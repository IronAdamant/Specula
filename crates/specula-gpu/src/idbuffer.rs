//! Sight's ID buffer: which draws cover which pixels of an offscreen capture.
//!
//! [`crate::Gpu::render_with_ids`] renders a frame as usual and, in the same submission, three
//! more graph passes declared after the frame's own (so the frame's image is unchanged bit for
//! bit). Every count is **reproducible**: identical frames give identical numbers.
//!
//! - `id_buffer`, one rendering instance with its own D32 depth, over the frame's visible draw
//!   records (`shaders/ids.vert`):
//!   1. **opaque** draws (`Material::Lit`) write the caller's id into an `R32_UINT` image, depth
//!      tested and written: each pixel ends with its nearest opaque draw, as in the scene (ties go
//!      to the first drawn, as in the scene). Counted on the CPU (below);
//!   2. **insert**: blended draws (glass, blob shadows, lava lamps), depth tested against that
//!      opaque depth and never writing it, put the id of every fragment the scene shows
//!      ([`faces_for`]: all of them, except the lava lamp's, which mirror `shade_lava_lamp`'s only
//!      `discard`) into their pixel's **set of the [`SEEN_SLOTS`] smallest ids**: unordered slots
//!      `(capture epoch << SEEN_ID_BITS) | id`, changed only by `atomicCompSwap`, at most one
//!      successful swap per id and pixel whatever the arrival order: an id claims the first free
//!      slot, or replaces the largest of a full set when smaller (the replaced id is out for
//!      good), or is out itself; ids out of a set are marked *incomplete* (a plain flag store, no
//!      counting). A slot only ever decreases within a capture, so the final set is the K smallest
//!      ids whatever order the GPU runs the fragments in. Slots tagged with another capture's epoch
//!      count as free, so they are never cleared between captures (only when new or when the
//!      12-bit epoch wraps: clearing 16 bytes a pixel costs an iGPU many milliseconds at 2048²).
//!      Frames without blended draws skip all of this;
//!   3. **raster**, every draw again with no depth test and the same face rules, marks each id
//!      rasterized at all (one flag): an id with no visible pixel but rasterized fragments was
//!      hidden by something (`occluded`); one never rasterized was sub-pixel or culled.
//!
//!   **Occluders** ([`crate::Gpu::render_with_ids_occluded`]: a game's opaque interface panels,
//!   drawn over the frame as overlay quads) come first: `vkCmdClearAttachments` sets the ID depth
//!   to the near plane inside each rectangle, so steps 1 and 2 (depth tested, strictly nearer)
//!   count nothing there, while step 3 (no depth test) still marks what lies under them as
//!   rasterized: an entity entirely under a panel is `occluded`, one partly under it counts only
//!   its uncovered pixels. The id image stays 0 (no id) there. One clear per rectangle, before
//!   any draw; nothing else changes, and without occluders nothing is recorded.
//! - `id_resolve` (depth only, read): a full-screen pass counts each id of each pixel's final set
//!   once (pixels and min/max box, with atomics: sums and extremes don't depend on order; only
//!   pixels on the edge of an id's region touch the atomics, as runs and extremes, because
//!   contended atomics are what iGPUs are slowest at: see `perf/README.md`). An *incomplete* id
//!   (more than [`SEEN_SLOTS`] blended ids overlapped at some pixel and it wasn't among the
//!   smallest) keeps the count and box of the pixels where it is in the set: **lower bounds**,
//!   reported `approx`. Which ids those are, and their counts, follow from the final sets, so this
//!   is reproducible too, and no pass counts per fragment or contends on one address per
//!   fragment: the cost grows with layers × pixels like the scene's own blended overdraw (about
//!   as much again), never serialised. Ids at or above 2^[`SEEN_ID_BITS`] never fit a slot:
//!   0 pixels, approx.
//! - `id_copy` copies the opaque ids to one shared host-visible buffer, and the stats to a small
//!   host buffer. The opaque ids are counted on the CPU in place, run-length per row, in parallel
//!   for big images ([`IdCounts`]).
//!
//! So any number of layers of one id at a pixel (both faces of a shell, a concave cup or torus,
//! the camera inside a glass box, glass's two passes) counts that pixel once, exactly, while at
//! most [`SEEN_SLOTS`] different blended ids overlap there. Glass behind glass is counted; glass
//! never hides anything. A blended model whose own glass lies over its own opaque part counts
//! that pixel in both. Blended materials count their whole mesh: a lava lamp its proxy box, a
//! blob shadow its quad (including its faded corners). (Earlier attempts chose faces by mesh
//! topology, or kept only the first ids to arrive; the first broke on seams, concave shells and
//! cameras inside glass, the second on order.)
//!
//! The blended steps need `fragmentStoresAndAtomics` (every desktop GPU and the Steam Deck);
//! without it (or with [`crate::Gpu::set_ids_count_blended`] off) blended draws go uncounted: ids
//! with blended draws report `counted: false` and `rasterized` is unknown
//! ([`IdCounts::blended_counted`]).
//!
//! Memory: the id image and its depth are graph images *shared by every target*, sized up to a
//! bucket ([`bucket_for`]: the largest capture so far, rounded up to 256 px) and rendered with a
//! render area of the capture's size; the readback is one buffer for all targets, and so are the
//! slots (16 bytes per pixel of the bucket, exactly, once a capture has blended draws; counted
//! as render targets, for the `Gpu`'s life). So caching
//! four capture sizes costs one set, not four.
//!
//! The stats buffer and the slots are private to these passes (the graph has no fragment-storage
//! access kind): `id_buffer` clears the stats (and the slots when needed) and makes the clears
//! visible to fragment shaders; `id_resolve` makes the inserts visible to the resolve; `id_copy`
//! makes the counts visible to the copy and the copy to the host. Offscreen renders are
//! synchronous, so nothing earlier still uses them.
//!
//! **The scene's streams and discards, mirrored.** Every ID pipeline that draws meshes shares
//! `shaders/ids_vert.glsl` in the scene's four stream variants (plain, skinned, textured, both:
//! `PipelineKey::Ids { skinned, textured }`), chosen per batch exactly as the scene chooses them.
//! Skinned meshes are posed in `id_model()` from the frame's bone palettes as `mesh_vert.glsl`
//! poses them; the skinned and textured ID pipelines are built in the background with the scene's
//! variants (`render::id_keys`), never in a frame, and until they are ready their draws are
//! skipped, as the scene skips them. Textured draws cut alpha-masked texels (`cut_out` in
//! `ids.glsl`: the base colour's alpha against the draw's cutoff, as `mesh_frag.glsl` and the
//! masked shadow casters cut) in the opaque pass and in `shown()`. Anything new that moves vertices
//! (morphs, wind) or discards fragments must be mirrored here, or the ID buffer disagrees with
//! the image.
//!
//! **2D items** (sprites, tile chunks, vector shapes) take ids too: `render_with_ids`'s ids list
//! the mesh draws then the 2D draws, and each step draws the 2D plan's batches after its meshes,
//! with the scene's own vertex code and texel cuts (`ids_sprite.glsl`, `ids_shape.frag`, sharing
//! `sprite_vert.glsl` / `sprite_texel.glsl` / `shape_vert.glsl` with the scene): cutouts write
//! opaque ids (depth tested "or equal" and written, as in the scene), blended items and shapes
//! join the id sets, and every item marks itself rasterized. Their shaders find the capture's
//! buffers through an [`IdParams2d`] pointer in the 2D push constants' padding.
//!
//! The window path never declares these passes.

use crate::batch::Plan;
use crate::device::{buffer_barrier, Buffer, Device};
use crate::graph::{Final, ImageDesc, Load, Prior, Res};
use crate::render::{FrameContext, FrameGraph, FramePass, PipelineKey, Pipelines, DEPTH_FORMAT, PUSH_STAGES};
use crate::vk::*;
use crate::{GpuError, MemoryKind};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

/// The opaque id image's format.
pub const ID_FORMAT: VkFormat = VK_FORMAT_R32_UINT;
/// Bytes per pixel of the id image and its readback.
pub const BYTES_PER_PIXEL: u64 = 4;
/// `u32`s per id in the stats buffer; must match `STATS_STRIDE` in `shaders/ids.glsl`.
pub const STATS_STRIDE: usize = 7;
/// Shared id images grow in steps of this many pixels per side.
pub const BUCKET: u32 = 256;
/// `VK_PIPELINE_STAGE_2_ALL_TRANSFER_BIT` (copies and fills).
const ALL_TRANSFER: VkFlags64 = 0x1000;

/// The ID pipelines (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdPass {
    /// `id_buffer`: opaque ids into the image.
    Opaque,
    /// `id_buffer`: blended ids into the per-pixel sets.
    Insert,
    /// `id_buffer`: rasterized-at-all flags.
    Raster,
    /// `id_resolve`: counts the sets (full screen).
    Resolve,
}

/// Push constants of the ID pipelines; must match `Push` in `shaders/ids.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IdPush {
    pub frame: u64,
    pub draws: u64,
    pub ids: u64,
    pub stats: u64,
    /// The per-pixel "ids already counted here" slots (see the module docs).
    pub seen: u64,
    /// Which fragments the scene shows (`FACES_*`), per batch.
    pub faces: u32,
    /// Pixels per row of the capture (indexes `seen`).
    pub width: u32,
    /// This capture's tag in `seen` (1..=[`MAX_EPOCH`]).
    pub epoch: u32,
    /// Rows of the capture.
    pub height: u32,
    /// The frame's bone palettes (skinned draws; `FrameUpload::bones_address`).
    pub bones: u64,
}


/// What the 2D ID shaders read through their push constants' padding; must match `IdParams` in
/// `shaders/sprite.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IdParams2d {
    pub ids: u64,
    pub stats: u64,
    pub seen: u64,
    pub width: u32,
    pub epoch: u32,
}

/// Every fragment the rasteriser produces is shown: opaque draws, glass (the scene draws its back
/// faces, then its front faces), blob shadows.
pub const FACES_ALL: u32 = 0;
/// The lava lamp's proxy box, as `shade_lava_lamp` shades it (its only `discard`): near faces, or
/// far faces when the camera is inside the box.
pub const FACES_LAVA: u32 = 2;
/// Blended ids kept per pixel: the smallest [`SEEN_SLOTS`], in `u32` slots (16 bytes a pixel).
pub const SEEN_SLOTS: u64 = 4;
/// Bits of a slot holding the id (`SEEN_ID_BITS` in `shaders/ids.glsl`); the rest tag the capture.
pub const SEEN_ID_BITS: u32 = 20;
/// Captures before the slot tags wrap (and the slots are cleared once).
pub const MAX_EPOCH: u32 = (1 << (32 - SEEN_ID_BITS)) - 1;
/// `VK_WHOLE_SIZE`.
const VK_WHOLE_SIZE: u64 = u64::MAX;

/// The fragments of `material` the scene shows, mirroring `mesh.frag`'s `discard`s exactly
/// (glass discards per pass, but its two passes together show every fragment).
pub fn faces_for(material: crate::Material) -> u32 {
    match material {
        crate::Material::LavaLamp => FACES_LAVA,
        _ => FACES_ALL,
    }
}

/// The ID passes' resources in a frame's graph.
#[derive(Clone, Copy, Debug, Default)]
pub struct IdRes {
    pub ids: Res,
    pub depth: Res,
    pub readback: Res,
}

/// The shared id images' size for a capture of `extent`, given the bucket so far: never smaller
/// than before (so alternating capture sizes never reallocate), rounded up to [`BUCKET`].
pub fn bucket_for(extent: VkExtent2D, previous: (u32, u32)) -> (u32, u32) {
    let up = |v: u32| v.div_ceil(BUCKET).max(1) * BUCKET;
    (previous.0.max(up(extent.width)), previous.1.max(up(extent.height)))
}

/// Declares the ID passes after the frame's own passes in `g`; the id images are `bucket` sized.
pub fn declare(g: &mut FrameGraph, extent: VkExtent2D, bucket: (u32, u32)) -> IdRes {
    let ids = g.shared("ids", ImageDesc::color(ID_FORMAT, bucket.0, bucket.1, MemoryKind::RenderTargets));
    let depth = g.shared("ids_depth", ImageDesc::depth(DEPTH_FORMAT, bucket.0, bucket.1, MemoryKind::RenderTargets));
    g.pass("id_buffer", FramePass::IdBuffer).color(ids, Load::Clear).depth(depth, Load::Clear);
    g.pass("id_resolve", FramePass::IdResolve).depth_read(depth);
    let bytes = extent.width as u64 * extent.height as u64 * BYTES_PER_PIXEL;
    let readback = g.import_buffer("id_readback", bytes, Prior::Discard { after: VK_PIPELINE_STAGE_2_HOST_BIT }, Final::HostRead);
    g.pass("id_copy", FramePass::IdCopy).transfer_src(ids).transfer_dst(readback);
    IdRes { ids, depth, readback }
}

/// What recording the ID passes needs beyond the frame's context.
#[derive(Clone, Copy, Debug)]
pub struct IdFrame {
    /// By streams: `skinned as usize | (textured as usize) << 1` (`VK_NULL_HANDLE`: that
    /// variant isn't built yet, so its draws are skipped, as the scene skips them).
    pub opaque: [VkPipeline; 4],
    /// The counting passes' pipelines (`VK_NULL_HANDLE` when they don't run: no fragment
    /// atomics, or no blended draws for insert and resolve). Insert by `skinned as usize`.
    pub insert: [VkPipeline; 2],
    pub raster: [VkPipeline; 4],
    pub resolve: VkPipeline,
    /// 2D items: sprites and tile chunks by step (opaque = cutouts, insert, raster) and vector
    /// shapes (insert, raster); `VK_NULL_HANDLE` where they don't run.
    pub sprite: [VkPipeline; 3],
    pub shape: [VkPipeline; 2],
    /// Device address of this capture's [`IdParams2d`] (0 without 2D draws).
    pub params_2d: u64,
    pub layout: VkPipelineLayout,
    /// Device address of the per-record ids.
    pub ids_address: u64,
    /// The stats buffer (device), its address and size, and its host copy (null without atomics).
    pub stats: VkBuffer,
    pub stats_address: u64,
    pub stats_bytes: u64,
    pub stats_host: VkBuffer,
    /// The per-pixel id sets (device) and their address (null without blended draws).
    pub seen: VkBuffer,
    pub seen_address: u64,
    /// Clear the slots first (new buffer, or the epoch wrapped); and the epoch to tag them with.
    pub seen_clear: bool,
    pub epoch: u32,
    pub res: IdRes,
    pub readback: VkBuffer,
    pub extent: VkExtent2D,
}

/// The id of each draw record, in record order (`plan.entries`), from one id per draw.
pub fn record_ids(plan: &Plan, draw_ids: &[u32], out: &mut Vec<u32>) {
    out.clear();
    out.extend(plan.entries.iter().map(|&(i, _)| draw_ids.get(i).copied().unwrap_or(0)));
}

/// Fragment-shader storage writes made visible to fragment-shader storage reads and writes.
fn fragment_to_fragment(buffer: VkBuffer) -> VkBufferMemoryBarrier2 {
    buffer_barrier(
        buffer,
        VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
        VK_ACCESS_2_SHADER_STORAGE_WRITE_BIT,
        VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
        VK_ACCESS_2_SHADER_STORAGE_READ_BIT | VK_ACCESS_2_SHADER_STORAGE_WRITE_BIT,
    )
}

/// Records ID pass `p` (the graph recorded its barriers; the storage buffers' are recorded here:
/// only fragment shaders access them, so their barriers name the fragment stage only).
pub fn record(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, tag: FramePass, f: &IdFrame) {
    let dev = ctx.dev;
    match tag {
        FramePass::IdBuffer => {
            if f.stats != VK_NULL_HANDLE {
                // SAFETY: recording, outside any rendering instance; the stats buffer holds
                // stats_bytes (a multiple of 4), the slots are filled whole, and nothing pending
                // uses either (offscreen renders are synchronous).
                unsafe {
                    (dev.fns.cmd_fill_buffer)(cmd, f.stats, 0, f.stats_bytes, 0);
                    if f.seen_clear {
                        (dev.fns.cmd_fill_buffer)(cmd, f.seen, 0, VK_WHOLE_SIZE, 0);
                    }
                }
                let clear_to_atomics = |buffer| {
                    buffer_barrier(
                        buffer,
                        ALL_TRANSFER,
                        VK_ACCESS_2_TRANSFER_WRITE_BIT,
                        VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
                        VK_ACCESS_2_SHADER_STORAGE_READ_BIT | VK_ACCESS_2_SHADER_STORAGE_WRITE_BIT,
                    )
                };
                if f.seen_clear {
                    dev.barrier(cmd, &[clear_to_atomics(f.stats), clear_to_atomics(f.seen)], &[]);
                } else {
                    dev.barrier(cmd, &[clear_to_atomics(f.stats)], &[]);
                }
            }
            record_id_buffer(ctx, cmd, g, p, f);
        }
        FramePass::IdResolve => {
            if f.resolve != VK_NULL_HANDLE {
                // The inserts (and incomplete flags) are complete before the resolve reads them.
                dev.barrier(cmd, &[fragment_to_fragment(f.stats), fragment_to_fragment(f.seen)], &[]);
                record_id_resolve(ctx, cmd, g, p, f);
            }
        }
        FramePass::IdCopy => {
            let region = VkBufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: VkImageSubresourceLayers {
                    aspect_mask: VK_IMAGE_ASPECT_COLOR_BIT,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: VkOffset3D { x: 0, y: 0, z: 0 },
                image_extent: VkExtent3D { width: f.extent.width, height: f.extent.height, depth: 1 },
            };
            let image = g.image(f.res.ids).image;
            // SAFETY: recording; the graph moved the ids image to TRANSFER_SRC_OPTIMAL; the copy
            // region (the capture's size) lies inside the bucket-sized image, and the readback
            // buffer holds width × height × 4 bytes.
            unsafe { (dev.fns.cmd_copy_image_to_buffer)(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, f.readback, 1, &region) };
            if f.stats != VK_NULL_HANDLE {
                let atomics_to_copy = buffer_barrier(
                    f.stats,
                    VK_PIPELINE_STAGE_2_FRAGMENT_SHADER_BIT,
                    VK_ACCESS_2_SHADER_STORAGE_WRITE_BIT,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_READ_BIT,
                );
                dev.barrier(cmd, &[atomics_to_copy], &[]);
                let copy = VkBufferCopy { src_offset: 0, dst_offset: 0, size: f.stats_bytes };
                // SAFETY: recording; both buffers hold stats_bytes.
                unsafe { (dev.fns.cmd_copy_buffer)(cmd, f.stats, f.stats_host, 1, &copy) };
                let copy_to_host = buffer_barrier(
                    f.stats_host,
                    VK_PIPELINE_STAGE_2_COPY_BIT,
                    VK_ACCESS_2_TRANSFER_WRITE_BIT,
                    VK_PIPELINE_STAGE_2_HOST_BIT,
                    VK_ACCESS_2_HOST_READ_BIT,
                );
                dev.barrier(cmd, &[copy_to_host], &[]);
            }
        }
        _ => unreachable!("not an ID pass"),
    }
}

/// The push constants every ID pipeline shares.
fn push_for(ctx: &FrameContext, f: &IdFrame) -> IdPush {
    IdPush {
        frame: ctx.upload.frame_address(),
        draws: ctx.upload.records_address(),
        ids: f.ids_address,
        stats: f.stats_address,
        seen: f.seen_address,
        faces: FACES_ALL,
        width: f.extent.width,
        epoch: f.epoch,
        height: f.extent.height,
        bones: ctx.upload.bones_address(ctx.scratch.records.len(), ctx.scratch.overlay.len()),
    }
}

/// One step of `id_buffer`: which batches it draws and with which of its pipelines.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Opaque,
    Insert,
    Raster,
}

impl Step {
    /// The pipeline for a batch with these streams (`VK_NULL_HANDLE`: not drawn, as in the
    /// scene, whose variant isn't built either).
    fn pipeline(self, f: &IdFrame, skinned: bool, textured: bool) -> VkPipeline {
        let streams = skinned as usize | (textured as usize) << 1;
        match self {
            Step::Opaque => f.opaque[streams],
            // Blended draws are never textured (`batch::textured`).
            Step::Insert => f.insert[skinned as usize],
            Step::Raster => f.raster[streams],
        }
    }

    fn wants(self, blended: bool) -> bool {
        match self {
            Step::Opaque => !blended,
            Step::Insert => blended,
            Step::Raster => true,
        }
    }
}

/// Draws the plan's batches `step` wants, each with its streams' pipeline (skinned meshes posed
/// like the scene, textured ones with its alpha-mask cut), pushing each batch's face rule. Glass
/// is planned twice (back faces, then front faces); it is drawn once (every face of it either
/// way). Batches whose pipeline variant isn't built are skipped (the scene skips them too).
///
/// # Safety
/// `cmd` is recording inside a rendering instance, with the descriptor set bound for
/// `f.layout`; every ID pipeline uses `f.layout`; mesh buffers are live (`check_frame`); the
/// frame upload holds one record per plan entry (and the frame's bone palettes) and the ids
/// buffer one id per record.
unsafe fn draw_batches(ctx: &FrameContext, cmd: VkCommandBuffer, f: &IdFrame, step: Step, compare: i32) {
    let dev = ctx.dev;
    let mut push = push_for(ctx, f);
    let mut pushed = None;
    let mut bound = VK_NULL_HANDLE;
    let mut bound_mesh = None;
    let twice = |b: &&crate::batch::Batch| b.material == crate::Material::Glass && b.pass == 0;
    for batch in ctx.scratch.plan.batches.iter().filter(|b| step.wants(b.material.blended()) && !twice(b)) {
        let Some(mesh) = ctx.meshes.get(&batch.mesh.0) else { continue };
        // The scene's choice of streams (render.rs), so the same variants draw.
        let skinned = batch.skinned && mesh.skin_offset.is_some();
        let textured = batch.textured && mesh.surface_offset.is_some();
        let pipeline = step.pipeline(f, skinned, textured);
        if pipeline == VK_NULL_HANDLE {
            continue;
        }
        push.faces = if step == Step::Opaque { FACES_ALL } else { faces_for(batch.material) };
        // SAFETY: as the caller promises; every referenced struct outlives its call.
        unsafe {
            if pipeline != bound {
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
                if step != Step::Raster {
                    // Dynamic in the depth-tested pipelines (the raster ones don't test depth).
                    (dev.fns.cmd_set_depth_compare_op)(cmd, compare);
                }
                bound = pipeline;
            }
            if pushed != Some(push.faces) {
                (dev.fns.cmd_push_constants)(cmd, f.layout, PUSH_STAGES, 0, size_of::<IdPush>() as u32, &push as *const IdPush as *const c_void);
                pushed = Some(push.faces);
            }
            if bound_mesh != Some(batch.mesh) {
                // Every stream the mesh has (the pipeline reads those its variant declares).
                mesh.bind(dev, cmd);
                bound_mesh = Some(batch.mesh);
            }
            (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, batch.count, 0, 0, batch.first);
        }
    }
}

/// Draws the frame's 2D batches `step` wants (opaque: cutout sprites and tile chunks; insert:
/// blended ones and shapes; raster: all), in the scene's order, with the scene's depth test (or
/// equal: a later layer wins ties, as in the scene).
///
/// # Safety
/// `cmd` is recording inside the `id_buffer` rendering instance, the descriptor set bound for
/// `f.layout`; the 2D plan's records are in the frame upload; shape meshes are live (`check_2d`).
unsafe fn draw_2d(ctx: &FrameContext, cmd: VkCommandBuffer, f: &IdFrame, step: Step) {
    use crate::sprite::{BatchKind, QUAD_MESH};
    let plan = &ctx.scratch.two_d;
    if plan.batches.is_empty() || f.params_2d == 0 {
        return;
    }
    let dev = ctx.dev;
    let items = ctx.upload.items_address(ctx.scratch.records.len(), ctx.scratch.overlay.len(), ctx.frame.bones.len());
    let mut push = plan.push(ctx.upload.frame_address(), items);
    // The padding after `mode` holds the IdParams2d pointer (see `shaders/sprite.glsl`).
    push.pad = [f.params_2d as u32, (f.params_2d >> 32) as u32];
    let compare = if ctx.frame.reverse_z { VK_COMPARE_OP_GREATER_OR_EQUAL } else { VK_COMPARE_OP_LESS_OR_EQUAL };
    let mut bound = VK_NULL_HANDLE;
    let mut bound_mesh = None;
    for b in &plan.batches {
        let pipeline = match (step, b.kind) {
            (Step::Opaque, BatchKind::Cutout) => f.sprite[0],
            (Step::Insert, BatchKind::Blended) => f.sprite[1],
            (Step::Insert, BatchKind::Shape(_)) => f.shape[0],
            (Step::Raster, BatchKind::Shape(_)) => f.shape[1],
            (Step::Raster, _) => f.sprite[2],
            _ => VK_NULL_HANDLE,
        };
        if pipeline == VK_NULL_HANDLE {
            continue;
        }
        // SAFETY: as the caller promises; every referenced struct outlives its call; the push
        // covers SpritePush's 128 bytes at most.
        unsafe {
            if pipeline != bound {
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
                if bound == VK_NULL_HANDLE {
                    (dev.fns.cmd_push_constants)(
                        cmd,
                        f.layout,
                        PUSH_STAGES,
                        0,
                        size_of::<crate::sprite::SpritePush>() as u32,
                        &push as *const crate::sprite::SpritePush as *const c_void,
                    );
                }
                if step != Step::Raster {
                    (dev.fns.cmd_set_depth_compare_op)(cmd, compare);
                }
                bound = pipeline;
            }
            match b.kind {
                BatchKind::Shape(mesh) => {
                    let Some(m) = ctx.meshes.get(&mesh.0) else { continue };
                    if bound_mesh != Some(mesh.0) {
                        let offset: VkDeviceSize = 0;
                        (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &m.buffer.handle, &offset);
                        (dev.fns.cmd_bind_index_buffer)(cmd, m.buffer.handle, m.index_offset, VK_INDEX_TYPE_UINT32);
                        bound_mesh = Some(mesh.0);
                    }
                    (dev.fns.cmd_draw_indexed)(cmd, m.index_count, b.count, 0, 0, b.first);
                }
                _ => {
                    let Some(q) = ctx.meshes.get(&QUAD_MESH) else { continue };
                    if bound_mesh != Some(QUAD_MESH) {
                        (dev.fns.cmd_bind_index_buffer)(cmd, q.buffer.handle, q.index_offset, VK_INDEX_TYPE_UINT32);
                        bound_mesh = Some(QUAD_MESH);
                    }
                    (dev.fns.cmd_draw_indexed)(cmd, 6, b.count, 0, 0, b.first);
                }
            }
        }
    }
}

fn record_id_buffer(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, f: &IdFrame) {
    let (dev, frame) = (ctx.dev, ctx.frame);
    let color = g.color_attachment(p, f.res.ids, [0.0; 4]); // 0 bits: no id
    let depth = g.depth_attachment(p, f.res.depth, if frame.reverse_z { 0.0 } else { 1.0 }, None);
    // The capture's corner of the bucket-sized images.
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: f.extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 1,
        p_color_attachments: &color,
        p_depth_attachment: &depth,
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: f.extent.width as f32, height: f.extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
    let compare = if frame.reverse_z { VK_COMPARE_OP_GREATER } else { VK_COMPARE_OP_LESS };
    // SAFETY: `cmd` is recording; every referenced struct outlives its call; the descriptor set
    // (material textures, for the alpha-mask cut) is compatible with `f.layout`, every ID
    // pipeline's layout; see `draw_batches` for the draws. The occluder clear names the instance's
    // depth attachment and rectangles inside its render area (clamped by `set_occluders`), one layer.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        ctx.descriptors.bind(dev, cmd, f.layout);
        if !ctx.id_occluders.is_empty() {
            // Occluders: the depth at the near plane, which no draw passes (strictly nearer for
            // meshes; a 2D item exactly on the near plane, 5 cm from the eye, would).
            let near = VkClearAttachment {
                aspect_mask: VK_IMAGE_ASPECT_DEPTH_BIT,
                color_attachment: 0,
                clear_value: VkClearValue { float32: [if frame.reverse_z { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0] },
            };
            // Every rectangle lies inside the render area (`set_occluders` clamped them).
            (dev.fns.cmd_clear_attachments)(cmd, 1, &near, ctx.id_occluders.len() as u32, ctx.id_occluders.as_ptr());
        }
        // Opaque draws, then blended ones, then all of them; each step's 2D items after its
        // meshes (as the scene draws them after its opaque meshes).
        for step in [Step::Opaque, Step::Insert, Step::Raster] {
            draw_batches(ctx, cmd, f, step, compare);
            draw_2d(ctx, cmd, f, step);
        }
        (dev.fns.cmd_end_rendering)(cmd);
    }
}

/// `id_resolve`: the full-screen resolve of the per-pixel sets.
fn record_id_resolve(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, f: &IdFrame) {
    let (dev, frame) = (ctx.dev, ctx.frame);
    let depth = g.depth_attachment(p, f.res.depth, if frame.reverse_z { 0.0 } else { 1.0 }, None);
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: f.extent };
    let rendering = VkRenderingInfo {
        s_type: ST_RENDERING_INFO,
        p_next: ptr::null(),
        flags: 0,
        render_area: area,
        layer_count: 1,
        view_mask: 0,
        color_attachment_count: 0,
        p_color_attachments: ptr::null(),
        p_depth_attachment: &depth,
        p_stencil_attachment: ptr::null(),
    };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: f.extent.width as f32, height: f.extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
    let push = push_for(ctx, f);
    // SAFETY: `cmd` is recording; every referenced struct outlives its call; the pipeline uses
    // `f.layout` and renders depth-only (the graph put the depth in DEPTH_ATTACHMENT for reading,
    // untested); its full-screen triangle covers exactly the render area.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, f.resolve);
        (dev.fns.cmd_push_constants)(cmd, f.layout, PUSH_STAGES, 0, size_of::<IdPush>() as u32, &push as *const IdPush as *const c_void);
        (dev.fns.cmd_draw)(cmd, 3, 1, 0, 0);
        (dev.fns.cmd_end_rendering)(cmd);
    }
}

/// Where a [`Grown`] buffer lives.
#[derive(Clone, Copy)]
enum Memory {
    /// Host-written, shader-read (per frame), device-local when possible.
    Upload,
    /// GPU only.
    Device,
    /// GPU-written, host-read (cached when possible).
    Readback,
    /// GPU only, sized exactly (no doubling), for the life of the `Gpu` like the shared id images
    /// it shadows (the per-pixel sets: sized by the bucket, which only grows in big steps).
    Shared,
}

/// A buffer (with a device address when its usage asks), grown on demand (at least doubling,
/// except [`Memory::Shared`]).
struct Grown {
    buffer: Buffer,
    address: u64,
}

impl Grown {
    /// `slot`'s buffer, at least `bytes` big. Offscreen renders are synchronous: the old buffer is
    /// never in use when replaced.
    fn ensure<'a>(slot: &'a mut Option<Grown>, dev: &Device, bytes: u64, usage: VkFlags, memory: Memory, what: &str) -> Result<&'a Grown, GpuError> {
        if slot.as_ref().is_none_or(|g| g.buffer.bytes < bytes) {
            let want = match memory {
                Memory::Shared => bytes,
                _ => bytes.max(slot.as_ref().map_or(1024, |g| g.buffer.bytes * 2)),
            };
            let host = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
            let buffer = match memory {
                // Written by the host, read by shaders: device-local when the device allows it.
                Memory::Upload => dev
                    .create_buffer(want, usage, host, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, MemoryKind::PerFrame, what)
                    .or_else(|_| dev.create_buffer(want, usage, host, 0, MemoryKind::PerFrame, what))?,
                Memory::Device => dev.create_buffer(want, usage, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0, MemoryKind::PerFrame, what)?,
                Memory::Readback => dev.create_buffer(want, usage, host, VK_MEMORY_PROPERTY_HOST_CACHED_BIT, MemoryKind::Readback, what)?,
                Memory::Shared => dev.create_buffer(want, usage, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0, MemoryKind::RenderTargets, what)?,
            };
            let address = if usage & VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT != 0 { dev.buffer_address(&buffer) } else { 0 };
            if let Some(old) = slot.replace(Grown { buffer, address }) {
                dev.destroy_buffer(&old.buffer);
            }
        }
        Ok(slot.as_ref().expect("just made"))
    }
}

/// Every ID buffer resource of a `Gpu`: per-record ids, the shared readback, the stats buffers,
/// the shared images' bucket, and which target's ids the readback holds.
#[derive(Default)]
pub struct IdResources {
    upload: Option<Grown>,
    /// The 2D ID shaders' [`IdParams2d`].
    params_2d: Option<Grown>,
    scratch: Vec<u32>,
    readback: Option<Grown>,
    stats: Option<Grown>,
    stats_host: Option<Grown>,
    seen: Option<Grown>,
    /// The epoch the last ID render with blended draws tagged `seen` with.
    epoch: u32,
    /// The sets must be cleared before use (new memory or a wrapped epoch), until a render that
    /// cleared them completes.
    seen_dirty: bool,
    /// Size of the shared id images (grows; see [`bucket_for`]).
    pub bucket: (u32, u32),
    /// Count blended draws even when the device could (tests and comparisons turn it off).
    pub blended_disabled: bool,
    /// The ID render being recorded, and the last one finished (whose results the buffers hold).
    pending: Option<Last>,
    last: Option<Last>,
    /// The ID render being recorded's occluders, clamped to its extent (reused).
    occluders: Vec<VkClearRect>,
}

/// What an ID render's results describe.
#[derive(Clone, Debug)]
struct Last {
    target: u32,
    extent: VkExtent2D,
    max_id: u32,
    /// Blended draws were counted (atomics on).
    blended_counted: bool,
    /// Per id (index `id - 1`): it has blended draws (uncounted without atomics).
    has_blended: Vec<bool>,
    /// This render clears the id sets.
    cleared: bool,
}

impl IdResources {
    /// Uploads `draw_ids` in record order, makes sure the readback and stats buffers are big
    /// enough, and returns what recording needs. The graph's readback buffer is bound here.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        dev: &Device,
        pipelines: &mut Pipelines,
        plans: (&Plan, &crate::sprite::Plan2d),
        frame: &crate::FrameDesc,
        draw_ids: &[u32],
        res: IdRes,
        target: u32,
        extent: VkExtent2D,
        graph: &mut FrameGraph,
    ) -> Result<IdFrame, GpuError> {
        let (plan, plan_2d) = plans;
        let draws = frame.draws;
        record_ids(plan, draw_ids, &mut self.scratch);
        // The 2D records' ids follow the meshes' (the 2D plan's order; `draw_ids` gives the 2D
        // draws theirs after the mesh draws').
        let ids_2d_at = self.scratch.len();
        let ids_2d = &draw_ids[draws.len().min(draw_ids.len())..];
        self.scratch.extend(plan_2d.order.iter().map(|&i| ids_2d.get(i as usize).copied().unwrap_or(0)));
        let addr = VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT;
        let upload = Grown::ensure(&mut self.upload, dev, (self.scratch.len().max(1) * 4) as u64, addr, Memory::Upload, "id upload")?;
        // SAFETY: `mapped` is coherent host memory of at least `scratch.len()` u32s that the GPU
        // no longer reads (offscreen renders wait for completion).
        unsafe { ptr::copy_nonoverlapping(self.scratch.as_ptr(), upload.buffer.mapped as *mut u32, self.scratch.len()) };
        let ids_address = upload.address;
        let ids_2d_address = upload.address + ids_2d_at as u64 * 4;
        let pixels = extent.width as u64 * extent.height as u64;
        let readback = Grown::ensure(&mut self.readback, dev, pixels * BYTES_PER_PIXEL, VK_BUFFER_USAGE_TRANSFER_DST_BIT, Memory::Readback, "id readback")?;
        let readback = readback.buffer.handle;
        graph.bind_buffer(res.readback, readback);
        let max_id = draw_ids.iter().copied().max().unwrap_or(0);
        let mut has_blended = vec![false; max_id as usize];
        for (d, &id) in draws.iter().zip(draw_ids) {
            if id != 0 && d.material.blended() {
                has_blended[(id - 1) as usize] = true;
            }
        }
        for (d, &id) in frame.draws_2d.iter().zip(ids_2d) {
            let blended = match d {
                crate::Draw2d::Sprite(s) => s.blend == crate::SpriteBlend::Blended,
                crate::Draw2d::Tiles(t) => t.blend == crate::SpriteBlend::Blended,
                crate::Draw2d::Shape(_) => true,
            };
            if id != 0 && blended {
                has_blended[(id - 1) as usize] = true;
            }
        }
        let count_blended = dev.fragment_atomics && !self.blended_disabled;
        let (mut stats, mut stats_address, mut stats_host, mut stats_bytes) = (VK_NULL_HANDLE, 0, VK_NULL_HANDLE, 0);
        let (mut seen, mut seen_address, mut seen_clear) = (VK_NULL_HANDLE, 0, false);
        let (mut insert, mut raster, mut resolve) = ([VK_NULL_HANDLE; 2], [VK_NULL_HANDLE; 4], VK_NULL_HANDLE);
        // The variants built (the plain ones always are; skinned and textured ones by the
        // background build, never here: a capture skips draws whose variant isn't ready, exactly
        // as the scene does).
        let ready = pipelines.variants;
        let variant = |pass: IdPass, skinned: bool, textured: bool| {
            let have = (!skinned || ready.skinned) && (!textured || ready.textured);
            have.then(|| pipelines.built(PipelineKey::Ids { pass, skinned, textured })).flatten().unwrap_or(VK_NULL_HANDLE)
        };
        let streams = |pass: IdPass| [(false, false), (true, false), (false, true), (true, true)].map(|(s, t)| variant(pass, s, t));
        if count_blended {
            stats_bytes = (max_id.max(1) as usize * STATS_STRIDE * 4) as u64;
            let usage = addr | VK_BUFFER_USAGE_TRANSFER_SRC_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT | VK_BUFFER_USAGE_STORAGE_BUFFER_BIT;
            let s = Grown::ensure(&mut self.stats, dev, stats_bytes, usage, Memory::Device, "id stats")?;
            (stats, stats_address) = (s.buffer.handle, s.address);
            if has_blended.iter().any(|b| *b) {
                // Only frames with blended draws use the sets. Sized for the shared images'
                // bucket, so captures of other sizes reuse them (clearing them is slow on an
                // iGPU, hence the epochs).
                let bucket_bytes = (self.bucket.0 as u64 * self.bucket.1 as u64).max(pixels) * SEEN_SLOTS * 4;
                let usage = addr | VK_BUFFER_USAGE_TRANSFER_DST_BIT | VK_BUFFER_USAGE_STORAGE_BUFFER_BIT;
                let before = self.seen.as_ref().map(|g| g.buffer.handle);
                let s = Grown::ensure(&mut self.seen, dev, bucket_bytes, usage, Memory::Shared, "id sets")?;
                (seen, seen_address) = (s.buffer.handle, s.address);
                if before != Some(seen) {
                    self.seen_dirty = true; // new memory: undefined contents
                }
                seen_clear = self.next_epoch();
                insert = [variant(IdPass::Insert, false, false), variant(IdPass::Insert, true, false)];
                resolve = variant(IdPass::Resolve, false, false);
            }
            stats_host = Grown::ensure(&mut self.stats_host, dev, stats_bytes, VK_BUFFER_USAGE_TRANSFER_DST_BIT, Memory::Readback, "id stats readback")?.buffer.handle;
            raster = streams(IdPass::Raster);
        }
        // 2D items: cutouts always (opaque ids); blended items and shapes through the sets.
        let built = |pass: IdPass, shape: bool| pipelines.built(PipelineKey::Ids2d { pass, shape }).unwrap_or(VK_NULL_HANDLE);
        let mut sprite = [built(IdPass::Opaque, false), VK_NULL_HANDLE, VK_NULL_HANDLE];
        let mut shape = [VK_NULL_HANDLE; 2];
        if count_blended {
            if insert[0] != VK_NULL_HANDLE {
                (sprite[1], shape[0]) = (built(IdPass::Insert, false), built(IdPass::Insert, true));
            }
            (sprite[2], shape[1]) = (built(IdPass::Raster, false), built(IdPass::Raster, true));
        }
        // Where the 2D ID shaders find the counts (a pointer in their push constants' padding).
        let mut params_address = 0;
        if !plan_2d.batches.is_empty() {
            let p = Grown::ensure(&mut self.params_2d, dev, size_of::<IdParams2d>() as u64, addr, Memory::Upload, "id 2d params")?;
            let params = IdParams2d { ids: ids_2d_address, stats: stats_address, seen: seen_address, width: extent.width, epoch: self.epoch };
            // SAFETY: `mapped` is coherent host memory of at least one IdParams2d that the GPU no
            // longer reads (offscreen renders wait for completion); the mapping is 8-byte aligned.
            unsafe { ptr::write(p.buffer.mapped as *mut IdParams2d, params) };
            params_address = p.address;
        }
        self.pending = Some(Last { target, extent, max_id, blended_counted: count_blended, has_blended, cleared: seen_clear });
        Ok(IdFrame {
            opaque: streams(IdPass::Opaque),
            insert,
            raster,
            resolve,
            sprite,
            shape,
            params_2d: params_address,
            layout: pipelines.layout,
            ids_address,
            stats,
            stats_address,
            stats_bytes,
            stats_host,
            seen,
            seen_address,
            seen_clear,
            epoch: self.epoch,
            res,
            readback,
            extent,
        })
    }

    /// The occluders of the ID render about to be recorded: `[x, y, width, height]` pixel
    /// rectangles, clamped to `extent` (whose rectangles must lie inside the render area);
    /// empty ones are dropped.
    pub fn set_occluders(&mut self, rects: &[[u32; 4]], extent: VkExtent2D) {
        self.occluders.clear();
        self.occluders.extend(rects.iter().filter_map(|&r| clamp_rect(r, extent)));
    }

    /// What [`IdResources::set_occluders`] kept.
    pub fn occluders(&self) -> &[VkClearRect] {
        &self.occluders
    }

    /// The next capture's epoch. Slots of older epochs count as free, so they are cleared (returns
    /// true) only while `seen_dirty` (new memory, or the epoch wrapped) — and stay "to clear" until
    /// a render that cleared them completes ([`IdResources::finished`]).
    fn next_epoch(&mut self) -> bool {
        self.epoch += 1;
        if self.epoch > MAX_EPOCH {
            self.seen_dirty = true;
        }
        if self.seen_dirty {
            self.epoch = 1;
        }
        self.seen_dirty
    }

    /// A render of `target` is starting (`ids`: with the ID buffer): results it would overwrite or
    /// make stale are forgotten.
    pub fn starting(&mut self, target: u32, ids: bool) {
        self.pending = None;
        if ids || self.last.as_ref().is_some_and(|l| l.target == target) {
            self.last = None;
        }
    }

    /// The ID render recorded by [`IdResources::frame`] completed: its results are readable.
    pub fn finished(&mut self) {
        if let Some(p) = self.pending.take() {
            if p.cleared {
                self.seen_dirty = false; // the clear really ran
            }
            self.last = Some(p);
        }
    }

    /// `target` is going away.
    pub fn forget(&mut self, target: u32) {
        if self.last.as_ref().is_some_and(|l| l.target == target) {
            self.last = None;
        }
    }

    fn check(&self, target: u32, what: &str) -> Result<&Last, GpuError> {
        match &self.last {
            Some(l) if l.target == target => Ok(l),
            _ => Err(GpuError::new(format!("{what}: target {target}'s last render had no ID buffer (use render_with_ids)"))),
        }
    }

    fn opaque_ids(&self, extent: VkExtent2D) -> &[u32] {
        let n = extent.width as usize * extent.height as usize;
        let buffer = &self.readback.as_ref().expect("an ID render made it").buffer;
        // SAFETY: the readback is persistently mapped coherent memory of at least n u32s (sized for
        // the last ID render, which `check` confirmed); that render finished (fence waited) and
        // the graph recorded the transfer → host barrier; the mapping is 4-byte aligned
        // (`minMemoryMapAlignment` ≥ 64), and nothing writes it while `self` is borrowed.
        unsafe { std::slice::from_raw_parts(buffer.mapped as *const u32, n) }
    }

    /// The opaque id image of `target`'s last ID render.
    pub fn image(&self, target: u32) -> Result<IdImage, GpuError> {
        let extent = self.check(target, "read_ids")?.extent;
        Ok(IdImage { width: extent.width, height: extent.height, ids: self.opaque_ids(extent).to_vec() })
    }

    /// Per id `1..=max_id` of `target`'s last ID render: opaque pixels counted here (in place),
    /// blended pixels and rasterization from the stats buffer. Without blended counting, ids with
    /// blended draws are marked `counted: false` (their pixels are unknown).
    pub fn counts(&self, target: u32, max_id: u32) -> Result<IdCounts, GpuError> {
        let last = self.check(target, "id_coverage")?;
        let mut per_id = coverage(self.opaque_ids(last.extent), last.extent.width, max_id);
        let rendered_max = last.max_id;
        if let (Some(host), true) = (&self.stats_host, last.blended_counted && rendered_max > 0) {
            let n = rendered_max.min(max_id) as usize;
            let len = rendered_max as usize * STATS_STRIDE;
            // SAFETY: coherent mapped memory holding at least rendered_max × STATS_STRIDE u32s,
            // written by the finished render's copy (copy → host barrier recorded).
            let stats = unsafe { std::slice::from_raw_parts(host.buffer.mapped as *const u32, len) };
            for (c, s) in per_id.iter_mut().zip(stats.chunks_exact(STATS_STRIDE)).take(n) {
                c.add_blended(s);
            }
        }
        if !last.blended_counted {
            for (c, &blended) in per_id.iter_mut().zip(last.has_blended.iter().chain(std::iter::repeat(&false))) {
                c.rasterized = None;
                c.counted = !blended;
            }
        }
        Ok(IdCounts { per_id, blended_counted: last.blended_counted })
    }

    pub fn destroy(&mut self, dev: &Device) {
        for g in [self.upload.take(), self.params_2d.take(), self.readback.take(), self.stats.take(), self.stats_host.take(), self.seen.take()]
            .into_iter()
            .flatten()
        {
            dev.destroy_buffer(&g.buffer);
        }
    }
}

/// `[x, y, width, height]` clamped to `extent`, as a one-layer clear rectangle (`None` when empty).
fn clamp_rect(r: [u32; 4], extent: VkExtent2D) -> Option<VkClearRect> {
    let (x0, y0) = (r[0].min(extent.width), r[1].min(extent.height));
    let (x1, y1) = (r[0].saturating_add(r[2]).min(extent.width), r[1].saturating_add(r[3]).min(extent.height));
    (x1 > x0 && y1 > y0).then(|| VkClearRect {
        rect: VkRect2D { offset: VkOffset2D { x: x0 as i32, y: y0 as i32 }, extent: VkExtent2D { width: x1 - x0, height: y1 - y0 } },
        base_array_layer: 0,
        layer_count: 1,
    })
}

/// What one id covers in a capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdCoverage {
    /// `opaque_pixels + blended_pixels`.
    pub pixels: u64,
    /// Pixels where it is the nearest opaque surface.
    pub opaque_pixels: u64,
    /// Front-facing blended pixels in front of the nearest opaque surface.
    pub blended_pixels: u64,
    /// Inclusive pixel bounds of those pixels (meaningless when `pixels` is 0).
    pub min: [u32; 2],
    pub max: [u32; 2],
    /// Any fragment of it the scene shows was rasterized (depth ignored); `None` when unknown
    /// (blended draws not counted).
    pub rasterized: Option<bool>,
    /// All of its draws were counted. False for ids with blended draws when blended draws aren't
    /// counted (no fragment atomics): then its pixels are unknown, not zero.
    pub counted: bool,
    /// Its blended pixels and box are **lower bounds**: at some pixel more than [`SEEN_SLOTS`]
    /// blended ids overlapped and it wasn't among the smallest, so that pixel isn't counted for
    /// it (nor are any of an id at or above 2^[`SEEN_ID_BITS`]). Reproducible all the same.
    pub approx: bool,
}

impl Default for IdCoverage {
    fn default() -> Self {
        IdCoverage {
            pixels: 0,
            opaque_pixels: 0,
            blended_pixels: 0,
            min: [u32::MAX; 2],
            max: [0; 2],
            rasterized: Some(false),
            counted: true,
            approx: false,
        }
    }
}

impl IdCoverage {
    fn add_run(&mut self, x0: u32, x1: u32, y: u32) {
        let n = u64::from(x1 - x0 + 1);
        self.pixels += n;
        self.opaque_pixels += n;
        self.min = [self.min[0].min(x0), self.min[1].min(y)];
        self.max = [self.max[0].max(x1), self.max[1].max(y)];
        self.rasterized = Some(true);
    }

    fn merge(&mut self, o: &IdCoverage) {
        self.pixels += o.pixels;
        self.opaque_pixels += o.opaque_pixels;
        self.blended_pixels += o.blended_pixels;
        self.min = [self.min[0].min(o.min[0]), self.min[1].min(o.min[1])];
        self.max = [self.max[0].max(o.max[0]), self.max[1].max(o.max[1])];
        self.rasterized = Some(self.rasterized == Some(true) || o.rasterized == Some(true));
    }

    /// Adds one id's stats record (see `shaders/ids.glsl`).
    fn add_blended(&mut self, s: &[u32]) {
        let px = u64::from(s[0]);
        self.approx |= s[6] != 0;
        if px > 0 {
            let (min, max) = ([!s[1], !s[2]], [s[3], s[4]]);
            self.pixels += px;
            self.blended_pixels += px;
            self.min = [self.min[0].min(min[0]), self.min[1].min(min[1])];
            self.max = [self.max[0].max(max[0]), self.max[1].max(max[1])];
        }
        if s[5] != 0 || px > 0 || s[6] != 0 {
            self.rasterized = Some(true);
        }
    }
}

/// [`crate::Gpu::id_coverage`]'s answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdCounts {
    /// Index `id - 1`.
    pub per_id: Vec<IdCoverage>,
    /// Blended draws were counted (fragment atomics available).
    pub blended_counted: bool,
}

/// The opaque ids of an ID render, rows top to bottom (0 = none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdImage {
    pub width: u32,
    pub height: u32,
    pub ids: Vec<u32>,
}

impl IdImage {
    /// Opaque coverage of ids `1..=max_id` (index `id - 1`); ids above are ignored.
    pub fn coverage(&self, max_id: u32) -> Vec<IdCoverage> {
        coverage(&self.ids, self.width, max_id)
    }
}

/// Images at least this big are counted on several threads.
const PARALLEL_PIXELS: usize = 1 << 18;

/// Opaque coverage of `ids` (rows `width` wide) for ids `1..=max_id`: runs of one id along a row
/// are added at once (and runs of 0 skipped), in parallel row bands for big images.
pub fn coverage(ids: &[u32], width: u32, max_id: u32) -> Vec<IdCoverage> {
    let w = width as usize;
    if w == 0 || ids.is_empty() {
        return vec![IdCoverage::default(); max_id as usize];
    }
    let rows = ids.len() / w;
    let threads = if ids.len() >= PARALLEL_PIXELS {
        std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8).min(rows)
    } else {
        1
    };
    if threads <= 1 {
        return coverage_rows(ids, w, 0, max_id);
    }
    let band = rows.div_ceil(threads);
    let parts: Vec<Vec<IdCoverage>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let (r0, r1) = (t * band, ((t + 1) * band).min(rows));
                let slice = &ids[r0 * w..r1 * w];
                s.spawn(move || coverage_rows(slice, w, r0 as u32, max_id))
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("counting thread")).collect()
    });
    let mut out = vec![IdCoverage::default(); max_id as usize];
    for part in parts {
        for (o, p) in out.iter_mut().zip(&part) {
            if p.pixels > 0 {
                o.merge(p);
            }
        }
    }
    out
}

fn coverage_rows(ids: &[u32], w: usize, first_row: u32, max_id: u32) -> Vec<IdCoverage> {
    let mut out = vec![IdCoverage::default(); max_id as usize];
    for (y, row) in ids.chunks_exact(w).enumerate() {
        let y = first_row + y as u32;
        let mut x = 0;
        while x < w {
            let id = row[x];
            let start = x;
            x += 1;
            while x < w && row[x] == id {
                x += 1;
            }
            if id != 0 && id <= max_id {
                out[(id - 1) as usize].add_run(start as u32, (x - 1) as u32, y);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawItem, FrameDesc, Gpu, GpuOptions, Material, Validation, Vertex};

    #[test]
    fn record_ids_follow_the_plan() {
        let mut plan = Plan::default();
        plan.entries = vec![(2, 0), (0, 0), (1, 0), (1, 1)];
        let mut out = Vec::new();
        record_ids(&plan, &[10, 11, 12], &mut out);
        assert_eq!(out, [12, 10, 11, 11], "glass draws twice (back then front faces), with its id both times");
        record_ids(&plan, &[7], &mut out);
        assert_eq!(out, [0, 7, 0, 0], "missing ids count as none");
    }

    #[test]
    fn buckets_only_grow_in_steps() {
        let e = |w, h| VkExtent2D { width: w, height: h };
        assert_eq!(bucket_for(e(960, 540), (0, 0)), (1024, 768));
        assert_eq!(bucket_for(e(320, 240), (1024, 768)), (1024, 768), "a smaller capture reuses the images");
        assert_eq!(bucket_for(e(2048, 100), (1024, 768)), (2048, 768));
        assert_eq!(bucket_for(e(1, 1), (0, 0)), (256, 256));
    }

    fn naive(ids: &[u32], w: u32, max_id: u32) -> Vec<IdCoverage> {
        let mut out = vec![IdCoverage::default(); max_id as usize];
        for (i, &id) in ids.iter().enumerate() {
            if id != 0 && id <= max_id {
                out[(id - 1) as usize].add_run(i as u32 % w, i as u32 % w, i as u32 / w);
            }
        }
        out
    }

    #[test]
    fn run_length_and_parallel_counts_match_pixel_by_pixel() {
        for (w, h) in [(37u32, 23u32), (1024, 300)] {
            let mut seed = 12345u64;
            let mut next = |n: u32| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((seed >> 33) % u64::from(n)) as u32
            };
            let mut ids = Vec::new();
            let mut o = 0;
            for _ in 0..w * h {
                if next(5) == 0 {
                    o = next(7);
                }
                ids.push(o);
            }
            assert!(ids.len() < PARALLEL_PIXELS || w == 1024);
            assert_eq!(coverage(&ids, w, 5), naive(&ids, w, 5), "{w}x{h}");
        }
        let img = IdImage { width: 3, height: 1, ids: vec![1, 0, 1] };
        let c = img.coverage(1);
        assert_eq!((c[0].pixels, c[0].min, c[0].max), (2, [0, 0], [2, 0]));
    }

    #[test]
    fn blended_stats_add_to_coverage() {
        let mut c = IdCoverage::default();
        c.add_run(10, 19, 5);
        c.add_blended(&[30, !2, !4, 12, 9, 1, 0]);
        assert_eq!((c.pixels, c.opaque_pixels, c.blended_pixels, c.min, c.max), (40, 10, 30, [2, 4], [19, 9]));
        let mut hidden = IdCoverage::default();
        hidden.add_blended(&[0, 0, 0, 0, 0, 1, 0]);
        assert_eq!((hidden.pixels, hidden.rasterized), (0, Some(true)), "rasterized but nothing visible: occluded");
    }

    fn cube() -> (Vec<Vertex>, Vec<u32>) {
        let mut v = Vec::new();
        let mut idx = Vec::new();
        // Outward-facing, wound like the engine's own cube (counter-clockwise seen from outside).
        for axis in 0..3 {
            for sign in [-0.5f32, 0.5] {
                let base = v.len() as u32;
                let (u, w) = ((axis + 1) % 3, (axis + 2) % 3);
                for (a, b) in [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                    let mut p = [0.0; 3];
                    p[axis] = sign;
                    p[u] = a;
                    p[w] = b;
                    let mut n = [0.0; 3];
                    n[axis] = sign * 2.0;
                    v.push(Vertex::new(p, n));
                }
                if sign > 0.0 {
                    idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                } else {
                    idx.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
                }
            }
        }
        (v, idx)
    }

    /// A unit quad in the XY plane whose normal faces -Z (away from a camera on +Z).
    fn back_quad() -> Vec<Vertex> {
        [[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]].map(|p| Vertex::new(p, [0.0, 0.0, -1.0])).to_vec()
    }

    #[test]
    fn slot_epochs_clear_only_when_needed() {
        let last = |cleared| Last { target: 1, extent: VkExtent2D { width: 1, height: 1 }, max_id: 0, blended_counted: true, has_blended: Vec::new(), cleared };
        let mut r = IdResources { seen_dirty: true, ..Default::default() };
        assert!(r.next_epoch(), "new memory is cleared");
        // Reviewer finding: a render that fails after choosing to clear must not lose the clear.
        r.pending = None; // it failed: never finished
        assert!(r.next_epoch(), "still to clear after a failed render");
        r.pending = Some(last(true));
        r.finished();
        assert_eq!(r.epoch, 1);
        for e in 2..=MAX_EPOCH {
            assert!(!r.next_epoch());
            r.pending = Some(last(false));
            r.finished();
            assert_eq!(r.epoch, e);
        }
        assert!(r.next_epoch(), "wrapping clears");
        assert_eq!(r.epoch, 1);
        assert_eq!(MAX_EPOCH, 4095);
    }

    #[test]
    fn face_rules_mirror_the_scene() {
        use crate::Material::*;
        assert_eq!([faces_for(Glass), faces_for(BlobShadow), faces_for(LavaLamp), faces_for(Lit)], [FACES_ALL, FACES_ALL, FACES_LAVA, FACES_ALL]);
        // Pushed out of some pixel's set: a lower bound, reported as counted.
        let mut c = IdCoverage::default();
        c.add_blended(&[60, !10, !10, 19, 19, 0, 1]);
        assert_eq!((c.blended_pixels, c.pixels, c.approx, c.rasterized), (60, 60, true, Some(true)));
    }

    fn trs(t: [f32; 3], s: [f32; 3]) -> [f32; 16] {
        [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, t[0], t[1], t[2], 1.0]
    }

    fn glass(mesh: crate::MeshId, t: [f32; 3], s: [f32; 3]) -> DrawItem {
        DrawItem { material: Material::Glass, ..DrawItem::new(mesh, trs(t, s), [0.8, 0.9, 1.0, 0.2]) }
    }

    /// Occlusion, half-hidden, exact extents, glass over and behind glass, z-fighting, sub-pixel
    /// and instanced draws, checked against the projection and the image on every adapter.
    #[test]
    fn ids_count_visible_pixels_with_occlusion() {
        if !crate::is_available() {
            eprintln!("ids_count_visible_pixels_with_occlusion: SKIPPED — Vulkan is not available");
            return;
        }
        for a in crate::enumerate_adapters().unwrap() {
            let options = GpuOptions { validation: Validation::On, adapter: crate::AdapterPreference::Index(a.index), ..Default::default() };
            let mut gpu = match Gpu::new(&options) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("ids_count_visible_pixels_with_occlusion: SKIPPED on adapter {} — {e}", a.index);
                    continue;
                }
            };
            let (v, i) = cube();
            let mesh = gpu.upload_mesh(&v, &i).unwrap();
            let (w, h) = (320u32, 240u32);
            let target = gpu.create_target(w, h).unwrap();
            // Camera at (0, 0, 10) looking down -Z with the engine's reversed-Z infinite projection
            // (40 degrees vertical field of view): clip = (f/a x, -f y, near, 10 - z).
            let f = 1.0 / 20f32.to_radians().tan();
            let aspect = w as f32 / h as f32;
            let vp = [f / aspect, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 10.0];
            let project = |p: [f32; 3]| {
                let cw = 10.0 - p[2];
                (((f / aspect * p[0]) / cw + 1.0) * 0.5 * w as f32, ((-f * p[1]) / cw + 1.0) * 0.5 * h as f32)
            };
            let area = |a: [f32; 3], b: [f32; 3]| {
                let (p, q) = (project(a), project(b));
                (q.0 - p.0).abs() * (q.1 - p.1).abs()
            };
            let mut draws = vec![
                // 1: the wall, x -2..2, y -1.5..1.5, z -0.1..0.1.
                DrawItem::new(mesh, trs([0.0, 0.0, 0.0], [4.0, 3.0, 0.2]), [0.5, 0.5, 0.5, 1.0]),
                // 2: fully behind the wall.
                DrawItem::new(mesh, trs([0.0, 0.0, -3.0], [1.0, 1.0, 1.0]), [1.0, 0.0, 0.0, 1.0]),
                // 3: half behind the wall's right edge: the edge (x = 2 at 9.9 m) projects to
                // x = 2 * 12.5 / 9.9 at the cube's front face (12.5 m), so centre it there.
                DrawItem::new(mesh, trs([2.0 * 12.5 / 9.9, 0.0, -3.0], [1.0, 1.0, 1.0]), [0.0, 1.0, 0.0, 1.0]),
                // 4: a glass pane in front of the wall's left half (y -0.5..0.5).
                glass(mesh, [-1.0, 0.0, 2.0], [1.0, 1.0, 0.1]),
                // 5: a smaller glass pane behind it (y -0.25..0.25): visible through it.
                glass(mesh, [-1.0, 0.0, 1.0], [0.5, 0.5, 0.1]),
                // 6, 7: two red/blue cubes in exactly the same place (z-fighting), clear of the wall.
                DrawItem::new(mesh, trs([-3.0, 2.2, 0.5], [0.6, 0.6, 0.6]), [1.0, 0.0, 0.0, 1.0]),
                DrawItem::new(mesh, trs([-3.0, 2.2, 0.5], [0.6, 0.6, 0.6]), [0.0, 0.0, 1.0, 1.0]),
                // 8: far and tiny: rasterizes no pixel at all.
                DrawItem::new(mesh, trs([3.5, 2.5, -500.0], [0.001, 0.001, 0.001]), [1.0; 4]),
            ];
            // 9..=28: a row of small cubes of one mesh (one instanced call), below the wall.
            for k in 0..20 {
                draws.push(DrawItem::new(mesh, trs([-3.8 + k as f32 * 0.4, -2.2, 0.0], [0.25, 0.25, 0.25]), [1.0, 0.7, 0.0, 1.0]));
            }
            let ids: Vec<u32> = (1..=draws.len() as u32).collect();
            let frame = FrameDesc { view_proj: vp, draws: &draws, reverse_z: true, ..Default::default() };
            gpu.render(target, &frame).unwrap();
            let plain = gpu.read_rgba8(target).unwrap();
            assert!(gpu.read_ids(target).is_err(), "a plain render has no ID buffer");
            let wrong = gpu.render_with_ids(target, &frame, &[1, 2]).unwrap_err();
            assert!(matches!(wrong.reason, Some(crate::ErrorReason::DataLength { given: 2, expected: 28 })), "{wrong}");
            gpu.render_with_ids(target, &frame, &ids).unwrap();
            assert!(gpu.read_rgba8(target).unwrap() == plain, "adapter {}: the ID passes change the image", a.index);
            let img = gpu.read_ids(target).unwrap();
            assert_eq!((img.width, img.height, img.ids.len()), (w, h, (w * h) as usize));
            let counts = gpu.id_coverage(target, 28).unwrap();
            let c = &counts.per_id;
            let opaque = img.coverage(28);
            for (k, (all, op)) in c.iter().zip(&opaque).enumerate() {
                assert_eq!(all.opaque_pixels, op.opaque_pixels, "adapter {}: id {}", a.index, k + 1);
            }
            let atomics = counts.blended_counted;
            // The hidden cube: no pixel, but rasterized (so: occluded).
            assert_eq!(c[1].pixels, 0, "adapter {}: the cube behind the wall shows {:?}", a.index, c[1]);
            if atomics {
                assert_eq!(c[1].rasterized, Some(true));
            }
            // Half hidden: its front face's projected square is 1 / (12.5 tan 20°) of the half-height.
            let side = 1.0 / (12.5 * 20f32.to_radians().tan()) * 0.5 * h as f32;
            let ratio = c[2].pixels as f32 / (side * side);
            assert!((0.4..0.6).contains(&ratio), "adapter {}: half hidden shows {ratio} of its area", a.index);
            // Exact extents: the wall's front face (z = 0.1), within a pixel; glass hides none of it.
            let wall = c[0];
            let (x0, y0) = project([-2.0, 1.5, 0.1]);
            let (x1, y1) = project([2.0, -1.5, 0.1]);
            for (got, want) in [(wall.min[0], x0), (wall.min[1], y0), (wall.max[0] + 1, x1), (wall.max[1] + 1, y1)] {
                assert!((got as f32 - want).abs() <= 1.0, "adapter {}: wall extent {got} vs projected {want}", a.index);
            }
            let wall_area = area([-2.0, 1.5, 0.1], [2.0, -1.5, 0.1]);
            assert!((wall.pixels as f32 - wall_area).abs() / wall_area < 0.02, "adapter {}: wall {} px of {wall_area}", a.index, wall.pixels);
            if atomics {
                // Each pane counts its front face once: the front pane in full, and the back pane
                // in full too although it is behind the front one.
                for (k, front_area) in [(3, area([-1.5, 0.5, 2.05], [-0.5, -0.5, 2.05])), (4, area([-1.25, 0.25, 1.05], [-0.75, -0.25, 1.05]))] {
                    let r = c[k].blended_pixels as f32 / front_area;
                    assert!((0.95..1.05).contains(&r), "adapter {}: glass {} counts {} px of {front_area}", a.index, k + 1, c[k].blended_pixels);
                }
                assert!(c[7].rasterized == Some(false) && c[7].pixels == 0, "adapter {}: the tiny cube: {:?}", a.index, c[7]);
            } else {
                assert_eq!(c[3].pixels + c[4].pixels, 0, "without atomics blended draws go uncounted");
            }
            // Z-fighting: every pixel goes to exactly one of the two, the one the image shows.
            let (red, blue) = (c[5], c[6]);
            let fight = area([-3.3, 2.5, 0.8], [-2.7, 1.9, 0.8]);
            // (At least its front face; its bottom and side show too from this camera.)
            assert!((red.pixels + blue.pixels) as f32 >= 0.95 * fight, "adapter {}: {red:?} {blue:?} vs {fight}", a.index);
            for (pi, &id) in img.ids.iter().enumerate() {
                if id == 6 || id == 7 {
                    let px = &plain[pi * 4..pi * 4 + 3];
                    let shows_red = px[0] > px[2];
                    assert_eq!(shows_red, id == 6, "adapter {}: pixel {pi} id {id} colour {px:?}", a.index);
                }
            }
            // Instanced: every cube of the row has its own pixels, inside its own projected box.
            for k in 0..20 {
                let cv = c[8 + k];
                let x = -3.8 + k as f32 * 0.4;
                // Its box: all eight corners projected.
                let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for corner in 0..8 {
                    let s = |bit: usize| if corner & bit == 0 { -0.125 } else { 0.125 };
                    let (px, py) = project([x + s(1), -2.2 + s(2), s(4)]);
                    (bx0, by0, bx1, by1) = (bx0.min(px), by0.min(py), bx1.max(px), by1.max(py));
                }
                assert!(cv.pixels > 20, "adapter {}: cube {k}: {cv:?}", a.index);
                assert!((cv.min[0] as f32) >= bx0 - 1.0 && (cv.max[0] as f32) <= bx1 + 1.0, "adapter {}: cube {k} x {cv:?} vs {bx0}..{bx1}", a.index);
                assert!((cv.min[1] as f32) >= by0 - 1.0 && (cv.max[1] as f32) <= by1 + 1.0, "adapter {}: cube {k} y {cv:?} vs {by0}..{by1}", a.index);
            }
            // Without blended counting (devices lacking fragment atomics): blended ids are
            // uncounted (not "0 px"), opaque ones unchanged.
            if atomics {
                gpu.set_ids_count_blended(false);
                gpu.render_with_ids(target, &frame, &ids).unwrap();
                let off = gpu.id_coverage(target, 28).unwrap();
                assert!(!off.blended_counted);
                for (k, (o, on)) in off.per_id.iter().zip(c).enumerate() {
                    let blended = draws[k].material.blended();
                    assert_eq!(o.counted, !blended, "adapter {}: id {}", a.index, k + 1);
                    assert_eq!((o.opaque_pixels, o.blended_pixels, o.rasterized), (on.opaque_pixels, 0, None), "adapter {}: id {}", a.index, k + 1);
                }
                gpu.set_ids_count_blended(true);
                gpu.render_with_ids(target, &frame, &ids).unwrap();
                assert_eq!(&gpu.id_coverage(target, 28).unwrap().per_id, c, "back on: the same counts");
            }

            // Blended draws seen the way the scene shows them (reviewer findings): a blob shadow
            // and an open glass quad whose normals face away (seen "from below" / "from behind":
            // the scene draws them anyway), and the camera inside a lava lamp's proxy box (the
            // scene shades its far faces).
            if atomics {
                let back_quad = gpu.upload_mesh(&back_quad(), &[0, 1, 2, 0, 2, 3]).unwrap();
                let quads = [
                    DrawItem { material: Material::BlobShadow, ..DrawItem::new(back_quad, trs([-1.5, 0.0, 0.0], [2.0, 2.0, 1.0]), [0.0, 0.0, 0.0, 0.6]) },
                    glass(back_quad, [1.5, 0.0, 0.0], [2.0, 2.0, 1.0]),
                    // An opaque wall behind them.
                    DrawItem::new(mesh, trs([0.0, 0.0, -2.0], [8.0, 6.0, 0.2]), [0.4, 0.4, 0.4, 1.0]),
                ];
                let frame2 = FrameDesc { view_proj: vp, draws: &quads, reverse_z: true, ..Default::default() };
                gpu.render_with_ids(target, &frame2, &[1, 2, 3]).unwrap();
                let c2 = gpu.id_coverage(target, 3).unwrap().per_id;
                let quad_area = area([-1.0, 1.0, 0.0], [1.0, -1.0, 0.0]);
                for (k, what) in [(0, "blob shadow from below"), (1, "open glass from behind")] {
                    let r = c2[k].blended_pixels as f32 / quad_area;
                    assert!((0.95..1.05).contains(&r), "adapter {}: {what}: {} px of {quad_area}", a.index, c2[k].blended_pixels);
                    assert_eq!(c2[k].rasterized, Some(true));
                }
                // Inside a lava lamp: its far faces fill the view.
                let lamp = [
                    DrawItem { material: Material::LavaLamp, ..DrawItem::new(mesh, trs([0.0, 0.0, 10.0], [3.0, 3.0, 3.0]), [1.0, 0.3, 0.0, 1.0]) },
                    DrawItem::new(mesh, trs([0.0, 0.0, 0.0], [4.0, 3.0, 0.2]), [0.5, 0.5, 0.5, 1.0]),
                ];
                let frame3 = FrameDesc { view_proj: vp, draws: &lamp, reverse_z: true, ..Default::default() };
                gpu.render_with_ids(target, &frame3, &[1, 2]).unwrap();
                let c3 = gpu.id_coverage(target, 2).unwrap().per_id;
                assert_eq!(c3[0].blended_pixels, u64::from(w * h), "adapter {}: inside the lamp: {:?}", a.index, c3[0]);
                assert_eq!(c3[0].rasterized, Some(true));
                assert!(c3[1].opaque_pixels > 0, "the lamp hides nothing");
                // Inside a glass box (a greenhouse): only its back faces face the camera, and the
                // scene shows them over the whole view.
                let case = [glass(mesh, [0.0, 0.0, 10.0], [3.0, 3.0, 3.0]), DrawItem::new(mesh, trs([0.0, 0.0, 0.0], [4.0, 3.0, 0.2]), [0.5; 4])];
                let frame4 = FrameDesc { view_proj: vp, draws: &case, reverse_z: true, ..Default::default() };
                gpu.render_with_ids(target, &frame4, &[1, 2]).unwrap();
                let c4 = gpu.id_coverage(target, 2).unwrap().per_id;
                assert_eq!(c4[0].blended_pixels, u64::from(w * h), "adapter {}: inside a glass box: {:?}", a.index, c4[0]);
                gpu.destroy_mesh(back_quad);
            }

            // Another target's render doesn't hand out this one's ids.
            let other = gpu.create_target(64, 64).unwrap();
            gpu.render_with_ids(other, &frame, &ids).unwrap();
            assert!(gpu.id_coverage(target, 28).is_err(), "the shared readback now holds the other target's ids");
            gpu.destroy_target(other);
            gpu.destroy_target(target);
            gpu.destroy_mesh(mesh);
            let errors: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= crate::Severity::Warning).collect();
            assert!(errors.is_empty(), "adapter {}: {errors:#?}", a.index);
        }
    }

    #[test]
    fn occluder_rects_clamp_to_the_capture() {
        let e = VkExtent2D { width: 100, height: 50 };
        let r = |x, y, w, h| VkRect2D { offset: VkOffset2D { x, y }, extent: VkExtent2D { width: w, height: h } };
        let mut res = IdResources::default();
        res.set_occluders(&[[10, 10, 20, 5], [90, 40, 30, 30], [100, 0, 5, 5], [5, 5, 0, 9], [u32::MAX, 1, u32::MAX, 1]], e);
        let kept: Vec<_> = res.occluders().iter().map(|c| (c.rect, c.base_array_layer, c.layer_count)).collect();
        assert_eq!(kept, [(r(10, 10, 20, 5), 0, 1), (r(90, 40, 10, 10), 0, 1)], "clamped; empty and off-image ones dropped");
    }

    /// Occluders (a HUD's opaque panels) hide what is under them from the ID counts, opaque and
    /// blended alike, keep the "rasterized" flag (so: occluded), leave the image alone, and change
    /// nothing when there are none.
    #[test]
    fn occluders_hide_what_is_under_them() {
        if !crate::is_available() {
            eprintln!("occluders_hide_what_is_under_them: SKIPPED — Vulkan is not available");
            return;
        }
        for a in crate::enumerate_adapters().unwrap() {
            let options = GpuOptions { validation: Validation::On, adapter: crate::AdapterPreference::Index(a.index), ..Default::default() };
            let mut gpu = match Gpu::new(&options) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("occluders_hide_what_is_under_them: SKIPPED on adapter {} — {e}", a.index);
                    continue;
                }
            };
            let (v, i) = cube();
            let mesh = gpu.upload_mesh(&v, &i).unwrap();
            let (w, h) = (320u32, 240u32);
            let target = gpu.create_target(w, h).unwrap();
            let f = 1.0 / 20f32.to_radians().tan();
            let vp = [f / (w as f32 / h as f32), 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.05, 10.0];
            let draws = [
                // 1: a wall across the middle.
                DrawItem::new(mesh, trs([0.0, 0.0, 0.0], [4.0, 3.0, 0.2]), [0.5, 0.5, 0.5, 1.0]),
                // 2: a glass pane in front of its left part.
                glass(mesh, [-1.0, 0.0, 2.0], [1.0, 1.0, 0.1]),
                // 3: a cube in front of its right part.
                DrawItem::new(mesh, trs([1.2, 0.0, 1.0], [0.8, 0.8, 0.8]), [0.0, 1.0, 0.0, 1.0]),
            ];
            let frame = FrameDesc { view_proj: vp, draws: &draws, reverse_z: true, ..Default::default() };
            gpu.render_with_ids(target, &frame, &[1, 2, 3]).unwrap();
            let plain = gpu.read_rgba8(target).unwrap();
            let before_img = gpu.read_ids(target).unwrap();
            let before = gpu.id_coverage(target, 3).unwrap();
            gpu.render_with_ids_occluded(target, &frame, &[1, 2, 3], &[]).unwrap();
            assert_eq!(gpu.id_coverage(target, 3).unwrap(), before, "adapter {}: no occluders, the same counts", a.index);
            let (glass_c, cube_c) = (before.per_id[1], before.per_id[2]);
            assert!(cube_c.pixels > 100, "adapter {}: the cube shows: {cube_c:?}", a.index);
            // A panel over the whole glass pane (and the wall around it), one over the cube's left
            // half, and one hanging off the image's corner.
            let glass_box = if before.blended_counted {
                [glass_c.min[0] - 2, glass_c.min[1] - 2, glass_c.max[0] - glass_c.min[0] + 5, glass_c.max[1] - glass_c.min[1] + 5]
            } else {
                [60, 60, 80, 120] // uncounted: any panel over the left part
            };
            let mid = (cube_c.min[0] + cube_c.max[0]) / 2;
            let half = [cube_c.min[0], 0, mid - cube_c.min[0], h];
            let corner = [w - 10, h - 10, 40, 40];
            let rects = [glass_box, half, corner];
            gpu.render_with_ids_occluded(target, &frame, &[1, 2, 3], &rects).unwrap();
            assert!(gpu.read_rgba8(target).unwrap() == plain, "adapter {}: occluders change the image", a.index);
            let img = gpu.read_ids(target).unwrap();
            let after = gpu.id_coverage(target, 3).unwrap();
            let inside = |p: usize| {
                let (x, y) = ((p as u32) % w, (p as u32) / w);
                rects.iter().any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
            };
            // Opaque ids: exactly the earlier pixels outside the panels.
            for (p, (&now, &was)) in img.ids.iter().zip(&before_img.ids).enumerate() {
                assert_eq!(now, if inside(p) { 0 } else { was }, "adapter {}: pixel {p}", a.index);
            }
            let wall_left = before_img.ids.iter().enumerate().filter(|&(p, &id)| id == 1 && !inside(p)).count() as u64;
            assert_eq!(after.per_id[0].opaque_pixels, wall_left, "adapter {}: the wall keeps its uncovered pixels", a.index);
            // The cube: half hidden, its box starts at the panel's edge.
            let c = after.per_id[2];
            assert!(c.pixels > 0 && c.pixels < cube_c.pixels * 2 / 3, "adapter {}: half the cube: {c:?} of {cube_c:?}", a.index);
            assert_eq!(c.min[0], mid, "adapter {}: its box starts right of the panel", a.index);
            // The glass: entirely under a panel: nothing counted, yet rasterized (so: occluded).
            let g = after.per_id[1];
            if after.blended_counted {
                assert_eq!((g.pixels, g.rasterized), (0, Some(true)), "adapter {}: the covered pane: {g:?}", a.index);
                assert_eq!(c.rasterized, Some(true));
            } else {
                assert!(!g.counted);
            }
            let many = vec![[0, 0, 1, 1]; crate::MAX_ID_OCCLUDERS + 1];
            let e = gpu.render_with_ids_occluded(target, &frame, &[1, 2, 3], &many).unwrap_err();
            assert_eq!(e.limit.map(|l| l.what), Some(crate::Limit::IdOccluders), "{e}");
            gpu.destroy_target(target);
            gpu.destroy_mesh(mesh);
            let errors: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= crate::Severity::Warning).collect();
            assert!(errors.is_empty(), "adapter {}: {errors:#?}", a.index);
        }
    }
}
