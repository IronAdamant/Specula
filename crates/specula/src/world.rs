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
//! - **Animation.** Model entities with a rig may carry an [`AnimSpec`] (`anim`): which clip of
//!   their anim set plays, how fast, from when on the engine clock, crossfading from the previous
//!   clip. The pose is a pure function of the render time (see [`AnimSpec`]), so a capture at a
//!   given time always shows the same pose. Saved, hashed and journaled like any component.
//! - **Models.** [`Shape::Model`] entities name a recipe file ([`ModelSpec`]); the world asks its
//!   [`ModelSource`] (the engine's model library) to build it on spawn and on change, so broken
//!   recipes are refused up front, and keeps the built model's handle and bounds on the entity
//!   (derived: not saved or hashed).
//! - **Presentation.** `visible: false` hides an entity and its subtree (not drawn, not a sight
//!   subject, not in [`World::bounds`]); [`Layer::Ui`] marks a game's interface, which captures
//!   leave out. **Transient** entities ([`Entity::transient`]) are game presentation derived from
//!   the game's own state: they take ids from [`TRANSIENT_ID_BASE`] up, are never saved,
//!   journaled or hashed, and never move the revision (so they never trigger an autosave). They
//!   are recorded while a command runs, so a rejected command still rolls them back, and nothing
//!   persistent may sit below one.
//!
//! Game code running in-process calls [`World::spawn`], [`World::update`], [`World::delete`],
//! [`World::get`] and [`World::query`] directly; the MCP tools are thin parsers over the same calls.

use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::Json;
use crate::two_d::{TileEdits, TwoD};
use specula_gpu::{Environment, Material, Surface};
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
    if (s.shape == Shape::Model) != s.model.is_some() {
        return Err(invalid("recipe", "'recipe' is required for shape 'model' and only allowed for it".into()));
    }
    match (&s.two_d, s.shape.is_2d()) {
        (Some(t), true) if t.key() == s.shape.name() => {}
        (None, false) => {}
        _ => {
            let name = s.shape.name();
            return Err(invalid(
                if s.shape.is_2d() { name } else { s.two_d.as_ref().map_or(name, TwoD::key) },
                format!("shape '{name}' needs its '{name}' component, and only sprite, tilemap and shape2d take one"),
            ));
        }
    }
    if let Some(m) = &s.model {
        m.check()?;
        if s.look.surface.seed != 0.0 {
            return Err(invalid("seed", "a model's 'seed' is its model seed (ModelSpec::seed), not a surface seed".into()));
        }
    }
    check_tags(&s.tags)?;
    if s.tags.len() > MAX_TAGS {
        return Err(invalid("tags", format!("'tags': at most {MAX_TAGS}")));
    }
    if !matches!(s.props, Json::Null | Json::Obj(_)) {
        return Err(invalid("props", "'props' must be a JSON object".into()));
    }
    s.collider.as_ref().map_or(Ok(()), Collider::validate)?;
    if let Some(a) = &s.anim {
        if s.shape != Shape::Model {
            return Err(invalid("anim", "'anim' is only for model entities (shape 'model' with a rig)".into()));
        }
        a.check()?;
    }
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
    if let Some(r) = &u.recipe {
        validate_recipe_path(r)?;
    }
    if let Some(p) = &u.params {
        ModelSpec { recipe: "x.json".into(), params: p.clone(), seed: None }.check()?;
    }
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
    if let Some(Some(a)) = &u.anim {
        a.check()?;
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
    let effects = [
        p.shadow.map(|e| e.check()),
        p.bloom.map(|e| e.check()),
        p.sky.map(|e| e.check()),
        p.fog.map(|e| e.check()),
        p.grading.map(|e| e.check()),
        p.ssao.map(|e| e.check()),
    ];
    for result in effects.into_iter().flatten() {
        result.map_err(|(field, message)| invalid(&field, message))?;
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
    /// A procedural model built from a recipe file (see [`ModelSpec`]).
    Model,
    /// A textured quad in the local XY plane, or a billboard (see [`crate::two_d`]).
    Sprite,
    /// Layers of tiles on a grid in the local XY plane (see [`crate::two_d`]).
    Tilemap,
    /// Filled and stroked vector paths in the local XY plane (see [`crate::two_d`]).
    Shape2d,
}

impl Shape {
    pub const ALL: [Shape; 9] = [
        Shape::Group,
        Shape::Cube,
        Shape::Quad,
        Shape::Sphere,
        Shape::Lathe,
        Shape::Model,
        Shape::Sprite,
        Shape::Tilemap,
        Shape::Shape2d,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Group => "group",
            Shape::Cube => "cube",
            Shape::Quad => "quad",
            Shape::Sphere => "sphere",
            Shape::Lathe => "lathe",
            Shape::Model => "model",
            Shape::Sprite => "sprite",
            Shape::Tilemap => "tilemap",
            Shape::Shape2d => "shape2d",
        }
    }

    /// Drawn by the 2D renderer from a [`TwoD`] component.
    pub fn is_2d(self) -> bool {
        matches!(self, Shape::Sprite | Shape::Tilemap | Shape::Shape2d)
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

/// Most recipe parameters a model entity may set.
pub const MAX_MODEL_PARAMS: usize = 32;
/// Largest model seed (seeds share the `seed` argument with the lava lamp's, so they stay exact
/// in an `f32`).
pub const MAX_MODEL_SEED: u64 = 1_000_000;
/// Longest recipe path, in bytes.
pub const MAX_RECIPE_PATH: usize = 256;

/// Which recipe a [`Shape::Model`] entity is built from, with what parameters and seed. The engine
/// builds it (see the `models` module): same recipe file content, parameters and seed ⇒ the same
/// model, shared by every entity that uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelSpec {
    /// The recipe file, relative to the engine's assets root (`--assets`), e.g.
    /// `"recipes/warding_stone.json"`. Forward slashes; no `..`, no absolute paths.
    pub recipe: String,
    /// Recipe parameter values by name, sorted by name (`[("tier", 2.0)]`). Unset parameters take
    /// the recipe's defaults.
    pub params: Vec<(String, f64)>,
    /// Variant seed (overrides the recipe's own seed), 0..=[`MAX_MODEL_SEED`].
    pub seed: Option<u64>,
}

impl ModelSpec {
    pub fn new(recipe: impl Into<String>) -> ModelSpec {
        ModelSpec { recipe: recipe.into(), params: Vec::new(), seed: None }
    }

    /// Sets parameter `name` (kept sorted).
    pub fn with_param(mut self, name: &str, value: f64) -> ModelSpec {
        self.set_param(name, value);
        self
    }

    pub fn with_seed(mut self, seed: u64) -> ModelSpec {
        self.seed = Some(seed);
        self
    }

    pub fn set_param(&mut self, name: &str, value: f64) {
        match self.params.binary_search_by(|(k, _)| k.as_str().cmp(name)) {
            Ok(i) => self.params[i].1 = value,
            Err(i) => self.params.insert(i, (name.to_owned(), value)),
        }
    }

    /// A string naming exactly this spec (exact bits): the model library's key for it.
    pub fn key(&self) -> String {
        let mut k = self.recipe.clone();
        for (name, v) in &self.params {
            k.push_str(&format!("|{name}={:016x}", v.to_bits()));
        }
        if let Some(s) = self.seed {
            k.push_str(&format!("|#{s}"));
        }
        k
    }

    pub fn params_json(&self) -> Json {
        Json::obj(self.params.iter().map(|(k, v)| (k.clone(), Json::from(*v))))
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("recipe", Json::from(self.recipe.as_str()))];
        if !self.params.is_empty() {
            f.push(("params", self.params_json()));
        }
        if let Some(s) = self.seed {
            f.push(("seed", Json::from(s)));
        }
        Json::obj(f)
    }

    /// The recipe path is safe, parameters are named, finite and sorted, the seed in range.
    pub fn check(&self) -> Result<(), WorldError> {
        validate_recipe_path(&self.recipe)?;
        if self.params.len() > MAX_MODEL_PARAMS {
            return Err(invalid("params", format!("'params': at most {MAX_MODEL_PARAMS} parameters")));
        }
        for (i, (name, v)) in self.params.iter().enumerate() {
            let named = !name.is_empty()
                && name.len() <= 64
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
            if !named {
                return Err(invalid("params", format!("'params': '{name}' is not a parameter name (A-Z a-z 0-9 _)")));
            }
            if !v.is_finite() || v.abs() > 1.0e9 {
                return Err(invalid("params", format!("'params': '{name}' must be a finite number within ±1e9")));
            }
            if i > 0 && self.params[i - 1].0 >= *name {
                return Err(invalid("params", "'params' must be sorted by name, each name once".into()));
            }
        }
        match self.seed {
            Some(s) if s > MAX_MODEL_SEED => Err(invalid("seed", format!("'seed' of a model must be an integer 0..{MAX_MODEL_SEED}"))),
            _ => Ok(()),
        }
    }
}

/// A model source relative to the assets root: 1..256 bytes of `A-Z a-z 0-9 _ - . /`, segments
/// separated by single `/`, none empty or starting or ending with `.`, ending in `.json` (a
/// recipe) or `.glb` / `.gltf` (an imported glTF asset, see [`is_gltf_path`]).
pub fn validate_recipe_path(path: &str) -> Result<(), WorldError> {
    let ok = !path.is_empty()
        && path.len() <= MAX_RECIPE_PATH
        && path.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/'))
        // No empty, dot-led or dot-ended segments: covers `.` and `..`, hidden files, and the
        // trailing dots Windows strips (which would let two keys name one file).
        && path.split('/').all(|seg| !seg.is_empty() && !seg.starts_with('.') && !seg.ends_with('.'))
        && (path.ends_with(".json") || is_gltf_path(path));
    if ok {
        Ok(())
    } else {
        Err(invalid(
            "recipe",
            format!(
                "'recipe' must be a .json recipe or a .glb/.gltf asset relative to the assets root, like \
                 \"recipes/warding_stone.json\" (A-Z a-z 0-9 _ - . /, no '..', at most {MAX_RECIPE_PATH} bytes); got {path:?}"
            ),
        ))
    }
}

/// A model source that is a glTF asset (`.glb` or `.gltf`), imported instead of built: it takes
/// no `params` and no `seed`.
pub fn is_gltf_path(path: &str) -> bool {
    path.ends_with(".glb") || path.ends_with(".gltf")
}

/// Most bytes in an animation clip name.
pub const MAX_CLIP_NAME: usize = 128;
/// Playback speeds (clip seconds per clock second).
pub const ANIM_SPEED: RangeInclusive<f32> = 0.0..=8.0;
/// Clip time offsets (seconds).
pub const ANIM_OFFSET: RangeInclusive<f32> = -1.0e6..=1.0e6;
/// Crossfade durations (seconds).
pub const ANIM_CROSSFADE: RangeInclusive<f32> = 0.0..=60.0;
/// Clip start times (world time seconds, see [`World::world_time`]).
pub const ANIM_START: RangeInclusive<f64> = -1.0e9..=1.0e9;

/// The animation a model entity plays: one clip of its model's anim set (a recipe's
/// `anims/<recipe name>.json`, or a glTF asset's own animations).
///
/// The pose is a pure function of the render time `T` (world time, [`World::world_time`]; a
/// capture's `time_s`): clip time = `(T − start) × speed + time_offset`, never below 0, wrapped or held by
/// the clip's loop mode (or by `looping` when set). While `T` is less than `crossfade` seconds
/// past `start`, the pose blends (smoothstep) from `from`, the animation it replaced, still
/// playing on its own terms. So any frame can be rendered at any time with the same result, and
/// game entities animate without per-tick state.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimSpec {
    pub clip: String,
    /// 0..=8 (1 = as authored).
    pub speed: f32,
    /// `Some(true)` loops a play-once clip; `Some(false)` holds a looping clip's last pose;
    /// `None` keeps the clip's own loop mode.
    pub looping: Option<bool>,
    /// Seconds of clip time added (phase: two entities on one clip out of step).
    pub time_offset: f32,
    /// World time ([`World::world_time`]) the clip starts at. `None` in a request: 0 at spawn
    /// (phase-locked to world time, so a filmstrip at `time_s` 0, 0.25, … shows the clip from its
    /// start) and the current world time on update (it starts at once). Always set on an entity;
    /// never rewritten: world time itself continues across saves and loads, so a loaded world
    /// hashes and renders (at a given `time_s`) exactly as it did when saved, and every
    /// animation resumes in phase.
    pub start: Option<f64>,
    /// Seconds to blend from the previous animation (0 = cut).
    pub crossfade: f32,
    /// The animation being faded out (set by the world when `crossfade` > 0; its own `from` is
    /// dropped, so an interrupted fade restarts from the interrupted clip).
    pub from: Option<Box<AnimSpec>>,
}

impl AnimSpec {
    /// `clip` at speed 1, its own loop mode, no offset or fade.
    pub fn new(clip: impl Into<String>) -> AnimSpec {
        AnimSpec { clip: clip.into(), speed: 1.0, looping: None, time_offset: 0.0, start: None, crossfade: 0.0, from: None }
    }

    /// The clip time (seconds, before wrapping) at render time `t`.
    pub fn clip_time(&self, t: f64) -> f64 {
        ((t - self.start.unwrap_or(0.0)) * self.speed as f64 + self.time_offset as f64).max(0.0)
    }

    /// The blend weight of this clip over `from` at render time `t` (1 = fully this clip).
    pub fn fade_weight(&self, t: f64) -> f32 {
        if self.from.is_none() || self.crossfade <= 0.0 {
            return 1.0;
        }
        let x = ((t - self.start.unwrap_or(0.0)) / self.crossfade as f64).clamp(0.0, 1.0) as f32;
        x * x * (3.0 - 2.0 * x)
    }

    pub fn check(&self) -> Result<(), WorldError> {
        if self.clip.is_empty() || self.clip.len() > MAX_CLIP_NAME || self.clip.chars().any(char::is_control) {
            return Err(invalid("anim.clip", format!("'anim.clip' must be a clip name of 1..={MAX_CLIP_NAME} bytes")));
        }
        check_num("anim.speed", self.speed, &ANIM_SPEED)?;
        check_num("anim.time_offset", self.time_offset, &ANIM_OFFSET)?;
        check_num("anim.crossfade", self.crossfade, &ANIM_CROSSFADE)?;
        if let Some(s) = self.start {
            if !s.is_finite() || !ANIM_START.contains(&s) {
                return Err(invalid("anim.start", format!("'anim.start' must be clock seconds in {ANIM_START:?}")));
            }
        }
        if let Some(f) = &self.from {
            if f.from.is_some() {
                return Err(invalid("anim.from", "'anim.from' can't fade from another animation itself".into()));
            }
            f.check()?;
        }
        Ok(())
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("clip", Json::from(self.clip.as_str())), ("speed", Json::from(self.speed))];
        if let Some(l) = self.looping {
            f.push(("loop", Json::from(l)));
        }
        if self.time_offset != 0.0 {
            f.push(("time_offset", Json::from(self.time_offset)));
        }
        if let Some(s) = self.start {
            f.push(("start", Json::from(s)));
        }
        if self.crossfade > 0.0 {
            f.push(("crossfade", Json::from(self.crossfade)));
        }
        if let Some(from) = &self.from {
            f.push(("from", from.to_json()));
        }
        Json::obj(f)
    }

    /// From `{clip, speed?, loop?, time_offset?, start? (seconds or "now"), crossfade?, from?}`
    /// or a bare clip name. `"start": "now"` leaves `start` unset (resolved by the world).
    pub fn from_json(v: &Json) -> Result<AnimSpec, String> {
        if let Some(clip) = v.as_str() {
            return Ok(AnimSpec::new(clip));
        }
        let Some(obj) = v.as_object() else {
            return Err("'anim' must be a clip name, {clip, speed?, loop?, time_offset?, start?, crossfade?} or false".into());
        };
        for (k, _) in obj {
            if !matches!(k.as_str(), "clip" | "speed" | "loop" | "time_offset" | "start" | "crossfade" | "from") {
                return Err(format!("'anim.{k}' is not an anim field (clip, speed, loop, time_offset, start, crossfade)"));
            }
        }
        let num = |k: &str| -> Result<Option<f64>, String> {
            match v.get(k) {
                None | Some(Json::Null) => Ok(None),
                Some(n) => n.as_f64().map(Some).ok_or_else(|| format!("'anim.{k}' must be a number")),
            }
        };
        let clip = v.get("clip").and_then(Json::as_str).ok_or("'anim.clip' (a clip name) is required")?;
        let mut a = AnimSpec::new(clip);
        a.speed = num("speed")?.map_or(1.0, |x| x as f32);
        a.looping = match v.get("loop") {
            None | Some(Json::Null) => None,
            Some(Json::Bool(b)) => Some(*b),
            Some(_) => return Err("'anim.loop' must be true or false".into()),
        };
        a.time_offset = num("time_offset")?.unwrap_or(0.0) as f32;
        a.start = match v.get("start") {
            Some(Json::Str(s)) if s == "now" => None,
            _ => num("start")?,
        };
        a.crossfade = num("crossfade")?.unwrap_or(0.0) as f32;
        if let Some(f) = v.get("from") {
            a.from = Some(Box::new(AnimSpec::from_json(f).map_err(|e| e.replacen("'anim.", "'anim.from.", 1))?));
        }
        Ok(a)
    }

    fn hash_into(&self, h: &mut StableHasher) {
        h.write_str(&self.clip);
        h.write_f32(self.speed);
        h.write_u8(match self.looping {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        h.write_f32(self.time_offset);
        h.write_f64(self.start.unwrap_or(0.0));
        h.write_f32(self.crossfade);
        match &self.from {
            None => h.write_u8(0),
            Some(f) => {
                h.write_u8(1);
                f.hash_into(h);
            }
        }
    }
}

/// Where a model entity's build stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelState {
    /// Being built (off the engine thread); drawn once it is ready.
    #[default]
    Building,
    Ready,
    /// The build failed (budget, geometry...): nothing to draw until the recipe is fixed.
    Failed,
}

impl ModelState {
    pub fn name(self) -> &'static str {
        match self {
            ModelState::Building => "building",
            ModelState::Ready => "ready",
            ModelState::Failed => "failed",
        }
    }
}

/// What the engine knows about a model entity's built model: its slot in the model library, its
/// state and (once built) its object-space bounds. Derived data: not saved, not hashed, refreshed
/// when builds finish and when recipes change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedModel {
    pub handle: u32,
    pub state: ModelState,
    /// `None` until a build is ready.
    pub bounds: Option<(Vec3, Vec3)>,
}

/// Builds models for the world (the engine's model library). The world asks it whenever a model
/// entity is spawned or its recipe, parameters or seed change. It answers at once: what can be
/// checked without building (the path, the file, the style, the recipe's structure, parameter
/// names and ranges) is refused with a structured error; the build itself may still be running
/// (state [`ModelState::Building`]).
pub trait ModelSource: Send + Sync {
    fn resolve(&self, spec: &ModelSpec) -> Result<ResolvedModel, WorldError>;
}

/// The world's model source, if any (`Debug` for the world's derive).
#[derive(Clone, Default)]
struct SourceSlot(Option<Arc<dyn ModelSource>>);

impl fmt::Debug for SourceSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() { "ModelSource" } else { "None" })
    }
}

/// Which pass an entity belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// The scene: in the window and in every capture.
    #[default]
    World,
    /// A game's interface in the scene (cursors, range rings, selection marks): drawn in the
    /// window, left out of `sight_frame` captures (unless `include_ui`), of the world's bounds
    /// and of auto-framing. Children of a `ui` entity are `ui` too.
    Ui,
}

impl Layer {
    pub const ALL: [Layer; 2] = [Layer::World, Layer::Ui];

    pub fn name(self) -> &'static str {
        match self {
            Layer::World => "world",
            Layer::Ui => "ui",
        }
    }

    pub fn parse(s: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|l| l.name() == s)
    }
}

/// Transient entities take ids from here up, so they never move the persistent id counter
/// (which world files and state hashes include). Still exact in a JSON number.
pub const TRANSIENT_ID_BASE: u64 = 1 << 40;

/// The largest persistent id counter a world may have (world files with more are refused): the
/// next persistent spawn would reach the transient range.
pub const MAX_NEXT_ID: u64 = TRANSIENT_ID_BASE - 2;

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
    /// The grading LUT, a PNG strip under the assets root (none by default). The engine loads it
    /// and sets `light.grading.lut`; the world keeps only the path.
    pub grading_lut: crate::effects::AssetPath,
}

impl Default for Env {
    fn default() -> Self {
        Env { light: Environment::default(), background: [0.10, 0.11, 0.13], grading_lut: crate::effects::AssetPath::NONE }
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
            ("shadow", crate::effects::ShadowPatch::to_json(&l.shadow, false)),
            ("bloom", crate::effects::BloomPatch::to_json(&l.bloom, false)),
            ("fxaa", Json::from(l.fxaa)),
            ("sky", crate::effects::SkyPatch::to_json(&l.sky, false)),
            ("fog", crate::effects::FogPatch::to_json(&l.fog, false)),
            ("grading", crate::effects::GradingPatch::to_json(&l.grading, &self.grading_lut, false)),
            ("ssao", crate::effects::SsaoPatch::to_json(&l.ssao, false)),
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
            ("shadow", crate::effects::ShadowPatch::to_json(&l.shadow, true)),
            ("bloom", crate::effects::BloomPatch::to_json(&l.bloom, true)),
            ("fxaa", Json::from(l.fxaa)),
            ("sky", crate::effects::SkyPatch::to_json(&l.sky, true)),
            ("fog", crate::effects::FogPatch::to_json(&l.fog, true)),
            ("grading", crate::effects::GradingPatch::to_json(&l.grading, &self.grading_lut, true)),
            ("ssao", crate::effects::SsaoPatch::to_json(&l.ssao, true)),
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
    pub shadow: Option<crate::effects::ShadowPatch>,
    pub bloom: Option<crate::effects::BloomPatch>,
    pub fxaa: Option<bool>,
    pub sky: Option<crate::effects::SkyPatch>,
    pub fog: Option<crate::effects::FogPatch>,
    pub grading: Option<crate::effects::GradingPatch>,
    pub ssao: Option<crate::effects::SsaoPatch>,
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
        let l = &mut env.light;
        if let Some(p) = &self.shadow {
            p.apply(&mut l.shadow);
        }
        if let Some(p) = &self.bloom {
            p.apply(&mut l.bloom);
        }
        if let Some(v) = self.fxaa {
            l.fxaa = v;
        }
        if let Some(p) = &self.sky {
            p.apply(&mut l.sky);
        }
        if let Some(p) = &self.fog {
            p.apply(&mut l.fog);
        }
        if let Some(p) = &self.grading {
            p.apply(&mut l.grading, &mut env.grading_lut);
        }
        if let Some(p) = &self.ssao {
            p.apply(&mut env.light.ssao);
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
    /// Present exactly when `shape` is [`Shape::Model`].
    pub model: Option<ModelSpec>,
    /// Present exactly when `shape` is a 2D shape ([`Shape::is_2d`]).
    pub two_d: Option<TwoD>,
    /// Model entities only: the animation playing (its `start` always set).
    pub anim: Option<AnimSpec>,
    /// `false` hides the entity and everything below it: not drawn, not a sight subject, not in
    /// the world's bounds. Still saved, queried, journaled and claimed like any field.
    pub visible: bool,
    /// See [`Layer`]; children of a `ui` entity are `ui` too.
    pub layer: Layer,
    /// Set at spawn (inherited from a transient parent), never changed: game presentation derived
    /// from the game's own state. Not saved, not journaled, not in the state hash, never marks
    /// the world unsaved; drawn, queried and seen like any entity (see [`World`]).
    pub transient: bool,
    /// 1 when spawned, +1 on every change.
    pub version: u64,
    /// Agent name (shared: see [`World`]'s interning).
    pub created_by: Arc<str>,
    /// Agent name of the last change.
    pub modified_by: Arc<str>,
    /// Cached world transform: parent's world × local. Kept current by [`World`].
    world: Mat4,
    /// Cached: visible itself and every ancestor visible.
    shown: bool,
    /// Cached: on the `ui` layer itself or below a `ui` ancestor.
    in_ui: bool,
    /// The engine's built model (model entities, when a model source resolved it).
    resolved: Option<ResolvedModel>,
    /// Cached [`Entity::content_hash`]; cleared by every change to what it covers.
    hash: HashCache,
}

/// [`Entity::content_hash`]'s cache. Clones start empty, so a copy (a journal snapshot, a
/// replayed or restored entity) is always hashed afresh from its own fields.
#[derive(Debug, Default)]
struct HashCache(Cell<Option<u64>>);

impl Clone for HashCache {
    fn clone(&self) -> Self {
        HashCache::default()
    }
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

    /// Draws with a mesh of its own making (a lathe's profile, a model's build) that the engine
    /// uploads and frees.
    pub fn owns_mesh(&self) -> bool {
        matches!(self.shape, Shape::Lathe | Shape::Model)
    }

    /// Drawn: visible, and every ancestor visible.
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// On the `ui` layer, itself or through an ancestor.
    pub fn in_ui(&self) -> bool {
        self.in_ui
    }

    /// The engine's built model for a model entity (`None` until a model source resolved it).
    pub fn resolved_model(&self) -> Option<ResolvedModel> {
        self.resolved
    }

    /// A model entity's build: `building`, `ready`, `failed`, or `unresolved` (no model source,
    /// or its recipe was refused when the world was loaded). Empty for other shapes.
    pub fn model_state(&self) -> &'static str {
        match (&self.model, self.resolved) {
            (None, _) => "",
            (Some(_), None) => "unresolved",
            (Some(_), Some(r)) => r.state.name(),
        }
    }

    /// Object-space bounding box; `None` for groups, which have no geometry (and for a model the
    /// engine could not build).
    pub fn local_bounds(&self) -> Option<(Vec3, Vec3)> {
        match (self.shape, &self.profile) {
            (Shape::Group, _) => None,
            (Shape::Model, _) => self.resolved.and_then(|r| r.bounds),
            (Shape::Lathe, Some(p)) => Some(p.bounds()),
            (Shape::Quad, _) => Some((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.0, 0.5))),
            (Shape::Sprite | Shape::Tilemap | Shape::Shape2d, _) => self.two_d.as_ref().map(TwoD::bounds),
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
        if let Some(t) = &self.two_d {
            fields.push((t.key(), t.listing_json()));
        }
        if let Some(m) = &self.model {
            fields.push(("recipe", Json::from(m.recipe.as_str())));
            fields.push(("params", m.params_json()));
            fields.push(("seed", Json::from(m.seed)));
            let state = self.model_state();
            if state != "ready" {
                fields.push(("model_state", Json::from(state)));
            }
        }
        if let Some(a) = &self.anim {
            fields.push(("anim", a.to_json()));
        }
        if !self.visible {
            fields.push(("visible", Json::from(false)));
        }
        if self.layer != Layer::World {
            fields.push(("layer", Json::from(self.layer.name())));
        }
        if self.transient {
            fields.push(("transient", Json::from(true)));
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
    /// A hash of everything [`World::state_hash`] covers about this entity, computed once per
    /// change: hashing the world each tick only re-hashes what changed (foundation F3).
    pub fn content_hash(&self) -> u64 {
        if let Some(h) = self.hash.0.get() {
            return h;
        }
        let mut h = StableHasher::with_seed(0x456e_7469_7479_0001);
        self.hash_into(&mut h);
        let v = h.finish();
        self.hash.0.set(Some(v));
        v
    }

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
        // Fields added after the first hash format: written only when not default, so worlds
        // without them keep their hashes.
        if let Some(m) = &self.model {
            h.write_str("model");
            h.write_str(&m.key());
        }
        if !self.visible {
            h.write_str("hidden");
        }
        if self.layer != Layer::World {
            h.write_str(self.layer.name());
        }
        if let Some(t) = &self.two_d {
            t.hash_into(h);
        }
        if let Some(a) = &self.anim {
            h.write_str("anim");
            a.hash_into(h);
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
    /// Required for [`Shape::Model`], refused for other shapes.
    pub model: Option<ModelSpec>,
    /// Required for the 2D shapes (of the matching kind), refused for others.
    pub two_d: Option<TwoD>,
    /// Model entities only (see [`AnimSpec`]; an unset `start` becomes 0).
    pub anim: Option<AnimSpec>,
    pub visible: bool,
    pub layer: Layer,
    /// See [`Entity::transient`]. A child of a transient parent is transient whatever this says.
    pub transient: bool,
}

impl SpawnSpec {
    /// A light-grey matte top-level `shape` at the origin (models: white, so they show as
    /// authored).
    pub fn new(shape: Shape) -> SpawnSpec {
        SpawnSpec {
            shape,
            name: None,
            parent: None,
            position: Vec3::ZERO,
            rotation_deg: Vec3::ZERO,
            scale: Vec3::ONE,
            color: if shape == Shape::Model { [1.0; 3] } else { [0.8, 0.8, 0.8] },
            look: Look::default(),
            light: None,
            profile: None,
            tags: BTreeSet::new(),
            props: Json::Null,
            collider: None,
            body: None,
            model: None,
            two_d: None,
            anim: None,
            visible: true,
            layer: Layer::World,
            transient: false,
        }
    }

    /// A model entity built from `spec` (white: as authored).
    pub fn model(spec: ModelSpec) -> SpawnSpec {
        SpawnSpec { model: Some(spec), ..SpawnSpec::new(Shape::Model) }
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
    /// Model entities only: a new recipe path.
    pub recipe: Option<String>,
    /// Model entities only: replaces every parameter (sorted by name).
    pub params: Option<Vec<(String, f64)>>,
    /// Model entities only: `Some(None)` stops animating (the bind pose); a new [`AnimSpec`]
    /// replaces the whole component (unset `start` = the world's clock now; `crossfade` > 0
    /// blends from the animation it replaces).
    pub anim: Option<Option<AnimSpec>>,
    pub visible: Option<bool>,
    pub layer: Option<Layer>,
    /// 2D entities only: replaces the component (of the same kind).
    pub two_d: Option<TwoD>,
    /// Tile maps only: cells to write (`tilemap_set`).
    pub tiles: Option<TileEdits>,
    /// Reject the change unless the entity is still at this version.
    pub expect_version: Option<u64>,
}

impl UpdateSpec {
    /// Only the transform and colour change (what a game moves every tick).
    pub fn pose_only(&self) -> bool {
        self.name.is_none()
            && self.parent.is_none()
            && self.look.is_empty()
            && self.light.is_none()
            && self.profile.is_none()
            && self.tags.is_none()
            && self.add_tags.is_empty()
            && self.remove_tags.is_empty()
            && self.props.is_none()
            && self.collider.is_none()
            && self.body.is_none()
            && self.recipe.is_none()
            && self.params.is_none()
            && self.anim.is_none()
            && self.visible.is_none()
            && self.layer.is_none()
            && self.two_d.is_none()
            && self.tiles.is_none()
    }

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
            && self.recipe.is_none()
            && self.params.is_none()
            && self.anim.is_none()
            && self.visible.is_none()
            && self.layer.is_none()
            && self.two_d.is_none()
            && self.tiles.is_none()
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

/// What the world records about its own changes, for the journal (see [`World::set_recording`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Recording {
    /// Nothing (the default: world files, tests and replays pay nothing).
    #[default]
    Off,
    /// Every change with the state before and after: enough to undo it and to replay it.
    Full,
    /// Only *which* entities changed; [`World::take_changes`] then snapshots each once, as it is
    /// now: one after-only change per entity (replayable, not undoable). For high-rate game
    /// updates (foundation F3): a change costs a map insert, not a copy of the entity.
    Snapshot,
}

/// One recorded change to the world. Entities are whole copies (exact bits), so replaying the
/// `after` states in order rebuilds the world bit for bit, and the `before` states undo it.
#[derive(Clone, Debug)]
pub enum Change {
    Spawn { path: String, after: Box<Entity> },
    Set {
        path: String,
        /// The path before the change, when it was renamed or moved to another parent.
        prev_path: Option<String>,
        /// `None` when recorded with [`Recording::Snapshot`].
        before: Option<Box<Entity>>,
        after: Box<Entity>,
    },
    /// One per removed entity: deleting a subtree records its entities children first.
    Delete { path: String, before: Box<Entity> },
    /// Boxed: environments are large (lighting, fog, shadows) and environment changes rare.
    Env { before: Box<Env>, after: Box<Env> },
    /// An entity whose transform or colour alone changed, recorded by [`Recording::Snapshot`]
    /// without copying the entity (replayable, not undoable).
    Pose { path: String, pose: Pose },
    /// An agent's named camera set (`after`) or deleted (`after: None`); `before` is what it was.
    /// Not world state for the hash (see [`crate::cameras`]). `path` is
    /// `@camera/<owner>/<name>`.
    Camera {
        path: String,
        owner: String,
        name: String,
        before: Option<Box<crate::cameras::NamedCamera>>,
        after: Option<Box<crate::cameras::NamedCamera>>,
    },
}

/// The journal path of a camera change: `@camera/<owner>/<name>` (never an entity path).
pub fn camera_path(owner: &str, name: &str) -> String {
    format!("@camera/{owner}/{name}")
}

/// Ids at and above this name cameras in the journal (entity ids never get this high): undo
/// conflicts and history filters treat each camera as its own thing.
pub const CAMERA_ID_BASE: u64 = 1 << 62;

/// The fields a game moves every tick: transform and colour (plus the version they reached).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub id: u64,
    pub version: u64,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
    pub color: [f32; 3],
}

impl Pose {
    pub fn of(e: &Entity) -> Pose {
        Pose { id: e.id, version: e.version, position: e.position, rotation_deg: e.rotation_deg, scale: e.scale, color: e.color }
    }

    /// Puts the pose on `e` (a copy of the same entity).
    pub fn apply_to(&self, e: &mut Entity) {
        e.position = self.position;
        e.rotation_deg = self.rotation_deg;
        e.scale = self.scale;
        e.color = self.color;
        e.version = self.version;
        e.hash.0.set(None);
    }
}

/// What [`Recording::Snapshot`] knows about an entity changed since the last take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dirty {
    /// Spawned since (didn't exist before).
    Spawned,
    /// Only its transform or colour changed.
    Pose,
    /// Anything else changed.
    Changed,
}

impl Change {
    /// A change to a transient entity (never journaled).
    pub fn is_transient(&self) -> bool {
        match self {
            Change::Spawn { after, .. } | Change::Set { after, .. } => after.transient,
            Change::Delete { before, .. } => before.transient,
            Change::Env { .. } | Change::Pose { .. } | Change::Camera { .. } => false,
        }
    }

    /// The entity's id; 0 for the environment; for a camera, `CAMERA_ID_BASE` plus a hash of its
    /// path.
    pub fn id(&self) -> u64 {
        match self {
            Change::Spawn { after, .. } | Change::Set { after, .. } => after.id,
            Change::Delete { before, .. } => before.id,
            Change::Env { .. } => ROOT,
            Change::Pose { pose, .. } => pose.id,
            Change::Camera { path, .. } => CAMERA_ID_BASE | (specula_core::hash::hash_str(path) >> 2),
        }
    }

    pub fn op(&self) -> &'static str {
        match self {
            Change::Spawn { .. } => "spawn",
            Change::Set { .. } | Change::Pose { .. } => "set",
            Change::Delete { .. } => "delete",
            Change::Env { .. } => "env",
            Change::Camera { after: Some(_), .. } => "camera_set",
            Change::Camera { after: None, .. } => "camera_delete",
        }
    }

    /// Where it happened: the entity's path afterwards (before, for a delete); `/` for the
    /// environment.
    pub fn path(&self) -> &str {
        match self {
            Change::Spawn { path, .. }
            | Change::Set { path, .. }
            | Change::Delete { path, .. }
            | Change::Pose { path, .. }
            | Change::Camera { path, .. } => path,
            Change::Env { .. } => "/",
        }
    }

    /// The entity's version afterwards (`None` for deletes and the environment).
    pub fn version(&self) -> Option<u64> {
        match self {
            Change::Spawn { after, .. } | Change::Set { after, .. } => Some(after.version),
            Change::Pose { pose, .. } => Some(pose.version),
            Change::Camera { after: Some(c), .. } => Some(c.version),
            _ => None,
        }
    }

    /// Heap and inline size, for the journal's memory bound: every allocation counted with the
    /// allocator's overhead, B-tree nodes at their real size.
    pub fn approx_bytes(&self) -> usize {
        let entity = |e: &Entity| {
            std::mem::size_of::<Entity>()
                + ALLOC
                + heap(&e.name)
                + tags_bytes(&e.tags)
                + e.profile.as_ref().map_or(0, |p| ALLOC + p.points.len() * 8)
                + json_heap(&e.props)
                + e.collider.as_ref().map_or(0, |c| heap(&c.layer))
                + e.two_d.as_ref().map_or(0, TwoD::heap_bytes)
                + e.model.as_ref().map_or(0, |m| {
                    heap(&m.recipe) + ALLOC + m.params.iter().map(|(k, _)| std::mem::size_of::<(String, f64)>() + heap(k)).sum::<usize>()
                })
                + e.anim.as_ref().map_or(0, |a| heap(&a.clip) + a.from.as_ref().map_or(0, |f| ALLOC + std::mem::size_of::<AnimSpec>() + heap(&f.clip)))
        };
        std::mem::size_of::<Change>()
            + match self {
                Change::Spawn { path, after } => heap(path) + entity(after),
                Change::Set { path, prev_path, before, after } => {
                    heap(path) + prev_path.as_deref().map_or(0, heap) + before.as_deref().map_or(0, entity) + entity(after)
                }
                Change::Delete { path, before } => heap(path) + entity(before),
                Change::Pose { path, .. } => heap(path),
                Change::Env { .. } => 2 * (std::mem::size_of::<Env>() + ALLOC),
                Change::Camera { path, owner, name, before, after } => {
                    let cam = |c: &Option<Box<crate::cameras::NamedCamera>>| {
                        c.as_ref().map_or(0, |c| {
                            std::mem::size_of::<crate::cameras::NamedCamera>()
                                + ALLOC
                                + heap(&c.spec.name)
                                + c.spec.follow.as_ref().map_or(0, |f| heap(&f.path))
                        })
                    };
                    heap(path) + heap(owner) + heap(name) + cam(before) + cam(after)
                }
            }
    }
}

/// Bookkeeping per heap allocation (allocator header and rounding), roughly.
const ALLOC: usize = 16;
/// A B-tree node of `String` keys (11 keys + header), as `BTreeSet<String>` allocates them.
const BTREE_NODE: usize = 11 * std::mem::size_of::<String>() + 24 + ALLOC;

fn heap(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.len() + ALLOC
    }
}

fn tags_bytes(tags: &BTreeSet<String>) -> usize {
    if tags.is_empty() {
        0
    } else {
        tags.len().div_ceil(11) * BTREE_NODE + tags.iter().map(|t| heap(t)).sum::<usize>()
    }
}

/// Heap memory held by a JSON value (no allocation: it runs for every journaled change).
fn json_heap(v: &Json) -> usize {
    const NODE: usize = std::mem::size_of::<Json>();
    match v {
        Json::Obj(pairs) if !pairs.is_empty() => {
            ALLOC + pairs.iter().map(|(k, v)| std::mem::size_of::<String>() + NODE + heap(k) + json_heap(v)).sum::<usize>()
        }
        Json::Arr(items) if !items.is_empty() => ALLOC + items.iter().map(|v| NODE + json_heap(v)).sum::<usize>(),
        Json::Str(s) => heap(s),
        _ => 0,
    }
}

/// A point to roll the world back to (see `World::rollback_to`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    next_id: u64,
    revision: u64,
    changes: usize,
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
pub const FIELDS: [&str; 39] = [
    "id", "name", "path", "parent", "shape", "position", "rotation_deg", "scale", "world_position", "color",
    "material", "opacity", "emissive", "metallic", "roughness", "specular", "color2", "seed", "light", "profile",
    "tags", "props", "collider", "body", "version", "created_by", "modified_by", "children", "bounds", "recipe",
    "params", "visible", "layer", "transient", "model_state", "anim", "sprite", "tilemap", "shape2d",
];

/// Entity changes [`World::draw_changes_since`] remembers.
pub const DRAW_LOG: usize = 1024;

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
    /// What changes are recorded for the journal (off by default: world files, tests and replays
    /// pay nothing).
    recording: Recording,
    /// Changes recorded since the last [`World::take_changes`], in the order they happened.
    changes: Vec<Change>,
    /// [`Recording::Snapshot`]: entities changed since the last take → whether each existed
    /// before its first change.
    dirty: BTreeMap<u64, Dirty>,
    /// [`Recording::Snapshot`]: removed entities that existed before, children first.
    dirty_removed: Vec<(String, Entity)>,
    /// [`Recording::Snapshot`]: the environment before its first change.
    dirty_env: Option<Env>,
    /// Transient ids handed out so far (the next is `TRANSIENT_ID_BASE + transient_seq + 1`).
    transient_seq: u64,
    /// Transient entities alive.
    transient_count: usize,
    /// Moves on every change to a persistent entity (derived state included): what a renderer
    /// keys cached per-entity draw data on. Transient entities (game presentation, changing
    /// every tick) are left out: renderers read them live. Never hashed or saved.
    draw_epoch: u64,
    /// The latest changes as `(draw_epoch after it, entity id)`, oldest first, at most
    /// [`DRAW_LOG`] (see [`World::draw_changes_since`]).
    draw_log: VecDeque<(u64, u64)>,
    /// Changes up to this epoch are no longer in `draw_log` (it overflowed, or a change touched
    /// every entity).
    draw_floor: u64,
    /// Ids of the entities carrying a point light, so frames gather lights without visiting every
    /// entity (a 10k-entity world has a handful). Kept by `link`, `unlink` and `update`.
    lit: BTreeSet<u64>,
    /// Ids of the 2D entities (sprites, tilemaps, shapes), so the window's 2D pass doesn't visit
    /// every 3D entity. Kept by `link` and `unlink` (an entity's `two_d` comes with its shape).
    flat: BTreeSet<u64>,
    /// Bumped whenever an entity that may own a mesh (lathe, model) comes, goes or changes its
    /// mesh, so the engine re-syncs GPU meshes only then.
    mesh_epoch: u64,
    /// The engine clock (seconds) as of its last tick. World time = this + `time_offset`.
    clock_s: f64,
    /// World time minus the engine clock: 0 for a world made in this run; for a loaded world,
    /// the world time it was saved at minus the engine clock when loaded, so world time resumes
    /// where it stopped. Not state (never hashed): saved as the world time it gives.
    time_offset: f64,
    /// See [`World::model_drops`].
    model_drops: u64,
    /// Builds model entities' models (the engine's model library).
    models: SourceSlot,
    /// Agents' named sight cameras: agent name → camera name → camera (see [`crate::cameras`]).
    /// Saved with the world and journaled, but never hashed: they are tooling, not state.
    cameras: BTreeMap<String, BTreeMap<String, crate::cameras::NamedCamera>>,
    /// Bumped whenever an entity is linked, unlinked, renamed or moved to another parent: path
    /// lookups cached against it stay valid while it holds (follow cameras, every frame).
    layout_epoch: u64,
    /// The distinct vector shapes of the persistent entities, against their vertex budget.
    shapes: crate::two_d::ShapeBudget,
    /// The same for transient entities (game presentation), against a budget of their own.
    transient_shapes: crate::two_d::ShapeBudget,
    /// The two budgets (perf.toml `[budgets]`).
    shape_limits: crate::two_d::ShapeLimits,
}

/// The shape2d spec of an entity's 2D component, if it has one.
fn shape_spec(t: Option<&TwoD>) -> Option<&crate::two_d::Shape2dSpec> {
    match t {
        Some(TwoD::Shape2d(s)) => Some(s),
        _ => None,
    }
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

    /// Records a change in `id`'s top-level subtree (transient entities are never saved, so
    /// their changes touch nothing).
    fn touch(&mut self, id: u64) {
        if self.entities.get(&id).is_none_or(|e| e.transient) {
            return;
        }
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
    /// Every entity, transient ones included.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Entities that are saved and hashed (not transient).
    pub fn persistent_len(&self) -> usize {
        self.entities.len() - self.transient_count
    }

    pub fn transient_len(&self) -> usize {
        self.transient_count
    }

    /// See the field: changes whenever GPU meshes may need building or freeing.
    /// World time (seconds): the time animations live in ([`AnimSpec::start`]), what frames
    /// render at and what new animations start at. It is the engine clock for a world made in
    /// this run, and continues from where it was saved for a loaded one ([`World::adopt_clock`]).
    pub fn world_time(&self) -> f64 {
        self.clock_s + self.time_offset
    }

    /// The engine sets this as its clock ticks (and a test as it likes); world time follows.
    pub fn set_clock_s(&mut self, seconds: f64) {
        if seconds.is_finite() {
            self.clock_s = seconds.clamp(*ANIM_START.start(), *ANIM_START.end());
        }
    }

    /// Joins the engine clock reading `engine_s` without moving world time: the engine calls it
    /// on a loaded world, whose clock is the world time it was saved at.
    pub fn adopt_clock(&mut self, engine_s: f64) {
        if engine_s.is_finite() {
            let now = self.world_time();
            self.set_clock_s(engine_s);
            self.time_offset = now - self.clock_s;
        }
    }

    pub fn mesh_epoch(&self) -> u64 {
        self.mesh_epoch
    }

    /// Bumped whenever a model entity may have stopped using its model (removed, its spec
    /// changed, an old state put back, a batch rolled back): when the engine looks for builds
    /// nobody waits for any more.
    pub fn model_drops(&self) -> u64 {
        self.model_drops
    }

    /// The model library model entities are built with (`None`: model entities spawn
    /// unresolved, with no bounds, and draw nothing).
    pub fn model_source(&self) -> Option<&Arc<dyn ModelSource>> {
        self.models.0.as_ref()
    }

    /// Sets the model library and resolves every model entity with it. Returns the entities it
    /// could not build (they keep their spec and draw nothing until their recipe is fixed).
    pub fn set_model_source(&mut self, source: Option<Arc<dyn ModelSource>>) -> Vec<(u64, WorldError)> {
        self.models = SourceSlot(source);
        self.refresh_models()
    }

    /// Re-resolves every model entity (after recipes changed on disk): new bounds, same specs.
    /// Derived data only: nothing is recorded and the world isn't marked changed.
    pub fn refresh_models(&mut self) -> Vec<(u64, WorldError)> {
        self.mesh_epoch += 1;
        let Some(source) = self.models.0.clone() else {
            self.draw_reset();
            for e in self.entities.values_mut() {
                e.resolved = None;
            }
            return Vec::new();
        };
        let mut failed = Vec::new();
        self.draw_reset();
        for e in self.entities.values_mut() {
            let Some(spec) = &e.model else { continue };
            match source.resolve(spec) {
                Ok(r) => e.resolved = Some(r),
                Err(err) => {
                    // An old handle may name another spec by now.
                    e.resolved = None;
                    failed.push((e.id, err));
                }
            }
        }
        failed
    }

    /// Re-resolves only the model entities whose handle is in `handles` (their builds just
    /// finished or changed). Derived data only, like [`World::refresh_models`].
    pub fn refresh_model_handles(&mut self, handles: &std::collections::BTreeSet<u32>) {
        let Some(source) = self.models.0.clone() else { return };
        let mut touched = false;
        // Entities whose resolution changed, recorded for draw caches after the walk (a finished
        // build most frames changes nothing a cache holds).
        let mut changed = Vec::new();
        for e in self.entities.values_mut() {
            let (Some(spec), Some(r)) = (&e.model, e.resolved) else { continue };
            if handles.contains(&r.handle) {
                let now = source.resolve(spec).ok();
                if now != Some(r) {
                    changed.push(e.id);
                }
                e.resolved = now;
                touched = true;
            }
        }
        for id in changed {
            self.draw_touch(id);
        }
        if touched {
            self.mesh_epoch += 1;
        }
    }

    /// Resolves `spec` with the model source, if there is one.
    fn resolve_model(&self, spec: &ModelSpec) -> Result<Option<ResolvedModel>, WorldError> {
        match &self.models.0 {
            Some(source) => source.resolve(spec).map(Some),
            None => Ok(None),
        }
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

    /// The persistent entities, by id (they come before every transient one in [`World::entities`]).
    pub fn persistent_entities(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.entities.range(..TRANSIENT_ID_BASE).map(|(_, e)| e)
    }

    /// The transient entities (game presentation), by id.
    pub fn transient_entities(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.entities.range(TRANSIENT_ID_BASE..).map(|(_, e)| e)
    }

    /// See the field: unchanged means no persistent entity changed in any way since it was read.
    pub fn draw_epoch(&self) -> u64 {
        self.draw_epoch
    }

    /// The ids of the persistent entities that changed (were spawned, changed, moved, removed)
    /// after [`World::draw_epoch`] was `epoch`, oldest first, possibly repeated; `None` when the record
    /// no longer reaches back that far (more than [`DRAW_LOG`] changes since, or a change to
    /// every entity): read everything again then. What the window's draw cache patches itself
    /// from.
    pub fn draw_changes_since(&self, epoch: u64) -> Option<impl Iterator<Item = u64> + '_> {
        if epoch < self.draw_floor || epoch > self.draw_epoch {
            return None;
        }
        let start = self.draw_log.partition_point(|&(e, _)| e <= epoch);
        Some(self.draw_log.range(start..).map(|&(_, id)| id))
    }

    /// Records a change to entity `id` for [`World::draw_changes_since`] (transient ones aren't
    /// recorded: see the field).
    fn draw_touch(&mut self, id: u64) {
        if id >= TRANSIENT_ID_BASE {
            return;
        }
        self.draw_epoch += 1;
        if self.draw_log.len() >= DRAW_LOG {
            if let Some((e, _)) = self.draw_log.pop_front() {
                self.draw_floor = e;
            }
        }
        self.draw_log.push_back((self.draw_epoch, id));
    }

    /// A change to (possibly) every entity: the change record starts again from here.
    fn draw_reset(&mut self) {
        self.draw_epoch += 1;
        self.draw_log.clear();
        self.draw_floor = self.draw_epoch;
    }

    /// The entities carrying a point light, by id (from an index: no other entity is visited).
    pub fn lit_entities(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.lit.iter().filter_map(|id| self.entities.get(id))
    }

    /// The 2D entities (with `two_d`), by id (from an index: no 3D entity is visited).
    pub fn two_d_entities(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.flat.iter().filter_map(|id| self.entities.get(id))
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    pub fn set_env(&mut self, patch: &EnvPatch) -> Result<(), WorldError> {
        check_env(patch)?;
        let before = self.env;
        patch.apply(&mut self.env);
        self.revision += 1;
        self.record_env(before);
        Ok(())
    }

    /// Agents' named cameras: agent name → camera name → camera.
    pub fn cameras(&self) -> &BTreeMap<String, BTreeMap<String, crate::cameras::NamedCamera>> {
        &self.cameras
    }

    /// Creates or replaces camera `spec.name` of agent `owner` (its version goes up by one);
    /// `persisted`: saved with the world (cameras of agents that never said agent_hello aren't).
    /// Recorded as a [`Change::Camera`]; bounded per agent and per world.
    pub fn set_camera(
        &mut self,
        owner: &str,
        spec: crate::cameras::CameraSpec,
        persisted: bool,
    ) -> Result<&crate::cameras::NamedCamera, WorldError> {
        let before = self.cameras.get(owner).and_then(|c| c.get(&spec.name));
        let version = before.map_or(1, |c| c.version + 1);
        let name = spec.name.clone();
        self.restore_camera(owner, &name, Some(crate::cameras::NamedCamera { version, spec, persisted }))?;
        Ok(&self.cameras[owner][&name])
    }

    /// Puts camera `name` of `owner` as given (`None` removes it), refusing to go past the camera
    /// limits (`camera_limit`, `camera_owner_limit`): what set_camera and undo use.
    pub(crate) fn restore_camera(&mut self, owner: &str, name: &str, camera: Option<crate::cameras::NamedCamera>) -> Result<(), WorldError> {
        use crate::cameras::{MAX_CAMERAS_PER_AGENT, MAX_OWNERS};
        let held = self.cameras.get(owner);
        if camera.is_some() && !held.is_some_and(|c| c.contains_key(name)) {
            if held.is_none() && self.cameras.len() >= MAX_OWNERS {
                return Err(WorldError::new(
                    "camera_owner_limit",
                    format!("at most {MAX_OWNERS} agents hold cameras in one world; another agent must camera_delete theirs first"),
                ));
            }
            if held.is_some_and(|c| c.len() >= MAX_CAMERAS_PER_AGENT) {
                return Err(WorldError::new(
                    "camera_limit",
                    format!("at most {MAX_CAMERAS_PER_AGENT} cameras per agent; camera_delete one first"),
                )
                .with("owner", owner)
                .with("camera", name));
            }
        }
        self.put_camera_raw(owner, name, camera);
        Ok(())
    }

    /// See the `layout_epoch` field.
    pub fn layout_epoch(&self) -> u64 {
        self.layout_epoch
    }

    /// Deletes agent `owner`'s camera `name` (every one of its cameras with `None`); returns how
    /// many went. Each is recorded as a [`Change::Camera`].
    pub fn delete_cameras(&mut self, owner: &str, name: Option<&str>) -> Result<usize, WorldError> {
        let names: Vec<String> = match (self.cameras.get(owner), name) {
            (Some(c), Some(n)) if c.contains_key(n) => vec![n.to_owned()],
            (_, Some(n)) => {
                return Err(WorldError::new("camera_not_found", format!("you have no camera named '{n}' (camera_list shows them)")))
            }
            (Some(c), None) => c.keys().cloned().collect(),
            (None, None) => Vec::new(),
        };
        for n in &names {
            self.put_camera_raw(owner, n, None);
        }
        Ok(names.len())
    }

    /// Puts camera `name` of `owner` exactly as given (`None` removes it), recorded like any
    /// change: the journal uses it to replay and undo, world files to load.
    pub(crate) fn put_camera_raw(&mut self, owner: &str, name: &str, camera: Option<crate::cameras::NamedCamera>) {
        let held = self.cameras.entry(owner.to_owned()).or_default();
        let before = match &camera {
            Some(c) => held.insert(name.to_owned(), c.clone()),
            None => held.remove(name),
        };
        if held.is_empty() {
            self.cameras.remove(owner);
        }
        if before.is_none() && camera.is_none() {
            return; // nothing was there
        }
        self.revision += 1;
        if self.recording != Recording::Off {
            self.changes.push(Change::Camera {
                path: camera_path(owner, name),
                owner: owner.to_owned(),
                name: name.to_owned(),
                before: before.map(Box::new),
                after: camera.map(Box::new),
            });
        }
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
    /// The persistent id counter (at least the largest persistent id). Kept below the transient
    /// range: world files are checked on load ([`MAX_NEXT_ID`]); a counter from elsewhere (a
    /// replay) is capped there, where spawns are refused rather than given a transient id.
    pub(crate) fn set_next_id(&mut self, next_id: u64) {
        let max = self.entities.range(..TRANSIENT_ID_BASE).next_back().map_or(0, |(id, _)| *id);
        self.next_id = next_id.max(max).min(MAX_NEXT_ID);
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

    /// Adds an entity. Persistent entities take the next id; transient ones (see
    /// [`Entity::transient`]) take theirs from [`TRANSIENT_ID_BASE`] up, so presentation never
    /// moves the saved id counter.
    pub fn spawn(&mut self, spec: SpawnSpec, by: &str) -> Result<&Entity, WorldError> {
        // Ids are only used up by spawns that succeed.
        let id = self.insert(spec, None, 1, by, by)?;
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
        if id == ROOT || id > MAX_NEXT_ID || self.entities.contains_key(&id) {
            return Err(WorldError::new("invalid_world", format!("entity id {id} is 0, out of range or used twice")).with("field", "id"));
        }
        if spec.transient {
            return Err(invalid("transient", "transient entities are never saved, so none can be restored".into()));
        }
        self.insert(spec, Some(id), version.max(1), created_by, modified_by)?;
        self.next_id = self.next_id.max(id);
        Ok(())
    }

    /// Inserts a checked entity under `id` (`None`: the next free id of its kind). Returns its id.
    fn insert(&mut self, spec: SpawnSpec, id: Option<u64>, version: u64, created_by: &str, modified_by: &str) -> Result<u64, WorldError> {
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
        // Below a transient parent everything is transient (it is never saved, so neither could
        // its children be).
        let transient = spec.transient || parent.is_some_and(|p| self.entities[&p].transient);
        self.check_shapes(shape_spec(spec.two_d.as_ref()), None, transient)?;
        let resolved = match &spec.model {
            Some(m) => self.resolve_model(m)?,
            None => None,
        };
        let id = match id {
            Some(id) => id,
            None if transient => TRANSIENT_ID_BASE + self.transient_seq + 1,
            // Persistent ids stay below the transient range (a spawn there would take, or later
            // be overwritten by, a transient entity's id).
            None if self.next_id + 1 >= TRANSIENT_ID_BASE - 1 => {
                return Err(WorldError::new(
                    "limit",
                    format!("the world has used every persistent entity id (next would be {}; they end below {})", self.next_id + 1, TRANSIENT_ID_BASE - 1),
                )
                .with("field", "id")
                .with("limit", TRANSIENT_ID_BASE - 2)
                .with("value", self.next_id + 1));
            }
            None => self.next_id + 1,
        };
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
            model: spec.model,
            two_d: spec.two_d,
            anim: spec.anim.map(|mut a| {
                a.start.get_or_insert(0.0);
                a
            }),
            visible: spec.visible,
            layer: spec.layer,
            transient,
            version,
            created_by,
            modified_by,
            world: Mat4::IDENTITY,
            shown: true,
            in_ui: false,
            resolved,
            hash: HashCache::default(),
        });
        if transient {
            self.transient_seq = id - TRANSIENT_ID_BASE;
        } else {
            self.next_id = self.next_id.max(id);
        }
        self.refresh(id);
        self.touch(id);
        self.bump(transient);
        if matches!(spec.shape, Shape::Lathe | Shape::Model) {
            self.mesh_epoch += 1;
        }
        match self.recording {
            Recording::Off => {}
            // Game presentation runs every tick: transient entities cost no record at all.
            Recording::Snapshot if transient => {}
            Recording::Snapshot => {
                self.dirty.entry(id).or_insert(Dirty::Spawned);
            }
            Recording::Full => {
                let path = self.path_of(id);
                self.changes.push(Change::Spawn { path, after: Box::new(self.entities[&id].clone()) });
            }
        }
        Ok(id)
    }

    /// Counts a change: the revision (what autosave and the state-hash cache key on) moves only
    /// for persistent state.
    fn bump(&mut self, transient: bool) {
        if !transient {
            self.revision += 1;
        }
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
    ///
    /// For model entities, `look.seed` (the `seed` argument) sets the **model** seed: it must be
    /// an integer 0..=[`MAX_MODEL_SEED`].
    pub fn update(&mut self, key: &EntityKey, change: UpdateSpec, by: &str) -> Result<&Entity, WorldError> {
        let mut change = change;
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
        if matches!(change.anim, Some(Some(_))) && e.shape != Shape::Model {
            return Err(WorldError::new(
                "invalid_component",
                format!("'{}' is a {}; only model entities animate", e.name, e.shape.name()),
            )
            .with("field", "anim"));
        }
        // A new animation starts now (unless told when) and fades from the one it replaces.
        if let Some(Some(a)) = &mut change.anim {
            a.start.get_or_insert(self.world_time());
            a.from = match (&e.anim, a.crossfade > 0.0) {
                (Some(old), true) => Some(Box::new(AnimSpec { from: None, ..old.clone() })),
                _ => None,
            };
        }
        if (change.recipe.is_some() || change.params.is_some()) && e.shape != Shape::Model {
            let field = if change.recipe.is_some() { "recipe" } else { "params" };
            return Err(WorldError::new(
                "invalid_component",
                format!("'{}' is a {}; only model entities have a recipe and params", e.name, e.shape.name()),
            )
            .with("field", field));
        }
        // A 2D component's new value, worked out before anything changes.
        let mut new_two_d = None;
        if change.two_d.is_some() || change.tiles.is_some() {
            let field = if change.two_d.is_some() { "two_d" } else { "tiles" };
            let wrong = |want: &str| {
                WorldError::new("invalid_component", format!("'{}' is a {}; {want}", e.name, e.shape.name())).with("field", field)
            };
            let mut next = match change.two_d.take() {
                Some(t) if Some(t.key()) == e.two_d.as_ref().map(TwoD::key) => t,
                Some(t) => return Err(wrong(&format!("only {} entities take a '{}'", t.key(), t.key()))),
                None => e.two_d.clone().ok_or_else(|| wrong("only tilemap entities have tiles"))?,
            };
            if let Some(edits) = change.tiles.take() {
                match &mut next {
                    TwoD::Tilemap(t) => {
                        std::sync::Arc::make_mut(t).apply(&edits).map_err(|m| crate::two_d::invalid("cells", m))?;
                    }
                    _ => return Err(wrong("only tilemap entities have tiles")),
                }
            }
            self.check_shapes(shape_spec(Some(&next)), shape_spec(e.two_d.as_ref()), e.transient)?;
            new_two_d = Some(next);
        }
        // A model's new spec, resolved before anything changes (a broken recipe changes nothing).
        let mut new_model = None;
        if let Some(current) = &e.model {
            let seed = match change.look.seed.take() {
                None => None,
                Some(s) if s >= 0.0 && s.fract() == 0.0 && s <= MAX_MODEL_SEED as f32 => Some(s as u64),
                Some(_) => {
                    return Err(invalid("seed", format!("'seed' of a model must be an integer 0..{MAX_MODEL_SEED}")));
                }
            };
            let mut spec = current.clone();
            if let Some(r) = change.recipe.take() {
                spec.recipe = r;
            }
            if let Some(p) = change.params.take() {
                spec.params = p;
            }
            if seed.is_some() {
                spec.seed = seed;
            }
            if spec != *current {
                spec.check()?;
                let resolved = self.resolve_model(&spec)?;
                new_model = Some((spec, resolved));
            }
        }
        let new_parent = match &change.parent {
            None => e.parent,
            Some(None) => None,
            Some(Some(k)) => Some(self.resolve(k).map_err(|err| err.with("field", "parent"))?),
        };
        let reparent = new_parent != e.parent;
        if reparent {
            if let Some(p) = new_parent {
                if !e.transient && self.entities[&p].transient {
                    let (me, there) = (self.path_of(id), self.path_of(p));
                    return Err(WorldError::new(
                        "transient_parent",
                        format!("{me} is saved, so it can't move under {there}, which is transient (never saved)"),
                    )
                    .with("path", me)
                    .with("parent", there));
                }
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

        let before = (self.recording == Recording::Full).then(|| Box::new(e.clone()));
        let pose_only = change.pose_only();
        let prev_path = (self.recording == Recording::Full && relink).then(|| self.path_of(id));
        let old = relink.then(|| (e.parent.unwrap_or(ROOT), e.name.clone()));
        let restyled = change.visible.is_some_and(|v| v != e.visible) || change.layer.is_some_and(|l| l != e.layer);
        let transient = e.transient;
        if reparent {
            self.touch(id); // the subtree it leaves
        }
        let by = self.intern(by);
        let moved = reparent || restyled || change.position.is_some() || change.rotation_deg.is_some() || change.scale.is_some();
        let remeshed = change.profile.is_some() || new_model.is_some();
        self.draw_touch(id);
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
            if l.is_some() {
                self.lit.insert(id);
            } else {
                self.lit.remove(&id);
            }
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
        if let Some(a) = change.anim {
            e.anim = a;
        }
        if let Some((spec, resolved)) = new_model {
            e.model = Some(spec);
            e.resolved = resolved;
        }
        if let Some(v) = change.visible {
            e.visible = v;
        }
        if let Some(l) = change.layer {
            e.layer = l;
        }
        if let Some(t) = new_two_d {
            let pool = if e.transient { &mut self.transient_shapes } else { &mut self.shapes };
            if let Some(TwoD::Shape2d(s)) = &e.two_d {
                pool.release(s);
            }
            if let TwoD::Shape2d(s) = &t {
                pool.charge(s);
            }
            e.two_d = Some(t);
        }
        e.version += 1;
        e.hash.0.set(None);
        e.modified_by = by;
        if let Some((old_parent, old_name)) = old {
            let new_key = (e.parent.unwrap_or(ROOT), e.name.clone());
            self.names.remove(&(old_name.clone(), id));
            self.names.insert((new_key.1.clone(), id));
            self.children.remove(&(old_parent, old_name));
            self.children.insert(new_key, id);
            self.layout_epoch += 1;
        }
        if moved {
            self.refresh(id);
        }
        self.touch(id);
        self.bump(transient);
        if remeshed {
            self.mesh_epoch += 1;
            self.model_drops += 1;
        }
        match self.recording {
            Recording::Off => {}
            Recording::Snapshot if transient => {}
            Recording::Snapshot => {
                let mark = if pose_only { Dirty::Pose } else { Dirty::Changed };
                let slot = self.dirty.entry(id).or_insert(mark);
                if *slot == Dirty::Pose {
                    *slot = mark;
                }
            }
            Recording::Full => {
                let after = Box::new(self.entities[&id].clone());
                self.changes.push(Change::Set { path: self.path_of(id), prev_path, before, after });
            }
        }
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
        let paths: Vec<String> =
            if self.recording == Recording::Off { Vec::new() } else { order.iter().map(|&i| self.path_of(i)).collect() };
        let removed: Vec<Entity> = order.into_iter().map(|i| self.unlink(i).expect("in the subtree")).collect();
        // A transient root has only transient descendants.
        self.bump(removed[0].transient);
        if removed.iter().any(|e| e.shape == Shape::Model) {
            self.model_drops += 1;
        }
        if removed.iter().any(Entity::owns_mesh) {
            self.mesh_epoch += 1;
        }
        // Children first, so undoing the changes in reverse puts parents back before children.
        for (e, p) in removed.iter().zip(paths).rev() {
            self.record_removal(p, e);
        }
        Ok(Deleted { id, path, removed })
    }

    /// Applies every op or none: on the first failure everything already applied is rolled back
    /// (ids, versions, indexes and the revision counter included) and the failing op's index is
    /// returned.
    /// Later ops can address entities spawned earlier in the batch by path.
    pub fn apply_batch(&mut self, ops: Vec<Op>, by: &str) -> Result<Vec<OpResult>, (usize, WorldError)> {
        let (next_id, revision, recorded, transient_seq) = (self.next_id, self.revision, self.changes.len(), self.transient_seq);
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
                    self.transient_seq = transient_seq;
                    self.revision = revision;
                    self.mesh_epoch += 1;
                    self.model_drops += 1;
                    self.changes.truncate(recorded);
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

    /// Indexes `e` (its id must not be in the world).
    fn link(&mut self, e: Entity) {
        self.layout_epoch += 1;
        self.children.insert((e.parent.unwrap_or(ROOT), e.name.clone()), e.id);
        self.names.insert((e.name.clone(), e.id));
        if e.transient {
            self.transient_count += 1;
        }
        if e.light.is_some() {
            self.lit.insert(e.id);
        }
        if e.two_d.is_some() {
            self.flat.insert(e.id);
        }
        self.draw_touch(e.id);
        if let Some(TwoD::Shape2d(s)) = &e.two_d {
            if e.transient { &mut self.transient_shapes } else { &mut self.shapes }.charge(s);
        }
        self.entities.insert(e.id, e);
    }

    /// Removes one entity from every index (its children are left in place).
    fn unlink(&mut self, id: u64) -> Option<Entity> {
        self.draw_touch(id);
        let e = self.entities.remove(&id)?;
        self.lit.remove(&id);
        self.flat.remove(&id);
        self.layout_epoch += 1;
        self.children.remove(&(e.parent.unwrap_or(ROOT), e.name.clone()));
        self.names.remove(&(e.name.clone(), id));
        if e.transient {
            self.transient_count -= 1;
        }
        if let Some(s) = shape_spec(e.two_d.as_ref()) {
            if e.transient { &mut self.transient_shapes } else { &mut self.shapes }.release(s);
        }
        Some(e)
    }

    /// The distinct vector shapes of the persistent entities and their vertex bounds.
    pub fn shape_budget(&self) -> &crate::two_d::ShapeBudget {
        &self.shapes
    }

    /// The distinct vector shapes of the transient entities (game presentation).
    pub fn transient_shape_budget(&self) -> &crate::two_d::ShapeBudget {
        &self.transient_shapes
    }

    pub fn shape_limits(&self) -> crate::two_d::ShapeLimits {
        self.shape_limits
    }

    /// Usage of the two shape budgets, by their perf.toml names:
    /// `{shape2d_vertices: {used, limit, shapes}, transient_shape2d_vertices: {…}}` (`used`: the
    /// vertex bounds of the distinct shapes; `shapes`: how many distinct shapes).
    pub fn shape_budgets_json(&self) -> Json {
        let pool = |b: &crate::two_d::ShapeBudget, limit: usize| {
            Json::obj([("used", Json::from(b.vertices())), ("limit", Json::from(limit)), ("shapes", Json::from(b.distinct()))])
        };
        Json::obj([
            ("shape2d_vertices", pool(&self.shapes, self.shape_limits.persistent)),
            ("transient_shape2d_vertices", pool(&self.transient_shapes, self.shape_limits.transient)),
        ])
    }

    /// Sets the two shape budgets (perf.toml). Shapes already in the world stay, whatever the new
    /// limits; only later changes are checked against them.
    pub fn set_shape_limits(&mut self, limits: crate::two_d::ShapeLimits) {
        self.shape_limits = limits;
    }

    /// Refuses an entity's shape going from `old` to `new` when the distinct shapes of its
    /// pool (persistent or `transient` entities) would pass that pool's budget (identical shapes
    /// count once). The budgets are separate so game effects can never crowd out authored
    /// content, and a refusal says exactly what an effect costs.
    fn check_shapes(
        &self,
        new: Option<&crate::two_d::Shape2dSpec>,
        old: Option<&crate::two_d::Shape2dSpec>,
        transient: bool,
    ) -> Result<(), WorldError> {
        let Some(n) = new else { return Ok(()) };
        let (pool, max, budget) = if transient {
            (&self.transient_shapes, self.shape_limits.transient, "transient_shape2d_vertices")
        } else {
            (&self.shapes, self.shape_limits.persistent, "shape2d_vertices")
        };
        let total = pool.after(new, old);
        if total <= max || total <= pool.vertices() {
            return Ok(());
        }
        let whose = if transient { "transient (game-spawned) shape2d shapes" } else { "world's distinct shape2d shapes" };
        Err(WorldError::new(
            "limit",
            format!(
                "the {whose} would need up to {total} vertices; at most {max} ({budget} in perf.toml; this one: up to {}; \
                 identical shapes count once: reuse one, delete unused ones, or use fewer items or a larger tolerance)",
                n.vertices
            ),
        )
        .with("field", "shape2d")
        .with("budget", budget)
        .with("limit", max)
        .with("value", total)
        .with("shape_vertices", n.vertices))
    }

    /// Recomputes the world transforms (and the inherited visibility and layer) of `id` and
    /// everything below it. A leaf costs no allocation: games move hundreds of them every tick.
    fn refresh(&mut self, id: u64) {
        if !self.refresh_one(id) {
            return;
        }
        let mut stack: Vec<u64> = self.child_range(id).map(|(_, &c)| c).collect();
        while let Some(at) = stack.pop() {
            if self.refresh_one(at) {
                stack.extend(self.child_range(at).map(|(_, &c)| c));
            }
        }
    }

    /// Recomputes one entity's cached world state from its parent's; returns whether it has
    /// children.
    fn refresh_one(&mut self, at: u64) -> bool {
        let Some(parent) = self.entities.get(&at).map(|e| e.parent) else { return false };
        let inherited = parent.and_then(|p| self.entities.get(&p)).map(|p| (p.world, p.shown, p.in_ui));
        self.draw_touch(at);
        let e = self.entities.get_mut(&at).expect("checked above");
        let local = e.local_matrix();
        // Top-level entities use their local matrix as is (no identity multiply to round).
        let (world, shown, in_ui) = match inherited {
            Some((pw, shown, in_ui)) => (pw * local, shown, in_ui),
            None => (local, true, false),
        };
        e.world = world;
        e.shown = shown && e.visible;
        e.in_ui = in_ui || e.layer == Layer::Ui;
        self.child_range(at).next().is_some()
    }

    /// What the world records about its changes from now on.
    pub fn set_recording(&mut self, recording: Recording) {
        self.recording = recording;
    }

    pub fn recording(&self) -> Recording {
        self.recording
    }

    /// The changes recorded since the last call, oldest first. With [`Recording::Snapshot`]:
    /// one change per entity that changed, as it is now (spawns and sets by id, then removals
    /// children first, then the environment).
    ///
    /// Transient entities' changes are left out: they are recorded while a command runs (so a
    /// rejected command rolls them back like anything else), but they are never journaled.
    pub fn take_changes(&mut self) -> Vec<Change> {
        let mut out = std::mem::take(&mut self.changes);
        out.retain(|c| !c.is_transient());
        if self.dirty.is_empty() && self.dirty_env.is_none() && self.dirty_removed.is_empty() {
            return out;
        }
        let dirty = std::mem::take(&mut self.dirty);
        out.reserve(dirty.len());
        for (&id, &how) in &dirty {
            if let Some(e) = self.entities.get(&id) {
                let path = self.path_of(id);
                out.push(match how {
                    Dirty::Pose => Change::Pose { path, pose: Pose::of(e) },
                    Dirty::Changed => Change::Set { path, prev_path: None, before: None, after: Box::new(e.clone()) },
                    Dirty::Spawned => Change::Spawn { path, after: Box::new(e.clone()) },
                });
            }
        }
        let mut gone = BTreeSet::new();
        for (path, e) in std::mem::take(&mut self.dirty_removed) {
            // Still gone (a rolled-back batch may have restored it), and listed once.
            if !self.entities.contains_key(&e.id) && gone.insert(e.id) {
                out.push(Change::Delete { path, before: Box::new(e) });
            }
        }
        if let Some(before) = self.dirty_env.take() {
            if before != self.env {
                out.push(Change::Env { before: Box::new(before), after: Box::new(self.env) });
            }
        }
        out
    }

    /// Records that `e` (at `path`) was removed.
    fn record_removal(&mut self, path: String, e: &Entity) {
        match self.recording {
            Recording::Off => {}
            Recording::Snapshot if e.transient => {}
            Recording::Snapshot => {
                if *self.dirty.entry(e.id).or_insert(Dirty::Changed) != Dirty::Spawned {
                    self.dirty_removed.push((path, e.clone()));
                }
            }
            Recording::Full => self.changes.push(Change::Delete { path, before: Box::new(e.clone()) }),
        }
    }

    /// Records an environment change from `before`.
    fn record_env(&mut self, before: Env) {
        match self.recording {
            Recording::Off => {}
            Recording::Snapshot => {
                self.dirty_env.get_or_insert(before);
            }
            Recording::Full => self.changes.push(Change::Env { before: Box::new(before), after: Box::new(self.env) }),
        }
    }

    /// Recorded changes not taken yet.
    pub fn pending_changes(&self) -> &[Change] {
        &self.changes
    }

    /// Remembers the current state, so [`World::rollback_to`] can return to it. Needs
    /// [`Recording::Full`] for every change in between.
    pub(crate) fn mark(&self) -> Mark {
        Mark { next_id: self.next_id, revision: self.revision, changes: self.changes.len() }
    }

    /// How many changes were pending at `mark`: `pending_changes()[mark_changes(m)..]` are the
    /// ones since.
    pub(crate) fn mark_changes(&self, mark: Mark) -> usize {
        mark.changes.min(self.changes.len())
    }

    /// Undoes every change recorded since `mark` exactly (versions and attribution included) and
    /// forgets them. The id counter is **not** rewound: ids handed out in between (which game code
    /// may have kept) are never handed out again.
    ///
    /// The revision never goes back either: it moves past every state the world has been in, so
    /// whatever is keyed on it (the engine's cached state hash, autosave) sees the rollback, and
    /// the id counter that moved on.
    pub(crate) fn rollback_to(&mut self, mark: Mark) {
        let undo: Vec<Change> = self.changes.drain(mark.changes.min(self.changes.len())..).collect();
        let recording = std::mem::replace(&mut self.recording, Recording::Off);
        let revision = self.revision;
        for change in undo.iter().rev() {
            self.apply_before(change);
        }
        self.recording = recording;
        if !undo.is_empty() || self.next_id != mark.next_id || revision != mark.revision {
            self.revision = self.revision.max(revision) + 1;
        }
    }

    /// Re-checks the changes recorded since `mark` one step at a time, each against the world as
    /// it was right after that step (a later step can hide what an earlier one did: a rename
    /// followed by a delete, two moves). On the first rejection everything since `mark` is rolled
    /// back and the error returned; otherwise the world and its recorded changes are as before.
    pub(crate) fn recheck_steps(
        &mut self,
        mark: Mark,
        check: &mut dyn FnMut(&World, &Change) -> Result<(), WorldError>,
    ) -> Result<(), WorldError> {
        let start = self.mark_changes(mark);
        if start == self.changes.len() {
            return Ok(());
        }
        let steps: Vec<Change> = self.changes.drain(start..).collect();
        let (next_id, revision) = (self.next_id, self.revision);
        let recording = std::mem::replace(&mut self.recording, Recording::Off);
        // Back to the start, then forward one change at a time.
        for change in steps.iter().rev() {
            self.apply_before(change);
        }
        let mut failed = None;
        for (done, change) in steps.iter().enumerate() {
            // Agents' and game tools' changes are full states (no poses), so no author is needed.
            self.apply_after(change, "");
            if let Err(e) = check(self, change) {
                failed = Some((done, e));
                break;
            }
        }
        match failed {
            None => {
                self.recording = recording;
                self.changes.extend(steps);
                self.next_id = self.next_id.max(next_id);
                self.revision = revision.max(self.revision);
                Ok(())
            }
            Some((done, e)) => {
                for change in steps[..=done].iter().rev() {
                    self.apply_before(change);
                }
                self.recording = recording;
                self.next_id = self.next_id.max(next_id);
                // Forward only (see `rollback_to`).
                self.revision = self.revision.max(revision) + 1;
                Err(e)
            }
        }
    }

    /// Puts the state a change left (its `after`; removes what it deleted).
    pub(crate) fn apply_after(&mut self, change: &Change, by: &str) {
        match change {
            Change::Spawn { after, .. } | Change::Set { after, .. } => self.put_raw((**after).clone()),
            Change::Delete { before, .. } => {
                self.remove_raw(before.id);
            }
            Change::Env { after, .. } => self.replace_env(**after),
            Change::Pose { pose, .. } => self.apply_pose(pose, by),
            Change::Camera { owner, name, after, .. } => self.put_camera_raw(owner, name, after.as_deref().cloned()),
        }
    }

    /// Puts back the state before a change (needs its `before`).
    fn apply_before(&mut self, change: &Change) {
        match change {
            Change::Spawn { after, .. } => {
                self.remove_raw(after.id);
            }
            Change::Set { before: Some(before), .. } | Change::Delete { before, .. } => self.put_raw((**before).clone()),
            Change::Set { before: None, after, .. } => {
                debug_assert!(false, "rollback over an after-only change of entity {}", after.id);
            }
            Change::Pose { pose, .. } => {
                debug_assert!(false, "rollback over a pose of entity {}", pose.id);
            }
            Change::Env { before, .. } => self.replace_env(**before),
            Change::Camera { owner, name, before, .. } => self.put_camera_raw(owner, name, before.as_deref().cloned()),
        }
    }

    /// Puts `e` into the world exactly as given, inserting it or replacing the entity with its id
    /// (re-indexed, world transforms refreshed for its subtree). No checks: the journal uses it to
    /// replay and undo states the world held before. Recorded like any change.
    pub(crate) fn put_raw(&mut self, mut e: Entity) {
        e.created_by = self.intern(&e.created_by);
        e.modified_by = self.intern(&e.modified_by);
        let id = e.id;
        let recording = self.recording;
        let old = self.entities.get(&id);
        let existed = old.is_some();
        // Same parent and local transform: its world transform (and its subtree's) stand as they
        // are. Recomputing a big subtree for a colour change is what made long batches slow.
        let unmoved = old.filter(|o| {
            let bits = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
            o.parent == e.parent
                && bits(o.position) == bits(e.position)
                && bits(o.rotation_deg) == bits(e.rotation_deg)
                && bits(o.scale) == bits(e.scale)
                && o.visible == e.visible
                && o.layer == e.layer
        });
        if let Some(o) = unmoved {
            e.world = o.world;
            e.shown = o.shown;
            e.in_ui = o.in_ui;
        }
        // A state from the journal carries the handle and bounds of its time; recipes may have
        // changed since, and the handle may have been reused for another spec: never keep it.
        if let (Some(spec), Some(source)) = (&e.model, &self.models.0) {
            e.resolved = source.resolve(spec).ok();
        }
        let transient = e.transient;
        let remeshed = e.owns_mesh() || old.is_some_and(Entity::owns_mesh);
        let unmoved = unmoved.is_some();
        let before = match (recording, old) {
            (Recording::Full, Some(o)) => Some(Box::new(o.clone())),
            _ => None,
        };
        let prev_path = (recording == Recording::Full && existed).then(|| self.path_of(id));
        if existed {
            self.touch(id);
            self.unlink_exact(id);
        }
        self.link(e);
        if !unmoved {
            self.refresh(id);
        }
        self.touch(id);
        if transient {
            self.transient_seq = self.transient_seq.max(id - TRANSIENT_ID_BASE);
        } else {
            self.next_id = self.next_id.max(id);
        }
        self.bump(transient);
        if remeshed {
            self.mesh_epoch += 1;
            self.model_drops += 1;
        }
        if recording == Recording::Snapshot && !transient {
            let mark = if existed { Dirty::Changed } else { Dirty::Spawned };
            let slot = self.dirty.entry(id).or_insert(mark);
            if *slot == Dirty::Pose {
                *slot = mark;
            }
        }
        if recording == Recording::Full {
            let path = self.path_of(id);
            let after = Box::new(self.entities[&id].clone());
            self.changes.push(match prev_path {
                None => Change::Spawn { path, after },
                Some(prev) => {
                    let prev_path = (prev != path).then_some(prev);
                    Change::Set { path, prev_path, before, after }
                }
            });
        }
    }

    /// Removes one entity (its children, if any, are left in place: callers remove subtrees
    /// children first). Recorded like any change.
    pub(crate) fn remove_raw(&mut self, id: u64) -> Option<Entity> {
        let path = (self.recording != Recording::Off && self.entities.contains_key(&id)).then(|| self.path_of(id));
        self.touch_removal(id);
        let e = self.unlink_exact(id)?;
        self.bump(e.transient);
        if e.shape == Shape::Model {
            self.model_drops += 1;
        }
        if e.owns_mesh() {
            self.mesh_epoch += 1;
        }
        if let Some(path) = path {
            self.record_removal(path, &e);
        }
        Some(e)
    }

    /// Replaces the whole environment. Recorded like any change.
    pub(crate) fn replace_env(&mut self, env: Env) {
        let before = self.env;
        self.env = env;
        self.revision += 1;
        self.record_env(before);
    }

    /// Replays a [`Change::Pose`]: the entity takes the pose, attributed to `by`. Unknown ids
    /// are ignored (the change's entity was replayed away).
    pub(crate) fn apply_pose(&mut self, pose: &Pose, by: &str) {
        let by = self.intern(by);
        self.draw_touch(pose.id);
        let Some(e) = self.entities.get_mut(&pose.id) else { return };
        pose.apply_to(e);
        e.modified_by = by;
        let transient = e.transient;
        self.refresh(pose.id);
        self.touch(pose.id);
        self.bump(transient);
    }

    /// Restores the id counter as a replayed change left it (never below an existing id).
    pub(crate) fn replay_next_id(&mut self, next_id: u64) {
        self.set_next_id(next_id);
    }

    /// Like [`World::unlink`], but only drops index entries that still point at `id` (a replayed
    /// rename may already have given its old name to another entity).
    fn unlink_exact(&mut self, id: u64) -> Option<Entity> {
        self.draw_touch(id);
        let e = self.entities.remove(&id)?;
        self.lit.remove(&id);
        self.flat.remove(&id);
        self.layout_epoch += 1;
        let key = (e.parent.unwrap_or(ROOT), e.name.clone());
        if self.children.get(&key) == Some(&id) {
            self.children.remove(&key);
        }
        self.names.remove(&(e.name.clone(), id));
        if e.transient {
            self.transient_count -= 1;
        }
        if let Some(s) = shape_spec(e.two_d.as_ref()) {
            if e.transient { &mut self.transient_shapes } else { &mut self.shapes }.release(s);
        }
        Some(e)
    }

    /// Whether [`World::put_raw`] may put `e` back where it was: its parent exists and isn't
    /// inside it, its name is free among its siblings, and the depth limit holds.
    pub(crate) fn check_put(&self, e: &Entity) -> Result<(), WorldError> {
        let existing = self.entities.contains_key(&e.id);
        if let Some(p) = e.parent {
            if !self.entities.contains_key(&p) {
                return Err(WorldError::new(
                    "parent_gone",
                    format!("'{}' can't go back under entity {p}: that parent no longer exists", e.name),
                )
                .with("id", e.id)
                .with("parent", p));
            }
            if existing && self.is_ancestor_or_self(e.id, p) {
                let (me, there) = (self.path_of(e.id), self.path_of(p));
                return Err(WorldError::new("cycle", format!("{me} cannot move under {there}: that is itself or inside it"))
                    .with("path", me)
                    .with("parent", there));
            }
            let height = if existing { self.height_of(e.id) } else { 1 };
            if self.depth_of(p) + height > MAX_DEPTH {
                return Err(too_deep(&self.path_of(p)));
            }
        }
        match self.children.get(&(e.parent.unwrap_or(ROOT), e.name.clone())) {
            Some(&other) if other != e.id => return Err(self.name_taken(&e.name, other)),
            _ => {}
        }
        // Entities keep their kind (ids are per kind), so the old state's shape is in the same pool.
        let old = self.entities.get(&e.id).and_then(|o| shape_spec(o.two_d.as_ref()));
        self.check_shapes(shape_spec(e.two_d.as_ref()), old, e.transient)
    }

    /// Ids of `id` and everything below it, children before their parents.
    pub(crate) fn subtree_children_first(&self, id: u64) -> Vec<u64> {
        let mut order = Vec::new();
        let mut stack = vec![id];
        while let Some(at) = stack.pop() {
            if !self.entities.contains_key(&at) {
                continue;
            }
            order.push(at);
            stack.extend(self.child_range(at).map(|(_, &c)| c));
        }
        order.reverse();
        order
    }

    /// Whether the entity has children.
    pub fn has_children(&self, id: u64) -> bool {
        self.child_range(id).next().is_some()
    }

    /// Whether the entity has children that are not transient (game presentation the game may
    /// have hung under it doesn't count).
    pub fn has_persistent_children(&self, id: u64) -> bool {
        self.child_range(id).any(|(_, c)| self.entities.get(c).is_some_and(|e| !e.transient))
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
                "seed" => match &e.model {
                    Some(m) => Json::from(m.seed),
                    None => Json::from(s.seed),
                },
                "visible" => Json::from(e.visible),
                "layer" => Json::from(e.layer.name()),
                "transient" => Json::from(e.transient),
                "model_state" => if e.model.is_some() { Json::from(e.model_state()) } else { Json::Null },
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

    /// World-space bounds of everything shown with geometry (hidden and `ui` entities left out),
    /// or `None` when there is none.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut corners = self.entities.values().filter(|e| e.shown && !e.in_ui).filter_map(|e| e.world_corners()).flatten();
        let first = corners.next()?;
        Some(corners.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// A hash of everything that defines the world (entities by id, the environment, the id
    /// counter), stable across runs and platforms. Versions and attribution are left out, and so
    /// are transient entities: they are game presentation, derived from the game's own state
    /// (which the game hashes itself), so two runs of the same game agree whatever frame rate
    /// drew them.
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
        // Effects change what is drawn, so they are part of the state.
        crate::effects::ShadowPatch::hash(&l.shadow, &mut h);
        crate::effects::BloomPatch::hash(&l.bloom, &mut h);
        h.write_bool(l.fxaa);
        crate::effects::SkyPatch::hash(&l.sky, &mut h);
        crate::effects::FogPatch::hash(&l.fog, &mut h);
        crate::effects::GradingPatch::hash(&l.grading, &self.env.grading_lut, &mut h);
        crate::effects::SsaoPatch::hash_if_set(&l.ssao, &mut h);
        h.write_u64(self.persistent_len() as u64);
        for e in self.entities.range(..TRANSIENT_ID_BASE).map(|(_, e)| e) {
            h.write_u64(e.content_hash());
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
        format!("{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}", w.entities, w.children, w.names, w.lit, w.flat, w.next_id, w.revision, w.env)
    }

    /// The change record renderers patch caches from: every change to an entity (moves of a
    /// subtree included), in order; gone once it overflows or a change touches every entity.
    #[test]
    fn draw_changes_record_what_changed() {
        let mut w = World::default();
        let a = put(&mut w, Shape::Cube, "a", None, Vec3::ZERO);
        let g = put(&mut w, Shape::Group, "g", None, Vec3::ZERO);
        let b = put(&mut w, Shape::Cube, "b", Some("/g"), Vec3::ZERO);
        let since = w.draw_epoch();
        assert_eq!(w.draw_changes_since(since).unwrap().count(), 0);
        let mv = |p: Vec3| UpdateSpec { position: Some(p), ..UpdateSpec::default() };
        w.update(&EntityKey::Id(a), mv(Vec3::X), "t").unwrap();
        w.update(&EntityKey::Id(g), mv(Vec3::Y), "t").unwrap();
        let changed: BTreeSet<u64> = w.draw_changes_since(since).unwrap().collect();
        assert_eq!(changed, BTreeSet::from([a, g, b]), "the group's move reaches its child");
        w.delete(&EntityKey::Id(a), None).unwrap();
        assert!(w.draw_changes_since(since).unwrap().any(|id| id == a));
        assert!(w.draw_changes_since(w.draw_epoch() + 1).is_none(), "an epoch from another world");
        let mid = w.draw_epoch();
        for i in 0..DRAW_LOG + 1 {
            w.update(&EntityKey::Id(b), mv(Vec3::new(i as f32, 0.0, 0.0)), "t").unwrap();
        }
        assert!(w.draw_changes_since(mid).is_none(), "the record overflowed");
        assert_eq!(w.draw_changes_since(w.draw_epoch() - 3).unwrap().count(), 3);
        w.refresh_models();
        assert!(w.draw_changes_since(w.draw_epoch() - 1).is_none(), "a change to every entity starts the record again");
    }

    /// The lit-entity index agrees with a scan of every entity.
    fn assert_lit_index(w: &World) {
        let scanned: Vec<u64> = w.entities().filter(|e| e.light.is_some()).map(|e| e.id).collect();
        let indexed: Vec<u64> = w.lit_entities().map(|e| e.id).collect();
        assert_eq!(indexed, scanned);
        assert_eq!(w.lit.len(), scanned.len(), "no stale ids in the index");
        let flat: Vec<u64> = w.entities().filter(|e| e.two_d.is_some()).map(|e| e.id).collect();
        assert_eq!(w.two_d_entities().map(|e| e.id).collect::<Vec<_>>(), flat);
        assert_eq!(w.flat.len(), flat.len());
    }

    /// F3: frames gather point lights from an index, never by visiting every entity. The index
    /// follows spawns (persistent and transient), light changes, deletes of whole subtrees, a
    /// failed batch's rollback and journal replays.
    #[test]
    fn lit_index_follows_every_change() {
        let lamp = || LightSpec { intensity: 1.0, range: 3.0, color: None, offset: Vec3::ZERO };
        let mut w = World::default();
        w.set_recording(Recording::Full);
        let group = put(&mut w, Shape::Group, "g", None, Vec3::ZERO);
        let mut lit = SpawnSpec::new(Shape::Cube);
        lit.name = Some("lamp".into());
        lit.parent = Some(EntityKey::Id(group));
        lit.light = Some(lamp());
        let lamp_id = w.spawn(lit, "t").unwrap().id;
        for i in 0..50 {
            put(&mut w, Shape::Cube, &format!("c{i}"), None, Vec3::ZERO);
        }
        let mut torch = SpawnSpec::new(Shape::Sphere);
        torch.name = Some("torch".into());
        torch.transient = true;
        torch.light = Some(lamp());
        let torch_id = w.spawn(torch, "game").unwrap().id;
        let mut tree = SpawnSpec::new(Shape::Sprite);
        tree.name = Some("tree".into());
        tree.parent = Some(EntityKey::Id(group));
        tree.two_d = Some(crate::two_d::parse("sprite", &Json::parse(r#"{"size":[2,4]}"#).unwrap()).unwrap());
        let tree_id = w.spawn(tree, "t").unwrap().id;
        assert_eq!(w.two_d_entities().map(|e| e.id).collect::<Vec<_>>(), [tree_id], "the 2D index holds the sprite alone");
        assert_lit_index(&w);
        assert_eq!(w.lit_entities().map(|e| e.id).collect::<Vec<_>>(), [lamp_id, torch_id]);

        // Light added to a plain cube, then removed; removed from the transient torch.
        let set = |light| UpdateSpec { light: Some(light), ..UpdateSpec::default() };
        w.update(&EntityKey::Path("/c7".into()), set(Some(lamp())), "t").unwrap();
        assert_lit_index(&w);
        assert_eq!(w.lit.len(), 3);
        w.update(&EntityKey::Path("/c7".into()), set(None), "t").unwrap();
        w.update(&EntityKey::Id(torch_id), set(None), "game").unwrap();
        assert_lit_index(&w);
        assert_eq!(w.lit.len(), 1);

        // A batch that adds a light then fails rolls the index back too.
        let mut bad = SpawnSpec::new(Shape::Cube);
        bad.name = Some("lit2".into());
        bad.light = Some(lamp());
        let before = snapshot(&w);
        let ops = vec![Op::Spawn(bad), Op::Set(EntityKey::Path("/c3".into()), set(Some(lamp()))), Op::Delete(EntityKey::Path("/nope".into()), None)];
        assert!(w.apply_batch(ops, "t").is_err());
        assert_eq!(snapshot(&w), before);
        assert_lit_index(&w);

        // Deleting the group takes its lamp out of the index; rolling the delete back (as undo
        // and failed steps do) brings it back.
        let mark = w.mark();
        w.delete(&EntityKey::Id(group), None).unwrap();
        assert_lit_index(&w);
        assert_eq!((w.lit.len(), w.flat.len()), (0, 0));
        w.rollback_to(mark);
        assert_lit_index(&w);
        assert_eq!(w.lit_entities().map(|e| e.id).collect::<Vec<_>>(), [lamp_id]);
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    /// Reviewer finding: a clone carried its cached content hash, so a copy changed through its
    /// public fields (or a replay check) could report the original's hash.
    #[test]
    fn clones_hash_their_own_fields() {
        let mut w = World::default();
        let id = put(&mut w, Shape::Cube, "c", None, Vec3::ZERO);
        let e = w.get(id).unwrap();
        let original = e.content_hash();
        let mut copy = e.clone();
        assert_eq!(copy.content_hash(), original);
        let mut moved = e.clone();
        moved.position = Vec3::new(1.0, 0.0, 0.0);
        assert_ne!(moved.content_hash(), original, "a clone is hashed afresh");
        copy.version += 7;
        assert_eq!(copy.content_hash(), original, "versions aren't content");
    }

    /// Snapshot recording (game ticks): one after-only change per entity, deletes of entities
    /// that existed, nothing for spawn-then-delete; replaying them rebuilds the world.
    #[test]
    fn snapshot_recording_keeps_one_change_per_entity() {
        let mut w = World::default();
        let keep = put(&mut w, Shape::Group, "keep", None, Vec3::ZERO);
        put(&mut w, Shape::Cube, "kid", Some("/keep"), Vec3::ZERO);
        let doomed = put(&mut w, Shape::Cube, "doomed", None, Vec3::ZERO);
        let base = w.clone();
        w.set_recording(Recording::Snapshot);
        for x in 0..5 {
            w.update(&EntityKey::Id(keep), UpdateSpec { position: Some(Vec3::new(x as f32, 0.0, 0.0)), ..Default::default() }, "g")
                .unwrap();
        }
        let tmp = put(&mut w, Shape::Sphere, "tmp", None, Vec3::ZERO);
        w.delete(&EntityKey::Id(tmp), None).unwrap();
        put(&mut w, Shape::Sphere, "new", Some("/keep"), Vec3::ZERO);
        w.delete(&EntityKey::Id(doomed), None).unwrap();
        w.set_env(&EnvPatch { exposure: Some(3.0), ..Default::default() }).unwrap();
        w.set_env(&EnvPatch { exposure: Some(4.0), ..Default::default() }).unwrap();
        assert!(w.pending_changes().is_empty(), "nothing copied while the tick runs");
        let changes = w.take_changes();
        let ops: Vec<&str> = changes.iter().map(Change::op).collect();
        assert_eq!(ops, ["set", "spawn", "delete", "env"]);
        assert!(matches!(&changes[0], Change::Pose { .. }), "a moved entity is recorded as a pose, not copied");
        let mut replayed = base;
        for c in &changes {
            match c {
                Change::Spawn { after, .. } | Change::Set { after, .. } => replayed.put_raw((**after).clone()),
                Change::Delete { before, .. } => {
                    replayed.remove_raw(before.id);
                }
                Change::Env { after, .. } => replayed.replace_env(**after),
                Change::Pose { pose, .. } => replayed.apply_pose(pose, "g"),
                Change::Camera { .. } => unreachable!("no cameras here"),
            }
        }
        replayed.replay_next_id(w.next_id());
        assert_eq!(replayed.state_hash(), w.state_hash());
        assert!(w.take_changes().is_empty());
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

    /// A model source that builds anything not named "broken": tier n is n metres tall.
    struct FakeModels;

    impl ModelSource for FakeModels {
        fn resolve(&self, spec: &ModelSpec) -> Result<ResolvedModel, WorldError> {
            if spec.recipe.contains("broken") {
                return Err(WorldError::new("recipe_invalid", "broken on purpose").with("field", "recipe"));
            }
            let tier = spec.params.iter().find(|(k, _)| k == "tier").map_or(1.0, |p| p.1) as f32;
            Ok(ResolvedModel { handle: tier as u32, state: ModelState::Ready, bounds: Some((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, tier, 0.5))) })
        }
    }

    fn with_models() -> World {
        let mut w = World::default();
        w.set_model_source(Some(Arc::new(FakeModels)));
        w
    }

    /// A finished build records only the entities whose resolution changed (draw caches patch
    /// those) instead of starting the change record again; nothing changed, nothing recorded.
    #[test]
    fn finished_builds_record_only_what_they_changed() {
        let mut w = with_models();
        let mut s = SpawnSpec::model(ModelSpec::new("recipes/a.json"));
        s.name = Some("a".into());
        let a = w.spawn(s, "t").unwrap().id;
        put(&mut w, Shape::Cube, "c", None, Vec3::ZERO);
        let since = w.draw_epoch();
        // Handle 1's build finished, but FakeModels resolves it as before: nothing to record.
        w.refresh_model_handles(&BTreeSet::from([1u32]));
        assert_eq!(w.draw_changes_since(since).map(|c| c.count()), Some(0), "the record survives an unrelated build");
        // A handle no entity uses: nothing either.
        w.refresh_model_handles(&BTreeSet::from([7u32]));
        assert_eq!(w.draw_changes_since(since).map(|c| c.count()), Some(0));
        // A change of resolution (the source now resolves the spec differently) is recorded for that entity.
        w.models = SourceSlot(Some(Arc::new(Tier2)));
        w.refresh_model_handles(&BTreeSet::from([1u32]));
        assert_eq!(w.draw_changes_since(since).unwrap().collect::<Vec<_>>(), [a]);
    }

    /// Resolves everything as handle 2 (a rebuilt spec).
    struct Tier2;

    impl ModelSource for Tier2 {
        fn resolve(&self, _: &ModelSpec) -> Result<ResolvedModel, WorldError> {
            Ok(ResolvedModel { handle: 2, state: ModelState::Ready, bounds: Some((Vec3::ZERO, Vec3::ONE)) })
        }
    }

    /// Persistent ids never reach the transient range: a counter from a replay is capped, and a
    /// spawn past it is refused (structured), while transient spawns go on.
    #[test]
    fn persistent_ids_stay_below_the_transient_range() {
        let mut w = World::default();
        w.set_next_id(u64::MAX);
        assert_eq!(w.next_id(), MAX_NEXT_ID);
        let e = w.spawn(SpawnSpec::new(Shape::Cube), "t").expect_err("refused");
        assert_eq!((e.code, e.detail("field")), ("limit", Some(&Json::from("id"))));
        let mut t = SpawnSpec::new(Shape::Cube);
        t.transient = true;
        assert!(w.spawn(t, "game").unwrap().id > TRANSIENT_ID_BASE);
        assert_eq!(w.persistent_entities().count(), 0);
        let mut w = World::default();
        w.set_next_id(MAX_NEXT_ID - 1);
        let last = w.spawn(SpawnSpec::new(Shape::Cube), "t").unwrap().id;
        assert_eq!(last, MAX_NEXT_ID, "the last persistent id is still handed out");
        assert!(w.spawn(SpawnSpec::new(Shape::Cube), "t").is_err());
    }

    #[test]
    fn anims_start_on_the_clock_and_crossfade_from_what_they_replace() {
        let mut w = with_models();
        // Only model entities animate.
        let mut cube = SpawnSpec::new(Shape::Cube);
        cube.anim = Some(AnimSpec::new("walk"));
        assert_eq!(w.spawn(cube, "t").err().unwrap().detail("field"), Some(&Json::from("anim")));
        let mut s = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        s.anim = Some(AnimSpec::new("walk"));
        let id = w.spawn(s, "t").unwrap().id;
        let a = w.get(id).unwrap().anim.clone().unwrap();
        assert_eq!(a.start, Some(0.0), "spawned: phase-locked to the clock");
        let h0 = w.get(id).unwrap().content_hash();
        // A new clip on update starts now and fades from the walk.
        w.set_clock_s(12.5);
        let mut attack = AnimSpec::new("attack");
        attack.crossfade = 0.25;
        attack.speed = 1.5;
        w.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(attack)), ..Default::default() }, "t").unwrap();
        let e = w.get(id).unwrap();
        let a = e.anim.as_ref().unwrap();
        assert_eq!((a.start, a.from.as_ref().map(|f| f.clip.as_str())), (Some(12.5), Some("walk")));
        assert_ne!(e.content_hash(), h0, "anims are state");
        assert_eq!(a.fade_weight(12.5), 0.0);
        assert_eq!(a.fade_weight(12.75), 1.0);
        assert!((a.clip_time(13.5) - 1.5).abs() < 1e-9);
        assert_eq!(a.clip_time(10.0), 0.0, "never below 0");
        // JSON round trip (world files, world_query), and errors carry the field.
        assert_eq!(&AnimSpec::from_json(&a.to_json()).unwrap(), a);
        assert_eq!(AnimSpec::from_json(&Json::from("idle")).unwrap(), AnimSpec::new("idle"));
        assert!(AnimSpec::from_json(&Json::parse(r#"{"clip":"x","sped":2}"#).unwrap()).unwrap_err().contains("sped"));
        let mut fast = AnimSpec::new("walk");
        fast.speed = 9.0;
        let err = w.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(fast)), ..Default::default() }, "t").unwrap_err();
        assert_eq!(err.detail("field"), Some(&Json::from("anim.speed")));
        // Stopping: the bind pose.
        w.update(&EntityKey::Id(id), UpdateSpec { anim: Some(None), ..Default::default() }, "t").unwrap();
        assert!(w.get(id).unwrap().anim.is_none());
    }

    #[test]
    fn model_entities_resolve_bounds_and_refuse_broken_recipes() {
        let mut w = with_models();
        let spec = ModelSpec::new("recipes/stone.json").with_param("tier", 2.0).with_seed(7);
        let id = w.spawn(SpawnSpec::model(spec.clone()), "t").unwrap().id;
        let e = w.get(id).unwrap();
        assert_eq!(e.color, [1.0; 3], "models default to white: as authored");
        assert_eq!(e.local_bounds(), Some((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5))));
        assert_eq!(e.resolved_model().unwrap().handle, 2);
        let json = e.to_json("/x");
        assert_eq!((json.get("recipe"), json.get("seed")), (Some(&Json::from("recipes/stone.json")), Some(&Json::from(7))));
        // A broken recipe is refused at spawn and changes nothing (no id used).
        let next = w.next_id();
        let err = w.spawn(SpawnSpec::model(ModelSpec::new("broken.json")), "t").err().unwrap();
        assert_eq!((err.code, w.next_id()), ("recipe_invalid", next));
        // Updates re-resolve; `seed` on a model is the model seed; a broken update changes nothing.
        let up = UpdateSpec { params: Some(vec![("tier".into(), 3.0)]), ..Default::default() };
        let e = w.update(&EntityKey::Id(id), up, "t").unwrap();
        assert_eq!(e.local_bounds().unwrap().1.y, 3.0);
        let seed = UpdateSpec { look: LookPatch { seed: Some(12.0), ..Default::default() }, ..Default::default() };
        let e = w.update(&EntityKey::Id(id), seed, "t").unwrap();
        assert_eq!((e.model.as_ref().unwrap().seed, e.look.surface.seed), (Some(12), 0.0));
        let version = e.version;
        let bad = UpdateSpec { recipe: Some("broken.json".into()), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(id), bad, "t").err().unwrap().code, "recipe_invalid");
        let frac = UpdateSpec { look: LookPatch { seed: Some(1.5), ..Default::default() }, ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(id), frac, "t").err().unwrap().detail("field"), Some(&Json::from("seed")));
        assert_eq!(w.get(id).unwrap().version, version, "rejected updates change nothing");
        // Only models take recipes; a model needs one; paths can't escape the assets root.
        let cube = put(&mut w, Shape::Cube, "c", None, Vec3::ZERO);
        let up = UpdateSpec { recipe: Some("a.json".into()), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(cube), up, "t").err().unwrap().code, "invalid_component");
        assert!(w.spawn(SpawnSpec::new(Shape::Model), "t").is_err());
        for bad in ["../up.json", "/abs.json", "a//b.json", "a/./b.json", "x.txt", "c:\\x.json", ""] {
            assert!(validate_recipe_path(bad).is_err(), "{bad}");
        }
        let unsorted = ModelSpec { recipe: "a.json".into(), params: vec![("b".into(), 1.0), ("a".into(), 1.0)], seed: None };
        assert!(unsorted.check().is_err());
        assert!(ModelSpec::new("a.json").with_seed(MAX_MODEL_SEED + 1).check().is_err());
        // The spec is part of the state; the derived bounds are not.
        let before = w.state_hash();
        let failed = w.set_model_source(None);
        assert!(failed.is_empty() && w.get(id).unwrap().local_bounds().is_none());
        assert_eq!(w.state_hash(), before);
    }

    /// Reviewer finding: a journal state put back whose spec no longer resolves kept its old
    /// handle, which may name another spec's model by then.
    #[test]
    fn put_back_states_never_keep_a_stale_model_handle() {
        let mut w = with_models();
        let id = w.spawn(SpawnSpec::model(ModelSpec::new("recipes/ok.json").with_param("tier", 3.0)), "t").unwrap().id;
        let mut old = w.get(id).unwrap().clone();
        assert_eq!(old.resolved_model().unwrap().handle, 3);
        old.model = Some(ModelSpec::new("recipes/broken.json"));
        w.put_raw(old);
        let e = w.get(id).unwrap();
        assert_eq!((e.resolved_model(), e.model_state(), e.local_bounds()), (None, "unresolved", None));
        assert_eq!(e.to_json("/x").get("model_state"), Some(&Json::from("unresolved")));
    }

    #[test]
    fn hidden_entities_and_their_subtrees_are_not_shown() {
        let mut w = World::default();
        let g = put(&mut w, Shape::Group, "g", None, Vec3::ZERO);
        let kid = put(&mut w, Shape::Cube, "kid", Some("/g"), Vec3::new(10.0, 0.0, 0.0));
        put(&mut w, Shape::Cube, "other", None, Vec3::ZERO);
        let all = w.bounds().unwrap();
        let hash = w.state_hash();
        let hide = |v: bool| UpdateSpec { visible: Some(v), ..Default::default() };
        w.set_recording(Recording::Full);
        w.update(&EntityKey::Id(g), hide(false), "t").unwrap();
        assert_eq!(w.take_changes().len(), 1, "journaled like any change");
        assert!(!w.get(kid).unwrap().shown(), "children of a hidden entity are hidden");
        assert!(w.get(kid).unwrap().visible, "their own flag stays");
        assert!(w.bounds().unwrap().1.x < 1.0, "hidden entities leave the world's bounds: {:?}", w.bounds());
        assert_ne!(w.state_hash(), hash, "visibility is state");
        // A new child of a hidden parent is hidden too; showing the parent shows the subtree.
        let late = put(&mut w, Shape::Cube, "late", Some("/g"), Vec3::ZERO);
        assert!(!w.get(late).unwrap().shown());
        w.update(&EntityKey::Id(g), hide(true), "t").unwrap();
        assert!(w.get(kid).unwrap().shown() && w.get(late).unwrap().shown());
        assert_eq!(w.bounds().unwrap(), all);
        assert_eq!(w.get(g).unwrap().to_json("/g").get("visible"), None, "defaults stay out of listings");
    }

    #[test]
    fn ui_layer_is_inherited_and_left_out_of_bounds() {
        let mut w = World::default();
        let mut ui = SpawnSpec::new(Shape::Group);
        ui.name = Some("ui".into());
        ui.layer = Layer::Ui;
        w.spawn(ui, "game").unwrap();
        let cursor = put(&mut w, Shape::Cube, "cursor", Some("/ui"), Vec3::new(0.0, -50.0, 0.0));
        put(&mut w, Shape::Cube, "stone", None, Vec3::ZERO);
        assert!(w.get(cursor).unwrap().in_ui() && w.get(cursor).unwrap().layer == Layer::World);
        assert!(w.bounds().unwrap().0.y > -1.0, "the parked cursor doesn't stretch the auto camera");
        // Moving it out of the ui group puts it back in the world.
        w.update(&EntityKey::Id(cursor), UpdateSpec { parent: Some(None), ..Default::default() }, "t").unwrap();
        assert!(!w.get(cursor).unwrap().in_ui());
        assert!(w.bounds().unwrap().0.y < -40.0);
    }

    #[test]
    fn transient_entities_never_touch_saved_state() {
        let mut w = World::default();
        let base = put(&mut w, Shape::Group, "enemies", None, Vec3::ZERO);
        w.set_recording(Recording::Full);
        w.take_changes();
        let (revision, next_id, hash, stamp) = (w.revision(), w.next_id(), w.state_hash(), w.subtree_stamp(base));
        let mut s = SpawnSpec::new(Shape::Sphere);
        s.name = Some("shambler".into());
        s.parent = Some(EntityKey::Id(base));
        s.transient = true;
        let id = w.spawn(s, "game").unwrap().id;
        assert!(id > TRANSIENT_ID_BASE, "transient ids come from their own range");
        let kid = put(&mut w, Shape::Cube, "eye", Some("/enemies/shambler"), Vec3::ZERO);
        assert!(w.get(kid).unwrap().transient, "children of transient entities are transient");
        for x in 0..10 {
            let mv = UpdateSpec { position: Some(Vec3::new(x as f32, 0.0, 0.0)), ..Default::default() };
            w.update(&EntityKey::Id(id), mv, "game").unwrap();
        }
        assert!(w.take_changes().is_empty(), "never journaled");
        assert_eq!((w.revision(), w.next_id(), w.state_hash(), w.subtree_stamp(base)), (revision, next_id, hash, stamp));
        assert_eq!((w.len(), w.persistent_len(), w.transient_len()), (3, 1, 2));
        // Visible to queries and sight like anything else.
        let page = w.query(&Query { path_prefix: Some("/enemies".into()), limit: 10, ..Default::default() }).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(w.get(id).unwrap().to_json("/enemies/shambler").get("transient"), Some(&Json::from(true)));
        // Saved entities can't hide under transient ones (they would vanish from the files).
        let saved = put(&mut w, Shape::Cube, "loot", None, Vec3::ZERO);
        let under = UpdateSpec { parent: Some(Some(EntityKey::Id(id))), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(saved), under, "t").err().unwrap().code, "transient_parent");
        w.take_changes();
        // Snapshot recording (game ticks) costs nothing for them; deleting them is invisible too.
        w.set_recording(Recording::Snapshot);
        let revision = w.revision();
        w.update(&EntityKey::Id(id), UpdateSpec { scale: Some(Vec3::ONE * 2.0), ..Default::default() }, "game").unwrap();
        w.delete(&EntityKey::Id(id), None).unwrap();
        assert!(w.dirty.is_empty() && w.dirty_removed.is_empty());
        assert!(w.take_changes().is_empty());
        assert_eq!((w.revision(), w.transient_len()), (revision, 0));
        // A failed batch takes its transient spawns back with it.
        let mut t = SpawnSpec::new(Shape::Cube);
        t.transient = true;
        let ops = vec![Op::Spawn(t), Op::Delete(EntityKey::Path("/nope".into()), None)];
        assert!(w.apply_batch(ops, "t").is_err());
        assert_eq!(w.transient_len(), 0);
        assert!(w.restore(SpawnSpec { transient: true, ..SpawnSpec::new(Shape::Cube) }, 50, 1, "t", "t").is_err());
    }

    #[test]
    fn moving_a_leaf_allocates_nothing_but_subtrees_still_follow() {
        let mut w = World::default();
        let g = put(&mut w, Shape::Group, "g", None, Vec3::ZERO);
        let kid = put(&mut w, Shape::Cube, "kid", Some("/g"), Vec3::new(1.0, 0.0, 0.0));
        let deep = put(&mut w, Shape::Cube, "deep", Some("/g/kid"), Vec3::new(1.0, 0.0, 0.0));
        w.update(&EntityKey::Id(g), UpdateSpec { position: Some(Vec3::new(5.0, 0.0, 0.0)), ..Default::default() }, "t").unwrap();
        assert!(close(w.get(deep).unwrap().world_position(), Vec3::new(7.0, 0.0, 0.0)));
        w.update(&EntityKey::Id(kid), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }, "t").unwrap();
        assert!(close(w.get(deep).unwrap().world_position(), Vec3::new(6.0, 0.0, 0.0)));
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
