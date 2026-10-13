//! Textures (uploaded images in the bindless array) and overlay quads (HUD, text, panels) drawn
//! over the finished frame.

use crate::device::{Device, Image};
use crate::render::{FrameUpload, LinearCache, PushConstants, PUSH_STAGES};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{GpuError, Limit, OverlayKind, OverlayQuad, TextureFormat, TextureId, MAX_OVERLAY_QUADS, PASS_OVERLAY};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;

pub fn vk_format(f: TextureFormat) -> VkFormat {
    match f {
        TextureFormat::R8 => VK_FORMAT_R8_UNORM,
        TextureFormat::Rgba8Srgb => VK_FORMAT_R8G8B8A8_SRGB,
        TextureFormat::Rgba8Unorm => VK_FORMAT_R8G8B8A8_UNORM,
    }
}

pub struct Texture {
    pub image: Image,
    pub slot: u32,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
}

/// Live textures by id.
#[derive(Default)]
pub struct Textures {
    pub map: HashMap<u32, Texture>,
}

impl Textures {
    pub fn slot(&self, id: TextureId) -> Option<u32> {
        self.map.get(&id.0).map(|t| t.slot)
    }
}

/// Check pixel data for a `width × height` region of `format`.
pub fn check_pixels(what: &str, width: u32, height: u32, format: TextureFormat, pixels: &[u8]) -> Result<(), GpuError> {
    if width == 0 || height == 0 {
        return Err(GpuError::new(format!("{what}: size {width}x{height} must not be 0")));
    }
    let want = width as u64 * height as u64 * format.bytes_per_pixel() as u64;
    if pixels.len() as u64 != want {
        return Err(GpuError::new(format!(
            "{what}: {} bytes given; {width}x{height} {format:?} needs {want}",
            pixels.len()
        )));
    }
    Ok(())
}

/// One quad as the overlay shaders read it; must match `Quad` in `shaders/overlay.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverlayRecord {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    /// Linear RGB, straight alpha.
    pub color: [f32; 4],
    /// Corner radius, unused ×3.
    pub params: [f32; 4],
    /// Texture slot + 1 (0 = none), kind (1 = alpha mask), unused ×2.
    pub info: [u32; 4],
}

pub const OVERLAY_RECORD_SIZE: usize = size_of::<OverlayRecord>();

/// Fail early: too many quads, or a quad names a texture that does not exist.
pub fn check_overlay(overlay: &[OverlayQuad], textures: &Textures) -> Result<(), GpuError> {
    if overlay.len() > MAX_OVERLAY_QUADS {
        return Err(GpuError::limit(Limit::OverlayQuads, overlay.len() as u64, MAX_OVERLAY_QUADS as u64));
    }
    match overlay.iter().find_map(|q| q.texture.filter(|t| textures.slot(*t).is_none())) {
        Some(t) => Err(GpuError::new(format!("unknown texture id {} in overlay", t.0))),
        None => Ok(()),
    }
}

/// The shader-side records of `overlay` (colours to linear, textures to slots) into `out`.
pub fn build_overlay(overlay: &[OverlayQuad], textures: &Textures, cache: &mut LinearCache, out: &mut Vec<OverlayRecord>) {
    out.clear();
    out.extend(overlay.iter().map(|q| {
        let c = cache.get([q.color[0], q.color[1], q.color[2]]);
        let finite = |v: [f32; 4]| v.map(|x| if x.is_finite() { x } else { 0.0 });
        OverlayRecord {
            rect: finite(q.rect),
            uv: finite(q.uv),
            color: [c[0], c[1], c[2], if q.color[3].is_nan() { 0.0 } else { q.color[3].clamp(0.0, 1.0) }],
            params: [if q.radius.is_finite() { q.radius.max(0.0) } else { 0.0 }, 0.0, 0.0, 0.0],
            info: [
                q.texture.and_then(|t| textures.slot(t)).map_or(0, |s| s + 1),
                (q.kind == OverlayKind::AlphaMask) as u32,
                0,
                0,
            ],
        }
    }));
}

/// Record units (of `record_size` bytes) that `quads` overlay records take in the upload buffer.
pub fn overlay_units(quads: usize, record_size: usize) -> usize {
    (quads * OVERLAY_RECORD_SIZE).div_ceil(record_size)
}

/// What the last pass needs to draw the overlay inside its rendering instance.
#[derive(Clone, Copy)]
pub struct OverlayDraw {
    pub pipeline: VkPipeline,
    pub layout: VkPipelineLayout,
    pub frame: u64,
    pub quads: u64,
    pub count: u32,
}

/// Draw the overlay (inside a rendering instance whose viewport covers the target), timed as its
/// own pass.
pub fn record_overlay(dev: &Device, cmd: VkCommandBuffer, draw: &OverlayDraw, timer: Option<&mut PassTimer>) {
    if draw.count == 0 || draw.pipeline == VK_NULL_HANDLE {
        return;
    }
    let mut timer = timer;
    let pass = timer.as_mut().and_then(|t| t.begin_pass(dev, cmd, PASS_OVERLAY));
    let pc = PushConstants { frame: draw.frame, draws: draw.quads, material: 0, pass: 0 };
    // SAFETY: `cmd` is recording inside a rendering instance compatible with the pipeline; the
    // global set is bound with `layout`; `quads` holds `count` records in the frame's upload buffer.
    unsafe {
        (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, draw.pipeline);
        (dev.fns.cmd_push_constants)(
            cmd,
            draw.layout,
            PUSH_STAGES,
            0,
            size_of::<PushConstants>() as u32,
            &pc as *const PushConstants as *const c_void,
        );
        (dev.fns.cmd_draw)(cmd, 6, draw.count, 0, 0);
    }
    if let Some(t) = timer {
        t.end_pass(dev, cmd, pass);
    }
}

impl FrameUpload {
    /// Device address of the overlay records that follow `records` draw records.
    pub fn overlay_address(&self, records: usize) -> u64 {
        self.records_address() + (records * size_of::<crate::render::DrawRecord>()) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn record_matches_shader_layout() {
        assert_eq!(offset_of!(OverlayRecord, color), 32);
        assert_eq!(offset_of!(OverlayRecord, info), 64);
        assert_eq!(OVERLAY_RECORD_SIZE, 80);
        assert_eq!(overlay_units(0, 112), 0);
        assert_eq!(overlay_units(7, 112), 5);
    }

    #[test]
    fn records_convert_colours_and_slots() {
        let textures = Textures::default();
        let quads = [OverlayQuad { radius: -3.0, color: [1.0, 0.5, 0.0, 2.0], ..OverlayQuad::solid([1.0, 2.0, 3.0, f32::NAN], [0.0; 4]) }];
        let mut out = Vec::new();
        build_overlay(&quads, &textures, &mut LinearCache::default(), &mut out);
        assert_eq!(out[0].rect, [1.0, 2.0, 3.0, 0.0], "non-finite becomes 0");
        assert_eq!(out[0].color[3], 1.0, "alpha clamped");
        assert!((out[0].color[1] - 0.214_041).abs() < 1e-5, "linear");
        assert_eq!(out[0].params[0], 0.0);
        assert_eq!(out[0].info, [0, 0, 0, 0]);
        let bad = [OverlayQuad { texture: Some(TextureId(42)), ..Default::default() }];
        assert!(check_overlay(&bad, &textures).unwrap_err().message.contains("42"));
    }
}
