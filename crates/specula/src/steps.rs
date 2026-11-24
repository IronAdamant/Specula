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
    /// Whether it has a render on the GPU that a step lands (and answers [`Step::Wait`] for
    /// until the GPU is done). While a queued job does, the window looks at the step jobs about
    /// every millisecond within its serve wait, where it otherwise looks once a frame
    /// (`schedule`, "Step jobs waiting on the GPU"; light review 4: one render landed a frame).
    fn on_gpu(&self) -> bool {
        false
    }
    /// Called once, on every way a job leaves the queue ([`Ended`]): what it holds of the
    /// engine's (a render target, a pin on a build) goes back here, whether it answered or was
    /// dropped. Its answer, if any, is sent after this.
    fn on_end(&mut self, _engine: &mut Engine, _how: Ended) {}
}

/// How a step job left the queue (its [`StepJob::on_end`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// It answered (`Step::Done`) or handed its answer on (`Step::Handed`).
    Answered,
    /// It had to queue and the queue was full (`job_busy`), after its first slices ran.
    Refused,
    /// Past its deadline (`timed_out`).
    TimedOut,
    /// Its session closed (`session_closed`; never a kept job).
    SessionClosed,
    /// The engine is stopping (`shutting_down`).
    ShuttingDown,
}

struct Entry {
    job: Box<dyn StepJob>,
    owner: JobOwner,
    session: String,
    cancel_on_close: bool,
    /// Counted as its agent's but kept when its session closes ([`Engine::step_job_kept`]).
    kept: bool,
    reply: Reply,
    deadline: Instant,
    class: Class,
    trace: specula_core::trace::Ctx,
    started: Instant,
    /// Its last step landed its render and it has more to do (the pass ran out of credit before
    /// it sent the next): pulses are still for it ([`Steps::gpu_waiting`]).
    landed: bool,
}

impl Entry {
    /// A job the window's pulses are for: its render is on the GPU, or it has just landed one
    /// and has more to do.
    fn pulsed_for(&self) -> bool {
        self.landed || self.job.on_gpu()
    }
}

/// Queued step jobs and their credit.
#[derive(Default)]
pub struct Steps {
    queue: VecDeque<Entry>,
    /// Render targets whose job ended while their render was still on the GPU: each goes back
    /// to the engine's target cache once a poll finds its render done ([`Engine::land_orphans`]),
    /// never waited for.
    pub(crate) orphans: Vec<((u32, u32), specula_gpu::TargetId)>,
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

    /// Whether a queued job has a render on the GPU ([`StepJob::on_gpu`]), or has just landed one
    /// and has more to do: the jobs the window's pulses are for (`schedule`, "Step jobs waiting
    /// on the GPU").
    pub fn gpu_waiting(&self) -> bool {
        self.queue.iter().any(Entry::pulsed_for)
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
    pub(crate) fn step_job(&mut self, start: StepStart, job: Box<dyn StepJob>) {
        self.step_job_as(start, job, false);
    }

    /// [`Engine::step_job`] for a job whose answer is kept for its owner's identity, which
    /// outlives a connection (`sprite_bake`): counted as the agent's job, never dropped when
    /// the session that started it closes (its own deadline still ends it).
    pub(crate) fn step_job_kept(&mut self, start: StepStart, job: Box<dyn StepJob>) {
        self.step_job_as(start, job, true);
    }

    fn step_job_as(&mut self, start: StepStart, mut job: Box<dyn StepJob>, kept: bool) {
        let StepStart { owner, session, call, reply, deadline, class } = start;
        let trace = specula_core::trace::Ctx::current();
        let started = Instant::now();
        loop {
            match job.step(self, &reply) {
                Step::Done(out) => {
                    job.on_end(self, Ended::Answered);
                    let _ = reply.send(out);
                    return;
                }
                Step::Handed => {
                    job.on_end(self, Ended::Answered);
                    return;
                }
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
            job.on_end(self, Ended::Refused);
            reply.refuse(&e);
            return;
        }
        self.steps.queue.push_back(Entry { job, owner, session, cancel_on_close: call != 0, kept, reply, deadline, class, trace, started, landed: false });
    }

    /// Runs queued slices round-robin within the credit earned at `share` of the wall time since
    /// the last pump; drops jobs past their deadline (`timed_out`) or of closed sessions.
    /// `leave_pulsed`: leaves alone the jobs the window's pulses are for (the frame's own pump,
    /// after a window that pulsed: `Schedule::frame_pump`). Answers whether a job did more than
    /// wait.
    pub(crate) fn pump_steps(&mut self, share: f64, server: &specula_protocol::Server, leave_pulsed: bool) -> bool {
        let now = Instant::now();
        let earned = self.steps.last_pump.map_or(MAX_CREDIT_MS, |t| (now - t).as_secs_f64() * 1000.0 * share);
        self.steps.last_pump = Some(now);
        if self.steps.queue.is_empty() {
            self.steps.credit_ms = MAX_CREDIT_MS;
            return false;
        }
        if self.steps.last_session_check.is_none_or(|t| now - t >= SESSION_CHECK) {
            self.steps.last_session_check = Some(now);
            let alive = self.open_session_ids(server.all_sessions());
            self.drop_closed_steps(&alive);
        }
        self.steps.credit_ms = (self.steps.credit_ms + earned).min(MAX_CREDIT_MS);
        self.land_orphans();
        self.run_steps(leave_pulsed)
    }

    /// Drops the queued jobs of sessions not in `alive` (`session_closed`), whatever they were
    /// doing: one that waits ([`Step::Wait`]) is dropped as one with more to do is.
    pub(crate) fn drop_closed_steps(&mut self, alive: &BTreeSet<String>) {
        for mut e in std::mem::take(&mut self.steps.queue) {
            if e.cancel_on_close && !e.kept && !alive.contains(&e.session) {
                e.job.on_end(self, Ended::SessionClosed);
                let _ = e.reply.send(stopped("session_closed", &format!("the session closed; its {} was dropped", e.job.kind())));
            } else {
                self.steps.queue.push_back(e);
            }
        }
    }

    /// The queued jobs' slices for this pass, within the credit earned (`leave_pulsed`: see
    /// [`Engine::pump_steps`]). Answers whether a job did more than wait.
    pub(crate) fn run_steps(&mut self, leave_pulsed: bool) -> bool {
        // Jobs in a row that had nothing to do until a worker returns (or were left alone):
        // when that is all of them, going round again would only spin.
        let mut waited = 0;
        let mut worked = false;
        while self.steps.credit_ms > 0.0 {
            let Some(mut e) = self.steps.queue.pop_front() else { break };
            if Instant::now() > e.deadline {
                e.job.on_end(self, Ended::TimedOut);
                let _ = e.reply.send(stopped("timed_out", &format!("the {} ran out of its caller's time; ask for less (fewer subjects, rows or frames) or call again when the engine is less busy", e.job.kind())));
                continue;
            }
            if leave_pulsed && e.pulsed_for() {
                self.steps.queue.push_back(e);
                waited += 1;
                if waited >= self.steps.queue.len() {
                    break;
                }
                continue;
            }
            let _c = e.trace.enter();
            let flying = e.job.on_gpu();
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
            let landed = flying && !e.job.on_gpu() && !matches!(step, Step::Wait);
            if landed {
                // What landing a render cost, from its step's start to its end (LIGHT.md, the
                // fifth round's measures).
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "step.landed", kind = e.job.kind(), ms = took.as_secs_f64() * 1000.0);
            }
            e.landed = landed && matches!(step, Step::More);
            worked |= !matches!(step, Step::Wait);
            match step {
                Step::More => {
                    waited = 0;
                    self.steps.queue.push_back(e)
                }
                Step::Done(out) => {
                    waited = 0;
                    e.job.on_end(self, Ended::Answered);
                    let _ = e.reply.send(out);
                }
                Step::Handed => {
                    waited = 0;
                    e.job.on_end(self, Ended::Answered);
                }
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
        worked
    }

    pub(crate) fn steps_busy(&self) -> bool {
        self.steps.busy()
    }

    /// The engine is stopping: every queued job ends ([`Ended::ShuttingDown`]) and is answered
    /// `shutting_down`; a target whose render is still on the GPU is landed (waited for: the
    /// engine is stopping, and the device goes next).
    pub(crate) fn steps_shutting_down(&mut self) {
        for mut e in std::mem::take(&mut self.steps.queue) {
            e.job.on_end(self, Ended::ShuttingDown);
            let _ = e.reply.send(stopped("shutting_down", &format!("the engine is shutting down; the {} was stopped before it answered (what it had already committed stays: project_list says)", e.job.kind())));
        }
        if !self.steps.orphans.is_empty() {
            let _ = self.gpu.settle_offscreen(true);
            self.land_orphans();
        }
    }

    /// A render target whose job ended with its render on the GPU: back to the engine's target
    /// cache once that render is done ([`Engine::land_orphans`]).
    pub(crate) fn orphan_target(&mut self, size: (u32, u32), target: specula_gpu::TargetId) {
        self.steps.orphans.push((size, target));
    }

    /// Orphaned targets whose render is done go back to the target cache (or are destroyed when
    /// it is full); one still rendering is polled, never waited for.
    pub(crate) fn land_orphans(&mut self) {
        if self.steps.orphans.is_empty() {
            return;
        }
        for (size, t) in std::mem::take(&mut self.steps.orphans) {
            if self.gpu.offscreen_pending_target() == Some(t) {
                match self.gpu.settle_offscreen_within(Some(Duration::ZERO)) {
                    Ok(false) => {
                        self.steps.orphans.push((size, t));
                        continue;
                    }
                    // Done, or failed (the device's error is the next render's to say).
                    Ok(true) | Err(_) => {}
                }
            }
            self.give_back_target(size, t);
        }
    }

    /// A target an offscreen job held alone goes back to the engine's cache of offscreen targets,
    /// or is destroyed when the cache is full.
    pub(crate) fn give_back_target(&mut self, size: (u32, u32), t: specula_gpu::TargetId) {
        if self.targets.len() < crate::MAX_CACHED_TARGETS {
            self.targets.push((size, t));
        } else {
            self.gpu.destroy_target(t);
        }
    }

    /// Tests: every queued job is past its deadline (the next pass drops them all).
    #[cfg(test)]
    pub(crate) fn steps_expire_all(&mut self) {
        let past = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
        for e in &mut self.steps.queue {
            e.deadline = past;
        }
    }

    /// Tests: how many jobs are queued.
    #[cfg(test)]
    pub(crate) fn steps_queued(&self) -> usize {
        self.steps.queue.len()
    }

    /// Tests: one pass of the queue with a full credit, orphaned targets landed first (as
    /// [`Engine::pump_steps`] does).
    #[cfg(test)]
    pub(crate) fn steps_pass(&mut self) -> bool {
        self.steps.credit_ms = MAX_CREDIT_MS;
        self.land_orphans();
        self.run_steps(false)
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
            e.run_steps(false);
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
        e.run_steps(false);
        assert_eq!(e.steps.queue.len(), 1, "still waiting, still queued");
        assert!(e.steps.credit_ms > before - 1.0 && asked.elapsed() < Duration::from_secs(5), "the pass spun on a job that only waits");
        drop(waiting_answer);
    }

    use super::*;

    /// A job with a render on the GPU for `looks` looks, then done.
    struct Renders {
        looks: usize,
    }

    impl StepJob for Renders {
        fn kind(&self) -> &'static str {
            "renders"
        }

        fn step(&mut self, _: &mut Engine, _: &Reply) -> Step {
            if self.looks == 0 {
                return Step::Done(ToolOutput::text("{}"));
            }
            self.looks -= 1;
            Step::Wait
        }

        fn on_gpu(&self) -> bool {
            self.looks > 0
        }
    }

    /// Light review 4: the window pulses (looks at the step jobs about every millisecond) only
    /// while a queued job has a render on the GPU: not for a job that waits for a worker, and
    /// not once the render has landed.
    #[test]
    fn only_a_job_with_a_render_on_the_gpu_asks_for_pulses() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let start = |name: &'static str| {
            let (reply, answer) = crate::jobs::channel();
            let owner = crate::jobs::JobOwner { name: name.into(), key: name.into() };
            (StepStart { owner, session: name.into(), call: 0, reply, deadline: Instant::now() + Duration::from_secs(120), class: crate::schedule::Class::Render }, answer)
        };
        assert!(!e.steps.gpu_waiting(), "nothing queued");
        let (s, _worker) = start("film");
        e.step_job(s, Box::new(Waits { left: 1_000_000 }));
        assert!(e.steps.busy() && !e.steps.gpu_waiting(), "a job waiting for a worker asks for none");
        let (s, rendered) = start("light");
        e.step_job(s, Box::new(Renders { looks: 3 }));
        assert!(e.steps.gpu_waiting(), "a job with its render on the GPU does");
        let give_up = Instant::now() + Duration::from_secs(60);
        while rendered.try_recv().is_err() {
            assert!(Instant::now() < give_up, "the render never landed");
            e.steps.credit_ms = MAX_CREDIT_MS;
            e.run_steps(false);
        }
        assert!(e.steps.busy() && !e.steps.gpu_waiting(), "landed and answered: no more pulses");
    }

    /// A job with renders as `perf_light`'s: a step that finds its render on the GPU waits the
    /// first time and lands it the next; the step after a landing sends the next render.
    struct Lands {
        renders: usize,
        flying: bool,
        looked: bool,
        steps: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl StepJob for Lands {
        fn kind(&self) -> &'static str {
            "lands"
        }

        fn step(&mut self, _: &mut Engine, _: &Reply) -> Step {
            self.steps.set(self.steps.get() + 1);
            if self.flying {
                if !std::mem::replace(&mut self.looked, true) {
                    return Step::Wait;
                }
                (self.flying, self.looked) = (false, false);
                self.renders -= 1;
                return if self.renders == 0 { Step::Done(ToolOutput::text("{}")) } else { Step::More };
            }
            self.flying = true;
            Step::More
        }

        fn on_gpu(&self) -> bool {
            self.flying
        }
    }

    /// Light review 5: the engine's half of the frame's own pump. A job that has just landed a
    /// render and has more to do (the pass ran out of credit before it sent the next) is still
    /// one the pulses are for, or the window would stop looking at it until the frame's pump; and
    /// a pump asked to leave such jobs alone (the frame's own, after a window that pulsed) steps
    /// none of them, whether its render is on the GPU or just landed, and says it did nothing.
    #[test]
    fn a_landed_job_with_more_to_do_is_pulsed_for_and_left_alone_by_the_frame_after_pulses() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let (reply, _answer) = crate::jobs::channel();
        let owner = crate::jobs::JobOwner { name: "light".into(), key: "light".into() };
        let steps = std::rc::Rc::new(std::cell::Cell::new(0));
        let start = StepStart { owner, session: "light".into(), call: 0, reply, deadline: Instant::now() + Duration::from_secs(120), class: crate::schedule::Class::Render };
        e.step_job(start, Box::new(Lands { renders: 3, flying: false, looked: false, steps: std::rc::Rc::clone(&steps) }));
        let pass = |e: &mut Engine, credit: f64, leave: bool| {
            e.steps.credit_ms = credit;
            e.run_steps(leave)
        };
        // Its call sent the first render and (within its inline time, or in a pass after it)
        // found it on the GPU.
        if steps.get() == 1 {
            pass(&mut e, MAX_CREDIT_MS, false);
        }
        assert_eq!(steps.get(), 2);
        assert!(e.steps.gpu_waiting());
        // Left alone while its render is on the GPU.
        assert!(!pass(&mut e, MAX_CREDIT_MS, true) && steps.get() == 2, "a pump that leaves pulsed jobs alone stepped one");
        // Landed, with the credit spent by the landing: it has more to do, and pulses are for it.
        assert!(pass(&mut e, 1e-9, false) && steps.get() == 3, "the landing is work");
        assert!(e.steps.gpu_waiting(), "a job that landed and has more to do stopped the pulses");
        assert!(!pass(&mut e, MAX_CREDIT_MS, true) && steps.get() == 3, "a pump that leaves pulsed jobs alone stepped a landed one");
        // Its next step sends the next render; a step finding it on the GPU waits.
        assert!(pass(&mut e, MAX_CREDIT_MS, false) && steps.get() == 5);
        assert!(e.steps.gpu_waiting());
    }

    /// A job that records how it ended, after `waits` waits (or for ever).
    struct Ends {
        how: std::rc::Rc<std::cell::RefCell<Vec<Ended>>>,
        waits: Option<usize>,
    }

    impl StepJob for Ends {
        fn kind(&self) -> &'static str {
            "ends"
        }

        fn step(&mut self, _: &mut Engine, _: &Reply) -> Step {
            match &mut self.waits {
                Some(0) => Step::Done(ToolOutput::text("{}")),
                Some(n) => {
                    *n -= 1;
                    Step::Wait
                }
                None => Step::Wait,
            }
        }

        fn on_end(&mut self, _: &mut Engine, how: Ended) {
            self.how.borrow_mut().push(how);
        }
    }

    /// Review 1 of the sprite bake, finding 3: a job dropped by the queue gave back nothing it
    /// held. Every way a job leaves the queue calls its end hook once, before its answer:
    /// answered (in its call, or in a pass), refused for a full queue, past its deadline, its
    /// session closed, the engine stopping. A kept job is never dropped for its session.
    #[test]
    fn every_way_a_job_leaves_the_queue_calls_its_end_once() {
        let Some(mut e) = crate::workers::charge_tests::engine() else { return };
        let how = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let start = |name: &str, call: u64| {
            let (reply, answer) = crate::jobs::channel();
            let owner = crate::jobs::JobOwner { name: name.into(), key: name.into() };
            (StepStart { owner, session: name.into(), call, reply, deadline: Instant::now() + Duration::from_secs(120), class: crate::schedule::Class::Render }, answer)
        };
        let ends = |waits| Box::new(Ends { how: std::rc::Rc::clone(&how), waits });
        let code = |a: &std::sync::mpsc::Receiver<ToolOutput>| a.try_recv().map(|o| format!("{o:?}")).unwrap_or_default();
        // Answered in its call.
        let (s, a) = start("ann", 1);
        e.step_job(s, ends(Some(0)));
        assert_eq!(*how.borrow(), [Ended::Answered]);
        assert!(!code(&a).is_empty());
        // Answered in a pass.
        let (s, _a) = start("ann", 1);
        e.step_job(s, ends(Some(3)));
        while how.borrow().len() < 2 {
            e.steps_pass();
        }
        assert_eq!(how.borrow()[1], Ended::Answered);
        // Refused: the owner's slots full.
        let mut held = Vec::new();
        for _ in 0..MAX_STEPS_PER_OWNER {
            let (s, a) = start("bob", 2);
            e.step_job(s, ends(None));
            held.push(a);
        }
        let (s, a) = start("bob", 2);
        e.step_job(s, ends(None));
        assert_eq!(how.borrow()[2], Ended::Refused);
        assert!(code(&a).contains("job_busy"));
        // Past its deadline: every one of bob's.
        e.steps_expire_all();
        e.steps_pass();
        assert_eq!(how.borrow()[3..], [Ended::TimedOut; MAX_STEPS_PER_OWNER]);
        assert!(held.iter().all(|a| code(a).contains("timed_out")));
        // Its session closed: a plain job ends so; a kept one stays queued.
        let (s, a) = start("cy", 3);
        e.step_job(s, ends(None));
        let (s, kept) = start("cy", 3);
        e.step_job_kept(s, ends(None));
        e.drop_closed_steps(&BTreeSet::new());
        assert_eq!(*how.borrow().last().unwrap(), Ended::SessionClosed);
        assert!(code(&a).contains("session_closed") && e.steps_queued() == 1);
        // The engine stopping: the kept one too.
        e.steps_shutting_down();
        assert_eq!(*how.borrow().last().unwrap(), Ended::ShuttingDown);
        assert!(code(&kept).contains("shutting_down") && e.steps_queued() == 0);
        assert_eq!(how.borrow().len(), 3 + MAX_STEPS_PER_OWNER + 2, "each job's end was called once: {:?}", how.borrow());
    }

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

