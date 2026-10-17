//! Definitions: JSON parsed into a flat list of nodes, checked against a host's
//! [`Vocabulary`], every problem reported as a [`Diagnostic`] with its path.
//!
//! The format (every node is an object with one type key):
//!
//! | Node | JSON |
//! |---|---|
//! | sequence | `{"sequence": [nodes], "label"?, "reactive"?: true}`: children in order until one fails; reactive re-checks the conditions before the running child every tick |
//! | selector | `{"selector": [nodes], "reactive"?: true}`: the first child that doesn't fail; reactive re-checks earlier children every tick |
//! | parallel | `{"parallel": [nodes], "succeed"?: all, "fail"?: 1}`: all children each tick, until enough succeed or fail |
//! | decorators | `{"invert": node}`, `{"force_success": node}`, `{"force_failure": node}`, `{"repeat": node, "times"?}`, `{"retry": node, "times"?: 3}`, `{"cooldown": node, "ticks" or "s"}`, `{"timeout": node, "ticks" or "s"}` |
//! | wait | `{"wait": ticks}` or `{"wait_s": seconds}` |
//! | condition | `{"if": name, ...args}`; built in: `{"if": "bb", "key", "op": "==" \| "!=" \| "<" \| "<=" \| ">" \| ">=" \| "exists", "value"}` |
//! | action | `{"do": name, ...args}` |
//! | set | `{"set": key, "value": v}`: writes the blackboard |
//! | utility | `{"utility": [{"label", "weight"?: 1, "considerations": [...], "run": node}], "momentum"?: 0.1}` |
//!
//! A consideration is `{"input": name, ...args, "range"?: [lo, hi], "curve"?: "linear" |
//! "quadratic" | "inverse" | "step" | "power" | "logistic", "at"?, "exponent"?, "steepness"?,
//! "midpoint"?}`: the input mapped from `range` to 0..1, through the curve, clamped.
//!
//! A definition is `{"root": node, "label"?}` or a bare node.

use crate::{closest, Arg, ArgKind, Diagnostic, Verb, Vocabulary, MAX_CONSIDERATIONS, MAX_DEPTH, MAX_DIAGNOSTICS, MAX_NODES, MAX_OPTIONS};
use specula_core::dmath;
use specula_core::Json;

/// A duration: ticks, or seconds converted at the host's tick rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Time {
    Ticks(u32),
    Seconds(f32),
}

impl Time {
    pub fn ticks(self, hz: u32) -> u32 {
        match self {
            Time::Ticks(t) => t,
            Time::Seconds(s) => (dmath::round(s * hz as f32) as u32).max(1),
        }
    }
}

/// A response curve over 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Linear,
    Quadratic,
    Inverse,
    Step { at: f32 },
    Power { exponent: f32 },
    Logistic { steepness: f32, midpoint: f32 },
}

impl Curve {
    pub fn eval(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let y = match self {
            Curve::Linear => x,
            Curve::Quadratic => x * x,
            Curve::Inverse => 1.0 - x,
            Curve::Step { at } => {
                if x >= at {
                    1.0
                } else {
                    0.0
                }
            }
            Curve::Power { exponent } => dmath::pow(x, exponent),
            Curve::Logistic { steepness, midpoint } => 1.0 / (1.0 + dmath::exp(-steepness * (x - midpoint))),
        };
        if y.is_finite() {
            y.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// One consideration of a utility option.
#[derive(Clone, Debug, PartialEq)]
pub struct Consideration {
    pub input: String,
    pub args: Json,
    pub range: (f64, f64),
    pub curve: Curve,
}

impl Consideration {
    /// The input value mapped to 0..1 by `range` and the curve.
    pub fn score(&self, value: f64) -> f32 {
        let (lo, hi) = self.range;
        let x = if hi != lo { ((value - lo) / (hi - lo)) as f32 } else { 0.0 };
        self.curve.eval(if x.is_finite() { x } else { 0.0 })
    }
}

/// One option of a utility node (its node is the utility node's child at the same index).
#[derive(Clone, Debug, PartialEq)]
pub struct UtilOption {
    pub label: String,
    pub weight: f32,
    pub considerations: Vec<Consideration>,
}

/// Comparison of the built-in blackboard condition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Exists,
}

const OPS: [(&str, Op); 7] = [("==", Op::Eq), ("!=", Op::Ne), ("<", Op::Lt), ("<=", Op::Le), (">", Op::Gt), (">=", Op::Ge), ("exists", Op::Exists)];

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// `reactive`: conditions before the running child are checked again every tick.
    Sequence { reactive: bool },
    Selector { reactive: bool },
    /// Succeeds when `succeed` children have, fails when `fail` have.
    Parallel { succeed: usize, fail: usize },
    Invert,
    ForceSuccess,
    ForceFailure,
    Repeat { times: Option<u32> },
    Retry { times: u32 },
    Cooldown { time: Time },
    Timeout { time: Time },
    Wait { time: Time },
    Condition { name: String, args: Json },
    Compare { key: String, op: Op, value: Json },
    Action { name: String, args: Json },
    Set { key: String, value: Json },
    Utility { options: Vec<UtilOption>, momentum: f32 },
}

impl NodeKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Sequence { .. } => "sequence",
            NodeKind::Selector { .. } => "selector",
            NodeKind::Parallel { .. } => "parallel",
            NodeKind::Invert => "invert",
            NodeKind::ForceSuccess => "force_success",
            NodeKind::ForceFailure => "force_failure",
            NodeKind::Repeat { .. } => "repeat",
            NodeKind::Retry { .. } => "retry",
            NodeKind::Cooldown { .. } => "cooldown",
            NodeKind::Timeout { .. } => "timeout",
            NodeKind::Wait { .. } => "wait",
            NodeKind::Condition { .. } | NodeKind::Compare { .. } => "if",
            NodeKind::Action { .. } => "do",
            NodeKind::Set { .. } => "set",
            NodeKind::Utility { .. } => "utility",
        }
    }
}

/// One node: its kind, its children (indices into [`Def::nodes`]), a label for reports and its
/// path in the JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub children: Vec<usize>,
    pub label: String,
    pub path: String,
}

/// A validated definition (node 0 is the root).
#[derive(Clone, Debug, PartialEq)]
pub struct Def {
    pub label: String,
    pub nodes: Vec<Node>,
}

const TYPES: [&str; 17] = [
    "sequence", "selector", "parallel", "invert", "force_success", "force_failure", "repeat", "retry", "cooldown", "timeout", "wait", "wait_s", "if", "do", "set", "utility", "root",
];

struct Parser<'a> {
    vocab: &'a Vocabulary,
    nodes: Vec<Node>,
    diags: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn err(&mut self, path: &str, code: &'static str, message: impl Into<String>, hint: Option<String>) {
        if self.diags.len() < MAX_DIAGNOSTICS {
            self.diags.push(Diagnostic { path: path.to_owned(), code, message: message.into(), hint });
        }
    }

    fn keys(&mut self, v: &Json, path: &str, allowed: &[&str]) {
        if let Json::Obj(pairs) = v {
            for (k, _) in pairs {
                if !allowed.contains(&k.as_str()) {
                    let hint = closest(k, allowed.iter().copied()).map(|c| format!("did you mean '{c}'?"));
                    self.err(path, "unknown_key", format!("'{k}' is not a key of this node (allowed: {})", allowed.join(", ")), hint);
                }
            }
        }
    }

    fn time(&mut self, v: &Json, path: &str) -> Time {
        match (v.get("ticks"), v.get("s")) {
            (Some(t), None) => match t.as_u64() {
                Some(t) if (1..=1_000_000).contains(&t) => Time::Ticks(t as u32),
                _ => {
                    self.err(path, "invalid_value", "'ticks' must be an integer 1..1000000", None);
                    Time::Ticks(1)
                }
            },
            (None, Some(s)) => match s.as_f64() {
                Some(s) if s > 0.0 && s <= 100_000.0 => Time::Seconds(s as f32),
                _ => {
                    self.err(path, "invalid_value", "'s' must be seconds, 0..100000", None);
                    Time::Ticks(1)
                }
            },
            _ => {
                self.err(path, "missing_key", "give the duration as 'ticks' (an integer) or 's' (seconds), one of them", None);
                Time::Ticks(1)
            }
        }
    }

    fn label(v: &Json, fallback: String) -> String {
        v.get("label").and_then(Json::as_str).map(|s| s.chars().take(64).collect()).unwrap_or(fallback)
    }

    fn args(&mut self, v: &Json, path: &str, verb: &Verb, reserved: &[&str]) -> Json {
        let mut out: Vec<(String, Json)> = Vec::new();
        let names: Vec<&str> = verb.args.iter().map(|a| a.name).collect();
        if let Json::Obj(pairs) = v {
            for (k, val) in pairs {
                if reserved.contains(&k.as_str()) {
                    continue;
                }
                match verb.args.iter().find(|a| a.name == k) {
                    None => {
                        let hint = closest(k, names.iter().copied()).map(|c| format!("did you mean '{c}'?"));
                        let list = if names.is_empty() { "none".to_string() } else { names.join(", ") };
                        self.err(path, "unknown_argument", format!("'{}' takes no argument '{k}' (it takes: {list})", verb.name), hint);
                    }
                    Some(a) => {
                        if let Err(m) = check_arg(a, val) {
                            self.err(&format!("{path}.{k}"), "invalid_argument", format!("'{k}' must be {m}"), None);
                        }
                        out.push((k.clone(), val.clone()));
                    }
                }
            }
        }
        for a in &verb.args {
            if a.required && v.get(a.name).is_none_or(Json::is_null) {
                self.err(path, "missing_argument", format!("'{}' needs '{}': {}", verb.name, a.name, a.kind.describe()), None);
            }
        }
        Json::Obj(out)
    }

    fn verb<'v>(&mut self, list: &'v [Verb], kind: &str, name: &str, path: &str) -> Option<&'v Verb> {
        match list.iter().find(|v| v.name == name) {
            Some(v) => Some(v),
            None => {
                let mut names: Vec<&str> = list.iter().map(|v| v.name).collect();
                if kind == "condition" {
                    names.push("bb");
                }
                let hint = closest(name, names.iter().copied()).map(|c| format!("did you mean '{c}'?"));
                self.err(path, "unknown_name", format!("there is no {kind} '{name}' (known: {})", names.join(", ")), hint);
                None
            }
        }
    }

    fn children(&mut self, list: &Json, path: &str, key: &str, depth: usize) -> Vec<usize> {
        match list.as_array() {
            Some(items) if !items.is_empty() => items.iter().enumerate().filter_map(|(i, n)| self.node(n, &format!("{path}.{key}[{i}]"), depth + 1)).collect(),
            _ => {
                self.err(path, "invalid_value", format!("'{key}' must be a non-empty array of nodes"), None);
                Vec::new()
            }
        }
    }

    fn child(&mut self, n: &Json, path: &str, key: &str, depth: usize) -> Vec<usize> {
        self.node(n, &format!("{path}.{key}"), depth + 1).into_iter().collect()
    }

    /// Parses one node; returns its index (`None` when it was too broken to keep).
    fn node(&mut self, v: &Json, path: &str, depth: usize) -> Option<usize> {
        if depth > MAX_DEPTH {
            self.err(path, "budget", format!("nodes nest at most {MAX_DEPTH} deep"), None);
            return None;
        }
        if self.nodes.len() >= MAX_NODES {
            if self.nodes.len() == MAX_NODES {
                self.err(path, "budget", format!("a definition has at most {MAX_NODES} nodes"), None);
            }
            return None;
        }
        let Json::Obj(pairs) = v else {
            self.err(path, "invalid_value", "a node must be an object with one type key (sequence, selector, if, do, ...)", None);
            return None;
        };
        let types: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).filter(|k| TYPES.contains(k) && *k != "root").collect();
        let ty = match types.as_slice() {
            [one] => *one,
            [] => {
                let first = pairs.first().map(|(k, _)| k.as_str()).unwrap_or("");
                let hint = closest(first, TYPES.iter().copied()).map(|c| format!("did you mean '{c}'?"));
                self.err(path, "unknown_node", format!("no node type key here (one of: {})", TYPES[..16].join(", ")), hint);
                return None;
            }
            many => {
                self.err(path, "ambiguous_node", format!("a node has exactly one type key; this one has {}", many.join(", ")), None);
                return None;
            }
        };
        // Reserve the slot so parents come before children (pre-order).
        let index = self.nodes.len();
        self.nodes.push(Node { kind: NodeKind::ForceFailure, children: Vec::new(), label: String::new(), path: path.to_owned() });
        let body = v.get(ty).expect("present");
        let (kind, children, label) = match ty {
            "sequence" | "selector" => {
                self.keys(v, path, &[ty, "label", "reactive"]);
                let reactive = match v.get("reactive") {
                    None => true,
                    Some(Json::Bool(b)) => *b,
                    Some(_) => {
                        self.err(path, "invalid_value", "'reactive' must be true or false", None);
                        true
                    }
                };
                let kind = if ty == "sequence" { NodeKind::Sequence { reactive } } else { NodeKind::Selector { reactive } };
                (kind, self.children(body, path, ty, depth), Parser::label(v, format!("{ty}#{index}")))
            }
            "parallel" => {
                self.keys(v, path, &["parallel", "label", "succeed", "fail"]);
                let kids = self.children(body, path, ty, depth);
                let n = kids.len().max(1);
                let count = |p: &mut Parser, k: &str, d: usize| match v.get(k) {
                    None => d,
                    Some(c) => match c.as_u64() {
                        Some(c) if c >= 1 && c as usize <= n => c as usize,
                        _ => {
                            p.err(path, "invalid_value", format!("'{k}' must be 1..{n} (the number of children)"), None);
                            d
                        }
                    },
                };
                let succeed = count(self, "succeed", n);
                let fail = count(self, "fail", 1);
                (NodeKind::Parallel { succeed, fail }, kids, Parser::label(v, format!("parallel#{index}")))
            }
            "invert" | "force_success" | "force_failure" => {
                self.keys(v, path, &[ty, "label"]);
                let kind = match ty {
                    "invert" => NodeKind::Invert,
                    "force_success" => NodeKind::ForceSuccess,
                    _ => NodeKind::ForceFailure,
                };
                (kind, self.child(body, path, ty, depth), Parser::label(v, format!("{ty}#{index}")))
            }
            "repeat" | "retry" => {
                self.keys(v, path, &[ty, "label", "times"]);
                let times = match v.get("times") {
                    None => None,
                    Some(t) => match t.as_u64() {
                        Some(t) if (1..=1_000_000).contains(&t) => Some(t as u32),
                        _ => {
                            self.err(path, "invalid_value", "'times' must be an integer 1..1000000", None);
                            None
                        }
                    },
                };
                let kind = if ty == "repeat" { NodeKind::Repeat { times } } else { NodeKind::Retry { times: times.unwrap_or(3) } };
                (kind, self.child(body, path, ty, depth), Parser::label(v, format!("{ty}#{index}")))
            }
            "cooldown" | "timeout" => {
                self.keys(v, path, &[ty, "label", "ticks", "s"]);
                let time = self.time(v, path);
                let kind = if ty == "cooldown" { NodeKind::Cooldown { time } } else { NodeKind::Timeout { time } };
                (kind, self.child(body, path, ty, depth), Parser::label(v, format!("{ty}#{index}")))
            }
            "wait" | "wait_s" => {
                self.keys(v, path, &[ty, "label"]);
                let time = match (ty, body.as_f64()) {
                    ("wait", Some(t)) if (1.0..=1.0e6).contains(&t) && t == dmath::floor_f64(t) => Time::Ticks(t as u32),
                    ("wait_s", Some(s)) if s > 0.0 && s <= 1.0e5 => Time::Seconds(s as f32),
                    _ => {
                        let m = if ty == "wait" { "'wait' is a number of ticks (an integer 1..1000000); use 'wait_s' for seconds" } else { "'wait_s' is seconds, 0..100000" };
                        self.err(path, "invalid_value", m, None);
                        Time::Ticks(1)
                    }
                };
                (NodeKind::Wait { time }, Vec::new(), Parser::label(v, format!("wait#{index}")))
            }
            "if" => {
                let Some(name) = body.as_str() else {
                    self.err(path, "invalid_value", "'if' names a condition (a string)", None);
                    return self.drop(index);
                };
                if name == "bb" {
                    self.keys(v, path, &["if", "label", "key", "op", "value"]);
                    let key = v.get("key").and_then(Json::as_str).unwrap_or_default().to_owned();
                    if key.is_empty() {
                        self.err(path, "missing_argument", "'bb' needs 'key': the blackboard key to compare", None);
                    }
                    let op_name = v.get("op").and_then(Json::as_str).unwrap_or("==");
                    let op = match OPS.iter().find(|(n, _)| *n == op_name) {
                        Some((_, op)) => *op,
                        None => {
                            self.err(path, "invalid_value", format!("'op' must be one of {}", OPS.map(|(n, _)| n).join(", ")), None);
                            Op::Eq
                        }
                    };
                    let value = v.get("value").cloned().unwrap_or(Json::Null);
                    if op != Op::Exists && value.is_null() {
                        self.err(path, "missing_argument", "'bb' needs 'value' to compare with (or op 'exists')", None);
                    }
                    if matches!(op, Op::Lt | Op::Le | Op::Gt | Op::Ge) && value.as_f64().is_none() {
                        self.err(path, "invalid_value", "ordering ops (<, <=, >, >=) compare numbers: 'value' must be a number", None);
                    }
                    (NodeKind::Compare { key: key.clone(), op, value }, Vec::new(), Parser::label(v, format!("bb {key} {op_name}")))
                } else {
                    let vocab = self.vocab;
                    let Some(verb) = self.verb(&vocab.conditions, "condition", name, &format!("{path}.if")) else { return self.drop(index) };
                    let args = self.args(v, path, verb, &["if", "label"]);
                    (NodeKind::Condition { name: name.to_owned(), args }, Vec::new(), Parser::label(v, format!("if {name}")))
                }
            }
            "do" => {
                let Some(name) = body.as_str() else {
                    self.err(path, "invalid_value", "'do' names an action (a string)", None);
                    return self.drop(index);
                };
                let vocab = self.vocab;
                let Some(verb) = self.verb(&vocab.actions, "action", name, &format!("{path}.do")) else { return self.drop(index) };
                let args = self.args(v, path, verb, &["do", "label"]);
                (NodeKind::Action { name: name.to_owned(), args }, Vec::new(), Parser::label(v, format!("do {name}")))
            }
            "set" => {
                self.keys(v, path, &["set", "label", "value"]);
                let key = body.as_str().unwrap_or_default().to_owned();
                if key.is_empty() || key.len() > 64 {
                    self.err(path, "invalid_value", "'set' names a blackboard key (a string of 1..64 characters)", None);
                }
                let Some(value) = v.get("value").cloned() else {
                    self.err(path, "missing_argument", "'set' needs 'value'", None);
                    return self.drop(index);
                };
                (NodeKind::Set { key: key.clone(), value }, Vec::new(), Parser::label(v, format!("set {key}")))
            }
            _ => {
                self.keys(v, path, &["utility", "label", "momentum"]);
                let momentum = match v.get("momentum") {
                    None => 0.1,
                    Some(m) => match m.as_f64() {
                        Some(m) if (0.0..=1.0).contains(&m) => m as f32,
                        _ => {
                            self.err(path, "invalid_value", "'momentum' must be 0..1 (a bonus for the running option, so agents don't dither)", None);
                            0.1
                        }
                    },
                };
                let Some(items) = body.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_OPTIONS) else {
                    self.err(path, "invalid_value", format!("'utility' must be an array of 1..{MAX_OPTIONS} options"), None);
                    return self.drop(index);
                };
                let mut options = Vec::new();
                let mut kids = Vec::new();
                for (i, o) in items.iter().enumerate() {
                    let op = format!("{path}.utility[{i}]");
                    self.keys(o, &op, &["label", "weight", "considerations", "run"]);
                    let weight = match o.get("weight") {
                        None => 1.0,
                        Some(w) => match w.as_f64() {
                            Some(w) if (0.0..=1000.0).contains(&w) => w as f32,
                            _ => {
                                self.err(&op, "invalid_value", "'weight' must be 0..1000", None);
                                1.0
                            }
                        },
                    };
                    let mut cons = Vec::new();
                    match o.get("considerations") {
                        None => {}
                        Some(Json::Arr(list)) if list.len() <= MAX_CONSIDERATIONS => {
                            for (k, c) in list.iter().enumerate() {
                                if let Some(c) = self.consideration(c, &format!("{op}.considerations[{k}]")) {
                                    cons.push(c);
                                }
                            }
                        }
                        Some(_) => self.err(&op, "invalid_value", format!("'considerations' must be an array of at most {MAX_CONSIDERATIONS}"), None),
                    }
                    let Some(run) = o.get("run") else {
                        self.err(&op, "missing_key", "an option needs 'run': the node to run while it is chosen", None);
                        continue;
                    };
                    let Some(child) = self.node(run, &format!("{op}.run"), depth + 1) else { continue };
                    options.push(UtilOption { label: Parser::label(o, format!("option {i}")), weight, considerations: cons });
                    kids.push(child);
                }
                (NodeKind::Utility { options, momentum }, kids, Parser::label(v, format!("utility#{index}")))
            }
        };
        if kind_needs_children(&kind) && children.is_empty() {
            return self.drop(index);
        }
        self.nodes[index] = Node { kind, children, label, path: path.to_owned() };
        Some(index)
    }

    /// A node that failed to parse: its slot stays, as a failing wait, so indices hold (the
    /// definition is rejected anyway).
    fn drop(&mut self, index: usize) -> Option<usize> {
        self.nodes[index].kind = NodeKind::ForceFailure;
        None
    }

    fn consideration(&mut self, c: &Json, path: &str) -> Option<Consideration> {
        const RESERVED: [&str; 7] = ["input", "range", "curve", "at", "exponent", "steepness", "midpoint"];
        let Some(name) = c.get("input").and_then(Json::as_str) else {
            self.err(path, "missing_key", "a consideration needs 'input': the name of a value to score", None);
            return None;
        };
        let vocab = self.vocab;
        let verb = self.verb(&vocab.inputs, "input", name, &format!("{path}.input"))?;
        let args = self.args(c, path, verb, &RESERVED);
        let range = match c.get("range") {
            None => (0.0, 1.0),
            Some(r) => match r.as_array().map(|a| a.iter().map(Json::as_f64).collect::<Option<Vec<f64>>>()) {
                Some(Some(v)) if v.len() == 2 && v[0].is_finite() && v[1].is_finite() && v[0] != v[1] => (v[0], v[1]),
                _ => {
                    self.err(path, "invalid_value", "'range' must be [lo, hi] (two different numbers): the input's values that map to 0 and 1", None);
                    (0.0, 1.0)
                }
            },
        };
        let num = |p: &mut Parser, k: &str, d: f32, lo: f32, hi: f32| match c.get(k) {
            None => d,
            Some(v) => match v.as_f64() {
                Some(x) if (f64::from(lo)..=f64::from(hi)).contains(&x) => x as f32,
                _ => {
                    p.err(path, "invalid_value", format!("'{k}' must be {lo}..{hi}"), None);
                    d
                }
            },
        };
        let curve_name = c.get("curve").and_then(Json::as_str).unwrap_or("linear");
        let curve = match curve_name {
            "linear" => Curve::Linear,
            "quadratic" => Curve::Quadratic,
            "inverse" => Curve::Inverse,
            "step" => Curve::Step { at: num(self, "at", 0.5, 0.0, 1.0) },
            "power" => Curve::Power { exponent: num(self, "exponent", 2.0, 0.01, 16.0) },
            "logistic" => Curve::Logistic { steepness: num(self, "steepness", 10.0, -100.0, 100.0), midpoint: num(self, "midpoint", 0.5, 0.0, 1.0) },
            other => {
                let names = ["linear", "quadratic", "inverse", "step", "power", "logistic"];
                let hint = closest(other, names.iter().copied()).map(|n| format!("did you mean '{n}'?"));
                self.err(path, "invalid_value", format!("unknown curve '{other}' (one of: {})", names.join(", ")), hint);
                Curve::Linear
            }
        };
        Some(Consideration { input: name.to_owned(), args, range, curve })
    }
}

fn kind_needs_children(k: &NodeKind) -> bool {
    !matches!(k, NodeKind::Wait { .. } | NodeKind::Condition { .. } | NodeKind::Compare { .. } | NodeKind::Action { .. } | NodeKind::Set { .. })
}

fn is_point(v: &Json) -> bool {
    v.as_array().is_some_and(|a| a.len() == 3 && a.iter().all(|c| c.as_f64().is_some_and(f64::is_finite)))
}

fn check_arg(a: &Arg, v: &Json) -> Result<(), String> {
    let ok = match a.kind {
        ArgKind::Number { min, max } => v.as_f64().is_some_and(|x| x.is_finite() && x >= min && x <= max),
        ArgKind::Bool => v.as_bool().is_some(),
        ArgKind::Str => v.as_str().is_some(),
        ArgKind::Target => match v {
            Json::Str(s) => !s.is_empty() && s.len() <= 256,
            Json::Num(_) => v.as_u64().is_some(),
            _ => is_point(v),
        },
        ArgKind::Points => match v {
            Json::Str(s) => s.starts_with('/'),
            Json::Arr(list) => !list.is_empty() && list.len() <= 64 && list.iter().all(is_point),
            _ => false,
        },
        ArgKind::Any => true,
    };
    if ok {
        Ok(())
    } else {
        Err(a.kind.describe())
    }
}

impl Def {
    /// Parses and checks a definition against a host's vocabulary. Every problem found is
    /// reported (up to [`MAX_DIAGNOSTICS`]).
    pub fn parse(v: &Json, vocab: &Vocabulary) -> Result<Def, Vec<Diagnostic>> {
        let mut p = Parser { vocab, nodes: Vec::new(), diags: Vec::new() };
        let (root, path) = match v.get("root") {
            Some(r) => {
                p.keys(v, "", &["root", "label"]);
                (r, "root")
            }
            None => (v, "root"),
        };
        let label = v.get("root").and_then(|_| v.get("label")).and_then(Json::as_str).unwrap_or("mind").chars().take(64).collect();
        p.node(root, path, 0);
        if p.diags.is_empty() && p.nodes.is_empty() {
            p.err("root", "invalid_value", "the definition has no nodes", None);
        }
        if p.diags.is_empty() {
            Ok(Def { label, nodes: p.nodes })
        } else {
            Err(p.diags)
        }
    }

    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }

    /// Counts per node type, for reports.
    pub fn summary_json(&self) -> Json {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for n in &self.nodes {
            *counts.entry(n.kind.type_name()).or_default() += 1;
        }
        Json::obj([
            ("label", Json::from(self.label.as_str())),
            ("nodes", Json::from(self.nodes.len())),
            ("types", Json::obj(counts.into_iter().map(|(k, v)| (k, Json::from(v))))),
        ])
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn vocab() -> Vocabulary {
        let target = Arg { name: "target", kind: ArgKind::Target, required: true };
        Vocabulary {
            conditions: vec![Verb {
                name: "sees",
                doc: "",
                args: vec![target, Arg { name: "range", kind: ArgKind::Number { min: 0.0, max: 1000.0 }, required: false }],
            }],
            actions: vec![
                Verb { name: "chase", doc: "", args: vec![target] },
                Verb { name: "patrol", doc: "", args: vec![Arg { name: "points", kind: ArgKind::Points, required: true }] },
            ],
            inputs: vec![Verb { name: "distance", doc: "", args: vec![target] }],
        }
    }

    fn parse(text: &str) -> Result<Def, Vec<Diagnostic>> {
        Def::parse(&Json::parse(text).unwrap(), &vocab())
    }

    #[test]
    fn a_guard_parses() {
        let d = parse(
            r#"{"root": {"selector": [
                {"sequence": [{"if": "sees", "target": "/player", "range": 8}, {"do": "chase", "target": "/player"}]},
                {"do": "patrol", "points": [[0,0,0],[4,0,0]]}
            ]}, "label": "guard"}"#,
        )
        .unwrap();
        assert_eq!(d.label, "guard");
        assert_eq!(d.nodes.len(), 5);
        assert_eq!(d.nodes[0].children, vec![1, 4]);
        assert_eq!(d.nodes[2].path, "root.selector[0].sequence[0]");
    }

    #[test]
    fn diagnostics_say_where_and_what() {
        let e = parse(r#"{"sequence": [{"if": "seez", "target": "/p"}, {"do": "chase"}, {"do": "patrol", "points": "x", "speed": 2}, {"wiat": 3}]}"#).unwrap_err();
        let codes: Vec<(&str, &str)> = e.iter().map(|d| (d.path.as_str(), d.code)).collect();
        assert!(codes.contains(&("root.sequence[0].if", "unknown_name")), "{e:#?}");
        assert!(e.iter().any(|d| d.code == "unknown_name" && d.hint.as_deref() == Some("did you mean 'sees'?")));
        assert!(codes.contains(&("root.sequence[1]", "missing_argument")));
        assert!(codes.contains(&("root.sequence[2].points", "invalid_argument")));
        assert!(codes.contains(&("root.sequence[2]", "unknown_argument")));
        assert!(codes.contains(&("root.sequence[3]", "unknown_node")));
        assert!(e.iter().any(|d| d.hint.as_deref() == Some("did you mean 'wait'?")), "{e:#?}");
    }

    #[test]
    fn utility_and_curves() {
        let d = parse(
            r#"{"utility": [
                {"label": "chase", "considerations": [{"input": "distance", "target": "/p", "range": [0, 10], "curve": "inverse"}], "run": {"do": "chase", "target": "/p"}},
                {"label": "idle", "weight": 0.2, "run": {"wait": 10}}
            ], "momentum": 0.2}"#,
        )
        .unwrap();
        let NodeKind::Utility { options, momentum } = &d.nodes[0].kind else { panic!() };
        assert_eq!(options.len(), 2);
        assert_eq!(*momentum, 0.2);
        assert_eq!(options[0].considerations[0].score(2.5), 0.75);
        assert_eq!(Curve::Logistic { steepness: 10.0, midpoint: 0.5 }.eval(0.5), 0.5);
        assert_eq!(Curve::Step { at: 0.3 }.eval(0.29), 0.0);
    }

    #[test]
    fn caps_hold() {
        let mut deep = String::from(r#"{"wait": 1}"#);
        for _ in 0..40 {
            deep = format!(r#"{{"invert": {deep}}}"#);
        }
        let e = parse(&deep).unwrap_err();
        assert!(e.iter().any(|d| d.code == "budget"));
        let wide = format!(r#"{{"sequence": [{}]}}"#, vec![r#"{"wait": 1}"#; 600].join(","));
        let e = parse(&wide).unwrap_err();
        assert!(e.iter().any(|d| d.code == "budget"));
    }
}
