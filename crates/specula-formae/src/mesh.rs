//! What a solid builds to: a closed polygon mesh, its measures as built, and its checks.
//!
//! **Conventions.** Points in metres, right-handed, +Y up. A face is a planar polygon whose
//! points are wound counter-clockwise seen from outside the solid. Every face belongs to a
//! named [`Surface`] that is flat (each face is its own plane: a box's side, a cylinder's lid) or
//! smooth (the faces stand for one curved surface: a sphere, a cone's side). A smooth surface's
//! faces may carry an exact outward normal at each corner, where the surface is analytic. A
//! well-made mesh ([`Mesh::validate`]) is watertight (every edge is used by exactly two faces,
//! once in each direction), encloses a positive volume, has no edge or face under the floor of
//! its size, has every face planar, has the Euler characteristic of its genus, and has every
//! normal it gives on its face's outer side.

use crate::maths::*;
use crate::outline::EDGE_FLOOR;
use crate::Measures;

/// A built solid.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// The points.
    pub points: Vec<P3>,
    /// The faces.
    pub faces: Vec<Face>,
    /// The surfaces the faces belong to.
    pub surfaces: Vec<Surface>,
    /// A bound on the distance between the build and the true solid at its detail (0 for a
    /// solid of flat faces). For surfaces of arcs and lines it is a bound; where a piece is a
    /// parametric curve it is that piece's estimated chord error plus the bound of the rest.
    pub chord_error: f64,
}

/// One face: a planar polygon.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// Its points (indices into [`Mesh::points`]), counter-clockwise seen from outside.
    pub verts: Vec<u32>,
    /// The surface it belongs to (an index into [`Mesh::surfaces`]).
    pub surface: u32,
    /// The exact outward unit normal of its surface at each corner, where the surface is smooth
    /// and analytic there; `None` for a flat face (its plane's normal is the normal) and where
    /// the surface's normal is not known in closed form.
    pub normals: Option<Vec<P3>>,
}

/// A named surface of a solid.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    /// Its name (`side`, `top`, `sphere`...).
    pub name: String,
    /// Whether its faces stand for one curved surface (shaded smooth across their edges).
    pub smooth: bool,
    /// For a smooth surface, the greatest radius it turns at, metres (a sphere's radius; a
    /// torus's outer radius), which with the turn between its faces gives what its faceting
    /// shows ([`Round`]).
    pub radius: Option<f64>,
}

/// What the faceting of a smooth surface is, as the modeller's faceting check records it: the
/// greatest turn between the normals of two neighbouring faces of the surface, measured on the
/// build, and the surface's greatest radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round {
    /// The surface (an index into [`Mesh::surfaces`]).
    pub surface: usize,
    /// The greatest turn between neighbouring faces, degrees.
    pub turn_deg: f64,
    /// The surface's greatest radius, metres.
    pub radius: f64,
}

impl Round {
    /// How far the middle of a facet can stand in from the true surface: `r (1 − cos(t / 2))`.
    pub fn sag(&self) -> f64 {
        self.radius * versin(rad(self.turn_deg) * 0.5)
    }
}

/// Why a mesh is not well made (from [`Mesh::validate`] and the build's own checks).
#[derive(Clone, Debug, PartialEq)]
pub enum SolidFault {
    /// No points or faces, or a face of fewer than three points.
    TooFew {
        /// The face.
        face: usize,
    },
    /// A coordinate is not finite.
    NotFinite {
        /// The point.
        point: usize,
    },
    /// A face names a point that is not there, or names one point twice.
    BadIndex {
        /// The face.
        face: usize,
    },
    /// A point no face uses.
    Unused {
        /// The point.
        point: usize,
    },
    /// An edge is no longer than [`EDGE_FLOOR`] (a billionth) of the solid's size.
    ShortEdge {
        /// The face.
        face: usize,
        /// The edge's first corner in the face.
        edge: usize,
        /// Its length.
        length: f64,
    },
    /// A face's area is no more than [`EDGE_FLOOR`] of the solid's size times its longest edge
    /// (it is thinner than the floor).
    ZeroArea {
        /// The face.
        face: usize,
        /// Its area.
        area: f64,
    },
    /// A face's points stray from its plane by more than rounding allows.
    NotPlanar {
        /// The face.
        face: usize,
        /// How far its farthest point is from its plane.
        off: f64,
        /// What rounding allows.
        tolerance: f64,
    },
    /// A directed edge is used twice, or is not used in the other direction: the mesh is not
    /// closed, or its faces are not wound one way.
    Open {
        /// The edge's first point.
        a: u32,
        /// Its second point.
        b: u32,
    },
    /// The Euler characteristic is not that of the solid's genus.
    Euler {
        /// V − E + F.
        euler: i64,
        /// 2 − 2 × genus.
        expected: i64,
    },
    /// The volume is not positive: the faces are wound inward.
    Inward {
        /// The signed volume.
        volume: f64,
    },
    /// Two faces cross each other.
    Crossing {
        /// The first face.
        a: usize,
        /// The second face.
        b: usize,
    },
    /// The section or outline a solid is made from crosses itself at this detail: the build's
    /// rings, as the plane figure's check names them.
    SectionCrossing {
        /// The first edge's ring and point.
        a: (usize, usize),
        /// The second edge's ring and point.
        b: (usize, usize),
    },
    /// The section or outline a solid is made from has two edges nearer each other than the
    /// floor (see [`crate::Fault::Gap`]).
    SectionGap {
        /// The first edge's ring and point.
        a: (usize, usize),
        /// The second edge's ring and point.
        b: (usize, usize),
        /// How far apart they come.
        gap: f64,
    },
    /// The section or outline a solid is made from has an edge under the floor.
    SectionShortEdge {
        /// The edge's first point in the section.
        point: usize,
        /// Its length.
        length: f64,
    },
    /// A normal given at a corner of a face lies on the face's inner side (a right angle or more
    /// from the face's own normal): shaded smooth, the face would show dark.
    NormalBehind {
        /// The face.
        face: usize,
        /// The corner (its place in the face).
        corner: usize,
        /// The cosine of the angle between the corner's normal and the face's.
        cos: f64,
    },
    /// A pyramid's apex does not see this edge of its base from inside (the edge does not turn
    /// counter-clockwise about the point under the apex), so its faces would fold through each
    /// other.
    ApexOutside {
        /// The edge's first point in the base.
        edge: usize,
    },
    /// A raised figure's inset outline (see [`crate::raise`]) collapses an edge or stops being
    /// simple at this inset.
    InsetTooDeep {
        /// The inset asked for, metres.
        inset: f64,
        /// The parameter of the raised form that sets the inset (`width`, `inset` or `run`).
        param: &'static str,
        /// The deepest inset found to hold at this detail and the parameter's value that gives it
        /// (the same form built with that value has been built and does not fail for its inset);
        /// `None` where no value a caller may give (none from the least length up) holds, or where
        /// no search was made (`searched` false).
        holds: Option<(f64, f64)>,
        /// Whether the search for the deepest that holds was made: a caller may limit it
        /// ([`crate::ops::Solid::with_search`]), and with no builds left it is not made.
        searched: bool,
    },
    /// A flat cap with holes could not be cut into triangles that tile it (a mistake: the
    /// outline was checked first).
    CapNotCut,
}

/// How far, as a multiple of the machine epsilon times the solid's reach `R` (its greatest
/// coordinate), times `1 + n D² / A` for a face of `n` points, diameter `D` and area `A`, a face's
/// points may stray from its plane. Why: every point is a few operations (a product, a sum, a
/// lift) from exact, so each coordinate is within about `δ = 4 ε R` of a point of the true
/// plane; the plane is fitted through the face's mean with its Newell normal, a sum of `n`
/// cross products each off by about `δ D`, which tilts it by up to `n δ D / A` and so moves a
/// point as far as `D` from the mean by `n δ D² / A` (a sliver's normal, or a long ragged cap's,
/// is known less well than a square's); 32 is four times `2 δ`. (A face first failed the bound
/// `32 ε R (1 + L² / A)`, `L` its longest edge: the top of a prism of a spiral band, every
/// point at one height, off by 3.6e-15 against 2.5e-15.)
pub const PLANAR_EPS: f64 = 32.0;

impl Mesh {
    /// The least and greatest corners of the points.
    pub fn bounds(&self) -> Option<(P3, P3)> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &self.points {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        (!self.points.is_empty()).then_some((lo, hi))
    }

    /// The solid's size: the diagonal of its bounds.
    pub fn size(&self) -> f64 {
        self.bounds().map(|(lo, hi)| dist3(lo, hi)).unwrap_or(0.0)
    }

    /// The greatest size of a coordinate.
    pub fn reach(&self) -> f64 {
        self.points.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs()))
    }

    fn p(&self, i: u32) -> P3 {
        self.points[i as usize]
    }

    /// A face's area vector (Newell's): its area times its unit normal, for a planar face.
    pub fn area_vector(&self, f: &Face) -> P3 {
        let p0 = self.p(f.verts[0]);
        let mut s = [0.0; 3];
        for i in 1..f.verts.len().saturating_sub(1) {
            let c = cross3(sub3(self.p(f.verts[i]), p0), sub3(self.p(f.verts[i + 1]), p0));
            s = add3(s, c);
        }
        scale3(s, 0.5)
    }

    /// The measures of what was built: the volume and its centroid by the divergence theorem
    /// (each face a fan of triangles, each with a tetrahedron to a point near the middle: exact
    /// for planar faces), the surface area (the summed faces' areas) and the bounds.
    pub fn measures(&self) -> Measures {
        let Some((lo, hi)) = self.bounds() else { return Measures::default() };
        let c = scale3(add3(lo, hi), 0.5);
        let (mut v6, mut m, mut area) = (0.0, [0.0; 3], 0.0);
        for f in &self.faces {
            let a = sub3(self.p(f.verts[0]), c);
            for i in 1..f.verts.len() - 1 {
                let b = sub3(self.p(f.verts[i]), c);
                let d = sub3(self.p(f.verts[i + 1]), c);
                let t = dot3(a, cross3(b, d));
                v6 += t;
                m = add3(m, scale3(add3(add3(a, b), d), t));
            }
            area += len3(self.area_vector(f));
        }
        let vol = v6 / 6.0;
        let centroid = add3(c, scale3(m, 1.0 / (4.0 * v6)));
        Measures { size: Some(vol), boundary: Some(area), centroid: Some(centroid), bounds: Some((lo, hi)), series: None }
    }

    /// The directed edges, sorted: `(from, to, face, corner)`.
    fn edges(&self) -> Vec<(u32, u32, u32, u32)> {
        let mut e = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let n = f.verts.len();
            for i in 0..n {
                e.push((f.verts[i], f.verts[(i + 1) % n], fi as u32, i as u32));
            }
        }
        e.sort_unstable();
        e
    }

    /// V − E + F (E counted from the directed edges, each edge twice).
    pub fn euler(&self) -> i64 {
        let e: usize = self.faces.iter().map(|f| f.verts.len()).sum();
        self.points.len() as i64 - (e / 2) as i64 + self.faces.len() as i64
    }

    /// Checks that the mesh is well made for a solid of `genus` (0 for a ball, 1 for a torus):
    /// at least one face, every face of three points or more naming points that are there,
    /// none twice; every coordinate finite and every point used; no edge and no face under the
    /// floor ([`EDGE_FLOOR`] of the size); every face planar ([`PLANAR_EPS`]); every directed
    /// edge used once and its reverse once (closed, and wound one way); the Euler
    /// characteristic `2 − 2 genus`; a positive volume (wound outward); and every normal given at
    /// a face's corner on the face's outer side (less than a right angle from its own normal),
    /// one a corner. Near linear in the mesh's size. It does not look for faces that cross:
    /// [`Mesh::self_crossing`] does.
    pub fn validate(&self, genus: u32) -> Result<(), SolidFault> {
        if self.faces.is_empty() || self.points.is_empty() {
            return Err(SolidFault::TooFew { face: 0 });
        }
        if let Some(i) = self.points.iter().position(|p| !p.iter().all(|x| x.is_finite())) {
            return Err(SolidFault::NotFinite { point: i });
        }
        let mut used = vec![false; self.points.len()];
        for (fi, f) in self.faces.iter().enumerate() {
            if f.verts.len() < 3 {
                return Err(SolidFault::TooFew { face: fi });
            }
            for (i, &v) in f.verts.iter().enumerate() {
                if v as usize >= self.points.len() || f.verts[..i].contains(&v) {
                    return Err(SolidFault::BadIndex { face: fi });
                }
                used[v as usize] = true;
            }
            if f.surface as usize >= self.surfaces.len() {
                return Err(SolidFault::BadIndex { face: fi });
            }
        }
        if let Some(i) = used.iter().position(|u| !u) {
            return Err(SolidFault::Unused { point: i });
        }
        let size = self.size();
        let unit = PLANAR_EPS * f64::EPSILON * self.reach();
        for (fi, f) in self.faces.iter().enumerate() {
            let n = f.verts.len();
            let mut longest = 0.0f64;
            for i in 0..n {
                let l = dist3(self.p(f.verts[i]), self.p(f.verts[(i + 1) % n]));
                if l <= size * EDGE_FLOOR {
                    return Err(SolidFault::ShortEdge { face: fi, edge: i, length: l });
                }
                longest = longest.max(l);
            }
            let av = self.area_vector(f);
            let area = len3(av);
            if area <= size * EDGE_FLOOR * longest {
                return Err(SolidFault::ZeroArea { face: fi, area });
            }
            if n > 3 {
                let nrm = scale3(av, 1.0 / area);
                let mut mean = [0.0; 3];
                for &v in &f.verts {
                    mean = add3(mean, self.p(v));
                }
                let mean = scale3(mean, 1.0 / n as f64);
                let off = f.verts.iter().map(|&v| dot3(nrm, sub3(self.p(v), mean)).abs()).fold(0.0, f64::max);
                // (D: the face's diameter about its mean, at most twice the farthest point.)
                let dd = 2.0 * f.verts.iter().map(|&v| dist3(self.p(v), mean)).fold(0.0, f64::max);
                let tol = unit * (1.0 + n as f64 * dd * dd / area);
                if off > tol {
                    return Err(SolidFault::NotPlanar { face: fi, off, tolerance: tol });
                }
            }
        }
        let edges = self.edges();
        for w in edges.windows(2) {
            if (w[0].0, w[0].1) == (w[1].0, w[1].1) {
                return Err(SolidFault::Open { a: w[0].0, b: w[0].1 });
            }
        }
        for e in &edges {
            if edges.binary_search_by(|x| (x.0, x.1).cmp(&(e.1, e.0))).is_err() {
                return Err(SolidFault::Open { a: e.0, b: e.1 });
            }
        }
        let euler = self.euler();
        let expected = 2 - 2 * i64::from(genus);
        if euler != expected {
            return Err(SolidFault::Euler { euler, expected });
        }
        let volume = self.measures().size.unwrap_or(0.0);
        if volume.is_nan() || volume <= 0.0 {
            return Err(SolidFault::Inward { volume });
        }
        for (fi, f) in self.faces.iter().enumerate() {
            if let Some(ns) = &f.normals {
                let n = norm3(self.area_vector(f));
                for (corner, v) in ns.iter().enumerate() {
                    let cos = dot3(*v, n);
                    if cos.is_nan() || cos <= 0.0 || ns.len() != f.verts.len() {
                        return Err(SolidFault::NormalBehind { face: fi, corner, cos });
                    }
                }
            }
        }
        Ok(())
    }

    /// The faceting of each smooth surface that has a radius: the greatest turn between the
    /// normals of two of its faces that share an edge.
    pub fn rounds(&self) -> Vec<Round> {
        let mut worst = vec![0.0f64; self.surfaces.len()];
        let normal = |f: &Face| norm3(self.area_vector(f));
        let edges = self.edges();
        for e in &edges {
            if e.0 > e.1 {
                continue;
            }
            if let Ok(i) = edges.binary_search_by(|x| (x.0, x.1).cmp(&(e.1, e.0))) {
                let (fa, fb) = (&self.faces[e.2 as usize], &self.faces[edges[i].2 as usize]);
                if fa.surface == fb.surface && self.surfaces[fa.surface as usize].smooth {
                    let c = dot3(normal(fa), normal(fb)).clamp(-1.0, 1.0);
                    let s = &mut worst[fa.surface as usize];
                    *s = s.max(acos(c));
                }
            }
        }
        self.surfaces
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match (s.smooth, s.radius) {
                (true, Some(r)) => Some(Round { surface: i, turn_deg: worst[i] * 180.0 / PI, radius: r }),
                _ => None,
            })
            .collect()
    }

    /// The faces as triangles `(face, [a, b, c])`: a convex face as a fan, any other by
    /// cutting ears in its own plane.
    pub fn triangles(&self) -> Vec<(u32, [u32; 3])> {
        let mut out = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let fi = fi as u32;
            let n = f.verts.len();
            let nrm = self.area_vector(f);
            let ax = (0..3).max_by(|&a, &b| nrm[a].abs().total_cmp(&nrm[b].abs())).unwrap_or(2);
            let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
            let sign = if nrm[ax] >= 0.0 { 1.0 } else { -1.0 };
            let q: Vec<Pt> = f.verts.iter().map(|&i| {
                let p = self.p(i);
                [p[u], p[v]]
            }).collect();
            let turn = |a: Pt, b: Pt, c: Pt| sign * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]));
            if (0..n).all(|i| turn(q[i], q[(i + 1) % n], q[(i + 2) % n]) > 0.0) {
                for i in 1..n - 1 {
                    out.push((fi, [f.verts[0], f.verts[i], f.verts[i + 1]]));
                }
                continue;
            }
            let mut idx: Vec<usize> = (0..n).collect();
            let mut guard = 0;
            while idx.len() > 3 && guard < n * n {
                guard += 1;
                let m = idx.len();
                let mut cut = None;
                for k in 0..m {
                    let (a, b, c) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
                    if turn(q[a], q[b], q[c]) <= 0.0 {
                        continue;
                    }
                    let inside = idx.iter().any(|&j| j != a && j != b && j != c && turn(q[a], q[b], q[j]) >= 0.0 && turn(q[b], q[c], q[j]) >= 0.0 && turn(q[c], q[a], q[j]) >= 0.0);
                    if !inside {
                        cut = Some(k);
                        break;
                    }
                }
                let k = cut.unwrap_or(0);
                let m = idx.len();
                out.push((fi, [f.verts[idx[(k + m - 1) % m]], f.verts[idx[k]], f.verts[idx[(k + 1) % m]]]));
                idx.remove(k);
            }
            if idx.len() == 3 {
                out.push((fi, [f.verts[idx[0]], f.verts[idx[1]], f.verts[idx[2]]]));
            }
        }
        out
    }

    /// The first two faces found that cross each other (an edge of one passes through the
    /// inside of the other), if any. Two triangles are not compared where a corner of one is a
    /// point of the other's face (the faces meet there by construction), nor two of one face;
    /// faces that touch without crossing, and coplanar overlaps, are not reported. The triangles
    /// are put in a tree of boxes and the pairs whose boxes meet are compared, each once.
    pub fn self_crossing(&self) -> Option<(usize, usize)> {
        self.self_crossing_where(|_, _| true)
    }

    /// As [`Mesh::self_crossing`], comparing only the pairs of faces `compare` takes (a swept
    /// tube's build leaves out pairs its own check has proved apart).
    pub fn self_crossing_where(&self, compare: impl Fn(usize, usize) -> bool) -> Option<(usize, usize)> {
        let tris = self.triangles();
        if tris.is_empty() {
            return None;
        }
        let mut faces_of: Vec<Vec<u32>> = vec![Vec::new(); self.points.len()];
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in &f.verts {
                faces_of[v as usize].push(fi as u32);
            }
        }
        let bb: Vec<(P3, P3)> = tris
            .iter()
            .map(|(_, t)| {
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for &i in t {
                    let p = self.p(i);
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                (lo, hi)
            })
            .collect();
        // A tree of boxes over the triangles, split at the median of their middles along the
        // axis where the middles spread farthest, down to leaves of at most 8; pairs are found by
        // descending two branches at once wherever their boxes meet, so each pair of triangles is
        // compared at most once, and the work is near n log n however unlike in size the
        // triangles are. (Split along the node's longest side instead, a helix 10 km across with
        // coils 0.15 m apart and faces a kilometre long was never split by height: every leaf
        // held faces of every coil, and its search compared a billion pairs, 56 s.)
        struct Node {
            lo: P3,
            hi: P3,
            kids: Option<(usize, usize)>,
            tris: std::ops::Range<usize>,
        }
        let mut order: Vec<usize> = (0..tris.len()).collect();
        let mut nodes: Vec<Node> = Vec::new();
        let mut stack = vec![(0usize, tris.len(), usize::MAX, false)];
        while let Some((s, e, parent, right)) = stack.pop() {
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            let mut clo = [f64::INFINITY; 3];
            let mut chi = [f64::NEG_INFINITY; 3];
            for &t in &order[s..e] {
                for k in 0..3 {
                    lo[k] = lo[k].min(bb[t].0[k]);
                    hi[k] = hi[k].max(bb[t].1[k]);
                    let c = bb[t].0[k] + bb[t].1[k];
                    clo[k] = clo[k].min(c);
                    chi[k] = chi[k].max(c);
                }
            }
            let id = nodes.len();
            nodes.push(Node { lo, hi, kids: None, tris: s..e });
            if parent != usize::MAX {
                let kids = nodes[parent].kids.get_or_insert((usize::MAX, usize::MAX));
                if right {
                    kids.1 = id;
                } else {
                    kids.0 = id;
                }
            }
            if e - s > 8 {
                let ax = (0..3).max_by(|&x, &y| (chi[x] - clo[x]).total_cmp(&(chi[y] - clo[y]))).unwrap_or(0);
                let mid = (s + e) / 2;
                let centre = |t: usize| bb[t].0[ax] + bb[t].1[ax];
                order[s..e].select_nth_unstable_by(mid - s, |&x, &y| centre(x).total_cmp(&centre(y)));
                stack.push((mid, e, id, true));
                stack.push((s, mid, id, false));
            }
        }
        let meet = |a: &Node, b: &Node| (0..3).all(|k| a.lo[k] <= b.hi[k] && b.lo[k] <= a.hi[k]);
        let test = |i: usize, j: usize| -> Option<(usize, usize)> {
            let ((fa, ta), (fb, tb)) = (tris[i], tris[j]);
            if fa == fb || ta.iter().any(|v| tb.contains(v)) || !compare(fa as usize, fb as usize) {
                return None;
            }
            // (A triangle with a corner on the other's face: the faces meet there.)
            let on = |t: &[u32; 3], f: u32| t.iter().any(|&v| faces_of[v as usize].binary_search(&f).is_ok());
            if on(&ta, fb) || on(&tb, fa) {
                return None;
            }
            let (la, ha) = bb[i];
            let (lb, hb) = bb[j];
            if (0..3).any(|q| ha[q] < lb[q] || hb[q] < la[q]) {
                return None;
            }
            if triangles_cross(ta.map(|v| self.p(v)), tb.map(|v| self.p(v))) {
                let (fa, fb) = (fa as usize, fb as usize);
                return Some((fa.min(fb), fa.max(fb)));
            }
            None
        };
        let mut pairs = vec![(0usize, 0usize)];
        while let Some((a, b)) = pairs.pop() {
            let (na, nb) = (&nodes[a], &nodes[b]);
            if a != b && !meet(na, nb) {
                continue;
            }
            match (na.kids, nb.kids) {
                (None, None) => {
                    for (x, &i) in order[na.tris.clone()].iter().enumerate() {
                        let from = if a == b { x + 1 } else { 0 };
                        for &j in &order[nb.tris.clone()][from..] {
                            if let Some(found) = test(i, j) {
                                return Some(found);
                            }
                        }
                    }
                }
                _ if a == b => {
                    let (l, r) = na.kids.expect("a branch");
                    pairs.extend([(l, l), (r, r), (l, r)]);
                }
                (Some((l, r)), _) if nb.kids.is_none() || na.tris.len() >= nb.tris.len() => pairs.extend([(l, b), (r, b)]),
                (_, Some((l, r))) => pairs.extend([(a, l), (a, r)]),
                (Some((l, r)), None) => pairs.extend([(l, b), (r, b)]),
            }
        }
        None
    }

    /// The distance from `p` to the mesh's surface.
    pub fn distance(&self, p: P3) -> f64 {
        self.triangles().iter().map(|(_, t)| point_triangle(p, t.map(|v| self.p(v)))).fold(f64::INFINITY, f64::min)
    }

    /// Whether `p` is inside the solid: by the winding number (the solid angle the surface
    /// subtends at `p`, over 4π), which is 1 inside and 0 outside.
    pub fn contains(&self, p: P3) -> bool {
        let mut w = 0.0;
        for (_, t) in self.triangles() {
            let [a, b, c] = t.map(|v| sub3(self.p(v), p));
            let (la, lb, lc) = (len3(a), len3(b), len3(c));
            let num = dot3(a, cross3(b, c));
            let den = la * lb * lc + dot3(a, b) * lc + dot3(a, c) * lb + dot3(b, c) * la;
            w += 2.0 * atan2(num, den);
        }
        w / (4.0 * PI) > 0.5
    }
}

/// `(b − a) · ((c − a) × (d − a))`: positive when `d` is on the side of the plane `a b c` its
/// normal (counter-clockwise) points to.
fn orient3(a: P3, b: P3, c: P3, d: P3) -> f64 {
    dot3(sub3(b, a), cross3(sub3(c, a), sub3(d, a)))
}

/// The sign of [`orient3`]: 1 or −1 where it is past its rounding (about `16 ε |b − a| |c − a|
/// |d − a|`: each of its products rounds by a few ε of their size), 0 where it is within it (the
/// four points as good as coplanar: on a very flat solid, triangles that do not cross would
/// otherwise be read as crossing by rounding).
fn orient_sign(a: P3, b: P3, c: P3, d: P3) -> i32 {
    let o = orient3(a, b, c, d);
    let err = 16.0 * f64::EPSILON * len3(sub3(b, a)) * len3(sub3(c, a)) * len3(sub3(d, a));
    if o > err {
        1
    } else if o < -err {
        -1
    } else {
        0
    }
}

/// Whether the segment `p`-`q` passes through the inside of the triangle `t` (strictly, past
/// rounding: a segment that touches the triangle, or one as good as in its plane, does not).
fn segment_crosses(p: P3, q: P3, t: [P3; 3]) -> bool {
    let (o1, o2) = (orient_sign(t[0], t[1], t[2], p), orient_sign(t[0], t[1], t[2], q));
    if o1 * o2 != -1 {
        return false;
    }
    let s = [orient_sign(p, q, t[0], t[1]), orient_sign(p, q, t[1], t[2]), orient_sign(p, q, t[2], t[0])];
    s.iter().all(|x| *x == 1) || s.iter().all(|x| *x == -1)
}

/// Whether two triangles cross: an edge of either passes through the other.
fn triangles_cross(a: [P3; 3], b: [P3; 3]) -> bool {
    (0..3).any(|i| segment_crosses(a[i], a[(i + 1) % 3], b)) || (0..3).any(|i| segment_crosses(b[i], b[(i + 1) % 3], a))
}

/// The distance from `p` to the triangle `t`.
fn point_triangle(p: P3, t: [P3; 3]) -> f64 {
    let n = cross3(sub3(t[1], t[0]), sub3(t[2], t[0]));
    let nn = dot3(n, n);
    if nn > 0.0 {
        // Inside the triangle's prism: the distance to its plane.
        let inside = (0..3).all(|i| dot3(cross3(sub3(t[(i + 1) % 3], t[i]), sub3(p, t[i])), n) >= 0.0);
        if inside {
            return dot3(sub3(p, t[0]), n).abs() / nn.sqrt();
        }
    }
    (0..3).map(|i| seg_dist(p, t[i], t[(i + 1) % 3])).fold(f64::INFINITY, f64::min)
}

fn seg_dist(p: P3, a: P3, b: P3) -> f64 {
    let d = sub3(b, a);
    let l2 = dot3(d, d);
    let u = if l2 > 0.0 { (dot3(sub3(p, a), d) / l2).clamp(0.0, 1.0) } else { 0.0 };
    dist3(p, add3(a, scale3(d, u)))
}
