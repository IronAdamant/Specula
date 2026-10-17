//! Live Lex (`lex_live`): rule sets the engine steps on its own fixed tick, on the main thread,
//! presented as world entities (docs/design/LEX.md, "Live Lex").
//!
//! **Stepping.** A run is a `specula_lex::Sim` of a stored set (`/lex/<set>`), compiled on the
//! calling agent's thread and handed to the main thread. Every engine tick, after the hosted
//! game and the modules, each run steps as many Lex ticks as its own `tick_hz` has accumulated
//! (a 30 Hz set under a 60 Hz engine steps every other engine tick), so pausing, stepping and
//! fast-forwarding the engine clock (`time_control`) drive Lex time too.
//!
//! **Natives.** Hooks the hosted game implements in-process ([`crate::game::Game::lex_natives`])
//! run as in `lex_simulate`. Any other declared hook is a module hook: the run calls
//! [`LogicHost::call_hook`] with the hook's reason and `caller` `lex:<set>`, and the commit
//! journals its world changes under `lex:<set>` as part of Lex's tick. Module hooks can't change
//! the run's state (only the world), so with no world-writing hooks, a live run and
//! `lex_simulate` with the same seed, params and commands (at the same ticks: `journal`) reach
//! the same hash.
//!
//! **Presentation.** Each live entity that matches a `present` rule (tags, first match wins) is
//! a world entity `/lex_live/<set>/e<id>` built from that rule's spawn fields and moved to the
//! entity's position each engine tick; entities that end are deleted. These updates are the
//! rules' own output, like a hosted game's: journaled replay-only under `lex:<set>` (coalesced
//! per tick) and exempt from claims. Starting and stopping a run are the caller's edits
//! (claim-checked, undoable).
//!
//! **Commands** (`action: command`) apply at once, at the run's current tick (exactly as
//! `lex_simulate` applies commands stamped with that tick), and are journaled with it.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::journal::{Kind, Writer};
use crate::logic::LogicHost;
use crate::schedule::Class;
use crate::swarm::Swarm;
use crate::tools::{check_keys, Command, Tools};
use crate::world::{EntityKey, Mark, Recording, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_lex::rules::{NativeDef, Rules, TagFilter};
use specula_lex::sim::NativeHost;
use specula_lex::{Options, Sim};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// The top-level group live runs present under.
pub const ROOT: &str = "lex_live";
/// Live runs at once.
const MAX_RUNS: usize = 4;
/// World entities one run presents (the rest are counted, not shown).
const MAX_SHOWN: usize = 2_000;
/// Commands one run journals (past it, commands are refused: a run must stay replayable).
const MAX_JOURNAL: usize = 100_000;
/// `present` rules per run.
const MAX_PRESENT: usize = 32;
/// Lex ticks one engine tick may step (a fast set under a slow engine catches up gradually).
const MAX_STEPS_PER_TICK: u32 = 64;
/// Refusals and hook failures kept for `status`.
const RECENT: usize = 16;
/// `journal` pages (default, most).
const JOURNAL_PAGE: (usize, usize) = (500, 5_000);

/// What the main thread does for `lex_live`.
pub enum LiveCall {
    Start(Box<Start>),
    Stop { set: String },
    Command { set: String, name: String, args: Json },
    Status { set: Option<String> },
    Journal { set: String, offset: usize, limit: usize },
}

impl LiveCall {
    pub fn class(&self) -> Class {
        match self {
            LiveCall::Status { .. } | LiveCall::Journal { .. } => Class::Read,
            _ => Class::Edit,
        }
    }
}

/// A run to start, compiled on the caller's thread.
pub struct Start {
    pub set: String,
    pub rules: Arc<Rules>,
    pub seed: u64,
    pub params: Json,
    pub present: Vec<Present>,
}

/// Entities with every tag in `tags` (and none of the `!tag`s) are shown as `spawn`.
pub struct Present {
    pub tags: Vec<String>,
    /// world_spawn fields (checked when the run starts; parsed for each entity shown).
    pub spawn: Json,
}

/// One presented entity.
struct Shown {
    id: u64,
    rule: usize,
    pos: [i64; 3],
}

struct Run {
    sim: Sim,
    seed: u64,
    params: Json,
    /// The engine tick the run started at (its Lex tick 0).
    started: u64,
    present: Vec<(TagFilter, Json)>,
    group: u64,
    shown: BTreeMap<u32, Shown>,
    /// Live entities not shown: past [`MAX_SHOWN`], or matching no rule.
    hidden: usize,
    /// A presentation write failed (the group was deleted?): presenting stopped.
    present_error: Option<String>,
    /// Accepted commands: (Lex tick, command, args, who).
    journal: Vec<(u32, String, Json, String)>,
    recent: VecDeque<Json>,
    hook_calls: u64,
    hook_failures: u64,
}

/// The live runs, by set.
#[derive(Default)]
pub struct Live {
    runs: BTreeMap<String, Run>,
}

impl Live {
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

fn push_recent(q: &mut VecDeque<Json>, j: Json) {
    if q.len() == RECENT {
        q.pop_front();
    }
    q.push_back(j);
}

fn error(code: &'static str, message: impl Into<String>) -> ToolOutput {
    crate::lex_tools::err(code, message)
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn hex(h: u64) -> Json {
    Json::from(format!("{h:016x}"))
}

/// A tag list as a filter over the rules' tags (an unknown tag never matches).
fn filter_of(rules: &Rules, tags: &[String]) -> TagFilter {
    let mut f = TagFilter::default();
    for t in tags {
        match t.strip_prefix('!') {
            Some(n) => {
                if let Some(id) = rules.tag(n) {
                    f.none.push(id);
                }
            }
            None => f.all.push(rules.tag(t).unwrap_or(u16::MAX)),
        }
    }
    f
}

fn matches_arch(rules: &Rules, arch: u16, f: &TagFilter) -> bool {
    let tags = &rules.archetypes[usize::from(arch)].tags;
    f.all.iter().all(|t| tags.binary_search(t).is_ok()) && !f.none.iter().any(|t| tags.binary_search(t).is_ok())
}

fn metres(p: [i64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32 / 1000.0, p[1] as f32 / 1000.0, p[2] as f32 / 1000.0)
}

/// Module hooks for a live run: `call_hook`, journaled as the run's tick.
struct Host<'a> {
    logic: &'a mut LogicHost,
    world: &'a mut World,
    swarm: &'a mut Swarm,
    tick: u64,
    hz: u32,
    agent: &'a str,
    calls: &'a mut u64,
    failures: &'a mut u64,
    recent: &'a mut VecDeque<Json>,
}

impl NativeHost for Host<'_> {
    fn call(&mut self, hook: &NativeDef, entity: Option<u32>, args: &[(String, i64)], lex_tick: u32) -> Result<(), String> {
        *self.calls += 1;
        let mut a: Vec<(String, Json)> = args.iter().map(|(k, v)| (k.clone(), Json::from(*v))).collect();
        a.push(("entity".into(), entity.map_or(Json::Null, Json::from)));
        a.push(("lex_tick".into(), Json::from(lex_tick)));
        let reason: String = hook.reason.chars().take(300).collect();
        let reason = if reason.trim().is_empty() { "a Lex native".to_owned() } else { reason };
        let (swarm, agent, tick) = (&mut *self.swarm, self.agent, self.tick);
        let commit = &mut |w: &mut World, _: Mark, _: &str, _| {
            swarm.game_changes(w, tick, agent, Kind::GameTick);
            Ok(())
        };
        match self.logic.call_hook(&hook.hook, &Json::Obj(a), &reason, agent, self.world, self.tick, self.hz, commit) {
            Ok(_) => Ok(()),
            Err(e) => {
                *self.failures += 1;
                push_recent(self.recent, Json::obj([("tick", Json::from(lex_tick)), ("hook", Json::from(hook.hook.as_str())), ("error", e.to_json())]));
                Err(format!("{}: {}", e.code, e.message))
            }
        }
    }
}

/// What a call needs from the engine (so the runner is testable without one).
pub(crate) struct Env<'a> {
    pub world: &'a mut World,
    pub logic: &'a mut LogicHost,
    pub swarm: &'a mut Swarm,
    pub natives: &'a specula_lex::Natives,
    /// The engine tick now, and the engine's rate.
    pub tick: u64,
    pub hz: u32,
}

impl crate::Engine {
    pub(crate) fn lex_live_call(&mut self, call: LiveCall, by: &str, w: Writer) -> ToolOutput {
        let (tick, hz) = (self.clock.tick(), self.clock.hz());
        let crate::Engine { lex_live, world, logic, swarm, lex_natives, .. } = self;
        lex_live.call(call, by, w, Env { world, logic, swarm, natives: lex_natives, tick, hz })
    }

    /// One engine tick of every live run (after the game and the modules).
    pub(crate) fn lex_live_tick(&mut self) {
        let (tick, hz) = (self.clock.tick(), self.clock.hz());
        let crate::Engine { lex_live, world, logic, swarm, lex_natives, .. } = self;
        lex_live.tick(Env { world, logic, swarm, natives: lex_natives, tick, hz });
    }
}

impl Live {
    pub(crate) fn call(&mut self, call: LiveCall, by: &str, w: Writer, env: Env) -> ToolOutput {
        match call {
            LiveCall::Start(s) => self.start(*s, w, env),
            LiveCall::Stop { set } => {
                let Some(run) = self.runs.remove(&set) else { return no_run(self, &set) };
                let mut j = status_json(&set, &run, env.natives);
                j.set("stopped", Json::Bool(true));
                if env.world.get(run.group).is_some() {
                    let group = run.group;
                    if let Err(e) = env.swarm.commit(env.world, env.tick, w, Kind::Delete, |wd| wd.delete(&EntityKey::Id(group), None).map(|_| ())) {
                        // The run is stopped either way; its entities stay for whoever holds them.
                        j.set("cleanup_refused", e.to_json());
                    }
                }
                ToolOutput::text(j.to_pretty())
            }
            LiveCall::Command { set, name, args } => {
                let agent = format!("lex:{set}");
                let Some(run) = self.runs.get_mut(&set) else { return no_run(self, &set) };
                if run.journal.len() >= MAX_JOURNAL {
                    return error("journal_full", format!("this run has journaled {MAX_JOURNAL} commands; stop it and start another to keep runs replayable"));
                }
                let call = match run.sim.parse_command(&name, &args) {
                    Ok(c) => c,
                    Err(r) => return ToolOutput::error(r.to_json().to_pretty()),
                };
                let at = run.sim.tick();
                let Env { world, logic, swarm, tick, hz, .. } = env;
                let mut host = Host { logic, world, swarm, tick, hz, agent: &agent, calls: &mut run.hook_calls, failures: &mut run.hook_failures, recent: &mut run.recent };
                match run.sim.apply_hosted(&call, &mut host) {
                    Ok(()) => {
                        run.journal.push((at, name.clone(), args.clone(), by.to_owned()));
                        present(world, swarm, tick, &agent, run);
                        let j = Json::obj([
                            ("accepted", Json::Bool(true)),
                            ("set", Json::from(set.as_str())),
                            ("command", Json::from(name.as_str())),
                            ("tick", Json::from(at)),
                            ("hash", hex(run.sim.hash())),
                        ]);
                        ToolOutput::text(j.to_pretty())
                    }
                    Err(r) => {
                        let mut j = r.to_json();
                        j.set("tick", Json::from(at));
                        j.set("command", Json::from(name.as_str()));
                        push_recent(&mut run.recent, j.clone());
                        ToolOutput::error(j.to_pretty())
                    }
                }
            }
            LiveCall::Status { set } => {
                let list: Vec<Json> = match &set {
                    Some(s) => match self.runs.get(s) {
                        Some(run) => vec![status_json(s, run, env.natives)],
                        None => return no_run(self, s),
                    },
                    None => self.runs.iter().map(|(s, r)| status_json(s, r, env.natives)).collect(),
                };
                let hooks: Vec<Json> = env.logic.hooks().iter().map(|h| Json::from(h.qualified())).collect();
                ToolOutput::text(Json::obj([("runs", Json::Arr(list)), ("module_hooks", Json::Arr(hooks))]).to_pretty())
            }
            LiveCall::Journal { set, offset, limit } => {
                let Some(run) = self.runs.get(&set) else { return no_run(self, &set) };
                let page: Vec<Json> = run
                    .journal
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|(t, n, a, by)| Json::obj([("tick", Json::from(*t)), ("command", Json::from(n.as_str())), ("args", a.clone()), ("by", Json::from(by.as_str()))]))
                    .collect();
                let next = (offset + page.len() < run.journal.len()).then_some(offset + page.len());
                let j = Json::obj([
                    ("set", Json::from(set.as_str())),
                    ("seed", Json::from(run.seed.to_string())),
                    ("params", run.params.clone()),
                    ("tick", Json::from(run.sim.tick())),
                    ("hash", hex(run.sim.hash())),
                    ("rules_hash", hex(run.sim.rules().hash)),
                    ("outcome", run.sim.outcome().map_or(Json::Null, |(r, t)| Json::obj([("result", Json::from(r)), ("tick", Json::from(t))]))),
                    ("commands", Json::Arr(page)),
                    ("total", Json::from(run.journal.len())),
                    ("next_offset", next.map_or(Json::Null, Json::from)),
                    (
                        "replay",
                        Json::from("lex_simulate {set, seed, params, ticks: tick, commands} with these commands (tick, command, args) reaches this hash when no module hook writes the world"),
                    ),
                ]);
                ToolOutput::text(j.to_pretty())
            }
        }
    }

    fn start(&mut self, s: Start, w: Writer, env: Env) -> ToolOutput {
        if self.runs.contains_key(&s.set) {
            return error("already_running", format!("set '{}' is already live; lex_live {{action: stop}} it first", s.set));
        }
        if self.runs.len() >= MAX_RUNS {
            return error("runs_full", format!("{MAX_RUNS} sets are live; stop one first"));
        }
        let sim = match Sim::new_hosted(Arc::clone(&s.rules), s.seed, env.natives) {
            Ok(sim) => sim,
            Err(m) => return error("native_failed", m),
        };
        let set = s.set.clone();
        // The group: /lex_live/<set>, replacing a leftover one (a saved world, say).
        let made = env.swarm.commit(env.world, env.tick, w, Kind::Batch, |wd| {
            let root = match wd.find_path(&format!("/{ROOT}")) {
                Ok(id) => id,
                Err(_) => {
                    let mut g = SpawnSpec::new(Shape::Group);
                    g.name = Some(ROOT.into());
                    wd.spawn(g, w.name)?.id
                }
            };
            if let Ok(old) = wd.find_path(&format!("/{ROOT}/{set}")) {
                wd.delete(&EntityKey::Id(old), None)?;
            }
            let mut g = SpawnSpec::new(Shape::Group);
            g.name = Some(set.clone());
            g.parent = Some(EntityKey::Id(root));
            g.props = Json::obj([("lex_live", Json::obj([("set", Json::from(set.as_str())), ("seed", Json::from(s.seed.to_string()))]))]);
            Ok(wd.spawn(g, w.name)?.id)
        });
        let group = match made {
            Ok((id, _)) => id,
            Err(e) => return world_error(&e),
        };
        let rules_present = s.present.into_iter().map(|p| (filter_of(&s.rules, &p.tags), p.spawn)).collect();
        let mut run = Run {
            sim,
            seed: s.seed,
            params: s.params,
            started: env.tick,
            present: rules_present,
            group,
            shown: BTreeMap::new(),
            hidden: 0,
            present_error: None,
            journal: Vec::new(),
            recent: VecDeque::new(),
            hook_calls: 0,
            hook_failures: 0,
        };
        present(env.world, env.swarm, env.tick, &format!("lex:{}", s.set), &mut run);
        let mut j = status_json(&s.set, &run, env.natives);
        j.set("started", Json::Bool(true));
        self.runs.insert(s.set, run);
        ToolOutput::text(j.to_pretty())
    }

    /// One engine tick: each run steps the Lex ticks its rate has accumulated, then shows them.
    pub(crate) fn tick(&mut self, env: Env) {
        let Env { world, logic, swarm, tick, hz, .. } = env;
        let hz = hz.max(1);
        for (set, run) in self.runs.iter_mut() {
            let agent = format!("lex:{set}");
            let lex_hz = u64::from(run.sim.rules().tick_hz);
            let done = tick.saturating_sub(run.started) + 1;
            let target = (u128::from(done) * u128::from(lex_hz) / u128::from(hz)).min(u128::from(u32::MAX)) as u32;
            let mut steps = 0;
            while run.sim.tick() < target && run.sim.outcome().is_none() && steps < MAX_STEPS_PER_TICK {
                let mut host = Host { logic, world, swarm, tick, hz, agent: &agent, calls: &mut run.hook_calls, failures: &mut run.hook_failures, recent: &mut run.recent };
                run.sim.step_hosted(&[], &mut host);
                steps += 1;
            }
            present(world, swarm, tick, &agent, run);
        }
    }
}

fn no_run(live: &Live, set: &str) -> ToolOutput {
    let names: Vec<&str> = live.runs.keys().map(String::as_str).collect();
    crate::lex_tools::err_with("not_running", format!("set '{set}' is not live"), vec![("live", Json::arr(names.iter().map(|n| Json::from(*n)))), ("suggestion", Json::from("lex_live {action: start, set}"))])
}

/// Shows the run's entities as world entities (journaled replay-only under `lex:<set>`).
fn present(world: &mut World, swarm: &mut Swarm, tick: u64, agent: &str, run: &mut Run) {
    if run.present_error.is_some() {
        return;
    }
    let prev = world.recording();
    world.set_recording(Recording::Snapshot);
    let r = run.sim.rules().clone();
    let mut stale: Vec<u32> = Vec::new();
    let mut fail: Option<String> = None;
    let mut hidden = 0;
    {
        let mut shown = run.shown.iter().peekable();
        for e in run.sim.entities().iter().filter(|e| e.alive) {
            // Presented entities that are gone come before this id.
            while let Some((&sid, _)) = shown.next_if(|(sid, _)| **sid < e.id) {
                stale.push(sid);
            }
            let rule = run.present.iter().position(|(f, _)| matches_arch(&r, e.arch, f));
            let have = shown.next_if(|(sid, _)| **sid == e.id).map(|(_, s)| (s.id, s.rule, s.pos));
            match (rule, have) {
                (None, Some(_)) => {
                    stale.push(e.id);
                    hidden += 1;
                }
                (None, None) => hidden += 1,
                (Some(ri), Some((wid, sri, pos))) if ri == sri => {
                    if pos != e.pos {
                        let change = UpdateSpec { position: Some(metres(e.pos)), ..Default::default() };
                        if let Err(err) = world.update(&EntityKey::Id(wid), change, agent) {
                            fail = Some(format!("moving {}: {}", e.id, err.message));
                            break;
                        }
                    }
                }
                (Some(_), Some(_)) => stale.push(e.id),
                (Some(_), None) => {}
            }
        }
        stale.extend(shown.map(|(sid, _)| *sid));
    }
    for sid in &stale {
        if let Some(s) = run.shown.remove(sid) {
            let _ = world.delete(&EntityKey::Id(s.id), None);
        }
    }
    // New (or re-ruled) entities, within the cap.
    if fail.is_none() {
        for e in run.sim.entities().iter().filter(|e| e.alive) {
            if run.shown.contains_key(&e.id) {
                continue;
            }
            let Some(ri) = run.present.iter().position(|(f, _)| matches_arch(&r, e.arch, f)) else { continue };
            if run.shown.len() >= MAX_SHOWN {
                hidden += 1;
                continue;
            }
            let Ok(mut spec) = crate::tools::parse_spawn_fields(&run.present[ri].1) else {
                fail = Some(format!("present rule {ri} no longer parses"));
                break;
            };
            spec.name = Some(format!("e{}", e.id));
            spec.parent = Some(EntityKey::Id(run.group));
            spec.position = metres(e.pos);
            let mut props = Json::obj([("lex_live", Json::obj([("id", Json::from(e.id)), ("archetype", Json::from(r.archetypes[usize::from(e.arch)].id.as_str()))]))]);
            if let Json::Obj(extra) = &spec.props {
                for (k, v) in extra {
                    props.set(k, v.clone());
                }
            }
            spec.props = props;
            match world.spawn(spec, agent) {
                Ok(ent) => {
                    run.shown.insert(e.id, Shown { id: ent.id, rule: ri, pos: e.pos });
                }
                Err(err) => {
                    fail = Some(format!("showing {}: {}", e.id, err.message));
                    break;
                }
            }
        }
    }
    for (lid, s) in run.shown.iter_mut() {
        if let Some(e) = run.sim.entity(*lid) {
            s.pos = e.pos;
        }
    }
    run.hidden = hidden;
    run.present_error = fail;
    world.set_recording(prev);
    swarm.game_changes(world, tick, agent, Kind::GameTick);
}

fn status_json(set: &str, run: &Run, in_process: &specula_lex::Natives) -> Json {
    let s = run.sim.state();
    let r = run.sim.rules();
    let res = Json::Obj(r.resources.iter().zip(&s.res).take(32).map(|(d, v)| (d.id.clone(), Json::from(*v))).collect());
    let natives = Json::arr(r.natives.iter().take(32).map(|n| {
        Json::obj([
            ("hook", Json::from(n.hook.as_str())),
            ("runs", Json::from(if in_process.contains_key(&n.hook) { "in_process" } else { "module" })),
        ])
    }));
    let diag = Json::obj([
        ("counts", Json::Obj(s.diag.counts.iter().map(|(k, v)| ((*k).to_owned(), Json::from(*v))).collect())),
        ("first", Json::arr(s.diag.first.iter().take(5).map(|m| Json::from(m.as_str())))),
    ]);
    let mut j = Json::obj([
        ("set", Json::from(set)),
        ("tick", Json::from(run.sim.tick())),
        ("tick_hz", Json::from(r.tick_hz)),
        ("seed", Json::from(run.seed.to_string())),
        ("outcome", run.sim.outcome().map_or(Json::Null, |(res, t)| Json::obj([("result", Json::from(res)), ("tick", Json::from(t))]))),
        ("wave", Json::from(u32::from(s.wave))),
        ("resources", res),
        ("entities", Json::from(run.sim.entities().iter().filter(|e| e.alive).count())),
        ("shown", Json::from(run.shown.len())),
        ("hidden", Json::from(run.hidden)),
        ("commands", Json::from(run.journal.len())),
        ("hash", hex(run.sim.hash())),
        ("rules_hash", hex(r.hash)),
        ("natives", natives),
        ("module_hook_calls", Json::from(run.hook_calls)),
        ("module_hook_failures", Json::from(run.hook_failures)),
        ("diagnostics", diag),
        ("recent", Json::Arr(run.recent.iter().cloned().collect())),
        ("world", Json::from(format!("/{ROOT}/{set}"))),
    ]);
    if let Some(e) = &run.present_error {
        j.set("present_error", Json::from(e.as_str()));
    }
    j
}

// ---- protocol threads ----

/// Answers `lex_live` on the calling agent's thread (compiling there for `start`).
pub fn call(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    match parse(tools, session, args) {
        Ok(Ok(c)) => tools.send(session, Command::LexLive(c)),
        Ok(Err(out)) => out,
        Err(m) => ToolOutput::error(crate::tools::argument_error(&m).to_pretty()),
    }
}

fn parse(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<Result<LiveCall, ToolOutput>, String> {
    let action = crate::tools::string_arg(args, "action")?.unwrap_or("status");
    let set = || crate::lex_tools::set_arg(args, true).map(|s| s.expect("required"));
    Ok(Ok(match action {
        "start" => {
            check_keys(args, "lex_live start", &[&["action", "set", "seed", "params", "present"]])?;
            let set = set()?;
            let params = crate::lex_tools::params_arg(args)?;
            let seed = crate::lex_tools::seed_arg(args)?;
            let present = present_arg(args)?;
            let l = match crate::lex_tools::fetch(tools, session, &set) {
                Ok(l) => l,
                Err(e) => return Ok(Err(e)),
            };
            if !l.problems.is_empty() {
                return Ok(Err(crate::lex_tools::err_with("invalid_set", format!("set '{set}' has a broken header or item ids; fix them with lex_edit first"), vec![("diagnostics", crate::lex_tools::diag_json(&l.problems, 50))])));
            }
            let opts = Options { overrides: params.clone(), ..Default::default() };
            let rules = match specula_lex::compile(&l.doc, &opts) {
                Ok(c) => Arc::new(c.rules),
                Err(ds) => return Ok(Err(crate::lex_tools::err_with("invalid", "the rules don't compile; fix these first (each says what to change)", vec![("diagnostics", crate::lex_tools::diag_json(&ds, 50))]))),
            };
            LiveCall::Start(Box::new(Start { set, rules, seed, params: Json::Obj(params), present }))
        }
        "stop" => {
            check_keys(args, "lex_live stop", &[&["action", "set"]])?;
            LiveCall::Stop { set: set()? }
        }
        "command" => {
            check_keys(args, "lex_live command", &[&["action", "set", "command", "args"]])?;
            let name = crate::tools::string_arg(args, "command")?.ok_or("'command' is required: a command id of the set")?.to_owned();
            LiveCall::Command { set: set()?, name, args: args.get("args").cloned().unwrap_or(Json::Null) }
        }
        "status" => {
            check_keys(args, "lex_live status", &[&["action", "set"]])?;
            LiveCall::Status { set: crate::lex_tools::set_arg(args, false)? }
        }
        "journal" => {
            check_keys(args, "lex_live journal", &[&["action", "set", "offset", "limit"]])?;
            let offset = crate::tools::int_arg(args, "offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
            let limit = crate::tools::int_arg(args, "limit", 1, JOURNAL_PAGE.1 as u64)?.map_or(JOURNAL_PAGE.0, |l| l as usize);
            LiveCall::Journal { set: set()?, offset, limit }
        }
        other => return Err(format!("'action' {other:?} is not one of start, stop, command, status, journal")),
    }))
}

/// `present`: [{tags, spawn}], at most [`MAX_PRESENT`]. Default: every entity as a small cube.
fn present_arg(args: &Json) -> Result<Vec<Present>, String> {
    let Some(list) = args.get("present").filter(|p| !p.is_null()) else {
        let cube = Json::obj([("shape", Json::from("cube")), ("scale", Json::arr([0.5, 0.5, 0.5].map(Json::from)))]);
        return Ok(vec![Present { tags: Vec::new(), spawn: cube }]);
    };
    let Some(list) = list.as_array().filter(|l| l.len() <= MAX_PRESENT) else {
        return Err(format!("'present' must be an array of at most {MAX_PRESENT} {{tags, spawn}} rules"));
    };
    let mut out = Vec::new();
    for (i, p) in list.iter().enumerate() {
        check_keys(p, &format!("present[{i}]"), &[&["tags", "spawn"]])?;
        let tags: Vec<String> = match p.get("tags") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) if a.len() <= 16 => a.iter().map(|t| t.as_str().map(str::to_owned).ok_or_else(|| format!("present[{i}].tags are tag names"))).collect::<Result<_, _>>()?,
            Some(_) => return Err(format!("present[{i}].tags must be an array of at most 16 tag names (\"!tag\" excludes)")),
        };
        let mut spec_json = p.get("spawn").cloned().unwrap_or_else(|| Json::obj([("shape", Json::from("cube"))]));
        for k in ["name", "parent", "position"] {
            if spec_json.get(k).is_some() {
                return Err(format!("present[{i}].spawn.{k} is set by the run (each entity's name, group and position)"));
            }
        }
        if spec_json.get("shape").is_none() {
            spec_json.set("shape", Json::from("cube"));
        }
        crate::tools::parse_spawn_fields(&spec_json).map_err(|m| format!("present[{i}].spawn: {m}"))?;
        out.push(Present { tags, spawn: spec_json });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANN: Writer = Writer { name: "ann", session: Some("aaaaaa11") };

    /// Walkers spawn every second and walk 10 m; a command builds a tower; a native per walker
    /// is left to the host (a module hook nobody runs here).
    const RULES: &str = r#"{"lex": 1, "name": "t", "tick_hz": 30, "items": {
        "gold": {"kind": "resource", "start": 5},
        "spots": {"kind": "sites", "points": {"a": [0, 0, 0]}},
        "lane": {"kind": "path", "points": [[0, 0, 0], [10, 0, 0]]},
        "walker": {"kind": "archetype", "tags": ["enemy"], "attributes": {"hp": "5pt"},
                   "behaviours": [{"type": "health", "attribute": "hp"},
                                  {"type": "follow_path", "path": "@spawn", "speed": "2m/s", "on_arrive": [{"despawn": "self"}]}]},
        "tower": {"kind": "archetype", "tags": ["stone"]},
        "build": {"kind": "command", "args": {"at": "site"}, "cost": {"gold": 5}, "do": [{"spawn": "tower", "at": "arg.at"}]},
        "glow": {"kind": "native", "hook": "fx/glow", "reason": "a world effect", "phase": "any"},
        "ping": {"kind": "command", "do": [{"native": "glow"}]},
        "waves": {"kind": "waves", "waves": [{"groups": [{"archetype": "walker", "count": 4, "at": ["lane"], "every": "1s"}]}]}
    }}"#;

    fn rules() -> Arc<Rules> {
        let doc = specula_lex::Document::parse(RULES).unwrap();
        Arc::new(specula_lex::compile(&doc, &Options::default()).unwrap().rules)
    }

    struct Rig {
        live: Live,
        world: World,
        logic: LogicHost,
        swarm: Swarm,
        natives: specula_lex::Natives,
        tick: u64,
        hz: u32,
    }

    impl Rig {
        fn new(hz: u32) -> Rig {
            let mut world = World::default();
            world.set_recording(Recording::Full);
            Rig { live: Live::default(), world, logic: LogicHost::new(), swarm: Swarm::default(), natives: Default::default(), tick: 0, hz }
        }

        fn call(&mut self, call: LiveCall) -> ToolOutput {
            let env = Env { world: &mut self.world, logic: &mut self.logic, swarm: &mut self.swarm, natives: &self.natives, tick: self.tick, hz: self.hz };
            self.live.call(call, "ann", ANN, env)
        }

        fn ticks(&mut self, n: u64) {
            for _ in 0..n {
                let env = Env { world: &mut self.world, logic: &mut self.logic, swarm: &mut self.swarm, natives: &self.natives, tick: self.tick, hz: self.hz };
                self.live.tick(env);
                self.tick += 1;
            }
        }

        fn shown(&self) -> Vec<String> {
            let Ok(g) = self.world.find_path("/lex_live/t") else { return Vec::new() };
            self.world.children_of(Some(g)).map(|id| self.world.path_of(id)).collect()
        }
    }

    fn json(out: &ToolOutput) -> Json {
        crate::lex_tools::output_json(out)
    }

    fn start(present: Vec<Present>) -> LiveCall {
        LiveCall::Start(Box::new(Start { set: "t".into(), rules: rules(), seed: 3, params: Json::Obj(Vec::new()), present }))
    }

    fn walkers_as_spheres() -> Vec<Present> {
        vec![Present { tags: vec!["enemy".into()], spawn: Json::obj([("shape", Json::from("sphere"))]) }]
    }

    #[test]
    fn a_live_run_steps_on_its_own_rate_and_shows_its_entities() {
        let mut rig = Rig::new(60);
        let out = rig.call(start(walkers_as_spheres()));
        assert!(!out.is_error, "{}", json(&out));
        assert!(rig.world.find_path("/lex_live/t").is_ok(), "the group exists");
        // A 30 Hz set under a 60 Hz engine: one Lex tick per two engine ticks.
        rig.ticks(60);
        let st = json(&rig.call(LiveCall::Status { set: Some("t".into()) }));
        let run = &st.get("runs").and_then(Json::as_array).unwrap()[0];
        assert_eq!(run.get("tick").and_then(Json::as_u64), Some(30));
        assert_eq!(rig.shown(), vec!["/lex_live/t/e1".to_owned()], "the first walker, shown as a sphere; the tower rule matches nothing yet");
        let e1 = rig.world.find_path("/lex_live/t/e1").unwrap();
        let x0 = rig.world.get(e1).unwrap().position.x;
        rig.ticks(20);
        assert!(rig.world.get(e1).unwrap().position.x > x0, "it moves with its Lex position");
        // Entities that end are deleted (each walks 10 m in 5 s).
        rig.ticks(60 * 10);
        assert!(rig.shown().is_empty(), "{:?}", rig.shown());
    }

    #[test]
    fn commands_apply_at_the_runs_tick_and_the_journal_replays_headless() {
        let mut rig = Rig::new(30);
        rig.call(start(vec![Present { tags: vec![], spawn: Json::obj([("shape", Json::from("cube"))]) }]));
        rig.ticks(17);
        let ok = json(&rig.call(LiveCall::Command { set: "t".into(), name: "build".into(), args: Json::parse(r#"{"at": "a"}"#).unwrap() }));
        assert_eq!((ok.get("accepted"), ok.get("tick").and_then(Json::as_u64)), (Some(&Json::Bool(true)), Some(17)), "{ok}");
        let refused = rig.call(LiveCall::Command { set: "t".into(), name: "build".into(), args: Json::parse(r#"{"at": "a"}"#).unwrap() });
        assert!(refused.is_error && json(&refused).get("code") == Some(&Json::from("not_enough_gold")), "{}", json(&refused));
        assert!(rig.call(LiveCall::Command { set: "t".into(), name: "nope".into(), args: Json::Null }).is_error);
        rig.ticks(100);
        let j = json(&rig.call(LiveCall::Journal { set: "t".into(), offset: 0, limit: 10 }));
        let cmds = j.get("commands").and_then(Json::as_array).unwrap();
        assert_eq!(cmds.len(), 1, "only accepted commands are journaled: {j}");
        // Headless, with the same seed and the same command at the same tick: the same hash.
        // (new_hosted: the set declares a module hook, which this run never calls.)
        let mut sim = Sim::new_hosted(rules(), 3, &Default::default()).unwrap();
        let call = sim.parse_command("build", cmds[0].get("args").unwrap()).unwrap();
        let at = cmds[0].get("tick").and_then(Json::as_u64).unwrap() as u32;
        let tick = j.get("tick").and_then(Json::as_u64).unwrap() as u32;
        while sim.tick() < tick {
            let now = if sim.tick() == at { vec![call.clone()] } else { Vec::new() };
            sim.step(&now);
        }
        assert_eq!(j.get("hash"), Some(&hex(sim.hash())), "live and headless agree");
        // The tower is shown (the catch-all rule), and stopping removes everything.
        assert!(rig.shown().iter().any(|p| p.ends_with("/e2") || p.contains("/e")), "{:?}", rig.shown());
        let stop = json(&rig.call(LiveCall::Stop { set: "t".into() }));
        assert_eq!(stop.get("stopped"), Some(&Json::Bool(true)));
        assert!(rig.world.find_path("/lex_live/t").is_err(), "{stop}");
        assert!(rig.call(LiveCall::Status { set: Some("t".into()) }).is_error, "not live any more");
    }

    #[test]
    fn module_hooks_nobody_runs_fail_as_data_and_refuse_the_command() {
        let mut rig = Rig::new(30);
        rig.call(start(walkers_as_spheres()));
        let out = rig.call(LiveCall::Command { set: "t".into(), name: "ping".into(), args: Json::Null });
        assert!(out.is_error, "the hook isn't running, so the command is refused: {}", json(&out));
        let st = json(&rig.call(LiveCall::Status { set: Some("t".into()) }));
        let run = &st.get("runs").and_then(Json::as_array).unwrap()[0];
        assert_eq!(run.get("module_hook_failures").and_then(Json::as_u64), Some(1));
        assert_eq!(run.get("natives").and_then(Json::as_array).unwrap()[0].get("runs"), Some(&Json::from("module")));
        let recent = run.get("recent").and_then(Json::as_array).unwrap();
        assert!(recent.iter().any(|r| r.get("hook") == Some(&Json::from("fx/glow"))), "{recent:?}");
    }

    #[test]
    fn runs_are_bounded_and_unique() {
        let mut rig = Rig::new(30);
        assert!(!rig.call(start(Vec::new())).is_error);
        assert_eq!(json(&rig.call(start(Vec::new()))).get("error"), Some(&Json::from("already_running")));
        for k in 0..3 {
            let s = Start { set: format!("t{k}"), rules: rules(), seed: 1, params: Json::Obj(Vec::new()), present: Vec::new() };
            assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        }
        let s = Start { set: "t9".into(), rules: rules(), seed: 1, params: Json::Obj(Vec::new()), present: Vec::new() };
        assert_eq!(json(&rig.call(LiveCall::Start(Box::new(s)))).get("error"), Some(&Json::from("runs_full")));
    }
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    vec![ToolDef {
        name: "lex_live".into(),
        description: "Runs a stored Lex rule set live in the engine: the engine steps it on its own tick (its tick_hz; time_control pauses, steps and fast-forwards it with everything else) and shows its entities as world entities under /lex_live/<set> (models and positions from 'present' rules), journaled as the run's tick under lex:<set>. Natives the hosted game implements run in-process; any other native calls the module hook it names (module/name) with its reason. action: start {set, seed?, params?, present?: [{tags: [..], spawn: {shape, color, scale, model ...}}]}, command {set, command, args} (applied at once at the run's tick, journaled), status {set?}, journal {set, offset?, limit?} (the accepted commands with their ticks: lex_simulate with the same seed, params and commands reaches the same hash), stop {set}. At most 4 runs; 2000 shown entities each.".into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("action", t("string", "start, command, status (default), journal or stop.")),
                    ("set", t("string", "The rule set's name (it lives at /lex/<set>).")),
                    ("seed", t("integer", "start: the run's seed (default 1; a decimal string for values above 2^53).")),
                    ("params", t("object", "start: param overrides, param id -> value in the param's unit.")),
                    ("present", t("array", "start: [{tags: [\"enemy\"], spawn: {shape: \"sphere\", color: [0.8,0.2,0.2], scale: [0.6,0.6,0.6]}}]; the first rule whose tags an entity's archetype has (\"!tag\" excludes) shows it. Default: every entity as a 0.5 m cube.")),
                    ("command", t("string", "command: the command id.")),
                    ("args", t("object", "command: its args (sites and archetypes by name, entities by id or by a site name).")),
                    ("offset", t("integer", "journal: the first command to list (default 0).")),
                    ("limit", t("integer", "journal: commands per page, 1..5000 (default 500).")),
                ]),
            ),
            ("required", Json::arr([])),
        ]),
    }]
}
