//! `Gpu` internals: adapter selection, meshes, offscreen targets, readback, window plumbing.

use crate::batch::mesh_bounds;
use crate::device::{buffer_barrier, image_barrier, Buffer, Device, Image};
use crate::instance::Instance;
use crate::descriptors::Descriptors;
use crate::graph::{Final, GraphInfo, Prior, TransientPool};
use crate::render::{
    check_frame, declare_frame, frame_features, prepare_frame, prepare_graph, record_frame, CacheIdentity, FrameContext, FrameGraph,
    FrameScratch, FrameShape, FrameUpload, Mesh, Pipelines,
};
use crate::overlay::{check_pixels, vk_format, Texture, Textures};
use crate::post::PostMode;
use crate::shadow::shadow_desc;
use crate::swapchain::{Window, WINDOW_OWNER};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{
    AdapterInfo, AdapterKind, AdapterPreference, CpuTiming, FrameDesc, FramePath, FrameStats, GpuError, GpuOptions, MemoryKind, MeshId,
    TargetId, TextureFormat, TextureId, ValidationMessage, Vertex,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::time::Instant;

/// Offscreen colour format (readback bytes are sRGB RGBA8).
pub const TARGET_FORMAT: VkFormat = crate::render::LDR_FORMAT;

/// Where `run_sync` spent its time, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
struct SyncTimes {
    record_ms: f64,
    submit_ms: f64,
    wait_ms: f64,
}

struct Target {
    color: Image,
    readback: Buffer,
    extent: VkExtent2D,
    /// Its frame graph; the depth buffer and post-chain images are the graph's transients.
    graph: FrameGraph,
    /// False until the first `render`; the colour image then sits in TRANSFER_SRC_OPTIMAL.
    rendered: bool,
}

/// Whose graph rendered the latest frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphSource {
    Window,
    Target(u32),
}

pub struct GpuInner {
    adapter: AdapterInfo,
    max_image_dimension_2d: u32,
    cmd_pool: VkCommandPool,
    /// Command buffer + fence for synchronous work (uploads, offscreen renders, readback).
    sync_cmd: VkCommandBuffer,
    sync_fence: VkFence,
    /// Frame data and draw records for offscreen renders (synchronous, so one buffer suffices).
    upload: Option<FrameUpload>,
    /// Timestamps for offscreen renders (`None` without timestamp support).
    timer: Option<PassTimer>,
    /// Draw-record scratch and the sRGB conversion cache, shared by both paths.
    scratch: FrameScratch,
    pipelines: Pipelines,
    /// The global descriptor set (shadow map, bindless textures).
    descriptors: Descriptors,
    /// Images the frame graphs own: depth buffers, post-chain images, the shadow map.
    pool: TransientPool,
    /// Every frame graph submission numbered this or lower ([`TransientPool::note_used`]) has
    /// finished on the GPU.
    completed: u64,
    last_graph: Option<GraphSource>,
    meshes: HashMap<u32, Mesh>,
    textures: Textures,
    targets: HashMap<u32, Target>,
    next_id: u32,
    window: Option<Window>,
    /// Frames rendered or presented so far (shared counter).
    frame_no: u64,
    /// Latest frame stats per path (`frame` 0 = none yet), updated in place each frame;
    /// pipelines and memory are filled in when asked.
    last_offscreen: FrameStats,
    last_window: FrameStats,
    // Dropped after `Drop::drop` released everything above, device before instance.
    device: Device,
    instance: Instance,
}

fn rank(kind: AdapterKind, pref: AdapterPreference) -> u32 {
    let (first, second) = match pref {
        AdapterPreference::LowPower => (AdapterKind::Integrated, AdapterKind::Discrete),
        _ => (AdapterKind::Discrete, AdapterKind::Integrated),
    };
    match kind {
        k if k == first => 0,
        k if k == second => 1,
        AdapterKind::Virtual => 2,
        AdapterKind::Other => 3,
        AdapterKind::Cpu => 4,
        _ => 5,
    }
}

/// Pick the physical device for `pref`: `(index, handle, graphics queue family)`.
fn select_adapter(instance: &Instance, pref: AdapterPreference) -> Result<(usize, VkPhysicalDevice, u32), GpuError> {
    let devices = instance.physical_devices()?;
    if devices.is_empty() {
        return Err(GpuError::new("no Vulkan adapters found"));
    }
    let describe = |i: usize, why: &str| format!("adapter {i} ({}): {why}", instance.adapter_info(i, devices[i]).name);
    if let AdapterPreference::Index(i) = pref {
        let Some(&pd) = devices.get(i) else {
            return Err(GpuError::new(format!("adapter index {i} out of range ({} adapters)", devices.len())));
        };
        let family = instance.check_suitable(pd).map_err(|why| GpuError::new(describe(i, &why)))?;
        return Ok((i, pd, family));
    }
    let mut order: Vec<usize> = (0..devices.len()).collect();
    order.sort_by_key(|&i| rank(instance.adapter_info(i, devices[i]).kind, pref)); // stable
    let mut reasons = Vec::new();
    for i in order {
        match instance.check_suitable(devices[i]) {
            Ok(family) => return Ok((i, devices[i], family)),
            Err(why) => reasons.push(describe(i, &why)),
        }
    }
    Err(GpuError::new(format!("no suitable Vulkan 1.3 adapter: {}", reasons.join("; "))))
}

/// Record into `cmd` with `record`, submit with `fence`, and wait for completion (see
/// [`GpuInner::run_sync`]).
fn run_sync(dev: &Device, cmd: VkCommandBuffer, fence: VkFence, record: impl FnOnce(&Device, VkCommandBuffer)) -> Result<SyncTimes, GpuError> {
    let t0 = Instant::now();
    dev.begin(cmd)?;
    record(dev, cmd);
    dev.end(cmd)?;
    let t1 = Instant::now();
    // A failed submit queues nothing and leaves the fence untouched.
    dev.submit(cmd, None, None, fence)?;
    let t2 = Instant::now();
    let waited = dev.wait_fence(fence);
    if waited.is_err() {
        // Timed out (work may still be running) or the device was lost: drain the queue before
        // anyone frees the resources this work touches.
        dev.wait_idle();
    }
    let t3 = Instant::now();
    let reset = dev.reset_fence(fence);
    let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1e3;
    waited.and(reset).map(|()| SyncTimes { record_ms: ms(t0, t1), submit_ms: ms(t1, t2), wait_ms: ms(t2, t3) })
}

impl GpuInner {
    pub fn new(options: &GpuOptions) -> Result<GpuInner, GpuError> {
        let instance = Instance::new(options.validation, true)?;
        let (index, physical, family) = select_adapter(&instance, options.adapter)?;
        let adapter = instance.adapter_info(index, physical);
        let props = instance.properties(physical);
        let max_image_dimension_2d = props.limits.max_image_dimension_2d;
        let device = Device::new(&instance, physical, family)?;
        let id = CacheIdentity { vendor_id: props.vendor_id, device_id: props.device_id, uuid: props.pipeline_cache_uuid };
        // Linear depth-compare filtering is optional for D16; without it shadows filter nearest.
        let shadow_linear = instance.format_features(physical, crate::shadow::SHADOW_FORMAT).optimal_tiling_features
            & VK_FORMAT_FEATURE_SAMPLED_IMAGE_FILTER_LINEAR_BIT
            != 0;
        let mut descriptors = Descriptors::new(&device, shadow_linear)?;
        let pipelines = match Pipelines::new(&device, descriptors.layout, options.pipeline_cache.as_deref(), &id) {
            Ok(p) => p,
            Err(e) => {
                descriptors.destroy(&device);
                return Err(e);
            }
        };
        let mut gpu = GpuInner {
            adapter,
            max_image_dimension_2d,
            cmd_pool: VK_NULL_HANDLE,
            sync_cmd: ptr::null_mut(),
            sync_fence: VK_NULL_HANDLE,
            upload: None,
            timer: None,
            scratch: FrameScratch::default(),
            pipelines,
            descriptors,
            pool: TransientPool::default(),
            completed: 0,
            last_graph: None,
            meshes: HashMap::new(),
            textures: Textures::default(),
            targets: HashMap::new(),
            next_id: 1,
            window: None,
            frame_no: 0,
            last_offscreen: FrameStats::default(),
            last_window: FrameStats::default(),
            device,
            instance,
        };
        // On error `gpu` drops and releases whatever was created.
        gpu.cmd_pool = gpu.device.create_command_pool()?;
        gpu.sync_cmd = gpu.device.allocate_command_buffer(gpu.cmd_pool)?;
        gpu.sync_fence = gpu.device.create_fence(false)?;
        gpu.upload = Some(FrameUpload::new(&gpu.device)?);
        gpu.timer = PassTimer::new(&gpu.device)?;
        // Every offscreen pipeline variant now, so no frame ever waits for one.
        gpu.pipelines.prebuild(&gpu.device, TARGET_FORMAT)?;
        gpu.pipelines.prebuild_common(&gpu.device)?;
        Ok(gpu)
    }

    pub fn adapter(&self) -> &AdapterInfo {
        &self.adapter
    }

    pub fn validation_enabled(&self) -> bool {
        self.instance.validation_enabled()
    }

    pub fn take_validation_messages(&mut self) -> Vec<ValidationMessage> {
        self.instance.take_messages()
    }

    /// Tear everything down, then return the messages still pending, including teardown ones.
    pub fn close(self) -> Vec<ValidationMessage> {
        let sink = self.instance.sink();
        drop(self);
        sink.map_or_else(Vec::new, |s| s.take())
    }

    fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// Record with `record`, submit, and wait for completion.
    ///
    /// On return — success or error — nothing submitted here is still running, so callers may free
    /// what the commands used, and the fence is unsignaled for the next call.
    fn run_sync(&self, record: impl FnOnce(&Device, VkCommandBuffer)) -> Result<SyncTimes, GpuError> {
        run_sync(&self.device, self.sync_cmd, self.sync_fence, record)
    }

    pub fn upload_mesh(&mut self, vertices: &[Vertex], indices: &[u32]) -> Result<MeshId, GpuError> {
        if vertices.is_empty() || indices.is_empty() {
            return Err(GpuError::new(format!(
                "upload_mesh: need at least one vertex and one triangle (got {} vertices, {} indices)",
                vertices.len(),
                indices.len()
            )));
        }
        if indices.len() % 3 != 0 {
            return Err(GpuError::new(format!("upload_mesh: index count {} is not a multiple of 3", indices.len())));
        }
        if let Some((at, &bad)) = indices.iter().enumerate().find(|(_, &i)| i as usize >= vertices.len()) {
            return Err(GpuError::new(format!(
                "upload_mesh: index {bad} at position {at} is out of range ({} vertices)",
                vertices.len()
            )));
        }
        let vb_bytes = std::mem::size_of_val(vertices) as u64;
        let ib_bytes = std::mem::size_of_val(indices) as u64;
        let total = vb_bytes + ib_bytes;
        let dev = &self.device;

        let staging = dev.create_buffer(
            total,
            VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            0,
            MemoryKind::Staging,
            "mesh staging",
        )?;
        // SAFETY: `staging.mapped` points to at least `total` bytes of coherent host memory;
        // `Vertex` is `repr(C)` plain f32 data and `u32` indices are plain data; regions don't overlap.
        unsafe {
            let dst = staging.mapped as *mut u8;
            ptr::copy_nonoverlapping(vertices.as_ptr() as *const u8, dst, vb_bytes as usize);
            ptr::copy_nonoverlapping(indices.as_ptr() as *const u8, dst.add(vb_bytes as usize), ib_bytes as usize);
        }
        let buffer = match dev.create_buffer(
            total,
            VK_BUFFER_USAGE_VERTEX_BUFFER_BIT | VK_BUFFER_USAGE_INDEX_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT,
            VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT,
            0,
            MemoryKind::Meshes,
            "mesh",
        ) {
            Ok(b) => b,
            Err(e) => {
                dev.destroy_buffer(&staging);
                return Err(e);
            }
        };
        let copied = self.run_sync(|dev, cmd| {
            let region = VkBufferCopy { src_offset: 0, dst_offset: 0, size: total };
            // SAFETY: recording; both buffers are live and at least `total` bytes.
            unsafe { (dev.fns.cmd_copy_buffer)(cmd, staging.handle, buffer.handle, 1, &region) };
            dev.barrier(
                cmd,
                &[buffer_barrier(
                    buffer.handle,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_WRITE_BIT,
                    VK_PIPELINE_STAGE_2_VERTEX_ATTRIBUTE_INPUT_BIT | VK_PIPELINE_STAGE_2_INDEX_INPUT_BIT,
                    VK_ACCESS_2_VERTEX_ATTRIBUTE_READ_BIT | VK_ACCESS_2_INDEX_READ_BIT,
                )],
                &[],
            );
        });
        self.device.destroy_buffer(&staging);
        if let Err(e) = copied {
            self.device.destroy_buffer(&buffer);
            return Err(e);
        }
        let id = self.next_id();
        let bounds = mesh_bounds(vertices);
        self.meshes.insert(id, Mesh { buffer, index_offset: vb_bytes, index_count: indices.len() as u32, bounds });
        Ok(MeshId(id))
    }

    pub fn destroy_mesh(&mut self, mesh: MeshId) {
        let Some(m) = self.meshes.remove(&mesh.0) else { return };
        // A presented frame may still read the mesh.
        if let Some(w) = &self.window {
            if w.wait_frames(&self.device).is_err() {
                self.device.wait_idle();
            }
        }
        self.device.destroy_buffer(&m.buffer);
    }

    /// Wait until no window frame in flight can still read a resource about to be freed.
    fn wait_window_frames(&self) {
        if let Some(w) = &self.window {
            if w.wait_frames(&self.device).is_err() {
                self.device.wait_idle();
            }
        }
    }

    /// Copy `pixels` (tightly packed) into the `width × height` region at (`x`, `y`) of `image`,
    /// which is in `SHADER_READ_ONLY_OPTIMAL` (or `UNDEFINED` when `fresh`) and ends there.
    #[allow(clippy::too_many_arguments)]
    fn copy_to_image(
        &self,
        image: VkImage,
        fresh: bool,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), GpuError> {
        let dev = &self.device;
        let staging = dev.create_buffer(
            pixels.len() as u64,
            VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            0,
            MemoryKind::Staging,
            "texture staging",
        )?;
        // SAFETY: `staging.mapped` points to at least `pixels.len()` bytes of coherent host memory.
        unsafe { ptr::copy_nonoverlapping(pixels.as_ptr(), staging.mapped as *mut u8, pixels.len()) };
        let copied = self.run_sync(|dev, cmd| {
            // Frames submitted earlier (same queue) finish reading before the copy writes.
            let (old, src_stage, src_access) = if fresh {
                (VK_IMAGE_LAYOUT_UNDEFINED, VK_PIPELINE_STAGE_2_NONE, VK_ACCESS_2_NONE)
            } else {
                (VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT, VK_ACCESS_2_NONE)
            };
            dev.barrier(
                cmd,
                &[],
                &[image_barrier(
                    image,
                    VK_IMAGE_ASPECT_COLOR_BIT,
                    src_stage,
                    src_access,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_WRITE_BIT,
                    old,
                    VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                )],
            );
            let region = VkBufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: VkImageSubresourceLayers {
                    aspect_mask: VK_IMAGE_ASPECT_COLOR_BIT,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: VkOffset3D { x: x as i32, y: y as i32, z: 0 },
                image_extent: VkExtent3D { width, height, depth: 1 },
            };
            // SAFETY: recording; the buffer holds the tightly packed region, which lies inside
            // the image (checked by the callers); the image is in TRANSFER_DST_OPTIMAL.
            unsafe {
                (dev.fns.cmd_copy_buffer_to_image)(cmd, staging.handle, image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &region)
            };
            dev.barrier(
                cmd,
                &[],
                &[image_barrier(
                    image,
                    VK_IMAGE_ASPECT_COLOR_BIT,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_WRITE_BIT,
                    VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
                    VK_ACCESS_2_SHADER_SAMPLED_READ_BIT,
                    VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                    VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
                )],
            );
        });
        self.device.destroy_buffer(&staging);
        copied.map(|_| ())
    }

    pub fn upload_texture(&mut self, width: u32, height: u32, format: TextureFormat, pixels: &[u8]) -> Result<TextureId, GpuError> {
        check_pixels("upload_texture", width, height, format, pixels)?;
        let max = self.max_image_dimension_2d;
        if width > max || height > max {
            return Err(GpuError::rejected(
                crate::ErrorReason::TooLarge { width, height, max },
                format!("upload_texture: size {width}x{height} exceeds {max} in a dimension"),
            ));
        }
        let slot = self.descriptors.alloc_slot()?;
        let image = match self.device.create_image(
            vk_format(format),
            width,
            height,
            VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT,
            VK_IMAGE_ASPECT_COLOR_BIT,
            MemoryKind::Textures,
            "texture",
        ) {
            Ok(i) => i,
            Err(e) => {
                self.descriptors.free_slot(slot);
                return Err(e);
            }
        };
        if let Err(e) = self.copy_to_image(image.handle, true, 0, 0, width, height, pixels) {
            self.device.destroy_image(&image);
            self.descriptors.free_slot(slot);
            return Err(e);
        }
        // A fresh slot: no pending frame reads it.
        self.descriptors.write_texture(&self.device, slot, image.view);
        let id = self.next_id();
        self.textures.map.insert(id, Texture { image, slot, format, width, height });
        Ok(TextureId(id))
    }

    pub fn update_texture(&mut self, texture: TextureId, x: u32, y: u32, width: u32, height: u32, pixels: &[u8]) -> Result<(), GpuError> {
        let t = self.textures.map.get(&texture.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Texture, texture.0, ""))?;
        check_pixels("update_texture", width, height, t.format, pixels)?;
        if x as u64 + width as u64 > t.width as u64 || y as u64 + height as u64 > t.height as u64 {
            let reason =
                crate::ErrorReason::RegionOutside { x, y, width, height, texture_width: t.width, texture_height: t.height };
            return Err(GpuError::rejected(
                reason,
                format!("update_texture: region {width}x{height} at ({x}, {y}) leaves the {}x{} texture", t.width, t.height),
            ));
        }
        self.copy_to_image(t.image.handle, false, x, y, width, height, pixels)
    }

    pub fn destroy_texture(&mut self, texture: TextureId) {
        let Some(t) = self.textures.map.remove(&texture.0) else { return };
        self.wait_window_frames();
        self.device.destroy_image(&t.image);
        self.descriptors.free_slot(t.slot);
    }

    pub fn create_target(&mut self, width: u32, height: u32) -> Result<TargetId, GpuError> {
        let max = self.max_image_dimension_2d;
        if width == 0 || height == 0 || width > max || height > max {
            return Err(GpuError::new(format!("create_target: size {width}x{height} must be 1..={max} in each dimension")));
        }
        let dev = &self.device;
        let color = dev.create_image(
            TARGET_FORMAT,
            width,
            height,
            VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
            VK_IMAGE_ASPECT_COLOR_BIT,
            MemoryKind::RenderTargets,
            "target colour",
        )?;
        let readback = match dev.create_buffer(
            width as u64 * height as u64 * 4,
            VK_BUFFER_USAGE_TRANSFER_DST_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            VK_MEMORY_PROPERTY_HOST_CACHED_BIT,
            MemoryKind::Readback,
            "target readback",
        ) {
            Ok(b) => b,
            Err(e) => {
                dev.destroy_image(&color);
                return Err(e);
            }
        };
        let id = self.next_id();
        let graph = FrameGraph::new(id);
        self.targets.insert(id, Target { color, readback, extent: VkExtent2D { width, height }, graph, rendered: false });
        Ok(TargetId(id))
    }

    pub fn destroy_target(&mut self, target: TargetId) {
        let Some(t) = self.targets.remove(&target.0) else { return };
        // Offscreen work is synchronous, so nothing on the GPU still uses the target or its
        // graph's images.
        self.pool.release_owner(&self.device, &mut self.descriptors, target.0);
        self.device.destroy_buffer(&t.readback);
        self.device.destroy_image(&t.color);
        if self.last_graph == Some(GraphSource::Target(target.0)) {
            self.last_graph = None;
        }
    }

    pub fn render(&mut self, target: TargetId, frame: &FrameDesc) -> Result<(), GpuError> {
        check_frame(&self.meshes, &self.textures, frame)?;
        let Some(t) = self.targets.get_mut(&target.0) else {
            return Err(GpuError::unknown(crate::IdKind::Target, target.0, ""));
        };
        let extent = t.extent;
        let start = Instant::now();
        let upload = self.upload.as_mut().expect("created in new");
        // Offscreen work is synchronous: no earlier frame still reads this buffer.
        prepare_frame(&self.device, &self.meshes, &self.textures, frame, extent, upload, &mut self.scratch)?;
        let env = &frame.environment;
        let shadow = (env.shadow.enabled && self.scratch.shadow.data.count > 0).then(|| shadow_desc(&env.shadow));
        let features = frame_features(frame, &self.scratch, shadow.is_some());
        let mode = PostMode::of(env).fit(extent);
        let pipelines = self.pipelines.frame(&self.device, TARGET_FORMAT, features, mode, env.sky.enabled, !frame.overlay.is_empty())?;
        // Graph images replaced (or long unused) are freed once no pending frame can use them.
        self.pool.collect(&self.device, &mut self.descriptors, self.completed);
        let shape = FrameShape {
            out_name: "target",
            format: TARGET_FORMAT,
            extent,
            // Its contents are rendered anew; wait for the last readback's copy.
            prior: Prior::Discard { after: VK_PIPELINE_STAGE_2_COPY_BIT },
            fin: Final::TransferSrc,
            mode,
            shadow,
            overlay: !frame.overlay.is_empty(),
        };
        let res = declare_frame(&mut t.graph, &shape);
        prepare_graph(&self.device, &mut self.descriptors, &mut self.pool, &mut t.graph, &res)?;
        t.graph.bind_image(res.out, t.color.handle, t.color.view);
        let prepare_ms = start.elapsed().as_secs_f64() * 1e3;
        let frame_no = self.frame_no + 1;
        let upload = self.upload.as_ref().expect("created in new");
        let mut timer = self.timer.take();
        let mut draws = Default::default();
        let ctx = FrameContext {
            dev: &self.device,
            pipelines,
            descriptors: &self.descriptors,
            meshes: &self.meshes,
            frame,
            scratch: &self.scratch,
            upload,
        };
        let graph = &mut t.graph;
        let ran = run_sync(&self.device, self.sync_cmd, self.sync_fence, |dev, cmd| {
            if let Some(t) = timer.as_mut() {
                t.begin_frame(dev, cmd, frame_no);
            }
            draws = record_frame(&ctx, cmd, graph, &res, timer.as_mut());
        });
        // The render completed (or failed with nothing pending): its timestamps are readable.
        // Stats are updated in place, reusing their storage.
        let stats = &mut self.last_offscreen;
        if let (Ok(_), Some(t)) = (&ran, timer.as_mut()) {
            t.submitted();
            match t.read_into(&self.device, &mut stats.gpu) {
                Ok(true) => stats.timestamp_error = None,
                Ok(false) => {}
                Err(r) => stats.timestamp_error = Some(r),
            }
        }
        self.timer = timer;
        let times = ran?;
        // Synchronous: this frame and every one before it finished.
        self.completed = self.pool.note_used(&t.graph);
        t.rendered = true;
        self.frame_no = frame_no;
        self.last_graph = Some(GraphSource::Target(target.0));
        let stats = &mut self.last_offscreen;
        stats.frame = frame_no;
        stats.path = FramePath::Offscreen;
        stats.width = extent.width;
        stats.height = extent.height;
        stats.cpu = CpuTiming { wait_ms: times.wait_ms, record_ms: prepare_ms + times.record_ms, submit_ms: times.submit_ms };
        stats.timestamps_supported = self.timer.is_some();
        stats.draws = draws;
        stats.graph = t.graph.frame_stats();
        stats.draw_buffer_bytes = self.upload.as_ref().map_or(0, |u| u.bytes());
        Ok(())
    }

    pub fn read_rgba8(&mut self, target: TargetId) -> Result<Vec<u8>, GpuError> {
        let t = self.targets.get(&target.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Target, target.0, ""))?;
        if !t.rendered {
            return Err(GpuError::new(format!("read_rgba8: target {} has not been rendered yet", target.0)));
        }
        let (image, buffer, extent) = (t.color.handle, t.readback.handle, t.extent);
        self.run_sync(|dev, cmd| {
            let region = VkBufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0, // tightly packed
                buffer_image_height: 0,
                image_subresource: VkImageSubresourceLayers {
                    aspect_mask: VK_IMAGE_ASPECT_COLOR_BIT,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: VkOffset3D { x: 0, y: 0, z: 0 },
                image_extent: VkExtent3D { width: extent.width, height: extent.height, depth: 1 },
            };
            // SAFETY: recording; the image is in TRANSFER_SRC_OPTIMAL (end of `render`) and the
            // buffer holds width*height*4 bytes.
            unsafe {
                (dev.fns.cmd_copy_image_to_buffer)(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, buffer, 1, &region)
            };
            dev.barrier(
                cmd,
                &[buffer_barrier(
                    buffer,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_WRITE_BIT,
                    VK_PIPELINE_STAGE_2_HOST_BIT,
                    VK_ACCESS_2_HOST_READ_BIT,
                )],
                &[],
            );
        })?;
        let len = extent.width as usize * extent.height as usize * 4;
        let mut out = vec![0u8; len];
        // SAFETY: the readback buffer is persistently mapped coherent memory of `len` bytes and
        // the copy into it completed (fence waited, host-read barrier recorded).
        unsafe { ptr::copy_nonoverlapping(t.readback.mapped as *const u8, out.as_mut_ptr(), len) };
        Ok(out)
    }

    pub fn attach_window(&mut self, hinstance: *mut c_void, hwnd: *mut c_void, width: u32, height: u32) -> Result<(), GpuError> {
        if let Some(mut old) = self.window.take() {
            self.device.wait_idle();
            self.pool.release_owner(&self.device, &mut self.descriptors, WINDOW_OWNER);
            old.destroy(&self.instance, &self.device, self.cmd_pool);
            if self.last_graph == Some(GraphSource::Window) {
                self.last_graph = None;
            }
        }
        let window = Window::new(&self.instance, &self.device, self.cmd_pool, hinstance, hwnd, width, height)?;
        // Every variant for the window's format now, so presenting never waits for a pipeline.
        if let Err(e) = self.pipelines.prebuild(&self.device, window.format()) {
            let mut window = window;
            window.destroy(&self.instance, &self.device, self.cmd_pool);
            return Err(e);
        }
        self.window = Some(window);
        Ok(())
    }

    pub fn resize_window(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        let window = self.window.as_mut().ok_or_else(|| GpuError::new("resize_window: no window attached"))?;
        window.resize(&self.instance, &self.device, width, height)
    }

    pub fn present(&mut self, frame: &FrameDesc) -> Result<(), GpuError> {
        if self.window.is_none() {
            return Err(GpuError::new("present: no window attached"));
        }
        check_frame(&self.meshes, &self.textures, frame)?;
        let window = self.window.as_mut().expect("checked");
        let frame_no = self.frame_no + 1;
        let presented = window.present(
            &self.instance,
            &self.device,
            &mut self.pipelines,
            &mut self.descriptors,
            &mut self.pool,
            &mut self.completed,
            &self.meshes,
            &self.textures,
            frame,
            frame_no,
            &mut self.scratch,
            &mut self.last_window,
        )?;
        if presented {
            self.frame_no = frame_no;
            self.last_graph = Some(GraphSource::Window);
        }
        Ok(())
    }

    /// The latest frame of `path` (or of either path), with current pipeline and memory state.
    pub fn last_frame_stats(&self, path: Option<FramePath>) -> FrameStats {
        let latest = match path {
            Some(FramePath::Offscreen) => &self.last_offscreen,
            Some(FramePath::Window) => &self.last_window,
            None if self.last_window.frame > self.last_offscreen.frame => &self.last_window,
            None => &self.last_offscreen,
        };
        let mut stats = latest.clone();
        stats.timestamps_supported = self.timer.is_some();
        stats.pipelines = self.pipelines.stats();
        stats.memory = self.device.memory_stats(&self.instance);
        stats
    }

    pub fn mark_warm(&mut self) {
        self.pipelines.mark_warm();
    }

    pub fn last_graph(&self) -> Option<GraphInfo> {
        match self.last_graph? {
            GraphSource::Window => self.window.as_ref().map(|w| w.graph.info().clone()),
            GraphSource::Target(id) => self.targets.get(&id).map(|t| t.graph.info().clone()),
        }
    }

    pub fn pipeline_cache_data(&self) -> Result<Vec<u8>, GpuError> {
        self.pipelines.cache_data(&self.device)
    }
}

impl Drop for GpuInner {
    fn drop(&mut self) {
        let dev = &self.device;
        dev.wait_idle();
        if let Some(mut w) = self.window.take() {
            w.destroy(&self.instance, dev, self.cmd_pool);
        }
        for (_, m) in self.meshes.drain() {
            dev.destroy_buffer(&m.buffer);
        }
        for (_, t) in self.textures.map.drain() {
            dev.destroy_image(&t.image);
        }
        for (_, t) in self.targets.drain() {
            dev.destroy_buffer(&t.readback);
            dev.destroy_image(&t.color);
        }
        self.pool.destroy(dev, &mut self.descriptors);
        if let Some(u) = self.upload.take() {
            u.destroy(dev);
        }
        if let Some(mut t) = self.timer.take() {
            t.destroy(dev);
        }
        self.pipelines.destroy(dev);
        self.descriptors.destroy(dev);
        // SAFETY: device idle; objects belong to `dev` and are destroyed once (null is ignored).
        unsafe {
            (dev.fns.destroy_fence)(dev.handle, self.sync_fence, ptr::null());
            (dev.fns.destroy_command_pool)(dev.handle, self.cmd_pool, ptr::null()); // frees sync_cmd
        }
        // `device`, then `instance` drop next (field order).
    }
}

const _: () = assert!(size_of::<Vertex>() == 28);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, Validation};

    /// The hand-transcribed limit offsets read sane values from every real driver: members with
    /// spec-guaranteed minimums around the one we use (`timestampPeriod`).
    #[test]
    fn device_limits_read_at_header_offsets() {
        let Ok(instance) = Instance::new(Validation::Off, false) else {
            eprintln!("device_limits_read_at_header_offsets: SKIPPED — Vulkan is not available");
            return;
        };
        for (i, pd) in instance.physical_devices().unwrap().into_iter().enumerate() {
            let limits = instance.properties(pd).limits;
            let push = limits.word(VkPhysicalDeviceLimits::MAX_PUSH_CONSTANTS_SIZE);
            let max_index = limits.word(VkPhysicalDeviceLimits::MAX_DRAW_INDEXED_INDEX_VALUE);
            let period = limits.timestamp_period();
            eprintln!("adapter {i}: maxPushConstantsSize {push}, maxDrawIndexedIndexValue {max_index:#x}, timestampPeriod {period} ns");
            assert!((128..=65536).contains(&push), "maxPushConstantsSize {push}");
            assert!(max_index >= (1 << 24) - 1, "maxDrawIndexedIndexValue {max_index}");
            assert!(period.is_finite() && period > 0.0 && period < 1e6, "timestampPeriod {period}");
        }
    }

    /// The add-a-pass recipe of `graph.rs`, run on the GPU: a depth pass, an `R32_UINT` id pass
    /// that depth-tests against it, and a copy of the ids into a host-visible buffer, every barrier
    /// from the graph — under validation including synchronization validation. Repeated frames
    /// reuse the compiled graph and its images; a new size replaces them.
    #[test]
    fn graph_runs_an_id_buffer_pass() {
        use crate::graph::{Final, ImageDesc, Load, Prior, RenderGraph};
        if !crate::is_available() {
            eprintln!("graph_runs_an_id_buffer_pass: SKIPPED — Vulkan is not available");
            return;
        }
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum P {
            Scene,
            Ids,
            Copy,
        }
        for a in crate::enumerate_adapters().unwrap() {
            let options = GpuOptions { validation: Validation::On, adapter: crate::AdapterPreference::Index(a.index), ..Default::default() };
            let mut gpu = match GpuInner::new(&options) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("graph_runs_an_id_buffer_pass: SKIPPED on adapter {} — {e}", a.index);
                    continue;
                }
            };
            gpu.take_validation_messages();
            let host = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
            let readback =
                gpu.device.create_buffer(64 * 64 * 4, VK_BUFFER_USAGE_TRANSFER_DST_BIT, host, 0, MemoryKind::Readback, "ids").unwrap();
            let mut g = RenderGraph::<P>::new(4242);
            let mut first_ids = VK_NULL_HANDLE;
            // Round 4 goes back to the first size before anything was collected: the replaced
            // images are taken back, not recreated.
            for (round, (w, h)) in [(16u32, 8u32), (16, 8), (16, 8), (32, 4), (16, 8)].into_iter().enumerate() {
                let id = 0x1234_5600 + round as u32;
                if round != 4 {
                    gpu.pool.collect(&gpu.device, &mut gpu.descriptors, gpu.completed);
                }
                g.begin();
                let depth = g.transient("depth", ImageDesc::depth(crate::render::DEPTH_FORMAT, w, h, MemoryKind::RenderTargets));
                g.pass("scene", P::Scene).depth(depth, Load::Clear);
                let ids = g.transient("ids", ImageDesc::color(VK_FORMAT_R32_UINT, w, h, MemoryKind::RenderTargets));
                g.pass("id_buffer", P::Ids).color(ids, Load::Clear).depth_read(depth);
                let bytes = w as u64 * h as u64 * 4;
                let buf = g.import_buffer("id_readback", bytes, Prior::Discard { after: VK_PIPELINE_STAGE_2_HOST_BIT }, Final::HostRead);
                g.pass("id_copy", P::Copy).transfer_src(ids).transfer_dst(buf);
                g.prepare(&gpu.device, &mut gpu.descriptors, &mut gpu.pool).unwrap();
                g.bind_buffer(buf, readback.handle);
                match round {
                    0 => first_ids = g.image(ids).image,
                    4 => assert_eq!(g.image(ids).image, first_ids, "the 16x8 image is reused"),
                    _ => {}
                }
                let graph = &mut g;
                run_sync(&gpu.device, gpu.sync_cmd, gpu.sync_fence, |dev, cmd| {
                    for p in 0..graph.pass_count() {
                        graph.barriers(dev, cmd, p);
                        let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: VkExtent2D { width: w, height: h } };
                        let tag = graph.tag(p);
                        if tag == P::Copy {
                            let region = VkBufferImageCopy {
                                buffer_offset: 0,
                                buffer_row_length: 0,
                                buffer_image_height: 0,
                                image_subresource: VkImageSubresourceLayers {
                                    aspect_mask: VK_IMAGE_ASPECT_COLOR_BIT,
                                    mip_level: 0,
                                    base_array_layer: 0,
                                    layer_count: 1,
                                },
                                image_offset: VkOffset3D { x: 0, y: 0, z: 0 },
                                image_extent: VkExtent3D { width: w, height: h, depth: 1 },
                            };
                            let image = graph.image(ids).image;
                            // SAFETY: recording; the graph moved `ids` to TRANSFER_SRC_OPTIMAL; the
                            // buffer holds at least w*h*4 bytes.
                            unsafe {
                                (dev.fns.cmd_copy_image_to_buffer)(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, readback.handle, 1, &region)
                            };
                            continue;
                        }
                        let depth_att = graph.depth_attachment(p, depth, 0.25, None);
                        let color_att = (tag == P::Ids).then(|| graph.color_attachment(p, ids, [f32::from_bits(id), 0.0, 0.0, 0.0]));
                        let rendering = VkRenderingInfo {
                            s_type: ST_RENDERING_INFO,
                            p_next: ptr::null(),
                            flags: 0,
                            render_area: area,
                            layer_count: 1,
                            view_mask: 0,
                            color_attachment_count: color_att.is_some() as u32,
                            p_color_attachments: color_att.as_ref().map_or(ptr::null(), |c| c as *const _),
                            p_depth_attachment: &depth_att,
                            p_stencil_attachment: ptr::null(),
                        };
                        // SAFETY: recording; the attachments are the graph's, in the layouts it set.
                        unsafe {
                            (dev.fns.cmd_begin_rendering)(cmd, &rendering);
                            (dev.fns.cmd_end_rendering)(cmd);
                        }
                    }
                    graph.finish(dev, cmd);
                })
                .unwrap();
                gpu.completed = gpu.pool.note_used(&g);
                let n = (w * h) as usize;
                let mut out = vec![0u32; n];
                // SAFETY: coherent mapped memory of at least n u32s; the copy finished (fence) and
                // the graph recorded the transfer → host barrier.
                unsafe { ptr::copy_nonoverlapping(readback.mapped as *const u32, out.as_mut_ptr(), n) };
                assert!(out.iter().all(|&v| v == id), "adapter {}: round {round}: {:x?}", a.index, &out[..4]);
                let stats = g.frame_stats();
                assert_eq!(stats.recompiled, round == 0 || round >= 3, "round {round}: recompile only when the declaration changes");
                assert_eq!(stats.barriers, g.info().barrier_count() as u32);
                if round == 0 {
                    eprintln!("{}", g.info());
                }
            }
            // The 32x4 images were replaced by the 16x8 ones again; collecting frees them.
            assert_eq!(gpu.pool.len(), 4);
            gpu.pool.collect(&gpu.device, &mut gpu.descriptors, gpu.completed);
            assert_eq!(gpu.pool.len(), 2);
            gpu.pool.release_owner(&gpu.device, &mut gpu.descriptors, 4242);
            assert_eq!(gpu.pool.len(), 0);
            gpu.device.destroy_buffer(&readback);
            let messages = gpu.take_validation_messages();
            let errors: Vec<_> = messages.iter().filter(|m| m.severity == Severity::Error).collect();
            assert!(errors.is_empty(), "adapter {}: {errors:#?}", a.index);
        }
    }

    /// Proves the debug messenger really delivers errors (so "zero errors" elsewhere means something).
    #[test]
    fn validation_errors_are_captured() {
        if !crate::is_available() {
            eprintln!("validation_errors_are_captured: SKIPPED — Vulkan is not available");
            return;
        }
        let options = GpuOptions { validation: Validation::On, ..Default::default() };
        let mut gpu = match GpuInner::new(&options) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("validation_errors_are_captured: SKIPPED — {e}");
                return;
            }
        };
        assert!(gpu.validation_enabled());
        let early = gpu.take_validation_messages();
        // Errors only: GPU-assisted validation (VK_LAYER_ENABLES) adds informational warnings here.
        assert!(early.iter().all(|m| m.severity != Severity::Error), "{early:#?}");
        // Deliberate misuse: an undefined VkFenceCreateFlags bit.
        let info = VkFenceCreateInfo { s_type: ST_FENCE_CREATE_INFO, p_next: ptr::null(), flags: 0x4000_0000 };
        let mut fence = VK_NULL_HANDLE;
        let dev = &gpu.device;
        // SAFETY: valid device and struct; the bad flag is caught by the validation layer, and the
        // fence (if created) is destroyed right away.
        unsafe {
            (dev.fns.create_fence)(dev.handle, &info, ptr::null(), &mut fence);
            (dev.fns.destroy_fence)(dev.handle, fence, ptr::null());
        }
        let messages = gpu.take_validation_messages();
        eprintln!("{messages:#?}");
        assert!(
            messages.iter().any(|m| m.severity == Severity::Error && m.id_name.contains("VkFenceCreateInfo-flags")),
            "expected a VkFenceCreateInfo-flags error"
        );
        assert!(gpu.take_validation_messages().is_empty(), "drained");

        // Synchronization validation is on: two copies into one buffer with no barrier between
        // them are a write-after-write hazard.
        let host = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
        let src = gpu.device.create_buffer(64, VK_BUFFER_USAGE_TRANSFER_SRC_BIT, host, 0, MemoryKind::Staging, "src").unwrap();
        let dst = gpu.device.create_buffer(64, VK_BUFFER_USAGE_TRANSFER_DST_BIT, host, 0, MemoryKind::Staging, "dst").unwrap();
        gpu.run_sync(|dev, cmd| {
            let region = VkBufferCopy { src_offset: 0, dst_offset: 0, size: 64 };
            // SAFETY: recording; both buffers are live and 64 bytes.
            unsafe {
                (dev.fns.cmd_copy_buffer)(cmd, src.handle, dst.handle, 1, &region);
                (dev.fns.cmd_copy_buffer)(cmd, src.handle, dst.handle, 1, &region);
            }
        })
        .unwrap();
        gpu.device.destroy_buffer(&src);
        gpu.device.destroy_buffer(&dst);
        let messages = gpu.take_validation_messages();
        assert!(
            messages.iter().any(|m| m.severity == Severity::Error && m.id_name.contains("SYNC-HAZARD-WRITE-AFTER-WRITE")),
            "expected a sync-validation WAW hazard, got {messages:#?}"
        );
    }
}
