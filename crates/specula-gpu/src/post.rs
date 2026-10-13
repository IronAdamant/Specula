//! The post chain: per-target intermediate images (HDR scene, bloom levels, the tone mapped image
//! FXAA reads) and the full-screen passes between the scene and the final image.
//!
//! Paths, by [`PostMode`]:
//! - nothing on: the scene renders straight into the final image, tone mapped in the forward
//!   pass (the M0/M1 path, bit for bit);
//! - FXAA only: scene (tone mapped) → LDR image → FXAA → final;
//! - bloom: scene (exposed linear) → HDR image → bloom chain → tonemap → final, or → LDR image
//!   → FXAA → final.

use crate::descriptors::Descriptors;
use crate::device::{image_barrier, Device, Image};
use crate::render::{FramePipelines, LDR_FORMAT, HDR_FORMAT, PUSH_STAGES};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{Environment, GpuError, MemoryKind, PASS_BLOOM, PASS_FXAA, PASS_TONEMAP};
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
}

impl PostMode {
    pub fn of(env: &Environment) -> PostMode {
        PostMode {
            bloom_levels: if env.bloom.enabled { env.bloom.levels.clamp(1, MAX_BLOOM_LEVELS) } else { 0 },
            fxaa: env.fxaa,
        }
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
        !self.bloom() && !self.fxaa
    }
}

/// An intermediate image and its slot in the bindless array.
pub struct PostImage {
    pub image: Image,
    pub slot: u32,
}

/// Bloom levels (level 0 = half the target's size): `levels` hold the downsampled bright pass,
/// `up[i]` the blur of everything from level `i` down. Every pass writes a fresh image (no
/// load-and-blend into an image read before: that corrupted on Intel, and it costs tile loads).
pub struct BloomChain {
    pub levels: Vec<PostImage>,
    /// One fewer than `levels` (the smallest level is its own blur).
    pub up: Vec<PostImage>,
    pub sizes: Vec<VkExtent2D>,
}

/// A target's post images, created on first use and kept while the size and settings hold.
#[derive(Default)]
pub struct PostTargets {
    extent: VkExtent2D,
    pub hdr: Option<PostImage>,
    pub ldr: Option<PostImage>,
    pub bloom: Option<BloomChain>,
}

/// Levels that fit `extent` (each at least 1×1), up to `wanted`.
pub fn bloom_level_count(extent: VkExtent2D, wanted: u32) -> u32 {
    let mut n = 0;
    while n < wanted && (extent.width.min(extent.height) >> (n + 1)) >= 1 {
        n += 1;
    }
    n
}

fn post_image(dev: &Device, descriptors: &mut Descriptors, format: VkFormat, extent: VkExtent2D, what: &str) -> Result<PostImage, GpuError> {
    let slot = descriptors.alloc_slot()?;
    let image = match dev.create_image(
        format,
        extent.width,
        extent.height,
        VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT,
        VK_IMAGE_ASPECT_COLOR_BIT,
        MemoryKind::PostTargets,
        what,
    ) {
        Ok(i) => i,
        Err(e) => {
            descriptors.free_slot(slot);
            return Err(e);
        }
    };
    descriptors.write_texture(dev, slot, image.view);
    Ok(PostImage { image, slot })
}

impl BloomChain {
    fn new(dev: &Device, descriptors: &mut Descriptors, extent: VkExtent2D, levels: u32) -> Result<BloomChain, GpuError> {
        let mut chain = BloomChain { levels: Vec::new(), up: Vec::new(), sizes: Vec::new() };
        for i in 1..=levels {
            let size = VkExtent2D { width: (extent.width >> i).max(1), height: (extent.height >> i).max(1) };
            let made = post_image(dev, descriptors, HDR_FORMAT, size, "bloom level").and_then(|down| {
                chain.levels.push(down);
                chain.sizes.push(size);
                if i < levels {
                    chain.up.push(post_image(dev, descriptors, HDR_FORMAT, size, "bloom blur")?);
                }
                Ok(())
            });
            if let Err(e) = made {
                chain.destroy(dev, descriptors);
                return Err(e);
            }
        }
        Ok(chain)
    }

    fn count(&self) -> u32 {
        self.levels.len() as u32
    }

    fn destroy(&mut self, dev: &Device, descriptors: &mut Descriptors) {
        for img in self.levels.drain(..).chain(self.up.drain(..)) {
            descriptors.free_slot(img.slot);
            dev.destroy_image(&img.image);
        }
        self.sizes.clear();
    }
}

impl PostTargets {
    /// Have the images `mode` needs at `extent`. Images that pending frames may still use are only
    /// destroyed (size or level count changed) after `wait_idle`.
    pub fn ensure(
        &mut self,
        dev: &Device,
        descriptors: &mut Descriptors,
        extent: VkExtent2D,
        mode: PostMode,
        mut wait_idle: impl FnMut(),
    ) -> Result<(), GpuError> {
        if mode.direct() {
            return Ok(());
        }
        let levels = mode.bloom_levels;
        let any = self.hdr.is_some() || self.ldr.is_some() || self.bloom.is_some();
        let chain_mismatch = self.bloom.as_ref().is_some_and(|c| mode.bloom() && c.count() != levels);
        if any && (self.extent != extent || chain_mismatch) {
            wait_idle();
            if self.extent != extent {
                self.destroy(dev, descriptors);
            } else if let Some(mut c) = self.bloom.take() {
                c.destroy(dev, descriptors);
            }
        }
        self.extent = extent;
        if mode.bloom() && self.hdr.is_none() {
            self.hdr = Some(post_image(dev, descriptors, HDR_FORMAT, extent, "HDR scene")?);
        }
        if mode.bloom() && self.bloom.is_none() && levels > 0 {
            self.bloom = Some(BloomChain::new(dev, descriptors, extent, levels)?);
        }
        if mode.fxaa && self.ldr.is_none() {
            self.ldr = Some(post_image(dev, descriptors, LDR_FORMAT, extent, "tone mapped image")?);
        }
        Ok(())
    }

    /// Release everything. No pending frame may use the images.
    pub fn destroy(&mut self, dev: &Device, descriptors: &mut Descriptors) {
        for img in [self.hdr.take(), self.ldr.take()].into_iter().flatten() {
            descriptors.free_slot(img.slot);
            dev.destroy_image(&img.image);
        }
        if let Some(mut c) = self.bloom.take() {
            c.destroy(dev, descriptors);
        }
    }
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

/// Move a colour image between layouts for the post chain.
pub fn color_barrier(dev: &Device, cmd: VkCommandBuffer, image: VkImage, to_read: bool) {
    let b = if to_read {
        image_barrier(
            image,
            VK_IMAGE_ASPECT_COLOR_BIT,
            VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
            VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
            VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
            VK_ACCESS_2_SHADER_SAMPLED_READ_BIT,
            VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
            VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
        )
    } else {
        // Contents discarded; ordered after earlier frames' reads and writes of it.
        image_barrier(
            image,
            VK_IMAGE_ASPECT_COLOR_BIT,
            VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT | VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
            VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
            VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
            VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT | VK_ACCESS_2_COLOR_ATTACHMENT_READ_BIT,
            VK_IMAGE_LAYOUT_UNDEFINED,
            VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
        )
    };
    dev.barrier(cmd, &[], &[b]);
}

/// One full-screen pass into `view`: begin rendering (keep or discard what is there), draw the
/// triangle, run `extra` (e.g. the overlay) in the same rendering instance, end.
#[allow(clippy::too_many_arguments)]
pub fn fullscreen(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipeline: VkPipeline,
    layout: VkPipelineLayout,
    view: VkImageView,
    extent: VkExtent2D,
    load: bool,
    push: &PostPush,
    extra: &mut dyn FnMut(VkCommandBuffer),
) {
    let color = VkRenderingAttachmentInfo {
        s_type: ST_RENDERING_ATTACHMENT_INFO,
        p_next: ptr::null(),
        image_view: view,
        image_layout: VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
        resolve_mode: VK_RESOLVE_MODE_NONE,
        resolve_image_view: VK_NULL_HANDLE,
        resolve_image_layout: VK_IMAGE_LAYOUT_UNDEFINED,
        load_op: if load { VK_ATTACHMENT_LOAD_OP_LOAD } else { VK_ATTACHMENT_LOAD_OP_DONT_CARE },
        store_op: VK_ATTACHMENT_STORE_OP_STORE,
        clear_value: VkClearValue::default(),
    };
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 1,
        p_color_attachments: &color,
        p_depth_attachment: ptr::null(),
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: extent.width as f32, height: extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
    // SAFETY: `cmd` is recording outside a rendering instance; the pipeline renders one colour
    // attachment of `view`'s format with no depth; the global set is bound with `layout`; the
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

/// Where the post chain ends.
pub struct FinalImage {
    pub view: VkImageView,
    pub extent: VkExtent2D,
}

/// Record the post chain after the scene was rendered into the HDR image (bloom) or the LDR image
/// (FXAA only). `last_pass` runs inside the final pass's rendering instance (the overlay).
#[allow(clippy::too_many_arguments)]
pub fn record_post(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipelines: &FramePipelines,
    env: &Environment,
    frame_address: u64,
    post: &PostTargets,
    mode: PostMode,
    out: &FinalImage,
    timer: Option<&mut PassTimer>,
    last_pass: &mut dyn FnMut(VkCommandBuffer),
) {
    let mut timer = timer;
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
    let none = &mut |_: VkCommandBuffer| {};
    if let (true, Some(hdr), Some(chain)) = (mode.bloom(), post.hdr.as_ref(), post.bloom.as_ref()) {
        let pass = timer.as_mut().and_then(|t| t.begin_pass(dev, cmd, PASS_BLOOM));
        color_barrier(dev, cmd, hdr.image.handle, true);
        let n = chain.levels.len();
        let b = &env.bloom;
        let (threshold, knee) = (b.threshold.max(0.0), b.knee.max(0.0));
        for i in 0..n {
            let (src, size, first) = if i == 0 { (hdr.slot, post.extent, 1) } else { (chain.levels[i - 1].slot, chain.sizes[i - 1], 0) };
            let p = push(src, NO_TEXTURE, size, first, [threshold, knee, 0.0, 0.0]);
            color_barrier(dev, cmd, chain.levels[i].image.handle, false);
            fullscreen(dev, cmd, pipelines.bloom_down, layout, chain.levels[i].image.view, chain.sizes[i], false, &p, none);
            color_barrier(dev, cmd, chain.levels[i].image.handle, true);
        }
        // up[i] = tent(up[i + 1], or the smallest level) + level i.
        for i in (1..n).rev() {
            let smaller = if i == n - 1 { &chain.levels[i] } else { &chain.up[i] };
            let out = &chain.up[i - 1];
            color_barrier(dev, cmd, out.image.handle, false);
            let p = push(smaller.slot, chain.levels[i - 1].slot, chain.sizes[i], 0, [1.0, 0.0, 0.0, 0.0]);
            fullscreen(dev, cmd, pipelines.bloom_up, layout, out.image.view, chain.sizes[i - 1], false, &p, none);
            color_barrier(dev, cmd, out.image.handle, true);
        }
        let blur = chain.up.first().unwrap_or(&chain.levels[0]);
        if let Some(t) = timer.as_mut() {
            t.end_pass(dev, cmd, pass);
        }

        let pass = timer.as_mut().and_then(|t| t.begin_pass(dev, cmd, PASS_TONEMAP));
        let intensity = b.intensity.max(0.0) / n as f32;
        let p = push(hdr.slot, blur.slot, post.extent, 0, [intensity, 0.0, 0.0, 0.0]);
        match (mode.fxaa, post.ldr.as_ref()) {
            (true, Some(ldr)) => {
                color_barrier(dev, cmd, ldr.image.handle, false);
                fullscreen(dev, cmd, pipelines.composite, layout, ldr.image.view, post.extent, false, &p, none);
            }
            _ => fullscreen(dev, cmd, pipelines.composite, layout, out.view, out.extent, false, &p, last_pass),
        }
        if let Some(t) = timer.as_mut() {
            t.end_pass(dev, cmd, pass);
        }
    }
    if let (true, Some(ldr)) = (mode.fxaa, post.ldr.as_ref()) {
        let pass = timer.as_mut().and_then(|t| t.begin_pass(dev, cmd, PASS_FXAA));
        color_barrier(dev, cmd, ldr.image.handle, true);
        let p = push(ldr.slot, NO_TEXTURE, post.extent, 0, [0.0; 4]);
        fullscreen(dev, cmd, pipelines.fxaa, layout, out.view, out.extent, false, &p, last_pass);
        if let Some(t) = timer {
            t.end_pass(dev, cmd, pass);
        }
    }
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
        assert!(!m.bloom() && !m.direct());
    }
}
