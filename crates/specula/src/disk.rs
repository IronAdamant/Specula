//! The disk footprint budget (owner, 2026-10-22: sort it out "before it impacts someone else's
//! system ... especially when they have even less space"). Design: docs/design/DISK.md.
//!
//! - **One accounting point.** Everything the engine writes goes through [`book`]: [`DiskBook::write`]
//!   (admit, write, account) or, for writers that write their own way (atomic world saves, Cargo),
//!   [`DiskBook::admit`] before and [`DiskBook::wrote`] after. `crates/specula/tests/disk_guard.rs`
//!   fails when engine code writes a file anywhere else.
//! - **Measured cheaply.** Each category's folders are measured by a background walker, a slice of
//!   entries at a time (never a full recursive walk per call), and writes made through the book move
//!   the numbers at once. Free space is read from the OS (`specula_sys::disk`), cached for a moment.
//! - **A budget** from `perf.toml [disk]`: a total for the engine's output, caps per category and a
//!   free-space floor. A write that crosses the soft line (`warn_percent`) is answered with a
//!   structured `disk_warning`; one that would leave less than the floor free is refused
//!   (`disk_floor`, with the numbers and what to delete). Over budget, the walker prunes: the oldest
//!   captures first, then cache entries (unreferenced content), then expired scratch folders.
//! - **Never pruned:** saves, world files and journals (review evidence lives there: submissions and
//!   critiques keep copies of their captures beside the queue, see `evidence`), project files,
//!   another running engine's captures, scratch folders still leased, captures younger than
//!   `capture_min_age_s`.
//! - **Only what helps.** A prune frees at most what is actually prunable (old captures, content a
//!   pruner can give back, expired scratch), and doesn't run again until usage grows past what the
//!   last one left.
//! - **Deleting** stays inside the engine's own roots: paths are built from the canonical root,
//!   every step is checked to be a real folder (no symlink, junction or other reparse point), and
//!   links met inside a folder being removed are removed as links, never followed.
//!
//! Wall-clock time and threads are fine here: nothing in the simulation depends on the disk.

use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MB: u64 = 1 << 20;
/// Free space is read again after this long (or after a large admitted write).
const SPACE_FRESH: Duration = Duration::from_secs(2);
/// Entries the walker looks at per slice, and the pause between slices.
const SLICE: usize = 2_000;
const SLICE_PAUSE: Duration = Duration::from_millis(250);
/// A category is measured again this long after its last walk finished.
const REWALK: Duration = Duration::from_secs(30);
/// Entries one walk (or one prune's candidate list) looks at, at most; deeper than this is not walked.
const WALK_ENTRIES: usize = 250_000;
const WALK_DEPTH: usize = 32;
/// Over budget, pruning goes down to this share of the line it crossed (so it doesn't run per write).
const PRUNE_TO_PERCENT: u64 = 85;
/// Expired scratch folders are looked for this often.
const SCRATCH_EVERY: Duration = Duration::from_secs(60);
/// Agents' full measures run at most this often (others get the walker's numbers).
const MEASURE_EVERY: Duration = Duration::from_secs(10);
/// Background pruning runs at most this often, and so do agents' prunes (`disk_prune`).
const PRUNE_COOLDOWN: Duration = Duration::from_secs(30);
const AGENT_PRUNE_EVERY: Duration = Duration::from_secs(10);
/// Prune records kept for reports.
const PRUNE_LOG: usize = 32;
/// Sessions remembered as told about the current warning.
const MAX_TOLD: usize = 1024;
/// A scratch folder's marker file (its lease and when it ends).
pub const LEASE_MARKER: &str = ".specula-lease";
/// Where a hosted game writes its recordings, under the engine's working directory (Sepulcra's
/// `play_record`): the `recordings` category, with playtest replays.
pub const RECORDINGS_DIR: &str = "recordings";

/// What the engine writes, by where it goes and whether it may be pruned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// Sight frames and packets, studio sheets, VFX previews, procgen images, desync traces,
    /// perf captures, Lex exports (`--captures`).
    Captures,
    /// The pipeline cache and other rebuildable caches (`--cache`).
    Cache,
    /// The content store of binary project files' versions and uploads in progress.
    Content,
    /// Module builds' Cargo targets and the loaded copies of module libraries.
    Modules,
    /// Scratch-folder leases.
    Scratch,
    /// Save games (`--saves`).
    Saves,
    /// World files, the task board and review queue beside them (`--worlds`).
    Worlds,
    /// Mounted project files (the store's own size; their folders are the project's, never walked).
    Project,
    /// Playtest replays (`<cwd>/playtests`) and the hosted game's recordings (Sepulcra's
    /// `play_record`, `<cwd>/recordings`): evidence and replays, measured, never pruned.
    Recordings,
    /// The hosted game's player settings (`--settings`, by default the per-user settings folder
    /// with a window): one small file, never pruned.
    Settings,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Captures,
        Category::Cache,
        Category::Content,
        Category::Modules,
        Category::Scratch,
        Category::Saves,
        Category::Worlds,
        Category::Project,
        Category::Recordings,
        Category::Settings,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Captures => "captures",
            Category::Cache => "cache",
            Category::Content => "content",
            Category::Modules => "modules",
            Category::Scratch => "scratch",
            Category::Saves => "saves",
            Category::Worlds => "worlds",
            Category::Project => "project",
            Category::Recordings => "recordings",
            Category::Settings => "settings",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.name() == s)
    }

    /// Whether the engine may delete what it wrote here when over budget.
    pub fn prunable(self) -> bool {
        matches!(self, Category::Captures | Category::Content | Category::Scratch)
    }

    /// What an agent can do about this category's bytes.
    fn advice(self) -> &'static str {
        match self {
            Category::Captures => "disk_prune removes the oldest captures (never ones a review names)",
            Category::Cache => "the pipeline cache is rebuilt when missing: delete the cache folder's files by hand if it must go (never pruned)",
            Category::Content => "disk_prune removes content no file or kept journal entry needs",
            Category::Modules => "logic_build drops a module's stale artefacts; delete a module's target under the cache's modules folder when it is no longer built",
            Category::Scratch => "lease_release {delete: true} removes your scratch folders; expired ones go after their grace period",
            Category::Saves => "save_delete removes save games you no longer need (never pruned)",
            Category::Worlds => "world folders are never pruned: delete worlds you no longer need by hand",
            Category::Project => "project files are never pruned: project_delete removes files you no longer need",
            Category::Recordings => "playtest replays and game recordings are evidence, never pruned: delete ones you no longer need from playtests/ and recordings/ by hand",
            Category::Settings => "the player's settings file is the player's: never pruned (a few KiB)",
        }
    }
}

/// Who a write is for: an agent's write is refused below the floor; an essential one (the world
/// autosave, the task board) is written anyway and reported, since losing it costs more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Agent,
    Essential,
}

/// `perf.toml [disk]`.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskConfig {
    /// `total_mb`: the engine's own output in all (every category), default 16 GiB.
    pub total_mb: u64,
    /// `free_floor_mb`: agents' writes are refused when they would leave less free (default 2 GiB).
    pub free_floor_mb: u64,
    /// `warn_percent`: a write past this share of the total or of a cap is warned about (default 90).
    pub warn_percent: u64,
    /// `<category>_mb`: per-category caps (`captures_mb`, `content_mb`...).
    pub caps_mb: BTreeMap<Category, u64>,
    /// `scratch_grace_s`: an ended scratch lease's folder is removed this long after it ended
    /// (default an hour: its owner may still be reading it).
    pub scratch_grace_s: u64,
    /// `capture_min_age_s`: captures younger than this are never pruned (default 5 minutes: an
    /// agent is still looking at them, or about to name them in a review).
    pub capture_min_age_s: u64,
}

impl Default for DiskConfig {
    fn default() -> DiskConfig {
        DiskConfig {
            total_mb: 16 * 1024,
            free_floor_mb: 2 * 1024,
            warn_percent: 90,
            caps_mb: [(Category::Captures, 4 * 1024), (Category::Cache, 1024), (Category::Content, 2 * 1024), (Category::Modules, 8 * 1024), (Category::Scratch, 8 * 1024)].into(),
            scratch_grace_s: 3600,
            capture_min_age_s: 300,
        }
    }
}

impl DiskConfig {
    pub fn parse(v: &Json) -> Result<DiskConfig, String> {
        let mut cfg = DiskConfig::default();
        let mb = |k: &str, x: &Json| x.as_u64().filter(|n| (1..=1 << 24).contains(n)).ok_or(format!("[disk] {k} must be a whole number of megabytes, 1..16777216"));
        for (k, x) in v.as_object().ok_or("[disk] must be a table")? {
            match k.as_str() {
                "total_mb" => cfg.total_mb = mb(k, x)?,
                "free_floor_mb" => cfg.free_floor_mb = x.as_u64().filter(|n| *n <= 1 << 30).ok_or("[disk] free_floor_mb must be a whole number of megabytes (0 turns the floor off)")?,
                "warn_percent" => cfg.warn_percent = x.as_u64().filter(|n| (50..=99).contains(n)).ok_or("[disk] warn_percent must be 50..99")?,
                "scratch_grace_s" => cfg.scratch_grace_s = x.as_u64().filter(|n| *n <= 7 * 86_400).ok_or("[disk] scratch_grace_s must be 0..604800 seconds")?,
                "capture_min_age_s" => cfg.capture_min_age_s = x.as_u64().filter(|n| *n <= 86_400).ok_or("[disk] capture_min_age_s must be 0..86400 seconds")?,
                other => match other.strip_suffix("_mb").and_then(Category::parse) {
                    Some(c) => {
                        cfg.caps_mb.insert(c, mb(k, x)?);
                    }
                    None => {
                        return Err(format!(
                            "unknown key '{other}' in [disk] (known: total_mb, free_floor_mb, warn_percent, scratch_grace_s, capture_min_age_s, and a cap per category: {})",
                            Category::ALL.map(|c| format!("{}_mb", c.name())).join(", ")
                        ))
                    }
                },
            }
        }
        Ok(cfg)
    }

    fn floor(&self) -> u64 {
        self.free_floor_mb * MB
    }
}

/// Why a write was refused (or failed): a code agents branch on, a message, and the numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskError {
    /// `disk_floor` (refused: too little free space) or `io`.
    pub code: &'static str,
    pub message: String,
    pub detail: Vec<(&'static str, Json)>,
}

impl DiskError {
    fn io(path: &Path, e: &std::io::Error) -> DiskError {
        DiskError { code: "io", message: format!("could not write {}: {e}", path.display()), detail: vec![("path", Json::from(path.display().to_string()))] }
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        f.extend(self.detail.iter().cloned());
        Json::obj(f)
    }

    /// As a world error (for tools that answer those).
    pub fn to_world(&self) -> crate::world::WorldError {
        crate::world::WorldError { code: self.code, message: self.message.clone(), details: self.detail.clone() }
    }
}

impl std::fmt::Display for DiskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DiskError {}

/// One category's use.
#[derive(Clone, Debug, Default)]
struct Usage {
    /// What the last finished walk found.
    measured: u64,
    files: u64,
    /// Written (positive) and removed (negative) through the book since that walk began.
    delta: i64,
    delta_files: i64,
    walked: Option<Instant>,
    /// The last walk ran out of entries: the numbers are a lower bound.
    truncated: bool,
}

impl Usage {
    fn bytes(&self) -> u64 {
        (self.measured as i64).saturating_add(self.delta).max(0) as u64
    }

    fn file_count(&self) -> u64 {
        (self.files as i64).saturating_add(self.delta_files).max(0) as u64
    }
}

/// A walk in progress.
struct Walk {
    cat: Category,
    stack: Vec<(PathBuf, usize)>,
    bytes: u64,
    files: u64,
    entries: usize,
    truncated: bool,
}

/// What was pruned, for reports.
#[derive(Clone, Debug)]
struct PruneRecord {
    at_unix: u64,
    why: String,
    files: u64,
    bytes: u64,
    by_category: Vec<(Category, u64, u64)>,
}

/// A cache that can give bytes back (the content store): asked for `need` bytes, it removes
/// entries nothing references (oldest first) and says what it freed `(files, bytes)`.
pub type Pruner = Arc<dyn Fn(u64, bool) -> (u64, u64) + Send + Sync>;

#[derive(Default)]
struct State {
    configured: bool,
    config: DiskConfig,
    /// Each category's folders, canonical (the first is where its writes go).
    roots: BTreeMap<Category, Vec<PathBuf>>,
    usage: BTreeMap<Category, Usage>,
    /// Bytes and files written this run, by writer (`sight`, `studio`, `vfx`...).
    by_source: BTreeMap<&'static str, (u64, u64)>,
    /// Free space per root, when read, and bytes admitted since (subtracted from the estimate).
    space: BTreeMap<PathBuf, (Result<specula_sys::disk::Space, specula_sys::disk::DiskError>, Instant, u64)>,
    /// Total use a prune left still over budget (what is left can't be pruned): no prune runs
    /// again until use grows past it.
    unhelped_at: Option<u64>,
    /// Scratch folders whose lease is live in this engine (never removed, whatever their marker says).
    live_scratch: BTreeSet<PathBuf>,
    /// When an agent last pruned (`disk_prune`).
    last_agent_prune: Option<Instant>,
    /// This run's capture folder.
    current_run: Option<PathBuf>,
    pruners: Vec<(Category, Pruner)>,
    walk: Option<Walk>,
    last_scratch: Option<Instant>,
    last_prune: Option<Instant>,
    /// Over budget (or below the warning line of free space): the walker prunes next.
    prune_wanted: bool,
    pruned: VecDeque<PruneRecord>,
    pruned_total: (u64, u64),
    refused: u64,
    /// Essential writes made below the floor (written anyway).
    essential_below_floor: u64,
    /// The current warning: its sequence number (bumped when it gets worse) and text.
    warning: Option<(u64, Json, BTreeSet<String>)>,
    warn_seq: u64,
    told: BTreeMap<String, u64>,
    project_bytes: (u64, u64),
    /// When an agent last asked for a full measure (`disk_status {measure}`): at most one per
    /// [`MEASURE_EVERY`], so a loop of calls can't keep the disk busy.
    last_measure: Option<Instant>,
}

/// The engine's one accounting point for disk writes (see the module docs).
#[derive(Default)]
pub struct DiskBook {
    state: Mutex<State>,
    wake: Condvar,
    walker: OnceLock<()>,
    /// Held while a prune runs (one at a time).
    pruning: Mutex<()>,
}

/// The engine's book (one per process, like the module build service).
pub fn book() -> &'static DiskBook {
    static BOOK: OnceLock<DiskBook> = OnceLock::new();
    BOOK.get_or_init(DiskBook::default)
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn mb(bytes: u64) -> Json {
    Json::from((bytes as f64 / MB as f64 * 10.0).round() / 10.0)
}

/// Canonical when it exists; otherwise the nearest existing ancestor's canonical form with the
/// rest appended (a root the engine will create).
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(c) = path.canonicalize() {
        return c;
    }
    let mut rest = Vec::new();
    let mut at = path;
    while let Some(parent) = at.parent() {
        if let Some(n) = at.file_name() {
            rest.push(n.to_owned());
        }
        if let Ok(c) = parent.canonicalize() {
            let mut out = c;
            out.extend(rest.iter().rev());
            return out;
        }
        at = parent;
    }
    path.to_path_buf()
}

impl DiskBook {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Sets the budget and the folders each category writes to, and starts the walker (once).
    /// `current_run` is this run's capture folder.
    pub fn configure(&'static self, config: DiskConfig, roots: Vec<(Category, PathBuf)>, current_run: Option<PathBuf>) {
        self.configure_local(config, roots, current_run);
        if self.walker.set(()).is_ok() {
            let started = std::thread::Builder::new().name("specula-disk".into()).spawn(move || self.walk_forever());
            if let Err(e) = started {
                eprintln!("specula: disk use won't be measured or pruned in the background: {e}");
            }
        }
    }

    /// [`DiskBook::configure`] without the walker thread (tests drive [`DiskBook::step`]).
    pub fn configure_local(&self, config: DiskConfig, roots: Vec<(Category, PathBuf)>, current_run: Option<PathBuf>) {
        let mut st = self.lock();
        st.configured = true;
        st.config = config;
        st.roots.clear();
        for (c, r) in roots {
            let r = canonical(&r);
            let list = st.roots.entry(c).or_default();
            if !list.contains(&r) {
                list.push(r);
            }
        }
        st.current_run = current_run.map(|r| canonical(&r));
        st.space.clear();
        st.walk = None;
        st.usage.clear();
        st.prune_wanted = true;
    }

    /// The folder `cat` writes to (`None` when not configured).
    pub fn root(&self, cat: Category) -> Option<PathBuf> {
        self.lock().roots.get(&cat).and_then(|r| r.first().cloned())
    }

    /// Adds a pruner for a cache category (the content store).
    pub fn add_pruner(&self, cat: Category, pruner: Pruner) {
        self.lock().pruners.push((cat, pruner));
    }

    /// The scratch folders leased now in this engine (canonical): never removed.
    pub fn set_live_scratch(&self, dirs: BTreeSet<PathBuf>) {
        self.lock().live_scratch = dirs;
    }

    /// Reads free space for `roots` whose reading is stale (or all, `force`), outside the book's
    /// lock: the OS call never holds up a writer.
    fn refresh_space(&self, only: Option<&Path>, force: bool) {
        let stale: Vec<PathBuf> = {
            let st = self.lock();
            let roots: Vec<PathBuf> = match only {
                Some(r) => vec![r.to_path_buf()],
                None => st.roots.values().filter_map(|r| r.first().cloned()).collect(),
            };
            roots.into_iter().filter(|r| force || st.space.get(r).is_none_or(|(_, at, _)| at.elapsed() >= SPACE_FRESH)).collect()
        };
        if stale.is_empty() {
            return;
        }
        let read: Vec<(PathBuf, Result<specula_sys::disk::Space, specula_sys::disk::DiskError>)> = stale.into_iter().map(|r| {
            let s = specula_sys::disk::space(&r);
            (r, s)
        }).collect();
        let mut st = self.lock();
        for (r, s) in read {
            st.space.insert(r, (s, Instant::now(), 0));
        }
    }

    /// The project store's size `(bytes, files)` (its folders are the project's: never walked).
    pub fn set_project(&self, bytes: u64, files: u64) {
        self.lock().project_bytes = (bytes, files);
    }

    /// Checks that `bytes` more may be written for `cat`: refused (`disk_floor`) when an agent's
    /// write would leave less than the floor free on its volume. Never blocks on the disk: free
    /// space is cached for a moment and lowered by what was admitted since.
    pub fn admit(&self, cat: Category, bytes: u64, need: Need) -> Result<(), DiskError> {
        let root = {
            let st = self.lock();
            if !st.configured {
                return Ok(());
            }
            let Some(root) = st.roots.get(&cat).and_then(|r| r.first().cloned()) else { return Ok(()) };
            root
        };
        // A large write reads free space again; otherwise a reading under two seconds old will do.
        self.refresh_space(Some(&root), bytes >= 64 * MB);
        let mut st = self.lock();
        let floor = st.config.floor();
        let available = st.space_of(&root).ok().map(|s| s.available);
        if let Some(avail) = available {
            if avail.saturating_sub(bytes) < floor {
                match need {
                    Need::Agent => {
                        st.refused += 1;
                        st.prune_wanted = true;
                        let e = st.refusal(cat, &root, avail, bytes);
                        drop(st);
                        self.wake.notify_all();
                        return Err(e);
                    }
                    Need::Essential => st.essential_below_floor += 1,
                }
            }
        }
        if let Some(e) = st.space.get_mut(&root) {
            e.2 = e.2.saturating_add(bytes);
        }
        st.check_lines();
        let prune = st.prune_wanted;
        drop(st);
        if prune {
            self.wake.notify_all();
        }
        Ok(())
    }

    /// Records bytes written (or, negative, removed) for `cat` by `source`.
    pub fn wrote(&self, cat: Category, source: &'static str, bytes: i64, files: i64) {
        let mut st = self.lock();
        let u = st.usage.entry(cat).or_default();
        u.delta = u.delta.saturating_add(bytes);
        u.delta_files = u.delta_files.saturating_add(files);
        if bytes > 0 {
            let s = st.by_source.entry(source).or_default();
            s.0 = s.0.saturating_add(bytes as u64);
            s.1 = s.1.saturating_add(files.max(0) as u64);
        }
        if st.configured {
            st.check_lines();
        }
        let prune = st.prune_wanted;
        drop(st);
        if prune {
            self.wake.notify_all();
        }
    }

    /// Writes `data` to `path` for `cat` (its folders are made): admitted first, then accounted.
    pub fn write(&self, cat: Category, source: &'static str, path: &Path, data: &[u8], need: Need) -> Result<(), DiskError> {
        self.write_file(cat, source, path, data, need, false)
    }

    /// [`DiskBook::write`] with the data on disk before it returns: for a file that is then renamed
    /// over another, so a power cut never leaves the new name with nothing in it.
    pub fn write_synced(&self, cat: Category, source: &'static str, path: &Path, data: &[u8], need: Need) -> Result<(), DiskError> {
        self.write_file(cat, source, path, data, need, true)
    }

    fn write_file(&self, cat: Category, source: &'static str, path: &Path, data: &[u8], need: Need, sync: bool) -> Result<(), DiskError> {
        use std::io::Write as _;
        let old = std::fs::symlink_metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len());
        self.admit(cat, (data.len() as u64).saturating_sub(old.unwrap_or(0)), need)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| DiskError::io(parent, &e))?;
        }
        let mut f = std::fs::File::create(path).map_err(|e| DiskError::io(path, &e))?;
        f.write_all(data).and_then(|()| if sync { f.sync_all() } else { Ok(()) }).map_err(|e| DiskError::io(path, &e))?;
        self.wrote(cat, source, data.len() as i64 - old.unwrap_or(0) as i64, i64::from(old.is_none()));
        Ok(())
    }

    /// Appends `data` to `path` for `cat` (a log that grows by lines: the trace's JSONL sink):
    /// admitted first, then accounted.
    pub fn append(&self, cat: Category, source: &'static str, path: &Path, data: &[u8], need: Need) -> Result<(), DiskError> {
        use std::io::Write as _;
        let new = std::fs::symlink_metadata(path).map_or(true, |m| !m.is_file());
        self.admit(cat, data.len() as u64, need)?;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| DiskError::io(path, &e))?;
        f.write_all(data).map_err(|e| DiskError::io(path, &e))?;
        self.wrote(cat, source, data.len() as i64, i64::from(new));
        Ok(())
    }

    /// Writes `data` to `path` without ever waiting on the book: a panic hook's crash report
    /// (the panicking thread may hold the book's lock). Not admitted: its writer bounds these files
    /// itself (count × size, a reserved budget: docs/design/TRACE.md). Accounted when the book is
    /// free at that instant, else the walker counts it on its next pass.
    pub fn write_reserved(&self, cat: Category, source: &'static str, path: &Path, data: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, data)?;
        self.account_reserved(cat, source, data.len() as i64, 1);
        Ok(())
    }

    /// Accounts bytes written (or, negative, removed) without waiting on the book: when it is
    /// busy this instant, the walker's next pass counts them instead (a panic hook's crash
    /// reports and their pruning).
    pub fn account_reserved(&self, cat: Category, source: &'static str, bytes: i64, files: i64) {
        if let Ok(mut st) = self.state.try_lock() {
            let u = st.usage.entry(cat).or_default();
            u.delta = u.delta.saturating_add(bytes);
            u.delta_files = u.delta_files.saturating_add(files);
            if bytes > 0 {
                let s = st.by_source.entry(source).or_default();
                s.0 = s.0.saturating_add(bytes as u64);
                s.1 = s.1.saturating_add(files.max(0) as u64);
            }
        }
    }

    /// Copies `from` to `to` (a new file) for `cat`: admitted first, then accounted.
    pub fn copy(&self, cat: Category, source: &'static str, from: &Path, to: &Path, need: Need) -> Result<u64, DiskError> {
        let len = std::fs::metadata(from).map_err(|e| DiskError::io(from, &e))?.len();
        self.admit(cat, len, need)?;
        let n = std::fs::copy(from, to).map_err(|e| DiskError::io(to, &e))?;
        self.wrote(cat, source, n as i64, 1);
        Ok(n)
    }

    /// Records a file replaced in place (`old`: its size before, `None` when it is new).
    pub fn replaced(&self, cat: Category, source: &'static str, old: Option<u64>, new: u64) {
        self.wrote(cat, source, new as i64 - old.unwrap_or(0) as i64, i64::from(old.is_none()));
    }

    /// Makes `dir` (and its parents) for `cat`, admitted like a small write.
    pub fn make_dir(&self, cat: Category, dir: &Path, need: Need) -> Result<(), DiskError> {
        if dir.is_dir() {
            return Ok(());
        }
        self.admit(cat, 4096, need)?;
        std::fs::create_dir_all(dir).map_err(|e| DiskError::io(dir, &e))
    }

    /// The warning a session hasn't been told yet (each once; again when it gets worse).
    pub fn warning_for(&self, session: &str) -> Option<Json> {
        let mut st = self.lock();
        let (seq, text, _) = st.warning.clone()?;
        if st.told.get(session) == Some(&seq) {
            return None;
        }
        if st.told.len() >= MAX_TOLD && !st.told.contains_key(session) {
            // Forget the sessions told longest ago (lowest sequence numbers).
            if let Some(k) = st.told.iter().min_by_key(|(_, s)| **s).map(|(k, _)| k.clone()) {
                st.told.remove(&k);
            }
        }
        st.told.insert(session.to_owned(), seq);
        Some(text)
    }

    /// Adds the current disk warning to a tool's answer (its JSON object), once per session.
    pub fn annotate(&self, session: &str, mut out: specula_protocol::ToolOutput) -> specula_protocol::ToolOutput {
        // Nothing to tell this session: the answer goes out untouched (no parse, no copy).
        {
            let st = self.lock();
            match &st.warning {
                None => return out,
                Some((seq, _, _)) if st.told.get(session) == Some(seq) => return out,
                Some(_) => {}
            }
        }
        // The answer's first text item: sight tools answer the image first, then the packet.
        let Some((i, t)) = out.content.iter().enumerate().find_map(|(i, c)| match c {
            specula_protocol::Content::Text(t) => Some((i, t)),
            specula_protocol::Content::Image { .. } => None,
        }) else {
            return out;
        };
        let Ok(mut j) = Json::parse(t) else { return out };
        if j.as_object().is_none() {
            return out;
        }
        let Some(w) = self.warning_for(session) else { return out };
        j.set("disk_warning", w);
        out.content[i] = specula_protocol::Content::Text(j.to_string());
        out
    }

    /// `disk_status` (`detailed`: per writer, the prune log, every root), with free space read
    /// now where its reading is stale. For a caller's own thread: it asks the system.
    pub fn report(&self, detailed: bool) -> Json {
        self.refresh_space(None, false);
        self.report_as_read(detailed)
    }

    /// The report from what the book already holds, without asking the system: `engine_info.disk`,
    /// built on the engine thread. Free space is the background reader's last reading (it reads
    /// every second or two); asked here, on a volume that was slow to answer, it held the engine
    /// thread 10 ms inside `engine_info` (performance pass, 2026-11-14).
    pub fn report_as_read(&self, detailed: bool) -> Json {
        let mut st = self.lock();
        if !st.configured {
            return Json::obj([("configured", Json::Bool(false))]);
        }
        let now = Instant::now();
        let total = st.total_used();
        let cfg = st.config.clone();
        let mut cats = Vec::new();
        for c in Category::ALL {
            let (bytes, files) = st.used(c);
            let mut f = vec![("mb", mb(bytes)), ("files", Json::from(files))];
            if let Some(cap) = cfg.caps_mb.get(&c) {
                f.push(("cap_mb", Json::from(*cap)));
            }
            if let Some(u) = st.usage.get(&c) {
                if u.truncated {
                    f.push(("partial", Json::Bool(true)));
                }
                if detailed {
                    f.push(("measured_s_ago", u.walked.map_or(Json::Null, |w| mb_secs(now.duration_since(w)))));
                }
            }
            if detailed {
                if let Some(r) = st.roots.get(&c) {
                    f.push(("folders", Json::arr(r.iter().map(|p| Json::from(p.display().to_string())))));
                }
                f.push(("prunable", Json::Bool(c.prunable())));
            }
            cats.push((c.name(), Json::obj(f)));
        }
        let roots: Vec<(Category, PathBuf)> = st.roots.iter().filter_map(|(c, r)| r.first().map(|p| (*c, p.clone()))).collect();
        let mut volumes: BTreeMap<String, (Json, Vec<&'static str>)> = BTreeMap::new();
        let (mut unsupported, mut unread) = (None, false);
        for (c, r) in &roots {
            match st.space_of(r) {
                Err(e) if e.code == "not_read" => unread = true,
                Ok(s) => {
                    let key = s.volume.display().to_string();
                    let e = volumes.entry(key.clone()).or_insert_with(|| {
                        (Json::obj([("volume", Json::from(key)), ("available_mb", mb(s.available)), ("total_mb", mb(s.total)), ("floor_mb", Json::from(cfg.free_floor_mb))]), Vec::new())
                    });
                    e.1.push(c.name());
                }
                Err(e) => unsupported = Some(e.to_string()),
            }
        }
        let volumes: Vec<Json> = volumes
            .into_values()
            .map(|(mut v, cats)| {
                v.set("categories", Json::arr(cats.into_iter().map(Json::from)));
                v
            })
            .collect();
        let state = st.state_name();
        let mut out = vec![
            ("state", Json::from(state)),
            ("budget", Json::obj([("total_mb", Json::from(cfg.total_mb)), ("used_mb", mb(total)), ("warn_percent", Json::from(cfg.warn_percent))])),
            ("categories", Json::obj(cats)),
        ];
        if !volumes.is_empty() {
            out.push(("volumes", Json::Arr(volumes)));
        }
        if let Some(u) = unsupported {
            out.push(("free_space", Json::obj([("unsupported", Json::from(u))])));
        } else if unread {
            out.push(("free_space", Json::obj([("pending", Json::from("not read yet: the background reader reads it within a second or two (disk_status reads it now)"))])));
        }
        if let Some((_, w, _)) = &st.warning {
            out.push(("warning", w.clone()));
        }
        out.push(("pruned", Json::obj([("files", Json::from(st.pruned_total.0)), ("mb", mb(st.pruned_total.1))])));
        out.push(("refused", Json::from(st.refused)));
        if st.essential_below_floor > 0 {
            out.push(("essential_writes_below_floor", Json::from(st.essential_below_floor)));
        }
        if detailed {
            out.push(("written_this_run", Json::obj(st.by_source.iter().map(|(k, (b, n))| (*k, Json::obj([("mb", mb(*b)), ("files", Json::from(*n))]))))));
            out.push((
                "prune_log",
                Json::arr(st.pruned.iter().rev().map(|r| {
                    Json::obj([
                        ("at_unix", Json::from(r.at_unix)),
                        ("why", Json::from(r.why.as_str())),
                        ("files", Json::from(r.files)),
                        ("mb", mb(r.bytes)),
                        ("by_category", Json::obj(r.by_category.iter().map(|(c, n, b)| (c.name(), Json::obj([("files", Json::from(*n)), ("mb", mb(*b))]))))),
                    ])
                })),
            ));
        }
        Json::obj(out)
    }

    /// Prunes now, in order (oldest captures, content nothing needs, expired scratch), down to the
    /// budget's prune line, and at least `at_least` bytes (a lead's `disk_prune {free_mb}`);
    /// `dry_run` reports what would go. Returns the report, or `busy` while another prune runs.
    ///
    /// What may be freed is worked out first from what is actually prunable now: captures past
    /// `capture_min_age_s` in this run's and finished runs' folders, content a pruner can give
    /// back, scratch past its grace. Each need is capped at that, so a prune never deletes for
    /// nothing; one that can't get under the budget is remembered, and the next waits until use
    /// grows past it.
    pub fn prune(&self, dry_run: bool, why: &str, at_least: u64) -> Json {
        // One prune at a time (the walker's, an agent's): each walks the captures. A second one
        // never waits: it answers busy (the walker just tries again later).
        let _one = match self.pruning.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Json::obj([("error", Json::from("busy")), ("message", Json::from("another prune is running; try again in a moment"))]),
        };
        self.refresh_space(None, false);
        // What is prunable now, per category.
        let caps = self.capture_candidates();
        let cap_bytes: u64 = caps.iter().map(|c| c.3).sum();
        let pruners: Vec<Pruner> = self.lock().pruners.iter().filter(|(c, _)| *c == Category::Content).map(|(_, p)| Arc::clone(p)).collect();
        let content_bytes: u64 = pruners.iter().map(|p| p(u64::MAX, true).1).sum();
        let scratch = self.scratch_candidates();
        let scratch_bytes: u64 = scratch.iter().map(|(_, (_, b))| *b).sum();
        let prunable = cap_bytes + content_bytes + scratch_bytes;
        let mut plan = {
            let mut st = self.lock();
            if !dry_run {
                st.prune_wanted = false;
            }
            st.prune_plan(prunable, &[(Category::Captures, cap_bytes), (Category::Content, content_bytes), (Category::Scratch, scratch_bytes)].into())
        };
        plan.need = plan.need.max(at_least.min(prunable));
        let mut by_category = Vec::new();
        let (mut files, mut bytes) = (0u64, 0u64);
        let mut items = Vec::new();
        let mut need = plan.need;
        for cat in [Category::Captures, Category::Content, Category::Scratch] {
            let want = need.max(plan.per_cat.get(&cat).copied().unwrap_or(0));
            // Expired scratch always goes (it is past its grace); the rest only when needed.
            if want == 0 && cat != Category::Scratch {
                continue;
            }
            let (n, b) = match cat {
                Category::Captures => self.prune_captures(&caps, want, dry_run, &mut items),
                Category::Scratch => self.prune_scratch(&scratch, dry_run, &mut items),
                _ => pruners.iter().fold((0, 0), |(n, b), p| {
                    let (pn, pb) = p(want.saturating_sub(b), dry_run);
                    (n + pn, b + pb)
                }),
            };
            if n > 0 {
                by_category.push((cat, n, b));
                if !dry_run {
                    self.wrote(cat, "prune", -(b as i64), -(n as i64));
                }
            }
            files += n;
            bytes += b;
            need = need.saturating_sub(b);
        }
        let mut st = self.lock();
        if !dry_run {
            if files > 0 {
                st.pruned_total.0 += files;
                st.pruned_total.1 += bytes;
                if st.pruned.len() == PRUNE_LOG {
                    st.pruned.pop_front();
                }
                st.pruned.push_back(PruneRecord { at_unix: unix_now(), why: why.to_owned(), files, bytes, by_category: by_category.clone() });
                // Free space moved: read it again.
                st.space.clear();
            }
            // Still over the total: what is left can't be pruned. Not again until use grows.
            let total = st.total_used();
            st.unhelped_at = (total > st.config.total_mb * MB).then_some(total);
            // Wanted again only if the lines (with what is left) say so.
            st.prune_wanted = false;
            st.check_lines();
        }
        Json::obj([
            ("dry_run", Json::Bool(dry_run)),
            ("needed_mb", mb(plan.need)),
            ("prunable_mb", mb(prunable)),
            ("files", Json::from(files)),
            ("mb", mb(bytes)),
            ("by_category", Json::obj(by_category.iter().map(|(c, n, b)| (c.name(), Json::obj([("files", Json::from(*n)), ("mb", mb(*b))]))))),
            ("items", Json::arr(items.into_iter().take(20))),
            ("state", Json::from(st.state_name())),
        ])
    }

    /// [`DiskBook::prune`] for an agent's `disk_prune`: a real prune at most once per 10 s (a loop
    /// of calls can't keep the disk busy deleting); the wait starts only once one has run. Dry runs
    /// aren't limited (one prune or dry run runs at a time).
    pub fn prune_for_agent(&self, dry_run: bool, why: &str, at_least: u64) -> Result<Json, DiskError> {
        if !dry_run {
            let st = self.lock();
            if let Some(t) = st.last_agent_prune.filter(|t| t.elapsed() < AGENT_PRUNE_EVERY) {
                let wait = AGENT_PRUNE_EVERY.saturating_sub(t.elapsed()).as_secs() + 1;
                return Err(DiskError { code: "too_soon", message: format!("disk_prune ran less than 10 s ago; try again in {wait} s (the engine also prunes on its own when over budget)"), detail: vec![("retry_after_s", Json::from(wait))] });
            }
        }
        let r = self.prune(dry_run, why, at_least);
        if !dry_run && r.get("error").is_none() {
            self.lock().last_agent_prune = Some(Instant::now());
        }
        Ok(r)
    }

    /// Captures that may go, oldest first: `(modified, path, root, bytes)`. Only in this run's
    /// folder and finished runs' (another running engine's are its own), never younger than
    /// `capture_min_age_s`, never through a link.
    fn capture_candidates(&self) -> Vec<(SystemTime, PathBuf, PathBuf, u64)> {
        let (roots, skip, current, min_age) = {
            let st = self.lock();
            (st.roots.get(&Category::Captures).cloned().unwrap_or_default(), st.other_roots(Category::Captures), st.current_run.clone(), Duration::from_secs(st.config.capture_min_age_s))
        };
        let young = SystemTime::now().checked_sub(min_age).unwrap_or(UNIX_EPOCH);
        let mut cands: Vec<(SystemTime, PathBuf, PathBuf, u64)> = Vec::new();
        let mut budget = WALK_ENTRIES;
        for root in &roots {
            let Ok(entries) = std::fs::read_dir(root) else { continue };
            for e in entries.flatten() {
                let path = root.join(e.file_name());
                let Ok(m) = std::fs::symlink_metadata(&path) else { continue };
                if !m.is_dir() || is_link(&m) || skip.contains(&path) {
                    continue;
                }
                let mine = current.as_ref() == Some(&path);
                let finished = e.file_name().to_str().and_then(parse_run_name).is_some_and(|(_, pid)| !specula_sys::process_is_running(pid));
                if !mine && !finished {
                    continue;
                }
                collect_files(&path, &skip, &mut budget, &mut |p, m| {
                    let modified = m.modified().unwrap_or(UNIX_EPOCH);
                    if min_age.is_zero() || modified < young {
                        cands.push((modified, p.to_path_buf(), root.clone(), m.len()));
                    }
                });
            }
        }
        cands.sort();
        cands
    }

    /// Expired scratch folders (not live in this engine) with their `(files, bytes)`.
    fn scratch_candidates(&self) -> Vec<(PathBuf, (u64, u64))> {
        let (roots, grace, live) = {
            let st = self.lock();
            (st.roots.get(&Category::Scratch).cloned().unwrap_or_default(), st.config.scratch_grace_s, st.live_scratch.clone())
        };
        let now = unix_now();
        roots.iter().flat_map(|r| expired_scratch(r, now, grace)).filter(|d| !live.contains(d)).map(|d| {
            let size = tree_size(&d);
            (d, size)
        }).collect()
    }

    fn prune_captures(&self, cands: &[(SystemTime, PathBuf, PathBuf, u64)], want: u64, dry_run: bool, items: &mut Vec<Json>) -> (u64, u64) {
        let current = self.lock().current_run.clone();
        let (mut n, mut b) = (0u64, 0u64);
        let mut emptied = BTreeSet::new();
        for (_, path, root, len) in cands.iter().cloned() {
            if b >= want {
                break;
            }
            if !dry_run {
                if remove_file_inside(&root, &path).is_err() {
                    continue;
                }
                if let Some(parent) = path.parent() {
                    emptied.insert((root.clone(), parent.to_path_buf()));
                }
            }
            if items.len() < 20 {
                items.push(Json::obj([("path", Json::from(path.display().to_string())), ("kb", Json::from(len / 1024))]));
            }
            n += 1;
            b += len;
        }
        // Folders left empty go too (never a root, never this run's folder).
        for (root, dir) in emptied.into_iter().rev() {
            let mut at = dir;
            while at != root && Some(&at) != current.as_ref() && at.starts_with(&root) {
                if remove_empty_dir_inside(&root, &at).is_err() {
                    break;
                }
                let Some(p) = at.parent() else { break };
                at = p.to_path_buf();
            }
        }
        (n, b)
    }

    fn prune_scratch(&self, cands: &[(PathBuf, (u64, u64))], dry_run: bool, items: &mut Vec<Json>) -> (u64, u64) {
        let roots = self.lock().roots.get(&Category::Scratch).cloned().unwrap_or_default();
        let (mut n, mut b) = (0u64, 0u64);
        for (dir, size) in cands {
            let Some(root) = roots.iter().find(|r| dir.starts_with(r)) else { continue };
            let size = if dry_run { *size } else { remove_tree_inside(root, dir).unwrap_or((0, 0)) };
            if size.0 > 0 || !dir.exists() {
                if items.len() < 20 {
                    items.push(Json::obj([("path", Json::from(dir.display().to_string())), ("kb", Json::from(size.1 / 1024)), ("why", Json::from("expired scratch lease"))]));
                }
                n += size.0.max(1);
                b += size.1;
            }
        }
        (n, b)
    }

    /// One slice of background work: a slice of the current walk, expired scratch, pruning when
    /// wanted. Returns whether more is waiting.
    pub fn step(&self) -> bool {
        self.refresh_space(None, false);
        let (prune, scratch) = {
            let mut st = self.lock();
            if !st.configured {
                return false;
            }
            let scratch = st.last_scratch.is_none_or(|t| t.elapsed() >= SCRATCH_EVERY);
            if scratch {
                st.last_scratch = Some(Instant::now());
            }
            // Pruning that couldn't get under the line (everything left is protected) waits a
            // while before looking again.
            let prune = st.prune_wanted && st.last_prune.is_none_or(|t| t.elapsed() >= PRUNE_COOLDOWN);
            if prune {
                st.last_prune = Some(Instant::now());
            }
            (prune, scratch)
        };
        let walking = self.walk_step(None);
        if prune {
            self.prune(false, "over budget", 0);
        } else if scratch {
            let mut items = Vec::new();
            let cands = self.scratch_candidates();
            let (n, b) = self.prune_scratch(&cands, false, &mut items);
            if n > 0 {
                self.wrote(Category::Scratch, "prune", -(b as i64), -(n as i64));
                let mut st = self.lock();
                st.pruned_total.0 += n;
                st.pruned_total.1 += b;
                st.pruned.push_back(PruneRecord { at_unix: unix_now(), why: "expired scratch leases".into(), files: n, bytes: b, by_category: vec![(Category::Scratch, n, b)] });
                if st.pruned.len() > PRUNE_LOG {
                    st.pruned.pop_front();
                }
            }
        }
        walking
    }

    fn walk_forever(&self) {
        loop {
            let busy = self.step();
            let st = self.lock();
            let pause = if busy { SLICE_PAUSE } else { Duration::from_secs(1) };
            let _ = self.wake.wait_timeout(st, pause);
        }
    }

    /// Bytes and files of `cat` now (for tests and tools).
    pub fn used(&self, cat: Category) -> (u64, u64) {
        self.lock().used(cat)
    }

    /// [`DiskBook::measure_now`] for an agent: at most once per [`MEASURE_EVERY`]. Returns
    /// whether it measured.
    pub fn measure_for_agent(&self) -> bool {
        {
            let mut st = self.lock();
            if st.last_measure.is_some_and(|t| t.elapsed() < MEASURE_EVERY) {
                return false;
            }
            st.last_measure = Some(Instant::now());
        }
        self.measure_now();
        true
    }

    /// Walks every category to the end now (tests and `disk_status {measure: true}`; bounded by
    /// the walk's entry budget).
    pub fn measure_now(&self) {
        let cats: Vec<Category> = self.lock().roots.keys().copied().filter(|c| *c != Category::Project).collect();
        for cat in cats {
            self.lock().start_walk(cat);
            while self.walk_step(Some(cat)) {}
        }
    }

    /// One slice of the current walk (starting the next one due when none runs), with the disk
    /// read outside the lock so writers never wait on it. `only`: walk just that category.
    /// Returns whether that walk goes on.
    fn walk_step(&self, only: Option<Category>) -> bool {
        let (mut walk, skip) = {
            let mut st = self.lock();
            if st.walk.is_none() {
                if only.is_some() {
                    return false;
                }
                match st.next_walk_cat(false) {
                    Some(c) => st.start_walk(c),
                    None => return false,
                }
            }
            let Some(walk) = st.walk.take() else { return false };
            if only.is_some_and(|c| c != walk.cat) {
                st.walk = Some(walk);
                return false;
            }
            let skip = st.other_roots(walk.cat);
            (walk, skip)
        };
        walk_slice(&mut walk, &skip);
        let mut st = self.lock();
        if walk.stack.is_empty() {
            let u = st.usage.entry(walk.cat).or_default();
            u.measured = walk.bytes;
            u.files = walk.files;
            u.truncated = walk.truncated;
            u.walked = Some(Instant::now());
            st.check_lines();
            false
        } else {
            st.walk = Some(walk);
            true
        }
    }
}

/// Looks at up to [`SLICE`] more entries of `walk` (whole folders at a time), never through a
/// link and never into `skip`.
fn walk_slice(walk: &mut Walk, skip: &BTreeSet<PathBuf>) {
    let mut slice = SLICE;
    while slice > 0 {
        let Some((dir, depth)) = walk.stack.pop() else { break };
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            slice = slice.saturating_sub(1);
            walk.entries += 1;
            if walk.entries > WALK_ENTRIES {
                walk.truncated = true;
                walk.stack.clear();
                return;
            }
            let p = e.path();
            let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
            if is_link(&m) {
                continue;
            }
            if m.is_dir() {
                if depth + 1 >= WALK_DEPTH {
                    walk.truncated = true;
                } else if !skip.contains(&p) {
                    walk.stack.push((p, depth + 1));
                }
            } else {
                walk.bytes += m.len();
                walk.files += 1;
            }
        }
    }
}

fn mb_secs(d: Duration) -> Json {
    Json::from((d.as_secs_f64() * 10.0).round() / 10.0)
}

struct Plan {
    /// Bytes to free in all (the total budget, or free space under the warning line).
    need: u64,
    /// Bytes to free per category over its cap.
    per_cat: BTreeMap<Category, u64>,
}

impl State {
    fn used(&self, cat: Category) -> (u64, u64) {
        if cat == Category::Project {
            return self.project_bytes;
        }
        self.usage.get(&cat).map_or((0, 0), |u| (u.bytes(), u.file_count()))
    }

    fn total_used(&self) -> u64 {
        Category::ALL.into_iter().map(|c| self.used(c).0).sum()
    }

    /// Other categories' roots (a walk of one never counts another's folder inside it).
    fn other_roots(&self, cat: Category) -> BTreeSet<PathBuf> {
        self.roots.iter().filter(|(c, _)| **c != cat).flat_map(|(_, r)| r.iter().cloned()).collect()
    }

    /// Free space for `root` as last read ([`DiskBook::refresh_space`] reads it, outside the
    /// lock), lowered by what was admitted since.
    fn space_of(&self, root: &Path) -> Result<specula_sys::disk::Space, specula_sys::disk::DiskError> {
        let Some((s, _, admitted)) = self.space.get(root) else {
            return Err(specula_sys::disk::DiskError { code: "not_read", message: "free space not read yet".into(), os_code: None });
        };
        s.clone().map(|mut s| {
            s.available = s.available.saturating_sub(*admitted);
            s
        })
    }

    fn min_available(&self) -> Option<u64> {
        let roots: Vec<PathBuf> = self.roots.values().filter_map(|r| r.first().cloned()).collect();
        roots.iter().filter_map(|r| self.space_of(r).ok().map(|s| s.available)).min()
    }

    fn state_name(&mut self) -> &'static str {
        let floor = self.config.floor();
        if self.min_available().is_some_and(|a| a < floor) {
            return "below_floor";
        }
        if self.total_used() > self.config.total_mb * MB || self.config.caps_mb.iter().any(|(c, cap)| self.used(*c).0 > cap * MB) {
            return "over_budget";
        }
        if self.warning.is_some() {
            return "warn";
        }
        "ok"
    }

    /// Works out the soft lines crossed now: a warning (bumped when a new line is crossed) and
    /// whether pruning is due.
    fn check_lines(&mut self) {
        let cfg = self.config.clone();
        let pct = cfg.warn_percent;
        let mut crossed: Vec<(String, String)> = Vec::new();
        let total = self.total_used();
        if total * 100 > cfg.total_mb * MB * pct {
            crossed.push(("total".into(), format!("the engine's output is {:.0} MB of its {} MB budget", total as f64 / MB as f64, cfg.total_mb)));
        }
        // What a prune left only gets lower: use that fell (a world deleted by hand) is the new mark.
        self.unhelped_at = self.unhelped_at.map(|a| a.min(total));
        if total > cfg.total_mb * MB && self.unhelped_at.is_none_or(|at| total > at.saturating_add(MB)) {
            self.prune_wanted = true;
        }
        if total <= cfg.total_mb * MB {
            self.unhelped_at = None;
        }
        for (c, cap) in &cfg.caps_mb {
            let used = self.used(*c).0;
            if used * 100 > cap * MB * pct {
                crossed.push((c.name().into(), format!("{} is {:.0} MB of its {cap} MB cap", c.name(), used as f64 / MB as f64)));
            }
            if used > cap * MB && c.prunable() {
                self.prune_wanted = true;
            }
        }
        let floor = cfg.floor();
        if let Some(a) = self.min_available() {
            if a < floor.saturating_mul(2) {
                crossed.push(("free".into(), format!("{:.0} MB is free on the engine's disk; writes are refused below {} MB", a as f64 / MB as f64, cfg.free_floor_mb)));
                self.prune_wanted = true;
            }
        }
        if crossed.is_empty() {
            self.warning = None;
            return;
        }
        let keys: BTreeSet<String> = crossed.iter().map(|(k, _)| k.clone()).collect();
        let worse = self.warning.as_ref().is_none_or(|(_, _, old)| !keys.is_subset(old));
        if worse {
            self.warn_seq += 1;
        }
        let biggest = self.biggest();
        let text = Json::obj([
            ("code", Json::from("disk_budget")),
            ("message", Json::from(crossed.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>().join("; "))),
            ("crossed", Json::arr(keys.iter().map(|k| Json::from(k.as_str())))),
            ("biggest", biggest),
            ("hint", Json::from("the engine prunes old captures, unreferenced cache entries and expired scratch when over budget; disk_status shows the details and disk_prune {dry_run: true} what would go")),
        ]);
        self.warning = Some((self.warn_seq, text, keys));
    }

    /// The largest categories, with what an agent can do about each.
    fn biggest(&self) -> Json {
        let mut v: Vec<(u64, Category)> = Category::ALL.into_iter().map(|c| (self.used(c).0, c)).filter(|(b, _)| *b > 0).collect();
        v.sort_by(|a, b| b.cmp(a));
        Json::arr(v.into_iter().take(4).map(|(b, c)| {
            let mut f = vec![("category", Json::from(c.name())), ("mb", mb(b)), ("what_to_do", Json::from(c.advice()))];
            if let Some(r) = self.roots.get(&c).and_then(|r| r.first()) {
                f.push(("folder", Json::from(r.display().to_string())));
            }
            Json::obj(f)
        }))
    }

    fn refusal(&self, cat: Category, root: &Path, avail: u64, bytes: u64) -> DiskError {
        let cfg = &self.config;
        let volume = self.space.get(root).and_then(|(s, _, _)| s.as_ref().ok()).map(|s| s.volume.display().to_string()).unwrap_or_default();
        DiskError {
            code: "disk_floor",
            message: format!(
                "refused: writing {:.1} MB of {} would leave {:.0} MB free on {volume}, under the {} MB floor ([disk] free_floor_mb in perf.toml); nothing was written. Free space first (see what_to_delete), then retry",
                bytes as f64 / MB as f64,
                cat.name(),
                avail.saturating_sub(bytes) as f64 / MB as f64,
                cfg.free_floor_mb
            ),
            detail: vec![
                ("category", Json::from(cat.name())),
                ("volume", Json::from(volume)),
                ("needed_bytes", Json::from(bytes)),
                ("available_mb", mb(avail)),
                ("floor_mb", Json::from(cfg.free_floor_mb)),
                ("engine_use_mb", mb(self.total_used())),
                ("what_to_delete", self.biggest()),
                ("prune", Json::from("disk_prune frees what the engine may delete (old captures, unreferenced cache, expired scratch)")),
            ],
        }
    }

    /// How much to free: over the total, down to its prune line; under twice the free-space
    /// floor, back above it; over a cap, down to its line. Each capped at what is prunable
    /// (`prunable`, and `per_cat` per category).
    fn prune_plan(&self, prunable: u64, per_cat_prunable: &BTreeMap<Category, u64>) -> Plan {
        let cfg = &self.config;
        let mut need = 0u64;
        let total = self.total_used();
        if total > cfg.total_mb * MB {
            need = (total - cfg.total_mb * MB * PRUNE_TO_PERCENT / 100).min(prunable);
        }
        if let Some(a) = self.min_available() {
            let free_line = cfg.floor().saturating_mul(2);
            if a < free_line {
                need = need.max((free_line - a).min(prunable));
            }
        }
        let mut per_cat = BTreeMap::new();
        for (c, cap) in &cfg.caps_mb {
            let used = self.used(*c).0;
            if used > cap * MB {
                let can = per_cat_prunable.get(c).copied().unwrap_or(0);
                per_cat.insert(*c, (used - cap * MB * PRUNE_TO_PERCENT / 100).min(can));
            }
        }
        Plan { need, per_cat }
    }

    fn next_walk_cat(&self, any: bool) -> Option<Category> {
        // The category walked longest ago (never walked first).
        Category::ALL
            .into_iter()
            .filter(|c| *c != Category::Project && self.roots.contains_key(c))
            .filter(|c| any || self.usage.get(c).and_then(|u| u.walked).is_none_or(|w| w.elapsed() >= REWALK))
            .min_by_key(|c| self.usage.get(c).and_then(|u| u.walked))
    }

    fn start_walk(&mut self, cat: Category) {
        let roots = self.roots.get(&cat).cloned().unwrap_or_default();
        let u = self.usage.entry(cat).or_default();
        // Writes from here on are counted again on top of what the walk finds.
        u.delta = 0;
        u.delta_files = 0;
        self.walk = Some(Walk { cat, stack: roots.into_iter().map(|r| (r, 0)).collect(), bytes: 0, files: 0, entries: 0, truncated: false });
    }
}

fn logic_category(kind: specula_logic::disk::Kind) -> Category {
    match kind {
        specula_logic::disk::Kind::Modules => Category::Modules,
        specula_logic::disk::Kind::Project => Category::Project,
    }
}

/// The module host's writes (specula-logic can't see the book): installed as its hook at start.
pub fn logic_admit(kind: specula_logic::disk::Kind, bytes: u64, essential: bool) -> Result<(), String> {
    book().admit(logic_category(kind), bytes, if essential { Need::Essential } else { Need::Agent }).map_err(|e| e.to_string())
}

pub fn logic_wrote(kind: specula_logic::disk::Kind, bytes: i64, files: i64) {
    book().wrote(logic_category(kind), "logic", bytes, files);
}

/// A regular file's size (`None`: no such file).
pub fn file_len(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// `run-<secs>-<pid>` (a capture run's folder name).
pub fn parse_run_name(name: &str) -> Option<(u64, u32)> {
    let rest = name.strip_prefix("run-")?;
    let (secs, pid) = rest.split_once('-')?;
    if secs.is_empty() || pid.is_empty() || !secs.bytes().all(|b| b.is_ascii_digit()) || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((secs.parse().ok()?, pid.parse().ok()?))
}

/// Whether `m` is a link of any kind: a symbolic link, a junction or another reparse point.
pub fn is_link(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    m.file_type().is_symlink()
}

/// Visits the regular files under `dir` (never through a link, never into `skip`), within `budget` entries.
fn collect_files(dir: &Path, skip: &BTreeSet<PathBuf>, budget: &mut usize, f: &mut dyn FnMut(&Path, &std::fs::Metadata)) {
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            let p = e.path();
            let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
            if is_link(&m) {
                continue;
            }
            if m.is_dir() {
                if depth + 1 < WALK_DEPTH && !skip.contains(&p) {
                    stack.push((p, depth + 1));
                }
            } else if m.is_file() {
                f(&p, &m);
            }
        }
    }
}

/// Refuses `path` unless it lies strictly inside `root` (canonical) and every existing step from
/// `root` down to it is a real folder, the last a real file or folder: no symlink, junction or
/// other reparse point anywhere (one could lead out of the engine's folders).
pub fn check_inside(root: &Path, path: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| format!("the root {} can't be resolved: {e}", root.display()))?;
    let rel = path.strip_prefix(&root).map_err(|_| format!("{} is outside {}", path.display(), root.display()))?;
    if rel.as_os_str().is_empty() {
        return Err(format!("{} is the root itself", path.display()));
    }
    let mut at = root.clone();
    for c in rel.components() {
        let Component::Normal(part) = c else { return Err(format!("{} has a '..' or prefix step", path.display())) };
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if is_link(&m) => return Err(format!("{} is a link; the engine never deletes through links", at.display())),
            Ok(_) => {}
            Err(e) => return Err(format!("{}: {e}", at.display())),
        }
    }
    Ok(())
}

/// Removes one regular file inside `root` (see [`check_inside`]). Returns its size.
pub fn remove_file_inside(root: &Path, path: &Path) -> Result<u64, String> {
    check_inside(root, path)?;
    let m = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(m.len())
}

/// Marks one regular file inside `root` as just used (its modified time, now; no bytes written),
/// so age-based collection (review evidence a new call named again) leaves it alone a while.
pub fn touch_inside(root: &Path, path: &Path) -> Result<(), String> {
    check_inside(root, path)?;
    let m = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let f = std::fs::OpenOptions::new().write(true).open(path).map_err(|e| e.to_string())?;
    f.set_modified(SystemTime::now()).map_err(|e| e.to_string())
}

/// Removes an empty folder inside `root` (never the root).
pub fn remove_empty_dir_inside(root: &Path, dir: &Path) -> Result<(), String> {
    check_inside(root, dir)?;
    std::fs::remove_dir(dir).map_err(|e| e.to_string())
}

/// Removes a folder inside `root` with everything in it. Links met inside are removed as links
/// (their targets are never touched or followed). Returns `(files, bytes)` removed (links not
/// counted).
///
/// After [`check_inside`], the removal itself is `std::fs::remove_dir_all`, which on Windows works
/// from open folder handles and never follows a reparse point: a folder swapped for a junction
/// while it is being emptied can't lead the delete outside (std's fix for CVE-2022-21658).
pub fn remove_tree_inside(root: &Path, dir: &Path) -> Result<(u64, u64), String> {
    check_inside(root, dir)?;
    let m = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !m.is_dir() || is_link(&m) {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let size = tree_size(dir);
    std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(size)
}

/// What a folder deleted in one rename is called while it waits to be emptied: renamed in place
/// (in the same folder, the cheapest rename there is) to `.<name>.trash.<pid>.<n>`, a name no lease
/// folder has (the walker's lease names end in digits after a `-`). A shared trash folder made
/// renames wait behind the worker emptying it (about 140 ms, review of the bounds lane).
pub const TRASH_MARK: &str = ".trash.";

static TRASHED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Moves `dir` (a folder inside `root`, never through a link) aside in one rename: it is gone
/// from its path at once, its bytes still counted where they were (same root, same category)
/// until [`remove_tree_inside`] empties it, on a worker. Returns where it went.
pub fn move_to_trash(root: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(root, dir)?;
    let m = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !m.is_dir() || is_link(&m) {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let parent = dir.parent().ok_or("no parent folder")?;
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let n = TRASHED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let to = parent.join(format!(".{name}{TRASH_MARK}{}.{n}", std::process::id()));
    std::fs::rename(dir, &to).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(to)
}

/// The folders under `root` (directly) waiting to be emptied: this run's, and what a run that
/// ended before its worker emptied them left.
pub fn trash_left(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with('.') && n.contains(TRASH_MARK)))
        .map(|e| e.path())
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !is_link(&m)))
        .take(1024)
        .collect();
    out.sort();
    out
}

/// `(files, bytes)` under `dir`, never through links (bounded).
fn tree_size(dir: &Path) -> (u64, u64) {
    let mut out = (0, 0);
    let mut budget = WALK_ENTRIES;
    collect_files(dir, &BTreeSet::new(), &mut budget, &mut |_, m| {
        out.0 += 1;
        out.1 += m.len();
    });
    out
}

/// A scratch lease's marker: which lease, until when (unix seconds), and whether its owner asked
/// to keep the folder.
pub fn write_lease_marker(dir: &Path, lease: u64, owner: &str, ends_unix: u64, keep: bool) -> std::io::Result<()> {
    let j = Json::obj([("lease", Json::from(lease)), ("owner", Json::from(owner)), ("ends_unix", Json::from(ends_unix)), ("keep", Json::Bool(keep)), ("pid", Json::from(std::process::id()))]);
    std::fs::write(dir.join(LEASE_MARKER), j.to_string())
}

/// A lease folder's name, `<owner>-<lease>-<secs>-<pid>`, split: `(owner, lease, pid)`.
fn parse_scratch_name(name: &str) -> Option<(&str, u64, u32)> {
    let mut parts = name.rsplitn(4, '-');
    let pid = parts.next()?.parse::<u32>().ok()?;
    let secs = parts.next()?;
    let lease = parts.next()?.parse::<u64>().ok()?;
    let owner = parts.next()?;
    (!owner.is_empty() && !secs.is_empty() && secs.bytes().all(|b| b.is_ascii_digit())).then_some((owner, lease, pid))
}

/// Scratch folders directly under `root` whose lease ended more than `grace_s` ago (and whose
/// owner didn't ask to keep them). Only folders named as a lease names them
/// (`<owner>-<lease>-<secs>-<pid>`) whose marker says the same lease, owner and process: a marker
/// dropped into any other folder never gets it removed. Folders without a marker are never touched.
fn expired_scratch(root: &Path, now_unix: u64, grace_s: u64) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten().take(10_000) {
        let p = root.join(e.file_name());
        let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
        if !m.is_dir() || is_link(&m) {
            continue;
        }
        let Ok(text) = crate::safe_path::read_bounded(&p.join(LEASE_MARKER), 4096, "marker") else { continue };
        let Ok(j) = Json::parse(&text) else { continue };
        let Some((owner, lease, pid)) = e.file_name().to_str().and_then(parse_scratch_name).map(|(o, l, p)| (o.to_owned(), l, p)) else { continue };
        let marker_owner = j.get("owner").and_then(Json::as_str).map(crate::leases::folder_safe);
        if j.get("lease").and_then(Json::as_u64) != Some(lease) || j.get("pid").and_then(Json::as_u64) != Some(u64::from(pid)) || marker_owner.as_deref() != Some(owner.as_str()) {
            continue;
        }
        let ends = j.get("ends_unix").and_then(Json::as_u64).unwrap_or(u64::MAX);
        let keep = j.get("keep").and_then(Json::as_bool).unwrap_or(false);
        if !keep && ends.saturating_add(grace_s) < now_unix {
            out.push(p);
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("specula-disk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        canonical(&d)
    }

    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd").args(["/c", "mklink", "/J"]).arg(link).arg(target).output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn config_parses_and_names_what_it_knows() {
        let t = specula_core::toml::parse("total_mb = 100\nfree_floor_mb = 0\ncaptures_mb = 10\nwarn_percent = 80\nscratch_grace_s = 5").unwrap();
        let c = DiskConfig::parse(&t).unwrap();
        assert_eq!((c.total_mb, c.free_floor_mb, c.warn_percent, c.scratch_grace_s, c.caps_mb[&Category::Captures]), (100, 0, 80, 5, 10));
        let e = DiskConfig::parse(&specula_core::toml::parse("captures = 1").unwrap()).unwrap_err();
        assert!(e.contains("captures_mb"), "{e}");
        assert!(DiskConfig::parse(&specula_core::toml::parse("warn_percent = 120").unwrap()).is_err());
        assert!(crate::perf::PerfConfig::parse("[disk]\ntotal_mb = 5\n").is_ok_and(|p| p.disk.total_mb == 5));
    }

    /// Integration check (2026-10-29): playtest replays and Sepulcra's recordings were booked as
    /// captures but written outside every root (never measured, never capped). They are their
    /// own category now: both folders measured, and never pruned, even far over budget.
    #[test]
    fn recordings_are_measured_and_never_pruned() {
        let base = scratch("recordings");
        let book = DiskBook::default();
        let (caps, plays, recs) = (base.join("captures"), base.join("playtests"), base.join(RECORDINGS_DIR));
        let mut cfg = DiskConfig { free_floor_mb: 0, total_mb: 1, capture_min_age_s: 0, ..DiskConfig::default() };
        cfg.caps_mb.insert(Category::Recordings, 1);
        book.configure_local(cfg, vec![(Category::Captures, caps.clone()), (Category::Recordings, plays.clone()), (Category::Recordings, recs.clone())], None);
        assert!(!Category::Recordings.prunable() && Category::parse("recordings") == Some(Category::Recordings));
        book.write(Category::Recordings, "playtest", &plays.join("r.json"), &vec![1u8; 700 << 10], Need::Agent).unwrap();
        book.write(Category::Recordings, "play_record", &recs.join("a.json"), &vec![2u8; 700 << 10], Need::Agent).unwrap();
        book.write(Category::Captures, "sight", &caps.join("run-1-1").join("a.png"), &[0u8; 1000], Need::Agent).unwrap();
        book.measure_now();
        assert_eq!(book.used(Category::Recordings), (1400 << 10, 2), "both folders are measured");
        let pruned = book.prune(false, "test", 64 << 20);
        assert!(plays.join("r.json").is_file() && recs.join("a.json").is_file(), "never pruned: {pruned}");
        let r = book.report(true);
        assert!(r.to_string().contains("recordings"), "{r}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `engine_info`'s report never asks the system for free space (the engine thread builds
    /// it): a book nobody has read space for says so, and holds no reading afterwards either.
    #[test]
    fn the_engine_threads_report_never_reads_the_disk() {
        let base = scratch("as-read");
        let book = DiskBook::default();
        book.configure_local(DiskConfig::default(), vec![(Category::Captures, base.join("captures"))], None);
        let r = book.report_as_read(false);
        assert!(r.get("volumes").is_none() && r.get("free_space").and_then(|f| f.get("pending")).is_some(), "{r}");
        assert!(book.lock().space.is_empty(), "and it read nothing");
        // The caller-thread report reads it, where the platform can.
        let r = book.report(false);
        assert!(r.get("free_space").and_then(|f| f.get("pending")).is_none(), "{r}");
        assert_eq!(book.lock().space.len(), 1);
        // From then on the engine thread's report shows that reading.
        let again = book.report_as_read(false);
        assert_eq!(again.get("volumes"), r.get("volumes"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The accounting point: writes move the numbers at once; the walker's measurement replaces
    /// them; other categories' folders inside a root are not counted twice.
    #[test]
    fn writes_are_accounted_and_walks_measure() {
        let base = scratch("acct");
        let book = DiskBook::default();
        let caps = base.join("captures");
        let scratch_root = caps.join("scratch");
        book.configure_local(DiskConfig { free_floor_mb: 0, ..DiskConfig::default() }, vec![(Category::Captures, caps.clone()), (Category::Scratch, scratch_root.clone())], None);
        book.write(Category::Captures, "sight", &caps.join("run-1-1").join("a.png"), &[0u8; 1000], Need::Agent).unwrap();
        book.write(Category::Scratch, "lease", &scratch_root.join("x").join("f"), &[0u8; 300], Need::Agent).unwrap();
        assert_eq!(book.used(Category::Captures), (1000, 1));
        // Overwriting counts the difference.
        book.write(Category::Captures, "sight", &caps.join("run-1-1").join("a.png"), &[0u8; 400], Need::Agent).unwrap();
        assert_eq!(book.used(Category::Captures), (400, 1));
        // A synced write is the same write, on disk before it returns (R2 of m5-a11y).
        book.write_synced(Category::Captures, "sight", &caps.join("run-1-1").join("a.png"), &[7u8; 400], Need::Agent).unwrap();
        assert_eq!(book.used(Category::Captures), (400, 1));
        assert_eq!(std::fs::read(caps.join("run-1-1").join("a.png")).unwrap(), vec![7u8; 400]);
        std::fs::write(caps.join("outside-the-book.bin"), [0u8; 50]).unwrap();
        book.measure_now();
        assert_eq!(book.used(Category::Captures), (450, 2), "the walk sees every file, and not scratch's");
        assert_eq!(book.used(Category::Scratch), (300, 1));
        let r = book.report(true);
        assert_eq!(r.get("state").and_then(Json::as_str), Some("ok"), "{r}");
        assert!(r.get("written_this_run").and_then(|w| w.get("sight")).is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Refused below the floor (simulated: a floor above the real free space), with the numbers
    /// and what to delete; essential writes go through and are counted.
    #[test]
    fn writes_below_the_floor_are_refused_with_numbers() {
        let base = scratch("floor");
        let book = DiskBook::default();
        let caps = base.join("captures");
        let Ok(space) = specula_sys::disk::space(&base) else {
            eprintln!("skipped: free space isn't readable on this platform");
            return;
        };
        let floor_mb = space.available / MB + 1024;
        book.configure_local(DiskConfig { free_floor_mb: floor_mb, ..DiskConfig::default() }, vec![(Category::Captures, caps.clone()), (Category::Worlds, base.join("worlds"))], None);
        let e = book.write(Category::Captures, "sight", &caps.join("a.png"), &[1u8; 10], Need::Agent).unwrap_err();
        assert_eq!(e.code, "disk_floor");
        let j = e.to_json();
        assert_eq!(j.get("floor_mb").and_then(Json::as_u64), Some(floor_mb));
        assert!(j.get("available_mb").is_some() && j.get("what_to_delete").is_some(), "{j}");
        assert!(!caps.join("a.png").exists(), "nothing was written");
        book.write(Category::Worlds, "autosave", &base.join("worlds/w/world.json"), b"{}", Need::Essential).unwrap();
        let r = book.report(false);
        assert_eq!(r.get("state").and_then(Json::as_str), Some("below_floor"));
        assert_eq!(r.get("refused").and_then(Json::as_u64), Some(1));
        assert_eq!(r.get("essential_writes_below_floor").and_then(Json::as_u64), Some(1));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The soft line: the write that crosses it gets one warning per session; a worse state warns again.
    #[test]
    fn crossing_the_soft_line_warns_each_session_once() {
        let base = scratch("warn");
        let book = DiskBook::default();
        let caps = base.join("captures");
        let mut cfg = DiskConfig { free_floor_mb: 0, total_mb: 100, ..DiskConfig::default() };
        cfg.caps_mb.insert(Category::Captures, 1);
        book.configure_local(cfg, vec![(Category::Captures, caps.clone())], None);
        book.write(Category::Captures, "sight", &caps.join("a.bin"), &vec![0u8; 500 << 10], Need::Agent).unwrap();
        assert!(book.warning_for("s1").is_none(), "under the line");
        book.write(Category::Captures, "sight", &caps.join("b.bin"), &vec![0u8; 500 << 10], Need::Agent).unwrap();
        let w = book.warning_for("s1").expect("crossed 90% of the 1 MB cap");
        assert_eq!(w.get("crossed").and_then(Json::as_array).map(|a| a.len()), Some(1), "{w}");
        assert!(book.warning_for("s1").is_none(), "once per session");
        assert!(book.warning_for("s2").is_some(), "every session hears it");
        let out = book.annotate("s3", specula_protocol::ToolOutput::text("{\"ok\":true}"));
        let Some(specula_protocol::Content::Text(t)) = out.content.first() else { panic!() };
        assert!(t.contains("disk_warning"), "{t}");
        // Integration check (2026-10-29): a sight tool answers its image first, then the packet:
        // the warning rides on the first text item, the image is untouched.
        let image = specula_protocol::Content::Image { mime_type: "image/png".into(), data_base64: "AAAA".into() };
        let sight = specula_protocol::ToolOutput { content: vec![image.clone(), specula_protocol::Content::Text("{\"image\":{}}".into())], is_error: false };
        let out = book.annotate("s4", sight);
        assert_eq!(out.content[0], image);
        let specula_protocol::Content::Text(t) = &out.content[1] else { panic!() };
        assert!(Json::parse(t).unwrap().get("disk_warning").is_some(), "{t}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Pruning order and protection: the oldest captures go first, a review's capture and another
    /// running engine's run never go, expired scratch goes after its grace, leased scratch never.
    #[test]
    fn pruning_takes_the_oldest_captures_and_spares_what_is_named() {
        let base = scratch("prune");
        let book = DiskBook::default();
        let caps = base.join("captures");
        let scratch_root = base.join("scratch");
        let mine = caps.join(format!("run-9-{}", std::process::id()));
        let dead = caps.join("run-1-4000000001");
        let alive = caps.join(format!("run-2-{}", std::process::id())); // a running engine's (this process)
        for d in [&mine, &dead, &alive] {
            std::fs::create_dir_all(d).unwrap();
        }
        let write = |p: &Path, kb: usize| std::fs::write(p, vec![0u8; kb << 10]).unwrap();
        // Finished runs are known only where process lookups work (elsewhere every run counts as
        // running, conservatively): there the old captures sit in this run's folder.
        let older = if cfg!(windows) { &dead } else { &mine };
        write(&older.join("old.png"), 300);
        std::thread::sleep(Duration::from_millis(30));
        write(&older.join("second.png"), 300);
        std::thread::sleep(Duration::from_millis(30));
        write(&alive.join("theirs.png"), 300);
        write(&mine.join("new.png"), 300);
        std::fs::write(caps.join("notes.txt"), "not a run").unwrap();
        // Scratch: one expired long ago, one still leased, one without a marker.
        let pid = std::process::id();
        for (name, lease, ends) in [(format!("ann-1-100-{pid}"), 1, 1u64), (format!("ann-2-100-{pid}"), 2, u64::MAX / 2)] {
            std::fs::create_dir_all(scratch_root.join(&name)).unwrap();
            write(&scratch_root.join(&name).join("f.bin"), 10);
            write_lease_marker(&scratch_root.join(&name), lease, "ann", ends, false).unwrap();
        }
        std::fs::create_dir_all(scratch_root.join("unmarked")).unwrap();
        let mut cfg = DiskConfig { free_floor_mb: 0, scratch_grace_s: 0, capture_min_age_s: 0, ..DiskConfig::default() };
        cfg.caps_mb.insert(Category::Captures, 1);
        book.configure_local(cfg, vec![(Category::Captures, caps.clone()), (Category::Scratch, scratch_root.clone())], Some(mine.clone()));
        book.measure_now();
        assert_eq!(book.used(Category::Captures).0, (1200 << 10) + 9, "every file, notes.txt too");
        let dry = book.prune(true, "test", 0);
        assert!(older.join("old.png").exists(), "a dry run deletes nothing: {dry}");
        let r = book.prune(false, "test", 0);
        assert!(!older.join("old.png").exists(), "the oldest goes first: {r}");
        assert!(!older.join("second.png").exists() && mine.join("new.png").exists(), "down to 85% of the 1 MB cap, oldest first: old.png, then second.png");
        assert!(alive.join("theirs.png").exists(), "another running engine's captures are its own");
        assert!(caps.join("notes.txt").exists(), "only run folders are pruned");
        assert!(!scratch_root.join(format!("ann-1-100-{pid}")).exists(), "expired scratch went");
        assert!(scratch_root.join(format!("ann-2-100-{pid}")).exists() && scratch_root.join("unmarked").exists(), "leased and unmarked scratch stay");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The dangerous part: a junction inside a captures folder that points outside is never
    /// followed, by the walk, by pruning or by removing a scratch folder.
    #[cfg(windows)]
    #[test]
    fn junctions_out_of_a_root_are_never_followed() {
        let base = scratch("junction");
        let caps = base.join("captures");
        let run = caps.join("run-1-4000000002");
        let outside = base.join("precious");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("thesis.docx"), vec![7u8; 4096]).unwrap();
        if !junction(&run.join("j"), &outside) {
            eprintln!("skipped: can't make a junction here");
            return;
        }
        std::fs::write(run.join("old.png"), vec![0u8; 2048]).unwrap();
        let book = DiskBook::default();
        let mut cfg = DiskConfig { free_floor_mb: 0, capture_min_age_s: 0, ..DiskConfig::default() };
        cfg.caps_mb.insert(Category::Captures, 1);
        book.configure_local(cfg, vec![(Category::Captures, caps.clone())], None);
        book.measure_now();
        assert_eq!(book.used(Category::Captures), (2048, 1), "the walk doesn't count what the junction leads to");
        // Force pruning everything: still nothing outside goes.
        book.wrote(Category::Captures, "test", 4 << 20, 0);
        book.prune(false, "test", 0);
        assert!(outside.join("thesis.docx").exists(), "never followed");
        // Deleting through the junction is refused; removing the tree removes only the link.
        assert!(remove_file_inside(&caps, &run.join("j").join("thesis.docx")).is_err());
        assert!(check_inside(&caps, &run.join("j")).is_err());
        assert!(remove_tree_inside(&caps, &run.join("j")).is_err(), "a link is not a folder to empty");
        let scratch_root = base.join("scratch");
        let lease = scratch_root.join("ann-1-x");
        std::fs::create_dir_all(&lease).unwrap();
        assert!(junction(&lease.join("out"), &outside));
        std::fs::write(lease.join("f.bin"), [1u8; 10]).unwrap();
        let (files, _) = remove_tree_inside(&scratch_root, &lease).unwrap();
        assert_eq!(files, 1, "the file (the link is removed as a link)");
        assert!(!lease.exists());
        assert!(outside.join("thesis.docx").exists(), "the junction's target is untouched");
        // Outside the root, or the root itself: refused.
        assert!(remove_tree_inside(&scratch_root, &outside).is_err());
        assert!(remove_tree_inside(&scratch_root, &scratch_root).is_err());
        assert!(remove_file_inside(&caps, &caps.join("..").join("precious").join("thesis.docx")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    fn old_file(p: &Path, kb: usize) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![0u8; kb << 10]).unwrap();
        let t = std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(7200));
        std::fs::File::options().write(true).open(p).unwrap().set_times(t).unwrap();
    }

    /// Review findings (rounds 1 and 2): over budget mostly because of what is never pruned, a prune
    /// frees what helps (the old capture), never a fresh one, and doesn't run again until use
    /// grows; the state stays over budget and the engine warns.
    #[test]
    fn overage_in_unprunable_data_prunes_once_and_spares_fresh_captures() {
        let base = scratch("unprunable");
        let caps = base.join("captures");
        let run = caps.join(format!("run-9-{}", std::process::id()));
        old_file(&run.join("old.png"), 64);
        std::fs::write(run.join("fresh.png"), vec![0u8; 64 << 10]).unwrap();
        let book = DiskBook::default();
        let cfg = DiskConfig { free_floor_mb: 0, total_mb: 20, ..DiskConfig::default() };
        book.configure_local(cfg, vec![(Category::Captures, caps.clone()), (Category::Worlds, base.join("worlds"))], Some(run.clone()));
        book.measure_now();
        book.wrote(Category::Worlds, "world", 30 << 20, 1);
        let r = book.prune(false, "test", 0);
        assert_eq!(r.get("prunable_mb").and_then(Json::as_f64), Some(0.1), "{r}");
        assert!(!run.join("old.png").exists() && run.join("fresh.png").exists(), "the old capture helps; the fresh one is younger than capture_min_age_s");
        let rep = book.report(false);
        assert_eq!(rep.get("state").and_then(Json::as_str), Some("over_budget"));
        assert!(rep.get("warning").is_some(), "warned");
        // Nothing more to gain: no prune is wanted until use grows past what is left.
        book.wrote(Category::Captures, "test", 100 << 10, 1);
        assert!(!book.lock().prune_wanted, "not again for 100 KB");
        book.wrote(Category::Captures, "test", 2 << 20, 1);
        assert!(book.lock().prune_wanted, "again once use grew");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review probe (round 2): live scratch made the overage, and counted as prunable, so old
    /// captures were wiped on every prune while the total stayed over budget. Live scratch isn't
    /// prunable; one prune takes what helps, and the walker doesn't come back for nothing.
    #[test]
    fn live_scratch_overage_doesnt_wipe_captures_every_prune() {
        let base = scratch("r2live");
        let caps = base.join("captures");
        let run = caps.join(format!("run-9-{}", std::process::id()));
        old_file(&run.join("a.png"), 2048);
        let sroot = base.join("scratch");
        let lease = sroot.join(format!("ann-1-100-{}", std::process::id()));
        std::fs::create_dir_all(&lease).unwrap();
        std::fs::write(lease.join("target.bin"), vec![0u8; 15 << 20]).unwrap();
        write_lease_marker(&lease, 1, "ann", u64::MAX / 2, false).unwrap();
        let book = DiskBook::default();
        let cfg = DiskConfig { free_floor_mb: 0, total_mb: 20, ..DiskConfig::default() };
        book.configure_local(cfg, vec![(Category::Captures, caps.clone()), (Category::Scratch, sroot.clone()), (Category::Worlds, base.join("worlds"))], Some(run.clone()));
        book.set_live_scratch([canonical(&lease)].into());
        book.measure_now();
        book.wrote(Category::Worlds, "world", 6 << 20, 1);
        let r = book.prune(false, "test", 0);
        assert_eq!(r.get("prunable_mb").and_then(Json::as_f64), Some(2.0), "live scratch isn't prunable: {r}");
        assert!(lease.join("target.bin").exists());
        old_file(&run.join("b.png"), 512);
        book.wrote(Category::Captures, "test", 512 << 10, 1);
        book.step();
        assert!(run.join("b.png").exists(), "the walker doesn't come back to wipe the next capture");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review probe (round 2): pruning the old capture gets the total under budget, but nothing
    /// was pruned because it couldn't reach 85% of it. Now what helps is pruned.
    #[test]
    fn a_prune_that_helps_partly_still_runs() {
        let base = scratch("r2partial");
        let caps = base.join("captures");
        let run = caps.join(format!("run-9-{}", std::process::id()));
        old_file(&run.join("old.png"), 4096);
        let book = DiskBook::default();
        let cfg = DiskConfig { free_floor_mb: 0, total_mb: 20, ..DiskConfig::default() };
        book.configure_local(cfg, vec![(Category::Captures, caps.clone()), (Category::Worlds, base.join("worlds"))], Some(run.clone()));
        book.measure_now();
        book.wrote(Category::Worlds, "world", 19 << 20, 1);
        let r = book.prune(false, "test", 0);
        assert!(!run.join("old.png").exists(), "23 MB over a 20 MB budget: the 4 MB old capture goes: {r}");
        assert_ne!(book.report(false).get("state").and_then(Json::as_str), Some("over_budget"), "under budget again");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review probe (round 3): the mark a prune left was never lowered when use fell (a world
    /// deleted by hand) but stayed over budget, so new prunable captures waited for nothing.
    #[test]
    fn the_unhelped_mark_follows_use_down() {
        let base = scratch("unhelped");
        let caps = base.join("captures");
        let run = caps.join(format!("run-9-{}", std::process::id()));
        std::fs::create_dir_all(&run).unwrap();
        let book = DiskBook::default();
        let cfg = DiskConfig { free_floor_mb: 0, total_mb: 20, ..DiskConfig::default() };
        book.configure_local(cfg, vec![(Category::Captures, caps.clone()), (Category::Worlds, base.join("worlds"))], Some(run.clone()));
        book.measure_now();
        book.wrote(Category::Worlds, "world", 30 << 20, 1);
        book.prune(false, "test", 0);
        assert_eq!(book.lock().unhelped_at.map(|b| b >> 20), Some(30));
        // A world is deleted by hand: 21 MB of world files left.
        book.wrote(Category::Worlds, "world", -(9 << 20), -1);
        // 8 MB of old captures come in: 29 MB, 9 over; pruning frees 8.
        for i in 0..8 {
            old_file(&run.join(format!("c{i}.png")), 1024);
        }
        book.wrote(Category::Captures, "test", 8 << 20, 8);
        assert!(book.lock().prune_wanted, "use grew past what the last prune left (21 MB): prune again");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review finding (round 2): the agents' prune cooldown was set by dry runs and before a busy
    /// answer. Only a real prune that ran starts it.
    #[test]
    fn only_a_real_prune_starts_the_cooldown() {
        let base = scratch("cooldown");
        let book = DiskBook::default();
        book.configure_local(DiskConfig { free_floor_mb: 0, ..DiskConfig::default() }, vec![(Category::Captures, base.join("captures"))], None);
        assert!(book.prune_for_agent(true, "t", 0).is_ok() && book.prune_for_agent(true, "t", 0).is_ok(), "dry runs aren't limited");
        {
            let _busy = book.pruning.lock().unwrap();
            let r = book.prune_for_agent(false, "t", 0).unwrap();
            assert_eq!(r.get("error").and_then(Json::as_str), Some("busy"));
        }
        assert!(book.prune_for_agent(false, "t", 0).is_ok(), "a busy answer didn't start the cooldown");
        assert_eq!(book.prune_for_agent(false, "t", 0).unwrap_err().code, "too_soon");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review finding: a marker dropped into any folder under the scratch root got it removed.
    /// Only a folder named as a lease names it, with a marker saying the same, and not live.
    #[test]
    fn forged_lease_markers_remove_nothing() {
        let base = scratch("forged");
        let root = base.join("scratch");
        let pid = std::process::id();
        let mk = |name: &str, lease: u64, owner: &str| {
            let d = root.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("work.txt"), "x").unwrap();
            write_lease_marker(&d, lease, owner, 1, false).unwrap();
            d
        };
        let victim = mk("someones-results", 1, "ann");
        let wrong_lease = mk(&format!("ann-1-100-{pid}"), 2, "ann");
        let wrong_owner = mk(&format!("bob-3-100-{pid}"), 3, "ann");
        let live = mk(&format!("ann-4-100-{pid}"), 4, "ann");
        let real = mk(&format!("ann-5-100-{pid}"), 5, "ann");
        let book = DiskBook::default();
        book.configure_local(DiskConfig { free_floor_mb: 0, scratch_grace_s: 0, ..DiskConfig::default() }, vec![(Category::Scratch, root.clone())], None);
        book.set_live_scratch([canonical(&live)].into());
        book.prune(false, "test", 0);
        for d in [&victim, &wrong_lease, &wrong_owner, &live] {
            assert!(d.exists(), "{} must stay", d.display());
        }
        assert!(!real.exists(), "a real expired lease folder goes");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Review probe: a folder swapped for a junction while it is being emptied never leads the
    /// delete outside the root.
    #[cfg(windows)]
    #[test]
    fn a_folder_swapped_for_a_junction_mid_delete_stays_inside() {
        // What the machine ran short of is not the code's fault: OS error 1450 ("insufficient
        // system resources") from a write of the setup failed this test once in a loaded full run
        // (2026-11-24), and an `exists()` that fails for that reason reads as a file lost. The
        // setup's writes and the reading of what is left say their errors; one of the machine's
        // own shortages (memory, the page file, the disk, system resources) skips the test with
        // its reason. The delete under test is never asked: its result is not what is held, only
        // that nothing outside was lost, and a file found missing is always a failure.
        let b = scratch("race");
        let shortage = |e: &std::io::Error| matches!(e.raw_os_error(), Some(8 | 14 | 112 | 1450 | 1453 | 1455));
        match junction_race(&b) {
            Ok(()) => {}
            Err(e) if shortage(&e) => {
                let (test, why) = ("a_folder_swapped_for_a_junction_mid_delete_stays_inside", format!("the machine ran short ({e}): not the code under test"));
                crate::skip_test(test, &why);
                // Said in the gate's log too, past the harness's capture (a trace event alone is
                // seen by nobody there), so a test that skips on every run cannot read as a pass.
                use std::io::Write as _;
                let _ = writeln!(std::io::stderr(), "{test}: SKIPPED — {why}");
            }
            Err(e) => panic!("the test's own setup failed: {e}"),
        }
        let _ = std::fs::remove_dir_all(&b);
    }

    #[cfg(windows)]
    fn junction_race(b: &Path) -> std::io::Result<()> {
        let outside = b.join("outside");
        let root = b.join("scratch");
        std::fs::create_dir_all(&outside)?;
        std::fs::create_dir_all(&root)?;
        const N: usize = 1500;
        for k in 0..N {
            std::fs::write(outside.join(format!("f{k:05}")), b"precious")?;
        }
        for round in 0..3 {
            let lease = root.join(format!("lease-{round}"));
            let sub = lease.join("sub");
            std::fs::create_dir_all(&sub)?;
            for k in 0..N {
                std::fs::write(sub.join(format!("f{k:05}")), b"scratch")?;
            }
            let jtmp = b.join(format!("jtmp-{round}"));
            if !junction(&jtmp, &outside) {
                crate::skip_test("a_folder_swapped_for_a_junction_mid_delete_stays_inside", "can't make a junction here");
                return Ok(());
            }
            let (s2, l2, j2) = (sub.clone(), lease.clone(), jtmp.clone());
            let racer = std::thread::spawn(move || {
                let start = Instant::now();
                while std::fs::read_dir(&s2).map(|r| r.count()).unwrap_or(0) >= N - 100 && start.elapsed() < Duration::from_secs(5) {}
                let _ = std::fs::rename(&s2, l2.join("sub_old"));
                let _ = std::fs::rename(&j2, &s2);
            });
            let _ = remove_tree_inside(&root, &lease);
            racer.join().unwrap();
            // A file is there, or it is not (lost); any other answer is an error said, never
            // read as either.
            // (A file found lost fails the test before any error of the reading is answered.)
            let (mut lost, mut unread) = (0, None);
            for k in 0..N {
                match std::fs::symlink_metadata(outside.join(format!("f{k:05}"))) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => lost += 1,
                    Err(e) => unread = Some(e),
                }
            }
            assert_eq!(lost, 0, "round {round}: files outside the root were deleted through a swapped junction");
            if let Some(e) = unread {
                return Err(e);
            }
            let _ = std::fs::remove_dir(&sub);
            let _ = std::fs::remove_dir(&jtmp);
            let _ = std::fs::remove_dir_all(&lease);
        }
        Ok(())
    }

    /// A root that is itself a link is resolved once (canonical) and everything below is built from it.
    #[test]
    fn deletes_outside_or_through_dot_dot_are_refused() {
        let base = scratch("dots");
        std::fs::create_dir_all(base.join("root/a")).unwrap();
        std::fs::write(base.join("keep.txt"), "x").unwrap();
        let root = base.join("root");
        assert!(remove_file_inside(&root, &base.join("keep.txt")).is_err());
        assert!(remove_file_inside(&root, &root.join("a").join("..").join("..").join("keep.txt")).is_err());
        assert!(base.join("keep.txt").exists());
        assert_eq!(parse_run_name("run-12-34"), Some((12, 34)));
        assert_eq!(parse_run_name("run-12-34x"), None);
        let _ = std::fs::remove_dir_all(&base);
    }
}
