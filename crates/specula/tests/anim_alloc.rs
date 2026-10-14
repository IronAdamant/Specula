//! Foundation F3: drawing hundreds of animated (transient, game-style) creatures allocates
//! nothing once warm: posing each at the frame's time, writing its palette into the frame's bone
//! list and appending its draws. Needs the GPU for uploads (skips cleanly without Vulkan).
//!
//! The only test in this file, so the counting allocator sees no other test's threads.

use specula::models::{GpuModels, Library, LodChoice, LodStats, SharedLibrary};
use specula::world::{AnimSpec, ModelSource, ModelSpec, SpawnSpec, World};
use specula_core::math::Vec3;
use std::alloc::{GlobalAlloc, Layout, System};
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
fn drawing_300_animated_creatures_allocates_nothing() {
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
    let kinds = [("recipes/shambler.json", "walk"), ("recipes/skeleton_runner.json", "sprint"), ("recipes/wraith.json", "float")];
    let mut ids = Vec::new();
    for i in 0..300 {
        let (recipe, clip) = kinds[i % kinds.len()];
        let mut s = SpawnSpec::model(ModelSpec::new(recipe));
        s.position = Vec3::new((i % 20) as f32 * 2.0, 0.0, (i / 20) as f32 * 2.0);
        s.transient = true;
        let mut a = AnimSpec::new(clip);
        a.time_offset = i as f32 * 0.137;
        s.anim = Some(a);
        ids.push(world.spawn(s, "game").unwrap().id);
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    while shared.lock().in_flight() > 0 && Instant::now() < deadline {
        let (_, changed) = shared.lock().finish(Some(Duration::from_millis(50)));
        world.refresh_model_handles(&changed);
    }
    let mut models = GpuModels::default();
    assert!(models.sync(&mut gpu, &shared, &world, 0).is_empty());
    let mut out = Vec::with_capacity(4096);
    let frame = |models: &GpuModels, t: f64, out: &mut Vec<specula_gpu::DrawItem>| {
        out.clear();
        models.begin_frame(t);
        let mut stats = LodStats::default();
        for &id in &ids {
            let e = world.get(id).unwrap();
            models.draw(e, Vec3::new(20.0, 10.0, 40.0), 50.0, 1080, &mut LodChoice::Fixed(0), &mut stats, out);
        }
        let bones = models.take_bones();
        let n = bones.len();
        models.put_bones(bones);
        n
    };
    let mut bones = 0;
    for k in 0..10 {
        bones = frame(&models, k as f64 / 60.0, &mut out);
    }
    assert!(bones >= 300 * 10, "{bones} bone matrices for 300 creatures");
    assert!(out.iter().all(|d| d.skin.is_some()), "every creature draws posed");
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    for k in 10..130 {
        frame(&models, k as f64 / 60.0, &mut out);
    }
    let per_frame_ms = start.elapsed().as_secs_f64() * 1e3 / 120.0;
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    eprintln!("300 animated creatures: {per_frame_ms:.2} ms CPU per frame (posing + palettes + draws), {bones} bone matrices");
    assert_eq!(allocations, 0, "warm animated frames allocate");
    drop(models);
    drop(gpu);
}
