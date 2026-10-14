//! The post chain: per-target intermediate images (HDR scene, bloom levels, the tone mapped image
//! FXAA reads) and the full-screen passes between the scene and the final image.
//!
//! Paths, by [`PostMode`]:
//! - nothing on: the scene renders straight into the final image, tone mapped in the forward
//!   pass (the M0/M1 path, bit for bit);
//! - FXAA only: scene (tone mapped) → LDR image → FXAA → final;
//! - bloom and/or grading: scene (exposed linear) → HDR image → bloom chain (if on) → tonemap
//!   (+ grading) → final, or → LDR image → FXAA → final.
//!
//! Grading ([`crate::GradingSettings`]) runs inside the tonemap pass (`shaders/composite.frag`):
//! its numbers travel in the frame data ([`grading_data`]), its LUT slot and the steps that are
//! not neutral in `FrameData::post_slots` (each neutral step is skipped, so neutral grading
//! changes nothing, bit for bit).

use crate::device::Device;
use crate::render::{FrameGraph, FramePass, FramePipelines, FrameRes, PUSH_STAGES};
use crate::vk::*;
use crate::{Environment, GradingSettings};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

/// Most bloom levels.
pub const MAX_BLOOM_LEVELS: u32 = 8;
/// `PostPush::src2` when there is no second input.
const NO_TEXTURE: u32 = u32::MAX;

/// Push constants of the post passes; must match `Push` in `shaders/post.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostPush {
    pub frame: u64,
    pub src: u32,
    pub src2: u32,
    pub texel: [f32; 2],
    pub mode: u32,
    pub pad: u32,
    pub params: [f32; 4],
}

const _: () = assert!(size_of::<PostPush>() == 48);

/// Which post passes a frame runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PostMode {
    /// Bloom levels (0 = no bloom).
    pub bloom_levels: u32,
    pub fxaa: bool,
    /// Grading with at least one step that isn't neutral.
    pub grading: bool,
    /// Screen-space ambient occlusion (passes before the scene; see `ssao.rs`).
    pub ssao: bool,
}

impl PostMode {
    pub fn of(env: &Environment) -> PostMode {
        PostMode {
            bloom_levels: if env.bloom.enabled { env.bloom.levels.clamp(1, MAX_BLOOM_LEVELS) } else { 0 },
            fxaa: env.fxaa,
            grading: env.grading.active(),
            ssao: env.ssao.enabled,
        }
    }

    /// The scene renders exposed linear light into an HDR image and a tonemap pass follows.
    pub fn hdr(self) -> bool {
        self.bloom() || self.grading
    }

    /// The mode as it applies to a target of `extent` (bloom needs at least one level to fit).
    pub fn fit(self, extent: VkExtent2D) -> PostMode {
        PostMode { bloom_levels: bloom_level_count(extent, self.bloom_levels), ..self }
    }

    pub fn bloom(self) -> bool {
        self.bloom_levels > 0
    }

    /// The scene goes straight into the final image.
    pub fn direct(self) -> bool {
        !self.hdr() && !self.fxaa
    }
}

/// Names of the bloom images and passes, by level (level 0 = half the target's size): `levels`
/// hold the downsampled bright pass, `blur[i]` the blur of everything from level `i` down. Every
/// pass writes a fresh image (no load-and-blend into an image read before: that corrupted on
/// Intel, and it costs tile loads).
pub const BLOOM_LEVELS: [&str; MAX_BLOOM_LEVELS as usize] =
    ["bloom_0", "bloom_1", "bloom_2", "bloom_3", "bloom_4", "bloom_5", "bloom_6", "bloom_7"];
pub const BLOOM_BLURS: [&str; MAX_BLOOM_LEVELS as usize - 1] =
    ["bloom_blur_0", "bloom_blur_1", "bloom_blur_2", "bloom_blur_3", "bloom_blur_4", "bloom_blur_5", "bloom_blur_6"];
pub const BLOOM_DOWN_PASSES: [&str; MAX_BLOOM_LEVELS as usize] =
    ["bloom_down_0", "bloom_down_1", "bloom_down_2", "bloom_down_3", "bloom_down_4", "bloom_down_5", "bloom_down_6", "bloom_down_7"];
pub const BLOOM_UP_PASSES: [&str; MAX_BLOOM_LEVELS as usize - 1] =
    ["bloom_up_0", "bloom_up_1", "bloom_up_2", "bloom_up_3", "bloom_up_4", "bloom_up_5", "bloom_up_6"];

/// Levels that fit `extent` (each at least 1×1), up to `wanted`.
pub fn bloom_level_count(extent: VkExtent2D, wanted: u32) -> u32 {
    let mut n = 0;
    while n < wanted && (extent.width.min(extent.height) >> (n + 1)) >= 1 {
        n += 1;
    }
    n
}

/// Size of bloom level `i` of a target of `extent`.
pub fn bloom_level_size(extent: VkExtent2D, i: u32) -> VkExtent2D {
    VkExtent2D { width: (extent.width >> (i + 1)).max(1), height: (extent.height >> (i + 1)).max(1) }
}

/// The clear value of an HDR scene target such that, after the tonemap pass, the background shows
/// `clear` (sRGB) as the forward path would: the inverse of the ACES curve when tone mapping.
pub fn hdr_clear(clear_linear: [f32; 4], tonemap: bool) -> [f32; 4] {
    if !tonemap {
        return clear_linear;
    }
    let inv = |y: f32| {
        // Solve (2.51 − 2.43y)x² + (0.03 − 0.59y)x − 0.14y = 0 for x ≥ 0 (the curve's range ends
        // just above 1).
        let y = y.clamp(0.0, 0.999) as f64;
        let (a, b, c) = (2.51 - 2.43 * y, 0.03 - 0.59 * y, -0.14 * y);
        ((-b + (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)) as f32
    };
    [inv(clear_linear[0]), inv(clear_linear[1]), inv(clear_linear[2]), clear_linear[3]]
}

/// One full-screen pass into `color` (from the graph: view, layout, load/store ops): begin
/// rendering, draw the triangle, run `extra` (e.g. the overlay) in the same rendering instance, end.
#[allow(clippy::too_many_arguments)]
pub fn fullscreen(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipeline: VkPipeline,
    layout: VkPipelineLayout,
    color: &VkRenderingAttachmentInfo,
    extent: VkExtent2D,
    push: &PostPush,
    extra: &mut dyn FnMut(VkCommandBuffer),
) {
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 1,
        p_color_attachments: color,
        p_depth_attachment: ptr::null(),
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: extent.width as f32, height: extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
    // SAFETY: `cmd` is recording outside a rendering instance; the pipeline renders one colour
    // attachment of `color`'s format with no depth; the global set is bound with `layout`; the
    // push range covers `PostPush`.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        (dev.fns.cmd_push_constants)(cmd, layout, PUSH_STAGES, 0, size_of::<PostPush>() as u32, push as *const PostPush as *const c_void);
        (dev.fns.cmd_draw)(cmd, 3, 1, 0, 0);
    }
    extra(cmd);
    // SAFETY: ends the rendering instance begun above.
    unsafe { (dev.fns.cmd_end_rendering)(cmd) };
}

/// Record post pass `p` of `g` (a bloom level, the tonemap or FXAA). The graph recorded its
/// barriers already. `extra` runs inside the pass's rendering instance (the inline overlay).
#[allow(clippy::too_many_arguments)]
pub fn record_post_pass(
    dev: &Device,
    cmd: VkCommandBuffer,
    g: &FrameGraph,
    p: usize,
    res: &FrameRes,
    pipelines: &FramePipelines,
    env: &Environment,
    frame_address: u64,
    extra: &mut dyn FnMut(VkCommandBuffer),
) {
    let layout = pipelines.layout;
    let push = |src: u32, src2: u32, size: VkExtent2D, mode: u32, params: [f32; 4]| PostPush {
        frame: frame_address,
        src,
        src2,
        texel: [1.0 / size.width as f32, 1.0 / size.height as f32],
        mode,
        pad: 0,
        params,
    };
    let extent = res.extent;
    let n = res.bloom_levels as usize;
    let slot = |r| g.image(r).slot;
    let b = &env.bloom;
    let none = &mut |_: VkCommandBuffer| {};
    match g.tag(p) {
        FramePass::BloomDown(i) => {
            let i = i as usize;
            let (threshold, knee) = (b.threshold.max(0.0), b.knee.max(0.0));
            let (src, size, first) =
                if i == 0 { (slot(res.hdr), extent, 1) } else { (slot(res.levels[i - 1]), bloom_level_size(extent, i as u32 - 1), 0) };
            let pc = push(src, NO_TEXTURE, size, first, [threshold, knee, 0.0, 0.0]);
            let out = g.color_attachment(p, res.levels[i], [0.0; 4]);
            fullscreen(dev, cmd, pipelines.bloom_down, layout, &out, bloom_level_size(extent, i as u32), &pc, none);
        }
        FramePass::BloomUp(i) => {
            // blur[i - 1] = tent(blur[i], or the smallest level) + level i - 1.
            let i = i as usize;
            let smaller = if i == n - 1 { res.levels[i] } else { res.blurs[i] };
            let pc = push(slot(smaller), slot(res.levels[i - 1]), bloom_level_size(extent, i as u32), 0, [1.0, 0.0, 0.0, 0.0]);
            let out = g.color_attachment(p, res.blurs[i - 1], [0.0; 4]);
            fullscreen(dev, cmd, pipelines.bloom_up, layout, &out, bloom_level_size(extent, i as u32 - 1), &pc, none);
        }
        FramePass::Tonemap => {
            let pc = if n > 0 {
                let blur = if n > 1 { res.blurs[0] } else { res.levels[0] };
                let intensity = b.intensity.max(0.0) / n as f32;
                push(slot(res.hdr), slot(blur), extent, 0, [intensity, 0.0, 0.0, 0.0])
            } else {
                push(slot(res.hdr), NO_TEXTURE, extent, 0, [0.0; 4]) // grading without bloom
            };
            let target = if res.fxaa { res.ldr } else { res.out };
            let out = g.color_attachment(p, target, [0.0; 4]);
            fullscreen(dev, cmd, pipelines.composite, layout, &out, extent, &pc, extra);
        }
        FramePass::Fxaa => {
            let pc = push(slot(res.ldr), NO_TEXTURE, extent, 0, [0.0; 4]);
            let out = g.color_attachment(p, res.out, [0.0; 4]);
            fullscreen(dev, cmd, pipelines.fxaa, layout, &out, extent, &pc, extra);
        }
        other => debug_assert!(false, "not a post pass: {other:?}"),
    }
}

/// Grading step bits (`FrameData::post_slots[3]`, `GRADE_*` in `shaders/composite.frag`; the
/// low seven come from [`GradingSettings::steps`]).
pub const GRADE_LUT: u32 = 16;
/// The LUT texture is `Rgba8Srgb` (the sampler decoded it: encode again).
pub const GRADE_LUT_SRGB: u32 = 128;

/// LMS gains of a white balance (the model of Unity's post stack): the white point moves along
/// the daylight locus by `temperature` (− bluer, + more amber: the image is corrected the other
/// way round, so + warms it) and off it by `tint` (+ a greener white point: the image turns
/// magenta), and each cone response is scaled to map that white to D65.
pub fn white_balance(temperature: f32, tint: f32) -> [f32; 3] {
    let (t1, t2) = (temperature as f64, tint as f64);
    let x = 0.31271 - t1 * if t1 < 0.0 { 0.1 } else { 0.05 };
    let standard_y = 2.87 * x - 3.0 * x * x - 0.275_095_07;
    let y = standard_y + t2 * 0.05;
    let lms = |x: f64, y: f64| {
        let (big_x, big_y, big_z) = (x / y, 1.0, (1.0 - x - y) / y);
        [
            0.7328 * big_x + 0.4296 * big_y - 0.1624 * big_z,
            -0.7036 * big_x + 1.6975 * big_y + 0.0061 * big_z,
            0.0030 * big_x + 0.0136 * big_y + 0.9834 * big_z,
        ]
    };
    let d65 = [0.949_237, 1.035_42, 1.087_28];
    let w = lms(x, y);
    [(d65[0] / w[0]) as f32, (d65[1] / w[1]) as f32, (d65[2] / w[2]) as f32]
}

/// The grading numbers of the frame data: balance (+ contrast), lift (+ saturation),
/// 1 / gamma (+ vibrance), gain (+ LUT strength), effects (vignette, radius, grain, unused).
pub fn grading_data(g: &GradingSettings) -> [[f32; 4]; 5] {
    let wb = white_balance(g.temperature.clamp(-1.0, 1.0), g.tint.clamp(-1.0, 1.0));
    let inv_gamma = g.gamma.map(|v| 1.0 / v.max(1e-3));
    [
        [wb[0], wb[1], wb[2], g.contrast.max(0.0)],
        [g.lift[0], g.lift[1], g.lift[2], g.saturation.max(0.0)],
        [inv_gamma[0], inv_gamma[1], inv_gamma[2], g.vibrance],
        [g.gain[0].max(0.0), g.gain[1].max(0.0), g.gain[2].max(0.0), g.lut_strength.clamp(0.0, 1.0)],
        [g.vignette.clamp(0.0, 1.0), g.vignette_radius.clamp(0.0, 0.99), g.grain.max(0.0), 0.0],
    ]
}

/// `FrameData::post_slots` of a frame: the LUT's slot and size, the SSAO image's slot, and the
/// grading steps to run (none unless the frame grades).
pub fn post_slots(env: &Environment, textures: &crate::overlay::Textures, ao_slot: u32, mode: PostMode) -> [u32; 4] {
    let mut slots = [crate::graph::NO_SLOT, 0, ao_slot, 0];
    if !mode.grading {
        return slots;
    }
    let mut steps = env.grading.steps();
    match env.grading.lut.and_then(|id| textures.map.get(&id.0)) {
        Some(t) if steps & GRADE_LUT != 0 => {
            slots[0] = t.slot;
            slots[1] = t.height;
            if t.format == crate::TextureFormat::Rgba8Srgb {
                steps |= GRADE_LUT_SRGB;
            }
        }
        _ => steps &= !GRADE_LUT,
    }
    slots[3] = steps;
    slots
}

/// Check the grading LUT, if one is set: a live `N² × N` RGBA8 texture.
pub fn check_lut(env: &Environment, textures: &crate::overlay::Textures) -> Result<(), crate::GpuError> {
    let g = &env.grading;
    let Some(id) = g.lut.filter(|_| g.enabled) else { return Ok(()) };
    let Some(t) = textures.map.get(&id.0) else {
        return Err(crate::GpuError::unknown(crate::IdKind::Texture, id.0, " (grading LUT)"));
    };
    let n = t.height;
    let rgba = matches!(t.format, crate::TextureFormat::Rgba8Unorm | crate::TextureFormat::Rgba8Srgb);
    if !rgba || !(2..=crate::MAX_LUT_SIZE).contains(&n) || t.width != n * n {
        return Err(crate::GpuError::rejected(
            crate::ErrorReason::BadLut { width: t.width, height: t.height },
            format!(
                "grading LUT texture {} is {}x{} {:?}; a 3D LUT is an RGBA8 strip N*N x N with N 2..={}",
                id.0,
                t.width,
                t.height,
                t.format,
                crate::MAX_LUT_SIZE
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aces(x: f32) -> f32 {
        ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
    }

    #[test]
    fn hdr_clear_inverts_the_tone_curve() {
        for y in [0.0, 0.002, 0.05, 0.2, 0.5, 0.9] {
            let x = hdr_clear([y, y, y, 1.0], true)[0];
            assert!((aces(x) - y).abs() < 1e-5, "{y} -> {x} -> {}", aces(x));
        }
        assert_eq!(hdr_clear([0.3, 0.2, 0.1, 1.0], false), [0.3, 0.2, 0.1, 1.0]);
    }

    #[test]
    fn bloom_levels_fit_the_target() {
        assert_eq!(bloom_level_count(VkExtent2D { width: 1600, height: 900 }, 5), 5);
        assert_eq!(bloom_level_count(VkExtent2D { width: 8, height: 4 }, 5), 2);
        assert_eq!(bloom_level_count(VkExtent2D { width: 1, height: 1 }, 5), 0);
    }

    #[test]
    fn modes() {
        let mut env = Environment::default();
        assert!(PostMode::of(&env).direct());
        env.bloom.enabled = true;
        env.bloom.levels = 99;
        assert_eq!(PostMode::of(&env).bloom_levels, MAX_BLOOM_LEVELS);
        env.bloom.enabled = false;
        env.fxaa = true;
        let m = PostMode::of(&env);
        assert!(!m.bloom() && !m.direct() && !m.hdr());
        env.fxaa = false;
        env.grading.enabled = true;
        assert!(PostMode::of(&env).direct(), "neutral grading adds no pass");
        env.grading.vignette = 0.3;
        let m = PostMode::of(&env);
        assert!(m.grading && m.hdr() && !m.direct());
    }

    #[test]
    fn white_balance_is_neutral_at_zero_and_warms_with_temperature() {
        let g = white_balance(0.0, 0.0);
        for c in g {
            assert!((c - 1.0).abs() < 2e-3, "{g:?}");
        }
        // Grey through the shader's LMS matrices (shaders/composite.frag).
        let apply = |wb: [f32; 3]| -> [f32; 3] {
            let rgb = [0.5f32, 0.5, 0.5];
            let to_lms = [
                [3.90405e-1, 5.49941e-1, 8.92632e-3],
                [7.08416e-2, 9.63172e-1, 1.35775e-3],
                [2.31082e-2, 1.28021e-1, 9.36245e-1],
            ];
            let to_rgb = [[2.85847, -1.62879, -2.48910e-2], [-2.10182e-1, 1.15820, 3.24281e-4], [-4.18120e-2, -1.18169e-1, 1.06867]];
            let lms: [f32; 3] = std::array::from_fn(|r| (0..3).map(|k| to_lms[r][k] * rgb[k]).sum::<f32>() * wb[r]);
            std::array::from_fn(|r| (0..3).map(|k| to_rgb[r][k] * lms[k]).sum())
        };
        let warm = apply(white_balance(0.5, 0.0));
        assert!(warm[0] > warm[2] + 0.02, "warmer: more red than blue {warm:?}");
        let cool = apply(white_balance(-0.5, 0.0));
        assert!(cool[2] > cool[0] + 0.02, "cooler {cool:?}");
        let magenta = apply(white_balance(0.0, 0.5));
        assert!(magenta[1] < magenta[0] && magenta[1] < magenta[2], "+tint is magenta {magenta:?}");
    }

    #[test]
    fn grading_steps_are_neutral_by_default() {
        let mut g = GradingSettings::default();
        assert_eq!(g.steps(), 0);
        g.enabled = true;
        assert!(!g.active());
        g.lut_strength = 0.5; // no LUT: still neutral
        assert!(!g.active());
        g.gamma = [1.0, 1.1, 1.0];
        assert_eq!(g.steps(), 8);
        let d = grading_data(&g);
        assert!((d[2][1] - 1.0 / 1.1).abs() < 1e-6);
    }
}
