//! `Gpu` internals: adapter selection, meshes, offscreen targets, readback, window plumbing.

use crate::device::{buffer_barrier, image_barrier, Buffer, Device, Image};
use crate::instance::Instance;
use crate::render::{check_meshes, record_scene, Mesh, Pipelines, RenderTarget, DEPTH_FORMAT};
use crate::swapchain::Window;
use crate::vk::*;
use crate::{AdapterInfo, AdapterKind, AdapterPreference, FrameDesc, GpuError, GpuOptions, MeshId, TargetId, ValidationMessage, Vertex};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

/// Offscreen colour format (readback bytes are sRGB RGBA8).
pub const TARGET_FORMAT: VkFormat = VK_FORMAT_R8G8B8A8_SRGB;

struct Target {
    color: Image,
    depth: Image,
    readback: Buffer,
    extent: VkExtent2D,
    /// False until the first `render`; the colour image then sits in TRANSFER_SRC_OPTIMAL.
    rendered: bool,
}

pub struct GpuInner {
    adapter: AdapterInfo,
    max_image_dimension_2d: u32,
    pool: VkCommandPool,
    /// Command buffer + fence for synchronous work (uploads, offscreen renders, readback).
    sync_cmd: VkCommandBuffer,
    sync_fence: VkFence,
    pipelines: Pipelines,
    meshes: HashMap<u32, Mesh>,
    targets: HashMap<u32, Target>,
    next_id: u32,
    window: Option<Window>,
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

impl GpuInner {
    pub fn new(options: &GpuOptions) -> Result<GpuInner, GpuError> {
        let instance = Instance::new(options.validation, true)?;
        let (index, physical, family) = select_adapter(&instance, options.adapter)?;
        let adapter = instance.adapter_info(index, physical);
        let max_image_dimension_2d = instance.properties(physical).limits.max_image_dimension_2d;
        let device = Device::new(&instance, physical, family)?;
        let pipelines = Pipelines::new(&device)?;
        let mut gpu = GpuInner {
            adapter,
            max_image_dimension_2d,
            pool: VK_NULL_HANDLE,
            sync_cmd: ptr::null_mut(),
            sync_fence: VK_NULL_HANDLE,
            pipelines,
            meshes: HashMap::new(),
            targets: HashMap::new(),
            next_id: 1,
            window: None,
            device,
            instance,
        };
        // On error `gpu` drops and releases whatever was created.
        gpu.pool = gpu.device.create_command_pool()?;
        gpu.sync_cmd = gpu.device.allocate_command_buffer(gpu.pool)?;
        gpu.sync_fence = gpu.device.create_fence(false)?;
        gpu.pipelines.get(&gpu.device, TARGET_FORMAT)?;
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

    fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// Record with `record`, submit, and wait for completion.
    fn run_sync(&self, record: impl FnOnce(&Device, VkCommandBuffer)) -> Result<(), GpuError> {
        let dev = &self.device;
        dev.begin(self.sync_cmd)?;
        record(dev, self.sync_cmd);
        dev.end(self.sync_cmd)?;
        dev.submit(self.sync_cmd, None, None, self.sync_fence)?;
        dev.wait_fence(self.sync_fence)?;
        dev.reset_fence(self.sync_fence)
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
        self.meshes.insert(id, Mesh { buffer, index_offset: vb_bytes, index_count: indices.len() as u32 });
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
            "target colour",
        )?;
        let depth = match dev.create_image(
            DEPTH_FORMAT,
            width,
            height,
            VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT,
            VK_IMAGE_ASPECT_DEPTH_BIT,
            "target depth",
        ) {
            Ok(d) => d,
            Err(e) => {
                dev.destroy_image(&color);
                return Err(e);
            }
        };
        let readback = match dev.create_buffer(
            width as u64 * height as u64 * 4,
            VK_BUFFER_USAGE_TRANSFER_DST_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            VK_MEMORY_PROPERTY_HOST_CACHED_BIT,
            "target readback",
        ) {
            Ok(b) => b,
            Err(e) => {
                dev.destroy_image(&depth);
                dev.destroy_image(&color);
                return Err(e);
            }
        };
        let id = self.next_id();
        self.targets
            .insert(id, Target { color, depth, readback, extent: VkExtent2D { width, height }, rendered: false });
        Ok(TargetId(id))
    }

    pub fn destroy_target(&mut self, target: TargetId) {
        let Some(t) = self.targets.remove(&target.0) else { return };
        // Offscreen work is synchronous, so nothing on the GPU still uses the target.
        self.device.destroy_buffer(&t.readback);
        self.device.destroy_image(&t.depth);
        self.device.destroy_image(&t.color);
    }

    pub fn render(&mut self, target: TargetId, frame: &FrameDesc) -> Result<(), GpuError> {
        check_meshes(&self.meshes, frame)?;
        let t = self.targets.get(&target.0).ok_or_else(|| GpuError::new(format!("unknown target id {}", target.0)))?;
        let pipeline = self.pipelines.get(&self.device, TARGET_FORMAT)?;
        let rt = RenderTarget {
            color_view: t.color.view,
            depth_view: t.depth.view,
            depth_image: t.depth.handle,
            extent: t.extent,
        };
        let color = t.color.handle;
        let (layout, meshes) = (self.pipelines.layout, &self.meshes);
        self.run_sync(|dev, cmd| {
            // Previous contents are discarded; order after any earlier copy-out of this image.
            dev.barrier(
                cmd,
                &[],
                &[image_barrier(
                    color,
                    VK_IMAGE_ASPECT_COLOR_BIT,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_NONE,
                    VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
                    VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
                    VK_IMAGE_LAYOUT_UNDEFINED,
                    VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
                )],
            );
            record_scene(dev, cmd, pipeline, layout, meshes, &rt, frame);
            dev.barrier(
                cmd,
                &[],
                &[image_barrier(
                    color,
                    VK_IMAGE_ASPECT_COLOR_BIT,
                    VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
                    VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_READ_BIT,
                    VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
                    VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
                )],
            );
        })?;
        if let Some(t) = self.targets.get_mut(&target.0) {
            t.rendered = true;
        }
        Ok(())
    }

    pub fn read_rgba8(&mut self, target: TargetId) -> Result<Vec<u8>, GpuError> {
        let t = self.targets.get(&target.0).ok_or_else(|| GpuError::new(format!("unknown target id {}", target.0)))?;
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
            old.destroy(&self.instance, &self.device, self.pool);
        }
        let window = Window::new(&self.instance, &self.device, self.pool, hinstance, hwnd, width, height)?;
        self.pipelines.get(&self.device, window.format())?;
        self.window = Some(window);
        Ok(())
    }

    pub fn resize_window(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        let window = self.window.as_mut().ok_or_else(|| GpuError::new("resize_window: no window attached"))?;
        window.resize(&self.instance, &self.device, width, height)
    }

    pub fn present(&mut self, frame: &FrameDesc) -> Result<(), GpuError> {
        let window = self.window.as_mut().ok_or_else(|| GpuError::new("present: no window attached"))?;
        window.present(&self.instance, &self.device, &mut self.pipelines, &self.meshes, frame)
    }
}

impl Drop for GpuInner {
    fn drop(&mut self) {
        let dev = &self.device;
        dev.wait_idle();
        if let Some(mut w) = self.window.take() {
            w.destroy(&self.instance, dev, self.pool);
        }
        for (_, m) in self.meshes.drain() {
            dev.destroy_buffer(&m.buffer);
        }
        for (_, t) in self.targets.drain() {
            dev.destroy_buffer(&t.readback);
            dev.destroy_image(&t.depth);
            dev.destroy_image(&t.color);
        }
        self.pipelines.destroy(dev);
        // SAFETY: device idle; objects belong to `dev` and are destroyed once (null is ignored).
        unsafe {
            (dev.fns.destroy_fence)(dev.handle, self.sync_fence, ptr::null());
            (dev.fns.destroy_command_pool)(dev.handle, self.pool, ptr::null()); // frees sync_cmd
        }
        // `device`, then `instance` drop next (field order).
    }
}

const _: () = assert!(size_of::<Vertex>() == 24);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, Validation};

    /// Proves the debug messenger really delivers errors (so "zero errors" elsewhere means something).
    #[test]
    fn validation_errors_are_captured() {
        if !crate::is_available() {
            eprintln!("validation_errors_are_captured: SKIPPED — Vulkan is not available");
            return;
        }
        let options = GpuOptions { validation: Validation::On, adapter: AdapterPreference::HighPerformance };
        let mut gpu = match GpuInner::new(&options) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("validation_errors_are_captured: SKIPPED — {e}");
                return;
            }
        };
        assert!(gpu.validation_enabled());
        assert!(gpu.take_validation_messages().is_empty());
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
        let src = gpu.device.create_buffer(64, VK_BUFFER_USAGE_TRANSFER_SRC_BIT, host, 0, "src").unwrap();
        let dst = gpu.device.create_buffer(64, VK_BUFFER_USAGE_TRANSFER_DST_BIT, host, 0, "dst").unwrap();
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
