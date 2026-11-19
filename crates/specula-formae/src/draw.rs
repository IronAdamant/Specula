//! Drawn: a solid as a flat picture (two and a half dimensions, the second sense: what a 2D
//! game shows of a 3D thing).
//!
//! [`draw`] projects a built [`Mesh`] in a parallel view ([`View`]; the named ones are in
//! [`views`]) and answers the parts of its faces that are seen, as convex plane polygons in the
//! view's picture (`u` right, `v` up), each with its depth, its shade and the face and surface it
//! came from, in back-to-front order; and the silhouette, the outline of the whole picture
//! (rings: outer counter-clockwise, holes clockwise).
//!
//! **Views.** An orthographic view looks from the azimuth `φ` (from +Z toward +X) and the
//! elevation `e` (up from the ground): the viewer is along `d = (cos e sin φ, sin e, cos e cos φ)`,
//! the picture's right is `(cos φ, 0, −sin φ)` and its up `d × right`. The front view is `φ = 0,
//! e = 0` (the picture is x right, y up); the top is `e = 90°` (x right, −z up, as the library's
//! plane figures are raised); the side looks from +X. An oblique view keeps the front view's
//! picture and draws depth (−z) along the angle `α` at the share `k`: `u = x − k z cos α`,
//! `v = y − k z sin α` (cavalier `k = 1`, cabinet `k = 1/2`).
//!
//! **Shade.** `(1 + n · L) / 2`, `n` the face's outward unit normal and `L` the light, from the
//! upper left in front of the picture: `(−right + 2 up + 2 toward) / 3` (for an oblique view, the
//! front view's). 1 faces the light, 0 faces away; a face seen square on is 2/3 + 1/6.
//!
//! **Hidden surfaces, and why they are right.** A face is seen only if it faces the viewer (its
//! projected area is positive past rounding: the projection of a closed surface's first point
//! along a ray from the viewer is where the ray enters the solid). Each such face is a convex
//! polygon in the picture, or is cut into its triangles where its picture is not convex.
//! - **A convex solid** (every edge's neighbouring face lies on the inner side of its face's
//!   plane, to rounding) is drawn by that alone: the front faces of a convex solid do not
//!   overlap in the picture.
//! - **Any other** is drawn by subtraction. Two front polygons whose pictures overlap (by more
//!   than rounding) are compared once, at the middle of their overlap: the one farther there is
//!   behind the other over all of the overlap, because the overlap is convex, the difference of
//!   their depths is affine over it, and it cannot be zero inside it (the faces of a solid of
//!   this library do not cross, and two that meet do so only on their edges, which are on the
//!   overlap's boundary). Each polygon then loses, by convex clipping, every polygon in front of
//!   it. At a point of the picture the nearest front polygon keeps it (nothing in front of it
//!   there) and every other polygon over it has that one in front over their overlap and loses
//!   it: what is answered covers the picture once. Pieces thinner than rounding are dropped. Two
//!   polygons whose depths at their overlap's middle are within rounding of each other (faces
//!   that nearly touch over an area) cannot be ordered soundly and are refused as data
//!   (`faces_too_near_in_depth`); so is a drawing past [`MAX_PIECES`] or [`MAX_WORK`]
//!   (`drawing_too_large`).
//!
//! **The silhouette** is found apart from the polygons: the edges between a face that faces
//! the viewer and one that does not (the contour) are cut where they cross in the picture, and a
//! piece of one is on the outline when the picture is not covered just beside it on the side
//! away from its front face (a point a ten-billionth of the picture's size off it lies in no
//! front polygon). The pieces are joined into rings.

use crate::error::Error;
use crate::maths::*;
use crate::mesh::Mesh;

/// How a view projects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    /// Parallel rays along the view's direction, the picture square to them.
    Orthographic {
        /// From +Z toward +X, degrees.
        azimuth: f64,
        /// Up from the ground, degrees (90 looks straight down).
        elevation: f64,
    },
    /// The front view's picture with depth (−z) drawn along `angle` degrees at the share `depth`.
    Oblique {
        /// The receding axis's angle in the picture, degrees from the right.
        angle: f64,
        /// How much of a unit of depth is drawn (1 cavalier, 0.5 cabinet).
        depth: f64,
    },
}

/// A named view.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Its name (snake_case).
    pub name: &'static str,
    /// What it is.
    pub about: &'static str,
    /// Its projection.
    pub projection: Projection,
}

/// The isometric elevation, `atan(1/√2)` (35.264 degrees), in degrees.
fn iso_e() -> f64 {
    atan(1.0 / 2f64.sqrt()) * 180.0 / PI
}

/// The views by name. The isometric looks down `atan(1/√2)` = 35.264 degrees at 45 (its three
/// axes draw 120 degrees apart, each foreshortened to √(2/3)); the game isometric looks down 30
/// degrees at 45 (its ground axes draw at `atan(1/2)` = 26.565 degrees: two pixels across for one
/// up); the dimetric is the one whose x and y are drawn alike and z at half of them (azimuth
/// `asin(√(1/8))` = 20.705, elevation `acos(√(8/9))` = 19.471: its x axis draws 7.18 degrees
/// below the right and its z axis 41.41 degrees below the left, the drafting standard's 7° 10′
/// and 41° 25′).
pub fn views() -> Vec<View> {
    let dim_a = asin((1.0f64 / 8.0).sqrt()) * 180.0 / PI;
    let dim_e = acos((8.0f64 / 9.0).sqrt()) * 180.0 / PI;
    vec![
        View { name: "top", about: "from above: x right, -z up", projection: Projection::Orthographic { azimuth: 0.0, elevation: 90.0 } },
        View { name: "front", about: "from +z: x right, y up", projection: Projection::Orthographic { azimuth: 0.0, elevation: 0.0 } },
        View { name: "side", about: "from +x: -z right, y up", projection: Projection::Orthographic { azimuth: 90.0, elevation: 0.0 } },
        View { name: "isometric", about: "true isometric: from azimuth 45, elevation 35.264", projection: Projection::Orthographic { azimuth: 45.0, elevation: iso_e() } },
        View { name: "game_isometric", about: "the 2:1 isometric of games: from azimuth 45, elevation 30 (ground axes at 26.565)", projection: Projection::Orthographic { azimuth: 45.0, elevation: 30.0 } },
        View { name: "dimetric", about: "dimetric, x and y alike and z at half: azimuth 20.705, elevation 19.471", projection: Projection::Orthographic { azimuth: dim_a, elevation: dim_e } },
        View { name: "cavalier", about: "oblique: the front view with depth drawn whole at 45 degrees", projection: Projection::Oblique { angle: 45.0, depth: 1.0 } },
        View { name: "cabinet", about: "oblique: the front view with depth drawn at half at 45 degrees", projection: Projection::Oblique { angle: 45.0, depth: 0.5 } },
    ]
}

/// The named view, if there is one.
pub fn view(name: &str) -> Option<View> {
    views().into_iter().find(|v| v.name == name)
}

/// The most polygons a drawing answers.
pub const MAX_PIECES: usize = 262_144;
/// The most pairs of polygons a drawing compares, and pieces it clips.
pub const MAX_WORK: u64 = 50_000_000;

/// One seen part of a face.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    /// Its corners in the picture, counter-clockwise; convex.
    pub points: Vec<Pt>,
    /// The depth of its middle (along the view, larger farther).
    pub depth: f64,
    /// Its shade, 0 to 1 (see the module).
    pub shade: f64,
    /// The face it is part of (an index into the mesh's faces).
    pub face: u32,
    /// That face's surface.
    pub surface: u32,
}

/// A solid drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    /// The seen parts of faces, farthest first.
    pub polygons: Vec<Drawn>,
    /// The outline of the picture: rings, outer counter-clockwise, holes clockwise.
    pub silhouette: Vec<Vec<Pt>>,
    /// Why the silhouette is empty, where it could not be found: its pieces did not close into
    /// rings, which happens where parts of the contour fall on one another in the picture to
    /// rounding (a coil seen along its axis, whose turns draw one circle; a stellated solid seen
    /// square on). The polygons are right all the same: they are found apart from it.
    pub silhouette_refused: Option<Error>,
    /// The light, in the world.
    pub light: P3,
    /// Whether the solid was found convex (drawn by facing alone).
    pub convex: bool,
}

impl Projection {
    /// The rows `(ru, rv, rd)`: a point's picture is `(ru · p, rv · p)` and its depth `rd · p`.
    pub fn rows(&self) -> [P3; 3] {
        match *self {
            Projection::Orthographic { azimuth, elevation } => {
                let (sa, ca) = sin_cos(rad(azimuth));
                let (se, ce) = sin_cos(rad(elevation));
                let (se, ce) = if elevation == 90.0 { (1.0, 0.0) } else if elevation == -90.0 { (-1.0, 0.0) } else { (se, ce) };
                let d = [ce * sa, se, ce * ca];
                let right = [ca, 0.0, -sa];
                let up = cross3(d, right);
                [right, up, scale3(d, -1.0)]
            }
            Projection::Oblique { angle, depth } => {
                let (s, c) = sin_cos(rad(angle));
                [[1.0, 0.0, -depth * c], [0.0, 1.0, -depth * s], [0.0, 0.0, -1.0]]
            }
        }
    }
    /// The direction toward the viewer along the rays (the rows' first two crossed).
    pub fn toward(&self) -> P3 {
        let r = self.rows();
        cross3(r[0], r[1])
    }
    /// The light, in the world.
    pub fn light(&self) -> P3 {
        let r = self.rows();
        let t = norm3(self.toward());
        let (right, up) = match self {
            Projection::Orthographic { .. } => (r[0], r[1]),
            Projection::Oblique { .. } => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        };
        let t = if matches!(self, Projection::Oblique { .. }) { [0.0, 0.0, 1.0] } else { t };
        norm3(add3(add3(scale3(right, -1.0), scale3(up, 2.0)), scale3(t, 2.0)))
    }
    fn check(&self) -> Result<(), Error> {
        match *self {
            Projection::Orthographic { azimuth, elevation } => {
                if !(azimuth.is_finite() && azimuth.abs() <= 360.0) {
                    return Err(Error::OutOfRange { form: "draw", param: "azimuth", value: azimuth, min: -360.0, max: 360.0 });
                }
                if !(elevation.is_finite() && elevation.abs() <= 90.0) {
                    return Err(Error::OutOfRange { form: "draw", param: "elevation", value: elevation, min: -90.0, max: 90.0 });
                }
            }
            Projection::Oblique { angle, depth } => {
                if !(angle.is_finite() && angle.abs() <= 360.0) {
                    return Err(Error::OutOfRange { form: "draw", param: "angle", value: angle, min: -360.0, max: 360.0 });
                }
                if !(depth.is_finite() && (0.05..=2.0).contains(&depth)) {
                    return Err(Error::OutOfRange { form: "draw", param: "depth", value: depth, min: 0.05, max: 2.0 });
                }
            }
        }
        Ok(())
    }
}

fn turn(a: Pt, b: Pt, c: Pt) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn area2(p: &[Pt]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum()
}

fn centroid(p: &[Pt]) -> Pt {
    let (mut cx, mut cy, mut a) = (0.0, 0.0, 0.0);
    let o = p[0];
    for i in 1..p.len().saturating_sub(1) {
        let (b, c) = (sub(p[i], o), sub(p[i + 1], o));
        let t = b[0] * c[1] - b[1] * c[0];
        a += t;
        cx += t * (b[0] + c[0]);
        cy += t * (b[1] + c[1]);
    }
    if a == 0.0 {
        let k = p.len() as f64;
        let s = p.iter().fold([0.0, 0.0], |s, q| add(s, *q));
        return [s[0] / k, s[1] / k];
    }
    [o[0] + cx / (3.0 * a), o[1] + cy / (3.0 * a)]
}

/// The part of the convex polygon `p` on the left of the line `a → b` (`keep_left`) or on its
/// right, by Sutherland and Hodgman's step.
fn half(p: &[Pt], a: Pt, b: Pt, keep_left: bool) -> Vec<Pt> {
    let side = |q: Pt| {
        let t = turn(a, b, q);
        if keep_left {
            t
        } else {
            -t
        }
    };
    let n = p.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let (c, d) = (p[i], p[(i + 1) % n]);
        let (sc, sd) = (side(c), side(d));
        if sc >= 0.0 {
            out.push(c);
        }
        if (sc > 0.0 && sd < 0.0) || (sc < 0.0 && sd > 0.0) {
            let t = sc / (sc - sd);
            out.push([c[0] + (d[0] - c[0]) * t, c[1] + (d[1] - c[1]) * t]);
        }
    }
    out
}

/// The overlap of two convex counter-clockwise polygons.
fn overlap(p: &[Pt], q: &[Pt]) -> Vec<Pt> {
    let mut r = p.to_vec();
    for i in 0..q.len() {
        if r.len() < 3 {
            return Vec::new();
        }
        r = half(&r, q[i], q[(i + 1) % q.len()], true);
    }
    r
}

struct Poly {
    pts: Vec<Pt>,
    face: u32,
    lo: Pt,
    hi: Pt,
}

/// Draws `mesh` (a built solid: closed, wound outward, its faces not crossing) in `projection`.
pub fn draw(mesh: &Mesh, projection: Projection) -> Result<Drawing, Error> {
    projection.check()?;
    let rows = projection.rows();
    let toward = projection.toward();
    let light = projection.light();
    let pic: Vec<Pt> = mesh.points.iter().map(|p| [dot3(rows[0], *p), dot3(rows[1], *p)]).collect();
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for q in &pic {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let size = dist(lo, hi).max(f64::MIN_POSITIVE);
    let reach = pic.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs())).max(size);
    let tiny = 1e-24 * reach * reach;
    // The faces that face the viewer, as convex polygons in the picture.
    let mut facing = vec![false; mesh.faces.len()];
    let mut polys: Vec<Poly> = Vec::new();
    let tris = mesh.triangles();
    let mut tri_of: Vec<Vec<[u32; 3]>> = vec![Vec::new(); mesh.faces.len()];
    for (f, t) in &tris {
        tri_of[*f as usize].push(*t);
    }
    for (fi, f) in mesh.faces.iter().enumerate() {
        let pts: Vec<Pt> = f.verts.iter().map(|&v| pic[v as usize]).collect();
        let a = 0.5 * area2(&pts);
        let bound = 64.0 * f64::EPSILON * reach * reach * f.verts.len() as f64;
        let av = mesh.area_vector(f);
        if a <= bound || dot3(av, toward) <= 0.0 {
            continue;
        }
        facing[fi] = true;
        let n = pts.len();
        let convex = (0..n).all(|i| turn(pts[i], pts[(i + 1) % n], pts[(i + 2) % n]) > 0.0);
        let mut push = |pts: Vec<Pt>| {
            let (mut l, mut h) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for q in &pts {
                for k in 0..2 {
                    l[k] = l[k].min(q[k]);
                    h[k] = h[k].max(q[k]);
                }
            }
            polys.push(Poly { pts, face: fi as u32, lo: l, hi: h });
        };
        if convex {
            push(pts);
        } else {
            for t in &tri_of[fi] {
                let tp: Vec<Pt> = t.iter().map(|&v| pic[v as usize]).collect();
                if 0.5 * area2(&tp) > tiny {
                    push(tp);
                }
            }
        }
    }
    let convex = is_convex(mesh);
    // Depth on a face's plane at a point of the picture: the ray `Q(u, v) + z R` meets the plane
    // `n · p = n · p₀` at `z = (n · p₀ − n · Q) / (n · R)`, `Q` and `R` from the inverse rows.
    let inv = invert(rows);
    let depth_on = |face: u32, q: Pt| -> f64 {
        let f = &mesh.faces[face as usize];
        let n = mesh.area_vector(f);
        let p0 = mesh.points[f.verts[0] as usize];
        let qq = [inv[0][0] * q[0] + inv[0][1] * q[1], inv[1][0] * q[0] + inv[1][1] * q[1], inv[2][0] * q[0] + inv[2][1] * q[1]];
        let r = [inv[0][2], inv[1][2], inv[2][2]];
        (dot3(n, p0) - dot3(n, qq)) / dot3(n, r)
    };
    let mut pieces: Vec<(Vec<Pt>, u32)> = Vec::new();
    if convex {
        pieces = polys.iter().map(|p| (p.pts.clone(), p.face)).collect();
    } else {
        // The pairs whose boxes meet, by a grid over the picture.
        let side = ((polys.len() as f64).sqrt() as usize).clamp(1, 1024);
        let cell = |x: f64, k: usize| (((x - lo[k]) / (hi[k] - lo[k]).max(f64::MIN_POSITIVE) * side as f64) as usize).min(side - 1);
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); side * side];
        let mut work = 0u64;
        for (i, p) in polys.iter().enumerate() {
            for gx in cell(p.lo[0], 0)..=cell(p.hi[0], 0) {
                for gy in cell(p.lo[1], 1)..=cell(p.hi[1], 1) {
                    grid[gx * side + gy].push(i as u32);
                    work += 1;
                }
            }
        }
        let mut occluders: Vec<Vec<u32>> = vec![Vec::new(); polys.len()];
        let mut seen: Vec<u32> = vec![u32::MAX; polys.len()];
        for i in 0..polys.len() {
            let p = &polys[i];
            for gx in cell(p.lo[0], 0)..=cell(p.hi[0], 0) {
                for gy in cell(p.lo[1], 1)..=cell(p.hi[1], 1) {
                    for &j in &grid[gx * side + gy] {
                        let j = j as usize;
                        if j <= i || seen[j] == i as u32 {
                            continue;
                        }
                        seen[j] = i as u32;
                        work += 1;
                        if work > MAX_WORK {
                            return Err(too_large(polys.len(), work));
                        }
                        let q = &polys[j];
                        if q.face == p.face || (0..2).any(|k| p.hi[k] < q.lo[k] || q.hi[k] < p.lo[k]) {
                            continue;
                        }
                        let o = overlap(&p.pts, &q.pts);
                        if o.len() < 3 {
                            continue;
                        }
                        let oa = 0.5 * area2(&o);
                        let scale = (0.5 * area2(&p.pts)).min(0.5 * area2(&q.pts));
                        if oa <= 1e-9 * scale + tiny {
                            continue;
                        }
                        let c = centroid(&o);
                        let (zi, zj) = (depth_on(p.face, c), depth_on(q.face, c));
                        let tol = 1e-12 * reach.max(zi.abs()).max(zj.abs());
                        if (zi - zj).abs() <= tol {
                            return Err(Error::Constraint { form: "draw", rule: "faces_too_near_in_depth", message: "two faces overlap in the picture with depths within rounding of each other: which is in front cannot be told soundly".into(), values: vec![("face_a", p.face as f64), ("face_b", q.face as f64), ("depth_a", zi), ("depth_b", zj)] });
                        }
                        if zj < zi {
                            occluders[i].push(j as u32);
                        } else {
                            occluders[j].push(i as u32);
                        }
                    }
                }
            }
        }
        for (i, p) in polys.iter().enumerate() {
            let mut keep = vec![p.pts.clone()];
            for &j in &occluders[i] {
                let c = &polys[j as usize].pts;
                let mut next = Vec::with_capacity(keep.len());
                for piece in keep {
                    let mut rest = piece;
                    for k in 0..c.len() {
                        if rest.len() < 3 {
                            break;
                        }
                        let (a, b) = (c[k], c[(k + 1) % c.len()]);
                        let out = half(&rest, a, b, false);
                        if out.len() >= 3 && 0.5 * area2(&out) > 1e-15 * size * size {
                            next.push(out);
                        }
                        rest = half(&rest, a, b, true);
                    }
                    work += c.len() as u64;
                }
                keep = next;
                if work > MAX_WORK || pieces.len() + keep.len() > MAX_PIECES {
                    return Err(too_large(pieces.len() + keep.len(), work));
                }
            }
            pieces.extend(keep.into_iter().map(|k| (k, p.face)));
        }
    }
    if pieces.len() > MAX_PIECES {
        return Err(too_large(pieces.len(), 0));
    }
    let mut polygons: Vec<Drawn> = pieces
        .into_iter()
        .map(|(p, f)| (clean(p, 1e-12 * size), f))
        .filter(|(p, _)| p.len() >= 3)
        .map(|(points, face)| {
            let c = centroid(&points);
            let f = &mesh.faces[face as usize];
            let n = norm3(mesh.area_vector(f));
            Drawn { depth: depth_on(face, c), shade: (0.5 + 0.5 * dot3(n, light)).clamp(0.0, 1.0), face, surface: f.surface, points }
        })
        .collect();
    polygons.sort_by(|a, b| b.depth.total_cmp(&a.depth).then(a.face.cmp(&b.face)));
    let (silhouette, silhouette_refused) = match outline(mesh, &pic, &facing, &polys, lo, hi, convex) {
        Ok(s) => (s, None),
        Err(e @ Error::Constraint { rule: "silhouette_not_closed", .. }) => (Vec::new(), Some(e)),
        Err(e) => return Err(e),
    };
    Ok(Drawing { polygons, silhouette, silhouette_refused, light, convex })
}

/// The polygon with each point within `eps` of the one before it left out (a clipped piece's
/// corners where a cut passed through one).
fn clean(p: Vec<Pt>, eps: f64) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(p.len());
    for q in p {
        if out.last().is_none_or(|l| dist(*l, q) > eps) {
            out.push(q);
        }
    }
    while out.len() >= 2 && dist(out[0], out[out.len() - 1]) <= eps {
        out.pop();
    }
    out
}

fn too_large(pieces: usize, work: u64) -> Error {
    Error::Constraint { form: "draw", rule: "drawing_too_large", message: "the drawing would have more polygons, or take more work, than its ceilings allow (a finer detail of a solid that folds over itself many times): draw a coarser build".into(), values: vec![("pieces", pieces as f64), ("max_pieces", MAX_PIECES as f64), ("work", work as f64), ("max_work", MAX_WORK as f64)] }
}

/// The inverse of a 3 × 3 matrix given by rows.
fn invert(m: [P3; 3]) -> [P3; 3] {
    let (a, b, c) = (m[0], m[1], m[2]);
    let det = dot3(a, cross3(b, c));
    let (x, y, z) = (cross3(b, c), cross3(c, a), cross3(a, b));
    // The inverse's columns are x, y, z over the determinant: its rows are their components.
    [[x[0] / det, y[0] / det, z[0] / det], [x[1] / det, y[1] / det, z[1] / det], [x[2] / det, y[2] / det, z[2] / det]]
}

/// Whether every edge's neighbouring face lies on the inner side of each face's plane (to
/// rounding): then the closed surface is convex.
pub(crate) fn is_convex(mesh: &Mesh) -> bool {
    let reach = mesh.reach();
    let tol = 1e-12 * reach;
    let mut edges: Vec<(u32, u32, u32)> = Vec::new();
    for (fi, f) in mesh.faces.iter().enumerate() {
        let n = f.verts.len();
        for i in 0..n {
            edges.push((f.verts[i], f.verts[(i + 1) % n], fi as u32));
        }
    }
    edges.sort_unstable();
    for (fi, f) in mesh.faces.iter().enumerate() {
        let nrm = norm3(mesh.area_vector(f));
        let p0 = mesh.points[f.verts[0] as usize];
        let n = f.verts.len();
        for i in 0..n {
            let (a, b) = (f.verts[i], f.verts[(i + 1) % n]);
            let Ok(k) = edges.binary_search_by(|e| (e.0, e.1).cmp(&(b, a))) else { return false };
            let g = &mesh.faces[edges[k].2 as usize];
            for &v in &g.verts {
                if dot3(nrm, sub3(mesh.points[v as usize], p0)) > tol {
                    return false;
                }
            }
        }
        let _ = fi;
    }
    true
}

/// The silhouette (see the module).
#[allow(clippy::too_many_arguments)]
fn outline(mesh: &Mesh, pic: &[Pt], facing: &[bool], polys: &[Poly], lo: Pt, hi: Pt, convex: bool) -> Result<Vec<Vec<Pt>>, Error> {
    let size = dist(lo, hi).max(f64::MIN_POSITIVE);
    // The contour: directed edges of front faces whose other face is not front (the front face on
    // the edge's left in the picture).
    let mut owner: Vec<(u32, u32, u32)> = Vec::new();
    for (fi, f) in mesh.faces.iter().enumerate() {
        let n = f.verts.len();
        for i in 0..n {
            owner.push((f.verts[i], f.verts[(i + 1) % n], fi as u32));
        }
    }
    owner.sort_unstable();
    let mut segs: Vec<(Pt, Pt)> = Vec::new();
    for (fi, f) in mesh.faces.iter().enumerate() {
        if !facing[fi] {
            continue;
        }
        let n = f.verts.len();
        for i in 0..n {
            let (a, b) = (f.verts[i], f.verts[(i + 1) % n]);
            let other = owner.binary_search_by(|e| (e.0, e.1).cmp(&(b, a))).ok().map(|k| owner[k].2 as usize);
            if other.is_none_or(|g| !facing[g]) {
                segs.push((pic[a as usize], pic[b as usize]));
            }
        }
    }
    if convex {
        // A convex solid's picture is the hull of its points' (a parallel projection of a convex
        // body is convex).
        return Ok(vec![hull(pic)]);
    }
    // Cut the contour where its pieces cross or touch.
    let n = segs.len();
    let side = ((n as f64).sqrt() as usize).clamp(1, 512);
    let cell = |x: f64, k: usize| (((x - lo[k]) / (hi[k] - lo[k]).max(f64::MIN_POSITIVE) * side as f64) as usize).min(side - 1);
    let mut grid: Vec<Vec<u32>> = vec![Vec::new(); side * side];
    for (i, (a, b)) in segs.iter().enumerate() {
        for gx in cell(a[0].min(b[0]), 0)..=cell(a[0].max(b[0]), 0) {
            for gy in cell(a[1].min(b[1]), 1)..=cell(a[1].max(b[1]), 1) {
                grid[gx * side + gy].push(i as u32);
            }
        }
    }
    let mut cuts: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; n];
    let mut work = 0u64;
    let mut seen = vec![u32::MAX; n];
    for i in 0..n {
        let (a, b) = segs[i];
        for gx in cell(a[0].min(b[0]), 0)..=cell(a[0].max(b[0]), 0) {
            for gy in cell(a[1].min(b[1]), 1)..=cell(a[1].max(b[1]), 1) {
                for &j in &grid[gx * side + gy] {
                    let j = j as usize;
                    if j <= i || seen[j] == i as u32 {
                        continue;
                    }
                    seen[j] = i as u32;
                    work += 1;
                    if work > MAX_WORK {
                        return Err(too_large(n, work));
                    }
                    let (c, d) = segs[j];
                    let r = sub(b, a);
                    let s = sub(d, c);
                    let den = r[0] * s[1] - r[1] * s[0];
                    let eps = 1e-12;
                    if den.abs() > 1e-18 * len(r) * len(s) {
                        let w = sub(c, a);
                        let t = (w[0] * s[1] - w[1] * s[0]) / den;
                        let u = (w[0] * r[1] - w[1] * r[0]) / den;
                        if (-eps..=1.0 + eps).contains(&t) && (-eps..=1.0 + eps).contains(&u) {
                            cuts[i].push(t.clamp(0.0, 1.0));
                            cuts[j].push(u.clamp(0.0, 1.0));
                        }
                    } else {
                        // Along each other: each one's ends that lie on the other cut it.
                        let on = |p: Pt, x: Pt, y: Pt| -> Option<f64> {
                            let e = sub(y, x);
                            let l2 = e[0] * e[0] + e[1] * e[1];
                            let t = ((p[0] - x[0]) * e[0] + (p[1] - x[1]) * e[1]) / l2;
                            let off = turn(x, y, p).abs() / l2.sqrt();
                            (off <= 1e-12 * size && t > 0.0 && t < 1.0).then_some(t)
                        };
                        for p in [c, d] {
                            if let Some(t) = on(p, a, b) {
                                cuts[i].push(t);
                            }
                        }
                        for p in [a, b] {
                            if let Some(t) = on(p, c, d) {
                                cuts[j].push(t);
                            }
                        }
                    }
                }
            }
        }
    }
    // Coverage: a point strictly inside a front polygon (by a hair past rounding).
    let pside = ((polys.len() as f64).sqrt() as usize).clamp(1, 1024);
    let pcell = |x: f64, k: usize| (((x - lo[k]) / (hi[k] - lo[k]).max(f64::MIN_POSITIVE) * pside as f64) as usize).min(pside - 1);
    let mut pgrid: Vec<Vec<u32>> = vec![Vec::new(); pside * pside];
    for (i, p) in polys.iter().enumerate() {
        for gx in pcell(p.lo[0], 0)..=pcell(p.hi[0], 0) {
            for gy in pcell(p.lo[1], 1)..=pcell(p.hi[1], 1) {
                pgrid[gx * pside + gy].push(i as u32);
            }
        }
    }
    let covered = |q: Pt| -> bool {
        if q[0] < lo[0] || q[0] > hi[0] || q[1] < lo[1] || q[1] > hi[1] {
            return false;
        }
        pgrid[pcell(q[0], 0) * pside + pcell(q[1], 1)].iter().any(|&i| {
            let p = &polys[i as usize].pts;
            let k = p.len();
            (0..k).all(|e| {
                let (a, b) = (p[e], p[(e + 1) % k]);
                turn(a, b, q) / dist(a, b).max(f64::MIN_POSITIVE) > 1e-13 * size
            })
        })
    };
    let delta = 1e-10 * size;
    let mut keep: Vec<(Pt, Pt)> = Vec::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        let mut ts = cuts[i].clone();
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() <= 1e-12);
        let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        for w in ts.windows(2) {
            let (p, q) = (at(w[0]), at(w[1]));
            let d = sub(q, p);
            let l = len(d);
            if l <= 1e-12 * size {
                continue;
            }
            // Between two cuts the cover beside a piece does not change (its edge is made of the
            // contour, which is cut wherever it meets itself), so it is tried at three points and
            // taken as covered if one is: a point that falls on the shared edge of two front
            // polygons is in neither strictly.
            let right = [d[1] / l * delta, -d[0] / l * delta];
            let beside = |t: f64| add([p[0] + d[0] * t, p[1] + d[1] * t], right);
            if ![0.293, 0.5, 0.707].iter().any(|&t| covered(beside(t))) {
                keep.push((p, q));
            }
        }
    }
    chain(keep, size)
}

/// The convex hull of points, counter-clockwise (Andrew's monotone chain), leaving out points on
/// its edges.
fn hull(pts: &[Pt]) -> Vec<Pt> {
    let mut p = pts.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut lower: Vec<Pt> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && turn(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Pt> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && turn(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The rings that directed pieces make, end to start (ends within a hair of each other are one
/// point); where several leave one point, the one turning farthest right from the way in.
fn chain(segs: Vec<(Pt, Pt)>, size: f64) -> Result<Vec<Vec<Pt>>, Error> {
    let hair = 1e-11 * size;
    let key = |p: Pt| ((p[0] / hair) as i64, (p[1] / hair) as i64);
    // Points that are one: a grid of hairs, each point to the first point seen in its cell or a
    // neighbouring one.
    type Cells = std::collections::BTreeMap<(i64, i64), Vec<u32>>;
    let mut ids: Cells = Cells::new();
    let mut pts: Vec<Pt> = Vec::new();
    let id = |p: Pt, ids: &mut Cells, pts: &mut Vec<Pt>| -> u32 {
        let (kx, ky) = key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = ids.get(&(kx + dx, ky + dy)) {
                    if let Some(&i) = v.iter().find(|&&i| dist(pts[i as usize], p) <= hair) {
                        return i;
                    }
                }
            }
        }
        let i = pts.len() as u32;
        pts.push(p);
        ids.entry((kx, ky)).or_default().push(i);
        i
    };
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for (a, b) in segs {
        let (i, j) = (id(a, &mut ids, &mut pts), id(b, &mut ids, &mut pts));
        if i != j {
            edges.push((i, j));
        }
    }
    edges.sort_unstable();
    edges.dedup();
    let mut used = vec![false; edges.len()];
    let mut rings = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut ring = Vec::new();
        let mut e = start;
        let first = edges[start].0;
        let mut closed = false;
        for _ in 0..=edges.len() {
            used[e] = true;
            let (a, b) = edges[e];
            ring.push(pts[a as usize]);
            if b == first {
                closed = true;
                break;
            }
            // The edges leaving b not used: the one turning farthest right.
            let from = edges.partition_point(|x| x.0 < b);
            let to = edges.partition_point(|x| x.0 <= b);
            let din = sub(pts[b as usize], pts[a as usize]);
            let mut best: Option<(f64, usize)> = None;
            for (k, &u) in used.iter().enumerate().take(to).skip(from) {
                if u {
                    continue;
                }
                let dout = sub(pts[edges[k].1 as usize], pts[b as usize]);
                let ang = atan2(din[0] * dout[1] - din[1] * dout[0], din[0] * dout[0] + din[1] * dout[1]);
                if best.is_none_or(|(x, _)| ang < x) {
                    best = Some((ang, k));
                }
            }
            match best {
                Some((_, k)) => e = k,
                None => return Err(Error::Constraint { form: "draw", rule: "silhouette_not_closed", message: "the outline's pieces do not close into rings (parts of the contour fall on one another in the picture)".into(), values: vec![("point_u", pts[b as usize][0]), ("point_v", pts[b as usize][1])] }),
            }
        }
        if !closed {
            return Err(Error::Constraint { form: "draw", rule: "silhouette_not_closed", message: "the outline's pieces do not close into rings (parts of the contour fall on one another in the picture)".into(), values: vec![("pieces", ring.len() as f64)] });
        }
        if ring.len() >= 3 {
            rings.push(ring);
        }
    }
    rings.sort_by(|a, b| area2(b).abs().total_cmp(&area2(a).abs()));
    Ok(rings)
}
