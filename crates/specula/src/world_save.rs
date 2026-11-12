//! An explicit `world_save`, off the engine thread (docs/design/WORKERS.md, C5).
//!
//! The engine thread plans the save (takes the subtrees that changed since the last save off the
//! world, as the autosave always did), and a worker on the world lane renders and writes the
//! files and the task board; the answer comes back through `then`, which updates what the engine
//! knows was saved. Serialising and writing 20,000 entities took 310 ms of the engine thread
//! before (class C, 2026-10-29).
//!
//! Saves and autosaves never overlap: a save asked for while an autosave (or another save) is
//! writing waits and starts when it ends, and an autosave doesn't start while a save is writing.
//! Waiting saves of the same world (and overwrite) are one write that answers every caller (review
//! of 998af3d: ten agents saving at once got five `job_busy`); at most [`MAX_WAITING`] different
//! worlds wait. `world_load` writes the waiting saves first.

use super::*;
use crate::workers::{Cancel, JobKind, Lane, Offload};

/// Different worlds whose saves wait for a write in progress, at most.
const MAX_WAITING: usize = 4;

/// A save asked for, not started yet (every caller that asked for the same world and overwrite).
pub(crate) struct WaitingSave {
    /// The world it saves (`None`: the current one, whatever it is when the save starts).
    name: Option<String>,
    overwrite: bool,
    /// The first caller's identity key (who made the world folder, for later overwrites).
    key: String,
    replies: Vec<jobs::Reply>,
    deadline: Instant,
}

/// The world taken off in slices of about this much engine time, a loop pass each.
const PLAN_SLICE: Duration = Duration::from_millis(2);

/// A save whose plan is being taken off the world ([`Engine::pump_save_planning`]).
pub(crate) struct SavePlanning {
    planner: persist::Planner,
    name: String,
    key: String,
    replies: Vec<jobs::Reply>,
    dir: PathBuf,
    lock: Option<persist::WorldLock>,
    check_exists: bool,
    trace: specula_core::trace::Ctx,
    /// The world it plans (a world that took its place mid-plan is never saved under its name).
    generation: u64,
}

/// What the worker brings back.
struct Written {
    /// Another world lives there (and no overwrite): the refusal; nothing was written.
    exists: Option<Json>,
    result: Result<(persist::Saved, persist::SaveReport), persist::PersistError>,
    board: Option<(u64, crate::swarm_engine::WriteResult)>,
}

fn answer_all(replies: &[jobs::Reply], out: &ToolOutput) {
    for r in replies {
        let _ = r.send(ToolOutput { content: out.content.clone(), is_error: out.is_error });
    }
}

impl Engine {
    /// `world_save`: the authority checks, then the save (now, or after the write in progress).
    pub(crate) fn handle_world_save(&mut self, request: Request) {
        let Request { command: Command::Save { name, overwrite }, session, reply, deadline, .. } = request else { unreachable!("only world_save comes here") };
        let key = self.swarm.key(&session);
        // Saving under the current name is a checkpoint anyone may make. Another name renames the
        // live world (its autosave target, evidence and board files), and overwrite of an existing
        // world folder the caller didn't save in this run (another agent's, or an earlier run's:
        // creators aren't recorded across runs) replaces that world: the lead's call.
        let current = name.as_ref().is_none_or(|n| Some(n) == self.world_name.as_ref());
        let renames = !current && self.world_name.is_some();
        let replaces = !current && overwrite && name.as_ref().is_some_and(|n| self.worlds_dir.join(n).join("world.json").exists() && self.world_folders.get(n) != Some(&key));
        let have = authority::authority(&self.swarm, &session.id);
        let what = if renames { "world_save under another name (it renames the live world)" } else { "world_save with overwrite over a world folder another agent (or an earlier run) made" };
        if let Some(Err(e)) = (renames || replaces).then(|| authority::require(have, authority::Authority::Lead, authority::Site::WorldSave, what)) {
            let _ = reply.send(world_error(&e.with("current", self.world_name.as_deref().unwrap_or(""))));
            return;
        }
        // A load is replacing the world: a save now would be written after it, of a world that
        // is no longer the one this caller meant (`world_load`'s module docs).
        if let Some(e) = self.load_busy("a save now would be of the world it replaces") {
            reply.refuse(&e);
            return;
        }
        let target = name.clone().or_else(|| self.world_name.clone());
        if self.world_write_busy() {
            // The same world waiting already: one write answers both.
            let same = self.waiting_saves.iter_mut().find(|w| w.overwrite == overwrite && w.name.clone().or_else(|| self.world_name.clone()) == target);
            if let Some(w) = same {
                w.replies.push(reply);
                w.deadline = w.deadline.min(deadline);
                return;
            }
            if self.waiting_saves.len() >= MAX_WAITING {
                let e = authority::refusal(authority::Site::JobBusy, format!("saves of {MAX_WAITING} other worlds are waiting for the one being written: call again when it has answered"))
                    .with("retry_after_s", 1)
                    .with("retry", true);
                reply.refuse(&e);
                return;
            }
            self.waiting_saves.push_back(WaitingSave { name, overwrite, key, replies: vec![reply], deadline });
            return;
        }
        self.start_world_save(WaitingSave { name, overwrite, key, replies: vec![reply], deadline });
    }

    /// Whether the world's folder is being written (an autosave or a save).
    pub(crate) fn world_write_busy(&self) -> bool {
        self.autosave_job.is_some() || self.world_save_running
    }

    /// Starts the waiting saves whose turn it is (once per loop pass; a load asks too).
    pub(crate) fn pump_world_saves(&mut self) {
        self.pump_save_planning();
        self.finish_autosave(false);
        // A load parked behind the saves: it starts once they are written, and answers at its
        // deadline if they aren't. (Review R2 of m5-gpuwork: this was called only from the
        // branch below, where nothing is being written, so the deadline answer could never
        // run: a load behind a stuck save held every save off until its caller gave up.)
        self.pump_world_load();
        while !self.world_write_busy() {
            let Some(save) = self.waiting_saves.pop_front() else {
                // A load parked behind the saves goes once they are written.
                self.pump_world_load();
                return;
            };
            if Instant::now() > save.deadline {
                let out = ToolOutput::error(
                    Json::obj([
                        ("error", Json::from("engine_busy")),
                        ("message", Json::from("skipped: the save waited too long for the write before it; nothing was written: call again")),
                        ("retry", Json::Bool(true)),
                    ])
                    .to_string(),
                );
                answer_all(&save.replies, &out);
                continue;
            }
            self.start_world_save(save);
        }
    }

    /// Before a load replaces the world: the saves asked for so far are written (blocking: a load
    /// is rare, lead-gated work, and those saves are of the world it replaces).
    pub(crate) fn flush_world_saves(&mut self) {
        let until = Instant::now() + Duration::from_secs(25);
        while (self.world_write_busy() || !self.waiting_saves.is_empty()) && Instant::now() < until {
            self.pump_world_saves();
            if self.world_save_running {
                self.workers_wait_one(Duration::from_millis(20));
            } else if self.autosave_job.is_some() {
                self.finish_autosave(true);
            }
        }
    }

    fn start_world_save(&mut self, save: WaitingSave) {
        let WaitingSave { name, overwrite, key, replies, .. } = save;
        let Some(name) = name.or_else(|| self.world_name.clone()) else {
            let out = ToolOutput::error(
                Json::obj([
                    ("error", Json::from("no_world_name")),
                    ("message", Json::from("give 'name': there is no current world (start with --world NAME or save once with a name)")),
                ])
                .to_string(),
            );
            answer_all(&replies, &out);
            return;
        };
        let dir = self.worlds_dir.join(&name);
        let current = self.world_name.as_deref() == Some(name.as_str());
        // Whether another world lives there is looked at on the worker (a file read).
        let check_exists = !current && !overwrite;
        let lock = match self.lock_for(&dir) {
            Ok(lock) => lock,
            Err(e) => {
                answer_all(&replies, &ToolOutput::error(e.to_json().to_string()));
                return;
            }
        };
        // Taken off the world in slices (a loop pass each, `pump_save_planning`); rendered and
        // written on the worker.
        let planner = persist::Planner::new(&self.world, &dir, self.saved.as_ref());
        self.world_save_running = true;
        self.save_planning = Some(SavePlanning { planner, name, key, replies, dir, lock, check_exists, trace: specula_core::trace::Ctx::current(), generation: self.world_generation });
        self.pump_save_planning();
    }

    /// The save being planned takes its next slice of the world; once every changed subtree is
    /// copied, the plan is finished (the world as it is then) and written on the worker.
    /// The cache of what was saved, replaced; the previous one (100,000 loose entities' records)
    /// is freed on a worker: dropping it here held the engine thread 85-95 ms (stalls fix round).
    pub(crate) fn replace_saved(&mut self, saved: persist::Saved) {
        if let Some(old) = self.saved.replace(saved) {
            let job = crate::workers::Offload::engine(JobKind::Drop);
            let _ = self.offload(job, move |_| drop(old), |_, _| {});
        }
    }

    pub(crate) fn pump_save_planning(&mut self) {
        let Some(p) = self.save_planning.as_mut() else { return };
        if p.generation != self.world_generation {
            // (Every replacement flushes saves first; this is the guard if one didn't.)
            let p = self.save_planning.take().expect("checked");
            self.world_save_running = false;
            let out = ToolOutput::error(
                Json::obj([("error", Json::from("world_replaced")), ("message", Json::from("the world was replaced while this save was being planned; nothing was written: save again"))]).to_string(),
            );
            answer_all(&p.replies, &out);
            return;
        }
        let started = Instant::now();
        let trace = p.trace;
        let _c = trace.enter();
        let done = {
            let _z = profile::zone("world_save.plan");
            p.planner.step(&self.world, Some(PLAN_SLICE))
        };
        if !done {
            self.engine_work_done(trace.call, Some(crate::schedule::Class::Disk), "world_save_plan", started.elapsed());
            return;
        }
        let SavePlanning { planner, name, key, replies, dir, lock, check_exists, .. } = self.save_planning.take().expect("checked");
        let mut plan = {
            let _z = profile::zone("world_save.plan");
            planner.finish(&self.world)
        };
        plan.need = disk::Need::Agent;
        self.write_world_save(plan, (name, key, replies, dir, lock, check_exists));
        self.engine_work_done(trace.call, Some(crate::schedule::Class::Disk), "world_save_plan", started.elapsed());
    }

    /// A planned save, rendered and written on the worker; `then` answers.
    #[allow(clippy::type_complexity)]
    fn write_world_save(&mut self, plan: persist::Plan, ctx: (String, String, Vec<jobs::Reply>, PathBuf, Option<persist::WorldLock>, bool)) {
        let (name, key, replies, dir, lock, check_exists) = ctx;
        let board = self.board_write_for(&dir);
        let (revision, seq) = (self.world.revision(), self.swarm.journal().last_seq());
        let (entities, state_hash) = (self.world.persistent_len(), format!("{:016x}", self.world.state_hash()));
        self.world_save_running = true;
        let shown = name.clone();
        let work_dir = dir.clone();
        let work = move |_: &Cancel| {
            if check_exists && work_dir.join("world.json").exists() {
                // Another world lives there: never replace it by accident. (Its world.json records
                // its entity count, so this costs one small read, not a load.)
                let (entities, problem) = match persist::peek_entity_count(&work_dir) {
                    Ok(n) => (Json::from(n), Json::Null),
                    Err(e) => (Json::Null, e.to_json()),
                };
                let refusal = Json::obj([
                    ("error", Json::from("world_exists")),
                    ("message", Json::from(format!("a different world named '{shown}' already exists; pick another name, world_load it, or pass overwrite: true to replace it"))),
                    ("name", Json::from(shown.as_str())),
                    ("entities", entities),
                    ("unreadable", problem),
                ]);
                return Written { exists: Some(refusal), result: Err(persist::PersistError::new("world_exists", "not written")), board: None };
            }
            let result = {
                let _z = profile::zone("world_save.write");
                persist::execute(plan)
            };
            // The board goes beside the world once the world is there.
            let board = board.and_then(|(rev, write)| result.is_ok().then(|| (rev, write())));
            Written { exists: None, result, board }
        };
        let refused = replies.clone();
        let generation = self.world_generation;
        let then = move |e: &mut Engine, r: Result<Written, crate::workers::Failed>| {
            e.world_save_running = false;
            let w = match r {
                Ok(w) => w,
                Err(f) => {
                    answer_all(&replies, &f.to_output(JobKind::WorldSave));
                    e.pump_world_saves();
                    return;
                }
            };
            // The backstop for every path at once: a write that comes back to a world that
            // isn't the one it was planned from names, marks and caches nothing (the files
            // hold the old world as it was). Loads refuse saves, so this shouldn't happen.
            if e.world_generation != generation {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "world_save.world_replaced", name = name.as_str());
                let out = ToolOutput::error(
                    Json::obj([
                        ("error", Json::from("world_replaced")),
                        ("message", Json::from("the world was replaced while this save was being written: the files hold the world as it was when the save began, and the live world was left as it is; save again if you meant the live one")),
                    ])
                    .to_string(),
                );
                answer_all(&replies, &out);
                e.pump_world_saves();
                return;
            }
            if let Some(refusal) = w.exists {
                answer_all(&replies, &ToolOutput::error(refusal.to_string()));
                e.pump_world_saves();
                return;
            }
            let board_error = w.board.map(|(rev, result)| e.board_written(rev, result)).unwrap_or_else(|| e.board_io.error.clone());
            match w.result {
                Ok((saved, report)) => {
                    if lock.is_some() {
                        e.world_lock = lock;
                    }
                    e.replace_saved(saved);
                    let old_root = e.evidence_root();
                    e.world_name = Some(name.clone());
                    // What was taken off the world when the save began: later changes stay unsaved.
                    e.saved_revision = revision;
                    e.saved_seq = seq;
                    e.last_autosave = Instant::now();
                    e.autosave_error = None;
                    e.note_leftovers(&report);
                    // A world just named (or saved under another name) takes the queue's evidence along.
                    e.carry_evidence(&old_root, &dir);
                    // Who made each world folder this run (at most a few hundred names).
                    if e.world_folders.len() < 512 || e.world_folders.contains_key(&name) {
                        e.world_folders.entry(name.clone()).or_insert(key);
                    }
                    let out = ToolOutput::text(
                        Json::obj([
                            ("saved", Json::from(name)),
                            ("board_save_error", board_error.unwrap_or(Json::Null)),
                            ("dir", Json::from(report.dir.display().to_string())),
                            ("generation", Json::from(report.generation)),
                            ("files", Json::from(report.files)),
                            ("files_written", Json::from(report.written)),
                            ("files_removed", Json::from(report.removed)),
                            ("leftover_files", Json::arr(report.leftovers.iter().map(|f| Json::from(f.as_str())))),
                            // Transient entities are never saved.
                            ("entities", Json::from(entities)),
                            ("state_hash", Json::from(state_hash)),
                        ])
                        .to_string(),
                    );
                    answer_all(&replies, &out);
                }
                Err(err) => {
                    if err.invalidates_cache() {
                        e.saved = None;
                    }
                    answer_all(&replies, &ToolOutput::error(err.to_json().to_string()));
                }
            }
            // The next waiting save (or autosave) may go.
            e.pump_world_saves();
        };
        let job = Offload::engine(JobKind::WorldSave).lane(Lane::World).class(crate::schedule::Class::Disk);
        if let Err(e) = self.offload(job, work, then) {
            self.world_save_running = false;
            for r in &refused {
                r.refuse(&e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Writer;

    /// An engine whose live world autosaves to `worlds/A` in a scratch folder (what `--world A`
    /// sets up). `None`: no Vulkan here; said, and skipped.
    fn autosaving(tag: &str) -> Option<(Engine, std::path::PathBuf)> {
        if !specula_gpu::is_available() {
            crate::skip_test(tag, "Vulkan is not available");
            return None;
        }
        let Ok(gpu) = specula_gpu::Gpu::new(&specula_gpu::GpuOptions::default()) else {
            crate::skip_test(tag, "no GPU");
            return None;
        };
        let root = std::env::temp_dir().join(format!("specula-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let mut e = Engine::new(gpu, &root.join("captures"), crate::models::Library::new(&assets, None)).expect("engine");
        e.worlds_dir = root.join("worlds");
        e.world_name = Some("A".into());
        Some((e, root))
    }

    fn cube(e: &mut Engine, name: &str) {
        let op = crate::world::Op::Spawn(crate::tools::parse_spawn(&Json::parse(&format!(r#"{{"name": "{name}", "shape": "cube"}}"#)).unwrap()).unwrap());
        let w = Writer::new("t", Some("t"));
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Batch, crate::write_policy::Rule::World, |wd| wd.apply_batch(vec![op], "t").map(|_| ()).map_err(|(_, err)| err)).unwrap();
    }

    /// The autosave's turn comes (the loop's own call).
    fn due(e: &mut Engine) {
        e.last_autosave = Instant::now() - crate::AUTOSAVE_EVERY * 2;
        e.autosave_if_due();
    }

    /// Lets an autosave's worker finish, and collects it as the loop does.
    fn collect(e: &mut Engine) {
        for _ in 0..6000 {
            if e.autosave_job.is_none() {
                return;
            }
            e.workers_drain(Duration::from_millis(10));
            e.pump_workers(None);
            e.finish_autosave(false);
        }
        panic!("the autosave never finished");
    }

    fn on_disk(e: &Engine) -> Option<u64> {
        crate::persist::peek_entity_count(&e.worlds_dir.join("A")).unwrap()
    }

    /// The autosave writes a changed world and marks it saved; with nothing changed it does
    /// nothing. A change made **while it writes** is not in the files and stays unsaved: the
    /// job marks the revision it planned from, not the one the world has when it comes back
    /// (the test never asks whether the worker has finished yet, so it holds whenever it runs).
    #[test]
    fn an_autosave_marks_what_it_planned_and_later_changes_stay_unsaved() {
        let Some((mut e, root)) = autosaving("autosave-marks") else { return };
        cube(&mut e, "a");
        assert!(e.dirty());
        let planned = e.world.revision();
        due(&mut e);
        // Changed again while (or just after) it writes.
        cube(&mut e, "b");
        collect(&mut e);
        assert_eq!(e.saved_revision, planned, "it marks the revision it wrote");
        assert!(e.dirty(), "the later change is unsaved");
        assert_eq!(on_disk(&e), Some(1), "and is not in the files");
        assert!(e.autosave_error.is_none());
        // Its next turn writes the rest.
        due(&mut e);
        collect(&mut e);
        assert!(!e.dirty());
        assert_eq!(on_disk(&e), Some(2));
        // Nothing changed: no job, and its turn is taken (the timer starts again).
        let before = Instant::now();
        due(&mut e);
        assert!(e.autosave_job.is_none(), "an unchanged world is not written");
        assert!(e.last_autosave >= before, "the timer starts from this turn");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// When it doesn't run: no world to save to, an explicit save writing the same folder, or
    /// an autosave already in flight (one write of the folder at a time).
    #[test]
    fn an_autosave_skips_its_turn_when_it_has_nowhere_to_write_or_a_write_is_running() {
        let Some((mut e, root)) = autosaving("autosave-skips") else { return };
        cube(&mut e, "a");
        e.world_name = None;
        due(&mut e);
        assert!(e.autosave_job.is_none() && e.dirty(), "a world with no name is not autosaved");
        assert!(!e.worlds_dir.exists(), "nothing was written anywhere");
        e.world_name = Some("A".into());
        e.world_save_running = true;
        due(&mut e);
        assert!(e.autosave_job.is_none() && e.dirty(), "not beside an explicit save");
        e.world_save_running = false;
        // One in flight (held open here: its worker hasn't reported): no second one.
        let (tx, done) = std::sync::mpsc::channel();
        e.autosave_job = Some(crate::AutosaveJob { revision: 0, seq: 0, generation: e.world_generation, done });
        due(&mut e);
        assert!(e.autosave_job.as_ref().is_some_and(|j| j.revision == 0), "the one in flight is kept, and none is started beside it");
        assert_eq!(on_disk(&e), None, "nothing was written");
        drop(tx);
        e.autosave_job = None;
        due(&mut e);
        collect(&mut e);
        assert!(!e.dirty() && on_disk(&e) == Some(1), "and it writes on its next turn");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The backstop behind "no autosave beside a load" (review R2 of m5-gpuwork: it had no
    /// test): an autosave that comes back to a world that isn't the one it was planned from
    /// marks and caches nothing. The new world's revision counter and saved state are its own;
    /// marking them from the old world's job would call unsaved changes saved.
    #[test]
    fn an_autosave_that_comes_back_to_a_replaced_world_marks_nothing() {
        let Some((mut e, root)) = autosaving("autosave-replaced") else { return };
        cube(&mut e, "a");
        // The job's own work, done here so that its result is in hand before the world changes
        // under it (whatever a worker's timing).
        let result = crate::persist::execute(crate::persist::plan(&e.world, &e.worlds_dir.join("A"), None));
        assert!(result.is_ok(), "the write itself succeeded");
        let (tx, done) = std::sync::mpsc::channel();
        tx.send(result).unwrap();
        e.autosave_job = Some(crate::AutosaveJob { revision: e.world.revision(), seq: e.swarm.journal().last_seq(), generation: e.world_generation, done });
        let (revision, seq) = (e.saved_revision, e.saved_seq);
        // Another world takes its place while it writes.
        e.world_generation += 1;
        e.finish_autosave(false);
        assert!(e.autosave_job.is_none(), "collected");
        assert_eq!((e.saved_revision, e.saved_seq), (revision, seq), "the live world is not marked saved");
        assert!(e.saved.is_none(), "and the old world's files are not its cache");
        assert!(e.dirty() && e.autosave_error.is_none());
        // The same result for the world it was planned from is taken.
        let result = crate::persist::execute(crate::persist::plan(&e.world, &e.worlds_dir.join("A"), None));
        let (tx, done) = std::sync::mpsc::channel();
        tx.send(result).unwrap();
        e.autosave_job = Some(crate::AutosaveJob { revision: e.world.revision(), seq: e.swarm.journal().last_seq(), generation: e.world_generation, done });
        e.finish_autosave(false);
        assert!(!e.dirty() && e.saved.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A failed autosave is kept as an error with the revision it was writing (`engine_info`'s
    /// `last_autosave_error`), leaves the world unsaved, and is tried again on its next turn;
    /// the next success clears it. So is a worker that stopped without reporting.
    #[test]
    fn a_failed_autosave_is_reported_leaves_the_world_unsaved_and_is_tried_again() {
        let Some((mut e, root)) = autosaving("autosave-fails") else { return };
        cube(&mut e, "a");
        // The world's folder can't be made: a file has its name.
        std::fs::create_dir_all(&e.worlds_dir).unwrap();
        std::fs::write(e.worlds_dir.join("A"), b"not a folder").unwrap();
        let revision = e.world.revision();
        due(&mut e);
        collect(&mut e);
        let error = e.autosave_error.clone().expect("the failure is kept");
        assert_eq!(error.get("at_revision").and_then(Json::as_u64), Some(revision), "{error}");
        assert!(error.get("error").and_then(Json::as_str).is_some(), "it has a code: {error}");
        assert!(e.dirty(), "nothing is marked saved");
        let traced = specula_core::trace::read(|r| r.iter().rev().any(|ev| ev.code == "autosave.failed" && ev.field("revision").and_then(|v| v.as_u64()) == Some(revision)));
        assert!(traced, "a failed autosave is a trace event (agents debug from traces)");
        // The cause goes away: its next turn writes, and the error is cleared.
        std::fs::remove_file(e.worlds_dir.join("A")).unwrap();
        due(&mut e);
        collect(&mut e);
        assert!(!e.dirty() && e.autosave_error.is_none() && on_disk(&e) == Some(1));
        // A worker that stopped without reporting (it panicked): an error, not a wait for ever.
        cube(&mut e, "b");
        let (tx, done) = std::sync::mpsc::channel();
        drop(tx);
        e.autosave_job = Some(crate::AutosaveJob { revision: e.world.revision(), seq: 0, generation: e.world_generation, done });
        e.finish_autosave(false);
        assert!(e.autosave_job.is_none());
        let error = e.autosave_error.clone().expect("kept");
        assert!(error.to_string().contains("stopped without reporting"), "{error}");
        assert!(e.dirty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// At shutdown the autosave writes before it returns (no worker: the process is ending),
    /// and collects one still in flight first.
    #[test]
    fn the_shutdown_autosave_writes_before_it_returns() {
        let Some((mut e, root)) = autosaving("autosave-shutdown") else { return };
        cube(&mut e, "a");
        due(&mut e);
        cube(&mut e, "b");
        // What the engine does on its way out.
        e.finish_autosave(true);
        e.autosave(true);
        assert!(e.autosave_job.is_none() && !e.dirty(), "everything is written when it returns");
        assert_eq!(on_disk(&e), Some(2));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The backstop behind "a load refuses saves": a write that comes back to a world that isn't
    /// the one it was planned from names, marks and caches nothing (review R2 of m5-gpuwork: it
    /// had no test). Skipped cleanly without Vulkan.
    #[test]
    fn a_write_that_comes_back_to_a_replaced_world_names_and_marks_nothing() {
        if !specula_gpu::is_available() {
            crate::skip_test("world_save::tests::written_replaced", "Vulkan is not available");
            return;
        }
        let Ok(gpu) = specula_gpu::Gpu::new(&specula_gpu::GpuOptions::default()) else {
            crate::skip_test("world_save::tests::written_replaced", "no GPU");
            return;
        };
        let root = std::env::temp_dir().join(format!("specula-save-written-replaced-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let mut e = Engine::new(gpu, &root.join("captures"), crate::models::Library::new(&assets, None)).expect("engine");
        e.worlds_dir = root.join("worlds");
        let w = Writer::new("t", Some("t"));
        let op = crate::world::Op::Spawn(crate::tools::parse_spawn(&Json::parse(r#"{"name": "c", "shape": "cube"}"#).unwrap()).unwrap());
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Batch, crate::write_policy::Rule::World, |wd| wd.apply_batch(vec![op], "t").map(|_| ()).map_err(|(_, err)| err)).unwrap();
        let revision = e.saved_revision;
        let session = specula_protocol::SessionInfo { id: "save-test".into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() };
        let (reply, rx) = jobs::channel();
        let deadline = Instant::now() + Duration::from_secs(60);
        e.handle_world_save(Request { session, call: 1, command: Command::Save { name: Some("small".into()), overwrite: false }, reply, deadline, trace: specula_core::trace::Ctx::current(), via: None });
        // Planned, and its write on a worker (nothing comes back until the workers are pumped).
        for _ in 0..1000 {
            if e.save_planning.is_none() {
                break;
            }
            e.pump_save_planning();
        }
        assert!(e.save_planning.is_none() && e.world_save_running, "the write is on its worker");
        // Another world takes its place while it writes (what a load would do, did it not
        // refuse saves).
        e.world_generation += 1;
        let out = loop {
            if let Ok(out) = rx.try_recv() {
                break out;
            }
            e.workers_drain(Duration::from_millis(10));
            e.pump_workers(None);
        };
        assert!(out.is_error && format!("{:?}", out.content).contains("world_replaced"), "{:?}", out.content);
        assert!(e.world_name.is_none(), "the live world is not named after the save");
        assert_eq!(e.saved_revision, revision, "and is not marked saved");
        assert!(!e.world_write_busy());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review of 07f3276: a world that took the live world's place while a `world_save` of it
    /// was being planned in slices was saved under the old world's name. A plan whose world was
    /// replaced is answered `world_replaced` and writes nothing (`save_read` and `world_load`
    /// flush planned saves before they replace the world). Skipped cleanly without Vulkan.
    #[test]
    fn a_save_planned_across_a_world_replacement_writes_nothing() {
        if !specula_gpu::is_available() {
            crate::skip_test("world_save::tests::replaced", "Vulkan is not available");
            return;
        }
        let Ok(gpu) = specula_gpu::Gpu::new(&specula_gpu::GpuOptions::default()) else {
            crate::skip_test("world_save::tests::replaced", "no GPU");
            return;
        };
        let root = std::env::temp_dir().join(format!("specula-save-replaced-{}", std::process::id()));
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let mut e = Engine::new(gpu, &root.join("captures"), crate::models::Library::new(&assets, None)).expect("engine");
        e.worlds_dir = root.join("worlds");
        let w = Writer::new("t", Some("t"));
        for b in 0..20 {
            let ops: Vec<crate::world::Op> = (0..1000)
                .map(|i| crate::world::Op::Spawn(crate::tools::parse_spawn(&Json::parse(&format!(r#"{{"name": "c{b}_{i}", "shape": "cube"}}"#)).unwrap()).unwrap()))
                .collect();
            e.world.commit(&mut e.swarm, 0, w, journal::Kind::Batch, crate::write_policy::Rule::World, |wd| wd.apply_batch(ops, "t").map(|_| ()).map_err(|(_, err)| err)).unwrap();
        }
        let session = specula_protocol::SessionInfo { id: "save-test".into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() };
        let (reply, rx) = jobs::channel();
        let deadline = Instant::now() + Duration::from_secs(60);
        e.handle_world_save(Request { session, call: 1, command: Command::Save { name: Some("big".into()), overwrite: false }, reply, deadline, trace: specula_core::trace::Ctx::current(), via: None });
        assert!(e.save_planning.is_some(), "20,000 entities are planned over several passes");
        // Another world takes its place mid-plan (what a load does, minus the flush).
        e.world_generation += 1;
        e.pump_world_saves();
        let out = rx.recv_timeout(Duration::from_secs(10)).expect("answered");
        assert!(out.is_error && format!("{:?}", out.content).contains("world_replaced"), "{:?}", out.content);
        assert!(!e.world_write_busy() && e.save_planning.is_none());
        assert!(!e.worlds_dir.join("big").join("world.json").exists(), "nothing written");
        let _ = std::fs::remove_dir_all(&root);
    }
}
