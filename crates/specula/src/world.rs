//! The live world: entities agents spawn and the engine renders.
//!
//! M0 keeps this deliberately small — a flat list of shapes with transforms and colours, each
//! attributed to the agent that created it. M1 replaces it with the command/journal/claims model.

use specula_core::math::{Mat4, Quat, Vec3};
use specula_core::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Cube,
    /// 1×1 plane in XZ (normal +Y) — floors and ground.
    Quad,
    Sphere,
}

impl Shape {
    pub const ALL: [Shape; 3] = [Shape::Cube, Shape::Quad, Shape::Sphere];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Cube => "cube",
            Shape::Quad => "quad",
            Shape::Sphere => "sphere",
        }
    }

    pub fn parse(s: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|shape| shape.name() == s)
    }

    /// Object-space bounding box.
    pub fn local_bounds(self) -> (Vec3, Vec3) {
        match self {
            Shape::Cube | Shape::Sphere => (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)),
            Shape::Quad => (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 0.0, 0.5)),
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
    /// Client name of the agent session that spawned it.
    pub created_by: String,
}

impl Entity {
    pub fn model(&self) -> Mat4 {
        let r = self.rotation_deg;
        Mat4::from_trs(self.position, Quat::from_euler_xyz_deg(r.x, r.y, r.z), self.scale)
    }

    /// World-space corners of the oriented bounding box.
    pub fn world_corners(&self) -> [Vec3; 8] {
        let (lo, hi) = self.shape.local_bounds();
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

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("id", Json::from(self.id)),
            ("name", Json::from(self.name.as_str())),
            ("shape", Json::from(self.shape.name())),
            ("position", vec3_json(self.position)),
            ("rotation_deg", vec3_json(self.rotation_deg)),
            ("scale", vec3_json(self.scale)),
            ("color", Json::from(color_hex(self.color))),
            ("created_by", Json::from(self.created_by.as_str())),
        ])
    }
}

pub struct SpawnSpec {
    pub shape: Shape,
    pub name: Option<String>,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
    pub color: [f32; 3],
}

#[derive(Default)]
pub struct World {
    entities: Vec<Entity>,
    next_id: u64,
}

impl World {
    pub fn spawn(&mut self, spec: SpawnSpec, created_by: &str) -> Result<&Entity, String> {
        self.next_id += 1;
        let id = self.next_id;
        let name = spec.name.unwrap_or_else(|| format!("{}_{id}", spec.shape.name()));
        if self.entities.iter().any(|e| e.name == name) {
            return Err(format!("an entity named '{name}' already exists; names must be unique"));
        }
        self.entities.push(Entity {
            id,
            name,
            shape: spec.shape,
            position: spec.position,
            rotation_deg: spec.rotation_deg,
            scale: spec.scale,
            color: spec.color,
            created_by: created_by.to_owned(),
        });
        Ok(self.entities.last().expect("just pushed"))
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
