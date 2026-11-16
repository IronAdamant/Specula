//! The start screen's engine side (docs/design/LAUNCH.md): what its buttons ask for is done
//! here, the disk work on a worker, and the project chosen is where the engine starts again
//! once it has shut down.
//!
//! An engine opened with no project (the executable double-clicked, or `--launcher`) is a
//! plain engine with the start screen over its window. Choosing a project ends this engine
//! cleanly (as Ctrl+C does) and starts the executable again with `--project FOLDER` and the
//! same flags: a project is a working directory, a set of caches and a world, and an engine
//! has exactly one of each from start to stop. `specula open` from a terminal hands a project
//! to a start screen that is still waiting (`launch_engine::Handover`), so the window the
//! person already has open becomes that project, on its port.

use crate::launch_engine::Handover;
use crate::launch_instances as instances;
use crate::launch_list::same_path;
use crate::launch_screen::{Out, Row, StartScreen};
use crate::launch_store::{self as store, Store};
use crate::workers::{Failed, JobKind, Offload};
use crate::Engine;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

/// The start screen and what it needs.
#[derive(Default)]
pub(crate) struct Start {
    pub screen: StartScreen,
    /// The person's project list (`None`: this engine keeps none, and shows no start screen).
    store: Option<Store>,
    /// Where a new project is made.
    parent: Option<PathBuf>,
    /// The project chosen on the screen: where the executable starts again after shutdown.
    chosen: Option<PathBuf>,
    /// Shared with the watcher thread: whether this engine is still only a start screen, and
    /// the project `specula open` handed it.
    handover: Handover,
    /// The listed folders are still to be checked (on a worker, from the first frame).
    unchecked: bool,
    /// A paste into the folder field waiting for the clipboard thread (`text_window`).
    paste: Option<(Receiver<crate::text_window::ClipAnswer>, Instant)>,
    /// How the engine is asked to stop once a project is chosen (`None`: the engine's own
    /// `interrupt::request`; a test passes its own, the interrupt flag being the whole process's).
    stop: Option<fn()>,
}

/// The listed projects as the list file names them, with nothing checked: one small file of
/// the per-user folder is read (start-up, before the first frame).
pub(crate) fn listed(store: &Store) -> Vec<Row> {
    let (list, _) = store.load();
    list.projects.iter().map(|p| Row { name: p.name.clone(), path: p.path.clone(), last_opened: p.last_opened, missing: false, running: None }).collect()
}

/// The listed projects as the screen shows them: whether each folder is still there, and
/// whether another engine has it open now. A worker's job: a folder on a drive that is not
/// there (a network share, an unplugged disk) can take many seconds to say so, and that wait
/// is never the window's.
pub(crate) fn rows(store: &Store) -> Vec<Row> {
    rows_with(store, instances::serving)
}

/// [`rows`], with what counts as an engine that is serving (a pid alone is not: one that was
/// reused by another program would show a project as open; review R2-6).
fn rows_with(store: &Store, alive: fn(&instances::Instance) -> bool) -> Vec<Row> {
    let (engines, _) = instances::list(&store.dir, alive);
    let me = std::process::id();
    let mut rows = listed(store);
    for row in &mut rows {
        row.missing = !row.path.is_dir();
        row.running = engines.iter().find(|i| i.pid != me && i.project.as_ref().is_some_and(|(path, _)| same_path(path, &row.path))).map(|i| i.port);
    }
    rows
}

/// The folder typed on the screen, as a project to open: there, and with no marker that is
/// broken (a folder that is not a project yet is given its marker by the engine that opens it).
pub(crate) fn typed_folder(text: &str) -> Result<PathBuf, String> {
    let path = std::path::absolute(Path::new(text.trim())).map_err(|e| format!("{text}: {e}"))?;
    if !path.is_dir() {
        return Err(format!("There is no folder at {}.", path.display()));
    }
    if crate::launch_engine::project_locked(&path) {
        return Err(format!("{} is open in an engine now: that engine's window is the one to use.", path.display()));
    }
    store::read_marker(&path).map(|_| path)
}

fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// What a worker's failure says on the screen.
fn failed(f: &Failed) -> String {
    format!("That could not be done ({f:?}): try again.")
}

impl Start {
    /// Opens the start screen (start-up, before the first frame): reads the project list.
    /// Which folders are gone, and which another engine has open, follows from a worker.
    pub fn open(&mut self, store: Store, parent: Option<PathBuf>, handover: Handover) {
        self.screen.open(listed(&store), parent.clone(), now_s());
        self.unchecked = true;
        self.store = Some(store);
        self.parent = parent;
        self.handover = handover;
    }

    /// After shutdown: the project to start again in, and whether its window opens behind
    /// the others. Chosen on the screen: in front (the person's own choice at the window).
    /// Handed over by `specula open`: behind, as every engine a command starts, unless the
    /// command said `--front` (review R1-13).
    pub fn take_chosen(&mut self) -> Option<(PathBuf, bool)> {
        let handed = self.handover.handed.lock().ok().and_then(|mut h| h.take());
        self.chosen.take().map(|folder| (folder, false)).or(handed.map(|(folder, front)| (folder, !front)))
    }
}

impl Engine {
    /// One window frame of the start screen: the person's own input, and what its buttons
    /// asked for. While it is open nothing under it gets window input. Returns whether it was
    /// open this frame.
    pub(crate) fn launch_frame(&mut self, size: (u32, u32)) -> bool {
        let open = self.start.screen.is_open();
        // (For the watcher thread: a project is handed over only to an engine that is still
        // nothing but a start screen.)
        let entities = self.world.len();
        self.start.handover.state.store(if open && entities == 0 { Handover::IDLE } else { Handover::BUSY }, Ordering::Relaxed);
        if !open {
            self.start.paste = None;
            return false;
        }
        // The clipboard's answer to a Ctrl+V in the folder field.
        if let Some((rx, since)) = self.start.paste.take() {
            match rx.try_recv() {
                Ok(Ok(Some(text))) => self.start.screen.pasted(&text),
                Ok(Ok(None)) => self.start.screen.answered(Err("The clipboard holds no text.".into())),
                Ok(Err(why)) => self.start.screen.answered(Err(format!("The clipboard could not be read ({why})."))),
                Err(TryRecvError::Empty) if since.elapsed() < crate::text_window::CLIPBOARD_WAIT => self.start.paste = Some((rx, since)),
                Err(_) => self.start.screen.answered(Err("The clipboard did not answer.".into())),
            }
        }
        if std::mem::take(&mut self.start.unchecked) {
            self.launch_refresh();
        }
        self.start.screen.set_world_note(if entities == 0 {
            String::new()
        } else {
            format!("This engine's world holds {entities} {} in no project: opening a project leaves them behind.", if entities == 1 { "entity" } else { "entities" })
        });
        let pads = self.logic.pad_states(true).to_vec();
        let out = self.start.screen.frame(&self.input, &pads, size, crate::editor::system_fonts);
        if let Some(why) = self.start.screen.problem().map(str::to_owned) {
            // No font to draw it with: a screen nobody can see must not hold the window.
            self.start.screen.close(&why);
            return false;
        }
        self.input.clear();
        if let Some(out) = out {
            self.launch_do(out);
        }
        true
    }

    /// What a press asked for: the disk (and the clipboard) on a worker, the answer back on
    /// the engine thread.
    fn launch_do(&mut self, out: Out) {
        let job = Offload::engine(JobKind::Launch);
        let started = match out {
            // The person's own paste: through the clipboard's thread, where the input monitor
            // sees every read of it (review R1-1).
            Out::Paste => {
                match crate::text_window::paste_for_start_screen() {
                    Some(rx) => self.start.paste = Some((rx, Instant::now())),
                    None => self.start.screen.answered(Err("The clipboard could not be read.".into())),
                }
                return;
            }
            Out::Open(folder) => {
                let check = folder.clone();
                self.offload(
                    job,
                    // Checked at the moment of opening: the rows may be older than the folder's
                    // state (the list is checked on a worker after the screen opens).
                    move |_| (check.is_dir(), crate::launch_engine::project_locked(&check)),
                    move |e: &mut Engine, r: Result<(bool, bool), Failed>| match r {
                        Ok((true, false)) => e.launch_go(folder),
                        Ok((true, true)) => {
                            e.start.screen.answered(Err(format!("{} is open in an engine now: that engine's window is the one to use.", folder.display())));
                            e.launch_refresh();
                        }
                        Ok((false, _)) => {
                            e.start.screen.answered(Err(format!("{} is not there any more. Forget takes it off the list.", folder.display())));
                            e.launch_refresh();
                        }
                        Err(f) => e.start.screen.answered(Err(failed(&f))),
                    },
                )
            }
            Out::Create(name) => {
                let Some(parent) = self.start.parent.clone() else {
                    self.start.screen.answered(Err("This machine names no home folder to make projects in: 'specula new NAME --in FOLDER' says where.".into()));
                    return;
                };
                self.offload(
                    job,
                    // (A project that builds and plays, as `specula new` makes it.)
                    move |_| crate::scaffold::create(&parent, &name, crate::approved::folder().as_deref()).map(|(folder, _)| folder),
                    |e: &mut Engine, r: Result<Result<PathBuf, String>, Failed>| match r {
                        Ok(Ok(folder)) => e.launch_go(folder),
                        Ok(Err(why)) => e.start.screen.answered(Err(why)),
                        Err(f) => e.start.screen.answered(Err(failed(&f))),
                    },
                )
            }
            Out::Folder(text) => self.offload(
                job,
                move |_| typed_folder(&text),
                |e: &mut Engine, r: Result<Result<PathBuf, String>, Failed>| match r {
                    Ok(Ok(folder)) => e.launch_go(folder),
                    Ok(Err(why)) => e.start.screen.answered(Err(why)),
                    Err(f) => e.start.screen.answered(Err(failed(&f))),
                },
            ),
            Out::Forget(folder) => {
                let Some(st) = self.start.store.clone() else { return };
                self.offload(
                    job,
                    move |_| (st.forget(&folder).map_err(|e| e.to_string()), rows(&st)),
                    |e: &mut Engine, r: Result<(Result<bool, String>, Vec<Row>), Failed>| match r {
                        Ok((done, rows)) => {
                            e.start.screen.set_rows(rows);
                            e.start.screen.answered(match done {
                                Ok(_) => Ok("Taken off the list. Its folder was not touched.".into()),
                                Err(why) => Err(format!("The list could not be written ({why}).")),
                            });
                        }
                        Err(f) => e.start.screen.answered(Err(failed(&f))),
                    },
                )
            }
        };
        if started.is_err() {
            self.start.screen.answered(Err("The engine is busy: try again.".into()));
        }
    }

    /// The list is read and its folders checked, on a worker (when the screen opens, and when
    /// a folder turned out to be gone).
    fn launch_refresh(&mut self) {
        let Some(st) = self.start.store.clone() else { return };
        let _ = self.offload(Offload::engine(JobKind::Launch), move |_| rows(&st), |e: &mut Engine, r: Result<Vec<Row>, Failed>| {
            if let Ok(rows) = r {
                e.start.screen.checked(rows);
            }
        });
    }

    /// The start screen's quads for a window frame, over everything else of the engine's.
    /// Window frames only: a capture never shows a person's project list.
    pub(crate) fn launch_overlay(&mut self, size: (u32, u32), out: &mut Vec<specula_gpu::OverlayQuad>) {
        let room = specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len());
        self.start.screen.overlay(&mut self.gpu, size, room, out);
    }

    /// The project is chosen: this engine stops as on Ctrl+C, and the executable starts again
    /// in it (`run_with`'s end).
    fn launch_go(&mut self, folder: PathBuf) {
        // The person closed the screen while the folder was being checked or made: closing it
        // said "no project", and that stands (review R1-14).
        if !self.start.screen.is_open() {
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "launch.chosen_after_close");
            return;
        }
        // (No folder in the trace: the next engine says its project itself.)
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "launch.chosen");
        self.start.screen.leaving(format!("Opening {}...", folder.display()));
        self.start.chosen = Some(folder);
        (self.start.stop.unwrap_or(crate::interrupt::request))();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch_list::Project;

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-launch-window-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real engine (GPU-backed; skips cleanly without Vulkan).
    fn engine() -> Option<Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test("launch_window", "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test("launch_window", &e.to_string());
                return None;
            }
        };
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let captures = std::env::temp_dir().join(format!("specula-launch-window-tests-{}", std::process::id()));
        Some(Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine"))
    }

    static STOPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn counted_stop() {
        STOPS.fetch_add(1, Ordering::Relaxed);
    }

    fn project(path: &Path, name: &str, at: u64) -> Project {
        Project { path: path.to_owned(), name: name.to_owned(), exe: None, last_world: None, last_opened: at, version: "0".into() }
    }

    /// The whole path of a choice at the window: the screen's press, the worker's check, the
    /// engine asked to stop with the project to start again in. And what review R1 found: a
    /// hand-over waits for the first frame's answer (R1-6), a project an engine holds is not
    /// opened twice (R1-2), Close while a check is on its worker means no project (R1-14), and
    /// a paste comes back through the clipboard thread's answer (R1-1).
    #[test]
    fn a_choice_at_the_window_goes_through_a_worker_and_closing_means_no_project() {
        use specula_platform::Event;
        let Some(mut e) = engine() else { return };
        let dir = scratch("engine");
        let st = Store::new(dir.join("user"));
        let (frost, crypt) = (store::create(&dir, "frost").unwrap(), store::create(&dir, "crypt").unwrap());
        st.opened(project(&frost, "frost", 100)).unwrap();
        st.opened(project(&crypt, "crypt", 200)).unwrap();
        let size = (1280, 800);
        let enter = || vec![Event::KeyDown { code: 0x0D, repeat: false }];
        let wait = std::time::Duration::from_secs(20);
        let open = |e: &mut Engine| {
            e.start = Start::default();
            e.start.open(st.clone(), Some(dir.join("made")), Handover::default());
            e.start.stop = Some(counted_stop);
        };
        open(&mut e);
        // Before the first frame nobody has said whether this is only a start screen.
        assert_eq!(e.start.handover.state.load(Ordering::Relaxed), Handover::UNKNOWN);
        assert!(e.launch_frame(size));
        assert_eq!(e.start.handover.state.load(Ordering::Relaxed), Handover::IDLE);
        e.workers_drain(wait);
        // Enter: the project last worked on. Checked on a worker, then the engine is asked to
        // stop, with where to start again (in front: the person chose it at the window).
        e.input = enter();
        assert!(e.launch_frame(size));
        assert!(e.input.is_empty(), "the screen is modal: nothing under it hears the keys");
        assert_eq!((e.start.chosen.clone(), STOPS.load(Ordering::Relaxed)), (None, 0), "not before its folder is checked");
        e.workers_drain(wait);
        assert_eq!(STOPS.load(Ordering::Relaxed), 1);
        assert_eq!(e.start.take_chosen(), Some((crypt.clone(), false)));
        // A project an engine has open is not opened a second time, though the row had not
        // said so yet: checked at the moment of opening.
        open(&mut e);
        e.launch_frame(size);
        e.workers_drain(wait);
        if crate::launch_engine::locks_here() {
            let held = crate::launch_engine::lock(&crypt).unwrap();
            e.input = enter();
            e.launch_frame(size);
            e.workers_drain(wait);
            assert_eq!((e.start.take_chosen(), STOPS.load(Ordering::Relaxed)), (None, 1));
            assert!(e.start.screen.status_for_test().contains("is open in an engine now"), "{}", e.start.screen.status_for_test());
            drop(held);
            crate::launch_engine::forget_lock_for_test(&crypt);
        }
        // Close while the folder is being checked: closing said "no project", and that stands.
        open(&mut e);
        e.launch_frame(size);
        e.workers_drain(wait);
        e.input = enter();
        e.launch_frame(size);
        e.input = vec![Event::KeyUp { code: 0x0D }, Event::KeyDown { code: 0x1B, repeat: false }];
        e.launch_frame(size);
        assert!(!e.start.screen.is_open());
        e.workers_drain(wait);
        assert_eq!((e.start.take_chosen(), STOPS.load(Ordering::Relaxed)), (None, 1), "the check came back to a closed screen");
        // Closed: not a frame of it, and no hand-over is taken (the engine is a plain one now).
        assert!(!e.launch_frame(size));
        assert_eq!(e.start.handover.state.load(Ordering::Relaxed), Handover::BUSY);
        // A paste: the clipboard thread's answer reaches the folder field; none in time is said.
        open(&mut e);
        e.launch_frame(size);
        e.workers_drain(wait);
        e.start.screen.folder_mode_for_test();
        let (reply, rx) = crate::jobs::channel();
        e.start.paste = Some((rx, Instant::now()));
        e.launch_frame(size);
        assert!(e.start.paste.is_some(), "still waiting");
        reply.send(Ok(Some("\"D:\\Games\\crypt\"\n".to_owned())));
        e.launch_frame(size);
        assert!(e.start.paste.is_none());
        assert_eq!(e.start.screen.typed_for_test(), "D:\\Games\\crypt");
        let (_never, rx) = crate::jobs::channel::<crate::text_window::ClipAnswer>();
        e.start.paste = Some((rx, Instant::now() - crate::text_window::CLIPBOARD_WAIT - std::time::Duration::from_millis(1)));
        e.launch_frame(size);
        assert!(e.start.paste.is_none() && e.start.screen.status_for_test().contains("did not answer"), "{}", e.start.screen.status_for_test());
        // Something in the world: no longer only a start screen (a hand-over would be refused),
        // and the screen says what opening a project would leave behind.
        let w = crate::journal::Writer::new("bob", Some("bob"));
        let mut cube = crate::world::SpawnSpec::new(crate::world::Shape::Cube);
        cube.name = Some("a".into());
        e.world.commit(&mut e.swarm, 0, w, crate::journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(cube, "bob").map(|x| x.id)).unwrap();
        e.launch_frame(size);
        assert_eq!(e.start.handover.state.load(Ordering::Relaxed), Handover::BUSY);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rows_say_what_is_missing_and_what_another_engine_has_open() {
        let dir = scratch("rows");
        let st = Store::new(dir.join("user"));
        let crypt = store::create(&dir, "crypt").unwrap();
        let frost = store::create(&dir, "frost").unwrap();
        st.opened(project(&crypt, "crypt", 100)).unwrap();
        st.opened(project(&dir.join("gone"), "gone", 200)).unwrap();
        st.opened(project(&frost, "frost", 300)).unwrap();
        // Another engine (a process that is running: this test's parent has no instance file,
        // so one is written under a pid that is alive and is not this one) has crypt open.
        let other = instances::Instance { pid: 4, port: 7480, project: Some((crypt.clone(), "crypt".into())), world: None, window: true, started: 1, version: "0".into(), game: None, launcher: false };
        instances::write(&st.dir, &other).unwrap();
        // This process's own instance never marks a project as open elsewhere.
        let mine = instances::Instance { pid: std::process::id(), port: 7481, project: Some((frost.clone(), "frost".into())), ..other.clone() };
        instances::write(&st.dir, &mine).unwrap();
        // As listed (what the screen opens with, nothing checked), then as checked.
        assert_eq!(listed(&st).iter().map(|r| (r.name.as_str(), r.missing, r.running)).collect::<Vec<_>>(), [("frost", false, None), ("gone", false, None), ("crypt", false, None)]);
        // (With the real check neither file is an engine that serves: nothing is "open now".)
        assert!(rows(&st).iter().all(|r| r.running.is_none()));
        instances::write(&st.dir, &other).unwrap();
        instances::write(&st.dir, &mine).unwrap();
        let rows = rows_with(&st, |_| true);
        assert_eq!(rows.iter().map(|r| (r.name.as_str(), r.missing, r.last_opened)).collect::<Vec<_>>(), [("frost", false, 300), ("gone", true, 200), ("crypt", false, 100)]);
        assert_eq!(rows[0].running, None, "this engine's own instance is not another engine");
        assert_eq!(rows[2].running, Some(7480));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_typed_folder_must_be_there_and_must_not_hold_a_broken_marker() {
        let dir = scratch("typed");
        let plain = dir.join("plain folder");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(typed_folder(&format!("  {}  ", plain.display())), Ok(plain.clone()));
        assert!(plain.read_dir().unwrap().next().is_none(), "checking a folder writes nothing into it");
        assert!(typed_folder(&dir.join("nowhere").display().to_string()).unwrap_err().contains("There is no folder at"));
        std::fs::write(plain.join(crate::launch_list::MARKER), "{ not a marker").unwrap();
        assert!(typed_folder(&plain.display().to_string()).unwrap_err().contains("is not a project marker"));
        let made = store::create(&dir, "crypt").unwrap();
        assert_eq!(typed_folder(&made.display().to_string()), Ok(made.clone()));
        // A project an engine has open is not opened a second time.
        if crate::launch_engine::locks_here() {
            let held = crate::launch_engine::lock(&made).unwrap();
            assert!(typed_folder(&made.display().to_string()).unwrap_err().contains("is open in an engine now"));
            drop(held);
            crate::launch_engine::forget_lock_for_test(&made);
        }
        assert_eq!(typed_folder(&made.display().to_string()), Ok(made));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_project_chosen_on_the_screen_comes_before_one_handed_over() {
        let mut s = Start::default();
        assert_eq!(s.take_chosen(), None);
        // Handed over by a command: its window opens behind the others, unless `--front`.
        *s.handover.handed.lock().unwrap() = Some((PathBuf::from("D:/handed"), false));
        assert_eq!(s.take_chosen(), Some((PathBuf::from("D:/handed"), true)));
        assert_eq!(s.take_chosen(), None, "taken once");
        *s.handover.handed.lock().unwrap() = Some((PathBuf::from("D:/handed"), true));
        assert_eq!(s.take_chosen(), Some((PathBuf::from("D:/handed"), false)));
        // Chosen at the window: in front, and before anything handed over.
        *s.handover.handed.lock().unwrap() = Some((PathBuf::from("D:/handed"), false));
        s.chosen = Some(PathBuf::from("D:/chosen"));
        assert_eq!(s.take_chosen(), Some((PathBuf::from("D:/chosen"), false)));
    }
}
