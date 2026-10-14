//! The mesh pipelines: shaders, pipeline creation and caching, per-frame data, per-draw records,
//! the frame's render graph (`declare_frame`, `record_frame`) and scene recording (instanced batches).

use crate::batch::{Bounds, Plan};
use crate::descriptors::Descriptors;
use crate::device::{Buffer, Device};
use crate::graph::{Final, ImageDesc, Load, Prior, RenderGraph, Res, TransientPool};
use crate::overlay::{build_overlay, check_overlay, overlay_units, record_overlay, OverlayDraw, OverlayRecord, Textures};
use crate::post::{
    bloom_level_size, hdr_clear, record_post_pass, PostMode, BLOOM_BLURS, BLOOM_DOWN_PASSES, BLOOM_LEVELS, BLOOM_UP_PASSES,
    MAX_BLOOM_LEVELS,
};
use crate::shadow::{plan_shadows, ShadowData, ShadowPlan, SHADOW_FORMAT};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{
    CacheRejection, DrawItem, DrawStats, FrameDesc, GpuError, Limit, Material, MemoryKind, PipelineCacheLoad, PipelineStats,
    Vertex, MAX_POINT_LIGHTS, MAX_SHADOW_CASCADES, PASS_BLOOM, PASS_FXAA, PASS_OVERLAY, PASS_SCENE, PASS_SHADOW, PASS_TONEMAP,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{offset_of, size_of};
use std::ptr;
use std::time::Instant;

/// Every shader module, by name (for errors) and SPIR-V; [`Shader`] indexes this table.
static SHADERS: [(&str, &[u8]); 14] = [
    ("mesh.vert", include_bytes!("../shaders/mesh.vert.spv")),
    ("mesh.frag", include_bytes!("../shaders/mesh.frag.spv")),
    ("shadow.vert", include_bytes!("../shaders/shadow.vert.spv")),
    ("fullscreen.vert", include_bytes!("../shaders/fullscreen.vert.spv")),
    ("bloom_down.frag", include_bytes!("../shaders/bloom_down.frag.spv")),
    ("bloom_up.frag", include_bytes!("../shaders/bloom_up.frag.spv")),
    ("composite.frag", include_bytes!("../shaders/composite.frag.spv")),
    ("fxaa.frag", include_bytes!("../shaders/fxaa.frag.spv")),
    ("sky.vert", include_bytes!("../shaders/sky.vert.spv")),
    ("sky.frag", include_bytes!("../shaders/sky.frag.spv")),
    ("overlay.vert", include_bytes!("../shaders/overlay.vert.spv")),
    ("overlay.frag", include_bytes!("../shaders/overlay.frag.spv")),
    ("ssao.frag", include_bytes!("../shaders/ssao.frag.spv")),
    ("ssao_blur.frag", include_bytes!("../shaders/ssao_blur.frag.spv")),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shader {
    MeshVert = 0,
    MeshFrag = 1,
    ShadowVert = 2,
    FullscreenVert = 3,
    BloomDownFrag = 4,
    BloomUpFrag = 5,
    CompositeFrag = 6,
    FxaaFrag = 7,
    SkyVert = 8,
    SkyFrag = 9,
    OverlayVert = 10,
    OverlayFrag = 11,
    SsaoFrag = 12,
    SsaoBlurFrag = 13,
}

/// Offscreen targets and the tone mapped image FXAA reads (readback bytes are sRGB RGBA8).
pub const LDR_FORMAT: VkFormat = VK_FORMAT_R8G8B8A8_SRGB;

pub const DEPTH_FORMAT: VkFormat = VK_FORMAT_D32_SFLOAT;

/// Bits of the scene shaders' specialization constant 0 (`FEATURES` in `shaders/mesh.frag`).
/// Specialized away when off, so a frame without the feature runs exactly the code it ran before
/// the feature existed (bit-identical output, no cost).
pub const FEAT_SHADOWS: u32 = 1;
pub const FEAT_FOG: u32 = 2;
/// Output linear, exposed HDR for the post chain instead of tone mapped display values. Implied by
/// rendering into [`HDR_FORMAT`].
pub const FEAT_HDR: u32 = 4;
/// The opaque pipeline: Lit only (the other materials' code is specialized away).
pub const FEAT_OPAQUE: u32 = 8;
/// The blended pipeline: no Lit path (see the Intel note at `tint` in `shaders/mesh.frag`).
pub const FEAT_BLENDED: u32 = 16;
/// Screen-space ambient occlusion: the HDR scene's alpha carries the fraction of each pixel's
/// light that is ambient (the tonemap pass scales that part by the occlusion); see `ssao.rs`.
pub const FEAT_SSAO: u32 = 32;
/// Feature combinations built for every colour format.
const FEATURE_SETS: [u32; 4] = [0, FEAT_SHADOWS, FEAT_FOG, FEAT_SHADOWS | FEAT_FOG];

/// Push constant bytes every pipeline layout reserves (the minimum every device supports).
const PUSH_BYTES: u32 = 128;

/// Push constants shared by both stages, set once per batch; must match the `Push` block in
/// `shaders/mesh.*`. Far below the 128 bytes every device guarantees.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PushConstants {
    /// Device address of this frame's [`FrameData`].
    pub frame: u64,
    /// Device address of this frame's [`DrawRecord`] array; an instance reads record
    /// `gl_InstanceIndex` (the batch's first record + instance).
    pub draws: u64,
    /// [`Material`] as `MAT_*` in the shader (one per batch).
    pub material: u32,
    /// Glass: 0 = back faces, 1 = front faces.
    pub pass: u32,
}

/// Per-draw data as the shaders read it; must match `Draw` in `shaders/mesh.*` (std430).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawRecord {
    /// Object → world, column-major.
    pub model: [f32; 16],
    /// Linear RGB; a = opacity.
    pub color: [f32; 4],
    /// Linear RGB secondary colour; a = seed.
    pub color2: [f32; 4],
    /// emissive, metallic, roughness, specular.
    pub params: [f32; 4],
}

/// One point light as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuPointLight {
    /// xyz world position, w range (m).
    pub pos_range: [f32; 4],
    /// Linear RGB × intensity; w unused.
    pub color: [f32; 4],
}

/// Per-frame data read through a buffer device address; must match `Frame` in `shaders/mesh.*`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameData {
    pub view_proj: [f32; 16],
    /// xyz camera position, w time in seconds.
    pub camera_time: [f32; 4],
    /// xyz direction towards the sun (normalized), w unused.
    pub sun_dir: [f32; 4],
    /// Linear RGB × intensity; w unused.
    pub sun_color: [f32; 4],
    /// Linear RGB × intensity; w exposure.
    pub ambient: [f32; 4],
    /// Linear clear colour; w = 1 for tone mapping.
    pub sky: [f32; 4],
    /// Width, height (pixels), point light count, unused.
    pub viewport: [f32; 4],
    pub lights: [GpuPointLight; MAX_POINT_LIGHTS],
    /// Inverse of `view_proj` (identity if singular).
    pub inv_view_proj: [f32; 16],
    /// World → shadow clip per cascade.
    pub shadow_mats: [[f32; 16]; MAX_SHADOW_CASCADES as usize],
    /// Cascades in use, filter radius (texels of cascade 0), normal offset (texels), unused.
    pub shadow_params: [f32; 4],
    /// World size of a texel per cascade.
    pub shadow_texel: [f32; 4],
    /// View depth = xyz·p + w.
    pub shadow_axis: [f32; 4],
    /// View depths where shadows start and finish fading out; unused.
    pub shadow_fade: [f32; 4],
    /// Linear RGB; w density per metre at the reference height.
    pub fog_color: [f32; 4],
    /// Falloff per metre, reference height, start distance, sun scatter.
    pub fog_params: [f32; 4],
    /// Linear RGB × intensity; w disc radius (radians, 0 = none).
    pub sky_zenith: [f32; 4],
    /// Linear RGB × intensity; w disc intensity.
    pub sky_horizon: [f32; 4],
    /// Linear RGB × intensity; w glow strength.
    pub sky_ground: [f32; 4],
    /// Horizon sharpness, unused ×3.
    pub sky_params: [f32; 4],
}

pub const FRAME_DATA_SIZE: u64 = size_of::<FrameData>() as u64;
/// Where the draw records start in a frame's upload buffer (frame data first, 256-aligned).
pub const RECORDS_OFFSET: u64 = FRAME_DATA_SIZE.div_ceil(256) * 256;
const RECORD_SIZE: u64 = size_of::<DrawRecord>() as u64;
/// Records a fresh upload buffer has room for.
const INITIAL_RECORDS: u32 = 256;

// Records follow the frame data at an aligned offset; push constants stay within the 128 bytes
// every device guarantees.
const _: () = assert!(RECORDS_OFFSET >= FRAME_DATA_SIZE && RECORDS_OFFSET % 256 == 0);
const _: () = assert!(size_of::<PushConstants>() <= 128);

const MAT_LIT: u32 = 0;
const MAT_GLASS: u32 = 1;
const MAT_BLOB_SHADOW: u32 = 2;
const MAT_LAVA_LAMP: u32 = 3;

fn material_id(m: Material) -> u32 {
    match m {
        Material::Lit => MAT_LIT,
        Material::Glass => MAT_GLASS,
        Material::BlobShadow => MAT_BLOB_SHADOW,
        Material::LavaLamp => MAT_LAVA_LAMP,
    }
}

/// Pack `frame` (and its shadow cascades) for the shaders.
pub fn frame_data(frame: &FrameDesc, extent: VkExtent2D, shadow: &ShadowData) -> FrameData {
    let env = &frame.environment;
    let lin = |c: [f32; 3], k: f32| {
        let l = srgb_to_linear_rgba([c[0], c[1], c[2], 1.0]);
        [l[0] * k, l[1] * k, l[2] * k, 0.0]
    };
    let d = env.sun_direction;
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let sun_dir = if len > 1e-6 && len.is_finite() { [d[0] / len, d[1] / len, d[2] / len, 0.0] } else { [0.0, 1.0, 0.0, 0.0] };
    let mut ambient = lin(env.ambient_color, env.ambient_intensity.max(0.0));
    ambient[3] = env.exposure.max(0.0);
    let mut sky = srgb_to_linear_rgba(frame.clear_color);
    sky[3] = if env.tonemap { 1.0 } else { 0.0 };
    let mut lights = [GpuPointLight::default(); MAX_POINT_LIGHTS];
    for (slot, l) in lights.iter_mut().zip(frame.lights) {
        let p = l.position;
        *slot = GpuPointLight { pos_range: [p[0], p[1], p[2], l.range.max(1e-3)], color: lin(l.color, l.intensity.max(0.0)) };
    }
    let c = camera_position(&frame.view_proj);
    let (fog, sky_s) = (&env.fog, &env.sky);
    let sky_k = sky_s.intensity.max(0.0);
    const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    FrameData {
        view_proj: frame.view_proj,
        camera_time: [c[0], c[1], c[2], frame.time_s],
        sun_dir,
        sun_color: lin(env.sun_color, env.sun_intensity.max(0.0)),
        ambient,
        sky,
        viewport: [extent.width as f32, extent.height as f32, frame.lights.len().min(MAX_POINT_LIGHTS) as f32, 0.0],
        lights,
        inv_view_proj: crate::shadow::invert(&frame.view_proj).unwrap_or(IDENTITY),
        shadow_mats: shadow.matrices,
        shadow_params: [shadow.count as f32, shadow.softness, shadow.normal_offset, 0.0],
        shadow_texel: shadow.texel,
        shadow_axis: shadow.axis,
        shadow_fade: [shadow.fade[0], shadow.fade[1], 0.0, 0.0],
        fog_color: {
            let mut c = lin(fog.color, 1.0);
            c[3] = fog.density.max(0.0);
            c
        },
        fog_params: [fog.falloff.max(0.0), fog.height, fog.start_distance.max(0.0), fog.sun_scatter.max(0.0)],
        sky_zenith: {
            let mut c = lin(sky_s.zenith, sky_k);
            c[3] = sky_s.disc_size_deg.clamp(0.0, 45.0).to_radians();
            c
        },
        sky_horizon: {
            let mut c = lin(sky_s.horizon, sky_k);
            c[3] = sky_s.disc_intensity.max(0.0);
            c
        },
        sky_ground: {
            let mut c = lin(sky_s.ground, sky_k);
            c[3] = sky_s.glow.max(0.0);
            c
        },
        sky_params: [sky_s.horizon_sharpness.clamp(0.1, 64.0), 0.0, 0.0, 0.0],
    }
}

/// The camera's world position: the point that `view_proj` maps to clip x = y = w = 0. Falls back
/// to the origin for a singular or orthographic matrix.
pub fn camera_position(view_proj: &[f32; 16]) -> [f32; 3] {
    // Solve view_proj · p = (0, 0, 1, 0) by Gaussian elimination with partial pivoting (f64).
    let mut a = [[0.0f64; 5]; 4];
    for (r, row) in a.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().take(4).enumerate() {
            *cell = view_proj[c * 4 + r] as f64;
        }
        row[4] = if r == 2 { 1.0 } else { 0.0 };
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs())).unwrap_or(col);
        if a[pivot][col].abs() < 1e-12 {
            return [0.0; 3];
        }
        a.swap(col, pivot);
        let pivot_row = a[col];
        for (r, row) in a.iter_mut().enumerate() {
            if r != col {
                let f = row[col] / pivot_row[col];
                for (cell, p) in row.iter_mut().zip(pivot_row).skip(col) {
                    *cell -= f * p;
                }
            }
        }
    }
    let p: [f64; 4] = std::array::from_fn(|r| a[r][4] / a[r][r]);
    if p[3].abs() < 1e-12 || !p.iter().all(|v| v.is_finite()) {
        return [0.0; 3];
    }
    [(p[0] / p[3]) as f32, (p[1] / p[3]) as f32, (p[2] / p[3]) as f32]
}

/// The shader-side record of a draw (colours converted to linear, parameters clamped).
#[cfg(test)]
pub fn draw_record(draw: &DrawItem) -> DrawRecord {
    draw_record_with(draw, &mut |rgb| rgb.map(srgb_to_linear))
}

fn draw_record_with(draw: &DrawItem, linear: &mut impl FnMut([f32; 3]) -> [f32; 3]) -> DrawRecord {
    let s = &draw.surface;
    let c = linear([draw.color[0], draw.color[1], draw.color[2]]);
    let c2 = linear(s.color2);
    DrawRecord {
        model: draw.model,
        color: [c[0], c[1], c[2], draw.color[3]],
        color2: [c2[0], c2[1], c2[2], s.seed],
        params: [s.emissive.max(0.0), s.metallic.clamp(0.0, 1.0), s.roughness.clamp(0.0, 1.0), s.specular.max(0.0)],
    }
}

/// FxHash-style hasher for small fixed keys (no DoS concern: keys are our own colours).
#[derive(Default)]
pub struct FastHasher(u64);

impl std::hash::Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }

    fn write_u32(&mut self, n: u32) {
        self.write_u64(n as u64);
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// sRGB → linear conversions remembered across frames, keyed by the exact input bits, so the
/// result is bit-identical to converting afresh (`powf` is slow: ~100 ns on some C runtimes, three
/// per draw). Bounded: cleared when full.
#[derive(Default)]
pub struct LinearCache {
    map: HashMap<[u32; 3], [f32; 3], std::hash::BuildHasherDefault<FastHasher>>,
}

impl LinearCache {
    const MAX_ENTRIES: usize = 1 << 16;

    pub fn get(&mut self, rgb: [f32; 3]) -> [f32; 3] {
        let key = rgb.map(f32::to_bits);
        if let Some(&v) = self.map.get(&key) {
            return v;
        }
        if self.map.len() >= Self::MAX_ENTRIES {
            self.map.clear();
        }
        let v = rgb.map(srgb_to_linear);
        self.map.insert(key, v);
        v
    }
}

/// Fill `out` with the records of `plan.entries`, in order. Runs of equal looks (common in
/// batches) convert once; other colours come from `cache`.
pub fn build_records(frame: &FrameDesc, plan: &Plan, cache: &mut LinearCache, out: &mut Vec<DrawRecord>) {
    out.clear();
    out.reserve(plan.entries.len());
    let mut last: Option<(&DrawItem, DrawRecord)> = None;
    for &(i, _) in &plan.entries {
        let d = &frame.draws[i];
        let record = match last {
            Some((prev, r)) if prev.color == d.color && prev.surface == d.surface => DrawRecord { model: d.model, ..r },
            _ => draw_record_with(d, &mut |rgb| cache.get(rgb)),
        };
        last = Some((d, record));
        out.push(record);
    }
}

/// Append one record per shadow caster entry (only the model matters to the shadow pass).
pub fn append_shadow_records(frame: &FrameDesc, shadow: &ShadowPlan, out: &mut Vec<DrawRecord>) {
    out.reserve(shadow.entries.len());
    out.extend(shadow.entries.iter().map(|&i| DrawRecord {
        model: frame.draws[i].model,
        color: [0.0; 4],
        color2: [0.0; 4],
        params: [0.0; 4],
    }));
}

/// Reusable CPU-side scratch for preparing frames: once warm, preparing a frame allocates nothing.
#[derive(Default)]
pub struct FrameScratch {
    pub plan: Plan,
    pub shadow: ShadowPlan,
    pub records: Vec<DrawRecord>,
    pub overlay: Vec<OverlayRecord>,
    pub linear: LinearCache,
}

impl FrameScratch {
    /// Index of the first shadow caster record (they follow the camera's records).
    pub fn shadow_first_record(&self) -> u32 {
        self.plan.entries.len() as u32
    }
}

/// Most draw records one frame may hold: instance indices are `u32`.
pub const MAX_DRAW_RECORDS: u64 = u32::MAX as u64;

/// How an upload buffer's capacity (in records) follows demand: grow at once (at least doubling),
/// shrink only after [`UploadSizing::SHRINK_AFTER`] consecutive uses needing at most a quarter of
/// it, to twice the largest need seen meanwhile (never below the initial size). So a one-off spike
/// doesn't pin memory forever, and a steady load never reallocates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadSizing {
    pub capacity: u32,
    quiet_uses: u32,
    quiet_peak: u32,
}

impl UploadSizing {
    pub const SHRINK_AFTER: u32 = 300;

    pub fn new(capacity: u32) -> UploadSizing {
        UploadSizing { capacity, quiet_uses: 0, quiet_peak: 0 }
    }

    /// Account one use needing `records`; the capacity to reallocate to, if it should change.
    /// Errors (structured) beyond [`MAX_DRAW_RECORDS`].
    pub fn next(&mut self, records: usize) -> Result<Option<u32>, GpuError> {
        if records as u64 > MAX_DRAW_RECORDS {
            return Err(GpuError::limit(Limit::DrawRecords, records as u64, MAX_DRAW_RECORDS));
        }
        let records = records as u32;
        if records > self.capacity {
            self.quiet_uses = 0;
            self.quiet_peak = 0;
            return Ok(Some(records.max(self.capacity.saturating_mul(2))));
        }
        if self.capacity <= INITIAL_RECORDS || records > self.capacity / 4 {
            self.quiet_uses = 0;
            self.quiet_peak = 0;
            return Ok(None);
        }
        self.quiet_uses += 1;
        self.quiet_peak = self.quiet_peak.max(records);
        if self.quiet_uses < Self::SHRINK_AFTER {
            return Ok(None);
        }
        let target = self.quiet_peak.saturating_mul(2).max(INITIAL_RECORDS);
        self.quiet_uses = 0;
        self.quiet_peak = 0;
        Ok((target < self.capacity).then_some(target))
    }
}

/// Create with `preferred` memory flags (device-local: fast GPU reads on ReBAR and integrated
/// GPUs), retrying with none if that fails, e.g. when a 256 MB BAR heap is exhausted.
fn with_device_local_fallback<T>(mut create: impl FnMut(VkFlags) -> Result<T, GpuError>) -> Result<T, GpuError> {
    create(VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT).or_else(|_| create(0))
}

/// A frame's upload buffer: [`FrameData`] at offset 0, [`DrawRecord`]s at [`RECORDS_OFFSET`].
/// Host-visible (device-local when possible), persistently mapped, read by the shaders through
/// its device address. Sized by [`UploadSizing`].
pub struct FrameUpload {
    buffer: Buffer,
    address: u64,
    sizing: UploadSizing,
}

impl FrameUpload {
    pub fn new(dev: &Device) -> Result<FrameUpload, GpuError> {
        let (buffer, address) = Self::create(dev, INITIAL_RECORDS)?;
        Ok(FrameUpload { buffer, address, sizing: UploadSizing::new(INITIAL_RECORDS) })
    }

    fn create(dev: &Device, capacity: u32) -> Result<(Buffer, u64), GpuError> {
        let buffer = with_device_local_fallback(|preferred| {
            dev.create_buffer(
                RECORDS_OFFSET + capacity as u64 * RECORD_SIZE,
                VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT,
                VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                preferred,
                MemoryKind::PerFrame,
                "frame upload",
            )
        })?;
        let address = dev.buffer_address(&buffer);
        Ok((buffer, address))
    }

    /// Make room for `records` this frame, growing or (after a long quiet spell) shrinking the
    /// buffer. The GPU must not be reading this buffer (callers waited for the frame that last
    /// used it). A failed shrink keeps the current buffer; a failed doubling retries at exactly
    /// the size needed.
    pub fn reserve(&mut self, dev: &Device, records: usize) -> Result<(), GpuError> {
        let Some(capacity) = self.sizing.next(records)? else { return Ok(()) };
        let shrinking = capacity < self.sizing.capacity;
        let created = Self::create(dev, capacity).map(|b| (b, capacity)).or_else(|e| {
            let exact = records as u32; // ≤ MAX_DRAW_RECORDS, checked by `next`
            if shrinking {
                Err(e)
            } else if exact < capacity {
                Self::create(dev, exact).map(|b| (b, exact))
            } else {
                Err(e)
            }
        });
        match created {
            Ok(((buffer, address), capacity)) => {
                dev.destroy_buffer(&self.buffer);
                self.buffer = buffer;
                self.address = address;
                self.sizing.capacity = capacity;
                Ok(())
            }
            Err(_) if shrinking => Ok(()), // the current buffer is still big enough
            Err(e) => Err(e),
        }
    }

    /// Write the frame's data, records and overlay quads (after the records), all within the
    /// reserved capacity.
    pub fn write(&self, data: &FrameData, records: &[DrawRecord], overlay: &[OverlayRecord]) {
        let units = records.len() + overlay_units(overlay.len(), RECORD_SIZE as usize);
        assert!(units <= self.sizing.capacity as usize, "reserve() before write()");
        // SAFETY: `mapped` is coherent host memory of RECORDS_OFFSET + capacity × RECORD_SIZE bytes
        // that the GPU no longer reads (see `reserve`); both copies stay in bounds and the types
        // are plain `repr(C)` data.
        unsafe {
            let base = self.buffer.mapped as *mut u8;
            ptr::copy_nonoverlapping(data as *const FrameData as *const u8, base, size_of::<FrameData>());
            ptr::copy_nonoverlapping(
                records.as_ptr() as *const u8,
                base.add(RECORDS_OFFSET as usize),
                std::mem::size_of_val(records),
            );
            ptr::copy_nonoverlapping(
                overlay.as_ptr() as *const u8,
                base.add(RECORDS_OFFSET as usize + std::mem::size_of_val(records)),
                std::mem::size_of_val(overlay),
            );
        }
    }

    pub fn frame_address(&self) -> u64 {
        self.address
    }

    pub fn records_address(&self) -> u64 {
        self.address + RECORDS_OFFSET
    }

    pub fn bytes(&self) -> u64 {
        self.buffer.bytes
    }

    pub fn destroy(&self, dev: &Device) {
        dev.destroy_buffer(&self.buffer);
    }
}

pub const PUSH_STAGES: VkFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;

/// An uploaded mesh: vertices at offset 0, `u32` indices at `index_offset`, one device-local buffer.
pub struct Mesh {
    pub buffer: Buffer,
    pub index_offset: u64,
    pub index_count: u32,
    /// Object-space bounding sphere, for frustum culling.
    pub bounds: Bounds,
}

/// Identity of the device a pipeline cache blob must come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheIdentity {
    pub vendor_id: u32,
    pub device_id: u32,
    pub uuid: [u8; VK_UUID_SIZE],
}

/// Check a `VkPipelineCacheHeaderVersionOne` (all fields little-endian) against this device, so a
/// blob from another GPU or driver is never handed to the driver.
pub fn check_cache_header(blob: &[u8], id: &CacheIdentity) -> Result<(), CacheRejection> {
    if blob.len() < 32 {
        return Err(CacheRejection::TooShort { len: blob.len() });
    }
    let word = |i: usize| u32::from_le_bytes([blob[i], blob[i + 1], blob[i + 2], blob[i + 3]]);
    let (header_size, header_version) = (word(0), word(4));
    if header_version != VK_PIPELINE_CACHE_HEADER_VERSION_ONE || header_size < 32 || header_size as usize > blob.len() {
        return Err(CacheRejection::BadHeader { header_size, header_version });
    }
    if word(8) != id.vendor_id {
        return Err(CacheRejection::OtherVendor { vendor_id: word(8) });
    }
    if word(12) != id.device_id {
        return Err(CacheRejection::OtherDevice { device_id: word(12) });
    }
    if blob[16..32] != id.uuid {
        return Err(CacheRejection::OtherDriver);
    }
    Ok(())
}

/// One graphics pipeline. All of them are built up front ([`Pipelines::prebuild`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineKey {
    /// Scene draws into `format` (+ D32 depth, compare op dynamic): opaque or blended, with the
    /// specialization constant `features` (`FEAT_*`).
    Mesh { format: VkFormat, blended: bool, features: u32 },
    /// Depth-only shadow casters into a D16 layer.
    Shadow,
    /// Bloom downsample (and bright pass) into an HDR level.
    BloomDown,
    /// Bloom upsample: the blur below, tent-filtered, plus this level.
    BloomUp,
    /// HDR (+ bloom) → display values in `format`.
    Composite { format: VkFormat },
    /// FXAA into `format`.
    Fxaa { format: VkFormat },
    /// The sky behind the scene in `format` (+ D32 depth, EQUAL far plane), with or without fog.
    Sky { format: VkFormat, fog: bool },
    /// Overlay quads into `format`, inside the scene pass (`depth`: its D32 attachment, unused)
    /// or the last post pass (no depth).
    Overlay { format: VkFormat, depth: bool },
    /// Ambient occlusion (+ view depth) from the SSAO depth.
    Ssao,
    /// The depth-aware 4×4 blur of the occlusion.
    SsaoBlur,
}

/// What vertex data a pipeline reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VertexInput {
    /// None (full-screen triangles, overlay quads).
    None,
    /// [`Vertex`]: position, normal, colour.
    Mesh,
    /// [`Vertex`]'s position only.
    Position,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blend {
    Off,
    /// dst = src + dst × (1 − src.a).
    Premultiplied,
    /// Colour as `Premultiplied`; alpha (SSAO's ambient fraction) dst.a × (1 − src.a): what a
    /// blended layer covers is less occludable.
    PremultipliedKeepAlpha,
}

/// How a pipeline is built.
#[derive(Clone, Copy, Debug)]
struct PipelineDesc {
    vert: Shader,
    frag: Option<Shader>,
    /// Specialization constant 0.
    spec: u32,
    vertex: VertexInput,
    color: Option<VkFormat>,
    depth: Option<VkFormat>,
    depth_test: bool,
    depth_write: bool,
    /// `None` = dynamic (`vkCmdSetDepthCompareOp`).
    compare: Option<i32>,
    blend: Blend,
    depth_bias: bool,
}

impl PipelineKey {
    fn desc(self) -> PipelineDesc {
        match self {
            PipelineKey::Mesh { format, blended, features } => PipelineDesc {
                vert: Shader::MeshVert,
                frag: Some(Shader::MeshFrag),
                spec: features | if format == HDR_FORMAT { FEAT_HDR } else { 0 } | if blended { FEAT_BLENDED } else { FEAT_OPAQUE },
                vertex: VertexInput::Mesh,
                color: Some(format),
                depth: Some(DEPTH_FORMAT),
                depth_test: true,
                // Blended surfaces never hide what is behind them.
                depth_write: !blended,
                compare: None,
                blend: match (blended, features & FEAT_SSAO != 0) {
                    (false, _) => Blend::Off,
                    (true, false) => Blend::Premultiplied,
                    (true, true) => Blend::PremultipliedKeepAlpha,
                },
                depth_bias: false,
            },
            PipelineKey::Shadow => PipelineDesc {
                vert: Shader::ShadowVert,
                frag: None,
                spec: 0,
                vertex: VertexInput::Position,
                color: None,
                depth: Some(SHADOW_FORMAT),
                depth_test: true,
                depth_write: true,
                compare: Some(VK_COMPARE_OP_LESS),
                blend: Blend::Off,
                depth_bias: true,
            },
            PipelineKey::BloomDown => fullscreen(Shader::BloomDownFrag, HDR_FORMAT, Blend::Off),
            PipelineKey::BloomUp => fullscreen(Shader::BloomUpFrag, HDR_FORMAT, Blend::Off),
            PipelineKey::Composite { format } => fullscreen(Shader::CompositeFrag, format, Blend::Off),
            PipelineKey::Fxaa { format } => fullscreen(Shader::FxaaFrag, format, Blend::Off),
            PipelineKey::Sky { format, fog } => PipelineDesc {
                vert: Shader::SkyVert,
                frag: Some(Shader::SkyFrag),
                spec: if fog { FEAT_FOG } else { 0 } | if format == HDR_FORMAT { FEAT_HDR } else { 0 },
                vertex: VertexInput::None,
                color: Some(format),
                depth: Some(DEPTH_FORMAT),
                depth_test: true,
                depth_write: false,
                compare: Some(VK_COMPARE_OP_EQUAL),
                blend: Blend::Off,
                depth_bias: false,
            },
            PipelineKey::Overlay { format, depth } => PipelineDesc {
                vert: Shader::OverlayVert,
                frag: Some(Shader::OverlayFrag),
                spec: 0,
                vertex: VertexInput::None,
                color: Some(format),
                depth: depth.then_some(DEPTH_FORMAT),
                depth_test: false,
                depth_write: false,
                compare: Some(VK_COMPARE_OP_LESS),
                blend: Blend::Premultiplied,
                depth_bias: false,
            },
            PipelineKey::Ssao => fullscreen(Shader::SsaoFrag, crate::ssao::AO_FORMAT, Blend::Off),
            PipelineKey::SsaoBlur => fullscreen(Shader::SsaoBlurFrag, crate::ssao::AO_FORMAT, Blend::Off),
        }
    }
}

/// A full-screen triangle pass into one colour attachment, no depth.
fn fullscreen(frag: Shader, format: VkFormat, blend: Blend) -> PipelineDesc {
    PipelineDesc {
        vert: Shader::FullscreenVert,
        frag: Some(frag),
        spec: 0,
        vertex: VertexInput::None,
        color: Some(format),
        depth: None,
        depth_test: false,
        depth_write: false,
        compare: Some(VK_COMPARE_OP_LESS),
        blend,
        depth_bias: false,
    }
}

/// The colour format of HDR scene targets (bloom input).
pub const HDR_FORMAT: VkFormat = VK_FORMAT_R16G16B16A16_SFLOAT;

/// Shader modules, the pipeline layout (global descriptor set + push constants), the pipeline
/// cache and every pipeline.
pub struct Pipelines {
    modules: Vec<VkShaderModule>,
    pub layout: VkPipelineLayout,
    cache: VkPipelineCache,
    by_key: Vec<(PipelineKey, VkPipeline)>,
    created: u32,
    /// `created` when `mark_warm` was called.
    warm_at: Option<u32>,
    create_ms: f64,
    cache_load: PipelineCacheLoad,
}

/// The pipelines one frame uses; `VK_NULL_HANDLE` for passes the frame doesn't run.
#[derive(Clone, Copy)]
pub struct FramePipelines {
    pub layout: VkPipelineLayout,
    pub opaque: VkPipeline,
    pub blended: VkPipeline,
    pub shadow: VkPipeline,
    pub sky: VkPipeline,
    pub bloom_down: VkPipeline,
    pub bloom_up: VkPipeline,
    pub composite: VkPipeline,
    pub fxaa: VkPipeline,
    pub overlay: VkPipeline,
    pub ssao: VkPipeline,
    pub ssao_blur: VkPipeline,
}

fn spirv_words(bytes: &[u8]) -> Vec<u32> {
    bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn create_shader(dev: &Device, spv: &[u8], what: &str) -> Result<VkShaderModule, GpuError> {
    let words = spirv_words(spv);
    let info = VkShaderModuleCreateInfo {
        s_type: ST_SHADER_MODULE_CREATE_INFO,
        p_next: ptr::null(),
        flags: 0,
        code_size: words.len() * 4,
        p_code: words.as_ptr(),
    };
    let mut module = VK_NULL_HANDLE;
    // SAFETY: `words` is 4-byte aligned SPIR-V that outlives the call.
    let r = unsafe { (dev.fns.create_shader_module)(dev.handle, &info, ptr::null(), &mut module) };
    if r != VK_SUCCESS {
        return Err(GpuError::vk(&format!("vkCreateShaderModule ({what})"), r));
    }
    Ok(module)
}

fn create_cache(dev: &Device, data: &[u8]) -> Result<VkPipelineCache, VkResult> {
    let info = VkPipelineCacheCreateInfo {
        s_type: ST_PIPELINE_CACHE_CREATE_INFO,
        p_next: ptr::null(),
        flags: 0,
        initial_data_size: data.len(),
        p_initial_data: if data.is_empty() { ptr::null() } else { data.as_ptr() as *const c_void },
    };
    let mut cache = VK_NULL_HANDLE;
    // SAFETY: valid device; `data` (header checked by the caller) outlives the call.
    let r = unsafe { (dev.fns.create_pipeline_cache)(dev.handle, &info, ptr::null(), &mut cache) };
    if r == VK_SUCCESS {
        Ok(cache)
    } else {
        Err(r)
    }
}

impl Pipelines {
    /// Shaders, layout (over `set_layout`) and the pipeline cache, seeded from `cache_blob` when it
    /// was made by this device and driver (the outcome is in [`Pipelines::stats`]). No pipelines
    /// yet: see [`Pipelines::prebuild`].
    pub fn new(
        dev: &Device,
        set_layout: VkDescriptorSetLayout,
        cache_blob: Option<&[u8]>,
        id: &CacheIdentity,
    ) -> Result<Pipelines, GpuError> {
        let mut p = Pipelines {
            modules: Vec::new(),
            layout: VK_NULL_HANDLE,
            cache: VK_NULL_HANDLE,
            by_key: Vec::new(),
            created: 0,
            warm_at: None,
            create_ms: 0.0,
            cache_load: PipelineCacheLoad::NotProvided,
        };
        let result = (|| {
            for (name, spv) in SHADERS {
                p.modules.push(create_shader(dev, spv, name)?);
            }
            let range = VkPushConstantRange { stage_flags: PUSH_STAGES, offset: 0, size: PUSH_BYTES };
            let info = VkPipelineLayoutCreateInfo {
                s_type: ST_PIPELINE_LAYOUT_CREATE_INFO,
                p_next: ptr::null(),
                flags: 0,
                set_layout_count: 1,
                p_set_layouts: &set_layout,
                push_constant_range_count: 1,
                p_push_constant_ranges: &range,
            };
            // SAFETY: valid device and create info.
            let r = unsafe { (dev.fns.create_pipeline_layout)(dev.handle, &info, ptr::null(), &mut p.layout) };
            if r != VK_SUCCESS {
                return Err(GpuError::vk("vkCreatePipelineLayout", r));
            }
            let seeded = match cache_blob {
                None => None,
                Some(blob) => match check_cache_header(blob, id) {
                    Err(why) => {
                        p.cache_load = PipelineCacheLoad::Rejected(why);
                        None
                    }
                    Ok(()) => match create_cache(dev, blob) {
                        Ok(cache) => {
                            p.cache_load = PipelineCacheLoad::Loaded { bytes: blob.len() };
                            Some(cache)
                        }
                        Err(r) => {
                            p.cache_load = PipelineCacheLoad::Rejected(CacheRejection::Driver { vk_result: r });
                            None
                        }
                    },
                },
            };
            p.cache = match seeded {
                Some(cache) => cache,
                None => create_cache(dev, &[]).map_err(|r| GpuError::vk("vkCreatePipelineCache", r))?,
            };
            Ok(())
        })();
        match result {
            Ok(()) => Ok(p),
            Err(e) => {
                p.destroy(dev);
                Err(e)
            }
        }
    }

    /// Every pipeline that renders into `color_format` (a target or window format).
    pub fn prebuild(&mut self, dev: &Device, color_format: VkFormat) -> Result<(), GpuError> {
        self.prebuild_scene(dev, color_format)?;
        self.get(dev, PipelineKey::Composite { format: color_format })?;
        self.get(dev, PipelineKey::Fxaa { format: color_format })?;
        for depth in [false, true] {
            self.get(dev, PipelineKey::Overlay { format: color_format, depth })?;
        }
        Ok(())
    }

    fn prebuild_scene(&mut self, dev: &Device, color_format: VkFormat) -> Result<(), GpuError> {
        for blended in [false, true] {
            for features in FEATURE_SETS {
                self.get(dev, PipelineKey::Mesh { format: color_format, blended, features })?;
                if color_format == HDR_FORMAT {
                    // SSAO always renders the scene in HDR (its ambient fraction rides in alpha).
                    self.get(dev, PipelineKey::Mesh { format: color_format, blended, features: features | FEAT_SSAO })?;
                }
            }
        }
        for fog in [false, true] {
            self.get(dev, PipelineKey::Sky { format: color_format, fog })?;
        }
        Ok(())
    }

    /// Pipelines that don't depend on the output format: shadows, the HDR scene, bloom.
    pub fn prebuild_common(&mut self, dev: &Device) -> Result<(), GpuError> {
        self.get(dev, PipelineKey::Shadow)?;
        self.prebuild_scene(dev, HDR_FORMAT)?;
        self.get(dev, PipelineKey::BloomDown)?;
        self.get(dev, PipelineKey::BloomUp)?;
        self.get(dev, PipelineKey::Ssao)?;
        self.get(dev, PipelineKey::SsaoBlur)?;
        Ok(())
    }

    /// The pipelines of a frame ending in `final_format`, with scene `features` and post `mode`.
    /// `sky`: the frame draws the sky; `overlay`: it has overlay quads.
    pub fn frame(
        &mut self,
        dev: &Device,
        final_format: VkFormat,
        features: u32,
        mode: PostMode,
        sky: bool,
        overlay: bool,
    ) -> Result<FramePipelines, GpuError> {
        let scene_format = if mode.hdr() {
            HDR_FORMAT
        } else if mode.fxaa {
            LDR_FORMAT
        } else {
            final_format
        };
        let layout = self.layout;
        let mut get = |want: bool, key: PipelineKey| if want { self.get(dev, key) } else { Ok(VK_NULL_HANDLE) };
        let composite_format = if mode.fxaa { LDR_FORMAT } else { final_format };
        Ok(FramePipelines {
            layout,
            opaque: get(true, PipelineKey::Mesh { format: scene_format, blended: false, features })?,
            blended: get(true, PipelineKey::Mesh { format: scene_format, blended: true, features })?,
            shadow: get(features & FEAT_SHADOWS != 0, PipelineKey::Shadow)?,
            sky: get(sky, PipelineKey::Sky { format: scene_format, fog: features & FEAT_FOG != 0 })?,
            bloom_down: get(mode.bloom(), PipelineKey::BloomDown)?,
            bloom_up: get(mode.bloom(), PipelineKey::BloomUp)?,
            composite: get(mode.hdr(), PipelineKey::Composite { format: composite_format })?,
            fxaa: get(mode.fxaa, PipelineKey::Fxaa { format: final_format })?,
            overlay: get(overlay, PipelineKey::Overlay { format: final_format, depth: mode.direct() })?,
            ssao: get(features & FEAT_SSAO != 0, PipelineKey::Ssao)?,
            ssao_blur: get(features & FEAT_SSAO != 0, PipelineKey::SsaoBlur)?,
        })
    }

    /// The pipeline for `key`. Normally prebuilt; one created here is counted against the warm mark.
    pub fn get(&mut self, dev: &Device, key: PipelineKey) -> Result<VkPipeline, GpuError> {
        if let Some(&(_, p)) = self.by_key.iter().find(|(k, _)| *k == key) {
            return Ok(p);
        }
        let start = Instant::now();
        let p = self.create(dev, key)?;
        self.create_ms += start.elapsed().as_secs_f64() * 1e3;
        self.created += 1;
        self.by_key.push((key, p));
        Ok(p)
    }

    /// Loading is done: any pipeline created from now on is reported as a hitch.
    pub fn mark_warm(&mut self) {
        self.warm_at = Some(self.created);
    }

    pub fn stats(&self) -> PipelineStats {
        PipelineStats {
            created: self.created,
            created_after_warm: self.warm_at.map_or(0, |w| self.created - w),
            warm: self.warm_at.is_some(),
            create_ms: self.create_ms,
            cache: self.cache_load,
        }
    }

    /// The pipeline cache's contents, to persist and pass back as `GpuOptions::pipeline_cache`.
    pub fn cache_data(&self, dev: &Device) -> Result<Vec<u8>, GpuError> {
        loop {
            let mut size = 0usize;
            // SAFETY: valid device and cache; size query with a null buffer.
            let r = unsafe { (dev.fns.get_pipeline_cache_data)(dev.handle, self.cache, &mut size, ptr::null_mut()) };
            if r != VK_SUCCESS {
                return Err(GpuError::vk("vkGetPipelineCacheData", r));
            }
            let mut data = vec![0u8; size];
            // SAFETY: `data` holds `size` bytes.
            let r = unsafe {
                (dev.fns.get_pipeline_cache_data)(dev.handle, self.cache, &mut size, data.as_mut_ptr() as *mut c_void)
            };
            match r {
                VK_SUCCESS => {
                    data.truncate(size);
                    return Ok(data);
                }
                VK_INCOMPLETE => continue, // grew between the calls
                _ => return Err(GpuError::vk("vkGetPipelineCacheData", r)),
            }
        }
    }

    fn create(&self, dev: &Device, key: PipelineKey) -> Result<VkPipeline, GpuError> {
        let d = key.desc();
        let entry = c"main";
        let spec_entry = VkSpecializationMapEntry { constant_id: 0, offset: 0, size: size_of::<u32>() };
        let spec = VkSpecializationInfo {
            map_entry_count: 1,
            p_map_entries: &spec_entry,
            data_size: size_of::<u32>(),
            p_data: &d.spec as *const u32 as *const c_void,
        };
        let stage = |stage: VkFlags, shader: Shader| VkPipelineShaderStageCreateInfo {
            s_type: ST_PIPELINE_SHADER_STAGE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            stage,
            module: self.modules[shader as usize],
            p_name: entry.as_ptr(),
            p_specialization_info: &spec as *const VkSpecializationInfo as *const c_void,
        };
        let mut stages = vec![stage(VK_SHADER_STAGE_VERTEX_BIT, d.vert)];
        if let Some(frag) = d.frag {
            stages.push(stage(VK_SHADER_STAGE_FRAGMENT_BIT, frag));
        }
        let binding = VkVertexInputBindingDescription {
            binding: 0,
            stride: size_of::<Vertex>() as u32,
            input_rate: VK_VERTEX_INPUT_RATE_VERTEX,
        };
        let attributes = [
            VkVertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: VK_FORMAT_R32G32B32_SFLOAT,
                offset: offset_of!(Vertex, position) as u32,
            },
            VkVertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: VK_FORMAT_R32G32B32_SFLOAT,
                offset: offset_of!(Vertex, normal) as u32,
            },
            VkVertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: VK_FORMAT_R8G8B8A8_UNORM,
                offset: offset_of!(Vertex, color) as u32,
            },
        ];
        let (binding_count, attribute_count) = match d.vertex {
            VertexInput::None => (0, 0),
            VertexInput::Mesh => (1, attributes.len() as u32),
            VertexInput::Position => (1, 1),
        };
        let vertex_input = VkPipelineVertexInputStateCreateInfo {
            s_type: ST_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            vertex_binding_description_count: binding_count,
            p_vertex_binding_descriptions: &binding,
            vertex_attribute_description_count: attribute_count,
            p_vertex_attribute_descriptions: attributes.as_ptr(),
        };
        let input_assembly = VkPipelineInputAssemblyStateCreateInfo {
            s_type: ST_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            topology: VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            primitive_restart_enable: VK_FALSE,
        };
        let viewport = VkPipelineViewportStateCreateInfo {
            s_type: ST_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            viewport_count: 1,
            p_viewports: ptr::null(), // dynamic
            scissor_count: 1,
            p_scissors: ptr::null(), // dynamic
        };
        let raster = VkPipelineRasterizationStateCreateInfo {
            s_type: ST_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            depth_clamp_enable: VK_FALSE,
            rasterizer_discard_enable: VK_FALSE,
            polygon_mode: VK_POLYGON_MODE_FILL,
            cull_mode: VK_CULL_MODE_NONE,
            front_face: VK_FRONT_FACE_COUNTER_CLOCKWISE,
            depth_bias_enable: d.depth_bias as VkBool32,
            depth_bias_constant_factor: 0.0,
            depth_bias_clamp: 0.0,
            depth_bias_slope_factor: 0.0,
            line_width: 1.0,
        };
        let multisample = VkPipelineMultisampleStateCreateInfo {
            s_type: ST_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            rasterization_samples: VK_SAMPLE_COUNT_1_BIT,
            sample_shading_enable: VK_FALSE,
            min_sample_shading: 0.0,
            p_sample_mask: ptr::null(),
            alpha_to_coverage_enable: VK_FALSE,
            alpha_to_one_enable: VK_FALSE,
        };
        let depth_stencil = VkPipelineDepthStencilStateCreateInfo {
            s_type: ST_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            depth_test_enable: d.depth_test as VkBool32,
            depth_write_enable: d.depth_write as VkBool32,
            depth_compare_op: d.compare.unwrap_or(VK_COMPARE_OP_LESS),
            depth_bounds_test_enable: VK_FALSE,
            stencil_test_enable: VK_FALSE,
            front: VkStencilOpState::default(),
            back: VkStencilOpState::default(),
            min_depth_bounds: 0.0,
            max_depth_bounds: 1.0,
        };
        let blend_attachment = match d.blend {
            Blend::Off => VkPipelineColorBlendAttachmentState {
                blend_enable: VK_FALSE,
                color_write_mask: VK_COLOR_COMPONENT_RGBA,
                ..Default::default()
            },
            Blend::PremultipliedKeepAlpha => VkPipelineColorBlendAttachmentState {
                blend_enable: VK_TRUE,
                src_color_blend_factor: VK_BLEND_FACTOR_ONE,
                dst_color_blend_factor: VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
                color_blend_op: VK_BLEND_OP_ADD,
                src_alpha_blend_factor: VK_BLEND_FACTOR_ZERO,
                dst_alpha_blend_factor: VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
                alpha_blend_op: VK_BLEND_OP_ADD,
                color_write_mask: VK_COLOR_COMPONENT_RGBA,
            },
            Blend::Premultiplied => VkPipelineColorBlendAttachmentState {
                blend_enable: VK_TRUE,
                src_color_blend_factor: VK_BLEND_FACTOR_ONE,
                dst_color_blend_factor: VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
                color_blend_op: VK_BLEND_OP_ADD,
                src_alpha_blend_factor: VK_BLEND_FACTOR_ONE,
                dst_alpha_blend_factor: VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
                alpha_blend_op: VK_BLEND_OP_ADD,
                color_write_mask: VK_COLOR_COMPONENT_RGBA,
            },
        };
        let blend = VkPipelineColorBlendStateCreateInfo {
            s_type: ST_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            logic_op_enable: VK_FALSE,
            logic_op: 0,
            attachment_count: d.color.is_some() as u32,
            p_attachments: &blend_attachment,
            blend_constants: [0.0; 4],
        };
        let mut dynamic_states = vec![VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR];
        if d.compare.is_none() && d.depth_test {
            dynamic_states.push(VK_DYNAMIC_STATE_DEPTH_COMPARE_OP);
        }
        if d.depth_bias {
            dynamic_states.push(VK_DYNAMIC_STATE_DEPTH_BIAS);
        }
        let dynamic = VkPipelineDynamicStateCreateInfo {
            s_type: ST_PIPELINE_DYNAMIC_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            dynamic_state_count: dynamic_states.len() as u32,
            p_dynamic_states: dynamic_states.as_ptr(),
        };
        let color_format = d.color.unwrap_or(0);
        let rendering = VkPipelineRenderingCreateInfo {
            s_type: ST_PIPELINE_RENDERING_CREATE_INFO,
            p_next: ptr::null(),
            view_mask: 0,
            color_attachment_count: d.color.is_some() as u32,
            p_color_attachment_formats: &color_format,
            depth_attachment_format: d.depth.unwrap_or(0),
            stencil_attachment_format: 0, // VK_FORMAT_UNDEFINED
        };
        let info = VkGraphicsPipelineCreateInfo {
            s_type: ST_GRAPHICS_PIPELINE_CREATE_INFO,
            p_next: &rendering as *const _ as *const c_void,
            flags: 0,
            stage_count: stages.len() as u32,
            p_stages: stages.as_ptr(),
            p_vertex_input_state: &vertex_input,
            p_input_assembly_state: &input_assembly,
            p_tessellation_state: ptr::null(),
            p_viewport_state: &viewport,
            p_rasterization_state: &raster,
            p_multisample_state: &multisample,
            p_depth_stencil_state: if d.depth.is_some() { &depth_stencil } else { ptr::null() },
            p_color_blend_state: if d.color.is_some() { &blend } else { ptr::null() },
            p_dynamic_state: &dynamic,
            layout: self.layout,
            render_pass: VK_NULL_HANDLE,
            subpass: 0,
            base_pipeline_handle: VK_NULL_HANDLE,
            base_pipeline_index: -1,
        };
        let mut pipeline = VK_NULL_HANDLE;
        // SAFETY: every state struct referenced by `info` is a local alive for the call; `cache`
        // is this device's pipeline cache, used from this thread only.
        let r = unsafe { (dev.fns.create_graphics_pipelines)(dev.handle, self.cache, 1, &info, ptr::null(), &mut pipeline) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk(&format!("vkCreateGraphicsPipelines ({key:?})"), r));
        }
        Ok(pipeline)
    }

    pub fn destroy(&mut self, dev: &Device) {
        // SAFETY: the device is idle; every object is destroyed once (null handles are ignored).
        unsafe {
            for (_, p) in self.by_key.drain(..) {
                (dev.fns.destroy_pipeline)(dev.handle, p, ptr::null());
            }
            (dev.fns.destroy_pipeline_cache)(dev.handle, self.cache, ptr::null());
            (dev.fns.destroy_pipeline_layout)(dev.handle, self.layout, ptr::null());
            for m in self.modules.drain(..) {
                (dev.fns.destroy_shader_module)(dev.handle, m, ptr::null());
            }
        }
        self.cache = VK_NULL_HANDLE;
        self.layout = VK_NULL_HANDLE;
    }
}

/// Fail early (before recording) if a draw names a mesh that does not exist or there are too
/// many lights.
pub fn check_frame(meshes: &HashMap<u32, Mesh>, textures: &Textures, frame: &FrameDesc) -> Result<(), GpuError> {
    check_overlay(frame.overlay, textures)?;
    crate::post::check_lut(&frame.environment, textures)?;
    if frame.lights.len() > MAX_POINT_LIGHTS {
        return Err(GpuError::limit(Limit::PointLights, frame.lights.len() as u64, MAX_POINT_LIGHTS as u64));
    }
    match frame.draws.iter().find(|d| !meshes.contains_key(&d.mesh.0)) {
        Some(d) => Err(GpuError::unknown(crate::IdKind::Mesh, d.mesh.0, " in draw list")),
        None => Ok(()),
    }
}

/// Cull and batch `frame` against `meshes`, plan its shadow casters, then write its frame data
/// and draw records into `upload` (resizing it). The GPU must not be reading `upload`. The plans
/// are left in `scratch` for recording.
pub fn prepare_frame(
    dev: &Device,
    meshes: &HashMap<u32, Mesh>,
    textures: &Textures,
    frame: &FrameDesc,
    extent: VkExtent2D,
    upload: &mut FrameUpload,
    scratch: &mut FrameScratch,
) -> Result<(), GpuError> {
    let bounds = |m: crate::MeshId| meshes.get(&m.0).map(|mesh| mesh.bounds);
    crate::batch::plan_into(frame, bounds, &mut scratch.plan);
    plan_shadows(frame, &scratch.plan, bounds, &mut scratch.shadow);
    build_records(frame, &scratch.plan, &mut scratch.linear, &mut scratch.records);
    append_shadow_records(frame, &scratch.shadow, &mut scratch.records);
    build_overlay(frame.overlay, textures, &mut scratch.linear, &mut scratch.overlay);
    upload.reserve(dev, scratch.records.len() + overlay_units(scratch.overlay.len(), RECORD_SIZE as usize))?;
    upload.write(&frame_data(frame, extent, &scratch.shadow.data), &scratch.records, &scratch.overlay);
    Ok(())
}

/// Everything recording a frame needs besides the command buffer, its graph and the timer.
pub struct FrameContext<'a> {
    pub dev: &'a Device,
    pub pipelines: FramePipelines,
    pub descriptors: &'a Descriptors,
    pub meshes: &'a HashMap<u32, Mesh>,
    pub frame: &'a FrameDesc<'a>,
    pub scratch: &'a FrameScratch,
    pub upload: &'a FrameUpload,
    /// Uploaded textures (the grading LUT).
    pub textures: &'a Textures,
}

/// The scene features a frame prepared in `scratch` uses, given whether it renders a shadow map.
pub fn frame_features(frame: &FrameDesc, scratch: &FrameScratch, have_shadow_map: bool) -> u32 {
    let mut f = 0;
    if have_shadow_map && scratch.shadow.data.count > 0 {
        f |= FEAT_SHADOWS;
    }
    if frame.environment.fog.enabled {
        f |= FEAT_FOG;
    }
    if frame.environment.ssao.enabled {
        f |= FEAT_SSAO;
    }
    f
}

/// A pass of the frame graph (the tag `record_frame` dispatches on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePass {
    Shadow,
    /// SSAO from the scene's depth: the occlusion, its blur.
    Ssao,
    SsaoBlur,
    Scene,
    /// Bloom bright pass / downsample into level `n`.
    BloomDown(u8),
    /// Bloom blur into `blur[n - 1]` from level `n`.
    BloomUp(u8),
    Tonemap,
    Fxaa,
    /// The overlay quads, inline in the pass that writes the final image.
    Overlay,
}

impl FramePass {
    /// The timing ([`crate::PassTiming::name`]) the pass counts towards.
    pub fn timing(self) -> &'static str {
        match self {
            FramePass::Shadow => PASS_SHADOW,
            FramePass::Ssao | FramePass::SsaoBlur => crate::PASS_SSAO,
            FramePass::Scene => PASS_SCENE,
            FramePass::BloomDown(_) | FramePass::BloomUp(_) => PASS_BLOOM,
            FramePass::Tonemap => PASS_TONEMAP,
            FramePass::Fxaa => PASS_FXAA,
            FramePass::Overlay => PASS_OVERLAY,
        }
    }
}

pub type FrameGraph = RenderGraph<FramePass>;

/// The resources of a declared frame in its graph (unused ones are `Res::default()`).
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameRes {
    pub extent: VkExtent2D,
    /// The final image (offscreen target or swapchain image).
    pub out: Res,
    pub depth: Res,
    pub shadow: Option<Res>,
    /// What the scene renders into: `out`, `hdr` (bloom) or `ldr` (FXAA only).
    pub scene: Res,
    pub hdr: Res,
    pub ldr: Res,
    pub levels: [Res; MAX_BLOOM_LEVELS as usize],
    pub blurs: [Res; MAX_BLOOM_LEVELS as usize - 1],
    pub bloom_levels: u32,
    pub fxaa: bool,
    /// The scene renders into `hdr` (bloom or grading follows).
    pub post_hdr: bool,
    /// SSAO: raw and blurred occlusion (`Some` size when on).
    pub ssao: Option<VkExtent2D>,
    pub ao_raw: Res,
    pub ao: Res,
}

/// What a frame renders into and which passes it runs.
#[derive(Clone, Copy, Debug)]
pub struct FrameShape {
    /// Name of the final image in the graph ("target", "swapchain").
    pub out_name: &'static str,
    pub format: VkFormat,
    pub extent: VkExtent2D,
    pub prior: Prior,
    pub fin: Final,
    pub mode: PostMode,
    /// The shadow map, when the frame renders shadows.
    pub shadow: Option<ImageDesc>,
    pub overlay: bool,
    /// The SSAO images' size, when the frame renders SSAO ([`crate::ssao::ssao_extent`]).
    pub ssao: Option<VkExtent2D>,
}

/// Declare a frame's passes into `g`: shadow → scene (+ overlay inline when the scene writes the
/// final image) → bloom chain → tonemap → FXAA, the overlay inline in whichever writes last.
pub fn declare_frame(g: &mut FrameGraph, s: &FrameShape) -> FrameRes {
    g.begin();
    let (w, h) = (s.extent.width, s.extent.height);
    let mode = s.mode;
    let mut r = FrameRes {
        extent: s.extent,
        bloom_levels: mode.bloom_levels,
        fxaa: mode.fxaa,
        post_hdr: mode.hdr(),
        ssao: s.ssao,
        ..Default::default()
    };
    r.out = g.import_image(s.out_name, s.format, w, h, s.prior, s.fin);
    // With SSAO the scene's depth is read afterwards (bindless): the occlusion is computed from it.
    let depth = ImageDesc { bindless: s.ssao.is_some(), ..ImageDesc::depth(DEPTH_FORMAT, w, h, MemoryKind::RenderTargets) };
    r.depth = g.transient("depth", depth);
    if let Some(desc) = s.shadow {
        let map = g.shared("shadow", desc);
        g.pass("shadow", FramePass::Shadow).depth(map, Load::Clear);
        r.shadow = Some(map);
    }
    if mode.hdr() {
        r.hdr = g.transient("hdr", ImageDesc::color(HDR_FORMAT, w, h, MemoryKind::PostTargets));
    }
    if mode.fxaa {
        r.ldr = g.transient("ldr", ImageDesc::color(LDR_FORMAT, w, h, MemoryKind::PostTargets));
    }
    r.scene = if mode.hdr() {
        r.hdr
    } else if mode.fxaa {
        r.ldr
    } else {
        r.out
    };
    let scene = g.pass("scene", FramePass::Scene).color(r.scene, Load::Clear).depth(r.depth, Load::Clear);
    if let Some(map) = r.shadow {
        scene.sampled(map);
    }
    if let Some(size) = s.ssao {
        crate::ssao::declare(g, &mut r, size);
    }
    let overlay = |g: &mut FrameGraph, out| {
        if s.overlay {
            g.pass("overlay", FramePass::Overlay).inline().color(out, Load::Keep);
        }
    };
    if mode.direct() {
        overlay(g, r.out);
    }
    if mode.bloom() {
        let n = mode.bloom_levels as usize;
        for i in 0..n {
            let size = bloom_level_size(s.extent, i as u32);
            r.levels[i] = g.transient(BLOOM_LEVELS[i], ImageDesc::color(HDR_FORMAT, size.width, size.height, MemoryKind::PostTargets));
            if i + 1 < n {
                r.blurs[i] = g.transient(BLOOM_BLURS[i], ImageDesc::color(HDR_FORMAT, size.width, size.height, MemoryKind::PostTargets));
            }
        }
        for (i, name) in BLOOM_DOWN_PASSES.iter().enumerate().take(n) {
            let src = if i == 0 { r.hdr } else { r.levels[i - 1] };
            g.pass(name, FramePass::BloomDown(i as u8)).sampled(src).color(r.levels[i], Load::DontCare);
        }
        for i in (1..n).rev() {
            let smaller = if i == n - 1 { r.levels[i] } else { r.blurs[i] };
            g.pass(BLOOM_UP_PASSES[i - 1], FramePass::BloomUp(i as u8))
                .sampled(smaller)
                .sampled(r.levels[i - 1])
                .color(r.blurs[i - 1], Load::DontCare);
        }
    }
    if mode.hdr() {
        let dst = if mode.fxaa { r.ldr } else { r.out };
        let mut tonemap = g.pass("post_tonemap", FramePass::Tonemap).sampled(r.hdr);
        if mode.bloom() {
            let n = mode.bloom_levels as usize;
            tonemap = tonemap.sampled(if n > 1 { r.blurs[0] } else { r.levels[0] });
        }
        if r.ssao.is_some() {
            tonemap = tonemap.sampled(r.ao);
        }
        tonemap.color(dst, Load::DontCare);
        if !mode.fxaa {
            overlay(g, r.out);
        }
    }
    if mode.fxaa {
        g.pass("post_fxaa", FramePass::Fxaa).sampled(r.ldr).color(r.out, Load::DontCare);
        overlay(g, r.out);
    }
    r
}

/// Compile (if changed) and resolve a declared frame graph, and point descriptor binding 0 at the
/// frame's shadow map. Rewriting it waits for the GPU first (frames in flight may sample the old
/// map): that only happens when the shadow settings change.
pub fn prepare_graph(
    dev: &Device,
    descriptors: &mut Descriptors,
    pool: &mut TransientPool,
    g: &mut FrameGraph,
    res: &FrameRes,
) -> Result<(), GpuError> {
    g.prepare(dev, descriptors, pool)?;
    if let Some(map) = res.shadow {
        let view = g.image(map).array_view;
        if descriptors.shadow_view != view {
            if descriptors.shadow_view != VK_NULL_HANDLE {
                dev.wait_idle();
            }
            descriptors.write_shadow(dev, view);
        }
    }
    Ok(())
}

/// The pass timer, begun and ended as the frame moves between timings.
struct Timing<'t> {
    timer: Option<&'t mut PassTimer>,
    name: Option<&'static str>,
    pass: Option<usize>,
}

impl Timing<'_> {
    fn enter(&mut self, dev: &Device, cmd: VkCommandBuffer, name: &'static str) {
        if self.name == Some(name) {
            return;
        }
        self.end(dev, cmd);
        self.name = Some(name);
        self.pass = self.timer.as_mut().and_then(|t| t.begin_pass(dev, cmd, name));
    }

    fn end(&mut self, dev: &Device, cmd: VkCommandBuffer) {
        if let Some(t) = self.timer.as_mut() {
            t.end_pass(dev, cmd, self.pass.take());
        }
        self.name = None;
        self.pass = None;
    }
}

/// Record a whole frame: every pass of `g` in order, each after the barriers the graph computed
/// for it, then the graph's final transitions. `g` is prepared and its imported images bound.
pub fn record_frame(ctx: &FrameContext, cmd: VkCommandBuffer, g: &mut FrameGraph, res: &FrameRes, timer: Option<&mut PassTimer>) -> DrawStats {
    let dev = ctx.dev;
    let mut timing = Timing { timer, name: None, pass: None };
    let mut shadow_stats = DrawStats::default();
    let mut stats = DrawStats::default();
    ctx.descriptors.bind(dev, cmd, ctx.pipelines.layout);
    let env = &ctx.frame.environment;
    let overlay = OverlayDraw {
        pipeline: ctx.pipelines.overlay,
        layout: ctx.pipelines.layout,
        frame: ctx.upload.frame_address(),
        quads: ctx.upload.overlay_address(ctx.scratch.records.len()),
        count: ctx.scratch.overlay.len() as u32,
    };
    for p in 0..g.pass_count() {
        if g.is_inline(p) {
            continue;
        }
        let tag = g.tag(p);
        timing.enter(dev, cmd, tag.timing());
        g.barriers(dev, cmd, p);
        let g = &*g;
        // The overlay draws inside the rendering instance of the pass that writes the final
        // image, and is timed on its own.
        let with_overlay = g.inline_after(p).is_some() && overlay.count > 0;
        let mut inline = |cmd: VkCommandBuffer| {
            if with_overlay {
                timing.end(dev, cmd);
                record_overlay(dev, cmd, &overlay, timing.timer.as_deref_mut());
            }
        };
        match tag {
            FramePass::Shadow => {
                if let Some(map) = res.shadow {
                    crate::shadow::record_shadow_pass(
                        dev,
                        cmd,
                        ctx.pipelines.shadow,
                        ctx.pipelines.layout,
                        ctx.meshes,
                        g,
                        p,
                        map,
                        &ctx.scratch.shadow,
                        ctx.scratch.shadow_first_record(),
                        ctx.upload,
                        env.shadow.bias,
                        &mut shadow_stats,
                    );
                }
            }
            FramePass::Scene => {
                let clear = srgb_to_linear_rgba(ctx.frame.clear_color);
                let clear = if res.post_hdr { hdr_clear(clear, env.tonemap) } else { clear };
                stats = record_scene(ctx, cmd, g, p, res, clear, &mut inline);
            }
            FramePass::Ssao | FramePass::SsaoBlur => crate::ssao::record_pass(ctx, cmd, g, p, res),
            FramePass::Overlay => {} // inline: recorded by the pass before it
            _ => record_post_pass(dev, cmd, g, p, res, &ctx.pipelines, env, ctx.textures, ctx.upload.frame_address(), &mut inline),
        }
    }
    timing.end(dev, cmd);
    g.finish(dev, cmd);
    stats.overlay_quads = overlay.count;
    stats.shadow_cascades = shadow_stats.shadow_cascades;
    stats.shadow_draw_calls = shadow_stats.shadow_draw_calls;
    stats.shadow_instances = shadow_stats.shadow_instances;
    stats.shadow_triangles = shadow_stats.shadow_triangles;
    stats
}

/// Record the scene pass `p` of `g` (the graph recorded its barriers): clear, one instanced draw
/// per batch of the frame's plan, the sky between the opaque and blended draws, then `inline`
/// (the overlay) inside the same rendering instance.
pub fn record_scene(
    ctx: &FrameContext,
    cmd: VkCommandBuffer,
    g: &FrameGraph,
    p: usize,
    res: &FrameRes,
    clear: [f32; 4],
    inline: &mut dyn FnMut(VkCommandBuffer),
) -> DrawStats {
    let (dev, pipelines, frame, upload) = (ctx.dev, &ctx.pipelines, ctx.frame, ctx.upload);
    let plan = &ctx.scratch.plan;
    let color_attachment = g.color_attachment(p, res.scene, clear);
    // The farthest depth of the mode.
    let depth_attachment = g.depth_attachment(p, res.depth, if frame.reverse_z { 0.0 } else { 1.0 }, None);
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: res.extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 1,
        p_color_attachments: &color_attachment,
        p_depth_attachment: &depth_attachment,
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport {
        x: 0.0,
        y: 0.0,
        width: res.extent.width as f32,
        height: res.extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let mut stats = DrawStats {
        submitted: frame.draws.len() as u32,
        culled: plan.culled,
        unbatched_draw_calls: plan.entries.len() as u32,
        ..Default::default()
    };
    // SAFETY: `cmd` is recording; all referenced structs outlive each call; the pipelines and
    // layout are compatible; every mesh buffer is live (checked by `check_frame`) and the GPU
    // does not outlive it (callers wait before destroying meshes); `upload` holds the frame data
    // and one record per plan entry, so every instance index is in bounds.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        (dev.fns.cmd_set_depth_compare_op)(cmd, if frame.reverse_z { VK_COMPARE_OP_GREATER } else { VK_COMPARE_OP_LESS });
        ctx.descriptors.bind(dev, cmd, pipelines.layout);
        let mut bound = VK_NULL_HANDLE;
        let mut bound_mesh = None;
        let mut sky_pending = pipelines.sky != VK_NULL_HANDLE;
        let draw_sky = |cmd: VkCommandBuffer| {
            // Behind everything opaque, before anything blended (glass shows the sky through it).
            (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipelines.sky);
            let push = crate::post::PostPush {
                frame: upload.frame_address(),
                src: 0,
                src2: 0,
                texel: [0.0; 2],
                mode: frame.reverse_z as u32,
                pad: 0,
                params: [0.0; 4],
            };
            (dev.fns.cmd_push_constants)(
                cmd,
                pipelines.layout,
                PUSH_STAGES,
                0,
                size_of::<crate::post::PostPush>() as u32,
                &push as *const crate::post::PostPush as *const c_void,
            );
            (dev.fns.cmd_draw)(cmd, 3, 1, 0, 0);
        };
        for batch in &plan.batches {
            if sky_pending && batch.material.blended() {
                draw_sky(cmd);
                // The sky pipeline's depth compare is static, so binding it invalidated the
                // dynamic compare op the mesh pipelines use: set it again before their draws.
                (dev.fns.cmd_set_depth_compare_op)(cmd, if frame.reverse_z { VK_COMPARE_OP_GREATER } else { VK_COMPARE_OP_LESS });
                sky_pending = false;
                bound = pipelines.sky;
                stats.pipeline_binds += 1;
            }
            let Some(mesh) = ctx.meshes.get(&batch.mesh.0) else { continue };
            let pipeline = if batch.material.blended() { pipelines.blended } else { pipelines.opaque };
            if pipeline != bound {
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
                bound = pipeline;
                stats.pipeline_binds += 1;
            }
            let pc = PushConstants {
                frame: upload.frame_address(),
                draws: upload.records_address(),
                material: material_id(batch.material),
                pass: batch.pass,
            };
            (dev.fns.cmd_push_constants)(
                cmd,
                pipelines.layout,
                PUSH_STAGES,
                0,
                size_of::<PushConstants>() as u32,
                &pc as *const PushConstants as *const c_void,
            );
            if bound_mesh != Some(batch.mesh) {
                let offset: VkDeviceSize = 0;
                (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &mesh.buffer.handle, &offset);
                (dev.fns.cmd_bind_index_buffer)(cmd, mesh.buffer.handle, mesh.index_offset, VK_INDEX_TYPE_UINT32);
                bound_mesh = Some(batch.mesh);
            }
            (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, batch.count, 0, 0, batch.first);
            stats.draw_calls += 1;
            stats.instances += batch.count;
            stats.triangles += (mesh.index_count / 3) as u64 * batch.count as u64;
        }
        if sky_pending {
            draw_sky(cmd);
            stats.pipeline_binds += 1;
        }
    }
    inline(cmd);
    // SAFETY: ends the rendering instance begun above.
    unsafe { (dev.fns.cmd_end_rendering)(cmd) };
    stats
}

// ---------------------------------------------------------------------------------------------
// Colour conversion
// ---------------------------------------------------------------------------------------------

/// sRGB-encoded channel → linear (IEC 61966-2-1), clamped to 0..1.
pub fn srgb_to_linear(c: f32) -> f32 {
    let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// RGB converted to linear; alpha passed through.
pub fn srgb_to_linear_rgba(c: [f32; 4]) -> [f32; 4] {
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeshId, Surface};

    #[test]
    fn push_constants_match_shader_layout() {
        assert_eq!(size_of::<PushConstants>(), 24);
        assert_eq!(offset_of!(PushConstants, draws), 8);
        assert_eq!(offset_of!(PushConstants, material), 16);
        assert_eq!(offset_of!(PushConstants, pass), 20);
        assert_eq!(size_of::<Vertex>(), 28);
        assert_eq!(offset_of!(Vertex, color), 24);
    }

    #[test]
    fn draw_record_matches_shader_layout() {
        // std430 `Draw` in shaders/mesh.*: mat4 + 3 × vec4, array stride 112.
        assert_eq!(offset_of!(DrawRecord, color), 64);
        assert_eq!(offset_of!(DrawRecord, color2), 80);
        assert_eq!(offset_of!(DrawRecord, params), 96);
        assert_eq!(size_of::<DrawRecord>(), 112);
        assert_eq!(RECORDS_OFFSET, 1280);
    }

    #[test]
    fn frame_data_matches_shader_layout() {
        // std430 offsets of `Frame` in shaders/mesh.*.
        assert_eq!(offset_of!(FrameData, camera_time), 64);
        assert_eq!(offset_of!(FrameData, viewport), 144);
        assert_eq!(offset_of!(FrameData, lights), 160);
        assert_eq!(offset_of!(FrameData, inv_view_proj), 160 + 32 * MAX_POINT_LIGHTS);
        assert_eq!(offset_of!(FrameData, shadow_mats), 736);
        assert_eq!(offset_of!(FrameData, shadow_params), 992);
        assert_eq!(offset_of!(FrameData, shadow_fade), 1040);
        assert_eq!(offset_of!(FrameData, fog_color), 1056);
        assert_eq!(offset_of!(FrameData, sky_params), 1136);
        assert_eq!(size_of::<FrameData>(), 1152);
    }

    #[test]
    fn default_frame_data_reproduces_m0_lighting() {
        let frame = FrameDesc::default();
        let d = frame_data(&frame, VkExtent2D { width: 4, height: 2 }, &ShadowData::default());
        assert!((d.ambient[0] - 0.25).abs() < 1e-6 && (d.sun_color[1] - 0.75).abs() < 1e-6);
        assert_eq!(d.ambient[3], 1.0, "exposure");
        assert_eq!(d.sky[3], 0.0, "no tone mapping");
        let l = (0.4f32 * 0.4 + 0.8 * 0.8 + 0.45 * 0.45).sqrt();
        assert!((d.sun_dir[0] - 0.4 / l).abs() < 1e-6);
        assert_eq!(d.viewport, [4.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn camera_position_comes_from_view_proj() {
        // A camera at (0, 0, 5) looking down −Z (Vulkan clip, reversed Z, near 0.05).
        let mut vp = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0];
        for r in 0..4 {
            vp[12 + r] += vp[8 + r] * -5.0;
        }
        let p = camera_position(&vp);
        assert!((p[0]).abs() < 1e-5 && (p[1]).abs() < 1e-5 && (p[2] - 5.0).abs() < 1e-5, "{p:?}");
        assert_eq!(camera_position(&[0.0; 16]), [0.0; 3], "singular falls back to the origin");
    }

    #[test]
    fn records_follow_plan_order_and_convert_once_per_run() {
        let mut model = [0.0; 16];
        model[0] = 1.0;
        model[15] = 1.0;
        let mut a = DrawItem::new(MeshId(1), model, [0.5, 0.25, 1.0, 0.3]);
        a.surface = Surface { emissive: -1.0, metallic: 2.0, roughness: 0.4, specular: 0.5, color2: [1.0, 0.5, 0.0], seed: 7.0 };
        let mut b = a;
        b.model[12] = 3.0;
        let c = DrawItem::new(MeshId(1), model, [0.0, 0.0, 0.0, 1.0]);
        let draws = [a, b, c];
        let frame = FrameDesc { draws: &draws, ..Default::default() };
        let mut plan = Plan::default();
        plan.entries = vec![(2, 0), (1, 0), (0, 0), (0, 1)];
        let mut out = Vec::new();
        build_records(&frame, &plan, &mut LinearCache::default(), &mut out);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], draw_record(&c));
        assert_eq!(out[1], draw_record(&b));
        assert_eq!(out[2], draw_record(&a));
        assert_eq!(out[3], draw_record(&a));
        assert_eq!(out[1].model[12], 3.0, "the memo keeps each draw's own model");
        let r = draw_record(&a);
        assert_eq!(r.params, [0.0, 1.0, 0.4, 0.5], "clamped");
        assert_eq!(r.color[3], 0.3, "opacity passes through");
        assert_eq!(r.color2[3], 7.0, "seed");
        assert!((r.color[0] - 0.214_041).abs() < 1e-5, "linear");
    }

    #[test]
    fn upload_sizing_grows_at_once_and_shrinks_with_hysteresis() {
        let mut s = UploadSizing::new(INITIAL_RECORDS);
        assert_eq!(s.next(100).unwrap(), None);
        assert_eq!(s.next(300).unwrap(), Some(512), "at least doubling");
        s.capacity = 512;
        assert_eq!(s.next(5000).unwrap(), Some(5000), "or exactly the need");
        s.capacity = 5000;
        // A spike is over: small frames for a while, but not long enough.
        for _ in 0..UploadSizing::SHRINK_AFTER - 1 {
            assert_eq!(s.next(40).unwrap(), None);
        }
        // One busy frame (over a quarter) resets the count.
        assert_eq!(s.next(1300).unwrap(), None);
        for _ in 0..UploadSizing::SHRINK_AFTER - 1 {
            assert_eq!(s.next(300).unwrap(), None);
        }
        assert_eq!(s.next(600).unwrap(), Some(1200), "twice the quiet peak");
        s.capacity = 1200;
        // The initial size never shrinks.
        let mut small = UploadSizing::new(INITIAL_RECORDS);
        for _ in 0..2 * UploadSizing::SHRINK_AFTER {
            assert_eq!(small.next(0).unwrap(), None);
        }
        // Structured overflow error.
        let err = s.next(MAX_DRAW_RECORDS as usize + 1).unwrap_err();
        assert_eq!(
            err.limit,
            Some(crate::LimitExceeded { what: Limit::DrawRecords, requested: MAX_DRAW_RECORDS + 1, max: MAX_DRAW_RECORDS })
        );
    }

    #[test]
    fn shrink_target_respects_initial_size() {
        let mut s = UploadSizing::new(4096);
        for _ in 0..UploadSizing::SHRINK_AFTER - 1 {
            s.next(10).unwrap();
        }
        assert_eq!(s.next(10).unwrap(), Some(INITIAL_RECORDS));
    }

    #[test]
    fn device_local_falls_back_to_any_host_memory() {
        let mut tried = Vec::new();
        let r: Result<u32, GpuError> = with_device_local_fallback(|flags| {
            tried.push(flags);
            if flags == 0 {
                Ok(7)
            } else {
                Err(GpuError::vk("vkAllocateMemory", VK_ERROR_OUT_OF_DEVICE_MEMORY))
            }
        });
        assert_eq!((r.unwrap(), tried), (7, vec![VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0]));
        let mut calls = 0;
        let ok: Result<u32, GpuError> = with_device_local_fallback(|_| {
            calls += 1;
            Ok(1)
        });
        assert_eq!((ok.unwrap(), calls), (1, 1), "no retry when the first try works");
    }

    #[test]
    fn too_many_lights_is_a_structured_error() {
        let lights = [crate::PointLight { position: [0.0; 3], color: [1.0; 3], intensity: 1.0, range: 1.0 }; MAX_POINT_LIGHTS + 1];
        let frame = FrameDesc { lights: &lights, ..Default::default() };
        let err = check_frame(&HashMap::new(), &Textures::default(), &frame).unwrap_err();
        assert_eq!(
            err.limit,
            Some(crate::LimitExceeded { what: Limit::PointLights, requested: 17, max: MAX_POINT_LIGHTS as u64 })
        );
        assert!(err.message.contains("17"), "{err}");
    }

    #[test]
    fn linear_cache_is_bit_identical_and_bounded() {
        let mut cache = LinearCache::default();
        let mut x = 0x1234_5678u32;
        for _ in 0..3 {
            for i in 0..1000u32 {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let rgb = [(x >> 8) as f32 / 16_777_216.0, i as f32 / 999.0, f32::from_bits(x)];
                let want = rgb.map(srgb_to_linear);
                assert_eq!(cache.get(rgb).map(f32::to_bits), want.map(f32::to_bits), "{rgb:?}");
                assert_eq!(cache.get(rgb).map(f32::to_bits), want.map(f32::to_bits), "cached {rgb:?}");
            }
        }
        for i in 0..LinearCache::MAX_ENTRIES as u32 + 10 {
            cache.get([f32::from_bits(i), 0.0, 0.0]);
        }
        assert!(cache.map.len() <= LinearCache::MAX_ENTRIES);
    }

    #[test]
    fn cache_header_checks_identity() {
        let id = CacheIdentity { vendor_id: 0x10de, device_id: 0x2717, uuid: [7; 16] };
        let mut blob = Vec::new();
        for w in [32u32, 1, 0x10de, 0x2717] {
            blob.extend_from_slice(&w.to_le_bytes());
        }
        blob.extend_from_slice(&[7; 16]);
        blob.extend_from_slice(b"driver data");
        assert_eq!(check_cache_header(&blob, &id), Ok(()));
        assert_eq!(check_cache_header(&blob[..20], &id), Err(CacheRejection::TooShort { len: 20 }));
        let mut bad = blob.clone();
        bad[4] = 2;
        assert_eq!(check_cache_header(&bad, &id), Err(CacheRejection::BadHeader { header_size: 32, header_version: 2 }));
        let mut bad = blob.clone();
        bad[0] = 200; // header claims to be longer than the blob
        assert!(matches!(check_cache_header(&bad, &id), Err(CacheRejection::BadHeader { .. })));
        let mut bad = blob.clone();
        bad[8] = 0x86;
        bad[9] = 0x80;
        assert_eq!(check_cache_header(&bad, &id), Err(CacheRejection::OtherVendor { vendor_id: 0x8086 }));
        let mut bad = blob.clone();
        bad[12] = 0;
        assert_eq!(check_cache_header(&bad, &id), Err(CacheRejection::OtherDevice { device_id: 0x2700 }));
        let mut bad = blob.clone();
        bad[31] = 8;
        assert_eq!(check_cache_header(&bad, &id), Err(CacheRejection::OtherDriver));
    }

    #[test]
    fn embedded_spirv_is_valid_shape() {
        for (_, spv) in SHADERS {
            assert_eq!(spv.len() % 4, 0);
            assert_eq!(spirv_words(spv)[0], 0x0723_0203, "SPIR-V magic");
        }
    }

    #[test]
    fn srgb_conversion_endpoints() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!((srgb_to_linear(0.5) - 0.214_041).abs() < 1e-5);
        assert_eq!(srgb_to_linear(f32::NAN), 0.0);
    }
}
