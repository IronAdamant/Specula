//! `world_place`: placement by relations (Locus, docs/design/LOCUS.md).
//!
//! The agent names what to place (`spawn`: world_spawn's arguments, or `entity`: one to move)
//! and its relations to other things; [`specula_locus::solve`] finds the positions, with this
//! module answering its probes from the world: physics queries for colliders, oriented boxes for
//! drawn entities without one, the navmesh for `on_navmesh`. A placement is written through the
//! same swarm commit path as `world_batch` (one journal entry, claims checked, undoable), or not
//! at all: unsatisfied relations come back as data with the nearest valid alternative, which
//! `on_unsatisfied: "alternative"` writes instead. `dry_run` only reports.

use crate::write_policy::WorldWrite;
use crate::journal::{Kind as JKind, Writer};
use crate::nav::{self, Nav};
use crate::physics::{self, ColliderShape, Physics};
use crate::schedule::Class;
use crate::tools::{bool_arg, check_keys, int_arg, key_value, num_in, string_arg};
use crate::world::{self, EntityKey, Op, OpResult, Shape, UpdateSpec, World, WorldError};
use specula_core::math::{Quat, Vec3};
use specula_core::Json;
use specula_locus::{solve, Box3, BoxSet, Placement, Probe, Rel, Relation, Request};
use specula_protocol::ToolOutput;
use std::collections::BTreeSet;

/// Drawn entities without a collider one call checks against at most.
const MAX_BOXES: usize = 8192;
/// Most entities one `avoid {tag}` or `spacing {among}` takes.
const MAX_TAGGED: usize = 16_384;
/// Work one world_place may do (see `specula_locus::Request::max_work`): a few hundred
/// milliseconds at most.
const MAX_WORK: u64 = 1_500_000;
/// Instances listed with every relation (more list only the unsatisfied ones).
const FULL_DETAIL: usize = 8;
/// Bytes of placements an answer lists in detail; past them, placements are listed briefly.
const MAX_ANSWER_BYTES: usize = 48 * 1024;

/// A target as the agent named it.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    Point(Vec3),
    Entity(EntityKey),
}

/// One relation as parsed (targets unresolved).
#[derive(Clone, Debug, PartialEq)]
pub enum RelSpec {
    On { target: Option<EntityKey> },
    Against { target: EntityKey, side: Option<(f32, f32)>, gap: f32 },
    Along { target: Option<EntityKey>, from: Option<Spot>, to: Option<Spot>, offset: f32, side: Option<(f32, f32)> },
    Between { a: Spot, b: Spot, t: Option<f32> },
    Near { target: Spot, min: f32, max: f32 },
    Inside { target: Option<EntityKey>, region: Option<(Vec3, Vec3)> },
    Facing { target: Option<Spot>, direction: Option<(f32, f32)> },
    Spacing { min: f32, max: Option<f32>, among: Option<String> },
    Avoid { target: Option<EntityKey>, tag: Option<String>, distance: f32 },
    OnNavmesh { mesh: Option<String>, reachable_from: Option<Spot> },
}

pub struct PlaceCall {
    /// world_spawn's arguments (re-parsed per instance), or the entity to move.
    pub spawn: Option<Json>,
    pub entity: Option<EntityKey>,
    pub count: usize,
    pub relations: Vec<(RelSpec, bool)>,
    pub footprint: Option<(Vec3, Vec3)>,
    pub front_deg: f32,
    pub settle: bool,
    pub collide: bool,
    pub dry_run: bool,
    pub alternative: bool,
    pub seed: u64,
    pub expect_version: Option<u64>,
    pub ignore: Vec<EntityKey>,
}

impl PlaceCall {
    /// By its work, not by whether it writes: many instances or tag sets are big either way.
    pub fn class(&self) -> Class {
        let tagged = self.relations.iter().any(|(r, _)| matches!(r, RelSpec::Avoid { tag: Some(_), .. } | RelSpec::Spacing { among: Some(_), .. }));
        if self.count > 8 || tagged {
            Class::Big
        } else if self.dry_run {
            Class::Read
        } else {
            Class::Edit
        }
    }
}

/// A world direction by name: north (-Z), south (+Z), east (+X), west (-X), or "+x" / "-z" ...,
/// or `[x, z]`.
fn direction_arg(v: &Json, key: &str) -> Result<(f32, f32), String> {
    let named = |s: &str| -> Option<(f32, f32)> {
        Some(match s {
            "north" | "-z" => (0.0, -1.0),
            "south" | "+z" | "z" => (0.0, 1.0),
            "east" | "+x" | "x" => (1.0, 0.0),
            "west" | "-x" => (-1.0, 0.0),
            _ => return None,
        })
    };
    match v {
        Json::Str(s) => named(s).ok_or_else(|| format!("'{key}' must be north, south, east, west, +x, -x, +z, -z or [x, z]")),
        Json::Arr(a) if a.len() == 2 => {
            let (x, z) = (a[0].as_f64().unwrap_or(f64::NAN) as f32, a[1].as_f64().unwrap_or(f64::NAN) as f32);
            let l = (x * x + z * z).sqrt();
            if l.is_finite() && l > 1.0e-6 {
                Ok((x / l, z / l))
            } else {
                Err(format!("'{key}' as [x, z] must be a non-zero direction"))
            }
        }
        _ => Err(format!("'{key}' must be north, south, east, west, +x, -x, +z, -z or [x, z]")),
    }
}

fn spot(v: Option<&Json>, key: &str) -> Result<Option<Spot>, String> {
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Arr(a)) if a.len() == 3 => {
            let c: Option<Vec<f32>> = a.iter().map(|x| x.as_f64().map(|f| f as f32)).collect();
            let c = c.ok_or_else(|| format!("'{key}' must be a point [x, y, z] or an entity"))?;
            let p = Vec3::new(c[0], c[1], c[2]);
            world::check_point(key, p).map_err(|e| e.message)?;
            Ok(Some(Spot::Point(p)))
        }
        Some(v) => key_value(v, key).map(|k| Some(Spot::Entity(k))),
    }
}

fn need<T>(v: Option<T>, rel: &str, key: &str, what: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("relation '{rel}' needs '{key}': {what}"))
}

fn entity_arg(r: &Json, key: &str) -> Result<Option<EntityKey>, String> {
    match r.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => key_value(v, key).map(Some),
    }
}

const RELS: [&str; 10] = ["on", "against", "along", "between", "near", "inside", "facing", "spacing", "avoid", "on_navmesh"];

fn relation_arg(r: &Json) -> Result<(RelSpec, bool), String> {
    let kind = r.get("rel").and_then(Json::as_str).ok_or_else(|| format!("each relation needs 'rel': one of {}", RELS.join(", ")))?;
    let soft = bool_arg(r, "soft")?.unwrap_or(false);
    let num = |k: &str, lo: f32, hi: f32| num_in(r, k, lo..=hi);
    let keys = |allowed: &[&str]| -> Result<(), String> {
        let mut all = vec!["rel", "soft"];
        all.extend_from_slice(allowed);
        check_keys(r, &format!("relation '{kind}'"), &[&all])
    };
    let spec = match kind {
        "on" => {
            keys(&["target"])?;
            let target = match r.get("target") {
                Some(Json::Str(s)) if s == "ground" => None,
                _ => entity_arg(r, "target")?,
            };
            RelSpec::On { target }
        }
        "against" => {
            keys(&["target", "side", "gap"])?;
            RelSpec::Against {
                target: need(entity_arg(r, "target")?, kind, "target", "the entity to stand against (a wall)")?,
                side: r.get("side").filter(|v| !v.is_null()).map(|v| direction_arg(v, "side")).transpose()?,
                gap: num("gap", 0.0, 100.0)?.unwrap_or(0.0),
            }
        }
        "along" => {
            keys(&["target", "from", "to", "offset", "side"])?;
            let target = entity_arg(r, "target")?;
            let (from, to) = (spot(r.get("from"), "from")?, spot(r.get("to"), "to")?);
            if target.is_none() && (from.is_none() || to.is_none()) {
                return Err("relation 'along' needs 'target' (an entity: its long axis) or 'from' and 'to' (the line's ends)".into());
            }
            RelSpec::Along {
                target,
                from,
                to,
                offset: num("offset", -1000.0, 1000.0)?.unwrap_or(0.0),
                side: r.get("side").filter(|v| !v.is_null()).map(|v| direction_arg(v, "side")).transpose()?,
            }
        }
        "between" => {
            keys(&["a", "b", "t"])?;
            RelSpec::Between {
                a: need(spot(r.get("a"), "a")?, kind, "a", "a point or entity")?,
                b: need(spot(r.get("b"), "b")?, kind, "b", "a point or entity")?,
                t: num("t", 0.0, 1.0)?,
            }
        }
        "near" => {
            keys(&["target", "max", "min"])?;
            let max = need(num("max", 0.0, 1.0e5)?, kind, "max", "the farthest it may be, metres")?;
            let min = num("min", 0.0, 1.0e5)?.unwrap_or(0.0);
            if min > max {
                return Err("relation 'near': 'min' must not exceed 'max'".into());
            }
            RelSpec::Near { target: need(spot(r.get("target"), "target")?, kind, "target", "a point or entity")?, min, max }
        }
        "inside" => {
            keys(&["target", "region"])?;
            let region = match r.get("region").filter(|v| !v.is_null()) {
                None => None,
                Some(g) => {
                    let lo = crate::tools::vec3_arg(g, "min")?.ok_or("relation 'inside': 'region' needs min and max [x, y, z]")?;
                    let hi = crate::tools::vec3_arg(g, "max")?.ok_or("relation 'inside': 'region' needs min and max [x, y, z]")?;
                    if hi.x <= lo.x || hi.z <= lo.z || hi.y < lo.y {
                        return Err("relation 'inside': 'region' max must exceed min".into());
                    }
                    Some((lo, hi))
                }
            };
            let target = entity_arg(r, "target")?;
            if target.is_none() && region.is_none() {
                return Err("relation 'inside' needs 'target' (an entity or group: its bounds) or 'region' {min, max}".into());
            }
            RelSpec::Inside { target, region }
        }
        "facing" => {
            keys(&["target", "direction"])?;
            let target = spot(r.get("target"), "target")?;
            let direction = r.get("direction").filter(|v| !v.is_null()).map(|v| direction_arg(v, "direction")).transpose()?;
            if target.is_none() && direction.is_none() {
                return Err("relation 'facing' needs 'target' (a point or entity) or 'direction' (north, east, [x, z], ...)".into());
            }
            RelSpec::Facing { target, direction }
        }
        "spacing" => {
            keys(&["min", "max", "among"])?;
            RelSpec::Spacing {
                min: need(num("min", 0.0, 1.0e4)?, kind, "min", "the least gap between footprints, metres")?,
                max: num("max", 0.0, 1.0e4)?,
                among: string_arg(r, "among")?.map(str::to_owned),
            }
        }
        "avoid" => {
            keys(&["target", "tag", "distance"])?;
            let target = entity_arg(r, "target")?;
            let tag = string_arg(r, "tag")?.map(str::to_owned);
            if target.is_none() && tag.is_none() {
                return Err("relation 'avoid' needs 'target' (an entity) or 'tag' (every entity with it)".into());
            }
            RelSpec::Avoid { target, tag, distance: need(num("distance", 0.0, 1.0e4)?, kind, "distance", "how far to keep, metres")? }
        }
        "on_navmesh" => {
            keys(&["mesh", "reachable_from"])?;
            RelSpec::OnNavmesh { mesh: string_arg(r, "mesh")?.map(str::to_owned), reachable_from: spot(r.get("reachable_from"), "reachable_from")? }
        }
        other => {
            let hint = specula_mens::closest(other, RELS.iter().copied()).map(|c| format!("; did you mean '{c}'?")).unwrap_or_default();
            return Err(format!("unknown relation '{other}' (one of: {}){hint}", RELS.join(", ")));
        }
    };
    Ok((spec, soft))
}

/// Parses `world_place` (`None`: another tool).
pub fn parse(name: &str, args: &Json) -> Option<Result<PlaceCall, String>> {
    if name != "world_place" {
        return None;
    }
    Some((|| {
        check_keys(
            args,
            name,
            &[&["spawn", "entity", "count", "relations", "footprint", "front", "settle", "collide", "dry_run", "on_unsatisfied", "seed", "expect_version", "ignore"]],
        )?;
        let spawn = match args.get("spawn") {
            None | Some(Json::Null) => None,
            Some(s @ Json::Obj(_)) => {
                // Check it now (each instance is parsed again when placed).
                crate::tools::parse_spawn(s)?;
                if s.get("position").is_some() {
                    return Err("'spawn.position' is what world_place solves for: leave it out (the relations say where)".into());
                }
                Some(s.clone())
            }
            Some(_) => return Err("'spawn' must be world_spawn's arguments (an object)".into()),
        };
        let entity = entity_arg(args, "entity")?;
        if spawn.is_some() == entity.is_some() {
            return Err("world_place needs exactly one of 'spawn' (world_spawn's arguments for something new) or 'entity' (an existing entity to move)".into());
        }
        let count = int_arg(args, "count", 1, specula_locus::MAX_COUNT as u64)?.unwrap_or(1) as usize;
        if count > 1 && entity.is_some() {
            return Err("'count' is for spawns: an entity moves once".into());
        }
        let list = args.get("relations").and_then(Json::as_array).ok_or("'relations' must be an array of {rel: ..., ...}")?;
        if list.is_empty() || list.len() > specula_locus::MAX_RELATIONS {
            return Err(format!("'relations' must hold 1..{} relations", specula_locus::MAX_RELATIONS));
        }
        let relations = list.iter().map(relation_arg).collect::<Result<Vec<_>, _>>()?;
        let footprint = match args.get("footprint").filter(|v| !v.is_null()) {
            None => None,
            Some(f) => {
                check_keys(f, "footprint", &[&["size", "min", "max"]])?;
                match (crate::tools::vec3_arg(f, "size")?, crate::tools::vec3_arg(f, "min")?, crate::tools::vec3_arg(f, "max")?) {
                    (Some(s), None, None) if s.x > 0.0 && s.y >= 0.0 && s.z > 0.0 => Some((s * -0.5, s * 0.5)),
                    (None, Some(lo), Some(hi)) if hi.x > lo.x && hi.y >= lo.y && hi.z > lo.z => Some((lo, hi)),
                    _ => return Err("'footprint' is {size: [x, y, z]} (centred on the origin) or {min, max} (local bounds)".into()),
                }
            }
        };
        let front_deg = match string_arg(args, "front")? {
            None | Some("+z") => 0.0,
            Some("-z") => 180.0,
            Some("+x") => 90.0,
            Some("-x") => -90.0,
            Some(_) => return Err("'front' (the object's front, local) must be +z, -z, +x or -x".into()),
        };
        let alternative = match string_arg(args, "on_unsatisfied")? {
            None | Some("refuse") => false,
            Some("alternative") => true,
            Some(_) => return Err("'on_unsatisfied' must be \"refuse\" or \"alternative\"".into()),
        };
        let ignore = match args.get("ignore") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) if a.len() <= 64 => a.iter().map(|v| key_value(v, "ignore")).collect::<Result<_, _>>()?,
            Some(_) => return Err("'ignore' must be an array of at most 64 entities".into()),
        };
        Ok(PlaceCall {
            spawn,
            entity,
            count,
            relations,
            footprint,
            front_deg,
            settle: bool_arg(args, "settle")?.unwrap_or(true),
            collide: bool_arg(args, "collide")?.unwrap_or(true),
            dry_run: bool_arg(args, "dry_run")?.unwrap_or(false),
            alternative,
            seed: int_arg(args, "seed", 0, u64::MAX >> 11)?.unwrap_or(0),
            expect_version: int_arg(args, "expect_version", 1, u64::MAX >> 11)?,
            ignore,
        })
    })())
}

pub fn definition() -> specula_protocol::ToolDef {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    specula_protocol::ToolDef {
        name: "world_place".into(),
        description: "Place by relations instead of coordinates: say what goes where relative to other things and the \
                      engine solves the position and yaw deterministically, checks collisions (physics colliders and \
                      drawn entities' boxes), settles it on the ground, and writes it (one journal entry like \
                      world_batch: claims apply, world_undo undoes it). Give 'spawn' (world_spawn's arguments, \
                      without position; 'count' makes several, named name_1, name_2...) or 'entity' (move one). \
                      Relations, each {rel, ..., soft?: true for a preference}: on {target | \"ground\"} (rest on its \
                      top); against {target, side?: north|south|east|west, gap?} (back to a wall's face, front away); \
                      along {target (its long axis) | from, to; offset?, side?} (on a line; several spread evenly); \
                      between {a, b, t?: 0..1}; near {target, max, min?} (gap between footprints, m); inside {target \
                      (its bounds, groups included) | region {min, max}}; facing {target | direction}; spacing {min, \
                      max?, among?: tag} (gaps between instances); avoid {target | tag, distance}; on_navmesh {mesh?, \
                      reachable_from?}. Targets: an entity (id, \"/path\", unique name) or a point [x, y, z]. North is \
                      -Z. Objects face local +Z (or 'front'). Per instance it returns position, rotation_deg, what it \
                      rests on, and each relation: satisfied, off (m or degrees), why. If any hard relation fails, \
                      nothing is written: the answer (error 'unsatisfied') gives the nearest valid (collision-free) \
                      alternative and hints; on_unsatisfied: \"alternative\" writes that instead. dry_run: true only \
                      reports and applies nothing: it sees physics and navigation as of the last tick (colliders moved \
                      or removed since then may answer at their old place; the real call sees them as they are)."
            .into(),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("spawn", t("object", "world_spawn's arguments (shape, name, parent, scale, color, collider, ...), without position")),
                    ("entity", Json::obj([("description", Json::from("An existing entity to move (id, path or unique name)"))])),
                    ("count", t("integer", "How many to spawn, 1..64 (default 1)")),
                    ("relations", Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("object"))])), ("description", Json::from("1..16 relations"))])),
                    ("footprint", t("object", "The object's size for placing: {size: [x, y, z]} or {min, max} local bounds (default: from its shape, collider or built model)")),
                    ("front", t("string", "The object's front, local: +z (default), -z, +x or -x")),
                    ("settle", t("boolean", "Rest on whatever is below when no relation says where vertically (default true)")),
                    ("collide", t("boolean", "Refuse spots that overlap anything (default true)")),
                    ("dry_run", t("boolean", "Solve and report, write nothing")),
                    ("on_unsatisfied", t("string", "\"refuse\" (default: write nothing) or \"alternative\" (write the nearest valid alternative)")),
                    ("seed", t("integer", "Shifts the candidate grid (default 0); same seed, same answer")),
                    ("expect_version", t("integer", "Moves: refuse unless the entity is at this version")),
                    ("ignore", Json::obj([("type", Json::from("array")), ("description", Json::from("Entities the collision check ignores"))])),
                ]),
            ),
            ("required", Json::arr([Json::from("relations")])),
        ]),
    }
}

// ---------------------------------------------------------------------------------------------
// Geometry from the world

/// An entity as an upright box: its world corners projected on its own X and Z axes (groups:
/// the axis-aligned bounds of what is below them).
fn entity_box(world: &World, id: u64) -> Option<Box3> {
    let e = world.get(id)?;
    if let Some(c) = e.world_corners() {
        let m = e.model();
        let axis = |i: usize| {
            let (x, z) = (m.cols[i][0], m.cols[i][2]);
            let l = (x * x + z * z).sqrt();
            if l > 1.0e-6 {
                Some((x / l, z / l))
            } else {
                None
            }
        };
        let (ax, az) = match (axis(0), axis(2)) {
            (Some(a), Some(b)) => (a, b),
            _ => ((1.0, 0.0), (0.0, 1.0)),
        };
        let centre = c.iter().fold(Vec3::ZERO, |s, p| s + *p) * 0.125;
        let (mut hx, mut hz, mut lo, mut hi) = (0.0f32, 0.0f32, f32::INFINITY, f32::NEG_INFINITY);
        for p in &c {
            let (dx, dz) = (p.x - centre.x, p.z - centre.z);
            hx = hx.max((dx * ax.0 + dz * ax.1).abs());
            hz = hz.max((dx * az.0 + dz * az.1).abs());
            lo = lo.min(p.y);
            hi = hi.max(p.y);
        }
        return Some(Box3 { center: Vec3::new(centre.x, (lo + hi) * 0.5, centre.z), half: Vec3::new(hx, (hi - lo) * 0.5, hz), ax, az });
    }
    // A group: the bounds of its subtree.
    let mut out: Option<(Vec3, Vec3)> = None;
    world.walk(Some(id), |d, _| {
        if d.id != id && d.world_corners().is_some() {
            let (a, b) = d.world_aabb();
            out = Some(match out {
                None => (a, b),
                Some((x, y)) => (x.min(a), y.max(b)),
            });
        }
    });
    match out {
        Some((lo, hi)) => Some(Box3::aligned(lo, hi)),
        None => Some(Box3::point(e.world_position())),
    }
}

/// Local bounds of a collider (in the entity's unscaled local frame).
fn collider_bounds(c: &physics::Collider) -> Option<(Vec3, Vec3)> {
    let o = c.offset;
    match &c.shape {
        ColliderShape::Box { size } => Some((o - *size * 0.5, o + *size * 0.5)),
        ColliderShape::Sphere { radius } => Some((o - Vec3::ONE * *radius, o + Vec3::ONE * *radius)),
        ColliderShape::Capsule { radius, height } => Some((o - Vec3::new(*radius, height * 0.5, *radius), o + Vec3::new(*radius, height * 0.5, *radius))),
        ColliderShape::Hull { points } => {
            let lo = points.iter().fold(Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY), |a, p| a.min(*p));
            let hi = points.iter().fold(Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY), |a, p| a.max(*p));
            Some((o + lo, o + hi))
        }
        _ => None,
    }
}

fn mul(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x * b.x, a.y * b.y, a.z * b.z)
}

/// Drawn entities' boxes by grid cell (2 m), so a probe looks only at those near it; boxes too
/// big for the grid are always checked.
struct Boxes {
    list: Vec<(u64, Box3)>,
    cells: std::collections::HashMap<(i32, i32), Vec<u32>>,
    big: Vec<u32>,
}

const CELL: f32 = 2.0;
/// Cells one box may occupy before it counts as big.
const MAX_BOX_CELLS: i64 = 256;

fn cell_range(lo: f32, hi: f32) -> (i64, i64) {
    (specula_core::dmath::floor(lo / CELL) as i64, specula_core::dmath::floor(hi / CELL) as i64)
}

fn xz_bounds(b: &Box3) -> (f32, f32, f32, f32) {
    let c = b.corners();
    let (mut x0, mut z0, mut x1, mut z1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (x, z) in c {
        x0 = x0.min(x);
        z0 = z0.min(z);
        x1 = x1.max(x);
        z1 = z1.max(z);
    }
    (x0, z0, x1, z1)
}

impl Boxes {
    fn new(list: Vec<(u64, Box3)>) -> Boxes {
        let mut cells: std::collections::HashMap<(i32, i32), Vec<u32>> = std::collections::HashMap::new();
        let mut big = Vec::new();
        for (i, (_, b)) in list.iter().enumerate() {
            let (x0, z0, x1, z1) = xz_bounds(b);
            let ((cx0, cx1), (cz0, cz1)) = (cell_range(x0, x1), cell_range(z0, z1));
            if (cx1 - cx0 + 1) * (cz1 - cz0 + 1) > MAX_BOX_CELLS || !(x0.is_finite() && x1.is_finite() && z0.is_finite() && z1.is_finite()) {
                big.push(i as u32);
                continue;
            }
            for cz in cz0..=cz1 {
                for cx in cx0..=cx1 {
                    cells.entry((cx as i32, cz as i32)).or_default().push(i as u32);
                }
            }
        }
        Boxes { list, cells, big }
    }

    /// Indices of the boxes that may meet the X/Z rectangle, sorted, without repeats.
    fn near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<u32> {
        let ((cx0, cx1), (cz0, cz1)) = (cell_range(x0, x1), cell_range(z0, z1));
        let mut out = self.big.clone();
        if (cx1 - cx0 + 1) * (cz1 - cz0 + 1) > MAX_BOX_CELLS * 4 {
            out.extend(0..self.list.len() as u32);
        } else {
            for cz in cz0..=cz1 {
                for cx in cx0..=cx1 {
                    if let Some(v) = self.cells.get(&(cx as i32, cz as i32)) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Answers the solver's probes from the world.
struct WorldProbe<'a> {
    physics: &'a Physics,
    nav: &'a Nav,
    mesh: Option<u64>,
    exclude: BTreeSet<u64>,
    boxes: Boxes,
    /// Polygons reachable from the last `reachable_from` start polygon (one flood per call).
    flood: Option<(specula_via::PolyRef, Vec<specula_via::PolyRef>)>,
}

impl Probe for WorldProbe<'_> {
    fn ground(&mut self, x: f32, z: f32, from_y: f32, depth: f32, only: Option<u64>) -> Option<(f32, u64)> {
        let mut best: Option<(f32, u64)> = None;
        let filter = specula_physics::Filter::default();
        let (hits, _) = self.physics.space.raycast(Vec3::new(x, from_y, z), Vec3::new(0.0, -1.0, 0.0), depth, &filter, 16);
        if let Some(h) = hits.iter().find(|h| !self.exclude.contains(&h.key) && only.is_none_or(|o| o == h.key)) {
            best = Some((h.point.y, h.key));
        }
        for i in self.boxes.near(x, z, x, z) {
            let (id, b) = &self.boxes.list[i as usize];
            if self.exclude.contains(id) || only.is_some_and(|o| o != *id) {
                continue;
            }
            let top = b.top();
            if top <= from_y && top >= from_y - depth && b.outside((x, z)) == 0.0 && best.is_none_or(|(y, bid)| top > y || (top == y && *id < bid)) {
                best = Some((top, *id));
            }
        }
        best
    }

    fn blocked(&mut self, b: &Box3) -> Option<u64> {
        let yaw = specula_core::dmath::atan2(-b.ax.1, b.ax.0).to_degrees();
        let pose = specula_physics::Pose::new(b.center, Quat::from_euler_xyz_deg(0.0, yaw, 0.0));
        let shape = specula_physics::Shape::cuboid(Vec3::new(b.half.x.max(1.0e-4), b.half.y.max(1.0e-4), b.half.z.max(1.0e-4)));
        let (keys, _) = self.physics.space.overlap(&shape, &pose, &specula_physics::Filter::default(), 64);
        let mut first = keys.into_iter().filter(|k| !self.exclude.contains(k)).min();
        let (x0, z0, x1, z1) = xz_bounds(b);
        for i in self.boxes.near(x0, z0, x1, z1) {
            let (id, o) = &self.boxes.list[i as usize];
            if !self.exclude.contains(id) && first.is_none_or(|f| *id < f) && o.overlaps(b) {
                first = Some(*id);
            }
        }
        first
    }

    fn navmesh(&mut self, p: Vec3) -> Option<Vec3> {
        let slot = self.nav.meshes.get(&self.mesh?)?;
        let h = slot.settings.config.agent_height;
        slot.mesh.nearest(p, Vec3::new(0.5, h, 0.5)).map(|(_, q)| q)
    }

    fn reachable(&mut self, from: Vec3, to: Vec3) -> bool {
        let Some(slot) = self.mesh.and_then(|m| self.nav.meshes.get(&m)) else { return false };
        let ext = Vec3::new(1.0, slot.settings.config.agent_height, 1.0);
        let (Some((a, _)), Some((b, _))) = (slot.mesh.nearest(from, ext), slot.mesh.nearest(to, ext)) else { return false };
        if self.flood.as_ref().is_none_or(|(s, _)| *s != a) {
            // Bounded by the navmesh's polygons, once per call (not one search per candidate).
            self.flood = Some((a, slot.mesh.reachable_from(a, usize::MAX)));
        }
        self.flood.as_ref().is_some_and(|(_, set)| set.binary_search(&b).is_ok())
    }

    fn navmesh_area(&mut self) -> Option<Box3> {
        let slot = self.nav.meshes.get(&self.mesh?)?;
        let mut out: Option<(Vec3, Vec3)> = None;
        for (_, t) in slot.mesh.tiles() {
            for p in &t.polys {
                let r = slot.mesh.rect(p);
                let (lo, hi) = (Vec3::new(r[0], p.h.iter().copied().fold(f32::INFINITY, f32::min), r[1]), Vec3::new(r[2], p.h.iter().copied().fold(f32::NEG_INFINITY, f32::max), r[3]));
                out = Some(match out {
                    None => (lo, hi),
                    Some((a, b)) => (a.min(lo), b.max(hi)),
                });
            }
        }
        out.map(|(lo, hi)| Box3::aligned(lo, hi))
    }
}

/// The world box a placement can involve: the relations' geometry and the start, grown by the
/// solver's search ring, the object's size and some room for settling and spacing.
fn region_of(rels: &[Relation], start: Vec3, lo: Vec3, hi: Vec3, navmesh: Option<Box3>) -> (Vec3, Vec3) {
    let (mut a, mut b) = (start, start);
    let mut point = |p: Vec3, grow: f32| {
        a = a.min(p - Vec3::new(grow, grow, grow));
        b = b.max(p + Vec3::new(grow, grow, grow));
    };
    let boxed = |x: &Box3, grow: f32, point: &mut dyn FnMut(Vec3, f32)| {
        let (x0, z0, x1, z1) = xz_bounds(x);
        point(Vec3::new(x0, x.bottom(), z0), grow);
        point(Vec3::new(x1, x.top(), z1), grow);
    };
    for r in rels {
        match &r.rel {
            Rel::On { target: Some((x, _)) } | Rel::Inside { region: x, .. } => boxed(x, 0.0, &mut point),
            Rel::Against { target, gap, .. } => boxed(target, *gap, &mut point),
            Rel::Near { target, max, .. } => boxed(target, *max, &mut point),
            Rel::Along { a: p, b: q, offset } => {
                point(*p, offset.abs());
                point(*q, offset.abs());
            }
            Rel::Between { a: p, b: q, .. } => {
                point(*p, 0.0);
                point(*q, 0.0);
            }
            Rel::OnNavmesh { .. } => {
                if let Some(n) = &navmesh {
                    boxed(n, 0.0, &mut point);
                }
            }
            _ => {}
        }
    }
    let grow = 6.0 + (hi - lo).length();
    (a - Vec3::new(grow, grow, grow), b + Vec3::new(grow, grow, grow))
}

fn r3(v: Vec3) -> Json {
    let r = |x: f32| Json::from((f64::from(x) * 1000.0).round() / 1000.0);
    Json::arr([r(v.x), r(v.y), r(v.z)])
}

fn placement_json(world: &World, p: &Placement, rels: &[(RelSpec, bool)], full: bool) -> Json {
    let mut j = Json::obj([("position", r3(p.position)), ("yaw_deg", Json::from((f64::from(p.yaw_deg) * 100.0).round() / 100.0)), ("satisfied", Json::from(p.satisfied))]);
    if let Some(id) = p.rests_on {
        j.set("rests_on", Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))]));
    }
    if let Some(id) = p.blocked_by {
        j.set("blocked_by", Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))]));
    }
    if let Some(i) = p.overlaps_instance {
        j.set("overlaps_instance", Json::from(i));
    }
    let list: Vec<Json> = p
        .relations
        .iter()
        .filter(|r| full || !r.satisfied)
        .map(|r| {
            let mut o = Json::obj([
                ("index", Json::from(r.index)),
                ("rel", Json::from(r.rel)),
                ("satisfied", Json::from(r.satisfied)),
                ("off", Json::from((f64::from(r.off) * 1000.0).round() / 1000.0)),
            ]);
            if rels.get(r.index).is_some_and(|(_, soft)| *soft) {
                o.set("soft", Json::from(true));
            }
            if let Some(w) = &r.why {
                o.set("why", Json::from(w.as_str()));
            }
            o
        })
        .collect();
    j.set("relations", Json::Arr(list));
    j
}

impl crate::Engine {
    /// The placements as JSON, in detail up to [`MAX_ANSWER_BYTES`], then briefly (position,
    /// yaw, satisfied and how many relations failed).
    fn placements_json(&self, list: &[Placement], rels: &[(RelSpec, bool)], full: bool) -> Json {
        let mut out = Vec::with_capacity(list.len());
        let mut bytes = 0;
        for p in list {
            let mut j = placement_json(&self.world, p, rels, full);
            bytes += j.to_string().len();
            if bytes > MAX_ANSWER_BYTES {
                let failed = p.relations.iter().filter(|r| !r.satisfied).count();
                j.set("relations", Json::obj([("unsatisfied", Json::from(failed)), ("listed", Json::from(false))]));
            }
            out.push(j);
        }
        Json::Arr(out)
    }

    fn resolve_spot(&self, s: &Spot) -> Result<(Vec3, Option<u64>), WorldError> {
        match s {
            Spot::Point(p) => Ok((*p, None)),
            Spot::Entity(k) => {
                let id = self.world.resolve(k)?;
                Ok((self.world.get(id).expect("resolved").world_position(), Some(id)))
            }
        }
    }

    fn spot_box(&self, s: &Spot) -> Result<Box3, WorldError> {
        match s {
            Spot::Point(p) => Ok(Box3::point(*p)),
            Spot::Entity(k) => {
                let id = self.world.resolve(k)?;
                Ok(entity_box(&self.world, id).expect("resolved"))
            }
        }
    }

    /// The boxes of every entity with `tag` (except the one being moved): all of them, or a
    /// structured refusal past [`MAX_TAGGED`].
    fn tagged_boxes(&self, tag: &str, moving: Option<u64>) -> Result<Vec<Box3>, WorldError> {
        let mut out = Vec::new();
        for e in self.world.persistent_entities().filter(|e| e.tags.contains(tag) && Some(e.id) != moving) {
            if out.len() == MAX_TAGGED {
                return Err(WorldError::new("budget", format!("more than {MAX_TAGGED} entities are tagged '{tag}': use a narrower tag (or a target)"))
                    .with("field", "relations")
                    .with("tag", tag));
            }
            if let Some(b) = entity_box(&self.world, e.id) {
                out.push(b);
            }
        }
        Ok(out)
    }

    fn target_box(&self, k: &EntityKey) -> Result<(Box3, u64), WorldError> {
        let id = self.world.resolve(k)?;
        Ok((entity_box(&self.world, id).expect("resolved"), id))
    }

    /// The solver's relations, targets resolved.
    fn resolve_relations(&self, specs: &[(RelSpec, bool)], moving: Option<u64>) -> Result<(Vec<Relation>, Option<u64>), WorldError> {
        let mut out = Vec::new();
        let mut mesh = None;
        for (spec, soft) in specs {
            let rel = match spec {
                RelSpec::On { target } => Rel::On { target: target.as_ref().map(|k| self.target_box(k)).transpose()? },
                RelSpec::Against { target, side, gap } => Rel::Against { target: self.target_box(target)?.0, side: *side, gap: *gap },
                RelSpec::Along { target, from, to, offset, side } => {
                    let (a, b) = match (target, from, to) {
                        (Some(k), _, _) => {
                            let (bx, _) = self.target_box(k)?;
                            // The long horizontal axis through the centre, at the base.
                            let (axis, half) = if bx.half.x >= bx.half.z { (bx.ax, bx.half.x) } else { (bx.az, bx.half.z) };
                            let c = Vec3::new(bx.center.x, bx.bottom(), bx.center.z);
                            (c - Vec3::new(axis.0, 0.0, axis.1) * half, c + Vec3::new(axis.0, 0.0, axis.1) * half)
                        }
                        (None, Some(f), Some(t)) => (self.resolve_spot(f)?.0, self.resolve_spot(t)?.0),
                        _ => unreachable!("checked when parsed"),
                    };
                    // A side turns the offset toward that world direction.
                    let offset = match side {
                        Some(s) => {
                            let (dx, dz) = (b.x - a.x, b.z - a.z);
                            let left = (dz, -dx);
                            if left.0 * s.0 + left.1 * s.1 >= 0.0 {
                                offset.abs()
                            } else {
                                -offset.abs()
                            }
                        }
                        None => *offset,
                    };
                    Rel::Along { a, b, offset }
                }
                RelSpec::Between { a, b, t } => Rel::Between { a: self.resolve_spot(a)?.0, b: self.resolve_spot(b)?.0, t: *t },
                RelSpec::Near { target, min, max } => Rel::Near { target: self.spot_box(target)?, min: *min, max: *max },
                RelSpec::Inside { target, region } => match (target, region) {
                    (Some(k), _) => Rel::Inside { region: self.target_box(k)?.0, y: None },
                    (None, Some((lo, hi))) => Rel::Inside { region: Box3::aligned(*lo, *hi), y: (hi.y > lo.y).then_some((lo.y, hi.y)) },
                    _ => unreachable!("checked when parsed"),
                },
                RelSpec::Facing { target, direction } => Rel::Facing { point: target.as_ref().map(|s| self.resolve_spot(s).map(|r| r.0)).transpose()?, direction: *direction },
                RelSpec::Spacing { min, max, among } => {
                    let among = match among {
                        Some(tag) => self.tagged_boxes(tag, moving)?,
                        None => Vec::new(),
                    };
                    Rel::Spacing { min: *min, max: *max, among: BoxSet::new(among) }
                }
                RelSpec::Avoid { target, tag, distance } => {
                    let mut boxes = Vec::new();
                    if let Some(k) = target {
                        boxes.push(self.target_box(k)?.0);
                    }
                    if let Some(tag) = tag {
                        boxes.extend(self.tagged_boxes(tag, moving)?);
                    }
                    Rel::Avoid { boxes: BoxSet::new(boxes), distance: *distance }
                }
                RelSpec::OnNavmesh { mesh: m, reachable_from } => {
                    let mid = self.nav.find_mesh(m.as_deref())?;
                    crate::nav::ready_mesh(&self.nav, mid)?;
                    mesh = Some(mid);
                    Rel::OnNavmesh { reachable_from: reachable_from.as_ref().map(|s| self.resolve_spot(s).map(|r| r.0)).transpose()? }
                }
            };
            out.push(Relation { rel, soft: *soft });
        }
        Ok((out, mesh))
    }

    /// The object's local bounds (scaled) and where they came from.
    fn footprint(&self, call: &PlaceCall, spawn: Option<&world::SpawnSpec>) -> Result<((Vec3, Vec3), &'static str), WorldError> {
        if let Some(f) = call.footprint {
            return Ok((f, "footprint"));
        }
        let unknown = || {
            WorldError::new("footprint_unknown", "can't tell how big it is: give 'footprint' {size: [x, y, z]} (or a collider; for a model, model_build it first)").with("field", "footprint")
        };
        match (spawn, &call.entity) {
            (Some(s), _) => {
                let parent_scale = match &s.parent {
                    Some(k) => {
                        let id = self.world.resolve(k)?;
                        physics::world_pose(&self.world, self.world.get(id).expect("resolved")).1
                    }
                    None => Vec3::ONE,
                };
                let scale = mul(s.scale, parent_scale);
                let (local, from) = if let Some(b) = s.collider.as_ref().and_then(collider_bounds) {
                    (b, "collider")
                } else {
                    match (s.shape, &s.profile) {
                        (Shape::Cube | Shape::Sphere, _) => ((Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)), "shape"),
                        (Shape::Quad, _) => ((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.01, 0.5)), "shape"),
                        (Shape::Lathe, Some(p)) => (p.bounds(), "shape"),
                        (Shape::Model, _) => {
                            let bounds = s.model.as_ref().and_then(|m| self.world.model_source().and_then(|src| src.resolve(m).ok())).and_then(|r| r.bounds);
                            (bounds.ok_or_else(unknown)?, "model_bounds")
                        }
                        _ => return Err(unknown()),
                    }
                };
                Ok(((mul(local.0, scale), mul(local.1, scale)), from))
            }
            (None, Some(k)) => {
                let id = self.world.resolve(k)?;
                let e = self.world.get(id).expect("resolved");
                let (_, scale) = physics::world_pose(&self.world, e);
                let local = e.collider.as_ref().and_then(collider_bounds).or_else(|| e.local_bounds());
                match local {
                    Some(l) => Ok(((mul(l.0, scale), mul(l.1, scale)), if e.collider.is_some() { "collider" } else { "shape" })),
                    None => {
                        // A group: its subtree's bounds around its origin.
                        let b = entity_box(&self.world, id).ok_or_else(unknown)?;
                        let p = e.world_position();
                        let lo = Vec3::new(b.center.x - b.half.x - p.x, b.bottom() - p.y, b.center.z - b.half.z - p.z);
                        let hi = Vec3::new(b.center.x + b.half.x - p.x, b.top() - p.y, b.center.z + b.half.z - p.z);
                        if b.half == Vec3::ZERO {
                            return Err(unknown());
                        }
                        Ok(((lo, hi), "group_bounds"))
                    }
                }
            }
            _ => Err(unknown()),
        }
    }

    pub(crate) fn place_call(&mut self, call: PlaceCall, w: Writer, tick: u64) -> ToolOutput {
        match self.place(call, w, tick) {
            Ok(j) => ToolOutput::text(j.to_string()),
            Err(Err(e)) => ToolOutput::error(e.to_json().to_pretty()),
            Err(Ok(j)) => ToolOutput::error(j.to_pretty()),
        }
    }

    /// `Err(Ok(json))`: a structured refusal carrying the solution; `Err(Err(e))`: an error.
    fn place(&mut self, call: PlaceCall, w: Writer, tick: u64) -> Result<Json, Result<Json, WorldError>> {
        let started = std::time::Instant::now();
        let spawn0 = match &call.spawn {
            Some(s) => Some(crate::tools::parse_spawn(s).map_err(|m| Err(WorldError::new("invalid_argument", m).with("field", "spawn")))?),
            None => None,
        };
        let moving = match &call.entity {
            Some(k) => Some(self.world.resolve(k).map_err(Err)?),
            None => None,
        };
        // A write sees this tick's colliders and navmesh (it is an input at a tick boundary, like
        // any edit). A dry run applies nothing: it reads physics and navigation as of the last
        // tick, and checks colliders physics hasn't taken in yet as boxes.
        if !call.dry_run {
            self.physics.sync(&self.world, self.world_generation);
            self.nav.sync(&self.world, self.world_generation);
        }
        let ((lo, hi), footprint_from) = self.footprint(&call, spawn0.as_ref()).map_err(Err)?;
        let (relations, mesh) = self.resolve_relations(&call.relations, moving).map_err(Err)?;
        let (start, yaw) = match (moving, &spawn0) {
            (Some(id), _) => {
                let e = self.world.get(id).expect("resolved");
                let f = physics::world_rotation(&self.world, e).rotate(Vec3::Z);
                (e.world_position(), specula_core::dmath::atan2(f.x, f.z).to_degrees())
            }
            (None, Some(s)) => {
                let base = match &s.parent {
                    Some(k) => self.world.resolve(k).ok().and_then(|id| self.world.get(id)).map_or(Vec3::ZERO, |p| p.world_position()),
                    None => Vec3::ZERO,
                };
                (base, s.rotation_deg.y)
            }
            _ => (Vec3::ZERO, 0.0),
        };
        // What collisions are checked against: the moved entity's subtree and 'ignore' aren't.
        let mut exclude = BTreeSet::new();
        if let Some(id) = moving {
            self.world.walk(Some(id), |e, _| {
                exclude.insert(e.id);
            });
        }
        for k in &call.ignore {
            let id = self.world.resolve(k).map_err(Err)?;
            exclude.insert(id);
        }
        // Drawn entities without colliders near what the call is about.
        let area = mesh.and_then(|m| self.nav.meshes.get(&m)).map(|s| Box3::aligned(s.settings.config.bounds_min, s.settings.config.bounds_max));
        let (rlo, rhi) = region_of(&relations, start, lo, hi, area);
        let mut boxes = Vec::new();
        for e in self.world.persistent_entities() {
            let unsynced = call.dry_run && e.collider.is_some() && !self.physics.space.contains(e.id);
            if (e.collider.is_some() && !unsynced) || !e.shown() || e.in_ui() || e.shape.is_2d() || exclude.contains(&e.id) || e.local_bounds().is_none() {
                continue;
            }
            if e.props.get(nav::MESH_KEY).is_some() {
                continue;
            }
            let (a, b) = e.world_aabb();
            // X/Z only: settling looks for ground tens of metres below.
            if a.x > rhi.x || b.x < rlo.x || a.z > rhi.z || b.z < rlo.z {
                continue;
            }
            if boxes.len() >= MAX_BOXES {
                return Err(Err(WorldError::new(
                    "budget",
                    format!("more than {MAX_BOXES} drawn entities without colliders lie around the relations' area: give them colliders (physics checks those), or narrow the relations (a smaller region or target)"),
                )));
            }
            if let Some(b) = entity_box(&self.world, e.id) {
                boxes.push((e.id, b));
            }
        }
        let req = Request {
            lo,
            hi,
            count: call.count,
            relations,
            start,
            yaw_deg: yaw,
            front_deg: call.front_deg,
            settle: call.settle,
            collide: call.collide,
            seed: call.seed,
            max_work: MAX_WORK,
        };
        let mut probe = WorldProbe { physics: &self.physics, nav: &self.nav, mesh, exclude, boxes: Boxes::new(boxes), flood: None };
        let solution = solve(&req, &mut probe);
        if solution.budget_exceeded {
            return Err(Err(WorldError::new(
                "budget",
                format!(
                    "this placement needs more work than one call may do ({} units, {} of {} instances solved): fewer instances, a smaller region, or a narrower tag",
                    solution.work,
                    solution.placements.len(),
                    call.count
                ),
            )
            .with("work", solution.work)));
        }
        let all_ok = solution.placements.iter().all(|p| p.satisfied);
        let full = solution.placements.len() <= FULL_DETAIL;
        let mut out = Json::obj([
            ("satisfied", Json::from(all_ok)),
            ("footprint", Json::obj([("min", r3(lo)), ("max", r3(hi)), ("from", Json::from(footprint_from))])),
            ("placements", self.placements_json(&solution.placements, &call.relations, full)),
            ("candidates", Json::from(solution.placements.iter().map(|p| p.candidates).sum::<usize>())),
            ("ms", Json::from((started.elapsed().as_secs_f64() * 1.0e5).round() / 100.0)),
        ]);
        if !solution.hints.is_empty() {
            out.set("hints", Json::arr(solution.hints.iter().map(|h| Json::from(h.as_str()))));
        }
        if footprint_from == "model_bounds" {
            out.set("note", Json::from("the footprint came from the model's built bounds: give the model a collider or 'footprint' to make placements independent of build timing"));
        }
        if call.dry_run {
            out.set("dry_run", Json::from(true));
            return Ok(out);
        }
        if solution.placements.len() < call.count {
            return Err(Ok(Json::obj([
                ("error", Json::from("no_placement")),
                ("message", Json::from("the solver found no candidate for some instances; nothing was written")),
                ("solution", out),
            ])));
        }
        if !all_ok {
            let no_free = solution.placements.iter().any(|p| p.blocked_by.is_some() || p.overlaps_instance.is_some());
            if !call.alternative || no_free {
                let (code, message) = if no_free {
                    ("no_free_spot", "some instances have no collision-free spot near what the relations ask; nothing was written (see blocked_by)")
                } else {
                    ("unsatisfied", "some relations can't be satisfied; nothing was written. Each placement shown is the nearest valid alternative: pass on_unsatisfied: \"alternative\" to write those, or relax the relations (see hints)")
                };
                out.set("error", Json::from(code));
                out.set("message", Json::from(message));
                return Err(Ok(out));
            }
        }
        // Write: spawns as one batch, or the move.
        let mut ops = Vec::new();
        let base_name = call.spawn.as_ref().and_then(|s| s.get("name")).and_then(Json::as_str).map(str::to_owned);
        for (i, p) in solution.placements.iter().enumerate() {
            match &call.spawn {
                Some(s) => {
                    let mut spec = crate::tools::parse_spawn(s).map_err(|m| Err(WorldError::new("invalid_argument", m)))?;
                    if call.count > 1 {
                        if let Some(n) = &base_name {
                            spec.name = Some(format!("{n}_{}", i + 1));
                        }
                    }
                    let parent = match &spec.parent {
                        Some(k) => Some(self.world.resolve(k).map_err(Err)?),
                        None => None,
                    };
                    let (pos, rot) = self.to_local(parent, p.position, spec.rotation_deg, p.yaw_deg);
                    spec.position = pos;
                    spec.rotation_deg = rot;
                    ops.push(Op::Spawn(spec));
                }
                None => {
                    let id = moving.expect("a move");
                    let e = self.world.get(id).expect("resolved");
                    let (pos, rot) = self.to_local(e.parent, p.position, e.rotation_deg, p.yaw_deg);
                    ops.push(Op::Set(EntityKey::Id(id), UpdateSpec { position: Some(pos), rotation_deg: Some(rot), expect_version: call.expect_version, ..Default::default() }));
                }
            }
        }
        let by = w.name.to_owned();
        let apply = |wd: &mut dyn WorldWrite| wd.apply_batch(ops, &by).map_err(|(i, e)| e.with("op_index", i));
        match self.world.commit(&mut self.swarm, tick, w, JKind::Batch, crate::write_policy::Rule::World, apply) {
            Ok((results, seq)) => {
                if let Json::Obj(pairs) = &mut out {
                    if let Some((_, Json::Arr(list))) = pairs.iter_mut().find(|(k, _)| k == "placements") {
                        for (j, r) in list.iter_mut().zip(&results) {
                            let (id, path) = match r {
                                OpResult::Spawned { id, path } | OpResult::Updated { id, path, .. } | OpResult::Deleted { id, path, .. } => (*id, path.clone()),
                            };
                            j.set("id", Json::from(id));
                            j.set("path", Json::from(path));
                        }
                    }
                }
                out.set("journal", Json::from(seq));
                if !all_ok {
                    out.set("wrote", Json::from("the nearest valid alternatives (some relations unsatisfied)"));
                }
                Ok(out)
            }
            Err(e) => Err(Err(self.conflict_help(e, Some(&crate::conflict::Yours::named(vec!["(world_place)".into()]))))),
        }
    }

    /// A world position and yaw as a local position and rotation under `parent` (pitch and roll
    /// kept from `rot`).
    fn to_local(&self, parent: Option<u64>, p: Vec3, rot: Vec3, yaw: f32) -> (Vec3, Vec3) {
        let q_world = Quat::from_euler_xyz_deg(rot.x, yaw, rot.z);
        match parent.and_then(|id| self.world.get(id)) {
            Some(par) => {
                let inv = par.model().inverse().unwrap_or(specula_core::math::Mat4::IDENTITY);
                let pq = physics::world_rotation(&self.world, par);
                let local_q = (pq.conjugate() * q_world).normalize();
                (inv.transform_point(p), specula_physics::math::quat_to_euler_deg(local_q))
            }
            None => (p, Vec3::new(rot.x, yaw, rot.z)),
        }
    }
}
