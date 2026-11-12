//! Window surface + swapchain (FIFO), frames in flight, present.

use crate::device::{Buffer, Device, FENCE_TIMEOUT_NS};
use crate::instance::{enumerate, Instance};
use crate::descriptors::Descriptors;
use crate::graph::{Final, Prior, TransientPool};
use crate::render::{
    check_frame, declare_frame, frame_features, prepare_frame, prepare_graph, record_frame, FrameContext, FrameGraph, FrameScratch,
    FrameShape, FrameUpload, Mesh, Pipelines,
};
use crate::overlay::Textures;
use crate::post::PostMode;
use crate::shadow::shadow_desc;
use crate::timing::PassTimer;
use crate::vk::*;
use crate::{CpuTiming, FrameDesc, FramePath, FrameStats, GpuError, MemoryKind};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::time::Instant;

const FRAMES_IN_FLIGHT: usize = 2;
/// Bytes of each frame's flash readback buffer: the blocks of a final image up to 4096 × 2304
/// (8 bytes per 8x8 block).
const FLASH_BYTES: u64 = (4096 / 8) * (2304 / 8) * 8;
/// Owner of the window graph's transient images.
pub const WINDOW_OWNER: u32 = u32::MAX;

/// Per-frame-in-flight objects. `acquired`, `upload` and `timer` are reused once `fence` shows
/// the frame finished.
struct Frame {
    cmd: VkCommandBuffer,
    fence: VkFence,
    acquired: VkSemaphore,
    /// This frame's frame data and draw records.
    upload: Option<FrameUpload>,
    /// `None` without timestamp support.
    timer: Option<PassTimer>,
    /// Transient-pool number of the frame last submitted from this slot (complete, with every
    /// submission before it, once `fence` signals).
    submission: u64,
    /// The flash grid's readback (host-visible), and what the frame last submitted from this slot
    /// wrote into it: `(frame number, width, height)`.
    flash: Option<Buffer>,
    flash_wrote: Option<(u64, (u32, u32), VkExtent2D)>,
}

fn ms(from: Instant, to: Instant) -> f64 {
    (to - from).as_secs_f64() * 1e3
}

pub struct Window {
    surface: VkSurfaceKHR,
    swapchain: VkSwapchainKHR,
    format: VkSurfaceFormatKHR,
    extent: VkExtent2D,
    images: Vec<VkImage>,
    views: Vec<VkImageView>,
    /// One per swapchain image: signalled by the frame's submit, waited on by present. Indexed by
    /// image so it is never re-signalled while a present may still be waiting on it.
    render_done: Vec<VkSemaphore>,
    frames: Vec<Frame>,
    frame_index: usize,
    /// Client size requested by the application.
    wanted: (u32, u32),
    /// Swapchain must be (re)built before the next present.
    stale: bool,
    /// Why the flash watch can't run on this swapchain (`None`: it can).
    flash_unsupported: Option<&'static str>,
    /// The bindless slot of each swapchain image (the flash watch samples the final image), and
    /// the swapchain they were written for.
    flash_slots: Vec<u32>,
    flash_slots_for: VkSwapchainKHR,
    /// The watch's two state images (half resolution) and their bindless slots: each frame
    /// reads one (the previous frame's) and writes the other; `flash_parity` is the one read.
    flash_state: Vec<(crate::device::Image, u32)>,
    flash_parity: usize,
    flash_extent: Option<VkExtent2D>,
    /// The window's frame graph; its depth buffer and post-chain images are the graph's
    /// transients (owner [`WINDOW_OWNER`]; the owner of the pool frees them before `destroy`).
    pub graph: FrameGraph,
}

fn surface_fns(inst: &Instance) -> Result<&SurfaceFns, GpuError> {
    inst.surface.as_ref().ok_or_else(|| {
        GpuError::new("window surfaces unavailable: the Vulkan loader lacks VK_KHR_surface/VK_KHR_win32_surface")
    })
}

fn swapchain_fns(dev: &Device) -> Result<&SwapchainFns, GpuError> {
    dev.swapchain.as_ref().ok_or_else(|| GpuError::new("the selected adapter does not support VK_KHR_swapchain"))
}

impl Window {
    pub fn new(
        inst: &Instance,
        dev: &Device,
        pool: VkCommandPool,
        hinstance: *mut c_void,
        hwnd: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<Window, GpuError> {
        let sfns = surface_fns(inst)?;
        swapchain_fns(dev)?;
        if hwnd.is_null() {
            return Err(GpuError::new("attach_window: hwnd is null"));
        }
        let info = VkWin32SurfaceCreateInfoKHR {
            s_type: ST_WIN32_SURFACE_CREATE_INFO_KHR,
            p_next: ptr::null(),
            flags: 0,
            hinstance,
            hwnd,
        };
        let mut surface = VK_NULL_HANDLE;
        // SAFETY: the caller passes a live HINSTANCE/HWND pair; `info` is valid for the call.
        let r = unsafe { (sfns.create_win32_surface)(inst.handle, &info, ptr::null(), &mut surface) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateWin32SurfaceKHR", r));
        }
        let mut window = Window {
            surface,
            swapchain: VK_NULL_HANDLE,
            format: VkSurfaceFormatKHR::default(),
            extent: VkExtent2D::default(),
            images: Vec::new(),
            views: Vec::new(),
            render_done: Vec::new(),
            frames: Vec::new(),
            frame_index: 0,
            wanted: (width, height),
            stale: true,
            flash_unsupported: Some("no swapchain yet"),
            flash_slots: Vec::new(),
            flash_slots_for: VK_NULL_HANDLE,
            flash_state: Vec::new(),
            flash_parity: 0,
            flash_extent: None,
            graph: FrameGraph::new(WINDOW_OWNER),
        };
        match window.init(inst, dev, pool) {
            Ok(()) => Ok(window),
            Err(e) => {
                window.destroy(inst, dev, pool);
                Err(e)
            }
        }
    }

    fn init(&mut self, inst: &Instance, dev: &Device, pool: VkCommandPool) -> Result<(), GpuError> {
        let sfns = surface_fns(inst)?;
        let mut supported = VK_FALSE;
        // SAFETY: valid physical device, queue family index and surface.
        let r = unsafe { (sfns.get_physical_device_surface_support)(dev.physical, dev.queue_family, self.surface, &mut supported) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkGetPhysicalDeviceSurfaceSupportKHR", r));
        }
        if supported == VK_FALSE {
            return Err(GpuError::new(format!(
                "queue family {} of the selected adapter cannot present to this window",
                dev.queue_family
            )));
        }
        let formats = enumerate("vkGetPhysicalDeviceSurfaceFormatsKHR", VkSurfaceFormatKHR::default(), |n, p| {
            // SAFETY: valid physical device and surface; count pointer and null or `*n` buffer.
            unsafe { (sfns.get_physical_device_surface_formats)(dev.physical, self.surface, n, p) }
        })?;
        self.format = choose_format(&formats).ok_or_else(|| GpuError::new("surface reports no formats"))?;

        for _ in 0..FRAMES_IN_FLIGHT {
            let cmd = dev.allocate_command_buffer(pool)?;
            let frame = Frame { cmd, fence: VK_NULL_HANDLE, acquired: VK_NULL_HANDLE, upload: None, timer: None, submission: 0, flash: None, flash_wrote: None };
            self.frames.push(frame);
            let f = self.frames.last_mut().expect("just pushed");
            f.fence = dev.create_fence(true)?;
            f.acquired = dev.create_semaphore()?;
            f.upload = Some(FrameUpload::new(dev)?);
            f.timer = PassTimer::new(dev)?;
            f.flash = Some(dev.create_buffer(
                FLASH_BYTES,
                VK_BUFFER_USAGE_TRANSFER_DST_BIT,
                VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                VK_MEMORY_PROPERTY_HOST_CACHED_BIT,
                MemoryKind::Staging,
                "flash readback",
            )?);
        }
        if self.wanted.0 > 0 && self.wanted.1 > 0 {
            self.rebuild(inst, dev)?;
        }
        Ok(())
    }

    pub fn format(&self) -> VkFormat {
        self.format.format
    }

    pub fn resize(&mut self, inst: &Instance, dev: &Device, width: u32, height: u32) -> Result<(), GpuError> {
        self.wanted = (width, height);
        self.stale = true;
        if width > 0 && height > 0 {
            self.rebuild(inst, dev)?;
        }
        Ok(())
    }

    /// (Re)create the swapchain for the current surface size. `Ok(false)` = nothing to present to
    /// (zero-sized / minimized window); stays stale and is retried on the next present.
    fn rebuild(&mut self, inst: &Instance, dev: &Device) -> Result<bool, GpuError> {
        let sfns = surface_fns(inst)?;
        let scfns = swapchain_fns(dev)?;
        let mut caps = VkSurfaceCapabilitiesKHR::default();
        // SAFETY: valid physical device and surface; `caps` is a valid out-struct.
        let r = unsafe { (sfns.get_physical_device_surface_capabilities)(dev.physical, self.surface, &mut caps) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkGetPhysicalDeviceSurfaceCapabilitiesKHR", r));
        }
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            VkExtent2D {
                width: self.wanted.0.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: self.wanted.1.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 || self.wanted.0 == 0 || self.wanted.1 == 0 {
            self.stale = true;
            return Ok(false);
        }

        // Nothing may still use the old swapchain's images, views or `render_done` semaphores
        // (frames in flight render into them; queued presents wait on the semaphores). The frame
        // graph's images (depth, post chain) need no wait here: a new size makes new ones and the
        // old ones are freed once their frames finished. Deferring this wait too would need the
        // old swapchain's views and semaphores kept until a present fence
        // (VK_EXT_swapchain_maintenance1) says they are free.
        dev.wait_idle();

        let mut min_images = caps.min_image_count + 1;
        if caps.max_image_count != 0 {
            min_images = min_images.min(caps.max_image_count);
        }
        let composite_alpha = if caps.supported_composite_alpha & VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR != 0 {
            VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR
        } else {
            // Lowest supported bit (spec: at least one is set).
            caps.supported_composite_alpha & caps.supported_composite_alpha.wrapping_neg()
        };
        let pre_transform = if caps.supported_transforms & VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR != 0 {
            VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR
        } else {
            caps.current_transform
        };
        // The flash watch reads the final image whole (bilinear taps averaging 2x2 pixels): the
        // swapchain images must be sampleable, with linear filtering.
        let wanted = VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT | VK_FORMAT_FEATURE_SAMPLED_IMAGE_FILTER_LINEAR_BIT;
        let flash = if caps.supported_usage_flags & VK_IMAGE_USAGE_SAMPLED_BIT == 0 {
            Some("the swapchain images can't be sampled (no SAMPLED usage)")
        } else if inst.format_features(dev.physical, self.format.format).optimal_tiling_features & wanted != wanted {
            Some("the swapchain format can't be sampled with linear filtering")
        } else {
            None
        };
        self.flash_unsupported = flash;
        let old = self.swapchain;
        let info = VkSwapchainCreateInfoKHR {
            s_type: ST_SWAPCHAIN_CREATE_INFO_KHR,
            p_next: ptr::null(),
            flags: 0,
            surface: self.surface,
            min_image_count: min_images,
            image_format: self.format.format,
            image_color_space: self.format.color_space,
            image_extent: extent,
            image_array_layers: 1,
            image_usage: VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | if flash.is_none() { VK_IMAGE_USAGE_SAMPLED_BIT } else { 0 },
            image_sharing_mode: VK_SHARING_MODE_EXCLUSIVE,
            queue_family_index_count: 0,
            p_queue_family_indices: ptr::null(),
            pre_transform,
            composite_alpha,
            present_mode: VK_PRESENT_MODE_FIFO_KHR,
            clipped: VK_TRUE,
            old_swapchain: old,
        };
        let mut swapchain = VK_NULL_HANDLE;
        // SAFETY: valid device, surface and (possibly null) old swapchain; `info` lives for the call.
        let r = unsafe { (scfns.create_swapchain)(dev.handle, &info, ptr::null(), &mut swapchain) };
        // The old swapchain is retired either way; release everything that belonged to it.
        self.destroy_swapchain_resources(dev);
        if old != VK_NULL_HANDLE {
            // SAFETY: device idle; `old` is retired and destroyed exactly once.
            unsafe { (scfns.destroy_swapchain)(dev.handle, old, ptr::null()) };
        }
        self.swapchain = VK_NULL_HANDLE;
        if r != VK_SUCCESS {
            self.stale = true;
            return Err(GpuError::vk("vkCreateSwapchainKHR", r));
        }
        self.swapchain = swapchain;
        self.extent = extent;

        self.images = enumerate("vkGetSwapchainImagesKHR", VK_NULL_HANDLE, |n, p| {
            // SAFETY: valid device and swapchain; count pointer and null or `*n` buffer.
            unsafe { (scfns.get_swapchain_images)(dev.handle, swapchain, n, p) }
        })?;
        for &image in &self.images {
            self.views.push(dev.create_view(image, self.format.format, VK_IMAGE_ASPECT_COLOR_BIT)?);
            self.render_done.push(dev.create_semaphore()?);
        }
        self.stale = false;
        Ok(true)
    }

    fn destroy_swapchain_resources(&mut self, dev: &Device) {
        // SAFETY: the device is idle (callers wait); each object is destroyed exactly once.
        unsafe {
            for view in self.views.drain(..) {
                (dev.fns.destroy_image_view)(dev.handle, view, ptr::null());
            }
            for semaphore in self.render_done.drain(..) {
                (dev.fns.destroy_semaphore)(dev.handle, semaphore, ptr::null());
            }
        }
        self.images.clear();
    }

    /// Wait until no frame in flight is using any resource.
    pub fn wait_frames(&self, dev: &Device) -> Result<(), GpuError> {
        // Null fences come from a failed recovery; nothing can be pending on them.
        for f in self.frames.iter().filter(|f| f.fence != VK_NULL_HANDLE) {
            dev.wait_fence(f.fence)?;
        }
        Ok(())
    }

    /// Render and present `frame` (number `frame_no`), updating the window's `stats` in place:
    /// GPU timings whenever a completed frame's are read, everything else when a frame is
    /// presented. `Ok(false)`: nothing was presented (minimized, or the swapchain was out of
    /// date). `scratch` is reusable CPU-side space.
    #[allow(clippy::too_many_arguments)]
    pub fn present(
        &mut self,
        inst: &Instance,
        dev: &Device,
        pipelines: &mut Pipelines,
        descriptors: &mut Descriptors,
        pool: &mut TransientPool,
        completed: &mut u64,
        meshes: &HashMap<u32, Mesh>,
        textures: &Textures,
        frame: &FrameDesc,
        frame_no: u64,
        scratch: &mut FrameScratch,
        stats: &mut FrameStats,
    ) -> Result<bool, GpuError> {
        check_frame(meshes, textures, frame)?;
        if self.wanted.0 == 0 || self.wanted.1 == 0 {
            return Ok(false); // minimized: nothing to present
        }
        if (self.stale || self.swapchain == VK_NULL_HANDLE) && !self.rebuild(inst, dev)? {
            return Ok(false);
        }
        let scfns = swapchain_fns(dev)?;
        let t0 = Instant::now();
        let f = &mut self.frames[self.frame_index];
        if f.fence == VK_NULL_HANDLE || f.acquired == VK_NULL_HANDLE {
            return Err(GpuError::new(
                "window frame resources were lost after a failed submit; attach the window again",
            ));
        }
        {
            // The GPU still drawing the frame that last used this slot (or work queued before it:
            // a big capture) holds the window here.
            let _z = specula_core::profile::zone("gpu.frame_fence");
            dev.wait_fence(f.fence)?;
        }
        // The fence shows the GPU finished the frame that last used this slot (and every frame
        // submitted before it): its timestamps are ready and its upload buffer is free.
        *completed = (*completed).max(f.submission);
        if let (Some((n, (w, h), extent)), Some(b)) = (f.flash_wrote.take(), f.flash.as_ref()) {
            let halfs = (w * h * 4) as usize;
            stats.flash.halfs.resize(halfs, 0);
            // SAFETY: the frame that wrote the buffer finished (its fence signalled); the buffer
            // is host-visible, coherent and mapped, holding `w × h × 8` bytes (at most FLASH_BYTES).
            unsafe { ptr::copy_nonoverlapping(b.mapped as *const u16, stats.flash.halfs.as_mut_ptr(), halfs) };
            (stats.flash.frame, stats.flash.width, stats.flash.height, stats.flash.extent) = (n, w, h, (extent.width, extent.height));
        }
        let fits = flash_bytes(self.extent) <= FLASH_BYTES;
        stats.flash.unsupported = self.flash_unsupported.or((!fits).then_some("the window is larger than the flash watch's readback (4096 x 2304)"));
        if let Some(t) = f.timer.as_mut() {
            match t.read_into(dev, &mut stats.gpu) {
                Ok(true) => stats.timestamp_error = None,
                Ok(false) => {}
                Err(r) => stats.timestamp_error = Some(r),
            }
        }
        let t1 = Instant::now();
        let upload = f.upload.as_mut().expect("created in init");
        prepare_frame(dev, meshes, textures, frame, self.extent, upload, scratch)?;
        let env = &frame.environment;
        let shadow = (env.shadow.enabled && scratch.shadow.data.count > 0).then(|| shadow_desc(&env.shadow));
        let features = frame_features(frame, scratch, shadow.is_some());
        let mode = PostMode::of(env).fit(self.extent);
        let pipelines = pipelines.frame(dev, self.format.format, features, mode, env.sky.enabled, !frame.overlay.is_empty(), (!frame.effects.particles.is_empty(), !frame.effects.decals.is_empty()), self.flash_unsupported.is_none() && fits)?;
        // Graph images replaced (resized, re-configured) or long unused are freed once no frame in
        // flight can use them.
        pool.collect(dev, descriptors, *completed);
        let shape = FrameShape {
            out_name: "swapchain",
            format: self.format.format,
            extent: self.extent,
            // The acquire semaphore is waited at COLOR_ATTACHMENT_OUTPUT, so the first transition is
            // ordered after the presentation engine released the image.
            prior: Prior::Discard { after: VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT },
            fin: Final::Present,
            mode,
            shadow,
            overlay: !frame.overlay.is_empty(),
            two_d: !frame.draws_2d.is_empty(),
            ssao: crate::ssao::ssao_extent(env, self.extent),
            particles: !frame.effects.particles.is_empty(),
            decals: !scratch.effects.decals.is_empty(),
            water: scratch.plan.batches.iter().any(|b| b.material == crate::Material::Water),
            blended: scratch.plan.batches.iter().any(|b| b.material.blended() && b.material != crate::Material::Water),
            flash: self.flash_unsupported.is_none() && fits,
            flash_reset: self.flash_extent != Some(self.extent) || self.flash_slots_for != self.swapchain,
        };
        let res = declare_frame(&mut self.graph, &shape);
        prepare_graph(dev, descriptors, pool, &mut self.graph, &res)?;
        if shape.flash && self.flash_slots_for != self.swapchain {
            // New swapchain images: their slots point at the new views (nothing in flight uses
            // the old ones: a rebuild waited for the device).
            for &s in &self.flash_slots {
                descriptors.free_slot(s);
            }
            self.flash_slots.clear();
            for &view in &self.views {
                let s = descriptors.alloc_slot()?;
                descriptors.write_texture(dev, s, view);
                self.flash_slots.push(s);
            }
            // The two state images, at the new size.
            for (img, s) in self.flash_state.drain(..) {
                descriptors.free_slot(s);
                dev.destroy_image(&img);
            }
            let (half, _) = crate::render::flash_sizes(self.extent);
            for _ in 0..2 {
                let img = dev.create_image_with(
                    crate::render::FLASH_STATE_FORMAT,
                    half.0,
                    half.1,
                    1,
                    1,
                    VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT,
                    VK_IMAGE_ASPECT_COLOR_BIT,
                    MemoryKind::PostTargets,
                    "flash state",
                )?;
                let s = match descriptors.alloc_slot() {
                    Ok(s) => s,
                    Err(e) => {
                        dev.destroy_image(&img);
                        return Err(e);
                    }
                };
                descriptors.write_texture(dev, s, img.view);
                self.flash_state.push((img, s));
            }
            self.flash_parity = 0;
            self.flash_slots_for = self.swapchain;
        }
        let descriptors = &*descriptors;
        let t2 = Instant::now();

        let mut index = 0u32;
        // SAFETY: valid device and swapchain; `f.acquired` is unsignaled (its previous wait
        // completed, as `f.fence` shows); `index` is a valid out-pointer.
        let r = unsafe {
            (scfns.acquire_next_image)(dev.handle, self.swapchain, FENCE_TIMEOUT_NS, f.acquired, VK_NULL_HANDLE, &mut index)
        };
        match r {
            VK_SUCCESS => {}
            VK_SUBOPTIMAL_KHR => self.stale = true, // still usable this frame; rebuild next time
            VK_ERROR_OUT_OF_DATE_KHR => {
                self.stale = true; // nothing acquired, semaphore untouched: skip this frame
                return Ok(false);
            }
            _ => return Err(GpuError::vk("vkAcquireNextImageKHR", r)),
        }
        let t3 = Instant::now();
        let i = index as usize;
        if shape.flash {
            self.graph.bind_image_slot(res.out, self.images[i], self.views[i], self.flash_slots[i]);
            self.graph.bind_buffer(res.flash_readback, f.flash.as_ref().expect("created in init").handle);
            // Read the previous frame's state, write the other image; next frame swaps.
            let (prev, next) = (&self.flash_state[self.flash_parity], &self.flash_state[1 - self.flash_parity]);
            if !res.flash_reset {
                self.graph.bind_image_slot(res.flash_prev, prev.0.handle, prev.0.view, prev.1);
            }
            self.graph.bind_image_slot(res.flash_next, next.0.handle, next.0.view, next.1);
            self.flash_parity = 1 - self.flash_parity;
            self.flash_extent = Some(self.extent);
        } else {
            self.graph.bind_image(res.out, self.images[i], self.views[i]);
        }

        {
            let _z = specula_core::profile::zone("gpu.frame_begin");
            dev.begin(f.cmd)?;
        }
        if let Some(t) = f.timer.as_mut() {
            t.begin_frame(dev, f.cmd, frame_no);
        }
        let upload = f.upload.as_ref().expect("created in init");
        let ctx = FrameContext { dev, pipelines, descriptors, meshes, frame, scratch, upload, textures, ids: None, id_occluders: &[], recorded: None };
        let draws = record_frame(&ctx, f.cmd, &mut self.graph, &res, f.timer.as_mut());
        let upload_bytes = upload.bytes();
        dev.end(f.cmd)?;
        let t4 = Instant::now();
        dev.reset_fence(f.fence)?;
        let submitted = dev.submit(
            f.cmd,
            Some((f.acquired, VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT)),
            Some((self.render_done[i], VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT)),
            f.fence,
        );
        if let Err(e) = submitted {
            self.recover_frame(dev, self.frame_index);
            return Err(e);
        }
        if let Some(t) = f.timer.as_mut() {
            t.submitted();
        }
        f.submission = pool.note_used(&self.graph);
        f.flash_wrote = shape.flash.then_some((frame_no, res.flash_blocks_size, self.extent));

        let present = VkPresentInfoKHR {
            s_type: ST_PRESENT_INFO_KHR,
            p_next: ptr::null(),
            wait_semaphore_count: 1,
            p_wait_semaphores: &self.render_done[i],
            swapchain_count: 1,
            p_swapchains: &self.swapchain,
            p_image_indices: &index,
            p_results: ptr::null_mut(),
        };
        // SAFETY: valid queue; all referenced handles/structs live for the call.
        let r = unsafe { (scfns.queue_present)(dev.queue, &present) };
        self.frame_index = (self.frame_index + 1) % FRAMES_IN_FLIGHT;
        let t5 = Instant::now();
        match r {
            VK_SUCCESS => {}
            VK_SUBOPTIMAL_KHR | VK_ERROR_OUT_OF_DATE_KHR => self.stale = true,
            _ => return Err(GpuError::vk("vkQueuePresentKHR", r)),
        }
        stats.frame = frame_no;
        stats.path = FramePath::Window;
        stats.width = self.extent.width;
        stats.height = self.extent.height;
        stats.cpu = CpuTiming { wait_ms: ms(t0, t1) + ms(t2, t3), record_ms: ms(t1, t2) + ms(t3, t4), submit_ms: ms(t4, t5) };
        stats.timestamps_supported = self.frames.iter().all(|f| f.timer.is_some());
        stats.draws = draws;
        stats.graph = self.graph.frame_stats();
        stats.draw_buffer_bytes = upload_bytes;
        Ok(true)
    }

    /// After a failed submit nothing was queued: the frame's fence (already reset) would never
    /// signal, its acquire semaphore stays signaled, and the acquired image is never presented.
    /// Replace the fence (signaled) and semaphore, and mark the swapchain stale so the next present
    /// rebuilds it, which reclaims the image. If replacements can't be created, the handles stay
    /// null and `present` reports that the window must be attached again.
    fn recover_frame(&mut self, dev: &Device, index: usize) {
        dev.wait_idle();
        let frame = &mut self.frames[index];
        // SAFETY: the device is idle; both objects belong to `dev` and are destroyed exactly once
        // (their handles are replaced right below).
        unsafe {
            (dev.fns.destroy_fence)(dev.handle, frame.fence, ptr::null());
            (dev.fns.destroy_semaphore)(dev.handle, frame.acquired, ptr::null());
        }
        frame.fence = dev.create_fence(true).unwrap_or(VK_NULL_HANDLE);
        frame.acquired = dev.create_semaphore().unwrap_or(VK_NULL_HANDLE);
        self.stale = true;
    }

    /// Frees the swapchain images' bindless slots (before `destroy`; the device must be idle).
    pub fn release_slots(&mut self, descriptors: &mut Descriptors) {
        for s in self.flash_slots.drain(..) {
            descriptors.free_slot(s);
        }
        for (_, s) in &self.flash_state {
            descriptors.free_slot(*s);
        }
        self.flash_slots_for = VK_NULL_HANDLE;
    }

    /// Destroy everything. The device must be idle.
    pub fn destroy(&mut self, inst: &Instance, dev: &Device, pool: VkCommandPool) {
        self.destroy_swapchain_resources(dev);
        // SAFETY: device idle; every object belongs to `dev`/`inst` and is destroyed once.
        unsafe {
            for mut f in self.frames.drain(..) {
                if let Some(u) = &f.upload {
                    u.destroy(dev);
                }
                if let Some(t) = f.timer.as_mut() {
                    t.destroy(dev);
                }
                if let Some(b) = &f.flash {
                    dev.destroy_buffer(b);
                }
                for (img, _) in self.flash_state.drain(..) {
                    dev.destroy_image(&img);
                }
                (dev.fns.destroy_fence)(dev.handle, f.fence, ptr::null());
                (dev.fns.destroy_semaphore)(dev.handle, f.acquired, ptr::null());
                (dev.fns.free_command_buffers)(dev.handle, pool, 1, &f.cmd);
            }
            if let (Some(scfns), true) = (dev.swapchain.as_ref(), self.swapchain != VK_NULL_HANDLE) {
                (scfns.destroy_swapchain)(dev.handle, self.swapchain, ptr::null());
            }
            if let Some(sfns) = inst.surface.as_ref() {
                (sfns.destroy_surface)(inst.handle, self.surface, ptr::null());
            }
        }
        self.swapchain = VK_NULL_HANDLE;
        self.surface = VK_NULL_HANDLE;
    }
}

/// Bytes of the flash watch's blocks of a final image of `extent`.
fn flash_bytes(extent: VkExtent2D) -> u64 {
    let (_, (w, h)) = crate::render::flash_sizes(extent);
    u64::from(w) * u64::from(h) * 8
}

/// Prefer `B8G8R8A8_SRGB`, then `R8G8B8A8_SRGB` (sRGB non-linear colour space), else the first.
fn choose_format(formats: &[VkSurfaceFormatKHR]) -> Option<VkSurfaceFormatKHR> {
    [VK_FORMAT_B8G8R8A8_SRGB, VK_FORMAT_R8G8B8A8_SRGB]
        .iter()
        .find_map(|&want| {
            formats.iter().copied().find(|f| f.format == want && f.color_space == VK_COLOR_SPACE_SRGB_NONLINEAR_KHR)
        })
        .or_else(|| formats.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_choice_prefers_bgra_srgb() {
        let f = |format| VkSurfaceFormatKHR { format, color_space: VK_COLOR_SPACE_SRGB_NONLINEAR_KHR };
        assert_eq!(choose_format(&[f(44), f(VK_FORMAT_R8G8B8A8_SRGB), f(VK_FORMAT_B8G8R8A8_SRGB)]), Some(f(VK_FORMAT_B8G8R8A8_SRGB)));
        assert_eq!(choose_format(&[f(44), f(VK_FORMAT_R8G8B8A8_SRGB)]), Some(f(VK_FORMAT_R8G8B8A8_SRGB)));
        assert_eq!(choose_format(&[f(44)]), Some(f(44)));
        assert_eq!(choose_format(&[]), None);
    }
}
