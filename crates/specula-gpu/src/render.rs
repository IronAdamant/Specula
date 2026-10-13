//! The mesh pipelines: shaders, pipeline creation and caching, per-frame data, per-draw records
//! and scene recording (instanced batches).

use crate::batch::{Bounds, Plan};
use crate::device::{image_barrier, Buffer, Device};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{
    CacheRejection, DrawItem, DrawStats, FrameDesc, GpuError, Material, MemoryKind, PipelineCacheLoad, PipelineStats, Vertex,
    MAX_POINT_LIGHTS, PASS_SCENE,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{offset_of, size_of};
use std::ptr;
use std::time::Instant;

static VERT_SPV: &[u8] = include_bytes!("../shaders/mesh.vert.spv");
static FRAG_SPV: &[u8] = include_bytes!("../shaders/mesh.frag.spv");

pub const DEPTH_FORMAT: VkFormat = VK_FORMAT_D32_SFLOAT;

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

/// Pack `frame` for the shaders.
pub fn frame_data(frame: &FrameDesc, extent: VkExtent2D) -> FrameData {
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
    FrameData {
        view_proj: frame.view_proj,
        camera_time: [c[0], c[1], c[2], frame.time_s],
        sun_dir,
        sun_color: lin(env.sun_color, env.sun_intensity.max(0.0)),
        ambient,
        sky,
        viewport: [extent.width as f32, extent.height as f32, frame.lights.len().min(MAX_POINT_LIGHTS) as f32, 0.0],
        lights,
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
    let p: Vec<f64> = (0..4).map(|r| a[r][4] / a[r][r]).collect();
    if p[3].abs() < 1e-12 || !p.iter().all(|v| v.is_finite()) {
        return [0.0; 3];
    }
    [(p[0] / p[3]) as f32, (p[1] / p[3]) as f32, (p[2] / p[3]) as f32]
}

/// The shader-side record of a draw (colours converted to linear, parameters clamped).
pub fn draw_record(draw: &DrawItem) -> DrawRecord {
    let s = &draw.surface;
    let c2 = srgb_to_linear_rgba([s.color2[0], s.color2[1], s.color2[2], 1.0]);
    DrawRecord {
        model: draw.model,
        color: srgb_to_linear_rgba(draw.color),
        color2: [c2[0], c2[1], c2[2], s.seed],
        params: [s.emissive.max(0.0), s.metallic.clamp(0.0, 1.0), s.roughness.clamp(0.0, 1.0), s.specular.max(0.0)],
    }
}

/// Fill `out` with the records of `plan.entries`, in order. Runs of equal colours (common in
/// batches) convert once.
pub fn build_records(frame: &FrameDesc, plan: &Plan, out: &mut Vec<DrawRecord>) {
    out.clear();
    out.reserve(plan.entries.len());
    let mut last: Option<(&DrawItem, DrawRecord)> = None;
    for &(i, _) in &plan.entries {
        let d = &frame.draws[i];
        let record = match last {
            Some((prev, r)) if prev.color == d.color && prev.surface == d.surface => DrawRecord { model: d.model, ..r },
            _ => draw_record(d),
        };
        last = Some((d, record));
        out.push(record);
    }
}

/// A frame's upload buffer: [`FrameData`] at offset 0, [`DrawRecord`]s at [`RECORDS_OFFSET`].
/// Host-visible (device-local when the device offers it), persistently mapped, read by the
/// shaders through its device address. Grows as needed.
pub struct FrameUpload {
    buffer: Buffer,
    address: u64,
    /// Records it has room for.
    capacity: u32,
}

impl FrameUpload {
    pub fn new(dev: &Device) -> Result<FrameUpload, GpuError> {
        Self::with_capacity(dev, INITIAL_RECORDS)
    }

    fn with_capacity(dev: &Device, capacity: u32) -> Result<FrameUpload, GpuError> {
        let buffer = dev.create_buffer(
            RECORDS_OFFSET + capacity as u64 * RECORD_SIZE,
            VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT,
            MemoryKind::PerFrame,
            "frame upload",
        )?;
        let address = dev.buffer_address(&buffer);
        Ok(FrameUpload { buffer, address, capacity })
    }

    /// Make room for `records`, replacing the buffer (at least doubling) when it is too small.
    /// The GPU must not be reading this buffer (callers waited for the frame that last used it).
    pub fn reserve(&mut self, dev: &Device, records: usize) -> Result<(), GpuError> {
        if records <= self.capacity as usize {
            return Ok(());
        }
        let wanted = records.max(2 * self.capacity as usize);
        let capacity = u32::try_from(wanted)
            .map_err(|_| GpuError::new(format!("{records} draw records in one frame; at most {} fit", u32::MAX)))?;
        let grown = Self::with_capacity(dev, capacity)?;
        dev.destroy_buffer(&self.buffer);
        *self = grown;
        Ok(())
    }

    /// Write the frame's data and records (`records.len()` ≤ the reserved capacity).
    pub fn write(&self, data: &FrameData, records: &[DrawRecord]) {
        assert!(records.len() <= self.capacity as usize, "reserve() before write()");
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

const PUSH_STAGES: VkFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;

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

/// Every pipeline variant per colour format: (reverse_z, blended).
const VARIANTS: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];

/// Shader modules, layout, the pipeline cache and every pipeline variant per colour format.
pub struct Pipelines {
    vert: VkShaderModule,
    frag: VkShaderModule,
    pub layout: VkPipelineLayout,
    cache: VkPipelineCache,
    by_format: Vec<((VkFormat, bool, bool), VkPipeline)>,
    created: u32,
    /// `created` when `mark_warm` was called.
    warm_at: Option<u32>,
    create_ms: f64,
    cache_load: PipelineCacheLoad,
}

/// The opaque and blended pipelines for one target.
#[derive(Clone, Copy)]
pub struct PipelinePair {
    pub opaque: VkPipeline,
    pub blended: VkPipeline,
    pub layout: VkPipelineLayout,
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
    /// Shaders, layout and the pipeline cache, seeded from `cache_blob` when it was made by this
    /// device and driver (the outcome is in [`Pipelines::stats`]). No pipelines yet: see
    /// [`Pipelines::prebuild`].
    pub fn new(dev: &Device, cache_blob: Option<&[u8]>, id: &CacheIdentity) -> Result<Pipelines, GpuError> {
        let mut p = Pipelines {
            vert: VK_NULL_HANDLE,
            frag: VK_NULL_HANDLE,
            layout: VK_NULL_HANDLE,
            cache: VK_NULL_HANDLE,
            by_format: Vec::new(),
            created: 0,
            warm_at: None,
            create_ms: 0.0,
            cache_load: PipelineCacheLoad::NotProvided,
        };
        let result = (|| {
            p.vert = create_shader(dev, VERT_SPV, "mesh.vert")?;
            p.frag = create_shader(dev, FRAG_SPV, "mesh.frag")?;
            let range = VkPushConstantRange { stage_flags: PUSH_STAGES, offset: 0, size: size_of::<PushConstants>() as u32 };
            let info = VkPipelineLayoutCreateInfo {
                s_type: ST_PIPELINE_LAYOUT_CREATE_INFO,
                p_next: ptr::null(),
                flags: 0,
                set_layout_count: 0,
                p_set_layouts: ptr::null(),
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

    /// Create every variant (depth mode × blending) for `color_format` that doesn't exist yet.
    pub fn prebuild(&mut self, dev: &Device, color_format: VkFormat) -> Result<(), GpuError> {
        for (reverse_z, blended) in VARIANTS {
            self.get_one(dev, color_format, reverse_z, blended)?;
        }
        Ok(())
    }

    /// The pipelines rendering into `color_format` (+ D32 depth, reversed or standard). They are
    /// normally prebuilt; one created here (an unexpected format) is counted against the warm mark.
    pub fn get(&mut self, dev: &Device, color_format: VkFormat, reverse_z: bool) -> Result<PipelinePair, GpuError> {
        Ok(PipelinePair {
            opaque: self.get_one(dev, color_format, reverse_z, false)?,
            blended: self.get_one(dev, color_format, reverse_z, true)?,
            layout: self.layout,
        })
    }

    fn get_one(&mut self, dev: &Device, color_format: VkFormat, reverse_z: bool, blended: bool) -> Result<VkPipeline, GpuError> {
        let key = (color_format, reverse_z, blended);
        if let Some(&(_, p)) = self.by_format.iter().find(|(k, _)| *k == key) {
            return Ok(p);
        }
        let start = Instant::now();
        let p = self.create(dev, color_format, reverse_z, blended)?;
        self.create_ms += start.elapsed().as_secs_f64() * 1e3;
        self.created += 1;
        self.by_format.push((key, p));
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

    fn create(&self, dev: &Device, color_format: VkFormat, reverse_z: bool, blended: bool) -> Result<VkPipeline, GpuError> {
        let entry = c"main";
        let stages = [
            VkPipelineShaderStageCreateInfo {
                s_type: ST_PIPELINE_SHADER_STAGE_CREATE_INFO,
                p_next: ptr::null(),
                flags: 0,
                stage: VK_SHADER_STAGE_VERTEX_BIT,
                module: self.vert,
                p_name: entry.as_ptr(),
                p_specialization_info: ptr::null(),
            },
            VkPipelineShaderStageCreateInfo {
                s_type: ST_PIPELINE_SHADER_STAGE_CREATE_INFO,
                p_next: ptr::null(),
                flags: 0,
                stage: VK_SHADER_STAGE_FRAGMENT_BIT,
                module: self.frag,
                p_name: entry.as_ptr(),
                p_specialization_info: ptr::null(),
            },
        ];
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
        ];
        let vertex_input = VkPipelineVertexInputStateCreateInfo {
            s_type: ST_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            vertex_binding_description_count: 1,
            p_vertex_binding_descriptions: &binding,
            vertex_attribute_description_count: attributes.len() as u32,
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
            depth_bias_enable: VK_FALSE,
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
            depth_test_enable: VK_TRUE,
            // Blended surfaces never hide what is behind them.
            depth_write_enable: if blended { VK_FALSE } else { VK_TRUE },
            depth_compare_op: if reverse_z { VK_COMPARE_OP_GREATER } else { VK_COMPARE_OP_LESS },
            depth_bounds_test_enable: VK_FALSE,
            stencil_test_enable: VK_FALSE,
            front: VkStencilOpState::default(),
            back: VkStencilOpState::default(),
            min_depth_bounds: 0.0,
            max_depth_bounds: 1.0,
        };
        // Blended materials output premultiplied colour: dst = src + dst × (1 − src.a).
        let blend_attachment = if blended {
            VkPipelineColorBlendAttachmentState {
                blend_enable: VK_TRUE,
                src_color_blend_factor: VK_BLEND_FACTOR_ONE,
                dst_color_blend_factor: VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
                color_blend_op: VK_BLEND_OP_ADD,
                src_alpha_blend_factor: VK_BLEND_FACTOR_ONE,
                dst_alpha_blend_factor: VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
                alpha_blend_op: VK_BLEND_OP_ADD,
                color_write_mask: VK_COLOR_COMPONENT_RGBA,
            }
        } else {
            VkPipelineColorBlendAttachmentState {
                blend_enable: VK_FALSE,
                color_write_mask: VK_COLOR_COMPONENT_RGBA,
                ..Default::default()
            }
        };
        let blend = VkPipelineColorBlendStateCreateInfo {
            s_type: ST_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            logic_op_enable: VK_FALSE,
            logic_op: 0,
            attachment_count: 1,
            p_attachments: &blend_attachment,
            blend_constants: [0.0; 4],
        };
        let dynamic_states = [VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR];
        let dynamic = VkPipelineDynamicStateCreateInfo {
            s_type: ST_PIPELINE_DYNAMIC_STATE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            dynamic_state_count: dynamic_states.len() as u32,
            p_dynamic_states: dynamic_states.as_ptr(),
        };
        let rendering = VkPipelineRenderingCreateInfo {
            s_type: ST_PIPELINE_RENDERING_CREATE_INFO,
            p_next: ptr::null(),
            view_mask: 0,
            color_attachment_count: 1,
            p_color_attachment_formats: &color_format,
            depth_attachment_format: DEPTH_FORMAT,
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
            p_depth_stencil_state: &depth_stencil,
            p_color_blend_state: &blend,
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
            return Err(GpuError::vk(&format!("vkCreateGraphicsPipelines (colour format {color_format}, blended {blended})"), r));
        }
        Ok(pipeline)
    }

    pub fn destroy(&mut self, dev: &Device) {
        // SAFETY: the device is idle; every object is destroyed once (null handles are ignored).
        unsafe {
            for (_, p) in self.by_format.drain(..) {
                (dev.fns.destroy_pipeline)(dev.handle, p, ptr::null());
            }
            (dev.fns.destroy_pipeline_cache)(dev.handle, self.cache, ptr::null());
            (dev.fns.destroy_pipeline_layout)(dev.handle, self.layout, ptr::null());
            (dev.fns.destroy_shader_module)(dev.handle, self.vert, ptr::null());
            (dev.fns.destroy_shader_module)(dev.handle, self.frag, ptr::null());
        }
        self.cache = VK_NULL_HANDLE;
        self.layout = VK_NULL_HANDLE;
        self.vert = VK_NULL_HANDLE;
        self.frag = VK_NULL_HANDLE;
    }
}

/// Fail early (before recording) if a draw names a mesh that does not exist or there are too
/// many lights.
pub fn check_frame(meshes: &HashMap<u32, Mesh>, frame: &FrameDesc) -> Result<(), GpuError> {
    if frame.lights.len() > MAX_POINT_LIGHTS {
        return Err(GpuError::new(format!("{} point lights given; at most {MAX_POINT_LIGHTS} per frame", frame.lights.len())));
    }
    match frame.draws.iter().find(|d| !meshes.contains_key(&d.mesh.0)) {
        Some(d) => Err(GpuError::new(format!("unknown mesh id {} in draw list", d.mesh.0))),
        None => Ok(()),
    }
}

/// Cull and batch `frame` against `meshes`, then write its frame data and draw records into
/// `upload` (growing it). The GPU must not be reading `upload`. `records` is scratch space.
pub fn prepare_frame(
    dev: &Device,
    meshes: &HashMap<u32, Mesh>,
    frame: &FrameDesc,
    extent: VkExtent2D,
    upload: &mut FrameUpload,
    records: &mut Vec<DrawRecord>,
) -> Result<Plan, GpuError> {
    let plan = crate::batch::plan(frame, |m| meshes.get(&m.0).map(|mesh| mesh.bounds));
    build_records(frame, &plan, records);
    upload.reserve(dev, records.len())?;
    upload.write(&frame_data(frame, extent), records);
    Ok(plan)
}

/// Where a frame is drawn: colour + depth views of `extent`.
pub struct RenderTarget {
    pub color_view: VkImageView,
    pub depth_view: VkImageView,
    pub depth_image: VkImage,
    pub extent: VkExtent2D,
}

/// Record the scene pass: depth layout transition, clear, one instanced draw per batch of `plan`.
/// The colour image must already be in `COLOR_ATTACHMENT_OPTIMAL` (the caller owns its
/// transitions); it stays there. `upload` holds the frame's data and records (`prepare_frame`).
/// With a `timer` (already reset for this frame) the pass is timestamped.
#[allow(clippy::too_many_arguments)]
pub fn record_scene(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipelines: PipelinePair,
    meshes: &HashMap<u32, Mesh>,
    target: &RenderTarget,
    frame: &FrameDesc,
    plan: &Plan,
    upload: &FrameUpload,
    timer: Option<&mut PassTimer>,
) -> DrawStats {
    let mut timer = timer;
    let pass = timer.as_mut().and_then(|t| t.begin_pass(dev, cmd, PASS_SCENE));
    // Depth contents are never kept between frames: discard them, but order the transition after
    // the previous frame's depth reads/writes (same image, possibly still in flight).
    let depth_stages = VK_PIPELINE_STAGE_2_EARLY_FRAGMENT_TESTS_BIT | VK_PIPELINE_STAGE_2_LATE_FRAGMENT_TESTS_BIT;
    dev.barrier(
        cmd,
        &[],
        &[image_barrier(
            target.depth_image,
            VK_IMAGE_ASPECT_DEPTH_BIT,
            depth_stages,
            VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT,
            depth_stages,
            VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_READ_BIT | VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT,
            VK_IMAGE_LAYOUT_UNDEFINED,
            VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL,
        )],
    );

    let clear = srgb_to_linear_rgba(frame.clear_color);
    let color_attachment = VkRenderingAttachmentInfo {
        s_type: ST_RENDERING_ATTACHMENT_INFO,
        p_next: ptr::null(),
        image_view: target.color_view,
        image_layout: VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
        resolve_mode: VK_RESOLVE_MODE_NONE,
        resolve_image_view: VK_NULL_HANDLE,
        resolve_image_layout: VK_IMAGE_LAYOUT_UNDEFINED,
        load_op: VK_ATTACHMENT_LOAD_OP_CLEAR,
        store_op: VK_ATTACHMENT_STORE_OP_STORE,
        clear_value: VkClearValue { float32: clear },
    };
    let depth_attachment = VkRenderingAttachmentInfo {
        s_type: ST_RENDERING_ATTACHMENT_INFO,
        p_next: ptr::null(),
        image_view: target.depth_view,
        image_layout: VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL,
        resolve_mode: VK_RESOLVE_MODE_NONE,
        resolve_image_view: VK_NULL_HANDLE,
        resolve_image_layout: VK_IMAGE_LAYOUT_UNDEFINED,
        load_op: VK_ATTACHMENT_LOAD_OP_CLEAR,
        store_op: VK_ATTACHMENT_STORE_OP_DONT_CARE,
        // VkClearDepthStencilValue { depth, stencil: 0 }: the farthest depth of the mode.
        clear_value: VkClearValue { float32: [if frame.reverse_z { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0] },
    };
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: target.extent };
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
        width: target.extent.width as f32,
        height: target.extent.height as f32,
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
        let mut bound = VK_NULL_HANDLE;
        let mut bound_mesh = None;
        for batch in &plan.batches {
            let Some(mesh) = meshes.get(&batch.mesh.0) else { continue };
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
        (dev.fns.cmd_end_rendering)(cmd);
    }
    if let Some(t) = timer {
        t.end_pass(dev, cmd, pass);
    }
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
        assert_eq!(size_of::<Vertex>(), 24);
    }

    #[test]
    fn draw_record_matches_shader_layout() {
        // std430 `Draw` in shaders/mesh.*: mat4 + 3 × vec4, array stride 112.
        assert_eq!(offset_of!(DrawRecord, color), 64);
        assert_eq!(offset_of!(DrawRecord, color2), 80);
        assert_eq!(offset_of!(DrawRecord, params), 96);
        assert_eq!(size_of::<DrawRecord>(), 112);
        assert_eq!(RECORDS_OFFSET, 768);
    }

    #[test]
    fn frame_data_matches_shader_layout() {
        // std430 offsets of `Frame` in shaders/mesh.*.
        assert_eq!(offset_of!(FrameData, camera_time), 64);
        assert_eq!(offset_of!(FrameData, viewport), 144);
        assert_eq!(offset_of!(FrameData, lights), 160);
        assert_eq!(size_of::<FrameData>(), 160 + 32 * MAX_POINT_LIGHTS);
    }

    #[test]
    fn default_frame_data_reproduces_m0_lighting() {
        let frame = FrameDesc::default();
        let d = frame_data(&frame, VkExtent2D { width: 4, height: 2 });
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
        let plan = Plan { entries: vec![(2, 0), (1, 0), (0, 0), (0, 1)], ..Default::default() };
        let mut out = Vec::new();
        build_records(&frame, &plan, &mut out);
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
        for spv in [VERT_SPV, FRAG_SPV] {
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
