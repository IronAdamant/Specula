//! Building modules with Cargo, one build at a time, merging duplicate requests.
//!
//! Cargo locks its target directory, so two agents building at once just take turns, each
//! paying for a full Cargo run. [`BuildService`] turns the lock into a scheduler: requests queue
//! in arrival order, one Cargo runs at a time, and a request for a build that is already queued
//! (same module, profile and mode) joins it instead of queueing another. A request never joins
//! a build that has already *started*, because the source may have changed since it began.
//!
//! **What a module may build from** (checked before Cargo builds anything, see [`vet`]): crates
//! by path under the request's `root` only (no registry or git crates: zero third-party crates,
//! AGENTS.md rule 1), no build scripts, no proc-macros, and no `.cargo/` configuration or
//! `rust-toolchain` file between the module and the root (a `rustc-wrapper`, linker or toolchain
//! there would run). Every Cargo the engine starts is pinned to the engine's own toolchain
//! ([`cargo_command`]), runs `--offline` (nothing is downloaded) and builds with
//! `panic = "unwind"` (an aborting module would take the engine down with its first panic). The
//! build's own output is checked again before its library may load ([`check_build_output`]), so a
//! manifest edited between the check and the build can't get anything loaded (a build script it
//! adds has already run by then: the checks are no sandbox). A module, and anything it builds, still runs with the engine's
//! privileges: these checks keep builds zero-dependency and free of build-time code, they are no
//! sandbox.

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
    /// Every crate the build uses, the module included, must live under one of these folders
    /// (the engine's project).
    pub roots: Vec<PathBuf>,
    /// Cargo's `--target-dir` (default: Cargo's own choice, e.g. `CARGO_TARGET_DIR`).
    pub target_dir: Option<PathBuf>,
}

impl BuildRequest {
    /// `roots` defaults to `dir`'s Cargo workspace root.
    pub fn new(dir: impl Into<PathBuf>) -> BuildRequest {
        let dir = dir.into();
        let roots = vec![workspace_root(&dir)];
        BuildRequest { dir, release: false, check: false, fix: false, timeout: Duration::from_secs(300), roots, target_dir: None }
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
    /// Errors first, then warnings, in Cargo's order: the module's own messages, plus errors in
    /// other crates of the build (tagged with their `package`; their warnings are left out).
    pub diagnostics: Vec<Diagnostic>,
    pub fixes_applied: Vec<Applied>,
    pub stderr_tail: Vec<String>,
    /// The module crate's artifacts (its library, or a check build's metadata).
    pub artifacts: Vec<PathBuf>,
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

    /// Determinism lint warnings ([`crate::lint`]) among the diagnostics.
    pub fn determinism_warnings(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.code.as_deref().is_some_and(|c| c.starts_with("determinism::"))).count()
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

/// Refuses a build that would use more than the module's own path crates (see the module docs):
/// `Err((code, message))` naming the offending package, target or file. Codes: `outside_root`,
/// `cargo_config`, `toolchain_file`, `metadata_failed`, `third_party_crate`, `build_script`,
/// `proc_macro`. The folders are checked before anything runs.
///
/// `cargo metadata` may rewrite the workspace's `Cargo.lock` while resolving; if the build is
/// refused (or metadata fails) the lock file is put back as it was.
pub fn vet(req: &BuildRequest) -> Result<(), (&'static str, String)> {
    let roots: Vec<PathBuf> = req.roots.iter().map(|r| fix::canonical(r)).collect();
    let dir = fix::canonical(&req.dir);
    let Some(root) = roots.iter().filter(|r| dir.starts_with(r)).max_by_key(|r| r.components().count()).cloned() else {
        let list: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
        return Err(("outside_root", format!("{} is not inside {}", dir.display(), list.join(" or "))));
    };
    // Folder-level Cargo config between the module and the root (the root's own is the project's).
    let mut at = Some(dir.as_path());
    while let Some(d) = at.filter(|d| *d != root && d.starts_with(&root)) {
        let cfg = d.join(".cargo");
        if cfg.exists() {
            return Err(("cargo_config", format!("{} configures Cargo for this module (a wrapper or linker there would run); modules use the project's configuration only: remove it", cfg.display())));
        }
        for name in ["rust-toolchain", "rust-toolchain.toml"] {
            let tc = d.join(name);
            if tc.exists() {
                return Err(("toolchain_file", format!("{} picks a toolchain for this module (rustup would run or download it); modules build with the engine's toolchain: remove it", tc.display())));
            }
        }
        at = d.parent();
    }
    // Compile-time paths (`#[path]`, `include_str!`...) stay inside the module's src/.
    crate::paths::check_module(&dir)?;
    let lock = workspace_root(&dir).join("Cargo.lock");
    let lock_before = std::fs::read(&lock).ok();
    let restore = || match &lock_before {
        Some(bytes) => {
            if std::fs::read(&lock).ok().as_deref() != Some(bytes.as_slice()) {
                let _ = std::fs::write(&lock, bytes);
            }
        }
        None => {
            let _ = std::fs::remove_file(&lock);
        }
    };
    let out = cargo_command()
        .args(["metadata", "--format-version", "1", "--offline", "--color", "never", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            restore();
            return Err(("metadata_failed", format!("cannot run {}: {e}", cargo_program().display())));
        }
    };
    if !out.status.success() {
        restore();
        let err = String::from_utf8_lossy(&out.stderr);
        let last: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = last[last.len().saturating_sub(6)..].join(" | ");
        return Err((
            "metadata_failed",
            format!("cargo metadata --offline failed (modules may use only path crates under the project, e.g. specula-sdk): {tail}"),
        ));
    }
    let meta = specula_core::Json::parse(&String::from_utf8_lossy(&out.stdout))
        .map_err(|e| ("metadata_failed", format!("cargo metadata printed no JSON: {e:?}")));
    let checked = meta.and_then(|m| check_metadata(&m, &roots));
    if checked.is_err() {
        restore();
    }
    checked
}

/// The rules of [`vet`] over `cargo metadata` output: every package is a path crate under one of
/// `roots`, with no build script and no proc-macro target.
pub fn check_metadata(meta: &specula_core::Json, roots: &[PathBuf]) -> Result<(), (&'static str, String)> {
    use specula_core::Json;
    let roots: Vec<PathBuf> = roots.iter().map(|r| fix::canonical(r)).collect();
    for p in meta.get("packages").and_then(Json::as_array).unwrap_or_default() {
        let name = p.get("name").and_then(Json::as_str).unwrap_or("?");
        if let Some(source) = p.get("source").and_then(Json::as_str) {
            return Err(("third_party_crate", format!("package '{name}' comes from {source}: modules may depend only on path crates in the project (zero third-party crates)")));
        }
        let manifest = PathBuf::from(p.get("manifest_path").and_then(Json::as_str).unwrap_or_default());
        let canon = fix::canonical(&manifest);
        if !roots.iter().any(|r| canon.starts_with(r)) {
            let list: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
            return Err(("outside_root", format!("package '{name}' ({}) is outside the project ({})", manifest.display(), list.join(" or "))));
        }
        for t in p.get("targets").and_then(Json::as_array).unwrap_or_default() {
            let kinds: Vec<&str> = t.get("kind").and_then(Json::as_array).unwrap_or_default().iter().filter_map(Json::as_str).collect();
            let src = t.get("src_path").and_then(Json::as_str).unwrap_or("?");
            if kinds.contains(&"custom-build") {
                return Err(("build_script", format!("package '{name}' has a build script ({src}); nothing in a module's build may run code at build time: remove it")));
            }
            if kinds.contains(&"proc-macro") {
                return Err(("proc_macro", format!("package '{name}' is a proc-macro crate ({src}); modules may not use proc-macros (they run code at build time)")));
            }
        }
    }
    Ok(())
}

/// The Cargo to run: the one that launched us (`$CARGO`), else `cargo` on the PATH.
pub fn cargo_program() -> PathBuf {
    if let Some(c) = std::env::var_os("CARGO") {
        return PathBuf::from(c);
    }
    // The Cargo that built the engine (an absolute path into its toolchain), if it's still there.
    match option_env!("CARGO").map(PathBuf::from) {
        Some(c) if c.is_absolute() && c.is_file() => c,
        _ => PathBuf::from("cargo"),
    }
}

/// The toolchain the engine was built with (e.g. `stable-x86_64-pc-windows-gnu`): rustup's
/// `RUSTUP_TOOLCHAIN` now or at build time, else the toolchain folder the building Cargo lived in.
pub fn engine_toolchain() -> Option<String> {
    if let Some(t) = std::env::var("RUSTUP_TOOLCHAIN").ok().filter(|t| !t.is_empty()) {
        return Some(t);
    }
    if let Some(t) = option_env!("RUSTUP_TOOLCHAIN").filter(|t| !t.is_empty()) {
        return Some(t.to_owned());
    }
    // .../toolchains/<name>/bin/cargo
    let cargo = PathBuf::from(option_env!("CARGO")?);
    let toolchain_dir = cargo.parent()?.parent()?;
    (toolchain_dir.parent()?.file_name()? == "toolchains").then(|| toolchain_dir.file_name()?.to_str().map(str::to_owned)).flatten()
}

/// A Cargo command pinned to the engine's toolchain: `RUSTUP_TOOLCHAIN` set (so rustup ignores
/// toolchain files and overrides), and `RUSTC` set to the rustc next to an absolute Cargo.
pub fn cargo_command() -> Command {
    let program = cargo_program();
    let mut cmd = Command::new(&program);
    if let Some(t) = engine_toolchain() {
        cmd.env("RUSTUP_TOOLCHAIN", t);
    }
    if program.is_absolute() {
        let rustc = program.with_file_name(if cfg!(windows) { "rustc.exe" } else { "rustc" });
        if rustc.is_file() {
            cmd.env("RUSTC", rustc);
        }
    }
    cmd
}

/// Checks a build's own messages (after [`vet`] checked the manifest, which may have changed
/// since): every artifact is a path crate under `roots`, no build script ran, no proc-macro was
/// built. `Err((code, message))` names the package.
pub fn check_build_output(messages: &[Message], roots: &[PathBuf]) -> Result<(), (&'static str, String)> {
    let roots: Vec<PathBuf> = roots.iter().map(|r| fix::canonical(r)).collect();
    let late = "appeared in the build itself (the manifest changed after it was checked); the build was not loaded";
    for m in messages {
        match m {
            Message::BuildScript { package_id } => {
                return Err(("build_script", format!("a build script ran for {package_id}: {late}")));
            }
            Message::Artifact { manifest, package_id, kinds, .. } => {
                if !package_id.starts_with("path+file://") {
                    return Err(("third_party_crate", format!("{package_id} is not a path crate: {late}")));
                }
                let canon = fix::canonical(manifest);
                if !roots.iter().any(|r| canon.starts_with(r)) {
                    return Err(("outside_root", format!("{} is outside the project: {late}", manifest.display())));
                }
                if kinds.iter().any(|k| k == "proc-macro") {
                    return Err(("proc_macro", format!("{package_id} is a proc-macro: {late}")));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `Cargo.lock` as it was before a build, to put back if the build is refused.
struct LockGuard {
    path: PathBuf,
    before: Option<Vec<u8>>,
}

impl LockGuard {
    fn take(dir: &Path) -> LockGuard {
        let path = workspace_root(dir).join("Cargo.lock");
        let before = std::fs::read(&path).ok();
        LockGuard { path, before }
    }

    fn restore(&self) {
        match &self.before {
            Some(bytes) => {
                if std::fs::read(&self.path).ok().as_deref() != Some(bytes.as_slice()) {
                    let _ = std::fs::write(&self.path, bytes);
                }
            }
            None => {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

/// The default [`Runner`]: Cargo, with fix rounds when asked.
pub fn run_cargo(req: &BuildRequest) -> BuildReport {
    let start = Instant::now();
    let lock = LockGuard::take(&req.dir);
    if let Err((code, message)) = vet(req) {
        return BuildReport {
            error: Some((code, message)),
            command: format!("{} metadata --offline", cargo_program().display()),
            build_time: start.elapsed(),
            ..BuildReport::default()
        };
    }
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
    if report.error.as_ref().is_some_and(|e| matches!(e.0, "build_script" | "third_party_crate" | "outside_root" | "proc_macro")) {
        lock.restore();
    }
    // The authoritative compile-time path check: every file rustc read for the module (its
    // dep-info) lies inside the module's folder. A refused build loads nothing, and its
    // diagnostics go (an include! error can echo the file it read).
    if report.ok {
        let dep = report.package.as_deref().and_then(|name| report.artifacts.iter().find_map(|a| crate::paths::find_dep_info(a, name)));
        let checked = match dep {
            Some(d) => crate::paths::check_dep_info(&d, &req.dir, &workspace_root(&req.dir)),
            None => Err(("source_path", "the build left no dep-info to check which files it read; the module isn't loaded".to_owned())),
        };
        if let Err((code, message)) = checked {
            report.ok = false;
            report.error = Some((code, message));
            report.library = None;
            report.diagnostics.clear();
            report.stderr_tail.clear();
            return BuildReport { build_time: start.elapsed(), ..report };
        }
    }
    // Determinism lints on the module's sources (crate::lint): warnings after the compiler's.
    report.diagnostics.extend(crate::lint::scan_dir(&req.dir));
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
        "--offline".into(),
        "--color".into(),
        "never".into(),
        "--manifest-path".into(),
        manifest.to_string_lossy().into_owned(),
    ];
    if req.release {
        args.push("--release".into());
    }
    if let Some(t) = &req.target_dir {
        args.push("--target-dir".into());
        args.push(t.to_string_lossy().into_owned());
    }
    // Modules unwind (the SDK catches panics at the boundary): a module's manifest can't make
    // a panic abort the engine.
    for profile in ["dev", "release"] {
        args.push("--config".into());
        args.push(format!("profile.{profile}.panic=\"unwind\""));
    }
    let program = cargo_program();
    let mut report = BuildReport {
        command: format!("{} {}", program.display(), args.join(" ")),
        root: root.clone(),
        ..BuildReport::default()
    };
    let child = cargo_command()
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
    let mut seen = Vec::new();
    for line in &lines {
        let message = diag::parse_line(line, &root);
        if matches!(message, Message::Artifact { .. } | Message::BuildScript { .. }) {
            seen.push(message.clone());
        }
        match message {
            Message::Diagnostic { manifest: m, target, mut diagnostic } => {
                // "aborting due to..." summaries carry no information an agent needs.
                if diagnostic.file.is_none() && diagnostic.message.starts_with("aborting due to") {
                    continue;
                }
                if fix::canonical(&m) != manifest {
                    // Another crate's warnings are not the module's business; its errors are
                    // (they stop the build: a broken SDK, a full disk).
                    if !diagnostic.is_error() {
                        continue;
                    }
                    diagnostic.package = Some(target);
                }
                if diagnostic.is_error() {
                    errors.push(diagnostic);
                } else {
                    warnings.push(diagnostic);
                }
            }
            Message::Artifact { manifest: m, name, kinds, files, fresh, .. } if fix::canonical(&m) == manifest => {
                report.package = Some(name);
                report.fresh = fresh;
                report.artifacts.extend(files.iter().cloned());
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
    let vetted = check_build_output(&seen, &req.roots);
    let tail: Vec<String> = stderr.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect();
    report.stderr_tail = tail[tail.len().saturating_sub(STDERR_TAIL)..].to_vec();
    match (status, finished) {
        (None, _) => report.error = Some(("timeout", format!("cargo ran longer than {} s and was stopped", req.timeout.as_secs()))),
        (Some(_), _) if vetted.is_err() => {
            report.library = None;
            report.error = vetted.err();
        }
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
    fn metadata_rules_name_what_they_refuse() {
        use specula_core::Json;
        let root = fix::canonical(&std::env::temp_dir());
        let roots = vec![root.clone()];
        let r = root.display().to_string().replace('\\', "/");
        let pkg = |name: &str, source: &str, kinds: &str| {
            format!(r#"{{"name": "{name}", "source": {source}, "manifest_path": "{r}/{name}/Cargo.toml", "targets": [{{"kind": [{kinds}], "src_path": "{r}/{name}/x.rs"}}]}}"#)
        };
        let meta = |pkgs: &[String]| Json::parse(&format!(r#"{{"packages": [{}]}}"#, pkgs.join(","))).unwrap();
        let ok = pkg("m", "null", r#""cdylib""#);
        assert_eq!(check_metadata(&meta(std::slice::from_ref(&ok)), &roots), Ok(()));
        let reg = pkg("itoa", r#""registry+https://github.com/rust-lang/crates.io-index""#, r#""lib""#);
        let e = check_metadata(&meta(&[ok.clone(), reg]), &roots).unwrap_err();
        assert_eq!(e.0, "third_party_crate");
        assert!(e.1.contains("itoa"), "{}", e.1);
        let bs = pkg("m2", "null", r#""cdylib", "custom-build""#);
        assert_eq!(check_metadata(&meta(&[bs]), &roots).unwrap_err().0, "build_script");
        let pm = pkg("derive", "null", r#""proc-macro""#);
        assert_eq!(check_metadata(&meta(&[pm]), &roots).unwrap_err().0, "proc_macro");
        let elsewhere = Json::parse(r#"{"packages": [{"name": "far", "source": null, "manifest_path": "/somewhere/else/far/Cargo.toml", "targets": []}]}"#).unwrap();
        assert_eq!(check_metadata(&elsewhere, &[root.join("specula-nowhere")]).unwrap_err().0, "outside_root");
    }

    #[test]
    fn folder_cargo_config_and_paths_outside_the_root_are_refused() {
        let base = std::env::temp_dir().join(format!("specula-vet-{}", std::process::id()));
        let module = base.join("mods").join("m");
        std::fs::create_dir_all(base.join("mods").join(".cargo")).unwrap();
        std::fs::create_dir_all(&module).unwrap();
        let mut req = BuildRequest::new(&module);
        req.roots = vec![base.clone()];
        assert_eq!(vet(&req).unwrap_err().0, "cargo_config");
        req.roots = vec![base.join("other")];
        std::fs::create_dir_all(&req.roots[0]).unwrap();
        assert_eq!(vet(&req).unwrap_err().0, "outside_root");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn toolchain_files_are_refused_before_anything_runs() {
        let base = std::env::temp_dir().join(format!("specula-vet-tc-{}", std::process::id()));
        let module = base.join("tcmod");
        std::fs::create_dir_all(&module).unwrap();
        // The reviewer's probe: a toolchain path pointing at a fake cargo.
        std::fs::write(module.join("rust-toolchain.toml"), "[toolchain]\npath = \"C:/nowhere/faketc\"\n").unwrap();
        let mut req = BuildRequest::new(&module);
        req.roots = vec![base.clone()];
        let e = vet(&req).unwrap_err();
        assert_eq!(e.0, "toolchain_file");
        assert!(e.1.contains("rust-toolchain.toml"), "{}", e.1);
        std::fs::remove_file(module.join("rust-toolchain.toml")).unwrap();
        std::fs::write(base.join("rust-toolchain"), "nightly").unwrap();
        req.roots = vec![base.parent().unwrap().to_owned()];
        assert_eq!(vet(&req).unwrap_err().0, "toolchain_file", "a plain rust-toolchain above the module too");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn every_cargo_is_pinned_to_the_engines_toolchain() {
        let Some(toolchain) = engine_toolchain() else {
            println!("skipped: this build of the engine doesn't know its toolchain (not built through rustup)");
            return;
        };
        let cmd = cargo_command();
        let env: Vec<(String, String)> = cmd
            .get_envs()
            .filter_map(|(k, v)| Some((k.to_str()?.to_owned(), v?.to_str()?.to_owned())))
            .collect();
        assert!(env.contains(&("RUSTUP_TOOLCHAIN".to_owned(), toolchain)), "{env:?}");
    }

    #[test]
    fn the_builds_own_output_is_checked_too() {
        let root = fix::canonical(&std::env::temp_dir());
        let roots = vec![root.clone()];
        let path = |n: &str| root.join(n).join("Cargo.toml");
        let art = |id: &str, n: &str, kind: &str| Message::Artifact {
            manifest: path(n),
            package_id: id.to_owned(),
            name: n.to_owned(),
            kinds: vec![kind.to_owned()],
            files: vec![],
            fresh: false,
        };
        let ok = art("path+file:///x/m#0.1.0", "m", "cdylib");
        assert_eq!(check_build_output(std::slice::from_ref(&ok), &roots), Ok(()));
        let reg = art("registry+https://github.com/rust-lang/crates.io-index#itoa@1.0.11", "itoa", "lib");
        assert_eq!(check_build_output(&[ok.clone(), reg], &roots).unwrap_err().0, "third_party_crate");
        let bs = Message::BuildScript { package_id: "path+file:///x/m#0.1.0".into() };
        assert_eq!(check_build_output(&[ok.clone(), bs], &roots).unwrap_err().0, "build_script");
        let pm = art("path+file:///x/d#0.1.0", "d", "proc-macro");
        assert_eq!(check_build_output(&[pm], &roots).unwrap_err().0, "proc_macro");
        let far = Message::Artifact {
            manifest: PathBuf::from("/far/away/Cargo.toml"),
            package_id: "path+file:///far/away#0.1.0".into(),
            name: "far".into(),
            kinds: vec!["lib".into()],
            files: vec![],
            fresh: false,
        };
        assert_eq!(check_build_output(&[far], &[root.join("specula-nowhere")]).unwrap_err().0, "outside_root");
    }

    #[test]
    fn workspace_root_is_found_upwards() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = workspace_root(here);
        assert!(root.join("crates").join("specula-logic").exists(), "{}", root.display());
    }
}
