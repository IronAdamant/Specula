//! Specula GPU layer: hand-written Vulkan bindings and the render-hardware interface.
//!
//! - `vulkan-1` is loaded **at runtime** (Windows: `LoadLibraryW` + `GetProcAddress` from kernel32 via
//!   raw-dylib; later `dlopen` on Linux/macOS). All other entry points come through
//!   `vkGetInstanceProcAddr` / `vkGetDeviceProcAddr`. A machine without Vulkan gets a clean
//!   `GpuError`, never a failed process start.
//! - Validation layers (`VK_LAYER_KHRONOS_validation`) are enabled when requested and present;
//!   their messages are captured for agents via [`Gpu::take_validation_messages`].
//! - Everything works **headless** (offscreen targets + readback); a window swapchain is optional.
//!
//! # Rendering contract (M0)
//!
//! - Matrices are column-major `[f32; 16]` in Vulkan clip conventions (y down, depth 0..1) — build
//!   them with `specula_core::math` (`perspective_vk`, `look_at_rh`, `from_trs`).
//! - One pipeline: triangle lists of [`Vertex`], depth test (less, D32), **no face culling**, a fixed
//!   directional light (world direction towards the light ≈ normalize(0.4, 0.8, 0.45)) with
//!   `lit = color * (0.25 + 0.75 * max(dot(n, l), 0))`.
//! - Colours given to the API are **sRGB** (as picked by humans/agents). Offscreen targets are
//!   `R8G8B8A8_SRGB`; the swapchain prefers `B8G8R8A8_SRGB`. Readback bytes are therefore sRGB RGBA8,
//!   ready to write as PNG.

use std::ffi::c_void;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    Off,
    On,
    /// On when the validation layer is installed, off otherwise.
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterPreference {
    /// Discrete GPU first.
    HighPerformance,
    /// Integrated GPU first.
    LowPower,
    /// Index into [`enumerate_adapters`].
    Index(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuOptions {
    pub validation: Validation,
    pub adapter: AdapterPreference,
}

impl Default for GpuOptions {
    fn default() -> Self {
        GpuOptions { validation: Validation::Auto, adapter: AdapterPreference::HighPerformance }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterInfo {
    pub index: usize,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub kind: AdapterKind,
    /// Vulkan API version supported by the device (major, minor, patch).
    pub api_version: (u32, u32, u32),
    /// Raw vendor-specific driver version.
    pub driver_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Verbose,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationMessage {
    pub severity: Severity,
    /// e.g. `VUID-vkCmdDraw-None-02699`.
    pub id_name: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuError {
    pub message: String,
    /// The failing `VkResult`, when a Vulkan call failed.
    pub vk_result: Option<i32>,
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.vk_result {
            Some(r) => write!(f, "{} (VkResult {r})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for GpuError {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TargetId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawItem {
    pub mesh: MeshId,
    /// Model (object → world) matrix, column-major.
    pub model: [f32; 16],
    /// sRGB colour, alpha currently ignored (opaque).
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct FrameDesc<'a> {
    /// Projection × view, column-major, Vulkan clip conventions.
    pub view_proj: [f32; 16],
    /// sRGB clear colour.
    pub clear_color: [f32; 4],
    pub draws: &'a [DrawItem],
}

/// True if the Vulkan loader can be loaded and an instance created.
pub fn is_available() -> bool {
    todo!("specula-gpu: is_available")
}

/// All physical devices, in Vulkan enumeration order (`index` matches [`AdapterPreference::Index`]).
pub fn enumerate_adapters() -> Result<Vec<AdapterInfo>, GpuError> {
    todo!("specula-gpu: enumerate_adapters")
}

/// Instance, device, queue and all GPU resources. Not `Send`; use from one thread.
pub struct Gpu {
    _not_send: std::marker::PhantomData<*mut c_void>,
}

impl Gpu {
    pub fn new(options: &GpuOptions) -> Result<Gpu, GpuError> {
        let _ = options;
        todo!("specula-gpu: Gpu::new")
    }

    pub fn adapter(&self) -> &AdapterInfo {
        todo!("specula-gpu: Gpu::adapter")
    }

    pub fn validation_enabled(&self) -> bool {
        todo!("specula-gpu: Gpu::validation_enabled")
    }

    /// Drain validation messages captured since the last call.
    pub fn take_validation_messages(&mut self) -> Vec<ValidationMessage> {
        todo!("specula-gpu: Gpu::take_validation_messages")
    }

    /// Upload an indexed triangle list (device-local buffers).
    pub fn upload_mesh(&mut self, vertices: &[Vertex], indices: &[u32]) -> Result<MeshId, GpuError> {
        let _ = (vertices, indices);
        todo!("specula-gpu: Gpu::upload_mesh")
    }

    pub fn destroy_mesh(&mut self, mesh: MeshId) {
        let _ = mesh;
        todo!("specula-gpu: Gpu::destroy_mesh")
    }

    /// Offscreen colour (`R8G8B8A8_SRGB`) + depth target.
    pub fn create_target(&mut self, width: u32, height: u32) -> Result<TargetId, GpuError> {
        let _ = (width, height);
        todo!("specula-gpu: Gpu::create_target")
    }

    pub fn destroy_target(&mut self, target: TargetId) {
        let _ = target;
        todo!("specula-gpu: Gpu::destroy_target")
    }

    /// Render a frame into an offscreen target. Completes before returning (M0: simple, synchronous).
    pub fn render(&mut self, target: TargetId, frame: &FrameDesc) -> Result<(), GpuError> {
        let _ = (target, frame);
        todo!("specula-gpu: Gpu::render")
    }

    /// Read back the target as tightly packed sRGB RGBA8, top row first.
    pub fn read_rgba8(&mut self, target: TargetId) -> Result<Vec<u8>, GpuError> {
        let _ = target;
        todo!("specula-gpu: Gpu::read_rgba8")
    }

    /// Create a surface + swapchain for a native window (Windows: `HINSTANCE`, `HWND`).
    pub fn attach_window(
        &mut self,
        hinstance: *mut c_void,
        hwnd: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        let _ = (hinstance, hwnd, width, height);
        todo!("specula-gpu: Gpu::attach_window")
    }

    /// Recreate the swapchain for a new client size. 0×0 (minimized) is allowed; presenting is skipped.
    pub fn resize_window(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        let _ = (width, height);
        todo!("specula-gpu: Gpu::resize_window")
    }

    /// Render a frame to the window and present it. Recreates an out-of-date swapchain transparently.
    pub fn present(&mut self, frame: &FrameDesc) -> Result<(), GpuError> {
        let _ = frame;
        todo!("specula-gpu: Gpu::present")
    }
}
