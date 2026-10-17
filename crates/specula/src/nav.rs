//! Navigation and AI in the engine (docs/design/NAVIGATION.md): navmeshes built from the world,
//! nav agents walking them as a crowd, and minds (behaviour trees, utility AI) deciding where.
//!
//! - **Everything is world data.** A navmesh is an entity (`/nav/<name>`, a group) whose
//!   `props.navmesh` holds its settings; a nav agent is an entity with `props.nav_agent`; a mind is
//!   `props.mind` (an inline definition, or the path of an entity whose `props.mind_def` holds
//!   one). So all of it is saved, journaled, claim-checked and undoable like any edit. The
//!   navmesh polygons, corridors and minds' running state are derived, hidden state: hashed every
//!   tick (`world_hash` reports it as `nav`).
//! - **Geometry** comes from static colliders (exact shapes) and drawn primitives without a
//!   collider (cube, quad, sphere, lathe bounds). Dynamic bodies, characters, agents, triggers,
//!   transient, hidden and `ui` entities, 2D shapes and entities tagged `nav_ignore` are left out;
//!   models without a collider too (counted: their bounds depend on when a build finishes).
//! - **Each fixed tick, before physics:** the world's change log marks the tiles under changed
//!   geometry dirty and rebuilds at most `tiles_per_tick` of them per navmesh; minds tick (their
//!   actions set agents' goals); crowds step; agents' new positions (or, for physics characters,
//!   their walking velocity) are written back as `nav` journal entries (after-only, coalesced,
//!   exempt from claims, like physics).

use crate::physics::{self, BodyKind, ColliderShape, Physics};
use crate::world::{self, Entity, EntityKey, Shape, UpdateSpec, World};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::rng::Rng;
use specula_core::{dmath, Json};
use specula_mens::{self as mens, Arg, ArgKind, Def, Diagnostic, Mind, Phase, Status, Verb, Vocabulary};
use specula_via::{AgentParams, AgentStatus, Crowd, CrowdSettings, NavError, NavMesh, PathResult, Source, Triangle};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

/// The journal's name for navigation's own writes.
pub const AGENT: &str = "nav";
/// Props keys the navigation system reads.
pub const MESH_KEY: &str = "navmesh";
pub const AGENT_KEY: &str = "nav_agent";
pub const MIND_KEY: &str = "mind";
pub const DEF_KEY: &str = "mind_def";
/// Entities with this tag never feed a navmesh.
pub const IGNORE_TAG: &str = "nav_ignore";
/// Most navmeshes one world has.
pub const MAX_MESHES: usize = 16;
/// Most minds one world runs.
pub const MAX_MINDS: usize = 1024;
/// Per-tick hashes kept.
const HASHES: usize = 1024;
/// Columns of tile grid (with borders) rebuilt per tick at most (about 1-3 ms of work; a tile
/// at the default settings is 3,364).
pub const COLUMNS_PER_TICK: usize = 16_384;
/// Recent nav_path answers kept for the `paths` overlay.
const QUERIES: usize = 4;

fn invalid(field: &str, message: impl Into<String>) -> world::WorldError {
    world::WorldError::new("invalid_value", message).with("field", field)
}

fn nav_error(e: &NavError) -> world::WorldError {
    let mut w = world::WorldError::new(e.code, e.message.clone());
    for (k, v) in &e.details {
        w = w.with(k, v.clone());
    }
    w
}

pub(crate) fn point_of(v: &Json) -> Option<Vec3> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let c: Option<Vec<f32>> = a.iter().map(|x| x.as_f64().filter(|f| f.is_finite() && f.abs() <= f64::from(world::MAX_COORD)).map(|f| f as f32)).collect();
    let c = c?;
    Some(Vec3::new(c[0], c[1], c[2]))
}

// ---------------------------------------------------------------------------------------------
// Settings

/// Which geometry feeds a navmesh.
#[derive(Clone, Debug, PartialEq)]
pub struct Sources {
    pub colliders: bool,
    pub shapes: bool,
    /// Kinematic bodies' colliders too (moving ones rebuild their tiles as they move).
    pub kinematic: bool,
    /// Only entities at or below this path.
    pub under: Option<String>,
    pub exclude_tags: Vec<String>,
}

impl Default for Sources {
    fn default() -> Sources {
        Sources { colliders: true, shapes: true, kinematic: false, under: None, exclude_tags: Vec::new() }
    }
}

/// A navmesh's settings (`props.navmesh`).
#[derive(Clone, Debug, PartialEq)]
pub struct MeshSettings {
    pub config: specula_via::Config,
    pub sources: Sources,
    pub tiles_per_tick: usize,
}

impl MeshSettings {
    pub fn to_json(&self) -> Json {
        let c = &self.config;
        let mut src = vec![
            ("colliders", Json::from(self.sources.colliders)),
            ("shapes", Json::from(self.sources.shapes)),
            ("kinematic", Json::from(self.sources.kinematic)),
        ];
        if let Some(u) = &self.sources.under {
            src.push(("under", Json::from(u.as_str())));
        }
        if !self.sources.exclude_tags.is_empty() {
            src.push(("exclude_tags", Json::arr(self.sources.exclude_tags.iter().map(|t| Json::from(t.as_str())))));
        }
        let exact = |v: Vec3| Json::arr([Json::from(v.x), Json::from(v.y), Json::from(v.z)]);
        Json::obj([
            ("cell_size", Json::from(c.cell_size)),
            ("cell_height", Json::from(c.cell_height)),
            ("agent_radius", Json::from(c.agent_radius)),
            ("agent_height", Json::from(c.agent_height)),
            ("max_step", Json::from(c.max_step)),
            ("max_slope_deg", Json::from(c.max_slope_deg)),
            ("tile_cells", Json::from(c.tile_cells)),
            ("bounds", Json::obj([("min", exact(c.bounds_min)), ("max", exact(c.bounds_max))])),
            ("sources", Json::obj(src)),
            ("tiles_per_tick", Json::from(self.tiles_per_tick)),
        ])
    }

    /// Reads `props.navmesh` (every key optional; missing ones take defaults).
    pub fn from_json(v: &Json) -> Result<MeshSettings, world::WorldError> {
        const KEYS: [&str; 10] = ["cell_size", "cell_height", "agent_radius", "agent_height", "max_step", "max_slope_deg", "tile_cells", "bounds", "sources", "tiles_per_tick"];
        if let Json::Obj(pairs) = v {
            if let Some((k, _)) = pairs.iter().find(|(k, _)| !KEYS.contains(&k.as_str())) {
                return Err(invalid("navmesh", format!("'{k}' is not a navmesh setting ({})", KEYS.join(", "))));
            }
        } else {
            return Err(invalid("navmesh", "props.navmesh must be an object of settings"));
        }
        let d = specula_via::Config::default();
        let num = |k: &str, def: f32| -> Result<f32, world::WorldError> {
            match v.get(k) {
                None | Some(Json::Null) => Ok(def),
                Some(x) => x.as_f64().map(|f| f as f32).ok_or_else(|| invalid(k, format!("'{k}' must be a number"))),
            }
        };
        let (bmin, bmax) = match v.get("bounds") {
            None | Some(Json::Null) => (d.bounds_min, d.bounds_max),
            Some(b) => (
                b.get("min").and_then(point_of).ok_or_else(|| invalid("bounds", "'bounds' needs min and max: [x, y, z]"))?,
                b.get("max").and_then(point_of).ok_or_else(|| invalid("bounds", "'bounds' needs min and max: [x, y, z]"))?,
            ),
        };
        let tile_cells = match v.get("tile_cells") {
            None | Some(Json::Null) => d.tile_cells,
            Some(t) => t.as_u64().ok_or_else(|| invalid("tile_cells", "'tile_cells' must be an integer"))?.min(u64::from(u32::MAX)) as u32,
        };
        let config = specula_via::Config {
            cell_size: num("cell_size", d.cell_size)?,
            cell_height: num("cell_height", d.cell_height)?,
            agent_radius: num("agent_radius", d.agent_radius)?,
            agent_height: num("agent_height", d.agent_height)?,
            max_step: num("max_step", d.max_step)?,
            max_slope_deg: num("max_slope_deg", d.max_slope_deg)?,
            tile_cells,
            bounds_min: bmin,
            bounds_max: bmax,
        };
        config.validate().map_err(|e| nav_error(&e))?;
        let mut sources = Sources::default();
        if let Some(s) = v.get("sources").filter(|s| !s.is_null()) {
            const SKEYS: [&str; 5] = ["colliders", "shapes", "kinematic", "under", "exclude_tags"];
            if let Json::Obj(pairs) = s {
                if let Some((k, _)) = pairs.iter().find(|(k, _)| !SKEYS.contains(&k.as_str())) {
                    return Err(invalid("sources", format!("'{k}' is not a source setting ({})", SKEYS.join(", "))));
                }
            }
            let flag = |k: &str, d: bool| -> Result<bool, world::WorldError> {
                match s.get(k) {
                    None | Some(Json::Null) => Ok(d),
                    Some(x) => x.as_bool().ok_or_else(|| invalid("sources", format!("'sources.{k}' must be true or false"))),
                }
            };
            sources.colliders = flag("colliders", true)?;
            sources.shapes = flag("shapes", true)?;
            sources.kinematic = flag("kinematic", false)?;
            sources.under = match s.get("under") {
                None | Some(Json::Null) => None,
                Some(Json::Str(p)) if p.trim_end_matches('/').is_empty() && p.starts_with('/') => None,
                Some(Json::Str(p)) if p.starts_with('/') => Some(p.trim_end_matches('/').to_owned()),
                Some(_) => return Err(invalid("sources", "'sources.under' must be a path like \"/level\"")),
            };
            sources.exclude_tags = match s.get("exclude_tags") {
                None | Some(Json::Null) => Vec::new(),
                Some(Json::Arr(list)) if list.len() <= 16 => {
                    list.iter().map(|t| t.as_str().map(str::to_owned).ok_or_else(|| invalid("sources", "'sources.exclude_tags' must be tag names"))).collect::<Result<_, _>>()?
                }
                Some(_) => return Err(invalid("sources", "'sources.exclude_tags' must be an array of at most 16 tags")),
            };
            if !sources.colliders && !sources.shapes {
                return Err(invalid("sources", "a navmesh needs colliders, shapes or both as sources"));
            }
        }
        let tiles_per_tick = match v.get("tiles_per_tick") {
            None | Some(Json::Null) => 8,
            Some(t) => match t.as_u64() {
                Some(n) if (1..=256).contains(&n) => n as usize,
                _ => return Err(invalid("tiles_per_tick", "'tiles_per_tick' must be 1..256")),
            },
        };
        Ok(MeshSettings { config, sources, tiles_per_tick })
    }
}

/// A nav agent's settings (`props.nav_agent`).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentSpec {
    /// The navmesh: its entity's name (under `/nav`) or path; `None` = the only one.
    pub mesh: Option<String>,
    pub speed: f32,
    pub radius: Option<f32>,
    pub accel: f32,
    pub arrive: f32,
    pub avoid: bool,
    /// Where to go when no mind decides (a point or an entity).
    pub goal: Option<Json>,
}

impl AgentSpec {
    pub fn from_json(v: &Json) -> Result<AgentSpec, world::WorldError> {
        const KEYS: [&str; 7] = ["mesh", "speed", "radius", "accel", "arrive", "avoid", "goal"];
        let Json::Obj(pairs) = v else { return Err(invalid("nav_agent", "props.nav_agent must be an object")) };
        if let Some((k, _)) = pairs.iter().find(|(k, _)| !KEYS.contains(&k.as_str())) {
            return Err(invalid("nav_agent", format!("'{k}' is not a nav_agent setting ({})", KEYS.join(", "))));
        }
        let num = |k: &str, d: f32, lo: f32, hi: f32| -> Result<f32, world::WorldError> {
            match v.get(k) {
                None | Some(Json::Null) => Ok(d),
                Some(x) => match x.as_f64() {
                    Some(f) if f >= f64::from(lo) && f <= f64::from(hi) => Ok(f as f32),
                    _ => Err(invalid(k, format!("'nav_agent.{k}' must be {lo}..{hi}"))),
                },
            }
        };
        let goal = match v.get("goal") {
            None | Some(Json::Null) => None,
            Some(g) if point_of(g).is_some() || g.as_str().is_some_and(|s| !s.is_empty()) || g.as_u64().is_some() => Some(g.clone()),
            Some(_) => return Err(invalid("goal", "'nav_agent.goal' must be a point [x, y, z] or an entity (path, unique name or id)")),
        };
        Ok(AgentSpec {
            mesh: v.get("mesh").and_then(Json::as_str).map(str::to_owned),
            speed: num("speed", 2.0, 0.0, 100.0)?,
            radius: match v.get("radius") {
                None | Some(Json::Null) => None,
                Some(_) => Some(num("radius", 0.3, 0.01, 10.0)?),
            },
            accel: num("accel", 8.0, 0.1, 1000.0)?,
            arrive: num("arrive", 0.25, 0.01, 100.0)?,
            avoid: match v.get("avoid") {
                None | Some(Json::Null) => true,
                Some(b) => b.as_bool().ok_or_else(|| invalid("avoid", "'nav_agent.avoid' must be true or false"))?,
            },
            goal,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Geometry sources

/// One entity's geometry, in world space.
#[derive(Clone, Debug)]
struct SourceEntry {
    version: u64,
    /// Shown and not on the ui layer (both follow ancestors without changing the version).
    drawn: bool,
    cols: [[f32; 4]; 4],
    /// Its id and its ancestors' (`sources.under` matches by id: a rename keeps them).
    ancestors: Vec<u64>,
    tags: BTreeSet<String>,
    /// Collider geometry, and whether its body is kinematic.
    collider: Option<(Source, bool)>,
    /// Drawn-primitive geometry (no collider).
    shape: Option<Source>,
}

/// Whether `e`'s parent chain is still `ancestors` (itself first): a subtree moved to a parent
/// with the same world transform changes no version or matrix below it.
fn same_ancestors(world: &World, e: &Entity, ancestors: &[u64]) -> bool {
    let mut at = Some(e.id);
    for &a in ancestors {
        if at != Some(a) {
            return false;
        }
        at = world.get(a).and_then(|x| x.parent);
    }
    at.is_none()
}

/// `sources.under` resolved: `None` (no filter), `Some(None)` (the path doesn't exist: nothing),
/// `Some(Some(id))`.
fn under_of(world: &World, s: &Sources) -> Option<Option<u64>> {
    s.under.as_ref().map(|p| world.find_path(p).ok())
}

impl SourceEntry {
    fn pick(&self, s: &Sources, under: Option<Option<u64>>) -> Option<&Source> {
        match under {
            Some(Some(u)) if !self.ancestors.contains(&u) => return None,
            Some(None) => return None,
            _ => {}
        }
        if s.exclude_tags.iter().any(|t| self.tags.contains(t)) {
            return None;
        }
        match &self.collider {
            Some((src, kinematic)) if s.colliders && (!kinematic || s.kinematic) => Some(src),
            Some(_) => None,
            None if s.shapes => self.shape.as_ref(),
            None => None,
        }
    }
}

fn tri(m: &Mat4, a: Vec3, b: Vec3, c: Vec3) -> Triangle {
    Triangle([m.transform_point(a), m.transform_point(b), m.transform_point(c)])
}

/// A solid box: the axis-aligned fast path when the transform doesn't turn it, else triangles.
fn box_source(m: &Mat4, lo: Vec3, hi: Vec3) -> Source {
    let c = &m.cols;
    let aligned = c[0][1] == 0.0 && c[0][2] == 0.0 && c[1][0] == 0.0 && c[1][2] == 0.0 && c[2][0] == 0.0 && c[2][1] == 0.0;
    if aligned {
        let (a, b) = (m.transform_point(lo), m.transform_point(hi));
        Source::aligned_box(a.min(b), a.max(b))
    } else {
        Source::new(box_tris(m, lo, hi), true)
    }
}

fn box_tris(m: &Mat4, lo: Vec3, hi: Vec3) -> Vec<Triangle> {
    let c = |i: usize| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
    let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let mut out = Vec::with_capacity(12);
    for f in faces {
        out.push(tri(m, c(f[0]), c(f[1]), c(f[2])));
        out.push(tri(m, c(f[0]), c(f[2]), c(f[3])));
    }
    out
}

/// A closed ring prism (`sides` around Y, from `y0` to `y1`, radius `r`) around `centre`.
fn prism_tris(m: &Mat4, centre: Vec3, r: f32, y0: f32, y1: f32, sides: usize) -> Vec<Triangle> {
    let ring: Vec<(f32, f32)> = (0..sides)
        .map(|k| {
            let (s, c) = dmath::sin_cos(k as f32 * std::f32::consts::TAU / sides as f32);
            (centre.x + c * r, centre.z + s * r)
        })
        .collect();
    let mut out = Vec::new();
    for k in 0..sides {
        let (a, b) = (ring[k], ring[(k + 1) % sides]);
        let (a0, b0, a1, b1) = (Vec3::new(a.0, y0, a.1), Vec3::new(b.0, y0, b.1), Vec3::new(a.0, y1, a.1), Vec3::new(b.0, y1, b.1));
        out.push(tri(m, a0, b0, b1));
        out.push(tri(m, a0, b1, a1));
        out.push(tri(m, Vec3::new(centre.x, y1, centre.z), a1, b1));
        out.push(tri(m, Vec3::new(centre.x, y0, centre.z), b0, a0));
    }
    out
}

/// A UV sphere (8 around, 6 high) of radius `r` around `centre`.
fn sphere_tris(m: &Mat4, centre: Vec3, r: f32) -> Vec<Triangle> {
    let (lon, lat) = (8usize, 6usize);
    let at = |i: usize, j: usize| {
        let (st, ct) = dmath::sin_cos(j as f32 * std::f32::consts::PI / lat as f32);
        let (sp, cp) = dmath::sin_cos(i as f32 * std::f32::consts::TAU / lon as f32);
        Vec3::new(centre.x + r * st * cp, centre.y + r * ct, centre.z + r * st * sp)
    };
    let mut out = Vec::new();
    for j in 0..lat {
        for i in 0..lon {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            out.push(tri(m, a, b, c));
            out.push(tri(m, a, c, d));
        }
    }
    out
}

fn collider_source(m: &Mat4, c: &physics::Collider) -> Option<Source> {
    if let ColliderShape::Box { size } = &c.shape {
        return Some(box_source(m, c.offset - *size * 0.5, c.offset + *size * 0.5));
    }
    collider_tris(m, c).map(|(t, solid)| Source::new(t, solid))
}

fn collider_tris(m: &Mat4, c: &physics::Collider) -> Option<(Vec<Triangle>, bool)> {
    let o = c.offset;
    Some(match &c.shape {
        ColliderShape::Box { size } => (box_tris(m, o - *size * 0.5, o + *size * 0.5), true),
        ColliderShape::Sphere { radius } => (sphere_tris(m, o, *radius), true),
        ColliderShape::Capsule { radius, height } => (prism_tris(m, o, *radius, o.y - height * 0.5, o.y + height * 0.5, 8), true),
        ColliderShape::Hull { points } => {
            let pts: Vec<Vec3> = points.iter().map(|p| *p + o).collect();
            let hull = specula_physics::shape::Hull::new(&pts).ok()?;
            let mut out = Vec::new();
            for f in 0..hull.face_count() {
                let face = hull.face(f);
                for k in 1..face.len().saturating_sub(1) {
                    out.push(tri(m, hull.points[face[0] as usize], hull.points[face[k] as usize], hull.points[face[k + 1] as usize]));
                }
            }
            (out, true)
        }
        ColliderShape::Mesh { vertices, triangles } => {
            (triangles.iter().map(|t| tri(m, vertices[t[0] as usize] + o, vertices[t[1] as usize] + o, vertices[t[2] as usize] + o)).collect(), false)
        }
        ColliderShape::Heightfield { columns, rows, cell, heights } => {
            let (cols, rows) = (*columns as usize, *rows as usize);
            let at = |i: usize, j: usize| {
                Vec3::new(o.x + (i as f32 - (cols - 1) as f32 * 0.5) * cell[0], o.y + heights[j * cols + i], o.z + (j as f32 - (rows - 1) as f32 * 0.5) * cell[1])
            };
            let mut out = Vec::with_capacity((cols - 1) * (rows - 1) * 2);
            for j in 0..rows - 1 {
                for i in 0..cols - 1 {
                    out.push(tri(m, at(i, j), at(i, j + 1), at(i + 1, j + 1)));
                    out.push(tri(m, at(i, j), at(i + 1, j + 1), at(i + 1, j)));
                }
            }
            (out, false)
        }
    })
}

/// Whether an entity moves on its own (a nav agent, a dynamic body, a character): it and
/// everything below it never feed a navmesh.
fn mover(e: &Entity) -> bool {
    e.props.get(AGENT_KEY).is_some() || e.body.is_some_and(|b| b.kind == BodyKind::Dynamic || b.character.is_some())
}

/// Whether an entity or one of its ancestors moves on its own.
fn moves(world: &World, e: &Entity) -> bool {
    let mut at = Some(e);
    while let Some(x) = at {
        if mover(x) {
            return true;
        }
        at = x.parent.and_then(|p| world.get(p));
    }
    false
}

/// Whether an entity can feed a navmesh at all (and, if so, its geometry).
fn source_of(world: &World, e: &Entity) -> Option<SourceEntry> {
    if e.transient || !e.shown() || e.in_ui() || e.shape.is_2d() || e.tags.contains(IGNORE_TAG) {
        return None;
    }
    if e.props.get(MESH_KEY).is_some() || moves(world, e) {
        return None;
    }
    let m = e.model();
    let collider = match &e.collider {
        Some(c) if !c.is_trigger => collider_source(&m, c).map(|s| (s, e.body.is_some_and(|b| b.kind == BodyKind::Kinematic))),
        _ => None,
    };
    let shape = if e.collider.is_some() {
        None
    } else {
        match (e.shape, &e.profile) {
            (Shape::Cube, _) => Some(box_source(&m, Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5))),
            (Shape::Sphere, _) => Some(Source::new(sphere_tris(&m, Vec3::ZERO, 0.5), true)),
            (Shape::Quad, _) => {
                let (a, b, c, d) = (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.0, -0.5), Vec3::new(0.5, 0.0, 0.5), Vec3::new(-0.5, 0.0, 0.5));
                Some(Source::new(vec![tri(&m, a, c, b), tri(&m, a, d, c)], false))
            }
            (Shape::Lathe, Some(p)) => {
                let (lo, hi) = p.bounds();
                Some(box_source(&m, lo, hi))
            }
            _ => None,
        }
    };
    if collider.is_none() && shape.is_none() {
        return None;
    }
    let mut ancestors = vec![e.id];
    let mut at = e.parent;
    while let Some(p) = at {
        ancestors.push(p);
        at = world.get(p).and_then(|x| x.parent);
    }
    Some(SourceEntry { version: e.version, drawn: true, cols: m.cols, ancestors, tags: e.tags.clone(), collider, shape })
}

// ---------------------------------------------------------------------------------------------
// Meshes, agents, minds

/// One navmesh and the agents on it.
pub struct MeshSlot {
    pub id: u64,
    pub path: String,
    pub settings: MeshSettings,
    pub mesh: NavMesh,
    pub crowd: Crowd,
    pub dirty: BTreeSet<(u32, u32)>,
    /// The props it was built from (a change rebuilds everything).
    props: Json,
    /// Tiles rebuilt since the mesh was made, and the last tick's rebuilds.
    pub rebuilt_tiles: u64,
    pub last_rebuild: Vec<(u32, u32)>,
    /// Budget errors of recent tile builds (newest last, at most 16).
    pub errors: VecDeque<Json>,
    /// Models without a collider in its area at the last full build.
    pub models_skipped: usize,
    /// `sources.under` as last resolved (a rename or a new entity there rebuilds every tile).
    under: Option<Option<u64>>,
}

/// Where an agent was told to go.
#[derive(Clone, Debug, PartialEq)]
enum Goal {
    Point(Vec3),
    Entity(u64),
}

/// A nav agent's link to its navmesh and its runtime state.
#[derive(Clone, Debug)]
struct AgentLink {
    /// The `props.nav_agent` it was made from (compared, not versions: write-back bumps those).
    props: Json,
    spec: AgentSpec,
    mesh: Option<u64>,
    /// Height of the entity's origin above the navmesh surface.
    offset_y: f32,
    external: bool,
    /// The goal a mind set (minds own the goal when present).
    mind_goal: Option<Goal>,
    /// A yaw a mind asked for (`face`).
    face: Option<f32>,
    patrol_index: usize,
    wander_count: u64,
    /// Ticks a chase or move has had no complete path.
    unreachable_ticks: u32,
    /// The action node that set `mind_goal` (an abort clears only its own goal).
    goal_owner: Option<usize>,
    /// What navigation last wrote (world position), to tell its own moves from teleports.
    written: Option<Vec3>,
}

/// A mind attached to an entity.
struct MindSlot {
    /// The `props.mind` it was made from.
    props: Json,
    /// The definition entity it reads (`props.mind = {def: "/path"}`), and its version.
    def_from: Option<(u64, u64)>,
    /// The definition's path, also while nothing is there yet.
    def_path: Option<String>,
    mind: Option<Mind>,
}

/// An agent as the `paths` overlay draws it.
pub struct AgentPath {
    pub id: u64,
    pub pos: Vec3,
    /// Its path ahead: its position, then the next corners.
    pub corners: Vec<Vec3>,
    pub target: Option<Vec3>,
    pub vel: Vec3,
    pub radius: f32,
}

/// What the last tick did.
#[derive(Clone, Debug, Default)]
pub struct TickReport {
    pub tick: u64,
    pub tiles_rebuilt: usize,
    pub minds: usize,
    pub agents: usize,
    pub moving: usize,
    pub repaths: usize,
    pub waiting: usize,
    pub nodes: usize,
    pub ms: f64,
}

/// Navigation and AI state.
#[derive(Default)]
pub struct Nav {
    pub meshes: BTreeMap<u64, MeshSlot>,
    sources: BTreeMap<u64, SourceEntry>,
    /// Entities with children that move on their own (their subtrees are left out of navmeshes).
    movers: BTreeSet<u64>,
    agents: BTreeMap<u64, AgentLink>,
    minds: BTreeMap<u64, MindSlot>,
    /// Entities minds read definitions from, and paths minds wait for (may hold stale entries:
    /// they only decide when to look).
    def_ids: BTreeSet<u64>,
    def_paths: BTreeSet<String>,
    /// Minds that can't run, and why (kept apart from agents' problems).
    pub mind_problems: BTreeMap<u64, Json>,
    /// `(world generation, draw epoch)` synced up to.
    synced: Option<(u64, u64)>,
    /// Entities navigation can't use, and why.
    pub problems: BTreeMap<u64, Json>,
    pub hashes: VecDeque<(u64, u64)>,
    pub last: Option<TickReport>,
    /// Recent nav_path answers (mesh, path) for the `paths` overlay.
    pub queries: VecDeque<(u64, PathResult)>,
    /// Wall-clock milliseconds of recent ticks.
    pub tick_ms: VecDeque<f64>,
    vocab: Option<Vocabulary>,
}

/// The engine's AI vocabulary: what minds can sense and do here.
pub fn vocabulary() -> Vocabulary {
    let target = |req: bool| Arg { name: "target", kind: ArgKind::Target, required: req };
    let num = |name: &'static str, min: f64, max: f64| Arg { name, kind: ArgKind::Number { min, max }, required: false };
    Vocabulary {
        conditions: vec![
            Verb {
                name: "sees",
                doc: "the target is within range and the field of view (around the agent's front, local +Z) and no collider blocks the line of sight (a physics raycast from the agent's eye)",
                args: vec![target(true), num("range", 0.0, 1.0e4), num("fov_deg", 1.0, 360.0), num("eye", -10.0, 10.0)],
            },
            Verb { name: "near", doc: "the target is within distance (metres, centre to centre in X/Z)", args: vec![target(true), num("distance", 0.0, 1.0e4)] },
            Verb { name: "has_goal", doc: "the agent has somewhere to go", args: vec![] },
            Verb { name: "arrived", doc: "the agent reached its goal", args: vec![] },
            Verb { name: "on_navmesh", doc: "the agent stands on its navmesh", args: vec![] },
        ],
        actions: vec![
            Verb {
                name: "move_to",
                doc: "walk to the target (an entity's position when the action starts, or a point); success on arrival, failure if unreachable",
                args: vec![target(true), num("speed", 0.0, 100.0)],
            },
            Verb {
                name: "patrol",
                doc: "walk the points in order (or an entity's children, by name), looping unless loop is false; running while patrolling",
                args: vec![Arg { name: "points", kind: ArgKind::Points, required: true }, Arg { name: "loop", kind: ArgKind::Bool, required: false }, num("speed", 0.0, 100.0)],
            },
            Verb {
                name: "chase",
                doc: "follow a moving target, repathing as it moves; success within 'within' metres, failure past 'give_up' metres or when it can't be reached for 2 s",
                args: vec![target(true), num("within", 0.0, 1.0e4), num("give_up", 0.0, 1.0e4), num("speed", 0.0, 100.0)],
            },
            Verb { name: "flee", doc: "walk away from the target until 'distance' metres from it", args: vec![target(true), num("distance", 0.1, 1.0e4), num("speed", 0.0, 100.0)] },
            Verb { name: "wander", doc: "walk to a seeded random navmesh point within 'radius'; success on arrival", args: vec![num("radius", 0.5, 1.0e4), num("speed", 0.0, 100.0)] },
            Verb { name: "stop", doc: "stand still (clears the goal)", args: vec![] },
            Verb { name: "face", doc: "turn the agent's front toward the target", args: vec![target(true)] },
        ],
        inputs: vec![
            Verb { name: "distance", doc: "metres to the target in X/Z (1e6 when it doesn't exist)", args: vec![target(true)] },
            Verb { name: "bb", doc: "a blackboard number (0 when unset)", args: vec![Arg { name: "key", kind: ArgKind::Str, required: true }] },
            Verb {
                name: "prop",
                doc: "a number from the agent's props (or the target's, with 'of')",
                args: vec![Arg { name: "key", kind: ArgKind::Str, required: true }, Arg { name: "of", kind: ArgKind::Target, required: false }],
            },
        ],
    }
}

/// Resolves a target argument: `"$key"` (a blackboard value), a path, a unique name, an id or a
/// point. Entities resolve to their world position.
fn resolve_target(world: &World, v: &Json, bb: &mens::run::Blackboard) -> Option<(Vec3, Option<u64>)> {
    let v = match v {
        Json::Str(s) if s.starts_with('$') => bb.get(&s[1..])?,
        _ => v,
    };
    if let Some(p) = point_of(v) {
        return Some((p, None));
    }
    let key = match v {
        Json::Str(s) if s.starts_with('/') => EntityKey::Path(s.clone()),
        Json::Str(s) => EntityKey::Name(s.clone()),
        _ => EntityKey::Id(v.as_u64()?),
    };
    let id = world.resolve(&key).ok()?;
    world.get(id).map(|e| (e.world_position(), Some(id)))
}

fn horizontal(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    (dx * dx + dz * dz).sqrt()
}

/// The engine as a mind's host: senses read the world and physics, deeds set the agent's goal.
struct EngineHost<'a> {
    world: &'a World,
    physics: &'a Physics,
    meshes: &'a BTreeMap<u64, MeshSlot>,
    link: Option<&'a mut AgentLink>,
    id: u64,
    /// The action node being run.
    node: usize,
}

impl EngineHost<'_> {
    fn me(&self) -> Option<&Entity> {
        self.world.get(self.id)
    }

    fn crowd_agent(&self) -> Option<&specula_via::CrowdAgent> {
        let link = self.link.as_ref()?;
        self.meshes.get(&link.mesh?)?.crowd.agent(self.id)
    }

    fn mesh(&self) -> Option<&MeshSlot> {
        self.meshes.get(&self.link.as_ref()?.mesh?)
    }

    fn front(&self) -> (f32, f32) {
        let Some(e) = self.me() else { return (0.0, 1.0) };
        let m = e.model();
        let (x, z) = (m.cols[2][0], m.cols[2][2]);
        let l = (x * x + z * z).sqrt();
        if l > 1.0e-6 {
            (x / l, z / l)
        } else {
            (0.0, 1.0)
        }
    }

    fn set_goal(&mut self, g: Option<Goal>) -> bool {
        match self.link.as_deref_mut() {
            Some(l) => {
                if l.mind_goal != g {
                    l.unreachable_ticks = 0;
                }
                l.goal_owner = g.as_ref().map(|_| self.node);
                l.mind_goal = g;
                true
            }
            None => false,
        }
    }

    /// Ticks without a complete path (counts up while the crowd says partial).
    fn unreachable(&mut self) -> u32 {
        let partial = self.crowd_agent().is_some_and(|a| matches!(a.status, AgentStatus::Partial | AgentStatus::OffMesh));
        match self.link.as_deref_mut() {
            Some(l) => {
                l.unreachable_ticks = if partial { l.unreachable_ticks + 1 } else { 0 };
                l.unreachable_ticks
            }
            None => 0,
        }
    }

    fn arrived(&self) -> bool {
        self.crowd_agent().is_some_and(|a| a.status == AgentStatus::Arrived)
    }

    fn points(&self, v: &Json) -> Vec<Vec3> {
        match v {
            Json::Str(p) => match self.world.find_path(p) {
                Ok(id) => self.world.children_of(Some(id)).filter_map(|c| self.world.get(c)).map(Entity::world_position).take(64).collect(),
                Err(_) => Vec::new(),
            },
            Json::Arr(list) => list.iter().filter_map(point_of).take(64).collect(),
            _ => Vec::new(),
        }
    }

    fn speed(&mut self, args: &Json) {
        if let (Some(s), Some(l)) = (args.get("speed").and_then(Json::as_f64), self.link.as_deref_mut()) {
            l.spec.speed = s as f32;
        }
    }
}

impl mens::Host for EngineHost<'_> {
    fn condition(&mut self, name: &str, args: &Json, bb: &mens::run::Blackboard) -> bool {
        let Some(me) = self.me().map(Entity::world_position) else { return false };
        match name {
            "sees" => {
                let Some((at, tid)) = args.get("target").and_then(|t| resolve_target(self.world, t, bb)) else { return false };
                let range = args.get("range").and_then(Json::as_f64).unwrap_or(10.0) as f32;
                let fov = args.get("fov_deg").and_then(Json::as_f64).unwrap_or(120.0) as f32;
                let eye = args.get("eye").and_then(Json::as_f64).unwrap_or(0.5) as f32;
                let d = horizontal(me, at);
                if d > range {
                    return false;
                }
                if fov < 360.0 && d > 1.0e-4 {
                    let f = self.front();
                    let (dx, dz) = ((at.x - me.x) / d, (at.z - me.z) / d);
                    let cos = f.0 * dx + f.1 * dz;
                    if cos < dmath::cos((fov * 0.5).to_radians()) {
                        return false;
                    }
                }
                // Line of sight: the first collider on the way must be the target's (or none).
                let from = me + Vec3::new(0.0, eye, 0.0);
                let to = at + Vec3::new(0.0, eye.min(0.5), 0.0);
                let dir = to - from;
                let len = dir.length();
                if len < 1.0e-4 {
                    return true;
                }
                let filter = specula_physics::Filter { mask: u64::MAX, triggers: false, exclude: Some(self.id) };
                let (hits, _) = self.physics.space.raycast(from, dir, len, &filter, 4);
                match hits.iter().find(|h| Some(h.key) != Some(self.id)) {
                    None => true,
                    Some(h) => Some(h.key) == tid || tid.is_some_and(|t| is_ancestor(self.world, t, h.key)),
                }
            }
            "near" => {
                let Some((at, _)) = args.get("target").and_then(|t| resolve_target(self.world, t, bb)) else { return false };
                horizontal(me, at) <= args.get("distance").and_then(Json::as_f64).unwrap_or(2.0) as f32
            }
            "has_goal" => self.link.as_ref().is_some_and(|l| l.mind_goal.is_some()),
            "arrived" => self.arrived(),
            "on_navmesh" => self.crowd_agent().is_some_and(|a| a.poly.is_some()),
            _ => false,
        }
    }

    fn action(&mut self, node: usize, name: &str, args: &Json, phase: Phase, bb: &mut mens::run::Blackboard) -> Status {
        self.node = node;
        if phase == Phase::Abort {
            // Only the goal this node set: the action that replaced it may have set its own.
            if self.link.as_ref().is_some_and(|l| l.goal_owner == Some(node)) {
                self.set_goal(None);
            }
            return Status::Failure;
        }
        if self.link.is_none() && name != "face" && name != "stop" {
            // Moving needs a nav agent.
            return Status::Failure;
        }
        let me = match self.me() {
            Some(e) => e.world_position(),
            None => return Status::Failure,
        };
        match name {
            "move_to" => {
                if phase == Phase::Start {
                    let Some((at, _)) = args.get("target").and_then(|t| resolve_target(self.world, t, bb)) else { return Status::Failure };
                    self.speed(args);
                    self.set_goal(Some(Goal::Point(at)));
                    return Status::Running;
                }
                if self.arrived() {
                    self.set_goal(None);
                    return Status::Success;
                }
                if self.unreachable() > 60 {
                    self.set_goal(None);
                    return Status::Failure;
                }
                Status::Running
            }
            "patrol" => {
                let points = args.get("points").map(|p| self.points(p)).unwrap_or_default();
                if points.is_empty() {
                    return Status::Failure;
                }
                let looping = args.get("loop").and_then(Json::as_bool).unwrap_or(true);
                if phase == Phase::Start {
                    self.speed(args);
                }
                let link = self.link.as_deref_mut().expect("checked");
                let mut index = link.patrol_index % points.len();
                let arrive = link.spec.arrive + 0.1;
                if phase != Phase::Start && horizontal(me, points[index]) <= arrive + 0.15 && self.arrived() {
                    index += 1;
                    if index >= points.len() && !looping {
                        let link = self.link.as_deref_mut().expect("checked");
                        link.patrol_index = 0;
                        self.set_goal(None);
                        return Status::Success;
                    }
                    index %= points.len();
                }
                self.link.as_deref_mut().expect("checked").patrol_index = index;
                self.set_goal(Some(Goal::Point(points[index])));
                Status::Running
            }
            "chase" => {
                let Some((at, tid)) = args.get("target").and_then(|t| resolve_target(self.world, t, bb)) else {
                    self.set_goal(None);
                    return Status::Failure;
                };
                if phase == Phase::Start {
                    self.speed(args);
                }
                let within = args.get("within").and_then(Json::as_f64).unwrap_or(1.0) as f32;
                let give_up = args.get("give_up").and_then(Json::as_f64).unwrap_or(30.0) as f32;
                let d = horizontal(me, at);
                if d <= within {
                    self.set_goal(None);
                    return Status::Success;
                }
                if d > give_up || (phase != Phase::Start && self.unreachable() > 60) {
                    self.set_goal(None);
                    return Status::Failure;
                }
                self.set_goal(Some(tid.map_or(Goal::Point(at), Goal::Entity)));
                Status::Running
            }
            "flee" => {
                let Some((at, _)) = args.get("target").and_then(|t| resolve_target(self.world, t, bb)) else { return Status::Failure };
                let want = args.get("distance").and_then(Json::as_f64).unwrap_or(8.0) as f32;
                let d = horizontal(me, at);
                if d >= want {
                    self.set_goal(None);
                    return Status::Success;
                }
                if phase == Phase::Start {
                    self.speed(args);
                }
                let (dx, dz) = if d > 1.0e-4 { ((me.x - at.x) / d, (me.z - at.z) / d) } else { (1.0, 0.0) };
                let away = Vec3::new(me.x + dx * want, me.y, me.z + dz * want);
                let Some(slot) = self.mesh() else { return Status::Failure };
                let target = slot.mesh.nearest(away, Vec3::new(want, slot.settings.config.agent_height * 2.0, want)).map_or(away, |(_, p)| p);
                self.set_goal(Some(Goal::Point(target)));
                Status::Running
            }
            "wander" => {
                if phase == Phase::Start {
                    self.speed(args);
                    let radius = args.get("radius").and_then(Json::as_f64).unwrap_or(5.0) as f32;
                    let id = self.id;
                    let Some(slot) = self.mesh() else { return Status::Failure };
                    let count = self.link.as_ref().map_or(0, |l| l.wander_count);
                    let mut rng = Rng::new(id, count);
                    let Some((_, p)) = slot.mesh.random_point_near(&mut rng, me, radius) else { return Status::Failure };
                    if let Some(l) = self.link.as_deref_mut() {
                        l.wander_count += 1;
                    }
                    self.set_goal(Some(Goal::Point(p)));
                    return Status::Running;
                }
                if self.arrived() {
                    self.set_goal(None);
                    return Status::Success;
                }
                if self.unreachable() > 60 {
                    self.set_goal(None);
                    return Status::Failure;
                }
                Status::Running
            }
            "stop" => {
                self.set_goal(None);
                Status::Success
            }
            "face" => {
                let Some((at, _)) = args.get("target").and_then(|t| resolve_target(self.world, t, bb)) else { return Status::Failure };
                let yaw = dmath::atan2(at.x - me.x, at.z - me.z).to_degrees();
                if let Some(l) = self.link.as_deref_mut() {
                    l.face = Some(yaw);
                }
                Status::Success
            }
            _ => Status::Failure,
        }
    }

    fn input(&mut self, name: &str, args: &Json, bb: &mens::run::Blackboard) -> f64 {
        match name {
            "distance" => {
                let me = self.me().map(Entity::world_position);
                match (me, args.get("target").and_then(|t| resolve_target(self.world, t, bb))) {
                    (Some(m), Some((at, _))) => f64::from(horizontal(m, at)),
                    _ => 1.0e6,
                }
            }
            "bb" => args.get("key").and_then(Json::as_str).and_then(|k| bb.get(k)).and_then(Json::as_f64).unwrap_or(0.0),
            "prop" => {
                let key = args.get("key").and_then(Json::as_str).unwrap_or_default();
                let of = match args.get("of") {
                    Some(t) => resolve_target(self.world, t, bb).and_then(|(_, id)| id),
                    None => Some(self.id),
                };
                of.and_then(|id| self.world.get(id)).and_then(|e| e.props.get(key)).and_then(Json::as_f64).unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }
}

/// Whether `ancestor` is `id` or one of its parents.
fn is_ancestor(world: &World, ancestor: u64, id: u64) -> bool {
    let mut at = Some(id);
    while let Some(i) = at {
        if i == ancestor {
            return true;
        }
        at = world.get(i).and_then(|e| e.parent);
    }
    false
}

impl Nav {
    /// Forgets everything (a new world).
    pub fn reset(&mut self) {
        let vocab = self.vocab.take();
        *self = Nav { vocab, ..Nav::default() };
    }

    pub fn vocab(&mut self) -> &Vocabulary {
        self.vocab.get_or_insert_with(vocabulary)
    }

    /// Whether anything navigates (meshes, agents or minds exist).
    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty() && self.agents.is_empty() && self.minds.is_empty()
    }

    /// The mesh slot for an agent's `mesh` setting.
    fn mesh_for(&self, name: Option<&str>) -> Option<u64> {
        match name {
            None => {
                if self.meshes.len() == 1 {
                    self.meshes.keys().next().copied()
                } else {
                    self.meshes.iter().find(|(_, m)| m.path == "/nav/default").map(|(k, _)| *k)
                }
            }
            Some(n) => self.meshes.iter().find(|(_, m)| m.path == n || m.path.rsplit('/').next() == Some(n)).map(|(k, _)| *k),
        }
    }

    /// The mesh a tool names (`None`: the only one, or `/nav/default`).
    pub fn find_mesh(&self, name: Option<&str>) -> Result<u64, world::WorldError> {
        self.mesh_for(name).ok_or_else(|| {
            let known: Vec<&str> = self.meshes.values().map(|m| m.path.as_str()).collect();
            let msg = match (name, known.is_empty()) {
                (_, true) => "no navmesh yet: build one with nav_build".to_string(),
                (Some(n), _) => format!("no navmesh '{n}' (navmeshes: {})", known.join(", ")),
                (None, _) => format!("several navmeshes: name one with 'mesh' ({})", known.join(", ")),
            };
            world::WorldError::new("no_navmesh", msg)
        })
    }

    /// Syncs with what changed in the world since the last sync (or everything).
    pub fn sync(&mut self, world: &World, generation: u64) {
        let epoch = world.draw_epoch();
        let changed: Option<Vec<u64>> = match self.synced {
            Some((g, e)) if g == generation => {
                if e == epoch {
                    Some(Vec::new())
                } else {
                    world.draw_changes_since(e).map(|it| it.collect())
                }
            }
            Some(_) => {
                self.reset();
                None
            }
            None => None,
        };
        match changed {
            Some(mut ids) => {
                ids.sort_unstable();
                ids.dedup();
                for id in ids {
                    self.upsert(world, id);
                }
            }
            None => {
                let mut live = BTreeSet::new();
                for e in world.persistent_entities() {
                    live.insert(e.id);
                    self.upsert(world, e.id);
                }
                let gone: Vec<u64> = self
                    .sources
                    .keys()
                    .chain(self.agents.keys())
                    .chain(self.minds.keys())
                    .chain(self.meshes.keys())
                    .filter(|k| !live.contains(k))
                    .copied()
                    .collect();
                for id in gone {
                    self.upsert(world, id);
                }
            }
        }
        self.synced = Some((generation, epoch));
    }

    /// Brings one entity up to date (gone entities are forgotten).
    fn upsert(&mut self, world: &World, id: u64) {
        let e = world.get(id).filter(|e| !e.transient);
        // Navmesh entities.
        match e.and_then(|e| e.props.get(MESH_KEY).map(|p| (e, p))) {
            Some((e, props)) => {
                if self.meshes.get(&id).is_none_or(|m| m.props != *props) {
                    match MeshSettings::from_json(props) {
                        Ok(settings) => {
                            if self.meshes.len() >= MAX_MESHES && !self.meshes.contains_key(&id) {
                                self.problems.insert(id, Json::from(format!("a world has at most {MAX_MESHES} navmeshes")));
                            } else {
                                self.make_mesh(world, e, settings, props.clone());
                                self.problems.remove(&id);
                            }
                        }
                        Err(err) => {
                            self.meshes.remove(&id);
                            self.problems.insert(id, err.to_json());
                        }
                    }
                } else {
                    let path = world.path_of(id);
                    if let Some(m) = self.meshes.get_mut(&id).filter(|m| m.path != path) {
                        m.path = path;
                        // Agents name navmeshes by name or path.
                        self.relink_agents(world);
                    }
                }
            }
            None => {
                if self.meshes.remove(&id).is_some() {
                    self.relink_agents(world);
                }
            }
        }
        // A parent that starts or stops moving on its own takes its subtree in or out.
        let is_mover = e.is_some_and(|e| mover(e) && world.has_children(e.id));
        if is_mover != self.movers.contains(&id) {
            if is_mover {
                self.movers.insert(id);
            } else {
                self.movers.remove(&id);
            }
            let mut below = Vec::new();
            world.walk(Some(id), |d, _| {
                if d.id != id {
                    below.push(d.id);
                }
            });
            for d in below {
                // Forget what was known, so the next look recomputes it.
                if let Some(mut s) = self.sources.remove(&d) {
                    s.version = u64::MAX;
                    self.sources.insert(d, s);
                }
                self.upsert(world, d);
            }
        }
        // Geometry.
        let new = e.and_then(|e| {
            let drawn = e.shown() && !e.in_ui();
            let unchanged = self.sources.get(&id).is_some_and(|s| {
                s.version == e.version
                    && s.drawn == drawn
                    && s.cols.iter().flatten().zip(e.model().cols.iter().flatten()).all(|(a, b)| a.to_bits() == b.to_bits())
                    && same_ancestors(world, e, &s.ancestors)
            });
            if unchanged {
                None
            } else {
                Some(source_of(world, e))
            }
        });
        if let Some(new) = new {
            let old = self.sources.remove(&id);
            for m in self.meshes.values_mut() {
                let under = under_of(world, &m.settings.sources);
                for s in [old.as_ref(), new.as_ref()].into_iter().flatten() {
                    if let Some(src) = s.pick(&m.settings.sources, under) {
                        m.dirty.extend(m.mesh.tiles_touching(src.lo, src.hi));
                    }
                }
            }
            if let Some(n) = new {
                self.sources.insert(id, n);
            }
        } else if e.is_none() {
            if let Some(old) = self.sources.remove(&id) {
                for m in self.meshes.values_mut() {
                    // (A deleted `under` root: its whole old area is dirty anyway.)
                    let under = under_of(world, &m.settings.sources).map(|u| u.or(old.ancestors.last().copied()));
                    if let Some(src) = old.pick(&m.settings.sources, under) {
                        m.dirty.extend(m.mesh.tiles_touching(src.lo, src.hi));
                    }
                }
            }
        }
        // Agents.
        match e.and_then(|e| e.props.get(AGENT_KEY).map(|p| (e, p))) {
            Some((e, props)) => {
                let external = e.body.is_some_and(|b| b.character.is_some());
                if self.agents.get(&id).is_none_or(|a| a.props != *props || a.external != external) {
                    match AgentSpec::from_json(props) {
                        Ok(spec) => self.link_agent(world, e, spec, props.clone()),
                        Err(err) => {
                            self.drop_agent(id);
                            self.problems.insert(id, err.to_json());
                        }
                    }
                }
            }
            None => self.drop_agent(id),
        }
        // Minds (and the minds that read this entity's definition).
        match e.and_then(|e| e.props.get(MIND_KEY).map(|p| (e, p))) {
            Some((e, props)) => {
                if self.minds.get(&id).is_none_or(|m| m.props != *props) {
                    self.load_mind(world, e.id, props.clone());
                }
            }
            None => {
                if self.minds.remove(&id).is_some() {
                    self.forget_mind_goals(id);
                    self.reindex_definitions();
                }
                self.mind_problems.remove(&id);
            }
        }
        // Minds reading this entity's definition, or waiting for one at its path.
        let at_path = (!self.def_paths.is_empty() && e.is_some_and(|e| e.props.get(DEF_KEY).is_some())).then(|| world.path_of(id)).filter(|p| self.def_paths.contains(p));
        if self.def_ids.contains(&id) || at_path.is_some() {
            let version = e.map(|e| e.version);
            let readers: Vec<u64> = self
                .minds
                .iter()
                .filter(|(_, m)| match m.def_from {
                    Some((d, v)) => d == id && version != Some(v),
                    None => at_path.is_some() && m.def_path == at_path,
                })
                .map(|(k, _)| *k)
                .collect();
            for r in readers {
                if let Some(props) = world.get(r).and_then(|e| e.props.get(MIND_KEY)).cloned() {
                    self.load_mind(world, r, props);
                }
            }
        }
        if e.is_none() {
            self.problems.remove(&id);
        }
    }

    fn make_mesh(&mut self, world: &World, e: &Entity, settings: MeshSettings, props: Json) {
        let settings_sources = settings.sources.clone();
        let mesh = match NavMesh::new(settings.config) {
            Ok(m) => m,
            Err(err) => {
                self.problems.insert(e.id, err.to_json());
                return;
            }
        };
        let (nx, nz) = mesh.tile_count();
        let mut dirty = BTreeSet::new();
        for tz in 0..nz {
            for tx in 0..nx {
                dirty.insert((tx, tz));
            }
        }
        self.meshes.insert(
            e.id,
            MeshSlot {
                id: e.id,
                path: world.path_of(e.id),
                settings,
                mesh,
                crowd: Crowd::new(CrowdSettings::default()),
                dirty,
                props,
                rebuilt_tiles: 0,
                last_rebuild: Vec::new(),
                errors: VecDeque::new(),
                models_skipped: 0,
                under: under_of(world, &settings_sources),
            },
        );
        self.relink_agents(world);
    }

    /// Re-attaches every agent (a navmesh appeared, changed or went).
    fn relink_agents(&mut self, world: &World) {
        let ids: Vec<u64> = self.agents.keys().copied().collect();
        for id in ids {
            if let (Some(e), Some((spec, props))) = (world.get(id), self.agents.get(&id).map(|a| (a.spec.clone(), a.props.clone()))) {
                self.agents.remove(&id);
                self.link_agent(world, e, spec, props);
            }
        }
    }

    fn drop_agent(&mut self, id: u64) {
        if let Some(a) = self.agents.remove(&id) {
            if let Some(m) = a.mesh.and_then(|m| self.meshes.get_mut(&m)) {
                m.crowd.remove(id);
            }
        }
    }

    fn link_agent(&mut self, world: &World, e: &Entity, spec: AgentSpec, props: Json) {
        let id = e.id;
        let old = self.agents.remove(&id);
        let mesh = self.mesh_for(spec.mesh.as_deref());
        if let Some(prev) = old.as_ref().and_then(|o| o.mesh).filter(|m| Some(*m) != mesh) {
            if let Some(m) = self.meshes.get_mut(&prev) {
                m.crowd.remove(id);
            }
        }
        let external = e.body.is_some_and(|b| b.character.is_some());
        let (_, scale) = physics::world_pose(world, e);
        // The origin's height above the feet: a character's collider bottom, else its drawn bottom.
        let collider_bottom = e.collider.as_ref().and_then(|c| match c.shape {
            ColliderShape::Capsule { height, .. } => Some(c.offset.y - height * 0.5),
            ColliderShape::Sphere { radius } => Some(c.offset.y - radius),
            ColliderShape::Box { size } => Some(c.offset.y - size.y * 0.5),
            _ => None,
        });
        let offset_y = match (external, collider_bottom) {
            (true, Some(lo)) => -lo * scale.y,
            (true, None) => 0.0,
            // A model's built bounds arrive when its build finishes (wall-clock time, F2): models
            // use their collider's bottom, or their origin.
            (false, lo) if e.shape == Shape::Model => lo.map_or(0.0, |lo| -lo * scale.y),
            (false, _) => e.local_bounds().map_or(0.0, |(lo, _)| -lo.y * scale.y),
        };
        let mut problem = None;
        if let Some(mid) = mesh {
            let slot = self.meshes.get_mut(&mid).expect("found");
            let radius = spec.radius.unwrap_or(slot.settings.config.agent_radius);
            let params = AgentParams { radius, speed: spec.speed, accel: spec.accel, arrive: spec.arrive, avoid: spec.avoid, external };
            let pos = e.world_position() - Vec3::new(0.0, offset_y, 0.0);
            if let Err(err) = slot.crowd.upsert(id, params, pos) {
                problem = Some(err.to_json());
            }
        } else {
            problem = Some(Json::from(match &spec.mesh {
                Some(m) => format!("no navmesh '{m}' (build one with nav_build)"),
                None => "no navmesh to walk on (build one with nav_build, or name one with nav_agent.mesh)".to_string(),
            }));
        }
        match problem {
            Some(p) => {
                self.problems.insert(id, p);
            }
            None => {
                self.problems.remove(&id);
            }
        }
        let keep = old.as_ref();
        self.agents.insert(
            id,
            AgentLink {
                props,
                spec,
                mesh,
                offset_y,
                external,
                mind_goal: keep.and_then(|o| o.mind_goal.clone()),
                face: keep.and_then(|o| o.face),
                patrol_index: keep.map_or(0, |o| o.patrol_index),
                wander_count: keep.map_or(0, |o| o.wander_count),
                unreachable_ticks: 0,
                goal_owner: keep.and_then(|o| o.goal_owner),
                written: keep.and_then(|o| o.written),
            },
        );
    }

    /// What a mind left on its agent (goal, owner, facing, patrol index) goes with it.
    fn forget_mind_goals(&mut self, id: u64) {
        if let Some(l) = self.agents.get_mut(&id) {
            l.mind_goal = None;
            l.goal_owner = None;
            l.face = None;
            l.patrol_index = 0;
            l.unreachable_ticks = 0;
        }
    }

    /// The definition indexes from the live minds (they only grow between rebuilds).
    fn reindex_definitions(&mut self) {
        self.def_ids = self.minds.values().filter_map(|m| m.def_from.map(|(d, _)| d)).collect();
        self.def_paths = self.minds.values().filter_map(|m| m.def_path.clone()).collect();
    }

    /// Reads `props.mind` into a running mind (a problem when it doesn't validate).
    fn load_mind(&mut self, world: &World, id: u64, props: Json) {
        if self.def_paths.len() + self.def_ids.len() > 2 * self.minds.len() + 32 {
            self.reindex_definitions();
        }
        let version = props.clone();
        let def_json = props.get("def").cloned().unwrap_or(Json::Null);
        let def_path = def_json.as_str().map(str::to_owned);
        if let Some(p) = &def_path {
            self.def_paths.insert(p.clone());
        }
        let (def, from) = match &def_json {
            Json::Str(p) => match world.find_path(p) {
                Ok(d) => {
                    self.def_ids.insert(d);
                    let v = world.get(d).map_or(0, |e| e.version);
                    match world.get(d).and_then(|e| e.props.get(DEF_KEY)) {
                        Some(dj) => (dj.clone(), Some((d, v))),
                        None => {
                            self.minds.insert(id, MindSlot { props: version, def_from: Some((d, v)), def_path, mind: None });
                            self.mind_problems.insert(id, Json::from(format!("'{p}' has no props.{DEF_KEY}")));
                            return;
                        }
                    }
                }
                Err(_) => {
                    self.minds.insert(id, MindSlot { props: version, def_from: None, def_path, mind: None });
                    self.mind_problems.insert(id, Json::from(format!("mind definition '{p}' doesn't exist (it loads when an entity with props.{DEF_KEY} appears there)")));
                    return;
                }
            },
            Json::Obj(_) => (def_json.clone(), None),
            _ => {
                self.minds.insert(id, MindSlot { props: version, def_from: None, def_path: None, mind: None });
                self.mind_problems.insert(id, Json::from("props.mind needs 'def': a definition object or the path of an entity with props.mind_def"));
                return;
            }
        };
        if self.minds.len() >= MAX_MINDS && !self.minds.contains_key(&id) {
            self.mind_problems.insert(id, Json::from(format!("a world runs at most {MAX_MINDS} minds")));
            return;
        }
        let parsed = Def::parse(&def, self.vocab());
        match parsed {
            Ok(d) => {
                let bb: mens::run::Blackboard = match props.get("blackboard") {
                    Some(Json::Obj(pairs)) => pairs.iter().take(mens::run::MAX_KEYS).cloned().collect(),
                    _ => BTreeMap::new(),
                };
                // A changed definition keeps nothing; a changed blackboard alone keeps the run.
                let keep = self.minds.get(&id).and_then(|m| m.mind.as_ref()).filter(|m| *m.def == d).cloned();
                let mind = match keep {
                    Some(mut m) => {
                        for (k, v) in bb {
                            m.bb.insert(k, v);
                        }
                        m
                    }
                    None => {
                        self.forget_mind_goals(id);
                        Mind::new(Arc::new(d), bb, 30)
                    }
                };
                self.minds.insert(id, MindSlot { props: version, def_from: from, def_path, mind: Some(mind) });
                self.mind_problems.remove(&id);
            }
            Err(diags) => {
                self.forget_mind_goals(id);
                self.minds.insert(id, MindSlot { props: version, def_from: from, def_path, mind: None });
                self.mind_problems.insert(id, Json::obj([("invalid_mind", Json::arr(diags.iter().map(Diagnostic::to_json)))]));
            }
        }
    }

    /// Rebuilds up to `budget` dirty tiles of a mesh (all with `None`), in tile order.
    pub fn rebuild(&mut self, mesh_id: u64, budget: Option<usize>, world: &World) -> usize {
        let Some(slot) = self.meshes.get_mut(&mesh_id) else { return 0 };
        // Per tick: at most `budget` tiles and about COLUMNS_PER_TICK columns of work (a tile's
        // grid with its border), always at least one tile.
        let d = slot.mesh.derived();
        let side = (slot.settings.config.tile_cells + 2 * d.border) as usize;
        let by_work = (COLUMNS_PER_TICK / (side * side)).max(1);
        let n = budget.map_or(usize::MAX, |b| b.min(by_work)).min(slot.dirty.len());
        let take: Vec<(u32, u32)> = slot.dirty.iter().take(n).copied().collect();
        if take.is_empty() && budget.is_some() {
            slot.last_rebuild.clear();
            return 0;
        }
        let under = under_of(world, &slot.settings.sources);
        let picked: Vec<(&SourceEntry, &Source)> = self.sources.values().filter_map(|s| s.pick(&slot.settings.sources, under).map(|src| (s, src))).collect();
        for &(tx, tz) in &take {
            slot.dirty.remove(&(tx, tz));
            let (lo, hi) = slot.mesh.tile_input_bounds(tx, tz);
            let mine: Vec<&Source> = picked.iter().filter(|(_, s)| s.touches(lo, hi)).map(|(_, s)| *s).collect();
            let stats = slot.mesh.build_tile(tx, tz, &mine);
            for e in &stats.errors {
                if slot.errors.len() == 16 {
                    slot.errors.pop_front();
                }
                slot.errors.push_back(e.to_json());
            }
        }
        if budget.is_none() {
            // A full build: count models that would have fed it with a collider.
            let (lo, hi) = (slot.settings.config.bounds_min, slot.settings.config.bounds_max);
            slot.models_skipped = world
                .persistent_entities()
                .filter(|e| e.shape == Shape::Model && e.collider.is_none() && e.shown() && !e.tags.contains(IGNORE_TAG))
                .filter(|e| {
                    let (a, b) = e.world_aabb();
                    a.x <= hi.x && b.x >= lo.x && a.z <= hi.z && b.z >= lo.z && a.y <= hi.y && b.y >= lo.y
                })
                .count();
        }
        slot.rebuilt_tiles += take.len() as u64;
        slot.last_rebuild = take.clone();
        take.len()
    }

    /// One fixed tick: sync, rebuild dirty tiles, minds, crowds, write-back. The caller records
    /// changes as a snapshot and journals them.
    pub fn tick(&mut self, world: &mut World, physics: &Physics, generation: u64, tick: u64, dt: f32, hz: u32) {
        let started = std::time::Instant::now();
        self.sync(world, generation);
        if self.is_empty() {
            return;
        }
        let mut report = TickReport { tick, ..TickReport::default() };
        // A navmesh limited to a subtree that now resolves to another entity (or none) starts over.
        for slot in self.meshes.values_mut() {
            if slot.settings.sources.under.is_some() {
                let u = under_of(world, &slot.settings.sources);
                if u != slot.under {
                    slot.under = u;
                    let (nx, nz) = slot.mesh.tile_count();
                    for tz in 0..nz {
                        for tx in 0..nx {
                            slot.dirty.insert((tx, tz));
                        }
                    }
                }
            }
        }
        let ids: Vec<(u64, usize)> = self.meshes.iter().map(|(k, m)| (*k, m.settings.tiles_per_tick)).collect();
        for (mid, budget) in ids {
            report.tiles_rebuilt += self.rebuild(mid, Some(budget), world);
        }
        // Minds decide (their actions set agents' goals).
        let mut minds = std::mem::take(&mut self.minds);
        for (id, slot) in minds.iter_mut() {
            let Some(mind) = slot.mind.as_mut() else { continue };
            mind.hz = hz;
            let mut host = EngineHost { world: &*world, physics, meshes: &self.meshes, link: self.agents.get_mut(id), id: *id, node: 0 };
            mind.tick(&mut host, tick);
            report.minds += 1;
        }
        self.minds = minds;
        // Goals and positions into the crowds.
        for (id, link) in self.agents.iter_mut() {
            let Some(slot) = link.mesh.and_then(|m| self.meshes.get_mut(&m)) else { continue };
            let Some(e) = world.get(*id) else { continue };
            let wp = e.world_position();
            let moved_by_others = link.external || link.written.is_none_or(|w| w.x.to_bits() != wp.x.to_bits() || w.y.to_bits() != wp.y.to_bits() || w.z.to_bits() != wp.z.to_bits());
            if moved_by_others {
                slot.crowd.set_position(*id, wp - Vec3::new(0.0, link.offset_y, 0.0));
            }
            let goal = if self.minds.contains_key(id) {
                link.mind_goal.clone()
            } else {
                link.spec.goal.as_ref().and_then(|g| match point_of(g) {
                    Some(p) => Some(Goal::Point(p)),
                    None => resolve_target(world, g, &BTreeMap::new()).and_then(|(_, gid)| gid).map(Goal::Entity),
                })
            };
            let target = match goal {
                Some(Goal::Point(p)) => Some(p),
                Some(Goal::Entity(g)) => world.get(g).map(Entity::world_position),
                None => None,
            };
            slot.crowd.set_target(*id, target);
            if let Some(a) = slot.crowd.agent(*id) {
                if a.params.speed != link.spec.speed {
                    let params = AgentParams { speed: link.spec.speed, ..a.params };
                    let pos = a.pos;
                    let _ = slot.crowd.upsert(*id, params, pos);
                }
            }
        }
        // Crowds step.
        for slot in self.meshes.values_mut() {
            if slot.crowd.is_empty() {
                continue;
            }
            let s = slot.crowd.step(&slot.mesh, dt);
            report.agents += s.agents;
            report.moving += s.moving;
            report.repaths += s.repaths;
            report.waiting += s.waiting;
            report.nodes += s.nodes;
        }
        // Write back.
        let links: Vec<(u64, u64)> = self.agents.iter().filter_map(|(id, l)| l.mesh.map(|m| (*id, m))).collect();
        for (id, mid) in links {
            let Some(a) = self.meshes.get(&mid).and_then(|s| s.crowd.agent(id)) else { continue };
            let (pos, vel, moving) = (a.pos, a.vel, a.desired != Vec3::ZERO || a.vel != Vec3::ZERO);
            let link = self.agents.get_mut(&id).expect("listed");
            let Some(e) = world.get(id) else { continue };
            let top = e.parent.is_none();
            let speed2 = vel.x * vel.x + vel.z * vel.z;
            let yaw = link.face.take().or_else(|| (speed2 > 0.0025).then(|| dmath::atan2(vel.x, vel.z).to_degrees()));
            let mut change = UpdateSpec::default();
            if top {
                if let Some(y) = yaw {
                    let r = e.rotation_deg;
                    if (r.y - y).abs() > 1.0e-3 {
                        change.rotation_deg = Some(Vec3::new(r.x, y, r.z));
                    }
                }
            }
            if link.external {
                if let Some(b) = e.body {
                    let v = Vec3::new(vel.x, b.velocity.y, vel.z);
                    if v != b.velocity {
                        change.body = Some(Some(physics::Body { velocity: v, sleeping: false, ..b }));
                    }
                }
            } else if moving || link.written.is_none() {
                let want = pos + Vec3::new(0.0, link.offset_y, 0.0);
                let local = match e.parent.and_then(|p| world.get(p)) {
                    Some(parent) => parent.model().inverse().unwrap_or(Mat4::IDENTITY).transform_point(want),
                    None => want,
                };
                if local.x.to_bits() != e.position.x.to_bits() || local.y.to_bits() != e.position.y.to_bits() || local.z.to_bits() != e.position.z.to_bits() {
                    change.position = Some(local);
                }
            }
            if change.is_empty() {
                if !link.external {
                    link.written = Some(e.world_position());
                }
                continue;
            }
            match world.update(&EntityKey::Id(id), change, AGENT) {
                Ok(e) => {
                    link.written = Some(e.world_position());
                }
                Err(err) => {
                    self.problems.insert(id, Json::from(format!("navigation could not move it: {}", err.message)));
                }
            }
        }
        // Navigation's own writes are known: skip them next sync.
        if let Some((g, _)) = self.synced {
            self.synced = Some((g, world.draw_epoch()));
        }
        if self.hashes.len() == HASHES {
            self.hashes.pop_front();
        }
        self.hashes.push_back((tick, self.hash()));
        report.ms = started.elapsed().as_secs_f64() * 1000.0;
        if self.tick_ms.len() == 120 {
            self.tick_ms.pop_front();
        }
        self.tick_ms.push_back(report.ms);
        self.last = Some(report);
    }

    /// Hash of navigation's hidden state: navmeshes, crowds, minds.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::default();
        h.write_u64(self.meshes.len() as u64);
        for (id, m) in &self.meshes {
            h.write_u64(*id);
            h.write_u64(m.mesh.hash());
            h.write_u64(m.crowd.hash());
            h.write_u64(m.dirty.len() as u64);
        }
        h.write_u64(self.minds.len() as u64);
        for (id, m) in &self.minds {
            h.write_u64(*id);
            if let Some(mind) = &m.mind {
                mind.hash_into(&mut h);
            }
        }
        for (id, l) in &self.agents {
            h.write_u64(*id);
            h.write_u64(l.patrol_index as u64);
            h.write_u64(l.wander_count);
            h.write_u32(l.unreachable_ticks);
            match &l.mind_goal {
                Some(Goal::Point(p)) => {
                    h.write_f32(p.x);
                    h.write_f32(p.y);
                    h.write_f32(p.z);
                }
                Some(Goal::Entity(g)) => h.write_u64(*g),
                None => h.write_u8(0),
            }
            h.write_u64(l.goal_owner.map_or(u64::MAX, |n| n as u64));
            h.write_u64(l.face.map_or(u64::MAX, |f| u64::from(f.to_bits())));
        }
        h.finish()
    }

    /// The nav hash at the end of the last tick (`None` before navigation ran).
    pub fn last_hash(&self) -> Option<u64> {
        self.hashes.back().map(|(_, h)| *h)
    }

    /// One agent's state as JSON.
    pub fn agent_json(&self, world: &World, id: u64) -> Option<Json> {
        let link = self.agents.get(&id)?;
        let mut j = Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))]);
        j.set("mesh", Json::from(link.mesh.and_then(|m| self.meshes.get(&m)).map(|m| m.path.as_str())));
        j.set("moved_by", Json::from(if link.external { "physics character" } else { "navigation" }));
        if let Some(a) = link.mesh.and_then(|m| self.meshes.get(&m)).and_then(|m| m.crowd.agent(id)) {
            if let Json::Obj(pairs) = a.to_json() {
                for (k, v) in pairs {
                    j.set(k, v);
                }
            }
        }
        j.set("mind", Json::from(self.minds.contains_key(&id)));
        if let Some(p) = self.problems.get(&id) {
            j.set("problem", p.clone());
        }
        Some(j)
    }

    pub fn agent_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.agents.keys().copied()
    }

    pub fn mind_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.minds.keys().copied()
    }

    /// A mind's status (`None`: no mind there).
    pub fn mind_json(&self, world: &World, id: u64, max_bytes: usize) -> Option<Json> {
        let slot = self.minds.get(&id)?;
        let mut j = match &slot.mind {
            Some(m) => m.status_json(max_bytes),
            None => Json::obj([("status", Json::from("not running"))]),
        };
        j.set("id", Json::from(id));
        j.set("path", Json::from(world.path_of(id)));
        match (slot.def_from, &slot.def_path) {
            (Some((d, _)), _) => j.set("definition", Json::from(world.path_of(d))),
            (None, Some(p)) => j.set("definition", Json::from(p.as_str())),
            _ => false,
        };
        if let Some(p) = self.mind_problems.get(&id) {
            j.set("problem", p.clone());
        }
        if let Some(a) = self.agent_json(world, id) {
            j.set("agent", a);
        }
        Some(j)
    }

    /// The length of an agent's remaining path (`None`: no such agent or no goal). The seam the
    /// Lex lane's `navigate` behaviour reads as `path_remaining`.
    pub fn path_remaining(&self, id: u64) -> Option<f32> {
        let link = self.agents.get(&id)?;
        let a = self.meshes.get(&link.mesh?)?.crowd.agent(id)?;
        a.target.map(|_| a.remaining)
    }

    /// Summaries of every navmesh, for nav_info and perf_frame.
    pub fn summary_json(&self) -> Json {
        let meshes: Vec<Json> = self
            .meshes
            .values()
            .map(|m| {
                let mut j = m.mesh.summary_json();
                j.set("path", Json::from(m.path.as_str()));
                j.set("dirty_tiles", Json::from(m.dirty.len()));
                j.set("rebuilt_tiles", Json::from(m.rebuilt_tiles));
                j.set("agents", Json::from(m.crowd.len()));
                if m.models_skipped > 0 {
                    j.set(
                        "models_without_collider",
                        Json::obj([
                            ("count", Json::from(m.models_skipped)),
                            ("hint", Json::from("models feed a navmesh only through a collider (their built bounds depend on when a build finishes)")),
                        ]),
                    );
                }
                if !m.errors.is_empty() {
                    j.set("errors", Json::arr(m.errors.iter().cloned()));
                }
                j
            })
            .collect();
        let last = self.last.as_ref();
        let ms: Vec<f64> = self.tick_ms.iter().copied().collect();
        let mean = if ms.is_empty() { 0.0 } else { ms.iter().sum::<f64>() / ms.len() as f64 };
        let max = ms.iter().copied().fold(0.0, f64::max);
        Json::obj([
            ("navmeshes", Json::Arr(meshes)),
            ("agents", Json::from(self.agents.len())),
            ("minds", Json::from(self.minds.len())),
            ("problems", Json::from(self.problems.len() + self.mind_problems.len())),
            ("hash", Json::from(self.last_hash().map(|h| format!("{h:016x}")))),
            (
                "last_tick",
                last.map_or(Json::Null, |r| {
                    Json::obj([
                        ("tick", Json::from(r.tick)),
                        ("tiles_rebuilt", Json::from(r.tiles_rebuilt)),
                        ("minds", Json::from(r.minds)),
                        ("agents", Json::from(r.agents)),
                        ("moving", Json::from(r.moving)),
                        ("repaths", Json::from(r.repaths)),
                        ("repaths_waiting", Json::from(r.waiting)),
                        ("path_nodes", Json::from(r.nodes)),
                    ])
                }),
            ),
            ("tick_ms", Json::obj([("mean", Json::from((mean * 1000.0).round() / 1000.0)), ("max", Json::from((max * 1000.0).round() / 1000.0)), ("ticks", Json::from(ms.len()))])),
        ])
    }

    /// Keeps a nav_path answer for the `paths` overlay.
    pub fn remember_query(&mut self, mesh: u64, p: PathResult) {
        if self.queries.len() == QUERIES {
            self.queries.pop_front();
        }
        self.queries.push_back((mesh, p));
    }

    /// The agents' positions, corridors and goals, for the overlay.
    /// Agents are drawn where their entities are now (a character physics moved after this
    /// tick's steering is ahead of the crowd's copy), at their feet.
    pub fn agent_paths(&self, world: &World, max_corners: usize) -> Vec<AgentPath> {
        let mut out = Vec::new();
        for (id, link) in &self.agents {
            let Some(slot) = link.mesh.and_then(|m| self.meshes.get(&m)) else { continue };
            let Some(a) = slot.crowd.agent(*id) else { continue };
            let pos = world.get(*id).map_or(a.pos, |e| e.world_position() - Vec3::new(0.0, link.offset_y, 0.0));
            let walking = !a.corridor.is_empty() && a.target.is_some();
            let corners = if walking { slot.mesh.straighten(pos, a.path_end, &a.corridor, max_corners) } else { Vec::new() };
            // The goal where the path ends (on the navmesh), not a target entity's centre.
            let target = if walking { Some(a.path_end) } else { a.target };
            out.push(AgentPath { id: *id, pos, corners, target, vel: a.vel, radius: a.params.radius });
        }
        out
    }

    /// The world position an agent's origin goes to for a crowd position (with its offset).
    pub fn agent_offset(&self, id: u64) -> f32 {
        self.agents.get(&id).map_or(0.0, |l| l.offset_y)
    }
}

/// Builds every tile of a navmesh now (nav_build). Returns (tiles built, milliseconds).
pub fn build_now(nav: &mut Nav, world: &World, generation: u64, mesh: u64) -> (usize, f64) {
    nav.sync(world, generation);
    let started = std::time::Instant::now();
    let n = nav.rebuild(mesh, None, world);
    (n, started.elapsed().as_secs_f64() * 1000.0)
}

/// Bounds of the geometry that would feed a navmesh with these sources (for nav_build without
/// bounds): `None` when there is none.
pub fn source_bounds(nav: &mut Nav, world: &World, generation: u64, sources: &Sources) -> Option<(Vec3, Vec3)> {
    nav.sync(world, generation);
    let mut out: Option<(Vec3, Vec3)> = None;
    let under = under_of(world, sources);
    for s in nav.sources.values() {
        if let Some(src) = s.pick(sources, under) {
            out = Some(match out {
                None => (src.lo, src.hi),
                Some((a, b)) => (a.min(src.lo), b.max(src.hi)),
            });
        }
    }
    out
}

/// A quaternion for a yaw (degrees).
pub fn yaw_quat(yaw: f32) -> Quat {
    Quat::from_euler_xyz_deg(0.0, yaw, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::SpawnSpec;

    fn cube(w: &mut World, name: &str, p: Vec3, s: Vec3) -> u64 {
        let mut spec = SpawnSpec::new(Shape::Cube);
        spec.name = Some(name.into());
        spec.position = p;
        spec.scale = s;
        w.spawn(spec, "t").unwrap().id
    }

    fn navmesh(w: &mut World) -> u64 {
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("nav".into());
        w.spawn(s, "t").unwrap();
        let settings = MeshSettings {
            config: specula_via::Config { bounds_min: Vec3::new(-10.0, -1.0, -10.0), bounds_max: Vec3::new(10.0, 4.0, 10.0), tile_cells: 32, ..Default::default() },
            sources: Sources::default(),
            tiles_per_tick: 8,
        };
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("default".into());
        s.parent = Some(EntityKey::Path("/nav".into()));
        s.props = Json::obj([(MESH_KEY, settings.to_json())]);
        w.spawn(s, "t").unwrap().id
    }

    fn room(w: &mut World) {
        cube(w, "floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(20.0, 1.0, 20.0));
        cube(w, "wall", Vec3::new(0.0, 1.0, -2.0), Vec3::new(0.5, 2.0, 16.0));
    }

    fn walker(w: &mut World, name: &str, at: Vec3, extra: Vec<(&str, Json)>) -> u64 {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        s.position = at;
        s.scale = Vec3::new(0.5, 1.0, 0.5);
        let mut props = vec![(AGENT_KEY, Json::obj([("speed", Json::from(3.0))]))];
        props.extend(extra);
        s.props = Json::obj(props);
        w.spawn(s, "t").unwrap().id
    }

    fn run(nav: &mut Nav, w: &mut World, ticks: u64) {
        let physics = Physics::default();
        for t in 0..ticks {
            nav.tick(w, &physics, 0, t, 1.0 / 30.0, 30);
            let _ = w.take_changes();
        }
    }

    #[test]
    fn navmesh_from_shapes_and_an_agent_walks_around_the_wall() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let goal = Json::arr([Json::from(5.0), Json::from(0.0), Json::from(-2.0)]);
        let a = walker(&mut w, "walker", Vec3::new(-5.0, 0.5, -2.0), vec![]);
        w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(3.0)), ("goal", goal)]))])), ..Default::default() }, "t").unwrap();
        let mut nav = Nav::default();
        nav.sync(&w, 0);
        let (n, _) = build_now(&mut nav, &w, 0, m);
        assert!(n > 0);
        assert!(nav.meshes[&m].mesh.poly_count() > 0);
        run(&mut nav, &mut w, 300);
        let p = w.get(a).unwrap().world_position();
        assert!(horizontal(p, Vec3::new(5.0, 0.0, -2.0)) < 0.4, "arrived: {p:?} {:?}", nav.agent_json(&w, a));
        assert!((p.y - 0.5).abs() < 0.06, "the cube's origin rides half its height above the floor: {p:?}");
    }

    #[test]
    fn edits_rebuild_only_touched_tiles() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let before = nav.meshes[&m].mesh.hash();
        run(&mut nav, &mut w, 1);
        assert!(nav.meshes[&m].dirty.is_empty());
        cube(&mut w, "crate", Vec3::new(5.0, 0.5, 5.0), Vec3::ONE);
        nav.sync(&w, 0);
        let dirty = nav.meshes[&m].dirty.len();
        assert!(dirty > 0 && dirty < 25, "{dirty} tiles");
        run(&mut nav, &mut w, 2);
        assert_ne!(nav.meshes[&m].mesh.hash(), before);
        // The same world built from scratch gives the same navmesh.
        let mut fresh = Nav::default();
        build_now(&mut fresh, &w, 0, m);
        assert_eq!(fresh.meshes[&m].mesh.hash(), nav.meshes[&m].mesh.hash());
    }

    #[test]
    fn a_mind_patrols_then_chases() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let def = Json::parse(
            r#"{"root": {"selector": [
                {"sequence": [{"if": "near", "target": "/thief", "distance": 4}, {"do": "chase", "target": "/thief", "within": 0.8}]},
                {"do": "patrol", "points": [[4, 0, 4], [4, 0, 7]]}
            ]}}"#,
        )
        .unwrap();
        let g = walker(&mut w, "guard", Vec3::new(3.0, 0.5, 3.0), vec![(MIND_KEY, Json::obj([("def", def)]))]);
        cube(&mut w, "thief", Vec3::new(-6.0, 0.5, -8.0), Vec3::new(0.4, 1.0, 0.4));
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        assert!(nav.problems.is_empty() && nav.mind_problems.is_empty(), "{:?} {:?}", nav.problems, nav.mind_problems);
        let mut visited = [false; 2];
        for _ in 0..12 {
            run(&mut nav, &mut w, 25);
            let p = w.get(g).unwrap().world_position();
            visited[0] |= horizontal(p, Vec3::new(4.0, 0.0, 4.0)) < 0.5;
            visited[1] |= horizontal(p, Vec3::new(4.0, 0.0, 7.0)) < 0.5;
        }
        assert!(visited[0] && visited[1], "patrolled both points: {visited:?}");
        // The thief comes close: the guard chases and catches up.
        let thief = w.resolve(&EntityKey::Path("/thief".into())).unwrap();
        w.update(&EntityKey::Id(thief), UpdateSpec { position: Some(Vec3::new(5.5, 0.5, 2.0)), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 90);
        let p = w.get(g).unwrap().world_position();
        assert!(horizontal(p, Vec3::new(5.5, 0.0, 2.0)) < 1.3, "caught: {p:?} {:?}", nav.mind_json(&w, g, 2048).map(|j| j.to_pretty()));
        let status = nav.mind_json(&w, g, 2048).unwrap();
        let trace = status.get("trace").unwrap().to_string();
        assert!(trace.contains("do chase"), "{trace}");
    }

    #[test]
    fn what_moves_never_carves_the_navmesh() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let a = walker(&mut w, "walker", Vec3::new(-5.0, 0.5, 5.0), vec![]);
        // Its visible body: a child cube without a collider.
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("body".into());
        s.parent = Some(EntityKey::Id(a));
        s.scale = Vec3::new(1.5, 1.0, 1.5);
        w.spawn(s, "t").unwrap();
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let slot = &nav.meshes[&m];
        assert!(slot.mesh.nearest(Vec3::new(-5.0, 0.0, 5.0), Vec3::new(0.01, 0.5, 0.01)).is_some(), "no hole under the agent");
        // A crate that becomes an agent stops being an obstacle (its tiles rebuild).
        let c = cube(&mut w, "crate", Vec3::new(5.0, 0.5, 5.0), Vec3::ONE);
        run(&mut nav, &mut w, 2);
        assert!(nav.meshes[&m].mesh.nearest(Vec3::new(5.0, 0.0, 5.0), Vec3::new(0.01, 0.5, 0.01)).is_none(), "a crate is an obstacle");
        w.update(&EntityKey::Id(c), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::Obj(Vec::new()))])), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 3);
        assert!(nav.meshes[&m].mesh.nearest(Vec3::new(5.0, 0.0, 5.0), Vec3::new(0.01, 0.5, 0.01)).is_some(), "an agent isn't");
    }

    #[test]
    fn a_move_to_that_takes_over_keeps_its_goal() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let def = Json::parse(
            r#"{"selector": [
                {"sequence": [{"if": "near", "target": "/thief", "distance": 4}, {"do": "move_to", "target": "/thief"}]},
                {"do": "patrol", "points": [[4, 0, 4], [4, 0, 7]]}
            ]}"#,
        )
        .unwrap();
        let g = walker(&mut w, "guard", Vec3::new(4.0, 0.5, 4.0), vec![(MIND_KEY, Json::obj([("def", def)]))]);
        let thief = cube(&mut w, "thief", Vec3::new(-6.0, 0.5, -8.0), Vec3::new(0.4, 1.0, 0.4));
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        run(&mut nav, &mut w, 30);
        // The thief comes within 4 m: move_to starts (and patrol is aborted after it).
        let at = w.get(g).unwrap().world_position();
        w.update(&EntityKey::Id(thief), UpdateSpec { position: Some(Vec3::new(at.x + 3.0, 0.5, at.z)), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 90);
        let p = w.get(g).unwrap().world_position();
        // (The thief's cube carves the navmesh around it: the nearest walkable point is ~0.7 m off.)
        assert!(horizontal(p, Vec3::new(at.x + 3.0, 0.0, at.z)) < 1.0, "walked to the thief: {p:?} {:?}", nav.agent_json(&w, g));
    }

    #[test]
    fn a_subtree_moved_under_another_parent_leaves_or_joins_under() {
        let mut w = World::default();
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("level".into());
        w.spawn(s, "t").unwrap();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("floor".into());
        s.parent = Some(EntityKey::Path("/level".into()));
        s.position = Vec3::new(0.0, -0.5, 0.0);
        s.scale = Vec3::new(20.0, 1.0, 20.0);
        w.spawn(s, "t").unwrap();
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("props".into());
        s.parent = Some(EntityKey::Path("/level".into()));
        let props = w.spawn(s, "t").unwrap().id;
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("crate".into());
        s.parent = Some(EntityKey::Id(props));
        s.position = Vec3::new(3.0, 0.5, 3.0);
        w.spawn(s, "t").unwrap();
        let m = navmesh(&mut w);
        let mut settings = MeshSettings::from_json(w.get(m).unwrap().props.get(MESH_KEY).unwrap()).unwrap();
        settings.sources.under = Some("/level".into());
        w.update(&EntityKey::Id(m), UpdateSpec { props: Some(Json::obj([(MESH_KEY, settings.to_json())])), ..Default::default() }, "t").unwrap();
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        assert!(nav.meshes[&m].mesh.nearest(Vec3::new(3.0, 0.0, 3.0), Vec3::new(0.01, 0.5, 0.01)).is_none(), "the crate carves");
        // Move /level/props to the top level (same world transform): the crate leaves /level.
        w.update(&EntityKey::Id(props), UpdateSpec { parent: Some(None), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 20);
        let mut fresh = Nav::default();
        build_now(&mut fresh, &w, 0, m);
        assert_eq!(nav.meshes[&m].mesh.hash(), fresh.meshes[&m].mesh.hash(), "incremental equals a full build");
        assert!(nav.meshes[&m].mesh.nearest(Vec3::new(3.0, 0.0, 3.0), Vec3::new(0.01, 0.5, 0.01)).is_some(), "it no longer carves");
    }

    #[test]
    fn a_replaced_mind_drops_the_old_goal() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let go = Json::parse(r#"{"do": "move_to", "target": [8, 0, 8]}"#).unwrap();
        let a = walker(&mut w, "a", Vec3::new(-7.5, 0.5, -7.5), vec![(MIND_KEY, Json::obj([("def", go)]))]);
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        run(&mut nav, &mut w, 10);
        let wait = Json::parse(r#"{"wait": 100000}"#).unwrap();
        w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(MIND_KEY, Json::obj([("def", wait)]))])), ..Default::default() }, "t").unwrap();
        // (It slows to a stop: 8 m/s² from 2 m/s takes a quarter second.)
        run(&mut nav, &mut w, 20);
        let p0 = w.get(a).unwrap().world_position();
        run(&mut nav, &mut w, 60);
        let p1 = w.get(a).unwrap().world_position();
        assert!(horizontal(p0, p1) < 0.05, "stands still once its mind only waits: {p0:?} -> {p1:?}");
    }

    #[test]
    fn a_mind_waits_for_its_definition() {
        let mut w = World::default();
        let id = walker(&mut w, "g", Vec3::ZERO, vec![(MIND_KEY, Json::obj([("def", Json::from("/brain"))]))]);
        let mut nav = Nav::default();
        nav.sync(&w, 0);
        assert!(nav.mind_problems.get(&id).is_some_and(|p| p.to_string().contains("doesn't exist")));
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("brain".into());
        s.props = Json::obj([(DEF_KEY, Json::parse(r#"{"wait": 5}"#).unwrap())]);
        let b = w.spawn(s, "t").unwrap().id;
        nav.sync(&w, 0);
        assert!(!nav.mind_problems.contains_key(&id), "{:?}", nav.mind_problems);
        assert_eq!(nav.mind_json(&w, id, 1024).unwrap().get("definition").and_then(Json::as_str), Some("/brain"));
        // An invalid edit of the definition is the mind's problem, and an agent relink keeps it.
        w.update(&EntityKey::Id(b), UpdateSpec { props: Some(Json::obj([(DEF_KEY, Json::parse(r#"{"do": "chace", "target": "/x"}"#).unwrap())])), ..Default::default() }, "t").unwrap();
        nav.sync(&w, 0);
        assert!(nav.mind_problems.get(&id).is_some_and(|p| p.to_string().contains("chase")));
        let m = navmesh(&mut w);
        nav.sync(&w, 0);
        assert!(nav.meshes.contains_key(&m) && nav.mind_problems.contains_key(&id), "a navmesh appearing (agents relink) keeps the mind's problem");
    }

    #[test]
    fn invalid_minds_are_problems_with_diagnostics() {
        let mut w = World::default();
        let def = Json::parse(r#"{"do": "chace", "target": "/x"}"#).unwrap();
        let id = walker(&mut w, "bad", Vec3::ZERO, vec![(MIND_KEY, Json::obj([("def", def)]))]);
        let mut nav = Nav::default();
        nav.sync(&w, 0);
        let p = nav.mind_problems.get(&id).unwrap().to_string();
        assert!(p.contains("did you mean 'chase'"), "{p}");
    }

    #[test]
    fn twin_runs_hash_the_same() {
        let make = || {
            let mut w = World::default();
            room(&mut w);
            let m = navmesh(&mut w);
            for i in 0..6 {
                let goal = Json::arr([Json::from(6.0 - i as f64), Json::from(0.0), Json::from(5.0)]);
                let id = walker(&mut w, &format!("a{i}"), Vec3::new(-6.0 + i as f32 * 0.7, 0.5, -6.0), vec![]);
                w.update(&EntityKey::Id(id), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("goal", goal)]))])), ..Default::default() }, "t").unwrap();
            }
            let mut nav = Nav::default();
            build_now(&mut nav, &w, 0, m);
            run(&mut nav, &mut w, 200);
            (nav.last_hash(), w.state_hash())
        };
        assert_eq!(make(), make());
    }
}
