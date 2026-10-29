//! `world_hash {verify: true}` as a step job (docs/design/WORKERS.md, C6): the journal replayed
//! from its base a few hundred changes per step, then the replayed world hashed a slice at a
//! time, like the trace's idle replay check. A whole replay in one call took 150 ms of the engine
//! thread (class C, 2026-10-29).
//!
//! The answer's other fields, the live hash and the journal entry the replay must reach are taken
//! when the call starts: later edits don't change what is verified (the replay stops at that
//! entry), but a journal rebase or eviction meanwhile ends the check (`replayable: false`, with
//! why).

use super::*;
use crate::steps::{Step, StepJob, StepStart};

/// Changes applied (or entities hashed) per check of the clock.
const CHUNK: usize = 64;
/// One step's time.
const STEP: Duration = Duration::from_millis(1);

enum Phase {
    /// Waiting for a replay slot (each replay holds a copy of the base world): where to stop.
    Waiting((u64, u64)),
    Replaying(Box<journal::ReplayRun>),
    Hashing(Box<World>, crate::world::HashRun),
}

struct Verify {
    /// The answer so far (everything but `verify`).
    out: Json,
    live: u64,
    entries: usize,
    phase: Option<Phase>,
    spent: Duration,
    steps: u64,
    /// Its replay slot, given back when the job ends however it ends.
    slot: Option<ReplaySlot>,
}

/// One of the [`crate::steps::MAX_REPLAYS`] replay slots, held while a replay runs.
struct ReplaySlot(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl ReplaySlot {
    fn take(count: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Option<ReplaySlot> {
        use std::sync::atomic::Ordering::Relaxed;
        let mut n = count.load(Relaxed);
        while n < crate::steps::MAX_REPLAYS {
            match count.compare_exchange(n, n + 1, Relaxed, Relaxed) {
                Ok(_) => return Some(ReplaySlot(std::sync::Arc::clone(count))),
                Err(now) => n = now,
            }
        }
        None
    }
}

impl Drop for ReplaySlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Verify {
    fn answer(&mut self, verify: Json) -> ToolOutput {
        let mut out = std::mem::replace(&mut self.out, Json::Null);
        out.set("verify", verify);
        ToolOutput::text(out.to_string())
    }
}

impl StepJob for Verify {
    fn kind(&self) -> &'static str {
        "world_hash verify"
    }

    fn step(&mut self, e: &mut Engine, _: &jobs::Reply) -> Step {
        let t = Instant::now();
        let stepped: Result<bool, &'static str> = loop {
            match self.phase.as_mut() {
                Some(Phase::Waiting(point)) => {
                    let point = *point;
                    let Some(slot) = ReplaySlot::take(&e.steps.replays) else { break Ok(false) };
                    self.slot = Some(slot);
                    match e.swarm.journal().replay_begin_at(point) {
                        Some(run) => self.phase = Some(Phase::Replaying(Box::new(run))),
                        None => break Err("older journal entries were dropped (memory limit)"),
                    }
                }
                Some(Phase::Replaying(run)) => match e.swarm.journal().replay_step(run, CHUNK) {
                    Ok(true) => {
                        let Some(Phase::Replaying(run)) = self.phase.take() else { unreachable!() };
                        let world = (*run).into_world();
                        let hash = world.state_hash_begin();
                        self.phase = Some(Phase::Hashing(Box::new(world), hash));
                    }
                    Ok(false) => {}
                    Err(why) => break Err(why),
                },
                Some(Phase::Hashing(world, hash)) => {
                    if world.state_hash_step(hash, CHUNK) {
                        break Ok(true);
                    }
                }
                None => break Err("the check lost its state"),
            }
            if t.elapsed() >= STEP {
                break Ok(false);
            }
        };
        self.spent += t.elapsed();
        self.steps += 1;
        match stepped {
            Ok(false) => Step::More,
            Ok(true) => {
                let Some(Phase::Hashing(world, hash)) = self.phase.take() else { unreachable!("hashed") };
                let replayed = hash.finish();
                self.slot = None;
                e.drop_elsewhere(world);
                let ms = (self.spent.as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
                Step::Done(self.answer(Json::obj([
                    ("replayable", Json::from(true)),
                    ("entries", Json::from(self.entries)),
                    ("replay_hash", Json::from(journal::hex(replayed))),
                    ("matches", Json::from(replayed == self.live)),
                    ("ms", Json::from(ms)),
                    ("steps", Json::from(self.steps)),
                ])))
            }
            Err(why) => {
                match self.phase.take() {
                    Some(Phase::Replaying(run)) => e.drop_elsewhere(Box::new((*run).into_world())),
                    Some(Phase::Hashing(world, _)) => e.drop_elsewhere(world),
                    Some(Phase::Waiting(_)) | None => {}
                }
                self.slot = None;
                Step::Done(self.answer(Json::obj([("replayable", Json::from(false)), ("message", Json::from(format!("the replay stopped: {why}; call again")))])))
            }
        }
    }
}

impl Engine {
    /// `world_hash {verify: true}`: the report now, the replay check in steps.
    pub(crate) fn start_hash_verify(&mut self, request: Request) {
        let Request { command: Command::Hash { recent, tick, .. }, session, reply, deadline, call, .. } = request else { unreachable!("only world_hash verify comes here") };
        let ToolOutput { content, .. } = self.hash_report(recent, tick);
        let out = content.into_iter().find_map(|c| if let Content::Text(t) = c { Json::parse(&t).ok() } else { None }).unwrap_or(Json::Null);
        let live = self.state_hash();
        let journal = self.swarm.journal();
        if !journal.replayable() {
            let mut out = out;
            out.set("verify", Json::obj([("replayable", Json::from(false)), ("message", Json::from("older journal entries were dropped (memory limit), so it can't replay from its base"))]));
            let _ = reply.send(ToolOutput::text(out.to_string()));
            return;
        }
        let entries = journal.len();
        // The replay begins when a slot is free, towards the journal as it is now.
        let job = Verify { out, live, entries, phase: Some(Phase::Waiting(journal.replay_point())), spent: Duration::ZERO, steps: 0, slot: None };
        let owner = jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        self.step_job(StepStart { owner, session: session.id.clone(), call, reply, deadline, class: schedule::Class::Big }, Box::new(job));
    }

    /// Frees a world on a worker: dropping 12,000 entities took one step about 10 ms on the engine
    /// thread (review, e45f66c).
    pub(crate) fn drop_elsewhere(&mut self, world: Box<World>) {
        let job = crate::workers::Offload::engine(crate::workers::JobKind::Drop);
        let _ = self.offload(job, move |_| drop(world), |_, _| {});
    }
}
