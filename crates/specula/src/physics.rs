//! Physics in the engine (foundation F4; docs/design/PHYSICS.md): the world's `collider` and
//! `body` components, the `physics` environment settings, and the per-tick bridge between the
//! world and [`specula_physics::Space`].
//!
//! - **Components are data.** An entity with a `collider` collides (static unless it has a
//!   `body`); a `body` makes it kinematic or dynamic, and its velocity, sleep state and
//!   character controller live in the component, so world files, `world_hash` and the journal
//!   carry them. Collider shapes are box, sphere, capsule, convex `hull` (points), and static
//!   triangle `mesh` and `heightfield` (inline data, bounded). Shapes are in the entity's local
//!   (scaled) coordinates. Layers are names; `mask` lists the layer names a collider collides
//!   with (all by default).
//! - **Each fixed tick** (after the hosted game's tick): entities whose components or world
//!   transforms changed since last tick are synced into the space (only what changed, through
//!   the world's change log); characters move; the space steps; and the bodies that moved are
//!   written back (position, rotation, body velocity and sleep state), journaled as `physics`
//!   changes like a game's (after-only, coalesced per tick, exempt from claims).
//! - **Determinism (F2).** Physics state hidden from the world (warm-start impulses, sleep
//!   timers, exact rotations) is hashed by the space each tick: `world_hash` reports it next to
//!   the world's own hash, so replays and twin runs can compare both.
//! - Transient entities (a game's presentation) never take part.

use crate::world::{self, check_num, check_point, vec3_json, validate_tag, WorldError};
use specula_core::hash::StableHasher;
use specula_core::math::Vec3;
use specula_core::Json;
use std::sync::Arc;

#[path = "physics_save.rs"]
mod save;

// ---------------------------------------------------------------------------------------------
// Components

/// Collider sizes and radii, metres.
pub const COLLIDER_SIZE: std::ops::RangeInclusive<f32> = 1.0e-4..=1.0e5;
/// Inline collider data limits (world files stay small enough to read and diff).
pub const MAX_HULL_POINTS: usize = specula_physics::shape::MAX_HULL_POINTS;
pub const MAX_MESH_VERTICES: usize = 4096;
pub const MAX_MESH_TRIANGLES: usize = 8192;
pub const MAX_HEIGHTFIELD_SIDE: u32 = 129;
/// Most layer names a collider's mask lists.
pub const MAX_MASK_LAYERS: usize = 32;

fn invalid(field: &str, message: String) -> WorldError {
    WorldError::new("invalid_value", message).with("field", field)
}

/// Collision shape, in the entity's local (scaled) coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum ColliderShape {
    /// Full extents (a unit cube's collider is `size: [1, 1, 1]`).
    Box { size: Vec3 },
    Sphere { radius: f32 },
    /// Along local +Y; `height` is the total height including both caps (≥ 2 × radius).
    Capsule { radius: f32, height: f32 },
    /// The convex hull of these points (4..=64).
    Hull { points: Arc<Vec<Vec3>> },
    /// Static only: triangles over vertices (counter-clockwise faces out).
    Mesh { vertices: Arc<Vec<Vec3>>, triangles: Arc<Vec<[u32; 3]>> },
    /// Static only: `columns × rows` heights (row by row along +Z, each along +X), `cell`
    /// metres apart on X and Z, centred on the entity's origin.
    Heightfield { columns: u32, rows: u32, cell: [f32; 2], heights: Arc<Vec<f32>> },
}

impl ColliderShape {
    pub fn name(&self) -> &'static str {
        match self {
            ColliderShape::Box { .. } => "box",
            ColliderShape::Sphere { .. } => "sphere",
            ColliderShape::Capsule { .. } => "capsule",
            ColliderShape::Hull { .. } => "hull",
            ColliderShape::Mesh { .. } => "mesh",
            ColliderShape::Heightfield { .. } => "heightfield",
        }
    }

    /// Meshes and heightfields have no volume: static bodies only.
    pub fn static_only(&self) -> bool {
        matches!(self, ColliderShape::Mesh { .. } | ColliderShape::Heightfield { .. })
    }
}

/// What an entity collides as.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    pub shape: ColliderShape,
    pub offset: Vec3,
    /// Reports overlaps (trigger enter/exit) instead of blocking.
    pub is_trigger: bool,
    /// Collision layer name (layers are named, never bit numbers).
    pub layer: String,
    /// The layer names it collides with; `None` = every layer.
    pub mask: Option<Vec<String>>,
    pub friction: f32,
    pub restitution: f32,
}

pub const DEFAULT_FRICTION: f32 = 0.6;

impl Default for Collider {
    fn default() -> Collider {
        Collider {
            shape: ColliderShape::Box { size: Vec3::ONE },
            offset: Vec3::ZERO,
            is_trigger: false,
            layer: "default".into(),
            mask: None,
            friction: DEFAULT_FRICTION,
            restitution: 0.0,
        }
    }
}

fn points_json(points: &[Vec3]) -> Json {
    Json::arr(points.iter().map(|p| vec3_json(*p)))
}

impl Collider {
    /// JSON (defaults left out, so older worlds' colliders read and hash as before).
    pub fn to_json(&self) -> Json {
        let mut f = match &self.shape {
            ColliderShape::Box { size } => vec![("shape", Json::from("box")), ("size", vec3_json(*size))],
            ColliderShape::Sphere { radius } => vec![("shape", Json::from("sphere")), ("radius", Json::from(*radius))],
            ColliderShape::Capsule { radius, height } => {
                vec![("shape", Json::from("capsule")), ("radius", Json::from(*radius)), ("height", Json::from(*height))]
            }
            ColliderShape::Hull { points } => vec![("shape", Json::from("hull")), ("points", points_json(points))],
            ColliderShape::Mesh { vertices, triangles } => vec![
                ("shape", Json::from("mesh")),
                ("vertices", points_json(vertices)),
                ("triangles", Json::arr(triangles.iter().map(|t| Json::arr(t.map(Json::from))))),
            ],
            ColliderShape::Heightfield { columns, rows, cell, heights } => vec![
                ("shape", Json::from("heightfield")),
                ("columns", Json::from(*columns)),
                ("rows", Json::from(*rows)),
                ("cell", Json::arr(cell.map(Json::from))),
                ("heights", Json::arr(heights.iter().map(|h| Json::from(*h)))),
            ],
        };
        if self.offset != Vec3::ZERO {
            f.push(("offset", vec3_json(self.offset)));
        }
        if self.is_trigger {
            f.push(("is_trigger", Json::from(true)));
        }
        if self.layer != "default" {
            f.push(("layer", Json::from(self.layer.as_str())));
        }
        if let Some(m) = &self.mask {
            f.push(("mask", Json::arr(m.iter().map(|l| Json::from(l.as_str())))));
        }
        if self.friction != DEFAULT_FRICTION {
            f.push(("friction", Json::from(self.friction)));
        }
        if self.restitution != 0.0 {
            f.push(("restitution", Json::from(self.restitution)));
        }
        Json::obj(f)
    }

    /// Value checks (the world runs them on every write).
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        match &self.shape {
            ColliderShape::Box { size } => {
                [size.x, size.y, size.z].iter().try_for_each(|v| check_num("collider.size", *v, &COLLIDER_SIZE))?
            }
            ColliderShape::Sphere { radius } => check_num("collider.radius", *radius, &COLLIDER_SIZE)?,
            ColliderShape::Capsule { radius, height } => {
                check_num("collider.radius", *radius, &COLLIDER_SIZE)?;
                check_num("collider.height", *height, &COLLIDER_SIZE)?;
                if *height < 2.0 * radius {
                    return Err(invalid("collider.height", "'collider.height' is the total height and must be at least 2 x radius".into()));
                }
            }
            ColliderShape::Hull { points } => {
                if !(4..=MAX_HULL_POINTS).contains(&points.len()) {
                    return Err(invalid("collider.points", format!("'collider.points' must list 4 to {MAX_HULL_POINTS} points")));
                }
                points.iter().try_for_each(|p| check_point("collider.points", *p))?;
                // Flat or degenerate point sets are refused here, not at simulation time.
                specula_physics::shape::Hull::new(points).map_err(|e| invalid("collider.points", format!("'collider.points': {}", e.message)))?;
            }
            ColliderShape::Mesh { vertices, triangles } => {
                if vertices.len() < 3 || vertices.len() > MAX_MESH_VERTICES || triangles.is_empty() || triangles.len() > MAX_MESH_TRIANGLES {
                    return Err(invalid(
                        "collider.triangles",
                        format!("a mesh collider takes 3..{MAX_MESH_VERTICES} vertices and 1..{MAX_MESH_TRIANGLES} triangles"),
                    ));
                }
                vertices.iter().try_for_each(|p| check_point("collider.vertices", *p))?;
                if let Some(i) = triangles.iter().position(|t| t.iter().any(|v| *v as usize >= vertices.len())) {
                    return Err(invalid("collider.triangles", format!("'collider.triangles'[{i}] uses a vertex index past the vertices")));
                }
            }
            ColliderShape::Heightfield { columns, rows, cell, heights } => {
                let side = 2..=MAX_HEIGHTFIELD_SIDE;
                if !side.contains(columns) || !side.contains(rows) {
                    return Err(invalid("collider.columns", format!("'collider.columns' and 'rows' must be 2..{MAX_HEIGHTFIELD_SIDE}")));
                }
                if heights.len() != (*columns * *rows) as usize {
                    return Err(invalid("collider.heights", format!("'collider.heights' needs columns x rows = {} numbers", columns * rows)));
                }
                cell.iter().try_for_each(|c| check_num("collider.cell", *c, &COLLIDER_SIZE))?;
                heights.iter().try_for_each(|h| check_num("collider.heights", *h, &(-1.0e5..=1.0e5)))?;
            }
        }
        check_point("collider.offset", self.offset)?;
        validate_tag(&self.layer).map_err(|e| e.with("field", "collider.layer"))?;
        if let Some(m) = &self.mask {
            if m.len() > MAX_MASK_LAYERS {
                return Err(invalid("collider.mask", format!("'collider.mask' lists at most {MAX_MASK_LAYERS} layers")));
            }
            m.iter().try_for_each(|l| validate_tag(l).map_err(|e| e.with("field", "collider.mask")))?;
        }
        check_num("collider.friction", self.friction, &(0.0..=10.0))?;
        check_num("collider.restitution", self.restitution, &(0.0..=1.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Never moves (walls, terrain).
    Static,
    /// Moved by game code or its velocity; pushes dynamic bodies (doors, platforms, characters).
    Kinematic,
    /// Moved by the simulation (crates, debris).
    Dynamic,
}

impl BodyKind {
    pub const ALL: [BodyKind; 3] = [BodyKind::Static, BodyKind::Kinematic, BodyKind::Dynamic];

    pub fn name(self) -> &'static str {
        match self {
            BodyKind::Static => "static",
            BodyKind::Kinematic => "kinematic",
            BodyKind::Dynamic => "dynamic",
        }
    }

    pub fn parse(s: &str) -> Option<BodyKind> {
        BodyKind::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// A kinematic body walked by the character controller (P2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterSpec {
    pub step_height: f32,
    pub max_slope_deg: f32,
    pub snap_distance: f32,
    pub push_force: f32,
    pub gravity: bool,
}

impl Default for CharacterSpec {
    fn default() -> CharacterSpec {
        let d = specula_physics::CharacterConfig::default();
        CharacterSpec { step_height: d.step_height, max_slope_deg: d.max_slope_deg, snap_distance: d.snap_distance, push_force: d.push_force, gravity: d.gravity }
    }
}

impl CharacterSpec {
    pub fn config(&self) -> specula_physics::CharacterConfig {
        specula_physics::CharacterConfig {
            step_height: self.step_height,
            max_slope_deg: self.max_slope_deg,
            snap_distance: self.snap_distance,
            push_force: self.push_force,
            gravity: self.gravity,
            ..specula_physics::CharacterConfig::default()
        }
    }

    fn to_json(self) -> Json {
        let d = CharacterSpec::default();
        let mut f = Vec::new();
        if self.step_height != d.step_height {
            f.push(("step_height", Json::from(self.step_height)));
        }
        if self.max_slope_deg != d.max_slope_deg {
            f.push(("max_slope_deg", Json::from(self.max_slope_deg)));
        }
        if self.snap_distance != d.snap_distance {
            f.push(("snap_distance", Json::from(self.snap_distance)));
        }
        if self.push_force != d.push_force {
            f.push(("push_force", Json::from(self.push_force)));
        }
        if self.gravity != d.gravity {
            f.push(("gravity", Json::from(self.gravity)));
        }
        if f.is_empty() {
            Json::from(true)
        } else {
            Json::obj(f)
        }
    }
}

/// How an entity moves: its kind, mass, and the motion state physics writes back each tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub kind: BodyKind,
    /// Kilograms; used by dynamic bodies.
    pub mass: f32,
    /// m/s, world frame. Dynamic: the current velocity. Kinematic: the velocity it moves at
    /// (a character: its walking intent; the vertical part jumps and falls).
    pub velocity: Vec3,
    /// rad/s, world frame.
    pub angular_velocity: Vec3,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    /// At rest (physics sets it; an agent may set it to start a body asleep).
    pub sleeping: bool,
    /// The tunnelling guard against static colliders (on by default).
    pub ccd: bool,
    /// World axes it can't move along / turn about (x, y, z).
    pub lock_position: [bool; 3],
    pub lock_rotation: [bool; 3],
    /// Kinematic bodies only: walk with the character controller.
    pub character: Option<CharacterSpec>,
}

pub const DEFAULT_ANGULAR_DAMPING: f32 = 0.05;

impl Default for Body {
    fn default() -> Body {
        Body::new(BodyKind::Static, 1.0)
    }
}

fn axes_json(a: [bool; 3]) -> Json {
    Json::from(["x", "y", "z"].iter().zip(a).filter(|(_, on)| *on).map(|(n, _)| *n).collect::<String>())
}

fn axes_arg(v: &Json, field: &str) -> Result<[bool; 3], String> {
    let s = v.as_str().ok_or_else(|| format!("'{field}' must be a string of axes, e.g. \"z\" or \"xy\""))?;
    let mut out = [false; 3];
    for c in s.chars() {
        match c {
            'x' => out[0] = true,
            'y' => out[1] = true,
            'z' => out[2] = true,
            _ => return Err(format!("'{field}' must use only x, y and z (got '{s}')")),
        }
    }
    Ok(out)
}

impl Body {
    pub fn new(kind: BodyKind, mass: f32) -> Body {
        Body {
            kind,
            mass,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            linear_damping: 0.0,
            angular_damping: DEFAULT_ANGULAR_DAMPING,
            gravity_scale: 1.0,
            sleeping: false,
            ccd: true,
            lock_position: [false; 3],
            lock_rotation: [false; 3],
            character: None,
        }
    }

    /// Only `kind` and `mass` differ from the defaults (how worlds before physics stored bodies).
    fn is_plain(&self) -> bool {
        *self == Body::new(self.kind, self.mass)
    }

    pub fn to_json(self) -> Json {
        let mut f = vec![("kind", Json::from(self.kind.name())), ("mass", Json::from(self.mass))];
        if self.velocity != Vec3::ZERO {
            f.push(("velocity", vec3_json(self.velocity)));
        }
        if self.angular_velocity != Vec3::ZERO {
            f.push(("angular_velocity", vec3_json(self.angular_velocity)));
        }
        if self.linear_damping != 0.0 {
            f.push(("linear_damping", Json::from(self.linear_damping)));
        }
        if self.angular_damping != DEFAULT_ANGULAR_DAMPING {
            f.push(("angular_damping", Json::from(self.angular_damping)));
        }
        if self.gravity_scale != 1.0 {
            f.push(("gravity_scale", Json::from(self.gravity_scale)));
        }
        if self.sleeping {
            f.push(("sleeping", Json::from(true)));
        }
        if !self.ccd {
            f.push(("ccd", Json::from(false)));
        }
        if self.lock_position != [false; 3] {
            f.push(("lock_position", axes_json(self.lock_position)));
        }
        if self.lock_rotation != [false; 3] {
            f.push(("lock_rotation", axes_json(self.lock_rotation)));
        }
        if let Some(c) = self.character {
            f.push(("character", c.to_json()));
        }
        Json::obj(f)
    }

    /// Hashes the body: kind and mass as before physics, the rest only when not default (so
    /// older worlds keep their hashes).
    /// Hashes the body: kind and mass as before physics (plain bodies keep their old hashes),
    /// then the motion fields directly (no JSON: moving bodies are hashed every tick).
    pub(crate) fn hash(&self, h: &mut StableHasher) {
        h.write_str(self.kind.name());
        h.write_f32(self.mass);
        if !self.is_plain() {
            h.write_u8(0xb0);
            self.hash_fixed(h);
            for v in [self.velocity, self.angular_velocity] {
                [v.x, v.y, v.z].iter().for_each(|c| h.write_f32(*c));
            }
            h.write_bool(self.sleeping);
        }
    }

    /// Everything but the motion state (velocities, sleeping).
    fn hash_fixed(&self, h: &mut StableHasher) {
        h.write_f32(self.linear_damping);
        h.write_f32(self.angular_damping);
        h.write_f32(self.gravity_scale);
        h.write_bool(self.ccd);
        self.lock_position.iter().chain(&self.lock_rotation).for_each(|l| h.write_bool(*l));
        match self.character {
            None => h.write_u8(0),
            Some(c) => {
                h.write_u8(1);
                [c.step_height, c.max_slope_deg, c.snap_distance, c.push_force].iter().for_each(|v| h.write_f32(*v));
                h.write_bool(c.gravity);
            }
        }
    }
}

thread_local! {
    /// The JSON text of box, sphere and capsule colliders, by [`Collider::field_hash`]: their
    /// world-hash contribution is that text (the layout from before physics, so existing worlds
    /// keep their hashes), built once per distinct collider instead of every tick a body moves.
    /// A pure cache: the result never depends on what is in it.
    static COLLIDER_TEXT: std::cell::RefCell<std::collections::BTreeMap<u64, Arc<str>>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Distinct collider texts kept (cleared when full).
const COLLIDER_TEXT_CACHE: usize = 4096;

impl Collider {
    /// A hash of every field, computed directly.
    pub fn field_hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x436f_6c6c_6964_6572);
        let v3 = |h: &mut StableHasher, v: &Vec3| [v.x, v.y, v.z].iter().for_each(|c| h.write_f32(*c));
        h.write_str(self.shape.name());
        match &self.shape {
            ColliderShape::Box { size } => v3(&mut h, size),
            ColliderShape::Sphere { radius } => h.write_f32(*radius),
            ColliderShape::Capsule { radius, height } => {
                h.write_f32(*radius);
                h.write_f32(*height);
            }
            ColliderShape::Hull { points } => {
                h.write_u64(points.len() as u64);
                points.iter().for_each(|p| v3(&mut h, p));
            }
            ColliderShape::Mesh { vertices, triangles } => {
                h.write_u64(vertices.len() as u64);
                vertices.iter().for_each(|p| v3(&mut h, p));
                h.write_u64(triangles.len() as u64);
                triangles.iter().flatten().for_each(|i| h.write_u32(*i));
            }
            ColliderShape::Heightfield { columns, rows, cell, heights } => {
                h.write_u32(*columns);
                h.write_u32(*rows);
                cell.iter().chain(heights.iter()).for_each(|v| h.write_f32(*v));
            }
        }
        v3(&mut h, &self.offset);
        h.write_bool(self.is_trigger);
        h.write_str(&self.layer);
        match &self.mask {
            None => h.write_u8(0),
            Some(m) => {
                h.write_u64(m.len() as u64 + 1);
                m.iter().for_each(|l| h.write_str(l));
            }
        }
        h.write_f32(self.friction);
        h.write_f32(self.restitution);
        h.finish()
    }

    /// The collider's part of the world hash.
    pub(crate) fn hash(&self, h: &mut StableHasher) {
        let key = self.field_hash();
        match self.shape {
            ColliderShape::Box { .. } | ColliderShape::Sphere { .. } | ColliderShape::Capsule { .. } => COLLIDER_TEXT.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.len() >= COLLIDER_TEXT_CACHE && !cache.contains_key(&key) {
                    cache.clear();
                }
                let text = cache.entry(key).or_insert_with(|| self.to_json().to_string().into());
                h.write_str(text);
            }),
            // Shapes newer than the hash format: their fields, directly.
            _ => h.write_u64(key),
        }
    }
}

pub const MASS: std::ops::RangeInclusive<f32> = 1.0e-6..=1.0e9;
const SPEED: std::ops::RangeInclusive<f32> = -1.0e4..=1.0e4;
const SPIN: std::ops::RangeInclusive<f32> = -1.0e3..=1.0e3;

pub(crate) fn check_body(b: &Body) -> Result<(), WorldError> {
    check_num("body.mass", b.mass, &MASS)?;
    [b.velocity.x, b.velocity.y, b.velocity.z].iter().try_for_each(|v| check_num("body.velocity", *v, &SPEED))?;
    [b.angular_velocity.x, b.angular_velocity.y, b.angular_velocity.z].iter().try_for_each(|v| check_num("body.angular_velocity", *v, &SPIN))?;
    check_num("body.linear_damping", b.linear_damping, &(0.0..=1000.0))?;
    check_num("body.angular_damping", b.angular_damping, &(0.0..=1000.0))?;
    check_num("body.gravity_scale", b.gravity_scale, &(-100.0..=100.0))?;
    if let Some(c) = &b.character {
        if b.kind != BodyKind::Kinematic {
            return Err(invalid("body.character", "'body.character' is for kinematic bodies (kind \"kinematic\")".into()));
        }
        check_num("body.character.step_height", c.step_height, &(0.0..=10.0))?;
        check_num("body.character.max_slope_deg", c.max_slope_deg, &(0.0..=89.0))?;
        check_num("body.character.snap_distance", c.snap_distance, &(0.0..=10.0))?;
        check_num("body.character.push_force", c.push_force, &(0.0..=1.0e6))?;
    }
    Ok(())
}

/// What a collider and a body must agree on (checked on spawn and whenever either changes).
pub(crate) fn check_pair(c: Option<&Collider>, b: Option<&Body>) -> Result<(), WorldError> {
    if let (Some(c), Some(b)) = (c, b) {
        if c.shape.static_only() && b.kind != BodyKind::Static {
            return Err(WorldError::new(
                "invalid_component",
                format!("'{}' colliders have no volume, so only static bodies may use them; use a box, sphere, capsule or hull", c.shape.name()),
            )
            .with("field", "collider.shape"));
        }
    }
    if let (Some(c), Some(b)) = (c, b) {
        if let ColliderShape::Hull { points } = &c.shape {
            let max = specula_physics::shape::MAX_MOVING_HULL_POINTS;
            if b.kind != BodyKind::Static && points.len() > max {
                return Err(WorldError::new(
                    "invalid_component",
                    format!(
                        "a hull on a {} body takes at most {max} points (this one has {}): hull pairs cost edges x edges; simplify it, or make the body static (static hulls take up to {MAX_HULL_POINTS})",
                        b.kind.name(),
                        points.len()
                    ),
                )
                .with("field", "collider.points")
                .with("limit", max));
            }
        }
    }
    if let Some(b) = b {
        if b.character.is_some() && c.is_none_or(|c| c.shape.static_only() || c.is_trigger) {
            return Err(WorldError::new("invalid_component", "a character needs a solid convex collider (a capsule, usually)").with("field", "collider"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Tool and world-file arguments

fn num(v: &Json, field: &str) -> Result<f32, String> {
    match v.as_f64() {
        Some(f) if f.is_finite() && f.abs() <= f64::from(f32::MAX) => Ok(f as f32),
        _ => Err(format!("'{field}' must be a finite number")),
    }
}

fn opt_num(o: &Json, key: &str, field: &str) -> Result<Option<f32>, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => num(v, field).map(Some),
    }
}

fn point(v: &Json, field: &str) -> Result<Vec3, String> {
    match v.as_array() {
        Some([x, y, z]) => Ok(Vec3::new(num(x, field)?, num(y, field)?, num(z, field)?)),
        _ => Err(format!("'{field}' must be [x, y, z]")),
    }
}

fn opt_point(o: &Json, key: &str, field: &str) -> Result<Option<Vec3>, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => point(v, field).map(Some),
    }
}

fn opt_bool(o: &Json, key: &str, field: &str) -> Result<Option<bool>, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| format!("'{field}' must be true or false")),
    }
}

fn check_object_keys(o: &Json, what: &str, allowed: &[&str]) -> Result<(), String> {
    let pairs = o.as_object().ok_or_else(|| format!("'{what}' must be an object"))?;
    if let Some((k, _)) = pairs.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
        return Err(format!("'{what}.{k}' is not a field; valid: {}", allowed.join(", ")));
    }
    Ok(())
}

const COLLIDER_KEYS: &[&str] = &[
    "shape", "size", "radius", "height", "points", "vertices", "triangles", "columns", "rows", "cell", "heights", "offset", "is_trigger",
    "layer", "mask", "friction", "restitution",
];

/// `collider`: `{shape, ...}` or `false` to remove it (see the world_spawn schema).
pub(crate) fn collider_arg(args: &Json) -> Result<Option<Option<Collider>>, String> {
    const ERR: &str = "'collider' must be false or {shape: box|sphere|capsule|hull|mesh|heightfield, ...}";
    let c = match args.get("collider") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Bool(false)) => return Ok(Some(None)),
        Some(c @ Json::Obj(_)) => c,
        Some(_) => return Err(ERR.into()),
    };
    check_object_keys(c, "collider", COLLIDER_KEYS)?;
    let size = |key: &str, default: Option<f32>| -> Result<f32, String> {
        match opt_num(c, key, &format!("collider.{key}"))? {
            Some(v) => Ok(v),
            None => default.ok_or_else(|| format!("'collider.{key}' is required (metres)")),
        }
    };
    let points = |key: &str| -> Result<Vec<Vec3>, String> {
        let field = format!("collider.{key}");
        let list = c.get(key).and_then(Json::as_array).ok_or_else(|| format!("'{field}' must be an array of [x, y, z] points"))?;
        if list.len() > MAX_MESH_VERTICES {
            return Err(format!("'{field}': at most {MAX_MESH_VERTICES} points"));
        }
        list.iter().map(|p| point(p, &field)).collect()
    };
    let shape = match c.get("shape").and_then(Json::as_str) {
        Some("box") => ColliderShape::Box { size: opt_point(c, "size", "collider.size")?.unwrap_or(Vec3::ONE) },
        Some("sphere") => ColliderShape::Sphere { radius: size("radius", Some(0.5))? },
        Some("capsule") => ColliderShape::Capsule { radius: size("radius", None)?, height: size("height", None)? },
        Some("hull") => ColliderShape::Hull { points: Arc::new(points("points")?) },
        Some("mesh") => {
            let vertices = points("vertices")?;
            let list = c.get("triangles").and_then(Json::as_array).ok_or("'collider.triangles' must be an array of [a, b, c] vertex indices")?;
            if list.len() > MAX_MESH_TRIANGLES {
                return Err(format!("'collider.triangles': at most {MAX_MESH_TRIANGLES}"));
            }
            let triangles = list
                .iter()
                .map(|t| match t.as_array() {
                    Some([a, b, c]) => match (a.as_u64(), b.as_u64(), c.as_u64()) {
                        (Some(a), Some(b), Some(c)) if a.max(b).max(c) < u64::from(u32::MAX) => Ok([a as u32, b as u32, c as u32]),
                        _ => Err("'collider.triangles' entries must be [a, b, c] vertex indices".to_string()),
                    },
                    _ => Err("'collider.triangles' entries must be [a, b, c] vertex indices".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            ColliderShape::Mesh { vertices: Arc::new(vertices), triangles: Arc::new(triangles) }
        }
        Some("heightfield") => {
            let count = |key: &str| {
                c.get(key)
                    .and_then(Json::as_u64)
                    .filter(|n| *n <= u64::from(MAX_HEIGHTFIELD_SIDE))
                    .map(|n| n as u32)
                    .ok_or_else(|| format!("'collider.{key}' must be a whole number 2..{MAX_HEIGHTFIELD_SIDE}"))
            };
            let (columns, rows) = (count("columns")?, count("rows")?);
            let cell = match c.get("cell") {
                None | Some(Json::Null) => [1.0, 1.0],
                Some(v) => match v.as_array() {
                    Some([x, z]) => [num(x, "collider.cell")?, num(z, "collider.cell")?],
                    _ => match v.as_f64() {
                        Some(_) => {
                            let s = num(v, "collider.cell")?;
                            [s, s]
                        }
                        None => return Err("'collider.cell' must be a number or [x, z] (metres)".into()),
                    },
                },
            };
            let list = c.get("heights").and_then(Json::as_array).ok_or("'collider.heights' must be an array of numbers")?;
            if list.len() > (MAX_HEIGHTFIELD_SIDE * MAX_HEIGHTFIELD_SIDE) as usize {
                return Err("'collider.heights': too many numbers".into());
            }
            let heights = list.iter().map(|h| num(h, "collider.heights")).collect::<Result<Vec<_>, _>>()?;
            ColliderShape::Heightfield { columns, rows, cell, heights: Arc::new(heights) }
        }
        _ => return Err(ERR.into()),
    };
    let mask = match c.get("mask") {
        None | Some(Json::Null) => None,
        Some(v) => {
            let list = v.as_array().ok_or("'collider.mask' must be an array of layer names")?;
            if list.len() > MAX_MASK_LAYERS {
                return Err(format!("'collider.mask' lists at most {MAX_MASK_LAYERS} layers"));
            }
            let mut names: Vec<String> =
                list.iter().map(|l| l.as_str().map(str::to_owned).ok_or("'collider.mask' must be an array of layer names")).collect::<Result<_, _>>()?;
            names.sort();
            names.dedup();
            Some(names)
        }
    };
    let layer = match c.get("layer") {
        None | Some(Json::Null) => "default".to_owned(),
        Some(v) => v.as_str().ok_or("'collider.layer' must be a layer name")?.to_owned(),
    };
    Ok(Some(Some(Collider {
        shape,
        offset: opt_point(c, "offset", "collider.offset")?.unwrap_or(Vec3::ZERO),
        is_trigger: opt_bool(c, "is_trigger", "collider.is_trigger")?.unwrap_or(false),
        layer,
        mask,
        friction: opt_num(c, "friction", "collider.friction")?.unwrap_or(DEFAULT_FRICTION),
        restitution: opt_num(c, "restitution", "collider.restitution")?.unwrap_or(0.0),
    })))
}

const BODY_KEYS: &[&str] = &[
    "kind",
    "mass",
    "velocity",
    "angular_velocity",
    "linear_damping",
    "angular_damping",
    "gravity_scale",
    "sleeping",
    "ccd",
    "lock_position",
    "lock_rotation",
    "character",
];
const CHARACTER_KEYS: &[&str] = &["step_height", "max_slope_deg", "snap_distance", "push_force", "gravity"];

/// `body`: `"static"|"kinematic"|"dynamic"`, `{kind, ...}`, or `false` to remove it.
pub(crate) fn body_arg(args: &Json) -> Result<Option<Option<Body>>, String> {
    const ERR: &str = "'body' must be false, \"static\", \"kinematic\", \"dynamic\" or {kind, mass?, velocity?, ...}";
    let b = match args.get("body") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Bool(false)) => return Ok(Some(None)),
        Some(Json::Str(k)) => return BodyKind::parse(k).map(|k| Some(Some(Body::new(k, 1.0)))).ok_or_else(|| ERR.into()),
        Some(b @ Json::Obj(_)) => b,
        Some(_) => return Err(ERR.into()),
    };
    check_object_keys(b, "body", BODY_KEYS)?;
    let kind = b.get("kind").and_then(Json::as_str).and_then(BodyKind::parse).ok_or(ERR)?;
    let mut body = Body::new(kind, opt_num(b, "mass", "body.mass")?.unwrap_or(1.0));
    body.velocity = opt_point(b, "velocity", "body.velocity")?.unwrap_or(Vec3::ZERO);
    body.angular_velocity = opt_point(b, "angular_velocity", "body.angular_velocity")?.unwrap_or(Vec3::ZERO);
    body.linear_damping = opt_num(b, "linear_damping", "body.linear_damping")?.unwrap_or(0.0);
    body.angular_damping = opt_num(b, "angular_damping", "body.angular_damping")?.unwrap_or(DEFAULT_ANGULAR_DAMPING);
    body.gravity_scale = opt_num(b, "gravity_scale", "body.gravity_scale")?.unwrap_or(1.0);
    body.sleeping = opt_bool(b, "sleeping", "body.sleeping")?.unwrap_or(false);
    body.ccd = opt_bool(b, "ccd", "body.ccd")?.unwrap_or(true);
    if let Some(v) = b.get("lock_position").filter(|v| !v.is_null()) {
        body.lock_position = axes_arg(v, "body.lock_position")?;
    }
    if let Some(v) = b.get("lock_rotation").filter(|v| !v.is_null()) {
        body.lock_rotation = axes_arg(v, "body.lock_rotation")?;
    }
    body.character = match b.get("character") {
        None | Some(Json::Null) | Some(Json::Bool(false)) => None,
        Some(Json::Bool(true)) => Some(CharacterSpec::default()),
        Some(c @ Json::Obj(_)) => {
            check_object_keys(c, "body.character", CHARACTER_KEYS)?;
            let d = CharacterSpec::default();
            Some(CharacterSpec {
                step_height: opt_num(c, "step_height", "body.character.step_height")?.unwrap_or(d.step_height),
                max_slope_deg: opt_num(c, "max_slope_deg", "body.character.max_slope_deg")?.unwrap_or(d.max_slope_deg),
                snap_distance: opt_num(c, "snap_distance", "body.character.snap_distance")?.unwrap_or(d.snap_distance),
                push_force: opt_num(c, "push_force", "body.character.push_force")?.unwrap_or(d.push_force),
                gravity: opt_bool(c, "gravity", "body.character.gravity")?.unwrap_or(d.gravity),
            })
        }
        Some(_) => return Err("'body.character' must be true, false or {step_height?, max_slope_deg?, snap_distance?, push_force?, gravity?}".into()),
    };
    Ok(Some(Some(body)))
}

// ---------------------------------------------------------------------------------------------
// Settings (world_env {physics})

/// The world's physics settings: simulation state (saved, hashed, journaled with the
/// environment).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsSettings {
    pub gravity: [f32; 3],
    /// Solver substeps per 1/60 s (scaled to the tick rate).
    pub substeps: u32,
    pub sleep: bool,
    /// 2D: dynamic bodies stay in the XY plane (Specula's 2D plane) and turn about Z only.
    pub plane_2d: bool,
    /// Contact stiffness, Hz.
    pub contact_hertz: f32,
    pub enabled: bool,
}

impl Default for PhysicsSettings {
    fn default() -> PhysicsSettings {
        let d = specula_physics::Settings::default();
        PhysicsSettings {
            gravity: [d.gravity.x, d.gravity.y, d.gravity.z],
            substeps: d.substeps,
            sleep: d.sleep,
            plane_2d: d.plane_2d,
            contact_hertz: d.contact_hertz,
            enabled: true,
        }
    }
}

/// A partial update to [`PhysicsSettings`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicsPatch {
    pub gravity: Option<[f32; 3]>,
    pub substeps: Option<u32>,
    pub sleep: Option<bool>,
    pub plane_2d: Option<bool>,
    pub contact_hertz: Option<f32>,
    pub enabled: Option<bool>,
}

const SETTING_KEYS: &[&str] = &["gravity", "substeps", "sleep", "plane_2d", "contact_hertz", "enabled"];

impl PhysicsPatch {
    pub fn parse(v: &Json) -> Result<PhysicsPatch, String> {
        check_object_keys(v, "physics", SETTING_KEYS)?;
        let gravity = opt_point(v, "gravity", "physics.gravity")?.map(|g| [g.x, g.y, g.z]);
        let substeps = match v.get("substeps") {
            None | Some(Json::Null) => None,
            Some(n) => Some(n.as_u64().filter(|n| (1..=16).contains(n)).ok_or("'physics.substeps' must be a whole number 1..16")? as u32),
        };
        Ok(PhysicsPatch {
            gravity,
            substeps,
            sleep: opt_bool(v, "sleep", "physics.sleep")?,
            plane_2d: opt_bool(v, "plane_2d", "physics.plane_2d")?,
            contact_hertz: opt_num(v, "contact_hertz", "physics.contact_hertz")?,
            enabled: opt_bool(v, "enabled", "physics.enabled")?,
        })
    }

    /// `Err((field, message))` for the first value out of range.
    pub fn check(&self) -> Result<(), (String, String)> {
        if let Some(g) = self.gravity {
            if !g.iter().all(|c| c.is_finite() && c.abs() <= 1000.0) {
                return Err(("physics.gravity".into(), "'physics.gravity' components must be within ±1000 m/s²".into()));
            }
        }
        if let Some(h) = self.contact_hertz {
            if !(h.is_finite() && (1.0..=1000.0).contains(&h)) {
                return Err(("physics.contact_hertz".into(), "'physics.contact_hertz' must be 1..1000".into()));
            }
        }
        Ok(())
    }

    pub fn apply(&self, s: &mut PhysicsSettings) {
        if let Some(v) = self.gravity {
            s.gravity = v;
        }
        if let Some(v) = self.substeps {
            s.substeps = v;
        }
        if let Some(v) = self.sleep {
            s.sleep = v;
        }
        if let Some(v) = self.plane_2d {
            s.plane_2d = v;
        }
        if let Some(v) = self.contact_hertz {
            s.contact_hertz = v;
        }
        if let Some(v) = self.enabled {
            s.enabled = v;
        }
    }

    pub fn to_json(s: &PhysicsSettings) -> Json {
        Json::obj([
            ("gravity", Json::arr(s.gravity.map(Json::from))),
            ("substeps", Json::from(s.substeps)),
            ("sleep", Json::from(s.sleep)),
            ("plane_2d", Json::from(s.plane_2d)),
            ("contact_hertz", Json::from(s.contact_hertz)),
            ("enabled", Json::from(s.enabled)),
        ])
    }

    /// Hashes nothing while the settings are all defaults, so older worlds keep their hash.
    pub fn hash_if_set(s: &PhysicsSettings, h: &mut StableHasher) {
        if *s != PhysicsSettings::default() {
            h.write_str("physics");
            s.gravity.iter().for_each(|g| h.write_f32(*g));
            h.write_u32(s.substeps);
            h.write_bool(s.sleep);
            h.write_bool(s.plane_2d);
            h.write_f32(s.contact_hertz);
            h.write_bool(s.enabled);
        }
    }
}

/// The world's settings as the space's, for a tick rate of `hz`. `substeps` counts per 1/60 s,
/// so a slower tick (the engine's 30 Hz without a game) takes proportionally more and the
/// solver sees the same substep length (as stiff) at any tick rate.
pub fn space_settings(s: &PhysicsSettings, hz: u32) -> specula_physics::Settings {
    let per_tick = (s.substeps * 60).div_ceil(hz.max(1)).clamp(1, 64);
    specula_physics::Settings {
        gravity: Vec3::new(s.gravity[0], s.gravity[1], s.gravity[2]),
        substeps: per_tick,
        sleep: s.sleep,
        plane_2d: s.plane_2d,
        contact_hertz: s.contact_hertz,
        ..specula_physics::Settings::default()
    }
}

// ---------------------------------------------------------------------------------------------
// The engine's physics: world <-> space

use crate::world::{EntityKey, Recording, UpdateSpec, World};
use specula_core::math::Quat;
use specula_physics::{BodyDesc, ColliderDesc, Event, EventKind, Pose, Shape, Space, StepReport};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Per-tick physics hashes kept for world_hash.
pub const HASHES: usize = 1024;
/// Events kept for physics_events (oldest dropped first).
pub const MAX_EVENTS: usize = 4096;
/// Ticks of check records kept (physics_check's window).
pub const CHECK_WINDOW: usize = 1800;
/// A kinematic body moved by hand farther than this (m) in one tick teleports instead of
/// sweeping (and pushing) its way there.
pub const TELEPORT_DISTANCE: f32 = 2.0;
/// Layer names physics tells apart (bits of a 64-bit mask).
pub const MAX_LAYERS: usize = 64;
/// Built shapes kept for reuse (hulls and meshes are costly to build).
const SHAPE_CACHE: usize = 1024;
/// Entities listed as not simulated (with why) at most.
const MAX_PROBLEMS: usize = 256;
/// The agent name physics writes under.
pub const AGENT: &str = "physics";

/// What physics last saw of an entity (so unchanged entities are skipped).
#[derive(Clone, Debug)]
struct Seen {
    version: u64,
    world: [[f32; 4]; 4],
    /// Collider, scale and the body's fixed parameters: unchanged means only pose or motion did.
    shape_key: u64,
    body: Option<Body>,
    /// The scale the body's collider was built at (its world matrix's column lengths then: they
    /// jitter in the last bit as the body turns, and the space keeps the shape it was built
    /// with, so save games carry it).
    scale: Vec3,
}

/// One logged event.
#[derive(Clone, Copy, Debug)]
pub struct Logged {
    pub seq: u64,
    pub tick: u64,
    pub event: Event,
}

/// One tick's numbers for the checks.
#[derive(Clone, Copy, Debug)]
pub struct TickRecord {
    pub tick: u64,
    /// `(separation, a, b)`.
    pub deepest: Option<(f32, u64, u64)>,
    pub energy_change: f64,
    pub fast: usize,
    pub ccd: usize,
    pub awake: usize,
    /// Joules: kinetic energy of the awake bodies.
    pub kinetic: f64,
    /// The bodies that gained the most energy: `(id, joules)`.
    pub gainers: [(u64, f64); 2],
}

/// The engine's physics: the space and its bridge to the world.
pub struct Physics {
    pub space: Space,
    seen: BTreeMap<u64, Seen>,
    layers: BTreeMap<String, u32>,
    /// `(world generation, draw epoch)` synced up to.
    synced: Option<(u64, u64)>,
    /// Entities to look at again next sync (children of moved bodies).
    pending: BTreeSet<u64>,
    /// write_back's list of bodies, reused.
    keys: Vec<u64>,
    shapes: BTreeMap<u64, Shape>,
    /// Character entities and the velocity their last move left.
    characters: BTreeMap<u64, Vec3>,
    pub hashes: VecDeque<(u64, u64)>,
    pub events: VecDeque<Logged>,
    next_event: u64,
    /// Entities not simulated, and why.
    pub problems: BTreeMap<u64, Json>,
    pub window: VecDeque<TickRecord>,
    /// The last step's report and the tick it ran at.
    pub last: Option<(u64, StepReport)>,
    /// Budget errors of recent steps (newest last, at most 16).
    pub errors: VecDeque<Json>,
    /// Wall-clock milliseconds of the last few ticks (sync + characters + step + write-back).
    pub tick_ms: VecDeque<f64>,
    /// The world's settings reached the space once (later changes wake sleeping bodies).
    settings_applied: bool,
}

impl Default for Physics {
    fn default() -> Physics {
        Physics {
            space: Space::default(),
            seen: BTreeMap::new(),
            layers: BTreeMap::new(),
            synced: None,
            pending: BTreeSet::new(),
            keys: Vec::new(),
            shapes: BTreeMap::new(),
            characters: BTreeMap::new(),
            hashes: VecDeque::new(),
            events: VecDeque::new(),
            next_event: 1,
            problems: BTreeMap::new(),
            window: VecDeque::new(),
            last: None,
            errors: VecDeque::new(),
            tick_ms: VecDeque::new(),
            settings_applied: false,
        }
    }
}

/// Whether an entity takes part in physics.
pub fn relevant(e: &world::Entity) -> bool {
    !e.transient && (e.collider.is_some() || e.body.is_some())
}

fn mat_cols(m: &specula_core::math::Mat4) -> [[f32; 4]; 4] {
    m.cols
}

fn same_bits(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> bool {
    a.iter().flatten().zip(b.iter().flatten()).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// An entity's world rotation (its own Euler angles composed up the parent chain, through
/// `dmath`).
pub fn world_rotation(world: &World, e: &world::Entity) -> Quat {
    let r = e.rotation_deg;
    let mut q = Quat::from_euler_xyz_deg(r.x, r.y, r.z);
    let mut at = e.parent;
    while let Some(id) = at {
        let Some(p) = world.get(id) else { break };
        let r = p.rotation_deg;
        q = Quat::from_euler_xyz_deg(r.x, r.y, r.z) * q;
        at = p.parent;
    }
    q.normalize()
}

/// An entity's world pose and scale (scale from the world matrix's column lengths).
pub fn world_pose(world: &World, e: &world::Entity) -> (Pose, Vec3) {
    let m = e.model();
    let col = |i: usize| Vec3::new(m.cols[i][0], m.cols[i][1], m.cols[i][2]).length();
    let p = Vec3::new(m.cols[3][0], m.cols[3][1], m.cols[3][2]);
    (Pose { p, q: world_rotation(world, e) }, Vec3::new(col(0), col(1), col(2)))
}

fn mul3(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x * b.x, a.y * b.y, a.z * b.z)
}

/// The collider's shape, scaled.
fn build_shape(c: &Collider, s: Vec3) -> Result<Shape, String> {
    let max_xz = s.x.max(s.z);
    let shape = match &c.shape {
        ColliderShape::Box { size } => Shape::cuboid(mul3(*size, s) * 0.5),
        ColliderShape::Sphere { radius } => Shape::sphere(radius * s.x.max(s.y).max(s.z)),
        ColliderShape::Capsule { radius, height } => {
            let r = radius * max_xz;
            Shape::capsule(r, (height * s.y * 0.5 - r).max(0.0))
        }
        ColliderShape::Hull { points } => {
            let pts: Vec<Vec3> = points.iter().map(|p| mul3(*p, s)).collect();
            Shape::hull(&pts).map_err(|e| e.message)?
        }
        ColliderShape::Mesh { vertices, triangles } => {
            Shape::mesh(vertices.iter().map(|p| mul3(*p, s)).collect(), triangles.to_vec()).map_err(|e| e.message)?
        }
        ColliderShape::Heightfield { columns, rows, cell, heights } => Shape::heightfield(
            *columns as usize,
            *rows as usize,
            heights.iter().map(|h| h * s.y).collect(),
            (cell[0] * s.x, cell[1] * s.z),
        )
        .map_err(|e| e.message)?,
    };
    Ok(shape)
}

impl Physics {
    /// Forgets everything (a new world).
    pub fn reset(&mut self) {
        *self = Physics::default();
    }

    /// The bit of a layer name (allocated on first use; "default" is bit 0).
    fn layer_bit(&mut self, name: &str) -> Result<u64, String> {
        if self.layers.is_empty() {
            self.layers.insert("default".into(), 0);
        }
        if let Some(b) = self.layers.get(name) {
            return Ok(1u64 << b);
        }
        if self.layers.len() >= MAX_LAYERS {
            return Err(format!("physics tells apart at most {MAX_LAYERS} layer names; '{name}' is one too many"));
        }
        let bit = self.layers.len() as u32;
        self.layers.insert(name.to_owned(), bit);
        Ok(1u64 << bit)
    }

    /// The bits of layer names (unknown names get bits too, so masks may name layers that
    /// arrive later).
    pub fn mask_bits(&mut self, names: Option<&[String]>) -> Result<u64, String> {
        match names {
            None => Ok(u64::MAX),
            Some(list) => list.iter().try_fold(0u64, |m, n| self.layer_bit(n).map(|b| m | b)),
        }
    }

    /// The bit of a known layer name (0 for names no collider uses: queries on them hit nothing).
    pub fn layer_mask_of(&self, name: &str) -> u64 {
        match self.layers.get(name) {
            Some(b) => 1u64 << b,
            None if name == "default" => 1,
            None => 0,
        }
    }

    /// Layer names by bit, for reports.
    pub fn layers_json(&self) -> Json {
        let mut by_bit: Vec<(&u32, &String)> = self.layers.iter().map(|(n, b)| (b, n)).collect();
        by_bit.sort();
        Json::arr(by_bit.into_iter().map(|(b, n)| Json::obj([("name", Json::from(n.as_str())), ("bit", Json::from(*b))])))
    }

    fn shape_key(c: Option<&Collider>, scale: Vec3, b: Option<&Body>) -> u64 {
        let mut h = StableHasher::with_seed(0x7368_6170_6531);
        if let Some(c) = c {
            h.write_u64(c.field_hash());
        }
        [scale.x, scale.y, scale.z].iter().for_each(|v| h.write_f32(*v));
        if let Some(b) = b {
            // What can't change without rebuilding the body (motion state can).
            h.write_str(b.kind.name());
            h.write_f32(b.mass);
            b.hash_fixed(&mut h);
        }
        h.finish()
    }

    fn shape_for(&mut self, c: &Collider, scale: Vec3) -> Result<Shape, String> {
        let mut h = StableHasher::with_seed(0x7368_6170_6532);
        h.write_u64(c.field_hash());
        [scale.x, scale.y, scale.z].iter().for_each(|v| h.write_f32(*v));
        let key = h.finish();
        if let Some(s) = self.shapes.get(&key) {
            return Ok(s.clone());
        }
        let s = build_shape(c, scale)?;
        if self.shapes.len() >= SHAPE_CACHE {
            self.shapes.clear();
        }
        self.shapes.insert(key, s.clone());
        Ok(s)
    }

    fn note_problem(&mut self, world: &World, id: u64, error: &str) {
        self.space.remove(id);
        self.seen.remove(&id);
        self.characters.remove(&id);
        if self.problems.len() < MAX_PROBLEMS || self.problems.contains_key(&id) {
            self.problems.insert(id, Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id))), ("error", Json::from(error))]));
        }
    }

    fn forget(&mut self, id: u64) {
        self.space.remove(id);
        self.seen.remove(&id);
        self.characters.remove(&id);
        self.problems.remove(&id);
    }

    /// Brings one entity's physics up to date with the world.
    fn upsert(&mut self, world: &World, e: &world::Entity) {
        self.upsert_scaled(world, e, None);
    }

    /// [`Physics::upsert`]; a body (re)built here takes its collider at `built_scale` when given
    /// (a save game's, see `physics_save`) instead of the world matrix's.
    fn upsert_scaled(&mut self, world: &World, e: &world::Entity, built_scale: Option<Vec3>) {
        let id = e.id;
        if !relevant(e) {
            self.forget(id);
            return;
        }
        let cols = mat_cols(&e.model());
        if let Some(s) = self.seen.get(&id) {
            if s.version == e.version && same_bits(&s.world, &cols) {
                return;
            }
        }
        let (pose, scale) = world_pose(world, e);
        let scale = built_scale.unwrap_or(scale);
        let key = Physics::shape_key(e.collider.as_ref(), scale, e.body.as_ref());
        let body = e.body.unwrap_or_else(|| Body::new(BodyKind::Static, 1.0));
        let seen = self.seen.get(&id).cloned();
        // Only a rebuild takes the new scale; a pose or motion change keeps the built shape's.
        let built = match &seen {
            Some(s) if s.shape_key == key && self.space.contains(id) => s.scale,
            _ => scale,
        };
        match seen {
            Some(s) if s.shape_key == key && self.space.contains(id) => {
                // Only the pose or the motion changed.
                if !same_bits(&s.world, &cols) {
                    let old = self.space.body(id).map(|b| b.pose.p).unwrap_or(pose.p);
                    if body.kind == BodyKind::Kinematic && body.character.is_none() && (pose.p - old).length() <= TELEPORT_DISTANCE {
                        self.space.move_kinematic(id, pose);
                    } else {
                        self.space.set_pose(id, pose);
                    }
                }
                let prev = s.body.unwrap_or(body);
                if prev.velocity != body.velocity || prev.angular_velocity != body.angular_velocity {
                    self.space.set_velocity(id, body.velocity, body.angular_velocity);
                    if body.character.is_some() {
                        self.characters.insert(id, body.velocity);
                    }
                }
                if body.sleeping != prev.sleeping && !body.sleeping {
                    self.space.wake(id);
                }
            }
            _ => {
                let collider = match &e.collider {
                    None => None,
                    Some(c) => {
                        let shape = match self.shape_for(c, scale) {
                            Ok(s) => s,
                            Err(m) => return self.note_problem(world, id, &format!("collider: {m}")),
                        };
                        let layer = match self.layer_bit(&c.layer) {
                            Ok(b) => b,
                            Err(m) => return self.note_problem(world, id, &m),
                        };
                        let mask = match self.mask_bits(c.mask.as_deref()) {
                            Ok(b) => b,
                            Err(m) => return self.note_problem(world, id, &m),
                        };
                        Some(ColliderDesc {
                            shape,
                            offset: Pose::at(mul3(c.offset, scale)),
                            is_trigger: c.is_trigger,
                            layer,
                            mask,
                            friction: c.friction,
                            restitution: c.restitution,
                        })
                    }
                };
                let kind = match body.kind {
                    BodyKind::Static => specula_physics::BodyKind::Static,
                    BodyKind::Kinematic => specula_physics::BodyKind::Kinematic,
                    BodyKind::Dynamic => specula_physics::BodyKind::Dynamic,
                };
                let mut d = BodyDesc::new(kind, pose, collider);
                d.velocity = body.velocity;
                d.angular_velocity = body.angular_velocity;
                d.mass = body.mass;
                d.linear_damping = body.linear_damping;
                d.angular_damping = body.angular_damping;
                d.gravity_scale = body.gravity_scale;
                d.lock_linear = body.lock_position;
                d.lock_angular = body.lock_rotation;
                d.sleeping = body.sleeping && body.kind == BodyKind::Dynamic;
                d.ccd = body.ccd;
                if body.character.is_some() {
                    // A character moves by its controller, not by its velocity.
                    d.velocity = Vec3::ZERO;
                    d.angular_velocity = Vec3::ZERO;
                }
                if let Err(err) = self.space.insert(id, d) {
                    return self.note_problem(world, id, &err.message);
                }
                if body.character.is_some() {
                    self.characters.insert(id, body.velocity);
                } else {
                    self.characters.remove(&id);
                }
                self.problems.remove(&id);
            }
        }
        self.seen.insert(id, Seen { version: e.version, world: cols, shape_key: key, body: e.body, scale: built });
    }

    /// Syncs what changed in the world since the last sync (or everything).
    pub fn sync(&mut self, world: &World, generation: u64) {
        let epoch = world.draw_epoch();
        let changed: Option<Vec<u64>> = match self.synced {
            Some((g, e)) if g == generation => world.draw_changes_since(e).map(|it| it.collect()),
            Some((g, _)) if g != generation => {
                self.reset();
                None
            }
            _ => None,
        };
        match changed {
            Some(mut ids) => {
                ids.extend(std::mem::take(&mut self.pending));
                ids.sort_unstable();
                ids.dedup();
                for id in ids {
                    match world.get(id) {
                        Some(e) => self.upsert(world, e),
                        None => self.forget(id),
                    }
                }
            }
            None => {
                // The change log doesn't reach back: compare everything (state survives).
                self.pending.clear();
                let mut live = BTreeSet::new();
                for e in world.persistent_entities() {
                    if relevant(e) {
                        live.insert(e.id);
                        self.upsert(world, e);
                    }
                }
                let gone: Vec<u64> = self.seen.keys().filter(|k| !live.contains(k)).copied().collect();
                for id in gone {
                    self.forget(id);
                }
                let stale: Vec<u64> = self.problems.keys().filter(|k| !live.contains(k)).copied().collect();
                for id in stale {
                    self.problems.remove(&id);
                }
            }
        }
        self.synced = Some((generation, epoch));
    }

    /// Characters walk (before the step, like game code moving kinematic bodies).
    fn move_characters(&mut self, world: &World, dt: f32) {
        let chars: Vec<(u64, Vec3)> = self.characters.iter().map(|(k, v)| (*k, *v)).collect();
        for (id, v) in chars {
            let Some(cfg) = world.get(id).and_then(|e| e.body).and_then(|b| b.character) else { continue };
            match self.space.move_character(id, v, &cfg.config(), dt) {
                Ok(m) => {
                    self.characters.insert(id, m.velocity);
                }
                Err(e) => {
                    let msg = e.message.clone();
                    self.note_problem(world, id, &msg);
                }
            }
        }
    }

    /// Writes the bodies the step moved back into the world (journaled by the caller).
    fn write_back(&mut self, world: &mut World) {
        let mut keys = std::mem::take(&mut self.keys);
        keys.clear();
        keys.extend(self.space.stepped().chain(self.characters.keys().copied()));
        keys.sort_unstable();
        keys.dedup();
        for &id in &keys {
            let Some(view) = self.space.body(id) else { continue };
            let Some(e) = world.get(id) else { continue };
            let Some(body) = e.body else { continue };
            let character = body.character.is_some();
            // Kinematic bodies driven by the world (game code moving them) stay as the world has them.
            if body.kind == BodyKind::Kinematic && !character && body.velocity == Vec3::ZERO && body.angular_velocity == Vec3::ZERO {
                continue;
            }
            let m = e.model();
            let at = Vec3::new(m.cols[3][0], m.cols[3][1], m.cols[3][2]);
            let new_body = if character {
                Body { velocity: self.characters.get(&id).copied().unwrap_or(body.velocity), ..body }
            } else if body.kind == BodyKind::Dynamic {
                Body { velocity: view.velocity, angular_velocity: view.angular_velocity, sleeping: view.sleeping, ..body }
            } else {
                body
            };
            // (The world rotation, which costs trigonometry, only when the position is unchanged.)
            let moved = view.pose.p.x.to_bits() != at.x.to_bits()
                || view.pose.p.y.to_bits() != at.y.to_bits()
                || view.pose.p.z.to_bits() != at.z.to_bits()
                || specula_physics::math::quat_dot(view.pose.q, world_rotation(world, e)).abs() < 1.0 - 1.0e-7;
            if !moved && new_body == body {
                continue;
            }
            let mut change = UpdateSpec { body: (new_body != body).then_some(Some(new_body)), ..Default::default() };
            if moved {
                // Into the parent's frame.
                let (local_p, local_q) = match e.parent.and_then(|p| world.get(p)) {
                    Some(parent) => {
                        let inv = parent.model().inverse().unwrap_or(specula_core::math::Mat4::IDENTITY);
                        let pq = world_rotation(world, parent);
                        (inv.transform_point(view.pose.p), (pq.conjugate() * view.pose.q).normalize())
                    }
                    None => (view.pose.p, view.pose.q),
                };
                change.position = Some(local_p);
                change.rotation_deg = Some(specula_physics::math::quat_to_euler_deg(local_q));
            }
            let has_children = world.has_children(id);
            match world.update(&EntityKey::Id(id), change, AGENT) {
                Ok(e) => {
                    let (version, cols) = (e.version, mat_cols(&e.model()));
                    if let Some(s) = self.seen.get_mut(&id) {
                        s.version = version;
                        s.world = cols;
                        s.body = e.body;
                    }
                    if has_children {
                        let mut stack: Vec<u64> = world.children_of(Some(id)).collect();
                        while let Some(c) = stack.pop() {
                            if world.get(c).is_some_and(relevant) {
                                self.pending.insert(c);
                            }
                            stack.extend(world.children_of(Some(c)));
                        }
                    }
                }
                Err(err) => {
                    let msg = format!("physics could not write back: {}", err.message);
                    self.note_problem(world, id, &msg);
                }
            }
        }
        self.keys = keys;
    }

    /// One fixed tick: sync, characters, step, write-back. The caller records changes as a
    /// snapshot and journals them.
    pub fn tick(&mut self, world: &mut World, generation: u64, settings: &PhysicsSettings, tick: u64, dt: f32) {
        let started = std::time::Instant::now();
        // Simulation input: dmath, not the platform's roundf (DETERMINISM.md rule 4).
        let hz = specula_core::dmath::round(1.0 / dt) as u32;
        let want = space_settings(settings, hz);
        if self.space.settings != want {
            // New gravity, substeps or 2D mode: resting bodies must feel it.
            let wake = self.space.settings.gravity != want.gravity || self.space.settings.plane_2d != want.plane_2d || self.space.settings.substeps != want.substeps || self.space.settings.sleep != want.sleep;
            self.space.settings = want;
            // (Not on the first tick: a loaded world's resting bodies start asleep.)
            if wake && self.settings_applied {
                self.space.wake_all();
            }
        }
        self.settings_applied = true;
        self.sync(world, generation);
        if self.space.is_empty() {
            // Report the empty state's hash once (world_hash must not show a stale one).
            let empty = self.space.hash();
            if self.hashes.back().is_some_and(|(_, h)| *h != empty) {
                if self.hashes.len() == HASHES {
                    self.hashes.pop_front();
                }
                self.hashes.push_back((tick, empty));
                self.last = None;
            }
            return;
        }
        self.move_characters(world, dt);
        let report = self.space.step(dt).clone();
        self.write_back(world);
        // Physics' own writes are known: skip them in the change log next sync.
        self.synced = Some((generation, world.draw_epoch()));
        for ev in &report.events {
            if self.events.len() == MAX_EVENTS {
                self.events.pop_front();
            }
            self.events.push_back(Logged { seq: self.next_event, tick, event: *ev });
            self.next_event += 1;
        }
        for e in &report.errors {
            if self.errors.len() == 16 {
                self.errors.pop_front();
            }
            let mut j = e.to_json();
            j.set("tick", Json::from(tick));
            self.errors.push_back(j);
        }
        if self.window.len() == CHECK_WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(TickRecord {
            tick,
            deepest: report.deepest,
            energy_change: report.energy_change,
            fast: report.fast_total,
            ccd: report.events.iter().filter(|e| e.kind == EventKind::Ccd).count(),
            awake: report.awake,
            kinetic: report.kinetic_energy,
            gainers: [0, 1].map(|i| report.energy_gainers.get(i).copied().unwrap_or((0, 0.0))),
        });
        if self.hashes.len() == HASHES {
            self.hashes.pop_front();
        }
        self.hashes.push_back((tick, self.space.hash()));
        self.last = Some((tick, report));
        if self.tick_ms.len() == 120 {
            self.tick_ms.pop_front();
        }
        self.tick_ms.push_back(started.elapsed().as_secs_f64() * 1000.0);
    }

    /// The physics hash at the end of the last tick (`None` before physics ran).
    pub fn hash(&self) -> Option<u64> {
        self.hashes.back().map(|(_, h)| *h)
    }

    /// physics_info / perf_frame's summary.
    pub fn summary_json(&self, settings: &PhysicsSettings) -> Json {
        let r = self.last.as_ref().map(|(_, r)| r);
        let t = r.map(|r| r.times).unwrap_or_default();
        let us = |v: f64| Json::from((v * 10.0).round() / 10000.0);
        let tick_ms = {
            let mut v: Vec<f64> = self.tick_ms.iter().copied().collect();
            v.sort_by(f64::total_cmp);
            if v.is_empty() {
                Json::Null
            } else {
                let mean = v.iter().sum::<f64>() / v.len() as f64;
                let p95 = v[(v.len() * 95 / 100).min(v.len() - 1)];
                Json::obj([("mean_ms", Json::from((mean * 1000.0).round() / 1000.0)), ("p95_ms", Json::from((p95 * 1000.0).round() / 1000.0))])
            }
        };
        Json::obj([
            ("settings", PhysicsPatch::to_json(settings)),
            ("bodies", Json::from(self.space.len())),
            ("awake", Json::from(r.map_or(0, |r| r.awake))),
            ("sleeping", Json::from(r.map_or(0, |r| r.sleeping))),
            ("characters", Json::from(self.characters.len())),
            ("pairs", Json::from(r.map_or(0, |r| r.pairs))),
            ("touching", Json::from(r.map_or(0, |r| r.touching))),
            ("constraints", Json::from(r.map_or(0, |r| r.constraints))),
            ("tick", tick_ms),
            (
                "stages_ms",
                Json::obj([
                    ("pairs", us(t.pairs_us)),
                    ("narrowphase", us(t.narrowphase_us)),
                    ("solve", us(t.solve_us)),
                    ("kinematic", us(t.integrate_us)),
                    ("ccd", us(t.ccd_us)),
                    ("sleep", us(t.sleep_us)),
                    ("broadphase", us(t.broadphase_us)),
                    ("total", us(t.total_us)),
                ]),
            ),
            ("hash", Json::from(self.hash().map(crate::journal::hex))),
            ("layers", self.layers_json()),
            ("not_simulated", Json::arr(self.problems.values().take(16).cloned())),
            ("not_simulated_total", Json::from(self.problems.len())),
            ("errors", Json::arr(self.errors.iter().cloned())),
            ("checks", self.checks_json()),
        ])
    }

    /// The automatic checks over the recorded window (reported like budget errors).
    pub fn checks_json(&self) -> Json {
        let deepest = self.window.iter().filter_map(|r| r.deepest.map(|d| (d, r.tick))).fold(None, |m: Option<((f32, u64, u64), u64)>, x| match m {
            Some(y) if y.0 .0 <= x.0 .0 => Some(y),
            _ => Some(x),
        });
        let gain = self.window.iter().fold(None, |m: Option<(f64, u64)>, r| match m {
            Some(y) if y.0 >= r.energy_change => Some(y),
            _ => Some((r.energy_change, r.tick)),
        });
        let fast: usize = self.window.iter().map(|r| r.fast).max().unwrap_or(0);
        let ccd: usize = self.window.iter().map(|r| r.ccd).sum();
        Json::obj([
            ("window_ticks", Json::from(self.window.len())),
            (
                "deepest_penetration",
                deepest.map_or(Json::Null, |((d, a, b), t)| Json::obj([("depth_m", Json::from(-d)), ("a", Json::from(a)), ("b", Json::from(b)), ("tick", Json::from(t))])),
            ),
            ("max_energy_gain", gain.map_or(Json::Null, |(e, t)| Json::obj([("joules", Json::from(e)), ("tick", Json::from(t))]))),
            ("max_fast_bodies", Json::from(fast)),
            ("tunnelling_guard_hits", Json::from(ccd)),
        ])
    }
}

impl crate::Engine {
    /// Runs physics for the tick that just finished (after the hosted game's), recording and
    /// journaling what it moved.
    pub(crate) fn physics_tick(&mut self) {
        let settings = self.world.env().physics;
        if !settings.enabled {
            return;
        }
        let tick = self.clock.tick();
        let dt = 1.0 / self.clock.hz() as f32;
        let _z = specula_core::profile::zone("physics");
        self.world.set_recording(Recording::Snapshot);
        self.physics.tick(&mut self.world, self.world_generation, &settings, tick, dt);
        self.world.set_recording(Recording::Full);
        self.swarm.game_changes(&mut self.world, tick, AGENT, crate::journal::Kind::Physics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{Journal, Kind};
    use crate::world::{Shape as WShape, SpawnSpec};

    const DT: f32 = 1.0 / 60.0;

    fn floor(w: &mut World) -> u64 {
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("floor".into());
        s.position = Vec3::new(0.0, -0.5, 0.0);
        s.scale = Vec3::new(20.0, 1.0, 20.0);
        s.collider = Some(Collider::default());
        w.spawn(s, "t").unwrap().id
    }

    fn crate_at(w: &mut World, name: &str, p: Vec3) -> u64 {
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some(name.into());
        s.position = p;
        s.collider = Some(Collider::default());
        s.body = Some(Body::new(BodyKind::Dynamic, 10.0));
        w.spawn(s, "t").unwrap().id
    }

    /// Runs `n` physics ticks the way the engine does (snapshot recording), journaling into `j`.
    fn run(w: &mut World, p: &mut Physics, j: Option<&mut Journal>, n: u64) {
        let settings = w.env().physics;
        let mut j = j;
        for tick in 0..n {
            w.set_recording(Recording::Snapshot);
            p.tick(w, 0, &settings, tick, DT);
            w.set_recording(Recording::Full);
            let changes = w.take_changes();
            if let Some(j) = j.as_deref_mut() {
                j.append_snapshot(tick, AGENT, None, Kind::Physics, changes, w.next_id());
            }
        }
    }

    /// Where an engine physics tick goes with 1,000 awake boxes (review round 2, finding 6). Run
    /// with `cargo test --release -p specula --lib bridge_profile -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bridge_profile() {
        let mut w = World::default();
        let f = floor(&mut w);
        w.update(&EntityKey::Id(f), UpdateSpec { scale: Some(Vec3::new(80.0, 1.0, 80.0)), ..Default::default() }, "t").unwrap();
        for i in 0..1000u32 {
            let (x, y, z) = ((i % 10) as f32 * 1.6 - 8.0, 0.5 + (i / 100) as f32 * 1.02, ((i / 10) % 10) as f32 * 1.6 - 8.0);
            crate_at(&mut w, &format!("c{i}"), Vec3::new(x, y, z));
        }
        let mut settings = w.env().physics;
        settings.sleep = false;
        let mut p = Physics::default();
        p.space.settings = space_settings(&settings, 60);
        let (mut sync, mut step, mut back, mut changes, mut space_stages) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let mut j = Journal::default();
        let n = 120u64;
        for tick in 0..n {
            w.set_recording(Recording::Snapshot);
            let t0 = std::time::Instant::now();
            p.sync(&w, 0);
            let t1 = std::time::Instant::now();
            let report = p.space.step(DT).clone();
            let t2 = std::time::Instant::now();
            p.write_back(&mut w);
            p.synced = Some((0, w.draw_epoch()));
            let _ = p.space.hash();
            let t3 = std::time::Instant::now();
            w.set_recording(Recording::Full);
            let ch = w.take_changes();
            j.append_snapshot(tick, AGENT, None, Kind::Physics, ch, w.next_id());
            let t4 = std::time::Instant::now();
            if tick >= 20 {
                sync += (t1 - t0).as_secs_f64();
                step += (t2 - t1).as_secs_f64();
                back += (t3 - t2).as_secs_f64();
                changes += (t4 - t3).as_secs_f64();
                let t = report.times;
                space_stages += (t.pairs_us + t.narrowphase_us + t.solve_us + t.integrate_us + t.ccd_us) / 1.0e6;
            }
            assert!(tick < 20 || report.awake > 900, "tick {tick}: {} awake", report.awake);
        }
        let ms = |v: f64| v * 1000.0 / (n - 20) as f64;
        println!(
            "1000 awake boxes, per tick: sync {:.3} ms, space step {:.3} ms (stages {:.3}), write-back + hash {:.3} ms, changes + journal {:.3} ms",
            ms(sync),
            ms(step),
            ms(space_stages),
            ms(back),
            ms(changes)
        );
        // The pieces, once over every body.
        let ids: Vec<u64> = p.space.stepped().collect();
        let t = std::time::Instant::now();
        let poses: Vec<Pose> = ids.iter().map(|id| world_pose(&w, w.get(*id).unwrap()).0).collect();
        let world_pose_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let eulers: Vec<Vec3> = poses.iter().map(|p| specula_physics::math::quat_to_euler_deg(p.q)).collect();
        let euler_ms = t.elapsed().as_secs_f64() * 1000.0;
        w.set_recording(Recording::Snapshot);
        let t = std::time::Instant::now();
        for (id, (pose, r)) in ids.iter().zip(poses.iter().zip(&eulers)) {
            let body = w.get(*id).unwrap().body;
            let change = UpdateSpec { position: Some(pose.p), rotation_deg: Some(*r), body: Some(body), ..Default::default() };
            w.update(&EntityKey::Id(*id), change, AGENT).unwrap();
        }
        let update_ms = t.elapsed().as_secs_f64() * 1000.0;
        w.set_recording(Recording::Full);
        let t = std::time::Instant::now();
        let _ = p.space.hash();
        let hash_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("pieces over {} bodies: world_pose {world_pose_ms:.3} ms, quat_to_euler {euler_ms:.3} ms, world.update {update_ms:.3} ms, space hash {hash_ms:.3} ms", ids.len());
    }

    #[test]
    fn a_crate_falls_lands_and_sleeps_in_the_world() {
        let mut w = World::default();
        floor(&mut w);
        let id = crate_at(&mut w, "crate", Vec3::new(0.0, 3.0, 0.0));
        let mut p = Physics::default();
        run(&mut w, &mut p, None, 30);
        let e = w.get(id).unwrap();
        assert!(e.position.y < 3.0 && e.body.unwrap().velocity.y < -1.0, "falling: {:?} {:?}", e.position, e.body);
        assert_eq!(e.modified_by.as_ref(), AGENT);
        run(&mut w, &mut p, None, 240);
        let e = w.get(id).unwrap();
        assert!((e.position.y - 0.5).abs() < 0.02, "resting on the floor: {:?}", e.position);
        let b = e.body.unwrap();
        assert!(b.sleeping && b.velocity == Vec3::ZERO, "{b:?}");
        // Asleep: physics stops writing (the version holds).
        let v = e.version;
        run(&mut w, &mut p, None, 30);
        assert_eq!(w.get(id).unwrap().version, v);
        // An agent moving it wakes it and it falls again.
        w.update(&EntityKey::Id(id), UpdateSpec { position: Some(Vec3::new(0.0, 2.0, 0.0)), ..Default::default() }, "ann").unwrap();
        run(&mut w, &mut p, None, 10);
        let e = w.get(id).unwrap();
        assert!(e.position.y < 2.0 && !e.body.unwrap().sleeping);
    }

    #[test]
    fn physics_changes_replay_from_the_journal_and_runs_are_deterministic() {
        let build = || {
            let mut w = World::default();
            floor(&mut w);
            for i in 0..6 {
                crate_at(&mut w, &format!("c{i}"), Vec3::new((i % 3) as f32 * 0.3, 1.0 + i as f32 * 1.1, (i / 3) as f32 * 0.2));
            }
            w
        };
        let mut a = build();
        let base = a.clone();
        let mut j = Journal::new(&base, "test");
        let mut pa = Physics::default();
        let mut b = build();
        let mut pb = Physics::default();
        for t in 0..200 {
            run(&mut a, &mut pa, Some(&mut j), 1);
            run(&mut b, &mut pb, None, 1);
            assert_eq!(a.state_hash(), b.state_hash(), "world diverged at tick {t}");
            assert_eq!(pa.hash(), pb.hash(), "physics diverged at tick {t}");
        }
        assert_eq!(j.replay().expect("replayable").state_hash(), a.state_hash(), "the journal replays physics exactly");
    }

    #[test]
    fn children_of_moved_parents_and_parented_bodies() {
        let mut w = World::default();
        floor(&mut w);
        let mut g = SpawnSpec::new(WShape::Group);
        g.name = Some("room".into());
        g.position = Vec3::new(5.0, 0.0, 0.0);
        g.rotation_deg = Vec3::new(0.0, 90.0, 0.0);
        let room = w.spawn(g, "t").unwrap().id;
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("box".into());
        s.parent = Some(EntityKey::Id(room));
        s.position = Vec3::new(1.0, 2.0, 0.0);
        s.collider = Some(Collider::default());
        s.body = Some(Body::new(BodyKind::Dynamic, 1.0));
        let id = w.spawn(s, "t").unwrap().id;
        let mut p = Physics::default();
        run(&mut w, &mut p, None, 200);
        let e = w.get(id).unwrap();
        // Lands on the floor, in world space at the room's offset; local coordinates follow.
        let wp = e.world_position();
        assert!((wp.y - 0.5).abs() < 0.02 && (wp.x - 5.0).abs() < 0.05 && (wp.z + 1.0).abs() < 0.05, "{wp:?}");
        assert!((e.position.x - 1.0).abs() < 0.05 && (e.position.y - 0.5).abs() < 0.02, "local {:?}", e.position);
    }

    #[test]
    fn kinematic_bodies_moved_by_game_code_push_and_characters_walk() {
        let mut w = World::default();
        floor(&mut w);
        let bx = crate_at(&mut w, "crate", Vec3::new(1.5, 0.5, 0.0));
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("pusher".into());
        s.position = Vec3::new(0.0, 0.5, 0.0);
        s.collider = Some(Collider::default());
        s.body = Some(Body::new(BodyKind::Kinematic, 1.0));
        let pusher = w.spawn(s, "t").unwrap().id;
        let mut p = Physics::default();
        run(&mut w, &mut p, None, 5);
        for i in 0..90 {
            // Game code moves it 2 cm a tick.
            w.update(&EntityKey::Id(pusher), UpdateSpec { position: Some(Vec3::new(0.02 * i as f32, 0.5, 0.0)), ..Default::default() }, "game").unwrap();
            run(&mut w, &mut p, None, 1);
        }
        assert_eq!(w.get(pusher).unwrap().position.x, 0.02 * 89.0, "physics leaves game-driven bodies where the game put them");
        assert!(w.get(bx).unwrap().position.x > 2.0, "the crate was pushed: {:?}", w.get(bx).unwrap().position);

        let mut c = SpawnSpec::new(WShape::Group);
        c.name = Some("hero".into());
        c.position = Vec3::new(-3.0, 0.95, 3.0);
        c.collider = Some(Collider { shape: ColliderShape::Capsule { radius: 0.3, height: 1.8 }, ..Default::default() });
        let mut body = Body::new(BodyKind::Kinematic, 70.0);
        body.character = Some(CharacterSpec::default());
        body.velocity = Vec3::new(1.5, 0.0, 0.0);
        c.body = Some(body);
        let hero = w.spawn(c, "t").unwrap().id;
        run(&mut w, &mut p, None, 60);
        let e = w.get(hero).unwrap();
        assert!((e.position.x + 1.5).abs() < 0.1 && (e.position.y - 0.91).abs() < 0.05, "walked 1.5 m: {:?}", e.position);
        assert_eq!(e.body.unwrap().velocity.x, 1.5, "the walking intent stays");
        assert_eq!(p.space.character_state(hero).map(|s| s.grounded), Some(true));
    }

    #[test]
    fn components_round_trip_and_are_validated() {
        let j = Json::parse(
            r#"{"collider":{"shape":"hull","points":[[0,0,0],[1,0,0],[0,1,0],[0,0,1]],"layer":"debris","mask":["world","debris"],"friction":0.3,"restitution":0.5},
                "body":{"kind":"dynamic","mass":3,"velocity":[1,2,3],"lock_position":"z","lock_rotation":"xy","ccd":false}}"#,
        )
        .unwrap();
        let c = collider_arg(&j).unwrap().unwrap().unwrap();
        let b = body_arg(&j).unwrap().unwrap().unwrap();
        c.validate().unwrap();
        check_body(&b).unwrap();
        let back = Json::obj([("collider", c.to_json()), ("body", b.to_json())]);
        assert_eq!(collider_arg(&back).unwrap().unwrap().unwrap(), c);
        assert_eq!(body_arg(&back).unwrap().unwrap().unwrap(), b);
        // Old worlds' plain components keep their JSON.
        assert_eq!(Body::new(BodyKind::Static, 1.0).to_json().to_string(), r#"{"kind":"static","mass":1}"#);
        // Static-only shapes, characters on the wrong kind, flat hulls: refused.
        let mesh = Collider {
            shape: ColliderShape::Mesh { vertices: Arc::new(vec![Vec3::ZERO, Vec3::X, Vec3::Z]), triangles: Arc::new(vec![[0, 2, 1]]) },
            ..Default::default()
        };
        assert_eq!(check_pair(Some(&mesh), Some(&Body::new(BodyKind::Dynamic, 1.0))).unwrap_err().code, "invalid_component");
        let mut walker = Body::new(BodyKind::Dynamic, 1.0);
        walker.character = Some(CharacterSpec::default());
        assert!(check_body(&walker).is_err());
        let flat = Collider { shape: ColliderShape::Hull { points: Arc::new(vec![Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::new(1.0, 0.0, 1.0)]) }, ..Default::default() };
        assert!(flat.validate().is_err());
        // Settings patches.
        let patch = PhysicsPatch::parse(&Json::parse(r#"{"gravity":[0,-1.62,0],"plane_2d":true}"#).unwrap()).unwrap();
        let mut s = PhysicsSettings::default();
        patch.apply(&mut s);
        assert_eq!((s.gravity[1], s.plane_2d, s.substeps), (-1.62, true, 4));
        assert!(PhysicsPatch::parse(&Json::parse(r#"{"gravity":[0,-1,0],"wind":1}"#).unwrap()).is_err());
        assert!(PhysicsPatch::parse(&Json::parse(r#"{"substeps":40}"#).unwrap()).is_err());
    }

    #[test]
    fn worlds_without_physics_keep_their_hashes_and_unsimulated_entities_say_why() {
        let w = World::default();
        let mut h = StableHasher::with_seed(1);
        PhysicsPatch::hash_if_set(&w.env().physics, &mut h);
        assert_eq!(h.finish(), StableHasher::with_seed(1).finish());
        let mut w = World::default();
        let mut s = SpawnSpec::new(WShape::Cube);
        s.collider = Some(Collider { layer: "l0".into(), ..Default::default() });
        w.spawn(s, "t").unwrap();
        let mut p = Physics::default();
        // 64 layer names fill the table; the 65th collider's layer is refused, with why.
        for i in 1..=64 {
            let mut s = SpawnSpec::new(WShape::Cube);
            s.collider = Some(Collider { layer: format!("l{i}"), ..Default::default() });
            w.spawn(s, "t").unwrap();
        }
        run(&mut w, &mut p, None, 1);
        assert!(!p.problems.is_empty(), "too many layer names are reported");
        let first = p.problems.values().next().unwrap();
        assert!(first.get("error").and_then(Json::as_str).unwrap().contains("layer"), "{first}");
    }

    // -----------------------------------------------------------------------------------------
    // Review round 1 regressions.

    /// Finding 2: a 1000 x 0.01 x 1000 plate (and other thin boxes) syncs without a panic and
    /// holds a crate.
    #[test]
    fn thin_plates_sync_and_hold() {
        let mut w = World::default();
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("plate".into());
        s.scale = Vec3::new(1000.0, 0.01, 1000.0);
        s.collider = Some(Collider::default());
        w.spawn(s, "t").unwrap();
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("sliver".into());
        s.position = Vec3::new(30.0, 5.0, 0.0);
        s.scale = Vec3::new(1.0e-3, 1.0e-3, 1.0e-3);
        s.collider = Some(Collider { shape: ColliderShape::Box { size: Vec3::new(1.0e-4, 1.0e-4, 1.0e-4) }, ..Default::default() });
        w.spawn(s, "t").unwrap();
        let id = crate_at(&mut w, "crate", Vec3::new(0.0, 2.0, 0.0));
        let mut p = Physics::default();
        run(&mut w, &mut p, None, 240);
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        let e = w.get(id).unwrap();
        assert!((e.position.y - 0.505).abs() < 0.02 && e.body.unwrap().sleeping, "{:?}", e.position);
    }

    /// Finding 4: an idle character beside a crate doesn't keep it awake or bump its version.
    #[test]
    fn an_idle_character_leaves_its_neighbours_alone() {
        let mut w = World::default();
        floor(&mut w);
        let crate_id = crate_at(&mut w, "crate", Vec3::new(0.62, 0.5, 0.0));
        let mut c = SpawnSpec::new(WShape::Group);
        c.name = Some("hero".into());
        c.position = Vec3::new(-0.3, 0.95, 0.0);
        c.collider = Some(Collider { shape: ColliderShape::Capsule { radius: 0.3, height: 1.8 }, ..Default::default() });
        let mut body = Body::new(BodyKind::Kinematic, 70.0);
        body.character = Some(CharacterSpec::default());
        c.body = Some(body);
        let hero = w.spawn(c, "t").unwrap().id;
        let mut p = Physics::default();
        run(&mut w, &mut p, None, 300);
        let (v_crate, v_hero) = (w.get(crate_id).unwrap().version, w.get(hero).unwrap().version);
        run(&mut w, &mut p, None, 120);
        assert!(w.get(crate_id).unwrap().body.unwrap().sleeping);
        assert_eq!((w.get(crate_id).unwrap().version, w.get(hero).unwrap().version), (v_crate, v_hero));
    }

    /// Finding 6: a hull with more than 16 points is refused on a moving body, by the world.
    #[test]
    fn big_hulls_are_static_only() {
        // Auto-fitted colliders for moving bodies use the same cap physics enforces.
        assert_eq!(specula_model::collider::MAX_MOVING_HULL_POINTS, specula_physics::shape::MAX_MOVING_HULL_POINTS);
        let pts: Vec<Vec3> = (0..20).map(|i| {
            let a = i as f32 * 2.399;
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / 20.0;
            let r = (1.0 - y * y).sqrt();
            Vec3::new(specula_core::dmath::cos(a) * r, y, specula_core::dmath::sin(a) * r)
        }).collect();
        let hull = Collider { shape: ColliderShape::Hull { points: Arc::new(pts) }, ..Default::default() };
        let e = check_pair(Some(&hull), Some(&Body::new(BodyKind::Dynamic, 1.0))).unwrap_err();
        assert_eq!((e.code, e.detail("field").and_then(Json::as_str)), ("invalid_component", Some("collider.points")));
        check_pair(Some(&hull), Some(&Body::new(BodyKind::Static, 1.0))).unwrap();
        check_pair(Some(&hull), None).unwrap();
    }

    /// Finding 7: old-layout colliders hash exactly as before (their JSON text), from the cache;
    /// bodies hash without JSON.
    #[test]
    fn collider_hashes_keep_the_old_layout() {
        for c in [
            Collider::default(),
            Collider { shape: ColliderShape::Capsule { radius: 0.3, height: 1.8 }, layer: "player".into(), is_trigger: true, ..Default::default() },
            Collider { shape: ColliderShape::Sphere { radius: 0.25 }, offset: Vec3::new(0.0, 1.0, 0.0), ..Default::default() },
        ] {
            for _ in 0..2 {
                let (mut a, mut b) = (StableHasher::with_seed(9), StableHasher::with_seed(9));
                c.hash(&mut a);
                b.write_str(&c.to_json().to_string());
                assert_eq!(a.finish(), b.finish());
            }
        }
        let (mut a, mut b) = (StableHasher::with_seed(9), StableHasher::with_seed(9));
        Body::new(BodyKind::Dynamic, 3.0).hash(&mut a);
        b.write_str("dynamic");
        b.write_f32(3.0);
        assert_eq!(a.finish(), b.finish(), "plain bodies hash as before physics");
        let mut moving = Body::new(BodyKind::Dynamic, 3.0);
        moving.velocity = Vec3::new(1.0, 0.0, 0.0);
        let (mut c, mut d) = (StableHasher::with_seed(9), StableHasher::with_seed(9));
        moving.hash(&mut c);
        moving.velocity = Vec3::new(1.0, 0.0, 1.0e-7);
        moving.hash(&mut d);
        assert_ne!(c.finish(), d.finish());
    }

    /// Findings 9 and 10: settings changes wake sleeping bodies; an emptied space reports its
    /// (empty) hash instead of a stale one.
    #[test]
    fn settings_changes_wake_and_empty_spaces_hash() {
        let mut w = World::default();
        let floor_id = floor(&mut w);
        let id = crate_at(&mut w, "crate", Vec3::new(0.0, 0.5, 0.0));
        let mut p = Physics::default();
        run(&mut w, &mut p, None, 120);
        assert!(w.get(id).unwrap().body.unwrap().sleeping);
        w.set_env(&crate::world::EnvPatch { physics: Some(PhysicsPatch { gravity: Some([0.0, 9.81, 0.0]), ..Default::default() }), ..Default::default() }).unwrap();
        run(&mut w, &mut p, None, 30);
        let e = w.get(id).unwrap();
        assert!(!e.body.unwrap().sleeping && e.position.y > 1.0, "gravity upwards lifts it: {:?}", e.position);
        let stale = p.hash().unwrap();
        w.delete(&EntityKey::Id(id), None).unwrap();
        w.delete(&EntityKey::Id(floor_id), None).unwrap();
        run(&mut w, &mut p, None, 2);
        assert_ne!(p.hash(), Some(stale));
        assert_eq!(p.hash(), Some(specula_physics::Space::default().hash()));
    }
}
