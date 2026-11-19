//! How solids are made. A solid form's `geom` gives a [`Make`]: from it the library counts the
//! build before any work, builds it, takes its exact measures and its error. The kinds here:
//!
//! - [`Poly`]: a polyhedron of flat faces, given exactly (no detail).
//! - [`Rev`]: a profile of pieces (lines, arcs, parametric pieces) in the half plane `r >= 0`
//!   turned about the Y axis, fully or through an angle; an open profile runs from the axis to
//!   the axis (a ball's kind, genus 0), a closed one is a ring off the axis (a torus's kind).
//! - [`Extrude`] and [`Cone`]: a plane outline raised straight up (a prism) or to an apex (a
//!   pyramid).
//! - [`Gen`]: anything else, given by its own functions.
//!
//! **Revolution.** A point `(r, y)` of the profile at the ring's point `e` is `(r e₀, y, −r e₁)`:
//! on a circle (`e = (cos θ, sin θ)`) a true solid of revolution; on an ellipse or a superellipse
//! ([`Ring::Super`]) a solid whose section at every height is that ring scaled by `r` (an
//! ellipsoid, an elliptic cylinder, a superellipsoid). Each step of the sampled profile between
//! two of the ring's samples is a quad with two sides parallel (the quad `(r_a, e_i)`,
//! `(r_a, e_j)`, `(r_b, e_j)`, `(r_b, e_i)` has its first and last sides parallel to the ring's
//! chord `e_j − e_i`), so it is planar whatever the rounding. A step that ends on the axis is a
//! fan of triangles to a point there; a flat step from the axis (a cylinder's lid) is one polygon.
//!
//! **The error.** The true surface lies within the profile's greatest sag (scaled by the ring's
//! reach) of the profile's chords turned on the true ring, and those within the ring's greatest
//! sag at the greatest radius of the faces: the sum is a bound wherever each step's sag is one
//! (lines, arcs, and parametric pieces with a normal or a support function: see
//! `Piece::step_sag`).

use crate::detail::segments;
use crate::geom::{Geom, Seg};
use crate::maths::*;
use crate::mesh::{Face, Mesh, SolidFault, Surface};
use crate::outline::{Fault, Outline};
use crate::Measures;
use std::rc::Rc;
use std::sync::OnceLock;

/// What a solid form's geometry is: the functions the library derives everything from.
pub(crate) trait Make {
    /// The points and faces a build at `detail` has, exactly, before any work.
    fn count(&self, detail: u32) -> (u64, u64);
    /// The build at `detail`, and the fault its construction found (a section that crosses
    /// itself at this detail), if any. The mesh's own check is the library's.
    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>);
    /// The exact measures from the geometry, where they are known.
    fn exact(&self) -> Measures;
    /// A bound on the build's distance from the true solid at `detail` (0 for flat faces);
    /// `None` where none is known (then the build's own chord error is the estimate).
    fn error(&self, detail: u32) -> Option<f64>;
    /// The genus: 0 for a ball, 1 for a torus.
    fn genus(&self) -> u32;
    /// The construction's own check of a build at `detail` that the mesh's check passed: where the
    /// construction does not prove that no two faces cross (a tube swept along a curve, a
    /// superellipsoid), the search for faces that cross, and what the construction knows that
    /// makes it cheaper.
    fn check(&self, _mesh: &Mesh, _detail: u32) -> Option<SolidFault> {
        None
    }
    /// The same construction with a refusal's search limited to `builds` builds of its own size,
    /// where a refusal searches (a raised form's deepest inset); `None` where none does.
    fn limited(&self, _builds: u32) -> Option<std::rc::Rc<dyn Make>> {
        None
    }
    /// Whether it has curved surfaces.
    #[cfg_attr(not(test), allow(dead_code, reason = "read by the tests only"))]
    fn curved(&self) -> bool;
    /// The plane pieces it is made from, where it is made from a profile or an outline (a test
    /// holds every arc's ends to its circle).
    #[cfg_attr(not(test), allow(dead_code, reason = "read by the tests only"))]
    fn pieces(&self) -> Vec<Seg> {
        Vec::new()
    }
}

// ---------------------------------------------------------------- quadrature

/// The nodes and weights of the 8-point Gauss–Legendre rule on [−1, 1], computed (not typed):
/// the roots of the Legendre polynomial P₈ by Newton's method from Chebyshev-like guesses, and
/// the weights `2 / ((1 − x²) P₈'(x)²)`. A test holds it against the integrals of x⁰ … x¹⁴.
pub(crate) fn gauss() -> &'static [(f64, f64)] {
    static G: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    G.get_or_init(|| {
        const N: usize = 8;
        let legendre = |x: f64| {
            let (mut p0, mut p1) = (1.0, x);
            for k in 1..N {
                let p2 = ((2 * k + 1) as f64 * x * p1 - k as f64 * p0) / (k + 1) as f64;
                p0 = p1;
                p1 = p2;
            }
            (p1, N as f64 * (x * p1 - p0) / (x * x - 1.0))
        };
        (0..N)
            .map(|i| {
                let mut x = cos(PI * (i as f64 + 0.75) / (N as f64 + 0.5));
                for _ in 0..64 {
                    let (p, d) = legendre(x);
                    let dx = p / d;
                    x -= dx;
                    if dx.abs() <= 1e-17 {
                        break;
                    }
                }
                let (_, d) = legendre(x);
                (x, 2.0 / ((1.0 - x * x) * d * d))
            })
            .collect()
    })
}

/// `∫ f` from `a` to `b` by the 8-point rule (exact for polynomials of degree 15).
pub(crate) fn integrate<const K: usize>(f: impl Fn(f64) -> [f64; K], a: f64, b: f64) -> [f64; K] {
    let (h, m) = (0.5 * (b - a), 0.5 * (a + b));
    let mut s = [0.0; K];
    for &(x, w) in gauss() {
        let v = f(m + h * x);
        for i in 0..K {
            s[i] += w * h * v[i];
        }
    }
    s
}

/// The integrals of a profile of lines and arcs in the `(r, y)` half plane, taken about the
/// point `at`: with `ρ = r − at₀` and `η = y − at₁`, `[∮ ρ dη, ∮ ρ² dη, ∮ ρ³ dη, ∮ ρ η dη,
/// ∮ ρ² η dη, ∫ ρ ds, ∫ ds]`. Each piece's own numbers are taken less `at` before anything is
/// multiplied (a line's ends, an arc's centre), so a thin section far from the axis, integrated
/// about a point of its own, keeps its digits: nothing as large as its distance is squared and
/// then cancelled. (About the origin, `∮ r² dy` of a section `1e-4` across at 675 m cancelled to
/// 9e-4 of its value.) On a line the integrands are polynomials of degree 3 at most, which the
/// rule integrates exactly; an arc is cut into pieces of at most π/8, on which the rule's error
/// for its integrands (trigonometric polynomials of degree 4) is under `1e-20` of the value (the
/// rule's error term `(b − a)¹⁷ (8!)⁴ / (17 (16!)³) f⁽¹⁶⁾` with `f⁽¹⁶⁾ <= 4¹⁶ R⁴`): exact to
/// rounding. `None` when a piece is parametric.
pub(crate) fn profile_integrals(segs: &[&Seg], at: Pt) -> Option<[f64; 7]> {
    let mut s = [0.0; 7];
    for seg in segs {
        let v = match seg {
            Seg::Line { a, b } => {
                let (a0, a1) = (a[0] - at[0], a[1] - at[1]);
                let (dr, dy) = (b[0] - a[0], b[1] - a[1]);
                let l = (dr * dr + dy * dy).sqrt();
                integrate(
                    |u| {
                        let (r, y) = (a0 + u * dr, a1 + u * dy);
                        [r * dy, r * r * dy, r * r * r * dy, r * y * dy, r * r * y * dy, r * l, l]
                    },
                    0.0,
                    1.0,
                )
            }
            Seg::Arc { r: big, a0, sweep, ends, .. } => {
                // The point at the angle `a0 + d` is taken from the nearer end (as the build
                // places it: `geom::Seg::at`), so a shallow arc whose centre is far off keeps its
                // digits; `d` runs from 0 to the sweep.
                let (e0, e1) = ([ends[0][0] - at[0], ends[0][1] - at[1]], [ends[1][0] - at[0], ends[1][1] - at[1]]);
                let m = ceil(sweep.abs() / (PI / 8.0)).max(1.0) as usize;
                let g = if *sweep >= 0.0 { 1.0 } else { -1.0 };
                let mut t = [0.0; 7];
                for i in 0..m {
                    let (t0, t1) = (sweep * i as f64 / m as f64, sweep * (i + 1) as f64 / m as f64);
                    let v = integrate(
                        |d| {
                            let p = if (d - 0.5 * sweep) * g <= 0.0 { add(e0, arc_step(*big, *a0, d)) } else { add(e1, arc_step(*big, a0 + sweep, d - sweep)) };
                            let (r, y, dy) = (p[0], p[1], big * cos(a0 + d));
                            [r * dy, r * r * dy, r * r * r * dy, r * y * dy, r * r * y * dy, r * big * g, big * g]
                        },
                        t0,
                        t1,
                    );
                    for k in 0..7 {
                        t[k] += v[k];
                    }
                }
                t
            }
            Seg::Fn { .. } => return None,
        };
        for k in 0..7 {
            s[k] += v[k];
        }
    }
    Some(s)
}

/// The least and greatest of `cos θ` and of `sin θ` for θ in `[0, a]`.
fn trig_ranges(a: f64) -> ((f64, f64), (f64, f64)) {
    let mut c = (1.0f64, 1.0f64);
    let mut s = (0.0f64, 0.0f64);
    let mut at = |t: f64| {
        let (sn, cs) = sin_cos(t);
        c = (c.0.min(cs), c.1.max(cs));
        s = (s.0.min(sn), s.1.max(sn));
    };
    at(a);
    for q in 1..4 {
        let t = q as f64 * 0.5 * PI;
        if t < a {
            // The exact values at the quarter turns.
            let (sn, cs) = [(1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)][q - 1];
            c = (c.0.min(cs), c.1.max(cs));
            s = (s.0.min(sn), s.1.max(sn));
        }
    }
    (c, s)
}

/// The bounds of `r` from `[r_lo, r_hi]` turned through `[0, a]` and stretched by `sx`, `sz`,
/// with `y` from `[y_lo, y_hi]`.
fn swept_bounds(r: (f64, f64), y: (f64, f64), a: f64, sx: f64, sz: f64) -> (P3, P3) {
    if a >= TAU {
        return ([-sx * r.1, y.0, -sz * r.1], [sx * r.1, y.1, sz * r.1]);
    }
    let ((cmin, cmax), (smin, smax)) = trig_ranges(a);
    let hi = |m: f64| if m >= 0.0 { r.1 * m } else { r.0 * m };
    let lo = |m: f64| if m >= 0.0 { r.0 * m } else { r.1 * m };
    ([sx * lo(cmin), y.0, -sz * hi(smax)], [sx * hi(cmax), y.1, -sz * lo(smin)])
}

// ---------------------------------------------------------------- polyhedra

/// A polyhedron of flat faces, exact: its points, its faces (counter-clockwise from outside),
/// the surface of each face, and the surfaces' names.
#[derive(Clone, Debug)]
pub(crate) struct Poly {
    pub pts: Vec<P3>,
    pub faces: Vec<Vec<u32>>,
    pub surf: Vec<u32>,
    pub names: Vec<String>,
}

/// The name of the surface of regular faces of `n` sides.
fn polygon_name(n: usize) -> String {
    match n {
        3 => "triangles".into(),
        4 => "squares".into(),
        5 => "pentagons".into(),
        6 => "hexagons".into(),
        8 => "octagons".into(),
        10 => "decagons".into(),
        _ => format!("faces_of_{n}"),
    }
}

impl Poly {
    /// The mesh.
    pub fn mesh(&self) -> Mesh {
        Mesh {
            points: self.pts.clone(),
            faces: self.faces.iter().zip(&self.surf).map(|(f, &s)| Face { verts: f.clone(), surface: s, normals: None }).collect(),
            surfaces: self.names.iter().map(|n| Surface { name: n.clone(), smooth: false, radius: None }).collect(),
            chord_error: 0.0,
        }
    }

    /// A polyhedron of these faces whose surfaces are its faces grouped by their number of
    /// sides (`triangles`, `squares`...), or all in one surface named `one`.
    pub fn grouped(pts: Vec<P3>, faces: Vec<Vec<u32>>, one: Option<&str>) -> Poly {
        let mut names: Vec<String> = Vec::new();
        let mut sizes: Vec<usize> = faces.iter().map(|f| f.len()).collect();
        sizes.sort_unstable();
        sizes.dedup();
        let surf = faces
            .iter()
            .map(|f| match one {
                Some(_) => 0,
                None => sizes.iter().position(|&s| s == f.len()).unwrap_or(0) as u32,
            })
            .collect();
        match one {
            Some(n) => names.push(n.to_string()),
            None => names.extend(sizes.iter().map(|&s| polygon_name(s))),
        }
        Poly { pts, faces, surf, names }
    }

    /// Every point mapped.
    pub fn map(&self, f: impl Fn(P3) -> P3) -> Poly {
        Poly { pts: self.pts.iter().map(|&p| f(p)).collect(), ..self.clone() }
    }

    /// Scaled about the origin by `s`, then lifted so that its lowest point is at `y = 0`.
    pub fn placed(&self, s: f64) -> Poly {
        let low = self.pts.iter().map(|p| p[1] * s).fold(f64::INFINITY, f64::min);
        self.map(|p| [p[0] * s, p[1] * s - low, p[2] * s])
    }

    /// The convex polyhedron whose edges join the points at the least distance apart (every
    /// edge of a Platonic or an Archimedean solid is the same length, and no two points are
    /// closer than an edge), its faces traced round each edge (centred on the origin). The
    /// table's tests hold the counts and the convexity of what this finds against the
    /// literature, so wrong coordinates cannot pass.
    pub fn by_edges(pts: Vec<P3>) -> Poly {
        let n = pts.len();
        let mut least = f64::INFINITY;
        for i in 0..n {
            for j in i + 1..n {
                least = least.min(dist3(pts[i], pts[j]));
            }
        }
        let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
        for i in 0..n {
            for j in 0..n {
                if i != j && dist3(pts[i], pts[j]) <= least * (1.0 + 1e-9) {
                    nb[i].push(j as u32);
                }
            }
        }
        let faces = trace(&pts, &mut nb);
        Poly::grouped(pts, faces, None)
    }

    /// The dual (polar reciprocal) in the sphere of radius √ρ² about the origin: a point for each
    /// face, at `ρ² / d` along its plane's unit normal (`d` the plane's distance), and a face for
    /// each point, through the points of the faces round it. Each new face lies in the plane
    /// `p · x = ρ²` of its old point `p`, so a polyhedron with every point at distance `R` gives
    /// a dual with every face at distance `ρ² / R`.
    pub fn dual(&self, rho2: f64, name: &str) -> Poly {
        let mesh = self.mesh();
        let pts: Vec<P3> = mesh
            .faces
            .iter()
            .map(|f| {
                let nrm = norm3(mesh.area_vector(f));
                let d = dot3(nrm, self.pts[f.verts[0] as usize]);
                scale3(nrm, rho2 / d)
            })
            .collect();
        // The face of each directed edge, then each point's faces in order round it.
        let mut edge_face: Vec<((u32, u32), u32)> = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for i in 0..f.len() {
                edge_face.push(((f[i], f[(i + 1) % f.len()]), fi as u32));
            }
        }
        edge_face.sort_unstable();
        let face_of = |a: u32, b: u32| edge_face[edge_face.binary_search_by(|e| e.0.cmp(&(a, b))).expect("closed")].1;
        let mut faces = Vec::new();
        for v in 0..self.pts.len() as u32 {
            // Start at any edge out of v; the face to its left; then the next edge out of v round
            // that face is the reverse of the edge into v in it.
            let start = edge_face.iter().find(|e| e.0 .0 == v).expect("every point has an edge").0;
            let mut out = Vec::new();
            let mut e = start;
            loop {
                let f = face_of(e.0, e.1);
                out.push(f);
                // In face f, the edge into v: (u, v); the next face round v is the one on the
                // other side of it, which has (v, u).
                let fv = &self.faces[f as usize];
                let k = fv.iter().position(|&x| x == v).expect("v is in its face");
                let u = fv[(k + fv.len() - 1) % fv.len()];
                e = (v, u);
                if e == start || out.len() > self.faces.len() {
                    break;
                }
            }
            faces.push(out);
        }
        Poly::grouped(pts, faces, Some(name))
    }
}

/// The faces of a convex polyhedron centred on the origin from its points and edges: each
/// point's neighbours are put in order counter-clockwise seen from outside, and a face is
/// traced by turning, at each point, to the neighbour before the one it came from.
fn trace(pts: &[P3], nb: &mut [Vec<u32>]) -> Vec<Vec<u32>> {
    for (i, list) in nb.iter_mut().enumerate() {
        let up = norm3(pts[i]);
        let helper = if up[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let e1 = norm3(cross3(helper, up));
        let e2 = cross3(up, e1);
        let angle = |j: u32| {
            let d = sub3(pts[j as usize], pts[i]);
            atan2(dot3(d, e2), dot3(d, e1))
        };
        list.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
    }
    let mut seen: Vec<(u32, u32)> = Vec::new();
    let mut faces = Vec::new();
    for u in 0..pts.len() as u32 {
        for &v in &nb[u as usize].clone() {
            if seen.contains(&(u, v)) {
                continue;
            }
            let mut face = vec![u];
            let (mut a, mut b) = (u, v);
            while b != u && face.len() <= pts.len() {
                seen.push((a, b));
                face.push(b);
                let l = &nb[b as usize];
                let k = l.iter().position(|&x| x == a).expect("edges are mutual");
                let next = l[(k + l.len() - 1) % l.len()];
                a = b;
                b = next;
            }
            seen.push((a, b));
            faces.push(face);
        }
    }
    faces
}

impl Make for Poly {
    fn count(&self, _: u32) -> (u64, u64) {
        (self.pts.len() as u64, self.faces.len() as u64)
    }
    fn build(&self, _: u32) -> (Mesh, Option<SolidFault>) {
        (self.mesh(), None)
    }
    fn exact(&self) -> Measures {
        self.mesh().measures()
    }
    fn error(&self, _: u32) -> Option<f64> {
        Some(0.0)
    }
    fn genus(&self) -> u32 {
        0
    }
    fn curved(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------- revolution

/// A profile normal's function: the outward normal (any length) at a parametric piece's
/// parameter.
pub(crate) type NFn = Rc<dyn Fn(f64) -> Pt>;

/// A support function: the greatest `d · p` over the convex curve a parametric piece lies on, for
/// a unit direction `d`, in the piece's own coordinates.
pub(crate) type SFn = Rc<dyn Fn(Pt) -> f64>;

/// One piece of a profile: its geometry, the surface it turns into, and for a parametric piece
/// its outward normal and, where it lies on a convex curve whose support function is known in
/// closed form, that function.
///
/// **A parametric piece with a normal turns monotonically, and by under a quarter turn from one
/// sample to the next** (every one in the table does: each is cut where its normal would turn
/// further). Then the chord of each step has a normal between its ends' normals (the mean value
/// theorem: the chord's direction is the curve's tangent somewhere between), less than a
/// quarter turn from each, and the step lies in the triangle of its chord and its end tangents.
#[derive(Clone)]
pub(crate) struct Piece {
    pub seg: Seg,
    pub name: std::borrow::Cow<'static, str>,
    pub normal: Option<NFn>,
    pub support: Option<SFn>,
}

/// A piece whose normal follows from its geometry (a line or an arc).
pub(crate) fn piece(seg: Seg, name: &'static str) -> Piece {
    Piece { seg, name: name.into(), normal: None, support: None }
}

/// A parametric piece with its outward normal.
pub(crate) fn fpiece(seg: Seg, name: &'static str, normal: impl Fn(f64) -> Pt + 'static) -> Piece {
    Piece { seg, name: name.into(), normal: Some(Rc::new(normal)), support: None }
}

impl Piece {
    /// A flat piece turns into a flat surface: a line at one height.
    pub fn flat(&self) -> bool {
        matches!(self.seg, Seg::Line { a, b } if a[1] == b[1])
    }
    /// The outward unit normal at sample `i` of `k` (outward: to the right of the way the
    /// profile runs, the region being on its left).
    pub fn normal_at(&self, i: u64, k: u64) -> Option<Pt> {
        let u = i as f64 / k as f64;
        let unit = |n: Pt| {
            let l = len(n);
            [n[0] / l, n[1] / l]
        };
        match &self.seg {
            Seg::Line { a, b } => Some(unit([b[1] - a[1], a[0] - b[0]])),
            Seg::Arc { a0, sweep, .. } => {
                let (s, c) = sin_cos(a0 + sweep * u);
                let g = if *sweep >= 0.0 { 1.0 } else { -1.0 };
                Some([g * c, g * s])
            }
            Seg::Fn { t0, t1, .. } => self.normal.as_ref().map(|f| unit(f(t0 + (t1 - t0) * u))),
        }
    }
    /// The greatest distance between step `i` of `k` (its samples `a` and `b`) and its chord,
    /// and whether that is a bound (`true`) or only the piece's estimate `est` (`false`): 0 for a
    /// line; an arc's sag `r (1 − cos(δ / 2))`, exactly; for a parametric piece on a convex curve
    /// with a support function `h`, `h(ν) − ν · a` for the chord's outward normal `ν`, exactly
    /// (the curve's farthest point out along `ν` is the one whose normal is `ν`, which lies in
    /// this step: see [`Piece`]); for one with a normal, `(L / 2) tan(δ / 2)` for the chord's
    /// length `L` and the normal's turn `δ` across the step, a bound (the step lies in the
    /// triangle of its chord and its end tangents, whose base angles sum to `δ`, and whose height
    /// is greatest when they are equal).
    fn step_sag(&self, i: u64, k: u64, a: Pt, b: Pt, est: f64) -> (f64, bool) {
        match &self.seg {
            Seg::Line { .. } => (0.0, true),
            Seg::Arc { r, sweep, .. } => (r * versin(sweep.abs() / (2 * k) as f64), true),
            Seg::Fn { .. } => {
                let d = sub(b, a);
                let l = len(d);
                if let (Some(h), true) = (&self.support, l > 0.0) {
                    let nu = [d[1] / l, -d[0] / l];
                    return ((h(nu) - (nu[0] * a[0] + nu[1] * a[1])).max(0.0), true);
                }
                if let (Some(na), Some(nb)) = (self.normal_at(i, k), self.normal_at(i + 1, k)) {
                    let turn = atan2((na[0] * nb[1] - na[1] * nb[0]).abs(), na[0] * nb[0] + na[1] * nb[1]);
                    if turn < 0.5 * PI {
                        return (0.5 * l * tan(0.5 * turn), true);
                    }
                }
                (est, false)
            }
        }
    }
}

/// `sgn(v) |v|^e`.
fn spow(v: f64, e: f64) -> f64 {
    if v < 0.0 {
        -powf(-v, e)
    } else {
        powf(v, e)
    }
}

/// The point and the outward unit normal of the superellipse `|x / a|^n + |y / b|^n = 1` at the
/// parameter whose cosine and sine are `c` and `s`: `(a sgn(c) |c|^(2/n), b sgn(s) |s|^(2/n))`,
/// its normal along `(sgn(c) |c|^(2(n−1)/n) / a, sgn(s) |s|^(2(n−1)/n) / b)` (the gradient). At
/// `n = 2` it is an ellipse, taken without powers.
pub(crate) fn superellipse(a: f64, b: f64, n: f64, c: f64, s: f64) -> (Pt, Pt) {
    let (p, g) = if n == 2.0 {
        ([a * c, b * s], [c / a, s / b])
    } else {
        let (e, f) = (2.0 / n, 2.0 * (n - 1.0) / n);
        ([a * spow(c, e), b * spow(s, e)], [spow(c, f) / a, spow(s, f) / b])
    };
    let l = len(g);
    (p, [g[0] / l, g[1] / l])
}

/// The support function of the superellipse of semi-axes `a`, `b` and exponent `n` (its
/// greatest `d · p`) in the unit direction `d`: `(|a d₀|^q + |b d₁|^q)^(1/q)`, `q = n / (n − 1)`
/// (Hölder's inequality, with equality where the normal is `d`).
pub(crate) fn superellipse_support(a: f64, b: f64, n: f64, d: Pt) -> f64 {
    let (x, y) = ((a * d[0]).abs(), (b * d[1]).abs());
    if n == 2.0 {
        return (x * x + y * y).sqrt();
    }
    let m = x.max(y);
    if m == 0.0 {
        return 0.0;
    }
    let q = n / (n - 1.0);
    m * powf(powf(x / m, q) + powf(y / m, q), 1.0 / q)
}

/// `(cos, sin)` of the quarter turns `t` (from +x, counter-clockwise), with the axis points
/// exact: the quadrant `⌊t⌋`'s turn of `sin_cos` of what is left over.
pub(crate) fn quarter_turns(t: f64) -> (f64, f64) {
    let q = floor(t);
    let f = t - q;
    let (s, c) = if f == 0.0 { (0.0, 1.0) } else { sin_cos(0.5 * PI * f) };
    match (q as i64).rem_euclid(4) {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    }
}

/// The ring a profile is turned on: the solid's point for the profile's `(r, y)` at the ring's
/// point `e` is `(r e₀, y, −r e₁)`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Ring {
    /// The unit circle, at angles evenly through the sweep (a solid of revolution).
    Circle,
    /// The superellipse `|x / a|^n + |z / b|^n = 1` (an ellipse at `n = 2`), a full turn only, each
    /// quadrant cut evenly in its own parameter into the segments of a quarter turn, so that its
    /// four points on the axes are samples. The solid's section at every height is the same
    /// superellipse scaled by `r`.
    Super { a: f64, b: f64, n: f64 },
}

/// A ring's samples: each column's point `e`, the outward unit normal `m` there and `h = e · m`;
/// the normal and `h` at the middle of each step (a pole's normals); the greatest sag of a step's
/// chord for `r = 1`; and the greatest `|e|` over the whole true ring (how far the ring scales a
/// distance in `r`).
struct RingS {
    e: Vec<Pt>,
    m: Vec<Pt>,
    h: Vec<f64>,
    mid: Vec<(Pt, f64)>,
    sag: f64,
    reach: f64,
}

/// A solid of revolution, or of a profile turned on a superellipse (see the module's
/// documentation).
#[derive(Clone)]
pub(crate) struct Rev {
    /// The profile, counter-clockwise in `(r, y)` (the region on its left): from the axis to
    /// the axis when open, a ring off the axis when closed. In its own coordinates: the solid's
    /// `(r, y)` is the profile's point plus `shift`.
    pub pieces: Vec<Piece>,
    pub closed: bool,
    /// The angle turned through, radians, up to 2π (a full turn for a superellipse ring).
    pub sweep: f64,
    /// What it is turned on.
    pub ring: Ring,
    /// Where the profile's own origin is in `(r, y)`: a closed profile's measures are taken in
    /// its own coordinates, so that a thin section far from the axis keeps its digits. An open
    /// one, which reaches the axis, has none.
    pub shift: Pt,
    /// The name of the two flat faces where a partial turn is cut.
    pub cut: &'static str,
}

/// The sampled profile: its points (with the shift), for each step its piece and its sample `i`
/// of `k`, each step's sag (see [`Piece::step_sag`]), and whether every sag is a bound.
struct Prof {
    q: Vec<Pt>,
    step: Vec<(usize, u64, u64)>,
    sag: Vec<f64>,
    sure: bool,
}

impl Rev {
    fn full(&self) -> bool {
        self.sweep >= TAU
    }
    fn turns(&self, detail: u32) -> u64 {
        match self.ring {
            Ring::Circle => segments(self.sweep.min(TAU) / TAU, detail),
            Ring::Super { .. } => 4 * segments(0.25, detail),
        }
    }
    fn m(&self, detail: u32) -> u64 {
        self.pieces.iter().map(|p| p.seg.k(detail)).sum()
    }
    fn discs(&self) -> (bool, bool) {
        if self.closed || !self.full() {
            return (false, false);
        }
        let first = self.pieces.first().map(|p| p.flat() && p.seg.start()[0] == 0.0).unwrap_or(false);
        let last = self.pieces.last().map(|p| p.flat() && p.seg.end()[0] == 0.0).unwrap_or(false);
        (first, last)
    }
    fn sample(&self, detail: u32) -> Prof {
        let mut q = Vec::new();
        let mut step = Vec::new();
        let mut sag = Vec::new();
        let mut sure = true;
        for (pi, p) in self.pieces.iter().enumerate() {
            let k = p.seg.k(detail);
            let est = p.seg.emit(detail, &mut q);
            for i in 0..k {
                let (s, b) = p.step_sag(i, k, p.seg.at(i, k), p.seg.at(i + 1, k), est);
                sag.push(s);
                sure &= b;
                step.push((pi, i, k));
            }
        }
        if !self.closed {
            if let Some(p) = self.pieces.last() {
                q.push(p.seg.end());
            }
            // An open profile's ends are on the axis. An arc lands on its pole exactly (its ends
            // are given, or are its circle's points at a quarter turn, which `polar` takes
            // exactly); a parametric piece's end may be a few parts in 1e17 of the profile's
            // reach off it, which is put on it. (The reach is the profile's own: an arc's far
            // centre once made it, and so would have put a point a metre off the axis on it.)
            let reach = q.iter().flatten().map(|x| x.abs()).fold(0.0f64, f64::max);
            let last = q.len() - 1;
            for i in [0, last] {
                if q[i][0].abs() <= 1e-12 * reach {
                    q[i][0] = 0.0;
                }
            }
        }
        if self.shift != [0.0, 0.0] {
            for p in &mut q {
                *p = [p[0] + self.shift[0], p[1] + self.shift[1]];
            }
        }
        Prof { q, step, sag, sure }
    }
    fn ring_samples(&self, detail: u32) -> RingS {
        let na = self.turns(detail) as usize;
        match self.ring {
            Ring::Circle => {
                let full = self.full();
                let ang = |i: f64| if full { TAU * i / na as f64 } else { self.sweep * i / na as f64 };
                let cols = if full { na } else { na + 1 };
                let e: Vec<Pt> = (0..cols)
                    .map(|i| {
                        let (s, c) = sin_cos(ang(i as f64));
                        [c, s]
                    })
                    .collect();
                let mid = (0..na)
                    .map(|i| {
                        let (s, c) = sin_cos(0.5 * (ang(i as f64) + ang(i as f64 + 1.0)));
                        ([c, s], 1.0)
                    })
                    .collect();
                RingS { m: e.clone(), h: vec![1.0; cols], e, mid, sag: versin(0.5 * self.sweep.min(TAU) / na as f64), reach: 1.0 }
            }
            Ring::Super { a, b, n } => {
                let k = na / 4;
                let at = |t: f64| {
                    let (c, s) = quarter_turns(t);
                    let (p, m) = superellipse(a, b, n, c, s);
                    (p, m, p[0] * m[0] + p[1] * m[1])
                };
                let (mut e, mut m, mut h, mut mid) = (Vec::with_capacity(na), Vec::with_capacity(na), Vec::with_capacity(na), Vec::with_capacity(na));
                for i in 0..na {
                    let (q, j) = ((i / k) as f64, (i % k) as f64);
                    let (p, nn, hh) = at(q + j / k as f64);
                    e.push(p);
                    m.push(nn);
                    h.push(hh);
                    let (_, nm, hm) = at(q + (j + 0.5) / k as f64);
                    mid.push((nm, hm));
                }
                // Each chord's sag, exactly: the support function at its outward normal, less the
                // chord's own distance along it.
                let mut sag = 0.0f64;
                for i in 0..na {
                    let (p, r) = (e[i], e[(i + 1) % na]);
                    let d = sub(r, p);
                    let l = len(d);
                    let nu = [d[1] / l, -d[0] / l];
                    sag = sag.max(superellipse_support(a, b, n, nu) - (nu[0] * p[0] + nu[1] * p[1]));
                }
                // The ring lies in its ellipse (n <= 2) or in its box (n > 2).
                let reach = if n <= 2.0 { a.max(b) } else { (a * a + b * b).sqrt() };
                RingS { e, m, h, mid, sag, reach }
            }
        }
    }
    fn segs(&self) -> Vec<Seg> {
        self.pieces.iter().map(|p| p.seg.clone()).collect()
    }
    /// The build's error from its samples: the profile's greatest sag, each step's scaled by the
    /// ring's reach `a` as a distance in `r` is (`a L / √(a² Δr² + Δy²)` for a step of length `L`:
    /// the meridian at a point `e` of the ring is the profile with `r` scaled by `|e| <= a`, and a
    /// chord's sag grows with that scale), plus the ring's greatest sag at the greatest `r`
    /// sampled. Each term bounds how far one surface lies from the next: the true surface from
    /// the profile's chords turned on the true ring (the first), and those from the faces, whose
    /// section at each height is the chord surface's with the ring's chords (the second). It is
    /// a bound where every step's sag is one ([`Prof::sure`]).
    fn bound(&self, pr: &Prof, ring: &RingS) -> f64 {
        let n = pr.q.len();
        let a = ring.reach;
        let mut profile = 0.0f64;
        for (j, &s) in pr.sag.iter().enumerate() {
            let scaled = if a == 1.0 {
                s
            } else {
                let (p, r) = (pr.q[j], pr.q[(j + 1) % n]);
                let (dr, dy) = (r[0] - p[0], r[1] - p[1]);
                let l = (dr * dr + dy * dy).sqrt();
                let la = (a * a * dr * dr + dy * dy).sqrt();
                if la > 0.0 {
                    s * a * l / la
                } else {
                    s * a.max(1.0)
                }
            };
            profile = profile.max(scaled);
        }
        let rmax = pr.q.iter().map(|p| p[0]).fold(0.0f64, f64::max);
        profile + rmax * ring.sag
    }
}

/// A plane figure's fault as a solid's section fault.
pub(crate) fn section_fault(f: Fault) -> SolidFault {
    match f {
        Fault::ShortEdge { point, length, .. } => SolidFault::SectionShortEdge { point, length },
        Fault::Crossing { a, b } => SolidFault::SectionCrossing { a, b },
        Fault::Gap { a, b, gap } => SolidFault::SectionGap { a, b, gap },
        _ => SolidFault::TooFew { face: 0 },
    }
}

#[derive(Clone, Copy)]
enum Row {
    None,
    Pole(u32),
    Ring(u32),
}

/// **Why no two faces cross, and no normal lies behind its face.** The profile is checked as a
/// simple ring (counter-clockwise, off the axis but at its ends), and a superellipse ring's
/// samples turn strictly counter-clockwise about the axis, each quadrant's in its own quadrant
/// (checked): then the build's section at each height is the profile's chord there scaled on
/// the sampled ring, a simple polygon, and two faces of one band of heights lie in different
/// angles. A face between two rows is a trapezoid with its parallel sides on the rows; in the
/// plane of the step's middle angle its normal is along `(Δy μ, −Δr h_M)` (`μ` the ring chord's
/// outward normal, `h_M` its middle's distance along it), and a corner's normal along
/// `(n_r m, n_y h)` (the profile's normal `(n_r, n_y)` and the ring's `m` and `h` there). Their
/// product is `n_r Δy (m · μ) − n_y Δr h h_M`. On a circle (`m · μ = cos(δ/2)`, `h = h_M`
/// = 1 up to that cosine) it is `cos(δ/2)` times `n · (Δy, −Δr)`, positive where the profile's
/// normal at each end of a step is less than a quarter turn from the chord's (an arc's turn
/// each step is at most an eighth; a parametric piece's under a quarter, see [`Piece`]). On a
/// superellipse `m · μ > 0` (each step is in one quadrant, where the normal turns under a quarter
/// turn) and `h, h_M > 0`; a profile whose every step stays in one quadrant of `(r, y)` (the
/// superellipsoid's and the ellipsoid's are cut at the equator) has `n_r Δy >= 0` and
/// `−n_y Δr >= 0`, not both zero. [`Mesh::validate`] holds every build to it all the same.
impl Make for Rev {
    fn pieces(&self) -> Vec<Seg> {
        self.segs()
    }
    fn count(&self, detail: u32) -> (u64, u64) {
        let (m, na) = (self.m(detail), self.turns(detail));
        let (d0, d1) = self.discs();
        match (self.closed, self.full()) {
            (true, true) => (m * na, m * na),
            (true, false) => (m * (na + 1), m * na + 2),
            (false, true) => {
                let pts = (m - 1) * na + u64::from(!d0) + u64::from(!d1);
                let faces = (m - 2) * na + if d0 { 1 } else { na } + if d1 { 1 } else { na };
                (pts, faces)
            }
            (false, false) => ((m - 1) * (na + 1) + 2, m * na + 2),
        }
    }

    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>) {
        let pr = self.sample(detail);
        let ring = self.ring_samples(detail);
        let m = pr.step.len();
        // The profile, closed along the axis when open, must be a simple ring
        // counter-clockwise, off the axis but at its ends.
        let mut fault = Outline { rings: vec![pr.q.clone()], chord_error: 0.0 }.validate().err().map(section_fault);
        if !self.closed && fault.is_none() && (pr.q[0][0] != 0.0 || pr.q[m][0] != 0.0 || pr.q[1..m].iter().any(|p| p[0] <= 0.0)) {
            fault = Some(SolidFault::TooFew { face: 0 });
        }
        // A superellipse ring's samples turn strictly counter-clockwise, each in its quadrant.
        if let (Ring::Super { .. }, None) = (self.ring, &fault) {
            let na = ring.e.len();
            let k = na / 4;
            let quadrant = [[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]];
            for i in 0..na {
                let (p, r) = (ring.e[i], ring.e[(i + 1) % na]);
                let [sx, sy] = quadrant[i / k];
                let inside = p[0] * sx >= 0.0 && p[1] * sy >= 0.0;
                if !(p[0] * r[1] - p[1] * r[0] > 0.0 && inside) {
                    fault = Some(SolidFault::SectionCrossing { a: (1, i), b: (1, (i + 1) % na) });
                    break;
                }
            }
        }
        let full = self.full();
        let na = self.turns(detail) as usize;
        let cols = if full { na } else { na + 1 };
        let at = |p: Pt, i: usize| [p[0] * ring.e[i][0], p[1], -(p[0] * ring.e[i][1])];
        let turn_n = |n: Pt, m: Pt, h: f64| norm3([n[0] * m[0], n[1] * h, -(n[0] * m[1])]);
        let nrm = |n: Pt, i: usize| turn_n(n, ring.m[i % cols], ring.h[i % cols]);
        let mid = |n: Pt, i: usize| turn_n(n, ring.mid[i].0, ring.mid[i].1);
        let (d0, d1) = self.discs();
        let nrow = if self.closed { m } else { m + 1 };
        let mut points: Vec<P3> = Vec::new();
        let mut rows = Vec::with_capacity(nrow);
        for (j, &p) in pr.q.iter().enumerate().take(nrow) {
            let end = !self.closed && (j == 0 || j == m);
            if end && full && ((j == 0 && d0) || (j == m && d1)) {
                rows.push(Row::None);
            } else if end {
                rows.push(Row::Pole(points.len() as u32));
                points.push([0.0, p[1], 0.0]);
            } else {
                rows.push(Row::Ring(points.len() as u32));
                points.extend((0..cols).map(|i| at(p, i)));
            }
        }
        // Surfaces: one a name; a smooth one's radius is the greatest it turns at.
        let mut surfaces: Vec<Surface> = Vec::new();
        let mut of_piece = Vec::new();
        for (pi, p) in self.pieces.iter().enumerate() {
            let rmax = pr.step.iter().enumerate().filter(|(_, s)| s.0 == pi).flat_map(|(j, _)| [pr.q[j][0], pr.q[(j + 1) % pr.q.len()][0]]).fold(0.0f64, f64::max) * ring.reach;
            let arc = if let Seg::Arc { r, .. } = p.seg { r } else { 0.0 };
            let radius = (!p.flat()).then_some(rmax.max(arc));
            match surfaces.iter().position(|s| s.name == p.name) {
                Some(i) => {
                    if let (Some(a), Some(b)) = (surfaces[i].radius, radius) {
                        surfaces[i].radius = Some(a.max(b));
                    }
                    of_piece.push(i as u32);
                }
                None => {
                    of_piece.push(surfaces.len() as u32);
                    surfaces.push(Surface { name: p.name.to_string(), smooth: !p.flat(), radius });
                }
            }
        }
        let mut faces = Vec::new();
        let idx = |base: u32, i: usize| base + (i % cols) as u32;
        for j in 0..m {
            let (pi, i0, k) = pr.step[j];
            let p = &self.pieces[pi];
            let surface = of_piece[pi];
            let smooth = !p.flat();
            let (na_n, nb_n) = (p.normal_at(i0, k), p.normal_at(i0 + 1, k));
            let ns = |v: Vec<P3>| (smooth && na_n.is_some() && nb_n.is_some()).then_some(v);
            let (a_n, b_n) = (na_n.unwrap_or([1.0, 0.0]), nb_n.unwrap_or([1.0, 0.0]));
            match (rows[j], rows[(j + 1) % nrow]) {
                (Row::Ring(a), Row::Ring(b)) => {
                    for i in 0..na {
                        faces.push(Face { verts: vec![idx(a, i), idx(a, i + 1), idx(b, i + 1), idx(b, i)], surface, normals: ns(vec![nrm(a_n, i), nrm(a_n, i + 1), nrm(b_n, i + 1), nrm(b_n, i)]) });
                    }
                }
                (Row::Pole(pv), Row::Ring(b)) => {
                    for i in 0..na {
                        faces.push(Face { verts: vec![pv, idx(b, i + 1), idx(b, i)], surface, normals: ns(vec![mid(a_n, i), nrm(b_n, i + 1), nrm(b_n, i)]) });
                    }
                }
                (Row::Ring(a), Row::Pole(pv)) => {
                    for i in 0..na {
                        faces.push(Face { verts: vec![idx(a, i), idx(a, i + 1), pv], surface, normals: ns(vec![nrm(a_n, i), nrm(a_n, i + 1), mid(b_n, i)]) });
                    }
                }
                (Row::None, Row::Ring(b)) => faces.push(Face { verts: (0..na).rev().map(|i| idx(b, i)).collect(), surface, normals: None }),
                (Row::Ring(a), Row::None) => faces.push(Face { verts: (0..na).map(|i| idx(a, i)).collect(), surface, normals: None }),
                _ => fault = fault.or(Some(SolidFault::TooFew { face: faces.len() })),
            }
        }
        if !full {
            let cut = surfaces.len() as u32;
            surfaces.push(Surface { name: self.cut.to_string(), smooth: false, radius: None });
            let column = |i: usize| -> Vec<u32> {
                rows.iter()
                    .filter_map(|r| match r {
                        Row::Pole(v) => Some(*v),
                        Row::Ring(b) => Some(idx(*b, i)),
                        Row::None => None,
                    })
                    .collect()
            };
            faces.push(Face { verts: column(0), surface: cut, normals: None });
            let mut last = column(na);
            last.reverse();
            faces.push(Face { verts: last, surface: cut, normals: None });
        }
        let chord = self.bound(&pr, &ring);
        (Mesh { points, faces, surfaces, chord_error: chord }, fault)
    }

    /// The exact measures, where the profile is lines and arcs. A closed profile (a ring off the
    /// axis) is measured in its own coordinates: its volume by Pappus's theorem, `α A (x̄ + s)`,
    /// from its area `A` and centroid `x̄` as a plane figure's are taken (Green's theorem, the same
    /// code) and the shift `s`, so that a section far out keeps its digits and a figure turned
    /// by `revolve_of` has the volume its own measures give; its centroid and area from
    /// [`profile_integrals`] about its own origin. An open profile is integrated as it lies.
    fn exact(&self) -> Measures {
        let segs = self.segs();
        let refs: Vec<&Seg> = segs.iter().collect();
        let at = if self.closed { [0.0, 0.0] } else { [-self.shift[0], -self.shift[1]] };
        let Some([j0, j1, j2, j3, j4, j5, j6]) = profile_integrals(&refs, at) else { return Measures::default() };
        let (r0, y0) = if self.closed { (self.shift[0], self.shift[1]) } else { (0.0, 0.0) };
        // With r = r0 + ρ and y = y0 + η, and ∮ dη = ∮ η dη = 0 round a closed profile:
        // ∮ r² dy = 2 r0 ∮ ρ dη + ∮ ρ² dη; ∮ r³ dy = 3 r0² ∮ ρ dη + 3 r0 ∮ ρ² dη + ∮ ρ³ dη;
        // ∮ r² y dy = y0 ∮ r² dy + 2 r0 ∮ ρ η dη + ∮ ρ² η dη; ∫ r ds = r0 ∫ ds + ∫ ρ ds.
        let i1 = 2.0 * r0 * j0 + j1;
        let i3 = 3.0 * r0 * r0 * j0 + 3.0 * r0 * j1 + j2;
        let iy = 2.0 * r0 * j3 + j4;
        let s = r0 * j6 + j5;
        let a = self.sweep.min(TAU);
        let full = self.full();
        let bounds = Geom::Path { segs: segs.clone(), closed: self.closed }.exact().bounds.map(|(lo, hi)| {
            let (lo, hi) = ([lo[0] + self.shift[0], lo[1] + self.shift[1]], [hi[0] + self.shift[0], hi[1] + self.shift[1]]);
            let rmin = if self.closed { lo[0] } else { 0.0 };
            match self.ring {
                Ring::Circle => swept_bounds((rmin, hi[0]), (lo[1], hi[1]), a, 1.0, 1.0),
                Ring::Super { a: ea, b: eb, .. } => ([-ea * hi[0], lo[1], -eb * hi[0]], [ea * hi[0], hi[1], eb * hi[0]]),
            }
        });
        let centroid_y = y0 + iy / i1;
        match self.ring {
            Ring::Super { a: ea, b: eb, n } => {
                let g1 = gamma(1.0 + 1.0 / n);
                let area_e = if n == 2.0 { PI * ea * eb } else { 4.0 * ea * eb * g1 * g1 / gamma(1.0 + 2.0 / n) };
                Measures { size: Some(area_e * i1), boundary: None, centroid: Some([0.0, centroid_y, 0.0]), bounds, series: None }
            }
            Ring::Circle => {
                let volume = if self.closed {
                    let g = Geom::Rings(vec![segs]).exact();
                    match (g.size, g.centroid) {
                        (Some(ar), Some(c)) => a * ar * (c[0] + self.shift[0]),
                        _ => 0.5 * a * i1,
                    }
                } else {
                    0.5 * a * i1
                };
                let area = Some(a * s + if full { 0.0 } else { 2.0 * j0 });
                let (cx, cz) = if full {
                    (0.0, 0.0)
                } else {
                    let h = sin(0.5 * a);
                    ((i3 / 3.0) * sin(a) / (0.5 * a * i1), -(i3 / 3.0) * 2.0 * h * h / (0.5 * a * i1))
                };
                Measures { size: Some(volume), boundary: area, centroid: Some([cx, centroid_y, cz]), bounds, series: None }
            }
        }
    }

    fn error(&self, detail: u32) -> Option<f64> {
        let pr = self.sample(detail);
        pr.sure.then(|| self.bound(&pr, &self.ring_samples(detail)))
    }

    fn genus(&self) -> u32 {
        u32::from(self.closed && self.full())
    }

    fn curved(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------- outlines raised

/// The samples of one ring of pieces, with each step's piece and sample.
fn ring_samples(segs: &[Seg], detail: u32) -> (Vec<Pt>, Vec<(usize, u64, u64)>, f64) {
    let mut q = Vec::new();
    let mut step = Vec::new();
    let mut err = 0.0f64;
    for (si, s) in segs.iter().enumerate() {
        let k = s.k(detail);
        err = err.max(s.emit(detail, &mut q));
        step.extend((0..k).map(|i| (si, i, k)));
    }
    (q, step, err)
}

/// The outward normal in the plane at sample `i` of `k` of a ring's piece, where its geometry
/// gives it (a line's, an arc's); `None` for a parametric piece.
fn plane_normal(s: &Seg, i: u64, k: u64) -> Option<Pt> {
    match s {
        Seg::Fn { .. } => None,
        _ => piece(s.clone(), "").normal_at(i, k),
    }
}

/// A plane outline (one ring, counter-clockwise in `(u, v)`) raised straight up by `height`: a
/// prism. The outline's `(u, v)` is `(x, −z)`, so seen from above (+Y) it is as drawn; the
/// prism stands on `y = 0`.
#[derive(Clone)]
pub(crate) struct Extrude {
    pub segs: Vec<Seg>,
    pub height: f64,
    /// The outline's own closed measures (area, perimeter, centroid, bounds).
    pub plane: Measures,
}

impl Make for Extrude {
    fn count(&self, detail: u32) -> (u64, u64) {
        let n: u64 = self.segs.iter().map(|s| s.k(detail)).sum();
        (2 * n, n + 2)
    }
    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>) {
        let (q, step, err) = ring_samples(&self.segs, detail);
        let fault = Outline { rings: vec![q.clone()], chord_error: 0.0 }.validate().err().map(section_fault);
        let n = q.len();
        let h = self.height;
        let mut points: Vec<P3> = q.iter().map(|p| [p[0], 0.0, -p[1]]).collect();
        points.extend(q.iter().map(|p| [p[0], h, -p[1]]));
        let mut surfaces = vec![
            Surface { name: "bottom".into(), smooth: false, radius: None },
            Surface { name: "top".into(), smooth: false, radius: None },
            Surface { name: "sides".into(), smooth: false, radius: None },
        ];
        let mut of_seg = Vec::new();
        for (si, s) in self.segs.iter().enumerate() {
            match s {
                Seg::Line { .. } => of_seg.push(2u32),
                Seg::Arc { r, .. } => {
                    of_seg.push(surfaces.len() as u32);
                    surfaces.push(Surface { name: format!("side_{si}"), smooth: true, radius: Some(*r) });
                }
                Seg::Fn { .. } => {
                    of_seg.push(surfaces.len() as u32);
                    surfaces.push(Surface { name: format!("side_{si}"), smooth: true, radius: None });
                }
            }
        }
        let mut faces = vec![
            Face { verts: (0..n as u32).rev().collect(), surface: 0, normals: None },
            Face { verts: (n as u32..2 * n as u32).collect(), surface: 1, normals: None },
        ];
        for (i, &(si, j, k)) in step.iter().enumerate() {
            let (a, b) = (i as u32, ((i + 1) % n) as u32);
            let s = &self.segs[si];
            let normals = match (s, plane_normal(s, j, k), plane_normal(s, j + 1, k)) {
                (Seg::Arc { .. }, Some(na), Some(nb)) => {
                    let (na, nb) = ([na[0], 0.0, -na[1]], [nb[0], 0.0, -nb[1]]);
                    Some(vec![na, nb, nb, na])
                }
                _ => None,
            };
            faces.push(Face { verts: vec![a, b, b + n as u32, a + n as u32], surface: of_seg[si], normals });
        }
        (Mesh { points, faces, surfaces, chord_error: err }, fault)
    }
    fn exact(&self) -> Measures {
        let p = &self.plane;
        let h = self.height;
        Measures {
            size: p.size.map(|a| a * h),
            boundary: match (p.size, p.boundary) {
                (Some(a), Some(l)) => Some(2.0 * a + l * h),
                _ => None,
            },
            centroid: p.centroid.map(|c| [c[0], 0.5 * h, -c[1]]),
            bounds: p.bounds.map(|(lo, hi)| ([lo[0], 0.0, -hi[1]], [hi[0], h, -lo[1]])),
            series: p.series,
        }
    }
    fn error(&self, detail: u32) -> Option<f64> {
        Geom::Rings(vec![self.segs.clone()]).sag(detail)
    }
    fn genus(&self) -> u32 {
        0
    }
    fn curved(&self) -> bool {
        self.segs.iter().any(|s| !matches!(s, Seg::Line { .. }))
    }
}

/// A plane outline (as for [`Extrude`]) raised to an apex `height` over its origin: a pyramid,
/// or a cone over a curved outline. The outline must be seen whole from its origin (every edge
/// turning counter-clockwise about it); the table's own pyramids are, and the operator refuses
/// an outline that is not.
#[derive(Clone)]
pub(crate) struct Cone {
    pub segs: Vec<Seg>,
    pub height: f64,
    pub plane: Measures,
    /// The lateral area, where it is known in closed form.
    pub lateral: Option<f64>,
}

impl Make for Cone {
    fn count(&self, detail: u32) -> (u64, u64) {
        let n: u64 = self.segs.iter().map(|s| s.k(detail)).sum();
        (n + 1, n + 1)
    }
    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>) {
        let (q, step, err) = ring_samples(&self.segs, detail);
        let fault = Outline { rings: vec![q.clone()], chord_error: 0.0 }.validate().err().map(section_fault);
        let n = q.len();
        let apex = [0.0, self.height, 0.0];
        // The apex sees every edge from inside: each turns counter-clockwise about the origin.
        let fault = fault.or_else(|| (0..n).find(|&i| q[i][0] * q[(i + 1) % n][1] - q[i][1] * q[(i + 1) % n][0] <= 0.0).map(|edge| SolidFault::ApexOutside { edge }));
        let mut points: Vec<P3> = q.iter().map(|p| [p[0], 0.0, -p[1]]).collect();
        points.push(apex);
        let mut surfaces = vec![Surface { name: "base".into(), smooth: false, radius: None }, Surface { name: "sides".into(), smooth: false, radius: None }];
        let mut of_seg = Vec::new();
        for (si, s) in self.segs.iter().enumerate() {
            match s {
                Seg::Line { .. } => of_seg.push(1u32),
                _ => {
                    of_seg.push(surfaces.len() as u32);
                    let r = if let Seg::Arc { r, .. } = s { Some(*r) } else { None };
                    surfaces.push(Surface { name: format!("side_{si}"), smooth: true, radius: r });
                }
            }
        }
        let mut faces = vec![Face { verts: (0..n as u32).rev().collect(), surface: 0, normals: None }];
        for (i, &(si, j, k)) in step.iter().enumerate() {
            let (a, b) = (i, (i + 1) % n);
            let s = &self.segs[si];
            // A generator's normal: the outline's tangent there crossed with the way to the
            // apex (the same all along the generator); at the apex, where the surface has none,
            // the mean of the face's two.
            let gen = |p: Pt, nrm: Pt| {
                let t = [-nrm[1], 0.0, -nrm[0]];
                norm3(cross3(t, sub3(apex, [p[0], 0.0, -p[1]])))
            };
            let normals = match (s, plane_normal(s, j, k), plane_normal(s, j + 1, k)) {
                (Seg::Arc { .. }, Some(na), Some(nb)) => {
                    let (ga, gb) = (gen(q[a], na), gen(q[b], nb));
                    Some(vec![ga, gb, norm3(add3(ga, gb))])
                }
                _ => None,
            };
            faces.push(Face { verts: vec![a as u32, b as u32, n as u32], surface: of_seg[si], normals });
        }
        (Mesh { points, faces, surfaces, chord_error: err }, fault)
    }
    fn exact(&self) -> Measures {
        let p = &self.plane;
        let h = self.height;
        Measures {
            size: p.size.map(|a| a * h / 3.0),
            boundary: match (p.size, self.lateral) {
                (Some(a), Some(l)) => Some(a + l),
                _ => None,
            },
            centroid: p.centroid.map(|c| [0.75 * c[0], 0.25 * h, -0.75 * c[1]]),
            bounds: p.bounds.map(|(lo, hi)| ([lo[0].min(0.0), 0.0, (-hi[1]).min(0.0)], [hi[0].max(0.0), h, (-lo[1]).max(0.0)])),
            series: p.series,
        }
    }
    fn error(&self, detail: u32) -> Option<f64> {
        Geom::Rings(vec![self.segs.clone()]).sag(detail)
    }
    fn genus(&self) -> u32 {
        0
    }
    fn curved(&self) -> bool {
        self.segs.iter().any(|s| !matches!(s, Seg::Line { .. }))
    }
}

// ---------------------------------------------------------------- any other

/// A construction's own check of a build ([`Make::check`]).
pub(crate) type Check = Rc<dyn Fn(&Mesh, u32) -> Option<SolidFault>>;

/// A solid given by its own functions.
#[derive(Clone)]
pub(crate) struct Gen {
    pub count: Rc<dyn Fn(u32) -> (u64, u64)>,
    pub build: Rc<dyn Fn(u32) -> (Mesh, Option<SolidFault>)>,
    pub exact: Measures,
    pub error: Rc<dyn Fn(u32) -> Option<f64>>,
    pub genus: u32,
    /// The construction's own check ([`Make::check`]).
    pub check: Option<Check>,
}

impl Make for Gen {
    fn count(&self, detail: u32) -> (u64, u64) {
        (self.count)(detail)
    }
    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>) {
        (self.build)(detail)
    }
    fn exact(&self) -> Measures {
        self.exact
    }
    fn error(&self, detail: u32) -> Option<f64> {
        (self.error)(detail)
    }
    fn genus(&self) -> u32 {
        self.genus
    }
    fn check(&self, mesh: &Mesh, detail: u32) -> Option<SolidFault> {
        self.check.as_ref().and_then(|c| c(mesh, detail))
    }
    fn curved(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------- a lattice on a box's faces

/// A mesh on the six faces of a lattice of `n[0] × n[1] × n[2]` points (a point is on a face
/// when one of its indices is at an end), each point placed by `point` (its position and, on a
/// smooth surface, its normal), each cell a quad or, where `cell` says to split it, two
/// triangles, in the surface `cell` names. A cell `(i, j)` of the face across axis `k` spans
/// indices `i..i + 1` of axis `(k + 1) % 3` and `j..j + 1` of axis `(k + 2) % 3`.
pub(crate) struct Lattice<P, C> {
    pub n: [u32; 3],
    pub point: P,
    pub cell: C,
    pub surfaces: Vec<Surface>,
}

/// The cells of a face across axis `k`: `(cells along u, cells along v)`.
fn face_cells(n: [u32; 3], k: usize) -> (u32, u32) {
    (n[(k + 1) % 3] - 1, n[(k + 2) % 3] - 1)
}

impl<P: Fn([u32; 3]) -> (P3, Option<P3>), C: Fn(usize, u32, u32) -> (bool, u32)> Lattice<P, C> {
    /// The points: every point of the lattice less those inside it.
    pub fn points(&self) -> u64 {
        let [a, b, c] = self.n.map(u64::from);
        a * b * c - a.saturating_sub(2) * b.saturating_sub(2) * c.saturating_sub(2)
    }
    /// The faces: a quad for each cell, two triangles for each split one.
    pub fn faces(&self) -> u64 {
        let mut f = 0u64;
        for k in 0..3 {
            let (cu, cv) = face_cells(self.n, k);
            for i in 0..cu {
                for j in 0..cv {
                    f += if (self.cell)(k, i, j).0 { 2 } else { 1 };
                }
            }
        }
        2 * f
    }
    /// The mesh.
    pub fn mesh(&self) -> Mesh {
        let mut id: std::collections::BTreeMap<[u32; 3], u32> = std::collections::BTreeMap::new();
        let mut points = Vec::new();
        let mut normals: Vec<Option<P3>> = Vec::new();
        let mut faces = Vec::new();
        for k in 0..3 {
            let (u, v) = ((k + 1) % 3, (k + 2) % 3);
            let (cu, cv) = face_cells(self.n, k);
            for side in 0..2 {
                let mut get = |i: u32, j: u32| -> u32 {
                    let mut q = [0u32; 3];
                    q[k] = side * (self.n[k] - 1);
                    q[u] = i;
                    q[v] = j;
                    *id.entry(q).or_insert_with(|| {
                        let (p, n) = (self.point)(q);
                        points.push(p);
                        normals.push(n);
                        (points.len() - 1) as u32
                    })
                };
                for i in 0..cu {
                    for j in 0..cv {
                        let (split, surface) = (self.cell)(k, i, j);
                        let mut c = [get(i, j), get(i + 1, j), get(i + 1, j + 1), get(i, j + 1)];
                        if side == 0 {
                            c.reverse();
                        }
                        if split {
                            faces.push((vec![c[0], c[1], c[2]], surface));
                            faces.push((vec![c[0], c[2], c[3]], surface));
                        } else {
                            faces.push((c.to_vec(), surface));
                        }
                    }
                }
            }
        }
        let faces = faces
            .into_iter()
            .map(|(verts, surface)| {
                let smooth = self.surfaces[surface as usize].smooth;
                let ns: Option<Vec<P3>> = if smooth { verts.iter().map(|&i| normals[i as usize]).collect() } else { None };
                Face { verts, surface, normals: ns }
            })
            .collect();
        Mesh { points, faces, surfaces: self.surfaces.clone(), chord_error: 0.0 }
    }
}


/// The greatest distance between a sphere of centre `c` and radius `r` and the faces inscribed
/// in it (all their points on it): `r` less each face's plane's distance from the centre,
/// exactly (the cap of the sphere over a face's circumcircle holds every point of the sphere
/// over the face).
pub(crate) fn sphere_error(m: &Mesh, c: P3, r: f64, surface: u32) -> f64 {
    let mut worst = 0.0f64;
    for f in m.faces.iter().filter(|f| f.surface == surface) {
        let n = norm3(m.area_vector(f));
        worst = worst.max(r - dot3(n, sub3(m.points[f.verts[0] as usize], c)).abs());
    }
    worst
}

// ---------------------------------------------------------------- tubes along a curve

/// A tube swept along a curve: `rings` sections of the section's `corners` points each, the
/// `ring(i)` giving section `i`'s points and normals (`None` where they are not known). A
/// closed tube joins its last section to its first with the first's points shifted by `shift`
/// (a half-twisted band's section comes back turned); an open one is capped by two flat faces.
/// Each quad between sections is split into two triangles, the diagonals alternating (a swept
/// section's quads are not planar); the mesh's `chord_error` is the greatest twist of a quad,
/// which the caller adds its own terms to. `side_surface(j)` names the surface of the strip after
/// corner `j`.
#[allow(clippy::too_many_arguments, reason = "a tube is its rings, corners, closing, shift, sections, the strips' surfaces, the surfaces and the cap: one call a sweep")]
pub(crate) fn tube_mesh(rings: usize, corners: usize, closed: bool, shift: usize, ring: impl Fn(usize) -> Vec<(P3, Option<P3>)>, side_surface: impl Fn(usize) -> u32, surfaces: Vec<Surface>, cap: u32) -> Mesh {
    let mut points = Vec::with_capacity(rings * corners);
    let mut normals = Vec::with_capacity(rings * corners);
    for i in 0..rings {
        for (p, n) in ring(i) {
            points.push(p);
            normals.push(n);
        }
    }
    let at = |i: usize, j: usize| -> u32 {
        if i == rings {
            ((j + shift) % corners) as u32
        } else {
            (i * corners + j % corners) as u32
        }
    };
    let steps = if closed { rings } else { rings - 1 };
    let mut faces = Vec::new();
    let mut twist = 0.0f64;
    for i in 0..steps {
        for j in 0..corners {
            let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
            let s = side_surface(j);
            let ns = |v: &[u32]| v.iter().map(|&x| normals[x as usize]).collect::<Option<Vec<P3>>>();
            // The quad's twist: its centre is `|a + c − b − d| / 4` off either diagonal's triangles.
            let p = |x: u32| points[x as usize];
            twist = twist.max(0.25 * len3(sub3(add3(p(a), p(c)), add3(p(b), p(d)))));
            // The diagonals alternate, so that a twisted strip's cut leans one way and then the
            // other (one way only, its volume errs to first order in the twist).
            let tris = if (i + j).is_multiple_of(2) { [[a, b, c], [a, c, d]] } else { [[a, b, d], [b, c, d]] };
            for t in tris {
                faces.push(Face { verts: t.to_vec(), surface: s, normals: ns(&t) });
            }
        }
    }
    if !closed {
        faces.push(Face { verts: (0..corners as u32).rev().collect(), surface: cap, normals: None });
        faces.push(Face { verts: (0..corners).map(|j| at(rings - 1, j)).collect(), surface: cap, normals: None });
    }
    Mesh { points, faces, surfaces, chord_error: twist }
}

/// The check of a tube [`tube_mesh`] made (`rings` sections of `corners` points, closed or
/// capped): no two of its faces cross. Faces of neighbouring strips are kept apart by the
/// section between them, so they are not compared: each two neighbouring sections must lie in
/// planes that do not meet inside the tube (every point of the next section ahead of this one's
/// plane, every point of this one behind the next one's), which is checked exactly; then every
/// pair of faces two strips or more apart is searched ([`Mesh::self_crossing_where`]). The faces
/// of one strip are not compared where the strip check below shows them apart (each in its own
/// wedge about the strip's axis); where it does not (a strip that turns far, as a thin band's
/// twist does), they are searched too. The check is sufficient, not necessary: a round wire
/// whose frame turns as a whole fails it and does not cross.
pub(crate) fn tube_check(m: &Mesh, rings: usize, corners: usize, closed: bool, shift: usize) -> Option<SolidFault> {
    let steps = if closed { rings } else { rings - 1 };
    let mut wedged = vec![true; steps];
    let ring = |i: usize| &m.points[i * corners..(i + 1) * corners];
    let centre = |i: usize| scale3(ring(i).iter().fold([0.0; 3], |s, p| add3(s, *p)), 1.0 / corners as f64);
    let normal = |i: usize| {
        let (pts, c) = (ring(i), centre(i));
        let mut s = [0.0; 3];
        for j in 0..corners {
            s = add3(s, cross3(sub3(pts[j], c), sub3(pts[(j + 1) % corners], c)));
        }
        norm3(s)
    };
    for (i, wedge) in wedged.iter_mut().enumerate() {
        let (a, b) = (i, (i + 1) % rings);
        let (ca, cb) = (centre(a), centre(b));
        let along = sub3(cb, ca);
        let (mut na, mut nb) = (normal(a), normal(b));
        if dot3(na, along) < 0.0 {
            na = scale3(na, -1.0);
        }
        if dot3(nb, along) < 0.0 {
            nb = scale3(nb, -1.0);
        }
        let ahead = ring(b).iter().all(|p| dot3(sub3(*p, ca), na) > 0.0);
        let behind = ring(a).iter().all(|p| dot3(sub3(*p, cb), nb) < 0.0);
        if !(ahead && behind) {
            let f = 2 * corners * i;
            return Some(SolidFault::Crossing { a: f, b: f + 1 });
        }
        // The strip's faces in their own wedges about the line through the two middles: each
        // corner turns by under half the angle to its neighbours, and each face's span and its two
        // corners' turns stay under half a turn. Then every face lies in its wedge (its corners
        // do, and a face is in their hull, which a wedge under half a turn holds), and faces
        // that are not neighbours are in wedges apart.
        let w = norm3(along);
        let e1 = norm3(sub3(sub3(ring(a)[0], ca), scale3(w, dot3(sub3(ring(a)[0], ca), w))));
        let e2 = cross3(w, e1);
        let angle = |p: P3| {
            let d = sub3(p, ca);
            atan2(dot3(d, e2), dot3(d, e1))
        };
        let wrap = |x: f64| x - TAU * floor((x + PI) / TAU);
        let pos = |x: f64| x - TAU * floor(x / TAU);
        let al: Vec<f64> = ring(a).iter().map(|p| angle(*p)).collect();
        // (The last section of a closed band joins the first with its corners shifted.)
        let sh = if b == 0 { shift } else { 0 };
        let be: Vec<f64> = (0..corners).map(|j| angle(ring(b)[(j + sh) % corners])).collect();
        let turn: Vec<f64> = (0..corners).map(|j| wrap(be[j] - al[j]).abs()).collect();
        let gap = |v: &[f64], j: usize| pos(v[(j + 1) % corners] - v[j]);
        for j in 0..corners {
            let prev = (j + corners - 1) % corners;
            let least = gap(&al, prev).min(gap(&al, j)).min(gap(&be, prev)).min(gap(&be, j));
            let span = gap(&al, j).max(gap(&be, j)) + turn[j] + turn[(j + 1) % corners];
            if turn[j] >= 0.5 * least || span >= PI {
                *wedge = false;
                break;
            }
        }
    }
    let strip = |f: usize| if f < 2 * corners * steps { f / (2 * corners) } else if f == 2 * corners * steps { 0 } else { steps - 1 };
    let gap = |x: usize, y: usize| {
        let d = x.abs_diff(y);
        if closed {
            d.min(steps - d)
        } else {
            d
        }
    };
    m.self_crossing_where(|fa, fb| {
        let (sa, sb) = (strip(fa), strip(fb));
        gap(sa, sb) > 1 || (sa == sb && !wedged[sa])
    })
    .map(|(a, b)| SolidFault::Crossing { a, b })
}
