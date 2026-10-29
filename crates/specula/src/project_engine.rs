//! The engine side of the project tools (see `project_tools` for their arguments and
//! docs/design/PROJECT_DATA.md): snapshots, commits through the swarm's claims and journal, the
//! disk kept equal to the store, imports of edits made outside the engine, and hot reload.

use crate::journal::Writer;
use crate::project::{self, Event, FileKind, Mount, MountKind, ProjectFile, DISK_AGENT};
use crate::project_tools::{Committed, FixesCommitted, ModuleText, ProjectCall, Snap};
use crate::world::{Change, WorldError};
use crate::Engine;
use specula_core::Json;
use specula_protocol::ToolOutput;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Journal entries examined for an earlier version.
const SCAN: usize = 50_000;
/// Failed disk writes are retried this often.
const RETRY_EVERY: Duration = Duration::from_secs(1);
/// project_list stops adding files past this many bytes.
const LIST_BYTES: usize = 64 * 1024;

/// A project call that reads the disk, held while a sync writes on its worker: run in order when
/// it ends, so the engine's view of the disk (stamps, pending writes, held files) only ever moves
/// between whole syncs, as when syncs ran on this thread.
pub(crate) struct Deferred {
    call: ProjectCall,
    name: String,
    key: Option<String>,
    by: String,
}

/// After the next sync: a call's file (`Some(path)`) disk state and what was reloaded.
pub(crate) type SyncWaiter = (Option<String>, SyncThen);

/// What runs when the sync it waits for ends: with its file's disk state and what was reloaded.
pub(crate) type SyncThen = Box<dyn FnOnce(&mut Engine, Json, Vec<&'static str>)>;

/// A commit's answer, now (refused, changed) or once the disk has its file.
enum CommitStep {
    Now(Committed),
    /// Committed as journal entry `seq`: the answer is made when the sync ends (`backup`: where a
    /// replaced unreadable file's bytes went).
    Synced { seq: Option<u64>, backup: Option<String> },
}

/// Whether a project call reads or writes the disk (so waits for a sync in progress).
fn waits_for_sync(call: &ProjectCall) -> bool {
    matches!(call, ProjectCall::Snapshot { .. } | ProjectCall::Commit { .. } | ProjectCall::ModuleTexts { .. } | ProjectCall::CommitFixes { .. } | ProjectCall::Mount { .. })
}

/// The text `path` had at `version`, and the changes after it (newest first), from the journal
/// (or the world it started from).
fn at_version(journal: &crate::journal::Journal, path: &str, version: u64) -> (Option<Arc<str>>, Vec<Json>) {
    let id = project::file_id(path);
    let mut later = Vec::new();
    for e in journal.touching(id, SCAN) {
        for c in e.changes.iter().rev().filter(|c| c.id() == id) {
            let Change::File { before, after, kept, .. } = c else { continue };
            // A change whose text was dropped (the journal's file-text budget) has no text to give.
            if let Some(a) = after.as_deref().filter(|a| a.version == version) {
                return (kept.then(|| Arc::clone(&a.text)), later);
            }
            if after.as_deref().is_none_or(|a| a.version > version) && later.len() < 8 {
                let mut f = vec![("journal", Json::from(e.seq)), ("by", Json::from(&*e.by)), ("tick", Json::from(e.tick)), ("op", Json::from(c.op()))];
                if let Some(a) = after.as_deref() {
                    f.push(("version", Json::from(a.version)));
                }
                if let Some(v) = &e.via {
                    f.push(("via", Json::from(&**v)));
                }
                later.push(Json::obj(f));
            }
            if let Some(b) = before.as_deref().filter(|b| b.version == version) {
                return (kept.then(|| Arc::clone(&b.text)), later);
            }
            if before.as_deref().is_some_and(|b| b.version < version) {
                return (None, later);
            }
        }
    }
    let base = journal.base().files().get(path).filter(|f| f.version == version).map(|f| Arc::clone(&f.text));
    (base, later)
}

/// A project commit's writes: the file, then (`also`) the entity change that goes with it, in
/// one journal entry: both are checked against claims, and one undo takes both back.
pub(crate) fn file_commit(wd: &mut dyn crate::write_policy::WorldWrite, path: &str, file: Option<ProjectFile>, also: Option<crate::project_tools::Also>, by: &str) -> Result<(), WorldError> {
    wd.put_file(path, file, false);
    if let Some(a) = also {
        let (key, change) = *a;
        wd.update(&key, change, by)?;
    }
    Ok(())
}

impl Engine {
    /// Answers a project call on the main thread.
    pub(crate) fn project_call(&mut self, call: ProjectCall, w: Writer, by: &str) -> ToolOutput {
        if self.project_syncing && waits_for_sync(&call) {
            if matches!(call, ProjectCall::Mount { .. }) {
                // A mount answers in the call itself (rare, and it scans a folder anyway).
                self.project_wait_sync();
            } else {
                self.project_deferred.push_back(Deferred { call, name: w.name.to_owned(), key: w.session.map(str::to_owned), by: by.to_owned() });
                return ToolOutput::text("{}");
            }
        }
        match call {
            ProjectCall::List { prefix, kind, after, limit } => ToolOutput::text(self.project_list(prefix.as_deref(), kind, after.as_deref(), limit).to_string()),
            ProjectCall::Snapshot { path, version, context, reply } => {
                let snap = self.project_snapshot(&path, version, context);
                let _ = reply.send(snap);
                ToolOutput::text("{}")
            }
            ProjectCall::Commit { path, base, file, replace_unreadable, also, reply } => {
                match self.project_commit(&path, base, file, replace_unreadable, also, w, by, crate::write_policy::Rule::Project) {
                    CommitStep::Now(done) => {
                        let _ = reply.send(done);
                    }
                    // Answered once the file is on disk (written on a worker).
                    CommitStep::Synced { seq, backup } => self.project_sync_then(Some(&path.clone()), move |e, disk, reloaded| {
                        let version = e.world.files().get(&path).map(|f| f.version);
                        let mut j = vec![("path", Json::from(path.as_str())), ("version", Json::from(version)), ("journal", Json::from(seq))];
                        if let Some(f) = e.world.files().get(&path) {
                            j.push(("bytes", Json::from(if f.kind.is_binary() { project::binary_bytes(f) } else { f.text.len() as u64 })));
                        } else {
                            j.push(("deleted", Json::Bool(true)));
                        }
                        j.push(("disk", disk));
                        if let Some(b) = backup {
                            j.push(("backup", Json::from(b)));
                        }
                        j.push(("reloaded", Json::arr(reloaded.into_iter().map(Json::from))));
                        let _ = reply.send(Committed::Done(Json::obj(j)));
                    }),
                }
                ToolOutput::text("{}")
            }
            ProjectCall::Mount { dir, name } => {
                // Its files are imported under /project/<name> as `disk` (claims exempt: a human's
                // edit can't be refused), so the agent asking is checked there first: a (re)mount
                // can't overwrite files another agent claimed.
                if let Err(e) = self.swarm.claims.check_subtree(&project::journal_path(&name), w, Instant::now(), "project_mount") {
                    return crate::world_error(&e);
                }
                self.project_mount(dir, name)
            }
            ProjectCall::FixGate { dir } => self.project_fix_gate(&dir, w),
            ProjectCall::ModuleTexts { dir, reply } => {
                let texts = self.project_module_texts(&dir);
                let _ = reply.send(texts);
                ToolOutput::text("{}")
            }
            ProjectCall::Owner { reply } => {
                // The identity key alone: a name given (or changed) by agent_hello later keeps them.
                let _ = reply.send(w.session.unwrap_or(w.name).to_owned());
                ToolOutput::text("{}")
            }
            ProjectCall::CommitFixes { files, reply } => {
                let (mut done, written) = self.project_commit_fixes(files, w, by);
                match written {
                    None => {
                        let _ = reply.send(done);
                    }
                    // A fix that didn't reach the disk stops the rounds: the next build would
                    // compile the old text.
                    Some(paths) => self.project_sync_then(None, move |e, _, _| {
                        done.stop = paths.iter().any(|p| e.project.pending.contains_key(p) || e.project.held.contains_key(p));
                        done.committed = paths;
                        let _ = reply.send(done);
                    }),
                }
                ToolOutput::text("{}")
            }
        }
    }

    fn project_snapshot(&mut self, path: &str, version: Option<u64>, context: bool) -> Snap {
        if let Err(e) = project::check_path(path) {
            return Snap { error: Some(e), ..Snap::default() };
        }
        // Another spelling of a file the store has is that file on Windows: never a second entry
        // (looking it up on disk would find, and import, the other file).
        if let Some(e) = self.project_case_clash(path) {
            return Snap { error: Some(e), ..Snap::default() };
        }
        // Fresh from disk: an edit made outside shows before anyone builds on the old text.
        if let Some(ev) = self.project.refresh(path) {
            self.project_import(vec![ev]);
        }
        let located = self.project.locate(path);
        let kind = located.as_ref().and_then(|(m, rel)| project::kind_for(m.kind, rel));
        let current = self.world.files().get(path).cloned();
        let module_dir = located.filter(|(m, _)| m.kind == MountKind::Module).map(|(m, _)| m.root);
        let (at, later) = match (version, &current) {
            (Some(v), Some(c)) if v == c.version => (Some(Arc::clone(&c.text)), Vec::new()),
            (Some(v), _) => at_version(self.swarm.journal(), path, v),
            (None, _) => (None, Vec::new()),
        };
        let kind = current.as_ref().map(|f| f.kind).or(kind);
        let ctx = match (context, kind) {
            (true, Some(k)) => {
                let over = self.project_style_override();
                crate::project_kinds::context(path, k, self.world.files(), over.as_ref())
            }
            _ => Vec::new(),
        };
        let held = self.project.held.contains_key(path);
        Snap { kind, module_dir, current, at_version: at, later, context: ctx, error: None, held }
    }

    /// One file's commit under `rule`: the project tools' (`Rule::Project`), or the taste tools'
    /// for taste state (`Rule::Taste`, through [`Engine::project_put`]).
    #[allow(clippy::too_many_arguments)]
    fn project_commit(&mut self, path: &str, base: (u64, u64), file: Option<(FileKind, Arc<str>)>, replace_unreadable: bool, also: Option<crate::project_tools::Also>, w: Writer, by: &str, rule: crate::write_policy::Rule) -> CommitStep {
        match self.project_commit_now(path, base, file, replace_unreadable, also, w, by, rule) {
            Ok((seq, backup)) => CommitStep::Synced { seq, backup },
            Err(c) => CommitStep::Now(c),
        }
    }

    /// The commit itself: `Ok((journal entry, backup))`, or why not.
    #[allow(clippy::too_many_arguments)]
    fn project_commit_now(&mut self, path: &str, base: (u64, u64), file: Option<(FileKind, Arc<str>)>, replace_unreadable: bool, also: Option<crate::project_tools::Also>, w: Writer, by: &str, rule: crate::write_policy::Rule) -> Result<(Option<u64>, Option<String>), Committed> {
        if let Some(e) = self.project_case_clash(path) {
            return Err(Committed::Refused(e));
        }
        if let Some(ev) = self.project.refresh(path) {
            self.project_import(vec![ev]);
        }
        let cur = self.world.files().get(path);
        if (cur.map_or(0, |f| f.version), cur.map_or(0, |f| f.hash)) != base {
            return Err(Committed::Changed);
        }
        let Some((mount, rel)) = self.project.locate(path) else {
            return Err(Committed::Refused(WorldError::new("not_mirrored", format!("{path} isn't in a mount (project_list shows them)")).with("path", path)));
        };
        let Some(kind) = project::kind_for(mount.kind, &rel) else {
            return Err(Committed::Refused(WorldError::new("not_mirrored", format!("{path} isn't a kind of file the project mirrors")).with("path", path)));
        };
        // Never into a linked folder: the write could only fail, after the commit.
        if let Some(dir) = mount.disk_path(&rel).parent() {
            if let Err(e) = project::check_inside(&mount.root, dir) {
                return Err(Committed::Refused(WorldError::new("linked_folder", format!("{path}: {e}; nothing was changed (write somewhere that isn't behind a link)")).with("path", path)));
            }
        }
        // The disk holds something that can't be mirrored: never overwritten unless asked to.
        let mut backup = None;
        if let Some((_, _, why)) = self.project.held.get(path).cloned() {
            if !replace_unreadable {
                return Err(Committed::Refused(
                    WorldError::new("disk_unreadable", format!("{path} was changed on disk into something that can't be mirrored ({why}); nothing was changed. Fix it on disk (it is imported then), or pass replace_unreadable: true to replace it (its bytes are backed up first)"))
                        .with("path", path)
                        .with("why", why.as_str()),
                ));
            }
            match self.project.back_up_and_release(path) {
                Ok(b) => backup = Some(b.display().to_string()),
                Err(e) => return Err(Committed::Refused(WorldError::new("backup_failed", format!("{path} couldn't be backed up before replacing it: {e}; nothing was changed")).with("path", path))),
            }
        }
        let new = match file {
            Some((_, text)) => {
                if let Err(e) = project::room_for(self.world.files(), path, text.len()) {
                    return Err(Committed::Refused(e));
                }
                // Binary files: their bytes must be in the content store, within the store's bound.
                let disk_bytes = if kind.is_binary() {
                    let Some((id, bytes)) = crate::blobs::parse_pointer(&text) else {
                        return Err(Committed::Refused(WorldError::new("not_a_pointer", format!("{path} is a binary file: write it with project_upload")).with("path", path)));
                    };
                    if !self.content.as_ref().is_some_and(|c| c.has(id)) {
                        return Err(Committed::Refused(WorldError::new("not_kept", format!("{path}: its content {} isn't in the content store; upload it again", id.hex())).with("path", path)));
                    }
                    if let Err(e) = project::room_for_binary(self.world.files(), path, bytes) {
                        return Err(Committed::Refused(e));
                    }
                    bytes
                } else {
                    text.len() as u64
                };
                // Through the disk budget: refused below the free-space floor before anything changes.
                if let Err(e) = crate::disk::book().admit(crate::disk::Category::Project, disk_bytes, crate::disk::Need::Agent) {
                    return Err(Committed::Refused(e.to_world()));
                }
                if cur.is_none() {
                    // One file on Windows (which folds case in Unicode): refuse a second spelling
                    // everywhere, so projects stay portable.
                    let folded = project::fold_case(path);
                    if let Some(other) = self.world.files().keys().find(|p| project::fold_case(p) == folded) {
                        return Err(Committed::Refused(WorldError::new("case_clash", format!("{other} exists; {path} differs only in case, which is the same file on Windows")).with("path", path).with("existing", other.as_str())));
                    }
                }
                // Versions never repeat at a path, even across a delete (no ABA for expect_version).
                Some(ProjectFile::new(kind, text, self.world.file_version_floor(path).max(base.0) + 1, Arc::from(by)))
            }
            None if cur.is_none() => return Err(Committed::Refused(WorldError::new("not_found", format!("there is no project file {path}")).with("path", path))),
            None => None,
        };
        let tick = self.clock.tick();
        match self.world.commit(&mut self.swarm, tick, w, crate::journal::Kind::File, rule, |wd| file_commit(wd, path, new, also, by)) {
            Err(e) => Err(Committed::Refused(e)),
            Ok((_, seq)) => Ok((seq, backup)),
        }
    }

    /// Writes project file `path` as `w` from its current text (`None`: it doesn't exist yet),
    /// validated by kind like an agent's write, then committed through the write policy
    /// (`Rule::Project`: claim-checked, journaled, undoable) and synced to disk. The taste tools'
    /// files (galleries, picks, boards, onboarding) and what their approvals edit (style.toml, a
    /// theme) go through here (docs/design/GALLERIES.md).
    pub(crate) fn project_put(&mut self, path: &str, edit: impl FnOnce(Option<&str>) -> Result<String, WorldError>, w: Writer, by: &str, rule: crate::write_policy::Rule) -> Result<Json, WorldError> {
        let snap = self.project_snapshot(path, None, true);
        if let Some(e) = snap.error {
            return Err(e);
        }
        let Some(kind) = snap.kind else {
            return Err(WorldError::new("not_mirrored", format!("{path} isn't a kind of file the project mirrors")).with("path", path));
        };
        let old = snap.current.as_ref().map(|f| Arc::clone(&f.text));
        let text = edit(old.as_deref())?;
        if old.as_deref() == Some(text.as_str()) {
            return Ok(Json::obj([("path", Json::from(path)), ("version", Json::from(snap.current.as_ref().map(|f| f.version))), ("unchanged", Json::Bool(true))]));
        }
        let problems = crate::project_kinds::validate(path, kind, &text, old.as_deref(), &snap.context);
        if let Some(p) = problems.iter().find(|p| p.is_error()) {
            return Err(WorldError::new("invalid_file", format!("{path}: {} ({}); nothing was changed", p.message, p.code)).with("path", path).with("problem", p.to_json()));
        }
        let base = snap.current.as_ref().map_or((0, 0), |f| (f.version, f.hash));
        // The caller names its rule: the taste savers (galleries, suggestions, boards,
        // onboarding) pass `Rule::Taste`, writers of results `Rule::Project`. Never derived from
        // the path (taste review 2: gallery_apply into a taste file got the taste rule).
        let step = self.project_commit(path, base, Some((kind, Arc::from(text))), false, None, w, by, rule);
        self.project_step_now(path, step, "written")
    }

    /// Deletes project file `path` as `w` under the caller's `rule` (claims, journal and undo as
    /// any write; `gallery_delete` deletes taste state so).
    pub(crate) fn project_remove(&mut self, path: &str, w: Writer, by: &str, rule: crate::write_policy::Rule) -> Result<Json, WorldError> {
        let snap = self.project_snapshot(path, None, true);
        if let Some(e) = snap.error {
            return Err(e);
        }
        let Some(current) = snap.current.as_ref() else {
            return Err(WorldError::new("not_found", format!("{path} doesn't exist")).with("path", path));
        };
        let step = self.project_commit(path, (current.version, current.hash), None, false, None, w, by, rule);
        self.project_step_now(path, step, "deleted")
    }

    /// A commit's answer for engine-thread callers (the taste tools): at once, with the journal
    /// entry and version; the disk follows on the project lane (`disk: syncing`), as for any
    /// project write, so the engine thread never waits for a file.
    fn project_step_now(&mut self, path: &str, step: CommitStep, what: &str) -> Result<Json, WorldError> {
        match step {
            CommitStep::Now(Committed::Done(j)) => Ok(j),
            CommitStep::Now(Committed::Refused(e)) => Err(e),
            CommitStep::Now(Committed::Changed) => Err(WorldError::new("busy", format!("{path} changed while it was being {what}; nothing was changed (try again)")).with("path", path)),
            CommitStep::Synced { seq, .. } => {
                self.project_sync_then(Some(path), |_, _, _| {});
                let version = self.world.files().get(path).map(|f| f.version);
                Ok(Json::obj([("path", Json::from(path)), ("version", Json::from(version)), ("journal", Json::from(seq)), ("disk", Json::from("syncing"))]))
            }
        }
    }

    /// The mounts, the disk's state and one page of files.
    fn project_list(&self, prefix: Option<&str>, kind: Option<FileKind>, after: Option<&str>, limit: usize) -> Json {
        let files = self.world.files();
        let mounts: Vec<Json> = self
            .project
            .mounts()
            .iter()
            .map(|m| {
                let n = files.range(format!("{}/", m.name)..).take_while(|(p, _)| p.starts_with(&format!("{}/", m.name))).count();
                let mut f = vec![("name", Json::from(m.name.as_str())), ("folder", Json::from(m.root.display().to_string())), ("files", Json::from(n))];
                if let Some((skipped, truncated)) = self.project.skipped.get(&m.name) {
                    if !skipped.is_empty() {
                        f.push(("skipped", Json::arr(skipped.iter().take(10).map(|(p, why)| Json::obj([("path", Json::from(p.as_str())), ("why", Json::from(why.as_str()))])))));
                    }
                    if *truncated {
                        f.push(("scan_truncated", Json::Bool(true)));
                    }
                }
                Json::obj(f)
            })
            .collect();
        let mut out = vec![("mounts", Json::Arr(mounts))];
        if !self.project.pending.is_empty() {
            out.push(("disk_pending", Json::arr(self.project.pending.iter().take(20).map(|(p, e)| Json::obj([("path", Json::from(p.as_str())), ("error", Json::from(e.as_str()))])))));
        }
        if !self.project.held.is_empty() {
            out.push(("disk_unreadable", Json::arr(self.project.held.iter().take(20).map(|(p, (_, _, why))| Json::obj([("path", Json::from(p.as_str())), ("why", Json::from(why.as_str()))])))));
        }
        if !self.project.conflicts.is_empty() {
            out.push(("disk_conflicts", Json::arr(self.project.conflicts.iter().rev().take(10).map(|c| Json::obj([("path", Json::from(c.path.as_str())), ("message", Json::from(c.message.as_str()))])))));
        }
        let mut page = Vec::new();
        let mut bytes = 0;
        let mut next = None;
        let start = after.map_or(std::ops::Bound::Unbounded, |a| std::ops::Bound::Excluded(a.to_owned()));
        let matching = files.range::<String, _>((start, std::ops::Bound::Unbounded)).filter(|(p, f)| prefix.is_none_or(|x| p.starts_with(x)) && kind.is_none_or(|k| f.kind == k));
        let total = files.iter().filter(|(p, f)| prefix.is_none_or(|x| p.starts_with(x)) && kind.is_none_or(|k| f.kind == k)).count();
        for (p, f) in matching {
            if page.len() == limit || bytes > LIST_BYTES {
                next = page.last().and_then(|j: &Json| j.get("path")).and_then(Json::as_str).map(str::to_owned);
                break;
            }
            let size = if f.kind.is_binary() { project::binary_bytes(f) } else { f.text.len() as u64 };
            let mut item = vec![("path", Json::from(p.as_str())), ("kind", Json::from(f.kind.name())), ("version", Json::from(f.version)), ("bytes", Json::from(size)), ("by", Json::from(&*f.modified_by))];
            if self.project.pending.contains_key(p) {
                item.push(("disk", Json::from("pending")));
            }
            let j = Json::obj(item);
            bytes += j.to_string().len();
            page.push(j);
        }
        out.push(("total", Json::from(total)));
        out.push(("files", Json::Arr(page)));
        if let Some(n) = next {
            out.push(("next_cursor", Json::from(n)));
        }
        Json::obj(out)
    }

    fn project_mount(&mut self, dir: PathBuf, name: String) -> ToolOutput {
        // Only real modules: a cdylib depending on specula-sdk alone, never the engine's own crates.
        let refuse = |code: &str, message: String| ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message))]).to_string());
        let cwd = std::env::current_dir().map(|d| specula_logic::fix::canonical(&d)).unwrap_or_default();
        if specula_logic::fix::canonical(&dir) == cwd {
            return refuse("not_a_module", "the engine's working directory is the project itself, not a module crate".into());
        }
        if dir.join("build.rs").exists() {
            return refuse("build_script", format!("{} has a build.rs: modules may not run code at build time", dir.display()));
        }
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        let problems = crate::project_kinds::check_module_manifest_at(&dir, "Cargo.toml", &manifest);
        if !problems.is_empty() {
            return ToolOutput::error(
                Json::obj([
                    ("error", Json::from("not_a_module")),
                    ("message", Json::from(format!("{} isn't a module crate (a cdylib depending only on specula-sdk); nothing was mounted", dir.display()))),
                    ("problems", Json::arr(problems.iter().take(20).map(crate::project_kinds::Problem::to_json))),
                ])
                .to_string(),
            );
        }
        // One disk file, one project path: a mount may not lie inside another, or hold one.
        let canon = specula_logic::fix::canonical(&dir);
        if let Some(m) = self.project.mounts().iter().find(|m| m.name != name && {
            let r = specula_logic::fix::canonical(&m.root);
            canon.starts_with(&r) || r.starts_with(&canon)
        }) {
            return refuse("mount_overlap", format!("{} overlaps the mount {} ({}): one file would have two project paths (and two sets of claims)", dir.display(), m.name, m.root.display()));
        }
        let mounted = self.project.mounts().iter().any(|m| m.name == name);
        if !mounted && self.project.module_mounts() >= project::MAX_MODULE_MOUNTS {
            return ToolOutput::error(Json::obj([("error", Json::from("mount_limit")), ("message", Json::from(format!("at most {} module crates are mounted at once", project::MAX_MODULE_MOUNTS)))]).to_string());
        }
        if let Some(other) = self.project.mounts().iter().find(|m| m.name == name && m.root != dir) {
            return ToolOutput::error(Json::obj([("error", Json::from("mount_taken")), ("message", Json::from(format!("{name} is already mounted from {}", other.root.display())))]).to_string());
        }
        let n = self.project_mount_folder(Mount { name: name.clone(), root: dir.clone(), kind: MountKind::Module });
        let files = self.world.files().range(format!("{name}/")..).take_while(|(p, _)| p.starts_with(&format!("{name}/"))).count();
        let mut j = Json::obj([("mount", Json::from(name.as_str())), ("files", Json::from(files)), ("imported", Json::from(n))]);
        // A module inside the engine's own Cargo workspace: its edits reach every workspace build.
        if specula_logic::build::workspace_root(&dir) == cwd {
            j.set("warning", Json::from("workspace_member: this module belongs to the engine's own Cargo workspace, so an edit that doesn't compile breaks the whole workspace's cargo build and cargo test, not just this module (manifests and paths are still checked on every commit)"));
        }
        ToolOutput::text(j.to_string())
    }

    /// Mounts `mount` and imports its files (one journal entry by `disk`); returns how many changed.
    fn project_mount_folder(&mut self, mount: Mount) -> usize {
        let found = self.project.mount(mount);
        let events = found.into_iter().map(|(path, kind, text, stamp)| Event::Changed { path, kind, text, stamp }).collect();
        self.project_import(events).len()
    }

    /// `case_clash` when the store has `path` under another spelling (Unicode case folding).
    fn project_case_clash(&self, path: &str) -> Option<WorldError> {
        if self.world.files().contains_key(path) {
            return None;
        }
        let folded = project::fold_case(path);
        let other = self.world.files().keys().find(|p| project::fold_case(p) == folded)?;
        Some(WorldError::new("case_clash", format!("{other} exists; {path} differs only in case, which is the same file on Windows: use {other}")).with("path", path).with("existing", other.as_str()))
    }

    /// The engine's `--style FILE` text, which governs every recipe when given.
    fn project_style_override(&self) -> Option<Arc<str>> {
        let path = self.library.lock().style_override()?.to_path_buf();
        project::read_text(&path).ok().map(|(t, _)| t)
    }

    /// The mount of a module folder, if it is mounted.
    fn project_module_mount(&self, dir: &std::path::Path) -> Option<Mount> {
        let dir = specula_logic::fix::canonical(dir);
        self.project.mounts().into_iter().find(|m| m.kind == MountKind::Module && specula_logic::fix::canonical(&m.root) == dir)
    }

    /// `logic_build {fix: true}` on a mounted module: refused while another agent holds a claim
    /// on any of its files (the fixes would write them).
    fn project_fix_gate(&mut self, dir: &std::path::Path, w: Writer) -> ToolOutput {
        let Some(m) = self.project_module_mount(dir) else { return ToolOutput::text("{}") };
        match self.swarm.claims.check_subtree(&project::journal_path(&m.name), w, std::time::Instant::now(), "apply fixes to") {
            Ok(()) => ToolOutput::text(Json::obj([("mount", Json::from(m.name.as_str()))]).to_string()),
            Err(e) => ToolOutput::error(e.to_json().to_string()),
        }
    }

    /// A mounted module's files as the store holds them (outside edits imported first): what a
    /// fix build compiles, so its fixes are worked out against exactly these texts.
    ///
    /// Refused (`disk_out_of_sync`, naming the files) while any of them is held, has a write
    /// pending, or isn't on disk as the store has it: Cargo compiles the disk, and fixes worked
    /// out against other text would cut into it.
    fn project_module_texts(&mut self, dir: &std::path::Path) -> Result<Vec<ModuleText>, Json> {
        let Some(m) = self.project_module_mount(dir) else { return Ok(Vec::new()) };
        let prefix = format!("{}/", m.name);
        let paths: Vec<String> = self.world.files().range(prefix.clone()..).take_while(|(p, _)| p.starts_with(&prefix)).map(|(p, _)| p.clone()).collect();
        let events: Vec<Event> = paths.iter().filter_map(|p| self.project.refresh(p)).collect();
        if !events.is_empty() {
            self.project_import(events);
        }
        let out_of_sync: Vec<Json> = self
            .world
            .files()
            .range(prefix.clone()..)
            .take_while(|(p, _)| p.starts_with(&prefix))
            .filter_map(|(p, f)| {
                let why = if let Some((_, _, why)) = self.project.held.get(p) {
                    format!("held: the disk holds something that can't be mirrored ({why})")
                } else if let Some(e) = self.project.pending.get(p) {
                    format!("its last write hasn't reached the disk ({e})")
                } else if self.project.stamp_of(p).map(|s| s.hash) != Some(f.hash) {
                    "the disk doesn't hold the store's text".to_owned()
                } else {
                    return None;
                };
                Some(Json::obj([("path", Json::from(p.as_str())), ("why", Json::from(why))]))
            })
            .collect();
        if !out_of_sync.is_empty() {
            return Err(Json::obj([
                ("error", Json::from("disk_out_of_sync")),
                ("message", Json::from(format!("{} of the module's files aren't on disk as the store has them, and Cargo compiles the disk: fix them first (project_list shows held and pending files); nothing was built or fixed", out_of_sync.len()))),
                ("files", Json::Arr(out_of_sync)),
            ]));
        }
        Ok(self.world
            .files()
            .range(prefix.clone()..)
            .take_while(|(p, _)| p.starts_with(&prefix))
            .map(|(p, f)| ModuleText { disk: specula_logic::fix::canonical(&m.disk_path(&p[prefix.len()..])), path: p.clone(), version: f.version, hash: f.hash, text: Arc::clone(&f.text) })
            .collect())
    }

    /// `logic_build {fix: true}`'s fixes for a mounted module, committed as the caller's change
    /// (claims checked, journaled, undoable) and then written to disk by the engine: never by the
    /// fixer, so nothing is imported as `disk`. A file changed since the build read it (another
    /// agent's edit, a human's) keeps its text: its fixes are skipped and reported, since their
    /// offsets belong to the old text.
    /// The fixes committed now; `Some(paths)` when they were (the caller answers once the disk has
    /// them).
    fn project_commit_fixes(&mut self, files: Vec<(String, u64, u64, String)>, w: Writer, by: &str) -> (FixesCommitted, Option<Vec<String>>) {
        let mut out = FixesCommitted::default();
        let mut puts = Vec::new();
        for (path, version, hash, text) in files {
            if let Some(ev) = self.project.refresh(&path) {
                self.project_import(vec![ev]);
            }
            // The disk must still hold exactly the text the fixes were planned from.
            let on_disk = !self.project.held.contains_key(&path) && !self.project.pending.contains_key(&path) && self.project.stamp_of(&path).map(|s| s.hash) == Some(hash);
            match self.world.files().get(&path) {
                Some(_) if !on_disk => out.skipped.push((path, "the disk doesn't hold the text the build compiled (held, pending, or changed)".into())),
                Some(f) if (f.version, f.hash) == (version, hash) => {
                    if text.len() > project::MAX_FILE_BYTES {
                        out.skipped.push((path, "the fixed file would pass 1 MiB".into()));
                        continue;
                    }
                    let next = ProjectFile::new(f.kind, Arc::from(text), self.world.file_version_floor(&path) + 1, Arc::from(by));
                    puts.push((path, next));
                }
                Some(f) => out.skipped.push((path, format!("changed during the build (now version {} by {}): its fixes belong to the old text", f.version, f.modified_by))),
                None => out.skipped.push((path, "deleted during the build".into())),
            }
        }
        if puts.is_empty() {
            return (out, None);
        }
        let paths: Vec<String> = puts.iter().map(|(p, _)| p.clone()).collect();
        let tick = self.clock.tick();
        match self.world.commit(&mut self.swarm, tick, w, crate::journal::Kind::File, crate::write_policy::Rule::Project, |wd| {
            for (p, f) in puts {
                wd.put_file(&p, Some(f), false);
            }
            Ok(())
        }) {
            Ok((_, seq)) => {
                out.journal = seq;
                return (out, Some(paths));
            }
            Err(e) => out.error = Some(e.to_json()),
        }
        (out, None)
    }

    /// At start: the assets root becomes the `assets` mount, its files the store's first entry.
    pub(crate) fn project_mount_assets(&mut self, root: PathBuf) {
        self.swarm.reserve(DISK_AGENT);
        let root = specula_logic::fix::canonical(&root);
        let n = self.project_mount_folder(Mount { name: project::ASSETS.into(), root, kind: MountKind::Assets });
        if n > 0 {
            eprintln!("specula: {n} project files mirrored (project_list)");
        }
    }

    /// Imports what the disk holds (edits made outside the engine, mounts, races found while
    /// writing) as one journal entry by `disk`. Returns the paths whose text changed.
    fn project_import(&mut self, events: Vec<Event>) -> Vec<String> {
        // The last word per path.
        let mut by_path: BTreeMap<String, Event> = BTreeMap::new();
        for ev in events {
            by_path.insert(ev.path().to_owned(), ev);
        }
        let mut puts: Vec<(String, Option<ProjectFile>, bool)> = Vec::new();
        let mut stamps = Vec::new();
        for (path, ev) in by_path {
            if !self.project.still_current(&ev) {
                continue;
            }
            let cur = self.world.files().get(&path);
            match ev {
                Event::Changed { kind, text, stamp, .. } => {
                    if cur.is_some_and(|f| f.hash == stamp.hash) {
                        // Readable again, and what the store holds: nothing is held any more.
                        self.project.held.remove(&path);
                        self.project.pending.remove(&path);
                        self.project.stamp(&path, stamp);
                        continue;
                    }
                    let room = if kind.is_binary() {
                        project::room_for_binary(self.world.files(), &path, crate::blobs::parse_pointer(&text).map_or(0, |(_, b)| b))
                    } else {
                        project::room_for(self.world.files(), &path, text.len())
                    };
                    if let Err(e) = room {
                        self.project.note_conflict(&path, format!("changed on disk but not mirrored: {}", e.message));
                        continue;
                    }
                    if self.project.pending.contains_key(&path) {
                        let v = cur.map_or(0, |f| f.version);
                        self.project.note_conflict(&path, format!("edited on disk while the engine's version {v} wasn't written yet: the disk's text was kept, and version {v} is in the journal (project_read with version, or world_undo)"));
                    }
                    self.project.held.remove(&path);
                    let version = self.world.file_version_floor(&path) + 1;
                    puts.push((path.clone(), Some(ProjectFile::new(kind, text, version, Arc::from(DISK_AGENT))), cur.is_none()));
                    stamps.push((path, Some(stamp)));
                }
                Event::Removed { .. } => {
                    self.project.held.remove(&path);
                    if cur.is_some() {
                        puts.push((path.clone(), None, false));
                    }
                    stamps.push((path, None));
                }
                Event::Unreadable { len, mtime, why, .. } => {
                    let fresh = self.project.held.get(&path).is_none_or(|h| (h.0, h.1) != (len, mtime));
                    if fresh {
                        self.project.note_conflict(&path, format!("changed on disk into something that can't be mirrored ({why}): held, never overwritten; fix it on disk or replace it with replace_unreadable: true"));
                    }
                    self.project.held.insert(path, (len, mtime, why));
                }
            }
        }
        let tick = self.clock.tick();
        let changed: Vec<String> = puts.iter().map(|(p, _, _)| p.clone()).collect();
        if !puts.is_empty() {
            let result = self.world.disk(&mut self.swarm, tick, |wd| {
                for (p, f, import) in puts {
                    wd.put_file(&p, f, import);
                }
                Ok(())
            });
            if let Err(e) = result {
                eprintln!("specula: project files not imported: {e}");
                return Vec::new();
            }
        }
        for (p, s) in stamps {
            self.project.pending.remove(&p);
            match s {
                Some(s) => self.project.stamp(&p, s),
                None => self.project.forget(&p),
            }
        }
        changed
    }

    /// Runs `then` with `path`'s disk state (`"written"`, `{pending: error}`, `{conflict: ...}`) and
    /// what was reloaded, once a sync that started after now has ended (it starts now unless one
    /// is writing; then right after it).
    fn project_sync_then(&mut self, path: Option<&str>, then: impl FnOnce(&mut Engine, Json, Vec<&'static str>) + 'static) {
        self.project_waiters.push((path.map(str::to_owned), Box::new(then)));
        self.project_sync_start();
    }

    /// Starts a sync of the disk with the store: planned here, written on the project lane
    /// (temporary file, `fsync`, rename, read-back: one `fsync` per file held the engine thread
    /// before, class C), applied when it returns. One at a time: a sync asked for meanwhile
    /// starts when it ends.
    fn project_sync_start(&mut self) {
        if self.project_syncing {
            return;
        }
        self.project_retry = Instant::now();
        let plan = self.project.sync_plan(self.world.files(), self.world.files_revision());
        let waiters = std::mem::take(&mut self.project_waiters);
        if plan.is_empty() {
            // Nothing to write: nothing touches the disk.
            let done = plan.execute();
            self.project_synced(done, waiters);
            return;
        }
        self.project_syncing = true;
        let job = crate::workers::Offload::engine(crate::workers::JobKind::ProjectSync).lane(crate::workers::Lane::Project).class(crate::schedule::Class::Edit);
        let then = move |e: &mut Engine, r: Result<project::SyncDone, crate::workers::Failed>| {
            e.project_syncing = false;
            match r {
                Ok(done) => e.project_synced(done, waiters),
                Err(f) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "project.sync_failed", error = format!("{f:?}"));
                    let state = Json::obj([("pending", Json::from(format!("the write failed on its worker ({f:?})"))), ("note", Json::from("the change is committed and journaled; the engine retries the write every second"))]);
                    for (_, k) in waiters {
                        k(e, state.clone(), Vec::new());
                    }
                }
            }
            // Calls that waited for the disk go now, in order (one may start the next sync); calls
            // that asked for a sync while this one wrote get theirs at once.
            e.project_run_deferred();
            if !e.project_waiters.is_empty() {
                e.project_sync_start();
            }
            e.project_after_request();
        };
        let _ = self.offload(job, move |_| plan.execute(), then);
    }

    /// Runs `then` once the disk holds what the store holds now (a sync that starts after now has
    /// ended): an undo or a load answers after its files are written.
    pub(crate) fn project_sync_after(&mut self, then: impl FnOnce(&mut Engine) + 'static) {
        self.project_sync_then(None, move |e, _, _| then(e));
    }

    /// Waits for the sync in progress (a mount, which answers in its own call).
    fn project_wait_sync(&mut self) {
        let until = Instant::now() + Duration::from_secs(20);
        while self.project_syncing && Instant::now() < until {
            self.workers_wait_one(Duration::from_millis(20));
        }
    }

    /// Runs the calls held while a sync wrote, in order, until one starts another sync.
    fn project_run_deferred(&mut self) {
        while !self.project_syncing {
            let Some(d) = self.project_deferred.pop_front() else { return };
            let w = Writer::new(&d.name, d.key.as_deref());
            // Their answers go through their own reply channels.
            let _ = self.project_call(d.call, w, &d.by);
        }
    }

    /// A sync ended: what it found applied, races imported, what changed hot-reloaded, and the
    /// calls waiting for it answered with their file's state.
    fn project_synced(&mut self, done: project::SyncDone, waiters: Vec<SyncWaiter>) {
        let synced = self.project.sync_apply(done);
        let mut changed = synced.changed;
        let state_of = |p: &str| -> Json {
            if let Some((_, e)) = synced.failed.iter().find(|(q, _)| q == p) {
                return Json::obj([("pending", Json::from(e.as_str())), ("note", Json::from("the change is committed and journaled; the engine retries the write every second (project_list shows it)"))]);
            }
            if synced.raced.iter().any(|(q, _, _)| q == p) {
                return Json::obj([("conflict", Json::from("someone wrote the file on disk at the same moment: their text was kept and imported as a newer version; yours is in the journal"))]);
            }
            Json::from("written")
        };
        let states: Vec<(Json, SyncThen)> = waiters.into_iter().map(|(p, k)| (p.as_deref().map_or(Json::from("written"), state_of), k)).collect();
        for (p, e) in &synced.failed {
            eprintln!("specula: project file {p} not written: {e}");
        }
        if !synced.raced.is_empty() {
            let events = synced
                .raced
                .into_iter()
                .map(|(p, text, stamp)| {
                    let kind = self.world.files().get(&p).map(|f| f.kind).or_else(|| self.project.locate(&p).and_then(|(m, rel)| project::kind_for(m.kind, &rel))).unwrap_or(FileKind::Json);
                    self.project.note_conflict(&p, "edited on disk while the engine wrote it: the disk's text was kept, the engine's is in the journal".into());
                    Event::Changed { path: p, kind, text, stamp }
                })
                .collect();
            changed.extend(self.project_import(events));
        }
        let reloaded = self.project_reload(&changed);
        for (state, k) in states {
            k(self, state, reloaded.clone());
        }
    }

    /// Reloads what uses `changed` files: model recipes, styles and anim sets in the engine's
    /// library (now), everything under `assets/` in the hosted game ([`crate::game::Game::assets_changed`]).
    fn project_reload(&mut self, changed: &[String]) -> Vec<&'static str> {
        let mut done = Vec::new();
        if changed.is_empty() {
            return done;
        }
        let assets: Vec<String> = changed.iter().filter_map(|p| p.strip_prefix("assets/")).map(str::to_owned).collect();
        if assets.iter().any(|r| matches!(project::kind_for(MountKind::Assets, r), Some(FileKind::Recipe | FileKind::Style | FileKind::AnimSet | FileKind::Glb | FileKind::Gltf | FileKind::Png))) {
            self.poll_assets(true);
            done.push("models");
        }
        if !assets.is_empty() && self.game_call(crate::sim_writes::GameCall::AssetsChanged(&assets)) {
            done.push("game");
        }
        done
    }

    /// Once per loop: edits the watcher found on disk come in, failed writes are retried, and a
    /// store changed by anything (undo, a load) reaches the disk.
    pub(crate) fn project_pump(&mut self) {
        // While a sync writes, edits found on disk wait (they are compared with what it wrote).
        if self.project_syncing {
            return;
        }
        let events = self.project.drain();
        if !events.is_empty() {
            let changed = self.project_import(events);
            self.project_reload(&changed);
        }
        self.project_after_request();
    }

    /// After a command: if the store changed (undo, a world load), the disk follows. Failed
    /// writes wait [`RETRY_EVERY`] between tries.
    pub(crate) fn project_after_request(&mut self) {
        if self.project_syncing || self.world.files_revision() == self.project.synced_revision {
            return;
        }
        if !self.project.pending.is_empty() && self.project_retry.elapsed() < RETRY_EVERY {
            return;
        }
        self.project_sync_start();
    }
}

#[cfg(test)]
mod tests {
    use crate::claims::{ClaimSpec, Scope};
    use crate::journal::{Kind, UndoRequest, Writer};
    use crate::project::{FileKind, ProjectFile};
    use crate::swarm::Swarm;
    use crate::world::{Recording, Shape, SpawnSpec, World};
    use specula_protocol::SessionInfo;
    use std::sync::Arc;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    fn file(text: &str, version: u64, by: &str) -> Option<ProjectFile> {
        Some(ProjectFile::new(FileKind::Recipe, Arc::from(text), version, Arc::from(by)))
    }

    #[test]
    fn files_share_claims_journal_undo_and_replay() {
        let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.hello(&s2, "bob".into(), "builder".into(), None, &live).unwrap();
        let (k1, k2) = (sw.key(&s1), sw.key(&s2));
        let (ann, bob) = (Writer::new("ann", Some(&k1)), Writer::new("bob", Some(&k2)));
        let p = "assets/recipes/t.json";
        // An import (the start-up mount) can't be undone into a delete.
        sw.commit_disk(&mut w, 0, |wd| {
            wd.put_file_raw(p, file("{\"v\": 0}", 1, "disk"), true);
            Ok(())
        })
        .unwrap();
        let imported = sw.journal().entries().last().unwrap().seq;
        let undo_import = sw.undo(&mut w, 0, ann, &UndoRequest { ids: Some(vec![imported]), count: 1, force: true });
        assert_eq!(undo_import.unwrap_err().code, "not_undoable");
        // Ann writes v2, bob v3; history is attributed per file.
        sw.commit_project(&mut w, 1, ann, |wd| {
            wd.put_file_raw(p, file("{\"v\": 1}", 2, "ann"), false);
            Ok(())
        })
        .unwrap();
        sw.commit_project(&mut w, 2, bob, |wd| {
            wd.put_file_raw(p, file("{\"v\": 2}", 3, "bob"), false);
            Ok(())
        })
        .unwrap();
        let last = sw.journal().entries().last().unwrap();
        assert_eq!((last.kind, &*last.by, last.changes[0].op(), last.changes[0].path()), (Kind::File, "bob", "file_write", "/project/assets/recipes/t.json"));
        // Ann's undo would overwrite bob's later change: refused without force.
        assert_eq!(sw.undo(&mut w, 3, ann, &UndoRequest { ids: None, count: 1, force: false }).unwrap_err().code, "undo_conflict");
        // Bob undoes his: back to ann's text, as a new version.
        let u = sw.undo(&mut w, 3, bob, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
        assert_eq!(u.changes, 1);
        let f = &w.files()[p];
        assert_eq!((&*f.text, f.version, &*f.modified_by), ("{\"v\": 1}", 4, "bob"));
        // A path claim on the folder blocks the other agent, not the owner.
        sw.take_claim(&w, &s2, ClaimSpec { scope: Scope::path("/project/assets/recipes").unwrap(), lease: std::time::Duration::from_secs(60), note: String::new() }).unwrap();
        let refused = sw.commit_project(&mut w, 4, ann, |wd| {
            wd.put_file_raw(p, file("{\"v\": 9}", 5, "ann"), false);
            Ok(())
        });
        assert_eq!(refused.unwrap_err().code, "claimed");
        assert_eq!(&*w.files()[p].text, "{\"v\": 1}", "nothing changed");
        sw.commit_project(&mut w, 4, bob, |wd| {
            wd.put_file_raw(p, file("{\"v\": 5}", 5, "bob"), false);
            Ok(())
        })
        .unwrap();
        // ...but a world load isn't blocked by claims on files (they stay).
        assert!(sw.check_replace_world(ann).is_ok());
        // Nothing but the project tools writes files, and no entity lives in /project.
        let spawn = sw.commit(&mut w, 5, ann, Kind::Spawn, |wd| {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some("project".into());
            wd.spawn(s, "ann").map(|_| ())
        });
        assert_eq!(spawn.unwrap_err().code, "reserved");
        let sneak = sw.commit(&mut w, 5, ann, Kind::Set, |wd| {
            wd.put_file_raw("assets/recipes/x.json", file("{}", 1, "ann"), false);
            Ok(())
        });
        assert_eq!(sneak.unwrap_err().code, "reserved");
        // Deleting, then undoing the delete, restores it.
        sw.commit_project(&mut w, 6, bob, |wd| {
            wd.put_file_raw(p, None, false);
            Ok(())
        })
        .unwrap();
        assert!(!w.files().contains_key(p));
        sw.undo(&mut w, 7, bob, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
        assert_eq!(&*w.files()[p].text, "{\"v\": 5}");
        // The journal replays the files byte for byte (F2: replays reproduce project data).
        let replayed = sw.journal().replay().unwrap();
        assert_eq!(replayed.files(), w.files());
        assert_eq!(replayed.state_hash(), w.state_hash(), "files aren't world state: the hash is the world's");
    }

    /// Review finding: one agent writing a 980 KB file 28 times evicted everyone's history
    /// (world_undo found nothing, world_hash verify said replayable: false). File text now has
    /// its own budget: past it the oldest file texts go, and nothing else.
    #[test]
    fn file_text_never_evicts_world_history() {
        let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.hello(&s2, "bob".into(), "builder".into(), None, &live).unwrap();
        let (k1, k2) = (sw.key(&s1), sw.key(&s2));
        let (ann, bob) = (Writer::new("ann", Some(&k1)), Writer::new("bob", Some(&k2)));
        sw.journal_for_test().set_file_budget(8 << 20);
        sw.commit(&mut w, 0, bob, Kind::Spawn, |wd| wd.spawn(SpawnSpec::new(Shape::Cube), "bob").map(|_| ())).unwrap();
        let p = "assets/anims/big.json";
        let mut first = None;
        for i in 0..28u64 {
            let text = format!("[{i}{}]", ",0".repeat(490_000));
            let (_, seq) = sw
                .commit_project(&mut w, 1 + i, ann, |wd| {
                    let v = wd.file_version_floor(p) + 1;
                    wd.put_file_raw(p, Some(ProjectFile::new(FileKind::AnimSet, Arc::from(text), v, Arc::from("ann"))), false);
                    Ok(())
                })
                .unwrap();
            first = first.or(seq);
        }
        let (held, budget) = sw.journal().file_text();
        assert!(held <= budget, "{held} > {budget}");
        assert!(sw.journal().replayable(), "the world's replay base is untouched");
        assert_eq!(sw.journal().replay().unwrap().state_hash(), w.state_hash());
        // Bob's change is still his to undo.
        sw.undo(&mut w, 40, bob, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
        assert_eq!(w.len(), 0);
        // Ann's oldest write lost its text: it can't be undone, and says why.
        let e = sw.undo(&mut w, 41, ann, &UndoRequest { ids: Some(vec![first.unwrap()]), count: 1, force: true }).unwrap_err();
        assert_eq!(e.code, "too_old");
        // Her newest still can.
        sw.undo(&mut w, 42, ann, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
        assert!(w.files()[p].text.starts_with("[26,"));
    }

    /// Review finding: a file deleted and made again restarted at version 1, so an
    /// `expect_version` read from the old file matched the new one (ABA).
    #[test]
    fn versions_never_repeat_at_a_path() {
        let mut w = World::default();
        let p = "assets/sounds/aba.json";
        for v in 1..=3 {
            w.put_file_raw(p, file("{}", v, "ann"), false);
        }
        w.put_file_raw(p, None, false);
        assert_eq!(w.file_version_floor(p), 3, "the deleted file's last version is remembered");
        assert_eq!(w.file_version_floor("assets/sounds/other.json"), 0);
        w.put_file_raw(p, file("{}", w.file_version_floor(p) + 1, "ann"), false);
        assert_eq!(w.files()[p].version, 4);
        // Replays and world loads carry the floor.
        let mut next = World::default();
        let rev = w.files_revision();
        w.put_file_raw(p, None, false);
        next.adopt_files(w.take_files(), rev);
        assert_eq!(next.file_version_floor(p), 4);
    }

    /// Review round 2: a version whose text the budget dropped was read back as "" (and used as
    /// an empty conflict base). It is "not kept" now.
    #[test]
    fn dropped_texts_are_not_kept_versions() {
        let s1 = session("aaaaaa11");
        let mut sw = Swarm::default();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        sw.hello(&s1, "ann".into(), "builder".into(), None, std::slice::from_ref(&s1)).unwrap();
        let k1 = sw.key(&s1);
        let ann = Writer::new("ann", Some(&k1));
        sw.journal_for_test().set_file_budget(3_000);
        let p = "assets/sounds/s.json";
        for v in 1..=4u64 {
            let text = format!("{{\"v\": {v}, \"pad\": \"{}\"}}", "x".repeat(1_000));
            sw.commit_project(&mut w, v, ann, |wd| {
                wd.put_file_raw(p, file(&text, v, "ann"), false);
                Ok(())
            })
            .unwrap();
        }
        let (old, later) = super::at_version(sw.journal(), p, 1);
        assert!(old.is_none(), "version 1's text was dropped: not kept, never \"\"");
        assert!(!later.is_empty(), "the later changes are still listed");
        assert!(super::at_version(sw.journal(), p, 3).0.is_some_and(|t| t.contains("\"v\": 3")));
    }

    /// Review round 2: the list of entries holding file text had no bound tied to the rings.
    #[test]
    fn file_text_entries_follow_the_rings() {
        let mut j = crate::journal::Journal::with_limits(&World::default(), "t", 50, 1 << 30);
        let mut w = World::default();
        w.set_recording(Recording::Full);
        for v in 1..=2_000u64 {
            crate::journal::commit(&mut w, &mut j, v, "ann", Kind::File, None, |wd| {
                wd.put_file_raw("assets/sounds/s.json", file("{}", v, "ann"), false);
                Ok(())
            })
            .unwrap();
        }
        assert!(j.file_text_entries() <= 50, "{}", j.file_text_entries());
    }

    /// Review round 2: tombstone eviction took the lexicographically first path, not the oldest.
    #[test]
    fn the_oldest_tombstone_goes_first() {
        let mut w = World::default();
        let made = |w: &mut World, p: &str, v: u64| {
            w.put_file_raw(p, file("{}", v, "ann"), false);
            w.put_file_raw(p, None, false);
        };
        made(&mut w, "assets/zzz_first.json", 7);
        for i in 0..crate::world::MAX_FILE_TOMBSTONES {
            made(&mut w, &format!("assets/a{i:05}.json"), 2);
        }
        assert_eq!(w.file_version_floor("assets/a00000.json"), 7, "the oldest (zzz_first, v7) went and raised the floor");
        assert_eq!(w.file_version_floor("assets/zzz_first.json"), 7);
        made(&mut w, "assets/b.json", 9);
        assert_eq!(w.file_version_floor("assets/a00001.json"), 7, "a00000 went next (v2): the floor stays 7");
    }

    /// Binary files: the journal holds pointers, never bytes; their history has its own budget
    /// (past it the oldest binary changes can't be undone, and the content store may let their
    /// bytes go: `blob_pointers` stops naming them); undo puts the earlier pointer back.
    #[test]
    fn binary_history_has_its_own_budget() {
        use crate::blobs::{parse_pointer, pointer, BlobId};
        let s1 = session("aaaaaa11");
        let mut sw = Swarm::default();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        sw.hello(&s1, "ann".into(), "builder".into(), None, std::slice::from_ref(&s1)).unwrap();
        let k1 = sw.key(&s1);
        let ann = Writer::new("ann", Some(&k1));
        sw.journal_for_test().set_blob_budget(3 << 20);
        let p = "assets/textures/t.png";
        let mut seqs = Vec::new();
        for v in 1..=4u64 {
            let text = pointer(BlobId(v, v, 0), 1 << 20);
            let (_, seq) = sw
                .commit_project(&mut w, v, ann, |wd| {
                    wd.put_file_raw(p, Some(ProjectFile::new(FileKind::Png, Arc::from(text), v, Arc::from("ann"))), false);
                    Ok(())
                })
                .unwrap();
            seqs.push(seq.unwrap());
        }
        let (held, budget) = sw.journal().file_blobs();
        assert!(held <= budget, "{held} > {budget}");
        assert!(sw.journal().file_text().0 < 4096, "the journal holds pointers, not bytes");
        let named: Vec<BlobId> = sw.journal().blob_pointers().iter().filter_map(|t| parse_pointer(t)).map(|(id, _)| id).collect();
        assert!(!named.contains(&BlobId(1, 1, 0)), "the oldest content is no longer needed: {named:?}");
        let e = sw.undo(&mut w, 9, ann, &UndoRequest { ids: Some(vec![seqs[0]]), count: 1, force: true }).unwrap_err();
        assert_eq!(e.code, "too_old");
        sw.undo(&mut w, 10, ann, &UndoRequest { ids: None, count: 1, force: false }).unwrap();
        assert_eq!(parse_pointer(&w.files()[p].text).map(|(id, _)| id), Some(BlobId(3, 3, 0)), "undo restores the earlier content");
        assert_eq!(sw.journal().replay().unwrap().files(), w.files(), "replays reproduce the pointers");
    }

    /// Review finding: `logic_build {fix: true}` wrote a mounted module's sources past other
    /// agents' claims on single files.
    #[test]
    fn subtree_checks_see_claims_below() {
        let (s1, s2) = (session("aaaaaa11"), session("bbbbbb22"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let w = World::default();
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.hello(&s2, "bob".into(), "builder".into(), None, &live).unwrap();
        let k1 = sw.key(&s1);
        let ann = Writer::new("ann", Some(&k1));
        sw.take_claim(&w, &s2, ClaimSpec { scope: Scope::path("/project/modules/ambulo/src/lib.rs").unwrap(), lease: std::time::Duration::from_secs(60), note: String::new() }).unwrap();
        let now = std::time::Instant::now();
        assert!(sw.claims.check_path("/project/modules/ambulo", ann, now, "fix").is_ok(), "nothing covers the folder itself");
        assert_eq!(sw.claims.check_subtree("/project/modules/ambulo", ann, now, "fix").unwrap_err().code, "claimed");
        assert!(sw.claims.check_subtree("/project/modules/other", ann, now, "fix").is_ok());
    }
}
