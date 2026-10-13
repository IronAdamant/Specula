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
//! # Rendering contract
//!
//! - Matrices are column-major `[f32; 16]` in Vulkan clip conventions (y down, depth 0..1) — build
//!   them with `specula_core::math` (`perspective_vk`, `look_at_rh`, `from_trs`).
//! - Triangle lists of [`Vertex`] (position, normal, sRGB vertex colour multiplied with the draw
//!   colour; white is neutral), D32 depth test (LESS with clear 1, or GREATER with clear 0 when
//!   [`FrameDesc::reverse_z`] is set — use `perspective_vk_reverse_z`).
//! - Opaque draws ([`Material::Lit`]) come first, with no face culling. Blended draws (every other
//!   material) follow, sorted far to near, with depth test on and depth writes off.
//! - With the default [`Environment`] and [`Surface`], a `Lit` draw is shaded exactly as in M0:
//!   `lit = color * (0.25 + 0.75 * max(dot(n, l), 0))`, light direction ≈ normalize(0.4, 0.8, 0.45).
//! - Per-frame data (camera, environment, lights, time) and one record per draw (model, colours,
//!   surface) reach the shaders through buffer device addresses; push constants carry only the
//!   two addresses, the material and the glass pass.
//!
//! # Performance
//!
//! - **Culling:** a draw whose mesh's bounding sphere lies outside the view frustum is skipped
//!   (standard or reversed Z, finite or infinite far plane).
//! - **Instancing:** opaque draws are sorted by mesh so equal ones merge into one instanced draw
//!   call; blended draws keep their far-to-near order and merge only when adjacent. Depth testing
//!   makes the opaque order irrelevant except where two opaque surfaces are exactly coplanar.
//! - **Pipelines** are all built up front: every variant for the offscreen format at
//!   [`Gpu::new`] and for the window's format at [`Gpu::attach_window`]. A `VkPipelineCache` can be
//!   persisted ([`Gpu::pipeline_cache_data`]) and fed back ([`GpuOptions::pipeline_cache`]).
//!   Call [`Gpu::mark_warm`] after loading; a pipeline created later is counted as a budget error
//!   ([`PipelineStats::created_after_warm`]).
//! - **Statistics:** [`Gpu::last_frame_stats`] — GPU time per pass (timestamp queries, read without
//!   stalling), CPU time, draw calls, instances, triangles, culled draws, pipelines and memory.
//! - Colours given to the API are **sRGB** (as picked by humans/agents). Offscreen targets are
//!   `R8G8B8A8_SRGB`; the swapchain prefers `B8G8R8A8_SRGB`. Readback bytes are therefore sRGB RGBA8,
//!   ready to write as PNG.

use std::ffi::c_void;
use std::fmt;

mod batch;
mod descriptors;
mod device;
mod gpu;
mod instance;
mod loader;
mod render;
mod shadow;
mod stats;
mod swapchain;
mod timing;
mod vk;

pub use stats::*;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuOptions {
    pub validation: Validation,
    pub adapter: AdapterPreference,
    /// Bytes from an earlier [`Gpu::pipeline_cache_data`], to skip shader compilation. A blob
    /// from another GPU or driver is safely ignored; [`PipelineStats::cache`] says what happened.
    pub pipeline_cache: Option<Vec<u8>>,
}

impl Default for GpuOptions {
    fn default() -> Self {
        GpuOptions { validation: Validation::Auto, adapter: AdapterPreference::HighPerformance, pipeline_cache: None }
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
    /// Set when a request exceeded a fixed limit.
    pub limit: Option<LimitExceeded>,
}

/// A limit of the GPU layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// Point lights per frame ([`MAX_POINT_LIGHTS`]).
    PointLights,
    /// Visible draw records per frame (instance indices are `u32`).
    DrawRecords,
    /// Live textures plus post-chain images (slots in the bindless array).
    Textures,
    /// Overlay quads per frame ([`MAX_OVERLAY_QUADS`]).
    OverlayQuads,
}

/// What was asked for and what is allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitExceeded {
    pub what: Limit,
    pub requested: u64,
    pub max: u64,
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
        GpuError { message: message.into(), vk_result: None, limit: None }
    }

    /// `what` failed with `result`.
    pub(crate) fn vk(what: &str, result: i32) -> GpuError {
        GpuError { message: format!("{what} failed: {}", vk::result_name(result)), vk_result: Some(result), limit: None }
    }

    /// `requested` exceeds the limit `what` (at most `max`).
    pub(crate) fn limit(what: Limit, requested: u64, max: u64) -> GpuError {
        let name = match what {
            Limit::PointLights => "point lights per frame",
            Limit::DrawRecords => "visible draw records per frame",
            Limit::Textures => "textures (and post-chain images) alive at once",
            Limit::OverlayQuads => "overlay quads per frame",
        };
        GpuError {
            message: format!("{requested} {name} requested; at most {max}"),
            vk_result: None,
            limit: Some(LimitExceeded { what, requested, max }),
        }
    }
}

/// One mesh vertex (28 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// sRGB colour and alpha (0–255), multiplied with the draw's colour (and, for blended
    /// materials, its opacity). White ([`Vertex::WHITE`]) leaves the draw colour unchanged, bit
    /// for bit. The ray-marched `LavaLamp` ignores it.
    pub color: [u8; 4],
}

impl Vertex {
    /// The neutral vertex colour.
    pub const WHITE: [u8; 4] = [255; 4];

    /// A white vertex.
    pub const fn new(position: [f32; 3], normal: [f32; 3]) -> Vertex {
        Vertex { position, normal, color: Vertex::WHITE }
    }

    /// A vertex with an sRGB colour.
    pub const fn with_color(position: [f32; 3], normal: [f32; 3], color: [u8; 4]) -> Vertex {
        Vertex { position, normal, color }
    }
}

impl Default for Vertex {
    /// At the origin, zero normal, white.
    fn default() -> Self {
        Vertex::new([0.0; 3], [0.0; 3])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TargetId(pub u32);

/// How a draw is shaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Material {
    /// Opaque. Sun, ambient and point lights; optional emission, specular and metal.
    #[default]
    Lit,
    /// Blended thin glass or liquid shell: Fresnel reflection plus tinted, lit body of
    /// `color.a` opacity. Drawn twice, back faces then front faces.
    Glass,
    /// Blended soft dark disc on the XZ quad (a contact shadow); `color.a` is its darkness.
    BlobShadow,
    /// Ray-marched lava lamp inside the unit-cube proxy: glass vessel, liquid (`Surface::color2`)
    /// and moving wax (`color`), with a glow halo. Animated by [`FrameDesc::time_s`].
    LavaLamp,
}

impl Material {
    /// Drawn in the blended pass.
    pub fn blended(self) -> bool {
        self != Material::Lit
    }
}

/// Per-draw material parameters. The default gives M0's matte look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// Light emitted, as a multiple of `color` (0 = none). Values above 1 glow (HDR).
    pub emissive: f32,
    /// 0 = dielectric, 1 = metal (specular tinted by `color`, no diffuse).
    pub metallic: f32,
    /// 0 = mirror-sharp highlights, 1 = very broad.
    pub roughness: f32,
    /// Strength of highlights and reflections (0 = none, as in M0).
    pub specular: f32,
    /// Secondary sRGB colour (the lava lamp's liquid).
    pub color2: [f32; 3],
    /// Variation seed (the lava lamp's wax motion).
    pub seed: f32,
}

impl Default for Surface {
    fn default() -> Self {
        Surface { emissive: 0.0, metallic: 0.0, roughness: 0.5, specular: 0.0, color2: [0.0; 3], seed: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawItem {
    pub mesh: MeshId,
    /// Model (object → world) matrix, column-major.
    pub model: [f32; 16],
    /// sRGB colour; alpha is the opacity of blended materials (ignored by `Lit`).
    pub color: [f32; 4],
    pub material: Material,
    pub surface: Surface,
}

impl DrawItem {
    /// A matte (M0-style) draw.
    pub fn new(mesh: MeshId, model: [f32; 16], color: [f32; 4]) -> DrawItem {
        DrawItem { mesh, model, color, material: Material::Lit, surface: Surface::default() }
    }
}

/// A point light: inverse-square falloff, smoothly cut to zero at `range`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub position: [f32; 3],
    /// sRGB.
    pub color: [f32; 3],
    /// Radiant intensity: at 1 m an intensity of 1 lights a white surface facing it fully.
    pub intensity: f32,
    /// Metres; no light beyond it.
    pub range: f32,
}

/// At most this many point lights per frame.
pub const MAX_POINT_LIGHTS: usize = 16;

/// At most this many overlay quads per frame.
pub const MAX_OVERLAY_QUADS: usize = 65_536;

/// Most shadow cascades.
pub const MAX_SHADOW_CASCADES: u32 = 4;

/// The sun's (or moon's) shadow: a cascaded shadow map fitted to the view. Off by default.
///
/// Only opaque (`Lit`) draws cast; every material receives (the lava lamp's glass included).
/// Cascades split the visible depth range (from the nearest visible draw to `distance`, or the
/// farthest visible draw if nearer); each is a square fitted around its slice, snapped to whole
/// texels so it does not shimmer as the camera moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub enabled: bool,
    /// Texels per side of each cascade (clamped to 256..=8192).
    pub resolution: u32,
    /// 1..=[`MAX_SHADOW_CASCADES`]. More cascades keep near shadows sharp over a long distance.
    pub cascades: u32,
    /// View depth (m) where shadows end; they fade out over the last 10%.
    pub distance: f32,
    /// Penumbra radius in texels of the nearest cascade (0 = hard, hardware 2×2 filtering only).
    pub softness: f32,
    /// Scales the depth bias (constant and slope) and the receiver's offset along its normal.
    /// Raise it if surfaces shadow themselves ("acne"); lower it if shadows detach.
    pub bias: f32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        ShadowSettings { enabled: false, resolution: 2048, cascades: 3, distance: 80.0, softness: 1.5, bias: 1.0 }
    }
}

/// Scene-wide lighting. The default reproduces M0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// World direction towards the sun (normalized by the renderer).
    pub sun_direction: [f32; 3],
    /// sRGB.
    pub sun_color: [f32; 3],
    pub sun_intensity: f32,
    /// sRGB; also the colour reflections pick up from the surroundings.
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    /// Multiplies all light before display.
    pub exposure: f32,
    /// Filmic (ACES) tone mapping plus dithering; off = plain clamp, as in M0.
    pub tonemap: bool,
    /// Directional shadows from the sun (off by default).
    pub shadow: ShadowSettings,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            sun_direction: [0.4, 0.8, 0.45],
            sun_color: [1.0; 3],
            sun_intensity: 0.75,
            ambient_color: [1.0; 3],
            ambient_intensity: 0.25,
            exposure: 1.0,
            tonemap: false,
            shadow: ShadowSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameDesc<'a> {
    /// Projection × view, column-major, Vulkan clip conventions. The camera position used for
    /// highlights, reflections and ray marching is derived from it.
    pub view_proj: [f32; 16],
    /// sRGB clear colour.
    pub clear_color: [f32; 4],
    pub draws: &'a [DrawItem],
    /// `view_proj` uses reversed Z (depth 1 at the near plane, 0 at infinity): depth test GREATER,
    /// depth cleared to 0. `false`: standard depth 0..1, test LESS, cleared to 1.
    pub reverse_z: bool,
    pub environment: Environment,
    /// At most [`MAX_POINT_LIGHTS`].
    pub lights: &'a [PointLight],
    /// Seconds of animation time. Captures pass it explicitly, so a frame is reproducible.
    pub time_s: f32,
}

impl Default for FrameDesc<'_> {
    fn default() -> Self {
        FrameDesc {
            view_proj: [0.0; 16],
            clear_color: [0.0, 0.0, 0.0, 1.0],
            draws: &[],
            reverse_z: false,
            environment: Environment::default(),
            lights: &[],
            time_s: 0.0,
        }
    }
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

    /// Statistics of the most recent frame from [`Gpu::render`] or [`Gpu::present`] (whichever
    /// ran last), with the current pipeline and memory state. `frame == 0` before any frame.
    pub fn last_frame_stats(&self) -> FrameStats {
        self.inner.last_frame_stats(None)
    }

    /// Like [`Gpu::last_frame_stats`], but the latest frame of one path (`None` if it has none
    /// yet): the window's frame rate is not disturbed by agents' offscreen captures.
    pub fn last_frame_stats_of(&self, path: FramePath) -> Option<FrameStats> {
        let stats = self.inner.last_frame_stats(Some(path));
        (stats.frame != 0).then_some(stats)
    }

    /// Declare loading finished: every pipeline the game needs exists. Pipelines created after
    /// this are counted in [`PipelineStats::created_after_warm`] (a budget error: each is a hitch).
    pub fn mark_warm(&mut self) {
        self.inner.mark_warm()
    }

    /// The pipeline cache's contents, to save (e.g. at shutdown) and pass back through
    /// [`GpuOptions::pipeline_cache`] next time.
    pub fn pipeline_cache_data(&self) -> Result<Vec<u8>, GpuError> {
        self.inner.pipeline_cache_data()
    }
}
