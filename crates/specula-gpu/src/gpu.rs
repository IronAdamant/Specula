//! `Gpu` internals: adapter selection, meshes, offscreen targets, readback, window plumbing.

use crate::batch::mesh_bounds;
use crate::device::{buffer_barrier, image_barrier, Buffer, Device, Image};
use crate::instance::Instance;
use crate::descriptors::Descriptors;
use crate::graph::{Final, GraphInfo, Prior, TransientPool};
use crate::idbuffer::{IdCounts, IdImage, IdResources};
use crate::render::{
    check_frame, declare_frame, frame_features, prepare_frame, prepare_graph, record_frame, CacheIdentity, ContentHint, FrameContext,
    FrameGraph, FrameScratch, FrameShape, FrameUpload, Mesh, Pipelines, SurfaceVertex, Variants,
};
use crate::overlay::{check_pixels, vk_format, Texture, Textures};
use crate::post::PostMode;
use crate::shadow::shadow_desc;
use crate::swapchain::{Window, WINDOW_OWNER};
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{
    AdapterInfo, AdapterKind, AdapterPreference, CpuTiming, ErrorReason, FrameDesc, FramePath, FrameStats, GpuError, GpuOptions,
    MemoryKind, MeshId, MeshStream, MeshStreams, SkinVertex, TargetId, TextureFormat, TextureId, TextureSampler, ValidationMessage,
    Vertex,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::time::{Duration, Instant};

/// Offscreen colour format (readback bytes are sRGB RGBA8).
pub const TARGET_FORMAT: VkFormat = crate::render::LDR_FORMAT;

/// The meshes every `Gpu` holds for its whole life (the sprite quad, the terrain patch grids):
/// never destroyed through the API.
fn shared_mesh(mesh: MeshId) -> bool {
    mesh.0 == crate::sprite::QUAD_MESH || mesh == crate::terrain::GRID_MESH || mesh == crate::terrain::QUARTER_MESH
}

/// Where `run_sync` spent its time, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
struct SyncTimes {
    record_ms: f64,
    submit_ms: f64,
    wait_ms: f64,
}

struct Target {
    color: Image,
    /// Its memory is allocated on the image heap's worker ([`Target::readback_ahead`] until it
    /// is back: a null buffer here meanwhile; `GpuInner::target_readback` finishes it).
    readback: Buffer,
    readback_ahead: Option<crate::device::PendingBuffer>,
    extent: VkExtent2D,
    /// Its frame graph; the depth buffer and post-chain images are the graph's transients.
    graph: FrameGraph,
    /// False until the first `render`; the colour image then sits in TRANSFER_SRC_OPTIMAL.
    rendered: bool,
    /// The readback buffer holds the last render's pixels (an asynchronous render copies them in
    /// its own submission): `read_rgba8` copies them out without another GPU round trip.
    copied: bool,
}

/// An offscreen render submitted without waiting ([`GpuInner::render_async`]): what is settled
/// once its fence signals ([`GpuInner::settle_offscreen`]).
struct Pending {
    target: u32,
    /// Its graph submission ([`TransientPool::note_used`]): `completed` reaches it when it ends.
    submission: u64,
    /// Its timestamps are to be read (the timer recorded into it).
    timed: bool,
    prepare_ms: f64,
    times: SyncTimes,
    /// The colour image is copied into the target's readback buffer in its submission.
    copied: bool,
}

/// Offscreen colour images copied into their target's readback buffer on the GPU (in a
/// render's own submission, or by `read_rgba8` when the render left it out), and copied out of
/// that buffer by the host (`read_rgba8`), since start-up: what tests hold a timings-only render
/// to (`GpuInner::render_async_timings`: neither).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Readbacks {
    pub gpu_copies: u64,
    pub host_reads: u64,
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
    /// Command buffer + fence of the asynchronous offscreen render (`render_async`); at most one is
    /// in flight ([`Pending`]). Every other offscreen use settles it first.
    async_cmd: VkCommandBuffer,
    async_fence: VkFence,
    pending: Option<Pending>,
    /// The next settled asynchronous render reports a lost device (tests, `inject_offscreen_fault`).
    offscreen_fault: bool,
    /// Offscreen colour images copied into a readback buffer, and read out of one by the host,
    /// since start-up ([`GpuInner::readbacks`]).
    readbacks: Readbacks,
    /// Frame data and draw records for offscreen renders (synchronous, so one buffer suffices).
    upload: Option<FrameUpload>,
    /// Sight's ID buffer resources, shared by every target (`render_with_ids`).
    ids: IdResources,
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
    /// What the last ID render's colour passes recorded (`last_colour_draws`); the skip list a
    /// test injected (`debug_skip_colour_draws`).
    colour_record: std::cell::RefCell<crate::render::ColourRecord>,
    /// Queued uploads and deferred frees (`stream.rs`): submitted before each frame.
    uploads: crate::stream::Uploads,
    /// The resident baked GI volumes (`gi.rs`).
    gi: crate::gi::GiBuffer,
    /// The resident glyph store of in-world text (`glyphs.rs`).
    glyphs: crate::glyphs::GlyphBuffer,
    /// The latest sight capture's and window frame's content ([`ContentHint`]): a new target's
    /// graph is prepared ahead in their shape ([`GpuInner::prewarm_target`]).
    hint_offscreen: Option<ContentHint>,
    hint_window: Option<ContentHint>,
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
    let zw = specula_core::profile::zone("vk.wait_fence");
    let waited = dev.wait_fence(fence);
    drop(zw);
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

/// Record into `cmd` with `record` and submit with `fence`, without waiting (see
/// [`GpuInner::render_async`]): `wait_ms` is 0.
fn submit_async(dev: &Device, cmd: VkCommandBuffer, fence: VkFence, record: impl FnOnce(&Device, VkCommandBuffer)) -> Result<SyncTimes, GpuError> {
    let t0 = Instant::now();
    dev.begin(cmd)?;
    record(dev, cmd);
    dev.end(cmd)?;
    let t1 = Instant::now();
    // A failed submit queues nothing and leaves the fence untouched.
    dev.submit(cmd, None, None, fence)?;
    let t2 = Instant::now();
    let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1e3;
    Ok(SyncTimes { record_ms: ms(t0, t1), submit_ms: ms(t1, t2), wait_ms: 0.0 })
}

/// Records the copy of a rendered target's colour image (in TRANSFER_SRC_OPTIMAL, the end of a
/// render) into its readback buffer, made visible to the host.
fn record_readback(dev: &Device, cmd: VkCommandBuffer, image: VkImage, buffer: VkBuffer, extent: VkExtent2D) {
    let region = VkBufferImageCopy {
        buffer_offset: 0,
        buffer_row_length: 0, // tightly packed
        buffer_image_height: 0,
        image_subresource: VkImageSubresourceLayers { aspect_mask: VK_IMAGE_ASPECT_COLOR_BIT, mip_level: 0, base_array_layer: 0, layer_count: 1 },
        image_offset: VkOffset3D { x: 0, y: 0, z: 0 },
        image_extent: VkExtent3D { width: extent.width, height: extent.height, depth: 1 },
    };
    // SAFETY: recording; the image is in TRANSFER_SRC_OPTIMAL (end of a render) and the buffer
    // holds width*height*4 bytes.
    unsafe { (dev.fns.cmd_copy_image_to_buffer)(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, buffer, 1, &region) };
    dev.barrier(cmd, &[buffer_barrier(buffer, VK_PIPELINE_STAGE_2_COPY_BIT, VK_ACCESS_2_TRANSFER_WRITE_BIT, VK_PIPELINE_STAGE_2_HOST_BIT, VK_ACCESS_2_HOST_READ_BIT)], &[]);
}

/// `render_with_ids`' argument checks.
fn check_ids(frame: &FrameDesc, ids: &[u32], occluders: &[[u32; 4]]) -> Result<(), GpuError> {
    if occluders.len() > crate::MAX_ID_OCCLUDERS {
        return Err(GpuError::limit(crate::Limit::IdOccluders, occluders.len() as u64, crate::MAX_ID_OCCLUDERS as u64));
    }
    let expected = frame.draws.len() + frame.draws_2d.len();
    if ids.len() != expected {
        return Err(GpuError::rejected(
            ErrorReason::DataLength { given: ids.len() as u64, expected: expected as u64 },
            format!("render_with_ids: {} ids for {} draws and {} 2D draws (one per draw, the 2D ones after)", ids.len(), frame.draws.len(), frame.draws_2d.len()),
        ));
    }
    Ok(())
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
        // The chosen adapter's own cache file, when the caller gave a folder and no blob.
        let from_dir = match (&options.pipeline_cache, &options.pipeline_cache_dir) {
            (None, Some(dir)) => {
                let file = dir.join(crate::pipeline_cache_file_name(props.vendor_id, props.device_id));
                std::fs::metadata(&file).ok().filter(|m| m.is_file() && m.len() <= crate::MAX_PIPELINE_CACHE_FILE).and_then(|_| std::fs::read(&file).ok())
            }
            _ => None,
        };
        let cache_blob = options.pipeline_cache.as_deref().or(from_dir.as_deref());
        let pipelines = match Pipelines::new(&device, descriptors.layout, cache_blob, &id) {
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
            async_cmd: ptr::null_mut(),
            async_fence: VK_NULL_HANDLE,
            pending: None,
            offscreen_fault: false,
            readbacks: Readbacks::default(),
            upload: None,
            ids: IdResources::default(),
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
            colour_record: Default::default(),
            uploads: crate::stream::Uploads::default(),
            gi: crate::gi::GiBuffer::default(),
            glyphs: crate::glyphs::GlyphBuffer::default(),
            hint_offscreen: None,
            hint_window: None,
            device,
            instance,
        };
        // On error `gpu` drops and releases whatever was created.
        gpu.cmd_pool = gpu.device.create_command_pool()?;
        gpu.uploads.warm(&gpu.device)?;
        gpu.device.warm_image_heap()?;
        gpu.sync_cmd = gpu.device.allocate_command_buffer(gpu.cmd_pool)?;
        gpu.sync_fence = gpu.device.create_fence(false)?;
        gpu.async_cmd = gpu.device.allocate_command_buffer(gpu.cmd_pool)?;
        gpu.async_fence = gpu.device.create_fence(false)?;
        gpu.upload = Some(FrameUpload::new(&gpu.device)?);
        gpu.timer = PassTimer::new(&gpu.device)?;
        // The quad every sprite and tile chunk is drawn with (indices only; the shader makes the
        // corners), under an id `upload_mesh` never hands out.
        let quad = gpu.upload_mesh(&[Vertex::default(); 4], &[0, 1, 2, 0, 2, 3], &MeshStreams::default())?;
        let mesh = gpu.meshes.remove(&quad.0).expect("just uploaded");
        gpu.meshes.insert(crate::sprite::QUAD_MESH, mesh);
        // The terrain's patch grids (`terrain.rs`), likewise.
        for (id, n) in [(crate::terrain::GRID_MESH, crate::terrain::GRID), (crate::terrain::QUARTER_MESH, crate::terrain::GRID / 2)] {
            let (v, i) = crate::terrain::grid_mesh(n);
            let grid = gpu.upload_mesh(&v, &i, &MeshStreams::default())?;
            let mesh = gpu.meshes.remove(&grid.0).expect("just uploaded");
            gpu.meshes.insert(id.0, mesh);
        }
        // Every offscreen pipeline variant now, so no frame ever waits for one.
        gpu.pipelines.prebuild(&gpu.device, TARGET_FORMAT)?;
        gpu.pipelines.prebuild_common(&gpu.device)?;
        Ok(gpu)
    }

    pub fn adapter(&self) -> &AdapterInfo {
        &self.adapter
    }

    pub fn memory_stats(&self) -> crate::MemoryStats {
        self.device.memory_stats(&self.instance)
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
        // Ids run 1 ..= u32::MAX - 3 (u32::MAX is the shared sprite quad, the two below it the
        // terrain's patch grids).
        self.next_id = if self.next_id >= crate::terrain::QUARTER_MESH.0 - 1 { 1 } else { self.next_id + 1 };
        id
    }

    /// Record with `record`, submit, and wait for completion.
    ///
    /// On return — success or error — nothing submitted here is still running, so callers may free
    /// what the commands used, and the fence is unsignaled for the next call.
    fn run_sync(&self, record: impl FnOnce(&Device, VkCommandBuffer)) -> Result<SyncTimes, GpuError> {
        run_sync(&self.device, self.sync_cmd, self.sync_fence, record)
    }

    pub fn upload_mesh(&mut self, vertices: &[Vertex], indices: &[u32], streams: &MeshStreams) -> Result<MeshId, GpuError> {
        let layout = mesh_layout(vertices, indices, streams)?;
        let total = layout.total;
        let dev = &self.device;
        let staging = dev.create_buffer(
            total,
            VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            0,
            MemoryKind::Staging,
            "mesh staging",
        )?;
        // SAFETY: `staging.mapped` points to at least `total` bytes of coherent host memory.
        unsafe { layout.write(staging.mapped as *mut u8, vertices, indices) };
        let buffer = match create_mesh_buffer(dev, total) {
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
        Ok(self.finish_mesh(buffer, layout, vertices, indices.len(), None))
    }

    /// [`GpuInner::upload_mesh`] without waiting: the copy goes into the current upload batch,
    /// submitted before the next frame (see `stream.rs`). The mesh can be drawn at once.
    pub fn queue_mesh(&mut self, vertices: &[Vertex], indices: &[u32], streams: &MeshStreams) -> Result<MeshId, GpuError> {
        let layout = mesh_layout(vertices, indices, streams)?;
        let total = layout.total;
        // A range of a mesh block, not an allocation of its own (see `stream.rs`).
        let (block, at, handle, size) = {
            let _z = specula_core::profile::zone("gpu.range");
            self.uploads.blocks.alloc(&self.device, total)?
        };
        let staged = {
            let _z = specula_core::profile::zone("gpu.stage");
            match self.uploads.stage(&self.device, total) {
                Ok(s) => s,
                Err(e) => {
                    self.uploads.retire(crate::stream::Retired::Range(block, at, size));
                    return Err(e);
                }
            }
        };
        let zw = specula_core::profile::zone("gpu.write");
        // SAFETY: `staged.dst` points to at least `total` bytes of mapped staging memory that no
        // submitted batch uses (its slot is recording).
        unsafe { layout.write(staged.dst, vertices, indices) };
        drop(zw);
        let zc = specula_core::profile::zone("gpu.record");
        let cmd = self.uploads.recording_cmd(&self.device)?;
        crate::stream::record_buffer_copy(&self.device, cmd, &staged, handle, at, total);
        drop(zc);
        // The block's buffer, borrowed: never destroyed through the mesh (`Mesh::pooled`).
        let buffer = Buffer { handle, memory: VK_NULL_HANDLE, mapped: ptr::null_mut(), bytes: size, kind: MemoryKind::Meshes };
        let _zf = specula_core::profile::zone("gpu.finish");
        let id = self.finish_mesh(buffer, layout, vertices, indices.len(), Some((block, at, size)));
        self.uploads.queued(id.0, total);
        Ok(id)
    }

    /// Registers an uploaded mesh buffer: starts its pipeline variants and hands out its id.
    /// `pooled`: the mesh lives at `(block, offset, bytes)` of a mesh block.
    fn finish_mesh(&mut self, buffer: Buffer, layout: MeshLayout, vertices: &[Vertex], index_count: usize, pooled: Option<(u32, u64, u64)>) -> MeshId {
        // The first mesh with skin or UVs starts building those pipeline variants on a worker;
        // its draws are skipped until they exist (`pipeline_variants`), no frame waits.
        if layout.skin.is_some() || layout.surface.is_some() {
            let formats = self.formats();
            let want = Variants { skinned: layout.skin.is_some(), textured: layout.surface.is_some(), ..Variants::default() };
            self.pipelines.request_variants(&self.device, want, &formats);
        }
        let id = self.next_id();
        let bounds = mesh_bounds(vertices);
        let base = pooled.map_or(0, |p| p.1);
        self.meshes.insert(
            id,
            Mesh {
                buffer,
                base,
                pooled,
                index_offset: base + layout.index_offset,
                index_count: index_count as u32,
                bounds,
                skin_offset: layout.skin.as_ref().map(|_| base + layout.skin_offset),
                surface_offset: layout.surface.as_ref().map(|_| base + layout.surface_offset),
                joints: layout.joints,
            },
        );
        MeshId(id)
    }

    /// Frees a mesh once no submitted frame can read it, without waiting (see `stream.rs`).
    pub fn retire_mesh(&mut self, mesh: MeshId) {
        if shared_mesh(mesh) {
            return;
        }
        if let Some(m) = self.meshes.remove(&mesh.0) {
            self.uploads.retire(match m.pooled {
                Some((block, at, size)) => crate::stream::Retired::Range(block, at, size),
                None => crate::stream::Retired::Buffer(m.buffer),
            });
        }
    }

    /// Frees a texture once no submitted frame can read it, without waiting.
    pub fn retire_texture(&mut self, texture: TextureId) {
        if let Some(t) = self.textures.map.remove(&texture.0) {
            self.uploads.retire(crate::stream::Retired::Image(t.image, t.slot));
        }
    }

    /// Submits the upload batch (and retirements) now, and frees what finished batches release.
    pub fn flush_uploads(&mut self) -> Result<(), GpuError> {
        let flushed = self.uploads.flush(&self.device);
        for slot in self.uploads.take_freed_slots() {
            self.descriptors.free_slot(slot);
        }
        flushed
    }

    pub fn upload_stats(&self) -> crate::UploadStats {
        self.uploads.stats()
    }

    pub fn mesh_space(&self) -> (u64, u64, u64) {
        self.uploads.blocks.space()
    }

    pub fn grow_mesh_space(&mut self) -> Result<(), GpuError> {
        self.uploads.blocks.grow(&self.device)
    }

    pub fn mesh_fits(&mut self, bytes: u64) -> bool {
        // A block grown ahead joins as soon as it is there.
        let _ = self.uploads.blocks.poll_grow(&self.device, false);
        self.uploads.blocks.fits(bytes)
    }

    pub fn mesh_growing(&self) -> bool {
        self.uploads.blocks.growing()
    }

    pub fn upload_slot_free(&self) -> bool {
        self.uploads.slot_free()
    }

    /// Before a synchronous destroy of `id`: if a batch that may not have finished copies into
    /// it, finish every batch first.
    fn settle_upload_of(&mut self, id: u32) {
        if self.uploads.is_pending(id) {
            let _ = self.uploads.finish(&self.device);
            for slot in self.uploads.take_freed_slots() {
                self.descriptors.free_slot(slot);
            }
        }
    }

    /// Builds the variants now, on this thread (loading time; `variants` changes only once all
    /// of them exist).
    pub fn prepare_variants(&mut self, skinned: bool, textured: bool) -> Result<(), GpuError> {
        let formats = self.formats();
        self.pipelines.build_variants(&self.device, Variants { skinned, textured, ..Variants::default() }, &formats)
    }

    /// Starts building the GI pipelines on a worker (window frames draw the flat ambient until
    /// they exist; offscreen renders build them at once). Nothing if they exist or are coming.
    /// Only the set `quality` draws with (lean for low, per pixel for high).
    pub fn request_gi_variants(&mut self, quality: crate::GiQuality) {
        let per_pixel = quality.per_pixel();
        if self.pipelines.gi_asked(per_pixel) {
            return; // cheap: callers may ask every frame
        }
        let formats = self.formats();
        self.pipelines.request_variants(&self.device, gi_variants(per_pixel), &formats);
    }

    /// Bytes the resident GI volumes take on the GPU, and how often the buffer was rebuilt.
    pub fn gi_stats(&self) -> (u64, u64) {
        (self.gi.bytes(), self.gi.rebuilds)
    }

    /// Whether the last render drew baked GI (its set bound and its pipelines ready).
    pub fn gi_drawn(&self) -> bool {
        self.scratch.gi.active()
    }

    /// The last render's irradiance-view stretch (`crate::gi::irradiance_view`: its scale and the
    /// luminance it starts at; `None` when it drew no irradiance view).
    pub fn gi_view_stretch(&self) -> Option<(f32, f32)> {
        let b = &self.scratch.gi;
        (b.active() && b.view == crate::GiView::Irradiance).then_some((b.view_scale, b.view_lo))
    }

    /// Tier low's per-draw records kept across frames: entries, their bytes, and how many draws
    /// the last frame sampled (zero for a frame whose draws didn't change).
    pub fn gi_cache_stats(&self) -> (usize, usize, usize) {
        let c = &self.scratch.gi_cache;
        (c.len(), c.bytes(), c.made)
    }

    /// Why the last GI set couldn't be bound (its frames draw the flat ambient), if it couldn't.
    pub fn gi_failure(&self) -> Option<&str> {
        self.gi.failure.as_ref().map(|(_, e)| e.as_str())
    }

    /// Binds `frame`'s GI for the next prepare. GI never fails a frame: any error (limits, memory,
    /// the pipelines) leaves that set unbound, the frame draws the flat ambient, and
    /// [`Gpu::gi_failure`] says why (the same set isn't retried until its key changes).
    fn bind_gi(&mut self, frame: &FrameDesc, offscreen: bool) {
        if let Err(e) = self.try_bind_gi(frame, offscreen) {
            self.gi.fail(frame.gi.key, e.to_string());
            self.scratch.gi = crate::gi::GiBinding::default();
        }
    }

    /// [`Gpu::bind_gi`]'s work: rebuilds the buffer if its key changed. `offscreen`: build missing
    /// GI pipelines now (the capture must show what it asked for), and a frame without GI leaves
    /// the window's buffer alone; otherwise ask a worker and draw this frame without GI.
    fn try_bind_gi(&mut self, frame: &FrameDesc, offscreen: bool) -> Result<(), GpuError> {
        let per_pixel = frame.gi.quality.per_pixel();
        let has = |v: crate::PipelineVariants| if per_pixel { v.gi_pixel } else { v.gi };
        let mut ready = has(self.pipelines.ready());
        if frame.gi.active() && !ready && !self.gi.failed(&frame.gi) {
            if offscreen {
                // `build_variants` first waits for a background build (one making this set takes
                // its pipelines; only the missing ones are created here).
                let formats = self.formats();
                self.pipelines.build_variants(&self.device, gi_variants(per_pixel), &formats)?;
                ready = has(self.pipelines.ready());
            } else {
                self.request_gi_variants(frame.gi.quality);
            }
        }
        if self.gi.stale(&frame.gi) {
            let words = crate::gi::words(&frame.gi)?;
            let (cmd, fence) = (self.sync_cmd, self.sync_fence);
            let buffer = crate::gi::upload(&self.device, &words, |dev, record| run_sync(dev, cmd, fence, |d, c| record(d, c)).map(|_| ()))?;
            self.gi.install(&mut self.uploads, &self.device, buffer, &frame.gi);
        }
        self.scratch.gi = self.gi.bind(&mut self.uploads, &frame.gi, ready, offscreen);
        Ok(())
    }

    /// Makes `frame`'s glyph store resident (copying only new words) and sets the address the
    /// frame's glyph records carry. Text never fails a frame: an error (memory, limits) leaves its
    /// glyphs undrawn, and [`Gpu::glyph_stats`] says why.
    fn bind_glyphs(&mut self, frame: &FrameDesc) {
        let text = &frame.text;
        if text.words.is_empty() {
            self.scratch.glyph_store = (0, 0);
            return;
        }
        let (cmd, fence) = (self.sync_cmd, self.sync_fence);
        let synced = self.glyphs.sync(&self.device, &mut self.uploads, text, |dev, record| run_sync(dev, cmd, fence, |d, c| record(d, c)).map(|_| ()));
        self.scratch.glyph_store = match synced {
            Ok(()) if self.glyphs.holds(text) => (self.glyphs.address(), self.glyphs.resident_words() as u32),
            _ => (0, 0),
        };
    }

    /// Makes up to `max_new` more words of glyph store `store` resident ahead of the frame that
    /// draws it (a parked capture, a pass at a time), in a buffer of at least `reserve` words:
    /// that frame then copies only what is left, never the whole store at once (review of
    /// cd954bd, major 1). Errors are remembered like a frame's ([`Gpu::glyph_stats`]).
    pub fn preload_glyphs(&mut self, store: u64, words: &[u32], reserve: usize, max_new: usize) {
        self.glyphs.reserve = reserve.min(crate::glyphs::MAX_GLYPH_WORDS);
        let have = if self.glyphs.store_id() == store { self.glyphs.resident_words() } else { 0 };
        let end = words.len().min(have.saturating_add(max_new));
        if end <= have {
            return;
        }
        let text = crate::TextFrame { store, words: &words[..end], glyphs: &[] };
        let (cmd, fence) = (self.sync_cmd, self.sync_fence);
        let _ = self.glyphs.sync(&self.device, &mut self.uploads, &text, |dev, record| run_sync(dev, cmd, fence, |d, c| record(d, c)).map(|_| ()));
    }

    /// The glyph store's resident bytes, words copied and buffers made since start-up, and why
    /// the last store couldn't be made resident (if it couldn't).
    pub fn glyph_stats(&self) -> (u64, u64, u64, Option<&str>) {
        let g = &self.glyphs;
        (g.bytes(), g.words_copied, g.rebuilds, g.failure.as_ref().map(|(_, e)| e.as_str()))
    }

    /// Starts building the terrain and water pipelines on a worker (a no-op once built or while
    /// building); their draws are skipped until [`Gpu::pipeline_variants`] says `terrain`.
    pub fn request_terrain_pipelines(&mut self) {
        if self.pipelines.variants.terrain {
            return;
        }
        let formats = self.formats();
        self.pipelines.request_variants(&self.device, Variants { terrain: true, ..Variants::default() }, &formats);
    }

    /// The terrain and water pipelines now, on this thread (loading time and tests).
    pub fn prepare_terrain_pipelines(&mut self) -> Result<(), GpuError> {
        let formats = self.formats();
        self.pipelines.build_variants(&self.device, Variants { terrain: true, ..Variants::default() }, &formats)
    }

    /// Starts building the form views' pipelines on a worker (a no-op once built, while
    /// building, or after a failure: [`Gpu::retry_failed_variants`] asks again).
    pub fn request_form_pipelines(&mut self) {
        if self.pipelines.variants.form || self.pipelines.ready().failed.is_some_and(|f| f.form) {
            return;
        }
        let formats = self.formats();
        self.pipelines.request_variants(&self.device, Variants { form: true, ..Variants::default() }, &formats);
    }

    /// The form views' pipelines now, on this thread (loading time and tests).
    pub fn prepare_form_pipelines(&mut self) -> Result<(), GpuError> {
        let formats = self.formats();
        self.pipelines.build_variants(&self.device, Variants { form: true, ..Variants::default() }, &formats)
    }

    /// The pipeline variants ready to draw with (a finished background build is taken first).
    pub fn pipeline_variants(&mut self) -> crate::PipelineVariants {
        self.poll_pipelines();
        self.pipelines.ready()
    }

    pub fn retry_failed_variants(&mut self) -> bool {
        self.poll_pipelines();
        let Some(f) = self.pipelines.ready().failed else { return false };
        let formats = self.formats();
        self.pipelines.request_variants(&self.device, Variants { skinned: f.skinned, textured: f.textured, gi: f.gi, gi_pixel: f.gi_pixel, terrain: f.terrain, form: f.form }, &formats);
        true
    }

    pub fn inject_background_fault(&mut self, fault: crate::BackgroundFault) {
        self.pipelines.inject_background_fault(fault);
    }

    /// Takes finished background pipelines; starts a queued request.
    fn poll_pipelines(&mut self) {
        self.pipelines.poll_background(false);
        if let Some((want, _)) = self.pipelines.take_requeue() {
            let formats = self.formats();
            self.pipelines.request_variants(&self.device, want, &formats);
        }
    }

    /// Colour formats scene pipelines are built for: offscreen, HDR, and the window's.
    fn formats(&self) -> Vec<VkFormat> {
        let mut f = vec![TARGET_FORMAT, crate::render::HDR_FORMAT];
        if let Some(w) = &self.window {
            if !f.contains(&w.format()) {
                f.push(w.format());
            }
        }
        f
    }

    pub fn destroy_mesh(&mut self, mesh: MeshId) {
        if shared_mesh(mesh) {
            return; // the shared sprite quad lives as long as the GPU
        }
        self.settle_upload_of(mesh.0);
        if self.pending.is_some() {
            // An asynchronous offscreen render in flight may read it: freed in queue order
            // instead, once a batch submitted after that render finishes (no wait here).
            self.retire_mesh(mesh);
            return;
        }
        let Some(m) = self.meshes.remove(&mesh.0) else { return };
        // A presented frame may still read the mesh.
        self.wait_window_frames();
        match m.pooled {
            Some((block, at, size)) => self.uploads.blocks.free(&self.device, block, at, size),
            None => self.device.destroy_buffer(&m.buffer),
        }
    }

    /// Wait until no window frame in flight can still read a resource about to be freed. Only
    /// with no asynchronous offscreen render pending: that is not a window frame, so callers
    /// retire in queue order instead while one is (`destroy_mesh`, `destroy_texture`).
    fn wait_window_frames(&self) {
        debug_assert!(self.pending.is_none(), "a synchronous free with an asynchronous render in flight");
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
        self.textures.map.insert(id, Texture { image, slot, format, width, height, levels: 1 });
        Ok(TextureId(id))
    }

    pub fn upload_texture_levels(
        &mut self,
        width: u32,
        height: u32,
        format: TextureFormat,
        levels: &[&[u8]],
        sampler: TextureSampler,
    ) -> Result<TextureId, GpuError> {
        let (offsets, total) = self.level_plan(width, height, format, levels)?;
        let vk_sampler = self.descriptors.sampler(&self.device, sampler)?;
        let slot = self.descriptors.alloc_slot()?;
        let dev = &self.device;
        let image = match dev.create_image_with(
            vk_format(format),
            width,
            height,
            levels.len() as u32,
            1,
            VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT,
            VK_IMAGE_ASPECT_COLOR_BIT,
            MemoryKind::Textures,
            "material texture",
        ) {
            Ok(i) => i,
            Err(e) => {
                self.descriptors.free_slot(slot);
                return Err(e);
            }
        };
        let staging = match dev.create_buffer(
            total,
            VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            0,
            MemoryKind::Staging,
            "texture staging",
        ) {
            Ok(b) => b,
            Err(e) => {
                dev.destroy_image(&image);
                self.descriptors.free_slot(slot);
                return Err(e);
            }
        };
        for (level, &at) in levels.iter().zip(&offsets) {
            // SAFETY: `staging.mapped` holds `total` bytes; each level fits at its offset.
            unsafe { ptr::copy_nonoverlapping(level.as_ptr(), (staging.mapped as *mut u8).add(at as usize), level.len()) };
        }
        let regions = level_regions(width, height, &offsets);
        let copied = self.run_sync(|dev, cmd| {
            dev.barrier(
                cmd,
                &[],
                &[image_barrier(
                    image.handle,
                    VK_IMAGE_ASPECT_COLOR_BIT,
                    VK_PIPELINE_STAGE_2_NONE,
                    VK_ACCESS_2_NONE,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_WRITE_BIT,
                    VK_IMAGE_LAYOUT_UNDEFINED,
                    VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                )],
            );
            // SAFETY: recording; every region lies inside the staging buffer and its mip level;
            // the image is in TRANSFER_DST_OPTIMAL.
            unsafe {
                (dev.fns.cmd_copy_buffer_to_image)(
                    cmd,
                    staging.handle,
                    image.handle,
                    VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                    regions.len() as u32,
                    regions.as_ptr(),
                )
            };
            dev.barrier(
                cmd,
                &[],
                &[image_barrier(
                    image.handle,
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
        if let Err(e) = copied {
            self.device.destroy_image(&image);
            self.descriptors.free_slot(slot);
            return Err(e);
        }
        // A fresh slot: no pending frame reads it.
        self.descriptors.write_texture_with(&self.device, slot, image.view, vk_sampler);
        let id = self.next_id();
        self.textures.map.insert(id, Texture { image, slot, format, width, height, levels: levels.len() as u32 });
        Ok(TextureId(id))
    }

    /// [`GpuInner::upload_texture_levels`] without waiting: the copies go into the current
    /// upload batch, submitted before the next frame (see `stream.rs`).
    pub fn queue_texture_levels(
        &mut self,
        width: u32,
        height: u32,
        format: TextureFormat,
        levels: &[&[u8]],
        sampler: TextureSampler,
    ) -> Result<TextureId, GpuError> {
        let vk_sampler = self.descriptors.sampler(&self.device, sampler)?;
        self.queue_image(width, height, format, levels, Some(vk_sampler), "material texture")
    }

    /// [`GpuInner::upload_texture`] without waiting (see [`GpuInner::queue_texture_levels`]).
    pub fn queue_texture(&mut self, width: u32, height: u32, format: TextureFormat, pixels: &[u8]) -> Result<TextureId, GpuError> {
        self.queue_image(width, height, format, &[pixels], None, "texture")
    }

    /// A queued sampled image; `sampler` None: the default (linear) one.
    fn queue_image(
        &mut self,
        width: u32,
        height: u32,
        format: TextureFormat,
        levels: &[&[u8]],
        vk_sampler: Option<VkSampler>,
        what: &str,
    ) -> Result<TextureId, GpuError> {
        let (offsets, total) = self.level_plan(width, height, format, levels)?;
        let slot = self.descriptors.alloc_slot()?;
        let zi = specula_core::profile::zone("gpu.create_image");
        let image = match self.device.create_image_with(
            vk_format(format),
            width,
            height,
            levels.len() as u32,
            1,
            VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT,
            VK_IMAGE_ASPECT_COLOR_BIT,
            MemoryKind::Textures,
            what,
        ) {
            Ok(i) => i,
            Err(e) => {
                self.descriptors.free_slot(slot);
                return Err(e);
            }
        };
        drop(zi);
        let zs = specula_core::profile::zone("gpu.stage");
        let staged = match self.uploads.stage(&self.device, total) {
            Ok(s) => s,
            Err(e) => {
                self.device.destroy_image(&image);
                self.descriptors.free_slot(slot);
                return Err(e);
            }
        };
        drop(zs);
        let _zw = specula_core::profile::zone("gpu.write");
        for (level, &at) in levels.iter().zip(&offsets) {
            // SAFETY: `staged.dst` holds `total` bytes no submitted batch uses; each level fits at
            // its offset.
            unsafe { ptr::copy_nonoverlapping(level.as_ptr(), staged.dst.add(at as usize), level.len()) };
        }
        let mut regions = level_regions(width, height, &offsets);
        let cmd = self.uploads.recording_cmd(&self.device)?;
        crate::stream::record_image_copy(&self.device, cmd, &staged, image.handle, &mut regions);
        // A fresh slot: no pending frame reads it.
        match vk_sampler {
            Some(s) => self.descriptors.write_texture_with(&self.device, slot, image.view, s),
            None => self.descriptors.write_texture(&self.device, slot, image.view),
        }
        let id = self.next_id();
        self.textures.map.insert(id, Texture { image, slot, format, width, height, levels: levels.len() as u32 });
        self.uploads.queued(id, total);
        Ok(TextureId(id))
    }

    /// Checks a texture's mip levels; returns each level's 4-byte aligned offset in one staging
    /// range, and the range's size.
    fn level_plan(&self, width: u32, height: u32, format: TextureFormat, levels: &[&[u8]]) -> Result<(Vec<u64>, u64), GpuError> {
        let Some(&base) = levels.first() else {
            return Err(GpuError::new("upload_texture_levels: no levels given"));
        };
        check_pixels("upload_texture_levels", width, height, format, base)?;
        let max = self.max_image_dimension_2d;
        if width > max || height > max {
            return Err(GpuError::rejected(
                crate::ErrorReason::TooLarge { width, height, max },
                format!("upload_texture_levels: size {width}x{height} exceeds {max} in a dimension"),
            ));
        }
        let full = crate::mip_count(width, height) as usize;
        if levels.len() > full {
            return Err(GpuError::new(format!("upload_texture_levels: {} levels given; a {width}x{height} image has {full}", levels.len())));
        }
        let mut offsets = Vec::with_capacity(levels.len());
        let mut total = 0u64;
        for (i, level) in levels.iter().enumerate() {
            let (w, h) = ((width >> i).max(1), (height >> i).max(1));
            check_pixels(&format!("upload_texture_levels (level {i})"), w, h, format, level)?;
            offsets.push(total);
            total = (total + level.len() as u64).div_ceil(4) * 4;
        }
        Ok((offsets, total))
    }

    /// Writes a region of a one-level texture, queued: the copy goes in the upload batch,
    /// submitted before the next frame or offscreen render on the same queue, so that frame
    /// samples the new texels and frames already submitted finish reading the old ones first
    /// (the batch's barrier). A HUD's glyph atlas growing used to wait for the GPU here (a fence
    /// per update, 10-32 ms on a loaded iGPU).
    ///
    /// Two cases still cost the caller's thread, and callers that run every frame should stay
    /// out of them: with no batch recording and all six upload slots in flight it waits for the
    /// oldest one (`gpu.slot_wait`), and an update over 8 MB gets a staging buffer of its own,
    /// allocated here.
    pub fn update_texture(&mut self, texture: TextureId, x: u32, y: u32, width: u32, height: u32, pixels: &[u8]) -> Result<(), GpuError> {
        let t = self.textures.map.get(&texture.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Texture, texture.0, ""))?;
        if t.levels > 1 {
            return Err(GpuError::new(format!(
                "update_texture: texture {} has {} mip levels (a material texture); upload it again instead",
                texture.0, t.levels
            )));
        }
        check_pixels("update_texture", width, height, t.format, pixels)?;
        if x as u64 + width as u64 > t.width as u64 || y as u64 + height as u64 > t.height as u64 {
            let reason =
                crate::ErrorReason::RegionOutside { x, y, width, height, texture_width: t.width, texture_height: t.height };
            return Err(GpuError::rejected(
                reason,
                format!("update_texture: region {width}x{height} at ({x}, {y}) leaves the {}x{} texture", t.width, t.height),
            ));
        }
        let image = t.image.handle;
        let bytes = pixels.len() as u64;
        let staged = {
            let _z = specula_core::profile::zone("gpu.stage");
            self.uploads.stage(&self.device, bytes)?
        };
        // SAFETY: `staged.dst` holds at least `bytes` bytes that no submitted batch uses.
        unsafe { ptr::copy_nonoverlapping(pixels.as_ptr(), staged.dst, pixels.len()) };
        let cmd = self.uploads.recording_cmd(&self.device)?;
        crate::stream::record_image_update(&self.device, cmd, &staged, image, (x, y, width, height));
        // Pending until its batch finishes: destroying the texture first settles it.
        self.uploads.queued(texture.0, bytes);
        Ok(())
    }

    pub fn has_texture(&self, texture: TextureId) -> bool {
        self.textures.map.contains_key(&texture.0)
    }

    pub fn destroy_texture(&mut self, texture: TextureId) {
        self.settle_upload_of(texture.0);
        if self.pending.is_some() {
            // An asynchronous offscreen render may sample it: freed in queue order (see
            // `destroy_mesh`).
            self.retire_texture(texture);
            return;
        }
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
        // The readback's memory (16 MB at 2048 x 2048, host-visible: 10-45 ms of
        // `vkAllocateMemory` on a loaded iGPU) is allocated on the image heap's worker; the first
        // use finishes it (waiting only if it isn't back), a prewarm polls it.
        let readback = match dev.create_buffer_ahead(
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
        let none = Buffer { handle: VK_NULL_HANDLE, memory: VK_NULL_HANDLE, mapped: ptr::null_mut(), bytes: 0, kind: MemoryKind::Readback };
        self.targets.insert(
            id,
            Target { color, readback: none, readback_ahead: Some(readback), extent: VkExtent2D { width, height }, graph, rendered: false, copied: false },
        );
        Ok(TargetId(id))
    }

    /// Whether `target`'s readback buffer is there (`wait`: makes sure it is, waiting for the
    /// image heap's worker if it must).
    fn target_readback(&mut self, target: u32, wait: bool) -> Result<bool, GpuError> {
        let Some(t) = self.targets.get_mut(&target) else {
            return Err(GpuError::unknown(crate::IdKind::Target, target, ""));
        };
        let Some(p) = t.readback_ahead.take() else { return Ok(true) };
        match self.device.finish_buffer(p, wait)? {
            Ok(b) => {
                t.readback = b;
                Ok(true)
            }
            Err(p) => {
                t.readback_ahead = Some(p);
                Ok(false)
            }
        }
    }

    pub fn destroy_target(&mut self, target: TargetId) {
        if self.pending.as_ref().is_some_and(|p| p.target == target.0) {
            let _ = self.settle_offscreen(true);
        }
        let Some(mut t) = self.targets.remove(&target.0) else { return };
        // Offscreen work is synchronous (an asynchronous render of it was settled above), so
        // nothing on the GPU still uses the target or its graph's images.
        self.pool.release_owner(&self.device, &mut self.descriptors, target.0);
        self.ids.forget(target.0);
        if let Some(p) = t.readback_ahead.take() {
            self.device.abandon_buffer(p);
        }
        self.device.destroy_buffer(&t.readback);
        self.device.destroy_image(&t.color);
        if self.last_graph == Some(GraphSource::Target(target.0)) {
            self.last_graph = None;
        }
    }

    pub fn render(&mut self, target: TargetId, frame: &FrameDesc) -> Result<(), GpuError> {
        self.render_frame(target, frame, None, true, false)
    }

    /// Makes `target`'s render graph images ahead of its first render, for a frame in `env`
    /// with `content` (`None`: like the latest offscreen frame's, else the window's; else shadows
    /// from `env` alone), with sight's ID passes when `ids`. Works until `until` (at least one image a
    /// call), never waits for the GPU, binds and retires nothing: `Ok(true)` once every image
    /// exists. A frame whose content differs makes what the prediction missed, as before.
    pub fn prewarm_target(&mut self, target: TargetId, env: &crate::Environment, ids: bool, content: Option<ContentHint>, until: Instant) -> Result<bool, GpuError> {
        let _z = specula_core::profile::zone("gpu.prewarm");
        // The target's readback and the ID buffers for its size come from the image heap's
        // worker: polled here, never waited for.
        let readback = {
            let _z = specula_core::profile::zone("gpu.prewarm_readback");
            self.target_readback(target.0, false)?
        };
        let hint = content.or(self.hint_offscreen).or(self.hint_window).unwrap_or(ContentHint { shadow_casters: true, ..ContentHint::default() });
        let Some(t) = self.targets.get_mut(&target.0) else {
            return Err(GpuError::unknown(crate::IdKind::Target, target.0, ""));
        };
        let extent = t.extent;
        let shape = FrameShape {
            out_name: "target",
            format: TARGET_FORMAT,
            extent,
            prior: Prior::Discard { after: VK_PIPELINE_STAGE_2_COPY_BIT },
            fin: Final::TransferSrc,
            mode: PostMode::of(env).fit(extent),
            shadow: (env.shadow.enabled && hint.shadow_casters).then(|| shadow_desc(&env.shadow)),
            overlay: hint.overlay,
            two_d: hint.two_d,
            ssao: crate::ssao::ssao_extent(env, extent),
            particles: hint.particles,
            decals: hint.decals,
            water: hint.water,
            blended: hint.blended,
            flash: false,
            flash_reset: false,
        };
        declare_frame(&mut t.graph, &shape);
        if ids {
            crate::idbuffer::declare(&mut t.graph, extent, crate::idbuffer::bucket_for(extent, self.ids.bucket));
        }
        let images = t.graph.prewarm(&self.device, &mut self.descriptors, &mut self.pool, until)?;
        let id_buffers = {
            let _z = specula_core::profile::zone("gpu.prewarm_ids");
            !ids || self.ids.prewarm(&self.device, extent, hint.blended || hint.two_d)?
        };
        Ok(images && readback && id_buffers)
    }

    /// Transient images `target`'s graph, as last declared, still lacks (tests).
    pub fn target_images_missing(&self, target: TargetId) -> usize {
        self.targets.get(&target.0).map_or(0, |t| t.graph.missing_transients(&self.pool))
    }

    /// [`GpuInner::render`] (or `render_with_ids` with `ids`), submitted without waiting: the
    /// colour image is copied into the target's readback buffer in the same submission. The
    /// render is settled by [`GpuInner::settle_offscreen`] (a poll, or a wait); every other
    /// offscreen use (another render, a readback, destroying the target) settles it first.
    pub fn render_async(&mut self, target: TargetId, frame: &FrameDesc, ids: Option<(&[u32], &[[u32; 4]])>) -> Result<(), GpuError> {
        if let Some((ids, occluders)) = ids {
            check_ids(frame, ids, occluders)?;
        }
        self.render_frame(target, frame, ids, false, true)
    }

    /// [`GpuInner::render_async`] with no ID buffer and **no copy of its image**: a render whose
    /// timings are all that is wanted (`perf_light`). The same render; only the copy into the
    /// readback buffer is left out of its submission, so a later `read_rgba8` of the target makes
    /// that copy itself.
    pub fn render_async_timings(&mut self, target: TargetId, frame: &FrameDesc) -> Result<(), GpuError> {
        self.render_frame(target, frame, None, false, false)
    }

    /// What offscreen images were copied back, and read, since start-up ([`Readbacks`]).
    pub fn readbacks(&self) -> Readbacks {
        self.readbacks
    }

    /// Whether an asynchronous render is in flight (not yet settled).
    pub fn offscreen_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn offscreen_pending_target(&self) -> Option<TargetId> {
        self.pending.as_ref().map(|p| TargetId(p.target))
    }

    /// The next asynchronous render to settle reports a lost device once it has really ended.
    pub fn inject_offscreen_fault(&mut self) {
        self.offscreen_fault = true;
    }

    /// Settles the asynchronous render in flight, if any: with `wait`, waits for it; otherwise
    /// only looks (`Ok(false)`: still running). Once settled (`Ok(true)`, also when none was in
    /// flight) its ID results, readback and stats are readable. A failed render (device lost,
    /// timeout) settles with its error.
    pub fn settle_offscreen(&mut self, wait: bool) -> Result<bool, GpuError> {
        self.settle_offscreen_within(if wait { None } else { Some(Duration::ZERO) })
    }

    /// [`GpuInner::settle_offscreen`] waiting at most `limit` (`None`: until it ends; zero: a
    /// poll): `Ok(false)` when it is still running then.
    pub fn settle_offscreen_within(&mut self, limit: Option<Duration>) -> Result<bool, GpuError> {
        let Some(p) = &self.pending else { return Ok(true) };
        let dev = &self.device;
        let started = Instant::now();
        let mut waited = match limit {
            None => {
                let _z = specula_core::profile::zone("vk.wait_fence");
                dev.wait_fence(self.async_fence)
            }
            Some(limit) => {
                let _z = (!limit.is_zero()).then(|| specula_core::profile::zone("vk.wait_fence"));
                let ns = u64::try_from(limit.as_nanos()).unwrap_or(u64::MAX);
                // SAFETY: a live fence of this device; the timeout bounds the wait (zero: a poll).
                let r = unsafe { (dev.fns.wait_for_fences)(dev.handle, 1, &self.async_fence, VK_TRUE, ns) };
                match r {
                    VK_TIMEOUT => return Ok(false),
                    VK_SUCCESS => Ok(()),
                    r => Err(GpuError::vk("vkWaitForFences (offscreen poll)", r)),
                }
            }
        };
        if waited.is_ok() && std::mem::take(&mut self.offscreen_fault) {
            // (Tests: a render that ended in a lost device, after it really ended.)
            waited = Err(GpuError::vk("vkWaitForFences (offscreen, injected)", VK_ERROR_DEVICE_LOST));
        }
        if waited.is_err() {
            // Timed out or lost: drain the queue before anyone frees what the render touched.
            dev.wait_idle();
        }
        let reset = dev.reset_fence(self.async_fence);
        let wait_ms = started.elapsed().as_secs_f64() * 1e3;
        let target = p.target;
        let p = self.pending.take().expect("checked");
        self.completed = self.completed.max(p.submission);
        let stats = &mut self.last_offscreen;
        if let (true, Ok(()), Some(t)) = (p.timed, &waited, self.timer.as_mut()) {
            match t.read_into(&self.device, &mut stats.gpu) {
                Ok(true) => stats.timestamp_error = None,
                Ok(false) => {}
                Err(r) => stats.timestamp_error = Some(r),
            }
        }
        // How long the engine thread waited for it here (none when a poll found it done).
        stats.cpu = CpuTiming { wait_ms, record_ms: p.prepare_ms + p.times.record_ms, submit_ms: p.times.submit_ms };
        waited.and(reset)?;
        self.ids.finished();
        if let Some(t) = self.targets.get_mut(&target) {
            t.copied = p.copied;
        }
        Ok(true)
    }

    /// `render`, plus sight's ID buffer: `ids` holds one id per draw of `frame` (0 = not
    /// counted), `occluders` pixel rectangles that hide what is under them in the ID buffer
    /// (`Gpu::render_with_ids_occluded`); read it back with [`GpuInner::read_ids`].
    pub fn render_with_ids(&mut self, target: TargetId, frame: &FrameDesc, ids: &[u32], occluders: &[[u32; 4]]) -> Result<(), GpuError> {
        check_ids(frame, ids, occluders)?;
        self.render_frame(target, frame, Some((ids, occluders)), true, false)
    }

    /// [`GpuInner::read_ids`] inside `rect` (`[x0, y0, x1, y1)`, clipped) only.
    pub fn read_ids_rect(&mut self, target: TargetId, rect: [u32; 4]) -> Result<IdImage, GpuError> {
        self.settle_offscreen(true)?;
        self.targets.get(&target.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Target, target.0, ""))?;
        self.ids.image_rect(target.0, rect)
    }

    /// The opaque id image of the target's last render (`render_with_ids`).
    pub fn read_ids(&mut self, target: TargetId) -> Result<IdImage, GpuError> {
        self.settle_offscreen(true)?;
        self.targets.get(&target.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Target, target.0, ""))?;
        self.ids.image(target.0)
    }

    /// Per-id coverage of the target's last ID render, opaque ids counted in place (no copy).
    pub fn id_coverage(&self, target: TargetId, max_id: u32) -> Result<IdCounts, GpuError> {
        self.targets.get(&target.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Target, target.0, ""))?;
        if self.pending.is_some() {
            return Err(GpuError::new("id_coverage: an asynchronous render is still in flight (settle_offscreen first)"));
        }
        self.ids.counts(target.0, max_id)
    }

    /// The draws the last ID render's colour passes recorded ([`crate::render::ColourRecord`]):
    /// `(mesh draw indices, 2D draw indices)`, after every readiness check. Empty before any.
    pub fn last_colour_draws(&self) -> (Vec<u32>, Vec<u32>) {
        let r = self.colour_record.borrow();
        (r.draws.clone(), r.draws_2d.clone())
    }

    /// Makes later ID renders' colour passes leave out the batches holding these mesh draws (empty:
    /// none): a fault injected by tests of sight's packet checks, the way a draw whose pipeline
    /// isn't ready is left out. Never used by the engine.
    pub fn debug_skip_colour_draws(&mut self, draws: &[usize]) {
        self.colour_record.borrow_mut().skip = draws.to_vec();
    }

    /// Whether ID renders count blended draws (`fragmentStoresAndAtomics`, not switched off).
    pub fn ids_count_blended(&self) -> bool {
        self.device.fragment_atomics && !self.ids.blended_disabled
    }

    /// Switch blended counting off (or back on, where the device can): tests and comparisons of
    /// the path devices without fragment atomics take.
    pub fn set_ids_count_blended(&mut self, on: bool) {
        self.ids.blended_disabled = !on;
    }

    /// `copy`: an asynchronous render (`!wait`) copies its image into the readback buffer in its
    /// own submission (a synchronous one never does: `read_rgba8` copies when asked).
    fn render_frame(&mut self, target: TargetId, frame: &FrameDesc, ids: Option<(&[u32], &[[u32; 4]])>, wait: bool, copy: bool) -> Result<(), GpuError> {
        let copy = copy && !wait;
        // One offscreen render at a time: they share the ID buffers, the frame upload buffer and
        // the timer.
        self.settle_offscreen(true)?;
        let zp = specula_core::profile::zone("gpu.prepare");
        check_frame(&self.meshes, &self.textures, frame)?;
        // Its readback (made ahead on the image heap's worker; a prewarm has usually finished it).
        self.target_readback(target.0, true)?;
        // Queued uploads go first on the queue: the frame draws with them.
        {
            let _z = specula_core::profile::zone("gpu.flush_uploads");
            self.flush_uploads()?;
        }
        self.poll_pipelines();
        if !self.targets.contains_key(&target.0) {
            return Err(GpuError::unknown(crate::IdKind::Target, target.0, ""));
        }
        self.bind_gi(frame, true);
        self.bind_glyphs(frame);
        let Some(t) = self.targets.get_mut(&target.0) else {
            return Err(GpuError::unknown(crate::IdKind::Target, target.0, ""));
        };
        let extent = t.extent;
        // Whatever happens, the shared ID buffers no longer describe an earlier render.
        self.ids.starting(target.0, ids.is_some());
        let start = Instant::now();
        let upload = self.upload.as_mut().expect("created in new");
        // Offscreen work is synchronous: no earlier frame still reads this buffer.
        let zf = specula_core::profile::zone("gpu.prepare_frame");
        prepare_frame(&self.device, &self.meshes, &self.textures, frame, extent, upload, &mut self.scratch)?;
        drop(zf);
        let env = &frame.environment;
        // Sight's renders (with ids) predict the next capture's shape; other offscreen renders
        // (the asset studio's stage, filmstrips) pass their own content.
        if ids.is_some() {
            self.hint_offscreen = Some(ContentHint::of(frame, &self.scratch));
        }
        let shadow = (env.shadow.enabled && self.scratch.shadow.data.count > 0).then(|| shadow_desc(&env.shadow));
        let features = frame_features(frame, &self.scratch, shadow.is_some());
        let mode = PostMode::of(env).fit(extent);
        let zpl = specula_core::profile::zone("gpu.pipelines");
        let pipelines = self.pipelines.frame(&self.device, TARGET_FORMAT, features, mode, env.sky.enabled, !frame.overlay.is_empty(), (!frame.effects.particles.is_empty(), !frame.effects.decals.is_empty()), false)?;
        drop(zpl);
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
            two_d: !frame.draws_2d.is_empty(),
            ssao: crate::ssao::ssao_extent(env, extent),
            particles: !frame.effects.particles.is_empty(),
            decals: !self.scratch.effects.decals.is_empty(),
            water: self.scratch.plan.batches.iter().any(|b| b.material == crate::Material::Water),
            blended: self.scratch.plan.batches.iter().any(|b| b.material.blended() && b.material != crate::Material::Water),
            flash: false,
            flash_reset: false,
        };
        let res = declare_frame(&mut t.graph, &shape);
        // Sight's ID passes come after the frame's own, so the image is the same with or without.
        let id_res = ids.map(|_| {
            self.ids.bucket = crate::idbuffer::bucket_for(extent, self.ids.bucket);
            crate::idbuffer::declare(&mut t.graph, extent, self.ids.bucket)
        });
        {
            let _z = specula_core::profile::zone("gpu.prepare_graph");
            prepare_graph(&self.device, &mut self.descriptors, &mut self.pool, &mut t.graph, &res)?;
        }
        t.graph.bind_image(res.out, t.color.handle, t.color.view);
        let id_frame = match (ids, id_res) {
            (Some((ids, occluders)), Some(id_res)) => {
                let _z = specula_core::profile::zone("gpu.ids_frame");
                let plans = (&self.scratch.plan, &self.scratch.two_d);
                self.ids.set_occluders(occluders, extent);
                Some(self.ids.frame(&self.device, &mut self.pipelines, plans, frame, ids, id_res, target.0, extent, &mut t.graph)?)
            }
            _ => None,
        };
        let prepare_ms = start.elapsed().as_secs_f64() * 1e3;
        drop(zp);
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
            ids: id_frame,
            id_occluders: if id_frame.is_some() { self.ids.occluders() } else { &[] },
            textures: &self.textures,
            recorded: id_frame.is_some().then_some(&self.colour_record),
        };
        if id_frame.is_some() {
            let mut r = self.colour_record.borrow_mut();
            r.draws.clear();
            r.draws_2d.clear();
        }
        let graph = &mut t.graph;
        let (color, readback) = (t.color.handle, t.readback.handle);
        let mut record = |dev: &Device, cmd: VkCommandBuffer, draws: &mut crate::DrawStats, timer: Option<&mut PassTimer>| {
            let mut timer = timer;
            if let Some(t) = timer.as_mut() {
                t.begin_frame(dev, cmd, frame_no);
            }
            *draws = record_frame(&ctx, cmd, graph, &res, timer);
            if copy {
                // The readback rides in the same submission (see `render_async`).
                record_readback(dev, cmd, color, readback, extent);
            }
        };
        let ran = if wait {
            run_sync(&self.device, self.sync_cmd, self.sync_fence, |dev, cmd| record(dev, cmd, &mut draws, timer.as_mut()))
        } else {
            submit_async(&self.device, self.async_cmd, self.async_fence, |dev, cmd| record(dev, cmd, &mut draws, timer.as_mut()))
        };
        // A synchronous render completed (or failed with nothing pending): its timestamps are
        // readable; an asynchronous one's are read when it is settled. Stats are updated in place,
        // reusing their storage.
        let stats = &mut self.last_offscreen;
        if let (Ok(_), Some(t)) = (&ran, timer.as_mut()) {
            t.submitted();
            if wait {
                match t.read_into(&self.device, &mut stats.gpu) {
                    Ok(true) => stats.timestamp_error = None,
                    Ok(false) => {}
                    Err(r) => stats.timestamp_error = Some(r),
                }
            }
        }
        let timed = timer.is_some();
        self.timer = timer;
        let times = ran?;
        let submission = self.pool.note_used(&t.graph);
        if wait {
            // Synchronous: this frame and every one before it finished.
            self.completed = submission;
            self.ids.finished();
        } else {
            self.pending = Some(Pending { target: target.0, submission, timed, prepare_ms, times, copied: copy });
            if copy {
                self.readbacks.gpu_copies += 1;
            }
        }
        t.rendered = true;
        t.copied = false;
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

    /// `target`'s last render as it landed: its colour (copied back in its own submission) and its
    /// opaque ids. Never waits and never records a copy: `Ok(None)` while any asynchronous render
    /// is in flight (settle it first); an error when the render left its colour on the GPU or
    /// another target's ID render came after it (render it again).
    pub fn read_landed(&mut self, target: TargetId) -> Result<Option<(Vec<u8>, IdImage)>, GpuError> {
        if self.pending.is_some() {
            return Ok(None);
        }
        let t = self.targets.get(&target.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Target, target.0, ""))?;
        if !t.rendered || !t.copied {
            return Err(GpuError::new(format!("read_landed: target {}'s last render was not copied back in its submission (render_async with the copy)", target.0)));
        }
        let ids = self.ids.image(target.0)?;
        let len = t.extent.width as usize * t.extent.height as usize * 4;
        // SAFETY: as `read_rgba8`'s: persistently mapped coherent memory of `len` bytes; the
        // render's own submission copied into it and has ended (nothing is pending, so it was
        // settled: fence signalled, host-read barrier recorded); nothing writes it while `self`
        // is borrowed.
        let colour = unsafe { std::slice::from_raw_parts(t.readback.mapped as *const u8, len) }.to_vec();
        self.readbacks.host_reads += 1;
        Ok(Some((colour, ids)))
    }

    pub fn read_rgba8(&mut self, target: TargetId) -> Result<Vec<u8>, GpuError> {
        if self.pending.as_ref().is_some_and(|p| p.target == target.0) {
            self.settle_offscreen(true)?;
        }
        let t = self.targets.get(&target.0).ok_or_else(|| GpuError::unknown(crate::IdKind::Target, target.0, ""))?;
        if !t.rendered {
            return Err(GpuError::new(format!("read_rgba8: target {} has not been rendered yet", target.0)));
        }
        let (image, buffer, extent, copied) = (t.color.handle, t.readback.handle, t.extent, t.copied);
        if !copied {
            self.run_sync(|dev, cmd| record_readback(dev, cmd, image, buffer, extent))?;
            self.readbacks.gpu_copies += 1;
        }
        self.readbacks.host_reads += 1;
        let t = self.targets.get(&target.0).expect("checked above");
        let len = extent.width as usize * extent.height as usize * 4;
        let _z = specula_core::profile::zone("gpu.readback_copy");
        // SAFETY: the readback buffer is persistently mapped coherent memory of `len` bytes and
        // the copy into it completed (fence waited or settled, host-read barrier recorded);
        // nothing writes it while `self` is borrowed. Copied out once (no zeroed buffer first).
        let out = unsafe { std::slice::from_raw_parts(t.readback.mapped as *const u8, len) }.to_vec();
        Ok(out)
    }

    pub fn attach_window(&mut self, hinstance: *mut c_void, hwnd: *mut c_void, width: u32, height: u32) -> Result<(), GpuError> {
        if let Some(mut old) = self.window.take() {
            self.device.wait_idle();
            self.pool.release_owner(&self.device, &mut self.descriptors, WINDOW_OWNER);
            old.release_slots(&mut self.descriptors);
            old.destroy(&self.instance, &self.device, self.cmd_pool);
            if self.last_graph == Some(GraphSource::Window) {
                self.last_graph = None;
            }
        }
        let window = Window::new(&self.instance, &self.device, self.cmd_pool, hinstance, hwnd, width, height)?;
        // A background build in flight doesn't know this format: finish it first.
        self.pipelines.poll_background(true);
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
        // Queued uploads go first on the queue: the frame draws with them.
        self.flush_uploads()?;
        self.poll_pipelines();
        self.bind_gi(frame, false);
        self.bind_glyphs(frame);
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
            self.hint_window = Some(ContentHint::of(frame, &self.scratch));
        }
        Ok(())
    }

    pub fn last_window_timing(&self) -> (u64, CpuTiming) {
        (self.last_window.frame, self.last_window.cpu)
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

    /// The window's last flash watch blocks, borrowed (no copy of the frame's stats).
    pub fn flash_grid(&self) -> &crate::FlashGrid {
        &self.last_window.flash
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
        self.pending = None;
        self.uploads.destroy(dev);
        if let Some(mut w) = self.window.take() {
            w.destroy(&self.instance, dev, self.cmd_pool);
        }
        for (_, m) in self.meshes.drain() {
            // Block meshes went with the blocks (`uploads.destroy` above).
            if m.pooled.is_none() {
                dev.destroy_buffer(&m.buffer);
            }
        }
        for (_, t) in self.textures.map.drain() {
            dev.destroy_image(&t.image);
        }
        for (_, mut t) in self.targets.drain() {
            if let Some(p) = t.readback_ahead.take() {
                dev.abandon_buffer(p);
            }
            dev.destroy_buffer(&t.readback);
            dev.destroy_image(&t.color);
        }
        self.pool.destroy(dev, &mut self.descriptors);
        if let Some(u) = self.upload.take() {
            u.destroy(dev);
        }
        self.ids.destroy(dev);
        self.gi.destroy(dev);
        self.glyphs.destroy(dev);
        if let Some(mut t) = self.timer.take() {
            t.destroy(dev);
        }
        self.pipelines.destroy(dev);
        self.descriptors.destroy(dev);
        // SAFETY: device idle; objects belong to `dev` and are destroyed once (null is ignored).
        unsafe {
            (dev.fns.destroy_fence)(dev.handle, self.sync_fence, ptr::null());
            (dev.fns.destroy_fence)(dev.handle, self.async_fence, ptr::null());
            (dev.fns.destroy_command_pool)(dev.handle, self.cmd_pool, ptr::null()); // frees sync_cmd
        }
        // `device`, then `instance` drop next (field order).
    }
}

const _: () = assert!(size_of::<Vertex>() == 28 && size_of::<SkinVertex>() == 8);

/// A mesh's streams, checked and laid out for one buffer: vertices, then skin, then surface
/// (UV + tangent), then indices.
struct MeshLayout {
    skin: Option<Vec<SkinVertex>>,
    surface: Option<Vec<SurfaceVertex>>,
    /// Bone matrices a skinned draw's palette needs (see `Mesh::joints`).
    joints: u32,
    vb_bytes: u64,
    skin_offset: u64,
    surface_offset: u64,
    index_offset: u64,
    total: u64,
}

impl MeshLayout {
    /// Writes the mesh's bytes at `dst`.
    ///
    /// # Safety
    /// `dst` must point to at least `self.total` writable bytes.
    unsafe fn write(&self, dst: *mut u8, vertices: &[Vertex], indices: &[u32]) {
        let skin_bytes = (self.surface_offset - self.skin_offset) as usize;
        let surface_bytes = (self.index_offset - self.surface_offset) as usize;
        let index_bytes = (self.total - self.index_offset) as usize;
        // SAFETY: the caller gives `total` bytes at `dst`; `Vertex`, `SkinVertex` and
        // `SurfaceVertex` are `repr(C)` plain data, as are `u32` indices; the regions are laid
        // out one after another and don't overlap.
        unsafe {
            ptr::copy_nonoverlapping(vertices.as_ptr() as *const u8, dst, self.vb_bytes as usize);
            if let Some(skin) = &self.skin {
                ptr::copy_nonoverlapping(skin.as_ptr() as *const u8, dst.add(self.skin_offset as usize), skin_bytes);
            }
            if let Some(s) = &self.surface {
                ptr::copy_nonoverlapping(s.as_ptr() as *const u8, dst.add(self.surface_offset as usize), surface_bytes);
            }
            ptr::copy_nonoverlapping(indices.as_ptr() as *const u8, dst.add(self.index_offset as usize), index_bytes);
        }
    }
}

/// Checks a mesh and lays its streams out (see [`MeshLayout`]).
fn mesh_layout(vertices: &[Vertex], indices: &[u32], streams: &MeshStreams) -> Result<MeshLayout, GpuError> {
    if vertices.is_empty() || indices.is_empty() {
        return Err(GpuError::new(format!(
            "upload_mesh: need at least one vertex and one triangle (got {} vertices, {} indices)",
            vertices.len(),
            indices.len()
        )));
    }
    if !indices.len().is_multiple_of(3) {
        return Err(GpuError::new(format!("upload_mesh: index count {} is not a multiple of 3", indices.len())));
    }
    if let Some((at, &bad)) = indices.iter().enumerate().find(|(_, &i)| i as usize >= vertices.len()) {
        return Err(GpuError::new(format!("upload_mesh: index {bad} at position {at} is out of range ({} vertices)", vertices.len())));
    }
    let n = vertices.len();
    let bad = |stream: MeshStream, vertex: usize, why: String| {
        GpuError::rejected(ErrorReason::BadStream { stream, vertex: vertex as u32 }, format!("upload_mesh: {why}"))
    };
    let mut joints = 0u32;
    // The shader reads all four joints (a zero weight multiplies, it doesn't skip): joints
    // without weight are stored as 0, so every joint read lies inside the draw's palette.
    let skin: Option<Vec<SkinVertex>> = match streams.skin {
        None => None,
        Some(skin) => {
            if skin.len() != n {
                return Err(bad(MeshStream::Skin, skin.len(), format!("{} skin entries for {n} vertices", skin.len())));
            }
            let mut clean = Vec::with_capacity(n);
            for (i, s) in skin.iter().enumerate() {
                let mut c = *s;
                for k in 0..4 {
                    if s.weights[k] == 0 {
                        c.joints[k] = 0;
                    } else if s.joints[k] == 255 {
                        return Err(bad(MeshStream::Skin, i, format!("vertex {i} gives weight to joint 255 (joints are 0..=254)")));
                    }
                    joints = joints.max(c.joints[k] as u32 + 1);
                }
                clean.push(c);
            }
            Some(clean)
        }
    };
    let surface: Option<Vec<SurfaceVertex>> = match streams.uvs {
        None => None,
        Some(uvs) => {
            if uvs.len() != n {
                return Err(bad(MeshStream::Uvs, uvs.len(), format!("{} UVs for {n} vertices", uvs.len())));
            }
            let generated;
            let tangents = match streams.tangents {
                Some(t) if t.len() != n => {
                    return Err(bad(MeshStream::Tangents, t.len(), format!("{} tangents for {n} vertices", t.len())));
                }
                Some(t) => t,
                None => {
                    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| v.position).collect();
                    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| v.normal).collect();
                    generated = crate::generate_tangents(&positions, &normals, uvs, indices);
                    &generated
                }
            };
            Some(uvs.iter().zip(tangents).map(|(&uv, t)| SurfaceVertex { uv, tangent: snorm16_tangent(*t) }).collect())
        }
    };
    let vb_bytes = std::mem::size_of_val(vertices) as u64;
    let skin_bytes = skin.as_ref().map_or(0, |s| std::mem::size_of_val(s.as_slice()) as u64);
    let surface_bytes = surface.as_ref().map_or(0, |s| std::mem::size_of_val(s.as_slice()) as u64);
    let ib_bytes = std::mem::size_of_val(indices) as u64;
    let skin_offset = vb_bytes;
    let surface_offset = skin_offset + skin_bytes;
    let index_offset = surface_offset + surface_bytes;
    Ok(MeshLayout { skin, surface, joints, vb_bytes, skin_offset, surface_offset, index_offset, total: index_offset + ib_bytes })
}

/// One copy region per mip level of a `width × height` image, at `offsets` in its staging.
fn level_regions(width: u32, height: u32, offsets: &[u64]) -> Vec<VkBufferImageCopy> {
    offsets
        .iter()
        .enumerate()
        .map(|(i, &at)| VkBufferImageCopy {
            buffer_offset: at,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: VkImageSubresourceLayers {
                aspect_mask: VK_IMAGE_ASPECT_COLOR_BIT,
                mip_level: i as u32,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: VkOffset3D { x: 0, y: 0, z: 0 },
            image_extent: VkExtent3D { width: (width >> i).max(1), height: (height >> i).max(1), depth: 1 },
        })
        .collect()
}

/// A device-local buffer for a mesh's vertex and index streams.
pub(crate) fn create_mesh_buffer(dev: &Device, total: u64) -> Result<Buffer, GpuError> {
    dev.create_buffer(
        total,
        VK_BUFFER_USAGE_VERTEX_BUFFER_BIT | VK_BUFFER_USAGE_INDEX_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT,
        VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT,
        0,
        MemoryKind::Meshes,
        "mesh",
    )
}

/// A tangent as the surface stream stores it: xyz normalized, snorm16, w the sign (±1).
fn snorm16_tangent(t: [f32; 4]) -> [i16; 4] {
    let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
    let n = |v: f32| if len > 1e-20 && len.is_finite() { v / len } else { 0.0 };
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    [q(n(t[0])), q(n(t[1])), q(n(t[2])), if t[3] < 0.0 { -32767 } else { 32767 }]
}

/// The GI pipeline set a tier draws with: the lean one (`low`) or the per-pixel one (`high`).
fn gi_variants(per_pixel: bool) -> Variants {
    Variants { gi: !per_pixel, gi_pixel: per_pixel, ..Variants::default() }
}

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

    /// Changing the shadow map is atomic: a frame whose graph fails to prepare after the new map
    /// was created leaves the old map alive and in descriptor binding 0; the next good frame
    /// swaps it; freeing a map binding 0 points at makes the next frame rewrite the binding.
    #[test]
    fn shadow_map_swap_survives_a_failed_frame() {
        use crate::graph::SHARED_OWNER;
        use crate::render::{declare_frame, prepare_graph, FrameShape};
        use crate::ShadowSettings;
        let test = "shadow_map_swap_survives_a_failed_frame";
        if !crate::is_available() {
            eprintln!("{test}: SKIPPED — Vulkan is not available");
            return;
        }
        let options = GpuOptions { validation: Validation::On, ..Default::default() };
        let mut gpu = match GpuInner::new(&options) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("{test}: SKIPPED — {e}");
                return;
            }
        };
        let extent = VkExtent2D { width: 64, height: 32 };
        let shape = |resolution: u32, bloom: bool| FrameShape {
            out_name: "target",
            format: TARGET_FORMAT,
            extent,
            prior: Prior::Discard { after: VK_PIPELINE_STAGE_2_COPY_BIT },
            fin: Final::TransferSrc,
            mode: PostMode { bloom_levels: if bloom { 2 } else { 0 }, fxaa: false, ..Default::default() },
            shadow: Some(shadow_desc(&ShadowSettings { enabled: true, resolution, ..Default::default() })),
            overlay: false,
            two_d: false,
            ssao: None,
            particles: false,
            decals: false,
            water: false,
            blended: false,
            flash: false,
            flash_reset: false,
        };
        let mut g = FrameGraph::new(77);
        let shadow_view = |gpu: &GpuInner| gpu.descriptors.shadow_view;
        // Frame 1: a 256² map, bound in binding 0.
        let res = declare_frame(&mut g, &shape(256, false));
        prepare_graph(&gpu.device, &mut gpu.descriptors, &mut gpu.pool, &mut g, &res).unwrap();
        let old = g.image(res.shadow.unwrap()).array_view;
        assert_eq!(shadow_view(&gpu), old);
        // Frame 2: a 512² map is created, then a later image (the HDR target) fails.
        gpu.pool.fail_creates = Some(1);
        let res = declare_frame(&mut g, &shape(512, true));
        let err = prepare_graph(&gpu.device, &mut gpu.descriptors, &mut gpu.pool, &mut g, &res).unwrap_err();
        assert!(err.message.contains("injected"), "{err}");
        gpu.pool.fail_creates = None;
        // Nothing pending: collect everything collectable. The old map must survive.
        gpu.pool.collect(&gpu.device, &mut gpu.descriptors, u64::MAX);
        assert_eq!(shadow_view(&gpu), old, "binding 0 still points at the old map");
        assert!(gpu.pool.has_view(old), "the old map is still alive");
        // Frame 3: the swap completes; the old map is freed.
        let res = declare_frame(&mut g, &shape(512, true));
        prepare_graph(&gpu.device, &mut gpu.descriptors, &mut gpu.pool, &mut g, &res).unwrap();
        let new = g.image(res.shadow.unwrap()).array_view;
        assert_eq!(shadow_view(&gpu), new);
        gpu.pool.collect(&gpu.device, &mut gpu.descriptors, u64::MAX);
        assert!(!gpu.pool.has_view(old) && gpu.pool.has_view(new));
        assert_eq!(shadow_view(&gpu), new);
        // Freeing the bound map forgets it, so the next frame rewrites binding 0.
        gpu.pool.release_owner(&gpu.device, &mut gpu.descriptors, SHARED_OWNER);
        assert_eq!(shadow_view(&gpu), VK_NULL_HANDLE);
        let res = declare_frame(&mut g, &shape(512, true));
        prepare_graph(&gpu.device, &mut gpu.descriptors, &mut gpu.pool, &mut g, &res).unwrap();
        assert_eq!(shadow_view(&gpu), g.image(res.shadow.unwrap()).array_view);
        gpu.pool.release_owner(&gpu.device, &mut gpu.descriptors, 77);
        let messages = gpu.take_validation_messages();
        let errors: Vec<_> = messages.iter().filter(|m| m.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{errors:#?}");
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
