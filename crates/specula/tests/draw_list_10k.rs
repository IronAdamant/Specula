//! Foundation F3: the window's per-frame draw list for a 10,000-entity world (8,000 cubes and
//! 2,000 tombstone models sharing one build, plus a few lamps) allocates nothing once warm,
//! equals the full walk, is patched (never rebuilt) while a game moves 1 or 100 enemies a frame,
//! and is never slower than the full walk it replaces; gathering point lights doesn't visit the
//! cubes. Prints the CPU time per frame; measure with
//! `cargo test --release -p specula --test draw_list_10k -- --nocapture` (the engine's
//! `frame/draw_list` budget on the reference iGPU is 1.5 ms). Needs the GPU for uploads (skips
//! cleanly without Vulkan).
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula::meshes::{LatheMeshes, ShapeMeshes};
use specula::models::{GpuModels, Library, LodChoice, SharedLibrary};
use specula::sight::{self, Camera, DrawCache, Meshes, View};
use specula::world::{EntityKey, LightSpec, ModelSource, ModelSpec, Shape, SpawnSpec, UpdateSpec, World};
use specula_model::{BuildParams, Style};
use specula_core::math::Vec3;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged; only counts allocations.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as our caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as our caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as our caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn a_10k_entity_draw_list_allocates_nothing() {
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
    let mut world = World::default();
    world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
    for i in 0..10_000u32 {
        let pos = Vec3::new((i % 100) as f32 * 1.6 - 80.0, 0.0, -((i / 100) as f32 * 1.6));
        let mut s = if i % 5 == 0 {
            SpawnSpec::model(ModelSpec::new("recipes/tombstone.json"))
        } else {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.scale = Vec3::new(0.45, 0.6, 0.14);
            s
        };
        s.name = Some(format!("e{i:05}"));
        s.position = pos;
        s.rotation_deg = Vec3::new(0.0, (i % 24) as f32 - 12.0, 0.0);
        world.spawn(s, "t").unwrap();
    }
    for k in 0..4 {
        let mut s = SpawnSpec::new(Shape::Sphere);
        s.name = Some(format!("lamp{k}"));
        s.position = Vec3::new(k as f32 * 10.0, 1.0, -5.0);
        s.light = Some(LightSpec { intensity: 2.0, range: 6.0, color: None, offset: Vec3::ZERO });
        world.spawn(s, "t").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    while shared.lock().in_flight() > 0 && Instant::now() < deadline {
        let (_, changed) = shared.lock().finish(Some(Duration::from_millis(50)));
        world.refresh_model_handles(&changed);
    }
    let mut models = GpuModels::default();
    assert!(models.sync(&mut gpu, &shared, &world, 0).is_empty());
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let lathes = LatheMeshes::default();
    let meshes = Meshes { shapes: &shapes, lathes: &lathes, models: &models };
    let camera = Camera::new(Vec3::new(0.0, 9.0, 14.0), Vec3::new(0.0, 0.0, -45.0), 50.0);
    let mut levels = BTreeMap::new();
    let mut draws = Vec::new();
    let mut lights = Vec::new();
    let mut cache = DrawCache::default();
    // One window frame's CPU draw work: the draw list (through the window's cache, or rebuilt
    // from every entity as captures do) and the point lights.
    let frame = |world: &World, cached: Option<&mut DrawCache>, levels: &mut BTreeMap<u64, u8>, draws: &mut Vec<specula_gpu::DrawItem>,
                 lights: &mut Vec<specula_gpu::PointLight>| {
        models.begin_frame(1.0);
        let mut view = View { camera, viewport_h: 720, include_ui: true, lod: LodChoice::Remembered(levels) };
        let stats = match cached {
            Some(cache) => cache.draw_list(world, 0, &meshes, &mut view, draws),
            None => sight::draw_list(world, &meshes, &mut view, draws),
        };
        sight::lights(world, true, lights);
        let bones = models.take_bones();
        models.put_bones(bones);
        stats
    };
    for _ in 0..5 {
        frame(&world, Some(&mut cache), &mut levels, &mut draws, &mut lights);
    }
    let stats = frame(&world, Some(&mut cache), &mut levels, &mut draws, &mut lights);
    assert_eq!(stats.missing, 0, "every tombstone has its build");
    assert_eq!(stats.histogram.iter().sum::<u32>(), 2000);
    // Draws per tombstone: its material parts at the level the frame chose, from the recipe.
    let style = Style::parse(&std::fs::read_to_string(assets.join("style.json")).unwrap()).unwrap();
    let tomb = specula_model::build(&std::fs::read_to_string(assets.join("recipes/tombstone.json")).unwrap(), &BuildParams::new(&style)).unwrap();
    let level = stats.histogram.iter().position(|&n| n == 2000).expect("one level for every tombstone");
    let parts = tomb.lods[level.min(tomb.lods.len() - 1)].parts.len();
    assert_eq!(draws.len(), 8004 + 2000 * parts, "one draw per cube and lamp, {parts} per tombstone");
    assert_eq!(lights.len(), 4);
    let mut uncached = Vec::new();
    frame(&world, None, &mut levels, &mut uncached, &mut lights);
    assert!(uncached == draws, "the window's cached draw list is the full one");
    // Enemies a hosted game moves every tick (transient, as Sepulcra's are).
    let enemies: Vec<u64> = (0..100)
        .map(|k| {
            let mut s = SpawnSpec::new(Shape::Sphere);
            s.name = Some(format!("enemy{k:03}"));
            s.transient = true;
            s.position = Vec3::new(k as f32 * 0.5 - 25.0, 0.5, -10.0);
            world.spawn(s, "game").unwrap().id
        })
        .collect();
    // Stone markers an agent moves (persistent: the cache patches them in).
    let markers: Vec<u64> = world.entities().filter(|e| e.shape == Shape::Cube).take(100).map(|e| e.id).collect();
    let step = |world: &mut World, moves: usize, persistent: bool, k: usize| {
        let ids = if persistent { &markers } else { &enemies };
        for (j, &id) in ids.iter().take(moves).enumerate() {
            let p = Vec3::new(j as f32 * 0.5 - 25.0, 0.5, -10.0 - (k % 50) as f32 * 0.1);
            world.update(&EntityKey::Id(id), UpdateSpec { position: Some(p), ..UpdateSpec::default() }, "game").unwrap();
        }
    };
    let frames = 120;
    frame(&world, Some(&mut cache), &mut levels, &mut draws, &mut lights);
    let rebuilds = cache.rebuilds();
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..frames {
        frame(&world, Some(&mut cache), &mut levels, &mut draws, &mut lights);
        frame(&world, None, &mut levels, &mut uncached, &mut lights);
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(allocations, 0, "warm 10k-entity frames allocate");
    assert_eq!(cache.rebuilds(), rebuilds, "a world that doesn't change keeps its draws");
    // Moving enemies: the cache is patched (no rebuild), stays equal to the full walk and
    // allocates nothing once warm.
    for k in 0..10 {
        step(&mut world, 100, k % 2 == 0, k);
        frame(&world, Some(&mut cache), &mut levels, &mut draws, &mut lights);
    }
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for k in 0..frames {
        // (the world's updates themselves allocate nothing either: tests/transient_alloc.rs)
        step(&mut world, if k % 2 == 0 { 1 } else { 100 }, k % 4 < 2, k);
        frame(&world, Some(&mut cache), &mut levels, &mut draws, &mut lights);
    }
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed) - before, 0, "patched frames allocate");
    assert_eq!(cache.rebuilds(), rebuilds, "moving markers are patched in and enemies drawn live, never rebuilt");
    assert!(cache.patches() > 0);
    frame(&world, None, &mut levels, &mut uncached, &mut lights);
    assert!(uncached == draws, "the patched draw list is the full one");
    // Timings, warm and with cold caches (as in the window, where the GPU driver, presentation
    // and, on an iGPU, the GPU itself evict the world between frames: sweep 64 MB first).
    let mut sweep = vec![0u8; 64 << 20];
    let mut medians = BTreeMap::new();
    let modes = [(0usize, false), (1, false), (100, false), (1, true), (100, true)];
    for (moves, persistent) in modes {
        let what = if persistent { "markers (persistent)" } else { "enemies (transient)" };
        for (label, cached) in [("full walk", false), ("window cache", true)] {
            let (mut warm, mut cold) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
            // The full walk's frames moved enemies without the cache seeing them: one catch-up
            // frame first (a rebuild when that overflowed the world's change record).
            frame(&world, cached.then_some(&mut cache), &mut levels, &mut draws, &mut lights);
            let rebuilds = cache.rebuilds();
            for k in 0..frames * 2 {
                step(&mut world, moves, persistent, k);
                let is_cold = k % 2 == 1;
                if is_cold {
                    for (i, b) in sweep.iter_mut().enumerate().step_by(64) {
                        *b = b.wrapping_add((i + k) as u8);
                    }
                }
                let start = Instant::now();
                frame(&world, cached.then_some(&mut cache), &mut levels, &mut draws, &mut lights);
                let ms = start.elapsed().as_secs_f64() * 1e3;
                if is_cold { cold.push(ms) } else { warm.push(ms) }
            }
            warm.sort_by(f64::total_cmp);
            cold.sort_by(f64::total_cmp);
            eprintln!("10k entities, {moves} {what} moved per frame, {label}: warm {:.3} ms median; cold caches {:.3} ms median, {:.3} ms p95",
                      warm[frames / 2], cold[frames / 2], cold[frames * 95 / 100]);
            medians.insert((moves, persistent, cached), (warm[frames / 2], cold[frames / 2]));
            assert_eq!(cache.rebuilds(), rebuilds, "{moves} {what} per frame: never rebuilt while timing");
        }
    }
    std::hint::black_box(&sweep);
    // The cache must never cost more than the walk it replaces (beyond noise). Timings mean
    // something only in an optimised build.
    if !cfg!(debug_assertions) {
        for (moves, persistent) in modes {
            let (walk, cache) = (medians[&(moves, persistent, false)], medians[&(moves, persistent, true)]);
            assert!(cache.0 <= walk.0 * 1.1 + 0.02 && cache.1 <= walk.1 * 1.1 + 0.02,
                    "{moves} moves per frame (persistent {persistent}): the cache ({cache:?} ms warm, cold) is slower than the full walk ({walk:?})");
        }
    }
    drop(models);
    drop(gpu);
}
