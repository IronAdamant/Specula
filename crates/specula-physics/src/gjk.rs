//! Convex geometry for the narrowphase and the queries: support mappings, polytope views of
//! hulls and triangles, and GJK distance (Gilbert–Johnson–Keerthi, with Ericson's closest-point
//! tests for the simplex).
//!
//! Spheres and capsules enter as their *cores* (a point, a segment) plus a radius: GJK works on
//! the cores and the radius is subtracted afterwards, which is exact and robust.

use crate::math::{len_sq, Mat3, Pose, Vec3};
use crate::shape::Hull;

/// A convex set described by its support mapping.
pub trait Support {
    /// The point of the set farthest along `d` (world space).
    fn support(&self, d: Vec3) -> Vec3;
    /// Some point inside the set.
    fn center(&self) -> Vec3;
}

/// A point or a segment (the core of a sphere or capsule), in world space.
#[derive(Clone, Copy, Debug)]
pub enum Core {
    Point(Vec3),
    Segment(Vec3, Vec3),
}

impl Support for Core {
    fn support(&self, d: Vec3) -> Vec3 {
        match *self {
            Core::Point(p) => p,
            Core::Segment(a, b) => {
                if b.dot(d) > a.dot(d) {
                    b
                } else {
                    a
                }
            }
        }
    }

    fn center(&self) -> Vec3 {
        match *self {
            Core::Point(p) => p,
            Core::Segment(a, b) => (a + b) * 0.5,
        }
    }
}

impl Core {
    pub fn endpoints(&self) -> (Vec3, Vec3) {
        match *self {
            Core::Point(p) => (p, p),
            Core::Segment(a, b) => (a, b),
        }
    }

    pub fn translated(&self, d: Vec3) -> Core {
        match *self {
            Core::Point(p) => Core::Point(p + d),
            Core::Segment(a, b) => Core::Segment(a + d, b + d),
        }
    }
}

/// A convex polytope in world space: faces with outward planes and counter-clockwise loops,
/// and edges with the normals of the faces either side.
pub trait Polytope: Support {
    fn face_count(&self) -> usize;
    /// Outward unit normal and offset (`n · x = d` on the face).
    fn plane(&self, f: usize) -> (Vec3, f32);
    /// The face's vertices, counter-clockwise seen from outside.
    fn face_points(&self, f: usize, out: &mut Polygon);
    fn edge_count(&self) -> usize;
    /// Endpoints and the normals of the two faces that meet there.
    fn edge(&self, e: usize) -> (Vec3, Vec3, Vec3, Vec3);
    fn vertex_count(&self) -> usize;
    fn vertex(&self, i: usize) -> Vec3;
    /// The face whose normal is most aligned with `d`.
    fn best_face(&self, d: Vec3) -> usize {
        let mut best = 0;
        let mut best_dot = f32::MIN;
        for f in 0..self.face_count() {
            let x = self.plane(f).0.dot(d);
            if x > best_dot {
                best = f;
                best_dot = x;
            }
        }
        best
    }
}

/// Most vertices a polygon being clipped may have.
pub const MAX_POLY: usize = 72;

/// A small fixed-capacity polygon (no allocation in the narrowphase). Each point carries a
/// feature key for contact ids.
#[derive(Clone, Copy)]
pub struct Polygon {
    pub pts: [Vec3; MAX_POLY],
    pub keys: [u32; MAX_POLY],
    pub len: usize,
}

impl Default for Polygon {
    fn default() -> Polygon {
        Polygon { pts: [Vec3::ZERO; MAX_POLY], keys: [0; MAX_POLY], len: 0 }
    }
}

impl Polygon {
    pub fn push(&mut self, p: Vec3, key: u32) {
        if self.len < MAX_POLY {
            self.pts[self.len] = p;
            self.keys[self.len] = key;
            self.len += 1;
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

/// A hull placed in the world.
#[derive(Clone, Copy)]
pub struct PosedHull<'a> {
    pub hull: &'a Hull,
    pub pose: Pose,
    pub rot: Mat3,
}

impl<'a> PosedHull<'a> {
    pub fn new(hull: &'a Hull, pose: Pose) -> PosedHull<'a> {
        PosedHull { hull, pose, rot: Mat3::from_quat(pose.q) }
    }

    #[inline]
    fn world(&self, p: Vec3) -> Vec3 {
        self.rot.mul_vec(p) + self.pose.p
    }
}

impl Support for PosedHull<'_> {
    fn support(&self, d: Vec3) -> Vec3 {
        let local = self.rot.mul_vec_t(d);
        self.world(self.hull.points[self.hull.support_index(local)])
    }

    fn center(&self) -> Vec3 {
        self.world(self.hull.centroid)
    }
}

impl Polytope for PosedHull<'_> {
    fn face_count(&self) -> usize {
        self.hull.face_count()
    }

    fn plane(&self, f: usize) -> (Vec3, f32) {
        let n = self.rot.mul_vec(self.hull.normals[f]);
        (n, self.hull.offsets[f] + n.dot(self.pose.p))
    }

    fn face_points(&self, f: usize, out: &mut Polygon) {
        out.clear();
        for &v in self.hull.face(f) {
            out.push(self.world(self.hull.points[v as usize]), u32::from(v));
        }
    }

    fn edge_count(&self) -> usize {
        self.hull.edges.len()
    }

    fn edge(&self, e: usize) -> (Vec3, Vec3, Vec3, Vec3) {
        let ed = self.hull.edges[e];
        (
            self.world(self.hull.points[ed.v[0] as usize]),
            self.world(self.hull.points[ed.v[1] as usize]),
            self.rot.mul_vec(self.hull.normals[ed.faces[0] as usize]),
            self.rot.mul_vec(self.hull.normals[ed.faces[1] as usize]),
        )
    }

    fn vertex_count(&self) -> usize {
        self.hull.points.len()
    }

    fn vertex(&self, i: usize) -> Vec3 {
        self.world(self.hull.points[i])
    }
}

/// Most vertices and faces of a hull copied to world space once per test ([`WorldHull`]);
/// bigger hulls transform on the fly ([`PosedHull`]).
pub const WORLD_HULL_MAX: usize = 32;

/// A small hull (a box, a simple hull) transformed to world space once, so the separating-axis
/// test and clipping read plain world vertices and planes.
pub struct WorldHull<'a> {
    hull: &'a Hull,
    pts: [Vec3; WORLD_HULL_MAX],
    planes: [(Vec3, f32); WORLD_HULL_MAX],
    center: Vec3,
}

impl<'a> WorldHull<'a> {
    /// `None` when the hull is too big to copy.
    pub fn new(p: &PosedHull<'a>) -> Option<WorldHull<'a>> {
        let h = p.hull;
        if h.points.len() > WORLD_HULL_MAX || h.face_count() > WORLD_HULL_MAX {
            return None;
        }
        let mut pts = [Vec3::ZERO; WORLD_HULL_MAX];
        for (o, q) in pts.iter_mut().zip(&h.points) {
            *o = p.world(*q);
        }
        let mut planes = [(Vec3::ZERO, 0.0f32); WORLD_HULL_MAX];
        for (f, o) in planes.iter_mut().enumerate().take(h.face_count()) {
            let n = p.rot.mul_vec(h.normals[f]);
            *o = (n, h.offsets[f] + n.dot(p.pose.p));
        }
        Some(WorldHull { hull: h, pts, planes, center: p.world(h.centroid) })
    }
}

impl Support for WorldHull<'_> {
    fn support(&self, d: Vec3) -> Vec3 {
        let n = self.hull.points.len();
        let mut best = self.pts[0];
        let mut best_dot = best.dot(d);
        for p in &self.pts[1..n] {
            let x = p.dot(d);
            if x > best_dot {
                best = *p;
                best_dot = x;
            }
        }
        best
    }

    fn center(&self) -> Vec3 {
        self.center
    }
}

impl Polytope for WorldHull<'_> {
    fn face_count(&self) -> usize {
        self.hull.face_count()
    }

    fn plane(&self, f: usize) -> (Vec3, f32) {
        self.planes[f]
    }

    fn face_points(&self, f: usize, out: &mut Polygon) {
        out.clear();
        for &v in self.hull.face(f) {
            out.push(self.pts[v as usize], u32::from(v));
        }
    }

    fn edge_count(&self) -> usize {
        self.hull.edges.len()
    }

    fn edge(&self, e: usize) -> (Vec3, Vec3, Vec3, Vec3) {
        let ed = self.hull.edges[e];
        (self.pts[ed.v[0] as usize], self.pts[ed.v[1] as usize], self.planes[ed.faces[0] as usize].0, self.planes[ed.faces[1] as usize].0)
    }

    fn vertex_count(&self) -> usize {
        self.hull.points.len()
    }

    fn vertex(&self, i: usize) -> Vec3 {
        self.pts[i]
    }
}

/// A triangle in world space, as a two-sided polytope (faces `n` and `-n`).
#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub v: [Vec3; 3],
    /// Unit normal (counter-clockwise winding).
    pub n: Vec3,
}

impl Triangle {
    pub fn new(v: [Vec3; 3]) -> Triangle {
        let n = crate::math::normalize_or((v[1] - v[0]).cross(v[2] - v[0]), Vec3::Y);
        Triangle { v, n }
    }
}

impl Support for Triangle {
    fn support(&self, d: Vec3) -> Vec3 {
        let (a, b, c) = (self.v[0].dot(d), self.v[1].dot(d), self.v[2].dot(d));
        if a >= b && a >= c {
            self.v[0]
        } else if b >= c {
            self.v[1]
        } else {
            self.v[2]
        }
    }

    fn center(&self) -> Vec3 {
        (self.v[0] + self.v[1] + self.v[2]) * (1.0 / 3.0)
    }
}

impl Polytope for Triangle {
    fn face_count(&self) -> usize {
        2
    }

    fn plane(&self, f: usize) -> (Vec3, f32) {
        let n = if f == 0 { self.n } else { -self.n };
        (n, n.dot(self.v[0]))
    }

    fn face_points(&self, f: usize, out: &mut Polygon) {
        out.clear();
        if f == 0 {
            (0..3).for_each(|i| out.push(self.v[i], i as u32));
        } else {
            [0, 2, 1].into_iter().for_each(|i| out.push(self.v[i], i as u32));
        }
    }

    fn edge_count(&self) -> usize {
        3
    }

    fn edge(&self, e: usize) -> (Vec3, Vec3, Vec3, Vec3) {
        (self.v[e], self.v[(e + 1) % 3], self.n, -self.n)
    }

    fn vertex_count(&self) -> usize {
        3
    }

    fn vertex(&self, i: usize) -> Vec3 {
        self.v[i]
    }
}

/// Closest points between two convex sets.
#[derive(Clone, Copy, Debug)]
pub struct Closest {
    /// 0 when they overlap.
    pub distance: f32,
    pub a: Vec3,
    pub b: Vec3,
    pub overlap: bool,
}

#[derive(Clone, Copy, Default)]
struct Vert {
    w: Vec3,
    a: Vec3,
    b: Vec3,
}

const GJK_ITERATIONS: usize = 40;

/// Distance and closest points between `a` and `b` (GJK).
pub fn gjk<A: Support + ?Sized, B: Support + ?Sized>(a: &A, b: &B) -> Closest {
    let mut s = [Vert::default(); 4];
    let mut n = 0usize;
    let mut bary = [0.0f32; 4];
    let mut v = a.center() - b.center();
    if len_sq(v) < 1.0e-12 {
        v = Vec3::X;
    }
    // Seed with one support point.
    {
        let sa = a.support(-v);
        let sb = b.support(v);
        s[0] = Vert { w: sa - sb, a: sa, b: sb };
        n = n.max(1);
        bary[0] = 1.0;
        v = s[0].w;
    }
    for _ in 0..GJK_ITERATIONS {
        let vv = len_sq(v);
        if vv < 1.0e-14 {
            return overlap(&s, n, &bary);
        }
        let d = -v;
        let sa = a.support(d);
        let sb = b.support(-d);
        let w = sa - sb;
        // No more progress towards the origin: v is the closest point.
        if vv - v.dot(w) <= 1.0e-6 * vv + 1.0e-14 {
            break;
        }
        if s[..n].iter().any(|x| len_sq(x.w - w) < 1.0e-14) {
            break;
        }
        s[n] = Vert { w, a: sa, b: sb };
        n += 1;
        let (nv, inside) = solve(&mut s, &mut n, &mut bary);
        if inside {
            return overlap(&s, n, &bary);
        }
        if len_sq(nv) >= vv {
            // Numerical stall: keep the previous answer.
            break;
        }
        v = nv;
    }
    let (mut pa, mut pb) = (Vec3::ZERO, Vec3::ZERO);
    for i in 0..n {
        pa = pa + s[i].a * bary[i];
        pb = pb + s[i].b * bary[i];
    }
    let dist = (pa - pb).length();
    Closest { distance: dist, a: pa, b: pb, overlap: dist <= 1.0e-7 }
}

fn overlap(s: &[Vert; 4], n: usize, bary: &[f32; 4]) -> Closest {
    let (mut pa, mut pb) = (Vec3::ZERO, Vec3::ZERO);
    for i in 0..n {
        pa = pa + s[i].a * bary[i];
        pb = pb + s[i].b * bary[i];
    }
    Closest { distance: 0.0, a: pa, b: pb, overlap: true }
}

/// Replaces the simplex by the smallest sub-simplex holding its point closest to the origin;
/// returns that point and whether the origin is inside the (full) tetrahedron.
fn solve(s: &mut [Vert; 4], n: &mut usize, bary: &mut [f32; 4]) -> (Vec3, bool) {
    match *n {
        1 => {
            bary[0] = 1.0;
            (s[0].w, false)
        }
        2 => {
            let (a, b) = (s[0].w, s[1].w);
            let ab = b - a;
            let t = (-a).dot(ab) / len_sq(ab).max(1.0e-30);
            if t <= 0.0 {
                *n = 1;
                bary[0] = 1.0;
                (a, false)
            } else if t >= 1.0 {
                s[0] = s[1];
                *n = 1;
                bary[0] = 1.0;
                (b, false)
            } else {
                bary[0] = 1.0 - t;
                bary[1] = t;
                (a + ab * t, false)
            }
        }
        3 => {
            let (p, w) = closest_on_triangle(Vec3::ZERO, s[0].w, s[1].w, s[2].w);
            keep(s, n, bary, &[w[0], w[1], w[2], 0.0]);
            (p, false)
        }
        _ => {
            let (a, b, c, d) = (s[0].w, s[1].w, s[2].w, s[3].w);
            let faces: [([usize; 3], usize); 4] = [([0, 1, 2], 3), ([0, 2, 3], 1), ([0, 3, 1], 2), ([1, 3, 2], 0)];
            let pts = [a, b, c, d];
            let mut best: Option<(f32, Vec3, [f32; 4])> = None;
            let mut outside_any = false;
            for (f, opp) in faces {
                let (fa, fb, fc) = (pts[f[0]], pts[f[1]], pts[f[2]]);
                let nrm = (fb - fa).cross(fc - fa);
                let sp = (-fa).dot(nrm);
                let sd = (pts[opp] - fa).dot(nrm);
                // Outside this face (or the tetrahedron is flat).
                if sp * sd < 0.0 || sd.abs() < 1.0e-20 {
                    outside_any = true;
                    let (p, w) = closest_on_triangle(Vec3::ZERO, fa, fb, fc);
                    let d2 = len_sq(p);
                    if best.is_none_or(|(bd, _, _)| d2 < bd) {
                        let mut full = [0.0; 4];
                        full[f[0]] = w[0];
                        full[f[1]] = w[1];
                        full[f[2]] = w[2];
                        best = Some((d2, p, full));
                    }
                }
            }
            if !outside_any {
                // Inside: barycentrics of the origin are not needed for an overlap answer;
                // keep the full simplex with equal weights so witness points are sensible.
                *bary = [0.25; 4];
                return (Vec3::ZERO, true);
            }
            let (_, p, w) = best.expect("some face");
            keep(s, n, bary, &w);
            (p, false)
        }
    }
}

/// Keeps the vertices with a positive weight, in order.
fn keep(s: &mut [Vert; 4], n: &mut usize, bary: &mut [f32; 4], w: &[f32; 4]) {
    let mut k = 0;
    for i in 0..*n {
        if w[i] > 0.0 {
            s[k] = s[i];
            bary[k] = w[i];
            k += 1;
        }
    }
    if k == 0 {
        k = 1;
        bary[0] = 1.0;
    }
    *n = k;
}

/// Closest point on triangle `abc` to `p` and its barycentric weights (Ericson 5.1.5).
pub fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> (Vec3, [f32; 3]) {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + ab * v, [1.0 - v, v, 0.0]);
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + ac * w, [1.0 - w, 0.0, w]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w, [0.0, 1.0 - w, w]);
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    (a + ab * v + ac * w, [1.0 - v - w, v, w])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;
    use crate::shape::Shape;
    use specula_core::math::Quat;

    #[test]
    fn points_segments_and_boxes() {
        let Shape::Box { hull, .. } = Shape::cuboid(v3(1.0, 1.0, 1.0)) else { panic!() };
        let bx = PosedHull::new(&hull, Pose::IDENTITY);
        let c = gjk(&Core::Point(v3(3.0, 0.5, 0.0)), &bx);
        assert!((c.distance - 2.0).abs() < 1e-5, "{c:?}");
        assert!((c.b - v3(1.0, 0.5, 0.0)).length() < 1e-5);
        let c = gjk(&Core::Point(v3(3.0, 3.0, 0.0)), &bx);
        assert!((c.distance - 8f32.sqrt()).abs() < 1e-5);
        let c = gjk(&Core::Point(v3(0.2, 0.1, 0.0)), &bx);
        assert!(c.overlap);
        let seg = Core::Segment(v3(-5.0, 2.0, 0.0), v3(5.0, 2.0, 0.0));
        let c = gjk(&seg, &bx);
        assert!((c.distance - 1.0).abs() < 1e-5, "{c:?}");
        // A rotated box: its corner points at the probe.
        let rot = PosedHull::new(&hull, Pose::new(Vec3::ZERO, Quat::from_euler_xyz_deg(0.0, 0.0, 45.0)));
        let c = gjk(&Core::Point(v3(0.0, 3.0, 0.0)), &rot);
        assert!((c.distance - (3.0 - 2f32.sqrt())).abs() < 1e-4, "{c:?}");
    }

    #[test]
    fn box_box_distance() {
        let Shape::Box { hull, .. } = Shape::cuboid(v3(0.5, 0.5, 0.5)) else { panic!() };
        let a = PosedHull::new(&hull, Pose::IDENTITY);
        let b = PosedHull::new(&hull, Pose::at(v3(2.0, 0.3, 0.1)));
        let c = gjk(&a, &b);
        assert!((c.distance - 1.0).abs() < 1e-5, "{c:?}");
        let b = PosedHull::new(&hull, Pose::at(v3(0.9, 0.3, 0.1)));
        assert!(gjk(&a, &b).overlap);
    }

    #[test]
    fn triangles() {
        let t = Triangle::new([v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)]);
        let c = gjk(&Core::Point(v3(0.2, 1.0, 0.2)), &t);
        assert!((c.distance - 1.0).abs() < 1e-5);
        let (p, w) = closest_on_triangle(v3(2.0, 0.0, 2.0), t.v[0], t.v[1], t.v[2]);
        assert!((p - v3(0.5, 0.0, 0.5)).length() < 1e-6 && w[0].abs() < 1e-6);
    }
}
