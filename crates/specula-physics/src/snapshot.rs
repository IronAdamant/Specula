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
//! What is not saved, because it is derived or rebuilt: shapes (built from the world's
//! colliders), slot and proxy numbering and the trees' internal layout (every result is sorted
//! by key, so they never reach the simulation), solver buffers and the last step's report.
//! Contact geometry (points, normals, separations) is recomputed by every narrowphase but is
//! saved anyway, so a restored space reads back the same contacts before its next step.

use super::*;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// Bumped when the layout changes; older snapshots are refused with `unsupported_version`.
pub const SNAPSHOT_VERSION: u32 = 1;

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
}

fn r_settings(r: &mut ByteReader<'_>) -> Result<Settings, DecodeError> {
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
    Ok(Settings {
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
        w.len_u32(self.by_key.len());
        for (&key, &s) in &self.by_key {
            let b = self.slot(s);
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
        w.len_u32(self.pairs.len());
        for (&(ka, kb), &slot) in &self.pairs {
            let c = self.contact(slot);
            w.u64(ka);
            w.u64(kb);
            w.bool(c.touching);
            w.bool(c.trigger);
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
        w.len_u32(self.moved.len());
        self.moved.iter().for_each(|k| w.u64(*k));
        w.len_u32(self.awake.len());
        self.awake.iter().for_each(|k| w.u64(*k));
        w.len_u32(self.islands.len());
        for (id, members) in &self.islands {
            w.u32(*id);
            w.len_u32(members.len());
            members.iter().for_each(|k| w.u64(*k));
        }
        w.len_u32(self.characters.len());
        for (k, c) in &self.characters {
            w.u64(*k);
            w.bool(c.grounded);
            w.vec3(c.ground_normal);
            w.opt_u64(c.ground);
        }
    }

    /// Puts [`Space::save_state`]'s state back into this space, which must hold exactly the
    /// saved bodies (rebuilt from the same world: same keys, kinds and colliders). Checks
    /// everything before changing anything: on an error the space is as it was.
    pub fn restore_state(&mut self, r: &mut ByteReader<'_>) -> Result<(), PhysicsError> {
        let version = r.u32("version").map_err(decode)?;
        if version != SNAPSHOT_VERSION {
            return Err(PhysicsError::new("unsupported_version", format!("physics snapshot version {version}; this build reads {SNAPSHOT_VERSION}"))
                .with("version", version));
        }
        let settings = r_settings(r).map_err(decode)?;
        let steps = r.u64("steps").map_err(decode)?;
        let dt = r.f32("dt").map_err(decode)?;
        let next_island = r.u32("next_island").map_err(decode)?;
        let refused = r.u64("refused").map_err(decode)?.min(u64::from(u32::MAX)) as usize;
        // Bodies: at least ~200 bytes each.
        let n = r.count("bodies", self.settings.max_bodies.max(settings.max_bodies), 200).map_err(decode)?;
        if n != self.by_key.len() {
            return Err(PhysicsError::new(
                "snapshot_mismatch",
                format!("the snapshot has {n} bodies but the world rebuilt {}: the save's world and physics don't belong together", self.by_key.len()),
            )
            .with("saved", n)
            .with("rebuilt", self.by_key.len()));
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
            let Some(&s) = self.by_key.get(&key) else {
                return Err(mismatch(format!("the snapshot has body {key}, which the world didn't rebuild"), key));
            };
            let b = self.slot(s);
            if b.desc.kind as u8 != kind {
                return Err(mismatch(format!("body {key} is {} in the world but kind {kind} in the snapshot", b.desc.kind.name()), key));
            }
            if fat.is_some() != (b.proxy != NONE) || fat.is_some_and(|(st, _)| st != b.in_static_tree) {
                return Err(mismatch(format!("body {key}'s collider differs between the world and the snapshot"), key));
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
            let touching = r.bool(f).map_err(d)?;
            let trigger = r.bool(f).map_err(d)?;
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
            contacts.push(Contact { key: (ka, kb), a: sa, b: sb, points, len, touching, trigger, friction, restitution, sat });
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
        for k in moved.iter().chain(&awake).chain(islands.values().flatten()).chain(characters.keys()) {
            if !self.by_key.contains_key(k) {
                return Err(mismatch(format!("the snapshot names body {k}, which the world didn't rebuild"), *k));
            }
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
