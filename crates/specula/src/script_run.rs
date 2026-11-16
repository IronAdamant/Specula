//! `--script FILE`: a headless run of a project's game, from its start, by a script of inputs
//! (`specula run`: docs/design/FIRST_HOUR.md, "A headless run").
//!
//! One command builds the game file's modules, plays a number of ticks with inputs at given
//! ticks, and says how the game stands: the state as the game file's `report` shapes it, and a
//! frame. It is what a game's own check is made of, and what whoever judges the game runs. The
//! measured comparison's builders each wrote this themselves, as a script of their own that
//! called the engine tool by tool (a third to two fifths of what a build cost, and thousands of
//! calls a run: docs/design/LEVERAGE.md, "Results (2026-11-16)").
//!
//! **The run is the game's, as a player would have it.** An input is a word: a press on the
//! interface (`up`, `down`, `left`, `right`, `accept` or `enter`, `back` or `escape`, or a
//! source such as `Key.Space`), an action of a module (held until the next input, as a held
//! key is), or `none` (nothing held). A word that is both does both, as one key does. The
//! game's clock ticks only while the game has it running: the title screen and a pause hold
//! it, and a run that ended has no further tick. The script's own count of ticks goes on
//! whatever screen is up.
//!
//! **Nothing of this machine's time is in it.** The engine is started with the clock standing
//! still (`Engine::script_clock`), and every tick is one this script asked for
//! ([`TimeOp::Play`]): the same seed and inputs give the same run.
//!
//! The work is a caller's, on a thread of its own, as the operator who started the engine
//! ([`crate::run_modules::RUN_SESSION`]): it uses the tools an agent would, through the same
//! checks, and writes its answer to the file the command reads.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use specula_core::Json;
use specula_ludus::game::{GameFile, View};
use specula_protocol::{Content, SessionInfo, ToolHandler, ToolOutput};

use crate::startgate::StartGate;
use crate::tools::{Command, TimeOp, Tools};

/// Most ticks one run plays (at 60 Hz, about four and a half hours of game).
pub const MAX_TICKS: u64 = 1_000_000;
/// Most inputs in one run's script.
pub const MAX_INPUTS: usize = 10_000;
/// A frame's sides, in pixels.
pub const FRAME_SIDE: std::ops::RangeInclusive<u32> = 64..=2048;
/// How long the run waits for the engine's loop to begin.
const START_WAIT: Duration = Duration::from_secs(120);
/// Most diagnostics of a build that failed that the answer carries.
const MAX_DIAGNOSTICS: usize = 8;
/// Most widgets read for the interface's texts.
const MAX_WIDGETS: usize = 2000;
/// A held action is held for this many ticks at a time, and held again before they run out.
const HOLD_TICKS: u64 = 18_000;

/// What a run is asked: `specula run` writes it, the engine reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub ticks: u64,
    pub seed: Option<u64>,
    /// At a tick, a word. In the order given; ticks never go back.
    pub inputs: Vec<(u64, String)>,
    pub frame: Option<PathBuf>,
    pub size: (u32, u32),
    /// Where the answer goes.
    pub out: PathBuf,
}

impl Spec {
    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([
            ("ticks", Json::from(self.ticks)),
            ("inputs", Json::arr(self.inputs.iter().map(|(tick, word)| Json::arr([Json::from(*tick), Json::from(word.as_str())])))),
            ("size", Json::arr([Json::from(u64::from(self.size.0)), Json::from(u64::from(self.size.1))])),
            ("out", Json::from(self.out.to_string_lossy().as_ref())),
        ]);
        if let Some(seed) = self.seed {
            j.set("seed", Json::from(seed));
        }
        if let Some(frame) = &self.frame {
            j.set("frame", Json::from(frame.to_string_lossy().as_ref()));
        }
        j
    }

    pub fn from_json(j: &Json) -> Result<Spec, String> {
        let ticks = j.get("ticks").and_then(Json::as_u64).filter(|t| *t <= MAX_TICKS).ok_or_else(|| format!("ticks: a whole number up to {MAX_TICKS}"))?;
        let mut inputs = Vec::new();
        for item in j.get("inputs").and_then(Json::as_array).unwrap_or(&[]) {
            let pair = item.as_array().filter(|p| p.len() == 2);
            let (Some(tick), Some(word)) = (pair.and_then(|p| p[0].as_u64()), pair.and_then(|p| p[1].as_str())) else {
                return Err("inputs: a list of [tick, word]".to_owned());
            };
            inputs.push((tick, word.to_owned()));
        }
        check_inputs(&inputs)?;
        let side = |i: usize| j.get("size").and_then(Json::as_array).and_then(|s| s.get(i)).and_then(Json::as_u64).and_then(|n| u32::try_from(n).ok()).filter(|n| FRAME_SIDE.contains(n));
        let size = match j.get("size") {
            None => (960, 540),
            Some(_) => (side(0).ok_or("size: [width, height], each 64 to 2048")?, side(1).ok_or("size: [width, height], each 64 to 2048")?),
        };
        Ok(Spec {
            ticks,
            seed: match j.get("seed") {
                None => None,
                Some(seed) => Some(seed.as_u64().filter(|s| *s <= MAX_SEED).ok_or_else(|| format!("seed: a whole number up to {MAX_SEED}"))?),
            },
            inputs,
            frame: j.get("frame").and_then(Json::as_str).map(PathBuf::from),
            size,
            out: j.get("out").and_then(Json::as_str).map(PathBuf::from).ok_or("out: where the answer is written")?,
        })
    }
}

/// A script's inputs are in order of their ticks, and there are at most [`MAX_INPUTS`].
pub fn check_inputs(inputs: &[(u64, String)]) -> Result<(), String> {
    if inputs.len() > MAX_INPUTS {
        return Err(format!("a script has at most {MAX_INPUTS} inputs"));
    }
    if inputs.windows(2).any(|w| w[1].0 < w[0].0) {
        return Err("a script's inputs are in the order of their ticks".to_owned());
    }
    if let Some((_, word)) = inputs.iter().find(|(_, w)| w.is_empty() || w.len() > 64 || w.chars().any(|c| c.is_whitespace() || c.is_control())) {
        return Err(format!("'{word}' is not an input: a word of 1 to 64 characters with no blank"));
    }
    Ok(())
}

/// A script as a command line gives it: `T:word,T:word` (`0:enter,5:up,90:none`).
pub fn parse_inputs(text: &str) -> Result<Vec<(u64, String)>, String> {
    let mut out = Vec::new();
    for item in text.split([',', '\n']).map(str::trim).filter(|i| !i.is_empty()) {
        let (tick, word) = item.split_once(':').ok_or_else(|| format!("'{item}' is not TICK:WORD (0:enter,5:up,90:none)"))?;
        let tick = tick.trim().parse::<u64>().map_err(|_| format!("'{item}': the tick is a whole number"))?;
        out.push((tick, word.trim().to_owned()));
    }
    check_inputs(&out)?;
    Ok(out)
}

/// `time_control` is refused while a script has the clock, except `info` (and the script's own
/// ticks): as while a playtest runs.
pub fn clock_refused(op: TimeOp, scripted: bool) -> bool {
    scripted && !matches!(op, TimeOp::Info | TimeOp::Play(_))
}

/// The seeds a run takes: whole numbers a JSON number holds exactly.
pub const MAX_SEED: u64 = (1 << 53) - 1;

/// No input of a script is later than its last tick: one that is would never be given.
pub fn check_span(inputs: &[(u64, String)], ticks: u64) -> Result<(), String> {
    match inputs.iter().find(|(at, _)| *at > ticks) {
        Some((at, word)) => Err(format!("the input '{word}' is at tick {at}, after the run's last tick ({ticks}): it would never be given")),
        None => Ok(()),
    }
}

/// The parts of one input: words joined by `+` (`up+shoot`: both, until the next input). A
/// part is a word, or an action with its value: `throttle=0.5` (an axis), `move=1/0` (an
/// axis of two: x/y). The value a part holds its action at: `true` for a word alone.
fn parts(word: &str) -> Result<Vec<(&str, Json)>, String> {
    let mut out = Vec::new();
    for part in word.split('+') {
        let (name, value) = match part.split_once('=') {
            None => (part, Json::Bool(true)),
            Some((name, value)) => {
                let numbers: Option<Vec<f64>> = value.split('/').map(|n| n.parse::<f64>().ok().filter(|n| n.is_finite())).collect();
                match numbers.as_deref() {
                    Some([x]) => (name, Json::Num(*x)),
                    Some([x, y]) => (name, Json::arr([Json::Num(*x), Json::Num(*y)])),
                    _ => return Err(format!("'{part}': an action's value is a number (throttle=0.5) or two with a slash (move=1/0)")),
                }
            }
        };
        if name.is_empty() {
            return Err(format!("'{word}' is not an input: a word, or words joined by + (up+shoot)"));
        }
        out.push((name, value));
    }
    if out.len() > 1 && out.iter().any(|(name, _)| *name == "none") {
        return Err(format!("'{word}': none is an input of its own (nothing held)"));
    }
    Ok(out)
}

/// The press on the interface a word is, if it is one (`ui_act`'s names).
fn press_of(word: &str) -> Option<&str> {
    match word {
        "up" | "down" | "left" | "right" | "accept" | "back" => Some(word),
        "enter" => Some("accept"),
        "escape" => Some("back"),
        w if w.contains('.') => Some(w),
        _ => None,
    }
}

fn session() -> SessionInfo {
    SessionInfo { id: crate::run_modules::RUN_SESSION.into(), client_name: "specula run".into(), client_version: env!("CARGO_PKG_VERSION").into(), protocol_version: String::new() }
}

fn refusal(code: &str, message: impl Into<String>) -> Json {
    Json::obj([("error", Json::from(code)), ("message", Json::from(message.into().as_str()))])
}

/// A tool's answer as JSON: `Err` when the tool answered an error.
fn read(out: &ToolOutput) -> Result<Json, Json> {
    let json = out
        .content
        .iter()
        .find_map(|c| match c {
            Content::Text(t) => Json::parse(t).ok(),
            Content::Image { .. } => None,
        })
        .unwrap_or(Json::Null);
    if out.is_error {
        Err(if json.get("error").is_some() { json } else { refusal("refused", json.to_string()) })
    } else {
        Ok(json)
    }
}

struct Run<'t> {
    tools: &'t Tools,
    session: SessionInfo,
    /// The loaded modules, by their own names, each with its actions.
    modules: Vec<(String, Vec<String>)>,
    /// What is held now: (module, action, the value it is held at).
    held: Vec<(String, String, Json)>,
    /// Ticks the holds still have.
    hold_left: u64,
}

impl Run<'_> {
    fn call(&self, tool: &str, args: Json) -> Result<Json, Json> {
        read(&self.tools.call(&self.session, tool, &args)).map_err(|mut e| {
            e.set("tool", Json::from(tool));
            e
        })
    }

    /// Builds and loads one module folder; its own name.
    fn build(&self, dir: &str, seed: Option<u64>) -> Result<String, Json> {
        let mut args = Json::obj([("path", Json::from(dir))]);
        if let Some(seed) = seed {
            args.set("seed", Json::from(seed));
        }
        let out = self.tools.call(&self.session, "logic_build", &args);
        let answer = match read(&out) {
            Ok(a) => a,
            Err(a) => a,
        };
        let name = answer.get("reload").and_then(|r| r.get("module")).and_then(Json::as_str);
        if let (false, Some(name)) = (out.is_error, name) {
            return Ok(name.to_owned());
        }
        // A build that failed: how many errors, and the first of them, each where it is.
        let mut e = match answer.get("diagnostics").and_then(Json::as_array) {
            Some(all) if !all.is_empty() => {
                let errors = all.iter().filter(|d| d.get("level").and_then(Json::as_str) != Some("warning"));
                let brief = |d: &Json| {
                    let mut one = Json::obj::<&str>([]);
                    for key in ["file", "line", "column", "code", "message"] {
                        if let Some(v) = d.get(key).filter(|v| !v.is_null()) {
                            one.set(key, v.clone());
                        }
                    }
                    one
                };
                let mut e = refusal("build_failed", format!("{dir} did not build"));
                e.set("errors", answer.get("errors").cloned().unwrap_or(Json::Null));
                e.set("diagnostics", Json::arr(errors.take(MAX_DIAGNOSTICS).map(brief)));
                e
            }
            _ => match answer.get("reload_error") {
                Some(why) => {
                    let mut e = refusal("load_failed", format!("{dir} built and did not load"));
                    e.set("why", why.clone());
                    e
                }
                None if answer.get("error").is_some() => answer.clone(),
                None => refusal("build_failed", format!("{dir} did not build, and the build said nothing this run can read")),
            },
        };
        e.set("module", Json::from(dir));
        Err(e)
    }

    fn release(&mut self) -> Result<(), Json> {
        // (Let go with its own value and no ticks: a value must be of its action's kind.)
        for (module, action, value) in std::mem::take(&mut self.held) {
            self.call("logic_input", Json::obj([("module", Json::from(module.as_str())), ("actions", Json::obj([(action.as_str(), value)])), ("ticks", Json::from(0u64))]))?;
        }
        Ok(())
    }

    fn hold(&mut self) -> Result<(), Json> {
        for (module, action, value) in &self.held {
            self.call("logic_input", Json::obj([("module", Json::from(module.as_str())), ("actions", Json::obj([(action.as_str(), value.clone())])), ("ticks", Json::from(HOLD_TICKS))]))?;
        }
        self.hold_left = HOLD_TICKS;
        Ok(())
    }

    /// One input: what was held is let go, then each of its parts does what it is.
    fn input(&mut self, word: &str) -> Result<(), Json> {
        self.release()?;
        for (name, value) in parts(word).map_err(|why| refusal("bad_input", why))? {
            if name == "none" {
                continue;
            }
            if let (Some(press), Json::Bool(true)) = (press_of(name), &value) {
                // (A step that did not fit a call's time is taken up by the next call.)
                let mut steps = Json::arr([Json::obj([("press", Json::from(press))])]);
                for _ in 0..64 {
                    let did = self.call("ui_act", Json::obj([("surface", Json::from("game")), ("device", Json::from("keyboard")), ("steps", steps)]))?;
                    match did.get("rest").filter(|r| r.as_array().is_some_and(|r| !r.is_empty())) {
                        Some(rest) => steps = rest.clone(),
                        None => break,
                    }
                }
            }
            if let Some((module, _)) = self.modules.iter().find(|(_, actions)| actions.iter().any(|a| a == name)) {
                self.held.push((module.clone(), name.to_owned(), value));
            }
        }
        if !self.held.is_empty() {
            self.hold()?;
        }
        Ok(())
    }

    /// Plays up to `ticks` ticks of the game. Fewer were played when the game holds its clock
    /// (the title, a pause, a run that ended): the rest pass with nothing ticking.
    fn play(&mut self, ticks: u64) -> Result<(), Json> {
        let mut left = ticks;
        while left > 0 {
            let ask = left.min(crate::game::STEPS_AT_MOST);
            if !self.held.is_empty() && self.hold_left <= ask {
                self.hold()?;
            }
            let said = read(&self.tools.send(&self.session, Command::Time(TimeOp::Play(ask))))?;
            let played = said.get("played").and_then(Json::as_u64).unwrap_or(0);
            self.hold_left = self.hold_left.saturating_sub(played);
            left -= played.min(left);
            if played < ask {
                // Fewer than asked: the game holds its clock, and the rest of the span passes
                // with nothing ticking. Anything else is ticks lost, and a run that lost
                // ticks is not the run that was asked for.
                if said.get("game_clock").and_then(Json::as_str) == Some("running") {
                    let mut e = refusal("ticks_lost", format!("{ask} ticks were asked for with the game's clock running and {played} ran: something else has the engine's clock"));
                    e.set("clock", said);
                    return Err(e);
                }
                break;
            }
        }
        Ok(())
    }
}

/// What the interface shows: the screen that has the focus (or the top one), the focused
/// widget and its place among that screen's buttons, and every text, in order.
fn interface(run: &Run) -> Result<Json, Json> {
    // A frame with no input first: the screens are the ones that are up now (a run that ended
    // on its last tick brought one up), laid out with what the modules keep now.
    run.call("ui_act", Json::obj([("surface", Json::from("game")), ("steps", Json::arr([Json::obj([("wait", Json::from(1u64))])]))]))?;
    let mut widgets: Vec<Json> = Vec::new();
    let mut screens = Json::Null;
    let mut offset = 0u64;
    loop {
        let page = run.call("ui_tree", Json::obj([("surface", Json::from("game")), ("offset", Json::from(offset)), ("limit", Json::from(200u64))]))?;
        if screens.is_null() {
            screens = page.get("screens").cloned().unwrap_or(Json::Null);
        }
        widgets.extend(page.get("widgets").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default());
        match page.get("next_offset").and_then(Json::as_u64) {
            Some(next) if widgets.len() < MAX_WIDGETS && next > offset => offset = next,
            _ => break,
        }
    }
    let shown: Vec<&Json> = screens.as_array().map(|s| s.iter().collect()).unwrap_or_default();
    let name = |s: &&Json| s.get("name").and_then(Json::as_str).map(str::to_owned);
    let screen = shown.iter().find(|s| s.get("has_focus").and_then(Json::as_bool) == Some(true)).and_then(name).or_else(|| shown.last().and_then(name));
    let of_screen = |w: &&Json| w.get("screen").and_then(Json::as_str) == screen.as_deref();
    let text_of = |w: &Json| w.get("text").filter(|t| t.as_str().is_some_and(|t| !t.is_empty())).cloned();
    let buttons: Vec<&Json> = widgets.iter().filter(of_screen).filter(|w| w.get("type").and_then(Json::as_str) == Some("button")).collect();
    let focused = buttons.iter().position(|w| w.get("focused").and_then(Json::as_bool) == Some(true));
    Ok(Json::obj([
        ("screen", screen.as_deref().map_or(Json::Null, Json::from)),
        ("screens", Json::arr(shown.iter().filter_map(name).map(|n| Json::from(n.as_str())))),
        ("focus", focused.and_then(|i| buttons[i].get("id").cloned()).unwrap_or(Json::Null)),
        ("focus_index", focused.map_or(Json::Null, |i| Json::from(i as u64))),
        // Every text that is up, the screens underneath included, in order; and the top
        // screen's alone (a menu over the field: what the menu says).
        ("texts", Json::arr(widgets.iter().filter_map(text_of))),
        ("top_texts", Json::arr(widgets.iter().filter(of_screen).filter_map(text_of))),
    ]))
}

/// The value a report's line shows (`null`: nothing has that key).
fn value_of(key: &str, ticks: u64, seed: Option<u64>, game: &Json, ui: &Json, states: &Json) -> Json {
    let Some((head, rest)) = key.split_once('.') else { return Json::Null };
    let found = match head {
        "run" => match rest {
            "ticks" => Some(Json::from(ticks)),
            "seed" => seed.map(Json::from),
            _ => None,
        },
        "ui" => ui.get(rest).cloned(),
        "setting" => game.get("settings").and_then(|s| s.get(rest)).cloned(),
        "game" => match rest {
            "screen" => game.get("screens").and_then(Json::as_array).and_then(|s| s.last()).cloned(),
            "title" => game.get("game").cloned(),
            other => game.get(other).cloned(),
        },
        module => states.get(module).and_then(|s| s.get(rest)).cloned(),
    };
    found.unwrap_or(Json::Null)
}

fn run(tools: &Tools, gate: &StartGate, spec: &Spec) -> Result<Json, Json> {
    let text = std::fs::read_to_string(specula_ludus::game::FILE).map_err(|e| refusal("no_game", format!("{}: {e} (a headless run plays a project's game file)", specula_ludus::game::FILE)))?;
    let game = GameFile::parse(&text).map_err(|p| refusal("bad_game_file", format!("{}: {}: {} ({})", specula_ludus::game::FILE, p.at, p.message, p.code)))?;
    if gate.wait(START_WAIT).is_err() {
        return Err(refusal("no_engine", "the engine did not start"));
    }
    let mut run = Run { tools, session: session(), modules: Vec::new(), held: Vec::new(), hold_left: 0 };
    for dir in game.modules.iter().take(crate::run_modules::MAX_MODULES) {
        let name = run.build(dir, spec.seed)?;
        run.modules.push((name, Vec::new()));
    }
    for (name, actions) in &mut run.modules {
        let status = read(&tools.call(&run.session, "logic_status", &Json::obj([("module", Json::from(name.as_str()))])))?;
        let listed = status.get("modules").and_then(Json::as_array).and_then(|m| m.first()).and_then(|m| m.get("actions")).and_then(Json::as_array);
        *actions = listed.map(|a| a.iter().filter_map(Json::as_str).map(str::to_owned).collect()).unwrap_or_default();
    }
    // Every word is known before the first tick, and every action's value is of its kind (a
    // button is held, an axis has a number, an axis of two has two): a misspelt input is not a
    // run that did nothing, and none fails a run half way.
    check_span(&spec.inputs, spec.ticks).map_err(|why| refusal("bad_input", why))?;
    let mut probed: Vec<String> = Vec::new();
    for (tick, word) in &spec.inputs {
        for (name, value) in parts(word).map_err(|why| refusal("bad_input", why))? {
            let module = run.modules.iter().find(|(_, actions)| actions.iter().any(|a| a == name)).map(|(module, _)| module.clone());
            let press = matches!(value, Json::Bool(true)) && press_of(name).is_some();
            if name != "none" && !press && module.is_none() {
                let mut e = refusal("unknown_input", format!("'{name}' (at tick {tick}) is not an input of this game"));
                let mut words: Vec<Json> = ["up", "down", "left", "right", "enter", "escape", "none"].map(Json::from).to_vec();
                words.extend(run.modules.iter().flat_map(|(_, actions)| actions.iter().map(|a| Json::from(a.as_str()))));
                e.set("inputs", Json::Arr(words));
                e.set("also", Json::from("words joined by + are held together (up+shoot); an action that is not a button takes a value (throttle=0.5, move=1/0); a source such as Key.Enter presses the interface only, never a module's action: name the action"));
                return Err(e);
            }
            // (Asked of the engine once a part, with no ticks: nothing is held by it.)
            if let Some(module) = module.filter(|_| !probed.iter().any(|p| p == word)) {
                if let Err(mut e) = run.call("logic_input", Json::obj([("module", Json::from(module.as_str())), ("actions", Json::obj([(name, value)])), ("ticks", Json::from(0u64))])) {
                    e.set("error", Json::from("bad_input"));
                    e.set("input", Json::from(word.as_str()));
                    e.set("at_tick", Json::from(*tick));
                    return Err(e);
                }
            }
        }
        probed.push(word.clone());
    }
    let mut tick = 0u64;
    let mut next = 0usize;
    while tick < spec.ticks {
        while let Some((_, word)) = spec.inputs.get(next).filter(|(at, _)| *at <= tick) {
            run.input(word)?;
            next += 1;
        }
        let until = spec.inputs.get(next).map_or(spec.ticks, |(at, _)| (*at).min(spec.ticks));
        run.play(until - tick)?;
        tick = until;
    }
    // (Inputs at the last tick itself are done before the state is read.)
    while let Some((_, word)) = spec.inputs.get(next).filter(|(at, _)| *at <= tick) {
        run.input(word)?;
        next += 1;
    }

    let state = run.call("game_state", Json::obj::<&str>([]))?;
    let ui = interface(&run)?;
    let mut states = Json::obj::<&str>([]);
    for (name, _) in &run.modules {
        let status = run.call("logic_status", Json::obj([("module", Json::from(name.as_str())), ("detailed", Json::Bool(true))]))?;
        let module = status.get("modules").and_then(Json::as_array).and_then(|m| m.first()).cloned().unwrap_or(Json::Null);
        if module.get("status").and_then(Json::as_str).is_some_and(|s| s != "running") {
            let mut e = refusal("module_stopped", format!("the module {name} is not running: the state below is as it stopped"));
            e.set("module", module);
            return Err(e);
        }
        // The status shows a bounded number of the keys a module keeps: a report that read
        // only some of them would print `null` for the rest, and nobody would know.
        let (kept, read) = (module.get("state_keys").and_then(Json::as_u64).unwrap_or(0), module.get("state").and_then(Json::as_object).map_or(0, |s| s.len() as u64));
        if kept > read {
            let mut e = refusal("state_too_wide", format!("the module {name} keeps {kept} values and a run reads {read} of them: keep what belongs together under one key (a list, a map)"));
            e.set("module", Json::from(name.as_str()));
            return Err(e);
        }
        states.set(name, module.get("state").cloned().unwrap_or(Json::Null));
    }
    let report = if game.report.is_empty() {
        Json::obj([("tick", Json::from(tick)), ("game", state.clone()), ("ui", ui), ("state", states)])
    } else {
        Json::Obj(game.report.iter().map(|(name, key)| (name.clone(), value_of(key, tick, spec.seed, &state, &ui, &states))).collect())
    };
    if let Some(frame) = &spec.frame {
        let mut args = Json::obj([("width", Json::from(u64::from(spec.size.0))), ("height", Json::from(u64::from(spec.size.1))), ("include_ui", Json::Bool(true))]);
        match game.view {
            Some(View::Flat { center, height }) => {
                args.set("view_2d", Json::obj([("center", Json::arr(center.map(Json::Num))), ("height_m", Json::Num(height))]));
            }
            Some(View::Eye { eye, target, fov_y_deg }) => {
                args.set("eye", Json::arr(eye.map(Json::Num)));
                args.set("target", Json::arr(target.map(Json::Num)));
                args.set("fov_y_deg", Json::Num(fov_y_deg));
            }
            None => {}
        }
        let shot = run.call("sight_frame", args)?;
        let made = shot.get("image").and_then(|i| i.get("path")).and_then(Json::as_str).ok_or_else(|| refusal("no_frame", "the frame was not written"))?;
        crate::disk::book().copy(crate::disk::Category::Captures, "script", Path::new(made), frame, crate::disk::Need::Essential).map_err(|e| refusal("no_frame", format!("{}: {}", frame.display(), e.to_json())))?;
    }
    let mut answer = Json::obj([("report", report)]);
    // What the game says is wrong with its own files rides beside the report, never inside it.
    if let Some(problems) = state.get("problems").filter(|p| p.as_array().is_some_and(|p| !p.is_empty())) {
        answer.set("problems", problems.clone());
    }
    Ok(answer)
}

/// What the run's answer file holds: `{"ok": true, "report": ..., "problems": [...]}` or
/// `{"ok": false, "error": {...}}`.
fn answer_of(result: Result<Json, Json>) -> Json {
    match result {
        Ok(mut answer) => {
            answer.set("ok", Json::Bool(true));
            answer
        }
        Err(error) => Json::obj([("ok", Json::Bool(false)), ("error", error)]),
    }
}

/// Runs `spec` once the engine's loop has begun, writes its answer and asks the engine to
/// stop. Returns at once.
pub(crate) fn start(tools: Arc<Tools>, gate: Arc<StartGate>, spec: Spec) {
    let out = spec.out.clone();
    let spawned = std::thread::Builder::new().name("specula-script".into()).spawn(move || {
        let answer = answer_of(run(&tools, &gate, &spec));
        write_answer(&spec.out, &answer);
        specula_sys::interrupt::request();
    });
    if let Err(e) = spawned {
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "script.no_thread", why = e.to_string().as_str());
        write_answer(&out, &answer_of(Err(refusal("no_thread", e.to_string()))));
        specula_sys::interrupt::request();
    }
}

/// The code of the answer of an engine that was refused its project's lock.
pub const PROJECT_OPEN: &str = "project_open";

/// The answer of an engine started for a run whose project another engine has open.
pub(crate) fn project_open(name: &str) -> Json {
    answer_of(Err(refusal(PROJECT_OPEN, format!("{name} is open in another engine: a headless run needs the project to itself"))))
}

/// The answer, for the command that started this engine to read when the engine has ended.
pub(crate) fn write_answer(out: &Path, answer: &Json) {
    let done = crate::disk::book().write_synced(crate::disk::Category::Scratch, "script", out, answer.to_string().as_bytes(), crate::disk::Need::Essential);
    if let Err(e) = done {
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "script.no_answer", why = e.to_json().to_string().as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_is_ticks_and_words_in_order() {
        assert_eq!(parse_inputs("0:enter, 5:up,90:none").unwrap(), vec![(0, "enter".to_owned()), (5, "up".to_owned()), (90, "none".to_owned())]);
        assert_eq!(parse_inputs("").unwrap(), Vec::new());
        assert_eq!(parse_inputs("3:Key.Space\n3:shoot").unwrap().len(), 2, "two at one tick, and a line each");
        assert!(parse_inputs("enter").unwrap_err().contains("TICK:WORD"));
        assert!(parse_inputs("x:enter").unwrap_err().contains("whole number"));
        assert!(parse_inputs("9:up,3:down").unwrap_err().contains("order"));
        assert!(parse_inputs("1:").unwrap_err().contains("not an input"));
        let many: Vec<String> = (0..=MAX_INPUTS).map(|i| format!("{i}:up")).collect();
        assert!(parse_inputs(&many.join(",")).unwrap_err().contains("at most"));
    }

    #[test]
    fn a_spec_is_read_back_as_it_was_written() {
        let spec = Spec { ticks: 600, seed: Some(7), inputs: vec![(0, "enter".into()), (5, "up".into())], frame: Some(PathBuf::from("out/f.png")), size: (800, 480), out: PathBuf::from("answer.json") };
        assert_eq!(Spec::from_json(&spec.to_json()).unwrap(), spec);
        let bare = Spec { ticks: 0, seed: None, inputs: Vec::new(), frame: None, size: (960, 540), out: PathBuf::from("a") };
        assert_eq!(Spec::from_json(&Json::parse(r#"{"ticks": 0, "out": "a"}"#).unwrap()).unwrap(), bare);
        let bad = |text: &str| Spec::from_json(&Json::parse(text).unwrap()).unwrap_err();
        assert!(bad(r#"{"ticks": 2000000, "out": "a"}"#).contains("ticks"));
        assert!(bad(r#"{"ticks": 1, "out": "a", "size": [10, 480]}"#).contains("size"));
        assert!(bad(r#"{"ticks": 1}"#).contains("out"));
        assert!(bad(r#"{"ticks": 1, "out": "a", "inputs": [[5, "up"], [1, "down"]]}"#).contains("order"));
    }

    #[test]
    fn nobody_elses_word_moves_a_scripts_clock() {
        for op in [TimeOp::Pause, TimeOp::Resume, TimeOp::Step(5), TimeOp::Scale(64.0)] {
            assert!(clock_refused(op, true), "{op:?}");
            assert!(!clock_refused(op, false), "{op:?}: no script, no refusal");
        }
        assert!(!clock_refused(TimeOp::Info, true) && !clock_refused(TimeOp::Play(5), true));
    }

    #[test]
    fn a_word_is_a_press_an_action_or_both() {
        assert_eq!((press_of("enter"), press_of("escape"), press_of("up"), press_of("Key.Space")), (Some("accept"), Some("back"), Some("up"), Some("Key.Space")));
        assert_eq!((press_of("shoot"), press_of("none")), (None, None));
    }

    #[test]
    fn a_reports_line_shows_the_value_its_key_names() {
        let j = |text: &str| Json::parse(text).unwrap();
        let game = j(r#"{"game": "Hold", "phase": "running", "score": 30, "screens": ["hud", "pause"], "settings": {"volume": 7, "language": "es"}}"#);
        let ui = j(r#"{"screen": "pause", "focus_index": 1, "texts": ["Paused", "Resume"]}"#);
        let states = j(r#"{"gate": {"wave": 2, "walkers": [{"x": 3.5, "y": 2, "health": 1}]}}"#);
        let v = |key: &str| value_of(key, 120, Some(4), &game, &ui, &states).to_string();
        assert_eq!((v("run.ticks"), v("run.seed"), v("game.phase"), v("game.screen"), v("game.title")), ("120".into(), "4".into(), "\"running\"".into(), "\"pause\"".into(), "\"Hold\"".into()));
        assert_eq!((v("setting.volume"), v("ui.focus_index"), v("ui.texts"), v("gate.wave")), ("7".into(), "1".into(), "[\"Paused\",\"Resume\"]".into(), "2".into()));
        assert_eq!(v("gate.walkers"), r#"[{"x":3.5,"y":2,"health":1}]"#);
        // Nothing has the key: null, and the line is still there.
        assert_eq!((v("gate.nothing"), v("other.key"), v("plain"), v("setting.none")), ("null".into(), "null".into(), "null".into(), "null".into()));
    }

    #[test]
    fn an_answer_says_whether_the_run_was_made() {
        let ok = answer_of(Ok(Json::obj([("report", Json::obj([("tick", Json::from(3u64))]))])));
        assert_eq!(ok.to_string(), r#"{"report":{"tick":3},"ok":true}"#);
        let failed = answer_of(Err(refusal("build_failed", "modules/gate did not build")));
        assert_eq!(failed.get("ok").and_then(Json::as_bool), Some(false));
        assert_eq!(failed.get("error").and_then(|e| e.get("error")).and_then(Json::as_str), Some("build_failed"));
    }
}
