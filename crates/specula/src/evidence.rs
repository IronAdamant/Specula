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
//!   (`evidence_budget`). The engine then only checks that the copies are there.
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
//! - **Collection never guesses.** Only files this run's queue named and then dropped are
//!   removed (at most once a minute, only files older than a minute); nothing is removed while the
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
fn content_name(bytes: &[u8], ext: &str, n: u32) -> String {
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
}

impl Default for CallCopies {
    fn default() -> Self {
        CallCopies { done: BTreeMap::new(), budget: MAX_CALL_BYTES, reads: 0 }
    }
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
        return Err(refuse("evidence_budget", format!("one call copies at most {} MB of evidence; '{given}' would pass it: submit in smaller parts", MAX_CALL_BYTES >> 20), given).with("limit_mb", MAX_CALL_BYTES >> 20));
    }
    let bytes = std::fs::read(&source).map_err(|_| refuse("capture_missing", format!("'{given}' can't be read"), given))?;
    call.reads += 1;
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
                    let r = format!("{REF_PREFIX}{name}");
                    call.done.insert(given.to_owned(), r.clone());
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
                call.budget -= len;
                let r = format!("{REF_PREFIX}{name}");
                call.done.insert(given.to_owned(), r.clone());
                return Ok(r);
            }
        }
    }
    Err(refuse("evidence_full", format!("{MAX_SAME_NAME} different files share '{given}''s content name"), given))
}

/// `review_submit` and `critique_add`, on the calling agent's connection thread: copies the files
/// they name into the queue's evidence and answers the arguments naming the copies, so the
/// engine thread never copies. Errors are the tool's answer.
pub fn prepare(tools: &crate::tools::Tools, session: &SessionInfo, name: &str, args: &Json) -> Result<Json, ToolOutput> {
    let fail = |e: WorldError| ToolOutput::error(e.to_json().to_string());
    let out = tools.send(session, crate::tools::Command::ReviewRoots);
    let text = out.content.iter().find_map(|c| if let specula_protocol::Content::Text(t) = c { Some(t.clone()) } else { None }).unwrap_or_default();
    let j = Json::parse(&text).map_err(|_| out.clone())?;
    let path = |k: &str| PathBuf::from(j.get(k).and_then(Json::as_str).unwrap_or_default());
    let roots = ReviewRoots { captures: path("captures"), run: path("run"), worlds: path("worlds"), queue: path("queue") };
    let mut call = CallCopies::default();
    let mut args = args.clone();
    let one = |v: &mut Json, png: bool, field: String, call: &mut CallCopies| -> Result<(), ToolOutput> {
        if let Some(given) = v.as_str().map(str::to_owned) {
            let r = copy_in(&roots, &given, png, call).map_err(|e| fail(WorldError { message: format!("{field}: {}", e.message), ..e }.with("field", field.as_str())))?;
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
                        one(&mut c, true, format!("captures[{i}]"), &mut call)?;
                    } else {
                        if let Some(mut p) = c.get("path").cloned() {
                            one(&mut p, true, format!("captures[{i}]"), &mut call)?;
                            c.set("path", p);
                        }
                        if let Some(mut p) = c.get("packet").cloned() {
                            one(&mut p, false, format!("captures[{i}].packet"), &mut call)?;
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
                            one(&mut p, false, format!("items[{i}].{k}"), &mut call)?;
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
    /// The unsaved folder's last heartbeat.
    alive: Option<Instant>,
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
        Ok(r)
    }

    /// Starts the record over when the queue's folder changed.
    fn note_root(&mut self) {
        let root = self.evidence_root();
        if self.evidence.root.as_ref() != Some(&root) {
            self.evidence.root = Some(root);
            self.evidence.seen.clear();
            self.evidence.blocked = false;
            self.evidence.gc = None;
        }
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
    /// and critiques it dropped): at most once a minute after the queue changed, only files older
    /// than a minute, never while the load reported a problem, the queue isn't saved, or an
    /// `.unreadable-*` queue sits in the folder. Also beats the unsaved folder's heartbeat.
    pub(crate) fn evidence_gc(&mut self) {
        self.note_root();
        let root = self.evidence_root();
        if self.world_name.is_none() && self.evidence.alive.is_none_or(|t| t.elapsed() >= ALIVE_EVERY) && root.join(EVIDENCE_DIR).is_dir() {
            self.evidence.alive = Some(Instant::now());
            let _ = disk::book().write(Category::Worlds, "evidence", &root.join(ALIVE_FILE), b"alive", Need::Essential);
        }
        let named: BTreeSet<String> = self.swarm.review.file_refs().into_iter().filter(|r| ref_file(r).is_some()).map(str::to_owned).collect();
        self.evidence.seen.extend(named.iter().cloned());
        let rev = self.swarm.review.revision();
        if self.evidence.gc.is_some_and(|(r, at)| r == rev || at.elapsed() < GC_EVERY) {
            return;
        }
        self.evidence.gc = Some((rev, Instant::now()));
        let unreadable = std::fs::read_dir(&root).map(|it| it.flatten().any(|e| e.file_name().to_string_lossy().contains(".unreadable-"))).unwrap_or(false);
        if self.evidence.blocked || self.board_io.review_blocked || unreadable {
            return;
        }
        let dir = std::path::absolute(&root).unwrap_or(root).join(EVIDENCE_DIR);
        let worlds = disk::canonical(&self.worlds_dir);
        let old = SystemTime::now().checked_sub(GC_MIN_AGE).unwrap_or(SystemTime::UNIX_EPOCH);
        let dropped: Vec<String> = self.evidence.seen.iter().filter(|r| !named.contains(*r)).cloned().collect();
        for r in dropped {
            let Some(f) = ref_file(&r) else { continue };
            let p = disk::canonical(&dir.join(f));
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                self.evidence.seen.remove(&r);
                continue;
            };
            if !m.is_file() || m.modified().is_ok_and(|t| t > old) {
                continue;
            }
            if let Ok(b) = disk::remove_file_inside(&worlds, &p) {
                disk::book().wrote(Category::Worlds, "evidence", -(b as i64), -1);
                add_used(&dir, -(b as i64));
                self.evidence.seen.remove(&r);
            }
        }
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
}
