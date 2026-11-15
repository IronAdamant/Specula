//! Headless runs with bounded reports: what `lex_simulate` returns.
//!
//! A run applies timed commands, steps until a tick limit or an outcome, samples `watch`
//! expressions, checks `expect` assertions at the end and summarises everything in a report
//! whose size doesn't grow with the run's length (samples and rejections are capped).

use std::cell::RefCell;

use specula_core::Json;

use crate::diag::{codes, Diagnostic};
use crate::expr::{self, Ex, Registry, Scope, Ty};
use crate::rules::Rules;
use crate::sim::{CommandCall, Rejection, Sim, Val};
use crate::units::{human, Unit};

/// Most samples a report carries (the sample interval is widened to fit).
pub const MAX_SAMPLES: usize = 200;
/// Most rejections listed (all are counted).
pub const MAX_REJECTIONS: usize = 50;

/// A compiled global expression (no `self`), for `watch` and `expect`.
#[derive(Clone, Debug)]
pub struct Query {
    pub src: String,
    pub ex: Ex,
    pub ty: Ty,
}

/// The compiler's names, rebuilt from compiled rules (for queries and tools).
pub fn registry(r: &Rules) -> Registry {
    let mut reg = Registry { tick_hz: r.tick_hz, has_waves: r.waves.is_some(), ..Default::default() };
    for (i, a) in r.attrs.iter().enumerate() {
        reg.attrs.insert(a.name.clone(), (i as u16, a.unit));
    }
    for (i, x) in r.resources.iter().enumerate() {
        reg.resources.insert(x.id.clone(), (i as u16, x.unit));
    }
    for p in &r.params {
        reg.params.insert(p.id.clone(), (p.value, p.unit));
    }
    for (i, a) in r.archetypes.iter().enumerate() {
        reg.archetypes.insert(a.id.clone(), i as u16);
        reg.arch_attrs.push(a.attrs.iter().map(|(x, _)| *x).collect());
    }
    for (i, s) in r.sites.iter().enumerate() {
        reg.sites.insert(s.name.clone(), i as u16);
    }
    for (i, t) in r.tags.iter().enumerate() {
        reg.tags.insert(t.clone(), i as u16);
    }
    reg.streams = RefCell::new(r.streams.iter().enumerate().map(|(i, s)| (s.clone(), i as u16)).collect());
    reg
}

fn draws_random(ex: &Ex) -> bool {
    use Ex::*;
    match ex {
        Rand(..) | Chance(..) => true,
        Attr(a, _) | Alive(a) | PathRemaining(a) | ArchOf(a) | Next(a) | ArchAttr(a, _) | Neg(a) | Not(a) | Abs(a) | Exists(a) | Has(a, _) | Occupied(a, _) => draws_random(a),
        Arith(_, a, b) | Cmp(_, a, b) | And(a, b) | Or(a, b) | Min(a, b) | Max(a, b) | Dist(a, b) => draws_random(a) || draws_random(b),
        If(a, b, c) | Clamp(a, b, c) => draws_random(a) || draws_random(b) || draws_random(c),
        _ => false,
    }
}

/// Compiles a `watch` or `expect` expression: global names only, and no random draws (a query
/// must never change the run).
pub fn compile_query(r: &Rules, src: &str) -> Result<Query, Diagnostic> {
    let reg = registry(r);
    let sc = Scope::new(&reg, "a query (watch or expect)");
    let to_diag = |e: expr::ExprError| {
        let mut d = Diagnostic::error(e.code, "", "query", format!("{} (in \"{src}\")", e.message));
        d.expected = e.expected;
        d.got = Some(src.to_owned());
        d.suggestion = e.suggestion;
        d.column = Some(e.column);
        d
    };
    let (ex, ty) = expr::compile(src, &sc).map_err(to_diag)?;
    if draws_random(&ex) {
        return Err(Diagnostic::error(codes::INVALID, "", "query", format!("\"{src}\" draws random numbers, which would change the run")).suggest("remove rand() / chance() from the query"));
    }
    Ok(Query { src: src.to_owned(), ex, ty })
}

/// A value for a report: plain numbers as numbers, other units in human form, entities by id.
pub fn show_val(r: &Rules, v: Val, ty: Ty) -> Json {
    match (v, ty) {
        (Val::Int(x), Ty::Num(Unit::Int)) => Json::from(x),
        (Val::Int(x), Ty::Num(u)) => Json::from(human(x, u, r.tick_hz)),
        (Val::Bool(b), _) => Json::Bool(b),
        (Val::Ent(Some(id)), _) => Json::from(id),
        (Val::Arch(Some(a)), _) => Json::from(r.archetypes[usize::from(a)].id.as_str()),
        (Val::Site(s), _) => Json::from(r.sites[usize::from(s)].name.as_str()),
        _ => Json::Null,
    }
}

/// Most entries in each per-name list of a report (archetypes, events, resources, waves).
pub const MAX_LISTED: usize = 100;

/// The first [`MAX_LISTED`] pairs, plus `"..."` saying how many were left out.
fn capped(mut pairs: Vec<(String, Json)>) -> Json {
    if pairs.len() > MAX_LISTED {
        let more = pairs.len() - MAX_LISTED;
        pairs.truncate(MAX_LISTED);
        pairs.push(("...".to_owned(), Json::from(format!("{more} more"))));
    }
    Json::Obj(pairs)
}

/// A command scheduled at a tick (`name` for the report).
#[derive(Clone, Debug)]
pub struct Timed {
    pub tick: u32,
    pub name: String,
    pub call: CommandCall,
}

#[derive(Clone, Debug, Default)]
pub struct RunSpec {
    /// Most ticks to run (a run also stops at an outcome: nothing happens after one).
    pub ticks: u32,
    pub commands: Vec<Timed>,
    pub watch: Vec<Query>,
    pub expect: Vec<Query>,
    /// Sample `watch` every this many ticks (0: only at the end); widened to fit [`MAX_SAMPLES`].
    pub sample_every: u32,
    /// A wall-clock limit (tools set one so a runaway rule set can't hang a call). Where the
    /// run stopped is reported; everything up to that tick is still deterministic.
    pub deadline: Option<std::time::Instant>,
}

/// Runs `spec` on `sim` and reports.
#[allow(clippy::disallowed_methods, reason = "the wall-clock deadline only stops a runaway call; a stopped run is reported as stopped_early, never as a result")]
pub fn run(sim: &mut Sim, spec: &RunSpec) -> Json {
    let r = std::sync::Arc::clone(sim.rules());
    let start = sim.tick();
    let end = start.saturating_add(spec.ticks);
    let mut every = spec.sample_every;
    if every > 0 && (spec.ticks / every) as usize > MAX_SAMPLES {
        every = spec.ticks.div_ceil(MAX_SAMPLES as u32);
    }
    let mut cmds: Vec<&Timed> = spec.commands.iter().collect();
    cmds.sort_by_key(|c| c.tick);
    let mut next = 0;
    let mut rejections: Vec<Json> = Vec::new();
    let mut refused = 0usize;
    let mut applied = 0usize;
    let mut samples: Vec<Json> = Vec::new();
    let sample = |sim: &mut Sim, samples: &mut Vec<Json>| {
        let mut row = vec![("tick".to_owned(), Json::from(sim.tick()))];
        for q in &spec.watch {
            let v = sim.query(&q.ex).map(|v| show_val(&r, v, q.ty)).unwrap_or(Json::Null);
            row.push((q.src.clone(), v));
        }
        samples.push(Json::Obj(row));
    };
    let mut note = |tick: u32, name: &str, e: &Rejection, rejections: &mut Vec<Json>| {
        refused += 1;
        if rejections.len() < MAX_REJECTIONS {
            let mut j = e.to_json();
            j.set("tick", Json::from(tick));
            j.set("command", Json::from(name));
            rejections.push(j);
        }
    };
    let mut stopped_early = false;
    // The deadline also reaches inside ticks (the sim reads the clock as it spends work).
    sim.set_deadline(spec.deadline);
    while sim.tick() < end && sim.outcome().is_none() {
        let now = sim.tick();
        if sim.interrupted() || spec.deadline.is_some_and(|d| std::time::Instant::now() > d) {
            stopped_early = true;
            break;
        }
        let mut batch: Vec<CommandCall> = Vec::new();
        let mut names: Vec<&str> = Vec::new();
        while next < cmds.len() && cmds[next].tick <= now {
            batch.push(cmds[next].call.clone());
            names.push(&cmds[next].name);
            next += 1;
        }
        for (i, e) in sim.step(&batch) {
            note(now, names[i], &e, &mut rejections);
        }
        applied += batch.len();
        if every > 0 && !spec.watch.is_empty() && (sim.tick() - start).is_multiple_of(every) && samples.len() < MAX_SAMPLES {
            sample(sim, &mut samples);
        }
    }
    sim.set_deadline(None);
    stopped_early |= sim.interrupted();
    let unapplied = cmds.len() - next;
    if !spec.watch.is_empty() && samples.last().and_then(|s| s.get("tick")).and_then(Json::as_u64) != Some(u64::from(sim.tick())) {
        sample(sim, &mut samples);
    }
    let mut expect: Vec<Json> = Vec::with_capacity(spec.expect.len());
    for q in &spec.expect {
        // A run cut short by the wall clock stopped at a machine-dependent tick, so its
        // expectations are not judged.
        let (pass, value) = if stopped_early {
            (false, Json::from("not reached: the run stopped early"))
        } else {
            match sim.query(&q.ex) {
                Ok(v) => (v == Val::Bool(true), show_val(&r, v, q.ty)),
                Err(e) => (false, Json::from(format!("error: {}", e.message))),
            }
        };
        expect.push(Json::obj([("expect", Json::from(q.src.as_str())), ("pass", Json::Bool(pass)), ("value", value)]));
    }
    let st = sim.state();
    let mut per_arch = vec![0usize; r.archetypes.len()];
    for e in st.ents.iter().filter(|e| e.alive) {
        per_arch[usize::from(e.arch)] += 1;
    }
    let by_arch: Vec<(String, Json)> =
        r.archetypes.iter().zip(&per_arch).filter(|(_, n)| **n > 0).map(|(a, n)| (a.id.clone(), Json::from(*n))).collect();
    let by_arch = capped(by_arch);
    let events = capped(r.events.iter().zip(&st.event_counts).filter(|(_, c)| **c > 0).map(|(e, c)| (e.name.clone(), Json::from(*c))).collect());
    let waves = r.waves.as_ref().map(|w| {
        Json::obj([
            ("current", Json::from(u32::from(st.wave))),
            ("total", Json::from(w.waves.len())),
            (
                "stats",
                Json::arr(st.wave_stats.iter().enumerate().filter(|(_, s)| s.started_at.is_some()).take(MAX_LISTED).map(|(i, s)| {
                    Json::obj([
                        ("wave", Json::from(i + 1)),
                        ("started_at", Json::from(s.started_at)),
                        ("cleared_at", Json::from(s.cleared_at)),
                        ("spawned", Json::from(s.spawned)),
                        ("died", Json::from(s.died)),
                        ("removed", Json::from(s.removed)),
                        ("alive", Json::from(s.alive)),
                        ("pending", Json::from(s.pending)),
                    ])
                })),
            ),
        ])
    });
    let resources = capped(r.resources.iter().zip(&st.res).map(|(d, v)| (d.id.clone(), show_val(&r, Val::Int(*v), Ty::Num(d.unit)))).collect());
    let natives = capped(r.natives.iter().zip(&st.native_calls).map(|(n, c)| (n.hook.clone(), Json::from(*c))).collect());
    let mut out = vec![
        ("rules_hash", Json::from(format!("{:016x}", r.hash))),
        ("seed", Json::from(st.seed.to_string())),
        ("from_tick", Json::from(start)),
        ("tick", Json::from(sim.tick())),
        ("seconds", Json::from(f64::from(sim.tick()) / f64::from(r.tick_hz))),
        ("complete", Json::Bool(!stopped_early)),
        ("outcome", sim.outcome().filter(|_| !stopped_early).map_or(Json::Null, |(res, t)| Json::obj([("result", Json::from(res)), ("tick", Json::from(t))]))),
        ("resources", resources),
        ("entities", Json::obj([("alive", Json::from(st.ents.iter().filter(|e| e.alive).count())), ("by_archetype", by_arch)])),
        ("events", events),
        ("commands", Json::obj([("applied", Json::from(applied - refused)), ("refused", Json::from(refused)), ("not_reached", Json::from(unapplied))])),
    ];
    if stopped_early {
        // Where it stopped depends on the machine's speed, so nothing here is a result to
        // compare: no hash, no outcome, and the run can't be saved or continued.
        out.push((
            "stopped_early",
            Json::from(format!("the wall-time budget ran out near tick {}; this run is incomplete (no hash or outcome): run fewer ticks per call, or split it with save_as and from", sim.tick())),
        ));
    } else {
        out.push(("hash", Json::from(format!("{:016x}", sim.hash()))));
    }
    if let Some(w) = waves {
        out.push(("waves", w));
    }
    if !rejections.is_empty() {
        out.push(("rejections", Json::Arr(rejections)));
    }
    if !r.natives.is_empty() {
        out.push(("native_calls", natives));
    }
    if !st.diag.counts.is_empty() {
        out.push((
            "runtime_diagnostics",
            Json::obj([
                ("counts", Json::Obj(st.diag.counts.iter().map(|(k, v)| ((*k).to_owned(), Json::from(*v))).collect())),
                ("first", Json::arr(st.diag.first.iter().map(|s| Json::from(s.as_str())))),
            ]),
        ));
    }
    if !expect.is_empty() {
        let all = expect.iter().all(|e| e.get("pass").and_then(Json::as_bool) == Some(true));
        out.push(("expect_passed", Json::Bool(all)));
        out.push(("expect", Json::Arr(expect)));
    }
    if !samples.is_empty() {
        out.push(("samples", Json::Arr(samples)));
    }
    Json::obj(out)
}
