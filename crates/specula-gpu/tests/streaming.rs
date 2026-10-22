//! Queued uploads and deferred frees (`Gpu::queue_mesh`, `queue_texture_levels`, `retire_*`) on
//! real GPUs, under validation (synchronization validation included). Every test skips cleanly
//! (prints why, passes) without Vulkan.

mod common;

use common::*;
use specula_gpu::{
    generate_mips, AdapterPreference, DrawItem, Environment, FrameDesc, Gpu, MaterialTextures, MeshStreams, Surface, TextureFilter,
    TextureFormat, TextureSampler, UPLOAD_ARENA_BYTES,
};

const W: u32 = 128;
const H: u32 = 128;

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
    let view = look_at_rh([0.0, 0.0, 6.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let view_proj = mul(&perspective_vk_reverse_z(45f32.to_radians(), 1.0, 0.05), &view);
    let environment = Environment { ambient_intensity: 1.0, sun_intensity: 0.6, ..Default::default() };
    FrameDesc { view_proj, clear_color: [0.0, 0.0, 0.0, 1.0], draws, reverse_z: true, environment, ..Default::default() }
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

/// Renders `draws` until every retired resource is freed. Frees are bounded per frame (a time
/// budget, so a loaded machine frees fewer a frame) and wait for the GPU: how many frames that
/// takes isn't fixed, only that it ends.
fn drain(gpu: &mut Gpu, target: specula_gpu::TargetId, draws: &[DrawItem]) -> u32 {
    for frames in 1..=500 {
        render(gpu, target, draws);
        if gpu.upload_stats().retired_pending == 0 {
            return frames;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("retired resources never freed: {:?}", gpu.upload_stats());
}

fn render(gpu: &mut Gpu, target: specula_gpu::TargetId, draws: &[DrawItem]) -> Vec<u8> {
    gpu.render(target, &frame(draws)).unwrap();
    gpu.read_rgba8(target).unwrap()
}

/// A queued mesh and texture draw exactly as uploaded ones, in the very next frame; retired ones
/// are freed after a later batch, never while a frame could read them.
#[test]
fn queued_uploads_draw_like_synchronous_ones() {
    every_adapter("queued_uploads_draw_like_synchronous_ones", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let target = gpu.create_target(W, H).unwrap();
        let (cv, ci) = cube();
        let (qv, qi, uv) = quad();
        let px = checker();
        let nearest = TextureSampler { mag: TextureFilter::Nearest, min: TextureFilter::Nearest, mipmap: TextureFilter::Nearest, ..Default::default() };
        let levels = generate_mips(16, 16, TextureFormat::Rgba8Srgb, &px).unwrap();
        let refs: Vec<&[u8]> = levels.iter().map(Vec::as_slice).collect();
        let streams = MeshStreams { uvs: Some(&uv), ..Default::default() };

        let sync_cube = gpu.upload_mesh(&cv, &ci).unwrap();
        let sync_quad = gpu.upload_mesh_with(&qv, &qi, &streams).unwrap();
        let sync_tex = gpu.upload_texture_levels(16, 16, TextureFormat::Rgba8Srgb, &refs, nearest).unwrap();
        wait_variants(&mut gpu, false, true);
        let textured = |mesh, tex| {
            let mut d = DrawItem::new(mesh, translation([1.2, 0.0, 0.0]), [1.0; 4]);
            d.surface = Surface { textures: MaterialTextures { base_color: Some(tex), ..Default::default() }, ..Default::default() };
            d
        };
        let scene = |cube, quad, tex| vec![DrawItem::new(cube, translation([-1.2, 0.0, 0.0]), [0.9, 0.7, 0.2, 1.0]), textured(quad, tex)];
        let expected = render(&mut gpu, target, &scene(sync_cube, sync_quad, sync_tex));

        // Queued, then drawn by the very next render (the batch goes first on the queue).
        let q_cube = gpu.queue_mesh(&cv, &ci, &MeshStreams::default()).unwrap();
        let q_quad = gpu.queue_mesh(&qv, &qi, &streams).unwrap();
        let q_tex = gpu.queue_texture_levels(16, 16, TextureFormat::Rgba8Srgb, &refs, nearest).unwrap();
        let before = gpu.upload_stats();
        assert_eq!(before.items, 3, "{test}: {before:?}");
        let got = render(&mut gpu, target, &scene(q_cube, q_quad, q_tex));
        assert_eq!(got, expected, "{test}: queued uploads render exactly like synchronous ones");
        assert!(gpu.upload_stats().batches >= 1);
        write_png(&format!("streaming_{}", test.replace(['[', ']'], "_")), &got, W, H);

        // Retire everything queued; the next frames free them (after a later batch finished).
        gpu.retire_mesh(q_cube);
        gpu.retire_mesh(q_quad);
        gpu.retire_texture(q_tex);
        assert!(!gpu.has_texture(q_tex), "{test}: a retired texture's id is gone at once");
        let still = render(&mut gpu, target, &scene(sync_cube, sync_quad, sync_tex));
        assert_eq!(still, expected);
        drain(&mut gpu, target, &scene(sync_cube, sync_quad, sync_tex));
        let s = gpu.upload_stats();
        assert_eq!(s.freed, 3, "{test}: retired resources are freed: {s:?}");
        assert_eq!(s.retired_pending, 0, "{test}: {s:?}");

        // Queued and retired before any frame: the copy still runs before the free.
        let gone = gpu.queue_mesh(&cv, &ci, &MeshStreams::default()).unwrap();
        gpu.retire_mesh(gone);
        // A synchronous destroy of a mesh whose copy is still queued settles the batch first.
        let doomed = gpu.queue_mesh(&cv, &ci, &MeshStreams::default()).unwrap();
        gpu.destroy_mesh(doomed);
        drain(&mut gpu, target, &scene(sync_cube, sync_quad, sync_tex));
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    });
}

/// More than an arena's worth in one batch (dedicated staging) and more batches than slots
/// without a frame (queuing waits for the oldest slot instead of overwriting it): every mesh
/// still draws.
#[test]
fn large_and_many_batches_stay_correct() {
    every_adapter("large_and_many_batches_stay_correct", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let target = gpu.create_target(W, H).unwrap();
        // A grid of tiny triangles: big enough to exceed the arena on its own.
        let side = 700u32;
        let mut v = Vec::new();
        let mut idx = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let (fx, fy) = (x as f32 / side as f32 * 2.0 - 1.0, y as f32 / side as f32 * 2.0 - 1.0);
                let base = v.len() as u32;
                let n = [0.0, 0.0, 1.0];
                let d = 2.0 / side as f32;
                for p in [[fx, fy], [fx + d, fy], [fx + d, fy + d], [fx, fy + d]] {
                    v.push(specula_gpu::Vertex { position: [p[0], p[1], 0.0], normal: n, color: specula_gpu::Vertex::WHITE });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        let bytes = (v.len() * 28 + idx.len() * 4) as u64;
        assert!(bytes > UPLOAD_ARENA_BYTES, "the test mesh ({bytes} B) must not fit an arena");
        // Mesh memory grown ahead: the next queued mesh lands in it without an allocation.
        let (_, total, next) = gpu.mesh_space();
        gpu.grow_mesh_space().unwrap();
        let (free, grown, _) = gpu.mesh_space();
        assert_eq!(grown, total + next, "{test}: one more block");
        assert!(free >= next, "{test}: the new block is empty");
        // It joins without anyone waiting for it (polled, as a streamer does).
        let t = std::time::Instant::now();
        while gpu.mesh_growing() && t.elapsed() < std::time::Duration::from_secs(10) {
            gpu.mesh_fits(1);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        eprintln!("{test}: a {next}-byte block grew on the worker in {:?}", t.elapsed());
        assert!(!gpu.mesh_growing() && gpu.mesh_fits(1), "{test}: the block joined");
        let (cv, ci) = cube();
        let small = gpu.queue_mesh(&cv, &ci, &MeshStreams::default()).unwrap();
        assert_eq!(gpu.mesh_space().1, grown, "{test}: no allocation for a mesh that fits");
        assert_eq!(gpu.upload_stats().mesh_blocks_grown, 1, "{test}: the block came from the worker");
        gpu.retire_mesh(small);
        let sync = gpu.upload_mesh(&v, &idx).unwrap();
        let expected = render(&mut gpu, target, &[DrawItem::new(sync, IDENTITY, [0.3, 0.8, 0.4, 1.0])]);
        let big = gpu.queue_mesh(&v, &idx, &MeshStreams::default()).unwrap();
        assert_eq!(gpu.upload_stats().dedicated_staging, 1);
        let got = render(&mut gpu, target, &[DrawItem::new(big, IDENTITY, [0.3, 0.8, 0.4, 1.0])]);
        assert_eq!(got, expected, "{test}: an oversized queued mesh draws like the uploaded one");
        // Six batches flushed back to back, no frame between: slots are reused only once done.
        let (cv, ci) = cube();
        let mut meshes = Vec::new();
        for _ in 0..6 {
            meshes.push(gpu.queue_mesh(&cv, &ci, &MeshStreams::default()).unwrap());
            gpu.flush_uploads().unwrap();
        }
        let draws: Vec<DrawItem> =
            meshes.iter().enumerate().map(|(i, m)| DrawItem::new(*m, scale_translation(0.3, [i as f32 * 0.6 - 1.5, 0.0, 0.0]), [1.0; 4])).collect();
        let image = render(&mut gpu, target, &draws);
        // Each cube's own columns (camera 6 m away, 45 degrees: 25.75 px per metre at x = 0).
        for i in 0..6 {
            let centre = 64.0 + (i as f32 * 0.6 - 1.5) * (64.0 / (6.0 * 22.5f32.to_radians().tan()));
            let cols = (centre as u32 - 2)..=(centre as u32 + 2);
            let lit = (0..H).flat_map(|y| cols.clone().map(move |x| (x, y))).filter(|&(x, y)| pixel(&image, W, x, y)[0] > 40).count();
            assert!(lit > 10, "{test}: cube {i} drew ({lit} px at x {centre:.0})");
        }
        let s = gpu.upload_stats();
        assert!(s.batches >= 7, "{test}: {s:?}");
        for m in meshes {
            gpu.retire_mesh(m);
        }
        gpu.retire_mesh(big);
        drain(&mut gpu, target, &[]);
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    });
}
