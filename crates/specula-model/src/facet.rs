//! How coarsely each shape a recipe names round was built, **measured in the finished model**
//! (docs/design/FORM.md, "Faceting, from what the recipe names round").
//!
//! A recipe says what is meant to be round; the facets are what its counts made of it. A
//! shape notes, as it is built, its turn between neighbouring faces and its radius in its own
//! frame, and it is given a slot of the build's record, so that every triangle it makes knows
//! it ([`crate::Shape::owners`]: through every operation, a boolean and a scatter). Every
//! operation that moves its triangles afterwards carries the linear map it applied
//! ([`Moved`]), so what a facet is in the finished model is known exactly:
//!
//! - under a move, a turn, a mirror, a uniform scale, copies and scatters (similarities), the
//!   turn is as built and the radius and how far a facet stands in from the true curve scale
//!   by the factor, the largest copy's;
//! - under a scale that is not the same in every direction, or a lean, a ring (a cylinder's,
//!   a cone's, a lathe's, a torus's round its axis, a capsule's round its axis) becomes an
//!   ellipse: its polygon is mapped vertex by vertex and its largest turn and the furthest
//!   any facet stands in from the mapped curve are measured (an affine map keeps a chord's
//!   tangent point, so the furthest point of a mapped arc from its chord is the mapped arc's
//!   middle); any other kind is **not measured** and says so;
//! - a taper, a twist and a bend are not affine: what they make of a facet is not measured;
//! - a displacement moves the facets and the curve they stand for together (one field): the
//!   facets are as they were, and how far it moved them is said beside them.
//!
//! A shape with no triangle of its own in the finished model (a cutter that cuts nothing, a
//! shape cut wholly away) says nothing: exact, from the tie. A cutter that does cut speaks
//! for the walls it left. Nothing here changes a mesh.

use crate::dmath;
use crate::math3::Affine;
use crate::shape::{Shape, NO_OWNER};
use specula_core::Json;

/// A 3×3 linear map, columns.
pub(crate) type M3 = [[f64; 3]; 3];

pub(crate) const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Distinct linear maps one round shape keeps (its copies'); past them, while every map is a
/// similarity, only the largest factor is kept (all a similarity needs); otherwise it is not
/// measured.
pub(crate) const MAX_MAPS: usize = 32;

/// Round shapes recorded while one model is built (past them, a round shape is not recorded
/// and the facts say how many were not).
pub(crate) const MAX_RECS: usize = 4096;

/// A shape a recipe names round, and how coarsely it was built, in the finished model:
/// **what a mesh alone cannot say** (a box and a four-sided cylinder are one mesh; whether a
/// ten-sided column is a coarse cylinder or a prism is what its maker meant, and the recipe
/// says which: a prism is an extruded outline of ten sides, a cylinder is a cylinder).
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    /// Where in the recipe (`parts[2].shape`, `parts[0].shape.of[1]`,
    /// `parts[1].include(gem).parts[0].shape`; a sculpt's primitive adds `.tree` and its
    /// number, depth first: `parts[0].shape.tree#3`).
    pub path: String,
    /// The shape's name in the recipe.
    pub what: &'static str,
    /// Which of its numbers made this turn: `sides`, `segments`, `rings`, `subdivisions`,
    /// `resolution`.
    pub count: &'static str,
    /// That number as built.
    pub count_value: u32,
    /// The largest turn between neighbouring faces round it, degrees, in the finished model
    /// (a cone's: its base's, from one side to the next).
    pub turn_deg: f32,
    /// The largest radius it turns at in the finished model, metres.
    pub radius_m: f32,
    /// How far the middle of a facet stands in from the true curve, at the most, metres, in
    /// the finished model: what is seen of the faceting in a silhouette.
    pub sag: f32,
    /// How far a displacement moved its facets (with the curve they stand for), metres; 0 for
    /// none.
    pub displaced_m: f32,
    /// `None`: measured. Otherwise why not (a taper, a twist or a bend; a shape other than a
    /// ring stretched unevenly); the turn, radius and sag are then as it was built.
    pub unmeasured: Option<&'static str>,
}

impl Round {
    /// How far the middle of a facet stands in from the true curve, metres.
    pub fn sag_m(&self) -> f32 {
        self.sag
    }

    pub fn measured(&self) -> bool {
        self.unmeasured.is_none()
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f32, k: f64| Json::from(dmath::round_f64(f64::from(x) * k) / k);
        let mut j = Json::obj([
            ("path", Json::from(self.path.as_str())),
            ("shape", Json::from(self.what)),
            ("count", Json::from(self.count)),
            ("count_value", Json::from(self.count_value)),
            ("turn_deg", r(self.turn_deg, 100.0)),
            ("radius_m", r(self.radius_m, 1.0e4)),
            ("sag_m", r(self.sag, 1.0e5)),
        ]);
        if self.displaced_m > 0.0 {
            j.set("displaced_m", r(self.displaced_m, 1.0e4));
        }
        if let Some(why) = self.unmeasured {
            j.set("not_measured", Json::from(why));
        }
        j
    }
}

/// How far the middle of a facet that turns by `turn_deg` stands in from a curve of
/// `radius`.
pub fn sag_of(radius: f64, turn_deg: f64) -> f64 {
    radius * (1.0 - dmath::cos_f64((turn_deg * 0.5).to_radians()))
}

/// What a shape notes as it is built, before it has a slot.
#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub what: &'static str,
    pub count: &'static str,
    pub count_value: u32,
    pub turn_deg: f64,
    pub radius: f64,
    /// A ring round the shape's own Y axis of this many sides (its first vertex's angle is
    /// found on the built mesh).
    pub ring: Option<u32>,
    /// A sculpt's primitive: its number, depth first.
    pub primitive: Option<usize>,
}

/// A round shape while the model is built.
#[derive(Clone, Debug)]
pub(crate) struct Rec {
    pub note: Note,
    /// The made-record it hangs on (its path) and its slot (its triangles).
    pub record: u32,
    pub slot: u32,
    /// The ring's first vertex's angle about Y, radians.
    pub phase: f64,
    /// The linear maps its copies were put through since it was built.
    pub maps: Vec<M3>,
    /// Past [`MAX_MAPS`] copies, all similarities: the largest factor.
    pub many: Option<f64>,
    pub displaced: f64,
    pub unknown: Option<&'static str>,
}

/// What an operation did to the triangles of a shape, for the round shapes among them.
pub(crate) enum Moved {
    Linear(M3),
    /// Copies, each through its own map (an array, a radial, a mirror that keeps both halves,
    /// a scatter's places).
    Copies(Vec<M3>),
    /// Not affine.
    Unknown(&'static str),
    /// Displaced by up to this far.
    Displaced(f64),
}

/// An affine's linear part.
pub(crate) fn linear(a: &Affine) -> M3 {
    a.cols.map(|c| c.map(f64::from))
}

fn mul(a: &M3, b: &M3) -> M3 {
    let mut out = [[0.0; 3]; 3];
    for (j, col) in b.iter().enumerate() {
        for i in 0..3 {
            out[j][i] = a[0][i] * col[0] + a[1][i] * col[1] + a[2][i] * col[2];
        }
    }
    out
}

fn apply(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// The factor of a similarity (`None` for a map that stretches some ways more than others).
pub(crate) fn similarity(m: &M3) -> Option<f64> {
    let g = [0, 1, 2].map(|i| [0, 1, 2].map(|j| dot(m[i], m[j])));
    let s = (g[0][0] + g[1][1] + g[2][2]) / 3.0;
    if !(s.is_finite() && s > 0.0) {
        return None;
    }
    let off = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (g[i][j] - if i == j { s } else { 0.0 }).abs()).fold(0.0, f64::max);
    (off <= 1e-5 * s).then(|| s.sqrt())
}

/// The largest stretch of a map: the square root of its Gram matrix's largest eigenvalue (by
/// a fixed number of power steps from a fixed start: the same on every machine; an upper
/// bound is its Frobenius norm, which it never passes).
fn stretch(m: &M3) -> f64 {
    let g = [0, 1, 2].map(|i| [0, 1, 2].map(|j| dot(m[i], m[j])));
    let mut v = [1.0, 0.7, 0.4];
    let mut l = 0.0;
    for _ in 0..64 {
        let w = [dot(g[0], v), dot(g[1], v), dot(g[2], v)];
        let n = norm(w);
        if !(n > 0.0 && n.is_finite()) {
            break;
        }
        l = n / norm(v);
        v = w.map(|x| x / n);
    }
    l.sqrt()
}

impl Rec {
    pub(crate) fn moved(&mut self, mv: &Moved) {
        if self.unknown.is_some() {
            return;
        }
        match mv {
            Moved::Linear(l) => {
                self.displaced *= stretch(l);
                match (&mut self.many, similarity(l)) {
                    (Some(f), Some(s)) => *f *= s,
                    (Some(_), None) => self.unknown = Some("copied many ways, then stretched unevenly"),
                    (None, _) => self.maps.iter_mut().for_each(|m| *m = mul(l, m)),
                }
            }
            Moved::Copies(ls) => {
                self.displaced *= ls.iter().map(stretch).fold(0.0, f64::max);
                if let Some(f) = &mut self.many {
                    match ls.iter().map(similarity).collect::<Option<Vec<f64>>>() {
                        Some(s) => *f *= s.into_iter().fold(0.0, f64::max),
                        None => self.unknown = Some("copied many ways, then stretched unevenly"),
                    }
                    return;
                }
                let mut out: Vec<M3> = Vec::new();
                for l in ls {
                    for m in &self.maps {
                        let x = mul(l, m);
                        if !out.contains(&x) {
                            out.push(x);
                        }
                    }
                }
                if out.len() <= MAX_MAPS {
                    self.maps = out;
                } else {
                    match out.iter().map(similarity).collect::<Option<Vec<f64>>>() {
                        Some(s) => {
                            self.many = Some(s.into_iter().fold(0.0, f64::max));
                            self.maps.clear();
                        }
                        None => self.unknown = Some("copied many ways, stretched unevenly"),
                    }
                }
            }
            Moved::Unknown(why) => self.unknown = Some(why),
            Moved::Displaced(d) => self.displaced += d.abs(),
        }
    }

    /// What it is in the finished model.
    fn measure(&self, path: String) -> Round {
        let n = &self.note;
        let as_built = |unmeasured: Option<&'static str>| Round {
            path: path.clone(),
            what: n.what,
            count: n.count,
            count_value: n.count_value,
            turn_deg: n.turn_deg as f32,
            radius_m: n.radius as f32,
            sag: sag_of(n.radius, n.turn_deg) as f32,
            displaced_m: self.displaced as f32,
            unmeasured,
        };
        if self.unknown.is_some() {
            return as_built(self.unknown);
        }
        let (mut turn, mut radius, mut sag) = (0.0f64, 0.0f64, 0.0f64);
        let factors: Vec<(Option<f64>, Option<&M3>)> = match self.many {
            Some(f) => vec![(Some(f), None)],
            None => self.maps.iter().map(|m| (similarity(m), Some(m))).collect(),
        };
        // (A sphere's faces stand in further at their middles than the turn at their edges
        // says: measured face by face, once as built, then scaled.)
        let ball = (n.what == "sphere").then(|| sphere_under(&IDENTITY, n.radius, n.count_value));
        for (f, m) in factors {
            let (t, r, s) = match (f, m, n.ring) {
                (Some(f), _, _) => match ball {
                    Some((t, r, s)) => (t, r * f, s * f),
                    None => (n.turn_deg, n.radius * f, sag_of(n.radius, n.turn_deg) * f),
                },
                (None, Some(m), Some(sides)) => ring_under(m, n.radius, sides, self.phase),
                (None, Some(m), None) if n.what == "sphere" => sphere_under(m, n.radius, n.count_value),
                _ => return as_built(Some("stretched unevenly (only a ring and a sphere are measured so)")),
            };
            turn = turn.max(t);
            radius = radius.max(r);
            sag = sag.max(s);
        }
        Round { path, what: n.what, count: n.count, count_value: n.count_value, turn_deg: turn as f32, radius_m: radius as f32, sag: sag as f32, displaced_m: self.displaced as f32, unmeasured: None }
    }
}

/// A ring of `sides` round Y at `radius` (its first vertex at `phase` about Y) put through
/// `m`: its largest turn from one side to the next (degrees), its largest radius, and the
/// furthest any side stands in from the mapped circle (the mapped arc's middle: an affine map
/// keeps the point where an arc's tangent runs along its chord).
pub(crate) fn ring_under(m: &M3, radius: f64, sides: u32, phase: f64) -> (f64, f64, f64) {
    let n = sides.max(3) as usize;
    let at = |a: f64| {
        let (s, c) = dmath::sin_cos_f64(a);
        apply(m, [radius * c, 0.0, -radius * s])
    };
    let step = std::f64::consts::TAU / n as f64;
    let p: Vec<[f64; 3]> = (0..n).map(|k| at(phase + step * k as f64)).collect();
    let (mut turn, mut far, mut sag) = (0.0f64, 0.0f64, 0.0f64);
    for k in 0..n {
        let (a, b, c) = (p[(k + n - 1) % n], p[k], p[(k + 1) % n]);
        let (e0, e1) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - b[0], c[1] - b[1], c[2] - b[2]]);
        let t = dmath::atan2_f64(norm(cross(e0, e1)), dot(e0, e1)).to_degrees();
        turn = turn.max(t);
        far = far.max(norm(b));
        let mid = at(phase + step * (k as f64 + 0.5));
        let w = [mid[0] - b[0], mid[1] - b[1], mid[2] - b[2]];
        let l = norm(e1);
        if l > 0.0 {
            sag = sag.max(norm(cross(w, e1)) / l);
        }
    }
    (turn, far, sag)
}

/// A sphere of `radius` and `sub` subdivisions (as [`Shape::sphere`] builds it) put through
/// `m`: the largest turn between two faces that share an edge (degrees), its largest radius,
/// and the furthest any face stands in from the mapped sphere (an ellipsoid). That is exact:
/// a face's furthest point of the curve is where the curve's tangent plane is parallel to the
/// face, and an affine map keeps that, so it is the mapped sphere's point along the face's
/// own normal before the map.
pub(crate) fn sphere_under(m: &M3, radius: f64, sub: u32) -> (f64, f64, f64) {
    let s = Shape::sphere(1.0, sub.min(5));
    // (Centred: the sphere rests on y = 0.)
    let c = s.positions.iter().fold([0.0f64; 3], |a, p| [a[0] + f64::from(p[0]), a[1] + f64::from(p[1]), a[2] + f64::from(p[2])]).map(|x| x / s.positions.len().max(1) as f64);
    let local: Vec<[f64; 3]> = s.positions.iter().map(|p| [f64::from(p[0]) - c[0], f64::from(p[1]) - c[1], f64::from(p[2]) - c[2]]).collect();
    let image: Vec<[f64; 3]> = local.iter().map(|p| apply(m, p.map(|x| x * radius))).collect();
    let sub3 = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (mut sag, mut far) = (0.0f64, 0.0f64);
    let mut normals = Vec::with_capacity(s.tris.len());
    for t in &s.tris {
        let [a, b, cc] = t.map(|i| i as usize);
        let n0 = cross(sub3(local[b], local[a]), sub3(local[cc], local[a]));
        let n0 = n0.map(|x| x / norm(n0));
        let n1 = cross(sub3(image[b], image[a]), sub3(image[cc], image[a]));
        let n1 = n1.map(|x| x / norm(n1));
        let top = apply(m, n0.map(|x| x * radius));
        sag = sag.max(dot(n1, sub3(top, image[a])).abs());
        far = far.max(norm(image[a]));
        normals.push(n1);
    }
    // (Faces that share an edge, by their corners' numbers: the sphere's corners are shared.)
    let mut edges: Vec<(u32, u32, usize)> = s.tris.iter().enumerate().flat_map(|(i, t)| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])].map(|(a, b)| (a.min(b), a.max(b), i))).collect();
    edges.sort_unstable();
    let mut turn = 0.0f64;
    for w in edges.windows(2) {
        if (w[0].0, w[0].1) == (w[1].0, w[1].1) {
            let (p, q) = (normals[w[0].2], normals[w[1].2]);
            turn = turn.max(dmath::atan2_f64(norm(cross(p, q)), dot(p, q)).to_degrees());
        }
    }
    (turn, far, sag)
}

/// The angle about Y of the vertex of `s` (on triangles of `slot`, or all when it has no
/// owners) furthest from the Y axis: a ring's first vertex, up to a turn of one side.
pub(crate) fn phase_of(s: &Shape) -> f64 {
    let mut best = (-1.0f64, 0.0f64);
    for p in &s.positions {
        let r = f64::from(p[0]) * f64::from(p[0]) + f64::from(p[2]) * f64::from(p[2]);
        if r > best.0 * (1.0 + 1e-9) {
            best = (r, dmath::atan2_f64(-f64::from(p[2]), f64::from(p[0])));
        }
    }
    best.1
}

/// The slots `owners` holds, sorted, once each.
pub(crate) fn slots_in(owners: &[u32]) -> Vec<u32> {
    let mut v: Vec<u32> = owners.iter().copied().filter(|&o| o != NO_OWNER).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// What the round shapes come to in the finished model (`shapes`): one [`Round`] for each
/// with a triangle of its own left, and how many there are. Kept, at most [`crate::MAX_ROUND`],
/// **in the order the facts answer by**: every round shape that is the answer at some size of
/// the asset (the coarsest turn among those standing in at least so far: the shapes no other
/// both stands in further and turns more than), the furthest standing first; then the rest by
/// how far they stand in; the ones not measured are kept before both, up to eight.
pub(crate) fn finish(recs: &[Rec], paths: &[String], shapes: &[&Shape]) -> (Vec<Round>, usize) {
    if recs.is_empty() {
        return (Vec::new(), 0);
    }
    let slots = recs.iter().map(|r| r.slot as usize + 1).max().unwrap_or(0);
    let mut present = vec![false; slots];
    for s in shapes {
        for (t, &o) in s.tris.iter().zip(&s.owners) {
            if let Some(p) = present.get_mut(o as usize) {
                if !*p && crate::made::triangle_area(s, *t) > 0.0 {
                    *p = true;
                }
            }
        }
    }
    let mut all: Vec<Round> = recs
        .iter()
        .filter(|r| present.get(r.slot as usize).copied().unwrap_or(false))
        .map(|r| {
            let base = paths.get(r.record as usize).cloned().unwrap_or_default();
            r.measure(match r.note.primitive {
                Some(i) => format!("{base}.tree#{i}"),
                None => base,
            })
        })
        .collect();
    let total = all.len();
    // (Not measured first, up to eight; then the front by stand-in; then the rest.)
    let mut out: Vec<Round> = Vec::new();
    let mut rest: Vec<Round> = Vec::new();
    for r in all.drain(..) {
        if !r.measured() && out.len() < 8 {
            out.push(r);
        } else if r.measured() {
            rest.push(r);
        }
    }
    rest.sort_by(|a, b| b.sag.total_cmp(&a.sag).then(b.turn_deg.total_cmp(&a.turn_deg)).then(a.path.cmp(&b.path)));
    let mut front = Vec::new();
    let mut others = Vec::new();
    let mut best_turn = f32::NEG_INFINITY;
    for r in rest {
        if r.turn_deg > best_turn {
            best_turn = r.turn_deg;
            front.push(r);
        } else {
            others.push(r);
        }
    }
    out.extend(front);
    out.extend(others);
    out.truncate(crate::recipe::MAX_ROUND);
    (out, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(v: [f64; 3]) -> M3 {
        [[v[0], 0.0, 0.0], [0.0, v[1], 0.0], [0.0, 0.0, v[2]]]
    }

    /// Under a similarity a ring is as built, scaled; under a scale that stretches one way
    /// it is an ellipse, whose sides near its ends turn more and whose middle sides stand in
    /// further: held against the same ring's polygon measured point by point here.
    #[test]
    fn a_ring_under_a_map_is_measured_exactly() {
        let (t, r, s) = ring_under(&scale([2.0; 3]), 1.0, 6, 0.0);
        assert!((t - 60.0).abs() < 1e-9 && (r - 2.0).abs() < 1e-9 && (s - 2.0 * sag_of(1.0, 60.0)).abs() < 1e-9, "{t} {r} {s}");
        // Stretched along its axis only: the ring is untouched.
        let (t, r, s) = ring_under(&scale([1.0, 5.0, 1.0]), 1.0, 12, 0.3);
        assert!((t - 30.0).abs() < 1e-9 && (r - 1.0).abs() < 1e-9 && (s - sag_of(1.0, 30.0)).abs() < 1e-9);
        // Stretched across it by three: an ellipse of 3 by 1 drawn with 12 points.
        let (t, r, s) = ring_under(&scale([3.0, 1.0, 1.0]), 1.0, 12, 0.0);
        let pts: Vec<(f64, f64)> = (0..12).map(|k| (3.0 * dmath::cos_f64(k as f64 * 30f64.to_radians()), dmath::sin_f64(k as f64 * 30f64.to_radians()))).collect();
        let mut turn: f64 = 0.0;
        for k in 0..12 {
            let (a, b, c) = (pts[(k + 11) % 12], pts[k], pts[(k + 1) % 12]);
            let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
            turn = turn.max(dmath::atan2_f64(u.0 * v.1 - u.1 * v.0, u.0 * v.0 + u.1 * v.1).to_degrees());
        }
        // (The furthest stand-in, by a fine search along each mapped arc.)
        let mut far: f64 = 0.0;
        for k in 0..12 {
            let (a, b) = (pts[k], pts[(k + 1) % 12]);
            for j in 0..=1000 {
                let th = (k as f64 + j as f64 / 1000.0) * 30f64.to_radians();
                let q = (3.0 * dmath::cos_f64(th), dmath::sin_f64(th));
                let d = ((b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0)).abs() / dmath::hypot_f64(b.0 - a.0, b.1 - a.1);
                far = far.max(d);
            }
        }
        assert!((t - turn).abs() < 1e-6 && (r - 3.0).abs() < 1e-9 && (s - far).abs() < 1e-6 * far, "{t} against {turn}; {s} against {far}");
        assert!(t > 50.0 && s > sag_of(1.0, 30.0) * 2.0, "the ends of the ellipse turn more than a twelfth of a turn");
    }

    /// A sphere measured face by face: as built its largest turn is about the nominal one (41.8°
    /// over two to the subdivisions, and up to a fifth more once subdivided) and its faces stand
    /// in by at least what that turn says; stretched to an egg twice as tall its faces turn more and stand in further,
    /// by no more than the stretch.
    #[test]
    fn a_sphere_under_a_map_is_measured_face_by_face() {
        for sub in [0u32, 1, 2, 3] {
            let (t, r, s) = sphere_under(&IDENTITY, 0.5, sub);
            let want = 41.81 / f64::from(1u32 << sub);
            // (A face's middle stands in further than its edges: the twenty faces of none stand
            // in by a fifth of the radius, three times what the turn at an edge alone says, and
            // a subdivided one's by about four times.)
            assert!(t >= want - 0.01 && t < 1.2 * want && (r - 0.5).abs() < 1e-6 && s >= sag_of(0.5, want) && s < 0.25 * 0.5, "sub {sub}: {t} {r} {s}");
            let (t2, r2, s2) = sphere_under(&scale([1.0, 2.0, 1.0]), 0.5, sub);
            assert!(t2 > t && (r2 - 1.0).abs() < 1e-6 && s2 > s && s2 <= 2.0 * s * 1.0001, "sub {sub}: egg {t2} {r2} {s2}");
        }
    }

    #[test]
    fn a_similarity_is_told_apart() {
        let r = linear(&Affine::rotate_euler_deg([10.0, 33.0, -70.0]));
        assert!(similarity(&r).is_some_and(|f| (f - 1.0).abs() < 1e-6));
        assert!(similarity(&mul(&scale([2.0; 3]), &r)).is_some_and(|f| (f - 2.0).abs() < 1e-5));
        assert!(similarity(&mul(&scale([2.0, 1.0, 2.0]), &r)).is_none());
        assert!(similarity(&scale([-1.0, 1.0, 1.0])).is_some_and(|f| (f - 1.0).abs() < 1e-12), "a mirror");
        assert!((stretch(&mul(&scale([1.0, 4.0, 2.0]), &r)) - 4.0).abs() < 1e-6);
        assert_eq!(mul(&IDENTITY, &r), r);
    }
}
