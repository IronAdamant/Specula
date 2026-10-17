//! Designer targets as data: what to measure, what it should be, how close is close enough,
//! who plays (the policy) and on which seeds.
//!
//! ```json
//! { "balance": 1, "name": "slice_feel",
//!   "ticks": "15m",
//!   "seeds": { "train": 16, "holdout": 16, "base": 1 },
//!   "policy": { "every": "1s", "wait_on": ["not_enough_dust"], "steps": [
//!       { "command": "place", "args": { "stone": "warding_1", "site": "top:8,14" } },
//!       { "command": "upgrade", "args": { "stone": "top:8,14" }, "after_wave": 2 } ] },
//!   "targets": [
//!       { "id": "wins", "metric": "win_rate", "target": 0.6, "tolerance": 0.15 },
//!       { "metric": "leaks", "wave": 3, "target": 2, "tolerance": 1 },
//!       { "metric": "resource", "resource": "dust", "at": "90s", "target": 120, "tolerance": 25 },
//!       { "metric": "time_to_kill", "archetype": "shambler", "target": 6, "tolerance": 1.5 } ] }
//! ```
//!
//! [`Targets::parse`] checks the document's shape; [`Targets::bind`] checks it against a
//! compiled rule set (resources, archetypes, tags, waves, commands, expressions exist; durations
//! become ticks at the rules' rate). Both report every problem as a Lex [`Diagnostic`] (`item` is
//! the target's id, or `policy` / `seeds` / `""` for the header) and never stop at the first.

use specula_core::Json;
use specula_lex::diag::{codes, did_you_mean};
use specula_lex::expr::Ty;
use specula_lex::run::{compile_query, Query};
use specula_lex::units::{dec_from_f64, parse_dec, scale, Dec, Unit};
use specula_lex::{CommandCall, Diagnostic, Rules};

/// The targets document format version.
pub const FORMAT_VERSION: u64 = 1;
pub const MAX_TARGETS: usize = 64;
pub const MAX_STEPS: usize = 512;
pub const MAX_SEEDS: usize = 1024;
/// Longest run (two hours at 30 Hz), as `lex_simulate`.
pub const MAX_TICKS: u64 = 216_000;
/// A targets document is small data.
pub const MAX_DOC_BYTES: usize = 256 * 1024;

/// A time: seconds (converted at the rules' rate when bound) or whole ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dur {
    Seconds(Dec),
    Ticks(u64),
}

impl Dur {
    pub fn ticks(self, hz: u32) -> u64 {
        match self {
            Dur::Ticks(t) => t,
            Dur::Seconds(d) => scale(d, i128::from(hz), 1).map_or(u64::MAX, |s| s.value.max(0) as u64),
        }
    }
}

/// What a target measures, per run (then averaged over seeds).
#[derive(Clone, Debug, PartialEq)]
pub enum Metric {
    /// 1 when the run ended with `result` (default `won`), else 0: the mean is the win rate.
    Outcome { result: String },
    /// 1 when wave `wave` (1-based) was cleared, else 0: win rate per wave.
    WaveCleared { wave: u16 },
    /// Wave entities removed without dying (leaks), in wave `wave` or in all waves.
    Leaks { wave: Option<u16> },
    /// Wave entities that died, in wave `wave` or in all.
    Kills { wave: Option<u16> },
    /// A resource's value (in its human unit) at `at` (or at the end): one point of a curve.
    Resource { resource: String, at: Option<Dur> },
    /// Mean seconds from spawning to dying for entities of an archetype (or with a tag); runs
    /// where none died don't count.
    TimeToKill { archetype: Option<String>, tag: Option<String> },
    /// Seconds from wave `wave`'s start to its clearing; runs where it wasn't cleared don't count.
    WaveClearTime { wave: u16 },
    /// Seconds until the outcome; runs without one don't count.
    EndTime,
    /// A Lex global expression at the end (numbers in their human unit, true = 1).
    Expr { src: String },
}

impl Metric {
    pub fn name(&self) -> &'static str {
        match self {
            Metric::Outcome { result } if result == "won" => "win_rate",
            Metric::Outcome { .. } => "outcome_rate",
            Metric::WaveCleared { .. } => "wave_cleared",
            Metric::Leaks { .. } => "leaks",
            Metric::Kills { .. } => "kills",
            Metric::Resource { .. } => "resource",
            Metric::TimeToKill { .. } => "time_to_kill",
            Metric::WaveClearTime { .. } => "wave_clear_time",
            Metric::EndTime => "end_time",
            Metric::Expr { .. } => "expr",
        }
    }

    /// The unit the value, target and tolerance are in, for reports.
    pub fn unit(&self) -> &'static str {
        match self {
            Metric::Outcome { .. } | Metric::WaveCleared { .. } => "fraction of runs (0..1)",
            Metric::Leaks { .. } | Metric::Kills { .. } => "entities per run",
            Metric::Resource { .. } => "the resource's unit",
            Metric::TimeToKill { .. } | Metric::WaveClearTime { .. } | Metric::EndTime => "seconds",
            Metric::Expr { .. } => "the expression's human unit",
        }
    }
}

/// The metric catalogue, for tools (`balance_targets` with no document).
pub fn catalogue() -> Json {
    let m = |name: &str, fields: &str, what: &str| Json::obj([("metric", Json::from(name)), ("fields", Json::from(fields)), ("measures", Json::from(what))]);
    Json::arr([
        m("win_rate", "", "fraction of runs won (outcome 'won')"),
        m("outcome_rate", "result", "fraction of runs that ended with this outcome result"),
        m("wave_cleared", "wave (1-based)", "fraction of runs that cleared this wave (win rate per wave)"),
        m("leaks", "wave? (1-based; all waves when left out)", "wave entities removed without dying (reached the goal), per run"),
        m("kills", "wave?", "wave entities that died, per run"),
        m("resource", "resource, at? (a time like \"90s\"; the end when left out)", "the resource's value then: targets at several times make a curve"),
        m("time_to_kill", "archetype? | tag?", "mean seconds from spawn to death"),
        m("wave_clear_time", "wave", "seconds from the wave's start until it was cleared"),
        m("end_time", "", "seconds until the run's outcome"),
        m("expr", "expr (a Lex expression without self, e.g. \"res.tombstones\")", "its value at the end of the run"),
    ])
}

/// One target: a metric, the value it should have, and how far off is still on target.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: String,
    pub metric: Metric,
    pub target: f64,
    pub tolerance: f64,
    /// Its share of the loss (default 1).
    pub weight: f64,
}

/// One scripted command of the policy.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub command: String,
    pub args: Json,
    /// Not before this time.
    pub at: Option<Dur>,
    /// Not before this wave has started (1-based; `waves.current >= after_wave`).
    pub after_wave: Option<u16>,
    /// Not before this Lex condition holds (e.g. `"res.dust >= 120"`).
    pub when: Option<String>,
}

/// Who plays: a build order. Every `every`, steps are tried in order: a due step's command is
/// applied; refused with a code in `wait_on`, it waits for the next try; refused otherwise, it
/// is skipped (and counted). A step that isn't due yet stops the try.
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub every: Dur,
    pub wait_on: Vec<String>,
    pub steps: Vec<Step>,
}

/// A parsed targets document.
#[derive(Clone, Debug, PartialEq)]
pub struct Targets {
    pub name: String,
    pub ticks: Option<Dur>,
    pub policy: Policy,
    pub train: Vec<u64>,
    pub holdout: Vec<u64>,
    pub targets: Vec<Target>,
}

/// Collects diagnostics while reading.
struct Reader {
    diags: Vec<Diagnostic>,
}

impl Reader {
    fn err(&mut self, code: &'static str, item: &str, path: &str, msg: impl Into<String>) -> &mut Diagnostic {
        self.diags.push(Diagnostic::error(code, item, path, msg));
        self.diags.last_mut().expect("pushed")
    }

    fn keys(&mut self, o: &Json, item: &str, path: &str, allowed: &[&str]) {
        for (k, _) in o.as_object().unwrap_or_default() {
            if !allowed.contains(&k.as_str()) && k != "notes" {
                let s = did_you_mean(k, allowed.iter().copied());
                let d = self.err(codes::UNKNOWN_FIELD, item, &join(path, k), format!("unknown field '{k}'"));
                if let Some(s) = s {
                    d.suggestion = Some(s);
                }
            }
        }
    }

    fn num(&mut self, o: &Json, key: &str, item: &str, path: &str) -> Option<f64> {
        match o.get(key) {
            None => None,
            Some(Json::Num(v)) if v.is_finite() => Some(*v),
            Some(v) => {
                let d = self.err(codes::WRONG_TYPE, item, &join(path, key), format!("'{key}' must be a number"));
                d.got = Some(v.to_string());
                None
            }
        }
    }

    fn string(&mut self, o: &Json, key: &str, item: &str, path: &str) -> Option<String> {
        match o.get(key) {
            None => None,
            Some(Json::Str(s)) if !s.is_empty() && s.len() <= 1000 => Some(s.clone()),
            Some(v) => {
                let d = self.err(codes::WRONG_TYPE, item, &join(path, key), format!("'{key}' must be a string of 1..1000 characters"));
                d.got = Some(v.to_string());
                None
            }
        }
    }

    fn wave(&mut self, o: &Json, item: &str, path: &str) -> Option<u16> {
        match o.get("wave") {
            None => None,
            Some(v) => match v.as_u64().filter(|w| (1..=10_000).contains(w)) {
                Some(w) => Some(w as u16),
                None => {
                    let d = self.err(codes::OUT_OF_RANGE, item, &join(path, "wave"), "'wave' is a wave number, 1-based (1 is the first wave)");
                    d.got = Some(v.to_string());
                    None
                }
            },
        }
    }

    fn dur(&mut self, o: &Json, key: &str, item: &str, path: &str) -> Option<Dur> {
        let v = o.get(key)?;
        let parsed = match v {
            Json::Num(n) if *n >= 0.0 => dec_from_f64(*n).map(Dur::Seconds),
            Json::Str(s) => {
                if let Some(t) = s.strip_suffix('t') {
                    t.parse::<u64>().ok().map(Dur::Ticks)
                } else if let Some(m) = s.strip_suffix('m') {
                    parse_dec(m).filter(|d| d.mant >= 0).map(|d| Dur::Seconds(Dec { mant: d.mant * 60, scale: d.scale }))
                } else {
                    s.strip_suffix('s').and_then(parse_dec).filter(|d| d.mant >= 0).map(Dur::Seconds)
                }
            }
            _ => None,
        };
        if parsed.is_none() {
            let d = self.err(codes::WRONG_TYPE, item, &join(path, key), format!("'{key}' must be a time: seconds as a number, or \"90s\", \"2.5m\", \"900t\""));
            d.got = Some(v.to_string());
        }
        parsed
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn seeds(r: &mut Reader, v: Option<&Json>, base: u64, key: &str, default: u64) -> Vec<u64> {
    match v {
        None => (0..default).map(|i| base + i).collect(),
        Some(Json::Num(_)) => match v.and_then(Json::as_u64).filter(|n| (1..=MAX_SEEDS as u64).contains(n)) {
            Some(n) => (0..n).map(|i| base + i).collect(),
            None => {
                r.err(codes::OUT_OF_RANGE, "seeds", key, format!("'{key}' is a count of seeds, 1..{MAX_SEEDS}, or a list of seeds"));
                Vec::new()
            }
        },
        Some(Json::Arr(a)) if !a.is_empty() && a.len() <= MAX_SEEDS => {
            let list: Option<Vec<u64>> = a.iter().map(Json::as_u64).collect();
            list.unwrap_or_else(|| {
                r.err(codes::WRONG_TYPE, "seeds", key, "seeds are whole numbers (below 2^53)");
                Vec::new()
            })
        }
        Some(_) => {
            r.err(codes::WRONG_TYPE, "seeds", key, format!("'{key}' is a count of seeds or a list of 1..{MAX_SEEDS} seeds"));
            Vec::new()
        }
    }
}

impl Targets {
    /// Parses a targets document (text). Every problem is reported.
    pub fn parse_text(text: &str) -> Result<Targets, Vec<Diagnostic>> {
        if text.len() > MAX_DOC_BYTES {
            return Err(vec![Diagnostic::error(codes::TOO_LARGE, "", "", format!("a targets document is at most {MAX_DOC_BYTES} bytes"))]);
        }
        match Json::parse(text) {
            Ok(j) => Targets::parse(&j),
            Err(e) => Err(vec![Diagnostic::error(codes::SYNTAX, "", "", format!("not JSON: {}", e.message)).got(format!("byte {}", e.offset))]),
        }
    }

    /// Parses a targets document. Every problem is reported.
    pub fn parse(doc: &Json) -> Result<Targets, Vec<Diagnostic>> {
        let mut r = Reader { diags: Vec::new() };
        if doc.as_object().is_none() {
            return Err(vec![Diagnostic::error(codes::WRONG_TYPE, "", "", "a targets document is a JSON object")]);
        }
        r.keys(doc, "", "", &["balance", "name", "ticks", "seeds", "policy", "targets"]);
        match doc.get("balance").and_then(Json::as_u64) {
            Some(FORMAT_VERSION) => {}
            Some(v) => {
                r.err(codes::UNSUPPORTED_VERSION, "", "balance", format!("targets format version {v}; this build reads {FORMAT_VERSION}"));
            }
            None => {
                r.err(codes::MISSING_FIELD, "", "balance", "'balance' (the format version) is required").suggestion = Some("add \"balance\": 1".into());
            }
        }
        let name = r.string(doc, "name", "", "").unwrap_or_else(|| "targets".into());
        let ticks = r.dur(doc, "ticks", "", "");
        // Seeds.
        let (mut train, mut holdout) = (Vec::new(), Vec::new());
        match doc.get("seeds") {
            None => {
                train = (1..=16).collect();
                holdout = (10_001..=10_016).collect();
            }
            Some(s @ Json::Obj(_)) => {
                r.keys(s, "seeds", "", &["train", "holdout", "base"]);
                let base = s.get("base").and_then(Json::as_u64).unwrap_or(1);
                train = seeds(&mut r, s.get("train"), base, "train", 16);
                holdout = seeds(&mut r, s.get("holdout"), base + 10_000, "holdout", 16);
                if let Some(x) = train.iter().find(|x| holdout.contains(x)) {
                    r.err(codes::INVALID, "seeds", "holdout", format!("seed {x} is in both train and holdout: held-out seeds must be ones tuning never saw"));
                }
            }
            Some(_) => {
                r.err(codes::WRONG_TYPE, "seeds", "", "'seeds' is {\"train\": N or [seeds], \"holdout\": N or [seeds], \"base\": first seed}");
            }
        }
        // The policy.
        let mut policy = Policy { every: Dur::Seconds(Dec::int(1)), wait_on: vec!["not_enough_dust".into()], steps: Vec::new() };
        match doc.get("policy") {
            None => {}
            Some(p @ Json::Obj(_)) => {
                r.keys(p, "policy", "", &["every", "wait_on", "steps"]);
                if let Some(e) = r.dur(p, "every", "policy", "") {
                    policy.every = e;
                }
                if let Some(w) = p.get("wait_on") {
                    match w.as_array().and_then(|a| a.iter().map(|c| c.as_str().map(str::to_owned)).collect::<Option<Vec<_>>>()) {
                        Some(list) => policy.wait_on = list,
                        None => {
                            r.err(codes::WRONG_TYPE, "policy", "wait_on", "'wait_on' lists rejection codes (strings) a step waits on, e.g. [\"not_enough_dust\"]");
                        }
                    }
                }
                let steps = p.get("steps").and_then(Json::as_array).unwrap_or_default();
                if steps.len() > MAX_STEPS {
                    r.err(codes::TOO_MANY, "policy", "steps", format!("at most {MAX_STEPS} steps"));
                }
                for (i, s) in steps.iter().take(MAX_STEPS).enumerate() {
                    let path = format!("steps[{i}]");
                    if s.as_object().is_none() {
                        r.err(codes::WRONG_TYPE, "policy", &path, "a step is {\"command\": id, \"args\": {...}, \"at\"?, \"after_wave\"?, \"when\"?}");
                        continue;
                    }
                    r.keys(s, "policy", &path, &["command", "args", "at", "after_wave", "when"]);
                    let Some(command) = r.string(s, "command", "policy", &path) else {
                        if s.get("command").is_none() {
                            r.err(codes::MISSING_FIELD, "policy", &join(&path, "command"), "'command' (a command id) is required");
                        }
                        continue;
                    };
                    let after_wave = match s.get("after_wave") {
                        None => None,
                        Some(v) => match v.as_u64().filter(|w| *w <= 10_000) {
                            Some(w) => Some(w as u16),
                            None => {
                                r.err(codes::OUT_OF_RANGE, "policy", &join(&path, "after_wave"), "'after_wave' is a wave number (1 = once the first wave has started)");
                                None
                            }
                        },
                    };
                    policy.steps.push(Step {
                        command,
                        args: s.get("args").cloned().unwrap_or(Json::Null),
                        at: r.dur(s, "at", "policy", &path),
                        after_wave,
                        when: r.string(s, "when", "policy", &path),
                    });
                }
            }
            Some(_) => {
                r.err(codes::WRONG_TYPE, "policy", "", "'policy' is {\"every\", \"wait_on\", \"steps\": [...]}");
            }
        }
        // Targets.
        let list = doc.get("targets").and_then(Json::as_array).unwrap_or_default();
        if list.is_empty() {
            r.err(codes::MISSING_FIELD, "", "targets", "'targets' must list at least one target").suggestion =
                Some("e.g. {\"metric\": \"win_rate\", \"target\": 0.6, \"tolerance\": 0.1}".into());
        }
        if list.len() > MAX_TARGETS {
            r.err(codes::TOO_MANY, "", "targets", format!("at most {MAX_TARGETS} targets"));
        }
        let mut targets: Vec<Target> = Vec::new();
        for (i, t) in list.iter().take(MAX_TARGETS).enumerate() {
            let path = format!("targets[{i}]");
            if t.as_object().is_none() {
                r.err(codes::WRONG_TYPE, "", &path, "a target is {\"metric\", ..., \"target\", \"tolerance\"}");
                continue;
            }
            let id = t.get("id").and_then(Json::as_str).map(str::to_owned).unwrap_or_else(|| format!("t{i}"));
            let item = id.as_str();
            if targets.iter().any(|x| x.id == id) {
                r.err(codes::DUPLICATE, item, "id", format!("two targets are called '{id}'"));
            }
            let Some(metric_name) = r.string(t, "metric", item, "") else {
                if t.get("metric").is_none() {
                    r.err(codes::MISSING_FIELD, item, "metric", "'metric' is required").suggestion = Some("one of: win_rate, outcome_rate, wave_cleared, leaks, kills, resource, time_to_kill, wave_clear_time, end_time, expr".into());
                }
                continue;
            };
            let base = ["id", "metric", "target", "tolerance", "weight"];
            let mut allowed: Vec<&str> = base.to_vec();
            let metric = match metric_name.as_str() {
                "win_rate" => Some(Metric::Outcome { result: "won".into() }),
                "outcome_rate" => {
                    allowed.push("result");
                    let result = r.string(t, "result", item, "");
                    if result.is_none() {
                        r.err(codes::MISSING_FIELD, item, "result", "'outcome_rate' needs 'result' (an outcome's result name, e.g. \"lost\")");
                    }
                    result.map(|result| Metric::Outcome { result })
                }
                "wave_cleared" | "wave_clear_time" => {
                    allowed.push("wave");
                    let w = r.wave(t, item, "");
                    if w.is_none() && t.get("wave").is_none() {
                        r.err(codes::MISSING_FIELD, item, "wave", format!("'{metric_name}' needs 'wave' (1-based)"));
                    }
                    w.map(|wave| if metric_name == "wave_cleared" { Metric::WaveCleared { wave } } else { Metric::WaveClearTime { wave } })
                }
                "leaks" | "kills" => {
                    allowed.push("wave");
                    let wave = r.wave(t, item, "");
                    Some(if metric_name == "leaks" { Metric::Leaks { wave } } else { Metric::Kills { wave } })
                }
                "resource" => {
                    allowed.extend(["resource", "at"]);
                    let res = r.string(t, "resource", item, "");
                    if res.is_none() && t.get("resource").is_none() {
                        r.err(codes::MISSING_FIELD, item, "resource", "'resource' needs 'resource' (a resource id, e.g. \"dust\")");
                    }
                    let at = r.dur(t, "at", item, "");
                    res.map(|resource| Metric::Resource { resource, at })
                }
                "time_to_kill" => {
                    allowed.extend(["archetype", "tag"]);
                    let (archetype, tag) = (r.string(t, "archetype", item, ""), r.string(t, "tag", item, ""));
                    if archetype.is_some() && tag.is_some() {
                        r.err(codes::INVALID, item, "tag", "give 'archetype' or 'tag', not both");
                    }
                    Some(Metric::TimeToKill { archetype, tag })
                }
                "end_time" => Some(Metric::EndTime),
                "expr" => {
                    allowed.push("expr");
                    let src = r.string(t, "expr", item, "");
                    if src.is_none() && t.get("expr").is_none() {
                        r.err(codes::MISSING_FIELD, item, "expr", "'expr' needs 'expr' (a Lex expression, e.g. \"res.tombstones\")");
                    }
                    src.map(|src| Metric::Expr { src })
                }
                other => {
                    let s = did_you_mean(other, ["win_rate", "outcome_rate", "wave_cleared", "leaks", "kills", "resource", "time_to_kill", "wave_clear_time", "end_time", "expr"]);
                    let d = r.err(codes::UNKNOWN_REF, item, "metric", format!("unknown metric '{other}'"));
                    d.suggestion = s;
                    None
                }
            };
            r.keys(t, item, "", &allowed);
            let target = r.num(t, "target", item, "");
            if target.is_none() && t.get("target").is_none() {
                r.err(codes::MISSING_FIELD, item, "target", "'target' (the value it should have) is required");
            }
            let tolerance = r.num(t, "tolerance", item, "");
            match tolerance {
                None if t.get("tolerance").is_none() => {
                    r.err(codes::MISSING_FIELD, item, "tolerance", "'tolerance' (how far off is still on target) is required").suggestion = Some("e.g. 10% of the target".into());
                }
                Some(v) if v <= 0.0 => {
                    r.err(codes::OUT_OF_RANGE, item, "tolerance", "'tolerance' must be more than 0");
                }
                _ => {}
            }
            let weight = r.num(t, "weight", item, "").unwrap_or(1.0);
            if !(0.0..=1000.0).contains(&weight) {
                r.err(codes::OUT_OF_RANGE, item, "weight", "'weight' is 0..1000");
            }
            if let (Some(metric), Some(target), Some(tolerance)) = (metric, target, tolerance) {
                if let Metric::Outcome { .. } | Metric::WaveCleared { .. } = metric {
                    if !(0.0..=1.0).contains(&target) {
                        r.err(codes::OUT_OF_RANGE, item, "target", format!("'{metric_name}' is a fraction of runs: the target is 0..1")).suggestion = Some(format!("write {}", target / 100.0));
                    }
                }
                targets.push(Target { id, metric, target, tolerance: tolerance.max(f64::MIN_POSITIVE), weight });
            }
        }
        if r.diags.iter().any(Diagnostic::is_error) {
            return Err(r.diags);
        }
        Ok(Targets { name, ticks, policy, train, holdout, targets })
    }

    /// Checks the document against a compiled rule set and resolves names, commands and times.
    pub fn bind(&self, rules: &Rules) -> Result<Bound, Vec<Diagnostic>> {
        let mut diags = Vec::new();
        let hz = rules.tick_hz;
        let ticks = self.ticks.map_or(MAX_TICKS, |d| d.ticks(hz));
        if ticks == 0 || ticks > MAX_TICKS {
            diags.push(Diagnostic::error(codes::OUT_OF_RANGE, "", "ticks", format!("'ticks' is 1..{MAX_TICKS} ticks ({} at {hz} Hz)", MAX_TICKS / u64::from(hz))));
        }
        let waves = rules.waves.as_ref().map_or(0, |w| w.waves.len());
        let mut bound = Vec::with_capacity(self.targets.len());
        for t in &self.targets {
            let item = t.id.as_str();
            let wave_ok = |w: u16, diags: &mut Vec<Diagnostic>| {
                if usize::from(w) > waves {
                    diags.push(Diagnostic::error(codes::OUT_OF_RANGE, item, "wave", format!("wave {w}, but the rules have {waves} waves")));
                }
            };
            let b = match &t.metric {
                Metric::Outcome { result } => {
                    if !rules.outcomes.iter().any(|o| &o.result == result) {
                        let names: Vec<&str> = rules.outcomes.iter().map(|o| o.result.as_str()).collect();
                        diags.push(Diagnostic::error(codes::UNKNOWN_REF, item, "result", format!("no outcome has the result '{result}'")).suggest_opt(did_you_mean(result, names)));
                    }
                    BoundMetric::Outcome(result.clone())
                }
                Metric::WaveCleared { wave } => {
                    wave_ok(*wave, &mut diags);
                    BoundMetric::WaveCleared(*wave)
                }
                Metric::WaveClearTime { wave } => {
                    wave_ok(*wave, &mut diags);
                    BoundMetric::WaveClearTime(*wave)
                }
                Metric::Leaks { wave } | Metric::Kills { wave } => {
                    if let Some(w) = wave {
                        wave_ok(*w, &mut diags);
                    }
                    if waves == 0 {
                        diags.push(Diagnostic::error(codes::INVALID, item, "metric", "the rules have no waves: leaks and kills count wave entities"));
                    }
                    if matches!(t.metric, Metric::Leaks { .. }) {
                        BoundMetric::Leaks(*wave)
                    } else {
                        BoundMetric::Kills(*wave)
                    }
                }
                Metric::Resource { resource, at } => match rules.resource(resource) {
                    Some(i) => {
                        let at = at.map(|d| d.ticks(hz));
                        if at.is_some_and(|a| a > ticks) {
                            diags.push(Diagnostic::warning(codes::OUT_OF_RANGE, item, "at", "this time is after the runs end ('ticks'): the value at the end is used"));
                        }
                        BoundMetric::Resource { index: i, at, unit: rules.resources[usize::from(i)].unit }
                    }
                    None => {
                        let names: Vec<&str> = rules.resources.iter().map(|r| r.id.as_str()).collect();
                        diags.push(Diagnostic::error(codes::UNKNOWN_REF, item, "resource", format!("no resource '{resource}'")).suggest_opt(did_you_mean(resource, names)));
                        BoundMetric::EndTime
                    }
                },
                Metric::TimeToKill { archetype, tag } => {
                    let arch = archetype.as_ref().map(|a| match rules.archetype(a) {
                        Some(i) => Some(i),
                        None => {
                            let names: Vec<&str> = rules.archetypes.iter().map(|x| x.id.as_str()).collect();
                            diags.push(Diagnostic::error(codes::UNKNOWN_REF, item, "archetype", format!("no archetype '{a}'")).suggest_opt(did_you_mean(a, names)));
                            None
                        }
                    });
                    let tag = tag.as_ref().map(|g| match rules.tag(g) {
                        Some(i) => Some(i),
                        None => {
                            diags.push(Diagnostic::error(codes::UNKNOWN_REF, item, "tag", format!("no archetype has the tag '{g}'")).suggest_opt(did_you_mean(g, rules.tags.iter().map(String::as_str))));
                            None
                        }
                    });
                    BoundMetric::TimeToKill { arch: arch.flatten(), tag: tag.flatten() }
                }
                Metric::EndTime => BoundMetric::EndTime,
                Metric::Expr { src } => match compile_query(rules, src) {
                    Ok(q) if matches!(q.ty, Ty::Num(_) | Ty::Bool) => BoundMetric::Expr(q),
                    Ok(q) => {
                        diags.push(Diagnostic::error(codes::WRONG_TYPE, item, "expr", format!("'{src}' is {}; a metric is a number or a condition", q.ty.describe())));
                        BoundMetric::EndTime
                    }
                    Err(mut d) => {
                        d.item = t.id.clone();
                        d.path = "expr".into();
                        diags.push(d);
                        BoundMetric::EndTime
                    }
                },
            };
            bound.push(b);
        }
        let mut steps = Vec::with_capacity(self.policy.steps.len());
        for (i, s) in self.policy.steps.iter().enumerate() {
            let path = format!("steps[{i}]");
            let call = match specula_lex::sim::parse_command(rules, &s.command, &s.args) {
                Ok(c) => Some(c),
                Err(e) => {
                    let mut d = Diagnostic::error(codes::INVALID, "policy", &path, format!("command '{}' doesn't take these args: {} ({})", s.command, e.message, e.code));
                    if e.code == "unknown_command" || !rules.commands.iter().any(|c| c.id == s.command) {
                        d = d.suggest_opt(did_you_mean(&s.command, rules.commands.iter().map(|c| c.id.as_str())));
                    }
                    diags.push(d);
                    None
                }
            };
            let when = match &s.when {
                None => None,
                Some(src) => match compile_query(rules, src) {
                    Ok(q) if q.ty == Ty::Bool => Some(q),
                    Ok(_) => {
                        diags.push(Diagnostic::error(codes::WRONG_TYPE, "policy", format!("{path}.when"), format!("'{src}' must be a condition (true or false)")));
                        None
                    }
                    Err(mut d) => {
                        d.item = "policy".into();
                        d.path = format!("{path}.when");
                        diags.push(d);
                        None
                    }
                },
            };
            if let Some(w) = s.after_wave {
                if usize::from(w) > waves {
                    diags.push(Diagnostic::error(codes::OUT_OF_RANGE, "policy", format!("{path}.after_wave"), format!("wave {w}, but the rules have {waves} waves")));
                }
            }
            if let Some(call) = call {
                steps.push(BoundStep { call, at: s.at.map(|d| d.ticks(hz)), after_wave: s.after_wave, when });
            }
        }
        let every = self.policy.every.ticks(hz).max(1);
        if diags.iter().any(Diagnostic::is_error) {
            return Err(diags);
        }
        Ok(Bound { ticks: ticks as u32, every: every.min(u64::from(u32::MAX)) as u32, wait_on: self.policy.wait_on.clone(), steps, metrics: bound, warnings: diags })
    }

    /// The canonical JSON form (what `balance_targets` shows back).
    pub fn to_json(&self) -> Json {
        let dur = |d: &Dur| match d {
            Dur::Ticks(t) => Json::from(format!("{t}t")),
            Dur::Seconds(s) => Json::from(format!("{}s", dec_str(*s))),
        };
        let targets = self.targets.iter().map(|t| {
            let mut f = vec![("id", Json::from(t.id.as_str())), ("metric", Json::from(t.metric.name()))];
            match &t.metric {
                Metric::Outcome { result } if result != "won" => f.push(("result", Json::from(result.as_str()))),
                Metric::WaveCleared { wave } | Metric::WaveClearTime { wave } => f.push(("wave", Json::from(u32::from(*wave)))),
                Metric::Leaks { wave: Some(w) } | Metric::Kills { wave: Some(w) } => f.push(("wave", Json::from(u32::from(*w)))),
                Metric::Resource { resource, at } => {
                    f.push(("resource", Json::from(resource.as_str())));
                    if let Some(a) = at {
                        f.push(("at", dur(a)));
                    }
                }
                Metric::TimeToKill { archetype, tag } => {
                    if let Some(a) = archetype {
                        f.push(("archetype", Json::from(a.as_str())));
                    }
                    if let Some(g) = tag {
                        f.push(("tag", Json::from(g.as_str())));
                    }
                }
                Metric::Expr { src } => f.push(("expr", Json::from(src.as_str()))),
                _ => {}
            }
            f.extend([("target", Json::from(t.target)), ("tolerance", Json::from(t.tolerance)), ("weight", Json::from(t.weight)), ("unit", Json::from(t.metric.unit()))]);
            Json::obj(f)
        });
        let steps = self.policy.steps.iter().map(|s| {
            let mut f = vec![("command", Json::from(s.command.as_str())), ("args", s.args.clone())];
            if let Some(a) = &s.at {
                f.push(("at", dur(a)));
            }
            if let Some(w) = s.after_wave {
                f.push(("after_wave", Json::from(u32::from(w))));
            }
            if let Some(w) = &s.when {
                f.push(("when", Json::from(w.as_str())));
            }
            Json::obj(f)
        });
        let seeds = |v: &[u64]| Json::arr(v.iter().map(|s| Json::from(*s)));
        let mut out = vec![("balance", Json::from(FORMAT_VERSION)), ("name", Json::from(self.name.as_str()))];
        if let Some(t) = &self.ticks {
            out.push(("ticks", dur(t)));
        }
        out.push(("seeds", Json::obj([("train", seeds(&self.train)), ("holdout", seeds(&self.holdout))])));
        out.push((
            "policy",
            Json::obj([("every", dur(&self.policy.every)), ("wait_on", Json::arr(self.policy.wait_on.iter().map(|c| Json::from(c.as_str())))), ("steps", Json::arr(steps))]),
        ));
        out.push(("targets", Json::arr(targets)));
        Json::obj(out)
    }
}

fn dec_str(d: Dec) -> String {
    if d.scale == 0 {
        return d.mant.to_string();
    }
    let p = 10i128.pow(d.scale);
    let (neg, a) = (d.mant < 0, d.mant.abs());
    let frac = format!("{:0width$}", a % p, width = d.scale as usize);
    format!("{}{}.{}", if neg { "-" } else { "" }, a / p, frac.trim_end_matches('0'))
}

/// A metric resolved against rules.
#[derive(Clone, Debug)]
pub enum BoundMetric {
    Outcome(String),
    WaveCleared(u16),
    WaveClearTime(u16),
    Leaks(Option<u16>),
    Kills(Option<u16>),
    Resource { index: u16, at: Option<u64>, unit: Unit },
    TimeToKill { arch: Option<u16>, tag: Option<u16> },
    EndTime,
    Expr(Query),
}

/// A policy step resolved against rules.
#[derive(Clone, Debug)]
pub struct BoundStep {
    pub call: CommandCall,
    pub at: Option<u64>,
    pub after_wave: Option<u16>,
    pub when: Option<Query>,
}

/// Targets resolved against one compiled rule set (see [`Targets::bind`]).
#[derive(Clone, Debug)]
pub struct Bound {
    pub ticks: u32,
    pub every: u32,
    pub wait_on: Vec<String>,
    pub steps: Vec<BoundStep>,
    pub metrics: Vec<BoundMetric>,
    pub warnings: Vec<Diagnostic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> Result<Targets, Vec<Diagnostic>> {
        Targets::parse_text(s)
    }

    #[test]
    fn a_good_document_parses_and_round_trips() {
        let t = doc(r#"{"balance": 1, "name": "x", "ticks": "10m", "seeds": {"train": 4, "holdout": [90, 91]},
            "policy": {"every": "0.5s", "steps": [{"command": "place", "args": {"a": 1}, "at": 30, "after_wave": 1, "when": "res.dust >= 50"}]},
            "targets": [{"metric": "win_rate", "target": 0.6, "tolerance": 0.1},
                        {"id": "l3", "metric": "leaks", "wave": 3, "target": 2, "tolerance": 1, "weight": 2},
                        {"metric": "resource", "resource": "dust", "at": "90s", "target": 100, "tolerance": 20}]}"#)
        .unwrap();
        assert_eq!(t.train, vec![1, 2, 3, 4]);
        assert_eq!(t.holdout, vec![90, 91]);
        assert_eq!(t.ticks.unwrap().ticks(30), 18_000);
        assert_eq!(t.policy.every.ticks(30), 15);
        assert_eq!(t.targets[1].metric, Metric::Leaks { wave: Some(3) });
        let again = Targets::parse(&t.to_json()).unwrap();
        assert_eq!(again, t);
    }

    #[test]
    fn every_problem_is_reported_with_a_suggestion() {
        let e = doc(r#"{"balance": 1, "seeds": {"train": [1, 2], "holdout": [2]},
            "policy": {"stepz": []},
            "targets": [{"metric": "win_rat", "target": 0.5, "tolerance": 0.1},
                        {"metric": "win_rate", "target": 60, "tolerance": 0},
                        {"metric": "leaks", "wave": 0, "target": 1, "tolerance": 1, "wavee": 2}]}"#)
        .unwrap_err();
        let codes: Vec<(&str, &str)> = e.iter().map(|d| (d.code, d.path.as_str())).collect();
        assert!(codes.contains(&("invalid", "holdout")), "{codes:?}");
        assert!(codes.contains(&("unknown_field", "stepz")), "{codes:?}");
        assert!(codes.contains(&("unknown_ref", "metric")), "{codes:?}");
        assert!(codes.contains(&("out_of_range", "tolerance")), "{codes:?}");
        assert!(codes.contains(&("out_of_range", "wave")), "{codes:?}");
        assert!(codes.contains(&("unknown_field", "wavee")), "{codes:?}");
        let typo = e.iter().find(|d| d.code == "unknown_ref").unwrap();
        assert!(typo.suggestion.as_deref().unwrap_or("").contains("win_rate"), "{typo:?}");
        assert!(doc("[1]").is_err());
        assert!(doc("{").unwrap_err()[0].code == "syntax");
        assert_eq!(doc(r#"{"balance": 2, "targets": []}"#).unwrap_err()[0].code, "unsupported_version");
    }
}
