//! Vector shapes: paths (lines, polylines, polygons, rectangles with rounded corners, circles,
//! ellipses, arcs, quadratic and cubic Béziers) flattened and tessellated into triangles for
//! [`crate::ShapeDraw`].
//!
//! - **Fills** use the even-odd rule over nested contours: a contour inside an odd number of
//!   others is a hole. Each solid contour with its holes is bridged into one polygon and ear
//!   clipped, then improved by Delaunay edge flips (no needless slivers). Every interior edge is
//!   shared by exactly two triangles and every boundary edge is a contour edge: watertight.
//!   Self-intersecting contours are not supported (they tessellate, but may overlap).
//! - **Strokes** follow each contour with miter (limited), round or bevel joins and butt, square
//!   or round caps. Where a stroke overlaps itself (tight inner corners), translucent strokes
//!   blend twice there.
//! - **Anti-aliasing** (optional): every edge gets a fringe about one pixel wide whose alpha fades
//!   to zero. Fringe vertices store their push direction in [`Vertex::normal`] with its length in
//!   **pixels**; the shape shader moves them in screen space, so edges stay one pixel soft at any
//!   distance or zoom. Without it, edges are hard (and `normal` is zero).
//! - **Deterministic:** `f64` arithmetic, trigonometry from `specula_core::dmath`, fixed
//!   iteration orders (no hash maps): the same path gives the same bits on every platform.
//!
//! Shapes are 2D in their own plane (x right, y up, z = 0): place them with
//! [`crate::ShapeDraw::model`].

use crate::Vertex;
use specula_core::dmath;
use std::collections::BTreeMap;
use std::fmt;

type P2 = [f64; 2];

/// Most points (after flattening) one tessellation call accepts.
pub const MAX_SHAPE_POINTS: usize = 65_536;
/// Most vertices one [`ShapeMesh`] may hold (fills and strokes stop with an error before more).
pub const MAX_SHAPE_VERTICES: usize = 262_144;
/// Most points of one filled polygon (a solid contour with its holes bridged in): ear clipping
/// is quadratic, so this bounds a fill's time.
pub const MAX_FILL_POLYGON: usize = 4096;
/// Most segments of one round join or cap.
pub const MAX_ROUND_SEGMENTS: usize = 64;
/// Most segments of one flattened curve or arc.
pub const MAX_CURVE_SEGMENTS: usize = 1024;
/// Tolerances below this fraction of a path's extent are raised to it (finer is invisible and
/// only costs points).
pub const MIN_RELATIVE_TOLERANCE: f64 = 1e-5;

/// A path command. Coordinates are in the shape's plane (metres, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo([f32; 2]),
    LineTo([f32; 2]),
    /// Quadratic Bézier: control point, end point.
    QuadTo([f32; 2], [f32; 2]),
    /// Cubic Bézier: two control points, end point.
    CubicTo([f32; 2], [f32; 2], [f32; 2]),
    /// An elliptical arc around `center`, from angle `start` (radians, counter-clockwise from +x)
    /// sweeping `sweep` (negative = clockwise). A line joins it to the current point.
    Arc { center: [f32; 2], radii: [f32; 2], start: f32, sweep: f32 },
    /// Back to the contour's first point; the contour is closed.
    Close,
}

/// A sequence of contours, built with the methods below or from [`PathCmd`]s.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub cmds: Vec<PathCmd>,
}

/// A flattened contour.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VectorError {
    /// More than [`MAX_SHAPE_POINTS`] points after flattening.
    TooManyPoints { points: usize, max: usize },
    /// The mesh would pass [`MAX_SHAPE_VERTICES`] vertices.
    TooManyVertices { vertices: usize, max: usize },
    /// A filled polygon (holes bridged in) has more than [`MAX_FILL_POLYGON`] points.
    FillTooComplex { points: usize, max: usize },
    /// A coordinate, size or tolerance is not finite (or a size is negative).
    NotFinite { what: &'static str },
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VectorError::TooManyPoints { points, max } => {
                write!(f, "the shape flattens to {points} points; at most {max} (raise the tolerance or simplify)")
            }
            VectorError::NotFinite { what } => write!(f, "{what} must be finite"),
            VectorError::TooManyVertices { vertices, max } => write!(
                f,
                "the shape needs more than {max} vertices ({vertices} so far); use fewer or simpler items, thinner \
                 strokes or a larger tolerance"
            ),
            VectorError::FillTooComplex { points, max } => write!(
                f,
                "a filled contour (with its holes) has {points} points; at most {max} (raise the tolerance or split it)"
            ),
        }
    }
}

impl std::error::Error for VectorError {}

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn move_to(&mut self, p: [f32; 2]) -> &mut Path {
        self.cmds.push(PathCmd::MoveTo(p));
        self
    }

    pub fn line_to(&mut self, p: [f32; 2]) -> &mut Path {
        self.cmds.push(PathCmd::LineTo(p));
        self
    }

    pub fn quad_to(&mut self, c: [f32; 2], p: [f32; 2]) -> &mut Path {
        self.cmds.push(PathCmd::QuadTo(c, p));
        self
    }

    pub fn cubic_to(&mut self, c1: [f32; 2], c2: [f32; 2], p: [f32; 2]) -> &mut Path {
        self.cmds.push(PathCmd::CubicTo(c1, c2, p));
        self
    }

    pub fn arc_to(&mut self, center: [f32; 2], radii: [f32; 2], start: f32, sweep: f32) -> &mut Path {
        self.cmds.push(PathCmd::Arc { center, radii, start, sweep });
        self
    }

    pub fn close(&mut self) -> &mut Path {
        self.cmds.push(PathCmd::Close);
        self
    }

    /// A closed polygon through `points`.
    pub fn polygon(points: &[[f32; 2]]) -> Path {
        let mut p = Path::polyline(points);
        if !p.cmds.is_empty() {
            p.close();
        }
        p
    }

    /// An open polyline through `points`.
    pub fn polyline(points: &[[f32; 2]]) -> Path {
        let mut p = Path::new();
        for (i, &q) in points.iter().enumerate() {
            if i == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
        p
    }

    /// The rectangle from (`x`, `y`) (bottom left) of `w` × `h`, counter-clockwise.
    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Path {
        Path::polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]])
    }

    /// A rectangle with corners rounded by `r` (clamped to half the shorter side).
    pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
        let r = r.max(0.0).min(0.5 * w.abs().min(h.abs()));
        if r <= 0.0 {
            return Path::rect(x, y, w, h);
        }
        use std::f32::consts::FRAC_PI_2 as Q;
        let mut p = Path::new();
        p.move_to([x + r, y]);
        p.arc_to([x + w - r, y + r], [r, r], -Q, Q);
        p.arc_to([x + w - r, y + h - r], [r, r], 0.0, Q);
        p.arc_to([x + r, y + h - r], [r, r], Q, Q);
        p.arc_to([x + r, y + r], [r, r], 2.0 * Q, Q);
        p.close();
        p
    }

    pub fn circle(center: [f32; 2], r: f32) -> Path {
        Path::ellipse(center, [r, r])
    }

    pub fn ellipse(center: [f32; 2], radii: [f32; 2]) -> Path {
        let mut p = Path::new();
        p.arc_to(center, radii, 0.0, std::f32::consts::TAU);
        p.close();
        p
    }

    /// An open arc of a circle.
    pub fn arc(center: [f32; 2], r: f32, start: f32, sweep: f32) -> Path {
        let mut p = Path::new();
        p.arc_to(center, [r, r], start, sweep);
        p
    }

    /// A closed wedge (pie slice) of a circle.
    pub fn pie(center: [f32; 2], r: f32, start: f32, sweep: f32) -> Path {
        let mut p = Path::new();
        p.move_to(center);
        p.arc_to(center, [r, r], start, sweep);
        p.close();
        p
    }

    /// The largest coordinate or radius magnitude in the path (0 for an empty path).
    pub fn extent(&self) -> f64 {
        let m = |p: &[f32; 2]| (p[0].abs() as f64).max(p[1].abs() as f64);
        self.cmds.iter().fold(0.0f64, |acc, c| {
            let v = match c {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => m(p),
                PathCmd::QuadTo(a, b) => m(a).max(m(b)),
                PathCmd::CubicTo(a, b, c) => m(a).max(m(b)).max(m(c)),
                PathCmd::Arc { center, radii, .. } => m(center) + m(radii),
                PathCmd::Close => 0.0,
            };
            if v.is_finite() { acc.max(v) } else { acc }
        })
    }

    /// Flatten into contours, each curve split so no point of it is farther than `tolerance`
    /// (metres) from the polyline (raised to [`MIN_RELATIVE_TOLERANCE`] × the path's extent;
    /// at most [`MAX_CURVE_SEGMENTS`] per curve). Consecutive duplicate points are dropped.
    pub fn flatten(&self, tolerance: f32) -> Result<Vec<Contour>, VectorError> {
        if !(tolerance.is_finite() && tolerance > 0.0) {
            return Err(VectorError::NotFinite { what: "tolerance (> 0)" });
        }
        let tol = (tolerance as f64).max(self.extent() * MIN_RELATIVE_TOLERANCE);
        let mut out: Vec<Contour> = Vec::new();
        let mut cur: Vec<P2> = Vec::new();
        let mut total = 0usize;
        let p2 = |p: [f32; 2]| -> Result<P2, VectorError> {
            if p[0].is_finite() && p[1].is_finite() {
                Ok([p[0] as f64, p[1] as f64])
            } else {
                Err(VectorError::NotFinite { what: "path coordinates" })
            }
        };
        fn finish(cur: &mut Vec<P2>, closed: bool, out: &mut Vec<Contour>) {
            let mut pts = std::mem::take(cur);
            if closed && pts.len() > 1 && pts.first() == pts.last() {
                pts.pop();
            }
            if !pts.is_empty() {
                out.push(Contour { points: pts, closed });
            }
        }
        let push = |cur: &mut Vec<P2>, p: P2, total: &mut usize| -> Result<(), VectorError> {
            if cur.last() != Some(&p) {
                cur.push(p);
                *total += 1;
                if *total > MAX_SHAPE_POINTS {
                    return Err(VectorError::TooManyPoints { points: *total, max: MAX_SHAPE_POINTS });
                }
            }
            Ok(())
        };
        let mut last: P2 = [0.0, 0.0];
        for cmd in &self.cmds {
            match *cmd {
                PathCmd::MoveTo(p) => {
                    finish(&mut cur, false, &mut out);
                    last = p2(p)?;
                    push(&mut cur, last, &mut total)?;
                }
                PathCmd::LineTo(p) => {
                    if cur.is_empty() {
                        push(&mut cur, last, &mut total)?;
                    }
                    last = p2(p)?;
                    push(&mut cur, last, &mut total)?;
                }
                PathCmd::QuadTo(c, p) => {
                    if cur.is_empty() {
                        push(&mut cur, last, &mut total)?;
                    }
                    let (a, c, b) = (last, p2(c)?, p2(p)?);
                    let d = len(sub(add(a, b), scale(c, 2.0)));
                    let n = segments(d / (4.0 * tol));
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        let u = 1.0 - t;
                        push(&mut cur, [u * u * a[0] + 2.0 * u * t * c[0] + t * t * b[0], u * u * a[1] + 2.0 * u * t * c[1] + t * t * b[1]], &mut total)?;
                    }
                    last = b;
                }
                PathCmd::CubicTo(c1, c2, p) => {
                    if cur.is_empty() {
                        push(&mut cur, last, &mut total)?;
                    }
                    let (a, c1, c2, b) = (last, p2(c1)?, p2(c2)?, p2(p)?);
                    let m = len(add(sub(a, scale(c1, 2.0)), c2)).max(len(add(sub(c1, scale(c2, 2.0)), b)));
                    let n = segments(0.75 * m / tol);
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        let u = 1.0 - t;
                        let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        let q = [
                            w0 * a[0] + w1 * c1[0] + w2 * c2[0] + w3 * b[0],
                            w0 * a[1] + w1 * c1[1] + w2 * c2[1] + w3 * b[1],
                        ];
                        push(&mut cur, q, &mut total)?;
                    }
                    last = b;
                }
                PathCmd::Arc { center, radii, start, sweep } => {
                    let c = p2(center)?;
                    let (rx, ry) = (radii[0] as f64, radii[1] as f64);
                    if !(rx.is_finite() && ry.is_finite() && start.is_finite() && sweep.is_finite()) || rx < 0.0 || ry < 0.0 {
                        return Err(VectorError::NotFinite { what: "arc radii and angles (radii >= 0)" });
                    }
                    let (start, mut sweep) = (start as f64, sweep as f64);
                    // A full turn given in f32 (TAU rounds) closes exactly on its first point.
                    let full = (sweep.abs() - std::f64::consts::TAU).abs() < 1e-5;
                    if full {
                        sweep = std::f64::consts::TAU.copysign(sweep);
                    }
                    let n = arc_segments(rx.max(ry), sweep, tol);
                    let first = cur.len();
                    for k in 0..=n {
                        if full && k == n {
                            let p0 = cur[first.min(cur.len() - 1)];
                            push(&mut cur, p0, &mut total)?;
                            continue;
                        }
                        let (s, co) = dmath::sin_cos_f64(start + sweep * (k as f64 / n as f64));
                        push(&mut cur, [c[0] + rx * co, c[1] + ry * s], &mut total)?;
                    }
                    last = *cur.last().expect("pushed");
                }
                PathCmd::Close => {
                    if let Some(&first) = cur.first() {
                        last = first;
                    }
                    finish(&mut cur, true, &mut out);
                }
            }
        }
        finish(&mut cur, false, &mut out);
        Ok(out)
    }
}

/// Segments for a curve whose chord error bound is `k / n²`: `ceil(sqrt(k))`, 1..=1024.
fn segments(k: f64) -> usize {
    if k.is_nan() || k <= 1.0 {
        return 1;
    }
    (k.sqrt().ceil() as usize).clamp(1, MAX_CURVE_SEGMENTS)
}

/// Segments for an arc of radius `r` sweeping `sweep` so the sagitta stays within `tol`.
fn arc_segments(r: f64, sweep: f64, tol: f64) -> usize {
    if r <= tol || sweep == 0.0 {
        return 1.max((sweep.abs() / (std::f64::consts::PI / 2.0)).ceil() as usize);
    }
    let step = 2.0 * dmath::acos_f64(1.0 - tol / r);
    let n = sweep.abs() / step;
    if n.is_finite() { (n.ceil() as usize).clamp(1, MAX_CURVE_SEGMENTS) } else { MAX_CURVE_SEGMENTS }
}

/// Room for `more` vertices in `mesh`, or the structured error.
fn reserve(mesh: &ShapeMesh, more: usize) -> Result<(), VectorError> {
    let vertices = mesh.vertices.len().saturating_add(more);
    if vertices > MAX_SHAPE_VERTICES {
        Err(VectorError::TooManyVertices { vertices, max: MAX_SHAPE_VERTICES })
    } else {
        Ok(())
    }
}

fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(a: P2, k: f64) -> P2 {
    [a[0] * k, a[1] * k]
}

fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn len(a: P2) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: P2) -> P2 {
    let l = len(a);
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        [0.0, 0.0]
    }
}

/// The left normal of a unit direction.
fn left(d: P2) -> P2 {
    [-d[1], d[0]]
}

/// Twice the signed area (positive = counter-clockwise).
fn area2(ring: &[P2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| cross(ring[i], ring[(i + 1) % n])).sum()
}

/// `p` inside `ring` (even-odd crossing test; points on an edge may go either way).
fn inside(ring: &[P2], p: P2) -> bool {
    let n = ring.len();
    let mut c = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
        j = i;
    }
    c
}

/// A triangle mesh for [`crate::Gpu::upload_mesh`]: positions in the shape's plane (z = 0),
/// fringe push directions (pixels) in `normal`, sRGB colours with alpha.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Ear-clipping work so far: the sum of each clipped polygon's points squared (at most
    /// [`MAX_FILL_WORK`] per mesh, so a shape of many big fills stays quick too).
    pub fill_work: u64,
}

/// Most ear-clipping work (sum of polygon points squared) one mesh may take: about two of the
/// largest fills ([`MAX_FILL_POLYGON`] points each).
pub const MAX_FILL_WORK: u64 = 2 * (MAX_FILL_POLYGON as u64) * (MAX_FILL_POLYGON as u64);

impl ShapeMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    fn vertex(&mut self, p: P2, push: P2, color: [u8; 4]) -> u32 {
        self.vertices.push(Vertex::with_color([p[0] as f32, p[1] as f32, 0.0], [push[0] as f32, push[1] as f32, 0.0], color));
        (self.vertices.len() - 1) as u32
    }

    fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.indices.extend_from_slice(&[a, b, c, a, c, d]);
    }
}

/// Contours cleaned for filling: closed, no repeated or collinear points, at least 3 points and
/// some area.
fn fill_rings(contours: &[Contour]) -> Vec<Vec<P2>> {
    let mut rings = Vec::new();
    for c in contours {
        let mut r = dedup(&c.points, true);
        // Drop collinear (and spike) points until none is left.
        let mut changed = true;
        while changed && r.len() >= 3 {
            changed = false;
            let n = r.len();
            let mut keep = Vec::with_capacity(n);
            for i in 0..n {
                let (a, b, c) = (r[(i + n - 1) % n], r[i], r[(i + 1) % n]);
                let (u, v) = (sub(b, a), sub(c, b));
                let scale = len(u) * len(v);
                if cross(u, v).abs() <= 1e-12 * scale && dot(u, v) >= 0.0 && scale > 0.0 || b == c {
                    changed = true;
                    continue; // straight through (or repeated): not a corner
                }
                if cross(u, v).abs() <= 1e-12 * scale && dot(u, v) < 0.0 {
                    changed = true;
                    continue; // a spike back along itself: no area
                }
                keep.push(b);
            }
            if changed {
                r = keep;
            }
        }
        if r.len() >= 3 && area2(&r).abs() > 1e-18 {
            rings.push(r);
        }
    }
    rings
}

/// `points` without consecutive points closer than a millionth of the contour's extent (f32
/// rounding leaves such near-duplicates, e.g. where a circle closes); `closed`: also not the
/// last next to the first.
fn dedup(points: &[P2], closed: bool) -> Vec<P2> {
    let extent = points.iter().fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs()));
    let eps = 1e-6 * extent;
    let near = |a: P2, b: P2| (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps;
    let mut r: Vec<P2> = Vec::with_capacity(points.len());
    for &p in points {
        if r.last().is_none_or(|&q| !near(q, p)) {
            r.push(p);
        }
    }
    while closed && r.len() > 1 && near(r[0], r[r.len() - 1]) {
        r.pop();
    }
    r
}

/// Fill `contours` (even-odd over nesting) with `color` (sRGB, straight alpha) into `mesh`.
pub fn fill(contours: &[Contour], color: [u8; 4], aa: bool, mesh: &mut ShapeMesh) -> Result<(), VectorError> {
    let mut rings = fill_rings(contours);
    let total: usize = rings.iter().map(Vec::len).sum();
    if total > MAX_SHAPE_POINTS {
        return Err(VectorError::TooManyPoints { points: total, max: MAX_SHAPE_POINTS });
    }
    // Nesting depth: how many other rings contain each ring (tested at its first edge's middle).
    let probe: Vec<P2> = rings.iter().map(|r| scale(add(r[0], r[1]), 0.5)).collect();
    let depth: Vec<usize> =
        (0..rings.len()).map(|i| (0..rings.len()).filter(|&j| j != i && inside(&rings[j], probe[i])).count()).collect();
    // Solid rings counter-clockwise, holes clockwise: the filled side is always on the left.
    for (r, &d) in rings.iter_mut().zip(&depth) {
        if (area2(r) > 0.0) != (d % 2 == 0) {
            r.reverse();
        }
    }
    // Global vertices: one per ring point.
    let mut pos: Vec<P2> = Vec::with_capacity(total);
    let mut ring_ids: Vec<Vec<u32>> = Vec::with_capacity(rings.len());
    for r in &rings {
        ring_ids.push((pos.len() as u32..(pos.len() + r.len()) as u32).collect());
        pos.extend_from_slice(r);
    }
    reserve(mesh, pos.len() * if aa { 2 } else { 1 })?;
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    for (i, _) in rings.iter().enumerate().filter(|(i, _)| depth[*i] % 2 == 0) {
        // Holes: odd rings whose innermost container is ring i.
        let mut holes: Vec<usize> = (0..rings.len())
            .filter(|&h| depth[h] == depth[i] + 1 && inside(&rings[i], probe[h]))
            .collect();
        holes.sort_by(|&a, &b| {
            let mx = |h: usize| rings[h].iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
            mx(b).total_cmp(&mx(a)).then(a.cmp(&b))
        });
        let points = ring_ids[i].len() + holes.iter().map(|&h| ring_ids[h].len() + 2).sum::<usize>();
        if points > MAX_FILL_POLYGON {
            return Err(VectorError::FillTooComplex { points, max: MAX_FILL_POLYGON });
        }
        let mut poly = ring_ids[i].clone();
        for h in holes {
            bridge(&mut poly, &ring_ids[h], &pos);
        }
        let start = triangles.len();
        if poly.len() == ring_ids[i].len() && convex(&poly, &pos) {
            // No holes and convex: a zigzag strip from both ends (linear time, no slivers to fix
            // beyond what the outline forces).
            strip(&poly, &mut triangles);
            delaunay_flips(&mut triangles[start..], &pos);
        } else {
            mesh.fill_work = mesh.fill_work.saturating_add((poly.len() as u64).pow(2));
            if mesh.fill_work > MAX_FILL_WORK {
                return Err(VectorError::FillTooComplex { points: poly.len(), max: MAX_FILL_POLYGON });
            }
            ear_clip(&poly, &pos, &mut triangles);
            delaunay_flips(&mut triangles[start..], &pos);
        }
    }
    // Emit.
    let base = mesh.vertices.len() as u32;
    let miters: Vec<P2> = if aa { ring_miters(&ring_ids, &pos) } else { vec![[0.0, 0.0]; pos.len()] };
    for (p, m) in pos.iter().zip(&miters) {
        mesh.vertex(*p, scale(*m, -0.5), color);
    }
    for t in &triangles {
        mesh.indices.extend(t.iter().map(|v| base + v));
    }
    if aa {
        let clear = [color[0], color[1], color[2], 0];
        let outer_base = mesh.vertices.len() as u32;
        for (p, m) in pos.iter().zip(&miters) {
            mesh.vertex(*p, scale(*m, 0.5), clear);
        }
        for ids in &ring_ids {
            let n = ids.len();
            for k in 0..n {
                let (a, b) = (ids[k], ids[(k + 1) % n]);
                // Inner edge a -> b (fill on its left), outer copies on the right.
                mesh.quad(base + a, outer_base + a, outer_base + b, base + b);
            }
        }
    }
    Ok(())
}

/// Outward miter vectors (unit perpendicular distance to both edges, at most 4 long) of every
/// ring vertex; the filled side is on each ring's left.
fn ring_miters(ring_ids: &[Vec<u32>], pos: &[P2]) -> Vec<P2> {
    let mut out = vec![[0.0, 0.0]; pos.len()];
    for ids in ring_ids {
        let n = ids.len();
        for k in 0..n {
            let (u, v, w) = (pos[ids[(k + n - 1) % n] as usize], pos[ids[k] as usize], pos[ids[(k + 1) % n] as usize]);
            // Right normals (outward).
            let n0 = scale(left(unit(sub(v, u))), -1.0);
            let n1 = scale(left(unit(sub(w, v))), -1.0);
            out[ids[k] as usize] = miter(n0, n1, 4.0);
        }
    }
    out
}

/// The miter of two unit normals: perpendicular distance 1 to both lines, clamped to `limit`.
fn miter(n0: P2, n1: P2, limit: f64) -> P2 {
    let d = 1.0 + dot(n0, n1);
    let m = if d > 1e-9 { scale(add(n0, n1), 1.0 / d) } else { n0 };
    let l = len(m);
    if l > limit {
        scale(m, limit / l)
    } else {
        m
    }
}

/// Splices hole `hole` (clockwise) into polygon `poly` (counter-clockwise) through a bridge from
/// the hole's rightmost vertex to a visible polygon vertex (Eberly's method).
fn bridge(poly: &mut Vec<u32>, hole: &[u32], pos: &[P2]) {
    let p = |i: u32| pos[i as usize];
    // The hole's rightmost vertex (lowest y, then first, on ties).
    let hm = (0..hole.len())
        .max_by(|&a, &b| p(hole[a])[0].total_cmp(&p(hole[b])[0]).then(p(hole[b])[1].total_cmp(&p(hole[a])[1])).then(b.cmp(&a)))
        .expect("rings have points");
    let m = p(hole[hm]);
    // The nearest polygon edge hit by the ray from m towards +x.
    let n = poly.len();
    let mut best: Option<(f64, usize)> = None; // (x of the hit, index of the edge's start)
    for k in 0..n {
        let (a, b) = (p(poly[k]), p(poly[(k + 1) % n]));
        if (a[1] - m[1]) * (b[1] - m[1]) > 0.0 || a[1] == b[1] {
            continue;
        }
        let x = a[0] + (m[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
        if x >= m[0] && best.is_none_or(|(bx, _)| x < bx) {
            best = Some((x, k));
        }
    }
    let target = match best {
        None => (0..n).min_by(|&a, &b| len(sub(p(poly[a]), m)).total_cmp(&len(sub(p(poly[b]), m)))).expect("non-empty"),
        Some((x, k)) => {
            let (ka, kb) = (k, (k + 1) % n);
            let i = [x, m[1]];
            // The edge end farther along +x is visible unless a reflex vertex lies in the
            // triangle (m, i, end); then the one at the smallest angle to the ray is.
            let mut cand = if p(poly[ka])[0] > p(poly[kb])[0] { ka } else { kb };
            if p(poly[ka]) == i {
                cand = ka;
            } else if p(poly[kb]) == i {
                cand = kb;
            }
            let c = p(poly[cand]);
            let tri = [m, i, c];
            let mut pick = cand;
            let mut pick_key = (f64::INFINITY, f64::INFINITY);
            for j in 0..n {
                if j == cand {
                    continue;
                }
                let q = p(poly[j]);
                let (prev, next) = (p(poly[(j + n - 1) % n]), p(poly[(j + 1) % n]));
                let reflex = cross(sub(q, prev), sub(next, q)) <= 0.0;
                if reflex && q != m && point_in_triangle(q, tri[0], tri[1], tri[2]) {
                    let d = sub(q, m);
                    let key = (d[1].abs() / d[0].max(1e-300), len(d));
                    if key < pick_key {
                        pick_key = key;
                        pick = j;
                    }
                }
            }
            if pick_key.0.is_finite() {
                pick
            } else {
                cand
            }
        }
    };
    // poly[..=target], hole from m around back to m, then poly[target..].
    let mut spliced = Vec::with_capacity(n + hole.len() + 2);
    spliced.extend_from_slice(&poly[..=target]);
    spliced.extend((0..=hole.len()).map(|k| hole[(hm + k) % hole.len()]));
    spliced.extend_from_slice(&poly[target..]);
    *poly = spliced;
}

/// A convex counter-clockwise ring around `center` as a triangle fan (with its fringe): a dot of
/// a round- or square-capped stroke, in linear time.
fn disc_fan(center: P2, ring: &[P2], color: [u8; 4], aa: bool, mesh: &mut ShapeMesh) -> Result<(), VectorError> {
    let n = ring.len();
    reserve(mesh, 1 + n * if aa { 2 } else { 1 })?;
    let ids = vec![(0..n as u32).collect::<Vec<u32>>()];
    let miters = if aa { ring_miters(&ids, ring) } else { vec![[0.0, 0.0]; n] };
    let c = mesh.vertex(center, [0.0, 0.0], color);
    let base = mesh.vertices.len() as u32;
    for (p, m) in ring.iter().zip(&miters) {
        mesh.vertex(*p, scale(*m, -0.5), color);
    }
    for k in 0..n as u32 {
        mesh.indices.extend_from_slice(&[c, base + k, base + (k + 1) % n as u32]);
    }
    if aa {
        let clear = [color[0], color[1], color[2], 0];
        let outer = mesh.vertices.len() as u32;
        for (p, m) in ring.iter().zip(&miters) {
            mesh.vertex(*p, scale(*m, 0.5), clear);
        }
        for k in 0..n as u32 {
            let j = (k + 1) % n as u32;
            mesh.quad(base + k, outer + k, outer + j, base + j);
        }
    }
    Ok(())
}

/// Every corner of the counter-clockwise polygon turns left (or goes straight).
fn convex(poly: &[u32], pos: &[P2]) -> bool {
    let n = poly.len();
    let p = |k: usize| pos[poly[k % n] as usize];
    (0..n).all(|k| cross(sub(p(k + 1), p(k)), sub(p(k + 2), p(k + 1))) >= 0.0)
}

/// Triangulates a convex counter-clockwise polygon as a strip zigzagging from both ends.
fn strip(poly: &[u32], out: &mut Vec<[u32; 3]>) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let (mut lo, mut hi) = (0usize, n - 1);
    let mut from_lo = true;
    while hi - lo >= 2 {
        if from_lo {
            out.push([poly[lo], poly[lo + 1], poly[hi]]);
            lo += 1;
        } else {
            out.push([poly[lo], poly[hi - 1], poly[hi]]);
            hi -= 1;
        }
        from_lo = !from_lo;
    }
}

/// `p` in the closed triangle `a b c` (either orientation).
fn point_in_triangle(p: P2, a: P2, b: P2, c: P2) -> bool {
    let (d1, d2, d3) = (cross(sub(b, a), sub(p, a)), cross(sub(c, b), sub(p, b)), cross(sub(a, c), sub(p, c)));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Ear clipping of a simple counter-clockwise polygon (indices into `pos`; bridged polygons
/// repeat bridge vertices). Appends counter-clockwise triangles.
fn ear_clip(poly: &[u32], pos: &[P2], out: &mut Vec<[u32; 3]>) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let p = |k: usize| pos[poly[k] as usize];
    let mut prev: Vec<usize> = (0..n).map(|k| (k + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|k| (k + 1) % n).collect();
    let mut alive = vec![true; n];
    let corner = |prev: &[usize], next: &[usize], k: usize| cross(sub(p(k), p(prev[k])), sub(p(next[k]), p(k)));
    let mut remaining = n;
    let mut k = 0;
    let mut stalled = 0;
    while remaining > 3 {
        let (a, c) = (prev[k], next[k]);
        let convex = corner(&prev, &next, k) > 0.0;
        let mut ear = convex;
        if ear {
            let (pa, pb, pc) = (p(a), p(k), p(c));
            // Only non-convex vertices can lie inside a convex ear.
            let mut j = next[c];
            while j != a {
                let q = p(j);
                if q != pa && q != pb && q != pc && corner(&prev, &next, j) <= 0.0 && point_in_triangle(q, pa, pb, pc) {
                    ear = false;
                    break;
                }
                j = next[j];
            }
        }
        if !ear && stalled > remaining {
            // No ear in a whole lap (degenerate input): clip the flattest corner to make progress.
            let mut j = k;
            let mut best = (f64::INFINITY, k);
            for _ in 0..remaining {
                let area = corner(&prev, &next, j).abs();
                if area < best.0 {
                    best = (area, j);
                }
                j = next[j];
            }
            k = best.1;
            ear = true;
        }
        if ear {
            let (a, c) = (prev[k], next[k]);
            if corner(&prev, &next, k) != 0.0 {
                out.push([poly[a], poly[k], poly[c]]);
            }
            next[a] = c;
            prev[c] = a;
            alive[k] = false;
            remaining -= 1;
            stalled = 0;
            k = a;
        } else {
            k = next[k];
            stalled += 1;
        }
    }
    let a = (0..n).find(|&i| alive[i]).expect("three left");
    let (b, c) = (next[a], next[next[a]]);
    if cross(sub(p(b), p(a)), sub(p(c), p(b))) != 0.0 {
        out.push([poly[a], poly[b], poly[c]]);
    }
}

/// Lawson flips towards a Delaunay triangulation (no needless slivers). Only edges shared by two
/// triangles flip, and only when the quad they form is strictly convex, so the boundary and the
/// covered area never change.
fn delaunay_flips(tris: &mut [[u32; 3]], pos: &[P2]) {
    let p = |i: u32| pos[i as usize];
    let mut edges: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (t, tri) in tris.iter().enumerate() {
        for e in 0..3 {
            edges.insert((tri[e], tri[(e + 1) % 3]), t);
        }
    }
    let mut stack: Vec<(u32, u32)> = edges.keys().copied().filter(|&(a, b)| a < b).collect();
    stack.reverse();
    let mut budget = 16 * tris.len() + 64;
    while let Some((a, b)) = stack.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let (Some(&t1), Some(&t2)) = (edges.get(&(a, b)), edges.get(&(b, a))) else { continue };
        if t1 == t2 {
            continue;
        }
        let opposite = |t: usize, a: u32, b: u32| tris[t].iter().copied().find(|&v| v != a && v != b);
        let (Some(c), Some(d)) = (opposite(t1, a, b), opposite(t2, b, a)) else { continue };
        if c == d {
            continue;
        }
        // t1 = (a, b, c) counter-clockwise; d is across a-b.
        let (pa, pb, pc, pd) = (p(a), p(b), p(c), p(d));
        // Clearly inside only: points on one circle (every flattened arc) would otherwise flip
        // back and forth on rounding noise.
        let s = dot(sub(pa, pc), sub(pa, pc)).max(dot(sub(pb, pc), sub(pb, pc))).max(dot(sub(pd, pc), sub(pd, pc)));
        let ic = in_circle(pa, pb, pc, pd);
        if ic.is_nan() || ic <= 1e-9 * s * s {
            continue;
        }
        // The new edge c-d must lie inside the quad: a and b on opposite sides, strictly.
        if !(cross(sub(pd, pc), sub(pa, pc)) < 0.0 && cross(sub(pd, pc), sub(pb, pc)) > 0.0) {
            continue;
        }
        if edges.contains_key(&(c, d)) || edges.contains_key(&(d, c)) {
            continue;
        }
        for (u, v) in [(a, b), (b, c), (c, a)] {
            edges.remove(&(u, v));
        }
        for (u, v) in [(b, a), (a, d), (d, b)] {
            edges.remove(&(u, v));
        }
        tris[t1] = [c, a, d];
        tris[t2] = [d, b, c];
        for (t, tri) in [(t1, tris[t1]), (t2, tris[t2])] {
            for e in 0..3 {
                edges.insert((tri[e], tri[(e + 1) % 3]), t);
            }
        }
        for (u, v) in [(a, d), (d, b), (b, c), (c, a)] {
            stack.push((u.min(v), u.max(v)));
        }
    }
}

/// Positive when `d` lies inside the circumcircle of the counter-clockwise triangle `a b c`.
fn in_circle(a: P2, b: P2, c: P2, d: P2) -> f64 {
    let (ad, bd, cd) = (sub(a, d), sub(b, d), sub(c, d));
    let (a2, b2, c2) = (dot(ad, ad), dot(bd, bd), dot(cd, cd));
    ad[0] * (bd[1] * c2 - b2 * cd[1]) - ad[1] * (bd[0] * c2 - b2 * cd[0]) + a2 * (bd[0] * cd[1] - bd[1] * cd[0])
}

/// How strokes meet at corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineJoin {
    /// Sharp corners; past `limit` × the half width (SVG's miter limit) they are beveled.
    Miter { limit: f32 },
    Round,
    Bevel,
}

/// How open strokes end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    /// At the end point.
    #[default]
    Butt,
    /// Half the width past it.
    Square,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    /// Metres.
    pub width: f32,
    /// sRGB, straight alpha.
    pub color: [u8; 4],
    pub join: LineJoin,
    pub cap: LineCap,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        StrokeStyle { width: 0.05, color: [255; 4], join: LineJoin::Miter { limit: 4.0 }, cap: LineCap::Butt }
    }
}

/// One cross-section of a stroke: its left and right points with their outward push
/// directions, an extra push along the path (cap fringes) and its opacity (0 for cap fringes).
#[derive(Clone, Copy, Debug)]
struct Pair {
    l: P2,
    r: P2,
    ol: P2,
    or: P2,
    along: P2,
    alpha: f64,
}

fn pair(p: P2, ol: P2, or: P2, hw: f64) -> Pair {
    Pair { l: add(p, scale(ol, hw)), r: add(p, scale(or, hw)), ol, or, along: [0.0, 0.0], alpha: 1.0 }
}

/// Stroke `contours` into `mesh`; curves of round joins and caps are split to `tolerance`.
pub fn stroke(contours: &[Contour], style: &StrokeStyle, tolerance: f32, aa: bool, mesh: &mut ShapeMesh) -> Result<(), VectorError> {
    if !(style.width.is_finite() && style.width >= 0.0) {
        return Err(VectorError::NotFinite { what: "stroke width (>= 0)" });
    }
    if !(tolerance.is_finite() && tolerance > 0.0) {
        return Err(VectorError::NotFinite { what: "tolerance (> 0)" });
    }
    let hw = style.width as f64 * 0.5;
    if hw <= 0.0 {
        return Ok(());
    }
    // Round joins and caps: finer than a thousandth of the half width is invisible.
    let tol = (tolerance as f64).max(hw * 1e-3);
    let per_pair = if aa { 4 } else { 2 };
    // A lower bound of what the strokes need, so a hopeless shape fails before any work.
    let disc = 4 * arc_segments(hw, std::f64::consts::FRAC_PI_2, tol).min(MAX_ROUND_SEGMENTS);
    let least: usize = contours
        .iter()
        .map(|c| match c.points.len() {
            0 => 0,
            1 if style.cap == LineCap::Butt => 0,
            1 => disc * (per_pair / 2),
            n => n * per_pair,
        })
        .fold(0usize, usize::saturating_add);
    reserve(mesh, least)?;
    let mut pairs: Vec<Pair> = Vec::new();
    for c in contours {
        let pts = dedup(&c.points, c.closed);
        if pts.len() == 1 {
            // A dot: round and square caps still show.
            let p = pts[0];
            // A disc as round as a round cap (at most 4 × MAX_ROUND_SEGMENTS points), or a square.
            let points: Vec<P2> = match style.cap {
                LineCap::Butt => continue,
                LineCap::Round => {
                    let n = disc;
                    (0..n)
                        .map(|k| {
                            let (s, c) = dmath::sin_cos_f64(std::f64::consts::TAU * k as f64 / n as f64);
                            [p[0] + hw * c, p[1] + hw * s]
                        })
                        .collect()
                }
                LineCap::Square => vec![[p[0] - hw, p[1] - hw], [p[0] + hw, p[1] - hw], [p[0] + hw, p[1] + hw], [p[0] - hw, p[1] + hw]],
            };
            disc_fan(p, &points, style.color, aa, mesh)?;
            continue;
        }
        let closed = c.closed && pts.len() > 2;
        pairs.clear();
        let n = pts.len();
        let dir = |i: usize| unit(sub(pts[(i + 1) % n], pts[i]));
        if !closed {
            let d0 = dir(0);
            cap_pairs(&mut pairs, pts[0], d0, hw, style.cap, tol, aa, true);
        }
        let (first, last) = if closed { (0, n) } else { (1, n - 1) };
        for i in first..last {
            let d_in = dir((i + n - 1) % n);
            let d_out = dir(i);
            join_pairs(&mut pairs, pts[i], d_in, d_out, hw, style.join, tol, len(sub(pts[i], pts[(i + n - 1) % n])).min(len(sub(pts[(i + 1) % n], pts[i]))));
            reserve(mesh, (pairs.len() + 2 * MAX_ROUND_SEGMENTS) * per_pair)?;
        }
        if closed {
            let again = pairs[0];
            pairs.push(again);
        } else {
            let d = dir(n - 2);
            cap_pairs(&mut pairs, pts[n - 1], d, hw, style.cap, tol, aa, false);
        }
        reserve(mesh, pairs.len() * per_pair)?;
        emit_strip(&pairs, style.color, aa, mesh);
    }
    Ok(())
}

/// Cross-sections at a corner: one for a miter, several around the outside for bevel and round.
#[allow(clippy::too_many_arguments)]
fn join_pairs(out: &mut Vec<Pair>, p: P2, d0: P2, d1: P2, hw: f64, join: LineJoin, tol: f64, shortest: f64) {
    let (l0, l1) = (left(d0), left(d1));
    let turn = cross(d0, d1);
    let m = miter(l0, l1, f64::INFINITY);
    let miter_ok = match join {
        LineJoin::Miter { limit } => len(m) <= (limit as f64).max(1.0),
        _ => false,
    };
    if miter_ok || dot(d0, d1) > 1.0 - 1e-12 {
        out.push(pair(p, m, scale(m, -1.0), hw));
        return;
    }
    // The inner side meets at the (clamped) miter point; the outer side goes around.
    let reach = (hw * hw + shortest * shortest).sqrt();
    let inner_len = len(m) * hw;
    let inner = if inner_len > reach { scale(m, reach / inner_len) } else { m };
    let angle = dmath::atan2_f64(turn, dot(d0, d1)); // signed turn
    let steps = match join {
        LineJoin::Round => arc_segments(hw, angle, tol).min(MAX_ROUND_SEGMENTS),
        _ => 1,
    };
    for k in 0..=steps {
        let (s, c) = dmath::sin_cos_f64(angle * (k as f64 / steps as f64));
        if turn >= 0.0 {
            // Left turn: outside on the right, from −l0 towards −l1.
            let o = [-l0[0] * c + l0[1] * s, -l0[1] * c - l0[0] * s];
            out.push(Pair { l: add(p, scale(inner, hw)), r: add(p, scale(o, hw)), ol: inner, or: o, along: [0.0, 0.0], alpha: 1.0 });
        } else {
            let o = [l0[0] * c - l0[1] * s, l0[1] * c + l0[0] * s];
            let neg = scale(inner, -1.0);
            out.push(Pair { l: add(p, scale(o, hw)), r: add(p, scale(neg, hw)), ol: o, or: neg, along: [0.0, 0.0], alpha: 1.0 });
        }
    }
}

/// Cross-sections of a cap at `p`, the path leaving along `d` (`start`) or arriving along it.
#[allow(clippy::too_many_arguments)]
fn cap_pairs(out: &mut Vec<Pair>, p: P2, d: P2, hw: f64, cap: LineCap, tol: f64, aa: bool, start: bool) {
    let l = left(d);
    let back = if start { scale(d, -1.0) } else { d }; // away from the stroke
    match cap {
        LineCap::Round => {
            let steps = arc_segments(hw, std::f64::consts::FRAC_PI_2, tol).min(MAX_ROUND_SEGMENTS);
            let mut ring: Vec<Pair> = (0..=steps)
                .map(|k| {
                    let (s, c) = dmath::sin_cos_f64(std::f64::consts::FRAC_PI_2 * (k as f64 / steps as f64));
                    // From the tip (k = 0) to the full width.
                    let ol = add(scale(back, c), scale(l, s));
                    let or = add(scale(back, c), scale(l, -s));
                    Pair { l: add(p, scale(ol, hw)), r: add(p, scale(or, hw)), ol, or, along: [0.0, 0.0], alpha: 1.0 }
                })
                .collect();
            if !start {
                ring.reverse();
            }
            out.extend(ring);
        }
        LineCap::Butt | LineCap::Square => {
            let q = if cap == LineCap::Square { add(p, scale(back, hw)) } else { p };
            let body = Pair { along: if aa { scale(back, -0.5) } else { [0.0, 0.0] }, ..pair(q, l, scale(l, -1.0), hw) };
            let fringe = Pair { along: scale(back, 0.5), alpha: 0.0, ..body };
            if !aa {
                out.push(body);
            } else if start {
                out.extend([fringe, body]);
            } else {
                out.extend([body, fringe]);
            }
        }
    }
}

/// Quads between consecutive cross-sections: the stroke, and with `aa` its two side fringes.
fn emit_strip(pairs: &[Pair], color: [u8; 4], aa: bool, mesh: &mut ShapeMesh) {
    let clear = [color[0], color[1], color[2], 0];
    let with = |a: f64| [color[0], color[1], color[2], (color[3] as f64 * a).round() as u8];
    let mut prev: Option<[u32; 4]> = None;
    for q in pairs {
        let ids = if aa {
            [
                mesh.vertex(q.l, add(scale(q.ol, 0.5), q.along), clear),
                mesh.vertex(q.l, add(scale(q.ol, -0.5), q.along), with(q.alpha)),
                mesh.vertex(q.r, add(scale(q.or, -0.5), q.along), with(q.alpha)),
                mesh.vertex(q.r, add(scale(q.or, 0.5), q.along), clear),
            ]
        } else {
            let (a, b) = (mesh.vertex(q.l, [0.0, 0.0], color), mesh.vertex(q.r, [0.0, 0.0], color));
            [a, a, b, b]
        };
        if let Some(p) = prev {
            if aa {
                mesh.quad(p[0], p[1], ids[1], ids[0]);
                mesh.quad(p[1], p[2], ids[2], ids[1]);
                mesh.quad(p[2], p[3], ids[3], ids[2]);
            } else {
                mesh.quad(p[1], p[2], ids[2], ids[1]);
            }
        }
        prev = Some(ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Triangles of a non-AA fill as positions.
    fn tris(mesh: &ShapeMesh) -> Vec<[P2; 3]> {
        let p = |i: u32| {
            let v = mesh.vertices[i as usize].position;
            [v[0] as f64, v[1] as f64]
        };
        mesh.indices.chunks(3).map(|t| [p(t[0]), p(t[1]), p(t[2])]).collect()
    }

    fn tri_area(t: &[P2; 3]) -> f64 {
        0.5 * cross(sub(t[1], t[0]), sub(t[2], t[0]))
    }

    /// Checks a fill: every triangle counter-clockwise with real area, areas sum to `area`, and
    /// every directed edge is either a ring edge (once) or matched by its reverse (watertight).
    fn check_fill(contours: &[Contour], area: f64, min_angle_deg: f64) {
        let mut mesh = ShapeMesh::default();
        fill(contours, [255; 4], false, &mut mesh).unwrap();
        let ts = tris(&mesh);
        let total: f64 = ts.iter().map(tri_area).sum();
        assert!((total - area).abs() < 1e-4 * area.max(1.0), "area {total} vs {area}");
        let mut worst = 180.0f64;
        for t in &ts {
            assert!(tri_area(t) > 1e-12, "degenerate or clockwise triangle {t:?}");
            for k in 0..3 {
                let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                let cosv = dot(unit(sub(b, a)), unit(sub(c, a))).clamp(-1.0, 1.0);
                worst = worst.min(dmath::acos_f64(cosv).to_degrees());
            }
        }
        assert!(worst >= min_angle_deg, "sliver: smallest angle {worst:.3}°");
        // Delaunay: no interior edge of a convex quad can be flipped to a better pair.
        let mut owner: BTreeMap<(u32, u32), [u32; 3]> = BTreeMap::new();
        for t in mesh.indices.chunks(3) {
            for k in 0..3 {
                owner.insert((t[k], t[(k + 1) % 3]), [t[0], t[1], t[2]]);
            }
        }
        let p = |i: u32| {
            let v = mesh.vertices[i as usize].position;
            [v[0] as f64, v[1] as f64]
        };
        for (&(a, b), t1) in &owner {
            let Some(t2) = owner.get(&(b, a)) else { continue };
            let c = t1.iter().copied().find(|&v| v != a && v != b).unwrap();
            let d = t2.iter().copied().find(|&v| v != a && v != b).unwrap();
            let (pa, pb, pc, pd) = (p(a), p(b), p(c), p(d));
            let convex = cross(sub(pd, pc), sub(pa, pc)) < 0.0 && cross(sub(pd, pc), sub(pb, pc)) > 0.0;
            let s = dot(sub(pa, pc), sub(pa, pc)) + dot(sub(pb, pc), sub(pb, pc)) + dot(sub(pd, pc), sub(pd, pc));
            assert!(!convex || in_circle(pa, pb, pc, pd) <= 1e-5 * s * s, "edge {a}-{b} is not locally Delaunay");
        }
        // Edge manifold: (a, b) with no (b, a) must be a boundary edge, each used once.
        let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in mesh.indices.chunks(3) {
            for k in 0..3 {
                *edges.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        assert!(edges.values().all(|&n| n == 1), "a directed edge used twice");
        let boundary = edges.keys().filter(|&&(a, b)| !edges.contains_key(&(b, a))).count();
        let ring_edges: usize = fill_rings(contours).iter().map(Vec::len).sum();
        assert_eq!(boundary, ring_edges, "boundary edges are exactly the contour edges");
    }

    fn flat(p: &Path) -> Vec<Contour> {
        p.flatten(0.001).unwrap()
    }

    #[test]
    fn convex_and_concave_polygons_are_watertight() {
        check_fill(&flat(&Path::rect(0.0, 0.0, 4.0, 2.0)), 8.0, 1.0);
        // An L shape (concave).
        let l = Path::polygon(&[[0.0, 0.0], [3.0, 0.0], [3.0, 1.0], [1.0, 1.0], [1.0, 3.0], [0.0, 3.0]]);
        check_fill(&flat(&l), 5.0, 1.0);
        // A five-pointed star (very concave), given clockwise: orientation doesn't matter.
        let mut star: Vec<[f32; 2]> = (0..10)
            .map(|k| {
                let a = std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                let r = if k % 2 == 0 { 2.0 } else { 0.8 };
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        star.reverse();
        let contours = flat(&Path::polygon(&star));
        let ring = &contours[0].points;
        let area = area2(ring).abs() * 0.5;
        check_fill(&contours, area, 1.0);
    }

    #[test]
    fn holes_and_islands_follow_even_odd() {
        // A 10 x 10 square with a 4 x 4 hole and a 2 x 2 island inside the hole.
        let mut p = Path::rect(0.0, 0.0, 10.0, 10.0);
        p.cmds.extend(Path::rect(3.0, 3.0, 4.0, 4.0).cmds);
        p.cmds.extend(Path::rect(4.0, 4.0, 2.0, 2.0).cmds);
        check_fill(&flat(&p), 100.0 - 16.0 + 4.0, 1.0);
        // Two holes side by side and a circle hole.
        let mut q = Path::rect(0.0, 0.0, 12.0, 6.0);
        q.cmds.extend(Path::rect(1.0, 1.0, 3.0, 4.0).cmds);
        q.cmds.extend(Path::rect(5.0, 1.0, 2.0, 2.0).cmds);
        q.cmds.extend(Path::circle([9.5, 3.0], 1.5).cmds);
        let contours = flat(&q);
        let circle = area2(&contours[3].points).abs() * 0.5;
        check_fill(&contours, 72.0 - 12.0 - 4.0 - circle, 0.3);
    }

    #[test]
    fn circles_flatten_within_tolerance_and_fill_without_slivers() {
        let c = Path::circle([0.0, 0.0], 1.0).flatten(0.001).unwrap();
        assert_eq!(c.len(), 1);
        let pts = &c[0].points;
        assert!(pts.len() > 60 && pts.len() < 120, "{} points", pts.len());
        for w in pts.windows(2) {
            let mid = scale(add(w[0], w[1]), 0.5);
            assert!(1.0 - len(mid) <= 0.001 + 1e-9, "sagitta within tolerance");
        }
        // Any triangulation of n points on a circle has an angle of at most 180°/n (an inscribed
        // angle over one edge); nothing should be thinner than that.
        check_fill(&c, area2(pts) * 0.5, 0.9 * 180.0 / pts.len() as f64);
        let r = Path::rounded_rect(0.0, 0.0, 4.0, 2.0, 0.5).flatten(0.002).unwrap();
        let expected = 8.0 - (4.0 - std::f64::consts::PI) * 0.25;
        check_fill(&r, area2(&r[0].points) * 0.5, 0.3);
        assert!((area2(&r[0].points) * 0.5 - expected).abs() < 0.01);
    }

    #[test]
    fn beziers_end_where_they_should() {
        let mut p = Path::new();
        p.move_to([0.0, 0.0]).quad_to([1.0, 2.0], [2.0, 0.0]).cubic_to([3.0, -1.0], [4.0, 1.0], [5.0, 0.0]);
        let c = p.flatten(0.01).unwrap();
        let pts = &c[0].points;
        assert_eq!(pts[0], [0.0, 0.0]);
        assert!(pts.contains(&[2.0, 0.0]) && pts.last() == Some(&[5.0, 0.0]));
        // The quadratic's apex is at (1, 1): within tolerance of the polyline.
        let seg_dist = |q: P2, a: P2, b: P2| {
            let t = (dot(sub(q, a), sub(b, a)) / dot(sub(b, a), sub(b, a))).clamp(0.0, 1.0);
            len(sub(q, add(a, scale(sub(b, a), t))))
        };
        let nearest = pts.windows(2).map(|w| seg_dist([1.0, 1.0], w[0], w[1])).fold(f64::INFINITY, f64::min);
        assert!(nearest <= 0.01, "{nearest}");
        assert!(!c[0].closed);
        assert_eq!(p.flatten(0.0), Err(VectorError::NotFinite { what: "tolerance (> 0)" }));
    }

    #[test]
    fn aa_fringe_wraps_every_edge() {
        let mut mesh = ShapeMesh::default();
        fill(&flat(&Path::rect(0.0, 0.0, 1.0, 1.0)), [10, 20, 30, 255], true, &mut mesh).unwrap();
        // 4 inner + 4 outer vertices; 2 fill triangles + 4 edges x 2 fringe triangles.
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (8, 6 + 24));
        let (inner, outer) = mesh.vertices.split_at(4);
        for (i, o) in inner.iter().zip(outer) {
            assert_eq!(i.position, o.position);
            assert_eq!((i.color[3], o.color[3]), (255, 0));
            // Corner miters point outward diagonally: sqrt(2) long, half a pixel each way.
            let l = (o.normal[0].powi(2) + o.normal[1].powi(2)).sqrt();
            assert!((l - 0.5 * 2f32.sqrt()).abs() < 1e-5, "{l}");
            assert_eq!(i.normal[0], -o.normal[0]);
            let out = [o.position[0] as f64 - 0.5, o.position[1] as f64 - 0.5];
            assert!(dot(out, [o.normal[0] as f64, o.normal[1] as f64]) > 0.0, "points away from the centre");
        }
    }

    #[test]
    fn strokes_cover_their_width_with_joins_and_caps() {
        let line = Path::polyline(&[[0.0, 0.0], [2.0, 0.0]]);
        let style = StrokeStyle { width: 0.5, ..Default::default() };
        let mut mesh = ShapeMesh::default();
        stroke(&flat(&line), &style, 0.001, false, &mut mesh).unwrap();
        let area: f64 = tris(&mesh).iter().map(|t| tri_area(t).abs()).sum();
        assert!((area - 1.0).abs() < 1e-9, "butt: 2 x 0.5, got {area}");
        let mut sq = ShapeMesh::default();
        stroke(&flat(&line), &StrokeStyle { cap: LineCap::Square, ..style }, 0.001, false, &mut sq).unwrap();
        let area: f64 = tris(&sq).iter().map(|t| tri_area(t).abs()).sum();
        assert!((area - 1.25).abs() < 1e-9, "square caps add 2 x 0.25 x 0.5, got {area}");
        let mut round = ShapeMesh::default();
        stroke(&flat(&line), &StrokeStyle { cap: LineCap::Round, ..style }, 0.0001, false, &mut round).unwrap();
        let area: f64 = tris(&round).iter().map(|t| tri_area(t).abs()).sum();
        let want = 1.0 + std::f64::consts::PI * 0.0625;
        assert!((area - want).abs() < 2e-3, "round caps add a disc of radius 0.25: {area} vs {want}");
        // A right-angle corner: miter fills the square corner, bevel cuts it, round in between.
        let corner = Path::polyline(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]]);
        let area_of = |join| {
            let mut m = ShapeMesh::default();
            stroke(&flat(&corner), &StrokeStyle { join, ..style }, 0.0001, false, &mut m).unwrap();
            tris(&m).iter().map(|t| tri_area(t).abs()).sum::<f64>()
        };
        let (miter, bevel, round_j) = (area_of(LineJoin::Miter { limit: 4.0 }), area_of(LineJoin::Bevel), area_of(LineJoin::Round));
        // Union area: 2 x (2 x 0.5) minus the overlap square inside the corner, plus the outer corner.
        assert!((miter - 2.0).abs() < 1e-9, "miter {miter}");
        // The bevel cuts the outer corner's triangle: half of 0.25 x 0.25.
        assert!((bevel - (2.0 - 0.03125)).abs() < 1e-9, "bevel {bevel}");
        assert!(round_j > bevel && round_j < miter, "round {round_j}");
        // A limit below the corner's miter ratio (sqrt 2) bevels it.
        assert!((area_of(LineJoin::Miter { limit: 1.2 }) - bevel).abs() < 1e-9);
        // A closed square ring: outer 2.5^2 minus inner 1.5^2.
        let ring = flat(&Path::rect(0.0, 0.0, 2.0, 2.0));
        let mut m = ShapeMesh::default();
        stroke(&ring, &style, 0.001, false, &mut m).unwrap();
        let area: f64 = tris(&m).iter().map(|t| tri_area(t).abs()).sum();
        assert!((area - (6.25 - 2.25)).abs() < 1e-9, "ring {area}");
    }

    #[test]
    fn stroke_fringes_fade_to_zero_and_dots_show() {
        let mut m = ShapeMesh::default();
        let style = StrokeStyle { width: 0.2, color: [200, 100, 50, 255], ..Default::default() };
        stroke(&flat(&Path::polyline(&[[0.0, 0.0], [1.0, 0.0]])), &style, 0.001, true, &mut m).unwrap();
        // Butt caps with AA: fringe pair, body, body, fringe pair = 4 sections x 4 vertices.
        assert_eq!(m.vertices.len(), 16);
        let alphas: Vec<u8> = m.vertices.iter().map(|v| v.color[3]).collect();
        assert_eq!(&alphas[..8], &[0, 0, 0, 0, 0, 255, 255, 0]);
        let mut dot_mesh = ShapeMesh::default();
        let single = [Contour { points: vec![[1.0, 1.0]], closed: false }];
        stroke(&single, &StrokeStyle { cap: LineCap::Round, ..style }, 0.001, false, &mut dot_mesh).unwrap();
        assert!(!dot_mesh.is_empty(), "a round-capped point is a dot");
        let mut none = ShapeMesh::default();
        stroke(&single, &style, 0.001, false, &mut none).unwrap();
        assert!(none.is_empty(), "a butt-capped point is nothing");
    }

    /// Hostile shapes (the review's probes) fail fast with a structured error, and the biggest
    /// accepted ones stay quick.
    #[test]
    fn hostile_shapes_fail_fast_and_big_ones_stay_quick() {
        let timed = |what: &str, f: &dyn Fn() -> Result<ShapeMesh, VectorError>| {
            let t = std::time::Instant::now();
            let r = f();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            eprintln!("{what}: {:?} in {ms:.1} ms", r.as_ref().map(|m| m.vertices.len()));
            assert!(ms < 1000.0, "{what} took {ms:.0} ms");
            r
        };
        let wide = StrokeStyle { width: 100_000.0, join: LineJoin::Round, cap: LineCap::Round, color: [255; 4] };
        // A 16,384-point zigzag with a 100 km round-joined stroke at a 10 µm tolerance.
        let zig: Vec<[f32; 2]> = (0..16_384).map(|i| [i as f32, (i % 2) as f32 * 1000.0]).collect();
        let r = timed("zigzag stroke", &|| {
            let mut m = ShapeMesh::default();
            stroke(&Path::polyline(&zig).flatten(1e-5)?, &wide, 1e-5, true, &mut m).map(|()| m)
        });
        assert!(matches!(r, Err(VectorError::TooManyVertices { .. })), "{r:?}");
        // 16,384 separate dots with round caps.
        let mut dots = Path::new();
        for i in 0..16_384 {
            dots.move_to([i as f32, 0.0]);
        }
        let r = timed("dots", &|| {
            let mut m = ShapeMesh::default();
            stroke(&dots.flatten(1e-5)?, &wide, 1e-5, true, &mut m).map(|()| m)
        });
        assert!(matches!(r, Err(VectorError::TooManyVertices { .. })), "{r:?}");
        // One ring of 15 huge arcs at a tiny tolerance: the tolerance is raised to the extent.
        let mut arcs = Path::new();
        for i in 0..15 {
            arcs.arc_to([0.0, 0.0], [1e6, 1e6], (i as f32 * 24.0).to_radians(), 24f32.to_radians());
        }
        arcs.close();
        let r = timed("15 huge arcs", &|| {
            let mut m = ShapeMesh::default();
            fill(&arcs.flatten(1e-5)?, [255; 4], false, &mut m).map(|()| m)
        });
        assert!(r.is_ok_and(|m| m.vertices.len() < 4096));
        // Combs: many reflex corners. Just over the polygon cap fails; just under it is the
        // slowest fill accepted.
        let comb = |n: usize| {
            let mut pts = Vec::new();
            for i in 0..n {
                let x = i as f32;
                pts.extend([[x, 0.0], [x + 0.25, 10.0], [x + 0.5, 0.1]]);
            }
            pts.extend([[n as f32, -5.0], [0.0, -5.0]]);
            Path::polygon(&pts)
        };
        let over = comb(1400);
        let r = timed("comb over the cap", &|| {
            let mut m = ShapeMesh::default();
            fill(&over.flatten(0.01)?, [255; 4], false, &mut m).map(|()| m)
        });
        assert!(matches!(r, Err(VectorError::FillTooComplex { .. })), "{r:?}");
        let worst = comb((MAX_FILL_POLYGON - 2) / 3);
        let r = timed("comb at the cap", &|| {
            let mut m = ShapeMesh::default();
            fill(&worst.flatten(0.01)?, [255; 4], true, &mut m).map(|()| m)
        });
        assert!(r.is_ok());
        // Convex fills skip ear clipping: as many 1024-point circles as the vertex budget holds.
        let circle = Path::circle([0.0, 0.0], 100.0);
        let r = timed("128 1024-point circles (convex fills)", &|| {
            let mut m = ShapeMesh::default();
            for _ in 0..128 {
                fill(&circle.flatten(1e-3)?, [255; 4], true, &mut m)?;
            }
            Ok(m)
        });
        assert!(r.is_ok_and(|m| m.vertices.len() > 150_000));
        // Many big fills in one shape: the work budget stops the third.
        let r = timed("three combs at the cap", &|| {
            let mut m = ShapeMesh::default();
            for _ in 0..3 {
                fill(&worst.flatten(0.01)?, [255; 4], false, &mut m)?;
            }
            Ok(m)
        });
        assert!(matches!(r, Err(VectorError::FillTooComplex { .. })), "{r:?}");
        let r = timed("two combs at the cap (worst accepted fill work)", &|| {
            let mut m = ShapeMesh::default();
            for _ in 0..2 {
                fill(&worst.flatten(0.01)?, [255; 4], true, &mut m)?;
            }
            Ok(m)
        });
        assert!(r.is_ok());
        // The most round-capped dots that fit the vertex budget (convex: no ear clipping).
        let mut many = Path::new();
        for i in 0..1800 {
            many.move_to([i as f32 * 3.0e5, 0.0]);
        }
        let r = timed("1800 round dots (worst accepted stroke)", &|| {
            let mut m = ShapeMesh::default();
            stroke(&many.flatten(1e-5)?, &wide, 1e-5, true, &mut m).map(|()| m)
        });
        assert!(r.is_ok());
    }

    #[test]
    fn tessellation_is_deterministic_and_bounded() {
        let mut p = Path::circle([0.3, -0.2], 0.7);
        p.cmds.extend(Path::rect(-0.1, -0.1, 0.2, 0.2).cmds);
        let run = || {
            let mut m = ShapeMesh::default();
            let c = p.flatten(0.0005).unwrap();
            fill(&c, [1, 2, 3, 4], true, &mut m).unwrap();
            stroke(&c, &StrokeStyle { join: LineJoin::Round, cap: LineCap::Round, ..Default::default() }, 0.0005, true, &mut m).unwrap();
            m
        };
        let (a, b) = (run(), run());
        assert_eq!(a, b);
        let bits = |m: &ShapeMesh| m.vertices.iter().flat_map(|v| v.position.iter().chain(&v.normal).map(|f| f.to_bits())).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b));
        let huge = Path::circle([0.0, 0.0], 1e4);
        assert!(matches!(huge.flatten(1e-9), Ok(_) | Err(VectorError::TooManyPoints { .. })));
        let mut many = Path::new();
        for k in 0..(MAX_SHAPE_POINTS + 10) {
            many.line_to([k as f32, (k % 2) as f32]);
        }
        assert!(matches!(many.flatten(0.1), Err(VectorError::TooManyPoints { .. })));
        assert!(matches!(Path::polygon(&[[f32::NAN, 0.0]]).flatten(0.1), Err(VectorError::NotFinite { .. })));
    }
}
