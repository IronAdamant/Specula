//! Frustum culling, draw ordering and instancing batches: pure functions over a [`FrameDesc`].
//!
//! The renderer turns a frame's draw list into a [`Plan`]: which (draw, pass) entries are drawn, in
//! which order, and how consecutive entries merge into instanced draw calls. Per-draw records are
//! written to the GPU in `entries` order, so a batch is a contiguous range of records.

use crate::render::camera_position;
use crate::{FrameDesc, Material, MeshId, Vertex};

/// A bounding sphere in a mesh's object space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub center: [f32; 3],
    pub radius: f32,
}

/// Sphere around the vertices' axis-aligned box centre that contains every vertex.
pub fn mesh_bounds(vertices: &[Vertex]) -> Bounds {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for v in vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(v.position[k]);
            hi[k] = hi[k].max(v.position[k]);
        }
    }
    if vertices.is_empty() || !lo.iter().chain(&hi).all(|x| x.is_finite()) {
        // Never cull what we cannot bound.
        return Bounds { center: [0.0; 3], radius: f32::INFINITY };
    }
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let r2 = vertices
        .iter()
        .map(|v| {
            let d = [v.position[0] - center[0], v.position[1] - center[1], v.position[2] - center[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
        })
        .fold(0.0f32, f32::max);
    Bounds { center, radius: r2.sqrt() }
}

/// `bounds` placed by `model`: world centre and a radius covering any scale or shear.
pub fn world_sphere(model: &[f32; 16], bounds: Bounds) -> ([f32; 3], f32) {
    let c = bounds.center;
    let center = [
        model[0] * c[0] + model[4] * c[1] + model[8] * c[2] + model[12],
        model[1] * c[0] + model[5] * c[1] + model[9] * c[2] + model[13],
        model[2] * c[0] + model[6] * c[1] + model[10] * c[2] + model[14],
    ];
    // The 3×3 part stretches a vector by at most σ_max, and σ_max² (the largest eigenvalue of MᵀM)
    // is at most any row sum of |MᵀM| (Gershgorin) and at most the squared Frobenius norm. With
    // orthogonal columns (rotation × scale) the first bound is exact: the largest column length.
    let col = |i: usize| [model[i * 4], model[i * 4 + 1], model[i * 4 + 2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (a, b, cc) = (col(0), col(1), col(2));
    let (ab, bc, ac) = (dot(a, b).abs(), dot(b, cc).abs(), dot(a, cc).abs());
    let gershgorin = (dot(a, a) + ab + ac).max(dot(b, b) + ab + bc).max(dot(cc, cc) + ac + bc);
    let frobenius = dot(a, a) + dot(b, b) + dot(cc, cc);
    (center, bounds.radius * gershgorin.min(frobenius).sqrt())
}

/// The planes of a view frustum, from a `view_proj` in Vulkan clip conventions (−w ≤ x, y ≤ w,
/// 0 ≤ z ≤ w). Works for standard and reversed Z, finite or infinite far planes: degenerate
/// planes (an infinite far plane) are dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct Frustum {
    /// `(n, d)` with |n| = 1: a point p is inside when n·p + d ≥ 0.
    planes: Vec<[f32; 4]>,
}

impl Frustum {
    pub fn from_view_proj(m: &[f32; 16]) -> Frustum {
        let row = |r: usize| [m[r] as f64, m[4 + r] as f64, m[8 + r] as f64, m[12 + r] as f64];
        let (x, y, z, w) = (row(0), row(1), row(2), row(3));
        let add = |a: [f64; 4], b: [f64; 4], s: f64| [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2], a[3] + s * b[3]];
        let candidates = [add(w, x, 1.0), add(w, x, -1.0), add(w, y, 1.0), add(w, y, -1.0), z, add(w, z, -1.0)];
        let planes = candidates
            .iter()
            .filter_map(|p| {
                let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                // A zero normal is a plane at infinity (or a degenerate matrix): it bounds nothing.
                (len > 1e-12 && p.iter().all(|v| v.is_finite()))
                    .then(|| [(p[0] / len) as f32, (p[1] / len) as f32, (p[2] / len) as f32, (p[3] / len) as f32])
            })
            .collect();
        Frustum { planes }
    }

    /// False only when the sphere is certainly outside one plane. NaN inputs count as visible.
    pub fn sphere_visible(&self, center: [f32; 3], radius: f32) -> bool {
        // A small margin keeps rounding from ever culling something that touches the view.
        let margin = radius * 1e-3 + 1e-4;
        !self.planes.iter().any(|p| {
            let dist = p[0] * center[0] + p[1] * center[1] + p[2] * center[2] + p[3];
            dist < -(radius + margin)
        })
    }
}

/// Consecutive entries drawn with one instanced call: records `first .. first + count`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Batch {
    pub mesh: MeshId,
    pub material: Material,
    /// Glass: 0 = back faces, 1 = front faces; 0 for everything else.
    pub pass: u32,
    pub first: u32,
    pub count: u32,
}

/// What to draw this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// (draw index, pass) in record order.
    pub entries: Vec<(usize, u32)>,
    pub batches: Vec<Batch>,
    /// Draws skipped by frustum culling.
    pub culled: u32,
}

/// Sort key for opaque draws: material (and so pipeline), mesh, then near to far (early depth
/// rejection hides what is behind instead of shading it twice), then list position. `dist2` is
/// non-negative or NaN, so its bits order like its value (NaN last).
fn opaque_key(material: Material, mesh: MeshId, dist2: f32, index: usize) -> (u32, u32, u32, usize) {
    (material as u32, mesh.0, dist2.to_bits(), index)
}

/// Cull, order and batch `frame`'s draws. `bounds` gives a mesh's object-space sphere (`None` =
/// never cull it).
///
/// Order: opaque draws first, sorted by (material, mesh) so equal ones batch, and near to far
/// within a batch (by bounding-sphere centre) so hidden surfaces fail the depth test early. Depth
/// testing makes their order irrelevant except for exactly coplanar overlaps. Then blended draws
/// far to near by model origin; glass appears twice, back faces (pass 0) then front faces (pass 1).
/// Blended draws sharing an origin are nested shells listed outermost first: all their back faces
/// in list order, then their front faces in reverse, so an inner liquid sits inside its glass.
/// Blended entries keep this order exactly and batch only when adjacent.
pub fn plan(frame: &FrameDesc, bounds: impl Fn(MeshId) -> Option<Bounds>) -> Plan {
    let frustum = Frustum::from_view_proj(&frame.view_proj);
    let c = camera_position(&frame.view_proj);
    let mut culled = 0u32;
    let mut blended = Vec::new();
    // Visible opaque draws as sort keys.
    let mut opaque = Vec::with_capacity(frame.draws.len());
    for (i, d) in frame.draws.iter().enumerate() {
        let (center, keep) = match bounds(d.mesh) {
            Some(b) => {
                let (center, r) = world_sphere(&d.model, b);
                (center, frustum.sphere_visible(center, r))
            }
            None => ([d.model[12], d.model[13], d.model[14]], true),
        };
        if !keep {
            culled += 1;
        } else if d.material.blended() {
            blended.push(i);
        } else {
            let v = [center[0] - c[0], center[1] - c[1], center[2] - c[2]];
            // `abs` only clears the sign of a NaN, so every NaN sorts last.
            let dist2 = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).abs();
            opaque.push(opaque_key(d.material, d.mesh, dist2, i));
        }
    }

    opaque.sort_unstable(); // keys are unique (they end in the index): deterministic
    let mut entries: Vec<(usize, u32)> = opaque.into_iter().map(|k| (k.3, 0)).collect();

    let dist2 = |i: usize| {
        let m = &frame.draws[i].model;
        let d = [m[12] - c[0], m[13] - c[1], m[14] - c[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    blended.sort_by(|&a, &b| dist2(b).total_cmp(&dist2(a))); // stable: ties keep list order
    for group in blended.chunk_by(|&a, &b| dist2(a) == dist2(b)) {
        entries.extend(group.iter().map(|&i| (i, 0)));
        let glass = group.iter().rev().filter(|&&i| frame.draws[i].material == Material::Glass);
        entries.extend(glass.map(|&i| (i, 1)));
    }

    let mut batches: Vec<Batch> = Vec::new();
    for (n, &(i, pass)) in entries.iter().enumerate() {
        let d = &frame.draws[i];
        match batches.last_mut() {
            Some(b) if b.mesh == d.mesh && b.material == d.material && b.pass == pass => b.count += 1,
            _ => batches.push(Batch { mesh: d.mesh, material: d.material, pass, first: n as u32, count: 1 }),
        }
    }
    Plan { entries, batches, culled }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DrawItem;

    const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

    fn at(x: f32, y: f32, z: f32) -> [f32; 16] {
        let mut m = IDENTITY;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    }

    fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
        let mut o = [0.0; 16];
        for c in 0..4 {
            for r in 0..4 {
                o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
            }
        }
        o
    }

    /// Camera at (0, 0, z) looking down −Z, 90° vertical FOV, square aspect.
    fn camera(z: f32, reverse_z: bool) -> [f32; 16] {
        let (near, far) = (0.05f32, 100.0f32);
        let proj = if reverse_z {
            [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0]
        } else {
            [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, far / (near - far), -1.0, 0.0, 0.0, near * far / (near - far), 0.0]
        };
        mul(&proj, &at(0.0, 0.0, -z))
    }

    const UNIT: Bounds = Bounds { center: [0.0; 3], radius: 0.87 };

    #[test]
    fn mesh_bounds_cover_every_vertex() {
        let v = |x: f32, y: f32, z: f32| Vertex { position: [x, y, z], normal: [0.0; 3] };
        let b = mesh_bounds(&[v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 4.0, 0.0), v(2.0, 4.0, 0.0)]);
        assert_eq!(b.center, [1.0, 2.0, 0.0]);
        assert!((b.radius - 5f32.sqrt()).abs() < 1e-6);
        assert_eq!(mesh_bounds(&[]).radius, f32::INFINITY);
        assert_eq!(mesh_bounds(&[v(f32::NAN, 0.0, 0.0)]).radius, f32::INFINITY);
    }

    #[test]
    fn world_sphere_follows_scale_and_shear() {
        let mut m = at(1.0, 2.0, 3.0);
        m[0] = 2.0;
        m[5] = 3.0;
        let (c, r) = world_sphere(&m, Bounds { center: [1.0, 1.0, 0.0], radius: 1.0 });
        assert_eq!(c, [3.0, 5.0, 3.0]);
        assert_eq!(r, 3.0, "largest axis scale");
        // A shear (x += y) stretches some vectors by the golden ratio, more than any column's
        // length (√2): the bound must cover that.
        let mut s = IDENTITY;
        s[4] = 1.0;
        let (_, r) = world_sphere(&s, Bounds { center: [0.0; 3], radius: 1.0 });
        let golden = (1.0 + 5f32.sqrt()) / 2.0;
        assert!(r >= golden && (r - 3f32.sqrt()).abs() < 1e-6, "{r}");
    }

    #[test]
    fn frustum_culls_in_both_depth_modes() {
        for reverse_z in [false, true] {
            let f = Frustum::from_view_proj(&camera(0.0, reverse_z));
            let vis = |x: f32, y: f32, z: f32| f.sphere_visible([x, y, z], 0.5);
            assert!(vis(0.0, 0.0, -5.0), "ahead");
            assert!(!vis(0.0, 0.0, 5.0), "behind (reverse_z={reverse_z})");
            assert!(!vis(20.0, 0.0, -5.0), "far right");
            assert!(!vis(0.0, -20.0, -5.0), "far below");
            assert!(vis(5.3, 0.0, -5.0), "straddles the right plane");
            assert!(!vis(5.8, 0.0, -5.0), "just outside the right plane");
            assert!(vis(0.0, 0.0, -0.3), "straddles the near plane");
            assert_eq!(vis(0.0, 0.0, -1000.0), reverse_z, "beyond the far plane only with a finite far");
        }
        // Infinite far plane: dropped, so 5 planes remain.
        assert_eq!(Frustum::from_view_proj(&camera(0.0, true)).planes.len(), 5);
        // A zero matrix bounds nothing.
        assert!(Frustum::from_view_proj(&[0.0; 16]).sphere_visible([1e9, 0.0, 0.0], 0.0));
    }

    #[test]
    fn nan_spheres_are_never_culled() {
        let f = Frustum::from_view_proj(&camera(0.0, true));
        assert!(f.sphere_visible([f32::NAN, 0.0, 0.0], 1.0));
        assert!(f.sphere_visible([0.0, 0.0, 5.0], f32::NAN));
    }

    fn draw(mesh: u32, model: [f32; 16], material: Material) -> DrawItem {
        let mut d = DrawItem::new(MeshId(mesh), model, [1.0; 4]);
        d.material = material;
        d
    }

    #[test]
    fn opaque_draws_sort_by_mesh_and_batch() {
        let draws = [
            draw(2, at(0.0, 0.0, -3.0), Material::Lit),
            draw(1, at(1.0, 0.0, -3.0), Material::Lit),
            draw(2, at(2.0, 0.0, -3.0), Material::Lit),
            draw(1, at(3.0, 0.0, -3.0), Material::Lit),
            draw(1, at(0.0, 0.0, 9.0), Material::Lit), // behind the camera
        ];
        let frame = FrameDesc { draws: &draws, view_proj: camera(0.0, true), reverse_z: true, ..Default::default() };
        let p = plan(&frame, |_| Some(UNIT));
        assert_eq!(p.culled, 1);
        assert_eq!(p.entries, vec![(1, 0), (3, 0), (0, 0), (2, 0)]);
        assert_eq!(
            p.batches,
            vec![
                Batch { mesh: MeshId(1), material: Material::Lit, pass: 0, first: 0, count: 2 },
                Batch { mesh: MeshId(2), material: Material::Lit, pass: 0, first: 2, count: 2 },
            ]
        );
        // Without bounds nothing is culled.
        assert_eq!(plan(&frame, |_| None).culled, 0);
    }

    #[test]
    fn opaque_instances_draw_near_to_far() {
        // Listed far to near (the worst case for early depth rejection); a NaN model sorts last.
        let mut nan = at(0.0, 0.0, -1.0);
        nan[12] = f32::NAN;
        let draws = [
            draw(1, at(0.0, 0.0, -9.0), Material::Lit),
            draw(1, nan, Material::Lit),
            draw(1, at(0.0, 0.0, -6.0), Material::Lit),
            draw(1, at(0.0, 0.0, -3.0), Material::Lit),
            draw(1, at(0.0, 0.0, -3.0), Material::Lit), // a tie keeps list order
        ];
        let frame = FrameDesc { draws: &draws, view_proj: camera(0.0, true), ..Default::default() };
        let p = plan(&frame, |_| Some(UNIT));
        assert_eq!(p.entries, vec![(3, 0), (4, 0), (2, 0), (0, 0), (1, 0)]);
        assert_eq!(p.batches.len(), 1);
    }

    #[test]
    fn blended_draws_follow_opaque_far_to_near() {
        let draws = [
            draw(1, at(0.0, 0.0, -2.0), Material::Glass),
            draw(1, at(0.0, 0.0, -1.0), Material::Lit),
            draw(1, at(0.0, 0.0, -5.0), Material::LavaLamp),
        ];
        let frame = FrameDesc { draws: &draws, ..Default::default() };
        assert_eq!(plan(&frame, |_| Some(UNIT)).entries, vec![(1, 0), (2, 0), (0, 0), (0, 1)]);
    }

    #[test]
    fn nested_glass_shells_draw_inner_inside_outer() {
        let outer = draw(1, IDENTITY, Material::Glass);
        let draws = [outer, outer];
        let frame = FrameDesc { draws: &draws, view_proj: camera(5.0, true), ..Default::default() };
        let p = plan(&frame, |_| Some(UNIT));
        assert_eq!(p.entries, vec![(0, 0), (1, 0), (1, 1), (0, 1)]);
        // Adjacent equal entries batch; instances keep their order on the GPU.
        assert_eq!(p.batches.iter().map(|b| (b.pass, b.first, b.count)).collect::<Vec<_>>(), vec![(0, 0, 2), (1, 2, 2)]);
    }

    #[test]
    fn blended_draws_batch_only_when_adjacent() {
        // Far to near: shadow A (z −9), glass (z −6), shadow B (z −3): A and B must not merge.
        let draws = [
            draw(7, at(0.0, 0.0, -3.0), Material::BlobShadow),
            draw(8, at(0.0, 0.0, -6.0), Material::Glass),
            draw(7, at(0.0, 0.0, -9.0), Material::BlobShadow),
            draw(7, at(0.0, 0.0, -9.5), Material::BlobShadow),
        ];
        let frame = FrameDesc { draws: &draws, view_proj: camera(0.0, true), ..Default::default() };
        let p = plan(&frame, |_| Some(UNIT));
        assert_eq!(p.entries, vec![(3, 0), (2, 0), (1, 0), (1, 1), (0, 0)]);
        let shape: Vec<_> = p.batches.iter().map(|b| (b.mesh.0, b.pass, b.count)).collect();
        assert_eq!(shape, vec![(7, 0, 2), (8, 0, 1), (8, 1, 1), (7, 0, 1)]);
    }

    #[test]
    fn ten_thousand_equal_cubes_are_one_batch() {
        let draws: Vec<DrawItem> =
            (0..10_000).map(|i| draw(3, at((i % 100) as f32 * 0.1 - 5.0, (i / 100) as f32 * 0.1 - 5.0, -20.0), Material::Lit)).collect();
        let frame = FrameDesc { draws: &draws, view_proj: camera(0.0, true), ..Default::default() };
        let p = plan(&frame, |_| Some(Bounds { center: [0.0; 3], radius: 0.05 }));
        assert_eq!((p.culled, p.entries.len(), p.batches.len()), (0, 10_000, 1));
        assert_eq!(p.batches[0].count, 10_000);
    }
}
