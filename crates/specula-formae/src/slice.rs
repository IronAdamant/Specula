//! Layered: a solid cut by planes (two and a half dimensions, the third sense: what sprite
//! stacking draws, and what a cross-section is).
//!
//! [`slice`] cuts a built [`Mesh`] by a [`Plane`] and answers the outline of the cut: its rings
//! in the plane's own frame ([`Plane::frame`]), each region an [`Outline`] (its outer ring
//! counter-clockwise, its holes clockwise, seen from the side the plane's normal points to).
//! [`layers`] cuts it at `count` evenly spaced heights along an axis.
//!
//! **How a cut is made, and why its rings close.** Each point of the mesh is above the plane
//! (its signed distance is 0 or more) or below it; no point is on it. A triangle with points on
//! both sides is crossed along a segment between two of its edges, each edge's crossing point
//! computed from the edge's own ends, always from its point above (so the two triangles that
//! share an edge place its crossing at the same bits). Within a triangle the segment runs from
//! the edge where its outline leaves the side above to the edge where it comes back (so the solid
//! is on its left seen from above the plane: along `n × N` for the triangle's outward normal `N`).
//! Every crossed edge is crossed by its two triangles, once leaving and once coming back, so the
//! segments join edge to edge into closed rings, exactly; where a mesh point lies on the plane
//! (taken as above), crossings fall on it (or within rounding of it) and a ring visits it once for each edge through it,
//! and those repeats are left out.
//!
//! **Regions.** Rings of a closed surface that does not cross itself do not cross each other. A
//! ring inside an even number of others is an outer ring and turns counter-clockwise; inside an
//! odd number, a hole of the ring just outside it, clockwise. A cut whose rings break that rule
//! comes from a mesh that is not closed or crosses itself: refused as data (`slice_not_sound`).
//! A ring that falls to fewer than three points, or to no area (a plane through a cone's apex),
//! is left out.
//!
//! **Layers' error.** The layers sit at the middles of `count` equal spans of the solid's
//! extent, so the sum of their areas times the span is the midpoint rule for the volume: its
//! error falls as the square of the span (for a ball of radius `r` cut into `N` layers it is
//! exactly `2 π r³ / (3 N²)` over the true volume).

use crate::error::Error;
use crate::maths::*;
use crate::mesh::Mesh;
use crate::outline::{signed_area, Outline};

/// A plane: the points `p` with `normal · p = offset` (the normal is taken as a unit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Its normal.
    pub normal: P3,
    /// Its distance from the origin along the normal (over the normal's length).
    pub offset: f64,
}

/// An axis of the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// +X.
    X,
    /// +Y.
    Y,
    /// +Z.
    Z,
}

impl Axis {
    fn unit(self) -> P3 {
        match self {
            Axis::X => [1.0, 0.0, 0.0],
            Axis::Y => [0.0, 1.0, 0.0],
            Axis::Z => [0.0, 0.0, 1.0],
        }
    }
}

impl Plane {
    /// The plane square to `axis` at `height` along it.
    pub fn at(axis: Axis, height: f64) -> Plane {
        Plane { normal: axis.unit(), offset: height }
    }
    /// The plane's own frame `(u, v)`, so that `(u, v, normal)` is right-handed and a ring
    /// counter-clockwise in `(u, v)` is counter-clockwise seen from the side the normal points
    /// to: for +Y (a horizontal cut) `u = +x, v = −z` (as a plane figure is raised); for +Z
    /// `u = +x, v = +y`; for +X `u = −z, v = +y` (as the front and side views draw).
    pub fn frame(&self) -> (P3, P3) {
        let n = norm3(self.normal);
        let u = if n[1].abs() >= 0.9 { norm3(sub3([1.0, 0.0, 0.0], scale3(n, n[0]))) } else { norm3(cross3([0.0, 1.0, 0.0], n)) };
        (u, cross3(n, u))
    }
}

/// A cut: its plane, its rings (in the plane's frame) and its regions.
#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    /// The plane.
    pub plane: Plane,
    /// Every ring, as found.
    pub rings: Vec<Vec<Pt>>,
    /// Each outer ring with its holes, the largest first.
    pub regions: Vec<Outline>,
}

impl Slice {
    /// The area of the cut (the regions' areas, holes taken out).
    pub fn area(&self) -> f64 {
        self.rings.iter().map(|r| signed_area(r)).sum()
    }
}

/// The most layers [`layers`] cuts.
pub const MAX_LAYERS: u32 = 4096;
/// The most crossings of triangles and layers [`layers`] works through.
pub const MAX_WORK: u64 = 50_000_000;

/// Cuts `mesh` (a built solid: closed, wound outward, not crossing itself) by `plane`.
pub fn slice(mesh: &Mesh, plane: Plane) -> Result<Slice, Error> {
    check_plane(&plane)?;
    let t: Vec<[u32; 3]> = mesh.triangles().into_iter().map(|(_, t)| t).collect();
    let all: Vec<usize> = (0..t.len()).collect();
    cut(mesh, &t, &all, plane)
}

fn check_plane(plane: &Plane) -> Result<(), Error> {
    let l = len3(plane.normal);
    if !(l.is_finite() && l > 0.0) {
        return Err(Error::Constraint { form: "slice", rule: "normal_of_no_length", message: "the plane's normal must be a finite direction".into(), values: vec![("length", l)] });
    }
    if !plane.offset.is_finite() {
        return Err(Error::OutOfRange { form: "slice", param: "offset", value: plane.offset, min: -f64::MAX, max: f64::MAX });
    }
    Ok(())
}

type Edge = (u32, u32);

fn cut(mesh: &Mesh, t: &[[u32; 3]], which: &[usize], plane: Plane) -> Result<Slice, Error> {
    let n = norm3(plane.normal);
    let off = plane.offset / len3(plane.normal);
    let d = |i: u32| dot3(n, mesh.points[i as usize]) - off;
    let above = |i: u32| d(i) >= 0.0;
    // The crossing point of an edge, from its point above.
    let cross_at = |e: Edge| -> P3 {
        let (a, b) = if above(e.0) { (e.0, e.1) } else { (e.1, e.0) };
        let (da, db) = (d(a), d(b));
        let (pa, pb) = (mesh.points[a as usize], mesh.points[b as usize]);
        add3(pa, scale3(sub3(pb, pa), da / (da - db)))
    };
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    // Each crossed triangle's segment: from the edge where its outline leaves the side above to
    // the edge where it comes back.
    let mut segs: Vec<(Edge, Edge)> = Vec::new();
    for &ti in which {
        let v = t[ti];
        let up = v.map(above);
        if up.iter().all(|x| *x) || up.iter().all(|x| !*x) {
            continue;
        }
        let (mut leave, mut back) = (None, None);
        for k in 0..3 {
            let (a, b) = (v[k], v[(k + 1) % 3]);
            match (up[k], up[(k + 1) % 3]) {
                (true, false) => leave = Some(key(a, b)),
                (false, true) => back = Some(key(a, b)),
                _ => {}
            }
        }
        if let (Some(x), Some(y)) = (leave, back) {
            segs.push((x, y));
        }
    }
    segs.sort_unstable();
    let (u, w) = plane.frame();
    // Points of a ring within rounding of the one before are one point (a plane through mesh
    // points meets their edges at 1e-17 of them, not at their bits).
    let same = 1e-12 * mesh.reach();
    let mut rings: Vec<Vec<Pt>> = Vec::new();
    let mut used = vec![false; segs.len()];
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        let mut ring: Vec<Pt> = Vec::new();
        let mut at = start;
        let mut closed = false;
        for _ in 0..=segs.len() {
            used[at] = true;
            let (x, y) = segs[at];
            let p = cross_at(x);
            let q = [dot3(u, p), dot3(w, p)];
            if ring.last().is_none_or(|l| dist(*l, q) > same) {
                ring.push(q);
            }
            if y == segs[start].0 {
                closed = true;
                break;
            }
            match segs.binary_search_by(|s| s.0.cmp(&y)) {
                Ok(k) if !used[k] => at = k,
                _ => break,
            }
        }
        if !closed {
            return Err(not_sound("a ring of the cut does not close: the mesh is not closed"));
        }
        while ring.len() > 1 && dist(ring[0], ring[ring.len() - 1]) <= same {
            ring.pop();
        }
        if ring.len() >= 3 && signed_area(&ring) != 0.0 {
            rings.push(ring);
        }
    }
    let regions = regions(&rings)?;
    // Each region is held to what an outline of this library is (`Outline::validate`): a cut of a
    // needle can have an edge under a billionth of its size, or two parts the floor apart where
    // the plane grazes the surface; refused as data, as a figure's build would be.
    for (i, r) in regions.iter().enumerate() {
        match r.validate() {
            Ok(()) => {}
            Err(crate::outline::Fault::ShortEdge { length, .. }) => return Err(Error::Constraint { form: "slice", rule: "edge_under_floor", message: "an edge of the cut is no longer than a billionth of the cut's size: the solid's parts are too far apart in size for a well-made outline (a needle cut across)".into(), values: vec![("region", i as f64), ("edge", length), ("size", r.size())] }),
            Err(crate::outline::Fault::Gap { gap, .. }) => return Err(Error::Constraint { form: "slice", rule: "edge_under_floor", message: "two edges of the cut come no farther apart than a billionth of its size (the plane grazes the surface, or the solid is that thin there)".into(), values: vec![("region", i as f64), ("gap", gap), ("size", r.size())] }),
            Err(_) => return Err(not_sound("a region of the cut is not a well-made outline")),
        }
    }
    Ok(Slice { plane, rings, regions })
}

fn not_sound(why: &str) -> Error {
    Error::Constraint { form: "slice", rule: "slice_not_sound", message: format!("{why} (or it crosses itself): a cut of it is not a well-made outline"), values: Vec::new() }
}

/// Whether `p` is inside the ring (by the crossings of a ray to +x).
fn inside(p: Pt, r: &[Pt]) -> bool {
    let mut c = false;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            c = !c;
        }
    }
    c
}

/// The rings grouped: each outer ring (inside an even number of others) with the holes just
/// inside it. (Each ring's first point is tested against the larger rings: O(n²) in rings, of
/// which a cut has few.)
fn regions(rings: &[Vec<Pt>]) -> Result<Vec<Outline>, Error> {
    let n = rings.len();
    let areas: Vec<f64> = rings.iter().map(|r| signed_area(r)).collect();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut depth = vec![0usize; n];
    for i in 0..n {
        let p = rings[i][0];
        for j in 0..n {
            if i != j && areas[j].abs() > areas[i].abs() && inside(p, &rings[j]) {
                depth[i] += 1;
                if parent[i].is_none_or(|b| areas[j].abs() < areas[b].abs()) {
                    parent[i] = Some(j);
                }
            }
        }
    }
    let mut out: Vec<(f64, Outline)> = Vec::new();
    for i in 0..n {
        let outer = depth[i].is_multiple_of(2);
        if outer != (areas[i] > 0.0) {
            return Err(not_sound("a ring of the cut turns the wrong way for how deep it lies"));
        }
        if outer {
            let mut o = vec![rings[i].clone()];
            o.extend((0..n).filter(|&j| parent[j] == Some(i) && !depth[j].is_multiple_of(2)).map(|j| rings[j].clone()));
            out.push((areas[i], Outline { rings: o, chord_error: 0.0 }));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(out.into_iter().map(|x| x.1).collect())
}

/// `count` cuts (1 to [`MAX_LAYERS`]) square to `axis`, at the middles of equal spans of the
/// solid's extent along it, lowest first. Each triangle is visited for the layers its extent
/// spans, so the work is near linear in the triangles and the crossings; past [`MAX_WORK`] it is
/// refused before any cut is made.
pub fn layers(mesh: &Mesh, axis: Axis, count: u32) -> Result<Vec<Slice>, Error> {
    if !(1..=MAX_LAYERS).contains(&count) {
        return Err(Error::OutOfRange { form: "layers", param: "count", value: count as f64, min: 1.0, max: MAX_LAYERS as f64 });
    }
    let k = match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    };
    let Some((lo, hi)) = mesh.bounds() else { return Ok(Vec::new()) };
    let (a, b) = (lo[k], hi[k]);
    let step = (b - a) / count as f64;
    let t: Vec<[u32; 3]> = mesh.triangles().into_iter().map(|(_, t)| t).collect();
    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); count as usize];
    if step > 0.0 {
        let mut spans: Vec<(usize, usize, usize)> = Vec::with_capacity(t.len());
        let mut work = 0u64;
        for (ti, tr) in t.iter().enumerate() {
            let ys = tr.map(|v| mesh.points[v as usize][k]);
            let (y0, y1) = (ys[0].min(ys[1]).min(ys[2]), ys[0].max(ys[1]).max(ys[2]));
            // The layers whose heights `a + step (i + 1/2)` lie in [y0, y1], one wider each way.
            let first = (floor((y0 - a) / step - 0.5).max(0.0) as usize).min(count as usize - 1);
            let last = (ceil((y1 - a) / step - 0.5).max(0.0) as usize).min(count as usize - 1);
            work += (last - first + 1) as u64;
            spans.push((ti, first, last));
        }
        if work > MAX_WORK {
            return Err(Error::Constraint { form: "layers", rule: "layers_too_many", message: "the cuts would cross more triangles than the work ceiling allows: fewer layers, or a coarser build".into(), values: vec![("crossings", work as f64), ("max_work", MAX_WORK as f64), ("count", count as f64)] });
        }
        for (ti, f, l) in spans {
            for layer in &mut by_layer[f..=l] {
                layer.push(ti);
            }
        }
    }
    (0..count as usize).map(|i| cut(mesh, &t, &by_layer[i], Plane::at(axis, a + step * (i as f64 + 0.5)))).collect()
}
