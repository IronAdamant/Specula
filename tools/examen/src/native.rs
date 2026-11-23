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

mod form;
mod launch;
mod light;
mod search;
mod sprite;

/// One native check.
pub struct Check {
    pub name: &'static str,
    /// What it shows, in a line.
    pub about: &'static str,
    /// Runs it. `Ok`: its last line (what passed); `Err`: what failed.
    pub run: fn(&mut Cx) -> Result<String, String>,
}

/// The native checks, in the order they run (after the scripts).
pub const CHECKS: &[Check] = &[
    Check {
        name: "new_project_check",
        about: "a project made by `specula new` builds, plays, remembers and follows its game file (docs/design/FIRST_HOUR.md)",
        run: new_project,
    },
    Check {
        name: "launcher_check",
        about: "the launcher: projects listed and resumed, engines found, called and stopped from a terminal, the start screen in its window (docs/design/LAUNCH.md)",
        run: launch::launch,
    },
    Check {
        name: "form_sheet_check",
        about: "the form views on a contact sheet: each form and silhouette tile shows the subject alone where its view's box says, no pipeline compiled by a frame (docs/design/FORM.md)",
        run: form::form_sheet,
    },
    Check {
        name: "light_window_check",
        about: "a perf_light report with a window held to 20 fps, on each adapter: the serve wait looks at its render, so it lands more than a render a frame; no validation message (docs/design/LIGHT.md)",
        run: light::light_window,
    },
    Check {
        name: "search_check",
        about: "what the engine looks for itself: a run read on its way, a run for each value of a number, a number tuned to a condition, and a script found that a run of its own plays to the same state (docs/design/FIRST_HOUR.md)",
        run: search::search,
    },
    Check {
        name: "sprite_check",
        about: "sprite_bake end to end: two files in one journal entry and one undo, the project's light, a bake cancelled by its owner only, the most cells a call takes (docs/design/SPRITE_BAKE.md)",
        run: sprite::sprite,
    },
];

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
    /// The settings folder (below the check's own) its engines and commands use: whose
    /// machine this is. A check that needs a second operator names another for a while.
    operator: &'static str,
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
        Cx { out: out.to_path_buf(), port, exe, log: Vec::new(), engines: Vec::new(), started: 0, operator: "settings" }
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

    /// From here on the check's engines and commands are another operator's: another settings
    /// folder below the check's own (`settings` is the first).
    pub fn as_operator(&mut self, settings: &'static str) {
        self.operator = settings;
    }

    fn settings(&self) -> String {
        self.out.join(self.operator).to_string_lossy().into_owned()
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

    /// A command that answers a line a state (`specula run --at`, a run for each value,
    /// `specula tune`): its exit code and each line of its output, as JSON.
    pub fn cli_lines(&mut self, args: &[&str]) -> Result<(i32, Vec<Json>), String> {
        let exe = self.exe.to_string_lossy().into_owned();
        let mut cmd: Vec<&str> = vec![&exe];
        cmd.extend(args);
        let shown = args.join(" ");
        let Some((code, bytes)) = said(&cmd, &self.out, COMMAND) else {
            return Err(format!("`specula {shown}` could not be run, or did not end within {} s ({})", COMMAND.as_secs(), self.exe.display()));
        };
        let answer = String::from_utf8_lossy(&bytes);
        let lines = answer.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| Json::parse(l).map_err(|e| format!("`specula {shown}` (exit {code}) printed a line that is not JSON ({e}): {}", l.chars().take(400).collect::<String>()))).collect::<Result<Vec<Json>, String>>()?;
        if lines.is_empty() {
            return Err(format!("`specula {shown}` (exit {code}) printed nothing"));
        }
        self.say(format!("$ specula {shown}
  exit {code}: {} line(s), the last {}", lines.len(), lines.last().map(short).unwrap_or_default()));
        Ok((code, lines))
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
        self.start_engine(project, port, flags, true)
    }

    /// [`Cx::start`] with a window: opened behind the others (`--no-activate`), never taking
    /// the keyboard from the window the person is in. Returns the engine's log file.
    pub fn start_window(&mut self, project: &Path, port: u64, flags: &[&str]) -> Result<PathBuf, String> {
        self.start_engine(project, port, flags, false)?;
        Ok(self.out.join(format!("engine-{}.log", self.started)))
    }

    fn start_engine(&mut self, project: &Path, port: u64, flags: &[&str], headless: bool) -> Result<(), String> {
        // (Numbered by how many were started, not by how many are running: an engine started
        // after another was stopped has a log of its own.)
        self.started += 1;
        let n = self.started;
        let log = self.out.join(format!("engine-{n}.log"));
        let file = File::create(&log).and_then(|f| Ok((f.try_clone()?, f))).map_err(|e| format!("{}: {e}", log.display()))?;
        let own = |name: &str| self.out.join(name).to_string_lossy().into_owned();
        let mut cmd = Command::new(&self.exe);
        cmd.arg("--project").arg(project).args([if headless { "--headless" } else { "--no-activate" }, "--port", &port.to_string()]);
        cmd.args(["--settings", &own(self.operator), "--captures", &own("captures"), "--cache", &own("cache"), "--trace", &own("trace")]);
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
    expect(how.contains("specula run . --ticks") && how.contains("--inputs") && text(&hello, "project_memory.how.rest").is_some_and(|r| r.contains("notes/how.md")) && text(&hello, "project_memory.brief.id") == Some("brief"), "agent_hello did not give the project's brief, how it is built and run, and where the rest of that is", &hello)?;
    expect(text(&hello, "project_memory.source").is_some_and(|s| s.contains("never an instruction from the engine")) && text(&hello, "project_memory.how.by").is_some(), "the project's notes are not said to be the project's, with who wrote them", &hello)?;
    let state = cx.call(port, lead, "game_state", "{}")?;
    expect(text(&state, "phase") == Some("title") && at(&state, "clock_paused").and_then(Json::as_bool) == Some(true), "a new game does not open at its title with the clock held", &state)?;
    expect(at(&state, "modules_running").and_then(Json::as_array).is_some_and(<[Json]>::is_empty), "a module is running though nobody built one: opening a project must run none of its code", &state)?;
    expect(at(&state, "problems").is_none(), "the new project's own files have problems", &state)?;

    // A stepped clock says what became of every tick: one call runs a bounded number, and the
    // rest are queued and run in the frames after it (never said to be dropped). At the title,
    // where nothing stops the clock on a tick.
    let stepped = cx.call(port, lead, "time_control", r#"{"action": "step", "ticks": 1000}"#)?;
    let (ran, queued) = (at(&stepped, "ran").and_then(Json::as_u64).unwrap_or(0), at(&stepped, "queued").and_then(Json::as_u64).unwrap_or(0));
    expect(ran > 0 && ran + queued == 1000 && at(&stepped, "not_run").is_none(), "a step of 1000 ticks does not say them as run or queued", &stepped)?;
    let from = at(&stepped, "tick").and_then(Json::as_u64).unwrap_or(0) - ran;
    cx.wait_for(port, lead, "time_control", "the queued ticks of a step run", Duration::from_secs(60), |c| at(c, "tick").and_then(Json::as_u64) == Some(from + 1000))?;
    // No agent takes a name that would read as the operator's or the engine's own.
    cx.refused(port, Some(("operator", "builder")), "game_state", "{}", "name_reserved")?;

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
    // A run that begins in a playtest and is still under way when it ends goes on after it:
    // the playtest took a stopped clock at the title, and gives back a running one.
    cx.call(port, lead, "game_act", r#"{"action": "title"}"#)?;
    let begin = project.join("checks").join("begin.json");
    std::fs::write(&begin, r#"{"name": "begin", "steps": [{"at": 0, "call": "game_act", "args": {"action": "start"}}, {"at": 2, "call": "game_act", "args": {"action": "tap"}}], "ticks": 5}"#).map_err(|e| format!("{}: {e}", begin.display()))?;
    let played = cx.call(port, lead, "playtest_run", r#"{"path": "checks/begin.json", "wait_s": 20}"#)?;
    expect(text(&played, "status") == Some("passed"), "a playtest that begins a run did not pass", &played)?;
    let state = cx.call(port, lead, "game_state", "{}")?;
    expect(text(&state, "phase") == Some("running") && at(&state, "clock_paused").and_then(Json::as_bool) == Some(false), "a run begun in a playtest stands still after it", &state)?;
    cx.refused(port, lead, "game_act", r#"{"action": "quit"}"#, "players_only")?;
    // The game's own save and load are the player's menu: an agent has the save tools, with
    // their rules. And a save of a game whose one action is called `tap` loads again: taken
    // in a run, and read back at the title, it brings the run back.
    cx.call(port, lead, "game_act", r#"{"action": "title"}"#)?;
    let started = cx.call(port, lead, "game_act", r#"{"action": "start"}"#)?;
    let runs = at(&started, "runs").and_then(Json::as_u64);
    cx.refused(port, lead, "game_act", r#"{"action": "save:slot1"}"#, "use_save_tools")?;
    cx.refused(port, lead, "game_act", r#"{"action": "load:slot1"}"#, "use_save_tools")?;
    cx.call(port, lead, "save_write", r#"{"name": "check1"}"#)?;
    let left = cx.call(port, lead, "game_act", r#"{"action": "title"}"#)?;
    expect(text(&left, "phase") == Some("title"), "the run was not given up", &left)?;
    cx.call(port, lead, "save_read", r#"{"name": "check1", "force": true}"#)?;
    let state = cx.call(port, lead, "game_state", "{}")?;
    // (The phase is what says the run is back: giving a run up does not change how many there were.)
    expect(text(&state, "phase") == Some("running") && at(&state, "runs").and_then(Json::as_u64) == runs && at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "a save of a run, read at the title, did not bring the run back", &state)?;
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

    // Whatever the notes' files say, no note is hidden. A front matter padded past its bound
    // is not a note (so the check at the write reads what the notes' reader reads); and two
    // notes written on disk to replace each other are imported as they are, one line of the
    // two is not followed, and the list says so.
    let padded = Json::obj([("path", Json::Str("notes/padded.md".to_owned())), ("text", Json::Str(format!("---\ntitle: Padded\nkind: brief\n{}---\ntext\n", "\n".repeat(20))))]);
    let args = cx.out.join("padded-args.json");
    std::fs::write(&args, padded.to_string()).map_err(|e| format!("{}: {e}", args.display()))?;
    cx.refused(port, lead, "project_write", &format!("@{}", args.to_string_lossy()), "invalid")?;
    let before = cx.call(port, lead, "memory_list", "{}")?;
    for (id, other) in [("ring-a", "ring-b"), ("ring-b", "ring-a")] {
        let path = project.join("notes").join(format!("{id}.md"));
        std::fs::write(&path, format!("---\ntitle: Ring {id}\nkind: fact\nsupersedes: {other}\n---\nwritten on disk\n")).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let listed = cx.wait_for(port, lead, "memory_list", "notes written on disk are imported", Duration::from_secs(30), |l| at(l, "problems").is_some())?;
    let said = at(&listed, "problems").map(Json::to_string).unwrap_or_default();
    let current = |l: &Json| at(l, "current").and_then(Json::as_u64).unwrap_or(0);
    expect(said.contains("ring-b says it supersedes ring-a") && current(&listed) == current(&before) + 1, "two notes that replace each other on disk: one of them must stay listed, and it must be said", &listed)?;
    for id in ["ring-a", "ring-b"] {
        let path = project.join("notes").join(format!("{id}.md"));
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    cx.wait_for(port, lead, "memory_list", "notes removed on disk are gone", Duration::from_secs(30), |l| at(l, "problems").is_none() && current(l) == current(&before))?;
    // Files that are not notes take no note's place, however many (review R3: a thousand
    // drafts with ids that sort first left the project with no notes for anyone). More of
    // them than the notes that are read, written on disk; the notes are all still there,
    // and the next agent is still told the brief.
    let drafts = 1030;
    for i in 0..drafts {
        let path = project.join("notes").join(format!("0000-draft-{i:04}.md"));
        // (Each begins as a note does and is none: it has no title.)
        std::fs::write(&path, "---\nkind: brief\n---\ndraft\n").map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let flooded = cx.wait_for(port, lead, "memory_list", "the drafts written on disk are imported", Duration::from_secs(60), |l| at(l, "not_notes_total").and_then(Json::as_u64) == Some(drafts))?;
    expect(current(&flooded) == current(&before) && at(&flooded, "total").and_then(Json::as_u64) == at(&before, "total").and_then(Json::as_u64), "files that are not notes pushed notes out of the list", &flooded)?;
    // And they do not blind the check at the write: a second brief is still refused there
    // (review R4: among a thousand such files it was accepted, and the reader then said so).
    let second = Json::obj([("path", Json::Str("notes/pitch.md".to_owned())), ("text", Json::Str("---\ntitle: Another brief\nkind: brief\n---\ntext\n".to_owned()))]);
    let args = cx.out.join("pitch-args.json");
    std::fs::write(&args, second.to_string()).map_err(|e| format!("{}: {e}", args.display()))?;
    cx.refused(port, lead, "project_write", &format!("@{}", args.to_string_lossy()), "brief_exists")?;
    let hello = cx.call(port, None, "agent_hello", r#"{"name": "among-drafts"}"#)?;
    expect(text(&hello, "project_memory.brief.id") == Some("brief"), "among a thousand drafts the next agent is not told the brief", &hello)?;
    for i in 0..drafts {
        let path = project.join("notes").join(format!("0000-draft-{i:04}.md"));
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    cx.wait_for(port, lead, "memory_list", "the drafts removed on disk are gone", Duration::from_secs(60), |l| at(l, "not_notes").is_none())?;

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

    // 8. Again with `--run` and no lead at all: the module is loaded at start as the lead
    // last built it. What was chosen and noted is still there.
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

    // 11. One start with `--run-as-is` approves nothing: `--run` after it still holds the
    // builder's change back (review R2: the as-is start recorded what it loaded, and every
    // later `--run` ran it).
    let held_back = |cx: &mut Cx, port: u64, why: &str, says: &str| -> Result<(), String> {
        cx.start(&project, port, &["--run"])?;
        let state = cx.wait_for(port, None, "game_state", "--run decides about the module", Duration::from_secs(180), done)?;
        let said = at(&state, "problems").map(Json::to_string).unwrap_or_default();
        expect(at(&state, "modules_running").and_then(Json::as_array).is_some_and(<[Json]>::is_empty) && said.contains("was not loaded") && said.contains(says), why, &state)?;
        cx.stop(port)
    };
    held_back(cx, port2, "--run loaded what one --run-as-is start had loaded: an as-is start must approve nothing", "changed since")?;

    // 12. A granted lead builds the builder's change, which is the review: `--run` loads it.
    let lena = Some(("lena", "lead"));
    let builds = |cx: &mut Cx, port: u64| -> Result<Json, String> {
        cx.start(&project, port, &["--lead", "lena"])?;
        let built = cx.call(port, lena, "logic_build", r#"{"path": "modules/tapfive"}"#)?;
        cx.stop(port)?;
        expect(at(&built, "ok").and_then(Json::as_bool) == Some(true) && text(&built, "reload.status") == Some("running"), "the lead's build did not load", &built)?;
        Ok(built)
    };
    let built = builds(cx, port3)?;
    expect(at(&built, "at_start").is_none(), "a build of a module that keeps to its own folder says --run will not load it", &built)?;
    cx.start(&project, port4, &["--run"])?;
    let state = cx.wait_for(port4, None, "game_state", "--run loads what the lead built", Duration::from_secs(180), done)?;
    expect(at(&state, "problems").is_none() && at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "--run did not load the module a lead built", &state)?;
    cx.stop(port4)?;

    // 13. The project root's build configuration is held too: a `.cargo/` there names what a
    // build runs, and no module's folder covers it (review R2: a wrapper named there ran
    // under `--run` with every module untouched). Harmless here: a setting that changes nothing.
    let config = project.join(".cargo");
    std::fs::create_dir_all(&config).and_then(|()| std::fs::write(config.join("config.toml"), "[term]\nquiet = true\n")).map_err(|e| format!("{}: {e}", config.display()))?;
    held_back(cx, port2, "--run built under a build configuration nobody with authority built with", "build configuration")?;
    std::fs::remove_dir_all(&config).map_err(|e| format!("{}: {e}", config.display()))?;

    // 14. What `--run` loads is built in a folder of the operator's own, never taken from a
    // build folder a project could bring with it (review R3: Cargo takes a library it finds
    // as built from the sources when the file times say so). Replayed here: another operator
    // (another settings folder) builds changed sources into the same build cache; the sources
    // are put back as they were recorded, with their file time; `--run`, held to the first
    // operator's record, must load the library of the recorded sources and not the other.
    let source = project.join("modules").join("tapfive").join("src").join("lib.rs");
    let recorded = std::fs::read_to_string(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let written = std::fs::metadata(&source).and_then(|m| m.modified()).map_err(|e| format!("{}: {e}", source.display()))?;
    let other = recorded.replace("cx.set_state(\"score\", taps * 10 + left)?;", "cx.set_state(\"score\", 777u64)?;");
    expect(other != recorded, "the sample module does not set its score as the check expects", &Json::Null)?;
    std::fs::write(&source, &other).map_err(|e| format!("{}: {e}", source.display()))?;
    cx.as_operator("settings-other");
    let built = builds(cx, port3);
    cx.as_operator("settings");
    built?;
    std::fs::write(&source, &recorded).and_then(|()| std::fs::File::options().write(true).open(&source)?.set_modified(written)).map_err(|e| format!("{}: {e}", source.display()))?;
    cx.start(&project, port4, &["--run"])?;
    let state = cx.wait_for(port4, None, "game_state", "--run loads the recorded module", Duration::from_secs(180), done)?;
    expect(at(&state, "problems").is_none() && at(&state, "modules_running.0").and_then(Json::as_str) == Some("tapfive"), "--run did not load the module whose sources are the recorded ones", &state)?;
    cx.call(port4, None, "game_act", r#"{"action": "start"}"#)?;
    let scored = |s: &Json| at(s, "score").and_then(Json::as_f64).is_some_and(|score| score > 0.0);
    let state = cx.wait_for(port4, None, "game_state", "the run's first ticks", Duration::from_secs(60), scored)?;
    expect(at(&state, "score").and_then(Json::as_f64) != Some(777.0), "--run loaded a library built from other sources than the recorded ones (it was in the build cache)", &state)?;
    cx.stop(port4)?;

    // 15. A module that compiles a crate from outside its own folder is not for `--run`: the
    // record covers the module's folder, and a helper crate elsewhere could change unseen
    // (review R2, the blocker: a builder wrote a helper's source and `--run` loaded it). The
    // lead's own build loads it and says so, and `--run` loads nothing of it. (A crate beside
    // a workspace's member, used by it, is the workspace's without being named.)
    let helper = project.join("modules").join("helper");
    let module = project.join("modules").join("tapfive");
    let write = |path: PathBuf, text: String| std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()));
    std::fs::create_dir_all(helper.join("src")).map_err(|e| format!("{}: {e}", helper.display()))?;
    write(helper.join("Cargo.toml"), "[package]\nname = \"helper\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"rlib\"]\n".to_owned())?;
    write(helper.join("src").join("lib.rs"), "pub fn marker() -> i64 {\n    111\n}\n".to_owned())?;
    let manifest = std::fs::read_to_string(module.join("Cargo.toml")).map_err(|e| format!("the module's Cargo.toml: {e}"))?;
    expect(manifest.contains("[dependencies]"), "the module's manifest has no [dependencies] to add a helper to", &Json::Str(manifest.clone()))?;
    write(module.join("Cargo.toml"), manifest.replace("[dependencies]", "[dependencies]\nhelper = { path = \"../helper\" }"))?;
    let lib = std::fs::read_to_string(module.join("src").join("lib.rs")).map_err(|e| format!("the module's lib.rs: {e}"))?;
    write(module.join("src").join("lib.rs"), lib + "\n/// (The check's: the module uses a crate from outside its folder.)\npub fn helper_marker() -> i64 {\n    helper::marker()\n}\n")?;
    let built = builds(cx, port3)?;
    expect(text(&built, "at_start").is_some_and(|s| s.contains("outside its own folder")), "the lead's build of a module with a crate outside its folder does not say that --run will not load it", &built)?;
    held_back(cx, port2, "--run loaded a module that compiles a crate from outside its own folder", "was not loaded")?;
    // 16. `specula run`: one command builds a project's modules, plays its game with no window
    // by a script of inputs and prints how it stands (docs/design/FIRST_HOUR.md, "A headless
    // run"): what the measured comparison's builders each wrote as a script of their own. On a
    // project of its own, as `specula new` leaves one, with no engine started for it.
    let (code, made) = cx.cli(&["new", "runfive", "--in", &projects.to_string_lossy(), "--settings", &settings])?;
    expect(code == 0 && text(&made, "next").is_some_and(|n| n.contains("specula run")), "`specula new` does not say how to run the project it made", &made)?;
    let runfive = projects.join("runfive").to_string_lossy().into_owned();
    let run_out = cx.out.join("run");
    let own = |name: &str| run_out.join(name).to_string_lossy().into_owned();
    const TAPS: &str = "0:enter,5:tap,6:none,10:tap,11:none,15:tap,16:none,20:tap,21:none,25:tap";
    let frame = own("won.png");
    let (code, won) = cx.cli(&["run", &runfive, "--ticks", "40", "--seed", "1", "--inputs", TAPS, "--frame", &frame, "--size", "800x480"])?;
    let is = |j: &Json, key: &str, n: f64| at(j, key).and_then(Json::as_f64) == Some(n);
    expect(code == 0 && is(&won, "tick", 40.0) && text(&won, "phase") == Some("won") && is(&won, "taps", 5.0) && is(&won, "score", 60.0), "a run of five taps did not print a won game as the game file's report shapes it", &won)?;
    expect(at(&won, "texts").and_then(Json::as_array).is_some_and(|t| t.iter().any(|x| x.as_str() == Some("You won"))), "the report's texts are not the won screen's", &won)?;
    // The frame is the picture asked for: a PNG of that size.
    let png = std::fs::read(&frame).map_err(|e| format!("the run's frame {frame}: {e}"))?;
    let side = |at: usize| png.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    expect(png.starts_with(b"\x89PNG\r\n\x1a\n") && (side(16), side(20)) == (Some(800), Some(480)), "the run's frame is not a PNG of 800 by 480", &Json::from(png.len() as u64))?;
    // The same seed and inputs: the same answer, to the letter, and the same frame, byte for byte.
    let frame2 = own("won-again.png");
    let (_, again) = cx.cli(&["run", &runfive, "--ticks", "40", "--seed", "1", "--inputs", TAPS, "--frame", &frame2, "--size", "800x480"])?;
    expect(again.to_string() == won.to_string(), "the same seed and inputs printed another state", &again)?;
    let png2 = std::fs::read(&frame2).map_err(|e| format!("the second run's frame {frame2}: {e}"))?;
    expect(png2 == png, "the same seed and inputs drew another frame", &Json::from(png2.len() as u64))?;
    // The script counts every tick; the game's clock ticks only while a run is under way. At
    // the title nothing is kept yet, and a pause holds the run where it was.
    let (_, title) = cx.cli(&["run", &runfive, "--ticks", "30"])?;
    expect(is(&title, "tick", 30.0) && text(&title, "phase") == Some("title") && at(&title, "taps").is_some_and(Json::is_null), "thirty ticks at the title did not leave the game at its title, untouched", &title)?;
    let (_, paused) = cx.cli(&["run", &runfive, "--ticks", "7", "--inputs", "0:enter,5:tap,6:escape"])?;
    let (_, later) = cx.cli(&["run", &runfive, "--ticks", "250", "--inputs", "0:enter,5:tap,6:escape"])?;
    expect(text(&paused, "phase") == Some("paused") && text(&later, "phase") == Some("paused") && at(&paused, "left") == at(&later, "left") && is(&later, "tick", 250.0), "a paused run did not stand still while the script's ticks went on", &later)?;
    // Enter on the pause menu's first button resumes it, and its clock runs again.
    let (_, resumed) = cx.cli(&["run", &runfive, "--ticks", "250", "--inputs", "0:enter,5:tap,6:escape,200:enter"])?;
    expect(text(&resumed, "phase") == Some("running") && at(&resumed, "left").and_then(Json::as_f64) < at(&later, "left").and_then(Json::as_f64), "a resumed run did not go on", &resumed)?;
    // Left and Right step a setting on its button, and `--settings` keeps what the player
    // chose: a later run with the same folder comes up with it, and one without does not.
    let kept = own("settings");
    let (_, set) = cx.cli(&["run", &runfive, "--ticks", "4", "--settings", &kept, "--inputs", "0:down,1:enter,2:left,3:left"])?;
    expect(is(&set, "volume", 6.0) && text(&set, "screen") == Some("settings"), "Left on the volume's button did not step it down (8 to 6) on the settings screen", &set)?;
    let (_, came_back) = cx.cli(&["run", &runfive, "--ticks", "0", "--settings", &kept])?;
    let (_, fresh) = cx.cli(&["run", &runfive, "--ticks", "0"])?;
    expect(is(&came_back, "volume", 6.0) && is(&fresh, "volume", 8.0), "a later run did not come up with the kept settings (or one without the folder did)", &came_back)?;
    // What a run cannot do as asked is refused before a tick is played: an input after its last
    // tick (it would never be given), and a value that is not of its action's kind.
    let (code, late) = cx.cli(&["run", &runfive, "--ticks", "10", "--inputs", "0:enter,50:tap"])?;
    expect(code == 1 && text(&late, "message").is_some_and(|m| m.contains("after the run's last tick")), "an input after the last tick was not refused", &late)?;
    let (code, wrong) = cx.cli(&["run", &runfive, "--ticks", "10", "--inputs", "0:enter,5:tap=0.5"])?;
    expect(code == 1 && text(&wrong, "error") == Some("bad_input") && text(&wrong, "input") == Some("tap=0.5"), "a button given a number was not refused with the input and why", &wrong)?;
    // Two words at once are held together: a tap with the interface's down is still a tap.
    let (_, both) = cx.cli(&["run", &runfive, "--ticks", "8", "--inputs", "0:enter,5:tap+down"])?;
    expect(is(&both, "taps", 1.0), "two words joined by + were not both done", &both)?;
    // A word the game does not take is refused before a tick is played, with the words it takes.
    let (code, unknown) = cx.cli(&["run", &runfive, "--ticks", "5", "--inputs", "0:jump"])?;
    expect(code == 1 && text(&unknown, "error") == Some("unknown_input") && at(&unknown, "inputs").and_then(Json::as_array).is_some_and(|w| w.iter().any(|x| x.as_str() == Some("tap"))), "an unknown input was not refused with the game's own inputs", &unknown)?;
    // A module that does not build is the compiler's words, as data, and exit code 1.
    let lib = projects.join("runfive").join("modules").join("runfive").join("src").join("lib.rs");
    let good = std::fs::read_to_string(&lib).map_err(|e| format!("{}: {e}", lib.display()))?;
    std::fs::write(&lib, good.replacen("let hz = u64::from(cx.tick_hz());", "let hz = u64::from(cx.tick_hz())", 1)).map_err(|e| format!("{}: {e}", lib.display()))?;
    let failed = cx.cli(&["run", &runfive, "--ticks", "1"]);
    std::fs::write(&lib, &good).map_err(|e| format!("{}: {e}", lib.display()))?;
    let (code, failed) = failed?;
    expect(code == 1 && text(&failed, "error") == Some("build_failed") && text(&failed, "diagnostics.0.file").is_some_and(|f| f.ends_with("lib.rs")) && at(&failed, "diagnostics.0.line").and_then(Json::as_u64).is_some(), "a module that does not build was not answered with where and why", &failed)?;
    // A module whose call fails is an error and never a state (a failed call is undone, so a
    // state printed all the same reads as rules that do nothing): which error, at which tick,
    // how often, as one line. Here the second tap spawns a second child of one name.
    let tap = "if cx.action(\"tap\").pressed {";
    expect(good.contains(tap), "the sample module no longer reads its tap as this check expects", &Json::Null)?;
    std::fs::write(&lib, good.replacen(tap, &format!("{tap}\n                cx.spawn(Spawn::new(\"cube\").name(\"twice\").parent(ROOT))?;"), 1)).map_err(|e| format!("{}: {e}", lib.display()))?;
    // (The run starts at the script's tick 4, so the game's own ticks are four behind the
    // script's: what is answered is the script's tick, the one an input is written at.)
    const LATE_TAPS: &str = "4:enter,9:tap,10:none,14:tap,15:none,19:tap,20:none,24:tap,25:none,29:tap";
    let failing = cx.cli(&["run", &runfive, "--ticks", "44", "--inputs", LATE_TAPS]);
    // What the rules log is said beside the state, a line each with the script's tick; and
    // what a module prints for itself is in nobody's way, on either of its outputs, with a
    // line end or without (the states of a followed run come by the engine's standard error).
    std::fs::write(&lib, good.replacen(tap, &format!("{tap}\n                cx.log(\"tapped\")?;\n                println!(\"a module's own print\");\n                eprint!(\"and words of its own with no line end\");"), 1)).map_err(|e| format!("{}: {e}", lib.display()))?;
    let exe = cx.exe.clone();
    let raw = |args: &[&str], steps: &[u8]| -> Result<std::process::Output, String> {
        use std::io::Write;
        let mut child = std::process::Command::new(&exe).args(args).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| format!("{}: {e}", exe.display()))?;
        // (All of its input at once, and its end: what it says is read when it has ended.)
        if let Some(mut to) = child.stdin.take() {
            to.write_all(steps).map_err(|e| format!("the run's input: {e}"))?;
        }
        child.wait_with_output().map_err(|e| format!("{}: {e}", exe.display()))
    };
    let logged = raw(&["run", &runfive, "--ticks", "12", "--inputs", "4:enter,9:tap"], b"");
    // A step that is refused changes nothing: a tap held through one is still the same tap
    // (so the tap after it is no second press), each line written is answered by one line and
    // no other (a line that is not text among them), and the log comes when the run ends.
    let held = raw(&["run", &runfive, "--follow"], b"1 enter\n1 tap\n1 tap+nope\n2\n1 tap\n\xE9\n1\n");
    std::fs::write(&lib, &good).map_err(|e| format!("{}: {e}", lib.display()))?;
    let (code, failing) = failing?;
    expect(code == 1 && text(&failing, "error") == Some("module_failed") && text(&failing, "first.code") == Some("name_taken") && at(&failing, "first.run_tick").and_then(Json::as_u64) == Some(14) && at(&failing, "first.tick").and_then(Json::as_u64) == Some(10), "a module whose tick failed was not answered with the error and the script's tick it failed at", &failing)?;
    expect(at(&failing, "failures").and_then(Json::as_u64) == Some(4) && at(&failing, "last_tick").and_then(Json::as_u64) == Some(29) && text(&failing, "message").is_some_and(|m| m.contains("name_taken") && m.contains("at tick 14")), "a failing module's answer does not count its failures, or its message does not say which and when", &failing)?;
    let logged = logged?;
    let (said, beside) = (String::from_utf8_lossy(&logged.stdout).into_owned(), String::from_utf8_lossy(&logged.stderr).into_owned());
    expect(logged.status.success() && beside.lines().any(|l| l == "specula run: runfive logged at tick 9: tapped") && said.lines().count() == 1 && said.starts_with("{\"tick\":12,"), "what a module logged was not said beside the run's one line of state, at the script's tick", &Json::from(format!("{said}\n{beside}").as_str()))?;
    let held = held?;
    let (said, beside) = (String::from_utf8_lossy(&held.stdout).into_owned(), String::from_utf8_lossy(&held.stderr).into_owned());
    let lines: Vec<Json> = said.lines().filter_map(|l| Json::parse(l).ok()).collect();
    expect(held.status.success() && said.lines().count() == 8 && lines.len() == 8, "a followed run did not answer each line with one line of JSON and nothing else (a module's own print is not the run's)", &Json::from(said.as_str()))?;
    expect(text(&lines[3], "error") == Some("unknown_input") && text(&lines[6], "error") == Some("bad_step") && is(&lines[2], "taps", 1.0) && is(&lines[4], "taps", 1.0) && is(&lines[5], "taps", 2.0) && is(&lines[7], "taps", 2.0) && is(&lines[7], "tick", 6.0), "a step that was refused changed the run (a held tap was let go or pressed again, or ticks passed), or the same word given again was not a press again", &Json::from(said.as_str()))?;
    expect(beside.lines().any(|l| l == "specula run: runfive logged at tick 1: tapped"), "a followed run did not say what its module logged when it ended", &Json::from(beside.as_str()))?;
    // A refusal is one line too, where the state would have been.
    let refused = raw(&["run", &runfive, "--ticks", "10", "--inputs", "0:enter,50:tap"], b"")?;
    let line = String::from_utf8_lossy(&refused.stdout).into_owned();
    expect(line.lines().count() == 1 && line.starts_with("{\"error\":\"usage\""), "a run's refusal is not one line", &Json::from(line.as_str()))?;
    // A followed run: the state, then a step a line read from its input, the state after each;
    // a line that is not a step is answered and the run goes on; the same steps as a script
    // give the same state. Whoever plays reads a state before it writes its next step.
    {
        use std::io::{BufRead, Write};
        let mut child = std::process::Command::new(&exe)
            .args(["run", &runfive, "--follow", "--seed", "1"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("{}: {e}", exe.display()))?;
        let mut to = child.stdin.take().ok_or("the followed run has no input")?;
        let mut from = std::io::BufReader::new(child.stdout.take().ok_or("the followed run has no output")?);
        let mut read = |what: &str| -> Result<Json, String> {
            let mut line = String::new();
            from.read_line(&mut line).map_err(|e| format!("the followed run's output: {e}"))?;
            Json::parse(line.trim()).map_err(|e| format!("after {what} the followed run did not answer a line of JSON ({e}): {line}"))
        };
        let opening = read("its start")?;
        expect(is(&opening, "tick", 0.0) && text(&opening, "phase") == Some("title"), "a followed run did not print its state at the start", &opening)?;
        let mut last = opening;
        for step in ["1 enter", "4", "1 tap", "4 none", "1 tap", "4 none", "1 tap", "4 none", "1 tap", "4 none", "1 tap", "14 none"] {
            writeln!(to, "{step}").and_then(|()| to.flush()).map_err(|e| format!("the followed run's input: {e}"))?;
            last = read(step)?;
        }
        expect(is(&last, "tick", 40.0) && text(&last, "phase") == Some("won") && is(&last, "taps", 5.0), "five taps given step by step did not win the game", &last)?;
        writeln!(to, "ten taps").and_then(|()| to.flush()).map_err(|e| format!("the followed run's input: {e}"))?;
        let not_a_step = read("a line that is no step")?;
        expect(text(&not_a_step, "error") == Some("bad_step"), "a line that is not a step was not answered as one", &not_a_step)?;
        writeln!(to, "2 jump").and_then(|()| to.flush()).map_err(|e| format!("the followed run's input: {e}"))?;
        let no_word = read("a word the game does not take")?;
        expect(text(&no_word, "error") == Some("unknown_input"), "a step with a word the game does not take was not answered as one", &no_word)?;
        writeln!(to, "3").and_then(|()| to.flush()).map_err(|e| format!("the followed run's input: {e}"))?;
        let after = read("three more ticks")?;
        expect(is(&after, "tick", 43.0) && text(&after, "phase") == Some("won"), "a followed run did not go on after two lines it refused", &after)?;
        drop(to);
        let ended = child.wait().map_err(|e| format!("the followed run: {e}"))?;
        let mut rest = String::new();
        let _ = std::io::Read::read_to_string(&mut from, &mut rest);
        expect(ended.success() && rest.trim().is_empty(), "a followed run did not end with its input, or said more after its last state", &Json::from(rest.as_str()))?;
    }
    // The game's own checks: every run the project's `checks/runs.json` lists, played by one
    // command, each held against what it expects. The sample's own hold as `specula new` wrote
    // them (a setting changed in one check is found in the next, and not by a new player).
    let checked = raw(&["check", &runfive], b"")?;
    let said = String::from_utf8_lossy(&checked.stdout).into_owned();
    let lines: Vec<Json> = said.lines().filter_map(|l| Json::parse(l).ok()).collect();
    let ok = |l: &Json| at(l, "ok").and_then(Json::as_bool);
    expect(checked.status.success() && said.lines().count() == 8 && lines.len() == 8 && lines[..7].iter().all(|l| ok(l) == Some(true)) && is(&lines[7], "checks", 7.0) && is(&lines[7], "passed", 7.0), "the sample's own checks did not all hold, a line each and a count", &Json::from(said.as_str()))?;
    // A check that does not hold says what differs, with the state; a run that cannot be made
    // is that check's error and the others are played; a check that expects nothing prints its
    // state; a frame is written below the project; and the command's exit code is 1.
    std::fs::create_dir_all(&run_out).map_err(|e| format!("{}: {e}", run_out.display()))?;
    let own_checks = own("own-checks.json");
    write(
        PathBuf::from(&own_checks),
        format!(
            r#"{{"format": "specula.checks.v1", "checks": [
  {{"name": "holds", "ticks": 40, "inputs": "{TAPS}", "frame": "checks/out/won.png", "size": [320, 200], "expect": {{"phase": "won", "taps": {{"gte": 5, "lt": 6}}, "texts": {{"has": "You won"}}}}}},
  {{"name": "does not", "ticks": 40, "inputs": ["0:enter,5:tap,6:none", "10:tap,11:none,15:tap,16:none,20:tap,21:none,25:tap"], "expect": {{"taps": 4, "phase": "won"}}}},
  {{"name": "to read", "ticks": 0}},
  {{"name": "no such word", "ticks": 2, "inputs": "0:jump"}}
]}}"#
        ),
    )?;
    let off = raw(&["check", &runfive, "--file", &own_checks], b"")?;
    let said = String::from_utf8_lossy(&off.stdout).into_owned();
    let lines: Vec<Json> = said.lines().filter_map(|l| Json::parse(l).ok()).collect();
    expect(off.status.code() == Some(1) && lines.len() == 5 && ok(&lines[0]) == Some(true) && at(&lines[0], "state").is_none(), "a check that holds is not one line that says so and no more", &Json::from(said.as_str()))?;
    expect(ok(&lines[1]) == Some(false) && is(&lines[1], "differs.taps.is", 5.0) && is(&lines[1], "differs.taps.expected", 4.0) && at(&lines[1], "differs.phase").is_none() && text(&lines[1], "state.phase") == Some("won"), "a check that does not hold does not say what differs, with the state", &lines[1])?;
    expect(ok(&lines[2]) == Some(true) && text(&lines[2], "state.phase") == Some("title"), "a check that expects nothing does not print its state", &lines[2])?;
    expect(ok(&lines[3]) == Some(false) && text(&lines[3], "error.error") == Some("unknown_input"), "a run that cannot be made is not its check's error", &lines[3])?;
    expect(is(&lines[4], "checks", 4.0) && is(&lines[4], "passed", 2.0) && text(&lines[4], "failed.0") == Some("does not") && text(&lines[4], "failed.1") == Some("no such word"), "the count of the checks is not what was played", &lines[4])?;
    let shot = std::fs::read(projects.join("runfive").join("checks").join("out").join("won.png")).map_err(|e| format!("the check's frame: {e}"))?;
    expect(shot.starts_with(b"\x89PNG\r\n\x1a\n") && shot.get(16..24) == Some(&[0, 0, 1, 64, 0, 0, 0, 200][..]), "a check's frame is not a PNG of 320 by 200 below the project", &Json::from(shot.len() as u64))?;
    let only = raw(&["check", &runfive, "--file", &own_checks, "--only", "holds"], b"")?;
    expect(only.status.success() && String::from_utf8_lossy(&only.stdout).lines().count() == 2, "--only did not play the one check named", &Json::from(String::from_utf8_lossy(&only.stdout).as_ref()))?;
    // One engine has a project at a time: a run on a project an engine has open is refused
    // with that engine, at once.
    let port5 = port + 5;
    cx.start(&projects.join("runfive"), port5, &[])?;
    let (code, open) = cx.cli(&["run", &runfive, "--ticks", "1", "--settings", &settings])?;
    expect(code == 1 && text(&open, "error") == Some("project_open") && at(&open, "engine.port").and_then(Json::as_u64) == Some(port5), "a run on a project that an engine has open was not refused with that engine", &open)?;
    // And nobody else's word moves a run's clock: asked of an engine that is not a run's, it
    // is done (so the refusal under a script is the script's, held by a test of its own).
    cx.stop(port5)?;
    // Each run's engine ended with it: none is listed, and none was left listening.
    let (_, listed) = cx.cli(&["list", "--settings", &kept])?;
    expect(is(&listed, "count", 0.0), "a headless run left an engine behind", &listed)?;
    Ok("new project: built, played, saved, changed, noted, loaded again by --run; --run held to what a lead built: sources, the root's build configuration, and nothing from outside a module's folder; and `specula run` plays a project headless by a script: its state, a frame, kept settings, the same run twice".to_owned())
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
