//! Every form's **distance field** (stage 5a): for a form and its arguments, the signed distance
//! at a point (negative inside), whether it is exact or a bound on each side, a box that holds
//! the solid, and what a sample costs, by count. The proofs it rests on are
//! `docs/design/DISTANCE_FIELDS.md`'s (cited by number below); what that page does not prove is
//! proved in `docs/design/FORMAE.md`, "Distance fields".
//!
//! What each form gets (the table in FORMAE.md is generated from [`table`] and held by a test):
//!
//! - **A plane figure** ([`plane`]): its distance to its own pieces, lines and arcs exactly; a
//!   parametric piece by its samples at the asked detail (the field is then exact for the sampled
//!   outline and within twice the sampling's error of the true one: 4.4). The sign is the parity of
//!   a ray's crossings (see [`Plane2`]), right at a vertex, on an edge and for several rings.
//! - **A figure raised as a slab** (`raise::slab`, `ops::prism_of`, every table solid that is an
//!   extrusion): exact (4.1). **A relief**: the union of its plate and its raised figure, exact
//!   outside and a bound inside (3.2).
//! - **A figure turned** (`ops::revolve_of`, every solid of revolution of the table): exact, from
//!   the figure's distance with the stretch of axis it touches left out of its surface, which is
//!   the distance of the figure with its mirror image (4.2: the mirror's own pieces are never
//!   nearer to a point on this side of the axis than the pieces they mirror, so only the axis
//!   stretch, which is inside the mirrored figure, is left out). Turned through part of a turn: its
//!   meeting with the wedge of the turn (3.3: exact inside for a wedge up to a half turn, a bound
//!   outside; a bound on both sides past a half turn). On an ellipse (an ellipsoid, an elliptic
//!   cylinder): the turned field scaled unevenly, a bound on both sides (2.5).
//! - **A sphere built any way** (geodesic, cube-sphere): the sphere's own distance, exact. **The
//!   rounded box**: the inner box's distance less the radius, exact on both sides (FORMAE.md).
//!   **The bicylinder**: the meeting of its two cylinders, exact inside, a bound outside (3.3).
//! - **Any other solid**: its build at the asked detail. Where that is convex, the distance to its
//!   nearest face, edge or corner, exact on both sides at a cost by its faces; otherwise the
//!   distance to its nearest triangle with the sign of its winding number, exact on both sides
//!   for the build at a cost by its triangles (at most [`MAX_MESH_TRIANGLES`]). A flat-faced solid's
//!   build is the solid; a curved one's is within its chord error ([`Truth`]).
//! - **A curve** has no inside: refused as data, naming its band.
//!
//! Every field here is 1-Lipschitz (each is a distance, or built from distances by the lesser,
//! the greater and an even or uneven scale: 2.1 to 2.5, 3.1), so it never exceeds the distance
//! to its own surface (2.3). Nothing here builds anything at a sample: the work is done when the
//! field is made, and a sample looks at a fixed list of pieces.

use crate::detail::{DETAIL_MAX, DETAIL_MIN};
use crate::error::Error;
use crate::geom::{Geom, Seg};
use crate::maths::{atan2, sin_cos};
use crate::mesh::Mesh;
use crate::ops::Solid;
use crate::spec::Spec;
use crate::table::Kind;
use crate::Built;

/// A point in the plane.
pub type Pt = [f64; 2];
/// A point in space.
pub type P3 = [f64; 3];

/// The most triangles (or a convex build's faces) a field of a build looks at for a sample.
pub const MAX_MESH_TRIANGLES: u64 = 4096;
/// The most pieces a plane figure's field looks at for a sample (its lines and arcs, a
/// parametric piece's samples).
pub const MAX_PLANE_PIECES: u64 = 16_384;

/// How a field stands on one side of its surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The signed distance itself.
    Exact,
    /// A bound: the right sign, never more than the distance in size, and possibly less.
    Bound,
}

/// How a field stands outside and inside its solid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sides {
    /// Outside the solid.
    pub outside: Side,
    /// Inside it.
    pub inside: Side,
}

const EXACT: Sides = Sides { outside: Side::Exact, inside: Side::Exact };
const OUT_EXACT: Sides = Sides { outside: Side::Exact, inside: Side::Bound };
const IN_EXACT: Sides = Sides { outside: Side::Bound, inside: Side::Exact };
const BOUND: Sides = Sides { outside: Side::Bound, inside: Side::Bound };

/// What solid a field is the field of, against the form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Truth {
    /// The form itself.
    Form,
    /// The form sampled at `detail` (its parametric pieces, or its whole build), whose surface is
    /// within `error` metres of the form's: the field is within `2 error` of the form's own, and
    /// within `error` where the two have the same sign (DISTANCE_FIELDS.md 4.4). `bound`: whether
    /// `error` is a bound or the library's estimate (a parametric piece's chord error is an
    /// estimate).
    Sampled {
        /// The detail sampled at.
        detail: u32,
        /// How far the sampled surface is from the form's.
        error: f64,
        /// Whether `error` is a bound.
        bound: bool,
    },
}

/// What a sample costs, by count: the pieces (lines, arcs, faces, edges, triangles) looked at,
/// and the square roots, trigonometric calls and powers taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    /// Pieces looked at.
    pub pieces: u64,
    /// Square roots taken.
    pub roots: u64,
    /// Trigonometric calls (sines, cosines, arc tangents).
    pub trig: u64,
    /// Powers and logarithms (none of these fields takes one).
    pub powers: u64,
}

impl Cost {
    fn add(&mut self, o: Cost) {
        self.pieces += o.pieces;
        self.roots += o.roots;
        self.trig += o.trig;
        self.powers += o.powers;
    }
    /// Whether every count of `self` is at most `o`'s.
    pub fn within(&self, o: &Cost) -> bool {
        self.pieces <= o.pieces && self.roots <= o.roots && self.trig <= o.trig && self.powers <= o.powers
    }
}

// ---------------------------------------------------------------- the plane

/// A piece of a plane figure's boundary.
#[derive(Clone, Debug)]
enum Part {
    /// A straight piece; `edge`: whether it is surface (a stretch of axis a turned figure touches
    /// is not).
    Line { a: Pt, b: Pt, edge: bool },
    /// An arc of the circle of radius `r` from `e0` to `e1`, turning by `sweep` (counter-clockwise
    /// when positive); `u0`, `u1`: the unit directions from its centre to its ends; `from`: the
    /// end of the piece before it in its ring, where its chord starts for the parity (two pieces'
    /// ends, each placed from its own centre, can differ in their last bits, and a ray through
    /// the gap would count one chord and not the other).
    Arc { r: f64, sweep: f64, e0: Pt, e1: Pt, u0: Pt, u1: Pt, from: Pt },
}

fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: Pt, b: Pt) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: Pt, b: Pt) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// The side of the line through `a` and `b` that `p` is on, as a point moved by `ε` along +x and
/// then `ε²` along +y would be (so it is never 0: the same tie-break as the crossing count's).
fn side(a: Pt, b: Pt, p: Pt) -> f64 {
    let d = sub(b, a);
    let o = cross(d, sub(p, a));
    if o != 0.0 {
        o
    } else if d[1] != 0.0 {
        -d[1]
    } else {
        d[0]
    }
}

/// Whether a ray from `p` along +x crosses the segment from `a` to `b`, counted half-open in
/// height (a vertex at `p`'s height belongs to the piece that goes up from it) and decided by
/// [`side`], so that a point on the segment counts as just to its right.
fn crosses(a: Pt, b: Pt, p: Pt) -> bool {
    if a[1] <= p[1] && p[1] < b[1] {
        side(a, b, p) > 0.0
    } else if b[1] <= p[1] && p[1] < a[1] {
        side(a, b, p) < 0.0
    } else {
        false
    }
}

fn seg_d2(a: Pt, b: Pt, p: Pt) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 { (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let q = [a[0] + ab[0] * t - p[0], a[1] + ab[1] * t - p[1]];
    dot(q, q)
}

impl Part {
    /// The squared distance from `p` (an arc's in-range distance taken with one root).
    fn d2(&self, p: Pt, c: &mut Cost) -> f64 {
        match *self {
            Part::Line { a, b, .. } => seg_d2(a, b, p),
            Part::Arc { r, sweep, e0, e1, u0, u1, .. } => {
                let (w0, w1) = (sub(p, e0), sub(p, e1));
                // (Whether p's direction from the centre lies in the arc's sweep: v = w0 + r u0,
                // so cross(u0, v) = cross(u0, w0) and cross(v, u1) = cross(w1, u1), in numbers
                // the size of the arc and not of its radius.)
                let (s, t) = if sweep >= 0.0 { (cross(u0, w0), cross(w1, u1)) } else { (cross(u1, w1), cross(w0, u0)) };
                let inside = if sweep.abs() <= std::f64::consts::PI { s >= 0.0 && t >= 0.0 } else { !(s < 0.0 && t < 0.0) };
                if inside {
                    // |v|² − r² = |w0|² + 2 r w0·u0, without the cancellation of a far centre.
                    let q = dot(w0, w0) + 2.0 * r * dot(w0, u0);
                    c.roots += 1;
                    let d = q / ((r * r + q).max(0.0).sqrt() + r);
                    d * d
                } else {
                    dot(w0, w0).min(dot(w1, w1))
                }
            }
        }
    }
    /// The parity this piece adds to a ray from `p` along +x: a line's crossing; an arc's chord's
    /// crossing, and one more when `p` is in the arc's segment (between its chord and itself).
    fn parity(&self, p: Pt) -> bool {
        match *self {
            Part::Line { a, b, .. } => crosses(a, b, p),
            Part::Arc { r, sweep, e0, e1, u0, from, .. } => {
                let w0 = sub(p, e0);
                let q = dot(w0, w0) + 2.0 * r * dot(w0, u0);
                // (The arc lies to the right of its chord when it turns counter-clockwise. A whole
                // circle has a chord of no length, and its segment is its disc. The chord is the
                // one the crossing counts, from the end before it.)
                let on_arc_side = from == e1 || if sweep >= 0.0 { side(from, e1, p) < 0.0 } else { side(from, e1, p) > 0.0 };
                crosses(from, e1, p) ^ (q < 0.0 && on_arc_side)
            }
        }
    }
}

/// A plane figure's boundary as exact pieces, its rings closed. **The sign** is the parity of
/// the crossings of a ray from the point along +x (the even-odd rule, which for simple rings, the
/// holes inside the outer ring, is inside exactly where the figure is). Each line is counted
/// half-open in height and by one orientation test with a fixed tie-break ([`side`]), so a ray
/// through a vertex counts it once when the ring passes through and twice or not at all where
/// it only touches, and a point on an edge is read as just to its right. An arc counts as its
/// chord plus its segment: the ring of the arc with its chord turned back is a simple closed
/// curve, so the ring's winding number is the chord polygon's plus one for each segment that
/// holds the point, and the parity of the sum is the sum of the parities. The segment's test
/// uses the same orientation test as the chord's crossing, so the two agree at a point on the
/// chord. The distance is to every piece but a stretch of axis.
#[derive(Clone, Debug)]
pub struct Plane2 {
    parts: Vec<Part>,
    arcs: u64,
}

impl Plane2 {
    /// From closed rings of pieces, a parametric piece sampled at `detail`; `axis`: drop a line
    /// on `x = 0` from the surface (a turned figure's); `sampled` is set when a piece was sampled.
    fn of_rings(rings: &[Vec<Seg>], detail: u32, shift: Pt, axis_eps: Option<f64>, sampled: &mut bool) -> Plane2 {
        let mut parts = Vec::new();
        let mv = |p: Pt| [p[0] + shift[0], p[1] + shift[1]];
        let on_axis = |a: Pt, b: Pt| axis_eps.is_some_and(|e| a[0].abs() <= e && b[0].abs() <= e);
        for ring in rings {
            let first = parts.len();
            for seg in ring {
                match seg {
                    Seg::Line { a, b } => {
                        let (a, b) = (mv(*a), mv(*b));
                        parts.push(Part::Line { a, b, edge: !on_axis(a, b) });
                    }
                    Seg::Arc { c, r, sweep, ends, .. } => {
                        let (e0, e1) = (mv(ends[0]), mv(ends[1]));
                        let u = |e: Pt| [(e[0] - c[0]) / r, (e[1] - c[1]) / r];
                        parts.push(Part::Arc { r: *r, sweep: *sweep, e0, e1, u0: u(ends[0]), u1: u(ends[1]), from: e0 });
                    }
                    Seg::Fn { .. } => {
                        *sampled = true;
                        let k = seg.k(detail);
                        let mut prev = mv(seg.at(0, k));
                        for i in 1..=k {
                            let q = mv(seg.at(i, k));
                            parts.push(Part::Line { a: prev, b: q, edge: !on_axis(prev, q) });
                            prev = q;
                        }
                    }
                }
            }
            // (Each piece's crossing starts where the piece before it ends, so the ring the
            // parity counts is closed bit for bit.)
            let n = parts.len() - first;
            let end = |p: &Part| match p {
                Part::Line { b, .. } => *b,
                Part::Arc { e1, .. } => *e1,
            };
            let ends: Vec<Pt> = parts[first..].iter().map(end).collect();
            for i in 0..n {
                let before = ends[(i + n - 1) % n];
                match &mut parts[first + i] {
                    Part::Line { a, .. } => *a = before,
                    Part::Arc { from, .. } => *from = before,
                }
            }
        }
        let arcs = parts.iter().filter(|p| matches!(p, Part::Arc { .. })).count() as u64;
        Plane2 { parts, arcs }
    }

    /// The signed distance at `p`.
    pub fn at(&self, p: Pt, c: &mut Cost) -> f64 {
        let mut best = f64::INFINITY;
        let mut inside = false;
        for part in &self.parts {
            c.pieces += 1;
            inside ^= part.parity(p);
            if !matches!(part, Part::Line { edge: false, .. }) {
                best = best.min(part.d2(p, c));
            }
        }
        c.roots += 1;
        let d = best.sqrt();
        if inside {
            -d
        } else {
            d
        }
    }

    /// The most a sample costs.
    pub fn worst(&self) -> Cost {
        Cost { pieces: self.parts.len() as u64, roots: self.arcs + 1, trig: 0, powers: 0 }
    }

    /// The box of its pieces (an arc's extremes where its sweep reaches them).
    pub fn bounds(&self) -> (Pt, Pt) {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        let mut take = |p: Pt| {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        };
        for part in &self.parts {
            match *part {
                Part::Line { a, b, .. } => {
                    take(a);
                    take(b);
                }
                Part::Arc { r, sweep, e0, e1, u0, u1, .. } => {
                    take(e0);
                    take(e1);
                    let c = [e0[0] - r * u0[0], e0[1] - r * u0[1]];
                    for dir in [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]] {
                        let (s, t) = if sweep >= 0.0 { (cross(u0, dir), cross(dir, u1)) } else { (cross(u1, dir), cross(dir, u0)) };
                        let inside = if sweep.abs() <= std::f64::consts::PI { s >= 0.0 && t >= 0.0 } else { !(s < 0.0 && t < 0.0) };
                        if inside {
                            take([c[0] + r * dir[0], c[1] + r * dir[1]]);
                        }
                    }
                }
            }
        }
        (lo, hi)
    }
}

/// A plane figure's field.
#[derive(Clone, Debug)]
pub struct PlaneField {
    shape: Plane2,
    /// Exact on both sides (of the figure, or of its samples: see `truth`).
    pub sides: Sides,
    /// What figure it is the field of.
    pub truth: Truth,
    /// A box that holds the figure.
    pub bounds: (Pt, Pt),
    /// The most a sample costs.
    pub cost: Cost,
}

impl PlaneField {
    /// The signed distance at `p` (x right, y up, the figure's own frame).
    pub fn at(&self, p: Pt) -> f64 {
        self.shape.at(p, &mut Cost::default())
    }
    /// The same, adding what it cost to `cost`.
    pub fn at_counted(&self, p: Pt, cost: &mut Cost) -> f64 {
        self.shape.at(p, cost)
    }
}

fn check_detail(detail: u32) -> Result<(), Error> {
    if (DETAIL_MIN..=DETAIL_MAX).contains(&detail) {
        Ok(())
    } else {
        Err(Error::DetailOutOfRange { detail, min: DETAIL_MIN, max: DETAIL_MAX })
    }
}

fn refuse(form: &'static str, rule: &'static str, message: String, values: Vec<(&'static str, f64)>) -> Error {
    Error::Constraint { form, rule, message, values }
}

/// The pieces a figure's field at `detail` looks at.
fn plane_pieces(rings: &[Vec<Seg>], detail: u32) -> u64 {
    rings.iter().flatten().map(|s| if matches!(s, Seg::Fn { .. }) { s.k(detail) } else { 1 }).sum()
}

/// The coarsest-first search for a detail at which `pieces(d)` is at most `most`: halving from
/// `detail`, at most a dozen counts.
fn coarser(detail: u32, most: u64, pieces: impl Fn(u32) -> u64) -> Option<u32> {
    let mut d = detail / 2;
    while d >= DETAIL_MIN {
        if pieces(d) <= most {
            return Some(d);
        }
        d /= 2;
    }
    None
}

/// What a figure's parametric pieces' sampling error is at `detail` (a bound where the library
/// has one, else the build's estimate).
fn sampled_error(spec: &Spec, detail: u32) -> Result<(f64, bool), Error> {
    if let Some(e) = spec.error(detail)? {
        return Ok((e, true));
    }
    let e = match spec.build(detail)? {
        Built::Outline(o) => o.chord_error,
        Built::Mesh(m) => m.chord_error,
        _ => 0.0,
    };
    Ok((e, false))
}

/// A plane figure's signed distance at `detail` (which samples its parametric pieces only),
/// holes included. A curve has no inside and is refused, naming its band; a solid is
/// [`of`]'s.
pub fn plane(spec: &Spec, detail: u32) -> Result<PlaneField, Error> {
    check_detail(detail)?;
    let name = spec.form.name;
    let rings = match spec.geom_of()? {
        Geom::Rings(r) => r,
        Geom::Path { .. } => return Err(no_inside(name)),
        Geom::Solid(_) => return Err(refuse(name, "a_solid_not_a_figure", format!("'{name}' is a solid: its field is field::of"), Vec::new())),
    };
    // (A figure the library refuses to build at this detail is refused here too.)
    spec.build(detail)?;
    let n = plane_pieces(&rings, detail);
    if n > MAX_PLANE_PIECES {
        return Err(too_many(name, n, MAX_PLANE_PIECES, detail, coarser(detail, MAX_PLANE_PIECES, |d| plane_pieces(&rings, d))));
    }
    let mut sampled = false;
    let shape = Plane2::of_rings(&rings, detail, [0.0, 0.0], None, &mut sampled);
    let truth = if sampled {
        let (error, bound) = sampled_error(spec, detail)?;
        Truth::Sampled { detail, error, bound }
    } else {
        Truth::Form
    };
    Ok(PlaneField { bounds: shape.bounds(), cost: shape.worst(), shape, sides: EXACT, truth })
}

fn no_inside(name: &'static str) -> Error {
    refuse(name, "a_curve_has_no_inside", format!("'{name}' is a curve, which has no inside and so no signed distance: its band (a plane figure of a width) has one, and a sculpt can swell a curve's distance itself"), Vec::new())
}

fn too_many(name: &'static str, n: u64, most: u64, detail: u32, coarser: Option<u32>) -> Error {
    let mut values = vec![("pieces", n as f64), ("ceiling", most as f64), ("detail", detail as f64)];
    if let Some(d) = coarser {
        values.push(("coarser_detail_that_fits", d as f64));
    }
    refuse(name, "field_too_large", format!("a sample of '{name}''s field at detail {detail} would look at {n} pieces, over the ceiling of {most}: a coarser detail"), values)
}

// ---------------------------------------------------------------- space

/// What a solid's field is made of (built once; a sample only reads it).
#[derive(Clone, Debug)]
pub(crate) enum Repr {
    /// A plane figure in `(u, v) = (x, −z)` raised from `y0` to `y1` (4.1).
    Slab { plane: Plane2, y0: f64, y1: f64 },
    /// A figure in `(r, y)` turned about the Y axis (4.2), through `wedge` radians from the +x
    /// axis toward −z when not a full turn, and stretched by `stretch` along x and z (2.5).
    Turned { prof: Plane2, wedge: Option<(f64, P3)>, stretch: Option<[f64; 2]> },
    /// A ball.
    Ball { c: P3, r: f64 },
    /// A box of half sizes `half` about `c`, swollen by `r`.
    RoundBox { c: P3, half: P3, r: f64 },
    /// A cylinder of radius `r` about the line through `c` along axis `axis` (0 x, 2 z), unending.
    Rod { c: P3, axis: usize, r: f64 },
    /// The greatest of the fields (a meeting: 3.3), in a box.
    Meet(Vec<Repr>, (P3, P3)),
    /// The least of the fields (a union: 3.2).
    Union(Vec<Repr>),
    /// A convex polyhedron: each face's plane (outward unit normal, offset) and its corners.
    Convex { planes: Vec<(P3, f64)>, faces: Vec<Vec<P3>>, edges: u64 },
    /// A closed mesh's triangles.
    Tris(Vec<[P3; 3]>),
}

fn sub3(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot3(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross3(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len3(a: P3) -> f64 {
    dot3(a, a).sqrt()
}

/// The squared distance from `p` to the segment `a b` in space.
fn seg3_d2(a: P3, b: P3, p: P3) -> f64 {
    let ab = sub3(b, a);
    let l2 = dot3(ab, ab);
    let t = if l2 > 0.0 { (dot3(sub3(p, a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let q = [a[0] + ab[0] * t - p[0], a[1] + ab[1] * t - p[1], a[2] + ab[2] * t - p[2]];
    dot3(q, q)
}

/// The squared distance from `p` to the triangle `a b c`: to its plane where the foot is inside
/// it (each edge's side by the triangle's own normal), else to its nearest edge.
fn tri_d2(t: &[P3; 3], p: P3) -> f64 {
    let [a, b, c] = *t;
    let n = cross3(sub3(b, a), sub3(c, a));
    let nn = dot3(n, n);
    if nn > 0.0 {
        let ins = |u: P3, v: P3| dot3(cross3(sub3(v, u), sub3(p, u)), n) >= 0.0;
        if ins(a, b) && ins(b, c) && ins(c, a) {
            let h = dot3(sub3(p, a), n);
            return h * h / nn;
        }
    }
    seg3_d2(a, b, p).min(seg3_d2(b, c, p)).min(seg3_d2(c, a, p))
}

/// The solid angle the triangle subtends at `p`, over 4π, signed by its winding (Van Oosterom
/// and Strackee's formula: `tan(Ω/2) = det / (|a||b||c| + (a·b)|c| + (a·c)|b| + (b·c)|a|)`).
fn winding(t: &[P3; 3], p: P3) -> f64 {
    let (a, b, c) = (sub3(t[0], p), sub3(t[1], p), sub3(t[2], p));
    let (la, lb, lc) = (len3(a), len3(b), len3(c));
    let det = dot3(a, cross3(b, c));
    let den = la * lb * lc + dot3(a, b) * lc + dot3(a, c) * lb + dot3(b, c) * la;
    atan2(det, den) / (2.0 * std::f64::consts::PI)
}

impl Repr {
    pub(crate) fn at(&self, p: P3, c: &mut Cost) -> f64 {
        match self {
            Repr::Slab { plane, y0, y1 } => {
                let w1 = plane.at([p[0], -p[2]], c);
                let w2 = (p[1] - 0.5 * (y0 + y1)).abs() - 0.5 * (y1 - y0);
                c.roots += 1;
                w1.max(w2).min(0.0) + (w1.max(0.0) * w1.max(0.0) + w2.max(0.0) * w2.max(0.0)).sqrt()
            }
            Repr::Turned { prof, wedge, stretch } => {
                let (x, z, least) = match stretch {
                    Some([a, b]) => (p[0] / a, p[2] / b, a.min(*b).min(1.0)),
                    None => (p[0], p[2], 1.0),
                };
                c.roots += 1;
                let f = least * prof.at([(x * x + z * z).sqrt(), p[1]], c);
                match wedge {
                    None => f,
                    Some((sweep, n1)) => {
                        // The half-space past θ = 0 is z > 0 (the solid turns toward −z); past the
                        // other cut, along its outward normal `n1`.
                        let (f0, f1) = (p[2], dot3(p, *n1));
                        let w = if *sweep <= std::f64::consts::PI { f0.max(f1) } else { f0.min(f1) };
                        f.max(w)
                    }
                }
            }
            Repr::Ball { c: o, r } => {
                c.roots += 1;
                len3(sub3(p, *o)) - r
            }
            Repr::RoundBox { c: o, half, r } => {
                let q = [(p[0] - o[0]).abs() - half[0], (p[1] - o[1]).abs() - half[1], (p[2] - o[2]).abs() - half[2]];
                let out = [q[0].max(0.0), q[1].max(0.0), q[2].max(0.0)];
                c.roots += 1;
                len3(out) + q[0].max(q[1]).max(q[2]).min(0.0) - r
            }
            Repr::Rod { c: o, axis, r } => {
                let d = sub3(p, *o);
                let (u, v) = if *axis == 0 { (d[1], d[2]) } else { (d[0], d[1]) };
                c.roots += 1;
                (u * u + v * v).sqrt() - r
            }
            Repr::Meet(list, _) => list.iter().map(|x| x.at(p, c)).fold(f64::NEG_INFINITY, f64::max),
            Repr::Union(list) => list.iter().map(|x| x.at(p, c)).fold(f64::INFINITY, f64::min),
            Repr::Convex { planes, faces, .. } => {
                let mut most = f64::NEG_INFINITY;
                for (n, d) in planes {
                    most = most.max(dot3(*n, p) - d);
                }
                c.pieces += planes.len() as u64;
                if most <= 0.0 {
                    return most;
                }
                // Outside: the nearest point of a convex solid lies on a face that faces `p`.
                let mut best = f64::INFINITY;
                for ((n, d), face) in planes.iter().zip(faces) {
                    let h = dot3(*n, p) - d;
                    if h <= 0.0 {
                        continue;
                    }
                    c.pieces += face.len() as u64;
                    let foot = [p[0] - h * n[0], p[1] - h * n[1], p[2] - h * n[2]];
                    let k = face.len();
                    let inside = (0..k).all(|i| dot3(cross3(sub3(face[(i + 1) % k], face[i]), sub3(foot, face[i])), *n) >= 0.0);
                    if inside {
                        best = best.min(h * h);
                    } else {
                        for i in 0..k {
                            best = best.min(seg3_d2(face[i], face[(i + 1) % k], p));
                        }
                    }
                }
                c.roots += 1;
                best.sqrt()
            }
            Repr::Tris(tris) => {
                let (mut best, mut w) = (f64::INFINITY, 0.0);
                for t in tris {
                    best = best.min(tri_d2(t, p));
                    w += winding(t, p);
                }
                let n = tris.len() as u64;
                c.pieces += n;
                c.roots += 3 * n + 1;
                c.trig += n;
                let d = best.sqrt();
                if w > 0.5 {
                    -d
                } else {
                    d
                }
            }
        }
    }

    pub(crate) fn worst(&self) -> Cost {
        match self {
            Repr::Slab { plane, .. } => {
                let mut c = plane.worst();
                c.roots += 1;
                c
            }
            Repr::Turned { prof, .. } => {
                let mut c = prof.worst();
                c.roots += 1;
                c
            }
            Repr::Ball { .. } | Repr::RoundBox { .. } | Repr::Rod { .. } => Cost { pieces: 0, roots: 1, trig: 0, powers: 0 },
            Repr::Meet(list, _) | Repr::Union(list) => {
                let mut c = Cost::default();
                for x in list {
                    c.add(x.worst());
                }
                c
            }
            Repr::Convex { planes, edges, .. } => Cost { pieces: planes.len() as u64 + edges, roots: 1, trig: 0, powers: 0 },
            Repr::Tris(t) => {
                let n = t.len() as u64;
                Cost { pieces: n, roots: 3 * n + 1, trig: n, powers: 0 }
            }
        }
    }

    pub(crate) fn bounds(&self) -> (P3, P3) {
        match self {
            Repr::Slab { plane, y0, y1 } => {
                let (lo, hi) = plane.bounds();
                ([lo[0], *y0, -hi[1]], [hi[0], *y1, -lo[1]])
            }
            Repr::Turned { prof, stretch, .. } => {
                let (lo, hi) = prof.bounds();
                let [a, b] = stretch.unwrap_or([1.0, 1.0]);
                let r = hi[0].max(-lo[0]).max(0.0);
                ([-a * r, lo[1], -b * r], [a * r, hi[1], b * r])
            }
            Repr::Ball { c, r } => ([c[0] - r, c[1] - r, c[2] - r], [c[0] + r, c[1] + r, c[2] + r]),
            Repr::RoundBox { c, half, r } => ([0, 1, 2].map(|k| c[k] - half[k] - r), [0, 1, 2].map(|k| c[k] + half[k] + r)),
            Repr::Rod { .. } => ([f64::NEG_INFINITY; 3], [f64::INFINITY; 3]),
            Repr::Meet(_, b) => *b,
            Repr::Union(list) => {
                let mut b = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                for x in list {
                    let (lo, hi) = x.bounds();
                    b = ([0, 1, 2].map(|k| b.0[k].min(lo[k])), [0, 1, 2].map(|k| b.1[k].max(hi[k])));
                }
                b
            }
            Repr::Convex { faces, .. } => points_box(faces.iter().flatten().copied()),
            Repr::Tris(t) => points_box(t.iter().flatten().copied()),
        }
    }
}

fn points_box(pts: impl Iterator<Item = P3>) -> (P3, P3) {
    let mut b = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in pts {
        for k in 0..3 {
            b.0[k] = b.0[k].min(p[k]);
            b.1[k] = b.1[k].max(p[k]);
        }
    }
    b
}

/// What a construction says its field is ([`crate::solid::Make::field`]).
pub(crate) struct Plan {
    pub repr: Repr,
    pub sides: Sides,
    /// Whether a parametric piece was sampled (then the field is of the samples).
    pub sampled: bool,
}

impl Plan {
    /// A figure's rings raised from `y0` to `y1`, sampled at `detail`.
    pub(crate) fn slab(rings: &[Vec<Seg>], detail: u32, y0: f64, y1: f64) -> Plan {
        let mut sampled = false;
        let plane = Plane2::of_rings(rings, detail, [0.0, 0.0], None, &mut sampled);
        Plan { repr: Repr::Slab { plane, y0, y1 }, sides: EXACT, sampled }
    }
    /// A relief: a plate under a raised figure, their union (3.2).
    pub(crate) fn relief(rect: (Pt, Pt), plate: f64, figure: Plan, top: f64) -> Plan {
        let (lo, hi) = rect;
        let corners = [[lo[0], lo[1]], [hi[0], lo[1]], [hi[0], hi[1]], [lo[0], hi[1]]];
        let ring: Vec<Seg> = (0..4).map(|i| Seg::Line { a: corners[i], b: corners[(i + 1) % 4] }).collect();
        let base = Plan::slab(&[ring], DETAIL_MIN, 0.0, plate);
        let Repr::Slab { plane, .. } = figure.repr else { return figure };
        Plan { repr: Repr::Union(vec![base.repr, Repr::Slab { plane, y0: plate, y1: top }]), sides: OUT_EXACT, sampled: figure.sampled }
    }
    /// A profile of pieces (an open one from the axis to the axis, closed by it) moved by `shift`
    /// and turned through `sweep`, on a ring stretched by `stretch`.
    pub(crate) fn turned(pieces: &[Seg], closed: bool, shift: Pt, sweep: f64, stretch: Option<[f64; 2]>, detail: u32) -> Plan {
        let mut ring: Vec<Seg> = pieces.to_vec();
        let mut reach = 0.0f64;
        for s in pieces {
            for p in [s.start(), s.end()] {
                reach = reach.max(p[0].abs() + shift[0].abs()).max(p[1].abs() + shift[1].abs());
            }
        }
        let eps = 1e-12 * reach;
        if !closed {
            // (The axis closes it. Its ends land on the axis exactly: an arc to a pole lands
            // within rounding of it, and a sample on the axis must not be read as beside it.)
            let (first, last) = (pieces[0].start(), pieces[pieces.len() - 1].end());
            ring.push(Seg::Line { a: [-shift[0], last[1]], b: [-shift[0], first[1]] });
            if let Some(Seg::Line { a, .. }) = ring.first_mut() {
                if (a[0] + shift[0]).abs() <= eps {
                    a[0] = -shift[0];
                }
            }
            let n = ring.len() - 2;
            if let Seg::Line { b, .. } = &mut ring[n] {
                if (b[0] + shift[0]).abs() <= eps {
                    b[0] = -shift[0];
                }
            }
        }
        let mut sampled = false;
        let prof = Plane2::of_rings(&[ring], detail, shift, Some(eps), &mut sampled);
        let full = sweep >= std::f64::consts::TAU;
        let wedge = (!full).then(|| {
            let (s, c) = sin_cos(sweep);
            (sweep, [-s, 0.0, -c])
        });
        let sides = match (stretch.is_some(), wedge) {
            (true, _) => BOUND,
            (false, None) => EXACT,
            (false, Some((w, _))) if w <= std::f64::consts::PI => IN_EXACT,
            _ => BOUND,
        };
        Plan { repr: Repr::Turned { prof, wedge, stretch }, sides, sampled }
    }
    /// A ball of radius `r` about `c`.
    pub(crate) fn ball(c: P3, r: f64) -> Plan {
        Plan { repr: Repr::Ball { c, r }, sides: EXACT, sampled: false }
    }
    /// A box of half sizes `half` about `c` with edges and corners rounded by `r` (FORMAE.md:
    /// a convex solid swollen by a ball is the solid's distance less the radius, on both sides).
    pub(crate) fn round_box(c: P3, half: P3, r: f64) -> Plan {
        Plan { repr: Repr::RoundBox { c, half: half.map(|h| h - r), r }, sides: EXACT, sampled: false }
    }
    /// The bicylinder of radius `r` standing on y = 0: where the cylinders along x and along z
    /// meet (3.3).
    pub(crate) fn bicylinder(r: f64) -> Plan {
        let c = [0.0, r, 0.0];
        let b = ([-r, 0.0, -r], [r, 2.0 * r, r]);
        Plan { repr: Repr::Meet(vec![Repr::Rod { c, axis: 0, r }, Repr::Rod { c, axis: 2, r }], b), sides: IN_EXACT, sampled: false }
    }
}

/// A solid's field.
#[derive(Clone, Debug)]
pub struct Field {
    repr: Repr,
    /// What it is: `slab`, `relief`, `turned`, `ball`, `rounded_box`, `meeting`, `convex_build`,
    /// `mesh_build`.
    pub how: &'static str,
    /// Exact or a bound, outside and inside.
    pub sides: Sides,
    /// What solid it is the field of.
    pub truth: Truth,
    /// A box that holds the solid (the field's own: the form's, or the sampled one's).
    pub bounds: (P3, P3),
    /// The most a sample costs.
    pub cost: Cost,
}

impl Field {
    /// The signed distance at `p` (metres; negative inside).
    pub fn at(&self, p: P3) -> f64 {
        self.repr.at(p, &mut Cost::default())
    }
    /// The same, adding what it cost to `cost`.
    pub fn at_counted(&self, p: P3, cost: &mut Cost) -> f64 {
        self.repr.at(p, cost)
    }
}

fn finish(plan: Plan, how: &'static str, truth: Truth) -> Field {
    let bounds = plan.repr.bounds();
    Field { cost: plan.repr.worst(), bounds, repr: plan.repr, how, sides: plan.sides, truth }
}

fn how_of(repr: &Repr) -> &'static str {
    match repr {
        Repr::Slab { .. } => "slab",
        Repr::Turned { stretch: Some(_), .. } => "turned_stretched",
        Repr::Turned { wedge: Some(_), .. } => "turned_part",
        Repr::Turned { .. } => "turned",
        Repr::Ball { .. } => "ball",
        Repr::RoundBox { .. } => "rounded_box",
        Repr::Rod { .. } | Repr::Meet(..) => "meeting",
        Repr::Union(_) => "relief",
        Repr::Convex { .. } => "convex_build",
        Repr::Tris(_) => "mesh_build",
    }
}

/// A solid's field at `detail`: its construction's own where it has one, else its build's
/// (refused past [`MAX_MESH_TRIANGLES`], naming a coarser detail that fits). A plane figure is
/// [`plane`]'s, raised or turned through [`crate::raise`] or [`crate::ops`] and then
/// [`of_solid`]; a curve is refused.
pub fn of(spec: &Spec, detail: u32) -> Result<Field, Error> {
    check_detail(detail)?;
    let name = spec.form.name;
    let make = match spec.geom_of()? {
        Geom::Solid(m) => m,
        Geom::Path { .. } => return Err(no_inside(name)),
        Geom::Rings(_) => {
            return Err(refuse(name, "a_figure_has_no_solid", format!("'{name}' is a plane figure: its field in the plane is field::plane, and as a solid it is raised (raise::slab) or turned (ops::revolve_of) and then field::of_solid"), Vec::new()));
        }
    };
    spec.check()?;
    let error = || spec.error(detail);
    let build = || match spec.build(detail)? {
        Built::Mesh(m) => Ok(m),
        _ => Err(refuse(name, "not_a_solid", format!("'{name}' does not build to a solid"), Vec::new())),
    };
    let count = |d: u32| spec.count(d).map(|c| c.parts).unwrap_or(u64::MAX);
    from_make(name, make.as_ref(), detail, &error, &build, &count)
}

/// The field of a solid an operator or a raise made ([`crate::ops`], [`crate::raise`]).
pub fn of_solid(solid: &Solid, detail: u32) -> Result<Field, Error> {
    check_detail(detail)?;
    let error = || solid.error(detail);
    let build = || solid.build(detail);
    let count = |d: u32| solid.count(d).map(|c| c.parts).unwrap_or(u64::MAX);
    from_make(solid.figure, solid.make().as_ref(), detail, &error, &build, &count)
}

type Try<T> = Result<T, Error>;

fn from_make(name: &'static str, make: &dyn crate::solid::Make, detail: u32, error: &dyn Fn() -> Try<Option<f64>>, build: &dyn Fn() -> Try<Mesh>, faces: &dyn Fn(u32) -> u64) -> Result<Field, Error> {
    if let Some(plan) = make.field(detail) {
        // (A construction the library refuses to build at this detail is refused here too.)
        build()?;
        if let Repr::Slab { plane, .. } | Repr::Turned { prof: plane, .. } = &plan.repr {
            let n = plane.parts.len() as u64;
            if n > MAX_PLANE_PIECES {
                return Err(too_many(name, n, MAX_PLANE_PIECES, detail, None));
            }
        }
        let truth = if plan.sampled {
            match error()? {
                Some(e) => Truth::Sampled { detail, error: e, bound: true },
                None => Truth::Sampled { detail, error: build()?.chord_error, bound: false },
            }
        } else {
            Truth::Form
        };
        let how = how_of(&plan.repr);
        return Ok(finish(plan, how, truth));
    }
    // Its build: faces first (a convex build looks at its faces), triangles if not convex.
    let f = faces(detail);
    if f > MAX_MESH_TRIANGLES {
        return Err(too_many(name, f, MAX_MESH_TRIANGLES, detail, coarser(detail, MAX_MESH_TRIANGLES, faces)));
    }
    let mesh = build()?;
    let truth = if mesh.chord_error == 0.0 {
        Truth::Form
    } else {
        Truth::Sampled { detail, error: mesh.chord_error, bound: error()?.is_some() }
    };
    if let Some(repr) = convex(&mesh) {
        return Ok(finish(Plan { repr, sides: EXACT, sampled: false }, "convex_build", truth));
    }
    let tris: Vec<[P3; 3]> = mesh.triangles().into_iter().map(|(_, t)| t.map(|i| mesh.points[i as usize])).collect();
    if tris.len() as u64 > MAX_MESH_TRIANGLES {
        let n = tris.len() as u64;
        // (A non-convex build's triangles: about twice its faces' count.)
        return Err(too_many(name, n, MAX_MESH_TRIANGLES, detail, coarser(detail, MAX_MESH_TRIANGLES / 2, faces)));
    }
    Ok(finish(Plan { repr: Repr::Tris(tris), sides: EXACT, sampled: false }, "mesh_build", truth))
}

/// The build as a convex polyhedron, when it is one: every point on or behind every face's plane
/// (to `1e-12` of its reach). A face's plane is its Newell normal through its centroid.
fn convex(mesh: &Mesh) -> Option<Repr> {
    let reach = mesh.points.iter().map(|p| p[0].abs().max(p[1].abs()).max(p[2].abs())).fold(0.0f64, f64::max);
    let tol = 1e-12 * reach;
    let mut planes = Vec::with_capacity(mesh.faces.len());
    let mut faces = Vec::with_capacity(mesh.faces.len());
    let mut edges = 0u64;
    for f in &mesh.faces {
        let pts: Vec<P3> = f.verts.iter().map(|&i| mesh.points[i as usize]).collect();
        let k = pts.len();
        let mut n = [0.0; 3];
        let mut c = [0.0; 3];
        for i in 0..k {
            let (a, b) = (pts[i], pts[(i + 1) % k]);
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
            for j in 0..3 {
                c[j] += a[j] / k as f64;
            }
        }
        let l = len3(n);
        if l == 0.0 {
            return None;
        }
        let n = n.map(|x| x / l);
        let d = dot3(n, c);
        if pts.iter().any(|p| (dot3(n, *p) - d).abs() > tol) {
            return None;
        }
        edges += k as u64;
        planes.push((n, d));
        faces.push(pts);
    }
    for (n, d) in &planes {
        if mesh.points.iter().any(|p| dot3(*n, *p) - d > tol) {
            return None;
        }
    }
    Some(Repr::Convex { planes, faces, edges })
}

/// One row of the table of what each form's field is: its name, kind, and at its defaults and
/// `detail`, how its field is made, its sides and whether it is of the form or of samples; or the
/// rule it is refused by.
pub struct Row {
    /// The form's name.
    pub name: &'static str,
    /// Its kind.
    pub kind: Kind,
    /// The field's `how`, or `refused`.
    pub how: &'static str,
    /// Its sides, when it has a field.
    pub sides: Option<Sides>,
    /// What it is the field of.
    pub truth: Option<Truth>,
    /// The refusal's rule, when refused.
    pub refused: Option<&'static str>,
    /// The most a sample costs.
    pub cost: Cost,
}

/// Every form's field at its defaults and `detail`, in the table's order.
pub fn table(detail: u32) -> Vec<Row> {
    let mut rows = Vec::new();
    for f in crate::forms() {
        let row = |how, sides, truth, refused, cost| Row { name: f.name, kind: f.kind, how, sides, truth, refused, cost };
        let got = Spec::new(f.name, Some(f.kind), &[]).and_then(|s| match f.kind {
            Kind::Solid => of(&s, detail).map(|x| (x.how, x.sides, x.truth, x.cost)),
            _ => plane(&s, detail).map(|x| ("plane", x.sides, x.truth, x.cost)),
        });
        rows.push(match got {
            Ok((how, sides, truth, cost)) => row(how, Some(sides), Some(truth), None, cost),
            Err(e) => {
                let rule = match &e {
                    Error::Constraint { rule, .. } => *rule,
                    other => other.code(),
                };
                row("refused", None, None, Some(rule), Cost::default())
            }
        });
    }
    rows
}
