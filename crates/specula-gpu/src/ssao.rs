//! Screen-space ambient occlusion ([`crate::SsaoSettings`]): three passes before the scene.
//!
//! 1. `ao_depth`: the opaque (`Lit`) batches, depth only, at the occlusion's resolution
//!    (`resolution` × the frame; half by default: a quarter of the pixels). Its own depth image,
//!    so the scene pass is untouched: it still clears and writes its own depth (no load, nothing
//!    stored: tile GPUs keep it on chip), and needs no exact depth match with this pass.
//! 2. `ssao` (`shaders/ssao.frag`): per pixel, the position from depth, the normal from the
//!    nearer of each pair of neighbours (no normals target: tile GPUs would have to store one),
//!    then `samples` taps on a screen-space spiral scaled to `radius` metres, rotated per pixel
//!    by a 4×4 interleaved pattern (deterministic: pixel coordinates only). The estimator is
//!    McGuire's Scalable Ambient Obscurance (2012). Writes occlusion + view depth (RG16F).
//! 3. `ssao_blur` (`shaders/ssao_blur.frag`): a 4×4 blur (exactly one period of the pattern)
//!    whose taps are weighted by depth similarity, so occlusion doesn't bleed across edges.
//!
//! The scene pass (`shaders/mesh.frag`, `FEAT_SSAO`, opaque pipeline only) upsamples it with
//! depth-aware bilinear weights and scales **only the ambient terms** (ambient diffuse and the
//! ambient/environment reflection), leaving the sun, point lights and emission as they were.
//!
//! Why there, and not a multiply in post: occlusion is about light arriving from all around;
//! direct light is already occluded by shadows. Multiplying the final colour would darken lit
//! faces and glowing runes too (the usual "dirty" SSAO look) and needs the HDR path. The cost of
//! doing it right is the depth-only pass (vertex work again for opaque geometry, at a reduced
//! pixel count) instead of reusing the scene's depth; in exchange the main pass keeps its
//! load/store-free depth and its exact output when SSAO is off (the feature is specialized out).

use crate::graph::{ImageDesc, Load};
use crate::post::{fullscreen, PostPush};
use crate::render::{FrameContext, FrameGraph, FramePass, FrameRes, PushConstants, DEPTH_FORMAT, PUSH_STAGES};
use crate::vk::*;
use crate::{Environment, MemoryKind, MAX_SSAO_SAMPLES};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

/// Occlusion (r) and view depth in metres (g).
pub const AO_FORMAT: VkFormat = VK_FORMAT_R16G16_SFLOAT;

/// The size of the occlusion images for a frame of `extent`, when SSAO is on.
pub fn ssao_extent(env: &Environment, extent: VkExtent2D) -> Option<VkExtent2D> {
    if !env.ssao.enabled {
        return None;
    }
    let k = if env.ssao.resolution.is_finite() { env.ssao.resolution.clamp(0.25, 1.0) } else { 0.5 };
    let scale = |n: u32| ((n as f32 * k).round() as u32).max(1);
    Some(VkExtent2D { width: scale(extent.width), height: scale(extent.height) })
}

/// Declare the three passes (before the scene) and their images into `r`.
pub fn declare(g: &mut FrameGraph, r: &mut FrameRes, size: VkExtent2D) {
    let (w, h) = (size.width, size.height);
    let depth = ImageDesc { bindless: true, ..ImageDesc::depth(DEPTH_FORMAT, w, h, MemoryKind::PostTargets) };
    r.ao_depth = g.transient("ao_depth", depth);
    r.ao_raw = g.transient("ao_raw", ImageDesc::color(AO_FORMAT, w, h, MemoryKind::PostTargets));
    r.ao = g.transient("ao", ImageDesc::color(AO_FORMAT, w, h, MemoryKind::PostTargets));
    g.pass("ao_depth", FramePass::AoDepth).depth(r.ao_depth, Load::Clear);
    g.pass("ssao", FramePass::Ssao).sampled(r.ao_depth).color(r.ao_raw, Load::DontCare);
    g.pass("ssao_blur", FramePass::SsaoBlur).sampled(r.ao_raw).color(r.ao, Load::DontCare);
}

/// Pixels per metre at view depth 1 m in an image `height` pixels tall: half the height times
/// the projection's vertical scale, which is the length of `view_proj`'s second row (its 3×3
/// part: the view's rotation is orthonormal).
pub fn projection_scale(view_proj: &[f32; 16], height: u32) -> f32 {
    let row = [view_proj[1], view_proj[5], view_proj[9]];
    0.5 * height as f32 * (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt()
}

/// Record the depth-only pass `p`: every opaque batch of the frame's plan.
pub fn record_depth(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, res: &FrameRes) {
    let Some(size) = res.ssao else { return };
    let dev = ctx.dev;
    let frame = ctx.frame;
    let depth = g.depth_attachment(p, res.ao_depth, if frame.reverse_z { 0.0 } else { 1.0 }, None);
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: size };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 0,
        p_color_attachments: ptr::null(),
        p_depth_attachment: &depth,
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: size.width as f32, height: size.height as f32, min_depth: 0.0, max_depth: 1.0 };
    let pc = PushConstants { frame: ctx.upload.frame_address(), draws: ctx.upload.records_address(), material: 0, pass: 0 };
    // SAFETY: `cmd` is recording outside a rendering instance; the pipeline is depth-only for
    // DEPTH_FORMAT with a dynamic compare op; every mesh in a batch is live (`check_frame`) and
    // the plan's records are in `upload`.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, ctx.pipelines.ao_depth);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        (dev.fns.cmd_set_depth_compare_op)(cmd, if frame.reverse_z { VK_COMPARE_OP_GREATER } else { VK_COMPARE_OP_LESS });
        (dev.fns.cmd_push_constants)(
            cmd,
            ctx.pipelines.layout,
            PUSH_STAGES,
            0,
            size_of::<PushConstants>() as u32,
            &pc as *const PushConstants as *const c_void,
        );
        let mut bound = None;
        for b in ctx.scratch.plan.batches.iter().filter(|b| !b.material.blended()) {
            let Some(mesh) = ctx.meshes.get(&b.mesh.0) else { continue };
            if bound != Some(b.mesh) {
                let offset: VkDeviceSize = 0;
                (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &mesh.buffer.handle, &offset);
                (dev.fns.cmd_bind_index_buffer)(cmd, mesh.buffer.handle, mesh.index_offset, VK_INDEX_TYPE_UINT32);
                bound = Some(b.mesh);
            }
            (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, b.count, 0, 0, b.first);
        }
        (dev.fns.cmd_end_rendering)(cmd);
    }
}

/// Record the occlusion pass or its blur (`p`).
pub fn record_pass(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, res: &FrameRes) {
    let Some(size) = res.ssao else { return };
    let s = &ctx.frame.environment.ssao;
    let texel = [1.0 / size.width as f32, 1.0 / size.height as f32];
    let none = &mut |_: VkCommandBuffer| {};
    let (pipeline, src, out, pc) = match g.tag(p) {
        FramePass::Ssao => {
            let radius = if s.radius.is_finite() { s.radius.clamp(0.01, 100.0) } else { 0.75 };
            let intensity = if s.intensity.is_finite() { s.intensity.clamp(0.0, 10.0) } else { 1.0 };
            let samples = s.samples.clamp(1, MAX_SSAO_SAMPLES);
            let mode = samples | (ctx.frame.reverse_z as u32) << 8;
            let scale = projection_scale(&ctx.frame.view_proj, size.height);
            (ctx.pipelines.ssao, res.ao_depth, res.ao_raw, (mode, [radius, intensity, scale, 0.0]))
        }
        _ => (ctx.pipelines.ssao_blur, res.ao_raw, res.ao, (0, [0.0; 4])),
    };
    let push = PostPush {
        frame: ctx.upload.frame_address(),
        src: g.image(src).slot,
        src2: u32::MAX,
        texel,
        mode: pc.0,
        pad: 0,
        params: pc.1,
    };
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
    fn projection_scale_is_pixels_per_metre_at_one_metre() {
        // 90° vertical field of view: at 1 m the image spans 2 m, so 450 px per metre for 900 px.
        let f = 1.0f32; // 1 / tan(45°)
        let vp = [f, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0];
        assert!((projection_scale(&vp, 900) - 450.0).abs() < 1e-3);
    }
}
