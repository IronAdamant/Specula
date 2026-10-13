//! Automatic colliders fitted to a mesh: box, capsule or convex hull (see docs/design/PHYSICS.md).

use std::collections::HashMap;

use crate::math3::{self, V3};
use crate::mesh::Bounds;

/// Which collider a recipe asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColliderKind {
    None,
    Aabb,
    Capsule,
    ConvexHull,
}

impl ColliderKind {
    pub fn parse(s: &str) -> Option<ColliderKind> {
        Some(match s {
            "none" => ColliderKind::None,
            "aabb" | "box" => ColliderKind::Aabb,
            "capsule" => ColliderKind::Capsule,
            "convex_hull" | "hull" => ColliderKind::ConvexHull,
            _ => return None,
        })
    }
}

/// A fitted collider, in the model's space.
#[derive(Clone, Debug, PartialEq)]
pub enum Collider {
    None,
    Aabb { min: V3, max: V3 },
    /// Segment `a`–`b` swept by `radius`; contains every input point.
    Capsule { a: V3, b: V3, radius: f32 },
    /// Convex polyhedron: counter-clockwise outward triangles over `points`.
    ConvexHull { points: Vec<V3>, indices: Vec<u32> },
}

/// Most vertices a hull keeps (physics cost grows with it).
pub const MAX_HULL_POINTS: usize = 48;

/// Directions sampled when reducing a point cloud to its extreme points before hulling.
pub const HULL_DIRECTIONS: usize = 192;
/// Most input points [`convex_hull`] accepts (callers reduce first); bounds its O(n·faces) cost.
pub const MAX_HULL_INPUT: usize = 512;

/// Fits `kind` to `points`. The note says why a hull fell back to a box, if it did.
pub fn fit(kind: ColliderKind, points: &[V3]) -> (Collider, Option<String>) {
    let aabb = || {
        let b = Bounds::of(points);
        Collider::Aabb { min: b.min, max: b.max }
    };
    match kind {
        ColliderKind::None => (Collider::None, None),
        ColliderKind::Aabb => (aabb(), None),
        ColliderKind::Capsule => (capsule(points), None),
        ColliderKind::ConvexHull => {
            // Hull the extreme points only: bounded work whatever the mesh size.
            let extreme = extreme_points(points, HULL_DIRECTIONS);
            match hull_limited(&extreme, MAX_HULL_POINTS) {
                Some((p, i)) => (Collider::ConvexHull { points: p, indices: i }, None),
                None => (aabb(), Some("convex hull failed (flat or degenerate points); fell back to an AABB".to_string())),
            }
        }
    }
}

/// The points that are furthest along `directions` evenly spread directions (a Fibonacci
/// sphere) plus the six axes, deduplicated, in first-found order. At most `directions + 6`
/// points, so hulling them is cheap; they lie on the true hull, so the result is a slightly
/// conservative inner hull.
pub fn extreme_points(points: &[V3], directions: usize) -> Vec<V3> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut dirs: Vec<V3> = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
    let golden = crate::dmath::PI * (3.0 - 5.0f64.sqrt());
    for i in 0..directions {
        let z = 1.0 - 2.0 * (i as f64 + 0.5) / directions as f64;
        let r = (1.0 - z * z).max(0.0).sqrt();
        let (s, c) = crate::dmath::sin_cos_f64(golden * i as f64);
        dirs.push([(r * c) as f32, z as f32, (r * s) as f32]);
    }
    let mut out: Vec<V3> = Vec::new();
    for d in dirs {
        let mut best = 0;
        let mut best_v = f32::MIN;
        for (i, p) in points.iter().enumerate() {
            let v = math3::dot(*p, d);
            if v > best_v {
                best_v = v;
                best = i;
            }
        }
        let p = crate::shape::canon(points[best]);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Capsule along the longest bounds axis through the bounds centre, just containing every point.
pub fn capsule(points: &[V3]) -> Collider {
    let b = Bounds::of(points);
    let size = b.size();
    let axis = if size[1] >= size[0] && size[1] >= size[2] { 1 } else if size[0] >= size[2] { 0 } else { 2 };
    let c = b.center();
    let mut dir = [0.0; 3];
    dir[axis] = 1.0;
    let radial = |p: V3| {
        let d = math3::sub(p, c);
        let t = math3::dot(d, dir);
        (t, math3::length(math3::sub(d, math3::scale(dir, t))))
    };
    let radius = points.iter().map(|&p| radial(p).1).fold(0.0f32, f32::max).max(1e-4);
    // Half-length h such that every point is within radius of the segment [-h, h].
    let mut h = 0.0f32;
    for &p in points {
        let (t, r) = radial(p);
        let cap = (radius * radius - r * r).max(0.0).sqrt();
        h = h.max(t.abs() - cap);
    }
    Collider::Capsule { a: math3::sub(c, math3::scale(dir, h)), b: math3::add(c, math3::scale(dir, h)), radius }
}

/// Convex hull of `points` (incremental, `f64` predicates). `None` if the points are flat.
pub fn convex_hull(points: &[V3]) -> Option<(Vec<V3>, Vec<u32>)> {
    // Unique points in input order.
    let mut seen: HashMap<[u32; 3], ()> = HashMap::new();
    let pts: Vec<[f64; 3]> = points
        .iter()
        .map(|p| crate::shape::canon(*p))
        .filter(|p| seen.insert([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()], ()).is_none())
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect();
    if pts.len() < 4 || pts.len() > MAX_HULL_INPUT {
        return None;
    }
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let len = |a: [f64; 3]| dot(a, a).sqrt();
    let mut ext = 0.0f64;
    for p in &pts {
        ext = ext.max(p[0].abs()).max(p[1].abs()).max(p[2].abs());
    }
    let eps = 1e-9 * ext.max(1.0) * 1e3;
    // Initial tetrahedron: extreme x, farthest from that line, farthest from that plane.
    let i0 = (0..pts.len()).min_by(|&a, &b| pts[a][0].total_cmp(&pts[b][0])).unwrap();
    let i1 = (0..pts.len()).max_by(|&a, &b| len(sub(pts[a], pts[i0])).total_cmp(&len(sub(pts[b], pts[i0])))).unwrap();
    let line = sub(pts[i1], pts[i0]);
    let i2 = (0..pts.len()).max_by(|&a, &b| len(cross(line, sub(pts[a], pts[i0]))).total_cmp(&len(cross(line, sub(pts[b], pts[i0]))))).unwrap();
    let nrm = cross(line, sub(pts[i2], pts[i0]));
    if len(nrm) <= eps * eps {
        return None;
    }
    let i3 = (0..pts.len()).max_by(|&a, &b| dot(nrm, sub(pts[a], pts[i0])).abs().total_cmp(&dot(nrm, sub(pts[b], pts[i0])).abs())).unwrap();
    if dot(nrm, sub(pts[i3], pts[i0])).abs() <= eps * len(nrm) {
        return None;
    }
    let mut faces: Vec<[usize; 3]> = if dot(nrm, sub(pts[i3], pts[i0])) < 0.0 {
        vec![[i0, i1, i2], [i0, i3, i1], [i1, i3, i2], [i2, i3, i0]]
    } else {
        vec![[i0, i2, i1], [i0, i1, i3], [i1, i2, i3], [i2, i0, i3]]
    };
    let plane = |f: &[usize; 3]| {
        let n = cross(sub(pts[f[1]], pts[f[0]]), sub(pts[f[2]], pts[f[0]]));
        let l = len(n);
        let n = [n[0] / l, n[1] / l, n[2] / l];
        (n, dot(n, pts[f[0]]))
    };
    for (pi, &p) in pts.iter().enumerate() {
        if [i0, i1, i2, i3].contains(&pi) {
            continue;
        }
        let visible: Vec<bool> = faces
            .iter()
            .map(|f| {
                let (n, d) = plane(f);
                dot(n, p) - d > eps
            })
            .collect();
        if !visible.iter().any(|&v| v) {
            continue;
        }
        // Horizon: directed edges of visible faces whose reverse belongs to a hidden face.
        let mut vis_edges: HashMap<(usize, usize), ()> = HashMap::new();
        for (f, _) in faces.iter().zip(&visible).filter(|(_, v)| **v) {
            for k in 0..3 {
                vis_edges.insert((f[k], f[(k + 1) % 3]), ());
            }
        }
        let mut next = Vec::with_capacity(faces.len() + 4);
        let mut horizon = Vec::new();
        for (f, &v) in faces.iter().zip(&visible) {
            if v {
                for k in 0..3 {
                    let (a, b) = (f[k], f[(k + 1) % 3]);
                    if !vis_edges.contains_key(&(b, a)) {
                        horizon.push((a, b));
                    }
                }
            } else {
                next.push(*f);
            }
        }
        for (a, b) in horizon {
            next.push([a, b, pi]);
        }
        faces = next;
        if faces.len() > 8 * MAX_HULL_INPUT {
            return None; // numerically confused: give up rather than grow without bound
        }
    }
    // Compact the used points.
    let mut map: HashMap<usize, u32> = HashMap::new();
    let mut out_pts = Vec::new();
    let mut idx = Vec::with_capacity(faces.len() * 3);
    for f in &faces {
        for &v in f {
            let id = *map.entry(v).or_insert_with(|| {
                out_pts.push([pts[v][0] as f32, pts[v][1] as f32, pts[v][2] as f32]);
                out_pts.len() as u32 - 1
            });
            idx.push(id);
        }
    }
    Some((out_pts, idx))
}

/// Hull with at most `max_points` vertices: if the full hull has more, its vertices are thinned
/// on a coarsening grid (keeping one real hull vertex per cell) and the hull recomputed.
pub fn hull_limited(points: &[V3], max_points: usize) -> Option<(Vec<V3>, Vec<u32>)> {
    let (mut p, mut i) = convex_hull(points)?;
    let b = Bounds::of(points);
    let ext = b.size()[0].max(b.size()[1]).max(b.size()[2]);
    let mut cell = ext / 16.0;
    while p.len() > max_points && cell < ext {
        let mut cells: HashMap<[i32; 3], ()> = HashMap::new();
        let thin: Vec<V3> = p
            .iter()
            .filter(|q| {
                let d = math3::sub(**q, b.min);
                let key = [(d[0] / cell) as i32, (d[1] / cell) as i32, (d[2] / cell) as i32];
                cells.insert(key, ()).is_none()
            })
            .copied()
            .collect();
        if let Some((np, ni)) = convex_hull(&thin) {
            if np.len() <= max_points {
                return Some((np, ni));
            }
            (p, i) = (np, ni);
        }
        cell *= 1.4;
    }
    Some((p, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Shape;

    fn inside_hull(p: V3, pts: &[V3], idx: &[u32], tol: f32) -> bool {
        idx.chunks_exact(3).all(|t| {
            let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
            let n = math3::normalize(math3::tri_normal(a, b, c));
            math3::dot(n, math3::sub(p, a)) <= tol
        })
    }

    #[test]
    fn hull_of_box_and_sphere() {
        let s = Shape::cuboid([1.0, 2.0, 1.0], 0.0).unwrap();
        let (p, i) = convex_hull(&s.positions).unwrap();
        assert_eq!(p.len(), 8);
        assert_eq!(i.len() / 3, 12);
        let sp = Shape::sphere(1.0, 2);
        let (p, i) = convex_hull(&sp.positions).unwrap();
        assert_eq!(p.len(), 162);
        for q in &sp.positions {
            assert!(inside_hull(*q, &p, &i, 1e-4));
        }
        // Closed and outward.
        let m = crate::mesh::MeshData {
            normals: vec![[0.0, 1.0, 0.0]; p.len()],
            colors: vec![[0; 4]; p.len()],
            positions: p.clone(),
            indices: i.clone(),
            parts: vec![crate::mesh::Part { material: "h".into(), first_index: 0, index_count: i.len() as u32 }],
            ..Default::default()
        };
        assert!(m.is_watertight());
        assert!(m.signed_volume() > 0.0);
    }

    #[test]
    fn limited_hull_and_flat_input() {
        let sp = Shape::sphere(1.0, 3);
        let (p, i) = hull_limited(&extreme_points(&sp.positions, HULL_DIRECTIONS), 40).unwrap();
        assert!(p.len() <= 40 && p.len() >= 8, "{}", p.len());
        // Still roughly contains the sphere.
        for q in &sp.positions {
            assert!(inside_hull(*q, &p, &i, 0.35));
        }
        let flat = Shape::plane([1.0, 1.0], [3, 3]);
        assert!(convex_hull(&flat.positions).is_none());
        let (c, note) = fit(ColliderKind::ConvexHull, &flat.positions);
        assert!(matches!(c, Collider::Aabb { .. }));
        assert!(note.unwrap().contains("AABB"));
        // Too many raw points: refused (callers reduce first), never a huge allocation.
        assert!(convex_hull(&Shape::sphere(1.0, 3).positions).is_none());
    }

    #[test]
    fn huge_meshes_hull_in_bounded_work() {
        // Review regression: sphere 5 + subdivide 1 (81,920 triangles) made a 13 GB allocation.
        let mut s = Shape::sphere(1.0, 5);
        s.subdivide(1);
        let t = std::time::Instant::now();
        let (c, note) = fit(ColliderKind::ConvexHull, &s.positions);
        assert!(note.is_none());
        let Collider::ConvexHull { points, indices } = c else { panic!("expected a hull") };
        assert!(points.len() <= MAX_HULL_POINTS);
        for q in s.positions.iter().step_by(97) {
            assert!(inside_hull(*q, &points, &indices, 0.3));
        }
        assert!(t.elapsed().as_secs_f32() < 5.0, "took {:?}", t.elapsed());
        // Coplanar points on a subdivided box don't bloat the hull: just the 8 corners.
        let mut b = Shape::cuboid([1.0, 1.0, 1.0], 0.0).unwrap();
        b.subdivide(4);
        let (c, _) = fit(ColliderKind::ConvexHull, &b.positions);
        let Collider::ConvexHull { points, .. } = c else { panic!() };
        assert_eq!(points.len(), 8);
    }

    #[test]
    fn capsule_contains_points() {
        let s = Shape::capsule(0.3, 1.8, 12, 4);
        let Collider::Capsule { a, b, radius } = capsule(&s.positions) else { panic!() };
        assert!((radius - 0.3).abs() < 0.01);
        assert!((a[1] - 0.3).abs() < 0.02 && (b[1] - 1.5).abs() < 0.02, "{a:?} {b:?}");
        let box_ = Shape::cuboid([0.4, 2.0, 0.4], 0.0).unwrap();
        let Collider::Capsule { a, b, radius } = capsule(&box_.positions) else { panic!() };
        for p in &box_.positions {
            let ab = math3::sub(b, a);
            let t = (math3::dot(math3::sub(*p, a), ab) / math3::dot(ab, ab)).clamp(0.0, 1.0);
            let d = math3::length(math3::sub(*p, math3::add(a, math3::scale(ab, t))));
            assert!(d <= radius + 1e-5);
        }
    }
}
