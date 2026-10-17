//! The character controller behind the SDK's `move_character`.
//!
//! [`CharacterMover`] is the interface; [`Kinematic`] is a minimal placeholder until the physics
//! lane's kinematic character controller (P2) takes its place through
//! [`super::LogicHost::set_mover`]. Modules never see the difference: they ask to move by a
//! displacement and get back where they ended up and whether they stand on something.

use crate::world::{ColliderShape, EntityKey, UpdateSpec, World, WorldError};
use specula_core::math::Vec3;

/// What a move did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    /// The character's new local position (also written to its entity).
    pub position: Vec3,
    pub grounded: bool,
    /// Part of the horizontal motion was stopped by an obstacle.
    pub blocked: bool,
}

/// Moves characters through the world (deterministically: F2).
pub trait CharacterMover {
    /// Shown in `logic_status`.
    fn name(&self) -> &str;

    /// Moves entity `id` by `motion` (metres, world axes, gravity included) and sets its
    /// position, attributing the change to `by`.
    fn move_character(&mut self, world: &mut World, id: u64, motion: Vec3, by: &str) -> Result<Moved, WorldError>;
}

/// Highest step a character walks up without jumping.
pub const STEP_HEIGHT: f32 = 0.35;
/// Below this height a falling character stops (there is nothing under it).
pub const FLOOR_Y: f32 = -100.0;

/// The placeholder controller: a vertical capsule (the entity's capsule collider, else 0.3 m ×
/// 1.8 m, feet at the entity's origin) against the world-space boxes of every other entity's
/// non-trigger collider. It lands on tops (stepping up to [`STEP_HEIGHT`]), stops against sides
/// (sliding along them, one axis at a time) and has no ceilings, slopes or pushing. Motion is
/// applied to the local position as a world displacement, so parents must not rotate or scale.
/// Cost is linear in the number of colliders; physics brings a broadphase.
#[derive(Clone, Copy, Debug, Default)]
pub struct Kinematic;

/// A world-space box: `(min, max)`.
type Aabb = (Vec3, Vec3);

impl Kinematic {
    fn obstacles(world: &World, me: u64) -> Vec<Aabb> {
        let mut out = Vec::new();
        for e in world.entities() {
            let Some(c) = &e.collider else { continue };
            if e.id == me || c.is_trigger || !e.shown() {
                continue;
            }
            let half = match c.shape {
                ColliderShape::Box { size } => size * 0.5,
                ColliderShape::Sphere { radius } => Vec3::new(radius, radius, radius),
                ColliderShape::Capsule { radius, height } => Vec3::new(radius, height * 0.5, radius),
            };
            let m = e.model();
            let mut lo = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
            let mut hi = -lo;
            for i in 0..8 {
                let corner = Vec3::new(
                    c.offset.x + if i & 1 == 0 { -half.x } else { half.x },
                    c.offset.y + if i & 2 == 0 { -half.y } else { half.y },
                    c.offset.z + if i & 4 == 0 { -half.z } else { half.z },
                );
                let p = m.transform_point(corner);
                lo = lo.min(p);
                hi = hi.max(p);
            }
            out.push((lo, hi));
        }
        out
    }
}

/// Whether a circle of `r` around `(x, z)` touches the box's footprint.
fn footprint_hits(b: &Aabb, x: f32, z: f32, r: f32) -> bool {
    let dx = x - x.clamp(b.0.x, b.1.x);
    let dz = z - z.clamp(b.0.z, b.1.z);
    dx * dx + dz * dz < r * r
}

impl CharacterMover for Kinematic {
    fn name(&self) -> &str {
        "kinematic placeholder (until physics P2)"
    }

    fn move_character(&mut self, world: &mut World, id: u64, motion: Vec3, by: &str) -> Result<Moved, WorldError> {
        if ![motion.x, motion.y, motion.z].iter().all(|v| v.is_finite() && v.abs() <= 100.0) {
            return Err(WorldError::new("invalid_field", "'motion' must be three finite numbers within ±100 m per tick")
                .with("field", "motion"));
        }
        let e = world.get(id).ok_or_else(|| WorldError::new("not_found", format!("no entity with id {id}")).with("id", id))?;
        let (radius, height) = match e.collider.as_ref().map(|c| c.shape) {
            Some(ColliderShape::Capsule { radius, height }) => (radius * e.scale.x.abs(), height * e.scale.y.abs()),
            _ => (0.3, 1.8),
        };
        let feet = e.world_position();
        let local = e.position;
        let boxes = Kinematic::obstacles(world, id);
        // Walls: boxes that reach above a step and below the head.
        let blocks = |x: f32, z: f32, y: f32| {
            boxes.iter().any(|b| b.1.y > y + STEP_HEIGHT && b.0.y < y + height && footprint_hits(b, x, z, radius))
        };
        let (mut x, mut z) = (feet.x + motion.x, feet.z + motion.z);
        let mut blocked = false;
        if (motion.x != 0.0 || motion.z != 0.0) && blocks(x, z, feet.y) {
            blocked = true;
            // Slide: keep whichever single axis is free (x first, for a fixed order).
            if !blocks(feet.x + motion.x, feet.z, feet.y) {
                z = feet.z;
            } else if !blocks(feet.x, feet.z + motion.z, feet.y) {
                x = feet.x;
            } else {
                (x, z) = (feet.x, feet.z);
            }
        }
        // Ground: the highest top under the feet that is at most a step above them.
        let support = boxes
            .iter()
            .filter(|b| b.1.y <= feet.y + STEP_HEIGHT && footprint_hits(b, x, z, radius * 0.5))
            .map(|b| b.1.y)
            .fold(FLOOR_Y, f32::max);
        let mut y = feet.y + motion.y;
        let mut grounded = false;
        if motion.y <= 0.0 && y <= support {
            y = support;
            grounded = true;
        }
        let new_local = local + Vec3::new(x - feet.x, y - feet.y, z - feet.z);
        if new_local != local {
            let change = UpdateSpec { position: Some(new_local), ..UpdateSpec::default() };
            world.update(&EntityKey::Id(id), change, by)?;
        }
        Ok(Moved { position: new_local, grounded, blocked })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Collider, Shape, SpawnSpec};

    fn boxed(world: &mut World, name: &str, pos: Vec3, scale: Vec3) -> u64 {
        let spec = SpawnSpec {
            name: Some(name.into()),
            position: pos,
            scale,
            collider: Some(Collider { shape: ColliderShape::Box { size: Vec3::ONE }, offset: Vec3::ZERO, is_trigger: false, layer: "default".into() }),
            ..SpawnSpec::new(Shape::Cube)
        };
        world.spawn(spec, "t").unwrap().id
    }

    #[test]
    fn lands_steps_and_stops_at_walls() {
        let mut w = World::default();
        boxed(&mut w, "ground", Vec3::new(0.0, -0.5, 0.0), Vec3::new(20.0, 1.0, 20.0));
        boxed(&mut w, "step", Vec3::new(2.0, 0.1, 0.0), Vec3::new(1.0, 0.2, 4.0));
        boxed(&mut w, "wall", Vec3::new(0.0, 1.0, -3.0), Vec3::new(10.0, 2.0, 0.5));
        let me = w.spawn(SpawnSpec { name: Some("me".into()), position: Vec3::new(0.0, 2.0, 0.0), ..SpawnSpec::new(Shape::Sphere) }, "t").unwrap().id;
        let mut k = Kinematic;
        // Falls and lands on the ground's top (y = 0).
        let mut m = Moved::default();
        for _ in 0..30 {
            m = k.move_character(&mut w, me, Vec3::new(0.0, -0.2, 0.0), "t").unwrap();
        }
        assert!(m.grounded && m.position.y == 0.0, "{m:?}");
        // Walks onto the 0.2 m step.
        for _ in 0..20 {
            m = k.move_character(&mut w, me, Vec3::new(0.1, -0.05, 0.0), "t").unwrap();
        }
        assert!(m.grounded && (m.position.y - 0.2).abs() < 1e-6 && m.position.x > 1.6, "{m:?}");
        // Walks into the wall and stops before it, sliding along x.
        for _ in 0..40 {
            m = k.move_character(&mut w, me, Vec3::new(0.05, -0.05, -0.1), "t").unwrap();
        }
        assert!(m.blocked && m.position.z > -3.0 + 0.25, "{m:?}");
        assert!(k.move_character(&mut w, me, Vec3::new(f32::NAN, 0.0, 0.0), "t").is_err());
        assert_eq!(k.move_character(&mut w, 999, Vec3::ZERO, "t").unwrap_err().code, "not_found");
    }
}
