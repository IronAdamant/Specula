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
/// Crates one build may compile (the module, the SDK, core and helpers); past it the build is
/// refused (a crate with no files has no dep-info).
const MAX_COMPILED: usize = 64;
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
    /// `compile_errors`, `build_path_too_long`, `fix_failed`.
    pub error: Option<(&'static str, String)>,
    /// With `build_path_too_long`: the file the linker could not open and its length in
    /// characters ([`unread_link_arguments`]).
    pub link_arguments: Option<(String, usize)>,
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
    /// Every crate the build compiled or found fresh (the module and its path dependencies):
    /// what [`check_compiled`] scans after the build.
    pub compiled: Vec<Compiled>,
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

/// One crate of a build, from Cargo's artifact message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Compiled {
    pub manifest: PathBuf,
    /// The library target's name.
    pub name: String,
    pub files: Vec<PathBuf>,
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
                let outcome = specula_core::trace::catch_unwind(std::panic::AssertUnwindSafe(|| runner(&req)));
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
    // The project's SDK is the engine's own, or the build is refused before Cargo runs, with
    // what differs and the command that puts it right (docs/design/FIRST_HOUR.md, slice 1).
    if let Some(why) = sdk_differs(&root) {
        return Err(("sdk_differs", why));
    }
    // Compile-time paths (`#[path]`, `include_str!`...) stay inside the module's src/.
    crate::paths::check_module(&dir)?;
    let lock = workspace_root(&dir).join("Cargo.lock");
    let lock_before = std::fs::read(&lock).ok();
    let restore = || match &lock_before {
        Some(bytes) => {
            if std::fs::read(&lock).ok().as_deref() != Some(bytes.as_slice()) {
                let _ = crate::disk::write(crate::disk::Kind::Project, &lock, bytes, true);
            }
        }
        None => {
            let _ = crate::disk::remove_file(crate::disk::Kind::Project, &lock);
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
    let checked = meta.and_then(|m| check_metadata(&m, &roots).and_then(|()| check_reach(&m, &dir.join("Cargo.toml"))));
    if checked.is_err() {
        restore();
    }
    checked
}

/// Engine crates that call the operating system (windows, input, the GPU, audio, sockets, the
/// loader). A module may not depend on them: through them it could read the window's keyboard
/// or the clipboard outside the engine's rules (docs/design/GAMEPLAY_MODULES.md).
pub const OS_CRATES: &[&str] = &["specula-sys", "specula-platform", "specula-input", "specula-gpu", "specula-audio", "specula-protocol", "specula-logic", "specula"];

/// Crates a module may always depend on without a source check (the SDK and what it re-exports).
const SDK_CRATES: &[&str] = &["specula-sdk", "specula-core"];

/// What a module can reach outside the engine's API, before Cargo runs: its normal dependencies
/// (not its dev-dependencies, which never reach the library) may not include an [`OS_CRATES`]
/// crate (`os_crate`). The sources are checked after the build, over every file rustc compiled
/// ([`check_compiled`]). A lint, not a sandbox: see the module docs.
pub fn check_reach(meta: &specula_core::Json, manifest: &Path) -> Result<(), (&'static str, String)> {
    use specula_core::Json;
    let manifest = fix::canonical(manifest);
    let packages = meta.get("packages").and_then(Json::as_array).unwrap_or_default();
    let pkg = |id: &str| packages.iter().find(|p| p.get("id").and_then(Json::as_str) == Some(id));
    let Some(module) = packages.iter().find(|p| p.get("manifest_path").and_then(Json::as_str).is_some_and(|m| fix::canonical(Path::new(m)) == manifest)) else {
        return Err(("metadata_failed", "cargo metadata doesn't list the module's own package".into()));
    };
    let module_id = module.get("id").and_then(Json::as_str).unwrap_or_default().to_owned();
    let nodes = meta.get("resolve").and_then(|r| r.get("nodes")).and_then(Json::as_array).unwrap_or_default();
    // The module's normal-dependency closure (a dependency counts if any of its kinds is normal).
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = vec![module_id.clone()];
    while let Some(id) = stack.pop() {
        if seen.len() >= 4096 {
            return Err(("metadata_failed", "the module's dependency graph has more than 4096 packages: too many to check".into()));
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(node) = nodes.iter().find(|n| n.get("id").and_then(Json::as_str) == Some(id.as_str())) else { continue };
        for d in node.get("deps").and_then(Json::as_array).unwrap_or_default() {
            let normal = d.get("dep_kinds").and_then(Json::as_array).unwrap_or_default().iter().any(|k| k.get("kind").is_none_or(|k| k.is_null()));
            if normal {
                if let Some(p) = d.get("pkg").and_then(Json::as_str) {
                    stack.push(p.to_owned());
                }
            }
        }
    }
    let name_of = |id: &str| pkg(id).and_then(|p| p.get("name")).and_then(Json::as_str).unwrap_or("?").to_owned();
    for id in &seen {
        let name = name_of(id);
        if *id != module_id && OS_CRATES.contains(&name.as_str()) {
            return Err((
                "os_crate",
                format!("the module depends on {name}, an engine crate that calls the operating system; modules reach the world, input and sound through specula-sdk's Cx only (depend on specula-sdk)"),
            ));
        }
    }
    Ok(())
}

/// Whether every file of the crate folder `copy` (build output apart) is, byte for byte, the file
/// of the same name in `original`: a copy of a crate, or of part of it, with nothing added and
/// nothing changed. A copy that changed `lib.rs` to leave a module out is not one.
fn same_files(copy: &Path, original: &Path) -> bool {
    if fix::canonical(copy) == fix::canonical(original) {
        return true;
    }
    let text = |p: &Path| std::fs::read(p).ok().map(|b| String::from_utf8_lossy(&b).replace("\r\n", "\n"));
    let mut stack = vec![PathBuf::new()];
    let mut seen = 0;
    while let Some(below) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(copy.join(&below)) else { return false };
        for entry in entries.flatten() {
            let rel = below.join(entry.file_name());
            if entry.path().is_dir() {
                if entry.file_name() != "target" {
                    stack.push(rel);
                }
                continue;
            }
            seen += 1;
            // (More files than either crate has: not a copy of one.)
            if seen > 256 || text(&copy.join(&rel)).is_none_or(|t| Some(t) != text(&original.join(&rel))) {
                return false;
            }
        }
    }
    seen > 0
}

/// The manifests of the crates a module may use without a source check: the engine's own
/// `specula-sdk` and `specula-core`. **Trusted by what they say**, never by name (any package
/// may call itself `specula-core`) and no longer by where they lie (anything written to
/// `crates/specula-sdk` under a project used to be compiled unchecked): the crates this engine
/// was built from; and under each build root, a copy that is the SDK the engine itself writes
/// ([`crate::sdk_source::is_the_engines`]) or a copy of the crates it was built from, file for
/// file.
pub fn trusted_manifests(roots: &[PathBuf]) -> Vec<PathBuf> {
    let built_from = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for c in SDK_CRATES {
        let m = built_from.join(c).join("Cargo.toml");
        if m.is_file() {
            out.push(fix::canonical(&m));
        }
    }
    for root in roots {
        let engines = crate::sdk_source::is_the_engines(root);
        for c in SDK_CRATES {
            let dir = root.join("crates").join(c);
            let m = dir.join("Cargo.toml");
            if m.is_file() && (engines || same_files(&dir, &built_from.join(c))) {
                out.push(fix::canonical(&m));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Why the SDK under a build root is not one the engine trusts, when it has one that is not: the
/// first differences, and the command that puts it right. `None` when the root has no SDK of its
/// own (a module in the engine's own repository) or has the engine's.
pub fn sdk_differs(root: &Path) -> Option<String> {
    let manifests: Vec<PathBuf> = SDK_CRATES.iter().map(|c| root.join("crates").join(c).join("Cargo.toml")).filter(|m| m.is_file()).collect();
    let trusted = trusted_manifests(&[root.to_path_buf()]);
    if manifests.iter().all(|m| trusted.contains(&fix::canonical(m))) {
        return None;
    }
    let differences = crate::sdk_source::differs(root);
    let shown: Vec<&str> = differences.iter().take(6).map(String::as_str).collect();
    let more = if differences.len() > shown.len() { format!(" and {} more", differences.len() - shown.len()) } else { String::new() };
    Some(format!(
        "the SDK under {} is not this engine's ({}{more}). A module is built against the engine's own SDK, version {}: `specula sdk` writes it into the project (it replaces crates/specula-sdk and crates/specula-core, and touches nothing else)",
        root.join("crates").display(),
        shown.join("; "),
        crate::sdk_source::VERSION
    ))
}

/// After a build: every file rustc read (its dep-info) for the module and for every other crate
/// the build compiled, except the engine's SDK and core ([`trusted_manifests`]), is scanned for
/// reaches around the engine's API ([`crate::lint::reach_scan`]): foreign functions, native
/// libraries, inline assembly, `unsafe`, and `std`'s files, processes, sockets and environment.
/// Any extension counts (`#[path = "imp.txt"]`, `include!`), wherever the crate's `[lib] path`
/// points; a crate with no dep-info, or files too many or too big to read, is refused too.
/// `Err(("native_code", message))` names the crate, file and line.
pub fn check_compiled(compiled: &[Compiled], roots: &[PathBuf], workspace: &Path) -> Result<(), (&'static str, String)> {
    let trusted = trusted_manifests(roots);
    let mut budget = crate::lint::ScanBudget::default();
    for c in compiled {
        if trusted.contains(&fix::canonical(&c.manifest)) {
            continue;
        }
        let refuse = |what: String| Err(("native_code", format!("{} ({}): {what}; the library isn't loaded (modules reach the machine only through specula-sdk's Cx)", c.name, c.manifest.display())));
        // (Every library the build made is answered for; more than one dep-info only when two
        // builds left the same library, and then every one is scanned.)
        let deps = crate::paths::dep_infos_of(&c.files, &c.name);
        if deps.is_empty() {
            return refuse("the build left no dep-info to say which files it compiled".into());
        }
        let mut files: Vec<PathBuf> = Vec::new();
        for dep in &deps {
            let Ok(text) = std::fs::read_to_string(dep) else {
                return refuse(format!("its dep-info {} can't be read", dep.display()));
            };
            let listed = crate::paths::dep_info_files(&text);
            if listed.is_empty() {
                return refuse(format!("its dep-info {} lists no files", dep.display()));
            }
            for f in listed {
                let p = PathBuf::from(f);
                let p = if p.is_absolute() { p } else { workspace.join(p) };
                if !files.contains(&p) {
                    files.push(p);
                }
            }
        }
        if let Some((file, r)) = crate::lint::first_reach(&files, &mut budget) {
            return Err((
                "native_code",
                format!(
                    "{}:{} ({}) {}: {}; modules may not call the operating system, native libraries or std's files, processes and sockets, nor use unsafe code (the library isn't loaded; use specula-sdk's Cx)",
                    file.display(),
                    r.line,
                    c.name,
                    r.rule,
                    r.what
                ),
            ));
        }
    }
    Ok(())
}

/// The first crate an approved build compiled that the proposal's digest doesn't cover: any crate
/// outside the module's folder `dir`, except the engine's SDK and core ([`trusted_manifests`]).
/// A proposed module is self-contained, so the folder's digest pins every source that is loaded
/// (a helper crate elsewhere could change after the review without the digest changing).
pub fn unpinned<'a>(compiled: &'a [Compiled], roots: &[PathBuf], dir: &Path) -> Option<&'a Compiled> {
    let trusted = trusted_manifests(roots);
    let dir = fix::canonical(dir);
    compiled.iter().find(|c| {
        let m = fix::canonical(&c.manifest);
        !trusted.contains(&m) && !m.starts_with(&dir)
    })
}

/// Module folders whose build is under way (canonical paths; a folder may be there twice).
static BUILDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static BUILDING_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A module folder held for a build: until it drops, [`building`] is true for every path in it.
pub struct Building(PathBuf);

/// Holds the module folder `dir` while its sources are digested, built, digested again and
/// loaded. The engine's own writers leave a held folder alone ([`building`]), so an agent can't
/// swap the sources for the build and put them back before the second digest.
pub fn hold(dir: &Path) -> Building {
    let dir = fix::canonical(dir);
    BUILDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(dir.clone());
    BUILDING_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Building(dir)
}

impl Drop for Building {
    fn drop(&mut self) {
        let mut held = BUILDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(at) = held.iter().position(|d| *d == self.0) {
            held.swap_remove(at);
            BUILDING_COUNT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Whether `path` (a file or folder, there or not) lies in a module folder being built.
pub fn building(path: &Path) -> bool {
    if BUILDING_COUNT.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        return false;
    }
    // The nearest ancestor that exists, canonical, with the rest put back (a new file has no
    // canonical path of its own).
    let mut rest = Vec::new();
    let mut at = path;
    let base = loop {
        if at.exists() {
            break fix::canonical(at);
        }
        match (at.parent(), at.file_name()) {
            (Some(p), Some(n)) => {
                rest.push(n.to_owned());
                at = p;
            }
            _ => break at.to_owned(),
        }
    };
    let full = rest.iter().rev().fold(base, |p, n| p.join(n));
    BUILDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|d| full.starts_with(d))
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

/// The longest path the Rust toolchain's own linker (a gcc) opens on Windows: the older file
/// API's 260, less the terminator.
pub const LINKER_PATH_MAX: usize = 259;

/// How long the path of rustc's file of linker arguments will be for a build of `package` into
/// the Cargo target `target`, as Windows counts it: `<target>\release\build\<package>\<16
/// hex>\out\rustcXXXXXX\linker-arguments`. The layout is Cargo's since Rust 1.100, the deeper
/// of the two it has used, and the profile's name the longer of the two: a build that fits by
/// this fits. Known before Cargo runs, so the engine can choose a folder the link will fit in
/// instead of refusing the build after it compiled (`build_path_too_long`).
pub fn link_arguments_chars(target: &Path, package: &str) -> usize {
    let below = [r"\release\build\", package, r"\0123456789abcdef\out\rustcXXXXXX\linker-arguments"];
    target.to_string_lossy().encode_utf16().count() + below.iter().map(|part| part.encode_utf16().count()).sum::<usize>()
}

/// A link that failed because the linker never read its arguments (found by M5's last audit:
/// one live check failed in an output folder one character deeper than the audit before it).
/// rustc hands a long list of linker arguments over in a file,
/// `<the crate's output folder>/rustcXXXXXX/linker-arguments`, and the toolchain's linker
/// cannot open a path past [`LINKER_PATH_MAX`]. It then takes `@<path>` for a file's name,
/// sees none of its arguments, and fails with words about something else: `'-fuse-linker-plugin',
/// but liblto_plugin.dll not found` (the argument that switches the plugin off was in the
/// file). Since Rust 1.100 the output folder is `build/<package>/<hash>/out/` where it was
/// `deps/`: 29 characters and the package's name nearer the limit.
///
/// `Some((the file, its length))` when `d` is such a failure: a link error whose notes show
/// that the linker saw no arguments, and whose argument file is past the limit (lengths in
/// UTF-16 units, as Windows counts). A link that failed for another reason, or in a
/// shallower folder, is `None`: it keeps its own diagnostics and nothing is guessed.
/// The path is read as rustc prints it, so one it prints escaped (characters outside ASCII)
/// may be counted longer than it is and one it prints otherwise than `C:\...` is not
/// recognised (review R4): such a build answers `compile_errors` with the linker's own
/// words, as every such build did before.
pub fn unread_link_arguments(d: &Diagnostic) -> Option<(String, usize)> {
    if !d.is_error() || !d.message.starts_with("linking with") {
        return None;
    }
    // The linker's own words: it did not see `-fno-use-linker-plugin`, or it names the file.
    let unseen = d.notes.iter().any(|n| (n.contains("liblto_plugin") && n.contains("not found")) || n.contains("linker-arguments"));
    if !unseen {
        return None;
    }
    // The link's own folder, from what rustc passes for every library it links there
    // (`symbols.o`; on windows-gnu a `.def` list as well). rustc prints each argument in
    // quotes with its backslashes doubled.
    let file = d.notes.iter().flat_map(|n| n.split('"')).find_map(|arg| {
        let arg = arg.strip_prefix("-Wl,").unwrap_or(arg).replace("\\\\", "\\");
        let folder = ["symbols.o", "list.def"].iter().find_map(|f| arg.strip_suffix(f))?;
        let folder = folder.strip_suffix(['\\', '/'])?;
        let name = folder.rsplit(['\\', '/']).next()?;
        (name.len() == "rustcXXXXXX".len() && name.starts_with("rustc")).then(|| format!("{folder}\\linker-arguments"))
    })?;
    let chars = file.encode_utf16().count();
    (chars > LINKER_PATH_MAX).then_some((file, chars))
}

/// Names what failed in a build whose diagnostics hold errors: the linker's unread arguments
/// when that is what it was ([`unread_link_arguments`]), else the count of compile errors.
fn name_errors(report: &mut BuildReport) {
    report.link_arguments = report.diagnostics.iter().find_map(unread_link_arguments);
    report.error = Some(match &report.link_arguments {
        Some((file, chars)) => (
            "build_path_too_long",
            format!(
                "the module compiled but was not linked: its build folder is too deep. rustc hands the linker its arguments in a file, here {file} ({chars} characters), and the Rust toolchain's linker opens a path of at most {LINKER_PATH_MAX} characters on Windows, so it read none of them (what the linker says in the diagnostics follows from that). Build in a folder at least {} character(s) less deep: move the project, or start the engine with --cache DIR in a shorter folder (module builds go under the cache)",
                chars - LINKER_PATH_MAX
            ),
        ),
        None => ("compile_errors", format!("{} compile error(s): see diagnostics", report.errors())),
    });
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
                    let _ = crate::disk::write(crate::disk::Kind::Project, &self.path, bytes, true);
                }
            }
            None => {
                let _ = crate::disk::remove_file(crate::disk::Kind::Project, &self.path);
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
        // (Every library the build made is answered for; more than one dep-info only when two
        // builds left the same library, and then every one must pass.)
        let deps = report.package.as_deref().map(|name| crate::paths::dep_infos_of(&report.artifacts, name)).unwrap_or_default();
        let checked = if deps.is_empty() {
            Err(("source_path", "the build left no dep-info to check which files it read; the module isn't loaded".to_owned()))
        } else {
            let root = workspace_root(&req.dir);
            deps.iter().try_for_each(|d| crate::paths::check_dep_info(d, &req.dir, &root))
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
    // What every compiled crate reaches (crate::lint::reach_scan over each crate's dep-info).
    if report.ok {
        if let Err((code, message)) = check_compiled(&report.compiled, &req.roots, &workspace_root(&req.dir)) {
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
        if let Message::Artifact { manifest, name, files, .. } = &message {
            if report.compiled.len() < MAX_COMPILED {
                report.compiled.push(Compiled { manifest: manifest.clone(), name: name.clone(), files: files.clone() });
            } else {
                // More crates than any module needs: refused below rather than half-checked.
                report.compiled.push(Compiled { manifest: PathBuf::new(), name: "(too many crates to check)".into(), files: Vec::new() });
                report.compiled.truncate(MAX_COMPILED + 1);
            }
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
        (Some(_), _) if report.errors() > 0 => name_errors(&mut report),
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

    /// M5's last audit: a module's link failed with words about a plugin, because the file
    /// rustc handed the linker its arguments in was at a path of 260 characters. The notes
    /// below are that build's, with the folder's depth as the variable.
    #[test]
    fn the_links_argument_file_is_as_long_as_cargo_makes_it() {
        // (The path M5's last audit met, with its real names' lengths.)
        let target = Path::new(r"D:\x\.specula-cache\modules\ambulo-8c5d3caf");
        let real = r"D:\x\.specula-cache\modules\ambulo-8c5d3caf\release\build\ambulo\0123456789abcdef\out\rustcXXXXXX\linker-arguments";
        assert_eq!(link_arguments_chars(target, "ambulo"), real.len());
        assert_eq!(link_arguments_chars(Path::new("X"), "ambulo"), 72);
    }

    #[test]
    fn a_link_that_never_read_its_arguments_is_named_for_its_folders_depth() {
        let link_error = |folder: &str, said: &str| {
            let quoted = folder.replace('\\', "\\\\");
            Diagnostic {
                level: "error".into(),
                code: None,
                message: "linking with `x86_64-w64-mingw32-gcc` failed: exit code: 1".into(),
                file: None,
                line: 0,
                column: 0,
                end_line: 0,
                end_column: 0,
                label: None,
                notes: vec![
                    format!("note: \"x86_64-w64-mingw32-gcc\" \"-Wl,{quoted}\\\\list.def\" \"-fno-use-linker-plugin\" \"-Wl,--dynamicbase\" \"{quoted}\\\\symbols.o\" \"<16 object files omitted>\" \"-shared\""),
                    "note: some arguments are omitted. use `--verbose` to show all linker arguments".into(),
                    format!("note: {said}"),
                ],
                fixes: Vec::new(),
                rendered: None,
                package: None,
            }
        };
        let plugin = "x86_64-w64-mingw32-gcc: fatal error: '-fuse-linker-plugin', but liblto_plugin.dll not found\ncompilation terminated.\r\n";
        // `<folder>\linker-arguments` is the folder's length and 17 more.
        let folder = |chars: usize| {
            let tail = "\\debug\\build\\ambulo/cead12802eafbb58\\out\\rustc50vqeF";
            format!("C:\\{}{tail}", "p".repeat(chars - 3 - tail.len()))
        };
        let (file, chars) = unread_link_arguments(&link_error(&folder(243), plugin)).expect("260 characters is one past what the linker opens");
        assert_eq!(chars, 260);
        assert!(file.ends_with("\\out\\rustc50vqeF\\linker-arguments") && file.starts_with("C:\\ppp"), "{file}");
        assert_eq!(unread_link_arguments(&link_error(&folder(242), plugin)), None, "259 characters is opened: whatever failed, it was not this");
        // A deep folder alone is no reason: the linker read its arguments and said what it missed.
        assert_eq!(unread_link_arguments(&link_error(&folder(300), "ld.exe: undefined reference to `missing'")), None);
        // A linker that names the file it could not open.
        let named = "gcc.exe: error: @C:\\p\\rustc50vqeF\\linker-arguments: No such file or directory";
        assert_eq!(unread_link_arguments(&link_error(&folder(300), named)).map(|(_, n)| n), Some(317));
        // Not a link, not an error, or a folder that is not rustc's own for a link.
        let mut other = link_error(&folder(300), plugin);
        other.message = "mismatched types".into();
        assert_eq!(unread_link_arguments(&other), None);
        let mut warning = link_error(&folder(300), plugin);
        warning.level = "warning".into();
        assert_eq!(unread_link_arguments(&warning), None);
        let elsewhere = format!("C:\\{}\\out\\objects", "p".repeat(300));
        assert_eq!(unread_link_arguments(&link_error(&elsewhere, plugin)), None);

        // What the build's answer says: its own code, the numbers, and what to do.
        let mut report = BuildReport { diagnostics: vec![link_error(&folder(243), plugin)], ..BuildReport::default() };
        name_errors(&mut report);
        let (code, message) = report.error.clone().unwrap();
        assert_eq!(code, "build_path_too_long");
        assert!(message.contains("(260 characters)") && message.contains("at most 259") && message.contains("at least 1 character(s) less deep") && message.contains("--cache DIR"), "{message}");
        assert_eq!(report.link_arguments.as_ref().map(|l| l.1), Some(260));
        // Any other failed build is its compile errors, as before.
        let mut report = BuildReport { diagnostics: vec![link_error(&folder(242), plugin), other], ..BuildReport::default() };
        name_errors(&mut report);
        assert_eq!(report.error.as_ref().map(|e| e.0), Some("compile_errors"));
        assert!(report.error.as_ref().unwrap().1.starts_with("2 compile error(s)"));
        assert_eq!(report.link_arguments, None);
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

    /// A module may not reach the OS through an engine crate; its dev-dependencies (tests) don't
    /// count.
    #[test]
    fn modules_may_not_depend_on_os_crates() {
        let base = std::env::temp_dir().join(format!("specula-reach-vet-{}", std::process::id()));
        for d in ["m/src", "helper/src"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        std::fs::write(base.join("m/Cargo.toml"), "").unwrap();
        std::fs::write(base.join("m/src/lib.rs"), "pub fn tick() {}\n").unwrap();
        std::fs::write(base.join("helper/src/lib.rs"), "pub fn x() {}\n").unwrap();
        let path = |p: &str| base.join(p).join("Cargo.toml").to_string_lossy().replace('\\', "/");
        let meta = |deps: &str| {
            let text = format!(
                r#"{{"packages": [
                    {{"id": "m", "name": "m", "manifest_path": "{}"}},
                    {{"id": "sdk", "name": "specula-sdk", "manifest_path": "/x/sdk/Cargo.toml"}},
                    {{"id": "eng", "name": "specula", "manifest_path": "/x/eng/Cargo.toml"}},
                    {{"id": "plat", "name": "specula-platform", "manifest_path": "/x/plat/Cargo.toml"}},
                    {{"id": "helper", "name": "helper", "manifest_path": "{}"}}],
                  "resolve": {{"nodes": [{{"id": "m", "deps": [{deps}]}}, {{"id": "helper", "deps": []}}]}}}}"#,
                path("m"),
                path("helper")
            );
            specula_core::Json::parse(&text).unwrap()
        };
        let normal = |p: &str| format!(r#"{{"pkg": "{p}", "dep_kinds": [{{"kind": null}}]}}"#);
        let dev = |p: &str| format!(r#"{{"pkg": "{p}", "dep_kinds": [{{"kind": "dev"}}]}}"#);
        let manifest = base.join("m/Cargo.toml");
        assert_eq!(check_reach(&meta(&[normal("sdk"), dev("eng")].join(",")), &manifest), Ok(()), "tests may link the engine");
        assert_eq!(check_reach(&meta(&normal("plat")), &manifest).unwrap_err().0, "os_crate");
        assert_eq!(check_reach(&meta(&[normal("sdk"), normal("eng")].join(",")), &manifest).unwrap_err().0, "os_crate");
        assert_eq!(check_reach(&meta(&normal("helper")), &manifest), Ok(()));
        // Sources are checked after the build (check_compiled), whatever the metadata says.
        std::fs::write(base.join("helper/src/lib.rs"), "#[link(name = \"user32\")]
extern \"system\" { fn GetAsyncKeyState(k: i32) -> i16; }
").unwrap();
        assert_eq!(check_reach(&meta(&normal("helper")), &manifest), Ok(()), "no source scan before the build");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// After the build: every compiled crate but the engine's SDK and core (trusted by canonical
    /// path, never by name) is scanned over the files its dep-info lists, whatever their
    /// extension; no dep-info, or too many crates, is refused.
    #[test]
    fn compiled_crates_are_scanned_except_the_trusted_sdk() {
        let base = std::env::temp_dir().join(format!("specula-compiled-{}", std::process::id()));
        let deps = base.join("target/debug/deps");
        std::fs::create_dir_all(&deps).unwrap();
        std::fs::create_dir_all(base.join("m/src")).unwrap();
        std::fs::write(base.join("m/src/lib.rs"), "#[path = \"imp.txt\"] mod imp;
").unwrap();
        std::fs::write(base.join("m/src/imp.txt"), "pub fn ok() {}
").unwrap();
        let rmeta = deps.join("libm-0123456789abcdef.rmeta");
        std::fs::write(&rmeta, "").unwrap();
        let dep = |files: &[&Path]| {
            let list: Vec<String> = files.iter().map(|f| f.to_string_lossy().replace(' ', "\\ ")).collect();
            std::fs::write(deps.join("m-0123456789abcdef.d"), format!("{}: {}
", rmeta.display(), list.join(" "))).unwrap();
        };
        let (lib, imp) = (base.join("m/src/lib.rs"), base.join("m/src/imp.txt"));
        dep(&[&lib, &imp]);
        let m = Compiled { manifest: base.join("m/Cargo.toml"), name: "m".into(), files: vec![rmeta.clone()] };
        let roots = vec![base.clone()];
        assert_eq!(check_compiled(std::slice::from_ref(&m), &roots, &base), Ok(()));
        std::fs::write(&imp, "extern \"system\" { fn GetAsyncKeyState(k: i32) -> i16; }
").unwrap();
        let e = check_compiled(std::slice::from_ref(&m), &roots, &base).unwrap_err();
        assert!(e.0 == "native_code" && e.1.contains("imp.txt"), "{e:?}");
        // The engine's SDK by path is trusted; a crate merely named specula-core is not.
        let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../specula-sdk/Cargo.toml");
        let trusted = Compiled { manifest: sdk, name: "specula_sdk".into(), files: vec![rmeta.clone()] };
        assert_eq!(check_compiled(&[trusted], &roots, &base), Ok(()));
        let named = Compiled { manifest: base.join("m/Cargo.toml"), name: "specula-core".into(), files: vec![rmeta.clone()] };
        assert_eq!(check_compiled(&[named], &roots, &base).unwrap_err().0, "native_code");
        // No dep-info to say what was compiled: refused.
        let lost = Compiled { manifest: base.join("x/Cargo.toml"), name: "x".into(), files: vec![] };
        assert!(check_compiled(&[lost], &roots, &base).unwrap_err().1.contains("dep-info"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review R2 of m5-gmods: an approval pins the module folder's digest, so an approved build
    /// may compile nothing outside that folder but the engine's SDK and core (by path).
    #[test]
    fn an_approved_build_compiles_only_what_its_digest_covers() {
        let base = std::env::temp_dir().join(format!("specula-unpinned-{}", std::process::id()));
        for d in ["m/src", "m/inner/src", "helper/src", "m2/src"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
            std::fs::write(base.join(d).join("../Cargo.toml"), "").unwrap();
        }
        let c = |p: &str, name: &str| Compiled { manifest: base.join(p).join("Cargo.toml"), name: name.into(), files: vec![] };
        let sdk = Compiled { manifest: Path::new(env!("CARGO_MANIFEST_DIR")).join("../specula-sdk/Cargo.toml"), name: "specula_sdk".into(), files: vec![] };
        let roots = vec![base.clone()];
        let dir = base.join("m");
        assert!(unpinned(&[c("m", "m"), c("m/inner", "inner"), sdk.clone()], &roots, &dir).is_none(), "the module, a crate inside it and the SDK");
        assert_eq!(unpinned(&[c("m", "m"), sdk.clone(), c("helper", "helper")], &roots, &dir).map(|c| c.name.as_str()), Some("helper"), "a helper next to the module");
        assert_eq!(unpinned(&[c("m2", "m2")], &roots, &dir).map(|c| c.name.as_str()), Some("m2"), "a folder whose name starts the same isn't inside");
        // Trust is by path: a crate that calls itself specula-sdk elsewhere is not pinned.
        assert_eq!(unpinned(&[c("helper", "specula-sdk")], &roots, &dir).map(|c| c.name.as_str()), Some("specula-sdk"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review R2 of m5-gmods: while a module folder is held for a build, every path in it reads
    /// as building (the engine's writers leave it alone), and nothing else does.
    #[test]
    fn a_held_module_folder_covers_its_paths_until_released() {
        let base = std::env::temp_dir().join(format!("specula-building-{}", std::process::id()));
        std::fs::create_dir_all(base.join("m/src")).unwrap();
        std::fs::create_dir_all(base.join("m2/src")).unwrap();
        std::fs::write(base.join("m/src/lib.rs"), "").unwrap();
        assert!(!building(&base.join("m/src/lib.rs")));
        let held = hold(&base.join("m"));
        assert!(building(&base.join("m/src/lib.rs")), "a file there");
        assert!(building(&base.join("m/src/new/deep.rs")), "a file that doesn't exist yet");
        assert!(building(&base.join("m")), "the folder itself");
        assert!(building(&base.join("m/src/../src/lib.rs")), "by another spelling");
        assert!(!building(&base.join("m2/src/lib.rs")), "a folder whose name starts the same");
        assert!(!building(&base.join("other.rs")));
        let again = hold(&base.join("m"));
        drop(held);
        assert!(building(&base.join("m/src/lib.rs")), "held twice: still held");
        drop(again);
        assert!(!building(&base.join("m/src/lib.rs")), "released");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The SDK is trusted by what it says (docs/design/FIRST_HOUR.md, slice 1): the copy the
    /// engine writes, and a copy of the crates this engine was built from; a copy that says
    /// anything else is checked like any other crate, and the build is refused before Cargo
    /// runs with what differs.
    #[test]
    fn the_sdk_under_a_project_is_trusted_by_what_it_says() {
        let base = std::env::temp_dir().join(format!("specula-sdk-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let manifests = |root: &Path| SDK_CRATES.iter().map(|c| fix::canonical(&root.join("crates").join(c).join("Cargo.toml"))).collect::<Vec<_>>();
        let trusted = |root: &Path| trusted_manifests(&[root.to_path_buf()]);

        // No SDK of its own: nothing to refuse (a module in the engine's own repository).
        let none = base.join("none");
        std::fs::create_dir_all(&none).unwrap();
        assert_eq!(sdk_differs(&none), None);

        // The SDK the engine writes.
        let written = base.join("written");
        crate::sdk_source::write_into(&written).unwrap();
        assert!(manifests(&written).iter().all(|m| trusted(&written).contains(m)));
        assert_eq!(sdk_differs(&written), None);

        // One line added to it: neither crate is trusted, and the refusal names the file.
        let abi = written.join("crates/specula-sdk/src/abi.rs");
        let text = std::fs::read_to_string(&abi).unwrap();
        std::fs::write(&abi, format!("{text}\nextern \"C\" {{ fn system(c: *const u8) -> i32; }}\n")).unwrap();
        assert!(manifests(&written).iter().all(|m| !trusted(&written).contains(m)), "a changed SDK is checked like any crate");
        let why = sdk_differs(&written).expect("refused");
        assert!(why.contains("crates/specula-sdk/src/abi.rs: not the engine's text") && why.contains("specula sdk"), "{why}");

        // A copy of the crates this engine was built from (what the engine's own tests make).
        let built_from = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let copied = base.join("copied");
        for c in SDK_CRATES {
            let (from, to) = (built_from.join(c), copied.join("crates").join(c));
            std::fs::create_dir_all(to.join("src")).unwrap();
            std::fs::copy(from.join("Cargo.toml"), to.join("Cargo.toml")).unwrap();
            for entry in std::fs::read_dir(from.join("src")).unwrap().flatten().filter(|e| e.path().is_file()) {
                std::fs::copy(entry.path(), to.join("src").join(entry.file_name())).unwrap();
            }
        }
        assert!(manifests(&copied).iter().all(|m| trusted(&copied).contains(m)), "{:?}", sdk_differs(&copied));
        assert_eq!(sdk_differs(&copied), None);
        // ...until a file is added to it, or one is changed.
        std::fs::write(copied.join("crates/specula-core/src/extra.rs"), "pub fn x() {}").unwrap();
        assert!(!trusted(&copied).contains(&manifests(&copied)[1]), "a file the engine's crate does not have");
        assert!(trusted(&copied).contains(&manifests(&copied)[0]), "the other crate is still a copy");
        assert!(sdk_differs(&copied).is_some());
        let _ = std::fs::remove_dir_all(&base);
    }
}
