//! `world_load` off the engine thread (M5 gpuwork; `docs/design/WORKERS.md`, "world_load").
//!
//! A load used to read and parse the world's files, write the outgoing world's task board and
//! read the new one's, all in the call: 112-250 ms on the engine thread for a world of 5,000
//! cubes on the reference iGPU (zones: `world_load.read` 99 ms, `board.save` 38 ms,
//! `world_load.board` 15 ms, `world_load.replace` 25 ms). Now the call checks what it may do
//! (the write policy, force, unsaved changes) and hands the disk work to a worker in the world
//! files' lane (`Lane::World`: after any save of the outgoing world, before any later one):
//!
//! 1. the outgoing board and review queue, rendered on the engine thread, are written;
//! 2. the world's files are read and parsed (`persist::load_limited`);
//! 3. the new world's `board.json` and `review.json` are read.
//!
//! Its continuation checks the write policy and unsaved changes again (agents may have written
//! meanwhile) and replaces the world as before. Saves asked for before the load are written first:
//! a load waits (parked, a loop pass at a time, until its request's deadline) while the world's
//! folder is being written. One load at a time; another is refused with `job_busy`, naming the
//! loader, and `retry`.
//!
//! **A load is a world write.** From the moment it is accepted until it has answered, nothing
//! else may write a world folder: a `world_save` is refused (`job_busy`, the loader named), and
//! the autosave and the board's autosave skip their turn ([`Engine::load_busy`]). Review R1 of
//! m5-gpuwork: a save that arrived while the load read was written after it and named the
//! loaded world after the one it replaced, so the next save wrote the new world over the old
//! world's folder; and an autosave in that window saved the changes `force` discards.

use crate::journal::Writer;
use crate::persist;
use crate::tools::{Command, Request};
use crate::workers::{JobKind, Lane, Offload};
use crate::world::World;
use crate::{authority, write_policy, Engine};
use specula_core::Json;
use specula_protocol::ToolOutput;
use std::path::PathBuf;

/// A load parked until the world's folder is no longer being written, or on the worker.
pub(crate) enum Loading {
    Parked(Box<Request>),
    /// On the worker; `owner` is the agent that asked.
    Running { owner: String },
}

/// What the worker reads and writes for a load.
struct Disk {
    read: Result<(World, persist::Saved), persist::PersistError>,
    board: std::io::Result<String>,
    review: std::io::Result<String>,
    /// The outgoing board's write (`None`: there was none to write).
    board_written: Option<crate::swarm_engine::WriteResult>,
}

/// The outgoing world's board and review queue, rendered for the worker to write.
struct BoardOut {
    dir: PathBuf,
    text: String,
    review: Option<String>,
    revision: u64,
}

fn unsaved(current: Option<&str>, board_unsaved: bool, entities: usize) -> ToolOutput {
    ToolOutput::error(
        Json::obj([
            ("error", Json::from("unsaved_changes")),
            ("message", Json::from("the live world (or its task board) has unsaved changes; world_save first, or pass force: true to discard them")),
            ("current_world", current.map_or(Json::Null, Json::from)),
            ("board_unsaved", Json::from(board_unsaved)),
            ("entities", Json::from(entities)),
        ])
        .to_pretty(),
    )
}

impl Engine {
    /// The refusal for `what` while a load is waiting or reading (`None`: no load): `job_busy`
    /// with the loader as `owner` (AGENTS rule 14) and `retry`.
    pub(crate) fn load_busy(&self, what: &str) -> Option<crate::world::WorldError> {
        let owner = match self.loading.as_ref()? {
            Loading::Parked(r) => self.swarm.agent_name(&r.session),
            Loading::Running { owner } => owner.clone(),
        };
        Some(
            authority::refusal(authority::Site::JobBusy, format!("{owner}'s world_load is in progress ({what}): call again when it has answered"))
                .with("job", "world_load")
                .with("owner", owner.as_str())
                .with("retry_after_s", 1)
                .with("retry", true),
        )
    }

    /// `world_load`: checked here, read on a worker, applied when it is back (module docs).
    pub(crate) fn handle_world_load(&mut self, request: Request) {
        if let Some(e) = self.load_busy("one load at a time") {
            request.reply.refuse(&e);
            return;
        }
        // Nor beside a save game's read, which replaces the world too. (Review R3 of
        // m5-gpuwork: a load accepted here won, and the save's read, already made, was thrown
        // away at its swap-in. The read refuses while a load is in progress; so does the load
        // while a read is.)
        if let Some(e) = self.save_read_busy() {
            request.reply.refuse(&e);
            return;
        }
        if !self.load_checks(&request) {
            return;
        }
        if self.world_write_busy() || !self.waiting_saves.is_empty() {
            // Saves asked for before this load are of the world it replaces: written first.
            self.loading = Some(Loading::Parked(Box::new(request)));
            return;
        }
        self.start_world_load(request);
    }

    /// The checks a load makes before (and again after) reading: the write policy (playtests,
    /// others' claims) and force's authority. False when it answered (refused).
    fn load_checks(&mut self, request: &Request) -> bool {
        let Command::Load { force, .. } = request.command else { return false };
        let by = self.swarm.agent_name(&request.session);
        let session = self.swarm.key(&request.session);
        let w = Writer::new(&by, Some(&session)).via(request.via.as_deref());
        match write_policy::may_replace(&self.swarm, w, &self.playtests) {
            // Force discards every agent's unsaved changes (coordinator, 2026-10-29): the
            // lead's call; with nothing unsaved it changes nothing.
            Ok(_) if force && self.dirty() && authority::authority(&self.swarm, &request.session.id) < authority::Authority::Lead && self.others_unsaved(&session).is_some() => {
                let e = authority::may_force(authority::authority(&self.swarm, &request.session.id), false, "world_load {force} (it discards another agent's unsaved changes)")
                    .expect_err("not the lead")
                    .with("unsaved_by", self.others_unsaved(&session).unwrap_or_default());
                let _ = request.reply.send(ToolOutput::error(e.to_json().to_pretty()));
                false
            }
            Ok(_) => true,
            Err(e) => {
                let _ = request.reply.send(ToolOutput::error(e.to_json().to_pretty()));
                false
            }
        }
    }

    /// A parked load starts once the world's folder is no longer being written (a loop pass).
    pub(crate) fn pump_world_load(&mut self) {
        let Some(Loading::Parked(parked)) = &self.loading else { return };
        let late = std::time::Instant::now() > parked.deadline;
        if self.world_write_busy() || !self.waiting_saves.is_empty() {
            // The saves before it are still being written: it waits, but not past its deadline.
            if late {
                let Some(Loading::Parked(request)) = self.loading.take() else { return };
                let out = ToolOutput::error(
                    Json::obj([
                        ("error", Json::from("engine_busy")),
                        ("message", Json::from("skipped: the load waited too long for the saves before it; nothing was loaded: call again")),
                        ("retry", Json::Bool(true)),
                    ])
                    .to_string(),
                );
                let _ = request.reply.send(out);
            }
            return;
        }
        let Some(Loading::Parked(request)) = self.loading.take() else { return };
        if self.load_checks(&request) {
            self.start_world_load(*request);
        }
    }

    fn start_world_load(&mut self, request: Request) {
        let Command::Load { ref name, force } = request.command else { return };
        let name = name.clone();
        // A board write running on the board's own thread finishes first (it is short).
        self.finish_board_save(true);
        let board_out = self.board_dir().map(|dir| {
            let _z = specula_core::profile::zone("board.render");
            let text = self.swarm.board.render_cached();
            let review = self.review_text(&dir);
            BoardOut { dir, text, review, revision: self.board_revision() }
        });
        // Unsaved: the world, or a board with nowhere to go (written on the worker otherwise).
        let board_unsaved = board_out.is_none() && self.board_unsaved();
        if (self.dirty() || board_unsaved) && !force {
            let _ = request.reply.send(unsaved(self.world_name.as_deref(), board_unsaved, self.world.len()));
            return;
        }
        let dir = self.worlds_dir.join(&name);
        let lock = match self.lock_for(&dir) {
            Ok(lock) => lock,
            Err(e) => {
                let _ = request.reply.send(ToolOutput::error(e.to_json().to_pretty()));
                return;
            }
        };
        let limits = self.world.shape_limits();
        let read_dir = dir.clone();
        let work = move |_: &crate::workers::Cancel| {
            let board_written = board_out.map(|b| {
                let _z = specula_core::profile::zone("board.write");
                let r = crate::swarm_engine::write_board(&b.dir, &b.text).and_then(|()| b.review.map_or(Ok(()), |r| crate::review_engine::write_review(&b.dir, &r)));
                (b.revision, r.map_err(|e| (b.dir.display().to_string(), e.to_string())))
            });
            let read = {
                let _z = specula_core::profile::zone("world_load.read");
                persist::load_limited(&read_dir, limits)
            };
            let board = std::fs::read_to_string(read_dir.join(crate::swarm_engine::BOARD_FILE));
            let review = std::fs::read_to_string(read_dir.join(crate::review_engine::REVIEW_FILE));
            (Disk { read, board, review, board_written: board_written.as_ref().map(|(_, r)| r.clone()) }, board_written.map(|(rev, _)| rev))
        };
        self.loading = Some(Loading::Running { owner: self.swarm.agent_name(&request.session) });
        let job = Offload::engine(JobKind::WorldLoad).lane(Lane::World).class(crate::schedule::Class::Disk);
        let offloaded = self.offload(job, work, move |engine, r| {
            engine.loading = None;
            match r {
                Ok((disk, revision)) => engine.finish_world_load(request, name, force, dir, lock, disk, revision),
                Err(f) => {
                    let _ = request.reply.send(f.to_output(JobKind::WorldLoad));
                }
            }
        });
        if let Err(e) = offloaded {
            self.loading = None;
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "world_load.not_offloaded", error = e.message.clone());
        }
    }

    /// The load's continuation (engine thread): the checks again, then the world replaced.
    #[allow(clippy::too_many_arguments)]
    fn finish_world_load(&mut self, request: Request, name: String, force: bool, dir: PathBuf, lock: Option<persist::WorldLock>, disk: Disk, revision: Option<u64>) {
        if let (Some(written), Some(revision)) = (disk.board_written.clone(), revision) {
            self.board_io.note(revision, written);
        }
        // Agents may have written meanwhile, a playtest started or claims been taken.
        if !self.load_checks(&request) {
            return;
        }
        let board_unsaved = self.board_unsaved();
        if (self.dirty() || board_unsaved) && !force {
            let _ = request.reply.send(unsaved(self.world_name.as_deref(), board_unsaved, self.world.len()));
            return;
        }
        let by = self.swarm.agent_name(&request.session);
        let session = self.swarm.key(&request.session);
        let w = Writer::new(&by, Some(&session)).via(request.via.as_deref());
        let may = match write_policy::may_replace(&self.swarm, w, &self.playtests) {
            Ok(may) => may,
            Err(e) => {
                let _ = request.reply.send(ToolOutput::error(e.to_json().to_pretty()));
                return;
            }
        };
        let out = match disk.read {
            Ok((mut world, saved)) => {
                if lock.is_some() {
                    self.world_lock = lock;
                }
                // Live Lex runs presented into the outgoing world, and world files hold no runs:
                // they stop, and what the files kept of /lex_live (nothing would update it) goes.
                let removed = crate::lex_live::remove_presentation(&mut world);
                let stopped = self.lex_live.stop_all();
                let zr = specula_core::profile::zone("world_load.replace");
                let mut problems = self.replace_world(may, world, &name);
                drop(zr);
                problems.extend(saved.warnings.iter().cloned());
                let zb = specula_core::profile::zone("world_load.board");
                let board_problem = self.load_board_read(&dir, disk.board, disk.review);
                drop(zb);
                let had_problem = board_problem.is_some();
                problems.extend(board_problem);
                // The loaded queue's evidence lives in the world's folder (older queues point into captures).
                let (_, missing) = self.adopt_evidence(&dir, had_problem);
                if missing > 0 {
                    problems.push(format!("{missing} file(s) the review queue names are missing from its evidence (review_list marks them missing)"));
                }
                self.replace_saved(saved);
                self.world_name = Some(name.clone());
                self.saved_revision = self.world.revision();
                self.saved_seq = self.swarm.journal().last_seq();
                self.last_autosave = std::time::Instant::now();
                let mut out = vec![
                    ("loaded", Json::from(name)),
                    ("dir", Json::from(dir.display().to_string())),
                    ("entities", Json::from(self.world.len())),
                    ("state_hash", Json::from(format!("{:016x}", self.world.state_hash()))),
                    ("warnings", Json::arr(problems.into_iter().take(20).map(Json::from))),
                ];
                if !stopped.is_empty() || removed > 0 {
                    out.push((
                        "lex_live",
                        Json::obj([
                            ("stopped", Json::arr(stopped.into_iter().map(Json::from))),
                            ("presentation_removed", Json::from(removed)),
                            ("note", Json::from("world files hold no live Lex runs: runs live before the load stopped, and /lex_live entities in the files were removed; lex_live {action: start} runs a set again (save_write / save_read keep runs)")),
                        ]),
                    ));
                }
                ToolOutput::text(Json::obj(out).to_pretty())
            }
            Err(e) => ToolOutput::error(e.to_json().to_pretty()),
        };
        let _ = request.reply.send(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{jobs, journal};
    use std::time::{Duration, Instant};

    /// An engine with its worlds in a scratch folder (`None`: no Vulkan here; said, and skipped).
    fn engine(tag: &str) -> Option<(Engine, PathBuf)> {
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
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let mut e = Engine::new(gpu, &root.join("captures"), crate::models::Library::new(&assets, None)).expect("engine");
        e.worlds_dir = root.join("worlds");
        Some((e, root))
    }

    fn session(id: &str) -> specula_protocol::SessionInfo {
        specula_protocol::SessionInfo { id: id.into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() }
    }

    fn request(by: &str, command: Command, reply: jobs::Reply) -> Request {
        Request { session: session(by), call: 1, command, reply, deadline: Instant::now() + Duration::from_secs(60), trace: specula_core::trace::Ctx::current(), via: None }
    }

    /// One cube named `name`, written as `by` (so it is that session's own unsaved change).
    fn spawn(e: &mut Engine, by: &str, name: &str) {
        let op = crate::world::Op::Spawn(crate::tools::parse_spawn(&Json::parse(&format!(r#"{{"name": "{name}", "shape": "cube"}}"#)).unwrap()).unwrap());
        let w = Writer::new(by, Some(by));
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Batch, write_policy::Rule::World, |wd| wd.apply_batch(vec![op], by).map(|_| ()).map_err(|(_, err)| err)).unwrap();
    }

    /// Runs the loop's own steps until `rx` is answered.
    fn settle(e: &mut Engine, rx: &std::sync::mpsc::Receiver<ToolOutput>) -> ToolOutput {
        for _ in 0..3000 {
            if let Ok(out) = rx.try_recv() {
                return out;
            }
            e.pump_world_saves();
            e.workers_drain(Duration::from_millis(10));
            e.pump_workers(None);
        }
        panic!("no answer");
    }

    fn save(e: &mut Engine, by: &str, name: Option<&str>) -> ToolOutput {
        let (reply, rx) = jobs::channel();
        e.handle_world_save(request(by, Command::Save { name: name.map(str::to_owned), overwrite: false }, reply));
        settle(e, &rx)
    }

    fn load(e: &mut Engine, by: &str, name: &str, force: bool) -> ToolOutput {
        let (reply, rx) = jobs::channel();
        e.handle_world_load(request(by, Command::Load { name: name.into(), force }, reply));
        settle(e, &rx)
    }

    fn text(out: &ToolOutput) -> String {
        format!("{:?}", out.content)
    }

    fn on_disk(e: &Engine, world: &str) -> Option<u64> {
        persist::peek_entity_count(&e.worlds_dir.join(world)).unwrap()
    }

    /// Two saved worlds, X (one cube) and Y (two), with X loaded. `None`: skipped.
    fn two_worlds(tag: &str) -> Option<(Engine, PathBuf, u64, u64)> {
        let (mut e, root) = engine(tag)?;
        spawn(&mut e, "lee", "a");
        let out = save(&mut e, "lee", Some("X"));
        assert!(!out.is_error, "{}", text(&out));
        // (A second world from the same engine: forget the name, so the save names a new one.)
        e.world_name = None;
        spawn(&mut e, "lee", "b");
        let out = save(&mut e, "lee", Some("Y"));
        assert!(!out.is_error, "{}", text(&out));
        let out = load(&mut e, "lee", "X", false);
        assert!(!out.is_error && e.world_name.as_deref() == Some("X"), "{}", text(&out));
        let (x, y) = (on_disk(&e, "X").expect("X is saved"), on_disk(&e, "Y").expect("Y is saved"));
        assert_eq!(y, x + 1);
        Some((e, root, x, y))
    }

    /// Review R1 of m5-gpuwork, the blocker: a `world_save` that arrived while a `world_load`
    /// read was written after it and named the loaded world after the one it replaced; the next
    /// save then wrote the new world over the old world's folder. A load is a world write: a
    /// save during it is refused, naming the loader (and so is a second load: M3).
    #[test]
    fn a_save_during_a_load_is_refused_and_never_renames_the_loaded_world() {
        let Some((mut e, root, x, y)) = two_worlds("load-save-race") else { return };
        let (reply, loading) = jobs::channel();
        e.handle_world_load(request("lee", Command::Load { name: "Y".into(), force: false }, reply));
        assert!(e.loading.is_some(), "the load is on its worker");
        // Bob's save of "the current world" arrives while it reads.
        let (reply, saved) = jobs::channel();
        e.handle_world_save(request("bob", Command::Save { name: None, overwrite: false }, reply));
        let out = saved.try_recv().expect("answered at once");
        assert!(out.is_error && text(&out).contains("job_busy") && text(&out).contains("world_load") && text(&out).contains("lee") && text(&out).contains("retry"), "{}", text(&out));
        // So is a second load, with the loader named (AGENTS rule 14).
        let (reply, second) = jobs::channel();
        e.handle_world_load(request("bob", Command::Load { name: "Y".into(), force: false }, reply));
        let out = second.try_recv().expect("answered at once");
        assert!(out.is_error && text(&out).contains("job_busy") && text(&out).contains(r#"owner"#) && text(&out).contains("lee"), "{}", text(&out));
        // The load lands: the live world is Y, by name and by content.
        let out = settle(&mut e, &loading);
        assert!(!out.is_error, "{}", text(&out));
        assert_eq!(e.world_name.as_deref(), Some("Y"));
        assert!(e.loading.is_none());
        // A save now is of Y, into Y's folder: X is as it was.
        spawn(&mut e, "bob", "c");
        let out = save(&mut e, "bob", None);
        assert!(!out.is_error && text(&out).contains(r#"Y"#), "{}", text(&out));
        assert_eq!(on_disk(&e, "X"), Some(x), "X's folder is untouched");
        assert_eq!(on_disk(&e, "Y"), Some(y + 1));
        drop(e);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R2 of m5-gpuwork (N1): a load parked behind a save answers `engine_busy` at its
    /// request's deadline, from the loop's own pass. The deadline answer was written and could
    /// never run (the pass reached a parked load only when nothing was being written), and the
    /// tests didn't see it because their helper called the load's pump itself.
    #[test]
    fn a_load_parked_behind_a_stuck_save_answers_at_its_deadline() {
        let Some((mut e, root, _, _)) = two_worlds("load-deadline") else { return };
        // A save that doesn't finish holds the world's folder (a stuck disk).
        e.world_save_running = true;
        let (reply, loading) = jobs::channel();
        let mut r = request("lee", Command::Load { name: "Y".into(), force: false }, reply);
        r.deadline = Instant::now() - Duration::from_secs(1);
        e.handle_world_load(r);
        assert!(matches!(e.loading, Some(Loading::Parked(_))), "parked behind the save");
        assert!(e.load_busy("").is_some(), "and saves are refused while it waits");
        e.pump_world_saves();
        let out = loading.try_recv().expect("answered at its deadline by the loop's pass");
        assert!(out.is_error && text(&out).contains("engine_busy") && text(&out).contains("retry"), "{}", text(&out));
        assert!(e.loading.is_none() && e.load_busy("").is_none(), "saves are accepted again");
        assert_eq!(e.world_name.as_deref(), Some("X"), "nothing was loaded");
        e.world_save_running = false;
        // Parked with time left, it starts when the save is done.
        e.world_save_running = true;
        let (reply, loading) = jobs::channel();
        e.handle_world_load(request("lee", Command::Load { name: "Y".into(), force: false }, reply));
        e.pump_world_saves();
        assert!(matches!(e.loading, Some(Loading::Parked(_))) && loading.try_recv().is_err(), "still waiting");
        e.world_save_running = false;
        let out = settle(&mut e, &loading);
        assert!(!out.is_error && e.world_name.as_deref() == Some("Y"), "{}", text(&out));
        drop(e);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R3 of m5-gpuwork (M2): and the other way round. A `world_load` asked while a save
    /// game's read is in flight is refused, naming who is reading, instead of winning and
    /// wasting the read.
    #[test]
    fn a_load_is_refused_while_a_save_games_read_is_in_progress() {
        let Some((mut e, root, _, _)) = two_worlds("load-beside-saveread") else { return };
        let (reading, _read) = jobs::channel();
        e.save_read_in_flight_for_test("slot", "bob", reading);
        let (reply, loading) = jobs::channel();
        e.handle_world_load(request("lee", Command::Load { name: "Y".into(), force: false }, reply));
        let out = loading.try_recv().expect("answered at once");
        assert!(out.is_error && text(&out).contains("job_busy") && text(&out).contains("save_read") && text(&out).contains("bob") && text(&out).contains("retry"), "{}", text(&out));
        assert!(e.loading.is_none() && e.world_name.as_deref() == Some("X"), "nothing was parked or loaded");
        // Once the read is done, a load goes.
        e.save_load = None;
        let out = load(&mut e, "lee", "Y", false);
        assert!(!out.is_error && e.world_name.as_deref() == Some("Y"), "{}", text(&out));
        drop(e);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R2 of m5-gpuwork (N2): a save game's read replaces the world too, and writes the
    /// board on the engine thread first, while a load's worker may be writing the same board
    /// file. One replacement of the world at a time: it is refused, naming the loader.
    #[test]
    fn a_save_games_read_is_refused_while_a_load_is_in_progress() {
        let Some((mut e, root, _, _)) = two_worlds("load-saveread") else { return };
        let (reply, loading) = jobs::channel();
        e.handle_world_load(request("lee", Command::Load { name: "Y".into(), force: false }, reply));
        assert!(e.loading.is_some());
        let Err(refused) = e.check_load_allowed(false, "bob", Some("bob"), None) else { panic!("a save game's read beside a load must be refused") };
        assert_eq!(refused.code, "job_busy", "{}", refused.message);
        assert!(refused.message.contains("lee") && refused.message.contains("world_load"), "{}", refused.message);
        let out = settle(&mut e, &loading);
        assert!(!out.is_error && e.world_name.as_deref() == Some("Y"), "{}", text(&out));
        assert!(e.check_load_allowed(false, "bob", Some("bob"), None).map_err(|r| r.code).err() != Some("job_busy"), "and not once it has answered");
        drop(e);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R1 of m5-gpuwork, M1: the autosave fired while a forced load read, and saved the
    /// changes `force` discards into the outgoing world's folder (the next load of that world
    /// brought them back).
    #[test]
    fn no_autosave_runs_while_a_load_replaces_the_world() {
        let Some((mut e, root, x, _)) = two_worlds("load-autosave-race") else { return };
        let before = e.world.len();
        // Lee's own unsaved changes to X, then a forced load of Y that discards them.
        for i in 0..5 {
            spawn(&mut e, "lee", &format!("d{i}"));
        }
        assert!(e.dirty());
        let (reply, loading) = jobs::channel();
        e.handle_world_load(request("lee", Command::Load { name: "Y".into(), force: true }, reply));
        assert!(e.loading.is_some());
        // The autosave's turn comes while it reads.
        e.last_autosave = Instant::now() - crate::AUTOSAVE_EVERY * 2;
        e.autosave_if_due();
        assert!(e.autosave_job.is_none(), "no autosave beside a load");
        let out = settle(&mut e, &loading);
        assert!(!out.is_error && e.world_name.as_deref() == Some("Y"), "{}", text(&out));
        e.workers_drain(Duration::from_secs(5));
        e.pump_workers(None);
        assert_eq!(on_disk(&e, "X"), Some(x), "the discarded changes were not saved into X");
        // Loaded again, X is as it was saved.
        let out = load(&mut e, "lee", "X", false);
        assert!(!out.is_error && e.world.len() == before, "{} entities, {before} before the discarded changes: {}", e.world.len(), text(&out));
        drop(e);
        let _ = std::fs::remove_dir_all(&root);
    }
}
