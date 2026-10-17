//! Save games: the physics bridge's state and the space's hidden state (docs/design/SAVES.md).
//!
//! A save is taken between ticks. [`Physics::save_state`] first syncs the space with the world
//! (what the next tick's sync would do), so the bytes describe one consistent state: the world
//! as saved plus the physics it implies. [`Physics::restore_state`] rebuilds the bodies from the
//! loaded world exactly as a fresh sync does, then puts the hidden state back (the layer table,
//! what the bridge last saw of every entity, the characters' carried velocities, the space's
//! warm starts, sleep timers, pair memory and fat boxes) and checks that the space's hash is the
//! one the saving run had, so a loaded run continues bit-identically.

use super::*;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// Bumped when the layout changes.
pub const BRIDGE_VERSION: u32 = 1;
/// A section bigger than this is refused (20,000 bodies with every field is about 6 MB).
const MAX_ITEMS: usize = 1 << 20;

fn bad(e: DecodeError) -> WorldError {
    WorldError::new("bad_save", format!("the save's physics section is damaged: {e}"))
        .with("section", "physics")
        .with("decode", e.code)
        .with("offset", e.offset)
        .with("field", e.field.as_str())
}

impl Physics {
    /// The bridge and space state for a save (syncing with `world` first), or `None` when
    /// physics has never stepped (nothing is hidden then: a load syncs afresh).
    pub fn save_state(&mut self, world: &World, generation: u64) -> Option<Vec<u8>> {
        if self.hashes.is_empty() && self.space.is_empty() {
            return None;
        }
        self.sync(world, generation);
        let mut w = ByteWriter::with_capacity(256 + self.space.len() * 300);
        w.u32(BRIDGE_VERSION);
        w.len_u32(self.layers.len());
        for (name, bit) in &self.layers {
            w.str(name);
            w.u32(*bit);
        }
        w.len_u32(self.seen.len());
        for (id, s) in &self.seen {
            w.u64(*id);
            w.u64(s.version);
            s.world.iter().flatten().for_each(|v| w.f32(*v));
            w.u64(s.shape_key);
            w.vec3(s.scale);
        }
        w.len_u32(self.characters.len());
        for (id, v) in &self.characters {
            w.u64(*id);
            w.vec3(*v);
        }
        w.len_u32(self.problems.len());
        for (id, j) in &self.problems {
            w.u64(*id);
            w.str(&j.to_string());
        }
        w.bool(self.settings_applied);
        w.u64(self.next_event);
        w.u64(self.space.hash());
        let mut space = ByteWriter::with_capacity(self.space.len() * 280);
        self.space.save_state(&mut space);
        w.blob(space.bytes());
        Some(w.into_bytes())
    }

    /// Rebuilds physics for a loaded `world` from [`Physics::save_state`]'s bytes. On an error
    /// physics is reset (the next tick syncs afresh, deterministically but not bit-identically).
    pub fn restore_state(&mut self, world: &World, generation: u64, tick: u64, data: &[u8]) -> Result<(), WorldError> {
        let out = self.restore_inner(world, generation, tick, data);
        if out.is_err() {
            self.reset();
        }
        out
    }

    fn restore_inner(&mut self, world: &World, generation: u64, tick: u64, data: &[u8]) -> Result<(), WorldError> {
        let mut r = ByteReader::new(data);
        let version = r.u32("version").map_err(bad)?;
        if version != BRIDGE_VERSION {
            return Err(WorldError::new("unsupported_version", format!("physics save version {version}; this build reads {BRIDGE_VERSION}"))
                .with("section", "physics"));
        }
        let n = r.count("layers", MAX_LAYERS, 8).map_err(bad)?;
        let mut layers = BTreeMap::new();
        for _ in 0..n {
            let name = r.str("layers.name", 256).map_err(bad)?;
            let bit = r.u32("layers.bit").map_err(bad)?;
            if bit as usize >= MAX_LAYERS {
                return Err(bad(r.error("invalid", "layers.bit", format!("layer bit {bit} past {MAX_LAYERS}"))));
            }
            layers.insert(name, bit);
        }
        let n = r.count("seen", MAX_ITEMS, 100).map_err(bad)?;
        let mut seen = Vec::with_capacity(n);
        for _ in 0..n {
            let id = r.u64("seen.id").map_err(bad)?;
            let version = r.u64("seen.version").map_err(bad)?;
            let mut cols = [[0.0f32; 4]; 4];
            for v in cols.iter_mut().flatten() {
                *v = r.f32("seen.world").map_err(bad)?;
            }
            let shape_key = r.u64("seen.shape_key").map_err(bad)?;
            let scale = r.vec3("seen.scale").map_err(bad)?;
            seen.push((id, version, cols, shape_key, scale));
        }
        let n = r.count("characters", MAX_ITEMS, 20).map_err(bad)?;
        let mut characters = BTreeMap::new();
        for _ in 0..n {
            let id = r.u64("characters.id").map_err(bad)?;
            characters.insert(id, r.vec3("characters.velocity").map_err(bad)?);
        }
        let n = r.count("problems", MAX_PROBLEMS, 12).map_err(bad)?;
        let mut problems = BTreeMap::new();
        for _ in 0..n {
            let id = r.u64("problems.id").map_err(bad)?;
            let text = r.str("problems.error", 4096).map_err(bad)?;
            problems.insert(id, Json::parse(&text).unwrap_or(Json::from(text)));
        }
        let settings_applied = r.bool("settings_applied").map_err(bad)?;
        let next_event = r.u64("next_event").map_err(bad)?;
        let hash = r.u64("hash").map_err(bad)?;
        let space = r.blob("space", 1 << 30).map_err(bad)?;
        r.finish().map_err(bad)?;

        // Rebuild the bodies the saving run had from the world, in id order, each collider at
        // the scale it was built at there, with the saved layer bits. (Entities it couldn't
        // simulate, its problems, aren't in its space and stay out.)
        self.reset();
        self.layers = layers;
        for &(id, _, _, _, scale) in &seen {
            let Some(e) = world.get(id) else {
                return Err(WorldError::new("save_mismatch", format!("the physics section names entity {id}, which the save's world doesn't have"))
                    .with("section", "physics")
                    .with("id", id));
            };
            self.upsert_scaled(world, e, Some(scale));
        }
        self.seen.clear();
        for (id, version, cols, shape_key, scale) in seen {
            let e = world.get(id).expect("checked above");
            if !self.space.contains(id) {
                let why = self.problems.get(&id).cloned().unwrap_or(Json::Null);
                return Err(WorldError::new("save_mismatch", format!("entity {} ({id}) had a body when saved but can't have one now", world.path_of(id)))
                    .with("section", "physics")
                    .with("id", id)
                    .with("problem", why));
            }
            self.seen.insert(id, Seen { version, world: cols, shape_key, body: e.body, scale });
        }
        let mut sr = ByteReader::new(space);
        self.space.restore_state(&mut sr).map_err(|e| {
            let mut err = WorldError::new(if e.code == "bad_snapshot" { "bad_save" } else { "save_mismatch" }, e.message.clone()).with("section", "physics");
            for (k, v) in e.details {
                err = err.with(k, v);
            }
            err
        })?;
        sr.finish().map_err(bad)?;
        self.characters = characters;
        self.problems = problems;
        self.settings_applied = settings_applied;
        self.next_event = next_event;
        let now = self.space.hash();
        if now != hash {
            return Err(WorldError::new(
                "save_mismatch",
                format!("physics restored to hash {now:016x}, but the saving run had {hash:016x}: the save's world and physics don't belong together"),
            )
            .with("section", "physics"));
        }
        self.hashes.push_back((tick, now));
        // Everything the loaded world holds is now known to physics.
        self.synced = Some((generation, world.draw_epoch()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Shape as WShape, SpawnSpec};

    const DT: f32 = 1.0 / 30.0;

    fn scene() -> World {
        let mut w = World::default();
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("floor".into());
        s.position = Vec3::new(0.0, -0.5, 0.0);
        s.scale = Vec3::new(20.0, 1.0, 20.0);
        s.collider = Some(Collider::default());
        w.spawn(s, "t").unwrap();
        for i in 0..12u32 {
            let mut s = SpawnSpec::new(if i % 3 == 0 { WShape::Sphere } else { WShape::Cube });
            s.name = Some(format!("c{i}"));
            s.position = Vec3::new((i % 4) as f32 * 0.7 - 1.0, 0.6 + (i / 4) as f32 * 1.3, (i % 2) as f32 * 0.3);
            s.rotation_deg = Vec3::new(0.0, i as f32 * 17.0, 3.0);
            s.collider = Some(if i % 3 == 0 { Collider { shape: ColliderShape::Sphere { radius: 0.5 }, ..Collider::default() } } else { Collider::default() });
            s.body = Some(Body::new(BodyKind::Dynamic, 5.0 + i as f32));
            w.spawn(s, "t").unwrap();
        }
        w
    }

    fn tick(w: &mut World, p: &mut Physics, t: u64) {
        let settings = w.env().physics;
        w.set_recording(Recording::Snapshot);
        p.tick(w, 0, &settings, t, DT);
        w.set_recording(Recording::Full);
        let _ = w.take_changes();
    }

    fn load(files: &[(String, String)]) -> World {
        crate::persist::load_with(|f| {
            files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| crate::persist::PersistError::new("not_found", f.to_owned()))
        })
        .unwrap()
        .0
    }

    /// The point of the physics section: a world reloaded from its files diverges (hidden state
    /// is lost), the same world plus this snapshot continues bit for bit.
    #[test]
    fn a_saved_world_and_physics_continue_bit_identically() {
        let mut a = scene();
        let mut pa = Physics::default();
        for t in 0..45 {
            tick(&mut a, &mut pa, t);
        }
        let bytes = pa.save_state(&a, 0).expect("physics ran");
        let files = crate::persist::render(&a);
        assert_eq!(load(&files).state_hash(), a.state_hash(), "world files restore the world bit for bit");
        // With the snapshot.
        let mut b = load(&files);
        let mut pb = Physics::default();
        pb.restore_state(&b, 0, 45, &bytes).unwrap();
        assert_eq!(pb.hash(), Some(pa.space.hash()));
        // The restored space is the saved one, byte for byte.
        let (mut x, mut y) = (ByteWriter::new(), ByteWriter::new());
        pa.space.save_state(&mut x);
        pb.space.save_state(&mut y);
        assert_eq!(x.bytes(), y.bytes());
        // Without it (what a world_load does).
        let mut c = load(&files);
        let mut pc = Physics::default();
        let mut c_diverged = None;
        for t in 45..200 {
            tick(&mut a, &mut pa, t);
            tick(&mut b, &mut pb, t);
            tick(&mut c, &mut pc, t);
            assert_eq!(a.state_hash(), b.state_hash(), "the restored world diverged at tick {t}");
            assert_eq!(pa.hash(), pb.hash(), "restored physics diverged at tick {t}");
            if c_diverged.is_none() && a.state_hash() != c.state_hash() {
                c_diverged = Some(t);
            }
        }
        assert!(c_diverged.is_some(), "a reload without the snapshot should drift (or this test proves nothing)");
    }

    #[test]
    fn damaged_physics_sections_are_errors_and_leave_physics_reset() {
        let mut a = scene();
        let mut pa = Physics::default();
        for t in 0..10 {
            tick(&mut a, &mut pa, t);
        }
        let bytes = pa.save_state(&a, 0).unwrap();
        for cut in (0..bytes.len()).step_by(97) {
            let mut p = Physics::default();
            let e = p.restore_state(&a, 0, 10, &bytes[..cut]).unwrap_err();
            assert!(["bad_save", "unsupported_version", "save_mismatch"].contains(&e.code), "{cut}: {e:?}");
            assert!(p.space.is_empty() && p.hashes.is_empty());
        }
        // A world without the bodies is a mismatch.
        let mut p = Physics::default();
        let e = p.restore_state(&World::default(), 0, 10, &bytes).unwrap_err();
        assert_eq!(e.code, "save_mismatch");
    }
}
