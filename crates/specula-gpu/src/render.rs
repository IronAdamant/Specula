//! The M0 mesh pipeline: shaders, pipeline creation, per-draw constants and scene recording.

use crate::device::{image_barrier, Buffer, Device};
use crate::vk::*;
use crate::{FrameDesc, GpuError, Vertex};
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
    /// projection × view × model, column-major.
    pub mvp: [f32; 16],
    /// Columns of inverse-transpose(model 3×3); w unused.
    pub normal_matrix: [[f32; 4]; 3],
    /// Linear RGB; a unused.
    pub color: [f32; 4],
}

const PUSH_STAGES: VkFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;

/// An uploaded mesh: vertices at offset 0, `u32` indices at `index_offset`, one device-local buffer.
pub struct Mesh {
    pub buffer: Buffer,
    pub index_offset: u64,
    pub index_count: u32,
}

/// Shader modules, layout and one pipeline per colour format.
pub struct Pipelines {
    vert: VkShaderModule,
    frag: VkShaderModule,
    pub layout: VkPipelineLayout,
    by_format: Vec<(VkFormat, VkPipeline)>,
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

    /// The pipeline rendering into `color_format` (+ D32 depth), created on first use.
    pub fn get(&mut self, dev: &Device, color_format: VkFormat) -> Result<VkPipeline, GpuError> {
        if let Some(&(_, p)) = self.by_format.iter().find(|(f, _)| *f == color_format) {
            return Ok(p);
        }
        let p = self.create(dev, color_format)?;
        self.by_format.push((color_format, p));
        Ok(p)
    }

    fn create(&self, dev: &Device, color_format: VkFormat) -> Result<VkPipeline, GpuError> {
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
            depth_write_enable: VK_TRUE,
            depth_compare_op: VK_COMPARE_OP_LESS,
            depth_bounds_test_enable: VK_FALSE,
            stencil_test_enable: VK_FALSE,
            front: VkStencilOpState::default(),
            back: VkStencilOpState::default(),
            min_depth_bounds: 0.0,
            max_depth_bounds: 1.0,
        };
        let blend_attachment = VkPipelineColorBlendAttachmentState {
            blend_enable: VK_FALSE,
            color_write_mask: VK_COLOR_COMPONENT_RGBA,
            ..Default::default()
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
            return Err(GpuError::vk(&format!("vkCreateGraphicsPipelines (colour format {color_format})"), r));
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

/// Fail early (before recording) if a draw names a mesh that does not exist.
pub fn check_meshes(meshes: &HashMap<u32, Mesh>, frame: &FrameDesc) -> Result<(), GpuError> {
    match frame.draws.iter().find(|d| !meshes.contains_key(&d.mesh.0)) {
        Some(d) => Err(GpuError::new(format!("unknown mesh id {} in draw list", d.mesh.0))),
        None => Ok(()),
    }
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
pub fn record_scene(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipeline: VkPipeline,
    layout: VkPipelineLayout,
    meshes: &HashMap<u32, Mesh>,
    target: &RenderTarget,
    frame: &FrameDesc,
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
        // VkClearDepthStencilValue { depth: 1.0, stencil: 0 }
        clear_value: VkClearValue { float32: [1.0, 0.0, 0.0, 0.0] },
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
    // layout are compatible; every mesh buffer is live (checked by `check_meshes`) and the GPU
    // does not outlive it (callers wait before destroying meshes).
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        for draw in frame.draws {
            let Some(mesh) = meshes.get(&draw.mesh.0) else { continue };
            let pc = PushConstants {
                mvp: mat4_mul(&frame.view_proj, &draw.model),
                normal_matrix: normal_matrix(&draw.model),
                color: srgb_to_linear_rgba(draw.color),
            };
            (dev.fns.cmd_push_constants)(
                cmd,
                layout,
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
// Small CPU math (kept local: specula-core math is implemented separately)
// ---------------------------------------------------------------------------------------------

/// Column-major `a * b` (applies `b` first).
pub fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Columns of inverse-transpose of the model's upper 3×3, i.e. `(b×c, c×a, a×b) / det` for
/// columns `a, b, c`. A singular matrix yields the (unscaled) cofactor matrix instead of infinities;
/// the shader normalizes anyway.
pub fn normal_matrix(model: &[f32; 16]) -> [[f32; 4]; 3] {
    let col = |c: usize| [model[c * 4], model[c * 4 + 1], model[c * 4 + 2]];
    let (a, b, c) = (col(0), col(1), col(2));
    let (n0, n1, n2) = (cross(b, c), cross(c, a), cross(a, b));
    let det = a[0] * n0[0] + a[1] * n0[1] + a[2] * n0[2];
    let s = if det.abs() > 1e-20 { 1.0 / det } else { 1.0 };
    let v = |n: [f32; 3]| [n[0] * s, n[1] * s, n[2] * s, 0.0];
    [v(n0), v(n1), v(n2)]
}

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
        assert_eq!(offset_of!(PushConstants, normal_matrix), 64);
        assert_eq!(offset_of!(PushConstants, color), 112);
        assert_eq!(size_of::<Vertex>(), 24);
    }

    #[test]
    fn embedded_spirv_is_valid_shape() {
        for spv in [VERT_SPV, FRAG_SPV] {
            assert_eq!(spv.len() % 4, 0);
            assert_eq!(spirv_words(spv)[0], 0x0723_0203, "SPIR-V magic");
        }
    }

    #[test]
    fn mat4_mul_composes_column_major() {
        // Translation by (1,2,3) then scale by 2: S * T moves (0,0,0) to (2,4,6).
        let mut t = [0.0; 16];
        let mut s = [0.0; 16];
        for i in 0..4 {
            t[i * 5] = 1.0;
            s[i * 5] = if i < 3 { 2.0 } else { 1.0 };
        }
        t[12] = 1.0;
        t[13] = 2.0;
        t[14] = 3.0;
        let m = mat4_mul(&s, &t);
        assert_eq!(&m[12..16], &[2.0, 4.0, 6.0, 1.0]);
        assert_eq!(m[0], 2.0);
    }

    #[test]
    fn normal_matrix_handles_nonuniform_scale() {
        // Scale x by 2: normals' x component must shrink (inverse-transpose = diag(1/2, 1, 1)).
        let mut m = [0.0; 16];
        m[0] = 2.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        let n = normal_matrix(&m);
        assert!((n[0][0] - 0.5).abs() < 1e-6);
        assert!((n[1][1] - 1.0).abs() < 1e-6);
        assert!((n[2][2] - 1.0).abs() < 1e-6);
        assert_eq!(n[0][1], 0.0);
    }

    #[test]
    fn srgb_conversion_endpoints() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!((srgb_to_linear(0.5) - 0.214_041).abs() < 1e-5);
        assert_eq!(srgb_to_linear(f32::NAN), 0.0);
    }
}
