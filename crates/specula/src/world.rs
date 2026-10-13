//! The live world: entities agents spawn and the engine renders.
//!
//! M0 keeps this deliberately small — a flat list of shapes with transforms, colours and looks,
//! each attributed to the agent that created it. M1 replaces it with the command/journal/claims
//! model.

use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::Json;
use specula_gpu::{Environment, Material, Surface};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Cube,
    /// 1×1 plane in XZ (normal +Y) — floors and ground.
    Quad,
    Sphere,
    /// A profile spun around +Y (see [`Profile`]).
    Lathe,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Cube, Shape::Quad, Shape::Sphere, Shape::Lathe];

    pub fn name(self) -> &'static str {
        match self {
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

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: u64,
    pub name: String,
    pub shape: Shape,
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
    /// Client name of the agent session that spawned it.
    pub created_by: String,
}

impl Entity {
    pub fn model(&self) -> Mat4 {
        let r = self.rotation_deg;
        Mat4::from_trs(self.position, Quat::from_euler_xyz_deg(r.x, r.y, r.z), self.scale)
    }

    /// Object-space bounding box.
    pub fn local_bounds(&self) -> (Vec3, Vec3) {
        match (self.shape, &self.profile) {
            (Shape::Lathe, Some(p)) => p.bounds(),
            (Shape::Quad, _) => (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.0, 0.5)),
            _ => (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)),
        }
    }

    /// World-space corners of the oriented bounding box.
    pub fn world_corners(&self) -> [Vec3; 8] {
        let (lo, hi) = self.local_bounds();
        let m = self.model();
        let mut out = [Vec3::ZERO; 8];
        for (i, slot) in out.iter_mut().enumerate() {
            let corner = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            *slot = m.transform_point(corner);
        }
        out
    }

    /// The entity's point light in world space, if it carries one.
    pub fn world_light(&self) -> Option<specula_gpu::PointLight> {
        let l = self.light?;
        let p = self.model().transform_point(l.offset);
        Some(specula_gpu::PointLight {
            position: [p.x, p.y, p.z],
            color: l.color.unwrap_or(self.color),
            intensity: l.intensity,
            range: l.range,
        })
    }

    pub fn to_json(&self) -> Json {
        let mut fields = vec![
            ("id", Json::from(self.id)),
            ("name", Json::from(self.name.as_str())),
            ("shape", Json::from(self.shape.name())),
            ("position", vec3_json(self.position)),
            ("rotation_deg", vec3_json(self.rotation_deg)),
            ("scale", vec3_json(self.scale)),
            ("color", Json::from(color_hex(self.color))),
        ];
        self.look.to_json_fields(&mut fields);
        if let Some(l) = self.light {
            fields.push(("light", l.to_json()));
        }
        if let Some(p) = &self.profile {
            fields.push(("profile", p.to_json()));
        }
        fields.push(("created_by", Json::from(self.created_by.as_str())));
        Json::obj(fields)
    }
}

pub struct SpawnSpec {
    pub shape: Shape,
    pub name: Option<String>,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
    pub color: [f32; 3],
    pub look: Look,
    pub light: Option<LightSpec>,
    /// Required for [`Shape::Lathe`], refused for other shapes (checked by the tool parser).
    pub profile: Option<Profile>,
}

impl SpawnSpec {
    /// A light-grey matte `shape` at the origin.
    pub fn new(shape: Shape) -> SpawnSpec {
        SpawnSpec {
            shape,
            name: None,
            position: Vec3::ZERO,
            rotation_deg: Vec3::ZERO,
            scale: Vec3::ONE,
            color: [0.8, 0.8, 0.8],
            look: Look::default(),
            light: None,
            profile: None,
        }
    }
}

/// Fields to change in [`World::update`]; `None` leaves a field as it is.
#[derive(Default)]
pub struct UpdateSpec {
    pub name: Option<String>,
    pub position: Option<Vec3>,
    pub rotation_deg: Option<Vec3>,
    pub scale: Option<Vec3>,
    pub color: Option<[f32; 3]>,
    pub look: LookPatch,
    /// `Some(None)` removes the light.
    pub light: Option<Option<LightSpec>>,
    /// Lathe entities only.
    pub profile: Option<Profile>,
}

#[derive(Default)]
pub struct World {
    entities: Vec<Entity>,
    next_id: u64,
    pub env: Env,
}

impl World {
    pub fn spawn(&mut self, spec: SpawnSpec, created_by: &str) -> Result<&Entity, String> {
        if (spec.shape == Shape::Lathe) != spec.profile.is_some() {
            return Err("a 'profile' is required for shape 'lathe' and only allowed for it".into());
        }
        // Ids are only used up by spawns that succeed.
        let id = self.next_id + 1;
        let name = spec.name.unwrap_or_else(|| format!("{}_{id}", spec.shape.name()));
        if self.entities.iter().any(|e| e.name == name) {
            return Err(format!("an entity named '{name}' already exists; names must be unique"));
        }
        self.next_id = id;
        self.entities.push(Entity {
            id,
            name,
            shape: spec.shape,
            position: spec.position,
            rotation_deg: spec.rotation_deg,
            scale: spec.scale,
            color: spec.color,
            look: spec.look,
            light: spec.light,
            profile: spec.profile,
            created_by: created_by.to_owned(),
        });
        Ok(self.entities.last().expect("just pushed"))
    }

    /// Change some properties of an existing entity; its id never changes.
    pub fn update(&mut self, key: &EntityKey, change: UpdateSpec) -> Result<&Entity, String> {
        if let Some(name) = &change.name {
            if self.entities.iter().any(|e| e.name == *name && !key.matches(e)) {
                return Err(format!("an entity named '{name}' already exists; names must be unique"));
            }
        }
        let e = self.entities.iter_mut().find(|e| key.matches(e)).ok_or("no such entity")?;
        if change.profile.is_some() && e.shape != Shape::Lathe {
            return Err(format!("'{}' is a {}; only lathe entities have a profile", e.name, e.shape.name()));
        }
        if let Some(name) = change.name {
            e.name = name;
        }
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
        Ok(e)
    }

    /// Remove by id or name.
    pub fn delete(&mut self, key: &EntityKey) -> Option<Entity> {
        let i = self.entities.iter().position(|e| key.matches(e))?;
        Some(self.entities.remove(i))
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// World-space bounds of everything, or `None` when empty.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut corners = self.entities.iter().flat_map(|e| e.world_corners());
        let first = corners.next()?;
        Some(corners.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }
}

pub enum EntityKey {
    Id(u64),
    Name(String),
}

impl EntityKey {
    fn matches(&self, e: &Entity) -> bool {
        match self {
            EntityKey::Id(id) => e.id == *id,
            EntityKey::Name(name) => e.name == *name,
        }
    }
}

pub fn vec3_json(v: Vec3) -> Json {
    Json::arr([Json::from(v.x), Json::from(v.y), Json::from(v.z)])
}

pub fn color_hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lathe() -> Profile {
        Profile { points: vec![[0.5, 0.0], [0.3, 2.0]], segments: 12 }
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
        assert_eq!(e.local_bounds(), (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5)));
        let id = e.id;
        let change = UpdateSpec { profile: Some(Profile { segments: 8, ..lathe() }), ..Default::default() };
        assert_eq!(w.update(&EntityKey::Id(id), change).unwrap().profile.as_ref().unwrap().segments, 8);
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
        let light = w.entities()[0].world_light().unwrap();
        assert_eq!((light.position, light.color), ([0.0, 0.5, 0.0], [1.0, 0.5, 0.0]));
        let look = LookPatch { material: Some(Material::Glass), opacity: Some(0.3), ..Default::default() };
        let e = w.update(&EntityKey::Id(id), UpdateSpec { look, light: Some(None), ..Default::default() }).unwrap();
        assert_eq!((e.look.material, e.look.opacity, e.light), (Material::Glass, 0.3, None));
        let json = e.to_json();
        assert_eq!(json.get("material").and_then(Json::as_str), Some("glass"));
        assert!(json.get("emissive").is_none(), "defaults are left out");
    }
}
