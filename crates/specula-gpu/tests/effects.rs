//! Particles and decals on real GPUs (src/effects.rs): particles add light or cover, hide behind
//! geometry and fade softly into it; decals paint onto the opaque surface inside their box (and
//! only there), multiplying or re-lit, with the camera outside or inside the box; frames without
//! effects keep their old passes; no validation errors. Skips cleanly without Vulkan. Captures
//! go to `$SPECULA_TEST_OUT` as `effects_*.png`.

mod common;

use common::*;
use specula_gpu::{
    AdapterPreference, Decal, DecalMode, DrawItem, Effects, Environment, FrameDesc, Gpu, Material, MeshId, OverlayQuad, Particle, ParticleMaterial, Vertex,
};

const W: u32 = 320;
const H: u32 = 200;

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

fn box_model(s: V3, t: V3) -> [f32; 16] {
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
    fn new(gpu: &mut Gpu, eye: V3) -> Scene {
        let (v, i) = cube();
        let cube = gpu.upload_mesh(&v, &i).unwrap();
        let n = [0.0, 1.0, 0.0];
        let quad = [[-0.5, 0.0, 0.5], [0.5, 0.0, 0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, -0.5]].map(|p| Vertex::new(p, n));
        let ground = gpu.upload_mesh(&quad, &[0, 1, 2, 0, 2, 3]).unwrap();
        let view = look_at_rh(eye, [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(60f32.to_radians(), W as f32 / H as f32, 0.05), &view);
        Scene { cube, ground, view_proj }
    }

    fn draws(&self, glass: bool) -> Vec<DrawItem> {
        let mut d = vec![
            DrawItem::new(self.ground, box_model([40.0, 1.0, 40.0], [0.0, 0.0, 0.0]), [0.6, 0.6, 0.6, 1.0]),
            DrawItem::new(self.cube, box_model([2.0, 2.0, 2.0], [0.0, 1.0, 0.0]), [0.5, 0.55, 0.6, 1.0]),
        ];
        if glass {
            let mut g = DrawItem::new(self.cube, box_model([1.0, 1.0, 1.0], [-4.0, 0.5, 2.0]), [0.7, 0.9, 1.0, 0.3]);
            g.material = Material::Glass;
            d.push(g);
        }
        d
    }

    /// The image and the frame graph's pass names.
    fn render(&self, gpu: &mut Gpu, env: Environment, effects: Effects, glass: bool, name: &str) -> (Vec<u8>, Vec<String>) {
        let target = gpu.create_target(W, H).unwrap();
        let draws = self.draws(glass);
        // A HUD quad in the corner: in a direct frame it joins the last effect pass (no depth there).
        let overlay = [OverlayQuad::solid([2.0, 2.0, 12.0, 8.0], [1.0, 0.0, 0.0, 1.0])];
        let frame = FrameDesc {
            view_proj: self.view_proj,
            clear_color: [0.05, 0.07, 0.12, 1.0],
            draws: &draws,
            reverse_z: true,
            environment: env,
            effects,
            overlay: &overlay,
            ..Default::default()
        };
        gpu.render(target, &frame).unwrap();
        let rgba = gpu.read_rgba8(target).unwrap();
        let names = gpu.last_graph().map(|g| g.passes.iter().map(|p| p.name.to_string()).collect()).unwrap_or_default();
        gpu.destroy_target(target);
        let label: String = gpu.adapter().name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        write_png(&format!("effects_{name}_{label}.png"), &rgba, W, H);
        assert_eq!(pixel(&rgba, W, 6, 5)[..3], [255, 0, 0], "{name}: the overlay is drawn over the effects");
        (rgba, names)
    }

    fn px(&self, rgba: &[u8], p: V3) -> [u8; 4] {
        let (x, y) = project(&self.view_proj, p, W, H);
        pixel(rgba, W, x as u32, y as u32)
    }
}

fn sum(p: [u8; 4]) -> i32 {
    p[0] as i32 + p[1] as i32 + p[2] as i32
}

fn hdr_env() -> Environment {
    let mut env = Environment { tonemap: true, ..Default::default() };
    env.bloom.enabled = true;
    env.ssao.enabled = true;
    env.fog.enabled = true;
    env.fog.density = 0.002;
    env
}

#[test]
fn particles_add_light_cover_and_hide_behind_geometry() {
    every_adapter("particles", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu, [0.0, 1.5, 10.0]);
        let front = [2.5, 1.0, 3.0];
        let behind = [0.0, 1.0, -3.0];
        for (env, kind) in [(Environment::default(), "direct"), (hdr_env(), "hdr")] {
            // With bloom on, glowing effects spill a little light onto their surroundings.
            let tol = if kind == "hdr" { 12 } else { 3 };
            let (base, names) = scene.render(&mut gpu, env, Effects::default(), false, &format!("particles_base_{kind}"));
            assert!(!names.is_empty() && !names.iter().any(|n| n == "particles" || n == "decals"), "{test}: no effect passes without effects: {names:?}");
            let materials = [ParticleMaterial::default()];
            let glow = Particle { position: front, size: 1.0, color: [3.0, 0.8, 0.1, 1.0], additive: 1.0, ..Default::default() };
            let dark = Particle { position: [-2.5, 1.0, 3.0], size: 1.0, color: [0.0, 0.0, 0.0, 1.0], additive: 0.0, ..Default::default() };
            let hidden = Particle { position: behind, size: 0.6, color: [5.0, 5.0, 5.0, 1.0], additive: 1.0, ..Default::default() };
            let particles = [hidden, dark, glow];
            let fx = Effects { particles: &particles, materials: &materials, decals: &[] };
            let (lit, names) = scene.render(&mut gpu, env, fx, false, &format!("particles_{kind}"));
            let stats = gpu.last_frame_stats();
            assert_eq!((stats.draws.particles, stats.draws.particle_draw_calls), (3, 1), "{test}");
            assert!(names.iter().any(|n| n == "particles"), "{test}: {names:?}");
            let (b, l) = (scene.px(&base, front), scene.px(&lit, front));
            assert!(sum(l) > sum(b) + 60 && l[0] > l[2], "{test} {kind}: additive glow {b:?} -> {l:?}");
            let (b, l) = (scene.px(&base, [-2.5, 1.0, 3.0]), scene.px(&lit, [-2.5, 1.0, 3.0]));
            assert!(sum(l) + 30 < sum(b), "{test} {kind}: an alpha particle covers {b:?} -> {l:?}");
            let (b, l) = (scene.px(&base, behind), scene.px(&lit, behind));
            assert!((sum(l) - sum(b)).abs() <= tol, "{test} {kind}: behind the cube nothing shows {b:?} -> {l:?}");
        }
        // Soft: a quad half in the ground fades where it meets it.
        let materials = [ParticleMaterial::default(), ParticleMaterial { soft: 1.0, ..Default::default() }];
        let near_ground = [3.0, 0.08, 3.0];
        let hard = [Particle { position: [3.0, 0.3, 3.0], size: 1.4, color: [0.0, 0.5, 0.5, 1.0], additive: 1.0, material: 0, ..Default::default() }];
        let soft = [Particle { material: 1, ..hard[0] }];
        let (h, _) = scene.render(&mut gpu, Environment::default(), Effects { particles: &hard, materials: &materials, decals: &[] }, false, "particles_hard");
        let (s, _) = scene.render(&mut gpu, Environment::default(), Effects { particles: &soft, materials: &materials, decals: &[] }, false, "particles_soft");
        assert!(sum(scene.px(&s, near_ground)) + 40 < sum(scene.px(&h, near_ground)), "{test}: soft fades {:?} vs {:?}", scene.px(&s, near_ground), scene.px(&h, near_ground));
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}

#[test]
fn decals_paint_the_surface_in_their_box_only() {
    every_adapter("decals", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let scene = Scene::new(&mut gpu, [0.0, 4.0, 10.0]);
        let on = [3.0, 0.0, 3.0];
        let off = [-3.0, 0.0, 3.0];
        for (env, kind) in [(Environment::default(), "direct"), (hdr_env(), "hdr")] {
            // With bloom on, glowing effects spill a little light onto their surroundings.
            let tol = if kind == "hdr" { 12 } else { 3 };
            let (base, _) = scene.render(&mut gpu, env, Effects::default(), true, &format!("decals_base_{kind}"));
            let decals = [
                Decal { model: box_model([2.0, 0.6, 2.0], on), color: [0.1, 0.1, 0.1, 1.0], mode: DecalMode::Multiply, ..Default::default() },
                Decal { model: box_model([1.0, 0.6, 1.0], [0.0, 0.0, 4.0]), color: [1.0, 0.1, 0.1, 1.0], emissive: 2.0, ..Default::default() },
            ];
            let fx = Effects { particles: &[], materials: &[], decals: &decals };
            let (painted, names) = scene.render(&mut gpu, env, fx, true, &format!("decals_{kind}"));
            let stats = gpu.last_frame_stats();
            assert_eq!((stats.draws.decals, stats.draws.decal_draw_calls), (2, 2), "{test}");
            assert!(names.iter().any(|n| n == "decals") && names.iter().any(|n| n == "scene_blend"), "{test}: {names:?}");
            let (b, p) = (scene.px(&base, on), scene.px(&painted, on));
            assert!(sum(p) * 2 < sum(b), "{test} {kind}: multiply darkens {b:?} -> {p:?}");
            let (b, p) = (scene.px(&base, off), scene.px(&painted, off));
            assert!((sum(p) - sum(b)).abs() <= tol, "{test} {kind}: outside the box nothing changes {b:?} -> {p:?}");
            let r = scene.px(&painted, [0.0, 0.0, 4.0]);
            assert!(r[0] as i32 > r[1] as i32 + 40, "{test} {kind}: the lit decal is red {r:?}");
            // The cube stands outside both boxes.
            let (b, p) = (scene.px(&base, [0.0, 1.0, 1.0]), scene.px(&painted, [0.0, 1.0, 1.0]));
            assert!((sum(p) - sum(b)).abs() <= tol, "{test} {kind}: the cube is untouched {b:?} -> {p:?}");
        }
        // The camera inside a decal's box (its back faces still cover the screen).
        let (base, _) = scene.render(&mut gpu, Environment::default(), Effects::default(), false, "decals_inside_base");
        let big = [Decal { model: box_model([60.0, 20.0, 60.0], [0.0, 0.0, 0.0]), color: [0.5, 0.5, 0.5, 1.0], mode: DecalMode::Multiply, ..Default::default() }];
        let (inside, names) = scene.render(&mut gpu, Environment::default(), Effects { particles: &[], materials: &[], decals: &big }, false, "decals_inside");
        assert!(!names.iter().any(|n| n == "scene_blend"), "{test}: nothing blended, no scene_blend pass: {names:?}");
        let (b, p) = (scene.px(&base, off), scene.px(&inside, off));
        assert!(sum(p) + 20 < sum(b), "{test}: inside the box the decal still applies {b:?} -> {p:?}");
        assert!(validation_errors(test, gpu).is_empty(), "{test}: validation errors");
    });
}

/// Timing report: what effects cost on each GPU, in a Sepulcra-like HDR frame (shadows, SSAO,
/// bloom, FXAA) at 1280x720: none, particles only, decals only, both. Median GPU ms over 40
/// frames. `cargo test --release -p specula-gpu --test effects effects_cost -- --ignored --nocapture`.
#[test]
#[ignore = "timing report"]
fn effects_cost_report() {
    every_adapter("effects_cost", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let (w, h) = (1280u32, 720u32);
        let (v, i) = cube();
        let cube = gpu.upload_mesh(&v, &i).unwrap();
        let view = look_at_rh([0.0, 14.0, 24.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(45f32.to_radians(), w as f32 / h as f32, 0.05), &view);
        let mut draws = vec![DrawItem::new(cube, box_model([60.0, 1.0, 60.0], [0.0, -0.5, 0.0]), [0.36, 0.39, 0.44, 1.0])];
        for k in 0..400 {
            let (x, z) = ((k % 20) as f32 * 2.0 - 19.0, (k / 20) as f32 * 2.0 - 19.0);
            draws.push(DrawItem::new(cube, box_model([0.8, 1.6, 0.8], [x, 0.8, z]), [0.55, 0.59, 0.64, 1.0]));
        }
        let mut glass = DrawItem::new(cube, box_model([1.0, 1.0, 1.0], [2.0, 2.5, 4.0]), [0.6, 0.9, 1.0, 0.3]);
        glass.material = Material::Glass;
        draws.push(glass);
        let mut env = hdr_env();
        env.shadow.enabled = true;
        env.fxaa = true;
        let materials = [ParticleMaterial::default(), ParticleMaterial { soft: 0.4, lit: true, ..Default::default() }];
        let particles: Vec<Particle> = (0..600)
            .map(|k| Particle {
                position: [((k * 37) % 40) as f32 * 0.5 - 10.0, 0.5 + ((k * 13) % 11) as f32 * 0.2, ((k * 53) % 40) as f32 * 0.5 - 10.0],
                size: 0.35,
                color: [2.0, 0.8, 0.3, 0.8],
                additive: (k % 2) as f32,
                material: (k % 2) as u16,
                ..Default::default()
            })
            .collect();
        let decals: Vec<Decal> = (0..12)
            .map(|k| Decal {
                model: box_model([2.0, 0.6, 2.0], [(k % 4) as f32 * 4.0 - 6.0, 0.0, (k / 4) as f32 * 4.0 - 4.0]),
                color: [0.2, 0.2, 0.2, 1.0],
                mode: if k % 2 == 0 { DecalMode::Multiply } else { DecalMode::Lit },
                ..Default::default()
            })
            .collect();
        let target = gpu.create_target(w, h).unwrap();
        for (label, p, d) in [("none", false, false), ("particles", true, false), ("decals", false, true), ("both", true, true)] {
            let fx = Effects { particles: if p { &particles } else { &[] }, materials: &materials, decals: if d { &decals } else { &[] } };
            let frame = FrameDesc { view_proj, clear_color: [0.05, 0.07, 0.12, 1.0], draws: &draws, reverse_z: true, environment: env, effects: fx, ..Default::default() };
            let mut totals = Vec::new();
            let mut passes: std::collections::BTreeMap<&str, Vec<f64>> = Default::default();
            for _ in 0..40 {
                gpu.render(target, &frame).unwrap();
                if let Some(t) = gpu.last_frame_stats().gpu {
                    totals.push(t.total_ms);
                    for p in t.passes {
                        passes.entry(p.name).or_default().push(p.gpu_ms);
                    }
                }
            }
            let median = |v: &mut Vec<f64>| {
                v.sort_by(f64::total_cmp);
                v.get(v.len() / 2).copied().unwrap_or(0.0)
            };
            let per: Vec<String> = passes.iter_mut().map(|(n, v)| format!("{n} {:.3}", median(v))).collect();
            eprintln!("{} {label}: total {:.3} ms | {}", gpu.adapter().name, median(&mut totals), per.join(", "));
        }
        gpu.destroy_target(target);
    });
}
