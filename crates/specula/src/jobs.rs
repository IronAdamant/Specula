//! Deferred and cancellable jobs: who started each, and the one answer it owes (see
//! `docs/design/AUTHORITY.md`, "Jobs").
//!
//! Every request to the engine carries a [`Reply`] ([`crate::tools::Request::reply`]), and a
//! tool call that answers later (perf_capture, stream_fly, model builds, parked captures, vfx
//! previews, playtests, background saves, the studio's renders) keeps it: [`channel`] is the only
//! way to make one, and no other module may hold a `Sender` of a tool answer
//! (`tests/authority_guard.rs`: any `Sender<..ToolOutput..>`, aliases included, outside this
//! file fails). A job that runs one at a time lives in a [`Slot`], which records its
//! [`JobOwner`] and:
//! - refuses a second start while busy, naming the owner (`job_busy`), never silently replacing
//!   it; the owner's own new start replaces its old job (the old one is answered);
//! - lets only the owner cancel ([`Slot::cancel`]); a lead's or the owner's forced cancel is
//!   allowed and the caller logs it on the board;
//! - runs the caller's `check` (claims, as the caller) on every start, even one that changes
//!   nothing, before anything else.
//!
//! **For the GI and disk lanes:** a bake or an upload is a `Slot<Bake>` (or a `Vec<Job<_>>` when
//! several may run): `start` with the caller's [`JobOwner`] and a `check` that tests the caller's
//! claims on what the job will write; `cancel` for their cancel tools; answer with
//! [`Job::answer`]. Their `Sender<ToolOutput>` fields become [`Reply`]s.

use crate::authority::{refusal, Authority, Site};
use crate::journal::Writer;
use crate::world::WorldError;
use specula_core::Json;
use specula_protocol::ToolOutput;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

/// Where a deferred call's one answer goes (a tool's answer; the studio's renders send
/// `Result<Rendered, ToolOutput>`). Made only by [`channel`].
pub struct Reply<T = ToolOutput>(Sender<T>);

impl<T> Clone for Reply<T> {
    fn clone(&self) -> Self {
        Reply(self.0.clone())
    }
}

/// A reply channel: the [`Reply`] the engine answers through, and the caller's end.
pub fn channel<T>() -> (Reply<T>, Receiver<T>) {
    let (tx, rx) = std::sync::mpsc::channel();
    (Reply(tx), rx)
}

impl<T> Reply<T> {
    /// Sends the answer; false when the caller has gone (timed out or disconnected).
    pub fn send(&self, out: T) -> bool {
        self.0.send(out).is_ok()
    }
}

impl Reply {
    /// Sends a refusal as the error answer.
    pub fn refuse(&self, e: &WorldError) -> bool {
        self.send(ToolOutput::error(e.to_json().to_string()))
    }
}

/// What kind of job (in refusals and listings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PerfCapture,
    StreamFly,
    ModelBuild,
    Capture,
    VfxPreview,
    Playtest,
    SaveGame,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::PerfCapture => "perf_capture",
            Kind::StreamFly => "stream_fly",
            Kind::ModelBuild => "model_build",
            Kind::Capture => "capture",
            Kind::VfxPreview => "vfx_preview",
            Kind::Playtest => "playtest",
            Kind::SaveGame => "save_game",
        }
    }
}

/// Who started a job: the agent's name and identity key (a resume keeps the key).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobOwner {
    pub name: String,
    pub key: String,
}

impl JobOwner {
    pub fn of(w: Writer) -> JobOwner {
        JobOwner { name: w.name.to_owned(), key: w.session.unwrap_or(w.name).to_owned() }
    }

    /// The engine's own jobs (the hosted game's menu, startup).
    pub fn engine(name: &str) -> JobOwner {
        JobOwner { name: name.to_owned(), key: format!("engine:{name}") }
    }

    pub fn is(&self, other: &JobOwner) -> bool {
        self.key == other.key && self.name == other.name
    }
}

/// A job in progress: its owner, its answer, its own state.
pub struct Job<S> {
    pub id: u64,
    pub kind: Kind,
    pub owner: JobOwner,
    pub started: Instant,
    reply: Reply,
    pub state: S,
}

impl<S> Job<S> {
    pub fn new(id: u64, kind: Kind, owner: JobOwner, reply: Reply, state: S) -> Job<S> {
        Job { id, kind, owner, started: Instant::now(), reply, state }
    }

    /// Answers the caller and ends the job.
    pub fn answer(self, out: ToolOutput) -> S {
        let _ = self.reply.send(out);
        self.state
    }

    pub fn reply(&self) -> &Reply {
        &self.reply
    }

    pub fn json(&self) -> Json {
        Json::obj([("id", Json::from(self.id)), ("kind", Json::from(self.kind.name())), ("owner", Json::from(self.owner.name.as_str()))])
    }
}

/// A job that runs one at a time.
pub struct Slot<S> {
    kind: Kind,
    job: Option<Job<S>>,
    next_id: u64,
}

impl<S> Slot<S> {
    pub fn new(kind: Kind) -> Slot<S> {
        Slot { kind, job: None, next_id: 0 }
    }

    /// Starts a job for `owner`. `check` runs first, always (claims, as the caller). Busy with
    /// another owner's job: refused with the owner named (the caller is answered with it). Busy
    /// with the caller's own: `replaced` answers the old one and the new one starts.
    pub fn start(
        &mut self,
        owner: JobOwner,
        reply: Reply,
        check: impl FnOnce() -> Result<(), WorldError>,
        state: impl FnOnce() -> S,
        replaced: impl FnOnce() -> ToolOutput,
    ) -> Result<&mut Job<S>, WorldError> {
        if let Err(e) = check() {
            reply.refuse(&e);
            return Err(e);
        }
        if let Some(j) = &self.job {
            if !j.owner.is(&owner) {
                let e = self.busy();
                reply.refuse(&e);
                return Err(e);
            }
            let old = self.job.take().expect("checked");
            old.answer(replaced());
        }
        self.next_id += 1;
        self.job = Some(Job::new(self.next_id, self.kind, owner, reply, state()));
        Ok(self.job.as_mut().expect("just set"))
    }

    /// The refusal a busy slot gives (`job_busy`, naming the owner).
    pub fn busy(&self) -> WorldError {
        let Some(j) = &self.job else { return refusal(Site::JobBusy, format!("{} is free", self.kind.name())) };
        refusal(
            Site::JobBusy,
            format!("{}'s {} (job {}) is running; one runs at a time: wait for it to answer, or ask {}", j.owner.name, j.kind.name(), j.id, j.owner.name),
        )
        .with("job", j.id)
        .with("kind", j.kind.name())
        .with("owner", j.owner.name.as_str())
    }

    /// Whether a job runs that `owner` didn't start.
    pub fn held_by_other(&self, owner: &JobOwner) -> bool {
        self.job.as_ref().is_some_and(|j| !j.owner.is(owner))
    }

    /// Cancels the job: its owner may; a lead or the owner may with `force` (the caller logs
    /// that on the board). Returns the job, to be answered by the caller.
    pub fn cancel(&mut self, by: &JobOwner, authority: Authority, force: bool) -> Result<Option<Job<S>>, WorldError> {
        let Some(j) = &self.job else { return Ok(None) };
        if !j.owner.is(by) && !(force && authority >= Authority::Lead) {
            let mut e = refusal(
                Site::JobNotYours,
                format!("{}'s {} (job {}) is theirs: only they may cancel it (a lead or the owner may with force, logged on the board)", j.owner.name, j.kind.name(), j.id),
            )
            .with("job", j.id)
            .with("owner", j.owner.name.as_str());
            if force {
                e = e.with("needs", Authority::Lead.name());
            }
            return Err(e);
        }
        Ok(self.job.take())
    }

    pub fn get(&self) -> Option<&Job<S>> {
        self.job.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut Job<S>> {
        self.job.as_mut()
    }

    /// Ends the job (the engine finished it; the caller answers it).
    pub fn take(&mut self) -> Option<Job<S>> {
        self.job.take()
    }

    pub fn is_busy(&self) -> bool {
        self.job.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn owner(n: &str) -> JobOwner {
        JobOwner { name: n.into(), key: format!("k-{n}") }
    }

    fn reply() -> (Reply, mpsc::Receiver<ToolOutput>) {
        channel()
    }

    fn code(rx: &mpsc::Receiver<ToolOutput>) -> String {
        let out = rx.try_recv().expect("answered");
        assert!(out.is_error);
        let specula_protocol::Content::Text(t) = &out.content[0] else { panic!() };
        Json::parse(t).unwrap().get("error").and_then(Json::as_str).unwrap().to_owned()
    }

    #[test]
    fn a_busy_slot_refuses_another_owner_and_names_the_owner() {
        let mut slot: Slot<u32> = Slot::new(Kind::StreamFly);
        let (r1, rx1) = reply();
        slot.start(owner("ann"), r1, || Ok(()), || 1, || ToolOutput::error("replaced")).unwrap();
        let (r2, rx2) = reply();
        let e = slot.start(owner("bob"), r2, || Ok(()), || 2, || ToolOutput::error("replaced")).err().unwrap();
        assert_eq!((e.code, e.detail("owner")), ("job_busy", Some(&Json::from("ann"))));
        assert_eq!(code(&rx2), "job_busy");
        assert!(rx1.try_recv().is_err(), "ann's job was not replaced or answered");
        assert_eq!(slot.get().unwrap().state, 1);
        // Ann's own new start replaces hers (and answers the old one).
        let (r3, _rx3) = reply();
        slot.start(owner("ann"), r3, || Ok(()), || 3, || ToolOutput::error("replaced by your newer job")).unwrap();
        assert!(rx1.try_recv().unwrap().is_error);
        assert_eq!(slot.get().unwrap().state, 3);
    }

    #[test]
    fn only_the_owner_cancels_unless_a_lead_forces() {
        let mut slot: Slot<u32> = Slot::new(Kind::PerfCapture);
        let (r, _rx) = reply();
        slot.start(owner("ann"), r, || Ok(()), || 1, || ToolOutput::error("x")).unwrap();
        assert_eq!(slot.cancel(&owner("bob"), Authority::Agent, false).err().unwrap().code, "not_job_owner");
        assert_eq!(slot.cancel(&owner("bob"), Authority::Reviewer, true).err().unwrap().code, "not_job_owner");
        assert!(slot.is_busy());
        assert!(slot.cancel(&owner("lee"), Authority::Lead, true).unwrap().is_some());
        let (r, _rx) = reply();
        slot.start(owner("ann"), r, || Ok(()), || 2, || ToolOutput::error("x")).unwrap();
        assert!(slot.cancel(&owner("ann"), Authority::Agent, false).unwrap().is_some());
    }

    #[test]
    fn the_check_runs_on_every_start() {
        let mut slot: Slot<u32> = Slot::new(Kind::StreamFly);
        let (r, rx) = reply();
        let e = slot.start(owner("ann"), r, || Err(WorldError::new("claimed", "no")), || 1, || ToolOutput::error("x")).err().unwrap();
        assert_eq!(e.code, "claimed");
        assert_eq!(code(&rx), "claimed");
        assert!(!slot.is_busy());
    }
}
