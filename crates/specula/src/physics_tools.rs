//! The physics tools agents call (`physics_*`), their schemas, and the `colliders` and
//! `contacts` sight overlays.
//!
//! - Queries: `physics_raycast`, `physics_cast` (sweep a sphere, capsule or box),
//!   `physics_overlap`. Hits come sorted (distance, then id) and bounded.
//! - State as data: `physics_contacts` (pairs and points), `physics_events` (contact begin/end,
//!   trigger enter/exit, tunnelling-guard stops, with a cursor), `physics_info` (settings,
//!   counts, stage times, budget errors, the physics hash, automatic checks), `physics_explain`
//!   (why an entity moves, rests or passes through: contacts, neighbours and the layer mask that
//!   filtered them, the tunnelling indicator).
//! - Checks as data: `physics_check` asserts rest, contacts, penetration, tunnelling, energy and
//!   positions over the recorded window, for headless scenario tests.
//! - `physics_impulse` changes a body's velocity (journaled and undoable like any set).
//!
//! Bodies and colliders themselves are set through `world_spawn` / `world_set` (`collider`,
//! `body`) and the settings through `world_env {physics}`; time steps through `time_control`.

use crate::journal::{Kind as JKind, Writer};
use crate::physics::{self, Body, BodyKind};
use crate::schedule::Class;
use crate::sight_overlay::{self as ov, Painter};
use crate::tools::{bool_arg, check_keys, entity_key, int_arg, key_value, num_in, string_arg, vec3_arg};
use crate::world::{EntityKey, UpdateSpec, World, WorldError};
use specula_core::math::{Quat, Vec3};
use specula_core::Json;
use specula_physics::{EventKind, Filter, Pose, Shape};
use specula_protocol::{ToolDef, ToolOutput};

/// Most hits a query returns.
pub const MAX_HITS: usize = 64;
/// Most items a listing returns per call.
pub const MAX_LIST: usize = 256;
/// Bodies and contact points the overlays draw at most.
const MAX_DRAWN_BODIES: usize = 400;
const MAX_DRAWN_TRIANGLES: usize = 3000;
const MAX_DRAWN_CONTACTS: usize = 512;

/// A query shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QueryShape {
    Sphere { radius: f32 },
    /// Total height along local +Y.
    Capsule { radius: f32, height: f32 },
    /// Full extents.
    Box { size: Vec3 },
}

impl QueryShape {
    fn shape(&self) -> Shape {
        match *self {
            QueryShape::Sphere { radius } => Shape::sphere(radius),
            QueryShape::Capsule { radius, height } => Shape::capsule(radius, (height * 0.5 - radius).max(0.0)),
            QueryShape::Box { size } => Shape::cuboid(size * 0.5),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryFilter {
    /// Layer names to hit (`None` = all).
    pub layers: Option<Vec<String>>,
    pub triggers: bool,
    pub exclude: Option<EntityKey>,
}

/// Which bodies a check looks at.
#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    All,
    /// This entity and everything below it.
    Under(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum CheckSpec {
    AtRest { scope: Scope, max_speed: f32 },
    Touching { a: EntityKey, b: EntityKey, expect: bool },
    NoPenetration { max_depth: f32 },
    NoTunnelling,
    Energy { max_gain_j: f32 },
    Inside { entity: EntityKey, min: Vec3, max: Vec3 },
    /// `joint_error` / `joint_limits` (see [`crate::joint_tools::run_check`]).
    Joint { name: &'static str, args: Json },
}

pub enum PhysicsCall {
    Raycast { origin: Vec3, direction: Vec3, max_distance: f32, filter: QueryFilter, max_hits: usize },
    Cast { shape: QueryShape, from: Vec3, to: Vec3, rotation_deg: Vec3, filter: QueryFilter, max_hits: usize },
    Overlap { shape: QueryShape, at: Vec3, rotation_deg: Vec3, filter: QueryFilter, limit: usize },
    Contacts { entity: Option<EntityKey>, touching_only: bool, limit: usize, after: Option<(u64, u64)> },
    Events { after: u64, limit: usize, kinds: Vec<EventKind>, entity: Option<EntityKey> },
    Info,
    Explain { entity: EntityKey },
    Check { checks: Vec<CheckSpec>, since_tick: Option<u64> },
    Impulse { entity: EntityKey, impulse: Vec3, at: Option<Vec3>, angular: Vec3 },
    /// `physics_joint` / `physics_joints` (see [`crate::joint_tools`]).
    Joint(crate::joint_tools::JointCall),
    /// `ragdoll_set` / `ragdoll_info` (see [`crate::ragdoll_tools`]).
    Ragdoll(crate::ragdoll_tools::RagdollCall),
}

impl PhysicsCall {
    /// How the engine's schedule treats the call.
    pub fn class(&self) -> Class {
        match self {
            PhysicsCall::Impulse { .. } => Class::Edit,
            PhysicsCall::Joint(j) if !j.is_read() => Class::Edit,
            PhysicsCall::Ragdoll(r) if !r.is_read() => Class::Edit,
            _ => Class::Read,
        }
    }
}

const EVENT_KINDS: [EventKind; 5] = [EventKind::ContactBegin, EventKind::ContactEnd, EventKind::TriggerEnter, EventKind::TriggerExit, EventKind::Ccd];

fn shape_arg(args: &Json) -> Result<QueryShape, String> {
    const ERR: &str = "'shape' must be {shape: sphere (radius) | capsule (radius, height) | box (size [x,y,z])}";
    let s = args.get("shape").ok_or(ERR)?;
    check_keys(s, "shape", &[&["shape", "radius", "height", "size"]])?;
    let size = |k: &str| num_in(s, k, 1.0e-4..=1.0e4).map_err(|e| format!("shape: {e}"));
    match s.get("shape").and_then(Json::as_str) {
        Some("sphere") => Ok(QueryShape::Sphere { radius: size("radius")?.unwrap_or(0.5) }),
        Some("capsule") => {
            let radius = size("radius")?.ok_or("shape: a capsule needs 'radius'")?;
            let height = size("height")?.ok_or("shape: a capsule needs 'height' (total, >= 2 x radius)")?;
            if height < 2.0 * radius {
                return Err("shape: a capsule's 'height' is the total height, at least 2 x radius".into());
            }
            Ok(QueryShape::Capsule { radius, height })
        }
        Some("box") => {
            let size = vec3_arg(s, "size")?.unwrap_or(Vec3::ONE);
            if ![size.x, size.y, size.z].iter().all(|c| (1.0e-4..=1.0e4).contains(c)) {
                return Err("shape: 'size' components must be 0.0001..10000".into());
            }
            Ok(QueryShape::Box { size })
        }
        _ => Err(ERR.into()),
    }
}

fn filter_arg(args: &Json, triggers_default: bool) -> Result<QueryFilter, String> {
    let layers = match args.get("layers") {
        None | Some(Json::Null) => None,
        Some(v) => Some(
            v.as_array()
                .ok_or("'layers' must be an array of layer names")?
                .iter()
                .map(|l| l.as_str().map(str::to_owned).ok_or("'layers' must be an array of layer names"))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    let exclude = match args.get("exclude") {
        None | Some(Json::Null) => None,
        Some(v) => Some(key_value(v, "exclude")?),
    };
    Ok(QueryFilter { layers, triggers: bool_arg(args, "triggers")?.unwrap_or(triggers_default), exclude })
}

fn point_arg(args: &Json, key: &str) -> Result<Vec3, String> {
    let p = vec3_arg(args, key)?.ok_or_else(|| format!("'{key}' is required: [x, y, z]"))?;
    crate::world::check_point(key, p).map_err(|e| e.message)?;
    Ok(p)
}

fn scope_arg(v: Option<&Json>) -> Result<Scope, String> {
    match v {
        None | Some(Json::Null) => Ok(Scope::All),
        Some(Json::Str(s)) if s == "/" || s == "*" => Ok(Scope::All),
        Some(Json::Str(s)) if s.starts_with('/') => Ok(Scope::Under(s.trim_end_matches('/').to_owned())),
        Some(_) => Err("'entities' must be a path (\"/tower\": it and everything below) or \"/\" for all".into()),
    }
}

fn check_arg(c: &Json) -> Result<CheckSpec, String> {
    let kind = c.get("check").and_then(Json::as_str).ok_or("each check needs 'check': at_rest, touching, not_touching, no_penetration, no_tunnelling, energy, inside, joint_error or joint_limits")?;
    let num = |k: &str, default: f32| -> Result<f32, String> { Ok(num_in(c, k, 0.0..=1.0e6)?.unwrap_or(default)) };
    let key = |k: &str| -> Result<EntityKey, String> { key_value(c.get(k).ok_or_else(|| format!("check '{kind}' needs '{k}' (an id or path)"))?, k) };
    match kind {
        "at_rest" => {
            check_keys(c, "check", &[&["check", "entities", "max_speed"]])?;
            Ok(CheckSpec::AtRest { scope: scope_arg(c.get("entities"))?, max_speed: num("max_speed", 0.05)? })
        }
        "touching" | "not_touching" => {
            check_keys(c, "check", &[&["check", "a", "b"]])?;
            Ok(CheckSpec::Touching { a: key("a")?, b: key("b")?, expect: kind == "touching" })
        }
        "no_penetration" => {
            check_keys(c, "check", &[&["check", "max_depth"]])?;
            Ok(CheckSpec::NoPenetration { max_depth: num("max_depth", 0.02)? })
        }
        "no_tunnelling" => {
            check_keys(c, "check", &[&["check"]])?;
            Ok(CheckSpec::NoTunnelling)
        }
        "energy" => {
            check_keys(c, "check", &[&["check", "max_gain_j"]])?;
            Ok(CheckSpec::Energy { max_gain_j: num("max_gain_j", 0.5)? })
        }
        "inside" => {
            check_keys(c, "check", &[&["check", "entity", "min", "max"]])?;
            Ok(CheckSpec::Inside { entity: key("entity")?, min: point_arg(c, "min")?, max: point_arg(c, "max")? })
        }
        "joint_error" | "joint_limits" => {
            let (name, keys): (&'static str, &[&str]) =
                if kind == "joint_error" { ("joint_error", &["check", "max_m", "max_deg"]) } else { ("joint_limits", &["check", "max_deg", "max_m"]) };
            check_keys(c, "check", &[keys])?;
            for k in &keys[1..] {
                num(k, 0.0)?;
            }
            Ok(CheckSpec::Joint { name, args: c.clone() })
        }
        other => Err(format!("unknown check '{other}' (at_rest, touching, not_touching, no_penetration, no_tunnelling, energy, inside, joint_error, joint_limits)")),
    }
}

/// Parses a physics tool call (`None`: not a physics tool).
pub fn parse(name: &str, args: &Json) -> Option<Result<PhysicsCall, String>> {
    let call = match name {
        "physics_raycast" => (|| {
            check_keys(args, name, &[&["origin", "direction", "max_distance", "layers", "triggers", "exclude", "max_hits"]])?;
            let direction = vec3_arg(args, "direction")?.ok_or("'direction' is required: [x, y, z]")?;
            if direction.length() < 1.0e-9 {
                return Err("'direction' must not be zero".to_string());
            }
            Ok(PhysicsCall::Raycast {
                origin: point_arg(args, "origin")?,
                direction,
                max_distance: num_in(args, "max_distance", 0.0..=1.0e6)?.unwrap_or(1000.0),
                filter: filter_arg(args, false)?,
                max_hits: int_arg(args, "max_hits", 1, MAX_HITS as u64)?.unwrap_or(1) as usize,
            })
        })(),
        "physics_cast" => (|| {
            check_keys(args, name, &[&["shape", "from", "to", "rotation_deg", "layers", "triggers", "exclude", "max_hits"]])?;
            Ok(PhysicsCall::Cast {
                shape: shape_arg(args)?,
                from: point_arg(args, "from")?,
                to: point_arg(args, "to")?,
                rotation_deg: vec3_arg(args, "rotation_deg")?.unwrap_or(Vec3::ZERO),
                filter: filter_arg(args, false)?,
                max_hits: int_arg(args, "max_hits", 1, MAX_HITS as u64)?.unwrap_or(1) as usize,
            })
        })(),
        "physics_overlap" => (|| {
            check_keys(args, name, &[&["shape", "at", "rotation_deg", "layers", "triggers", "exclude", "limit"]])?;
            Ok(PhysicsCall::Overlap {
                shape: shape_arg(args)?,
                at: point_arg(args, "at")?,
                rotation_deg: vec3_arg(args, "rotation_deg")?.unwrap_or(Vec3::ZERO),
                filter: filter_arg(args, true)?,
                limit: int_arg(args, "limit", 1, MAX_LIST as u64)?.unwrap_or(64) as usize,
            })
        })(),
        "physics_contacts" => (|| {
            check_keys(args, name, &[&["entity", "touching", "limit", "cursor"]])?;
            let after = match string_arg(args, "cursor")? {
                None => None,
                Some(c) => {
                    let (a, b) = c.split_once(':').ok_or("'cursor' must be the next_cursor of an earlier physics_contacts answer")?;
                    Some((a.parse().map_err(|_| "bad 'cursor'")?, b.parse().map_err(|_| "bad 'cursor'")?))
                }
            };
            let entity = match args.get("entity") {
                None | Some(Json::Null) => None,
                Some(v) => Some(key_value(v, "entity")?),
            };
            Ok(PhysicsCall::Contacts {
                entity,
                touching_only: bool_arg(args, "touching")?.unwrap_or(true),
                limit: int_arg(args, "limit", 1, MAX_LIST as u64)?.unwrap_or(64) as usize,
                after,
            })
        })(),
        "physics_events" => (|| {
            check_keys(args, name, &[&["after", "limit", "kinds", "entity"]])?;
            let kinds = match args.get("kinds") {
                None | Some(Json::Null) => Vec::new(),
                Some(v) => v
                    .as_array()
                    .ok_or("'kinds' must be an array of event kinds")?
                    .iter()
                    .map(|k| {
                        let n = k.as_str().unwrap_or_default();
                        EVENT_KINDS.into_iter().find(|e| e.name() == n).ok_or_else(|| {
                            format!("unknown event kind '{n}' (one of: {})", EVENT_KINDS.map(EventKind::name).join(", "))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            };
            let entity = match args.get("entity") {
                None | Some(Json::Null) => None,
                Some(v) => Some(key_value(v, "entity")?),
            };
            Ok(PhysicsCall::Events {
                after: int_arg(args, "after", 0, u64::MAX)?.unwrap_or(0),
                limit: int_arg(args, "limit", 1, MAX_LIST as u64)?.unwrap_or(64) as usize,
                kinds,
                entity,
            })
        })(),
        "physics_info" => check_keys(args, name, &[&[]]).map(|()| PhysicsCall::Info),
        "physics_explain" => check_keys(args, name, &[&["id", "path", "name"]]).and_then(|()| Ok(PhysicsCall::Explain { entity: entity_key(args, name)? })),
        "physics_check" => (|| {
            check_keys(args, name, &[&["checks", "since_tick"]])?;
            let list = args.get("checks").and_then(Json::as_array).ok_or("'checks' must be an array of checks")?;
            if list.is_empty() || list.len() > 64 {
                return Err("'checks' must list 1..64 checks".to_string());
            }
            Ok(PhysicsCall::Check { checks: list.iter().map(check_arg).collect::<Result<_, _>>()?, since_tick: int_arg(args, "since_tick", 0, u64::MAX)? })
        })(),
        "physics_impulse" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "impulse", "at", "angular_impulse"]])?;
            let impulse = vec3_arg(args, "impulse")?.unwrap_or(Vec3::ZERO);
            let angular = vec3_arg(args, "angular_impulse")?.unwrap_or(Vec3::ZERO);
            if [impulse.x, impulse.y, impulse.z, angular.x, angular.y, angular.z].iter().any(|c| c.abs() > 1.0e7) {
                return Err("impulses must be within ±1e7 N·s".to_string());
            }
            let at = vec3_arg(args, "at")?;
            Ok(PhysicsCall::Impulse { entity: entity_key(args, name)?, impulse, at, angular })
        })(),
        _ => {
            return crate::joint_tools::parse(name, args)
                .map(|r| r.map(PhysicsCall::Joint))
                .or_else(|| crate::ragdoll_tools::parse(name, args).map(|r| r.map(PhysicsCall::Ragdoll)))
        }
    };
    Some(call)
}

pub fn definitions() -> Vec<ToolDef> {
    let mut defs = definitions_physics();
    defs.extend(crate::joint_tools::definitions());
    defs.extend(crate::ragdoll_tools::definitions());
    defs
}

fn definitions_physics() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let vec3 = |d: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("number"))])),
            ("minItems", Json::from(3)),
            ("maxItems", Json::from(3)),
            ("description", Json::from(d)),
        ])
    };
    let strings = |d: &str| Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("string"))])), ("description", Json::from(d))]);
    let key = |d: &str| Json::obj([("description", Json::from(d))]);
    let filter = |props: &mut Vec<(&str, Json)>, triggers: &str| {
        props.push(("layers", strings("Only colliders on these layer names (default: every layer)")));
        props.push(("triggers", t("boolean", triggers)));
        props.push(("exclude", key("An entity (id or path) to ignore, e.g. the caster itself")));
    };
    let shape = || {
        Json::obj([
            ("type", Json::from("object")),
            ("description", Json::from("{shape: sphere (radius, default 0.5) | capsule (radius, height = total along local +Y) | box (size [x,y,z], default 1)}")),
        ])
    };
    let id_props = || {
        vec![("id", t("integer", "Entity id")), ("path", t("string", "Entity path, e.g. \"/crates/c1\"")), ("name", t("string", "A top-level entity's name"))]
    };
    let mut raycast = vec![
        ("origin", vec3("Ray start [x, y, z] (world, m)")),
        ("direction", vec3("Ray direction (any length; normalised)")),
        ("max_distance", t("number", "How far, m (default 1000)")),
        ("max_hits", t("integer", "Hits to return, nearest first, 1..64 (default 1)")),
    ];
    filter(&mut raycast, "Include trigger volumes (default false)");
    let mut cast = vec![
        ("shape", shape()),
        ("from", vec3("Start position of the shape's centre")),
        ("to", vec3("End position")),
        ("rotation_deg", vec3("The shape's rotation (degrees, X then Y then Z; default none)")),
        ("max_hits", t("integer", "Hits to return, earliest first, 1..64 (default 1)")),
    ];
    filter(&mut cast, "Include trigger volumes (default false)");
    let mut overlap = vec![
        ("shape", shape()),
        ("at", vec3("The shape's centre")),
        ("rotation_deg", vec3("The shape's rotation (degrees)")),
        ("limit", t("integer", "Entities to list, 1..256 (default 64); the total is always given")),
    ];
    filter(&mut overlap, "Include trigger volumes (default true)");
    let mut explain = id_props();
    explain.truncate(3);
    let mut impulse = id_props();
    impulse.extend([
        ("impulse", vec3("Linear impulse, N·s (velocity changes by impulse / mass)")),
        ("at", vec3("World point the impulse acts at (default the centre of mass: no spin)")),
        ("angular_impulse", vec3("Angular impulse, N·m·s")),
    ]);
    vec![
        ToolDef {
            name: "physics_raycast".into(),
            description: "Cast a ray through the physics world: hits on colliders as data, nearest first (ties by id): \
                          id, path, distance, point, normal (facing the ray), triangle (meshes). Rays starting inside a \
                          shape don't hit it. Uses the colliders as of the last physics tick."
                .into(),
            input_schema: schema(raycast, &["origin", "direction"]),
        },
        ToolDef {
            name: "physics_cast".into(),
            description: "Sweep a shape (sphere, capsule or box) from 'from' to 'to' and report what it would hit first: \
                          fraction of the way (0..1), distance, point, normal, initially_overlapping. For line of fire \
                          with thickness, placement checks and \"will it fit\" questions."
                .into(),
            input_schema: schema(cast, &["shape", "from", "to"]),
        },
        ToolDef {
            name: "physics_overlap".into(),
            description: "List the entities whose colliders overlap a shape placed at 'at' (sorted by id, at most \
                          'limit', with the total). Triggers are included by default."
                .into(),
            input_schema: schema(overlap, &["shape", "at"]),
        },
        ToolDef {
            name: "physics_contacts".into(),
            description: "Contact pairs as data, in id order: a, b (id and path), touching, trigger, and each point \
                          (point, normal from a to b, separation m: negative is penetration, impulse N·s). Filter by \
                          entity; page with cursor."
                .into(),
            input_schema: schema(
                vec![
                    ("entity", key("Only pairs this entity (id or path) is in")),
                    ("touching", t("boolean", "Only touching pairs (default true); false adds speculative (near) pairs")),
                    ("limit", t("integer", "Pairs per page, 1..256 (default 64)")),
                    ("cursor", t("string", "next_cursor of an earlier answer")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "physics_events".into(),
            description: "What happened in recent physics ticks, oldest first: contact_begin / contact_end (a, b, point, \
                          normal, approach speed), trigger_enter / trigger_exit (a = the trigger), ccd (the tunnelling \
                          guard stopped fast body a at b: static, or a moving body that was not fast itself). Each has seq and tick; pass after = the last seq you \
                          saw to get only newer ones. The last 4096 are kept."
                .into(),
            input_schema: schema(
                vec![
                    ("after", t("integer", "Only events with seq above this (default 0)")),
                    ("limit", t("integer", "Events per call, 1..256 (default 64)")),
                    ("kinds", strings("Only these kinds: contact_begin, contact_end, trigger_enter, trigger_exit, ccd")),
                    ("entity", key("Only events involving this entity (id or path)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "physics_info".into(),
            description: "The physics world at a glance: settings (world_env physics), bodies (awake, sleeping, \
                          characters), pairs and contacts, the last tick's time per stage, budget errors (structured, \
                          like perf_frame's), entities not simulated and why, layer names, the physics state hash, \
                          and automatic checks over the recent window (deepest penetration, energy gain, fast bodies, \
                          tunnelling-guard hits)."
                .into(),
            input_schema: schema(vec![], &[]),
        },
        ToolDef {
            name: "physics_explain".into(),
            description: "Why an entity moves, rests or passes through things: its body and collider as physics sees \
                          them (scaled), whether it is simulated (and if not, why), its contacts, the colliders near \
                          it and whether they pair (and if not, why: layer_mask, both_triggers, no_dynamic_body), the \
                          tunnelling indicator (distance per tick vs its thickness, and whether the guard covers it), \
                          its character state, and its recent events."
                .into(),
            input_schema: schema(explain, &[]),
        },
        ToolDef {
            name: "physics_check".into(),
            description: "Assert physics outcomes as data (headless scenario tests: build a scene, time_control step N \
                          ticks, then check). Each check: {check: at_rest (entities: path or \"/\", max_speed 0.05 m/s; \
                          asleep counts) | touching / not_touching (a, b) | no_penetration (max_depth 0.02 m, over the \
                          window) | no_tunnelling (no body ended a tick deeper than half its thickness inside another, \
                          no unguarded fast body) | energy (max_gain_j 0.5 per tick, over the window) | inside (entity, \
                          min, max: its world position in a box) | joint_error (max_m 0.01 anchor gap, max_deg 2 \
                          misalignment, worst joint over the window) | joint_limits (max_deg 2, max_m 0.01 past any \
                          joint's limits, over the window)}. since_tick limits the window (the last 1800 ticks \
                          are kept). Returns each result with its data and pass: true only if all pass."
                .into(),
            input_schema: schema(
                vec![
                    ("checks", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("object"))])), ("description", Json::from("1..64 checks"))])),
                    ("since_tick", t("integer", "Only ticks from this one on (default: the whole window)")),
                ],
                &["checks"],
            ),
        },
        ToolDef {
            name: "physics_impulse".into(),
            description: "Kick a dynamic body: adds an impulse (and optional angular impulse) and writes the new velocity \
                          into its body component (journaled and undoable like world_set; claims apply). Wakes it."
                .into(),
            input_schema: schema(impulse, &[]),
        },
    ]
}

fn round(v: f32) -> Json {
    Json::from((f64::from(v) * 1.0e5).round() / 1.0e5)
}

fn rv3(v: Vec3) -> Json {
    Json::arr([round(v.x), round(v.y), round(v.z)])
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn entity_json(world: &World, id: u64) -> Json {
    match crate::physics::ragdoll::unkey(id) {
        // A ragdoll part (ragdoll_info names its parts).
        Some((e, part)) => Json::obj([("ragdoll", Json::obj([("id", Json::from(e)), ("path", Json::from(world.path_of(e)))])), ("part", Json::from(part as u64))]),
        None => Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))]),
    }
}

impl crate::Engine {
    /// The query filter's bits (unknown layer names match nothing).
    fn query_filter(&self, f: &QueryFilter) -> Result<Filter, WorldError> {
        let mask = match &f.layers {
            None => u64::MAX,
            Some(names) => names.iter().fold(0u64, |m, n| m | self.physics.layer_mask_of(n)),
        };
        let exclude = match &f.exclude {
            None => None,
            Some(k) => Some(self.world.resolve(k)?),
        };
        Ok(Filter { mask, triggers: f.triggers, exclude })
    }

    pub(crate) fn physics_call(&mut self, call: PhysicsCall, w: Writer, tick: u64) -> ToolOutput {
        match call {
            PhysicsCall::Raycast { origin, direction, max_distance, filter, max_hits } => {
                let f = match self.query_filter(&filter) {
                    Ok(f) => f,
                    Err(e) => return world_error(&e),
                };
                let (hits, total) = self.physics.space.raycast(origin, direction, max_distance, &f, max_hits);
                let list = hits.iter().map(|h| {
                    let mut j = entity_json(&self.world, h.key);
                    j.set("distance", round(h.at));
                    j.set("point", rv3(h.point));
                    j.set("normal", rv3(h.normal));
                    if self.physics.space.body(h.key).and_then(|b| b.collider).is_some_and(|c| c.shape.triangles().is_some()) {
                        j.set("triangle", Json::from(h.triangle));
                    }
                    j
                });
                ToolOutput::text(Json::obj([("hits", Json::arr(list)), ("total", Json::from(total))]).to_string())
            }
            PhysicsCall::Cast { shape, from, to, rotation_deg, filter, max_hits } => {
                let f = match self.query_filter(&filter) {
                    Ok(f) => f,
                    Err(e) => return world_error(&e),
                };
                let r = rotation_deg;
                let pose = Pose::new(from, Quat::from_euler_xyz_deg(r.x, r.y, r.z));
                let delta = to - from;
                let (hits, total) = self.physics.space.cast(&shape.shape(), &pose, delta, &f, max_hits, 0.001);
                let len = delta.length();
                let list = hits.iter().map(|h| {
                    let mut j = entity_json(&self.world, h.key);
                    j.set("fraction", round(h.at));
                    j.set("distance", round(h.at * len));
                    j.set("point", rv3(h.point));
                    j.set("normal", rv3(h.normal));
                    j.set("initially_overlapping", Json::from(h.initially_overlapping));
                    j
                });
                ToolOutput::text(Json::obj([("hits", Json::arr(list)), ("total", Json::from(total))]).to_string())
            }
            PhysicsCall::Overlap { shape, at, rotation_deg, filter, limit } => {
                let f = match self.query_filter(&filter) {
                    Ok(f) => f,
                    Err(e) => return world_error(&e),
                };
                let r = rotation_deg;
                let pose = Pose::new(at, Quat::from_euler_xyz_deg(r.x, r.y, r.z));
                let (keys, total) = self.physics.space.overlap(&shape.shape(), &pose, &f, limit);
                let list = keys.iter().map(|k| entity_json(&self.world, *k));
                ToolOutput::text(Json::obj([("entities", Json::arr(list)), ("total", Json::from(total))]).to_string())
            }
            PhysicsCall::Contacts { entity, touching_only, limit, after } => {
                let only = match entity.map(|k| self.world.resolve(&k)).transpose() {
                    Ok(o) => o,
                    Err(e) => return world_error(&e),
                };
                let mut items = Vec::new();
                let mut next = None;
                let mut total = 0usize;
                for c in self.physics.space.contacts(touching_only) {
                    if only.is_some_and(|k| c.a != k && c.b != k) {
                        continue;
                    }
                    total += 1;
                    if after.is_some_and(|a| (c.a, c.b) <= a) {
                        continue;
                    }
                    if items.len() == limit {
                        if next.is_none() {
                            next = items.last().map(|(a, b, _): &(u64, u64, Json)| format!("{a}:{b}"));
                        }
                        continue;
                    }
                    let points = c.points.iter().map(|(p, n, s, i)| {
                        Json::obj([("point", rv3(*p)), ("normal", rv3(*n)), ("separation", round(*s)), ("impulse", round(*i))])
                    });
                    let j = Json::obj([
                        ("a", entity_json(&self.world, c.a)),
                        ("b", entity_json(&self.world, c.b)),
                        ("touching", Json::from(c.touching)),
                        ("trigger", Json::from(c.trigger)),
                        ("points", Json::arr(points)),
                    ]);
                    items.push((c.a, c.b, j));
                }
                ToolOutput::text(
                    Json::obj([
                        ("pairs", Json::arr(items.into_iter().map(|(_, _, j)| j))),
                        ("total", Json::from(total)),
                        ("next_cursor", Json::from(next)),
                    ])
                    .to_string(),
                )
            }
            PhysicsCall::Events { after, limit, kinds, entity } => {
                let only = match entity.map(|k| self.world.resolve(&k)).transpose() {
                    Ok(o) => o,
                    Err(e) => return world_error(&e),
                };
                let mut list = Vec::new();
                let mut more = false;
                for l in self.physics.events.iter().filter(|l| l.seq > after) {
                    if !kinds.is_empty() && !kinds.contains(&l.event.kind) {
                        continue;
                    }
                    if only.is_some_and(|k| l.event.a != k && l.event.b != k) {
                        continue;
                    }
                    if list.len() == limit {
                        more = true;
                        break;
                    }
                    let e = &l.event;
                    let mut j = Json::obj([
                        ("seq", Json::from(l.seq)),
                        ("tick", Json::from(l.tick)),
                        ("kind", Json::from(e.kind.name())),
                        ("a", entity_json(&self.world, e.a)),
                        ("b", entity_json(&self.world, e.b)),
                    ]);
                    if e.kind != EventKind::ContactEnd && e.kind != EventKind::TriggerExit {
                        j.set("point", rv3(e.point));
                    }
                    if matches!(e.kind, EventKind::ContactBegin | EventKind::Ccd) {
                        j.set("normal", rv3(e.normal));
                        j.set("speed", round(e.speed));
                    }
                    list.push(j);
                }
                let last = list.last().and_then(|j| j.get("seq")).and_then(Json::as_u64).unwrap_or(after);
                ToolOutput::text(
                    Json::obj([
                        ("events", Json::Arr(list)),
                        ("last_seq", Json::from(last)),
                        ("more", Json::from(more)),
                        ("oldest_kept", Json::from(self.physics.events.front().map(|l| l.seq))),
                    ])
                    .to_string(),
                )
            }
            PhysicsCall::Joint(j) => self.joint_call(j, w, tick),
            PhysicsCall::Ragdoll(r) => self.ragdoll_call(r, w, tick),
            PhysicsCall::Info => ToolOutput::text(self.physics.summary_json(&self.world.env().physics).to_pretty()),
            PhysicsCall::Explain { entity } => match self.world.resolve(&entity) {
                Ok(id) => ToolOutput::text(self.explain(id).to_pretty()),
                Err(e) => world_error(&e),
            },
            PhysicsCall::Check { checks, since_tick } => {
                let results: Vec<Json> = checks.iter().map(|c| self.run_check(c, since_tick)).collect();
                let failed = results.iter().filter(|r| r.get("pass").and_then(Json::as_bool) != Some(true)).count();
                ToolOutput::text(
                    Json::obj([
                        ("pass", Json::from(failed == 0)),
                        ("failed", Json::from(failed)),
                        ("tick", Json::from(self.clock.tick())),
                        ("window", Json::obj([("from_tick", Json::from(self.physics.window.front().map(|r| r.tick))), ("ticks", Json::from(self.physics.window.len()))])),
                        ("results", Json::Arr(results)),
                    ])
                    .to_pretty(),
                )
            }
            PhysicsCall::Impulse { entity, impulse, at, angular } => {
                let id = match self.world.resolve(&entity) {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                let Some(body) = self.world.get(id).and_then(|e| e.body).filter(|b| b.kind == BodyKind::Dynamic) else {
                    return world_error(&WorldError::new("not_dynamic", "physics_impulse needs a dynamic body (body.kind \"dynamic\")").with("id", id));
                };
                // Bring physics up to date with the world first (a body made dynamic this tick,
                // other impulses since the last tick), so its mass is the one it has now.
                self.physics.sync(&self.world, self.world_generation);
                let Some((dv, dw)) = self.physics.space.impulse_delta(id, impulse, at, angular) else {
                    let why = self.physics.problems.get(&id).cloned().unwrap_or_else(|| Json::from("not simulated yet"));
                    return world_error(
                        &WorldError::new("not_simulated", "physics doesn't simulate this body (see 'why' and physics_explain), so it can't take an impulse")
                            .with("id", id)
                            .with("why", why),
                    );
                };
                // Impulses add up: the change goes on top of the body's velocity as the world has
                // it now (which already holds this tick's earlier impulses).
                let new = Body { velocity: body.velocity + dv, angular_velocity: body.angular_velocity + dw, sleeping: false, ..body };
                let change = UpdateSpec { body: Some(Some(new)), ..Default::default() };
                let by = w.name.to_owned();
                match self.world.commit(&mut self.swarm, tick, w, JKind::Set, crate::write_policy::Rule::World, |wd| wd.update(&EntityKey::Id(id), change, &by).map(|e| e.id)) {
                    Ok((id, seq)) => {
                        let b = self.world.get(id).and_then(|e| e.body).unwrap_or(new);
                        ToolOutput::text(
                            Json::obj([
                                ("id", Json::from(id)),
                                ("path", Json::from(self.world.path_of(id))),
                                ("velocity", rv3(b.velocity)),
                                ("angular_velocity", rv3(b.angular_velocity)),
                                ("journal", Json::from(seq)),
                            ])
                            .to_string(),
                        )
                    }
                    Err(e) => world_error(&e),
                }
            }
        }
    }

    fn explain(&self, id: u64) -> Json {
        let world = &self.world;
        let e = world.get(id).expect("resolved");
        let mut out = entity_json(world, id);
        out.set("collider", e.collider.as_ref().map_or(Json::Null, |c| c.to_json()));
        out.set("body", e.body.map_or(Json::Null, Body::to_json));
        if !physics::relevant(e) {
            out.set("simulated", Json::from(false));
            out.set("why", Json::from(if e.transient { "transient entities (a game's presentation) take no part in physics" } else { "no collider or body" }));
            return out;
        }
        let space = &self.physics.space;
        let Some(view) = space.body(id) else {
            out.set("simulated", Json::from(false));
            out.set(
                "why",
                self.physics.problems.get(&id).cloned().unwrap_or_else(|| Json::from("not synced yet: it joins at the next physics tick")),
            );
            return out;
        };
        out.set("simulated", Json::from(true));
        out.set(
            "state",
            Json::obj([
                ("kind", Json::from(view.kind.name())),
                ("position", rv3(view.pose.p)),
                ("center_of_mass", rv3(view.center)),
                ("velocity", rv3(view.velocity)),
                ("angular_velocity", rv3(view.angular_velocity)),
                ("sleeping", Json::from(view.sleeping)),
                ("mass", round(view.mass)),
                ("shape", Json::from(view.collider.map_or("none", |c| c.shape.name()))),
                ("thickness_m", round(view.inner_radius)),
            ]),
        );
        let dt = 1.0 / self.clock.hz() as f32;
        let per_tick = view.velocity.length() * dt;
        let ccd = e.body.is_none_or(|b| b.ccd);
        out.set(
            "tunnelling",
            Json::obj([
                ("distance_per_tick_m", round(per_tick)),
                ("thickness_m", round(view.inner_radius)),
                ("fast", Json::from(view.inner_radius > 0.0 && per_tick > 0.5 * view.inner_radius)),
                ("guarded", Json::from(ccd)),
                (
                    "note",
                    Json::from("fast = moves more than half its thickness per tick; the guard (ccd) stops it at static colliders and at moving bodies that were not fast that tick (two fast bodies can still pass through each other)"),
                ),
            ]),
        );
        let contacts: Vec<Json> = space
            .contacts(false)
            .filter(|c| c.a == id || c.b == id)
            .take(16)
            .map(|c| {
                let other = if c.a == id { c.b } else { c.a };
                let deepest = c.points.iter().map(|p| p.2).fold(f32::MAX, f32::min);
                Json::obj([
                    ("with", entity_json(world, other)),
                    ("touching", Json::from(c.touching)),
                    ("trigger", Json::from(c.trigger)),
                    ("points", Json::from(c.points.len())),
                    ("deepest_separation", if c.points.is_empty() { Json::Null } else { round(deepest) }),
                ])
            })
            .collect();
        out.set("contacts", Json::Arr(contacts));
        let neighbours: Vec<Json> = space
            .neighbours(id, 16)
            .into_iter()
            .map(|(k, why)| {
                let mut j = entity_json(world, k);
                j.set("pairs", Json::from(why.is_none()));
                if let Some(w) = why {
                    j.set("why_not", Json::from(w));
                }
                if let Some(c) = world.get(k).and_then(|o| o.collider.as_ref()) {
                    j.set("layer", Json::from(c.layer.as_str()));
                }
                j
            })
            .collect();
        out.set("neighbours", Json::Arr(neighbours));
        let joints: Vec<Json> = space.joints_of(id).into_iter().take(16).map(|k| self.joint_json(k)).collect();
        if !joints.is_empty() {
            out.set("joints", Json::Arr(joints));
        }
        if let Some(cs) = space.character_state(id) {
            out.set(
                "character",
                Json::obj([
                    ("grounded", Json::from(cs.grounded)),
                    ("ground", cs.ground.map_or(Json::Null, |g| entity_json(world, g))),
                    ("ground_normal", rv3(cs.ground_normal)),
                ]),
            );
        }
        let events: Vec<Json> = self
            .physics
            .events
            .iter()
            .rev()
            .filter(|l| l.event.a == id || l.event.b == id)
            .take(16)
            .map(|l| {
                let other = if l.event.a == id { l.event.b } else { l.event.a };
                Json::obj([("seq", Json::from(l.seq)), ("tick", Json::from(l.tick)), ("kind", Json::from(l.event.kind.name())), ("with", entity_json(world, other))])
            })
            .collect();
        out.set("recent_events", Json::Arr(events));
        out
    }

    fn run_check(&self, c: &CheckSpec, since: Option<u64>) -> Json {
        let world = &self.world;
        let window = || self.physics.window.iter().filter(move |r| since.is_none_or(|s| r.tick >= s));
        let fail = |name: &str, msg: String| Json::obj([("check", Json::from(name)), ("pass", Json::from(false)), ("error", Json::from(msg))]);
        match c {
            CheckSpec::AtRest { scope, max_speed } => {
                let mut checked = 0;
                let mut moving: Vec<Json> = Vec::new();
                let mut fastest = 0.0f32;
                for b in self.physics.space.bodies() {
                    if b.kind != specula_physics::BodyKind::Dynamic {
                        continue;
                    }
                    let path = world.path_of(crate::physics::ragdoll::owner_of(b.key));
                    if let Scope::Under(p) = scope {
                        if !(path == *p || path.starts_with(&format!("{p}/"))) {
                            continue;
                        }
                    }
                    checked += 1;
                    let speed = b.velocity.length().max(b.angular_velocity.length() * b.inner_radius.max(0.1));
                    fastest = fastest.max(speed);
                    if !b.sleeping && speed > *max_speed && moving.len() < 16 {
                        moving.push(Json::obj([("id", Json::from(b.key)), ("path", Json::from(path)), ("speed", round(speed))]));
                    }
                }
                Json::obj([
                    ("check", Json::from("at_rest")),
                    ("pass", Json::from(checked > 0 && moving.is_empty())),
                    ("bodies", Json::from(checked)),
                    ("fastest", round(fastest)),
                    ("moving", Json::Arr(moving)),
                ])
            }
            CheckSpec::Touching { a, b, expect } => {
                let name = if *expect { "touching" } else { "not_touching" };
                let (a, b) = match (world.resolve(a), world.resolve(b)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(e), _) | (_, Err(e)) => return fail(name, e.message),
                };
                let touching = self.physics.space.touching(a).contains(&b);
                Json::obj([
                    ("check", Json::from(name)),
                    ("pass", Json::from(touching == *expect)),
                    ("a", entity_json(world, a)),
                    ("b", entity_json(world, b)),
                    ("touching", Json::from(touching)),
                ])
            }
            CheckSpec::NoPenetration { max_depth } => {
                let worst = window().filter_map(|r| r.deepest.map(|d| (d, r.tick))).fold(None, |m: Option<((f32, u64, u64), u64)>, x| match m {
                    Some(y) if y.0 .0 <= x.0 .0 => Some(y),
                    _ => Some(x),
                });
                let depth = worst.map_or(0.0, |((d, _, _), _)| (-d).max(0.0));
                let mut j = Json::obj([("check", Json::from("no_penetration")), ("pass", Json::from(depth <= *max_depth)), ("deepest_m", round(depth))]);
                if let Some(((_, a, b), t)) = worst {
                    j.set("pair", Json::arr([entity_json(world, a), entity_json(world, b)]));
                    j.set("tick", Json::from(t));
                }
                j
            }
            CheckSpec::NoTunnelling => {
                // A body deeper than half its thickness inside another, or a fast body without the guard.
                let mut problems: Vec<Json> = Vec::new();
                for r in window() {
                    if let Some((d, a, b)) = r.deepest {
                        let thin = [a, b].iter().filter_map(|k| self.physics.space.body(*k)).map(|v| v.inner_radius).filter(|t| *t > 0.0).fold(f32::MAX, f32::min);
                        if thin < f32::MAX && -d > 0.5 * thin && problems.len() < 8 {
                            problems.push(Json::obj([
                                ("tick", Json::from(r.tick)),
                                ("pair", Json::arr([entity_json(world, a), entity_json(world, b)])),
                                ("depth_m", round(-d)),
                                ("thickness_m", round(thin)),
                            ]));
                        }
                    }
                }
                let unguarded: Vec<Json> = self
                    .physics
                    .space
                    .report()
                    .fast
                    .iter()
                    .filter(|f| world.get(f.key).and_then(|e| e.body).is_some_and(|b| !b.ccd))
                    .map(|f| Json::obj([("id", Json::from(f.key)), ("path", Json::from(world.path_of(f.key))), ("speed", round(f.speed)), ("thickness_m", round(f.thickness))]))
                    .collect();
                let guard_hits: usize = window().map(|r| r.ccd).sum();
                Json::obj([
                    ("check", Json::from("no_tunnelling")),
                    ("pass", Json::from(problems.is_empty() && unguarded.is_empty())),
                    ("deep_inside", Json::Arr(problems)),
                    ("unguarded_fast_bodies", Json::Arr(unguarded)),
                    ("guard_stops", Json::from(guard_hits)),
                ])
            }
            CheckSpec::Energy { max_gain_j } => {
                let worst = window().fold(None, |m: Option<&physics::TickRecord>, r| match m {
                    Some(y) if y.energy_change >= r.energy_change => Some(y),
                    _ => Some(r),
                });
                let gain = worst.map_or(0.0, |r| r.energy_change.max(0.0));
                let gainers = worst.map(|r| r.gainers.iter().filter(|(_, j)| *j > 0.0).map(|(k, j)| {
                    let mut e = entity_json(world, *k);
                    e.set("joules", Json::from((j * 1000.0).round() / 1000.0));
                    e
                }).collect::<Vec<_>>()).unwrap_or_default();
                Json::obj([
                    ("check", Json::from("energy")),
                    ("pass", Json::from(gain <= f64::from(*max_gain_j))),
                    ("max_gain_j", Json::from((gain * 1000.0).round() / 1000.0)),
                    ("tick", Json::from(worst.map(|r| r.tick))),
                    ("gained_most", Json::Arr(gainers)),
                    ("note", Json::from("energy a tick added to the bodies it moved: kinematic pushes and penetration recovery add some; a closed system should not")),
                ])
            }
            CheckSpec::Joint { name, args } => crate::joint_tools::run_check(self, name, args, since),
            CheckSpec::Inside { entity, min, max } => {
                let id = match world.resolve(entity) {
                    Ok(id) => id,
                    Err(e) => return fail("inside", e.message),
                };
                let p = world.get(id).expect("resolved").world_position();
                let inside = p.x >= min.x && p.y >= min.y && p.z >= min.z && p.x <= max.x && p.y <= max.y && p.z <= max.z;
                Json::obj([("check", Json::from("inside")), ("pass", Json::from(inside)), ("entity", entity_json(world, id)), ("position", rv3(p))])
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Overlays

const STATIC: [f32; 4] = [0.72, 0.74, 0.8, 0.85];
const KINEMATIC: [f32; 4] = [1.0, 0.82, 0.3, 0.95];
const AWAKE: [f32; 4] = [0.4, 1.0, 0.45, 0.95];
const ASLEEP: [f32; 4] = [0.45, 0.6, 1.0, 0.9];
const TRIGGER: [f32; 4] = [1.0, 0.4, 1.0, 0.8];
const PENETRATING: [f32; 4] = [1.0, 0.25, 0.25, 1.0];
const TOUCHING: [f32; 4] = [1.0, 0.6, 0.2, 1.0];
const SPECULATIVE: [f32; 4] = [0.4, 1.0, 0.5, 0.9];

/// An arc of radius `r` around `c` in the plane of unit axes `(a, b)`, from angle `span.0` to
/// `span.1` (radians).
fn circle(p: &mut Painter, c: Vec3, (a, b): (Vec3, Vec3), r: f32, color: [f32; 4], (from, to): (f32, f32), steps: usize) {
    let mut prev = None;
    for i in 0..=steps {
        let t = from + (to - from) * i as f32 / steps as f32;
        let (s, co) = specula_core::dmath::sin_cos(t);
        let q = c + a * (co * r) + b * (s * r);
        if let Some(pr) = prev {
            p.line(pr, q, color, 1.5);
        }
        prev = Some(q);
    }
}

fn draw_shape(p: &mut Painter, shape: &Shape, pose: &Pose, color: [f32; 4], budget: &mut usize) {
    let (x, y, z) = (pose.vector(Vec3::X), pose.vector(Vec3::Y), pose.vector(Vec3::Z));
    let tau = std::f32::consts::TAU;
    match shape {
        Shape::Sphere { radius } => {
            for (a, b) in [(x, y), (y, z), (z, x)] {
                circle(p, pose.p, (a, b), *radius, color, (0.0, tau), 24);
            }
        }
        Shape::Capsule { radius, half_height } => {
            let (top, bot) = (pose.point(Vec3::Y * *half_height), pose.point(Vec3::Y * -*half_height));
            circle(p, top, (x, z), *radius, color, (0.0, tau), 20);
            circle(p, bot, (x, z), *radius, color, (0.0, tau), 20);
            for d in [x, -x, z, -z] {
                p.line(top + d * *radius, bot + d * *radius, color, 1.5);
            }
            let pi = std::f32::consts::PI;
            for a in [x, z] {
                circle(p, top, (a, y), *radius, color, (0.0, pi), 10);
                circle(p, bot, (a, y), *radius, color, (pi, tau), 10);
            }
        }
        Shape::Box { hull, .. } | Shape::Hull(hull) => {
            for e in &hull.edges {
                p.line(pose.point(hull.points[e.v[0] as usize]), pose.point(hull.points[e.v[1] as usize]), color, 1.5);
            }
        }
        Shape::Compound(c) => {
            for (lp, s) in &c.parts {
                draw_shape(p, s, &pose.mul(lp), color, budget);
            }
        }
        Shape::Mesh(_) | Shape::Heightfield(_) => {
            let Some(mesh) = shape.triangles() else { return };
            for t in 0..mesh.triangles.len() {
                if *budget == 0 {
                    break;
                }
                *budget -= 1;
                let [a, b, c] = mesh.triangle(t).map(|v| pose.point(v));
                p.line(a, b, color, 1.0);
                p.line(b, c, color, 1.0);
                p.line(c, a, color, 1.0);
            }
        }
    }
}

/// Draws the `colliders` and `contacts` overlays; returns their packet details.
pub fn paint(physics: &physics::Physics, world: &World, p: &mut Painter, spec: &ov::Spec, colliders: bool, contacts: bool) -> Vec<(&'static str, Json)> {
    let mut details = Vec::new();
    let space = &physics.space;
    let selected = |k: u64| spec.entities.is_empty() || spec.selects(&world.path_of(crate::physics::ragdoll::owner_of(k)));
    if colliders {
        let (mut drawn, mut omitted) = (0usize, 0usize);
        let mut counts = [0usize; 5];
        let mut budget = MAX_DRAWN_TRIANGLES;
        for b in space.bodies() {
            let Some(c) = b.collider else { continue };
            if !selected(b.key) {
                continue;
            }
            if drawn >= MAX_DRAWN_BODIES {
                omitted += 1;
                continue;
            }
            let (color, slot) = if c.is_trigger {
                (TRIGGER, 4)
            } else {
                match b.kind {
                    specula_physics::BodyKind::Static => (STATIC, 0),
                    specula_physics::BodyKind::Kinematic => (KINEMATIC, 1),
                    specula_physics::BodyKind::Dynamic if b.sleeping => (ASLEEP, 3),
                    specula_physics::BodyKind::Dynamic => (AWAKE, 2),
                }
            };
            counts[slot] += 1;
            draw_shape(p, &c.shape, &b.pose.mul(&c.offset), color, &mut budget);
            drawn += 1;
        }
        details.push((
            "colliders",
            Json::obj([
                ("drawn", Json::from(drawn)),
                ("omitted", Json::from(omitted)),
                ("static", Json::from(counts[0])),
                ("kinematic", Json::from(counts[1])),
                ("awake", Json::from(counts[2])),
                ("asleep", Json::from(counts[3])),
                ("triggers", Json::from(counts[4])),
                ("mesh_triangles_skipped", Json::from(budget == 0)),
                (
                    "legend",
                    Json::obj([
                        ("static", Json::from(ov::hex(STATIC))),
                        ("kinematic", Json::from(ov::hex(KINEMATIC))),
                        ("awake", Json::from(ov::hex(AWAKE))),
                        ("asleep", Json::from(ov::hex(ASLEEP))),
                        ("trigger", Json::from(ov::hex(TRIGGER))),
                    ]),
                ),
            ]),
        ));
    }
    if contacts {
        let slop = space.settings.linear_slop;
        let (mut points, mut pairs, mut omitted) = (0usize, 0usize, 0usize);
        let mut deepest = 0.0f32;
        for c in space.contacts(false) {
            if c.trigger || c.points.is_empty() || !(selected(c.a) || selected(c.b)) {
                continue;
            }
            if points >= MAX_DRAWN_CONTACTS {
                omitted += 1;
                continue;
            }
            pairs += 1;
            for (pt, n, s, _) in &c.points {
                let color = if *s < -slop {
                    PENETRATING
                } else if *s <= slop {
                    TOUCHING
                } else {
                    SPECULATIVE
                };
                p.dot(*pt, 3.0, color);
                p.line(*pt, *pt + *n * 0.25, color, 2.0);
                deepest = deepest.min(*s);
                points += 1;
            }
        }
        details.push((
            "contacts",
            Json::obj([
                ("pairs", Json::from(pairs)),
                ("points", Json::from(points)),
                ("omitted_pairs", Json::from(omitted)),
                ("deepest_m", round(-deepest)),
                ("normal_length_m", Json::from(0.25)),
                (
                    "legend",
                    Json::obj([
                        ("penetrating", Json::from(ov::hex(PENETRATING))),
                        ("touching", Json::from(ov::hex(TOUCHING))),
                        ("speculative", Json::from(ov::hex(SPECULATIVE))),
                    ]),
                ),
            ]),
        ));
    }
    details
}
