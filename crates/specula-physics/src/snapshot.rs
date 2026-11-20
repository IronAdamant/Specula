//! Save games: the space's hidden state as bytes (docs/design/SAVES.md).
//!
//! A world file holds what physics shows (positions, Euler rotations, velocities, sleep flags),
//! not what it remembers: exact quaternions, warm-start impulses, sleep timers and islands, the
//! per-pair polytope memory, fat broadphase boxes, kinematic targets, the characters' ground
//! memory. A reloaded world therefore continues deterministically but not bit-identically.
//! [`Space::save_state`] writes all of that (floats as exact bits) and [`Space::restore_state`]
//! puts it back into a space rebuilt from the same world (same bodies, same shapes), so the
//! loaded run continues **bit-identically**: the same [`Space::hash`] on every later step.
//!
//! **Cosmetic bodies are left out** (with their contacts and joints): they are presentation
//! (transient entities' ragdolls, which saves don't keep either), outside the hash, and never
//! touch a simulated body, so the loaded run's simulation continues bit-identically without them.
//!
//! What is not saved, because it is derived or rebuilt: shapes (built from the world's
//! colliders), slot and proxy numbering and the trees' internal layout (every result is sorted
//! by key, so they never reach the simulation), solver buffers and the last step's report.
//! Contact geometry (points, normals, separations) is recomputed by every narrowphase but is
//! saved anyway, so a restored space reads back the same contacts before its next step.

use super::*;
use crate::joint::{Drive, JointKind};
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// Bumped when the layout changes. Version 2 adds the joint settings and joints' hidden state
/// (warm starts, last state, the step that solved them); version 1 snapshots (no joints) still
/// load. Version 3 adds, to each contact, the fastest its two bodies closed while it formed
/// (what its begin event says: the rules of a game hear it, so a restored run must say the
/// same); in an older snapshot it is 0. Others are refused with `unsupported_version`.
pub const SNAPSHOT_VERSION: u32 = 3;

/// Why a joint wasn't solved, as saved (index + 1; 0: it was).
const INACTIVE: [&str; 4] = ["missing_body", "asleep", "no_dynamic_body", "cosmetic_link"];

fn decode(e: DecodeError) -> PhysicsError {
    PhysicsError::new("bad_snapshot", format!("the physics snapshot is damaged: {e}"))
        .with("decode", e.code)
        .with("offset", e.offset)
        .with("field", e.field.as_str())
}

fn mismatch(message: String, key: u64) -> PhysicsError {
    PhysicsError::new("snapshot_mismatch", message).with("key", key)
}

fn w_pose(w: &mut ByteWriter, p: &Pose) {
    w.vec3(p.p);
    w.quat(p.q);
}

fn r_pose(r: &mut ByteReader<'_>, f: &str) -> Result<Pose, DecodeError> {
    Ok(Pose { p: r.vec3(f)?, q: r.quat(f)? })
}

fn w_mat3(w: &mut ByteWriter, m: &Mat3) {
    m.c.iter().for_each(|c| w.vec3(*c));
}

fn r_mat3(r: &mut ByteReader<'_>, f: &str) -> Result<Mat3, DecodeError> {
    Ok(Mat3 { c: [r.vec3(f)?, r.vec3(f)?, r.vec3(f)?] })
}

fn w_settings(w: &mut ByteWriter, s: &Settings) {
    w.vec3(s.gravity);
    w.u32(s.substeps);
    w.bool(s.sleep);
    for v in [
        s.contact_hertz,
        s.contact_damping_ratio,
        s.push_speed,
        s.restitution_threshold,
        s.linear_slop,
        s.speculative_distance,
        s.aabb_margin,
        s.max_linear_speed,
        s.sleep_threshold,
        s.time_to_sleep,
    ] {
        w.f32(v);
    }
    w.bool(s.plane_2d);
    w.u64(s.max_bodies as u64);
    w.u64(s.max_pairs as u64);
    w.f32(s.joint_hertz);
    w.f32(s.joint_damping_ratio);
    for n in [s.max_joints, s.max_cosmetic_bodies, s.max_cosmetic_pairs, s.threads] {
        w.u64(n as u64);
    }
    w.bool(s.coloured);
    w.bool(s.wide);
}

fn r_settings(r: &mut ByteReader<'_>, version: u32) -> Result<Settings, DecodeError> {
    let f = "settings";
    let gravity = r.vec3(f)?;
    let substeps = r.u32(f)?;
    let sleep = r.bool(f)?;
    let mut v = [0.0f32; 10];
    for x in &mut v {
        *x = r.f32(f)?;
    }
    let plane_2d = r.bool(f)?;
    let max_bodies = r.u64(f)?.min(1 << 32) as usize;
    let max_pairs = r.u64(f)?.min(1 << 32) as usize;
    let d = Settings::default();
    let (mut joint_hertz, mut joint_damping_ratio) = (d.joint_hertz, d.joint_damping_ratio);
    let mut counts = [d.max_joints, d.max_cosmetic_bodies, d.max_cosmetic_pairs, d.threads];
    let mut coloured = d.coloured;
    let mut wide = d.wide;
    if version >= 2 {
        joint_hertz = r.f32(f)?;
        joint_damping_ratio = r.f32(f)?;
        for c in &mut counts {
            *c = r.u64(f)?.min(1 << 32) as usize;
        }
        coloured = r.bool(f)?;
        wide = r.bool(f)?;
    }
    // Threads: at least one, and no more than a machine has (the results never depend on it).
    counts[3] = counts[3].clamp(1, 64);
    Ok(Settings {
        joint_hertz,
        joint_damping_ratio,
        max_joints: counts[0],
        max_cosmetic_bodies: counts[1],
        max_cosmetic_pairs: counts[2],
        threads: counts[3],
        coloured,
        wide,
        gravity,
        substeps,
        sleep,
        contact_hertz: v[0],
        contact_damping_ratio: v[1],
        push_speed: v[2],
        restitution_threshold: v[3],
        linear_slop: v[4],
        speculative_distance: v[5],
        aabb_margin: v[6],
        max_linear_speed: v[7],
        sleep_threshold: v[8],
        time_to_sleep: v[9],
        plane_2d,
        max_bodies,
        max_pairs,
    })
}

/// A joint's description as bytes (for engine-made joints a save keeps whole: a ragdoll's,
/// whose opened limits and motor targets aren't in the world).
pub fn write_joint_desc(w: &mut ByteWriter, d: &JointDesc) {
    w.u8(d.kind as u8);
    w.opt_u64(d.a);
    w.opt_u64(d.b);
    w_pose(w, &d.frame_a);
    w_pose(w, &d.frame_b);
    for r in [d.limit, d.swing] {
        w.bool(r.is_some());
        if let Some([lo, hi]) = r {
            w.f32(lo);
            w.f32(hi);
        }
    }
    for x in [d.length[0], d.length[1], d.spring_hertz, d.spring_damping, d.rest_length, d.friction] {
        w.f32(x);
    }
    w.bool(d.collide_connected);
    match d.motor {
        None => w.u8(0),
        Some(m) => {
            match m.drive {
                Drive::Speed(s) => {
                    w.u8(1);
                    w.f32(s);
                }
                Drive::Position(p) => {
                    w.u8(2);
                    w.f32(p);
                }
                Drive::Rotation(q) => {
                    w.u8(3);
                    w.quat(q);
                }
            }
            w.f32(m.max_force);
            w.f32(m.hertz);
            w.f32(m.damping_ratio);
        }
    }
}

/// Reads [`write_joint_desc`]'s bytes, checked as [`crate::joint::validate`] checks a new joint.
pub fn read_joint_desc(r: &mut ByteReader<'_>) -> Result<JointDesc, PhysicsError> {
    let f = "joint";
    let d = |e| decode(e);
    let kind = *JointKind::ALL.get(r.u8(f).map_err(d)? as usize).ok_or_else(|| decode(r.error("invalid", f, "unknown joint kind")))?;
    let a = r.opt_u64(f).map_err(d)?;
    let b = r.opt_u64(f).map_err(d)?;
    let frame_a = r_pose(r, f).map_err(d)?;
    let frame_b = r_pose(r, f).map_err(d)?;
    let mut ranges = [None, None];
    for x in &mut ranges {
        if r.bool(f).map_err(d)? {
            *x = Some([r.f32(f).map_err(d)?, r.f32(f).map_err(d)?]);
        }
    }
    let mut v = [0.0f32; 6];
    for x in &mut v {
        *x = r.f32(f).map_err(d)?;
    }
    let collide_connected = r.bool(f).map_err(d)?;
    let drive = match r.u8(f).map_err(d)? {
        0 => None,
        1 => Some(Drive::Speed(r.f32(f).map_err(d)?)),
        2 => Some(Drive::Position(r.f32(f).map_err(d)?)),
        3 => Some(Drive::Rotation(r.quat(f).map_err(d)?)),
        _ => return Err(decode(r.error("invalid", f, "unknown motor drive"))),
    };
    let motor = match drive {
        None => None,
        Some(drive) => Some(Motor { drive, max_force: r.f32(f).map_err(d)?, hertz: r.f32(f).map_err(d)?, damping_ratio: r.f32(f).map_err(d)? }),
    };
    let desc = JointDesc {
        kind,
        a,
        b,
        frame_a,
        frame_b,
        limit: ranges[0],
        swing: ranges[1],
        length: [v[0], v[1]],
        spring_hertz: v[2],
        spring_damping: v[3],
        rest_length: v[4],
        motor,
        friction: v[5],
        collide_connected,
    };
    crate::joint::validate(&desc).map_err(|e| PhysicsError::new("bad_snapshot", format!("a saved joint is invalid: {}", e.message)))?;
    Ok(desc)
}

/// One body's hidden state, as read (applied once every body has been read and checked).
struct BodyState {
    key: u64,
    kind: u8,
    pose: Pose,
    velocity: Vec3,
    angular_velocity: Vec3,
    sleeping: bool,
    com: Vec3,
    center: Vec3,
    inv_mass: f32,
    inertia_local: Mat3,
    inv_inertia_world: Mat3,
    sleep_time: f32,
    island: u32,
    target: Option<Pose>,
    declared_velocity: (Vec3, Vec3),
    inner_radius: f32,
    outer_radius: f32,
    c0: Vec3,
    q0: Quat,
    /// `(in the static tree, fat box)`, or `None` without a proxy.
    fat: Option<(bool, Aabb)>,
}

impl Space {
    /// The space's whole hidden state (see the module docs). Bodies and contacts in key order,
    /// so the same state always gives the same bytes.
    pub fn save_state(&self, w: &mut ByteWriter) {
        w.u32(SNAPSHOT_VERSION);
        w_settings(w, &self.settings);
        w.u64(self.steps);
        w.f32(self.dt);
        w.u32(self.next_island);
        w.u64(self.refused as u64);
        let kept = |k: &u64| !self.is_cosmetic(*k);
        w.len_u32(self.by_key.keys().filter(|k| kept(k)).count());
        for (&key, &s) in &self.by_key {
            let b = self.slot(s);
            if b.desc.cosmetic {
                continue;
            }
            w.u64(key);
            w.u8(b.desc.kind as u8);
            w_pose(w, &b.desc.pose);
            w.vec3(b.desc.velocity);
            w.vec3(b.desc.angular_velocity);
            w.bool(b.desc.sleeping);
            w.vec3(b.com);
            w.vec3(b.center);
            w.f32(b.inv_mass);
            w_mat3(w, &b.inertia_local);
            w_mat3(w, &b.inv_inertia_world);
            w.f32(b.sleep_time);
            w.u32(b.island);
            w.bool(b.target.is_some());
            if let Some(t) = &b.target {
                w_pose(w, t);
            }
            w.vec3(b.declared_velocity.0);
            w.vec3(b.declared_velocity.1);
            w.f32(b.inner_radius);
            w.f32(b.outer_radius);
            w.vec3(b.c0);
            w.quat(b.q0);
            match self.fat(b) {
                Some(f) => {
                    w.bool(true);
                    w.bool(b.in_static_tree);
                    w.vec3(f.min);
                    w.vec3(f.max);
                }
                None => w.bool(false),
            }
        }
        let pairs: Vec<(&(u64, u64), &u32)> = self.pairs.iter().filter(|(_, s)| !self.contact(**s).cosmetic).collect();
        w.len_u32(pairs.len());
        for (&(ka, kb), &slot) in pairs {
            let c = self.contact(slot);
            w.u64(ka);
            w.u64(kb);
            w.bool(c.touching);
            w.bool(c.trigger);
            w.f32(c.approach);
            w.f32(c.friction);
            w.f32(c.restitution);
            c.sat.save(w);
            w.len_u32(c.len);
            for p in &c.points[..c.len] {
                w.vec3(p.point);
                w.vec3(p.normal);
                w.f32(p.separation);
                w.u32(p.id);
                w.f32(p.normal_impulse);
                w.f32(p.tangent_impulse[0]);
                w.f32(p.tangent_impulse[1]);
                w.f32(p.max_normal_impulse);
            }
        }
        let keys = |w: &mut ByteWriter, ks: Vec<&u64>| {
            w.len_u32(ks.len());
            ks.into_iter().for_each(|k| w.u64(*k));
        };
        keys(w, self.moved.iter().filter(|k| kept(k)).collect());
        keys(w, self.awake.iter().filter(|k| kept(k)).collect());
        let islands: Vec<(&u32, Vec<&u64>)> = self.islands.iter().map(|(id, m)| (id, m.iter().filter(|k| kept(k)).collect::<Vec<_>>())).filter(|(_, m)| !m.is_empty()).collect();
        w.len_u32(islands.len());
        for (id, members) in islands {
            w.u32(*id);
            keys(w, members);
        }
        let characters: Vec<_> = self.characters.iter().filter(|(k, _)| kept(k)).collect();
        w.len_u32(characters.len());
        for (k, c) in characters {
            w.u64(*k);
            w.bool(c.grounded);
            w.vec3(c.ground_normal);
            w.opt_u64(c.ground);
        }
        let joints: Vec<_> = self.joints.iter().filter(|(_, j)| !self.joint_is_cosmetic(j)).collect();
        w.len_u32(joints.len());
        for (k, j) in joints {
            w.u64(*k);
            w.u64(j.desc_hash);
            for v in [j.imp.linear, j.imp.angular, j.imp.motor, j.imp.friction] {
                w.vec3(v);
            }
            for x in [j.imp.lower, j.imp.upper, j.imp.swing, j.imp.spring] {
                w.f32(x);
            }
            let s = &j.state;
            w.bool(s.active);
            w.u8(s.inactive.and_then(|i| INACTIVE.iter().position(|x| *x == i)).map_or(0, |i| i as u8 + 1));
            for x in [s.linear_error, s.angular_error, s.position, s.swing, s.limit_excess, s.force, s.torque] {
                w.f32(x);
            }
            w.u64(j.solved);
        }
    }

    /// Puts [`Space::save_state`]'s state back into this space, which must hold exactly the
    /// saved bodies (rebuilt from the same world: same keys, kinds and colliders). Checks
    /// everything before changing anything: on an error the space is as it was.
    pub fn restore_state(&mut self, r: &mut ByteReader<'_>) -> Result<(), PhysicsError> {
        let version = r.u32("version").map_err(decode)?;
        if !(1..=SNAPSHOT_VERSION).contains(&version) {
            return Err(PhysicsError::new("unsupported_version", format!("physics snapshot version {version}; this build reads {SNAPSHOT_VERSION}"))
                .with("version", version));
        }
        let settings = r_settings(r, version).map_err(decode)?;
        let steps = r.u64("steps").map_err(decode)?;
        let dt = r.f32("dt").map_err(decode)?;
        let next_island = r.u32("next_island").map_err(decode)?;
        let refused = r.u64("refused").map_err(decode)?.min(u64::from(u32::MAX)) as usize;
        // Bodies: at least ~200 bytes each.
        let n = r.count("bodies", self.settings.max_bodies.max(settings.max_bodies), 200).map_err(decode)?;
        let rebuilt = self.by_key.keys().filter(|k| !self.is_cosmetic(**k)).count();
        if n != rebuilt {
            return Err(PhysicsError::new(
                "snapshot_mismatch",
                format!("the snapshot has {n} bodies but the world rebuilt {rebuilt}: the save's world and physics don't belong together"),
            )
            .with("saved", n)
            .with("rebuilt", rebuilt));
        }
        let mut bodies = Vec::with_capacity(n);
        let mut last = None;
        for _ in 0..n {
            let f = "bodies";
            let d = |e| decode(e);
            let key = r.u64(f).map_err(d)?;
            if last.is_some_and(|l| key <= l) {
                return Err(decode(r.error("invalid", f, "body keys must be in increasing order")));
            }
            last = Some(key);
            let kind = r.u8(f).map_err(d)?;
            let pose = r_pose(r, f).map_err(d)?;
            let velocity = r.vec3(f).map_err(d)?;
            let angular_velocity = r.vec3(f).map_err(d)?;
            let sleeping = r.bool(f).map_err(d)?;
            let com = r.vec3(f).map_err(d)?;
            let center = r.vec3(f).map_err(d)?;
            let inv_mass = r.f32(f).map_err(d)?;
            let inertia_local = r_mat3(r, f).map_err(d)?;
            let inv_inertia_world = r_mat3(r, f).map_err(d)?;
            let sleep_time = r.f32(f).map_err(d)?;
            let island = r.u32(f).map_err(d)?;
            let target = if r.bool(f).map_err(d)? { Some(r_pose(r, f).map_err(d)?) } else { None };
            let declared_velocity = (r.vec3(f).map_err(d)?, r.vec3(f).map_err(d)?);
            let inner_radius = r.f32(f).map_err(d)?;
            let outer_radius = r.f32(f).map_err(d)?;
            let c0 = r.vec3(f).map_err(d)?;
            let q0 = r.quat(f).map_err(d)?;
            let fat = if r.bool(f).map_err(d)? {
                let st = r.bool(f).map_err(d)?;
                Some((st, Aabb { min: r.vec3(f).map_err(d)?, max: r.vec3(f).map_err(d)? }))
            } else {
                None
            };
            let Some(&s) = self.by_key.get(&key).filter(|s| !self.slot(**s).desc.cosmetic) else {
                return Err(mismatch(format!("the snapshot has body {key}, which the world didn't rebuild"), key));
            };
            let b = self.slot(s);
            if b.desc.kind as u8 != kind {
                return Err(mismatch(format!("body {key} is {} in the world but kind {kind} in the snapshot", b.desc.kind.name()), key));
            }
            if fat.is_some() != (b.proxy != NONE) || fat.is_some_and(|(st, _)| st != b.in_static_tree) {
                return Err(mismatch(format!("body {key}'s collider differs between the world and the snapshot"), key));
            }
            // Numbers the step divides by, compares or puts in the trees must be real numbers.
            let finite3 = |v: Vec3| crate::math::is_finite(v);
            let q_ok = |q: Quat| q.x.is_finite() && q.y.is_finite() && q.z.is_finite() && q.w.is_finite();
            let fat_ok = fat.is_none_or(|(_, f)| finite3(f.min) && finite3(f.max) && f.min.x <= f.max.x && f.min.y <= f.max.y && f.min.z <= f.max.z);
            let target_ok = target.is_none_or(|t| finite3(t.p) && q_ok(t.q));
            let all = [pose.p, velocity, angular_velocity, com, center, declared_velocity.0, declared_velocity.1, c0];
            if !(all.iter().all(|v| finite3(*v)) && q_ok(pose.q) && q_ok(q0) && fat_ok && target_ok && sleep_time.is_finite() && inv_mass.is_finite() && inner_radius.is_finite() && outer_radius.is_finite()) {
                return Err(decode(r.error("invalid", f, format!("body {key} has a number that isn't finite (or an inside-out box)"))));
            }
            bodies.push(BodyState {
                key,
                kind,
                pose,
                velocity,
                angular_velocity,
                sleeping,
                com,
                center,
                inv_mass,
                inertia_local,
                inv_inertia_world,
                sleep_time,
                island,
                target,
                declared_velocity,
                inner_radius,
                outer_radius,
                c0,
                q0,
                fat,
            });
        }
        // Contacts: at least ~60 bytes each.
        let m = r.count("contacts", settings.max_pairs.max(self.settings.max_pairs), 60).map_err(decode)?;
        let mut contacts = Vec::with_capacity(m);
        let mut last = None;
        for _ in 0..m {
            let f = "contacts";
            let d = |e| decode(e);
            let (ka, kb) = (r.u64(f).map_err(d)?, r.u64(f).map_err(d)?);
            if ka >= kb || last.is_some_and(|l| (ka, kb) <= l) {
                return Err(decode(r.error("invalid", f, "contact keys must be (lower, higher), in increasing order")));
            }
            last = Some((ka, kb));
            let (Some(&sa), Some(&sb)) = (self.by_key.get(&ka), self.by_key.get(&kb)) else {
                return Err(mismatch(format!("contact ({ka}, {kb}) names a body the snapshot doesn't have"), ka));
            };
            // Only bodies that make a pair (colliders in the broadphase, kinds, layers) have one.
            let (ba, bb) = (self.slot(sa), self.slot(sb));
            if ba.proxy == NONE || bb.proxy == NONE || !Space::should_pair(ba, bb) {
                return Err(mismatch(format!("contact ({ka}, {kb}) is between bodies that don't make a pair"), ka));
            }
            let touching = r.bool(f).map_err(d)?;
            let trigger = r.bool(f).map_err(d)?;
            let approach = if version >= 3 { r.f32(f).map_err(d)? } else { 0.0 };
            let friction = r.f32(f).map_err(d)?;
            let restitution = r.f32(f).map_err(d)?;
            let sat = SatCache::load(r).map_err(d)?;
            let len = r.count(f, MAX_POINTS, 44).map_err(d)?;
            let mut points = [CPoint::default(); MAX_POINTS];
            for p in points.iter_mut().take(len) {
                *p = CPoint {
                    point: r.vec3(f).map_err(d)?,
                    normal: r.vec3(f).map_err(d)?,
                    separation: r.f32(f).map_err(d)?,
                    id: r.u32(f).map_err(d)?,
                    normal_impulse: r.f32(f).map_err(d)?,
                    tangent_impulse: [r.f32(f).map_err(d)?, r.f32(f).map_err(d)?],
                    max_normal_impulse: r.f32(f).map_err(d)?,
                };
            }
            let cosmetic = ba.desc.cosmetic || bb.desc.cosmetic;
            contacts.push(Contact { key: (ka, kb), a: sa, b: sb, points, len, touching, trigger, friction, restitution, sat, cosmetic, approach });
        }
        let keys = |r: &mut ByteReader<'_>, f: &str, max: usize| -> Result<Vec<u64>, PhysicsError> {
            let n = r.count(f, max, 8).map_err(decode)?;
            (0..n).map(|_| r.u64(f).map_err(decode)).collect()
        };
        let moved = keys(r, "moved", n)?;
        let awake = keys(r, "awake", n)?;
        let island_count = r.count("islands", n, 8).map_err(decode)?;
        let mut islands = BTreeMap::new();
        for _ in 0..island_count {
            let id = r.u32("islands").map_err(decode)?;
            islands.insert(id, keys(r, "islands", n)?);
        }
        let char_count = r.count("characters", n, 22).map_err(decode)?;
        let mut characters = BTreeMap::new();
        for _ in 0..char_count {
            let f = "characters";
            let k = r.u64(f).map_err(decode)?;
            let grounded = r.bool(f).map_err(decode)?;
            let ground_normal = r.vec3(f).map_err(decode)?;
            let ground = r.opt_u64(f).map_err(decode)?;
            characters.insert(k, CharacterState { grounded, ground_normal, ground });
        }
        // Joints (version 2): the space was rebuilt with the same joints (same keys and
        // descriptions); their warm starts and last states come back.
        let mut joints = Vec::new();
        if version >= 2 {
            let jn = r.count("joints", settings.max_joints.max(self.settings.max_joints), 118).map_err(decode)?;
            let rebuilt = self.joints.values().filter(|j| !self.joint_is_cosmetic(j)).count();
            if jn != rebuilt {
                return Err(PhysicsError::new("snapshot_mismatch", format!("the snapshot has {jn} joints but the world rebuilt {rebuilt}"))
                    .with("saved", jn)
                    .with("rebuilt", rebuilt));
            }
            for _ in 0..jn {
                let f = "joints";
                let d = |e| decode(e);
                let k = r.u64(f).map_err(d)?;
                let desc_hash = r.u64(f).map_err(d)?;
                let mut v = [Vec3::ZERO; 4];
                for x in &mut v {
                    *x = r.vec3(f).map_err(d)?;
                }
                let mut s4 = [0.0f32; 4];
                for x in &mut s4 {
                    *x = r.f32(f).map_err(d)?;
                }
                let active = r.bool(f).map_err(d)?;
                let inactive = match r.u8(f).map_err(d)? {
                    0 => None,
                    i if (i as usize) <= INACTIVE.len() => Some(INACTIVE[i as usize - 1]),
                    _ => return Err(decode(r.error("invalid", f, "unknown reason a joint was inactive"))),
                };
                let mut s7 = [0.0f32; 7];
                for x in &mut s7 {
                    *x = r.f32(f).map_err(d)?;
                }
                let solved = r.u64(f).map_err(d)?;
                let finite = v.iter().all(|x| crate::math::is_finite(*x)) && s4.iter().chain(&s7).all(|x| x.is_finite());
                if !finite {
                    return Err(decode(r.error("invalid", f, format!("joint {k} has a number that isn't finite"))));
                }
                match self.joints.get(&k) {
                    Some(j) if j.desc_hash == desc_hash => {}
                    Some(_) => return Err(mismatch(format!("joint {k} differs between the world and the snapshot"), k)),
                    None => return Err(mismatch(format!("the snapshot has joint {k}, which the world didn't rebuild"), k)),
                }
                let imp = JointImpulses { linear: v[0], angular: v[1], motor: v[2], friction: v[3], lower: s4[0], upper: s4[1], swing: s4[2], spring: s4[3] };
                let state = JointState {
                    active,
                    inactive,
                    linear_error: s7[0],
                    angular_error: s7[1],
                    position: s7[2],
                    swing: s7[3],
                    limit_excess: s7[4],
                    force: s7[5],
                    torque: s7[6],
                };
                joints.push((k, imp, state, solved));
            }
        }
        for k in moved.iter().chain(&awake).chain(islands.values().flatten()).chain(characters.keys()) {
            if !self.by_key.contains_key(k) {
                return Err(mismatch(format!("the snapshot names body {k}, which the world didn't rebuild"), *k));
            }
        }
        // The awake set is what the bodies' kinds and sleep flags say it is (the step visits it;
        // islands are looked up leniently, so they need no such check).
        let want_awake: Vec<u64> = bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Kinematic as u8 || (b.kind == BodyKind::Dynamic as u8 && !b.sleeping))
            .map(|b| b.key)
            .collect();
        if want_awake != awake {
            return Err(decode(r.error("invalid", "awake", "the awake set doesn't match the bodies' kinds and sleep flags")));
        }
        // Everything checks: apply.
        self.settings = settings;
        self.steps = steps;
        self.dt = dt;
        self.next_island = next_island;
        self.refused = refused;
        for st in bodies {
            let s = self.by_key[&st.key];
            let b = self.slot_mut(s);
            debug_assert_eq!(b.desc.kind as u8, st.kind);
            b.desc.pose = st.pose;
            b.desc.velocity = st.velocity;
            b.desc.angular_velocity = st.angular_velocity;
            b.desc.sleeping = st.sleeping;
            b.com = st.com;
            b.center = st.center;
            b.inv_mass = st.inv_mass;
            b.inertia_local = st.inertia_local;
            b.inv_inertia_world = st.inv_inertia_world;
            b.sleep_time = st.sleep_time;
            b.island = st.island;
            b.target = st.target;
            b.declared_velocity = st.declared_velocity;
            b.inner_radius = st.inner_radius;
            b.outer_radius = st.outer_radius;
            b.c0 = st.c0;
            b.q0 = st.q0;
            b.solver = NONE;
            if let Some((in_static, fat)) = st.fat {
                let proxy = b.proxy;
                if in_static {
                    self.static_tree.update(proxy, fat);
                } else {
                    self.dynamic_tree.update(proxy, fat);
                }
            }
        }
        self.contacts.clear();
        self.contact_free.clear();
        self.pairs.clear();
        self.reverse.clear();
        self.sim.clear();
        self.touching_count = 0;
        for c in contacts {
            let (ka, kb) = c.key;
            self.touching_count += usize::from(c.touching);
            self.pairs.insert((ka, kb), self.contacts.len() as u32);
            self.reverse.insert((kb, ka));
            self.contacts.push(Some(c));
        }
        self.moved = moved.into_iter().collect();
        self.awake = awake.into_iter().collect();
        self.islands = islands;
        self.characters = characters;
        for (k, imp, state, solved) in joints {
            let j = self.joints.get_mut(&k).expect("checked above");
            j.imp = imp;
            j.state = state;
            j.solved = solved;
        }
        self.dynamic_count = self.by_key.values().filter(|s| self.slot(**s).is_dynamic()).count();
        self.active.clear();
        self.inactive_cache.set(None);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Shape;

    fn scene(order: &[u64]) -> Space {
        let mut s = Space::default();
        for &k in order {
            let (kind, pose, shape) = match k {
                1 => (BodyKind::Static, Pose::at(Vec3::new(0.0, -0.5, 0.0)), Shape::cuboid(Vec3::new(10.0, 0.5, 10.0))),
                k if k % 3 == 0 => (BodyKind::Dynamic, Pose::at(Vec3::new(0.1 * k as f32, 0.5 + 1.1 * k as f32, 0.0)), Shape::sphere(0.5)),
                k => (BodyKind::Dynamic, Pose::at(Vec3::new(0.05 * k as f32, 0.5 + 1.1 * k as f32, 0.1)), Shape::cuboid(Vec3::new(0.5, 0.5, 0.5))),
            };
            let mut d = BodyDesc::new(kind, pose, Some(ColliderDesc::new(shape)));
            if k == 5 {
                d.angular_velocity = Vec3::new(0.0, 2.0, 0.0);
            }
            s.insert(k, d).unwrap();
        }
        s
    }

    #[test]
    fn a_restored_space_continues_bit_identically() {
        let keys: Vec<u64> = (1..=9).collect();
        let mut a = scene(&keys);
        for _ in 0..50 {
            a.step(1.0 / 60.0);
        }
        let mut w = ByteWriter::new();
        a.save_state(&mut w);
        let bytes = w.into_bytes();
        // Rebuilt from the same "world" in another order (as a load does), then restored.
        let mut rev = keys.clone();
        rev.reverse();
        let mut b = scene(&rev);
        assert_ne!(a.hash(), b.hash());
        let mut r = ByteReader::new(&bytes);
        b.restore_state(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.hash_uncached(), b.hash_uncached());
        for t in 0..300 {
            a.step(1.0 / 60.0);
            b.step(1.0 / 60.0);
            assert_eq!(a.hash(), b.hash(), "diverged {t} steps after the restore");
        }
        // And the saved bytes are canonical: saving the restored space again gives the same.
        let (mut wa, mut wb) = (ByteWriter::new(), ByteWriter::new());
        a.save_state(&mut wa);
        b.save_state(&mut wb);
        assert_eq!(wa.bytes(), wb.bytes());
    }

    /// A chain of ball joints hanging from the world, a box knocking into it, and (optionally)
    /// a cosmetic body falling beside it.
    fn jointed(order: &[u64], cosmetic: bool) -> Space {
        use crate::joint::{JointDesc, JointKind};
        let mut s = Space::default();
        for &k in order {
            let d = match k {
                1 => BodyDesc::new(BodyKind::Static, Pose::at(Vec3::new(0.0, -0.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(Vec3::new(10.0, 0.5, 10.0))))),
                9 => {
                    let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(Vec3::new(-2.0, 1.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(Vec3::new(0.2, 0.2, 0.2)))));
                    d.velocity = Vec3::new(6.0, 0.0, 0.0);
                    d
                }
                _ => BodyDesc::new(BodyKind::Dynamic, Pose::at(Vec3::new(0.0, 3.0 - 0.5 * (k - 10) as f32, 0.0)), Some(ColliderDesc::new(Shape::cuboid(Vec3::new(0.1, 0.25, 0.1))))),
            };
            s.insert(k, d).unwrap();
        }
        if cosmetic {
            let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(Vec3::new(3.0, 2.0, 0.0)), Some(ColliderDesc::new(Shape::sphere(0.3))));
            d.cosmetic = true;
            s.insert(50, d).unwrap();
        }
        for i in 0..5u64 {
            let top = Vec3::new(0.0, 3.25 - 0.5 * i as f32, 0.0);
            let a = (i > 0).then_some(9 + i);
            let fa = if i == 0 { Pose::at(top) } else { Pose::at(Vec3::new(0.0, -0.25, 0.0)) };
            s.insert_joint(100 + i, JointDesc::new(JointKind::Ball, a, Some(10 + i), fa, Pose::at(Vec3::new(0.0, 0.25, 0.0)))).unwrap();
        }
        s
    }

    #[test]
    fn joints_continue_bit_identically_and_cosmetic_bodies_stay_out() {
        let keys: Vec<u64> = vec![1, 9, 10, 11, 12, 13, 14];
        let mut a = jointed(&keys, true);
        for _ in 0..40 {
            a.step(1.0 / 60.0);
        }
        let mut w = ByteWriter::new();
        a.save_state(&mut w);
        let bytes = w.into_bytes();
        // Rebuilt in another order and without the cosmetic body (a load has no transient
        // entities), then restored.
        let mut rev = keys.clone();
        rev.reverse();
        let mut b = jointed(&rev, false);
        let mut r = ByteReader::new(&bytes);
        b.restore_state(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(a.hash(), b.hash());
        for t in 0..200 {
            a.step(1.0 / 60.0);
            b.step(1.0 / 60.0);
            assert_eq!(a.hash(), b.hash(), "diverged {t} steps after the restore");
        }
        // A space whose joints differ is a mismatch.
        let mut c = jointed(&keys, false);
        c.remove_joint(102);
        assert_eq!(c.restore_state(&mut ByteReader::new(&bytes)).unwrap_err().code, "snapshot_mismatch");
    }

    #[test]
    fn a_plain_rebuild_drifts_which_is_why_saves_carry_this() {
        let keys: Vec<u64> = (1..=9).collect();
        let mut a = scene(&keys);
        for _ in 0..50 {
            a.step(1.0 / 60.0);
        }
        // Rebuild from what a world file shows (poses and velocities), without the snapshot.
        let mut b = Space::default();
        for v in a.bodies() {
            let mut d = BodyDesc::new(v.kind, v.pose, v.collider.cloned());
            d.velocity = v.velocity;
            d.angular_velocity = v.angular_velocity;
            d.sleeping = v.sleeping;
            b.insert(v.key, d).unwrap();
        }
        let mut diverged = false;
        for _ in 0..120 {
            a.step(1.0 / 60.0);
            b.step(1.0 / 60.0);
            diverged |= a.hash() != b.hash();
        }
        assert!(diverged, "without warm starts and sleep timers the runs should part ways");
    }

    #[test]
    fn damaged_or_mismatched_snapshots_are_refused_without_changes() {
        let keys: Vec<u64> = (1..=6).collect();
        let mut a = scene(&keys);
        for _ in 0..20 {
            a.step(1.0 / 60.0);
        }
        let mut w = ByteWriter::new();
        a.save_state(&mut w);
        let bytes = w.into_bytes();
        let b0 = scene(&keys);
        // Every truncation is an error, and leaves the space untouched.
        for cut in (0..bytes.len()).step_by(7) {
            let mut b = b0.clone();
            let before = b.hash_uncached();
            assert!(b.restore_state(&mut ByteReader::new(&bytes[..cut])).is_err());
            assert_eq!(b.hash_uncached(), before);
        }
        // Flipped bytes never panic.
        let mut seed = 0x1234_5678u64;
        for _ in 0..400 {
            let mut x = bytes.clone();
            for _ in 0..3 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let i = (seed >> 33) as usize % x.len();
                x[i] ^= (seed >> 20) as u8 | 1;
            }
            let mut b = b0.clone();
            let _ = b.restore_state(&mut ByteReader::new(&x));
        }
        // A space with other bodies is a mismatch.
        let mut other = scene(&[1, 2, 3]);
        let e = other.restore_state(&mut ByteReader::new(&bytes)).unwrap_err();
        assert_eq!(e.code, "snapshot_mismatch");
    }
}
