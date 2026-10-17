//! The physics character controller behind modules' `move_character` (P2 through
//! [`crate::logic::LogicHost::set_mover`]; docs/design/MODULES.md).
//!
//! The SDK moves a character by a **displacement with gravity included** (modules own their
//! vertical speed) and gets back where it ended up and whether it stands on something. P2 is
//! velocity-based and walks kinematic bodies in the space, so this adapter plans each move
//! instead of making it:
//!
//! - **The space is lent, not shared.** The engine owns its physics by value; around module
//!   calls it moves it into this mover ([`crate::Engine::lend_physics`]) and takes it back after
//!   ([`crate::Engine::return_physics`]). Each move first syncs the space with the world (what
//!   the module did earlier this tick is there).
//! - **Nothing outside the world changes.** A module call that fails is rolled back by the
//!   world, but not by the space, so moves never touch the space: the controller plans the move
//!   ([`specula_physics::Space::plan_character`]) from what is underfoot now
//!   ([`specula_physics::Space::character_ground`], so it keeps no memory of its own) and the
//!   result is written as world changes: the character's new position, and the velocity
//!   changes of the dynamic bodies it pushed (as `physics_impulse` writes them). A rolled-back
//!   call takes all of that with it; physics picks the rest up at its next sync.
//! - The character is the entity's own collider (convex, not a trigger), else a 0.3 × 1.8 m
//!   capsule standing on the entity's origin (like the placeholder); `body.character` tunes the
//!   controller (step height, slope, snapping, pushing) when the entity has one. Gravity is the
//!   module's (in the displacement), never added here.
//!
//! It walks up steps, keeps to walkable slopes, snaps down stairs, slides along walls and
//! rides platforms, everything P2 does, without module changes.

use super::*;
use crate::logic::character::{CharacterMover, Kinematic, Moved};
use specula_physics::{CharacterBody, CharacterConfig};

/// The physics P2 controller for modules (see the module docs).
#[derive(Default)]
pub struct PhysicsMover {
    /// The engine's physics while a module call runs (`None`: lent nowhere; the placeholder
    /// moves characters instead).
    pub physics: Option<Physics>,
    /// The world generation the physics belongs to, and the tick length (s).
    pub generation: u64,
    pub dt: f32,
}

/// The placeholder's capsule (radius, total height) for entities without a collider.
const DEFAULT_CAPSULE: (f32, f32) = (0.3, 1.8);

impl CharacterMover for PhysicsMover {
    fn name(&self) -> &str {
        "physics P2 character controller (sweep and slide, steps, slopes, snapping, platforms, pushing)"
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn move_character(&mut self, world: &mut World, id: u64, motion: Vec3, by: &str) -> Result<Moved, WorldError> {
        let (generation, dt) = (self.generation, if self.dt > 0.0 { self.dt } else { 1.0 / 30.0 });
        let Some(physics) = self.physics.as_mut() else {
            return Kinematic.move_character(world, id, motion, by);
        };
        if ![motion.x, motion.y, motion.z].iter().all(|v| v.is_finite() && v.abs() <= 100.0) {
            return Err(WorldError::new("invalid_field", "'motion' must be three finite numbers within ±100 m per tick").with("field", "motion"));
        }
        let e = world.get(id).ok_or_else(|| WorldError::new("not_found", format!("no entity with id {id}")).with("id", id))?;
        // What the module did so far this tick reaches the space first.
        physics.sync(world, generation);
        let e_body = e.body;
        let (pose, scale) = world_pose(world, e);
        // The character's shape: its own convex collider, else the placeholder's capsule.
        let own = e.collider.as_ref().filter(|c| !c.is_trigger && !c.shape.static_only());
        let (shape, offset, mask) = match own {
            Some(c) => {
                let shape = physics.shape_for(c, scale).map_err(|m| WorldError::new("invalid_component", format!("the character's collider: {m}")).with("field", "collider"))?;
                let mask = physics.mask_bits(c.mask.as_deref()).map_err(|m| WorldError::new("invalid_component", m).with("field", "collider.mask"))?;
                (shape, Pose::at(mul3(c.offset, scale)), mask)
            }
            None => {
                let (r, h) = DEFAULT_CAPSULE;
                (Shape::capsule(r, h * 0.5 - r), Pose::at(Vec3::new(0.0, h * 0.5, 0.0)), u64::MAX)
            }
        };
        let cfg = CharacterConfig { gravity: false, ..e_body.and_then(|b| b.character).map_or_else(CharacterConfig::default, |c| c.config()) };
        let c = CharacterBody { shape: &shape, offset, pose, mask, exclude: Some(id) };
        let space = &physics.space;
        let prev = space.character_ground(&c, &cfg);
        let mv = space.plan_character(&c, motion * (1.0 / dt), &cfg, dt, prev);
        // Pushes: velocity changes of the dynamic bodies it walked into, as world changes.
        let pushes: Vec<(u64, Vec3, Vec3)> = mv
            .pushes
            .iter()
            .filter(|(k, _, _)| *k < joints::SYNTHETIC)
            .filter_map(|(k, imp, at)| space.impulse_delta(*k, *imp, Some(*at), Vec3::ZERO).map(|(dv, dw)| (*k, dv, dw)))
            .collect();
        for (k, dv, dw) in pushes {
            let Some(b) = world.get(k).and_then(|e| e.body).filter(|b| b.kind == BodyKind::Dynamic) else { continue };
            let pushed = Body { velocity: b.velocity + dv, angular_velocity: b.angular_velocity + dw, sleeping: false, ..b };
            world.update(&EntityKey::Id(k), UpdateSpec { body: Some(Some(pushed)), ..UpdateSpec::default() }, by)?;
        }
        // The new position, in the parent's frame.
        let e = world.get(id).expect("checked");
        let local = match e.parent.and_then(|p| world.get(p)) {
            Some(parent) => parent.model().inverse().unwrap_or(specula_core::math::Mat4::IDENTITY).transform_point(mv.position),
            None => mv.position,
        };
        let old = e.position;
        let bits = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
        if bits(local) != bits(old) {
            world.update(&EntityKey::Id(id), UpdateSpec { position: Some(local), ..UpdateSpec::default() }, by)?;
        }
        Ok(Moved { position: local, grounded: mv.grounded, blocked: mv.blocked })
    }
}

impl crate::Engine {
    /// Lends the physics to the module controller for the module calls that follow.
    pub(crate) fn lend_physics(&mut self) {
        let (generation, dt) = (self.world_generation, 1.0 / self.clock.hz().max(1) as f32);
        if let Some(m) = self.logic.mover_mut().as_any_mut().downcast_mut::<PhysicsMover>() {
            m.physics = Some(std::mem::take(&mut self.physics));
            m.generation = generation;
            m.dt = dt;
        }
    }

    /// Takes the physics back after module calls.
    pub(crate) fn return_physics(&mut self) {
        if let Some(m) = self.logic.mover_mut().as_any_mut().downcast_mut::<PhysicsMover>() {
            if let Some(p) = m.physics.take() {
                self.physics = p;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::{LogicHost, Origin};
    use crate::world::{Shape as WShape, SpawnSpec};
    use specula_logic::ModuleCode;
    use specula_sdk::{Cx, Module, ModuleInfo, Patch, SdkError, Spawn, Value as SValue, Vec3 as SVec3};

    const HZ: u32 = 30;

    /// Ambulo's walk (games/ambulo): walks +X at 2 m/s with its own gravity, through
    /// `move_character`, keeping its vertical speed in props.
    #[derive(Default)]
    struct Walker;
    impl Module for Walker {
        fn describe(&self) -> ModuleInfo {
            ModuleInfo { name: "walker", version: "1", ..ModuleInfo::default() }
        }
        fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
            if cx.find("/hero")?.is_none() {
                cx.spawn(
                    Spawn::new("group")
                        .name("hero")
                        .position(SVec3::new(-3.0, 0.0, 0.0))
                        .props(SValue::map().with("vy", 0.0f32))
                        .field("collider", SValue::map().with("shape", "capsule").with("radius", 0.3f32).with("height", 1.8f32).with("offset", SVec3::new(0.0, 0.9, 0.0))),
                )?;
            }
            Ok(())
        }
        fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
            let id = cx.find("/hero")?.expect("hero");
            let me = cx.entity(id)?.expect("hero");
            let dt = cx.dt();
            let vy = (me.prop("vy").as_f32().unwrap_or(0.0) - 18.0 * dt).max(-30.0);
            let moved = cx.move_character(id, SVec3::new(2.0 * dt, vy * dt, 0.0))?;
            let vy = if moved.grounded && vy < 0.0 { 0.0 } else { vy };
            cx.set(id, Patch::new().props(SValue::map().with("vy", vy).with("grounded", moved.grounded)))?;
            Ok(())
        }
    }

    /// Walks the hero into a crate, then fails: the host rolls the call back.
    #[derive(Default)]
    struct Clumsy;
    impl Module for Clumsy {
        fn describe(&self) -> ModuleInfo {
            ModuleInfo { name: "walker", version: "1", ..ModuleInfo::default() }
        }
        fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
            Walker.start(cx)
        }
        fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
            let id = cx.find("/hero")?.expect("hero");
            cx.move_character(id, SVec3::new(0.4, -0.01, 0.0))?;
            Err(SdkError::new("oops", "tripped"))
        }
    }

    #[test]
    fn rolled_back_calls_leave_nothing_behind() {
        let mut w = World::default();
        solid(&mut w, "floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 10.0), Vec3::ZERO);
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("crate".into());
        s.position = Vec3::new(-2.0, 0.5, 0.0);
        s.collider = Some(Collider::default());
        s.body = Some(Body::new(BodyKind::Dynamic, 5.0));
        let crate_id = w.spawn(s, "t").unwrap().id;
        let mut host = LogicHost::new();
        host.set_mover(Box::new(PhysicsMover { physics: Some(Physics::default()), generation: 0, dt: 1.0 / HZ as f32 }));
        let mut commit = crate::logic::tools::no_journal();
        host.load(ModuleCode::of::<Clumsy>(), Origin::default(), None, &mut w, 0, HZ, &mut commit).unwrap();
        let hero = w.find_path("/hero").unwrap();
        let (at, crate_v) = (w.get(hero).unwrap().position, w.get(crate_id).unwrap().version);
        for t in 1..=10 {
            host.tick(&mut w, t, HZ, &mut commit);
        }
        assert_eq!(w.get(hero).unwrap().position, at, "a failed call's move is rolled back");
        assert_eq!(w.get(crate_id).unwrap().version, crate_v, "and so are its pushes (they are world changes)");
        // The space never moved either: physics steps the crate exactly as a fresh space would.
        let mut control = w.clone();
        let mut p = host.mover_mut().as_any_mut().downcast_mut::<PhysicsMover>().unwrap().physics.take().unwrap();
        let settings = w.env().physics;
        p.tick(&mut w, 0, &settings, 11, 1.0 / HZ as f32);
        Physics::default().tick(&mut control, 0, &settings, 11, 1.0 / HZ as f32);
        assert_eq!(w.get(crate_id).unwrap().body, control.get(crate_id).unwrap().body);
        assert_eq!(w.get(crate_id).unwrap().position, control.get(crate_id).unwrap().position);
    }

    fn solid(w: &mut World, name: &str, at: Vec3, scale: Vec3, rot: Vec3) {
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some(name.into());
        s.position = at;
        s.scale = scale;
        s.rotation_deg = rot;
        s.collider = Some(Collider::default());
        w.spawn(s, "t").unwrap();
    }

    fn run(world: &mut World, physics: Physics, ticks: u64, mut each: impl FnMut(&World, u64)) -> (Physics, LogicHost) {
        let mut host = LogicHost::new();
        host.set_mover(Box::new(PhysicsMover { physics: Some(physics), generation: 0, dt: 1.0 / HZ as f32 }));
        let mut commit = crate::logic::tools::no_journal();
        host.load(ModuleCode::of::<Walker>(), Origin::default(), None, world, 0, HZ, &mut commit).unwrap();
        for t in 1..=ticks {
            host.tick(world, t, HZ, &mut commit);
            each(world, t);
        }
        let physics = host.mover_mut().as_any_mut().downcast_mut::<PhysicsMover>().unwrap().physics.take().unwrap();
        (physics, host)
    }

    #[test]
    fn modules_walk_up_stairs_and_slopes_with_the_physics_controller() {
        let mut w = World::default();
        solid(&mut w, "floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 10.0), Vec3::ZERO);
        // Three 0.2 m steps from x = 0, then a landing.
        for i in 0..3 {
            let h = 0.2 * (i + 1) as f32;
            solid(&mut w, &format!("step{i}"), Vec3::new(0.25 + 0.5 * i as f32 + 1.0, h * 0.5, 0.0), Vec3::new(0.5, h, 4.0), Vec3::ZERO);
        }
        solid(&mut w, "landing", Vec3::new(4.0, 0.3, 0.0), Vec3::new(2.0, 0.6, 4.0), Vec3::ZERO);
        // A 20-degree ramp down from the landing.
        solid(&mut w, "ramp", Vec3::new(6.8, 0.0, 0.0), Vec3::new(4.0, 0.2, 4.0), Vec3::new(0.0, 0.0, -20.0));
        let mut highest = f32::MIN;
        let (p, host) = run(&mut w, Physics::default(), 150, |w, _| {
            let e = w.get(w.find_path("/hero").unwrap()).unwrap();
            highest = highest.max(e.position.y);
        });
        let e = w.get(w.find_path("/hero").unwrap()).unwrap();
        assert!(highest > 0.55, "climbed the stairs onto the landing (0.6 m): highest {highest}");
        assert!(e.position.x > 7.0, "walked on past the ramp: {:?}", e.position);
        assert_eq!(e.props.get("grounded").and_then(Json::as_bool), Some(true), "on the ground at the end");
        assert!(host.status(None, false).unwrap().to_string().contains("physics P2"), "logic_status names the controller");
        assert!(!p.space.is_empty(), "the space came back");
    }

    #[test]
    fn walls_block_and_runs_repeat_exactly() {
        let mut w = World::default();
        solid(&mut w, "floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 10.0), Vec3::ZERO);
        solid(&mut w, "wall", Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.5, 2.0, 6.0), Vec3::ZERO);
        let (_, _) = run(&mut w, Physics::default(), 90, |_, _| {});
        let e = w.get(w.find_path("/hero").unwrap()).unwrap();
        assert!(e.position.x < -0.5 && e.position.x > -0.6, "stopped against the wall: {:?}", e.position);
        // The same run twice gives the same world (no hidden controller state).
        let mut w2 = World::default();
        solid(&mut w2, "floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 10.0), Vec3::ZERO);
        solid(&mut w2, "wall", Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.5, 2.0, 6.0), Vec3::ZERO);
        run(&mut w2, Physics::default(), 90, |_, _| {});
        assert_eq!(w.state_hash(), w2.state_hash());
    }
}
