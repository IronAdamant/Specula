//! The live world: a tree of entities that agents build, the engine renders and games simulate.
//!
//! Storage is ordered throughout (`BTreeMap`/`BTreeSet`, never `HashMap` iteration), so every
//! lookup is O(log n) and every iteration order is deterministic (foundation F2): by id for
//! rendering and hashing, by path for listings and world files.
//!
//! - **Hierarchy.** An entity may have a parent; names are unique among siblings, so a path like
//!   `/slab/stones/ward_03` names exactly one entity. Children are ordered by name. Transforms are
//!   local to the parent; the world transform is cached on the entity and refreshed for the whole
//!   subtree whenever a transform or parent changes, so rendering and sight read it for free.
//! - **Versions.** Every change bumps the entity's `version`; writers may pass `expect_version`
//!   and a stale write is rejected with the current state (optimistic concurrency for swarms).
//! - **Components.** Besides the look, entities carry `tags`, free-form `props` (game data until
//!   game modules define real components) and the reserved physics components `collider` and
//!   `body` (F4): stored, validated and saved, not simulated yet.
//!
//! Game code running in-process calls [`World::spawn`], [`World::update`], [`World::delete`],
//! [`World::get`] and [`World::query`] directly; the MCP tools are thin parsers over the same calls.

use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::Json;
use specula_gpu::{Environment, Material, Surface};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::RangeInclusive;
use std::sync::Arc;

/// Longest entity name, in characters.
pub const MAX_NAME_CHARS: usize = 128;
/// Deepest allowed nesting (a top-level entity is at depth 1). Keeps paths bounded and leaves room
/// for skeletons.
pub const MAX_DEPTH: usize = 64;
pub const MAX_TAGS: usize = 32;
pub const MAX_TAG_CHARS: usize = 64;
/// Largest `props` object, as compact JSON.
pub const MAX_PROPS_BYTES: usize = 16 * 1024;

/// Parent key of top-level entities in the child index (entity ids start at 1).
const ROOT: u64 = 0;

// Value limits. The world enforces them on every write (tools, world files and in-process game
// code alike), so everything the world holds can be saved and loaded again.

/// Largest |coordinate| of a position or offset (1000 km): keeps every transform and camera finite.
pub const MAX_COORD: f32 = 1.0e6;
pub const SCALE: RangeInclusive<f32> = 1.0e-3..=1.0e5;
/// Colour components (sRGB), opacity, metallic, roughness.
pub const UNIT: RangeInclusive<f32> = 0.0..=1.0;
pub const EMISSIVE: RangeInclusive<f32> = 0.0..=1000.0;
pub const SPECULAR: RangeInclusive<f32> = 0.0..=10.0;
pub const SEED: RangeInclusive<f32> = -1.0e6..=1.0e6;
pub const LIGHT_INTENSITY: RangeInclusive<f32> = 0.0..=1.0e6;
pub const LIGHT_RANGE: RangeInclusive<f32> = 0.01..=1.0e4;
/// Sun and ambient intensity, exposure.
pub const ENV_LEVEL: RangeInclusive<f32> = 0.0..=1000.0;
pub const PROFILE_POINTS: RangeInclusive<usize> = 2..=256;
pub const PROFILE_RADIUS: RangeInclusive<f32> = 0.0..=1.0e5;
pub const PROFILE_HEIGHT: RangeInclusive<f32> = -1.0e5..=1.0e5;
pub const SEGMENTS: RangeInclusive<u32> = 3..=256;
/// Collider sizes and radii, metres.
pub const COLLIDER_SIZE: RangeInclusive<f32> = 1.0e-4..=1.0e5;
pub const MASS: RangeInclusive<f32> = 1.0e-6..=1.0e9;

fn invalid(field: &str, message: String) -> WorldError {
    WorldError::new("invalid_value", message).with("field", field)
}

/// A finite number within `range`. Messages start with the quoted field, as argument errors do.
pub fn check_num(field: &str, v: f32, range: &RangeInclusive<f32>) -> Result<(), WorldError> {
    if v.is_finite() && range.contains(&v) {
        Ok(())
    } else {
        Err(invalid(field, format!("'{field}' must be a number from {} to {} (got {v})", range.start(), range.end())))
    }
}

/// A point or offset: finite, within ±[`MAX_COORD`] on every axis.
pub fn check_point(field: &str, p: Vec3) -> Result<(), WorldError> {
    if [p.x, p.y, p.z].iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD) {
        Ok(())
    } else {
        Err(invalid(field, format!("'{field}' components must be finite and within ±{MAX_COORD} m")))
    }
}

fn check_rotation(r: Vec3) -> Result<(), WorldError> {
    if [r.x, r.y, r.z].iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(invalid("rotation_deg", "'rotation_deg' components must be finite".into()))
    }
}

fn check_scale(s: Vec3) -> Result<(), WorldError> {
    if [s.x, s.y, s.z].iter().all(|c| SCALE.contains(c)) {
        Ok(())
    } else {
        Err(invalid("scale", format!("'scale' components must be between {} and {}", SCALE.start(), SCALE.end())))
    }
}

/// sRGB, every component 0..1.
pub fn check_color(field: &str, c: [f32; 3]) -> Result<(), WorldError> {
    if c.iter().all(|v| UNIT.contains(v)) {
        Ok(())
    } else {
        Err(invalid(field, format!("'{field}' components must be 0..1 (sRGB)")))
    }
}

fn check_look_patch(p: &LookPatch) -> Result<(), WorldError> {
    let nums = [
        ("opacity", p.opacity, &UNIT),
        ("emissive", p.emissive, &EMISSIVE),
        ("metallic", p.metallic, &UNIT),
        ("roughness", p.roughness, &UNIT),
        ("specular", p.specular, &SPECULAR),
        ("seed", p.seed, &SEED),
    ];
    for (field, v, range) in nums {
        if let Some(v) = v {
            check_num(field, v, range)?;
        }
    }
    p.color2.map_or(Ok(()), |c| check_color("color2", c))
}

fn check_look(l: &Look) -> Result<(), WorldError> {
    let s = &l.surface;
    check_look_patch(&LookPatch {
        material: None,
        opacity: Some(l.opacity),
        emissive: Some(s.emissive),
        metallic: Some(s.metallic),
        roughness: Some(s.roughness),
        specular: Some(s.specular),
        color2: Some(s.color2),
        seed: Some(s.seed),
    })
}

fn check_light(l: &LightSpec) -> Result<(), WorldError> {
    check_num("light.intensity", l.intensity, &LIGHT_INTENSITY)?;
    check_num("light.range", l.range, &LIGHT_RANGE)?;
    check_point("light.offset", l.offset)?;
    l.color.map_or(Ok(()), |c| check_color("light.color", c))
}

/// 2..256 points, radius 0..100000 (at least one above 0), |height| ≤ 100000, 3..256 segments.
pub fn check_profile(p: &Profile) -> Result<(), WorldError> {
    let points_ok = PROFILE_POINTS.contains(&p.points.len())
        && p.points.iter().all(|[r, y]| PROFILE_RADIUS.contains(r) && PROFILE_HEIGHT.contains(y));
    if !points_ok {
        return Err(invalid(
            "profile",
            "'profile' must be 2..256 [radius, height] pairs: radius 0..100000, |height| <= 100000".into(),
        ));
    }
    if !p.points.iter().any(|q| q[0] > 0.0) {
        return Err(invalid("profile", "'profile' needs at least one point with a radius above 0".into()));
    }
    if !SEGMENTS.contains(&p.segments) {
        return Err(invalid("segments", "'segments' must be an integer from 3 to 256".into()));
    }
    Ok(())
}

fn check_body(b: &Body) -> Result<(), WorldError> {
    check_num("body.mass", b.mass, &MASS)
}

fn check_tags<'a>(tags: impl IntoIterator<Item = &'a String>) -> Result<(), WorldError> {
    tags.into_iter().try_for_each(|t| validate_tag(t).map_err(|e| e.with("field", "tags")))
}

/// Everything [`World::spawn`] checks about the values (structure — parent, names — is checked
/// against the world).
pub fn check_spawn(s: &SpawnSpec) -> Result<(), WorldError> {
    check_point("position", s.position)?;
    check_rotation(s.rotation_deg)?;
    check_scale(s.scale)?;
    check_color("color", s.color)?;
    check_look(&s.look)?;
    s.light.as_ref().map_or(Ok(()), check_light)?;
    if (s.shape == Shape::Lathe) != s.profile.is_some() {
        return Err(invalid("profile", "'profile' is required for shape 'lathe' and only allowed for it".into()));
    }
    s.profile.as_ref().map_or(Ok(()), check_profile)?;
    check_tags(&s.tags)?;
    if s.tags.len() > MAX_TAGS {
        return Err(invalid("tags", format!("'tags': at most {MAX_TAGS}")));
    }
    if !matches!(s.props, Json::Null | Json::Obj(_)) {
        return Err(invalid("props", "'props' must be a JSON object".into()));
    }
    s.collider.as_ref().map_or(Ok(()), Collider::validate)?;
    s.body.as_ref().map_or(Ok(()), check_body)
}

/// Everything [`World::update`] can check without the world.
pub fn check_update(u: &UpdateSpec) -> Result<(), WorldError> {
    if let Some(n) = &u.name {
        validate_name(n)?;
    }
    u.position.map_or(Ok(()), |p| check_point("position", p))?;
    u.rotation_deg.map_or(Ok(()), check_rotation)?;
    u.scale.map_or(Ok(()), check_scale)?;
    u.color.map_or(Ok(()), |c| check_color("color", c))?;
    check_look_patch(&u.look)?;
    if let Some(Some(l)) = &u.light {
        check_light(l)?;
    }
    u.profile.as_ref().map_or(Ok(()), check_profile)?;
    check_tags(u.tags.iter().flatten().chain(&u.add_tags))?;
    if u.props.as_ref().is_some_and(|p| !matches!(p, Json::Obj(_))) {
        return Err(invalid("props", "'props' must be a JSON object".into()));
    }
    if let Some(Some(c)) = &u.collider {
        c.validate()?;
    }
    if let Some(Some(b)) = &u.body {
        check_body(b)?;
    }
    Ok(())
}

/// Everything [`World::set_env`] checks.
pub fn check_env(p: &EnvPatch) -> Result<(), WorldError> {
    if let Some(d) = p.sun_direction {
        let finite = [d.x, d.y, d.z].iter().all(|c| c.is_finite() && c.abs() <= MAX_COORD);
        if !finite || d.length() < 1e-6 {
            return Err(invalid("sun_direction", "'sun_direction' must be finite and not zero".into()));
        }
    }
    for (field, c) in [("sun_color", p.sun_color), ("ambient_color", p.ambient_color), ("background", p.background)] {
        c.map_or(Ok(()), |c| check_color(field, c))?;
    }
    for (field, v) in [("sun_intensity", p.sun_intensity), ("ambient_intensity", p.ambient_intensity), ("exposure", p.exposure)] {
        v.map_or(Ok(()), |v| check_num(field, v, &ENV_LEVEL))?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    /// A transform node with no mesh: groups children (districts, rooms, a lamp's parts).
    Group,
    Cube,
    /// 1×1 plane in XZ (normal +Y) — floors and ground.
    Quad,
    Sphere,
    /// A profile spun around +Y (see [`Profile`]).
    Lathe,
}

impl Shape {
    pub const ALL: [Shape; 5] = [Shape::Group, Shape::Cube, Shape::Quad, Shape::Sphere, Shape::Lathe];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Group => "group",
            Shape::Cube => "cube",
            Shape::Quad => "quad",
            Shape::Sphere => "sphere",
            Shape::Lathe => "lathe",
        }
    }

    pub fn parse(s: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|shape| shape.name() == s)
    }
}

/// A lathe profile: `[radius, height]` points from bottom to top, spun around +Y.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub points: Vec<[f32; 2]>,
    /// Steps around the axis.
    pub segments: u32,
}

impl Profile {
    /// Object-space bounding box.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let r = self.points.iter().map(|p| p[0]).fold(0.0f32, f32::max);
        let lo = self.points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let hi = self.points.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        (Vec3::new(-r, lo, -r), Vec3::new(r, hi, r))
    }

    /// Identifies the mesh this profile builds (FNV-1a over the exact bits).
    pub fn key(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |bits: u32| {
            for b in bits.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(self.segments);
        for p in &self.points {
            eat(p[0].to_bits());
            eat(p[1].to_bits());
        }
        h
    }

    fn to_json(&self) -> Json {
        Json::obj([
            ("points", Json::arr(self.points.iter().map(|p| Json::arr([Json::from(p[0]), Json::from(p[1])])))),
            ("segments", Json::from(self.segments)),
        ])
    }
}

pub const MATERIAL_NAMES: [&str; 4] = ["standard", "glass", "blob_shadow", "lava_lamp"];

pub fn material_name(m: Material) -> &'static str {
    match m {
        Material::Lit => "standard",
        Material::Glass => "glass",
        Material::BlobShadow => "blob_shadow",
        Material::LavaLamp => "lava_lamp",
    }
}

pub fn parse_material(s: &str) -> Option<Material> {
    [Material::Lit, Material::Glass, Material::BlobShadow, Material::LavaLamp].into_iter().find(|m| material_name(*m) == s)
}

/// How an entity is shaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub material: Material,
    /// Blended materials only.
    pub opacity: f32,
    pub surface: Surface,
}

impl Default for Look {
    fn default() -> Self {
        Look { material: Material::Lit, opacity: 1.0, surface: Surface::default() }
    }
}

impl Look {
    /// The fields that differ from the default, so listings stay short.
    fn to_json_fields(self, out: &mut Vec<(&'static str, Json)>) {
        let d = Look::default();
        out.push(("material", Json::from(material_name(self.material))));
        let s = &self.surface;
        let mut num = |key: &'static str, v: f32, default: f32| {
            if v != default {
                out.push((key, Json::from(v)));
            }
        };
        num("opacity", self.opacity, d.opacity);
        num("emissive", s.emissive, d.surface.emissive);
        num("metallic", s.metallic, d.surface.metallic);
        num("roughness", s.roughness, d.surface.roughness);
        num("specular", s.specular, d.surface.specular);
        num("seed", s.seed, d.surface.seed);
        if s.color2 != d.surface.color2 {
            out.push(("color2", Json::from(color_hex(s.color2))));
        }
    }
}

/// Changes to a [`Look`]; `None` leaves a field as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LookPatch {
    pub material: Option<Material>,
    pub opacity: Option<f32>,
    pub emissive: Option<f32>,
    pub metallic: Option<f32>,
    pub roughness: Option<f32>,
    pub specular: Option<f32>,
    pub color2: Option<[f32; 3]>,
    pub seed: Option<f32>,
}

impl LookPatch {
    pub fn is_empty(&self) -> bool {
        *self == LookPatch::default()
    }

    pub fn apply(&self, look: &mut Look) {
        let s = &mut look.surface;
        if let Some(v) = self.material {
            look.material = v;
        }
        if let Some(v) = self.opacity {
            look.opacity = v;
        }
        let set = |slot: &mut f32, v: Option<f32>| {
            if let Some(v) = v {
                *slot = v;
            }
        };
        set(&mut s.emissive, self.emissive);
        set(&mut s.metallic, self.metallic);
        set(&mut s.roughness, self.roughness);
        set(&mut s.specular, self.specular);
        set(&mut s.seed, self.seed);
        if let Some(c) = self.color2 {
            s.color2 = c;
        }
    }
}

/// A point light carried by an entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSpec {
    pub intensity: f32,
    pub range: f32,
    /// sRGB; `None` = the entity's colour.
    pub color: Option<[f32; 3]>,
    /// Where on the entity, in its local (scaled) coordinates.
    pub offset: Vec3,
}

impl LightSpec {
    fn to_json(self) -> Json {
        let mut fields = vec![
            ("intensity", Json::from(self.intensity)),
            ("range", Json::from(self.range)),
            ("offset", vec3_json(self.offset)),
        ];
        if let Some(c) = self.color {
            fields.push(("color", Json::from(color_hex(c))));
        }
        Json::obj(fields)
    }
}

/// Collision shape, in the entity's local (scaled) coordinates. Reserved for physics (F4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColliderShape {
    /// Full extents (a unit cube's collider is `size: [1, 1, 1]`).
    Box { size: Vec3 },
    Sphere { radius: f32 },
    /// Along local +Y; `height` is the total height including both caps (≥ 2 × radius).
    Capsule { radius: f32, height: f32 },
}

/// Reserved for physics (F4): stored, validated and saved; not simulated yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    pub shape: ColliderShape,
    pub offset: Vec3,
    /// Reports overlaps instead of blocking.
    pub is_trigger: bool,
    /// Collision layer name (layers are named in the game's config, never bit numbers).
    pub layer: String,
}

impl Collider {
    pub fn to_json(&self) -> Json {
        let mut f = match self.shape {
            ColliderShape::Box { size } => vec![("shape", Json::from("box")), ("size", vec3_json(size))],
            ColliderShape::Sphere { radius } => vec![("shape", Json::from("sphere")), ("radius", Json::from(radius))],
            ColliderShape::Capsule { radius, height } => vec![
                ("shape", Json::from("capsule")),
                ("radius", Json::from(radius)),
                ("height", Json::from(height)),
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
        Json::obj(f)
    }

    fn validate(&self) -> Result<(), WorldError> {
        match self.shape {
            ColliderShape::Box { size } => {
                [size.x, size.y, size.z].iter().try_for_each(|v| check_num("collider.size", *v, &COLLIDER_SIZE))?
            }
            ColliderShape::Sphere { radius } => check_num("collider.radius", radius, &COLLIDER_SIZE)?,
            ColliderShape::Capsule { radius, height } => {
                check_num("collider.radius", radius, &COLLIDER_SIZE)?;
                check_num("collider.height", height, &COLLIDER_SIZE)?;
                if height < 2.0 * radius {
                    return Err(invalid(
                        "collider.height",
                        "'collider.height' is the total height and must be at least 2 x radius".into(),
                    ));
                }
            }
        }
        check_point("collider.offset", self.offset)?;
        validate_tag(&self.layer).map_err(|e| e.with("field", "collider.layer"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Never moves (walls, terrain).
    Static,
    /// Moved by game code, pushes dynamic bodies (doors, characters).
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

/// Reserved for physics (F4): stored, validated and saved; not simulated yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub kind: BodyKind,
    /// Kilograms; used by dynamic bodies.
    pub mass: f32,
}

impl Body {
    pub fn to_json(self) -> Json {
        Json::obj([("kind", Json::from(self.kind.name())), ("mass", Json::from(self.mass))])
    }
}

/// Scene-wide lighting and background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub light: Environment,
    /// sRGB background (and the sky colour reflections pick up).
    pub background: [f32; 3],
}

impl Default for Env {
    fn default() -> Self {
        Env { light: Environment::default(), background: [0.10, 0.11, 0.13] }
    }
}

impl Env {
    pub fn to_json(self) -> Json {
        let l = &self.light;
        Json::obj([
            ("sun_direction", Json::arr(l.sun_direction.map(Json::from))),
            ("sun_color", Json::from(color_hex(l.sun_color))),
            ("sun_intensity", Json::from(l.sun_intensity)),
            ("ambient_color", Json::from(color_hex(l.ambient_color))),
            ("ambient_intensity", Json::from(l.ambient_intensity)),
            ("exposure", Json::from(l.exposure)),
            ("tonemap", Json::from(l.tonemap)),
            ("background", Json::from(color_hex(self.background))),
        ])
    }

    /// Like [`Env::to_json`] but bit-exact (colours as `[r, g, b]` when hex would round them):
    /// what world files store.
    pub fn to_json_exact(self) -> Json {
        let l = &self.light;
        Json::obj([
            ("sun_direction", Json::arr(l.sun_direction.map(Json::from))),
            ("sun_color", color_json_exact(l.sun_color)),
            ("sun_intensity", Json::from(l.sun_intensity)),
            ("ambient_color", color_json_exact(l.ambient_color)),
            ("ambient_intensity", Json::from(l.ambient_intensity)),
            ("exposure", Json::from(l.exposure)),
            ("tonemap", Json::from(l.tonemap)),
            ("background", color_json_exact(self.background)),
        ])
    }
}

/// Changes to the [`Env`]; `None` leaves a field as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnvPatch {
    pub sun_direction: Option<Vec3>,
    pub sun_color: Option<[f32; 3]>,
    pub sun_intensity: Option<f32>,
    pub ambient_color: Option<[f32; 3]>,
    pub ambient_intensity: Option<f32>,
    pub exposure: Option<f32>,
    pub tonemap: Option<bool>,
    pub background: Option<[f32; 3]>,
}

impl EnvPatch {
    pub fn apply(&self, env: &mut Env) {
        let l = &mut env.light;
        if let Some(d) = self.sun_direction {
            l.sun_direction = [d.x, d.y, d.z];
        }
        if let Some(c) = self.sun_color {
            l.sun_color = c;
        }
        if let Some(v) = self.sun_intensity {
            l.sun_intensity = v;
        }
        if let Some(c) = self.ambient_color {
            l.ambient_color = c;
        }
        if let Some(v) = self.ambient_intensity {
            l.ambient_intensity = v;
        }
        if let Some(v) = self.exposure {
            l.exposure = v;
        }
        if let Some(v) = self.tonemap {
            l.tonemap = v;
        }
        if let Some(c) = self.background {
            env.background = c;
        }
    }
}

/// A structured world error: a machine-readable `code`, a message for people, and details
/// (paths, versions, the current state) agents can act on without parsing prose.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldError {
    pub code: &'static str,
    pub message: String,
    pub details: Vec<(&'static str, Json)>,
}

impl WorldError {
    pub fn new(code: &'static str, message: impl Into<String>) -> WorldError {
        WorldError { code, message: message.into(), details: Vec::new() }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<Json>) -> WorldError {
        self.details.push((key, value.into()));
        self
    }

    pub fn detail(&self, key: &str) -> Option<&Json> {
        self.details.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        f.extend(self.details.iter().cloned());
        Json::obj(f)
    }
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WorldError {}

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: u64,
    /// Unique among its siblings; never contains `/`.
    pub name: String,
    /// `None` = top level.
    pub parent: Option<u64>,
    pub shape: Shape,
    /// Local to the parent (the world for top-level entities).
    pub position: Vec3,
    /// As given by the agent (degrees, X then Y then Z) — reported back verbatim.
    pub rotation_deg: Vec3,
    pub scale: Vec3,
    /// sRGB.
    pub color: [f32; 3],
    pub look: Look,
    pub light: Option<LightSpec>,
    /// Present exactly when `shape` is [`Shape::Lathe`].
    pub profile: Option<Profile>,
    /// Short labels for queries ("door", "spawn_point"); sorted, so output is deterministic.
    pub tags: BTreeSet<String>,
    /// Free-form game data: always an object, keys sorted recursively.
    pub props: Json,
    pub collider: Option<Collider>,
    pub body: Option<Body>,
    /// 1 when spawned, +1 on every change.
    pub version: u64,
    /// Agent name (shared: see [`World`]'s interning).
    pub created_by: Arc<str>,
    /// Agent name of the last change.
    pub modified_by: Arc<str>,
    /// Cached world transform: parent's world × local. Kept current by [`World`].
    world: Mat4,
}

impl Entity {
    /// Local transform (relative to the parent).
    pub fn local_matrix(&self) -> Mat4 {
        let r = self.rotation_deg;
        Mat4::from_trs(self.position, Quat::from_euler_xyz_deg(r.x, r.y, r.z), self.scale)
    }

    /// World transform (what renders).
    pub fn model(&self) -> Mat4 {
        self.world
    }

    pub fn world_position(&self) -> Vec3 {
        self.world.transform_point(Vec3::ZERO)
    }

    /// Object-space bounding box; `None` for groups, which have no geometry.
    pub fn local_bounds(&self) -> Option<(Vec3, Vec3)> {
        match (self.shape, &self.profile) {
            (Shape::Group, _) => None,
            (Shape::Lathe, Some(p)) => Some(p.bounds()),
            (Shape::Quad, _) => Some((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.0, 0.5))),
            _ => Some((Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5))),
        }
    }

    /// World-space corners of the oriented bounding box (`None` for groups).
    pub fn world_corners(&self) -> Option<[Vec3; 8]> {
        let (lo, hi) = self.local_bounds()?;
        let mut out = [Vec3::ZERO; 8];
        for (i, slot) in out.iter_mut().enumerate() {
            let corner = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            *slot = self.world.transform_point(corner);
        }
        Some(out)
    }

    /// World-space axis-aligned bounds; a group is the point at its origin.
    pub fn world_aabb(&self) -> (Vec3, Vec3) {
        match self.world_corners() {
            Some(c) => c.iter().skip(1).fold((c[0], c[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
            None => {
                let p = self.world_position();
                (p, p)
            }
        }
    }

    /// The entity's point light in world space, if it carries one.
    pub fn world_light(&self) -> Option<specula_gpu::PointLight> {
        let l = self.light?;
        let p = self.world.transform_point(l.offset);
        Some(specula_gpu::PointLight {
            position: [p.x, p.y, p.z],
            color: l.color.unwrap_or(self.color),
            intensity: l.intensity,
            range: l.range,
        })
    }

    /// Everything about the entity, defaults left out so listings stay short.
    pub fn to_json(&self, path: &str) -> Json {
        let mut fields = vec![
            ("id", Json::from(self.id)),
            ("name", Json::from(self.name.as_str())),
            ("path", Json::from(path)),
            ("shape", Json::from(self.shape.name())),
            ("position", vec3_json(self.position)),
            ("rotation_deg", vec3_json(self.rotation_deg)),
            ("scale", vec3_json(self.scale)),
        ];
        if self.parent.is_some() {
            fields.push(("world_position", vec3_json(self.world_position())));
        }
        fields.push(("color", Json::from(color_hex(self.color))));
        self.look.to_json_fields(&mut fields);
        if let Some(l) = self.light {
            fields.push(("light", l.to_json()));
        }
        if let Some(p) = &self.profile {
            fields.push(("profile", p.to_json()));
        }
        if !self.tags.is_empty() {
            fields.push(("tags", tags_json(&self.tags)));
        }
        if self.props.as_object().is_some_and(|o| !o.is_empty()) {
            fields.push(("props", self.props.clone()));
        }
        if let Some(c) = &self.collider {
            fields.push(("collider", c.to_json()));
        }
        if let Some(b) = self.body {
            fields.push(("body", b.to_json()));
        }
        fields.push(("version", Json::from(self.version)));
        fields.push(("created_by", Json::from(&*self.created_by)));
        fields.push(("modified_by", Json::from(&*self.modified_by)));
        Json::obj(fields)
    }

    /// Feeds everything that defines what the entity is and does, by exact bits. Versions and
    /// attribution are left out: two worlds that look and behave the same hash the same,
    /// whoever built them and however many edits it took.
    fn hash_into(&self, h: &mut StableHasher) {
        h.write_u64(self.id);
        h.write_u64(self.parent.unwrap_or(ROOT));
        h.write_str(&self.name);
        h.write_str(self.shape.name());
        for v in [self.position, self.rotation_deg, self.scale] {
            hash_vec3(h, v);
        }
        self.color.iter().for_each(|c| h.write_f32(*c));
        let (l, s) = (&self.look, &self.look.surface);
        h.write_str(material_name(l.material));
        for v in [l.opacity, s.emissive, s.metallic, s.roughness, s.specular, s.seed] {
            h.write_f32(v);
        }
        s.color2.iter().for_each(|c| h.write_f32(*c));
        h.write_bool(self.light.is_some());
        if let Some(light) = self.light {
            h.write_f32(light.intensity);
            h.write_f32(light.range);
            hash_vec3(h, light.offset);
            h.write_bool(light.color.is_some());
            light.color.unwrap_or_default().iter().for_each(|c| h.write_f32(*c));
        }
        h.write_bool(self.profile.is_some());
        if let Some(p) = &self.profile {
            h.write_u64(p.key());
        }
        h.write_u64(self.tags.len() as u64);
        self.tags.iter().for_each(|t| h.write_str(t));
        h.write_str(&self.props.to_string());
        match &self.collider {
            None => h.write_u8(0),
            Some(c) => {
                h.write_str(&c.to_json().to_string());
            }
        }
        match self.body {
            None => h.write_u8(0),
            Some(b) => {
                h.write_str(b.kind.name());
                h.write_f32(b.mass);
            }
        }
    }
}

fn hash_vec3(h: &mut StableHasher, v: Vec3) {
    h.write_f32(v.x);
    h.write_f32(v.y);
    h.write_f32(v.z);
}

/// Names an entity: by id, by path (`/slab/stones/ward_03`), or by a bare name that must be
/// unique in the whole world.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityKey {
    Id(u64),
    Path(String),
    Name(String),
}

impl fmt::Display for EntityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityKey::Id(id) => write!(f, "id {id}"),
            EntityKey::Path(p) => f.write_str(p),
            EntityKey::Name(n) => write!(f, "'{n}'"),
        }
    }
}

pub struct SpawnSpec {
    pub shape: Shape,
    /// Default: `<shape>_<id>`.
    pub name: Option<String>,
    /// `None` = top level.
    pub parent: Option<EntityKey>,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
    pub color: [f32; 3],
    pub look: Look,
    pub light: Option<LightSpec>,
    /// Required for [`Shape::Lathe`], refused for other shapes.
    pub profile: Option<Profile>,
    pub tags: BTreeSet<String>,
    /// A JSON object (or `Json::Null` for none).
    pub props: Json,
    pub collider: Option<Collider>,
    pub body: Option<Body>,
}

impl SpawnSpec {
    /// A light-grey matte top-level `shape` at the origin.
    pub fn new(shape: Shape) -> SpawnSpec {
        SpawnSpec {
            shape,
            name: None,
            parent: None,
            position: Vec3::ZERO,
            rotation_deg: Vec3::ZERO,
            scale: Vec3::ONE,
            color: [0.8, 0.8, 0.8],
            look: Look::default(),
            light: None,
            profile: None,
            tags: BTreeSet::new(),
            props: Json::Null,
            collider: None,
            body: None,
        }
    }
}

/// Fields to change in [`World::update`]; `None` leaves a field as it is.
#[derive(Clone, Debug, Default)]
pub struct UpdateSpec {
    /// Rename (unique among the new siblings).
    pub name: Option<String>,
    /// Reparent: `Some(None)` = top level. The **local** transform is kept, so the entity moves
    /// with its new parent (like dropping a file into another folder keeps its relative layout).
    pub parent: Option<Option<EntityKey>>,
    pub position: Option<Vec3>,
    pub rotation_deg: Option<Vec3>,
    pub scale: Option<Vec3>,
    pub color: Option<[f32; 3]>,
    pub look: LookPatch,
    /// `Some(None)` removes the light.
    pub light: Option<Option<LightSpec>>,
    /// Lathe entities only.
    pub profile: Option<Profile>,
    /// Replaces every tag.
    pub tags: Option<BTreeSet<String>>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    /// Merged into `props` key by key; a `null` value removes that key.
    pub props: Option<Json>,
    pub collider: Option<Option<Collider>>,
    pub body: Option<Option<Body>>,
    /// Reject the change unless the entity is still at this version.
    pub expect_version: Option<u64>,
}

impl UpdateSpec {
    /// No field would change (`expect_version` alone is not a change).
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.parent.is_none()
            && self.position.is_none()
            && self.rotation_deg.is_none()
            && self.scale.is_none()
            && self.color.is_none()
            && self.look.is_empty()
            && self.light.is_none()
            && self.profile.is_none()
            && self.tags.is_none()
            && self.add_tags.is_empty()
            && self.remove_tags.is_empty()
            && self.props.is_none()
            && self.collider.is_none()
            && self.body.is_none()
    }
}

/// What [`World::delete`] removed: the entity and its whole subtree, parents first.
pub struct Deleted {
    pub id: u64,
    pub path: String,
    pub removed: Vec<Entity>,
}

/// One operation of [`World::apply_batch`].
pub enum Op {
    Spawn(SpawnSpec),
    Set(EntityKey, UpdateSpec),
    Delete(EntityKey, Option<u64>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpResult {
    Spawned { id: u64, path: String },
    Updated { id: u64, path: String, version: u64 },
    Deleted { id: u64, path: String, count: usize },
}

impl OpResult {
    pub fn to_json(&self) -> Json {
        match self {
            OpResult::Spawned { id, path } => {
                Json::obj([("op", Json::from("spawn")), ("id", Json::from(*id)), ("path", Json::from(path.as_str()))])
            }
            OpResult::Updated { id, path, version } => Json::obj([
                ("op", Json::from("set")),
                ("id", Json::from(*id)),
                ("path", Json::from(path.as_str())),
                ("version", Json::from(*version)),
            ]),
            OpResult::Deleted { id, path, count } => Json::obj([
                ("op", Json::from("delete")),
                ("id", Json::from(*id)),
                ("path", Json::from(path.as_str())),
                ("deleted", Json::from(*count)),
            ]),
        }
    }
}

/// How to undo one applied batch operation.
enum Undo {
    Spawned(u64),
    Updated(Box<Entity>),
    Deleted(Vec<Entity>),
}

/// Filters for [`World::query`]; every given filter must match.
#[derive(Clone, Debug, Default)]
pub struct Query {
    /// The entity at this path and everything below it.
    pub path_prefix: Option<String>,
    /// All of these tags.
    pub tags: Vec<String>,
    pub shape: Option<Shape>,
    pub material: Option<Material>,
    pub created_by: Option<String>,
    /// World-space box `(min, max)` that the entity's world bounds must intersect.
    pub region: Option<(Vec3, Vec3)>,
    /// Resume after this path (in path order).
    pub after: Option<String>,
    pub limit: usize,
}

pub struct QueryPage<'a> {
    /// Matches in the whole world (not just this page).
    pub total: usize,
    /// Up to `limit` matches after the cursor, in path order, with their paths.
    pub items: Vec<(&'a Entity, String)>,
    /// More matches follow the last item.
    pub more: bool,
}

/// Every field an entity listing can return (see [`World::fields_json`]).
pub const FIELDS: [&str; 29] = [
    "id", "name", "path", "parent", "shape", "position", "rotation_deg", "scale", "world_position", "color",
    "material", "opacity", "emissive", "metallic", "roughness", "specular", "color2", "seed", "light", "profile",
    "tags", "props", "collider", "body", "version", "created_by", "modified_by", "children", "bounds",
];

#[derive(Clone, Debug, Default)]
pub struct World {
    entities: BTreeMap<u64, Entity>,
    /// `(parent id or ROOT, name) → id`: enforces unique sibling names and lists children in name
    /// order.
    children: BTreeMap<(u64, String), u64>,
    /// `(name, id)`, for bare-name lookups.
    names: BTreeSet<(String, u64)>,
    next_id: u64,
    env: Env,
    /// Bumped by every change; persistence compares it to know when to save. A failed batch
    /// restores it.
    revision: u64,
    /// Agent names, so attribution is a reference count per change rather than an allocation.
    agents: BTreeSet<Arc<str>>,
    /// Never goes back (unlike `revision`): stamps changes for [`World::subtree_stamp`].
    stamp: u64,
    /// Top-level id → stamp of the latest change anywhere in its subtree. Lets world files rewrite
    /// only the subtrees that changed.
    touched: BTreeMap<u64, u64>,
}

impl World {
    /// The shared copy of an agent name.
    fn intern(&mut self, name: &str) -> Arc<str> {
        if let Some(a) = self.agents.get(name) {
            return Arc::clone(a);
        }
        let a: Arc<str> = Arc::from(name);
        self.agents.insert(Arc::clone(&a));
        a
    }

    /// The top-level entity above (or at) `id`.
    fn top_of(&self, id: u64) -> Option<u64> {
        let mut at = self.entities.get(&id)?;
        while let Some(p) = at.parent.and_then(|p| self.entities.get(&p)) {
            at = p;
        }
        Some(at.id)
    }

    /// Records a change in `id`'s top-level subtree.
    fn touch(&mut self, id: u64) {
        if let Some(top) = self.top_of(id) {
            self.stamp += 1;
            self.touched.insert(top, self.stamp);
        }
    }

    /// Records that `id` is about to be removed: its top-level subtree changed, or, when `id` is
    /// itself top-level, the subtree is gone and its stamp is forgotten.
    fn touch_removal(&mut self, id: u64) {
        match self.entities.get(&id) {
            Some(e) if e.parent.is_none() => {
                self.touched.remove(&id);
            }
            Some(_) => self.touch(id),
            None => {}
        }
    }

    /// Changes so far in the subtree under top-level entity `top`, as an opaque stamp: equal stamps
    /// mean nothing in that subtree changed in between (0 = never changed since the world began).
    pub fn subtree_stamp(&self, top: u64) -> u64 {
        self.touched.get(&top).copied().unwrap_or(0)
    }
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Every entity, by id.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.entities.values()
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    pub fn set_env(&mut self, patch: &EnvPatch) -> Result<(), WorldError> {
        check_env(patch)?;
        patch.apply(&mut self.env);
        self.revision += 1;
        Ok(())
    }

    /// Changes so far (any change bumps it).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The id the next spawn gets is above this.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Restores the id counter from a world file. Never below an existing id, so a stale counter
    /// can't hand out an id twice.
    pub(crate) fn set_next_id(&mut self, next_id: u64) {
        let max = self.entities.keys().next_back().copied().unwrap_or(0);
        self.next_id = next_id.max(max);
    }

    fn child_range(&self, parent: u64) -> std::collections::btree_map::Range<'_, (u64, String), u64> {
        self.children.range((parent, String::new())..(parent + 1, String::new()))
    }

    /// Children of `parent` (`None` = top level), in name order.
    pub fn children_of(&self, parent: Option<u64>) -> impl Iterator<Item = u64> + '_ {
        self.child_range(parent.unwrap_or(ROOT)).map(|(_, &id)| id)
    }

    /// `/a/b/c`; empty for an unknown id.
    pub fn path_of(&self, id: u64) -> String {
        let mut names = Vec::new();
        let mut at = self.entities.get(&id);
        while let Some(e) = at {
            names.push(e.name.as_str());
            at = e.parent.and_then(|p| self.entities.get(&p));
        }
        let mut path = String::new();
        for n in names.iter().rev() {
            path.push('/');
            path.push_str(n);
        }
        path
    }

    /// 1 for top-level entities.
    fn depth_of(&self, id: u64) -> usize {
        let mut depth = 0;
        let mut at = Some(id);
        while let Some(e) = at.and_then(|i| self.entities.get(&i)) {
            depth += 1;
            at = e.parent;
        }
        depth
    }

    /// Levels in the subtree rooted at `id` (1 for a leaf).
    fn height_of(&self, id: u64) -> usize {
        let mut height = 0;
        let mut stack = vec![(id, 1)];
        while let Some((at, level)) = stack.pop() {
            height = height.max(level);
            stack.extend(self.child_range(at).map(|(_, &c)| (c, level + 1)));
        }
        height
    }

    /// Whether `ancestor` is `id` or above it.
    fn is_ancestor_or_self(&self, ancestor: u64, id: u64) -> bool {
        let mut at = Some(id);
        while let Some(i) = at {
            if i == ancestor {
                return true;
            }
            at = self.entities.get(&i).and_then(|e| e.parent);
        }
        false
    }

    /// The entity at `path` (`/a/b`).
    pub fn find_path(&self, path: &str) -> Result<u64, WorldError> {
        let invalid = |why: &str| {
            WorldError::new("invalid_path", format!("'{path}' is not a valid path: {why}")).with("path", path)
        };
        let rest = path.strip_prefix('/').ok_or_else(|| invalid("paths start with '/', e.g. /slab/stones"))?;
        if rest.is_empty() {
            return Err(invalid("'/' is the root, not an entity"));
        }
        let mut at = ROOT;
        let mut found = String::new();
        for name in rest.split('/') {
            if name.is_empty() {
                return Err(invalid("empty segment"));
            }
            match self.children.get(&(at, name.to_owned())) {
                Some(&id) => at = id,
                None => {
                    let parent = if found.is_empty() { "the top level".to_owned() } else { found.clone() };
                    return Err(WorldError::new("not_found", format!("no entity at {path}: {parent} has no child '{name}'"))
                        .with("path", path)
                        .with("deepest_existing", if found.is_empty() { Json::Null } else { Json::from(found) }));
                }
            }
            found.push('/');
            found.push_str(name);
        }
        Ok(at)
    }

    /// The id an [`EntityKey`] names. A bare name must be unique in the whole world; otherwise
    /// the error lists up to 5 matching paths.
    pub fn resolve(&self, key: &EntityKey) -> Result<u64, WorldError> {
        match key {
            EntityKey::Id(id) => {
                if self.entities.contains_key(id) {
                    Ok(*id)
                } else {
                    Err(WorldError::new("not_found", format!("no entity with id {id}")).with("id", *id))
                }
            }
            EntityKey::Path(path) => self.find_path(path),
            EntityKey::Name(name) if name.starts_with('/') => self.find_path(name),
            EntityKey::Name(name) => {
                let mut ids = self.names.range((name.clone(), 0)..=(name.clone(), u64::MAX)).map(|(_, id)| *id);
                match (ids.next(), ids.next()) {
                    (None, _) => Err(WorldError::new("not_found", format!("no entity named '{name}'")).with("name", name.as_str())),
                    (Some(id), None) => Ok(id),
                    (Some(a), Some(b)) => {
                        let all: Vec<u64> = [a, b].into_iter().chain(ids).collect();
                        let mut paths: Vec<String> = all.iter().map(|&id| self.path_of(id)).collect();
                        paths.sort_by(|x, y| path_cmp(x, y));
                        let count = paths.len();
                        paths.truncate(5);
                        Err(WorldError::new(
                            "ambiguous_name",
                            format!("{count} entities are named '{name}'; give a path or an id"),
                        )
                        .with("name", name.as_str())
                        .with("matches", count)
                        .with("paths", Json::arr(paths.into_iter().map(Json::from))))
                    }
                }
            }
        }
    }

    /// Visits the subtree at `root` (`None` = the whole world) in path order: parents before
    /// children, siblings by name.
    pub fn walk<'a>(&'a self, root: Option<u64>, mut visit: impl FnMut(&'a Entity, &str)) {
        let mut stack: Vec<(u64, String)> = Vec::new();
        match root {
            Some(id) if self.entities.contains_key(&id) => stack.push((id, self.path_of(id))),
            Some(_) => return,
            None => {
                stack.extend(self.child_range(ROOT).map(|((_, name), &id)| (id, format!("/{name}"))));
                stack.reverse();
            }
        }
        while let Some((id, path)) = stack.pop() {
            let e = &self.entities[&id];
            visit(e, &path);
            let first = stack.len();
            stack.extend(self.child_range(id).map(|((_, name), &c)| (c, format!("{path}/{name}"))));
            stack[first..].reverse();
        }
    }

    pub fn spawn(&mut self, spec: SpawnSpec, by: &str) -> Result<&Entity, WorldError> {
        // Ids are only used up by spawns that succeed.
        let id = self.next_id + 1;
        self.insert(spec, id, 1, by, by)?;
        self.next_id = id;
        Ok(&self.entities[&id])
    }

    /// Re-creates a saved entity with its id, version and attribution.
    pub(crate) fn restore(
        &mut self,
        spec: SpawnSpec,
        id: u64,
        version: u64,
        created_by: &str,
        modified_by: &str,
    ) -> Result<(), WorldError> {
        if id == ROOT || self.entities.contains_key(&id) {
            return Err(WorldError::new("invalid_world", format!("entity id {id} is 0 or used twice")).with("field", "id"));
        }
        self.insert(spec, id, version.max(1), created_by, modified_by)?;
        self.next_id = self.next_id.max(id);
        Ok(())
    }

    fn insert(&mut self, spec: SpawnSpec, id: u64, version: u64, created_by: &str, modified_by: &str) -> Result<(), WorldError> {
        check_spawn(&spec)?;
        let props = canonical_props(spec.props)?;
        let parent = match &spec.parent {
            Some(key) => Some(self.resolve(key).map_err(|e| e.with("field", "parent"))?),
            None => None,
        };
        let pkey = parent.unwrap_or(ROOT);
        if let Some(p) = parent {
            if self.depth_of(p) + 1 > MAX_DEPTH {
                return Err(too_deep(&self.path_of(p)));
            }
        }
        let name = match spec.name {
            Some(n) => {
                validate_name(&n)?;
                if let Some(&other) = self.children.get(&(pkey, n.clone())) {
                    return Err(self.name_taken(&n, other));
                }
                n
            }
            None => {
                let base = format!("{}_{id}", spec.shape.name());
                let mut name = base.clone();
                let mut n = 2;
                while self.children.contains_key(&(pkey, name.clone())) {
                    name = format!("{base}_{n}");
                    n += 1;
                }
                name
            }
        };
        let (created_by, modified_by) = (self.intern(created_by), self.intern(modified_by));
        self.link(Entity {
            id,
            name,
            parent,
            shape: spec.shape,
            position: spec.position,
            rotation_deg: spec.rotation_deg,
            scale: spec.scale,
            color: spec.color,
            look: spec.look,
            light: spec.light,
            profile: spec.profile,
            tags: spec.tags,
            props,
            collider: spec.collider,
            body: spec.body,
            version,
            created_by,
            modified_by,
            world: Mat4::IDENTITY,
        });
        self.refresh(id);
        self.touch(id);
        self.revision += 1;
        Ok(())
    }

    fn name_taken(&self, name: &str, other: u64) -> WorldError {
        let path = self.path_of(other);
        WorldError::new("name_taken", format!("{path} already exists; names must be unique among siblings"))
            .with("name", name)
            .with("path", path)
            .with("id", other)
    }

    fn check_version(&self, id: u64, expect: Option<u64>) -> Result<(), WorldError> {
        let e = &self.entities[&id];
        match expect {
            Some(v) if v != e.version => {
                let path = self.path_of(id);
                Err(WorldError::new(
                    "stale_version",
                    format!("{path} is at version {}, not {v}: someone changed it since you read it; re-read and retry", e.version),
                )
                .with("path", path.as_str())
                .with("id", id)
                .with("expected_version", v)
                .with("current_version", e.version)
                .with("current", e.to_json(&path)))
            }
            _ => Ok(()),
        }
    }

    /// Changes an existing entity; its id never changes and its version goes up by one.
    pub fn update(&mut self, key: &EntityKey, change: UpdateSpec, by: &str) -> Result<&Entity, WorldError> {
        check_update(&change)?;
        let id = self.resolve(key)?;
        self.check_version(id, change.expect_version)?;
        let e = &self.entities[&id];
        if change.profile.is_some() && e.shape != Shape::Lathe {
            return Err(WorldError::new(
                "invalid_component",
                format!("'{}' is a {}; only lathe entities have a profile", e.name, e.shape.name()),
            )
            .with("field", "profile"));
        }
        let new_parent = match &change.parent {
            None => e.parent,
            Some(None) => None,
            Some(Some(k)) => Some(self.resolve(k).map_err(|err| err.with("field", "parent"))?),
        };
        let reparent = new_parent != e.parent;
        if reparent {
            if let Some(p) = new_parent {
                if self.is_ancestor_or_self(id, p) {
                    let (me, there) = (self.path_of(id), self.path_of(p));
                    return Err(WorldError::new("cycle", format!("{me} cannot move under {there}: that is itself or inside it"))
                        .with("path", me)
                        .with("parent", there));
                }
                if self.depth_of(p) + self.height_of(id) > MAX_DEPTH {
                    return Err(too_deep(&self.path_of(p)));
                }
            }
        }
        let renamed = change.name.as_ref().is_some_and(|n| *n != e.name);
        let relink = reparent || renamed;
        if relink {
            let name = change.name.as_deref().unwrap_or(&e.name);
            if let Some(&other) = self.children.get(&(new_parent.unwrap_or(ROOT), name.to_owned())) {
                return Err(self.name_taken(name, other));
            }
        }
        let tags = match (&change.tags, change.add_tags.is_empty() && change.remove_tags.is_empty()) {
            (None, true) => None,
            (base, _) => {
                let mut tags = base.clone().unwrap_or_else(|| e.tags.clone());
                tags.extend(change.add_tags.iter().cloned());
                for t in &change.remove_tags {
                    tags.remove(t);
                }
                if tags.len() > MAX_TAGS {
                    return Err(WorldError::new("invalid_component", format!("at most {MAX_TAGS} tags")).with("field", "tags"));
                }
                Some(tags)
            }
        };
        let props = match &change.props {
            None => None,
            Some(patch) => Some(merge_props(&e.props, patch)?),
        };

        let old = relink.then(|| (e.parent.unwrap_or(ROOT), e.name.clone()));
        if reparent {
            self.touch(id); // the subtree it leaves
        }
        let by = self.intern(by);
        let moved = reparent || change.position.is_some() || change.rotation_deg.is_some() || change.scale.is_some();
        let e = self.entities.get_mut(&id).expect("resolved above");
        if let Some(name) = change.name {
            e.name = name;
        }
        e.parent = new_parent;
        if let Some(p) = change.position {
            e.position = p;
        }
        if let Some(r) = change.rotation_deg {
            e.rotation_deg = r;
        }
        if let Some(s) = change.scale {
            e.scale = s;
        }
        if let Some(c) = change.color {
            e.color = c;
        }
        change.look.apply(&mut e.look);
        if let Some(l) = change.light {
            e.light = l;
        }
        if let Some(p) = change.profile {
            e.profile = Some(p);
        }
        if let Some(t) = tags {
            e.tags = t;
        }
        if let Some(p) = props {
            e.props = p;
        }
        if let Some(c) = change.collider {
            e.collider = c;
        }
        if let Some(b) = change.body {
            e.body = b;
        }
        e.version += 1;
        e.modified_by = by;
        if let Some((old_parent, old_name)) = old {
            let new_key = (e.parent.unwrap_or(ROOT), e.name.clone());
            self.names.remove(&(old_name.clone(), id));
            self.names.insert((new_key.1.clone(), id));
            self.children.remove(&(old_parent, old_name));
            self.children.insert(new_key, id);
        }
        if moved {
            self.refresh(id);
        }
        self.touch(id);
        self.revision += 1;
        Ok(&self.entities[&id])
    }

    /// Removes an entity and its whole subtree.
    pub fn delete(&mut self, key: &EntityKey, expect_version: Option<u64>) -> Result<Deleted, WorldError> {
        let id = self.resolve(key)?;
        self.check_version(id, expect_version)?;
        let path = self.path_of(id);
        self.touch_removal(id);
        let mut order = Vec::new();
        let mut stack = vec![id];
        while let Some(at) = stack.pop() {
            order.push(at);
            let first = stack.len();
            stack.extend(self.child_range(at).map(|(_, &c)| c));
            stack[first..].reverse();
        }
        let removed = order.into_iter().map(|i| self.unlink(i).expect("in the subtree")).collect();
        self.revision += 1;
        Ok(Deleted { id, path, removed })
    }

    /// Applies every op or none: on the first failure everything already applied is rolled back
    /// (ids, versions, indexes and the revision counter included) and the failing op's index is
    /// returned.
    /// Later ops can address entities spawned earlier in the batch by path.
    pub fn apply_batch(&mut self, ops: Vec<Op>, by: &str) -> Result<Vec<OpResult>, (usize, WorldError)> {
        let (next_id, revision) = (self.next_id, self.revision);
        let mut undo = Vec::with_capacity(ops.len());
        let mut results = Vec::with_capacity(ops.len());
        for (index, op) in ops.into_iter().enumerate() {
            let step = match op {
                Op::Spawn(spec) => self.spawn(spec, by).map(|e| e.id).map(|id| {
                    undo.push(Undo::Spawned(id));
                    OpResult::Spawned { id, path: self.path_of(id) }
                }),
                Op::Set(key, change) => self.resolve(&key).and_then(|id| {
                    let before = Box::new(self.entities[&id].clone());
                    let version = self.update(&EntityKey::Id(id), change, by)?.version;
                    undo.push(Undo::Updated(before));
                    Ok(OpResult::Updated { id, path: self.path_of(id), version })
                }),
                Op::Delete(key, expect) => self.delete(&key, expect).map(|d| {
                    let result = OpResult::Deleted { id: d.id, path: d.path, count: d.removed.len() };
                    undo.push(Undo::Deleted(d.removed));
                    result
                }),
            };
            match step {
                Ok(r) => results.push(r),
                Err(error) => {
                    self.rollback(undo);
                    self.next_id = next_id;
                    self.revision = revision;
                    return Err((index, error));
                }
            }
        }
        Ok(results)
    }

    fn rollback(&mut self, undo: Vec<Undo>) {
        let mut touched = Vec::new();
        for step in undo.into_iter().rev() {
            match step {
                Undo::Spawned(id) => {
                    self.touch_removal(id);
                    self.unlink(id);
                }
                Undo::Updated(before) => {
                    touched.push(before.id);
                    self.touch(before.id);
                    self.unlink(before.id);
                    self.link(*before);
                }
                Undo::Deleted(removed) => {
                    touched.extend(removed.first().map(|e| e.id));
                    for e in removed {
                        self.link(e);
                    }
                }
            }
        }
        // World files must rewrite every subtree the batch touched, even though its content is
        // back to what was saved: stamps only ever move forward.
        for id in touched {
            if self.entities.contains_key(&id) {
                self.refresh(id);
                self.touch(id);
            }
        }
    }

    fn link(&mut self, e: Entity) {
        self.children.insert((e.parent.unwrap_or(ROOT), e.name.clone()), e.id);
        self.names.insert((e.name.clone(), e.id));
        self.entities.insert(e.id, e);
    }

    /// Removes one entity from every index (its children are left in place).
    fn unlink(&mut self, id: u64) -> Option<Entity> {
        let e = self.entities.remove(&id)?;
        self.children.remove(&(e.parent.unwrap_or(ROOT), e.name.clone()));
        self.names.remove(&(e.name.clone(), id));
        Some(e)
    }

    /// Recomputes the world transforms of `id` and everything below it.
    fn refresh(&mut self, id: u64) {
        let mut stack = vec![id];
        while let Some(at) = stack.pop() {
            let Some(parent) = self.entities.get(&at).map(|e| e.parent) else { continue };
            let parent_world = parent.and_then(|p| self.entities.get(&p)).map(|p| p.world);
            let e = self.entities.get_mut(&at).expect("checked above");
            let local = e.local_matrix();
            // Top-level entities use their local matrix as is (no identity multiply to round).
            e.world = parent_world.map_or(local, |pw| pw * local);
            stack.extend(self.child_range(at).map(|(_, &c)| c));
        }
    }

    /// Matching entities in path order, paged.
    pub fn query(&self, q: &Query) -> Result<QueryPage<'_>, WorldError> {
        let root = match &q.path_prefix {
            Some(p) => Some(self.find_path(p).map_err(|e| e.with("field", "path_prefix"))?),
            None => None,
        };
        let mut page = QueryPage { total: 0, items: Vec::new(), more: false };
        self.walk(root, |e, path| {
            let matches = q.tags.iter().all(|t| e.tags.contains(t))
                && q.shape.is_none_or(|s| s == e.shape)
                && q.material.is_none_or(|m| m == e.look.material && e.shape != Shape::Group)
                && q.created_by.as_ref().is_none_or(|c| c.as_str() == &*e.created_by)
                && q.region.is_none_or(|(min, max)| {
                    let (lo, hi) = e.world_aabb();
                    lo.x <= max.x && hi.x >= min.x && lo.y <= max.y && hi.y >= min.y && lo.z <= max.z && hi.z >= min.z
                });
            if !matches {
                return;
            }
            page.total += 1;
            if q.after.as_ref().is_some_and(|after| path_cmp(path, after) != Ordering::Greater) {
                return;
            }
            if page.items.len() < q.limit {
                page.items.push((e, path.to_owned()));
            } else {
                page.more = true;
            }
        });
        Ok(page)
    }

    /// The requested [`FIELDS`] of an entity, in the order asked (absent values are `null`).
    pub fn fields_json(&self, e: &Entity, path: &str, fields: &[String]) -> Json {
        let full = e.to_json(path);
        let (look, s) = (&e.look, &e.look.surface);
        Json::obj(fields.iter().map(|f| {
            let v = match f.as_str() {
                "parent" => e.parent.map_or(Json::Null, |p| Json::from(self.path_of(p))),
                "world_position" => vec3_json(e.world_position()),
                "opacity" => Json::from(look.opacity),
                "emissive" => Json::from(s.emissive),
                "metallic" => Json::from(s.metallic),
                "roughness" => Json::from(s.roughness),
                "specular" => Json::from(s.specular),
                "seed" => Json::from(s.seed),
                "color2" => Json::from(color_hex(s.color2)),
                "tags" => tags_json(&e.tags),
                "props" => e.props.clone(),
                "children" => Json::from(self.child_range(e.id).count()),
                "bounds" => {
                    let (lo, hi) = e.world_aabb();
                    Json::obj([("min", vec3_json(lo)), ("max", vec3_json(hi))])
                }
                other => full.get(other).cloned().unwrap_or(Json::Null),
            };
            (f.clone(), v)
        }))
    }

    /// World-space bounds of everything with geometry, or `None` when there is none.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut corners = self.entities.values().filter_map(|e| e.world_corners()).flatten();
        let first = corners.next()?;
        Some(corners.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// A hash of everything that defines the world (entities by id, the environment, the id
    /// counter), stable across runs and platforms. Versions and attribution are left out.
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x5370_6563_756c_6131);
        h.write_u64(self.next_id);
        let l = &self.env.light;
        for c in [l.sun_direction, l.sun_color, l.ambient_color, self.env.background] {
            c.iter().for_each(|v| h.write_f32(*v));
        }
        for v in [l.sun_intensity, l.ambient_intensity, l.exposure] {
            h.write_f32(v);
        }
        h.write_bool(l.tonemap);
        h.write_u64(self.entities.len() as u64);
        for e in self.entities.values() {
            e.hash_into(&mut h);
        }
        h.finish()
    }
}

fn too_deep(parent: &str) -> WorldError {
    WorldError::new("too_deep", format!("hierarchies are at most {MAX_DEPTH} levels deep (under {parent})"))
        .with("parent", parent)
        .with("max_depth", MAX_DEPTH)
}

/// Compares paths segment by segment (the order [`World::walk`] visits them in).
pub fn path_cmp(a: &str, b: &str) -> Ordering {
    a.split('/').cmp(b.split('/'))
}

/// Names are 1..=128 characters, with no `/`, control characters or surrounding whitespace, and
/// are not `.` or `..` (so paths read like file paths).
pub fn validate_name(name: &str) -> Result<(), WorldError> {
    let why = if name.is_empty() {
        "names must not be empty"
    } else if name.chars().count() > MAX_NAME_CHARS {
        "names must be at most 128 characters"
    } else if name.contains('/') {
        "names must not contain '/' (it separates path segments)"
    } else if name.chars().any(char::is_control) {
        "names must not contain control characters"
    } else if name.trim() != name {
        "names must not start or end with whitespace"
    } else if name == "." || name == ".." {
        "'.' and '..' are not names"
    } else {
        return Ok(());
    };
    Err(WorldError::new("invalid_name", why).with("name", name))
}

/// Tags are 1..=64 characters of `A-Z a-z 0-9 _ - . :`.
pub fn validate_tag(tag: &str) -> Result<(), WorldError> {
    let ok = (1..=MAX_TAG_CHARS).contains(&tag.len())
        && tag.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'));
    if ok {
        Ok(())
    } else {
        Err(WorldError::new("invalid_tag", format!("'{tag}': tags are 1..{MAX_TAG_CHARS} characters of A-Z a-z 0-9 _ - . :"))
            .with("tag", tag))
    }
}

fn tags_json(tags: &BTreeSet<String>) -> Json {
    Json::arr(tags.iter().map(|t| Json::from(t.as_str())))
}

/// Sorts object keys recursively (first of any duplicate wins, as [`Json::get`] reads), so equal
/// data always serializes, saves and hashes the same.
fn canonical(v: Json) -> Json {
    match v {
        Json::Obj(pairs) => {
            let mut map: BTreeMap<String, Json> = BTreeMap::new();
            for (k, v) in pairs {
                map.entry(k).or_insert_with(|| canonical(v));
            }
            Json::Obj(map.into_iter().collect())
        }
        Json::Arr(items) => Json::Arr(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

/// `props` as stored: an object (null = empty), canonical, at most [`MAX_PROPS_BYTES`].
pub fn canonical_props(props: Json) -> Result<Json, WorldError> {
    let props = match props {
        Json::Null => Json::Obj(Vec::new()),
        v @ Json::Obj(_) => canonical(v),
        _ => return Err(WorldError::new("invalid_component", "'props' must be a JSON object").with("field", "props")),
    };
    let size = props.to_string().len();
    if size > MAX_PROPS_BYTES {
        return Err(WorldError::new("invalid_component", format!("'props' is {size} bytes; at most {MAX_PROPS_BYTES}"))
            .with("field", "props"));
    }
    Ok(props)
}

/// `current` with `patch`'s keys set (a `null` value removes the key).
fn merge_props(current: &Json, patch: &Json) -> Result<Json, WorldError> {
    let Some(changes) = patch.as_object() else {
        return Err(WorldError::new("invalid_component", "'props' must be a JSON object").with("field", "props"));
    };
    let mut map: BTreeMap<String, Json> = current.as_object().unwrap_or_default().iter().cloned().collect();
    for (k, v) in changes {
        if v.is_null() {
            map.remove(k);
        } else {
            map.insert(k.clone(), v.clone());
        }
    }
    canonical_props(Json::Obj(map.into_iter().collect()))
}

pub fn vec3_json(v: Vec3) -> Json {
    Json::arr([Json::from(v.x), Json::from(v.y), Json::from(v.z)])
}

pub fn color_hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// `"#rrggbb"` or `"#rgb"` → sRGB components (the one conversion every reader uses, so saved hex
/// colours read back to the same bits).
pub fn color_from_hex(s: &str) -> Option<[f32; 3]> {
    let digits: Vec<u32> = s.strip_prefix('#')?.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
    let channel = |hi: u32, lo: u32| (hi * 16 + lo) as f32 / 255.0;
    match *digits.as_slice() {
        [r, g, b] => Some([channel(r, r), channel(g, g), channel(b, b)]),
        [r1, r2, g1, g2, b1, b2] => Some([channel(r1, r2), channel(g1, g2), channel(b1, b2)]),
        _ => None,
    }
}

/// Hex when that reads back bit-exactly, `[r, g, b]` otherwise: compact and lossless.
pub fn color_json_exact(c: [f32; 3]) -> Json {
    let hex = color_hex(c);
    let exact = color_from_hex(&hex).is_some_and(|back| back.iter().zip(c).all(|(a, b)| a.to_bits() == b.to_bits()));
    if exact {
        Json::from(hex)
    } else {
        Json::arr(c.map(Json::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lathe() -> Profile {
        Profile { points: vec![[0.5, 0.0], [0.3, 2.0]], segments: 12 }
    }

    /// Spawns `shape` named `name` under `parent` (a path) at `position`.
    fn put(w: &mut World, shape: Shape, name: &str, parent: Option<&str>, position: Vec3) -> u64 {
        let mut spec = SpawnSpec::new(shape);
        spec.name = Some(name.into());
        spec.parent = parent.map(|p| EntityKey::Path(p.into()));
        spec.position = position;
        w.spawn(spec, "t").unwrap().id
    }

    /// Everything a rollback must restore: entities (versions, attribution, cached world
    /// transforms), both indexes, the id and revision counters, the environment.
    fn snapshot(w: &World) -> String {
        format!("{:?}|{:?}|{:?}|{}|{}|{:?}", w.entities, w.children, w.names, w.next_id, w.revision, w.env)
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn lathe_needs_exactly_a_profile() {
        let mut w = World::default();
        assert!(w.spawn(SpawnSpec::new(Shape::Lathe), "t").is_err());
        let mut cube = SpawnSpec::new(Shape::Cube);
        cube.profile = Some(lathe());
        assert!(w.spawn(cube, "t").is_err());
        let mut spec = SpawnSpec::new(Shape::Lathe);
        spec.profile = Some(lathe());
        let e = w.spawn(spec, "t").unwrap();
        assert_eq!(e.local_bounds(), Some((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5))));
        let id = e.id;
        let change = UpdateSpec { profile: Some(Profile { segments: 8, ..lathe() }), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(id), change, "t").unwrap().profile.as_ref().unwrap().segments, 8);
        assert_eq!(w.next_id(), 1, "failed spawns use no ids");
    }

    #[test]
    fn profile_key_tracks_every_value() {
        let a = lathe();
        let mut b = lathe();
        assert_eq!(a.key(), b.key());
        b.points[1][1] = 2.001;
        assert_ne!(a.key(), b.key());
        b = lathe();
        b.segments = 13;
        assert_ne!(a.key(), b.key());
    }

    #[test]
    fn look_and_light_round_trip_through_updates() {
        let mut w = World::default();
        let mut spec = SpawnSpec::new(Shape::Sphere);
        spec.color = [1.0, 0.5, 0.0];
        spec.light = Some(LightSpec { intensity: 2.0, range: 3.0, color: None, offset: Vec3::new(0.0, 0.5, 0.0) });
        let id = w.spawn(spec, "t").unwrap().id;
        let light = w.get(id).unwrap().world_light().unwrap();
        assert_eq!((light.position, light.color), ([0.0, 0.5, 0.0], [1.0, 0.5, 0.0]));
        let look = LookPatch { material: Some(Material::Glass), opacity: Some(0.3), ..Default::default() };
        let e = w.update(&EntityKey::Id(id), UpdateSpec { look, light: Some(None), ..Default::default() }, "t").unwrap();
        assert_eq!((e.look.material, e.look.opacity, e.light), (Material::Glass, 0.3, None));
        let json = e.to_json("/x");
        assert_eq!(json.get("material").and_then(Json::as_str), Some("glass"));
        assert!(json.get("emissive").is_none(), "defaults are left out");
    }

    #[test]
    fn paths_and_sibling_unique_names() {
        let mut w = World::default();
        put(&mut w, Shape::Group, "slab", None, Vec3::ZERO);
        put(&mut w, Shape::Group, "stones", Some("/slab"), Vec3::ZERO);
        let ward = put(&mut w, Shape::Cube, "ward_03", Some("/slab/stones"), Vec3::ZERO);
        assert_eq!(w.path_of(ward), "/slab/stones/ward_03");
        assert_eq!(w.find_path("/slab/stones/ward_03"), Ok(ward));
        // Same name under a different parent is fine; under the same parent it is not.
        put(&mut w, Shape::Cube, "ward_03", Some("/slab"), Vec3::ZERO);
        let mut dup = SpawnSpec::new(Shape::Cube);
        dup.name = Some("ward_03".into());
        dup.parent = Some(EntityKey::Path("/slab".into()));
        assert_eq!(w.spawn(dup, "t").err().unwrap().code, "name_taken");
        for (bad, code) in [("slab", "invalid_path"), ("/", "invalid_path"), ("/slab//x", "invalid_path"), ("/slab/nope", "not_found")] {
            assert_eq!(w.find_path(bad).err().unwrap().code, code, "{bad}");
        }
        let missing = w.find_path("/slab/stones/nope/deeper").err().unwrap();
        assert_eq!(missing.detail("deepest_existing"), Some(&Json::from("/slab/stones")));
        for bad in ["a/b", "", " x", "..", "a\nb"] {
            assert_eq!(validate_name(bad).err().unwrap().code, "invalid_name", "{bad:?}");
        }
    }

    #[test]
    fn bare_names_resolve_only_when_unique() {
        let mut w = World::default();
        put(&mut w, Shape::Group, "a", None, Vec3::ZERO);
        put(&mut w, Shape::Group, "b", None, Vec3::ZERO);
        let lone = put(&mut w, Shape::Cube, "lone", Some("/a"), Vec3::ZERO);
        put(&mut w, Shape::Cube, "door", Some("/a"), Vec3::ZERO);
        put(&mut w, Shape::Cube, "door", Some("/b"), Vec3::ZERO);
        assert_eq!(w.resolve(&EntityKey::Name("lone".into())), Ok(lone));
        assert_eq!(w.resolve(&EntityKey::Name("/a/lone".into())), Ok(lone), "a name starting with / is a path");
        let err = w.resolve(&EntityKey::Name("door".into())).err().unwrap();
        assert_eq!(err.code, "ambiguous_name");
        assert_eq!(err.detail("paths"), Some(&Json::arr([Json::from("/a/door"), Json::from("/b/door")])));
        assert_eq!(w.resolve(&EntityKey::Name("nobody".into())).err().unwrap().code, "not_found");
    }

    #[test]
    fn world_transforms_compose_and_follow_parents() {
        let mut w = World::default();
        let mut base = SpawnSpec::new(Shape::Group);
        base.name = Some("base".into());
        base.position = Vec3::new(10.0, 0.0, 0.0);
        base.rotation_deg = Vec3::new(0.0, 90.0, 0.0);
        base.scale = Vec3::new(2.0, 2.0, 2.0);
        let base = w.spawn(base, "t").unwrap().id;
        let arm = put(&mut w, Shape::Cube, "arm", Some("/base"), Vec3::new(1.0, 0.0, 0.0));
        let mut lamp = SpawnSpec::new(Shape::Sphere);
        lamp.parent = Some(EntityKey::Id(arm));
        lamp.position = Vec3::new(0.0, 1.0, 0.0);
        lamp.light = Some(LightSpec { intensity: 1.0, range: 2.0, color: None, offset: Vec3::ZERO });
        let lamp = w.spawn(lamp, "t").unwrap().id;
        // +X rotated 90° about Y is -Z, scaled by 2: the arm sits 2 m north of the base.
        assert!(close(w.get(arm).unwrap().world_position(), Vec3::new(10.0, 0.0, -2.0)));
        assert!(close(w.get(lamp).unwrap().world_position(), Vec3::new(10.0, 2.0, -2.0)));
        let p = w.get(lamp).unwrap().world_light().unwrap().position;
        assert!(close(Vec3::new(p[0], p[1], p[2]), Vec3::new(10.0, 2.0, -2.0)), "lights use world transforms");
        // Moving the base moves the whole subtree.
        w.update(&EntityKey::Id(base), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }, "t").unwrap();
        assert!(close(w.get(lamp).unwrap().world_position(), Vec3::new(0.0, 2.0, -2.0)));
        assert!(w.get(base).unwrap().world_corners().is_none(), "groups have no geometry");
        let (lo, hi) = w.bounds().unwrap();
        assert!(lo.y >= -1.0 && hi.y <= 3.0, "{lo:?} {hi:?}");
    }

    #[test]
    fn reparenting_keeps_local_transform_and_rejects_cycles() {
        let mut w = World::default();
        let a = put(&mut w, Shape::Group, "a", None, Vec3::new(5.0, 0.0, 0.0));
        let b = put(&mut w, Shape::Group, "b", Some("/a"), Vec3::ZERO);
        let c = put(&mut w, Shape::Cube, "c", None, Vec3::new(1.0, 0.0, 0.0));
        let to = |p: Option<&str>| UpdateSpec {
            parent: Some(p.map(|p| EntityKey::Path(p.into()))),
            ..Default::default()
        };
        let e = w.update(&EntityKey::Id(c), to(Some("/a/b")), "t").unwrap();
        assert_eq!(e.position, Vec3::new(1.0, 0.0, 0.0), "local transform kept");
        assert!(close(e.world_position(), Vec3::new(6.0, 0.0, 0.0)));
        assert_eq!(w.path_of(c), "/a/b/c");
        assert_eq!(w.update(&EntityKey::Id(a), to(Some("/a/b")), "t").err().unwrap().code, "cycle");
        assert_eq!(w.update(&EntityKey::Id(a), to(Some("/a")), "t").err().unwrap().code, "cycle");
        w.update(&EntityKey::Id(c), to(None), "t").unwrap();
        assert_eq!(w.path_of(c), "/c");
        assert!(close(w.get(c).unwrap().world_position(), Vec3::new(1.0, 0.0, 0.0)));
        // A rename into a taken sibling name fails and changes nothing.
        put(&mut w, Shape::Cube, "d", Some("/a/b"), Vec3::ZERO);
        let clash = UpdateSpec { name: Some("d".into()), parent: Some(Some(EntityKey::Id(b))), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(c), clash, "t").err().unwrap().code, "name_taken");
        assert_eq!(w.path_of(c), "/c");
    }

    #[test]
    fn depth_is_capped() {
        let mut w = World::default();
        let mut parent: Option<u64> = None;
        for i in 0..MAX_DEPTH {
            let mut spec = SpawnSpec::new(Shape::Group);
            spec.name = Some(format!("n{i}"));
            spec.parent = parent.map(EntityKey::Id);
            parent = Some(w.spawn(spec, "t").unwrap().id);
        }
        let mut spec = SpawnSpec::new(Shape::Group);
        spec.parent = parent.map(EntityKey::Id);
        assert_eq!(w.spawn(spec, "t").err().unwrap().code, "too_deep");
        // Moving a two-level subtree under the deepest-but-one entity also overflows.
        let top = put(&mut w, Shape::Group, "top", None, Vec3::ZERO);
        put(&mut w, Shape::Group, "kid", Some("/top"), Vec3::ZERO);
        let change = UpdateSpec { parent: Some(parent.map(EntityKey::Id)), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(top), change, "t").err().unwrap().code, "too_deep");
    }

    #[test]
    fn delete_removes_the_subtree() {
        let mut w = World::default();
        put(&mut w, Shape::Group, "a", None, Vec3::ZERO);
        put(&mut w, Shape::Group, "b", Some("/a"), Vec3::ZERO);
        put(&mut w, Shape::Cube, "c", Some("/a/b"), Vec3::ZERO);
        put(&mut w, Shape::Cube, "d", Some("/a"), Vec3::ZERO);
        let keep = put(&mut w, Shape::Cube, "keep", None, Vec3::ZERO);
        let d = w.delete(&EntityKey::Path("/a".into()), None).unwrap();
        assert_eq!((d.path.as_str(), d.removed.len()), ("/a", 4));
        let names: Vec<&str> = d.removed.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", "d"], "parents first");
        assert_eq!(w.len(), 1);
        assert_eq!(w.find_path("/keep"), Ok(keep));
        assert_eq!(w.resolve(&EntityKey::Name("c".into())).err().unwrap().code, "not_found");
    }

    #[test]
    fn versions_reject_stale_writes() {
        let mut w = World::default();
        let id = put(&mut w, Shape::Cube, "box", None, Vec3::ZERO);
        assert_eq!(w.get(id).unwrap().version, 1);
        let mv = |v: Option<u64>, x: f32| UpdateSpec { position: Some(Vec3::new(x, 0.0, 0.0)), expect_version: v, ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(id), mv(Some(1), 1.0), "alice").unwrap().version, 2);
        let stale = w.update(&EntityKey::Id(id), mv(Some(1), 2.0), "bob").err().unwrap();
        assert_eq!(stale.code, "stale_version");
        assert_eq!(stale.detail("current_version"), Some(&Json::from(2)));
        let current = stale.detail("current").unwrap();
        assert_eq!(current.get("modified_by"), Some(&Json::from("alice")));
        assert_eq!(current.get("position"), Some(&vec3_json(Vec3::new(1.0, 0.0, 0.0))));
        assert_eq!(w.get(id).unwrap().position.x, 1.0, "the stale write changed nothing");
        assert_eq!(w.delete(&EntityKey::Id(id), Some(1)).err().unwrap().code, "stale_version");
        let e = w.get(id).unwrap();
        assert_eq!((&*e.created_by, &*e.modified_by), ("t", "alice"));
        assert!(w.delete(&EntityKey::Id(id), Some(2)).is_ok());
    }

    #[test]
    fn tags_props_and_physics_components() {
        let mut w = World::default();
        let mut spec = SpawnSpec::new(Shape::Cube);
        spec.tags = ["door".to_string(), "wood".to_string()].into();
        spec.props = Json::parse(r#"{"z":1,"a":{"y":2,"b":[{"d":1,"c":2}]}}"#).unwrap();
        spec.collider = Some(Collider {
            shape: ColliderShape::Capsule { radius: 0.3, height: 1.8 },
            offset: Vec3::ZERO,
            is_trigger: false,
            layer: "default".into(),
        });
        spec.body = Some(Body { kind: BodyKind::Kinematic, mass: 80.0 });
        let id = w.spawn(spec, "t").unwrap().id;
        let e = w.get(id).unwrap();
        assert_eq!(e.props.to_string(), r#"{"a":{"b":[{"c":2,"d":1}],"y":2},"z":1}"#, "keys sorted recursively");
        let change = UpdateSpec {
            add_tags: vec!["open".into()],
            remove_tags: vec!["wood".into()],
            props: Some(Json::parse(r#"{"z":null,"hp":3}"#).unwrap()),
            body: Some(None),
            ..Default::default()
        };
        let e = w.update(&EntityKey::Id(id), change, "t").unwrap();
        assert_eq!(e.tags.iter().map(String::as_str).collect::<Vec<_>>(), ["door", "open"]);
        assert_eq!(e.props.to_string(), r#"{"a":{"b":[{"c":2,"d":1}],"y":2},"hp":3}"#);
        assert_eq!(e.body, None);
        let json = e.to_json("/cube_1");
        assert_eq!(json.get("collider").unwrap().to_string(), r#"{"shape":"capsule","radius":0.3,"height":1.8}"#);
        for bad in [
            UpdateSpec { add_tags: vec!["has space".into()], ..Default::default() },
            UpdateSpec { props: Some(Json::from(3)), ..Default::default() },
            UpdateSpec { props: Some(Json::obj([("big", Json::from("x".repeat(MAX_PROPS_BYTES)))])), ..Default::default() },
            UpdateSpec {
                collider: Some(Some(Collider {
                    shape: ColliderShape::Capsule { radius: 1.0, height: 1.0 },
                    offset: Vec3::ZERO,
                    is_trigger: false,
                    layer: "default".into(),
                })),
                ..Default::default()
            },
        ] {
            let version = w.get(id).unwrap().version;
            assert!(w.update(&EntityKey::Id(id), bad, "t").is_err());
            assert_eq!(w.get(id).unwrap().version, version, "rejected changes don't count");
        }
    }

    #[test]
    fn batches_are_all_or_nothing() {
        let mut w = World::default();
        put(&mut w, Shape::Group, "keep", None, Vec3::ZERO);
        put(&mut w, Shape::Cube, "old", Some("/keep"), Vec3::ZERO);
        let before = snapshot(&w);
        let named = |name: &str, parent: Option<&str>| {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(name.into());
            s.parent = parent.map(|p| EntityKey::Path(p.into()));
            s
        };
        let ops = vec![
            Op::Spawn(named("new", None)),
            Op::Spawn(named("child", Some("/new"))),
            Op::Set(EntityKey::Path("/keep/old".into()), UpdateSpec {
                name: Some("renamed".into()),
                parent: Some(Some(EntityKey::Path("/new".into()))),
                ..Default::default()
            }),
            Op::Delete(EntityKey::Path("/keep".into()), None),
            Op::Set(EntityKey::Path("/new/child".into()), UpdateSpec { position: Some(Vec3::ONE), ..Default::default() }),
            Op::Delete(EntityKey::Path("/nope".into()), None),
        ];
        let (index, err) = w.apply_batch(ops, "t").err().unwrap();
        assert_eq!((index, err.code), (5, "not_found"));
        assert_eq!(snapshot(&w), before, "rolled back exactly: entities, indexes, counters");
        assert_eq!(w.path_of(w.find_path("/keep/old").unwrap()), "/keep/old");
        assert_eq!(w.len(), 2);

        let ops = vec![
            Op::Spawn(named("new", None)),
            Op::Spawn(named("child", Some("/new"))),
            Op::Set(EntityKey::Path("/new/child".into()), UpdateSpec { position: Some(Vec3::ONE), ..Default::default() }),
            Op::Delete(EntityKey::Path("/keep".into()), None),
        ];
        let results = w.apply_batch(ops, "t").unwrap();
        assert_eq!(results[1], OpResult::Spawned { id: 4, path: "/new/child".into() });
        assert_eq!(results[2], OpResult::Updated { id: 4, path: "/new/child".into(), version: 2 });
        assert_eq!(results[3], OpResult::Deleted { id: 1, path: "/keep".into(), count: 2 });
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn rollback_restores_world_transforms() {
        let mut w = World::default();
        put(&mut w, Shape::Group, "a", None, Vec3::new(3.0, 0.0, 0.0));
        let kid = put(&mut w, Shape::Cube, "kid", Some("/a"), Vec3::new(1.0, 0.0, 0.0));
        let ops = vec![
            Op::Set(EntityKey::Path("/a".into()), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }),
            Op::Delete(EntityKey::Path("/a/kid".into()), Some(99)),
        ];
        assert_eq!(w.apply_batch(ops, "t").err().unwrap().1.code, "stale_version");
        assert!(close(w.get(kid).unwrap().world_position(), Vec3::new(4.0, 0.0, 0.0)));
    }

    #[test]
    fn queries_filter_and_page_in_path_order() {
        let mut w = World::default();
        put(&mut w, Shape::Group, "b", None, Vec3::ZERO);
        put(&mut w, Shape::Group, "a", None, Vec3::new(100.0, 0.0, 0.0));
        for i in 0..7 {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(format!("c{i}"));
            s.parent = Some(EntityKey::Path("/a".into()));
            s.position = Vec3::new(i as f32 * 2.0, 0.0, 0.0);
            if i % 2 == 0 {
                s.tags = ["even".to_string()].into();
            }
            w.spawn(s, if i < 3 { "alice" } else { "bob" }).unwrap();
        }
        put(&mut w, Shape::Sphere, "s", Some("/b"), Vec3::ZERO);
        let paths = |q: &Query| -> (usize, Vec<String>, bool) {
            let p = w.query(q).unwrap();
            (p.total, p.items.into_iter().map(|(_, path)| path).collect(), p.more)
        };
        let all = paths(&Query { limit: 100, ..Default::default() });
        assert_eq!(all.0, 10);
        assert_eq!(&all.1[..3], ["/a", "/a/c0", "/a/c1"], "path order: parents first, then by name");
        assert_eq!(&all.1[8..], ["/b", "/b/s"]);

        // Paging walks the same order with no gaps or repeats.
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let (total, items, more) = paths(&Query { limit: 3, after: after.clone(), ..Default::default() });
            assert_eq!(total, 10);
            after = items.last().cloned();
            seen.extend(items);
            if !more {
                break;
            }
        }
        assert_eq!(seen, all.1);

        let q = |f: fn(&mut Query)| {
            let mut q = Query { limit: 100, ..Default::default() };
            f(&mut q);
            paths(&q).1
        };
        assert_eq!(q(|q| q.tags = vec!["even".into()]).len(), 4);
        assert_eq!(q(|q| q.created_by = Some("alice".into())), ["/a/c0", "/a/c1", "/a/c2"]);
        assert_eq!(q(|q| q.shape = Some(Shape::Sphere)), ["/b/s"]);
        assert_eq!(q(|q| q.path_prefix = Some("/b".into())), ["/b", "/b/s"]);
        assert_eq!(q(|q| q.material = Some(Material::Lit)).len(), 8, "groups have no material");
        // c3 is at x = 106 (± 0.5).
        assert_eq!(q(|q| q.region = Some((Vec3::new(105.6, -1.0, -1.0), Vec3::new(106.4, 1.0, 1.0)))), ["/a/c3"]);
        assert_eq!(w.query(&Query { path_prefix: Some("/zz".into()), limit: 1, ..Default::default() }).err().unwrap().code, "not_found");
    }

    #[test]
    fn state_hash_tracks_content_not_history() {
        let build = |by: &str| {
            let mut w = World::default();
            put(&mut w, Shape::Group, "g", None, Vec3::ZERO);
            let mut s = SpawnSpec::new(Shape::Cube);
            s.parent = Some(EntityKey::Path("/g".into()));
            w.spawn(s, by).unwrap();
            w
        };
        let (a, mut b) = (build("alice"), build("bob"));
        assert_eq!(a.state_hash(), b.state_hash(), "attribution is not state");
        let id = b.find_path("/g/cube_2").unwrap();
        b.update(&EntityKey::Id(id), UpdateSpec { position: Some(Vec3::new(0.0, 0.0, -0.0)), ..Default::default() }, "x").unwrap();
        assert_ne!(a.state_hash(), b.state_hash(), "-0 is a different bit pattern");
        b.update(&EntityKey::Id(id), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }, "x").unwrap();
        assert_eq!(a.state_hash(), b.state_hash());
        b.set_env(&EnvPatch { exposure: Some(2.0), ..Default::default() }).unwrap();
        assert_ne!(a.state_hash(), b.state_hash());
    }

    /// Reviewer finding: the in-process API accepted values the loader rejects, so autosave could
    /// write a world that can't be loaded again.
    #[test]
    fn the_rust_api_enforces_the_same_limits_as_the_tools() {
        let mut w = World::default();
        let field = |e: WorldError| e.detail("field").and_then(Json::as_str).map(str::to_owned);
        let spawn = |w: &mut World, f: fn(&mut SpawnSpec)| {
            let mut s = SpawnSpec::new(Shape::Cube);
            f(&mut s);
            w.spawn(s, "game").err().map(field)
        };
        assert_eq!(spawn(&mut w, |s| s.position.x = f32::NAN), Some(Some("position".into())));
        assert_eq!(spawn(&mut w, |s| s.position.y = 2.0e6), Some(Some("position".into())));
        assert_eq!(spawn(&mut w, |s| s.rotation_deg.z = f32::INFINITY), Some(Some("rotation_deg".into())));
        assert_eq!(spawn(&mut w, |s| s.scale.x = 0.0), Some(Some("scale".into())));
        assert_eq!(spawn(&mut w, |s| s.color = [1.5, 0.0, 0.0]), Some(Some("color".into())));
        assert_eq!(spawn(&mut w, |s| s.look.surface.emissive = -1.0), Some(Some("emissive".into())));
        assert_eq!(
            spawn(&mut w, |s| s.light = Some(LightSpec { intensity: f32::NAN, range: 1.0, color: None, offset: Vec3::ZERO })),
            Some(Some("light.intensity".into()))
        );
        assert_eq!(spawn(&mut w, |s| s.body = Some(Body { kind: BodyKind::Dynamic, mass: 0.0 })), Some(Some("body.mass".into())));
        assert_eq!(w.len(), 0);
        let id = w.spawn(SpawnSpec::new(Shape::Cube), "game").unwrap().id;
        let bad = UpdateSpec { position: Some(Vec3::new(0.0, f32::NAN, 0.0)), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(id), bad, "game").err().map(field), Some(Some("position".into())));
        let bad = UpdateSpec { color: Some([0.0, 2.0, 0.0]), ..Default::default() };
        assert!(w.update(&EntityKey::Id(id), bad, "game").is_err());
        assert_eq!(w.get(id).unwrap().version, 1, "rejected updates change nothing");
        let env = EnvPatch { exposure: Some(f32::NAN), ..Default::default() };
        assert_eq!(w.set_env(&env).err().map(field), Some(Some("exposure".into())));
        let env = EnvPatch { sun_direction: Some(Vec3::ZERO), ..Default::default() };
        assert!(w.set_env(&env).is_err());
        assert_eq!(*w.env(), Env::default());
    }

    #[test]
    fn subtree_stamps_track_changes_per_top_level_subtree() {
        let mut w = World::default();
        let a = put(&mut w, Shape::Group, "a", None, Vec3::ZERO);
        let b = put(&mut w, Shape::Group, "b", None, Vec3::ZERO);
        let leaf = put(&mut w, Shape::Cube, "leaf", Some("/a"), Vec3::ZERO);
        let (sa, sb) = (w.subtree_stamp(a), w.subtree_stamp(b));
        w.update(&EntityKey::Id(leaf), UpdateSpec { position: Some(Vec3::ONE), ..Default::default() }, "t").unwrap();
        assert!(w.subtree_stamp(a) > sa && w.subtree_stamp(b) == sb, "a deep change marks only its top");
        let (sa, sb) = (w.subtree_stamp(a), w.subtree_stamp(b));
        let to_b = UpdateSpec { parent: Some(Some(EntityKey::Id(b))), ..Default::default() };
        w.update(&EntityKey::Id(leaf), to_b, "t").unwrap();
        assert!(w.subtree_stamp(a) > sa && w.subtree_stamp(b) > sb, "a move marks both subtrees");
        let sb = w.subtree_stamp(b);
        let ops = vec![
            Op::Set(EntityKey::Id(leaf), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }),
            Op::Delete(EntityKey::Path("/nope".into()), None),
        ];
        assert!(w.apply_batch(ops, "t").is_err());
        assert!(w.subtree_stamp(b) > sb, "stamps never go back, even when a batch does");
        let (one, two) = (Arc::as_ptr(&w.get(a).unwrap().created_by), Arc::as_ptr(&w.get(b).unwrap().created_by));
        assert_eq!(one, two, "agent names are interned");
        // Removed top-level subtrees leave no stamp behind (also when a batch spawn is undone).
        w.delete(&EntityKey::Id(a), None).unwrap();
        let ops = vec![
            Op::Spawn(SpawnSpec::new(Shape::Group)),
            Op::Delete(EntityKey::Path("/nope".into()), None),
        ];
        assert!(w.apply_batch(ops, "t").is_err());
        assert_eq!(w.touched.keys().copied().collect::<Vec<_>>(), [b]);
    }

    #[test]
    fn exact_colours() {
        assert_eq!(color_json_exact([1.0, 0.0, 136.0 / 255.0]), Json::from("#ff0088"));
        assert_eq!(color_json_exact([0.8, 0.8, 0.8]), Json::from("#cccccc"), "204/255 rounds to 0.8f32");
        assert_eq!(color_json_exact([0.81, 0.8, 0.8]), Json::arr([0.81f32, 0.8, 0.8].map(Json::from)));
    }

    #[test]
    fn ten_thousand_entities_stay_fast() {
        let mut w = World::default();
        let started = std::time::Instant::now();
        for d in 0..100 {
            put(&mut w, Shape::Group, &format!("district_{d:03}"), None, Vec3::new(d as f32 * 50.0, 0.0, 0.0));
            let parent = format!("/district_{d:03}");
            for i in 0..99 {
                put(&mut w, Shape::Cube, &format!("house_{i:02}"), Some(&parent), Vec3::new(0.0, 0.0, i as f32));
            }
        }
        assert_eq!(w.len(), 10_000);
        let id = w.find_path("/district_042/house_17").unwrap();
        let page = w.query(&Query { tags: vec![], limit: 50, after: Some("/district_099/house_97".into()), ..Default::default() }).unwrap();
        assert_eq!((page.total, page.items.len()), (10_000, 1));
        assert!(close(w.get(id).unwrap().world_position(), Vec3::new(2100.0, 0.0, 17.0)));
        let moved = w.find_path("/district_042").unwrap();
        w.update(&EntityKey::Id(moved), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }, "t").unwrap();
        assert!(close(w.get(id).unwrap().world_position(), Vec3::new(0.0, 0.0, 17.0)));
        // Generous: debug builds on a busy CI machine. This catches accidental O(n²), not tuning.
        assert!(started.elapsed().as_secs() < 20, "took {:?}", started.elapsed());
    }
}
