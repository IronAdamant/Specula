//! Frame statistics: what the GPU layer did for a frame and what it holds, as plain data.
//!
//! Everything here is a number, a flag or an enum, so agents read budgets from data rather than
//! from log text. See [`crate::Gpu::last_frame_stats`].

/// Name of the main pass: depth clear plus every draw of the frame (and the sky).
pub const PASS_SCENE: &str = "scene";
/// The sun's shadow map: every cascade's depth-only draws.
pub const PASS_SHADOW: &str = "shadow";
/// Bloom: bright-pass downsampling and the upsampling blur chain.
pub const PASS_BLOOM: &str = "post_bloom";
/// HDR → display: bloom added back, exposure already applied, ACES and dithering.
pub const PASS_TONEMAP: &str = "post_tonemap";
/// FXAA anti-aliasing.
pub const PASS_FXAA: &str = "post_fxaa";
/// Overlay quads. Drawn inside the last pass's rendering, so that pass's time includes it.
pub const PASS_OVERLAY: &str = "overlay";

/// Which entry point produced a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FramePath {
    /// [`crate::Gpu::render`] into an offscreen target (synchronous).
    #[default]
    Offscreen,
    /// [`crate::Gpu::present`] to the window (up to two frames in flight).
    Window,
}

/// GPU time of one pass, from timestamp queries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassTiming {
    pub name: &'static str,
    pub gpu_ms: f64,
}

/// GPU timings of one completed frame.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuTiming {
    /// The [`FrameStats::frame`] these timings belong to. For window frames this lags the frame
    /// being reported (results are read only once a frame's fence has signalled, never by
    /// stalling), so compare the two numbers.
    pub frame: u64,
    /// First pass start to last pass end.
    pub total_ms: f64,
    pub passes: Vec<PassTiming>,
}

/// CPU time spent on a frame, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTiming {
    /// Blocked on the GPU: waiting for a frame slot's fence and acquiring a swapchain image
    /// (vsync shows up here), or, offscreen, waiting for the synchronous render to finish.
    pub wait_ms: f64,
    /// Culling, batching, writing per-draw data and recording commands.
    pub record_ms: f64,
    /// Queue submit (plus present for window frames).
    pub submit_ms: f64,
}

/// What was drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStats {
    /// Draw items in the [`crate::FrameDesc`].
    pub submitted: u32,
    /// Items skipped because their bounding sphere lies outside the view frustum.
    pub culled: u32,
    /// `vkCmdDrawIndexed` calls (instanced batches).
    pub draw_calls: u32,
    /// Instances across all draw calls. Glass counts twice (back faces, then front faces).
    pub instances: u32,
    /// Triangles across all draw calls.
    pub triangles: u64,
    /// Pipeline switches while recording.
    pub pipeline_binds: u32,
    /// Draw calls the pre-instancing renderer would have issued for the same visible items
    /// (one per item, two per glass item).
    pub unbatched_draw_calls: u32,
    /// Shadow cascades rendered (0 = shadows off or nothing visible to shadow).
    pub shadow_cascades: u32,
    /// Depth-only draw calls across all cascades.
    pub shadow_draw_calls: u32,
    /// Caster instances across all cascades (a caster spanning two cascades counts twice).
    pub shadow_instances: u32,
    /// Triangles drawn into the shadow map.
    pub shadow_triangles: u64,
    /// Overlay quads drawn (one instanced draw call, not counted in `draw_calls`).
    pub overlay_quads: u32,
}

/// Why a pipeline cache blob given in [`crate::GpuOptions::pipeline_cache`] was not used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheRejection {
    /// Shorter than the 32-byte header.
    TooShort { len: usize },
    /// Not a version-one header, or its size field is inconsistent.
    BadHeader { header_size: u32, header_version: u32 },
    /// Made on another GPU vendor.
    OtherVendor { vendor_id: u32 },
    /// Made on another device of the same vendor.
    OtherDevice { device_id: u32 },
    /// Made by another driver build (`pipelineCacheUUID` differs).
    OtherDriver,
    /// The driver refused the data (`vkCreatePipelineCache` failed with it).
    Driver { vk_result: i32 },
}

/// What happened to the pipeline cache blob at start-up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PipelineCacheLoad {
    /// No blob was given; the cache started empty.
    #[default]
    NotProvided,
    /// The blob seeded the cache.
    Loaded { bytes: usize },
    /// The blob was ignored; the cache started empty.
    Rejected(CacheRejection),
}

/// Pipeline creation so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PipelineStats {
    /// Graphics pipelines created since start-up.
    pub created: u32,
    /// Pipelines created after [`crate::Gpu::mark_warm`]: each one is a hitch the player can see,
    /// so a non-zero value is a budget error (F3). Always 0 before `mark_warm`.
    pub created_after_warm: u32,
    /// `mark_warm` has been called.
    pub warm: bool,
    /// Total time spent in `vkCreateGraphicsPipelines` (on the engine thread).
    pub create_ms: f64,
    pub cache: PipelineCacheLoad,
    /// Pipelines created on a worker thread (skinned and textured variants asked for by the
    /// first upload with those streams): no frame waits for them, so they are not budget
    /// errors. Not counted in `created`.
    pub created_background: u32,
    /// Wall time of those background builds.
    pub background_ms: f64,
    /// A background build is running (draws needing it are skipped meanwhile).
    pub background_pending: bool,
    /// Pipelines a background build failed to create.
    pub background_failed: u32,
}

/// What a block of memory held by Specula's allocator is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryKind {
    /// Vertex and index buffers.
    Meshes,
    /// Offscreen colour and depth images, and the window's depth image.
    RenderTargets,
    /// Host-readable copies of offscreen targets.
    Readback,
    /// Per-frame uploads: frame data and per-draw records.
    PerFrame,
    /// Transient upload buffers (freed right after use).
    Staging,
    /// The sun's shadow map.
    ShadowMaps,
    /// Intermediate images of the post chain (HDR scene, bloom levels, anti-aliasing input).
    PostTargets,
    /// Textures uploaded with `Gpu::upload_texture`.
    Textures,
}

impl MemoryKind {
    pub const ALL: [MemoryKind; 8] = [
        MemoryKind::Meshes,
        MemoryKind::RenderTargets,
        MemoryKind::Readback,
        MemoryKind::PerFrame,
        MemoryKind::Staging,
        MemoryKind::ShadowMaps,
        MemoryKind::PostTargets,
        MemoryKind::Textures,
    ];

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// One Vulkan memory heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapStats {
    pub index: u32,
    pub size: u64,
    pub device_local: bool,
    /// How much this process may use before the driver starts paging (`VK_EXT_memory_budget`;
    /// `None` when the device lacks the extension).
    pub budget: Option<u64>,
    /// How much this process uses now (`VK_EXT_memory_budget`).
    pub usage: Option<u64>,
}

/// GPU memory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryStats {
    /// Bytes our allocator holds, per kind, in [`MemoryKind::ALL`] order.
    pub by_kind: Vec<(MemoryKind, u64)>,
    /// Sum of `by_kind`.
    pub total_bytes: u64,
    /// Live `vkAllocateMemory` allocations.
    pub allocations: u32,
    pub heaps: Vec<HeapStats>,
}

impl MemoryStats {
    pub fn bytes(&self, kind: MemoryKind) -> u64 {
        self.by_kind.iter().find(|(k, _)| *k == kind).map_or(0, |(_, b)| *b)
    }
}

/// The frame's render graph (see [`crate::Gpu::last_graph`] for its passes and barriers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphStats {
    /// Passes in the graph (inline passes included).
    pub passes: u32,
    /// Image and buffer barriers recorded for the frame (batched into one
    /// `vkCmdPipelineBarrier2` per pass at most).
    pub barriers: u32,
    /// The graph was compiled for this frame (its passes, formats or sizes changed); otherwise
    /// the previous frame's plan was reused.
    pub recompiled: bool,
    /// Compilations of this path's graph so far.
    pub compiles: u64,
}

/// Statistics for the most recent frame. Frame-specific fields describe that frame; `pipelines`
/// and `memory` are the state when the stats were taken.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// 1 for the first frame (offscreen and window frames share the counter); 0 = none yet.
    pub frame: u64,
    pub path: FramePath,
    pub width: u32,
    pub height: u32,
    pub cpu: CpuTiming,
    /// Latest completed GPU timings; `None` before any are available or when the queue has no
    /// timestamp support (`timestamps_supported`).
    pub gpu: Option<GpuTiming>,
    pub timestamps_supported: bool,
    /// `VkResult` of the last failed timestamp read on this path (e.g. `VK_ERROR_DEVICE_LOST`
    /// = −4); `None` while reads succeed. `gpu` then keeps the last good timings.
    pub timestamp_error: Option<i32>,
    pub draws: DrawStats,
    pub graph: GraphStats,
    /// Size of the per-frame buffer holding frame data and per-draw records; it grows as needed.
    pub draw_buffer_bytes: u64,
    pub pipelines: PipelineStats,
    pub memory: MemoryStats,
}
