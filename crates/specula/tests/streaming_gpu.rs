//! Streaming on a real GPU (skips cleanly without Vulkan): uploads stay within the per-frame
//! byte budget and arrive coarsest LOD first; a camera that moves away and back makes the memory
//! budget evict what it no longer wants (never what it wants); far cells get HLOD proxies that
//! cover their members in a capture. (Streaming only reads the world: `stream_check.py` compares
//! world hashes with streaming on and off.)

use specula::meshes::{LatheMeshes, ShapeMeshes};
use specula::models::{GpuModels, Library, LodChoice, LodStats, SharedLibrary};
use specula::sight::{self, Camera, Meshes, View};
use specula::streaming::{StreamTable, Streamer};
use specula::world::{ModelSource, ModelSpec, SpawnSpec, World};
use specula_core::math::Vec3;
use specula_core::Json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn num(j: &Json, path: &[&str]) -> f64 {
    let mut v = j;
    for k in path {
        v = v.get(k).unwrap_or_else(|| panic!("no {path:?} in {j}"));
    }
    v.as_f64().unwrap_or_else(|| panic!("{path:?} is not a number in {j}"))
}

fn draw(world: &World, models: &GpuModels, shapes: &ShapeMeshes, lathes: &LatheMeshes, camera: Camera) -> LodStats {
    models.begin_frame(1.0);
    let meshes = Meshes { shapes, lathes, models };
    let mut view = View { camera, viewport_h: 720, include_ui: false, lod: LodChoice::Fresh };
    let mut out = Vec::new();
    sight::draw_list(world, &meshes, &mut view, &mut out, None)
}

#[test]
fn budgets_eviction_and_hlod_on_the_gpu() {
    if !specula_gpu::is_available() {
        eprintln!("SKIPPED: Vulkan is not available");
        return;
    }
    let mut gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
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
    // Two clusters 600 m apart: 40 unique cliffs and 32 tombstones each.
    for cluster in 0..2u64 {
        let x0 = cluster as f32 * 600.0;
        for i in 0..72u64 {
            let (recipe, seed) = if i < 40 { ("recipes/slab_cliff.json", 100 + cluster * 100 + i) } else { ("recipes/tombstone.json", 1000 + cluster * 100 + i % 8) };
            let mut s = SpawnSpec::model(ModelSpec { recipe: recipe.into(), params: Vec::new(), seed: Some(seed) });
            s.position = Vec3::new(x0 + (i % 9) as f32 * 12.0, 0.0, (i / 9) as f32 * 12.0);
            s.scale = if i < 40 { Vec3::new(0.6, 0.6, 0.6) } else { Vec3::ONE };
            world.spawn(s, "t").unwrap();
        }
    }
    // Every wait below ends on what it waits for (the builds landed, nothing left to upload,
    // every cell's proxy current), never on a count of frames: a count is the machine's speed
    // (under load the HLOD loop's 600 frames gave up with 95 of 144 models covered, about ten
    // times its usual time; it passed alone in 26 s, 2026-11-24). `HANG_BOUND` only turns a
    // hang into a failure that says what it was waiting for: it bounds each wait from its own
    // start, never the whole test (`HANG_BOUND`'s doc).
    let hung = |started: Instant, what: &str| assert!(started.elapsed() < specula_core::HANG_BOUND, "still waiting for {what} after {:?}: hung?", specula_core::HANG_BOUND);
    let started = Instant::now();
    while shared.lock().in_flight() > 0 {
        hung(started, "the 144 models' builds");
        let (_, changed) = shared.lock().finish(Some(Duration::from_millis(50)));
        world.refresh_model_handles(&changed, false);
    }
    let mut models = GpuModels::default();
    let mut stream = Streamer::new(StreamTable::default(), "integrated");
    let patch = Json::obj([
        ("upload_mb", Json::from(0.25)),
        ("upload_ms", Json::from(50.0)),
        ("memory_mb", Json::from(8.0)),
        ("radius_m", Json::from(200.0)),
        ("settle_s", Json::from(0.0)),
        ("hlod_min_m", Json::from(0.0)),
    ]);
    stream.set(&patch, "test", &mut gpu, &mut models).unwrap();
    let shapes = ShapeMeshes::upload(&mut gpu).unwrap();
    let lathes = LatheMeshes::default();
    let near_a = Camera::new(Vec3::new(50.0, 20.0, -30.0), Vec3::new(50.0, 0.0, 40.0), 60.0);
    let near_b = Camera::new(Vec3::new(650.0, 20.0, -30.0), Vec3::new(650.0, 0.0, 40.0), 60.0);
    let frame = |stream: &mut Streamer, models: &mut GpuModels, gpu: &mut specula_gpu::Gpu, cam: Camera| {
        stream.sync_models(models, gpu, &shared, &world, 0);
        stream.set_window(Some((cam, 1280, 720)));
        stream.begin_frame();
        stream.frame(gpu, models, &shared, &world, 0, true);
        // The window draws (and flushes the batch), here an offscreen render's flush.
        gpu.flush_uploads().unwrap();
        let j = stream.json(gpu, models);
        (num(&j, &["frame", "kb"]), num(&j, &["frame", "uploads"]), j)
    };
    // Near cluster A: uploads trickle within 0.25 MB a frame (one item may exceed it alone).
    let mut first = true;
    let mut saw_coarser = false;
    let started = Instant::now();
    loop {
        hung(started, "cluster A's builds to be uploaded");
        let (kb, uploads, j) = frame(&mut stream, &mut models, &mut gpu, near_a);
        if uploads > 1.0 {
            assert!(kb <= 256.0 + 1.0, "a frame of several uploads stays within upload_mb: {kb} KB");
        }
        if first && uploads > 0.0 {
            first = false;
            assert!(num(&j, &["partial_builds"]) > 0.0, "builds arrive coarsest LOD first: {j}");
        }
        saw_coarser |= draw(&world, &models, &shapes, &lathes, near_a).coarser > 0;
        if num(&j, &["pending_builds"]) == 0.0 && uploads == 0.0 {
            break;
        }
    }
    assert!(saw_coarser, "while finer levels stream, entities draw a coarser one");
    let stats = draw(&world, &models, &shapes, &lathes, near_a);
    // The draw list walks the whole world: all of A (72 models) draws. B (600 m away, beyond
    // radius_m) has no proxy yet, so its members stand in for it as memory allows.
    assert!(stats.histogram.iter().sum::<u32>() >= 72, "{stats:?}");
    let near = stream.state_json(&gpu, &models, &world, Some(Vec3::new(50.0, 0.0, 40.0)), 12);
    let missing = near.get("not_resident_near").and_then(Json::as_array).map_or(99, |a| a.len());
    assert_eq!(missing, 0, "everything near A is resident: {near}");
    assert!(stats.missing == 0);
    let j = stream.json(&gpu, &models);
    assert!(num(&j, &["resident_mb"]) <= 8.0 + 0.01, "{j}");
    // Fly to B: A's builds are evicted to make room (B alone fits the budget).
    let started = Instant::now();
    loop {
        hung(started, "cluster B's builds to be uploaded");
        let (_, uploads, j) = frame(&mut stream, &mut models, &mut gpu, near_b);
        if num(&j, &["pending_builds"]) == 0.0 && uploads == 0.0 {
            break;
        }
    }
    let j = stream.json(&gpu, &models);
    assert!(num(&j, &["totals", "evicted"]) > 0.0, "A's builds were evicted for B's: {j}");
    assert!(num(&j, &["resident_mb"]) <= 8.0 + 0.01, "{j}");
    let stats = draw(&world, &models, &shapes, &lathes, near_b);
    assert!(stats.histogram.iter().sum::<u32>() >= 72, "{stats:?}");
    let near = stream.state_json(&gpu, &models, &world, Some(Vec3::new(650.0, 0.0, 40.0)), 12);
    let missing = near.get("not_resident_near").and_then(Json::as_array).map_or(99, |a| a.len());
    assert_eq!(missing, 0, "everything near B is resident: {near}");
    // Far from both, high up: the cells' proxies cover their members in a capture.
    let far = Camera::new(Vec3::new(300.0, 400.0, -900.0), Vec3::new(300.0, 0.0, 40.0), 40.0);
    // Until every cell with members has a current proxy (or is held back by the memory
    // budget), and nothing is building or waiting to upload.
    let started = Instant::now();
    loop {
        let (_, _, j) = frame(&mut stream, &mut models, &mut gpu, far);
        let h = j.get("hlod").unwrap();
        hung(started, &format!("every cell's proxy to be current: {h}"));
        if num(h, &["current"]) + num(&j, &["blocked_proxies"]) >= num(h, &["cells"]) && num(h, &["building"]) == 0.0 && num(h, &["ready_to_upload"]) == 0.0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut proxies = Vec::new();
    stream.draw_capture(&far, 720, &mut proxies);
    assert!(!proxies.is_empty(), "far cells draw as proxies: {}", stream.json(&gpu, &models));
    let covered = stream.hlod.capture_cover.iter().map(|w| w.count_ones()).sum::<u32>();
    assert!(covered >= 100, "the proxies cover most of the 144 models ({covered})");
    assert!(proxies.len() < covered as usize / 4, "{} proxy draws for {covered} entities", proxies.len());
    let messages = gpu.close();
    let errors: Vec<_> = messages.iter().filter(|m| m.severity == specula_gpu::Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:#?}");
}
