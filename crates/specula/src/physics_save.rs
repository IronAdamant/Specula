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

/// Bumped when the layout changes. Version 2 adds the saved entities' ragdolls (version 1
/// saves, which have none, still load).
pub const BRIDGE_VERSION: u32 = 2;
/// A section bigger than this is refused (20,000 bodies with every field is about 6 MB).
const MAX_ITEMS: usize = 1 << 20;
/// A problem's saved text, at most (clipped when written, so whatever is written reads back).
const MAX_PROBLEM_TEXT: usize = 4096;

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
            // Clipped to what a load reads back (entity paths can be long).
            let text = j.to_string();
            let mut end = text.len().min(MAX_PROBLEM_TEXT);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            w.str(&text[..end]);
        }
        w.bool(self.settings_applied);
        w.u64(self.next_event);
        self.save_ragdolls(&mut w);
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
        if !(1..=BRIDGE_VERSION).contains(&version) {
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
            let text = r.str("problems.error", MAX_PROBLEM_TEXT).map_err(bad)?;
            problems.insert(id, Json::parse(&text).unwrap_or(Json::from(text)));
        }
        let settings_applied = r.bool("settings_applied").map_err(bad)?;
        let next_event = r.u64("next_event").map_err(bad)?;
        let ragdolls = if version >= 2 { Physics::read_ragdolls(&mut r)? } else { Vec::new() };
        let hash = r.u64("hash").map_err(bad)?;
        let space = r.blob("space", 1 << 30).map_err(bad)?;
        r.finish().map_err(bad)?;

        // Rebuild the bodies the saving run had from the world, in id order, each collider at
        // the scale it was built at there, with the saved layer bits. (Entities it couldn't
        // simulate, its problems, aren't in its space and stay out.)
        self.reset();
        self.layers = layers;
        // Ragdolls first: their entities' own bodies take their collision groups.
        self.rebuild_ragdolls(world, ragdolls)?;
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
        // The world's joints (after every body: their frames take the bodies' scales), and which
        // saved entities have ragdolls.
        for e in world.persistent_entities() {
            if joints::is_joint(e) {
                self.upsert_joint(world, e);
            }
            if e.props.get(ragdoll::PROP).is_some() {
                self.ragdolls.persistent.insert(e.id);
            }
        }
        let active: Vec<u64> = self.ragdolls.live.iter().filter(|(_, l)| l.off_since.is_none()).map(|(id, _)| *id).collect();
        for id in active {
            self.space.set_group(id, id + 1);
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

    /// Review round 3, the blocker (probe savejoint.py): a chain of scaled links hung from the
    /// world by a ball joint, saved after it has swung (1, 26 and 60 ticks), loads and continues
    /// bit for bit. Joint frames once came from the world matrix's column lengths, which change
    /// in the last bits as a body turns, so the rebuilt joint didn't match the saved one.
    #[test]
    fn a_swinging_chain_on_a_world_anchor_saves_and_loads() {
        for (steps, threads) in [(1u64, 1usize), (26, 1), (60, 1), (60, 4)] {
            let mut a = World::default();
            let mut ids = Vec::new();
            for i in 0..6u32 {
                let mut s = SpawnSpec::new(WShape::Cube);
                s.name = Some(format!("l{i}"));
                s.position = Vec3::new(0.5 * i as f32 + 0.25, 6.0, -3.0);
                s.scale = Vec3::new(0.5, 0.1, 0.1);
                s.collider = Some(Collider::default());
                s.body = Some(Body::new(BodyKind::Dynamic, 1.0));
                ids.push(a.spawn(s, "t").unwrap().id);
            }
            let joint = |w: &mut World, name: String, body: String| {
                let mut s = SpawnSpec::new(WShape::Group);
                s.name = Some(name);
                s.props = Json::parse(&format!(r#"{{"joint": {body}}}"#)).unwrap();
                w.spawn(s, "t").unwrap();
            };
            joint(&mut a, "anchor".into(), format!(r#"{{"kind": "ball", "a": null, "b": {}, "anchor_a": [0, 6, -3], "anchor_b": [-0.5, 0, 0]}}"#, ids[0]));
            for i in 1..6 {
                joint(&mut a, format!("j{i}"), format!(r#"{{"kind": "hinge", "a": {}, "b": {}, "anchor_a": [0.5, 0, 0], "anchor_b": [-0.5, 0, 0], "axis_a": [0, 0, 1], "axis_b": [0, 0, 1], "limit": [-60, 60]}}"#, ids[i - 1], ids[i]));
            }
            let mut pa = Physics { threads, ..Physics::default() };
            for t in 0..steps {
                tick(&mut a, &mut pa, t);
            }
            assert_eq!(pa.space.joints().count(), 6, "{steps}: every joint is in");
            let bytes = pa.save_state(&a, 0).expect("physics ran");
            let files = crate::persist::render(&a);
            let mut b = load(&files);
            let mut pb = Physics { threads, ..Physics::default() };
            pb.restore_state(&b, 0, steps, &bytes).unwrap_or_else(|e| panic!("{steps} ticks, {threads} threads: {e:?}"));
            for t in steps..steps + 45 {
                tick(&mut a, &mut pa, t);
                tick(&mut b, &mut pb, t);
                assert_eq!((a.state_hash(), pa.hash()), (b.state_hash(), pb.hash()), "{steps}: diverged at tick {t}");
            }
        }
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
