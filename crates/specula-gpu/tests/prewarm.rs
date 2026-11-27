//! A new size's render graph images made ahead of its first render (`Gpu::prewarm_target`), on
//! real GPUs under validation: sliced (one image a call when out of time), complete, and the first
//! render at that size then makes no image of its own (its prepare finds every one) and draws the
//! same pixels as a target that was never prewarmed. Skips cleanly without Vulkan.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, Gpu, BloomSettings, DrawItem, Environment, FrameDesc, MemoryKind, ShadowSettings, SsaoSettings};
use std::time::Instant;

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap_or_default() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

/// Shadows, bloom, SSAO and FXAA: the most images a frame has.
fn env() -> Environment {
    Environment {
        ambient_intensity: 1.0,
        sun_intensity: 0.8,
        shadow: ShadowSettings { enabled: true, ..Default::default() },
        bloom: BloomSettings { enabled: true, ..Default::default() },
        ssao: SsaoSettings { enabled: true, ..Default::default() },
        fxaa: true,
        ..Default::default()
    }
}

fn frame(draws: &[DrawItem], w: u32, h: u32) -> FrameDesc<'_> {
    let view = look_at_rh([0.0, 2.0, 6.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(45f32.to_radians(), w as f32 / h as f32, 0.05), &view);
    FrameDesc { view_proj, clear_color: [0.05, 0.05, 0.08, 1.0], draws, reverse_z: true, environment: env(), ..Default::default() }
}

#[test]
fn a_prewarmed_size_renders_without_making_images() {
    every_adapter("a_prewarmed_size_renders_without_making_images", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let (cv, ci) = cube();
        let cube = gpu.upload_mesh(&cv, &ci).unwrap();
        let draws = vec![
            DrawItem::new(cube, scale_translation(6.0, [0.0, -3.5, 0.0]), [0.5, 0.5, 0.55, 1.0]),
            DrawItem::new(cube, translation([0.0, 0.5, 0.0]), [0.9, 0.7, 0.2, 1.0]),
        ];
        // A first size renders: the content the prediction uses (shadow casters, no 2D, ...).
        let small = gpu.create_target(160, 120).unwrap();
        gpu.render_with_ids(small, &frame(&draws, 160, 120), &[1, 2]).unwrap();

        // A new size, prepared a slice at a time: out of time, one image a call.
        let (w, h) = (640, 400);
        let target = gpu.create_target(w, h).unwrap();
        let mut calls = 0;
        let started = Instant::now();
        while !gpu.prewarm_target(target, &env(), true, None, Instant::now()).unwrap() {
            calls += 1;
            // A slice may also wait for the image heap's worker to make room (deferred, never
            // allocated in the call): give it a moment.
            std::thread::sleep(std::time::Duration::from_millis(1));
            assert!(started.elapsed() < specula_core::HANG_BOUND, "{test}: the prewarm has not finished in {:?} ({calls} calls): hung?", specula_core::HANG_BOUND);
        }
        assert!(calls >= 4, "{test}: a frame with shadows, bloom, SSAO and FXAA has several images, made one a call ({calls} calls)");
        assert_eq!(gpu.target_images_missing(target), 0, "{test}: every image exists");
        // Done is done: another call makes nothing. (The image heap's blocks come and go on its
        // worker: what is counted is every other allocation, and the images in the heap.)
        let images = |s: &specula_gpu::MemoryStats| (s.allocations - s.image_heap.blocks, s.image_heap.images);
        let before = images(&gpu.memory_stats());
        assert!(gpu.prewarm_target(target, &env(), true, None, Instant::now()).unwrap());
        assert_eq!(images(&gpu.memory_stats()), before, "{test}: a finished prewarm makes nothing");
        let heap = gpu.memory_stats().image_heap;
        assert_eq!(heap.sync_blocks, 0, "{test}: no image heap block was allocated on the caller's thread: {heap:?}");

        // The first render at the new size makes no image of its own.
        let stats = gpu.memory_stats();
        gpu.render_with_ids(target, &frame(&draws, w, h), &[1, 2]).unwrap();
        let after = gpu.memory_stats();
        assert_eq!((images(&after).1, after.bytes(MemoryKind::PostTargets), after.bytes(MemoryKind::RenderTargets)),
            (images(&stats).1, stats.bytes(MemoryKind::PostTargets), stats.bytes(MemoryKind::RenderTargets)), "{test}: the first render found every image");
        // Nor any buffer: the ID readback made ahead replaced the smaller one (one allocation
        // fewer), nothing was allocated in the render.
        assert!(images(&after).0 <= images(&stats).0, "{test}: the first render allocated: {stats:?} -> {after:?}");
        assert_eq!(after.image_heap.sync_blocks, 0, "{test}: {:?}", after.image_heap);
        assert!(after.image_heap.buffers_ahead >= 2, "{test}: the target's readback and the ID readback were made ahead: {:?}", after.image_heap);
        assert_eq!(gpu.target_images_missing(target), 0);
        let warm = gpu.read_rgba8(target).unwrap();
        let counts = gpu.id_coverage(target, 2).unwrap().per_id;
        assert!(counts.iter().all(|c| c.pixels > 0), "{test}: both draws seen: {counts:?}");

        // The same pixels as a size rendered cold (whose render makes the post images itself:
        // the measure above sees image creation).
        gpu.destroy_target(target);
        let cold = gpu.create_target(w, h).unwrap();
        let post = |g: &Gpu| g.memory_stats().bytes(MemoryKind::PostTargets);
        let before = post(&gpu);
        gpu.render_with_ids(cold, &frame(&draws, w, h), &[1, 2]).unwrap();
        assert!(post(&gpu) > before, "{test}: a cold render makes its post images");
        assert_eq!(gpu.read_rgba8(cold).unwrap(), warm, "{test}: the same pixels with or without the prewarm");
        write_png(&format!("prewarm_{}", test.replace(['[', ']'], "_")), &warm, w, h);

        // A target destroyed mid-prewarm frees what was made (validation reports leaks at close).
        let half = gpu.create_target(1024, 640).unwrap();
        assert!(!gpu.prewarm_target(half, &env(), true, None, Instant::now()).unwrap());
        gpu.destroy_target(half);
        let messages = gpu.close();
        assert!(messages.is_empty(), "{test}: validation messages: {messages:?}");
    });
}
