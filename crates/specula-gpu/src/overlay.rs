//! Textures (uploaded images in the bindless array) and overlay quads (HUD, text, panels) drawn
//! over the finished frame.

use crate::device::{Device, Image};
use crate::render::{FrameUpload, LinearCache, PushConstants, PUSH_STAGES};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{
    ErrorReason, GpuError, IdKind, Limit, OverlayKind, OverlayQuad, TextureFormat, TextureId, MAX_OVERLAY_FEATHER, MAX_OVERLAY_QUADS,
    PASS_OVERLAY,
};
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
    /// Mip levels (1 for overlay textures).
    pub levels: u32,
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
        return Err(GpuError::rejected(ErrorReason::ZeroSize { width, height }, format!("{what}: size {width}x{height} must not be 0")));
    }
    let want = width as u64 * height as u64 * format.bytes_per_pixel() as u64;
    if pixels.len() as u64 != want {
        return Err(GpuError::rejected(
            ErrorReason::DataLength { given: pixels.len() as u64, expected: want },
            format!("{what}: {} bytes given; {width}x{height} {format:?} needs {want}", pixels.len()),
        ));
    }
    Ok(())
}

/// One quad or triangle as the overlay shaders read it; must match `Quad` in
/// `shaders/overlay.glsl`. Colours are linear RGB with straight alpha.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverlayRecord {
    /// Quad: x, y, width, height. Triangle: p0.xy, p1.xy.
    pub rect: [f32; 4],
    /// Quad: u0, v0, u1, v1. Triangle: p2.xy, 0, 0.
    pub uv: [f32; 4],
    /// Quad: fill at the top. Triangle: colour of p0.
    pub color: [f32; 4],
    /// Quad: fill at the bottom. Triangle: colour of p1.
    pub color2: [f32; 4],
    /// Quad: border at the top. Triangle: colour of p2.
    pub border0: [f32; 4],
    /// Quad: border at the bottom.
    pub border1: [f32; 4],
    /// Corner radius, border width, feather, unused.
    pub params: [f32; 4],
    /// Clip x0, y0, x1, y1 (whole pixels).
    pub clip: [f32; 4],
    /// Texture slot + 1 (0 = none), kind (`KIND_*`), unused ×2.
    pub info: [u32; 4],
}

pub const OVERLAY_RECORD_SIZE: usize = size_of::<OverlayRecord>();

/// `OverlayRecord::info[1]`.
pub const KIND_RGBA: u32 = 0;
pub const KIND_ALPHA_MASK: u32 = 1;
pub const KIND_TRIANGLE: u32 = 2;

/// "No clip": far beyond any target.
const NO_CLIP: [f32; 4] = [-1.0e9, -1.0e9, 1.0e9, 1.0e9];

/// Fail early: too many quads, or a quad names a texture that does not exist.
pub fn check_overlay(overlay: &[OverlayQuad], textures: &Textures) -> Result<(), GpuError> {
    if overlay.len() > MAX_OVERLAY_QUADS {
        return Err(GpuError::limit(Limit::OverlayQuads, overlay.len() as u64, MAX_OVERLAY_QUADS as u64));
    }
    match overlay.iter().find_map(|q| q.texture.filter(|t| textures.slot(*t).is_none())) {
        Some(t) => Err(GpuError::unknown(IdKind::Texture, t.0, " in overlay")),
        None => Ok(()),
    }
}

/// The shader-side records of `overlay` (colours to linear, textures to slots) into `out`.
pub fn build_overlay(overlay: &[OverlayQuad], textures: &Textures, cache: &mut LinearCache, out: &mut Vec<OverlayRecord>) {
    out.clear();
    out.extend(overlay.iter().map(|q| {
        let finite = |v: [f32; 4]| v.map(|x| if x.is_finite() { x } else { 0.0 });
        let num = |x: f32, lo: f32, hi: f32| if x.is_finite() { x.clamp(lo, hi) } else { lo };
        let mut lin = |c: [f32; 4]| {
            let l = cache.get([c[0], c[1], c[2]]);
            [l[0], l[1], l[2], if c[3].is_nan() { 0.0 } else { c[3].clamp(0.0, 1.0) }]
        };
        // Clip edges snap to whole pixels, as the software preview does.
        let clip = match q.clip {
            Some(c) if c.iter().all(|v| v.is_finite()) => [c[0].round(), c[1].round(), (c[0] + c[2]).round(), (c[1] + c[3]).round()],
            Some(_) => [0.0; 4],
            None => NO_CLIP,
        };
        if let Some(t) = q.triangle {
            let p = t.points.map(|p| p.map(|v| if v.is_finite() { v } else { 0.0 }));
            return OverlayRecord {
                rect: [p[0][0], p[0][1], p[1][0], p[1][1]],
                uv: [p[2][0], p[2][1], 0.0, 0.0],
                color: lin(t.colors[0]),
                color2: lin(t.colors[1]),
                border0: lin(t.colors[2]),
                border1: [0.0; 4],
                params: [0.0; 4],
                clip,
                info: [0, KIND_TRIANGLE, 0, 0],
            };
        }
        let top = lin(q.color);
        let bottom = q.color_bottom.map_or(top, &mut lin);
        OverlayRecord {
            rect: finite(q.rect),
            uv: finite(q.uv),
            color: top,
            color2: bottom,
            border0: lin(q.border_color[0]),
            border1: lin(q.border_color[1]),
            params: [num(q.radius, 0.0, 1.0e6), num(q.border, 0.0, 1.0e6), num(q.feather, 1.0, MAX_OVERLAY_FEATHER), 0.0],
            clip,
            info: [
                q.texture.and_then(|t| textures.slot(t)).map_or(0, |s| s + 1),
                if q.kind == OverlayKind::AlphaMask { KIND_ALPHA_MASK } else { KIND_RGBA },
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
    let pc = PushConstants { frame: draw.frame, draws: draw.quads, material: 0, pass: 0, bones: 0 };
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
        assert_eq!(offset_of!(OverlayRecord, params), 96);
        assert_eq!(offset_of!(OverlayRecord, clip), 112);
        assert_eq!(offset_of!(OverlayRecord, info), 128);
        assert_eq!(OVERLAY_RECORD_SIZE, 144);
        assert_eq!(overlay_units(0, 112), 0);
        assert_eq!(overlay_units(7, 112), 9);
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
        assert_eq!(out[0].params, [0.0, 0.0, 1.0, 0.0], "radius clamped, feather at least 1");
        assert_eq!(out[0].info, [0, 0, 0, 0]);
        assert_eq!(out[0].color2, out[0].color, "flat fill");
        // Gradient, border, feather, clip, triangle.
        let q = OverlayQuad {
            color_bottom: Some([0.0, 0.0, 0.0, 1.0]),
            border: 2.0,
            border_color: [[1.0; 4], [0.0, 0.0, 0.0, 0.5]],
            feather: 1.0e9,
            clip: Some([10.4, 20.6, 30.0, 40.0]),
            ..OverlayQuad::solid([0.0, 0.0, 10.0, 10.0], [1.0; 4])
        };
        let t = OverlayQuad::triangle([[1.0, 2.0], [3.0, 4.0], [5.0, f32::NAN]], [[1.0; 4], [0.5; 4], [0.0; 4]]);
        build_overlay(&[q, t], &textures, &mut LinearCache::default(), &mut out);
        assert_eq!(out[0].color2, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(out[0].border1[3], 0.5);
        assert_eq!(out[0].params[1..3], [2.0, MAX_OVERLAY_FEATHER]);
        assert_eq!(out[0].clip, [10.0, 21.0, 40.0, 61.0]);
        assert_eq!(out[1].info[1], KIND_TRIANGLE);
        assert_eq!((out[1].rect, out[1].uv), ([1.0, 2.0, 3.0, 4.0], [5.0, 0.0, 0.0, 0.0]));
        assert!((out[1].color2[0] - 0.214_041).abs() < 1e-5, "vertex colours linear");
        let bad = [OverlayQuad { texture: Some(TextureId(42)), ..Default::default() }];
        let err = check_overlay(&bad, &textures).unwrap_err();
        assert_eq!(err.reason, Some(ErrorReason::UnknownId { kind: IdKind::Texture, id: 42 }));
        assert!(err.message.contains("42"));
        let err = check_pixels("t", 2, 2, TextureFormat::Rgba8Srgb, &[0; 3]).unwrap_err();
        assert_eq!(err.reason, Some(ErrorReason::DataLength { given: 3, expected: 16 }));
        let err = check_pixels("t", 0, 2, TextureFormat::R8, &[]).unwrap_err();
        assert_eq!(err.reason, Some(ErrorReason::ZeroSize { width: 0, height: 2 }));
    }
}
