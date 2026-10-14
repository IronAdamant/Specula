//! Colour grading and SSAO on real GPUs. Every test skips cleanly (prints why, passes) without
//! Vulkan. Captures go to `$SPECULA_TEST_OUT` (or cargo's per-target temp dir) as `post_*.png`.

mod common;

use common::*;
use specula_gpu::{
    AdapterPreference, DrawItem, Environment, ErrorReason, FrameDesc, Gpu, Material, MeshId, Surface, TextureFormat, TextureId,
    Vertex,
};

const W: u32 = 480;
const H: u32 = 270;

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
        let view = look_at_rh([0.0, 6.0, 9.0], [0.0, 0.5, 0.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(50f32.to_radians(), W as f32 / H as f32, 0.05), &view);
        Scene { cube, ground, view_proj }
    }

    /// Ground, a 2 m stone cube at the origin, a smaller one beside it, a glowing cyan cube.
    fn draws(&self) -> Vec<DrawItem> {
        let mut glow = DrawItem::new(self.cube, scale3([0.6, 0.6, 0.6], [3.0, 0.3, 1.5]), [0.25, 0.88, 0.82, 1.0]);
        glow.surface = Surface { emissive: 2.0, ..Default::default() };
        vec![
            DrawItem::new(self.ground, scale3([60.0, 1.0, 60.0], [0.0; 3]), [0.5, 0.5, 0.5, 1.0]),
            DrawItem::new(self.cube, scale3([2.0, 2.0, 2.0], [0.0, 1.0, 0.0]), [0.55, 0.57, 0.6, 1.0]),
            DrawItem::new(self.cube, scale3([0.8, 0.8, 0.8], [-2.4, 0.4, 0.4]), [0.6, 0.55, 0.5, 1.0]),
            glow,
        ]
    }

    /// Mostly ambient light (where occlusion shows), a weak moon from the left.
    fn environment(&self) -> Environment {
        Environment {
            sun_direction: normalize([-0.7, 0.6, 0.2]),
            sun_color: [0.79, 0.84, 1.0],
            sun_intensity: 0.3,
            ambient_color: [0.8, 0.85, 1.0],
            ambient_intensity: 0.8,
            exposure: 1.0,
            tonemap: true,
            ..Default::default()
        }
    }

    fn render(&self, gpu: &mut Gpu, env: Environment, name: &str) -> Vec<u8> {
        self.render_at(gpu, env, name, 0.0).unwrap()
    }

    fn render_at(&self, gpu: &mut Gpu, env: Environment, name: &str, time_s: f32) -> Result<Vec<u8>, specula_gpu::GpuError> {
        self.render_with(gpu, self.view_proj, &self.draws(), env, name, time_s)
    }

    fn render_with(
        &self,
        gpu: &mut Gpu,
        view_proj: [f32; 16],
        draws: &[DrawItem],
        env: Environment,
        name: &str,
        time_s: f32,
    ) -> Result<Vec<u8>, specula_gpu::GpuError> {
        let target = gpu.create_target(W, H).unwrap();
        let frame = FrameDesc {
            view_proj,
            clear_color: [0.04, 0.06, 0.13, 1.0],
            draws,
            reverse_z: true,
            environment: env,
            time_s,
            ..Default::default()
        };
        let rendered = gpu.render(target, &frame);
        let out = rendered.and_then(|()| gpu.read_rgba8(target));
        gpu.destroy_target(target);
        let rgba = out?;
        let label: String = gpu.adapter().name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let path = write_png(&format!("post_{name}_{label}.png"), &rgba, W, H);
        eprintln!("wrote {}", path.display());
        Ok(rgba)
    }

    fn px(&self, rgba: &[u8], p: V3) -> [u8; 4] {
        let (x, y) = project(&self.view_proj, p, W, H);
        pixel(rgba, W, x as u32, y as u32)
    }
}

fn luma(p: [u8; 4]) -> i32 {
    p[0] as i32 + p[1] as i32 + p[2] as i32
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

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

fn check_clean(test: &str, gpu: Gpu) {
    let errors = validation_errors(test, gpu);
    assert!(errors.is_empty(), "{test}: {} validation error(s)", errors.len());
}

/// An identity 3D LUT strip, `n² × n`: red across each slice, green down, blue by slice.
fn identity_lut(n: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((n * n * n * 4) as usize);
    let v = |i: u32| ((i as f32 / (n - 1) as f32) * 255.0).round() as u8;
    for g in 0..n {
        for b in 0..n {
            for r in 0..n {
                out.extend_from_slice(&[v(r), v(g), v(b), 255]);
            }
        }
    }
    out
}

fn lut(gpu: &mut Gpu, format: TextureFormat) -> TextureId {
    gpu.upload_texture(32 * 32, 32, format, &identity_lut(32)).unwrap()
}

#[test]
fn neutral_grading_changes_nothing() {
    every_adapter("grading_neutral", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        for bloom in [false, true] {
            let mut env = scene.environment();
            env.bloom.enabled = bloom;
            let plain = scene.render(&mut gpu, env, &format!("neutral_off_{bloom}"));
            env.grading.enabled = true;
            env.grading.lut_strength = 0.3; // no LUT set: still neutral
            let graded = scene.render(&mut gpu, env, &format!("neutral_on_{bloom}"));
            assert_eq!(diff(&plain, &graded), (0, 0), "{test}: bloom {bloom}: neutral grading is bit-identical");
        }
        check_clean(test, gpu);
    });
}

#[test]
fn identity_lut_is_identity() {
    every_adapter("grading_lut", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let mut env = scene.environment();
        env.bloom.enabled = true; // the same post path either way: only the LUT differs
        let plain = scene.render(&mut gpu, env, "lut_off");
        for (format, tolerance) in [(TextureFormat::Rgba8Unorm, 1), (TextureFormat::Rgba8Srgb, 2)] {
            let id = lut(&mut gpu, format);
            env.grading.enabled = true;
            env.grading.lut = Some(id);
            let graded = scene.render(&mut gpu, env, &format!("lut_identity_{format:?}"));
            let (n, most) = diff(&plain, &graded);
            eprintln!("{test}: {format:?} identity LUT: {n} pixels differ, by at most {most}");
            assert!(most <= tolerance, "{test}: {format:?}: identity LUT within {tolerance}/255, got {most}");
            gpu.destroy_texture(id);
        }
        // A texture that isn't a LUT strip is a structured error, and so is a destroyed one.
        let bad = gpu.upload_texture(32, 32, TextureFormat::Rgba8Unorm, &[0; 32 * 32 * 4]).unwrap();
        env.grading.lut = Some(bad);
        let e = scene.render_at(&mut gpu, env, "bad", 0.0).unwrap_err();
        assert_eq!(e.reason, Some(ErrorReason::BadLut { width: 32, height: 32 }), "{e}");
        gpu.destroy_texture(bad);
        let e = scene.render_at(&mut gpu, env, "gone", 0.0).unwrap_err();
        assert!(matches!(e.reason, Some(ErrorReason::UnknownId { .. })), "{e}");
        check_clean(test, gpu);
    });
}

#[test]
fn grading_steps_do_what_they_say() {
    every_adapter("grading_steps", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let env = scene.environment();
        let plain = scene.render(&mut gpu, env, "steps_off");
        let top = [0.0, 2.0, 0.3]; // the grey cube's top face
        let glow = [3.0, 0.6, 1.5]; // the cyan cube

        let mut warm = env;
        warm.grading.enabled = true;
        warm.grading.temperature = 0.6;
        let img = scene.render(&mut gpu, warm, "warm");
        let (a, b) = (scene.px(&plain, top), scene.px(&img, top));
        eprintln!("{test}: warm {a:?} -> {b:?}");
        assert!(b[0] as i32 - b[2] as i32 > a[0] as i32 - a[2] as i32 + 10, "warmer: red gains on blue {a:?} -> {b:?}");

        let mut grey = env;
        grey.grading.enabled = true;
        grey.grading.saturation = 0.0;
        let img = scene.render(&mut gpu, grey, "grey");
        let p = scene.px(&img, glow);
        assert!(p[0].abs_diff(p[1]) <= 2 && p[1].abs_diff(p[2]) <= 2, "saturation 0 is grey: {p:?}");

        // Negative vibrance mutes the muted ground more than the saturated glow.
        let mut vib = env;
        vib.grading.enabled = true;
        vib.grading.vibrance = -0.8;
        let img = scene.render(&mut gpu, vib, "vibrance");
        let spread = |p: [u8; 4]| p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap();
        let kept = |p: V3| {
            let (a, b) = (scene.px(&plain, p), scene.px(&img, p));
            eprintln!("{test}: vibrance at {p:?}: {a:?} -> {b:?}");
            spread(b) as f32 / spread(a).max(1) as f32
        };
        let (glow_kept, ground_kept) = (kept(glow), kept([-3.0, 0.01, 3.5]));
        assert!(glow_kept >= 0.85, "the saturated colour keeps most of its saturation: {glow_kept}");
        assert!(ground_kept < glow_kept - 0.1, "the muted ground loses more: {ground_kept} vs {glow_kept}");

        let mut vig = env;
        vig.grading.enabled = true;
        vig.grading.vignette = 1.0;
        let img = scene.render(&mut gpu, vig, "vignette");
        let corner = |rgba: &[u8]| luma(pixel(rgba, W, 2, 2));
        let centre = |rgba: &[u8]| luma(pixel(rgba, W, W / 2, H / 2));
        assert!(corner(&img) * 2 < corner(&plain).max(4), "dark corners: {} -> {}", corner(&plain), corner(&img));
        assert!(centre(&img).abs_diff(centre(&plain)) <= 3, "the centre keeps its light");

        let mut lgg = env;
        lgg.grading.enabled = true;
        lgg.grading.lift = [0.0, 0.0, 0.08]; // blue shadows
        let img = scene.render(&mut gpu, lgg, "lift");
        let shadow = [-1.4, 0.6, 1.2]; // the cube's side away from the moon? any dark pixel will do
        let (a, b) = (scene.px(&plain, shadow), scene.px(&img, shadow));
        assert!(b[2] > a[2] + 5 && b[0].abs_diff(a[0]) <= 2, "lift raises blue in the shadows: {a:?} -> {b:?}");

        // Grain: reproducible at a fixed time, different at another.
        let mut grain = env;
        grain.grading.enabled = true;
        grain.grading.grain = 0.5;
        let g1 = scene.render_at(&mut gpu, grain, "grain_a", 1.0).unwrap();
        let g2 = scene.render_at(&mut gpu, grain, "grain_b", 1.0).unwrap();
        let g3 = scene.render_at(&mut gpu, grain, "grain_c", 2.0).unwrap();
        assert_eq!(g1, g2, "the same time gives the same grain");
        let (n, _) = diff(&g1, &g3);
        assert!(n > (W * H / 4) as usize, "another time gives other grain: {n} pixels changed");
        let (n, most) = diff(&plain, &g1);
        eprintln!("{test}: grain changed {n} pixels by at most {most}");
        assert!(most < 40, "grain is subtle at 0.5: {most}");
        check_clean(test, gpu);
    });
}

#[test]
fn ssao_darkens_creases_not_open_surfaces() {
    every_adapter("ssao", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let env = scene.environment();
        let plain = scene.render(&mut gpu, env, "ssao_off");
        let mut on = env;
        on.ssao.enabled = true;
        let ao = scene.render(&mut gpu, on, "ssao_on");
        let stats = gpu.last_frame_stats();
        if let Some(t) = &stats.gpu {
            let names: Vec<_> = t.passes.iter().map(|p| p.name).collect();
            eprintln!("{test}: passes {names:?}, {:?}", t.passes);
            assert!(names.contains(&specula_gpu::PASS_SSAO), "{names:?}");
        }
        // Where the cube meets the ground (in front, facing the camera): darker.
        for p in [[0.0, 0.04, 1.08], [0.0, 0.12, 1.0], [-2.4, 0.03, 0.86]] {
            let (a, b) = (scene.px(&plain, p), scene.px(&ao, p));
            eprintln!("{test}: crease {p:?} {a:?} -> {b:?}");
            assert!(luma(b) * 100 < luma(a) * 90, "{p:?}: the crease loses ambient light {a:?} -> {b:?}");
        }
        // Open ground, the middle of the cube's top: unchanged.
        for p in [[-3.0, 0.01, 3.5], [3.5, 0.01, 3.5], [0.0, 2.0, 0.0]] {
            let (a, b) = (scene.px(&plain, p), scene.px(&ao, p));
            eprintln!("{test}: open {p:?} {a:?} -> {b:?}");
            assert!(luma(a).abs_diff(luma(b)) <= 6, "{p:?}: open surfaces keep their light {a:?} -> {b:?}");
        }
        // Emission isn't occluded: the glowing cube's lit face at its foot keeps most of its light.
        let foot = [3.0, 0.1, 1.81];
        let (a, b) = (scene.px(&plain, foot), scene.px(&ao, foot));
        assert!(luma(b) * 100 >= luma(a) * 85, "emission stays: {a:?} -> {b:?}");
        // Deterministic, and switching it off again restores the image exactly.
        assert_eq!(scene.render(&mut gpu, on, "ssao_again"), ao, "SSAO is deterministic");
        let off = scene.render(&mut gpu, env, "ssao_off_again");
        assert_eq!(diff(&plain, &off), (0, 0), "SSAO off is bit-identical");
        // Every resolution and sample count renders cleanly; with grading, bloom and FXAA too.
        for (res, samples) in [(0.25, 4), (1.0, 16)] {
            let mut e = on;
            e.ssao.resolution = res;
            e.ssao.samples = samples;
            e.bloom.enabled = true;
            e.fxaa = true;
            e.grading.enabled = true;
            e.grading.contrast = 1.1;
            scene.render(&mut gpu, e, &format!("ssao_{res}_{samples}"));
        }
        check_clean(test, gpu);
    });
}

/// Linear luminance of an sRGB pixel (tone mapping off: output = scene light, clamped).
fn linear_luma(p: [u8; 4]) -> f32 {
    let c = |k: usize| srgb_to_linear(p[k] as f32 / 255.0);
    0.2126 * c(0) + 0.7152 * c(1) + 0.0722 * c(2)
}

#[test]
fn ssao_under_a_blob_shadow_takes_the_same_share() {
    // A blob shadow (black, premultiplied) over a crease darkens everything under it alike, so
    // SSAO must take the same share of the light with or without it.
    every_adapter("ssao_blob", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let mut env = scene.environment();
        env.tonemap = false; // linear output: shares compare directly
        env.ambient_intensity = 0.5;
        env.sun_intensity = 0.2;
        let mut on = env;
        on.ssao.enabled = true;
        let crease = [0.0, 0.04, 1.08];
        let mut with_blob = scene.draws();
        let mut blob = DrawItem::new(scene.ground, scale3([3.0, 1.0, 3.0], [0.0, 0.005, 1.3]), [0.0, 0.0, 0.0, 0.6]);
        blob.material = Material::BlobShadow;
        with_blob.push(blob);
        let mut shares = Vec::new();
        for (name, draws) in [("plain", scene.draws()), ("blob", with_blob)] {
            let off_img = scene.render_with(&mut gpu, scene.view_proj, &draws, env, &format!("blob_{name}_off"), 0.0).unwrap();
            let on_img = scene.render_with(&mut gpu, scene.view_proj, &draws, on, &format!("blob_{name}_on"), 0.0).unwrap();
            let (a, b) = (scene.px(&off_img, crease), scene.px(&on_img, crease));
            let share = linear_luma(b) / linear_luma(a).max(1e-6);
            eprintln!("{test}: {name}: crease {a:?} -> {b:?}, keeps {share:.3}");
            shares.push(share);
        }
        assert!(shares[0] < 0.9, "SSAO darkens the crease: {shares:?}");
        assert!((shares[0] - shares[1]).abs() < 0.04, "a blob shadow doesn't change SSAO's share: {shares:?}");
        check_clean(test, gpu);
    });
}

/// Reversed-Z orthographic projection (depth 1 at `near`, 0 at `far`), Vulkan clip (y down).
fn orthographic_vk_reverse_z(half_height: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let mut m = [0.0; 16];
    m[0] = 1.0 / (half_height * aspect);
    m[5] = -1.0 / half_height;
    m[10] = 1.0 / (far - near);
    m[14] = far / (far - near);
    m[15] = 1.0;
    m
}

#[test]
fn ssao_works_with_an_orthographic_camera() {
    every_adapter("ssao_ortho", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu);
        let view = look_at_rh([6.0, 6.0, 9.0], [0.0, 0.5, 0.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&orthographic_vk_reverse_z(4.0, W as f32 / H as f32, 0.1, 60.0), &view);
        let env = scene.environment();
        let mut on = env;
        on.ssao.enabled = true;
        // A long low box whose ground crease shows on screen right behind the big cube's top
        // edge, about 3.5 m further away: a blur that can't tell orthographic depths apart
        // smears that crease onto the top.
        let mut draws = scene.draws();
        draws.push(DrawItem::new(scene.cube, scale3([2.4, 0.8, 0.8], [-2.2, 0.4, -4.85]), [0.6, 0.55, 0.5, 1.0]));
        let off_img = scene.render_with(&mut gpu, view_proj, &draws, env, "ortho_off", 0.0).unwrap();
        let on_img = scene.render_with(&mut gpu, view_proj, &draws, on, "ortho_on", 0.0).unwrap();
        let px = |rgba: &[u8], p: V3| {
            let (x, y) = project(&view_proj, p, W, H);
            pixel(rgba, W, x as u32, y as u32)
        };
        for p in [[0.0, 0.04, 1.08], [1.08, 0.04, 0.0]] {
            let (a, b) = (px(&off_img, p), px(&on_img, p));
            eprintln!("{test}: crease {p:?} {a:?} -> {b:?}");
            assert!(luma(b) * 100 < luma(a) * 90, "{p:?}: the crease loses ambient light {a:?} -> {b:?}");
        }
        // Open ground and the cube's top, including a few pixels inside its back edge, where the
        // long box's crease shows right behind it on screen: the blur must not carry it across the
        // depth edge (orthographic depth taken as 1 everywhere did: those pixels lost 8 levels).
        // Within one occlusion texel of the edge the bilinear upsample still mixes (by design).
        for p in [[-3.0, 0.01, 3.5], [3.5, 0.01, -2.0], [0.0, 2.0, 0.0], [-0.88, 2.0, 0.0], [-0.6, 2.0, -0.75], [0.0, 2.0, -0.75], [0.6, 2.0, -0.75]] {
            let (a, b) = (px(&off_img, p), px(&on_img, p));
            eprintln!("{test}: open {p:?} {a:?} -> {b:?}");
            assert!(luma(a).abs_diff(luma(b)) <= 6, "{p:?}: open surfaces keep their light {a:?} -> {b:?}");
        }
        check_clean(test, gpu);
    });
}
