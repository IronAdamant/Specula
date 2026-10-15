//! The window's draw cache (`sight::DrawCache`, patched from the world's change record) draws
//! exactly what the full walk (`sight::draw_list`, with the window's remembered LODs) draws:
//! the same draws in the same order, the same LOD statistics and the same bone matrices, frame
//! after frame, while a fuzzer spawns, changes, reparents, hides, deletes and undoes entities
//! (shapes, lathes, sprites, real model builds, animated creatures, lights, transient ones),
//! rolls back failed batches, replays the journal, saves and loads the world, toggles the ui
//! layer, and moves the camera. Needs the GPU for uploads (skips cleanly without Vulkan).
//!
//! Ported from the M2 exit lane review's probe.

use specula::journal::{commit, Journal, Kind, UndoRequest};
use specula::meshes::{LatheMeshes, ShapeMeshes};
use specula::models::{GpuModels, Library, LodChoice, SharedLibrary};
use specula::sight::{self, Camera, DrawCache, Meshes, View};
use specula::world::{
    AnimSpec, Change, EntityKey, Layer, LightSpec, ModelSource, ModelSpec, Op, Profile, Recording, Shape, SpawnSpec,
    UpdateSpec, World, WorldError,
};
use specula_core::math::Vec3;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n.max(1)
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next(1_000_000) as f32 / 1_000_000.0)
    }
}

fn no_check(_: &World, _: &Change) -> Result<(), WorldError> {
    Ok(())
}

fn check_indexes(w: &World) {
    let lit: Vec<u64> = w.entities().filter(|e| e.light.is_some()).map(|e| e.id).collect();
    let idx: Vec<u64> = w.lit_entities().map(|e| e.id).collect();
    assert_eq!(lit, idx, "lit index");
    let flat: Vec<u64> = w.entities().filter(|e| e.two_d.is_some()).map(|e| e.id).collect();
    let idx: Vec<u64> = w.two_d_entities().map(|e| e.id).collect();
    assert_eq!(flat, idx, "2d index");
}

#[test]
fn draw_cache_matches_the_full_walk_under_fuzz() {
    if !specula_gpu::is_available() {
        eprintln!("SKIPPED: Vulkan is not available");
        return;
    }
    let mut gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Off, ..Default::default() }) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let shared = Arc::new(SharedLibrary::new(Library::new(&assets, None)));
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let mut models = GpuModels::default();
    let seeds: [u64; 3] = [1, 2, 3];
    let steps: u32 = if cfg!(debug_assertions) { 600 } else { 1500 };
    let recipes = ["recipes/tombstone.json", "recipes/shambler.json", "recipes/warding_stone.json"];
    for seed in seeds {
        let mut r = Lcg(seed);
        let mut lathes = LatheMeshes::default();
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        world.set_recording(Recording::Full);
        let mut journal = Journal::default();
        journal.rebase(&world, "world:t");
        let mut generation = 0u64;
        let mut cache = DrawCache::default();
        let mut remembered: BTreeMap<u64, u8> = BTreeMap::new();
        let mut eye = Vec3::new(0.0, 3.0, 10.0);
        let mut include_ui = true;
        let mut names = 0u32;
        let mut mismatches = 0;
        let mut drawn_models = 0u64;
        let mut lod_hist = [0u32; 8];
        for step in 0..steps {
            let ids: Vec<u64> = world.entities().map(|e| e.id).collect();
            let step64 = u64::from(step);
            let pick = |r: &mut Lcg| -> Option<u64> { if ids.is_empty() { None } else { Some(ids[r.next(ids.len() as u64) as usize]) } };
            let op = r.next(32);
            let agent = "ann";
            match op {
                0..=4 => {
                    let kind = r.next(7);
                    let mut s = match kind {
                        0 => SpawnSpec::new(Shape::Cube),
                        1 => SpawnSpec::new(Shape::Sphere),
                        2 => SpawnSpec::new(Shape::Group),
                        3 => {
                            let mut s = SpawnSpec::new(Shape::Lathe);
                            s.profile = Some(Profile { points: vec![[0.0, 0.0], [0.3 + r.next(3) as f32 * 0.1, 0.0], [0.2, 1.0], [0.0, 1.0]], segments: 12 });
                            s
                        }
                        4 => {
                            let mut s = SpawnSpec::new(Shape::Sprite);
                            let j = specula_core::Json::parse("{\"size\":[1,1]}").unwrap();
                            s.two_d = Some(specula::two_d::parse("sprite", &j).unwrap());
                            s
                        }
                        _ => {
                            let rec = recipes[r.next(3) as usize];
                            let mut s = SpawnSpec::model(ModelSpec::new(rec));
                            if rec.contains("shambler") && r.next(2) == 0 {
                                s.anim = Some(AnimSpec::new("walk"));
                            }
                            s
                        }
                    };
                    names += 1;
                    s.name = Some(format!("n{names}"));
                    s.position = Vec3::new(r.f(-20.0, 20.0), 0.0, r.f(-60.0, 5.0));
                    if r.next(3) == 0 {
                        s.parent = pick(&mut r).map(EntityKey::Id);
                    }
                    if r.next(6) == 0 {
                        s.light = Some(LightSpec { intensity: 1.0, range: 3.0, color: None, offset: Vec3::ZERO });
                    }
                    let transient = r.next(5) == 0;
                    s.transient = transient;
                    if transient {
                        let _ = world.spawn(s, "game");
                    } else {
                        let _ = commit(&mut world, &mut journal, step64, agent, Kind::Spawn, Some(&mut no_check), |w| w.spawn(s, agent).map(|_| ()));
                    }
                }
                5..=10 => {
                    let Some(id) = pick(&mut r) else { continue };
                    let mut u = UpdateSpec::default();
                    match r.next(9) {
                        0 => u.position = Some(Vec3::new(r.f(-20.0, 20.0), r.f(-1.0, 3.0), r.f(-80.0, 5.0))),
                        1 => u.color = Some([r.f(0.0, 1.0), 0.5, 0.5]),
                        2 => u.visible = Some(r.next(2) == 0),
                        3 => u.layer = Some(if r.next(2) == 0 { Layer::Ui } else { Layer::World }),
                        4 => u.light = Some(if r.next(2) == 0 { None } else { Some(LightSpec { intensity: 2.0, range: 2.0, color: None, offset: Vec3::ZERO }) }),
                        5 => u.parent = Some(if r.next(3) == 0 { None } else { pick(&mut r).map(EntityKey::Id) }),
                        6 => u.anim = Some(if r.next(2) == 0 { None } else { Some(AnimSpec::new("idle")) }),
                        7 => u.scale = Some(Vec3::new(r.f(0.2, 3.0), r.f(0.2, 3.0), r.f(0.2, 3.0))),
                        _ => u.params = Some(vec![("tier".into(), (1 + r.next(3)) as f64)]),
                    }
                    let transient = world.get(id).is_some_and(|e| e.transient);
                    if transient {
                        let _ = world.update(&EntityKey::Id(id), u, "game");
                    } else {
                        let _ = commit(&mut world, &mut journal, step64, agent, Kind::Set, Some(&mut no_check), |w| w.update(&EntityKey::Id(id), u, agent).map(|_| ()));
                    }
                }
                11..=12 => {
                    let Some(id) = pick(&mut r) else { continue };
                    let transient = world.get(id).is_some_and(|e| e.transient);
                    if transient {
                        let _ = world.delete(&EntityKey::Id(id), None);
                    } else {
                        let _ = commit(&mut world, &mut journal, step64, agent, Kind::Delete, Some(&mut no_check), |w| w.delete(&EntityKey::Id(id), None).map(|_| ()));
                    }
                }
                13 => {
                    let Some(id) = pick(&mut r) else { continue };
                    let mut s = SpawnSpec::new(Shape::Cube);
                    s.light = Some(LightSpec { intensity: 1.0, range: 3.0, color: None, offset: Vec3::ZERO });
                    names += 1;
                    s.name = Some(format!("n{names}"));
                    let ops = vec![
                        Op::Spawn(s),
                        Op::Set(EntityKey::Id(id), UpdateSpec { position: Some(Vec3::new(9.0, 9.0, 9.0)), light: Some(None), visible: Some(false), ..Default::default() }),
                        Op::Delete(EntityKey::Path("/nope/never".into()), None),
                    ];
                    let r2 = commit(&mut world, &mut journal, step64, agent, Kind::Batch, Some(&mut no_check), |w| w.apply_batch(ops, agent).map(|_| ()).map_err(|(_, e)| e));
                    assert!(r2.is_err());
                }
                14..=15 => {
                    let n = 1 + r.next(3) as usize;
                    let _ = journal.undo(&mut world, step64, agent, &UndoRequest { ids: None, count: n, force: true }, Some(&mut no_check));
                }
                16 => {
                    if r.next(2) == 0 {
                        if let Some(w) = journal.replay() {
                            let mut w = w;
                            w.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
                            w.set_recording(Recording::Full);
                            world = w;
                            journal.rebase(&world, "world:t");
                            generation += 1;
                            remembered.clear();
                        }
                    } else {
                        let files = specula::persist::render(&world);
                        let read = |f: &str| files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| specula::persist::PersistError::new("not_found", f));
                        let (mut w, _) = specula::persist::load_with(read).unwrap();
                        w.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
                        w.set_recording(Recording::Full);
                        world = w;
                        journal.rebase(&world, "world:t");
                        generation += 1;
                        remembered.clear();
                    }
                }
                17 => include_ui = !include_ui,
                _ => {}
            }
            eye = Vec3::new(eye.x + r.f(-2.0, 2.0), 3.0, if r.next(10) == 0 { r.f(-70.0, 60.0) } else { eye.z + r.f(-3.0, 3.0) });
            let wait = if r.next(4) == 0 { 30 } else { 0 };
            let (_, changed) = shared.lock().finish(Some(Duration::from_millis(wait)));
            if !changed.is_empty() {
                world.refresh_model_handles(&changed);
            }
            models.refresh_ready(&mut gpu);
            lathes.sync(&mut gpu, &world, generation);
            models.sync(&mut gpu, &shared, &world, generation);
            check_indexes(&world);
            // The window's levels live as long as their entity (as the engine's map did).
            remembered.retain(|id, _| world.get(*id).is_some());
            let camera = Camera::new(eye, Vec3::new(0.0, 0.0, -30.0), 50.0);
            let meshes = Meshes { shapes: &shapes, lathes: &lathes, models: &models };
            let t = f64::from(step) * 0.016;
            models.begin_frame(t);
            let mut a = Vec::new();
            let mut v = View { camera, viewport_h: 720, include_ui, lod: LodChoice::Remembered(&mut remembered) };
            let sa = sight::draw_list(&world, &meshes, &mut v, &mut a, None);
            let ba = models.take_bones();
            let ba2 = ba.clone();
            models.put_bones(ba);
            models.begin_frame(t);
            let mut b = Vec::new();
            let mut v = View { camera, viewport_h: 720, include_ui, lod: LodChoice::Fresh };
            let sb = cache.draw_list(&world, generation, &meshes, &mut v, &mut b);
            let bb = models.take_bones();
            let bb2 = bb.clone();
            models.put_bones(bb);
            drawn_models += sb.histogram.iter().map(|x| *x as u64).sum::<u64>();
            for (i, x) in sb.histogram.iter().enumerate() {
                lod_hist[i.min(7)] += *x;
            }
            if a != b || sa != sb || ba2 != bb2 {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("seed {seed} step {step} op {op}: MISMATCH draws {} vs {} stats {:?} vs {:?} bones_eq {}", a.len(), b.len(), sa, sb, ba2 == bb2);
                }
            }
        }
        eprintln!(
            "seed {seed}: entities {} models drawn {drawn_models} lod hist {lod_hist:?} rebuilds {} patches {} mismatches {mismatches}",
            world.len(),
            cache.rebuilds(),
            cache.patches()
        );
        assert_eq!(mismatches, 0, "seed {seed}: the cache drew differently from the full walk");
        assert!(drawn_models > 0, "seed {seed}: models were drawn");
        assert!(cache.patches() > cache.rebuilds(), "seed {seed}: most frames patch instead of rebuilding");
    }
    drop(models);
    drop(gpu);
}
