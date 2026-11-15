//! The worker pool: work the engine thread must not do itself (encodes, file writes and deletes),
//! run on a few threads with bounded slots, answered back on the engine thread
//! (docs/design/WORKERS.md).
//!
//! [`Engine::offload`] takes the work (`FnOnce(&Cancel) -> R`, owning everything it needs) and a
//! continuation (`FnOnce(&mut Engine, Result<R, Failed>)`) that runs on the engine thread when the
//! work is done ([`Engine::pump_workers`], once per loop pass). The continuation owns the call's
//! [`crate::jobs::Reply`] and answers through it; dropping it unanswered (shutdown) closes the
//! channel, which answers the caller "shutting down".

use crate::jobs::JobOwner;
use crate::schedule::Class;
use crate::world::WorldError;
use crate::Engine;
use specula_core::Json;
use specula_protocol::ToolOutput;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Worker threads (the reference laptop has four cores: the engine thread, the window thread and
/// the connection threads need the rest).
pub const WORKERS: usize = 2;
/// Agents' jobs queued or running at once, and per agent (identity key), and the pixels they may
/// hold between them (a 2048 x 2048 capture holds 16 MiB until it is encoded). A playtest's own
/// steps (call 0) don't count: one playtest runs at a time, holding the clock while it waits.
pub const MAX_AGENT_JOBS: usize = 64;
pub const MAX_PER_OWNER: usize = 4;
pub const MAX_PENDING_BYTES: u64 = 1 << 30;
// The engine's own jobs are never refused and never run inline (an inline 20,000-file delete is
// the stall this pool exists to prevent, review of 998af3d): their sources are bounded instead (one
// world write, one project sync and one trash emptying at a time; drops follow replays).
/// How often closed sessions' jobs are looked for.
const SESSION_CHECK: Duration = Duration::from_millis(250);

/// Where a job may run beside others: `Any` anywhere; jobs of one serial lane one at a time, in
/// the order they were offloaded (writes to one place never overlap).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lane {
    Any,
    /// The current world's folder (explicit saves; autosaves wait for them).
    World,
    /// Project file mirroring.
    Project,
    /// Deleting released scratch folders.
    Trash,
}

/// What a job is (in refusals, the trace and engine_info).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    CaptureEncode,
    PreviewEncode,
    WorldSave,
    Autosave,
    ProjectSync,
    ScratchDelete,
    Drop,
    TraceWrite,
    ReplayWrite,
    PerfWrite,
    ScratchMake,
    LeaseMarker,
    Warm,
    AnimCheck,
    QueryList,
    /// A gallery round's sheet packet read back after a restart (the taste tools).
    SheetRead,
    /// A capture's flash-watch feed whose pixels go to no other worker job.
    FlashFeed,
    /// A released scratch lease's folder moved into the trash (`lease_release {delete}`).
    ScratchTrash,
    /// The model library's scan of the assets folder (recipe, style, anim-set and glTF times).
    AssetScan,
    /// A capture view's subjects projected onto its image (`sight::SubjectSet::project`).
    SubjectProject,
    /// world_load's disk work: the outgoing board written, the world and its board read.
    WorldLoad,
    /// The hosted game's player settings written to their per-user file (`user_settings`).
    SettingsWrite,
    /// A timeline's audio cue: its sound recipe found and rendered (`timeline_audio`).
    TimelineSound,
    /// A module's sound recipe found and rendered (`Cx::sound`; presentation only).
    ModuleSound,
    /// The start screen's disk work: a folder checked or made, the project list read or
    /// written, the clipboard read for a pasted path (`launch_window`).
    Launch,
}

impl JobKind {
    pub fn name(self) -> &'static str {
        match self {
            JobKind::CaptureEncode => "capture_encode",
            JobKind::PreviewEncode => "vfx_preview_encode",
            JobKind::WorldSave => "world_save",
            JobKind::Autosave => "autosave",
            JobKind::ProjectSync => "project_sync",
            JobKind::ScratchDelete => "scratch_delete",
            JobKind::Drop => "drop",
            JobKind::TraceWrite => "desync_trace_write",
            JobKind::ReplayWrite => "playtest_replay_write",
            JobKind::PerfWrite => "perf_capture_write",
            JobKind::ScratchMake => "scratch_make",
            JobKind::LeaseMarker => "lease_marker",
            JobKind::Warm => "warm",
            JobKind::AnimCheck => "anim_check",
            JobKind::QueryList => "world_query_list",
            JobKind::SheetRead => "gallery_sheet_read",
            JobKind::FlashFeed => "flash_feed",
            JobKind::ScratchTrash => "scratch_trash",
            JobKind::AssetScan => "asset_scan",
            JobKind::SubjectProject => "subject_project",
            JobKind::WorldLoad => "world_load",
            JobKind::SettingsWrite => "settings_write",
            JobKind::TimelineSound => "timeline_sound",
            JobKind::ModuleSound => "module_sound",
            JobKind::Launch => "launch",
        }
    }
}

/// Set when a job's session closed: a queued job never starts, a running one may stop early.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Why a job has no result.
#[derive(Clone, Debug, PartialEq)]
pub enum Failed {
    /// Its session closed before it ran.
    Cancelled,
    /// The work panicked (the message); the worker survived.
    Panicked(String),
}

impl Failed {
    /// The answer a caller gets for it.
    pub fn to_output(&self, kind: JobKind) -> ToolOutput {
        let (code, message) = match self {
            Failed::Cancelled => ("session_closed", format!("the session closed before its {} ran", kind.name())),
            Failed::Panicked(m) => ("worker_failed", format!("the {} failed on its worker ({}); nothing else was affected: call again", kind.name(), crate::bounds::echo(m))),
        };
        ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message))]).to_string())
    }
}

/// What to offload: its kind, owner and lane, and the class its continuation is timed in.
pub struct Offload {
    kind: JobKind,
    /// The agent (and its session id) for agents' jobs; `None` for the engine's own.
    owner: Option<(JobOwner, String)>,
    lane: Lane,
    class: Class,
    /// Cancelled when the session closes (not a playtest's step: it outlives its starter's session).
    cancel_on_close: bool,
    /// What it holds until it runs (pixels), against [`MAX_PENDING_BYTES`].
    bytes: u64,
}

impl Offload {
    /// An agent's job. `call`: the request's call id (0 for a playtest step, which isn't cancelled
    /// with its starter's session).
    pub fn agent(kind: JobKind, owner: JobOwner, session: &str, call: u64) -> Offload {
        Offload { kind, owner: Some((owner, session.to_owned())), lane: Lane::Any, class: Class::Render, cancel_on_close: call != 0, bytes: 0 }
    }

    /// The engine's own job (a write it owes): never refused, never cancelled.
    pub fn engine(kind: JobKind) -> Offload {
        Offload { kind, owner: None, lane: Lane::Any, class: Class::Disk, cancel_on_close: false, bytes: 0 }
    }

    pub fn lane(mut self, lane: Lane) -> Offload {
        self.lane = lane;
        self
    }

    pub fn class(mut self, class: Class) -> Offload {
        self.class = class;
        self
    }

    /// What the job holds until it runs (the pixels it encodes).
    pub fn bytes(mut self, bytes: u64) -> Offload {
        self.bytes = bytes;
        self
    }
}

type Payload = Box<dyn Any + Send>;
type Work = Box<dyn FnOnce(&Cancel) -> Payload + Send>;
type Then = Box<dyn FnOnce(&mut Engine, Result<Payload, Failed>)>;

struct Task {
    id: u64,
    lane: Lane,
    cancel: Cancel,
    work: Work,
    trace: specula_core::trace::Ctx,
}

#[derive(Default)]
struct Queue {
    tasks: VecDeque<Task>,
    busy: BTreeSet<Lane>,
    stop: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

struct Pending {
    kind: JobKind,
    owner: Option<(JobOwner, String)>,
    cancel: Cancel,
    cancel_on_close: bool,
    bytes: u64,
    then: Then,
    class: Class,
    trace: specula_core::trace::Ctx,
    started: Instant,
    /// The tool call that started it (0: the engine's own): its continuation's commits are
    /// charged there ([`Engine::run_charged`]).
    call: u64,
}

/// The pool and the jobs waiting for their continuation.
pub struct Workers {
    shared: Arc<Shared>,
    done_tx: Sender<(u64, Result<Payload, Failed>)>,
    done_rx: Receiver<(u64, Result<Payload, Failed>)>,
    threads: usize,
    /// No thread could be started: work runs inline.
    no_threads: bool,
    pending: BTreeMap<u64, Pending>,
    next: u64,
    last_session_check: Instant,
    /// Totals for engine_info: (done, failed, cancelled, inline).
    pub counts: (u64, u64, u64, u64),
    /// The trash is being emptied, and folders were trashed since it began.
    trash_running: bool,
    trash_again: bool,
    /// Pixels the engine thread holds for agents' captures on their way to a worker (an orbit's
    /// sheet while its views render), charged against [`MAX_PENDING_BYTES`] with the queued
    /// jobs' ([`Workers::hold`]).
    held: Arc<AtomicU64>,
}

/// Bytes charged to the workers' cap while it lives ([`Workers::hold`]).
#[derive(Debug)]
pub struct Held {
    bytes: u64,
    on: Arc<AtomicU64>,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.on.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

impl Default for Workers {
    fn default() -> Workers {
        let (done_tx, done_rx) = mpsc::channel();
        Workers { shared: Arc::default(), done_tx, done_rx, threads: 0, no_threads: false, pending: BTreeMap::new(), next: 0, last_session_check: Instant::now(), counts: (0, 0, 0, 0), trash_running: false, trash_again: false, held: Arc::default() }
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        // Workers finish what is queued (writes the engine owes), then stop; the process may end
        // before they do (`Engine::workers_drain` waits for them first).
        let mut q = self.shared.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        q.stop = true;
        drop(q);
        self.shared.wake.notify_all();
    }
}

/// Tests only: how late a worker starts a job it took (`SPECULA_TEST_WORKER_DELAY_MS`; unset: at
/// once). The gate (`tools/examen`, its `slow` step) runs the engine's tests once with it set. Nothing may depend on when
/// a worker thread happens to run (the engine thread has a result or does the work itself): with
/// every worker late, code that does depend on it fails every time, where on a quiet machine it
/// failed one run in four (the orbit's pass count, review R1 of m5-gpuwork).
#[cfg(test)]
fn test_delay() -> Option<Duration> {
    static DELAY: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *DELAY.get_or_init(|| std::env::var("SPECULA_TEST_WORKER_DELAY_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()).filter(|ms| *ms > 0).map(Duration::from_millis))
}

fn worker_loop(shared: &Shared, done: &Sender<(u64, Result<Payload, Failed>)>) {
    loop {
        let task = {
            let mut q = shared.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                let free = q.tasks.iter().position(|t| t.lane == Lane::Any || !q.busy.contains(&t.lane));
                if let Some(i) = free {
                    let t = q.tasks.remove(i).expect("found");
                    if t.lane != Lane::Any {
                        q.busy.insert(t.lane);
                    }
                    break t;
                }
                if q.stop && q.tasks.is_empty() {
                    return;
                }
                q = shared.wake.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        let Task { id, lane, cancel, work, trace } = task;
        #[cfg(test)]
        if let Some(late) = test_delay() {
            std::thread::sleep(late);
        }
        let result = if cancel.is_set() {
            // What the job owns goes now (a slot guard gives its slot back), before it is answered.
            drop(work);
            Err(Failed::Cancelled)
        } else {
            let _c = trace.enter();
            specula_core::trace::catch_unwind(std::panic::AssertUnwindSafe(move || work(&cancel))).map_err(|p| Failed::Panicked(panic_text(&*p)))
        };
        {
            let mut q = shared.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            q.busy.remove(&lane);
        }
        shared.wake.notify_all();
        if done.send((id, result)).is_err() {
            return;
        }
    }
}

fn panic_text(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| (*s).to_owned()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".to_owned())
}

impl Workers {
    fn ensure_threads(&mut self) {
        while self.threads < WORKERS && !self.no_threads {
            let (shared, done) = (Arc::clone(&self.shared), self.done_tx.clone());
            let n = self.threads;
            match std::thread::Builder::new().name(format!("specula-worker-{n}")).spawn(move || worker_loop(&shared, &done)) {
                Ok(_) => self.threads += 1,
                Err(e) => {
                    if self.threads == 0 {
                        self.no_threads = true;
                    }
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "worker.no_thread", error = e.to_string(), why = "work runs on the engine thread");
                    break;
                }
            }
        }
    }

    /// Agents' jobs that count against their slots (not a playtest's: call 0).
    fn counted(&self) -> impl Iterator<Item = &Pending> {
        self.pending.values().filter(|p| p.owner.is_some() && p.cancel_on_close)
    }

    fn agent_jobs(&self) -> usize {
        self.counted().count()
    }

    fn engine_jobs(&self) -> usize {
        self.pending.values().filter(|p| p.owner.is_none()).count()
    }

    /// Whether `owner` may offload another job holding `bytes` (`job_busy` naming whose jobs fill
    /// the queue). A playtest's job (`call` 0) always may.
    pub fn room(&self, owner: &JobOwner, call: u64, bytes: u64) -> Result<(), WorldError> {
        if call == 0 {
            return Ok(());
        }
        let mine = self.counted().filter(|p| p.owner.as_ref().is_some_and(|(o, _)| o.is(owner))).count();
        let held: u64 = self.counted().map(|p| p.bytes).sum::<u64>() + self.held.load(Ordering::Relaxed);
        if self.agent_jobs() < MAX_AGENT_JOBS && mine < MAX_PER_OWNER && held + bytes <= MAX_PENDING_BYTES {
            return Ok(());
        }
        let owners: BTreeSet<&str> = self.counted().filter_map(|p| p.owner.as_ref()).map(|(o, _)| o.name.as_str()).collect();
        Err(crate::authority::refusal(
            crate::authority::Site::JobBusy,
            format!(
                "{} jobs are queued on the workers holding {} MiB ({MAX_PER_OWNER} per agent, {MAX_AGENT_JOBS} and {} MiB in all; yours: {mine}): call again in a moment",
                self.agent_jobs(),
                held >> 20,
                MAX_PENDING_BYTES >> 20
            ),
        )
        .with("queued_by", Json::arr(owners.into_iter().take(16).map(Json::from)))
        .with("retry_after_s", 1)
        .with("retry", true))
    }

    /// Charges `bytes` the engine holds for an agent's capture (see the field) until the guard
    /// drops. Callers ask [`Workers::room`] first.
    pub fn hold(&self, bytes: u64) -> Held {
        self.held.fetch_add(bytes, Ordering::Relaxed);
        Held { bytes, on: Arc::clone(&self.held) }
    }

    /// What [`Workers::hold`] guards hold now.
    pub fn held_bytes(&self) -> u64 {
        self.held.load(Ordering::Relaxed)
    }

    /// Jobs not answered yet.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn to_json(&self) -> Json {
        let (done, failed, cancelled, inline) = self.counts;
        Json::obj([
            ("threads", Json::from(self.threads)),
            ("pending", Json::from(self.pending.len())),
            ("agent_jobs", Json::from(self.agent_jobs())),
            ("engine_jobs", Json::from(self.engine_jobs())),
            ("done", Json::from(done)),
            ("failed", Json::from(failed)),
            ("cancelled", Json::from(cancelled)),
            ("inline", Json::from(inline)),
            ("held_bytes", Json::from(self.held_bytes())),
        ])
    }
}

impl Engine {
    /// Runs `work` on a worker and `then` on the engine thread with its result (see the module
    /// docs). An agent's job past its slots is refused (`job_busy`) and neither runs: the caller
    /// answers with the refusal. The engine's own jobs are never refused, and run inline only when
    /// no worker thread could be started.
    pub(crate) fn offload<R: Send + 'static>(&mut self, o: Offload, work: impl FnOnce(&Cancel) -> R + Send + 'static, then: impl FnOnce(&mut Engine, Result<R, Failed>) + 'static) -> Result<(), WorldError> {
        if let Some((owner, _)) = &o.owner {
            self.workers.room(owner, u64::from(o.cancel_on_close), o.bytes)?;
        }
        self.workers.ensure_threads();
        if self.workers.no_threads {
            self.workers.counts.3 += 1;
            let r = work(&Cancel::default());
            then(self, Ok(r));
            return Ok(());
        }
        self.workers.next += 1;
        let id = self.workers.next;
        let cancel = Cancel::default();
        let trace = specula_core::trace::Ctx::current();
        let then: Then = Box::new(move |e, r| then(e, r.map(|b| *b.downcast::<R>().unwrap_or_else(|_| unreachable!("a job's payload is its own type")))));
        self.workers.pending.insert(id, Pending { kind: o.kind, owner: o.owner, cancel: cancel.clone(), cancel_on_close: o.cancel_on_close, bytes: o.bytes, then, class: o.class, trace, started: Instant::now(), call: self.dispatch_call });
        let task = Task { id, lane: o.lane, cancel, work: Box::new(move |c| Box::new(work(c)) as Payload), trace };
        self.workers.shared.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner).tasks.push_back(task);
        self.workers.shared.wake.notify_one();
        Ok(())
    }

    /// Whether an agent may offload another job now (asked before expensive work whose end is an
    /// offload, such as a capture's render).
    pub(crate) fn workers_room(&self, owner: &JobOwner, call: u64, bytes: u64) -> Result<(), WorldError> {
        self.workers.room(owner, call, bytes)
    }

    /// Runs the continuations of finished jobs; cancels jobs of closed sessions (`alive`: the
    /// open sessions' ids, looked up at most every [`SESSION_CHECK`]).
    pub(crate) fn pump_workers(&mut self, server: Option<&specula_protocol::Server>) {
        if self.workers.pending.is_empty() {
            return;
        }
        if let Some(server) = server.filter(|_| self.workers.last_session_check.elapsed() >= SESSION_CHECK) {
            self.workers.last_session_check = Instant::now();
            let alive: BTreeSet<String> = server.all_sessions().into_iter().map(|s| s.id).collect();
            for p in self.workers.pending.values() {
                if p.cancel_on_close && p.owner.as_ref().is_some_and(|(_, s)| !alive.contains(s)) {
                    p.cancel.set();
                }
            }
        }
        let done: Vec<(u64, Result<Payload, Failed>)> = self.workers.done_rx.try_iter().collect();
        for (id, result) in done {
            let Some(p) = self.workers.pending.remove(&id) else { continue };
            match &result {
                Ok(_) => self.workers.counts.0 += 1,
                Err(Failed::Cancelled) => self.workers.counts.2 += 1,
                Err(Failed::Panicked(m)) => {
                    self.workers.counts.1 += 1;
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "worker.panicked", kind = p.kind.name(), message = m.as_str());
                }
            }
            let _c = p.trace.enter();
            let started = Instant::now();
            // Charged to the call that started the job, whichever request is pumping now (a
            // mount waiting for a sync, a load flushing saves).
            let by = p.owner.as_ref().map_or_else(|| p.kind.name().to_owned(), |(o, _)| o.name.clone());
            let then = p.then;
            self.run_charged(p.call, &by, move |e| then(e, result));
            let took = started.elapsed();
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "worker.done", kind = p.kind.name(), ms = p.started.elapsed().as_secs_f64() * 1000.0);
            self.engine_work_done(p.trace.call, Some(p.class), "worker_then", took);
        }
    }

    /// Empties the scratch root's trash (`disk::move_to_trash`) on the trash lane, accounted as
    /// scratch as each folder goes: one job at a time however many folders are released (review
    /// of 998af3d: a job per folder let empty leases fill the queue). Folders trashed while it runs
    /// are emptied by the next one.
    pub(crate) fn empty_trash_later(&mut self, root: std::path::PathBuf) {
        if self.workers.trash_running {
            self.workers.trash_again = true;
            return;
        }
        self.workers.trash_running = true;
        self.workers.trash_again = false;
        let job = Offload::engine(JobKind::ScratchDelete).lane(Lane::Trash);
        let then_root = root.clone();
        let work = move |_: &Cancel| -> Vec<String> {
            let mut problems = Vec::new();
            // Until nothing is left (a batch at a time), or a pass removes nothing.
            loop {
                let left = crate::disk::trash_left(&root);
                let mut removed = 0;
                for dir in &left {
                    match crate::disk::remove_tree_inside(&root, dir) {
                        Ok((files, bytes)) => {
                            removed += 1;
                            crate::disk::book().wrote(crate::disk::Category::Scratch, "lease", -(bytes as i64), -(files as i64));
                        }
                        Err(e) => problems.push(e),
                    }
                }
                if left.is_empty() || removed == 0 || problems.len() > 64 {
                    return problems;
                }
            }
        };
        let _ = self.offload(job, work, move |e, r| {
            e.workers.trash_running = false;
            let problems = match r {
                Ok(p) => p,
                Err(f) => vec![format!("{f:?}")],
            };
            for p in problems.iter().take(4) {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "worker.trash_left", error = p.as_str(), why = "a released scratch folder was not deleted (the next start sweeps the trash)");
            }
            if e.workers.trash_again {
                e.empty_trash_later(then_root);
            }
        });
    }

    /// perf_capture files of captures that ended this frame, written (and answered) on a worker.
    pub(crate) fn pump_perf_writes(&mut self) {
        for (job, n, stopped) in self.profiler.take_finished() {
            let o = Offload::engine(JobKind::PerfWrite).class(Class::Read);
            let _ = self.offload(o, move |_| crate::perf::write_capture(job, n, stopped), |_, _| {});
        }
    }

    /// Whether worker threads run (else offloaded work runs inline).
    pub(crate) fn workers_threaded(&mut self) -> bool {
        self.workers.ensure_threads();
        !self.workers.no_threads
    }

    /// At start: what an earlier run left in the scratch root's trash is emptied (on a worker).
    pub(crate) fn sweep_trash(&mut self) {
        let root = crate::disk::canonical(&self.scratch_root());
        if !crate::disk::trash_left(&root).is_empty() {
            self.empty_trash_later(root);
        }
    }

    /// Waits up to `limit` for one job to finish and runs the finished ones' continuations (a
    /// load waiting for a save it must not overtake).
    pub(crate) fn workers_wait_one(&mut self, limit: Duration) {
        if let Ok(msg) = self.workers.done_rx.recv_timeout(limit) {
            let _ = self.workers.done_tx.send(msg);
        }
        self.pump_workers(None);
    }

    /// At shutdown: waits up to `limit` for every job (writes the engine owes complete), running
    /// their continuations.
    pub(crate) fn workers_drain(&mut self, limit: Duration) {
        let until = Instant::now() + limit;
        while self.workers.busy() && Instant::now() < until {
            match self.workers.done_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(msg) => {
                    // Put it back for pump_workers to handle with the others.
                    let _ = self.workers.done_tx.send(msg);
                    self.pump_workers(None);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if self.workers.busy() {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "worker.unfinished_at_shutdown", jobs = self.workers.pending.len());
        }
    }
}

#[cfg(test)]
pub(crate) mod charge_tests {
    use super::*;
    use crate::journal::{Kind, Writer};

    /// A headless engine over Sepulcra's assets (`None`, after a skip line, without Vulkan).
    pub(crate) fn engine() -> Option<Engine> {
        engine_at(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets"))
    }

    /// A headless engine whose model library reads `assets` (a test's own folder).
    pub(crate) fn engine_at(assets: &std::path::Path) -> Option<Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test("charge_tests", "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test("charge_tests", &e.to_string());
                return None;
            }
        };
        let captures = std::env::temp_dir().join(format!("specula-charge-tests-{}", std::process::id()));
        Some(Engine::new(gpu, &captures, crate::models::Library::new(assets, None)).expect("engine"))
    }

    /// Offloads a job as tool call `call` whose continuation spawns `n` boxes as `who`.
    fn job(e: &mut Engine, call: u64, who: &'static str, n: usize) {
        e.dispatch_call = call;
        let _ = e.offload(Offload::engine(JobKind::ProjectSync), |_| (), move |e, _| {
            for i in 0..n {
                let spec = crate::tools::parse_spawn(&specula_core::Json::parse(&format!(r#"{{"name": "{who}{i}", "shape": "cube"}}"#)).unwrap()).unwrap();
                let w = Writer::new(who, Some(who));
                e.world.commit(&mut e.swarm, 0, w, Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(spec, who).map(|x| x.id)).unwrap();
            }
        });
        e.dispatch_call = 0;
    }

    /// The M4 final review: a request that pumps workers from inside its dispatch (a mount
    /// waiting for a sync, a load flushing saves) was charged the continuations' commits, other
    /// agents' (a debug engine panicked). They are charged to the calls that started them.
    #[test]
    fn a_pumped_continuation_is_charged_to_its_own_call() {
        let Some(mut e) = engine() else { return };
        // Another agent's job (call 9) commits once; a call 11 job commits twice (a real breach).
        job(&mut e, 9, "taste", 1);
        // c's request (call 17) waits for the jobs from inside its own dispatch, as
        // world_load's flush_world_saves and project_mount's project_wait_sync do.
        e.swarm.begin_dispatch("c");
        e.dispatch_call = 17;
        let until = Instant::now() + Duration::from_secs(20);
        while e.workers.busy() && Instant::now() < until {
            e.workers_wait_one(Duration::from_millis(20));
        }
        e.dispatch_call = 0;
        assert_eq!(e.swarm.end_dispatch(), 0, "c made no commit: the continuation's is call 9's");
        assert_eq!(e.multi_commit_calls, 0);
        assert_eq!(e.commits.count(9, 1, false), Some(2), "call 9 was charged its one commit (one more makes two)");
        e.multi_commit_calls = 0;
        // A job that itself commits twice is still a breach, of its own call (counted, not a
        // debug panic here: checked through the count directly).
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job(&mut e, 11, "b", 2);
            let until = Instant::now() + Duration::from_secs(20);
            while e.workers.busy() && Instant::now() < until {
                e.workers_wait_one(Duration::from_millis(20));
            }
        }));
        assert!(r.is_err() == cfg!(debug_assertions), "a continuation's own two commits are a breach of call 11");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(n: &str) -> JobOwner {
        JobOwner { name: n.into(), key: format!("k-{n}") }
    }

    /// The pool alone (no engine): tasks run on threads, lanes serialise, a panic is caught,
    /// a cancelled task never runs.
    #[test]
    fn the_pool_runs_tasks_by_lane_and_survives_panics() {
        let mut w = Workers::default();
        w.ensure_threads();
        let order = Arc::new(Mutex::new(Vec::new()));
        let push = |id: u64, lane: Lane, cancel: Cancel, f: Box<dyn FnOnce() -> u32 + Send>| {
            let order = Arc::clone(&order);
            let work: Work = Box::new(move |_| {
                let v = f();
                order.lock().unwrap().push((id, v));
                Box::new(v) as Payload
            });
            w.shared.queue.lock().unwrap().tasks.push_back(Task { id, lane, cancel, work, trace: Default::default() });
            w.shared.wake.notify_all();
        };
        push(1, Lane::World, Cancel::default(), Box::new(|| {
            std::thread::sleep(Duration::from_millis(30));
            1
        }));
        push(2, Lane::World, Cancel::default(), Box::new(|| 2));
        push(3, Lane::Any, Cancel::default(), Box::new(|| panic!("boom")));
        let c = Cancel::default();
        c.set();
        push(4, Lane::Any, c, Box::new(|| 4));
        let mut got = BTreeMap::new();
        while got.len() < 4 {
            let (id, r) = w.done_rx.recv_timeout(Duration::from_secs(10)).expect("every task answers");
            got.insert(id, r.map(|b| *b.downcast::<u32>().unwrap()));
        }
        assert_eq!(got[&1], Ok(1));
        assert_eq!(got[&2], Ok(2));
        assert!(matches!(&got[&3], Err(Failed::Panicked(m)) if m.contains("boom")));
        assert_eq!(got[&4], Err(Failed::Cancelled));
        let o = order.lock().unwrap();
        let p1 = o.iter().position(|(i, _)| *i == 1).unwrap();
        let p2 = o.iter().position(|(i, _)| *i == 2).unwrap();
        assert!(p1 < p2, "one serial lane runs in order: {o:?}");
    }

    /// Review of 998af3d (major 2): a vfx_preview whose session closed before its encode ran kept
    /// its slot forever (the guard was made inside the job's closure, which never ran). Whatever a
    /// job owns is dropped with it when it is cancelled: a guard moved in gives the slot back.
    #[test]
    fn a_cancelled_job_drops_what_it_owns() {
        struct Guard(Arc<std::sync::atomic::AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
            }
        }
        let mut w = Workers::default();
        w.ensure_threads();
        let slots = Arc::new(std::sync::atomic::AtomicUsize::new(1));
        let guard = Guard(Arc::clone(&slots));
        let cancel = Cancel::default();
        cancel.set();
        let work: Work = Box::new(move |_| {
            let _g = guard;
            Box::new(()) as Payload
        });
        w.shared.queue.lock().unwrap().tasks.push_back(Task { id: 9, lane: Lane::Any, cancel, work, trace: Default::default() });
        w.shared.wake.notify_all();
        let (id, r) = w.done_rx.recv_timeout(Duration::from_secs(10)).expect("answered");
        assert_eq!((id, r.err()), (9, Some(Failed::Cancelled)));
        assert_eq!(slots.load(Ordering::Relaxed), 0, "the cancelled job's guard gave its slot back");
    }

    #[test]
    fn slots_are_bounded_per_agent_and_in_all() {
        let mut w = Workers::default();
        let pending = |o: Option<JobOwner>, call: u64, bytes: u64| Pending {
            kind: JobKind::CaptureEncode,
            owner: o.map(|o| (o, "s".into())),
            cancel: Cancel::default(),
            cancel_on_close: call != 0,
            bytes,
            then: Box::new(|_, _| {}),
            class: Class::Render,
            trace: Default::default(),
            started: Instant::now(),
            call: 0,
        };
        for i in 0..MAX_PER_OWNER {
            assert!(w.room(&owner("ann"), 1, 0).is_ok());
            w.pending.insert(i as u64, pending(Some(owner("ann")), 1, 0));
        }
        let e = w.room(&owner("ann"), 1, 0).unwrap_err();
        assert_eq!(e.code, "job_busy");
        assert!(w.room(&owner("ann"), 0, 0).is_ok(), "a playtest's step (call 0) always has room");
        assert!(w.room(&owner("bob"), 1, 0).is_ok(), "another agent still has room");
        for i in 0..1000 {
            w.pending.insert(1000 + i as u64, pending(None, 0, 0));
        }
        assert!(w.room(&owner("bob"), 1, 0).is_ok(), "the engine's own jobs don't take agents' slots");
        for i in 0..40 {
            w.pending.insert(3000 + i as u64, pending(Some(owner("pt")), 0, 16 << 20));
        }
        assert!(w.room(&owner("bob"), 1, 0).is_ok(), "nor do a playtest's");
        assert_eq!(w.room(&owner("bob"), 1, MAX_PENDING_BYTES + 1).unwrap_err().code, "job_busy", "pixels held are bounded");
        for i in 0..MAX_AGENT_JOBS {
            w.pending.insert(5000 + i as u64, pending(Some(owner(&format!("a{i}"))), 1, 0));
        }
        assert_eq!(w.room(&owner("bob"), 1, 0).unwrap_err().code, "job_busy");
    }
}
