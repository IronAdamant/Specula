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

mod find;
pub use find::{Find, DEFAULT_FIND_SECONDS, DEFAULT_FIND_TRIES, DEFAULT_STEP, MAX_FIND_SECONDS, MAX_SWEEP, MAX_TRIES};
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
/// Most steps a followed run reads (each prints the state).
pub const MAX_STEPS: u64 = 100_000;
/// Most things one line of a report lists (`world.#TAG.positions`).
const WORLD_LINE: u64 = 200;
/// Most characters of one step's line.
const MAX_STEP_LINE: usize = 200;
/// Most of a module's own log lines an answer carries (the last ones).
const MAX_LOG: usize = 20;
/// Most stretches of the game's clock a run keeps the place of (one begins wherever the clock
/// started again: at most one an input).
const MAX_SPANS: usize = 4096;
/// The byte a followed run's state line begins with, on the engine's standard error: the
/// command that started the engine passes such a line on as the run's output, and keeps every
/// other as the engine's own words.
pub const STATE_MARK: u8 = 0x1E;

/// What a run is asked: `specula run` writes it, the engine reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub ticks: u64,
    pub seed: Option<u64>,
    /// At a tick, a word. In the order given; ticks never go back.
    pub inputs: Vec<(u64, String)>,
    pub frame: Option<PathBuf>,
    pub size: (u32, u32),
    /// After the script: steps read from standard input, a line each, the state printed after
    /// each (`specula run --follow`).
    pub follow: bool,
    /// Ticks on the way at which the state is printed too, in order (`--at 90,180,270`): what
    /// a thing does over a run, read from one run.
    pub at: Vec<u64>,
    /// The report's lines that are printed, by name (`--keys ball,score`); empty: all of them.
    /// A name that is no line of the report and is a key itself (`run.sounds_heard`,
    /// `game.phase`, `gate.wave`) is printed under its own name: what the engine knows is read
    /// without first being written into the game file.
    pub keys: Vec<String>,
    /// Names the command itself reads (a condition's, a check's): printed too where they are
    /// keys and no lines, whatever `keys` says.
    pub also: Vec<String>,
    /// Numbers the rules read with `Cx::param`, for this run (`--set grip=0.3`).
    pub set: Vec<(String, f64)>,
    /// `specula find`: after the script, the engine looks for inputs that reach a condition.
    pub find: Option<Find>,
    /// Where the answer goes.
    pub out: PathBuf,
}

/// Most ticks on the way one run prints the state at.
pub const MAX_AT: usize = 256;

/// The ticks of `--at`: whole numbers that rise, at most [`MAX_AT`] of them.
pub fn check_at(at: &[u64]) -> Result<(), String> {
    if at.len() > MAX_AT || at.iter().any(|t| *t > MAX_TICKS) || at.windows(2).any(|w| w[1] <= w[0]) {
        return Err(format!("at: at most {MAX_AT} ticks, each later than the one before (90,180,270), none past {MAX_TICKS}"));
    }
    Ok(())
}

/// A run's parameters: at most [`crate::logic::MAX_PARAMS`], each a name (a letter, then
/// letters, digits and `_`, 64 at most) and a number.
pub fn check_set(set: &[(String, f64)]) -> Result<(), String> {
    let name_ok = |n: &str| n.len() <= 64 && n.starts_with(|c: char| c.is_ascii_alphabetic()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if set.len() > crate::logic::MAX_PARAMS || set.iter().any(|(name, value)| !name_ok(name) || !value.is_finite()) {
        return Err(format!("set: at most {} parameters, each NAME=NUMBER (grip=0.3), a name being a letter then letters, digits and _", crate::logic::MAX_PARAMS));
    }
    if let Some((name, _)) = set.iter().enumerate().find(|(i, (name, _))| set[..*i].iter().any(|(n, _)| n == name)).map(|(_, s)| s) {
        return Err(format!("set: '{name}' is given twice"));
    }
    Ok(())
}

/// The names of `--keys`: at most as many as a report has lines.
pub fn check_keys(keys: &[String]) -> Result<(), String> {
    if keys.len() > specula_ludus::game::MAX_REPORT || keys.iter().any(|k| k.is_empty() || k.len() > 64 || k.chars().any(|c| c.is_whitespace() || c.is_control())) {
        return Err(format!("keys: at most {} names of the report's lines (ball,score)", specula_ludus::game::MAX_REPORT));
    }
    Ok(())
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
        if self.follow {
            j.set("follow", Json::Bool(true));
        }
        if !self.at.is_empty() {
            j.set("at", Json::arr(self.at.iter().map(|t| Json::from(*t))));
        }
        if !self.keys.is_empty() {
            j.set("keys", Json::arr(self.keys.iter().map(|k| Json::from(k.as_str()))));
        }
        if !self.also.is_empty() {
            j.set("also", Json::arr(self.also.iter().map(|k| Json::from(k.as_str()))));
        }
        if !self.set.is_empty() {
            j.set("set", Json::Obj(self.set.iter().map(|(name, value)| (name.clone(), Json::Num(*value))).collect()));
        }
        if let Some(find) = &self.find {
            j.set("find", find.to_json());
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
        let at: Vec<u64> = match j.get("at") {
            None => Vec::new(),
            Some(at) => at.as_array().and_then(|a| a.iter().map(Json::as_u64).collect()).ok_or("at: a list of ticks")?,
        };
        check_at(&at)?;
        if at.last().is_some_and(|last| *last > ticks) {
            return Err("at: no tick is later than the run's last".to_owned());
        }
        let keys: Vec<String> = match j.get("keys") {
            None => Vec::new(),
            Some(keys) => keys.as_array().and_then(|k| k.iter().map(|k| k.as_str().map(str::to_owned)).collect()).ok_or("keys: a list of names")?,
        };
        check_keys(&keys)?;
        let also: Vec<String> = match j.get("also") {
            None => Vec::new(),
            Some(also) => also.as_array().and_then(|k| k.iter().map(|k| k.as_str().map(str::to_owned)).collect()).ok_or("also: a list of names")?,
        };
        check_keys(&also)?;
        let set: Vec<(String, f64)> = match j.get("set") {
            None => Vec::new(),
            Some(set) => set.as_object().and_then(|s| s.iter().map(|(name, value)| value.as_f64().map(|v| (name.clone(), v))).collect()).ok_or("set: an object of numbers")?,
        };
        check_set(&set)?;
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
            follow: j.get("follow").and_then(Json::as_bool).unwrap_or(false),
            at,
            keys,
            also,
            set,
            find: j.get("find").map(Find::from_json).transpose()?,
            out: j.get("out").and_then(Json::as_str).map(PathBuf::from).ok_or("out: where the answer is written")?,
        })
    }
}

/// What a search is told to reach, or a tuning to hold: comparisons over the report's lines,
/// all of which must hold. `screen==won`, `score>=30`, `ball.y<-5` (a part of a line that is
/// an object), `screen==won&&lives>=2`. A value is a number, `true`, `false`, or a word.
#[derive(Clone, Debug, PartialEq)]
pub struct Until(pub Vec<(String, Op, Json)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Ge,
    Le,
    Gt,
    Lt,
}

const OPS: [(&str, Op); 6] = [("==", Op::Eq), ("!=", Op::Ne), (">=", Op::Ge), ("<=", Op::Le), (">", Op::Gt), ("<", Op::Lt)];
/// Most comparisons one condition joins.
const MAX_TERMS: usize = 8;

impl Until {
    pub fn parse(text: &str) -> Result<Until, String> {
        let shape = || format!("'{}' is not a condition: LINE OP VALUE with == != >= <= > <, several joined by && (screen==won&&score>=30)", text.chars().take(80).collect::<String>());
        let mut terms = Vec::new();
        for term in text.split("&&").map(str::trim) {
            let (at, (sign, op)) = OPS.iter().filter_map(|o| term.find(o.0).map(|at| (at, *o))).min_by_key(|(at, (sign, _))| (*at, std::cmp::Reverse(sign.len()))).ok_or_else(shape)?;
            let (name, value) = (term[..at].trim(), term[at + sign.len()..].trim());
            if name.is_empty() || value.is_empty() || name.len() > 128 || value.len() > 128 || name.chars().any(char::is_whitespace) {
                return Err(shape());
            }
            let value = match value {
                "true" => Json::Bool(true),
                "false" => Json::Bool(false),
                v => v.parse::<f64>().ok().filter(|n| n.is_finite()).map_or_else(|| Json::from(v), Json::Num),
            };
            if !matches!(op, Op::Eq | Op::Ne) && value.as_f64().is_none() {
                return Err(format!("'{term}': only numbers are compared with > and <"));
            }
            terms.push((name.to_owned(), op, value));
        }
        if terms.is_empty() || terms.len() > MAX_TERMS {
            return Err(shape());
        }
        Ok(Until(terms))
    }

    pub fn text(&self) -> String {
        let sign = |op: Op| OPS.iter().find(|o| o.1 == op).map_or("", |o| o.0);
        let word = |v: &Json| v.as_str().map_or_else(|| v.to_string(), str::to_owned);
        self.0.iter().map(|(name, op, value)| format!("{name}{}{}", sign(*op), word(value))).collect::<Vec<_>>().join("&&")
    }

    /// The value a name reads in a report: a line, or a part of a line that is an object
    /// (`ball.y`). A line whose own name has a dot in it is found first.
    pub fn read<'r>(report: &'r Json, name: &str) -> Option<&'r Json> {
        if let Some(v) = report.get(name) {
            return Some(v);
        }
        let (line, part) = name.split_once('.')?;
        part.split('.').try_fold(report.get(line)?, |v, key| v.get(key))
    }

    /// Whether every comparison holds of `report`. A line the report has not, or one that is
    /// not a number where numbers are compared, holds nothing but `!=`.
    pub fn holds(&self, report: &Json) -> bool {
        self.0.iter().all(|(name, op, want)| {
            let got = Until::read(report, name);
            let same = match (got, want) {
                (Some(g), w) if g.as_f64().is_some() && w.as_f64().is_some() => g.as_f64() == w.as_f64(),
                (Some(g), w) => g == w,
                (None, _) => false,
            };
            let numbers = got.and_then(Json::as_f64).zip(want.as_f64());
            match op {
                Op::Eq => same,
                Op::Ne => !same,
                Op::Ge => numbers.is_some_and(|(g, w)| g >= w),
                Op::Le => numbers.is_some_and(|(g, w)| g <= w),
                Op::Gt => numbers.is_some_and(|(g, w)| g > w),
                Op::Lt => numbers.is_some_and(|(g, w)| g < w),
            }
        })
    }

    /// Every name it reads.
    pub fn names(&self) -> Vec<String> {
        self.0.iter().map(|(name, _, _)| name.clone()).collect()
    }

    /// The names it reads that `report` has no value for: said before a search is begun.
    pub fn unread(&self, report: &Json) -> Vec<String> {
        self.0.iter().filter(|(name, _, _)| Until::read(report, name).is_none()).map(|(name, _, _)| name.clone()).collect()
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

/// One step of a followed run, as a line gives it: `TICKS` (that many ticks with what is held)
/// or `TICKS WORD` (the input given first: `10 up+shoot`, `1 enter`, `30 none`).
pub fn parse_step(line: &str) -> Result<(u64, Option<&str>), String> {
    let shape = || format!("'{}' is not a step: TICKS, or TICKS WORD (10 up+shoot, 1 enter, 30 none)", line.chars().take(40).collect::<String>());
    if line.len() > MAX_STEP_LINE {
        return Err(shape());
    }
    let mut words = line.split_whitespace();
    let ticks = words.next().and_then(|t| t.parse::<u64>().ok()).filter(|t| *t <= MAX_TICKS).ok_or_else(shape)?;
    let word = words.next();
    if words.next().is_some() {
        return Err(shape());
    }
    if let Some(word) = word {
        check_inputs(&[(0, word.to_owned())])?;
    }
    Ok((ticks, word))
}

/// `time_control` is refused while a script has the clock, except `info` (and the script's own
/// ticks): as while a playtest runs.
pub fn clock_refused(op: TimeOp, scripted: bool) -> bool {
    scripted && !matches!(op, TimeOp::Info | TimeOp::Play(_))
}

/// The seeds a run takes: whole numbers a JSON number holds exactly.
pub const MAX_SEED: u64 = (1 << 53) - 1;
/// The seed of a run that names none (`specula run` with no `--seed`): a headless run is the
/// same run every time. (Four of five judges of the second pass's engine games noted that a
/// run with no seed was not the run with seed 1 that their brief called the default.)
pub const DEFAULT_SEED: u64 = 1;

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

/// What a tool answered, as JSON.
fn said(out: &ToolOutput) -> Json {
    out.content
        .iter()
        .find_map(|c| match c {
            Content::Text(t) => Json::parse(t).ok(),
            Content::Image { .. } => None,
        })
        .unwrap_or(Json::Null)
}

/// A tool's answer as JSON: `Err` when the tool answered an error.
fn read(out: &ToolOutput) -> Result<Json, Json> {
    let json = said(out);
    if out.is_error {
        Err(if json.get("error").is_some() { json } else { refusal("refused", json.to_string()) })
    } else {
        Ok(json)
    }
}

/// The key a module's failure or log line carries the script's tick under.
pub const RUN_TICK: &str = "run_tick";

/// Every module's lines as one log, in the order of their ticks (a module's own lines stay in
/// the order it wrote them), the last [`MAX_LOG`] of them.
fn log_in_order(log: &mut Vec<Json>) {
    log.sort_by_key(|line| line.get("tick").and_then(Json::as_u64).unwrap_or(0));
    let over = log.len().saturating_sub(MAX_LOG);
    log.drain(..over);
}

/// How a game stands, as [`Run::stand`] read it.
struct Stand {
    report: Json,
    /// `game_state`'s answer.
    state: Json,
    /// The last lines the modules logged (`cx.log`), each with its tick.
    log: Vec<Json>,
    /// What a screen shows as written because nothing fills it ([`unfilled_texts`]).
    unfilled: Vec<String>,
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
    /// The script's ticks played so far (every one, whatever screen was up).
    at: u64,
    /// Where the game's own ticks fell among the script's: (the game's first tick, the script's
    /// tick it was, how many in a row). The game's clock stands at the title, in a pause and
    /// after a run's end, so the two counts part; what a module's failure and its log lines
    /// carry is the game's, and what the script's writer counts is the script's.
    spans: Vec<(u64, u64, u64)>,
    /// The game file's own words for inputs (`"inputs"`): each with what it stands for.
    words: Vec<(String, String)>,
    /// The report's lines that are printed (`--keys`); empty: all.
    lines: Vec<(String, String, bool)>,
}

/// `word` with the game file's own words replaced by what they stand for, part by part
/// (`menu_down` is `down`; `fire+left` is `up+shoot+left`). One pass: what a word stands for is
/// an input of the engine's or a module's, never another such word.
fn in_game_words(word: &str, words: &[(String, String)]) -> String {
    if words.is_empty() {
        return word.to_owned();
    }
    let parts: Vec<&str> = word.split('+').map(|part| words.iter().find(|(w, _)| w == part).map_or(part, |(_, means)| means.as_str())).collect();
    parts.join("+")
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
        // (The tool's own answer, whether it is an error or not: a build that loaded nothing
        // answers what went wrong as data, with no `error` of its own at the top.)
        let answer = said(&out);
        let name = answer.get("reload").and_then(|r| r.get("module")).and_then(Json::as_str);
        if let (false, Some(name)) = (out.is_error, name) {
            return Ok(name.to_owned());
        }
        // A build that failed: how many errors, and the first of them, each where it is. One
        // that built with warnings and did not load is a load that failed, and says why (it
        // was said as "did not build", with no error to read, until a game tried by hand on
        // 2026-11-21 had its actions file wrong and a warning in its rules).
        let has_errors = |all: &[Json]| all.iter().any(|d| d.get("level").and_then(Json::as_str) != Some("warning"));
        let mut e = match answer.get("diagnostics").and_then(Json::as_array) {
            Some(all) if has_errors(all) || (!all.is_empty() && answer.get("reload_error").is_none()) => {
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
        let word = &in_game_words(word, &self.words);
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

    /// Every part of `word` is one this game takes, with a value of its action's kind (a button
    /// is held, an axis has a number, an axis of two has two): asked before the word is given,
    /// so that a misspelt input is not a run that did nothing.
    fn check_word(&self, word: &str, at: Option<u64>) -> Result<(), Json> {
        let place = at.map_or(String::new(), |t| format!(" (at tick {t})"));
        let word = &in_game_words(word, &self.words);
        for (name, value) in parts(word).map_err(|why| refusal("bad_input", why))? {
            let module = self.modules.iter().find(|(_, actions)| actions.iter().any(|a| a == name)).map(|(module, _)| module.clone());
            let press = matches!(value, Json::Bool(true)) && press_of(name).is_some();
            if name != "none" && !press && module.is_none() {
                let mut e = refusal("unknown_input", format!("'{name}'{place} is not an input of this game"));
                let mut words: Vec<Json> = ["up", "down", "left", "right", "enter", "escape", "none"].map(Json::from).to_vec();
                words.extend(self.modules.iter().flat_map(|(_, actions)| actions.iter().map(|a| Json::from(a.as_str()))));
                words.extend(self.words.iter().map(|(w, _)| Json::from(w.as_str())));
                e.set("inputs", Json::Arr(words));
                e.set("also", Json::from("words joined by + are held together (up+shoot); an action that is not a button takes a value (throttle=0.5, move=1/0); a source such as Key.Enter presses the interface only, never a module's action: name the action"));
                return Err(e);
            }
            // (Asked of the engine with no ticks: nothing is held by it.)
            if let Some(module) = module {
                if let Err(mut e) = self.call("logic_input", Json::obj([("module", Json::from(module.as_str())), ("actions", Json::obj([(name, value)])), ("ticks", Json::from(0u64))])) {
                    e.set("error", Json::from("bad_input"));
                    e.set("input", Json::from(word.as_str()));
                    if let Some(at) = at {
                        e.set("at_tick", Json::from(at));
                    }
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// How the game stands after `tick` ticks of the run: the report (as the game file shapes
    /// it), the game's own state, and what the modules logged.
    ///
    /// A module that is not running, or one whose call failed, is an error and never a state:
    /// a call that fails is undone, the whole of it, so the game did not move in that tick,
    /// and a state printed all the same reads as a game whose rules do nothing (two of three
    /// builders of the comparison's second pass spent fifteen and thirty-four requests finding
    /// that out with prints of their own: docs/design/LEVERAGE.md).
    fn stand(&self, game: &GameFile, tick: u64, seed: Option<u64>) -> Result<Stand, Json> {
        let state = self.call("game_state", Json::obj::<&str>([]))?;
        let ui = interface(self)?;
        let mut states = Json::obj::<&str>([]);
        let mut log: Vec<Json> = Vec::new();
        for (name, _) in &self.modules {
            let status = self.call("logic_status", Json::obj([("module", Json::from(name.as_str())), ("detailed", Json::Bool(true))]))?;
            let module = status.get("modules").and_then(Json::as_array).and_then(|m| m.first()).cloned().unwrap_or(Json::Null);
            let lines = status.get("log").and_then(Json::as_array).unwrap_or(&[]);
            log.extend(lines.iter().skip(lines.len().saturating_sub(MAX_LOG)).map(|line| self.in_run_ticks(line)));
            let failures = module.get("failures").and_then(Json::as_u64).unwrap_or(0);
            let stopped = module.get("status").and_then(Json::as_str).is_some_and(|s| s != "running");
            if failures > 0 || stopped {
                // What went wrong first (a module that was stopped: what stopped it), with the
                // script's tick it happened at beside the game's own.
                let first = module.get("first_error").or_else(|| module.get("last_error")).map_or(Json::Null, |e| self.in_run_ticks(e));
                // (What the message is about: what stopped a stopped module, which may come
                // after failures that did not; else the first failure.)
                let ended = module.get("disabled_by").filter(|_| stopped).map(|e| self.in_run_ticks(e));
                let told = ended.as_ref().unwrap_or(&first);
                let word = |key: &str| told.get(key).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned));
                let what = format!("{}: {}{}", word("code").unwrap_or_else(|| "?".to_owned()), word("message").unwrap_or_default(), word("location").map_or_else(String::new, |at| format!(", at {at}")));
                let when = match (word(RUN_TICK), word("tick")) {
                    (Some(run), _) => format!("at tick {run}"),
                    (None, Some(game)) => format!("at the game's own tick {game} (its place among this run's ticks is not known)"),
                    (None, None) => "before a tick was played".to_owned(),
                };
                let mut e = refusal(
                    "module_failed",
                    if stopped {
                        format!("the module {name} stopped {when} ({what}): a module that panics, or does not end its call, is stopped, and the game went on without its rules, so there is no state of it to print")
                    } else {
                        format!("a call of the module {name} failed in {failures} of this run's ticks, first {when} ({what}). A call that fails is undone, the whole of it: the module's part of the game did not move in those ticks, so there is no state of it to print")
                    },
                );
                e.set("module", Json::from(name.as_str()));
                e.set("failures", Json::from(failures));
                if stopped {
                    e.set("stopped", Json::Bool(true));
                }
                // (What stopped it, when that is another failure than the first.)
                if let Some(ended) = ended.filter(|ended| ended.get("tick") != first.get("tick") || ended.get("code") != first.get("code")) {
                    e.set("stopped_by", ended);
                }
                // (The last one whole only when it is another error than the first.)
                let last = module.get("last_error").filter(|_| failures > 1).map(|l| self.in_run_ticks(l));
                let same = |key: &str| last.as_ref().and_then(|l| l.get(key)) == first.get(key);
                match &last {
                    Some(last) if same("code") && same("message") => drop(e.set("last_tick", last.get(RUN_TICK).or_else(|| last.get("tick")).cloned().unwrap_or(Json::Null))),
                    Some(last) => drop(e.set("last", last.clone())),
                    None => {}
                }
                e.set("first", first);
                log_in_order(&mut log);
                if !log.is_empty() {
                    e.set("log", Json::Arr(log));
                }
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
        let unfilled = unfilled_texts(&ui);
        // What a line reads from the engine itself and no module keeps: a thing's place as
        // physics left it this tick (`world.PATH.position`), the sounds the rules asked for.
        let mut beyond = Json::obj::<&str>([]);
        for (_, key, _) in &self.lines {
            if key == "run.sounds" || key == "run.sounds_heard" {
                let info = self.call("engine_info", Json::obj([("response_format", Json::from("detailed"))]))?;
                beyond.set(key, info.get("module_sounds").and_then(|s| s.get(if key == "run.sounds" { "order" } else { "heard" })).cloned().unwrap_or_else(|| Json::Arr(Vec::new())));
            } else if let Some(asked) = key.strip_prefix("world.") {
                beyond.set(key, self.of_world(asked)?);
            }
        }
        let report = if game.report.is_empty() {
            Json::obj([("tick", Json::from(tick)), ("game", state.clone()), ("ui", ui), ("state", states)])
        } else {
            // (A key asked for by itself that has no value is left out: it is then a line the
            // run has not, and whoever asked is told so, as for a name mistyped.)
            Json::Obj(self.lines.iter().map(|(name, key, own)| (name.clone(), value_of(key, tick, seed, &state, &ui, &states, &beyond), *own)).filter(|(_, value, own)| *own || !value.is_null()).map(|(name, value, _)| (name, value)).collect())
        };
        log_in_order(&mut log);
        Ok(Stand { report, state, log, unfilled })
    }

    /// A report's line that reads the world itself, as it stands when the state is read (after
    /// this tick's physics; what a module copies into its state in its own tick is a step
    /// behind that). `PATH.position` (`{x, y, z}`, in the world), `PATH.x` (`y`, `z`),
    /// `PATH.velocity`, `PATH.exists`; and of everything with a tag, in the order of their
    /// paths: `#TAG.positions` (the first [`WORLD_LINE`]) and `#TAG.count`. `null`: no such thing.
    fn of_world(&self, asked: &str) -> Result<Json, Json> {
        let Some((which, field)) = asked.rsplit_once('.') else { return Ok(Json::Null) };
        let xyz = |v: Option<&Json>| match v.and_then(Json::as_array) {
            Some([x, y, z]) => Json::obj([("x", x.clone()), ("y", y.clone()), ("z", z.clone())]),
            _ => Json::Null,
        };
        let fields = Json::arr(["path", "world_position", "body"].map(Json::from));
        if let Some(tag) = which.strip_prefix('#') {
            let found = self.call("world_query", Json::obj([("tags", Json::arr([Json::from(tag)])), ("limit", Json::from(WORLD_LINE)), ("fields", fields)]))?;
            let items = found.get("items").and_then(Json::as_array).unwrap_or(&[]);
            return Ok(match field {
                "positions" => Json::arr(items.iter().map(|i| xyz(i.get("world_position")))),
                "count" => found.get("total").cloned().unwrap_or(Json::Null),
                _ => Json::Null,
            });
        }
        // (A path nothing is at is "no such thing", as a tag nothing has is an empty list.)
        let found = match self.call("world_query", Json::obj([("path_prefix", Json::from(which)), ("limit", Json::from(1u64)), ("fields", fields)])) {
            Err(e) if e.get("error").and_then(Json::as_str) == Some("not_found") => Json::Null,
            other => other?,
        };
        let item = found.get("items").and_then(Json::as_array).and_then(|i| i.first()).filter(|i| i.get("path").and_then(Json::as_str) == Some(which));
        let Some(item) = item else { return Ok(if field == "exists" { Json::Bool(false) } else { Json::Null }) };
        let at = xyz(item.get("world_position"));
        Ok(match field {
            "position" => at,
            "x" | "y" | "z" => at.get(field).cloned().unwrap_or(Json::Null),
            "velocity" => match item.get("body").filter(|b| !b.is_null()) {
                Some(body) => match xyz(body.get("velocity")) {
                    Json::Null => Json::obj([("x", Json::Num(0.0)), ("y", Json::Num(0.0)), ("z", Json::Num(0.0))]),
                    v => v,
                },
                None => Json::Null,
            },
            "exists" => Json::Bool(true),
            _ => Json::Null,
        })
    }

    /// A failure or a log line of a module, with the script's tick it happened at
    /// ([`RUN_TICK`]) beside the game's own (`tick`), where a tick of this run was that one.
    fn in_run_ticks(&self, record: &Json) -> Json {
        let mut record = record.clone();
        if let Some(run) = record.get("tick").and_then(Json::as_u64).and_then(|game| self.run_tick(game)) {
            record.set(RUN_TICK, Json::from(run));
        }
        record
    }

    /// The script's tick a tick of the game was (`None`: none this run played, such as a
    /// module's start).
    fn run_tick(&self, game_tick: u64) -> Option<u64> {
        self.spans.iter().find(|(game, _, count)| (*game..*game + *count).contains(&game_tick)).map(|(game, run, _)| run + (game_tick - game))
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
            let played = said.get("played").and_then(Json::as_u64).unwrap_or(0).min(ask);
            self.hold_left = self.hold_left.saturating_sub(played);
            // The ticks that ran were the first of the span asked for (a clock that stops,
            // stops for the rest of it), and the clock says the tick it has reached.
            if let Some(first) = said.get("tick").and_then(Json::as_u64).and_then(|after| after.checked_sub(played)).filter(|_| played > 0) {
                let (at, room) = (self.at, self.spans.len() < MAX_SPANS);
                match self.spans.last_mut() {
                    Some((game, run, count)) if *game + *count == first && *run + *count == at => *count += played,
                    _ if room => self.spans.push((first, at, played)),
                    _ => {}
                }
            }
            self.at += if played < ask { left } else { played };
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
    // A button's words are a label inside it: the first text of its screen that lies within
    // its rectangle.
    let rect = |w: &Json| -> Option<[f64; 4]> {
        let r = w.get("rect")?.as_array()?;
        Some([r.first()?.as_f64()?, r.get(1)?.as_f64()?, r.get(2)?.as_f64()?, r.get(3)?.as_f64()?])
    };
    let within = |inner: [f64; 4], outer: [f64; 4]| inner[0] >= outer[0] - 0.5 && inner[1] >= outer[1] - 0.5 && inner[0] + inner[2] <= outer[0] + outer[2] + 0.5 && inner[1] + inner[3] <= outer[1] + outer[3] + 0.5;
    let words_on = |button: &Json| -> Option<Json> {
        let outer = rect(button)?;
        widgets.iter().filter(of_screen).filter(|w| rect(w).is_some_and(|inner| within(inner, outer))).find_map(text_of)
    };
    Ok(Json::obj([
        ("screen", screen.as_deref().map_or(Json::Null, Json::from)),
        ("screens", Json::arr(shown.iter().filter_map(name).map(|n| Json::from(n.as_str())))),
        // The focused button as a player reads it, and its id (2026-11-22: a brief's "selected"
        // is the words on the button; five games shipped the id, or another screen's texts).
        ("focus", focused.and_then(|i| words_on(buttons[i]).or_else(|| buttons[i].get("id").cloned())).unwrap_or(Json::Null)),
        ("focus_id", focused.and_then(|i| buttons[i].get("id").cloned()).unwrap_or(Json::Null)),
        ("focus_index", focused.map_or(Json::Null, |i| Json::from(i as u64))),
        // The top screen's texts (a menu over the field: what the menu says), under both
        // names it has had; and every text that is up, the screens underneath included.
        ("texts", Json::arr(widgets.iter().filter(of_screen).filter_map(text_of))),
        ("top_texts", Json::arr(widgets.iter().filter(of_screen).filter_map(text_of))),
        ("all_texts", Json::arr(widgets.iter().filter_map(text_of))),
    ]))
}

/// The value a report's line shows (`null`: nothing has that key).
fn value_of(key: &str, ticks: u64, seed: Option<u64>, game: &Json, ui: &Json, states: &Json, beyond: &Json) -> Json {
    let Some((head, rest)) = key.split_once('.') else { return Json::Null };
    let found = match head {
        "world" => beyond.get(key).cloned(),
        "run" => match rest {
            "sounds" | "sounds_heard" => beyond.get(key).cloned(),
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
    let mut run = Run { tools, session: session(), modules: Vec::new(), held: Vec::new(), hold_left: 0, at: 0, spans: Vec::new(), words: game.inputs.clone(), lines: lines_of(&game, &spec.keys, &spec.also) };
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
    let mut probed: Vec<&str> = Vec::new();
    for (tick, word) in &spec.inputs {
        if !probed.contains(&word.as_str()) {
            run.check_word(word, Some(*tick))?;
            probed.push(word);
        }
    }
    // A name of `--keys` that the report has no line of is said before a tick is played.
    if let Some(name) = spec.keys.iter().find(|k| !game.report.iter().any(|(name, _)| name == *k) && !k.contains('.')) {
        let mut e = refusal("unknown_key", format!("'{name}' is not a line of this game's report (game.json, \"report\"), and not a key (run.sounds_heard, game.phase, ui.top_texts, a module's gate.wave: notes/how.md lists them)"));
        e.set("lines", Json::arr(game.report.iter().map(|(name, _)| Json::from(name.as_str()))));
        return Err(e);
    }
    let mut tick = 0u64;
    let mut next = 0usize;
    // The state at each tick of `--at`, in order: read as the run passes it, with the inputs
    // of that tick done first, exactly as a run that ended there would read it.
    let mut on_the_way: Vec<Json> = Vec::new();
    // (A search begins where the script's last input leaves the game: the run's ticks are
    // then the most a script found may be long.)
    let play_to = if spec.find.is_some() { spec.inputs.last().map_or(0, |(at, _)| *at) } else { spec.ticks };
    let mut stops = spec.at.iter().copied().filter(|t| *t < play_to).peekable();
    loop {
        let stop = stops.peek().copied().unwrap_or(play_to);
        while tick < stop {
            while let Some((_, word)) = spec.inputs.get(next).filter(|(at, _)| *at <= tick) {
                run.input(word)?;
                next += 1;
            }
            let until = spec.inputs.get(next).map_or(stop, |(at, _)| (*at).min(stop));
            run.play(until - tick)?;
            tick = until;
        }
        // (Inputs at the tick itself are done before the state is read.)
        while let Some((_, word)) = spec.inputs.get(next).filter(|(at, _)| *at <= tick) {
            run.input(word)?;
            next += 1;
        }
        if stops.next().is_none() {
            break;
        }
        on_the_way.push(run.stand(&game, tick, spec.seed)?.report);
    }
    if let Some(asked) = &spec.find {
        let answer = find::find(&mut run, &game, spec, asked, tick, &spec.inputs)?;
        return Ok(Json::obj([("report", answer), ("searched", Json::Bool(true))]));
    }

    let mut stand = run.stand(&game, tick, spec.seed)?;
    let mut followed = None;
    if spec.follow {
        // The state now, then a step a line: whoever plays reads the one and writes the other.
        say(&stand.report)?;
        let mut steps = 0u64;
        let mut input = std::io::stdin().lock();
        loop {
            let line = match step_line(&mut input) {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(e) => return Err(refusal("no_input", format!("the run's steps could not be read ({e})"))),
            };
            // (A line that is too long, or is not text, is a line that is no step.)
            let line = line.as_deref().map(str::trim);
            if line == Ok("") {
                continue;
            }
            steps += 1;
            if steps > MAX_STEPS {
                return Err(refusal("too_many_steps", format!("a followed run reads at most {MAX_STEPS} lines (the ones it refuses too): this one ends here, after {tick} ticks")));
            }
            // A line that is not a step is answered on its own line, and the run goes on: the
            // player reads one line for each it wrote.
            let step = line.map_err(|why| refusal("bad_step", why.to_owned())).and_then(|line| parse_step(line).map_err(|why| refusal("bad_step", why))).and_then(|(ticks, word)| {
                if tick + ticks > MAX_TICKS {
                    return Err(refusal("too_many_ticks", format!("a run plays at most {MAX_TICKS} ticks")));
                }
                if let Some(word) = word {
                    // (Asking whether a word is one this game takes lets go of what it names:
                    // whatever the answer, what was held is held again before anything ticks,
                    // so a word that is refused changes nothing.)
                    let asked = run.check_word(word, None);
                    if !run.held.is_empty() {
                        run.hold()?;
                    }
                    asked?;
                }
                Ok((ticks, word))
            });
            match step {
                Err(e) => say(&e)?,
                Ok((ticks, word)) => {
                    if let Some(word) = word {
                        run.input(word)?;
                    }
                    run.play(ticks)?;
                    tick += ticks;
                    stand = run.stand(&game, tick, spec.seed)?;
                    say(&stand.report)?;
                }
            }
        }
        followed = Some(steps);
    }
    let Stand { report, state, log, unfilled } = stand;
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
    if !spec.at.is_empty() {
        // (The last tick of `--at` may be the run's last: that state is the report itself.)
        if spec.at.last() == Some(&spec.ticks) {
            on_the_way.push(answer.get("report").cloned().unwrap_or(Json::Null));
            answer.set("at_is_all", Json::Bool(true));
        }
        answer.set("at", Json::Arr(on_the_way));
    }
    if let Some(steps) = followed {
        answer.set("followed", Json::from(steps));
    }
    if !log.is_empty() {
        answer.set("log", Json::Arr(log));
    }
    // What the game says is wrong with its own files rides beside the report, never inside it;
    // and with it what a run can see and a player would: a screen showing a key nothing
    // fills, a sound the rules asked for that no file makes. (Neither changes the game's
    // state, so neither fails the run: a builder making 3D with physics and sound by hand,
    // 2026-11-21, shipped a mute game under a screen that read "Taps: {marble.taps}", and
    // the run said nothing.)
    let mut problems: Vec<Json> = state.get("problems").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    problems.extend(unfilled.into_iter().map(Json::from));
    problems.extend(run.silent_sounds()?.into_iter().map(Json::from));
    problems.extend(run.lost_bodies()?.into_iter().map(Json::from));
    problems.extend(run.unread_set()?.into_iter().map(Json::from));
    if !problems.is_empty() {
        answer.set("problems", Json::Arr(problems));
    }
    Ok(answer)
}

/// How a run's problems begin for a number it was given that no rule asked for (`specula tune`
/// looks for these words about the number it varies).
pub fn unread_words(name: &str) -> String {
    format!("{UNREAD_BEGINS}{name}' and no rule read it in this run")
}

/// How [`unread_words`] begins, whatever the name.
pub const UNREAD_BEGINS: &str = "the run set '";

/// The lines a run prints, each a name, the key it shows and whether it is the report's own:
/// the report's lines (those `keys` names, or all of them), then every name of `keys` and
/// `also` that is no line of the report (nor a part of one: `ball.x` is line `ball`'s) and is
/// a key itself, under its own name. So what the engine knows (`run.sounds_heard`,
/// `game.phase`, `world./gate/door.position`, a module's `gate.wave`) is read by its key from
/// a run, a check or a search without the game file naming it first (2026-11-22: four of
/// five builders' games failed an item whose proof was a line none had written in).
fn lines_of(game: &GameFile, keys: &[String], also: &[String]) -> Vec<(String, String, bool)> {
    let line = |k: &str| game.report.iter().any(|(name, _)| name == k || k.split_once('.').is_some_and(|(first, _)| first == name));
    let mut lines: Vec<(String, String, bool)> = game.report.iter().filter(|(name, _)| keys.is_empty() || keys.contains(name)).map(|(name, key)| (name.clone(), key.clone(), true)).collect();
    for key in keys.iter().chain(also) {
        if !line(key) && key.contains('.') && !lines.iter().any(|(name, _, _)| name == key) && lines.len() < 2 * specula_ludus::game::MAX_REPORT {
            lines.push((key.clone(), key.clone(), false));
        }
    }
    lines
}

/// The most problems of one kind a run says (the rest are counted in the last line).
const MAX_SAID: usize = 6;

/// What a screen shows as written because nothing fills it: a text with `{name.key}` left in
/// it. (A binding the state has is replaced before a text is shown, so what is left is
/// unfilled.) One line a key, with the text it shows in.
fn unfilled_texts(ui: &Json) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for text in ui.get("all_texts").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str) {
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let Some(len) = rest[open..].find('}') else { break };
            let key = &rest[open + 1..open + len];
            rest = &rest[open + len + 1..];
            let is_key = key.contains('.') && key.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            if !is_key || seen.contains(&key) {
                continue;
            }
            seen.push(key);
            if out.len() < MAX_SAID {
                let shown: String = text.chars().take(80).collect();
                out.push(format!("a screen shows \"{shown}\": nothing keeps \"{key}\" (a module keeps it with cx.set_state(\"{}\", ..), or the screen's text is changed)", key.split_once('.').map_or(key, |(_, k)| k)));
            }
        }
    }
    if seen.len() > MAX_SAID {
        out.push(format!("and {} more keys that nothing fills", seen.len() - MAX_SAID));
    }
    out
}

impl Run<'_> {
    /// The bodies that are out of the world as the run ends: fallen beyond everything they
    /// could land on (`physics_info`'s `out_of_world`).
    fn lost_bodies(&self) -> Result<Vec<String>, Json> {
        let info = self.call("physics_info", Json::obj::<&str>([]))?;
        let Some(out) = info.get("out_of_world") else { return Ok(Vec::new()) };
        let bodies = out.get("bodies").and_then(Json::as_array).unwrap_or(&[]);
        let mut said: Vec<String> = bodies
            .iter()
            .take(MAX_SAID)
            .map(|b| {
                format!(
                    "{} has fallen out of the world: it is {} m beyond everything it could land on, and still falling (the rules hear the event \"fell\" when it happens: put it back or delete it; notes/world.md)",
                    b.get("path").and_then(Json::as_str).unwrap_or("?"),
                    b.get("beyond_m").and_then(Json::as_f64).unwrap_or(0.0)
                )
            })
            .collect();
        let total = out.get("total").and_then(Json::as_u64).unwrap_or(0) as usize;
        if total > said.len() {
            said.push(format!("and {} more bodies out of the world", total - said.len()));
        }
        Ok(said)
    }

    /// The run's numbers (`--set`, a check's `set`) that no rule asked for: a name mistyped
    /// plays as the game is written, and only this says so.
    fn unread_set(&self) -> Result<Vec<String>, Json> {
        let info = self.call("engine_info", Json::obj([("response_format", Json::from("detailed"))]))?;
        let unread: Vec<&str> = info.get("params_unread").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).collect();
        // Where no number at all was read, the likelier cause is rules that have not come to
        // read them (a run still at its title screen reads no board), not every name mistyped.
        let none_read = info.get("params_read").and_then(Json::as_u64) == Some(0);
        let why = if none_read { "no number was read at all, so the rules that read them may not have run (a game still at its title: begin the run in --inputs); or the name is not the one cx.param is given" } else { "the rules read a number with cx.param(NAME, default), by exactly that name" };
        Ok(unread.iter().take(MAX_SAID).map(|name| format!("{}: {why}", unread_words(name))).collect())
    }

    /// The sounds the rules asked for in this engine's life that no file makes: the engine
    /// says which were asked for and where each is looked for, and the files are looked for
    /// here, on the script's own thread.
    fn silent_sounds(&self) -> Result<Vec<String>, Json> {
        // (The long form: the short one leaves the sounds out.)
        let info = self.call("engine_info", Json::obj([("response_format", Json::from("detailed"))]))?;
        let Some(sounds) = info.get("module_sounds") else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        let mut silent = 0usize;
        for asked in sounds.get("asked").and_then(Json::as_array).unwrap_or(&[]) {
            let paths: Vec<&str> = asked.get("looked_in").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).collect();
            if paths.iter().any(|p| Path::new(p).is_file()) {
                continue;
            }
            silent += 1;
            if out.len() < MAX_SAID {
                let name = asked.get("sound").and_then(Json::as_str).unwrap_or("?");
                let (module, sound) = name.split_once('/').unwrap_or(("", name));
                out.push(format!(
                    "the rules of {module} asked for the sound \"{sound}\" {} time(s) and no file makes it, so nothing plays: write assets/sounds/{sound}.json (notes/world.md says what a sound is; audio_preview measures one)",
                    asked.get("times").and_then(Json::as_u64).unwrap_or(0)
                ));
            }
        }
        if silent > MAX_SAID {
            out.push(format!("and {} more sounds that no file makes", silent - MAX_SAID));
        }
        Ok(out)
    }
}

/// One line of a followed run: the game's state (or what was wrong with a step), for whoever
/// plays to read before it writes its next step. It goes to the command that started this
/// engine, on the engine's standard error, as one write that begins with [`STATE_MARK`]; the
/// command passes it on as its own output. (Not on the engine's standard output: a module is
/// code in this process, and what it prints would land between the states.)
fn say(line: &Json) -> Result<(), Json> {
    use std::io::Write;
    let mut text = vec![STATE_MARK];
    text.extend_from_slice(line.to_string().as_bytes());
    text.push(b'\n');
    let mut out = std::io::stderr().lock();
    out.write_all(&text).and_then(|()| out.flush()).map_err(|e| refusal("no_reader", format!("the run's output could not be written ({e}): the command that started it has gone")))
}

/// The next line of a followed run's steps: `Ok(None)` at the end of the input; `Ok(Some(Err))`
/// for a line that is no step whatever it says (longer than a step is, or not text). Never
/// more than a step's length is kept of a line, however long it is.
fn step_line(input: &mut impl std::io::BufRead) -> std::io::Result<Option<Result<String, &'static str>>> {
    use std::io::{BufRead, Read};
    let mut line = Vec::new();
    // (Two more than a step's length: its line end, and one byte that says it is too long.)
    let limit = MAX_STEP_LINE as u64 + 2;
    if input.by_ref().take(limit).read_until(b'\n', &mut line)? == 0 {
        return Ok(None);
    }
    if line.last() != Some(&b'\n') && line.len() as u64 == limit {
        // The rest of a line that is too long is read past, a piece at a time.
        loop {
            let mut rest = Vec::new();
            let n = input.by_ref().take(8192).read_until(b'\n', &mut rest)?;
            if n == 0 || rest.last() == Some(&b'\n') {
                break;
            }
        }
        return Ok(Some(Err("a line longer than a step is: TICKS, or TICKS WORD (10 up+shoot, 1 enter, 30 none)")));
    }
    Ok(Some(String::from_utf8(line).map_err(|_| "a line that is not text (UTF-8): TICKS, or TICKS WORD (10 up+shoot, 1 enter, 30 none)")))
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

    /// A run prints the report's lines, and any key asked for by itself that is no line.
    #[test]
    fn a_key_is_read_without_being_a_line_of_the_report() {
        let game = GameFile::parse(r#"{"format": "specula.game.v1", "title": "g", "modules": ["modules/gate"], "report": {"tick": "run.ticks", "ball": "world./gate/ball.position", "score": "gate.score"}}"#).unwrap();
        let names = |keys: &[&str], also: &[&str]| -> Vec<String> {
            let own = |list: &[&str]| list.iter().map(|k| (*k).to_owned()).collect::<Vec<String>>();
            lines_of(&game, &own(keys), &own(also)).into_iter().map(|(name, key, own)| format!("{name}={key}{}", if own { "" } else { "*" })).collect()
        };
        assert_eq!(names(&[], &[]), ["tick=run.ticks", "ball=world./gate/ball.position", "score=gate.score"]);
        assert_eq!(names(&["score"], &[]), ["score=gate.score"]);
        // A key by itself, alone or beside lines; a part of a line is the line's, not a key.
        assert_eq!(names(&["run.sounds_heard"], &[]), ["run.sounds_heard=run.sounds_heard*"]);
        assert_eq!(names(&["score", "game.phase"], &["ball.x", "gate.wave", "game.phase", "score"]), ["score=gate.score", "game.phase=game.phase*", "gate.wave=gate.wave*"]);
        // What a condition reads is printed beside every line when no keys are named.
        assert_eq!(names(&[], &["game.phase"]).len(), 4);
        // A word with no dot is a name, and one the report has not is still refused (where the run begins).
        assert!(names(&["lives"], &[]).is_empty());
    }

    #[test]
    fn a_spec_is_read_back_as_it_was_written() {
        let spec = Spec { ticks: 600, seed: Some(7), inputs: vec![(0, "enter".into()), (5, "up".into())], frame: Some(PathBuf::from("out/f.png")), size: (800, 480), follow: true, at: vec![90, 180], keys: vec!["ball".into()], also: vec!["game.phase".into()], set: vec![("grip".into(), 0.25)], find: None, out: PathBuf::from("answer.json") };
        assert_eq!(Spec::from_json(&spec.to_json()).unwrap(), spec);
        let bare = Spec { ticks: 0, seed: None, inputs: Vec::new(), frame: None, size: (960, 540), follow: false, at: Vec::new(), keys: Vec::new(), also: Vec::new(), set: Vec::new(), find: None, out: PathBuf::from("a") };
        assert_eq!(Spec::from_json(&Json::parse(r#"{"ticks": 0, "out": "a"}"#).unwrap()).unwrap(), bare);
        let bad = |text: &str| Spec::from_json(&Json::parse(text).unwrap()).unwrap_err();
        assert!(bad(r#"{"ticks": 2000000, "out": "a"}"#).contains("ticks"));
        assert!(bad(r#"{"ticks": 1, "out": "a", "size": [10, 480]}"#).contains("size"));
        assert!(bad(r#"{"ticks": 1}"#).contains("out"));
        assert!(bad(r#"{"ticks": 9, "out": "a", "at": [5, 5]}"#).contains("at:"));
        assert!(bad(r#"{"ticks": 9, "out": "a", "at": [5, 10]}"#).contains("later than the run"));
        assert!(bad(r#"{"ticks": 9, "out": "a", "keys": ["a b"]}"#).contains("keys"));
        assert!(bad(r#"{"ticks": 1, "out": "a", "inputs": [[5, "up"], [1, "down"]]}"#).contains("order"));
    }

    #[test]
    fn a_condition_is_comparisons_over_the_reports_lines() {
        let report = Json::parse(r#"{"screen": "won", "score": 30, "over": true, "ball": {"x": 1.5, "y": -6}, "a.b": 2}"#).unwrap();
        let holds = |text: &str| Until::parse(text).unwrap().holds(&report);
        assert!(holds("screen==won") && holds("score>=30") && holds("score == 30") && holds("over==true") && holds("ball.y<-5") && holds("a.b==2"));
        assert!(holds("screen==won&&score>=30 && ball.x>1") && !holds("screen==won&&score>30"));
        assert!(!holds("screen==lost") && holds("screen!=lost") && !holds("score<30") && holds("score<=30") && !holds("ball.x<=1.4"));
        // A line the report has not holds nothing but "is not".
        assert!(!holds("lives>=1") && !holds("lives==3") && holds("lives!=3"));
        assert_eq!(Until::parse("lives>=1&&ball.z<0").unwrap().unread(&report), ["lives", "ball.z"]);
        assert_eq!(Until::parse(" screen == won && score>=30").unwrap().text(), "screen==won&&score>=30");
        for bad in ["", "screen", "screen=won", "==won", "screen==", "screen>won", "a==1&&"] {
            assert!(Until::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_games_own_words_stand_for_inputs_part_by_part() {
        let words = vec![("menu_down".to_owned(), "Key.Down".to_owned()), ("fire".to_owned(), "up+shoot".to_owned())];
        assert_eq!(in_game_words("menu_down", &words), "Key.Down");
        assert_eq!(in_game_words("fire+left", &words), "up+shoot+left");
        assert_eq!(in_game_words("down", &words), "down", "a word of the engine's is itself");
        assert_eq!(in_game_words("throttle=0.5", &[]), "throttle=0.5");
        // What a word stands for is given as it is: a press of the interface alone here.
        assert_eq!(parts(&in_game_words("menu_down", &words)).unwrap().iter().map(|(name, _)| press_of(name)).collect::<Vec<_>>(), [Some("Key.Down")]);
    }

    #[test]
    fn a_step_is_ticks_and_perhaps_a_word() {
        assert_eq!(parse_step("10 up+shoot").unwrap(), (10, Some("up+shoot")));
        assert_eq!(parse_step("  30  ").unwrap(), (30, None));
        assert_eq!(parse_step("0 enter").unwrap(), (0, Some("enter")));
        assert_eq!(parse_step("5 throttle=0.5").unwrap(), (5, Some("throttle=0.5")));
        for bad in ["", "up", "up 10", "10 up shoot", "-1 up", "1.5 up", "2000000 up", "10:up"] {
            assert!(parse_step(bad).unwrap_err().contains("TICKS WORD"), "{bad}");
        }
        assert!(parse_step(&format!("1 {}", "a".repeat(65))).unwrap_err().contains("1 to 64"));
        assert!(parse_step(&"9".repeat(300)).unwrap_err().contains("not a step"));
    }

    #[test]
    fn a_step_line_is_bounded_and_says_what_it_could_not_be() {
        let long = "7".repeat(5000);
        let text = format!("10 up\r\n\n{long}\n3\n\u{e9}t\u{e9}\n4");
        let mut bytes = text.into_bytes();
        // (One line that is not text: a lone byte of a longer character.)
        bytes.extend_from_slice(b"\n\xE9\n5\n");
        let mut input = std::io::Cursor::new(bytes);
        let mut next = || step_line(&mut input).unwrap();
        assert_eq!(next(), Some(Ok("10 up\r\n".to_owned())));
        assert_eq!(next(), Some(Ok("\n".to_owned())));
        assert!(next().unwrap().unwrap_err().contains("longer than a step"), "the long line is one refusal, and nothing of it is the next line");
        assert_eq!(next(), Some(Ok("3\n".to_owned())));
        assert_eq!(next(), Some(Ok("\u{e9}t\u{e9}\n".to_owned())));
        assert_eq!(next(), Some(Ok("4\n".to_owned())));
        assert!(next().unwrap().unwrap_err().contains("not text"));
        assert_eq!(next(), Some(Ok("5\n".to_owned())));
        assert_eq!(next(), None);
        // A last line with no line end, at exactly the limit, is a line.
        let mut input = std::io::Cursor::new("9".repeat(MAX_STEP_LINE).into_bytes());
        assert_eq!(step_line(&mut input).unwrap(), Some(Ok("9".repeat(MAX_STEP_LINE))));
    }

    #[test]
    fn modules_lines_are_one_log_by_tick() {
        let line = |module: &str, tick: u64| Json::obj([("module", Json::from(module)), ("tick", Json::from(tick))]);
        let mut log: Vec<Json> = (0..15).map(|t| line("a", t * 2)).chain((0..15).map(|t| line("b", t * 2 + 1))).collect();
        log_in_order(&mut log);
        assert_eq!(log.len(), MAX_LOG);
        let ticks: Vec<u64> = log.iter().filter_map(|l| l.get("tick").and_then(Json::as_u64)).collect();
        assert_eq!(ticks, (10..30).collect::<Vec<u64>>(), "the last twenty of both, not the last module's");
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
        let beyond = j(r#"{"world./gate/door.position": {"x": 1, "y": 2, "z": 3}, "run.sounds": ["coin", "bounce"]}"#);
        let v = |key: &str| value_of(key, 120, Some(4), &game, &ui, &states, &beyond).to_string();
        assert_eq!((v("world./gate/door.position"), v("run.sounds"), v("world./gate/none.x")), (r#"{"x":1,"y":2,"z":3}"#.into(), r#"["coin","bounce"]"#.into(), "null".into()));
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

    /// A text with a key left in it is said once a key, with the text; words in braces that
    /// are no key, and texts that were filled, are not.
    #[test]
    fn a_screen_text_nothing_fills_is_said() {
        let ui = Json::parse(r#"{"all_texts": ["Taps: {marble.taps}", "Time: {marble.left}", "Taps: {marble.taps}", "Score: 40", "Press {A} to start", "{} and {a.} and {.b}"]}"#).unwrap();
        let said = unfilled_texts(&ui);
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said[0].contains("\"Taps: {marble.taps}\"") && said[0].contains("nothing keeps \"marble.taps\"") && said[0].contains("cx.set_state(\"taps\""), "{}", said[0]);
        assert!(said[1].contains("marble.left"), "{}", said[1]);
        // Bounded: the keys past the most said are counted in one line.
        let many = Json::obj([("all_texts", Json::arr((0..20).map(|i| Json::from(format!("{{m.k{i}}}")))))]);
        let said = unfilled_texts(&many);
        assert_eq!(said.len(), MAX_SAID + 1);
        assert!(said[MAX_SAID].contains("14 more"), "{}", said[MAX_SAID]);
        assert!(unfilled_texts(&Json::Null).is_empty());
    }
}
