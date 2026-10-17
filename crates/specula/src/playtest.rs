//! Headless playtests as data: a spec of tool calls over time, assertions on their answers,
//! captures, and a replay file that verifies by hash.
//!
//! A **spec** is JSON. `steps` call any engine or game tool at a tick (`at`, counted from the
//! playtest's start); each may `expect` values in the answer (a dotted path into its JSON, matched
//! by a literal or by `{eq, ne, lt, lte, gt, gte, in, exists, contains, near/tol}`) or expect the
//! call to fail (`expect_error`). `until` ends the run as soon as a condition holds (a call made
//! every `every` ticks), failing after `ticks`; without it the run lasts `ticks` (by default up to
//! the last step). `end` steps run once at the end (final assertions, captures). `sight_frame`
//! (any call whose answer has `image.path`) is recorded as a capture.
//!
//! ```json
//! {"name": "centre_seed7_wins", "game": "sepulcra",
//!  "steps": [{"at": 0, "call": "play_new", "args": {"seed": "7", "layout": "centre"}},
//!            {"at": 900, "call": "play_state", "expect": {"wave": {"gte": 1}}}],
//!  "until": {"call": "play_state", "expect": {"outcome": {"ne": "running"}}, "every": 30},
//!  "ticks": 40000,
//!  "end": [{"call": "play_state", "expect": {"outcome": "won", "tombstones": {"gte": 1}}}]}
//! ```
//!
//! The runner ([`Run`]) is a pure state machine: it says what to do next ([`Action`]: call a tool,
//! take a hash, advance N ticks, finish) and the engine does it (`playtest_engine`), so the
//! schedule is unit-tested without an engine. The clock only moves when the run says so, so the
//! same spec in a fresh engine makes the same calls at the same ticks (F2).
//!
//! **Hashes**: at the start, every `checkpoint_every` ticks and at the end, the run records the
//! world's state hash, the physics hash and the game's subsystem hashes (as the desync detector
//! sees them), combined into one. `record` writes them with the spec into a **replay file**;
//! running the replay checks every hash again and stops at the first that differs, naming the
//! parts that differ (then `desync_compare` finds the component). The start hash is compared but
//! not required to match: a spec that begins by resetting the game (`play_new`) may start from any
//! state.

use crate::journal::hex;
use specula_core::hash::StableHasher;
use specula_core::Json;
use specula_protocol::{Content, ToolOutput};
use std::path::{Component, Path, PathBuf};

pub const FORMAT: &str = "specula-playtest";
pub const VERSION: u64 = 1;
/// Largest spec or replay file read.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_STEPS: usize = 256;
pub const MAX_END_STEPS: usize = 64;
pub const MAX_EXPECTS: usize = 32;
/// Longest run: about 9 hours of game time at 30 Hz (a minute or so of a headless engine).
pub const MAX_TICKS: u64 = 1_000_000;
/// A run with `until` and no `ticks` gives up after this many.
pub const DEFAULT_UNTIL_TICKS: u64 = 36_000;
pub const DEFAULT_CHECKPOINT_EVERY: u64 = 300;
/// Hashes one run records at most (the interval grows to fit).
pub const MAX_CHECKPOINTS: u64 = 1024;
pub const DEFAULT_EVERY: u64 = 30;
pub const DEFAULT_WALL_S: u64 = 600;
pub const MAX_WALL_S: u64 = 3600;
const MAX_FAILURES: usize = 64;
const MAX_CAPTURES: usize = 64;
const MAX_RESULTS: usize = 512;
/// Values quoted in failures are cut to this many characters of JSON.
const MAX_VALUE_CHARS: usize = 240;
const MAX_NAME: usize = 64;
const MAX_TEXT: usize = 400;

/// Tools a playtest may not call, and why (the error says what to do instead).
pub const FORBIDDEN: [(&str, &str); 6] = [
    ("time_control", "the playtest owns the clock: schedule calls with 'at' ticks, and end the run with 'ticks' or 'until'"),
    ("playtest_run", "playtests don't nest: run them one after another"),
    ("playtest_report", "playtests don't nest"),
    ("agent_hello", "a playtest acts as the agent that started it: call agent_hello before playtest_run"),
    ("logic_build", "module builds run outside the engine thread: build before the playtest"),
    ("logic_reload", "module reloads run outside the engine thread: reload before the playtest"),
];

/// A spec (or replay) problem: which field, and what to change.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecError {
    pub field: String,
    pub message: String,
}

impl SpecError {
    fn new(field: impl Into<String>, message: impl Into<String>) -> SpecError {
        SpecError { field: field.into(), message: message.into() }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([("error", Json::from("bad_spec")), ("field", Json::from(self.field.as_str())), ("message", Json::from(self.message.as_str()))])
    }
}

/// One segment of an expectation's path.
#[derive(Clone, Debug, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
    /// `#`: the length of an array, object or string.
    Len,
}

#[derive(Clone, Debug, PartialEq)]
enum Op {
    Eq(Json),
    Ne(Json),
    Lt(f64),
    Lte(f64),
    Gt(f64),
    Gte(f64),
    In(Vec<Json>),
    Exists(bool),
    Contains(Json),
    Near(f64, f64),
}

/// `path: matcher` in a step's `expect`.
#[derive(Clone, Debug, PartialEq)]
pub struct Expect {
    path: String,
    segs: Vec<Seg>,
    ops: Vec<Op>,
    /// The matcher as written (quoted in failures).
    source: Json,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ErrorWant {
    Any,
    /// The error's `error` or `code` field.
    Code(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub at: u64,
    pub call: String,
    pub args: Json,
    pub expect: Vec<Expect>,
    pub expect_error: Option<ErrorWant>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Until {
    pub call: String,
    pub args: Json,
    pub expect: Vec<Expect>,
    pub every: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub name: String,
    pub description: Option<String>,
    /// The hosted game this spec is for (refused in an engine hosting another).
    pub game: Option<String>,
    pub steps: Vec<Step>,
    pub until: Option<Until>,
    pub ticks: u64,
    pub end: Vec<Step>,
    /// Ticks between recorded hashes (after fitting [`MAX_CHECKPOINTS`]).
    pub checkpoint_every: u64,
    pub stop_on_failure: bool,
    pub max_wall_s: u64,
    /// The spec as given, for the replay file.
    pub source: Json,
}

const SPEC_KEYS: [&str; 10] = ["name", "description", "game", "steps", "until", "ticks", "end", "checkpoint_every", "stop_on_failure", "max_wall_s"];
const STEP_KEYS: [&str; 6] = ["at", "call", "args", "expect", "expect_error", "label"];
const UNTIL_KEYS: [&str; 4] = ["call", "args", "expect", "every"];
const MATCH_KEYS: [&str; 11] = ["eq", "ne", "lt", "lte", "gt", "gte", "in", "exists", "contains", "near", "tol"];

fn keys(v: &Json, field: &str, allowed: &[&str]) -> Result<(), SpecError> {
    let pairs = v.as_object().ok_or_else(|| SpecError::new(field, format!("'{field}' must be an object")))?;
    for (k, _) in pairs {
        if !allowed.contains(&k.as_str()) {
            let at = if field.is_empty() { k.clone() } else { format!("{field}.{k}") };
            return Err(SpecError::new(at, format!("unknown key '{k}' (allowed: {})", allowed.join(", "))));
        }
    }
    Ok(())
}

fn uint(v: &Json, key: &str, field: &str, lo: u64, hi: u64) -> Result<Option<u64>, SpecError> {
    match v.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(x) => match x.as_u64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(Some(n)),
            _ => Err(SpecError::new(field, format!("'{key}' must be an integer {lo}..{hi}"))),
        },
    }
}

fn text(v: &Json, key: &str, field: &str, max: usize) -> Result<Option<String>, SpecError> {
    match v.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) if !s.trim().is_empty() && s.chars().count() <= max => Ok(Some(s.clone())),
        Some(_) => Err(SpecError::new(field, format!("'{key}' must be a string of 1..{max} characters"))),
    }
}

/// A playtest name: 1..64 of `A-Z a-z 0-9 _ . -` (it names files).
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)) {
        return Err(format!("'{name}' is not a playtest name: 1..{MAX_NAME} of A-Z, a-z, 0-9, '_', '.', '-'"));
    }
    Ok(())
}

fn call_name(v: &Json, field: &str) -> Result<String, SpecError> {
    let name = match v.get("call") {
        Some(Json::Str(s)) => s.clone(),
        _ => return Err(SpecError::new(field, "'call' is required: the tool to call, e.g. \"play_state\"")),
    };
    if name.is_empty() || name.len() > 64 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(SpecError::new(format!("{field}.call"), format!("'{name}' is not a tool name")));
    }
    if let Some((_, why)) = FORBIDDEN.iter().find(|(n, _)| *n == name) {
        return Err(SpecError::new(format!("{field}.call"), format!("a playtest can't call {name}: {why}")));
    }
    Ok(name)
}

fn call_args(v: &Json, field: &str) -> Result<Json, SpecError> {
    match v.get("args") {
        None | Some(Json::Null) => Ok(Json::obj::<&str>([])),
        Some(a @ Json::Obj(_)) => Ok(a.clone()),
        Some(_) => Err(SpecError::new(format!("{field}.args"), "'args' must be an object of the tool's arguments")),
    }
}

fn parse_path(path: &str) -> Result<Vec<Seg>, String> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    path.split('.')
        .map(|s| match s {
            "" => Err(format!("'{path}' has an empty segment (paths look like \"enemies.items.0.hp\")")),
            "#" => Ok(Seg::Len),
            s if s.bytes().all(|b| b.is_ascii_digit()) && s.len() <= 9 => Ok(Seg::Index(s.parse().unwrap_or(0))),
            s => Ok(Seg::Key(s.to_owned())),
        })
        .collect()
}

fn num(v: &Json, field: &str, op: &str) -> Result<f64, SpecError> {
    v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| SpecError::new(field, format!("'{op}' needs a number")))
}

fn parse_matcher(m: &Json, field: &str) -> Result<Vec<Op>, SpecError> {
    let Json::Obj(pairs) = m else { return Ok(vec![Op::Eq(m.clone())]) };
    if pairs.is_empty() {
        return Err(SpecError::new(field, "an empty matcher matches nothing: give a value, or {eq|ne|lt|lte|gt|gte|in|exists|contains|near: ...}"));
    }
    keys(m, field, &MATCH_KEYS).map_err(|e| SpecError::new(e.field, format!("{} (to compare a whole object use {{\"eq\": {{...}}}})", e.message)))?;
    let mut ops = Vec::new();
    for (k, v) in pairs {
        ops.push(match k.as_str() {
            "eq" => Op::Eq(v.clone()),
            "ne" => Op::Ne(v.clone()),
            "lt" => Op::Lt(num(v, field, k)?),
            "lte" => Op::Lte(num(v, field, k)?),
            "gt" => Op::Gt(num(v, field, k)?),
            "gte" => Op::Gte(num(v, field, k)?),
            "in" => Op::In(v.as_array().ok_or_else(|| SpecError::new(field, "'in' needs an array of values"))?.to_vec()),
            "exists" => Op::Exists(v.as_bool().ok_or_else(|| SpecError::new(field, "'exists' needs true or false"))?),
            "contains" => Op::Contains(v.clone()),
            "near" => {
                let tol = match m.get("tol") {
                    Some(t) => num(t, field, "tol")?.abs(),
                    None => 1e-6,
                };
                Op::Near(num(v, field, k)?, tol)
            }
            "tol" if m.get("near").is_some() => continue,
            _ => return Err(SpecError::new(field, "'tol' goes with 'near'")),
        });
    }
    Ok(ops)
}

fn parse_expects(v: &Json, field: &str) -> Result<Vec<Expect>, SpecError> {
    let pairs = match v.get("expect") {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Obj(p)) => p,
        Some(_) => return Err(SpecError::new(format!("{field}.expect"), "'expect' must be an object of path: matcher")),
    };
    if pairs.len() > MAX_EXPECTS {
        return Err(SpecError::new(format!("{field}.expect"), format!("at most {MAX_EXPECTS} expectations per step")));
    }
    pairs
        .iter()
        .map(|(path, m)| {
            let f = format!("{field}.expect.{path}");
            let segs = parse_path(path).map_err(|e| SpecError::new(&f, e))?;
            Ok(Expect { path: path.clone(), segs, ops: parse_matcher(m, &f)?, source: m.clone() })
        })
        .collect()
}

fn parse_step(v: &Json, field: &str, timed: bool) -> Result<Step, SpecError> {
    let allowed: &[&str] = if timed { &STEP_KEYS } else { &STEP_KEYS[1..] };
    keys(v, field, allowed)?;
    let at = if timed {
        uint(v, "at", field, 0, MAX_TICKS)?.ok_or_else(|| SpecError::new(format!("{field}.at"), "'at' is required: the tick (from the start) to call at"))?
    } else {
        0
    };
    let expect_error = match v.get("expect_error") {
        None | Some(Json::Null) | Some(Json::Bool(false)) => None,
        Some(Json::Bool(true)) => Some(ErrorWant::Any),
        Some(Json::Str(c)) if !c.is_empty() && c.len() <= 64 => Some(ErrorWant::Code(c.clone())),
        Some(_) => return Err(SpecError::new(format!("{field}.expect_error"), "'expect_error' must be true or an error code")),
    };
    Ok(Step {
        at,
        call: call_name(v, field)?,
        args: call_args(v, field)?,
        expect: parse_expects(v, field)?,
        expect_error,
        label: text(v, "label", field, MAX_NAME)?,
    })
}

impl Spec {
    pub fn parse(v: &Json) -> Result<Spec, SpecError> {
        keys(v, "", &SPEC_KEYS).map_err(|e| if e.field.is_empty() { SpecError::new("spec", "the spec must be a JSON object") } else { e })?;
        let name = text(v, "name", "name", MAX_NAME)?.ok_or_else(|| SpecError::new("name", "'name' is required (it names the report and files)"))?;
        check_name(&name).map_err(|e| SpecError::new("name", e))?;
        let list = |key: &str, max: usize, timed: bool| -> Result<Vec<Step>, SpecError> {
            match v.get(key) {
                None | Some(Json::Null) => Ok(Vec::new()),
                Some(Json::Arr(items)) if items.len() <= max => {
                    items.iter().enumerate().map(|(i, s)| parse_step(s, &format!("{key}[{i}]"), timed)).collect()
                }
                Some(_) => Err(SpecError::new(key, format!("'{key}' must be an array of at most {max} steps"))),
            }
        };
        let steps = list("steps", MAX_STEPS, true)?;
        for (i, w) in steps.windows(2).enumerate() {
            if w[1].at < w[0].at {
                return Err(SpecError::new(format!("steps[{}].at", i + 1), format!("steps must be in tick order ({} comes after {})", w[1].at, w[0].at)));
            }
        }
        let end = list("end", MAX_END_STEPS, false)?;
        let until = match v.get("until") {
            None | Some(Json::Null) => None,
            Some(u) => {
                keys(u, "until", &UNTIL_KEYS)?;
                let expect = parse_expects(u, "until")?;
                if expect.is_empty() {
                    return Err(SpecError::new("until.expect", "'until' needs an 'expect': the condition that ends the run"));
                }
                Some(Until {
                    call: call_name(u, "until")?,
                    args: call_args(u, "until")?,
                    expect,
                    every: uint(u, "every", "until.every", 1, 10_000)?.unwrap_or(DEFAULT_EVERY),
                })
            }
        };
        let last_at = steps.last().map_or(0, |s| s.at);
        let ticks = match uint(v, "ticks", "ticks", 0, MAX_TICKS)? {
            Some(t) => t,
            None if until.is_some() => DEFAULT_UNTIL_TICKS.max(last_at),
            None => last_at,
        };
        if last_at > ticks {
            return Err(SpecError::new("ticks", format!("the run ends at tick {ticks}, before steps at tick {last_at}")));
        }
        let every = uint(v, "checkpoint_every", "checkpoint_every", 1, MAX_TICKS)?.unwrap_or(DEFAULT_CHECKPOINT_EVERY);
        let stop_on_failure = match v.get("stop_on_failure") {
            None | Some(Json::Null) => false,
            Some(Json::Bool(b)) => *b,
            Some(_) => return Err(SpecError::new("stop_on_failure", "'stop_on_failure' must be true or false")),
        };
        Ok(Spec {
            name,
            description: text(v, "description", "description", MAX_TEXT)?,
            game: text(v, "game", "game", MAX_NAME)?,
            steps,
            until,
            ticks,
            end,
            checkpoint_every: every.max(ticks.div_ceil(MAX_CHECKPOINTS)).max(1),
            stop_on_failure,
            max_wall_s: uint(v, "max_wall_s", "max_wall_s", 1, MAX_WALL_S)?.unwrap_or(DEFAULT_WALL_S),
            source: v.clone(),
        })
    }

    /// Every tool the spec calls (to check them against the engine's before starting).
    pub fn calls(&self) -> impl Iterator<Item = &str> {
        self.steps.iter().map(|s| s.call.as_str()).chain(self.end.iter().map(|s| s.call.as_str())).chain(self.until.iter().map(|u| u.call.as_str()))
    }
}

/// `v` at `segs`, or `None` when the path leads nowhere.
fn lookup(v: &Json, segs: &[Seg]) -> Option<Json> {
    let mut cur = v;
    for (i, s) in segs.iter().enumerate() {
        match s {
            Seg::Len => {
                if i + 1 != segs.len() {
                    return None;
                }
                return match cur {
                    Json::Arr(a) => Some(Json::from(a.len())),
                    Json::Obj(o) => Some(Json::from(o.len())),
                    Json::Str(s) => Some(Json::from(s.chars().count())),
                    _ => None,
                };
            }
            Seg::Index(n) => match cur {
                Json::Arr(a) => cur = a.get(*n)?,
                Json::Obj(_) => cur = cur.get(&n.to_string())?,
                _ => return None,
            },
            Seg::Key(k) => cur = cur.get(k)?,
        }
    }
    Some(cur.clone())
}

/// Equality with numbers compared by value (JSON has one number type).
fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Num(x), Json::Num(y)) => x == y,
        _ => a == b,
    }
}

impl Expect {
    /// `Ok` when the answer matches; else the actual value (null when missing).
    fn check(&self, answer: &Json) -> Result<(), Json> {
        let got = lookup(answer, &self.segs);
        let ok = self.ops.iter().all(|op| match (op, &got) {
            (Op::Exists(want), g) => g.is_some() == *want,
            (_, None) => false,
            (Op::Eq(x), Some(g)) => same(g, x),
            (Op::Ne(x), Some(g)) => !same(g, x),
            (Op::Lt(x), Some(g)) => g.as_f64().is_some_and(|g| g < *x),
            (Op::Lte(x), Some(g)) => g.as_f64().is_some_and(|g| g <= *x),
            (Op::Gt(x), Some(g)) => g.as_f64().is_some_and(|g| g > *x),
            (Op::Gte(x), Some(g)) => g.as_f64().is_some_and(|g| g >= *x),
            (Op::Near(x, tol), Some(g)) => g.as_f64().is_some_and(|g| (g - x).abs() <= *tol),
            (Op::In(xs), Some(g)) => xs.iter().any(|x| same(g, x)),
            (Op::Contains(x), Some(Json::Str(s))) => x.as_str().is_some_and(|x| s.contains(x)),
            (Op::Contains(x), Some(Json::Arr(a))) => a.iter().any(|e| same(e, x)),
            (Op::Contains(x), Some(Json::Obj(o))) => x.as_str().is_some_and(|k| o.iter().any(|(ok, _)| ok == k)),
            (Op::Contains(_), Some(_)) => false,
        });
        if ok {
            Ok(())
        } else {
            Err(got.unwrap_or(Json::Null))
        }
    }
}

/// A value for a failure report, cut to [`MAX_VALUE_CHARS`] of JSON.
fn quote(v: &Json) -> Json {
    let s = v.to_string();
    if s.len() <= MAX_VALUE_CHARS {
        v.clone()
    } else {
        let mut cut = MAX_VALUE_CHARS;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        Json::from(format!("{}... ({} bytes)", &s[..cut], s.len()))
    }
}

/// The first text of a tool's answer, parsed as JSON when it is (else the text as a string).
pub fn answer_json(out: &ToolOutput) -> Json {
    let text = out.content.iter().find_map(|c| match c {
        Content::Text(t) => Some(t.as_str()),
        Content::Image { .. } => None,
    });
    match text {
        Some(t) => Json::parse(t).unwrap_or_else(|_| Json::from(t)),
        None => Json::Null,
    }
}

/// An error answer's code: its `error` field, or `code` (game rejections) when that is more
/// specific, else the text itself.
fn error_codes(answer: &Json) -> Vec<String> {
    let mut out = Vec::new();
    for k in ["error", "code"] {
        if let Some(c) = answer.get(k).and_then(Json::as_str) {
            out.push(c.to_owned());
        }
    }
    if let Some(cause) = answer.get("cause") {
        out.extend(error_codes(cause));
    }
    out
}

/// Why the runner asked for a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Step(usize),
    Until,
    End(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashPoint {
    Start,
    Tick,
    End,
}

impl HashPoint {
    fn name(self) -> &'static str {
        match self {
            HashPoint::Start => "start",
            HashPoint::Tick => "tick",
            HashPoint::End => "end",
        }
    }
}

/// What the engine does next for a [`Run`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Call a tool ([`Run::call_of`]) and hand the answer to [`Run::complete`].
    Call(Why),
    /// Hash the state ([`Run::record_hash`]).
    Hash(HashPoint),
    /// Run this many ticks (or fewer) and report them with [`Run::advanced`].
    Advance(u64),
    /// A call is still being answered.
    Wait,
    /// The run is over ([`Run::status`]).
    Finish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Passed,
    Failed,
    /// A replay's hash differed.
    Diverged,
    /// The run couldn't go on (wall-time limit, an engine problem).
    Error,
    Cancelled,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Passed => "passed",
            Status::Failed => "failed",
            Status::Diverged => "diverged",
            Status::Error => "error",
            Status::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Checkpoint {
    pub point: HashPoint,
    pub tick: u64,
    pub hash: u64,
    pub parts: Vec<(String, u64)>,
}

impl Checkpoint {
    pub fn new(point: HashPoint, tick: u64, parts: Vec<(String, u64)>) -> Checkpoint {
        let mut h = StableHasher::with_seed(0x706c_6179_7465_7374);
        for (name, v) in &parts {
            h.write_str(name);
            h.write_u64(*v);
        }
        Checkpoint { point, tick, hash: h.finish(), parts }
    }

    fn to_json(&self) -> Json {
        Json::obj([
            ("at", Json::from(self.point.name())),
            ("tick", Json::from(self.tick)),
            ("hash", Json::from(hex(self.hash))),
            ("parts", Json::Obj(self.parts.iter().map(|(k, v)| (k.clone(), Json::from(hex(*v)))).collect())),
        ])
    }

    fn from_json(v: &Json, field: &str) -> Result<Checkpoint, SpecError> {
        let bad = || SpecError::new(field, "a checkpoint is {at, tick, hash, parts}");
        let point = match v.get("at").and_then(Json::as_str) {
            Some("start") => HashPoint::Start,
            Some("tick") => HashPoint::Tick,
            Some("end") => HashPoint::End,
            _ => return Err(bad()),
        };
        let parse_hex = |j: Option<&Json>| j.and_then(Json::as_str).and_then(|s| u64::from_str_radix(s, 16).ok());
        let tick = v.get("tick").and_then(Json::as_u64).ok_or_else(bad)?;
        let hash = parse_hex(v.get("hash")).ok_or_else(bad)?;
        let parts = match v.get("parts") {
            Some(Json::Obj(p)) => p.iter().map(|(k, h)| parse_hex(Some(h)).map(|h| (k.clone(), h)).ok_or_else(bad)).collect::<Result<_, _>>()?,
            _ => return Err(bad()),
        };
        Ok(Checkpoint { point, tick, hash, parts })
    }
}

#[derive(Clone, Debug)]
struct Failure {
    why: Why,
    label: Option<String>,
    tick: u64,
    call: String,
    kind: &'static str,
    path: Option<String>,
    expected: Json,
    actual: Json,
}

#[derive(Clone, Debug)]
struct Capture {
    label: String,
    tick: u64,
    png: Json,
    packet: Json,
}

/// Where a part of the run stands, for reports.
fn why_json(why: Why) -> (Json, &'static str) {
    match why {
        Why::Step(i) => (Json::from(i), "steps"),
        Why::End(i) => (Json::from(i), "end"),
        Why::Until => (Json::Null, "until"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Main,
    End,
    Done,
}

/// The expected hashes when running a replay.
#[derive(Clone, Debug, Default)]
pub struct Expected {
    pub checkpoints: Vec<Checkpoint>,
    /// The original run's status.
    pub status: Option<String>,
    pub path: String,
}

/// One playtest's progress (see the module docs).
pub struct Run {
    pub spec: Spec,
    /// Ticks since the start.
    pub tick: u64,
    phase: Phase,
    step_i: usize,
    end_i: usize,
    pending: Option<Why>,
    started: bool,
    last_tick_hash: Option<u64>,
    end_hashed: bool,
    until_checked: Option<u64>,
    until_met: Option<u64>,
    checkpoints: Vec<Checkpoint>,
    expected: Option<Expected>,
    /// The first checkpoint that differed: (index, expected, got).
    divergence: Option<(usize, Option<Checkpoint>, Checkpoint)>,
    start_matches: Option<bool>,
    status: Option<Status>,
    /// Why a run ended in error or was cancelled.
    stop_reason: Option<String>,
    failures: Vec<Failure>,
    failures_total: usize,
    captures: Vec<Capture>,
    captures_total: usize,
    /// (why, tick, call, ok) per call, at most [`MAX_RESULTS`].
    results: Vec<(Why, u64, bool)>,
    calls: u64,
    steps_passed: u64,
    steps_failed: u64,
}

impl Run {
    pub fn new(spec: Spec, expected: Option<Expected>) -> Run {
        Run {
            spec,
            tick: 0,
            phase: Phase::Main,
            step_i: 0,
            end_i: 0,
            pending: None,
            started: false,
            last_tick_hash: None,
            end_hashed: false,
            until_checked: None,
            until_met: None,
            checkpoints: Vec::new(),
            expected,
            divergence: None,
            start_matches: None,
            status: None,
            stop_reason: None,
            failures: Vec::new(),
            failures_total: 0,
            captures: Vec::new(),
            captures_total: 0,
            results: Vec::new(),
            calls: 0,
            steps_passed: 0,
            steps_failed: 0,
        }
    }

    pub fn status(&self) -> Option<Status> {
        self.status
    }

    pub fn is_replay(&self) -> bool {
        self.expected.is_some()
    }

    /// The tool and arguments of a call [`Action::Call`] asked for.
    pub fn call_of(&self, why: Why) -> (&str, &Json) {
        match why {
            Why::Step(i) => (&self.spec.steps[i].call, &self.spec.steps[i].args),
            Why::End(i) => (&self.spec.end[i].call, &self.spec.end[i].args),
            Why::Until => {
                let u = self.spec.until.as_ref().expect("until calls only exist with until");
                (&u.call, &u.args)
            }
        }
    }

    /// The next thing to do. Call it again after doing it.
    pub fn next_action(&mut self) -> Action {
        loop {
            if self.status.is_some() || self.phase == Phase::Done {
                return Action::Finish;
            }
            if self.pending.is_some() {
                return Action::Wait;
            }
            match self.phase {
                Phase::Main => {
                    if !self.started {
                        return Action::Hash(HashPoint::Start);
                    }
                    if let Some(s) = self.spec.steps.get(self.step_i) {
                        if s.at == self.tick {
                            self.pending = Some(Why::Step(self.step_i));
                            return Action::Call(Why::Step(self.step_i));
                        }
                    }
                    let every = self.spec.checkpoint_every;
                    if self.tick > 0 && self.tick % every == 0 && self.last_tick_hash != Some(self.tick) {
                        return Action::Hash(HashPoint::Tick);
                    }
                    if let Some(u) = &self.spec.until {
                        if self.tick % u.every == 0 && self.until_checked != Some(self.tick) {
                            self.pending = Some(Why::Until);
                            return Action::Call(Why::Until);
                        }
                    }
                    if self.tick >= self.spec.ticks {
                        if let Some(u) = &self.spec.until {
                            let (call, expect) = (u.call.clone(), Json::Obj(u.expect.iter().map(|e| (e.path.clone(), e.source.clone())).collect()));
                            self.fail(Failure {
                                why: Why::Until,
                                label: None,
                                tick: self.tick,
                                call,
                                kind: "until_not_met",
                                path: None,
                                expected: expect,
                                actual: Json::from(format!("still not met after {} ticks", self.spec.ticks)),
                            });
                        }
                        if self.phase == Phase::Main {
                            self.phase = Phase::End;
                        }
                        continue;
                    }
                    let mut next = self.spec.ticks;
                    if let Some(s) = self.spec.steps.get(self.step_i) {
                        next = next.min(s.at);
                    }
                    next = next.min((self.tick / every + 1) * every);
                    if let Some(u) = &self.spec.until {
                        next = next.min((self.tick / u.every + 1) * u.every);
                    }
                    return Action::Advance(next - self.tick);
                }
                Phase::End => {
                    if self.end_i < self.spec.end.len() {
                        self.pending = Some(Why::End(self.end_i));
                        return Action::Call(Why::End(self.end_i));
                    }
                    if !self.end_hashed {
                        return Action::Hash(HashPoint::End);
                    }
                    self.status = Some(if self.failures_total == 0 { Status::Passed } else { Status::Failed });
                    self.phase = Phase::Done;
                }
                Phase::Done => return Action::Finish,
            }
        }
    }

    /// `n` ticks ran.
    pub fn advanced(&mut self, n: u64) {
        self.tick += n;
    }

    fn fail(&mut self, f: Failure) {
        self.failures_total += 1;
        if self.failures.len() < MAX_FAILURES {
            self.failures.push(f);
        }
        if self.spec.stop_on_failure && self.status.is_none() {
            self.status = Some(Status::Failed);
            self.phase = Phase::Done;
        }
    }

    /// The answer to the call [`Action::Call`] asked for.
    pub fn complete(&mut self, out: &ToolOutput) {
        let Some(why) = self.pending.take() else { return };
        self.calls += 1;
        let answer = answer_json(out);
        let tick = self.tick;
        if why == Why::Until {
            self.until_checked = Some(tick);
            let u = self.spec.until.as_ref().expect("until calls only exist with until");
            if out.is_error {
                let call = u.call.clone();
                self.fail(Failure { why, label: None, tick, call, kind: "call_error", path: None, expected: Json::Null, actual: quote(&answer) });
                self.status.get_or_insert(Status::Failed);
                self.phase = Phase::Done;
                return;
            }
            if u.expect.iter().all(|e| e.check(&answer).is_ok()) {
                self.until_met = Some(tick);
                self.phase = Phase::End;
            }
            return;
        }
        let step = match why {
            Why::Step(i) => {
                self.step_i = i + 1;
                &self.spec.steps[i]
            }
            Why::End(i) => {
                self.end_i = i + 1;
                &self.spec.end[i]
            }
            Why::Until => unreachable!(),
        };
        let mut problems = Vec::new();
        match (&step.expect_error, out.is_error) {
            (Some(want), false) => {
                let expected = match want {
                    ErrorWant::Any => Json::from("an error"),
                    ErrorWant::Code(c) => Json::from(c.as_str()),
                };
                problems.push(("expected_error", None, expected, quote(&answer)));
            }
            (Some(ErrorWant::Code(c)), true) if !error_codes(&answer).iter().any(|e| e == c) => {
                problems.push(("wrong_error", None, Json::from(c.as_str()), quote(&answer)));
            }
            (None, true) => problems.push(("call_error", None, Json::Null, quote(&answer))),
            _ => {}
        }
        if problems.is_empty() {
            for e in &step.expect {
                if let Err(actual) = e.check(&answer) {
                    problems.push(("expect", Some(e.path.clone()), e.source.clone(), quote(&actual)));
                }
            }
        }
        if !out.is_error {
            if let Some(image) = answer.get("image").filter(|i| i.get("path").is_some()) {
                self.captures_total += 1;
                if self.captures.len() < MAX_CAPTURES {
                    let label = step.label.clone().unwrap_or_else(|| format!("{}@{tick}", step.call));
                    let png = image.get("path").cloned().unwrap_or(Json::Null);
                    let packet = image.get("packet_path").cloned().unwrap_or(Json::Null);
                    self.captures.push(Capture { label, tick, png, packet });
                }
            }
        }
        let ok = problems.is_empty();
        if self.results.len() < MAX_RESULTS {
            self.results.push((why, tick, ok));
        }
        if ok {
            self.steps_passed += 1;
        } else {
            self.steps_failed += 1;
        }
        let (label, call) = (step.label.clone(), step.call.clone());
        for (kind, path, expected, actual) in problems {
            self.fail(Failure { why, label: label.clone(), tick, call: call.clone(), kind, path, expected, actual });
        }
    }

    /// The state's hashes at the point [`Action::Hash`] asked for.
    pub fn record_hash(&mut self, point: HashPoint, parts: Vec<(String, u64)>) {
        let cp = Checkpoint::new(point, self.tick, parts);
        match point {
            HashPoint::Start => self.started = true,
            HashPoint::Tick => self.last_tick_hash = Some(self.tick),
            HashPoint::End => self.end_hashed = true,
        }
        let index = self.checkpoints.len();
        if let Some(exp) = &self.expected {
            let want = exp.checkpoints.get(index);
            if point == HashPoint::Start {
                // A spec may begin by resetting the game: the start is compared, not required.
                self.start_matches = Some(want.is_some_and(|w| w.point == HashPoint::Start && w.hash == cp.hash));
            } else if want.is_none_or(|w| w.point != point || w.tick != cp.tick || w.hash != cp.hash) {
                self.divergence = Some((index, want.cloned(), cp.clone()));
                self.status = Some(Status::Diverged);
                self.phase = Phase::Done;
            }
        }
        if self.checkpoints.len() < (MAX_CHECKPOINTS as usize) + 2 {
            self.checkpoints.push(cp);
        }
    }

    /// Stops the run (cancelled, or an error such as the wall-time limit).
    pub fn stop(&mut self, status: Status, reason: impl Into<String>) {
        if self.status.is_none() {
            self.status = Some(status);
            self.stop_reason = Some(reason.into());
            self.phase = Phase::Done;
            self.pending = None;
        }
    }

    /// Where the run is, while it runs.
    pub fn progress(&self) -> Json {
        let phase = match self.phase {
            Phase::Main => "steps",
            Phase::End => "end",
            Phase::Done => "done",
        };
        Json::obj([
            ("tick", Json::from(self.tick)),
            ("of", Json::from(self.spec.ticks)),
            ("phase", Json::from(phase)),
            ("steps_done", Json::from(self.step_i)),
            ("steps", Json::from(self.spec.steps.len())),
            ("failures", Json::from(self.failures_total)),
        ])
    }

    fn failure_json(&self, f: &Failure) -> Json {
        let (index, part) = why_json(f.why);
        let mut j = Json::obj([("part", Json::from(part)), ("index", index), ("tick", Json::from(f.tick)), ("call", Json::from(f.call.as_str())), ("kind", Json::from(f.kind))]);
        if let Some(l) = &f.label {
            j.set("label", Json::from(l.as_str()));
        }
        if let Some(p) = &f.path {
            j.set("path", Json::from(p.as_str()));
        }
        if !f.expected.is_null() {
            j.set("expected", f.expected.clone());
        }
        j.set("actual", f.actual.clone());
        j
    }

    /// The report: `detailed` adds every call's result, all captures and the checkpoints.
    pub fn report(&self, detailed: bool) -> Json {
        let status = self.status.map_or("running", Status::name);
        let mut out = Json::obj([("name", Json::from(self.spec.name.as_str())), ("status", Json::from(status))]);
        if self.status.is_none() {
            out.set("progress", self.progress());
        }
        out.set("ticks", Json::from(self.tick));
        out.set(
            "steps",
            Json::obj([
                ("total", Json::from(self.spec.steps.len() + self.spec.end.len())),
                ("passed", Json::from(self.steps_passed)),
                ("failed", Json::from(self.steps_failed)),
                ("calls", Json::from(self.calls)),
            ]),
        );
        if self.spec.until.is_some() {
            out.set("until_met_at", self.until_met.map_or(Json::Null, Json::from));
        }
        if let Some(r) = &self.stop_reason {
            out.set("reason", Json::from(r.as_str()));
        }
        let shown = if detailed { MAX_FAILURES } else { 5 };
        out.set("failures_total", Json::from(self.failures_total));
        if self.failures_total > 0 {
            out.set("failures", Json::arr(self.failures.iter().take(shown).map(|f| self.failure_json(f))));
        }
        let shown = if detailed { MAX_CAPTURES } else { 8 };
        if self.captures_total > 0 {
            out.set("captures_total", Json::from(self.captures_total));
            out.set(
                "captures",
                Json::arr(self.captures.iter().take(shown).map(|c| {
                    let mut j = Json::obj([("label", Json::from(c.label.as_str())), ("tick", Json::from(c.tick)), ("png", c.png.clone())]);
                    if detailed {
                        j.set("packet", c.packet.clone());
                    }
                    j
                })),
            );
        }
        let mut hash = Json::obj([("checkpoints", Json::from(self.checkpoints.len())), ("every", Json::from(self.spec.checkpoint_every))]);
        if let Some(s) = self.checkpoints.first() {
            hash.set("start", Json::from(hex(s.hash)));
        }
        if let Some(e) = self.checkpoints.last().filter(|c| c.point == HashPoint::End) {
            hash.set("end", Json::from(hex(e.hash)));
            if detailed {
                hash.set("end_parts", e.to_json().get("parts").cloned().unwrap_or(Json::Null));
            }
        }
        out.set("hash", hash);
        if let Some(exp) = &self.expected {
            let mut r = Json::obj([
                ("of", Json::from(exp.path.as_str())),
                ("verified", Json::from(self.divergence.is_none() && self.end_hashed)),
                ("checked", Json::from(self.checkpoints.iter().filter(|c| c.point != HashPoint::Start).count())),
                ("expected", Json::from(exp.checkpoints.iter().filter(|c| c.point != HashPoint::Start).count())),
                ("start_matches", self.start_matches.map_or(Json::Null, Json::from)),
                ("original_status", exp.status.clone().map_or(Json::Null, Json::from)),
            ]);
            if let Some((index, want, got)) = &self.divergence {
                let differing: Vec<Json> = match want {
                    Some(w) => {
                        let names = w.parts.iter().map(|(k, _)| k).chain(got.parts.iter().map(|(k, _)| k));
                        let mut seen: Vec<&String> = Vec::new();
                        for k in names {
                            if !seen.contains(&k) {
                                seen.push(k);
                            }
                        }
                        let of = |c: &Checkpoint, k: &String| c.parts.iter().find(|(n, _)| n == k).map(|(_, h)| *h);
                        seen.into_iter().filter(|k| of(w, k) != of(got, k)).map(|k| Json::from(k.as_str())).collect()
                    }
                    None => Vec::new(),
                };
                r.set(
                    "first_divergence",
                    Json::obj([
                        ("checkpoint", Json::from(*index)),
                        ("expected", want.as_ref().map_or(Json::from("no more checkpoints"), Checkpoint::to_json)),
                        ("got", got.to_json()),
                        ("parts_differing", Json::Arr(differing)),
                        ("next", Json::from("desync_trace in both runs, then desync_compare, names the component and field")),
                    ]),
                );
            }
            out.set("replay", r);
        }
        if detailed {
            out.set(
                "results",
                Json::arr(self.results.iter().map(|(why, tick, ok)| {
                    let (index, part) = why_json(*why);
                    let (call, _) = self.call_of(*why);
                    Json::obj([("part", Json::from(part)), ("index", index), ("tick", Json::from(*tick)), ("call", Json::from(call)), ("ok", Json::from(*ok))])
                })),
            );
            let n = self.checkpoints.len();
            out.set("checkpoint_list", Json::arr(self.checkpoints.iter().enumerate().filter(|(i, _)| *i < 8 || *i + 8 >= n).map(|(_, c)| c.to_json())));
        }
        out
    }

    /// The replay file: the spec, every checkpoint and how the run ended.
    pub fn replay_json(&self, game: Option<&str>, tick_hz: u32) -> Json {
        Json::obj([
            ("format", Json::from(FORMAT)),
            ("version", Json::from(VERSION)),
            ("name", Json::from(self.spec.name.as_str())),
            ("game", game.map_or(Json::Null, Json::from)),
            ("tick_hz", Json::from(tick_hz)),
            ("status", Json::from(self.status.map_or("running", Status::name))),
            ("ticks", Json::from(self.tick)),
            ("spec", self.spec.source.clone()),
            ("checkpoints", Json::arr(self.checkpoints.iter().map(Checkpoint::to_json))),
        ])
    }
}

/// A replay file's spec and expected hashes; `tick_hz` must match the engine's.
pub fn parse_replay(text: &str, path: &str, tick_hz: u32) -> Result<(Spec, Expected), SpecError> {
    let v = Json::parse(text).map_err(|e| SpecError::new("replay", format!("{path} is not JSON: {e}")))?;
    if v.get("format").and_then(Json::as_str) != Some(FORMAT) {
        return Err(SpecError::new("format", format!("{path} is not a playtest replay (format must be \"{FORMAT}\"; record one with playtest_run {{record}})")));
    }
    if v.get("version").and_then(Json::as_u64) != Some(VERSION) {
        return Err(SpecError::new("version", format!("{path}: replay version {VERSION} is the only one this engine reads")));
    }
    let hz = v.get("tick_hz").and_then(Json::as_u64);
    if hz != Some(u64::from(tick_hz)) {
        return Err(SpecError::new("tick_hz", format!("{path} was recorded at {hz:?} ticks per second; this engine ticks at {tick_hz}")));
    }
    let spec = Spec::parse(v.get("spec").unwrap_or(&Json::Null)).map_err(|e| SpecError::new(format!("spec.{}", e.field), e.message))?;
    let list = v.get("checkpoints").and_then(Json::as_array).ok_or_else(|| SpecError::new("checkpoints", "the replay has no checkpoints"))?;
    if list.len() as u64 > MAX_CHECKPOINTS + 2 {
        return Err(SpecError::new("checkpoints", "too many checkpoints"));
    }
    let checkpoints = list.iter().enumerate().map(|(i, c)| Checkpoint::from_json(c, &format!("checkpoints[{i}]"))).collect::<Result<Vec<_>, _>>()?;
    let status = v.get("status").and_then(Json::as_str).map(str::to_owned);
    Ok((spec, Expected { checkpoints, status, path: path.to_owned() }))
}

/// Windows device names: `NUL.json` is the device NUL, not a file.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A file path for playtest files: relative to `root` (the engine's working directory), without
/// `..`, drives, `:` or device names, and still inside `root` once links are followed. `write`
/// creates the parent folders.
pub fn safe_path(root: &Path, rel: &str, write: bool) -> Result<PathBuf, String> {
    let p = Path::new(rel);
    if rel.is_empty() || rel.len() > 512 || !p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
        return Err(format!("'{rel}' must be a relative path inside the engine's working directory (no '..', drive or root)"));
    }
    if rel.contains(':') {
        return Err(format!("'{rel}': ':' is not allowed (drives and alternate data streams)"));
    }
    for c in p.components() {
        if let Component::Normal(name) = c {
            let name = name.to_string_lossy();
            let stem = name.split('.').next().unwrap_or("").trim_end_matches([' ', '.']).to_ascii_uppercase();
            if RESERVED.contains(&stem.as_str()) {
                return Err(format!("'{rel}': '{name}' is a Windows device name"));
            }
        }
    }
    let root = root.canonicalize().map_err(|e| format!("the working directory: {e}"))?;
    let full = root.join(p);
    let inside = |real: &Path| if real.starts_with(&root) { Ok(()) } else { Err(format!("'{rel}' resolves outside the working directory")) };
    if !write {
        let real = full.canonicalize().map_err(|e| format!("{rel}: {e}"))?;
        inside(&real)?;
        return Ok(real);
    }
    let parent = full.parent().unwrap_or(&root).to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|e| format!("{rel}: {e}"))?;
    let real_parent = parent.canonicalize().map_err(|e| format!("{rel}: {e}"))?;
    inside(&real_parent)?;
    let target = real_parent.join(full.file_name().unwrap_or_default());
    if target.exists() {
        inside(&target.canonicalize().map_err(|e| format!("{rel}: {e}"))?)?;
    }
    Ok(target)
}

/// Reads a spec or replay file (at most [`MAX_FILE_BYTES`]).
pub fn read_file(root: &Path, rel: &str) -> Result<(PathBuf, String), String> {
    let path = safe_path(root, rel, false)?;
    let len = std::fs::metadata(&path).map_err(|e| format!("{rel}: {e}"))?.len();
    if len > MAX_FILE_BYTES {
        return Err(format!("{rel} is {len} bytes; playtest files are at most {MAX_FILE_BYTES}"));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{rel}: {e}"))?;
    Ok((path, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> Spec {
        Spec::parse(&Json::parse(text).unwrap()).unwrap()
    }

    fn ok(text: &str) -> ToolOutput {
        ToolOutput::text(text)
    }

    /// Drives a run with canned answers: `answer(call, tick)`; hashes are the tick.
    fn drive(run: &mut Run, answer: impl Fn(&str, u64) -> ToolOutput) -> Vec<String> {
        let mut log = Vec::new();
        for _ in 0..100_000 {
            match run.next_action() {
                Action::Call(why) => {
                    let (call, _) = run.call_of(why);
                    log.push(format!("{}@{}", call, run.tick));
                    let out = answer(call, run.tick);
                    run.complete(&out);
                }
                Action::Hash(p) => {
                    log.push(format!("hash:{}@{}", p.name(), run.tick));
                    run.record_hash(p, vec![("world".into(), run.tick)]);
                }
                Action::Advance(n) => {
                    assert!(n > 0, "advances move time");
                    run.advanced(n);
                }
                Action::Wait => panic!("no deferred answers here"),
                Action::Finish => return log,
            }
        }
        panic!("the run never finished");
    }

    #[test]
    fn steps_run_at_their_ticks_with_checkpoints_between() {
        let mut run = Run::new(
            spec(r#"{"name": "t", "checkpoint_every": 100, "steps": [{"at": 0, "call": "a"}, {"at": 150, "call": "b"}, {"at": 150, "call": "c"}], "ticks": 250, "end": [{"call": "d"}]}"#),
            None,
        );
        let log = drive(&mut run, |_, _| ok("{}"));
        assert_eq!(log, ["hash:start@0", "a@0", "hash:tick@100", "b@150", "c@150", "hash:tick@200", "d@250", "hash:end@250"]);
        assert_eq!(run.status(), Some(Status::Passed));
        assert_eq!(run.tick, 250);
    }

    #[test]
    fn until_ends_the_run_and_fails_when_never_met() {
        let text = r#"{"name": "u", "until": {"call": "s", "every": 10, "expect": {"done": true}}, "ticks": 100, "checkpoint_every": 1000}"#;
        let mut run = Run::new(spec(text), None);
        drive(&mut run, |_, t| ok(&format!(r#"{{"done": {}}}"#, t >= 35)));
        assert_eq!((run.status(), run.tick, run.until_met), (Some(Status::Passed), 40, Some(40)));
        let mut never = Run::new(spec(text), None);
        drive(&mut never, |_, _| ok(r#"{"done": false}"#));
        assert_eq!((never.status(), never.tick), (Some(Status::Failed), 100));
        let r = never.report(false);
        assert_eq!(r.get("failures").unwrap().as_array().unwrap()[0].get("kind").unwrap().as_str(), Some("until_not_met"));
    }

    #[test]
    fn matchers() {
        let s = spec(
            r#"{"name": "m", "steps": [{"at": 0, "call": "x", "expect": {
                "a": 3, "b": {"gte": 2, "lt": 5}, "c.0.d": "hi", "c.#": 2, "e": {"in": ["x", "y"]},
                "f": {"exists": false}, "g": {"contains": "ell"}, "h": {"near": 1.0, "tol": 0.01}, "i": {"ne": null},
                "j": {"contains": 4}, "k": {"eq": {"z": 1}}}}]}"#,
        );
        let answer = r#"{"a": 3.0, "b": 4, "c": [{"d": "hi"}, 1], "e": "y", "g": "hello", "h": 1.005, "i": 0, "j": [3, 4], "k": {"z": 1}}"#;
        let mut run = Run::new(s.clone(), None);
        drive(&mut run, |_, _| ok(answer));
        assert_eq!(run.status(), Some(Status::Passed), "{}", run.report(true));
        let mut bad = Run::new(s, None);
        drive(&mut bad, |_, _| ok(r#"{"a": 2, "b": 5, "c": [], "e": "z", "f": 1, "g": "x", "h": 2, "i": null, "j": [], "k": {"z": 2}}"#));
        assert_eq!(bad.failures_total, 11, "{}", bad.report(true));
        let report = bad.report(false);
        let first = &report.get("failures").unwrap().as_array().unwrap()[0];
        assert_eq!(first.get("path").unwrap().as_str(), Some("a"));
        assert_eq!(first.get("actual").unwrap().as_f64(), Some(2.0));
    }

    #[test]
    fn errors_are_failures_unless_expected() {
        let s = spec(
            r#"{"name": "e", "steps": [{"at": 0, "call": "a"}, {"at": 0, "call": "b", "expect_error": "not_enough_dust"},
                {"at": 0, "call": "c", "expect_error": true}, {"at": 0, "call": "d", "expect_error": "occupied"}]}"#,
        );
        let mut run = Run::new(s, None);
        drive(&mut run, |call, _| match call {
            "a" => ToolOutput::error(r#"{"error": "invalid_argument", "message": "x"}"#),
            "b" | "d" => ToolOutput::error(r#"{"error": "rejected", "code": "not_enough_dust"}"#),
            _ => ok("{}"),
        });
        let kinds: Vec<_> = run.failures.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["call_error", "expected_error", "wrong_error"]);
        assert_eq!((run.steps_passed, run.steps_failed), (1, 3));
    }

    #[test]
    fn captures_are_listed() {
        let mut run = Run::new(spec(r#"{"name": "c", "steps": [{"at": 5, "call": "sight_frame", "label": "look"}]}"#), None);
        drive(&mut run, |_, _| ok(r#"{"image": {"path": "a.png", "packet_path": "a.json"}}"#));
        let r = run.report(true);
        let c = &r.get("captures").unwrap().as_array().unwrap()[0];
        assert_eq!((c.get("label").unwrap().as_str(), c.get("tick").unwrap().as_u64()), (Some("look"), Some(5)));
        assert_eq!(c.get("packet").unwrap().as_str(), Some("a.json"));
    }

    #[test]
    fn replays_verify_and_name_the_first_divergence() {
        let text = r#"{"name": "r", "checkpoint_every": 50, "steps": [{"at": 0, "call": "a"}], "ticks": 200}"#;
        let mut first = Run::new(spec(text), None);
        drive(&mut first, |_, _| ok("{}"));
        let file = first.replay_json(Some("g"), 30).to_pretty();
        let (s, exp) = parse_replay(&file, "r.json", 30).unwrap();
        assert_eq!(exp.checkpoints.len(), 6, "start, 50, 100, 150, 200, end");
        let mut again = Run::new(s.clone(), Some(exp.clone()));
        drive(&mut again, |_, _| ok("{}"));
        assert_eq!(again.status(), Some(Status::Passed));
        assert_eq!(again.report(false).get("replay").unwrap().get("verified").unwrap().as_bool(), Some(true));
        // A run whose state differs from tick 100 on.
        let mut off = Run::new(s, Some(exp));
        for _ in 0..1000 {
            match off.next_action() {
                Action::Call(_) => off.complete(&ok("{}")),
                Action::Hash(p) => {
                    let h = if off.tick >= 100 { off.tick + 1 } else { off.tick };
                    off.record_hash(p, vec![("world".into(), h)]);
                }
                Action::Advance(n) => off.advanced(n),
                Action::Wait => unreachable!(),
                Action::Finish => break,
            }
        }
        assert_eq!((off.status(), off.tick), (Some(Status::Diverged), 100));
        let r = off.report(false);
        let d = r.get("replay").unwrap().get("first_divergence").unwrap();
        assert_eq!(d.get("parts_differing").unwrap(), &Json::arr([Json::from("world")]), "{d}");
        assert!(parse_replay(&file, "r.json", 60).is_err(), "another tick rate");
    }

    #[test]
    fn specs_are_checked_with_fields_named() {
        let err = |t: &str| Spec::parse(&Json::parse(t).unwrap()).unwrap_err();
        assert_eq!(err(r#"{"steps": []}"#).field, "name");
        assert_eq!(err(r#"{"name": "a b"}"#).field, "name");
        assert_eq!(err(r#"{"name": "a", "stepz": []}"#).field, "stepz");
        assert_eq!(err(r#"{"name": "a", "steps": [{"at": 5, "call": "x"}, {"at": 1, "call": "y"}]}"#).field, "steps[1].at");
        assert_eq!(err(r#"{"name": "a", "steps": [{"at": 0, "call": "time_control"}]}"#).field, "steps[0].call");
        assert_eq!(err(r#"{"name": "a", "steps": [{"at": 0}]}"#).field, "steps[0]");
        assert_eq!(err(r#"{"name": "a", "steps": [{"at": 9, "call": "x"}], "ticks": 5}"#).field, "ticks");
        assert_eq!(err(r#"{"name": "a", "until": {"call": "x"}}"#).field, "until.expect");
        assert_eq!(err(r#"{"name": "a", "end": [{"at": 1, "call": "x"}]}"#).field, "end[0].at");
        assert_eq!(err(r#"{"name": "a", "steps": [{"at": 0, "call": "x", "expect": {"a": {"bogus": 1}}}]}"#).field, "steps[0].expect.a.bogus");
        assert!(err(r#"{"name": "a", "steps": [{"at": 0, "call": "x", "expect": {"a..b": 1}}]}"#).field.starts_with("steps[0].expect"));
        // Long runs record at most MAX_CHECKPOINTS hashes.
        let s = spec(r#"{"name": "a", "ticks": 1000000, "checkpoint_every": 1}"#);
        assert!(s.ticks / s.checkpoint_every <= MAX_CHECKPOINTS);
    }

    #[test]
    fn stop_on_failure_and_stop() {
        let mut run = Run::new(spec(r#"{"name": "s", "stop_on_failure": true, "steps": [{"at": 0, "call": "a", "expect": {"x": 1}}, {"at": 10, "call": "b"}]}"#), None);
        let log = drive(&mut run, |_, _| ok(r#"{"x": 2}"#));
        assert_eq!(log, ["hash:start@0", "a@0"]);
        assert_eq!(run.status(), Some(Status::Failed));
        let mut c = Run::new(spec(r#"{"name": "s", "ticks": 10}"#), None);
        c.stop(Status::Cancelled, "asked");
        assert_eq!(c.next_action(), Action::Finish);
        assert_eq!(c.report(false).get("reason").unwrap().as_str(), Some("asked"));
    }

    #[test]
    fn paths_stay_inside_the_root() {
        let root = std::env::temp_dir().join(format!("specula-playtest-paths-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(safe_path(&root, "a/b.json", true).is_ok());
        for bad in ["../x.json", "C:/x.json", "/x.json", "a/NUL.json", "a:b", ""] {
            assert!(safe_path(&root, bad, true).is_err(), "{bad}");
        }
        std::fs::write(root.join("s.json"), "{}").unwrap();
        assert_eq!(read_file(&root, "s.json").unwrap().1, "{}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
