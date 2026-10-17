//! Playing an [`AnimGraph`]: the state is **data** ([`GraphPlay`], what the engine keeps in an
//! entity's `anim` component) and every pose is a **pure function of time** of it.
//!
//! A play holds parameter values, the graph clock at its anchor (the anim's `start`) and, per
//! layer, a short *blend stack* of states (the current state on top, the ones it is fading in
//! over below, each with its phase and crossfade). Time runs in **graph seconds**: `τ = clock +
//! (T − start) × speed`, so pausing (speed 0) freezes motion and fades alike.
//!
//! - **Resolving** a play at `τ` runs its state machines forward from the anchor with the stored
//!   parameters, which are constant from the anchor on: transitions whose conditions hold fire at
//!   once, exit-time transitions at the moment their state's phase reaches the exit. A periodic
//!   chain (idle → fidget → idle …) is detected and skipped a whole number of periods at a time,
//!   so the cost is bounded however far `τ` is from the anchor ([`MAX_STEPS`] transitions per
//!   resolve otherwise; past that the play is reported `truncated`).
//! - **Settling** ([`settle`]) folds what happened up to `now` into a new play anchored at `now`
//!   (phases continuous, fades still running, triggers that fired consumed). Writing new
//!   parameters is "settle at now, then store the new values": the engine does it on every
//!   parameter write, so changes blend instead of snapping.
//! - **Evaluating** ([`GraphPlayer::evaluate`]) samples every layer's stack (blend trees in step,
//!   crossfades with smoothstep), applies layers (override or additive, weighted, masked), strips
//!   root motion, then runs IK ([`crate::ik`]): feet on the ground the caller supplies, two-bone
//!   reaches, look-at. A steady frame allocates nothing.
//! - [`events`] and [`root_motion`] answer what fired and how far the root moved between two
//!   times: deterministic (exact `f64` time, IEEE basic operations and `dmath`), so they can feed
//!   a simulation sampled at its ticks. Poses, IK and events in the engine's render path are
//!   cosmetic.

use specula_core::hash::StableHasher;
use specula_core::{dmath, Json};

use crate::animator::{LayerMode, RootMotion, MAX_RATE};
use crate::blend::{self, Base};
use crate::clip::{Channel, LoopMode};
use crate::error::{err, is_ident, AnimError, AnimErrorCode};
use crate::graph::{self, Amount, AnimGraph, CondValue, Node, NodeId, Op, ParamKind, Solver, Value};
use crate::ik::{self, Ground};
use crate::math::{smoothstep, Affine, V3};
use crate::set::{AnimSet, ClipId};
use crate::skeleton::Pose;

/// Most states a layer's blend stack keeps (the current one and those it fades over).
pub const MAX_STACK: usize = 4;
/// Most transitions one resolve walks (after skipping whole periods of a periodic chain).
pub const MAX_STEPS: usize = 256;
/// Most graph-seconds a play's clock or phase may hold.
pub const MAX_CLOCK: f64 = 1e9;
/// Most events [`events`] reports in one call.
pub const MAX_EVENTS: usize = 256;
/// Longest window (graph seconds) [`events`] and [`root_motion`] look at.
pub const MAX_WINDOW_S: f64 = 600.0;

// ---------------------------------------------------------------------------------------------
// The persisted play

/// One state in a layer's blend stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub state: String,
    /// Cycles of the state's node at the play's clock (may be negative: entered later).
    pub phase: f64,
    /// Crossfade `(start, seconds)` in graph seconds (`None`: fully in).
    pub fade: Option<(f64, f32)>,
}

/// One layer's blend stack, bottom first.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerPlay {
    pub layer: String,
    pub stack: Vec<Entry>,
}

/// A graph playing: what an entity's `anim` holds in graph mode.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphPlay {
    /// The graph's name in the model's anim set.
    pub graph: String,
    /// Parameter values by name (sorted, unique); missing ones take the graph's defaults.
    pub params: Vec<(String, Value)>,
    /// Graph seconds at the anchor.
    pub clock: f64,
    /// Settled blend stacks; a layer not listed starts in its entry state at the anchor.
    pub layers: Vec<LayerPlay>,
}

impl GraphPlay {
    /// A fresh play of `graph`: defaults, entry states.
    pub fn new(graph: impl Into<String>) -> GraphPlay {
        GraphPlay { graph: graph.into(), params: Vec::new(), clock: 0.0, layers: Vec::new() }
    }

    pub fn param(&self, name: &str) -> Option<&Value> {
        self.params.binary_search_by(|(n, _)| n.as_str().cmp(name)).ok().map(|i| &self.params[i].1)
    }

    /// Sets (or replaces) a parameter, keeping the list sorted.
    pub fn set_param(&mut self, name: &str, v: Value) {
        match self.params.binary_search_by(|(n, _)| n.as_str().cmp(name)) {
            Ok(i) => self.params[i].1 = v,
            Err(i) => self.params.insert(i, (name.to_string(), v)),
        }
    }

    /// Checks names, counts and numbers (not against a graph: see [`bind_problems`]).
    pub fn check(&self) -> Result<(), String> {
        let name_ok = |s: &str| s.len() <= graph::MAX_NAME && is_ident(s);
        if !name_ok(&self.graph) {
            return Err(format!("'anim.graph' must be a graph name (an identifier of at most {} characters)", graph::MAX_NAME));
        }
        if self.params.len() > graph::MAX_PARAMS {
            return Err(format!("'anim.params' has {} parameters; at most {}", self.params.len(), graph::MAX_PARAMS));
        }
        for (i, (n, v)) in self.params.iter().enumerate() {
            if !name_ok(n) {
                return Err(format!("'anim.params.{n}' is not a parameter name"));
            }
            if i > 0 && self.params[i - 1].0 >= *n {
                return Err("'anim.params' must be sorted by name, each once".into());
            }
            let finite = match v {
                Value::Num(x) => x.is_finite(),
                Value::Vec3(p) => p.iter().all(|x| x.is_finite() && x.abs() <= 1e6),
                Value::Name(s) => name_ok(s),
                Value::Bool(_) => true,
            };
            if !finite {
                return Err(format!("'anim.params.{n}' must be finite (and |x| ≤ 1e6), or a name"));
            }
        }
        if !(self.clock.is_finite() && self.clock.abs() <= MAX_CLOCK) {
            return Err(format!("'anim.clock' must be graph seconds within ±{MAX_CLOCK}"));
        }
        if self.layers.len() > graph::MAX_LAYERS {
            return Err(format!("'anim.layers' has {} layers; at most {}", self.layers.len(), graph::MAX_LAYERS));
        }
        for (i, l) in self.layers.iter().enumerate() {
            if !name_ok(&l.layer) || self.layers[..i].iter().any(|o| o.layer == l.layer) {
                return Err(format!("'anim.layers.{}' must be a layer name, each once", l.layer));
            }
            if l.stack.is_empty() || l.stack.len() > MAX_STACK {
                return Err(format!("'anim.layers.{}' holds 1..={MAX_STACK} states", l.layer));
            }
            for e in &l.stack {
                if !name_ok(&e.state) || !(e.phase.is_finite() && e.phase.abs() <= MAX_CLOCK) {
                    return Err(format!("'anim.layers.{}': a state is {{state, phase (finite cycles), fade?}}", l.layer));
                }
                if let Some((s, d)) = e.fade {
                    if !(s.is_finite() && s.abs() <= MAX_CLOCK && d.is_finite() && (0.0..=60.0).contains(&d)) {
                        return Err(format!("'anim.layers.{}': fade is [start graph seconds, 0..=60 seconds]", l.layer));
                    }
                }
            }
        }
        Ok(())
    }

    /// The fields of a play (`graph`, `params`, `clock`, `layers`); the anim component adds its own.
    pub fn json_fields(&self) -> Vec<(&'static str, Json)> {
        let mut f = vec![("graph", Json::Str(self.graph.clone()))];
        if !self.params.is_empty() {
            f.push(("params", Json::obj(self.params.iter().map(|(n, v)| (n.clone(), v.to_json())))));
        }
        if self.clock != 0.0 {
            f.push(("clock", Json::Num(self.clock)));
        }
        if !self.layers.is_empty() {
            f.push((
                "layers",
                Json::obj(self.layers.iter().map(|l| {
                    (
                        l.layer.clone(),
                        Json::arr(l.stack.iter().map(|e| {
                            let mut o = vec![("state", Json::Str(e.state.clone())), ("phase", Json::Num(e.phase))];
                            if let Some((s, d)) = e.fade {
                                o.push(("fade", Json::arr([Json::Num(s), Json::Num(d as f64)])));
                            }
                            Json::obj(o)
                        })),
                    )
                })),
            ));
        }
        f
    }

    /// Reads a play's fields from an anim object (other fields are the caller's).
    pub fn from_json_fields(v: &Json) -> Result<GraphPlay, String> {
        let graph = v.get("graph").and_then(Json::as_str).ok_or("'anim.graph' must be the name of a graph in the model's anim set")?;
        let mut play = GraphPlay::new(graph);
        match v.get("params") {
            None | Some(Json::Null) => {}
            Some(Json::Obj(pairs)) => {
                if pairs.len() > graph::MAX_PARAMS {
                    return Err(format!("'anim.params' has {} parameters; at most {}", pairs.len(), graph::MAX_PARAMS));
                }
                for (n, pv) in pairs {
                    if play.param(n).is_some() {
                        return Err(format!("'anim.params.{n}' is given twice"));
                    }
                    let val = Value::from_json(pv, &format!("anim.params.{n}")).map_err(|e| format!("'{}': {}", e.path, e.message))?;
                    play.set_param(n, val);
                }
            }
            Some(_) => return Err("'anim.params' must be an object of parameter values".into()),
        }
        match v.get("clock") {
            None | Some(Json::Null) => {}
            Some(c) => play.clock = c.as_f64().ok_or("'anim.clock' must be graph seconds")?,
        }
        match v.get("layers") {
            None | Some(Json::Null) => {}
            Some(Json::Obj(pairs)) => {
                if pairs.len() > graph::MAX_LAYERS {
                    return Err(format!("'anim.layers' has {} layers; at most {}", pairs.len(), graph::MAX_LAYERS));
                }
                for (name, stack) in pairs {
                    let items = stack.as_array().filter(|a| a.len() <= MAX_STACK).ok_or_else(|| format!("'anim.layers.{name}' must be a list of at most {MAX_STACK} states"))?;
                    let mut out = Vec::new();
                    for it in items {
                        let state = it.get("state").and_then(Json::as_str).ok_or_else(|| format!("'anim.layers.{name}': each entry is {{state, phase, fade?}}"))?;
                        let phase = it.get("phase").map_or(Some(0.0), Json::as_f64).ok_or_else(|| format!("'anim.layers.{name}': phase is a number"))?;
                        let fade = match it.get("fade") {
                            None | Some(Json::Null) => None,
                            Some(f) => {
                                let a = f.as_array().filter(|a| a.len() == 2).ok_or_else(|| format!("'anim.layers.{name}': fade is [start, seconds]"))?;
                                Some((a[0].as_f64().ok_or("fade start is a number")?, a[1].as_f64().ok_or("fade seconds is a number")? as f32))
                            }
                        };
                        for k in it.as_object().into_iter().flatten().map(|(k, _)| k) {
                            if !matches!(k.as_str(), "state" | "phase" | "fade") {
                                return Err(format!("'anim.layers.{name}': '{k}' is not an entry field (state, phase, fade)"));
                            }
                        }
                        out.push(Entry { state: state.to_string(), phase, fade });
                    }
                    play.layers.push(LayerPlay { layer: name.clone(), stack: out });
                }
            }
            Some(_) => return Err("'anim.layers' must be an object: layer name → [{state, phase, fade?}]".into()),
        }
        play.check()?;
        Ok(play)
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.write_str(&self.graph);
        h.write_u64(self.params.len() as u64);
        for (n, v) in &self.params {
            h.write_str(n);
            match v {
                Value::Num(x) => {
                    h.write_u8(0);
                    h.write_f32(*x);
                }
                Value::Bool(b) => {
                    h.write_u8(1);
                    h.write_u8(*b as u8);
                }
                Value::Name(s) => {
                    h.write_u8(2);
                    h.write_str(s);
                }
                Value::Vec3(p) => {
                    h.write_u8(3);
                    p.iter().for_each(|x| h.write_f32(*x));
                }
            }
        }
        h.write_f64(self.clock);
        h.write_u64(self.layers.len() as u64);
        for l in &self.layers {
            h.write_str(&l.layer);
            h.write_u64(l.stack.len() as u64);
            for e in &l.stack {
                h.write_str(&e.state);
                h.write_f64(e.phase);
                match e.fade {
                    None => h.write_u8(0),
                    Some((s, d)) => {
                        h.write_u8(1);
                        h.write_f64(s);
                        h.write_f32(d);
                    }
                }
            }
        }
    }

    /// Approximate heap bytes (for the world's memory accounting).
    pub fn heap_bytes(&self) -> usize {
        self.graph.len()
            + self.params.iter().map(|(n, v)| n.len() + 32 + if let Value::Name(s) = v { s.len() } else { 0 }).sum::<usize>()
            + self.layers.iter().map(|l| l.layer.len() + 32 + l.stack.iter().map(|e| e.state.len() + 48).sum::<usize>()).sum::<usize>()
    }
}

/// How a play maps onto world time: `τ = clock + (t − anchor) × speed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    /// World time of the play's clock (the anim's `start`).
    pub anchor: f64,
    /// Graph seconds per world second (0 pauses).
    pub speed: f32,
}

impl Timing {
    pub fn tau(&self, clock: f64, t: f64) -> f64 {
        clock + (t - self.anchor) * self.speed as f64
    }
}

// ---------------------------------------------------------------------------------------------
// Binding a play to its graph

/// A parameter value, resolved against the graph.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RVal {
    Num(f32),
    Bool(bool),
    Enum(u16),
    Vec3(Option<[f32; 3]>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct REntry {
    state: u16,
    phase: f64,
    fade: Option<(f64, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RLayer {
    stack: [REntry; MAX_STACK],
    len: usize,
}

impl RLayer {
    fn single(state: u16, phase: f64) -> RLayer {
        let e = REntry { state, phase, fade: None };
        RLayer { stack: [e; MAX_STACK], len: 1 }
    }

    fn top(&self) -> &REntry {
        &self.stack[self.len - 1]
    }

    fn entries(&self) -> &[REntry] {
        &self.stack[..self.len]
    }
}

fn fade_weight(fade: Option<(f64, f32)>, tau: f64) -> f32 {
    match fade {
        None => 1.0,
        Some((_, d)) if d <= 0.0 => 1.0,
        Some((s, d)) => smoothstep(((tau - s) / d as f64) as f32),
    }
}

/// Problems binding a play to a graph: unknown parameters, wrong types, unknown states.
pub fn bind_problems(graph: &AnimGraph, play: &GraphPlay) -> Vec<String> {
    let mut out = Vec::new();
    for (n, v) in &play.params {
        match graph.param(n) {
            None => out.push(format!("no parameter '{n}' in graph '{}' (params: {})", play.graph, graph.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "))),
            Some(i) => {
                if let Err(m) = graph.params[i as usize].accept(v) {
                    out.push(m);
                }
            }
        }
    }
    for l in &play.layers {
        match graph.layer(&l.layer) {
            None => out.push(format!("no layer '{}' in graph '{}'", l.layer, play.graph)),
            Some(li) => {
                for e in &l.stack {
                    if graph.layers[li].state(&e.state).is_none() {
                        out.push(format!("no state '{}' in layer '{}' (it starts over in its entry state)", e.state, l.layer));
                    }
                }
            }
        }
    }
    out
}

/// Parameter values for the graph: the play's where valid, else the defaults.
fn bind_params(graph: &AnimGraph, play: &GraphPlay, out: &mut Vec<RVal>) {
    out.clear();
    for p in &graph.params {
        let v = play.param(&p.name).and_then(|v| p.accept(v).ok()).or_else(|| p.default.clone());
        out.push(match (&p.kind, v) {
            (ParamKind::Float { .. }, Some(Value::Num(x))) => RVal::Num(x),
            (ParamKind::Bool | ParamKind::Trigger, Some(Value::Bool(b))) => RVal::Bool(b),
            (ParamKind::Enum(values), Some(Value::Name(n))) => RVal::Enum(values.iter().position(|v| *v == n).unwrap_or(0) as u16),
            (ParamKind::Vec3, Some(Value::Vec3(v))) => RVal::Vec3(Some(v)),
            (ParamKind::Vec3, _) => RVal::Vec3(None),
            (ParamKind::Float { .. }, _) => RVal::Num(0.0),
            (ParamKind::Enum(_), _) => RVal::Enum(0),
            _ => RVal::Bool(false),
        });
    }
}

fn bind_layer(graph: &AnimGraph, li: usize, play: &GraphPlay) -> RLayer {
    let layer = &graph.layers[li];
    let Some(lp) = play.layers.iter().find(|l| l.layer == layer.name) else { return RLayer::single(layer.entry, 0.0) };
    let mut out = RLayer::single(layer.entry, 0.0);
    out.len = 0;
    for e in lp.stack.iter().take(MAX_STACK) {
        if let Some(s) = layer.state(&e.state) {
            out.stack[out.len] = REntry { state: s, phase: e.phase, fade: e.fade };
            out.len += 1;
        }
    }
    if out.len == 0 {
        return RLayer::single(layer.entry, 0.0);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Blend trees

/// A clip a node plays, with its share.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Leaf {
    /// `None`: the external pose.
    clip: Option<ClipId>,
    weight: f32,
    /// Sampled as a delta and added.
    additive: bool,
    speed: f32,
}

fn amount(a: Amount, params: &[RVal]) -> f32 {
    match a {
        Amount::Const(x) => x,
        Amount::Param(i, k) => match params[i as usize] {
            RVal::Num(x) => x * k,
            _ => 0.0,
        },
    }
}

fn float_of(params: &[RVal], i: u16) -> f32 {
    match params[i as usize] {
        RVal::Num(x) => x,
        _ => 0.0,
    }
}

/// Appends `node`'s leaves with weights scaled by `w`.
fn leaves(graph: &AnimGraph, node: NodeId, params: &[RVal], w: f32, additive: bool, out: &mut Vec<Leaf>) {
    if w <= 0.0 {
        return;
    }
    match &graph.nodes[node.0 as usize] {
        Node::Clip { clip, speed } => out.push(Leaf { clip: Some(*clip), weight: w, additive, speed: *speed }),
        Node::External => out.push(Leaf { clip: None, weight: w, additive, speed: 1.0 }),
        Node::Blend1 { param, points } => {
            let x = float_of(params, *param);
            let n = points.len();
            if n == 1 || x <= points[0].0 {
                return leaves(graph, points[0].1, params, w, additive, out);
            }
            if x >= points[n - 1].0 {
                return leaves(graph, points[n - 1].1, params, w, additive, out);
            }
            let i = points.partition_point(|p| p.0 <= x); // points[i-1].0 <= x < points[i].0
            let (x0, x1) = (points[i - 1].0, points[i].0);
            let t = (x - x0) / (x1 - x0);
            leaves(graph, points[i - 1].1, params, w * (1.0 - t), additive, out);
            leaves(graph, points[i].1, params, w * t, additive, out);
        }
        Node::Blend2 { params: ps, points } => {
            let p = [float_of(params, ps[0]), float_of(params, ps[1])];
            // Gradient band interpolation: each point's influence falls off towards every other.
            let mut sum = 0.0f32;
            let mut best = (f32::MAX, 0usize);
            let mut hs = [0.0f32; graph::MAX_POINTS];
            for (i, (pi, _)) in points.iter().enumerate() {
                let mut h = 1.0f32;
                for (j, (pj, _)) in points.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let d = [pj[0] - pi[0], pj[1] - pi[1]];
                    let l2 = d[0] * d[0] + d[1] * d[1];
                    if l2 > 0.0 {
                        let along = ((p[0] - pi[0]) * d[0] + (p[1] - pi[1]) * d[1]) / l2;
                        h = h.min(1.0 - along);
                    }
                }
                hs[i] = h.max(0.0);
                sum += hs[i];
                let dist = (p[0] - pi[0]) * (p[0] - pi[0]) + (p[1] - pi[1]) * (p[1] - pi[1]);
                if dist < best.0 {
                    best = (dist, i);
                }
            }
            if sum <= 1e-9 {
                return leaves(graph, points[best.1].1, params, w, additive, out);
            }
            for (i, (_, n)) in points.iter().enumerate() {
                leaves(graph, *n, params, w * hs[i] / sum, additive, out);
            }
        }
        Node::Additive { base, add, weight } => {
            leaves(graph, *base, params, w, additive, out);
            let k = amount(*weight, params).max(0.0);
            leaves(graph, *add, params, w * k, true, out);
        }
    }
}

/// Seconds per cycle and metres per cycle of a leaf list (weighted over its non-additive leaves).
fn cycle_of(set: &AnimSet, leaves: &[Leaf]) -> (f64, f64) {
    let (mut w, mut d, mut m) = (0.0f64, 0.0f64, 0.0f64);
    for l in leaves.iter().filter(|l| !l.additive) {
        let (dur, dist) = match l.clip {
            Some(c) => {
                let c = set.clip(c);
                (c.duration as f64 / l.speed.max(1e-3) as f64, c.distance as f64)
            }
            None => (1.0, 0.0),
        };
        w += l.weight as f64;
        d += l.weight as f64 * dur;
        m += l.weight as f64 * dist;
    }
    if w <= 0.0 {
        return (1.0, 0.0);
    }
    ((d / w).max(1e-3), m / w)
}

/// Clip time (unwrapped seconds) of a leaf at `phase` cycles.
fn leaf_time(set: &AnimSet, l: &Leaf, phase: f64) -> f64 {
    match l.clip {
        Some(c) => phase * set.clip(c).duration as f64,
        None => 0.0,
    }
}

// ---------------------------------------------------------------------------------------------
// Resolving

/// What a resolve found for one layer.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Resolved {
    layer: RLayer,
    truncated: bool,
}

/// A transition that fired while resolving (or a skip over whole periods).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fired {
    tau: f64,
    /// The layer's blend stack right after it (phases at the play's clock).
    layer: RLayer,
    /// False for a skip: the stack moved on whole periods, nothing was entered.
    entered: bool,
}

/// Per-resolve scratch and constants of one play.
struct Machine<'a> {
    set: &'a AnimSet,
    graph: &'a AnimGraph,
    scale: f32,
}

impl Machine<'_> {
    /// Cycles per graph second of state `s` of layer `li`.
    fn rate(&self, li: usize, s: u16, params: &[RVal], scratch: &mut Vec<Leaf>) -> f64 {
        let st = &self.graph.layers[li].states[s as usize];
        scratch.clear();
        leaves(self.graph, st.node, params, 1.0, false, scratch);
        let (dur, dist) = cycle_of(self.set, scratch);
        let mult = amount(st.speed, params).max(0.0) as f64;
        let mut r = mult / dur;
        if let Some(p) = st.match_speed {
            let target = float_of(params, p).max(0.0) as f64;
            let metres = dist * self.scale.max(1e-6) as f64;
            if metres > 1e-6 {
                r = mult * target / metres;
            }
        }
        r.clamp(0.0, MAX_RATE as f64 / dur)
    }

    fn conds_hold(&self, tr: &graph::Transition, params: &[RVal]) -> bool {
        tr.when.iter().all(|c| {
            let v = params[c.param as usize];
            match (v, &c.value) {
                (RVal::Num(x), CondValue::Num(y)) => match c.op {
                    Op::Eq => x == *y,
                    Op::Ne => x != *y,
                    Op::Lt => x < *y,
                    Op::Le => x <= *y,
                    Op::Gt => x > *y,
                    Op::Ge => x >= *y,
                },
                (RVal::Bool(a), CondValue::Bool(b)) => (a == *b) == (c.op == Op::Eq),
                (RVal::Enum(a), CondValue::Enum(b)) => (a == *b) == (c.op == Op::Eq),
                _ => false,
            }
        })
    }

    /// Runs layer `li` from the play's clock `t0` to `t_end` (graph seconds). `rates` holds every
    /// state's rate (cycles/s). Fired transitions go to `fired` (when given); periods are only
    /// skipped up to `skip_until`. `history` is scratch (the last firing of each transition).
    ///
    /// Two rules keep the work bounded: one instant fires at most one transition more than the
    /// layer has states (by then the machine is cycling without time passing: `a -> b when on`
    /// with `b -> a when on` stops after three firings, in `b`; transitions that would fire at
    /// that instant wait until time moves on), and a transition firing again with the same stack
    /// shifted in time marks a period, which is skipped whole periods at a time.
    #[allow(clippy::too_many_arguments)]
    fn resolve(&self, li: usize, mut layer: RLayer, params: &mut [RVal], rates: &[f64], t0: f64, t_end: f64, skip_until: f64, history: &mut Vec<Option<(f64, RLayer)>>, mut fired: Option<&mut Vec<Fired>>) -> Resolved {
        let lg = &self.graph.layers[li];
        let phase_at = |e: &REntry, t: f64| e.phase + (t - t0) * rates[e.state as usize];
        let mut t = t0;
        let mut steps = 0usize;
        // Firings at the current instant `t`, and the most one instant may have.
        let mut at_instant = 0usize;
        let instant_cap = lg.states.len() + 1;
        history.clear();
        history.resize(lg.transitions.len(), None);
        loop {
            let top = *layer.top();
            let mut best: Option<(f64, usize)> = None;
            for (ti, tr) in lg.transitions.iter().enumerate() {
                if !tr.leaves(top.state) || !self.conds_hold(tr, params) {
                    continue;
                }
                let tf = match tr.exit {
                    None => Some(t),
                    Some(x) => {
                        let r = rates[top.state as usize];
                        let p_now = phase_at(&top, t);
                        let looping = self.graph.node_loops[lg.states[top.state as usize].node.0 as usize];
                        if p_now >= x && !looping {
                            Some(t)
                        } else if r <= 0.0 {
                            None
                        } else {
                            let target = if p_now <= x { x } else { x + dmath::ceil_f64(p_now - x) };
                            Some((t0 + (target - top.phase) / r).max(t))
                        }
                    }
                };
                // An instant that has used up its firings lets time move on first.
                let tf = tf.filter(|&tf| tf > t || at_instant < instant_cap);
                if let Some(tf) = tf {
                    if best.is_none_or(|(b, _)| tf < b) {
                        best = Some((tf, ti));
                    }
                }
            }
            let Some((tf, ti)) = best else { break };
            if tf > t_end {
                break;
            }
            steps += 1;
            if steps > MAX_STEPS {
                return Resolved { layer, truncated: true };
            }
            let tr = &lg.transitions[ti];
            // Triggers it tested are consumed.
            let mut consumed = false;
            for c in &tr.when {
                if matches!(self.graph.params[c.param as usize].kind, ParamKind::Trigger) {
                    params[c.param as usize] = RVal::Bool(false);
                    consumed = true;
                }
            }
            let start = if tr.keep_phase { dmath::fract_f64(phase_at(&top, tf)) } else { 0.0 };
            let entry = REntry { state: tr.to, phase: start - (tf - t0) * rates[tr.to as usize], fade: (tr.blend > 0.0).then_some((tf, tr.blend)) };
            if tr.blend <= 0.0 {
                layer = RLayer::single(entry.state, entry.phase);
            } else {
                // Entries under one that is fully in by now are hidden for good.
                if let Some(k) = (0..layer.len).rev().find(|&k| fade_weight(layer.stack[k].fade, tf) >= 1.0) {
                    layer.stack.copy_within(k..layer.len, 0);
                    layer.len -= k;
                }
                if layer.len == MAX_STACK {
                    layer.stack.copy_within(1..MAX_STACK, 0);
                    layer.len -= 1;
                }
                layer.stack[layer.len] = entry;
                layer.len += 1;
            }
            if let Some(f) = fired.as_deref_mut() {
                f.push(Fired { tau: tf, layer, entered: true });
            }
            if tf > t {
                at_instant = 0;
            }
            at_instant += 1;
            t = tf;
            if consumed {
                history.iter_mut().for_each(|h| *h = None);
                continue;
            }
            // A periodic chain: this transition fired before with the same stack, shifted in time.
            if let Some((t_prev, prev)) = history[ti] {
                let period = tf - t_prev;
                if period > 1e-6 && same_shifted(&prev, &layer, period, rates) {
                    let room = skip_until.min(t_end) - tf;
                    let k = dmath::floor_f64(room / period) - 1.0;
                    if k >= 1.0 {
                        let shift = k * period;
                        for e in &mut layer.stack[..layer.len] {
                            e.phase -= shift * rates[e.state as usize];
                            if let Some((s, _)) = &mut e.fade {
                                *s += shift;
                            }
                        }
                        t = tf + shift;
                        at_instant = 0;
                        history.iter_mut().for_each(|h| *h = None);
                        if let Some(f) = fired.as_deref_mut() {
                            f.push(Fired { tau: t, layer, entered: false });
                        }
                        continue;
                    }
                }
            }
            history[ti] = Some((tf, layer));
        }
        // What lies under an entry that is fully in at `t_end` doesn't show.
        if let Some(k) = (0..layer.len).rev().find(|&k| fade_weight(layer.stack[k].fade, t_end) >= 1.0) {
            layer.stack.copy_within(k..layer.len, 0);
            layer.len -= k;
        }
        Resolved { layer, truncated: false }
    }
}

/// `b` is `a` shifted later by `period` (same states, phases at the shifted times, fades).
fn same_shifted(a: &RLayer, b: &RLayer, period: f64, rates: &[f64]) -> bool {
    a.len == b.len
        && a.entries().iter().zip(b.entries()).all(|(x, y)| {
            let r = rates[x.state as usize];
            // y's phase at t + period equals x's phase at t.
            x.state == y.state
                && (y.phase + period * r - x.phase).abs() <= 1e-6 * (1.0 + x.phase.abs())
                && match (x.fade, y.fade) {
                    (None, None) => true,
                    (Some((s0, d0)), Some((s1, d1))) => d0 == d1 && (s1 - period - s0).abs() <= 1e-6 * (1.0 + s0.abs()),
                    _ => false,
                }
        })
}

// ---------------------------------------------------------------------------------------------
// Public queries

/// Everything a resolve needs: the set, the graph and the entity's uniform scale (world metres
/// per model metre, for `match_speed`).
#[derive(Clone, Copy)]
pub struct Graphed<'a> {
    pub set: &'a AnimSet,
    pub graph: &'a AnimGraph,
    pub scale: f32,
}

/// Scratch for resolving (rates, leaves, parameters, layers): reuse it, nothing allocates once warm.
#[derive(Clone, Debug, Default)]
pub struct Scratch {
    params: Vec<RVal>,
    rates: Vec<Vec<f64>>,
    leaves: Vec<Leaf>,
    layers: Vec<Resolved>,
    fired: Vec<Fired>,
    history: Vec<Option<(f64, RLayer)>>,
}

impl Graphed<'_> {
    fn machine(&self) -> Machine<'_> {
        Machine { set: self.set, graph: self.graph, scale: self.scale }
    }

    /// Resolves every layer of `play` at graph time `tau` into `s.layers` (parameters with the
    /// triggers that fired consumed in `s.params`).
    fn resolve_all(&self, play: &GraphPlay, tau: f64, skip_until: f64, s: &mut Scratch, mut fired: Option<(usize, &mut Vec<Fired>)>) {
        bind_params(self.graph, play, &mut s.params);
        let m = self.machine();
        s.rates.resize_with(self.graph.layers.len(), Vec::new);
        s.layers.clear();
        for li in 0..self.graph.layers.len() {
            let mut rates = std::mem::take(&mut s.rates[li]);
            rates.clear();
            for st in 0..self.graph.layers[li].states.len() {
                rates.push(m.rate(li, st as u16, &s.params, &mut s.leaves));
            }
            let layer = bind_layer(self.graph, li, play);
            let f = match &mut fired {
                Some((l, f)) if *l == li => Some(&mut **f),
                _ => None,
            };
            let r = m.resolve(li, layer, &mut s.params, &rates, play.clock, tau, skip_until, &mut s.history, f);
            s.layers.push(r);
            s.rates[li] = rates;
        }
    }
}

/// What [`settle`] produced besides the new play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settled {
    /// A layer hit [`MAX_STEPS`] (a transition loop at one instant): its state is where it stopped.
    pub truncated: bool,
}

/// The play as of world time `now`: anchored there (its clock = graph time at `now`), blend
/// stacks resolved and re-phased, fully-faded entries dropped, fired triggers consumed. Evaluating
/// the result from `now` on gives exactly what `play` gives, so a parameter write can settle and
/// then replace values without a pop.
pub fn settle(g: Graphed, play: &GraphPlay, timing: Timing, now: f64, s: &mut Scratch) -> (GraphPlay, Settled) {
    let tau = timing.tau(play.clock, now);
    g.resolve_all(play, tau, tau, s, None);
    let mut out = GraphPlay { graph: play.graph.clone(), params: Vec::with_capacity(play.params.len()), clock: tau, layers: Vec::with_capacity(g.graph.layers.len()) };
    // Keep the play's own parameter values (unknown names too), with fired triggers cleared.
    for (n, v) in &play.params {
        let v = match (g.graph.param(n), v) {
            (Some(i), Value::Bool(true)) if matches!(g.graph.params[i as usize].kind, ParamKind::Trigger) && s.params[i as usize] == RVal::Bool(false) => Value::Bool(false),
            _ => v.clone(),
        };
        out.params.push((n.clone(), v));
    }
    let mut settled = Settled::default();
    for (li, r) in s.layers.iter().enumerate() {
        settled.truncated |= r.truncated;
        let lg = &g.graph.layers[li];
        let rates = &s.rates[li];
        let mut entries: Vec<Entry> = Vec::with_capacity(r.layer.len);
        let start = (0..r.layer.len).rev().find(|&k| fade_weight(r.layer.stack[k].fade, tau) >= 1.0).unwrap_or(0);
        for e in &r.layer.entries()[start..] {
            let phase = e.phase + (tau - play.clock) * rates[e.state as usize];
            let fade = e.fade.filter(|_| fade_weight(e.fade, tau) < 1.0);
            entries.push(Entry { state: lg.states[e.state as usize].name.clone(), phase, fade });
        }
        out.layers.push(LayerPlay { layer: lg.name.clone(), stack: entries });
    }
    (out, settled)
}

/// One state of a layer's blend stack at a moment.
#[derive(Clone, Debug, PartialEq)]
pub struct StateNow {
    pub state: String,
    /// Its crossfade weight (1 = fully in).
    pub fade: f32,
    /// Cycles since entering.
    pub phase: f64,
    /// Cycles per graph second.
    pub rate: f64,
    /// Clips it plays with their blend-tree shares.
    pub clips: Vec<(String, f32)>,
}

/// A layer at a moment: its blend stack, top (current state) last.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerNow {
    pub layer: String,
    pub weight: f32,
    pub stack: Vec<StateNow>,
    pub truncated: bool,
}

/// What a play is doing at world time `t` (for tools and tests).
pub fn state_at(g: Graphed, play: &GraphPlay, timing: Timing, t: f64, s: &mut Scratch) -> Vec<LayerNow> {
    let tau = timing.tau(play.clock, t);
    g.resolve_all(play, tau, tau, s, None);
    let mut out = Vec::new();
    for (li, r) in s.layers.iter().enumerate() {
        let lg = &g.graph.layers[li];
        let mut stack = Vec::new();
        for e in r.layer.entries() {
            let st = &lg.states[e.state as usize];
            let rate = s.rates[li][e.state as usize];
            let mut lv = Vec::new();
            leaves(g.graph, st.node, &s.params, 1.0, lg.mode == LayerMode::Additive, &mut lv);
            let clips = lv.iter().map(|l| (l.clip.map_or("(external)".to_string(), |c| g.set.clip(c).name.clone()), l.weight)).collect();
            stack.push(StateNow { state: st.name.clone(), fade: fade_weight(e.fade, tau), phase: e.phase + (tau - play.clock) * rate, rate, clips });
        }
        out.push(LayerNow { layer: lg.name.clone(), weight: amount(lg.weight, &s.params).clamp(0.0, 1.0), stack, truncated: r.truncated });
    }
    out
}

/// An event a play crossed.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphEvent {
    /// World time it happened.
    pub time: f64,
    pub layer: String,
    pub state: String,
    pub name: String,
    pub data: String,
}

/// Events crossed between world times `t0` and `t1` (`t0 ≤ t1`, at most [`MAX_WINDOW_S`] graph
/// seconds apart): each layer's current state's clip events (from the blend tree's strongest
/// clip) and states' `on_enter` events, in time order, at most [`MAX_EVENTS`]. Returns whether
/// some were dropped. Deterministic.
pub fn events(g: Graphed, play: &GraphPlay, timing: Timing, t0: f64, t1: f64, s: &mut Scratch, out: &mut Vec<GraphEvent>) -> bool {
    out.clear();
    let (tau0, tau1) = (timing.tau(play.clock, t0), timing.tau(play.clock, t1));
    let speed = timing.speed as f64;
    if tau1.is_nan() || tau0.is_nan() || tau1 <= tau0 || speed <= 0.0 {
        return false;
    }
    let tau1 = tau1.min(tau0 + MAX_WINDOW_S);
    let world = |tau: f64| timing.anchor + (tau - play.clock) / speed;
    let mut dropped = false;
    let mut scan = Vec::new();
    for li in 0..g.graph.layers.len() {
        let mut fired = std::mem::take(&mut s.fired);
        fired.clear();
        g.resolve_all(play, tau1, tau0, s, Some((li, &mut fired)));
        let lg = &g.graph.layers[li];
        let rates = s.rates[li].clone();
        // Segments of the timeline: the state on top from one firing to the next.
        let initial = bind_layer(g.graph, li, play);
        let mut cur = *initial.top();
        let mut seg_start = play.clock;
        let mut seg = |state_e: REntry, a: f64, b: f64, s: &mut Scratch, out: &mut Vec<GraphEvent>, dropped: &mut bool| {
            let (a, b) = (a.max(tau0), b.min(tau1));
            if b <= a {
                return;
            }
            let st = &lg.states[state_e.state as usize];
            s.leaves.clear();
            leaves(g.graph, st.node, &s.params, 1.0, false, &mut s.leaves);
            let Some(best) = s.leaves.iter().filter(|l| l.clip.is_some() && !l.additive).fold(None::<Leaf>, |acc, l| match acc {
                Some(b) if b.weight >= l.weight => Some(b),
                _ => Some(*l),
            }) else {
                return;
            };
            let clip = g.set.clip(best.clip.expect("filtered"));
            let r = rates[state_e.state as usize];
            let pa = state_e.phase + (a - play.clock) * r;
            let pb = state_e.phase + (b - play.clock) * r;
            scan.clear();
            *dropped |= clip.events_between(pa * clip.duration as f64, pb * clip.duration as f64, &mut scan);
            let d = clip.duration as f64;
            for &(ei, cycle) in &scan {
                let e = &clip.events[ei as usize];
                // The unwrapped clip time of the occurrence (ping-pong cycles run backwards), back
                // to graph time.
                let backward = clip.looping == LoopMode::PingPong && cycle % 2 == 1;
                let occurrence = cycle as f64 * d + if backward { d - e.time as f64 } else { e.time as f64 };
                let tau = if r > 0.0 { play.clock + (occurrence / d - state_e.phase) / r } else { a };
                if out.len() >= MAX_EVENTS {
                    *dropped = true;
                    return;
                }
                out.push(GraphEvent { time: world(tau.clamp(a, b)), layer: lg.name.clone(), state: st.name.clone(), name: e.name.clone(), data: e.data.clone() });
            }
        };
        let mut f_iter = fired.iter().peekable();
        loop {
            let next = f_iter.peek().map(|f| f.tau);
            let end = next.unwrap_or(tau1);
            seg(cur, seg_start, end, s, out, &mut dropped);
            let Some(f) = f_iter.next() else { break };
            let top = *f.layer.top();
            let st = &lg.states[top.state as usize];
            if let (Some(name), true) = (&st.on_enter, f.entered) {
                if f.tau >= tau0 && f.tau < tau1 {
                    if out.len() >= MAX_EVENTS {
                        dropped = true;
                    } else {
                        out.push(GraphEvent { time: world(f.tau), layer: lg.name.clone(), state: st.name.clone(), name: name.clone(), data: String::new() });
                    }
                }
            }
            cur = top;
            seg_start = f.tau;
        }
        s.fired = fired;
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    dropped
}

/// Most integration steps [`root_motion`] takes over one window (longer windows take longer
/// steps; each step's root delta is exact, only crossfade weights are sampled per step).
pub const MAX_ROOT_STEPS: usize = 4096;

/// Root translation (model space, metres) the base layer's root-motion states moved between
/// world times `t0` and `t1` (at most [`MAX_WINDOW_S`] graph seconds). One resolve over the
/// window gives the blend stack between transitions; the motion is integrated along it in steps
/// of 1/120 graph second (at most [`MAX_ROOT_STEPS`]) with each state's crossfade weight.
/// Deterministic: the same play and times give the same bits on every machine. A simulation
/// should ask for whole ticks (`[tick, tick + 1]`): splitting a window differently gives sums
/// equal to rounding, not bits.
pub fn root_motion(g: Graphed, play: &GraphPlay, timing: Timing, t0: f64, t1: f64, s: &mut Scratch) -> V3 {
    let (tau0, tau1) = (timing.tau(play.clock, t0), timing.tau(play.clock, t1));
    if tau1.is_nan() || tau0.is_nan() || tau1 <= tau0 {
        return [0.0; 3];
    }
    let tau1 = tau1.min(tau0 + MAX_WINDOW_S);
    let mut fired = std::mem::take(&mut s.fired);
    fired.clear();
    g.resolve_all(play, tau1, tau0, s, Some((0, &mut fired)));
    let lg = &g.graph.layers[0];
    let step = ((tau1 - tau0) / MAX_ROOT_STEPS as f64).max(1.0 / 120.0);
    let mut acc = [0.0f64; 3];
    let mut lv = std::mem::take(&mut s.leaves);
    // The stack in force from `seg_start` until the next firing.
    let mut stack = bind_layer(g.graph, 0, play);
    let mut seg_start = play.clock;
    let mut next = 0usize;
    loop {
        let seg_end = fired.get(next).map_or(tau1, |f| f.tau);
        let (a0, b0) = (seg_start.max(tau0), seg_end.min(tau1));
        if b0 > a0 {
            let n = dmath::ceil_f64((b0 - a0) / step).max(1.0) as usize;
            for k in 0..n {
                let a = a0 + (b0 - a0) * k as f64 / n as f64;
                let b = if k + 1 == n { b0 } else { a0 + (b0 - a0) * (k + 1) as f64 / n as f64 };
                let mid = 0.5 * (a + b);
                // Crossfade weights: each entry over what is under it.
                let mut ws = [0.0f32; MAX_STACK];
                let mut remaining = 1.0f32;
                for i in (0..stack.len).rev() {
                    let w = if i == 0 { 1.0 } else { fade_weight(stack.stack[i].fade, mid) };
                    ws[i] = remaining * w;
                    remaining *= 1.0 - w;
                }
                for (i, e) in stack.entries().iter().enumerate() {
                    let st = &lg.states[e.state as usize];
                    if st.root_motion == RootMotion::Off || ws[i] <= 0.0 {
                        continue;
                    }
                    let rate = s.rates[0][e.state as usize];
                    let (pa, pb) = (e.phase + (a - play.clock) * rate, e.phase + (b - play.clock) * rate);
                    lv.clear();
                    leaves(g.graph, st.node, &s.params, 1.0, false, &mut lv);
                    let total: f32 = lv.iter().filter(|l| !l.additive).map(|l| l.weight).sum();
                    for l in lv.iter().filter(|l| !l.additive) {
                        let Some(c) = l.clip else { continue };
                        let d = root_delta(g.set, c, pa, pb);
                        let w = ws[i] as f64 * (l.weight / total.max(1e-9)) as f64;
                        for j in 0..3 {
                            if st.root_motion == RootMotion::Horizontal && j == 1 {
                                continue;
                            }
                            acc[j] += w * d[j];
                        }
                    }
                }
            }
        }
        let Some(f) = fired.get(next) else { break };
        stack = f.layer;
        seg_start = f.tau;
        next += 1;
    }
    s.leaves = lv;
    s.fired = fired;
    [acc[0] as f32, acc[1] as f32, acc[2] as f32]
}

/// The root bone's translation track moved between phases `pa` and `pb` (cycles; looping clips
/// accumulate one cycle's displacement per wrap).
fn root_delta(set: &AnimSet, c: ClipId, pa: f64, pb: f64) -> [f64; 3] {
    let b = set.bound(c);
    let Some(ti) = b.root_t else { return [0.0; 3] };
    let clip = &b.clip;
    debug_assert_eq!(clip.tracks[ti].channel, Channel::Translation);
    let keys = &clip.tracks[ti].keys;
    let d = clip.duration as f64;
    let at = |p: f64| -> [f64; 3] {
        let u = p.max(0.0) * d;
        let v = keys.sample(clip.local_time(u), false, clip.wrap());
        let mut out = [v[0] as f64, v[1] as f64, v[2] as f64];
        if clip.looping == LoopMode::Loop {
            let k = dmath::floor_f64(p.max(0.0));
            let (end, start) = (keys.sample(clip.duration, false, None), keys.sample(0.0, false, None));
            for i in 0..3 {
                out[i] += k * (end[i] - start[i]) as f64;
            }
        }
        out
    };
    let (x, y) = (at(pa), at(pb));
    [y[0] - x[0], y[1] - x[1], y[2] - x[2]]
}

// ---------------------------------------------------------------------------------------------
// Evaluating poses

/// The world around an evaluation (all optional: a preview has none of it).
#[derive(Clone, Copy, Default)]
pub struct Surroundings<'a> {
    /// Model → world (the entity's transform): IK targets and the ground are in world space.
    pub to_world: Option<Affine>,
    /// What feet stand on.
    pub ground: Option<&'a dyn Ground>,
    /// The pose `external` nodes play (a ragdoll's, from physics; A4). `None`: the rest pose.
    pub external: Option<&'a Pose>,
}

/// What the IK did in the last evaluation (for tools).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IkReport {
    /// Per solver: name, effective weight, and for feet the result.
    pub solvers: Vec<(String, f32, Option<ik::FeetResult>)>,
}

/// A reusable evaluator: one per animated entity (or a pool), holding scratch poses.
#[derive(Clone, Debug, Default)]
pub struct GraphPlayer {
    scratch: Scratch,
    pose: Pose,
    layer_pose: Pose,
    entry_pose: Pose,
    tmp: Pose,
    model: Vec<Affine>,
    lv: Vec<Leaf>,
    ik_weights: Vec<f32>,
    /// Filled when `report_ik` is on.
    pub ik: IkReport,
    pub report_ik: bool,
}

impl GraphPlayer {
    pub fn new() -> GraphPlayer {
        GraphPlayer::default()
    }

    /// The local pose of the last evaluation.
    pub fn pose(&self) -> &Pose {
        &self.pose
    }

    /// Model-space bone transforms of the last evaluation (after IK).
    pub fn model(&self) -> &[Affine] {
        &self.model
    }

    /// Samples one blend-stack entry into `self.entry_pose`.
    fn sample_entry(&mut self, g: &Graphed, li: usize, e: &REntry, tau: f64, clock: f64, around: &Surroundings) {
        let skel = &g.set.skeleton;
        let lg = &g.graph.layers[li];
        let st = &lg.states[e.state as usize];
        let rate = self.scratch.rates[li][e.state as usize];
        let phase = e.phase + (tau - clock) * rate;
        let layer_additive = lg.mode == LayerMode::Additive;
        self.lv.clear();
        leaves(g.graph, st.node, &self.scratch.params, 1.0, layer_additive, &mut self.lv);
        // Start from rest (or no change, for additive layers).
        let base = if layer_additive { Base::Delta } else { Base::Rest };
        self.entry_pose.local.clear();
        match base {
            Base::Rest => self.entry_pose.local.extend_from_slice(&skel.rest),
            Base::Delta => self.entry_pose.local.resize(skel.len(), crate::math::Transform::IDENTITY),
        }
        let mut acc_w = 0.0f32;
        for k in 0..self.lv.len() {
            let l = self.lv[k];
            let leaf_base = if l.additive { Base::Delta } else { base };
            match l.clip {
                Some(c) => {
                    let b = g.set.bound(c);
                    let u = leaf_time(g.set, &l, phase);
                    blend::sample(skel, &b.clip, &b.bones, b.clip.local_time(u), leaf_base, &mut self.tmp);
                }
                None => {
                    self.tmp.local.clear();
                    match around.external {
                        Some(p) if p.local.len() == skel.len() => self.tmp.local.extend_from_slice(&p.local),
                        _ => self.tmp.local.extend_from_slice(&skel.rest),
                    }
                }
            }
            if l.additive && !layer_additive {
                blend::add(&mut self.entry_pose, &self.tmp, l.weight, None);
            } else {
                acc_w += l.weight;
                let w = if acc_w > 0.0 { l.weight / acc_w } else { 0.0 };
                blend::blend(&mut self.entry_pose, &self.tmp, w, None);
            }
        }
        match st.root_motion {
            RootMotion::Off => {}
            RootMotion::Horizontal if !layer_additive => {
                self.entry_pose.local[0].t[0] = skel.rest[0].t[0];
                self.entry_pose.local[0].t[2] = skel.rest[0].t[2];
            }
            RootMotion::Full if !layer_additive => self.entry_pose.local[0].t = skel.rest[0].t,
            _ => {}
        }
    }

    /// Evaluates `play` at world time `t`: layers, then IK. Returns the local pose; its
    /// model-space transforms are in [`GraphPlayer::model`].
    pub fn evaluate(&mut self, g: Graphed, play: &GraphPlay, timing: Timing, t: f64, around: &Surroundings) -> &Pose {
        let tau = timing.tau(play.clock, t);
        let mut scratch = std::mem::take(&mut self.scratch);
        g.resolve_all(play, tau, tau, &mut scratch, None);
        self.scratch = scratch;
        let skel = &g.set.skeleton;
        self.ik_weights.clear();
        self.ik_weights.resize(g.graph.ik.len(), 0.0);
        for li in 0..g.graph.layers.len() {
            let r = self.scratch.layers[li];
            let lg = &g.graph.layers[li];
            // The layer's stack, bottom up, each entry crossfaded over what is below.
            for k in 0..r.layer.len {
                let e = r.layer.stack[k];
                self.sample_entry(&g, li, &e, tau, play.clock, around);
                let w = if k == 0 { 1.0 } else { fade_weight(e.fade, tau) };
                if k == 0 {
                    std::mem::swap(&mut self.layer_pose, &mut self.entry_pose);
                } else {
                    blend::blend(&mut self.layer_pose, &self.entry_pose, w, None);
                }
                if li == 0 {
                    // IK weights follow the base layer's states through their crossfades.
                    let st = &lg.states[e.state as usize];
                    for (i, iw) in self.ik_weights.iter_mut().enumerate() {
                        let sw = st.ik.iter().find(|(k, _)| *k as usize == i).map_or(1.0, |(_, w)| *w);
                        *iw = if k == 0 { sw } else { *iw + (sw - *iw) * w };
                    }
                }
            }
            if li == 0 {
                std::mem::swap(&mut self.pose, &mut self.layer_pose);
                continue;
            }
            let lw = amount(lg.weight, &self.scratch.params).clamp(0.0, 1.0);
            let mask = lg.mask.map(|m| &g.set.mask(m).weights[..]);
            match lg.mode {
                LayerMode::Override => blend::blend(&mut self.pose, &self.layer_pose, lw, mask),
                LayerMode::Additive => blend::add(&mut self.pose, &self.layer_pose, lw, mask),
            }
        }
        skel.model_space(&self.pose, &mut self.model);
        self.run_ik(&g, around);
        &self.pose
    }

    fn run_ik(&mut self, g: &Graphed, around: &Surroundings) {
        let skel = &g.set.skeleton;
        if self.report_ik {
            self.ik.solvers.clear();
        }
        let to_world = around.to_world.unwrap_or(Affine::IDENTITY);
        let from_world = crate::math::inverse(&to_world).unwrap_or(Affine::IDENTITY);
        for (i, solver) in g.graph.ik.iter().enumerate() {
            let w = (amount(solver.weight, &self.scratch.params) * self.ik_weights[i]).clamp(0.0, 1.0);
            let mut feet_res = None;
            if w > 0.0 {
                match &solver.solver {
                    Solver::Feet(feet) => {
                        if let Some(ground) = around.ground {
                            feet_res = Some(ik::plant_feet(skel, &mut self.pose, &mut self.model, feet, &to_world, ground, w));
                        }
                    }
                    Solver::TwoBone { chain, target, keep_end } => {
                        if let RVal::Vec3(Some(p)) = self.scratch.params[*target as usize] {
                            ik::two_bone(skel, &mut self.pose, &mut self.model, chain, from_world.apply_point(p), w, *keep_end);
                        }
                    }
                    Solver::LookAt { spec, target } => {
                        if let RVal::Vec3(Some(p)) = self.scratch.params[*target as usize] {
                            ik::look_at(skel, &mut self.pose, &mut self.model, spec, from_world.apply_point(p), w);
                        }
                    }
                }
            }
            if self.report_ik {
                self.ik.solvers.push((solver.name.clone(), w, feet_res));
            }
        }
    }
}

/// A hash of the poses a play gives at `samples` evenly spaced times over `[t0, t1]` (exact
/// float bits; for determinism checks).
pub fn pose_hash(g: Graphed, play: &GraphPlay, timing: Timing, t0: f64, t1: f64, samples: usize) -> u64 {
    let mut p = GraphPlayer::new();
    let mut h = StableHasher::new();
    let n = samples.clamp(1, 100_000);
    for k in 0..=n {
        let t = t0 + (t1 - t0) * k as f64 / n as f64;
        let pose = p.evaluate(g, play, timing, t, &Surroundings::default());
        for tr in &pose.local {
            tr.t.iter().chain(tr.r.iter()).chain(tr.s.iter()).for_each(|v| h.write_f32(*v));
        }
    }
    h.finish()
}

/// Parameter values from a JSON object (`{"speed": 1.2, "act": "move"}`), checked against the
/// graph (names, types, enum values): for tools that write parameters.
pub fn params_from_json(graph: &AnimGraph, v: &Json, path: &str) -> Result<Vec<(String, Value)>, AnimError> {
    let pairs = v.as_object().ok_or_else(|| err(path, AnimErrorCode::WrongType, "parameters are an object: {name: value}"))?;
    let mut out: Vec<(String, Value)> = Vec::new();
    for (n, pv) in pairs {
        let p = format!("{path}.{n}");
        let i = graph.param(n).ok_or_else(|| err(&p, AnimErrorCode::UnknownName, format!("no parameter '{n}' (params: {})", graph.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "))))?;
        let val = Value::from_json(pv, &p)?;
        let val = graph.params[i as usize].accept(&val).map_err(|m| err(&p, AnimErrorCode::WrongType, m))?;
        out.push((n.clone(), val));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
