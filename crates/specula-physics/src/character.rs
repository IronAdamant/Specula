//! The kinematic character controller (P2): a body moved by game intent, not by forces.
//!
//! Each move sweeps the character's own collider (a capsule, usually) through the world and
//! slides along what it hits:
//!
//! 1. **Depenetrate**: push out of anything the character starts inside.
//! 2. **Slide**: cast along the motion; on a hit, move to the contact (minus a skin) and keep
//!    the part of the motion along the surface, up to `iterations` times. Too-steep surfaces
//!    count as walls (their normal is flattened), so slopes past the limit can't be climbed;
//!    on the ground, a rounded collider still rides over edges lower than `step_height`.
//! 3. **Step up**: blocked by a wall while on the ground? Try again lifted by `step_height`,
//!    then drop back down; keep it when it lands on ground further on.
//! 4. **Snap**: on the ground last move and not jumping, stick to ground within
//!    `snap_distance` below (walking down slopes and stairs without hopping).
//! 5. **Ground probe**: what is underfoot (normal, body), for the next move. The edge of a
//!    walkable surface counts as ground (a short ray just inside the edge checks the surface).
//!
//! It also inherits the velocity of a moving (kinematic) platform underfoot and pushes dynamic
//! bodies it walks into with a capped impulse. Everything is deterministic: casts return hits
//! in a fixed order and the controller's memory (grounded, ground) is part of the state hash.

use crate::math::{normalize_or, try_normalize, Pose, Quat, Vec3};
use crate::space::{BodyKind, CharacterState, Filter, Hit, Space};
use crate::PhysicsError;

/// How a character moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterConfig {
    pub up: Vec3,
    /// Steepest walkable slope, degrees.
    pub max_slope_deg: f32,
    /// Highest ledge it climbs without jumping, m.
    pub step_height: f32,
    /// Ground within this distance below sticks (walking downhill), m.
    pub snap_distance: f32,
    /// Gap kept from surfaces, m.
    pub skin: f32,
    pub iterations: u32,
    /// Largest force it pushes dynamic bodies with, N.
    pub push_force: f32,
    /// Gravity applies (the vertical part of the velocity integrates it).
    pub gravity: bool,
}

impl Default for CharacterConfig {
    fn default() -> CharacterConfig {
        CharacterConfig {
            up: Vec3::Y,
            max_slope_deg: 45.0,
            step_height: 0.3,
            snap_distance: 0.2,
            skin: 0.01,
            iterations: 4,
            push_force: 200.0,
            gravity: true,
        }
    }
}

/// What one move did.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterMove {
    /// The new body position (rotation is left alone).
    pub position: Vec3,
    pub velocity: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub ground: Option<u64>,
    /// Climbed a step this move.
    pub stepped: bool,
    /// Part of the horizontal motion was stopped by an obstacle.
    pub blocked: bool,
    /// What it touched this move: `(key, normal)`, at most 8.
    pub hits: Vec<(u64, Vec3)>,
    /// Dynamic bodies it walked into: `(key, impulse N·s, at)`. [`Space::move_character`]
    /// applies them; [`Space::plan_character`] only lists them.
    pub pushes: Vec<(u64, Vec3, Vec3)>,
}

/// A character's collider and pose for a move: a body in the space, or one that lives
/// elsewhere (the engine's module controller plans moves for entities without a body).
#[derive(Clone, Copy, Debug)]
pub struct CharacterBody<'a> {
    pub shape: &'a crate::Shape,
    /// The collider's pose in the body frame.
    pub offset: Pose,
    /// The body frame in the world.
    pub pose: Pose,
    /// The layers it collides with.
    pub mask: u64,
    /// Its own body, which its casts ignore.
    pub exclude: Option<u64>,
}

const MAX_HITS: usize = 8;

/// What one move works with.
struct Mover<'a> {
    shape: &'a crate::Shape,
    offset: Pose,
    rot: Quat,
    filter: Filter,
    up: Vec3,
    min_ground_dot: f32,
    skin: f32,
    cfg: &'a CharacterConfig,
    /// From the body origin down to the collider's lowest point, along `up`.
    feet: f32,
}

impl Space {
    /// Moves character `key` (a kinematic body with a convex collider) for one tick of `dt`
    /// seconds at `velocity` (m/s; its horizontal part is the walking intent, its vertical part
    /// jumps and falls). Returns the move, with the new velocity (gravity, landing, ceilings).
    pub fn move_character(&mut self, key: u64, velocity: Vec3, cfg: &CharacterConfig, dt: f32) -> Result<CharacterMove, PhysicsError> {
        let body = self.body(key).ok_or_else(|| PhysicsError::new("not_found", format!("no physics body {key}")).with("key", key))?;
        if body.kind != BodyKind::Kinematic {
            return Err(PhysicsError::new("not_kinematic", "characters are kinematic bodies").with("key", key));
        }
        let col =
            body.collider.ok_or_else(|| PhysicsError::new("no_collider", "a character needs a collider (a capsule)").with("key", key))?;
        if col.shape.is_static_only() {
            return Err(PhysicsError::new("static_only_shape", "a character's collider must be convex (capsule, sphere, box or hull)")
                .with("key", key));
        }
        let shape = col.shape.clone();
        let rot = body.pose.q;
        let c = CharacterBody { shape: &shape, offset: col.offset, pose: body.pose, mask: col.mask, exclude: Some(key) };
        let prev = self.characters.get(&key).copied().unwrap_or_default();
        let mv = self.plan_character(&c, velocity, cfg, dt, prev);
        for (k, imp, at) in &mv.pushes {
            self.apply_impulse(*k, *imp, *at);
        }
        let state = CharacterState { grounded: mv.grounded, ground_normal: mv.ground_normal, ground: mv.ground };
        self.characters.insert(key, state);
        self.set_pose(key, Pose { p: mv.position, q: rot });
        Ok(mv)
    }

    /// The mover for a character body.
    fn mover<'a>(c: &CharacterBody<'a>, cfg: &'a CharacterConfig) -> Mover<'a> {
        let up = normalize_or(cfg.up, Vec3::Y);
        let rot = c.pose.q;
        // The collider's lowest point along `up`, relative to the body origin.
        let b = c.shape.aabb(&Pose { p: Vec3::ZERO, q: rot }.mul(&c.offset));
        let lowest = (b.min.x * up.x).min(b.max.x * up.x) + (b.min.y * up.y).min(b.max.y * up.y) + (b.min.z * up.z).min(b.max.z * up.z);
        Mover {
            shape: c.shape,
            offset: c.offset,
            rot,
            filter: Filter { mask: c.mask, triggers: false, exclude: c.exclude },
            up,
            min_ground_dot: specula_core::dmath::cos(cfg.max_slope_deg.clamp(0.0, 89.9).to_radians()),
            skin: cfg.skin.max(1.0e-4),
            cfg,
            feet: (-lowest).max(0.0),
        }
    }

    /// What a character at its pose stands on now (within a few skins below): the memory a
    /// stateless controller starts each move from.
    pub fn character_ground(&self, c: &CharacterBody, cfg: &CharacterConfig) -> CharacterState {
        let m = Space::mover(c, cfg);
        match self.probe(&m, c.pose.p, 3.0 * m.skin) {
            Some(h) => CharacterState { grounded: true, ground_normal: h.normal, ground: Some(h.key) },
            None => CharacterState::default(),
        }
    }

    /// A move of `c` planned from memory `prev`, changing nothing: [`Space::move_character`]
    /// without its effects (the new position, the controller's memory and the pushes are the
    /// caller's to apply).
    pub fn plan_character(&self, c: &CharacterBody, velocity: Vec3, cfg: &CharacterConfig, dt: f32, prev: CharacterState) -> CharacterMove {
        let m = Space::mover(c, cfg);
        let up = m.up;
        let body_pose = c.pose;
        let mut v = velocity;

        // Vertical velocity: gravity in the air, none while standing.
        let g_up = self.settings.gravity.dot(up);
        let vy = v.dot(up);
        if cfg.gravity {
            if prev.grounded && vy <= 0.0 {
                v = v - up * vy;
            } else {
                v = v + up * (g_up * dt);
            }
        }
        // A moving platform underfoot carries the character.
        let mut carry = Vec3::ZERO;
        if let (true, Some(g)) = (prev.grounded, prev.ground) {
            if self.body(g).is_some_and(|gb| gb.kind == BodyKind::Kinematic) {
                carry = self.next_velocity(g, dt) * dt;
            }
        }
        let motion = v * dt + carry;
        let vertical = up * motion.dot(up);
        let horizontal = motion - vertical;

        let mut hits: Vec<(u64, Vec3)> = Vec::new();
        let note = |hits: &mut Vec<(u64, Vec3)>, h: &Hit| {
            if hits.len() < MAX_HITS && !hits.iter().any(|(k, _)| *k == h.key) {
                hits.push((h.key, h.normal));
            }
        };

        // 1. Depenetrate.
        let start = self.depenetrate(&m, body_pose.p);
        // 2. Slide: horizontal (riding over low edges while on the ground), then vertical.
        let (p1, h1) = self.slide(&m, start, horizontal, true, prev.grounded);
        h1.iter().for_each(|h| note(&mut hits, h));
        let flat_progress = horizontal_len(p1 - start, up);
        let (mut p2, h2) = self.slide(&m, p1, vertical, false, false);
        h2.iter().for_each(|h| note(&mut hits, h));
        let mut stepped = false;

        // 3. Step up, when blocked on the ground.
        let wanted = horizontal_len(horizontal, up);
        if prev.grounded && cfg.step_height > 0.0 && wanted > 1.0e-5 && wanted - flat_progress > 1.0e-4 {
            let (lifted, _) = self.slide(&m, start, up * cfg.step_height, false, false);
            let lift = (lifted - start).dot(up);
            if lift > m.skin {
                let (side, hs) = self.slide(&m, lifted, horizontal, true, false);
                if horizontal_len(side - start, up) > flat_progress + 1.0e-4 {
                    let drop = up * -(lift + cfg.snap_distance.max(m.skin));
                    let (down, _) = self.cast_body(&m, side, drop);
                    if let Some(h) = down.into_iter().find(|h| !h.initially_overlapping && self.is_ground(&m, h)) {
                        p2 = side + drop * h.at;
                        stepped = true;
                        hs.iter().for_each(|x| note(&mut hits, x));
                        note(&mut hits, &h);
                    }
                }
            }
        }

        // 4. Snap to the ground below, 5. and see what is underfoot.
        let mut ground: Option<Hit> = None;
        if prev.grounded && v.dot(up) <= 0.0 && !stepped {
            let reach = cfg.snap_distance.max(2.0 * m.skin);
            if let Some(h) = self.probe(&m, p2, reach) {
                if !h.initially_overlapping {
                    p2 = p2 - up * (reach * h.at);
                }
                ground = Some(h);
            }
        }
        if ground.is_none() {
            ground = self.probe(&m, p2, 3.0 * m.skin);
        }
        let grounded = ground.is_some() && v.dot(up) <= 1.0e-4;
        if grounded && v.dot(up) < 0.0 {
            v = v - up * v.dot(up);
        }
        // Bumped a ceiling while rising.
        if v.dot(up) > 0.0 && hits.iter().any(|(_, n)| n.dot(up) < -0.7) {
            v = v - up * v.dot(up);
        }
        // Push dynamic bodies walked into.
        let mut pushes: Vec<(u64, Vec3, Vec3)> = Vec::new();
        if cfg.push_force > 0.0 {
            let impulse_limit = cfg.push_force * dt;
            pushes = hits
                .iter()
                .filter_map(|(k, n)| {
                    let b = self.body(*k)?;
                    if b.kind != BodyKind::Dynamic {
                        return None;
                    }
                    let dir = try_normalize(-*n - up * (-*n).dot(up))?;
                    let speed = horizontal.dot(dir) / dt.max(1.0e-6);
                    if speed <= 0.0 {
                        return None;
                    }
                    Some((*k, dir * (b.mass * speed).min(impulse_limit), b.center))
                })
                .collect();
        }
        let blocked = wanted > 1.0e-5 && horizontal_len(p2 - start, up) < wanted - 1.0e-4;
        CharacterMove {
            position: p2,
            velocity: v,
            grounded,
            ground_normal: ground.map_or(Vec3::ZERO, |h| h.normal),
            ground: ground.map(|h| h.key),
            stepped,
            blocked,
            hits,
            pushes,
        }
    }

    /// Casts the character's collider (its body at `at`) along `delta`.
    fn cast_body(&self, m: &Mover, at: Vec3, delta: Vec3) -> (Vec<Hit>, usize) {
        let pose = Pose { p: at, q: m.rot }.mul(&m.offset);
        self.cast(m.shape, &pose, delta, &m.filter, 4, m.skin)
    }

    /// Ground within `dist` below the body at `at`.
    fn probe(&self, m: &Mover, at: Vec3, dist: f32) -> Option<Hit> {
        let (hits, _) = self.cast_body(m, at, m.up * -dist);
        hits.into_iter().find(|h| self.is_ground(m, h))
    }

    /// Whether a hit is something to stand on: a walkable surface, or the edge of one (a
    /// rounded collider resting on a corner gets a tilted normal; a short ray just inside the
    /// edge finds the surface itself).
    fn is_ground(&self, m: &Mover, h: &Hit) -> bool {
        let up_dot = h.normal.dot(m.up);
        if up_dot >= m.min_ground_dot {
            return true;
        }
        if up_dot <= 0.05 {
            return false;
        }
        let Some(inward) = try_normalize(-(h.normal - m.up * up_dot)) else {
            return false;
        };
        let origin = h.point + m.up * 0.02 + inward * 0.01;
        let (found, _) = self.raycast(origin, -m.up, 0.06, &m.filter, 1);
        found.first().is_some_and(|r| r.normal.dot(m.up) >= m.min_ground_dot)
    }

    /// Moves along `motion`, sliding along what it hits. `horizontal`: steep surfaces are walls
    /// (no climbing), except edges lower than the step height when `climb` (on the ground: a
    /// rounded collider rides over them).
    fn slide(&self, m: &Mover, mut pos: Vec3, motion: Vec3, horizontal: bool, climb: bool) -> (Vec3, Vec<Hit>) {
        let mut remaining = motion;
        let mut hits = Vec::new();
        let original = motion;
        for _ in 0..m.cfg.iterations.max(1) {
            if remaining.dot(remaining) < 1.0e-12 {
                break;
            }
            let (found, _) = self.cast_body(m, pos, remaining);
            let Some(h) = found.into_iter().find(|h| !h.initially_overlapping || h.normal.dot(remaining) < 0.0) else {
                pos = pos + remaining;
                break;
            };
            if !h.initially_overlapping {
                pos = pos + remaining * h.at;
                remaining = remaining * (1.0 - h.at);
            }
            let mut n = h.normal;
            let up_dot = n.dot(m.up);
            let walkable = up_dot >= m.min_ground_dot;
            if !walkable && (horizontal || remaining.dot(m.up) > 0.0) {
                let feet = pos.dot(m.up) - m.feet;
                let low_edge = climb && up_dot > 0.05 && h.point.dot(m.up) - feet <= m.cfg.step_height && self.is_ground(m, &h);
                if !low_edge {
                    // Too steep: a wall (no climbing it).
                    if let Some(flat) = try_normalize(n - m.up * up_dot) {
                        n = flat;
                    }
                }
            }
            remaining = remaining - n * remaining.dot(n);
            hits.push(h);
            // Don't turn back (corners).
            if remaining.dot(original) <= 0.0 {
                break;
            }
        }
        (pos, hits)
    }

    fn depenetrate(&self, m: &Mover, mut pos: Vec3) -> Vec3 {
        for _ in 0..4 {
            let pose = Pose { p: pos, q: m.rot }.mul(&m.offset);
            let bounds = m.shape.aabb(&pose);
            let (keys, _) = self.in_box(&bounds, 64);
            let mut push = Vec3::ZERO;
            let mut deepest = 0.0f32;
            for k in keys {
                if m.filter.exclude == Some(k) {
                    continue;
                }
                let Some(b) = self.body(k) else { continue };
                let Some(c) = b.collider else { continue };
                if c.is_trigger || c.layer & m.filter.mask == 0 {
                    continue;
                }
                let mut man = crate::contact::Manifold::default();
                crate::contact::collide(m.shape, &pose, &c.shape, &b.pose.mul(&c.offset), 0.0, &mut man);
                for p in man.as_slice() {
                    if p.separation < -m.skin * 0.5 && p.separation < deepest {
                        deepest = p.separation;
                        // The normal points from the character into the other: push back.
                        push = p.normal * (p.separation - m.skin * 0.5);
                    }
                }
            }
            if deepest == 0.0 {
                break;
            }
            pos = pos + push;
        }
        pos
    }
}

fn horizontal_len(v: Vec3, up: Vec3) -> f32 {
    (v - up * v.dot(up)).length()
}
