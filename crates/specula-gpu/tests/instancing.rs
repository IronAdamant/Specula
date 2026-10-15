//! Instanced batches, frustum culling, frame statistics and the pipeline cache on real GPUs.
//! Every test skips cleanly (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{
    AdapterPreference, CacheRejection, DrawItem, FramePath, FrameDesc, Gpu, GpuOptions, MemoryKind, PipelineCacheLoad, Validation,
    PASS_SCENE,
};

const W: u32 = 320;
const H: u32 = 320;
const GRID: usize = 10;
const SPACING: f32 = 1.5;

fn grid_colour(i: usize) -> [f32; 4] {
    [(i % GRID) as f32 / (GRID - 1) as f32, (i / GRID) as f32 / (GRID - 1) as f32, 0.5, 1.0]
}

fn grid_position(i: usize) -> V3 {
    let half = (GRID - 1) as f32 * SPACING / 2.0;
    [(i % GRID) as f32 * SPACING - half, (i / GRID) as f32 * SPACING - half, 0.0]
}

/// 100 cubes of one mesh with distinct colours, 10 cubes of a second mesh interleaved among them,
/// and 50 cubes behind the camera: two draw calls, 50 culled, every visible cube lit exactly as
/// a single draw would be.
fn instanced_grid_on(adapter: AdapterPreference, label: &str) {
    let test = format!("instanced_grid[{label}]");
    let Some(mut gpu) = gpu_or_skip(&test, adapter) else { return };
    eprintln!("{test}: {} (validation {})", gpu.adapter().name, gpu.validation_enabled());
    let (vertices, indices) = cube();
    let mesh_a = gpu.upload_mesh(&vertices, &indices).unwrap();
    let mesh_b = gpu.upload_mesh(&vertices, &indices).unwrap();
    let target = gpu.create_target(W, H).unwrap();
    gpu.mark_warm();
    let at_start = gpu.last_frame_stats().pipelines.created;

    let eye = [0.0, 0.0, 20.0];
    let view = look_at_rh(eye, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let mut draws = Vec::new();
    for i in 0..GRID * GRID {
        // Every tenth cube uses the second mesh: list order interleaves the meshes.
        let mesh = if i % 10 == 3 { mesh_b } else { mesh_a };
        draws.push(DrawItem::new(mesh, translation(grid_position(i)), grid_colour(i)));
        if i % 2 == 0 {
            // Behind the camera: culled.
            draws.push(DrawItem::new(mesh_a, translation([grid_position(i)[0], 0.0, 40.0]), [1.0, 0.0, 0.0, 1.0]));
        }
    }
    for reverse_z in [false, true] {
        let proj = if reverse_z {
            perspective_vk_reverse_z(45f32.to_radians(), 1.0, 0.05)
        } else {
            perspective_vk(45f32.to_radians(), 1.0, 0.05, 100.0)
        };
        let view_proj = mul(&proj, &view);
        let frame = FrameDesc { view_proj, clear_color: [0.0, 0.0, 0.0, 1.0], draws: &draws, reverse_z, ..Default::default() };
        gpu.render(target, &frame).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        if reverse_z {
            write_png(&format!("instanced_grid_{label}.png"), &rgba, W, H);
        }

        let ndl = dot([0.0, 0.0, 1.0], light_dir());
        for i in 0..GRID * GRID {
            let (x, y) = project(&view_proj, grid_position(i), W, H);
            let got = pixel(&rgba, W, x as u32, y as u32);
            let c = grid_colour(i);
            let want = [expected_lit(c[0], ndl), expected_lit(c[1], ndl), expected_lit(c[2], ndl), 255];
            for k in 0..4 {
                assert!(got[k].abs_diff(want[k]) <= 2, "{test}: cube {i} (reverse_z {reverse_z}) got {got:?}, want {want:?}");
            }
        }

        let s = gpu.last_frame_stats();
        eprintln!("{test}: reverse_z {reverse_z}: {s:#?}");
        assert_eq!(s.path, FramePath::Offscreen);
        assert_eq!((s.width, s.height), (W, H));
        assert_eq!(s.draws.submitted, 150);
        assert_eq!(s.draws.culled, 50);
        assert_eq!(s.draws.draw_calls, 2, "one instanced call per mesh");
        assert_eq!(s.draws.instances, 100);
        assert_eq!(s.draws.unbatched_draw_calls, 100);
        assert_eq!(s.draws.triangles, 100 * 12);
        assert_eq!(s.draws.pipeline_binds, 1);
        assert!(s.draw_buffer_bytes >= 768 + 100 * 112, "{}", s.draw_buffer_bytes);
        assert!(s.cpu.record_ms > 0.0 && s.cpu.wait_ms >= 0.0);
        if s.timestamps_supported {
            let gpu_t = s.gpu.as_ref().expect("synchronous renders have their timings at once");
            assert_eq!(gpu_t.frame, s.frame);
            assert_eq!(gpu_t.passes.len(), 1);
            assert_eq!(gpu_t.passes[0].name, PASS_SCENE);
            assert!(gpu_t.passes[0].gpu_ms > 0.0 && gpu_t.passes[0].gpu_ms < 1000.0, "{gpu_t:?}");
            assert!(gpu_t.total_ms >= gpu_t.passes[0].gpu_ms);
        } else {
            eprintln!("{test}: the queue has no timestamp support");
        }
        assert!(s.pipelines.warm);
        assert_eq!(s.pipelines.created, at_start, "every offscreen variant was built at start-up");
        assert_eq!(s.pipelines.created_after_warm, 0, "no pipeline created mid-game");
        assert!(s.memory.bytes(MemoryKind::Meshes) >= 2 * (24 * 24 + 36 * 4) as u64);
        assert!(s.memory.bytes(MemoryKind::RenderTargets) >= (W * H * 8) as u64);
        assert!(s.memory.bytes(MemoryKind::PerFrame) >= s.draw_buffer_bytes);
        assert_eq!(s.memory.bytes(MemoryKind::Staging), 0, "staging is freed after uploads");
        assert_eq!(s.memory.total_bytes, s.memory.by_kind.iter().map(|(_, b)| b).sum::<u64>());
        assert_eq!(s.memory.allocations, 7, "2 meshes, the shared sprite quad, colour, depth, readback, frame upload");
        assert!(!s.memory.heaps.is_empty() && s.memory.heaps.iter().any(|h| h.device_local));
        for h in &s.memory.heaps {
            // The spec bounds the budget by the heap size; usage may exceed the budget.
            if let (Some(budget), Some(_)) = (h.budget, h.usage) {
                assert!(budget > 0 && budget <= h.size, "{h:?}");
            }
        }
    }
    assert_eq!(gpu.last_frame_stats_of(FramePath::Window), None);
    assert_eq!(gpu.last_frame_stats_of(FramePath::Offscreen).unwrap().frame, 2);

    // 10,000 cubes in one call: the record buffer grows to fit.
    let many: Vec<DrawItem> = (0..10_000)
        .map(|i| {
            let p = [(i % 100) as f32 * 0.1 - 5.0, (i / 100) as f32 * 0.1 - 5.0, -5.0];
            DrawItem::new(mesh_a, scale_translation(0.08, p), [0.85, 0.55, 0.25, 1.0])
        })
        .collect();
    let view_proj = mul(&perspective_vk_reverse_z(90f32.to_radians(), 1.0, 0.05), &view);
    gpu.render(target, &FrameDesc { view_proj, draws: &many, reverse_z: true, ..Default::default() }).unwrap();
    let s = gpu.last_frame_stats();
    assert_eq!((s.draws.draw_calls, s.draws.instances, s.draws.culled), (1, 10_000, 0));
    assert!(s.draw_buffer_bytes >= 768 + 10_000 * 112, "{}", s.draw_buffer_bytes);
    let rgba = gpu.read_rgba8(target).unwrap();
    let (x, y) = project(&view_proj, [0.0, 0.0, -5.0], W, H);
    let got = pixel(&rgba, W, x as u32, y as u32);
    let ndl = dot([0.0, 0.0, 1.0], light_dir());
    let want = [expected_lit(0.85, ndl), expected_lit(0.55, ndl), expected_lit(0.25, ndl), 255];
    for k in 0..4 {
        assert!(got[k].abs_diff(want[k]) <= 2, "{test}: 10k grid centre got {got:?}, want {want:?}");
    }

    gpu.destroy_target(target);
    gpu.destroy_mesh(mesh_a);
    gpu.destroy_mesh(mesh_b);
    let errors = validation_errors(&test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}

#[test]
fn instanced_grid_on_every_adapter() {
    if !specula_gpu::is_available() {
        eprintln!("instanced_grid_on_every_adapter: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().expect("enumerate_adapters") {
        instanced_grid_on(AdapterPreference::Index(a.index), &format!("{}", a.index));
    }
}

/// A one-off spike grows the record buffer; a long run of small frames shrinks it again.
#[test]
fn upload_buffer_shrinks_after_a_spike() {
    let test = "upload_buffer_shrinks_after_a_spike";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    let (vertices, indices) = cube();
    let mesh = gpu.upload_mesh(&vertices, &indices).unwrap();
    let target = gpu.create_target(16, 16).unwrap();
    let view_proj = mul(&perspective_vk_reverse_z(90f32.to_radians(), 1.0, 0.05), &look_at_rh([0.0, 0.0, 5.0], [0.0; 3], [0.0, 1.0, 0.0]));
    let spike: Vec<DrawItem> = (0..5000).map(|i| DrawItem::new(mesh, scale_translation(0.01, [(i % 70) as f32 * 0.01, 0.0, 0.0]), [1.0; 4])).collect();
    let frame = |draws| FrameDesc { view_proj, draws, reverse_z: true, ..Default::default() };
    gpu.render(target, &frame(&spike)).unwrap();
    let big = gpu.last_frame_stats();
    assert_eq!(big.draws.instances, 5000);
    assert!(big.draw_buffer_bytes >= 768 + 5000 * 112, "{}", big.draw_buffer_bytes);

    let one = [DrawItem::new(mesh, IDENTITY, [1.0; 4])];
    for _ in 0..299 {
        gpu.render(target, &frame(&one)).unwrap();
    }
    assert_eq!(gpu.last_frame_stats().draw_buffer_bytes, big.draw_buffer_bytes, "no shrink before the quiet spell ends");
    gpu.render(target, &frame(&one)).unwrap();
    let small = gpu.last_frame_stats();
    assert!(small.draw_buffer_bytes < 1280 + 1000 * 112, "shrunk to {}", small.draw_buffer_bytes);
    assert!(small.memory.bytes(MemoryKind::PerFrame) < big.memory.bytes(MemoryKind::PerFrame));
    let rgba = gpu.read_rgba8(target).unwrap();
    assert_ne!(pixel(&rgba, 16, 8, 8), [0, 0, 0, 255], "the cube still renders after the shrink");

    gpu.destroy_target(target);
    gpu.destroy_mesh(mesh);
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}

#[test]
fn stats_before_any_frame() {
    let Some(gpu) = gpu_or_skip("stats_before_any_frame", AdapterPreference::HighPerformance) else { return };
    let s = gpu.last_frame_stats();
    assert_eq!(s.frame, 0);
    assert_eq!(s.gpu, None);
    assert!(s.pipelines.created >= 4, "built at start-up: {}", s.pipelines.created);
    assert!(!s.pipelines.warm);
    assert_eq!(s.pipelines.cache, PipelineCacheLoad::NotProvided);
    assert_eq!(gpu.last_frame_stats_of(FramePath::Offscreen), None);
}

fn gpu_with_cache(blob: Option<Vec<u8>>) -> Gpu {
    let options = |validation| GpuOptions { validation, pipeline_cache: blob.clone(), ..Default::default() };
    Gpu::new(&options(Validation::On)).or_else(|_| Gpu::new(&options(Validation::Off))).expect("Gpu::new")
}

#[test]
fn pipeline_cache_round_trip() {
    let test = "pipeline_cache_round_trip";
    let Some(gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    let blob = gpu.pipeline_cache_data().unwrap();
    let at_start = gpu.last_frame_stats().pipelines.created;
    eprintln!("{test}: {} bytes of pipeline cache from {}", blob.len(), gpu.adapter().name);
    assert!(blob.len() >= 32, "a header at least");
    assert!(validation_errors(test, gpu).is_empty());

    let warm = gpu_with_cache(Some(blob.clone()));
    assert_eq!(warm.last_frame_stats().pipelines.cache, PipelineCacheLoad::Loaded { bytes: blob.len() });
    assert!(validation_errors(test, warm).is_empty());

    let cases = [
        (b"not a cache".to_vec(), CacheRejection::TooShort { len: 11 }),
        ({
            let mut b = blob.clone();
            b[16] ^= 0xff; // pipelineCacheUUID
            b
        }, CacheRejection::OtherDriver),
        ({
            let mut b = blob.clone();
            b[8] ^= 0x01; // vendorID
            b
        }, CacheRejection::OtherVendor { vendor_id: u32::from_le_bytes([blob[8] ^ 1, blob[9], blob[10], blob[11]]) }),
    ];
    for (bad, why) in cases {
        let gpu = gpu_with_cache(Some(bad));
        let s = gpu.last_frame_stats();
        assert_eq!(s.pipelines.cache, PipelineCacheLoad::Rejected(why));
        assert_eq!(s.pipelines.created, at_start, "still builds everything, from scratch");
        assert!(validation_errors(test, gpu).is_empty());
    }
}
