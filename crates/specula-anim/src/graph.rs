//! Animation graphs as data (stage A3): parameters, blend trees, state machines in layers,
//! transitions with conditions and blend times, IK and look-at, joint limits. Game code (or an
//! agent) only sets parameters; the graph decides what plays.
//!
//! ```text
//! { "graph": 1,
//!   "params": { "act": { "type": "enum", "values": ["idle", "move", "attack", "death"] },
//!               "speed": { "type": "float", "min": 0, "max": 10 },          // m/s in the world
//!               "hit": "trigger", "look_at": "vec3", "look": { "type": "float", "default": 0 } },
//!   "nodes": { "locomotion": { "blend1d": "speed", "points": [[0.3, "shuffle"], [0.8, "walk"]] } },
//!   "states": {
//!     "idle":   "idle",                                                    // play a clip
//!     "move":   { "play": "locomotion", "match_speed": "speed" },          // feet keep up with `speed`
//!     "attack": { "play": "attack", "speed": 1.2 },
//!     "death":  { "play": "death", "ik": { "feet": 0 } } },
//!   "entry": "idle",
//!   "transitions": [
//!     { "from": "*", "to": "death", "when": "act == death", "blend": 0.15 },
//!     { "from": ["idle", "attack"], "to": "move", "when": "act == move", "blend": 0.25 },
//!     { "from": "attack", "to": "idle", "exit": "end", "blend": 0.2 } ],
//!   "layers": [ { "name": "flinch", "mode": "additive", "mask": "upper",          // more layers
//!                 "states": { "none": { "play": "rest" }, "hit": "recoil" },
//!                 "transitions": [ { "from": "none", "to": "hit", "when": "hit" },
//!                                  { "from": "hit", "to": "none", "exit": "end" } ] } ],
//!   "ik": [ { "name": "feet", "kind": "feet", "pelvis": "hips",
//!             "legs": [["thigh_l", "shin_l", "foot_l"], ["thigh_r", "shin_r", "foot_r"]] },
//!           { "name": "look", "kind": "look_at", "bones": ["chest", "head"], "target": "look_at",
//!             "weight": "look", "limit": 70 } ],
//!   "limits": { "shin_l": { "x": [-5, 160] } } }
//! ```
//!
//! - **Parameters** are `float` (`min`, `max`, `default`), `bool`, `trigger` (true until a
//!   transition that tests it fires, then false), `enum` (`values`) or `vec3` (IK and look-at
//!   targets, world space). Unknown parameters in a play are reported, not guessed.
//! - **Nodes** (blend trees): a clip name; `{clip, speed}`; `{blend1d: param, points: [[x, node]…]}`
//!   (linear between neighbours); `{blend2d: [px, py], points: [[[x, y], node]…]}` (gradient band
//!   interpolation, Johansen 2009, as in Unity's freeform blend spaces); `{base, add, weight}`
//!   (an additive clip on top of a node); `{external: true}`, the pose a caller supplies (the
//!   seam for ragdolls, stage A4: physics drives it, the graph blends to and from it like any
//!   state). Names in `nodes` can be reused anywhere a node goes. A blend tree's children play in
//!   step (synchronised by normalised phase), so a walk and a run blend without feet crossing.
//! - **States** play a node, at `speed` (number or float param) or, with `match_speed`, at the
//!   rate that keeps the feet planted at that parameter's ground speed (the clips' `distance`).
//!   `root_motion` (`off`, `horizontal`, `full`) moves the entity instead of the pose (see
//!   [`crate::play::root_motion`]); `ik` sets this state's weight for named IK solvers (default 1);
//!   `on_enter` names an event fired on entry.
//! - **Transitions** are tried in order; the first that can fire does. `from`: a state, a list or
//!   `"*"` (any other state). `when`: conditions, all true (`param`, `!param`, `param op value`
//!   with `== != < <= > >=`, joined by `and` or given as a list). `exit`: the state's phase (in
//!   cycles since it was entered, `"end"` = 1) from which it may fire; a looping state waits for
//!   the next time its cycle reaches that point. `blend`: crossfade seconds (smoothstep).
//!   `keep_phase`: the new state starts at the old one's phase in its cycle (walk ↔ run).
//! - **Layers**: the first is the base (the top-level `states`, `entry`, `transitions`); more
//!   `layers` each run their own state machine and apply on top, `override` or `additive`, with a
//!   `weight` and a `mask` of the anim set.
//! - **IK** runs after the layers, in order: `feet` (planted on the ground the engine finds under
//!   them, pelvis following), `two_bone` (an arm or leg reaching a `vec3` target: hands on
//!   handles), `look_at`. Each has a `weight` (number or float param), times its state weights.
//! - **Limits** (Euler degrees about the bone's own axes, rest-relative) and `feet` (the bones
//!   the motion checks treat as feet; default: the `feet` IK legs' ends) are for checks.
//!
//! Strict like every anim format: unknown fields, unknown names and impossible values are
//! errors with a path (`transitions[2].when[0]`) and an [`AnimErrorCode`].

use specula_core::Json;

use crate::animator::{LayerMode, RootMotion};
use crate::error::{err, is_ident, num, vecn, AnimError, AnimErrorCode, Obj};
use crate::ik::{Chain, Feet, LookAt};
use crate::set::{AnimSet, ClipId, MaskId};

/// The graph format version this crate reads.
pub const FORMAT_VERSION: u32 = 1;
pub const MAX_PARAMS: usize = 64;
pub const MAX_LAYERS: usize = 4;
pub const MAX_STATES: usize = 64;
pub const MAX_TRANSITIONS: usize = 256;
pub const MAX_NODES: usize = 256;
/// Deepest nesting of blend-tree nodes (counted through named nodes, however they are reused).
pub const MAX_NODE_DEPTH: usize = 8;
/// Most clips one node may blend, counting a named node once per use (a node reused twice
/// under another plays twice): bounds the work of every pose.
pub const MAX_LEAVES: u64 = 64;
pub const MAX_POINTS: usize = 16;
pub const MAX_CONDITIONS: usize = 8;
pub const MAX_IK: usize = 8;
pub const MAX_LIMITS: usize = 128;
/// Longest parameter, state, layer and node name.
pub const MAX_NAME: usize = 64;
/// Longest enum parameter value list.
pub const MAX_ENUM_VALUES: usize = 32;

/// Index of a node in [`AnimGraph::nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub u16);

/// A parameter value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f32),
    Bool(bool),
    /// An enum parameter's value.
    Name(String),
    Vec3([f32; 3]),
}

impl Value {
    pub fn to_json(&self) -> Json {
        match self {
            Value::Num(x) => Json::Num(*x as f64),
            Value::Bool(b) => Json::Bool(*b),
            Value::Name(s) => Json::Str(s.clone()),
            Value::Vec3(v) => Json::arr(v.iter().map(|x| Json::Num(*x as f64))),
        }
    }

    /// From JSON: a number, a boolean, a name or `[x, y, z]`.
    pub fn from_json(v: &Json, path: &str) -> Result<Value, AnimError> {
        match v {
            Json::Num(_) => Ok(Value::Num(num(v, path, -1e9, 1e9)?)),
            Json::Bool(b) => Ok(Value::Bool(*b)),
            Json::Str(s) if s.len() <= MAX_NAME && is_ident(s) => Ok(Value::Name(s.clone())),
            Json::Arr(_) => Ok(Value::Vec3(vecn(v, path, -1e6, 1e6)?)),
            _ => Err(err(path, AnimErrorCode::WrongType, "a parameter value is a number, true/false, an enum name or [x, y, z]")),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "float",
            Value::Bool(_) => "bool",
            Value::Name(_) => "enum",
            Value::Vec3(_) => "vec3",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    Float { min: f32, max: f32 },
    Bool,
    Trigger,
    Enum(Vec<String>),
    Vec3,
}

impl ParamKind {
    pub fn name(&self) -> &'static str {
        match self {
            ParamKind::Float { .. } => "float",
            ParamKind::Bool => "bool",
            ParamKind::Trigger => "trigger",
            ParamKind::Enum(_) => "enum",
            ParamKind::Vec3 => "vec3",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
    /// `None` for a `vec3` with no default (an unset target: its solver stays off).
    pub default: Option<Value>,
}

impl Param {
    /// `v` as this parameter takes it (floats clamped to the range), or why it can't be.
    pub fn accept(&self, v: &Value) -> Result<Value, String> {
        match (&self.kind, v) {
            (ParamKind::Float { min, max }, Value::Num(x)) => Ok(Value::Num(x.clamp(*min, *max))),
            (ParamKind::Bool | ParamKind::Trigger, Value::Bool(b)) => Ok(Value::Bool(*b)),
            (ParamKind::Enum(values), Value::Name(n)) if values.contains(n) => Ok(v.clone()),
            (ParamKind::Enum(values), Value::Name(n)) => Err(format!("'{n}' is not a value of '{}' ({})", self.name, values.join(", "))),
            (ParamKind::Vec3, Value::Vec3(_)) => Ok(v.clone()),
            (k, v) => Err(format!("'{}' is a {} parameter, not {} ({})", self.name, k.name(), v.type_name(), v.to_json())),
        }
    }
}

/// A number from a constant or a float parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    Const(f32),
    /// Parameter index and a scale.
    Param(u16, f32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Clip { clip: ClipId, speed: f32 },
    /// Points sorted by `x`.
    Blend1 { param: u16, points: Vec<(f32, NodeId)> },
    Blend2 { params: [u16; 2], points: Vec<([f32; 2], NodeId)> },
    /// `add` is sampled as a delta and added on top of `base`.
    Additive { base: NodeId, add: NodeId, weight: Amount },
    /// The pose the caller supplies (ragdolls, A4); the rest pose when none is given.
    External,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub name: String,
    pub node: NodeId,
    /// Playback rate multiplier.
    pub speed: Amount,
    /// A float parameter (world m/s): the rate is set so the node's clips cover that ground speed.
    pub match_speed: Option<u16>,
    pub root_motion: RootMotion,
    /// Weight of IK solver `i` in this state (solvers not listed: 1).
    pub ik: Vec<(u16, f32)>,
    pub on_enter: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CondValue {
    Num(f32),
    Bool(bool),
    /// Index into the enum's values.
    Enum(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cond {
    pub param: u16,
    pub op: Op,
    pub value: CondValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// `None`: any state but `to`.
    pub from: Option<Vec<u16>>,
    pub to: u16,
    pub when: Vec<Cond>,
    /// Phase (cycles since entering) it may fire from.
    pub exit: Option<f64>,
    /// Crossfade seconds.
    pub blend: f32,
    pub keep_phase: bool,
}

impl Transition {
    pub fn leaves(&self, state: u16) -> bool {
        match &self.from {
            None => state != self.to,
            Some(list) => list.contains(&state),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub mode: LayerMode,
    pub weight: Amount,
    pub mask: Option<MaskId>,
    pub states: Vec<State>,
    pub entry: u16,
    pub transitions: Vec<Transition>,
}

impl Layer {
    pub fn state(&self, name: &str) -> Option<u16> {
        self.states.iter().position(|s| s.name == name).map(|i| i as u16)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Solver {
    Feet(Feet),
    /// Reaches a `vec3` parameter (world space).
    TwoBone { chain: Chain, target: u16, keep_end: bool },
    LookAt { spec: LookAt, target: u16 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ik {
    pub name: String,
    pub solver: Solver,
    pub weight: Amount,
}

/// Allowed Euler degrees per axis (rest-relative, the bone's own axes).
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub bone: usize,
    pub axes: [Option<[f32; 2]>; 3],
}

/// A parsed, bound graph (clip, mask, bone and parameter names resolved to indices).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimGraph {
    pub params: Vec<Param>,
    pub nodes: Vec<Node>,
    /// `layers[0]` is the base layer.
    pub layers: Vec<Layer>,
    pub ik: Vec<Ik>,
    pub limits: Vec<Limit>,
    /// Ragdoll capsule radii by bone, model metres (A4; `"ragdoll": {"radius": {...}}`): bones
    /// not listed get a radius from their length.
    pub ragdoll_radius: Vec<(usize, f32)>,
    /// Bones the motion checks treat as feet.
    pub feet: Vec<usize>,
    /// Per node: whether every (non-additive) clip under it loops (exit times of looping states
    /// wait for the next cycle). All true for a structure-only check.
    pub node_loops: Vec<bool>,
    /// The graph as written (for tools).
    pub source: Json,
}

impl AnimGraph {
    pub fn param(&self, name: &str) -> Option<u16> {
        self.params.iter().position(|p| p.name == name).map(|i| i as u16)
    }

    pub fn layer(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.name == name)
    }

    /// Parses `text` and binds it to `set` (its clips, masks and skeleton).
    pub fn parse(text: &str, set: &AnimSet) -> Result<AnimGraph, AnimError> {
        let json = Json::parse(text).map_err(|e| {
            let mut er = err("", AnimErrorCode::Json, e.message.clone());
            er.offset = Some(e.offset);
            er
        })?;
        AnimGraph::from_json(&json, "", set)
    }

    /// Binds a graph object (at `path`, for errors) to `set`.
    pub fn from_json(v: &Json, path: &str, set: &AnimSet) -> Result<AnimGraph, AnimError> {
        parse_graph(v, &Binder::Set(set)).map_err(|e| e.under(path))
    }

    /// Checks a graph's structure without a skeleton or clips: every rule except that clip,
    /// mask and bone names exist and that IK chains are parent chains (what an agent can check
    /// before any model with the rig is built).
    pub fn check_structure(v: &Json) -> Result<AnimGraph, AnimError> {
        parse_graph(v, &Binder::Loose)
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing

/// Resolves names against an anim set, or (loosely) accepts any name.
enum Binder<'a> {
    Set(&'a AnimSet),
    Loose,
}

impl Binder<'_> {
    fn clip(&self, name: &str) -> Option<ClipId> {
        match self {
            Binder::Set(s) => s.id(name),
            Binder::Loose => Some(ClipId(0)),
        }
    }

    fn mask(&self, name: &str, path: &str) -> Result<MaskId, AnimError> {
        match self {
            Binder::Set(s) => s.mask_id(name).ok_or_else(|| err(path, AnimErrorCode::UnknownName, format!("no mask '{name}' in the anim set (masks: {})", s.mask_names().join(", ")))),
            Binder::Loose => Ok(MaskId(0)),
        }
    }

    fn bone(&self, name: &str, path: &str) -> Result<usize, AnimError> {
        match self {
            Binder::Set(s) => s.skeleton.find(name).ok_or_else(|| err(path, AnimErrorCode::UnknownBone, format!("no bone '{name}' (bones: {})", s.skeleton.names.join(", ")))),
            Binder::Loose => Ok(0),
        }
    }

    fn chain(&self, names: [&str; 3], path: &str) -> Result<Chain, AnimError> {
        let b = [self.bone(names[0], path)?, self.bone(names[1], path)?, self.bone(names[2], path)?];
        match self {
            Binder::Set(s) => Chain::new(&s.skeleton, b[0], b[1], b[2])
                .ok_or_else(|| err(path, AnimErrorCode::InvalidRig, format!("'{}' → '{}' → '{}' is not a parent chain (each bone the parent of the next)", names[0], names[1], names[2]))),
            Binder::Loose => Ok(Chain { upper: 0, lower: 0, end: 0, hinge: [1.0, 0.0, 0.0] }),
        }
    }

    fn clip_names(&self) -> String {
        match self {
            Binder::Set(s) => s.clips().map(|(_, c)| c.name.as_str()).take(40).collect::<Vec<_>>().join(", "),
            Binder::Loose => String::new(),
        }
    }
}

fn name_ok(s: &str) -> bool {
    s.len() <= MAX_NAME && is_ident(s)
}

fn req_name(path: &str, s: &str, what: &str) -> Result<String, AnimError> {
    if !name_ok(s) {
        return Err(err(path, AnimErrorCode::WrongType, format!("{what} names are identifiers (letters, digits, _) of at most {MAX_NAME} characters: '{s}'")));
    }
    Ok(s.to_string())
}

fn limit_list<'a>(v: &'a Json, path: &str, max: usize, what: &str) -> Result<&'a [Json], AnimError> {
    let items = v.as_array().ok_or_else(|| err(path, AnimErrorCode::WrongType, format!("expected a list of {what}")))?;
    if items.len() > max {
        return Err(err(path, AnimErrorCode::Limit, format!("{} {what}; at most {max}", items.len())).with_numbers(items.len() as f64, max as f64));
    }
    Ok(items)
}

fn limit_obj<'a>(v: &'a Json, path: &str, max: usize, what: &str) -> Result<&'a [(String, Json)], AnimError> {
    let pairs = v.as_object().ok_or_else(|| err(path, AnimErrorCode::WrongType, format!("expected an object of {what}")))?;
    if pairs.len() > max {
        return Err(err(path, AnimErrorCode::Limit, format!("{} {what}; at most {max}", pairs.len())).with_numbers(pairs.len() as f64, max as f64));
    }
    // Duplicate keys are errors (Obj::new checks the same for fixed-field objects).
    let mut keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort_unstable();
    if let Some(w) = keys.windows(2).find(|w| w[0] == w[1]) {
        return Err(err(&format!("{path}.{}", w[0]), AnimErrorCode::Duplicate, format!("'{}' given twice", w[0])));
    }
    Ok(pairs)
}

struct Ctx<'a, 'b> {
    binder: &'b Binder<'a>,
    params: Vec<Param>,
    nodes: Vec<Node>,
    /// Where each node was written (for errors).
    paths: Vec<String>,
    /// Named nodes: source, and the id once parsed (`None` while being parsed: a cycle).
    named: Vec<(String, &'b Json, Option<Option<NodeId>>)>,
}

impl<'a, 'b> Ctx<'a, 'b> {
    fn param(&self, name: &str, path: &str) -> Result<u16, AnimError> {
        self.params.iter().position(|p| p.name == name).map(|i| i as u16).ok_or_else(|| {
            let names: Vec<&str> = self.params.iter().map(|p| p.name.as_str()).collect();
            err(path, AnimErrorCode::UnknownName, format!("no parameter '{name}' (params: {})", names.join(", ")))
        })
    }

    fn float_param(&self, name: &str, path: &str) -> Result<u16, AnimError> {
        let i = self.param(name, path)?;
        match self.params[i as usize].kind {
            ParamKind::Float { .. } => Ok(i),
            ref k => Err(err(path, AnimErrorCode::WrongType, format!("'{name}' is a {} parameter; this needs a float", k.name()))),
        }
    }

    fn vec3_param(&self, name: &str, path: &str) -> Result<u16, AnimError> {
        let i = self.param(name, path)?;
        match self.params[i as usize].kind {
            ParamKind::Vec3 => Ok(i),
            ref k => Err(err(path, AnimErrorCode::WrongType, format!("'{name}' is a {} parameter; a target needs a vec3", k.name()))),
        }
    }

    fn amount(&self, v: &Json, path: &str, lo: f32, hi: f32) -> Result<Amount, AnimError> {
        match v {
            Json::Num(_) => Ok(Amount::Const(num(v, path, lo, hi)?)),
            Json::Str(s) => Ok(Amount::Param(self.float_param(s, path)?, 1.0)),
            Json::Obj(_) => {
                let mut o = Obj::new(v, path, "a parameter amount")?;
                let p = self.float_param(o.req_str("param")?, &o.at("param"))?;
                let scale = o.num("scale", 1.0, -100.0, 100.0)?;
                o.finish("param, scale")?;
                Ok(Amount::Param(p, scale))
            }
            _ => Err(err(path, AnimErrorCode::WrongType, "expected a number, a float parameter's name or {param, scale}")),
        }
    }

    fn push(&mut self, n: Node, path: &str) -> Result<NodeId, AnimError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(err(path, AnimErrorCode::Limit, format!("at most {MAX_NODES} nodes in a graph")));
        }
        self.nodes.push(n);
        self.paths.push(path.to_string());
        Ok(NodeId(self.nodes.len() as u16 - 1))
    }

    /// A node: a name (named node or clip) or an inline object.
    fn node(&mut self, v: &'b Json, path: &str, depth: usize) -> Result<NodeId, AnimError> {
        if depth > MAX_NODE_DEPTH {
            return Err(err(path, AnimErrorCode::Limit, format!("blend trees nest at most {MAX_NODE_DEPTH} deep")));
        }
        match v {
            Json::Str(name) => self.named_or_clip(name, path, depth),
            Json::Obj(_) => self.inline(v, path, depth),
            _ => Err(err(path, AnimErrorCode::WrongType, "a node is a clip or node name, or an object (clip, blend1d, blend2d, base + add, external)")),
        }
    }

    fn named_or_clip(&mut self, name: &str, path: &str, depth: usize) -> Result<NodeId, AnimError> {
        if let Some(i) = self.named.iter().position(|(n, _, _)| n == name) {
            match self.named[i].2 {
                Some(Some(id)) => return Ok(id),
                Some(None) => return Err(err(path, AnimErrorCode::InvalidRig, format!("node '{name}' contains itself"))),
                None => {}
            }
            self.named[i].2 = Some(None);
            let src = self.named[i].1;
            let id = self.node(src, &format!("nodes.{name}"), depth + 1)?;
            self.named[i].2 = Some(Some(id));
            return Ok(id);
        }
        match self.binder.clip(name) {
            Some(clip) => self.push(Node::Clip { clip, speed: 1.0 }, path),
            None => Err(err(path, AnimErrorCode::UnknownName, format!("no node or clip '{name}' (clips: {})", self.binder.clip_names()))),
        }
    }

    fn inline(&mut self, v: &'b Json, path: &str, depth: usize) -> Result<NodeId, AnimError> {
        let mut o = Obj::new(v, path, "a node")?;
        if let Some(c) = o.str("clip")? {
            let clip = self.binder.clip(c).ok_or_else(|| err(&format!("{path}.clip"), AnimErrorCode::UnknownName, format!("no clip '{c}' (clips: {})", self.binder.clip_names())))?;
            let speed = o.num("speed", 1.0, 0.01, 100.0)?;
            o.finish("clip, speed")?;
            return self.push(Node::Clip { clip, speed }, path);
        }
        if let Some(p) = o.get("blend1d") {
            let pp = o.at("blend1d");
            let param = self.float_param(p.as_str().ok_or_else(|| err(&pp, AnimErrorCode::WrongType, "blend1d names a float parameter"))?, &pp)?;
            let items = limit_list(o.req("points")?, &o.at("points"), MAX_POINTS, "points")?;
            if items.is_empty() {
                return Err(err(&o.at("points"), AnimErrorCode::MissingField, "at least one point"));
            }
            let mut points = Vec::with_capacity(items.len());
            for (i, it) in items.iter().enumerate() {
                let ip = format!("{}[{i}]", o.at("points"));
                let pair = it.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(&ip, AnimErrorCode::WrongType, "a point is [x, node]"))?;
                let x = num(&pair[0], &format!("{ip}[0]"), -1e6, 1e6)?;
                if let Some(&(prev, _)) = points.last() {
                    if x <= prev {
                        return Err(err(&format!("{ip}[0]"), AnimErrorCode::NotIncreasing, format!("{x} is not after {prev}: points go in increasing x")));
                    }
                }
                let n = self.node(&pair[1], &format!("{ip}[1]"), depth + 1)?;
                points.push((x, n));
            }
            o.finish("blend1d, points")?;
            return self.push(Node::Blend1 { param, points }, path);
        }
        if let Some(p) = o.get("blend2d") {
            let pp = o.at("blend2d");
            let names = p.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(&pp, AnimErrorCode::WrongType, "blend2d names two float parameters: [x, y]"))?;
            let mut params = [0u16; 2];
            for (k, n) in names.iter().enumerate() {
                let np = format!("{pp}[{k}]");
                params[k] = self.float_param(n.as_str().ok_or_else(|| err(&np, AnimErrorCode::WrongType, "a parameter name"))?, &np)?;
            }
            let items = limit_list(o.req("points")?, &o.at("points"), MAX_POINTS, "points")?;
            if items.is_empty() {
                return Err(err(&o.at("points"), AnimErrorCode::MissingField, "at least one point"));
            }
            let mut points: Vec<([f32; 2], NodeId)> = Vec::with_capacity(items.len());
            for (i, it) in items.iter().enumerate() {
                let ip = format!("{}[{i}]", o.at("points"));
                let pair = it.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(&ip, AnimErrorCode::WrongType, "a point is [[x, y], node]"))?;
                let at: [f32; 2] = vecn(&pair[0], &format!("{ip}[0]"), -1e6, 1e6)?;
                if points.iter().any(|(q, _)| q == &at) {
                    return Err(err(&format!("{ip}[0]"), AnimErrorCode::Duplicate, format!("two points at {at:?}")));
                }
                let n = self.node(&pair[1], &format!("{ip}[1]"), depth + 1)?;
                points.push((at, n));
            }
            o.finish("blend2d, points")?;
            return self.push(Node::Blend2 { params, points }, path);
        }
        if let Some(b) = o.get("base") {
            let base = self.node(b, &o.at("base"), depth + 1)?;
            let ap = o.at("add");
            let add = self.node(o.req("add")?, &ap, depth + 1)?;
            let weight = match o.get("weight") {
                Some(w) => self.amount(w, &o.at("weight"), 0.0, 10.0)?,
                None => Amount::Const(1.0),
            };
            o.finish("base, add, weight")?;
            return self.push(Node::Additive { base, add, weight }, path);
        }
        if let Some(e) = o.get("external") {
            if e != &Json::Bool(true) {
                return Err(err(&o.at("external"), AnimErrorCode::WrongType, "\"external\": true"));
            }
            o.finish("external")?;
            return self.push(Node::External, path);
        }
        Err(err(path, AnimErrorCode::MissingField, "a node object has one of: clip, blend1d, blend2d, base (with add), external"))
    }
}

fn parse_param(name: &str, v: &Json, path: &str) -> Result<Param, AnimError> {
    let name = req_name(path, name, "parameter")?;
    let (kind_s, o) = match v {
        Json::Str(s) => (s.as_str(), None),
        Json::Obj(_) => {
            let mut o = Obj::new(v, path, "a parameter")?;
            let k = o.req_str("type")?;
            (k, Some(o))
        }
        _ => return Err(err(path, AnimErrorCode::WrongType, "a parameter is \"float\", \"bool\", \"trigger\", \"vec3\" or {type, ...}")),
    };
    let tp = format!("{path}.type");
    let mut o = o;
    let default_of = |o: &mut Option<Obj>| -> Result<Option<Value>, AnimError> {
        match o.as_mut() {
            Some(o) => {
                let dp = o.at("default");
                o.get("default").map(|d| Value::from_json(d, &dp)).transpose()
            }
            None => Ok(None),
        }
    };
    let param = match kind_s {
        "float" => {
            let (min, max) = match o.as_mut() {
                Some(o) => (o.num("min", -1e6, -1e6, 1e6)?, o.num("max", 1e6, -1e6, 1e6)?),
                None => (-1e6, 1e6),
            };
            if min > max {
                return Err(err(path, AnimErrorCode::OutOfRange, format!("min {min} is above max {max}")));
            }
            let d = default_of(&mut o)?.unwrap_or(Value::Num(0.0_f32.clamp(min, max)));
            let p = Param { name, kind: ParamKind::Float { min, max }, default: None };
            let d = p.accept(&d).map_err(|m| err(&format!("{path}.default"), AnimErrorCode::WrongType, m))?;
            Param { default: Some(d), ..p }
        }
        "bool" | "trigger" => {
            let kind = if kind_s == "bool" { ParamKind::Bool } else { ParamKind::Trigger };
            let d = default_of(&mut o)?.unwrap_or(Value::Bool(false));
            let p = Param { name, kind, default: None };
            let d = p.accept(&d).map_err(|m| err(&format!("{path}.default"), AnimErrorCode::WrongType, m))?;
            Param { default: Some(d), ..p }
        }
        "enum" => {
            let oo = o.as_mut().ok_or_else(|| err(path, AnimErrorCode::MissingField, "an enum parameter is {\"type\": \"enum\", \"values\": [...]}"))?;
            let vp = oo.at("values");
            let items = limit_list(oo.req("values")?, &vp, MAX_ENUM_VALUES, "values")?;
            let mut values: Vec<String> = Vec::with_capacity(items.len());
            for (i, it) in items.iter().enumerate() {
                let ip = format!("{vp}[{i}]");
                let s = req_name(&ip, it.as_str().ok_or_else(|| err(&ip, AnimErrorCode::WrongType, "an enum value is a name"))?, "enum value")?;
                if values.contains(&s) {
                    return Err(err(&ip, AnimErrorCode::Duplicate, format!("'{s}' listed twice")));
                }
                values.push(s);
            }
            if values.is_empty() {
                return Err(err(&vp, AnimErrorCode::MissingField, "at least one value"));
            }
            let first = Value::Name(values[0].clone());
            let p = Param { name, kind: ParamKind::Enum(values), default: None };
            let d = default_of(&mut o)?.unwrap_or(first);
            let d = p.accept(&d).map_err(|m| err(&format!("{path}.default"), AnimErrorCode::WrongType, m))?;
            Param { default: Some(d), ..p }
        }
        "vec3" => {
            let p = Param { name, kind: ParamKind::Vec3, default: None };
            let d = default_of(&mut o)?;
            let d = d.map(|d| p.accept(&d)).transpose().map_err(|m| err(&format!("{path}.default"), AnimErrorCode::WrongType, m))?;
            Param { default: d, ..p }
        }
        other => return Err(err(&tp, AnimErrorCode::UnknownName, format!("unknown parameter type '{other}' (float, bool, trigger, enum, vec3)"))),
    };
    if let Some(o) = o {
        o.finish("type, min, max, default, values")?;
    }
    Ok(param)
}

/// Splits a condition string into tokens: names, numbers, operators and `!`.
fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            flush(&mut cur, &mut out);
        } else if "=!<>&".contains(c) {
            flush(&mut cur, &mut out);
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            if ["==", "!=", "<=", ">=", "&&"].contains(&two.as_str()) {
                out.push(two);
                i += 1;
            } else {
                out.push(c.to_string());
            }
        } else {
            cur.push(c);
        }
        i += 1;
    }
    flush(&mut cur, &mut out);
    out
}

fn parse_conditions(ctx: &Ctx, v: &Json, path: &str) -> Result<Vec<Cond>, AnimError> {
    let strings: Vec<(String, &str)> = match v {
        Json::Str(s) => vec![(path.to_string(), s.as_str())],
        Json::Arr(items) => {
            if items.len() > MAX_CONDITIONS {
                return Err(err(path, AnimErrorCode::Limit, format!("{} conditions; at most {MAX_CONDITIONS}", items.len())));
            }
            items
                .iter()
                .enumerate()
                .map(|(i, it)| it.as_str().map(|s| (format!("{path}[{i}]"), s)).ok_or_else(|| err(&format!("{path}[{i}]"), AnimErrorCode::WrongType, "a condition is a string like \"speed > 0.1\"")))
                .collect::<Result<_, _>>()?
        }
        _ => return Err(err(path, AnimErrorCode::WrongType, "\"when\" is a condition string or a list of them")),
    };
    let mut out = Vec::new();
    for (p, s) in strings {
        let toks = tokens(s);
        // Split on `and` / `&&`.
        for part in toks.split(|t| t == "and" || t == "&&") {
            if out.len() >= MAX_CONDITIONS {
                return Err(err(&p, AnimErrorCode::Limit, format!("at most {MAX_CONDITIONS} conditions per transition")));
            }
            out.push(parse_condition(ctx, part, &p, s)?);
        }
    }
    Ok(out)
}

fn parse_condition(ctx: &Ctx, toks: &[String], path: &str, text: &str) -> Result<Cond, AnimError> {
    let bad = |why: &str| err(path, AnimErrorCode::WrongType, format!("can't read the condition '{text}': {why} (forms: flag, !flag, param == value, param < number)"));
    let (negate, rest) = match toks.first().map(String::as_str) {
        Some("!") => (true, &toks[1..]),
        _ => (false, toks),
    };
    let Some(name) = rest.first() else { return Err(bad("it is empty")) };
    let param = ctx.param(name, path)?;
    let kind = &ctx.params[param as usize].kind;
    if rest.len() == 1 {
        return match kind {
            ParamKind::Bool | ParamKind::Trigger => Ok(Cond { param, op: Op::Eq, value: CondValue::Bool(!negate) }),
            k => Err(err(path, AnimErrorCode::WrongType, format!("'{name}' is a {} parameter: compare it (e.g. '{name} == ...')", k.name()))),
        };
    }
    if negate || rest.len() != 3 {
        return Err(bad("expected 'param op value'"));
    }
    let op = match rest[1].as_str() {
        "==" => Op::Eq,
        "!=" => Op::Ne,
        "<" => Op::Lt,
        "<=" => Op::Le,
        ">" => Op::Gt,
        ">=" => Op::Ge,
        other => return Err(bad(&format!("unknown operator '{other}'"))),
    };
    let raw = rest[2].as_str();
    let value = match kind {
        ParamKind::Float { .. } => CondValue::Num(raw.parse::<f32>().ok().filter(|x| x.is_finite()).ok_or_else(|| bad(&format!("'{raw}' is not a number")))?),
        ParamKind::Bool | ParamKind::Trigger => {
            if !matches!(op, Op::Eq | Op::Ne) {
                return Err(bad("flags compare with == or != only"));
            }
            CondValue::Bool(match raw {
                "true" => true,
                "false" => false,
                _ => return Err(bad(&format!("'{raw}' is not true or false"))),
            })
        }
        ParamKind::Enum(values) => {
            if !matches!(op, Op::Eq | Op::Ne) {
                return Err(bad("enums compare with == or != only"));
            }
            let i = values.iter().position(|v| v == raw).ok_or_else(|| err(path, AnimErrorCode::UnknownName, format!("'{raw}' is not a value of '{name}' ({})", values.join(", "))))?;
            CondValue::Enum(i as u16)
        }
        ParamKind::Vec3 => return Err(err(path, AnimErrorCode::WrongType, format!("'{name}' is a vec3: vectors can't be compared"))),
    };
    Ok(Cond { param, op, value })
}

fn parse_states<'b>(ctx: &mut Ctx<'_, 'b>, v: &'b Json, path: &str, ik: &[String]) -> Result<Vec<State>, AnimError> {
    let pairs = limit_obj(v, path, MAX_STATES, "states")?;
    if pairs.is_empty() {
        return Err(err(path, AnimErrorCode::MissingField, "at least one state"));
    }
    let mut out = Vec::with_capacity(pairs.len());
    for (name, sv) in pairs {
        let sp = format!("{path}.{name}");
        let name = req_name(&sp, name, "state")?;
        let state = match sv {
            Json::Str(_) => {
                let node = ctx.node(sv, &sp, 0)?;
                State { name, node, speed: Amount::Const(1.0), match_speed: None, root_motion: RootMotion::Off, ik: Vec::new(), on_enter: None }
            }
            _ => {
                let mut o = Obj::new(sv, &sp, "a state")?;
                let pp = o.at("play");
                let node = ctx.node(o.req("play")?, &pp, 0)?;
                let speed = match o.get("speed") {
                    Some(s) => ctx.amount(s, &o.at("speed"), 0.0, 100.0)?,
                    None => Amount::Const(1.0),
                };
                let match_speed = match o.str("match_speed")? {
                    Some(p) => Some(ctx.float_param(p, &o.at("match_speed"))?),
                    None => None,
                };
                let root_motion = match o.str("root_motion")? {
                    None | Some("off") => RootMotion::Off,
                    Some("horizontal") => RootMotion::Horizontal,
                    Some("full") => RootMotion::Full,
                    Some(other) => return Err(err(&o.at("root_motion"), AnimErrorCode::UnknownName, format!("root_motion '{other}' (off, horizontal, full)"))),
                };
                let mut weights = Vec::new();
                if let Some(w) = o.get("ik") {
                    let wp = o.at("ik");
                    for (k, val) in limit_obj(w, &wp, MAX_IK, "IK weights")? {
                        let kp = format!("{wp}.{k}");
                        let i = ik.iter().position(|n| n == k).ok_or_else(|| err(&kp, AnimErrorCode::UnknownName, format!("no IK solver '{k}' (solvers: {})", ik.join(", "))))?;
                        weights.push((i as u16, num(val, &kp, 0.0, 1.0)?));
                    }
                }
                let on_enter = match o.str("on_enter")? {
                    Some(e) => Some(req_name(&o.at("on_enter"), e, "event")?),
                    None => None,
                };
                o.finish("play, speed, match_speed, root_motion, ik, on_enter")?;
                State { name, node, speed, match_speed, root_motion, ik: weights, on_enter }
            }
        };
        out.push(state);
    }
    Ok(out)
}

fn parse_transitions(ctx: &Ctx, v: Option<&Json>, path: &str, states: &[State]) -> Result<Vec<Transition>, AnimError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let items = limit_list(v, path, MAX_TRANSITIONS, "transitions")?;
    let state_of = |name: &str, p: &str| -> Result<u16, AnimError> {
        states.iter().position(|s| s.name == name).map(|i| i as u16).ok_or_else(|| {
            let names: Vec<&str> = states.iter().map(|s| s.name.as_str()).collect();
            err(p, AnimErrorCode::UnknownName, format!("no state '{name}' in this layer (states: {})", names.join(", ")))
        })
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, t) in items.iter().enumerate() {
        let tp = format!("{path}[{i}]");
        let mut o = Obj::new(t, &tp, "a transition")?;
        let fp = o.at("from");
        let from = match o.req("from")? {
            Json::Str(s) if s == "*" => None,
            Json::Str(s) => Some(vec![state_of(s, &fp)?]),
            Json::Arr(list) if !list.is_empty() && list.len() <= MAX_STATES => {
                let mut v = Vec::new();
                for (k, s) in list.iter().enumerate() {
                    let kp = format!("{fp}[{k}]");
                    v.push(state_of(s.as_str().ok_or_else(|| err(&kp, AnimErrorCode::WrongType, "a state name"))?, &kp)?);
                }
                Some(v)
            }
            _ => return Err(err(&fp, AnimErrorCode::WrongType, "\"from\" is a state, a list of states or \"*\" (any)")),
        };
        let to = state_of(o.req_str("to")?, &o.at("to"))?;
        let when = match o.get("when") {
            Some(w) => parse_conditions(ctx, w, &o.at("when"))?,
            None => Vec::new(),
        };
        let exit = match o.get("exit") {
            None => None,
            Some(Json::Str(s)) if s == "end" => Some(1.0),
            Some(e) => Some(num(e, &o.at("exit"), 0.0, 1e4)? as f64),
        };
        if when.is_empty() && exit.is_none() && from.is_none() {
            return Err(err(&tp, AnimErrorCode::MissingField, "a transition from \"*\" needs a \"when\" or an \"exit\" (it would fire from every state at once)"));
        }
        let blend = o.num("blend", 0.2, 0.0, 60.0)?;
        let keep_phase = match o.get("keep_phase") {
            None => false,
            Some(Json::Bool(b)) => *b,
            Some(_) => return Err(err(&o.at("keep_phase"), AnimErrorCode::WrongType, "true or false")),
        };
        o.finish("from, to, when, exit, blend, keep_phase")?;
        out.push(Transition { from, to, when, exit, blend, keep_phase });
    }
    Ok(out)
}

fn parse_layer<'b>(ctx: &mut Ctx<'_, 'b>, o: &mut Obj<'b>, name: String, base: bool, ik: &[String]) -> Result<Layer, AnimError> {
    let sp = o.at("states");
    let states = parse_states(ctx, o.req("states")?, &sp, ik)?;
    let entry = match o.str("entry")? {
        Some(e) => states.iter().position(|s| s.name == e).ok_or_else(|| err(&o.at("entry"), AnimErrorCode::UnknownName, format!("no state '{e}'")))? as u16,
        None => 0,
    };
    let tp = o.at("transitions");
    let transitions = parse_transitions(ctx, o.get("transitions"), &tp, &states)?;
    let (mode, weight, mask) = if base {
        (LayerMode::Override, Amount::Const(1.0), None)
    } else {
        let mode = match o.str("mode")? {
            None | Some("override") => LayerMode::Override,
            Some("additive") => LayerMode::Additive,
            Some(other) => return Err(err(&o.at("mode"), AnimErrorCode::UnknownName, format!("layer mode '{other}' (override, additive)"))),
        };
        let weight = match o.get("weight") {
            Some(w) => ctx.amount(w, &o.at("weight"), 0.0, 1.0)?,
            None => Amount::Const(1.0),
        };
        let mask = match o.str("mask")? {
            Some(m) => Some(ctx.binder.mask(m, &o.at("mask"))?),
            None => None,
        };
        (mode, weight, mask)
    };
    Ok(Layer { name, mode, weight, mask, states, entry, transitions })
}

fn bone_names<'j>(v: &'j Json, path: &str, max: usize) -> Result<Vec<(&'j str, String)>, AnimError> {
    let items = limit_list(v, path, max, "bones")?;
    items
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let p = format!("{path}[{i}]");
            b.as_str().map(|s| (s, p.clone())).ok_or_else(|| err(&p, AnimErrorCode::WrongType, "a bone name"))
        })
        .collect()
}

fn parse_ik(ctx: &Ctx, v: &Json, path: &str) -> Result<Vec<Ik>, AnimError> {
    let items = limit_list(v, path, MAX_IK, "IK solvers")?;
    let mut out: Vec<Ik> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let ip = format!("{path}[{i}]");
        let mut o = Obj::new(it, &ip, "an IK solver")?;
        let kind = o.req_str("kind")?;
        let name = match o.str("name")? {
            Some(n) => req_name(&o.at("name"), n, "IK solver")?,
            None => kind.to_string(),
        };
        if out.iter().any(|k| k.name == name) {
            return Err(err(&ip, AnimErrorCode::Duplicate, format!("a second IK solver '{name}' (give it a \"name\")")));
        }
        let weight = match o.get("weight") {
            Some(w) => ctx.amount(w, &o.at("weight"), 0.0, 1.0)?,
            None => Amount::Const(1.0),
        };
        let chain3 = |v: &Json, p: &str| -> Result<Chain, AnimError> {
            let b = bone_names(v, p, 3)?;
            if b.len() != 3 {
                return Err(err(p, AnimErrorCode::WrongType, "a chain is [upper, lower, end] bone names"));
            }
            ctx.binder.chain([b[0].0, b[1].0, b[2].0], p)
        };
        let solver = match kind {
            "feet" => {
                let lp = o.at("legs");
                let legs_j = limit_list(o.req("legs")?, &lp, crate::ik::MAX_LEGS, "legs")?;
                let mut legs = Vec::new();
                for (k, l) in legs_j.iter().enumerate() {
                    legs.push(chain3(l, &format!("{lp}[{k}]"))?);
                }
                if legs.is_empty() {
                    return Err(err(&lp, AnimErrorCode::MissingField, "at least one leg"));
                }
                let pelvis = match o.str("pelvis")? {
                    Some(p) => Some(ctx.binder.bone(p, &o.at("pelvis"))?),
                    None => None,
                };
                let max_step = o.num("max_step", 0.4, 0.0, 10.0)?;
                let max_drop = o.num("max_drop", 0.4, 0.0, 10.0)?;
                o.finish("kind, name, weight, legs, pelvis, max_step, max_drop")?;
                Solver::Feet(Feet { legs, pelvis, max_step, max_drop })
            }
            "two_bone" => {
                let chain = chain3(o.req("chain")?, &o.at("chain"))?;
                let target = ctx.vec3_param(o.req_str("target")?, &o.at("target"))?;
                let keep_end = match o.get("keep_end") {
                    None => true,
                    Some(Json::Bool(b)) => *b,
                    Some(_) => return Err(err(&o.at("keep_end"), AnimErrorCode::WrongType, "true or false")),
                };
                o.finish("kind, name, weight, chain, target, keep_end")?;
                Solver::TwoBone { chain, target, keep_end }
            }
            "look_at" => {
                let bp = o.at("bones");
                let names = bone_names(o.req("bones")?, &bp, 8)?;
                if names.is_empty() {
                    return Err(err(&bp, AnimErrorCode::MissingField, "at least one bone (the last one looks)"));
                }
                let shares: Vec<f32> = match o.get("share") {
                    Some(s) => {
                        let sp = o.at("share");
                        let list = limit_list(s, &sp, 8, "shares")?;
                        if list.len() != names.len() {
                            return Err(err(&sp, AnimErrorCode::WrongType, format!("{} shares for {} bones", list.len(), names.len())));
                        }
                        list.iter().enumerate().map(|(k, x)| num(x, &format!("{sp}[{k}]"), 0.0, 1.0)).collect::<Result<_, _>>()?
                    }
                    // Most of the turn in the last bones.
                    None => (1..=names.len()).map(|k| k as f32).collect(),
                };
                let mut bones = Vec::new();
                for ((n, p), s) in names.iter().zip(&shares) {
                    bones.push((ctx.binder.bone(n, p)?, *s));
                }
                let axis = o.vec("axis", [0.0, 0.0, 1.0], -1.0, 1.0)?;
                if axis.iter().map(|x| x * x).sum::<f32>() < 1e-6 {
                    return Err(err(&o.at("axis"), AnimErrorCode::OutOfRange, "the look axis can't be zero"));
                }
                let limit_deg = o.num("limit", 70.0, 0.0, 180.0)?;
                let target = ctx.vec3_param(o.req_str("target")?, &o.at("target"))?;
                o.finish("kind, name, weight, bones, share, axis, limit, target")?;
                Solver::LookAt { spec: LookAt { bones, axis, limit_deg }, target }
            }
            other => return Err(err(&o.at("kind"), AnimErrorCode::UnknownName, format!("unknown IK kind '{other}' (feet, two_bone, look_at)"))),
        };
        out.push(Ik { name, solver, weight });
    }
    Ok(out)
}

fn parse_graph(v: &Json, binder: &Binder) -> Result<AnimGraph, AnimError> {
    let mut o = Obj::new(v, "", "an animation graph")?;
    if let Some(ver) = o.get("graph") {
        if ver.as_f64() != Some(FORMAT_VERSION as f64) {
            return Err(err("graph", AnimErrorCode::OutOfRange, format!("unsupported graph format (this build reads {FORMAT_VERSION})")));
        }
    }
    let mut ctx = Ctx { binder, params: Vec::new(), nodes: Vec::new(), paths: Vec::new(), named: Vec::new() };
    if let Some(p) = o.get("params") {
        for (name, pv) in limit_obj(p, "params", MAX_PARAMS, "parameters")? {
            let param = parse_param(name, pv, &format!("params.{name}"))?;
            ctx.params.push(param);
        }
    }
    if let Some(n) = o.get("nodes") {
        for (name, nv) in limit_obj(n, "nodes", MAX_NODES, "nodes")? {
            let name = req_name(&format!("nodes.{name}"), name, "node")?;
            if binder.clip(&name).is_some() && matches!(binder, Binder::Set(_)) {
                return Err(err(&format!("nodes.{name}"), AnimErrorCode::Duplicate, format!("'{name}' is also a clip: name the node differently")));
            }
            ctx.named.push((name, nv, None));
        }
    }
    // IK names first: states refer to them.
    let ik_names: Vec<String> = match o.get("ik") {
        Some(Json::Arr(items)) => items.iter().filter_map(|i| i.get("name").or_else(|| i.get("kind")).and_then(Json::as_str).map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    let mut layers = Vec::new();
    // The base layer: top-level states.
    layers.push(parse_layer(&mut ctx, &mut o, "base".into(), true, &ik_names)?);
    if let Some(ls) = o.get("layers") {
        let items = limit_list(ls, "layers", MAX_LAYERS - 1, "extra layers")?;
        for (i, l) in items.iter().enumerate() {
            let lp = format!("layers[{i}]");
            let mut lo = Obj::new(l, &lp, "a layer")?;
            let name = req_name(&lo.at("name"), lo.req_str("name")?, "layer")?;
            if layers.iter().any(|x: &Layer| x.name == name) {
                return Err(err(&lp, AnimErrorCode::Duplicate, format!("a second layer '{name}'")));
            }
            let layer = parse_layer(&mut ctx, &mut lo, name, false, &ik_names)?;
            lo.finish("name, mode, weight, mask, states, entry, transitions")?;
            layers.push(layer);
        }
    }
    let ik = match o.get("ik") {
        Some(v) => parse_ik(&ctx, v, "ik")?,
        None => Vec::new(),
    };
    let mut limits = Vec::new();
    if let Some(l) = o.get("limits") {
        for (bone, lv) in limit_obj(l, "limits", MAX_LIMITS, "limits")? {
            let lp = format!("limits.{bone}");
            let b = binder.bone(bone, &lp)?;
            let mut lo = Obj::new(lv, &lp, "a joint limit")?;
            let mut axes = [None; 3];
            for (k, ax) in ["x", "y", "z"].iter().enumerate() {
                if let Some(r) = lo.get(ax) {
                    let range: [f32; 2] = vecn(r, &lo.at(ax), -360.0, 360.0)?;
                    if range[0] > range[1] {
                        return Err(err(&lo.at(ax), AnimErrorCode::OutOfRange, format!("{range:?}: the low end comes first")));
                    }
                    axes[k] = Some(range);
                }
            }
            lo.finish("x, y, z")?;
            limits.push(Limit { bone: b, axes });
        }
    }
    let feet = match o.get("feet") {
        Some(f) => bone_names(f, "feet", 8)?.iter().map(|(n, p)| binder.bone(n, p)).collect::<Result<Vec<_>, _>>()?,
        None => ik
            .iter()
            .filter_map(|k| match &k.solver {
                Solver::Feet(f) => Some(f.legs.iter().map(|l| l.end).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect(),
    };
    // Ragdoll sizes (A4): capsule radii per bone (`ragdoll_set {fit: true}` measures them).
    let mut ragdoll_radius = Vec::new();
    if let Some(r) = o.get("ragdoll") {
        let mut ro = Obj::new(r, "ragdoll", "the ragdoll's sizes")?;
        if let Some(rad) = ro.get("radius") {
            for (bone, rv) in limit_obj(rad, "ragdoll.radius", MAX_LIMITS, "radii")? {
                let rp = format!("ragdoll.radius.{bone}");
                let b = binder.bone(bone, &rp)?;
                let x = rv.as_f64().filter(|x| x.is_finite() && (0.005..=10.0).contains(x));
                let x = x.ok_or_else(|| err(&rp, AnimErrorCode::OutOfRange, "a capsule radius in model metres, 0.005..10"))?;
                ragdoll_radius.push((b, x as f32));
            }
        }
        ro.finish("radius")?;
    }
    o.finish("graph, params, nodes, states, entry, transitions, layers, ik, limits, ragdoll, feet")?;
    // Named nodes nobody plays are still checked.
    for i in 0..ctx.named.len() {
        if ctx.named[i].2.is_none() {
            let name = ctx.named[i].0.clone();
            ctx.named_or_clip(&name, &format!("nodes.{name}"), 0)?;
        }
    }
    let node_loops = check_trees(&ctx)?;
    check_instant_cycles(&ctx.params, &layers)?;
    Ok(AnimGraph { params: ctx.params, nodes: ctx.nodes, layers, ik, limits, ragdoll_radius, feet, node_loops, source: v.clone() })
}

/// Work [`check_instant_cycles`] may do per graph (paths looked at); past it, the resolver's
/// own guard still keeps such a loop from spinning.
const CYCLE_CHECK_BUDGET: usize = 20_000;

/// Refuses transitions that fire at once (no exit time, no trigger: a trigger is spent by
/// firing) around a loop of states with conditions that can all hold together: `a -> b when on`
/// with `b -> a when on` would flip the layer back and forth without time passing.
fn check_instant_cycles(params: &[Param], layers: &[Layer]) -> Result<(), AnimError> {
    let mut budget = CYCLE_CHECK_BUDGET;
    for (li, l) in layers.iter().enumerate() {
        let prefix = if li == 0 { "transitions".to_string() } else { format!("layers[{}].transitions", li - 1) };
        let instant: Vec<usize> = (0..l.transitions.len())
            .filter(|&i| {
                let t = &l.transitions[i];
                t.exit.is_none_or(|x| x <= 0.0) && !t.when.iter().any(|c| matches!(params[c.param as usize].kind, ParamKind::Trigger))
            })
            .collect();
        if instant.is_empty() {
            continue;
        }
        for start in 0..l.states.len() as u16 {
            let mut stack: Vec<(u16, Vec<usize>)> = vec![(start, Vec::new())];
            while let Some((s, path)) = stack.pop() {
                for &ti in &instant {
                    let t = &l.transitions[ti];
                    if !t.leaves(s) || path.contains(&ti) {
                        continue;
                    }
                    if budget == 0 {
                        return Ok(());
                    }
                    budget -= 1;
                    let mut p = path.clone();
                    p.push(ti);
                    let conds: Vec<&Cond> = p.iter().flat_map(|&i| l.transitions[i].when.iter()).collect();
                    if !can_hold(params, &conds) {
                        continue;
                    }
                    if t.to == start {
                        let name = |s: u16| l.states[s as usize].name.as_str();
                        let mut at = start;
                        let steps: Vec<String> = p
                            .iter()
                            .map(|&i| {
                                let to = l.transitions[i].to;
                                let step = format!("{prefix}[{i}] ({} -> {})", name(at), name(to));
                                at = to;
                                step
                            })
                            .collect();
                        return Err(err(
                            &format!("{prefix}[{}]", p[0]),
                            AnimErrorCode::InstantCycle,
                            format!("{} can all fire at once: their conditions can hold together and none waits for an exit time, so the layer would cycle without time passing; give one an exit time or a condition the others exclude", steps.join(", ")),
                        ));
                    }
                    if p.len() < l.states.len() {
                        stack.push((t.to, p));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Whether conditions can all hold at once (per parameter: an enum or bool value, a number
/// interval; triggers can always hold).
fn can_hold(params: &[Param], conds: &[&Cond]) -> bool {
    let mut seen: Vec<u16> = conds.iter().map(|c| c.param).collect();
    seen.sort_unstable();
    seen.dedup();
    seen.into_iter().all(|p| {
        let cs = conds.iter().filter(|c| c.param == p);
        match &params[p as usize].kind {
            ParamKind::Float { min, max } => {
                let (mut lo, mut lo_open, mut hi, mut hi_open) = (*min as f64, false, *max as f64, false);
                let (mut eq, mut ne): (Option<f64>, Vec<f64>) = (None, Vec::new());
                for c in cs {
                    let CondValue::Num(v) = c.value else { return false };
                    let v = v as f64;
                    match c.op {
                        Op::Eq => {
                            if eq.is_some_and(|e| e != v) {
                                return false;
                            }
                            eq = Some(v);
                        }
                        Op::Ne => ne.push(v),
                        Op::Gt | Op::Ge if v > lo || (v == lo && c.op == Op::Gt) => (lo, lo_open) = (v, c.op == Op::Gt),
                        Op::Lt | Op::Le if v < hi || (v == hi && c.op == Op::Lt) => (hi, hi_open) = (v, c.op == Op::Lt),
                        _ => {}
                    }
                }
                let inside = |x: f64| (x > lo || (x == lo && !lo_open)) && (x < hi || (x == hi && !hi_open));
                match eq {
                    Some(e) => inside(e) && !ne.contains(&e),
                    None => lo < hi || (lo == hi && !lo_open && !hi_open && !ne.contains(&lo)),
                }
            }
            ParamKind::Bool | ParamKind::Trigger => {
                let mut want: Option<bool> = None;
                for c in cs {
                    let CondValue::Bool(b) = c.value else { return false };
                    let v = if c.op == Op::Eq { b } else { !b };
                    if want.is_some_and(|w| w != v) {
                        return false;
                    }
                    want = Some(v);
                }
                true
            }
            ParamKind::Enum(values) => {
                let (mut eq, mut ne): (Option<u16>, Vec<u16>) = (None, Vec::new());
                for c in cs {
                    let CondValue::Enum(v) = c.value else { return false };
                    if c.op == Op::Eq {
                        if eq.is_some_and(|e| e != v) {
                            return false;
                        }
                        eq = Some(v);
                    } else {
                        ne.push(v);
                    }
                }
                match eq {
                    Some(e) => !ne.contains(&e),
                    None => {
                        ne.sort_unstable();
                        ne.dedup();
                        ne.len() < values.len()
                    }
                }
            }
            ParamKind::Vec3 => true,
        }
    })
}

/// Every node's real depth and clip count (named nodes reused under each other count every use)
/// against [`MAX_NODE_DEPTH`] and [`MAX_LEAVES`], and whether it loops. Children always come
/// before their parents in the arena, so one pass in order sees every child first.
fn check_trees(ctx: &Ctx) -> Result<Vec<bool>, AnimError> {
    let n = ctx.nodes.len();
    let (mut depth, mut count, mut loops) = (vec![0usize; n], vec![0u64; n], vec![true; n]);
    for (i, node) in ctx.nodes.iter().enumerate() {
        let kids: Vec<usize> = match node {
            Node::Clip { .. } | Node::External => Vec::new(),
            Node::Blend1 { points, .. } => points.iter().map(|(_, c)| c.0 as usize).collect(),
            Node::Blend2 { points, .. } => points.iter().map(|(_, c)| c.0 as usize).collect(),
            Node::Additive { base, add, .. } => vec![base.0 as usize, add.0 as usize],
        };
        debug_assert!(kids.iter().all(|&k| k < i), "children are parsed before their parents");
        depth[i] = 1 + kids.iter().map(|&k| depth[k]).max().unwrap_or(0);
        count[i] = if kids.is_empty() { 1 } else { kids.iter().map(|&k| count[k]).fold(0u64, u64::saturating_add) };
        loops[i] = match node {
            Node::Clip { clip, .. } => match ctx.binder {
                Binder::Set(set) => set.clip(*clip).looping != crate::clip::LoopMode::Once,
                Binder::Loose => true,
            },
            Node::External => true,
            Node::Additive { base, .. } => loops[base.0 as usize],
            _ => kids.iter().all(|&k| loops[k]),
        };
        if depth[i] > MAX_NODE_DEPTH + 1 {
            return Err(err(&ctx.paths[i], AnimErrorCode::Limit, format!("this blend tree nests {} deep (through named nodes); at most {MAX_NODE_DEPTH}", depth[i] - 1)).with_numbers((depth[i] - 1) as f64, MAX_NODE_DEPTH as f64));
        }
        if count[i] > MAX_LEAVES {
            return Err(err(&ctx.paths[i], AnimErrorCode::Limit, format!("this blend tree plays {} clips (a named node used twice under another counts twice); at most {MAX_LEAVES}", count[i])).with_numbers(count[i] as f64, MAX_LEAVES as f64));
        }
    }
    Ok(loops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_tokens() {
        assert_eq!(tokens("speed>=0.5 and !hit"), ["speed", ">=", "0.5", "and", "!", "hit"]);
        assert_eq!(tokens("act==move&&x<1"), ["act", "==", "move", "&&", "x", "<", "1"]);
    }
}
