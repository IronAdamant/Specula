//! The sun's cascaded shadow map: fitting cascades to the view (pure maths), choosing casters per
//! cascade (instanced batches), the shadow map's description and the depth-only pass.
//!
//! Light space: `x = right·p`, `y = up·p`, `z = forward·p` with `forward` the direction the light
//! travels (−sun direction), so depth grows away from the light. A cascade is a square of half
//! size `radius` around a slice of the view frustum, its centre snapped to whole texels, so a
//! moving camera slides the texel grid by whole texels only (no shimmer).

use crate::batch::{world_sphere, Bounds, Plan};
use crate::device::Device;
use crate::graph::{ImageDesc, Res};
use crate::render::{FrameGraph, FrameUpload, Mesh, PushConstants, PUSH_STAGES};
use crate::vk::*;
use crate::{DrawStats, FrameDesc, Material, MemoryKind, MeshId, ShadowSettings, MAX_SHADOW_CASCADES};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

pub const SHADOW_FORMAT: VkFormat = VK_FORMAT_D16_UNORM;
const CASCADES: usize = MAX_SHADOW_CASCADES as usize;
/// Casters this far (m) towards the light from a cascade's box are still included; beyond, they
/// are clipped (this bounds the depth range, so the 16-bit depth keeps its precision).
const MAX_CASTER_REACH: f32 = 500.0;
/// Weight of the logarithmic split (vs. linear) in the cascade split scheme.
const SPLIT_LAMBDA: f32 = 0.7;

type M4 = [f32; 16];

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 1e-12 && l.is_finite() {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Inverse of a column-major 4×4 matrix (f64 Gauss–Jordan); `None` if singular.
pub fn invert(m: &M4) -> Option<M4> {
    let mut a = [[0.0f64; 8]; 4];
    for (r, row) in a.iter_mut().enumerate() {
        for c in 0..4 {
            row[c] = m[c * 4 + r] as f64;
        }
        row[4 + r] = 1.0;
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-15 || !a[pivot][col].is_finite() {
            return None;
        }
        a.swap(col, pivot);
        let p = a[col][col];
        for v in a[col].iter_mut() {
            *v /= p;
        }
        let pivot_row = a[col];
        for (r, row) in a.iter_mut().enumerate() {
            if r != col {
                let f = row[col];
                for (cell, pv) in row.iter_mut().zip(pivot_row) {
                    *cell -= f * pv;
                }
            }
        }
    }
    let mut out = [0.0f32; 16];
    for (r, row) in a.iter().enumerate() {
        for c in 0..4 {
            out[c * 4 + r] = row[4 + c] as f32;
        }
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}

fn transform(m: &M4, p: [f32; 3]) -> [f32; 4] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn unproject(inv: &M4, x: f32, y: f32, z: f32) -> [f32; 3] {
    let h = transform(inv, [x, y, z]);
    [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
}

/// View depth as a linear function of world position: `depth(p) = dir·p + offset`, growing away
/// from the camera. Perspective: the clip `w` (distance along the view axis, from the camera).
/// Orthographic: distance from the near plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthAxis {
    pub dir: [f32; 3],
    pub offset: f32,
}

impl DepthAxis {
    pub fn depth(&self, p: [f32; 3]) -> f32 {
        dot(self.dir, p) + self.offset
    }
}

/// The view frustum as four corner rays (top-left, top-right, bottom-right, bottom-left), each
/// `origin + t·dir`, plus the depth axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRays {
    pub origins: [[f32; 3]; 4],
    pub dirs: [[f32; 3]; 4],
    pub axis: DepthAxis,
    /// Depth of the near plane.
    pub near: f32,
}

impl ViewRays {
    /// `None` for a singular matrix.
    pub fn new(view_proj: &M4, reverse_z: bool) -> Option<ViewRays> {
        let inv = invert(view_proj)?;
        let (z_near, z_mid) = if reverse_z { (1.0, 0.5) } else { (0.0, 0.5) };
        let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let origins = corners.map(|(x, y)| unproject(&inv, x, y, z_near));
        let mids = corners.map(|(x, y)| unproject(&inv, x, y, z_mid));
        let dirs: [[f32; 3]; 4] = std::array::from_fn(|i| {
            let (o, m) = (origins[i], mids[i]);
            [m[0] - o[0], m[1] - o[1], m[2] - o[2]]
        });
        let near_c = unproject(&inv, 0.0, 0.0, z_near);
        let mid_c = unproject(&inv, 0.0, 0.0, z_mid);
        let forward = normalize([mid_c[0] - near_c[0], mid_c[1] - near_c[1], mid_c[2] - near_c[2]]);
        let w = [view_proj[3], view_proj[7], view_proj[11]];
        let wl = dot(w, w).sqrt();
        let axis = if wl > 1e-6 && dot(w, forward) > 0.0 {
            DepthAxis { dir: [w[0] / wl, w[1] / wl, w[2] / wl], offset: view_proj[15] / wl }
        } else {
            DepthAxis { dir: forward, offset: -dot(forward, near_c) }
        };
        let rays = ViewRays { origins, dirs, axis, near: axis.depth(near_c) };
        let ok = rays.dirs.iter().all(|d| dot(axis.dir, *d) > 1e-9) && rays.near.is_finite();
        ok.then_some(rays)
    }

    /// The four corners of the frustum cross-section at view depth `d`.
    pub fn corners_at(&self, d: f32) -> [[f32; 3]; 4] {
        std::array::from_fn(|i| {
            let (o, v) = (self.origins[i], self.dirs[i]);
            let t = (d - self.axis.depth(o)) / dot(self.axis.dir, v);
            [o[0] + v[0] * t, o[1] + v[1] * t, o[2] + v[2] * t]
        })
    }
}

/// Orthonormal light basis: `right`, `up`, `forward` (the direction light travels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightBasis {
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
}

impl LightBasis {
    pub fn new(sun_direction: [f32; 3]) -> LightBasis {
        let to_sun = normalize(sun_direction);
        let forward = [-to_sun[0], -to_sun[1], -to_sun[2]];
        let hint = if forward[1].abs() > 0.99 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let right = normalize(cross(forward, hint));
        let up = cross(right, forward);
        LightBasis { right, up, forward }
    }

    pub fn project(&self, p: [f32; 3]) -> [f32; 3] {
        [dot(self.right, p), dot(self.up, p), dot(self.forward, p)]
    }
}

/// One cascade, in light space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cascade {
    /// Snapped centre (x, y) and the centre's depth z.
    pub center: [f32; 3],
    /// Half size of the square (m).
    pub radius: f32,
    /// Light-space depth range mapped to 0..1.
    pub z_near: f32,
    pub z_far: f32,
    /// World size of one texel.
    pub texel: f32,
    /// View depth where this cascade's slice ends.
    pub split_far: f32,
}

impl Cascade {
    /// World → shadow clip (x, y in −1..1, depth 0..1), column-major.
    pub fn matrix(&self, b: &LightBasis) -> M4 {
        let (r, u, f) = (b.right, b.up, b.forward);
        let s = 1.0 / self.radius;
        let dz = 1.0 / (self.z_far - self.z_near).max(1e-6);
        [
            r[0] * s, u[0] * s, f[0] * dz, 0.0, //
            r[1] * s, u[1] * s, f[1] * dz, 0.0, //
            r[2] * s, u[2] * s, f[2] * dz, 0.0, //
            -self.center[0] * s, -self.center[1] * s, -self.z_near * dz, 1.0,
        ]
    }

    /// Whether a light-space sphere can cast into this cascade.
    fn may_cast(&self, l: [f32; 3], r: f32) -> bool {
        (l[0] - self.center[0]).abs() <= self.radius + r && (l[1] - self.center[1]).abs() <= self.radius + r && l[2] - r <= self.z_far
    }
}

/// `x` rounded down (`up` false) or up to a quarter octave (…, 1, 1.19, 1.41, 1.68, 2, …), so
/// small changes in what is visible don't resize the cascades every frame.
fn quantize(x: f32, up: bool) -> f32 {
    if x.is_nan() || x <= 0.0 || !x.is_finite() {
        return x;
    }
    let steps = x.log2() * 4.0;
    let steps = if up { steps.ceil() } else { steps.floor() };
    (steps / 4.0).exp2()
}

/// View depths where the cascades end (the practical split scheme between `near` and `far`).
pub fn splits(near: f32, far: f32, count: usize) -> [f32; CASCADES] {
    let mut out = [far; CASCADES];
    let near = near.max(1e-3);
    for (i, s) in out.iter_mut().enumerate().take(count) {
        let t = (i + 1) as f32 / count as f32;
        let log = near * (far / near).powf(t);
        let lin = near + (far - near) * t;
        *s = SPLIT_LAMBDA * log + (1.0 - SPLIT_LAMBDA) * lin;
    }
    out[count - 1] = far;
    out
}

/// Fit `count` cascades of `resolution` texels to view depths `range` (already clamped to the
/// shadow distance). The centre is snapped to whole texels.
pub fn fit_cascades(rays: &ViewRays, basis: &LightBasis, range: (f32, f32), count: usize, resolution: u32) -> [Cascade; CASCADES] {
    let mut out = [Cascade::default(); CASCADES];
    let ends = splits(range.0, range.1, count);
    let mut start = range.0;
    for (c, &end) in out.iter_mut().zip(&ends).take(count) {
        let mut pts = [[0.0f32; 3]; 8];
        pts[..4].copy_from_slice(&rays.corners_at(start));
        pts[4..].copy_from_slice(&rays.corners_at(end));
        let mut centre = [0.0f32; 3];
        for p in &pts {
            for k in 0..3 {
                centre[k] += p[k] / 8.0;
            }
        }
        let r2 = pts.iter().map(|p| {
            let d = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
            dot(d, d)
        });
        // Rounded up to 1/64 m: corners move rigidly with the camera, so this is constant while
        // the slice is; rounding absorbs float noise.
        let radius = (r2.fold(0.0f32, f32::max).sqrt() * 64.0).ceil().max(1.0) / 64.0;
        let l = basis.project(centre);
        let texel = 2.0 * radius / resolution as f32;
        let snap = |v: f32| (v / texel).round() * texel;
        *c = Cascade {
            center: [snap(l[0]), snap(l[1]), l[2]],
            radius,
            z_near: l[2] - radius,
            z_far: l[2] + radius,
            texel,
            split_far: end,
        };
        start = end;
    }
    out
}

/// One instanced depth-only draw: records `first .. first + count` (indices into the frame's
/// record array) into cascade `cascade`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowBatch {
    pub cascade: u32,
    pub mesh: MeshId,
    pub first: u32,
    pub count: u32,
}

/// What the shaders need to sample the map (packed into the frame data).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowData {
    /// Cascades in use (0 = no shadows this frame).
    pub count: u32,
    pub matrices: [M4; CASCADES],
    /// World size of a texel per cascade.
    pub texel: [f32; CASCADES],
    pub axis: [f32; 4],
    /// View depths where shadows start and finish fading out.
    pub fade: [f32; 2],
    /// Filter radius in texels of cascade 0, and the receiver normal offset in texels.
    pub softness: f32,
    pub normal_offset: f32,
}

/// The shadow pass of a frame. Reuse one across frames: no allocation once warm.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowPlan {
    pub data: ShadowData,
    pub cascades: [Cascade; CASCADES],
    /// Draw indices in record order (all cascades, cascade by cascade).
    pub entries: Vec<usize>,
    /// Batches; `first` is relative to the start of `entries`.
    pub batches: Vec<ShadowBatch>,
    /// Light-space sphere of every draw (`None` = doesn't cast), scratch.
    spheres: Vec<Option<([f32; 3], f32)>>,
    keys: Vec<(u32, u32, usize)>,
}

/// Plan the shadow pass for `frame` after its camera `plan`: fit the cascades to what the camera
/// sees and pick each cascade's casters. Leaves `out.data.count == 0` when shadows are off, the
/// view is degenerate, or nothing visible lies within the shadow distance.
pub fn plan_shadows(frame: &FrameDesc, plan: &Plan, bounds: impl Fn(MeshId) -> Option<Bounds>, out: &mut ShadowPlan) {
    out.data.count = 0;
    out.entries.clear();
    out.batches.clear();
    let s: &ShadowSettings = &frame.environment.shadow;
    if !s.enabled || plan.entries.is_empty() {
        return;
    }
    let Some(rays) = ViewRays::new(&frame.view_proj, frame.reverse_z) else { return };
    let count = s.cascades.clamp(1, MAX_SHADOW_CASCADES) as usize;
    let resolution = clamp_resolution(s.resolution);
    let distance = if s.distance.is_finite() { s.distance.max(rays.near + 0.1) } else { 1e4 };
    // Receivers: the depth range of everything the camera sees.
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &(i, _) in &plan.entries {
        let d = &frame.draws[i];
        let (c, r) = match bounds(d.mesh) {
            Some(b) => world_sphere(&d.model, b),
            None => ([d.model[12], d.model[13], d.model[14]], f32::INFINITY),
        };
        let depth = rays.axis.depth(c);
        if depth.is_finite() && r.is_finite() {
            lo = lo.min(depth - r);
            hi = hi.max(depth + r);
        } else {
            lo = lo.min(rays.near);
            hi = hi.max(distance);
        }
    }
    let near = quantize(lo.max(rays.near).max(1e-3), false).max(rays.near);
    let far = quantize(hi, true).min(distance);
    if far.is_nan() || near.is_nan() || far <= near {
        return;
    }
    let basis = LightBasis::new(frame.environment.sun_direction);
    out.cascades = fit_cascades(&rays, &basis, (near, far), count, resolution);

    // Casters: opaque draws, as light-space spheres.
    out.spheres.clear();
    out.spheres.extend(frame.draws.iter().map(|d| {
        if d.material != Material::Lit {
            return None;
        }
        let (c, r) = match bounds(d.mesh) {
            Some(b) => world_sphere(&d.model, b),
            None => ([d.model[12], d.model[13], d.model[14]], f32::INFINITY),
        };
        let l = basis.project(c);
        (l.iter().all(|v| v.is_finite()) && !r.is_nan()).then_some((l, r))
    }));
    for k in 0..count {
        let cascade = &mut out.cascades[k];
        out.keys.clear();
        let mut z_near = cascade.z_near;
        for (i, s) in out.spheres.iter().enumerate() {
            let Some((l, r)) = *s else { continue };
            if cascade.may_cast(l, r) {
                z_near = z_near.min(l[2] - r);
                // Near the light first (early depth rejection), batched by mesh.
                let depth = (l[2] - r - cascade.z_near + MAX_CASTER_REACH).max(0.0);
                out.keys.push((frame.draws[i].mesh.0, depth.to_bits(), i));
            }
        }
        cascade.z_near = z_near.max(cascade.z_far - 2.0 * cascade.radius - MAX_CASTER_REACH);
        out.keys.sort_unstable();
        let base = out.entries.len() as u32;
        for (n, &(mesh, _, i)) in out.keys.iter().enumerate() {
            out.entries.push(i);
            match out.batches.last_mut() {
                Some(b) if b.cascade == k as u32 && b.mesh.0 == mesh => b.count += 1,
                _ => out.batches.push(ShadowBatch { cascade: k as u32, mesh: MeshId(mesh), first: base + n as u32, count: 1 }),
            }
        }
    }
    out.data = ShadowData {
        count: count as u32,
        matrices: std::array::from_fn(|k| if k < count { out.cascades[k].matrix(&basis) } else { [0.0; 16] }),
        texel: std::array::from_fn(|k| out.cascades[k].texel),
        axis: [rays.axis.dir[0], rays.axis.dir[1], rays.axis.dir[2], rays.axis.offset],
        fade: [far - 0.1 * (far - near), far],
        softness: s.softness.clamp(0.0, 16.0),
        normal_offset: 1.5 * s.bias.clamp(0.0, 16.0),
    };
}

pub fn clamp_resolution(r: u32) -> u32 {
    r.clamp(256, 8192)
}

/// The shadow map, a transient every graph shares: a D16 2D array (one layer per cascade), with a
/// view per layer to render into and an array view (descriptor binding 0) to sample. The array
/// view is `2D_ARRAY` over every layer, even when there is only one: the shaders sample it as
/// `sampler2DArrayShadow`, and a `2D` view there is undefined behaviour (GPU-assisted validation:
/// VUID-vkCmdDrawIndexed-viewType-07752).
pub fn shadow_desc(s: &ShadowSettings) -> ImageDesc {
    let resolution = clamp_resolution(s.resolution);
    ImageDesc {
        format: SHADOW_FORMAT,
        width: resolution,
        height: resolution,
        layers: s.cascades.clamp(1, MAX_SHADOW_CASCADES),
        array_view: true,
        bindless: false,
        memory: MemoryKind::ShadowMaps,
    }
}

/// Depth bias for `bias` (the settings' scale): (constant, slope) in D16 units.
pub fn depth_bias(bias: f32) -> (f32, f32) {
    let b = bias.clamp(0.0, 16.0);
    (1.25 * b, 1.75 * b)
}

/// Cutout sprites that cast shadows: `count` records at `push.items` (see `sprite.rs`).
#[derive(Clone, Copy)]
pub struct SpriteCasters {
    pub pipeline: VkPipeline,
    /// The shared quad's index buffer and offset (see `sprite::QUAD_MESH`).
    pub quad: (VkBuffer, VkDeviceSize),
    pub push: crate::sprite::SpritePush,
    pub count: u32,
}

/// Record the shadow pass `p` of `g` into `map`: every cascade's layer cleared and drawn
/// depth-only (the graph owns the barriers around it), meshes then shadow-casting sprites.
/// Updates `stats`' shadow counters.
#[allow(clippy::too_many_arguments)]
pub fn record_shadow_pass(
    dev: &Device,
    cmd: VkCommandBuffer,
    pipeline: VkPipeline,
    layout: VkPipelineLayout,
    meshes: &HashMap<u32, Mesh>,
    g: &FrameGraph,
    p: usize,
    map: Res,
    plan: &ShadowPlan,
    first_record: u32,
    upload: &FrameUpload,
    bias: f32,
    sprites: Option<SpriteCasters>,
    stats: &mut DrawStats,
) {
    let img = g.image(map);
    let size = img.width;
    let area = VkRect2D { offset: VkOffset2D { x: 0, y: 0 }, extent: VkExtent2D { width: size, height: size } };
    let viewport = VkViewport { x: 0.0, y: 0.0, width: size as f32, height: size as f32, min_depth: 0.0, max_depth: 1.0 };
    let (constant, slope) = depth_bias(bias);
    let count = plan.data.count.min(img.layers);
    stats.shadow_cascades = count;
    for k in 0..count {
        let depth = g.depth_attachment(p, map, 1.0, Some(k));
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
        // SAFETY: `cmd` is recording; the pipeline is depth-only for SHADOW_FORMAT; every mesh in
        // a batch is live (`check_frame`); records `first_record + first ..` exist in `upload`.
        unsafe {
            (dev.fns.cmd_begin_rendering)(cmd, &rendering);
            (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
            (dev.fns.cmd_set_viewport)(cmd, 0, 1, &viewport);
            (dev.fns.cmd_set_scissor)(cmd, 0, 1, &area);
            (dev.fns.cmd_set_depth_bias)(cmd, constant, 0.0, slope);
            let pc = PushConstants { frame: upload.frame_address(), draws: upload.records_address(), material: 0, pass: k };
            (dev.fns.cmd_push_constants)(
                cmd,
                layout,
                PUSH_STAGES,
                0,
                size_of::<PushConstants>() as u32,
                &pc as *const PushConstants as *const c_void,
            );
            let mut bound = None;
            for b in plan.batches.iter().filter(|b| b.cascade == k) {
                let Some(mesh) = meshes.get(&b.mesh.0) else { continue };
                if bound != Some(b.mesh) {
                    let offset: VkDeviceSize = 0;
                    (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &mesh.buffer.handle, &offset);
                    (dev.fns.cmd_bind_index_buffer)(cmd, mesh.buffer.handle, mesh.index_offset, VK_INDEX_TYPE_UINT32);
                    bound = Some(b.mesh);
                }
                (dev.fns.cmd_draw_indexed)(cmd, mesh.index_count, b.count, 0, 0, first_record + b.first);
                stats.shadow_draw_calls += 1;
                stats.shadow_instances += b.count;
                stats.shadow_triangles += (mesh.index_count / 3) as u64 * b.count as u64;
            }
            if let Some(s) = sprites.filter(|s| s.count > 0) {
                // Every caster into every cascade: cards are cheap, and their shadows may reach
                // into view from anywhere.
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, s.pipeline);
                let push = crate::sprite::SpritePush { pass: k, ..s.push };
                (dev.fns.cmd_push_constants)(
                    cmd,
                    layout,
                    PUSH_STAGES,
                    0,
                    size_of::<crate::sprite::SpritePush>() as u32,
                    &push as *const crate::sprite::SpritePush as *const c_void,
                );
                (dev.fns.cmd_bind_index_buffer)(cmd, s.quad.0, s.quad.1, VK_INDEX_TYPE_UINT32);
                (dev.fns.cmd_draw_indexed)(cmd, 6, s.count, 0, 0, 0);
                stats.shadow_draw_calls += 1;
                stats.shadow_instances += s.count;
                stats.shadow_triangles += 2 * s.count as u64;
            }
            (dev.fns.cmd_end_rendering)(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::plan;
    use crate::{DrawItem, Environment};

    const IDENTITY: M4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

    fn mul(a: &M4, b: &M4) -> M4 {
        let mut o = [0.0; 16];
        for c in 0..4 {
            for r in 0..4 {
                o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
            }
        }
        o
    }

    fn look_at(eye: [f32; 3], target: [f32; 3]) -> M4 {
        let f = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
        let s = normalize(cross(f, [0.0, 1.0, 0.0]));
        let u = cross(s, f);
        [s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0]
    }

    fn camera(eye: [f32; 3], target: [f32; 3], reverse_z: bool) -> M4 {
        let f = 1.0 / (30f32.to_radians()).tan();
        let (near, far) = (0.05f32, 500.0f32);
        let proj = if reverse_z {
            [f / 1.6, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0]
        } else {
            [f / 1.6, 0.0, 0.0, 0.0, 0.0, -f, 0.0, 0.0, 0.0, 0.0, far / (near - far), -1.0, 0.0, 0.0, near * far / (near - far), 0.0]
        };
        mul(&proj, &look_at(eye, target))
    }

    fn at(x: f32, y: f32, z: f32) -> M4 {
        let mut m = IDENTITY;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    }

    #[test]
    fn inverse_round_trips() {
        let m = camera([3.0, 5.0, 9.0], [0.0, 0.0, 0.0], false);
        let inv = invert(&m).unwrap();
        let id = mul(&m, &inv);
        for (i, v) in id.iter().enumerate() {
            assert!((v - IDENTITY[i]).abs() < 1e-4, "{id:?}");
        }
        assert!(invert(&[0.0; 16]).is_none());
    }

    #[test]
    fn view_rays_measure_depth_from_the_camera() {
        for reverse_z in [false, true] {
            let vp = camera([0.0, 2.0, 10.0], [0.0, 2.0, 0.0], reverse_z);
            let rays = ViewRays::new(&vp, reverse_z).unwrap();
            assert!((rays.axis.depth([0.0, 2.0, 0.0]) - 10.0).abs() < 1e-3, "{:?}", rays.axis);
            assert!((rays.near - 0.05).abs() < 1e-3, "near {}", rays.near);
            // Corners at depth 10 lie on the plane z = 0, symmetric about the view axis.
            let c = rays.corners_at(10.0);
            for p in c {
                assert!(p[2].abs() < 1e-3, "{c:?}");
            }
            assert!((c[0][0] + c[1][0]).abs() < 1e-3 && (c[0][1] - 2.0 + c[3][1] - 2.0).abs() < 1e-3);
        }
    }

    #[test]
    fn light_basis_is_orthonormal_and_faces_down_the_light() {
        for sun in [[0.4, 0.8, 0.45], [0.0, 1.0, 0.0], [-0.3, 0.2, -0.9]] {
            let b = LightBasis::new(sun);
            for (a, c) in [(b.right, b.up), (b.up, b.forward), (b.right, b.forward)] {
                assert!(dot(a, c).abs() < 1e-5);
            }
            for v in [b.right, b.up, b.forward] {
                assert!((dot(v, v) - 1.0).abs() < 1e-5);
            }
            assert!(dot(b.forward, normalize(sun)) < -0.999, "forward points away from the sun");
        }
    }

    #[test]
    fn splits_grow_and_end_at_far() {
        let s = splits(2.0, 80.0, 3);
        assert!(s[0] > 2.0 && s[0] < s[1] && s[1] < s[2] && s[2] == 80.0, "{s:?}");
        assert_eq!(splits(2.0, 80.0, 1)[0], 80.0);
    }

    #[test]
    fn cascades_cover_their_slices_and_snap_to_texels() {
        let vp = camera([5.0, 12.0, 25.0], [0.0, 0.0, 0.0], true);
        let rays = ViewRays::new(&vp, true).unwrap();
        let basis = LightBasis::new([0.4, 0.8, 0.45]);
        let cascades = fit_cascades(&rays, &basis, (3.0, 60.0), 3, 1024);
        let mut start = 3.0;
        for c in &cascades[..3] {
            let m = c.matrix(&basis);
            for p in rays.corners_at(start).iter().chain(&rays.corners_at(c.split_far)) {
                let s = transform(&m, *p);
                assert!(s[0].abs() <= 1.0 + 1e-4 && s[1].abs() <= 1.0 + 1e-4, "corner outside: {s:?}");
                assert!((0.0..=1.0 + 1e-4).contains(&s[2]), "depth {}", s[2]);
            }
            for v in [c.center[0], c.center[1]] {
                let t = v / c.texel;
                assert!((t - t.round()).abs() < 1e-3, "centre on the texel grid: {t}");
            }
            start = c.split_far;
        }
        assert!(cascades[0].radius < cascades[1].radius && cascades[1].radius < cascades[2].radius);
        // Moving the camera sideways keeps the radius and moves the centre by whole texels.
        let moved = camera([5.3, 12.0, 25.0], [0.3, 0.0, 0.0], true);
        let rays2 = ViewRays::new(&moved, true).unwrap();
        let again = fit_cascades(&rays2, &basis, (3.0, 60.0), 3, 1024);
        for (a, b) in cascades.iter().zip(&again).take(3) {
            assert_eq!(a.radius, b.radius);
            let dx = (b.center[0] - a.center[0]) / a.texel;
            assert!((dx - dx.round()).abs() < 1e-2, "{dx}");
        }
    }

    #[test]
    fn casters_are_chosen_per_cascade_and_reach_towards_the_light() {
        let unit = Bounds { center: [0.0; 3], radius: 0.87 };
        let mut draws = vec![
            DrawItem::new(MeshId(1), at(0.0, 0.0, 0.0), [1.0; 4]),       // visible, casts
            DrawItem::new(MeshId(2), at(0.0, 40.0, 0.0), [1.0; 4]),      // high above, out of view: still casts
            DrawItem::new(MeshId(1), at(300.0, 0.0, 300.0), [1.0; 4]),   // far off to the side: no
            DrawItem::new(MeshId(1), at(1.0, 0.0, 1.0), [1.0, 1.0, 1.0, 0.5]), // glass: never casts
        ];
        draws[3].material = Material::Glass;
        let mut environment = Environment { sun_direction: [0.0, 1.0, 0.0], ..Default::default() };
        environment.shadow.enabled = true;
        environment.shadow.cascades = 2;
        let frame = FrameDesc {
            view_proj: camera([0.0, 6.0, 12.0], [0.0, 0.0, 0.0], true),
            reverse_z: true,
            draws: &draws,
            environment,
            ..Default::default()
        };
        let p = plan(&frame, |_| Some(unit));
        let mut s = ShadowPlan::default();
        plan_shadows(&frame, &p, |_| Some(unit), &mut s);
        assert_eq!(s.data.count, 2);
        let in_cascade = |k: u32| -> Vec<usize> {
            s.batches.iter().filter(|b| b.cascade == k).flat_map(|b| s.entries[b.first as usize..(b.first + b.count) as usize].to_vec()).collect()
        };
        let all: Vec<usize> = (0..2).flat_map(in_cascade).collect();
        assert!(all.contains(&0) && all.contains(&1), "{all:?}");
        assert!(!all.contains(&2) && !all.contains(&3), "{all:?}");
        // The cascade holding the high caster reaches up to it.
        let k = (0..2).find(|&k| in_cascade(k).contains(&1)).unwrap() as usize;
        let l = LightBasis::new([0.0, 1.0, 0.0]).project([0.0, 40.0, 0.0]);
        assert!(s.cascades[k].z_near <= l[2] - 0.87 + 1e-3, "{:?}", s.cascades[k]);
        // Reusing the plan for a frame with shadows off empties it.
        let mut off = frame;
        off.environment.shadow.enabled = false;
        plan_shadows(&off, &p, |_| Some(unit), &mut s);
        assert_eq!((s.data.count, s.entries.len(), s.batches.len()), (0, 0, 0));
    }

    #[test]
    fn quantize_steps_by_quarter_octaves() {
        assert_eq!(quantize(1.0, false), 1.0);
        assert_eq!(quantize(2.0, true), 2.0);
        let q = quantize(1.1, false);
        assert!((q - 1.0).abs() < 1e-6, "{q}");
        let q = quantize(1.1, true);
        assert!((q - 2f32.powf(0.25)).abs() < 1e-5, "{q}");
    }
}
