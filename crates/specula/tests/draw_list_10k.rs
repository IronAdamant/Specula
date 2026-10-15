//! Foundation F3: the window's per-frame draw list for a 10,000-entity world (8,000 cubes and
//! 2,000 tombstone models sharing one build, plus a few lamps) allocates nothing once warm, and
//! gathering point lights doesn't visit the cubes. Prints the CPU time per frame; measure with
//! `cargo test --release -p specula --test draw_list_10k -- --nocapture` (the engine's
//! `frame/draw_list` budget on the reference iGPU is 1.5 ms). Needs the GPU for uploads (skips
//! cleanly without Vulkan).
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula::meshes::{LatheMeshes, ShapeMeshes};
use specula::models::{GpuModels, Library, LodChoice, SharedLibrary};
use specula::sight::{self, Camera, DrawCache, Meshes, View};
use specula::world::{LightSpec, ModelSource, ModelSpec, Shape, SpawnSpec, World};
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
    let frame = |cached: Option<&mut DrawCache>, levels: &mut BTreeMap<u64, u8>, draws: &mut Vec<specula_gpu::DrawItem>,
                 lights: &mut Vec<specula_gpu::PointLight>| {
        models.begin_frame(1.0);
        let mut view = View { camera, viewport_h: 720, include_ui: true, lod: LodChoice::Remembered(levels) };
        let stats = match cached {
            Some(cache) => cache.draw_list(&world, 0, &meshes, &mut view, draws),
            None => sight::draw_list(&world, &meshes, &mut view, draws),
        };
        sight::lights(&world, true, lights);
        let bones = models.take_bones();
        models.put_bones(bones);
        stats
    };
    for _ in 0..5 {
        frame(Some(&mut cache), &mut levels, &mut draws, &mut lights);
    }
    let stats = frame(Some(&mut cache), &mut levels, &mut draws, &mut lights);
    assert_eq!(stats.missing, 0, "every tombstone has its build");
    assert_eq!(stats.histogram.iter().sum::<u32>(), 2000);
    assert_eq!(draws.len(), 8004 + 2000 * models_parts(&draws), "one draw per cube and lamp, one per part per tombstone");
    assert_eq!(lights.len(), 4);
    let mut uncached = Vec::new();
    frame(None, &mut levels, &mut uncached, &mut lights);
    assert!(uncached == draws, "the window's cached draw list is the full one");
    let frames = 120;
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..frames {
        frame(Some(&mut cache), &mut levels, &mut draws, &mut lights);
        frame(None, &mut levels, &mut uncached, &mut lights);
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(allocations, 0, "warm 10k-entity frames allocate");
    assert_eq!(cache.rebuilds(), 1, "a world that doesn't change keeps its fixed draws");
    // Timings, warm and with cold caches (as in the window, where the GPU driver, presentation
    // and, on an iGPU, the GPU itself evict the world between frames: sweep 64 MB first).
    let mut sweep = vec![0u8; 64 << 20];
    for (label, cached) in [("full walk", false), ("window cache", true)] {
        let (mut warm, mut cold) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
        for k in 0..frames * 2 {
            let is_cold = k % 2 == 1;
            if is_cold {
                for (i, b) in sweep.iter_mut().enumerate().step_by(64) {
                    *b = b.wrapping_add((i + k) as u8);
                }
            }
            let start = Instant::now();
            frame(cached.then_some(&mut cache), &mut levels, &mut draws, &mut lights);
            let ms = start.elapsed().as_secs_f64() * 1e3;
            if is_cold { cold.push(ms) } else { warm.push(ms) }
        }
        warm.sort_by(f64::total_cmp);
        cold.sort_by(f64::total_cmp);
        eprintln!("10k entities, {label}: warm {:.3} ms median; cold caches {:.3} ms median, {:.3} ms p95", warm[frames / 2],
                  cold[frames / 2], cold[frames * 95 / 100]);
    }
    std::hint::black_box(&sweep);
    drop(models);
    drop(gpu);
}

/// Draws per tombstone (its material parts at the chosen LOD), from the draw list itself.
fn models_parts(draws: &[specula_gpu::DrawItem]) -> usize {
    let extra = draws.len() - 8004;
    assert_eq!(extra % 2000, 0, "{} draws", draws.len());
    extra / 2000
}
