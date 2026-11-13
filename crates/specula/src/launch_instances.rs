//! Running engines, found (docs/design/LAUNCH.md): an engine that serves writes a small file
//! in the user's per-user folder (`instances/<pid>.json`: its port, its project, whether it has a
//! window) and removes it when it stops. `specula list`, `specula open` and `specula call` read
//! them, so an agent with a shell finds a running engine without being told its port.
//!
//! A file can outlive its engine (a crash, a kill): a reader asks whether its port still
//! answers, and one that doesn't is left out and removed. The file holds no secret: when agents
//! get tokens (M6), the token goes here, in the user's own folder.

use specula_core::Json;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Instance files a reader looks at (more engines than this on one machine are not listed).
pub const MAX_INSTANCES: usize = 64;
/// An instance file's largest size read.
pub const MAX_INSTANCE_BYTES: u64 = 16 * 1024;
/// The folder, inside the per-user folder (a game's settings folder there never has a dot in
/// its name, so the two can't collide).
pub const FOLDER: &str = "engine.instances";
/// How long a reader waits for an instance's port to answer.
pub const ALIVE_WAIT: Duration = Duration::from_millis(300);
const SCHEMA: &str = "specula.instance.v1";

/// One running engine.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub pid: u32,
    pub port: u16,
    /// The project it has open (its folder and name), if it was opened on one.
    pub project: Option<(PathBuf, String)>,
    pub world: Option<String>,
    /// Whether a human's window is open (false: headless).
    pub window: bool,
    /// When it started serving: seconds since the Unix epoch, UTC.
    pub started: u64,
    pub version: String,
    /// The hosted game's name, if it hosts one.
    pub game: Option<String>,
    /// It was opened with no project and shows the start screen: `specula open` may hand it
    /// one ([`request_open`]).
    pub launcher: bool,
}

impl Instance {
    /// Where agents reach it (loopback only: AGENTS.md rule 12).
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }

    pub fn file_name(pid: u32) -> String {
        format!("{pid}.json")
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("schema", Json::from(SCHEMA)), ("pid", Json::from(self.pid)), ("port", Json::from(u32::from(self.port))), ("url", Json::from(self.url()))];
        if let Some((path, name)) = &self.project {
            f.push(("project", Json::obj([("path", Json::from(path.to_string_lossy().as_ref())), ("name", Json::from(name.as_str()))])));
        }
        if let Some(w) = &self.world {
            f.push(("world", Json::from(w.as_str())));
        }
        f.push(("window", Json::from(self.window)));
        f.push(("started_utc", Json::from(self.started)));
        f.push(("version", Json::from(self.version.as_str())));
        if let Some(g) = &self.game {
            f.push(("game", Json::from(g.as_str())));
        }
        if self.launcher {
            f.push(("launcher", Json::from(true)));
        }
        Json::obj(f)
    }

    /// Reads an instance file's text (`None`: not one, or its numbers are out of range).
    pub fn parse(text: &str) -> Option<Instance> {
        let j = Json::parse(text).ok()?;
        if j.get("schema").and_then(Json::as_str) != Some(SCHEMA) {
            return None;
        }
        let short = |j: &Json, k: &str, max: usize| j.get(k).and_then(Json::as_str).filter(|s| !s.is_empty() && s.len() <= max).map(str::to_owned);
        let pid = u32::try_from(j.get("pid").and_then(Json::as_u64)?).ok().filter(|p| *p != 0)?;
        let port = u16::try_from(j.get("port").and_then(Json::as_u64)?).ok().filter(|p| *p != 0)?;
        let project = j.get("project").and_then(|p| Some((PathBuf::from(short(p, "path", 1024)?), short(p, "name", 256)?)));
        Some(Instance {
            pid,
            port,
            project,
            world: short(&j, "world", 256),
            window: j.get("window").and_then(Json::as_bool).unwrap_or(false),
            started: j.get("started_utc").and_then(Json::as_u64).unwrap_or(0),
            version: short(&j, "version", 64).unwrap_or_default(),
            game: short(&j, "game", 128),
            launcher: j.get("launcher").and_then(Json::as_bool).unwrap_or(false),
        })
    }
}

/// The file that asks engine `pid` to stop (`specula stop`): beside its instance file. Whoever
/// can write the user's per-user folder can already end the user's processes; the engine that
/// reads it shuts down cleanly, as on Ctrl+C (worlds saved, its instance file removed).
pub fn stop_file(per_user: &Path, pid: u32) -> PathBuf {
    per_user.join(FOLDER).join(format!("{pid}.stop"))
}

/// Asks engine `pid` to stop.
pub fn request_stop(per_user: &Path, pid: u32) -> std::io::Result<()> {
    crate::launch_store::make_dir(&per_user.join(FOLDER))?;
    crate::launch_store::write_small(&stop_file(per_user, pid), "stop")
}

/// Whether engine `pid` was asked to stop; the request is taken (its file removed).
pub fn take_stop(per_user: &Path, pid: u32) -> bool {
    let file = stop_file(per_user, pid);
    let asked = file.exists();
    if asked {
        crate::launch_store::remove_inside(&per_user.join(FOLDER), &file);
    }
    asked
}

/// Most files looked at in the instances folder in one pass (instance files, requests and the
/// resume tokens of `specula call --as`).
pub const MAX_FOLDER_ENTRIES: usize = 8192;

/// The file that hands engine `pid` a project to open (`specula open`, to a start screen that
/// is waiting), and the one its engine answers a hand-over it does not take with.
pub fn open_file(per_user: &Path, pid: u32) -> PathBuf {
    per_user.join(FOLDER).join(format!("{pid}.open"))
}

pub fn refused_file(per_user: &Path, pid: u32) -> PathBuf {
    per_user.join(FOLDER).join(format!("{pid}.refused"))
}

/// Hands engine `pid` the project folder to open. `front`: a person asked to see its window
/// (`specula open --front`); else the engine it becomes opens behind the other windows.
/// `Ok(false)`: another command's hand-over is waiting there, and this one was not written (two
/// commands never overwrite each other's request). An answer left over from an earlier
/// hand-over is cleared first, so it is never read as the answer to this one.
pub fn request_open(per_user: &Path, pid: u32, folder: &Path, front: bool) -> std::io::Result<bool> {
    crate::launch_store::make_dir(&per_user.join(FOLDER))?;
    let file = open_file(per_user, pid);
    if file.exists() {
        return Ok(false);
    }
    take_refused(per_user, pid);
    let text = format!("{}{}", folder.to_string_lossy(), if front { "\nfront" } else { "" });
    // Made, never written over: of two commands at the same instant one makes the file and
    // the other is told it is there.
    crate::launch_store::create_small(&file, &text)
}

/// How long a hand-over file may be empty before it is read as one that names nothing (it is
/// made, then written: the watcher never takes it in between).
const OPEN_SETTLE: Duration = Duration::from_secs(2);

/// Whether a hand-over waits for engine `pid`, written (nothing is taken).
pub fn open_waits(per_user: &Path, pid: u32) -> bool {
    std::fs::metadata(open_file(per_user, pid)).is_ok_and(|m| m.len() > 0 || m.modified().ok().and_then(|at| at.elapsed().ok()).is_none_or(|age| age > OPEN_SETTLE))
}

/// The folder engine `pid` was handed, if any, and whether its window is to open in front; the
/// request is taken (its file removed). A file that is too long, empty, not text or not an
/// absolute path is taken and gives nothing (the engine starts again with it as `--project`:
/// never a word that could be read as a flag or against some other working directory).
pub fn take_open(per_user: &Path, pid: u32) -> Option<(PathBuf, bool)> {
    let file = open_file(per_user, pid);
    let size = std::fs::metadata(&file).ok()?.len();
    let text = if size as usize <= crate::launch_list::MAX_PATH_BYTES { std::fs::read_to_string(&file).ok() } else { None };
    crate::launch_store::remove_inside(&per_user.join(FOLDER), &file);
    let text = text?;
    let (path, flag) = text.split_once('\n').unwrap_or((&text, ""));
    let path = path.trim();
    Some((PathBuf::from(path), flag.trim() == "front")).filter(|(p, _)| !path.chars().any(char::is_control) && p.is_absolute())
}

/// Engine `pid` does not take the hand-over (it is no longer only a start screen).
pub fn refuse_open(per_user: &Path, pid: u32) {
    let _ = crate::launch_store::write_small(&refused_file(per_user, pid), "refused");
}

/// Whether engine `pid` refused a hand-over; the answer is taken (its file removed).
pub fn take_refused(per_user: &Path, pid: u32) -> bool {
    let file = refused_file(per_user, pid);
    let refused = file.exists();
    if refused {
        crate::launch_store::remove_inside(&per_user.join(FOLDER), &file);
    }
    refused
}

/// Whether something answers on an instance's port (a connection is opened and closed; nothing
/// is sent). Loopback only.
pub fn answers(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&addr, ALIVE_WAIT).is_ok()
}

/// Whether the engine an instance file names is serving now: its process owns a listening
/// socket on its port. "Something answers there" is not enough: a stopped engine's port goes
/// to the next program, and a command sent to it would reach the wrong engine (review R1-3).
/// Where the system's listener table can't be read (not Windows yet), a running process and
/// an answering port are the best that is known.
pub fn serving(i: &Instance) -> bool {
    match specula_sys::net::tcp_listeners() {
        Ok(listeners) => listeners.contains(&(i.port, i.pid)),
        Err(_) => specula_sys::process_is_running(i.pid) && answers(i.port),
    }
}

/// Whether a program listens on `port` now.
pub fn port_taken(port: u16) -> bool {
    specula_sys::net::tcp_listening_ports().map_or_else(|_| answers(port), |ports| ports.contains(&port))
}

/// Writes this engine's instance file (the folder is made), whole or not at all: a command
/// that reads the folder while the engine starts never finds half a file. The engine calls it
/// once its server is up, from start-up code, never from a frame.
pub fn write(per_user: &Path, i: &Instance) -> std::io::Result<PathBuf> {
    let dir = per_user.join(FOLDER);
    crate::launch_store::make_dir(&dir)?;
    let path = dir.join(Instance::file_name(i.pid));
    crate::launch_store::write_whole(&path, &i.to_json().to_pretty())?;
    Ok(path)
}

/// Removes an engine's instance file (at shutdown; a missing one is fine).
pub fn remove(per_user: &Path, pid: u32) {
    let dir = per_user.join(FOLDER);
    crate::launch_store::remove_inside(&dir, &dir.join(Instance::file_name(pid)));
}

/// The running engines: every instance file whose engine `alive` says is serving, newest
/// first; a file that isn't an instance, or whose engine is gone, is removed (`stale`: how many).
/// At most [`MAX_INSTANCES`] instance files are looked at.
pub fn list(per_user: &Path, alive: impl Fn(&Instance) -> bool) -> (Vec<Instance>, usize) {
    list_with(per_user, alive, specula_sys::process_is_running)
}

/// [`list`], with how a process is known to be running.
pub fn list_with(per_user: &Path, alive: impl Fn(&Instance) -> bool, running: impl Fn(u32) -> bool) -> (Vec<Instance>, usize) {
    let dir = per_user.join(FOLDER);
    let Ok(entries) = std::fs::read_dir(&dir) else { return (Vec::new(), 0) };
    let names: Vec<String> = entries.flatten().take(MAX_FOLDER_ENTRIES).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let (mut found, mut stale) = (Vec::new(), 0);
    // The pids whose instance file is still there after this pass (kept, or not looked at).
    let mut kept: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    // Instance files only, before the cap: a folder full of resume tokens never hides an
    // engine (review R1-4: `<pid>.agent-01.token` sorts before `<pid>.json`).
    for (n, name) in names.iter().filter(|n| n.ends_with(".json")).enumerate() {
        let named = name.strip_suffix(".json").and_then(|p| p.parse::<u32>().ok());
        if n >= MAX_INSTANCES {
            kept.extend(named);
            continue;
        }
        let path = dir.join(name);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_INSTANCE_BYTES);
        let read = if small { std::fs::read_to_string(&path).ok().and_then(|t| Instance::parse(&t)) } else { None };
        // (The file's name is its engine's pid: one renamed by hand is not trusted.)
        let read = read.filter(|i| Some(i.pid) == named);
        match (read, named) {
            (Some(i), _) if alive(&i) => {
                kept.insert(i.pid);
                found.push(i);
            }
            // Not an instance file, yet named for a process that is running: left alone (never
            // judged from a file that may be on its way; its engine removes it when it stops).
            (None, Some(pid)) if running(pid) => {
                kept.insert(pid);
            }
            _ => {
                stale += 1;
                crate::launch_store::remove_inside(&dir, &path);
            }
        }
    }
    // What a gone engine left beside its instance file (a stop request, a hand-over or its
    // refusal, the resume tokens of `specula call --as`): removed with it. Only names of that
    // shape: `<pid>.stop`, `<pid>.open`, `<pid>.refused`, `<pid>.<name>.token`, and only when
    // no instance file for that pid is left (one this pass did not look at is not judged).
    for name in &names {
        let pid = name.split('.').next().and_then(|p| p.parse::<u32>().ok());
        let ours = [".stop", ".open", ".refused", ".token"].iter().any(|end| name.ends_with(end));
        if pid.is_some_and(|pid| ours && !kept.contains(&pid)) {
            crate::launch_store::remove_inside(&dir, &dir.join(name));
        }
    }
    found.sort_by(|a, b| b.started.cmp(&a.started).then(a.pid.cmp(&b.pid)));
    (found, stale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(pid: u32, port: u16, started: u64) -> Instance {
        Instance { launcher: false, pid, port, project: Some((PathBuf::from("C:/games/crypt"), "crypt".into())), world: Some("night".into()), window: true, started, version: "0.3.0".into(), game: None }
    }

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-launch-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_instance_file_round_trips_and_says_where_to_reach_the_engine() {
        let i = instance(4242, 7471, 1_800_000_000);
        assert_eq!(i.url(), "http://127.0.0.1:7471/mcp");
        assert_eq!(Instance::parse(&i.to_json().to_pretty()), Some(i.clone()));
        let bare = Instance { project: None, world: None, window: false, game: Some("sepulcra".into()), ..i };
        assert_eq!(Instance::parse(&bare.to_json().to_string()), Some(bare));
        for bad in ["", "{}", r#"{"schema": "specula.instance.v1", "pid": 0, "port": 7471}"#, r#"{"schema": "specula.instance.v1", "pid": 5, "port": 70000}"#, r#"{"schema": "other", "pid": 5, "port": 7471}"#] {
            assert_eq!(Instance::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn running_engines_are_listed_newest_first_and_stale_files_are_removed() {
        let dir = scratch("list");
        assert_eq!(list(&dir, |_| true), (Vec::new(), 0), "no engine has run: no folder, an empty list");
        let (old, new, dead) = (instance(100, 7471, 10), instance(200, 7472, 20), instance(300, 7473, 30));
        for i in [&old, &new, &dead] {
            write(&dir, i).unwrap();
        }
        // Files that aren't an engine's: not JSON, another pid's contents, too large, not .json.
        let folder = dir.join(FOLDER);
        std::fs::write(folder.join("999.json"), "not json").unwrap();
        std::fs::write(folder.join("555.json"), instance(556, 7474, 40).to_json().to_string()).unwrap();
        std::fs::write(folder.join("777.json"), " ".repeat(MAX_INSTANCE_BYTES as usize + 1)).unwrap();
        std::fs::write(folder.join("notes.txt"), "kept").unwrap();
        let (found, stale) = list_with(&dir, |i| i.pid != 300, |_| false);
        assert_eq!(found, vec![new.clone(), old.clone()], "newest first, the dead one left out");
        assert_eq!(stale, 4, "the dead engine's file and the three that aren't one");
        let left: std::collections::BTreeSet<String> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left.into_iter().collect::<Vec<_>>(), ["100.json", "200.json", "notes.txt"]);
        // An engine that stops removes its own.
        remove(&dir, 200);
        remove(&dir, 200);
        assert_eq!(list(&dir, |_| true), (vec![old.clone()], 0));
        // A stop request is a file beside the instance: taken once by its engine; and what a
        // gone engine left (a request nobody took, a resume token) goes with it.
        assert!(!take_stop(&dir, 100));
        request_stop(&dir, 100).unwrap();
        assert!(stop_file(&dir, 100).is_file());
        assert!(take_stop(&dir, 100) && !take_stop(&dir, 100) && !stop_file(&dir, 100).exists());
        request_stop(&dir, 100).unwrap();
        request_stop(&dir, 4321).unwrap();
        std::fs::write(folder.join("4321.ann.token"), "t").unwrap();
        std::fs::write(folder.join("100.ann.token"), "t").unwrap();
        assert_eq!(list(&dir, |_| true), (vec![old], 0));
        let left: std::collections::BTreeSet<String> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left.into_iter().collect::<Vec<_>>(), ["100.ann.token", "100.json", "100.stop", "notes.txt"], "the running engine's own are kept");
        // A hand-over is a file beside the instance too: taken once, with the folder it names;
        // one that is no path is taken and gives nothing; a refusal is read once.
        assert_eq!(take_open(&dir, 100), None);
        let frost = std::path::absolute("games/frost").unwrap();
        assert!(request_open(&dir, 100, &frost, false).unwrap());
        // A second command's hand-over never overwrites one that is waiting.
        assert!(open_waits(&dir, 100));
        // (A file just made and not written yet is not one that waits: the watcher leaves it.)
        std::fs::write(open_file(&dir, 101), "").unwrap();
        assert!(open_file(&dir, 101).exists() && !open_waits(&dir, 101));
        std::fs::remove_file(open_file(&dir, 101)).unwrap();
        assert!(!request_open(&dir, 100, Path::new("D:/games/other"), true).unwrap());
        assert_eq!(take_open(&dir, 100), Some((frost.clone(), false)));
        assert!(take_open(&dir, 100).is_none() && !open_file(&dir, 100).exists() && !open_waits(&dir, 100));
        // `--front` travels with it; an answer left by an earlier hand-over is cleared by the next.
        refuse_open(&dir, 100);
        assert!(request_open(&dir, 100, &frost, true).unwrap());
        assert!(!refused_file(&dir, 100).exists(), "a stale refusal is never read as this hand-over's answer");
        assert_eq!(take_open(&dir, 100), Some((frost, true)));
        for bad in ["", "  \n", "D:/a\u{7}b", "games/frost", "--launcher", "."] {
            std::fs::write(open_file(&dir, 100), bad).unwrap();
            assert_eq!(take_open(&dir, 100), None, "{bad:?}");
            assert!(!open_file(&dir, 100).exists(), "taken all the same");
        }
        std::fs::write(open_file(&dir, 100), "x".repeat(crate::launch_list::MAX_PATH_BYTES + 1)).unwrap();
        assert!(take_open(&dir, 100).is_none() && !open_file(&dir, 100).exists(), "too long: taken, nothing given");
        assert!(!take_refused(&dir, 100));
        refuse_open(&dir, 100);
        assert!(take_refused(&dir, 100) && !take_refused(&dir, 100));
        // A gone engine's hand-over and refusal go with it.
        request_open(&dir, 4321, Path::new("D:/x"), false).unwrap();
        refuse_open(&dir, 4321);
        list(&dir, |_| true);
        assert!(!open_file(&dir, 4321).exists() && !refused_file(&dir, 4321).exists());
        // The start screen's flag is written only when set, and read back.
        let l = Instance { launcher: true, project: None, ..instance(7, 7500, 1) };
        assert!(l.to_json().to_string().contains(r#""launcher":true"#) && !instance(7, 7500, 1).to_json().to_string().contains("launcher"));
        assert_eq!(Instance::parse(&l.to_json().to_string()), Some(l));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review R1-4 and R1-9: resume tokens never hide an engine or get wiped with it, and a
    /// file that does not read as an instance is left alone while its process runs.
    #[test]
    fn tokens_never_hide_an_engine_and_a_running_engines_file_is_never_judged_unread() {
        let dir = scratch("tokens");
        let engine = instance(4000, 7471, 10);
        write(&dir, &engine).unwrap();
        let folder = dir.join(FOLDER);
        // A swarm's worth of `call --as` names: every one sorts before `4000.json`.
        for n in 0..(MAX_INSTANCES + 40) {
            std::fs::write(folder.join(format!("4000.agent-{n:03}.token")), "t").unwrap();
        }
        let (found, stale) = list_with(&dir, |_| true, |_| false);
        assert_eq!((found, stale), (vec![engine.clone()], 0), "the engine is found behind its tokens");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), MAX_INSTANCES + 41, "and no token of a listed engine is swept");
        // More instance files than one pass looks at: what belongs to the ones not looked at
        // is not swept either.
        for pid in 5000..5000 + MAX_INSTANCES as u32 + 3 {
            write(&dir, &instance(pid, 7000, 1)).unwrap();
            std::fs::write(folder.join(format!("{pid}.ann.token")), "t").unwrap();
        }
        let (found, _) = list_with(&dir, |_| true, |_| false);
        assert_eq!(found.len(), MAX_INSTANCES);
        let tokens = std::fs::read_dir(&folder).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".ann.token")).count();
        assert_eq!(tokens, MAX_INSTANCES + 3, "every engine's token is still there");
        // A file caught before it reads as an instance (review R1-9): kept while its process
        // runs, removed (with what was left beside it) once it does not.
        for pid in 5000..5000 + MAX_INSTANCES as u32 + 3 {
            remove(&dir, pid);
        }
        let half = folder.join("6000.json");
        std::fs::write(&half, "{ \"schema\": \"specula.inst").unwrap();
        std::fs::write(folder.join("6000.stop"), "stop").unwrap();
        list_with(&dir, |_| true, |pid| pid == 6000);
        assert!(half.is_file() && folder.join("6000.stop").is_file(), "a running process's file is not judged from half of it");
        list_with(&dir, |_| true, |_| false);
        assert!(!half.exists() && !folder.join("6000.stop").exists());
        // The file an engine writes is whole or absent: no temporary file is left beside it.
        assert!(!std::fs::read_dir(&folder).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains("tmp")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review R1-3: an engine is serving when its own process owns the listening socket.
    #[test]
    fn serving_means_this_process_owns_the_port() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mine = Instance { pid: std::process::id(), port, ..instance(1, 1, 1) };
        if specula_sys::net::tcp_listeners().is_err() {
            // (No listener table on this platform yet: a running process and an answering port.)
            assert!(serving(&mine));
            return;
        }
        assert!(serving(&mine) && port_taken(port));
        // The same port under another process's id (an engine that died, its port taken by
        // this test's listener): not serving, though something answers there.
        let dead = Instance { pid: std::process::id() ^ 0x4000_0000, ..mine.clone() };
        assert!(answers(port) && !serving(&dead));
        drop(listener);
    }

    #[test]
    fn a_listening_port_answers() {
        // (That a closed port doesn't is not asserted here: a port this test frees can be taken
        // by another test at once. `list`'s `alive` is what the tests above turn off.)
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        assert!(answers(listener.local_addr().unwrap().port()));
    }
}
