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

use crate::write_policy::WorldWrite;
use crate::physics::{self, BodyKind, ColliderShape, Physics};
use crate::world::{self, Entity, EntityKey, Shape, UpdateSpec, World};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::rng::Rng;
use specula_core::{dmath, Json};
use specula_mens::{self as mens, Arg, ArgKind, Def, Diagnostic, Mind, Phase, Status, Verb, Vocabulary};
use specula_via::{AgentParams, AgentStatus, Crowd, CrowdSettings, Filter, Input, LinkKind, Mark, NavError, NavMesh, OffMeshLink, PathResult, Source, Triangle};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

/// The journal's name for navigation's own writes.
pub const AGENT: &str = "nav";
/// Props keys the navigation system reads.
pub const MESH_KEY: &str = "navmesh";
pub const AGENT_KEY: &str = "nav_agent";
pub const MIND_KEY: &str = "mind";
pub const DEF_KEY: &str = "mind_def";
/// An entity's area: `props.nav_area = "water"` (its walkable surfaces) or
/// `{"area": "mud", "volume": true}` (its world bounds mark the open space inside them; it feeds
/// no geometry).
pub const AREA_KEY: &str = "nav_area";
/// Entities with this tag never feed a navmesh.
pub const IGNORE_TAG: &str = "nav_ignore";
/// Most navmeshes one world has.
pub const MAX_MESHES: usize = 16;
/// Most minds one world runs.
pub const MAX_MINDS: usize = 1024;
/// Per-tick hashes kept.
const HASHES: usize = 1024;
/// Columns of tile grid (with borders) rebuilt per tick at most, at least one tile: a default
/// tile (58 x 58 = 3,364 columns) takes about 0.3 ms on the reference machine's CPU class, the
/// widest allowed (96 x 96) a few milliseconds (more on a slow laptop).
pub const COLUMNS_PER_TICK: usize = 9_216;
/// Mind work per tick (node visits, senses and inputs at 1, line-of-sight raycasts at 16):
/// minds past it wait for the next tick, round robin by id; at least one runs. Measured
/// (release, 1,000 minds of 200 cached `sees`): about 0.4 microseconds a unit, so the budget is
/// about 1.7 ms a tick.
pub const MIND_WORK_PER_TICK: u64 = 4_096;
/// Version of [`Nav::save_state`]'s format.
pub const SAVE_VERSION: u64 = 1;
/// The save-game `nav` section: a version, then the state as JSON (floats as their bits).
pub const SECTION_VERSION: u32 = 1;
/// Largest `nav` section accepted (a thousand agents' corridors and minds fit in a few MB).
pub const MAX_SECTION_BYTES: usize = 64 << 20;

/// The save-game `nav` section for [`Nav::save_state`]'s state (`too_large` past the cap).
pub fn encode_section(state: &Json) -> Result<Vec<u8>, world::WorldError> {
    let mut out = SECTION_VERSION.to_le_bytes().to_vec();
    out.extend_from_slice(state.to_string().as_bytes());
    if out.len() > MAX_SECTION_BYTES {
        return Err(world::WorldError::new("too_large", format!("navigation's state is {} bytes; a save holds at most {MAX_SECTION_BYTES}", out.len())));
    }
    Ok(out)
}

fn tiles_json(set: &BTreeSet<(u32, u32)>) -> Json {
    Json::arr(set.iter().map(|(x, z)| Json::arr([Json::from(*x), Json::from(*z)])))
}

/// A tile list from a save, checked against a navmesh's size.
fn tiles_of(j: Option<&Json>, nx: u32, nz: u32) -> Option<BTreeSet<(u32, u32)>> {
    let mut out = BTreeSet::new();
    for t in j?.as_array()? {
        let a = t.as_array().filter(|a| a.len() == 2)?;
        let (x, z) = (u32::try_from(a[0].as_u64()?).ok()?, u32::try_from(a[1].as_u64()?).ok()?);
        if x >= nx || z >= nz {
            return None;
        }
        out.insert((x, z));
    }
    Some(out)
}

/// Decodes and checks a `nav` section's shape (on a load's worker thread): its version, its
/// size, its JSON and the kinds and counts of what it lists. Whether the state fits the world is
/// checked when it is restored ([`Nav::restore_state`]).
pub fn parse_section(bytes: &[u8]) -> Result<Json, (&'static str, String)> {
    if bytes.len() > MAX_SECTION_BYTES {
        return Err(("too_large", format!("the nav section is {} bytes; at most {MAX_SECTION_BYTES}", bytes.len())));
    }
    let Some((head, body)) = bytes.split_first_chunk::<4>() else { return Err(("corrupt_section", "the nav section is shorter than its version".into())) };
    let version = u32::from_le_bytes(*head);
    if version != SECTION_VERSION {
        return Err(("unsupported_version", format!("nav section version {version}; this build reads {SECTION_VERSION}")));
    }
    let text = std::str::from_utf8(body).map_err(|_| ("corrupt_section", "the nav section isn't UTF-8 text".to_string()))?;
    let j = Json::parse(text).map_err(|e| ("corrupt_section", format!("the nav section isn't JSON: {e:?}")))?;
    let list = |k: &str, max: usize| -> Result<&[Json], (&'static str, String)> {
        let l = j.get(k).and_then(Json::as_array).ok_or_else(|| ("corrupt_section", format!("the nav section has no '{k}' list")))?;
        if l.len() > max {
            return Err(("corrupt_section", format!("the nav section lists {} {k}; at most {max}", l.len())));
        }
        if l.iter().any(|x| x.get("id").and_then(Json::as_u64).is_none()) {
            return Err(("corrupt_section", format!("an entry of '{k}' has no id")));
        }
        Ok(l)
    };
    if j.get("version").and_then(Json::as_u64) != Some(SAVE_VERSION) {
        return Err(("unsupported_version", format!("nav state version must be {SAVE_VERSION}")));
    }
    if j.get("hash").and_then(Json::as_str).and_then(|s| u64::from_str_radix(s, 16).ok()).is_none() || j.get("mind_cursor").and_then(Json::as_u64).is_none() {
        return Err(("corrupt_section", "the nav section has no hash or mind_cursor".into()));
    }
    list("meshes", MAX_MESHES)?;
    list("minds", MAX_MINDS)?;
    list("agents", MAX_MESHES * specula_via::crowd::MAX_AGENTS)?;
    Ok(j)
}
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

/// A named area of a navmesh: what crossing it costs (times the distance) and its flags (agents
/// and queries may avoid areas by name or by flag).
#[derive(Clone, Debug, PartialEq)]
pub struct AreaDef {
    pub name: String,
    pub cost: f32,
    pub flags: Vec<String>,
}

/// Most named areas one navmesh has (ids 1..=31; 0 is plain ground).
pub const MAX_AREA_DEFS: usize = specula_via::MAX_AREAS - 1;

/// A navmesh's settings (`props.navmesh`).
#[derive(Clone, Debug, PartialEq)]
pub struct MeshSettings {
    pub config: specula_via::Config,
    pub sources: Sources,
    pub tiles_per_tick: usize,
    /// Named areas (`props.nav_area` on entities refers to them).
    pub areas: Vec<AreaDef>,
    /// Off-mesh links (jumps, drops, ladders); their `area` is an index into `areas` plus one.
    pub links: Vec<OffMeshLink>,
}

impl MeshSettings {
    pub fn new(config: specula_via::Config, sources: Sources, tiles_per_tick: usize) -> MeshSettings {
        MeshSettings { config, sources, tiles_per_tick, areas: Vec::new(), links: Vec::new() }
    }

    /// An area's id (0: plain ground, or a name it doesn't have).
    pub fn area_id(&self, name: &str) -> u8 {
        self.areas.iter().position(|a| a.name == name).map_or(0, |i| (i + 1) as u8)
    }

    /// The same settings but for the off-mesh links (a change of links alone rebuilds nothing).
    pub fn same_but_links(&self, o: &MeshSettings) -> bool {
        self.config == o.config && self.sources == o.sources && self.tiles_per_tick == o.tiles_per_tick && self.areas == o.areas
    }

    /// A search filter: the areas' costs, overridden by `costs` (by name), with the areas named
    /// in `avoid` or flagged with one of `avoid_flags` excluded.
    pub fn filter(&self, costs: &[(String, f32)], avoid: &[String], avoid_flags: &[String]) -> Filter {
        let mut f = Filter::default();
        for (i, a) in self.areas.iter().enumerate() {
            let id = i + 1;
            f.cost[id] = costs.iter().find(|(n, _)| *n == a.name).map_or(a.cost, |(_, c)| *c);
            if avoid.contains(&a.name) || a.flags.iter().any(|fl| avoid_flags.contains(fl)) {
                f.exclude |= 1 << id;
            }
        }
        if let Some((_, c)) = costs.iter().find(|(n, _)| n == "ground") {
            f.cost[0] = *c;
        }
        f
    }

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
        let j = Json::obj([
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
        ]);
        let mut j = j;
        if !self.areas.is_empty() {
            j.set(
                "areas",
                Json::arr(self.areas.iter().map(|a| {
                    Json::obj([("name", Json::from(a.name.as_str())), ("cost", Json::from(a.cost)), ("flags", Json::arr(a.flags.iter().map(|f| Json::from(f.as_str()))))])
                })),
            );
        }
        if !self.links.is_empty() {
            j.set(
                "links",
                Json::arr(self.links.iter().map(|l| {
                    let mut o = Json::obj([
                        ("from", exact(l.from)),
                        ("to", exact(l.to)),
                        ("kind", Json::from(l.kind.name())),
                        ("bidirectional", Json::from(l.bidirectional)),
                        ("cost", Json::from(l.cost)),
                        ("radius", Json::from(l.radius)),
                    ]);
                    if l.area > 0 {
                        if let Some(a) = self.areas.get(usize::from(l.area) - 1) {
                            o.set("area", Json::from(a.name.as_str()));
                        }
                    }
                    o
                })),
            );
        }
        j
    }

    /// Reads `props.navmesh` (every key optional; missing ones take defaults).
    pub fn from_json(v: &Json) -> Result<MeshSettings, world::WorldError> {
        const KEYS: [&str; 12] = ["cell_size", "cell_height", "agent_radius", "agent_height", "max_step", "max_slope_deg", "tile_cells", "bounds", "sources", "tiles_per_tick", "areas", "links"];
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
        let mut config = specula_via::Config {
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
        // Field ranges first: nothing is derived from an unchecked config.
        config.validate_fields().map_err(|e| nav_error(&e))?;
        // Settings saved before the tile-width cap: narrow the tiles (same navmesh, smaller tiles)
        // rather than refuse the world's navmesh.
        let border = specula_via::Derived::of(&config).border;
        let widest = (specula_via::MAX_TILE_SIDE as u32).saturating_sub(2 * border);
        if config.tile_cells > widest && widest >= 8 {
            config.tile_cells = widest;
        }
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
            None | Some(Json::Null) => DEFAULT_TILES_PER_TICK,
            Some(t) => match t.as_u64() {
                Some(n) if (1..=256).contains(&n) => n as usize,
                _ => return Err(invalid("tiles_per_tick", "'tiles_per_tick' must be 1..256")),
            },
        };
        let areas = parse_areas(v.get("areas"))?;
        let mut s = MeshSettings { config, sources, tiles_per_tick, areas, links: Vec::new() };
        s.links = parse_links(v.get("links"), &s)?;
        Ok(s)
    }
}

fn parse_areas(v: Option<&Json>) -> Result<Vec<AreaDef>, world::WorldError> {
    let list = match v {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Arr(l)) if l.len() <= MAX_AREA_DEFS => l,
        Some(_) => return Err(invalid("areas", format!("'areas' must be an array of at most {MAX_AREA_DEFS} {{name, cost?, flags?}}"))),
    };
    let mut out: Vec<AreaDef> = Vec::new();
    for (i, a) in list.iter().enumerate() {
        let bad = |m: String| invalid("areas", format!("areas[{i}]: {m}"));
        let Json::Obj(pairs) = a else { return Err(bad("must be {name, cost?, flags?}".into())) };
        if let Some((k, _)) = pairs.iter().find(|(k, _)| !["name", "cost", "flags"].contains(&k.as_str())) {
            return Err(bad(format!("'{k}' is not an area setting (name, cost, flags)")));
        }
        let name = a.get("name").and_then(Json::as_str).filter(|n| !n.is_empty() && n.len() <= 32 && *n != "ground").ok_or_else(|| bad("'name' must be 1..32 characters (not \"ground\")".into()))?;
        if out.iter().any(|o| o.name == name) {
            return Err(bad(format!("'{name}' is named twice")));
        }
        let cost = match a.get("cost") {
            None | Some(Json::Null) => 1.0,
            Some(c) => c.as_f64().filter(|c| (0.1..=1000.0).contains(c)).ok_or_else(|| bad("'cost' must be 0.1..1000".into()))? as f32,
        };
        let flags = match a.get("flags") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(f)) if f.len() <= 8 => f.iter().map(|x| x.as_str().filter(|s| !s.is_empty() && s.len() <= 32).map(str::to_owned).ok_or_else(|| bad("'flags' must be names".into()))).collect::<Result<_, _>>()?,
            Some(_) => return Err(bad("'flags' must be at most 8 names".into())),
        };
        out.push(AreaDef { name: name.to_owned(), cost, flags });
    }
    Ok(out)
}

fn parse_links(v: Option<&Json>, s: &MeshSettings) -> Result<Vec<OffMeshLink>, world::WorldError> {
    let list = match v {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Arr(l)) if l.len() <= specula_via::MAX_OFFMESH_LINKS => l,
        Some(_) => return Err(invalid("links", format!("'links' must be an array of at most {} links", specula_via::MAX_OFFMESH_LINKS))),
    };
    let mut out = Vec::new();
    for (i, l) in list.iter().enumerate() {
        let bad = |m: String| invalid("links", format!("links[{i}]: {m}"));
        let Json::Obj(pairs) = l else { return Err(bad("must be {from, to, kind?, bidirectional?, cost?, area?, radius?}".into())) };
        const LKEYS: [&str; 7] = ["from", "to", "kind", "bidirectional", "cost", "area", "radius"];
        if let Some((k, _)) = pairs.iter().find(|(k, _)| !LKEYS.contains(&k.as_str())) {
            return Err(bad(format!("'{k}' is not a link setting ({})", LKEYS.join(", "))));
        }
        let from = l.get("from").and_then(point_of).ok_or_else(|| bad("'from' must be a point [x, y, z]".into()))?;
        let to = l.get("to").and_then(point_of).ok_or_else(|| bad("'to' must be a point [x, y, z]".into()))?;
        let kind = match l.get("kind") {
            None | Some(Json::Null) => LinkKind::Jump,
            Some(k) => k.as_str().and_then(LinkKind::parse).ok_or_else(|| bad("'kind' must be jump, drop or ladder".into()))?,
        };
        let bidirectional = match l.get("bidirectional") {
            None | Some(Json::Null) => kind != LinkKind::Drop,
            Some(b) => b.as_bool().ok_or_else(|| bad("'bidirectional' must be true or false".into()))?,
        };
        let num = |k: &str, d: f32, lo: f64, hi: f64| -> Result<f32, world::WorldError> {
            match l.get(k) {
                None | Some(Json::Null) => Ok(d),
                Some(x) => x.as_f64().filter(|f| (lo..=hi).contains(f)).map(|f| f as f32).ok_or_else(|| bad(format!("'{k}' must be {lo}..{hi}"))),
            }
        };
        let area = match l.get("area") {
            None | Some(Json::Null) => 0,
            Some(a) => {
                let n = a.as_str().ok_or_else(|| bad("'area' must be an area's name".into()))?;
                match s.area_id(n) {
                    0 => return Err(bad(format!("no area '{n}' in 'areas'"))),
                    id => id,
                }
            }
        };
        if (to - from).length() > 64.0 {
            return Err(bad("a link spans at most 64 m".into()));
        }
        out.push(OffMeshLink { from, to, kind, bidirectional, cost: num("cost", 1.0, 0.1, 1000.0)?, area, radius: num("radius", 0.5, 0.05, 5.0)? });
    }
    Ok(out)
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
    /// Area costs for this agent (by area name; "ground" is plain ground), overriding the
    /// navmesh's; areas it never enters, by name or by flag.
    pub area_costs: Vec<(String, f32)>,
    pub avoid_areas: Vec<String>,
    pub avoid_flags: Vec<String>,
}

/// Area preferences: costs by area name, areas avoided by name, areas avoided by flag.
pub type AreaPrefs = (Vec<(String, f32)>, Vec<String>, Vec<String>);

/// `{name: cost}` (0.1..1000) and lists of names, as nav_agent and nav_path take them.
pub fn parse_area_prefs(v: &Json, prefix: &str) -> Result<AreaPrefs, world::WorldError> {
    let costs = match v.get("area_costs") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Obj(pairs)) if pairs.len() <= specula_via::MAX_AREAS => pairs
            .iter()
            .map(|(k, c)| {
                c.as_f64().filter(|c| (0.1..=1000.0).contains(c)).map(|c| (k.clone(), c as f32)).ok_or_else(|| invalid("area_costs", format!("'{prefix}area_costs.{k}' must be 0.1..1000")))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(invalid("area_costs", format!("'{prefix}area_costs' must be {{area: cost}}"))),
    };
    let names = |k: &str| -> Result<Vec<String>, world::WorldError> {
        match v.get(k) {
            None | Some(Json::Null) => Ok(Vec::new()),
            Some(Json::Arr(l)) if l.len() <= specula_via::MAX_AREAS => {
                l.iter().map(|x| x.as_str().map(str::to_owned).ok_or_else(|| invalid(k, format!("'{prefix}{k}' must be names")))).collect()
            }
            Some(_) => Err(invalid(k, format!("'{prefix}{k}' must be an array of names"))),
        }
    };
    Ok((costs, names("avoid_areas")?, names("avoid_flags")?))
}

impl AgentSpec {
    pub fn from_json(v: &Json) -> Result<AgentSpec, world::WorldError> {
        const KEYS: [&str; 10] = ["mesh", "speed", "radius", "accel", "arrive", "avoid", "goal", "area_costs", "avoid_areas", "avoid_flags"];
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
        let (area_costs, avoid_areas, avoid_flags) = parse_area_prefs(v, "nav_agent.")?;
        Ok(AgentSpec {
            area_costs,
            avoid_areas,
            avoid_flags,
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
    /// `props.nav_area`: the area its surfaces get, or (a volume) the world box that marks one.
    area: Option<String>,
    mark: Option<(Vec3, Vec3)>,
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
    fn passes(&self, s: &Sources, under: Option<Option<u64>>) -> bool {
        match under {
            Some(Some(u)) if !self.ancestors.contains(&u) => return false,
            Some(None) => return false,
            _ => {}
        }
        !s.exclude_tags.iter().any(|t| self.tags.contains(t))
    }

    /// An area volume this navmesh reads.
    fn mark_for(&self, s: &Sources, under: Option<Option<u64>>) -> Option<(Vec3, Vec3)> {
        self.mark.filter(|_| self.passes(s, under))
    }

    /// The world box whose tiles change when this entry comes, goes or changes.
    fn bounds_for(&self, s: &Sources, under: Option<Option<u64>>) -> Option<(Vec3, Vec3)> {
        self.pick(s, under).map(|src| (src.lo, src.hi)).or_else(|| self.mark_for(s, under))
    }

    fn pick(&self, s: &Sources, under: Option<Option<u64>>) -> Option<&Source> {
        if !self.passes(s, under) {
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
        // A compound: each part where it sits (solid, as its parts are).
        ColliderShape::Compound { parts } => {
            let mut out = Vec::new();
            for p in parts.iter() {
                let rot = specula_core::math::Quat::from_euler_xyz_deg(p.rotation_deg.x, p.rotation_deg.y, p.rotation_deg.z);
                let pm = *m * Mat4::from_trs(o + p.offset, rot, Vec3::ONE);
                let part = physics::Collider { shape: p.shape.clone(), offset: Vec3::ZERO, ..c.clone() };
                out.extend(collider_tris(&pm, &part)?.0);
            }
            (out, true)
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
    let (area, volume) = match e.props.get(AREA_KEY) {
        Some(Json::Str(a)) => (Some(a.clone()), false),
        Some(o @ Json::Obj(_)) => (o.get("area").and_then(Json::as_str).map(str::to_owned), o.get("volume").and_then(Json::as_bool).unwrap_or(false)),
        _ => (None, false),
    };
    // An area volume may be hidden (it is a marker, not geometry).
    if e.transient || (!e.shown() && !volume) || e.in_ui() || e.shape.is_2d() || e.tags.contains(IGNORE_TAG) {
        return None;
    }
    if e.props.get(MESH_KEY).is_some() || moves(world, e) {
        return None;
    }
    let ancestors_of = || {
        let mut ancestors = vec![e.id];
        let mut at = e.parent;
        while let Some(p) = at {
            ancestors.push(p);
            at = world.get(p).and_then(|x| x.parent);
        }
        ancestors
    };
    if volume {
        // The volume is the entity's unit box (-0.5..0.5 scaled, turned and placed).
        let m = e.model();
        let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for i in 0..8 {
            let c = m.transform_point(Vec3::new(if i & 1 == 0 { -0.5 } else { 0.5 }, if i & 2 == 0 { -0.5 } else { 0.5 }, if i & 4 == 0 { -0.5 } else { 0.5 }));
            lo = lo.min(c);
            hi = hi.max(c);
        }
        let mark = area.is_some().then_some((lo, hi));
        return Some(SourceEntry { version: e.version, drawn: e.shown() && !e.in_ui(), cols: e.model().cols, ancestors: ancestors_of(), tags: e.tags.clone(), collider: None, shape: None, area, mark });
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
    Some(SourceEntry { version: e.version, drawn: true, cols: m.cols, ancestors: ancestors_of(), tags: e.tags.clone(), collider, shape, area, mark: None })
}

// ---------------------------------------------------------------------------------------------
// Meshes, agents, minds

/// One navmesh and the agents on it.
#[derive(Clone)]
pub struct MeshSlot {
    pub id: u64,
    pub path: String,
    pub settings: MeshSettings,
    pub mesh: NavMesh,
    pub crowd: Crowd,
    pub dirty: BTreeSet<(u32, u32)>,
    /// Tiles never built yet (the navmesh is ready when there are none; later edits only make
    /// tiles dirty).
    pub unbuilt: BTreeSet<(u32, u32)>,
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
    /// Every tile built: the navmesh serves queries and agents. A first build fills `mesh` in
    /// place (queries say "building" meanwhile).
    pub ready: bool,
    /// New settings being built beside the serving navmesh, swapped in when complete.
    pub next: Option<Box<NextMesh>>,
    /// Big sources' triangles by tile, for this grid.
    buckets: Buckets,
    /// Created by a world load (or engine start): built in full before its first tick.
    complete_first: bool,
    /// For a restore: the tiles the load builds (the rest come from the save or stay unbuilt).
    load_plan: Option<BTreeSet<(u32, u32)>>,
}

/// Triangles of big sources by tile, per source entity.
type Buckets = BTreeMap<u64, BTreeMap<(u32, u32), Vec<u32>>>;

/// A navmesh being built for new settings while the old one serves.
#[derive(Clone)]
pub struct NextMesh {
    pub settings: MeshSettings,
    pub mesh: NavMesh,
    pub queue: BTreeSet<(u32, u32)>,
    props: Json,
    buckets: Buckets,
}

/// Sources with more triangles than this are bucketed by tile.
const BUCKET_MIN: usize = 256;
/// Work units (grid columns, 16 per triangle) of a default-size tile (48 cells and its border):
/// `tiles_per_tick` counts these. Measured: about 1,700 units per millisecond on the
/// development machine's CPU, so a default tile is about 2 ms.
pub const TILE_WORK: u64 = 3_364;
/// Default tile rebuild work per tick (about 4 ms).
pub const DEFAULT_TILES_PER_TICK: usize = 2;
/// Work one slice of a load builds (about 10 ms); slices run back to back within
/// [`LOAD_FRAME`] of wall time per frame.
pub const LOAD_SLICE_WORK: u64 = 16_000;
/// Wall time a frame spends on a load's slices (calls are read between frames).
pub const LOAD_FRAME: std::time::Duration = std::time::Duration::from_millis(30);
/// How long time_control step, and a playtest's ticks, wait for a load before answering
/// `loading`.
pub const LOAD_CALL_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
pub const LOAD_PLAYTEST_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// A navmesh's every tile is built: it serves. Counts the models that would have fed it with a
/// collider.
fn mark_ready(slot: &mut MeshSlot, world: &World) {
    slot.ready = true;
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

/// Builds queued tiles of `mesh` in tile order, at most `budget` work units (at least one tile;
/// `None`: all). Returns the tiles built and the work done.
#[allow(clippy::too_many_arguments)]
fn build_queue(
    mesh: &mut NavMesh,
    queue: &mut BTreeSet<(u32, u32)>,
    buckets: &mut Buckets,
    settings: &MeshSettings,
    sources: &BTreeMap<u64, SourceEntry>,
    world: &World,
    budget: Option<u64>,
    errors: &mut VecDeque<Json>,
) -> (usize, u64) {
    if queue.is_empty() {
        return (0, 0);
    }
    let under = under_of(world, &settings.sources);
    let area_of = |e: &SourceEntry| e.area.as_deref().map_or(0, |n| settings.area_id(n));
    let picked: Vec<(u64, &Source, u8)> = sources.iter().filter_map(|(id, s)| s.pick(&settings.sources, under).map(|src| (*id, src, area_of(s)))).collect();
    let all_marks: Vec<Mark> = sources
        .values()
        .filter_map(|s| s.mark_for(&settings.sources, under).map(|(lo, hi)| Mark { lo, hi, area: area_of(s) }))
        .filter(|m| m.area > 0)
        .collect();
    let mut marks: Vec<Mark> = Vec::new();
    // Bucketing a big source's triangles is work too (4 units a triangle).
    let mut work = 0u64;
    for (id, src, _) in &picked {
        if !src.aligned && src.triangles.len() > BUCKET_MIN && !buckets.contains_key(id) {
            buckets.insert(*id, src.bucket(mesh));
            work += 4 * src.triangles.len() as u64;
        }
    }
    let mut built = 0usize;
    while let Some(&(tx, tz)) = queue.iter().next() {
        let (lo, hi) = mesh.tile_input_bounds(tx, tz);
        let inputs: Vec<Input> = picked
            .iter()
            .filter(|(_, s, _)| s.touches(lo, hi))
            .filter_map(|(id, s, area)| match buckets.get(id) {
                Some(b) => b.get(&(tx, tz)).map(|l| Input { source: s, only: Some(l.as_slice()), area: *area }),
                None => Some(Input { source: s, only: None, area: *area }),
            })
            .collect();
        marks.clear();
        marks.extend(all_marks.iter().filter(|m| m.lo.x <= hi.x && m.hi.x >= lo.x && m.lo.z <= hi.z && m.hi.z >= lo.z));
        let cost = specula_via::tile_work(mesh.config(), &inputs);
        if budget.is_some_and(|b| (built > 0 || work > 0) && work + cost > b) {
            break;
        }
        queue.remove(&(tx, tz));
        work += cost;
        built += 1;
        let stats = mesh.build_tile_from(tx, tz, &inputs, &marks);
        for e in &stats.errors {
            if errors.len() == 16 {
                errors.pop_front();
            }
            errors.push_back(e.to_json());
        }
    }
    (built, work)
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
#[derive(Clone)]
struct MindSlot {
    /// The `props.mind` it was made from.
    props: Json,
    /// The definition entity it reads (`props.mind = {def: "/path"}`), and its version.
    def_from: Option<(u64, u64)>,
    /// The definition's path, also while nothing is there yet.
    def_path: Option<String>,
    mind: Option<Mind>,
    /// The mind's state hash, kept current as it runs (a mind that didn't run is unchanged, so
    /// the per-tick nav hash costs one word per mind, not its whole state).
    hash: u64,
}

fn mind_hash(m: &Mind) -> u64 {
    let mut h = StableHasher::default();
    m.hash_into(&mut h);
    h.finish()
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
    /// Minds the per-tick budget left for the next ticks.
    pub minds_skipped: usize,
    /// Mind work done (node visits, senses; line-of-sight raycasts count 16).
    pub mind_work: u64,
    pub ms: f64,
}

/// Navigation and AI state. (Clone: a save works on a copy, so saving never changes the run.)
#[derive(Clone, Default)]
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
    /// A full look at a newly loaded world (or the first one) is under way: navmeshes made now
    /// are built in full before their first tick.
    rescanning: bool,
    /// A restore waiting for the loaded navmeshes, and its outcome once applied.
    pending_restore: Option<Json>,
    restore_outcome: Option<Result<u64, world::WorldError>>,
    /// How the last restore went (for nav_info).
    pub last_restore: Option<Json>,
    /// The last mind that ran (the budget's round robin), and buffers reused every tick.
    mind_cursor: u64,
    mind_order: Vec<u64>,
    mesh_scratch: Vec<(u64, Option<u64>)>,
    link_scratch: Vec<(u64, u64)>,
    /// Parsed shared definitions by entity, with the version they were parsed at.
    def_cache: BTreeMap<u64, (u64, Arc<Def>)>,
    los_scratch: Vec<([u32; 6], u64, bool)>,
    /// Minds that can't run, and why (kept apart from agents' problems).
    pub mind_problems: BTreeMap<u64, Json>,
    /// Entities whose `props.nav_area` is malformed or names an area a navmesh lacks.
    pub area_problems: BTreeMap<u64, Json>,
    /// `(world generation, draw epoch)` synced up to.
    synced: Option<(u64, u64)>,
    /// Entities navigation can't use, and why.
    pub problems: BTreeMap<u64, Json>,
    pub hashes: VecDeque<(u64, u64)>,
    pub last: Option<TickReport>,
    /// Recent nav_path answers (mesh, path) for the `paths` overlay.
    /// (Mesh, its build counter then, path): the overlay draws only answers its navmesh still gives.
    pub queries: VecDeque<(u64, u64, PathResult)>,
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
    // (Paths and ids resolve without building a key: senses ask this many times a tick.)
    let id = match v {
        Json::Str(s) if s.starts_with('/') => world.find_path(s).ok()?,
        Json::Str(s) => world.resolve(&EntityKey::Name(s.clone())).ok()?,
        _ => v.as_u64().filter(|i| world.get(*i).is_some())?,
    };
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
    /// Work charged this tick (senses, inputs, raycasts).
    work: u64,
    /// Line-of-sight answers this tick by (eye, target point, target id): a definition asking the
    /// same `sees` many times casts once.
    los: Vec<([u32; 6], u64, bool)>,
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
        if self.mesh().is_some_and(|s| !s.ready) {
            return 0;
        }
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
        self.work += 1;
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
                let key = [from.x, from.y, from.z, to.x, to.y, to.z].map(f32::to_bits);
                let tkey = tid.unwrap_or(u64::MAX);
                if let Some((_, _, seen)) = self.los.iter().find(|(k, t, _)| *k == key && *t == tkey) {
                    return *seen;
                }
                self.work += 16;
                let filter = specula_physics::Filter { mask: u64::MAX, triggers: false, exclude: Some(self.id) };
                let (hits, _) = self.physics.space.raycast(from, dir, len, &filter, 4);
                let seen = match hits.iter().find(|h| Some(h.key) != Some(self.id)) {
                    None => true,
                    Some(h) => Some(h.key) == tid || tid.is_some_and(|t| is_ancestor(self.world, t, h.key)),
                };
                if self.los.len() < 64 {
                    self.los.push((key, tkey, seen));
                }
                seen
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
        self.work += 1;
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
                self.rescanning = true;
                None
            }
            None => {
                self.rescanning = true;
                None
            }
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
        self.rescanning = false;
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
                m.buckets.remove(&id);
                for s in [old.as_ref(), new.as_ref()].into_iter().flatten() {
                    if let Some((lo, hi)) = s.bounds_for(&m.settings.sources, under) {
                        m.dirty.extend(m.mesh.tiles_touching(lo, hi));
                    }
                    if let Some(n) = m.next.as_deref_mut() {
                        n.buckets.remove(&id);
                        if let Some((lo, hi)) = s.bounds_for(&n.settings.sources, under_of(world, &n.settings.sources)) {
                            n.queue.extend(n.mesh.tiles_touching(lo, hi));
                        }
                    }
                }
            }
            if let Some(n) = new {
                self.sources.insert(id, n);
            }
            self.check_area(world, id);
        } else if e.is_none() {
            self.area_problems.remove(&id);
            if let Some(old) = self.sources.remove(&id) {
                for m in self.meshes.values_mut() {
                    // (A deleted `under` root: its whole old area is dirty anyway.)
                    let under = under_of(world, &m.settings.sources).map(|u| u.or(old.ancestors.last().copied()));
                    m.buckets.remove(&id);
                    if let Some((lo, hi)) = old.bounds_for(&m.settings.sources, under) {
                        m.dirty.extend(m.mesh.tiles_touching(lo, hi));
                    }
                    if let Some(n) = m.next.as_deref_mut() {
                        n.buckets.remove(&id);
                        let under = under_of(world, &n.settings.sources).map(|u| u.or(old.ancestors.last().copied()));
                        if let Some((lo, hi)) = old.bounds_for(&n.settings.sources, under) {
                            n.queue.extend(n.mesh.tiles_touching(lo, hi));
                        }
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
        // Only the off-mesh links changed: set them in place (nothing to rebuild).
        if let Some(slot) = self.meshes.get_mut(&e.id).filter(|s| s.settings.same_but_links(&settings) && s.next.is_none()) {
            slot.mesh.set_offmesh_links(settings.links.clone());
            slot.settings.links = settings.links;
            slot.props = props;
            slot.crowd.invalidate_paths();
            return;
        }
        let settings_sources = settings.sources.clone();
        let mesh = match NavMesh::new(settings.config) {
            Ok(m) => m,
            Err(err) => {
                self.problems.insert(e.id, err.to_json());
                return;
            }
        };
        let mut mesh = mesh;
        mesh.set_offmesh_links(settings.links.clone());
        let (nx, nz) = mesh.tile_count();
        let mut dirty = BTreeSet::new();
        for tz in 0..nz {
            for tx in 0..nx {
                dirty.insert((tx, tz));
            }
        }
        // New settings for a navmesh that serves: build beside it; it keeps serving until done.
        if let Some(slot) = self.meshes.get_mut(&e.id).filter(|s| s.ready && !self.rescanning) {
            slot.next = Some(Box::new(NextMesh { settings, mesh, queue: dirty, props: props.clone(), buckets: BTreeMap::new() }));
            slot.props = props;
            return;
        }
        self.meshes.insert(
            e.id,
            MeshSlot {
                id: e.id,
                path: world.path_of(e.id),
                settings,
                mesh,
                crowd: Crowd::new(CrowdSettings::default()),
                unbuilt: dirty.clone(),
                dirty,
                props,
                rebuilt_tiles: 0,
                last_rebuild: Vec::new(),
                errors: VecDeque::new(),
                models_skipped: 0,
                under: under_of(world, &settings_sources),
                ready: false,
                next: None,
                buckets: BTreeMap::new(),
                complete_first: self.rescanning,
                load_plan: None,
            },
        );
        self.relink_agents(world);
        self.recheck_areas(world);
    }

    /// Re-checks every entity's `props.nav_area` (a navmesh's areas changed).
    fn recheck_areas(&mut self, world: &World) {
        let named: Vec<u64> = self.sources.iter().filter(|(_, s)| s.area.is_some()).map(|(k, _)| *k).chain(self.area_problems.keys().copied()).collect();
        for id in named {
            self.check_area(world, id);
        }
    }

    /// Checks entity `id`'s `props.nav_area`: well formed, and an area of every navmesh that
    /// reads it (else it would quietly be plain ground).
    fn check_area(&mut self, world: &World, id: u64) {
        let Some(e) = world.get(id) else {
            self.area_problems.remove(&id);
            return;
        };
        let problem = match e.props.get(AREA_KEY) {
            None => None,
            Some(Json::Str(_)) => None,
            Some(o @ Json::Obj(_)) if o.get("area").and_then(Json::as_str).is_some() && o.get("volume").is_none_or(|v| v.as_bool().is_some()) => None,
            Some(_) => Some(Json::obj([
                ("code", Json::from("invalid_nav_area")),
                ("message", Json::from("props.nav_area must be an area name (\"water\") or {area: name, volume: true|false}; it counts as plain ground until fixed")),
            ])),
        };
        let problem = problem.or_else(|| {
            let entry = self.sources.get(&id)?;
            let name = entry.area.as_deref()?;
            let missing: Vec<Json> = self
                .meshes
                .values()
                .filter(|m| entry.passes(&m.settings.sources, under_of(world, &m.settings.sources)) && m.settings.area_id(name) == 0)
                .take(8)
                .map(|m| Json::from(m.path.as_str()))
                .collect();
            (!missing.is_empty()).then(|| {
                Json::obj([
                    ("code", Json::from("unknown_area")),
                    ("message", Json::from(format!("props.nav_area '{name}' is not an area of these navmeshes (nav_build areas): it counts as plain ground there"))),
                    ("navmeshes", Json::Arr(missing)),
                ])
            })
        });
        match problem {
            Some(p) => {
                self.area_problems.insert(id, p);
            }
            None => {
                self.area_problems.remove(&id);
            }
        }
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
            slot.crowd.set_filter(id, slot.settings.filter(&spec.area_costs, &spec.avoid_areas, &spec.avoid_flags));
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
        let live = &self.def_ids;
        self.def_cache.retain(|k, _| live.contains(k));
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
                        // (A definition already parsed at this version isn't copied again: a
                        // thousand minds sharing a 16 KB definition would copy 16 MB.)
                        Some(_) if self.def_cache.get(&d).is_some_and(|(cv, _)| *cv == v) => (Json::Null, Some((d, v))),
                        Some(dj) => (dj.clone(), Some((d, v))),
                        None => {
                            self.forget_mind_goals(id);
                            self.minds.insert(id, MindSlot { props: version, def_from: Some((d, v)), def_path, mind: None, hash: 0 });
                            self.mind_problems.insert(id, Json::from(format!("'{p}' has no props.{DEF_KEY}")));
                            return;
                        }
                    }
                }
                Err(_) => {
                    self.forget_mind_goals(id);
                    self.minds.insert(id, MindSlot { props: version, def_from: None, def_path, mind: None, hash: 0 });
                    self.mind_problems.insert(id, Json::from(format!("mind definition '{p}' doesn't exist (it loads when an entity with props.{DEF_KEY} appears there)")));
                    return;
                }
            },
            Json::Obj(_) => (def_json.clone(), None),
            _ => {
                self.forget_mind_goals(id);
                self.minds.insert(id, MindSlot { props: version, def_from: None, def_path: None, mind: None, hash: 0 });
                self.mind_problems.insert(id, Json::from("props.mind needs 'def': a definition object or the path of an entity with props.mind_def"));
                return;
            }
        };
        if self.minds.len() >= MAX_MINDS && !self.minds.contains_key(&id) {
            self.forget_mind_goals(id);
            self.mind_problems.insert(id, Json::from(format!("a world runs at most {MAX_MINDS} minds")));
            return;
        }
        // A shared definition is parsed once per version of its entity.
        let cached = from.and_then(|f| self.def_cache.get(&f.0).filter(|(v, _)| *v == f.1).map(|(_, d)| d.clone()));
        let parsed = match cached {
            Some(d) => Ok(d),
            None => {
                let p = Def::parse(&def, self.vocab()).map(Arc::new);
                if let (Some((d, v)), Ok(p)) = (from, &p) {
                    self.def_cache.insert(d, (v, p.clone()));
                }
                p
            }
        };
        match parsed {
            Ok(d) => {
                let bb: mens::run::Blackboard = match props.get("blackboard") {
                    Some(Json::Obj(pairs)) => pairs.iter().take(mens::run::MAX_KEYS).cloned().collect(),
                    _ => BTreeMap::new(),
                };
                // A changed definition keeps nothing; a changed blackboard alone keeps the run.
                let keep = self.minds.get(&id).and_then(|m| m.mind.as_ref()).filter(|m| Arc::ptr_eq(&m.def, &d) || *m.def == *d).cloned();
                let mind = match keep {
                    Some(mut m) => {
                        for (k, v) in bb {
                            m.bb.insert(k, v);
                        }
                        m
                    }
                    None => {
                        self.forget_mind_goals(id);
                        Mind::new(d, bb, 30)
                    }
                };
                let hash = mind_hash(&mind);
                self.minds.insert(id, MindSlot { props: version, def_from: from, def_path, mind: Some(mind), hash });
                self.mind_problems.remove(&id);
            }
            Err(diags) => {
                self.forget_mind_goals(id);
                self.minds.insert(id, MindSlot { props: version, def_from: from, def_path, mind: None, hash: 0 });
                self.mind_problems.insert(id, Json::obj([("invalid_mind", Json::arr(diags.iter().map(Diagnostic::to_json)))]));
            }
        }
    }

    /// Builds a navmesh's queued work within `budget` work units (all with `None`): the new
    /// settings' navmesh first (swapped in when complete), else dirty tiles of the serving one.
    /// Returns the tiles built.
    pub fn rebuild(&mut self, mesh_id: u64, budget: Option<u64>, world: &World) -> usize {
        let Some(slot) = self.meshes.get_mut(&mesh_id) else { return 0 };
        let mut built = 0;
        let mut spent = 0u64;
        if let Some(n) = slot.next.as_deref_mut() {
            let (b, w) = build_queue(&mut n.mesh, &mut n.queue, &mut n.buckets, &n.settings, &self.sources, world, budget, &mut slot.errors);
            built += b;
            spent += w;
            if n.queue.is_empty() {
                let n = *slot.next.take().expect("present");
                slot.settings = n.settings;
                slot.mesh = n.mesh;
                slot.props = n.props;
                slot.buckets = n.buckets;
                slot.dirty.clear();
                slot.unbuilt.clear();
                slot.under = under_of(world, &slot.settings.sources);
                // Polygon refs of the old grid mean nothing now: the crowd starts over.
                slot.crowd = Crowd::new(CrowdSettings::default());
                self.relink_agents(world);
                self.recheck_areas(world);
                let Some(slot) = self.meshes.get_mut(&mesh_id) else { return built };
                slot.rebuilt_tiles += built as u64;
                return built;
            }
            if budget.is_some_and(|b| spent >= b) {
                slot.rebuilt_tiles += built as u64;
                return built;
            }
        }
        let rest = budget.map(|b| b.saturating_sub(spent).max(1));
        let (b, _) = build_queue(&mut slot.mesh, &mut slot.dirty, &mut slot.buckets, &slot.settings, &self.sources, world, rest, &mut slot.errors);
        built += b;
        let dirty = &slot.dirty;
        slot.unbuilt.retain(|t| dirty.contains(t));
        if slot.unbuilt.is_empty() && !slot.ready {
            mark_ready(slot, world);
        }
        slot.rebuilt_tiles += built as u64;
        built
    }

    /// One fixed tick: sync, rebuild dirty tiles, minds, crowds, write-back. The caller records
    /// changes as a snapshot and journals them.
    pub fn tick(&mut self, world: &mut dyn WorldWrite, physics: &mut Physics, generation: u64, tick: u64, dt: f32, hz: u32) {
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
                    slot.buckets.clear();
                    let (nx, nz) = slot.mesh.tile_count();
                    for tz in 0..nz {
                        for tx in 0..nx {
                            slot.dirty.insert((tx, tz));
                        }
                    }
                    if let Some(n) = slot.next.as_deref_mut() {
                        n.buckets.clear();
                        let (nx, nz) = n.mesh.tile_count();
                        for tz in 0..nz {
                            for tx in 0..nx {
                                n.queue.insert((tx, tz));
                            }
                        }
                    }
                }
            }
        }
        // Navmeshes a world load brought are complete before their first tick (the run that saved
        // the world had them complete); others build within their per-tick work.
        let mut ids = std::mem::take(&mut self.mesh_scratch);
        ids.clear();
        ids.extend(self.meshes.iter_mut().map(|(k, m)| {
            let all = std::mem::take(&mut m.complete_first);
            (*k, if all { None } else { Some(m.settings.tiles_per_tick as u64 * TILE_WORK) })
        }));
        for &(mid, budget) in &ids {
            report.tiles_rebuilt += self.rebuild(mid, budget, world);
        }
        self.mesh_scratch = ids;
        // Senses cast rays into the physics space: keep it in step with the world even when the
        // simulation is off (a disabled physics doesn't sync on its own).
        if !self.minds.is_empty() {
            physics.sync(world, generation);
        }
        let physics = &*physics;
        // Minds decide (their actions set agents' goals), within a per-tick work budget: round
        // robin by id from the one after the last that ran; at least one runs.
        let mut minds = std::mem::take(&mut self.minds);
        let mut order = std::mem::take(&mut self.mind_order);
        order.clear();
        order.extend(minds.iter().filter(|(_, s)| s.mind.is_some()).map(|(k, _)| *k));
        let split = order.partition_point(|k| *k <= self.mind_cursor);
        order.rotate_left(split);
        let mut los = std::mem::take(&mut self.los_scratch);
        for (n, id) in order.iter().enumerate() {
            if n > 0 && report.mind_work >= MIND_WORK_PER_TICK {
                report.minds_skipped = order.len() - n;
                break;
            }
            let Some(slot) = minds.get_mut(id) else { continue };
            let Some(mind) = slot.mind.as_mut() else { continue };
            mind.hz = hz;
            los.clear();
            let mut host = EngineHost { world: &*world, physics, meshes: &self.meshes, link: self.agents.get_mut(id), id: *id, node: 0, work: 0, los };
            mind.tick(&mut host, tick);
            report.mind_work += host.work + u64::from(mind.visits);
            slot.hash = mind_hash(mind);
            los = host.los;
            report.minds += 1;
            self.mind_cursor = *id;
        }
        self.los_scratch = los;
        self.mind_order = order;
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
            if slot.crowd.is_empty() || !slot.ready {
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
        let mut links = std::mem::take(&mut self.link_scratch);
        links.clear();
        links.extend(self.agents.iter().filter_map(|(id, l)| l.mesh.map(|m| (*id, m))));
        for &(id, mid) in &links {
            let Some(a) = self.meshes.get(&mid).and_then(|s| s.crowd.agent(id)) else { continue };
            let (pos, vel, moving) = (a.pos, a.vel, a.desired != Vec3::ZERO || a.vel != Vec3::ZERO);
            let crossing = a.crossing.is_some();
            let link = self.agents.get_mut(&id).expect("listed");
            let Some(e) = world.get(id) else { continue };
            let speed2 = vel.x * vel.x + vel.z * vel.z;
            let yaw = link.face.take().or_else(|| (speed2 > 0.0025).then(|| dmath::atan2(vel.x, vel.z).to_degrees()));
            let mut change = UpdateSpec::default();
            if let Some(y) = yaw {
                // The world heading in the parent's frame (exact for parents turned about Y).
                let local = match e.parent.and_then(|p| world.get(p)) {
                    Some(parent) => {
                        let inv = parent.model().inverse().unwrap_or(Mat4::IDENTITY);
                        let (s, c) = dmath::sin_cos(y.to_radians());
                        let d = inv.transform_point(Vec3::new(s, 0.0, c)) - inv.transform_point(Vec3::ZERO);
                        (d.x * d.x + d.z * d.z > 1.0e-12).then(|| dmath::atan2(d.x, d.z).to_degrees())
                    }
                    None => Some(y),
                };
                if let Some(y) = local {
                    // Turn about Y keeping any tilt. The Euler angles can't be edited in place: the
                    // physics write-back gives a yaw past 90 degrees as (180, 180 - yaw, 180), and
                    // changing only its Y mirrors the heading.
                    let r = e.rotation_deg;
                    let q = Quat::from_euler_xyz_deg(r.x, r.y, r.z);
                    let f = q.rotate(Vec3::new(0.0, 0.0, 1.0));
                    let now = dmath::atan2(f.x, f.z).to_degrees();
                    let mut diff = (y - now) % 360.0;
                    if diff > 180.0 {
                        diff -= 360.0;
                    } else if diff < -180.0 {
                        diff += 360.0;
                    }
                    if diff.abs() > 1.0e-3 {
                        let turn = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), diff.to_radians());
                        change.rotation_deg = Some(specula_physics::math::quat_to_euler_deg((turn * q).normalize()));
                    }
                }
            }
            if link.external && !crossing {
                if let Some(b) = e.body {
                    let v = Vec3::new(vel.x, b.velocity.y, vel.z);
                    if v != b.velocity {
                        change.body = Some(Some(physics::Body { velocity: v, sleeping: false, ..b }));
                    }
                }
            } else if moving || link.written.is_none() || crossing {
                // (A physics character crossing a link is carried: no velocity of its own.)
                if let Some(b) = e.body.filter(|b| link.external && b.velocity != Vec3::ZERO) {
                    change.body = Some(Some(physics::Body { velocity: Vec3::ZERO, sleeping: false, ..b }));
                }
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
        self.link_scratch = links;
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
            // (The mesh hash covers its off-mesh links.)
            h.write_u64(m.mesh.hash());
            h.write_u64(m.crowd.hash());
            h.write_u64(m.dirty.len() as u64);
            h.write_bool(m.ready);
            if let Some(n) = &m.next {
                h.write_u64(n.mesh.hash());
                h.write_u64(n.queue.len() as u64);
            }
        }
        h.write_u64(self.minds.len() as u64);
        h.write_u64(self.mind_cursor);
        for (id, m) in &self.minds {
            h.write_u64(*id);
            if m.mind.is_some() {
                h.write_u64(m.hash);
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

    /// Navigation's hidden state for a save game, exactly: each navmesh's crowd (with the mesh
    /// hash its polygon refs belong to), each mind's running state and blackboard, the agents'
    /// runtime goals, and the nav state hash. Everything else (navmeshes, settings, agents,
    /// definitions) is world data and comes back with the world. Pair with
    /// [`Nav::restore_state`] after loading the same world at the same tick.
    pub fn save_state(&self) -> Json {
        let f = |v: f32| Json::from(v.to_bits());
        let v = |p: Vec3| Json::arr([f(p.x), f(p.y), f(p.z)]);
        let meshes = self.meshes.iter().map(|(id, m)| {
            Json::obj([
                ("id", Json::from(*id)),
                ("path", Json::from(m.path.as_str())),
                ("ready", Json::from(m.ready)),
                ("mesh_hash", Json::from(format!("{:016x}", m.mesh.hash()))),
                // The build state: tiles waiting for a rebuild, tiles never built, and the
                // polygons of built tiles whose geometry has changed since (the world can't
                // rebuild those as they are).
                ("dirty", tiles_json(&m.dirty)),
                ("unbuilt", tiles_json(&m.unbuilt)),
                (
                    "stale",
                    Json::arr(m.dirty.difference(&m.unbuilt).filter_map(|&(tx, tz)| m.mesh.tile_state(tx, tz).map(|s| Json::obj([("tile", Json::arr([Json::from(tx), Json::from(tz)])), ("state", s)])))),
                ),
                ("counter", Json::from(m.mesh.build_counters().0)),
                ("rebuilt", Json::arr(m.mesh.build_counters().1.iter().map(|r| Json::from(*r)))),
                ("crowd", m.crowd.save()),
            ])
        });
        let minds = self.minds.iter().filter_map(|(id, m)| m.mind.as_ref().map(|mind| Json::obj([("id", Json::from(*id)), ("state", mind.save())])));
        let agents = self.agents.iter().map(|(id, l)| {
            Json::obj([
                ("id", Json::from(*id)),
                (
                    "mind_goal",
                    match &l.mind_goal {
                        Some(Goal::Point(p)) => Json::obj([("point", v(*p))]),
                        Some(Goal::Entity(e)) => Json::obj([("entity", Json::from(*e))]),
                        None => Json::Null,
                    },
                ),
                ("face", l.face.map_or(Json::Null, f)),
                ("patrol_index", Json::from(l.patrol_index)),
                ("wander_count", Json::from(l.wander_count)),
                ("unreachable_ticks", Json::from(l.unreachable_ticks)),
                ("goal_owner", l.goal_owner.map_or(Json::Null, Json::from)),
                ("written", l.written.map_or(Json::Null, v)),
            ])
        });
        Json::obj([
            ("version", Json::from(SAVE_VERSION)),
            ("hash", Json::from(format!("{:016x}", self.hash()))),
            ("mind_cursor", Json::from(self.mind_cursor)),
            ("meshes", Json::arr(meshes)),
            ("minds", Json::arr(minds)),
            ("agents", Json::arr(agents)),
        ])
    }

    /// Restores [`Nav::save_state`] for `world` (the world the save was made with, loaded):
    /// re-reads the world, and once every navmesh is built (the engine builds them in slices
    /// with the simulation clock held, see [`Nav::loading`]) puts back crowds, minds and goals
    /// and checks the nav state hash is the saved one. On any mismatch navigation starts fresh
    /// from the world and the outcome says what differed. Returns the restored hash when
    /// nothing needed building, else `None`: [`Nav::take_restore_outcome`] has it once loaded.
    pub fn restore_state(&mut self, world: &World, generation: u64, j: &Json) -> Result<Option<u64>, world::WorldError> {
        let bad = |what: String| world::WorldError::new("invalid_save", format!("navigation's saved state: {what}"));
        if j.get("version").and_then(Json::as_u64) != Some(SAVE_VERSION) {
            return Err(bad(format!("version must be {SAVE_VERSION}")));
        }
        self.reset();
        self.sync(world, generation);
        // The load builds only the tiles that were built and up to date in the saving run (the
        // others come from the save, or stay unbuilt).
        for m in j.get("meshes").and_then(Json::as_array).ok_or_else(|| bad("no meshes".into()))? {
            let Some(slot) = m.get("id").and_then(Json::as_u64).and_then(|id| self.meshes.get_mut(&id)) else { continue };
            let (nx, nz) = slot.mesh.tile_count();
            let dirty = tiles_of(m.get("dirty"), nx, nz).ok_or_else(|| bad(format!("navmesh '{}': its dirty tiles don't fit it", slot.path)))?;
            let clean: BTreeSet<(u32, u32)> = (0..nz).flat_map(|z| (0..nx).map(move |x| (x, z))).filter(|t| !dirty.contains(t)).collect();
            slot.load_plan = Some(clean);
        }
        self.pending_restore = Some(j.clone());
        if self.meshes.values().any(|m| m.complete_first) {
            return Ok(None);
        }
        self.apply_restore(world);
        self.take_restore_outcome().expect("just applied").map(Some)
    }

    /// Whether navigation can be saved exactly now. Its build state is saved as data (dirty and
    /// unbuilt tiles, and the polygons of tiles built from geometry that has changed since), so
    /// edits and first builds in progress are fine; only a switch to new settings (an old and a
    /// new navmesh at once) or a load still being prepared are refused: `nav_building`, retry.
    /// Navigation's state for a save of `world` now: a copy is synced to the world (marking what
    /// the next tick's sync would mark) and saved; the live state isn't touched, so a save is
    /// never an input to the run, whatever happens before the next tick.
    pub fn save_section(&self, world: &World, generation: u64) -> Result<Vec<u8>, world::WorldError> {
        let mut copy = self.clone();
        copy.sync(world, generation);
        copy.check_saveable()?;
        encode_section(&copy.save_state())
    }

    /// The nav hash a save of `world` now records (the synced copy's, as [`Nav::save_section`]).
    pub fn save_hash(&self, world: &World, generation: u64) -> u64 {
        let mut copy = self.clone();
        copy.sync(world, generation);
        copy.hash()
    }

    pub fn check_saveable(&self) -> Result<(), world::WorldError> {
        // (Nothing is built or changed for a save: saving is never an input to the run.)
        if let Some(m) = self.meshes.values().find(|m| m.next.is_some()) {
            return Err(world::WorldError::new("nav_building", format!("navmesh '{}' is switching to new settings (built beside the old ones): save once it has (nav_info shows progress)", m.path))
                .with("retry_after_ms", 500));
        }
        if self.loading() {
            return Err(world::WorldError::new("nav_building", "a loaded world's navigation is still being prepared: save once it is").with("retry_after_ms", 200));
        }
        Ok(())
    }

    /// The outcome of a deferred [`Nav::restore_state`], once applied.
    pub fn take_restore_outcome(&mut self) -> Option<Result<u64, world::WorldError>> {
        self.restore_outcome.take()
    }

    /// Whether a load is still being prepared: navmeshes the world came with are being built
    /// (or a restore waits for them). The engine holds the simulation clock meanwhile.
    pub fn loading(&self) -> bool {
        self.pending_restore.is_some() || self.meshes.values().any(|m| m.complete_first)
    }

    /// One slice of a load: builds the loaded navmeshes within `budget` work units (`None`: all
    /// of it), then applies a pending restore. The navmesh that results is the same however
    /// it is sliced, and no tick runs meanwhile, so the loaded world behaves like the run that
    /// saved it. Returns whether the load is done.
    pub fn load_slice(&mut self, world: &World, generation: u64, budget: Option<u64>) -> bool {
        self.sync(world, generation);
        // Only the tiles the load brought (never built yet): edits made meanwhile are left to the
        // ticks' rebuilds, so a busy swarm can't hold the clock.
        let ids: Vec<u64> = self.meshes.iter().filter(|(_, m)| m.complete_first).map(|(k, _)| *k).collect();
        for id in ids {
            let Some(slot) = self.meshes.get_mut(&id) else { continue };
            // A restore's plan (the tiles that were built and clean), or every tile.
            let planned = slot.load_plan.is_some();
            let mut queue = match slot.load_plan.take() {
                Some(p) => p,
                None => std::mem::take(&mut slot.unbuilt),
            };
            let before = queue.clone();
            let (b, _) = build_queue(&mut slot.mesh, &mut queue, &mut slot.buckets, &slot.settings, &self.sources, world, budget, &mut slot.errors);
            for t in before.difference(&queue) {
                slot.dirty.remove(t);
                slot.unbuilt.remove(t);
            }
            slot.rebuilt_tiles += b as u64;
            let done = queue.is_empty();
            if planned {
                slot.load_plan = Some(queue);
            } else {
                slot.unbuilt = queue;
            }
            if done {
                if planned {
                    // The restore puts the rest back (stale tiles, dirty and unbuilt sets).
                    slot.load_plan = None;
                } else if !slot.ready {
                    mark_ready(slot, world);
                }
                slot.complete_first = false;
            } else if budget.is_some() {
                return false;
            }
        }
        if self.meshes.values().any(|m| m.complete_first) {
            return false;
        }
        if self.pending_restore.is_some() {
            self.apply_restore(world);
        }
        true
    }

    fn apply_restore(&mut self, world: &World) {
        let Some(j) = self.pending_restore.take() else { return };
        let r = self.try_restore(world, &j);
        if r.is_err() {
            // Start fresh from the world (its navmeshes are built: no second load). Tiles a load
            // left unbuilt (the save's dirty ones) or a failed restore put back from the save are
            // rebuilt from the world over the ticks: every tile is marked dirty.
            for m in self.meshes.values_mut() {
                m.crowd = Crowd::new(CrowdSettings::default());
                let (nx, nz) = m.mesh.tile_count();
                let all: BTreeSet<(u32, u32)> = (0..nz).flat_map(|z| (0..nx).map(move |x| (x, z))).collect();
                m.unbuilt = all.iter().copied().filter(|&(x, z)| m.mesh.tile(m.mesh.tile_index(x, z)).is_none()).collect();
                m.dirty = all;
                m.ready = m.unbuilt.is_empty();
            }
            let minds: Vec<u64> = self.minds.keys().copied().collect();
            for id in minds {
                self.minds.remove(&id);
            }
            let agents: Vec<u64> = self.agents.keys().copied().collect();
            for id in agents {
                self.agents.remove(&id);
            }
            self.mind_cursor = 0;
            self.resync_actors(world);
        }
        self.last_restore = Some(match &r {
            Ok(h) => Json::obj([("restored", Json::from(true)), ("hash", Json::from(format!("{h:016x}")))]),
            Err(e) => Json::obj([("restored", Json::from(false)), ("error", Json::from(e.code)), ("message", Json::from(e.message.as_str()))]),
        });
        self.restore_outcome = Some(r);
    }

    /// Re-reads every agent and mind from the world (after a failed restore).
    fn resync_actors(&mut self, world: &World) {
        let ids: Vec<u64> = world.persistent_entities().filter(|e| e.props.get(AGENT_KEY).is_some() || e.props.get(MIND_KEY).is_some()).map(|e| e.id).collect();
        for id in ids {
            self.upsert(world, id);
        }
    }

    fn try_restore(&mut self, world: &World, j: &Json) -> Result<u64, world::WorldError> {
        let bad = |what: String| world::WorldError::new("invalid_save", format!("navigation's saved state: {what}"));
        let hex = |k: &str, o: &Json| o.get(k).and_then(Json::as_str).and_then(|s| u64::from_str_radix(s, 16).ok());
        let want = hex("hash", j).ok_or_else(|| bad("no hash".into()))?;
        for m in j.get("meshes").and_then(Json::as_array).ok_or_else(|| bad("no meshes".into()))? {
            let id = m.get("id").and_then(Json::as_u64).ok_or_else(|| bad("a mesh without id".into()))?;
            let slot = self.meshes.get_mut(&id).ok_or_else(|| {
                world::WorldError::new("state_mismatch", format!("the save has navmesh {id}, the world doesn't")).with("mesh", m.get("path").cloned().unwrap_or(Json::Null))
            })?;
            let saved = hex("mesh_hash", m).ok_or_else(|| bad("a mesh without mesh_hash".into()))?;
            // The build state: stale tiles from the save, then the dirty and unbuilt sets.
            let (nx, nz) = slot.mesh.tile_count();
            let stale = m.get("stale").and_then(Json::as_array).ok_or_else(|| bad("a mesh without stale tiles".into()))?;
            if stale.len() > nx as usize * nz as usize {
                return Err(bad("more stale tiles than the navmesh has".into()));
            }
            let dirty = tiles_of(m.get("dirty"), nx, nz).ok_or_else(|| bad("dirty tiles".into()))?;
            let unbuilt = tiles_of(m.get("unbuilt"), nx, nz).ok_or_else(|| bad("unbuilt tiles".into()))?;
            if !unbuilt.is_subset(&dirty) {
                return Err(bad("unbuilt tiles that aren't dirty".into()));
            }
            for s in stale {
                let t = tiles_of(Some(&Json::arr([s.get("tile").cloned().unwrap_or(Json::Null)])), nx, nz).and_then(|t| t.into_iter().next()).ok_or_else(|| bad("a stale tile outside the navmesh".into()))?;
                // A stale tile is built and waiting for a rebuild: dirty, not unbuilt.
                if !dirty.contains(&t) || unbuilt.contains(&t) {
                    return Err(bad("a stale tile that isn't dirty-and-built".into()));
                }
                slot.mesh.restore_tile(t.0, t.1, s.get("state").unwrap_or(&Json::Null)).map_err(|e| nav_error(&e))?;
            }
            slot.dirty = dirty;
            slot.unbuilt = unbuilt;
            match m.get("ready").and_then(Json::as_bool) {
                Some(true) => {
                    if !slot.ready {
                        mark_ready(slot, world);
                    }
                }
                Some(false) => slot.ready = false,
                None => return Err(bad("a mesh without ready".into())),
            }
            if saved != slot.mesh.hash() {
                return Err(world::WorldError::new("state_mismatch", format!("navmesh '{}' builds differently from the saved one (the world or its settings differ)", slot.path))
                    .with("saved", format!("{saved:016x}"))
                    .with("built", format!("{:016x}", slot.mesh.hash())));
            }
            // The build counters agents compare their polygons and corridors against.
            let counter = m.get("counter").and_then(Json::as_u64).ok_or_else(|| bad("a mesh without counter".into()))?;
            let rebuilt: Option<Vec<u64>> = m.get("rebuilt").and_then(Json::as_array).map(|l| l.iter().map(Json::as_u64).collect()).unwrap_or(None);
            if !rebuilt.is_some_and(|r| slot.mesh.restore_build_counters(counter, &r)) {
                return Err(world::WorldError::new("state_mismatch", format!("navmesh '{}': its saved build counters don't fit", slot.path)));
            }
            slot.crowd.restore(m.get("crowd").ok_or_else(|| bad("a mesh without crowd".into()))?).map_err(|e| nav_error(&e))?;
        }
        for m in j.get("minds").and_then(Json::as_array).ok_or_else(|| bad("no minds".into()))? {
            let id = m.get("id").and_then(Json::as_u64).ok_or_else(|| bad("a mind without id".into()))?;
            let slot = self.minds.get_mut(&id).ok_or_else(|| world::WorldError::new("state_mismatch", format!("the save has a mind on entity {id}, the world doesn't")).with("id", id))?;
            let def = slot.mind.as_ref().map(|x| Arc::clone(&x.def)).ok_or_else(|| world::WorldError::new("state_mismatch", format!("entity {id}'s mind can't run in this world")).with("id", id))?;
            let mind = Mind::restore(def, m.get("state").unwrap_or(&Json::Null)).ok_or_else(|| world::WorldError::new("state_mismatch", format!("entity {id}'s saved mind doesn't fit its definition")).with("id", id))?;
            slot.hash = mind_hash(&mind);
            slot.mind = Some(mind);
        }
        self.mind_cursor = j.get("mind_cursor").and_then(Json::as_u64).ok_or_else(|| bad("no mind_cursor".into()))?;
        let fb = |x: &Json| x.as_u64().and_then(|b| u32::try_from(b).ok()).map(f32::from_bits);
        let vb = |x: &Json| -> Option<Vec3> {
            let a = x.as_array()?;
            Some(Vec3::new(fb(a.first()?)?, fb(a.get(1)?)?, fb(a.get(2)?)?))
        };
        for a in j.get("agents").and_then(Json::as_array).ok_or_else(|| bad("no agents".into()))? {
            let id = a.get("id").and_then(Json::as_u64).ok_or_else(|| bad("an agent without id".into()))?;
            let link = self.agents.get_mut(&id).ok_or_else(|| world::WorldError::new("state_mismatch", format!("the save has nav agent {id}, the world doesn't")).with("id", id))?;
            let damaged = || bad(format!("agent {id}"));
            link.mind_goal = match a.get("mind_goal") {
                None | Some(Json::Null) => None,
                Some(g) => match (g.get("point"), g.get("entity")) {
                    (Some(p), _) => Some(Goal::Point(vb(p).ok_or_else(damaged)?)),
                    (_, Some(e)) => Some(Goal::Entity(e.as_u64().ok_or_else(damaged)?)),
                    _ => return Err(damaged()),
                },
            };
            link.face = match a.get("face") {
                None | Some(Json::Null) => None,
                Some(x) => Some(fb(x).ok_or_else(damaged)?),
            };
            link.patrol_index = a.get("patrol_index").and_then(Json::as_u64).ok_or_else(damaged)? as usize;
            link.wander_count = a.get("wander_count").and_then(Json::as_u64).ok_or_else(damaged)?;
            link.unreachable_ticks = a.get("unreachable_ticks").and_then(Json::as_u64).and_then(|v| u32::try_from(v).ok()).ok_or_else(damaged)?;
            link.goal_owner = match a.get("goal_owner") {
                None | Some(Json::Null) => None,
                Some(x) => Some(x.as_u64().ok_or_else(damaged)? as usize),
            };
            link.written = match a.get("written") {
                None | Some(Json::Null) => None,
                Some(x) => Some(vb(x).ok_or_else(damaged)?),
            };
        }
        let got = self.hash();
        if got != want {
            return Err(world::WorldError::new("state_mismatch", "the restored navigation state hashes differently from the saved one")
                .with("saved", format!("{want:016x}"))
                .with("restored", format!("{got:016x}")));
        }
        Ok(got)
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
    /// Navmesh and mind problems, one short line each (a code and the first diagnostic; ai_status
    /// has a mind's full list), within `max_bytes`. Returns the list and how many were left out.
    pub fn problems_json(&self, world: &World, max_bytes: usize) -> (Json, usize) {
        let all: Vec<(&u64, &Json)> = self.problems.iter().chain(self.mind_problems.iter()).chain(self.area_problems.iter()).collect();
        let mut out = Vec::new();
        let mut bytes = 2;
        for (n, (id, p)) in all.iter().enumerate() {
            let short = match p {
                Json::Obj(_) if p.get("invalid_mind").is_some() => {
                    let diags = p.get("invalid_mind").and_then(Json::as_array).unwrap_or(&[]);
                    let mut j = Json::obj([("code", Json::from("invalid_mind")), ("diagnostics", Json::from(diags.len()))]);
                    if let Some(first) = diags.first() {
                        j.set("first", first.clone());
                    }
                    j
                }
                Json::Str(s) => Json::obj([("message", Json::from(s.as_str()))]),
                other => (*other).clone(),
            };
            let item = Json::obj([("id", Json::from(**id)), ("path", Json::from(world.path_of(**id))), ("problem", short)]);
            let len = item.to_string().len() + 1;
            if bytes + len > max_bytes {
                return (Json::Arr(out), all.len() - n);
            }
            bytes += len;
            out.push(item);
        }
        (Json::Arr(out), 0)
    }

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
                j.set("ready", Json::from(m.ready));
                if !m.ready {
                    let (nx, nz) = m.mesh.tile_count();
                    let total = nx as usize * nz as usize;
                    j.set("building", Json::obj([("tiles_built", Json::from(total - m.unbuilt.len())), ("tiles", Json::from(total))]));
                }
                if let Some(n) = &m.next {
                    let (nx, nz) = n.mesh.tile_count();
                    let total = nx as usize * nz as usize;
                    j.set("building_new_settings", Json::obj([("tiles_built", Json::from(total - n.queue.len())), ("tiles", Json::from(total))]));
                }
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
            ("problems", Json::from(self.problems.len() + self.mind_problems.len() + self.area_problems.len())),
            // A loaded world's navmeshes building, the simulation clock held until they are.
            ("loading", Json::from(self.loading())),
            ("last_restore", self.last_restore.clone().unwrap_or(Json::Null)),
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
                        ("mind_work", Json::from(r.mind_work)),
                        ("minds_skipped", Json::from(r.minds_skipped)),
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
        let counter = self.meshes.get(&mesh).map_or(0, |m| m.mesh.counter());
        self.queries.push_back((mesh, counter, p));
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
/// A navmesh that serves queries: every tile built once (a first build in progress is a
/// structured `navmesh_building` error with its progress, not "unreachable").
pub fn ready_mesh(nav: &Nav, mid: u64) -> Result<&MeshSlot, crate::world::WorldError> {
    let slot = nav.meshes.get(&mid).ok_or_else(|| crate::world::WorldError::new("no_navmesh", "that navmesh is gone"))?;
    if !slot.ready {
        let (nx, nz) = slot.mesh.tile_count();
        let total = nx as usize * nz as usize;
        return Err(crate::world::WorldError::new("navmesh_building", format!("'{}' is still being built ({} of {total} tiles): it finishes over the next ticks", slot.path, total - slot.unbuilt.len()))
            .with("tiles_built", Json::from(total - slot.unbuilt.len()))
            .with("tiles", Json::from(total)));
    }
    Ok(slot)
}

pub fn build_now(nav: &mut Nav, world: &World, generation: u64, mesh: u64) -> (usize, f64) {
    build_within(nav, world, generation, mesh, None)
}

/// Builds a navmesh's queued work now, within `budget` work units (`None`: all). Returns (tiles
/// built, milliseconds).
pub fn build_within(nav: &mut Nav, world: &World, generation: u64, mesh: u64, budget: Option<u64>) -> (usize, f64) {
    nav.sync(world, generation);
    if let Some(m) = nav.meshes.get_mut(&mesh) {
        m.complete_first = false;
    }
    let started = std::time::Instant::now();
    let n = nav.rebuild(mesh, budget, world);
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
            areas: Vec::new(),
            links: Vec::new(),
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
        let mut physics = Physics::default();
        for t in 0..ticks {
            nav.tick(w, &mut physics, 0, t, 1.0 / 30.0, 30);
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
        // (It slows to a stop: 8 m/sÂ² from 2 m/s takes a quarter second.)
        run(&mut nav, &mut w, 20);
        let p0 = w.get(a).unwrap().world_position();
        run(&mut nav, &mut w, 60);
        let p1 = w.get(a).unwrap().world_position();
        assert!(horizontal(p0, p1) < 0.05, "stands still once its mind only waits: {p0:?} -> {p1:?}");
    }

    #[test]
    fn a_mind_whose_definition_goes_stops() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("brain".into());
        s.props = Json::obj([(DEF_KEY, Json::parse(r#"{"do": "move_to", "target": [8, 0, 8]}"#).unwrap())]);
        let b = w.spawn(s, "t").unwrap().id;
        let a = walker(&mut w, "a", Vec3::new(-7.5, 0.5, -7.5), vec![(MIND_KEY, Json::obj([("def", Json::from("/brain"))]))]);
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        run(&mut nav, &mut w, 10);
        w.delete(&EntityKey::Id(b), None).unwrap();
        run(&mut nav, &mut w, 20);
        let p0 = w.get(a).unwrap().world_position();
        run(&mut nav, &mut w, 60);
        assert!(horizontal(p0, w.get(a).unwrap().world_position()) < 0.05, "stops once its mind can't run");
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
    fn hostile_navmesh_settings_are_problems_not_panics() {
        for bad in [r#"{"cell_size": 0}"#, r#"{"agent_radius": 1e30}"#, r#"{"tile_cells": 0}"#, r#"{"cell_size": 1e-30, "agent_radius": 10}"#] {
            assert!(MeshSettings::from_json(&Json::parse(bad).unwrap()).is_err(), "{bad}");
        }
        let mut w = World::default();
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("nm".into());
        s.props = Json::obj([(MESH_KEY, Json::parse(r#"{"agent_radius": 1e30}"#).unwrap())]);
        let id = w.spawn(s, "t").unwrap().id;
        let mut nav = Nav::default();
        run(&mut nav, &mut w, 2);
        assert!(nav.problems.contains_key(&id) && nav.meshes.is_empty());
    }

    #[test]
    fn minds_share_a_per_tick_budget_round_robin() {
        let mut w = World::default();
        room(&mut w);
        cube(&mut w, "t", Vec3::new(5.0, 0.5, 5.0), Vec3::ONE);
        // A heavy definition: 290 sees that all fail (out of range), so every one is visited.
        let sel = format!(r#"{{"selector": [{}]}}"#, vec![r#"{"if": "sees", "target": "/t", "range": 0.1, "fov_deg": 360}"#; 290].join(","));
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("brain".into());
        s.props = Json::obj([(DEF_KEY, Json::parse(&sel).unwrap())]);
        w.spawn(s, "t").unwrap();
        for i in 0..300 {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(format!("m{i}"));
            s.position = Vec3::new(-5.0 + (i % 30) as f32 * 0.3, 0.5, -5.0 + (i / 30) as f32 * 0.3);
            s.props = Json::obj([(MIND_KEY, Json::obj([("def", Json::from("/brain"))]))]);
            w.spawn(s, "t").unwrap();
        }
        let mut nav = Nav::default();
        let mut physics = Physics::default();
        let started = std::time::Instant::now();
        nav.tick(&mut w, &mut physics, 0, 0, 1.0 / 30.0, 30);
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;
        // A timing, printed (it was about 190 ms when each mind copied the shared definition);
        // the counted work is asserted below.
        eprintln!("first tick with 300 new minds sharing a 290-node definition: {first_ms:.1} ms");
        let r = nav.last.clone().unwrap();
        assert!(r.minds < 300 && r.minds_skipped == 300 - r.minds, "{r:?}");
        // (At most one mind past the budget.)
        assert!(r.mind_work < MIND_WORK_PER_TICK + 600, "{r:?}");
        // Over a few ticks every mind gets its turn.
        let mut ran = r.minds;
        for t in 1..60 {
            nav.tick(&mut w, &mut physics, 0, t, 1.0 / 30.0, 30);
            ran += nav.last.as_ref().unwrap().minds;
        }
        assert!(ran >= 300, "{ran}");
    }

    #[test]
    fn new_settings_build_beside_the_serving_navmesh_and_loads_build_in_full() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        assert!(nav.meshes[&m].ready);
        let probe = |nav: &Nav| nav.meshes[&m].mesh.find_path(Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, 5.0), Vec3::new(1.0, 2.0, 1.0), 4096).is_some_and(|p| p.complete());
        assert!(probe(&nav));
        // New settings (a finer grid, one tile of work per tick): the old navmesh keeps serving.
        let mut s = MeshSettings::from_json(w.get(m).unwrap().props.get(MESH_KEY).unwrap()).unwrap();
        s.config.cell_size = 0.1;
        s.tiles_per_tick = 1;
        w.update(&EntityKey::Id(m), UpdateSpec { props: Some(Json::obj([(MESH_KEY, s.to_json())])), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 1);
        assert!(nav.meshes[&m].next.is_some() && probe(&nav), "still serving while the new one builds");
        let mut ticks = 1;
        while nav.meshes[&m].next.is_some() && ticks < 400 {
            run(&mut nav, &mut w, 1);
            ticks += 1;
            assert!(probe(&nav));
        }
        assert!(nav.meshes[&m].next.is_none() && nav.meshes[&m].settings.config.cell_size == 0.1, "swapped in after {ticks} ticks");
        // A world load (a new generation): the navmesh is complete at the first tick.
        let mut loaded = Nav::default();
        let mut physics = Physics::default();
        loaded.tick(&mut w, &mut physics, 7, 0, 1.0 / 30.0, 30);
        assert!(loaded.meshes[&m].ready && probe(&loaded));
        assert_eq!(loaded.meshes[&m].mesh.hash(), nav.meshes[&m].mesh.hash());
    }

    #[test]
    fn a_big_heightfield_builds_in_slices_reading_only_its_own_triangles() {
        let mut w = World::default();
        let n = 65u32;
        let heights: Vec<f32> = (0..n * n).map(|k| 0.15 * (((k % n) * 7 + (k / n) * 13) % 5) as f32 / 5.0).collect();
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("terrain".into());
        s.collider = Some(crate::physics::Collider {
            shape: crate::physics::ColliderShape::Heightfield { columns: n, rows: n, cell: [1.0, 1.0], heights: Arc::new(heights) },
            ..Default::default()
        });
        w.spawn(s, "t").unwrap();
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("nav".into());
        w.spawn(g, "t").unwrap();
        let settings = MeshSettings {
            config: specula_via::Config { bounds_min: Vec3::new(-33.0, -2.0, -33.0), bounds_max: Vec3::new(33.0, 4.0, 33.0), ..Default::default() },
            sources: Sources::default(),
            tiles_per_tick: 2,
            areas: Vec::new(),
            links: Vec::new(),
        };
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("default".into());
        s.parent = Some(EntityKey::Path("/nav".into()));
        s.props = Json::obj([(MESH_KEY, settings.to_json())]);
        let m = w.spawn(s, "t").unwrap().id;
        let mut nav = Nav::default();
        nav.sync(&w, 0);
        // One call does a slice; queries say "building" until the ticks finish it.
        let (first, _) = build_within(&mut nav, &w, 0, m, Some(40_000));
        let total = { let (nx, nz) = nav.meshes[&m].mesh.tile_count(); (nx * nz) as usize };
        assert!(first > 0 && first < total, "{first} of {total}");
        assert_eq!(ready_mesh(&nav, m).err().map(|e| e.code), Some("navmesh_building"));
        // The heightfield is bucketed: a tile reads its own few hundred triangles of 8,192.
        let b = &nav.meshes[&m].buckets;
        let most = b.values().flat_map(|t| t.values()).map(Vec::len).max().unwrap();
        assert!(most < 1_000, "{most}");
        let mut physics = Physics::default();
        let mut ticks = 0;
        while !nav.meshes[&m].ready && ticks < 1_000 {
            nav.tick(&mut w, &mut physics, 0, ticks, 1.0 / 30.0, 30);
            ticks += 1;
        }
        assert!(ready_mesh(&nav, m).is_ok() && ticks > 1, "{ticks} ticks");
        // The slices make the same navmesh as one full build.
        let mut whole = Nav::default();
        whole.sync(&w, 0);
        build_now(&mut whole, &w, 0, m);
        assert_eq!(whole.meshes[&m].mesh.hash(), nav.meshes[&m].mesh.hash());
        // Nudging the terrain re-buckets its 8,192 triangles: that is the tick's work (charged),
        // the tiles follow on the next ticks.
        let terrain = w.find_path("/terrain").unwrap();
        w.update(&EntityKey::Id(terrain), UpdateSpec { position: Some(Vec3::new(0.0, 0.01, 0.0)), ..Default::default() }, "t").unwrap();
        nav.tick(&mut w, &mut physics, 0, ticks, 1.0 / 30.0, 30);
        assert_eq!(nav.last.as_ref().unwrap().tiles_rebuilt, 0, "the bucketing used the tick's work");
        let mut more = 0;
        while !nav.meshes[&m].dirty.is_empty() && more < 2_000 {
            nav.tick(&mut w, &mut physics, 0, ticks + 1 + more, 1.0 / 30.0, 30);
            more += 1;
        }
        assert!(nav.meshes[&m].dirty.is_empty());
    }

    #[test]
    fn senses_see_walls_even_with_physics_off() {
        let mut w = World::default();
        // Only nav ticks run (as with physics disabled): nothing else syncs the physics space.
        let wall = cube(&mut w, "wall", Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.5, 2.0, 30.0));
        w.update(&EntityKey::Id(wall), UpdateSpec { collider: Some(Some(crate::physics::Collider::default())), ..Default::default() }, "t").unwrap();
        cube(&mut w, "t", Vec3::new(5.0, 0.5, 0.0), Vec3::ONE);
        let def = Json::parse(r#"{"selector": [{"sequence": [{"if": "sees", "target": "/t", "range": 50, "fov_deg": 360}, {"set": "saw", "value": 1}]}, {"set": "saw", "value": 0}]}"#).unwrap();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("g".into());
        s.position = Vec3::new(-5.0, 0.5, 0.0);
        s.props = Json::obj([(MIND_KEY, Json::obj([("def", def)]))]);
        let g = w.spawn(s, "t").unwrap().id;
        let mut nav = Nav::default();
        let saw = |nav: &Nav, w: &World| nav.mind_json(w, g, 4096).unwrap().get("blackboard").and_then(|b| b.get("saw")).and_then(Json::as_f64);
        run(&mut nav, &mut w, 3);
        assert_eq!(saw(&nav, &w), Some(0.0), "the wall hides the target");
        w.update(&EntityKey::Id(wall), UpdateSpec { position: Some(Vec3::new(0.0, 1.0, 40.0)), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 3);
        assert_eq!(saw(&nav, &w), Some(1.0), "the wall moved away");
    }

    #[test]
    fn many_invalid_minds_are_summarised_within_a_byte_cap() {
        let mut w = World::default();
        let bad: Vec<String> = (0..60).map(|i| format!(r#"{{"do": "no_such_action_{i}", "x": "{}"}}"#, "y".repeat(80))).collect();
        let def = Json::parse(&format!(r#"{{"sequence": [{}]}}"#, bad.join(","))).unwrap();
        for i in 0..40 {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(format!("m{i}"));
            s.props = Json::obj([(MIND_KEY, Json::obj([("def", def.clone())]))]);
            w.spawn(s, "t").unwrap();
        }
        let mut nav = Nav::default();
        nav.sync(&w, 0);
        assert_eq!(nav.mind_problems.len(), 40);
        let (list, left) = nav.problems_json(&w, 8 * 1024);
        assert!(list.to_string().len() <= 8 * 1024, "{}", list.to_string().len());
        let shown = list.as_array().unwrap().len();
        assert!(shown > 0 && shown + left == 40, "{shown} + {left}");
        assert!(list.to_string().contains("invalid_mind") && list.to_string().contains("\"diagnostics\""));
    }

    #[test]
    fn an_agent_whose_angles_read_180_0_180_still_faces_where_it_walks() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let a = walker(&mut w, "w", Vec3::new(4.0, 0.5, -6.0), vec![]);
        // (180, 30, 180) is a yaw of 150 degrees: how physics writes a character's turn back.
        w.update(&EntityKey::Id(a), UpdateSpec { rotation_deg: Some(Vec3::new(180.0, 30.0, 180.0)), ..Default::default() }, "t").unwrap();
        w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(3.0)), ("goal", Json::arr([Json::from(8.0), Json::from(0.0), Json::from(8.0)]))]))])), ..Default::default() }, "t").unwrap();
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let start = w.get(a).unwrap().world_position();
        run(&mut nav, &mut w, 10);
        let e = w.get(a).unwrap();
        let moved = e.world_position() - start;
        let f = e.model().cols[2];
        let l = (f[0] * f[0] + f[2] * f[2]).sqrt() * (moved.x * moved.x + moved.z * moved.z).sqrt();
        assert!(l > 0.0 && (f[0] * moved.x + f[2] * moved.z) / l > 0.9, "front {:?} moved {moved:?} rot {:?}", f, e.rotation_deg);
        // Upright stays upright.
        assert!(e.model().cols[1][1] > 0.99, "{:?}", e.rotation_deg);
    }

    #[test]
    fn parented_agents_turn_in_their_parents_frame() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("squad".into());
        g.rotation_deg = Vec3::new(0.0, 90.0, 0.0);
        w.spawn(g, "t").unwrap();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("w".into());
        s.parent = Some(EntityKey::Path("/squad".into()));
        // Local +X of a parent turned 90 degrees about Y.
        s.position = Vec3::new(0.0, 0.5, 6.0);
        s.scale = Vec3::new(0.5, 1.0, 0.5);
        s.props = Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(3.0)), ("goal", Json::arr([Json::from(-8.0), Json::from(0.0), Json::from(-8.0)]))]))]);
        let a = w.spawn(s, "t").unwrap().id;
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let start = w.get(a).unwrap().world_position();
        run(&mut nav, &mut w, 10);
        let e = w.get(a).unwrap();
        let moved = e.world_position() - start;
        // The agent's world front points the way it walks.
        let f = e.model().cols[2];
        let (fx, fz) = (f[0], f[2]);
        let l = (fx * fx + fz * fz).sqrt() * (moved.x * moved.x + moved.z * moved.z).sqrt();
        assert!(l > 0.0 && (fx * moved.x + fz * moved.z) / l > 0.8, "front {fx},{fz} moved {moved:?} rot {:?}", e.rotation_deg);
    }

    /// A navmesh entity with these extra settings (bounds of the test room).
    fn navmesh_with(w: &mut World, extra: &[(&str, Json)]) -> u64 {
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("nav".into());
        w.spawn(g, "t").unwrap();
        let mut j = MeshSettings::new(
            specula_via::Config { bounds_min: Vec3::new(-10.0, -1.0, -10.0), bounds_max: Vec3::new(10.0, 4.0, 10.0), tile_cells: 32, ..Default::default() },
            Sources::default(),
            8,
        )
        .to_json();
        for (k, v) in extra {
            j.set(*k, v.clone());
        }
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("default".into());
        s.parent = Some(EntityKey::Path("/nav".into()));
        s.props = Json::obj([(MESH_KEY, j)]);
        w.spawn(s, "t").unwrap().id
    }

    #[test]
    fn an_agent_jumps_a_gap_by_an_off_mesh_link_and_links_change_without_a_rebuild() {
        let mut w = World::default();
        cube(&mut w, "west", Vec3::new(-5.5, -0.5, 0.0), Vec3::new(9.0, 1.0, 20.0));
        cube(&mut w, "east", Vec3::new(5.5, -0.5, 0.0), Vec3::new(9.0, 1.0, 20.0));
        let link = Json::parse(r#"[{"from": [-1.3, 0, 0], "to": [1.3, 0, 0], "kind": "jump"}]"#).unwrap();
        let m = navmesh_with(&mut w, &[("links", link)]);
        let a = walker(&mut w, "w", Vec3::new(-6.0, 0.5, 2.0), vec![]);
        w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(3.0)), ("goal", Json::arr([Json::from(6.0), Json::from(0.0), Json::from(-2.0)]))]))])), ..Default::default() }, "t").unwrap();
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        assert!(nav.meshes[&m].mesh.resolved_link(0).is_some(), "{:?}", nav.problems);
        let built = nav.meshes[&m].rebuilt_tiles;
        let mut flew = 0.0f32;
        for _ in 0..12 {
            run(&mut nav, &mut w, 20);
            flew = flew.max(w.get(a).unwrap().world_position().y);
        }
        let p = w.get(a).unwrap().world_position();
        assert!(horizontal(p, Vec3::new(6.0, 0.0, -2.0)) < 0.5 && flew > 0.8, "{p:?} highest {flew}");
        // Moving the link only resets the links.
        let moved = Json::parse(r#"[{"from": [-1.3, 0, 5], "to": [1.3, 0, 5], "kind": "jump"}]"#).unwrap();
        let mut j = w.get(m).unwrap().props.get(MESH_KEY).unwrap().clone();
        j.set("links", moved);
        w.update(&EntityKey::Id(m), UpdateSpec { props: Some(Json::obj([(MESH_KEY, j)])), ..Default::default() }, "t").unwrap();
        run(&mut nav, &mut w, 1);
        let slot = &nav.meshes[&m];
        assert_eq!(slot.rebuilt_tiles, built, "no tiles rebuilt");
        assert!(slot.mesh.resolved_link(0).is_some_and(|r| (r.start_pos.z - 5.0).abs() < 0.5));
        // Bad links are refused with the field.
        let err = MeshSettings::from_json(&Json::parse(r#"{"links": [{"from": [0,0,0], "to": [1,0,0], "kind": "fly"}]}"#).unwrap()).unwrap_err();
        assert!(err.message.contains("kind"), "{}", err.message);
    }

    #[test]
    fn agents_avoid_areas_marked_by_volumes() {
        let mut w = World::default();
        cube(&mut w, "floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(20.0, 1.0, 20.0));
        // A pool across most of the room (x -10..6), marked by a volume that adds no geometry.
        let mut s = SpawnSpec::new(Shape::Group);
        s.name = Some("pool".into());
        s.position = Vec3::new(-2.0, 0.0, 0.0);
        s.scale = Vec3::new(16.0, 1.0, 3.0);
        s.props = Json::obj([(AREA_KEY, Json::obj([("area", Json::from("water")), ("volume", Json::from(true))]))]);
        w.spawn(s, "t").unwrap();
        let areas = Json::parse(r#"[{"name": "water", "cost": 5, "flags": ["wet"]}]"#).unwrap();
        let m = navmesh_with(&mut w, &[("areas", areas)]);
        let a = walker(&mut w, "w", Vec3::new(-5.0, 0.5, -6.0), vec![]);
        let goal = Json::arr([Json::from(-5.0), Json::from(0.0), Json::from(6.0)]);
        w.update(
            &EntityKey::Id(a),
            UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(3.0)), ("goal", goal), ("avoid_flags", Json::arr([Json::from("wet")]))]))])), ..Default::default() },
            "t",
        )
        .unwrap();
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let wet = nav.meshes[&m].mesh.tiles().flat_map(|(_, t)| t.polys.iter()).filter(|p| p.area == 1).count();
        assert!(wet > 0, "the volume marked polygons");
        let mut swam = false;
        for _ in 0..20 {
            run(&mut nav, &mut w, 20);
            let p = w.get(a).unwrap().world_position();
            swam |= p.x < 5.8 && p.z.abs() < 1.3;
        }
        let p = w.get(a).unwrap().world_position();
        assert!(!swam && horizontal(p, Vec3::new(-5.0, 0.0, 6.0)) < 0.5, "walked around: {p:?}");
    }

    #[test]
    fn a_saved_nav_state_continues_exactly_after_a_load() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let def = Json::parse(
            r#"{"root": {"selector": [
                {"sequence": [{"if": "near", "target": "/thief", "distance": 4}, {"do": "chase", "target": "/thief", "within": 0.8}]},
                {"do": "patrol", "points": [[4, 0, 4], [4, 0, 7], [-6, 0, 6]]}
            ]}}"#,
        )
        .unwrap();
        walker(&mut w, "guard", Vec3::new(3.0, 0.5, 3.0), vec![(MIND_KEY, Json::obj([("def", def)]))]);
        for i in 0..6 {
            let a = walker(&mut w, &format!("walker{i}"), Vec3::new(-6.0 + i as f32, 0.5, -6.0), vec![]);
            let goal = Json::arr([Json::from(6.0 - i as f32), Json::from(0.0), Json::from(6.0)]);
            w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(2.5)), ("goal", goal)]))])), ..Default::default() }, "t").unwrap();
        }
        cube(&mut w, "thief", Vec3::new(-6.0, 0.5, -8.0), Vec3::new(0.4, 1.0, 0.4));
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let mut physics = Physics::default();
        let tick = |nav: &mut Nav, w: &mut World, physics: &mut Physics, t: u64| {
            nav.tick(w, physics, 0, t, 1.0 / 30.0, 30);
            let _ = w.take_changes();
        };
        for t in 0..95 {
            tick(&mut nav, &mut w, &mut physics, t);
        }
        // Save; "load" the same world into a fresh engine (a new generation) and restore.
        let saved = Json::parse(&nav.save_state().to_string()).unwrap();
        let mut w2 = w.clone();
        let mut nav2 = Nav::default();
        let mut physics2 = Physics::default();
        assert_eq!(nav2.restore_state(&w2, 1, &saved).unwrap(), None, "waits for its navmesh");
        assert!(nav2.loading());
        // The engine builds it in slices with the clock held; the slices don't matter.
        let mut slices = 0;
        while !nav2.load_slice(&w2, 1, Some(2_000)) {
            slices += 1;
        }
        assert!(slices > 1 && !nav2.loading(), "{slices} slices");
        let h = nav2.take_restore_outcome().unwrap().unwrap();
        assert_eq!(h, nav.hash());
        for t in 95..260 {
            tick(&mut nav, &mut w, &mut physics, t);
            nav2.tick(&mut w2, &mut physics2, 1, t, 1.0 / 30.0, 30);
            let _ = w2.take_changes();
            assert_eq!(nav.hash(), nav2.hash(), "tick {t}");
        }
        // A save for another world is refused, and navigation starts fresh.
        let mut other = World::default();
        room(&mut other);
        let mut nav3 = Nav::default();
        let err = match nav3.restore_state(&other, 0, &saved) {
            Ok(None) => {
                nav3.load_slice(&other, 0, None);
                nav3.take_restore_outcome().unwrap().unwrap_err()
            }
            r => r.map(|_| ()).unwrap_err(),
        };
        assert_eq!(err.code, "state_mismatch", "{}", err.message);
        assert!(nav3.restore_state(&other, 0, &Json::parse(r#"{"version": 9}"#).unwrap()).is_err());
    }

    #[test]
    fn bad_or_unknown_nav_areas_are_problems() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh_with(&mut w, &[("areas", Json::parse(r#"[{"name": "water"}]"#).unwrap())]);
        let floor = w.find_path("/floor").unwrap();
        let mut nav = Nav::default();
        nav.sync(&w, 0);
        assert!(nav.area_problems.is_empty());
        let set = |w: &mut World, v: Json| {
            w.update(&EntityKey::Id(floor), UpdateSpec { props: Some(Json::obj([(AREA_KEY, v)])), ..Default::default() }, "t").unwrap();
        };
        set(&mut w, Json::from("lava"));
        nav.sync(&w, 0);
        let p = nav.area_problems.get(&floor).map(Json::to_string).unwrap_or_default();
        assert!(p.contains("unknown_area") && p.contains("/nav/default"), "{p}");
        set(&mut w, Json::from(3));
        nav.sync(&w, 0);
        assert!(nav.area_problems.get(&floor).is_some_and(|p| p.to_string().contains("invalid_nav_area")));
        set(&mut w, Json::from("water"));
        nav.sync(&w, 0);
        assert!(nav.area_problems.is_empty(), "{:?}", nav.area_problems);
        let (list, _) = nav.problems_json(&w, 4096);
        assert!(!list.to_string().contains("nav_area"));
        // Volumes too (the review's p22): an unknown area on a marking volume, and a number.
        let mut v1 = SpawnSpec::new(Shape::Group);
        v1.name = Some("v1".into());
        v1.props = Json::obj([(AREA_KEY, Json::obj([("area", Json::from("lava")), ("volume", Json::from(true))]))]);
        let v1 = w.spawn(v1, "t").unwrap().id;
        let mut v2 = SpawnSpec::new(Shape::Group);
        v2.name = Some("v2".into());
        v2.props = Json::obj([(AREA_KEY, Json::from(7))]);
        let v2 = w.spawn(v2, "t").unwrap().id;
        nav.sync(&w, 0);
        assert!(nav.area_problems.get(&v1).is_some_and(|p| p.to_string().contains("unknown_area")), "{:?}", nav.area_problems);
        assert!(nav.area_problems.get(&v2).is_some_and(|p| p.to_string().contains("invalid_nav_area")), "{:?}", nav.area_problems);
        let _ = m;
    }

    #[test]
    fn edits_during_a_load_never_hold_it() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        let wall = cube(&mut w, "mover", Vec3::new(0.0, 1.0, 5.0), Vec3::new(18.0, 2.0, 0.3));
        let mut nav = Nav::default();
        nav.sync(&w, 3);
        assert!(nav.loading());
        let mut slices = 0;
        // A swarm moves a long wall between every slice: the load still ends.
        while !nav.load_slice(&w, 3, Some(2_000)) {
            slices += 1;
            w.update(&EntityKey::Id(wall), UpdateSpec { position: Some(Vec3::new(0.0, 1.0, -6.0 + (slices % 12) as f32)), ..Default::default() }, "t").unwrap();
            assert!(slices < 500, "the load never ended");
        }
        assert!(!nav.loading() && nav.meshes[&m].ready);
        // The last edits' tiles are left to the ticks.
        let mut physics = Physics::default();
        let mut t = 0;
        while !nav.meshes[&m].dirty.is_empty() && t < 200 {
            nav.tick(&mut w, &mut physics, 3, t, 1.0 / 30.0, 30);
            t += 1;
        }
        assert!(nav.meshes[&m].dirty.is_empty());
    }

    #[test]
    fn the_save_section_round_trips_and_refuses_damage() {
        let mut w = World::default();
        room(&mut w);
        let m = navmesh(&mut w);
        walker(&mut w, "w", Vec3::new(-5.0, 0.5, -5.0), vec![]);
        let mut nav = Nav::default();
        build_now(&mut nav, &w, 0, m);
        let mut physics = Physics::default();
        for t in 0..20 {
            nav.tick(&mut w, &mut physics, 0, t, 1.0 / 30.0, 30);
        }
        cube(&mut w, "crate", Vec3::new(3.0, 0.5, 3.0), Vec3::ONE);
        let bytes = nav.save_section(&w, 0).unwrap();
        let parsed = parse_section(&bytes).unwrap();
        let mut back = Nav::default();
        assert_eq!(back.restore_state(&w, 1, &parsed).unwrap(), None);
        back.load_slice(&w, 1, None);
        assert_eq!(back.take_restore_outcome().unwrap().unwrap(), nav.save_hash(&w, 0));
        // Damage is a structured error, never a panic: every cut, flipped bytes, wrong versions.
        for cut in (0..bytes.len()).step_by(37) {
            let _ = parse_section(&bytes[..cut]);
        }
        assert_eq!(parse_section(&bytes[..3]).unwrap_err().0, "corrupt_section");
        let mut v2 = bytes.clone();
        v2[0] = 2;
        assert_eq!(parse_section(&v2).unwrap_err().0, "unsupported_version");
        for i in (4..bytes.len()).step_by(53) {
            let mut b = bytes.clone();
            b[i] ^= 0x5a;
            if let Ok(j) = parse_section(&b) {
                let mut n = Nav::default();
                if let Ok(None) = n.restore_state(&w, 1, &j) {
                    n.load_slice(&w, 1, None);
                    let _ = n.take_restore_outcome();
                }
                // Whatever came of it, navigation still ticks.
                let mut w2 = w.clone();
                n.tick(&mut w2, &mut physics, 1, 0, 1.0 / 30.0, 30);
            }
        }
        // A restore that fails part way (a damaged stale tile after a good one) leaves no saved
        // tile behind: every tile is rebuilt from the world over the ticks.
        let mut j = parse_section(&bytes).unwrap();
        if let Some(Json::Arr(meshes)) = j.get("meshes").cloned() {
            let mut meshes = meshes;
            if let Some(Json::Arr(stale)) = meshes[0].get("stale").cloned() {
                assert!(!stale.is_empty(), "the edit left stale tiles");
                let mut stale = stale;
                let mut broken = stale[0].clone();
                broken.set("state", Json::obj([("polys", Json::Arr(Vec::new()))]));
                stale.push(broken);
                meshes[0].set("stale", Json::Arr(stale));
            }
            j.set("meshes", Json::Arr(meshes));
        }
        let mut failed = Nav::default();
        if let Ok(None) = failed.restore_state(&w, 1, &j) {
            failed.load_slice(&w, 1, None);
        }
        assert!(failed.take_restore_outcome().is_some_and(|r| r.is_err()));
        let mut w3 = w.clone();
        for t in 0..200 {
            failed.tick(&mut w3, &mut physics, 1, t, 1.0 / 30.0, 30);
        }
        let mut whole = Nav::default();
        whole.sync(&w3, 0);
        build_now(&mut whole, &w3, 0, m);
        assert_eq!(failed.meshes[&m].mesh.hash(), whole.meshes[&m].mesh.hash(), "rebuilt from the world");
        // A switch to new settings in progress (two navmeshes at once) is refused, retry later.
        let mut j = w.get(m).unwrap().props.get(MESH_KEY).unwrap().clone();
        j.set("cell_size", Json::from(0.1));
        w.update(&EntityKey::Id(m), UpdateSpec { props: Some(Json::obj([(MESH_KEY, j)])), ..Default::default() }, "t").unwrap();
        assert_eq!(nav.save_section(&w, 0).unwrap_err().code, "nav_building");
    }

    #[test]
    fn a_save_never_changes_the_run_whatever_happens_before_the_next_tick() {
        // The review's p29: spawn a cube, save, delete the cube before the next tick. The run that
        // saved and one that didn't must stay identical (the save works on a copy).
        let run = |save: bool| {
            let mut w = World::default();
            room(&mut w);
            let mut nav = Nav::default();
            let mut physics = Physics::default();
            nav.tick(&mut w, &mut physics, 0, 0, 1.0 / 30.0, 30);
            let m = navmesh(&mut w);
            let a = walker(&mut w, "w", Vec3::new(-6.0, 0.5, -6.0), vec![]);
            w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(2.5)), ("goal", Json::arr([Json::from(6.0), Json::from(0.0), Json::from(6.0)]))]))])), ..Default::default() }, "t").unwrap();
            let mut hashes = Vec::new();
            for t in 1..200u64 {
                if t == 60 {
                    let c = cube(&mut w, "crate", Vec3::new(1.0, 0.5, 1.0), Vec3::new(2.0, 1.0, 2.0));
                    if save {
                        let bytes = nav.save_section(&w, 0).unwrap();
                        assert!(parse_section(&bytes).is_ok());
                    }
                    w.delete(&EntityKey::Id(c), None).unwrap();
                }
                nav.tick(&mut w, &mut physics, 0, t, 1.0 / 30.0, 30);
                let _ = w.take_changes();
                hashes.push((nav.hash(), nav.meshes[&m].dirty.len()));
            }
            (hashes, w.state_hash())
        };
        let (a, wa) = run(false);
        let (b, wb) = run(true);
        assert_eq!(a, b, "saving changed the run");
        assert_eq!(wa, wb);
    }

    #[test]
    fn saving_is_never_an_input_and_a_load_continues_mid_build_exactly() {
        // The review's p23: a run that saves and a run that doesn't stay identical, and a save
        // taken with dirty tiles (an edit just made) and unbuilt ones (a navmesh still being
        // built) loads and continues tick for tick like the saving run.
        // (The navmesh appears after the first tick: one present at the first look is a load,
        // built in full before ticking.)
        let setup = || {
            let mut w = World::default();
            room(&mut w);
            let a = walker(&mut w, "w", Vec3::new(-6.0, 0.5, -6.0), vec![]);
            w.update(&EntityKey::Id(a), UpdateSpec { props: Some(Json::obj([(AGENT_KEY, Json::obj([("speed", Json::from(2.5)), ("goal", Json::arr([Json::from(6.0), Json::from(0.0), Json::from(6.0)]))]))])), ..Default::default() }, "t").unwrap();
            w
        };
        let events = |w: &mut World, t: u64| -> Option<u64> {
            match t {
                1 => Some(navmesh_with(w, &[("tiles_per_tick", Json::from(1))])),
                20 => {
                    cube(w, "crate", Vec3::new(2.0, 0.5, 2.0), Vec3::new(1.0, 1.0, 1.0));
                    None
                }
                _ => None,
            }
        };
        let mut wa = setup();
        let mut wb = setup();
        let mut m = 0;
        let (mut na, mut nb) = (Nav::default(), Nav::default());
        let (mut pa, mut pb) = (Physics::default(), Physics::default());
        let step = |n: &mut Nav, w: &mut World, p: &mut Physics, t: u64| {
            n.tick(w, p, 0, t, 1.0 / 30.0, 30);
            let _ = w.take_changes();
        };
        let mut saved = None;
        for t in 0..160u64 {
            // Mid first build (tiles_per_tick 1) at the save, with an edit just made too.
            if let Some(id) = events(&mut wa, t) {
                m = id;
            }
            events(&mut wb, t);
            if t == 21 {
                let bytes = na.save_section(&wa, 0).unwrap();
                let hash = na.save_hash(&wa, 0);
                let j = parse_section(&bytes).unwrap();
                let mesh = j.get("meshes").and_then(Json::as_array).unwrap()[0].clone();
                assert!(mesh.get("ready").and_then(Json::as_bool) == Some(false) && !mesh.get("dirty").and_then(Json::as_array).unwrap().is_empty(), "saved mid-build");
                saved = Some((bytes, wa.clone(), hash));
            }
            step(&mut na, &mut wa, &mut pa, t);
            step(&mut nb, &mut wb, &mut pb, t);
            assert_eq!(na.hash(), nb.hash(), "tick {t}: saving changed the run");
        }
        assert_eq!(wa.state_hash(), wb.state_hash());
        // Load the save into a fresh world and continue alongside a fresh saving run.
        let (bytes, world_at_save, hash_at_save) = saved.unwrap();
        let mut w1 = setup();
        let (mut n1, mut p1) = (Nav::default(), Physics::default());
        for t in 0..21u64 {
            events(&mut w1, t);
            step(&mut n1, &mut w1, &mut p1, t);
        }
        n1.sync(&w1, 0);
        assert_eq!(n1.hash(), hash_at_save);
        let mut w2 = world_at_save;
        let mut n2 = Nav::default();
        let mut p2 = Physics::default();
        assert_eq!(n2.restore_state(&w2, 1, &parse_section(&bytes).unwrap()).unwrap(), None);
        while !n2.load_slice(&w2, 1, Some(3_000)) {}
        assert_eq!(n2.take_restore_outcome().unwrap().unwrap(), hash_at_save);
        for t in 21..160u64 {
            step(&mut n1, &mut w1, &mut p1, t);
            n2.tick(&mut w2, &mut p2, 1, t, 1.0 / 30.0, 30);
            let _ = w2.take_changes();
            assert_eq!(n1.hash(), n2.hash(), "tick {t}");
        }
        assert!(n2.meshes[&m].ready && n2.meshes[&m].dirty.is_empty());
        assert_eq!(w1.state_hash(), w2.state_hash());
    }

    #[test]
    fn navmesh_readiness_depends_only_on_the_world_and_the_ticks() {
        // However often navigation looks at the world between ticks (nav_build calls sync it),
        // the navmesh builds the same way: only ticks build (a load excepted, see Nav::loading).
        let run = |looks: usize| {
            let mut w = World::default();
            room(&mut w);
            let mut nav = Nav::default();
            let mut physics = Physics::default();
            nav.tick(&mut w, &mut physics, 0, 0, 1.0 / 30.0, 30);
            let m = navmesh_with(&mut w, &[("tiles_per_tick", Json::from(1))]);
            for _ in 0..looks {
                nav.sync(&w, 0);
            }
            assert!(!nav.loading(), "a navmesh made in a running world is not a load");
            let mut hashes = Vec::new();
            for t in 1..40 {
                nav.tick(&mut w, &mut physics, 0, t, 1.0 / 30.0, 30);
                hashes.push(nav.hash());
            }
            (hashes, nav.meshes[&m].ready)
        };
        let (one, ready) = run(1);
        assert!(ready);
        assert_eq!(one, run(3).0);
        assert_eq!(one, run(0).0);
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
