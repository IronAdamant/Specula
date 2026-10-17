//! Building modules with Cargo, one build at a time, merging duplicate requests.
//!
//! Cargo locks its target directory, so two agents building at once just take turns, each
//! paying for a full Cargo run. [`BuildService`] turns the lock into a scheduler: requests queue
//! in arrival order, one Cargo runs at a time, and a request for a build that is already queued
//! (same module, profile and mode) joins it instead of queueing another. A request never joins
//! a build that has already *started*, because the source may have changed since it began.

use crate::diag::{self, Diagnostic, Message};
use crate::fix::{self, Applied};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Most fix-and-rebuild rounds one `fix` build runs (fixes can reveal more fixes).
pub const MAX_FIX_ROUNDS: usize = 3;
/// Builds kept for [`BuildService::recent`].
const HISTORY: usize = 16;
/// Lines of Cargo's stderr kept for a failed build (manifest errors and the like).
const STDERR_TAIL: usize = 30;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildRequest {
    /// The module crate's folder (holds its `Cargo.toml`).
    pub dir: PathBuf,
    pub release: bool,
    /// `cargo check` only (type-check, no library, no reload).
    pub check: bool,
    /// Apply machine-applicable fixes inside `dir` and rebuild.
    pub fix: bool,
    /// Kill Cargo after this long.
    pub timeout: Duration,
}

impl BuildRequest {
    pub fn new(dir: impl Into<PathBuf>) -> BuildRequest {
        BuildRequest { dir: dir.into(), release: false, check: false, fix: false, timeout: Duration::from_secs(300) }
    }

    fn key(&self) -> (PathBuf, bool, bool, bool) {
        (fix::canonical(&self.dir), self.release, self.check, self.fix)
    }
}

/// What a build produced.
#[derive(Clone, Debug, Default)]
pub struct BuildReport {
    pub ok: bool,
    /// Machine-readable failure: `cargo_missing`, `cargo_failed`, `timeout`, `not_a_module`,
    /// `compile_errors`, `fix_failed`.
    pub error: Option<(&'static str, String)>,
    /// The package's library target name.
    pub package: Option<String>,
    /// The built `cdylib` (not for `check`).
    pub library: Option<PathBuf>,
    /// Cargo found nothing to rebuild.
    pub fresh: bool,
    /// Errors first, then warnings, in Cargo's order; only the module's own package.
    pub diagnostics: Vec<Diagnostic>,
    pub fixes_applied: Vec<Applied>,
    pub stderr_tail: Vec<String>,
    /// The Cargo command line (for humans reproducing it).
    pub command: String,
    /// Workspace root the file names are resolved against.
    pub root: PathBuf,
    /// Running Cargo (all rounds).
    pub build_time: Duration,
    /// Waiting for earlier builds.
    pub queued: Duration,
    /// Requests this one build answered (1 = no merging).
    pub requests: usize,
}

impl BuildReport {
    pub fn errors(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    pub fn warnings(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.level == "warning").count()
    }

    /// Machine-applicable fixes still on offer.
    pub fn fixable(&self) -> usize {
        self.diagnostics.iter().flat_map(|d| &d.fixes).filter(|f| f.machine_applicable()).count()
    }
}

/// Runs one build (Cargo, or a stand-in in tests).
pub type Runner = dyn Fn(&BuildRequest) -> BuildReport + Send + Sync;

/// A short record of a finished build.
#[derive(Clone, Debug)]
pub struct BuildSummary {
    pub dir: PathBuf,
    pub ok: bool,
    pub check: bool,
    pub build_ms: f64,
    pub requests: usize,
    pub errors: usize,
    pub warnings: usize,
}

struct Job {
    req: BuildRequest,
    started: bool,
    result: Option<Arc<BuildReport>>,
    /// Requests waiting on (or answered by) this job that haven't picked up the result.
    waiters: usize,
    requests: usize,
    since: Instant,
}

#[derive(Default)]
struct State {
    running: bool,
    /// Not-yet-started jobs, oldest first.
    queue: VecDeque<u64>,
    jobs: BTreeMap<u64, Job>,
    next: u64,
    history: VecDeque<BuildSummary>,
}

/// One Cargo at a time; duplicate queued requests merged (see the module docs).
pub struct BuildService {
    state: Mutex<State>,
    wake: Condvar,
    runner: Arc<Runner>,
}

impl Default for BuildService {
    fn default() -> BuildService {
        BuildService::new(Arc::new(run_cargo))
    }
}

impl BuildService {
    pub fn new(runner: Arc<Runner>) -> BuildService {
        BuildService { state: Mutex::new(State::default()), wake: Condvar::new(), runner }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Builds (or joins a queued identical build) and waits for the result.
    pub fn build(&self, req: BuildRequest) -> Arc<BuildReport> {
        let key = req.key();
        let mut st = self.lock();
        let joined = st.jobs.iter_mut().find(|(_, j)| !j.started && j.req.key() == key).map(|(id, j)| {
            j.waiters += 1;
            j.requests += 1;
            *id
        });
        let id = match joined {
            Some(id) => id,
            None => {
                let id = st.next;
                st.next += 1;
                st.jobs.insert(id, Job { req, started: false, result: None, waiters: 1, requests: 1, since: Instant::now() });
                st.queue.push_back(id);
                id
            }
        };
        loop {
            let job = st.jobs.get_mut(&id).expect("a job lives until its last waiter leaves");
            if let Some(r) = job.result.clone() {
                job.waiters -= 1;
                if job.waiters == 0 {
                    st.jobs.remove(&id);
                }
                return r;
            }
            if !st.running && st.queue.front() == Some(&id) {
                st.queue.pop_front();
                st.running = true;
                let job = st.jobs.get_mut(&id).expect("queued job");
                job.started = true;
                let (req, queued) = (job.req.clone(), job.since.elapsed());
                drop(st);
                let runner = Arc::clone(&self.runner);
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner(&req)));
                let mut report = outcome.unwrap_or_else(|_| BuildReport {
                    error: Some(("cargo_failed", "the build runner panicked (an engine bug)".into())),
                    ..BuildReport::default()
                });
                report.queued = queued;
                st = self.lock();
                st.running = false;
                let job = st.jobs.get_mut(&id).expect("started job");
                report.requests = job.requests;
                st.history.push_back(BuildSummary {
                    dir: req.dir.clone(),
                    ok: report.ok,
                    check: req.check,
                    build_ms: report.build_time.as_secs_f64() * 1000.0,
                    requests: report.requests,
                    errors: report.errors(),
                    warnings: report.warnings(),
                });
                if st.history.len() > HISTORY {
                    st.history.pop_front();
                }
                st.jobs.get_mut(&id).expect("started job").result = Some(Arc::new(report));
                self.wake.notify_all();
                continue;
            }
            st = self.wake.wait(st).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// `(running, queued)` builds right now.
    pub fn load(&self) -> (bool, usize) {
        let st = self.lock();
        (st.running, st.queue.len())
    }

    /// The last builds, oldest first.
    pub fn recent(&self) -> Vec<BuildSummary> {
        self.lock().history.iter().cloned().collect()
    }
}

/// The folder whose `Cargo.toml` has a `[workspace]` table, at or above `dir` (rustc reports
/// file names relative to it); `dir` itself if none.
pub fn workspace_root(dir: &Path) -> PathBuf {
    let dir = fix::canonical(dir);
    let mut at = Some(dir.as_path());
    while let Some(d) = at {
        let manifest = d.join("Cargo.toml");
        if std::fs::read_to_string(&manifest).is_ok_and(|t| t.lines().any(|l| l.trim() == "[workspace]")) {
            return d.to_owned();
        }
        at = d.parent();
    }
    dir
}

/// The Cargo to run: the one that launched us (`$CARGO`), else `cargo` on the PATH.
pub fn cargo_program() -> PathBuf {
    std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from)
}

/// The default [`Runner`]: Cargo, with fix rounds when asked.
pub fn run_cargo(req: &BuildRequest) -> BuildReport {
    let start = Instant::now();
    let mut report = cargo_once(req);
    let mut rounds = 0;
    while req.fix && rounds < MAX_FIX_ROUNDS && report.fixable() > 0 {
        rounds += 1;
        match fix::apply(&report.diagnostics, &req.dir) {
            Ok(applied) if applied.is_empty() => break,
            Ok(applied) => {
                let mut done = std::mem::take(&mut report.fixes_applied);
                done.extend(applied);
                report = cargo_once(req);
                report.fixes_applied = done;
            }
            Err(e) => {
                report.ok = false;
                report.error = Some(("fix_failed", format!("{}: {}", e.file.display(), e.message)));
                break;
            }
        }
    }
    report.build_time = start.elapsed();
    report
}

/// One Cargo run, its JSON messages parsed.
fn cargo_once(req: &BuildRequest) -> BuildReport {
    let root = workspace_root(&req.dir);
    let manifest = fix::canonical(&req.dir.join("Cargo.toml"));
    let mut args: Vec<String> = vec![
        if req.check { "check" } else { "build" }.into(),
        "--lib".into(),
        "--message-format=json".into(),
        "--color".into(),
        "never".into(),
        "--manifest-path".into(),
        manifest.to_string_lossy().into_owned(),
    ];
    if req.release {
        args.push("--release".into());
    }
    let program = cargo_program();
    let mut report = BuildReport {
        command: format!("{} {}", program.display(), args.join(" ")),
        root: root.clone(),
        ..BuildReport::default()
    };
    let child = Command::new(&program)
        .args(&args)
        .current_dir(&req.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            report.error = Some(("cargo_missing", format!("cannot run {}: {e}", program.display())));
            return report;
        }
    };
    // Both pipes drain on their own threads, so neither can fill up and stall Cargo.
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let out_thread = std::thread::spawn(move || BufReader::new(stdout).lines().map_while(Result::ok).collect::<Vec<_>>());
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = BufReader::new(stderr).take(1 << 20).read_to_string(&mut s);
        s
    });
    let deadline = Instant::now() + req.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break None,
        }
    };
    let lines = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut finished = None;
    for line in &lines {
        match diag::parse_line(line, &root) {
            Message::Diagnostic { manifest: m, diagnostic } if fix::canonical(&m) == manifest => {
                // "aborting due to..." summaries carry no information an agent needs.
                if diagnostic.file.is_none() && diagnostic.message.starts_with("aborting due to") {
                    continue;
                }
                if diagnostic.is_error() {
                    errors.push(diagnostic);
                } else {
                    warnings.push(diagnostic);
                }
            }
            Message::Artifact { manifest: m, name, kinds, files, fresh } if fix::canonical(&m) == manifest => {
                report.package = Some(name);
                report.fresh = fresh;
                if kinds.iter().any(|k| k == "cdylib") {
                    report.library = files.into_iter().find(|f| {
                        let ext = f.extension().and_then(|e| e.to_str()).unwrap_or_default();
                        matches!(ext, "dll" | "so" | "dylib")
                    });
                }
            }
            Message::Finished { success } => finished = Some(success),
            _ => {}
        }
    }
    errors.extend(warnings);
    report.diagnostics = errors;
    let tail: Vec<String> = stderr.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect();
    report.stderr_tail = tail[tail.len().saturating_sub(STDERR_TAIL)..].to_vec();
    match (status, finished) {
        (None, _) => report.error = Some(("timeout", format!("cargo ran longer than {} s and was stopped", req.timeout.as_secs()))),
        (Some(s), Some(true)) if s.success() => {
            if !req.check && report.library.is_none() {
                report.error = Some((
                    "not_a_module",
                    "the package built no cdylib: add `[lib] crate-type = [\"cdylib\"]` to its Cargo.toml".into(),
                ));
            } else {
                report.ok = true;
            }
        }
        (Some(_), _) if report.errors() > 0 => {
            report.error = Some(("compile_errors", format!("{} compile error(s): see diagnostics", report.errors())));
        }
        (Some(s), _) => {
            let last = report.stderr_tail.last().cloned().unwrap_or_default();
            report.error = Some(("cargo_failed", format!("cargo failed ({s}): {last}")));
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    /// A runner that blocks until told to finish, and counts its runs.
    fn gated() -> (Arc<Runner>, Arc<AtomicUsize>, mpsc::Sender<()>) {
        let runs = Arc::new(AtomicUsize::new(0));
        let (go, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let counted = Arc::clone(&runs);
        let runner: Arc<Runner> = Arc::new(move |req: &BuildRequest| {
            counted.fetch_add(1, Ordering::SeqCst);
            gate.lock().unwrap().recv().unwrap();
            BuildReport { ok: true, package: Some(req.dir.display().to_string()), ..BuildReport::default() }
        });
        (runner, runs, go)
    }

    fn wait_until(mut f: impl FnMut() -> bool) {
        while !f() {
            std::thread::yield_now();
        }
    }

    #[test]
    fn queued_duplicates_merge_into_one_build() {
        let (runner, runs, go) = gated();
        let svc = Arc::new(BuildService::new(runner));
        let spawn = |dir: &str| {
            let svc = Arc::clone(&svc);
            let req = BuildRequest::new(dir);
            std::thread::spawn(move || svc.build(req))
        };
        // A runs; B and C ask for the same module while it does; D asks for another one.
        let a = spawn("mod_a");
        wait_until(|| svc.load() == (true, 0));
        let b = spawn("mod_a");
        wait_until(|| svc.load().1 == 1);
        let c = spawn("mod_a");
        let d = spawn("mod_d");
        wait_until(|| svc.load().1 == 2 && svc.lock().jobs.values().any(|j| j.requests == 2));
        for _ in 0..3 {
            go.send(()).unwrap();
        }
        let (ra, rb, rc, rd) = (a.join().unwrap(), b.join().unwrap(), c.join().unwrap(), d.join().unwrap());
        assert_eq!(runs.load(Ordering::SeqCst), 3, "A, then B+C merged, then D");
        assert_eq!(ra.requests, 1);
        assert!(Arc::ptr_eq(&rb, &rc), "B and C share one report");
        assert_eq!(rb.requests, 2);
        assert_eq!(rd.package.as_deref(), Some("mod_d"));
        assert!(svc.lock().jobs.is_empty(), "finished jobs are dropped");
        assert_eq!(svc.recent().len(), 3);
    }

    #[test]
    fn a_panicking_runner_fails_the_build_not_the_service() {
        let svc = BuildService::new(Arc::new(|_: &BuildRequest| -> BuildReport { panic!("boom") }));
        let r = svc.build(BuildRequest::new("x"));
        assert!(!r.ok);
        assert_eq!(r.error.as_ref().map(|e| e.0), Some("cargo_failed"));
        assert_eq!(svc.load(), (false, 0));
    }

    #[test]
    fn workspace_root_is_found_upwards() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = workspace_root(here);
        assert!(root.join("crates").join("specula-logic").exists(), "{}", root.display());
    }
}
