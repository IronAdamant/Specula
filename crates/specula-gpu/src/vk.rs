//! Hand-written Vulkan FFI: only the types, constants and entry points Specula uses.
//!
//! Transcribed from `vulkan_core.h` / `vulkan_win32.h` (SDK 1.4.363). Field names are snake_case
//! versions of the C names; layouts are `#[repr(C)]` and checked against the headers' 64-bit
//! `sizeof`/`alignof` in the tests at the bottom. Non-dispatchable handles are `u64` (0 = null),
//! dispatchable handles are opaque pointers.

use std::ffi::{c_char, c_void, CStr};

// ---------------------------------------------------------------------------------------------
// Scalars and handles
// ---------------------------------------------------------------------------------------------

pub type VkResult = i32;
pub type VkBool32 = u32;
pub type VkFlags = u32;
pub type VkFlags64 = u64;
pub type VkDeviceSize = u64;
pub type VkStructureType = i32;
pub type VkFormat = i32;
pub type VkImageLayout = i32;

pub type VkInstance = *mut c_void;
pub type VkPhysicalDevice = *mut c_void;
pub type VkDevice = *mut c_void;
pub type VkQueue = *mut c_void;
pub type VkCommandBuffer = *mut c_void;

pub type VkBuffer = u64;
pub type VkImage = u64;
pub type VkImageView = u64;
pub type VkDeviceMemory = u64;
pub type VkShaderModule = u64;
pub type VkPipelineLayout = u64;
pub type VkPipeline = u64;
pub type VkCommandPool = u64;
pub type VkFence = u64;
pub type VkSemaphore = u64;
pub type VkSurfaceKHR = u64;
pub type VkSwapchainKHR = u64;
pub type VkDebugUtilsMessengerEXT = u64;
pub type VkQueryPool = u64;
pub type VkPipelineCache = u64;
pub type VkSampler = u64;
pub type VkDescriptorSetLayout = u64;
pub type VkDescriptorPool = u64;
pub type VkDescriptorSet = u64;

pub const VK_NULL_HANDLE: u64 = 0;
pub const VK_TRUE: VkBool32 = 1;
pub const VK_FALSE: VkBool32 = 0;
pub const VK_QUEUE_FAMILY_IGNORED: u32 = !0;
pub const VK_REMAINING_MIP_LEVELS: u32 = !0;
pub const VK_REMAINING_ARRAY_LAYERS: u32 = !0;
pub const VK_WHOLE_SIZE: u64 = !0;

pub const VK_MAX_PHYSICAL_DEVICE_NAME_SIZE: usize = 256;
pub const VK_MAX_EXTENSION_NAME_SIZE: usize = 256;
pub const VK_MAX_DESCRIPTION_SIZE: usize = 256;
pub const VK_UUID_SIZE: usize = 16;
pub const VK_MAX_MEMORY_TYPES: usize = 32;
pub const VK_MAX_MEMORY_HEAPS: usize = 16;

/// `VK_MAKE_API_VERSION(0, major, minor, patch)`.
pub const fn make_api_version(major: u32, minor: u32, patch: u32) -> u32 {
    (major << 22) | (minor << 12) | patch
}

pub const VK_API_VERSION_1_3: u32 = make_api_version(1, 3, 0);

/// `(major, minor, patch)` of a packed API version (variant bits ignored).
pub const fn api_version_parts(v: u32) -> (u32, u32, u32) {
    ((v >> 22) & 0x7f, (v >> 12) & 0x3ff, v & 0xfff)
}

// VkResult
pub const VK_SUCCESS: VkResult = 0;
pub const VK_NOT_READY: VkResult = 1;
pub const VK_TIMEOUT: VkResult = 2;
pub const VK_INCOMPLETE: VkResult = 5;
pub const VK_SUBOPTIMAL_KHR: VkResult = 1_000_001_003;
pub const VK_ERROR_OUT_OF_HOST_MEMORY: VkResult = -1;
pub const VK_ERROR_OUT_OF_DEVICE_MEMORY: VkResult = -2;
pub const VK_ERROR_INITIALIZATION_FAILED: VkResult = -3;
pub const VK_ERROR_DEVICE_LOST: VkResult = -4;
pub const VK_ERROR_MEMORY_MAP_FAILED: VkResult = -5;
pub const VK_ERROR_LAYER_NOT_PRESENT: VkResult = -6;
pub const VK_ERROR_EXTENSION_NOT_PRESENT: VkResult = -7;
pub const VK_ERROR_FEATURE_NOT_PRESENT: VkResult = -8;
pub const VK_ERROR_INCOMPATIBLE_DRIVER: VkResult = -9;
pub const VK_ERROR_TOO_MANY_OBJECTS: VkResult = -10;
pub const VK_ERROR_FORMAT_NOT_SUPPORTED: VkResult = -11;
pub const VK_ERROR_UNKNOWN: VkResult = -13;
pub const VK_ERROR_SURFACE_LOST_KHR: VkResult = -1_000_000_000;
pub const VK_ERROR_NATIVE_WINDOW_IN_USE_KHR: VkResult = -1_000_000_001;
pub const VK_ERROR_OUT_OF_DATE_KHR: VkResult = -1_000_001_004;

/// Symbolic name of a `VkResult`, for error messages.
pub fn result_name(r: VkResult) -> &'static str {
    match r {
        VK_SUCCESS => "VK_SUCCESS",
        VK_NOT_READY => "VK_NOT_READY",
        VK_TIMEOUT => "VK_TIMEOUT",
        VK_INCOMPLETE => "VK_INCOMPLETE",
        VK_SUBOPTIMAL_KHR => "VK_SUBOPTIMAL_KHR",
        VK_ERROR_OUT_OF_HOST_MEMORY => "VK_ERROR_OUT_OF_HOST_MEMORY",
        VK_ERROR_OUT_OF_DEVICE_MEMORY => "VK_ERROR_OUT_OF_DEVICE_MEMORY",
        VK_ERROR_INITIALIZATION_FAILED => "VK_ERROR_INITIALIZATION_FAILED",
        VK_ERROR_DEVICE_LOST => "VK_ERROR_DEVICE_LOST",
        VK_ERROR_MEMORY_MAP_FAILED => "VK_ERROR_MEMORY_MAP_FAILED",
        VK_ERROR_LAYER_NOT_PRESENT => "VK_ERROR_LAYER_NOT_PRESENT",
        VK_ERROR_EXTENSION_NOT_PRESENT => "VK_ERROR_EXTENSION_NOT_PRESENT",
        VK_ERROR_FEATURE_NOT_PRESENT => "VK_ERROR_FEATURE_NOT_PRESENT",
        VK_ERROR_INCOMPATIBLE_DRIVER => "VK_ERROR_INCOMPATIBLE_DRIVER",
        VK_ERROR_TOO_MANY_OBJECTS => "VK_ERROR_TOO_MANY_OBJECTS",
        VK_ERROR_FORMAT_NOT_SUPPORTED => "VK_ERROR_FORMAT_NOT_SUPPORTED",
        VK_ERROR_UNKNOWN => "VK_ERROR_UNKNOWN",
        VK_ERROR_SURFACE_LOST_KHR => "VK_ERROR_SURFACE_LOST_KHR",
        VK_ERROR_NATIVE_WINDOW_IN_USE_KHR => "VK_ERROR_NATIVE_WINDOW_IN_USE_KHR",
        VK_ERROR_OUT_OF_DATE_KHR => "VK_ERROR_OUT_OF_DATE_KHR",
        _ => "VkResult(?)",
    }
}

// VkStructureType
pub const ST_APPLICATION_INFO: VkStructureType = 0;
pub const ST_INSTANCE_CREATE_INFO: VkStructureType = 1;
pub const ST_DEVICE_QUEUE_CREATE_INFO: VkStructureType = 2;
pub const ST_DEVICE_CREATE_INFO: VkStructureType = 3;
pub const ST_MEMORY_ALLOCATE_INFO: VkStructureType = 5;
pub const ST_FENCE_CREATE_INFO: VkStructureType = 8;
pub const ST_SEMAPHORE_CREATE_INFO: VkStructureType = 9;
pub const ST_QUERY_POOL_CREATE_INFO: VkStructureType = 11;
pub const ST_BUFFER_CREATE_INFO: VkStructureType = 12;
pub const ST_IMAGE_CREATE_INFO: VkStructureType = 14;
pub const ST_IMAGE_VIEW_CREATE_INFO: VkStructureType = 15;
pub const ST_SHADER_MODULE_CREATE_INFO: VkStructureType = 16;
pub const ST_PIPELINE_CACHE_CREATE_INFO: VkStructureType = 17;
pub const ST_PIPELINE_SHADER_STAGE_CREATE_INFO: VkStructureType = 18;
pub const ST_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO: VkStructureType = 19;
pub const ST_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO: VkStructureType = 20;
pub const ST_PIPELINE_VIEWPORT_STATE_CREATE_INFO: VkStructureType = 22;
pub const ST_PIPELINE_RASTERIZATION_STATE_CREATE_INFO: VkStructureType = 23;
pub const ST_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO: VkStructureType = 24;
pub const ST_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO: VkStructureType = 25;
pub const ST_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO: VkStructureType = 26;
pub const ST_PIPELINE_DYNAMIC_STATE_CREATE_INFO: VkStructureType = 27;
pub const ST_GRAPHICS_PIPELINE_CREATE_INFO: VkStructureType = 28;
pub const ST_PIPELINE_LAYOUT_CREATE_INFO: VkStructureType = 30;
pub const ST_COMMAND_POOL_CREATE_INFO: VkStructureType = 39;
pub const ST_COMMAND_BUFFER_ALLOCATE_INFO: VkStructureType = 40;
pub const ST_COMMAND_BUFFER_BEGIN_INFO: VkStructureType = 42;
pub const ST_SAMPLER_CREATE_INFO: VkStructureType = 31;
pub const ST_DESCRIPTOR_SET_LAYOUT_CREATE_INFO: VkStructureType = 32;
pub const ST_DESCRIPTOR_POOL_CREATE_INFO: VkStructureType = 33;
pub const ST_DESCRIPTOR_SET_ALLOCATE_INFO: VkStructureType = 34;
pub const ST_WRITE_DESCRIPTOR_SET: VkStructureType = 35;
pub const ST_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES: VkStructureType = 51;
pub const ST_DESCRIPTOR_SET_LAYOUT_BINDING_FLAGS_CREATE_INFO: VkStructureType = 1_000_161_000;
pub const ST_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES: VkStructureType = 53;
pub const ST_MEMORY_ALLOCATE_FLAGS_INFO: VkStructureType = 1_000_060_000;
pub const ST_BUFFER_DEVICE_ADDRESS_INFO: VkStructureType = 1_000_244_001;
pub const ST_PHYSICAL_DEVICE_FEATURES_2: VkStructureType = 1_000_059_000;
pub const ST_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2: VkStructureType = 1_000_059_006;
pub const ST_PHYSICAL_DEVICE_MEMORY_BUDGET_PROPERTIES_EXT: VkStructureType = 1_000_237_000;
pub const ST_BUFFER_MEMORY_BARRIER_2: VkStructureType = 1_000_314_001;
pub const ST_IMAGE_MEMORY_BARRIER_2: VkStructureType = 1_000_314_002;
pub const ST_DEPENDENCY_INFO: VkStructureType = 1_000_314_003;
pub const ST_SUBMIT_INFO_2: VkStructureType = 1_000_314_004;
pub const ST_SEMAPHORE_SUBMIT_INFO: VkStructureType = 1_000_314_005;
pub const ST_COMMAND_BUFFER_SUBMIT_INFO: VkStructureType = 1_000_314_006;
pub const ST_RENDERING_INFO: VkStructureType = 1_000_044_000;
pub const ST_RENDERING_ATTACHMENT_INFO: VkStructureType = 1_000_044_001;
pub const ST_PIPELINE_RENDERING_CREATE_INFO: VkStructureType = 1_000_044_002;
pub const ST_SWAPCHAIN_CREATE_INFO_KHR: VkStructureType = 1_000_001_000;
pub const ST_PRESENT_INFO_KHR: VkStructureType = 1_000_001_001;
pub const ST_WIN32_SURFACE_CREATE_INFO_KHR: VkStructureType = 1_000_009_000;
pub const ST_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT: VkStructureType = 1_000_128_004;
pub const ST_VALIDATION_FEATURES_EXT: VkStructureType = 1_000_247_000;

// VkFormat
pub const VK_FORMAT_R8_UNORM: VkFormat = 9;
pub const VK_FORMAT_R8G8B8A8_UNORM: VkFormat = 37;
pub const VK_FORMAT_R8G8B8A8_SRGB: VkFormat = 43;
pub const VK_FORMAT_B8G8R8A8_SRGB: VkFormat = 50;
pub const VK_FORMAT_R32G32B32_SFLOAT: VkFormat = 106;
pub const VK_FORMAT_R16G16B16A16_SFLOAT: VkFormat = 97;
pub const VK_FORMAT_D16_UNORM: VkFormat = 124;
pub const VK_FORMAT_D32_SFLOAT: VkFormat = 126;

// VkImageLayout
pub const VK_IMAGE_LAYOUT_UNDEFINED: VkImageLayout = 0;
pub const VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL: VkImageLayout = 2;
pub const VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL: VkImageLayout = 5;
pub const VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL: VkImageLayout = 6;
pub const VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL: VkImageLayout = 7;
pub const VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL: VkImageLayout = 1_000_241_000;
pub const VK_IMAGE_LAYOUT_PRESENT_SRC_KHR: VkImageLayout = 1_000_001_002;

// VkPhysicalDeviceType
pub const VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU: i32 = 1;
pub const VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU: i32 = 2;
pub const VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU: i32 = 3;
pub const VK_PHYSICAL_DEVICE_TYPE_CPU: i32 = 4;

// Misc enums
pub const VK_IMAGE_TYPE_2D: i32 = 1;
pub const VK_IMAGE_VIEW_TYPE_2D: i32 = 1;
pub const VK_IMAGE_VIEW_TYPE_2D_ARRAY: i32 = 5;
pub const VK_IMAGE_TILING_OPTIMAL: i32 = 0;
pub const VK_SHARING_MODE_EXCLUSIVE: i32 = 0;
pub const VK_COMMAND_BUFFER_LEVEL_PRIMARY: i32 = 0;
pub const VK_INDEX_TYPE_UINT32: i32 = 1;
pub const VK_COMPARE_OP_LESS: i32 = 1;
pub const VK_COMPARE_OP_EQUAL: i32 = 2;
pub const VK_COMPARE_OP_LESS_OR_EQUAL: i32 = 3;
pub const VK_COMPARE_OP_GREATER: i32 = 4;
pub const VK_BLEND_FACTOR_ZERO: i32 = 0;
pub const VK_BLEND_FACTOR_ONE: i32 = 1;
pub const VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA: i32 = 7;
pub const VK_BLEND_OP_ADD: i32 = 0;
pub const VK_PIPELINE_BIND_POINT_GRAPHICS: i32 = 0;
pub const VK_DYNAMIC_STATE_VIEWPORT: i32 = 0;
pub const VK_DYNAMIC_STATE_SCISSOR: i32 = 1;
pub const VK_DYNAMIC_STATE_DEPTH_BIAS: i32 = 3;
pub const VK_DYNAMIC_STATE_DEPTH_COMPARE_OP: i32 = 1_000_267_008;
pub const VK_FILTER_LINEAR: i32 = 1;
pub const VK_SAMPLER_MIPMAP_MODE_NEAREST: i32 = 0;
pub const VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE: i32 = 2;
pub const VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER: i32 = 3;
pub const VK_BORDER_COLOR_FLOAT_OPAQUE_WHITE: i32 = 4;
pub const VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER: i32 = 1;
pub const VK_FRONT_FACE_COUNTER_CLOCKWISE: i32 = 0;
pub const VK_VERTEX_INPUT_RATE_VERTEX: i32 = 0;
pub const VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST: i32 = 3;
pub const VK_POLYGON_MODE_FILL: i32 = 0;
pub const VK_ATTACHMENT_LOAD_OP_LOAD: i32 = 0;
pub const VK_ATTACHMENT_LOAD_OP_CLEAR: i32 = 1;
pub const VK_ATTACHMENT_LOAD_OP_DONT_CARE: i32 = 2;
pub const VK_ATTACHMENT_STORE_OP_STORE: i32 = 0;
pub const VK_ATTACHMENT_STORE_OP_DONT_CARE: i32 = 1;
pub const VK_RESOLVE_MODE_NONE: u32 = 0;
pub const VK_PRESENT_MODE_FIFO_KHR: i32 = 2;
pub const VK_COLOR_SPACE_SRGB_NONLINEAR_KHR: i32 = 0;
pub const VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT: i32 = 4;
pub const VK_QUERY_TYPE_TIMESTAMP: i32 = 2;
pub const VK_PIPELINE_CACHE_HEADER_VERSION_ONE: u32 = 1;

// Flags
pub const VK_QUEUE_GRAPHICS_BIT: VkFlags = 0x1;
pub const VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT: VkFlags = 0x1;
pub const VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT: VkFlags = 0x2;
pub const VK_MEMORY_PROPERTY_HOST_COHERENT_BIT: VkFlags = 0x4;
pub const VK_MEMORY_PROPERTY_HOST_CACHED_BIT: VkFlags = 0x8;
pub const VK_MEMORY_HEAP_DEVICE_LOCAL_BIT: VkFlags = 0x1;
pub const VK_QUERY_RESULT_64_BIT: VkFlags = 0x1;
pub const VK_QUERY_RESULT_WITH_AVAILABILITY_BIT: VkFlags = 0x4;
pub const VK_BUFFER_USAGE_TRANSFER_SRC_BIT: VkFlags = 0x1;
pub const VK_BUFFER_USAGE_TRANSFER_DST_BIT: VkFlags = 0x2;
pub const VK_BUFFER_USAGE_INDEX_BUFFER_BIT: VkFlags = 0x40;
pub const VK_BUFFER_USAGE_VERTEX_BUFFER_BIT: VkFlags = 0x80;
pub const VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT: VkFlags = 0x2_0000;
pub const VK_MEMORY_ALLOCATE_DEVICE_ADDRESS_BIT: VkFlags = 0x2;
pub const VK_IMAGE_USAGE_TRANSFER_SRC_BIT: VkFlags = 0x1;
pub const VK_IMAGE_USAGE_TRANSFER_DST_BIT: VkFlags = 0x2;
pub const VK_IMAGE_USAGE_SAMPLED_BIT: VkFlags = 0x4;
pub const VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT: VkFlags = 0x10;
pub const VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT: VkFlags = 0x20;
pub const VK_IMAGE_ASPECT_COLOR_BIT: VkFlags = 0x1;
pub const VK_IMAGE_ASPECT_DEPTH_BIT: VkFlags = 0x2;
pub const VK_SAMPLE_COUNT_1_BIT: VkFlags = 0x1;
pub const VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT: VkFlags = 0x2;
pub const VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT: VkFlags = 0x1;
pub const VK_FENCE_CREATE_SIGNALED_BIT: VkFlags = 0x1;
pub const VK_SHADER_STAGE_VERTEX_BIT: VkFlags = 0x1;
pub const VK_SHADER_STAGE_FRAGMENT_BIT: VkFlags = 0x10;
pub const VK_CULL_MODE_NONE: VkFlags = 0;
pub const VK_COLOR_COMPONENT_RGBA: VkFlags = 0xf;
pub const VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT: VkFlags = 0x1;
pub const VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BIT: VkFlags = 0x80;
pub const VK_FORMAT_FEATURE_COLOR_ATTACHMENT_BLEND_BIT: VkFlags = 0x100;
pub const VK_FORMAT_FEATURE_SAMPLED_IMAGE_FILTER_LINEAR_BIT: VkFlags = 0x1000;
pub const VK_DESCRIPTOR_BINDING_UPDATE_AFTER_BIND_BIT: VkFlags = 0x1;
pub const VK_DESCRIPTOR_BINDING_UPDATE_UNUSED_WHILE_PENDING_BIT: VkFlags = 0x2;
pub const VK_DESCRIPTOR_BINDING_PARTIALLY_BOUND_BIT: VkFlags = 0x4;
pub const VK_DESCRIPTOR_SET_LAYOUT_CREATE_UPDATE_AFTER_BIND_POOL_BIT: VkFlags = 0x2;
pub const VK_DESCRIPTOR_POOL_CREATE_UPDATE_AFTER_BIND_BIT: VkFlags = 0x2;
pub const VK_FORMAT_FEATURE_DEPTH_STENCIL_ATTACHMENT_BIT: VkFlags = 0x200;
pub const VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR: VkFlags = 0x1;
pub const VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR: VkFlags = 0x1;
pub const VK_DEBUG_UTILS_MESSAGE_SEVERITY_INFO_BIT_EXT: VkFlags = 0x10;
pub const VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT: VkFlags = 0x100;
pub const VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT: VkFlags = 0x1000;
pub const VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT: VkFlags = 0x1;
pub const VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT: VkFlags = 0x2;
pub const VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT: VkFlags = 0x4;

// Synchronization2 (64-bit flags)
pub const VK_PIPELINE_STAGE_2_NONE: VkFlags64 = 0;
pub const VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT: VkFlags64 = 0x80;
pub const VK_PIPELINE_STAGE_2_EARLY_FRAGMENT_TESTS_BIT: VkFlags64 = 0x100;
pub const VK_PIPELINE_STAGE_2_LATE_FRAGMENT_TESTS_BIT: VkFlags64 = 0x200;
pub const VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT: VkFlags64 = 0x400;
pub const VK_PIPELINE_STAGE_2_HOST_BIT: VkFlags64 = 0x4000;
pub const VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT: VkFlags64 = 0x10000;
pub const VK_PIPELINE_STAGE_2_COPY_BIT: VkFlags64 = 0x1_0000_0000;
pub const VK_PIPELINE_STAGE_2_INDEX_INPUT_BIT: VkFlags64 = 0x10_0000_0000;
pub const VK_PIPELINE_STAGE_2_VERTEX_ATTRIBUTE_INPUT_BIT: VkFlags64 = 0x20_0000_0000;
pub const VK_ACCESS_2_NONE: VkFlags64 = 0;
pub const VK_ACCESS_2_INDEX_READ_BIT: VkFlags64 = 0x2;
pub const VK_ACCESS_2_VERTEX_ATTRIBUTE_READ_BIT: VkFlags64 = 0x4;
pub const VK_ACCESS_2_COLOR_ATTACHMENT_READ_BIT: VkFlags64 = 0x80;
pub const VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT: VkFlags64 = 0x100;
pub const VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_READ_BIT: VkFlags64 = 0x200;
pub const VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT: VkFlags64 = 0x400;
pub const VK_ACCESS_2_TRANSFER_READ_BIT: VkFlags64 = 0x800;
pub const VK_ACCESS_2_TRANSFER_WRITE_BIT: VkFlags64 = 0x1000;
pub const VK_ACCESS_2_HOST_READ_BIT: VkFlags64 = 0x2000;
pub const VK_ACCESS_2_SHADER_SAMPLED_READ_BIT: VkFlags64 = 0x1_0000_0000;

// ---------------------------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VkExtent2D {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VkExtent3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VkOffset2D {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VkOffset3D {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VkRect2D {
    pub offset: VkOffset2D,
    pub extent: VkExtent2D,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VkViewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

/// `VkClearValue` is a C union of `VkClearColorValue` (4 × 32 bit) and `VkClearDepthStencilValue`
/// (`float depth; uint32_t stencil`). Both start at offset 0, so one `[f32; 4]` covers both:
/// colour = all four, depth = `[depth, 0.0 /* stencil 0 */, 0, 0]`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VkClearValue {
    pub float32: [f32; 4],
}

#[repr(C)]
pub struct VkApplicationInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub p_application_name: *const c_char,
    pub application_version: u32,
    pub p_engine_name: *const c_char,
    pub engine_version: u32,
    pub api_version: u32,
}

#[repr(C)]
pub struct VkInstanceCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub p_application_info: *const VkApplicationInfo,
    pub enabled_layer_count: u32,
    pub pp_enabled_layer_names: *const *const c_char,
    pub enabled_extension_count: u32,
    pub pp_enabled_extension_names: *const *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkLayerProperties {
    pub layer_name: [c_char; VK_MAX_EXTENSION_NAME_SIZE],
    pub spec_version: u32,
    pub implementation_version: u32,
    pub description: [c_char; VK_MAX_DESCRIPTION_SIZE],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkExtensionProperties {
    pub extension_name: [c_char; VK_MAX_EXTENSION_NAME_SIZE],
    pub spec_version: u32,
}

pub type PfnDebugUtilsMessengerCallback = unsafe extern "system" fn(
    message_severity: VkFlags,
    message_types: VkFlags,
    p_callback_data: *const VkDebugUtilsMessengerCallbackDataEXT,
    p_user_data: *mut c_void,
) -> VkBool32;

#[repr(C)]
pub struct VkDebugUtilsMessengerCreateInfoEXT {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub message_severity: VkFlags,
    pub message_type: VkFlags,
    pub pfn_user_callback: Option<PfnDebugUtilsMessengerCallback>,
    pub p_user_data: *mut c_void,
}

#[repr(C)]
pub struct VkDebugUtilsMessengerCallbackDataEXT {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub p_message_id_name: *const c_char,
    pub message_id_number: i32,
    pub p_message: *const c_char,
    pub queue_label_count: u32,
    pub p_queue_labels: *const c_void,
    pub cmd_buf_label_count: u32,
    pub p_cmd_buf_labels: *const c_void,
    pub object_count: u32,
    pub p_objects: *const c_void,
}

#[repr(C)]
pub struct VkValidationFeaturesEXT {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub enabled_validation_feature_count: u32,
    pub p_enabled_validation_features: *const i32,
    pub disabled_validation_feature_count: u32,
    pub p_disabled_validation_features: *const i32,
}

/// The first two limits by name; the remaining 104 members (496 bytes, 8-aligned because of
/// `VkDeviceSize` fields) are kept opaque until needed. Size/alignment match the header.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceLimits {
    pub max_image_dimension_1d: u32,
    pub max_image_dimension_2d: u32,
    pub rest: [u64; 62],
}

impl VkPhysicalDeviceLimits {
    /// Byte offsets of 32-bit members inside the C struct (vulkan_core.h, 64-bit layout). The
    /// test-only ones let a GPU test check the offsets against limits with known minimums.
    #[cfg(test)]
    pub const MAX_PUSH_CONSTANTS_SIZE: usize = 32;
    #[cfg(test)]
    pub const MAX_DRAW_INDEXED_INDEX_VALUE: usize = 260;
    pub const TIMESTAMP_PERIOD: usize = 424;

    /// The 32-bit member at byte `offset` (one of the constants above, so at least 8).
    pub fn word(&self, offset: usize) -> u32 {
        let i = offset - 8; // past the two named fields
        let b = self.rest[i / 8].to_ne_bytes();
        let k = i % 8;
        u32::from_ne_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]])
    }

    /// Nanoseconds per timestamp tick.
    pub fn timestamp_period(&self) -> f32 {
        f32::from_bits(self.word(Self::TIMESTAMP_PERIOD))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceSparseProperties {
    pub residency_standard_2d_block_shape: VkBool32,
    pub residency_standard_2d_multisample_block_shape: VkBool32,
    pub residency_standard_3d_block_shape: VkBool32,
    pub residency_aligned_mip_size: VkBool32,
    pub residency_non_resident_strict: VkBool32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceProperties {
    pub api_version: u32,
    pub driver_version: u32,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: i32,
    pub device_name: [c_char; VK_MAX_PHYSICAL_DEVICE_NAME_SIZE],
    pub pipeline_cache_uuid: [u8; VK_UUID_SIZE],
    pub limits: VkPhysicalDeviceLimits,
    pub sparse_properties: VkPhysicalDeviceSparseProperties,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkQueueFamilyProperties {
    pub queue_flags: VkFlags,
    pub queue_count: u32,
    pub timestamp_valid_bits: u32,
    pub min_image_transfer_granularity: VkExtent3D,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkMemoryType {
    pub property_flags: VkFlags,
    pub heap_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkMemoryHeap {
    pub size: VkDeviceSize,
    pub flags: VkFlags,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VkPhysicalDeviceMemoryProperties {
    pub memory_type_count: u32,
    pub memory_types: [VkMemoryType; VK_MAX_MEMORY_TYPES],
    pub memory_heap_count: u32,
    pub memory_heaps: [VkMemoryHeap; VK_MAX_MEMORY_HEAPS],
}

#[repr(C)]
pub struct VkPhysicalDeviceMemoryProperties2 {
    pub s_type: VkStructureType,
    pub p_next: *mut c_void,
    pub memory_properties: VkPhysicalDeviceMemoryProperties,
}

/// `VK_EXT_memory_budget`: per-heap budget and current process usage, in bytes.
#[repr(C)]
pub struct VkPhysicalDeviceMemoryBudgetPropertiesEXT {
    pub s_type: VkStructureType,
    pub p_next: *mut c_void,
    pub heap_budget: [VkDeviceSize; VK_MAX_MEMORY_HEAPS],
    pub heap_usage: [VkDeviceSize; VK_MAX_MEMORY_HEAPS],
}

#[repr(C)]
pub struct VkQueryPoolCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub query_type: i32,
    pub query_count: u32,
    pub pipeline_statistics: VkFlags,
}

#[repr(C)]
pub struct VkPipelineCacheCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub initial_data_size: usize,
    pub p_initial_data: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkFormatProperties {
    pub linear_tiling_features: VkFlags,
    pub optimal_tiling_features: VkFlags,
    pub buffer_features: VkFlags,
}

/// The 55 `VkBool32` members of `VkPhysicalDeviceFeatures` (we enable none of them).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceFeatures {
    pub flags: [VkBool32; 55],
}

#[repr(C)]
pub struct VkPhysicalDeviceFeatures2 {
    pub s_type: VkStructureType,
    pub p_next: *mut c_void,
    pub features: VkPhysicalDeviceFeatures,
}

/// The 47 `VkBool32` members of `VkPhysicalDeviceVulkan12Features`, by index.
#[repr(C)]
pub struct VkPhysicalDeviceVulkan12Features {
    pub s_type: VkStructureType,
    pub p_next: *mut c_void,
    pub flags: [VkBool32; 47],
}

impl VkPhysicalDeviceVulkan12Features {
    /// Indices of members in [`Self::flags`] (declaration order in vulkan_core.h).
    pub const SHADER_SAMPLED_IMAGE_ARRAY_NON_UNIFORM_INDEXING: usize = 14;
    pub const DESCRIPTOR_BINDING_SAMPLED_IMAGE_UPDATE_AFTER_BIND: usize = 21;
    pub const DESCRIPTOR_BINDING_UPDATE_UNUSED_WHILE_PENDING: usize = 26;
    pub const DESCRIPTOR_BINDING_PARTIALLY_BOUND: usize = 27;
    pub const RUNTIME_DESCRIPTOR_ARRAY: usize = 29;
    pub const BUFFER_DEVICE_ADDRESS: usize = 38;

    /// The descriptor-indexing features the bindless texture array needs.
    pub const BINDLESS: [usize; 5] = [
        Self::SHADER_SAMPLED_IMAGE_ARRAY_NON_UNIFORM_INDEXING,
        Self::DESCRIPTOR_BINDING_SAMPLED_IMAGE_UPDATE_AFTER_BIND,
        Self::DESCRIPTOR_BINDING_UPDATE_UNUSED_WHILE_PENDING,
        Self::DESCRIPTOR_BINDING_PARTIALLY_BOUND,
        Self::RUNTIME_DESCRIPTOR_ARRAY,
    ];

    /// All features off, chained to `next`.
    pub fn new(next: *mut c_void) -> Self {
        VkPhysicalDeviceVulkan12Features { s_type: ST_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES, p_next: next, flags: [0; 47] }
    }
}

#[repr(C)]
pub struct VkMemoryAllocateFlagsInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub device_mask: u32,
}

#[repr(C)]
pub struct VkBufferDeviceAddressInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub buffer: VkBuffer,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkPhysicalDeviceVulkan13Features {
    pub s_type: VkStructureType,
    pub p_next: *mut c_void,
    pub robust_image_access: VkBool32,
    pub inline_uniform_block: VkBool32,
    pub descriptor_binding_inline_uniform_block_update_after_bind: VkBool32,
    pub pipeline_creation_cache_control: VkBool32,
    pub private_data: VkBool32,
    pub shader_demote_to_helper_invocation: VkBool32,
    pub shader_terminate_invocation: VkBool32,
    pub subgroup_size_control: VkBool32,
    pub compute_full_subgroups: VkBool32,
    pub synchronization2: VkBool32,
    pub texture_compression_astc_hdr: VkBool32,
    pub shader_zero_initialize_workgroup_memory: VkBool32,
    pub dynamic_rendering: VkBool32,
    pub shader_integer_dot_product: VkBool32,
    pub maintenance4: VkBool32,
}

impl VkPhysicalDeviceVulkan13Features {
    /// All features off, no chain.
    pub fn new() -> Self {
        VkPhysicalDeviceVulkan13Features {
            s_type: ST_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,
            p_next: std::ptr::null_mut(),
            robust_image_access: 0,
            inline_uniform_block: 0,
            descriptor_binding_inline_uniform_block_update_after_bind: 0,
            pipeline_creation_cache_control: 0,
            private_data: 0,
            shader_demote_to_helper_invocation: 0,
            shader_terminate_invocation: 0,
            subgroup_size_control: 0,
            compute_full_subgroups: 0,
            synchronization2: 0,
            texture_compression_astc_hdr: 0,
            shader_zero_initialize_workgroup_memory: 0,
            dynamic_rendering: 0,
            shader_integer_dot_product: 0,
            maintenance4: 0,
        }
    }
}

#[repr(C)]
pub struct VkDeviceQueueCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub queue_family_index: u32,
    pub queue_count: u32,
    pub p_queue_priorities: *const f32,
}

#[repr(C)]
pub struct VkDeviceCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub queue_create_info_count: u32,
    pub p_queue_create_infos: *const VkDeviceQueueCreateInfo,
    pub enabled_layer_count: u32,
    pub pp_enabled_layer_names: *const *const c_char,
    pub enabled_extension_count: u32,
    pub pp_enabled_extension_names: *const *const c_char,
    pub p_enabled_features: *const VkPhysicalDeviceFeatures,
}

#[repr(C)]
pub struct VkBufferCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub size: VkDeviceSize,
    pub usage: VkFlags,
    pub sharing_mode: i32,
    pub queue_family_index_count: u32,
    pub p_queue_family_indices: *const u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkMemoryRequirements {
    pub size: VkDeviceSize,
    pub alignment: VkDeviceSize,
    pub memory_type_bits: u32,
}

#[repr(C)]
pub struct VkMemoryAllocateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub allocation_size: VkDeviceSize,
    pub memory_type_index: u32,
}

#[repr(C)]
pub struct VkImageCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub image_type: i32,
    pub format: VkFormat,
    pub extent: VkExtent3D,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub samples: VkFlags,
    pub tiling: i32,
    pub usage: VkFlags,
    pub sharing_mode: i32,
    pub queue_family_index_count: u32,
    pub p_queue_family_indices: *const u32,
    pub initial_layout: VkImageLayout,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkComponentMapping {
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub a: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkImageSubresourceRange {
    pub aspect_mask: VkFlags,
    pub base_mip_level: u32,
    pub level_count: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

#[repr(C)]
pub struct VkImageViewCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub image: VkImage,
    pub view_type: i32,
    pub format: VkFormat,
    pub components: VkComponentMapping,
    pub subresource_range: VkImageSubresourceRange,
}

#[repr(C)]
pub struct VkShaderModuleCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub code_size: usize,
    pub p_code: *const u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkPushConstantRange {
    pub stage_flags: VkFlags,
    pub offset: u32,
    pub size: u32,
}

#[repr(C)]
pub struct VkPipelineLayoutCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub set_layout_count: u32,
    pub p_set_layouts: *const u64,
    pub push_constant_range_count: u32,
    pub p_push_constant_ranges: *const VkPushConstantRange,
}

#[repr(C)]
pub struct VkPipelineShaderStageCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub stage: VkFlags,
    pub module: VkShaderModule,
    pub p_name: *const c_char,
    pub p_specialization_info: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkVertexInputBindingDescription {
    pub binding: u32,
    pub stride: u32,
    pub input_rate: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkVertexInputAttributeDescription {
    pub location: u32,
    pub binding: u32,
    pub format: VkFormat,
    pub offset: u32,
}

#[repr(C)]
pub struct VkPipelineVertexInputStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub vertex_binding_description_count: u32,
    pub p_vertex_binding_descriptions: *const VkVertexInputBindingDescription,
    pub vertex_attribute_description_count: u32,
    pub p_vertex_attribute_descriptions: *const VkVertexInputAttributeDescription,
}

#[repr(C)]
pub struct VkPipelineInputAssemblyStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub topology: i32,
    pub primitive_restart_enable: VkBool32,
}

#[repr(C)]
pub struct VkPipelineViewportStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub viewport_count: u32,
    pub p_viewports: *const VkViewport,
    pub scissor_count: u32,
    pub p_scissors: *const VkRect2D,
}

#[repr(C)]
pub struct VkPipelineRasterizationStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub depth_clamp_enable: VkBool32,
    pub rasterizer_discard_enable: VkBool32,
    pub polygon_mode: i32,
    pub cull_mode: VkFlags,
    pub front_face: i32,
    pub depth_bias_enable: VkBool32,
    pub depth_bias_constant_factor: f32,
    pub depth_bias_clamp: f32,
    pub depth_bias_slope_factor: f32,
    pub line_width: f32,
}

#[repr(C)]
pub struct VkPipelineMultisampleStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub rasterization_samples: VkFlags,
    pub sample_shading_enable: VkBool32,
    pub min_sample_shading: f32,
    pub p_sample_mask: *const u32,
    pub alpha_to_coverage_enable: VkBool32,
    pub alpha_to_one_enable: VkBool32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkStencilOpState {
    pub fail_op: i32,
    pub pass_op: i32,
    pub depth_fail_op: i32,
    pub compare_op: i32,
    pub compare_mask: u32,
    pub write_mask: u32,
    pub reference: u32,
}

#[repr(C)]
pub struct VkPipelineDepthStencilStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub depth_test_enable: VkBool32,
    pub depth_write_enable: VkBool32,
    pub depth_compare_op: i32,
    pub depth_bounds_test_enable: VkBool32,
    pub stencil_test_enable: VkBool32,
    pub front: VkStencilOpState,
    pub back: VkStencilOpState,
    pub min_depth_bounds: f32,
    pub max_depth_bounds: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkPipelineColorBlendAttachmentState {
    pub blend_enable: VkBool32,
    pub src_color_blend_factor: i32,
    pub dst_color_blend_factor: i32,
    pub color_blend_op: i32,
    pub src_alpha_blend_factor: i32,
    pub dst_alpha_blend_factor: i32,
    pub alpha_blend_op: i32,
    pub color_write_mask: VkFlags,
}

#[repr(C)]
pub struct VkPipelineColorBlendStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub logic_op_enable: VkBool32,
    pub logic_op: i32,
    pub attachment_count: u32,
    pub p_attachments: *const VkPipelineColorBlendAttachmentState,
    pub blend_constants: [f32; 4],
}

#[repr(C)]
pub struct VkPipelineDynamicStateCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub dynamic_state_count: u32,
    pub p_dynamic_states: *const i32,
}

#[repr(C)]
pub struct VkPipelineRenderingCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub view_mask: u32,
    pub color_attachment_count: u32,
    pub p_color_attachment_formats: *const VkFormat,
    pub depth_attachment_format: VkFormat,
    pub stencil_attachment_format: VkFormat,
}

#[repr(C)]
pub struct VkGraphicsPipelineCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub stage_count: u32,
    pub p_stages: *const VkPipelineShaderStageCreateInfo,
    pub p_vertex_input_state: *const VkPipelineVertexInputStateCreateInfo,
    pub p_input_assembly_state: *const VkPipelineInputAssemblyStateCreateInfo,
    pub p_tessellation_state: *const c_void,
    pub p_viewport_state: *const VkPipelineViewportStateCreateInfo,
    pub p_rasterization_state: *const VkPipelineRasterizationStateCreateInfo,
    pub p_multisample_state: *const VkPipelineMultisampleStateCreateInfo,
    pub p_depth_stencil_state: *const VkPipelineDepthStencilStateCreateInfo,
    pub p_color_blend_state: *const VkPipelineColorBlendStateCreateInfo,
    pub p_dynamic_state: *const VkPipelineDynamicStateCreateInfo,
    pub layout: VkPipelineLayout,
    pub render_pass: u64,
    pub subpass: u32,
    pub base_pipeline_handle: VkPipeline,
    pub base_pipeline_index: i32,
}

#[repr(C)]
pub struct VkCommandPoolCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub queue_family_index: u32,
}

#[repr(C)]
pub struct VkCommandBufferAllocateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub command_pool: VkCommandPool,
    pub level: i32,
    pub command_buffer_count: u32,
}

#[repr(C)]
pub struct VkCommandBufferBeginInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub p_inheritance_info: *const c_void,
}

#[repr(C)]
pub struct VkFenceCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
}

#[repr(C)]
pub struct VkSemaphoreCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
}

#[repr(C)]
pub struct VkSemaphoreSubmitInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub semaphore: VkSemaphore,
    pub value: u64,
    pub stage_mask: VkFlags64,
    pub device_index: u32,
}

#[repr(C)]
pub struct VkCommandBufferSubmitInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub command_buffer: VkCommandBuffer,
    pub device_mask: u32,
}

#[repr(C)]
pub struct VkSubmitInfo2 {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub wait_semaphore_info_count: u32,
    pub p_wait_semaphore_infos: *const VkSemaphoreSubmitInfo,
    pub command_buffer_info_count: u32,
    pub p_command_buffer_infos: *const VkCommandBufferSubmitInfo,
    pub signal_semaphore_info_count: u32,
    pub p_signal_semaphore_infos: *const VkSemaphoreSubmitInfo,
}

#[repr(C)]
pub struct VkBufferMemoryBarrier2 {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub src_stage_mask: VkFlags64,
    pub src_access_mask: VkFlags64,
    pub dst_stage_mask: VkFlags64,
    pub dst_access_mask: VkFlags64,
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
    pub buffer: VkBuffer,
    pub offset: VkDeviceSize,
    pub size: VkDeviceSize,
}

#[repr(C)]
pub struct VkImageMemoryBarrier2 {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub src_stage_mask: VkFlags64,
    pub src_access_mask: VkFlags64,
    pub dst_stage_mask: VkFlags64,
    pub dst_access_mask: VkFlags64,
    pub old_layout: VkImageLayout,
    pub new_layout: VkImageLayout,
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
    pub image: VkImage,
    pub subresource_range: VkImageSubresourceRange,
}

#[repr(C)]
pub struct VkDependencyInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub dependency_flags: VkFlags,
    pub memory_barrier_count: u32,
    pub p_memory_barriers: *const c_void,
    pub buffer_memory_barrier_count: u32,
    pub p_buffer_memory_barriers: *const VkBufferMemoryBarrier2,
    pub image_memory_barrier_count: u32,
    pub p_image_memory_barriers: *const VkImageMemoryBarrier2,
}

#[repr(C)]
pub struct VkRenderingAttachmentInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub image_view: VkImageView,
    pub image_layout: VkImageLayout,
    pub resolve_mode: u32,
    pub resolve_image_view: VkImageView,
    pub resolve_image_layout: VkImageLayout,
    pub load_op: i32,
    pub store_op: i32,
    pub clear_value: VkClearValue,
}

#[repr(C)]
pub struct VkRenderingInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub render_area: VkRect2D,
    pub layer_count: u32,
    pub view_mask: u32,
    pub color_attachment_count: u32,
    pub p_color_attachments: *const VkRenderingAttachmentInfo,
    pub p_depth_attachment: *const VkRenderingAttachmentInfo,
    pub p_stencil_attachment: *const VkRenderingAttachmentInfo,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkBufferCopy {
    pub src_offset: VkDeviceSize,
    pub dst_offset: VkDeviceSize,
    pub size: VkDeviceSize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkImageSubresourceLayers {
    pub aspect_mask: VkFlags,
    pub mip_level: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkBufferImageCopy {
    pub buffer_offset: VkDeviceSize,
    pub buffer_row_length: u32,
    pub buffer_image_height: u32,
    pub image_subresource: VkImageSubresourceLayers,
    pub image_offset: VkOffset3D,
    pub image_extent: VkExtent3D,
}

#[repr(C)]
pub struct VkWin32SurfaceCreateInfoKHR {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub hinstance: *mut c_void,
    pub hwnd: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VkSurfaceCapabilitiesKHR {
    pub min_image_count: u32,
    pub max_image_count: u32,
    pub current_extent: VkExtent2D,
    pub min_image_extent: VkExtent2D,
    pub max_image_extent: VkExtent2D,
    pub max_image_array_layers: u32,
    pub supported_transforms: VkFlags,
    pub current_transform: VkFlags,
    pub supported_composite_alpha: VkFlags,
    pub supported_usage_flags: VkFlags,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VkSurfaceFormatKHR {
    pub format: VkFormat,
    pub color_space: i32,
}

#[repr(C)]
pub struct VkSwapchainCreateInfoKHR {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub surface: VkSurfaceKHR,
    pub min_image_count: u32,
    pub image_format: VkFormat,
    pub image_color_space: i32,
    pub image_extent: VkExtent2D,
    pub image_array_layers: u32,
    pub image_usage: VkFlags,
    pub image_sharing_mode: i32,
    pub queue_family_index_count: u32,
    pub p_queue_family_indices: *const u32,
    pub pre_transform: VkFlags,
    pub composite_alpha: VkFlags,
    pub present_mode: i32,
    pub clipped: VkBool32,
    pub old_swapchain: VkSwapchainKHR,
}

#[repr(C)]
pub struct VkPresentInfoKHR {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub wait_semaphore_count: u32,
    pub p_wait_semaphores: *const VkSemaphore,
    pub swapchain_count: u32,
    pub p_swapchains: *const VkSwapchainKHR,
    pub p_image_indices: *const u32,
    pub p_results: *mut VkResult,
}

#[repr(C)]
pub struct VkSamplerCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub mag_filter: i32,
    pub min_filter: i32,
    pub mipmap_mode: i32,
    pub address_mode_u: i32,
    pub address_mode_v: i32,
    pub address_mode_w: i32,
    pub mip_lod_bias: f32,
    pub anisotropy_enable: VkBool32,
    pub max_anisotropy: f32,
    pub compare_enable: VkBool32,
    pub compare_op: i32,
    pub min_lod: f32,
    pub max_lod: f32,
    pub border_color: i32,
    pub unnormalized_coordinates: VkBool32,
}

#[repr(C)]
pub struct VkDescriptorSetLayoutBinding {
    pub binding: u32,
    pub descriptor_type: i32,
    pub descriptor_count: u32,
    pub stage_flags: VkFlags,
    pub p_immutable_samplers: *const VkSampler,
}

#[repr(C)]
pub struct VkDescriptorSetLayoutBindingFlagsCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub binding_count: u32,
    pub p_binding_flags: *const VkFlags,
}

#[repr(C)]
pub struct VkDescriptorSetLayoutCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub binding_count: u32,
    pub p_bindings: *const VkDescriptorSetLayoutBinding,
}

#[repr(C)]
pub struct VkDescriptorPoolSize {
    pub ty: i32,
    pub descriptor_count: u32,
}

#[repr(C)]
pub struct VkDescriptorPoolCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub flags: VkFlags,
    pub max_sets: u32,
    pub pool_size_count: u32,
    pub p_pool_sizes: *const VkDescriptorPoolSize,
}

#[repr(C)]
pub struct VkDescriptorSetAllocateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub descriptor_pool: VkDescriptorPool,
    pub descriptor_set_count: u32,
    pub p_set_layouts: *const VkDescriptorSetLayout,
}

#[repr(C)]
pub struct VkDescriptorImageInfo {
    pub sampler: VkSampler,
    pub image_view: VkImageView,
    pub image_layout: VkImageLayout,
}

#[repr(C)]
pub struct VkWriteDescriptorSet {
    pub s_type: VkStructureType,
    pub p_next: *const c_void,
    pub dst_set: VkDescriptorSet,
    pub dst_binding: u32,
    pub dst_array_element: u32,
    pub descriptor_count: u32,
    pub descriptor_type: i32,
    pub p_image_info: *const VkDescriptorImageInfo,
    pub p_buffer_info: *const c_void,
    pub p_texel_buffer_view: *const c_void,
}

#[repr(C)]
pub struct VkSpecializationMapEntry {
    pub constant_id: u32,
    pub offset: u32,
    pub size: usize,
}

#[repr(C)]
pub struct VkSpecializationInfo {
    pub map_entry_count: u32,
    pub p_map_entries: *const VkSpecializationMapEntry,
    pub data_size: usize,
    pub p_data: *const c_void,
}

// ---------------------------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------------------------

/// `PFN_vkVoidFunction`; `None` = NULL.
pub type PfnVoidFunction = Option<unsafe extern "system" fn()>;

pub type PfnGetInstanceProcAddr =
    unsafe extern "system" fn(instance: VkInstance, p_name: *const c_char) -> PfnVoidFunction;

/// Declares a table of Vulkan entry points and a `load` function that resolves each one by name
/// through a `vkGet*ProcAddr`-style getter. A missing entry point fails the whole table with its name.
macro_rules! fn_table {
    ($(#[$meta:meta])* pub struct $table:ident {
        $( $field:ident = $sym:literal : fn($($arg:ty),* $(,)?) $(-> $ret:ty)?; )*
    }) => {
        $(#[$meta])*
        pub struct $table {
            $( pub $field: unsafe extern "system" fn($($arg),*) $(-> $ret)?, )*
        }

        impl $table {
            pub fn load(mut get: impl FnMut(&CStr) -> PfnVoidFunction) -> Result<Self, String> {
                Ok($table {
                    $( $field: {
                        let sym: &CStr = $sym;
                        match get(sym) {
                            // SAFETY: the loader returned the entry point named `sym`; its C
                            // prototype is the one declared here (transcribed from vulkan_core.h),
                            // and all function pointers share one representation.
                            Some(f) => unsafe {
                                std::mem::transmute::<
                                    unsafe extern "system" fn(),
                                    unsafe extern "system" fn($($arg),*) $(-> $ret)?,
                                >(f)
                            },
                            None => return Err(sym.to_string_lossy().into_owned()),
                        }
                    }, )*
                })
            }
        }
    };
}

fn_table! {
    /// Global commands, resolved with `vkGetInstanceProcAddr(NULL, …)`.
    pub struct EntryFns {
        create_instance = c"vkCreateInstance":
            fn(*const VkInstanceCreateInfo, *const c_void, *mut VkInstance) -> VkResult;
        enumerate_instance_extension_properties = c"vkEnumerateInstanceExtensionProperties":
            fn(*const c_char, *mut u32, *mut VkExtensionProperties) -> VkResult;
        enumerate_instance_layer_properties = c"vkEnumerateInstanceLayerProperties":
            fn(*mut u32, *mut VkLayerProperties) -> VkResult;
    }
}

pub type PfnEnumerateInstanceVersion = unsafe extern "system" fn(*mut u32) -> VkResult;

fn_table! {
    pub struct InstanceFns {
        destroy_instance = c"vkDestroyInstance": fn(VkInstance, *const c_void);
        enumerate_physical_devices = c"vkEnumeratePhysicalDevices":
            fn(VkInstance, *mut u32, *mut VkPhysicalDevice) -> VkResult;
        get_physical_device_properties = c"vkGetPhysicalDeviceProperties":
            fn(VkPhysicalDevice, *mut VkPhysicalDeviceProperties);
        get_physical_device_queue_family_properties = c"vkGetPhysicalDeviceQueueFamilyProperties":
            fn(VkPhysicalDevice, *mut u32, *mut VkQueueFamilyProperties);
        get_physical_device_memory_properties = c"vkGetPhysicalDeviceMemoryProperties":
            fn(VkPhysicalDevice, *mut VkPhysicalDeviceMemoryProperties);
        get_physical_device_memory_properties2 = c"vkGetPhysicalDeviceMemoryProperties2":
            fn(VkPhysicalDevice, *mut VkPhysicalDeviceMemoryProperties2);
        get_physical_device_features2 = c"vkGetPhysicalDeviceFeatures2":
            fn(VkPhysicalDevice, *mut VkPhysicalDeviceFeatures2);
        get_physical_device_format_properties = c"vkGetPhysicalDeviceFormatProperties":
            fn(VkPhysicalDevice, VkFormat, *mut VkFormatProperties);
        enumerate_device_extension_properties = c"vkEnumerateDeviceExtensionProperties":
            fn(VkPhysicalDevice, *const c_char, *mut u32, *mut VkExtensionProperties) -> VkResult;
        create_device = c"vkCreateDevice":
            fn(VkPhysicalDevice, *const VkDeviceCreateInfo, *const c_void, *mut VkDevice) -> VkResult;
        get_device_proc_addr = c"vkGetDeviceProcAddr": fn(VkDevice, *const c_char) -> PfnVoidFunction;
    }
}

fn_table! {
    /// `VK_EXT_debug_utils` (instance level).
    pub struct DebugUtilsFns {
        create_debug_utils_messenger = c"vkCreateDebugUtilsMessengerEXT":
            fn(VkInstance, *const VkDebugUtilsMessengerCreateInfoEXT, *const c_void, *mut VkDebugUtilsMessengerEXT)
                -> VkResult;
        destroy_debug_utils_messenger = c"vkDestroyDebugUtilsMessengerEXT":
            fn(VkInstance, VkDebugUtilsMessengerEXT, *const c_void);
    }
}

fn_table! {
    /// `VK_KHR_surface` + `VK_KHR_win32_surface` (instance level).
    pub struct SurfaceFns {
        destroy_surface = c"vkDestroySurfaceKHR": fn(VkInstance, VkSurfaceKHR, *const c_void);
        get_physical_device_surface_support = c"vkGetPhysicalDeviceSurfaceSupportKHR":
            fn(VkPhysicalDevice, u32, VkSurfaceKHR, *mut VkBool32) -> VkResult;
        get_physical_device_surface_capabilities = c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR":
            fn(VkPhysicalDevice, VkSurfaceKHR, *mut VkSurfaceCapabilitiesKHR) -> VkResult;
        get_physical_device_surface_formats = c"vkGetPhysicalDeviceSurfaceFormatsKHR":
            fn(VkPhysicalDevice, VkSurfaceKHR, *mut u32, *mut VkSurfaceFormatKHR) -> VkResult;
        create_win32_surface = c"vkCreateWin32SurfaceKHR":
            fn(VkInstance, *const VkWin32SurfaceCreateInfoKHR, *const c_void, *mut VkSurfaceKHR) -> VkResult;
    }
}

fn_table! {
    /// Core device commands (Vulkan 1.3), resolved with `vkGetDeviceProcAddr`.
    pub struct DeviceFns {
        destroy_device = c"vkDestroyDevice": fn(VkDevice, *const c_void);
        get_device_queue = c"vkGetDeviceQueue": fn(VkDevice, u32, u32, *mut VkQueue);
        device_wait_idle = c"vkDeviceWaitIdle": fn(VkDevice) -> VkResult;
        create_buffer = c"vkCreateBuffer":
            fn(VkDevice, *const VkBufferCreateInfo, *const c_void, *mut VkBuffer) -> VkResult;
        destroy_buffer = c"vkDestroyBuffer": fn(VkDevice, VkBuffer, *const c_void);
        get_buffer_memory_requirements = c"vkGetBufferMemoryRequirements":
            fn(VkDevice, VkBuffer, *mut VkMemoryRequirements);
        bind_buffer_memory = c"vkBindBufferMemory":
            fn(VkDevice, VkBuffer, VkDeviceMemory, VkDeviceSize) -> VkResult;
        create_image = c"vkCreateImage":
            fn(VkDevice, *const VkImageCreateInfo, *const c_void, *mut VkImage) -> VkResult;
        destroy_image = c"vkDestroyImage": fn(VkDevice, VkImage, *const c_void);
        get_image_memory_requirements = c"vkGetImageMemoryRequirements":
            fn(VkDevice, VkImage, *mut VkMemoryRequirements);
        bind_image_memory = c"vkBindImageMemory":
            fn(VkDevice, VkImage, VkDeviceMemory, VkDeviceSize) -> VkResult;
        create_image_view = c"vkCreateImageView":
            fn(VkDevice, *const VkImageViewCreateInfo, *const c_void, *mut VkImageView) -> VkResult;
        destroy_image_view = c"vkDestroyImageView": fn(VkDevice, VkImageView, *const c_void);
        allocate_memory = c"vkAllocateMemory":
            fn(VkDevice, *const VkMemoryAllocateInfo, *const c_void, *mut VkDeviceMemory) -> VkResult;
        free_memory = c"vkFreeMemory": fn(VkDevice, VkDeviceMemory, *const c_void);
        map_memory = c"vkMapMemory":
            fn(VkDevice, VkDeviceMemory, VkDeviceSize, VkDeviceSize, VkFlags, *mut *mut c_void) -> VkResult;
        unmap_memory = c"vkUnmapMemory": fn(VkDevice, VkDeviceMemory);
        create_shader_module = c"vkCreateShaderModule":
            fn(VkDevice, *const VkShaderModuleCreateInfo, *const c_void, *mut VkShaderModule) -> VkResult;
        destroy_shader_module = c"vkDestroyShaderModule": fn(VkDevice, VkShaderModule, *const c_void);
        create_pipeline_layout = c"vkCreatePipelineLayout":
            fn(VkDevice, *const VkPipelineLayoutCreateInfo, *const c_void, *mut VkPipelineLayout) -> VkResult;
        destroy_pipeline_layout = c"vkDestroyPipelineLayout": fn(VkDevice, VkPipelineLayout, *const c_void);
        create_graphics_pipelines = c"vkCreateGraphicsPipelines":
            fn(VkDevice, u64, u32, *const VkGraphicsPipelineCreateInfo, *const c_void, *mut VkPipeline) -> VkResult;
        destroy_pipeline = c"vkDestroyPipeline": fn(VkDevice, VkPipeline, *const c_void);
        create_command_pool = c"vkCreateCommandPool":
            fn(VkDevice, *const VkCommandPoolCreateInfo, *const c_void, *mut VkCommandPool) -> VkResult;
        destroy_command_pool = c"vkDestroyCommandPool": fn(VkDevice, VkCommandPool, *const c_void);
        allocate_command_buffers = c"vkAllocateCommandBuffers":
            fn(VkDevice, *const VkCommandBufferAllocateInfo, *mut VkCommandBuffer) -> VkResult;
        free_command_buffers = c"vkFreeCommandBuffers":
            fn(VkDevice, VkCommandPool, u32, *const VkCommandBuffer);
        begin_command_buffer = c"vkBeginCommandBuffer":
            fn(VkCommandBuffer, *const VkCommandBufferBeginInfo) -> VkResult;
        end_command_buffer = c"vkEndCommandBuffer": fn(VkCommandBuffer) -> VkResult;
        reset_command_buffer = c"vkResetCommandBuffer": fn(VkCommandBuffer, VkFlags) -> VkResult;
        create_fence = c"vkCreateFence":
            fn(VkDevice, *const VkFenceCreateInfo, *const c_void, *mut VkFence) -> VkResult;
        destroy_fence = c"vkDestroyFence": fn(VkDevice, VkFence, *const c_void);
        wait_for_fences = c"vkWaitForFences": fn(VkDevice, u32, *const VkFence, VkBool32, u64) -> VkResult;
        reset_fences = c"vkResetFences": fn(VkDevice, u32, *const VkFence) -> VkResult;
        create_semaphore = c"vkCreateSemaphore":
            fn(VkDevice, *const VkSemaphoreCreateInfo, *const c_void, *mut VkSemaphore) -> VkResult;
        destroy_semaphore = c"vkDestroySemaphore": fn(VkDevice, VkSemaphore, *const c_void);
        queue_submit2 = c"vkQueueSubmit2": fn(VkQueue, u32, *const VkSubmitInfo2, VkFence) -> VkResult;
        cmd_pipeline_barrier2 = c"vkCmdPipelineBarrier2": fn(VkCommandBuffer, *const VkDependencyInfo);
        cmd_begin_rendering = c"vkCmdBeginRendering": fn(VkCommandBuffer, *const VkRenderingInfo);
        cmd_end_rendering = c"vkCmdEndRendering": fn(VkCommandBuffer);
        cmd_bind_pipeline = c"vkCmdBindPipeline": fn(VkCommandBuffer, i32, VkPipeline);
        cmd_set_viewport = c"vkCmdSetViewport": fn(VkCommandBuffer, u32, u32, *const VkViewport);
        cmd_set_scissor = c"vkCmdSetScissor": fn(VkCommandBuffer, u32, u32, *const VkRect2D);
        cmd_bind_vertex_buffers = c"vkCmdBindVertexBuffers":
            fn(VkCommandBuffer, u32, u32, *const VkBuffer, *const VkDeviceSize);
        cmd_bind_index_buffer = c"vkCmdBindIndexBuffer": fn(VkCommandBuffer, VkBuffer, VkDeviceSize, i32);
        cmd_push_constants = c"vkCmdPushConstants":
            fn(VkCommandBuffer, VkPipelineLayout, VkFlags, u32, u32, *const c_void);
        cmd_draw_indexed = c"vkCmdDrawIndexed": fn(VkCommandBuffer, u32, u32, u32, i32, u32);
        get_buffer_device_address = c"vkGetBufferDeviceAddress": fn(VkDevice, *const VkBufferDeviceAddressInfo) -> u64;
        cmd_copy_buffer = c"vkCmdCopyBuffer":
            fn(VkCommandBuffer, VkBuffer, VkBuffer, u32, *const VkBufferCopy);
        cmd_copy_image_to_buffer = c"vkCmdCopyImageToBuffer":
            fn(VkCommandBuffer, VkImage, VkImageLayout, VkBuffer, u32, *const VkBufferImageCopy);
        create_query_pool = c"vkCreateQueryPool":
            fn(VkDevice, *const VkQueryPoolCreateInfo, *const c_void, *mut VkQueryPool) -> VkResult;
        destroy_query_pool = c"vkDestroyQueryPool": fn(VkDevice, VkQueryPool, *const c_void);
        cmd_reset_query_pool = c"vkCmdResetQueryPool": fn(VkCommandBuffer, VkQueryPool, u32, u32);
        cmd_write_timestamp2 = c"vkCmdWriteTimestamp2": fn(VkCommandBuffer, VkFlags64, VkQueryPool, u32);
        get_query_pool_results = c"vkGetQueryPoolResults":
            fn(VkDevice, VkQueryPool, u32, u32, usize, *mut c_void, VkDeviceSize, VkFlags) -> VkResult;
        create_pipeline_cache = c"vkCreatePipelineCache":
            fn(VkDevice, *const VkPipelineCacheCreateInfo, *const c_void, *mut VkPipelineCache) -> VkResult;
        destroy_pipeline_cache = c"vkDestroyPipelineCache": fn(VkDevice, VkPipelineCache, *const c_void);
        get_pipeline_cache_data = c"vkGetPipelineCacheData":
            fn(VkDevice, VkPipelineCache, *mut usize, *mut c_void) -> VkResult;
        create_sampler = c"vkCreateSampler":
            fn(VkDevice, *const VkSamplerCreateInfo, *const c_void, *mut VkSampler) -> VkResult;
        destroy_sampler = c"vkDestroySampler": fn(VkDevice, VkSampler, *const c_void);
        create_descriptor_set_layout = c"vkCreateDescriptorSetLayout":
            fn(VkDevice, *const VkDescriptorSetLayoutCreateInfo, *const c_void, *mut VkDescriptorSetLayout) -> VkResult;
        destroy_descriptor_set_layout = c"vkDestroyDescriptorSetLayout":
            fn(VkDevice, VkDescriptorSetLayout, *const c_void);
        create_descriptor_pool = c"vkCreateDescriptorPool":
            fn(VkDevice, *const VkDescriptorPoolCreateInfo, *const c_void, *mut VkDescriptorPool) -> VkResult;
        destroy_descriptor_pool = c"vkDestroyDescriptorPool": fn(VkDevice, VkDescriptorPool, *const c_void);
        allocate_descriptor_sets = c"vkAllocateDescriptorSets":
            fn(VkDevice, *const VkDescriptorSetAllocateInfo, *mut VkDescriptorSet) -> VkResult;
        update_descriptor_sets = c"vkUpdateDescriptorSets":
            fn(VkDevice, u32, *const VkWriteDescriptorSet, u32, *const c_void);
        cmd_bind_descriptor_sets = c"vkCmdBindDescriptorSets":
            fn(VkCommandBuffer, i32, VkPipelineLayout, u32, u32, *const VkDescriptorSet, u32, *const u32);
        cmd_draw = c"vkCmdDraw": fn(VkCommandBuffer, u32, u32, u32, u32);
        cmd_set_depth_bias = c"vkCmdSetDepthBias": fn(VkCommandBuffer, f32, f32, f32);
        cmd_set_depth_compare_op = c"vkCmdSetDepthCompareOp": fn(VkCommandBuffer, i32);
        cmd_copy_buffer_to_image = c"vkCmdCopyBufferToImage":
            fn(VkCommandBuffer, VkBuffer, VkImage, VkImageLayout, u32, *const VkBufferImageCopy);
    }
}

fn_table! {
    /// `VK_KHR_swapchain` (device level).
    pub struct SwapchainFns {
        create_swapchain = c"vkCreateSwapchainKHR":
            fn(VkDevice, *const VkSwapchainCreateInfoKHR, *const c_void, *mut VkSwapchainKHR) -> VkResult;
        destroy_swapchain = c"vkDestroySwapchainKHR": fn(VkDevice, VkSwapchainKHR, *const c_void);
        get_swapchain_images = c"vkGetSwapchainImagesKHR":
            fn(VkDevice, VkSwapchainKHR, *mut u32, *mut VkImage) -> VkResult;
        acquire_next_image = c"vkAcquireNextImageKHR":
            fn(VkDevice, VkSwapchainKHR, u64, VkSemaphore, VkFence, *mut u32) -> VkResult;
        queue_present = c"vkQueuePresentKHR": fn(VkQueue, *const VkPresentInfoKHR) -> VkResult;
    }
}

/// A fixed-size, NUL-padded C string field as a Rust `String` (lossy, stops at the first NUL).
pub fn fixed_cstr(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    /// Expected (size, align) on 64-bit targets, from `sizeof`/`_Alignof` on the SDK 1.4.363
    /// headers (clang, x86_64).
    macro_rules! layout {
        ($($t:ty => $size:expr, $align:expr;)*) => {
            $(
                assert_eq!(size_of::<$t>(), $size, "size_of {}", stringify!($t));
                assert_eq!(align_of::<$t>(), $align, "align_of {}", stringify!($t));
            )*
        };
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn struct_layouts_match_vulkan_headers() {
        layout! {
            VkApplicationInfo => 48, 8;
            VkInstanceCreateInfo => 64, 8;
            VkLayerProperties => 520, 4;
            VkExtensionProperties => 260, 4;
            VkDebugUtilsMessengerCreateInfoEXT => 48, 8;
            VkDebugUtilsMessengerCallbackDataEXT => 96, 8;
            VkValidationFeaturesEXT => 48, 8;
            VkPhysicalDeviceProperties => 824, 8;
            VkPhysicalDeviceLimits => 504, 8;
            VkPhysicalDeviceSparseProperties => 20, 4;
            VkQueueFamilyProperties => 24, 4;
            VkExtent3D => 12, 4;
            VkPhysicalDeviceMemoryProperties => 520, 8;
            VkMemoryType => 8, 4;
            VkMemoryHeap => 16, 8;
            VkPhysicalDeviceFeatures => 220, 4;
            VkPhysicalDeviceFeatures2 => 240, 8;
            VkPhysicalDeviceVulkan13Features => 80, 8;
            VkPhysicalDeviceVulkan12Features => 208, 8;
            VkMemoryAllocateFlagsInfo => 24, 8;
            VkBufferDeviceAddressInfo => 24, 8;
            VkFormatProperties => 12, 4;
            VkDeviceQueueCreateInfo => 40, 8;
            VkDeviceCreateInfo => 72, 8;
            VkBufferCreateInfo => 56, 8;
            VkMemoryRequirements => 24, 8;
            VkMemoryAllocateInfo => 32, 8;
            VkImageCreateInfo => 88, 8;
            VkImageViewCreateInfo => 80, 8;
            VkComponentMapping => 16, 4;
            VkImageSubresourceRange => 20, 4;
            VkShaderModuleCreateInfo => 40, 8;
            VkPushConstantRange => 12, 4;
            VkPipelineLayoutCreateInfo => 48, 8;
            VkGraphicsPipelineCreateInfo => 144, 8;
            VkPipelineShaderStageCreateInfo => 48, 8;
            VkVertexInputBindingDescription => 12, 4;
            VkVertexInputAttributeDescription => 16, 4;
            VkPipelineVertexInputStateCreateInfo => 48, 8;
            VkPipelineInputAssemblyStateCreateInfo => 32, 8;
            VkPipelineViewportStateCreateInfo => 48, 8;
            VkPipelineRasterizationStateCreateInfo => 64, 8;
            VkPipelineMultisampleStateCreateInfo => 48, 8;
            VkStencilOpState => 28, 4;
            VkPipelineDepthStencilStateCreateInfo => 104, 8;
            VkPipelineColorBlendAttachmentState => 32, 4;
            VkPipelineColorBlendStateCreateInfo => 56, 8;
            VkPipelineDynamicStateCreateInfo => 32, 8;
            VkPipelineRenderingCreateInfo => 40, 8;
            VkCommandPoolCreateInfo => 24, 8;
            VkCommandBufferAllocateInfo => 32, 8;
            VkCommandBufferBeginInfo => 32, 8;
            VkFenceCreateInfo => 24, 8;
            VkSemaphoreCreateInfo => 24, 8;
            VkSubmitInfo2 => 64, 8;
            VkCommandBufferSubmitInfo => 32, 8;
            VkSemaphoreSubmitInfo => 48, 8;
            VkDependencyInfo => 64, 8;
            VkBufferMemoryBarrier2 => 80, 8;
            VkImageMemoryBarrier2 => 96, 8;
            VkRenderingInfo => 72, 8;
            VkRenderingAttachmentInfo => 72, 8;
            VkClearValue => 16, 4;
            VkRect2D => 16, 4;
            VkOffset2D => 8, 4;
            VkExtent2D => 8, 4;
            VkViewport => 24, 4;
            VkBufferCopy => 24, 8;
            VkBufferImageCopy => 56, 8;
            VkImageSubresourceLayers => 16, 4;
            VkOffset3D => 12, 4;
            VkWin32SurfaceCreateInfoKHR => 40, 8;
            VkSurfaceCapabilitiesKHR => 52, 4;
            VkSurfaceFormatKHR => 8, 4;
            VkSwapchainCreateInfoKHR => 104, 8;
            VkPresentInfoKHR => 64, 8;
            VkPhysicalDeviceMemoryProperties2 => 536, 8;
            VkPhysicalDeviceMemoryBudgetPropertiesEXT => 272, 8;
            VkQueryPoolCreateInfo => 32, 8;
            VkPipelineCacheCreateInfo => 40, 8;
            VkSamplerCreateInfo => 80, 8;
            VkDescriptorSetLayoutBinding => 24, 8;
            VkDescriptorSetLayoutBindingFlagsCreateInfo => 32, 8;
            VkDescriptorSetLayoutCreateInfo => 32, 8;
            VkDescriptorPoolSize => 8, 4;
            VkDescriptorPoolCreateInfo => 40, 8;
            VkDescriptorSetAllocateInfo => 40, 8;
            VkDescriptorImageInfo => 24, 8;
            VkWriteDescriptorSet => 64, 8;
            VkSpecializationMapEntry => 16, 8;
            VkSpecializationInfo => 32, 8;
        }
    }

    #[test]
    fn limits_words_read_the_right_bytes() {
        let mut limits = VkPhysicalDeviceLimits { max_image_dimension_1d: 0, max_image_dimension_2d: 0, rest: [0; 62] };
        // Write through a byte view, as the driver would, then read back by C offset.
        // SAFETY: `limits` is plain data of exactly 504 bytes; the view does not outlive it.
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(&mut limits as *mut VkPhysicalDeviceLimits as *mut u8, size_of::<VkPhysicalDeviceLimits>())
        };
        bytes[424..428].copy_from_slice(&83.333f32.to_ne_bytes());
        bytes[32..36].copy_from_slice(&256u32.to_ne_bytes());
        bytes[260..264].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert_eq!(limits.timestamp_period(), 83.333);
        assert_eq!(limits.word(VkPhysicalDeviceLimits::MAX_PUSH_CONSTANTS_SIZE), 256);
        assert_eq!(limits.word(VkPhysicalDeviceLimits::MAX_DRAW_INDEXED_INDEX_VALUE), u32::MAX);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn field_offsets_match_vulkan_headers() {
        assert_eq!(offset_of!(VkPhysicalDeviceProperties, limits), 296);
        assert_eq!(offset_of!(VkPhysicalDeviceProperties, sparse_properties), 800);
        assert_eq!(offset_of!(VkPhysicalDeviceMemoryProperties, memory_heap_count), 260);
        assert_eq!(offset_of!(VkPhysicalDeviceMemoryProperties, memory_heaps), 264);
        assert_eq!(offset_of!(VkPhysicalDeviceVulkan13Features, synchronization2), 52);
        // bufferDeviceAddress sits at offset 168 in VkPhysicalDeviceVulkan12Features.
        assert_eq!(offset_of!(VkPhysicalDeviceVulkan12Features, flags) + 4 * VkPhysicalDeviceVulkan12Features::BUFFER_DEVICE_ADDRESS, 168);
        assert_eq!(offset_of!(VkPhysicalDeviceVulkan13Features, dynamic_rendering), 64);
        // Descriptor indexing members (vulkan_core.h offsets).
        let at = |i: usize| offset_of!(VkPhysicalDeviceVulkan12Features, flags) + 4 * i;
        assert_eq!(at(VkPhysicalDeviceVulkan12Features::SHADER_SAMPLED_IMAGE_ARRAY_NON_UNIFORM_INDEXING), 72);
        assert_eq!(at(VkPhysicalDeviceVulkan12Features::DESCRIPTOR_BINDING_SAMPLED_IMAGE_UPDATE_AFTER_BIND), 100);
        assert_eq!(at(VkPhysicalDeviceVulkan12Features::DESCRIPTOR_BINDING_UPDATE_UNUSED_WHILE_PENDING), 120);
        assert_eq!(at(VkPhysicalDeviceVulkan12Features::DESCRIPTOR_BINDING_PARTIALLY_BOUND), 124);
        assert_eq!(at(VkPhysicalDeviceVulkan12Features::RUNTIME_DESCRIPTOR_ARRAY), 132);
        assert_eq!(offset_of!(VkWriteDescriptorSet, p_image_info), 40);
        assert_eq!(offset_of!(VkSamplerCreateInfo, compare_enable), 56);
        assert_eq!(offset_of!(VkRenderingAttachmentInfo, clear_value), 52);
    }

    #[test]
    fn api_version_packing() {
        assert_eq!(VK_API_VERSION_1_3, 4_206_592);
        assert_eq!(api_version_parts(make_api_version(1, 4, 341)), (1, 4, 341));
    }

    #[test]
    fn fixed_cstr_stops_at_nul() {
        let mut buf = [0 as c_char; 8];
        for (i, b) in b"abc".iter().enumerate() {
            buf[i] = *b as c_char;
        }
        assert_eq!(fixed_cstr(&buf), "abc");
    }
}
