//! What a recipe is built of (docs/design/FORM.md, "From what the recipe is built of"): for
//! every shape a recipe names, **its own outlines, seen along its own axes, and its own surface
//! in the finished model**. A box's are three rectangles; a ball's, one circle; a cylinder's, a
//! circle and a rectangle; a cone's, a circle and a triangle; an extruded outline's, the outline
//! and the box it is raised to; a lathe's, a circle and its profile mirrored; a column's, its
//! section and its sides closed by their caps.
//!
//! This is what a silhouette of the whole asset cannot say: which part is what. A silhouette
//! reads a ball built of forty small boxes as a ball, and a crate under a round lid as half of
//! each by the angle it is seen from; the recipe says what each was made as. The asset studio
//! reads each outline with the measure it reads silhouettes with, and weighs it by the shape's
//! surface (`specula-studio`, `built`).
//!
//! **A record's surface is measured, not reckoned: every triangle of the finest level knows
//! what it was made as ([`crate::Shape::owners`]), through every operation, and a record's
//! surface is the sum of its own triangles in the finished model** ([`gather`]). What no
//! record speaks for is [`crate::ModelExtras::made_rest_m2`].
//! - Each shape generator of a part, each operand of a boolean and each primitive of a sculpt
//!   (`sdf`) that adds to it is recorded; what a boolean or a sculpt takes away (the second
//!   operand of a `difference` and after) is not what the thing is made of, and is left out.
//!   **The wall a cut makes is its cutter's surface**, and so no record's.
//! - A boolean's every face is a piece of one operand's face and keeps what that was made as;
//!   a sculpt's triangles are each given to the primitive that made them
//!   ([`crate::sdf::primitive_of_triangles`]), and its primitives' outlines are weighed by
//!   their own triangles.
//! - The outlines are the shape's as it is generated. Its own `ops`, its part's and the
//!   recipe's `finish` then move, turn, scale, copy, bend and tint it: the outlines are not
//!   changed by them (a taper does not make a box's outline a wedge's: said in the document as
//!   a limit); the surface is, and is measured after them: three copies count three times, and
//!   a scale by `[1, 0.05, 1]` changes a box and a ball by their own factors.
//! - **Where the tie is not kept, the surface is the rest's**: a bevel's strips (its tubes');
//!   triangles of a sculpt not asked within [`crate::sdf::MAX_OWNER_BUILD_WORK`]. Where it is
//!   kept but blurred: inside one boolean, coplanar pieces of one operand, one colour and one
//!   tag are joined into one face before they are cut into triangles, and the face is its first
//!   piece's (only an operand that holds several shapes can blur so: a nested boolean with its
//!   own ops, a sculpt); a sculpt past [`crate::sdf::MAX_OWNER_ASKED`] triangles is asked at
//!   every so many. The finest level is measured before its parts are welded at 10 µm.
//! - An include's records carry the including part's path, as its errors do
//!   (`parts[1].include(gem).parts[0].shape`); a scatter's items are recorded by what was
//!   placed of each (a variant's copies carry what it was made as). One record a path.
//! - A terrain, a building and an imported mesh record nothing.

use crate::dmath;
use crate::math3::{V2, V3};
use crate::shape::Shape;

/// One shape a recipe names, as it was made.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    /// Where in the recipe (`parts[2].shape`, `parts[0].shape.of[1]`).
    pub path: String,
    /// The shape's name in the recipe (`box`, `cylinder`, `sdf`, ...).
    pub what: &'static str,
    /// Its own surface in the finished model, square metres (after every operation, and what
    /// a boolean or a scatter left of it).
    pub area_m2: f32,
    /// Its outlines along its own axes (closed, in any units: only their shape is read), each
    /// with its weight among them. At most [`MAX_VIEW_POINTS`] points each. One of no points is
    /// the weight of outlines the record had no room for ([`MAX_VIEWS`], [`MAX_RECORD_VIEWS`]):
    /// not read.
    pub views: Vec<(Vec<V2>, f32)>,
}

/// A shape a recipe names, while the model is built: where, and what.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub path: String,
    pub what: &'static str,
}

/// Outlines of one record, while the model is built, and what triangles carry
/// ([`crate::Shape::owners`]): a plain shape has one slot, a sculpt one for each primitive that
/// adds to it. What a slot weighs is its triangles' surface in the finished model.
#[derive(Clone, Debug)]
pub(crate) struct Slot {
    pub record: u32,
    pub views: Views,
}

/// Slots in one build (past them, a shape's triangles are no record's).
pub(crate) const MAX_SLOTS: usize = 8192;

/// A shape's outlines, each with its weight among them.
pub type Views = Vec<(Vec<V2>, f32)>;

/// Shapes kept for one model's facts: the ones with the most surface.
pub const MAX_MADE: usize = 64;
/// Shapes kept while a model is built (the rest add to the surface nothing speaks for).
pub(crate) const MAX_KEPT: usize = 512;
/// Points in one outline.
pub const MAX_VIEW_POINTS: usize = 128;
/// Outlines of one shape (a sculpt has several for each of its primitives).
pub const MAX_VIEWS: usize = 96;
/// Outlines kept in one model's record in all (of [`MAX_VIEW_POINTS`] each at the most: a
/// quarter of a megabyte; the shapes with the most surface keep theirs first).
pub const MAX_RECORD_VIEWS: usize = 256;

/// A shape's surface: the sum of its triangles' areas.
pub fn area(s: &Shape) -> f32 {
    let mut sum = 0.0f64;
    for t in &s.tris {
        sum += f64::from(triangle_area(s, *t));
    }
    if sum.is_finite() {
        sum as f32
    } else {
        0.0
    }
}

/// One triangle's area (0 for one that is not finite).
pub fn triangle_area(s: &Shape, t: [u32; 3]) -> f32 {
    let p = |i: u32| s.positions.get(i as usize).copied().unwrap_or([0.0; 3]);
    let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
    let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
    let n: V3 = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let x = 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

/// What a model is built of, measured: each slot's surface is the sum of the triangles of
/// `shapes` (the finished parts) that carry it, a record's the sum of its slots', and its
/// outlines are its slots' each weighed by its slot's surface (an outline an earlier slot of
/// the same record has, once). **One record a path**: a scatter item's variants (one path,
/// built once each) are one shape. A shape with no surface left (cut away, never placed) is not
/// kept. A record keeps at most [`MAX_VIEWS`] outlines, its largest slots' first, and the
/// weight of the rest as one outline of no points (not read); the [`crate::MAX_MADE`] records
/// with the most surface are kept, and at most [`MAX_RECORD_VIEWS`] outlines in all, the
/// largest records' first (one whose outlines do not all fit keeps its heaviest that do, and
/// the rest's weight as one of no points). Also the share of `surface` no kept record speaks
/// for.
pub(crate) fn gather(records: &[Record], slots: &[Slot], shapes: &[&Shape], surface: f32) -> (Vec<Made>, f32) {
    let mut area = vec![0.0f64; slots.len()];
    for s in shapes {
        for (t, &o) in s.tris.iter().zip(&s.owners) {
            if let Some(a) = area.get_mut(o as usize) {
                *a += f64::from(triangle_area(s, *t));
            }
        }
    }
    // (By path, in the order built.)
    let mut by_path: std::collections::BTreeMap<&str, (&'static str, Vec<usize>)> = std::collections::BTreeMap::new();
    for (i, slot) in slots.iter().enumerate() {
        let Some(r) = records.get(slot.record as usize) else { continue };
        if area[i].is_finite() && area[i] > 0.0 {
            by_path.entry(r.path.as_str()).or_insert((r.what, Vec::new())).1.push(i);
        }
    }
    let mut out: Vec<Made> = by_path
        .into_iter()
        .map(|(path, (what, mine))| Made { path: path.to_string(), what, area_m2: mine.iter().map(|&i| area[i]).sum::<f64>() as f32, views: outlines(&mine, slots, &area) })
        .filter(|m| m.area_m2 > 0.0)
        .collect();
    out.sort_by(|a, b| b.area_m2.total_cmp(&a.area_m2).then_with(|| a.path.cmp(&b.path)));
    out.truncate(crate::MAX_MADE);
    let mut room = MAX_RECORD_VIEWS;
    for m in &mut out {
        let mut drawn: Vec<(Vec<V2>, f32)> = Vec::new();
        let mut unread = 0.0f32;
        for v in std::mem::take(&mut m.views) {
            if v.0.is_empty() {
                unread += v.1;
            } else {
                drawn.push(v);
            }
        }
        if drawn.len() > room {
            drawn.sort_by(|a, b| b.1.total_cmp(&a.1));
            unread += drawn[room..].iter().map(|v| v.1).sum::<f32>();
            drawn.truncate(room);
        }
        room -= drawn.len();
        if unread > 0.0 {
            drawn.push((Vec::new(), unread));
        }
        m.views = drawn;
    }
    let spoken: f32 = out.iter().map(|m| m.area_m2).sum();
    (out, (surface - spoken).max(0.0))
}

/// A record's outlines from its slots (`mine`, in the order built): the largest slots' chosen
/// first while there is room, kept in the order built, each weighed by its slot's surface.
fn outlines(mine: &[usize], slots: &[Slot], area: &[f64]) -> Views {
    let mut order = mine.to_vec();
    order.sort_by(|&a, &b| area[b].total_cmp(&area[a]).then(a.cmp(&b)));
    // (Each kept outline with its weight and where it was first met.)
    let mut kept: Vec<(&Vec<V2>, f64, (usize, usize))> = Vec::new();
    let mut unread = 0.0f64;
    for &i in &order {
        let total: f64 = slots[i].views.iter().map(|v| f64::from(v.1)).sum();
        if !(total > 0.0) {
            continue;
        }
        for (j, (points, w)) in slots[i].views.iter().enumerate() {
            let weight = area[i] * f64::from(*w) / total;
            if let Some(k) = kept.iter_mut().find(|k| k.2 .0 != i && k.0 == points) {
                k.1 += weight;
            } else if kept.len() < MAX_VIEWS {
                kept.push((points, weight, (i, j)));
            } else {
                unread += weight;
            }
        }
    }
    kept.sort_by_key(|k| k.2);
    let mut views: Views = kept.into_iter().map(|(p, w, _)| (p.clone(), w as f32)).collect();
    if unread > 0.0 {
        views.push((Vec::new(), unread as f32));
    }
    views
}

fn arc(out: &mut Vec<V2>, c: V2, r: f32, from_deg: f64, sweep_deg: f64, steps: u32) {
    for i in 0..=steps {
        let (s, co) = dmath::sin_cos_deg(from_deg + sweep_deg * f64::from(i) / f64::from(steps));
        out.push([c[0] + r * co as f32, c[1] + r * s as f32]);
    }
}

/// A circle (48 sides: the measure reads it as round at any size).
pub fn circle(r: f32) -> Vec<V2> {
    ellipse(r, r)
}

/// An ellipse of half widths `rx`, `ry`.
pub fn ellipse(rx: f32, ry: f32) -> Vec<V2> {
    (0..48)
        .map(|i| {
            let (s, c) = dmath::sin_cos_deg(7.5 * f64::from(i));
            [rx * c as f32, ry * s as f32]
        })
        .collect()
}

/// A regular polygon of `n` sides (a cylinder's own section: a recipe that asks for six sides
/// gets a hexagon). Sixteen sides and more are a circle.
pub fn ngon(n: u32, r: f32) -> Vec<V2> {
    if n >= 16 {
        return circle(r);
    }
    (0..n.max(3))
        .map(|i| {
            let (s, c) = dmath::sin_cos_deg(360.0 * f64::from(i) / f64::from(n.max(3)));
            [r * c as f32, r * s as f32]
        })
        .collect()
}

/// A rectangle `w` by `h`, its corners cut off flat by `cut` (0: sharp).
pub fn rect(w: f32, h: f32, cut: f32) -> Vec<V2> {
    let (x, y) = (0.5 * w, 0.5 * h);
    let c = cut.max(0.0).min(0.45 * w.min(h));
    if c <= 0.0 {
        return vec![[-x, -y], [x, -y], [x, y], [-x, y]];
    }
    vec![[-x + c, -y], [x - c, -y], [x, -y + c], [x, y - c], [x - c, y], [-x + c, y], [-x, y - c], [-x, -y + c]]
}

/// A rectangle `w` by `h` whose corners are rounded by `r`.
pub fn rounded(w: f32, h: f32, r: f32) -> Vec<V2> {
    let (x, y) = (0.5 * w, 0.5 * h);
    let r = r.max(0.0).min(x.min(y));
    if r <= 0.0 {
        return rect(w, h, 0.0);
    }
    let mut p = Vec::new();
    arc(&mut p, [x - r, -y + r], r, -90.0, 90.0, 8);
    arc(&mut p, [x - r, y - r], r, 0.0, 90.0, 8);
    arc(&mut p, [-x + r, y - r], r, 90.0, 90.0, 8);
    arc(&mut p, [-x + r, -y + r], r, 180.0, 90.0, 8);
    p
}

/// A superellipse of half widths `hx`, `hy`: `power` 2 is the ellipse, a high one the box.
pub fn superellipse(hx: f32, hy: f32, power: f32) -> Vec<V2> {
    let e = 2.0 / power.clamp(2.0, 64.0);
    (0..64)
        .map(|i| {
            let (s, c) = dmath::sin_cos_deg(5.625 * f64::from(i));
            let side = |v: f64, half: f32| half * (v.signum() as f32) * dmath::pow(v.abs() as f32, e);
            [side(c, hx), side(s, hy)]
        })
        .collect()
}

/// What a cylinder, a frustum or a cone is from the side: half width `r0` at the bottom, `r1`
/// at the top (0: a triangle), `h` tall.
pub fn trapezoid(r0: f32, r1: f32, h: f32) -> Vec<V2> {
    let mut p = vec![[-r0, 0.0], [r0, 0.0]];
    if r1 > 1e-6 * r0.max(h) {
        p.push([r1, h]);
        p.push([-r1, h]);
    } else {
        p.push([0.0, h]);
    }
    if r0 <= 1e-6 * r1.max(h) {
        p.remove(0);
    }
    p
}

/// What a capsule is from the side: `length` from end to end, round ends of radius `r`.
pub fn stadium(length: f32, r: f32) -> Vec<V2> {
    let half = (0.5 * length - r).max(0.0);
    let mut p = Vec::new();
    arc(&mut p, [half, 0.0], r, -90.0, 180.0, 16);
    arc(&mut p, [-half, 0.0], r, 90.0, 180.0, 16);
    p
}

/// What a round cone is from the side: a ball of radius `ra` and one of `rb`, `len` apart,
/// and what lies between them.
pub fn tapered(ra: f32, rb: f32, len: f32) -> Vec<V2> {
    if len <= (ra - rb).abs() {
        return circle(ra.max(rb));
    }
    // The two tangent lines leave each ball at the same angle off the perpendicular.
    let lean = f64::from(dmath::asin(((ra - rb) / len).clamp(-1.0, 1.0)).to_degrees());
    let mut p = Vec::new();
    arc(&mut p, [len, 0.0], rb, -90.0 + lean, 180.0 - 2.0 * lean, 16);
    arc(&mut p, [0.0, 0.0], ra, 90.0 - lean, 180.0 + 2.0 * lean, 16);
    p
}

/// A profile of `[half width, height]` rows, mirrored into the outline it is from the side (a
/// lathe; a loft's or a column's stations).
pub fn mirrored(profile: &[V2]) -> Vec<V2> {
    let mut p: Vec<V2> = profile.iter().map(|q| [q[0].max(0.0), q[1]]).collect();
    p.extend(profile.iter().rev().filter(|q| q[0] > 0.0).map(|q| [-q[0], q[1]]));
    p
}

/// Rows of `[half width, height, shift]` (a loft's stations leaning by their `dx` or `dz`):
/// the outline from the side, each row's middle moved by its shift.
pub fn leaning(rows: &[[f32; 3]]) -> Vec<V2> {
    let mut p: Vec<V2> = rows.iter().map(|q| [q[2] + q[0].max(0.0), q[1]]).collect();
    p.extend(rows.iter().rev().filter(|q| q[0] > 0.0).map(|q| [q[2] - q[0], q[1]]));
    p
}

/// A closed path seen from where it spreads widest: laid on the plane of its two greatest
/// extents (a ring's circle, a square frame's square).
pub fn plan(path: &[V3]) -> Vec<V2> {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in path {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let least = (0..3).fold(0, |m, k| if ext[k] < ext[m] { k } else { m });
    let (i, j) = match least {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    };
    path.iter().map(|p| [p[i], p[j]]).collect()
}

/// An outline fit to be kept: finite, of three points or more, enclosing something, of at most
/// [`MAX_VIEW_POINTS`] points (a longer one keeps every second, third... point).
pub fn kept(points: Vec<V2>) -> Option<Vec<V2>> {
    if points.len() < 3 || points.iter().any(|p| !(p[0].is_finite() && p[1].is_finite())) {
        return None;
    }
    let stride = points.len().div_ceil(MAX_VIEW_POINTS);
    let points: Vec<V2> = if stride > 1 { points.into_iter().step_by(stride).collect() } else { points };
    let doubled: f32 = (0..points.len()).map(|i| points[i][0] * points[(i + 1) % points.len()][1] - points[(i + 1) % points.len()][0] * points[i][1]).sum();
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in &points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    (points.len() >= 3 && size > 0.0 && doubled.abs() > 1e-6 * size * size).then_some(points)
}

/// The views of a shape from outlines and weights: the ones fit to keep.
pub fn views(given: Vec<(Vec<V2>, f32)>) -> Vec<(Vec<V2>, f32)> {
    given.into_iter().filter(|(_, w)| w.is_finite() && *w > 0.0).filter_map(|(p, w)| kept(p).map(|p| (p, w))).take(MAX_VIEWS).collect()
}

/// The half extents of an outline about the middle of its box.
pub fn half_extents(points: &[V2]) -> V2 {
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    [0.5 * (hi[0] - lo[0]).max(0.0), 0.5 * (hi[1] - lo[1]).max(0.0)]
}

/// What a column is from one side ([`crate::sdf::Prim::Column`]): its sections' half widths
/// `[half width, along]` from its first end to its last, closed past each end by the cap its
/// field has there, half an ellipse as wide as the end's section and as high as `caps` (the
/// end's narrower half, which the field takes as the cap's height).
pub fn capped(rows: &[V2], caps: [f32; 2]) -> Vec<V2> {
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else { return Vec::new() };
    let mut half: Vec<V2> = Vec::with_capacity(rows.len() + 18);
    // (From the foot of the first cap, on the axis, up its quarter of an ellipse ...)
    for i in 0..=8 {
        let (s, c) = dmath::sin_cos_deg(90.0 - 11.25 * f64::from(i));
        half.push([first[0] * c as f32, first[1] - caps[0] * s as f32]);
    }
    half.extend(rows.iter().skip(1).take(rows.len().saturating_sub(2)).copied());
    // (... and over the last.)
    for i in 0..=8 {
        let (s, c) = dmath::sin_cos_deg(11.25 * f64::from(i));
        half.push([last[0] * c as f32, last[1] + caps[1] * s as f32]);
    }
    if rows.len() == 1 {
        // (One section: the two caps meet at it.)
        half.remove(9);
    }
    mirrored(&half)
}

/// An outline elongated as a sculpt's `elongate` does: `by` inserted at its middle along each
/// of its two axes (half lengths), so that what was a ball's circle is a stadium.
fn elongated(points: Vec<V2>, by: [f32; 2]) -> Vec<V2> {
    if by == [0.0; 2] {
        return points;
    }
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in &points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let mid = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])];
    let push = |v: f32, m: f32, e: f32| if v > m { v + e } else if v < m { v - e } else { v };
    points.into_iter().map(|p| [push(p[0], mid[0], by[0]), push(p[1], mid[1], by[1])]).collect()
}

/// One outline of a sculpt's primitive while the tree is walked: the outline, the two of the
/// tree's axes it lies along (so that a stretch or an elongation further up is put on the right
/// ones) and its share of the primitive's outlines.
struct Seen {
    points: Vec<V2>,
    axes: (usize, usize),
    weight: f32,
}

/// The axis a segment from `a` to `b` runs on the most, and one other (the lower of the two
/// left, Y before Z for one along X).
fn along(a: V3, b: V3) -> (usize, usize) {
    let d = crate::math3::sub(b, a);
    let k = (0..3).fold(0, |m, i| if d[i].abs() > d[m].abs() { i } else { m });
    (k, if k == 1 { 0 } else { 1 })
}

/// The two axes other than `k`, the lower first.
fn across(k: usize) -> (usize, usize) {
    match k {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

/// A sculpt's outlines (docs/design/FORM.md, "From what the recipe is built of"): for each of
/// its primitives, in the order the tree's are met depth first, its outlines as it is from its
/// own sides with their shares among them, or none for one that does not add to it (what a
/// difference takes away, its second child and after, and a half space, which has no outline).
/// What each primitive weighs is the surface its own triangles have in the finished model
/// ([`crate::sdf::primitive_of_triangles`], [`gather`]). A node's own stretch and elongation and
/// every group's above it widen its outlines along the axes they lie on (a round cone's and a
/// column's along the axis they run on the most); a node's other modifiers (a turn, a twist, a
/// bend, a rounding, a shell) do not change them.
pub(crate) fn of_sculpt(node: &crate::sdf::SdfNode) -> Vec<Option<Views>> {
    let mut out: Vec<Option<Vec<Seen>>> = Vec::new();
    walk(node, true, &mut out);
    out.into_iter().map(|seen| seen.map(|s| s.into_iter().map(|s| (s.points, s.weight)).collect())).collect()
}

fn walk(node: &crate::sdf::SdfNode, adds: bool, out: &mut Vec<Option<Vec<Seen>>>) {
    use crate::sdf::{Combine, Kind, Prim};
    let s = node.m.stretch;
    let e = node.m.elongate;
    // (A node's stretch and elongation put on outlines along its own axes.)
    let widen = |seen: &mut Seen| {
        let (i, j) = seen.axes;
        let points = std::mem::take(&mut seen.points);
        seen.points = elongated(points, [e[i], e[j]]).into_iter().map(|p| [p[0] * s[i], p[1] * s[j]]).collect();
    };
    match &node.kind {
        Kind::Prim(prim) => {
            if !adds {
                out.push(None);
                return;
            }
            let mut mine: Vec<Seen> = Vec::new();
            let mut view = |points: Vec<V2>, axes: (usize, usize), weight: f32| mine.push(Seen { points, axes, weight });
            match *prim {
                Prim::Sphere { radius } => {
                    view(circle(radius), (0, 1), 1.0 / 3.0);
                    view(circle(radius), (0, 2), 1.0 / 3.0);
                    view(circle(radius), (2, 1), 1.0 / 3.0);
                }
                Prim::Ellipsoid { radii } => {
                    view(ellipse(radii[0], radii[1]), (0, 1), 1.0 / 3.0);
                    view(ellipse(radii[0], radii[2]), (0, 2), 1.0 / 3.0);
                    view(ellipse(radii[2], radii[1]), (2, 1), 1.0 / 3.0);
                }
                Prim::Box { half, round } => {
                    view(rounded(2.0 * half[0], 2.0 * half[1], round), (0, 1), 1.0 / 3.0);
                    view(rounded(2.0 * half[0], 2.0 * half[2], round), (0, 2), 1.0 / 3.0);
                    view(rounded(2.0 * half[2], 2.0 * half[1], round), (2, 1), 1.0 / 3.0);
                }
                Prim::Rounded { half, power, disc } => {
                    if disc {
                        view(circle(half[0]), (0, 2), 1.0 / 3.0);
                    } else {
                        view(superellipse(half[0], half[2], power), (0, 2), 1.0 / 3.0);
                    }
                    view(superellipse(half[0], half[1], power), (0, 1), 1.0 / 3.0);
                    view(superellipse(half[2], half[1], power), (2, 1), 1.0 / 3.0);
                }
                Prim::Cylinder { radius, radius_top, half_height, round } => {
                    view(circle(radius.max(radius_top)), (0, 2), 1.0 / 3.0);
                    // (Its `corner` rounds the two rims: from the side, the corners of its
                    // trapezoid, by as much.)
                    let side = trapezoid(radius, radius_top, 2.0 * half_height);
                    view(if round > 0.0 && radius == radius_top { rounded(2.0 * radius, 2.0 * half_height, round) } else { side }, (0, 1), 2.0 / 3.0);
                }
                Prim::Capsule { radius, half_height } => {
                    view(circle(radius), (0, 2), 1.0 / 3.0);
                    // (Along Y: the stadium stands on its end.)
                    view(stadium(2.0 * (half_height + radius), radius).into_iter().map(|p| [p[1], p[0]]).collect(), (0, 1), 2.0 / 3.0);
                }
                Prim::Torus { radius, tube } => {
                    view(circle(radius + tube), (0, 2), 1.0 / 3.0);
                    view(stadium(2.0 * (radius + tube), tube), (0, 1), 2.0 / 3.0);
                }
                Prim::RoundCone { a, b, ra, rb } => {
                    let len = crate::math3::length(crate::math3::sub(b, a));
                    // (Laid along the axis it runs on the most: its length along that axis, its
                    // width along another.)
                    let (k, w) = along(a, b);
                    view(circle(ra.max(rb)), across(k), 1.0 / 3.0);
                    view(tapered(ra, rb, len), (k, w), 2.0 / 3.0);
                }
                Prim::Column { a, b, side, sections, count, square } => {
                    let len = crate::math3::length(crate::math3::sub(b, a));
                    let rows = &sections[..(count as usize).clamp(1, sections.len())];
                    let (broad, deep) = rows.iter().fold((0.0f32, 0.0f32), |m, r| (m.0.max(r[1]), m.1.max(r[2])));
                    let (first, last) = (rows[0], rows[rows.len() - 1]);
                    // (Each end closes in a cap as high as its section there is narrow.)
                    let caps = [first[1].min(first[2]), last[1].min(last[2])];
                    // (Along the axis it runs on the most; its breadth toward the axis its
                    // `side` points along the most of the two left, its depth along the other.)
                    let (k, _) = along(a, b);
                    let (p, q) = across(k);
                    let (bx, dx) = if side[p].abs() >= side[q].abs() { (p, q) } else { (q, p) };
                    view(superellipse(broad, deep, square), (bx, dx), 1.0 / 3.0);
                    view(capped(&rows.iter().map(|r| [r[1], r[0] * len]).collect::<Vec<V2>>(), caps), (bx, k), 1.0 / 3.0);
                    view(capped(&rows.iter().map(|r| [r[2], r[0] * len]).collect::<Vec<V2>>(), caps), (dx, k), 1.0 / 3.0);
                }
                Prim::HalfSpace { .. } => {}
            }
            for seen in &mut mine {
                widen(seen);
            }
            out.push((!mine.is_empty()).then_some(mine));
        }
        Kind::Group { op, kids, .. } => {
            let before = out.len();
            for (i, kid) in kids.iter().enumerate() {
                walk(kid, adds && !(*op == Combine::Difference && i > 0), out);
            }
            // (A group's own stretch and elongation bear on all it holds, each outline along
            // its own axes.)
            for seen in out.iter_mut().skip(before).flatten().flatten() {
                widen(seen);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doubled_area(p: &[V2]) -> f32 {
        (0..p.len()).map(|i| p[i][0] * p[(i + 1) % p.len()][1] - p[(i + 1) % p.len()][0] * p[i][1]).sum()
    }

    /// Each outline is what it says: a closed polygon of the right size and area, wound one
    /// way, kept only when it encloses something.
    #[test]
    fn the_outlines_are_what_they_say() {
        let pi = std::f32::consts::PI;
        let near = |a: f32, b: f32, rel: f32| (a - b).abs() <= rel * b.abs();
        assert!(near(0.5 * doubled_area(&circle(2.0)), pi * 4.0, 0.01));
        assert!(near(0.5 * doubled_area(&ellipse(3.0, 1.0)), pi * 3.0, 0.01));
        assert_eq!(ngon(6, 1.0).len(), 6);
        assert_eq!(ngon(16, 1.0).len(), 48, "sixteen sides are a circle");
        assert!(near(0.5 * doubled_area(&rect(2.0, 1.0, 0.0)), 2.0, 1e-6));
        assert!(near(0.5 * doubled_area(&rect(2.0, 1.0, 0.1)), 2.0 - 4.0 * 0.005, 1e-5), "four corners cut by a tenth");
        assert!(near(0.5 * doubled_area(&rounded(2.0, 1.0, 0.25)), 2.0 - (4.0 - pi) * 0.0625, 0.01));
        assert!(near(0.5 * doubled_area(&superellipse(1.0, 1.0, 2.0)), pi, 0.01), "power 2 is the ellipse");
        assert!(near(0.5 * doubled_area(&superellipse(1.0, 1.0, 64.0)), 4.0, 0.06), "a high power is nearly the box");
        assert_eq!(trapezoid(1.0, 0.0, 2.0).len(), 3, "a cone from the side is a triangle");
        assert_eq!(trapezoid(1.0, 0.5, 2.0).len(), 4);
        assert!(near(0.5 * doubled_area(&trapezoid(1.0, 0.5, 2.0)), 3.0, 1e-6));
        assert!(near(0.5 * doubled_area(&stadium(4.0, 1.0)), 4.0 + pi, 0.01), "a rectangle of 2 by 2 and two half discs");
        assert!(near(0.5 * doubled_area(&tapered(1.0, 1.0, 2.0)), 4.0 + pi, 0.01), "two equal balls: a stadium");
        assert_eq!(tapered(1.0, 0.2, 0.5).len(), 48, "one ball inside the other: a circle");
        let cone = tapered(1.0, 0.25, 3.0);
        assert!(cone.iter().all(|p| p[0] >= -1.0 - 1e-4 && p[0] <= 3.25 + 1e-4 && p[1].abs() <= 1.0 + 1e-4));
        let vase = mirrored(&[[0.0, 0.0], [1.0, 0.0], [0.5, 1.0], [0.8, 2.0], [0.0, 2.0]]);
        assert_eq!(vase.len(), 8, "points on the axis are not mirrored");
        assert!(doubled_area(&vase).abs() > 1.0);
        // What is kept: finite, enclosing something, of a bounded length.
        assert!(kept(vec![[0.0, 0.0], [1.0, 0.0]]).is_none());
        assert!(kept(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]).is_none(), "a line encloses nothing");
        assert!(kept(vec![[0.0, 0.0], [f32::NAN, 0.0], [0.0, 1.0]]).is_none());
        let long: Vec<V2> = (0..1000).map(|i| { let (s, c) = dmath::sin_cos_deg(0.36 * f64::from(i)); [c as f32, s as f32] }).collect();
        // (A thousand points are kept as every eighth: at most 128.)
        assert!(kept(long).is_some_and(|p| p.len() == 125 && p.len() <= MAX_VIEW_POINTS));
        assert_eq!(views(vec![(circle(1.0), 1.0), (circle(1.0), 0.0), (vec![[0.0, 0.0]], 1.0)]).len(), 1);
        assert_eq!(half_extents(&rect(2.0, 1.0, 0.0)), [1.0, 0.5]);
        // A shape's surface.
        let cube = Shape::cuboid([1.0, 2.0, 3.0], 0.0).unwrap();
        assert!(near(area(&cube), 22.0, 1e-4), "{}", area(&cube));
    }
}
