//! Specula GPU layer: hand-written Vulkan bindings and the render-hardware interface.
//!
//! - `vulkan-1` is loaded **at runtime** through `specula_sys::dynlib` (Windows: `LoadLibraryW` +
//!   `GetProcAddress`; later `dlopen` on Linux/macOS). All other entry points come through
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

mod device;
mod gpu;
mod instance;
mod loader;
mod render;
mod swapchain;
mod vk;

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

impl GpuError {
    pub(crate) fn new(message: impl Into<String>) -> GpuError {
        GpuError { message: message.into(), vk_result: None }
    }

    /// `what` failed with `result`.
    pub(crate) fn vk(what: &str, result: i32) -> GpuError {
        GpuError { message: format!("{what} failed: {}", vk::result_name(result)), vk_result: Some(result) }
    }
}

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
    instance::Instance::new(Validation::Off, false).is_ok()
}

/// All physical devices, in Vulkan enumeration order (`index` matches [`AdapterPreference::Index`]).
pub fn enumerate_adapters() -> Result<Vec<AdapterInfo>, GpuError> {
    let instance = instance::Instance::new(Validation::Off, false)?;
    let devices = instance.physical_devices()?;
    Ok(devices.iter().enumerate().map(|(i, &pd)| instance.adapter_info(i, pd)).collect())
}

/// Instance, device, queue and all GPU resources. Not `Send`; use from one thread.
///
/// Dropping it waits for the GPU to go idle, then releases every resource, the window swapchain
/// and surface, the device and the instance, in that order.
pub struct Gpu {
    inner: gpu::GpuInner,
    _not_send: std::marker::PhantomData<*mut c_void>,
}

impl Gpu {
    /// Create the instance (validation per `options`), pick an adapter that supports Vulkan 1.3
    /// with dynamic rendering + synchronization2 and a graphics queue, and create the device.
    ///
    /// `Validation::On` fails if `VK_LAYER_KHRONOS_validation` (or `VK_EXT_debug_utils`) is
    /// missing; `Validation::Auto` silently runs without it. Enabled validation includes
    /// synchronization validation.
    pub fn new(options: &GpuOptions) -> Result<Gpu, GpuError> {
        Ok(Gpu { inner: gpu::GpuInner::new(options)?, _not_send: std::marker::PhantomData })
    }

    pub fn adapter(&self) -> &AdapterInfo {
        self.inner.adapter()
    }

    pub fn validation_enabled(&self) -> bool {
        self.inner.validation_enabled()
    }

    /// Drain validation messages captured since the last call (warnings and errors; at most the
    /// newest 4096 are kept).
    pub fn take_validation_messages(&mut self) -> Vec<ValidationMessage> {
        self.inner.take_validation_messages()
    }

    /// Release everything (exactly like dropping the `Gpu`) and return the validation messages
    /// not yet taken, including any produced while destroying resources, the device and the
    /// instance. Use it to check that shutdown is clean.
    pub fn close(self) -> Vec<ValidationMessage> {
        self.inner.close()
    }

    /// Upload an indexed triangle list (device-local buffers).
    ///
    /// Errors if there are no vertices/indices, the index count is not a multiple of 3, or an
    /// index is out of range.
    pub fn upload_mesh(&mut self, vertices: &[Vertex], indices: &[u32]) -> Result<MeshId, GpuError> {
        self.inner.upload_mesh(vertices, indices)
    }

    /// Unknown ids are ignored. Waits for in-flight window frames that may still use the mesh.
    pub fn destroy_mesh(&mut self, mesh: MeshId) {
        self.inner.destroy_mesh(mesh)
    }

    /// Offscreen colour (`R8G8B8A8_SRGB`) + depth target.
    pub fn create_target(&mut self, width: u32, height: u32) -> Result<TargetId, GpuError> {
        self.inner.create_target(width, height)
    }

    /// Unknown ids are ignored.
    pub fn destroy_target(&mut self, target: TargetId) {
        self.inner.destroy_target(target)
    }

    /// Render a frame into an offscreen target. Completes before returning (M0: simple, synchronous).
    pub fn render(&mut self, target: TargetId, frame: &FrameDesc) -> Result<(), GpuError> {
        self.inner.render(target, frame)
    }

    /// Read back the target as tightly packed sRGB RGBA8, top row first.
    ///
    /// Errors if the target has never been rendered.
    pub fn read_rgba8(&mut self, target: TargetId) -> Result<Vec<u8>, GpuError> {
        self.inner.read_rgba8(target)
    }

    /// Create a surface + swapchain for a native window (Windows: `HINSTANCE`, `HWND`).
    ///
    /// Replaces a previously attached window. Fails if the instance has no surface support or
    /// the adapter's graphics queue cannot present to this window.
    pub fn attach_window(
        &mut self,
        hinstance: *mut c_void,
        hwnd: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        self.inner.attach_window(hinstance, hwnd, width, height)
    }

    /// Recreate the swapchain for a new client size. 0×0 (minimized) is allowed; presenting is skipped.
    pub fn resize_window(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        self.inner.resize_window(width, height)
    }

    /// Render a frame to the window and present it. Recreates an out-of-date swapchain transparently.
    ///
    /// FIFO presentation: blocks for vsync when the swapchain is full. Up to two frames are in
    /// flight; a frame that hits an out-of-date swapchain is skipped (the next one rebuilds it).
    pub fn present(&mut self, frame: &FrameDesc) -> Result<(), GpuError> {
        self.inner.present(frame)
    }
}
