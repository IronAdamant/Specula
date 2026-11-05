//! Review evidence: copies of the captures and packets that review submissions and critiques
//! name, kept beside the review queue in the world's own folder (`<world>/evidence/`), so a
//! reviewer's evidence never depends on the captures folder, which the disk budget prunes.
//! Design: docs/design/DISK.md ("Review evidence").
//!
//! - **One confinement.** Every path a review names is resolved by
//!   [`crate::safe_path::review_file`]: lexical checks first (no UNC, verbatim, drive-relative or
//!   `..` name ever reaches the disk), then only a capture or a file in a world's `evidence`
//!   folder, never a world's own files.
//! - **Off the engine thread.** `review_submit` and `critique_add` copy their files on the calling
//!   agent's connection thread ([`prepare`]) before the engine sees the call: each named file is
//!   read once per call, its copy compared by length before it is read, at most 64 MiB per call
//!   (`evidence_budget`). The engine then only checks that the copies are there. A call that
//!   doesn't come through there (a playtest step's, run on the engine thread) reads at most 4 MiB.
//! - **Refused calls leave nothing behind.** The arguments are checked before anything is copied;
//!   a call refused after copying (a journal entry, a claim, a later file) hands its copies back
//!   ([`Made`]), and collection removes them once a minute old unless the queue names them.
//! - **Relative references.** The queue names `evidence/<hash>-<len>.<ext>`, relative to its own
//!   folder: a world folder copied or moved keeps its evidence, and no home path lands in world
//!   files. Copies, not hard links: a link shares the capture's bytes, so pruning would free
//!   nothing while the budget counted them twice.
//! - **World files are never pruned**, so the evidence lives as long as the queue names it (counted
//!   under `worlds`). An unsaved world's queue keeps it in this run's
//!   `<worlds>/.review-unsaved-<run>/evidence/`; `world_save` under a name copies it into the world
//!   (on a worker thread). A run's unsaved folder carries a heartbeat (`.alive`), and folders whose
//!   heartbeat stopped ten minutes ago are removed at start, whatever their process id.
//! - **Loading** adopts the queue's evidence: old references (absolute, into the captures or
//!   another world's evidence) are matched against `evidence/<file name>` with the same bytes
//!   first, then copied in while their files exist; references whose file is gone are reported
//!   (`missing: true` in review_list, a warning in the world_load answer).
//! - **Collection never guesses.** Only files this run's queue named and then dropped, or a
//!   refused call copied, are removed (at most once a minute, only files older than a minute; a
//!   copy found again is marked as just used); nothing is removed while the
//!   load reported a problem, the queue isn't saved, or an `.unreadable-*` queue sits in the folder.

use crate::disk::{self, Category, Need};
use crate::safe_path::ReviewRoots;
use crate::world::WorldError;
use crate::Engine;
use specula_core::hash::StableHasher;
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolOutput};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// The evidence folder inside a queue's folder, and the prefix of the references to it.
pub const EVIDENCE_DIR: &str = "evidence";
const REF_PREFIX: &str = "evidence/";
/// Largest file kept as evidence, a world's evidence in all, and what one call may copy.
pub const MAX_FILE: u64 = 16 << 20;
pub const MAX_BYTES: u64 = 2 << 30;
pub const MAX_CALL_BYTES: u64 = 64 << 20;
/// What one call copied on the engine thread (a playtest step's) may read in all.
pub const MAX_ENGINE_CALL_BYTES: u64 = 4 << 20;
/// Refused calls' copies remembered for a queue folder that isn't the current one (a world
/// switched between the copy and the answer), until that folder is current again.
const MAX_STRAYS: usize = 4096;
/// An unsaved world's queue folder: `<worlds>/.review-unsaved-<run>`, with its heartbeat file.
const UNSAVED_PREFIX: &str = ".review-unsaved-";
const ALIVE_FILE: &str = ".alive";
const ALIVE_EVERY: Duration = Duration::from_secs(60);
const DEAD_AFTER: Duration = Duration::from_secs(600);
/// Dropped evidence is removed this often at most (after the queue changed), only once it is
/// this old.
const GC_EVERY: Duration = Duration::from_secs(60);
const GC_MIN_AGE: Duration = Duration::from_secs(60);
/// Same-name files tried per content (a different content with the same name takes the next).
const MAX_SAME_NAME: u32 = 16;

fn refuse(code: &'static str, message: String, file: &str) -> WorldError {
    WorldError::new(code, message).with("file", file)
}

/// `<hash>-<len>[-n].<ext>`: the name a content gets in an evidence folder.
pub(crate) fn content_name(bytes: &[u8], ext: &str, n: u32) -> String {
    let mut h = StableHasher::with_seed(0x6576_6964_656e_6365);
    h.write_bytes(bytes);
    let ext: String = ext.chars().filter(char::is_ascii_alphanumeric).take(8).collect::<String>().to_ascii_lowercase();
    let ext = if ext.is_empty() { "bin".to_owned() } else { ext };
    if n == 0 {
        format!("{:016x}-{}.{ext}", h.finish(), bytes.len())
    } else {
        format!("{:016x}-{}-{n}.{ext}", h.finish(), bytes.len())
    }
}

pub(crate) fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'])
}

/// The file an `evidence/<file>` reference names (`None`: not such a reference).
fn ref_file(r: &str) -> Option<&str> {
    r.strip_prefix(REF_PREFIX).or_else(|| r.strip_prefix("evidence\\")).filter(|f| !f.is_empty() && !f.contains(['/', '\\']))
}

/// The name of this run's unsaved queue folder.
fn unsaved_name(run: &Path) -> String {
    let run = run.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("run-0-{}", std::process::id()));
    format!("{UNSAVED_PREFIX}{run}")
}

/// Removes unsaved queue folders under `worlds` whose run stopped (its heartbeat is ten minutes
/// old, or it never had one and the folder is), never this run's (`run`: this run's captures).
pub fn clean_unsaved(worlds: &Path, run: &Path) {
    let root = disk::canonical(worlds);
    let mine = unsaved_name(run);
    let Ok(entries) = std::fs::read_dir(&root) else { return };
    let stale = SystemTime::now().checked_sub(DEAD_AFTER).unwrap_or(SystemTime::UNIX_EPOCH);
    for e in entries.flatten() {
        let Some(name) = e.file_name().to_str().map(str::to_owned) else { continue };
        if !name.starts_with(UNSAVED_PREFIX) || name == mine {
            continue;
        }
        let dir = root.join(&name);
        let beat = std::fs::metadata(dir.join(ALIVE_FILE)).or_else(|_| std::fs::metadata(&dir)).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        if beat < stale {
            if let Ok((n, b)) = disk::remove_tree_inside(&root, &dir) {
                disk::book().wrote(Category::Worlds, "evidence", -(b as i64), -(n as i64));
            }
        }
    }
}

/// A world's evidence bytes, measured once per folder per process and kept up to date.
static USED: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

fn used_in(dir: &Path) -> u64 {
    let mut used = USED.lock().unwrap_or_else(|p| p.into_inner());
    *used.entry(dir.to_path_buf()).or_insert_with(|| {
        std::fs::read_dir(dir).map(|it| it.flatten().take(100_000).filter_map(|e| std::fs::symlink_metadata(e.path()).ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()).unwrap_or(0)
    })
}

fn add_used(dir: &Path, delta: i64) {
    let mut used = USED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(b) = used.get_mut(dir) {
        *b = (*b as i64 + delta).max(0) as u64;
    }
}

/// What one call has copied: each source once, within the call's budget.
pub struct CallCopies {
    done: BTreeMap<String, String>,
    budget: u64,
    /// Sources read (a test counts them).
    pub reads: usize,
    /// The references this call copied in or found already copied (`evidence/<file>`): what a
    /// refused call hands back to collection (see [`Made`]).
    made: Vec<String>,
    /// On the engine thread (a playtest step's call, see [`CallCopies::engine_thread`]): every
    /// read counts against the budget, not only the copies.
    engine: bool,
}

impl Default for CallCopies {
    fn default() -> Self {
        CallCopies { done: BTreeMap::new(), budget: MAX_CALL_BYTES, reads: 0, made: Vec::new(), engine: false }
    }
}

impl CallCopies {
    /// A call whose files are copied on the engine thread (one that didn't come through
    /// [`prepare`]: a playtest step's `review_submit` or `critique_add`). At most
    /// [`MAX_ENGINE_CALL_BYTES`] read in all (copies, and the reads that find a copy already
    /// there), so a step holds the engine up for a few milliseconds at most.
    pub fn engine_thread() -> CallCopies {
        CallCopies { budget: MAX_ENGINE_CALL_BYTES, engine: true, ..CallCopies::default() }
    }

    /// The references this call made.
    #[cfg(test)]
    pub fn made(&self) -> &[String] {
        &self.made
    }
}

/// What a call through [`prepare`] copied into which queue's evidence. When the call is refused
/// (an argument, a journal entry, a claim...), its copies go back to collection
/// ([`Engine::evidence_refused`]): nothing it copied stays behind unnamed for good.
pub struct Made {
    pub queue: PathBuf,
    pub refs: Vec<String>,
}

/// Keeps the file an agent named as evidence of the queue in `roots.queue`; returns its reference
/// (`evidence/<file>`). `png`: it must be a PNG. A file named twice in one call is read once.
pub fn copy_in(roots: &ReviewRoots, given: &str, png: bool, call: &mut CallCopies) -> Result<String, WorldError> {
    if let Some(done) = call.done.get(given) {
        return Ok(done.clone());
    }
    let source = crate::safe_path::review_file(roots, given, "the file").map_err(|r| refuse(r.code, r.message, given))?;
    let worlds = std::path::absolute(&roots.worlds).unwrap_or_else(|_| roots.worlds.clone());
    let dir = std::path::absolute(&roots.queue).unwrap_or_else(|_| roots.queue.clone()).join(EVIDENCE_DIR);
    // Already this queue's evidence: named as such.
    if source.parent() == Some(dir.as_path()) {
        let r = format!("{REF_PREFIX}{}", source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        call.done.insert(given.to_owned(), r.clone());
        return Ok(r);
    }
    let len = std::fs::symlink_metadata(&source).map(|m| m.len()).map_err(|_| refuse("capture_missing", format!("'{given}' is not a file"), given))?;
    if len > MAX_FILE {
        return Err(refuse("too_large", format!("'{given}' is {} MB; evidence files are at most {} MB", len >> 20, MAX_FILE >> 20), given).with("bytes", len));
    }
    if len > call.budget {
        if call.engine {
            return Err(refuse(
                "evidence_budget",
                format!(
                    "a call run on the engine thread (a playtest step) reads at most {} MB of review evidence; '{given}' would pass it: submit big captures over MCP (copied off the engine thread), or capture smaller frames in the playtest",
                    MAX_ENGINE_CALL_BYTES >> 20
                ),
                given,
            )
            .with("limit_mb", MAX_ENGINE_CALL_BYTES >> 20)
            .with("engine_thread", true));
        }
        return Err(refuse("evidence_budget", format!("one call copies at most {} MB of evidence; '{given}' would pass it: submit in smaller parts", MAX_CALL_BYTES >> 20), given).with("limit_mb", MAX_CALL_BYTES >> 20));
    }
    let bytes = std::fs::read(&source).map_err(|_| refuse("capture_missing", format!("'{given}' can't be read"), given))?;
    call.reads += 1;
    if call.engine {
        call.budget -= len;
    }
    if png && !is_png(&bytes) {
        return Err(refuse("capture_missing", format!("'{given}' is not a PNG (pass the image.path a sight tool returned)"), given));
    }
    // The evidence folder is the world's own: never written through a link.
    crate::project::check_inside(&worlds, &dir).map_err(|e| refuse("linked_folder", format!("the evidence folder: {e}"), given))?;
    disk::book().make_dir(Category::Worlds, &dir, Need::Agent).map_err(|e| e.to_world())?;
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin").to_owned();
    for n in 0..MAX_SAME_NAME {
        let name = content_name(&bytes, &ext, n);
        let p = dir.join(&name);
        match std::fs::symlink_metadata(&p) {
            // Same name and length: read it only now, to be sure it is the same content.
            Ok(m) if m.is_file() && m.len() == len => {
                if std::fs::read(&p).is_ok_and(|there| there == bytes) {
                    // Named again: collection (which waits for a file to be a minute old) leaves
                    // it alone until this call's answer has named it or handed it back.
                    if let Err(e) = disk::touch_inside(&disk::canonical(&worlds), &disk::canonical(&p)) {
                        eprintln!("specula: review evidence {name} not marked as used: {e}");
                    }
                    let r = format!("{REF_PREFIX}{name}");
                    call.done.insert(given.to_owned(), r.clone());
                    call.made.push(r.clone());
                    return Ok(r);
                }
            }
            Ok(_) => {}
            Err(_) => {
                let used = used_in(&dir);
                if used + len > MAX_BYTES {
                    return Err(refuse("evidence_full", format!("this world's review evidence holds {} MB, its limit is {} MB: decide (and let the queue drop) old submissions first", used >> 20, MAX_BYTES >> 20), given));
                }
                disk::book().write(Category::Worlds, "evidence", &p, &bytes, Need::Agent).map_err(|e| e.to_world())?;
                add_used(&dir, len as i64);
                if !call.engine {
                    call.budget -= len;
                }
                let r = format!("{REF_PREFIX}{name}");
                call.done.insert(given.to_owned(), r.clone());
                call.made.push(r.clone());
                return Ok(r);
            }
        }
    }
    Err(refuse("evidence_full", format!("{MAX_SAME_NAME} different files share '{given}''s content name"), given))
}

/// `review_submit` and `critique_add`, on the calling agent's connection thread: copies the files
/// they name into the queue's evidence and answers the arguments naming the copies, so the
/// engine thread never copies. Errors are the tool's answer. Also answers what it copied (even
/// when a later file is refused), so a refused call hands its copies back to collection.
pub fn prepare(tools: &crate::tools::Tools, session: &SessionInfo, name: &str, args: &Json) -> (Result<Json, ToolOutput>, Option<Made>) {
    // A submission's references to recorded results: checked here (each check's inputs read
    // again and digested), and the recorded captures join its captures, before anything is
    // copied; their copies are digested after (`records::verify`).
    let (referenced, plan);
    let args = if name == "review_submit" {
        (referenced, plan) = match crate::records::prepare(tools.studio(), args) {
            Ok(a) => a,
            Err(e) => return (Err(e), None),
        };
        &referenced
    } else {
        plan = crate::records::Plan::default();
        args
    };
    let out = tools.send(session, crate::tools::Command::ReviewRoots);
    let text = out.content.iter().find_map(|c| if let specula_protocol::Content::Text(t) = c { Some(t.clone()) } else { None }).unwrap_or_default();
    let Ok(j) = Json::parse(&text) else { return (Err(out), None) };
    let path = |k: &str| PathBuf::from(j.get(k).and_then(Json::as_str).unwrap_or_default());
    let roots = ReviewRoots { captures: path("captures"), run: path("run"), worlds: path("worlds"), queue: path("queue") };
    let mut call = CallCopies::default();
    let mut answer = prepare_in(&roots, name, args, &mut call);
    if let Ok(prepared) = &answer {
        if let Err(e) = crate::records::verify(tools.studio(), &session.id, &roots.queue, prepared, &plan) {
            answer = Err(e);
        }
    }
    let made = (!call.made.is_empty()).then(|| Made { queue: roots.queue.clone(), refs: std::mem::take(&mut call.made) });
    (answer, made)
}

/// [`prepare`]'s copies, given the roots (`call` records what was made).
pub fn prepare_in(roots: &ReviewRoots, name: &str, args: &Json, call: &mut CallCopies) -> Result<Json, ToolOutput> {
    let fail = |e: WorldError| ToolOutput::error(e.to_json().to_string());
    let mut args = args.clone();
    let one = |v: &mut Json, png: bool, field: String, call: &mut CallCopies| -> Result<(), ToolOutput> {
        if let Some(given) = v.as_str().map(str::to_owned) {
            let r = copy_in(roots, &given, png, call).map_err(|e| fail(WorldError { message: format!("{field}: {}", e.message), ..e }.with("field", field.as_str())))?;
            *v = Json::from(r);
        }
        Ok(())
    };
    match name {
        "review_submit" => {
            if let Some(Json::Arr(caps)) = args.get("captures").cloned() {
                let mut new = Vec::with_capacity(caps.len());
                for (i, mut c) in caps.into_iter().enumerate() {
                    if c.as_str().is_some() {
                        one(&mut c, true, format!("captures[{i}]"), call)?;
                    } else {
                        if let Some(mut p) = c.get("path").cloned() {
                            one(&mut p, true, format!("captures[{i}]"), call)?;
                            c.set("path", p);
                        }
                        if let Some(mut p) = c.get("packet").cloned() {
                            one(&mut p, false, format!("captures[{i}].packet"), call)?;
                            c.set("packet", p);
                        }
                    }
                    new.push(c);
                }
                args.set("captures", Json::Arr(new));
            }
        }
        "critique_add" => {
            if let Some(Json::Arr(items)) = args.get("items").cloned() {
                let mut new = Vec::with_capacity(items.len());
                for (i, mut it) in items.into_iter().enumerate() {
                    for k in ["capture", "packet"] {
                        if let Some(mut p) = it.get(k).cloned() {
                            one(&mut p, false, format!("items[{i}].{k}"), call)?;
                            it.set(k, p);
                        }
                    }
                    new.push(it);
                }
                args.set("items", Json::Arr(new));
            }
        }
        _ => {}
    }
    Ok(args)
}

/// What the engine keeps about evidence between calls.
#[derive(Default)]
pub struct EvidenceState {
    /// The queue folder the record below is about.
    root: Option<PathBuf>,
    /// References this run's queue has named in that folder (collection removes only those of
    /// them it no longer names).
    seen: BTreeSet<String>,
    /// The load reported a problem: nothing is collected in this folder this run.
    blocked: bool,
    /// The queue revision last collected after, and when.
    gc: Option<(u64, Instant)>,
    /// `seen` may hold references the queue doesn't name that weren't old enough to remove yet
    /// (or a refused call just handed some back): collect again a minute on, even if the queue
    /// didn't change.
    pending: bool,
    /// Refused calls' copies in queue folders that aren't the current one (a world switched
    /// between the copy and the answer): handed to `seen` when that folder is current again.
    strays: BTreeMap<PathBuf, BTreeSet<String>>,
    /// The unsaved folder's last heartbeat.
    alive: Option<Instant>,
}

impl EvidenceState {
    /// Takes the strays of `root`, which just became the current queue's folder.
    fn adopt_strays(&mut self, root: &Path) {
        if let Some(s) = self.strays.remove(root) {
            self.seen.extend(s);
            self.pending = true;
        }
    }

    /// One collection pass over `dir` (the current queue's evidence folder, inside `worlds`):
    /// removes the files `seen` holds that the queue doesn't name (`named`) and that weren't
    /// modified since `old`. Returns whether some of them were too young (collect again later).
    fn collect(&mut self, worlds: &Path, dir: &Path, named: &BTreeSet<String>, old: SystemTime) -> bool {
        let mut young = false;
        let dropped: Vec<String> = self.seen.iter().filter(|r| !named.contains(*r)).cloned().collect();
        for r in dropped {
            let Some(f) = ref_file(&r) else {
                self.seen.remove(&r);
                continue;
            };
            let p = disk::canonical(&dir.join(f));
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                self.seen.remove(&r);
                continue;
            };
            if !m.is_file() {
                continue;
            }
            if m.modified().is_ok_and(|t| t > old) {
                young = true;
                continue;
            }
            if let Ok(b) = disk::remove_file_inside(worlds, &p) {
                disk::book().wrote(Category::Worlds, "evidence", -(b as i64), -1);
                add_used(dir, -(b as i64));
                self.seen.remove(&r);
            }
        }
        young
    }
}

impl Engine {
    /// The folder holding the current queue (and its evidence): the world's when it has a name,
    /// else this run's unsaved folder.
    pub(crate) fn evidence_root(&self) -> PathBuf {
        match &self.world_name {
            Some(n) => self.worlds_dir.join(n),
            None => self.worlds_dir.join(unsaved_name(&self.capture_dir)),
        }
    }

    /// Where a review's files may be (see [`crate::safe_path::review_file`]).
    pub(crate) fn review_roots(&self) -> ReviewRoots {
        ReviewRoots { captures: self.captures_root().to_path_buf(), run: self.capture_dir.clone(), worlds: self.worlds_dir.clone(), queue: self.evidence_root() }
    }

    /// `Command::ReviewRoots`: the roots, for a connection thread's copies.
    pub(crate) fn review_roots_json(&self) -> ToolOutput {
        let r = self.review_roots();
        let s = |p: &Path| Json::from(p.display().to_string());
        ToolOutput::text(Json::obj([("captures", s(&r.captures)), ("run", s(&r.run)), ("worlds", s(&r.worlds)), ("queue", s(&r.queue))]).to_string())
    }

    /// A file a submission or critique names, as the queue will name it: an `evidence/` reference
    /// already copied (checked to be there), else copied now (a call that didn't come through
    /// [`prepare`], like a playtest's).
    pub(crate) fn keep_evidence(&mut self, given: &str, png: bool, call: &mut CallCopies) -> Result<String, WorldError> {
        let roots = self.review_roots();
        let r = match ref_file(given) {
            Some(_) => {
                crate::safe_path::review_file(&roots, given, "the file").map_err(|e| refuse(e.code, e.message, given))?;
                given.replace('\\', "/")
            }
            None => copy_in(&roots, given, png, call)?,
        };
        self.note_root();
        self.evidence.seen.insert(r.clone());
        // Named, or about to be: if the call is refused, a later pass collects it.
        self.evidence.pending = true;
        Ok(r)
    }

    /// Starts the record over when the queue's folder changed.
    fn note_root(&mut self) {
        let root = self.evidence_root();
        if self.evidence.root.as_ref() != Some(&root) {
            self.evidence.seen.clear();
            self.evidence.blocked = false;
            self.evidence.gc = None;
            self.evidence.pending = false;
            self.evidence.adopt_strays(&root);
            self.evidence.root = Some(root);
        }
    }

    /// `Command::EvidenceRefused`: a review_submit or critique_add was refused after
    /// [`prepare`] copied its files. They go to collection like dropped evidence: removed once
    /// a minute old unless the queue names them (another call may have named the same content).
    pub(crate) fn evidence_refused(&mut self, made: Made) -> ToolOutput {
        self.note_root();
        let refs = made.refs.into_iter().filter(|r| ref_file(r).is_some());
        if made.queue == self.evidence_root() {
            self.evidence.seen.extend(refs);
            self.evidence.pending = true;
        } else {
            let room = MAX_STRAYS.saturating_sub(self.evidence.strays.values().map(BTreeSet::len).sum());
            let kept: BTreeSet<String> = refs.take(room).collect();
            if !kept.is_empty() {
                self.evidence.strays.entry(made.queue).or_default().extend(kept);
            }
        }
        ToolOutput::text("{}")
    }

    /// Makes the queue's evidence live in `root` (a world just loaded, or just named by
    /// world_save): `evidence/<file>` references must be in its evidence folder; old references
    /// (absolute) are matched against `evidence/<file name>` with the same bytes first, then
    /// copied in while their files exist. `load_problem`: the load reported one (nothing is
    /// collected in this folder this run). Returns `(references changed, references missing)`.
    pub(crate) fn adopt_evidence(&mut self, root: &Path, load_problem: bool) -> (usize, usize) {
        let roots = ReviewRoots { queue: root.to_path_buf(), ..self.review_roots() };
        let dir = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()).join(EVIDENCE_DIR);
        let refs: BTreeSet<String> = self.swarm.review.file_refs().into_iter().map(str::to_owned).collect();
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        let mut missing = 0;
        let mut call = CallCopies { budget: MAX_BYTES, ..CallCopies::default() };
        for r in &refs {
            if let Some(f) = ref_file(r) {
                if !dir.join(f).is_file() {
                    missing += 1;
                }
                continue;
            }
            // An old reference: the same file already here (a moved world) wins.
            let name = Path::new(r).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let here = dir.join(&name);
            let source = crate::safe_path::review_file(&roots, r, "the file").ok();
            let same = here.is_file() && source.as_ref().is_none_or(|s| std::fs::read(s).ok() == std::fs::read(&here).ok());
            if same {
                map.insert(r.clone(), format!("{REF_PREFIX}{name}"));
                continue;
            }
            match source {
                Some(_) => match copy_in(&roots, r, false, &mut call) {
                    Ok(new) => {
                        map.insert(r.clone(), new);
                    }
                    Err(e) => {
                        eprintln!("specula: review evidence {r} not kept: {}", e.message);
                        missing += 1;
                    }
                },
                None => missing += 1,
            }
        }
        let n = if map.is_empty() { 0 } else { self.swarm.review.map_files(&mut |s| map.get(s).cloned()) };
        // The record for this folder starts here.
        self.evidence.root = Some(root.to_path_buf());
        self.evidence.seen = self.swarm.review.file_refs().into_iter().filter(|r| ref_file(r).is_some()).map(str::to_owned).collect();
        self.evidence.blocked = load_problem;
        self.evidence.gc = None;
        self.evidence.pending = false;
        self.evidence.adopt_strays(root);
        self.review_ui.changed(&self.swarm.review);
        self.review_ui.set_roots(ReviewRoots { queue: root.to_path_buf(), ..self.review_roots() });
        // The queue in that folder names its evidence from now on.
        if n > 0 && root.join(crate::review_engine::REVIEW_FILE).exists() && !load_problem {
            let text = self.swarm.review.render_cached();
            if let Err(e) = crate::review_engine::write_review(root, &text) {
                eprintln!("specula: review queue not rewritten with its evidence in {}: {e}", root.display());
            }
        }
        (n, missing)
    }

    /// world_save under a new name: the queue's evidence goes along (copied on a worker thread;
    /// the references are relative, so they stay). Returns how many files are being copied.
    pub(crate) fn carry_evidence(&mut self, from: &Path, to: &Path) -> usize {
        let names: Vec<String> = self.swarm.review.file_refs().into_iter().filter_map(ref_file).map(str::to_owned).collect::<BTreeSet<_>>().into_iter().collect();
        let (src, dst) = (from.join(EVIDENCE_DIR), to.join(EVIDENCE_DIR));
        let n = names.len();
        if n > 0 && src != dst {
            let worlds = disk::canonical(&self.worlds_dir);
            let spawned = std::thread::Builder::new().name("specula-evidence".into()).spawn(move || {
                if crate::project::check_inside(&worlds, &dst).is_err() || disk::book().make_dir(Category::Worlds, &dst, Need::Agent).is_err() {
                    return;
                }
                for f in names {
                    let (a, b) = (src.join(&f), dst.join(&f));
                    if a.is_file() && !b.is_file() {
                        if let Err(e) = disk::book().copy(Category::Worlds, "evidence", &a, &b, Need::Agent) {
                            eprintln!("specula: review evidence {f} not carried to {}: {e}", dst.display());
                        }
                    }
                }
            });
            if let Err(e) = spawned {
                eprintln!("specula: review evidence not carried to {}: {e}", to.display());
            }
        }
        self.note_root();
        self.review_ui.set_roots(self.review_roots());
        n
    }

    /// Removes evidence this run's queue named in its folder and no longer names (submissions
    /// and critiques it dropped), and what refused calls copied that it doesn't name: at most once
    /// a minute after the queue changed (or while such files wait to be a minute old), only
    /// files older than a minute, never while the load reported a problem, the queue isn't saved,
    /// or an `.unreadable-*` queue sits in the folder. Also beats the unsaved folder's heartbeat.
    pub(crate) fn evidence_gc(&mut self) {
        self.note_root();
        let root = self.evidence_root();
        if self.world_name.is_none() && self.evidence.alive.is_none_or(|t| t.elapsed() >= ALIVE_EVERY) && root.join(EVIDENCE_DIR).is_dir() {
            self.evidence.alive = Some(Instant::now());
            let _ = disk::book().write(Category::Worlds, "evidence", &root.join(ALIVE_FILE), b"alive", Need::Essential);
        }
        let rev = self.swarm.review.revision();
        let pending = self.evidence.pending;
        if self.evidence.gc.is_some_and(|(r, at)| (r == rev && !pending) || at.elapsed() < GC_EVERY) {
            return;
        }
        self.evidence.gc = Some((rev, Instant::now()));
        // (Everything the queue names went through keep_evidence or a load's adoption, so it is
        // in `seen` already; this only makes sure.)
        let named: BTreeSet<String> = self.swarm.review.file_refs().into_iter().filter(|r| ref_file(r).is_some()).map(str::to_owned).collect();
        self.evidence.seen.extend(named.iter().cloned());
        let unreadable = std::fs::read_dir(&root).map(|it| it.flatten().any(|e| e.file_name().to_string_lossy().contains(".unreadable-"))).unwrap_or(false);
        if self.evidence.blocked || self.board_io.review_blocked || unreadable {
            self.evidence.pending = false;
            return;
        }
        let dir = std::path::absolute(&root).unwrap_or(root).join(EVIDENCE_DIR);
        let worlds = disk::canonical(&self.worlds_dir);
        let old = SystemTime::now().checked_sub(GC_MIN_AGE).unwrap_or(SystemTime::UNIX_EPOCH);
        self.evidence.pending = self.evidence.collect(&worlds, &dir, &named, old);
    }

    /// Marks review files that are gone (`missing: true` on each capture, `capture_missing` /
    /// `packet_missing` on critiques and summaries) in a review_list or critique_list answer.
    pub(crate) fn mark_missing(&self, j: &mut Json) {
        let roots = self.review_roots();
        let gone = |p: &str| crate::safe_path::review_file(&roots, p, "the file").is_err();
        fn walk(j: &mut Json, gone: &dyn Fn(&str) -> bool) {
            match j {
                Json::Arr(a) => a.iter_mut().for_each(|x| walk(x, gone)),
                Json::Obj(_) => {
                    let capture_like = j.get("kind").and_then(Json::as_str).is_some_and(|k| crate::review::CAPTURE_KINDS.contains(&k));
                    if capture_like && j.get("path").and_then(Json::as_str).is_some_and(gone) {
                        j.set("missing", Json::Bool(true));
                    }
                    for k in ["capture", "packet"] {
                        if j.get(k).and_then(Json::as_str).is_some_and(gone) {
                            j.set(format!("{k}_missing"), Json::Bool(true));
                        }
                    }
                    if let Json::Obj(pairs) = j {
                        for (_, v) in pairs.iter_mut() {
                            walk(v, gone);
                        }
                    }
                }
                _ => {}
            }
        }
        walk(j, &gone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_names_are_stable_and_safe() {
        let a = content_name(b"png bytes", "PNG", 0);
        assert_eq!(a, content_name(b"png bytes", "png", 0));
        assert_ne!(a, content_name(b"png bytez", "png", 0));
        assert!(a.ends_with("-9.png"));
        assert!(content_name(b"x", "../../x", 2).ends_with("-1-2.x"), "{}", content_name(b"x", "../../x", 2));
        assert!(content_name(b"x", "", 0).ends_with(".bin"));
        assert_eq!(ref_file("evidence/a.png"), Some("a.png"));
        assert_eq!(ref_file("evidence/../a.png"), None);
        assert_eq!(ref_file("C:/x/evidence/a.png"), None);
    }

    /// Round-3 review: 32 critiques naming the same two files read each once, copies are relative
    /// references, the per-call budget holds, and nothing but captures or evidence is copied.
    #[test]
    fn a_call_reads_each_file_once_and_names_relative_copies() {
        let base = std::env::temp_dir().join(format!("specula-evidence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (caps, worlds) = (base.join("captures"), base.join("worlds"));
        let run = caps.join("run-1-1");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::create_dir_all(worlds.join("w")).unwrap();
        let png = specula_core::png::encode_rgba8(2, 2, &[9u8; 16], specula_core::deflate::Level::Fastest);
        std::fs::write(run.join("a.png"), &png).unwrap();
        std::fs::write(run.join("b.json"), b"{}").unwrap();
        std::fs::write(worlds.join("w").join("world.json"), b"{}").unwrap();
        let roots = ReviewRoots { captures: caps.clone(), run: run.clone(), worlds: worlds.clone(), queue: worlds.join("w") };
        let mut call = CallCopies::default();
        let a = run.join("a.png").display().to_string();
        for _ in 0..32 {
            assert_eq!(copy_in(&roots, &a, true, &mut call).unwrap(), format!("evidence/{}", content_name(&png, "png", 0)));
            copy_in(&roots, "b.json", false, &mut call).unwrap();
        }
        assert_eq!(call.reads, 2, "each file read once in the call");
        assert!(worlds.join("w").join("evidence").join(content_name(&png, "png", 0)).is_file());
        // A second call finds the copy by its name and length, and reads the source once.
        let mut again = CallCopies::default();
        copy_in(&roots, &a, true, &mut again).unwrap();
        assert_eq!(again.reads, 1);
        // A world's own files are never copied; a capture past the call's budget is refused.
        let wj = worlds.join("w").join("world.json").display().to_string();
        assert_eq!(copy_in(&roots, &wj, false, &mut CallCopies::default()).unwrap_err().code, "path_not_allowed");
        let mut tight = CallCopies { budget: 10, ..CallCopies::default() };
        assert_eq!(copy_in(&roots, &a, true, &mut tight).unwrap_err().code, "evidence_budget");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A scratch layout: (base, roots, this run's capture folder).
    fn layout(label: &str) -> (PathBuf, ReviewRoots, PathBuf) {
        let base = std::env::temp_dir().join(format!("specula-evidence-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (caps, worlds) = (base.join("captures"), base.join("worlds"));
        let run = caps.join("run-1-1");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::create_dir_all(worlds.join("w")).unwrap();
        let roots = ReviewRoots { captures: caps, run: run.clone(), worlds: worlds.clone(), queue: worlds.join("w") };
        (base, roots, run)
    }

    fn set_age(p: &Path, secs: u64) {
        let t = std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(secs));
        std::fs::File::options().write(true).open(p).unwrap().set_times(t).unwrap();
    }

    /// Integration check (2026-10-29): a refused review_submit left its copies in the evidence
    /// for good (the cap's advice couldn't free them). A call refused after copying hands back
    /// what it copied, even when a later file is the refusal; collection removes those copies
    /// once they are a minute old unless the queue names them.
    #[test]
    fn refused_calls_hand_their_copies_back_to_collection() {
        let (base, roots, run) = layout("refused");
        let mk = |n: u8| {
            let png = specula_core::png::encode_rgba8(2, 2, &[n; 16], specula_core::deflate::Level::Fastest);
            let p = run.join(format!("c{n}.png"));
            std::fs::write(&p, &png).unwrap();
            p.display().to_string()
        };
        let (a, b, c) = (mk(1), mk(2), mk(3));
        // The third capture isn't there: the call is refused after two copies.
        let args = Json::obj([("title", Json::from("x")), ("captures", Json::arr([Json::from(a.as_str()), Json::from(b.as_str()), Json::from(run.join("gone.png").display().to_string())]))]);
        let mut call = CallCopies::default();
        let err = prepare_in(&roots, "review_submit", &args, &mut call).unwrap_err();
        assert!(err.is_error);
        assert_eq!(call.made().len(), 2, "both copies are handed back");
        let dir = std::path::absolute(&roots.queue).unwrap().join(EVIDENCE_DIR);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        // An accepted call names c (and a again: one content, one file, named).
        let mut ok = CallCopies::default();
        let named_args = Json::obj([("captures", Json::arr([Json::from(c.as_str()), Json::from(a.as_str())]))]);
        let named_args = prepare_in(&roots, "review_submit", &named_args, &mut ok).unwrap();
        let named: BTreeSet<String> = named_args.get("captures").and_then(Json::as_array).unwrap().iter().filter_map(Json::as_str).map(str::to_owned).collect();
        let mut st = EvidenceState { seen: call.made().iter().cloned().chain(named.iter().cloned()).collect(), ..EvidenceState::default() };
        let worlds = disk::canonical(&roots.worlds);
        // Too young: kept, and the pass says to come back.
        assert!(st.collect(&worlds, &dir, &named, SystemTime::now() - Duration::from_secs(60)));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 3);
        // A minute on: the refused call's copy that nothing names goes; a's (named again) and c's stay.
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            set_age(&e.path(), 120);
        }
        assert!(!st.collect(&worlds, &dir, &named, SystemTime::now() - Duration::from_secs(60)));
        let left: BTreeSet<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| format!("evidence/{}", e.file_name().to_string_lossy())).collect();
        assert_eq!(left, named, "only what the queue names is left");
        // Strays (a world switched in between) are kept for their folder.
        st.strays.insert(roots.queue.clone(), ["evidence/x.png".to_owned()].into());
        st.adopt_strays(&roots.queue);
        assert!(st.pending && st.seen.contains("evidence/x.png") && st.strays.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Integration check (2026-10-29): arguments are refused before anything is copied, and a
    /// copy found again is marked as just used, so collection can't take it from under the call
    /// that named it again.
    #[test]
    fn a_copy_found_again_is_marked_used() {
        let (base, roots, run) = layout("touch");
        let png = specula_core::png::encode_rgba8(2, 2, &[5u8; 16], specula_core::deflate::Level::Fastest);
        std::fs::write(run.join("a.png"), &png).unwrap();
        let r = copy_in(&roots, "a.png", true, &mut CallCopies::default()).unwrap();
        let copy = std::path::absolute(&roots.queue).unwrap().join(r.replace('/', std::path::MAIN_SEPARATOR_STR));
        set_age(&copy, 7200);
        let mut again = CallCopies::default();
        assert_eq!(copy_in(&roots, "a.png", true, &mut again).unwrap(), r);
        assert_eq!(again.made(), [r]);
        let age = SystemTime::now().duration_since(std::fs::metadata(&copy).unwrap().modified().unwrap()).unwrap_or_default();
        assert!(age < Duration::from_secs(60), "found again: marked as used ({age:?} old)");
        // The tool checks its arguments before copying (tools.rs runs parse first): a bad one is
        // refused by the parser without touching the disk.
        let bad = Json::obj([("title", Json::from("x")), ("kind", Json::from("asset")), ("paths", Json::arr([Json::from("/a")])), ("captures", Json::from("not an array"))]);
        let e = crate::tools::parse("review_submit", &bad).err().expect("refused");
        assert!(e.contains("captures"), "{e}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Integration check (2026-10-29): a playtest step's review copied up to 64 MiB on the engine
    /// thread. On the engine thread a call reads at most 4 MiB of evidence in all (copies, and
    /// sources whose copy is already there), refused before the read with a structured error.
    #[test]
    fn engine_thread_calls_read_at_most_four_mib() {
        let (base, roots, run) = layout("engine");
        std::fs::write(run.join("big.bin"), vec![1u8; 5 << 20]).unwrap();
        std::fs::write(run.join("three.bin"), vec![2u8; 3 << 20]).unwrap();
        std::fs::write(run.join("two.bin"), vec![3u8; 2 << 20]).unwrap();
        let mut call = CallCopies::engine_thread();
        let e = copy_in(&roots, "big.bin", false, &mut call).unwrap_err();
        assert_eq!((e.code, call.reads), ("evidence_budget", 0), "refused before reading");
        assert_eq!(e.to_json().get("limit_mb").and_then(Json::as_u64), Some(4));
        assert_eq!(e.to_json().get("engine_thread"), Some(&Json::Bool(true)));
        copy_in(&roots, "three.bin", false, &mut call).unwrap();
        assert_eq!(copy_in(&roots, "two.bin", false, &mut call).unwrap_err().code, "evidence_budget");
        // A copy already there still costs its read.
        let mut again = CallCopies::engine_thread();
        copy_in(&roots, "three.bin", false, &mut again).unwrap();
        assert_eq!(copy_in(&roots, "two.bin", false, &mut again).unwrap_err().code, "evidence_budget");
        // Off the engine thread the same files fit (64 MiB, copies only).
        let mut off = CallCopies::default();
        copy_in(&roots, "three.bin", false, &mut off).unwrap();
        copy_in(&roots, "two.bin", false, &mut off).unwrap();
        copy_in(&roots, "big.bin", false, &mut off).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }
}
