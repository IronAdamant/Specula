//! The engine's own side of the launcher (docs/design/LAUNCH.md): opening a project
//! (`--project DIR`, or a working directory that is one), putting it first in the user's list,
//! and telling the machine an engine is running here (its instance file), so the command line
//! and other agents find it. Start-up and shutdown code only: never a frame.
//!
//! All of it follows `--settings`: `off` keeps nothing (every script and test), a folder keeps
//! it there, and with no flag it goes to the engine's per-user folder.

use crate::launch_instances::{self as instances, Instance};
use crate::launch_list::{Marker, Project};
use crate::launch_store::{self as store, Store};
use std::path::{Path, PathBuf};

/// A project the engine has open.
#[derive(Clone, Debug, PartialEq)]
pub struct Opened {
    /// Its folder (absolute): the engine's working directory.
    pub path: PathBuf,
    pub name: String,
    /// Its assets folder, when its marker names one that is there.
    pub assets: Option<PathBuf>,
    /// What could not be done on the way (a marker that could not be written): said once at
    /// start-up, never fatal.
    pub note: Option<String>,
}

/// Opens `dir` as a project: the engine's working directory becomes that folder (worlds,
/// captures, saves and caches are the project's), its marker is read, and a folder that has
/// none yet is given one, named after the folder. `enter`: change the working directory
/// (`--project`); false for a working directory that already is the project.
pub fn open(dir: &Path, enter: bool) -> Result<Opened, String> {
    let path = std::path::absolute(dir).map_err(|e| format!("--project {}: {e}", dir.display()))?;
    if !path.is_dir() {
        return Err(format!("--project {}: there is no such folder ('specula new NAME' makes a project; 'specula projects' lists them)", path.display()));
    }
    if enter {
        std::env::set_current_dir(&path).map_err(|e| format!("--project {}: {e}", path.display()))?;
    }
    let (marker, note) = match store::read_marker(&path)? {
        Some(m) => (m, None),
        None => {
            let m = Marker { name: store::name_for(&path), assets: None };
            let note = store::write_marker(&path, &m).err().map(|e| format!("{} could not be written ({e}): the folder is opened as a project all the same", path.join(crate::launch_list::MARKER).display()));
            (m, note)
        }
    };
    let assets = marker.assets.as_ref().map(|a| path.join(a)).filter(|a| a.is_dir());
    Ok(Opened { path, name: marker.name, assets, note })
}

/// One engine a project: two would write the same worlds and caches (review R1-2). An engine
/// holds a lock named after its project's folder for as long as it runs. The lock is a file in
/// the system's temporary folder (per user on Windows), not in the project: nothing is added
/// to a folder a person may keep in version control, and engines started with different
/// `--settings` still see each other. It is held as an open handle that shares the file with
/// nobody, so the system drops it when the process ends, however it ends: a crash leaves
/// nothing to clean up, and the file itself means nothing while nobody holds it.
///
/// Windows only so far (`locks_here`): elsewhere no engine runs yet, and the lock says it is
/// unavailable (AGENTS rule 4) until that port takes it with the system's own file lock.
#[cfg(any(windows, test))]
const LOCKS_FOLDER: &str = "specula-locks";

/// An engine's hold on its project: dropped (and the lock with it) when the engine ends.
#[derive(Debug)]
pub struct ProjectLock {
    _file: std::fs::File,
}

/// Why a project's lock was not taken.
#[derive(Debug, PartialEq)]
pub enum LockRefused {
    /// Another engine has the project open.
    Held,
    /// The lock could not be made (no such folder, a temporary folder that can't be written,
    /// a platform without it): the engine goes on without it and says so.
    Unavailable(String),
}

/// Whether projects are locked on this platform.
pub fn locks_here() -> bool {
    cfg!(windows)
}

/// The lock file for the project in `folder` (which must exist: it is named after the folder
/// as the system spells it, so two spellings of one folder are one lock).
#[cfg(any(windows, test))]
fn lock_path(folder: &Path) -> std::io::Result<PathBuf> {
    let key = crate::launch_list::path_key(&folder.canonicalize()?);
    Ok(std::env::temp_dir().join(LOCKS_FOLDER).join(format!("{:016x}.lock", specula_core::hash::hash_str(&key))))
}

/// `ERROR_SHARING_VIOLATION`: the file is open elsewhere in a way that does not share it.
#[cfg(windows)]
const SHARING_VIOLATION: i32 = 32;

/// Takes the project's lock for this engine (start-up, before anything of the project is
/// read or written).
pub fn lock(folder: &Path) -> Result<ProjectLock, LockRefused> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let unavailable = |e: &dyn std::fmt::Display| LockRefused::Unavailable(e.to_string());
        let path = lock_path(folder).map_err(|e| unavailable(&e))?;
        if !path.is_file() {
            // (Its one write, the folder's path for whoever looks: admitted by the disk book.)
            let note = format!("An engine holds this file open while it has this project open (docs/design/LAUNCH.md):\n{}\n", folder.display());
            let written = crate::disk::book().write_synced(crate::disk::Category::Settings, "launch", &path, note.as_bytes(), crate::disk::Need::Essential);
            // (Another engine made it, and holds it, between the look and the write: that is
            // "held", found below, never "no lock, go on". Review R2-4.)
            if let Err(e) = written {
                if !path.is_file() {
                    return Err(unavailable(&e));
                }
            }
        }
        // (A program asking whether the project is open has the file open for an instant:
        // asked a few times before "held" is believed.)
        for attempt in 0..10 {
            match std::fs::OpenOptions::new().read(true).share_mode(0).open(&path) {
                Ok(file) => return Ok(ProjectLock { _file: file }),
                Err(e) if e.raw_os_error() == Some(SHARING_VIOLATION) => {
                    if attempt < 9 {
                        std::thread::sleep(std::time::Duration::from_millis(30));
                    }
                }
                Err(e) => return Err(unavailable(&e)),
            }
        }
        Err(LockRefused::Held)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Err(LockRefused::Unavailable("a project's lock is not implemented on this platform yet".into()))
    }
}

/// Tests: removes the (unheld) lock file a test's project left in the temporary folder.
#[cfg(test)]
pub(crate) fn forget_lock_for_test(folder: &Path) {
    if let Ok(path) = lock_path(folder) {
        let _ = std::fs::remove_file(path);
    }
}

/// Whether an engine has the project in `folder` open now (it holds the lock). Nothing is
/// written: a folder nobody ever locked is not open anywhere.
pub fn project_locked(folder: &Path) -> bool {
    #[cfg(windows)]
    {
        lock_path(folder).is_ok_and(|path| std::fs::OpenOptions::new().read(true).open(path).is_err_and(|e| e.raw_os_error() == Some(SHARING_VIOLATION)))
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        false
    }
}

/// The project of a working directory that is one (it holds a marker), without `--project`.
/// `None`: it is not a project (or its marker is not one: the engine starts as it always has).
pub fn of_working_directory(cwd: &Path) -> Option<Opened> {
    match store::read_marker(cwd) {
        Ok(Some(_)) => open(cwd, false).ok(),
        _ => None,
    }
}

/// Where the launcher's files go for this engine: `--settings off` nowhere, `--settings DIR`
/// there, else the engine's per-user folder (`None` when the environment names none).
pub fn store_for(settings: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Option<Store> {
    match settings {
        Some("off") => None,
        Some(dir) => Some(Store::new(PathBuf::from(dir))),
        None => crate::user_settings::per_user_base(var).map(Store::new),
    }
}

/// How often a serving engine looks for a stop request.
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(400);

/// An engine that told the machine it is running: what to take back at shutdown.
pub struct Announced {
    store: Store,
    pid: u32,
    project: Option<PathBuf>,
    /// Ends the stop watcher.
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Tests: the stop watcher's turns so far (a test that says the watcher left something
    /// alone waits for a turn that began after it, never for a time).
    #[cfg(test)]
    turns: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

/// What an engine that shows the start screen shares with its watcher thread: `specula open`
/// may hand it a project, which it takes only while it is still nothing but a start screen.
#[derive(Clone, Default)]
pub struct Handover {
    /// Kept by the engine thread every frame: [`Handover::IDLE`] while the start screen is open
    /// and the world is empty, [`Handover::BUSY`] once it is not. [`Handover::UNKNOWN`] until
    /// the first frame: the instance file says "start screen" before the window exists, and a
    /// hand-over that arrives then waits for the answer instead of being refused (review R1-6).
    pub state: std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// The project handed over, and whether its window is to open in front: the engine stops,
    /// and the executable starts again in it.
    pub handed: std::sync::Arc<std::sync::Mutex<Option<(PathBuf, bool)>>>,
}

impl Handover {
    pub const UNKNOWN: u8 = 0;
    pub const IDLE: u8 = 1;
    pub const BUSY: u8 = 2;
}

/// What an engine says of itself when it starts serving.
pub struct Serving<'a> {
    pub port: u16,
    pub project: Option<&'a Opened>,
    pub world: Option<&'a str>,
    pub window: bool,
    pub game: Option<&'a str>,
    /// This executable, so "resume" starts the same one (a game's own, or the stock engine).
    pub exe: Option<PathBuf>,
    /// Seconds since the Unix epoch, UTC.
    pub now: u64,
    /// It shows the start screen (opened with no project).
    pub launcher: Option<Handover>,
    /// How the engine is asked to stop (a stop request, a hand-over taken): the engine's own
    /// `interrupt::request`. A test passes its own: the interrupt flag is the whole process's.
    pub stop: fn(),
}

/// Lists the project first and writes this engine's instance file. Problems are returned as
/// words for the start-up log: the engine runs all the same (it is only not found or listed).
pub fn announce(store: Store, s: &Serving<'_>) -> (Announced, Vec<String>) {
    let mut problems = Vec::new();
    let version = env!("CARGO_PKG_VERSION").to_owned();
    if let Some(p) = s.project {
        let project = Project { path: p.path.clone(), name: p.name.clone(), exe: s.exe.clone(), last_world: s.world.map(str::to_owned), last_opened: s.now, version: version.clone() };
        if let Err(e) = store.opened(project) {
            problems.push(format!("the project list could not be written ({e}): {} is open but not listed", p.name));
        }
    }
    let pid = std::process::id();
    let instance = Instance {
        pid,
        port: s.port,
        project: s.project.map(|p| (p.path.clone(), p.name.clone())),
        world: s.world.map(str::to_owned),
        window: s.window,
        started: s.now,
        version,
        game: s.game.map(str::to_owned),
        launcher: s.launcher.is_some(),
    };
    if let Err(e) = instances::write(&store.dir, &instance) {
        problems.push(format!("this engine's instance file could not be written ({e}): 'specula list' will not show it"));
    }
    // `specula stop`: an engine started in the background has no console to interrupt, so it
    // is asked through its own folder, and stops as it does on Ctrl+C. One small thread, started
    // once, looks a few times a second (never the engine thread).
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (dir, ended, handover, stop) = (store.dir.clone(), std::sync::Arc::clone(&done), s.launcher.clone(), s.stop);
    #[cfg(test)]
    let turns = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    #[cfg(test)]
    let turned = std::sync::Arc::clone(&turns);
    let watcher = std::thread::Builder::new().name("specula-launch-stop".into()).spawn(move || {
        while !ended.load(std::sync::atomic::Ordering::Relaxed) {
            if instances::take_stop(&dir, pid) {
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "launch.stop_requested", pid = u64::from(pid));
                stop();
                break;
            }
            // `specula open`, to a start screen: taken while this engine is still only that
            // (and the folder is there, and no engine has it open), else refused, and the
            // command starts an engine itself. Before the first frame nobody knows yet: the
            // request is left where it is.
            if let Some(h) = handover.as_ref() {
                let state = h.state.load(std::sync::atomic::Ordering::Relaxed);
                if state != Handover::UNKNOWN && instances::open_waits(&dir, pid) {
                    let handed = instances::take_open(&dir, pid);
                    let taken = handed.as_ref().is_some_and(|(folder, _)| state == Handover::IDLE && folder.is_dir() && !project_locked(folder));
                    // (No folder in the trace: agents in this engine read traces, and the
                    // person's other projects are not theirs to learn. Review R1-7.)
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "launch.handed", taken = taken);
                    if taken {
                        if let Ok(mut slot) = h.handed.lock() {
                            *slot = handed;
                        }
                        stop();
                        break;
                    }
                    instances::refuse_open(&dir, pid);
                }
            }
            #[cfg(test)]
            turned.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(STOP_POLL);
        }
    });
    if let Err(e) = watcher {
        problems.push(format!("'specula stop' will not reach this engine (no thread for its watcher: {e})"));
    }
    (
        Announced {
            store,
            pid,
            project: s.project.map(|p| p.path.clone()),
            done,
            #[cfg(test)]
            turns,
        },
        problems,
    )
}

impl Announced {
    /// The engine stops: its instance file goes, and the project remembers the world it had
    /// open.
    pub fn finish(self, world: Option<&str>) {
        self.done.store(true, std::sync::atomic::Ordering::Relaxed);
        instances::remove(&self.store.dir, self.pid);
        if let Some(path) = &self.project {
            let _ = self.store.closed(path, world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-launch-engine-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_opened_as_a_project_gets_its_marker_and_a_made_project_keeps_its_own() {
        let dir = scratch("open");
        // A plain folder: named after itself, its assets are the folder's own (as an engine
        // started there has always read them).
        let plain = dir.join("crypt");
        std::fs::create_dir_all(&plain).unwrap();
        let o = open(&plain, false).unwrap();
        assert_eq!((o.name.as_str(), &o.assets, &o.note), ("crypt", &None, &None));
        assert!(o.path.is_absolute() && plain.join(crate::launch_list::MARKER).is_file());
        assert_eq!(open(&plain, false).unwrap(), o, "opened again: the same, from its marker");
        // One made by `specula new`: its marker's name and its assets folder.
        let made = store::create(&dir, "Frozen Wasteland").unwrap();
        let o = open(&made, false).unwrap();
        assert_eq!((o.name.as_str(), o.assets.clone()), ("Frozen Wasteland", Some(made.join("assets"))));
        // A marker naming an assets folder that is gone: the project's own folder again.
        std::fs::remove_dir_all(made.join("assets")).unwrap();
        assert_eq!(open(&made, false).unwrap().assets, None);
        // No such folder, and a marker that isn't one, are said.
        assert!(open(&dir.join("nowhere"), false).unwrap_err().contains("there is no such folder"));
        std::fs::write(plain.join(crate::launch_list::MARKER), "not a marker").unwrap();
        assert!(open(&plain, false).unwrap_err().contains("is not a project marker"));
        // A working directory is a project only when it holds a marker that is one.
        assert!(of_working_directory(&made).is_some());
        assert!(of_working_directory(&plain).is_none() && of_working_directory(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review R1-2: one engine a project.
    #[test]
    fn a_project_is_held_by_one_engine_at_a_time() {
        let dir = scratch("lock");
        let made = store::create(&dir, "crypt").unwrap();
        if !locks_here() {
            // (No engine runs here yet: the lock says so, and nothing reads as open.)
            assert!(matches!(lock(&made), Err(LockRefused::Unavailable(_))) && !project_locked(&made));
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let before: Vec<_> = std::fs::read_dir(&made).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(!project_locked(&made) && !lock_path(&made).unwrap().exists(), "asking writes nothing");
        let held = lock(&made).unwrap();
        assert!(project_locked(&made) && lock_path(&made).unwrap().is_file());
        let after: Vec<_> = std::fs::read_dir(&made).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(before, after, "nothing is added to the project's folder");
        assert_eq!(lock(&made).unwrap_err(), LockRefused::Held, "a second engine is refused");
        // Another spelling of the folder is the same lock; another folder is another.
        let spelt = made.parent().unwrap().join("CRYPT").join(".");
        assert!(project_locked(&spelt), "one folder, one lock, however it is spelt");
        let other = store::create(&dir, "frost").unwrap();
        assert!(!project_locked(&other));
        drop(lock(&other).unwrap());
        // Asking does not take it from the holder, and does not leave it held.
        assert!(project_locked(&made) && project_locked(&made));
        drop(held);
        assert!(!project_locked(&made), "the engine ended: the project is free, its lock file means nothing");
        let again = lock(&made).unwrap();
        drop(again);
        // A folder that is not there: no lock, said as unavailable (the engine says so and goes on).
        assert!(matches!(lock(&dir.join("nowhere")), Err(LockRefused::Unavailable(_))));
        assert!(!project_locked(&dir.join("nowhere")) && !dir.join("nowhere").exists(), "and asking for it makes nothing");
        // The lock file is left in the temporary folder, held by nobody: tidied here.
        for folder in [&made, &other] {
            forget_lock_for_test(folder);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_launchers_files_follow_the_settings_flag() {
        let env = |k: &str| (k == "HOME" || k == concat!("APP", "DATA") || k == "XDG_CONFIG_HOME").then(|| "base".to_owned());
        assert!(store_for(Some("off"), env).is_none(), "off keeps nothing");
        assert_eq!(store_for(Some("D:/scratch/u"), env).unwrap().dir, PathBuf::from("D:/scratch/u"));
        let default = store_for(None, env).unwrap().dir;
        assert!(default.starts_with("base") && default.to_string_lossy().to_lowercase().contains("specula"), "{default:?}");
        assert!(store_for(None, |_| None).is_none(), "no per-user folder named: nothing kept");
    }

    #[test]
    fn a_serving_engine_is_listed_and_found_and_leaves_no_instance_behind() {
        let dir = scratch("announce");
        let made = store::create(&dir, "crypt").unwrap();
        let opened = open(&made, false).unwrap();
        let st = Store::new(dir.join("user"));
        let serving = Serving { port: 7471, project: Some(&opened), world: Some("night"), window: true, game: None, exe: Some(PathBuf::from("C:/specula/specula.exe")), now: 1_800_000_000, launcher: None, stop: no_stop };
        let (announced, problems) = announce(st.clone(), &serving);
        assert!(problems.is_empty(), "{problems:?}");
        // First in the list, with the executable that opened it and its world.
        let first = st.load().0.last().cloned().unwrap();
        assert_eq!((first.name.as_str(), first.last_world.as_deref(), first.exe.as_deref()), ("crypt", Some("night"), Some(Path::new("C:/specula/specula.exe"))));
        // Found: this process's instance file says the port and the project.
        let (found, _) = instances::list(&st.dir, |_| true);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].pid, found[0].port, found[0].project.as_ref().map(|p| p.1.as_str())), (std::process::id(), 7471, Some("crypt")));
        // At shutdown the instance file goes and the project keeps the world it had open then.
        announced.finish(Some("dawn"));
        assert!(instances::list(&st.dir, |_| true).0.is_empty());
        assert_eq!(st.load().0.last().and_then(|p| p.last_world.clone()).as_deref(), Some("dawn"));
        // An engine with no project is found all the same, and lists nothing.
        let bare = Store::new(dir.join("bare"));
        let (announced, problems) = announce(bare.clone(), &Serving { port: 7500, project: None, world: None, window: false, game: Some("sepulcra"), exe: None, now: 5, launcher: None, stop: no_stop });
        assert!(problems.is_empty() && bare.load().0.projects.is_empty());
        assert_eq!(instances::list(&bare.dir, |_| true).0[0].game.as_deref(), Some("sepulcra"));
        announced.finish(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// How often the watcher asked the engine to stop (the hand-over test's own).
    static STOPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn counted_stop() {
        STOPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn no_stop() {}

    /// Waits (a bounded time) for the watcher thread to have acted.
    fn wait_for(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + specula_core::speed_budget(std::time::Duration::from_secs(5));
        while !done() {
            assert!(std::time::Instant::now() < deadline, "the watcher never did this: {what}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Waits until the watcher has begun a whole turn since now, so what it leaves alone it
    /// has looked at. (A fixed wait of three polls said "left alone" of a watcher a loaded
    /// machine had not yet run: a check that could pass without looking.)
    fn looked(a: &Announced) {
        let seq = std::sync::atomic::Ordering::SeqCst;
        let now = a.turns.load(seq);
        // (The turn running now may have looked before the request: the one after it did not.)
        wait_for("look again", || a.turns.load(seq) >= now + 2);
    }

    #[test]
    fn a_start_screen_takes_a_project_handed_to_it_only_while_it_is_nothing_else() {
        let dir = scratch("handover");
        let made = store::create(&dir, "crypt").unwrap();
        let st = Store::new(dir.join("user"));
        let h = Handover::default();
        let serving = Serving { port: 7471, project: None, world: None, window: true, game: None, exe: None, now: 9, launcher: Some(h.clone()), stop: counted_stop };
        let (announced, problems) = announce(st.clone(), &serving);
        assert!(problems.is_empty(), "{problems:?}");
        let pid = std::process::id();
        let relaxed = std::sync::atomic::Ordering::Relaxed;
        assert!(instances::list(&st.dir, |_| true).0[0].launcher, "found as a start screen");
        // Before its first frame nobody knows whether it is only a start screen: a hand-over
        // waits, neither taken nor refused (review R1-6: it was refused, and whether "open the
        // .exe, then ask the agent" worked depended on which came first, a frame or the command).
        assert!(instances::request_open(&st.dir, pid, &made, false).unwrap());
        looked(&announced);
        assert!(instances::open_waits(&st.dir, pid) && !instances::refused_file(&st.dir, pid).exists() && h.handed.lock().unwrap().is_none());
        // Its first frame says it is busy (its screen was closed, or something is in its
        // world): the hand-over that waited is refused, said so.
        h.state.store(Handover::BUSY, relaxed);
        wait_for("refuse a hand-over while busy", || instances::refused_file(&st.dir, pid).exists());
        assert!(h.handed.lock().unwrap().is_none() && !instances::open_waits(&st.dir, pid));
        // Idle, but the folder is not there, or an engine has the project open, or the file
        // names no folder: refused too.
        h.state.store(Handover::IDLE, relaxed);
        assert!(instances::request_open(&st.dir, pid, &dir.join("nowhere"), false).unwrap());
        wait_for("refuse a folder that is not there", || instances::refused_file(&st.dir, pid).exists());
        if locks_here() {
            let held = lock(&made).unwrap();
            assert!(instances::request_open(&st.dir, pid, &made, false).unwrap());
            wait_for("refuse a project an engine has open", || instances::refused_file(&st.dir, pid).exists());
            drop(held);
            forget_lock_for_test(&made);
        }
        instances::take_refused(&st.dir, pid);
        std::fs::write(instances::open_file(&st.dir, pid), "not a folder").unwrap();
        wait_for("refuse a file that names no folder", || instances::refused_file(&st.dir, pid).exists());
        assert!(h.handed.lock().unwrap().is_none());
        // Idle and there: taken, with whether its window opens in front, the engine asked to
        // stop (once), nothing refused.
        assert_eq!(STOPS.load(relaxed), 0, "nothing refused stops the engine");
        assert!(instances::request_open(&st.dir, pid, &made, true).unwrap());
        wait_for("take the hand-over", || h.handed.lock().unwrap().is_some());
        assert_eq!(h.handed.lock().unwrap().clone(), Some((made, true)));
        wait_for("ask the engine to stop", || STOPS.load(relaxed) == 1);
        assert!(!instances::refused_file(&st.dir, pid).exists());
        announced.finish(None);
        // An engine that is no start screen never looks: the file stays for nobody.
        let plain = Store::new(dir.join("plain"));
        let (announced, _) = announce(plain.clone(), &Serving { port: 7472, project: None, world: None, window: true, game: None, exe: None, now: 9, launcher: None, stop: no_stop });
        assert!(instances::request_open(&plain.dir, pid, &dir, false).unwrap());
        looked(&announced);
        assert!(instances::open_waits(&plain.dir, pid) && !instances::refused_file(&plain.dir, pid).exists());
        announced.finish(None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
