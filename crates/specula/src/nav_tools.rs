//! The navigation and AI tools agents call (`nav_*`, `ai_*`), the engine's navigation tick, and
//! the `navmesh` and `paths` sight overlays (docs/design/NAVIGATION.md).
//!
//! - `nav_build` writes a navmesh entity (`/nav/<name>`, journaled, claim-checked, undoable) and
//!   builds it at once; later world edits rebuild only the tiles they touch, on the fixed tick.
//! - `nav_path` and `nav_point` query a navmesh; `nav_info` reports navmeshes and agents. Reads
//!   never apply world changes: they see navigation as of the last tick (or the last write), so
//!   calling one never changes what the simulation does.
//! - `nav_agent` makes an entity a nav agent (its `props.nav_agent`), `ai_set` gives it a mind
//!   (`props.mind`), validated first; `ai_check` validates a definition without writing anything;
//!   `ai_status` shows what minds are doing and why.

use crate::journal::{Kind as JKind, Writer};
use crate::nav::{self, AgentSpec, MeshSettings, Sources};
use crate::schedule::Class;
use crate::sight_overlay::{self as ov, Painter};
use crate::tools::{bool_arg, check_keys, entity_key, int_arg, key_value, num_in, string_arg};
use crate::world::{EntityKey, Op, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use specula_core::math::Vec3;
use specula_core::rng::Rng;
use specula_core::Json;
use specula_mens::Def;
use specula_protocol::{ToolDef, ToolOutput};
use specula_via::query::{path_json, MAX_NODES};

/// Polygons the navmesh overlay draws at most.
const MAX_DRAWN_POLYS: usize = 4000;
/// Corners per agent path the overlay draws.
const MAX_DRAWN_CORNERS: usize = 24;
/// Bytes of one ai_status answer.
const MAX_STATUS_BYTES: usize = 32 * 1024;
/// Bytes of nav_info's problem list, and of a whole nav_info answer.
const MAX_PROBLEM_BYTES: usize = 8 * 1024;
const MAX_INFO_BYTES: usize = 32 * 1024;

/// A point, or an entity whose world position is the point.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    Point(Vec3),
    Entity(EntityKey),
}

pub enum NavCall {
    Build { name: String, args: Json, expect_version: Option<u64> },
    Path { mesh: Option<String>, from: Spot, to: Spot, max_nodes: usize, max_corners: usize, prefs: Json },
    Point { mesh: Option<String>, at: Option<Spot>, random: usize, radius: Option<f32>, seed: u64 },
    Agent { entity: EntityKey, patch: Json, remove: bool, expect_version: Option<u64> },
    Info { mesh: Option<String>, agents: bool, after: u64, limit: usize },
    Check { def: Json, vocabulary: bool },
    SetMind { entity: EntityKey, def: Json, blackboard: Option<Json>, as_definition: bool, remove: bool, expect_version: Option<u64> },
    Status { entity: Option<EntityKey>, after: u64, limit: usize },
    ModelCollider { entity: EntityKey, kind: String, moving: Option<bool>, expect_version: Option<u64> },
}

impl NavCall {
    pub fn class(&self) -> Class {
        match self {
            NavCall::Build { .. } => Class::Big,
            NavCall::Agent { .. } | NavCall::SetMind { .. } | NavCall::ModelCollider { .. } => Class::Edit,
            _ => Class::Read,
        }
    }
}

fn spot_arg(v: Option<&Json>, key: &str) -> Result<Option<Spot>, String> {
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Arr(a)) => {
            let c: Option<Vec<f32>> = a.iter().map(|x| x.as_f64().map(|f| f as f32)).collect();
            match c.as_deref() {
                Some(&[x, y, z]) => {
                    let p = Vec3::new(x, y, z);
                    crate::world::check_point(key, p).map_err(|e| e.message)?;
                    Ok(Some(Spot::Point(p)))
                }
                _ => Err(format!("'{key}' must be a point [x, y, z] or an entity (id, path or unique name)")),
            }
        }
        Some(v) => key_value(v, key).map(|k| Some(Spot::Entity(k))),
    }
}

fn need_spot(args: &Json, key: &str) -> Result<Spot, String> {
    spot_arg(args.get(key), key)?.ok_or_else(|| format!("'{key}' is required: a point [x, y, z] or an entity (id, path or unique name)"))
}

/// Parses a navigation or AI tool call (`None`: not one of them).
pub fn parse(name: &str, args: &Json) -> Option<Result<NavCall, String>> {
    let call = match name {
        "nav_build" => (|| {
            check_keys(
                args,
                name,
                &[&["name", "agent", "like", "cell_size", "cell_height", "tile_cells", "bounds", "margin", "sources", "tiles_per_tick", "areas", "links", "remove", "expect_version"]],
            )?;
            let name = string_arg(args, "name")?.unwrap_or("default").to_owned();
            crate::world::validate_name(&name).map_err(|e| e.message)?;
            let expect_version = int_arg(args, "expect_version", 1, u64::MAX >> 11)?;
            Ok(NavCall::Build { name, args: args.clone(), expect_version })
        })(),
        "nav_path" => (|| {
            check_keys(args, name, &[&["mesh", "from", "to", "max_nodes", "max_corners", "area_costs", "avoid_areas", "avoid_flags"]])?;
            nav::parse_area_prefs(args, "").map_err(|e| e.message)?;
            Ok(NavCall::Path {
                prefs: args.clone(),
                mesh: string_arg(args, "mesh")?.map(str::to_owned),
                from: need_spot(args, "from")?,
                to: need_spot(args, "to")?,
                max_nodes: int_arg(args, "max_nodes", 1, MAX_NODES.1 as u64)?.unwrap_or(MAX_NODES.0 as u64) as usize,
                max_corners: int_arg(args, "max_corners", 2, 256)?.unwrap_or(64) as usize,
            })
        })(),
        "nav_point" => (|| {
            check_keys(args, name, &[&["mesh", "at", "random", "radius", "seed"]])?;
            let random = int_arg(args, "random", 0, 32)?.unwrap_or(0) as usize;
            let at = spot_arg(args.get("at"), "at")?;
            if at.is_none() && random == 0 {
                return Err("nav_point needs 'at' (the nearest navmesh point to it) or 'random' (how many random points)".to_string());
            }
            Ok(NavCall::Point {
                mesh: string_arg(args, "mesh")?.map(str::to_owned),
                at,
                random,
                radius: num_in(args, "radius", 0.1..=1.0e5)?,
                seed: int_arg(args, "seed", 0, u64::MAX >> 11)?.unwrap_or(0),
            })
        })(),
        "nav_agent" => (|| {
            check_keys(
                args,
                name,
                &[&["id", "path", "name", "mesh", "speed", "radius", "accel", "arrive", "avoid", "goal", "area_costs", "avoid_areas", "avoid_flags", "remove", "expect_version"]],
            )?;
            let entity = entity_key(args, name)?;
            let mut patch: Vec<(String, Json)> = Vec::new();
            for k in ["mesh", "speed", "radius", "accel", "arrive", "avoid", "goal", "area_costs", "avoid_areas", "avoid_flags"] {
                if let Some(v) = args.get(k) {
                    patch.push((k.to_owned(), v.clone()));
                }
            }
            Ok(NavCall::Agent {
                entity,
                patch: Json::Obj(patch),
                remove: bool_arg(args, "remove")?.unwrap_or(false),
                expect_version: int_arg(args, "expect_version", 1, u64::MAX >> 11)?,
            })
        })(),
        "nav_info" => (|| {
            check_keys(args, name, &[&["mesh", "agents", "cursor", "limit"]])?;
            Ok(NavCall::Info {
                mesh: string_arg(args, "mesh")?.map(str::to_owned),
                agents: bool_arg(args, "agents")?.unwrap_or(false),
                after: int_arg(args, "cursor", 0, u64::MAX >> 11)?.unwrap_or(0),
                limit: int_arg(args, "limit", 1, 128)?.unwrap_or(32) as usize,
            })
        })(),
        "ai_check" => (|| {
            check_keys(args, name, &[&["def", "vocabulary"]])?;
            let def = args.get("def").cloned().unwrap_or(Json::Null);
            let vocabulary = bool_arg(args, "vocabulary")?.unwrap_or(false);
            if def.is_null() && !vocabulary {
                return Err("ai_check needs 'def' (a definition to check) or 'vocabulary': true (what minds can use)".to_string());
            }
            Ok(NavCall::Check { def, vocabulary })
        })(),
        "ai_set" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "def", "blackboard", "as_definition", "remove", "expect_version"]])?;
            let remove = bool_arg(args, "remove")?.unwrap_or(false);
            let def = args.get("def").cloned().unwrap_or(Json::Null);
            if def.is_null() && !remove {
                return Err("ai_set needs 'def': a definition object, or the path of an entity holding one in props.mind_def (or remove: true)".to_string());
            }
            let blackboard = match args.get("blackboard") {
                None | Some(Json::Null) => None,
                Some(b @ Json::Obj(_)) => Some(b.clone()),
                Some(_) => return Err("'blackboard' must be an object of starting values".to_string()),
            };
            Ok(NavCall::SetMind {
                entity: entity_key(args, name)?,
                def,
                blackboard,
                as_definition: bool_arg(args, "as_definition")?.unwrap_or(false),
                remove,
                expect_version: int_arg(args, "expect_version", 1, u64::MAX >> 11)?,
            })
        })(),
        "ai_status" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "cursor", "limit"]])?;
            let present = ["id", "path", "name"].iter().any(|k| args.get(k).is_some_and(|v| !v.is_null()));
            Ok(NavCall::Status {
                entity: if present { Some(entity_key(args, name)?) } else { None },
                after: int_arg(args, "cursor", 0, u64::MAX >> 11)?.unwrap_or(0),
                limit: int_arg(args, "limit", 1, 32)?.unwrap_or(8) as usize,
            })
        })(),
        "model_collider" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "kind", "moving", "expect_version"]])?;
            let kind = string_arg(args, "kind")?.unwrap_or("auto").to_owned();
            if !["auto", "recipe", "box", "capsule", "hull", "mesh"].contains(&kind.as_str()) {
                return Err(format!("'kind' must be auto, recipe, box, capsule, hull or mesh (not '{kind}')"));
            }
            Ok(NavCall::ModelCollider { entity: entity_key(args, name)?, kind, moving: bool_arg(args, "moving")?, expect_version: int_arg(args, "expect_version", 1, u64::MAX >> 11)? })
        })(),
        _ => return None,
    };
    Some(call)
}

/// What a collider is, in a few hundred bytes: its shape, sizes or point / vertex / triangle
/// counts, the bounds of its points, its offset.
fn collider_summary(c: &crate::physics::Collider) -> Json {
    use crate::physics::ColliderShape;
    let mut j = Json::obj([("shape", Json::from(c.shape.name())), ("offset", r3(c.offset))]);
    let bounds = |pts: &[Vec3]| {
        let (mut lo, mut hi) = (Vec3::new(f32::MAX, f32::MAX, f32::MAX), Vec3::new(f32::MIN, f32::MIN, f32::MIN));
        for p in pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        Json::obj([("min", r3(lo)), ("max", r3(hi))])
    };
    match &c.shape {
        ColliderShape::Box { size } => {
            j.set("size", r3(*size));
        }
        ColliderShape::Sphere { radius } => {
            j.set("radius", Json::from(*radius));
        }
        ColliderShape::Capsule { radius, height } => {
            j.set("radius", Json::from(*radius));
            j.set("height", Json::from(*height));
        }
        ColliderShape::Hull { points } => {
            j.set("points", Json::from(points.len()));
            j.set("bounds", bounds(points));
        }
        ColliderShape::Mesh { vertices, triangles } => {
            j.set("vertices", Json::from(vertices.len()));
            j.set("triangles", Json::from(triangles.len()));
            j.set("bounds", bounds(vertices));
        }
        ColliderShape::Heightfield { columns, rows, .. } => {
            j.set("columns", Json::from(*columns));
            j.set("rows", Json::from(*rows));
        }
    }
    j
}

/// A physics collider fitted to a model's mesh (its model space, which is the entity's local
/// space), and a note when the fit changed what was asked. Moving bodies get hulls of at most
/// 16 points (physics refuses more); meshes are for static bodies only.
fn fit_model_collider(model: &specula_model::Model, kind: &str, moving: bool) -> Result<(crate::physics::Collider, Option<String>), WorldError> {
    use crate::physics::{Collider, ColliderShape};
    use specula_model::{Collider as Fit, ColliderKind};
    let mesh = model.lods.first().ok_or_else(|| WorldError::new("no_mesh", "the model has no mesh to fit"))?;
    if mesh.positions.len() < 4 {
        return Err(WorldError::new("no_mesh", "the model's mesh has fewer than 4 points"));
    }
    let cap = if moving { specula_model::collider::MAX_MOVING_HULL_POINTS } else { specula_model::collider::MAX_HULL_POINTS };
    let v = |p: [f32; 3]| Vec3::new(p[0], p[1], p[2]);
    if kind == "mesh" {
        if moving {
            return Err(WorldError::new("static_only", "a mesh collider is for static bodies only (this one moves): use hull").with("field", "kind"));
        }
        // The most detailed LOD that fits physics' mesh limits.
        let lod = model
            .lods
            .iter()
            .find(|m| m.positions.len() <= crate::physics::MAX_MESH_VERTICES && m.indices.len() / 3 <= crate::physics::MAX_MESH_TRIANGLES && m.indices.len() >= 3)
            .ok_or_else(|| {
                WorldError::new(
                    "budget",
                    format!("no LOD of the model fits a mesh collider ({} vertices, {} triangles at most): use hull", crate::physics::MAX_MESH_VERTICES, crate::physics::MAX_MESH_TRIANGLES),
                )
            })?;
        let vertices: Vec<Vec3> = lod.positions.iter().map(|p| v(*p)).collect();
        let triangles: Vec<[u32; 3]> = lod.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let note = (!std::ptr::eq(lod, mesh)).then(|| "the full-detail mesh is past physics' mesh limits: a lower LOD was used".to_string());
        return Ok((Collider { shape: ColliderShape::Mesh { vertices: std::sync::Arc::new(vertices), triangles: std::sync::Arc::new(triangles) }, ..Collider::default() }, note));
    }
    let (fit, mut note) = match kind {
        "recipe" if model.collider != Fit::None => {
            let hull_too_big = matches!(&model.collider, Fit::ConvexHull { points, .. } if points.len() > cap);
            if hull_too_big {
                let (f, n) = specula_model::collider::fit_limited(ColliderKind::ConvexHull, &mesh.positions, cap);
                (f, Some(n.unwrap_or_else(|| format!("the recipe's hull has more than {cap} points (a moving body's limit): refitted"))))
            } else {
                (model.collider.clone(), None)
            }
        }
        "recipe" => return Err(WorldError::new("no_collider", "the model's recipe asks for no collider: pass kind auto, box, capsule, hull or mesh").with("field", "kind")),
        "box" => specula_model::collider::fit_limited(ColliderKind::Aabb, &mesh.positions, cap),
        "capsule" => specula_model::collider::fit_limited(ColliderKind::Capsule, &mesh.positions, cap),
        _ => specula_model::collider::fit_limited(ColliderKind::ConvexHull, &mesh.positions, cap),
    };
    let shape_of = |fit: &Fit, note: &mut Option<String>| -> Option<(ColliderShape, Vec3)> {
        match fit {
            Fit::None => None,
            Fit::Aabb { min, max } => {
                let (lo, hi) = (v(*min), v(*max));
                let size = (hi - lo).max(Vec3::new(0.01, 0.01, 0.01));
                Some((ColliderShape::Box { size }, (lo + hi) * 0.5))
            }
            Fit::Capsule { a, b, radius } => {
                let (a, b) = (v(*a), v(*b));
                // Physics capsules stand along +Y.
                if (a.x - b.x).abs() > 1.0e-3 || (a.z - b.z).abs() > 1.0e-3 {
                    *note = Some("the fitted capsule lies sideways; physics capsules stand along +Y, so a hull was used".to_string());
                    return None;
                }
                let r = radius.max(0.01);
                Some((ColliderShape::Capsule { radius: r, height: ((a.y - b.y).abs() + 2.0 * r).max(2.0 * r) }, (a + b) * 0.5))
            }
            Fit::ConvexHull { points, .. } => Some((ColliderShape::Hull { points: std::sync::Arc::new(points.iter().map(|p| v(*p)).collect()) }, Vec3::ZERO)),
        }
    };
    let shaped = match shape_of(&fit, &mut note) {
        Some(s) => s,
        None => {
            let (hull, n) = specula_model::collider::fit_limited(ColliderKind::ConvexHull, &mesh.positions, cap);
            if n.is_some() {
                note = n;
            }
            shape_of(&hull, &mut note).ok_or_else(|| WorldError::new("no_fit", "no collider could be fitted to the model"))?
        }
    };
    Ok((Collider { shape: shaped.0, offset: shaped.1, ..Collider::default() }, note))
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let any = |d: &str| Json::obj([("description", Json::from(d))]);
    let spot = |d: &str| any(&format!("{d}: a point [x, y, z] or an entity (id, \"/path\" or unique name)"));
    let ids = || vec![("id", t("integer", "Entity id")), ("path", t("string", "Entity path, e.g. \"/guards/g1\"")), ("name", t("string", "A unique entity name"))];
    let mesh = || t("string", "The navmesh: its name (under /nav) or path (default: the only one, or /nav/default)");
    let mut agent = ids();
    agent.extend([
        ("mesh", mesh()),
        ("speed", t("number", "Top speed, m/s (default 2)")),
        ("radius", t("number", "Avoidance radius, m (default the navmesh's agent_radius)")),
        ("accel", t("number", "Largest change of velocity, m/s² (default 8)")),
        ("arrive", t("number", "Arrived within this distance of the goal, m (default 0.25)")),
        ("avoid", t("boolean", "Steer around other agents (default true)")),
        ("goal", spot("Where to go when it has no mind (null stops it)")),
        ("area_costs", t("object", "{area: cost} overriding the navmesh's area costs for this agent")),
        ("avoid_areas", t("array", "Areas it never enters, by name")),
        ("avoid_flags", t("array", "Areas it never enters, by flag")),
        ("remove", t("boolean", "Stop being a nav agent")),
        ("expect_version", t("integer", "Refuse unless the entity is at this version")),
    ]);
    let mut set_mind = ids();
    set_mind.extend([
        ("def", any("The definition (see ai_check), or the path of an entity whose props.mind_def holds one")),
        ("blackboard", t("object", "Starting blackboard values")),
        ("as_definition", t("boolean", "Store def as this entity's props.mind_def (a shared definition other minds name by path) instead of running it")),
        ("remove", t("boolean", "Remove the mind (or, with as_definition, the definition)")),
        ("expect_version", t("integer", "Refuse unless the entity is at this version")),
    ]);
    let mut status = ids();
    status.extend([("cursor", t("integer", "next_cursor of an earlier answer")), ("limit", t("integer", "Minds per page, 1..32 (default 8)"))]);
    vec![
        ToolDef {
            name: "nav_build".into(),
            description: "Build (or rebuild) a navmesh from the world's geometry: static colliders (exact) and drawn \
                          primitives without one (cube, quad, sphere, lathe bounds); dynamic bodies, characters, nav \
                          agents, triggers, hidden, ui, transient and 'nav_ignore'-tagged entities are left out, and \
                          models only count through a collider. Writes the settings as the entity /nav/<name> \
                          (props.navmesh; journaled, undoable, claims apply; the same settings again change nothing); \
                          the fixed ticks build it, tiles_per_tick of work each (while paused, time_control step), and \
                          afterwards world edits rebuild only the tiles they touch. Match a \
                          character with like: entity (its capsule radius and height, step_height and max_slope_deg), \
                          or give agent {radius 0.3, height 1.8, max_step 0.3, max_slope_deg 45}. Bounds default to the \
                          sources' bounds plus margin (1 m) and are then fixed. Returns tiles, polygons, links, \
                          milliseconds and problems."
                .into(),
            input_schema: schema(
                vec![
                    ("name", t("string", "Navmesh name (default \"default\"): the entity /nav/<name>")),
                    ("agent", t("object", "{radius, height, max_step, max_slope_deg} of the agents that walk it")),
                    ("like", any("A character entity (kinematic body with body.character and a capsule collider) to match")),
                    ("cell_size", t("number", "Grid cell width, m (default 0.15; 0.02..2)")),
                    ("cell_height", t("number", "Height resolution, m (default 0.05)")),
                    ("tile_cells", t("integer", "Cells per tile side (default 48; 8..86 at the default radius: a tile with its border, radius + 3 cells a side, is at most 96 cells)")),
                    ("bounds", t("object", "{min: [x,y,z], max: [x,y,z]} the navmesh covers")),
                    ("margin", t("number", "Metres added around the sources' bounds when bounds aren't given (default 1)")),
                    ("sources", t("object", "{colliders: true, shapes: true, kinematic: false, under: \"/level\", exclude_tags: [..]}")),
                    ("tiles_per_tick", t("integer", "Tile rebuild work per fixed tick after edits and while a build finishes, in default-size tiles (about 2 ms each; default 2; a tile with many triangles costs more)")),
                    ("areas", t("array", "Named areas, at most 31: [{name, cost?: 1 (0.1..1000, times the distance), flags?: [names]}]. Entities get one with props.nav_area: \"water\" (their walkable surfaces) or {area, volume: true} (their bounds mark the floor inside; they add no geometry). null removes them")),
                    ("links", t("array", "Off-mesh links, at most 256: [{from: [x,y,z], to: [x,y,z], kind?: jump|drop|ladder (default jump), bidirectional? (default: not for drops), cost?: 1, area?: name, radius?: 0.5 (how far from the navmesh its ends may be)}]. Paths and agents use them; changing only links rebuilds nothing. null removes them")),
                    ("remove", t("boolean", "Delete the navmesh /nav/<name> (/nav is written only by nav_build)")),
                    ("expect_version", t("integer", "Refuse unless the navmesh entity is at this version")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "nav_path".into(),
            description: "Find a path on a navmesh: A* over its polygons, then string-pulled (funnel). Returns status \
                          (complete | unreachable: to the reachable point nearest the goal | budget: max_nodes hit), \
                          corners (start, turns, end; on the navmesh), length (m), polygons, visited, and where from \
                          and to snapped onto the navmesh. Shown in sight_frame's 'paths' overlay."
                .into(),
            input_schema: schema(
                vec![
                    ("mesh", mesh()),
                    ("from", spot("Start")),
                    ("to", spot("Goal")),
                    ("max_nodes", t("integer", "Polygons the search may expand (default 4096)")),
                    ("max_corners", t("integer", "Corners to return, 2..256 (default 64)")),
                    ("area_costs", t("object", "{area: cost} overriding the navmesh's area costs (\"ground\" is plain ground)")),
                    ("avoid_areas", t("array", "Areas the path may not enter, by name")),
                    ("avoid_flags", t("array", "Areas the path may not enter, by flag")),
                ],
                &["from", "to"],
            ),
        },
        ToolDef {
            name: "nav_point".into(),
            description: "Points on a navmesh: the nearest one to 'at' (with its distance and whether 'at' is on the \
                          navmesh), and/or 'random' seeded random points (within 'radius' of 'at' when both are given)."
                .into(),
            input_schema: schema(
                vec![
                    ("mesh", mesh()),
                    ("at", spot("A place")),
                    ("random", t("integer", "How many random points, 0..32")),
                    ("radius", t("number", "Random points within this distance of 'at', m")),
                    ("seed", t("integer", "Seed for the random points (default 0)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "nav_agent".into(),
            description: "Make an entity a nav agent, or change one (props.nav_agent; journaled, undoable, claims apply): \
                          it walks its navmesh to 'goal' with the crowd (steering, avoiding other agents, sliding along \
                          walls). An entity with a physics character body is walked by the character controller (nav \
                          sets its walking velocity); others are moved along the navmesh surface. A mind (ai_set) \
                          decides goals instead of 'goal'. Returns the agent's state. Once it has walked, world_undo \
                          of this call is refused (undo_conflict: navigation moved it later); force: true undoes it and \
                          puts the entity back where it stood."
                .into(),
            input_schema: schema(agent, &[]),
        },
        ToolDef {
            name: "nav_info".into(),
            description: "Navigation at a glance: each navmesh (settings, tiles, polygons, links, dirty tiles waiting to \
                          rebuild, build errors, models skipped for want of a collider), the last tick (tiles rebuilt, \
                          minds, agents moving, repaths and those waiting, ms), problems (entities navigation can't use, \
                          and why) and the nav hash; with agents: true, the agents (status, position, goal, next corner, \
                          remaining length, stuck time), paged by id."
                .into(),
            input_schema: schema(
                vec![
                    ("mesh", mesh()),
                    ("agents", t("boolean", "List agents (paged)")),
                    ("cursor", t("integer", "next_cursor of an earlier answer")),
                    ("limit", t("integer", "Agents per page, 1..128 (default 32)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "ai_check".into(),
            description: "Validate an AI definition (behaviour tree or utility AI as JSON) without changing anything: \
                          diagnostics give the path, code, message and a hint. Nodes (one type key each): sequence [..] \
                          and selector [..] (reactive by default: earlier conditions / children are re-checked each \
                          tick), parallel [..] (succeed, fail), invert, force_success, force_failure, repeat (times), \
                          retry (times), cooldown / timeout (ticks or s), wait (ticks) or wait_s, {\"if\": condition, \
                          ...args}, {\"do\": action, ...args}, {\"set\": key, \"value\"}, {\"if\": \"bb\", key, op, value}, \
                          {\"utility\": [{label, weight, considerations: [{input, ...args, range [lo, hi], curve: linear | \
                          quadratic | inverse | step (at) | power (exponent) | logistic (steepness, midpoint)}], run: \
                          node}], momentum}. Conditions: sees, near, has_goal, arrived, on_navmesh. Actions: move_to, \
                          patrol, chase, flee, wander, stop, face. Inputs: distance, bb, prop. Targets: \"/path\", a \
                          unique name, an id, [x, y, z] or \"$key\" (a blackboard value). vocabulary: true lists every \
                          name with its arguments."
                .into(),
            input_schema: schema(
                vec![("def", any("The definition: {root: node, label} or a bare node")), ("vocabulary", t("boolean", "Also return the vocabulary"))],
                &[],
            ),
        },
        ToolDef {
            name: "ai_set".into(),
            description: "Give an entity a mind (props.mind; journaled, undoable, claims apply): the definition is \
                          validated first and refused with its diagnostics if invalid. It runs every fixed tick, \
                          deterministically; its move actions drive the entity's nav agent (nav_agent), so give it one \
                          to move. as_definition: true stores a shared definition (props.mind_def) that minds name by \
                          path instead. Once its agent has walked, world_undo of this call needs force: true (it also \
                          puts the entity back where it stood)."
                .into(),
            input_schema: schema(set_mind, &[]),
        },
        ToolDef {
            name: "ai_status".into(),
            description: "What minds are doing and why: the running path through the tree, each utility node's option \
                          scores and their considerations, the blackboard, a trace of recent decisions (what started, \
                          what took over, what stopped), problems, and the agent's navigation state. One entity, or a \
                          page of all minds by id. Reads show the state as of the last tick (a props.mind written with \
                          world_set appears after the next tick; ai_set shows it at once)."
                .into(),
            input_schema: schema(status, &[]),
        },
        ToolDef {
            name: "model_collider".into(),
            description: "Fit a physics collider to a model entity's built mesh and write it as the entity's collider \
                          (journaled, undoable, claims apply), so the model collides and carves navmeshes (models \
                          only feed a navmesh through a collider). kind: auto (a convex hull), recipe (the recipe's own \
                          fitted collider), box, capsule (upright), hull, or mesh (exact triangles, static bodies only). \
                          A moving body (dynamic or kinematic, or moving: true) gets a hull of at most 16 points, as \
                          physics requires. Returns a summary (shape, sizes or point / vertex / triangle counts, bounds, \
                          offset; world_query fields [\"collider\"] reads the collider itself) and a note when the fit \
                          changed what was asked."
                .into(),
            input_schema: schema(
                {
                    let mut p = ids();
                    p.extend([
                        ("kind", t("string", "auto | recipe | box | capsule | hull | mesh (default auto)")),
                        ("moving", t("boolean", "Fit for a moving body (default: whether its body is dynamic or kinematic)")),
                        ("expect_version", t("integer", "Refuse unless the entity is at this version")),
                    ]);
                    p
                },
                &[],
            ),
        },
    ]
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn r3(v: Vec3) -> Json {
    let r = |x: f32| Json::from((f64::from(x) * 1000.0).round() / 1000.0);
    Json::arr([r(v.x), r(v.y), r(v.z)])
}

impl crate::Engine {
    /// Navigation's fixed tick (before physics): journaled as `nav`.
    pub(crate) fn nav_tick(&mut self) {
        let tick = self.clock.tick();
        let hz = self.clock.hz();
        let _z = specula_core::profile::zone("nav");
        self.world.set_recording(crate::world::Recording::Snapshot);
        self.nav.tick(&mut self.world, &mut self.physics, self.world_generation, tick, 1.0 / hz as f32, hz);
        self.world.set_recording(crate::world::Recording::Full);
        self.swarm.game_changes(&mut self.world, tick, nav::AGENT, JKind::Nav);
    }

    fn spot(&self, s: &Spot) -> Result<Vec3, WorldError> {
        match s {
            Spot::Point(p) => Ok(*p),
            Spot::Entity(k) => {
                let id = self.world.resolve(k)?;
                Ok(self.world.get(id).expect("resolved").world_position())
            }
        }
    }

    pub(crate) fn nav_call(&mut self, call: NavCall, w: Writer, tick: u64) -> ToolOutput {
        match call {
            NavCall::Build { name, args, expect_version } => match self.nav_build(&name, &args, expect_version, w, tick) {
                Ok(j) => ToolOutput::text(j.to_pretty()),
                Err(e) => world_error(&e),
            },
            NavCall::Path { mesh, from, to, max_nodes, max_corners, prefs } => {
                let (mid, from, to) = match (self.nav.find_mesh(mesh.as_deref()), self.spot(&from), self.spot(&to)) {
                    (Ok(m), Ok(f), Ok(t)) => (m, f, t),
                    (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return world_error(&e),
                };
                // Reads see the navmesh as of the last tick (edits rebuild tiles on the tick).
                let slot = match nav::ready_mesh(&self.nav, mid) {
                    Ok(s) => s,
                    Err(e) => return world_error(&e),
                };
                let ext = Vec3::new(1.0, slot.settings.config.agent_height, 1.0);
                let (costs, avoid, avoid_flags) = nav::parse_area_prefs(&prefs, "").unwrap_or_default();
                let filter = slot.settings.filter(&costs, &avoid, &avoid_flags);
                let started = std::time::Instant::now();
                let Some(p) = slot.mesh.find_path_filtered(from, to, ext, max_nodes, &filter) else {
                    let near = slot.mesh.nearest(from, Vec3::new(10.0, 10.0, 10.0)).map(|(_, p)| r3(p));
                    return world_error(
                        &WorldError::new("start_off_navmesh", "'from' isn't on the navmesh (nothing within 1 m across and an agent height up or down)")
                            .with("from", r3(from))
                            .with("nearest_within_10m", near.unwrap_or(Json::Null)),
                    );
                };
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                let mut out = path_json(&p, max_corners);
                out.set("mesh", Json::from(slot.path.as_str()));
                out.set("from_snapped", r3(p.corners[0]));
                out.set("to_snapped", r3(*p.corners.last().expect("start and end")));
                out.set("ms", Json::from((ms * 1000.0).round() / 1000.0));
                if !p.complete() {
                    out.set(
                        "why",
                        Json::from(match p.status {
                            specula_via::PathStatus::Budget => "the search expanded max_nodes polygons before reaching the goal: raise max_nodes",
                            _ => "no walkable connection to the goal (or the goal is off the navmesh): the path ends at the reachable point nearest it",
                        }),
                    );
                }
                self.nav.remember_query(mid, p);
                ToolOutput::text(out.to_string())
            }
            NavCall::Point { mesh, at, random, radius, seed } => {
                let mid = match self.nav.find_mesh(mesh.as_deref()) {
                    Ok(m) => m,
                    Err(e) => return world_error(&e),
                };
                let at = match at.as_ref().map(|s| self.spot(s)).transpose() {
                    Ok(a) => a,
                    Err(e) => return world_error(&e),
                };
                let slot = match nav::ready_mesh(&self.nav, mid) {
                    Ok(s) => s,
                    Err(e) => return world_error(&e),
                };
                let mut out = Json::obj([("mesh", Json::from(slot.path.as_str()))]);
                if let Some(a) = at {
                    let h = slot.settings.config.agent_height;
                    match slot.mesh.nearest(a, Vec3::new(5.0, h * 2.0, 5.0)) {
                        Some((_, p)) => {
                            let d = (p - a).length();
                            out.set("nearest", r3(p));
                            out.set("distance", Json::from((f64::from(d) * 1000.0).round() / 1000.0));
                            out.set("on_navmesh", Json::from(slot.mesh.nearest(a, Vec3::new(0.01, h, 0.01)).is_some()));
                        }
                        None => {
                            out.set("nearest", Json::Null);
                            out.set("on_navmesh", Json::from(false));
                            out.set("why", Json::from("no navmesh within 5 m across and two agent heights up or down"));
                        }
                    }
                }
                if random > 0 {
                    let mut rng = Rng::new(seed, 0x6e61_7670);
                    let mut points = Vec::new();
                    let mut tries = 0;
                    while points.len() < random && tries < random * 4 {
                        tries += 1;
                        let found = match (at, radius) {
                            (Some(a), Some(r)) => slot.mesh.random_point_near(&mut rng, a, r),
                            _ => slot.mesh.random_point(&mut rng),
                        };
                        if let Some((_, p)) = found {
                            points.push(r3(p));
                        }
                    }
                    out.set("random", Json::Arr(points));
                }
                ToolOutput::text(out.to_string())
            }
            NavCall::Agent { entity, patch, remove, expect_version } => {
                let id = match self.world.resolve(&entity) {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                let value = if remove {
                    Json::Null
                } else {
                    let mut merged = self.world.get(id).and_then(|e| e.props.get(nav::AGENT_KEY)).cloned().unwrap_or(Json::Obj(Vec::new()));
                    if let Json::Obj(pairs) = &patch {
                        for (k, v) in pairs {
                            merged.set(k.clone(), v.clone());
                        }
                    }
                    // A null removes that setting.
                    if let Json::Obj(pairs) = &mut merged {
                        pairs.retain(|(_, v)| !v.is_null());
                    }
                    if let Err(e) = AgentSpec::from_json(&merged) {
                        return world_error(&e);
                    }
                    merged
                };
                let change = UpdateSpec { props: Some(Json::obj([(nav::AGENT_KEY, value)])), expect_version, ..Default::default() };
                let by = w.name.to_owned();
                match self.swarm.commit(&mut self.world, tick, w, JKind::Set, |wd| wd.update(&EntityKey::Id(id), change, &by).map(|e| e.id)) {
                    Ok((id, seq)) => {
                        self.nav.sync(&self.world, self.world_generation);
                        let mut out = self.nav.agent_json(&self.world, id).unwrap_or_else(|| Json::obj([("id", Json::from(id)), ("nav_agent", Json::from(false))]));
                        out.set("journal", Json::from(seq));
                        ToolOutput::text(out.to_pretty())
                    }
                    Err(e) => world_error(&e),
                }
            }
            NavCall::Info { mesh, agents, after, limit } => {
                let mut out = self.nav.summary_json();
                if let Some(m) = mesh {
                    match self.nav.find_mesh(Some(&m)) {
                        Ok(mid) => {
                            if let Some(slot) = self.nav.meshes.get(&mid) {
                                out.set("settings", slot.settings.to_json());
                            }
                        }
                        Err(e) => return world_error(&e),
                    }
                }
                let (problems, left) = self.nav.problems_json(&self.world, MAX_PROBLEM_BYTES);
                out.set("problem_list", problems);
                if left > 0 {
                    out.set("problems_not_listed", Json::from(left));
                }
                if agents {
                    // A page of agents, cut short at the byte cap (next_cursor continues it).
                    let mut bytes = out.to_string().len();
                    let mut page = Vec::new();
                    let mut last = None;
                    let mut more = false;
                    for id in self.nav.agent_ids().filter(|id| *id > after) {
                        if page.len() == limit {
                            more = true;
                            break;
                        }
                        let Some(j) = self.nav.agent_json(&self.world, id) else { continue };
                        let len = j.to_string().len() + 1;
                        if bytes + len > MAX_INFO_BYTES && !page.is_empty() {
                            more = true;
                            break;
                        }
                        bytes += len;
                        page.push(j);
                        last = Some(id);
                    }
                    out.set("agent_list", Json::Arr(page));
                    out.set("next_cursor", Json::from(if more { last } else { None }));
                }
                ToolOutput::text(out.to_string())
            }
            NavCall::Check { def, vocabulary } => {
                let mut out = Json::Obj(Vec::new());
                if !def.is_null() {
                    match Def::parse(&def, self.nav.vocab()) {
                        Ok(d) => {
                            out.set("valid", Json::from(true));
                            out.set("summary", d.summary_json());
                        }
                        Err(diags) => {
                            out.set("valid", Json::from(false));
                            out.set("diagnostics", Json::arr(diags.iter().map(specula_mens::Diagnostic::to_json)));
                        }
                    }
                }
                if vocabulary {
                    out.set("vocabulary", self.nav.vocab().to_json());
                }
                ToolOutput::text(out.to_pretty())
            }
            NavCall::SetMind { entity, def, blackboard, as_definition, remove, expect_version } => {
                let id = match self.world.resolve(&entity) {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                let key = if as_definition { nav::DEF_KEY } else { nav::MIND_KEY };
                let value = if remove {
                    Json::Null
                } else {
                    // Validate what will run.
                    let to_check = match &def {
                        Json::Str(p) if !as_definition => {
                            let found = self.world.find_path(p).ok().and_then(|d| self.world.get(d)).and_then(|e| e.props.get(nav::DEF_KEY).cloned());
                            match found {
                                Some(d) => d,
                                None => {
                                    return world_error(
                                        &WorldError::new("no_definition", format!("'{p}' isn't an entity with props.mind_def (store one with ai_set as_definition: true)"))
                                            .with("field", "def"),
                                    )
                                }
                            }
                        }
                        Json::Obj(_) => def.clone(),
                        _ => return world_error(&WorldError::new("invalid_value", "'def' must be a definition object (or, for a mind, a definition entity's path)").with("field", "def")),
                    };
                    if let Err(diags) = Def::parse(&to_check, self.nav.vocab()) {
                        return ToolOutput::error(
                            Json::obj([
                                ("error", Json::from("invalid_definition")),
                                ("message", Json::from(format!("the definition has {} problem(s); nothing was written", diags.len()))),
                                ("diagnostics", Json::arr(diags.iter().map(specula_mens::Diagnostic::to_json))),
                            ])
                            .to_pretty(),
                        );
                    }
                    if as_definition {
                        def
                    } else {
                        let mut m = vec![("def", def)];
                        if let Some(b) = blackboard {
                            m.push(("blackboard", b));
                        }
                        Json::obj(m)
                    }
                };
                let change = UpdateSpec { props: Some(Json::obj([(key, value)])), expect_version, ..Default::default() };
                let by = w.name.to_owned();
                match self.swarm.commit(&mut self.world, tick, w, JKind::Set, |wd| wd.update(&EntityKey::Id(id), change, &by).map(|e| e.id)) {
                    Ok((id, seq)) => {
                        self.nav.sync(&self.world, self.world_generation);
                        let mut out = self.nav.mind_json(&self.world, id, 4096).unwrap_or_else(|| Json::obj([("id", Json::from(id)), ("path", Json::from(self.world.path_of(id)))]));
                        if self.nav.agent_json(&self.world, id).is_none() && !as_definition && !remove {
                            out.set("warning", Json::from("this entity isn't a nav agent: its mind can sense but not move (give it one with nav_agent)"));
                        }
                        out.set("journal", Json::from(seq));
                        ToolOutput::text(out.to_pretty())
                    }
                    Err(e) => world_error(&e),
                }
            }
            NavCall::ModelCollider { entity, kind, moving, expect_version } => match self.model_collider(&entity, &kind, moving, expect_version, w, tick) {
                Ok(j) => ToolOutput::text(j.to_pretty()),
                Err(e) => world_error(&e),
            },
            NavCall::Status { entity, after, limit } => {
                match entity {
                    Some(k) => match self.world.resolve(&k) {
                        Ok(id) => match self.nav.mind_json(&self.world, id, MAX_STATUS_BYTES / 2) {
                            Some(j) => ToolOutput::text(j.to_pretty()),
                            None => world_error(&WorldError::new("no_mind", "this entity has no mind (give it one with ai_set)").with("id", id)),
                        },
                        Err(e) => world_error(&e),
                    },
                    None => {
                        let ids: Vec<u64> = self.nav.mind_ids().filter(|id| *id > after).collect();
                        let mut list = Vec::new();
                        let mut bytes = 0;
                        let mut last = None;
                        for id in ids.iter().take(limit) {
                            let Some(j) = self.nav.mind_json(&self.world, *id, 2048) else { continue };
                            let size = j.to_string().len();
                            if !list.is_empty() && bytes + size > MAX_STATUS_BYTES {
                                break;
                            }
                            bytes += size;
                            list.push(j);
                            last = Some(*id);
                        }
                        let more = last.is_some_and(|l| ids.iter().any(|i| *i > l));
                        ToolOutput::text(
                            Json::obj([("minds", Json::Arr(list)), ("total", Json::from(self.nav.mind_ids().count())), ("next_cursor", Json::from(if more { last } else { None }))]).to_string(),
                        )
                    }
                }
            }
        }
    }

    /// model_collider: a collider fitted to a model entity's built mesh, written as its collider.
    fn model_collider(&mut self, entity: &EntityKey, kind: &str, moving: Option<bool>, expect_version: Option<u64>, w: Writer, tick: u64) -> Result<Json, WorldError> {
        let id = self.world.resolve(entity)?;
        let e = self.world.get(id).expect("resolved");
        if e.shape != Shape::Model {
            return Err(WorldError::new("not_a_model", "model_collider fits a collider to a model entity (shape \"model\")").with("id", id));
        }
        let Some(r) = e.resolved_model().filter(|r| r.state == crate::world::ModelState::Ready) else {
            return Err(WorldError::new("model_not_ready", format!("the model isn't built yet ({}): call again when it is", e.model_state())).with("id", id));
        };
        let moving = moving.unwrap_or(e.body.is_some_and(|b| b.kind != crate::physics::BodyKind::Static));
        let built = self.library.lock().built(r.handle).cloned().ok_or_else(|| WorldError::new("model_not_ready", "the model's build is gone: call again").with("id", id))?;
        let (collider, note) = fit_model_collider(&built.model, kind, moving)?;
        collider.validate()?;
        let summary = collider_summary(&collider);
        let change = UpdateSpec { collider: Some(Some(collider.clone())), expect_version, ..Default::default() };
        let by = w.name.to_owned();
        let (_, seq) = self.swarm.commit(&mut self.world, tick, w, JKind::Set, |wd| wd.update(&EntityKey::Id(id), change, &by).map(|e| e.id))?;
        // A summary, not the collider itself (a mesh collider is hundreds of KB; world_query
        // fields: ["collider"] reads it).
        let mut out = Json::obj([("id", Json::from(id)), ("path", Json::from(self.world.path_of(id))), ("moving", Json::from(moving)), ("journal", Json::from(seq))]);
        if let Json::Obj(pairs) = summary {
            for (k, v) in pairs {
                out.set(k, v);
            }
        }
        if let Some(n) = note {
            out.set("note", Json::from(n));
        }
        Ok(out)
    }

    /// nav_build: settings from the arguments, the navmesh entity written, every tile built.
    fn nav_build(&mut self, name: &str, args: &Json, expect_version: Option<u64>, w: Writer, tick: u64) -> Result<Json, WorldError> {
        let path = format!("/nav/{name}");
        let existing = self.world.find_path(&path).ok();
        // /nav is reserved for these tools: removing a navmesh is one of them too.
        if args.get("remove").and_then(Json::as_bool) == Some(true) {
            let Some(id) = existing else { return Err(WorldError::new("no_navmesh", format!("there is no navmesh '{name}'")).with("path", path.as_str())) };
            let (_, seq) = self.swarm.commit_nav(&mut self.world, tick, w, JKind::Delete, |wd| wd.delete(&EntityKey::Id(id), expect_version).map(|_| ()))?;
            self.nav.sync(&self.world, self.world_generation);
            return Ok(Json::obj([("removed", Json::from(path.as_str())), ("journal", Json::from(seq))]));
        }
        // Start from the current settings when rebuilding one.
        let mut settings = match existing.and_then(|id| self.world.get(id)).and_then(|e| e.props.get(nav::MESH_KEY)) {
            Some(p) => MeshSettings::from_json(p)?,
            None => MeshSettings::new(specula_via::Config::default(), Sources::default(), nav::DEFAULT_TILES_PER_TICK),
        };
        let c = &mut settings.config;
        if let Some(like) = args.get("like").filter(|v| !v.is_null()) {
            let key = key_value(like, "like").map_err(|m| WorldError::new("invalid_argument", m).with("field", "like"))?;
            let id = self.world.resolve(&key)?;
            let e = self.world.get(id).expect("resolved");
            let (Some(body), Some(col)) = (e.body, e.collider.as_ref()) else {
                return Err(WorldError::new("not_a_character", "'like' must be a character: a kinematic body with body.character and a capsule collider").with("id", id));
            };
            let Some(ch) = body.character else {
                return Err(WorldError::new("not_a_character", "'like' has no body.character").with("id", id));
            };
            let (_, scale) = crate::physics::world_pose(&self.world, e);
            match col.shape {
                crate::physics::ColliderShape::Capsule { radius, height } => {
                    c.agent_radius = radius * scale.x.max(scale.z);
                    c.agent_height = height * scale.y;
                }
                crate::physics::ColliderShape::Sphere { radius } => {
                    c.agent_radius = radius * scale.x.max(scale.z);
                    c.agent_height = 2.0 * radius * scale.y;
                }
                crate::physics::ColliderShape::Box { size } => {
                    c.agent_radius = 0.5 * (size.x * scale.x).max(size.z * scale.z);
                    c.agent_height = size.y * scale.y;
                }
                _ => return Err(WorldError::new("not_a_character", "'like' needs a capsule, sphere or box collider").with("id", id)),
            }
            c.max_step = ch.step_height;
            c.max_slope_deg = ch.max_slope_deg;
        }
        if let Some(a) = args.get("agent").filter(|v| !v.is_null()) {
            check_keys(a, "agent", &[&["radius", "height", "max_step", "max_slope_deg"]]).map_err(|m| WorldError::new("invalid_argument", m).with("field", "agent"))?;
            let num = |k: &str| a.get(k).and_then(Json::as_f64).map(|f| f as f32);
            if let Some(v) = num("radius") {
                c.agent_radius = v;
            }
            if let Some(v) = num("height") {
                c.agent_height = v;
            }
            if let Some(v) = num("max_step") {
                c.max_step = v;
            }
            if let Some(v) = num("max_slope_deg") {
                c.max_slope_deg = v;
            }
        }
        let num = |k: &str| args.get(k).and_then(Json::as_f64).map(|f| f as f32);
        if let Some(v) = num("cell_size") {
            c.cell_size = v;
        }
        if let Some(v) = num("cell_height") {
            c.cell_height = v;
        }
        if let Some(v) = args.get("tile_cells").and_then(Json::as_u64) {
            c.tile_cells = v.min(u64::from(u32::MAX)) as u32;
        }
        if let Some(v) = args.get("tiles_per_tick").filter(|v| !v.is_null()) {
            settings.tiles_per_tick = v.as_u64().filter(|n| (1..=256).contains(n)).ok_or_else(|| WorldError::new("invalid_value", "'tiles_per_tick' must be 1..256").with("field", "tiles_per_tick"))? as usize;
        }
        if let Some(s) = args.get("sources").filter(|v| !v.is_null()) {
            // Parse through the settings reader (same checks as world data).
            let probe = Json::obj([("sources", s.clone())]);
            settings.sources = MeshSettings::from_json(&probe)?.sources;
        }
        let c = &mut settings.config;
        match args.get("bounds").filter(|v| !v.is_null()) {
            Some(b) => {
                let pt = |k: &str| b.get(k).and_then(nav::point_of);
                match (pt("min"), pt("max")) {
                    (Some(lo), Some(hi)) => {
                        c.bounds_min = lo;
                        c.bounds_max = hi;
                    }
                    _ => return Err(WorldError::new("invalid_value", "'bounds' must be {min: [x, y, z], max: [x, y, z]}").with("field", "bounds")),
                }
            }
            None if existing.is_none() || args.get("margin").is_some() || args.get("sources").is_some() => {
                let margin = num("margin").unwrap_or(1.0).clamp(0.0, 1000.0);
                let Some((lo, hi)) = nav::source_bounds(&mut self.nav, &self.world, self.world_generation, &settings.sources) else {
                    return Err(WorldError::new(
                        "no_geometry",
                        "nothing to build a navmesh from: no static colliders or drawn primitives (cubes, quads, ...) match the sources; spawn a floor first, or give 'bounds'",
                    ));
                };
                let c = &mut settings.config;
                c.bounds_min = Vec3::new(lo.x - margin, lo.y - 1.0, lo.z - margin);
                c.bounds_max = Vec3::new(hi.x + margin, hi.y + c.agent_height + 1.0, hi.z + margin);
            }
            None => {}
        }
        // Check everything before writing (a tile too wide is refused here; only old world data
        // is narrowed when read).
        let mut props = settings.to_json();
        for k in ["areas", "links"] {
            if let Some(v) = args.get(k) {
                if v.is_null() {
                    if let Json::Obj(pairs) = &mut props {
                        pairs.retain(|(n, _)| n != k);
                    }
                } else {
                    props.set(k, v.clone());
                }
            }
        }
        let read = MeshSettings::from_json(&props)?;
        let props = read.to_json();
        if read.config.tile_cells != settings.config.tile_cells {
            return Err(WorldError::new(
                "invalid_value",
                format!("'tile_cells' {} is too wide: with its border a tile is at most {} cells, so at most {} here", settings.config.tile_cells, specula_via::MAX_TILE_SIDE, read.config.tile_cells),
            )
            .with("field", "tile_cells")
            .with("max", Json::from(read.config.tile_cells)));
        }
        // The same settings again: nothing to write or do (how many calls were made never
        // matters; building happens on the ticks).
        let unchanged = existing.and_then(|id| self.world.get(id)).and_then(|e| e.props.get(nav::MESH_KEY)).is_some_and(|p| *p == props || MeshSettings::from_json(p).is_ok_and(|old| old == read)) && expect_version.is_none();
        let mut ops = Vec::new();
        if self.world.find_path("/nav").is_err() {
            let mut g = SpawnSpec::new(Shape::Group);
            g.name = Some("nav".into());
            ops.push(Op::Spawn(g));
        }
        match existing {
            Some(id) => ops.push(Op::Set(EntityKey::Id(id), UpdateSpec { props: Some(Json::obj([(nav::MESH_KEY, props.clone())])), expect_version, ..Default::default() })),
            None => {
                let mut s = SpawnSpec::new(Shape::Group);
                s.name = Some(name.to_owned());
                s.parent = Some(EntityKey::Path("/nav".into()));
                s.props = Json::obj([(nav::MESH_KEY, props.clone())]);
                s.tags.insert("navmesh".into());
                ops.push(Op::Spawn(s));
            }
        }
        let seq = if unchanged {
            None
        } else {
            let by = w.name.to_owned();
            self.swarm.commit_nav(&mut self.world, tick, w, JKind::Batch, |wd| wd.apply_batch(ops, &by).map_err(|(i, e)| e.with("op_index", i)))?.1
        };
        let id = self.world.find_path(&path)?;
        self.nav.sync(&self.world, self.world_generation);
        if let Some(p) = self.nav.problems.get(&id) {
            return Err(WorldError::new("navmesh_invalid", "the navmesh entity was written but its settings don't build").with("problem", p.clone()).with("journal", Json::from(seq)));
        }
        // No building here: the ticks build it within tiles_per_tick (the old settings' navmesh,
        // if any, serves until the new one is complete), so the navmesh at each tick depends only
        // on the world and the ticks, never on how many calls were made.
        let slot = self.nav.meshes.get(&id).ok_or_else(|| WorldError::new("navmesh_invalid", "the navmesh entity was written but didn't build").with("journal", Json::from(seq)))?;
        let mut out = slot.mesh.summary_json();
        out.set("path", Json::from(path.as_str()));
        out.set("settings", slot.settings.to_json());
        if unchanged {
            out.set("unchanged", Json::from(true));
        }
        let (left, total) = match &slot.next {
            Some(n) => (n.queue.len(), n.mesh.tile_count()),
            None => (slot.dirty.len(), slot.mesh.tile_count()),
        };
        out.set("complete", Json::from(left == 0 && slot.ready));
        if left > 0 {
            out.set("tiles_left", Json::from(left));
            out.set("tiles_total", Json::from(total.0 as usize * total.1 as usize));
            out.set(
                "note",
                Json::from(if slot.next.is_some() {
                    "built over the next ticks (tiles_per_tick; while paused, time_control step advances them; nav_info shows progress); the navmesh with the old settings serves until then"
                } else {
                    "built over the next ticks (tiles_per_tick; while paused, time_control step advances them; nav_info shows progress); queries answer navmesh_building until then"
                }),
            );
        }
        if slot.models_skipped > 0 {
            out.set("models_without_collider", Json::from(slot.models_skipped));
        }
        if !slot.errors.is_empty() {
            out.set("errors", Json::arr(slot.errors.iter().cloned()));
        }
        if left == 0 && slot.mesh.poly_count() == 0 {
            out.set(
                "warning",
                Json::from("the navmesh is empty: no walkable surface wide enough for agent_radius and tall enough for agent_height (check the floor's slope, the bounds' height and the sources)"),
            );
        }
        out.set("journal", Json::from(seq));
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------
// Overlays

const PATH: [f32; 4] = [1.0, 0.85, 0.2, 1.0];
const QUERY: [f32; 4] = [1.0, 0.5, 0.15, 1.0];
const AGENT_RING: [f32; 4] = [0.35, 0.95, 1.0, 1.0];
const GOAL: [f32; 4] = [1.0, 0.35, 0.85, 1.0];
const DIRTY: [f32; 4] = [1.0, 0.45, 0.1, 1.0];
const LINK: [f32; 4] = [1.0, 0.95, 0.3, 1.0];
const LINK_MISSING: [f32; 4] = [1.0, 0.2, 0.2, 1.0];

/// Fill and edge of a named area's polygons (by id; plain ground uses the tile colours).
fn area_color(area: u8) -> ([f32; 4], [f32; 4]) {
    const P: [[f32; 3]; 6] = [[0.85, 0.35, 0.95], [0.95, 0.75, 0.2], [0.95, 0.35, 0.35], [0.3, 0.9, 0.9], [0.6, 0.95, 0.3], [0.95, 0.55, 0.75]];
    let c = P[usize::from(area.saturating_sub(1)) % P.len()];
    ([c[0] * 0.8, c[1] * 0.8, c[2] * 0.8, 0.4], [c[0], c[1], c[2], 0.95])
}

fn tile_color(tx: u32, tz: u32) -> ([f32; 4], [f32; 4]) {
    if (tx + tz) % 2 == 0 {
        ([0.2, 0.75, 0.45, 0.28], [0.45, 1.0, 0.65, 0.85])
    } else {
        ([0.2, 0.55, 0.85, 0.28], [0.45, 0.8, 1.0, 0.85])
    }
}

fn ring(p: &mut Painter, c: Vec3, r: f32, color: [f32; 4], width: f32) {
    let mut prev = None;
    for i in 0..=20 {
        let (s, co) = specula_core::dmath::sin_cos(i as f32 * std::f32::consts::TAU / 20.0);
        let q = c + Vec3::new(co * r, 0.0, s * r);
        if let Some(pr) = prev {
            p.line(pr, q, color, width);
        }
        prev = Some(q);
    }
}

/// Draws the `navmesh` and `paths` overlays; returns their packet details.
pub fn paint(nav: &nav::Nav, world: &World, p: &mut Painter, navmesh: bool, paths: bool) -> Vec<(&'static str, Json)> {
    let mut details = Vec::new();
    let lift = Vec3::new(0.0, 0.03, 0.0);
    if navmesh {
        let mut list = Vec::new();
        let mut drawn = 0usize;
        let mut omitted = 0usize;
        for slot in nav.meshes.values() {
            let mesh = &slot.mesh;
            let mut polys = 0usize;
            for (index, tile) in mesh.tiles() {
                let (tx, tz) = mesh.tile_coords(index);
                let ground = tile_color(tx, tz);
                for poly in &tile.polys {
                    if drawn >= MAX_DRAWN_POLYS {
                        omitted += 1;
                        continue;
                    }
                    let (fill, edge) = if poly.area == 0 { ground } else { area_color(poly.area) };
                    let r = mesh.rect(poly);
                    let at = |x: f32, z: f32| Vec3::new(x, mesh.height(poly, x, z), z) + lift;
                    let corners = [at(r[0], r[1]), at(r[2], r[1]), at(r[2], r[3]), at(r[0], r[3])];
                    p.polygon(&corners, fill);
                    for i in 0..4 {
                        p.line(corners[i], corners[(i + 1) % 4], edge, 1.0);
                    }
                    drawn += 1;
                    polys += 1;
                }
            }
            for &(tx, tz) in &slot.dirty {
                let r = mesh.tile_rect(tx, tz);
                let y = slot.settings.config.bounds_min.y + 1.0;
                let c = [Vec3::new(r[0], y, r[1]), Vec3::new(r[2], y, r[1]), Vec3::new(r[2], y, r[3]), Vec3::new(r[0], y, r[3])];
                for i in 0..4 {
                    p.line(c[i], c[(i + 1) % 4], DIRTY, 2.0);
                }
            }
            // Off-mesh links: an arc from take-off to landing (red where an end found no navmesh).
            let mut links = Vec::new();
            for (i, l) in mesh.offmesh_links().iter().enumerate() {
                let found = mesh.resolved_link(i);
                let (a, b) = found.map_or((l.from, l.to), |r| (r.start_pos, r.end_pos));
                let color = if found.is_some() { LINK } else { LINK_MISSING };
                let lift_m = match l.kind {
                    specula_via::LinkKind::Jump => ((b - a).length() * 0.25).clamp(0.3, 2.0),
                    _ => 0.0,
                };
                let mut prev = a + lift;
                for k in 1..=12 {
                    let u = k as f32 / 12.0;
                    let q = a + (b - a) * u + Vec3::new(0.0, 4.0 * lift_m * u * (1.0 - u), 0.0) + lift;
                    p.line(prev, q, color, 2.5);
                    prev = q;
                }
                p.dot(a + lift, 5.0, color);
                p.dot(b + lift, if l.bidirectional { 5.0 } else { 3.0 }, color);
                if links.len() < 32 {
                    links.push(Json::obj([("from", r3(a)), ("to", r3(b)), ("kind", Json::from(l.kind.name())), ("bidirectional", Json::from(l.bidirectional)), ("found", Json::from(found.is_some()))]));
                }
            }
            let areas: Vec<Json> = slot
                .settings
                .areas
                .iter()
                .enumerate()
                .map(|(i, a)| Json::obj([("name", Json::from(a.name.as_str())), ("color", Json::from(ov::hex(area_color((i + 1) as u8).1)))]))
                .collect();
            let mut j = Json::obj([
                ("path", Json::from(slot.path.as_str())),
                ("polygons", Json::from(mesh.poly_count())),
                ("drawn", Json::from(polys)),
                ("tiles_built", Json::from(mesh.tiles().count())),
                ("dirty_tiles", Json::from(slot.dirty.len())),
            ]);
            if !links.is_empty() {
                j.set("offmesh_links", Json::Arr(links));
            }
            if !areas.is_empty() {
                j.set("areas", Json::Arr(areas));
            }
            list.push(j);
        }
        details.push((
            "navmesh",
            Json::obj([
                ("navmeshes", Json::Arr(list)),
                ("omitted_polygons", Json::from(omitted)),
                (
                    "legend",
                    Json::obj([
                        ("polygons", Json::from("green / blue by tile (alternating); named areas in their own colour (see each navmesh's areas)")),
                        ("dirty_tile", Json::from(ov::hex(DIRTY))),
                        ("offmesh_link", Json::from(ov::hex(LINK))),
                        ("offmesh_link_not_found", Json::from(ov::hex(LINK_MISSING))),
                    ]),
                ),
            ]),
        ));
    }
    if paths {
        let mut agents = Vec::new();
        let mut corners_drawn = 0usize;
        for nav::AgentPath { id, pos, corners, target, vel, radius } in nav.agent_paths(world, MAX_DRAWN_CORNERS) {
            let y = Vec3::new(0.0, 0.06, 0.0);
            for wnd in corners.windows(2) {
                p.line(wnd[0] + y, wnd[1] + y, PATH, 2.5);
            }
            for c in corners.iter().skip(1) {
                p.dot(*c + y, 3.5, PATH);
                corners_drawn += 1;
            }
            ring(p, pos + y, radius, AGENT_RING, 2.0);
            if vel.x * vel.x + vel.z * vel.z > 1.0e-4 {
                p.line(pos + y, pos + y + Vec3::new(vel.x, 0.0, vel.z) * 0.5, [1.0, 1.0, 1.0, 1.0], 2.0);
            }
            if let Some(t) = target {
                ring(p, t + y, 0.25, GOAL, 2.0);
            }
            if agents.len() < 32 {
                agents.push(Json::obj([
                    ("path", Json::from(world.path_of(id))),
                    ("position", r3(pos)),
                    ("corners", Json::from(corners.len())),
                    ("target", target.map_or(Json::Null, r3)),
                ]));
            }
        }
        let (mut queries, mut query_corners, mut stale) = (0, 0, 0);
        for (mesh, counter, q) in &nav.queries {
            // An answer from before its navmesh changed (or went) is no longer what nav_path says.
            if nav.meshes.get(mesh).is_none_or(|m| m.mesh.counter() != *counter) {
                stale += 1;
                continue;
            }
            query_corners += q.corners.len();
            let y = Vec3::new(0.0, 0.09, 0.0);
            for wnd in q.corners.windows(2) {
                p.line(wnd[0] + y, wnd[1] + y, QUERY, 3.0);
            }
            for c in &q.corners {
                p.dot(*c + y, 4.0, QUERY);
            }
            queries += 1;
        }
        details.push((
            "paths",
            Json::obj([
                ("agents", Json::Arr(agents)),
                ("agent_corners", Json::from(corners_drawn)),
                ("query_corners", Json::from(query_corners)),
                ("queries", Json::from(queries)),
                ("stale_queries_hidden", Json::from(stale)),
                (
                    "legend",
                    Json::obj([
                        ("agent_path", Json::from(ov::hex(PATH))),
                        ("nav_path_query", Json::from(ov::hex(QUERY))),
                        ("agent", Json::from(ov::hex(AGENT_RING))),
                        ("goal", Json::from(ov::hex(GOAL))),
                        ("velocity", Json::from("#ffffff")),
                    ]),
                ),
            ]),
        ));
    }
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::ColliderShape;

    #[test]
    fn only_the_nav_tools_write_navmesh_data() {
        use crate::journal::Kind;
        let mut w = World::default();
        w.set_recording(crate::world::Recording::Full);
        let mut sw = crate::swarm::Swarm::default();
        let ann = Writer { name: "ann", session: Some("s1") };
        let group = |name: &str, parent: Option<&str>| {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(name.into());
            s.parent = parent.map(|p| EntityKey::Path(p.into()));
            s
        };
        let code = |r: Result<((), Option<u64>), WorldError>| r.err().map(|e| e.code);
        // world_* writers may not make /nav, nor settings anywhere.
        let r = sw.commit(&mut w, 0, ann, Kind::Spawn, |wd| wd.spawn(group("nav", None), "ann").map(|_| ()));
        assert_eq!(code(r), Some("reserved"));
        let mut floor = SpawnSpec::new(Shape::Cube);
        floor.name = Some("floor".into());
        sw.commit(&mut w, 0, ann, Kind::Spawn, |wd| wd.spawn(floor, "ann").map(|_| ())).unwrap();
        let settings = UpdateSpec { props: Some(Json::obj([(nav::MESH_KEY, Json::Obj(Vec::new()))])), ..Default::default() };
        let r = sw.commit(&mut w, 0, ann, Kind::Set, |wd| wd.update(&EntityKey::Path("/floor".into()), settings, "ann").map(|_| ()));
        assert_eq!(code(r), Some("reserved"));
        // The nav tools may.
        sw.commit_nav(&mut w, 0, ann, Kind::Spawn, |wd| wd.spawn(group("nav", None), "ann").map(|_| ())).unwrap();
        let mut s = group("default", Some("/nav"));
        s.props = Json::obj([(nav::MESH_KEY, Json::Obj(Vec::new()))]);
        sw.commit_nav(&mut w, 0, ann, Kind::Spawn, |wd| wd.spawn(s, "ann").map(|_| ())).unwrap();
        // A cube with a collider under /nav (the bypass the review found), or moving a settings
        // group, through the world tools: refused.
        let mut wall = SpawnSpec::new(Shape::Cube);
        wall.name = Some("wall".into());
        wall.parent = Some(EntityKey::Path("/nav".into()));
        wall.collider = Some(crate::physics::Collider::default());
        let r = sw.commit(&mut w, 0, ann, Kind::Spawn, |wd| wd.spawn(wall, "ann").map(|_| ()));
        assert_eq!(code(r), Some("reserved"));
        let r = sw.commit(&mut w, 0, ann, Kind::Set, |wd| wd.update(&EntityKey::Path("/nav/default".into()), UpdateSpec { position: Some(Vec3::new(1.0, 0.0, 0.0)), ..Default::default() }, "ann").map(|_| ()));
        assert_eq!(code(r), Some("reserved"));
        let r = sw.commit(&mut w, 0, ann, Kind::Delete, |wd| wd.delete(&EntityKey::Path("/nav/default".into()), None).map(|_| ()));
        assert_eq!(code(r), Some("reserved"));
        assert!(w.find_path("/nav/default").is_ok());
    }

    fn model(recipe: &str) -> specula_model::Model {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let style = specula_model::Style::parse(&std::fs::read_to_string(assets.join("style.json")).unwrap()).unwrap();
        specula_model::build(&std::fs::read_to_string(assets.join("recipes").join(recipe)).unwrap(), &specula_model::BuildParams::new(&style)).unwrap()
    }

    #[test]
    fn colliders_fit_models_within_physics_limits() {
        let m = model("tombstone.json");
        // A moving body: a hull of at most 16 points, which physics accepts.
        let (c, _) = fit_model_collider(&m, "auto", true).unwrap();
        match &c.shape {
            ColliderShape::Hull { points } => assert!((4..=16).contains(&points.len()), "{}", points.len()),
            s => panic!("{s:?}"),
        }
        c.validate().unwrap();
        // A static body may have a finer hull, or the exact mesh.
        let (fine, _) = fit_model_collider(&m, "hull", false).unwrap();
        assert!(matches!(&fine.shape, ColliderShape::Hull { points } if points.len() >= 4));
        fine.validate().unwrap();
        let (mesh, _) = fit_model_collider(&m, "mesh", false).unwrap();
        assert!(matches!(mesh.shape, ColliderShape::Mesh { .. }));
        mesh.validate().unwrap();
        assert_eq!(fit_model_collider(&m, "mesh", true).unwrap_err().code, "static_only");
        // A box holds the model's bounds.
        let (b, _) = fit_model_collider(&m, "box", false).unwrap();
        let ColliderShape::Box { size } = b.shape else { panic!("{:?}", b.shape) };
        let (lo, hi) = (m.bounds.min, m.bounds.max);
        assert!((size.x - (hi[0] - lo[0])).abs() < 1.0e-3 && (b.offset.y - (lo[1] + hi[1]) * 0.5).abs() < 1.0e-3, "{size:?} {:?}", b.offset);
        // Deterministic.
        assert_eq!(fit_model_collider(&m, "auto", true).unwrap().0, c);
        // Answers summarise (F1): a mesh collider in a few hundred bytes.
        let s = collider_summary(&mesh);
        assert!(s.to_string().len() < 400 && s.get("triangles").is_some() && s.get("bounds").is_some(), "{}", s.to_string());
    }
}
