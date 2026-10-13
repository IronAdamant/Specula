//! Vulkan instance, validation/debug messenger, and physical-device queries.

use crate::loader::{self, Entry};
use crate::vk::*;
use crate::{AdapterInfo, AdapterKind, GpuError, Severity, Validation, ValidationMessage};
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::{Arc, Mutex};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";
const EXT_DEBUG_UTILS: &CStr = c"VK_EXT_debug_utils";
const EXT_VALIDATION_FEATURES: &CStr = c"VK_EXT_validation_features";
const EXT_SURFACE: &CStr = c"VK_KHR_surface";
const EXT_WIN32_SURFACE: &CStr = c"VK_KHR_win32_surface";

/// Oldest messages are dropped beyond this many undrained ones.
const MAX_MESSAGES: usize = 4096;

/// Where the debug messenger callback stores messages. Shared (`Arc`) so its address is stable
/// and it can outlive the instance: the callback can fire until `vkDestroyInstance` returns, and
/// `Gpu::close` reads teardown messages afterwards.
pub struct MessageSink {
    messages: Mutex<VecDeque<ValidationMessage>>,
}

impl MessageSink {
    fn push(&self, msg: ValidationMessage) {
        let mut q = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() >= MAX_MESSAGES {
            q.pop_front();
        }
        q.push_back(msg);
    }

    pub fn take(&self) -> Vec<ValidationMessage> {
        let mut q = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    }
}

/// # Safety
/// Called by the Vulkan loader/layers with `user_data` = the `MessageSink` registered at creation
/// and `data` valid for the duration of the call.
unsafe extern "system" fn debug_callback(
    severity: VkFlags,
    _types: VkFlags,
    data: *const VkDebugUtilsMessengerCallbackDataEXT,
    user_data: *mut c_void,
) -> VkBool32 {
    if data.is_null() || user_data.is_null() {
        return VK_FALSE;
    }
    // SAFETY: `user_data` is the `Arc<MessageSink>` held by the `Instance`, which outlives the
    // messenger and the instance; `data` is valid for this call per the Vulkan spec.
    let (sink, data) = unsafe { (&*(user_data as *const MessageSink), &*data) };
    // SAFETY: both strings are NUL-terminated (or null) and valid for this call.
    let (id_name, message) = unsafe { (opt_cstr(data.p_message_id_name), opt_cstr(data.p_message)) };
    let severity = if severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT != 0 {
        Severity::Error
    } else if severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT != 0 {
        Severity::Warning
    } else if severity & VK_DEBUG_UTILS_MESSAGE_SEVERITY_INFO_BIT_EXT != 0 {
        Severity::Info
    } else {
        Severity::Verbose
    };
    sink.push(ValidationMessage { severity, id_name, message });
    VK_FALSE
}

/// # Safety
/// `p` must be null or point to a NUL-terminated string.
unsafe fn opt_cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: guaranteed by the caller.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// Repeat a Vulkan "count, then fill" enumeration until it is complete.
pub fn enumerate<T: Copy>(
    what: &str,
    zero: T,
    mut call: impl FnMut(*mut u32, *mut T) -> VkResult,
) -> Result<Vec<T>, GpuError> {
    loop {
        let mut count = 0u32;
        let r = call(&mut count, ptr::null_mut());
        if r != VK_SUCCESS {
            return Err(GpuError::vk(what, r));
        }
        let mut items = vec![zero; count as usize];
        let r = call(&mut count, items.as_mut_ptr());
        match r {
            VK_SUCCESS => {
                items.truncate(count as usize);
                return Ok(items);
            }
            VK_INCOMPLETE => continue,
            _ => return Err(GpuError::vk(what, r)),
        }
    }
}

const ZERO_EXTENSION: VkExtensionProperties = VkExtensionProperties { extension_name: [0; 256], spec_version: 0 };

fn instance_extensions(entry: &Entry, layer: Option<&CStr>) -> Result<Vec<String>, GpuError> {
    let layer_ptr = layer.map_or(ptr::null(), |l| l.as_ptr());
    let props = enumerate("vkEnumerateInstanceExtensionProperties", ZERO_EXTENSION, |n, p| {
        // SAFETY: `n`/`p` are a valid count pointer and a null or `*n`-element buffer; `layer_ptr`
        // is null or a NUL-terminated layer name.
        unsafe { (entry.fns.enumerate_instance_extension_properties)(layer_ptr, n, p) }
    })?;
    Ok(props.iter().map(|e| fixed_cstr(&e.extension_name)).collect())
}

fn has_layer(entry: &Entry, name: &CStr) -> Result<bool, GpuError> {
    let zero = VkLayerProperties { layer_name: [0; 256], spec_version: 0, implementation_version: 0, description: [0; 256] };
    let layers = enumerate("vkEnumerateInstanceLayerProperties", zero, |n, p| {
        // SAFETY: valid count pointer and null or `*n`-element buffer.
        unsafe { (entry.fns.enumerate_instance_layer_properties)(n, p) }
    })?;
    let want = name.to_string_lossy();
    Ok(layers.iter().any(|l| fixed_cstr(&l.layer_name) == want))
}

fn contains(list: &[String], name: &CStr) -> bool {
    let name = name.to_string_lossy();
    list.iter().any(|e| *e == name)
}

pub struct Instance {
    pub handle: VkInstance,
    pub fns: InstanceFns,
    /// Present when the surface extensions were enabled.
    pub surface: Option<SurfaceFns>,
    messenger: Option<(DebugUtilsFns, VkDebugUtilsMessengerEXT)>,
    /// Declared after the handles: dropped after `Drop::drop` destroyed the instance.
    sink: Option<Arc<MessageSink>>,
}

impl Instance {
    /// Create a Vulkan 1.3 instance. `want_surface` enables `VK_KHR_surface` + the platform
    /// surface extension when the loader offers them (headless works without).
    pub fn new(validation: Validation, want_surface: bool) -> Result<Instance, GpuError> {
        let entry = loader::entry()?;
        if entry.instance_version < VK_API_VERSION_1_3 {
            let (a, b, c) = api_version_parts(entry.instance_version);
            return Err(GpuError::new(format!("Vulkan 1.3 required; the loader only supports {a}.{b}.{c}")));
        }

        let available = instance_extensions(entry, None)?;
        let mut extensions: Vec<&CStr> = Vec::new();
        let mut layers: Vec<&CStr> = Vec::new();

        let surface_available = contains(&available, EXT_SURFACE) && contains(&available, EXT_WIN32_SURFACE);
        if want_surface && surface_available {
            extensions.push(EXT_SURFACE);
            extensions.push(EXT_WIN32_SURFACE);
        }

        let mut validate = false;
        let mut sync_validation = false;
        if validation != Validation::Off {
            let layer_present = has_layer(entry, VALIDATION_LAYER)?;
            let layer_exts = if layer_present { instance_extensions(entry, Some(VALIDATION_LAYER))? } else { Vec::new() };
            let debug_utils = contains(&available, EXT_DEBUG_UTILS) || contains(&layer_exts, EXT_DEBUG_UTILS);
            if layer_present && debug_utils {
                validate = true;
                layers.push(VALIDATION_LAYER);
                extensions.push(EXT_DEBUG_UTILS);
                if contains(&layer_exts, EXT_VALIDATION_FEATURES) {
                    extensions.push(EXT_VALIDATION_FEATURES);
                    sync_validation = true;
                }
            } else if validation == Validation::On {
                let missing = if layer_present { "VK_EXT_debug_utils" } else { "VK_LAYER_KHRONOS_validation" };
                return Err(GpuError::new(format!(
                    "validation requested (Validation::On) but {missing} is not available; install the Vulkan SDK or use Validation::Auto"
                )));
            }
        }

        let sink = validate.then(|| Arc::new(MessageSink { messages: Mutex::new(VecDeque::new()) }));
        let sink_ptr = sink.as_ref().map_or(ptr::null_mut(), |s| Arc::as_ptr(s) as *mut c_void);

        let sync_features = [VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT];
        let validation_features = VkValidationFeaturesEXT {
            s_type: ST_VALIDATION_FEATURES_EXT,
            p_next: ptr::null(),
            enabled_validation_feature_count: sync_features.len() as u32,
            p_enabled_validation_features: sync_features.as_ptr(),
            disabled_validation_feature_count: 0,
            p_disabled_validation_features: ptr::null(),
        };
        let messenger_info = VkDebugUtilsMessengerCreateInfoEXT {
            s_type: ST_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT,
            p_next: if sync_validation { &validation_features as *const _ as *const c_void } else { ptr::null() },
            flags: 0,
            message_severity: VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT | VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT,
            message_type: VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT
                | VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT
                | VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT,
            pfn_user_callback: Some(debug_callback),
            p_user_data: sink_ptr,
        };

        let app_info = VkApplicationInfo {
            s_type: ST_APPLICATION_INFO,
            p_next: ptr::null(),
            p_application_name: c"Specula".as_ptr(),
            application_version: 0,
            p_engine_name: c"Specula".as_ptr(),
            engine_version: 0,
            api_version: VK_API_VERSION_1_3,
        };
        let layer_ptrs: Vec<*const c_char> = layers.iter().map(|l| l.as_ptr()).collect();
        let ext_ptrs: Vec<*const c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
        let create_info = VkInstanceCreateInfo {
            s_type: ST_INSTANCE_CREATE_INFO,
            // Chaining the messenger info captures messages from vkCreate/DestroyInstance too.
            p_next: if validate { &messenger_info as *const _ as *const c_void } else { ptr::null() },
            flags: 0,
            p_application_info: &app_info,
            enabled_layer_count: layer_ptrs.len() as u32,
            pp_enabled_layer_names: layer_ptrs.as_ptr(),
            enabled_extension_count: ext_ptrs.len() as u32,
            pp_enabled_extension_names: ext_ptrs.as_ptr(),
        };
        let mut handle: VkInstance = ptr::null_mut();
        // SAFETY: every pointer in `create_info` (and its pNext chain) refers to locals that live
        // until the call returns; `handle` is a valid out-pointer.
        let r = unsafe { (entry.fns.create_instance)(&create_info, ptr::null(), &mut handle) };
        if r != VK_SUCCESS {
            return Err(GpuError::vk("vkCreateInstance", r));
        }

        let get = |name: &CStr| entry.proc_addr(handle, name);
        let fns = match InstanceFns::load(get) {
            Ok(f) => f,
            Err(name) => {
                destroy_raw_instance(entry, handle);
                return Err(GpuError::new(format!("Vulkan instance is missing command {name}")));
            }
        };
        // From here on, `Drop` cleans up on error.
        let mut instance = Instance { handle, fns, surface: None, messenger: None, sink };

        if extensions.contains(&EXT_SURFACE) {
            instance.surface = Some(
                SurfaceFns::load(get).map_err(|n| GpuError::new(format!("surface extension enabled but {n} is missing")))?,
            );
        }
        if validate {
            let debug = DebugUtilsFns::load(get)
                .map_err(|n| GpuError::new(format!("VK_EXT_debug_utils enabled but {n} is missing")))?;
            let mut info = messenger_info;
            info.p_next = ptr::null();
            let mut messenger = VK_NULL_HANDLE;
            // SAFETY: `info` is fully initialized; its user data (the sink) outlives the messenger.
            let r = unsafe { (debug.create_debug_utils_messenger)(handle, &info, ptr::null(), &mut messenger) };
            if r != VK_SUCCESS {
                return Err(GpuError::vk("vkCreateDebugUtilsMessengerEXT", r));
            }
            instance.messenger = Some((debug, messenger));
        }
        Ok(instance)
    }

    pub fn validation_enabled(&self) -> bool {
        self.sink.is_some()
    }

    /// The message sink (None without validation); stays readable after the instance is gone.
    pub fn sink(&self) -> Option<Arc<MessageSink>> {
        self.sink.clone()
    }

    pub fn take_messages(&self) -> Vec<ValidationMessage> {
        self.sink.as_ref().map_or_else(Vec::new, |s| s.take())
    }

    pub fn physical_devices(&self) -> Result<Vec<VkPhysicalDevice>, GpuError> {
        enumerate("vkEnumeratePhysicalDevices", ptr::null_mut(), |n, p| {
            // SAFETY: live instance, valid count pointer, null or `*n`-element buffer.
            unsafe { (self.fns.enumerate_physical_devices)(self.handle, n, p) }
        })
    }

    pub fn properties(&self, pd: VkPhysicalDevice) -> VkPhysicalDeviceProperties {
        let mut props = VkPhysicalDeviceProperties {
            api_version: 0,
            driver_version: 0,
            vendor_id: 0,
            device_id: 0,
            device_type: 0,
            device_name: [0; VK_MAX_PHYSICAL_DEVICE_NAME_SIZE],
            pipeline_cache_uuid: [0; VK_UUID_SIZE],
            limits: VkPhysicalDeviceLimits { max_image_dimension_1d: 0, max_image_dimension_2d: 0, rest: [0; 62] },
            sparse_properties: VkPhysicalDeviceSparseProperties {
                residency_standard_2d_block_shape: 0,
                residency_standard_2d_multisample_block_shape: 0,
                residency_standard_3d_block_shape: 0,
                residency_aligned_mip_size: 0,
                residency_non_resident_strict: 0,
            },
        };
        // SAFETY: `pd` was enumerated from this instance; `props` is a valid out-struct.
        unsafe { (self.fns.get_physical_device_properties)(pd, &mut props) };
        props
    }

    pub fn adapter_info(&self, index: usize, pd: VkPhysicalDevice) -> AdapterInfo {
        let p = self.properties(pd);
        let kind = match p.device_type {
            VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU => AdapterKind::Discrete,
            VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU => AdapterKind::Integrated,
            VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU => AdapterKind::Virtual,
            VK_PHYSICAL_DEVICE_TYPE_CPU => AdapterKind::Cpu,
            _ => AdapterKind::Other,
        };
        AdapterInfo {
            index,
            name: fixed_cstr(&p.device_name),
            vendor_id: p.vendor_id,
            device_id: p.device_id,
            kind,
            api_version: api_version_parts(p.api_version),
            driver_version: p.driver_version,
        }
    }

    pub fn queue_families(&self, pd: VkPhysicalDevice) -> Vec<VkQueueFamilyProperties> {
        let mut count = 0u32;
        // SAFETY: valid count pointer, null buffer (count query).
        unsafe { (self.fns.get_physical_device_queue_family_properties)(pd, &mut count, ptr::null_mut()) };
        let mut families = vec![VkQueueFamilyProperties::default(); count as usize];
        // SAFETY: `families` holds `count` elements.
        unsafe { (self.fns.get_physical_device_queue_family_properties)(pd, &mut count, families.as_mut_ptr()) };
        families.truncate(count as usize);
        families
    }

    pub fn memory_properties(&self, pd: VkPhysicalDevice) -> VkPhysicalDeviceMemoryProperties {
        let mut props = VkPhysicalDeviceMemoryProperties {
            memory_type_count: 0,
            memory_types: [VkMemoryType::default(); VK_MAX_MEMORY_TYPES],
            memory_heap_count: 0,
            memory_heaps: [VkMemoryHeap::default(); VK_MAX_MEMORY_HEAPS],
        };
        // SAFETY: valid physical device and out-struct.
        unsafe { (self.fns.get_physical_device_memory_properties)(pd, &mut props) };
        props
    }

    pub fn format_features(&self, pd: VkPhysicalDevice, format: VkFormat) -> VkFormatProperties {
        let mut props = VkFormatProperties::default();
        // SAFETY: valid physical device and out-struct.
        unsafe { (self.fns.get_physical_device_format_properties)(pd, format, &mut props) };
        props
    }

    pub fn device_extensions(&self, pd: VkPhysicalDevice) -> Result<Vec<String>, GpuError> {
        let props = enumerate("vkEnumerateDeviceExtensionProperties", ZERO_EXTENSION, |n, p| {
            // SAFETY: valid physical device, count pointer and null or `*n`-element buffer.
            unsafe { (self.fns.enumerate_device_extension_properties)(pd, ptr::null(), n, p) }
        })?;
        Ok(props.iter().map(|e| fixed_cstr(&e.extension_name)).collect())
    }

    /// Why `pd` cannot run the M0 renderer, or the graphics queue family index to use.
    pub fn check_suitable(&self, pd: VkPhysicalDevice) -> Result<u32, String> {
        let props = self.properties(pd);
        if props.api_version < VK_API_VERSION_1_3 {
            let (a, b, c) = api_version_parts(props.api_version);
            return Err(format!("device supports Vulkan {a}.{b}.{c}; 1.3 is required"));
        }
        let family = self
            .queue_families(pd)
            .iter()
            .position(|f| f.queue_flags & VK_QUEUE_GRAPHICS_BIT != 0 && f.queue_count > 0)
            .ok_or("device has no graphics queue")?;

        let mut v13 = VkPhysicalDeviceVulkan13Features::new();
        let mut v12 = VkPhysicalDeviceVulkan12Features::new(&mut v13 as *mut _ as *mut c_void);
        let mut features2 = VkPhysicalDeviceFeatures2 {
            s_type: ST_PHYSICAL_DEVICE_FEATURES_2,
            p_next: &mut v12 as *mut _ as *mut c_void,
            features: VkPhysicalDeviceFeatures { flags: [0; 55] },
        };
        // SAFETY: `features2` chains `v12` then `v13`, all valid for the call.
        unsafe { (self.fns.get_physical_device_features2)(pd, &mut features2) };
        if v13.dynamic_rendering == 0 || v13.synchronization2 == 0 || v13.shader_demote_to_helper_invocation == 0 {
            return Err("device lacks dynamicRendering, synchronization2 or shaderDemoteToHelperInvocation".into());
        }
        if v12.flags[VkPhysicalDeviceVulkan12Features::BUFFER_DEVICE_ADDRESS] == 0 {
            return Err("device lacks bufferDeviceAddress".into());
        }
        if VkPhysicalDeviceVulkan12Features::BINDLESS.iter().any(|&i| v12.flags[i] == 0) {
            return Err("device lacks descriptor indexing (runtime arrays, partially bound, update-after-bind, \
                        non-uniform sampled-image indexing)"
                .into());
        }
        let sampled = |format| self.format_features(pd, format).optimal_tiling_features;
        let need = VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT | VK_FORMAT_FEATURE_SAMPLED_IMAGE_FILTER_LINEAR_BIT;
        if sampled(VK_FORMAT_R16G16B16A16_SFLOAT) & (need | VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BLEND_BIT)
            != need | VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BLEND_BIT
        {
            return Err("device cannot render to and filter R16G16B16A16_SFLOAT".into());
        }
        if sampled(VK_FORMAT_D16_UNORM) & (VK_FORMAT_FEATURE_DEPTH_STENCIL_ATTACHMENT_BIT | VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT)
            != VK_FORMAT_FEATURE_DEPTH_STENCIL_ATTACHMENT_BIT | VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT
        {
            return Err("device cannot render to and sample D16_UNORM (shadow maps)".into());
        }
        if self.format_features(pd, VK_FORMAT_D32_SFLOAT).optimal_tiling_features & VK_FORMAT_FEATURE_DEPTH_STENCIL_ATTACHMENT_BIT == 0 {
            return Err("device cannot use D32_SFLOAT as a depth attachment".into());
        }
        if self.format_features(pd, VK_FORMAT_R8G8B8A8_SRGB).optimal_tiling_features & VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BIT == 0 {
            return Err("device cannot render to R8G8B8A8_SRGB".into());
        }
        Ok(family as u32)
    }
}

fn destroy_raw_instance(entry: &Entry, handle: VkInstance) {
    if let Some(f) = entry.proc_addr(handle, c"vkDestroyInstance") {
        // SAFETY: `f` is `vkDestroyInstance` (prototype `void (VkInstance, const VkAllocationCallbacks*)`)
        // and `handle` is a live instance with no child objects.
        unsafe {
            let destroy = std::mem::transmute::<unsafe extern "system" fn(), unsafe extern "system" fn(VkInstance, *const c_void)>(f);
            destroy(handle, ptr::null());
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if let Some((debug, messenger)) = self.messenger.take() {
            // SAFETY: the messenger was created on this instance and is destroyed exactly once.
            unsafe { (debug.destroy_debug_utils_messenger)(self.handle, messenger, ptr::null()) };
        }
        // SAFETY: all child objects (devices, surfaces) are destroyed by their owners before the
        // instance is dropped; the instance is destroyed exactly once.
        unsafe { (self.fns.destroy_instance)(self.handle, ptr::null()) };
    }
}
