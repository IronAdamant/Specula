//! M2 rendering features on real GPUs: shadows, post, sky and fog, overlay. Every test skips
//! cleanly (prints why, passes) without Vulkan. Captures go to `$SPECULA_TEST_OUT` (or cargo's
//! per-target temp dir) as `features_*.png`, for people and agents to look at.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, Environment, FrameDesc, Gpu, Material, MeshId, Surface, Vertex};

const W: u32 = 480;
const H: u32 = 270;

/// Towards a low moon, about 30° up, from the left and slightly behind the camera.
fn moon() -> V3 {
    normalize([-0.75, 0.55, 0.35])
}

fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let n = [0.0, 1.0, 0.0];
    let v = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
    (v.to_vec(), vec![0, 1, 2, 0, 2, 3])
}

fn scale3(s: V3, t: V3) -> [f32; 16] {
    let mut m = IDENTITY;
    m[0] = s[0];
    m[5] = s[1];
    m[10] = s[2];
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m
}

struct Scene {
    cube: MeshId,
    ground: MeshId,
    view_proj: [f32; 16],
}

impl Scene {
    fn new(gpu: &mut Gpu) -> Scene {
        let (v, i) = cube();
        let cube = gpu.upload_mesh(&v, &i).unwrap();
        let (v, i) = quad();
        let ground = gpu.upload_mesh(&v, &i).unwrap();
        let view = look_at_rh([0.0, 9.0, 18.0], [0.0, 0.5, 0.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(50f32.to_radians(), W as f32 / H as f32, 0.05), &view);
        Scene { cube, ground, view_proj }
    }

    /// A ground plane, a 2 m cube in the middle, a row of smaller cubes, and one glowing cube.
    fn draws(&self) -> Vec<DrawItem> {
        let mut d = vec![
            DrawItem::new(self.ground, scale3([60.0, 1.0, 60.0], [0.0, 0.0, 0.0]), [0.36, 0.39, 0.44, 1.0]),
            DrawItem::new(self.cube, scale3([2.0, 2.0, 2.0], [0.0, 1.0, 0.0]), [0.55, 0.59, 0.64, 1.0]),
        ];
        for k in 0..5 {
            let x = -6.0 + 3.0 * k as f32;
            d.push(DrawItem::new(self.cube, scale3([0.8, 1.6, 0.8], [x, 0.8, -5.0]), [0.66, 0.62, 0.55, 1.0]));
        }
        let mut glow = DrawItem::new(self.cube, scale3([0.6, 0.6, 0.6], [4.0, 0.3, 3.0]), [0.25, 0.88, 0.82, 1.0]);
        glow.surface = Surface { emissive: 4.0, ..Default::default() };
        d.push(glow);
        d
    }

    fn environment(&self) -> Environment {
        Environment {
            sun_direction: moon(),
            sun_color: [0.79, 0.84, 1.0],
            sun_intensity: 1.0,
            ambient_color: [0.42, 0.47, 0.6],
            ambient_intensity: 0.35,
            exposure: 1.1,
            tonemap: true,
            ..Default::default()
        }
    }

    fn render(&self, gpu: &mut Gpu, env: Environment, name: &str) -> Vec<u8> {
        let target = gpu.create_target(W, H).unwrap();
        let draws = self.draws();
        let frame = FrameDesc {
            view_proj: self.view_proj,
            clear_color: [0.04, 0.06, 0.13, 1.0],
            draws: &draws,
            reverse_z: true,
            environment: env,
            ..Default::default()
        };
        gpu.render(target, &frame).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        gpu.destroy_target(target);
        let label: String = gpu.adapter().name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let path = write_png(&format!("features_{name}_{label}.png"), &rgba, W, H);
        eprintln!("wrote {}", path.display());
        rgba
    }

    fn px(&self, rgba: &[u8], p: V3) -> [u8; 4] {
        let (x, y) = project(&self.view_proj, p, W, H);
        pixel(rgba, W, x as u32, y as u32)
    }
}

fn luma(p: [u8; 4]) -> i32 {
    p[0] as i32 + p[1] as i32 + p[2] as i32
}

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

#[test]
fn shadows_fall_away_from_the_moon() {
    every_adapter("shadows", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let env = scene.environment();
        let plain = scene.render(&mut gpu, env, "no_shadows");
        let mut shadowed_env = env;
        shadowed_env.shadow.enabled = true;
        let shadowed = scene.render(&mut gpu, shadowed_env, "shadows");
        let stats = gpu.last_frame_stats();
        eprintln!("{test}: {:?}", stats.draws);
        assert!(stats.draws.shadow_cascades >= 1 && stats.draws.shadow_draw_calls >= 1);
        if let Some(t) = &stats.gpu {
            assert!(t.passes.iter().any(|p| p.name == specula_gpu::PASS_SHADOW), "{t:?}");
        }

        // Ground just beyond the big cube, away from the moon: in its shadow.
        let m = moon();
        let away = normalize([-m[0], 0.0, -m[2]]);
        let behind = [away[0] * 1.8, 0.01, away[2] * 1.8];
        let (a, b) = (scene.px(&plain, behind), scene.px(&shadowed, behind));
        eprintln!("{test}: ground behind the cube {a:?} -> {b:?}");
        assert!(luma(b) * 10 < luma(a) * 8, "shadowed ground is darker: {a:?} -> {b:?}");
        // On the moon's side of the cube the ground stays lit (the shadow falls away from the light).
        let front = [-away[0] * 2.5, 0.01, -away[2] * 2.5];
        let (a, b) = (scene.px(&plain, front), scene.px(&shadowed, front));
        assert!(luma(a).abs_diff(luma(b)) <= 6, "lit ground unchanged: {a:?} -> {b:?}");
        // No acne: open ground, the cube's top and its moon-facing side keep their light.
        for p in [[5.0, 0.01, 6.0], [0.3, 2.0, 0.3], [-1.0, 1.0, 0.2]] {
            let (a, b) = (scene.px(&plain, p), scene.px(&shadowed, p));
            assert!(luma(a).abs_diff(luma(b)) <= 6, "{p:?} stays lit: {a:?} -> {b:?}");
        }
        // No peter-panning: right at the cube's foot on the shadow side, it is already dark.
        let foot = [away[0] * 1.25, 0.01, away[2] * 1.25];
        let (a, b) = (scene.px(&plain, foot), scene.px(&shadowed, foot));
        assert!(luma(b) * 10 < luma(a) * 8, "shadow touches the cube's foot: {a:?} -> {b:?}");

        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
    });
}

#[test]
fn blended_materials_do_not_cast() {
    let test = "blended_do_not_cast";
    let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::HighPerformance) else { return };
    let scene = Scene::new(&mut gpu);
    let mut env = scene.environment();
    env.shadow.enabled = true;
    let target = gpu.create_target(64, 64).unwrap();
    let mut glass = DrawItem::new(scene.cube, scale3([2.0, 2.0, 2.0], [0.0, 1.0, 0.0]), [1.0, 1.0, 1.0, 0.3]);
    glass.material = Material::Glass;
    let ground = DrawItem::new(scene.ground, scale3([60.0, 1.0, 60.0], [0.0; 3]), [0.5; 4]);
    let draws = [ground, glass];
    let frame = FrameDesc { view_proj: scene.view_proj, draws: &draws, reverse_z: true, environment: env, ..Default::default() };
    gpu.render(target, &frame).unwrap();
    let s = gpu.last_frame_stats();
    assert_eq!(s.draws.shadow_instances, s.draws.shadow_cascades, "only the ground casts (once per cascade), not the glass: {:?}", s.draws);
    gpu.destroy_target(target);
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}

/// (pixels that differ, largest channel difference).
fn diff(a: &[u8], b: &[u8]) -> (usize, u8) {
    let mut n = 0;
    let mut most = 0;
    for (p, q) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let d = (0..4).map(|k| p[k].abs_diff(q[k])).max().unwrap();
        n += (d > 0) as usize;
        most = most.max(d);
    }
    (n, most)
}

#[test]
fn bloom_glows_around_emissive_only() {
    every_adapter("bloom", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let env = scene.environment();
        let plain = scene.render(&mut gpu, env, "no_bloom");

        // The HDR path with no glow added matches the forward path (16-bit float rounding only).
        let mut hdr = env;
        hdr.bloom.enabled = true;
        hdr.bloom.intensity = 0.0;
        let same = scene.render(&mut gpu, hdr, "bloom_zero");
        let (n, most) = diff(&plain, &same);
        eprintln!("{test}: HDR path vs forward: {n} pixels differ, by at most {most}");
        assert!(most <= 2, "the HDR path tone maps like the forward path: max diff {most}");

        let mut glow = env;
        glow.bloom.enabled = true;
        let bloomed = scene.render(&mut gpu, glow, "bloom");
        let stats = gpu.last_frame_stats();
        if let Some(t) = &stats.gpu {
            let names: Vec<_> = t.passes.iter().map(|p| p.name).collect();
            assert!(names.contains(&specula_gpu::PASS_BLOOM) && names.contains(&specula_gpu::PASS_TONEMAP), "{names:?}");
        }
        // Beside the glowing cube: brighter.
        let near = [4.75, 0.01, 3.0];
        let (a, b) = (scene.px(&plain, near), scene.px(&bloomed, near));
        eprintln!("{test}: beside the glow {a:?} -> {b:?}");
        assert!(luma(b) > luma(a) + 15, "glow spreads around the emissive cube: {a:?} -> {b:?}");
        // Lit stone far from it: unchanged (only what's brighter than the threshold blooms).
        for p in [[-5.0, 0.01, 5.0], [0.0, 2.0, 0.0], [-6.0, 1.6, -5.0]] {
            let (a, b) = (scene.px(&plain, p), scene.px(&bloomed, p));
            assert!(luma(a).abs_diff(luma(b)) <= 6, "{p:?} doesn't bloom: {a:?} -> {b:?}");
        }
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
    });
}

#[test]
fn fxaa_smooths_edges_only() {
    every_adapter("fxaa", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let mut env = scene.environment();
        env.tonemap = false; // no dither: flat areas must come through exactly
        let plain = scene.render(&mut gpu, env, "no_fxaa");
        env.fxaa = true;
        let smooth = scene.render(&mut gpu, env, "fxaa");
        let (n, most) = diff(&plain, &smooth);
        eprintln!("{test}: {n} pixels changed, by at most {most}");
        assert!(n > 50 && n < (W * H / 10) as usize, "only edge pixels change: {n}");
        // Flat ground and the middle of a face come through unchanged.
        for p in [[-5.0, 0.01, 5.0], [0.0, 1.0, 1.0]] {
            assert_eq!(scene.px(&plain, p), scene.px(&smooth, p), "{p:?}");
        }
        // Bloom and FXAA together (the FXAA-only path is above).
        env.bloom.enabled = true;
        env.tonemap = true;
        scene.render(&mut gpu, env, "bloom_fxaa");
        if let Some(t) = &gpu.last_frame_stats().gpu {
            let names: Vec<_> = t.passes.iter().map(|p| p.name).collect();
            assert!(names.ends_with(&[specula_gpu::PASS_BLOOM, specula_gpu::PASS_TONEMAP, specula_gpu::PASS_FXAA]), "{names:?}");
        }
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
    });
}

#[test]
fn sky_gradient_and_moon_disc() {
    every_adapter("sky", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let mut env = scene.environment();
        env.sky.enabled = true;
        let rgba = scene.render(&mut gpu, env, "sky");
        // The camera looks down: the top row is at the horizon, lower rows look below it (the
        // darker ground colour) where the ground plane ends.
        let horizon = pixel(&rgba, W, W / 2, 1);
        let below = pixel(&rgba, W, 10, 40);
        eprintln!("{test}: horizon {horizon:?}, below it {below:?}");
        assert!(luma(below) < luma(horizon), "below the horizon is darker: {below:?} vs {horizon:?}");
        assert_ne!(horizon, [10, 15, 33, 255], "the sky replaces the clear colour");
        // Looking at the moon: a bright disc where it should be, with a glow around it.
        let m = moon();
        let eye = [0.0, 2.0, 0.0];
        let at = [eye[0] + m[0], eye[1] + m[1], eye[2] + m[2]];
        let view_proj = mul(&perspective_vk_reverse_z(40f32.to_radians(), W as f32 / H as f32, 0.05), &look_at_rh(eye, at, [0.0, 1.0, 0.0]));
        let target = gpu.create_target(W, H).unwrap();
        gpu.render(target, &FrameDesc { view_proj, reverse_z: true, environment: env, ..Default::default() }).unwrap();
        let moon_view = gpu.read_rgba8(target).unwrap();
        write_png(&format!("features_moon_{}.png", gpu.adapter().index), &moon_view, W, H);
        let centre = pixel(&moon_view, W, W / 2, H / 2);
        let halo = pixel(&moon_view, W, W / 2 + 25, H / 2);
        let far = pixel(&moon_view, W, W / 2 + 200, H / 2);
        eprintln!("{test}: disc {centre:?}, halo {halo:?}, away {far:?}");
        // Looking up: the top of the frame (higher) is darker than the bottom (nearer the horizon).
        let (high, low) = (pixel(&moon_view, W, 2, 2), pixel(&moon_view, W, 2, H - 3));
        assert!(luma(high) < luma(low), "zenith darker than the horizon: {high:?} vs {low:?}");
        assert!(luma(centre) > 600, "the disc is near white: {centre:?}");
        assert!(luma(halo) > luma(far), "glow around the disc: {halo:?} vs {far:?}");
        gpu.destroy_target(target);
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
    });
}

#[test]
fn fog_thickens_with_distance_and_depth() {
    every_adapter("fog", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let env = scene.environment();
        let plain = scene.render(&mut gpu, env, "no_fog");
        let mut foggy = env;
        foggy.fog.enabled = true;
        foggy.fog.color = [0.5, 0.55, 0.65];
        foggy.fog.density = 0.05;
        foggy.fog.falloff = 1.5;
        foggy.fog.height = 0.4;
        foggy.fog.sun_scatter = 0.0;
        let fogged = scene.render(&mut gpu, foggy, "fog");
        let change = |p: V3| luma(scene.px(&fogged, p)) - luma(scene.px(&plain, p));
        // Ground: near the camera vs far away.
        let (near, far) = (change([2.0, 0.01, 10.0]), change([2.0, 0.01, -20.0]));
        eprintln!("{test}: ground change near {near}, far {far}");
        assert!(far > near + 20 && near >= 0, "fog thickens with distance: near {near}, far {far}");
        // A pillar at one distance: its foot (in the low fog) changes more than its top.
        let (foot, top) = (change([-6.0, 0.1, -4.55]), change([-6.0, 1.5, -4.55]));
        eprintln!("{test}: pillar foot {foot}, top {top}");
        assert!(foot > top + 10, "fog thickens below its height: foot {foot}, top {top}");
        // The sky fogs too: below the horizon it is all fog.
        foggy.sky.enabled = true;
        let both = scene.render(&mut gpu, foggy, "fog_sky");
        let horizon = pixel(&both, W, 4, 40);
        eprintln!("{test}: fogged sky near the horizon {horizon:?}");
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
    });
}
