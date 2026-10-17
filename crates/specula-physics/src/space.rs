//! The physics world: bodies, the broadphase, contacts, the step pipeline, sleeping, the
//! tunnelling guard, events, queries and the state hash.
//!
//! Per [`Space::step`]:
//!
//! ```text
//! 1. new pairs   bodies whose fat box changed since the last step query both trees
//! 2. narrowphase contact points for every active pair; begin/end and trigger enter/exit events
//! 3. solve       Soft Step over the awake dynamic bodies (see `solver`)
//! 4. integrate   write poses back; kinematic bodies move by their velocity or to their target
//! 5. tunnelling  fast bodies cast their inner sphere against static colliders and stop there
//! 6. sleep       islands (touching awake bodies) at rest long enough fall asleep together
//! 7. broadphase  boxes refreshed; checks, energy and the state hash
//! ```
//!
//! **Determinism (F2).** Bodies are keyed by a caller-chosen stable `u64` (the engine's entity
//! id). Everything that can affect the simulation iterates in key order (`BTreeMap`s, sorted
//! candidate lists); the tree's own traversal order never leaks out. Floating point is plain
//! IEEE `+ - * /` and `sqrt`. The same inputs give bit-identical states and [`Space::hash`].
//!
//! **Bounded work (F1/F3).** Bodies and pairs are capped ([`Settings::max_bodies`],
//! [`Settings::max_pairs`]); past a cap the extra work is dropped (deterministically, in key
//! order) and the step's report carries a structured `budget` error naming the limit.

use crate::contact::{collide, Manifold, MAX_POINTS};
use crate::math::{Aabb, Mat3, Pose, Quat, Vec3};
use crate::query::{self, CastHit, RayHit};
use crate::shape::Shape;
use crate::solver::{self, Constraint, SolverBody, SolverPoint, Softness, Tuning, STATIC};
use crate::tree::Tree;
use crate::PhysicsError;
use specula_core::hash::StableHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Simulation settings. Defaults follow Box2D v3 where it has an equivalent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// m/s².
    pub gravity: Vec3,
    /// Solver substeps per step (1..=16).
    pub substeps: u32,
    /// Bodies at rest may fall asleep.
    pub sleep: bool,
    /// Contact stiffness, Hz (capped at a quarter of the substep rate).
    pub contact_hertz: f32,
    pub contact_damping_ratio: f32,
    /// Fastest speed penetration is resolved at, m/s.
    pub push_speed: f32,
    /// Bounces only above this approach speed, m/s.
    pub restitution_threshold: f32,
    /// Allowed overlap (m): contacts within it count as touching.
    pub linear_slop: f32,
    /// Contacts are made this far ahead (m) so fast approaches stop in time.
    pub speculative_distance: f32,
    /// Broadphase boxes are fattened by this (m) so small motions don't touch the tree.
    pub aabb_margin: f32,
    pub max_linear_speed: f32,
    /// Speed (m/s, linear or at the body's outer radius) under which a body counts as resting.
    pub sleep_threshold: f32,
    /// Seconds at rest before an island sleeps.
    pub time_to_sleep: f32,
    /// 2D: dynamic bodies move only in the XY plane and turn only about Z.
    pub plane_2d: bool,
    pub max_bodies: usize,
    pub max_pairs: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            substeps: 4,
            sleep: true,
            contact_hertz: 30.0,
            contact_damping_ratio: 10.0,
            push_speed: 3.0,
            restitution_threshold: 1.0,
            linear_slop: 0.005,
            speculative_distance: 0.02,
            aabb_margin: 0.1,
            max_linear_speed: 400.0,
            sleep_threshold: 0.05,
            time_to_sleep: 0.5,
            plane_2d: false,
            max_bodies: 20_000,
            max_pairs: 65_536,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BodyKind {
    Static,
    Kinematic,
    Dynamic,
}

impl BodyKind {
    pub fn name(self) -> &'static str {
        match self {
            BodyKind::Static => "static",
            BodyKind::Kinematic => "kinematic",
            BodyKind::Dynamic => "dynamic",
        }
    }
}

/// A body's collider.
#[derive(Clone, Debug)]
pub struct ColliderDesc {
    pub shape: Shape,
    /// The shape's pose in the body frame.
    pub offset: Pose,
    /// Reports overlaps (enter/exit), never blocks.
    pub is_trigger: bool,
    /// One bit: this collider's layer.
    pub layer: u64,
    /// The layers it collides with.
    pub mask: u64,
    pub friction: f32,
    pub restitution: f32,
}

impl ColliderDesc {
    pub fn new(shape: Shape) -> ColliderDesc {
        ColliderDesc { shape, offset: Pose::IDENTITY, is_trigger: false, layer: 1, mask: u64::MAX, friction: 0.6, restitution: 0.0 }
    }
}

/// Everything a body is made of.
#[derive(Clone, Debug)]
pub struct BodyDesc {
    pub kind: BodyKind,
    /// The body frame (the entity's origin) in the world.
    pub pose: Pose,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    /// Kilograms (dynamic bodies).
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub lock_linear: [bool; 3],
    pub lock_angular: [bool; 3],
    /// Starts asleep (a loaded world's resting bodies).
    pub sleeping: bool,
    pub can_sleep: bool,
    /// The tunnelling guard against static colliders.
    pub ccd: bool,
    pub collider: Option<ColliderDesc>,
}

impl BodyDesc {
    pub fn new(kind: BodyKind, pose: Pose, collider: Option<ColliderDesc>) -> BodyDesc {
        BodyDesc {
            kind,
            pose,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.05,
            gravity_scale: 1.0,
            lock_linear: [false; 3],
            lock_angular: [false; 3],
            sleeping: false,
            can_sleep: true,
            ccd: true,
            collider,
        }
    }
}

const NONE: u32 = u32::MAX;

#[derive(Clone, Debug)]
struct Body {
    key: u64,
    desc: BodyDesc,
    /// Centre of mass in the body frame.
    com: Vec3,
    /// World centre of mass.
    center: Vec3,
    inv_mass: f32,
    inertia_local: Mat3,
    inv_inertia_world: Mat3,
    sleep_time: f32,
    /// Sleeping island id (`NONE` = awake or no island).
    island: u32,
    proxy: u32,
    in_static_tree: bool,
    /// Kinematic: where to be at the end of the next step, and the velocity to return to.
    target: Option<Pose>,
    declared_velocity: (Vec3, Vec3),
    inner_radius: f32,
    outer_radius: f32,
    /// Solver index this step.
    solver: u32,
    /// Centre and rotation at the step start.
    c0: Vec3,
    q0: Quat,
}

impl Body {
    fn is_dynamic(&self) -> bool {
        self.desc.kind == BodyKind::Dynamic
    }

    fn sleeping(&self) -> bool {
        self.desc.sleeping
    }

    /// Moves this step: an awake dynamic body, or a kinematic one with a velocity or target.
    fn active(&self) -> bool {
        match self.desc.kind {
            BodyKind::Static => false,
            BodyKind::Dynamic => !self.desc.sleeping,
            BodyKind::Kinematic => self.target.is_some() || self.desc.velocity != Vec3::ZERO || self.desc.angular_velocity != Vec3::ZERO,
        }
    }

    fn aabb(&self) -> Option<Aabb> {
        let c = self.desc.collider.as_ref()?;
        Some(c.shape.aabb(&self.desc.pose.mul(&c.offset)))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CPoint {
    point: Vec3,
    normal: Vec3,
    separation: f32,
    id: u32,
    normal_impulse: f32,
    tangent_impulse: [f32; 2],
    max_normal_impulse: f32,
}

#[derive(Clone, Debug)]
struct Contact {
    a: u32,
    b: u32,
    points: [CPoint; MAX_POINTS],
    len: usize,
    touching: bool,
    trigger: bool,
    friction: f32,
    restitution: f32,
    /// Updated this step (one of its bodies moves).
    sim: bool,
}

/// What happened during a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    ContactBegin,
    ContactEnd,
    TriggerEnter,
    TriggerExit,
    /// The tunnelling guard stopped a fast body at a static collider.
    Ccd,
}

impl EventKind {
    pub fn name(self) -> &'static str {
        match self {
            EventKind::ContactBegin => "contact_begin",
            EventKind::ContactEnd => "contact_end",
            EventKind::TriggerEnter => "trigger_enter",
            EventKind::TriggerExit => "trigger_exit",
            EventKind::Ccd => "ccd",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    /// Keys, lower first (for triggers: `a` is the trigger; for CCD: `a` is the fast body).
    pub a: u64,
    pub b: u64,
    pub point: Vec3,
    pub normal: Vec3,
    /// Approach speed (begin), or the fast body's speed (ccd), m/s.
    pub speed: f32,
}

/// A fast body (moves more than half its inner radius in a step): it could pass through
/// thin dynamic bodies (static ones are guarded).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FastBody {
    pub key: u64,
    pub speed: f32,
    /// Inner radius, m (the thickness the guard trusts).
    pub thickness: f32,
}

/// Wall-clock time per stage of the last step, microseconds (not part of the simulation).
#[derive(Clone, Copy, Debug, Default)]
pub struct StageTimes {
    pub pairs_us: f64,
    pub narrowphase_us: f64,
    pub solve_us: f64,
    pub integrate_us: f64,
    pub ccd_us: f64,
    pub sleep_us: f64,
    pub broadphase_us: f64,
    pub total_us: f64,
}

/// The last step's numbers.
#[derive(Clone, Debug, Default)]
pub struct StepReport {
    pub step: u64,
    pub bodies: usize,
    pub awake: usize,
    pub sleeping: usize,
    pub pairs: usize,
    pub touching: usize,
    pub constraints: usize,
    pub points: usize,
    pub new_pairs: usize,
    pub islands_slept: usize,
    pub events: Vec<Event>,
    pub fast: Vec<FastBody>,
    pub fast_total: usize,
    /// Deepest contact: `(separation, a, b)`.
    pub deepest: Option<(f32, u64, u64)>,
    /// Joules, dynamic bodies only: kinetic, and potential (−m g·x).
    pub kinetic_energy: f64,
    pub potential_energy: f64,
    /// Energy the step added to the bodies it moved (J; above zero = gained: pushes from
    /// kinematic bodies or penetration recovery; a closed system should not gain).
    pub energy_change: f64,
    pub errors: Vec<PhysicsError>,
    pub times: StageTimes,
}

/// Most fast bodies listed per report.
pub const MAX_FAST_LISTED: usize = 16;
/// Most events kept per step.
pub const MAX_EVENTS_PER_STEP: usize = 4096;

/// A query filter: which layers to hit, triggers or not, and one key to ignore.
#[derive(Clone, Copy, Debug)]
pub struct Filter {
    pub mask: u64,
    pub triggers: bool,
    pub exclude: Option<u64>,
}

impl Default for Filter {
    fn default() -> Filter {
        Filter { mask: u64::MAX, triggers: false, exclude: None }
    }
}

/// A query hit on a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub key: u64,
    /// Ray: distance (m); cast: fraction of the motion.
    pub at: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub triangle: u32,
    pub initially_overlapping: bool,
}

/// A read-only view of a body.
#[derive(Clone, Copy, Debug)]
pub struct BodyView<'a> {
    pub key: u64,
    pub kind: BodyKind,
    pub pose: Pose,
    pub center: Vec3,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub sleeping: bool,
    pub mass: f32,
    pub collider: Option<&'a ColliderDesc>,
    pub inner_radius: f32,
    pub aabb: Option<Aabb>,
}

/// A contact pair as data.
#[derive(Clone, Debug)]
pub struct ContactView {
    pub a: u64,
    pub b: u64,
    pub touching: bool,
    pub trigger: bool,
    /// `(point, normal A→B, separation, normal impulse)`.
    pub points: Vec<(Vec3, Vec3, f32, f32)>,
}

/// Per-character state carried between moves (the controller's memory).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterState {
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub ground: Option<u64>,
}

/// The physics world.
#[derive(Clone, Debug)]
pub struct Space {
    pub settings: Settings,
    slots: Vec<Option<Body>>,
    free: Vec<u32>,
    by_key: BTreeMap<u64, u32>,
    static_tree: Tree,
    dynamic_tree: Tree,
    contacts: BTreeMap<(u64, u64), Contact>,
    /// Bodies whose fat box changed (or that were added) since pairs were last found.
    moved: BTreeSet<u64>,
    islands: BTreeMap<u32, Vec<u64>>,
    next_island: u32,
    pub(crate) characters: BTreeMap<u64, CharacterState>,
    steps: u64,
    report: StepReport,
    /// Bodies refused at the cap since the last step (reported once).
    refused: usize,
    /// Keys of kinematic and awake dynamic bodies: what a step visits (sleeping bodies cost
    /// nothing).
    awake: BTreeSet<u64>,
    /// Contact keys reversed `(b, a)`, to find a body's pairs from either side.
    reverse: BTreeSet<(u64, u64)>,
    touching_count: usize,
    dynamic_count: usize,
    // Reused buffers.
    active: Vec<u32>,
    solver_bodies: Vec<SolverBody>,
    constraints: Vec<Constraint>,
    candidates: Vec<(u64, u64)>,
}

impl Default for Space {
    fn default() -> Space {
        Space::new(Settings::default())
    }
}

impl Space {
    pub fn new(settings: Settings) -> Space {
        Space {
            settings,
            slots: Vec::new(),
            free: Vec::new(),
            by_key: BTreeMap::new(),
            static_tree: Tree::default(),
            dynamic_tree: Tree::default(),
            contacts: BTreeMap::new(),
            moved: BTreeSet::new(),
            islands: BTreeMap::new(),
            next_island: 0,
            characters: BTreeMap::new(),
            steps: 0,
            report: StepReport::default(),
            refused: 0,
            awake: BTreeSet::new(),
            reverse: BTreeSet::new(),
            touching_count: 0,
            dynamic_count: 0,
            active: Vec::new(),
            solver_bodies: Vec::new(),
            constraints: Vec::new(),
            candidates: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    pub fn contains(&self, key: u64) -> bool {
        self.by_key.contains_key(&key)
    }

    pub fn keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.by_key.keys().copied()
    }

    pub fn report(&self) -> &StepReport {
        &self.report
    }

    fn get(&self, key: u64) -> Option<&Body> {
        self.by_key.get(&key).and_then(|&s| self.slots[s as usize].as_ref())
    }

    fn slot(&self, s: u32) -> &Body {
        self.slots[s as usize].as_ref().expect("live slot")
    }

    fn slot_mut(&mut self, s: u32) -> &mut Body {
        self.slots[s as usize].as_mut().expect("live slot")
    }

    /// Adds a body, or updates the body with this key in place (its contacts and their warm
    /// start survive unless its kind, trigger flag or layers changed).
    pub fn insert(&mut self, key: u64, desc: BodyDesc) -> Result<(), PhysicsError> {
        validate(&desc)?;
        if let Some(&s) = self.by_key.get(&key) {
            let old = self.slot(s);
            let same_pairs = old.desc.kind == desc.kind
                && match (&old.desc.collider, &desc.collider) {
                    (Some(a), Some(b)) => a.is_trigger == b.is_trigger && a.layer == b.layer && a.mask == b.mask,
                    (None, None) => true,
                    _ => false,
                };
            if !same_pairs {
                self.remove(key);
                return self.insert(key, desc);
            }
            self.wake_slot(s);
            let settings = self.settings;
            let b = self.slot_mut(s);
            b.desc = desc;
            refresh_mass(b, &settings);
            b.target = None;
            b.declared_velocity = (b.desc.velocity, b.desc.angular_velocity);
            b.island = NONE;
            b.sleep_time = 0.0;
            self.refresh_proxy(s, true);
            self.sync_awake(s);
            return Ok(());
        }
        if self.by_key.len() >= self.settings.max_bodies {
            self.refused += 1;
            return Err(PhysicsError::new("budget", format!("physics holds at most {} bodies (Settings::max_bodies)", self.settings.max_bodies))
                .with("limit", self.settings.max_bodies)
                .with("key", key));
        }
        let mut b = Body {
            key,
            desc,
            com: Vec3::ZERO,
            center: Vec3::ZERO,
            inv_mass: 0.0,
            inertia_local: Mat3::ZERO,
            inv_inertia_world: Mat3::ZERO,
            sleep_time: 0.0,
            island: NONE,
            proxy: NONE,
            in_static_tree: false,
            target: None,
            declared_velocity: (Vec3::ZERO, Vec3::ZERO),
            inner_radius: 0.0,
            outer_radius: 0.0,
            solver: NONE,
            c0: Vec3::ZERO,
            q0: Quat::IDENTITY,
        };
        b.declared_velocity = (b.desc.velocity, b.desc.angular_velocity);
        refresh_mass(&mut b, &self.settings);
        let s = match self.free.pop() {
            Some(s) => {
                self.slots[s as usize] = Some(b);
                s
            }
            None => {
                self.slots.push(Some(b));
                (self.slots.len() - 1) as u32
            }
        };
        self.by_key.insert(key, s);
        if self.slot(s).is_dynamic() {
            self.dynamic_count += 1;
        }
        self.refresh_proxy(s, true);
        self.sync_awake(s);
        Ok(())
    }

    /// Keeps [`Space::awake`] in step with a body's kind and sleep state.
    fn sync_awake(&mut self, s: u32) {
        let b = self.slot(s);
        let key = b.key;
        let want = match b.desc.kind {
            BodyKind::Static => false,
            BodyKind::Kinematic => true,
            BodyKind::Dynamic => !b.desc.sleeping,
        };
        if want {
            self.awake.insert(key);
        } else {
            self.awake.remove(&key);
        }
    }

    /// The contact keys a body is in, sorted.
    fn pairs_of(&self, key: u64) -> Vec<(u64, u64)> {
        let mut out: Vec<(u64, u64)> = self.contacts.range((key, 0)..=(key, u64::MAX)).map(|(k, _)| *k).collect();
        out.extend(self.reverse.range((key, 0)..=(key, u64::MAX)).map(|(b, a)| (*a, *b)));
        out.sort_unstable();
        out
    }

    fn drop_contact(&mut self, k: (u64, u64)) -> Option<Contact> {
        let c = self.contacts.remove(&k)?;
        self.reverse.remove(&(k.1, k.0));
        if c.touching {
            self.touching_count -= 1;
        }
        Some(c)
    }

    /// Removes a body; bodies resting on it wake up. Returns whether it existed.
    pub fn remove(&mut self, key: u64) -> bool {
        let Some(&s) = self.by_key.get(&key) else { return false };
        // Wake what touched it and drop its contacts.
        for p in self.pairs_of(key) {
            let c = self.drop_contact(p).expect("listed");
            let other = if c.a == s { c.b } else { c.a };
            if c.touching {
                self.wake_slot(other);
            }
        }
        self.by_key.remove(&key);
        self.awake.remove(&key);
        let b = self.slots[s as usize].take().expect("live");
        if b.desc.kind == BodyKind::Dynamic {
            self.dynamic_count -= 1;
        }
        if b.proxy != NONE {
            if b.in_static_tree {
                self.static_tree.remove(b.proxy);
            } else {
                self.dynamic_tree.remove(b.proxy);
            }
        }
        if b.island != NONE {
            if let Some(members) = self.islands.get_mut(&b.island) {
                members.retain(|k| *k != key);
            }
        }
        self.free.push(s);
        self.moved.remove(&key);
        self.characters.remove(&key);
        true
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        let settings = self.settings;
        *self = Space::new(settings);
    }

    /// Teleports a body (wakes it; a kinematic body's pending target is dropped).
    pub fn set_pose(&mut self, key: u64, pose: Pose) {
        let Some(&s) = self.by_key.get(&key) else { return };
        self.wake_slot(s);
        let b = self.slot_mut(s);
        b.desc.pose = pose;
        b.target = None;
        b.center = pose.point(b.com);
        update_inertia(b);
        let moves_others = b.desc.kind != BodyKind::Dynamic;
        self.refresh_proxy(s, false);
        if moves_others {
            // Moving a static or kinematic body by hand wakes what it now overlaps.
            if let Some(bounds) = self.slot(s).aabb() {
                let mut near = Vec::new();
                self.dynamic_tree.query(&bounds.expand(self.settings.aabb_margin), |o| {
                    near.push(o);
                    true
                });
                near.sort_unstable();
                for o in near {
                    self.wake_slot(o);
                }
            }
        }
    }

    pub fn set_velocity(&mut self, key: u64, v: Vec3, w: Vec3) {
        let Some(&s) = self.by_key.get(&key) else { return };
        self.wake_slot(s);
        let b = self.slot_mut(s);
        b.desc.velocity = v;
        b.desc.angular_velocity = w;
        b.declared_velocity = (v, w);
    }

    /// A kinematic body arrives at `target` at the end of the next step, moving there at the
    /// matching velocity (so it pushes what it meets).
    pub fn move_kinematic(&mut self, key: u64, target: Pose) {
        let Some(&s) = self.by_key.get(&key) else { return };
        if self.slot(s).desc.kind != BodyKind::Kinematic {
            self.set_pose(key, target);
            return;
        }
        self.slot_mut(s).target = Some(target);
    }

    /// Adds `impulse` (N·s) at world point `at` to a dynamic body (wakes it).
    pub fn apply_impulse(&mut self, key: u64, impulse: Vec3, at: Vec3) {
        let Some(&s) = self.by_key.get(&key) else { return };
        if !self.slot(s).is_dynamic() {
            return;
        }
        self.wake_slot(s);
        let plane = self.settings.plane_2d;
        let b = self.slot_mut(s);
        let (ll, la) = locks(b, plane);
        let mut dv = impulse * b.inv_mass;
        let mut dw = b.inv_inertia_world.mul_vec((at - b.center).cross(impulse));
        mask(&mut dv, ll);
        mask(&mut dw, la);
        b.desc.velocity = b.desc.velocity + dv;
        b.desc.angular_velocity = b.desc.angular_velocity + dw;
    }

    pub fn body(&self, key: u64) -> Option<BodyView<'_>> {
        let b = self.get(key)?;
        Some(BodyView {
            key,
            kind: b.desc.kind,
            pose: b.desc.pose,
            center: b.center,
            velocity: b.desc.velocity,
            angular_velocity: b.desc.angular_velocity,
            sleeping: b.desc.sleeping,
            mass: if b.inv_mass > 0.0 { 1.0 / b.inv_mass } else { 0.0 },
            collider: b.desc.collider.as_ref(),
            inner_radius: b.inner_radius,
            aabb: b.aabb(),
        })
    }

    pub fn bodies(&self) -> impl Iterator<Item = BodyView<'_>> + '_ {
        self.by_key.keys().filter_map(|k| self.body(*k))
    }

    /// A kinematic body's velocity over the next step of `dt` (towards its target if it has
    /// one); other bodies' current velocity.
    pub fn next_velocity(&self, key: u64, dt: f32) -> Vec3 {
        match self.get(key) {
            Some(b) => match b.target {
                Some(t) if dt > 0.0 => (t.p - b.desc.pose.p) * (1.0 / dt),
                _ => b.desc.velocity,
            },
            None => Vec3::ZERO,
        }
    }

    pub fn character_state(&self, key: u64) -> Option<CharacterState> {
        self.characters.get(&key).copied()
    }

    /// Contact pairs in key order (touching ones, or all).
    pub fn contacts(&self, touching_only: bool) -> impl Iterator<Item = ContactView> + '_ {
        self.contacts.iter().filter(move |(_, c)| !touching_only || c.touching).map(|((a, b), c)| ContactView {
            a: *a,
            b: *b,
            touching: c.touching,
            trigger: c.trigger,
            points: c.points[..c.len].iter().map(|p| (p.point, oriented(p.normal, self.slot(c.a).key, *a), p.separation, p.normal_impulse)).collect(),
        })
    }

    /// The keys a body touches (or overlaps, for triggers), sorted.
    pub fn touching(&self, key: u64) -> Vec<u64> {
        let mut out: Vec<u64> = self
            .pairs_of(key)
            .into_iter()
            .filter(|k| self.contacts.get(k).is_some_and(|c| c.touching))
            .map(|(a, b)| if a == key { b } else { a })
            .collect();
        out.sort_unstable();
        out
    }

    fn wake_slot(&mut self, s: u32) {
        let b = self.slot(s);
        if !b.desc.sleeping {
            return;
        }
        let island = b.island;
        if island != NONE {
            if let Some(members) = self.islands.remove(&island) {
                for k in members {
                    if let Some(&m) = self.by_key.get(&k) {
                        let mb = self.slot_mut(m);
                        mb.desc.sleeping = false;
                        mb.sleep_time = 0.0;
                        mb.island = NONE;
                        self.sync_awake(m);
                    }
                }
            }
        }
        let b = self.slot_mut(s);
        b.desc.sleeping = false;
        b.sleep_time = 0.0;
        b.island = NONE;
        self.sync_awake(s);
    }

    /// Wakes a body and its island.
    pub fn wake(&mut self, key: u64) {
        if let Some(&s) = self.by_key.get(&key) {
            self.wake_slot(s);
        }
    }

    /// Puts the body's box in the right tree; `force` re-inserts even when still inside the fat
    /// box (new bodies, changed shapes). Marks it moved when the fat box changed.
    fn refresh_proxy(&mut self, s: u32, force: bool) {
        let margin = self.settings.aabb_margin;
        let b = self.slot(s);
        let key = b.key;
        let want_static = b.desc.kind == BodyKind::Static;
        let aabb = b.aabb();
        let (proxy, in_static) = (b.proxy, b.in_static_tree);
        match aabb {
            None => {
                if proxy != NONE {
                    if in_static {
                        self.static_tree.remove(proxy);
                    } else {
                        self.dynamic_tree.remove(proxy);
                    }
                    self.slot_mut(s).proxy = NONE;
                }
            }
            Some(aabb) => {
                if proxy != NONE && in_static != want_static {
                    if in_static {
                        self.static_tree.remove(proxy);
                    } else {
                        self.dynamic_tree.remove(proxy);
                    }
                    self.slot_mut(s).proxy = NONE;
                }
                let proxy = self.slot(s).proxy;
                let tree = if want_static { &mut self.static_tree } else { &mut self.dynamic_tree };
                // Static boxes need no slack: they don't move.
                let fat = if want_static { aabb } else { aabb.expand(margin) };
                if proxy == NONE {
                    let p = tree.insert(fat, s);
                    let b = self.slot_mut(s);
                    b.proxy = p;
                    b.in_static_tree = want_static;
                    self.moved.insert(key);
                } else if force || !tree.fat_aabb(proxy).contains(&aabb) {
                    tree.update(proxy, fat);
                    self.moved.insert(key);
                }
            }
        }
    }

    fn fat(&self, b: &Body) -> Option<Aabb> {
        if b.proxy == NONE {
            return None;
        }
        Some(if b.in_static_tree { self.static_tree.fat_aabb(b.proxy) } else { self.dynamic_tree.fat_aabb(b.proxy) })
    }

    /// Whether two bodies' colliders make a pair (kinds, triggers, layers).
    fn should_pair(a: &Body, b: &Body) -> bool {
        let (Some(ca), Some(cb)) = (&a.desc.collider, &b.desc.collider) else { return false };
        if ca.mask & cb.layer == 0 || cb.mask & ca.layer == 0 {
            return false;
        }
        if ca.is_trigger && cb.is_trigger {
            return false;
        }
        let (sa, sb) = (a.desc.kind == BodyKind::Static, b.desc.kind == BodyKind::Static);
        if ca.is_trigger || cb.is_trigger {
            return !(sa && sb);
        }
        a.is_dynamic() || b.is_dynamic()
    }

    /// Finds pairs for every moved body (step 1).
    fn find_pairs(&mut self) {
        let mut cands = std::mem::take(&mut self.candidates);
        cands.clear();
        for &key in &self.moved {
            let Some(&s) = self.by_key.get(&key) else { continue };
            let b = self.slot(s);
            let Some(fat) = self.fat(b) else { continue };
            let mut visit = |other: u32| {
                if other != s {
                    let ok = self.slots[other as usize].as_ref().is_some_and(|o| Space::should_pair(b, o));
                    if ok {
                        let ko = self.slot(other).key;
                        cands.push((key.min(ko), key.max(ko)));
                    }
                }
                true
            };
            self.dynamic_tree.query(&fat, &mut visit);
            if b.desc.kind != BodyKind::Static {
                self.static_tree.query(&fat, &mut visit);
            }
        }
        self.moved.clear();
        cands.sort_unstable();
        cands.dedup();
        let mut dropped = 0usize;
        for &(ka, kb) in &cands {
            if self.contacts.contains_key(&(ka, kb)) {
                continue;
            }
            if self.contacts.len() >= self.settings.max_pairs {
                dropped += 1;
                continue;
            }
            let (sa, sb) = (self.by_key[&ka], self.by_key[&kb]);
            let (a, b) = (self.slot(sa), self.slot(sb));
            let (ca, cb) = (a.desc.collider.as_ref().expect("paired"), b.desc.collider.as_ref().expect("paired"));
            let trigger = ca.is_trigger || cb.is_trigger;
            let friction = (ca.friction * cb.friction).sqrt();
            let restitution = ca.restitution.max(cb.restitution);
            self.contacts.insert(
                (ka, kb),
                Contact { a: sa, b: sb, points: [CPoint::default(); MAX_POINTS], len: 0, touching: false, trigger, friction, restitution, sim: false },
            );
            self.reverse.insert((kb, ka));
            self.report.new_pairs += 1;
        }
        if dropped > 0 {
            self.report.errors.push(
                PhysicsError::new("budget", format!("{dropped} new contact pairs were dropped: physics keeps at most {} pairs (Settings::max_pairs)", self.settings.max_pairs))
                    .with("limit", self.settings.max_pairs)
                    .with("dropped", dropped),
            );
        }
        self.candidates = cands;
    }

    /// Contact points for every active pair (step 2).
    fn narrowphase(&mut self) {
        let margin = self.settings.speculative_distance;
        let slop = self.settings.linear_slop;
        let mut remove: Vec<(u64, u64)> = Vec::new();
        let mut wake: Vec<u32> = Vec::new();
        let mut events: Vec<Event> = Vec::new();
        let mut m = Manifold::default();
        let mut touching_delta: isize = 0;
        let mut deepest: Option<(f32, u64, u64)> = None;
        for (&(ka, kb), c) in self.contacts.iter_mut() {
            let (a, b) = (self.slots[c.a as usize].as_ref().expect("live"), self.slots[c.b as usize].as_ref().expect("live"));
            c.sim = a.active() || b.active();
            if !c.sim {
                continue;
            }
            let fa = fat_of(&self.static_tree, &self.dynamic_tree, a);
            let fb = fat_of(&self.static_tree, &self.dynamic_tree, b);
            if !fa.overlaps(&fb) {
                if c.touching {
                    events.push(end_event(c, ka, kb));
                }
                remove.push((ka, kb));
                continue;
            }
            let (ca, cb) = (a.desc.collider.as_ref().expect("paired"), b.desc.collider.as_ref().expect("paired"));
            let (pa, pb) = (a.desc.pose.mul(&ca.offset), b.desc.pose.mul(&cb.offset));
            let was = c.touching;
            // Bounding spheres apart: no points (cheap early out).
            let reach = a.outer_radius + b.outer_radius + margin;
            let far = crate::math::len_sq(a.center - b.center) > reach * reach;
            if c.trigger {
                if far {
                    m.len = 0;
                } else {
                    collide(&ca.shape, &pa, &cb.shape, &pb, 0.0, &mut m);
                }
                let deepest = m.as_slice().iter().fold(None::<crate::contact::ContactPoint>, |d, p| match d {
                    Some(q) if q.separation <= p.separation => Some(q),
                    _ => Some(*p),
                });
                c.touching = deepest.is_some_and(|p| p.separation <= 0.0);
                c.len = 0;
                touching_delta += isize::from(c.touching) - isize::from(was);
                if c.touching != was {
                    // `a` of a trigger event is the trigger.
                    let trig = if ca.is_trigger { a.key } else { b.key };
                    let other = if ca.is_trigger { b.key } else { a.key };
                    let p = deepest.map_or(Vec3::ZERO, |p| p.point);
                    events.push(Event { kind: if c.touching { EventKind::TriggerEnter } else { EventKind::TriggerExit }, a: trig, b: other, point: p, normal: Vec3::ZERO, speed: 0.0 });
                }
                continue;
            }
            if far {
                m.len = 0;
            } else {
                collide(&ca.shape, &pa, &cb.shape, &pb, margin, &mut m);
            }
            // Warm start: carry impulses over by feature id.
            let old = c.points;
            let old_len = c.len;
            c.len = m.len;
            let mut min_sep = f32::MAX;
            for (i, p) in m.as_slice().iter().enumerate() {
                let mut cp = CPoint { point: p.point, normal: p.normal, separation: p.separation, id: p.id, ..CPoint::default() };
                if let Some(o) = old[..old_len].iter().find(|o| o.id == p.id) {
                    cp.normal_impulse = o.normal_impulse;
                    cp.tangent_impulse = o.tangent_impulse;
                }
                c.points[i] = cp;
                min_sep = min_sep.min(p.separation);
            }
            c.touching = c.len > 0 && min_sep <= slop;
            touching_delta += isize::from(c.touching) - isize::from(was);
            if c.len > 0 && deepest.is_none_or(|(d, _, _)| min_sep < d) {
                deepest = Some((min_sep, ka, kb));
            }
            if c.touching && !was {
                let p = c.points[..c.len].iter().fold(c.points[0], |d, p| if p.separation < d.separation { *p } else { d });
                let rel = (b.desc.velocity + b.desc.angular_velocity.cross(p.point - b.center)) - (a.desc.velocity + a.desc.angular_velocity.cross(p.point - a.center));
                let normal = oriented(p.normal, a.key, ka);
                events.push(Event { kind: EventKind::ContactBegin, a: ka, b: kb, point: p.point, normal, speed: (-rel.dot(p.normal)).max(0.0) });
            } else if !c.touching && was {
                events.push(end_event(c, ka, kb));
            }
            if c.touching {
                if a.is_dynamic() && a.sleeping() {
                    wake.push(c.a);
                }
                if b.is_dynamic() && b.sleeping() {
                    wake.push(c.b);
                }
            }
        }
        self.touching_count = (self.touching_count as isize + touching_delta) as usize;
        self.report.deepest = deepest;
        for k in remove {
            self.drop_contact(k);
        }
        for s in wake {
            self.wake_slot(s);
        }
        let room = MAX_EVENTS_PER_STEP.saturating_sub(self.report.events.len());
        self.report.events.extend(events.into_iter().take(room));
    }

    /// Advances the simulation by `dt` seconds.
    pub fn step(&mut self, dt: f32) -> &StepReport {
        let t_start = Instant::now();
        self.report = StepReport { step: self.steps + 1, ..StepReport::default() };
        if self.refused > 0 {
            self.report.errors.push(
                PhysicsError::new("budget", format!("{} bodies were refused: physics holds at most {} (Settings::max_bodies)", self.refused, self.settings.max_bodies))
                    .with("limit", self.settings.max_bodies)
                    .with("refused", self.refused),
            );
            self.refused = 0;
        }
        if !(dt > 0.0 && dt.is_finite()) {
            return &self.report;
        }
        self.steps += 1;
        let mut t = Instant::now();
        let mut lap = |acc: &mut f64| {
            let now = Instant::now();
            *acc += (now - t).as_secs_f64() * 1e6;
            t = now;
        };
        let mut times = StageTimes::default();
        // Kinematic targets become velocities for this step.
        let inv_dt = 1.0 / dt;
        for k in self.awake.iter().copied().collect::<Vec<_>>() {
            let b = self.slot_mut(self.by_key[&k]);
            if b.desc.kind != BodyKind::Kinematic {
                continue;
            }
            if let Some(target) = b.target {
                b.desc.velocity = (target.p - b.desc.pose.p) * inv_dt;
                b.desc.angular_velocity = angular_velocity_to(b.desc.pose.q, target.q, inv_dt);
            }
        }
        self.find_pairs();
        lap(&mut times.pairs_us);
        self.narrowphase();
        lap(&mut times.narrowphase_us);
        // What this step moves: kinematic bodies and awake dynamic ones, in key order.
        let mut active = std::mem::take(&mut self.active);
        active.clear();
        active.extend(self.awake.iter().map(|k| self.by_key[k]));
        self.active = active;
        let (ke0, pe0) = self.energy();
        let energy_before = ke0 + pe0;
        self.solve(dt);
        lap(&mut times.solve_us);
        self.integrate_kinematic(dt);
        lap(&mut times.integrate_us);
        self.tunnelling_guard(dt);
        lap(&mut times.ccd_us);
        self.sleep(dt);
        lap(&mut times.sleep_us);
        // Refresh the boxes of what moved, and the report.
        let (ke, pe) = self.energy();
        let active = std::mem::take(&mut self.active);
        let mut awake = 0;
        for &s in &active {
            self.refresh_proxy(s, false);
            let b = self.slot_mut(s);
            b.solver = NONE;
            awake += usize::from(b.is_dynamic() && !b.desc.sleeping);
        }
        self.active = active;
        lap(&mut times.broadphase_us);
        self.report.bodies = self.by_key.len();
        self.report.awake = awake;
        self.report.sleeping = self.dynamic_count - self.awake.iter().filter(|k| self.get(**k).is_some_and(|b| b.is_dynamic())).count();
        self.report.pairs = self.contacts.len();
        self.report.touching = self.touching_count;
        self.report.kinetic_energy = ke;
        self.report.potential_energy = pe;
        self.report.energy_change = ke + pe - energy_before;
        times.total_us = (Instant::now() - t_start).as_secs_f64() * 1e6;
        self.report.times = times;
        &self.report
    }

    /// Kinetic and potential energy of the dynamic bodies this step moves (sleeping bodies
    /// hold still, so they never change it).
    fn energy(&self) -> (f64, f64) {
        let g = self.settings.gravity;
        let (mut ke, mut pe) = (0.0f64, 0.0f64);
        for &s in &self.active {
            let b = self.slot(s);
            if b.is_dynamic() && b.inv_mass > 0.0 {
                let m = 1.0 / b.inv_mass;
                let v = b.desc.velocity;
                let w = b.desc.angular_velocity;
                ke += 0.5 * f64::from(m) * f64::from(v.dot(v)) + 0.5 * f64::from(w.dot(b.inertia_world().mul_vec(w)));
                pe += -f64::from(m) * f64::from(g.dot(b.center));
            }
        }
        (ke, pe)
    }

    fn solve(&mut self, dt: f32) {
        let settings = self.settings;
        let mut sbodies = std::mem::take(&mut self.solver_bodies);
        let mut cons = std::mem::take(&mut self.constraints);
        sbodies.clear();
        cons.clear();
        let slots = std::mem::take(&mut self.active);
        for &s in &slots {
            let plane = settings.plane_2d;
            let b = self.slot_mut(s);
            b.solver = NONE;
            b.c0 = b.center;
            b.q0 = b.desc.pose.q;
            let simulated = match b.desc.kind {
                BodyKind::Dynamic => !b.desc.sleeping,
                BodyKind::Kinematic => b.active(),
                BodyKind::Static => false,
            };
            if !simulated {
                continue;
            }
            b.solver = sbodies.len() as u32;
            let kinematic = b.desc.kind == BodyKind::Kinematic;
            let (ll, la) = if kinematic { ([false; 3], [false; 3]) } else { locks(b, plane) };
            let mut inv_mass = Vec3::new(b.inv_mass, b.inv_mass, b.inv_mass);
            mask(&mut inv_mass, ll);
            let inv_i = lock_inertia(b.inv_inertia_world, la);
            let mut v = b.desc.velocity;
            let mut w = b.desc.angular_velocity;
            if !kinematic {
                mask(&mut v, ll);
                mask(&mut w, la);
            }
            sbodies.push(SolverBody {
                v,
                w,
                dp: Vec3::ZERO,
                dq: Quat::IDENTITY,
                inv_mass: if kinematic { Vec3::ZERO } else { inv_mass },
                inv_inertia: if kinematic { Mat3::ZERO } else { inv_i },
                gravity: if kinematic { Vec3::ZERO } else { settings.gravity * b.desc.gravity_scale },
                linear_damping: b.desc.linear_damping,
                angular_damping: b.desc.angular_damping,
                kinematic,
                lock_linear: ll,
                lock_angular: la,
            });
        }
        let substep_rate = settings.substeps.max(1) as f32 / dt;
        let hertz = settings.contact_hertz.min(0.25 * substep_rate);
        let h = dt / settings.substeps.max(1) as f32;
        let soft = Softness::new(hertz, settings.contact_damping_ratio, h);
        let static_soft = Softness::new(2.0 * hertz, settings.contact_damping_ratio, h);
        for (i, c) in self.contacts.values().enumerate() {
            if !c.sim || c.trigger || c.len == 0 {
                continue;
            }
            let (a, b) = (self.slot(c.a), self.slot(c.b));
            let (dyn_a, dyn_b) = (a.is_dynamic() && !a.desc.sleeping, b.is_dynamic() && !b.desc.sleeping);
            if !dyn_a && !dyn_b {
                continue;
            }
            let (ia, ib) = (if a.solver == NONE { STATIC } else { a.solver }, if b.solver == NONE { STATIC } else { b.solver });
            let both_dynamic = dyn_a && dyn_b;
            let mut points = [SolverPoint::default(); 4];
            for (k, p) in c.points[..c.len].iter().enumerate() {
                let ra = p.point - a.center;
                let rb = p.point - b.center;
                points[k] = SolverPoint {
                    ra,
                    rb,
                    normal: p.normal,
                    adjusted_separation: p.separation - (rb - ra).dot(p.normal),
                    normal_impulse: p.normal_impulse,
                    tangent_impulse: p.tangent_impulse,
                    ..SolverPoint::default()
                };
            }
            cons.push(Constraint {
                a: ia,
                b: ib,
                friction: c.friction,
                restitution: c.restitution,
                soft: if both_dynamic { soft } else { static_soft },
                points,
                len: c.len,
                contact: i as u32,
            });
        }
        self.report.constraints = cons.len();
        self.report.points = cons.iter().map(|c| c.len).sum();
        let tuning = Tuning {
            substeps: settings.substeps.max(1),
            contact_hertz: hertz,
            damping_ratio: settings.contact_damping_ratio,
            push_speed: settings.push_speed,
            restitution_threshold: settings.restitution_threshold,
            max_linear_speed: settings.max_linear_speed,
        };
        solver::step(&mut cons, &mut sbodies, dt, &tuning);
        // Store impulses for next step's warm start.
        let mut by_index: Vec<Option<&Constraint>> = vec![None; self.contacts.len()];
        for c in &cons {
            by_index[c.contact as usize] = Some(c);
        }
        for (i, c) in self.contacts.values_mut().enumerate() {
            if !c.sim {
                continue;
            }
            if let Some(sc) = by_index[i] {
                for k in 0..c.len {
                    c.points[k].normal_impulse = sc.points[k].normal_impulse;
                    c.points[k].tangent_impulse = sc.points[k].tangent_impulse;
                    c.points[k].max_normal_impulse = sc.points[k].max_normal_impulse;
                }
            }
        }
        // Write dynamic bodies back.
        for &s in &slots {
            let b = self.slot_mut(s);
            if b.solver == NONE || b.desc.kind != BodyKind::Dynamic {
                continue;
            }
            let sb = sbodies[b.solver as usize];
            b.center = b.c0 + sb.dp;
            let q = (sb.dq * b.q0).normalize();
            b.desc.pose = Pose { p: b.center - q.rotate(b.com), q };
            b.desc.velocity = sb.v;
            b.desc.angular_velocity = sb.w;
            update_inertia(b);
        }
        self.active = slots;
        self.solver_bodies = sbodies;
        self.constraints = cons;
    }

    fn integrate_kinematic(&mut self, dt: f32) {
        let slots = std::mem::take(&mut self.active);
        for &s in &slots {
            let b = self.slot_mut(s);
            if b.desc.kind != BodyKind::Kinematic || !b.active() {
                continue;
            }
            match b.target.take() {
                Some(target) => {
                    b.desc.pose = target;
                    (b.desc.velocity, b.desc.angular_velocity) = b.declared_velocity;
                }
                None => {
                    let p = b.desc.pose.p + b.desc.velocity * dt;
                    let q = crate::math::integrate_rotation(b.desc.pose.q, b.desc.angular_velocity, dt);
                    b.desc.pose = Pose { p, q };
                }
            }
            b.center = b.desc.pose.point(b.com);
        }
        self.active = slots;
    }

    /// Fast dynamic bodies cast their inner sphere from where they were to where they are
    /// against static colliders, and stop at the first hit (Box2D v3's continuous step, with a
    /// sphere core).
    fn tunnelling_guard(&mut self, dt: f32) {
        let slots = std::mem::take(&mut self.active);
        let mut fast_total = 0;
        for &s in &slots {
            let b = self.slot(s);
            if !b.is_dynamic() || b.desc.sleeping || b.solver == NONE {
                continue;
            }
            let Some(col) = &b.desc.collider else { continue };
            if col.is_trigger {
                continue;
            }
            let delta = b.center - b.c0;
            let moved = delta.length();
            let r = b.inner_radius;
            if r <= 0.0 || moved <= 0.5 * r {
                continue;
            }
            let (layer, mask_bits, key, ccd, c0) = (col.layer, col.mask, b.key, b.desc.ccd, b.c0);
            fast_total += 1;
            if self.report.fast.len() < MAX_FAST_LISTED {
                self.report.fast.push(FastBody { key, speed: moved / dt, thickness: r });
            }
            if !ccd {
                continue;
            }
            let probe = Shape::sphere(r);
            let start = Pose::at(c0);
            let swept = Aabb::around(c0, Vec3::new(r, r, r)).swept(delta);
            let mut best: Option<(f32, u64, CastHit)> = None;
            let mut cands: Vec<u32> = Vec::new();
            self.static_tree.query(&swept, |o| {
                cands.push(o);
                true
            });
            for o in cands {
                let ob = self.slot(o);
                let Some(oc) = &ob.desc.collider else { continue };
                if oc.is_trigger || oc.mask & layer == 0 || mask_bits & oc.layer == 0 {
                    continue;
                }
                let op = ob.desc.pose.mul(&oc.offset);
                if let Some(h) = query::cast_shape(&probe, &start, delta, &oc.shape, &op, self.settings.linear_slop) {
                    if h.initially_overlapping {
                        continue;
                    }
                    if best.is_none_or(|(f, k, _)| h.fraction < f || h.fraction == f && ob.key < k) {
                        best = Some((h.fraction, ob.key, h));
                    }
                }
            }
            if let Some((f, other, h)) = best {
                if f < 1.0 {
                    let speed = moved / dt;
                    let b = self.slot_mut(s);
                    b.center = b.c0 + delta * f;
                    let q = b.desc.pose.q;
                    b.desc.pose.p = b.center - q.rotate(b.com);
                    if self.report.events.len() < MAX_EVENTS_PER_STEP {
                        self.report.events.push(Event { kind: EventKind::Ccd, a: key, b: other, point: h.point, normal: h.normal, speed });
                    }
                }
            }
        }
        self.report.fast_total = fast_total;
        self.active = slots;
    }

    fn sleep(&mut self, dt: f32) {
        let settings = self.settings;
        let slots = self.active.clone();
        // Sleep timers.
        let mut members: Vec<u32> = Vec::new();
        for &s in &slots {
            let b = self.slot_mut(s);
            if !b.is_dynamic() || b.desc.sleeping || b.solver == NONE {
                continue;
            }
            let lin = b.desc.velocity.length();
            let ang = b.desc.angular_velocity.length() * b.outer_radius;
            if !settings.sleep || !b.desc.can_sleep || lin.max(ang) > settings.sleep_threshold {
                b.sleep_time = 0.0;
            } else {
                b.sleep_time += dt;
            }
            members.push(s);
        }
        if !settings.sleep || members.is_empty() {
            return;
        }
        // Islands: union-find over touching awake dynamic pairs; a moving kinematic body keeps
        // what it touches awake.
        let n = self.slots.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(p: &mut [u32], mut x: u32) -> u32 {
            while p[x as usize] != x {
                p[x as usize] = p[p[x as usize] as usize];
                x = p[x as usize];
            }
            x
        }
        let mut keep_awake: Vec<u32> = Vec::new();
        for c in self.contacts.values() {
            if !c.sim || !c.touching || c.trigger {
                continue;
            }
            let (a, b) = (self.slot(c.a), self.slot(c.b));
            let (da, db) = (a.is_dynamic() && !a.desc.sleeping, b.is_dynamic() && !b.desc.sleeping);
            if da && db {
                let (ra, rb) = (find(&mut parent, c.a), find(&mut parent, c.b));
                if ra != rb {
                    // Lower slot becomes the root: deterministic.
                    let (lo, hi) = (ra.min(rb), ra.max(rb));
                    parent[hi as usize] = lo;
                }
            } else if da && b.desc.kind == BodyKind::Kinematic && b.active() {
                keep_awake.push(c.a);
            } else if db && a.desc.kind == BodyKind::Kinematic && a.active() {
                keep_awake.push(c.b);
            }
        }
        for s in keep_awake {
            self.slot_mut(s).sleep_time = 0.0;
        }
        // Minimum sleep time per island, in slot order.
        let mut min_time: BTreeMap<u32, f32> = BTreeMap::new();
        for &s in &members {
            let r = find(&mut parent, s);
            let t = self.slot(s).sleep_time;
            let e = min_time.entry(r).or_insert(f32::MAX);
            *e = e.min(t);
        }
        let mut groups: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &s in &members {
            let r = find(&mut parent, s);
            if min_time[&r] >= settings.time_to_sleep {
                groups.entry(r).or_default().push(s);
            }
        }
        for (_, group) in groups {
            let id = self.next_island;
            self.next_island = self.next_island.wrapping_add(1);
            let mut keys = Vec::with_capacity(group.len());
            for s in group {
                let b = self.slot_mut(s);
                b.desc.sleeping = true;
                b.desc.velocity = Vec3::ZERO;
                b.desc.angular_velocity = Vec3::ZERO;
                b.island = id;
                keys.push(b.key);
                self.awake.remove(&keys[keys.len() - 1]);
            }
            keys.sort_unstable();
            self.islands.insert(id, keys);
            self.report.islands_slept += 1;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Queries

    fn passes(&self, b: &Body, f: &Filter) -> bool {
        let Some(c) = &b.desc.collider else { return false };
        if f.exclude == Some(b.key) || c.layer & f.mask == 0 {
            return false;
        }
        f.triggers || !c.is_trigger
    }

    /// Ray hits sorted by distance (ties by key), at most `max_hits`, and how many there were.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32, filter: &Filter, max_hits: usize) -> (Vec<Hit>, usize) {
        let Some(d) = crate::math::try_normalize(dir) else { return (Vec::new(), 0) };
        let mut hits: Vec<Hit> = Vec::new();
        let mut visit = |s: u32, cur: f32| -> f32 {
            let b = self.slot(s);
            if !self.passes(b, filter) {
                return cur;
            }
            let c = b.desc.collider.as_ref().expect("passes");
            if let Some(RayHit { distance, point, normal, triangle }) = query::ray_shape(&c.shape, &b.desc.pose.mul(&c.offset), origin, d, max) {
                hits.push(Hit { key: b.key, at: distance, point, normal, triangle, initially_overlapping: false });
            }
            cur
        };
        self.static_tree.ray(origin, d, max, &mut visit);
        self.dynamic_tree.ray(origin, d, max, &mut visit);
        hits.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.key.cmp(&b.key)));
        let total = hits.len();
        hits.truncate(max_hits);
        (hits, total)
    }

    /// Shape-cast hits (convex `shape` from `pose` along `delta`), sorted by fraction then key.
    pub fn cast(&self, shape: &Shape, pose: &Pose, delta: Vec3, filter: &Filter, max_hits: usize, skin: f32) -> (Vec<Hit>, usize) {
        let swept = shape.aabb(pose).swept(delta).expand(skin);
        let mut hits: Vec<Hit> = Vec::new();
        for s in self.candidates_in(&swept) {
            let b = self.slot(s);
            if !self.passes(b, filter) {
                continue;
            }
            let c = b.desc.collider.as_ref().expect("passes");
            if let Some(h) = query::cast_shape(shape, pose, delta, &c.shape, &b.desc.pose.mul(&c.offset), skin) {
                hits.push(Hit { key: b.key, at: h.fraction, point: h.point, normal: h.normal, triangle: h.triangle, initially_overlapping: h.initially_overlapping });
            }
        }
        hits.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.key.cmp(&b.key)));
        let total = hits.len();
        hits.truncate(max_hits);
        (hits, total)
    }

    /// Keys of bodies overlapping `shape` at `pose`, sorted, at most `limit`, and the total.
    pub fn overlap(&self, shape: &Shape, pose: &Pose, filter: &Filter, limit: usize) -> (Vec<u64>, usize) {
        let bounds = shape.aabb(pose);
        let mut keys: Vec<u64> = Vec::new();
        for s in self.candidates_in(&bounds) {
            let b = self.slot(s);
            if !self.passes(b, filter) {
                continue;
            }
            let c = b.desc.collider.as_ref().expect("passes");
            if query::overlap(shape, pose, &c.shape, &b.desc.pose.mul(&c.offset)) {
                keys.push(b.key);
            }
        }
        keys.sort_unstable();
        let total = keys.len();
        keys.truncate(limit);
        (keys, total)
    }

    /// Keys of bodies whose boxes overlap `bounds`, sorted (both trees).
    pub fn in_box(&self, bounds: &Aabb, limit: usize) -> (Vec<u64>, usize) {
        let mut keys: Vec<u64> = self.candidates_in(bounds).into_iter().map(|s| self.slot(s).key).collect();
        keys.sort_unstable();
        let total = keys.len();
        keys.truncate(limit);
        (keys, total)
    }

    /// Slots whose fat boxes overlap `b`, in key order.
    fn candidates_in(&self, b: &Aabb) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        self.static_tree.query(b, |s| {
            out.push(s);
            true
        });
        self.dynamic_tree.query(b, |s| {
            out.push(s);
            true
        });
        out.sort_by_key(|s| self.slot(*s).key);
        out
    }

    /// Why a body collides (or doesn't) with what is near it: bodies whose boxes overlap its
    /// own, each with whether they pair, and if not, why (layers, triggers, kinds).
    pub fn neighbours(&self, key: u64, limit: usize) -> Vec<(u64, Option<&'static str>)> {
        let Some(b) = self.get(key) else { return Vec::new() };
        let Some(bounds) = b.aabb() else { return Vec::new() };
        let mut out = Vec::new();
        for s in self.candidates_in(&bounds.expand(self.settings.aabb_margin)) {
            let o = self.slot(s);
            if o.key == key {
                continue;
            }
            let why = match (&b.desc.collider, &o.desc.collider) {
                (Some(ca), Some(cb)) if ca.mask & cb.layer == 0 || cb.mask & ca.layer == 0 => Some("layer_mask"),
                (Some(ca), Some(cb)) if ca.is_trigger && cb.is_trigger => Some("both_triggers"),
                _ if Space::should_pair(b, o) => None,
                _ => Some("no_dynamic_body"),
            };
            out.push((o.key, why));
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    // -----------------------------------------------------------------------------------------
    // Hash

    /// A 64-bit hash of the whole simulation state (bodies, contacts with their impulses,
    /// sleep state, characters), in key order: equal hashes mean bit-identical states.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x5068_7973_6963_7331);
        h.write_u64(self.by_key.len() as u64);
        for (&key, &s) in &self.by_key {
            let b = self.slot(s);
            h.write_u64(key);
            h.write_u8(b.desc.kind as u8);
            for v in [b.desc.pose.p, b.desc.velocity, b.desc.angular_velocity] {
                h.write_f32(v.x);
                h.write_f32(v.y);
                h.write_f32(v.z);
            }
            let q = b.desc.pose.q;
            [q.x, q.y, q.z, q.w].iter().for_each(|c| h.write_f32(*c));
            h.write_bool(b.desc.sleeping);
            h.write_f32(b.sleep_time);
            h.write_bool(b.target.is_some());
        }
        h.write_u64(self.contacts.len() as u64);
        for (&(a, b), c) in &self.contacts {
            h.write_u64(a);
            h.write_u64(b);
            h.write_bool(c.touching);
            for p in &c.points[..c.len] {
                h.write_u32(p.id);
                h.write_f32(p.normal_impulse);
                h.write_f32(p.tangent_impulse[0]);
                h.write_f32(p.tangent_impulse[1]);
            }
        }
        for (k, c) in &self.characters {
            h.write_u64(*k);
            h.write_bool(c.grounded);
            h.write_u64(c.ground.unwrap_or(u64::MAX));
        }
        h.finish()
    }
}

impl Body {
    fn inertia_world(&self) -> Mat3 {
        self.inertia_local.rotated(&Mat3::from_quat(self.desc.pose.q))
    }
}

fn fat_of(st: &Tree, dt: &Tree, b: &Body) -> Aabb {
    if b.in_static_tree {
        st.fat_aabb(b.proxy)
    } else {
        dt.fat_aabb(b.proxy)
    }
}

fn end_event(c: &Contact, ka: u64, kb: u64) -> Event {
    let kind = if c.trigger { EventKind::TriggerExit } else { EventKind::ContactEnd };
    Event { kind, a: ka, b: kb, point: Vec3::ZERO, normal: Vec3::ZERO, speed: 0.0 }
}

/// A contact normal is stored from body `c.a` to `c.b`; the pair key orders by key. Pairs are
/// created with `a` = the lower key, so this is the identity, kept for clarity.
fn oriented(n: Vec3, a_key: u64, low_key: u64) -> Vec3 {
    if a_key == low_key {
        n
    } else {
        -n
    }
}

fn validate(d: &BodyDesc) -> Result<(), PhysicsError> {
    let finite = |v: Vec3| crate::math::is_finite(v);
    if !finite(d.pose.p) || !(d.pose.q.x.is_finite() && d.pose.q.y.is_finite() && d.pose.q.z.is_finite() && d.pose.q.w.is_finite()) {
        return Err(PhysicsError::new("invalid_body", "the body's pose must be finite").with("field", "pose"));
    }
    if !finite(d.velocity) || !finite(d.angular_velocity) {
        return Err(PhysicsError::new("invalid_body", "velocities must be finite").with("field", "velocity"));
    }
    if d.kind == BodyKind::Dynamic && !(d.mass > 0.0 && d.mass.is_finite()) {
        return Err(PhysicsError::new("invalid_body", "a dynamic body needs a positive mass").with("field", "mass"));
    }
    if let Some(c) = &d.collider {
        if c.shape.is_static_only() && d.kind != BodyKind::Static {
            return Err(PhysicsError::new(
                "static_only_shape",
                format!("'{}' colliders have no volume, so only static bodies can use them; use a box, sphere, capsule or hull", c.shape.name()),
            )
            .with("field", "collider.shape")
            .with("kind", d.kind.name()));
        }
        if c.layer.count_ones() != 1 {
            return Err(PhysicsError::new("invalid_body", "a collider is on exactly one layer").with("field", "collider.layer"));
        }
        if !(c.friction >= 0.0 && c.friction.is_finite() && (0.0..=1.0).contains(&c.restitution)) {
            return Err(PhysicsError::new("invalid_body", "friction must be >= 0 and restitution 0..1").with("field", "collider.material"));
        }
    }
    Ok(())
}

/// Mass, inertia and radii from the description.
fn refresh_mass(b: &mut Body, s: &Settings) {
    let _ = s;
    b.inner_radius = 0.0;
    b.outer_radius = 0.5;
    match (&b.desc.collider, b.desc.kind) {
        (Some(c), BodyKind::Dynamic) => {
            let (com_shape, inertia_shape) = c.shape.mass_properties(b.desc.mass);
            // Into the body frame: rotate the inertia by the collider's offset, then shift.
            let r = Mat3::from_quat(c.offset.q);
            b.com = c.offset.point(com_shape);
            b.inertia_local = inertia_shape.rotated(&r);
            b.inv_mass = 1.0 / b.desc.mass;
        }
        (None, BodyKind::Dynamic) => {
            b.com = Vec3::ZERO;
            b.inertia_local = Mat3::diag(Vec3::ONE * (0.1 * b.desc.mass));
            b.inv_mass = 1.0 / b.desc.mass;
        }
        _ => {
            b.com = b.desc.collider.as_ref().map_or(Vec3::ZERO, |c| c.offset.p);
            b.inertia_local = Mat3::ZERO;
            b.inv_mass = 0.0;
        }
    }
    if let Some(c) = &b.desc.collider {
        b.inner_radius = c.shape.inner_radius();
        b.outer_radius = c.shape.outer_radius() + (b.com - c.offset.p).length();
    }
    b.center = b.desc.pose.point(b.com);
    update_inertia(b);
}

fn update_inertia(b: &mut Body) {
    if b.inv_mass == 0.0 {
        b.inv_inertia_world = Mat3::ZERO;
        return;
    }
    let r = Mat3::from_quat(b.desc.pose.q);
    b.inv_inertia_world = b.inertia_local.inverse().rotated(&r);
}

fn locks(b: &Body, plane_2d: bool) -> ([bool; 3], [bool; 3]) {
    let mut l = b.desc.lock_linear;
    let mut a = b.desc.lock_angular;
    if plane_2d && b.desc.kind == BodyKind::Dynamic {
        l[2] = true;
        a[0] = true;
        a[1] = true;
    }
    (l, a)
}

fn mask(v: &mut Vec3, axes: [bool; 3]) {
    if axes[0] {
        v.x = 0.0;
    }
    if axes[1] {
        v.y = 0.0;
    }
    if axes[2] {
        v.z = 0.0;
    }
}

/// `P · M · P` with `P` zeroing the locked world axes.
fn lock_inertia(m: Mat3, axes: [bool; 3]) -> Mat3 {
    if !axes.iter().any(|a| *a) {
        return m;
    }
    let mut out = m;
    for (c, col) in out.c.iter_mut().enumerate() {
        if axes[c] {
            *col = Vec3::ZERO;
        } else {
            mask(col, axes);
        }
    }
    out
}

/// The angular velocity that turns `from` into `to` in `1 / inv_dt` seconds (shortest way).
fn angular_velocity_to(from: Quat, to: Quat, inv_dt: f32) -> Vec3 {
    let mut d = (to * from.conjugate()).normalize();
    if d.w < 0.0 {
        d = Quat { x: -d.x, y: -d.y, z: -d.z, w: -d.w };
    }
    let v = Vec3::new(d.x, d.y, d.z);
    let s = v.length();
    if s < 1.0e-9 {
        return Vec3::ZERO;
    }
    let angle = 2.0 * specula_core::dmath::atan2(s, d.w);
    v * (angle / s * inv_dt)
}

