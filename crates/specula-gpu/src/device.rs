//! Logical device, queue, memory allocation and simple resource helpers.

use crate::instance::Instance;
use crate::vk::*;
use crate::{GpuError, HeapStats, MemoryKind, MemoryStats};
use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

const EXT_SWAPCHAIN: &CStr = c"VK_KHR_swapchain";
const EXT_MEMORY_BUDGET: &CStr = c"VK_EXT_memory_budget";

/// Upper bound for any synchronous GPU wait; a frame taking longer means something is wrong.
pub const FENCE_TIMEOUT_NS: u64 = 10_000_000_000;

pub struct Device {
    pub physical: VkPhysicalDevice,
    pub handle: VkDevice,
    pub fns: DeviceFns,
    /// Present when `VK_KHR_swapchain` was enabled.
    pub swapchain: Option<SwapchainFns>,
    pub queue: VkQueue,
    pub queue_family: u32,
    /// Meaningful bits of the queue's timestamps (0 = no timestamp support).
    pub timestamp_valid_bits: u32,
    /// Nanoseconds per timestamp tick.
    pub timestamp_period: f32,
    /// Largest sampler anisotropy enabled (1 = the device has no anisotropic filtering).
    pub max_anisotropy: f32,
    /// `VK_EXT_memory_budget` is enabled.
    pub memory_budget: bool,
    /// `fragmentStoresAndAtomics` is enabled (sight's ID buffer counts blended draws with it).
    pub fragment_atomics: bool,
    memory: VkPhysicalDeviceMemoryProperties,
    /// Bytes allocated per [`MemoryKind`], and the live allocation count.
    allocated: Cell<[u64; MemoryKind::ALL.len()]>,
    allocations: Cell<u32>,
}

impl Device {
    pub fn new(instance: &Instance, physical: VkPhysicalDevice, queue_family: u32) -> Result<Device, GpuError> {
        let available = instance.device_extensions(physical)?;
        let has = |name: &CStr| available.iter().any(|e| *e == name.to_string_lossy());
        let want_swapchain = instance.surface.is_some() && has(EXT_SWAPCHAIN);
        // Optional: heap budgets for frame statistics.
        let memory_budget = has(EXT_MEMORY_BUDGET);
        let mut ext_ptrs: Vec<*const c_char> = Vec::new();
        if want_swapchain {
            ext_ptrs.push(EXT_SWAPCHAIN.as_ptr());
        }
        if memory_budget {
            ext_ptrs.push(EXT_MEMORY_BUDGET.as_ptr());
        }

        let priority = 1.0f32;
        let queue_info = VkDeviceQueueCreateInfo {
            s_type: ST_DEVICE_QUEUE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            queue_family_index: queue_family,
            queue_count: 1,
            p_queue_priorities: &priority,
        };
        let mut v13 = VkPhysicalDeviceVulkan13Features::new();
        v13.dynamic_rendering = VK_TRUE;
        v13.synchronization2 = VK_TRUE;
        // `discard` compiles to OpDemoteToHelperInvocation for Vulkan 1.3 targets.
        v13.shader_demote_to_helper_invocation = VK_TRUE;
        // Per-frame data (camera, lights, time) is read through buffer device addresses.
        let mut v12 = VkPhysicalDeviceVulkan12Features::new(&mut v13 as *mut _ as *mut c_void);
        v12.flags[VkPhysicalDeviceVulkan12Features::BUFFER_DEVICE_ADDRESS] = VK_TRUE;
        // The bindless texture array (descriptor indexing; checked by `check_suitable`).
        for i in VkPhysicalDeviceVulkan12Features::BINDLESS {
            v12.flags[i] = VK_TRUE;
        }
        // Optional: anisotropic filtering for material textures (every desktop GPU has it) and
        // fragment shader buffer atomics (sight's ID buffer counts glass with them).
        let mut supported = VkPhysicalDeviceFeatures2 {
            s_type: ST_PHYSICAL_DEVICE_FEATURES_2,
            p_next: ptr::null_mut(),
            features: VkPhysicalDeviceFeatures { flags: [0; 55] },
        };
        // SAFETY: `supported` is a valid, unchained VkPhysicalDeviceFeatures2 for the call.
        unsafe { (instance.fns.get_physical_device_features2)(physical, &mut supported) };
        let anisotropy = supported.features.flags[VkPhysicalDeviceFeatures::SAMPLER_ANISOTROPY] != 0;
        let fragment_atomics = supported.features.flags[VkPhysicalDeviceFeatures::FRAGMENT_STORES_AND_ATOMICS] != 0;
        let mut features = VkPhysicalDeviceFeatures { flags: [0; 55] };
        features.flags[VkPhysicalDeviceFeatures::SAMPLER_ANISOTROPY] = anisotropy as VkBool32;
        features.flags[VkPhysicalDeviceFeatures::FRAGMENT_STORES_AND_ATOMICS] = fragment_atomics as VkBool32;
        let features2 = VkPhysicalDeviceFeatures2 {
            s_type: ST_PHYSICAL_DEVICE_FEATURES_2,
            p_next: &mut v12 as *mut _ as *mut c_void,
            features,
        };
        let info = VkDeviceCreateInfo {
            s_type: ST_DEVICE_CREATE_INFO,
            p_next: &features2 as *const _ as *const c_void,
            flags: 0,
            queue_create_info_count: 1,
            p_queue_create_infos: &queue_info,
            enabled_layer_count: 0,
            pp_enabled_layer_names: ptr::null(),
            enabled_extension_count: ext_ptrs.len() as u32,
            pp_enabled_extension_names: ext_ptrs.as_ptr(),
            p_enabled_features: ptr::null(),
        };
        let mut handle: VkDevice = ptr::null_mut();
        // SAFETY: all pointers in `info` refer to locals alive for the call; `physical` belongs to
        // `instance`.
        let r = unsafe { (instance.fns.create_device)(physical, &info, ptr::null(), &mut handle) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateDevice", r));
        }

        let get = |name: &CStr| {
            // SAFETY: `handle` is the live device just created; `name` is NUL-terminated.
            unsafe { (instance.fns.get_device_proc_addr)(handle, name.as_ptr()) }
        };
        let fns = match DeviceFns::load(get) {
            Ok(f) => f,
            Err(name) => {
                if let Some(f) = get(c"vkDestroyDevice") {
                    // SAFETY: `f` is vkDestroyDevice; the device has no child objects yet.
                    unsafe {
                        let destroy = std::mem::transmute::<
                            unsafe extern "system" fn(),
                            unsafe extern "system" fn(VkDevice, *const c_void),
                        >(f);
                        destroy(handle, ptr::null());
                    }
                }
                return Err(GpuError::new(format!("Vulkan device is missing command {name}")));
            }
        };
        let timestamp_valid_bits =
            instance.queue_families(physical).get(queue_family as usize).map_or(0, |f| f.timestamp_valid_bits);
        let mut device = Device {
            physical,
            handle,
            fns,
            swapchain: None,
            queue: ptr::null_mut(),
            queue_family,
            timestamp_valid_bits,
            timestamp_period: instance.properties(physical).limits.timestamp_period(),
            max_anisotropy: if anisotropy { instance.properties(physical).limits.max_sampler_anisotropy().max(1.0) } else { 1.0 },
            memory_budget,
            fragment_atomics,
            memory: instance.memory_properties(physical),
            allocated: Cell::new([0; MemoryKind::ALL.len()]),
            allocations: Cell::new(0),
        };
        if want_swapchain {
            device.swapchain = Some(
                SwapchainFns::load(get).map_err(|n| GpuError::new(format!("VK_KHR_swapchain enabled but {n} is missing")))?,
            );
        }
        // SAFETY: queue 0 of `queue_family` was requested at device creation.
        unsafe { (device.fns.get_device_queue)(handle, queue_family, 0, &mut device.queue) };
        Ok(device)
    }

    pub fn wait_idle(&self) {
        // SAFETY: live device. Errors (device lost) leave nothing further to wait for.
        unsafe { (self.fns.device_wait_idle)(self.handle) };
    }

    /// Memory type satisfying `type_bits` and `required`, preferring ones that also have `preferred`.
    pub fn memory_type(&self, type_bits: u32, required: VkFlags, preferred: VkFlags) -> Option<u32> {
        let types = &self.memory.memory_types[..self.memory.memory_type_count as usize];
        let fits = |i: usize, flags: VkFlags| type_bits & (1 << i) != 0 && types[i].property_flags & flags == flags;
        (0..types.len())
            .find(|&i| fits(i, required | preferred))
            .or_else(|| (0..types.len()).find(|&i| fits(i, required)))
            .map(|i| i as u32)
    }

    fn allocate(
        &self,
        req: &VkMemoryRequirements,
        required: VkFlags,
        preferred: VkFlags,
        device_address: bool,
        kind: MemoryKind,
        what: &str,
    ) -> Result<VkDeviceMemory, GpuError> {
        let type_index = self.memory_type(req.memory_type_bits, required, preferred).ok_or_else(|| {
            GpuError::new(format!("no memory type for {what} (type bits {:#x}, flags {required:#x})", req.memory_type_bits))
        })?;
        let flags = VkMemoryAllocateFlagsInfo {
            s_type: ST_MEMORY_ALLOCATE_FLAGS_INFO,
            p_next: ptr::null(),
            flags: VK_MEMORY_ALLOCATE_DEVICE_ADDRESS_BIT,
            device_mask: 0,
        };
        let info = VkMemoryAllocateInfo {
            s_type: ST_MEMORY_ALLOCATE_INFO,
            p_next: if device_address { &flags as *const _ as *const c_void } else { ptr::null() },
            allocation_size: req.size,
            memory_type_index: type_index,
        };
        let mut memory = VK_NULL_HANDLE;
        // SAFETY: valid device and allocate info; `memory` is a valid out-pointer.
        let r = unsafe { (self.fns.allocate_memory)(self.handle, &info, ptr::null(), &mut memory) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk(&format!("vkAllocateMemory ({what}, {} bytes)", req.size), r));
        }
        self.account(kind, req.size, true);
        Ok(memory)
    }

    /// Record one allocation of `bytes` for `kind` made (`added`) or freed.
    fn account(&self, kind: MemoryKind, bytes: u64, added: bool) {
        let mut all = self.allocated.get();
        let slot = &mut all[kind.index()];
        *slot = if added { slot.saturating_add(bytes) } else { slot.saturating_sub(bytes) };
        self.allocated.set(all);
        let n = self.allocations.get();
        self.allocations.set(if added { n + 1 } else { n.saturating_sub(1) });
    }

    /// Bytes our allocator holds, plus per-heap sizes and (with `VK_EXT_memory_budget`) budgets.
    pub fn memory_stats(&self, instance: &Instance) -> MemoryStats {
        let bytes = self.allocated.get();
        let by_kind: Vec<(MemoryKind, u64)> = MemoryKind::ALL.iter().map(|&k| (k, bytes[k.index()])).collect();
        let mut budget = VkPhysicalDeviceMemoryBudgetPropertiesEXT {
            s_type: ST_PHYSICAL_DEVICE_MEMORY_BUDGET_PROPERTIES_EXT,
            p_next: ptr::null_mut(),
            heap_budget: [0; VK_MAX_MEMORY_HEAPS],
            heap_usage: [0; VK_MAX_MEMORY_HEAPS],
        };
        let mut props = VkPhysicalDeviceMemoryProperties2 {
            s_type: ST_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2,
            p_next: if self.memory_budget { &mut budget as *mut _ as *mut c_void } else { ptr::null_mut() },
            memory_properties: self.memory,
        };
        // SAFETY: valid physical device; `props` (and `budget`, chained only when the extension is
        // enabled) are valid out-structs for the call.
        unsafe { (instance.fns.get_physical_device_memory_properties2)(self.physical, &mut props) };
        let m = &props.memory_properties;
        let heaps = (0..(m.memory_heap_count as usize).min(VK_MAX_MEMORY_HEAPS))
            .map(|i| HeapStats {
                index: i as u32,
                size: m.memory_heaps[i].size,
                device_local: m.memory_heaps[i].flags & VK_MEMORY_HEAP_DEVICE_LOCAL_BIT != 0,
                budget: self.memory_budget.then_some(budget.heap_budget[i]),
                usage: self.memory_budget.then_some(budget.heap_usage[i]),
            })
            .collect();
        MemoryStats { total_bytes: bytes.iter().sum(), by_kind, allocations: self.allocations.get(), heaps }
    }

    /// A buffer with its own memory. Host-visible buffers are persistently mapped.
    pub fn create_buffer(
        &self,
        size: u64,
        usage: VkFlags,
        required: VkFlags,
        preferred: VkFlags,
        kind: MemoryKind,
        what: &str,
    ) -> Result<Buffer, GpuError> {
        let info = VkBufferCreateInfo {
            s_type: ST_BUFFER_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            size,
            usage,
            sharing_mode: VK_SHARING_MODE_EXCLUSIVE,
            queue_family_index_count: 0,
            p_queue_family_indices: ptr::null(),
        };
        let mut handle = VK_NULL_HANDLE;
        // SAFETY: valid device and create info.
        let r = unsafe { (self.fns.create_buffer)(self.handle, &info, ptr::null(), &mut handle) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk(&format!("vkCreateBuffer ({what}, {size} bytes)"), r));
        }
        let mut buffer = Buffer { handle, memory: VK_NULL_HANDLE, mapped: ptr::null_mut(), bytes: 0, kind };
        let mut req = VkMemoryRequirements::default();
        // SAFETY: `handle` is a live buffer of this device.
        unsafe { (self.fns.get_buffer_memory_requirements)(self.handle, handle, &mut req) };
        let result = (|| {
            let device_address = usage & VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT != 0;
            buffer.memory = self.allocate(&req, required, preferred, device_address, kind, what)?;
            buffer.bytes = req.size;
            // SAFETY: memory was allocated for this buffer's requirements; offset 0 is aligned.
            let r = unsafe { (self.fns.bind_buffer_memory)(self.handle, handle, buffer.memory, 0) };
            if r != VK_SUCCESS {
                return Err(GpuError::vk("vkBindBufferMemory", r));
            }
            if required & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT != 0 {
                // SAFETY: host-visible memory, mapped once for the buffer's lifetime.
                let r = unsafe { (self.fns.map_memory)(self.handle, buffer.memory, 0, VK_WHOLE_SIZE, 0, &mut buffer.mapped) };
                if r != VK_SUCCESS {
                    return Err(GpuError::vk("vkMapMemory", r));
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(buffer),
            Err(e) => {
                self.destroy_buffer(&buffer);
                Err(e)
            }
        }
    }

    /// GPU address of a buffer created with `VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT`.
    pub fn buffer_address(&self, buffer: &Buffer) -> u64 {
        let info = VkBufferDeviceAddressInfo { s_type: ST_BUFFER_DEVICE_ADDRESS_INFO, p_next: ptr::null(), buffer: buffer.handle };
        // SAFETY: live buffer of this device, created with the device-address usage and memory flag.
        unsafe { (self.fns.get_buffer_device_address)(self.handle, &info) }
    }

    pub fn destroy_buffer(&self, buffer: &Buffer) {
        // SAFETY: the buffer and memory belong to this device, are no longer used by the GPU
        // (callers wait first), and are destroyed exactly once. Null handles are ignored by Vulkan.
        unsafe {
            if !buffer.mapped.is_null() {
                (self.fns.unmap_memory)(self.handle, buffer.memory);
            }
            (self.fns.destroy_buffer)(self.handle, buffer.handle, ptr::null());
            (self.fns.free_memory)(self.handle, buffer.memory, ptr::null());
        }
        if buffer.memory != VK_NULL_HANDLE {
            self.account(buffer.kind, buffer.bytes, false);
        }
    }

    /// A 2D optimal-tiling image in device-local memory, plus a full view.
    #[allow(clippy::too_many_arguments)]
    pub fn create_image(
        &self,
        format: VkFormat,
        width: u32,
        height: u32,
        usage: VkFlags,
        aspect: VkFlags,
        kind: MemoryKind,
        what: &str,
    ) -> Result<Image, GpuError> {
        self.create_image_with(format, width, height, 1, 1, usage, aspect, kind, what)
    }

    /// Like [`Device::create_image`], with `mips` levels and `layers` array layers. The view
    /// covers everything: `2D` for one layer, `2D_ARRAY` for more.
    #[allow(clippy::too_many_arguments)]
    pub fn create_image_with(
        &self,
        format: VkFormat,
        width: u32,
        height: u32,
        mips: u32,
        layers: u32,
        usage: VkFlags,
        aspect: VkFlags,
        kind: MemoryKind,
        what: &str,
    ) -> Result<Image, GpuError> {
        let info = VkImageCreateInfo {
            s_type: ST_IMAGE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            image_type: VK_IMAGE_TYPE_2D,
            format,
            extent: VkExtent3D { width, height, depth: 1 },
            mip_levels: mips,
            array_layers: layers,
            samples: VK_SAMPLE_COUNT_1_BIT,
            tiling: VK_IMAGE_TILING_OPTIMAL,
            usage,
            sharing_mode: VK_SHARING_MODE_EXCLUSIVE,
            queue_family_index_count: 0,
            p_queue_family_indices: ptr::null(),
            initial_layout: VK_IMAGE_LAYOUT_UNDEFINED,
        };
        let mut handle = VK_NULL_HANDLE;
        // SAFETY: valid device and create info.
        let r = unsafe { (self.fns.create_image)(self.handle, &info, ptr::null(), &mut handle) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk(&format!("vkCreateImage ({what}, {width}x{height})"), r));
        }
        let mut image = Image { handle, memory: VK_NULL_HANDLE, view: VK_NULL_HANDLE, bytes: 0, kind };
        let mut req = VkMemoryRequirements::default();
        // SAFETY: `handle` is a live image of this device.
        unsafe { (self.fns.get_image_memory_requirements)(self.handle, handle, &mut req) };
        let result = (|| {
            image.memory = self.allocate(&req, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0, false, kind, what)?;
            image.bytes = req.size;
            // SAFETY: memory matches the image's requirements; offset 0.
            let r = unsafe { (self.fns.bind_image_memory)(self.handle, handle, image.memory, 0) };
            if r != VK_SUCCESS {
                return Err(GpuError::vk("vkBindImageMemory", r));
            }
            let view_type = if layers > 1 { VK_IMAGE_VIEW_TYPE_2D_ARRAY } else { VK_IMAGE_VIEW_TYPE_2D };
            image.view = self.create_view_range(handle, format, aspect, view_type, (0, mips), (0, layers))?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(image),
            Err(e) => {
                self.destroy_image(&image);
                Err(e)
            }
        }
    }

    pub fn create_view(&self, image: VkImage, format: VkFormat, aspect: VkFlags) -> Result<VkImageView, GpuError> {
        self.create_view_range(image, format, aspect, VK_IMAGE_VIEW_TYPE_2D, (0, 1), (0, 1))
    }

    /// A view of `mips` = (first, count) levels and `layers` = (first, count) array layers.
    pub fn create_view_range(
        &self,
        image: VkImage,
        format: VkFormat,
        aspect: VkFlags,
        view_type: i32,
        mips: (u32, u32),
        layers: (u32, u32),
    ) -> Result<VkImageView, GpuError> {
        let info = VkImageViewCreateInfo {
            s_type: ST_IMAGE_VIEW_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            image,
            view_type,
            format,
            components: VkComponentMapping::default(), // identity swizzle
            subresource_range: VkImageSubresourceRange {
                aspect_mask: aspect,
                base_mip_level: mips.0,
                level_count: mips.1,
                base_array_layer: layers.0,
                layer_count: layers.1,
            },
        };
        let mut view = VK_NULL_HANDLE;
        // SAFETY: valid device, `image` is a live image of it.
        let r = unsafe { (self.fns.create_image_view)(self.handle, &info, ptr::null(), &mut view) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateImageView", r));
        }
        Ok(view)
    }

    pub fn destroy_image(&self, image: &Image) {
        // SAFETY: owned by this device, idle on the GPU, destroyed once; null handles are ignored.
        unsafe {
            (self.fns.destroy_image_view)(self.handle, image.view, ptr::null());
            (self.fns.destroy_image)(self.handle, image.handle, ptr::null());
            (self.fns.free_memory)(self.handle, image.memory, ptr::null());
        }
        if image.memory != VK_NULL_HANDLE {
            self.account(image.kind, image.bytes, false);
        }
    }

    pub fn create_fence(&self, signaled: bool) -> Result<VkFence, GpuError> {
        let info = VkFenceCreateInfo {
            s_type: ST_FENCE_CREATE_INFO,
            p_next: ptr::null(),
            flags: if signaled { VK_FENCE_CREATE_SIGNALED_BIT } else { 0 },
        };
        let mut fence = VK_NULL_HANDLE;
        // SAFETY: valid device and create info.
        let r = unsafe { (self.fns.create_fence)(self.handle, &info, ptr::null(), &mut fence) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateFence", r));
        }
        Ok(fence)
    }

    pub fn create_semaphore(&self) -> Result<VkSemaphore, GpuError> {
        let info = VkSemaphoreCreateInfo { s_type: ST_SEMAPHORE_CREATE_INFO, p_next: ptr::null(), flags: 0 };
        let mut semaphore = VK_NULL_HANDLE;
        // SAFETY: valid device and create info.
        let r = unsafe { (self.fns.create_semaphore)(self.handle, &info, ptr::null(), &mut semaphore) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateSemaphore", r));
        }
        Ok(semaphore)
    }

    pub fn wait_fence(&self, fence: VkFence) -> Result<(), GpuError> {
        // SAFETY: `fence` is a live fence of this device.
        let r = unsafe { (self.fns.wait_for_fences)(self.handle, 1, &fence, VK_TRUE, FENCE_TIMEOUT_NS) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkWaitForFences", r));
        }
        Ok(())
    }

    pub fn reset_fence(&self, fence: VkFence) -> Result<(), GpuError> {
        // SAFETY: `fence` is a live fence not in use by a pending submission (callers waited).
        let r = unsafe { (self.fns.reset_fences)(self.handle, 1, &fence) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkResetFences", r));
        }
        Ok(())
    }

    pub fn create_command_pool(&self) -> Result<VkCommandPool, GpuError> {
        let info = VkCommandPoolCreateInfo {
            s_type: ST_COMMAND_POOL_CREATE_INFO,
            p_next: ptr::null(),
            flags: VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT,
            queue_family_index: self.queue_family,
        };
        let mut pool = VK_NULL_HANDLE;
        // SAFETY: valid device and create info.
        let r = unsafe { (self.fns.create_command_pool)(self.handle, &info, ptr::null(), &mut pool) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateCommandPool", r));
        }
        Ok(pool)
    }

    pub fn allocate_command_buffer(&self, pool: VkCommandPool) -> Result<VkCommandBuffer, GpuError> {
        let info = VkCommandBufferAllocateInfo {
            s_type: ST_COMMAND_BUFFER_ALLOCATE_INFO,
            p_next: ptr::null(),
            command_pool: pool,
            level: VK_COMMAND_BUFFER_LEVEL_PRIMARY,
            command_buffer_count: 1,
        };
        let mut cmd: VkCommandBuffer = ptr::null_mut();
        // SAFETY: valid device, `pool` belongs to it; one output slot.
        let r = unsafe { (self.fns.allocate_command_buffers)(self.handle, &info, &mut cmd) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkAllocateCommandBuffers", r));
        }
        Ok(cmd)
    }

    /// Reset and begin a one-time-submit command buffer (its previous submission must be complete).
    pub fn begin(&self, cmd: VkCommandBuffer) -> Result<(), GpuError> {
        // SAFETY: `cmd` comes from a pool with RESET_COMMAND_BUFFER and is not pending.
        let r = unsafe { (self.fns.reset_command_buffer)(cmd, 0) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkResetCommandBuffer", r));
        }
        let info = VkCommandBufferBeginInfo {
            s_type: ST_COMMAND_BUFFER_BEGIN_INFO,
            p_next: ptr::null(),
            flags: VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
            p_inheritance_info: ptr::null(),
        };
        // SAFETY: `cmd` is in the initial state after the reset.
        let r = unsafe { (self.fns.begin_command_buffer)(cmd, &info) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkBeginCommandBuffer", r));
        }
        Ok(())
    }

    pub fn end(&self, cmd: VkCommandBuffer) -> Result<(), GpuError> {
        // SAFETY: `cmd` is recording.
        let r = unsafe { (self.fns.end_command_buffer)(cmd) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkEndCommandBuffer", r));
        }
        Ok(())
    }

    /// Submit one command buffer with optional wait/signal semaphores and fence.
    pub fn submit(
        &self,
        cmd: VkCommandBuffer,
        wait: Option<(VkSemaphore, VkFlags64)>,
        signal: Option<(VkSemaphore, VkFlags64)>,
        fence: VkFence,
    ) -> Result<(), GpuError> {
        let sem = |(semaphore, stage_mask): (VkSemaphore, VkFlags64)| VkSemaphoreSubmitInfo {
            s_type: ST_SEMAPHORE_SUBMIT_INFO,
            p_next: ptr::null(),
            semaphore,
            value: 0,
            stage_mask,
            device_index: 0,
        };
        let wait = wait.map(sem);
        let signal = signal.map(sem);
        let cmd_info = VkCommandBufferSubmitInfo {
            s_type: ST_COMMAND_BUFFER_SUBMIT_INFO,
            p_next: ptr::null(),
            command_buffer: cmd,
            device_mask: 0,
        };
        let submit = VkSubmitInfo2 {
            s_type: ST_SUBMIT_INFO_2,
            p_next: ptr::null(),
            flags: 0,
            wait_semaphore_info_count: wait.is_some() as u32,
            p_wait_semaphore_infos: wait.as_ref().map_or(ptr::null(), |w| w),
            command_buffer_info_count: 1,
            p_command_buffer_infos: &cmd_info,
            signal_semaphore_info_count: signal.is_some() as u32,
            p_signal_semaphore_infos: signal.as_ref().map_or(ptr::null(), |s| s),
        };
        // SAFETY: all referenced structs live until the call returns; `cmd` is executable.
        let r = unsafe { (self.fns.queue_submit2)(self.queue, 1, &submit, fence) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkQueueSubmit2", r));
        }
        Ok(())
    }

    pub fn barrier(&self, cmd: VkCommandBuffer, buffers: &[VkBufferMemoryBarrier2], images: &[VkImageMemoryBarrier2]) {
        let info = VkDependencyInfo {
            s_type: ST_DEPENDENCY_INFO,
            p_next: ptr::null(),
            dependency_flags: 0,
            memory_barrier_count: 0,
            p_memory_barriers: ptr::null(),
            buffer_memory_barrier_count: buffers.len() as u32,
            p_buffer_memory_barriers: buffers.as_ptr(),
            image_memory_barrier_count: images.len() as u32,
            p_image_memory_barriers: images.as_ptr(),
        };
        // SAFETY: `cmd` is recording; the slices outlive the call.
        unsafe { (self.fns.cmd_pipeline_barrier2)(cmd, &info) };
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: owners destroyed every child object before dropping the device; the device is
        // idle (the `Gpu` waits) and destroyed exactly once.
        unsafe { (self.fns.destroy_device)(self.handle, ptr::null()) };
    }
}

pub struct Buffer {
    pub handle: VkBuffer,
    pub memory: VkDeviceMemory,
    /// Non-null for host-visible buffers.
    pub mapped: *mut c_void,
    /// Size of the memory allocation (at least the requested size).
    pub bytes: u64,
    pub kind: MemoryKind,
}

pub struct Image {
    pub handle: VkImage,
    pub memory: VkDeviceMemory,
    pub view: VkImageView,
    pub bytes: u64,
    pub kind: MemoryKind,
}

pub fn full_range(aspect: VkFlags) -> VkImageSubresourceRange {
    VkImageSubresourceRange {
        aspect_mask: aspect,
        base_mip_level: 0,
        level_count: VK_REMAINING_MIP_LEVELS,
        base_array_layer: 0,
        layer_count: VK_REMAINING_ARRAY_LAYERS,
    }
}

/// Image layout transition / dependency on the whole image.
#[allow(clippy::too_many_arguments)]
pub fn image_barrier(
    image: VkImage,
    aspect: VkFlags,
    src_stage: VkFlags64,
    src_access: VkFlags64,
    dst_stage: VkFlags64,
    dst_access: VkFlags64,
    old_layout: VkImageLayout,
    new_layout: VkImageLayout,
) -> VkImageMemoryBarrier2 {
    VkImageMemoryBarrier2 {
        s_type: ST_IMAGE_MEMORY_BARRIER_2,
        p_next: ptr::null(),
        src_stage_mask: src_stage,
        src_access_mask: src_access,
        dst_stage_mask: dst_stage,
        dst_access_mask: dst_access,
        old_layout,
        new_layout,
        src_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
        dst_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
        image,
        subresource_range: full_range(aspect),
    }
}

pub fn buffer_barrier(
    buffer: VkBuffer,
    src_stage: VkFlags64,
    src_access: VkFlags64,
    dst_stage: VkFlags64,
    dst_access: VkFlags64,
) -> VkBufferMemoryBarrier2 {
    VkBufferMemoryBarrier2 {
        s_type: ST_BUFFER_MEMORY_BARRIER_2,
        p_next: ptr::null(),
        src_stage_mask: src_stage,
        src_access_mask: src_access,
        dst_stage_mask: dst_stage,
        dst_access_mask: dst_access,
        src_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
        dst_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
        buffer,
        offset: 0,
        size: VK_WHOLE_SIZE,
    }
}
