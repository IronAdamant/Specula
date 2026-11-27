//! Asynchronous offscreen renders (`Gpu::render_async`, `settle_offscreen`, the capture flight's
//! GPU side) on real GPUs, under validation: the same pixels and ID counts as a synchronous
//! render; meshes, textures and targets destroyed while one is in flight (the stalls review's
//! use-after-free, `VUID-vkDestroyBuffer-buffer-00922` then a lost device); a lost device while
//! one is in flight. Every test skips cleanly (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{
    generate_mips, AdapterPreference, DrawItem, Environment, FrameDesc, Gpu, MaterialTextures, MeshStreams, Surface, TargetId,
    TextureFilter, TextureFormat, TextureSampler,
};
use std::time::{Duration, Instant};

const W: u32 = 160;
const H: u32 = 120;

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap_or_default() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

fn frame(draws: &[DrawItem]) -> FrameDesc<'_> {
    let view = look_at_rh([0.0, 1.0, 6.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(45f32.to_radians(), W as f32 / H as f32, 0.05), &view);
    let environment = Environment { ambient_intensity: 1.0, sun_intensity: 0.6, ..Default::default() };
    FrameDesc { view_proj, clear_color: [0.05, 0.05, 0.08, 1.0], draws, reverse_z: true, environment, ..Default::default() }
}

/// A quad with UVs facing +Z.
fn quad() -> (Vec<specula_gpu::Vertex>, Vec<u32>, Vec<[f32; 2]>) {
    let n = [0.0, 0.0, 1.0];
    let v = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .map(|[x, y]| specula_gpu::Vertex { position: [x, y, 0.0], normal: n, color: specula_gpu::Vertex::WHITE });
    (v.to_vec(), vec![0, 1, 2, 0, 2, 3], vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
}

fn checker() -> Vec<u8> {
    (0..16 * 16).flat_map(|i| if (i % 16 / 4 + i / 64) % 2 == 0 { [230, 40, 40, 255] } else { [40, 40, 230, 255] }).collect()
}

/// One id per draw of [`Scene::draws`], no occluders.
fn ids() -> Option<(&'static [u32], &'static [[u32; 4]])> {
    Some((&[1, 2], &[]))
}

/// Polls until the render in flight has settled (never a blocking wait).
fn settle_by_polling(gpu: &mut Gpu, test: &str) -> u32 {
    let start = Instant::now();
    let mut polls = 0;
    while !gpu.settle_offscreen(false).expect("settle") {
        polls += 1;
        assert!(start.elapsed() < specula_core::HANG_BOUND, "{test}: the render has not settled in {:?}: hung?", specula_core::HANG_BOUND);
        std::thread::sleep(Duration::from_micros(200));
    }
    polls
}

/// A cube and a textured quad.
struct Scene {
    cube: specula_gpu::MeshId,
    quad: specula_gpu::MeshId,
    tex: specula_gpu::TextureId,
}

impl Scene {
    fn new(gpu: &mut Gpu) -> Scene {
        let (cv, ci) = cube();
        let (qv, qi, uv) = quad();
        let px = checker();
        let nearest = TextureSampler { mag: TextureFilter::Nearest, min: TextureFilter::Nearest, mipmap: TextureFilter::Nearest, ..Default::default() };
        let levels = generate_mips(16, 16, TextureFormat::Rgba8Srgb, &px).unwrap();
        let refs: Vec<&[u8]> = levels.iter().map(Vec::as_slice).collect();
        let cube = gpu.upload_mesh(&cv, &ci).unwrap();
        let quad = gpu.upload_mesh_with(&qv, &qi, &MeshStreams { uvs: Some(&uv), ..Default::default() }).unwrap();
        let tex = gpu.upload_texture_levels(16, 16, TextureFormat::Rgba8Srgb, &refs, nearest).unwrap();
        wait_variants(gpu, false, true);
        Scene { cube, quad, tex }
    }

    fn draws(&self) -> Vec<DrawItem> {
        let mut q = DrawItem::new(self.quad, translation([1.2, 0.0, 0.0]), [1.0; 4]);
        q.surface = Surface { textures: MaterialTextures { base_color: Some(self.tex), ..Default::default() }, ..Default::default() };
        vec![DrawItem::new(self.cube, translation([-1.2, 0.0, 0.0]), [0.9, 0.7, 0.2, 1.0]), q]
    }
}

/// Pixels and per-id coverage of one synchronous render with ids.
fn sync_render(gpu: &mut Gpu, target: TargetId, draws: &[DrawItem]) -> (Vec<u8>, Vec<specula_gpu::IdCoverage>) {
    gpu.render_with_ids(target, &frame(draws), &[1, 2]).unwrap();
    let counts = gpu.id_coverage(target, 2).unwrap().per_id;
    (gpu.read_rgba8(target).unwrap(), counts)
}

/// An asynchronous render (polled, never waited for) gives exactly a synchronous render's
/// pixels and ID counts; while it is in flight its ID counts refuse (settle first) and a readback
/// settles it; a render without ids too; a timed settle ends by its limit.
#[test]
fn async_renders_match_synchronous_ones() {
    every_adapter("async_renders_match_synchronous_ones", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let target = gpu.create_target(W, H).unwrap();
        let scene = Scene::new(&mut gpu);
        let draws = scene.draws();
        let (pixels, counts) = sync_render(&mut gpu, target, &draws);
        assert!(counts.iter().all(|c| c.pixels > 0), "{test}: both draws are seen: {counts:?}");

        gpu.render_async(target, &frame(&draws), ids()).unwrap();
        assert!(gpu.offscreen_pending(), "{test}: in flight");
        assert!(gpu.id_coverage(target, 2).is_err(), "{test}: counts refuse while it is in flight");
        let polls = settle_by_polling(&mut gpu, test);
        assert!(!gpu.offscreen_pending());
        assert_eq!(gpu.id_coverage(target, 2).unwrap().per_id, counts, "{test}: the same ID counts");
        assert_eq!(gpu.read_rgba8(target).unwrap(), pixels, "{test}: the same pixels (after {polls} polls)");
        write_png(&format!("async_render_{}", test.replace(['[', ']'], "_")), &pixels, W, H);

        // A readback settles the render in flight itself.
        gpu.render_async(target, &frame(&draws), None).unwrap();
        assert_eq!(gpu.read_rgba8(target).unwrap(), pixels, "{test}: without ids too");
        assert!(!gpu.offscreen_pending());

        // A timed settle: within its limit, or not yet (never past it by much).
        gpu.render_async(target, &frame(&draws), ids()).unwrap();
        let t = Instant::now();
        let done = gpu.settle_offscreen_within(Some(Duration::from_secs(5))).unwrap();
        assert!(done && t.elapsed() < Duration::from_secs(5), "{test}: a timed settle ends when the render does");
        assert!(gpu.settle_offscreen_within(Some(Duration::ZERO)).unwrap(), "{test}: nothing left in flight");
        assert_eq!(gpu.read_rgba8(target).unwrap(), pixels);
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    });
}

/// Meshes, textures and the target destroyed while an asynchronous render that draws them is in
/// flight: freed only after the GPU is done with them (queue order), with no validation message,
/// and the render still lands with the pixels it had.
#[test]
fn destroying_what_a_render_in_flight_draws_is_safe() {
    every_adapter("destroying_what_a_render_in_flight_draws_is_safe", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let target = gpu.create_target(W, H).unwrap();
        let keep = Scene::new(&mut gpu);
        let (pixels, _) = sync_render(&mut gpu, target, &keep.draws());
        for round in 0..4 {
            let doomed = Scene::new(&mut gpu);
            gpu.render_async(target, &frame(&doomed.draws()), ids()).unwrap();
            // A world_delete during a capture: the engine destroys the entity's meshes at once.
            gpu.destroy_mesh(doomed.cube);
            gpu.destroy_mesh(doomed.quad);
            gpu.destroy_texture(doomed.tex);
            assert!(!gpu.has_texture(doomed.tex), "{test}: its id is gone at once");
            settle_by_polling(&mut gpu, test);
            assert_eq!(gpu.read_rgba8(target).unwrap(), pixels, "{test} round {round}: the render drew them as they were");
        }
        // The retired memory is freed by later batches.
        let start = Instant::now();
        while gpu.upload_stats().retired_pending > 0 {
            gpu.flush_uploads().unwrap();
            sync_render(&mut gpu, target, &keep.draws());
            assert!(start.elapsed() < specula_core::HANG_BOUND, "{test}: retired memory not freed in {:?}: {:?}", specula_core::HANG_BOUND, gpu.upload_stats());
        }
        // The target itself, destroyed in flight.
        let other = gpu.create_target(W, H).unwrap();
        gpu.render_async(other, &frame(&keep.draws()), ids()).unwrap();
        gpu.destroy_target(other);
        assert!(!gpu.offscreen_pending(), "{test}: settled before its target went");
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    });
}

/// A render that ends in a lost device settles with that error (a capture answers
/// `gpu_device_lost`), and the next render is not left waiting on it.
#[test]
fn a_lost_device_in_flight_settles_with_its_error() {
    every_adapter("a_lost_device_in_flight_settles_with_its_error", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let target = gpu.create_target(W, H).unwrap();
        let scene = Scene::new(&mut gpu);
        let (pixels, counts) = sync_render(&mut gpu, target, &scene.draws());
        gpu.inject_offscreen_fault();
        gpu.render_async(target, &frame(&scene.draws()), ids()).unwrap();
        let start = Instant::now();
        let err = loop {
            match gpu.settle_offscreen(false) {
                Ok(false) => {
                    assert!(start.elapsed() < specula_core::HANG_BOUND, "{test}: the lost render has not settled in {:?}: hung?", specula_core::HANG_BOUND);
                    std::thread::sleep(Duration::from_micros(200));
                }
                Ok(true) => panic!("{test}: the injected loss was not reported"),
                Err(e) => break e,
            }
        };
        assert!(err.device_lost(), "{test}: {err:?}");
        assert!(!gpu.offscreen_pending(), "{test}: nothing is left in flight");
        assert_eq!(sync_render(&mut gpu, target, &scene.draws()), (pixels, counts), "{test}: later renders work");
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    });
}
