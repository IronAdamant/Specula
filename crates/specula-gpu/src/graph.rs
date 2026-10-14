//! The render graph: passes declare the images and buffers they use and how; the graph computes
//! every barrier and layout transition between them, picks attachment load/store ops, and owns the
//! transient images (post-chain targets, depth buffers, shadow maps).
//!
//! It is deliberately small and explicit. Passes run **in the order they are declared** (no
//! reordering, no culling); the graph only works out what must happen between them. Each frame the
//! renderer declares the frame's passes again into reused storage; when the declaration equals the
//! last one (same passes, accesses, formats and sizes) the compiled plan is reused, so a warm frame
//! neither allocates nor recompiles.
//!
//! # A frame, step by step
//!
//! ```text
//! // `completed`: the newest pool submission number known finished on the GPU (0 at start).
//! pool.collect(dev, descriptors, completed);       // free replaced / stale images no frame uses
//! g.begin();                                       // forget last frame's declaration (keeps storage)
//! let out = g.import_image("target", fmt, w, h, Prior::Discard { after: COPY }, Final::TransferSrc);
//! let depth = g.transient("depth", ImageDesc::depth(DEPTH_FORMAT, w, h, MemoryKind::RenderTargets));
//! g.pass("scene", FramePass::Scene).color(out, Load::Clear).depth(depth, Load::Clear);
//! g.prepare(dev, descriptors, pool)?;              // compile if changed, find/create transients
//! g.bind_image(out, image, view);                  // imported handles, every frame, after prepare
//! for p in 0..g.pass_count() {                     // (record into cmd)
//!     g.barriers(dev, cmd, p);                     // everything pass p needs, in one vkCmdPipelineBarrier2
//!     match g.tag(p) { FramePass::Scene => record_scene(…), … }
//! }
//! g.finish(dev, cmd);                              // final transitions (present, readback)
//! // submit cmd, then:
//! let n = pool.note_used(&g);                      // this submission's number
//! // once its fence signals (offscreen: right away; window: when the slot is reused):
//! completed = completed.max(n);
//! ```
//!
//! No `collect` may run between `prepare` and `note_used` (the frame's images must stay alive).
//! The frame code in `render.rs` (`declare_frame`, `prepare_graph`, `record_frame`) and its two
//! callers (`GpuInner::render`, `Window::present`) follow exactly this.
//!
//! # Accesses
//!
//! | `Access`              | stage                    | layout                 | reads old contents |
//! |-----------------------|--------------------------|------------------------|--------------------|
//! | `Color(load)`         | colour attachment output | `COLOR_ATTACHMENT`     | only `Load::Keep`  |
//! | `Depth(load)`         | early + late tests       | `DEPTH_ATTACHMENT`     | only `Load::Keep`  |
//! | `DepthRead`           | early + late tests       | `DEPTH_ATTACHMENT`     | yes (test only)    |
//! | `Sampled`             | fragment shader          | `SHADER_READ_ONLY`     | yes                |
//! | `StorageRead`/`Write` | compute shader           | `GENERAL`              | read / no          |
//! | `TransferSrc`/`Dst`   | copy                     | `TRANSFER_SRC` / `DST` | yes / no           |
//!
//! Writes that don't read (`Load::Clear`, `Load::DontCare`, `TransferDst`, `StorageWrite`)
//! discard the old contents, so their transition starts from `UNDEFINED` (free on tile GPUs).
//! An attachment is stored (`STORE`) only if a later pass reads it or it is imported with a final
//! use that needs it; otherwise its store op is `DONT_CARE` (a tile GPU never writes it out).
//!
//! # Mistakes are structured errors
//!
//! `prepare` validates a changed declaration and fails with [`GraphError`] (inside
//! `GpuError::reason`) instead of recording a hazard: reading what nothing wrote this frame,
//! overwriting contents nobody read, attachments of different sizes in one pass, a resource used
//! twice by one pass, an access that doesn't fit the resource, an inline pass that would need a
//! barrier, a presented or read-back image nothing wrote.
//!
//! # Inline passes
//!
//! A pass marked `.inline()` records inside the previous pass's rendering instance (the overlay
//! draws over the final image without storing and reloading it). It may only use attachments the
//! previous pass also renders to, in the same layout; the graph checks that no barrier is needed.
//!
//! # Transients
//!
//! [`TransientPool`] holds the images the graph owns, keyed by owner (each offscreen target and the
//! window have their own; shared ones such as the shadow map have [`SHARED_OWNER`]), name and
//! description (format, size, layers, usage — usage is derived from the declared accesses). A
//! changed size or format makes a new image; the old one is destroyed once no frame in flight can
//! use it. Every transient is written before it is read in each frame, so its first barrier
//! discards; the barrier waits for the image's last use in an earlier frame (tracked by the pool,
//! across graphs).
//!
//! # Adding a pass (the recipe; example: an "id_buffer" pass for sight occlusion)
//!
//! The pass renders entity ids into an `R32_UINT` image, depth-tested against the scene's depth,
//! then copies it into a host-visible buffer for sight to read after the frame's fence.
//!
//! 1. Give it a tag: add `IdBuffer` and `IdCopy` to `FramePass` (`render.rs`) and a timing name in
//!    `stats.rs` (`PASS_ID_BUFFER = "id_buffer"`).
//! 2. Declare it in `declare_frame` (`render.rs`), after the scene pass:
//!
//!    ```text
//!    let ids = g.transient("ids", ImageDesc::color(VK_FORMAT_R32_UINT, w, h, MemoryKind::RenderTargets));
//!    g.pass("id_buffer", FramePass::IdBuffer).color(ids, Load::Clear).depth_read(res.depth);
//!    let readback = g.import_buffer("id_readback", bytes, Prior::Discard { after: VK_PIPELINE_STAGE_2_HOST_BIT }, Final::HostRead);
//!    g.pass("id_copy", FramePass::IdCopy).transfer_src(ids).transfer_dst(readback);
//!    ```
//!
//!    The graph now stores the scene's depth (a later pass reads it), inserts depth-write → depth-
//!    test and colour → transfer-source barriers, and a transfer → host barrier at the end. Nothing
//!    else changes for the other passes.
//! 3. Bind the imported buffer each frame (`g.bind_buffer(readback, buffer.handle)`).
//! 4. Record it in `record_frame`'s `match` with no barriers of its own:
//!
//!    ```text
//!    FramePass::IdBuffer => {
//!        let color = g.color_attachment(p, res.ids, [0.0; 4]);
//!        let depth = g.depth_attachment(p, res.depth, 0.0, None);
//!        // begin rendering with them, bind the id pipeline, draw, end rendering
//!    }
//!    FramePass::IdCopy => { /* vkCmdCopyImageToBuffer from g.image(res.ids).image (TRANSFER_SRC_OPTIMAL) */ }
//!    ```
//! 5. Build its pipeline in `Pipelines::prebuild*` (never while recording a frame).
//!
//! `tests::id_buffer_recipe_compiles` and `gpu::tests::graph_runs_an_id_buffer_pass` run exactly this.

use crate::descriptors::Descriptors;
use crate::device::{Device, Image};
use crate::vk::*;
use crate::{ErrorReason, GpuError, MemoryKind};
use std::fmt;
use std::ptr;

/// Most passes, resources and uses in one graph (agents' debug output stays bounded).
pub const MAX_GRAPH_PASSES: usize = 64;
pub const MAX_GRAPH_RESOURCES: usize = 64;
pub const MAX_GRAPH_USES: usize = 256;
/// Most array layers of a transient image.
pub const MAX_LAYERS: usize = 8;
/// Owner of transients shared by every graph (the shadow map).
pub const SHARED_OWNER: u32 = 0;
/// A transient unused while its owner rendered this many frames is freed.
const STALE_FRAMES: u64 = 600;
/// No bindless slot.
pub const NO_SLOT: u32 = u32::MAX;

/// A resource of one graph declaration (valid until the next [`RenderGraph::begin`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Res(u16);

/// What an attachment write does with the old contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Load {
    /// Cleared to a value.
    Clear,
    /// Undefined; every pixel is written.
    DontCare,
    /// Kept (read-modify-write, e.g. blending over what is there).
    Keep,
}

/// How a pass uses a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Access {
    /// Colour attachment (written; blending may read it).
    Color(Load),
    /// Depth attachment, tested and written.
    Depth(Load),
    /// Depth attachment, tested only.
    DepthRead,
    /// Sampled by fragment shaders.
    Sampled,
    /// Storage image or buffer read by compute shaders.
    StorageRead,
    /// Storage image or buffer written (whole) by compute shaders.
    StorageWrite,
    /// Copy source.
    TransferSrc,
    /// Copy destination (the whole resource is written).
    TransferDst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sync {
    stage: VkFlags64,
    access: VkFlags64,
    layout: VkImageLayout,
}

impl Access {
    fn sync(self) -> Sync {
        let tests = VK_PIPELINE_STAGE_2_EARLY_FRAGMENT_TESTS_BIT | VK_PIPELINE_STAGE_2_LATE_FRAGMENT_TESTS_BIT;
        let (stage, access, layout) = match self {
            Access::Color(_) => (
                VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
                VK_ACCESS_2_COLOR_ATTACHMENT_READ_BIT | VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
                VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
            ),
            Access::Depth(_) => (
                tests,
                VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_READ_BIT | VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT,
                VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL,
            ),
            Access::DepthRead => (tests, VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_READ_BIT, VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL),
            Access::Sampled => {
                (VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT, VK_ACCESS_2_SHADER_SAMPLED_READ_BIT, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL)
            }
            Access::StorageRead => (VK_PIPELINE_STAGE_2_COMPUTE_SHADER_BIT, VK_ACCESS_2_SHADER_STORAGE_READ_BIT, VK_IMAGE_LAYOUT_GENERAL),
            Access::StorageWrite => (VK_PIPELINE_STAGE_2_COMPUTE_SHADER_BIT, VK_ACCESS_2_SHADER_STORAGE_WRITE_BIT, VK_IMAGE_LAYOUT_GENERAL),
            Access::TransferSrc => (VK_PIPELINE_STAGE_2_COPY_BIT, VK_ACCESS_2_TRANSFER_READ_BIT, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL),
            Access::TransferDst => (VK_PIPELINE_STAGE_2_COPY_BIT, VK_ACCESS_2_TRANSFER_WRITE_BIT, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL),
        };
        Sync { stage, access, layout }
    }

    /// Reads the contents left by earlier passes.
    pub fn reads(self) -> bool {
        match self {
            Access::Color(l) | Access::Depth(l) => l == Load::Keep,
            Access::DepthRead | Access::Sampled | Access::StorageRead | Access::TransferSrc => true,
            Access::StorageWrite | Access::TransferDst => false,
        }
    }

    pub fn writes(self) -> bool {
        matches!(self, Access::Color(_) | Access::Depth(_) | Access::StorageWrite | Access::TransferDst)
    }

    fn attachment(self) -> bool {
        matches!(self, Access::Color(_) | Access::Depth(_) | Access::DepthRead)
    }

    fn image_usage(self) -> VkFlags {
        match self {
            Access::Color(_) => VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT,
            Access::Depth(_) | Access::DepthRead => VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT,
            Access::Sampled => VK_IMAGE_USAGE_SAMPLED_BIT,
            Access::StorageRead | Access::StorageWrite => VK_IMAGE_USAGE_STORAGE_BIT,
            Access::TransferSrc => VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
            Access::TransferDst => VK_IMAGE_USAGE_TRANSFER_DST_BIT,
        }
    }

    /// `None`: not an access a buffer can have.
    fn buffer_usage(self) -> Option<VkFlags> {
        match self {
            Access::StorageRead | Access::StorageWrite => Some(VK_BUFFER_USAGE_STORAGE_BUFFER_BIT),
            Access::TransferSrc => Some(VK_BUFFER_USAGE_TRANSFER_SRC_BIT),
            Access::TransferDst => Some(VK_BUFFER_USAGE_TRANSFER_DST_BIT),
            _ => None,
        }
    }
}

/// A transient image the graph owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDesc {
    pub format: VkFormat,
    pub width: u32,
    pub height: u32,
    /// Array layers (1..=[`MAX_LAYERS`]); each gets its own attachment view.
    pub layers: u32,
    /// Also create a `2D_ARRAY` view over every layer (sampled as an array, like the shadow map).
    pub array_view: bool,
    /// Give it a slot in the bindless texture array (sampled post-chain inputs).
    pub bindless: bool,
    pub memory: MemoryKind,
}

impl ImageDesc {
    /// A single-layer colour image, sampled through the bindless array if it is sampled at all.
    pub fn color(format: VkFormat, width: u32, height: u32, memory: MemoryKind) -> ImageDesc {
        ImageDesc { format, width, height, layers: 1, array_view: false, bindless: true, memory }
    }

    /// A single-layer depth image.
    pub fn depth(format: VkFormat, width: u32, height: u32, memory: MemoryKind) -> ImageDesc {
        ImageDesc { format, width, height, layers: 1, array_view: false, bindless: false, memory }
    }
}

/// What happened to an imported resource before this frame.
#[allow(dead_code)] // `Contents`: for the passes that come next
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prior {
    /// Its contents don't matter. `after`: the stages of earlier work that must finish before this
    /// frame touches it (e.g. `COPY` for a target read back last time; `COLOR_ATTACHMENT_OUTPUT`
    /// for a swapchain image, whose acquire semaphore is waited at that stage).
    Discard { after: VkFlags64 },
    /// It holds contents in `layout`, last written at `stage` with `access`.
    Contents { layout: VkImageLayout, stage: VkFlags64, access: VkFlags64 },
}

/// What an imported resource is used for after the frame.
#[allow(dead_code)] // `AsIs`, `Sampled`: for the passes that come next
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Final {
    /// Left as the last pass left it; assumed to be needed (stored).
    AsIs,
    /// Presented (swapchain image).
    Present,
    /// Copied out by later commands (readback).
    TransferSrc,
    /// Sampled by fragment shaders in later frames.
    Sampled,
    /// Read by the host after the frame's fence (buffers).
    HostRead,
}

#[cfg_attr(not(test), allow(dead_code))] // `Buffer`: for the passes that come next
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Transient { desc: ImageDesc, shared: bool },
    Image { format: VkFormat, width: u32, height: u32, layers: u32, prior: Prior, fin: Final },
    Buffer { size: u64, prior: Prior, fin: Final },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResourceDecl {
    name: &'static str,
    kind: Kind,
}

impl ResourceDecl {
    fn is_buffer(&self) -> bool {
        matches!(self.kind, Kind::Buffer { .. })
    }

    fn extent(&self) -> Option<(u32, u32)> {
        match self.kind {
            Kind::Transient { desc, .. } => Some((desc.width, desc.height)),
            Kind::Image { width, height, .. } => Some((width, height)),
            Kind::Buffer { .. } => None,
        }
    }

    fn format(&self) -> VkFormat {
        match self.kind {
            Kind::Transient { desc, .. } => desc.format,
            Kind::Image { format, .. } => format,
            Kind::Buffer { .. } => 0,
        }
    }

    fn layers(&self) -> u32 {
        match self.kind {
            Kind::Transient { desc, .. } => desc.layers,
            Kind::Image { layers, .. } => layers,
            Kind::Buffer { .. } => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PassDecl<T> {
    name: &'static str,
    tag: T,
    inline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UseDecl {
    pass: u16,
    res: Res,
    access: Access,
}

#[derive(Clone, Debug, PartialEq)]
struct Decl<T> {
    resources: Vec<ResourceDecl>,
    passes: Vec<PassDecl<T>>,
    uses: Vec<UseDecl>,
}

impl<T> Default for Decl<T> {
    fn default() -> Self {
        Decl { resources: Vec::new(), passes: Vec::new(), uses: Vec::new() }
    }
}

impl<T: Clone> Decl<T> {
    fn clear(&mut self) {
        self.resources.clear();
        self.passes.clear();
        self.uses.clear();
    }

    /// Copy `other` into `self`, reusing storage.
    fn assign(&mut self, other: &Decl<T>) {
        self.resources.clone_from(&other.resources);
        self.passes.clone_from(&other.passes);
        self.uses.clone_from(&other.uses);
    }
}

/// Why a graph declaration was rejected (inside [`GpuError::reason`] as `ErrorReason::Graph`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// `pass` reads `resource`, which holds nothing yet this frame.
    ReadBeforeWrite { pass: &'static str, resource: &'static str },
    /// `pass` overwrites `resource` although nothing read what `written_by` wrote there.
    UnreadOverwrite { pass: &'static str, resource: &'static str, written_by: &'static str },
    /// `resource`, an attachment of `pass`, is `width`×`height`; the pass renders `expected`.
    SizeMismatch { pass: &'static str, resource: &'static str, width: u32, height: u32, expected: (u32, u32) },
    /// `pass` declares `resource` twice.
    DuplicateUse { pass: &'static str, resource: &'static str },
    /// Two transients of one owner (or two shared ones) are named `resource`: they would get the
    /// same pool image.
    DuplicateName { resource: &'static str },
    /// `access` doesn't apply to `resource` (e.g. a buffer as an attachment).
    WrongAccess { pass: &'static str, resource: &'static str, access: Access },
    /// A resource id that this declaration doesn't have (from another graph or an earlier frame).
    UnknownResource { pass: &'static str, index: u32 },
    /// `pass` is inline but `resource` would need a barrier (or isn't an attachment of the pass it
    /// joins).
    InlineNeedsBarrier { pass: &'static str, resource: &'static str },
    /// `pass` is inline but has no pass before it to join.
    InlineFirst { pass: &'static str },
    /// `resource` must hold contents after the frame (presented, read back) but nothing wrote it.
    NeverWritten { resource: &'static str },
    /// `pass` has more than one depth attachment or more than eight colour attachments.
    TooManyAttachments { pass: &'static str },
    /// A resource with no pixels or bytes, too many layers, or a final use it can't have (an image
    /// read by the host, a buffer presented or sampled).
    BadDesc { resource: &'static str },
    /// More passes, resources or uses than a graph may have.
    TooLarge { what: &'static str, count: u32, max: u32 },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            GraphError::ReadBeforeWrite { pass, resource } => write!(f, "pass '{pass}' reads '{resource}', which nothing wrote this frame"),
            GraphError::UnreadOverwrite { pass, resource, written_by } => {
                write!(f, "pass '{pass}' overwrites '{resource}', but nothing read what '{written_by}' wrote there")
            }
            GraphError::SizeMismatch { pass, resource, width, height, expected } => write!(
                f,
                "pass '{pass}': attachment '{resource}' is {width}x{height}, the pass renders {}x{}",
                expected.0, expected.1
            ),
            GraphError::DuplicateUse { pass, resource } => write!(f, "pass '{pass}' declares '{resource}' twice"),
            GraphError::DuplicateName { resource } => write!(f, "two transients are named '{resource}' (they would share one image)"),
            GraphError::WrongAccess { pass, resource, access } => write!(f, "pass '{pass}': {access:?} doesn't apply to '{resource}'"),
            GraphError::UnknownResource { pass, index } => write!(f, "pass '{pass}' uses resource #{index}, which this graph doesn't declare"),
            GraphError::InlineNeedsBarrier { pass, resource } => {
                write!(f, "inline pass '{pass}' can't use '{resource}' inside the previous pass's rendering (it needs a barrier)")
            }
            GraphError::InlineFirst { pass } => write!(f, "inline pass '{pass}' has no pass before it to join"),
            GraphError::NeverWritten { resource } => write!(f, "'{resource}' is used after the frame but no pass wrote it"),
            GraphError::TooManyAttachments { pass } => write!(f, "pass '{pass}' has more than one depth or eight colour attachments"),
            GraphError::BadDesc { resource } => {
                write!(f, "'{resource}' has a zero size, 0 or more than {MAX_LAYERS} layers, or a final use it can't have")
            }
            GraphError::TooLarge { what, count, max } => write!(f, "{count} graph {what}; at most {max}"),
        }
    }
}

fn graph_error(e: GraphError) -> GpuError {
    GpuError::rejected(ErrorReason::Graph(e), format!("render graph: {e}"))
}

// ---------------------------------------------------------------------------------------------
// Debug info for agents
// ---------------------------------------------------------------------------------------------

/// One barrier the graph records (image or buffer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphBarrier {
    pub resource: &'static str,
    /// Layout names (`"-"` for buffers).
    pub old_layout: &'static str,
    pub new_layout: &'static str,
    /// `VkPipelineStageFlags2` / `VkAccessFlags2`. A transient's first barrier waits for its last
    /// use in an earlier frame, known only when recording: `src_stage` shows `ALL_COMMANDS` there.
    pub src_stage: u64,
    pub src_access: u64,
    pub dst_stage: u64,
    pub dst_access: u64,
}

/// How one pass uses one resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphUse {
    pub resource: &'static str,
    pub access: Access,
    /// For attachments: `"clear"`, `"dont_care"` or `"load"`, and `"store"` or `"dont_care"`.
    pub load_op: &'static str,
    pub store_op: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphPass {
    pub name: &'static str,
    /// Recorded inside the previous pass's rendering instance.
    pub inline: bool,
    pub uses: Vec<GraphUse>,
    /// Recorded just before the pass, in one `vkCmdPipelineBarrier2`.
    pub barriers: Vec<GraphBarrier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphResource {
    pub name: &'static str,
    /// `"transient"`, `"shared"` (a transient every graph uses, e.g. the shadow map),
    /// `"imported"` (image) or `"buffer"`.
    pub kind: &'static str,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    /// `VkFormat` (0 for buffers).
    pub format: i32,
    /// Usage flags derived from the declared accesses.
    pub usage: u32,
    /// Buffers: size in bytes.
    pub bytes: u64,
}

/// A compiled frame graph, as data: what each pass reads and writes and which barriers the graph
/// inserts. From [`crate::Gpu::last_graph`]. `Display` prints it compactly, one line per pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphInfo {
    pub resources: Vec<GraphResource>,
    pub passes: Vec<GraphPass>,
    /// After the last pass (present, readback).
    pub final_barriers: Vec<GraphBarrier>,
    /// Times this graph was (re)compiled: only when its passes, formats or sizes changed.
    pub compiles: u64,
}

impl GraphInfo {
    /// Barriers per frame (image and buffer).
    pub fn barrier_count(&self) -> usize {
        self.passes.iter().map(|p| p.barriers.len()).sum::<usize>() + self.final_barriers.len()
    }
}

/// Short names of the stages in `stages`, joined with `+`.
pub fn stage_names(stages: u64) -> String {
    const NAMES: [(u64, &str); 9] = [
        (VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT, "color_out"),
        (VK_PIPELINE_STAGE_2_EARLY_FRAGMENT_TESTS_BIT, "early_z"),
        (VK_PIPELINE_STAGE_2_LATE_FRAGMENT_TESTS_BIT, "late_z"),
        (VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT, "fragment"),
        (VK_PIPELINE_STAGE_2_COMPUTE_SHADER_BIT, "compute"),
        (VK_PIPELINE_STAGE_2_COPY_BIT, "copy"),
        (VK_PIPELINE_STAGE_2_HOST_BIT, "host"),
        (VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT, "all"),
        (VK_PIPELINE_STAGE_2_VERTEX_ATTRIBUTE_INPUT_BIT, "vertex_input"),
    ];
    if stages == 0 {
        return "none".into();
    }
    let mut out = String::new();
    let mut rest = stages;
    for (bit, name) in NAMES {
        if stages & bit != 0 {
            if !out.is_empty() {
                out.push('+');
            }
            out.push_str(name);
            rest &= !bit;
        }
    }
    if rest != 0 {
        if !out.is_empty() {
            out.push('+');
        }
        out.push_str(&format!("{rest:#x}"));
    }
    out
}

pub fn layout_name(layout: VkImageLayout) -> &'static str {
    match layout {
        VK_IMAGE_LAYOUT_UNDEFINED => "undefined",
        VK_IMAGE_LAYOUT_GENERAL => "general",
        VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL => "color_attachment",
        VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL => "depth_attachment",
        VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL => "shader_read",
        VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL => "transfer_src",
        VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL => "transfer_dst",
        VK_IMAGE_LAYOUT_PRESENT_SRC_KHR => "present",
        _ => "other",
    }
}

impl fmt::Display for GraphBarrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.resource)?;
        if self.old_layout != self.new_layout {
            write!(f, "{}->{} ", self.old_layout, self.new_layout)?;
        }
        write!(f, "[{} -> {}]", stage_names(self.src_stage), stage_names(self.dst_stage))
    }
}

impl fmt::Display for GraphInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "render graph: {} passes, {} barriers, compiled {} time(s)", self.passes.len(), self.barrier_count(), self.compiles)?;
        for p in &self.passes {
            write!(f, "{}{}:", p.name, if p.inline { " (inline)" } else { "" })?;
            for u in &p.uses {
                write!(f, " {}={:?}", u.resource, u.access)?;
                if u.store_op != "-" {
                    write!(f, "({}/{})", u.load_op, u.store_op)?;
                }
            }
            for b in &p.barriers {
                write!(f, " | {b}")?;
            }
            writeln!(f)?;
        }
        if !self.final_barriers.is_empty() {
            write!(f, "end:")?;
            for b in &self.final_barriers {
                write!(f, " | {b}")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The compiled plan
// ---------------------------------------------------------------------------------------------

/// Where a barrier's source scope comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Src {
    Known { stage: VkFlags64, access: VkFlags64 },
    /// The transient's last use in an earlier frame (from the pool, when recording).
    Pool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BarrierPlan {
    res: Res,
    src: Src,
    dst_stage: VkFlags64,
    dst_access: VkFlags64,
    old_layout: VkImageLayout,
    new_layout: VkImageLayout,
}

/// Attachment ops of one use (`load`, `store`), `-1` when not an attachment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ops {
    load: i32,
    store: i32,
}

struct Plan<T> {
    decl: Decl<T>,
    /// Per pass: its uses (`decl.uses[a..b]`) and barriers (`barriers[c..d]`); one more entry
    /// holds the final barriers.
    use_ranges: Vec<(u32, u32)>,
    barrier_ranges: Vec<(u32, u32)>,
    barriers: Vec<BarrierPlan>,
    ops: Vec<Ops>,
    /// Per resource: usage flags, and the state after the frame (stage, access) for the pool.
    usage: Vec<VkFlags>,
    last: Vec<(VkFlags64, VkFlags64)>,
    info: GraphInfo,
}

impl<T> Default for Plan<T> {
    fn default() -> Self {
        Plan {
            decl: Decl::default(),
            use_ranges: Vec::new(),
            barrier_ranges: Vec::new(),
            barriers: Vec::new(),
            ops: Vec::new(),
            usage: Vec::new(),
            last: Vec::new(),
            info: GraphInfo::default(),
        }
    }
}

/// Per-resource state while planning.
#[derive(Clone, Copy, Debug)]
struct State {
    contents: bool,
    layout: VkImageLayout,
    /// Last write (a layout transition counts as a write in its barrier's destination stages).
    write: (VkFlags64, VkFlags64),
    /// Stages that read since the last write.
    read_stages: VkFlags64,
    /// Stages and accesses the last write is visible to.
    visible: (VkFlags64, VkFlags64),
    /// The pass whose write nothing has read yet.
    unread: Option<u16>,
    /// Touched this frame (the first barrier of a transient waits on the pool's state).
    touched: bool,
}

fn depth_format(format: VkFormat) -> bool {
    (VK_FORMAT_D16_UNORM..=VK_FORMAT_D32_SFLOAT_S8_UINT).contains(&format)
}

fn aspect_of(format: VkFormat) -> VkFlags {
    if depth_format(format) {
        VK_IMAGE_ASPECT_DEPTH_BIT
    } else {
        VK_IMAGE_ASPECT_COLOR_BIT
    }
}

fn load_op(access: Access) -> i32 {
    match access {
        Access::Color(Load::Clear) | Access::Depth(Load::Clear) => VK_ATTACHMENT_LOAD_OP_CLEAR,
        Access::Color(Load::DontCare) | Access::Depth(Load::DontCare) => VK_ATTACHMENT_LOAD_OP_DONT_CARE,
        _ => VK_ATTACHMENT_LOAD_OP_LOAD,
    }
}

fn final_needs_contents(f: Final) -> bool {
    !matches!(f, Final::AsIs) // AsIs keeps whatever is there; the others consume it
}

fn final_sync(f: Final) -> Option<(VkFlags64, VkFlags64, VkImageLayout)> {
    match f {
        Final::AsIs => None,
        Final::Present => Some((VK_PIPELINE_STAGE_2_NONE, VK_ACCESS_2_NONE, VK_IMAGE_LAYOUT_PRESENT_SRC_KHR)),
        Final::TransferSrc => Some((VK_PIPELINE_STAGE_2_COPY_BIT, VK_ACCESS_2_TRANSFER_READ_BIT, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL)),
        Final::Sampled => {
            Some((VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT, VK_ACCESS_2_SHADER_SAMPLED_READ_BIT, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL))
        }
        Final::HostRead => Some((VK_PIPELINE_STAGE_2_HOST_BIT, VK_ACCESS_2_HOST_READ_BIT, VK_IMAGE_LAYOUT_UNDEFINED)),
    }
}

impl<T: Copy + PartialEq> Plan<T> {
    /// Validate `decl` and plan its barriers into `self` (reusing storage).
    fn compile(&mut self, decl: &Decl<T>, compiles: u64) -> Result<(), GraphError> {
        let check = |what, count: usize, max: usize| {
            if count > max {
                Err(GraphError::TooLarge { what, count: count as u32, max: max as u32 })
            } else {
                Ok(())
            }
        };
        check("passes", decl.passes.len(), MAX_GRAPH_PASSES)?;
        check("resources", decl.resources.len(), MAX_GRAPH_RESOURCES)?;
        check("uses", decl.uses.len(), MAX_GRAPH_USES)?;
        for (i, r) in decl.resources.iter().enumerate() {
            if let Kind::Transient { shared, .. } = r.kind {
                let clash = decl.resources[..i].iter().any(|o| matches!(o.kind, Kind::Transient { shared: s, .. } if s == shared) && o.name == r.name);
                if clash {
                    return Err(GraphError::DuplicateName { resource: r.name });
                }
            }
            let bad = match r.kind {
                Kind::Transient { desc, .. } => {
                    desc.width == 0 || desc.height == 0 || desc.layers == 0 || desc.layers as usize > MAX_LAYERS
                }
                Kind::Image { width, height, fin, .. } => width == 0 || height == 0 || fin == Final::HostRead,
                Kind::Buffer { size, fin, .. } => size == 0 || matches!(fin, Final::Present | Final::Sampled),
            };
            if bad {
                return Err(GraphError::BadDesc { resource: r.name });
            }
        }
        self.use_ranges.clear();
        self.barrier_ranges.clear();
        self.barriers.clear();
        self.ops.clear();
        self.usage.clear();
        self.usage.resize(decl.resources.len(), 0);
        self.last.clear();
        self.last.resize(decl.resources.len(), (0, 0));

        let mut states: Vec<State> = decl
            .resources
            .iter()
            .map(|r| {
                let prior = match r.kind {
                    Kind::Transient { .. } => None,
                    Kind::Image { prior, .. } | Kind::Buffer { prior, .. } => Some(prior),
                };
                let (contents, layout, write) = match prior {
                    None => (false, VK_IMAGE_LAYOUT_UNDEFINED, (0, 0)),
                    Some(Prior::Discard { after }) => (false, VK_IMAGE_LAYOUT_UNDEFINED, (after, 0)),
                    Some(Prior::Contents { layout, stage, access }) => (true, layout, (stage, access)),
                };
                State { contents, layout, write, read_stages: 0, visible: (0, 0), unread: None, touched: false }
            })
            .collect();

        let mut u = 0usize;
        for (p, pass) in decl.passes.iter().enumerate() {
            let start = u;
            while u < decl.uses.len() && decl.uses[u].pass as usize == p {
                u += 1;
            }
            let uses = &decl.uses[start..u];
            self.use_ranges.push((start as u32, u as u32));
            let first_barrier = self.barriers.len();
            if pass.inline && p == 0 {
                return Err(GraphError::InlineFirst { pass: pass.name });
            }
            // Per-pass checks: known resources, kinds, duplicates, attachment sizes and counts.
            let mut extent: Option<(u32, u32)> = None;
            let (mut colors, mut depths) = (0, 0);
            for (i, use_) in uses.iter().enumerate() {
                let Some(r) = decl.resources.get(use_.res.0 as usize) else {
                    return Err(GraphError::UnknownResource { pass: pass.name, index: use_.res.0 as u32 });
                };
                if uses[..i].iter().any(|o| o.res == use_.res) {
                    return Err(GraphError::DuplicateUse { pass: pass.name, resource: r.name });
                }
                let a = use_.access;
                let fits = if r.is_buffer() {
                    a.buffer_usage().is_some()
                } else {
                    match a {
                        Access::Depth(_) | Access::DepthRead => depth_format(r.format()),
                        Access::Color(_) => !depth_format(r.format()),
                        _ => true,
                    }
                };
                if !fits {
                    return Err(GraphError::WrongAccess { pass: pass.name, resource: r.name, access: a });
                }
                if a.attachment() {
                    let (w, h) = r.extent().unwrap_or((0, 0));
                    match extent {
                        None => extent = Some((w, h)),
                        Some(e) if e != (w, h) => {
                            return Err(GraphError::SizeMismatch { pass: pass.name, resource: r.name, width: w, height: h, expected: e })
                        }
                        _ => {}
                    }
                    if matches!(a, Access::Color(_)) {
                        colors += 1;
                    } else {
                        depths += 1;
                    }
                }
                self.usage[use_.res.0 as usize] |= if r.is_buffer() { a.buffer_usage().unwrap_or(0) } else { a.image_usage() };
            }
            if colors > 8 || depths > 1 {
                return Err(GraphError::TooManyAttachments { pass: pass.name });
            }
            if pass.inline {
                // Every use must be an attachment the joined pass renders to, in the same layout,
                // with nothing to wait for (rasterization order covers blending over it).
                let (ps, pe) = self.use_ranges[p - 1];
                for use_ in uses {
                    let name = decl.resources[use_.res.0 as usize].name;
                    let joined = decl.uses[ps as usize..pe as usize].iter().find(|o| o.res == use_.res);
                    let ok = use_.access.attachment()
                        && joined.is_some_and(|o| o.access.attachment() && o.access.sync().layout == use_.access.sync().layout);
                    if !ok {
                        return Err(GraphError::InlineNeedsBarrier { pass: pass.name, resource: name });
                    }
                    let s = &mut states[use_.res.0 as usize];
                    if use_.access.reads() {
                        s.unread = None;
                    }
                    if use_.access.writes() {
                        s.unread = Some(p as u16);
                    }
                }
                self.barrier_ranges.push((first_barrier as u32, first_barrier as u32));
                continue;
            }
            for use_ in uses {
                let r = &decl.resources[use_.res.0 as usize];
                let a = use_.access;
                let sync = a.sync();
                let s = &mut states[use_.res.0 as usize];
                if a.reads() && !s.contents {
                    return Err(GraphError::ReadBeforeWrite { pass: pass.name, resource: r.name });
                }
                if a.writes() && !a.reads() {
                    if let Some(w) = s.unread {
                        return Err(GraphError::UnreadOverwrite {
                            pass: pass.name,
                            resource: r.name,
                            written_by: decl.passes[w as usize].name,
                        });
                    }
                }
                let buffer = r.is_buffer();
                let layout_change = !buffer && s.layout != sync.layout;
                let pool_src = matches!(r.kind, Kind::Transient { .. }) && !s.touched;
                if a.writes() || layout_change || pool_src {
                    // Wait for the reads since the last write (they already waited for it), or
                    // else for the last write itself.
                    let src = if pool_src {
                        Src::Pool
                    } else if s.read_stages != 0 {
                        Src::Known { stage: s.read_stages | s.write.0, access: 0 }
                    } else {
                        Src::Known { stage: s.write.0, access: s.write.1 }
                    };
                    let discard = !s.contents || (a.writes() && !a.reads());
                    let old_layout = if buffer {
                        VK_IMAGE_LAYOUT_UNDEFINED
                    } else if discard {
                        if s.layout == sync.layout && s.contents {
                            sync.layout // same layout: no transition needed
                        } else {
                            VK_IMAGE_LAYOUT_UNDEFINED
                        }
                    } else {
                        s.layout
                    };
                    self.barriers.push(BarrierPlan {
                        res: use_.res,
                        src,
                        dst_stage: sync.stage,
                        dst_access: sync.access,
                        old_layout,
                        new_layout: if buffer { VK_IMAGE_LAYOUT_UNDEFINED } else { sync.layout },
                    });
                    if a.writes() {
                        s.write = (sync.stage, sync.access & write_bits());
                        s.read_stages = 0;
                        s.visible = (0, 0);
                    } else {
                        // A layout transition is a write in this barrier's destination stages.
                        if layout_change || pool_src {
                            s.write.0 |= sync.stage;
                        }
                        s.read_stages |= sync.stage;
                        s.visible = (s.visible.0 | sync.stage, s.visible.1 | sync.access);
                    }
                } else if sync.stage & !s.visible.0 != 0 || sync.access & !s.visible.1 != 0 {
                    // A read in a stage the last write isn't visible to yet.
                    self.barriers.push(BarrierPlan {
                        res: use_.res,
                        src: Src::Known { stage: s.write.0, access: s.write.1 },
                        dst_stage: sync.stage,
                        dst_access: sync.access,
                        old_layout: if buffer { VK_IMAGE_LAYOUT_UNDEFINED } else { s.layout },
                        new_layout: if buffer { VK_IMAGE_LAYOUT_UNDEFINED } else { s.layout },
                    });
                    s.read_stages |= sync.stage;
                    s.visible = (s.visible.0 | sync.stage, s.visible.1 | sync.access);
                } else {
                    s.read_stages |= sync.stage;
                }
                if !buffer {
                    s.layout = sync.layout;
                }
                s.touched = true;
                if a.reads() {
                    s.unread = None;
                }
                if a.writes() {
                    s.contents = true;
                    s.unread = Some(p as u16);
                }
            }
            self.barrier_ranges.push((first_barrier as u32, self.barriers.len() as u32));
        }
        self.use_ranges.push((u as u32, u as u32));

        // Final transitions of imports; the pool's view of each transient.
        let first_final = self.barriers.len();
        for (i, r) in decl.resources.iter().enumerate() {
            let s = &states[i];
            let src = if s.read_stages != 0 { (s.read_stages | s.write.0, 0) } else { s.write };
            self.last[i] = (s.read_stages | s.write.0, s.write.1);
            let fin = match r.kind {
                Kind::Image { fin, .. } | Kind::Buffer { fin, .. } => fin,
                Kind::Transient { .. } => continue,
            };
            if final_needs_contents(fin) && !s.contents {
                return Err(GraphError::NeverWritten { resource: r.name });
            }
            let Some((stage, access, layout)) = final_sync(fin) else { continue };
            if r.is_buffer() {
                self.barriers.push(BarrierPlan {
                    res: Res(i as u16),
                    src: Src::Known { stage: src.0, access: src.1 },
                    dst_stage: stage,
                    dst_access: access,
                    old_layout: VK_IMAGE_LAYOUT_UNDEFINED,
                    new_layout: VK_IMAGE_LAYOUT_UNDEFINED,
                });
            } else if s.touched || s.layout != layout {
                self.barriers.push(BarrierPlan {
                    res: Res(i as u16),
                    src: Src::Known { stage: src.0, access: src.1 },
                    dst_stage: stage,
                    dst_access: access,
                    old_layout: s.layout,
                    new_layout: layout,
                });
            }
        }
        self.barrier_ranges.push((first_final as u32, self.barriers.len() as u32));

        // Store ops: keep an attachment only if something reads it later.
        for (i, use_) in decl.uses.iter().enumerate() {
            if !use_.access.attachment() {
                self.ops.push(Ops { load: -1, store: -1 });
                continue;
            }
            let r = &decl.resources[use_.res.0 as usize];
            let later = decl.uses[i + 1..].iter().any(|o| o.res == use_.res && o.access.reads());
            // An imported image is kept for whatever uses it after the frame.
            let after = !matches!(r.kind, Kind::Transient { .. });
            let store = if later || after { VK_ATTACHMENT_STORE_OP_STORE } else { VK_ATTACHMENT_STORE_OP_DONT_CARE };
            self.ops.push(Ops { load: load_op(use_.access), store });
        }
        self.decl.assign(decl);
        self.build_info(compiles);
        Ok(())
    }

    fn barrier_info(&self, b: &BarrierPlan) -> GraphBarrier {
        let r = &self.decl.resources[b.res.0 as usize];
        let (src_stage, src_access) = match b.src {
            Src::Known { stage, access } => (stage, access),
            Src::Pool => (VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT, 0),
        };
        let buffer = r.is_buffer();
        GraphBarrier {
            resource: r.name,
            old_layout: if buffer { "-" } else { layout_name(b.old_layout) },
            new_layout: if buffer { "-" } else { layout_name(b.new_layout) },
            src_stage,
            src_access,
            dst_stage: b.dst_stage,
            dst_access: b.dst_access,
        }
    }

    fn build_info(&mut self, compiles: u64) {
        let d = &self.decl;
        let resources = d
            .resources
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let (w, h) = r.extent().unwrap_or((0, 0));
                GraphResource {
                    name: r.name,
                    kind: match r.kind {
                        Kind::Transient { shared: true, .. } => "shared",
                        Kind::Transient { .. } => "transient",
                        Kind::Image { .. } => "imported",
                        Kind::Buffer { .. } => "buffer",
                    },
                    width: w,
                    height: h,
                    layers: r.layers(),
                    format: r.format(),
                    usage: self.usage[i],
                    bytes: match r.kind {
                        Kind::Buffer { size, .. } => size,
                        _ => 0,
                    },
                }
            })
            .collect();
        let passes = d
            .passes
            .iter()
            .enumerate()
            .map(|(p, pass)| {
                let (a, b) = self.use_ranges[p];
                let (c, e) = self.barrier_ranges[p];
                GraphPass {
                    name: pass.name,
                    inline: pass.inline,
                    uses: (a..b)
                        .map(|i| {
                            let use_ = &d.uses[i as usize];
                            let ops = self.ops[i as usize];
                            let op = |o: i32, store: bool| match (o, store) {
                                (-1, _) => "-",
                                (VK_ATTACHMENT_LOAD_OP_CLEAR, false) => "clear",
                                (VK_ATTACHMENT_LOAD_OP_LOAD, false) => "load",
                                (VK_ATTACHMENT_STORE_OP_STORE, true) => "store",
                                _ => "dont_care",
                            };
                            GraphUse {
                                resource: d.resources[use_.res.0 as usize].name,
                                access: use_.access,
                                load_op: op(ops.load, false),
                                store_op: op(ops.store, true),
                            }
                        })
                        .collect(),
                    barriers: self.barriers[c as usize..e as usize].iter().map(|b| self.barrier_info(b)).collect(),
                }
            })
            .collect();
        let (c, e) = self.barrier_ranges[d.passes.len()];
        let final_barriers = self.barriers[c as usize..e as usize].iter().map(|b| self.barrier_info(b)).collect();
        self.info = GraphInfo { resources, passes, final_barriers, compiles };
    }
}

fn write_bits() -> VkFlags64 {
    VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT
        | VK_ACCESS_2_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT
        | VK_ACCESS_2_TRANSFER_WRITE_BIT
        | VK_ACCESS_2_SHADER_STORAGE_WRITE_BIT
}

// ---------------------------------------------------------------------------------------------
// The graph
// ---------------------------------------------------------------------------------------------

/// The handles of an image for this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoundImage {
    pub image: VkImage,
    /// The whole image (`2D` for one layer).
    pub view: VkImageView,
    /// `2D_ARRAY` over every layer (transients with `array_view`).
    pub array_view: VkImageView,
    /// One `2D` view per layer, to render into.
    pub layer_views: [VkImageView; MAX_LAYERS],
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub aspect: VkFlags,
    /// Slot in the bindless texture array ([`NO_SLOT`] if none).
    pub slot: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Bound {
    image: BoundImage,
    buffer: VkBuffer,
    /// The transient's pool entry and its last use before this frame.
    entry: usize,
    prior: (VkFlags64, VkFlags64),
}

/// A render graph (one per render target or window); `T` tags passes for the recorder's `match`.
pub struct RenderGraph<T> {
    owner: u32,
    decl: Decl<T>,
    plan: Plan<T>,
    compiled: bool,
    bound: Vec<Bound>,
    image_barriers: Vec<VkImageMemoryBarrier2>,
    buffer_barriers: Vec<VkBufferMemoryBarrier2>,
    compiles: u64,
    /// The last `prepare` compiled (the declaration changed).
    recompiled: bool,
    /// Barriers recorded since `prepare` (image + buffer).
    recorded: u32,
}

/// Builder for one pass's uses; see [`RenderGraph::pass`].
pub struct PassBuilder<'g, T> {
    g: &'g mut RenderGraph<T>,
    pass: u16,
}

impl<T: Copy + PartialEq> PassBuilder<'_, T> {
    pub fn access(self, res: Res, access: Access) -> Self {
        self.g.decl.uses.push(UseDecl { pass: self.pass, res, access });
        self
    }

    pub fn color(self, res: Res, load: Load) -> Self {
        self.access(res, Access::Color(load))
    }

    pub fn depth(self, res: Res, load: Load) -> Self {
        self.access(res, Access::Depth(load))
    }

    // Part of the graph API for the passes that come next (ID buffer, GPU skinning, SSAO); used by
    // the tests today.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn depth_read(self, res: Res) -> Self {
        self.access(res, Access::DepthRead)
    }

    pub fn sampled(self, res: Res) -> Self {
        self.access(res, Access::Sampled)
    }

    // Part of the graph API for the passes that come next (ID buffer, GPU skinning, SSAO); used by
    // the tests today.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn transfer_src(self, res: Res) -> Self {
        self.access(res, Access::TransferSrc)
    }

    // Part of the graph API for the passes that come next (ID buffer, GPU skinning, SSAO); used by
    // the tests today.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn transfer_dst(self, res: Res) -> Self {
        self.access(res, Access::TransferDst)
    }

    /// Record inside the previous pass's rendering instance (see the module docs).
    pub fn inline(self) -> Self {
        self.g.decl.passes[self.pass as usize].inline = true;
        self
    }
}

impl<T: Copy + PartialEq + fmt::Debug> RenderGraph<T> {
    /// A graph whose transients belong to `owner` (not [`SHARED_OWNER`]).
    pub fn new(owner: u32) -> RenderGraph<T> {
        RenderGraph {
            owner,
            decl: Decl::default(),
            plan: Plan::default(),
            compiled: false,
            bound: Vec::new(),
            image_barriers: Vec::new(),
            buffer_barriers: Vec::new(),
            compiles: 0,
            recompiled: false,
            recorded: 0,
        }
    }

    /// Start declaring a frame (storage is kept).
    pub fn begin(&mut self) {
        self.decl.clear();
        self.recorded = 0;
    }

    fn add(&mut self, name: &'static str, kind: Kind) -> Res {
        let i = self.decl.resources.len();
        self.decl.resources.push(ResourceDecl { name, kind });
        Res(i.min(u16::MAX as usize) as u16)
    }

    /// An image the graph owns, private to this graph.
    pub fn transient(&mut self, name: &'static str, desc: ImageDesc) -> Res {
        self.add(name, Kind::Transient { desc, shared: false })
    }

    /// An image the graph owns, shared with every other graph (one per name and description).
    pub fn shared(&mut self, name: &'static str, desc: ImageDesc) -> Res {
        self.add(name, Kind::Transient { desc, shared: true })
    }

    /// An image owned elsewhere (a target, a swapchain image); bind its handles each frame.
    pub fn import_image(&mut self, name: &'static str, format: VkFormat, width: u32, height: u32, prior: Prior, fin: Final) -> Res {
        self.add(name, Kind::Image { format, width, height, layers: 1, prior, fin })
    }

    /// A buffer owned elsewhere; bind its handle each frame.
    // Part of the graph API for the passes that come next (ID buffer, GPU skinning, SSAO); used by
    // the tests today.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn import_buffer(&mut self, name: &'static str, size: u64, prior: Prior, fin: Final) -> Res {
        self.add(name, Kind::Buffer { size, prior, fin })
    }

    /// Add a pass (run after every pass declared before it).
    pub fn pass(&mut self, name: &'static str, tag: T) -> PassBuilder<'_, T> {
        let pass = self.decl.passes.len().min(u16::MAX as usize) as u16;
        self.decl.passes.push(PassDecl { name, tag, inline: false });
        PassBuilder { g: self, pass }
    }

    /// Compile the declaration if it changed (validating it), then find or create every transient
    /// in `pool`. The pool must not collect before this frame is recorded.
    pub fn prepare(&mut self, dev: &Device, descriptors: &mut Descriptors, pool: &mut TransientPool) -> Result<(), GpuError> {
        self.recompiled = !self.compiled || self.plan.decl != self.decl;
        if self.recompiled {
            self.compiled = false;
            self.compiles += 1;
            self.plan.compile(&self.decl, self.compiles).map_err(graph_error)?;
            self.compiled = true;
        }
        self.bound.clear();
        self.bound.resize(self.decl.resources.len(), Bound::default());
        for (i, r) in self.plan.decl.resources.iter().enumerate() {
            let b = &mut self.bound[i];
            match r.kind {
                Kind::Transient { desc, shared } => {
                    let owner = if shared { SHARED_OWNER } else { self.owner };
                    let e = pool.acquire(dev, descriptors, owner, r.name, desc, self.plan.usage[i])?;
                    let entry = &pool.entries[e];
                    b.entry = e;
                    b.prior = entry.last;
                    b.image = entry.bound;
                }
                Kind::Image { width, height, format, .. } => {
                    b.image = BoundImage { width, height, layers: 1, aspect: aspect_of(format), slot: NO_SLOT, ..Default::default() };
                }
                Kind::Buffer { .. } => {}
            }
        }
        // Every transient was found or created: only now retire the images they replace.
        for (i, r) in self.plan.decl.resources.iter().enumerate() {
            if let Kind::Transient { .. } = r.kind {
                pool.commit(self.bound[i].entry);
            }
        }
        Ok(())
    }

    /// The handles of imported image `res` for this frame (after `prepare`).
    pub fn bind_image(&mut self, res: Res, image: VkImage, view: VkImageView) {
        let b = &mut self.bound[res.0 as usize].image;
        b.image = image;
        b.view = view;
        b.layer_views[0] = view;
    }

    // Part of the graph API for the passes that come next (ID buffer, GPU skinning, SSAO); used by
    // the tests today.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn bind_buffer(&mut self, res: Res, buffer: VkBuffer) {
        self.bound[res.0 as usize].buffer = buffer;
    }

    pub fn image(&self, res: Res) -> &BoundImage {
        &self.bound[res.0 as usize].image
    }

    pub fn pass_count(&self) -> usize {
        self.plan.decl.passes.len()
    }

    pub fn tag(&self, pass: usize) -> T {
        self.plan.decl.passes[pass].tag
    }

    pub fn is_inline(&self, pass: usize) -> bool {
        self.plan.decl.passes[pass].inline
    }

    /// The inline pass that joins `pass`, if any.
    pub fn inline_after(&self, pass: usize) -> Option<usize> {
        self.plan.decl.passes.get(pass + 1).filter(|p| p.inline).map(|_| pass + 1)
    }

    fn ops(&self, pass: usize, res: Res) -> Ops {
        let (a, b) = self.plan.use_ranges[pass];
        let found = (a..b).find(|&i| self.plan.decl.uses[i as usize].res == res);
        debug_assert!(found.is_some(), "pass {pass} renders to a resource it did not declare");
        found.map_or(Ops { load: VK_ATTACHMENT_LOAD_OP_LOAD, store: VK_ATTACHMENT_STORE_OP_STORE }, |i| self.plan.ops[i as usize])
    }

    /// The colour attachment `res` of `pass`, with the graph's load/store ops.
    pub fn color_attachment(&self, pass: usize, res: Res, clear: [f32; 4]) -> VkRenderingAttachmentInfo {
        let ops = self.ops(pass, res);
        attachment(self.image(res).view, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, ops, VkClearValue { float32: clear })
    }

    /// The depth attachment `res` of `pass` (`layer` of a layered image), with the graph's ops.
    pub fn depth_attachment(&self, pass: usize, res: Res, clear_depth: f32, layer: Option<u32>) -> VkRenderingAttachmentInfo {
        let ops = self.ops(pass, res);
        let img = self.image(res);
        let view = match layer {
            Some(l) => img.layer_views[(l as usize).min(MAX_LAYERS - 1)],
            None => img.view,
        };
        // VkClearDepthStencilValue { depth, stencil: 0 }.
        attachment(view, VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL, ops, VkClearValue { float32: [clear_depth, 0.0, 0.0, 0.0] })
    }

    /// Record the barriers `pass` needs (one `vkCmdPipelineBarrier2`, none if there are none).
    pub fn barriers(&mut self, dev: &Device, cmd: VkCommandBuffer, pass: usize) {
        let (a, b) = self.plan.barrier_ranges[pass];
        self.record(dev, cmd, a as usize, b as usize);
    }

    /// Record the final transitions (present, readback).
    pub fn finish(&mut self, dev: &Device, cmd: VkCommandBuffer) {
        let (a, b) = self.plan.barrier_ranges[self.plan.decl.passes.len()];
        self.record(dev, cmd, a as usize, b as usize);
    }

    fn record(&mut self, dev: &Device, cmd: VkCommandBuffer, from: usize, to: usize) {
        if from == to {
            return;
        }
        self.image_barriers.clear();
        self.buffer_barriers.clear();
        for b in &self.plan.barriers[from..to] {
            let bound = &self.bound[b.res.0 as usize];
            let (src_stage, src_access) = match b.src {
                Src::Known { stage, access } => (stage, access),
                Src::Pool => bound.prior,
            };
            if self.plan.decl.resources[b.res.0 as usize].is_buffer() {
                self.buffer_barriers.push(VkBufferMemoryBarrier2 {
                    s_type: ST_BUFFER_MEMORY_BARRIER_2,
                    p_next: ptr::null(),
                    src_stage_mask: src_stage,
                    src_access_mask: src_access,
                    dst_stage_mask: b.dst_stage,
                    dst_access_mask: b.dst_access,
                    src_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
                    buffer: bound.buffer,
                    offset: 0,
                    size: VK_WHOLE_SIZE,
                });
            } else {
                self.image_barriers.push(VkImageMemoryBarrier2 {
                    s_type: ST_IMAGE_MEMORY_BARRIER_2,
                    p_next: ptr::null(),
                    src_stage_mask: src_stage,
                    src_access_mask: src_access,
                    dst_stage_mask: b.dst_stage,
                    dst_access_mask: b.dst_access,
                    old_layout: b.old_layout,
                    new_layout: b.new_layout,
                    src_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: VK_QUEUE_FAMILY_IGNORED,
                    image: bound.image.image,
                    subresource_range: crate::device::full_range(bound.image.aspect),
                });
            }
        }
        self.recorded += (to - from) as u32;
        dev.barrier(cmd, &self.buffer_barriers, &self.image_barriers);
    }

    /// The compiled graph as data.
    pub fn info(&self) -> &GraphInfo {
        &self.plan.info
    }

    /// (passes, barriers recorded this frame, recompiled by the last `prepare`, compiles so far).
    pub fn frame_stats(&self) -> crate::GraphStats {
        crate::GraphStats {
            passes: self.plan.decl.passes.len() as u32,
            barriers: self.recorded,
            recompiled: self.recompiled,
            compiles: self.compiles,
        }
    }
}

fn attachment(view: VkImageView, layout: VkImageLayout, ops: Ops, clear: VkClearValue) -> VkRenderingAttachmentInfo {
    VkRenderingAttachmentInfo {
        s_type: ST_RENDERING_ATTACHMENT_INFO,
        p_next: ptr::null(),
        image_view: view,
        image_layout: layout,
        resolve_mode: VK_RESOLVE_MODE_NONE,
        resolve_image_view: VK_NULL_HANDLE,
        resolve_image_layout: VK_IMAGE_LAYOUT_UNDEFINED,
        load_op: ops.load,
        store_op: ops.store,
        clear_value: clear,
    }
}

// ---------------------------------------------------------------------------------------------
// Transient images
// ---------------------------------------------------------------------------------------------

struct Entry {
    owner: u32,
    name: &'static str,
    desc: ImageDesc,
    usage: VkFlags,
    image: Image,
    bound: BoundImage,
    /// Submission number of the last use, and the count of frames the using graph had rendered then
    /// (shared images never go stale).
    last_used: u64,
    owner_frame: u64,
    /// Stages/accesses of its last use (the next frame's first barrier waits on them).
    last: (VkFlags64, VkFlags64),
    /// A newer image with the same owner and name exists: free this one once it is idle.
    replaced: bool,
}

/// The images every render graph owns, created on first use and reused across frames.
#[derive(Default)]
pub struct TransientPool {
    entries: Vec<Entry>,
    /// Frames each graph owner has rendered.
    owners: Vec<(u32, u64)>,
    /// Frames submitted (all graphs).
    submissions: u64,
    /// Tests: fail the creation after this many more succeed.
    #[cfg(test)]
    pub fail_creates: Option<u32>,
}

impl TransientPool {
    fn owner_frames(&self, owner: u32) -> u64 {
        self.owners.iter().find(|o| o.0 == owner).map_or(0, |o| o.1)
    }

    /// The entry for (`owner`, `name`, `desc`, `usage`), created if needed.
    fn acquire(
        &mut self,
        dev: &Device,
        descriptors: &mut Descriptors,
        owner: u32,
        name: &'static str,
        desc: ImageDesc,
        usage: VkFlags,
    ) -> Result<usize, GpuError> {
        let same_name = |e: &Entry| e.owner == owner && (ptr::eq(e.name, name) || e.name == name);
        let found = self.entries.iter().position(|e| e.desc == desc && e.usage == usage && same_name(e));
        match found {
            Some(i) => Ok(i),
            None => {
                #[cfg(test)]
                if self.fail_creates == Some(0) {
                    return Err(GpuError::new("injected transient creation failure"));
                } else if let Some(n) = self.fail_creates.as_mut() {
                    *n -= 1;
                }
                self.entries.push(create_entry(dev, descriptors, owner, name, desc, usage)?);
                Ok(self.entries.len() - 1)
            }
        }
    }

    /// Entry `i` is what its owner and name stand for now: every other image of that owner and
    /// name is replaced (freed once idle), and `i` is not (an image wanted again, e.g. a window
    /// resized back, before it was freed). Called only once a whole frame's transients were
    /// found, so a failed frame never retires an image that is still in use (or still bound in a
    /// descriptor, like the shadow map).
    fn commit(&mut self, i: usize) {
        let (owner, name) = (self.entries[i].owner, self.entries[i].name);
        for (k, e) in self.entries.iter_mut().enumerate() {
            if e.owner == owner && (ptr::eq(e.name, name) || e.name == name) {
                e.replaced = k != i;
            }
        }
    }

    /// `graph`'s frame was submitted: remember when and how its transients were last used.
    /// Returns the submission's number (they count up in submission order): once the GPU finished
    /// it, pass it (or a later one) to [`TransientPool::collect`].
    pub fn note_used<T: Copy + PartialEq + fmt::Debug>(&mut self, graph: &RenderGraph<T>) -> u64 {
        self.submissions += 1;
        let frame_no = self.submissions;
        let owner_frame = match self.owners.iter_mut().find(|o| o.0 == graph.owner) {
            Some(o) => {
                o.1 += 1;
                o.1
            }
            None => {
                self.owners.push((graph.owner, 1));
                1
            }
        };
        for (i, r) in graph.plan.decl.resources.iter().enumerate() {
            if let Kind::Transient { .. } = r.kind {
                let e = &mut self.entries[graph.bound[i].entry];
                e.last_used = frame_no;
                e.owner_frame = owner_frame;
                e.last = graph.plan.last[i];
            }
        }
        frame_no
    }

    /// Free images that were replaced, or that their owner stopped using long ago, once no frame
    /// that may use them is pending: every submission numbered `completed` or lower (see
    /// [`TransientPool::note_used`]) has finished.
    pub fn collect(&mut self, dev: &Device, descriptors: &mut Descriptors, completed: u64) {
        let mut i = 0;
        while i < self.entries.len() {
            let e = &self.entries[i];
            let stale = e.owner != SHARED_OWNER && self.owner_frames(e.owner).saturating_sub(e.owner_frame) >= STALE_FRAMES;
            if (e.replaced || stale) && e.last_used <= completed {
                let e = self.entries.swap_remove(i);
                destroy_entry(dev, descriptors, e);
            } else {
                i += 1;
            }
        }
    }

    /// Free every image of `owner` (its target or window is going away; nothing pending uses them).
    pub fn release_owner(&mut self, dev: &Device, descriptors: &mut Descriptors, owner: u32) {
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].owner == owner {
                let e = self.entries.swap_remove(i);
                destroy_entry(dev, descriptors, e);
            } else {
                i += 1;
            }
        }
        self.owners.retain(|o| o.0 != owner);
    }

    /// An image with `view` as one of its views is alive.
    #[cfg(test)]
    pub fn has_view(&self, view: VkImageView) -> bool {
        self.entries.iter().any(|e| e.bound.view == view || e.bound.array_view == view)
    }

    /// Free everything (the device is idle).
    pub fn destroy(&mut self, dev: &Device, descriptors: &mut Descriptors) {
        for e in self.entries.drain(..) {
            destroy_entry(dev, descriptors, e);
        }
        self.owners.clear();
    }

    /// Live images.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

fn create_entry(
    dev: &Device,
    descriptors: &mut Descriptors,
    owner: u32,
    name: &'static str,
    desc: ImageDesc,
    usage: VkFlags,
) -> Result<Entry, GpuError> {
    let aspect = aspect_of(desc.format);
    let bindless = desc.bindless && usage & VK_IMAGE_USAGE_SAMPLED_BIT != 0;
    let slot = if bindless { descriptors.alloc_slot()? } else { NO_SLOT };
    let image = match dev.create_image_with(desc.format, desc.width, desc.height, 1, desc.layers, usage, aspect, desc.memory, name) {
        Ok(i) => i,
        Err(e) => {
            if slot != NO_SLOT {
                descriptors.free_slot(slot);
            }
            return Err(e);
        }
    };
    let mut bound = BoundImage {
        image: image.handle,
        view: image.view,
        array_view: VK_NULL_HANDLE,
        layer_views: [VK_NULL_HANDLE; MAX_LAYERS],
        width: desc.width,
        height: desc.height,
        layers: desc.layers,
        aspect,
        slot,
    };
    let mut entry = Entry {
        owner,
        name,
        desc,
        usage,
        image,
        bound,
        last_used: 0,
        owner_frame: 0,
        last: (VK_PIPELINE_STAGE_2_NONE, VK_ACCESS_2_NONE),
        replaced: false,
    };
    let views = (|| {
        if desc.layers == 1 {
            bound.layer_views[0] = entry.image.view;
        } else {
            for l in 0..desc.layers {
                bound.layer_views[l as usize] =
                    dev.create_view_range(entry.image.handle, desc.format, aspect, VK_IMAGE_VIEW_TYPE_2D, (0, 1), (l, 1))?;
            }
        }
        if desc.array_view {
            bound.array_view =
                dev.create_view_range(entry.image.handle, desc.format, aspect, VK_IMAGE_VIEW_TYPE_2D_ARRAY, (0, 1), (0, desc.layers))?;
        }
        Ok(())
    })();
    entry.bound = bound;
    if let Err(e) = views {
        destroy_entry(dev, descriptors, entry);
        return Err(e);
    }
    if slot != NO_SLOT {
        descriptors.write_texture(dev, slot, entry.image.view);
    }
    Ok(entry)
}

fn destroy_entry(dev: &Device, descriptors: &mut Descriptors, e: Entry) {
    // SAFETY: views of this device's image, unused by pending work (callers check), destroyed once;
    // null handles are ignored.
    unsafe {
        if e.desc.layers > 1 {
            for &v in &e.bound.layer_views[..e.desc.layers as usize] {
                (dev.fns.destroy_image_view)(dev.handle, v, ptr::null());
            }
        }
        (dev.fns.destroy_image_view)(dev.handle, e.bound.array_view, ptr::null());
    }
    dev.destroy_image(&e.image);
    if e.bound.slot != NO_SLOT {
        descriptors.free_slot(e.bound.slot);
    }
    // Binding 0 points at a view that no longer exists: forget it, so the next frame with shadows
    // rewrites it even if the driver hands the new view the same handle.
    if e.bound.array_view != VK_NULL_HANDLE && descriptors.shadow_view == e.bound.array_view {
        descriptors.shadow_view = VK_NULL_HANDLE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum P {
        Shadow,
        Scene,
        Overlay,
        Fxaa,
        IdBuffer,
        IdCopy,
    }

    const LDR: VkFormat = VK_FORMAT_R8G8B8A8_SRGB;
    const W: u32 = 64;
    const H: u32 = 32;

    fn plan(g: &RenderGraph<P>) -> Result<Plan<P>, GraphError> {
        let mut plan = Plan::default();
        plan.compile(&g.decl, 1).map(|()| plan)
    }

    fn scene_graph(g: &mut RenderGraph<P>, fxaa: bool) -> (Res, Res) {
        g.begin();
        let out = g.import_image("target", LDR, W, H, Prior::Discard { after: VK_PIPELINE_STAGE_2_COPY_BIT }, Final::TransferSrc);
        let depth = g.transient("depth", ImageDesc::depth(VK_FORMAT_D32_SFLOAT, W, H, MemoryKind::RenderTargets));
        let shadow_desc = ImageDesc {
            format: VK_FORMAT_D16_UNORM,
            width: 256,
            height: 256,
            layers: 2,
            array_view: true,
            bindless: false,
            memory: MemoryKind::ShadowMaps,
        };
        let shadow = g.shared("shadow", shadow_desc);
        g.pass("shadow", P::Shadow).depth(shadow, Load::Clear);
        if fxaa {
            let ldr = g.transient("ldr", ImageDesc::color(LDR, W, H, MemoryKind::PostTargets));
            g.pass("scene", P::Scene).color(ldr, Load::Clear).depth(depth, Load::Clear).sampled(shadow);
            g.pass("fxaa", P::Fxaa).sampled(ldr).color(out, Load::DontCare);
        } else {
            g.pass("scene", P::Scene).color(out, Load::Clear).depth(depth, Load::Clear).sampled(shadow);
        }
        g.pass("overlay", P::Overlay).inline().color(out, Load::Keep);
        (out, depth)
    }

    fn barriers(info: &GraphInfo, pass: &str) -> Vec<String> {
        info.passes.iter().find(|p| p.name == pass).unwrap().barriers.iter().map(|b| b.to_string()).collect()
    }

    #[test]
    fn frame_barriers_and_ops() {
        let mut g = RenderGraph::<P>::new(7);
        scene_graph(&mut g, true);
        let p = plan(&g).unwrap();
        let info = &p.info;
        eprintln!("{info}");
        assert_eq!(barriers(info, "shadow"), ["shadow undefined->depth_attachment [all -> early_z+late_z]"]);
        assert_eq!(
            barriers(info, "scene"),
            [
                "ldr undefined->color_attachment [all -> color_out]",
                "depth undefined->depth_attachment [all -> early_z+late_z]",
                "shadow depth_attachment->shader_read [early_z+late_z -> fragment]",
            ]
        );
        assert_eq!(
            barriers(info, "fxaa"),
            ["ldr color_attachment->shader_read [color_out -> fragment]", "target undefined->color_attachment [copy -> color_out]"]
        );
        assert!(barriers(info, "overlay").is_empty());
        assert_eq!(info.final_barriers.len(), 1);
        assert_eq!(info.final_barriers[0].to_string(), "target color_attachment->transfer_src [color_out -> copy]");
        let scene = &info.passes[1];
        let ops: Vec<_> = scene.uses.iter().map(|u| (u.resource, u.load_op, u.store_op)).collect();
        assert_eq!(ops, [("ldr", "clear", "store"), ("depth", "clear", "dont_care"), ("shadow", "-", "-")], "depth is never read again");
        let fxaa = &info.passes[2];
        assert_eq!((fxaa.uses[1].load_op, fxaa.uses[1].store_op), ("dont_care", "store"), "the overlay loads, the readback needs it");
        assert_eq!(info.barrier_count(), 7);
        // Usage comes from the accesses.
        let usage = |n: &str| info.resources.iter().find(|r| r.name == n).unwrap().usage;
        assert_eq!(usage("ldr"), VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT);
        assert_eq!(usage("shadow"), VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT);
        // Direct path: the scene writes the target, the overlay joins it.
        scene_graph(&mut g, false);
        let p = plan(&g).unwrap();
        assert_eq!(barriers(&p.info, "scene")[0], "target undefined->color_attachment [copy -> color_out]");
        assert_eq!(p.info.passes[1].uses[0].store_op, "store");
    }

    /// The recipe in the module docs, as the ID-buffer lane will write it.
    #[test]
    fn id_buffer_recipe_compiles() {
        let mut g = RenderGraph::<P>::new(7);
        let (_, depth) = scene_graph(&mut g, false);
        let ids = g.transient("ids", ImageDesc::color(VK_FORMAT_R32_UINT, W, H, MemoryKind::RenderTargets));
        g.pass("id_buffer", P::IdBuffer).color(ids, Load::Clear).depth_read(depth);
        let readback = g.import_buffer("id_readback", (W * H * 4) as u64, Prior::Discard { after: VK_PIPELINE_STAGE_2_HOST_BIT }, Final::HostRead);
        g.pass("id_copy", P::IdCopy).transfer_src(ids).transfer_dst(readback);
        let p = plan(&g).unwrap();
        let info = &p.info;
        eprintln!("{info}");
        assert_eq!(info.passes[1].uses[1].store_op, "store", "the scene's depth is kept for the id pass");
        assert_eq!(
            barriers(info, "id_buffer"),
            ["ids undefined->color_attachment [all -> color_out]", "depth [early_z+late_z -> early_z+late_z]"]
        );
        assert_eq!(
            barriers(info, "id_copy"),
            ["ids color_attachment->transfer_src [color_out -> copy]", "id_readback [host -> copy]"]
        );
        let ends: Vec<_> = info.final_barriers.iter().map(|b| b.to_string()).collect();
        assert_eq!(ends, ["target color_attachment->transfer_src [color_out -> copy]", "id_readback [copy -> host]"]);
        let id_pass = &info.passes[3];
        assert_eq!((id_pass.uses[0].load_op, id_pass.uses[0].store_op), ("clear", "store"));
        assert_eq!(id_pass.uses[1].store_op, "dont_care", "nothing reads the depth after the id pass");
    }

    #[test]
    fn mistakes_are_structured_errors() {
        let t = |g: &mut RenderGraph<P>| {
            g.begin();
            let a = g.transient("a", ImageDesc::color(LDR, W, H, MemoryKind::PostTargets));
            let b = g.transient("b", ImageDesc::color(LDR, W / 2, H, MemoryKind::PostTargets));
            let out = g.import_image("out", LDR, W, H, Prior::Discard { after: 0 }, Final::Present);
            (a, b, out)
        };
        let mut g = RenderGraph::<P>::new(1);
        let (a, _, out) = t(&mut g);
        g.pass("fxaa", P::Fxaa).sampled(a).color(out, Load::DontCare);
        assert_eq!(plan(&g).err(), Some(GraphError::ReadBeforeWrite { pass: "fxaa", resource: "a" }));

        let (a, _, out) = t(&mut g);
        g.pass("scene", P::Scene).color(a, Load::Clear);
        g.pass("fxaa", P::Fxaa).color(a, Load::Clear);
        g.pass("x", P::IdBuffer).sampled(a).color(out, Load::Clear);
        assert_eq!(plan(&g).err(), Some(GraphError::UnreadOverwrite { pass: "fxaa", resource: "a", written_by: "scene" }));

        let (a, b, _) = t(&mut g);
        g.pass("scene", P::Scene).color(a, Load::Clear).color(b, Load::Clear);
        assert_eq!(
            plan(&g).err(),
            Some(GraphError::SizeMismatch { pass: "scene", resource: "b", width: W / 2, height: H, expected: (W, H) })
        );

        let (a, _, _) = t(&mut g);
        g.pass("scene", P::Scene).color(a, Load::Clear).sampled(a);
        assert_eq!(plan(&g).err(), Some(GraphError::DuplicateUse { pass: "scene", resource: "a" }));

        let (a, _, _) = t(&mut g);
        g.pass("scene", P::Scene).depth(a, Load::Clear);
        assert_eq!(plan(&g).err(), Some(GraphError::WrongAccess { pass: "scene", resource: "a", access: Access::Depth(Load::Clear) }));

        t(&mut g);
        g.pass("scene", P::Scene).color(Res(40), Load::Clear);
        assert_eq!(plan(&g).err(), Some(GraphError::UnknownResource { pass: "scene", index: 40 }));

        let (a, b, out) = t(&mut g);
        g.pass("scene", P::Scene).color(a, Load::Clear);
        g.pass("overlay", P::Overlay).inline().sampled(a).color(b, Load::Keep);
        g.pass("fxaa", P::Fxaa).sampled(a).color(out, Load::Clear);
        assert_eq!(plan(&g).err(), Some(GraphError::InlineNeedsBarrier { pass: "overlay", resource: "a" }));

        let (a, _, _) = t(&mut g);
        g.pass("overlay", P::Overlay).inline().color(a, Load::Keep);
        assert_eq!(plan(&g).err(), Some(GraphError::InlineFirst { pass: "overlay" }));

        let (a, _, _) = t(&mut g);
        g.pass("scene", P::Scene).color(a, Load::Clear);
        assert_eq!(plan(&g).err(), Some(GraphError::NeverWritten { resource: "out" }));

        g.begin();
        g.transient("z", ImageDesc::color(LDR, 0, 4, MemoryKind::PostTargets));
        assert_eq!(plan(&g).err(), Some(GraphError::BadDesc { resource: "z" }));

        // Same name and description: one pool image for two resources (garbage sampling).
        let (a, _, out) = t(&mut g);
        let a2 = g.transient("a", ImageDesc::color(LDR, W, H, MemoryKind::PostTargets));
        g.pass("scene", P::Scene).color(a, Load::Clear).color(a2, Load::Clear);
        g.pass("fxaa", P::Fxaa).sampled(a).sampled(a2).color(out, Load::Clear);
        assert_eq!(plan(&g).err(), Some(GraphError::DuplicateName { resource: "a" }));
        // A shared and a private transient may share a name (different owners).
        let (a, _, out) = t(&mut g);
        let s = g.shared("a", ImageDesc::color(LDR, W, H, MemoryKind::PostTargets));
        g.pass("scene", P::Scene).color(a, Load::Clear).color(s, Load::Clear);
        g.pass("fxaa", P::Fxaa).sampled(a).sampled(s).color(out, Load::Clear);
        assert!(plan(&g).is_ok());

        // Inside a GpuError: machine-readable reason plus words.
        let e = graph_error(GraphError::ReadBeforeWrite { pass: "fxaa", resource: "a" });
        assert_eq!(e.reason, Some(ErrorReason::Graph(GraphError::ReadBeforeWrite { pass: "fxaa", resource: "a" })));
        assert!(e.message.contains("fxaa") && e.message.contains("'a'"), "{e}");
    }

    #[test]
    fn unchanged_declarations_reuse_the_plan() {
        let mut g = RenderGraph::<P>::new(3);
        scene_graph(&mut g, true);
        let p = plan(&g).unwrap();
        g.plan = p;
        g.compiled = true;
        scene_graph(&mut g, true);
        assert!(g.plan.decl == g.decl, "the same frame declares the same graph");
        scene_graph(&mut g, false);
        assert!(g.plan.decl != g.decl, "a different pass set recompiles");
        // A size change too.
        g.begin();
        g.import_image("target", LDR, W + 1, H, Prior::Discard { after: 0 }, Final::TransferSrc);
        assert!(g.plan.decl != g.decl);
    }

    #[test]
    fn stage_and_layout_names() {
        assert_eq!(stage_names(0), "none");
        assert_eq!(stage_names(VK_PIPELINE_STAGE_2_COPY_BIT | VK_PIPELINE_STAGE_2_HOST_BIT), "copy+host");
        assert_eq!(layout_name(VK_IMAGE_LAYOUT_PRESENT_SRC_KHR), "present");
        assert!(depth_format(VK_FORMAT_D16_UNORM) && !depth_format(VK_FORMAT_R32_UINT));
    }
}
