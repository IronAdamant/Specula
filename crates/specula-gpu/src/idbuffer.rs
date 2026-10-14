//! Sight's ID buffer: which draw covers each pixel of an offscreen capture.
//!
//! [`crate::Gpu::render_with_ids`] renders a frame as usual and, in the same submission, two more
//! graph passes (declared after the frame's own, so the image the frame produces is unchanged
//! bit for bit):
//!
//! - `id_buffer` draws every visible draw record again, position only, writing the caller's id
//!   for that draw into an `R32G32_UINT` image with its own D32 depth (reversed or standard, as
//!   the frame). **Opaque** draws (`Material::Lit`) are depth tested and written and store their
//!   id in **R**: the nearest opaque surface per pixel, exactly the depth the scene resolves.
//!   **Blended** draws (glass, blob shadows, lava lamps) are depth tested but don't write depth,
//!   and store their id in **G**, drawn far to near like the scene, so G is the front-most blended
//!   surface in front of the opaque one. Glass is therefore visible itself but never hides what
//!   is behind it. Blended materials count their whole mesh (a lava lamp its proxy box, a blob
//!   shadow its quad): the ID pass doesn't evaluate their shaders. Nothing is alpha tested.
//! - `id_copy` copies the image into a host-visible buffer; the graph adds the transfer → host
//!   barrier, and [`crate::Gpu::read_ids`] reads it after the render's fence.
//!
//! Frustum culling applies as in the scene (a culled draw has no pixels). Ids are the caller's
//! (0 = don't count); sight passes small dense indices so counting is a table lookup.
//!
//! The window path never declares these passes.

use crate::batch::Plan;
use crate::device::{Buffer, Device};
use crate::graph::{Final, ImageDesc, Load, Prior, Res};
use crate::render::{FrameContext, FrameGraph, FramePass, DEPTH_FORMAT, PUSH_STAGES};
use crate::vk::*;
use crate::{GpuError, MemoryKind};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

/// `VK_FORMAT_R32G32_UINT` (a mandatory colour attachment format).
pub const ID_FORMAT: VkFormat = 101;
/// `VK_COLOR_COMPONENT_R_BIT` / `_G_BIT`: the opaque pipeline writes R, the blended one G.
pub const WRITE_R: VkFlags = 0x1;
pub const WRITE_G: VkFlags = 0x2;
/// Bytes per pixel of the ID image and its readback.
pub const BYTES_PER_PIXEL: u64 = 8;

/// Push constants of the ID pipelines; must match `Push` in `shaders/ids.vert`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IdPush {
    pub frame: u64,
    pub draws: u64,
    pub ids: u64,
}

/// The ID passes' resources in a frame's graph.
#[derive(Clone, Copy, Debug, Default)]
pub struct IdRes {
    pub ids: Res,
    pub depth: Res,
    pub readback: Res,
}

/// Declares the ID passes after the frame's own passes in `g`.
pub fn declare(g: &mut FrameGraph, extent: VkExtent2D) -> IdRes {
    let (w, h) = (extent.width, extent.height);
    let ids = g.transient("ids", ImageDesc::color(ID_FORMAT, w, h, MemoryKind::RenderTargets));
    let depth = g.transient("ids_depth", ImageDesc::depth(DEPTH_FORMAT, w, h, MemoryKind::RenderTargets));
    g.pass("id_buffer", FramePass::IdBuffer).color(ids, Load::Clear).depth(depth, Load::Clear);
    let bytes = w as u64 * h as u64 * BYTES_PER_PIXEL;
    let readback = g.import_buffer("id_readback", bytes, Prior::Discard { after: VK_PIPELINE_STAGE_2_HOST_BIT }, Final::HostRead);
    g.pass("id_copy", FramePass::IdCopy).transfer_src(ids).transfer_dst(readback);
    IdRes { ids, depth, readback }
}

/// What recording the ID passes needs beyond the frame's context.
#[derive(Clone, Copy, Debug)]
pub struct IdFrame {
    pub opaque: VkPipeline,
    pub blended: VkPipeline,
    pub layout: VkPipelineLayout,
    /// Device address of the per-record ids.
    pub ids_address: u64,
    pub res: IdRes,
    pub readback: VkBuffer,
    pub extent: VkExtent2D,
}

/// The id of each draw record, in record order (`plan.entries`), from one id per draw.
pub fn record_ids(plan: &Plan, draw_ids: &[u32], out: &mut Vec<u32>) {
    out.clear();
    out.extend(plan.entries.iter().map(|&(i, _)| draw_ids.get(i).copied().unwrap_or(0)));
}

/// Records ID pass `p` (its barriers are the graph's).
pub fn record(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, tag: FramePass, f: &IdFrame) {
    let dev = ctx.dev;
    match tag {
        FramePass::IdBuffer => record_ids_pass(ctx, cmd, g, p, f),
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
            // SAFETY: recording; the graph moved the ids image to TRANSFER_SRC_OPTIMAL and the
            // readback buffer holds width × height × 8 bytes.
            unsafe { (dev.fns.cmd_copy_image_to_buffer)(cmd, image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, f.readback, 1, &region) };
        }
        _ => unreachable!("not an ID pass"),
    }
}

fn record_ids_pass(ctx: &FrameContext, cmd: VkCommandBuffer, g: &FrameGraph, p: usize, f: &IdFrame) {
    let (dev, frame, upload) = (ctx.dev, ctx.frame, ctx.upload);
    let color = g.color_attachment(p, f.res.ids, [0.0; 4]); // 0 bits: no id
    let depth = g.depth_attachment(p, f.res.depth, if frame.reverse_z { 0.0 } else { 1.0 }, None);
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
    let push = IdPush { frame: upload.frame_address(), draws: upload.records_address(), ids: f.ids_address };
    let compare = if frame.reverse_z { VK_COMPARE_OP_GREATER } else { VK_COMPARE_OP_LESS };
    // SAFETY: `cmd` is recording; every referenced struct outlives its call; both pipelines use
    // `f.layout` and declare the depth compare op dynamic; mesh buffers are live (checked by
    // `check_frame`); the frame upload holds one record per plan entry and the ids buffer one id
    // per record, so every instance index is in bounds.
    unsafe {
        (dev.fns.cmd_begin_rendering)(cmd, &rendering);
        (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
        (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
        let mut bound = VK_NULL_HANDLE;
        let mut bound_mesh = None;
        for batch in &ctx.scratch.plan.batches {
            let Some(mesh) = ctx.meshes.get(&batch.mesh.0) else { continue };
            let pipeline = if batch.material.blended() { f.blended } else { f.opaque };
            if pipeline != bound {
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
                (dev.fns.cmd_set_depth_compare_op)(cmd, compare);
                (dev.fns.cmd_push_constants)(cmd, f.layout, PUSH_STAGES, 0, size_of::<IdPush>() as u32, &push as *const IdPush as *const c_void);
                bound = pipeline;
            }
            if bound_mesh != Some(batch.mesh) {
                let offset: VkDeviceSize = 0;
                (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &mesh.buffer.handle, &offset);
                (dev.fns.cmd_bind_index_buffer)(cmd, mesh.buffer.handle, mesh.index_offset, VK_INDEX_TYPE_UINT32);
                bound_mesh = Some(batch.mesh);
            }
            (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, batch.count, 0, 0, batch.first);
        }
        (dev.fns.cmd_end_rendering)(cmd);
    }
}

/// One `u32` id per draw record, read by `ids.vert` through its device address. Host-visible,
/// persistently mapped, grown on demand. Offscreen renders are synchronous, so it is never in use
/// when written.
pub struct IdUpload {
    buffer: Buffer,
    address: u64,
    capacity: usize,
}

impl IdUpload {
    fn create(dev: &Device, capacity: usize) -> Result<IdUpload, GpuError> {
        let buffer = dev.create_buffer(
            (capacity.max(1) * 4) as u64,
            VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT,
            VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
            VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT,
            MemoryKind::PerFrame,
            "id upload",
        )?;
        let address = dev.buffer_address(&buffer);
        Ok(IdUpload { buffer, address, capacity: capacity.max(1) })
    }

    /// Writes `ids` (growing the buffer when needed, at least doubling) into `slot`, creating it
    /// on first use. The GPU must not be reading the old buffer.
    pub fn write(slot: &mut Option<IdUpload>, dev: &Device, ids: &[u32]) -> Result<u64, GpuError> {
        if slot.as_ref().is_none_or(|u| u.capacity < ids.len()) {
            let want = ids.len().max(slot.as_ref().map_or(256, |u| u.capacity * 2));
            let fresh = IdUpload::create(dev, want)?;
            if let Some(old) = slot.replace(fresh) {
                old.destroy(dev);
            }
        }
        let u = slot.as_ref().expect("just made");
        // SAFETY: `mapped` is coherent host memory of `capacity` u32s, at least `ids.len()`, and
        // the GPU no longer reads it (offscreen renders wait for completion).
        unsafe { ptr::copy_nonoverlapping(ids.as_ptr(), u.buffer.mapped as *mut u32, ids.len()) };
        Ok(u.address)
    }

    pub fn destroy(&self, dev: &Device) {
        dev.destroy_buffer(&self.buffer);
    }
}

/// Per id: pixels covered and the inclusive pixel bounds of those pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdCoverage {
    pub pixels: u64,
    /// `[x, y]` of the first and last covered column and row.
    pub min: [u32; 2],
    pub max: [u32; 2],
}

/// An ID buffer read back: per pixel `[opaque id, front-most blended id]` (0 = none), rows top
/// to bottom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdImage {
    pub width: u32,
    pub height: u32,
    pub ids: Vec<[u32; 2]>,
}

impl IdImage {
    /// Coverage of ids `1..=max_id` (index `id - 1`): a pixel counts once for each distinct id
    /// in it (its opaque surface and the glass in front, unless they are the same entity).
    /// Ids above `max_id` are ignored.
    pub fn coverage(&self, max_id: u32) -> Vec<Option<IdCoverage>> {
        coverage(&self.ids, self.width, max_id)
    }
}

/// [`IdImage::coverage`] of `ids` rows `width` pixels wide.
pub fn coverage(ids: &[[u32; 2]], width: u32, max_id: u32) -> Vec<Option<IdCoverage>> {
    let mut out: Vec<Option<IdCoverage>> = vec![None; max_id as usize];
    // Runs of one id along a row are added at once (most pixels continue a run).
    let mut add = |id: u32, x0: u32, x1: u32, y: u32| {
        if id == 0 || id > max_id {
            return;
        }
        let n = u64::from(x1 - x0 + 1);
        let c = &mut out[(id - 1) as usize];
        match c {
            Some(c) => {
                c.pixels += n;
                c.min = [c.min[0].min(x0), c.min[1].min(y)];
                c.max = [c.max[0].max(x1), c.max[1].max(y)];
            }
            None => *c = Some(IdCoverage { pixels: n, min: [x0, y], max: [x1, y] }),
        }
    };
    let w = width as usize;
    if w == 0 {
        return out;
    }
    // Blended ids count where they differ from the opaque one.
    let blended = |p: [u32; 2]| if p[1] != p[0] { p[1] } else { 0 };
    for (y, row) in ids.chunks_exact(w).enumerate() {
        let y = y as u32;
        let mut prev = row[0];
        let (mut o_id, mut o_start) = (prev[0], 0u32);
        let (mut b_id, mut b_start) = (blended(prev), 0u32);
        for (x, &p) in row.iter().enumerate().skip(1) {
            if p == prev {
                continue; // both runs go on
            }
            prev = p;
            let x = x as u32;
            if p[0] != o_id {
                add(o_id, o_start, x - 1, y);
                (o_id, o_start) = (p[0], x);
            }
            let b = blended(p);
            if b != b_id {
                add(b_id, b_start, x - 1, y);
                (b_id, b_start) = (b, x);
            }
        }
        let last = w as u32 - 1;
        add(o_id, o_start, last, y);
        add(b_id, b_start, last, y);
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
    fn coverage_counts_each_entity_once_per_pixel() {
        // 3x2: entity 1 opaque at (0,0),(1,0); entity 2 glass over (1,0) and (2,1); entity 3's
        // opaque part with its own glass in front at (0,1).
        let img = IdImage { width: 3, height: 2, ids: vec![[1, 0], [1, 2], [0, 0], [3, 3], [0, 0], [0, 2]] };
        let c = img.coverage(3);
        assert_eq!(c[0], Some(IdCoverage { pixels: 2, min: [0, 0], max: [1, 0] }));
        assert_eq!(c[1], Some(IdCoverage { pixels: 2, min: [1, 0], max: [2, 1] }));
        assert_eq!(c[2], Some(IdCoverage { pixels: 1, min: [0, 1], max: [0, 1] }));
        assert_eq!(img.coverage(1).len(), 1, "ids above max are ignored");
    }

    #[test]
    fn run_length_coverage_matches_pixel_by_pixel() {
        let (w, h) = (37u32, 23u32);
        let mut seed = 12345u64;
        let mut next = |n: u32| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) % u64::from(n)) as u32
        };
        // Runs of random ids (0..=6), glass over some.
        let mut ids = Vec::new();
        let (mut o, mut b) = (0, 0);
        for _ in 0..w * h {
            if next(5) == 0 {
                o = next(7);
            }
            if next(7) == 0 {
                b = if next(2) == 0 { 0 } else { next(7) };
            }
            ids.push([o, b]);
        }
        let img = IdImage { width: w, height: h, ids };
        let mut naive: Vec<Option<IdCoverage>> = vec![None; 5];
        for (i, p) in img.ids.iter().enumerate() {
            let (x, y) = (i as u32 % w, i as u32 / w);
            let distinct: &[u32] = if p[0] == p[1] { &p[..1] } else { &p[..] };
            for &id in distinct {
                if id == 0 || id > 5 {
                    continue;
                }
                let c = naive[(id - 1) as usize].get_or_insert(IdCoverage { pixels: 0, min: [x, y], max: [x, y] });
                c.pixels += 1;
                c.min = [c.min[0].min(x), c.min[1].min(y)];
                c.max = [c.max[0].max(x), c.max[1].max(y)];
            }
        }
        assert_eq!(img.coverage(5), naive);
    }

    fn cube() -> (Vec<Vertex>, Vec<u32>) {
        let mut v = Vec::new();
        let mut idx = Vec::new();
        for axis in 0..3 {
            for sign in [-0.5f32, 0.5] {
                let base = v.len() as u32;
                for (a, b) in [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                    let mut p = [0.0; 3];
                    p[axis] = sign;
                    p[(axis + 1) % 3] = a;
                    p[(axis + 2) % 3] = b;
                    let mut n = [0.0; 3];
                    n[axis] = sign * 2.0;
                    v.push(Vertex::new(p, n));
                }
                idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        (v, idx)
    }

    fn trs(t: [f32; 3], s: [f32; 3]) -> [f32; 16] {
        [s[0], 0.0, 0.0, 0.0, 0.0, s[1], 0.0, 0.0, 0.0, 0.0, s[2], 0.0, t[0], t[1], t[2], 1.0]
    }

    /// A wall hides one cube fully and another half; glass in front hides nothing. Counts and
    /// extents are checked against what the projection says they must be, on every adapter.
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
            let draws = [
                // 1: the wall, x -2..2, y -1.5..1.5, z -0.1..0.1.
                DrawItem::new(mesh, trs([0.0, 0.0, 0.0], [4.0, 3.0, 0.2]), [0.5, 0.5, 0.5, 1.0]),
                // 2: fully behind the wall.
                DrawItem::new(mesh, trs([0.0, 0.0, -3.0], [1.0, 1.0, 1.0]), [1.0, 0.0, 0.0, 1.0]),
                // 3: half behind the wall's right edge: the edge (x = 2 at 9.9 m) projects to
                // x = 2 * 12.5 / 9.9 at the cube's front face (12.5 m), so centre it there.
                DrawItem::new(mesh, trs([2.0 * 12.5 / 9.9, 0.0, -3.0], [1.0, 1.0, 1.0]), [0.0, 1.0, 0.0, 1.0]),
                // 4: glass in front of the wall's left half.
                DrawItem { material: Material::Glass, ..DrawItem::new(mesh, trs([-1.0, 0.0, 2.0], [1.0, 1.0, 0.1]), [0.8, 0.9, 1.0, 0.2]) },
            ];
            let frame = FrameDesc { view_proj: vp, draws: &draws, reverse_z: true, ..Default::default() };
            gpu.render(target, &frame).unwrap();
            let plain = gpu.read_rgba8(target).unwrap();
            assert!(gpu.read_ids(target).is_err(), "a plain render has no ID buffer");
            let wrong = gpu.render_with_ids(target, &frame, &[1, 2]).unwrap_err();
            assert!(matches!(wrong.reason, Some(crate::ErrorReason::DataLength { given: 2, expected: 4 })), "{wrong}");
            gpu.render_with_ids(target, &frame, &[1, 2, 3, 4]).unwrap();
            assert!(gpu.read_rgba8(target).unwrap() == plain, "adapter {}: the ID passes change the image", a.index);
            let img = gpu.read_ids(target).unwrap();
            assert_eq!((img.width, img.height, img.ids.len()), (w, h, (w * h) as usize));
            let c = img.coverage(4);
            assert_eq!(gpu.id_coverage(target, 4).unwrap(), c, "counted in place, the same");
            assert!(c[1].is_none(), "adapter {}: the cube behind the wall shows {:?}", a.index, c[1]);
            let half = c[2].expect("the half-hidden cube shows");
            // Its front face's full projected square: side = 1 / (12.5 tan 20°) of the height.
            let side = 1.0 / (12.5 * 20f32.to_radians().tan()) * 0.5 * h as f32;
            let ratio = half.pixels as f32 / (side * side);
            assert!((0.4..0.6).contains(&ratio), "adapter {}: half hidden shows {ratio} of its area", a.index);
            // Exact extents: the wall's front face (z = 0.1) projected.
            let wall = c[0].unwrap();
            let (x0, y0) = project([-2.0, 1.5, 0.1]);
            let (x1, y1) = project([2.0, -1.5, 0.1]);
            for (got, want) in [(wall.min[0], x0), (wall.min[1], y0), (wall.max[0] + 1, x1), (wall.max[1] + 1, y1)] {
                assert!((got as f32 - want).abs() <= 1.0, "adapter {}: wall extent {got} vs projected {want}", a.index);
            }
            // The glass is visible and doesn't hide the wall behind it.
            let glass = c[3].expect("glass shows");
            assert!(glass.pixels > 100);
            let wall_area = (x1 - x0) * (y1 - y0);
            assert!((wall.pixels as f32 - wall_area).abs() / wall_area < 0.02, "adapter {}: wall {} px of {wall_area}", a.index, wall.pixels);
            gpu.destroy_target(target);
            gpu.destroy_mesh(mesh);
            let errors: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity == crate::Severity::Error).collect();
            assert!(errors.is_empty(), "adapter {}: {errors:#?}", a.index);
        }
    }
}
