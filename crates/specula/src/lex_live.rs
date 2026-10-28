//! Live Lex (`lex_live`): rule sets the engine steps on its own fixed tick, on the main thread,
//! presented as world entities (docs/design/LEX.md, "Live Lex").
//!
//! **Stepping.** A run is a `specula_lex::Sim` of a stored set (`/lex/<set>`), compiled on the
//! calling agent's thread and handed to the main thread. Every engine tick, after the hosted
//! game and the modules, each run steps as many Lex ticks as its own `tick_hz` has accumulated
//! (a 30 Hz set under a 60 Hz engine steps every other engine tick), so pausing, stepping and
//! fast-forwarding the engine clock (`time_control`) drive Lex time too.
//!
//! **Budgets (F1, F2, F3), in work, not time.** All live runs together may do
//! [`LIMITS`]`.work_per_tick` units of Lex work (`Sim::tick_work`, plus one per entity and per
//! tick) in one engine tick, taking turns going first: a run still short of its target tick when
//! that is spent falls behind (`behind` in status) and catches up later. A Lex tick that does more
//! than `step_work` (its phases were cut short) **halts** the run with `work_limit`. Both depend
//! only on the rules and the state, never on the machine, so presentation and hook writes (which
//! are in the world hash) land on the same engine ticks everywhere. Wall time is only a backstop:
//! a Lex tick past `backstop` (1 s), or an in-process native that panics, halts the run too, and
//! the halt says it was not deterministic. The engine goes on either way.
//!
//! **Natives.** Hooks the hosted game implements in-process ([`crate::game::Game::lex_natives`])
//! run as in `lex_simulate`. Any other declared hook is a module hook: the run calls
//! [`LogicHost::call_hook`] with the hook's reason and `caller` `lex:<set>`. In the run's own
//! ticks its world changes are journaled under `lex:<set>` as part of the tick; in an agent's
//! command they are staged, checked against claims as the agent's and journaled only if the whole
//! command succeeds (a refusal rolls back the world and the run together). Module hooks can't
//! change the run's state (only the world), so with no world-writing hooks, a live run and
//! `lex_simulate` with the same seed, params and commands (at the same ticks: `journal`) reach
//! the same hash.
//!
//! **Presentation.** Each live entity that matches a `present` rule (tags, first match wins) is
//! a world entity `/lex_live/<set>/e<id>` (props `lex_live: {id, archetype}`) built from that
//! rule's spawn fields and moved to the entity's position after each Lex step; entities that end
//! are deleted, and one an agent deleted is shown again. These updates are the rules' own
//! output, like a hosted game's: journaled replay-only under `lex:<set>` (coalesced per tick) and
//! exempt from claims; region claims don't cover `/lex_live` at all (the entities move with the
//! simulation), path claims do. Starting and stopping a run, and its commands, are the caller's:
//! a path claim on `/lex_live/<set>` by another agent refuses them.
//!
//! **Commands** (`action: command`) apply at once, at the run's current tick (exactly as
//! `lex_simulate` applies commands stamped with that tick), and are journaled with it.

mod save;
pub(crate) use save::{parse_state, LiveSnapshot};

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::journal::{Kind, Writer};
use crate::logic::LogicHost;
use crate::schedule::Class;
use crate::swarm::Swarm;
use crate::tools::{check_keys, Command, Tools};
use crate::world::{EntityKey, Mark, Recording, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use crate::write_policy::{self, Exempt, Opener, Rule};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_lex::rules::{NativeDef, Rules, TagFilter};
use specula_lex::{NativeArgs, NativeHost, Options, Sim};
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
/// What live runs may spend (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Lex work all runs together may do in one engine tick (deterministic).
    pub work_per_tick: u64,
    /// Lex work one Lex tick may do before its run halts with `work_limit` (deterministic).
    pub step_work: u64,
    /// Wall time one Lex tick may take before its run halts (a backstop: not deterministic).
    pub backstop: Duration,
}

/// The engine's limits, calibrated to a frame on the reference iGPU laptop (see LEX.md, "Live
/// Lex", for the measurement: 40-60 ns a unit in release): 50k units an engine tick is about
/// 3 ms, and one Lex tick may do 250k (about 10-15 ms, a frame) before the run halts (a step
/// that overspends leaves a debt that pauses the run for the engine ticks that pay it
/// back). perf.toml `[lex_live]` changes them.
pub const LIMITS: Limits = Limits { work_per_tick: 50_000, step_work: 250_000, backstop: Duration::from_secs(1) };
/// Refusals and hook failures kept for `status`.
const RECENT: usize = 16;
/// `journal` pages (default, most), and the most bytes of commands one page lists.
const JOURNAL_PAGE: (usize, usize) = (500, 5_000);
const JOURNAL_BYTES: usize = 256 * 1024;
/// The largest command args (compact JSON): the journal keeps them.
const MAX_ARGS_BYTES: usize = 4096;

/// What the main thread does for `lex_live`.
pub enum LiveCall {
    Start(Box<Start>),
    Stop { set: String },
    Command { set: String, name: String, args: Json },
    Status { set: Option<String> },
    Journal { set: String, offset: usize, limit: usize },
    /// An agent holds input actions (like `logic_input`), as if pressed at the window.
    Input { set: String, actions: Json, ticks: u64 },
    /// Carries on after a halt that left the run's state whole.
    Resume { set: String },
    /// Starts the run again from tick 0 with what it was started with.
    Restart { set: String },
}

/// `input`: the run's own actions (`specula.input.v1`: keyboard, mouse, gamepad, touch, F6) and
/// which command each fires, so a live run is playable without a hosted game.
pub struct InputSpec {
    /// The action map, as text.
    pub actions: String,
    pub bindings: Vec<InputBinding>,
}

/// When an action fires a command: `on` pressed (default), released or held (every engine
/// tick it is down). `args` are the command's, with `"$x"` / `"$y"` replaced by the action's
/// value in thousandths (an axis, a stick).
pub struct InputBinding {
    pub action: String,
    pub on: Edge,
    pub command: String,
    pub args: Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Pressed,
    Released,
    Held,
}

/// A run's input: the actions, one state per engine tick, and the bindings. The local player's
/// window and gamepad input and the agents' holds are kept apart per action, so neither is
/// ever taken for the other.
struct LiveInput {
    spec: Arc<InputSpec>,
    /// Window frames and gamepads (the local player).
    window: crate::logic::input::ModuleInput,
    /// Agents' holds.
    agents: crate::logic::input::ModuleInput,
    /// Who holds each action an agent holds (its commands run as that agent's).
    holders: BTreeMap<String, Holder>,
}

impl LiveInput {
    fn new(spec: &Arc<InputSpec>) -> Result<LiveInput, specula_input::InputError> {
        Ok(LiveInput {
            spec: Arc::clone(spec),
            window: crate::logic::input::ModuleInput::new(&spec.actions)?,
            agents: crate::logic::input::ModuleInput::new(&spec.actions)?,
            holders: BTreeMap::new(),
        })
    }
}

/// At most this many input bindings per run.
const MAX_BINDINGS: usize = 64;
/// Commands one run's input may fire in one engine tick (each is also charged to the work
/// allowance).
const MAX_INPUT_COMMANDS_PER_TICK: usize = 8;
/// The part of a run's journal its input may fill (agents' commands keep the rest).
const MAX_INPUT_JOURNAL: usize = MAX_JOURNAL / 2;

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
    pub input: Option<Arc<InputSpec>>,
    /// The set's document as compiled (a save game keeps it, so a load compiles the same rules).
    pub doc: Option<Arc<Json>>,
}

/// Entities with every tag in `tags` (and none of the `!tag`s) are shown as `spawn`.
#[derive(Clone)]
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
    /// `lex:<set>`: the name the run's world changes are journaled under.
    agent: String,
    seed: u64,
    params: Json,
    /// The engine tick the run started at (its Lex tick 0).
    started: u64,
    present: Vec<(TagFilter, Json)>,
    /// The `present` rules as given (for `restart`).
    present_src: Vec<Present>,
    input: Option<LiveInput>,
    group: u64,
    shown: BTreeMap<u32, Shown>,
    /// The Lex tick last presented (`None`: present on the next engine tick).
    presented: Option<u32>,
    /// Live entities not shown: past [`MAX_SHOWN`], or matching no rule.
    hidden: usize,
    /// The last presentation write that failed (presenting goes on).
    present_error: Option<String>,
    /// Lex ticks short of where the engine clock says the run should be.
    behind: u64,
    /// The work of the last Lex tick, and what the run spent of this engine tick's allowance.
    last_work: u64,
    spent: u64,
    /// What the run may still spend (its share of each engine tick's allowance; negative: the
    /// debt a heavy step left, paid back before it steps again).
    credit: i64,
    /// Commands input journaled (at most [`MAX_INPUT_JOURNAL`]), and input commands dropped
    /// past the per-tick cap or the allowance.
    input_journaled: usize,
    input_dropped: u64,
    /// The run was stopped by an overrun or a panic: why (it steps no more).
    halted: Option<Json>,
    /// Accepted commands: (Lex tick, command, args, who).
    journal: Vec<(u32, String, Json, String)>,
    recent: VecDeque<Json>,
    hook_calls: u64,
    hook_failures: u64,
    /// The set's document as compiled (for save games; `None`: started without one, tests).
    doc: Option<Arc<Json>>,
    /// Journal entries a save game left out (it keeps the newest that fit): `journal` starts
    /// after them.
    journal_dropped: usize,
    /// Module hook args, reused call to call.
    args: Json,
}

/// The live runs, by set.
pub struct Live {
    runs: BTreeMap<String, Run>,
    /// Which run steps first next tick (so no run starves the others of the budget).
    next_first: usize,
    /// perf.toml `[lex_live]` (default [`LIMITS`]).
    pub(crate) limits: Limits,
    /// The open sessions (id, agent name) as last seen ([`Live::end_holds`]): `None` until the
    /// engine has told it (tests).
    live_agents: Option<Vec<(String, String)>>,
}

impl Default for Live {
    fn default() -> Live {
        Live { runs: BTreeMap::new(), next_first: 0, limits: LIMITS, live_agents: None }
    }
}

impl Limits {
    /// perf.toml `[lex_live]`: `work_per_tick`, `step_work`, `backstop_ms`. One setting for every
    /// adapter: the work limits decide which engine tick a step lands on (in the world hash),
    /// so a world must step the same on every machine.
    pub fn parse(v: &Json) -> Result<Limits, String> {
        let mut l = LIMITS;
        let max_step = specula_lex::sim::MAX_WORK_PER_TICK;
        for (k, x) in v.as_object().ok_or("[lex_live] must be a table")? {
            let n = x.as_u64();
            match k.as_str() {
                "work_per_tick" => {
                    l.work_per_tick = n.filter(|n| (1..=1_000_000_000).contains(n)).ok_or("[lex_live] work_per_tick must be a whole number of work units from 1 to 1000000000")?;
                }
                "step_work" => {
                    l.step_work = n.filter(|n| (1..=max_step).contains(n)).ok_or(format!("[lex_live] step_work must be a whole number of work units from 1 to {max_step} (Lex's own cap)"))?;
                }
                "backstop_ms" => {
                    l.backstop = Duration::from_millis(n.filter(|n| (10..=600_000).contains(n)).ok_or("[lex_live] backstop_ms must be a whole number of milliseconds from 10 to 600000")?);
                }
                other => return Err(format!("unknown key '{other}' in [lex_live] (known: work_per_tick, step_work, backstop_ms)")),
            }
        }
        Ok(l)
    }

    pub fn to_json(self) -> Json {
        Json::obj([
            ("work_per_tick", Json::from(self.work_per_tick)),
            ("step_work", Json::from(self.step_work)),
            ("backstop_ms", Json::from(self.backstop.as_millis() as u64)),
        ])
    }
}

impl Live {
    pub(crate) fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// Stops every run (`world_load` replaced the world they presented into), keeping the
    /// limits; returns the stopped sets, for the caller's answer.
    pub(crate) fn stop_all(&mut self) -> Vec<String> {
        self.next_first = 0;
        std::mem::take(&mut self.runs).into_keys().collect()
    }

    /// What the live runs are, for the desync detector and save games (`None`: no run): each
    /// set's Lex state and its anchor, credit, debt and counters (what decides when it steps).
    /// What it presents is in the world hash, not here (a presented entity someone deleted is
    /// still listed by the saving engine until the next presentation, but not by a loaded one).
    pub(crate) fn state_hash(&self) -> Option<u64> {
        if self.runs.is_empty() {
            return None;
        }
        let mut h = specula_core::hash::StableHasher::with_seed(0x6c65_785f_6c69_7665);
        h.write_u64(self.next_first as u64);
        for (set, r) in &self.runs {
            h.write_str(set);
            h.write_u64(r.sim.hash());
            h.write_u64(r.started);
            h.write_i64(r.credit);
            h.write_u64(r.behind);
            h.write_bool(r.halted.is_some());
            h.write_u64((r.journal_dropped + r.journal.len()) as u64);
            h.write_u64(r.input_journaled as u64);
            h.write_u64(r.input_dropped);
            h.write_u64(r.hidden as u64);
        }
        Some(h.finish())
    }

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

/// A Lex refusal in the engine's error shape: `error` (its code) beside its own fields.
fn refusal_json(r: &specula_lex::Rejection) -> Json {
    let mut j = Json::obj([("error", Json::from(r.code.as_str()))]);
    if let Json::Obj(fields) = r.to_json() {
        for (k, v) in fields {
            j.set(&k, v);
        }
    }
    j
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn hex(h: u64) -> Json {
    Json::from(format!("{h:016x}"))
}

/// A panic's message (natives are other code: a bug there stops the run, not the engine).
fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned())).unwrap_or_else(|| "a panic".into())
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

/// Module hooks for a live run: `call_hook`.
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
    args: &'a mut Json,
    /// In an agent's command: leave the hooks' world changes staged for the caller to check
    /// and journal (or roll back) with the whole command. In the run's own ticks: journal them
    /// now, as the tick's.
    staged: bool,
}

impl NativeHost for Host<'_> {
    fn call(&mut self, hook: &NativeDef, entity: Option<u32>, args: NativeArgs<'_>, lex_tick: u32) -> Result<(), String> {
        *self.calls += 1;
        // The args object is rebuilt only when its keys change (same hook, same keys: values only).
        if !matches!(&*self.args, Json::Obj(v) if v.len() == args.len() + 2 && v.iter().zip(args.iter()).all(|((k, _), (n, _))| k == n)) {
            let mut v: Vec<(String, Json)> = args.iter().map(|(k, _)| (k.to_owned(), Json::Null)).collect();
            v.push(("entity".into(), Json::Null));
            v.push(("lex_tick".into(), Json::Null));
            *self.args = Json::Obj(v);
        }
        if let Json::Obj(v) = &mut *self.args {
            let n = args.len();
            for (slot, (_, x)) in v.iter_mut().zip(args.iter()) {
                slot.1 = Json::from(x);
            }
            v[n].1 = entity.map_or(Json::Null, Json::from);
            v[n + 1].1 = Json::from(lex_tick);
        }
        let reason: String = hook.reason.chars().take(300).collect();
        let reason = if reason.trim().is_empty() { "a Lex native".to_owned() } else { reason };
        let (swarm, agent, tick, staged) = (&mut *self.swarm, self.agent, self.tick, self.staged);
        let commit = &mut |w: &mut World, _: Mark, _: &str, _| {
            if !staged {
                write_policy::journal_exempt(w, swarm, tick, Exempt::LexLive { agent });
            }
            Ok(())
        };
        match self.logic.call_hook(&hook.hook, self.args, &reason, agent, self.world, self.tick, self.hz, commit) {
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
        world.session(swarm, tick, Opener::LexLive, |world, swarm| lex_live.call(call, by, w, Env { world, logic, swarm, natives: lex_natives, tick, hz }))
    }

    /// Ends live-input holds whose agents are gone (each frame while any hold is on).
    pub(crate) fn lex_live_end_holds(&mut self, server: &specula_protocol::Server) {
        if !self.lex_live.has_holds() {
            return;
        }
        let live: Vec<(String, String)> = server.all_sessions().iter().map(|s| (s.id.clone(), self.swarm.agent_name(s))).collect();
        self.lex_live.end_holds(&live);
    }

    /// One engine tick of every live run (after the game and the modules).
    pub(crate) fn lex_live_tick(&mut self) {
        let (tick, hz) = (self.clock.tick(), self.clock.hz());
        let crate::Engine { lex_live, world, logic, swarm, lex_natives, .. } = self;
        let limits = lex_live.limits;
        world.session(swarm, tick, Opener::LexLive, |world, swarm| lex_live.tick(Env { world, logic, swarm, natives: lex_natives, tick, hz }, limits));
    }
}

/// Stops a run stepping. `deterministic`: whether the same rules, seed and commands halt it at
/// the same tick on any machine (false for the wall-time backstop). `resumable`: whether its
/// state is whole (a tick or command finished or was undone), so `resume` may carry on.
fn halt(run: &mut Run, code: &str, message: String, deterministic: bool, resumable: bool) {
    eprintln!("specula: live Lex run {} halted at tick {}: {message}", run.agent, run.sim.tick());
    run.halted = Some(Json::obj([
        ("error", Json::from(code)),
        ("message", Json::from(message)),
        ("tick", Json::from(run.sim.tick())),
        ("deterministic", Json::Bool(deterministic)),
        ("resumable", Json::Bool(resumable)),
    ]));
}

fn halted_error(run: &Run) -> ToolOutput {
    let mut j = run.halted.clone().unwrap_or(Json::Null);
    let resumable = run.halted.as_ref().and_then(|h| h.get("resumable")) == Some(&Json::Bool(true));
    j.set("hint", Json::from(if resumable { "the run is halted; lex_live {action: resume} carries on, {action: restart} starts it again from tick 0" } else { "the run is halted part-way through a tick; lex_live {action: restart} starts it again from tick 0 (same seed, params, present, input)" }));
    ToolOutput::error(j.to_pretty())
}

/// Engine ticks a run needs for `lex_ticks` of its own (rounded down).
fn engine_ticks_for(lex_ticks: u32, lex_hz: u32, hz: u32) -> u64 {
    u64::from(lex_ticks) * u64::from(hz.max(1)) / u64::from(lex_hz.max(1))
}

/// Applies one command to `run` at its current tick, as one unit with its module hooks' world
/// changes (staged under one mark), module state, RNG draws and events: all kept if it succeeds
/// (and, for an agent `w`, its claims allow the world changes), none otherwise. `w: None` is the
/// run's own input (window, gamepad, held actions): journaled like the run's ticks. Answers the
/// accepted command, or the error (a panicking native also halts the run).
#[allow(clippy::too_many_arguments)]
fn apply_command(run: &mut Run, name: &str, args: &Json, by: &str, w: Option<Writer>, input: bool, world: &mut World, logic: &mut LogicHost, swarm: &mut Swarm, tick: u64, hz: u32) -> Result<Json, Json> {
    if run.journal.len() >= MAX_JOURNAL {
        return Err(Json::obj([
            ("error", Json::from("journal_full")),
            ("message", Json::from(format!("this run has journaled {MAX_JOURNAL} commands; restart it or start another to keep runs replayable"))),
        ]));
    }
    // Input may use only part of the journal, so agents' own commands always have room.
    if input && run.input_journaled >= MAX_INPUT_JOURNAL {
        return Err(Json::obj([
            ("error", Json::from("input_journal_full")),
            ("message", Json::from(format!("this run's input has journaled {MAX_INPUT_JOURNAL} commands (input may use that much of the journal); agents' commands still apply; restart the run to play on with input"))),
        ]));
    }
    let call = run.sim.parse_command(name, args).map_err(|r| refusal_json(&r))?;
    let at = run.sim.tick();
    let mark = world.mark();
    let agent = run.agent.clone();
    logic.stage();
    // Lex undoes a refused command, or one whose native panics, itself; it hands back what
    // undoes a successful one only when the command reached a native (no copy otherwise).
    let result = {
        let mut host = Host { logic, world, swarm, tick, hz, agent: &agent, calls: &mut run.hook_calls, failures: &mut run.hook_failures, recent: &mut run.recent, args: &mut run.args, staged: true };
        let sim = &mut run.sim;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sim.apply_undoable(&call, &mut host)))
    };
    let undo = |world: &mut World, swarm: &mut Swarm, logic: &mut LogicHost| {
        write_policy::rolled_back(world, swarm, mark);
        logic.drop_staged();
    };
    let before = match result {
        Err(p) => {
            undo(world, swarm, logic);
            halt(run, "native_panicked", format!("a native panicked in command '{name}': {}", panic_text(&*p)), true, true);
            let mut j = run.halted.clone().unwrap_or(Json::Null);
            j.set("hint", Json::from("the command was undone and the run is halted; lex_live {action: resume} carries on"));
            return Err(j);
        }
        Ok(Err(r)) => {
            undo(world, swarm, logic);
            let mut j = refusal_json(&r);
            j.set("tick", Json::from(at));
            j.set("command", Json::from(name));
            j.set("by", Json::from(by));
            push_recent(&mut run.recent, j.clone());
            return Err(j);
        }
        Ok(Ok(u)) => u,
    };
    match w {
        Some(w) => {
            if let Err(e) = write_policy::game_commit(world, swarm, mark, tick, &agent, w) {
                // The world is back at the mark; the run and the modules go back with it.
                logic.drop_staged();
                if let Some(u) = before {
                    run.sim.undo(u);
                }
                let mut j = e.to_json();
                j.set("command", Json::from(name));
                j.set("note", Json::from("the command's module hooks changed the world where a claim refuses it; nothing was applied"));
                return Err(j);
            }
        }
        None => write_policy::journal_exempt(world, swarm, tick, Exempt::LexLive { agent: &agent }),
    }
    logic.keep_staged();
    if input {
        run.input_journaled += 1;
    }
    run.journal.push((at, name.to_owned(), args.clone(), by.to_owned()));
    run.presented = None;
    present(world, swarm, tick, run);
    Ok(Json::obj([("accepted", Json::Bool(true)), ("command", Json::from(name)), ("tick", Json::from(at)), ("hash", hex(run.sim.hash()))]))
}

/// The commands a run's input fires this engine tick (its actions' state for the tick taken).
/// A command a run's input fires, and the agent holding the action (`None`: the window or a
/// gamepad, the local player).
struct Fired {
    action: String,
    command: String,
    args: Json,
    who: Option<Holder>,
}

/// An agent holding an input action: its name and session.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Holder {
    name: String,
    session: Option<String>,
}

/// The commands a run's input fires this engine tick (its actions' state for the tick taken):
/// edges (`pressed`, `released`) once, and `held` bindings separately (the caller fires those
/// once per Lex tick stepped).
fn input_commands(input: &mut LiveInput) -> (Vec<Fired>, Vec<Fired>) {
    let window = input.window.take_tick();
    let agents = input.agents.take_tick();
    let (mut edges, mut held) = (Vec::new(), Vec::new());
    // The local player's bindings first, then the agents' (each by who holds the action).
    for (states, by_agent) in [(&window, false), (&agents, true)] {
        for b in &input.spec.bindings {
            let Some(s) = states.get(&b.action).and_then(|v| v.as_list()) else { continue };
            let flag = |i: usize| s.get(i).and_then(|v| v.as_bool()).unwrap_or(false);
            let fires = match b.on {
                Edge::Pressed => flag(1),
                Edge::Released => flag(2),
                Edge::Held => flag(0),
            };
            if fires {
                let value = |i: usize| s.get(i).and_then(|v| v.as_f32()).unwrap_or(0.0);
                let (x, y) = (value(3), value(4));
                let who = if by_agent { input.holders.get(&b.action).cloned() } else { None };
                if by_agent && who.is_none() {
                    continue;
                }
                let f = Fired { action: b.action.clone(), command: b.command.clone(), args: substitute(&b.args, x, y), who };
                if b.on == Edge::Held {
                    held.push(f);
                } else {
                    edges.push(f);
                }
            }
        }
    }
    // An agent's hold is over once its action is idle again (the release has fired).
    input.holders.retain(|action, _| agents.get(action).is_some());
    (edges, held)
}

/// Whether a holder is still here: its session open under the same agent name (`live`: each
/// open session's id and name). A hold made without a session (the engine's own) always is.
fn holder_live(h: &Holder, live: &[(String, String)]) -> bool {
    match &h.session {
        None => true,
        Some(sid) => live.iter().any(|(s, name)| s == sid && *name == h.name),
    }
}

/// Applies commands a run's input fired: an agent's hold runs as that agent (its claims on the
/// run and on what the command's hooks change both apply; journaled `by` it), the window and
/// gamepads as the run's own input. Past [`MAX_INPUT_COMMANDS_PER_TICK`] in this engine tick, or
/// past the work allowance, the rest are dropped (counted in status).
#[allow(clippy::too_many_arguments)]
fn fire(run: &mut Run, list: Vec<Fired>, fired_n: &mut usize, live: Option<&[(String, String)]>, world: &mut World, logic: &mut LogicHost, swarm: &mut Swarm, tick: u64, hz: u32) {
    for f in list {
        if run.halted.is_some() || run.sim.outcome().is_some() {
            return;
        }
        // A holder whose session closed (or that goes by another name now) since the hold began
        // fires nothing more, whenever the ticks run (a step served before the next frame too).
        if let (Some(h), Some(live)) = (&f.who, live) {
            if !holder_live(h, live) {
                if let Some(i) = &mut run.input {
                    i.agents.release(&f.action);
                    if i.holders.remove(&f.action).is_some() {
                        push_recent(&mut run.recent, Json::obj([("event", Json::from("hold_ended")), ("action", Json::from(f.action.as_str())), ("by", Json::from(h.name.as_str())), ("why", Json::from("the agent's session closed or it goes by another name"))]));
                    }
                }
                continue;
            }
        }
        if *fired_n >= MAX_INPUT_COMMANDS_PER_TICK || run.credit <= 0 {
            run.input_dropped += 1;
            continue;
        }
        *fired_n += 1;
        // A command costs about what Lex copies to undo it: the state, by its entities.
        let cost = run.sim.entities().len() as u64 + 1;
        run.credit = run.credit.saturating_sub(i64::try_from(cost).unwrap_or(i64::MAX));
        run.spent = run.spent.saturating_add(cost);
        let path = format!("/{ROOT}/{}", run.agent.trim_start_matches("lex:"));
        let (by, result) = match &f.who {
            Some(h) => {
                let w = Writer { name: &h.name, session: h.session.as_deref() };
                let r = match swarm.check_claimed_path(&path, w, "fire this live run's input") {
                    Err(e) => Err(e.to_json()),
                    Ok(()) => apply_command(run, &f.command, &f.args, &h.name, Some(w), true, world, logic, swarm, tick, hz),
                };
                (h.name.as_str(), r)
            }
            None => ("input", apply_command(run, &f.command, &f.args, "input", None, true, world, logic, swarm, tick, hz)),
        };
        // Refusals are in `recent` already; the rest (claims, a bad binding's args) go there too.
        if let Err(mut j) = result {
            if j.get("by").is_none() {
                j.set("by", Json::from(by));
                j.set("command", Json::from(f.command.as_str()));
                push_recent(&mut run.recent, j);
            }
        }
    }
}

/// A binding's args with `"$x"` / `"$y"` as the action's value in thousandths.
fn substitute(args: &Json, x: f32, y: f32) -> Json {
    // The command's input: rounded with dmath, the same on every platform (rule 4).
    let milli = |v: f32| Json::from(specula_core::dmath::round_f64(f64::from(v) * 1000.0).clamp(-1e12, 1e12) as i64);
    match args {
        Json::Str(s) if s == "$x" => milli(x),
        Json::Str(s) if s == "$y" => milli(y),
        Json::Obj(fields) => Json::Obj(fields.iter().map(|(k, v)| (k.clone(), substitute(v, x, y))).collect()),
        Json::Arr(items) => Json::Arr(items.iter().map(|v| substitute(v, x, y)).collect()),
        other => other.clone(),
    }
}

impl Live {
    /// One window frame's input, for every run that has input bindings.
    pub(crate) fn feed_window(&mut self, events: &[specula_platform::Event], pads: &[specula_platform::gamepad::GamepadState], dt_s: f32) {
        for run in self.runs.values_mut() {
            if let Some(i) = &mut run.input {
                i.window.feed(events, pads, dt_s);
            }
        }
    }

    pub(crate) fn has_holds(&self) -> bool {
        self.runs.values().any(|r| r.input.as_ref().is_some_and(|i| !i.holders.is_empty()))
    }

    /// Ends the holds of agents that are gone: their session closed, or it now goes by another
    /// name (`live`: each open session's id and agent name). Their actions are released and
    /// fire nothing more as theirs.
    pub(crate) fn end_holds(&mut self, live: &[(String, String)]) {
        self.live_agents = Some(live.to_vec());
        for run in self.runs.values_mut() {
            let Some(input) = &mut run.input else { continue };
            let gone: Vec<(String, String)> = input
                .holders
                .iter()
                .filter(|(_, h)| !holder_live(h, live))
                .map(|(a, h)| (a.clone(), h.name.clone()))
                .collect();
            for (action, name) in gone {
                input.agents.release(&action);
                input.holders.remove(&action);
                push_recent(&mut run.recent, Json::obj([("event", Json::from("hold_ended")), ("action", Json::from(action.as_str())), ("by", Json::from(name.as_str())), ("why", Json::from("the agent's session closed or it goes by another name"))]));
            }
        }
    }

    pub(crate) fn has_input(&self) -> bool {
        self.runs.values().any(|r| r.input.is_some())
    }

    pub(crate) fn call(&mut self, call: LiveCall, by: &str, w: Writer, env: Env) -> ToolOutput {
        match call {
            LiveCall::Start(s) => self.start(*s, w, env),
            LiveCall::Stop { set } => {
                let Some(run) = self.runs.get(&set) else { return no_run(self, &set) };
                if let Err(e) = env.swarm.check_claimed_path(&format!("/{ROOT}/{set}"), w, "stop this live run") {
                    return world_error(&e);
                }
                let group = run.group;
                if env.world.get(group).is_some() {
                    if let Err(e) = write_policy::commit(env.world, env.swarm, env.tick, w, Kind::Delete, Rule::LexLive, |wd| wd.delete(&EntityKey::Id(group), None).map(|_| ())) {
                        return world_error(&e);
                    }
                }
                let run = self.runs.remove(&set).expect("found above");
                let mut j = status_json(&set, &run, env.natives);
                j.set("stopped", Json::Bool(true));
                ToolOutput::text(j.to_pretty())
            }
            LiveCall::Command { set, name, args } => {
                let Some(run) = self.runs.get_mut(&set) else { return no_run(self, &set) };
                if run.halted.is_some() {
                    return halted_error(run);
                }
                // Another agent's claim on the run (its world group) holds its input too.
                if let Err(e) = env.swarm.check_claimed_path(&format!("/{ROOT}/{set}"), w, "send commands to this live run") {
                    return world_error(&e);
                }
                let Env { world, logic, swarm, tick, hz, .. } = env;
                match apply_command(run, &name, &args, by, Some(w), false, world, logic, swarm, tick, hz) {
                    Ok(mut j) => {
                        j.set("set", Json::from(set.as_str()));
                        ToolOutput::text(j.to_pretty())
                    }
                    Err(j) => ToolOutput::error(j.to_pretty()),
                }
            }
            LiveCall::Input { set, actions, ticks } => {
                let Some(run) = self.runs.get_mut(&set) else { return no_run(self, &set) };
                if let Err(e) = env.swarm.check_claimed_path(&format!("/{ROOT}/{set}"), w, "hold this live run's input actions") {
                    return world_error(&e);
                }
                let lex_hz = u64::from(run.sim.rules().tick_hz.max(1));
                let Some(input) = &mut run.input else {
                    return error("no_input", format!("set '{set}' was started without input bindings: lex_live {{action: start, input: {{actions, bindings}}}}"));
                };
                // One agent holds an action at a time.
                let holder = Holder { name: by.to_owned(), session: w.session.map(str::to_owned) };
                for (name, _) in actions.as_object().unwrap_or_default() {
                    if let Some(other) = input.holders.get(name).filter(|h| **h != holder) {
                        return crate::lex_tools::err_with("held_by_another", format!("'{name}' is held by {}; it fires as theirs until they release it", other.name), vec![("action", Json::from(name.as_str())), ("holder", Json::from(other.name.as_str()))]);
                    }
                }
                // `ticks` are the run's own (Lex) ticks: held for that long at the engine's rate.
                let engine_ticks = (ticks * u64::from(env.hz.max(1))).div_ceil(lex_hz).min(crate::logic::input::MAX_HOLD_TICKS);
                match input.agents.hold(&actions, engine_ticks) {
                    Ok(held) => {
                        // The commands these actions fire run as this agent (claim-checked).
                        if let (Some(live), Some(sid)) = (&mut self.live_agents, &holder.session) {
                            if !live.iter().any(|(s, n)| s == sid && *n == holder.name) {
                                live.push((sid.clone(), holder.name.clone()));
                            }
                        }
                        for (name, _) in actions.as_object().unwrap_or_default() {
                            input.holders.insert(name.clone(), holder.clone());
                        }
                        let note = "held actions fire their bindings from the next engine tick, as your commands (your claims apply, the journal says by you); held bindings fire once per Lex tick";
                        ToolOutput::text(Json::obj([("set", Json::from(set.as_str())), ("held", held), ("lex_ticks", Json::from(ticks)), ("engine_ticks", Json::from(engine_ticks)), ("note", Json::from(note))]).to_pretty())
                    }
                    Err(m) => ToolOutput::error(crate::tools::argument_error(&m).to_pretty()),
                }
            }
            LiveCall::Resume { set } => {
                let Some(run) = self.runs.get_mut(&set) else { return no_run(self, &set) };
                if let Err(e) = env.swarm.check_claimed_path(&format!("/{ROOT}/{set}"), w, "resume this live run") {
                    return world_error(&e);
                }
                let Some(h) = &run.halted else { return error("not_halted", format!("set '{set}' is running; there is nothing to resume")) };
                if h.get("resumable") != Some(&Json::Bool(true)) {
                    let mut j = h.clone();
                    j.set("error", Json::from("not_resumable"));
                    j.set("halted_by", h.get("error").cloned().unwrap_or(Json::Null));
                    j.set("hint", Json::from("its state was cut off part-way through a tick; lex_live {action: restart} starts it again from tick 0 with the same seed, params, present and input"));
                    return ToolOutput::error(j.to_pretty());
                }
                run.halted = None;
                // The engine clock ran on while it was halted: carry on from where it stopped.
                run.started = env.tick.saturating_sub(engine_ticks_for(run.sim.tick(), run.sim.rules().tick_hz, env.hz));
                let mut j = status_json(&set, run, env.natives);
                j.set("resumed", Json::Bool(true));
                ToolOutput::text(j.to_pretty())
            }
            LiveCall::Restart { set } => {
                let Some(run) = self.runs.get(&set) else { return no_run(self, &set) };
                let s = Start {
                    set: set.clone(),
                    rules: Arc::clone(run.sim.rules()),
                    seed: run.seed,
                    params: run.params.clone(),
                    present: run.present_src.clone(),
                    input: run.input.as_ref().map(|i| Arc::clone(&i.spec)),
                    doc: run.doc.clone(),
                };
                if let Err(e) = env.swarm.check_claimed_path(&format!("/{ROOT}/{set}"), w, "restart this live run") {
                    return world_error(&e);
                }
                let group = run.group;
                if env.world.get(group).is_some() {
                    if let Err(e) = write_policy::commit(env.world, env.swarm, env.tick, w, Kind::Delete, Rule::LexLive, |wd| wd.delete(&EntityKey::Id(group), None).map(|_| ())) {
                        return world_error(&e);
                    }
                }
                self.runs.remove(&set);
                let out = self.start(s, w, env);
                if !out.is_error {
                    let mut j = crate::lex_tools::output_json(&out);
                    j.set("restarted", Json::Bool(true));
                    return ToolOutput::text(j.to_pretty());
                }
                out
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
                let mut budget = self.limits.to_json();
                budget.set("from", Json::from("perf.toml [lex_live]; deterministic work units (Lex tick work + 1 per entity + 1), wall time only as a backstop"));
                ToolOutput::text(Json::obj([("runs", Json::Arr(list)), ("module_hooks", Json::Arr(hooks)), ("budget", budget)]).to_pretty())
            }
            LiveCall::Journal { set, offset, limit } => {
                let Some(run) = self.runs.get(&set) else { return no_run(self, &set) };
                let mut bytes = 0;
                let page: Vec<Json> = run
                    .journal
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|(t, n, a, by)| Json::obj([("tick", Json::from(*t)), ("command", Json::from(n.as_str())), ("args", a.clone()), ("by", Json::from(by.as_str()))]))
                    .take_while(|j| {
                        bytes += j.to_string().len();
                        bytes <= JOURNAL_BYTES
                    })
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
                    // Commands a save game left out (it keeps the newest): this journal starts after
                    // them, so it replays only from a state saved at its first command.
                    ("dropped_before", Json::from(run.journal_dropped)),
                    ("next_offset", next.map_or(Json::Null, Json::from)),
                    ("replayable", Json::Bool(run.journal_dropped == 0)),
                    (
                        "replay",
                        Json::from(if run.journal_dropped == 0 {
                            "lex_simulate {set, seed, params, ticks: tick, commands} with these commands (tick, command, args) reaches this hash when no module hook writes the world".to_owned()
                        } else {
                            format!("a replay from tick 0 isn't possible: a save game left out the first {} commands (it keeps the newest that fit); these commands replay only from a state saved at the first of them", run.journal_dropped)
                        }),
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
        if let Err(e) = env.swarm.check_claimed_path(&format!("/{ROOT}/{}", s.set), w, "start this live run") {
            return world_error(&e);
        }
        let sim = match Sim::new_hosted(Arc::clone(&s.rules), s.seed, env.natives) {
            Ok(sim) => sim,
            Err(m) => return error("native_failed", m),
        };
        let input = match &s.input {
            None => None,
            Some(spec) => match LiveInput::new(spec) {
                Ok(i) => Some(i),
                Err(e) => return error("invalid_input", format!("input.actions: {e}")),
            },
        };
        let set = s.set.clone();
        // The group: /lex_live/<set>, replacing a leftover one (a saved world, say).
        let made = write_policy::commit(env.world, env.swarm, env.tick, w, Kind::Batch, Rule::LexLive, |wd| {
            let root = root_group(wd, w.name)?;
            if let Ok(old) = wd.find_path(&format!("/{ROOT}/{set}")) {
                wd.delete(&EntityKey::Id(old), None)?;
            }
            run_group(wd, root, &set, s.seed, w.name)
        });
        let group = match made {
            Ok((id, _)) => id,
            Err(e) => return world_error(&e),
        };
        let rules_present = s.present.iter().map(|p| (filter_of(&s.rules, &p.tags), p.spawn.clone())).collect();
        let mut run = Run {
            sim,
            agent: format!("lex:{}", s.set),
            seed: s.seed,
            params: s.params,
            started: env.tick,
            present: rules_present,
            present_src: s.present,
            input,
            group,
            shown: BTreeMap::new(),
            presented: None,
            hidden: 0,
            present_error: None,
            behind: 0,
            last_work: 0,
            spent: 0,
            credit: 0,
            input_journaled: 0,
            input_dropped: 0,
            halted: None,
            journal: Vec::new(),
            recent: VecDeque::new(),
            hook_calls: 0,
            hook_failures: 0,
            args: Json::Null,
            doc: s.doc,
            journal_dropped: 0,
        };
        present(env.world, env.swarm, env.tick, &mut run);
        let mut j = status_json(&s.set, &run, env.natives);
        j.set("started", Json::Bool(true));
        self.runs.insert(s.set, run);
        ToolOutput::text(j.to_pretty())
    }

    /// One engine tick: each run steps the Lex ticks its rate has accumulated, within its share
    /// of `limits.work_per_tick`, then shows what changed. The share is a credit: a step that
    /// overspends it leaves the run in debt, and it steps again only once later engine ticks
    /// have paid that back, so a heavy set slows down (deterministically) instead of holding the
    /// engine's thread.
    pub(crate) fn tick(&mut self, env: Env, limits: Limits) {
        let Env { world, logic, swarm, tick, hz, .. } = env;
        let hz = hz.max(1);
        let n = self.runs.len().max(1);
        // Only runs that can still step share the allowance (halted and ended runs take none).
        let stepping = self.runs.values().filter(|r| r.halted.is_none() && r.sim.outcome().is_none()).count().max(1);
        let share = i64::try_from((limits.work_per_tick / stepping as u64).max(1)).unwrap_or(i64::MAX);
        let first = self.next_first % n;
        self.next_first = (first + 1) % n;
        let live_agents = self.live_agents.as_deref();
        let order: Vec<String> = self.runs.keys().cycle().skip(first).take(self.runs.len()).cloned().collect();
        for set in order {
            let Some(run) = self.runs.get_mut(&set) else { continue };
            if run.halted.is_some() {
                continue;
            }
            run.spent = 0;
            run.credit = run.credit.saturating_add(share).min(share);
            // The run's input first: the edges its actions fire, at its current tick; `held`
            // bindings fire before each Lex tick stepped below (once per Lex tick, whatever the
            // engine's rate). At most MAX_INPUT_COMMANDS_PER_TICK, each charged to the credit.
            let mut fired_n = 0;
            let held = match &mut run.input {
                Some(input) => {
                    let (edges, held) = input_commands(input);
                    fire(run, edges, &mut fired_n, live_agents, world, logic, swarm, tick, hz);
                    held
                }
                None => Vec::new(),
            };
            if run.halted.is_some() {
                continue;
            }
            let lex_hz = u64::from(run.sim.rules().tick_hz);
            let done = tick.saturating_sub(run.started) + 1;
            let target = (u128::from(done) * u128::from(lex_hz) / u128::from(hz)).min(u128::from(u32::MAX)) as u32;
            // Below Lex's own cap, the live cap stops a tick part-way (deterministically).
            let stop = (limits.step_work < specula_lex::sim::MAX_WORK_PER_TICK).then_some(limits.step_work);
            let mut steps = 0;
            while run.sim.tick() < target && run.sim.outcome().is_none() && steps < MAX_STEPS_PER_TICK && run.credit > 0 {
                if !held.is_empty() {
                    let now: Vec<Fired> = held.iter().map(|f| Fired { action: f.action.clone(), command: f.command.clone(), args: f.args.clone(), who: f.who.clone() }).collect();
                    fire(run, now, &mut fired_n, live_agents, world, logic, swarm, tick, hz);
                    if run.halted.is_some() || run.sim.outcome().is_some() {
                        break;
                    }
                }
                let agent = run.agent.clone();
                let mut host = Host { logic, world, swarm, tick, hz, agent: &agent, calls: &mut run.hook_calls, failures: &mut run.hook_failures, recent: &mut run.recent, args: &mut run.args, staged: false };
                let sim = &mut run.sim;
                sim.set_deadline(Some(Instant::now() + limits.backstop));
                sim.set_work_stop(stop);
                let stepped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sim.step_hosted(&[], &mut host)));
                sim.set_deadline(None);
                sim.set_work_stop(None);
                if let Err(p) = stepped {
                    let message = format!("a native panicked: {}", panic_text(&*p));
                    halt(run, "native_panicked", message, true, false);
                    break;
                }
                let work = run.sim.tick_work();
                let advice = "lighten the rules (fewer entities scanning each other, longer cooldowns), raise [lex_live] step_work in perf.toml, or simulate them headless with lex_simulate";
                if run.sim.interrupted() {
                    if stop.is_some_and(|s| work > s) {
                        let message = format!("one Lex tick passed the live cap of {} units of work, so it stopped part-way; {advice}", limits.step_work);
                        halt(run, "work_limit", message, true, false);
                    } else {
                        let message = format!("one Lex tick ran past {} ms of wall time (the live backstop), so the run stopped part-way through it; {advice}", limits.backstop.as_millis());
                        halt(run, "tick_overrun", message, false, false);
                    }
                    break;
                }
                if work > limits.step_work {
                    // Lex's own cap: the tick finished (its last behaviours skipped, as headless).
                    let message = format!("one Lex tick did {work} units of work, past the live cap of {} (Lex skipped the rest of its behaviours, as it would headless); {advice}", limits.step_work);
                    halt(run, "work_limit", message, true, true);
                    break;
                }
                let cost = work + run.sim.entities().len() as u64 + 1;
                run.credit = run.credit.saturating_sub(i64::try_from(cost).unwrap_or(i64::MAX));
                run.spent = run.spent.saturating_add(cost);
                run.last_work = work;
                steps += 1;
            }
            run.behind = u64::from(target.saturating_sub(run.sim.tick())) * u64::from(run.sim.outcome().is_none() && run.halted.is_none());
            if run.presented != Some(run.sim.tick()) {
                present(world, swarm, tick, run);
            }
        }
    }
}

fn no_run(live: &Live, set: &str) -> ToolOutput {
    let names: Vec<&str> = live.runs.keys().map(String::as_str).collect();
    crate::lex_tools::err_with("not_running", format!("set '{set}' is not live"), vec![("live", Json::arr(names.iter().map(|n| Json::from(*n)))), ("suggestion", Json::from("lex_live {action: start, set}"))])
}

/// Removes `/lex_live` (every run's group and what it showed) from a world loaded from world
/// files, before it goes live: world files don't hold live runs, so after `world_load` nothing
/// would update or remove those entities. Returns how many were removed. The removal isn't
/// recorded (the journal starts from the world without them).
pub(crate) fn remove_presentation(world: &mut World) -> usize {
    let Ok(id) = world.find_path(&format!("/{ROOT}")) else { return 0 };
    let removed = world.delete(&EntityKey::Id(id), None).map_or(0, |d| d.removed.len());
    world.take_changes();
    removed
}

/// `/lex_live`, made if missing.
fn root_group(wd: &mut World, by: &str) -> Result<u64, WorldError> {
    match wd.find_path(&format!("/{ROOT}")) {
        Ok(id) => Ok(id),
        Err(_) => {
            let mut g = SpawnSpec::new(Shape::Group);
            g.name = Some(ROOT.into());
            Ok(wd.spawn(g, by)?.id)
        }
    }
}

/// `/lex_live/<set>`, a run's group.
fn run_group(wd: &mut World, root: u64, set: &str, seed: u64, by: &str) -> Result<u64, WorldError> {
    let mut g = SpawnSpec::new(Shape::Group);
    g.name = Some(set.to_owned());
    g.parent = Some(EntityKey::Id(root));
    g.props = Json::obj([("lex_live_run", Json::obj([("set", Json::from(set)), ("seed", Json::from(seed.to_string()))]))]);
    Ok(wd.spawn(g, by)?.id)
}

/// Shows the run's entities as world entities (journaled replay-only under `lex:<set>`). A
/// shown entity someone deleted is shown again; a deleted group is made again.
fn present(world: &mut World, swarm: &mut Swarm, tick: u64, run: &mut Run) {
    let prev = world.recording();
    world.set_recording(Recording::Snapshot);
    let agent = run.agent.clone();
    let set = agent.trim_start_matches("lex:").to_owned();
    if world.get(run.group).is_none() {
        // Deleted under the run: make it again (its entities went with it).
        run.shown.clear();
        match root_group(world, &agent).and_then(|root| run_group(world, root, &set, run.seed, &agent)) {
            Ok(g) => run.group = g,
            Err(e) => {
                run.present_error = Some(format!("the run's group /{ROOT}/{set} can't be made again: {}", e.message));
                world.set_recording(prev);
                write_policy::journal_exempt(world, swarm, tick, Exempt::LexLive { agent: &agent });
                return;
            }
        }
    }
    let r = run.sim.rules().clone();
    let mut stale: Vec<u32> = Vec::new();
    let mut hidden = 0;
    let mut fail: Option<String> = None;
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
                    if pos != e.pos && world.get(wid).is_some() {
                        let change = UpdateSpec { position: Some(metres(e.pos)), ..Default::default() };
                        if let Err(err) = world.update(&EntityKey::Id(wid), change, &agent) {
                            fail = Some(format!("moving {}: {}", e.id, err.message));
                            stale.push(e.id);
                        }
                    } else if world.get(wid).is_none() {
                        // Deleted by someone: shown again below.
                        stale.push(e.id);
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
            if world.get(s.id).is_some() {
                let _ = world.delete(&EntityKey::Id(s.id), None);
            }
        }
    }
    // New (or re-ruled, or deleted) entities, within the cap.
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
            hidden += 1;
            continue;
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
        // A leftover entity with this name (someone renamed or kept it) gives way.
        if let Ok(old) = world.find_path(&format!("/{ROOT}/{set}/e{}", e.id)) {
            let _ = world.delete(&EntityKey::Id(old), None);
        }
        match world.spawn(spec, &agent) {
            Ok(ent) => {
                run.shown.insert(e.id, Shown { id: ent.id, rule: ri, pos: e.pos });
            }
            Err(err) => {
                fail = Some(format!("showing {}: {}", e.id, err.message));
                hidden += 1;
            }
        }
    }
    for (lid, s) in run.shown.iter_mut() {
        if let Some(e) = run.sim.entity(*lid) {
            s.pos = e.pos;
        }
    }
    run.hidden = hidden;
    if fail.is_some() {
        run.present_error = fail;
    }
    run.presented = Some(run.sim.tick());
    world.set_recording(prev);
    write_policy::journal_exempt(world, swarm, tick, Exempt::LexLive { agent: &agent });
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
        ("behind", Json::from(run.behind)),
        ("work", Json::obj([("last_tick", Json::from(run.last_work)), ("this_engine_tick", Json::from(run.spent)), ("credit", Json::from(run.credit))])),
        ("input_commands", Json::obj([("journaled", Json::from(run.input_journaled)), ("max", Json::from(MAX_INPUT_JOURNAL)), ("dropped", Json::from(run.input_dropped))])),
        ("halted", run.halted.clone().unwrap_or(Json::Null)),
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
    if let Some(i) = &run.input {
        let bindings = Json::arr(i.spec.bindings.iter().take(MAX_BINDINGS).map(|b| {
            let on = match b.on {
                Edge::Pressed => "pressed",
                Edge::Released => "released",
                Edge::Held => "held",
            };
            Json::obj([("action", Json::from(b.action.as_str())), ("on", Json::from(on)), ("command", Json::from(b.command.as_str()))])
        }));
        j.set("input", Json::obj([("actions", Json::arr(i.window.names().map(Json::from))), ("bindings", bindings), ("held", i.agents.holds_json()), ("holders", Json::Obj(i.holders.iter().map(|(a, h)| (a.clone(), Json::from(h.name.as_str()))).collect()))]));
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
            check_keys(args, "lex_live start", &[&["action", "set", "seed", "params", "present", "input"]])?;
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
            let input = input_arg(args, &rules)?;
            LiveCall::Start(Box::new(Start { set, rules, seed, params: Json::Obj(params), present, input, doc: Some(Arc::new(l.doc.to_json())) }))
        }
        "input" => {
            check_keys(args, "lex_live input", &[&["action", "set", "actions", "ticks"]])?;
            let actions = args.get("actions").cloned().ok_or("'actions' is required: {action: true | number | [x, y]}")?;
            let ticks = crate::tools::int_arg(args, "ticks", 0, crate::logic::input::MAX_HOLD_TICKS)?.unwrap_or(1);
            LiveCall::Input { set: set()?, actions, ticks }
        }
        "resume" => {
            check_keys(args, "lex_live resume", &[&["action", "set"]])?;
            LiveCall::Resume { set: set()? }
        }
        "restart" => {
            check_keys(args, "lex_live restart", &[&["action", "set"]])?;
            LiveCall::Restart { set: set()? }
        }
        "stop" => {
            check_keys(args, "lex_live stop", &[&["action", "set"]])?;
            LiveCall::Stop { set: set()? }
        }
        "command" => {
            check_keys(args, "lex_live command", &[&["action", "set", "command", "args"]])?;
            let name = crate::tools::string_arg(args, "command")?.ok_or("'command' is required: a command id of the set")?.to_owned();
            let cargs = args.get("args").cloned().unwrap_or(Json::Null);
            if cargs.to_string().len() > MAX_ARGS_BYTES {
                return Err(format!("'args' is larger than {MAX_ARGS_BYTES} bytes (the run journals every command's args)"));
            }
            LiveCall::Command { set: set()?, name, args: cargs }
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
        other => return Err(format!("'action' {other:?} is not one of start, stop, command, input, resume, restart, status, journal")),
    }))
}

/// `present`: [{tags, spawn}], at most [`MAX_PRESENT`]. Default: every entity as a small cube.
fn input_arg(args: &Json, rules: &Rules) -> Result<Option<Arc<InputSpec>>, String> {
    let Some(v) = args.get("input").filter(|v| !v.is_null()) else { return Ok(None) };
    // A string names a `data` item of the set: the bindings live in the rule set.
    let v = match v {
        Json::Str(id) => rules.data.iter().find(|d| d.id == *id).map(|d| d.value.clone()).ok_or_else(|| format!("input: the set has no data item '{id}' (give the bindings, or the id of a data item holding them)"))?,
        other => other.clone(),
    };
    check_keys(&v, "input", &[&["actions", "bindings"]])?;
    let actions = v.get("actions").ok_or("input.actions is required: a specula.input.v1 action map ({format, actions: [{name, kind, bindings}]})")?.to_string();
    if actions.len() > 64 * 1024 {
        return Err("input.actions is larger than 64 KB".into());
    }
    let map = crate::logic::input::ModuleInput::new(&actions).map_err(|e| format!("input.actions: {e}"))?;
    let names: Vec<&str> = map.names().collect();
    let list = v.get("bindings").and_then(Json::as_array).ok_or("input.bindings is required: [{action, on?, command, args?}]")?;
    if list.is_empty() || list.len() > MAX_BINDINGS {
        return Err(format!("input.bindings must list 1 to {MAX_BINDINGS} bindings"));
    }
    let mut bindings = Vec::new();
    for (i, b) in list.iter().enumerate() {
        let at = format!("input.bindings[{i}]");
        check_keys(b, &at, &[&["action", "on", "command", "args"]])?;
        let action = b.get("action").and_then(Json::as_str).ok_or(format!("{at}.action is required"))?;
        if !names.contains(&action) {
            return Err(format!("{at}.action: no action '{action}' in input.actions (it has: {})", names.join(", ")));
        }
        let on = match b.get("on").map(|o| o.as_str()) {
            None | Some(Some("pressed")) => Edge::Pressed,
            Some(Some("released")) => Edge::Released,
            Some(Some("held")) => Edge::Held,
            _ => return Err(format!("{at}.on must be pressed, released or held")),
        };
        let command = b.get("command").and_then(Json::as_str).ok_or(format!("{at}.command is required: a command id of the set"))?;
        if !rules.commands.iter().any(|c| c.id == command) {
            return Err(format!("{at}.command: the set has no command '{command}'"));
        }
        let cargs = b.get("args").cloned().unwrap_or(Json::Null);
        if !matches!(cargs, Json::Null | Json::Obj(_)) || cargs.to_string().len() > MAX_ARGS_BYTES {
            return Err(format!("{at}.args must be an object of at most {MAX_ARGS_BYTES} bytes"));
        }
        bindings.push(InputBinding { action: action.to_owned(), on, command: command.to_owned(), args: cargs });
    }
    Ok(Some(Arc::new(InputSpec { actions, bindings })))
}

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

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    vec![ToolDef {
        name: "lex_live".into(),
        description: "Runs a stored Lex rule set live in the engine: the engine steps it on its own tick (its tick_hz; time_control pauses, steps and fast-forwards it with everything else) and shows its entities as world entities under /lex_live/<set> (models and positions from 'present' rules), journaled as the run's tick under lex:<set>. Natives the hosted game implements run in-process; any other native calls the module hook it names (module/name) with its reason. action: start {set, seed?, params?, present?: [{tags: [..], spawn: {shape, color, scale, model ...}}], input?}, command {set, command, args} (applied at once at the run's tick, journaled), input {set, actions, ticks?} (hold the run's input actions like a player), resume {set} (after a halt that left the state whole), restart {set} (from tick 0, same settings), status {set?}, journal {set, offset?, limit?} (the accepted commands with their ticks, input's included: lex_simulate with the same seed, params and commands reaches the same hash), stop {set}. input makes a run playable without a hosted game: {actions: a specula.input.v1 action map (keys, mouse, gamepad, touch), bindings: [{action, on: pressed|released|held, command, args (\"$x\"/\"$y\": the action value in thousandths)}]}, or the id of a data item of the set holding that. Runs share a work allowance per engine tick (deterministic; status says how far a run is behind); a Lex tick past the work cap, a panicking native or (backstop) 1 s of wall time halts that run (status.halted). /lex_live is reserved for lex_live (world tools may delete there, not spawn or set); path claims on /lex_live/<set> hold a run. At most 4 runs; 2000 shown entities each.".into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("action", t("string", "start, command, input, resume, restart, status (default), journal or stop.")),
                    ("input", Json::obj([("description", Json::from("start: {actions: {format: \"specula.input.v1\", actions: [{name, kind, bindings}]}, bindings: [{action, on?, command, args?}]}, or the id of a data item of the set holding it."))])),
                    ("actions", t("object", "input: {action: true | number | [x, y]} to hold, like logic_input.")),
                    ("ticks", t("integer", "input: how many of the run's own Lex ticks to hold the actions (default 1; 0 releases); held bindings fire once per Lex tick.")),
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
            self.ticks_with(n, LIMITS);
        }

        fn ticks_with(&mut self, n: u64, limits: Limits) {
            for _ in 0..n {
                let env = Env { world: &mut self.world, logic: &mut self.logic, swarm: &mut self.swarm, natives: &self.natives, tick: self.tick, hz: self.hz };
                self.live.tick(env, limits);
                self.tick += 1;
            }
        }

        fn run_status(&mut self, set: &str) -> Json {
            json(&self.call(LiveCall::Status { set: Some(set.into()) })).get("runs").and_then(Json::as_array).unwrap()[0].clone()
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
        LiveCall::Start(Box::new(Start { set: "t".into(), rules: rules(), seed: 3, params: Json::Obj(Vec::new()), present, input: None, doc: None }))
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
    fn another_agents_claim_on_the_run_holds_its_commands() {
        use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
        let mut rig = Rig::new(30);
        let session = |id: &str| SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let (ann, bob) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![ann.clone(), bob.clone()];
        rig.swarm.hello(&ann, "ann".into(), "builder".into(), None, &live).unwrap();
        rig.swarm.hello(&bob, "bob".into(), "builder".into(), None, &live).unwrap();
        assert!(!rig.call(start(Vec::new())).is_error);
        rig.swarm.take_claim(&rig.world, &bob, ClaimSpec { scope: Scope::path("/lex_live/t").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        let out = rig.call(LiveCall::Command { set: "t".into(), name: "build".into(), args: Json::parse(r#"{"at": "a"}"#).unwrap() });
        assert!(out.is_error && json(&out).get("error") == Some(&Json::from("claimed")), "{}", json(&out));
        let st = json(&rig.call(LiveCall::Status { set: Some("t".into()) }));
        assert_eq!(st.get("runs").and_then(Json::as_array).unwrap()[0].get("commands").and_then(Json::as_u64), Some(0), "nothing was applied");
    }

    fn two_agents(rig: &mut Rig) -> (SessionInfo, SessionInfo) {
        let session = |id: &str| SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() };
        let (ann, bob) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![ann.clone(), bob.clone()];
        rig.swarm.hello(&ann, "ann".into(), "builder".into(), None, &live).unwrap();
        rig.swarm.hello(&bob, "bob".into(), "builder".into(), None, &live).unwrap();
        (ann, bob)
    }

    /// Review repro (probe.py claims): bob's claim on the run holds ann's stop too; a region
    /// claim at the origin doesn't hold a start (live runs' output has no region footprint).
    #[test]
    fn claims_hold_stop_and_regions_dont_cover_live_runs() {
        use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
        let mut rig = Rig::new(30);
        let (_, bob) = two_agents(&mut rig);
        let region = Scope::region(Vec3::new(-100.0, -100.0, -100.0), Vec3::new(100.0, 100.0, 100.0)).unwrap();
        rig.swarm.take_claim(&rig.world, &bob, ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        let out = rig.call(start(walkers_as_spheres()));
        assert!(!out.is_error, "a region claim at the origin doesn't block a start: {}", json(&out));
        rig.ticks(40);
        assert!(!rig.shown().is_empty(), "entities shown inside bob's region");
        rig.swarm.take_claim(&rig.world, &bob, ClaimSpec { scope: Scope::path("/lex_live/t").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        let out = rig.call(LiveCall::Stop { set: "t".into() });
        assert!(out.is_error && json(&out).get("error") == Some(&Json::from("claimed")), "{}", json(&out));
        assert!(!rig.call(LiveCall::Status { set: Some("t".into()) }).is_error, "still live");
    }

    /// Review repro (probe.py heavy): live runs get a work allowance per engine tick (they fall
    /// behind, and catch up); one Lex tick past the work cap halts its run (deterministically),
    /// and one past the wall-time backstop too (marked not deterministic); never the engine.
    #[test]
    fn live_runs_fall_behind_within_the_budget_and_overruns_halt_the_run() {
        let mut rig = Rig::new(30);
        rig.call(start(walkers_as_spheres()));
        // An allowance of one unit: each step (at least 2 units) leaves a debt the next engine
        // ticks pay back before the run steps again.
        rig.ticks_with(10, Limits { work_per_tick: 1, ..LIMITS });
        let run = rig.run_status("t");
        let (lex, behind) = (run.get("tick").and_then(Json::as_u64).unwrap(), run.get("behind").and_then(Json::as_u64).unwrap());
        assert!(lex > 0 && lex < 10 && lex + behind == 10, "slowed, not stopped: {run}");
        rig.ticks(1);
        let run = rig.run_status("t");
        assert_eq!((run.get("tick").and_then(Json::as_u64), run.get("behind").and_then(Json::as_u64)), (Some(11), Some(0)), "caught up: {run}");

        // A set heavy enough to pass a small work cap (hundreds of entities pulsing), then the
        // backstop (a zero deadline).
        for (set, limits, code, deterministic) in [
            ("heavy", Limits { step_work: 10_000, ..LIMITS }, "work_limit", true),
            ("heavy2", Limits { backstop: Duration::ZERO, ..LIMITS }, "tick_overrun", false),
        ] {
            let heavy = r#"{"lex": 1, "name": "heavy", "tick_hz": 30, "items": {
                "lane": {"kind": "path", "points": [[0,0,0],[1,0,0]]},
                "e": {"kind": "archetype", "tags": ["enemy"], "attributes": {"hp": "1000000pt"},
                      "behaviours": [{"type": "health", "attribute": "hp"},
                                     {"type": "attack", "targets": ["enemy"], "range": "1000m", "cooldown": "0.034s", "delivery": "pulse", "payload": [{"damage": "0pt"}]}]},
                "waves": {"kind": "waves", "waves": [{"groups": [{"archetype": "e", "count": 600, "at": ["lane"], "every": "0s"}]}]}}}"#;
            let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(heavy).unwrap(), &Options::default()).unwrap().rules);
            let s = Start { set: set.into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
            assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
            rig.ticks_with(5, limits);
            let run = rig.run_status(set);
            let halted = run.get("halted").cloned().unwrap_or(Json::Null);
            assert_eq!(halted.get("error"), Some(&Json::from(code)), "{run}");
            assert_eq!(halted.get("deterministic"), Some(&Json::Bool(deterministic)), "{run}");
            let refused = rig.call(LiveCall::Command { set: set.into(), name: "x".into(), args: Json::Null });
            assert!(refused.is_error && json(&refused).get("error") == Some(&Json::from(code)), "a halted run takes no commands");
            rig.call(LiveCall::Stop { set: set.into() });
        }
    }

    /// Review round 2 (finding 3): when steps land depends on work, not on the machine. A run
    /// whose native sleeps (a slow machine) reaches the same ticks, falls behind on the same
    /// engine ticks and leaves the same world hash, tick by tick, as a fast one.
    #[test]
    fn steps_land_on_the_same_engine_ticks_on_a_slow_machine() {
        struct Nap(u64);
        impl specula_lex::NativeHook for Nap {
            fn run(&self, _cx: &mut specula_lex::NativeCtx<'_, '_>) -> Result<(), String> {
                std::thread::sleep(Duration::from_millis(self.0));
                Ok(())
            }
        }
        let items = RULES.replace(r#""glow": {"kind": "native", "hook": "fx/glow", "reason": "a world effect", "phase": "any"},"#, r#""glow": {"kind": "native", "hook": "test.nap", "reason": "a test", "phase": "any"}, "tick_nap": {"kind": "timer", "every": "1t", "do": [{"native": "glow"}]},"#);
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&items).unwrap(), &Options::default()).unwrap().rules);
        let trace = |nap: u64| {
            // A 15 Hz engine and a 30 Hz set: two Lex ticks wanted per engine tick, and an
            // allowance that runs out after about one (so the run is often behind).
            let mut rig = Rig::new(15);
            rig.natives.insert("test.nap".into(), Arc::new(Nap(nap)) as Arc<dyn specula_lex::NativeHook>);
            let s = Start { set: "t".into(), rules: Arc::clone(&rules), seed: 2, params: Json::Obj(Vec::new()), present: walkers_as_spheres(), input: None, doc: None };
            assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
            let limits = Limits { work_per_tick: 3, ..LIMITS };
            let mut out = Vec::new();
            for _ in 0..40 {
                rig.ticks_with(1, limits);
                let run = rig.run_status("t");
                out.push((run.get("tick").and_then(Json::as_u64), run.get("behind").and_then(Json::as_u64), rig.world.state_hash()));
            }
            out
        };
        let (fast, slow) = (trace(0), trace(3));
        assert!(fast.iter().any(|t| t.1 > Some(0)), "the allowance holds the run back: {fast:?}");
        assert_eq!(fast, slow);
    }

    /// Review round 2 (finding 1, hole.py): /lex_live is lex_live's. Ann can't spawn into it,
    /// move anything there or reparent an entity into it (so nothing hides from bob's region
    /// claim under it); deleting there is allowed (the run shows its entity again), and starting
    /// a run inside bob's region still works.
    #[test]
    fn lex_live_is_reserved_and_hides_nothing_from_region_claims() {
        use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
        let mut rig = Rig::new(30);
        let (_, bob) = two_agents(&mut rig);
        let bob_w = Writer { name: "bob", session: Some("bbbbbb22") };
        let mut statue = SpawnSpec::new(Shape::Cube);
        statue.name = Some("statue".into());
        statue.position = Vec3::new(20.0, 0.0, 20.0);
        rig.swarm.commit(&mut rig.world, 0, bob_w, Kind::Spawn, |wd| wd.spawn(statue, "bob").map(|e| e.id)).unwrap();
        let region = Scope::region(Vec3::new(-25.0, -5.0, -25.0), Vec3::new(25.0, 5.0, 25.0)).unwrap();
        rig.swarm.take_claim(&rig.world, &bob, ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        let out = rig.call(start(walkers_as_spheres()));
        assert!(!out.is_error, "start inside bob's region: {}", json(&out));
        rig.ticks(40);
        let code = |r: Result<((), Option<u64>), WorldError>| r.err().map(|e| e.code);
        // Control: a top-level spawn in bob's region is claimed.
        let mut cube = SpawnSpec::new(Shape::Cube);
        cube.position = Vec3::new(20.0, 0.0, 21.0);
        assert_eq!(code(rig.swarm.commit(&mut rig.world, 0, ANN, Kind::Spawn, |wd| wd.spawn(cube, "ann").map(|_| ()))), Some("claimed"));
        // Under /lex_live: reserved, at the run's group or at the root.
        for parent in ["/lex_live", "/lex_live/t"] {
            let mut squat = SpawnSpec::new(Shape::Cube);
            squat.name = Some("squat".into());
            squat.parent = Some(EntityKey::Path(parent.into()));
            squat.position = Vec3::new(20.0, 0.0, 21.0);
            squat.scale = Vec3::new(8.0, 8.0, 8.0);
            assert_eq!(code(rig.swarm.commit(&mut rig.world, 0, ANN, Kind::Spawn, |wd| wd.spawn(squat, "ann").map(|_| ()))), Some("reserved"), "{parent}");
        }
        let mut group = SpawnSpec::new(Shape::Group);
        group.name = Some("mine".into());
        group.parent = Some(EntityKey::Path("/lex_live".into()));
        assert_eq!(code(rig.swarm.commit(&mut rig.world, 0, ANN, Kind::Spawn, |wd| wd.spawn(group, "ann").map(|_| ()))), Some("reserved"), "even a group");
        // Moving a shown entity onto the statue, or the statue into /lex_live: refused.
        let onto = UpdateSpec { position: Some(Vec3::new(20.0, 0.0, 20.0)), ..Default::default() };
        assert_eq!(code(rig.swarm.commit(&mut rig.world, 0, ANN, Kind::Set, |wd| wd.update(&EntityKey::Path("/lex_live/t/e1".into()), onto, "ann").map(|_| ()))), Some("reserved"));
        let into = UpdateSpec { parent: Some(Some(EntityKey::Path("/lex_live".into()))), ..Default::default() };
        let r = code(rig.swarm.commit(&mut rig.world, 0, ANN, Kind::Set, |wd| wd.update(&EntityKey::Path("/statue".into()), into, "ann").map(|_| ())));
        assert!(matches!(r, Some("reserved") | Some("claimed")), "{r:?}");
        assert!(rig.world.find_path("/statue").is_ok());
        // Deleting a shown entity is allowed; the run shows it again.
        assert_eq!(code(rig.swarm.commit(&mut rig.world, 0, ANN, Kind::Delete, |wd| wd.delete(&EntityKey::Path("/lex_live/t/e1".into()), None).map(|_| ()))), None);
        rig.ticks(1);
        assert!(rig.world.find_path("/lex_live/t/e1").is_ok());
        assert!(!rig.call(LiveCall::Stop { set: "t".into() }).is_error, "stop inside bob's region");
    }

    /// Review round 2 (finding 2): a refused command takes back what its module hooks did to
    /// module state, RNG streams and queued events, not only the world. Uses an in-memory module
    /// (no build) whose hook keeps a counter, draws a number, spawns and emits.
    #[test]
    fn a_refused_command_takes_back_module_state_and_events() {
        use specula_sdk::{Cx, HookDef, Module, ModuleInfo, SdkError, Spawn, Value};
        #[derive(Default)]
        struct Counter;
        impl Module for Counter {
            fn describe(&self) -> ModuleInfo {
                ModuleInfo { name: "counter", hooks: vec![HookDef { name: "bump", doc: "counts, draws, spawns, emits", args: r#"{"type": "object"}"# }], ..ModuleInfo::default() }
            }
            fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
                cx.spawn(Spawn::new("group").name("counter")).map(drop)
            }
            fn hook(&self, cx: &mut Cx, _name: &str, _args: &Value) -> Result<Value, SdkError> {
                let n = cx.state("n")?.as_u64().unwrap_or(0) + 1;
                cx.set_state("n", n)?;
                let roll = cx.random_below("dice", 1000)?;
                if cx.find("/counter/made")?.is_none() {

                    cx.spawn(Spawn::new("cube").name("made").parent("/counter"))?;
                }
                cx.emit("bumped", Value::map().with("n", n).with("roll", roll))?;
                Ok(Value::from(n))
            }
        }
        let mut rig = Rig::new(30);
        let journal = &mut |w: &mut World, _: Mark, _: &str, _| {
            w.take_changes();
            Ok(())
        };
        rig.logic.load(specula_logic::ModuleCode::of::<Counter>(), crate::logic::Origin::default(), None, &mut rig.world, 0, 30, journal).unwrap();
        let set = r#"{"lex": 1, "name": "atom", "tick_hz": 30, "items": {
            "bump": {"kind": "native", "hook": "counter/bump", "reason": "a test"},
            "missing": {"kind": "native", "hook": "nomod/x", "reason": "a hook nobody provides"},
            "bad": {"kind": "command", "do": [{"native": "bump"}, {"native": "missing"}]},
            "good": {"kind": "command", "do": [{"native": "bump"}]}}}"#;
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(set).unwrap(), &Options::default()).unwrap().rules);
        let s = Start { set: "atom".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        let before = rig.logic.test_snapshot("counter");
        let next_id = rig.world.next_id();
        let out = rig.call(LiveCall::Command { set: "atom".into(), name: "bad".into(), args: Json::Null });
        assert!(out.is_error);
        assert_eq!(json(&out).get("error"), Some(&Json::from("runtime_error")), "refusals carry `error`: {}", json(&out));
        assert_eq!(rig.logic.test_snapshot("counter"), before, "state, RNG and events as they were");
        assert!(rig.world.find_path("/counter/made").is_err(), "the spawn was rolled back");
        assert!(rig.world.next_id() > next_id, "ids it used are not handed out again");
        assert_eq!(rig.swarm.journal().next_id_noted(), rig.world.next_id(), "and the journal knows");
        let out = rig.call(LiveCall::Command { set: "atom".into(), name: "good".into(), args: Json::Null });
        assert!(!out.is_error, "{}", json(&out));
        assert_ne!(rig.logic.test_snapshot("counter"), before, "kept when the command is");
        assert!(rig.world.find_path("/counter/made").is_ok());
    }

    /// Review round 4 (finding 1, probe.py heavy): a set doing far more work per Lex tick than an
    /// engine tick allows doesn't step once every engine tick (holding the main thread): each
    /// step's overspend is a debt that pauses the run until paid back, so it slows down, by the
    /// same Lex ticks on a fast and on a slow machine; past `step_work` a tick stops and halts.
    #[test]
    fn a_heavy_set_slows_down_instead_of_holding_the_engine() {
        struct Nap(u64);
        impl specula_lex::NativeHook for Nap {
            fn run(&self, _cx: &mut specula_lex::NativeCtx<'_, '_>) -> Result<(), String> {
                std::thread::sleep(Duration::from_millis(self.0));
                Ok(())
            }
        }
        let heavy = |count: u32| {
            format!(
                r#"{{"lex": 1, "name": "heavy", "tick_hz": 30, "items": {{
                "lane": {{"kind": "path", "points": [[0,0,0],[1,0,0]]}},
                "nap": {{"kind": "native", "hook": "test.nap", "reason": "a test", "phase": "any"}},
                "napping": {{"kind": "timer", "every": "1t", "do": [{{"native": "nap"}}]}},
                "e": {{"kind": "archetype", "tags": ["enemy"], "attributes": {{"hp": "1000000pt"}},
                      "behaviours": [{{"type": "health", "attribute": "hp"}},
                                     {{"type": "attack", "targets": ["enemy"], "range": "1000m", "cooldown": "0.034s", "delivery": "pulse", "payload": [{{"damage": "0pt"}}]}}]}},
                "waves": {{"kind": "waves", "waves": [{{"groups": [{{"archetype": "e", "count": {count}, "at": ["lane"], "every": "0s"}}]}}]}}}}}}"#
            )
        };
        let trace = |count: u32, nap: u64| {
            let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&heavy(count)).unwrap(), &Options::default()).unwrap().rules);
            let mut rig = Rig::new(60);
            rig.natives.insert("test.nap".into(), Arc::new(Nap(nap)) as Arc<dyn specula_lex::NativeHook>);
            let s = Start { set: "heavy".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
            assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
            let mut out = Vec::new();
            for _ in 0..90 {
                rig.ticks(1);
                let run = rig.run_status("heavy");
                out.push((run.get("tick").and_then(Json::as_u64).unwrap(), run.get("halted").and_then(|h| h.get("error")).cloned()));
            }
            (out, rig.run_status("heavy"))
        };
        // 200 entities pulsing at each other: about 160k units a Lex tick, past two engine ticks'
        // allowance, under the step cap.
        let (fast, run) = trace(200, 0);
        let lex = fast.last().unwrap().0;
        assert!(fast.iter().all(|t| t.1.is_none()), "never halted: {run}");
        assert!(lex > 5 && lex < 45, "slowed down: {lex} Lex ticks in 90 engine ticks (45 wanted): {run}");
        let steps_per_engine_tick = fast.windows(2).map(|w| w[1].0 - w[0].0).max().unwrap();
        assert_eq!(steps_per_engine_tick, 1, "at most one (overspending) step per engine tick");
        let (slow, _) = trace(200, 2);
        assert_eq!(fast, slow, "the same Lex ticks on a slow machine");
        // 2,000 entities: about 4M units a Lex tick. The first tick stops at the step cap and
        // the run halts; the engine never runs a whole such tick.
        let (huge, run) = trace(2_000, 0);
        let h = run.get("halted").cloned().unwrap();
        assert_eq!((h.get("error"), h.get("resumable"), h.get("deterministic")), (Some(&Json::from("work_limit")), Some(&Json::Bool(false)), Some(&Json::Bool(true))), "{run}");
        assert!(huge.last().unwrap().0 <= 1, "{run}");
    }

    /// How long a unit of live work takes on this machine (for calibrating [`LIMITS`]): run with
    /// `cargo test --release -p specula --lib measure_live_work -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure_live_work_unit_time() {
        for count in [100u32, 300, 700] {
            let doc = format!(
                r#"{{"lex": 1, "name": "m", "tick_hz": 30, "items": {{"lane": {{"kind": "path", "points": [[0,0,0],[1,0,0]]}},
                "e": {{"kind": "archetype", "tags": ["enemy"], "attributes": {{"hp": "1000000pt"}}, "behaviours": [{{"type": "health", "attribute": "hp"}}, {{"type": "attack", "targets": ["enemy"], "range": "1000m", "cooldown": "0.034s", "delivery": "pulse", "payload": [{{"damage": "0pt"}}]}}]}},
                "waves": {{"kind": "waves", "waves": [{{"groups": [{{"archetype": "e", "count": {count}, "at": ["lane"], "every": "0s"}}]}}]}}}}}}"#
            );
            let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&doc).unwrap(), &Options::default()).unwrap().rules);
            let mut sim = Sim::new(rules, 1, &Default::default()).unwrap();
            sim.run_to(5);
            let (mut units, t0) = (0u64, Instant::now());
            for _ in 0..30 {
                sim.step(&[]);
                units += sim.tick_work() + sim.entities().len() as u64 + 1;
            }
            let ns = t0.elapsed().as_nanos() as f64 / units as f64;
            println!("{count} entities: {} units a Lex tick, {ns:.1} ns a unit, {:.2} ms a tick", units / 30, ns * units as f64 / 30.0 / 1e6);
        }
    }

    /// An in-process native that panics halts its run (in a tick or in a command); the engine
    /// goes on.
    /// Generic input: the set's own data item binds Space to `build` (and a held stick to a
    /// command with the value). A window key press, or an agent holding the action, fires the
    /// command at the run's tick, journaled as `input` and replayable headless.
    /// A linked-in module whose `bump` hook keeps a counter and spawns `/counter/made` (n).
    fn load_counter(rig: &mut Rig) {
        use specula_sdk::{Cx, HookDef, Module, ModuleInfo, SdkError, Spawn, Value};
        #[derive(Default)]
        struct Counter;
        impl Module for Counter {
            fn describe(&self) -> ModuleInfo {
                ModuleInfo { name: "counter", hooks: vec![HookDef { name: "bump", doc: "counts and spawns", args: r#"{"type": "object"}"# }], ..ModuleInfo::default() }
            }
            fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
                cx.spawn(Spawn::new("group").name("counter")).map(drop)
            }
            fn hook(&self, cx: &mut Cx, _name: &str, _args: &Value) -> Result<Value, SdkError> {
                let n = cx.state("n")?.as_u64().unwrap_or(0) + 1;
                cx.set_state("n", n)?;
                cx.spawn(Spawn::new("cube").name(&format!("made{n}")).parent("/counter"))?;
                Ok(Value::from(n))
            }
        }
        let journal = &mut |w: &mut World, _: Mark, _: &str, _| {
            w.take_changes();
            Ok(())
        };
        rig.logic.load(specula_logic::ModuleCode::of::<Counter>(), crate::logic::Origin::default(), None, &mut rig.world, 0, 30, journal).unwrap();
    }

    /// A set whose `bump` command calls `counter/bump`, with input bindings: `go` (held) and
    /// `tap` (pressed) both fire `bump`; `many` binds `go` 64 times.
    fn bump_set(many: bool) -> (Arc<Rules>, Option<Arc<InputSpec>>) {
        let set = r#"{"lex": 1, "name": "t", "tick_hz": 30, "items": {
            "bump_hook": {"kind": "native", "hook": "counter/bump", "reason": "a test"},
            "bump": {"kind": "command", "do": [{"native": "bump_hook"}]}}}"#;
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(set).unwrap(), &Options::default()).unwrap().rules);
        let one = r#"{"action": "go", "on": "held", "command": "bump"}"#;
        let bindings: Vec<&str> = if many { vec![one; MAX_BINDINGS] } else { vec![one, r#"{"action": "tap", "command": "bump"}"#] };
        let input = format!(
            r#"{{"input": {{"actions": {{"format": "specula.input.v1", "actions": [{{"name": "go", "kind": "button", "bindings": ["Key.G"]}}, {{"name": "tap", "kind": "button", "bindings": ["Key.T"]}}]}}, "bindings": [{}]}}}}"#,
            bindings.join(", ")
        );
        let input = input_arg(&Json::parse(&input).unwrap(), &rules).unwrap();
        (rules, input)
    }

    /// Review round 3 (finding 1, input3.py launder): commands an agent's held input fires run
    /// as that agent, so bob's claim on /counter refuses ann's held `bump` like her direct
    /// command; bob's own hold goes through and is journaled by him.
    #[test]
    fn held_input_runs_as_the_agent_holding_it() {
        use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
        let mut rig = Rig::new(30);
        let (_, bob) = two_agents(&mut rig);
        let bob_w = Writer { name: "bob", session: Some("bbbbbb22") };
        load_counter(&mut rig);
        let (rules, input) = bump_set(false);
        let s = Start { set: "t".into(), rules: Arc::clone(&rules), seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.swarm.take_claim(&rig.world, &bob, ClaimSpec { scope: Scope::path("/counter").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        let direct = rig.call(LiveCall::Command { set: "t".into(), name: "bump".into(), args: Json::Null });
        assert!(direct.is_error && json(&direct).get("error") == Some(&Json::from("claimed")), "{}", json(&direct));
        assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("tap", Json::Bool(true))]), ticks: 1 }).is_error);
        rig.ticks(3);
        let run = rig.run_status("t");
        assert_eq!(run.get("commands").and_then(Json::as_u64), Some(0), "nothing journaled: {run}");
        let mut untouched = Sim::new_hosted(rules, 1, &Default::default()).unwrap();
        untouched.run_to(run.get("tick").and_then(Json::as_u64).unwrap() as u32);
        assert_eq!(run.get("hash").cloned(), Some(hex(untouched.hash())), "the run was put back (as if never commanded)");
        assert!(rig.world.find_path("/counter/made1").is_err(), "nor the world");
        let recent = run.get("recent").and_then(Json::as_array).unwrap();
        assert!(recent.iter().any(|r| r.get("by") == Some(&Json::from("ann")) && r.get("error") == Some(&Json::from("claimed"))), "{run}");
        // Bob's hold runs as bob: his claim allows it.
        let out = rig.live.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("tap", Json::Bool(true))]), ticks: 1 }, "bob", bob_w, Env { world: &mut rig.world, logic: &mut rig.logic, swarm: &mut rig.swarm, natives: &rig.natives, tick: rig.tick, hz: rig.hz });
        assert!(!out.is_error, "{}", json(&out));
        rig.ticks(3);
        let j = json(&rig.call(LiveCall::Journal { set: "t".into(), offset: 0, limit: 10 }));
        let cmds = j.get("commands").and_then(Json::as_array).unwrap();
        assert_eq!(cmds.len(), 1, "{j}");
        assert_eq!(cmds[0].get("by"), Some(&Json::from("bob")));
        assert!(rig.world.find_path("/counter/made1").is_ok());
    }

    /// Review round 3 (finding 2, input3.py flood): 64 held bindings fire once per Lex tick, at
    /// most MAX_INPUT_COMMANDS_PER_TICK per engine tick, the rest dropped and counted; input
    /// may fill only its part of the journal, so agents' commands still apply.
    #[test]
    fn held_input_is_capped_and_leaves_agents_room() {
        let mut rig = Rig::new(60);
        load_counter(&mut rig);
        let (rules, input) = bump_set(true);
        let s = Start { set: "t".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("go", Json::Bool(true))]), ticks: 100 }).is_error);
        rig.ticks(20);
        let run = rig.run_status("t");
        let lex_ticks = run.get("tick").and_then(Json::as_u64).unwrap();
        assert_eq!(lex_ticks, 10, "a 30 Hz set under a 60 Hz engine");
        let journaled = run.get("commands").and_then(Json::as_u64).unwrap();
        assert_eq!(journaled, lex_ticks * MAX_INPUT_COMMANDS_PER_TICK as u64, "8 per Lex tick stepped (one Lex tick per two engine ticks): {run}");
        let ic = run.get("input_commands").cloned().unwrap();
        assert_eq!(ic.get("dropped").and_then(Json::as_u64), Some(lex_ticks * (MAX_BINDINGS - MAX_INPUT_COMMANDS_PER_TICK) as u64), "{ic}");
        // Input's part of the journal used up: input is refused, agents still get through.
        rig.live.runs.get_mut("t").unwrap().input_journaled = MAX_INPUT_JOURNAL;
        rig.ticks(2);
        let run = rig.run_status("t");
        assert_eq!(run.get("commands").and_then(Json::as_u64), Some(journaled), "no more input commands");
        assert!(run.get("recent").and_then(Json::as_array).unwrap().iter().any(|r| r.get("error") == Some(&Json::from("input_journal_full"))), "{run}");
        let out = rig.call(LiveCall::Command { set: "t".into(), name: "bump".into(), args: Json::Null });
        assert!(!out.is_error, "an agent's own command still applies: {}", json(&out));
    }

    /// Review round 4 (finding 3, leave.py): a hold ends with its agent: when its session closes,
    /// or the session goes by another name, nothing more fires as that agent.
    #[test]
    fn holds_end_with_their_agents_session() {
        let mut rig = Rig::new(30);
        load_counter(&mut rig);
        let (rules, input) = bump_set(false);
        let s = Start { set: "t".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        let ann_live = [("aaaaaa11".to_owned(), "ann".to_owned())];
        for gone in [&[][..], &[("aaaaaa11".to_owned(), "annie".to_owned())][..]] {
            assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("go", Json::Bool(true))]), ticks: 1000 }).is_error);
            rig.ticks(5);
            rig.live.end_holds(&ann_live);
            assert!(rig.live.has_holds(), "ann is still here");
            let before = rig.run_status("t").get("commands").and_then(Json::as_u64).unwrap();
            assert!(before > 0);
            rig.live.end_holds(gone);
            assert!(!rig.live.has_holds());
            rig.ticks(20);
            let run = rig.run_status("t");
            assert_eq!(run.get("commands").and_then(Json::as_u64), Some(before), "nothing more as ann: {run}");
            assert!(run.get("recent").and_then(Json::as_array).unwrap().iter().any(|r| r.get("event") == Some(&Json::from("hold_ended"))), "{run}");
        }
    }

    /// Review round 5: a hold whose agent is gone fires nothing even when ticks run before the
    /// per-frame sweep has ended it (`fire` checks the holder too); and halted runs take no
    /// share of the allowance.
    #[test]
    fn gone_holders_never_fire_and_halted_runs_take_no_share() {
        let mut rig = Rig::new(30);
        load_counter(&mut rig);
        let (rules, input) = bump_set(false);
        let s = Start { set: "t".into(), rules: Arc::clone(&rules), seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.live.end_holds(&[("aaaaaa11".to_owned(), "ann".to_owned())]);
        assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("go", Json::Bool(true))]), ticks: 1000 }).is_error);
        rig.ticks(3);
        let before = rig.run_status("t").get("commands").and_then(Json::as_u64).unwrap();
        assert!(before > 0);
        // Ann's session closed; the engine has seen it, but the holds weren't swept yet.
        rig.live.live_agents = Some(Vec::new());
        rig.ticks(10);
        let run = rig.run_status("t");
        assert_eq!(run.get("commands").and_then(Json::as_u64), Some(before), "nothing fired as ann: {run}");
        assert!(run.get("recent").and_then(Json::as_array).unwrap().iter().any(|r| r.get("event") == Some(&Json::from("hold_ended"))), "{run}");
        assert!(!rig.live.has_holds());

        // Two runs, one halted: the other gets the whole allowance.
        let s = Start { set: "u".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.live.runs.get_mut("u").unwrap().halted = Some(Json::obj([("error", Json::from("test"))]));
        rig.live.runs.get_mut("t").unwrap().credit = 0;
        rig.ticks_with(1, Limits { work_per_tick: 1_000, ..LIMITS });
        let w = rig.run_status("t").get("work").cloned().unwrap();
        let (credit, spent) = (w.get("credit").and_then(Json::as_i64).unwrap(), w.get("this_engine_tick").and_then(Json::as_i64).unwrap());
        assert_eq!(credit + spent, 1_000, "the halted run took no share: {w}");
    }

    /// Review round 4 (finding 4): `input {ticks}` counts the run's Lex ticks (a held binding
    /// fires once per Lex tick, so 5 ticks fire 5 commands under a faster engine), and the local
    /// player's window input on an action an agent holds stays the player's.
    #[test]
    fn holds_count_lex_ticks_and_window_input_stays_the_players() {
        let mut rig = Rig::new(60);
        load_counter(&mut rig);
        let (rules, input) = bump_set(false);
        let s = Start { set: "t".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.ticks(4);
        assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("go", Json::Bool(true))]), ticks: 5 }).is_error);
        rig.ticks(30);
        let j = json(&rig.call(LiveCall::Journal { set: "t".into(), offset: 0, limit: 100 }));
        assert_eq!(j.get("total").and_then(Json::as_u64), Some(5), "5 Lex ticks held, 5 commands: {j}");
        // Ann holds `tap` while the player presses it at the window: one command each, each
        // attributed to whoever gave it.
        assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("tap", Json::Bool(true))]), ticks: 3 }).is_error);
        let down = specula_platform::Event::KeyDown { code: u32::from(b'T'), repeat: false };
        let up = specula_platform::Event::KeyUp { code: u32::from(b'T') };
        rig.live.feed_window(&[down], &[], 0.01);
        rig.live.feed_window(&[up], &[], 0.01);
        rig.ticks(2);
        let j = json(&rig.call(LiveCall::Journal { set: "t".into(), offset: 5, limit: 100 }));
        let by: Vec<Json> = j.get("commands").and_then(Json::as_array).unwrap().iter().filter_map(|c| c.get("by").cloned()).collect();
        assert_eq!(by, vec![Json::from("input"), Json::from("ann")], "{j}");
        // Another agent can't take over ann's hold.
        let bob_w = Writer { name: "bob", session: Some("bbbbbb22") };
        let out = rig.live.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("tap", Json::Bool(true))]), ticks: 3 }, "bob", bob_w, Env { world: &mut rig.world, logic: &mut rig.logic, swarm: &mut rig.swarm, natives: &rig.natives, tick: rig.tick, hz: rig.hz });
        assert!(out.is_error && json(&out).get("error") == Some(&Json::from("held_by_another")), "{}", json(&out));
    }

    /// A live run with input bindings, a tower that pulses, a random stream, presented entities
    /// and a journal: its document (for saves), started.
    fn saveable_run(rig: &mut Rig) {
        let extra = r#""controls": {"kind": "data", "value": {
            "actions": {"format": "specula.input.v1", "actions": [{"name": "build", "kind": "button", "bindings": ["Key.Space"]}]},
            "bindings": [{"action": "build", "command": "build", "args": {"at": "a"}}]}},
            "zap": {"kind": "archetype", "tags": ["stone"], "behaviours": [{"type": "attack", "targets": ["enemy"], "range": "20m", "cooldown": "0.3s", "delivery": "pulse", "payload": [{"damage": "1pt"}]}]},
            "zapper": {"kind": "command", "args": {"at": "site"}, "do": [{"spawn": "zap", "at": "arg.at"}]},
            "tithe": {"kind": "timer", "every": "0.5s", "do": [{"add": "gold", "amount": "rand(loot, 0, 3)"}]},"#;
        let items = RULES.replace(r#""gold": {"kind": "resource", "start": 5},"#, &format!(r#""gold": {{"kind": "resource", "start": 50}}, {extra}"#));
        let doc = Json::parse(&items).unwrap();
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::from_json(&doc).unwrap(), &Options::default()).unwrap().rules);
        let input = input_arg(&Json::obj([("input", Json::from("controls"))]), &rules).unwrap();
        let present = vec![
            Present { tags: vec!["enemy".into()], spawn: Json::obj([("shape", Json::from("sphere"))]) },
            Present { tags: vec!["stone".into()], spawn: Json::obj([("shape", Json::from("cube"))]) },
        ];
        let s = Start { set: "t".into(), rules, seed: 4, params: Json::Obj(Vec::new()), present, input, doc: Some(Arc::new(doc)) };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.ticks(10);
        assert!(!rig.call(LiveCall::Command { set: "t".into(), name: "zapper".into(), args: Json::parse(r#"{"at": "a"}"#).unwrap() }).is_error);
        assert!(!rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("build", Json::Bool(true))]), ticks: 1 }).is_error);
        rig.ticks(40);
    }

    /// Live runs in save games: a run saved mid-way and loaded into another engine (the world
    /// as saved, a fresh `Live`) is the same run (Lex state, anchor, credit, journal, what it
    /// shows, adopted by id) and steps on identically, world hash included.
    #[test]
    fn a_saved_live_run_loads_and_continues_identically() {
        let mut a = Rig::new(30);
        saveable_run(&mut a);
        let bytes = a.live.save_state().unwrap().expect("a run is live");
        let mut b = Rig::new(30);
        b.world = a.world.clone();
        b.tick = a.tick;
        let snap = parse_state(&bytes, &b.world, b.tick, &b.natives).unwrap();
        assert!(b.live.apply_state(Some(snap)).is_empty(), "no notes");
        assert_eq!(b.run_status("t").to_string(), a.run_status("t").to_string());
        assert_eq!(b.live.state_hash(), a.live.state_hash());
        assert_eq!(b.run_status("t").get("hash"), a.run_status("t").get("hash"));
        assert_eq!(b.live.save_state().unwrap(), Some(bytes.clone()), "the section round-trips byte for byte");
        let journal = |r: &mut Rig| json(&r.call(LiveCall::Journal { set: "t".into(), offset: 0, limit: 100 })).get("commands").cloned();
        assert_eq!(journal(&mut b), journal(&mut a));
        for t in 0..300 {
            a.ticks(1);
            b.ticks(1);
            assert_eq!((a.live.state_hash(), a.world.state_hash()), (b.live.state_hash(), b.world.state_hash()), "{t} ticks after the load");
        }
        let (sa, sb) = (a.run_status("t"), b.run_status("t"));
        assert_eq!((sa.get("tick"), sa.get("hash"), sa.get("shown")), (sb.get("tick"), sb.get("hash"), sb.get("shown")));
        // Commands after the load land the same way too.
        for r in [&mut a, &mut b] {
            assert!(!r.call(LiveCall::Command { set: "t".into(), name: "zapper".into(), args: Json::parse(r#"{"at": "a"}"#).unwrap() }).is_error);
            r.ticks(30);
        }
        assert_eq!((a.live.state_hash(), a.world.state_hash()), (b.live.state_hash(), b.world.state_hash()));
    }

    /// Review round 7 (adopt.py): a presented entity deleted before the save isn't a mismatch:
    /// the loaded run adopts what the world holds, matches the saving one's hashes at once, and
    /// both show the entity again the same way.
    #[test]
    fn a_presented_entity_deleted_before_a_save_still_matches() {
        let mut a = Rig::new(30);
        saveable_run(&mut a);
        let shown = a.shown();
        let gone = a.world.find_path(&shown[0]).unwrap();
        a.world.delete(&EntityKey::Id(gone), None).unwrap();
        let bytes = a.live.save_state().unwrap().unwrap();
        let mut b = Rig::new(30);
        b.world = a.world.clone();
        b.tick = a.tick;
        let notes = b.live.apply_state(Some(parse_state(&bytes, &b.world, b.tick, &b.natives).unwrap()));
        assert!(notes.iter().any(|n| n.get("warning") == Some(&Json::from("live_lex_presentation"))), "{notes:?}");
        assert_eq!((b.live.state_hash(), b.world.state_hash()), (a.live.state_hash(), a.world.state_hash()), "matches at the load");
        for t in 0..60 {
            a.ticks(1);
            b.ticks(1);
            assert_eq!((a.live.state_hash(), a.world.state_hash()), (b.live.state_hash(), b.world.state_hash()), "{t} ticks after the load");
        }
        assert_eq!(a.shown(), b.shown());
    }

    /// An agent's hold isn't saved (sessions aren't): a load starts without it.
    #[test]
    fn holds_are_left_out_of_saves() {
        let mut a = Rig::new(30);
        saveable_run(&mut a);
        assert!(!a.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("build", Json::Bool(true))]), ticks: 100 }).is_error);
        a.ticks(1);
        assert_eq!(a.live.holds_count(), 1);
        let bytes = a.live.save_state().unwrap().unwrap();
        let mut b = Rig::new(30);
        b.world = a.world.clone();
        b.live.apply_state(Some(parse_state(&bytes, &b.world, a.tick, &b.natives).unwrap()));
        assert_eq!(b.live.holds_count(), 0);
        assert_eq!(b.run_status("t").get("input").and_then(|i| i.get("held")), Some(&Json::obj::<&str>([])));
    }

    /// The section is checked, never trusted: cut at every length and with bits flipped it is an
    /// error or a run that steps (never a panic); a section that doesn't fit the save is refused.
    #[test]
    fn damaged_live_sections_are_errors_never_panics() {
        let mut a = Rig::new(30);
        saveable_run(&mut a);
        let bytes = a.live.save_state().unwrap().unwrap();
        let mut b = Rig::new(30);
        b.world = a.world.clone();
        let step = |snap: LiveSnapshot, base: &World| {
            let mut r = Rig::new(30);
            r.world = base.clone();
            r.tick = 60;
            r.live.apply_state(Some(snap));
            r.ticks(40);
        };
        for cut in (0..bytes.len()).step_by(1 + bytes.len() / 600) {
            assert!(parse_state(&bytes[..cut], &b.world, a.tick, &b.natives).is_err(), "cut at {cut}");
        }
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut accepted = 0;
        for _ in 0..400 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let mut bad = bytes.clone();
            let at = (x as usize) % bad.len();
            bad[at] ^= 1 << ((x >> 32) % 8);
            if let Ok(snap) = parse_state(&bad, &b.world, a.tick, &b.natives) {
                accepted += 1;
                step(snap, &b.world);
            }
        }
        println!("{accepted} of 400 flipped sections were accepted (each stepped 40 ticks)");
        // A section of another version is refused.
        let mut v = bytes.clone();
        v[0] = 9;
        assert_eq!(parse_state(&v, &b.world, a.tick, &b.natives).err().map(|e| e.code), Some("unsupported_version"));
    }

    #[test]
    fn input_bindings_make_a_run_playable_without_a_hosted_game() {
        let controls = r#""controls": {"kind": "data", "value": {
            "actions": {"format": "specula.input.v1", "actions": [{"name": "build", "kind": "button", "bindings": ["Key.Space", "Pad.South"]}]},
            "bindings": [{"action": "build", "on": "pressed", "command": "build", "args": {"at": "a"}}]}},"#;
        let items = RULES.replace(r#""gold": {"kind": "resource", "start": 5},"#, &format!(r#""gold": {{"kind": "resource", "start": 5}}, {controls}"#));
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&items).unwrap(), &Options::default()).unwrap().rules);
        let input = input_arg(&Json::obj([("input", Json::from("controls"))]), &rules).unwrap();
        let mut rig = Rig::new(30);
        let s = Start { set: "t".into(), rules: Arc::clone(&rules), seed: 3, params: Json::Obj(Vec::new()), present: walkers_as_spheres(), input, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.ticks(5);
        // A key press at the window (shorter than a tick still counts).
        let down = specula_platform::Event::KeyDown { code: specula_platform::keys::SPACE, repeat: false };
        let up = specula_platform::Event::KeyUp { code: specula_platform::keys::SPACE };
        rig.live.feed_window(&[down], &[], 0.01);
        rig.live.feed_window(&[up], &[], 0.01);
        rig.ticks(1);
        let run = rig.run_status("t");
        assert_eq!(run.get("resources").and_then(|r| r.get("gold")).and_then(Json::as_i64), Some(0), "built: {run}");
        // An agent holds the action: the second build is refused (not enough gold), as data.
        let held = rig.call(LiveCall::Input { set: "t".into(), actions: Json::obj([("build", Json::Bool(true))]), ticks: 1 });
        assert!(!held.is_error, "{}", json(&held));
        rig.ticks(1);
        let run = rig.run_status("t");
        let recent = run.get("recent").and_then(Json::as_array).unwrap();
        assert!(recent.iter().any(|r| r.get("by") == Some(&Json::from("ann")) && r.get("error") == Some(&Json::from("not_enough_gold"))), "ann's hold runs as ann: {run}");
        assert_eq!(run.get("input").and_then(|i| i.get("bindings")).and_then(Json::as_array).map(<[Json]>::len), Some(1));
        // The journal has the input's command; it replays headless to the live hash.
        let j = json(&rig.call(LiveCall::Journal { set: "t".into(), offset: 0, limit: 10 }));
        let cmds = j.get("commands").and_then(Json::as_array).unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].get("by"), Some(&Json::from("input")));
        let mut headless = Sim::new_hosted(Arc::clone(&rules), 3, &Default::default()).unwrap();
        let at = cmds[0].get("tick").and_then(Json::as_u64).unwrap() as u32;
        let live_tick = run.get("tick").and_then(Json::as_u64).unwrap() as u32;
        while headless.tick() < live_tick {
            let now: Vec<_> = if headless.tick() == at { vec![headless.parse_command("build", cmds[0].get("args").unwrap()).unwrap()] } else { Vec::new() };
            headless.step(&now);
        }
        assert_eq!(hex(headless.hash()), run.get("hash").cloned().unwrap(), "input commands replay like any other");
        // Bindings are checked when the run starts.
        let bad = |b: &str| input_arg(&Json::parse(&format!(r#"{{"input": {{"actions": {{"format": "specula.input.v1", "actions": [{{"name": "go", "kind": "button", "bindings": ["Key.G"]}}]}}, "bindings": [{b}]}}}}"#)).unwrap(), &rules).err().unwrap_or_default();
        assert!(bad(r#"{"action": "jump", "command": "build"}"#).contains("no action 'jump'"));
        assert!(bad(r#"{"action": "go", "command": "fly"}"#).contains("no command 'fly'"));
        assert!(bad(r#"{"action": "go", "command": "build", "on": "sometimes"}"#).contains("pressed, released or held"));
        assert!(input_arg(&Json::obj([("input", Json::from("nope"))]), &rules).err().unwrap_or_default().contains("no data item 'nope'"));
    }

    /// Halts that leave the state whole (the work cap) can be resumed; a halt part-way through a
    /// tick (a panicking native) can't, and `restart` starts the run again from tick 0.
    #[test]
    fn halted_runs_resume_or_restart() {
        let mut rig = Rig::new(30);
        // A native that panics in a command: the command is undone, so the state is whole.
        struct CmdBoom;
        impl specula_lex::NativeHook for CmdBoom {
            fn run(&self, _cx: &mut specula_lex::NativeCtx<'_, '_>) -> Result<(), String> {
                panic!("boom in a command");
            }
        }
        let items = RULES.replace(r#""hook": "fx/glow""#, r#""hook": "test.cmdboom""#);
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&items).unwrap(), &Options::default()).unwrap().rules);
        rig.natives.insert("test.cmdboom".into(), Arc::new(CmdBoom) as Arc<dyn specula_lex::NativeHook>);
        let s = Start { set: "t".into(), rules, seed: 3, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.ticks(30);
        assert!(rig.call(LiveCall::Command { set: "t".into(), name: "ping".into(), args: Json::Null }).is_error);
        rig.ticks(5);
        let run = rig.run_status("t");
        let h = run.get("halted").cloned().unwrap();
        assert_eq!((h.get("error"), h.get("resumable")), (Some(&Json::from("native_panicked")), Some(&Json::Bool(true))), "{run}");
        let at = run.get("tick").and_then(Json::as_u64).unwrap();
        let out = rig.call(LiveCall::Resume { set: "t".into() });
        assert!(!out.is_error, "{}", json(&out));
        rig.ticks(10);
        let run = rig.run_status("t");
        assert_eq!(run.get("halted"), Some(&Json::Null));
        assert_eq!(run.get("tick").and_then(Json::as_u64), Some(at + 10), "carries on from where it stopped: {run}");
        assert!(rig.call(LiveCall::Resume { set: "t".into() }).is_error, "nothing to resume");

        struct Boom;
        impl specula_lex::NativeHook for Boom {
            fn run(&self, cx: &mut specula_lex::NativeCtx<'_, '_>) -> Result<(), String> {
                if cx.tick() == 3 {
                    panic!("boom");
                }
                Ok(())
            }
        }
        let items = RULES.replace(r#""glow": {"kind": "native", "hook": "fx/glow", "reason": "a world effect", "phase": "any"},"#, r#""glow": {"kind": "native", "hook": "test.boom", "reason": "a test", "phase": "any"}, "tick_boom": {"kind": "timer", "every": "1t", "do": [{"native": "glow"}]},"#);
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&items).unwrap(), &Options::default()).unwrap().rules);
        rig.natives.insert("test.boom".into(), Arc::new(Boom) as Arc<dyn specula_lex::NativeHook>);
        let s = Start { set: "b".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.ticks(6);
        let out = rig.call(LiveCall::Resume { set: "b".into() });
        assert!(out.is_error && json(&out).get("error") == Some(&Json::from("not_resumable")), "{}", json(&out));
        let out = rig.call(LiveCall::Restart { set: "b".into() });
        assert!(!out.is_error, "{}", json(&out));
        let run = rig.run_status("b");
        assert_eq!((run.get("tick").and_then(Json::as_u64), run.get("halted")), (Some(0), Some(&Json::Null)), "{run}");
    }

    #[test]
    fn a_panicking_native_halts_its_run_not_the_engine() {
        struct Boom;
        impl specula_lex::NativeHook for Boom {
            fn run(&self, cx: &mut specula_lex::NativeCtx<'_, '_>) -> Result<(), String> {
                if cx.tick() >= 3 {
                    panic!("boom at tick {}", cx.tick());
                }
                Ok(())
            }
        }
        let items = RULES.replace(r#""glow": {"kind": "native", "hook": "fx/glow", "reason": "a world effect", "phase": "any"},"#, r#""glow": {"kind": "native", "hook": "test.boom", "reason": "a test", "phase": "any"}, "tick_boom": {"kind": "timer", "every": "1t", "do": [{"native": "glow"}]},"#);
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(&items).unwrap(), &Options::default()).unwrap().rules);
        let mut rig = Rig::new(30);
        rig.natives.insert("test.boom".into(), Arc::new(Boom) as Arc<dyn specula_lex::NativeHook>);
        let s = Start { set: "t".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        rig.ticks(10);
        let run = &json(&rig.call(LiveCall::Status { set: Some("t".into()) })).get("runs").and_then(Json::as_array).unwrap()[0].clone();
        let halted = run.get("halted").unwrap();
        assert_eq!(halted.get("error"), Some(&Json::from("native_panicked")), "{run}");
        assert!(halted.get("message").and_then(Json::as_str).is_some_and(|m| m.contains("boom at tick 3")), "{halted}");
        assert_eq!(run.get("tick").and_then(Json::as_u64), Some(3), "it stepped no more");
        // Stopping still works.
        assert!(!rig.call(LiveCall::Stop { set: "t".into() }).is_error);
    }

    /// Review finding 9: one failed presentation write doesn't stop presenting; an entity or the
    /// group someone deleted is shown again.
    #[test]
    fn deleted_presentation_comes_back() {
        let mut rig = Rig::new(30);
        rig.call(start(walkers_as_spheres()));
        rig.ticks(40);
        let e1 = rig.world.find_path("/lex_live/t/e1").unwrap();
        rig.world.delete(&EntityKey::Id(e1), None).unwrap();
        rig.ticks(1);
        assert!(rig.world.find_path("/lex_live/t/e1").is_ok(), "shown again");
        let g = rig.world.find_path("/lex_live/t").unwrap();
        rig.world.delete(&EntityKey::Id(g), None).unwrap();
        rig.ticks(1);
        assert!(rig.world.find_path("/lex_live/t/e1").is_ok(), "group and entities made again");
        let run = &json(&rig.call(LiveCall::Status { set: Some("t".into()) })).get("runs").and_then(Json::as_array).unwrap()[0].clone();
        assert_eq!(run.get("present_error"), None, "{run}");
    }

    /// Review repro (atomic.py): a command that calls a module hook (ambulo/launch writes the
    /// player) and then fails keeps nothing: not the hook's world changes, not its journal.
    /// Builds the ambulo module like `tests/module_reload.rs` (skips without Cargo or off Windows).
    #[test]
    fn a_refused_command_keeps_none_of_its_module_hooks_changes() {
        use specula_logic::{BuildRequest, BuildService, ModuleCode};
        use std::path::{Path, PathBuf};
        if !cfg!(windows) {
            println!("skipped: loading module libraries is Windows-only for now");
            return;
        }
        let cargo = specula_logic::build::cargo_program();
        if std::process::Command::new(&cargo).arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
            println!("skipped: cargo is not available");
            return;
        }
        fn copy_dir(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap().flatten() {
                let (src, dst) = (e.path(), to.join(e.file_name()));
                if src.is_dir() {
                    if e.file_name() != "target" {
                        copy_dir(&src, &dst);
                    }
                } else {
                    std::fs::write(&dst, std::fs::read(&src).unwrap()).unwrap();
                }
            }
        }
        let repo = specula_logic::fix::canonical(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."));
        let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| repo.join("target"), PathBuf::from);
        let base = specula_logic::fix::canonical(&target).join("module-tests-lex-live");
        std::fs::create_dir_all(base.join(".cargo")).unwrap();
        if let Ok(cfg) = std::fs::read(repo.join(".cargo").join("config.toml")) {
            std::fs::write(base.join(".cargo").join("config.toml"), cfg).unwrap();
        }
        let dir = base.join("ambulo");
        copy_dir(&repo.join("games").join("ambulo"), &dir);
        let sdk = repo.join("crates").join("specula-sdk");
        let manifest = format!(
            "[package]\nname = \"ambulo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nspecula-sdk = {{ path = {:?} }}\n\n[workspace]\n",
            sdk.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
        let mut req = BuildRequest::new(&dir);
        req.roots = vec![base.clone(), repo.clone()];
        req.target_dir = Some(base.join("target"));
        let built = BuildService::default().build(req);
        assert!(built.ok, "ambulo builds: {:?} {:?}", built.error, built.stderr_tail);
        let code = ModuleCode::open(built.library.as_ref().unwrap(), &base.join("loaded")).unwrap();
        let mut rig = Rig::new(30);
        let origin = crate::logic::Origin { dir: None, library: built.library.clone() };
        let commit = &mut |w: &mut World, _: Mark, _: &str, _| {
            w.take_changes();
            Ok(())
        };
        rig.logic.load(code, origin, None, &mut rig.world, 0, 30, commit).unwrap();
        let player = rig.world.find_path("/ambulo/player").unwrap();
        let props_before = rig.world.get(player).unwrap().props.clone();

        let set = r#"{"lex": 1, "name": "atom", "tick_hz": 30, "items": {
            "launch": {"kind": "native", "hook": "ambulo/launch", "reason": "a world effect"},
            "missing": {"kind": "native", "hook": "nomod/x", "reason": "a hook nobody provides"},
            "jump": {"kind": "command", "do": [{"native": "launch", "args": {"speed": 7}}, {"native": "missing"}]},
            "hop": {"kind": "command", "do": [{"native": "launch", "args": {"speed": 7}}]}}}"#;
        let rules = Arc::new(specula_lex::compile(&specula_lex::Document::parse(set).unwrap(), &Options::default()).unwrap().rules);
        let s = Start { set: "atom".into(), rules, seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        let hash = json(&rig.call(LiveCall::Status { set: Some("atom".into()) })).get("runs").and_then(Json::as_array).unwrap()[0].get("hash").cloned();
        let out = rig.call(LiveCall::Command { set: "atom".into(), name: "jump".into(), args: Json::Null });
        assert!(out.is_error, "the second hook fails, so the command is refused: {}", json(&out));
        assert_eq!(rig.world.get(player).unwrap().props, props_before, "the first hook's change was rolled back");
        let st = json(&rig.call(LiveCall::Status { set: Some("atom".into()) }));
        assert_eq!(st.get("runs").and_then(Json::as_array).unwrap()[0].get("hash").cloned(), hash, "and the run with it");
        // A command that succeeds keeps (and journals) its hook's change.
        let out = rig.call(LiveCall::Command { set: "atom".into(), name: "hop".into(), args: Json::Null });
        assert!(!out.is_error, "{}", json(&out));
        assert_ne!(rig.world.get(player).unwrap().props, props_before, "the launch landed");
    }

    #[test]
    fn runs_are_bounded_and_unique() {
        let mut rig = Rig::new(30);
        assert!(!rig.call(start(Vec::new())).is_error);
        assert_eq!(json(&rig.call(start(Vec::new()))).get("error"), Some(&Json::from("already_running")));
        for k in 0..3 {
            let s = Start { set: format!("t{k}"), rules: rules(), seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
            assert!(!rig.call(LiveCall::Start(Box::new(s))).is_error);
        }
        let s = Start { set: "t9".into(), rules: rules(), seed: 1, params: Json::Obj(Vec::new()), present: Vec::new(), input: None, doc: None };
        assert_eq!(json(&rig.call(LiveCall::Start(Box::new(s)))).get("error"), Some(&Json::from("runs_full")));
    }
}
