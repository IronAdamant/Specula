//! The mesh pipelines: shaders, pipeline creation, per-frame data, per-draw constants and scene
//! recording.

use crate::device::{image_barrier, Buffer, Device};
use crate::vk::*;
use crate::{FrameDesc, GpuError, Material, Vertex, MAX_POINT_LIGHTS};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{offset_of, size_of};
use std::ptr;

static VERT_SPV: &[u8] = include_bytes!("../shaders/mesh.vert.spv");
static FRAG_SPV: &[u8] = include_bytes!("../shaders/mesh.frag.spv");

pub const DEPTH_FORMAT: VkFormat = VK_FORMAT_D32_SFLOAT;

/// Push constants shared by both stages; must match the `Push` block in `shaders/mesh.*`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PushConstants {
    /// Object → world, column-major.
    pub model: [f32; 16],
    /// Linear RGB; a = opacity.
    pub color: [f32; 4],
    /// Linear RGB secondary colour; a = seed.
    pub color2: [f32; 4],
    /// emissive, metallic, roughness, specular.
    pub params: [f32; 4],
    /// Device address of this frame's [`FrameData`].
    pub frame: u64,
    /// [`Material`] as `MAT_*` in the shader.
    pub material: u32,
    /// Glass: 0 = back faces, 1 = front faces.
    pub pass: u32,
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

/// Write `frame`'s data into a persistently mapped buffer of at least [`FRAME_DATA_SIZE`] bytes.
pub fn write_frame_data(buffer: &Buffer, frame: &FrameDesc, extent: VkExtent2D) {
    let data = frame_data(frame, extent);
    // SAFETY: `buffer.mapped` is coherent host memory of at least FRAME_DATA_SIZE bytes that the
    // GPU no longer reads (callers wait for the frame that last used it); `FrameData` is plain data.
    unsafe {
        ptr::copy_nonoverlapping(&data as *const FrameData as *const u8, buffer.mapped as *mut u8, size_of::<FrameData>())
    };
}

/// A host-visible buffer for one frame's [`FrameData`] and its device address.
pub fn create_frame_buffer(dev: &Device) -> Result<(Buffer, u64), GpuError> {
    let buffer = dev.create_buffer(
        FRAME_DATA_SIZE,
        VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT,
        VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
        VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT,
        "frame data",
    )?;
    let address = dev.buffer_address(&buffer);
    Ok((buffer, address))
}

const PUSH_STAGES: VkFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;

/// An uploaded mesh: vertices at offset 0, `u32` indices at `index_offset`, one device-local buffer.
pub struct Mesh {
    pub buffer: Buffer,
    pub index_offset: u64,
    pub index_count: u32,
}

/// Shader modules, layout and one pipeline per (colour format, depth mode, blended).
pub struct Pipelines {
    vert: VkShaderModule,
    frag: VkShaderModule,
    pub layout: VkPipelineLayout,
    by_format: Vec<((VkFormat, bool, bool), VkPipeline)>,
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

impl Pipelines {
    pub fn new(dev: &Device) -> Result<Pipelines, GpuError> {
        let mut p = Pipelines { vert: VK_NULL_HANDLE, frag: VK_NULL_HANDLE, layout: VK_NULL_HANDLE, by_format: Vec::new() };
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

    /// The pipelines rendering into `color_format` (+ D32 depth, reversed or standard), created
    /// on first use.
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
        let p = self.create(dev, color_format, reverse_z, blended)?;
        self.by_format.push((key, p));
        Ok(p)
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
        // SAFETY: every state struct referenced by `info` is a local alive for the call.
        let r = unsafe { (dev.fns.create_graphics_pipelines)(dev.handle, VK_NULL_HANDLE, 1, &info, ptr::null(), &mut pipeline) };
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
            (dev.fns.destroy_pipeline_layout)(dev.handle, self.layout, ptr::null());
            (dev.fns.destroy_shader_module)(dev.handle, self.vert, ptr::null());
            (dev.fns.destroy_shader_module)(dev.handle, self.frag, ptr::null());
        }
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

/// Draw order: opaque draws as given, then blended ones far to near (by model origin). Glass
/// appears twice: back faces (pass 0), then front faces (pass 1). Blended draws sharing an origin
/// are treated as nested shells listed outermost first: all their back faces in list order, then
/// their front faces in reverse, so an inner liquid sits inside its outer glass. Returns
/// (draw index, pass).
pub fn draw_order(frame: &FrameDesc) -> Vec<(usize, u32)> {
    let mut order: Vec<(usize, u32)> =
        frame.draws.iter().enumerate().filter(|(_, d)| !d.material.blended()).map(|(i, _)| (i, 0)).collect();
    let c = camera_position(&frame.view_proj);
    let dist2 = |i: usize| {
        let m = &frame.draws[i].model;
        let d = [m[12] - c[0], m[13] - c[1], m[14] - c[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    let mut blended: Vec<usize> = (0..frame.draws.len()).filter(|&i| frame.draws[i].material.blended()).collect();
    blended.sort_by(|&a, &b| dist2(b).total_cmp(&dist2(a))); // stable: ties keep list order
    for group in blended.chunk_by(|&a, &b| dist2(a) == dist2(b)) {
        order.extend(group.iter().map(|&i| (i, 0)));
        let glass = group.iter().rev().filter(|&&i| frame.draws[i].material == Material::Glass);
        order.extend(glass.map(|&i| (i, 1)));
    }
    order
}

/// Where a frame is drawn: colour + depth views of `extent`.
pub struct RenderTarget {
    pub color_view: VkImageView,
    pub depth_view: VkImageView,
    pub depth_image: VkImage,
    pub extent: VkExtent2D,
}

/// Record: depth layout transition, clear, draw every item. The colour image must already be in
/// `COLOR_ATTACHMENT_OPTIMAL` (the caller owns its transitions); it stays there.
///
/// `frame_address` is the device address of this frame's [`FrameData`], already written.
pub fn record_scene(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipelines: PipelinePair,
    meshes: &HashMap<u32, Mesh>,
    target: &RenderTarget,
    frame: &FrameDesc,
    frame_address: u64,
) {
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
    // SAFETY: `cmd` is recording; all referenced structs outlive each call; the pipeline and
    // layout are compatible; every mesh buffer is live (checked by `check_frame`) and the GPU
    // does not outlive it (callers wait before destroying meshes).
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        let mut bound = VK_NULL_HANDLE;
        for (i, pass) in draw_order(frame) {
            let draw = &frame.draws[i];
            let Some(mesh) = meshes.get(&draw.mesh.0) else { continue };
            let pipeline = if draw.material.blended() { pipelines.blended } else { pipelines.opaque };
            if pipeline != bound {
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
                bound = pipeline;
            }
            let s = &draw.surface;
            let c2 = srgb_to_linear_rgba([s.color2[0], s.color2[1], s.color2[2], 1.0]);
            let pc = PushConstants {
                model: draw.model,
                color: srgb_to_linear_rgba(draw.color),
                color2: [c2[0], c2[1], c2[2], s.seed],
                params: [s.emissive.max(0.0), s.metallic.clamp(0.0, 1.0), s.roughness.clamp(0.0, 1.0), s.specular.max(0.0)],
                frame: frame_address,
                material: material_id(draw.material),
                pass,
            };
            (dev.fns.cmd_push_constants)(
                cmd,
                pipelines.layout,
                PUSH_STAGES,
                0,
                size_of::<PushConstants>() as u32,
                &pc as *const PushConstants as *const c_void,
            );
            let offset: VkDeviceSize = 0;
            (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &mesh.buffer.handle, &offset);
            (dev.fns.cmd_bind_index_buffer)(cmd, mesh.buffer.handle, mesh.index_offset, VK_INDEX_TYPE_UINT32);
            (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, 1, 0, 0, 0);
        }
        (dev.fns.cmd_end_rendering)(cmd);
    }
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

    #[test]
    fn push_constants_fit_128_bytes() {
        assert_eq!(size_of::<PushConstants>(), 128);
        assert_eq!(offset_of!(PushConstants, color), 64);
        assert_eq!(offset_of!(PushConstants, params), 96);
        assert_eq!(offset_of!(PushConstants, frame), 112);
        assert_eq!(offset_of!(PushConstants, pass), 124);
        assert_eq!(size_of::<Vertex>(), 24);
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

    /// A perspective camera at (0, 0, z) looking down −Z (Vulkan clip, reversed Z, near 0.05).
    fn look_from_z(z: f32) -> [f32; 16] {
        // proj (column-major): x' = x, y' = −y, z' = 0.05, w' = −z_view; view = translate(0, 0, −z).
        let proj = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 0.0];
        let mut vp = proj;
        // proj · translate: column 3 += proj · (0, 0, −z, 0).
        for r in 0..4 {
            vp[12 + r] += proj[8 + r] * -z;
        }
        vp
    }

    #[test]
    fn camera_position_comes_from_view_proj() {
        let p = camera_position(&look_from_z(5.0));
        assert!((p[0]).abs() < 1e-5 && (p[1]).abs() < 1e-5 && (p[2] - 5.0).abs() < 1e-5, "{p:?}");
        assert_eq!(camera_position(&[0.0; 16]), [0.0; 3], "singular falls back to the origin");
    }

    #[test]
    fn blended_draws_follow_opaque_far_to_near() {
        let at = |z: f32| {
            let mut m = [0.0; 16];
            m[0] = 1.0;
            m[5] = 1.0;
            m[10] = 1.0;
            m[15] = 1.0;
            m[14] = z;
            m
        };
        let mesh = crate::MeshId(1);
        let mut glass = crate::DrawItem::new(mesh, at(-2.0), [1.0; 4]);
        glass.material = Material::Glass;
        let mut lamp = crate::DrawItem::new(mesh, at(-5.0), [1.0; 4]);
        lamp.material = Material::LavaLamp;
        let draws = [glass, crate::DrawItem::new(mesh, at(-1.0), [1.0; 4]), lamp];
        let frame = FrameDesc { draws: &draws, ..Default::default() };
        assert_eq!(draw_order(&frame), vec![(1, 0), (2, 0), (0, 0), (0, 1)]);
    }

    #[test]
    fn nested_glass_shells_draw_inner_inside_outer() {
        let mut m = [0.0; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        let mut outer = crate::DrawItem::new(crate::MeshId(1), m, [1.0; 4]);
        outer.material = Material::Glass;
        let inner = outer;
        let draws = [outer, inner];
        let frame = FrameDesc { draws: &draws, view_proj: look_from_z(5.0), ..Default::default() };
        assert_eq!(draw_order(&frame), vec![(0, 0), (1, 0), (1, 1), (0, 1)]);
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
