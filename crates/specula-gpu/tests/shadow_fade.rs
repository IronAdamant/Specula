//! Swaying casters' sun shadows thin out near the wind's `shadow_end` instead of popping off at
//! it (M5 gpuwork, renderfollow follow-up): a row of swaying cubes over a lit floor, the camera
//! looking along the row, `shadow_end` 28 m (the fade from 21 m). The darkness of each cube's
//! shadow on the floor is measured in the image: full well before the fade, partial inside it,
//! none past the end, and never darker farther along. Still casters at the same distances keep
//! their full shadows. On real GPUs under validation; skips cleanly without Vulkan.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, Environment, FrameDesc, ShadowSettings, Wind, WindSettings};

const W: u32 = 640;
const H: u32 = 360;
const EYE: V3 = [0.0, 4.0, 0.0];
/// Caster view depths (the camera looks along -z, nearly level).
const DEPTHS: [f32; 7] = [12.0, 17.0, 22.0, 24.5, 26.5, 27.5, 31.0];

fn every_adapter(test: &str, f: impl Fn(&str, AdapterPreference)) {
    if !specula_gpu::is_available() {
        eprintln!("{test}: SKIPPED — Vulkan is not available");
        return;
    }
    for a in specula_gpu::enumerate_adapters().unwrap_or_default() {
        f(&format!("{test}[{}]", a.index), AdapterPreference::Index(a.index));
    }
}

fn env(shadow_end: f32) -> Environment {
    Environment {
        sun_direction: [0.0, 1.0, 0.0],
        sun_intensity: 0.9,
        ambient_intensity: 0.3,
        shadow: ShadowSettings { enabled: true, distance: 60.0, softness: 1.0, ..Default::default() },
        // Still air (no sway: the cubes stay put), but the draws are swaying casters.
        wind: WindSettings { strength: 0.0, shadow_end, ..Default::default() },
        ..Default::default()
    }
}

/// Mean brightness of the floor just beside each caster's foot, in its shadow (the sun is straight
/// above, so the shadow is the cube's footprint; the band in front of the cube is measured).
fn shadow_darkness(rgba: &[u8], view_proj: &[f32; 16], depth: f32, x: f32) -> f32 {
    let mut sum = 0.0f32;
    let mut n = 0.0f32;
    for k in 0..5 {
        for j in 0..5 {
            // The floor (y = 0) under the cube's near half, between its edges.
            let p = [x - 0.2 + 0.1 * k as f32, 0.0, -depth + 0.1 + 0.05 * j as f32];
            let (px, py) = project(view_proj, p, W, H);
            if px < 0.0 || py < 0.0 || px >= W as f32 || py >= H as f32 {
                continue;
            }
            let c = pixel(rgba, W, px as u32, py as u32);
            sum += (c[0] as f32 + c[1] as f32 + c[2] as f32) / 3.0;
            n += 1.0;
        }
    }
    sum / n.max(1.0)
}

/// Lit floor brightness at depth `depth`, away from every caster (x = 3).
fn lit(rgba: &[u8], view_proj: &[f32; 16], depth: f32) -> f32 {
    shadow_darkness(rgba, view_proj, depth, 3.0)
}

#[test]
fn swaying_shadows_fade_out_before_the_winds_shadow_end() {
    every_adapter("swaying_shadows_fade_out_before_the_winds_shadow_end", |test, adapter| {
        let Some(mut gpu) = gpu_or_skip(test, adapter) else { return };
        let (cv, ci) = cube();
        let cube = gpu.upload_mesh(&cv, &ci).unwrap();
        let view = look_at_rh(EYE, [0.0, 0.0, -40.0], [0.0, 1.0, 0.0]);
        let view_proj = mul(&perspective_vk_reverse_z(50f32.to_radians(), W as f32 / H as f32, 0.05), &view);
        let mut floor = DrawItem::new(cube, translation([0.0, -0.5, -25.0]), [0.8, 0.8, 0.8, 1.0]);
        // A 20 x 1 x 60 slab whose top is y = 0.
        floor.model = [20.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 60.0, 0.0, 0.0, -0.5, -25.0, 1.0];
        let target = gpu.create_target(W, H).unwrap();
        let mut measure = |swaying: bool, shadow_end: f32| -> Vec<(f32, f32)> {
            let mut draws = vec![floor];
            for &d in &DEPTHS {
                // A unit cube 1.5 m above the floor (the floor under it stays in view).
                let mut c = DrawItem::new(cube, translation([-2.0, 2.0, -d]), [0.6, 0.3, 0.2, 1.0]);
                if swaying {
                    c.surface.wind = Wind { amount: 0.05, height: 1.0, painted: false };
                }
                draws.push(c);
            }
            let frame = FrameDesc { view_proj, clear_color: [0.1, 0.1, 0.15, 1.0], draws: &draws, reverse_z: true, environment: env(shadow_end), ..Default::default() };
            gpu.render(target, &frame).unwrap();
            let rgba = gpu.read_rgba8(target).unwrap();
            DEPTHS.iter().map(|&d| (shadow_darkness(&rgba, &view_proj, d, -2.0), lit(&rgba, &view_proj, d))).collect()
        };
        let still = measure(false, 28.0);
        let sway = measure(true, 28.0);
        let unlimited = measure(true, 0.0);
        // How much of each shadow is left: 1 = as dark as a still caster's, 0 = the lit floor.
        let left: Vec<f32> = sway.iter().zip(&still).map(|(s, st)| ((s.1 - s.0) / (st.1 - st.0).max(1.0)).clamp(-0.2, 1.2)).collect();
        eprintln!("{test}: still {still:?}\n  sway {sway:?}\n  unlimited {unlimited:?}\n  left {left:?}");
        for (i, st) in still.iter().enumerate() {
            assert!(st.1 - st.0 > 20.0, "{test}: a still caster at {} m casts a clear shadow: {st:?}", DEPTHS[i]);
        }
        for (i, u) in unlimited.iter().enumerate() {
            assert!((u.0 - still[i].0).abs() <= 3.0, "{test}: with no shadow_end a swaying caster at {} m casts as a still one: {u:?} vs {:?}", DEPTHS[i], still[i]);
        }
        // Before the fade (12, 17 m): full.
        assert!(left[0] > 0.9 && left[1] > 0.9, "{test}: {left:?}");
        // Past the end (31 m): none.
        assert!(left[6].abs() < 0.1, "{test}: {left:?}");
        // Inside the fade (22-27.5 m): partial (the pop is gone), and thinner at every step
        // (measured on both test adapters: 0.81, 0.40, 0.17, 0.06).
        assert!(left[2..6].iter().all(|&l| l > 0.02 && l < 0.95), "{test}: no partial shadow in the fade: {left:?}");
        assert!(left[2..6].windows(2).all(|w| w[1] < w[0]), "{test}: the fade is not gradual: {left:?}");
        // Never darker farther along.
        for w in left.windows(2) {
            assert!(w[1] <= w[0] + 0.05, "{test}: the shadow grew darker farther along: {left:?}");
        }
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: validation errors: {errors:?}");
    });
}
