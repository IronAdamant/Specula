//! The engine side of the project tools (see `project_tools` for their arguments and
//! docs/design/PROJECT_DATA.md): snapshots, commits through the swarm's claims and journal, the
//! disk kept equal to the store, imports of edits made outside the engine, and hot reload.

use crate::game::Ctx;
use crate::journal::{Kind, Writer};
use crate::project::{self, Event, FileKind, Mount, MountKind, ProjectFile, DISK_AGENT};
use crate::project_tools::{Committed, ProjectCall, Snap};
use crate::world::{Change, Recording, WorldError};
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

/// The text `path` had at `version`, and the changes after it (newest first), from the journal
/// (or the world it started from).
fn at_version(journal: &crate::journal::Journal, path: &str, version: u64) -> (Option<Arc<str>>, Vec<Json>) {
    let id = project::file_id(path);
    let mut later = Vec::new();
    for e in journal.touching(id, SCAN) {
        for c in e.changes.iter().rev().filter(|c| c.id() == id) {
            let Change::File { before, after, .. } = c else { continue };
            if let Some(a) = after.as_deref().filter(|a| a.version == version) {
                return (Some(Arc::clone(&a.text)), later);
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
                return (Some(Arc::clone(&b.text)), later);
            }
            if before.as_deref().is_some_and(|b| b.version < version) {
                return (None, later);
            }
        }
    }
    let base = journal.base().files().get(path).filter(|f| f.version == version).map(|f| Arc::clone(&f.text));
    (base, later)
}

impl Engine {
    /// Answers a project call on the main thread.
    pub(crate) fn project_call(&mut self, call: ProjectCall, w: Writer, by: &str) -> ToolOutput {
        match call {
            ProjectCall::List { prefix, kind, after, limit } => ToolOutput::text(self.project_list(prefix.as_deref(), kind, after.as_deref(), limit).to_string()),
            ProjectCall::Snapshot { path, version, context, reply } => {
                let snap = self.project_snapshot(&path, version, context);
                let _ = reply.send(snap);
                ToolOutput::text("{}")
            }
            ProjectCall::Commit { path, base, file, replace_unreadable, reply } => {
                let done = self.project_commit(&path, base, file, replace_unreadable, w, by);
                let _ = reply.send(done);
                ToolOutput::text("{}")
            }
            ProjectCall::Mount { dir, name } => self.project_mount(dir, name),
            ProjectCall::FixGate { dir } => self.project_fix_gate(&dir, w),
            ProjectCall::Adopt { dir } => self.project_adopt(&dir, w, by),
        }
    }

    fn project_snapshot(&mut self, path: &str, version: Option<u64>, context: bool) -> Snap {
        if let Err(e) = project::check_path(path) {
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
            (Some(v), _) => at_version(&self.swarm.journal, path, v),
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
        Snap { kind, module_dir, current, at_version: at, later, context: ctx, error: None }
    }

    fn project_commit(&mut self, path: &str, base: (u64, u64), file: Option<(FileKind, Arc<str>)>, replace_unreadable: bool, w: Writer, by: &str) -> Committed {
        if let Some(ev) = self.project.refresh(path) {
            self.project_import(vec![ev]);
        }
        let cur = self.world.files().get(path);
        if (cur.map_or(0, |f| f.version), cur.map_or(0, |f| f.hash)) != base {
            return Committed::Changed;
        }
        let Some((mount, rel)) = self.project.locate(path) else {
            return Committed::Refused(WorldError::new("not_mirrored", format!("{path} isn't in a mount (project_list shows them)")).with("path", path));
        };
        let Some(kind) = project::kind_for(mount.kind, &rel) else {
            return Committed::Refused(WorldError::new("not_mirrored", format!("{path} isn't a kind of file the project mirrors")).with("path", path));
        };
        // Never into a linked folder: the write could only fail, after the commit.
        if let Some(dir) = mount.disk_path(&rel).parent() {
            if let Err(e) = project::check_inside(&mount.root, dir) {
                return Committed::Refused(WorldError::new("linked_folder", format!("{path}: {e}; nothing was changed (write somewhere that isn't behind a link)")).with("path", path));
            }
        }
        // The disk holds something that can't be mirrored: never overwritten unless asked to.
        let mut backup = None;
        if let Some((_, _, why)) = self.project.held.get(path).cloned() {
            if !replace_unreadable {
                return Committed::Refused(
                    WorldError::new("disk_unreadable", format!("{path} was changed on disk into something that can't be mirrored ({why}); nothing was changed. Fix it on disk (it is imported then), or pass replace_unreadable: true to replace it (its bytes are backed up first)"))
                        .with("path", path)
                        .with("why", why.as_str()),
                );
            }
            match self.project.back_up_and_release(path) {
                Ok(b) => backup = Some(b.display().to_string()),
                Err(e) => return Committed::Refused(WorldError::new("backup_failed", format!("{path} couldn't be backed up before replacing it: {e}; nothing was changed")).with("path", path)),
            }
        }
        let new = match file {
            Some((_, text)) => {
                if let Err(e) = project::room_for(self.world.files(), path, text.len()) {
                    return Committed::Refused(e);
                }
                if cur.is_none() {
                    // One file on Windows (which folds case in Unicode): refuse a second spelling
                    // everywhere, so projects stay portable.
                    let folded = project::fold_case(path);
                    if let Some(other) = self.world.files().keys().find(|p| project::fold_case(p) == folded) {
                        return Committed::Refused(WorldError::new("case_clash", format!("{other} exists; {path} differs only in case, which is the same file on Windows")).with("path", path).with("existing", other.as_str()));
                    }
                }
                // Versions never repeat at a path, even across a delete (no ABA for expect_version).
                Some(ProjectFile::new(kind, text, self.world.file_version_floor(path).max(base.0) + 1, Arc::from(by)))
            }
            None if cur.is_none() => return Committed::Refused(WorldError::new("not_found", format!("there is no project file {path}")).with("path", path)),
            None => None,
        };
        let tick = self.clock.tick();
        match self.swarm.commit_project(&mut self.world, tick, w, |wd| {
            wd.put_file_raw(path, new, false);
            Ok(())
        }) {
            Err(e) => Committed::Refused(e),
            Ok((_, seq)) => {
                let (disk, reloaded) = self.project_sync_for(Some(path));
                let version = self.world.files().get(path).map(|f| f.version);
                let mut j = vec![("path", Json::from(path)), ("version", Json::from(version)), ("journal", Json::from(seq))];
                if let Some(f) = self.world.files().get(path) {
                    j.push(("bytes", Json::from(f.text.len())));
                } else {
                    j.push(("deleted", Json::Bool(true)));
                }
                j.push(("disk", disk));
                if let Some(b) = backup {
                    j.push(("backup", Json::from(b)));
                }
                j.push(("reloaded", Json::arr(reloaded.into_iter().map(Json::from))));
                Committed::Done(Json::obj(j))
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
            let mut item = vec![("path", Json::from(p.as_str())), ("kind", Json::from(f.kind.name())), ("version", Json::from(f.version)), ("bytes", Json::from(f.text.len())), ("by", Json::from(&*f.modified_by))];
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
        let problems = crate::project_kinds::check_module_manifest("Cargo.toml", &manifest);
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
        let mounted = self.project.mounts().iter().any(|m| m.name == name);
        if !mounted && self.project.module_mounts() >= project::MAX_MODULE_MOUNTS {
            return ToolOutput::error(Json::obj([("error", Json::from("mount_limit")), ("message", Json::from(format!("at most {} module crates are mounted at once", project::MAX_MODULE_MOUNTS)))]).to_string());
        }
        if let Some(other) = self.project.mounts().iter().find(|m| m.name == name && m.root != dir) {
            return ToolOutput::error(Json::obj([("error", Json::from("mount_taken")), ("message", Json::from(format!("{name} is already mounted from {}", other.root.display())))]).to_string());
        }
        let n = self.project_mount_folder(Mount { name: name.clone(), root: dir, kind: MountKind::Module });
        let files = self.world.files().range(format!("{name}/")..).take_while(|(p, _)| p.starts_with(&format!("{name}/"))).count();
        ToolOutput::text(Json::obj([("mount", Json::from(name.as_str())), ("files", Json::from(files)), ("imported", Json::from(n))]).to_string())
    }

    /// Mounts `mount` and imports its files (one journal entry by `disk`); returns how many changed.
    fn project_mount_folder(&mut self, mount: Mount) -> usize {
        let found = self.project.mount(mount);
        let events = found.into_iter().map(|(path, kind, text, stamp)| Event::Changed { path, kind, text, stamp }).collect();
        self.project_import(events).len()
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

    /// After `logic_build {fix: true}` wrote fixes into a mounted module: they are committed as the
    /// calling agent's change (claims checked, journaled, undoable) instead of waiting to be
    /// imported as edits by `disk`. A claim that refuses them puts the store's text back on disk.
    fn project_adopt(&mut self, dir: &std::path::Path, w: Writer, by: &str) -> ToolOutput {
        let Some(m) = self.project_module_mount(dir) else { return ToolOutput::text("{}") };
        let prefix = format!("{}/", m.name);
        let paths: Vec<String> = self.project.mounts().iter().filter(|x| x.name == m.name).flat_map(|x| project::scan(x).files.into_iter().map(|(rel, _)| x.path_of(&rel))).collect();
        let mut puts = Vec::new();
        let mut stamps = Vec::new();
        for p in paths.iter().filter(|p| p.starts_with(&prefix)) {
            if let Some(Event::Changed { kind, text, stamp, .. }) = self.project.refresh(p) {
                let version = self.world.file_version_floor(p) + 1;
                puts.push((p.clone(), ProjectFile::new(kind, text, version, Arc::from(by))));
                stamps.push((p.clone(), stamp));
            }
        }
        if puts.is_empty() {
            return ToolOutput::text(Json::obj([("adopted", Json::from(0))]).to_string());
        }
        let n = puts.len();
        let tick = self.clock.tick();
        let result = self.swarm.commit_project(&mut self.world, tick, w, |wd| {
            for (p, f) in puts {
                wd.put_file_raw(&p, Some(f), false);
            }
            Ok(())
        });
        match result {
            Ok((_, seq)) => {
                for (p, s) in stamps {
                    self.project.stamp(&p, s);
                }
                ToolOutput::text(Json::obj([("adopted", Json::from(n)), ("journal", Json::from(seq))]).to_string())
            }
            Err(e) => {
                // Refused: the disk goes back to the store's text (the fixes are dropped).
                for (p, s) in stamps {
                    self.project.stamp(&p, project::Stamp { hash: 0, ..s });
                }
                self.project_sync_for(None);
                ToolOutput::error(e.to_json().to_string())
            }
        }
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
                        self.project.stamp(&path, stamp);
                        continue;
                    }
                    if let Err(e) = project::room_for(self.world.files(), &path, text.len()) {
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
            let result = self.swarm.commit_disk(&mut self.world, tick, |wd| {
                for (p, f, import) in puts {
                    wd.put_file_raw(&p, f, import);
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

    /// Syncs the disk with the store and hot-reloads what changed. Returns `path`'s disk state
    /// (`"written"`, `{pending: error}`, `{conflict: ...}`) and what was reloaded.
    fn project_sync_for(&mut self, path: Option<&str>) -> (Json, Vec<&'static str>) {
        self.project_retry = Instant::now();
        let synced = self.project.sync(self.world.files(), self.world.files_revision());
        let mut changed = synced.changed;
        let mut state = Json::from("written");
        if let Some(p) = path {
            if let Some((_, e)) = synced.failed.iter().find(|(q, _)| q == p) {
                state = Json::obj([("pending", Json::from(e.as_str())), ("note", Json::from("the change is committed and journaled; the engine retries the write every second (project_list shows it)"))]);
            }
            if synced.raced.iter().any(|(q, _, _)| q == p) {
                state = Json::obj([("conflict", Json::from("someone wrote the file on disk at the same moment: their text was kept and imported as a newer version; yours is in the journal"))]);
            }
        }
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
        (state, reloaded)
    }

    /// Reloads what uses `changed` files: model recipes, styles and anim sets in the engine's
    /// library (now), everything under `assets/` in the hosted game ([`crate::game::Game::assets_changed`]).
    fn project_reload(&mut self, changed: &[String]) -> Vec<&'static str> {
        let mut done = Vec::new();
        if changed.is_empty() {
            return done;
        }
        let assets: Vec<String> = changed.iter().filter_map(|p| p.strip_prefix("assets/")).map(str::to_owned).collect();
        if assets.iter().any(|r| matches!(project::kind_for(MountKind::Assets, r), Some(FileKind::Recipe | FileKind::Style | FileKind::AnimSet))) {
            self.poll_assets(true);
            done.push("models");
        }
        if !assets.is_empty() {
            if let Some(game) = self.game.as_mut() {
                self.world.set_recording(Recording::Snapshot);
                game.assets_changed(&mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed }, &assets);
                self.world.set_recording(Recording::Full);
                self.journal_game(Kind::GameFrame);
                done.push("game");
            }
        }
        done
    }

    /// Once per loop: edits the watcher found on disk come in, failed writes are retried, and a
    /// store changed by anything (undo, a load) reaches the disk.
    pub(crate) fn project_pump(&mut self) {
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
        if self.world.files_revision() == self.project.synced_revision {
            return;
        }
        if !self.project.pending.is_empty() && self.project_retry.elapsed() < RETRY_EVERY {
            return;
        }
        self.project_sync_for(None);
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
        let imported = sw.journal.entries().last().unwrap().seq;
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
        let last = sw.journal.entries().last().unwrap();
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
        let replayed = sw.journal.replay().unwrap();
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
        sw.journal.set_file_budget(8 << 20);
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
        let (held, budget) = sw.journal.file_text();
        assert!(held <= budget, "{held} > {budget}");
        assert!(sw.journal.replayable(), "the world's replay base is untouched");
        assert_eq!(sw.journal.replay().unwrap().state_hash(), w.state_hash());
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
