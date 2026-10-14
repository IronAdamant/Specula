//! Screen-space ambient occlusion ([`crate::SsaoSettings`]): three passes between the scene and
//! the tonemap pass (the occlusion and a two-pass blur), and a multiply inside the tonemap pass.
//!
//! 1. The scene (`shaders/mesh.frag`, `FEAT_SSAO`) renders to the HDR image as usual, and writes
//!    into its alpha — unused until now — the **exposed luminance of each pixel's ambient light**
//!    (ambient diffuse and the ambient/environment reflection, after fog; not the sun, point
//!    lights or emission). Blended layers attenuate it exactly as they attenuate the light behind
//!    them, `dst.a × (1 − src.a)`, and add none of their own (`Blend::PremultipliedKeepAlpha`);
//!    the sky and the background write 0. Its depth is stored and read afterwards.
//! 2. `ssao` (`shaders/ssao.frag`), at `resolution` × the frame (half by default: a quarter of
//!    the pixels): per pixel, the position from the scene's depth, the normal from the nearer of
//!    each pair of full-resolution neighbours (no normals target: tile GPUs would have to store
//!    one; perspective or orthographic projection, detected from the matrix), then `samples` taps on a screen-space spiral scaled to `radius` metres, rotated and
//!    scaled per pixel by interleaved gradient noise (Jimenez 2014: deterministic, pixel
//!    coordinates only, no repeating tile for the eye to lock onto). The estimator is McGuire's
//!    Scalable Ambient Obscurance (2012). Writes visibility + view depth (RG16F).
//! 3. `ssao_blur_x`, `ssao_blur_y` (`shaders/ssao_blur.frag`): a separable 9-tap Gaussian, each
//!    tap weighted by how far its depth is from the plane through this pixel (so occlusion doesn't
//!    bleed across edges, yet slanted surfaces still blur). Wide enough to average the noise into
//!    a smooth result that doesn't crawl when the camera moves (an earlier 4×4 rotation tile plus
//!    a 4×4 box left a mottle that did).
//! 4. The tonemap pass (`shaders/composite.frag`) upsamples the visibility bilinearly and scales
//!    only the ambient part: `c × (1 − (1 − visibility) × ambient / luminance(c))`. (A depth-aware
//!    upsample would read the full-resolution depth again, +0.35 ms on the Intel iGPU, to
//!    sharpen edges one occlusion texel wide; the blur before it is already depth-aware.)
//!
//! Why the ambient part only: occlusion is about light arriving from all around; direct light is
//! already occluded by shadows. A plain multiply of the final colour darkens lit faces and glowing
//! runes too (the usual "dirty" SSAO look). Why after the scene instead of before it: a depth
//! prepass for the occlusion repeats every opaque draw's vertex work (about 1 ms for the game
//! bench's 2,000 objects on the Intel iGPU) and needs the occlusion per fragment, overdraw
//! included; reading the scene's own depth and applying the occlusion once per pixel in the pass
//! that already reads the HDR image costs neither. The price: SSAO always takes the HDR path (a
//! 16-bit float scene image and the tonemap pass — free when bloom or grading is on), the scene's
//! depth is stored (a tile GPU writes it out once), and with blended layers over a crease the
//! split is approximate (a layer's own light is never occluded; what it covers is, in proportion).

use crate::graph::{ImageDesc, Load};
use crate::post::{fullscreen, PostPush};
use crate::render::{FrameContext, FrameGraph, FramePass, FrameRes};
use crate::vk::*;
use crate::{Environment, MemoryKind, MAX_SSAO_SAMPLES};

/// Visibility (r) and view depth in metres (g).
pub const AO_FORMAT: VkFormat = VK_FORMAT_R16G16_SFLOAT;

/// The occlusion's resolution as a fraction of the frame's.
fn scale_of(env: &Environment) -> f32 {
    if env.ssao.resolution.is_finite() {
        env.ssao.resolution.clamp(0.25, 1.0)
    } else {
        0.5
    }
}

/// The size of the occlusion images for a frame of `extent`, when SSAO is on.
pub fn ssao_extent(env: &Environment, extent: VkExtent2D) -> Option<VkExtent2D> {
    if !env.ssao.enabled {
        return None;
    }
    let k = scale_of(env);
    let scale = |n: u32| ((n as f32 * k).round() as u32).max(1);
    Some(VkExtent2D { width: scale(extent.width), height: scale(extent.height) })
}

/// Declare the three passes (after the scene, which wrote `r.depth`) and their images into `r`.
pub fn declare(g: &mut FrameGraph, r: &mut FrameRes, size: VkExtent2D) {
    let (w, h) = (size.width, size.height);
    r.ao_raw = g.transient("ao_raw", ImageDesc::color(AO_FORMAT, w, h, MemoryKind::PostTargets));
    r.ao_across = g.transient("ao_across", ImageDesc::color(AO_FORMAT, w, h, MemoryKind::PostTargets));
    r.ao = g.transient("ao", ImageDesc::color(AO_FORMAT, w, h, MemoryKind::PostTargets));
    g.pass("ssao", FramePass::Ssao).sampled(r.depth).color(r.ao_raw, Load::DontCare);
    g.pass("ssao_blur_x", FramePass::SsaoBlur(0)).sampled(r.ao_raw).color(r.ao_across, Load::DontCare);
    g.pass("ssao_blur_y", FramePass::SsaoBlur(1)).sampled(r.ao_across).color(r.ao, Load::DontCare);
}

/// Pixels per metre at view depth 1 m in an image `height` pixels tall: half the height times
/// the projection's vertical scale, which is the length of `view_proj`'s second row (its 3×3
/// part: the view's rotation is orthonormal).
pub fn projection_scale(view_proj: &[f32; 16], height: u32) -> f32 {
    let row = [view_proj[1], view_proj[5], view_proj[9]];
    0.5 * height as f32 * (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt()
}

/// NDC depth per metre if `view_proj` is orthographic (its clip w doesn't depend on the point:
/// the last row is (0, 0, 0, w)), else `None`.
pub fn orthographic_depth_scale(view_proj: &[f32; 16]) -> Option<f32> {
    let w_row = [view_proj[3], view_proj[7], view_proj[11]];
    let row = |r: usize| (view_proj[r] * view_proj[r] + view_proj[4 + r] * view_proj[4 + r] + view_proj[8 + r] * view_proj[8 + r]).sqrt();
    let scale = row(0).max(row(1)).max(row(2));
    let w = w_row.iter().map(|v| v.abs()).fold(0.0, f32::max);
    let depth = row(2);
    (scale > 0.0 && w <= 1e-6 * scale && depth > 0.0 && depth.is_finite()).then_some(depth)
}

/// Record the occlusion pass or its blur (`p`).
pub fn record_pass(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, res: &FrameRes) {
    let Some(size) = res.ssao else { return };
    let s = &ctx.frame.environment.ssao;
    let ortho = orthographic_depth_scale(&ctx.frame.view_proj);
    let radius = if s.radius.is_finite() { s.radius.clamp(0.01, 100.0) } else { 0.75 };
    let none = &mut |_: VkCommandBuffer| {};
    let (pipeline, src, out, texel, mode, pad, params) = match g.tag(p) {
        FramePass::Ssao => {
            // Works in the scene depth's (full resolution) pixels.
            let intensity = if s.intensity.is_finite() { s.intensity.clamp(0.0, 10.0) } else { 1.0 };
            let samples = s.samples.clamp(1, MAX_SSAO_SAMPLES);
            let mode = samples | (ctx.frame.reverse_z as u32) << 8;
            let full = res.extent;
            let scale = projection_scale(&ctx.frame.view_proj, full.height);
            let k = full.width as f32 / size.width as f32;
            let texel = [1.0 / full.width as f32, 1.0 / full.height as f32];
            let pad = ortho.map_or(0, f32::to_bits);
            (ctx.pipelines.ssao, res.depth, res.ao_raw, texel, mode, pad, [radius, intensity, scale, k])
        }
        FramePass::SsaoBlur(axis) => {
            let (src, out) = if axis == 0 { (res.ao_raw, res.ao_across) } else { (res.ao_across, res.ao) };
            // Orthographic depth counts from the near plane: an absolute tolerance (a quarter of
            // the radius) instead of the relative one.
            let tolerance = if ortho.is_some() { 0.25 * radius } else { 0.0 };
            let texel = [1.0 / size.width as f32, 1.0 / size.height as f32];
            (ctx.pipelines.ssao_blur, src, out, texel, axis as u32, 0, [tolerance, 0.0, 0.0, 0.0])
        }
        other => {
            debug_assert!(false, "not an SSAO pass: {other:?}");
            return;
        }
    };
    let push =
        PostPush { frame: ctx.upload.frame_address(), src: g.image(src).slot, src2: u32::MAX, texel, mode, pad, params };
    let color = g.color_attachment(p, out, [0.0; 4]);
    fullscreen(ctx.dev, cmd, pipeline, ctx.pipelines.layout, &color, size, &push, none);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_scales_and_is_off_by_default() {
        let mut env = Environment::default();
        let e = VkExtent2D { width: 1600, height: 900 };
        assert_eq!(ssao_extent(&env, e), None);
        env.ssao.enabled = true;
        assert_eq!(ssao_extent(&env, e), Some(VkExtent2D { width: 800, height: 450 }));
        env.ssao.resolution = 0.01; // clamped to a quarter
        assert_eq!(ssao_extent(&env, e), Some(VkExtent2D { width: 400, height: 225 }));
        env.ssao.resolution = f32::NAN;
        assert_eq!(ssao_extent(&env, VkExtent2D { width: 1, height: 1 }), Some(VkExtent2D { width: 1, height: 1 }));
    }

    #[test]
    fn orthographic_projections_are_recognised() {
        // Reversed-Z orthographic, 20 m tall view, near 0.1, far 100.1: depth 1 at near, 0 at far.
        let (h, n, f) = (10.0f32, 0.1f32, 100.1f32);
        let mut ortho = [0.0f32; 16];
        ortho[0] = 1.0 / h;
        ortho[5] = -1.0 / h;
        ortho[10] = 1.0 / (f - n);
        ortho[14] = f / (f - n);
        ortho[15] = 1.0;
        let scale = orthographic_depth_scale(&ortho).expect("orthographic");
        assert!((scale - 0.01).abs() < 1e-6, "{scale}");
        let persp = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0];
        assert_eq!(orthographic_depth_scale(&persp), None);
        assert_eq!(orthographic_depth_scale(&[0.0; 16]), None);
    }

    #[test]
    fn projection_scale_is_pixels_per_metre_at_one_metre() {
        // 90° vertical field of view: at 1 m the image spans 2 m, so 450 px per metre for 900 px.
        let f = 1.0f32; // 1 / tan(45°)
        let vp = [f, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0];
        assert!((projection_scale(&vp, 900) - 450.0).abs() < 1e-3);
    }
}
