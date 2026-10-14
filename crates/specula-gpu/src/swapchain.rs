//! Window surface + swapchain (FIFO), frames in flight, present.

use crate::device::{Device, FENCE_TIMEOUT_NS};
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
use crate::{CpuTiming, FrameDesc, FramePath, FrameStats, GpuError};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::time::Instant;

const FRAMES_IN_FLIGHT: usize = 2;
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
            let frame = Frame { cmd, fence: VK_NULL_HANDLE, acquired: VK_NULL_HANDLE, upload: None, timer: None, submission: 0 };
            self.frames.push(frame);
            let f = self.frames.last_mut().expect("just pushed");
            f.fence = dev.create_fence(true)?;
            f.acquired = dev.create_semaphore()?;
            f.upload = Some(FrameUpload::new(dev)?);
            f.timer = PassTimer::new(dev)?;
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

        // Nothing may still use the old images, views or semaphores.
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
            image_usage: VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT,
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
        dev.wait_fence(f.fence)?;
        // The fence shows the GPU finished the frame that last used this slot (and every frame
        // submitted before it): its timestamps are ready and its upload buffer is free.
        *completed = (*completed).max(f.submission);
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
        let pipelines = pipelines.frame(dev, self.format.format, features, mode, env.sky.enabled, !frame.overlay.is_empty())?;
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
        };
        let res = declare_frame(&mut self.graph, &shape);
        prepare_graph(dev, descriptors, pool, &mut self.graph, &res)?;
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
        self.graph.bind_image(res.out, self.images[i], self.views[i]);

        dev.begin(f.cmd)?;
        if let Some(t) = f.timer.as_mut() {
            t.begin_frame(dev, f.cmd, frame_no);
        }
        let upload = f.upload.as_ref().expect("created in init");
        let ctx = FrameContext { dev, pipelines, descriptors, meshes, frame, scratch, upload };
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
