//! Step jobs: work that needs the engine (the GPU, the world, the journal) split into slices,
//! run between frames within a share of the engine thread (docs/design/WORKERS.md).
//!
//! A tool whose work could hold the engine thread past its class budget starts a [`StepJob`] with
//! [`Engine::step_job`] instead of doing it in one call. Each [`StepJob::step`] does one slice (one
//! check, one tile render, a few hundred journal changes); [`Engine::pump_steps`] runs slices
//! round-robin within a credit earned at a share of wall time (as the studio's renders do), so a
//! slow slice on a busy iGPU is followed by frames without step work. Small jobs finish within
//! their call: the first slices run at once, for up to [`INLINE_MS`].

use crate::jobs::{JobOwner, Reply};
use crate::schedule::Class;
use crate::Engine;
use specula_core::Json;
use specula_protocol::ToolOutput;
use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

/// Agents' step jobs queued at once, and per agent (identity key). An agent waits for its answer, so
/// one job per agent of a 64-agent swarm fits; a playtest's steps (call 0) don't count: one playtest
/// runs at a time and holds the clock while it waits (review of 998af3d: twenty agents looping
/// `world_hash {verify}` filled the old 16 slots, and a playtest's own verify steps failed).
pub const MAX_STEP_JOBS: usize = 64;
pub const MAX_STEPS_PER_OWNER: usize = 4;
/// The share of wall time step jobs may use, with a window and headless.
pub const WINDOW_SHARE: f64 = 0.25;
pub const HEADLESS_SHARE: f64 = 0.8;
/// Journal replays (`world_hash {verify}`) holding a world copy at once.
pub const MAX_REPLAYS: usize = 2;
/// Credit saved up at most (milliseconds).
const MAX_CREDIT_MS: f64 = 12.0;
/// Slices run within the call that starts a job, for up to this long (milliseconds).
pub const INLINE_MS: f64 = 2.0;
/// How often closed sessions' jobs are looked for.
const SESSION_CHECK: Duration = Duration::from_millis(250);

/// What one slice did.
pub(crate) enum Step {
    /// More slices to go.
    More,
    /// The answer.
    Done(ToolOutput),
    /// The job handed its answer on (an offloaded encode owns a clone of the reply).
    Handed,
    /// Nothing to do until a worker's job returns (its continuation runs on the engine thread,
    /// between passes): the job goes to the back like any other, the jobs behind it get their
    /// slices, and the pass ends once every queued job has said this in turn.
    Wait,
}

/// Work in slices. A slice should stay within about a millisecond of CPU or one GPU render and
/// readback.
pub(crate) trait StepJob {
    fn kind(&self) -> &'static str;
    fn step(&mut self, engine: &mut Engine, reply: &Reply) -> Step;
}

struct Entry {
    job: Box<dyn StepJob>,
    owner: JobOwner,
    session: String,
    cancel_on_close: bool,
    reply: Reply,
    deadline: Instant,
    class: Class,
    trace: specula_core::trace::Ctx,
    started: Instant,
}

/// Queued step jobs and their credit.
#[derive(Default)]
pub struct Steps {
    queue: VecDeque<Entry>,
    /// Journal replays in progress (`world_hash {verify}`): each holds a copy of the journal's
    /// base world, so at most [`MAX_REPLAYS`] at once; the rest wait their turn cheaply.
    pub(crate) replays: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    credit_ms: f64,
    last_pump: Option<Instant>,
    last_session_check: Option<Instant>,
}

impl Steps {
    pub fn busy(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn to_json(&self) -> Json {
        Json::arr(self.queue.iter().map(|e| {
            Json::obj([("kind", Json::from(e.job.kind())), ("owner", Json::from(e.owner.name.as_str())), ("s", Json::from((e.started.elapsed().as_secs_f64() * 10.0).round() / 10.0))])
        }))
    }
}

/// Where a step job comes from: its owner, session and the call (0 for a playtest's step, which
/// outlives its starter's session).
pub(crate) struct StepStart {
    pub owner: JobOwner,
    pub session: String,
    pub call: u64,
    pub reply: Reply,
    pub deadline: Instant,
    pub class: Class,
}

impl Engine {
    /// Starts `job`: its first slices run now, for up to [`INLINE_MS`] (a small job answers in its
    /// call and is never refused); one that must queue is refused (`job_busy`) past
    /// [`MAX_STEP_JOBS`] agents' jobs queued or [`MAX_STEPS_PER_OWNER`] for this agent. A
    /// playtest's step (call 0) always queues.
    pub(crate) fn step_job(&mut self, start: StepStart, mut job: Box<dyn StepJob>) {
        let StepStart { owner, session, call, reply, deadline, class } = start;
        let trace = specula_core::trace::Ctx::current();
        let started = Instant::now();
        loop {
            match job.step(self, &reply) {
                Step::Done(out) => {
                    let _ = reply.send(out);
                    return;
                }
                Step::Handed => return,
                Step::More if started.elapsed().as_secs_f64() * 1000.0 < INLINE_MS => {}
                Step::More | Step::Wait => break,
            }
        }
        let agents = self.steps.queue.iter().filter(|e| e.cancel_on_close).count();
        let mine = self.steps.queue.iter().filter(|e| e.cancel_on_close && e.owner.is(&owner)).count();
        if must_refuse(agents, mine, call) {
            let owners: BTreeSet<&str> = self.steps.queue.iter().map(|e| e.owner.name.as_str()).collect();
            let e = crate::authority::refusal(
                crate::authority::Site::JobBusy,
                format!("{} stepped jobs are queued ({MAX_STEPS_PER_OWNER} per agent, {MAX_STEP_JOBS} in all; yours: {mine}): call again when one has answered", self.steps.queue.len()),
            )
            .with("queued_by", Json::arr(owners.into_iter().map(Json::from)))
            .with("retry_after_s", 1)
            .with("retry", true);
            reply.refuse(&e);
            return;
        }
        self.steps.queue.push_back(Entry { job, owner, session, cancel_on_close: call != 0, reply, deadline, class, trace, started });
    }

    /// Runs queued slices round-robin within the credit earned at `share` of the wall time since
    /// the last pump; drops jobs past their deadline (`timed_out`) or of closed sessions.
    pub(crate) fn pump_steps(&mut self, share: f64, server: &specula_protocol::Server) {
        let now = Instant::now();
        let earned = self.steps.last_pump.map_or(MAX_CREDIT_MS, |t| (now - t).as_secs_f64() * 1000.0 * share);
        self.steps.last_pump = Some(now);
        if self.steps.queue.is_empty() {
            self.steps.credit_ms = MAX_CREDIT_MS;
            return;
        }
        if self.steps.last_session_check.is_none_or(|t| now - t >= SESSION_CHECK) {
            self.steps.last_session_check = Some(now);
            let alive = self.open_session_ids(server.all_sessions());
            for e in std::mem::take(&mut self.steps.queue) {
                if e.cancel_on_close && !alive.contains(&e.session) {
                    let _ = e.reply.send(stopped("session_closed", &format!("the session closed; its {} was dropped", e.job.kind())));
                } else {
                    self.steps.queue.push_back(e);
                }
            }
        }
        self.steps.credit_ms = (self.steps.credit_ms + earned).min(MAX_CREDIT_MS);
        self.run_steps();
    }

    /// The queued jobs' slices for this pass, within the credit earned.
    pub(crate) fn run_steps(&mut self) {
        // Jobs in a row that had nothing to do until a worker returns: when that is all of
        // them, going round again would only spin.
        let mut waited = 0;
        while self.steps.credit_ms > 0.0 {
            let Some(mut e) = self.steps.queue.pop_front() else { break };
            if Instant::now() > e.deadline {
                let _ = e.reply.send(stopped("timed_out", &format!("the {} ran out of its caller's time; ask for less (fewer subjects, rows or frames) or call again when the engine is less busy", e.job.kind())));
                continue;
            }
            let _c = e.trace.enter();
            let t = Instant::now();
            let step = {
                // (A zone of its own, named for the job's kind: a slow frame says which job's
                // step it was, where its time counted as the frame's own.)
                let _z = specula_core::profile::zone(e.job.kind());
                e.job.step(self, &e.reply)
            };
            let took = t.elapsed();
            self.steps.credit_ms -= took.as_secs_f64() * 1000.0;
            self.engine_work_done(e.trace.call, Some(e.class), "step_job", took);
            match step {
                Step::More => {
                    waited = 0;
                    self.steps.queue.push_back(e)
                }
                Step::Done(out) => {
                    waited = 0;
                    let _ = e.reply.send(out);
                }
                Step::Handed => waited = 0,
                // It waits at the back: the jobs behind it are not held up by it (they were:
                // review of m5-framezones), and their order among themselves is kept.
                Step::Wait => {
                    self.steps.queue.push_back(e);
                    waited += 1;
                    if waited >= self.steps.queue.len() {
                        break;
                    }
                }
            }
        }
    }

    pub(crate) fn steps_busy(&self) -> bool {
        self.steps.busy()
    }
}

/// Whether a job that must queue is refused: an agent's past its slots; a playtest's (call 0)
/// never.
fn must_refuse(agents_queued: usize, mine: usize, call: u64) -> bool {
    call != 0 && (agents_queued >= MAX_STEP_JOBS || mine >= MAX_STEPS_PER_OWNER)
}

fn stopped(code: &str, message: &str) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message))]).to_string())
}

#[cfg(test)]
mod tests {
    /// A job that waits `waits` times for something a worker owes it, then is done; and one
    /// that counts its slices.
    struct Waits {
        left: usize,
    }

    impl StepJob for Waits {
        fn kind(&self) -> &'static str {
            "waits"
        }

        fn step(&mut self, _: &mut Engine, _: &Reply) -> Step {
            if self.left == 0 {
                return Step::Done(ToolOutput::text("{}"));
            }
            self.left -= 1;
            Step::Wait
        }
    }

    struct Counts {
        slices: std::rc::Rc<std::cell::Cell<usize>>,
        upto: usize,
    }

    impl StepJob for Counts {
        fn kind(&self) -> &'static str {
            "counts"
        }

        fn step(&mut self, _: &mut Engine, _: &Reply) -> Step {
            self.slices.set(self.slices.get() + 1);
            // (Past the slices a call runs inline, so that it queues.)
            std::thread::sleep(Duration::from_millis(1));
            if self.slices.get() >= self.upto {
                Step::Done(ToolOutput::text("{}"))
            } else {
                Step::More
            }
        }
    }

    /// Review of m5-framezones: a job waiting for a worker (a filmstrip for its checks) went
    /// back to the FRONT of the queue and ended the pass, so every job behind it stood still
    /// until the worker returned. It waits at the back; the others get their slices; and a
    /// pass in which every queued job waits ends without spinning.
    #[test]
    fn a_waiting_job_does_not_hold_up_the_jobs_behind_it() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let start = |name: &'static str| {
            let (reply, answer) = crate::jobs::channel();
            let owner = crate::jobs::JobOwner { name: name.into(), key: name.into() };
            (StepStart { owner, session: name.into(), call: 0, reply, deadline: Instant::now() + Duration::from_secs(120), class: crate::schedule::Class::Render }, answer)
        };
        // The waiting job first, then a job with work to do.
        let (s, waiting_answer) = start("film");
        e.step_job(s, Box::new(Waits { left: 1_000_000 }));
        let slices = std::rc::Rc::new(std::cell::Cell::new(0));
        let (s, counted_answer) = start("other");
        e.step_job(s, Box::new(Counts { slices: std::rc::Rc::clone(&slices), upto: 40 }));
        assert_eq!(e.steps.queue.len(), 2, "both are queued");
        let inline = slices.get();
        // Passes while the first job waits: the second job runs to its end.
        let give_up = Instant::now() + Duration::from_secs(60);
        while counted_answer.recv_timeout(Duration::from_millis(1)).is_err() {
            assert!(Instant::now() < give_up, "the job behind a waiting one never finished ({} slices, {inline} of them in its own call)", slices.get());
            e.steps.credit_ms = MAX_CREDIT_MS;
            e.run_steps();
        }
        assert_eq!(slices.get(), 40);
        // Only the waiting job is left: a pass asks it once and ends (it would spin until its
        // credit ran out otherwise).
        assert_eq!(e.steps.queue.len(), 1);
        let Some(Entry { job, .. }) = e.steps.queue.front() else { unreachable!() };
        assert_eq!(job.kind(), "waits");
        e.steps.credit_ms = MAX_CREDIT_MS;
        let before = e.steps.credit_ms;
        let asked = Instant::now();
        e.run_steps();
        assert_eq!(e.steps.queue.len(), 1, "still waiting, still queued");
        assert!(e.steps.credit_ms > before - 1.0 && asked.elapsed() < Duration::from_secs(5), "the pass spun on a job that only waits");
        drop(waiting_answer);
    }

    use super::*;

    /// Review of 998af3d (major 1): twenty agents looping world_hash {verify} filled the slots and
    /// a playtest's own verify steps all failed with job_busy. A playtest's step always queues; a
    /// swarm of 64 agents, one call each, fits.
    #[test]
    fn playtest_steps_always_queue_and_a_swarm_fits() {
        assert!(!must_refuse(MAX_STEP_JOBS + 100, MAX_STEPS_PER_OWNER + 5, 0));
        assert!(!must_refuse(63, 0, 7), "the 64th agent's job queues");
        assert!(must_refuse(MAX_STEP_JOBS, 0, 7));
        assert!(must_refuse(3, MAX_STEPS_PER_OWNER, 7));
    }
}

