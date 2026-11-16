//! Live checks written in Rust (AGENTS.md rule 16: a new check is Rust, here), run by `examen
//! live` beside the scripts that are still to be ported.
//!
//! A native check drives the engine as an agent with a shell does: through the engine's own
//! executable (`specula new`, an engine started on a project, `specula call TOOL '{json}'`,
//! `specula stop`). It links nothing of the engine. Everything it makes is below its own output
//! folder, engines included (their settings, captures, cache and trace), and every engine it
//! started is stopped when it ends, however it ends: by its own process id.
//!
//! **A check tries once.** A call that fails is the finding. Waiting is for a state the engine
//! reaches on its own (it listens; a module the start-up build is loading), with a bound, and
//! what was waited for is said.

use std::fs::File;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use specula_core::Json;

use crate::run::{said, stop_tree};

/// One native check.
pub struct Check {
    pub name: &'static str,
    /// What it shows, in a line.
    pub about: &'static str,
    /// Runs it. `Ok`: its last line (what passed); `Err`: what failed.
    pub run: fn(&mut Cx) -> Result<String, String>,
}

/// The native checks, in the order they run (after the scripts).
pub const CHECKS: &[Check] = &[Check {
    name: "new_project_check",
    about: "a project made by `specula new` builds, plays, remembers and follows its game file (docs/design/FIRST_HOUR.md)",
    run: new_project,
}];

pub fn find(name: &str) -> Option<&'static Check> {
    CHECKS.iter().find(|c| c.name == name)
}

/// How long one command of the engine's executable may take (a first module build compiles
/// the SDK; a playtest waits up to 20 s itself).
const COMMAND: Duration = Duration::from_secs(300);
/// How long an engine may take to listen, and to stop.
const START: Duration = Duration::from_secs(90);
const STOP: Duration = Duration::from_secs(30);

/// What a check works with: its folder, its ports, its log, and the engines it started.
pub struct Cx {
    /// The check's own folder: everything it makes is below it.
    pub out: PathBuf,
    /// Its first port (it owns the eight from it).
    pub port: u64,
    /// The engine's executable.
    exe: PathBuf,
    log: Vec<String>,
    engines: Vec<(u64, Child)>,
    /// Engines started so far.
    started: usize,
}

impl Drop for Cx {
    fn drop(&mut self) {
        for (_, child) in &mut self.engines {
            stop_tree(child);
        }
    }
}

/// A value inside a JSON answer by a dotted path (`reload.status`, `problems.0.at`).
pub fn at<'j>(json: &'j Json, path: &str) -> Option<&'j Json> {
    path.split('.').try_fold(json, |j, part| match part.parse::<usize>() {
        Ok(i) => j.as_array().and_then(|a| a.get(i)),
        Err(_) => j.get(part),
    })
}

fn text<'j>(json: &'j Json, path: &str) -> Option<&'j str> {
    at(json, path).and_then(Json::as_str)
}

fn short(json: &Json) -> String {
    let all = json.to_string();
    if all.chars().count() > 700 {
        format!("{} ...", all.chars().take(700).collect::<String>())
    } else {
        all
    }
}

/// `Err(what)` with the answer it was read from, unless `ok`.
fn expect(ok: bool, what: &str, answer: &Json) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{what}; the answer was: {}", short(answer)))
    }
}

impl Cx {
    pub fn new(out: &Path, port: u64) -> Cx {
        let exe = crate::run::target_dir().join("debug").join(if cfg!(windows) { "specula.exe" } else { "specula" });
        Cx { out: out.to_path_buf(), port, exe, log: Vec::new(), engines: Vec::new(), started: 0 }
    }

    pub fn say(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
    }

    /// The log so far, as the check's log file's text.
    pub fn log_text(&self) -> String {
        let mut text = self.log.join("\n");
        text.push('\n');
        text
    }

    fn settings(&self) -> String {
        self.out.join("settings").to_string_lossy().into_owned()
    }

    /// One command of the engine's executable: its exit code and its answer (its standard
    /// output, as JSON). Run in the check's own folder.
    pub fn cli(&mut self, args: &[&str]) -> Result<(i32, Json), String> {
        let exe = self.exe.to_string_lossy().into_owned();
        let mut cmd: Vec<&str> = vec![&exe];
        cmd.extend(args);
        let shown = args.join(" ");
        let Some((code, bytes)) = said(&cmd, &self.out, COMMAND) else {
            return Err(format!("`specula {shown}` could not be run, or did not end within {} s ({})", COMMAND.as_secs(), self.exe.display()));
        };
        let answer = String::from_utf8_lossy(&bytes);
        let json = Json::parse(answer.trim()).map_err(|e| format!("`specula {shown}` (exit {code}) did not answer JSON ({e}): {}", answer.chars().take(400).collect::<String>()))?;
        self.say(format!("$ specula {shown}\n  exit {code}: {}", short(&json)));
        Ok((code, json))
    }

    /// One tool call as `who` (an agent's name; `role`: the role it says), on the engine at
    /// `port`. `Ok`: the answer; `Err`: the refusal (the tool answered an error) or why the
    /// command did not run.
    pub fn call_as(&mut self, port: u64, who: Option<(&str, &str)>, tool: &str, args: &str) -> Result<Result<Json, Json>, String> {
        let (port, settings) = (port.to_string(), self.settings());
        let mut words = vec!["call", tool, args, "--port", &port, "--settings", &settings];
        if let Some((name, role)) = who {
            words.extend(["--as", name, "--role", role]);
        }
        let (code, json) = self.cli(&words)?;
        Ok(if code == 0 { Ok(json) } else { Err(json) })
    }

    /// A call that must be answered, not refused.
    pub fn call(&mut self, port: u64, who: Option<(&str, &str)>, tool: &str, args: &str) -> Result<Json, String> {
        self.call_as(port, who, tool, args)?.map_err(|refused| format!("{tool} {args} was refused: {}", short(&refused)))
    }

    /// A call that must be refused with `code`.
    pub fn refused(&mut self, port: u64, who: Option<(&str, &str)>, tool: &str, args: &str, code: &str) -> Result<Json, String> {
        match self.call_as(port, who, tool, args)? {
            Err(refused) if text(&refused, "error") == Some(code) => Ok(refused),
            Err(refused) => Err(format!("{tool} {args} was refused, but not with '{code}': {}", short(&refused))),
            Ok(answer) => Err(format!("{tool} {args} was answered, where '{code}' was expected: {}", short(&answer))),
        }
    }

    /// Starts a headless engine on `project` at `port` with `flags`, everything it keeps below
    /// the check's folder, and waits until it listens. It is stopped when the check ends.
    pub fn start(&mut self, project: &Path, port: u64, flags: &[&str]) -> Result<(), String> {
        // (Numbered by how many were started, not by how many are running: an engine started
        // after another was stopped has a log of its own.)
        self.started += 1;
        let n = self.started;
        let log = self.out.join(format!("engine-{n}.log"));
        let file = File::create(&log).and_then(|f| Ok((f.try_clone()?, f))).map_err(|e| format!("{}: {e}", log.display()))?;
        let own = |name: &str| self.out.join(name).to_string_lossy().into_owned();
        let mut cmd = Command::new(&self.exe);
        cmd.arg("--project").arg(project).args(["--headless", "--port", &port.to_string()]);
        cmd.args(["--settings", &own("settings"), "--captures", &own("captures"), "--cache", &own("cache"), "--trace", &own("trace")]);
        cmd.args(flags).current_dir(project).stdin(Stdio::null()).stdout(file.0).stderr(file.1);
        let mut child = cmd.spawn().map_err(|e| format!("the engine could not be started ({}): {e}", self.exe.display()))?;
        self.say(format!("engine {n} on port {port}: --project {} {} (log: {})", project.display(), flags.join(" "), log.display()));
        // It listens before its loop begins, and a call sent then waits for the loop.
        let addr = SocketAddr::from(([127, 0, 0, 1], port as u16));
        let began = Instant::now();
        loop {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
                break;
            }
            if let Ok(Some(status)) = child.try_wait() {
                let said = std::fs::read_to_string(&log).unwrap_or_default();
                return Err(format!("the engine ended before it listened ({status}): {}", said.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect::<String>()));
            }
            if began.elapsed() > START {
                stop_tree(&mut child);
                return Err(format!("the engine did not listen on port {port} within {} s ({})", START.as_secs(), log.display()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.engines.push((port, child));
        Ok(())
    }

    /// Asks the engine at `port` to stop (`specula stop`) and waits for its process to end
    /// cleanly: exit code 0.
    pub fn stop(&mut self, port: u64) -> Result<(), String> {
        let (p, settings) = (port.to_string(), self.settings());
        let (code, answer) = self.cli(&["stop", "--port", &p, "--settings", &settings])?;
        expect(code == 0 && at(&answer, "stopped").and_then(Json::as_bool) == Some(true), "`specula stop` did not stop the engine", &answer)?;
        let Some(i) = self.engines.iter().position(|(port_, _)| *port_ == port) else { return Ok(()) };
        let (_, mut child) = self.engines.remove(i);
        let began = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(format!("the engine on port {port} stopped with {status}, not cleanly")),
                Ok(None) if began.elapsed() > STOP => {
                    stop_tree(&mut child);
                    return Err(format!("the engine on port {port} was asked to stop and was still running {} s later", STOP.as_secs()));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(format!("the engine on port {port}: {e}")),
            }
        }
    }

    /// Waits until `tool` answers something `done` accepts, asking every half second for at
    /// most `limit`: for a state the engine reaches on its own. `what`: what is waited for.
    pub fn wait_for(&mut self, port: u64, who: Option<(&str, &str)>, tool: &str, what: &str, limit: Duration, done: impl Fn(&Json) -> bool) -> Result<Json, String> {
        let began = Instant::now();
        loop {
            let answer = self.call(port, who, tool, "{}")?;
            if done(&answer) {
                return Ok(answer);
            }
            if began.elapsed() > limit {
                return Err(format!("{what}: not so within {} s; the last answer was: {}", limit.as_secs(), short(&answer)));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

/// Runs `check` in `out` on `port` and writes its log to `log`. Returns whether it passed and
/// its last line.
pub fn run(check: &Check, out: &Path, port: u64, log: &Path) -> (bool, String) {
    if let Err(e) = std::fs::create_dir_all(out) {
        return (false, format!("cannot make {}: {e}", out.display()));
    }
    let mut cx = Cx::new(out, port);
    cx.say(format!("{}: {}", check.name, check.about));
    let result = (check.run)(&mut cx);
    let last = match &result {
        Ok(passed) => format!("PASS: {passed}"),
        Err(failed) => format!("FAIL: {failed}"),
    };
    cx.say(last.clone());
    let _ = std::fs::write(log, cx.log_text());
    // (Dropped here: every engine it started is stopped.)
    drop(cx);
    (result.is_ok(), last)
}

/// A project made by `specula new` builds, plays, remembers and follows its game file: the
/// first hour of a game built through the engine, end to end (docs/design/FIRST_HOUR.md).
fn new_project(cx: &mut Cx) -> Result<String, String> {
    let lead = Some(("lead", "lead"));
    let (port, settings) = (cx.port, cx.settings());
    let projects = cx.out.join("p");
    let project = projects.join("tapfive");
    if project.exists() {
        return Err(format!("{} is there already: give the check an output folder of its own (it never deletes one)", project.display()));
    }

    // 1. `specula new`: a project with every part, listed, with what to read first.
    let (code, made) = cx.cli(&["new", "tapfive", "--in", &projects.to_string_lossy(), "--settings", &settings])?;
    expect(code == 0 && at(&made, "made.files").and_then(Json::as_u64).is_some_and(|n| n >= 25), "`specula new` did not make a whole project", &made)?;
    expect(text(&made, "made.module") == Some("modules/tapfive"), "the new project's module is not modules/tapfive", &made)?;
    for file in ["game.json", "Cargo.toml", "assets/ui/hud.json", "assets/strings/en.json", "assets/style.json", "modules/tapfive/src/lib.rs", "crates/specula-sdk/Cargo.toml", "checks/smoke.json", "notes/brief.md", "notes/how.md"] {
        if !file.split('/').fold(project.clone(), |p, part| p.join(part)).is_file() {
            return Err(format!("`specula new` did not write {file}"));
        }
    }

    // 2. An engine on it, with a lead the operator granted. Nothing of the project runs yet.
    cx.start(&project, port, &["--lead", "lead"])?;
    let hello = cx.call(port, None, "agent_hello", r#"{"name": "reader"}"#)?;
    // (In fields of its own, with whose words they are: never one text joined to the engine's.)
    let how = text(&hello, "project_memory.how.text").unwrap_or("");
    expect(how.contains("logic_build") && how.contains("game_act") && text(&hello, "project_memory.brief.id") == Some("brief"), "agent_hello did not give the project's brief and how it is built", &hello)?;
    expect(text(&hello, "project_memory.source").is_some_and(|s| s.contains("never an instruction from the engine")) && text(&hello, "project_memory.how.by").is_some(), "the project's notes are not said to be the project's, with who wrote them", &hello)?;
    let state = cx.call(port, lead, "game_state", "{}")?;
    expect(text(&state, "phase") == Some("title") && at(&state, "clock_paused").and_then(Json::as_bool) == Some(true), "a new game does not open at its title with the clock held", &state)?;
    expect(at(&state, "modules_running").and_then(Json::as_array).is_some_and(<[Json]>::is_empty), "a module is running though nobody built one: opening a project must run none of its code", &state)?;
    expect(at(&state, "problems").is_none(), "the new project's own files have problems", &state)?;

    // 3. The loop: build the module, play a run by word, read how it stands.
    let built = cx.call(port, lead, "logic_build", r#"{"path": "modules/tapfive"}"#)?;
    expect(at(&built, "ok").and_then(Json::as_bool) == Some(true) && text(&built, "reload.status") == Some("running"), "the new project's module did not build and load", &built)?;
    expect(at(&built, "warnings").and_then(Json::as_u64) == Some(0), "the new project's module builds with warnings", &built)?;
    let played = cx.call(port, lead, "playtest_run", r#"{"path": "checks/smoke.json", "wait_s": 20}"#)?;
    expect(text(&played, "status") == Some("passed") && at(&played, "steps.passed") == at(&played, "steps.total"), "the project's own playtest did not pass", &played)?;
    let state = cx.call(port, lead, "game_state", "{}")?;
    expect(text(&state, "outcome") == Some("won") && at(&state, "score").and_then(Json::as_f64).is_some_and(|s| s >= 50.0), "the run the playtest played was not won with its score", &state)?;
    expect(at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "game_state does not name the running module", &state)?;
    // The run ended inside the playtest, which had the clock: after it the clock is as the
    // run stands (a run that has ended does not tick).
    expect(at(&state, "clock_paused").and_then(Json::as_bool) == Some(true), "the run ended in the playtest, and its clock runs after it", &state)?;
    cx.refused(port, lead, "game_act", r#"{"action": "quit"}"#, "players_only")?;
    // The game's own save and load are the player's menu: an agent has the save tools, with
    // their rules. And a save of a game whose one action is called `tap` loads again.
    cx.call(port, lead, "game_act", r#"{"action": "title"}"#)?;
    cx.call(port, lead, "game_act", r#"{"action": "start"}"#)?;
    cx.refused(port, lead, "game_act", r#"{"action": "save:slot1"}"#, "use_save_tools")?;
    cx.refused(port, lead, "game_act", r#"{"action": "load:slot1"}"#, "use_save_tools")?;
    cx.call(port, lead, "save_write", r#"{"name": "check1"}"#)?;
    cx.call(port, lead, "save_read", r#"{"name": "check1", "force": true}"#)?;
    let state = cx.call(port, lead, "game_state", "{}")?;
    expect(text(&state, "phase") == Some("running") && at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "a save of a run did not bring the run back", &state)?;
    // A game file that is not one is never stored, draft or not: the project would not open.
    cx.refused(port, lead, "project_write", r#"{"path": "game.json", "text": "{\"format\": \"specula.game.v1\", \"tick_hz\": 0}", "allow_errors": true}"#, "invalid")?;
    cx.refused(port, lead, "game_act", r#"{"action": "open:"}"#, "bad_action")?;

    // 4. The game file is followed while the game runs: three taps win now; a wrong key is
    // refused by name. Stepped by hand, on a paused clock, so the ticks are the check's.
    let patched = cx.call(port, lead, "project_patch", r#"{"path": "game.json", "ops": [{"set": "outcome.won.gte", "value": 3}]}"#)?;
    expect(text(&patched, "kind") == Some("game") && at(&patched, "reloaded").is_some_and(|r| r.to_string().contains("game")), "a change to game.json did not reach the game", &patched)?;
    let wrong = cx.refused(port, lead, "project_patch", r#"{"path": "game.json", "ops": [{"set": "tick_rate", "value": 60}]}"#, "invalid")?;
    expect(text(&wrong, "problems.0.at") == Some("tick_rate"), "a wrong key in game.json is not named", &wrong)?;
    cx.call(port, lead, "game_act", r#"{"action": "title"}"#)?;
    let started = cx.call(port, lead, "game_act", r#"{"action": "start"}"#)?;
    expect(text(&started, "phase") == Some("running") && at(&started, "clock_paused").and_then(Json::as_bool) == Some(false), "a run started by word does not run", &started)?;
    cx.call(port, lead, "time_control", r#"{"action": "pause"}"#)?;
    cx.call(port, lead, "time_control", r#"{"action": "step", "ticks": 2}"#)?;
    for _ in 0..3 {
        cx.call(port, lead, "game_act", r#"{"action": "tap"}"#)?;
        cx.call(port, lead, "time_control", r#"{"action": "step", "ticks": 2}"#)?;
    }
    let state = cx.call(port, lead, "game_state", "{}")?;
    // (A key the outcome reads holds a dot: read by its whole name.)
    let taps = at(&state, "reads").and_then(|r| r.get("tapfive.taps")).and_then(Json::as_f64);
    expect(text(&state, "outcome") == Some("won") && taps == Some(3.0), "three taps did not win after the game file said three", &state)?;

    // 5. The interface: the settings open over the title, and take the focus.
    cx.call(port, lead, "game_act", r#"{"action": "title"}"#)?;
    cx.call(port, lead, "game_act", r#"{"action": "open:settings"}"#)?;
    let frame = cx.call(port, lead, "ui_frame", r#"{"size": [1280, 720]}"#)?;
    let outlines = at(&frame, "overlays.ui.outlines").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    let flagged = |flag: &str| -> Vec<(&str, &str)> { outlines.iter().filter(|o| at(o, flag).and_then(Json::as_bool) == Some(true)).filter_map(|o| Some((text(o, "screen")?, text(o, "id")?))).collect() };
    let (focused, blocked) = (flagged("focused"), flagged("blocked"));
    expect(focused == [("settings", "volume")], &format!("the settings screen does not have the focus over the title (focused: {focused:?})"), &Json::arr(outlines.iter().take(12).cloned()))?;
    expect(blocked.len() == 3 && blocked.iter().all(|(screen, _)| *screen == "title"), &format!("the title's buttons are not blocked under the settings (blocked: {blocked:?})"), &Json::arr(outlines.iter().take(12).cloned()))?;
    if let Some(picture) = text(&frame, "image.path") {
        cx.say(format!("capture of the settings over the title: {picture}"));
    }
    let state = cx.call(port, lead, "game_state", "{}")?;
    expect(at(&state, "problems").is_none(), "the game reports problems", &state)?;
    cx.call(port, lead, "game_act", r#"{"action": "back"}"#)?;

    // 6. Memory: a note is kept with the project, as a file a person can read.
    let noted = cx.call(port, lead, "memory_note", r#"{"title": "Three taps win since the first check", "kind": "decision", "text": "The check changed the game file to three.", "about": ["game.json"]}"#)?;
    expect(text(&noted, "written") == Some("new") && text(&noted, "id") == Some("three-taps-win-since-the-first-check"), "a note was not written under its title's id", &noted)?;
    cx.refused(port, lead, "memory_note", r#"{"title": "Another brief", "kind": "brief"}"#, "brief_exists")?;
    let why = cx.call(port, lead, "memory_why", r#"{"path": "game.json"}"#)?;
    expect(text(&why, "notes.0.id") == Some("three-taps-win-since-the-first-check") && text(&why, "file.kind") == Some("game"), "memory_why does not lead with what was decided about the file", &why)?;
    if !project.join("notes").join("three-taps-win-since-the-first-check.md").is_file() {
        return Err("the note is not a file in the project's notes folder".to_owned());
    }

    // 7. A second language, chosen, and a setting the rules read: kept for the next start
    // (the operator named the settings folder, so an agent's change is kept there).
    let german = std::fs::read_to_string(project.join("assets").join("strings").join("en.json")).map_err(|e| format!("assets/strings/en.json: {e}"))?;
    let german = german.replace("\"locale\": \"en\"", "\"locale\": \"de\"").replace("\"name\": \"English\"", "\"name\": \"Deutsch\"").replace("\"source\": true,", "").replace("\"Start\"", "\"Los\"");
    let write = Json::obj([("path", Json::Str("assets/strings/de.json".to_owned())), ("text", Json::Str(german))]).to_string();
    cx.call(port, lead, "project_write", &write)?;
    let cycled = cx.call(port, lead, "game_act", r#"{"action": "cycle:language"}"#)?;
    expect(text(&cycled, "locale") == Some("de"), "the language did not change to the table just written", &cycled)?;
    cx.call(port, lead, "game_act", r#"{"action": "set:difficulty:hard"}"#)?;
    cx.stop(port)?;

    // 8. Again with `--run` and no lead at all: the operator says the code is theirs to run,
    // and the module is built and loaded at start. What was chosen and noted is still there.
    let port2 = port + 1;
    cx.start(&project, port2, &["--run"])?;
    let state = cx.wait_for(port2, None, "game_state", "--run loads the game file's module at start", Duration::from_secs(180), |s| at(s, "modules_running.0").and_then(Json::as_str) == Some("tapfive") || at(s, "problems").is_some())?;
    expect(at(&state, "problems").is_none() && at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "--run did not load the module", &state)?;
    expect(text(&state, "locale") == Some("de") && text(&state, "settings.difficulty") == Some("hard"), "the language and the difficulty were not kept for the next start", &state)?;
    let hello = cx.call(port2, None, "agent_hello", r#"{"name": "second"}"#)?;
    let also = at(&hello, "project_memory.also").map(Json::to_string).unwrap_or_default();
    expect(also.contains("Three taps win since the first check"), "the next agent is not told what the last one noted", &hello)?;
    // Nobody was granted anything: an agent's build is still only a proposal.
    cx.refused(port2, Some(("third", "builder")), "logic_build", r#"{"path": "modules/tapfive"}"#, "lead_only")?;
    // And an agent with no grant changes the module's source through the project tools, as
    // any builder may: written, journaled, and not built.
    let source = project.join("modules").join("tapfive").join("src").join("lib.rs");
    let changed = std::fs::read_to_string(&source).map_err(|e| format!("{}: {e}", source.display()))? + "\n// a builder's change, which nobody has reviewed\n";
    let args = cx.out.join("write-args.json");
    std::fs::write(&args, Json::obj([("path", Json::Str("modules/tapfive/src/lib.rs".to_owned())), ("text", Json::Str(changed))]).to_string()).map_err(|e| format!("{}: {e}", args.display()))?;
    let builder = Some(("carol", "builder"));
    cx.call(port2, builder, "project_mount", r#"{"module": "modules/tapfive"}"#)?;
    let wrote = cx.call(port2, builder, "project_write", &format!("@{}", args.to_string_lossy()))?;
    expect(text(&wrote, "disk") == Some("written"), "a builder's change to the module's source was not written", &wrote)?;
    cx.stop(port2)?;

    // 9. The review is not skipped by starting again (review R1 of M6 part 3, the critical
    // finding): `--run` runs only what an owner or a granted lead last built. The builder's
    // change is on disk, so the module is not loaded, and the game says why and what to do.
    let port3 = port + 2;
    cx.start(&project, port3, &["--run"])?;
    let done = |s: &Json| at(s, "problems").is_some() || at(s, "modules_running.0").is_some();
    let state = cx.wait_for(port3, None, "game_state", "--run decides about the changed module", Duration::from_secs(180), done)?;
    let said = at(&state, "problems").map(Json::to_string).unwrap_or_default();
    expect(at(&state, "modules_running").and_then(Json::as_array).is_some_and(<[Json]>::is_empty), "--run loaded a module whose sources a builder changed after its owner's last build", &state)?;
    expect(said.contains("was not loaded") && said.contains("changed since") && said.contains("--run-as-is"), "the game does not say why the module was not loaded, or what to do", &state)?;
    cx.stop(port3)?;

    // 10. `--run-as-is` is the operator saying that what is on disk now is theirs to run.
    let port4 = port + 3;
    cx.start(&project, port4, &["--run-as-is"])?;
    let state = cx.wait_for(port4, None, "game_state", "--run-as-is loads what is on disk", Duration::from_secs(180), done)?;
    expect(at(&state, "problems").is_none() && at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "--run-as-is did not load the module as it is on disk", &state)?;
    cx.stop(port4)?;
    Ok("new project: built, played, saved, changed, noted, loaded again by --run, and a builder's change held back until its owner builds it".to_owned())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_read_by_a_dotted_path() {
        let j = Json::parse(r#"{"reload": {"status": "running"}, "problems": [{"at": "tick_rate"}], "n": 3}"#).unwrap();
        assert_eq!(text(&j, "reload.status"), Some("running"));
        assert_eq!(text(&j, "problems.0.at"), Some("tick_rate"));
        assert_eq!(at(&j, "problems.1.at"), None);
        assert_eq!(at(&j, "n").and_then(Json::as_u64), Some(3));
        assert_eq!(at(&j, "reload.status.more"), None);
        assert!(expect(false, "it failed", &j).unwrap_err().starts_with("it failed; the answer was: {"));
        assert!(short(&Json::Str("x".repeat(5000))).ends_with(" ..."));
    }

    #[test]
    fn every_native_check_has_a_name_of_its_own() {
        let mut names: Vec<&str> = CHECKS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CHECKS.len());
        for check in CHECKS {
            assert!(check.name.ends_with("_check") && !check.about.is_empty() && find(check.name).is_some(), "{}", check.name);
            // (No script of the same name: `--only NAME` means one check.)
            assert!(!crate::run::root().join("scripts").join(format!("{}.py", check.name)).exists(), "{} is a script too", check.name);
        }
    }
}
