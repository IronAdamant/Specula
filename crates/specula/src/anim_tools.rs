//! The animation tools (`anim_*`, stage A3): read a model's anim set, write clips and graphs
//! into the world as anim assets (journaled, claimable, undoable), inspect an entity's play,
//! check motion against the game's `style.toml` rules, and render filmstrips and contact sheets
//! with the checks attached. See [`crate::anim`] for how assets and graph plays work.

use super::*;
use crate::anim::{self as eanim, ASSET_PROP, ASSET_TAG};
use crate::world::{EntityKey, Op, OpResult, Shape, SpawnSpec, UpdateSpec};
use specula_anim::checks::{self, MotionRules, Report};
use specula_anim::graph::{AnimGraph, ParamKind, Value};
use specula_anim::play::{self as aplay, Entry, GraphPlay, GraphPlayer, Graphed, LayerPlay, Scratch, Surroundings, Timing};
use specula_anim::{blend, film, AnimSet, Clip, LoopMode, Pose};
use specula_core::math::{Mat4, Vec3};
use specula_protocol::ToolDef;
use std::sync::Arc;

/// Rows one filmstrip may have.
pub const MAX_ROWS: usize = 12;
/// Frames per filmstrip row.
pub const MAX_FRAMES: u32 = 16;
/// Largest clip or graph JSON `anim_get` returns whole (bigger ones are summarised).
pub const MAX_JSON_BYTES: usize = 24 * 1024;
/// Longest window `anim_state` and entity filmstrips look at (seconds).
pub const MAX_WINDOW_S: f64 = 30.0;
/// Checks a report lists by default (failures first; the rest counted).
const DEFAULT_MAX_CHECKS: usize = 40;

/// What `anim_set` writes.
pub enum SetWhat {
    Clip(Json),
    Graph(Json),
    /// A JSON merge patch (RFC 7386) on the graph as it is now.
    Patch(Json),
    Delete,
}

/// A graph state to check or film, held with parameters (`None`: the graph's defaults, plus any
/// enum parameter with a value named like the state set to it).
pub struct StateSubject {
    pub state: String,
    pub params: Option<Json>,
}

/// A transition caused by changing parameters `from` → `to` after `settle_s` seconds.
pub struct TransitionSubject {
    pub from: Json,
    pub to: Json,
    pub settle_s: f32,
}

/// One filmstrip row.
pub enum Row {
    Clip(String),
    State(StateSubject),
    Transition(TransitionSubject),
}

pub enum AnimCall {
    Get { source: String, clip: Option<String>, graph: Option<String> },
    Set { source: String, name: String, what: SetWhat, expect_version: Option<u64> },
    State { key: EntityKey, time_s: Option<f64>, window_s: f64 },
    Check { source: Option<String>, key: Option<EntityKey>, graph: String, clips: Option<Vec<String>>, states: Option<Vec<StateSubject>>, transitions: Vec<TransitionSubject>, scale: Option<f32>, max_checks: usize },
    Filmstrip { source: Option<String>, key: Option<EntityKey>, graph: String, rows: Vec<Row>, frames: u32, tile: u32, window_s: f32, scale: Option<f32>, checks: bool },
}

/// The schedule class of an anim call (filmstrips render).
pub fn call_class(call: &AnimCall) -> schedule::Class {
    match call {
        AnimCall::Filmstrip { .. } => schedule::Class::Render,
        AnimCall::Set { .. } => schedule::Class::Edit,
        AnimCall::Check { .. } => schedule::Class::Big,
        AnimCall::Get { .. } | AnimCall::State { .. } => schedule::Class::Read,
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing

fn source_arg(args: &Json, required: bool) -> Result<Option<String>, String> {
    match tools::string_arg(args, "recipe")? {
        Some(r) if !r.is_empty() && r.len() <= 256 && !r.contains("..") => Ok(Some(r.to_string())),
        Some(_) => Err("'recipe' must be a recipe (or .glb/.gltf) path under the assets root, like \"recipes/shambler.json\"".into()),
        None if required => Err("'recipe' is required: the model the animation is for, like \"recipes/shambler.json\"".into()),
        None => Ok(None),
    }
}

fn name_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match tools::string_arg(args, key)? {
        Some(n) if n.len() <= specula_anim::graph::MAX_NAME && n.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => Ok(Some(n.to_string())),
        Some(n) => Err(format!("'{key}' must be an identifier (letters, digits, _; at most {} characters): '{n}'", specula_anim::graph::MAX_NAME)),
        None => Ok(None),
    }
}

fn entity_arg(args: &Json, tool: &str) -> Result<Option<EntityKey>, String> {
    let present = ["id", "path", "entity"].iter().any(|k| args.get(k).is_some_and(|v| !v.is_null()));
    if !present {
        return Ok(None);
    }
    if let Some(v) = args.get("entity").filter(|v| !v.is_null()) {
        return tools::key_value(v, "entity").map(Some);
    }
    tools::entity_key(args, tool).map(Some)
}

fn object_arg(args: &Json, key: &str) -> Result<Option<Json>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v @ Json::Obj(_)) => Ok(Some(v.clone())),
        Some(_) => Err(format!("'{key}' must be an object")),
    }
}

fn state_subject(v: &Json, path: &str) -> Result<StateSubject, String> {
    match v {
        Json::Str(s) => Ok(StateSubject { state: s.clone(), params: None }),
        Json::Obj(_) => {
            let state = v.get("state").and_then(Json::as_str).ok_or_else(|| format!("'{path}.state' (a state name) is required"))?;
            for (k, _) in v.as_object().into_iter().flatten() {
                if !matches!(k.as_str(), "state" | "params") {
                    return Err(format!("'{path}.{k}' is not a state field (state, params)"));
                }
            }
            Ok(StateSubject { state: state.to_string(), params: object_arg(v, "params")? })
        }
        _ => Err(format!("'{path}' is a state name or {{state, params}}")),
    }
}

fn transition_subject(v: &Json, path: &str) -> Result<TransitionSubject, String> {
    for (k, _) in v.as_object().ok_or_else(|| format!("'{path}' is {{from: params, to: params, settle_s?}}"))? {
        if !matches!(k.as_str(), "from" | "to" | "settle_s") {
            return Err(format!("'{path}.{k}' is not a transition field (from, to, settle_s)"));
        }
    }
    let from = object_arg(v, "from")?.ok_or_else(|| format!("'{path}.from' (the parameters before) is required"))?;
    let to = object_arg(v, "to")?.ok_or_else(|| format!("'{path}.to' (the parameters after) is required"))?;
    let settle_s = match v.get("settle_s") {
        None | Some(Json::Null) => 1.37,
        Some(s) => s.as_f64().filter(|x| (0.0..=30.0).contains(x)).ok_or_else(|| format!("'{path}.settle_s' must be 0..30 seconds"))? as f32,
    };
    Ok(TransitionSubject { from, to, settle_s })
}

fn list<'a>(args: &'a Json, key: &str, max: usize) -> Result<Option<&'a [Json]>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Arr(items)) if items.len() <= max => Ok(Some(items)),
        Some(Json::Arr(items)) => Err(format!("'{key}' has {} items; at most {max}", items.len())),
        Some(_) => Err(format!("'{key}' must be a list")),
    }
}

/// The anim call `name` names, if it is one of these tools.
pub fn parse(name: &str, args: &Json) -> Option<Result<AnimCall, String>> {
    let r = match name {
        "anim_get" => (|| {
            tools::check_keys(args, name, &[&["recipe", "clip", "graph"]])?;
            Ok(AnimCall::Get { source: source_arg(args, true)?.expect("required"), clip: name_arg(args, "clip")?, graph: name_arg(args, "graph")? })
        })(),
        "anim_set" => (|| {
            tools::check_keys(args, name, &[&["recipe", "name", "clip", "graph", "patch", "delete", "expect_version"]])?;
            let source = source_arg(args, true)?.expect("required");
            let n = name_arg(args, "name")?.ok_or("'name' is required: the clip's or graph's name")?;
            let given: Vec<&str> = ["clip", "graph", "patch", "delete"].into_iter().filter(|k| args.get(k).is_some_and(|v| !v.is_null() && *v != Json::Bool(false))).collect();
            if given.len() != 1 {
                return Err("give exactly one of 'clip' (a keyed clip or generator spec), 'graph', 'patch' (merge patch on a graph) or 'delete': true".into());
            }
            let what = match given[0] {
                "clip" => SetWhat::Clip(object_arg(args, "clip")?.expect("given")),
                "graph" => SetWhat::Graph(object_arg(args, "graph")?.expect("given")),
                "patch" => SetWhat::Patch(object_arg(args, "patch")?.expect("given")),
                _ => match tools::bool_arg(args, "delete")? {
                    Some(true) => SetWhat::Delete,
                    _ => return Err("'delete' must be true".into()),
                },
            };
            let expect_version = tools::int_arg(args, "expect_version", 1, u64::MAX >> 11)?;
            Ok(AnimCall::Set { source, name: n, what, expect_version })
        })(),
        "anim_state" => (|| {
            tools::check_keys(args, name, &[&["id", "path", "entity", "time_s", "window_s"]])?;
            let key = entity_arg(args, name)?.ok_or("anim_state needs the entity: 'id', 'path' or 'entity'")?;
            let time_s = args.get("time_s").filter(|v| !v.is_null()).map(|v| v.as_f64().filter(|t| t.is_finite() && t.abs() <= 1e9).ok_or("'time_s' must be world seconds")).transpose()?;
            let window_s = args.get("window_s").filter(|v| !v.is_null()).map(|v| v.as_f64().filter(|t| (0.0..=MAX_WINDOW_S).contains(t)).ok_or(format!("'window_s' must be 0..{MAX_WINDOW_S} seconds"))).transpose()?.unwrap_or(2.0);
            Ok(AnimCall::State { key, time_s, window_s })
        })(),
        "anim_check" => (|| {
            tools::check_keys(args, name, &[&["recipe", "id", "path", "entity", "graph", "clips", "states", "transitions", "scale", "max_checks"]])?;
            let source = source_arg(args, false)?;
            let key = entity_arg(args, name)?;
            if source.is_some() == key.is_some() {
                return Err("anim_check takes 'recipe' (check the model's clips and states) or the entity ('id', 'path' or 'entity'), not both".into());
            }
            let clips = list(args, "clips", 256)?.map(|l| l.iter().map(|c| c.as_str().map(str::to_string).ok_or("'clips' is a list of clip names")).collect::<Result<Vec<_>, _>>()).transpose()?;
            let states = list(args, "states", 64)?.map(|l| l.iter().enumerate().map(|(i, s)| state_subject(s, &format!("states[{i}]"))).collect::<Result<Vec<_>, _>>()).transpose()?;
            let transitions = list(args, "transitions", 32)?.map(|l| l.iter().enumerate().map(|(i, t)| transition_subject(t, &format!("transitions[{i}]"))).collect::<Result<Vec<_>, _>>()).transpose()?.unwrap_or_default();
            Ok(AnimCall::Check {
                source,
                key,
                graph: name_arg(args, "graph")?.unwrap_or_else(|| "creature".into()),
                clips,
                states,
                transitions,
                scale: tools::num_in(args, "scale", 0.01..=100.0)?,
                max_checks: tools::int_arg(args, "max_checks", 1, 500)?.unwrap_or(DEFAULT_MAX_CHECKS as u64) as usize,
            })
        })(),
        "anim_filmstrip" => (|| {
            tools::check_keys(args, name, &[&["recipe", "id", "path", "entity", "graph", "rows", "frames", "tile", "window_s", "scale", "checks"]])?;
            let source = source_arg(args, false)?;
            let key = entity_arg(args, name)?;
            if source.is_some() == key.is_some() {
                return Err("anim_filmstrip takes 'recipe' (rows of clips, states, transitions) or the entity ('id', 'path' or 'entity': its play over time in the world), not both".into());
            }
            let rows = match list(args, "rows", MAX_ROWS)? {
                None => Vec::new(),
                Some(items) => items
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let p = format!("rows[{i}]");
                        if let Some(c) = r.get("clip") {
                            return c.as_str().map(|c| Row::Clip(c.to_string())).ok_or(format!("'{p}.clip' is a clip name"));
                        }
                        if r.get("state").is_some() {
                            return state_subject(r, &p).map(Row::State);
                        }
                        if let Some(t) = r.get("transition") {
                            return transition_subject(t, &format!("{p}.transition")).map(Row::Transition);
                        }
                        Err(format!("'{p}' is {{clip}}, {{state, params?}} or {{transition: {{from, to, settle_s?}}}}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            };
            if key.is_some() && !rows.is_empty() {
                return Err("an entity filmstrip films the entity's own play: leave 'rows' out (use 'recipe' for clips and states)".into());
            }
            Ok(AnimCall::Filmstrip {
                source,
                key,
                graph: name_arg(args, "graph")?.unwrap_or_else(|| "creature".into()),
                rows,
                frames: tools::int_arg(args, "frames", 2, MAX_FRAMES as u64)?.unwrap_or(8) as u32,
                tile: tools::int_arg(args, "tile", 64, 320)?.unwrap_or(160) as u32,
                window_s: tools::num_in(args, "window_s", 0.1..=MAX_WINDOW_S as f32)?.unwrap_or(2.0),
                scale: tools::num_in(args, "scale", 0.01..=100.0)?,
                checks: tools::bool_arg(args, "checks")?.unwrap_or(true),
            })
        })(),
        _ => return None,
    };
    Some(r)
}

// ---------------------------------------------------------------------------------------------
// Tool definitions

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let recipe = |d: &str| t("string", d);
    let entity = || {
        vec![
            ("id", t("integer", "The entity's id")),
            ("path", t("string", "The entity's path, like \"/live/enemies/shambler_12\"")),
            ("entity", Json::obj([("description", Json::from("The entity: id, \"/path\" or a unique name"))])),
        ]
    };
    let params_doc = "graph parameters {name: value} (numbers, true/false, enum names, [x, y, z])";
    vec![
        ToolDef {
            name: "anim_get".into(),
            description: "A model's animation: its skeleton (bones), masks, clips (duration, loop, distance per cycle, events; \
                          whether each is the recipe's own or a world anim asset) and graphs (parameters with types and \
                          ranges, layers with states and transitions, IK solvers), plus problems with the world's anim \
                          assets for it. Give 'clip' or 'graph' to get that one as JSON (bounded; anim_set takes the same \
                          form). The model must be built (an entity uses it, or call model_build first)."
                .into(),
            input_schema: schema(
                vec![
                    ("recipe", recipe("The model: a recipe path under the assets root (\"recipes/shambler.json\") or a .glb/.gltf")),
                    ("clip", t("string", "Return this clip as JSON")),
                    ("graph", t("string", "Return this graph as JSON")),
                ],
                &["recipe"],
            ),
        },
        ToolDef {
            name: "anim_set".into(),
            description: "Writes a clip or an animation graph for a model, live: it becomes a world anim asset (a Group \
                          entity at /anims/<recipe stem>/clips/<name> or .../graphs/<name>, tagged anim_asset), journaled \
                          under you, claim-checked (claim /anims/<stem> to own a model's animation), saved with the world \
                          and undone with world_undo. Same-named clips and graphs replace the recipe's own for every \
                          entity of that model at once; new ones add. Validated against the built anim set (errors give \
                          a path and code); without a built model only the structure is checked. \
                          clip: a keyed clip {duration, loop, distance, tracks: [{bone, channel: rotation|translation|scale, \
                          interp, times, values}], events} or a generator spec {kind, duration, ...}: kinds gait (walk/run \
                          cycles from legs, stride, duty, lift, bounce, sway; \"body\": \"auto\" finds hips/spine/legs/arms by bone \
                          names), idle, hover, swing (windup_pose/strike_pose), leap, collapse (topple/crumble/scatter/fade), \
                          keys (keys: [{time, pose: {bone: {rot: [x,y,z] deg, move, scale}}}], interp), pose (one held pose), \
                          recoil (pose, attack, frequency, damping: hit reactions for additive layers), breathe, wobble, chain \
                          (secondary motion: source clip + bones, stiffness, damping, gravity: tails, shrouds, cloth strips). \
                          graph: {params: {name: float{min,max,default}|bool|trigger|enum{values}|vec3}, nodes: {name: \
                          node}, states: {name: clip | {play: node, speed, match_speed: param, root_motion, ik: {solver: \
                          weight}, on_enter}}, entry, transitions: [{from: state|[states]|\"*\", to, when: \"speed > 0.1 and \
                          !hit\", exit: phase|\"end\", blend: s, keep_phase}], layers: [{name, mode: override|additive, \
                          weight, mask, states, transitions}], ik: [{kind: feet (legs, pelvis, max_step, max_drop) | \
                          two_bone (chain, target: vec3 param) | look_at (bones, target, limit), weight}], limits: {bone: \
                          {x: [lo, hi] deg}}, feet: [bones]}. Nodes: a clip, {clip, speed}, {blend1d: param, points: [[x, \
                          node]...]}, {blend2d: [px, py], points: [[[x, y], node]...]}, {base, add, weight} (additive), \
                          {external: true} (a pose from outside: ragdolls). patch: a merge patch on the graph as it is now. \
                          delete: true removes the world's version (the recipe's own, if any, plays again)."
                .into(),
            input_schema: schema(
                vec![
                    ("recipe", recipe("The model the clip or graph is for (\"recipes/shambler.json\")")),
                    ("name", t("string", "The clip's or graph's name (an identifier)")),
                    ("clip", t("object", "A keyed clip or a generator spec (see the description)")),
                    ("graph", t("object", "A whole graph (see the description)")),
                    ("patch", t("object", "A JSON merge patch on the graph as it is now (null removes a key)")),
                    ("delete", t("boolean", "true: remove the world's version")),
                    ("expect_version", t("integer", "Only if the asset entity is still at this version (another agent hasn't changed it)")),
                ],
                &["recipe", "name"],
            ),
        },
        ToolDef {
            name: "anim_state".into(),
            description: "What an animated entity is doing at a moment (default: now): for a graph play, each layer's \
                          blend stack (state, crossfade weight, phase in cycles, rate, the clips it blends with their \
                          shares), the parameters in effect (defaults filled in), the events it fires and the root \
                          motion it makes over the next window_s seconds, what its IK did (feet: the ground found under \
                          each, offset, miss; pelvis), and problems (unknown parameters or states). For a clip anim: the \
                          clip, its time and fade. Change parameters with world_set {anim: {graph, params}}: the play \
                          settles at the write and blends; parameters you leave out keep their values."
                .into(),
            input_schema: schema(
                {
                    let mut p = entity();
                    p.push(("time_s", t("number", "World time in seconds (default: now)")));
                    p.push(("window_s", t("number", "Seconds of events and root motion to report from time_s, 0..30 (default 2)")));
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "anim_check".into(),
            description: "Measures motion and reports it as data against the game's style.toml [motion] rules (found next to \
                          the recipe or above it in the assets root; defaults otherwise): foot_sliding (cm/s while a sole is \
                          within contact_m, 2.5 cm, of the ground), floating (cm the lowest sole stays above it), \
                          ground_penetration (cm below, by skinned vertices), joint_limits (deg past a graph's limits), pops \
                          (cm per frame a transition jumps beyond the states' own motion, at 1.8 m scale), root_drift (cm), \
                          loop_seam (mm). A value over its limit is an error (in 'errors', with the subject, bone and time); \
                          [motion.allow] exempts deliberate style. Subjects: 'recipe' checks every clip and every base-layer \
                          state (or those listed in clips/states, with params) plus listed transitions (parameter changes); \
                          an entity checks what it plays now, at its own scale."
                .into(),
            input_schema: schema(
                {
                    let mut p = vec![("recipe", recipe("The model to check (or give the entity)"))];
                    p.extend(entity());
                    p.extend([
                        ("graph", t("string", "The graph whose states to check (default \"creature\")")),
                        ("clips", t("array", "Only these clips ([] for none)")),
                        ("states", t("array", &format!("Only these states: names or {{state, params}} ({params_doc}; default: the graph's defaults with any enum parameter named like the state set to it)"))),
                        ("transitions", t("array", &format!("Parameter changes to check for pops: [{{from: {{...}}, to: {{...}}, settle_s?}}] ({params_doc})"))),
                        ("scale", t("number", "World metres per model metre for match_speed states (default 1; an entity's own)")),
                        ("max_checks", t("integer", "Checks listed, failures first (default 40, at most 500); the rest are counted")),
                    ]);
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "anim_filmstrip".into(),
            description: "Animation sight: a contact sheet of poses over time, returned like sight captures (PNG plus a \
                          packet saved in the captures folder) with the motion checks of every row attached. With \
                          'recipe': one row per clip, state ({state, params}) or transition ({transition: {from, to}}), \
                          'frames' poses across its cycle (default: every clip), side view on a neutral stage with a \
                          ground plane. With an entity: one row of its own play in the world over window_s seconds from \
                          now (IK on the real ground, the world around it), side and front."
                .into(),
            input_schema: schema(
                {
                    let mut p = vec![("recipe", recipe("The model (or give the entity)"))];
                    p.extend(entity());
                    p.extend([
                        ("graph", t("string", "The graph states and transitions belong to (default \"creature\")")),
                        ("rows", t("array", "[{clip: name} | {state: name, params?} | {transition: {from: params, to: params, settle_s?}}], at most 12")),
                        ("frames", t("integer", "Poses per row, 2..16 (default 8)")),
                        ("tile", t("integer", "Tile size in pixels, 64..320 (default 160)")),
                        ("window_s", t("number", "Entity filmstrips: seconds to film from now (default 2)")),
                        ("scale", t("number", "World metres per model metre for match_speed states (default 1)")),
                        ("checks", t("boolean", "Attach the motion checks of each row (default true)")),
                    ]);
                    p
                },
                &[],
            ),
        },
    ]
}

// ---------------------------------------------------------------------------------------------
// Helpers

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_pretty())
}

fn anim_error(e: &specula_anim::AnimError, context: &str) -> ToolOutput {
    let mut j = e.to_json();
    j.set("error", Json::from("invalid_anim"));
    j.set("context", Json::from(context));
    ToolOutput::error(j.to_pretty())
}

/// RFC 7386 JSON merge patch.
fn merge_patch(target: &Json, patch: &Json) -> Json {
    let Json::Obj(changes) = patch else { return patch.clone() };
    let mut out: Vec<(String, Json)> = match target {
        Json::Obj(pairs) => pairs.clone(),
        _ => Vec::new(),
    };
    for (k, v) in changes {
        let i = out.iter().position(|(n, _)| n == k);
        match (v, i) {
            (Json::Null, Some(i)) => {
                out.remove(i);
            }
            (Json::Null, None) => {}
            (v, Some(i)) => out[i].1 = merge_patch(&out[i].1, v),
            (v, None) => out.push((k.clone(), merge_patch(&Json::Null, v))),
        }
    }
    Json::Obj(out)
}

/// The recipe stem a source's assets live under (`recipes/shambler.json` → `shambler`).
fn stem_of(source: &str) -> String {
    let file = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let stem = file.split('.').next().unwrap_or(file);
    let clean: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    if clean.is_empty() {
        "model".into()
    } else {
        clean
    }
}

/// Parameters for `graph` from JSON (checked), or the defaults with any enum parameter that has a
/// value named `state` set to it.
fn params_for(graph: &AnimGraph, params: Option<&Json>, state: Option<&str>) -> Result<Vec<(String, Value)>, ToolOutput> {
    match params {
        Some(p) => aplay::params_from_json(graph, p, "params").map_err(|e| anim_error(&e, "parameters")),
        None => Ok(graph
            .params
            .iter()
            .filter_map(|p| match (&p.kind, state) {
                (ParamKind::Enum(values), Some(s)) if values.iter().any(|v| v == s) => Some((p.name.clone(), Value::Name(s.to_string()))),
                _ => None,
            })
            .collect()),
    }
}

/// A parameter value for a label (numbers to two places).
fn short(v: &Value) -> String {
    match v {
        Value::Num(x) => format!("{x:.2}"),
        Value::Bool(b) => b.to_string(),
        Value::Name(n) => n.clone(),
        Value::Vec3(p) => format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]),
    }
}

fn value_json(v: &Value) -> Json {
    v.to_json()
}

fn round(x: f64, places: i32) -> Json {
    let k = 10f64.powi(places);
    Json::from((x * k).round() / k)
}

fn v3(v: [f32; 3]) -> Json {
    Json::arr(v.iter().map(|x| round(*x as f64, 4)))
}

/// A play of `graph` held in base-layer `state` with `params` from its start.
fn held_play(graph_name: &str, state: &str, params: &[(String, Value)]) -> GraphPlay {
    let mut p = GraphPlay::new(graph_name);
    for (n, v) in params {
        p.set_param(n, v.clone());
    }
    p.layers.push(LayerPlay { layer: "base".into(), stack: vec![Entry { state: state.to_string(), phase: 0.0, fade: None }] });
    p
}

/// What a model source resolves to: its build and anim set (the world's assets laid over it).
struct Source {
    built: Arc<models::Built>,
    set: Arc<AnimSet>,
    /// Whether an entity's live handle gave it (else a fresh build, without the handle cache).
    problems: Vec<String>,
}

/// The stage the filmstrip renders on: STYLE.md's moonlight, brightened for small tiles, with
/// the moon's shadow so feet visibly meet the ground.
fn stage() -> specula_gpu::Environment {
    let (el, az) = (35.0f32.to_radians(), 50.0f32.to_radians());
    let rgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
    specula_gpu::Environment {
        sun_direction: [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()],
        sun_color: rgb(0xc9, 0xd6, 0xff),
        sun_intensity: 1.8,
        ambient_color: rgb(0x2a, 0x34, 0x48),
        ambient_intensity: 3.0,
        exposure: 1.2,
        tonemap: true,
        shadow: specula_gpu::ShadowSettings { enabled: true, resolution: 2048, cascades: 1, distance: 120.0, softness: 1.5, bias: 1.0 },
        ..specula_gpu::Environment::default()
    }
}

/// One row of a recipe filmstrip, before rendering: what it is and its poses.
struct FilmRow {
    label: String,
    subject: String,
    times: Vec<f64>,
    poses: Vec<Pose>,
    report: Option<Report>,
    /// Where the row goes over the ground (the clip's travel), per frame.
    travel: Vec<f32>,
}

impl Engine {
    /// Answers an anim tool.
    pub(crate) fn anim_call(&mut self, call: AnimCall, w: Writer, tick: u64) -> ToolOutput {
        let _z = profile::zone("anim_tool");
        match call {
            AnimCall::Get { source, clip, graph } => self.anim_get(&source, clip.as_deref(), graph.as_deref()),
            AnimCall::Set { source, name, what, expect_version } => self.anim_set(&source, &name, what, expect_version, w, tick),
            AnimCall::State { key, time_s, window_s } => self.anim_state(&key, time_s, window_s),
            AnimCall::Check { source, key, graph, clips, states, transitions, scale, max_checks } => self.anim_check(source, key, &graph, clips, states, transitions, scale, max_checks),
            AnimCall::Filmstrip { source, key, graph, rows, frames, tile, window_s, scale, checks } => match key {
                Some(k) => self.anim_film_entity(&k, frames, tile, window_s, checks),
                None => self.anim_film_rows(&source.expect("parse checked"), &graph, rows, frames, tile, scale, checks),
            },
        }
    }

    /// The build and anim set of a model source (an entity's when one uses it, else built now
    /// if the build cache has it).
    fn anim_source(&mut self, source: &str) -> Result<Source, ToolOutput> {
        let lib = self.library.lock();
        if let Some(h) = lib.handle_for_source(source) {
            let built = Arc::clone(lib.built(h).expect("handle_for_source has a build"));
            let set = lib.anim_set(h).ok_or_else(|| error("not_rigged", format!("{source} has no rig: nothing to animate")))?;
            let problems = lib.anim_overlay_problems(h);
            return Ok(Source { built, set, problems });
        }
        drop(lib);
        let requested = self.library.lock().request(&world::ModelSpec::new(source));
        let built = match requested {
            Err(e) => return Err(world_error(&e)),
            Ok(models::BuildRequest::Pending(_)) => {
                return Err(error("model_building", format!("{source} is building (no entity uses it yet); call again in a moment, or model_build to wait for it")))
            }
            Ok(models::BuildRequest::Ready(b, _)) => b,
        };
        let lib = self.library.lock();
        let base = match &built.anims {
            Some(s) => Arc::clone(s),
            None => match built.model.rig.as_ref().and_then(|r| specula_anim::Skeleton::from_rig(r).ok()) {
                Some(skel) => Arc::new(AnimSet::new(skel)),
                None => return Err(error("not_rigged", format!("{source} has no rig: nothing to animate"))),
            },
        };
        let (set, problems) = match lib.anim_assets_for(source) {
            Some(a) => {
                let (s, p) = base.with_overlay(&a.clips, &a.graphs);
                (Arc::new(s), p.iter().map(|p| format!("{} '{}': {}", p.kind, p.name, p.error)).collect())
            }
            None => (base, Vec::new()),
        };
        Ok(Source { built, set, problems })
    }

    /// The motion rules for a source: the first style.toml from its folder up to the assets root.
    fn motion_rules(&self, source: &str) -> Result<MotionRules, ToolOutput> {
        let root = self.library.lock().root().to_path_buf();
        let mut dir = root.join(source);
        loop {
            if !dir.pop() || !dir.starts_with(&root) {
                return Ok(MotionRules::default());
            }
            let file = dir.join("style.toml");
            if let Ok(text) = std::fs::read_to_string(&file) {
                let shown = file.strip_prefix(&root).map(|p| p.display().to_string()).unwrap_or_else(|_| file.display().to_string());
                return MotionRules::from_toml(&text, &shown).map_err(|e| anim_error(&e, "style.toml"));
            }
            if dir == root {
                return Ok(MotionRules::default());
            }
        }
    }

    fn anim_get(&mut self, source: &str, clip: Option<&str>, graph: Option<&str>) -> ToolOutput {
        let src = match self.anim_source(source) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let set = &src.set;
        let assets = self.library.lock().anim_assets_for(source).cloned().unwrap_or_default();
        let from_world = |kind: &str, n: &str| if kind == "clip" { assets.clips.iter().any(|(m, _)| m == n) } else { assets.graphs.iter().any(|(m, _)| m == n) };
        let bounded = |j: Json, what: &str| -> Json {
            let text = j.to_string();
            if text.len() <= MAX_JSON_BYTES {
                j
            } else {
                Json::obj([("truncated", Json::from(true)), ("bytes", Json::from(text.len())), ("note", Json::from(format!("the {what} is over {MAX_JSON_BYTES} bytes as JSON; anim_get's summary lists its parts")))])
            }
        };
        if let Some(c) = clip {
            let Some(id) = set.id(c) else {
                return error("unknown_clip", format!("{source} has no clip '{c}' (clips: {})", set.clips().map(|(_, c)| c.name.as_str()).take(64).collect::<Vec<_>>().join(", ")));
            };
            return ToolOutput::text(Json::obj([("recipe", Json::from(source)), ("source", Json::from(if from_world("clip", c) { "world" } else { "recipe" })), ("clip", bounded(set.clip(id).to_json(), "clip"))]).to_string());
        }
        if let Some(g) = graph {
            let Some(gr) = set.graph(g) else {
                return error("unknown_graph", format!("{source} has no graph '{g}' (graphs: {})", set.graphs().map(|(n, _)| n).collect::<Vec<_>>().join(", ")));
            };
            return ToolOutput::text(Json::obj([("recipe", Json::from(source)), ("source", Json::from(if from_world("graph", g) { "world" } else { "recipe" })), ("graph", bounded(gr.source.clone(), "graph"))]).to_string());
        }
        let skel = &set.skeleton;
        let clips = Json::arr(set.clips().take(256).map(|(_, c)| {
            let mut events: Vec<&str> = c.events.iter().map(|e| e.name.as_str()).collect();
            events.dedup();
            events.truncate(16);
            Json::obj([
                ("name", Json::from(c.name.as_str())),
                ("duration", round(c.duration as f64, 4)),
                ("loop", Json::from(c.looping.name())),
                ("distance", round(c.distance as f64, 4)),
                ("events", Json::arr(events.into_iter().map(Json::from))),
                ("source", Json::from(if from_world("clip", &c.name) { "world" } else { "recipe" })),
            ])
        }));
        let graphs = Json::arr(set.graphs().map(|(n, g)| {
            let params = Json::arr(g.params.iter().map(|p| {
                let mut f = vec![("name", Json::from(p.name.as_str())), ("type", Json::from(p.kind.name()))];
                match &p.kind {
                    ParamKind::Float { min, max } => {
                        f.push(("min", Json::from(*min)));
                        f.push(("max", Json::from(*max)));
                    }
                    ParamKind::Enum(values) => f.push(("values", Json::arr(values.iter().map(|v| Json::from(v.as_str()))))),
                    _ => {}
                }
                if let Some(d) = &p.default {
                    f.push(("default", value_json(d)));
                }
                Json::obj(f)
            }));
            let layers = Json::arr(g.layers.iter().map(|l| {
                Json::obj([
                    ("name", Json::from(l.name.as_str())),
                    ("mode", Json::from(match l.mode {
                        specula_anim::LayerMode::Override => "override",
                        specula_anim::LayerMode::Additive => "additive",
                    })),
                    ("states", Json::arr(l.states.iter().map(|s| Json::from(s.name.as_str())))),
                    ("entry", Json::from(l.states[l.entry as usize].name.as_str())),
                    ("transitions", Json::from(l.transitions.len())),
                ])
            }));
            Json::obj([
                ("name", Json::from(n)),
                ("source", Json::from(if from_world("graph", n) { "world" } else { "recipe" })),
                ("params", params),
                ("layers", layers),
                ("ik", Json::arr(g.ik.iter().map(|k| Json::from(k.name.as_str())))),
            ])
        }));
        let mut out = Json::obj([
            ("recipe", Json::from(source)),
            ("bones", Json::from(skel.len())),
            ("bone_names", Json::arr(skel.names.iter().take(128).map(|n| Json::from(n.as_str())))),
            ("masks", Json::arr(set.mask_names().into_iter().map(Json::from))),
            ("clips", clips),
            ("graphs", graphs),
            ("assets", Json::arr(assets.entities.iter().map(|(p, id, v)| Json::obj([("path", Json::from(p.as_str())), ("id", Json::from(*id)), ("version", Json::from(*v))])))),
        ]);
        let mut problems = src.problems.clone();
        problems.extend(self.models.anim_asset_problems().iter().cloned());
        if !problems.is_empty() {
            out.set("problems", Json::arr(problems.into_iter().take(32).map(Json::from)));
        }
        ToolOutput::text(out.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn anim_set(&mut self, source: &str, name: &str, what: SetWhat, expect_version: Option<u64>, w: Writer, tick: u64) -> ToolOutput {
        let kind = match &what {
            SetWhat::Clip(_) => "clip",
            SetWhat::Delete => {
                // Whichever exists (a clip or a graph of that name).
                let stem = stem_of(source);
                let clip_path = format!("/anims/{stem}/clips/{name}");
                if self.world.find_path(&clip_path).is_ok() {
                    "clip"
                } else {
                    "graph"
                }
            }
            _ => "graph",
        };
        let stem = stem_of(source);
        let folder = format!("/anims/{stem}/{}s", kind);
        let path = format!("{folder}/{name}");
        let existing = self.world.find_path(&path).ok();
        // The data to write.
        let data = match &what {
            SetWhat::Clip(j) | SetWhat::Graph(j) => Some(j.clone()),
            SetWhat::Patch(p) => {
                let base = match existing.and_then(|id| self.world.get(id)).and_then(|e| eanim::asset_of(e).ok().map(|a| a.data.clone())) {
                    Some(d) => d,
                    None => match self.anim_source(source).ok().and_then(|s| s.set.graph(name).map(|g| g.source.clone())) {
                        Some(d) => d,
                        None => return error("unknown_graph", format!("no graph '{name}' for {source} to patch (give 'graph' to write a new one)")),
                    },
                };
                Some(merge_patch(&base, p))
            }
            SetWhat::Delete => None,
        };
        if data.is_none() && existing.is_none() {
            return error("not_found", format!("no world {kind} '{name}' for {source} at {path} (the recipe's own clips and graphs are in its files)"));
        }
        // Validate against the anim set it will join (the other assets included).
        let mut validated = "structure only: no built model uses this recipe yet (it is checked against the rig when one does)".to_string();
        let mut warnings: Vec<String> = Vec::new();
        let mut summary = Json::Null;
        if let Some(d) = &data {
            match self.anim_source(source) {
                Ok(src) => {
                    let mut assets = self.library.lock().anim_assets_for(source).cloned().unwrap_or_default();
                    let list = if kind == "clip" { &mut assets.clips } else { &mut assets.graphs };
                    match list.iter_mut().find(|(n, _)| n == name) {
                        Some(entry) => entry.1 = d.clone(),
                        None => list.push((name.to_string(), d.clone())),
                    }
                    let base = match &src.built.anims {
                        Some(s) => Arc::clone(s),
                        None => match src.built.model.rig.as_ref().and_then(|r| specula_anim::Skeleton::from_rig(r).ok()) {
                            Some(skel) => Arc::new(AnimSet::new(skel)),
                            None => return error("not_rigged", format!("{source} has no rig: nothing to animate")),
                        },
                    };
                    let (set, problems) = base.with_overlay(&assets.clips, &assets.graphs);
                    if let Some(p) = problems.iter().find(|p| p.kind == kind && p.name == name) {
                        return anim_error(&p.error, &format!("{kind} '{name}' for {source}"));
                    }
                    warnings.extend(problems.iter().map(|p| format!("with this change, {} '{}' no longer binds: {}", p.kind, p.name, p.error)));
                    validated = format!("against the built anim set of {source} ({} bones)", set.skeleton.len());
                    summary = if kind == "clip" {
                        let c = set.clip(set.id(name).expect("added"));
                        Json::obj([("duration", round(c.duration as f64, 4)), ("loop", Json::from(c.looping.name())), ("tracks", Json::from(c.tracks.len())), ("events", Json::from(c.events.len())), ("distance", round(c.distance as f64, 4))])
                    } else {
                        let g = set.graph(name).expect("added");
                        Json::obj([
                            ("params", Json::arr(g.params.iter().map(|p| Json::from(p.name.as_str())))),
                            ("layers", Json::arr(g.layers.iter().map(|l| Json::obj([("name", Json::from(l.name.as_str())), ("states", Json::arr(l.states.iter().map(|s| Json::from(s.name.as_str()))))])))),
                        ])
                    };
                }
                Err(_) => {
                    // No build: check what can be checked without a skeleton.
                    let structural = if kind == "graph" {
                        AnimGraph::check_structure(d).map(|_| ())
                    } else if d.get("kind").and_then(Json::as_str).unwrap_or("clip") == "clip" {
                        let mut c = d.clone();
                        c.set("name", Json::from(name));
                        Clip::from_json(&c, "").map(|_| ())
                    } else {
                        Ok(())
                    };
                    if let Err(e) = structural {
                        return anim_error(&e, &format!("{kind} '{name}' for {source}"));
                    }
                }
            }
        }
        // The world change: parents as needed, then the asset entity.
        let mut ops = Vec::new();
        if let Some(d) = data {
            let mut parent = String::new();
            for seg in ["anims".to_string(), stem.clone(), format!("{kind}s")] {
                let p = format!("{parent}/{seg}");
                if self.world.find_path(&p).is_err() {
                    let mut g = SpawnSpec::new(Shape::Group);
                    g.name = Some(seg.clone());
                    g.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.clone()));
                    ops.push(Op::Spawn(g));
                }
                parent = p;
            }
            let asset = Json::obj([("for", Json::from(source)), ("kind", Json::from(kind)), ("data", d)]);
            match existing {
                Some(id) => {
                    let mut u = UpdateSpec { expect_version, ..UpdateSpec::default() };
                    u.props = Some(Json::obj([(ASSET_PROP, asset)]));
                    u.add_tags = vec![ASSET_TAG.into()];
                    ops.push(Op::Set(EntityKey::Id(id), u));
                }
                None => {
                    if expect_version.is_some() {
                        return error("version_conflict", format!("{path} doesn't exist yet, so it has no version to expect"));
                    }
                    let mut s = SpawnSpec::new(Shape::Group);
                    s.name = Some(name.to_string());
                    s.parent = Some(EntityKey::Path(folder.clone()));
                    s.tags.insert(ASSET_TAG.into());
                    s.props = Json::obj([(ASSET_PROP, asset)]);
                    ops.push(Op::Spawn(s));
                }
            }
        } else {
            ops.push(Op::Delete(EntityKey::Id(existing.expect("checked")), expect_version));
        }
        let by = w.name.to_string();
        let result = self.swarm.commit(&mut self.world, tick, w, Kind::Batch, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e));
        let (results, seq) = match result {
            Ok(r) => r,
            Err(mut e) => {
                if e.code == "props_too_big" || e.message.contains("props") {
                    e.message.push_str(" (anim assets live in props, at most 16 KiB as JSON: write long keyframed clips as generator specs, or split them)");
                }
                return world_error(&e);
            }
        };
        // The assets reach the library (and every entity of the model) at once.
        self.models.refresh_anim_env(&self.library, &self.world, self.world_generation);
        let last = results.last();
        let (id, version) = match last {
            Some(OpResult::Spawned { id, .. }) => (*id, self.world.get(*id).map_or(0, |e| e.version)),
            Some(OpResult::Updated { id, version, .. }) => (*id, *version),
            Some(OpResult::Deleted { id, .. }) => (*id, 0),
            None => (0, 0),
        };
        let mut out = Json::obj([
            ("path", Json::from(path.as_str())),
            ("id", Json::from(id)),
            ("kind", Json::from(kind)),
            ("journal", Json::from(seq)),
            ("validated", Json::from(validated.as_str())),
        ]);
        if version > 0 {
            out.set("version", Json::from(version));
        } else {
            out.set("deleted", Json::from(true));
        }
        if !summary.is_null() {
            out.set(kind, summary);
        }
        if !warnings.is_empty() {
            out.set("warnings", Json::arr(warnings.into_iter().map(Json::from)));
        }
        ToolOutput::text(out.to_string())
    }

    fn anim_state(&mut self, key: &EntityKey, time_s: Option<f64>, window_s: f64) -> ToolOutput {
        let id = match self.world.resolve(key) {
            Ok(id) => id,
            Err(e) => return world_error(&e),
        };
        let e = self.world.get(id).expect("resolved").clone();
        let path = self.world.path_of(id);
        let Some(a) = e.anim.clone() else { return error("not_animated", format!("{path} has no anim (world_set it one: a clip name or {{graph, params}})")) };
        let t = time_s.unwrap_or_else(|| self.time_s());
        let set = crate::anim::anim_set_of(&self.world, &e);
        let mut out = Json::obj([("entity", Json::from(path.as_str())), ("time_s", round(t, 4))]);
        let Some(gp) = a.graph.as_deref() else {
            out.set("mode", Json::from("clip"));
            out.set("clip", Json::from(a.clip.as_str()));
            out.set("clip_time_s", round(a.clip_time(t), 4));
            out.set("speed", Json::from(a.speed));
            out.set("fade", round(a.fade_weight(t) as f64, 4));
            if let Some(f) = &a.from {
                out.set("from", Json::from(if f.graph.is_some() { "(a graph)" } else { f.clip.as_str() }));
            }
            if let Some(p) = models::anim_problem(&self.library.lock(), &e) {
                out.set("problems", Json::arr([Json::from(p)]));
            }
            return ToolOutput::text(out.to_string());
        };
        out.set("mode", Json::from("graph"));
        out.set("graph", Json::from(gp.graph.as_str()));
        out.set("speed", Json::from(a.speed));
        let Some(set) = set else {
            out.set("problems", Json::arr([Json::from("the model is still building (or has no anim set): the play isn't evaluated yet")]));
            return ToolOutput::text(out.to_string());
        };
        let Some(graph) = set.graph(&gp.graph) else {
            out.set("problems", Json::arr([Json::from(crate::anim::graph_problem(&set, &e, &a).unwrap_or_default())]));
            return ToolOutput::text(out.to_string());
        };
        let g = Graphed { set: &set, graph, scale: crate::anim::entity_scale(&e) };
        let timing = Timing { anchor: a.start.unwrap_or(0.0), speed: a.speed };
        let mut sc = Scratch::default();
        let layers = aplay::state_at(g, gp, timing, t, &mut sc);
        out.set(
            "layers",
            Json::arr(layers.iter().map(|l| {
                Json::obj([
                    ("layer", Json::from(l.layer.as_str())),
                    ("weight", round(l.weight as f64, 3)),
                    ("truncated", Json::from(l.truncated)),
                    (
                        "states",
                        Json::arr(l.stack.iter().map(|s| {
                            Json::obj([
                                ("state", Json::from(s.state.as_str())),
                                ("fade", round(s.fade as f64, 3)),
                                ("phase", round(s.phase, 4)),
                                ("rate_hz", round(s.rate, 4)),
                                ("clips", Json::arr(s.clips.iter().map(|(c, w)| Json::arr([Json::from(c.as_str()), round(*w as f64, 3)])))),
                            ])
                        })),
                    ),
                ])
            })),
        );
        // Parameters in effect.
        let params = Json::obj(graph.params.iter().map(|p| {
            let v = gp.param(&p.name).and_then(|v| p.accept(v).ok()).or_else(|| p.default.clone());
            (p.name.clone(), v.map_or(Json::Null, |v| value_json(&v)))
        }));
        out.set("params", params);
        let mut events = Vec::new();
        let dropped = aplay::events(g, gp, timing, t, t + window_s, &mut sc, &mut events);
        out.set(
            "events",
            Json::arr(events.iter().take(64).map(|ev| {
                let mut f = vec![("time_s", round(ev.time, 4)), ("layer", Json::from(ev.layer.as_str())), ("state", Json::from(ev.state.as_str())), ("name", Json::from(ev.name.as_str()))];
                if !ev.data.is_empty() {
                    f.push(("data", Json::from(ev.data.as_str())));
                }
                Json::obj(f)
            })),
        );
        if dropped || events.len() > 64 {
            out.set("events_truncated", Json::from(true));
        }
        let rm = aplay::root_motion(g, gp, timing, t, t + window_s, &mut sc);
        out.set("root_motion_m", v3(rm));
        let m = e.model();
        let world_rm = m.transform_point(Vec3::new(rm[0], rm[1], rm[2])) - m.transform_point(Vec3::ZERO);
        out.set("root_motion_world_m", v3([world_rm.x, world_rm.y, world_rm.z]));
        // What the IK did now (on the real ground).
        let mut player = GraphPlayer::new();
        player.report_ik = true;
        let ground = self.models.ground().cloned();
        let m = e.model();
        let c = &m.cols;
        let to_world = specula_anim::math::Affine { cols: [[c[0][0], c[0][1], c[0][2]], [c[1][0], c[1][1], c[1][2]], [c[2][0], c[2][1], c[2][2]]], t: [c[3][0], c[3][1], c[3][2]] };
        let around = Surroundings { to_world: Some(to_world), ground: ground.as_deref().map(|g| g as &dyn specula_anim::ik::Ground), external: None };
        player.evaluate(g, gp, timing, t, &around);
        out.set(
            "ik",
            Json::arr(player.ik.solvers.iter().map(|(n, w, feet)| {
                let mut f = vec![("name", Json::from(n.as_str())), ("weight", round(*w as f64, 3))];
                if let Some(r) = feet {
                    f.push(("pelvis_m", round(r.pelvis as f64, 4)));
                    f.push((
                        "feet",
                        Json::arr(r.legs[..r.count].iter().map(|l| {
                            Json::obj([
                                ("ground_m", l.ground.map_or(Json::Null, |g| round(g as f64, 4))),
                                ("offset_m", round(l.offset as f64, 4)),
                                ("miss_m", round(l.reach.miss as f64, 4)),
                            ])
                        })),
                    ));
                }
                Json::obj(f)
            })),
        );
        if ground.is_none() {
            out.set("ground", Json::from("none: no entity is tagged 'ground', so feet keep their animated height"));
        }
        let problems = aplay::bind_problems(graph, gp);
        if !problems.is_empty() {
            out.set("problems", Json::arr(problems.into_iter().map(Json::from)));
        }
        ToolOutput::text(out.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn anim_check(&mut self, source: Option<String>, key: Option<EntityKey>, graph_name: &str, clips: Option<Vec<String>>, states: Option<Vec<StateSubject>>, transitions: Vec<TransitionSubject>, scale: Option<f32>, max_checks: usize) -> ToolOutput {
        // An entity: what it plays now, at its own scale.
        let (source, entity) = match (source, key) {
            (Some(s), _) => (s, None),
            (None, Some(k)) => {
                let id = match self.world.resolve(&k) {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                let e = self.world.get(id).expect("resolved").clone();
                let Some(m) = e.model.as_ref() else { return error("not_a_model", format!("{} is not a model entity", self.world.path_of(id))) };
                (m.recipe.clone(), Some(e))
            }
            (None, None) => unreachable!("parse checked"),
        };
        let src = match self.anim_source(&source) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let rules = match self.motion_rules(&source) {
            Ok(r) => r,
            Err(e) => return e,
        };
        let set = &src.set;
        let mesh = src.built.model.lods.first();
        let rig = stem_of(&source);
        let mut report = Report::default();
        let mut graph_used = graph_name.to_string();
        let mut state_list: Vec<(String, Vec<(String, Value)>)> = Vec::new();
        let mut clip_list: Vec<String> = Vec::new();
        let mut scale = scale.unwrap_or(1.0);
        match &entity {
            Some(e) => {
                scale = crate::anim::entity_scale(e);
                let a = e.anim.as_ref();
                match a.and_then(|a| a.graph.as_deref()) {
                    Some(gp) => {
                        graph_used = gp.graph.clone();
                        if let Some(graph) = set.graph(&gp.graph) {
                            let mut sc = Scratch::default();
                            let timing = Timing { anchor: a.expect("graph").start.unwrap_or(0.0), speed: a.expect("graph").speed };
                            let now = aplay::state_at(Graphed { set, graph, scale }, gp, timing, self.time_s(), &mut sc);
                            if let Some(top) = now.first().and_then(|l| l.stack.last()) {
                                state_list.push((top.state.clone(), gp.params.clone()));
                            }
                        }
                    }
                    None => {
                        if let Some(a) = a {
                            clip_list.push(a.clip.clone());
                        }
                    }
                }
            }
            None => {
                clip_list = clips.unwrap_or_else(|| set.clips().map(|(_, c)| c.name.clone()).collect());
                if let Some(graph) = set.graph(graph_name) {
                    let subjects = states.unwrap_or_else(|| graph.layers[0].states.iter().map(|s| StateSubject { state: s.name.clone(), params: None }).collect());
                    for s in subjects {
                        match params_for(graph, s.params.as_ref(), Some(&s.state)) {
                            Ok(p) => state_list.push((s.state, p)),
                            Err(e) => return e,
                        }
                    }
                } else if states.is_some() || !transitions.is_empty() {
                    return error("unknown_graph", format!("{source} has no graph '{graph_name}' (graphs: {})", set.graphs().map(|(n, _)| n).collect::<Vec<_>>().join(", ")));
                }
            }
        }
        for c in &clip_list {
            let Some(id) = set.id(c) else { return error("unknown_clip", format!("{source} has no clip '{c}'")) };
            report.checks.extend(checks::check_clip(set, &rig, id, &rules, mesh).checks);
        }
        if let Some(graph) = set.graph(&graph_used) {
            let g = Graphed { set, graph, scale };
            for (state, params) in &state_list {
                match checks::check_state(g, &rig, &graph_used, state, params, 3.0, &rules, mesh) {
                    Ok(r) => {
                        report.notes.extend(r.notes);
                        report.checks.extend(r.checks);
                    }
                    Err(e) => return anim_error(&e, "states"),
                }
            }
            for t in &transitions {
                let from = match params_for(graph, Some(&t.from), None) {
                    Ok(p) => p,
                    Err(e) => return e,
                };
                let to = match params_for(graph, Some(&t.to), None) {
                    Ok(p) => p,
                    Err(e) => return e,
                };
                report.checks.extend(checks::check_transition(g, &rig, &graph_used, &from, &to, t.settle_s, &rules).checks);
            }
        }
        let mut out = report.to_json(max_checks);
        out.set("recipe", Json::from(source.as_str()));
        if let Some(e) = &entity {
            out.set("entity", Json::from(self.world.path_of(e.id)));
        }
        out.set("subjects", Json::obj([("clips", Json::from(clip_list.len())), ("states", Json::from(state_list.len())), ("transitions", Json::from(transitions.len()))]));
        out.set("counts", Json::obj([("checks", Json::from(report.checks.len())), ("failed", Json::from(report.failures().count()))]));
        out.set("rules", rules.to_json());
        if !src.problems.is_empty() {
            out.set("asset_problems", Json::arr(src.problems.iter().map(|p| Json::from(p.as_str()))));
        }
        ToolOutput::text(out.to_string())
    }

    /// Renders one stage tile: `draws` (with `bones`) through `eye` → `target`, into `sheet`.
    #[allow(clippy::too_many_arguments)]
    fn film_tile(&mut self, eye: Vec3, target: Vec3, fov: f32, draws: &[DrawItem], bones: &[[f32; 16]], tile: u32, sheet: &mut [u8], sw: u32, x0: u32, y0: u32) -> Result<(), String> {
        let view_proj = (Mat4::perspective_vk_reverse_z(fov.to_radians(), 1.0, 0.05) * Mat4::look_at_rh(eye, target, Vec3::new(0.0, 1.0, 0.0))).to_cols_array();
        let target_id = self.offscreen_target(tile, tile)?;
        let srgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
        let frame = FrameDesc {
            view_proj,
            clear_color: srgb(0x0b, 0x10, 0x20),
            draws,
            reverse_z: true,
            environment: stage(),
            lights: &[],
            time_s: 0.0,
            overlay: &[],
            draws_2d: &[],
            bones,
        };
        self.gpu.render(target_id, &frame).map_err(|e| capture::gpu_failure("render", &e, self.windowed))?;
        let px = self.gpu.read_rgba8(target_id).map_err(|e| capture::gpu_failure("readback", &e, self.windowed))?;
        film::blit(sheet, sw, x0, y0, &px, tile, tile);
        Ok(())
    }

    /// Saves a filmstrip sheet and its packet; returns the tool output.
    fn film_output(&mut self, sw: u32, sh: u32, sheet: &[u8], mut packet: Json) -> ToolOutput {
        let png_bytes = png::encode_rgba8(sw, sh, sheet, Level::Fastest);
        self.frames_captured += 1;
        let png_path = self.save_capture("png", &png_bytes);
        let mut warnings = Vec::new();
        if let Err(e) = &png_path {
            warnings.push(Json::from(format!("capture not saved to disk: {e}")));
        }
        let png_file = png_path.as_ref().ok();
        let packet_path = png_file.map(|p| p.with_extension("json"));
        packet.set(
            "image",
            Json::obj([
                ("width", Json::from(sw)),
                ("height", Json::from(sh)),
                ("png_bytes", Json::from(png_bytes.len())),
                ("path", png_file.map(|p| p.display().to_string()).into()),
                ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
            ]),
        );
        packet.set("frame", Json::from(self.frames_captured));
        if let Some(p) = &packet_path {
            if let Err(e) = std::fs::write(p, packet.to_pretty()) {
                warnings.push(Json::from(format!("packet not saved to disk: {e}")));
            }
        }
        if !warnings.is_empty() {
            packet.set("warnings", Json::Arr(warnings));
        }
        ToolOutput { content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_string())], is_error: false }
    }

    #[allow(clippy::too_many_arguments)]
    fn anim_film_rows(&mut self, source: &str, graph_name: &str, rows: Vec<Row>, frames: u32, tile: u32, scale: Option<f32>, with_checks: bool) -> ToolOutput {
        let src = match self.anim_source(source) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let rules = match self.motion_rules(source) {
            Ok(r) => r,
            Err(e) => return e,
        };
        let set = Arc::clone(&src.set);
        let mesh = src.built.model.lods.first();
        let rig = stem_of(source);
        let scale = scale.unwrap_or(1.0);
        let rows = if rows.is_empty() { set.clips().take(MAX_ROWS).map(|(_, c)| Row::Clip(c.name.clone())).collect() } else { rows };
        let n = frames.max(2) as usize;
        // Poses first (CPU), so a bad row fails before any rendering.
        let mut film: Vec<FilmRow> = Vec::new();
        for row in &rows {
            match row {
                Row::Clip(name) => {
                    let Some(id) = set.id(name) else { return error("unknown_clip", format!("{source} has no clip '{name}'")) };
                    let c = set.clip(id);
                    let span = if c.looping == LoopMode::Loop { n as f32 } else { (n - 1) as f32 };
                    let times: Vec<f64> = (0..n).map(|k| (c.duration * k as f32 / span) as f64).collect();
                    let poses = times
                        .iter()
                        .map(|&t| {
                            let mut p = set.skeleton.rest_pose();
                            blend::sample(&set.skeleton, c, set.track_bones(id), t as f32, blend::Base::Rest, &mut p);
                            p
                        })
                        .collect();
                    let report = with_checks.then(|| checks::check_clip(&set, &rig, id, &rules, mesh));
                    let travel = times.iter().map(|t| c.speed() * *t as f32).collect();
                    film.push(FilmRow { label: format!("{} {:.2}S {}", c.name, c.duration, c.looping.name()), subject: format!("{rig}/{}", c.name), times, poses, report, travel });
                }
                Row::State(s) => {
                    let Some(graph) = set.graph(graph_name) else { return error("unknown_graph", format!("{source} has no graph '{graph_name}'")) };
                    let params = match params_for(graph, s.params.as_ref(), Some(&s.state)) {
                        Ok(p) => p,
                        Err(e) => return e,
                    };
                    if graph.layers[0].state(&s.state).is_none() {
                        return error("unknown_state", format!("graph '{graph_name}' has no state '{}'", s.state));
                    }
                    let g = Graphed { set: &set, graph, scale };
                    let play = held_play(graph_name, &s.state, &params);
                    let timing = Timing { anchor: 0.0, speed: 1.0 };
                    let mut sc = Scratch::default();
                    let rate = aplay::state_at(g, &play, timing, 0.0, &mut sc)[0].stack.last().map_or(1.0, |e| e.rate).max(1e-3);
                    let cycle = (1.0 / rate).min(10.0);
                    let times: Vec<f64> = (0..n).map(|k| cycle * k as f64 / n as f64).collect();
                    let mut player = GraphPlayer::new();
                    let poses = times.iter().map(|&t| player.evaluate(g, &play, timing, t, &Surroundings::default()).clone()).collect();
                    let report = if with_checks { Some(checks::check_state(g, &rig, graph_name, &s.state, &params, 3.0, &rules, mesh).unwrap_or_default()) } else { None };
                    let st = &graph.layers[0].states[graph.layers[0].state(&s.state).expect("checked") as usize];
                    let v = st.match_speed.map_or(0.0, |p| match play.param(&graph.params[p as usize].name) {
                        Some(Value::Num(x)) => *x,
                        _ => 0.0,
                    }) / scale;
                    let travel = times.iter().map(|t| v * *t as f32).collect();
                    let shown: Vec<String> = params.iter().map(|(n, v)| format!("{n}={}", short(v))).collect();
                    film.push(FilmRow { label: format!("{}.{} {}", graph_name, s.state, shown.join(" ").replace('"', "")), subject: format!("{rig}/{graph_name}.{}", s.state), times, poses, report, travel });
                }
                Row::Transition(tr) => {
                    let Some(graph) = set.graph(graph_name) else { return error("unknown_graph", format!("{source} has no graph '{graph_name}'")) };
                    let (from, to) = match (params_for(graph, Some(&tr.from), None), params_for(graph, Some(&tr.to), None)) {
                        (Ok(a), Ok(b)) => (a, b),
                        (Err(e), _) | (_, Err(e)) => return e,
                    };
                    let g = Graphed { set: &set, graph, scale };
                    let mut before = GraphPlay::new(graph_name);
                    for (k, v) in &from {
                        before.set_param(k, v.clone());
                    }
                    let t0 = Timing { anchor: 0.0, speed: 1.0 };
                    let ts = tr.settle_s as f64;
                    let mut sc = Scratch::default();
                    let (mut after, _) = aplay::settle(g, &before, t0, ts, &mut sc);
                    for (k, v) in &to {
                        after.set_param(k, v.clone());
                    }
                    let t1 = Timing { anchor: ts, speed: 1.0 };
                    let blend_s = graph.layers.iter().flat_map(|l| l.transitions.iter().map(|t| t.blend)).fold(0.0f32, f32::max).min(2.0) as f64;
                    let (w0, w1) = (ts - 0.15, ts + blend_s + 0.25);
                    let times: Vec<f64> = (0..n).map(|k| w0 + (w1 - w0) * k as f64 / (n - 1) as f64).collect();
                    let mut player = GraphPlayer::new();
                    let poses = times
                        .iter()
                        .map(|&t| if t < ts { player.evaluate(g, &before, t0, t, &Surroundings::default()).clone() } else { player.evaluate(g, &after, t1, t, &Surroundings::default()).clone() })
                        .collect();
                    let report = with_checks.then(|| checks::check_transition(g, &rig, graph_name, &from, &to, tr.settle_s, &rules));
                    let name_of = |p: &GraphPlay, tm: Timing, t: f64, sc: &mut Scratch| aplay::state_at(g, p, tm, t, sc)[0].stack.last().map(|e| e.state.clone()).unwrap_or_default();
                    let (a, b) = (name_of(&before, t0, ts, &mut sc), name_of(&after, t1, w1, &mut sc));
                    film.push(FilmRow { label: format!("{graph_name}.{a}->{b} at {:.2}S", 0.15), subject: format!("{rig}/{graph_name}.{a}->{b}"), times, poses, report, travel: vec![0.0; n] });
                }
            }
        }
        // Layout: frames in a row, a label strip under each row.
        let label_h = 2 * film::CHAR_H + 6;
        let (sw, sh) = (n as u32 * tile, film.len() as u32 * (tile + label_h));
        let mut sheet = vec![0u8; (sw * sh * 4) as usize];
        film::fill(&mut sheet, sw, 0, 0, sw, sh, [14, 17, 26, 255]);
        let ground_mesh = self.meshes.get(Shape::Quad);
        let skel = &set.skeleton;
        let mut out_rows = Vec::new();
        let (mut model, mut skin) = (Vec::new(), Vec::new());
        for (ri, row) in film.iter().enumerate() {
            let y0 = ri as u32 * (tile + label_h);
            // One camera per row: fit every pose (side view, from the creature's left).
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for (k, p) in row.poses.iter().enumerate() {
                skel.model_space(p, &mut model);
                for (b, m) in model.iter().enumerate() {
                    let tail = m.apply_point(specula_anim::math::math3::sub(skel.tails[b], skel.heads[b]));
                    for q in [m.t, tail] {
                        let q = [q[0], q[1], q[2] + row.travel[k] * 0.0];
                        for i in 0..3 {
                            lo[i] = lo[i].min(q[i]);
                            hi[i] = hi[i].max(q[i]);
                        }
                    }
                }
            }
            lo[1] = lo[1].min(0.0);
            let centre = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
            // Joints and bone ends lie inside the mesh: a margin for its skin.
            let radius = 0.58 * specula_anim::math::math3::length([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) + 0.1;
            let fov = 30.0;
            let eye = film::frame_eye(centre, radius, 90.0, 12.0, fov);
            let mut tiles = Vec::new();
            for (k, pose) in row.poses.iter().enumerate() {
                skel.model_space(pose, &mut model);
                skel.skin_matrices(&model, &mut skin);
                let mut draws = Vec::new();
                let mut bones = Vec::new();
                if let Some(gm) = ground_mesh {
                    let g = Mat4::from_trs(Vec3::ZERO, specula_core::math::Quat::IDENTITY, Vec3::new(40.0, 1.0, 40.0));
                    let mut d = DrawItem::new(gm, g.to_cols_array(), [0x30 as f32 / 255.0, 0x35 as f32 / 255.0, 0x3d as f32 / 255.0, 1.0]);
                    d.surface = specula_gpu::Surface { roughness: 0.9, specular: 0.05, ..specula_gpu::Surface::default() };
                    draws.push(d);
                }
                if let Err(e) = self.models.posed_draws(&mut self.gpu, &src.built, &skin, &Mat4::IDENTITY, &mut bones, &mut draws) {
                    return error("render_unavailable", e);
                }
                let x0 = k as u32 * tile;
                if let Err(e) = self.film_tile(Vec3::new(eye[0], eye[1], eye[2]), Vec3::new(centre[0], centre[1], centre[2]), fov, &draws, &bones, tile, &mut sheet, sw, x0, y0) {
                    return ToolOutput::error(e);
                }
                film::draw_text(&mut sheet, sw, x0 + 3, y0 + 3, &format!("{:.2}", row.times[k]), [200, 200, 190]);
                tiles.push(Json::arr([x0, y0, tile, tile].map(Json::from)));
            }
            // Labels: the subject, and the checks that failed (or "checks ok").
            film::draw_text(&mut sheet, sw, 4, y0 + tile + 2, &row.label, [230, 226, 214]);
            let verdict = match &row.report {
                None => String::new(),
                Some(r) if r.passed() => "CHECKS OK".to_string(),
                Some(r) => r.failures().take(3).map(|c| format!("FAIL {} {:.1}{}", c.check, c.value, c.unit)).collect::<Vec<_>>().join("  "),
            };
            let colour = if row.report.as_ref().is_some_and(|r| !r.passed()) { [255, 120, 100] } else { [140, 220, 160] };
            film::draw_text(&mut sheet, sw, 4, y0 + tile + 4 + film::CHAR_H, &verdict, colour);
            let mut j = Json::obj([
                ("subject", Json::from(row.subject.as_str())),
                ("label", Json::from(row.label.as_str())),
                ("times_s", Json::arr(row.times.iter().map(|t| round(*t, 4)))),
                ("tiles", Json::Arr(tiles)),
            ]);
            if let Some(r) = &row.report {
                j.set("checks", r.to_json(12));
            }
            out_rows.push(j);
        }
        let failed: usize = film.iter().filter_map(|r| r.report.as_ref()).map(|r| r.failures().count()).sum();
        let packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("anim_filmstrip")),
            ("recipe", Json::from(source)),
            ("view", Json::from("side (from the model's left, +X), 12 deg up, neutral stage with a ground plane at y = 0")),
            ("frames", Json::from(n)),
            ("tile", Json::from(tile)),
            ("rows", Json::Arr(out_rows)),
            ("checks_failed", Json::from(failed)),
            ("rules", rules.to_json()),
        ]);
        self.film_output(sw, sh, &sheet, packet)
    }

    fn anim_film_entity(&mut self, key: &EntityKey, frames: u32, tile: u32, window_s: f32, with_checks: bool) -> ToolOutput {
        let id = match self.world.resolve(key) {
            Ok(id) => id,
            Err(e) => return world_error(&e),
        };
        let path = self.world.path_of(id);
        let Some(e) = self.world.get(id).cloned() else { return error("not_found", "gone") };
        if e.anim.is_none() {
            return error("not_animated", format!("{path} has no anim to film"));
        }
        // The game brings its presentation up to date first (as for captures).
        let _ = self.before_capture();
        let n = frames.max(2) as usize;
        let t0 = self.time_s();
        let times: Vec<f64> = (0..n).map(|k| t0 + window_s as f64 * k as f64 / (n - 1) as f64).collect();
        let (lo, hi) = capture::subtree_bounds(&self.world, id).unwrap_or((e.world_position(), e.world_position()));
        let centre = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(0.5) * 1.15;
        // Along its way (lanes run between raised blocks) and high enough to see over them.
        let views = [(35.0f32, 24.0f32, "front quarter"), (-145.0, 24.0, "back quarter")];
        let label_h = 2 * film::CHAR_H + 6;
        let (sw, sh) = (n as u32 * tile, views.len() as u32 * (tile + label_h));
        let mut sheet = vec![0u8; (sw * sh * 4) as usize];
        film::fill(&mut sheet, sw, 0, 0, sw, sh, [14, 17, 26, 255]);
        // The entity's own facing: views turn with it.
        let yaw = {
            let m = e.model();
            let f = m.transform_point(Vec3::new(0.0, 0.0, 1.0)) - m.transform_point(Vec3::ZERO);
            f.x.atan2(f.z).to_degrees()
        };
        let overlays = sight_overlay::Spec::default();
        let mut rows = Vec::new();
        for (vi, (az, el, name)) in views.iter().enumerate() {
            let y0 = vi as u32 * (tile + label_h);
            let eye = film::frame_eye([centre.x, centre.y, centre.z], radius, az + yaw, *el, 35.0);
            let camera = Camera::new(Vec3::new(eye[0], eye[1], eye[2]), centre, 35.0);
            let mut tiles = Vec::new();
            for (k, &t) in times.iter().enumerate() {
                let opts = capture::ViewOpts { include_ui: false, lod: Some(0), time_s: t, overlays: &overlays, caption: None, ids: false };
                let r = match self.render_view(camera, tile, tile, &opts) {
                    Ok(r) => r,
                    Err(e) => return ToolOutput::error(e),
                };
                let x0 = k as u32 * tile;
                film::blit(&mut sheet, sw, x0, y0, &r.rgba, tile, tile);
                film::draw_text(&mut sheet, sw, x0 + 3, y0 + 3, &format!("+{:.2}", t - t0), [200, 200, 190]);
                tiles.push(Json::arr([x0, y0, tile, tile].map(Json::from)));
            }
            film::draw_text(&mut sheet, sw, 4, y0 + tile + 2, &format!("{} {name}", path.rsplit('/').next().unwrap_or(&path)), [230, 226, 214]);
            rows.push(Json::obj([("view", Json::from(*name)), ("tiles", Json::Arr(tiles))]));
        }
        let state = self.anim_state(&EntityKey::Id(id), Some(t0), window_s as f64);
        let state_json = state.content.iter().find_map(|c| match c {
            Content::Text(t) => Json::parse(t).ok(),
            _ => None,
        });
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("anim_filmstrip")),
            ("entity", Json::from(path.as_str())),
            ("times_s", Json::arr(times.iter().map(|t| round(*t, 4)))),
            ("frames", Json::from(n)),
            ("tile", Json::from(tile)),
            ("rows", Json::Arr(rows)),
            ("note", Json::from("the entity's play at future times in the world as it is now (a moving entity is filmed in place)")),
        ]);
        if let Some(s) = state_json {
            packet.set("state", s);
        }
        if with_checks {
            let check = self.anim_check(None, Some(EntityKey::Id(id)), "creature", None, None, Vec::new(), None, 12);
            if let Some(Content::Text(t)) = check.content.first() {
                if let Ok(j) = Json::parse(t) {
                    packet.set("checks", j);
                }
            }
        }
        self.film_output(sw, sh, &sheet, packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_validates_and_definitions_are_well_formed() {
        let ok = |name: &str, a: &str| parse(name, &Json::parse(a).unwrap()).expect("an anim tool").unwrap_or_else(|e| panic!("{name} {a}: {e}"));
        let bad = |name: &str, a: &str| parse(name, &Json::parse(a).unwrap()).expect("an anim tool").err().unwrap_or_else(|| panic!("{name} {a} should fail"));
        assert!(matches!(ok("anim_get", r#"{"recipe": "recipes/shambler.json"}"#), AnimCall::Get { .. }));
        assert!(bad("anim_get", r#"{}"#).contains("recipe"));
        assert!(bad("anim_get", r#"{"recipe": "../x.json"}"#).contains("assets root"));
        assert!(matches!(ok("anim_set", r#"{"recipe": "r.json", "name": "wave", "clip": {"kind": "pose", "pose": {}}}"#), AnimCall::Set { what: SetWhat::Clip(_), .. }));
        assert!(bad("anim_set", r#"{"recipe": "r.json", "name": "wave", "clip": {}, "graph": {}}"#).contains("exactly one"));
        assert!(bad("anim_set", r#"{"recipe": "r.json", "name": "9lives", "delete": true}"#).contains("identifier"));
        assert!(matches!(ok("anim_state", r#"{"path": "/live/x", "window_s": 3}"#), AnimCall::State { .. }));
        assert!(bad("anim_state", r#"{"path": "/x", "window_s": 99}"#).contains("window_s"));
        assert!(bad("anim_check", r#"{"recipe": "r.json", "id": 3}"#).contains("not both"));
        assert!(matches!(ok("anim_check", r#"{"recipe": "r.json", "states": ["idle", {"state": "move", "params": {"speed": 1}}], "transitions": [{"from": {"act": "idle"}, "to": {"act": "move"}}]}"#), AnimCall::Check { .. }));
        assert!(bad("anim_check", r#"{"recipe": "r.json", "transitions": [{"from": {}}]}"#).contains("to"));
        assert!(matches!(ok("anim_filmstrip", r#"{"recipe": "r.json", "rows": [{"clip": "walk"}, {"state": "move"}, {"transition": {"from": {}, "to": {}}}], "frames": 6}"#), AnimCall::Filmstrip { .. }));
        assert!(bad("anim_filmstrip", r#"{"id": 4, "rows": [{"clip": "walk"}]}"#).contains("own play"));
        assert!(bad("anim_filmstrip", r#"{"recipe": "r.json", "frames": 40}"#).contains("frames"));
        assert!(parse("world_spawn", &Json::Null).is_none());
        for d in definitions() {
            assert!(d.name.len() <= 64 && d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'), "{}", d.name);
            assert!(d.description.len() > 80, "{} is documented", d.name);
            assert_eq!(d.input_schema.get("type"), Some(&Json::from("object")));
        }
    }

    #[test]
    fn merge_patches_and_stems() {
        let a = Json::parse(r#"{"states": {"idle": "idle", "move": "walk"}, "entry": "idle"}"#).unwrap();
        let p = Json::parse(r#"{"states": {"move": "run", "idle": null}, "entry": "move"}"#).unwrap();
        assert_eq!(merge_patch(&a, &p), Json::parse(r#"{"states": {"move": "run"}, "entry": "move"}"#).unwrap());
        assert_eq!(stem_of("recipes/shambler.json"), "shambler");
        assert_eq!(stem_of("models/hero.glb"), "hero");
        assert_eq!(stem_of("a b/odd name!.json"), "odd_name_");
    }
}
