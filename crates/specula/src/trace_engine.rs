//! The engine's watchdogs and invariant monitors (docs/design/TRACE.md, "Watchdogs and
//! incidents"): engine-thread stalls per call class, frames over budget, refused writes, sight
//! packets that contradict their own pixels, a world replay spot-check when idle, GPU validation
//! messages, jobs that never finish, GPU memory over budget, and disabled modules. Each raises an
//! incident ([`crate::incidents`]) with its event.
//!
//! Every monitor has a budget: per call or per frame one comparison; the GPU and job samples twice
//! and once a second; the replay at most once a minute, only when idle, backing off when it costs.

use crate::incidents::{self, Raise};
use crate::journal::Writer;
use crate::schedule::Class;
use crate::trace_sink::{self, ENGINE, GPU, JOBS, LOGIC, SIGHT, WORLD, WRITES};
use crate::world::WorldError;
use crate::Engine;
use specula_core::trace::{self, Level, Value};
use specula_core::{trace_event, Json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Stalls shown in perf_frame.
const RECENT_STALLS: usize = 8;
/// GPU frame time and memory are sampled this often.
const GPU_SAMPLE_EVERY: Duration = Duration::from_millis(500);
/// Job counts are sampled this often; a kind in flight this long without one finishing is stuck.
const JOBS_SAMPLE_EVERY: Duration = Duration::from_secs(1);
pub const JOBS_STUCK_AFTER: Duration = Duration::from_secs(60);
/// The replay spot-check: after this long without agent calls, at most this often.
const REPLAY_IDLE: Duration = Duration::from_secs(2);
const REPLAY_EVERY: Duration = Duration::from_secs(60);
/// The replay runs a step per loop pass: entries until this much time is spent (checked every
/// [`REPLAY_CHUNK`] entries), so a caller arriving mid-check waits at most about this long.
pub const REPLAY_STEP: Duration = Duration::from_millis(2);
/// Changes (or entities hashed) per check of the clock.
const REPLAY_CHUNK: usize = 64;
/// A replay estimated (from the last one's cost per entry) to take longer than this in all is
/// skipped, and the next try waits [`REPLAY_BACKOFF`].
const REPLAY_MAX_TOTAL: Duration = Duration::from_secs(10);
const REPLAY_BACKOFF: Duration = Duration::from_secs(600);
/// The replay starts from a copy of the journal's base world, one step: only bases this small
/// (a world built in this run has an empty base; a loaded big world isn't spot-checked).
const REPLAY_BASE_ENTITIES: usize = 5_000;
/// Sight subjects checked per packet.
pub const PACKET_SUBJECTS: usize = 64;
/// Routine conflicts (claims and versions doing their job) open an incident only past this many
/// in [`SPIKE_WINDOW_US`]; other refusals open one at once.
const SPIKE_COUNT: usize = 20;
const SPIKE_WINDOW_US: u64 = 10_000_000;
const ROUTINE: [&str; 4] = ["claimed", "claims_overlap", "stale_version", "undo_conflict"];
/// The codes the write policy itself refuses with (a writer's own errors, like a missing entity,
/// are only `write.failed` at debug).
const POLICY: [&str; 11] = [
    "claimed",
    "claims_overlap",
    "stale_version",
    "undo_conflict",
    "reserved",
    "transient",
    "live_world",
    "playtest_running",
    "unknown_effect",
    "too_many_effect_params",
    "photosensitive",
];

/// Stalls past their budget while `--strict-budget` is on (the engine then fails at shutdown).
static STRICT_FAILURES: AtomicU64 = AtomicU64::new(0);

/// The watchdogs' state (one field of the engine).
pub(crate) struct Watch {
    last_request: Instant,
    stalls: VecDeque<Json>,
    gpu_sampled: Option<Instant>,
    budgets: Option<(Instant, crate::perf::Budgets)>,
    /// Per job kind: the count last sampled and since when none finished.
    jobs: BTreeMap<&'static str, (usize, Instant)>,
    jobs_sampled: Option<Instant>,
    replay: Option<ReplayCheck>,
    replay_at: Option<Instant>,
    replay_seen: Option<(u64, u64)>,
    replay_wait: Duration,
    /// The last finished check: its engine-thread time in all, its longest step, and its cost per
    /// entry (what sizes the next).
    replay_ms: Option<(f64, f64)>,
    replay_us_per_entry: Option<f64>,
    /// Whether a stall past its budget fails the run (`--strict-budget`).
    pub strict: bool,
}

impl Default for Watch {
    fn default() -> Watch {
        Watch {
            last_request: Instant::now(),
            stalls: VecDeque::new(),
            gpu_sampled: None,
            budgets: None,
            jobs: BTreeMap::new(),
            jobs_sampled: None,
            replay: None,
            replay_at: None,
            replay_seen: None,
            replay_wait: REPLAY_EVERY,
            replay_ms: None,
            replay_us_per_entry: None,
            strict: false,
        }
    }
}

impl Watch {
    /// The recent stalls, newest last, and the incidents wanting attention: for perf_frame.
    pub fn json(&self) -> Json {
        let (errors, warnings) = incidents::active_counts();
        Json::obj([
            ("recent_stalls", Json::arr(self.stalls.iter().cloned())),
            ("budgets_ms", trace_sink::settings().budgets.to_json()),
            ("strict_budget", Json::from(self.strict)),
            ("strict_failures", Json::from(STRICT_FAILURES.load(Relaxed))),
            ("incidents_active", Json::obj([("error", Json::from(errors)), ("warn", Json::from(warnings))])),
            (
                "replay_check",
                Json::obj([
                    ("running", Json::from(self.replay.is_some())),
                    ("last_total_ms", Json::from(self.replay_ms.map(|m| m.0))),
                    ("last_longest_step_ms", Json::from(self.replay_ms.map(|m| m.1))),
                ]),
            ),
        ])
    }
}

/// Stalls that failed a strict run.
pub fn strict_failures() -> u64 {
    STRICT_FAILURES.load(Relaxed)
}

pub fn class_name(c: Class) -> &'static str {
    match c {
        Class::Read => "read",
        Class::Edit => "edit",
        Class::Render => "render",
        Class::Disk => "disk",
        Class::Big => "big",
        Class::Sim => "sim",
        Class::Game => "game",
    }
}

impl Engine {
    /// Handles one agent request inside its call's context, timing it against its class budget.
    pub(crate) fn handle_traced(&mut self, request: crate::tools::Request, server: &specula_protocol::Server) {
        let ctx = request.trace;
        let class = crate::call_class(&request.command);
        let _c = ctx.enter();
        let started = Instant::now();
        self.handle(request, server);
        self.watch.last_request = Instant::now();
        self.engine_work_done(ctx.call, Some(class), "request", started.elapsed());
    }

    /// One piece of engine-thread work took `took`: past its budget it is `engine.stall`.
    pub(crate) fn engine_work_done(&mut self, call: u64, class: Option<Class>, zone: &'static str, took: Duration) {
        let ms = took.as_secs_f64() * 1000.0;
        // The class's budget first (atomics, no lock); a tool with a budget of its own is looked
        // up only past it.
        if ms <= trace_sink::budget_for(None, class) {
            return;
        }
        let rec = trace_sink::call(call);
        let budget = trace_sink::budget_for(rec.as_ref().map(|r| &*r.tool), class);
        if ms <= budget {
            return;
        }
        // Work no agent asked for (a frame step) is named by its zone.
        let tool_name: String = rec.as_ref().map_or_else(|| zone.to_owned(), |r| r.tool.to_string());
        let args = rec.as_ref().map_or_else(String::new, |r| r.args.to_string());
        let class_str = class.map_or("frame", class_name);
        // The engine's own background work (the replay check) is traced, never a strict failure.
        let strict = self.watch.strict && zone != "replay_check";
        let level = if strict { Level::Error } else { Level::Warn };
        if strict {
            STRICT_FAILURES.fetch_add(1, Relaxed);
        }
        let ms_r = (ms * 100.0).round() / 100.0;
        let stall = Json::obj([
            ("call", Json::from(call)),
            ("tool", Json::from(tool_name.as_str())),
            ("class", Json::from(class_str)),
            ("zone", Json::from(zone)),
            ("ms", Json::from(ms_r)),
            ("budget_ms", Json::from(budget)),
            ("args", Json::from(args.as_str())),
            ("frame", Json::from(trace::now().1)),
        ]);
        if self.watch.stalls.len() >= RECENT_STALLS {
            self.watch.stalls.pop_front();
        }
        self.watch.stalls.push_back(stall.clone());
        incidents::raise(Raise {
            sub: &ENGINE,
            level,
            code: "engine.stall",
            key: &tool_name,
            summary: format!("{tool_name} held the engine thread {ms_r} ms (its {class_str} budget is {budget} ms)"),
            fields: vec![
                ("tool", Value::from(tool_name.as_str())),
                ("class", Value::Str(class_str)),
                ("zone", Value::Str(zone)),
                ("ms", Value::F64(ms_r)),
                ("budget_ms", Value::F64(budget)),
                ("args", Value::from(args.as_str())),
            ],
            evidence: stall,
            call,
            agent: None,
        });
    }

    /// After a window frame: its CPU work (outside serving and present) against the frame step
    /// budget; twice a second, the GPU's frame time against `perf.toml` and GPU memory.
    pub(crate) fn trace_frame(&mut self, frame_no: u64, frame_cpu: Duration) {
        trace::set_frame(frame_no);
        self.engine_work_done(0, None, "frame", frame_cpu);
        self.trace_gpu_sample();
    }

    fn trace_gpu_sample(&mut self) {
        let now = Instant::now();
        if self.watch.gpu_sampled.is_some_and(|t| now - t < GPU_SAMPLE_EVERY) {
            return;
        }
        self.watch.gpu_sampled = Some(now);
        if self.watch.budgets.as_ref().is_none_or(|(t, _)| now - *t > Duration::from_secs(5)) {
            let kind = crate::perf::adapter_kind(self.gpu.adapter().kind);
            self.watch.budgets = Some((now, self.perf_cfg.budgets(kind)));
        }
        let Some((_, budgets)) = &self.watch.budgets else { return };
        let (gpu_budget, memory_budget) = (budgets.gpu_ms, budgets.gpu_memory_mb);
        let path = if self.windowed { specula_gpu::FramePath::Window } else { specula_gpu::FramePath::Offscreen };
        let Some(stats) = self.gpu.last_frame_stats_of(path) else { return };
        if let Some(g) = &stats.gpu {
            if self.windowed && g.total_ms > gpu_budget {
                let mut passes: Vec<(&'static str, f64)> = g.passes.iter().map(|p| (p.name, p.gpu_ms)).collect();
                passes.sort_by(|a, b| b.1.total_cmp(&a.1));
                passes.truncate(3);
                let top = passes.first().map_or("none", |p| p.0);
                let top_ms = passes.first().map_or(0.0, |p| p.1);
                // One incident per cause, not per pass: a pass that alone takes more than the
                // whole budget is that pass's cost; anything else is load or contention (other
                // programs on the GPU, a busy machine), whichever pass happens to be slowest
                // (review, e45f66c and a Sepulcra run on the iGPU: six keys for one contended run).
                let cause = if top_ms > gpu_budget { format!("pass:{top}") } else { "spread".to_owned() };
                let r = |v: f64| (v * 100.0).round() / 100.0;
                let list = Json::arr(passes.iter().map(|(n, ms)| Json::obj([("pass", Json::from(*n)), ("ms", Json::from(r(*ms)))])));
                incidents::raise(Raise {
                    sub: &ENGINE,
                    level: Level::Warn,
                    code: "frame.over_budget",
                    key: &cause,
                    summary: format!("window frame {} took {} ms on the GPU (budget {gpu_budget} ms); most in {top}", g.frame, r(g.total_ms)),
                    fields: vec![("gpu_ms", Value::F64(r(g.total_ms))), ("budget_ms", Value::F64(gpu_budget)), ("top_pass", Value::Str(top)), ("gpu_frame", Value::U64(g.frame))],
                    evidence: Json::obj([("gpu_frame", Json::from(g.frame)), ("gpu_ms", Json::from(r(g.total_ms))), ("budget_ms", Json::from(gpu_budget)), ("top_passes", list)]),
                    call: 0,
                    agent: None,
                });
            }
        }
        if let Some(max) = memory_budget {
            let mb = stats.memory.total_bytes as f64 / 1_048_576.0;
            if mb > max {
                incidents::raise(Raise {
                    sub: &GPU,
                    level: Level::Warn,
                    code: "memory.over_budget",
                    key: "gpu",
                    summary: format!("GPU memory {:.1} MB is over perf.toml's gpu_memory_mb {max}", mb),
                    fields: vec![("mb", Value::F64((mb * 10.0).round() / 10.0)), ("budget_mb", Value::F64(max))],
                    evidence: Json::obj([("mb", Json::from((mb * 10.0).round() / 10.0)), ("budget_mb", Json::from(max)), ("allocations", Json::from(stats.memory.allocations))]),
                    call: 0,
                    agent: None,
                });
            }
        }
    }

    /// Once per loop iteration (both loops): headless GPU samples, the job sample once a second,
    /// and the replay spot-check when idle.
    pub(crate) fn trace_idle_checks(&mut self) {
        trace::set_tick(self.clock.tick());
        if !self.windowed {
            self.trace_gpu_sample();
        }
        let now = Instant::now();
        if self.watch.jobs_sampled.is_none_or(|t| now - t >= JOBS_SAMPLE_EVERY) {
            self.watch.jobs_sampled = Some(now);
            let counts = [
                ("model_builds", self.library.lock().in_flight()),
                ("parked_captures", self.capture_waiters.len()),
                ("capture_flight", { let (flying, waiting) = self.captures.counts(); flying + waiting }),
                ("save_writers", usize::from(self.saves_in_flight())),
            ];
            for (kind, n) in counts {
                let entry = self.watch.jobs.entry(kind).or_insert((n, now));
                if n == 0 || n < entry.0 {
                    *entry = (n, now);
                    continue;
                }
                entry.0 = n;
                let stuck_for = now - entry.1;
                if stuck_for >= JOBS_STUCK_AFTER {
                    entry.1 = now;
                    incidents::raise(Raise {
                        sub: &JOBS,
                        level: Level::Warn,
                        code: "jobs.stuck",
                        key: kind,
                        summary: format!("{n} {kind} in flight for {} s without one finishing", stuck_for.as_secs()),
                        fields: vec![("kind", Value::Str(kind)), ("in_flight", Value::from(n)), ("for_s", Value::U64(stuck_for.as_secs()))],
                        evidence: Json::obj([("kind", Json::from(kind)), ("in_flight", Json::from(n)), ("for_s", Json::from(stuck_for.as_secs()))]),
                        call: 0,
                        agent: None,
                    });
                }
            }
        }
        self.trace_replay_check(now);
    }

    /// The idle replay spot-check, a step at a time: started after [`REPLAY_IDLE`] without agent
    /// calls (at most every [`REPLAY_EVERY`], only when the world changed, only when the journal's
    /// base is small enough to copy in one short step), then one step of at most [`REPLAY_STEP`]
    /// per loop pass whether or not calls arrive: changes applied a few at a time (a 1,000-spawn
    /// entry is split), then the replayed world hashed a few entities at a time. The live
    /// world's hash and the journal's newest entry are taken when it starts: the replay of the
    /// entries up to it must give that hash. Each step is timed like any engine-thread work (it
    /// is the engine's own background work: never a `--strict-budget` failure).
    fn trace_replay_check(&mut self, now: Instant) {
        if let Some(mut check) = self.watch.replay.take() {
            let t = Instant::now();
            let stepped: Result<bool, &'static str> = loop {
                match &mut check.phase {
                    ReplayPhase::LiveHash(hash, seen) => {
                        // The live world, hashed a slice at a time while it holds still (a change
                        // abandons the check: the next idle spell starts again).
                        if (self.world_generation, self.world.revision()) != *seen {
                            break Err("the world changed while its hash was taken");
                        }
                        if self.world.state_hash_step(hash, REPLAY_CHUNK) {
                            check.live = hash.finish();
                            let Some(run) = self.swarm.journal().replay_begin() else { break Err("the journal can't be replayed") };
                            check.phase = ReplayPhase::Replaying(Box::new(run));
                        }
                    }
                    ReplayPhase::Replaying(run) => match self.swarm.journal().replay_step(run, REPLAY_CHUNK) {
                        Ok(true) => {
                            let ReplayPhase::Replaying(run) = std::mem::replace(&mut check.phase, ReplayPhase::Done) else { unreachable!() };
                            let world = (*run).into_world();
                            let hash = world.state_hash_begin();
                            check.phase = ReplayPhase::Hashing(Box::new(world), hash);
                        }
                        Ok(false) => {}
                        Err(why) => break Err(why),
                    },
                    ReplayPhase::Hashing(world, hash) => {
                        if world.state_hash_step(hash, REPLAY_CHUNK) {
                            let h = hash.finish();
                            if let ReplayPhase::Hashing(world, _) = std::mem::replace(&mut check.phase, ReplayPhase::Done) {
                                self.drop_elsewhere(world);
                            }
                            check.replay_hash = Some(h);
                            break Ok(true);
                        }
                    }
                    ReplayPhase::Done => break Ok(true),
                }
                if t.elapsed() >= REPLAY_STEP {
                    break Ok(false);
                }
            };
            let step = t.elapsed();
            check.spent += step;
            check.longest = check.longest.max(step);
            self.engine_work_done(0, None, "replay_check", step);
            match stepped {
                Ok(false) => self.watch.replay = Some(check),
                Ok(true) => self.trace_replay_finish(check),
                Err(why) => {
                    match check.phase {
                        ReplayPhase::Replaying(run) => self.drop_elsewhere(Box::new((*run).into_world())),
                        ReplayPhase::Hashing(world, _) => self.drop_elsewhere(world),
                        _ => {}
                    }
                    // Not seen: the next idle spell tries again.
                    self.watch.replay_seen = None;
                    trace_event!(&WORLD, Debug, "world.replay_abandoned", why = why);
                }
            }
            return;
        }
        if now - self.watch.last_request < REPLAY_IDLE || self.watch.replay_at.is_some_and(|t| now - t < self.watch.replay_wait) {
            return;
        }
        if self.playtests.running() || self.nav.loading() {
            return;
        }
        let seen = (self.world_generation, self.world.revision());
        if self.watch.replay_seen == Some(seen) {
            return;
        }
        let entries = self.swarm.journal().len();
        // Sized by what the last one cost per entry.
        if let Some(us) = self.watch.replay_us_per_entry {
            if Duration::from_secs_f64(us * entries as f64 / 1e6) > REPLAY_MAX_TOTAL {
                self.watch.replay_at = Some(now);
                self.watch.replay_wait = REPLAY_BACKOFF;
                return;
            }
        }
        // The base world is copied in one step: only small ones (a world built in this run has
        // an empty base; a loaded big world isn't spot-checked).
        if self.swarm.journal().base_entities() > REPLAY_BASE_ENTITIES {
            return;
        }
        if !self.swarm.journal().replayable() {
            return;
        }
        self.watch.replay_at = Some(now);
        self.watch.replay_seen = Some(seen);
        // The journal's newest entry is taken once the live hash is done (the world held still
        // till then); both hashes are taken in steps.
        let hash = self.world.state_hash_begin();
        self.watch.replay = Some(ReplayCheck { phase: ReplayPhase::LiveHash(hash, seen), replay_hash: None, live: 0, entries, spent: Duration::ZERO, longest: Duration::ZERO, tick: self.clock.tick() });
    }

    /// The replay reached its target and was hashed: against the live world's hash when it began.
    fn trace_replay_finish(&mut self, check: ReplayCheck) {
        let Some(replay_hash) = check.replay_hash else { return };
        let ms = check.spent.as_secs_f64() * 1000.0;
        let r2 = |v: f64| (v * 100.0).round() / 100.0;
        self.watch.replay_ms = Some((r2(ms), r2(check.longest.as_secs_f64() * 1000.0)));
        self.watch.replay_us_per_entry = Some(ms * 1000.0 / check.entries.max(1) as f64);
        self.watch.replay_wait = REPLAY_EVERY;
        let (live, entries) = (check.live, check.entries);
        trace_event!(&WORLD, Debug, "world.replay_check", entries = entries, ms = ms, matches = replay_hash == live);
        if replay_hash != live {
            let hex = crate::journal::hex;
            incidents::raise(Raise {
                sub: &WORLD,
                level: Level::Error,
                code: "world.replay_mismatch",
                key: "world",
                summary: format!("replaying the journal ({entries} entries) gives state {} but the live world was {}", hex(replay_hash), hex(live)),
                fields: vec![("entries", Value::from(entries)), ("replay_hash", Value::from(hex(replay_hash))), ("live_hash", Value::from(hex(live)))],
                evidence: Json::obj([
                    ("entries", Json::from(entries)),
                    ("replay_hash", Json::from(hex(replay_hash))),
                    ("live_hash", Json::from(hex(live))),
                    ("tick", Json::from(check.tick)),
                    ("next", Json::from("world_hash {verify: true} repeats it; desync_trace narrows it to a tick")),
                ]),
                call: 0,
                agent: None,
            });
        }
    }
}

/// Where a replay spot-check is.
enum ReplayPhase {
    /// The live world's hash, taken in steps while the world holds still (its generation and
    /// revision when it began).
    LiveHash(crate::world::HashRun, (u64, u64)),
    Replaying(Box<crate::journal::ReplayRun>),
    Hashing(Box<crate::world::World>, crate::world::HashRun),
    Done,
}

/// A replay spot-check in progress.
pub(crate) struct ReplayCheck {
    phase: ReplayPhase,
    replay_hash: Option<u64>,
    /// The live world's hash when it began, and the journal's size then.
    live: u64,
    entries: usize,
    spent: Duration,
    longest: Duration,
    tick: u64,
}

// ── write policy ────────────────────────────────────────────────────────────

struct Spikes {
    by_code: BTreeMap<&'static str, VecDeque<u64>>,
}

static SPIKES: Mutex<Spikes> = Mutex::new(Spikes { by_code: BTreeMap::new() });

/// A write rule's name in events and incidents.
pub(crate) fn rule_name(r: crate::write_policy::Rule) -> &'static str {
    r.name()
}

/// The write policy allowed `w`'s write under `rule` (journal entry `seq`).
pub(crate) fn write_committed(w: Writer, rule: &'static str, seq: Option<u64>, changes: usize) {
    trace_event!(&WRITES, Debug, "write.committed", agent = w.name, rule = rule, seq = seq, changes = changes);
}

/// The write policy (or the writer's own command) refused `w`'s write under `rule`.
pub(crate) fn write_refused(w: Writer, rule: &'static str, e: &WorldError) {
    if !POLICY.contains(&e.code) {
        trace_event!(&WRITES, Debug, "write.failed", agent = w.name, rule = rule, code = e.code, message = e.message.as_str());
        return;
    }
    let path = e.details.iter().find(|(k, _)| matches!(*k, "path" | "entity" | "claimed_path")).map(|(_, v)| v.as_str().map_or_else(|| v.to_string(), str::to_owned));
    trace_event!(&WRITES, Info, "write.refused", agent = w.name, rule = rule, code = e.code, path = path.clone(), message = e.message.as_str());
    let call = trace::Ctx::current().call;
    let evidence = || {
        Json::obj([
            ("agent", Json::from(w.name)),
            ("rule", Json::from(rule)),
            ("code", Json::from(e.code)),
            ("path", Json::from(path.clone())),
            ("message", Json::from(e.message.chars().take(400).collect::<String>())),
            ("tool", Json::from(trace_sink::tool_of(call).map(|t| t.to_string()))),
        ])
    };
    if ROUTINE.contains(&e.code) {
        let now = trace::now_us();
        let spike = {
            let mut s = SPIKES.lock().unwrap_or_else(PoisonError::into_inner);
            let times = s.by_code.entry(e.code).or_default();
            times.push_back(now);
            while times.front().is_some_and(|t| now.saturating_sub(*t) > SPIKE_WINDOW_US) || times.len() > SPIKE_COUNT * 2 {
                times.pop_front();
            }
            times.len() >= SPIKE_COUNT
        };
        if spike {
            incidents::raise(Raise {
                sub: &WRITES,
                level: Level::Warn,
                code: "swarm.refusal_spike",
                key: e.code,
                summary: format!("{SPIKE_COUNT}+ writes refused with {} in 10 s: agents are fighting over the same work (claim_list, intent_list)", e.code),
                fields: vec![("code", Value::Str(e.code)), ("rule", Value::Str(rule)), ("agent", Value::from(w.name))],
                evidence: evidence(),
                call,
                agent: Some(w.name.to_owned()),
            });
        }
        return;
    }
    let key = format!("{}:{rule}", e.code);
    incidents::raise(Raise {
        sub: &WRITES,
        level: Level::Info,
        code: "write.refused",
        key: &key,
        summary: format!("{}'s write refused by the {rule} rule: {}", w.name, e.code),
        fields: vec![("code", Value::Str(e.code)), ("rule", Value::Str(rule)), ("agent", Value::from(w.name)), ("path", Value::from(path.clone()))],
        evidence: evidence(),
        call,
        agent: Some(w.name.to_owned()),
    });
}

// ── sight packets ───────────────────────────────────────────────────────────

/// What a frame actually drew, recorded where the engine submits it: the entity ids each pass
/// was given. Today the colour pass (mesh and 2D draws, as `FrameDesc` gets them); when the
/// authority lane's `sight::FrameRecord` (what each pass submitted) merges, this maps onto its
/// colour passes and goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawnRecord {
    /// Sorted, deduplicated.
    pub colour: Vec<u64>,
}

impl DrawnRecord {
    /// From what the GPU says its colour passes recorded (`Gpu::last_colour_draws`): mesh draw
    /// indices and 2D draw indices, mapped to their entities through the capture's per-draw
    /// `owners` (mesh draws first, `mesh_count` of them, then the 2D draws; 0 = no entity).
    pub fn from_recorded(owners: &[u64], mesh_count: usize, mesh: &[u32], two_d: &[u32]) -> DrawnRecord {
        let of = |i: usize| owners.get(i).copied().unwrap_or(0);
        let mut colour: Vec<u64> = mesh.iter().map(|&d| of(d as usize)).chain(two_d.iter().map(|&d| of(mesh_count + d as usize))).filter(|id| *id != 0 && !crate::sight::is_proxy_owner(*id)).collect();
        colour.sort_unstable();
        colour.dedup();
        DrawnRecord { colour }
    }

    fn drew(&self, id: u64) -> bool {
        self.colour.binary_search(&id).is_ok()
    }
}

/// What a sight packet's subjects claim that the frame contradicts (at most [`PACKET_SUBJECTS`]
/// subjects checked). The independent check is `drawn`: a subject with `visible_px > 0` that no
/// colour draw of this frame submitted was counted in the ID buffer without being drawn (the
/// terrain lane's never-drawn water: 426,000 pixels of nothing). The box checks compare the
/// ID buffer's own numbers with each other and with the projected box (`screen`), an independent
/// computation from the subject's bounds.
pub fn check_packet<'a>(subjects: impl IntoIterator<Item = &'a Json>, w: u32, h: u32, drawn: Option<&DrawnRecord>) -> Vec<Json> {
    let rect = |j: Option<&Json>| -> Option<[f64; 4]> {
        let a = j?.as_array()?;
        (a.len() == 4).then(|| [0, 1, 2, 3].map(|i| a[i].as_f64().unwrap_or(f64::NAN)))
    };
    let mut problems = Vec::new();
    for s in subjects.into_iter().take(PACKET_SUBJECTS) {
        let px = s.get("visible_px").and_then(Json::as_u64).unwrap_or(0);
        if px == 0 || s.get("visible_px_approx").and_then(Json::as_bool) == Some(true) {
            continue;
        }
        let id = s.get("id").cloned().unwrap_or(Json::Null);
        let path = s.get("path").cloned().unwrap_or(Json::Null);
        let mut bad = |what: &str, detail: String| {
            problems.push(Json::obj([("id", id.clone()), ("path", path.clone()), ("problem", Json::from(what)), ("detail", Json::from(detail))]));
        };
        if let (Some(d), Some(eid)) = (drawn, id.as_u64()) {
            if !d.drew(eid) {
                bad("pixels_not_drawn", format!("visible_px {px} but this frame's colour pass drew nothing of entity {eid}"));
                continue;
            }
        }
        let Some(exact) = rect(s.get("screen_box_exact")) else {
            bad("pixels_without_box", format!("visible_px {px} but no screen_box_exact"));
            continue;
        };
        let [x, y, bw, bh] = exact;
        if !(x >= 0.0 && y >= 0.0 && bw >= 1.0 && bh >= 1.0 && x + bw <= f64::from(w) + 0.5 && y + bh <= f64::from(h) + 0.5) {
            bad("box_outside_image", format!("screen_box_exact {exact:?} in a {w}x{h} image"));
            continue;
        }
        if (px as f64) > bw * bh + 0.5 {
            bad("more_pixels_than_box", format!("visible_px {px} in a {bw}x{bh} box"));
        }
        if let Some([sx, sy, sw, sh]) = rect(s.get("screen")) {
            // The ID buffer's pixels must lie inside what the subject's bounds project to (a pixel
            // of slack for rasterisation).
            let inside = x >= sx - 1.5 && y >= sy - 1.5 && x + bw <= sx + sw + 1.5 && y + bh <= sy + sh + 1.5;
            if !inside {
                bad("pixels_outside_projection", format!("screen_box_exact {exact:?} is not inside screen {:?}", [sx, sy, sw, sh]));
            }
        }
    }
    problems
}

/// A path for incident evidence: relative to the working directory when inside it, else
/// `<captures>/<file>` (no home or temp folders in shared evidence).
pub fn shown_path(p: &std::path::Path, placeholder: &str) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = p.strip_prefix(&cwd) {
            return rel.display().to_string().replace('\\', "/");
        }
    }
    format!("<{placeholder}>/{}", p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))
}

/// Checks a capture's packet ([`check_packet`]); problems become a `sight.packet_inconsistent`
/// incident with the packet's path as evidence.
pub(crate) fn report_packet<'a>(subjects: impl IntoIterator<Item = &'a Json>, w: u32, h: u32, drawn: Option<&DrawnRecord>, packet: Option<&std::path::Path>) {
    let problems = check_packet(subjects, w, h, drawn);
    if problems.is_empty() {
        return;
    }
    let call = trace::Ctx::current().call;
    let tool = trace_sink::tool_of(call).map_or_else(|| "capture".to_owned(), |t| t.to_string());
    let first = problems[0].get("problem").and_then(Json::as_str).unwrap_or("inconsistent").to_owned();
    let n = problems.len();
    let packet = packet.map(|p| shown_path(p, "captures"));
    incidents::raise(Raise {
        sub: &SIGHT,
        level: Level::Error,
        code: "sight.packet_inconsistent",
        key: &format!("{tool}:{first}"),
        summary: format!("{n} subject(s) of a {w}x{h} {tool} packet contradict what the frame drew ({first})"),
        fields: vec![("problems", Value::from(n)), ("first", Value::from(first.as_str())), ("packet", Value::from(packet.clone()))],
        evidence: Json::obj([
            ("image", Json::arr([Json::from(w), Json::from(h)])),
            ("packet", Json::from(packet)),
            ("problems", Json::arr(problems.into_iter().take(8))),
        ]),
        call,
        agent: None,
    });
}

// ── GPU and modules ─────────────────────────────────────────────────────────

/// Validation errors taken from the GPU during the run (captures, previews, teardown).
static GPU_ERRORS: AtomicU64 = AtomicU64::new(0);

/// How many validation errors [`gpu_message`] has seen.
pub(crate) fn gpu_errors_seen() -> u64 {
    GPU_ERRORS.load(Relaxed)
}

/// A validation message a capture (or the window's teardown) took from the GPU.
pub(crate) fn gpu_message(error: bool, id_name: &str, message: &str) {
    if error {
        GPU_ERRORS.fetch_add(1, Relaxed);
    }
    let level = if error { Level::Error } else { Level::Warn };
    let call = trace::Ctx::current().call;
    incidents::raise(Raise {
        sub: &GPU,
        level,
        code: "gpu.validation",
        key: id_name,
        summary: format!("Vulkan validation: {id_name}"),
        fields: vec![("id", Value::from(id_name)), ("message", Value::from(message))],
        evidence: Json::obj([("id", Json::from(id_name)), ("message", Json::from(message.chars().take(1000).collect::<String>()))]),
        call,
        agent: None,
    });
}

/// The module host stopped calling module `name` (a panic or a runaway): an incident, and for
/// a panic a crash report agents can read after a restart (`trace_crash`).
pub(crate) fn module_disabled(name: &str, why: &Json, tick: u64) {
    let code = why.get("code").and_then(Json::as_str).unwrap_or("disabled");
    let message = why.get("message").and_then(Json::as_str).unwrap_or_default();
    let crash = (code == "panic")
        .then(|| trace_sink::write_crash("module_panic", &format!("module '{name}': {message}"), why.get("location").and_then(Json::as_str), Json::obj([("module", Json::from(name)), ("tick", Json::from(tick)), ("why", why.clone())])))
        .flatten()
        .map(|p| shown_path(&p, "trace"));
    incidents::raise(Raise {
        sub: &LOGIC,
        level: Level::Error,
        code: "module.disabled",
        key: name,
        summary: format!("module '{name}' was disabled at tick {tick} ({code})"),
        fields: vec![("module", Value::from(name)), ("why", Value::from(code)), ("message", Value::from(message)), ("crash_report", Value::from(crash.clone()))],
        evidence: Json::obj([("module", Json::from(name)), ("tick", Json::from(tick)), ("why", why.clone()), ("crash_report", Json::from(crash))]),
        call: trace::Ctx::current().call,
        agent: None,
    });
}

/// The GPU device was lost (a capture's render, or the window's present): once per run, a crash
/// report (the engine may run on headless with every render failing) and `gpu.device_lost`.
pub(crate) fn device_lost(step: &str, message: &str) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let report = trace_sink::write_crash("device_lost", &format!("GPU device lost during {step}: {message}"), None, Json::obj([("step", Json::from(step))]));
        incidents::raise(Raise {
            sub: &GPU,
            level: Level::Error,
            code: "gpu.device_lost",
            key: "gpu",
            summary: format!("the GPU device was lost during {step}: every later render fails; restart the engine"),
            fields: vec![("step", Value::from(step)), ("message", Value::from(message))],
            evidence: Json::obj([("step", Json::from(step)), ("message", Json::from(message)), ("crash_report", Json::from(report.map(|p| shown_path(&p, "trace"))))]),
            call: trace::Ctx::current().call,
            agent: None,
        });
    });
}


/// A model build job's span on its worker (inside the call that queued it).
pub(crate) fn job_span(name: &'static str) -> trace::Span {
    trace::span(&JOBS, Level::Info, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subj(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    #[test]
    fn a_consistent_packet_has_no_problems() {
        let s = [
            subj(r#"{"id":1,"path":"/a","screen":[10,10,100,80],"screen_box_exact":[20,20,50,40],"visible_px":1500}"#),
            subj(r#"{"id":2,"path":"/b","visible_px":0}"#),
            subj(r#"{"id":3,"path":"/c","visible_px":40,"visible_px_approx":true}"#),
        ];
        assert!(check_packet(&s, 320, 240, None).is_empty());
        assert!(check_packet(&s, 320, 240, Some(&DrawnRecord::from_recorded(&[1, 1, 2, 3], 3, &[0, 2], &[0]))).is_empty());
    }

    #[test]
    fn a_wrong_packet_is_caught() {
        // The terrain lane's bug: pixels counted for a subject whose box says otherwise.
        let s = [
            subj(r#"{"id":1,"path":"/terrain","screen":[0,0,10,10],"screen_box_exact":[200,150,50,40],"visible_px":1500}"#),
            subj(r#"{"id":2,"path":"/nobox","visible_px":9}"#),
            subj(r#"{"id":3,"path":"/over","screen_box_exact":[0,0,4,4],"visible_px":17}"#),
            subj(r#"{"id":4,"path":"/off","screen_box_exact":[300,200,50,50],"visible_px":17}"#),
        ];
        let p = check_packet(&s, 320, 240, None);
        let kinds: Vec<&str> = p.iter().filter_map(|j| j.get("problem").and_then(Json::as_str)).collect();
        assert_eq!(kinds, ["pixels_outside_projection", "pixels_without_box", "more_pixels_than_box", "box_outside_image"]);
        report_packet(&s, 320, 240, None, None);
        let (found, _, _) = incidents::list(None, false, Some("sight.packet_inconsistent"), 0, 10);
        assert!(found.iter().any(|i| i.count >= 1));
    }

    /// Review probe (b893d0a, major 3): the idle replay ran a 12k-entity journal in one go (148 ms
    /// on the engine thread). It runs in steps of a few entries now, and still matches.
    #[test]
    fn a_large_journal_replays_in_steps_and_matches() {
        use crate::journal::{self, Journal, Kind};
        use crate::world::{Shape, SpawnSpec, World};
        let mut w = World::default();
        w.set_recording(crate::world::Recording::Full);
        let mut j = Journal::default();
        for i in 0..2000 {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(format!("e{i}"));
            journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Spawn, None, |w| w.spawn(s, "ann").map(|_| ())).unwrap();
        }
        let live = w.state_hash();
        let mut run = j.replay_begin().expect("replayable");
        let mut steps = 0;
        loop {
            let before = run.progress().0;
            let done = j.replay_step(&mut run, REPLAY_CHUNK).unwrap();
            assert!(run.progress().0 - before <= REPLAY_CHUNK as u64, "a step applies at most a chunk");
            steps += 1;
            if done {
                break;
            }
            assert!(steps < 10_000);
        }
        assert!(steps >= 2000 / REPLAY_CHUNK, "{steps} steps");
        assert_eq!(run.world().state_hash(), live, "the stepped replay gives the live world");
        // Review probe (e45f66c, minor 5): the final hash was one step; it is stepped too, and
        // gives the same hash.
        let mut hash = w.state_hash_begin();
        let mut hash_steps = 0;
        while !w.state_hash_step(&mut hash, REPLAY_CHUNK) {
            hash_steps += 1;
        }
        assert_eq!(hash.finish(), live);
        assert!(hash_steps >= 2000 / REPLAY_CHUNK - 1, "{hash_steps} hash steps");
        // ...and a heavy entry (a 600-spawn batch) is split across steps.
        let mut heavy_world = World::default();
        heavy_world.set_recording(crate::world::Recording::Full);
        let mut hj = Journal::default();
        journal::commit(&mut heavy_world, &mut hj, 0, Writer::new("ann", Some("ann")), Kind::Batch, None, |w| {
            for i in 0..600 {
                let mut s = SpawnSpec::new(Shape::Cube);
                s.name = Some(format!("b{i}"));
                w.spawn(s, "ann")?;
            }
            Ok(())
        })
        .unwrap();
        let mut heavy = hj.replay_begin().unwrap();
        assert!(!hj.replay_step(&mut heavy, REPLAY_CHUNK).unwrap(), "one step doesn't take a 600-change entry whole");
        assert_eq!(heavy.progress().0, 0, "still inside the entry");
        while !hj.replay_step(&mut heavy, REPLAY_CHUNK).unwrap() {}
        assert_eq!(heavy.world().state_hash(), heavy_world.state_hash());
        // A write after the start isn't part of the run (the live hash was taken then).
        let mut run2 = j.replay_begin().unwrap();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("later".into());
        journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Spawn, None, |w| w.spawn(s, "ann").map(|_| ())).unwrap();
        while !j.replay_step(&mut run2, 512).unwrap() {}
        assert_eq!(run2.world().state_hash(), live);
        // A rebased journal abandons the run.
        let mut run3 = j.replay_begin().unwrap();
        j.rebase(&w, "loaded");
        assert!(j.replay_step(&mut run3, 4).is_err());
    }

    /// Review probe (e45f66c, major 1): the record was built from the draws given to the frame,
    /// so it could never disagree with the ID buffer. A GPU render whose colour pass leaves a draw
    /// out (as it leaves out a draw whose variant isn't ready) must raise `pixels_not_drawn`.
    #[test]
    fn a_draw_the_colour_pass_left_out_is_caught_on_the_gpu() {
        use specula_gpu::{DrawItem, FrameDesc, Gpu, GpuOptions, Validation};
        if !specula_gpu::is_available() {
            eprintln!("a_draw_the_colour_pass_left_out_is_caught_on_the_gpu: SKIPPED: Vulkan is not available");
            return;
        }
        let mut gpu = match Gpu::new(&GpuOptions { validation: Validation::On, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("a_draw_the_colour_pass_left_out_is_caught_on_the_gpu: SKIPPED: {e}");
                return;
            }
        };
        let (v, i) = crate::meshes::sphere(16, 8);
        let left = gpu.upload_mesh(&v, &i).unwrap();
        // A second mesh, so the two draws are separate batches.
        let (v2, i2) = crate::meshes::sphere(12, 6);
        let right = gpu.upload_mesh(&v2, &i2).unwrap();
        let (w, h) = (160u32, 120u32);
        let target = gpu.create_target(w, h).unwrap();
        let cam = crate::sight::Camera::new(specula_core::math::Vec3::new(0.0, 0.0, 4.0), specula_core::math::Vec3::ZERO, 45.0);
        let vp = cam.view_proj(w, h).to_cols_array();
        let at = |x: f32| specula_core::math::Mat4::from_trs(specula_core::math::Vec3::new(x, 0.0, 0.0), specula_core::math::Quat::IDENTITY, specula_core::math::Vec3::ONE).to_cols_array();
        let draws = [DrawItem::new(left, at(-1.0), [0.8, 0.2, 0.2, 1.0]), DrawItem::new(right, at(1.0), [0.2, 0.8, 0.2, 1.0])];
        let frame = FrameDesc { view_proj: vp, draws: &draws, reverse_z: true, ..Default::default() };
        let owners = [101u64, 202];
        let packet = |gpu: &mut Gpu| {
            gpu.render_with_ids(target, &frame, &[1, 2]).unwrap();
            let cov = gpu.id_coverage(target, 2).unwrap();
            let (mesh, two_d) = gpu.last_colour_draws();
            let drawn = DrawnRecord::from_recorded(&owners, 2, &mesh, &two_d);
            let subjects: Vec<Json> = owners
                .iter()
                .zip(&cov.per_id)
                .map(|(id, c)| {
                    let exact = Json::arr([c.min[0], c.min[1], c.max[0] - c.min[0] + 1, c.max[1] - c.min[1] + 1].map(Json::from));
                    Json::obj([("id", Json::from(*id)), ("visible_px", Json::from(c.pixels)), ("screen_box_exact", exact)])
                })
                .collect();
            assert!(subjects.iter().all(|s| s.get("visible_px").and_then(Json::as_u64).unwrap_or(0) > 100), "both spheres counted: {subjects:?}");
            check_packet(&subjects, w, h, Some(&drawn))
        };
        assert!(packet(&mut gpu).is_empty(), "everything drawn: consistent");
        gpu.debug_skip_colour_draws(&[1]);
        let p = packet(&mut gpu);
        gpu.debug_skip_colour_draws(&[]);
        assert_eq!(p.len(), 1, "{p:?}");
        assert_eq!(p[0].get("problem").and_then(Json::as_str), Some("pixels_not_drawn"));
        assert_eq!(p[0].get("id").and_then(Json::as_u64), Some(202));
        gpu.destroy_target(target);
        let msgs: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= specula_gpu::Severity::Warning).collect();
        assert!(msgs.is_empty(), "{msgs:#?}");
    }

    #[test]
    fn packet_checks_are_bounded() {
        let one = subj(r#"{"id":2,"path":"/nobox","visible_px":9}"#);
        let many = vec![one; 1000];
        assert_eq!(check_packet(&many, 10, 10, None).len(), PACKET_SUBJECTS);
    }

    /// Review probe (b893d0a, major 4): the box checks compare the ID buffer with itself, so they
    /// can't see the terrain lane's fault: water counted 426,000 pixels while the colour pass drew
    /// nothing of it. The drawn record is independent of the ID buffer.
    #[test]
    fn a_subject_counted_but_never_drawn_is_caught() {
        let water = subj(r#"{"id":77,"path":"/terrain/water","screen":[0,0,1280,720],"screen_box_exact":[0,200,1280,520],"visible_px":426000}"#);
        let ground = subj(r#"{"id":5,"path":"/terrain/ground","screen":[0,0,1280,720],"screen_box_exact":[0,0,1280,400],"visible_px":300000}"#);
        assert!(check_packet([&water, &ground], 1280, 720, None).is_empty(), "consistent with itself");
        // The capture's owners: draws 0 and 1 are the ground (5), draw 2 the water (77), draw 3 a
        // 2D item (9). The GPU recorded draws 0 and 1 and the 2D item, not the water.
        let drawn = DrawnRecord::from_recorded(&[5, 5, 77, 9], 3, &[1, 0], &[0]);
        assert_eq!(drawn.colour, [5, 9], "built from the GPU's list, not the draws the capture gave");
        let p = check_packet([&water, &ground], 1280, 720, Some(&drawn));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].get("problem").and_then(Json::as_str), Some("pixels_not_drawn"));
        assert_eq!(p[0].get("id").and_then(Json::as_u64), Some(77));
        report_packet([&water], 1280, 720, Some(&drawn), Some(std::path::Path::new("/somewhere/private/frame_00001.json")));
        let (found, _, _) = incidents::list(None, false, Some("sight.packet_inconsistent"), 0, 50);
        let inc = found.iter().find(|i| i.key.ends_with("pixels_not_drawn")).expect("raised");
        assert_eq!(inc.evidence.get("packet").and_then(Json::as_str), Some("<captures>/frame_00001.json"), "no absolute paths in evidence");
    }

    #[test]
    fn refusals_by_policy_open_incidents_and_routine_ones_only_spike() {
        let w = Writer::new("trace-test-agent", None);
        write_refused(w, "lex", &WorldError::new("reserved", "'/lex/x' is in /lex").with("path", "/lex/x"));
        let (found, _, _) = incidents::list(None, false, Some("write.refused"), 0, 50);
        let inc = found.iter().find(|i| i.key == "reserved:lex").expect("a reserved refusal is an incident at once");
        assert_eq!(inc.evidence.get("path").and_then(Json::as_str), Some("/lex/x"));
        write_refused(w, "world", &WorldError::new("not_found", "no entity"));
        let (none, _, _) = incidents::list(None, false, Some("write.refused"), 0, 50);
        assert!(!none.iter().any(|i| i.key.starts_with("not_found")), "a writer's own error isn't the policy's");
        for _ in 0..(SPIKE_COUNT - 1) {
            write_refused(w, "world", &WorldError::new("stale_version", "v"));
        }
        let (quiet, _, _) = incidents::list(None, false, Some("swarm.refusal_spike"), 0, 50);
        assert!(!quiet.iter().any(|i| i.key == "stale_version"));
        write_refused(w, "world", &WorldError::new("stale_version", "v"));
        let (spike, _, _) = incidents::list(None, false, Some("swarm.refusal_spike"), 0, 50);
        assert!(spike.iter().any(|i| i.key == "stale_version"));
    }
}
