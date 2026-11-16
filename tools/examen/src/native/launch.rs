//! The launcher's live check (docs/design/LAUNCH.md): the project list, opening and resuming a
//! project, and finding, calling and stopping a running engine from a terminal. Ported from
//! `scripts/launch_check.py` on 2026-11-17, line for line where it could be, with one
//! expectation brought up to date (below).
//!
//! Everything a person or an agent with only a shell does, done with the real executable:
//!
//!   specula projects | new | open | list | tools | call | stop | forget | help
//!
//! - A machine that has never run a project lists none; a project worked on is listed, the last
//!   one first; `open --last` resumes it.
//! - `open` starts an engine on a project (or finds the one that has it open) and waits until it
//!   serves; `list` finds it; `tools` and `call` use it with no MCP client; `call --as NAME`
//!   keeps one identity across commands; `stop` shuts it down cleanly.
//! - An engine started by hand with `--project` is listed and found the same way; one started
//!   with `--settings off` (every check's engine) is neither.
//! - An engine `open` starts with a window opens behind the other windows and takes no keyboard.
//! - The start screen (Windows; `--launcher`, as the executable opened with no argument shows
//!   it): an empty list offers a new project, made and opened from the keyboard alone; the
//!   project last worked on is first and Enter resumes it; `specula open` hands a project to a
//!   start screen that is waiting, and its window becomes that project on the same port; Esc
//!   leaves a plain engine. The screen is drawn in the window only: no capture and no UI tool
//!   shows the list. Keys go to the engine's own window as posted messages (nothing here moves
//!   the pointer, presses a key for the person or takes the keyboard), and the pictures are of
//!   that window alone (`specula_platform::probe`).
//!
//! Nothing here touches the real per-user folder: every command and engine gets `--settings
//! DIR` inside the check's folder (and the start screen's engines a home folder there, for new
//! projects). Engines are headless except the window checks, whose windows open behind the
//! others.
//!
//! **Every expectation is tried, and the check fails at its end if one did not hold** (as the
//! script did): a launcher that lists wrongly still shows whether it stops cleanly.
//!
//! **Brought up to date in the port.** The script expected a project made from the start screen
//! to open on a window with no text in it. Since M6 part 3 a new project is a game with a title
//! screen, and its window shows that: the expectation is now that the window no longer shows
//! the start screen and that the engine says the project's game stands at its title.
//!
//! **Where it differs from the script otherwise.** A picture is waited for (bounded) where the
//! script slept a fixed time: the window shows what the expectation is about, or the wait ends
//! and the expectation fails on the last picture. A folder the check has run in before is
//! refused (the script cleared its folder; a check here deletes nothing it did not make this
//! run). It still runs the release executable, as the script did.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use specula_core::Json;
use specula_platform::probe::{self, Outside, Shot};

use super::Cx;
use crate::run::stop_tree;

/// The adapter the check's engines draw with (the script's default: the reference device).
const ADAPTER: &str = "integrated";

/// What was tried, and what did not hold.
struct Tally<'c> {
    cx: &'c mut Cx,
    fails: Vec<String>,
    tried: usize,
}

impl Tally<'_> {
    fn check(&mut self, ok: bool, what: impl Into<String>) -> bool {
        let what = what.into();
        self.tried += 1;
        self.cx.say(format!("{} {what}", if ok { "ok  " } else { "FAIL" }));
        if !ok {
            self.fails.push(what);
        }
        ok
    }
}

/// The executable's commands, always with one per-user folder.
#[derive(Clone)]
struct Cli {
    exe: PathBuf,
    user: PathBuf,
    /// Where commands are run.
    cwd: PathBuf,
}

/// The start of an answer, for a line of the log.
fn brief(j: &Json, most: usize) -> String {
    j.to_string().chars().take(most).collect()
}

impl Cli {
    /// (exit code, the JSON answer or `{"text": the raw text}`) of `specula WORDS --settings
    /// USER`, read through pipes as an agent's shell reads it. A command that does not answer in
    /// `limit` is ended and reported with no exit code: `open` once left its engine holding
    /// these pipes, and the reader waited for the engine to end.
    fn run_for(&self, words: &[&str], limit: Duration) -> (Option<i32>, Json) {
        let cut = words.iter().position(|w| *w == "--").unwrap_or(words.len());
        let mut cmd = Command::new(&self.exe);
        cmd.args(&words[..cut]).arg("--settings").arg(&self.user).args(&words[cut..]);
        self.answer(cmd, limit)
    }

    fn run(&self, words: &[&str]) -> (Option<i32>, Json) {
        self.run_for(words, Duration::from_secs(240))
    }

    /// Runs `cmd` in the check's folder and reads its answer.
    fn answer(&self, cmd: Command, limit: Duration) -> (Option<i32>, Json) {
        match self.said(cmd, limit) {
            Ok((code, out, err)) => {
                // (Exit code 2, "no engine reached", is said on standard error.)
                let first = if code == 2 { &err } else { &out };
                let text = if !first.is_empty() {
                    first
                } else if !err.is_empty() {
                    &err
                } else {
                    &out
                };
                (Some(code), Json::parse(text).unwrap_or_else(|_| Json::obj([("text", Json::from(text.as_str()))])))
            }
            Err(why) => (None, Json::obj([("error", Json::from("no_answer")), ("text", Json::from(why.as_str()))])),
        }
    }

    /// Runs `cmd` in the check's folder: its exit code, and what it wrote to its standard
    /// output and to its standard error. `Err`: it could not be started, did not end within
    /// `limit` (it is ended), or left its output held open.
    fn said(&self, mut cmd: Command, limit: Duration) -> Result<(i32, String, String), String> {
        let none = |why: String| Err(why);
        cmd.current_dir(&self.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => return none(format!("{} could not be started: {e}", self.exe.display())),
        };
        // Each pipe is read on a thread of its own, and the threads are waited for with a
        // bound: a pipe someone else still holds open never ends.
        let (tx, rx) = std::sync::mpsc::channel::<(bool, Vec<u8>)>();
        let read = |pipe: Option<Box<dyn Read + Send>>, is_out: bool| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_end(&mut bytes);
                }
                let _ = tx.send((is_out, bytes));
            });
        };
        read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), true);
        read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>), false);
        let began = Instant::now();
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) if began.elapsed() > limit => {
                    stop_tree(&mut child);
                    return none(format!("no answer in {} s", limit.as_secs()));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return none(format!("the command could not be waited for: {e}")),
            }
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        for _ in 0..2 {
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok((true, bytes)) => out = bytes,
                Ok((false, bytes)) => err = bytes,
                Err(_) => return none("the command ended and its output was still held open 20 s later".to_owned()),
            }
        }
        Ok((code, String::from_utf8_lossy(&out).trim().to_owned(), String::from_utf8_lossy(&err).trim().to_owned()))
    }

    fn engines(&self) -> Vec<Json> {
        self.run(&["list"]).1.get("engines").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default()
    }

    fn projects(&self) -> Json {
        self.run(&["projects"]).1
    }

    /// Whatever happened, no engine listed in this folder stays behind: asked to stop, then
    /// (by its own process id, read from its instance file) ended.
    fn leave_none(&self) {
        for _ in 0..4 {
            let Some(port) = self.engines().first().and_then(|e| e.get("port")).and_then(Json::as_u64) else { break };
            self.run_for(&["stop", "--port", &port.to_string()], Duration::from_secs(120));
        }
        for engine in self.engines() {
            if let Some(pid) = engine.get("pid").and_then(Json::as_u64) {
                end(pid as u32);
            }
        }
    }
}

/// The projects' names in an answer of `specula projects`, in its order.
fn names(answer: &Json) -> Vec<String> {
    answer.get("projects").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|p| p.get("name").and_then(Json::as_str)).map(str::to_owned).collect()
}

fn word<'j>(j: &'j Json, path: &str) -> Option<&'j str> {
    super::at(j, path).and_then(Json::as_str)
}

fn number(j: &Json, path: &str) -> Option<u64> {
    super::at(j, path).and_then(Json::as_u64)
}

fn flag(j: &Json, path: &str) -> Option<bool> {
    super::at(j, path).and_then(Json::as_bool)
}

/// Ends the process `pid`, and what it started (an engine of this check's own, by its id).
fn end(pid: u32) {
    #[cfg(windows)]
    let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    #[cfg(not(windows))]
    let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

/// Waits until `get` gives something, asking ten times a second for at most `limit`.
fn wait_for<T>(limit: Duration, mut get: impl FnMut() -> Option<T>) -> Option<T> {
    let began = Instant::now();
    loop {
        if let Some(found) = get() {
            return Some(found);
        }
        if began.elapsed() > limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// An engine started by hand (not by `specula open`), ended when it is dropped.
struct Hand {
    child: Child,
}

impl Hand {
    /// Starts the executable with `--port PORT` and `flags`, as a check starts one: with no
    /// settings unless the flags name a folder, and a window that takes no keyboard. Waits
    /// until it listens.
    fn start(cli: &Cli, port: u64, log: &str, flags: &[&str], env: &[(&str, String)]) -> Result<Hand, String> {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port as u16));
        // (Something else on the port would answer for this engine, and be sent its calls.)
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
            return Err(format!("port {port} is already served by something else: the check starts no engine there"));
        }
        let log = cli.cwd.join(log);
        let file = std::fs::File::create(&log).map_err(|e| format!("{}: {e}", log.display()))?;
        let mut cmd = Command::new(&cli.exe);
        // The first of a process group of its own, so that it can be asked to stop as a
        // person's Ctrl+Break asks (`Hand::interrupt`) without this process hearing it.
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, specula_sys::interrupt::NEW_GROUP);
        cmd.args(["--port", &port.to_string()]);
        if !flags.contains(&"--headless") && !flags.contains(&"--no-activate") {
            cmd.arg("--no-activate");
        }
        if !flags.contains(&"--settings") {
            cmd.args(["--settings", "off"]);
        }
        cmd.args(flags).current_dir(&cli.cwd).stdin(Stdio::null()).stdout(Stdio::null()).stderr(file);
        for (key, value) in env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().map_err(|e| format!("the engine could not be started ({}): {e}", cli.exe.display()))?;
        let began = Instant::now();
        loop {
            if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
                return Ok(Hand { child });
            }
            if let Ok(Some(status)) = child.try_wait() {
                let said = std::fs::read_to_string(&log).unwrap_or_default();
                let tail: String = said.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
                return Err(format!("the engine ended before it listened ({status}): {tail}"));
            }
            if began.elapsed() > Duration::from_secs(90) {
                stop_tree(&mut child);
                return Err(format!("the engine did not listen on port {port} within 90 s ({})", log.display()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Its exit code once it has ended by itself, waiting at most `limit`.
    fn ended(&mut self, limit: Duration) -> Option<i32> {
        wait_for(limit, || self.child.try_wait().ok().flatten()).map(|status| status.code().unwrap_or(-1))
    }

    fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Asks it to stop as a person's Ctrl+Break does, and waits for it: whether it was asked,
    /// and its exit code if it ended within half a minute.
    fn interrupt(&mut self) -> (bool, Option<i32>) {
        let asked = specula_sys::interrupt::ask(self.pid());
        (asked, self.ended(Duration::from_secs(30)))
    }
}

impl Drop for Hand {
    fn drop(&mut self) {
        if self.running() {
            stop_tree(&mut self.child);
        }
    }
}

// ---- the engine's own window ----

/// A key press and release posted to the engine's window, as a keyboard would send them.
fn press(window: Outside, key: (u32, u32)) {
    probe::key(window, key, true);
    std::thread::sleep(Duration::from_millis(50));
    probe::key(window, key, false);
    std::thread::sleep(Duration::from_millis(250));
}

/// Characters posted to the engine's window, as typing sends them.
fn type_text(window: Outside, text: &str) {
    for c in text.chars() {
        probe::character(window, c);
        std::thread::sleep(Duration::from_millis(30));
    }
    std::thread::sleep(Duration::from_millis(300));
}

/// A picture of the engine's window once it shows what `shows` looks for, written as a PNG:
/// asked for every 150 ms for at most 20 s, and taken once more a moment after it first shows
/// (a screen half drawn, a word half typed). When the wait ends without it, the last picture is
/// what is returned and written: the expectation made of it then fails on what the window did
/// show. `None`: the window could not be pictured at all.
fn shot_when(window: Outside, path: &Path, shows: impl Fn(&Shot) -> bool) -> Option<Shot> {
    let began = Instant::now();
    let mut last = None;
    loop {
        if let Ok(shot) = probe::shot(window) {
            let ready = shows(&shot);
            last = Some(shot);
            if ready {
                std::thread::sleep(Duration::from_millis(300));
                if let Ok(settled) = probe::shot(window) {
                    last = Some(settled);
                }
                break;
            }
        }
        if began.elapsed() > Duration::from_secs(20) {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let shot = last?;
    let rgba: Vec<u8> = shot.bgra.as_chunks::<4>().0.iter().flat_map(|p| [p[2], p[1], p[0], 255]).collect();
    let _ = std::fs::write(path, specula_core::png::encode_rgba8(shot.width, shot.height, &rgba, specula_core::deflate::Level::Fast));
    Some(shot)
}

/// Whether a picture shows something: more than two colours among its sampled pixels.
fn lively(shot: &Shot) -> bool {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for p in shot.bgra.as_chunks::<4>().0.iter().step_by(97) {
        let colour = [p[0], p[1], p[2]];
        if !seen.contains(&colour) {
            seen.push(colour);
            if seen.len() > 2 {
                return true;
            }
        }
    }
    false
}

/// The share of sampled pixels that differ clearly between two pictures of one window.
fn changed(a: &Shot, b: &Shot) -> f64 {
    if (a.width, a.height) != (b.width, b.height) {
        return 1.0;
    }
    let (mut n, mut d) = (0u32, 0u32);
    for (p, q) in a.bgra.as_chunks::<4>().0.iter().zip(b.bgra.as_chunks::<4>().0.iter()).step_by(53) {
        n += 1;
        let apart: u32 = (0..3).map(|i| u32::from(p[i].abs_diff(q[i]))).sum();
        d += u32::from(apart > 48);
    }
    f64::from(d) / f64::from(n.max(1))
}

/// Whether a picture of the engine's window has ivory text in it, as the start screen's is (an
/// empty world's window has no pixel that bright).
fn has_text(shot: &Shot) -> bool {
    shot.bgra.as_chunks::<4>().0.iter().step_by(7).filter(|p| p[0] > 170 && p[1] > 190 && p[2] > 190).count() >= 40
}

/// The share of pixels that differ clearly between two captures (0 when the files are the
/// same), and how many colours the first has among its sampled pixels.
fn captures_differ(a: &Path, b: &Path) -> Result<(f64, usize), String> {
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let (fa, fb) = (read(a)?, read(b)?);
    let decode = |bytes: &[u8], p: &Path| specula_core::png::decode(bytes).map_err(|e| format!("{}: {e:?}", p.display()));
    let ia = decode(&fa, a)?;
    let mut colours: Vec<[u8; 3]> = Vec::new();
    for p in ia.rgba.as_chunks::<4>().0.iter().step_by(7) {
        let colour = [p[0], p[1], p[2]];
        if !colours.contains(&colour) && colours.len() < 64 {
            colours.push(colour);
        }
    }
    if fa == fb {
        return Ok((0.0, colours.len()));
    }
    let ib = decode(&fb, b)?;
    if (ia.width, ia.height) != (ib.width, ib.height) {
        return Ok((1.0, colours.len()));
    }
    let differ = ia.rgba.as_chunks::<4>().0.iter().zip(ib.rgba.as_chunks::<4>().0.iter()).filter(|(p, q)| (0..3).map(|i| u32::from(p[i].abs_diff(q[i]))).sum::<u32>() > 24).count();
    Ok((differ as f64 / f64::from((ia.width * ia.height).max(1)), colours.len()))
}

/// The first visible window of the process `pid`, waited for (at most 30 s).
fn window_of(pid: u32) -> Option<Outside> {
    wait_for(Duration::from_secs(30), || probe::window_of(pid))
}

/// The start screen in a real window (the module's first lines say what is checked).
fn start_screen(t: &mut Tally<'_>, exe: &Path, out: &Path, port: u64) -> Result<Cli, String> {
    let (user, home, shots) = (out.join("user_start"), out.join("home"), out.join("start"));
    for dir in [&home, &shots] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let cli = Cli { exe: exe.to_path_buf(), user: user.clone(), cwd: out.to_path_buf() };
    // New projects go under the home folder: this run's own, never the person's.
    let home_text = home.to_string_lossy().into_owned();
    let env = [("USERPROFILE", home_text.clone()), ("HOME", home_text)];
    let user_text = user.to_string_lossy().into_owned();
    let flags = ["--launcher", "--settings", &user_text, "--adapter", ADAPTER, "--size", "1100x760"];
    let port_text = port.to_string();
    // The engine that took the start screen's place: another process, on its port, with the project.
    let became = |old_pid: u32, name: &str| {
        wait_for(Duration::from_secs(120), || cli.engines().into_iter().find(|e| number(e, "pid") != Some(u64::from(old_pid)) && word(e, "project.name") == Some(name)))
    };
    let frame = |args: &str| cli.run(&["call", "sight_frame", args, "--port", &port_text]).1;
    let copy = |from: Option<&str>, to: &str| {
        if let Some(from) = from {
            let _ = std::fs::copy(from, shots.join(to));
        }
    };
    const FRAME: &str = r#"{"width": 480, "height": 270, "include_ui": true, "response_format": "detailed"}"#;
    const FRAME_AT_0: &str = r#"{"width": 480, "height": 270, "include_ui": true, "time_s": 0, "response_format": "detailed"}"#;

    // ---- no project yet: the list is empty, and a project is made from the keyboard alone ----
    let empty = {
        let mut eng = Hand::start(&cli, port, "start_a.log", &flags, &env)?;
        let pid = eng.pid();
        let window = window_of(pid).ok_or("the start screen's engine showed no window within 30 s")?;
        let listed = cli.engines();
        t.check(listed.len() == 1 && flag(&listed[0], "launcher") == Some(true) && listed[0].get("project").is_none(), format!("an engine opened with no project is found as a start screen ({})", brief(&Json::arr(listed.clone()), 160)));
        let empty = shot_when(window, &shots.join("01_empty_list.png"), |s| lively(s) && has_text(s)).ok_or("the start screen's window could not be pictured")?;
        t.check(lively(&empty) && has_text(&empty), "the start screen is drawn (01_empty_list.png)");
        let shown = frame(FRAME);
        t.check(word(&shown, "image.path").is_some_and(|p| Path::new(p).is_file()), format!("a capture is taken while the start screen is open ({})", brief(&shown, 160)));
        copy(word(&shown, "image.path"), "01_capture_while_open.png");
        press(window, probe::ENTER); // the focus is on "New project"
        type_text(window, "tomb");
        let named = shot_when(window, &shots.join("02_new_project_name.png"), |s| changed(&empty, s) > 0.01).ok_or("the window could not be pictured")?;
        t.check(changed(&empty, &named) > 0.01, format!("Enter on an empty list asks for the new project's name, and typing shows ({:.3} of the window changed)", changed(&empty, &named)));
        press(window, probe::ENTER);
        let code = eng.ended(Duration::from_secs(120));
        t.check(code == Some(0), format!("the project made, the start screen's engine ends cleanly (exit code {code:?})"));
        let e = became(pid, "tomb");
        let tomb = home.join("Specula Projects").join("tomb");
        let e_ = e.clone().unwrap_or(Json::Null);
        t.check(number(&e_, "port") == Some(port) && flag(&e_, "launcher") != Some(true) && word(&e_, "project.path").map(Path::new) == Some(tomb.as_path()), format!("and the executable starts again in the new project, on the same port ({})", brief(&e_, 200)));
        t.check(tomb.join("specula.project.json").is_file() && tomb.join("assets").is_dir(), "the project is a folder in the home's Specula Projects, with its marker and an assets folder");
        if let Some(e) = &e {
            let its_own = |s: &Shot| lively(s) && changed(&empty, s) > 0.05 && changed(&named, s) > 0.05;
            let opened = number(e, "pid").and_then(|pid| window_of(pid as u32)).and_then(|w| shot_when(w, &shots.join("03_project_open.png"), its_own));
            // (Brought up to date in the port: the project's own title screen is text too.)
            t.check(opened.as_ref().is_some_and(its_own), "its window shows the project, with no start screen over it (03_project_open.png)");
            let (code, state) = cli.run(&["call", "game_state", "--port", &port_text]);
            t.check(code == Some(0) && word(&state, "phase") == Some("title"), format!("what it shows is the project's own game, at its title ({code:?} {})", brief(&state, 160)));
            let (code, info) = cli.run(&["call", "engine_info", "--port", &port_text]);
            t.check(code == Some(0) && word(&info, "project.name") == Some("tomb"), format!("engine_info says the project ({})", info.get("project").map(|p| brief(p, 160)).unwrap_or_default()));
        }
        let (code, j) = cli.run(&["stop"]);
        t.check(code == Some(0) && flag(&j, "stopped") == Some(true), format!("it stops when asked ({code:?} {})", brief(&j, 120)));
        empty
    };

    // ---- two projects: the last one worked on is first, and Enter resumes it ----
    let start_projects = out.join("projects_start").to_string_lossy().into_owned();
    let (code, _) = cli.run(&["new", "crypt", "--in", &start_projects]);
    t.check(code == Some(0) && names(&cli.projects()) == ["crypt", "tomb"], "a second project is made: it is the last one worked on");
    {
        let mut eng = Hand::start(&cli, port, "start_b.log", &flags, &env)?;
        let pid = eng.pid();
        let window = window_of(pid).ok_or("the start screen's engine showed no window within 30 s")?;
        let two = shot_when(window, &shots.join("04_two_projects.png"), |s| lively(s) && has_text(s) && changed(&empty, s) > 0.005).ok_or("the window could not be pictured")?;
        t.check(lively(&two) && has_text(&two) && changed(&empty, &two) > 0.005, "the list shows the projects (04_two_projects.png)");
        // No tool shows a person's project list: not a capture with the interface in it, not
        // the UI tools. ("tomb" is on the screen, in the list, and nowhere else in this engine.)
        let shown = frame(FRAME);
        let mut said = shown.to_string();
        let packet = word(&shown, "image.packet_path").and_then(|p| std::fs::read(p).ok()).map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        said += packet.as_deref().unwrap_or("");
        for tool in ["ui_tree", "ui_check", "engine_info", "world_list"] {
            // (A refusal is an answer too.)
            said += &cli.run(&["call", tool, "--port", &port_text]).1.to_string();
        }
        t.check(
            word(&shown, "image.path").is_some() && packet.as_ref().is_some_and(|p| !p.is_empty()) && !said.contains("tomb") && !said.contains("crypt"),
            format!("no capture packet and no tool answer names a listed project (the packet was read: {})", packet.is_some()),
        );
        copy(word(&shown, "image.path"), "04_capture_while_open.png");
        press(window, probe::ENTER); // the focus is on the first project
        let code = eng.ended(Duration::from_secs(120));
        let e = became(pid, "crypt").unwrap_or(Json::Null);
        t.check(code == Some(0) && number(&e, "port") == Some(port), format!("Enter resumes the project last worked on ({code:?} {})", brief(&e, 160)));
        let (code, j) = cli.run(&["stop"]);
        t.check(code == Some(0) && flag(&j, "stopped") == Some(true), "and it stops when asked");
    }

    // ---- `specula open` hands a project to a start screen that is waiting ----
    {
        let mut eng = Hand::start(&cli, port, "start_c.log", &flags, &env)?;
        let pid = eng.pid();
        // At once: the engine serves, its window may not have drawn a frame yet (the hand-over
        // waits for the start screen's first frame instead of being refused).
        let (code, j) = cli.run_for(&["open", "tomb"], Duration::from_secs(200));
        t.check(
            code == Some(0) && flag(&j, "from_start_screen") == Some(true) && number(&j, "port") == Some(port) && word(&j, "project.name") == Some("tomb") && number(&j, "pid") != Some(u64::from(pid)),
            format!("specula open hands the project to the waiting start screen: its window becomes the project, on its port ({code:?} {})", brief(&j, 200)),
        );
        std::thread::sleep(Duration::from_millis(1500));
        let its = number(&j, "pid").unwrap_or(0) as u32;
        let front = probe::foreground_pid();
        t.check(probe::window_of(its).is_some() && front != Some(its), format!("and that window opened behind the others: a command never takes the keyboard (the foreground window is process {front:?}'s)"));
        let code = eng.ended(Duration::from_secs(60));
        t.check(code == Some(0) && cli.engines().len() == 1, format!("one engine serves it, and the start screen's ended cleanly ({code:?})"));
        t.check(names(&cli.projects()) == ["tomb", "crypt"], "the project opened is first in the list now");
        let (code, j) = cli.run(&["stop"]);
        t.check(code == Some(0) && flag(&j, "stopped") == Some(true), "and it stops when asked");
    }

    // ---- Esc: a plain engine, with no project ----
    {
        let mut eng = Hand::start(&cli, port, "start_d.log", &flags, &env)?;
        let window = window_of(eng.pid()).ok_or("the start screen's engine showed no window within 30 s")?;
        // Something in the world, so the captures below show a scene (a capture of an empty
        // world would prove only that the start screen is absent from it).
        let (_, made) = cli.run(&["call", "world_spawn", r##"{"shape": "cube", "path": "/a", "color": "#d08040"}"##, "--as", "builder", "--port", &port_text]);
        let before = shot_when(window, &shots.join("05_before_close.png"), |s| lively(s) && has_text(s)).ok_or("the window could not be pictured")?;
        let shown = frame(FRAME_AT_0);
        copy(word(&shown, "image.path"), "05_capture_while_open.png");
        press(window, probe::ESCAPE);
        let after = shot_when(window, &shots.join("06_closed.png"), |s| !has_text(s)).ok_or("the window could not be pictured")?;
        t.check(has_text(&before) && !has_text(&after) && changed(&before, &after) > 0.05, format!("Esc closes the start screen ({:.3} of the window changed; 06_closed.png)", changed(&before, &after)));
        t.check(eng.running(), "and the engine runs on, with no project");
        // A capture never showed the start screen: the one taken while it was open is the same
        // picture of the scene as one taken now.
        let hidden = frame(FRAME_AT_0);
        copy(word(&hidden, "image.path"), "06_capture_closed.png");
        let compared = match (word(&shown, "image.path"), word(&hidden, "image.path")) {
            (Some(a), Some(b)) => captures_differ(Path::new(a), Path::new(b)),
            _ => Err(format!("a capture has no picture ({} / {})", brief(&shown, 120), brief(&hidden, 120))),
        };
        t.check(
            compared.as_ref().is_ok_and(|(differ, colours)| *differ < 0.002 && *colours >= 3),
            format!("a capture taken while the start screen was open shows the scene and no start screen: the same picture as one taken with it closed ({compared:?}: the share that differs, the colours)"),
        );
        let (_, info) = cli.run(&["call", "engine_info", "--port", &port_text]);
        t.check(info.get("project").is_none_or(Json::is_null) && word(&made, "path") == Some("/a"), format!("it is an engine like any other ({} {})", brief(&made, 100), info.get("project").map(|p| brief(p, 80)).unwrap_or_default()));
        let (code, j) = cli.run(&["stop"]);
        // (No wait: the command answers when the process has ended, not when it began to.)
        let ended = eng.child.try_wait().ok().flatten().map(|s| s.code().unwrap_or(-1));
        t.check(code == Some(0) && flag(&j, "stopped") == Some(true) && ended == Some(0), format!("specula stop ends it cleanly, and answers once it has ended ({code:?} {}, exit code {ended:?})", brief(&j, 120)));
    }
    Ok(cli)
}

/// The check itself. Engines listed in any of the run's per-user folders are ended whatever
/// happens.
pub fn launch(cx: &mut Cx) -> Result<String, String> {
    // (A command of this check starts engines that outlive it: they must not hold this
    // process's own output open.)
    specula_sys::process::keep_std_handles();
    // The release executable, as the script ran (the runner builds it before any check).
    let (out, exe) = (cx.out.clone(), crate::run::target_dir().join("release").join(if cfg!(windows) { "specula.exe" } else { "specula" }));
    if !exe.is_file() {
        return Err(format!("{} is not there: cargo build -p specula --release (the runner builds it unless told --no-build)", exe.display()));
    }
    // 7: a folder this check ran in before holds its projects and its lists.
    if out.join("user").exists() || out.join("projects").exists() {
        return Err(format!("{} was used by this check before: give it a folder of its own (--out)", out.display()));
    }
    let port = cx.port;
    let folders = [out.join("user"), out.join("user_rel"), out.join("user_start")];
    let mut t = Tally { cx, fails: Vec::new(), tried: 0 };
    let result = all(&mut t, &exe, &out, port);
    for user in folders {
        Cli { exe: exe.clone(), user, cwd: out.clone() }.leave_none();
    }
    result?;
    if t.fails.is_empty() {
        Ok(format!("the launcher: {} expectations held (projects listed and resumed, engines found, called and stopped from a terminal, the start screen in its window)", t.tried))
    } else {
        Err(format!("{} of {} expectations did not hold: {}", t.fails.len(), t.tried, t.fails.join(" | ")))
    }
}

fn all(t: &mut Tally<'_>, exe: &Path, out: &Path, port: u64) -> Result<(), String> {
    let projects = out.join("projects");
    std::fs::create_dir_all(&projects).map_err(|e| format!("{}: {e}", projects.display()))?;
    let user = out.join("user");
    let cli = Cli { exe: exe.to_path_buf(), user: user.clone(), cwd: out.to_path_buf() };
    let (p1, p2, p3, p4) = (port.to_string(), (port + 1).to_string(), (port + 2).to_string(), (port + 3).to_string());
    let in_projects = projects.to_string_lossy().into_owned();
    let code_is = |code: Option<i32>, want: i32| code == Some(want);
    let stopped = |j: &Json| flag(j, "stopped") == Some(true);
    // (Exactly the two keys, as the script compared: an empty list says nothing more.)
    let empty_list = |j: &Json, key: &str| j.as_object().is_some_and(|o| o.len() == 2) && j.get(key).and_then(Json::as_array).is_some_and(<[Json]>::is_empty) && number(j, "count") == Some(0);

    // ---- a machine that has never run a project ----
    let (code, j) = cli.run(&["projects"]);
    t.check(code_is(code, 0) && empty_list(&j, "projects"), format!("no project before: an empty list ({code:?} {})", brief(&j, 120)));
    let (code, j) = cli.run(&["list"]);
    t.check(code_is(code, 0) && empty_list(&j, "engines"), format!("no engine running: an empty list ({code:?} {})", brief(&j, 120)));
    let (code, j) = cli.run(&["open", "--last"]);
    t.check(code_is(code, 1) && word(&j, "error") == Some("no_projects"), format!("nothing to resume says so, with what to do ({code:?} {})", brief(&j, 160)));
    let (code, j) = cli.run(&["help"]);
    let help = word(&j, "text").unwrap_or("");
    t.check(code_is(code, 0) && ["projects", "open", "new", "forget", "list", "stop", "tools", "call"].iter().all(|w| help.contains(&format!("specula {w}"))), "help names every command");
    let (code, j) = cli.run(&["opne", "crypt"]);
    t.check(code_is(code, 1) && word(&j, "error") == Some("usage") && word(&j, "message").is_some_and(|m| m.contains("is not a command")), format!("a word that is no command is said, as JSON, never run as an engine ({code:?} {})", brief(&j, 120)));
    for words in [&["open"][..], &["call", "world_list", "[1]"], &["stop", "extra"]] {
        let (code, j) = cli.run(words);
        t.check(code_is(code, 1) && word(&j, "error") == Some("usage") && word(&j, "message").is_some_and(|m| !m.is_empty()), format!("'specula {}' is answered as JSON too ({code:?} {})", words.join(" "), brief(&j, 100)));
    }

    // ---- new projects, listed most recent first ----
    let (code, j) = cli.run(&["new", "crypt", "--in", &in_projects]);
    let crypt = PathBuf::from(word(&j, "project.path").unwrap_or(""));
    t.check(code_is(code, 0) && crypt == projects.join("crypt") && crypt.join("specula.project.json").is_file() && crypt.join("assets").is_dir(), format!("new makes the project's folder, its marker and its assets folder ({code:?} {})", brief(&j, 160)));
    let (_, j) = cli.run(&["new", "frost", "--in", &in_projects]);
    let frost = PathBuf::from(word(&j, "project.path").unwrap_or(""));
    let j = cli.projects();
    t.check(names(&j) == ["frost", "crypt"] && word(&j, "last") == Some("frost"), format!("the projects are listed, the last one worked on first ({:?})", names(&j)));
    let (code, j) = cli.run(&["new", "crypt", "--in", &in_projects]);
    t.check(code_is(code, 1) && j.to_string().contains("not empty"), "a folder in use is never taken for a new project");

    // ---- open: an engine on the project, found, used and stopped from the terminal ----
    let began = Instant::now();
    let (code, j) = cli.run_for(&["open", "crypt", "--", "--headless", "--adapter", ADAPTER, "--port", &p1], Duration::from_secs(200));
    let pid1 = number(&j, "pid");
    t.check(code.is_some(), format!("open answers and ends while its engine runs on: its output can be read through a pipe ({} s)", began.elapsed().as_secs()));
    t.check(
        code_is(code, 0) && number(&j, "port") == Some(port) && flag(&j, "started") == Some(true) && word(&j, "project.name") == Some("crypt") && flag(&j, "window") == Some(false),
        format!("open starts an engine on the project and answers once it serves ({code:?} {})", brief(&j, 200)),
    );
    let _ = std::fs::write(out.join("report.json"), Json::obj([("open", j.clone())]).to_pretty());
    let (code, j) = cli.run(&["list"]);
    t.check(code_is(code, 0) && number(&j, "count") == Some(1) && number(&j, "engines.0.port") == Some(port) && word(&j, "engines.0.url") == Some(format!("http://127.0.0.1:{port}/mcp").as_str()), format!("list finds it, with where to reach it ({})", brief(&j, 200)));
    let j = cli.projects();
    t.check(names(&j) == ["crypt", "frost"] && number(&j, "projects.0.running.port") == Some(port), format!("the project opened is first now, and says its engine is running ({:?})", names(&j)));
    let (code, j) = cli.run(&["open", "crypt"]);
    t.check(code_is(code, 0) && flag(&j, "started") == Some(false) && number(&j, "pid") == pid1, format!("open again finds the engine that has it open, and starts none ({code:?} {})", brief(&j, 120)));
    let (code, j) = cli.run(&["tools", "terrain", "--project", "crypt"]);
    let listed: Vec<&str> = j.get("tools").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|tool| word(tool, "name")).collect();
    t.check(code_is(code, 0) && listed.contains(&"terrain_edit") && number(&j, "port") == Some(port), format!("tools lists what the engine can do, filtered ({:?})", &listed[..listed.len().min(6)]));
    let (code, j) = cli.run(&["call", "engine_info", "--project", "crypt"]);
    t.check(code_is(code, 0) && word(&j, "project.name") == Some("crypt") && word(&j, "project.path").map(Path::new) == Some(crypt.as_path()), format!("call runs a tool with no MCP client; engine_info says the project ({})", j.get("project").map(|p| brief(p, 160)).unwrap_or_default()));
    let (code, j) = cli.run(&["call", "world_spawn", r#"{"shape": "cube", "path": "/a"}"#, "--as", "ann", "--port", &p1]);
    t.check(code_is(code, 0) && j.get("error").is_none(), format!("call --as ann spawns as ann ({code:?} {})", brief(&j, 120)));
    let (code, j) = cli.run(&["call", "world_spawn", r#"{"shape": "cube", "path": "/b"}"#, "--as", "ann", "--port", &p1]);
    t.check(code_is(code, 0) && j.get("error").is_none(), format!("a second command as ann is the same ann (its name is not taken) ({code:?} {})", brief(&j, 160)));
    let by = |j: &Json| -> Vec<String> { j.get("entries").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|e| word(e, "by")).map(str::to_owned).collect() };
    let (code, j) = cli.run(&["call", "world_history", r#"{"limit": 4}"#, "--port", &p1]);
    t.check(code_is(code, 0) && by(&j).iter().take(2).map(String::as_str).eq(["ann", "ann"]), format!("both edits are journaled by ann ({:?})", by(&j)));
    for _ in 0..5 {
        cli.run(&["call", "world_list", "--as", "ann", "--port", &p1]);
    }
    let (code, j) = cli.run(&["call", "engine_info", r#"{"response_format": "detailed"}"#, "--port", &p1]);
    let left = j.get("agents").and_then(Json::as_array).unwrap_or(&[]).iter().filter(|agent| word(agent, "client") == Some("specula-cli")).count();
    t.check(code_is(code, 0) && left <= 1, format!("a command's session ends with the command, named or not: none piles up in the engine ({left} of the commands' sessions in engine_info.agents)"));
    cli.run(&["call", "world_spawn", r#"{"shape": "cube", "path": "/c"}"#, "--as", "ann", "--port", &p1]);
    let (_, j) = cli.run(&["call", "world_history", r#"{"limit": 1}"#, "--port", &p1]);
    t.check(by(&j) == ["ann"], "and ann is still ann after her sessions closed");
    let (code, j) = cli.run(&["call", "no_such_tool", "--port", &p1]);
    t.check(code_is(code, 1), format!("a tool's refusal is exit code 1 ({code:?} {})", brief(&j, 100)));
    let (code, j) = cli.run(&["call", "world_list", "--port", &p4]);
    t.check(code_is(code, 2) && word(&j, "error") == Some("no_engine"), format!("no engine on a port is exit code 2, and says so ({code:?} {})", brief(&j, 140)));

    // ---- two engines: a command says which ----
    let (code, j) = cli.run(&["open", "frost", "--", "--headless", "--adapter", ADAPTER, "--port", &p2]);
    t.check(code_is(code, 0) && flag(&j, "started") == Some(true) && number(&j, "port") == Some(port + 1), format!("a second project opens in an engine of its own ({code:?} {})", brief(&j, 120)));
    let (code, j) = cli.run(&["tools"]);
    t.check(code_is(code, 1) && word(&j, "error") == Some("several_engines") && j.get("engines").and_then(Json::as_array).map(<[Json]>::len) == Some(2), format!("with two engines a command asks which ({code:?} {:?})", word(&j, "error")));
    let j = cli.projects();
    t.check(names(&j) == ["frost", "crypt"], format!("the last one opened is first ({:?})", names(&j)));
    let (code, j) = cli.run(&["stop", "--project", "frost"]);
    t.check(code_is(code, 0) && stopped(&j) && number(&j, "port") == Some(port + 1), format!("stop shuts the project's engine down ({code:?} {})", brief(&j, 120)));
    t.check(number(&j, "pid").is_some_and(|pid| !specula_sys::process_is_running(pid as u32)), "and answers once the engine's process has ended (not only stopped serving)");
    let (_, j) = cli.run(&["list"]);
    t.check(number(&j, "count") == Some(1) && number(&j, "engines.0.port") == Some(port), "it is no longer listed, and the other one is");
    let left_in = |ending: &str| std::fs::read_dir(user.join("engine.instances")).map(|dir| dir.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(ending)).count()).unwrap_or(0);
    t.check(left_in(".stop") == 0, "a stop request leaves nothing behind");
    let (code, j) = cli.run(&["stop"]);
    t.check(code_is(code, 0) && stopped(&j) && number(&j, "pid") == pid1, format!("stop with one engine running needs no name ({code:?} {})", brief(&j, 120)));
    let (_, j) = cli.run(&["list"]);
    t.check(empty_list(&j, "engines") && left_in(".json") == 0, format!("no engine is left, and no instance file ({})", brief(&j, 100)));

    // ---- resume the last one ----
    let (code, j) = cli.run(&["open", "--last", "--", "--headless", "--adapter", ADAPTER, "--port", &p1]);
    t.check(code_is(code, 0) && word(&j, "project.name") == Some("frost"), format!("open --last resumes the project last worked on ({:?})", word(&j, "project.name")));
    cli.run(&["stop"]);

    // ---- one engine a project: two commands at once start one engine ----
    let openers: Vec<_> = [p1.clone(), p2.clone()]
        .into_iter()
        .map(|port| {
            let cli = cli.clone();
            std::thread::spawn(move || cli.run(&["open", "crypt", "--", "--headless", "--adapter", ADAPTER, "--port", &port]))
        })
        .collect();
    let answers: Vec<(Option<i32>, Json)> = openers.into_iter().filter_map(|opener| opener.join().ok()).collect();
    let mut started: Vec<bool> = answers.iter().map(|(_, j)| flag(j, "started") == Some(true)).collect();
    started.sort_unstable();
    let mut pids: Vec<Option<u64>> = answers.iter().map(|(_, j)| number(j, "pid")).collect();
    pids.dedup();
    t.check(
        answers.len() == 2 && answers.iter().all(|(code, _)| *code == Some(0)) && started == [false, true] && pids.len() == 1,
        format!("two 'specula open' of one project at once: one starts the engine, the other is answered with it ({:?})", answers.iter().map(|(c, j)| (*c, flag(j, "started"), number(j, "pid"), word(j, "error").map(str::to_owned))).collect::<Vec<_>>()),
    );
    let (_, listed) = cli.run(&["list"]);
    t.check(number(&listed, "count") == Some(1), format!("and one engine has the project ({:?})", number(&listed, "count")));
    // By hand: a second engine on the project is refused, and says why.
    let mut second = Command::new(exe);
    second.arg("--project").arg(&crypt).arg("--settings").arg(&user).args(["--headless", "--adapter", ADAPTER, "--port", &p3]);
    let refused = cli.said(second, Duration::from_secs(180));
    let tail = |text: &str| text.chars().rev().take(160).collect::<Vec<_>>().into_iter().rev().collect::<String>();
    t.check(
        refused.as_ref().is_ok_and(|(code, _, err)| *code != 0 && err.contains("is open in another engine")),
        format!("an engine started by hand on a project that is open is refused, on its standard error ({})", refused.as_ref().map_or_else(Clone::clone, |(code, _, err)| format!("{code} {}", tail(err)))),
    );

    // ---- an engine that died: its file is not an engine, whoever has its port now ----
    let (dead_pid, dead_port) = (number(&listed, "engines.0.pid"), number(&listed, "engines.0.port"));
    if let (true, Some(dead_pid), Some(dead_port)) = (cfg!(windows), dead_pid, dead_port) {
        end(dead_pid as u32);
        wait_for(Duration::from_secs(30), || (!specula_sys::process_is_running(dead_pid as u32)).then_some(()));
        let file = user.join("engine.instances").join(format!("{dead_pid}.json"));
        t.check(file.is_file(), "an engine that is killed leaves its instance file behind");
        {
            let _other = Hand::start(&cli, dead_port, "engine_other.log", &["--headless", "--adapter", ADAPTER], &[])?;
            t.check(number(&cli.run(&["list"]).1, "count") == Some(0), "another program on its port is not taken for it");
            let began = Instant::now();
            let (code, j) = cli.run(&["stop", "--project", "crypt"]);
            let none = |j: &Json| matches!(word(j, "error"), Some("no_engine" | "not_running"));
            t.check(code_is(code, 2) && none(&j) && began.elapsed() < Duration::from_secs(30), format!("and no command is sent to that program in its name ({code:?} {:?} in {} s)", word(&j, "error"), began.elapsed().as_secs()));
            let (code, j) = cli.run(&["call", "engine_info", "--project", "crypt"]);
            t.check(code_is(code, 2) && none(&j), format!("a call for the project reaches nobody ({code:?} {})", brief(&j, 100)));
        }
        t.check(!file.exists(), "and its file is gone");
    } else {
        cli.run(&["stop"]);
    }

    // ---- a relative --settings is the folder the command was run in, not one in the project ----
    let mut rel = Command::new(exe);
    rel.arg("open").arg(&crypt).args(["--settings", "user_rel", "--", "--headless", "--adapter", ADAPTER, "--port", &p2]);
    let (code, j) = cli.answer(rel, Duration::from_secs(240));
    t.check(code_is(code, 0) && number(&j, "port") == Some(port + 1) && out.join("user_rel").join("projects.json").is_file() && !crypt.join("user_rel").exists(), format!("a relative --settings names one folder for the command and the engine it starts ({code:?} {})", brief(&j, 120)));
    let rel_cli = Cli { exe: exe.to_path_buf(), user: out.join("user_rel"), cwd: out.to_path_buf() };
    let (code, j) = rel_cli.run(&["stop"]);
    t.check(code_is(code, 0) && stopped(&j), "and that engine is found there and stops");

    // ---- --port 0: the port the engine was given is the one it says ----
    let (code, j) = cli.run(&["open", "crypt", "--", "--headless", "--adapter", ADAPTER, "--port", "0"]);
    t.check(code_is(code, 0) && number(&j, "port").is_some_and(|p| p != 0) && number(&cli.run(&["list"]).1, "count") == Some(1), format!("an engine asked for any port is found on the one it got ({code:?} {:?})", number(&j, "port")));
    cli.run(&["stop"]);

    // ---- an engine started by hand, and a check's engine ----
    let plain = projects.join("plain");
    std::fs::create_dir_all(&plain).map_err(|e| format!("{}: {e}", plain.display()))?;
    let (plain_text, user_text) = (plain.to_string_lossy().into_owned(), user.to_string_lossy().into_owned());
    {
        let mut eng = Hand::start(&cli, port + 2, "engine_hand.log", &["--headless", "--adapter", ADAPTER, "--project", &plain_text, "--settings", &user_text], &[])?;
        let (_, info) = cli.run(&["call", "engine_info", "--port", &p3]);
        t.check(word(&info, "project.name") == Some("plain") && plain.join("specula.project.json").is_file(), format!("--project opens a plain folder as a project and gives it its marker ({})", info.get("project").map(|p| brief(p, 160)).unwrap_or_default()));
        let (_, j) = cli.run(&["list"]);
        t.check(number(&j, "count") == Some(1) && word(&j, "engines.0.project.name") == Some("plain"), "an engine started by hand is found the same way");
        let j = cli.projects();
        t.check(names(&j).first().map(String::as_str) == Some("plain"), format!("and its project is first in the list ({:?})", names(&j)));
        // Ended as a person's Ctrl+Break in its console ends it (not by `specula stop`, which
        // is tried above): the interrupt's path takes the instance file with it too.
        let (asked, code) = eng.interrupt();
        t.check(asked && code == Some(0), format!("an interrupt (Ctrl+Break) ends it cleanly (asked: {asked}; exit code {code:?})"));
    }
    let (_, j) = cli.run(&["list"]);
    t.check(number(&j, "count") == Some(0), "it takes its instance file with it when it stops");
    {
        let _eng = Hand::start(&cli, port + 2, "engine_script.log", &["--headless", "--adapter", ADAPTER], &[])?;
        let (_, j) = cli.run(&["list"]);
        t.check(number(&j, "count") == Some(0), "an engine started with --settings off (every check's) is not listed");
        let (code, j) = cli.run(&["stop", "--port", &p3]);
        t.check(code_is(code, 1) && word(&j, "error") == Some("not_listed"), format!("and stop says it is stopped where it was started ({code:?} {:?})", word(&j, "error")));
    }

    // ---- a project whose folder is gone ----
    let _ = std::fs::remove_dir_all(&frost);
    let j = cli.projects();
    let gone = j.get("projects").and_then(Json::as_array).unwrap_or(&[]).iter().find(|p| word(p, "name") == Some("frost")).cloned().unwrap_or(Json::Null);
    t.check(flag(&gone, "missing") == Some(true), "a project whose folder is gone stays listed, as missing");
    let (code, j) = cli.run(&["open", "frost"]);
    t.check(code_is(code, 1) && word(&j, "error") == Some("missing"), format!("opening it says so ({code:?} {:?})", word(&j, "error")));
    let (code, _) = cli.run(&["forget", "frost"]);
    let after = names(&cli.projects());
    t.check(code_is(code, 0) && !after.iter().any(|n| n == "frost"), format!("forget takes it off the list ({after:?})"));

    // ---- a window: behind the others, taking no keyboard ----
    if cfg!(windows) {
        let (code, j) = cli.run(&["open", "crypt", "--", "--adapter", ADAPTER, "--port", &p1, "--size", "640x400"]);
        let pid = number(&j, "pid").unwrap_or(0) as u32;
        t.check(code_is(code, 0) && flag(&j, "window") == Some(true), format!("open with no --headless starts an engine with a window ({code:?} {})", brief(&j, 140)));
        std::thread::sleep(Duration::from_secs(1));
        let front = probe::foreground_pid();
        t.check(front != Some(pid), format!("its window opened behind the others and took no keyboard (the foreground window is another process's: {front:?} vs {pid})"));
        let (code, j) = cli.run(&["stop"]);
        let still = specula_sys::process_is_running(pid);
        t.check(code_is(code, 0) && stopped(&j) && !still, format!("and it stops when asked: its process is gone when the command answers ({code:?} {}, still running: {still})", brief(&j, 120)));
        // ---- the start screen ----
        start_screen(t, exe, out, port)?;
    }
    Ok(())
}
