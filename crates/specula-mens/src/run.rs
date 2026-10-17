//! The runner: one [`Mind`] per agent ticks its definition against a [`Host`].
//!
//! Semantics, per tick:
//! - a **sequence** resumes at its running child and stops at the first failure; a reactive one
//!   (the default) first checks the conditions before that child again, and halts it when one
//!   fails (so `[if sees, do chase]` stops chasing the tick sight is lost);
//! - a **selector** (reactive) tries children from the first every tick, halting a lower one that
//!   was running when a higher one starts running or succeeds; non-reactive ones resume;
//! - a **parallel** ticks the children still going and counts results;
//! - **actions** get [`Phase::Start`] on their first tick, [`Phase::Continue`] while running, and
//!   [`Phase::Abort`] when a decision halts them;
//! - **repeat** and **retry** run their child again on the *next* tick (never twice in one), so a
//!   tick visits each node at most once;
//! - a **utility** node scores its options (product of considerations × weight, plus momentum
//!   for the running one), runs the best (ties: the earlier), and halts the one it replaces.

use crate::def::{Def, Node, NodeKind, Op};
use specula_core::Json;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

pub type Blackboard = BTreeMap<String, Json>;

/// Most trace lines a mind keeps.
pub const TRACE: usize = 16;
/// Most blackboard keys (writes past it are refused and traced).
pub const MAX_KEYS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Success,
    Failure,
    Running,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Success => "success",
            Status::Failure => "failure",
            Status::Running => "running",
        }
    }
}

/// Which call an action gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The first tick it runs (since it last finished or was halted).
    Start,
    Continue,
    /// A decision halted it while it was running (its result is ignored).
    Abort,
}

/// What a mind's definition calls into: the game's (or engine's) senses and deeds.
pub trait Host {
    fn condition(&mut self, name: &str, args: &Json, bb: &Blackboard) -> bool;
    fn action(&mut self, name: &str, args: &Json, phase: Phase, bb: &mut Blackboard) -> Status;
    fn input(&mut self, name: &str, args: &Json, bb: &Blackboard) -> f64;
}

#[derive(Clone, Debug, Default, PartialEq)]
struct NodeState {
    running: bool,
    /// Sequence / non-reactive selector: the child to resume; reactive selector: the running one.
    cursor: usize,
    /// Repeat / retry counts; wait / timeout ticks.
    count: u32,
    /// Cooldown: the tick it last succeeded.
    last_success: Option<u64>,
    /// Utility: the running option.
    chosen: Option<usize>,
    /// Utility: each option's last score and its considerations' scores.
    scores: Vec<(f32, Vec<f32>)>,
    /// Parallel: results of finished children this round.
    done: Vec<Option<Status>>,
}

/// One line of a mind's trace.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceLine {
    pub tick: u64,
    pub text: String,
}

/// An agent's mind: its definition, blackboard and running state.
#[derive(Clone, Debug)]
pub struct Mind {
    pub def: Arc<Def>,
    pub bb: Blackboard,
    states: Vec<NodeState>,
    pub tick: u64,
    pub hz: u32,
    pub trace: VecDeque<TraceLine>,
    /// The running leaves after the last tick (node indices).
    running_leaves: Vec<usize>,
    pub last_status: Option<Status>,
}

impl Mind {
    pub fn new(def: Arc<Def>, bb: Blackboard, hz: u32) -> Mind {
        let n = def.nodes.len();
        Mind { def, bb, states: vec![NodeState::default(); n], tick: 0, hz: hz.max(1), trace: VecDeque::new(), running_leaves: Vec::new(), last_status: None }
    }

    fn note(&mut self, text: String) {
        if self.trace.len() == TRACE {
            self.trace.pop_front();
        }
        self.trace.push_back(TraceLine { tick: self.tick, text });
    }

    /// Runs one tick at simulation tick `tick`.
    pub fn tick(&mut self, host: &mut dyn Host, tick: u64) -> Status {
        self.tick = tick;
        let def = Arc::clone(&self.def);
        let status = self.run(&def, 0, host);
        if !matches!(status, Status::Running) {
            // A finished root starts afresh next tick.
            self.reset_subtree(&def, 0);
        }
        let leaves: Vec<usize> = (0..def.nodes.len()).filter(|&i| self.states[i].running && def.nodes[i].children.is_empty()).collect();
        if leaves != self.running_leaves {
            let names: Vec<&str> = leaves.iter().map(|&i| def.nodes[i].label.as_str()).collect();
            let text = if names.is_empty() { format!("{} -> nothing running", status.name()) } else { format!("now: {}", names.join(", ")) };
            self.note(text);
            self.running_leaves = leaves;
        }
        if self.last_status != Some(status) && status != Status::Running {
            self.note(format!("root {}", status.name()));
        }
        self.last_status = Some(status);
        status
    }

    /// Halts everything running (actions get [`Phase::Abort`]).
    pub fn halt(&mut self, host: &mut dyn Host) {
        let def = Arc::clone(&self.def);
        self.halt_node(&def, 0, host);
        self.running_leaves.clear();
    }

    fn reset_subtree(&mut self, def: &Def, i: usize) {
        let keep = self.states[i].last_success;
        self.states[i] = NodeState { last_success: keep, ..NodeState::default() };
        for &c in &def.nodes[i].children {
            self.reset_subtree(def, c);
        }
    }

    fn halt_node(&mut self, def: &Def, i: usize, host: &mut dyn Host) {
        if !self.states[i].running {
            return;
        }
        let node = &def.nodes[i];
        for &c in &node.children {
            self.halt_node(def, c, host);
        }
        if let NodeKind::Action { name, args } = &node.kind {
            host.action(name, args, Phase::Abort, &mut self.bb);
        }
        let keep = self.states[i].last_success;
        self.states[i] = NodeState { last_success: keep, ..NodeState::default() };
    }

    fn finish(&mut self, def: &Def, i: usize, s: Status) -> Status {
        if s == Status::Running {
            self.states[i].running = true;
        } else {
            let keep = self.states[i].last_success;
            self.states[i] = NodeState { last_success: keep, ..NodeState::default() };
            // Children are finished too (a finished node never leaves running state below it).
            for &c in &def.nodes[i].children {
                if self.states[c].running {
                    self.reset_subtree(def, c);
                }
            }
        }
        s
    }

    fn run(&mut self, def: &Def, i: usize, host: &mut dyn Host) -> Status {
        let node: &Node = &def.nodes[i];
        let s = match &node.kind {
            NodeKind::Sequence { reactive } => {
                let mut out = Status::Success;
                let start = self.states[i].cursor;
                if *reactive && start > 0 {
                    let guards = node.children[..start].iter().filter(|&&c| matches!(def.nodes[c].kind, NodeKind::Condition { .. } | NodeKind::Compare { .. }));
                    let failed = guards.copied().find(|&c| self.run(def, c, host) == Status::Failure);
                    if let Some(g) = failed {
                        let running = node.children[start];
                        self.halt_node(def, running, host);
                        self.note(format!("{}: {} no longer holds, {} stopped", node.label, def.nodes[g].label, def.nodes[running].label));
                        let keep = self.states[i].last_success;
                        self.states[i] = NodeState { last_success: keep, ..NodeState::default() };
                        return Status::Failure;
                    }
                }
                for (k, &c) in node.children.iter().enumerate().skip(start) {
                    match self.run(def, c, host) {
                        Status::Success => continue,
                        Status::Failure => {
                            out = Status::Failure;
                            break;
                        }
                        Status::Running => {
                            self.states[i].cursor = k;
                            out = Status::Running;
                            break;
                        }
                    }
                }
                out
            }
            NodeKind::Selector { reactive } => {
                let start = if *reactive { 0 } else { self.states[i].cursor };
                let was = self.states[i].running.then_some(self.states[i].cursor);
                let mut out = Status::Failure;
                for (k, &c) in node.children.iter().enumerate().skip(start) {
                    let s = self.run(def, c, host);
                    if s == Status::Failure {
                        continue;
                    }
                    // A higher-priority child took over: halt the one that was running.
                    if let Some(w) = was.filter(|w| *w != k) {
                        let old = node.children[w];
                        if self.states[old].running {
                            self.halt_node(def, old, host);
                            self.note(format!("{}: {} takes over from {}", node.label, def.nodes[c].label, def.nodes[old].label));
                        }
                    }
                    self.states[i].cursor = k;
                    out = s;
                    break;
                }
                out
            }
            NodeKind::Parallel { succeed, fail } => {
                if self.states[i].done.len() != node.children.len() {
                    self.states[i].done = vec![None; node.children.len()];
                }
                for (k, &c) in node.children.iter().enumerate() {
                    if self.states[i].done[k].is_none() {
                        let s = self.run(def, c, host);
                        if s != Status::Running {
                            self.states[i].done[k] = Some(s);
                        }
                    }
                }
                let ok = self.states[i].done.iter().filter(|d| **d == Some(Status::Success)).count();
                let bad = self.states[i].done.iter().filter(|d| **d == Some(Status::Failure)).count();
                if ok >= *succeed || bad >= *fail {
                    for &c in &node.children {
                        self.halt_node(def, c, host);
                    }
                    if ok >= *succeed {
                        Status::Success
                    } else {
                        Status::Failure
                    }
                } else {
                    Status::Running
                }
            }
            NodeKind::Invert => match self.run(def, node.children[0], host) {
                Status::Success => Status::Failure,
                Status::Failure => Status::Success,
                Status::Running => Status::Running,
            },
            NodeKind::ForceSuccess => match self.run(def, node.children[0], host) {
                Status::Running => Status::Running,
                _ => Status::Success,
            },
            NodeKind::ForceFailure => {
                if node.children.is_empty() {
                    Status::Failure
                } else {
                    match self.run(def, node.children[0], host) {
                        Status::Running => Status::Running,
                        _ => Status::Failure,
                    }
                }
            }
            NodeKind::Repeat { times } => match self.run(def, node.children[0], host) {
                Status::Failure => Status::Failure,
                Status::Running => Status::Running,
                Status::Success => {
                    self.states[i].count += 1;
                    if times.is_some_and(|t| self.states[i].count >= t) {
                        Status::Success
                    } else {
                        // Again next tick.
                        self.reset_subtree(def, node.children[0]);
                        Status::Running
                    }
                }
            },
            NodeKind::Retry { times } => match self.run(def, node.children[0], host) {
                Status::Success => Status::Success,
                Status::Running => Status::Running,
                Status::Failure => {
                    self.states[i].count += 1;
                    if self.states[i].count >= *times {
                        Status::Failure
                    } else {
                        self.reset_subtree(def, node.children[0]);
                        Status::Running
                    }
                }
            },
            NodeKind::Cooldown { time } => {
                let ticks = u64::from(time.ticks(self.hz));
                let cooling = self.states[i].last_success.is_some_and(|t| self.tick < t + ticks);
                if cooling && !self.states[i].running {
                    Status::Failure
                } else {
                    let s = self.run(def, node.children[0], host);
                    if s == Status::Success {
                        self.states[i].last_success = Some(self.tick);
                    }
                    s
                }
            }
            NodeKind::Timeout { time } => {
                let s = self.run(def, node.children[0], host);
                if s == Status::Running {
                    self.states[i].count += 1;
                    if self.states[i].count >= time.ticks(self.hz) {
                        self.halt_node(def, node.children[0], host);
                        self.note(format!("{} timed out", node.label));
                        Status::Failure
                    } else {
                        Status::Running
                    }
                } else {
                    s
                }
            }
            NodeKind::Wait { time } => {
                self.states[i].count += 1;
                if self.states[i].count >= time.ticks(self.hz) {
                    Status::Success
                } else {
                    Status::Running
                }
            }
            NodeKind::Condition { name, args } => {
                if host.condition(name, args, &self.bb) {
                    Status::Success
                } else {
                    Status::Failure
                }
            }
            NodeKind::Compare { key, op, value } => {
                let have = self.bb.get(key);
                let ok = match op {
                    Op::Exists => have.is_some_and(|v| !v.is_null()),
                    Op::Eq => have.is_some_and(|v| json_eq(v, value)),
                    Op::Ne => !have.is_some_and(|v| json_eq(v, value)),
                    _ => match (have.and_then(Json::as_f64), value.as_f64()) {
                        (Some(a), Some(b)) => match op {
                            Op::Lt => a < b,
                            Op::Le => a <= b,
                            Op::Gt => a > b,
                            _ => a >= b,
                        },
                        _ => false,
                    },
                };
                if ok {
                    Status::Success
                } else {
                    Status::Failure
                }
            }
            NodeKind::Action { name, args } => {
                let phase = if self.states[i].running { Phase::Continue } else { Phase::Start };
                host.action(name, args, phase, &mut self.bb)
            }
            NodeKind::Set { key, value } => {
                if self.bb.len() >= MAX_KEYS && !self.bb.contains_key(key) {
                    self.note(format!("set {key} refused: the blackboard holds at most {MAX_KEYS} keys"));
                    Status::Failure
                } else {
                    self.bb.insert(key.clone(), value.clone());
                    Status::Success
                }
            }
            NodeKind::Utility { options, momentum } => {
                let current = self.states[i].chosen;
                let mut scores = Vec::with_capacity(options.len());
                let mut best: Option<(f32, usize)> = None;
                for (k, o) in options.iter().enumerate() {
                    let mut parts = Vec::with_capacity(o.considerations.len());
                    let mut score = o.weight;
                    for c in &o.considerations {
                        let v = host.input(&c.input, &c.args, &self.bb);
                        let s = c.score(v);
                        parts.push(s);
                        score *= s;
                    }
                    if current == Some(k) {
                        score += momentum;
                    }
                    if best.is_none_or(|(b, _)| score > b) {
                        best = Some((score, k));
                    }
                    scores.push((score, parts));
                }
                self.states[i].scores = scores;
                let (_, pick) = best.expect("at least one option");
                if let Some(old) = current.filter(|c| *c != pick) {
                    self.halt_node(def, node.children[old], host);
                    self.note(format!("{}: {} over {}", node.label, options[pick].label, options[old].label));
                } else if current.is_none() {
                    self.note(format!("{}: chose {}", node.label, options[pick].label));
                }
                let s = self.run(def, node.children[pick], host);
                // Keep the scores and choice while running (finish() clears them otherwise).
                if s == Status::Running {
                    self.states[i].chosen = Some(pick);
                }
                s
            }
        };
        if s == Status::Running {
            self.states[i].running = true;
            s
        } else {
            let scores = std::mem::take(&mut self.states[i].scores);
            let out = self.finish(def, i, s);
            self.states[i].scores = scores;
            out
        }
    }

    /// The running path, per utility node its options' scores, the blackboard (capped at
    /// `max_bytes` of JSON) and the trace.
    pub fn status_json(&self, max_bytes: usize) -> Json {
        let def = &self.def;
        let running: Vec<Json> = (0..def.nodes.len())
            .filter(|&i| self.states[i].running)
            .take(32)
            .map(|i| Json::obj([("node", Json::from(def.nodes[i].label.as_str())), ("path", Json::from(def.nodes[i].path.as_str()))]))
            .collect();
        let mut utility = Vec::new();
        for (i, n) in def.nodes.iter().enumerate() {
            if let NodeKind::Utility { options, .. } = &n.kind {
                let st = &self.states[i];
                if st.scores.is_empty() {
                    continue;
                }
                let opts = options.iter().zip(&st.scores).map(|(o, (s, parts))| {
                    Json::obj([
                        ("option", Json::from(o.label.as_str())),
                        ("score", Json::from((f64::from(*s) * 1000.0).round() / 1000.0)),
                        (
                            "considerations",
                            Json::arr(o.considerations.iter().zip(parts).map(|(c, p)| {
                                Json::obj([("input", Json::from(c.input.as_str())), ("score", Json::from((f64::from(*p) * 1000.0).round() / 1000.0))])
                            })),
                        ),
                    ])
                });
                utility.push(Json::obj([
                    ("node", Json::from(n.label.as_str())),
                    ("chosen", Json::from(st.chosen.map(|c| options[c].label.as_str()))),
                    ("options", Json::arr(opts)),
                ]));
            }
        }
        let mut bb = Vec::new();
        let mut bytes = 0;
        let mut cut = 0;
        for (k, v) in &self.bb {
            let size = k.len() + v.to_string().len();
            if bytes + size > max_bytes {
                cut += 1;
                continue;
            }
            bytes += size;
            bb.push((k.clone(), v.clone()));
        }
        let mut out = Json::obj([
            ("label", Json::from(def.label.as_str())),
            ("tick", Json::from(self.tick)),
            ("status", Json::from(self.last_status.map(Status::name))),
            ("running", Json::Arr(running)),
            ("utility", Json::Arr(utility)),
            ("blackboard", Json::Obj(bb)),
            ("trace", Json::arr(self.trace.iter().map(|t| Json::obj([("tick", Json::from(t.tick)), ("text", Json::from(t.text.as_str()))])))),
        ]);
        if cut > 0 {
            out.set("blackboard_cut", Json::from(cut));
        }
        out
    }

    /// Adds the running state and blackboard to a hash.
    pub fn hash_into(&self, h: &mut specula_core::hash::StableHasher) {
        h.write_u64(self.states.len() as u64);
        for s in &self.states {
            h.write_bool(s.running);
            h.write_u64(s.cursor as u64);
            h.write_u32(s.count);
            h.write_u64(s.last_success.unwrap_or(u64::MAX));
            h.write_u64(s.chosen.map_or(u64::MAX, |c| c as u64));
            for d in &s.done {
                h.write_u8(d.map_or(9, |s| s as u8));
            }
        }
        h.write_u64(self.bb.len() as u64);
        for (k, v) in &self.bb {
            h.write_str(k);
            h.write_str(&v.to_string());
        }
    }
}

/// Equality that treats numbers by value (1 == 1.0).
fn json_eq(a: &Json, b: &Json) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::tests::vocab;

    /// A scripted world: whether the player is seen, the distance, and a log of action calls.
    #[derive(Default)]
    struct World {
        sees: bool,
        distance: f64,
        log: Vec<String>,
        /// Ticks a chase/patrol step runs before succeeding.
        steps: u32,
        progress: u32,
    }

    impl Host for World {
        fn condition(&mut self, name: &str, _: &Json, _: &Blackboard) -> bool {
            name == "sees" && self.sees
        }
        fn action(&mut self, name: &str, _: &Json, phase: Phase, _: &mut Blackboard) -> Status {
            self.log.push(format!("{name}:{phase:?}"));
            match phase {
                Phase::Abort => Status::Failure,
                Phase::Start => {
                    self.progress = 0;
                    Status::Running
                }
                Phase::Continue => {
                    self.progress += 1;
                    if self.steps > 0 && self.progress >= self.steps {
                        Status::Success
                    } else {
                        Status::Running
                    }
                }
            }
        }
        fn input(&mut self, _: &str, _: &Json, _: &Blackboard) -> f64 {
            self.distance
        }
    }

    fn mind(text: &str) -> Mind {
        Mind::new(Arc::new(Def::parse(&Json::parse(text).unwrap(), &vocab()).unwrap()), Blackboard::new(), 30)
    }

    const GUARD: &str = r#"{"selector": [
        {"sequence": [{"if": "sees", "target": "/player"}, {"do": "chase", "target": "/player"}]},
        {"do": "patrol", "points": [[0,0,0],[4,0,0]]}
    ]}"#;

    #[test]
    fn reactive_selector_switches_and_aborts() {
        let mut m = mind(GUARD);
        let mut w = World::default();
        assert_eq!(m.tick(&mut w, 1), Status::Running);
        assert_eq!(m.tick(&mut w, 2), Status::Running);
        assert_eq!(w.log, ["patrol:Start", "patrol:Continue"]);
        w.sees = true;
        m.tick(&mut w, 3);
        assert_eq!(w.log[2..], ["chase:Start".to_string(), "patrol:Abort".to_string()]);
        m.tick(&mut w, 4);
        assert_eq!(w.log.last().unwrap(), "chase:Continue");
        w.sees = false;
        m.tick(&mut w, 5);
        // Lost sight: the chase stops, patrol starts again.
        assert_eq!(w.log[w.log.len() - 2..], ["chase:Abort".to_string(), "patrol:Start".to_string()], "{:?}", w.log);
        let s = m.status_json(1024);
        assert_eq!(s.get("running").and_then(Json::as_array).map(|a| a.len()), Some(2), "{}", s.to_pretty());
    }

    #[test]
    fn sequences_resume_and_waits_count() {
        let mut m = mind(r#"{"sequence": [{"set": "phase", "value": 1}, {"wait": 3}, {"set": "phase", "value": 2}]}"#);
        let mut w = World::default();
        assert_eq!(m.tick(&mut w, 1), Status::Running);
        assert_eq!(m.bb.get("phase"), Some(&Json::from(1)));
        assert_eq!(m.tick(&mut w, 2), Status::Running);
        assert_eq!(m.tick(&mut w, 3), Status::Success);
        assert_eq!(m.bb.get("phase"), Some(&Json::from(2)));
    }

    #[test]
    fn utility_picks_with_momentum() {
        let mut m = mind(
            r#"{"utility": [
                {"label": "chase", "considerations": [{"input": "distance", "target": "/p", "range": [0, 10], "curve": "inverse"}], "run": {"do": "chase", "target": "/p"}},
                {"label": "patrol", "weight": 0.5, "run": {"do": "patrol", "points": [[0,0,0]]}}
            ], "momentum": 0.1}"#,
        );
        let mut w = World { distance: 8.0, ..World::default() };
        m.tick(&mut w, 1);
        assert_eq!(w.log, ["patrol:Start"]);
        // chase 0.45 vs patrol 0.5 + 0.1 momentum: stays.
        w.distance = 5.5;
        m.tick(&mut w, 2);
        assert_eq!(w.log.last().unwrap(), "patrol:Continue");
        // chase 0.7 beats 0.6.
        w.distance = 3.0;
        m.tick(&mut w, 3);
        assert_eq!(w.log[2..], ["patrol:Abort".to_string(), "chase:Start".to_string()]);
        let s = m.status_json(1024);
        let u = &s.get("utility").and_then(Json::as_array).unwrap()[0];
        assert_eq!(u.get("chosen").and_then(Json::as_str), Some("chase"), "{}", s.to_pretty());
    }

    #[test]
    fn repeat_cooldown_timeout() {
        // Repeat twice: a one-step chase, then a 2-tick wait.
        let mut m = mind(r#"{"repeat": {"sequence": [{"do": "chase", "target": "/p"}, {"wait": 2}]}, "times": 2}"#);
        let mut w = World { steps: 1, ..World::default() };
        let mut t = 0;
        let mut status = Status::Running;
        while status == Status::Running && t < 50 {
            t += 1;
            status = m.tick(&mut w, t);
        }
        assert_eq!(status, Status::Success);
        assert_eq!(w.log.iter().filter(|l| l.ends_with("Start")).count(), 2);
        // A cooldown fails while cooling, then runs again.
        let mut m = mind(r#"{"cooldown": {"do": "chase", "target": "/p"}, "ticks": 5}"#);
        let mut w = World { steps: 1, ..World::default() };
        assert_eq!(m.tick(&mut w, 1), Status::Running);
        assert_eq!(m.tick(&mut w, 2), Status::Success);
        assert_eq!(m.tick(&mut w, 3), Status::Failure);
        assert_eq!(m.tick(&mut w, 6), Status::Failure);
        assert_eq!(m.tick(&mut w, 7), Status::Running);
        let mut m = mind(r#"{"timeout": {"do": "chase", "target": "/p"}, "ticks": 4}"#);
        let mut w = World::default();
        let mut t = 0;
        while m.tick(&mut w, t) == Status::Running {
            t += 1;
        }
        assert_eq!(t, 3);
        assert_eq!(w.log.last().unwrap(), "chase:Abort");
    }

    #[test]
    fn minds_are_deterministic() {
        let run = || {
            let mut m = mind(GUARD);
            let mut w = World { steps: 3, ..World::default() };
            let mut h = specula_core::hash::StableHasher::default();
            for t in 0..200u64 {
                w.sees = (t / 17) % 2 == 1;
                m.tick(&mut w, t);
                m.hash_into(&mut h);
            }
            (h.finish(), w.log)
        };
        assert_eq!(run(), run());
    }
}
