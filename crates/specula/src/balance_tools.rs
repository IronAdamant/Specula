//! The balance lab's tools (docs/design/BALANCE.md): `balance_targets`, `balance_sweep` and
//! `balance_tune`, on any Lex rule set.
//!
//! Like `lex_simulate`, they do their work on the calling agent's protocol thread (plus worker
//! threads for the runs), never on the engine thread: the main thread only hands out a snapshot
//! of the rule set. Runs are pure Lex simulations, so results don't depend on the thread count
//! (each run is independent and results are gathered by index). Every call is bounded: points,
//! seeds and runs are capped, a wall budget stops a call between runs and hands back where to
//! continue (`next_start`, or the tune `job`), and the worker threads all agents' calls use
//! together are capped. Every result is flagged `needs_human_playtest`.

use crate::tools::{check_keys, int_arg, string_arg, Tools};
use specula_balance::sweep::{sweep, Point, SweepOptions};
use specula_balance::targets::{catalogue, Targets};
use specula_balance::tune::{TuneJob, TuneOptions, TuneParam, MAX_EVALS};
use specula_balance::PLAYTEST_NOTE;
use specula_core::Json;
use specula_lex::{compile, Diagnostic, Document, Options};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A call stops starting runs after this long (inside the caller's timeout).
const WALL_BUDGET: Duration = Duration::from_secs(25);
/// Points per sweep call, seeds per point, runs per call.
const MAX_POINTS: usize = 64;
const MAX_SEEDS: usize = 256;
const MAX_RUNS: usize = 16_384;
/// Bytes of point results one answer lists (the rest come with `next_start`).
const MAX_RESPONSE_BYTES: usize = 96 * 1024;
/// Worker threads all balance calls use at once (each call gets at least one).
fn thread_cap() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).saturating_sub(2).clamp(1, 16)
}
static BUSY: AtomicUsize = AtomicUsize::new(0);
/// Tune jobs kept, in all and per session.
const MAX_JOBS: usize = 16;
const MAX_JOBS_PER_SESSION: usize = 4;

/// Worker threads granted to one call (released on drop).
struct Grant(usize);

impl Grant {
    /// Up to `wanted` of the threads not in use; `None` when every one is (the cap holds: the
    /// caller answers `busy`).
    fn take(wanted: Option<usize>) -> Option<Grant> {
        let cap = thread_cap();
        let want = wanted.unwrap_or(cap).clamp(1, cap);
        let mut busy = BUSY.load(Ordering::SeqCst);
        loop {
            let got = want.min(cap.saturating_sub(busy));
            if got == 0 {
                return None;
            }
            match BUSY.compare_exchange(busy, busy + got, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return Some(Grant(got)),
                Err(now) => busy = now,
            }
        }
    }
}

/// The answer when every balance worker thread is in use.
fn busy() -> ToolOutput {
    err(
        "busy",
        format!("all {} balance worker threads are in use by other calls; call again shortly", thread_cap()),
        vec![("retry_after_ms", Json::from(1000))],
    )
}

impl Drop for Grant {
    fn drop(&mut self) {
        BUSY.fetch_sub(self.0, Ordering::SeqCst);
    }
}

/// Kept tune jobs.
#[derive(Default)]
pub struct Jobs(Mutex<VecDeque<Job>>);

struct Job {
    name: String,
    session: String,
    owner: String,
    /// The rule set and targets it tunes (a later call must give the same).
    rules_hash: u64,
    targets: Json,
    job: TuneJob,
}

fn err(code: &'static str, message: impl Into<String>, extra: Vec<(&str, Json)>) -> ToolOutput {
    let mut f = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    f.extend(extra);
    ToolOutput::error(Json::obj(f).to_pretty())
}

fn diags(ds: &[Diagnostic]) -> Json {
    Json::arr(ds.iter().take(50).map(Diagnostic::to_json))
}

fn playtest(j: &mut Json) {
    j.set("needs_human_playtest", Json::from(true));
    j.set("playtest_note", Json::from(PLAYTEST_NOTE));
}

/// The targets: inline (`targets`) or a file under the engine's working directory.
fn targets_arg(args: &Json) -> Result<Targets, ToolOutput> {
    let parsed = match (args.get("targets"), string_arg(args, "targets_file").map_err(|m| err("invalid_argument", m, vec![]))?) {
        (Some(_), Some(_)) => return Err(err("invalid_argument", "give 'targets' or 'targets_file', not both", vec![])),
        (Some(t), None) => Targets::parse(t),
        (None, Some(path)) => {
            let p = crate::logic::tools::project_path(path, "targets_file").map_err(|m| err(if m.contains("does not exist") { "not_found" } else { "invalid_argument" }, m, vec![]))?;
            let size = match std::fs::metadata(&p) {
                Ok(m) if m.is_file() => m.len(),
                Ok(_) => return Err(err("not_found", format!("{path} is not a file"), vec![])),
                Err(e) => return Err(err(if e.kind() == std::io::ErrorKind::NotFound { "not_found" } else { "io" }, format!("{path}: {e}"), vec![])),
            };
            if size > specula_balance::targets::MAX_DOC_BYTES as u64 {
                return Err(err("too_large", format!("{path} is {size} bytes; a targets file is at most {}", specula_balance::targets::MAX_DOC_BYTES), vec![]));
            }
            let text = std::fs::read_to_string(&p).map_err(|e| err("io", format!("{path}: {e}"), vec![]))?;
            Targets::parse_text(&text)
        }
        (None, None) => return Err(err("invalid_argument", "'targets' (a targets document) or 'targets_file' (e.g. \"games/sepulcra/lex/slice.targets.json\") is required", vec![])),
    };
    parsed.map_err(|ds| err("invalid_targets", "the targets document has errors; each says what to change", vec![("diagnostics", diags(&ds))]))
}

fn threads_arg(args: &Json) -> Result<Option<usize>, ToolOutput> {
    int_arg(args, "threads", 1, 64).map(|t| t.map(|t| t as usize)).map_err(|m| err("invalid_argument", m, vec![]))
}

/// The seeds a sweep runs: `"train"` (default), `"holdout"` or a list.
fn seeds_arg(args: &Json, t: &Targets) -> Result<Vec<u64>, ToolOutput> {
    let seeds = match args.get("seeds") {
        None | Some(Json::Null) => t.train.clone(),
        Some(Json::Str(s)) if s == "train" => t.train.clone(),
        Some(Json::Str(s)) if s == "holdout" => t.holdout.clone(),
        Some(Json::Arr(a)) => a.iter().map(Json::as_u64).collect::<Option<Vec<u64>>>().ok_or_else(|| err("invalid_argument", "'seeds' lists whole numbers", vec![]))?,
        Some(_) => return Err(err("invalid_argument", "'seeds' is \"train\", \"holdout\" or a list of seeds", vec![])),
    };
    if seeds.is_empty() || seeds.len() > MAX_SEEDS {
        return Err(err("invalid_argument", format!("1..{MAX_SEEDS} seeds per point"), vec![]));
    }
    Ok(seeds)
}

/// Sweep points: an explicit list, or a grid (the product of each param's values).
fn points_arg(args: &Json) -> Result<Vec<Point>, ToolOutput> {
    let bad = |m: &str| err("invalid_argument", m, vec![]);
    match (args.get("points"), args.get("grid")) {
        (Some(_), Some(_)) => Err(bad("give 'points' or 'grid', not both")),
        (Some(Json::Arr(list)), None) => {
            let mut out = Vec::new();
            for p in list {
                match p {
                    Json::Obj(f) => out.push(f.clone()),
                    _ => return Err(bad("each of 'points' is {param: value}")),
                }
            }
            Ok(out)
        }
        (None, Some(Json::Obj(g))) => {
            let mut out: Vec<Point> = vec![Vec::new()];
            for (id, spec) in g {
                let values: Vec<Json> = match spec {
                    Json::Arr(v) if !v.is_empty() => v.clone(),
                    Json::Obj(_) => {
                        let num = |k: &str| spec.get(k).and_then(Json::as_f64);
                        let (Some(from), Some(to)) = (num("from"), num("to")) else {
                            return Err(bad("a grid range is {\"from\": a, \"to\": b, \"steps\": n} (numbers in the param's human unit)"));
                        };
                        let steps = spec.get("steps").and_then(Json::as_u64).unwrap_or(5).clamp(2, 64) as usize;
                        (0..steps).map(|i| from + (to - from) * i as f64 / (steps - 1) as f64).map(|v| Json::from((v * 1000.0).round() / 1000.0)).collect()
                    }
                    _ => return Err(bad("each grid param is a list of values or {from, to, steps}")),
                };
                let mut next = Vec::new();
                for p in &out {
                    for v in &values {
                        let mut q = p.clone();
                        q.push((id.clone(), v.clone()));
                        next.push(q);
                        if next.len() > 4096 {
                            return Err(bad("the grid has too many points; split it"));
                        }
                    }
                }
                out = next;
            }
            Ok(out)
        }
        (None, None) => Ok(vec![Vec::new()]),
        _ => Err(bad("'points' is a list of {param: value}; 'grid' is {param: [values] or {from, to, steps}}")),
    }
}

/// The rules as written, compiled (for binding checks and the param list).
fn base_rules(doc: &Document) -> Result<specula_lex::Rules, ToolOutput> {
    compile(doc, &Options::default()).map(|c| c.rules).map_err(|ds| err("invalid", "the rules don't compile; fix these first (lex_validate)", vec![("diagnostics", diags(&ds))]))
}

fn params_json(rules: &specula_lex::Rules) -> Json {
    Json::arr(rules.params.iter().map(|p| {
        let h = |v: i64| Json::from(specula_lex::units::human(v, p.unit, rules.tick_hz));
        Json::obj([("id", Json::from(p.id.as_str())), ("unit", Json::from(p.unit.name())), ("value", h(p.value)), ("min", h(p.min)), ("max", h(p.max)), ("doc", Json::from(p.doc.as_str()))])
    }))
}

/// Answers a `balance_*` call (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    match name {
        "balance_targets" => targets_tool(tools, session, args),
        "balance_sweep" => sweep_tool(tools, session, args),
        "balance_tune" => tune_tool(tools, session, args),
        other => err("unknown_tool", format!("unknown balance tool '{other}'"), vec![]),
    }
}

fn rules_doc(tools: &Tools, session: &SessionInfo, args: &Json, tool: &str) -> Result<(Document, Option<String>), ToolOutput> {
    crate::lex_tools::doc_arg(tools, session, args, tool)
}

fn targets_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "balance_targets", &[&["targets", "targets_file", "set", "document"]]) {
        return err("invalid_argument", m, vec![]);
    }
    if args.get("targets").is_none() && args.get("targets_file").is_none() {
        return ToolOutput::text(
            Json::obj([
                ("metrics", catalogue()),
                ("format", Json::from("{\"balance\": 1, \"name\", \"ticks\": \"12m\", \"seeds\": {\"train\": 24, \"holdout\": 24, \"base\": 1}, \"policy\": {\"every\": \"1s\", \"wait_on\": [codes], \"steps\": [{\"command\", \"args\", \"at\"?, \"after_wave\"?, \"when\"?}]} or \"policies\": [{\"name\", \"weight\"?, \"every\", \"wait_on\", \"steps\"}] (several play styles: every target must hold under each), \"targets\": [{\"id\", \"metric\", ..., \"target\", \"tolerance\", \"weight\"?, \"policies\"?: [names]}]}")),
                ("example", Json::from("games/sepulcra/lex/slice.targets.json")),
            ])
            .to_string(),
        );
    }
    let t = match targets_arg(args) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let mut out = Json::obj([("valid", Json::from(true)), ("targets", t.to_json())]);
    if args.get("set").is_some() || args.get("document").is_some() {
        let (doc, set) = match rules_doc(tools, session, args, "balance_targets") {
            Ok(x) => x,
            Err(e) => return e,
        };
        let rules = match base_rules(&doc) {
            Ok(r) => r,
            Err(e) => return e,
        };
        match t.bind(&rules) {
            Ok(b) => {
                out.set("bound_to", Json::from(set.unwrap_or_else(|| rules.name.clone())));
                out.set("warnings", diags(&b.warnings));
                out.set("params", params_json(&rules));
            }
            Err(ds) => return err("invalid_targets", "the targets don't fit this rule set; each diagnostic says what to change", vec![("diagnostics", diags(&ds))]),
        }
    }
    ToolOutput::text(out.to_string())
}

fn sweep_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "balance_sweep", &[&["set", "document", "targets", "targets_file", "points", "grid", "seeds", "threads", "start", "detailed"]]) {
        return err("invalid_argument", m, vec![]);
    }
    let started = Instant::now();
    let t = match targets_arg(args) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let (doc, set) = match rules_doc(tools, session, args, "balance_sweep") {
        Ok(x) => x,
        Err(e) => return e,
    };
    let (points, seeds, threads) = match (points_arg(args), seeds_arg(args, &t), threads_arg(args)) {
        (Ok(p), Ok(s), Ok(th)) => (p, s, th),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return e,
    };
    let start = match int_arg(args, "start", 0, 1 << 20) {
        Ok(s) => s.unwrap_or(0) as usize,
        Err(m) => return err("invalid_argument", m, vec![]),
    };
    let detailed = crate::tools::bool_arg(args, "detailed").ok().flatten().unwrap_or(false);
    // This call's slice of the points: at most MAX_POINTS and MAX_RUNS runs.
    let per_call = MAX_POINTS.min((MAX_RUNS / (seeds.len() * t.policies.len().max(1))).max(1));
    let end = (start + per_call).min(points.len());
    if start >= points.len() {
        return err("invalid_argument", format!("'start' {start} is past the {} points", points.len()), vec![]);
    }
    let Some(grant) = Grant::take(threads) else { return busy() };
    let opts = SweepOptions { threads: grant.0, deadline: Some(started + WALL_BUDGET) };
    let res = sweep(&doc, &t, &points[start..end], &seeds, &opts);
    drop(grant);
    let secs = started.elapsed().as_secs_f64();
    // Points listed in order while they fit the byte budget; the rest come again from next_start.
    let mut done: Vec<Json> = Vec::new();
    let mut bytes = 0usize;
    let mut cut_at = None;
    for (i, p) in res.points.iter().enumerate() {
        let Some(p) = p else { continue };
        let mut j = p.to_json(detailed);
        j.set("index", Json::from(start + i));
        bytes += j.to_string().len();
        if bytes > MAX_RESPONSE_BYTES && !done.is_empty() {
            cut_at = Some(start + i);
            break;
        }
        done.push(j);
    }
    let first_unfinished = res.unfinished.first().map(|i| start + i);
    let next = [cut_at, first_unfinished].into_iter().flatten().min().or((end < points.len()).then_some(end));
    let best = res.points.iter().flatten().filter(|p| p.errors.is_empty()).fold(None, |b: Option<&specula_balance::PointResult>, p| match b {
        Some(b) if b.loss <= p.loss => Some(b),
        _ => Some(p),
    });
    let mut out = Json::obj([
        ("set", set.map_or(Json::Null, Json::from)),
        ("targets", Json::from(t.name.as_str())),
        ("seeds", Json::from(seeds.len())),
        ("points", Json::Arr(done)),
        ("best", best.map_or(Json::Null, |b| Json::Obj(b.point.clone()))),
        ("complete", Json::from(next.is_none())),
        ("next_start", Json::from(next)),
        ("runs", Json::from(res.runs_done)),
        ("threads", Json::from(opts.threads)),
        ("seconds", Json::from((secs * 1000.0).round() / 1000.0)),
        ("runs_per_s", Json::from((res.runs_done as f64 / secs.max(1e-9)).round())),
    ]);
    if !res.unfinished.is_empty() {
        out.set("note", Json::from("the wall budget ran out: points from next_start on didn't finish; call again with start: next_start"));
    }
    playtest(&mut out);
    ToolOutput::text(out.to_string())
}

fn tune_param(v: &Json) -> Result<TuneParam, String> {
    match v {
        Json::Str(id) => Ok(TuneParam::new(id)),
        Json::Obj(_) => {
            let id = v.get("param").and_then(Json::as_str).ok_or("each of 'tune' is a param id or {\"param\", \"min\"?, \"max\"?, \"step\"?}")?;
            let mut p = TuneParam::new(id);
            match (v.get("min"), v.get("max")) {
                (Some(lo), Some(hi)) => p.range = Some((lo.clone(), hi.clone())),
                (None, None) => {}
                _ => return Err(format!("'{id}': give both 'min' and 'max' to narrow the range")),
            }
            p.step = v.get("step").cloned();
            Ok(p)
        }
        _ => Err("each of 'tune' is a param id or {\"param\", \"min\"?, \"max\"?, \"step\"?}".into()),
    }
}

fn tune_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    let keys: &[&str] = &["set", "document", "targets", "targets_file", "tune", "budget", "seed", "job", "evals", "state", "threads", "validate"];
    if let Err(m) = check_keys(args, "balance_tune", &[keys]) {
        return err("invalid_argument", m, vec![]);
    }
    let started = Instant::now();
    let t = match targets_arg(args) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let (doc, set) = match rules_doc(tools, session, args, "balance_tune") {
        Ok(x) => x,
        Err(e) => return e,
    };
    let rules = match base_rules(&doc) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let rules_hash = rules.hash;
    let threads = match threads_arg(args) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let name = match string_arg(args, "job") {
        Ok(n) => n.map(str::to_owned),
        Err(m) => return err("invalid_argument", m, vec![]),
    };
    if name.as_ref().is_some_and(|n| n.is_empty() || n.len() > 64) {
        return err("invalid_argument", "'job' is a name of 1..64 characters", vec![]);
    }
    let evals_now = match int_arg(args, "evals", 1, MAX_EVALS as u64) {
        Ok(n) => n.map_or(usize::MAX, |n| n as usize),
        Err(m) => return err("invalid_argument", m, vec![]),
    };
    let targets_json = t.to_json();
    // The job: a kept one (continued), one from `state` (resumed from data), or a new one.
    let kept = name.as_ref().and_then(|n| {
        let jobs = tools.balance_jobs.0.lock().unwrap_or_else(|p| p.into_inner());
        jobs.iter().find(|j| &j.name == n).map(|j| (j.session.clone(), j.owner.clone(), j.rules_hash, j.targets.clone(), j.job.clone()))
    });
    let mut job = match (kept, args.get("state")) {
        (Some((s, owner, _, _, _)), _) if s != session.id => {
            return err("job_owned", format!("the tune job '{}' belongs to {owner}'s session; pick another name, or pass its state to resume a copy", name.unwrap_or_default()), vec![]);
        }
        (Some((_, _, h, tj, j)), _) => {
            if h != rules_hash || tj != targets_json {
                return err(
                    "job_changed",
                    "the rules or targets changed since this job started: its evaluations measured other rules; start a new job (another name)",
                    vec![("rules_hash_then", Json::from(format!("{h:016x}"))), ("rules_hash_now", Json::from(format!("{rules_hash:016x}")))],
                );
            }
            j
        }
        (None, Some(state)) => match TuneJob::from_json(state) {
            Ok(j) => {
                // It must fit these rules: the params exist, with the unit and inside the range.
                let misfit = j.space.iter().find(|s| !rules.params.iter().any(|p| p.id == s.id && p.unit == s.unit && p.min <= s.min && s.max <= p.max));
                if let Some(s) = misfit {
                    return err("invalid_state", format!("'state' tunes '{}', which these rules don't have with that unit and range", s.id), vec![("params", params_json(&rules))]);
                }
                if j.tick_hz != rules.tick_hz {
                    return err("invalid_state", format!("'state' was made at {} Hz, these rules tick at {} Hz", j.tick_hz, rules.tick_hz), vec![]);
                }
                j
            }
            Err(m) => return err("invalid_state", format!("'state' isn't a tune job: {m}"), vec![]),
        },
        (None, None) => {
            let Some(list) = args.get("tune").and_then(Json::as_array) else {
                return err("invalid_argument", "'tune' lists the params to tune: [\"runner_hp\"] or [{\"param\": \"runner_hp\", \"min\": 20, \"max\": 90, \"step\": 1}]", vec![]);
            };
            let params: Result<Vec<TuneParam>, String> = list.iter().map(tune_param).collect();
            let params = match params {
                Ok(p) => p,
                Err(m) => return err("invalid_argument", m, vec![]),
            };
            let budget = int_arg(args, "budget", 1, MAX_EVALS as u64).ok().flatten().unwrap_or(24) as usize;
            let seed = int_arg(args, "seed", 0, u64::MAX >> 11).ok().flatten().unwrap_or(1);
            match TuneJob::new(&doc, &params, TuneOptions { budget, init: None, seed }) {
                Ok(j) => j,
                Err(ds) => return err("invalid_tune", "the tune request has errors; each says what to change", vec![("diagnostics", diags(&ds))]),
            }
        }
    };
    let Some(grant) = Grant::take(threads) else { return busy() };
    let opts = SweepOptions { threads: grant.0, deadline: Some(started + WALL_BUDGET) };
    let (n, cut) = job.advance(&doc, &t, evals_now, &opts);
    let validate = crate::tools::bool_arg(args, "validate").ok().flatten().unwrap_or(job.done());
    // Held-out validation is part of the answer only when it finished inside the wall budget.
    let held = if validate && !cut && started.elapsed() < WALL_BUDGET { job.validate(&doc, &t, &opts) } else { None };
    let held_status = match (&held, validate) {
        (Some(_), _) => "done",
        (None, false) => "not_requested",
        (None, true) => "unfinished",
    };
    drop(grant);
    let best = job.best().cloned();
    let mut out = Json::obj([
        ("set", set.map_or(Json::Null, Json::from)),
        ("targets", Json::from(t.name.as_str())),
        ("evaluated_now", Json::from(n)),
        ("evaluations", Json::from(job.evals.len())),
        ("budget", Json::from(job.opts.budget)),
        ("done", Json::from(job.done())),
        ("stopped_by_wall_budget", Json::from(cut)),
        ("best", best.as_ref().map_or(Json::Null, |b| {
            let mut j = Json::obj([("params", Json::Obj(job.point(&b.x))), ("loss", Json::from((b.loss * 10_000.0).round() / 10_000.0)), ("hit", Json::from(b.hit))]);
            if let Some(r) = &b.result {
                j.set("train", r.to_json(false));
            }
            j
        })),
        ("held_out", held.as_ref().map_or(Json::Null, |h| h.to_json(false))),
        ("held_out_hit", held.as_ref().map_or(Json::Null, |h| Json::from(h.hit))),
        ("held_out_status", Json::from(held_status)),
        ("finished", Json::from(job.done() && held.is_some())),
        ("seconds", Json::from((started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0)),
        ("state", job.to_json()),
    ]);
    if !job.done() {
        out.set("next", Json::from(if name.is_some() { "call balance_tune again with the same job (and set and targets) to continue" } else { "pass 'job' to keep it in the engine, or 'state' back to resume" }));
    } else if held_status == "unfinished" {
        out.set("next", Json::from("the tuning is done but the held-out check ran out of time: call balance_tune again with the same job (or this state) and validate: true"));
    }
    playtest(&mut out);
    // Keep it (bounded; a session's own oldest job makes room; another session's never does).
    if let Some(name) = name {
        let mut jobs = tools.balance_jobs.0.lock().unwrap_or_else(|p| p.into_inner());
        jobs.retain(|j| j.name != name);
        while jobs.iter().filter(|j| j.session == session.id).count() >= MAX_JOBS_PER_SESSION {
            let i = jobs.iter().position(|j| j.session == session.id).expect("counted");
            jobs.remove(i);
        }
        if jobs.len() >= MAX_JOBS {
            out.set("keep_error", Json::from(format!("the engine keeps at most {MAX_JOBS} tune jobs and none more are yours to replace: this one wasn't kept; resume it from 'state'")));
        } else {
            jobs.push_back(Job { name, session: session.id.clone(), owner: session.client_name.clone(), rules_hash, targets: targets_json, job });
        }
    }
    ToolOutput::text(out.to_string())
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    let rules = || vec![("set", t("string", "A Lex rule set stored in the engine (lex_list)")), ("document", t("object", "An inline Lex document instead of 'set'"))];
    let targets = || vec![("targets", t("object", "A targets document (balance_targets with no arguments shows the format)")), ("targets_file", t("string", "A targets file under the engine's working directory, e.g. games/sepulcra/lex/slice.targets.json"))];
    let threads = ("threads", t("integer", "Worker threads (capped by what all agents' balance calls share); results don't depend on it"));
    vec![
        def(
            "balance_targets",
            "Designer targets as data, checked: what a game should feel like in numbers (win_rate, wave_cleared per wave, leaks \
             and kills per wave, a resource at a time (curves), time_to_kill, wave_clear_time, end_time, or any Lex expression \
             at the end), each with target, tolerance and weight; the scripted policy that plays (a build order: steps with \
             command, args, at/after_wave/when, waiting on rejection codes like not_enough_dust) and train/holdout seeds. With \
             set or document the targets are also bound to that rule set (names, waves, commands, expressions) and the \
             tunable params are listed. Errors are Lex diagnostics that say what to change. No arguments: the metric catalogue.",
            schema(targets().into_iter().chain(rules()).collect(), &[]),
        ),
        def(
            "balance_sweep",
            "Run a Lex rule set headless at param points x seeds, in parallel, and score each point against the targets: per \
             target mean, stdev, min, max, hit, loss; outcomes; the rules hash with the params folded in. Points: 'points' \
             [{param: value}] or 'grid' {param: [values] | {from, to, steps}} (values in the param's human unit), or none for \
             the rules as written. Seeds: \"train\" (default), \"holdout\" or a list. Deterministic and independent of thread \
             count. Bounded: 64 points and 16,384 runs per call and a 25 s wall budget; continue with start: next_start. \
             Results are flagged needs_human_playtest: a scripted policy isn't people.",
            schema(
                rules()
                    .into_iter()
                    .chain(targets())
                    .chain([
                        ("points", t("array", "[{param: value}, ...]")),
                        ("grid", t("object", "{param: [values] | {from, to, steps}}: every combination")),
                        ("seeds", t("string", "\"train\" (default), \"holdout\", or an array of seeds")),
                        ("start", t("integer", "First point index (next_start of an unfinished call)")),
                        ("detailed", t("boolean", "More per-point detail")),
                        threads.clone(),
                    ])
                    .collect(),
                &[],
            ),
        ),
        def(
            "balance_tune",
            "Tune Lex param items toward the targets by seeded Bayesian optimisation (a Gaussian process, its length scale \
             chosen from a fixed grid by likelihood, expected improvement over seeded candidates) on the training seeds, then \
             check the best point on the held-out seeds it never saw. 'tune': [\"param\"] or [{param, min, max, step}]. \
             Deterministic: the same rules, targets, params, budget and seed give the same evaluations on any machine. \
             Resumable: name a 'job' to keep it in the engine and call again to continue (each call is bounded by 'evals' and \
             a 25 s wall budget), or pass a returned 'state' back. Returns the best params, their train and held-out scores, \
             and the job state. Every result needs a human playtest.",
            schema(
                rules()
                    .into_iter()
                    .chain(targets())
                    .chain([
                        ("tune", t("array", "Params to tune: ids, or {param, min?, max?, step?} (human units or unit literals)")),
                        ("budget", t("integer", "Total evaluations, 1..200 (default 24); each runs every training seed")),
                        ("seed", t("integer", "The optimiser's seed (default 1)")),
                        ("job", t("string", "Keep the job under this name; call again with it to continue")),
                        ("evals", t("integer", "Evaluations this call at most")),
                        ("state", t("object", "A returned 'state' to resume from")),
                        ("validate", t("boolean", "Check the best point on the held-out seeds now (default: when done)")),
                        threads,
                    ])
                    .collect(),
                &[],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_expand_and_definitions_are_well_formed() {
        let g = Json::parse(r#"{"grid": {"a": [1, 2], "b": {"from": 0, "to": 1, "steps": 3}}}"#).unwrap();
        let points = points_arg(&g).ok().unwrap();
        assert_eq!(points.len(), 6);
        assert_eq!(points[5], vec![("a".to_owned(), Json::from(2.0)), ("b".to_owned(), Json::from(1.0))]);
        assert_eq!(points_arg(&Json::Obj(Vec::new())).ok().unwrap(), vec![Vec::new()]);
        for d in definitions() {
            assert!(d.name.starts_with("balance_") && d.name.len() <= 64);
        }
        let a = Grant::take(Some(1000)).expect("threads are free");
        assert!(a.0 >= 1 && a.0 <= thread_cap());
        // With every thread taken, the next call is told to wait (the cap holds).
        let rest = Grant::take(None);
        if a.0 + rest.as_ref().map_or(0, |g| g.0) >= thread_cap() {
            assert!(Grant::take(Some(1)).is_none());
        }
        drop((a, rest));
    }
}
