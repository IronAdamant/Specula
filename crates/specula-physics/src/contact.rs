//! The narrowphase: contact points between two placed shapes.
//!
//! - **Sphere and capsule pairs** are closed-form (closest points between points and
//!   segments); parallel capsules get two points.
//! - **Sphere or capsule against a box or hull** runs GJK on the core (point or segment); when
//!   the core is inside the polytope a separating-axis test (faces, and segment × edges) finds
//!   the shallowest way out. A capsule lying on a face gets two points.
//! - **Box and hull pairs** use the separating-axis test (faces of both, plus edge pairs pruned
//!   by the Gauss-map test), then clip the incident face against the reference face: up to four
//!   points (deepest, farthest, then the largest area).
//! - **Against triangle meshes and heightfields** each nearby triangle is a two-sided polytope;
//!   contacts on a seam between triangles (an internal edge) take the triangle's normal instead,
//!   so bodies slide over seams without snagging. All points are reduced to four.
//!
//! Every point carries a feature id, stable while the same features touch, so the solver can
//! warm-start from last tick's impulses. Normals point from shape A to shape B; a separation
//! below zero is penetration, above zero (up to the speculative margin) a speculative contact.

use crate::gjk::{gjk, Core, Polygon, Polytope, PosedHull, Support, Triangle, WorldHull};
use crate::math::{closest_on_segment, closest_segments, len_sq, normalize_or, try_normalize, Aabb, Pose, Vec3};
use crate::shape::{Shape, TriMesh};

/// Most points in a manifold.
pub const MAX_POINTS: usize = 4;
/// Most triangles of a mesh tested against one body per tick, nearest to the body's own
/// vertices first (bounded work; more is reported in [`Manifold::skipped`]).
pub const MAX_MESH_TRIANGLES_PER_PAIR: usize = 256;
/// Most candidate triangles gathered for one pair before ordering them.
pub const MAX_MESH_CANDIDATES: usize = 16_384;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactPoint {
    /// World point, midway between the two surfaces.
    pub point: Vec3,
    /// Unit normal from A to B.
    pub normal: Vec3,
    /// Negative: penetration depth.
    pub separation: f32,
    /// Feature id (stable while the same features touch).
    pub id: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Manifold {
    pub points: [ContactPoint; MAX_POINTS],
    pub len: usize,
    /// Mesh triangles near the pair that were not tested (the per-pair cap); 0 normally.
    pub skipped: u32,
}

impl Manifold {
    pub fn as_slice(&self) -> &[ContactPoint] {
        &self.points[..self.len]
    }

    fn push(&mut self, p: ContactPoint) {
        if self.len < MAX_POINTS {
            self.points[self.len] = p;
            self.len += 1;
        }
    }

    fn flip(&mut self) {
        for p in &mut self.points[..self.len] {
            p.normal = -p.normal;
        }
    }
}

/// What a polytope pair remembers between ticks (temporal coherence for the separating-axis
/// test): the separating direction while apart, or the contact feature and the relative pose
/// it was found at. Apart pairs re-check their direction first (one support scan each); touching
/// pairs whose relative pose moved less than 5 mm and about 1 degree since the last full test
/// reuse the feature and only re-clip. Part of the pair's state; deterministic like the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SatCache {
    /// 0 none, 1 a face of A, 2 a face of B, 3 an edge pair, 4 apart.
    kind: u8,
    i: u16,
    j: u16,
    /// Apart: the separating direction, in A's frame.
    axis: Vec3,
    /// B's pose in A's frame at the last full test: position and quaternion.
    rel_p: Vec3,
    rel_q: [f32; 4],
}

const SAT_NONE: u8 = 0;
const SAT_FACE_A: u8 = 1;
const SAT_FACE_B: u8 = 2;
const SAT_EDGE: u8 = 3;
const SAT_APART: u8 = 4;

impl SatCache {
    fn set(&mut self, kind: u8, i: usize, j: usize, rel: &Pose) {
        self.kind = kind;
        self.i = i as u16;
        self.j = j as u16;
        self.rel_p = rel.p;
        self.rel_q = [rel.q.x, rel.q.y, rel.q.z, rel.q.w];
    }

    /// Feeds the memory into a state hash (it changes results, so it is state).
    pub fn hash_into(&self, h: &mut specula_core::hash::StableHasher) {
        h.write_u8(self.kind);
        if self.kind != SAT_NONE {
            h.write_u32(u32::from(self.i) << 16 | u32::from(self.j));
            for v in [self.axis, self.rel_p] {
                [v.x, v.y, v.z].iter().for_each(|c| h.write_f32(*c));
            }
            self.rel_q.iter().for_each(|c| h.write_f32(*c));
        }
    }

    /// Writes the memory for a save (see `Space::save_state`).
    pub fn save(&self, w: &mut specula_core::bytes::ByteWriter) {
        w.u8(self.kind);
        w.u16(self.i);
        w.u16(self.j);
        w.vec3(self.axis);
        w.vec3(self.rel_p);
        self.rel_q.iter().for_each(|c| w.f32(*c));
    }

    /// Reads [`SatCache::save`]'s bytes.
    pub fn load(r: &mut specula_core::bytes::ByteReader<'_>) -> Result<SatCache, specula_core::bytes::DecodeError> {
        let at = r.offset();
        let kind = r.u8("sat.kind")?;
        if kind > SAT_APART {
            return Err(specula_core::bytes::DecodeError::new("invalid", at, "sat.kind", format!("unknown pair memory kind {kind}")));
        }
        let (i, j) = (r.u16("sat.i")?, r.u16("sat.j")?);
        let (axis, rel_p) = (r.vec3("sat.axis")?, r.vec3("sat.rel_p")?);
        let rel_q = [r.f32("sat.rel_q")?, r.f32("sat.rel_q")?, r.f32("sat.rel_q")?, r.f32("sat.rel_q")?];
        Ok(SatCache { kind, i, j, axis, rel_p, rel_q })
    }

    fn near(&self, rel: &Pose) -> bool {
        let q = self.rel_q;
        let dot = q[0] * rel.q.x + q[1] * rel.q.y + q[2] * rel.q.z + q[3] * rel.q.w;
        len_sq(rel.p - self.rel_p) < 0.005 * 0.005 && dot.abs() > 0.99996
    }
}

/// The poses and memory a polytope pair's test may use.
struct Frames<'c> {
    pa: Pose,
    pb: Pose,
    cache: &'c mut SatCache,
}

/// Candidate points before reduction to [`MAX_POINTS`].
struct Candidates {
    pts: [ContactPoint; 64],
    len: usize,
}

impl Candidates {
    fn new() -> Candidates {
        Candidates { pts: [ContactPoint::default(); 64], len: 0 }
    }

    fn push(&mut self, p: ContactPoint) {
        if self.len == self.pts.len() {
            // Full: keep the best four so far and go on (nothing is dropped unseen).
            let mut m = Manifold::default();
            self.reduce(&mut m);
            self.len = 0;
            m.as_slice().iter().for_each(|q| {
                self.pts[self.len] = *q;
                self.len += 1;
            });
        }
        self.pts[self.len] = p;
        self.len += 1;
    }

    /// Keeps up to four: the deepest, the farthest from it, then the two that span the most
    /// area.
    fn reduce(&self, out: &mut Manifold) {
        out.len = 0;
        let pts = &self.pts[..self.len];
        if pts.len() <= MAX_POINTS {
            pts.iter().for_each(|p| out.push(*p));
            return;
        }
        let deepest = (0..pts.len()).fold(0, |b, i| if pts[i].separation < pts[b].separation { i } else { b });
        let p0 = pts[deepest].point;
        let far = (0..pts.len()).fold(deepest, |b, i| if len_sq(pts[i].point - p0) > len_sq(pts[b].point - p0) { i } else { b });
        let p1 = pts[far].point;
        let n = pts[deepest].normal;
        let area = |i: usize| (p1 - p0).cross(pts[i].point - p0).dot(n);
        let third = (0..pts.len()).fold(deepest, |b, i| if area(i).abs() > area(b).abs() { i } else { b });
        let sign = if area(third) >= 0.0 { -1.0 } else { 1.0 };
        let fourth = (0..pts.len()).fold(usize::MAX, |b, i| {
            let a = sign * area(i);
            if a > 1.0e-9 && (b == usize::MAX || a > sign * area(b)) {
                i
            } else {
                b
            }
        });
        for i in [deepest, far, third, fourth] {
            if i != usize::MAX && !out.as_slice().iter().any(|q| q.id == pts[i].id && q.point == pts[i].point) {
                out.push(pts[i]);
            }
        }
    }
}

/// Clipping buffers, reused per thread: zeroing them on every call cost more than the test.
struct Clip {
    poly: Polygon,
    buf: Polygon,
    scratch: Polygon,
    planes: [(Vec3, f32); crate::gjk::MAX_POLY],
    cands: Candidates,
    /// B's edges for the edge test: endpoints, negated face normals and their cross product.
    edges: [(Vec3, Vec3, Vec3, Vec3, Vec3); MAX_EDGES],
}

thread_local! {
    static CLIP: std::cell::RefCell<Clip> = std::cell::RefCell::new(Clip {
        poly: Polygon::default(),
        buf: Polygon::default(),
        scratch: Polygon::default(),
        planes: [(Vec3::ZERO, 0.0); crate::gjk::MAX_POLY],
        cands: Candidates::new(),
        edges: [(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO); MAX_EDGES],
    });
}

/// A shape placed for the narrowphase.
pub enum Convex<'a> {
    Core(Core, f32),
    Poly(PosedHull<'a>),
}

/// The convex view of a shape at `pose` (`None` for meshes and heightfields).
pub fn convex<'a>(shape: &'a Shape, pose: &Pose) -> Option<Convex<'a>> {
    match shape {
        Shape::Sphere { radius } => Some(Convex::Core(Core::Point(pose.p), *radius)),
        Shape::Capsule { radius, half_height } => Some(Convex::Core(
            Core::Segment(pose.point(Vec3::new(0.0, -half_height, 0.0)), pose.point(Vec3::new(0.0, *half_height, 0.0))),
            *radius,
        )),
        Shape::Box { hull, .. } | Shape::Hull(hull) => Some(Convex::Poly(PosedHull::new(hull, *pose))),
        Shape::Mesh(_) | Shape::Heightfield(_) => None,
    }
}

/// Contact points between `a` at `pa` and `b` at `pb` closer than `margin` (speculative).
pub fn collide(a: &Shape, pa: &Pose, b: &Shape, pb: &Pose, margin: f32, out: &mut Manifold) {
    collide_cached(a, pa, b, pb, margin, out, None);
}

/// [`collide`] with the pair's [`SatCache`] (hull pairs reuse last tick's feature).
pub fn collide_cached(a: &Shape, pa: &Pose, b: &Shape, pb: &Pose, margin: f32, out: &mut Manifold, cache: Option<&mut SatCache>) {
    out.len = 0;
    out.skipped = 0;
    match (a.triangles(), b.triangles()) {
        (Some(_), Some(_)) => {}
        (Some(mesh), None) => {
            if let Some(cb) = convex(b, pb) {
                mesh_vs_convex(mesh, pa, &cb, b.local_aabb(), pb, margin, out);
                out.flip();
            }
        }
        (None, Some(mesh)) => {
            if let Some(ca) = convex(a, pa) {
                mesh_vs_convex(mesh, pb, &ca, a.local_aabb(), pa, margin, out);
            }
        }
        (None, None) => {
            if let (Shape::Box { half: ha, hull: hulla }, Shape::Box { half: hb, hull: hullb }) = (a, b) {
                let (wa, wb) = (PosedHull::new(hulla, *pa), PosedHull::new(hullb, *pb));
                if let (Some(wa), Some(wb)) = (WorldHull::new(&wa), WorldHull::new(&wb)) {
                    box_vs_box(*ha, pa, &wa, *hb, pb, &wb, margin, out);
                    return;
                }
            }
            let (Some(ca), Some(cb)) = (convex(a, pa), convex(b, pb)) else {
                return;
            };
            match (&ca, &cb, cache) {
                (Convex::Poly(x), Convex::Poly(y), Some(cache)) => {
                    let frames = Some(Frames { pa: *pa, pb: *pb, cache });
                    match (WorldHull::new(x), WorldHull::new(y)) {
                        (Some(wa), Some(wb)) => poly_pair(&wa, &wb, margin, out, frames),
                        (Some(wa), None) => poly_pair(&wa, y, margin, out, frames),
                        (None, Some(wb)) => poly_pair(x, &wb, margin, out, frames),
                        (None, None) => poly_pair(x, y, margin, out, frames),
                    }
                }
                _ => convex_vs_convex(&ca, &cb, margin, out),
            }
        }
    }
}

/// Contacts between two convex shapes (normals from `a` to `b`).
pub fn convex_vs_convex(a: &Convex, b: &Convex, margin: f32, out: &mut Manifold) {
    out.len = 0;
    match (a, b) {
        (Convex::Core(ca, ra), Convex::Core(cb, rb)) => core_vs_core(ca, *ra, cb, *rb, margin, out),
        (Convex::Core(ca, ra), Convex::Poly(pb)) => match WorldHull::new(pb) {
            Some(w) => core_vs_poly(ca, *ra, &w, margin, out),
            None => core_vs_poly(ca, *ra, pb, margin, out),
        },
        (Convex::Poly(pa), Convex::Core(cb, rb)) => {
            match WorldHull::new(pa) {
                Some(w) => core_vs_poly(cb, *rb, &w, margin, out),
                None => core_vs_poly(cb, *rb, pa, margin, out),
            }
            out.flip();
        }
        // Small hulls (boxes) are copied to world space once for the whole test.
        (Convex::Poly(pa), Convex::Poly(pb)) => match (WorldHull::new(pa), WorldHull::new(pb)) {
            (Some(wa), Some(wb)) => poly_vs_poly(&wa, &wb, margin, out),
            (Some(wa), None) => poly_vs_poly(&wa, pb, margin, out),
            (None, Some(wb)) => poly_vs_poly(pa, &wb, margin, out),
            (None, None) => poly_vs_poly(pa, pb, margin, out),
        },
    }
}

fn surface_point(on_a: Vec3, on_b: Vec3, n: Vec3, ra: f32, rb: f32) -> Vec3 {
    // Midway between A's surface (core + n·ra) and B's (core − n·rb).
    ((on_a + n * ra) + (on_b - n * rb)) * 0.5
}

fn core_vs_core(a: &Core, ra: f32, b: &Core, rb: f32, margin: f32, out: &mut Manifold) {
    let (a0, a1) = a.endpoints();
    let (b0, b1) = b.endpoints();
    let (ca, cb, _, _) = closest_segments(a0, a1, b0, b1);
    let d = cb - ca;
    let dist = d.length();
    let sep = dist - ra - rb;
    if sep > margin {
        return;
    }
    let n = if dist > 1.0e-6 { d * (1.0 / dist) } else { fallback_normal(a, b) };
    // Two capsules side by side: two points along their overlap.
    if let (Core::Segment(..), Core::Segment(..)) = (a, b) {
        let (da, db) = (a1 - a0, b1 - b0);
        let (la, lb) = (da.length(), db.length());
        if la > 1.0e-5 && lb > 1.0e-5 && da.cross(db).length() < 0.02 * la * lb {
            let u = da * (1.0 / la);
            let (s0, s1) = ((b0 - a0).dot(u), (b1 - a0).dot(u));
            let (lo, hi) = (s0.min(s1).max(0.0), s0.max(s1).min(la));
            if hi - lo > 1.0e-4 {
                for (k, s) in [lo, hi].into_iter().enumerate() {
                    let pa = a0 + u * s;
                    let (pb, _) = closest_on_segment(pa, b0, b1);
                    let dd = pb - pa;
                    let dl = dd.length();
                    let nn = if dl > 1.0e-6 { dd * (1.0 / dl) } else { n };
                    let s = dl - ra - rb;
                    if s <= margin {
                        out.push(ContactPoint { point: surface_point(pa, pb, nn, ra, rb), normal: nn, separation: s, id: k as u32 + 1 });
                    }
                }
                if out.len > 0 {
                    return;
                }
            }
        }
    }
    out.push(ContactPoint { point: surface_point(ca, cb, n, ra, rb), normal: n, separation: sep, id: 0 });
}

/// A normal for coincident cores: across the segments if there are any, else up.
fn fallback_normal(a: &Core, b: &Core) -> Vec3 {
    let dir = match (a, b) {
        (Core::Segment(p, q), _) | (_, Core::Segment(p, q)) => *q - *p,
        _ => Vec3::Y,
    };
    let (t1, _) = crate::math::basis(normalize_or(dir, Vec3::Y));
    if matches!((a, b), (Core::Point(_), Core::Point(_))) {
        Vec3::Y
    } else {
        t1
    }
}

/// Side planes of a polytope face (outward normals, offsets), for clipping.
fn side_planes<P: Polytope + ?Sized>(p: &P, f: usize, buf: &mut Polygon, planes: &mut [(Vec3, f32); crate::gjk::MAX_POLY]) -> usize {
    let (n, _) = p.plane(f);
    p.face_points(f, buf);
    let k = buf.len;
    for (i, plane) in planes.iter_mut().enumerate().take(k) {
        let (v0, v1) = (buf.pts[i], buf.pts[(i + 1) % k]);
        let s = normalize_or((v1 - v0).cross(n), n);
        *plane = (s, s.dot(v0));
    }
    k
}

/// Clips `poly` to the inside (`s · x <= d`) of each plane (Sutherland–Hodgman), keeping keys.
fn clip_polygon(poly: &mut Polygon, planes: &[(Vec3, f32)], scratch: &mut Polygon) {
    // Ping-pong between the two buffers by reference (no copying the arrays).
    let (mut src, mut dst) = (&mut *poly, &mut *scratch);
    let mut swapped = false;
    for (k, (s, d)) in planes.iter().enumerate() {
        if src.len == 0 {
            break;
        }
        dst.clear();
        for i in 0..src.len {
            let j = if i + 1 == src.len { 0 } else { i + 1 };
            let (a, b) = (src.pts[i], src.pts[j]);
            let (ka, kb) = (src.keys[i], src.keys[j]);
            let (da, db) = (s.dot(a) - d, s.dot(b) - d);
            if da <= 0.0 {
                dst.push(a, ka);
            }
            if (da <= 0.0) != (db <= 0.0) {
                let t = da / (da - db);
                let key = 0x8000 | ((k as u32 & 0x3f) << 9) | ((ka.min(kb) & 0x7) << 6) | (ka.max(kb) & 0x3f);
                dst.push(a + (b - a) * t, key);
            }
        }
        std::mem::swap(&mut src, &mut dst);
        swapped = !swapped;
    }
    if swapped {
        // The result is in `scratch`: copy just its points back.
        let n = scratch.len;
        poly.pts[..n].copy_from_slice(&scratch.pts[..n]);
        poly.keys[..n].copy_from_slice(&scratch.keys[..n]);
        poly.len = n;
    }
}

fn core_vs_poly<P: Polytope + ?Sized>(core: &Core, r: f32, poly: &P, margin: f32, out: &mut Manifold) {
    out.len = 0;
    let c = gjk(core, poly);
    if !c.overlap && c.distance > 1.0e-5 {
        let sep = c.distance - r;
        if sep > margin {
            return;
        }
        let n = (c.b - c.a) * (1.0 / c.distance);
        if let Core::Segment(e0, e1) = *core {
            if face_points_for_segment(e0, e1, r, poly, n, margin, out) {
                return;
            }
        }
        out.push(ContactPoint { point: ((c.a + n * r) + c.b) * 0.5, normal: n, separation: sep, id: 0 });
        return;
    }
    // The core is inside: find the shallowest way out.
    let (e0, e1) = core.endpoints();
    let mut best_sep = f32::MIN;
    let mut best_axis = Vec3::Y;
    let mut best_edge: Option<usize> = None;
    for f in 0..poly.face_count() {
        let (nf, df) = poly.plane(f);
        let s = nf.dot(e0).min(nf.dot(e1)) - df - r;
        if s > best_sep {
            best_sep = s;
            best_axis = nf;
            best_edge = None;
        }
    }
    let seg = e1 - e0;
    if len_sq(seg) > 1.0e-10 {
        let center = poly.center();
        for e in 0..poly.edge_count() {
            let (q0, q1, _, _) = poly.edge(e);
            let Some(mut axis) = try_normalize(seg.cross(q1 - q0)) else {
                continue;
            };
            if axis.dot(q0 - center) < 0.0 {
                axis = -axis;
            }
            let s = axis.dot(e0).min(axis.dot(e1)) - axis.dot(poly.support(axis)) - r;
            // Prefer faces unless an edge is clearly shallower.
            if s > best_sep + 1.0e-4 {
                best_sep = s;
                best_axis = axis;
                best_edge = Some(e);
            }
        }
    }
    if best_sep > margin {
        return;
    }
    let n = -best_axis;
    match best_edge {
        None => {
            if matches!(core, Core::Segment(..)) && face_points_for_segment(e0, e1, r, poly, n, margin, out) {
                return;
            }
            let (nf, df) = (best_axis, poly.plane(poly.best_face(best_axis)).1);
            let p = if nf.dot(e0) <= nf.dot(e1) { e0 } else { e1 };
            let s = nf.dot(p) - df - r;
            let on_face = p - nf * (nf.dot(p) - df);
            out.push(ContactPoint { point: ((p - nf * r) + on_face) * 0.5, normal: n, separation: s, id: 0 });
        }
        Some(e) => {
            let (q0, q1, _, _) = poly.edge(e);
            let (ca, cb, _, _) = closest_segments(e0, e1, q0, q1);
            out.push(ContactPoint { point: ((ca + n * r) + cb) * 0.5, normal: n, separation: best_sep, id: 0x100 | e as u32 });
        }
    }
}

/// A segment core lying along a polytope face: two points, clipped to the face. `n` is the
/// normal from the core to the polytope. Returns whether it applied.
fn face_points_for_segment<P: Polytope + ?Sized>(e0: Vec3, e1: Vec3, r: f32, poly: &P, n: Vec3, margin: f32, out: &mut Manifold) -> bool {
    let f = poly.best_face(-n);
    let (nf, df) = poly.plane(f);
    if nf.dot(-n) < 0.98 {
        return false;
    }
    let seg = e1 - e0;
    let len = seg.length();
    if len < 1.0e-5 || (seg * (1.0 / len)).dot(nf).abs() > 0.05 {
        return false;
    }
    let (mut a, mut b) = (e0, e1);
    let inside = CLIP.with(|cell| {
        let clip = &mut *cell.borrow_mut();
        let k = side_planes(poly, f, &mut clip.buf, &mut clip.planes);
        for (s, d) in &clip.planes[..k] {
            let (da, db) = (s.dot(a) - d, s.dot(b) - d);
            if da > 0.0 && db > 0.0 {
                return false;
            }
            if da > 0.0 {
                a = a + (b - a) * (da / (da - db));
            } else if db > 0.0 {
                b = b + (a - b) * (db / (db - da));
            }
        }
        true
    });
    if !inside {
        return false;
    }
    let before = out.len;
    for (i, p) in [a, b].into_iter().enumerate() {
        let s = nf.dot(p) - df - r;
        if s <= margin {
            let on_face = p - nf * (nf.dot(p) - df);
            out.push(ContactPoint {
                point: ((p - nf * r) + on_face) * 0.5,
                normal: -nf,
                separation: s,
                id: ((f as u32) << 4) | (i as u32 + 1),
            });
        }
    }
    out.len > before
}

/// Most edges of a hull's polytope read for the edge test (a 64-point hull has at most 186).
const MAX_EDGES: usize = 192;

/// Box against box: the separating-axis test on the 15 axes of two oriented boxes (3 + 3 face
/// axes, 9 edge cross products), then the same clipping as any polytope pair.
#[allow(clippy::too_many_arguments)]
fn box_vs_box(ha: Vec3, pa: &Pose, wa: &WorldHull, hb: Vec3, pb: &Pose, wb: &WorldHull, margin: f32, out: &mut Manifold) {
    out.len = 0;
    let ra = crate::math::Mat3::from_quat(pa.q);
    let rb = crate::math::Mat3::from_quat(pb.q);
    let (ax, bx) = (ra.c, rb.c);
    let (ha3, hb3) = ([ha.x, ha.y, ha.z], [hb.x, hb.y, hb.z]);
    let t = pb.p - pa.p;
    // |a_i · b_j|, with a little slack so near-parallel edges don't make bogus axes.
    let mut abs_r = [[0.0f32; 3]; 3];
    for (i, row) in abs_r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = ax[i].dot(bx[j]).abs() + 1.0e-6;
        }
    }
    // Face axes of A and of B (separation along the axis, pointing from A to B).
    let mut sa = (f32::MIN, Vec3::Y);
    for i in 0..3 {
        let d = t.dot(ax[i]);
        let rb_sum = hb3[0] * abs_r[i][0] + hb3[1] * abs_r[i][1] + hb3[2] * abs_r[i][2];
        let s = d.abs() - ha3[i] - rb_sum;
        if s > sa.0 {
            sa = (s, if d >= 0.0 { ax[i] } else { -ax[i] });
        }
    }
    if sa.0 > margin {
        return;
    }
    let mut sb = (f32::MIN, Vec3::Y);
    for j in 0..3 {
        let d = t.dot(bx[j]);
        let ra_sum = ha3[0] * abs_r[0][j] + ha3[1] * abs_r[1][j] + ha3[2] * abs_r[2][j];
        let s = d.abs() - hb3[j] - ra_sum;
        if s > sb.0 {
            // B's face normal points from B towards A.
            sb = (s, if d >= 0.0 { -bx[j] } else { bx[j] });
        }
    }
    if sb.0 > margin {
        return;
    }
    let mut se = (f32::MIN, Vec3::Y, 0usize, 0usize);
    for i in 0..3 {
        for j in 0..3 {
            let c = ax[i].cross(bx[j]);
            let l = c.length();
            if l < 1.0e-5 {
                continue;
            }
            let axis = c * (1.0 / l);
            let d = t.dot(axis);
            let r_a: f32 = (0..3).map(|k| ha3[k] * ax[k].dot(axis).abs()).sum();
            let r_b: f32 = (0..3).map(|k| hb3[k] * bx[k].dot(axis).abs()).sum();
            let s = d.abs() - r_a - r_b;
            if s > se.0 {
                se = (s, if d >= 0.0 { axis } else { -axis }, i, j);
            }
        }
    }
    if se.0 > margin {
        return;
    }
    const REL_EDGE: f32 = 0.90;
    const REL_FACE: f32 = 0.98;
    const ABS: f32 = 0.0025;
    let max_face = sa.0.max(sb.0);
    if se.0 > REL_EDGE * max_face + ABS {
        let (_, axis, i, j) = se;
        // A's edge along a_i nearest B, and B's edge along b_j nearest A.
        let mut ca = pa.p;
        let mut cb = pb.p;
        for k in 0..3 {
            if k != i {
                ca = ca + ax[k] * (ha3[k] * axis.dot(ax[k]).signum());
            }
            if k != j {
                cb = cb - bx[k] * (hb3[k] * axis.dot(bx[k]).signum());
            }
        }
        let (ea, eb) = (ax[i] * ha3[i], bx[j] * hb3[j]);
        let (c1, c2, _, _) = closest_segments(ca - ea, ca + ea, cb - eb, cb + eb);
        out.push(ContactPoint { point: (c1 + c2) * 0.5, normal: axis, separation: se.0, id: 0x4000_0000 | (i * 3 + j) as u32 });
        return;
    }
    CLIP.with(|cell| {
        let clip = &mut *cell.borrow_mut();
        clip.cands.len = 0;
        if sb.0 > REL_FACE * sa.0 + ABS {
            face_contact(wb, wb.best_face(sb.1), wa, margin, true, clip);
        } else {
            face_contact(wa, wa.best_face(sa.1), wb, margin, false, clip);
        }
        clip.cands.reduce(out);
    });
}

fn face_query<A: Polytope + ?Sized, B: Polytope + ?Sized>(a: &A, b: &B) -> (f32, usize) {
    let mut best = (f32::MIN, 0);
    for f in 0..a.face_count() {
        let (n, d) = a.plane(f);
        let s = n.dot(b.support(-n)) - d;
        if s > best.0 {
            best = (s, f);
        }
    }
    best
}

fn poly_vs_poly<A: Polytope + ?Sized, B: Polytope + ?Sized>(a: &A, b: &B, margin: f32, out: &mut Manifold) {
    poly_pair(a, b, margin, out, None);
}

/// Separation of `a` and `b` along `axis` (from A towards B).
fn separation_along<A: Polytope + ?Sized, B: Polytope + ?Sized>(a: &A, b: &B, axis: Vec3) -> f32 {
    axis.dot(b.support(-axis)) - axis.dot(a.support(axis))
}

/// The contact of an edge pair (`None` when the pair is degenerate or apart).
fn edge_contact<A: Polytope + ?Sized, B: Polytope + ?Sized>(a: &A, b: &B, i: usize, j: usize) -> Option<(Vec3, f32, Vec3)> {
    let (p0, p1, _, _) = a.edge(i);
    let (q0, q1, _, _) = b.edge(j);
    let mut axis = crate::math::try_normalize((p1 - p0).cross(q1 - q0))?;
    if axis.dot(p0 - a.center()) < 0.0 {
        axis = -axis;
    }
    let (c1, c2, _, _) = closest_segments(p0, p1, q0, q1);
    Some((axis, axis.dot(q0 - p0), (c1 + c2) * 0.5))
}

fn poly_pair<A: Polytope + ?Sized, B: Polytope + ?Sized>(a: &A, b: &B, margin: f32, out: &mut Manifold, mut frames: Option<Frames>) {
    out.len = 0;
    let rel = frames.as_ref().map(|f| f.pa.inv_mul(&f.pb));
    // What the pair found last tick.
    if let (Some(f), Some(rel)) = (frames.as_mut(), rel.as_ref()) {
        let c = *f.cache;
        match c.kind {
            SAT_APART => {
                let axis = f.pa.vector(c.axis);
                if separation_along(a, b, axis) > margin {
                    return;
                }
            }
            // A stale index (the shape changed) means a fresh test below, never "no contacts".
            SAT_FACE_A | SAT_FACE_B
                if c.near(rel) && (c.kind == SAT_FACE_A && (c.i as usize) < a.face_count() || c.kind == SAT_FACE_B && (c.j as usize) < b.face_count()) =>
            {
                CLIP.with(|cell| {
                    let clip = &mut *cell.borrow_mut();
                    clip.cands.len = 0;
                    if c.kind == SAT_FACE_A {
                        face_contact(a, c.i as usize, b, margin, false, clip);
                    } else {
                        face_contact(b, c.j as usize, a, margin, true, clip);
                    }
                    clip.cands.reduce(out);
                });
                return;
            }
            SAT_EDGE if c.near(rel) && (c.i as usize) < a.edge_count() && (c.j as usize) < b.edge_count() && edge_contact(a, b, c.i as usize, c.j as usize).is_some() => {
                if let Some((axis, sep, point)) = edge_contact(a, b, c.i as usize, c.j as usize) {
                    if sep <= margin {
                        out.push(ContactPoint { point, normal: axis, separation: sep, id: 0x4000_0000 | ((c.i as u32) << 12) | c.j as u32 });
                    }
                }
                return;
            }
            _ => {}
        }
    }
    // Remember a separating direction (in A's frame) when apart.
    let apart = |frames: &mut Option<Frames>, axis: Vec3| {
        if let (Some(f), Some(rel)) = (frames.as_mut(), rel.as_ref()) {
            f.cache.set(SAT_APART, 0, 0, rel);
            f.cache.axis = f.pa.inv_vector(axis);
        }
    };
    // Clearly apart: GJK's distance (linear in the vertices) settles it before the
    // separating-axis test (edges x edges).
    let was_touching = frames.as_ref().is_some_and(|f| matches!(f.cache.kind, SAT_FACE_A | SAT_FACE_B | SAT_EDGE));
    if !was_touching && a.vertex_count() + b.vertex_count() > 12 {
        let c = gjk(a, b);
        if !c.overlap && c.distance > margin {
            if let Some(n) = crate::math::try_normalize(c.b - c.a) {
                apart(&mut frames, n);
            }
            return;
        }
    }
    let (sa, fa) = face_query(a, b);
    if sa > margin {
        apart(&mut frames, a.plane(fa).0);
        return;
    }
    let (sb, fb) = face_query(b, a);
    if sb > margin {
        apart(&mut frames, -b.plane(fb).0);
        return;
    }
    // Edge pairs. B's edges (with the cross product of their negated face normals) are read
    // once, not once per edge of A.
    let center_a = a.center();
    let mut se = f32::MIN;
    let mut edges = (0, 0);
    let mut edge_axis = Vec3::Y;
    let nb = b.edge_count().min(MAX_EDGES);
    CLIP.with(|cell| {
        let b_edges = &mut cell.borrow_mut().edges;
        for (j, e) in b_edges.iter_mut().enumerate().take(nb) {
            let (q0, q1, nb0, nb1) = b.edge(j);
            let (c, d) = (-nb0, -nb1);
            *e = (q0, q1, c, d, d.cross(c));
        }
        for i in 0..a.edge_count() {
            let (p0, p1, na0, na1) = a.edge(i);
            let ea = p1 - p0;
            let bxa = na1.cross(na0);
            for (j, &(q0, q1, c, d, dxc)) in b_edges.iter().enumerate().take(nb) {
                // The Gauss-map test (see `is_minkowski_face`), with the cross products hoisted.
                let (cba, dba, adc, bdc) = (c.dot(bxa), d.dot(bxa), na0.dot(dxc), na1.dot(dxc));
                if !(cba * dba < 0.0 && adc * bdc < 0.0 && cba * bdc > 0.0) {
                    continue;
                }
                let eb = q1 - q0;
                let c = ea.cross(eb);
                let cl = c.length();
                if cl < 1.0e-5 * (ea.length() * eb.length()).max(1.0e-12) {
                    continue;
                }
                let mut axis = c * (1.0 / cl);
                if axis.dot(p0 - center_a) < 0.0 {
                    axis = -axis;
                }
                let s = axis.dot(q0 - p0);
                if s > se {
                    se = s;
                    edges = (i, j);
                    edge_axis = axis;
                }
            }
        }
    });
    if se > margin {
        apart(&mut frames, edge_axis);
        return;
    }
    const REL_EDGE: f32 = 0.90;
    const REL_FACE: f32 = 0.98;
    const ABS: f32 = 0.0025;
    let max_face = sa.max(sb);
    let remember = |frames: &mut Option<Frames>, kind: u8, i: usize, j: usize| {
        if let (Some(f), Some(rel)) = (frames.as_mut(), rel.as_ref()) {
            f.cache.set(kind, i, j, rel);
        }
    };
    if se > REL_EDGE * max_face + ABS {
        let (p0, p1, _, _) = a.edge(edges.0);
        let (q0, q1, _, _) = b.edge(edges.1);
        let (c1, c2, _, _) = closest_segments(p0, p1, q0, q1);
        out.push(ContactPoint {
            point: (c1 + c2) * 0.5,
            normal: edge_axis,
            separation: se,
            id: 0x4000_0000 | ((edges.0 as u32) << 12) | edges.1 as u32,
        });
        remember(&mut frames, SAT_EDGE, edges.0, edges.1);
        return;
    }
    let face_b = sb > REL_FACE * sa + ABS;
    CLIP.with(|cell| {
        let clip = &mut *cell.borrow_mut();
        clip.cands.len = 0;
        if face_b {
            face_contact(b, fb, a, margin, true, clip);
        } else {
            face_contact(a, fa, b, margin, false, clip);
        }
        clip.cands.reduce(out);
    });
    if face_b {
        remember(&mut frames, SAT_FACE_B, 0, fb);
    } else {
        remember(&mut frames, SAT_FACE_A, fa, 0);
    }
    let _ = SAT_NONE;
}

/// Clips the incident face of `inc` against reference face `fr` of `reff`. `flipped`: the
/// reference is shape B (normals then point the other way).
fn face_contact<R: Polytope + ?Sized, I: Polytope + ?Sized>(reff: &R, fr: usize, inc: &I, margin: f32, flipped: bool, clip: &mut Clip) {
    let (nr, dr) = reff.plane(fr);
    // The incident face: most anti-parallel to the reference normal.
    let mut fi = 0;
    let mut min_dot = f32::MAX;
    for f in 0..inc.face_count() {
        let d = inc.plane(f).0.dot(nr);
        if d < min_dot {
            min_dot = d;
            fi = f;
        }
    }
    let Clip { poly, buf, scratch, planes, cands, .. } = clip;
    inc.face_points(fi, poly);
    let k = side_planes(reff, fr, buf, planes);
    clip_polygon(poly, &planes[..k], scratch);
    let normal = if flipped { -nr } else { nr };
    for i in 0..poly.len {
        let p = poly.pts[i];
        let s = nr.dot(p) - dr;
        if s <= margin {
            let id = ((fr as u32 & 0xff) << 24) | ((fi as u32 & 0xff) << 16) | (poly.keys[i] & 0xffff);
            cands.push(ContactPoint { point: p - nr * (s * 0.5), normal, separation: s, id });
        }
    }
}

/// Contacts of a convex shape against the triangles of `mesh` (placed at `mesh_pose`) near it;
/// normals point from the convex shape into the mesh.
fn mesh_vs_convex(mesh: &TriMesh, mesh_pose: &Pose, c: &Convex, local_aabb: Aabb, c_pose: &Pose, margin: f32, out: &mut Manifold) {
    // The convex shape's bounds in the mesh's frame.
    let rel = mesh_pose.inv_mul(c_pose);
    let world_bounds = shape_bounds_in(&local_aabb, &rel).expand(margin);
    let center = match c {
        Convex::Core(core, _) => core.center(),
        Convex::Poly(p) => p.center(),
    };
    let mut cands = Candidates::new();
    let mut m = Manifold::default();
    let world_hull = match c {
        Convex::Poly(p) => WorldHull::new(p),
        Convex::Core(..) => None,
    };
    // The body's own feature points in the mesh's frame: its vertices (or its core's ends).
    let mut features = [Vec3::ZERO; 64];
    let nf = match c {
        Convex::Core(core, _) => {
            let (e0, e1) = core.endpoints();
            features[0] = mesh_pose.inv_point(e0);
            features[1] = mesh_pose.inv_point(e1);
            2
        }
        Convex::Poly(p) => {
            let n = p.vertex_count().min(features.len());
            for (i, f) in features.iter_mut().enumerate().take(n) {
                *f = mesh_pose.inv_point(p.vertex(i));
            }
            n
        }
    };
    // Gather the nearby triangles, then visit those nearest a feature point first: a box on a
    // fine mesh gets its corners supported before anything else.
    // The candidate list is reused across calls (no allocation per pair per tick).
    let mut near = NEAR.with(|n| std::mem::take(&mut *n.borrow_mut()));
    near.clear();
    let mut overflow = false;
    mesh.each_in(&world_bounds, |t| {
        if near.len() == MAX_MESH_CANDIDATES {
            overflow = true;
            return false;
        }
        let [a, b, cc] = mesh.triangle(t);
        let centroid = (a + b + cc) * (1.0 / 3.0);
        let d = features[..nf].iter().map(|f| crate::math::len_sq(*f - centroid)).fold(f32::MAX, f32::min);
        near.push((d, t as u32));
        true
    });
    near.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let skipped = near.len().saturating_sub(MAX_MESH_TRIANGLES_PER_PAIR) + usize::from(overflow);
    for &(_, t) in near.iter().take(MAX_MESH_TRIANGLES_PER_PAIR) {
        let t = t as usize;
        let local = mesh.triangle(t);
        let tri = Triangle::new(local.map(|v| mesh_pose.point(v)));
        match c {
            Convex::Core(core, r) => core_vs_poly(core, *r, &tri, margin, &mut m),
            Convex::Poly(p) => match &world_hull {
                Some(w) => poly_vs_poly(w, &tri, margin, &mut m),
                None => poly_vs_poly(p, &tri, margin, &mut m),
            },
        }
        if m.len == 0 {
            continue;
        }
        // Seams between triangles: take the triangle's own normal.
        let side = if tri.n.dot(center - tri.v[0]) >= 0.0 { 1.0 } else { -1.0 };
        let nt = tri.n * side;
        let flags = mesh.edge_flags[t];
        let snap = m.as_slice().iter().any(|p| p.normal.dot(-nt) < 0.9999 && on_seam(&tri, p.point, flags));
        if snap {
            let (p, s) = match c {
                Convex::Core(core, r) => {
                    let (e0, e1) = core.endpoints();
                    let e = if nt.dot(e0) <= nt.dot(e1) { e0 } else { e1 };
                    (e - nt * *r, nt.dot(e - tri.v[0]) - r)
                }
                Convex::Poly(poly) => {
                    let q = poly.support(-nt);
                    (q, nt.dot(q - tri.v[0]))
                }
            };
            if s <= margin {
                cands.push(ContactPoint { point: p - nt * (s * 0.5), normal: -nt, separation: s, id: ((t as u32) << 6) | 0x3f });
            }
        } else {
            for p in m.as_slice() {
                cands.push(ContactPoint { id: ((t as u32) << 6) ^ (p.id & 0x3f) ^ ((p.id >> 16) & 0x3f), ..*p });
            }
        }
    }
    cands.reduce(out);
    out.skipped = skipped as u32;
    NEAR.with(|n| *n.borrow_mut() = near);
}

thread_local! {
    /// [`mesh_vs_convex`]'s candidate triangles, reused.
    static NEAR: std::cell::RefCell<Vec<(f32, u32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Whether `p` (near the triangle) sits on one of its internal edges or vertices (an inactive
/// edge in `flags`).
fn on_seam(tri: &Triangle, p: Vec3, flags: u8) -> bool {
    let (_, w) = crate::gjk::closest_on_triangle(p, tri.v[0], tri.v[1], tri.v[2]);
    const E: f32 = 1.0e-3;
    let zero = [w[0] <= E, w[1] <= E, w[2] <= E];
    let active = |e: usize| flags & (1 << e) != 0;
    match zero.iter().filter(|z| **z).count() {
        0 => false,
        1 => {
            let i = zero.iter().position(|z| *z).expect("one");
            !active((i + 1) % 3)
        }
        _ => {
            let j = (0..3).find(|i| !zero[*i]).unwrap_or(0);
            !active(j) && !active((j + 2) % 3)
        }
    }
}

/// Bounds of a local box placed at `pose`.
pub fn shape_bounds_in(local: &Aabb, pose: &Pose) -> Aabb {
    let m = crate::math::Mat3::from_quat(pose.q);
    let c = pose.point(local.center());
    let e = local.extents();
    let half = crate::math::abs(m.c[0]) * e.x + crate::math::abs(m.c[1]) * e.y + crate::math::abs(m.c[2]) * e.z;
    Aabb::around(c, half)
}

/// Whether two placed shapes overlap (touch or interpenetrate).
pub fn overlaps(a: &Shape, pa: &Pose, b: &Shape, pb: &Pose) -> bool {
    let mut m = Manifold::default();
    collide(a, pa, b, pb, 0.0, &mut m);
    m.as_slice().iter().any(|p| p.separation <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;
    use specula_core::math::Quat;

    fn run(a: &Shape, pa: Pose, b: &Shape, pb: Pose) -> Manifold {
        let mut m = Manifold::default();
        collide(a, &pa, b, &pb, 0.02, &mut m);
        m
    }

    #[test]
    fn spheres_and_capsules() {
        let s = Shape::sphere(0.5);
        let m = run(&s, Pose::IDENTITY, &s, Pose::at(v3(0.9, 0.0, 0.0)));
        assert_eq!(m.len, 1);
        assert!((m.points[0].separation + 0.1).abs() < 1e-5 && (m.points[0].normal - Vec3::X).length() < 1e-5);
        assert_eq!(run(&s, Pose::IDENTITY, &s, Pose::at(v3(1.1, 0.0, 0.0))).len, 0);
        // Two capsules lying side by side touch along a line: two points.
        let lying = Quat::from_euler_xyz_deg(0.0, 0.0, 90.0);
        let c = Shape::capsule(0.25, 1.0);
        let m = run(&c, Pose::new(Vec3::ZERO, lying), &c, Pose::new(v3(0.5, 0.45, 0.0), lying));
        assert_eq!(m.len, 2, "{m:?}");
        assert!(m.as_slice().iter().all(|p| (p.separation + 0.05).abs() < 1e-4 && p.normal.y > 0.999));
    }

    #[test]
    fn box_resting_on_box_has_four_points() {
        let ground = Shape::cuboid(v3(5.0, 0.5, 5.0));
        let cube = Shape::cuboid(v3(0.5, 0.5, 0.5));
        let m = run(&ground, Pose::at(v3(0.0, -0.5, 0.0)), &cube, Pose::at(v3(0.2, 0.49, -0.1)));
        assert_eq!(m.len, 4, "{m:?}");
        for p in m.as_slice() {
            assert!((p.separation + 0.01).abs() < 1e-5, "{p:?}");
            assert!((p.normal - Vec3::Y).length() < 1e-5);
        }
        // Ids are stable for the same features.
        let m2 = run(&ground, Pose::at(v3(0.0, -0.5, 0.0)), &cube, Pose::at(v3(0.21, 0.495, -0.1)));
        let ids = |m: &Manifold| {
            let mut v: Vec<u32> = m.as_slice().iter().map(|p| p.id).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(ids(&m), ids(&m2));
        // Rotated cube on its edge: edge or face contacts, never through the ground.
        let tilted = Pose::new(v3(0.0, 0.69, 0.0), Quat::from_euler_xyz_deg(0.0, 0.0, 45.0));
        let m = run(&ground, Pose::at(v3(0.0, -0.5, 0.0)), &cube, tilted);
        assert!(m.len >= 1 && m.as_slice().iter().all(|p| p.normal.y > 0.99 && p.separation < 0.0 && p.separation > -0.03), "{m:?}");
    }

    #[test]
    fn spheres_and_capsules_on_boxes() {
        let ground = Shape::cuboid(v3(5.0, 0.5, 5.0));
        let gp = Pose::at(v3(0.0, -0.5, 0.0));
        let m = run(&Shape::sphere(0.5), Pose::at(v3(1.0, 0.45, 0.0)), &ground, gp);
        assert_eq!(m.len, 1);
        assert!((m.points[0].normal + Vec3::Y).length() < 1e-5 && (m.points[0].separation + 0.05).abs() < 1e-5);
        // A capsule lying on the ground: two points.
        let lying = Pose::new(v3(0.0, 0.24, 0.0), Quat::from_euler_xyz_deg(90.0, 0.0, 0.0));
        let m = run(&Shape::capsule(0.25, 0.5), lying, &ground, gp);
        assert_eq!(m.len, 2, "{m:?}");
        // A sphere whose centre is inside the box is pushed out the nearest face.
        let m = run(&Shape::sphere(0.3), Pose::at(v3(4.9, -0.2, 0.0)), &ground, gp);
        assert_eq!(m.len, 1);
        assert!(m.points[0].normal.x < -0.99, "{m:?}");
    }

    #[test]
    fn meshes_have_no_seams() {
        // A flat floor of two triangles; a box sliding across the diagonal seam only ever
        // gets upward normals.
        let v = vec![v3(-5.0, 0.0, -5.0), v3(5.0, 0.0, -5.0), v3(5.0, 0.0, 5.0), v3(-5.0, 0.0, 5.0)];
        let floor = Shape::mesh(v, vec![[0, 2, 1], [0, 3, 2]]).unwrap();
        let cube = Shape::cuboid(v3(0.5, 0.5, 0.5));
        for x in [-1.0f32, -0.3, 0.0, 0.2, 0.7] {
            let m = run(&cube, Pose::new(v3(x, 0.49, x), Quat::from_euler_xyz_deg(0.0, 30.0, 0.0)), &floor, Pose::IDENTITY);
            assert!(m.len >= 3, "{x}: {m:?}");
            assert!(m.as_slice().iter().all(|p| (p.normal + Vec3::Y).length() < 1e-4), "{x}: {m:?}");
        }
        let hf = Shape::heightfield(3, 3, vec![0.0; 9], (2.0, 2.0)).unwrap();
        let m = run(&Shape::sphere(0.5), Pose::at(v3(0.0, 0.45, 0.0)), &hf, Pose::IDENTITY);
        assert!(
            m.len >= 1 && m.as_slice().iter().all(|p| (p.normal + Vec3::Y).length() < 1e-4 && (p.separation + 0.05).abs() < 1e-4),
            "{m:?}"
        );
    }

    /// The box-box fast path agrees with the general polytope test on random poses.
    #[test]
    fn box_fast_path_matches_the_general_test() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        let (a, b) = (Shape::cuboid(v3(0.5, 0.3, 0.4)), Shape::cuboid(v3(0.35, 0.6, 0.25)));
        let mut compared = 0;
        for _ in 0..2000 {
            let pa = Pose::new(v3(rnd(), rnd(), rnd()), Quat::from_euler_xyz_deg(rnd() * 360.0, rnd() * 360.0, rnd() * 360.0));
            let pb = Pose::new(v3(rnd() * 1.6, rnd() * 1.6, rnd() * 1.6), Quat::from_euler_xyz_deg(rnd() * 360.0, rnd() * 360.0, rnd() * 360.0));
            let mut fast = Manifold::default();
            collide(&a, &pa, &b, &pb, 0.02, &mut fast);
            let (Some(ca), Some(cb)) = (convex(&a, &pa), convex(&b, &pb)) else { panic!() };
            let mut general = Manifold::default();
            convex_vs_convex(&ca, &cb, 0.02, &mut general);
            let min = |m: &Manifold| m.as_slice().iter().map(|p| p.separation).fold(f32::MAX, f32::min);
            assert_eq!(fast.len > 0, general.len > 0, "{pa:?} {pb:?}\n{fast:?}\n{general:?}");
            if fast.len > 0 {
                assert!((min(&fast) - min(&general)).abs() < 2e-3, "{pa:?} {pb:?}\n{fast:?}\n{general:?}");
                compared += 1;
            }
        }
        assert!(compared > 200, "{compared}");
    }

    #[test]
    fn hulls_collide_like_their_shape() {
        let pts = [v3(-0.5, -0.5, -0.5), v3(0.5, -0.5, -0.5), v3(0.0, -0.5, 0.5), v3(0.0, 0.5, 0.0)];
        let tet = Shape::hull(&pts).unwrap();
        let ground = Shape::cuboid(v3(5.0, 0.5, 5.0));
        let m = run(&tet, Pose::at(v3(0.0, 0.49, 0.0)), &ground, Pose::at(v3(0.0, -0.5, 0.0)));
        assert_eq!(m.len, 3, "{m:?}");
        assert!(m.as_slice().iter().all(|p| (p.normal + Vec3::Y).length() < 1e-4 && (p.separation + 0.01).abs() < 1e-4));
    }
}
