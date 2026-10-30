//! Baked GI on real GPUs (`gi.rs`, `shaders/gi.glsl`): the probes replace the flat ambient inside
//! their volume, fade into it outside, cells' masks hide probes, the irradiance view shows the
//! ambient term alone, and an unchanged key never rebuilds the buffer. Skips cleanly without
//! Vulkan. Captures go to `$SPECULA_TEST_OUT` (or cargo's per-target temp dir) as `gi_*.png`.

mod common;

use common::*;
use specula_gpu::{f16_bits, AdapterPreference, DrawItem, Environment, FrameDesc, GiFrame, GiQuality, GiView, GiVolume, Gpu, TargetId, Vertex};

const W: u32 = 160;
const H: u32 = 120;

fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let n = [0.0, 1.0, 0.0];
    let v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
    (v.to_vec(), vec![0, 1, 2, 0, 2, 3])
}

fn h2(a: f32, b: f32) -> u32 {
    u32::from(f16_bits(a)) | (u32::from(f16_bits(b)) << 16)
}

/// A probe record (`specula_lux::Record`) with uniform irradiance `c0`, cell mask `mask` and no surface in reach.
fn probe(c0: [f32; 3], mask: u64) -> [u32; 12] {
    [h2(c0[0], c0[1]), h2(c0[2], 0.0), 0, 0, 0, 0, h2(0.0, 0.0), h2(0.0, 1.0), mask as u32, (mask >> 32) as u32, u32::MAX, 0x00ff_ffff]
}

/// A 3 x 2 x 3 lattice 4 m apart over x, z in [-4, 4].
fn volume(probes: &[[u32; 12]]) -> GiVolume<'_> {
    GiVolume { origin: [-4.0, -2.0, -4.0], spacing: 4.0, dims: [3, 2, 3], intensity: 1.0, probes }
}

/// sRGB byte of linear `c` (no tone mapping, exposure 1).
fn byte(c: f32) -> i32 {
    (linear_to_srgb(c) * 255.0).round() as i32
}

struct Rig {
    target: TargetId,
    view_proj: [f32; 16],
    draws: Vec<DrawItem>,
    environment: Environment,
}

impl Rig {
    fn new(gpu: &mut Gpu) -> Rig {
        let (v, i) = quad();
        let ground = gpu.upload_mesh(&v, &i).unwrap();
        let target = gpu.create_target(W, H).unwrap();
        let view = look_at_rh([0.0, 30.0, 0.01], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(70f32.to_radians(), W as f32 / H as f32, 0.05), &view);
        let mut g = DrawItem::new(ground, scale_translation(80.0, [0.0, 0.0, 0.0]), [1.0, 1.0, 1.0, 1.0]);
        g.surface.specular = 0.0;
        // No sun; a red flat ambient (linear 0.5, 0, 0).
        let environment = Environment { sun_intensity: 0.0, ambient_color: [1.0, 0.0, 0.0], ambient_intensity: 0.5, ..Default::default() };
        Rig { target, view_proj, draws: vec![g], environment }
    }

    fn render(&self, gpu: &mut Gpu, gi: GiFrame, label: &str) -> Vec<u8> {
        let frame = FrameDesc { view_proj: self.view_proj, draws: &self.draws, reverse_z: true, environment: self.environment, gi, ..Default::default() };
        gpu.render(self.target, &frame).unwrap();
        let rgba = gpu.read_rgba8(self.target).unwrap();
        write_png(&format!("gi_{label}.png"), &rgba, W, H);
        rgba
    }

    fn at(&self, rgba: &[u8], p: V3) -> [u8; 4] {
        let (x, y) = project(&self.view_proj, p, W, H);
        pixel(rgba, W, x as u32, y as u32)
    }
}

#[test]
fn probes_replace_the_flat_ambient_inside_their_volume() {
    let test = "probes_replace_the_flat_ambient_inside_their_volume";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    {
        let mut rig = Rig::new(&mut gpu);
        // Without GI: red everywhere.
        let flat = rig.render(&mut gpu, GiFrame::default(), "flat");
        assert_eq!(rig.at(&flat, [0.0; 3])[..3], [byte(0.5) as u8, 0, 0]);
        // With the volume (uniform green, linear 0.5): green inside, red well outside, a blend
        // over the spacing between.
        let green = vec![probe([0.0, 0.5, 0.0], u64::MAX); 18];
        let v = [volume(&green)];
        let gi = GiFrame { key: 1, volumes: &v, quality: GiQuality::High, view: GiView::Lit };
        let lit = rig.render(&mut gpu, gi, "lit");
        let c = rig.at(&lit, [0.0, 0.0, 0.0]);
        assert!((c[1] as i32 - byte(0.5)).abs() <= 2 && c[0] <= 2, "inside: {c:?}");
        let c = rig.at(&lit, [14.0, 0.0, 0.0]);
        assert!((c[0] as i32 - byte(0.5)).abs() <= 2 && c[1] <= 2, "outside: {c:?}");
        let c = rig.at(&lit, [6.0, 0.0, 0.0]);
        assert!(c[0] > 40 && c[1] > 40, "half a spacing out: both: {c:?}");
        // The same key: the buffer is reused.
        let (bytes, rebuilds) = gpu.gi_stats();
        assert_eq!(bytes, 16 + 48 + 18 * 64, "the header, a volume, each probe's record and its lean copy");
        rig.render(&mut gpu, gi, "lit_again");
        assert_eq!(gpu.gi_stats().1, rebuilds, "an unchanged key never rebuilds");
        // Masks hiding every probe make the cell dim, never black (a twenty-fourth of the valid
        // probes' mean, GI_HIDDEN_FLOOR), and not the flat ambient.
        let hidden = vec![probe([0.0, 0.5, 0.0], 0); 18];
        let v = [volume(&hidden)];
        let dark = rig.render(&mut gpu, GiFrame { key: 2, volumes: &v, quality: GiQuality::High, view: GiView::Lit }, "masked");
        let c = rig.at(&dark, [0.0; 3]);
        assert!(c[0] <= 2 && (c[1] as i32 - byte(0.5 / 24.0)).abs() <= 3, "hidden from every probe: {c:?}");
        // Tier low: a small tile gets an ambient cube the CPU samples half a spacing out from its
        // faces (hidden from every probe there means inside a solid prop, so it takes the valid
        // probes unmasked: green; its top face's point sits 2 cm past the volume's top, so a
        // trace of the flat red fades in), the 80 m ground reads the lean probes per pixel with
        // the masks (dark, as at high).
        let (v2, i2) = quad();
        let tile_mesh = gpu.upload_mesh(&v2, &i2).unwrap();
        let mut tile = DrawItem::new(tile_mesh, scale_translation(2.0, [0.0, 0.05, 0.0]), [1.0, 1.0, 1.0, 1.0]);
        tile.surface.specular = 0.0;
        rig.draws.push(tile);
        for (quality, label) in [(GiQuality::High, "high_tile"), (GiQuality::Low, "low")] {
            let img = rig.render(&mut gpu, GiFrame { key: 2, volumes: &v, quality, view: GiView::Lit }, label);
            let c = rig.at(&img, [0.0, 0.05, 0.0]);
            if quality == GiQuality::Low {
                assert!((c[1] as i32 - byte(0.5)).abs() <= 3 && c[0] <= 24, "{label}: the tile, per draw: {c:?}");
            }
            let c = rig.at(&img, [1.5, 0.0, 1.5]);
            assert!(c[0] <= 2 && (c[1] as i32 - byte(0.5 / 24.0)).abs() <= 3, "{label}: the ground, per pixel (dim): {c:?}");
            // Far out, outside the volume: the flat ambient (red).
            let c = rig.at(&img, [16.0, 0.0, 0.0]);
            assert!((c[0] as i32 - byte(0.5)).abs() <= 3 && c[1] <= 3, "{label}: far out: {c:?}");
        }
        rig.draws.pop();
        assert_eq!(gpu.gi_stats().1, rebuilds + 1, "quality isn't part of the buffer");
        // The irradiance view: the ambient term alone, whatever the albedo.
        let vg = [volume(&green)];
        let iv = rig.render(&mut gpu, GiFrame { key: 1, volumes: &vg, quality: GiQuality::High, view: GiView::Irradiance }, "irradiance");
        // ...stretched over its own range (every probe alike here: 0.5 green, luminance 0.36,
        // shown at the top of the view's range, 0.45), so a pale stage shows gradation.
        let (scale, start) = gpu.gi_view_stretch().expect("an irradiance view");
        assert!((0.5 * 0.7152 * scale - 0.42).abs() < 0.01 && start == 0.0, "the view's stretch {scale} {start}");
        let c = rig.at(&iv, [0.0; 3]);
        assert!((c[1] as i32 - byte(0.45 / 0.7152)).abs() <= 3 && c[0] <= 2, "irradiance view {c:?}");
        // Particles draw in the lit view, never in the irradiance view (their glow isn't GI).
        let glow = [specula_gpu::Particle { position: [0.0, 1.0, 0.0], size: 3.0, color: [4.0, 0.0, 0.0, 1.0], additive: 1.0, ..Default::default() }];
        let materials = [specula_gpu::ParticleMaterial::default()];
        let fx = specula_gpu::Effects { particles: &glow, materials: &materials, decals: &[] };
        let mut shots = Vec::new();
        for view in [GiView::Lit, GiView::Irradiance] {
            let gi = GiFrame { key: 1, volumes: &vg, quality: GiQuality::High, view };
            let frame = FrameDesc { view_proj: rig.view_proj, draws: &rig.draws, reverse_z: true, environment: rig.environment, gi, effects: fx, ..Default::default() };
            gpu.render(rig.target, &frame).unwrap();
            shots.push(rig.at(&gpu.read_rgba8(rig.target).unwrap(), [0.0, 1.0, 0.0]));
        }
        assert!(shots[0][0] > 200, "the particle glows in the lit view: {:?}", shots[0]);
        assert!(shots[1][0] <= 2, "no particle in the irradiance view: {:?}", shots[1]);
        // A frame without volumes keeps the buffer while it names the key, and an offscreen frame
        // naming none (a studio render, a preview, a filmstrip) leaves the window's buffer alone:
        // no rebuild when GI comes back (only a window frame without GI frees it).
        rig.render(&mut gpu, GiFrame { key: 2, ..GiFrame::default() }, "off_keep");
        assert!(gpu.gi_stats().0 > 0);
        let rebuilds = gpu.gi_stats().1;
        for _ in 0..3 {
            rig.render(&mut gpu, GiFrame::default(), "none");
            assert!(gpu.gi_stats().0 > 0);
            rig.render(&mut gpu, GiFrame { key: 1, volumes: &vg, quality: GiQuality::High, view: GiView::Lit }, "back");
        }
        assert_eq!(gpu.gi_stats().1, rebuilds, "no rebuild for the renders without GI");
        gpu.flush_uploads().unwrap();
        gpu.destroy_target(rig.target);
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation errors", errors.len());
}

#[test]
fn bad_volumes_fall_back_to_the_flat_ambient() {
    // GI never fails a frame (the window loop would stop): a set the GPU can't take draws the flat
    // ambient, and `gi_failure` says why, until a good set replaces it.
    let test = "bad_volumes_fall_back_to_the_flat_ambient";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    {
        let rig = Rig::new(&mut gpu);
        let probes = vec![[0u32; 12]; 7];
        let v = [GiVolume { origin: [0.0; 3], spacing: 1.0, dims: [2, 2, 2], intensity: 1.0, probes: &probes }];
        let flat = rig.render(&mut gpu, GiFrame { key: 9, volumes: &v, ..Default::default() }, "bad_short");
        assert!(gpu.gi_failure().is_some_and(|e| e.contains("GI volume 0")), "{:?}", gpu.gi_failure());
        assert!(!gpu.gi_drawn(), "a set that failed to bind is not drawn (sight packets say so)");
        assert_eq!(rig.at(&flat, [0.0; 3])[..3], [byte(0.5) as u8, 0, 0]);
        // More probes than the GPU side holds (MAX_GI_PROBES).
        let many = vec![probe([0.0, 0.5, 0.0], u64::MAX); 512 * 2 * 513];
        let big = [GiVolume { origin: [-4.0, -2.0, -4.0], spacing: 1.0, dims: [512, 2, 513], intensity: 1.0, probes: &many }];
        let flat = rig.render(&mut gpu, GiFrame { key: 10, volumes: &big, ..Default::default() }, "bad_many");
        assert!(gpu.gi_failure().is_some_and(|e| e.contains("probe")), "{:?}", gpu.gi_failure());
        assert_eq!(rig.at(&flat, [0.0; 3])[..3], [byte(0.5) as u8, 0, 0]);
        assert_eq!(gpu.gi_stats().0, 0);
        // A good set clears it.
        let green = vec![probe([0.0, 0.5, 0.0], u64::MAX); 18];
        let good = [volume(&green)];
        let lit = rig.render(&mut gpu, GiFrame { key: 11, volumes: &good, ..Default::default() }, "good_again");
        assert!(gpu.gi_failure().is_none() && gpu.gi_drawn());
        assert!((rig.at(&lit, [0.0; 3])[1] as i32 - byte(0.5)).abs() <= 2);
        gpu.flush_uploads().unwrap();
        gpu.destroy_target(rig.target);
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation errors", errors.len());
}

#[test]
fn a_capture_takes_the_gi_pipelines_a_background_build_is_making() {
    // From the round-4 review: an offscreen render while a worker builds the same GI set creates
    // none of its pipelines itself (`build_variants` waits for the worker first).
    let test = "a_capture_takes_the_gi_pipelines_a_background_build_is_making";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    {
        let rig = Rig::new(&mut gpu);
        rig.render(&mut gpu, GiFrame::default(), "before_gi");
        let created = gpu.last_frame_stats().pipelines.created;
        gpu.request_gi_variants(GiQuality::Low);
        let green = vec![probe([0.0, 0.5, 0.0], u64::MAX); 18];
        let v = [volume(&green)];
        rig.render(&mut gpu, GiFrame { key: 21, volumes: &v, quality: GiQuality::Low, view: GiView::Lit }, "while_building");
        let stats = gpu.last_frame_stats().pipelines;
        assert_eq!(stats.created, created, "the capture created GI pipelines beside the worker's ({} built there)", stats.created_background);
        assert!(gpu.pipeline_variants().gi, "the set is ready after the capture");
        gpu.flush_uploads().unwrap();
        gpu.destroy_target(rig.target);
    }
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation errors", errors.len());
}
