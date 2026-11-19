//! Solids of flat faces: the box and the wedge; the Platonic, Archimedean and Catalan solids;
//! prisms, antiprisms, pyramids, bipyramids, trapezohedra and frusta; and two stars in space.
//!
//! Every solid stands on `y = 0` with its middle on the Y axis, as the modeller's solids do
//! (`crates/specula-model/src/shape.rs`: "Solids rest on y = 0 and are centred on the Y axis").
//!
//! **The regular solids** are made from their points: the Platonic and Archimedean solids from
//! the coordinates of the literature (signed permutations of a few numbers; the snub cube's
//! from the tribonacci constant; the snub dodecahedron as an orbit, [`snub_orbit`]), joined by the edges of
//! least length and their faces traced round each edge ([`Poly::by_edges`]); the Catalan solids
//! as the Archimedean solids' polar duals ([`Poly::dual`]). The tests hold every one against the
//! counts of points, edges and faces of the literature, all edges equal and all points on one
//! sphere (convex), and the duals' faces congruent and all tangent to one sphere: wrong
//! coordinates cannot pass them. Each is computed once, at an edge (or an inradius) of 1, and
//! scaled.

use crate::anchor::{at3, out3, Anchor, On, MAX_NUMBERED};
use crate::error::{refuse, Refusal};
use crate::forms::polygons::{regular, star};
use crate::geom::{poly, Geom};
use crate::maths::*;
use crate::solid::{Cone, Extrude, Poly};
use crate::spec::Args;
use crate::table::{count, len, Alias, Form, Param, Set};
use crate::Measures;
use std::rc::Rc;
use std::sync::OnceLock;

/// φ, the golden ratio.
fn phi() -> f64 {
    0.5 * (1.0 + 5.0f64.sqrt())
}

/// The real root of a cubic `x³ + b x² + c x + d` near `x0`, by Newton's method.
fn cubic_root(b: f64, c: f64, d: f64, x0: f64) -> f64 {
    let mut x = x0;
    for _ in 0..64 {
        let f = ((x + b) * x + c) * x + d;
        let df = (3.0 * x + 2.0 * b) * x + c;
        let dx = f / df;
        x -= dx;
        if dx.abs() <= 1e-16 * x.abs() {
            break;
        }
    }
    x
}

/// The cyclic (even) permutations of a triple.
fn even(v: P3) -> [P3; 3] {
    [v, [v[1], v[2], v[0]], [v[2], v[0], v[1]]]
}
/// The odd permutations of a triple.
fn odd(v: P3) -> [P3; 3] {
    [[v[0], v[2], v[1]], [v[2], v[1], v[0]], [v[1], v[0], v[2]]]
}
/// Every choice of signs of a triple, with the number of plus signs (counting a zero as
/// neither) whose parity `plus` asks for (`None`: any).
fn signed(v: P3, plus: Option<usize>) -> Vec<P3> {
    let mut out = Vec::new();
    for m in 0..8 {
        let p: P3 = [0, 1, 2].map(|i| if m & (1 << i) != 0 { -v[i] } else { v[i] });
        let pluses = p.iter().filter(|x| **x > 0.0).count();
        let minuses = p.iter().filter(|x| **x < 0.0).count();
        let ok = match plus {
            None => true,
            Some(0) => pluses % 2 == 0 && minuses + pluses == 3,
            Some(_) => pluses % 2 == 1 && minuses + pluses == 3,
        };
        if ok {
            out.push(p);
        }
    }
    out
}
/// Even minus signs (for the truncated tetrahedron and the tetrahedron).
fn even_minus(v: P3) -> Vec<P3> {
    (0..8)
        .map(|m: u32| [0, 1, 2].map(|i| if m & (1 << i) != 0 { -v[i] } else { v[i] }))
        .filter(|p| p.iter().filter(|x| **x < 0.0).count() % 2 == 0)
        .collect()
}
fn all(v: P3) -> Vec<P3> {
    even(v).into_iter().chain(odd(v)).flat_map(|p| signed(p, None)).collect()
}
fn cyc(v: P3) -> Vec<P3> {
    even(v).into_iter().flat_map(|p| signed(p, None)).collect()
}
fn dedup(v: Vec<P3>) -> Vec<P3> {
    // (A sign put on a zero makes −0, which is the same point.)
    let mut v: Vec<P3> = v.into_iter().map(|p| p.map(|x| x + 0.0)).collect();
    v.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])).then(a[2].total_cmp(&b[2])));
    v.dedup();
    v
}

/// One regular solid: its name, its points, and the literature's numbers the tests hold it to.
pub(crate) struct Regular {
    pub name: &'static str,
    pub points: fn() -> Vec<P3>,
    /// Points, edges and faces, from the literature (the tests hold the built solid to them).
    #[cfg_attr(not(test), allow(dead_code, reason = "read by the tests only"))]
    pub vef: (usize, usize, usize),
    /// Its faces by number of sides: `(sides, how many)`.
    pub faces: &'static [(usize, usize)],
    /// Its volume at edge 1, where it is known in closed form.
    pub volume: fn() -> Option<f64>,
}

fn tet() -> Vec<P3> {
    even_minus([1.0, 1.0, 1.0])
}
fn cube() -> Vec<P3> {
    signed([1.0, 1.0, 1.0], None)
}
fn oct() -> Vec<P3> {
    dedup(all([1.0, 0.0, 0.0]))
}
fn ico() -> Vec<P3> {
    dedup(cyc([0.0, 1.0, phi()]))
}
fn dod() -> Vec<P3> {
    let f = phi();
    dedup(signed([1.0, 1.0, 1.0], None).into_iter().chain(cyc([0.0, 1.0 / f, f])).collect())
}
fn t_tet() -> Vec<P3> {
    dedup(even([3.0, 1.0, 1.0]).into_iter().chain(odd([3.0, 1.0, 1.0])).flat_map(even_minus).collect())
}
fn cuboct() -> Vec<P3> {
    dedup(all([1.0, 1.0, 0.0]))
}
fn t_cube() -> Vec<P3> {
    dedup(all([2.0f64.sqrt() - 1.0, 1.0, 1.0]))
}
fn t_oct() -> Vec<P3> {
    dedup(all([0.0, 1.0, 2.0]))
}
fn rco() -> Vec<P3> {
    dedup(all([1.0, 1.0, 1.0 + 2.0f64.sqrt()]))
}
fn trco() -> Vec<P3> {
    let s = 2.0f64.sqrt();
    dedup(all([1.0, 1.0 + s, 1.0 + 2.0 * s]))
}
fn snub_cube() -> Vec<P3> {
    // t, the tribonacci constant: the real root of t³ − t² − t − 1.
    let t = cubic_root(-1.0, -1.0, -1.0, 1.8);
    let v = [1.0, 1.0 / t, t];
    dedup(even(v).into_iter().flat_map(|p| signed(p, Some(0))).chain(odd(v).into_iter().flat_map(|p| signed(p, Some(1)))).collect())
}
fn icosidodec() -> Vec<P3> {
    let f = phi();
    dedup(cyc([0.0, 0.0, f]).into_iter().chain(cyc([0.5, 0.5 * f, 0.5 * f * f])).collect())
}
fn t_dod() -> Vec<P3> {
    let f = phi();
    dedup([[0.0, 1.0 / f, 2.0 + f], [1.0 / f, f, 2.0 * f], [f, 2.0, f + 1.0]].into_iter().flat_map(cyc).collect())
}
fn t_ico() -> Vec<P3> {
    let f = phi();
    dedup([[0.0, 1.0, 3.0 * f], [1.0, 2.0 + f, 2.0 * f], [f, 2.0, f * f * f]].into_iter().flat_map(cyc).collect())
}
fn rid() -> Vec<P3> {
    let f = phi();
    dedup([[1.0, 1.0, f * f * f], [f * f, f, 2.0 * f], [2.0 + f, 0.0, f * f]].into_iter().flat_map(cyc).collect())
}
fn trid() -> Vec<P3> {
    let f = phi();
    let g = 1.0 / f;
    dedup([[g, g, 3.0 + f], [2.0 * g, f, 1.0 + 2.0 * f], [g, f * f, -1.0 + 3.0 * f], [2.0 * f - 1.0, 2.0, 2.0 + f], [f, 3.0, 2.0 * f]].into_iter().flat_map(cyc).collect())
}
/// `v` turned by `t` radians about the unit axis `u` (Rodrigues).
fn rot(u: P3, t: f64, v: P3) -> P3 {
    let (s, c) = sin_cos(t);
    add3(add3(scale3(v, c), scale3(cross3(u, v), s)), scale3(u, dot3(u, v) * (1.0 - c)))
}

/// A snub solid as the orbit of one point under a rotation group: the group of the rotations
/// by `2π / n` about `an` (a vertex of the fundamental triangle, `n` = 4 for the cube's, 5 for
/// the dodecahedron's) and by `2π / 3` about `a3`; the point is where its distances to its
/// turns about `an`, about `a3` and by a half turn about `a2` (the third corner) are equal, so
/// that every edge of the solid (a polygon's about `an`, a triangle's about `a3` and the edge
/// across `a2`) is one length. It is found by Newton's method from the triangle's middle. The
/// snub cube made this way is held by a test against its coordinates from the tribonacci
/// constant.
pub(crate) fn snub_orbit(an: P3, n: f64, a3: P3, a2: P3) -> Vec<P3> {
    let (an, a3, a2) = (norm3(an), norm3(a3), norm3(a2));
    let tn = TAU / n;
    let f = |p: P3| {
        let d2 = |q: P3| {
            let d = sub3(p, q);
            dot3(d, d)
        };
        let (x, y, z) = (d2(rot(an, tn, p)), d2(rot(a3, TAU / 3.0, p)), d2(rot(a2, PI, p)));
        [x - z, y - z]
    };
    // The point by two angles in a frame about the triangle's middle.
    let m = norm3(add3(add3(an, a3), a2));
    let e1 = norm3(sub3(an, scale3(m, dot3(an, m))));
    let e2 = cross3(m, e1);
    let at = |u: f64, w: f64| norm3(add3(add3(m, scale3(e1, u)), scale3(e2, w)));
    let (mut u, mut w) = (0.0, 0.0);
    for _ in 0..100 {
        let r = f(at(u, w));
        let h = 1e-7;
        let (fu, fw) = (f(at(u + h, w)), f(at(u, w + h)));
        let j = [[(fu[0] - r[0]) / h, (fw[0] - r[0]) / h], [(fu[1] - r[1]) / h, (fw[1] - r[1]) / h]];
        let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
        let du = (r[0] * j[1][1] - r[1] * j[0][1]) / det;
        let dw = (j[0][0] * r[1] - j[1][0] * r[0]) / det;
        u -= du;
        w -= dw;
        if du.abs() + dw.abs() <= 1e-16 {
            break;
        }
    }
    let p = at(u, w);
    // The orbit: everything the two generating turns reach.
    let mut pts = vec![p];
    let mut i = 0;
    while i < pts.len() && pts.len() <= 120 {
        for q in [rot(an, tn, pts[i]), rot(a3, TAU / 3.0, pts[i])] {
            if !pts.iter().any(|x| dist3(*x, q) <= 1e-9) {
                pts.push(q);
            }
        }
        i += 1;
    }
    pts
}

/// The fundamental triangle of a solid's rotations: a point of `pts` (an `n`-fold axis), the
/// middle of a face through it and of an edge of that face through it. `pts` are the points of
/// a Platonic solid whose faces about a point are triangles meeting there (`ico`: n = 5) or of
/// one whose points are the faces' middles of the other (`oct`: n = 4).
fn triangle_of(pts: &[P3]) -> (P3, P3, P3) {
    let v0 = pts[0];
    let least = pts[1..].iter().map(|p| dist3(*p, v0)).fold(f64::INFINITY, f64::min);
    let near: Vec<P3> = pts[1..].iter().copied().filter(|p| dist3(*p, v0) <= least * (1.0 + 1e-9)).collect();
    let v1 = near[0];
    let v2 = *near.iter().find(|p| (dist3(**p, v1) - least).abs() <= least * 1e-9).expect("a face through v0 and v1");
    (v0, scale3(add3(add3(v0, v1), v2), 1.0 / 3.0), scale3(add3(v0, v1), 0.5))
}

fn snub_dod() -> Vec<P3> {
    let (a5, a3, a2) = triangle_of(&ico());
    snub_orbit(a5, 5.0, a3, a2)
}

/// The snub cube by the same orbit (a test holds it to [`snub_cube`]'s coordinates).
#[cfg(test)]
pub(crate) fn snub_cube_orbit() -> Vec<P3> {
    let (a4, a3, a2) = triangle_of(&oct());
    snub_orbit(a4, 4.0, a3, a2)
}

fn s2() -> f64 {
    2.0f64.sqrt()
}
fn s5() -> f64 {
    5.0f64.sqrt()
}

/// The Platonic solids (0..5, with the cube at 1) and the Archimedean (5..18), in this order.
pub(crate) const REGULAR: &[Regular] = &[
    Regular { name: "tetrahedron", points: tet, vef: (4, 6, 4), faces: &[(3, 4)], volume: || Some(1.0 / (6.0 * s2())) },
    Regular { name: "cube", points: cube, vef: (8, 12, 6), faces: &[(4, 6)], volume: || Some(1.0) },
    Regular { name: "octahedron", points: oct, vef: (6, 12, 8), faces: &[(3, 8)], volume: || Some(s2() / 3.0) },
    Regular { name: "dodecahedron", points: dod, vef: (20, 30, 12), faces: &[(5, 12)], volume: || Some((15.0 + 7.0 * s5()) / 4.0) },
    Regular { name: "icosahedron", points: ico, vef: (12, 30, 20), faces: &[(3, 20)], volume: || Some(5.0 * (3.0 + s5()) / 12.0) },
    Regular { name: "truncated_tetrahedron", points: t_tet, vef: (12, 18, 8), faces: &[(3, 4), (6, 4)], volume: || Some(23.0 * s2() / 12.0) },
    Regular { name: "cuboctahedron", points: cuboct, vef: (12, 24, 14), faces: &[(3, 8), (4, 6)], volume: || Some(5.0 * s2() / 3.0) },
    Regular { name: "truncated_cube", points: t_cube, vef: (24, 36, 14), faces: &[(3, 8), (8, 6)], volume: || Some((21.0 + 14.0 * s2()) / 3.0) },
    Regular { name: "truncated_octahedron", points: t_oct, vef: (24, 36, 14), faces: &[(4, 6), (6, 8)], volume: || Some(8.0 * s2()) },
    Regular { name: "rhombicuboctahedron", points: rco, vef: (24, 48, 26), faces: &[(3, 8), (4, 18)], volume: || Some((12.0 + 10.0 * s2()) / 3.0) },
    Regular { name: "truncated_cuboctahedron", points: trco, vef: (48, 72, 26), faces: &[(4, 12), (6, 8), (8, 6)], volume: || Some(22.0 + 14.0 * s2()) },
    Regular { name: "snub_cube", points: snub_cube, vef: (24, 60, 38), faces: &[(3, 32), (4, 6)], volume: || None },
    Regular { name: "icosidodecahedron", points: icosidodec, vef: (30, 60, 32), faces: &[(3, 20), (5, 12)], volume: || Some((45.0 + 17.0 * s5()) / 6.0) },
    Regular { name: "truncated_dodecahedron", points: t_dod, vef: (60, 90, 32), faces: &[(3, 20), (10, 12)], volume: || Some(5.0 * (99.0 + 47.0 * s5()) / 12.0) },
    Regular { name: "truncated_icosahedron", points: t_ico, vef: (60, 90, 32), faces: &[(5, 12), (6, 20)], volume: || Some((125.0 + 43.0 * s5()) / 4.0) },
    Regular { name: "rhombicosidodecahedron", points: rid, vef: (60, 120, 62), faces: &[(3, 20), (4, 30), (5, 12)], volume: || Some((60.0 + 29.0 * s5()) / 3.0) },
    Regular { name: "truncated_icosidodecahedron", points: trid, vef: (120, 180, 62), faces: &[(4, 30), (6, 20), (10, 12)], volume: || Some(95.0 + 50.0 * s5()) },
    Regular { name: "snub_dodecahedron", points: snub_dod, vef: (60, 150, 92), faces: &[(3, 80), (5, 12)], volume: || None },
];

/// The Catalan solids: each the dual of the Archimedean solid at `REGULAR[i]`.
pub(crate) const CATALAN: &[(&str, usize)] = &[
    ("triakis_tetrahedron", 5),
    ("rhombic_dodecahedron", 6),
    ("triakis_octahedron", 7),
    ("tetrakis_hexahedron", 8),
    ("deltoidal_icositetrahedron", 9),
    ("disdyakis_dodecahedron", 10),
    ("pentagonal_icositetrahedron", 11),
    ("rhombic_triacontahedron", 12),
    ("triakis_icosahedron", 13),
    ("pentakis_dodecahedron", 14),
    ("deltoidal_hexecontahedron", 15),
    ("disdyakis_triacontahedron", 16),
    ("pentagonal_hexecontahedron", 17),
];

/// The regular solids at edge 1 and the Catalan solids at inradius 1, centred on the origin,
/// computed once; the regular ones' circumradii.
pub(crate) fn units() -> &'static (Vec<Poly>, Vec<f64>, Vec<Poly>) {
    static U: OnceLock<(Vec<Poly>, Vec<f64>, Vec<Poly>)> = OnceLock::new();
    U.get_or_init(|| {
        let mut polys = Vec::new();
        let mut radii = Vec::new();
        for r in REGULAR {
            let p = Poly::by_edges((r.points)());
            let mesh = p.mesh();
            let f = &mesh.faces[0];
            let e = dist3(mesh.points[f.verts[0] as usize], mesh.points[f.verts[1] as usize]);
            let p = p.map(|q| scale3(q, 1.0 / e));
            radii.push(len3(p.pts[0]));
            polys.push(p);
        }
        // The dual in the sphere of radius √R (R the circumradius) has every face at R / R = 1.
        let cat = CATALAN.iter().map(|&(_, i)| polys[i].dual(radii[i], "faces")).collect();
        (polys, radii, cat)
    })
}

/// The closed forms of the regular solid `i` at edge `e`: the volume from the literature where
/// it is known, and the area as the sum of its regular faces' areas (`n e² / (4 tan(π / n))`
/// each).
fn regular_closed(i: usize, e: f64) -> Measures {
    let r = &REGULAR[i];
    let area: f64 = r.faces.iter().map(|&(n, c)| c as f64 * n as f64 * e * e / (4.0 * tan(PI / n as f64))).sum();
    Measures { size: (r.volume)().map(|v| v * e * e * e), boundary: Some(area), ..Measures::default() }
}

/// The scale of a regular solid's arguments: its `edge`, or its `radius` over the unit's
/// circumradius.
fn regular_scale(i: usize, a: &Args) -> f64 {
    if a.has("edge") {
        a.f("edge")
    } else {
        a.f("radius") / units().1[i]
    }
}

/// The anchors of a polyhedron as placed: its centre (the mean of its points, which its
/// symmetry fixes), its base and top (the middle of its lowest and of its highest points: a
/// face, an edge or a point it rests on), and its first points.
fn poly_anchors(p: &Poly) -> Vec<Anchor> {
    let n = p.pts.len() as f64;
    let c = scale3(p.pts.iter().fold([0.0; 3], |s, q| add3(s, *q)), 1.0 / n);
    let (lo, hi) = p.pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), q| (l.min(q[1]), h.max(q[1])));
    let reach = p.pts.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs()));
    let mid = |y: f64| {
        let near: Vec<&P3> = p.pts.iter().filter(|q| (q[1] - y).abs() <= 1e-12 * reach).collect();
        scale3(near.iter().fold([0.0; 3], |s, q| add3(s, **q)), 1.0 / near.len() as f64)
    };
    let mut v = vec![at3("centre", c, On::Inside), out3("base", mid(lo), [0.0, -1.0, 0.0], On::Boundary), out3("top", mid(hi), [0.0, 1.0, 0.0], On::Boundary)];
    v.extend(p.pts.iter().take(MAX_NUMBERED).enumerate().map(|(i, q)| out3(&format!("vertex_{i}"), *q, sub3(*q, c), On::Vertex)));
    v
}

macro_rules! regular_form {
    ($i:expr) => {{
        fn g(a: &Args) -> Result<Geom, Refusal> {
            Ok(Geom::Solid(Rc::new(units().0[$i].placed(regular_scale($i, a)))))
        }
        fn c(a: &Args) -> Measures {
            regular_closed($i, regular_scale($i, a) * 1.0)
        }
        fn an(a: &Args) -> Vec<Anchor> {
            poly_anchors(&units().0[$i].placed(regular_scale($i, a)))
        }
        (g as fn(&Args) -> Result<Geom, Refusal>, c as fn(&Args) -> Measures, an as fn(&Args) -> Vec<Anchor>)
    }};
}

macro_rules! catalan_form {
    ($i:expr) => {{
        fn g(a: &Args) -> Result<Geom, Refusal> {
            Ok(Geom::Solid(Rc::new(units().2[$i].placed(a.f("radius")))))
        }
        fn an(a: &Args) -> Vec<Anchor> {
            poly_anchors(&units().2[$i].placed(a.f("radius")))
        }
        (g as fn(&Args) -> Result<Geom, Refusal>, an as fn(&Args) -> Vec<Anchor>)
    }};
}

// ---------------------------------------------------------------- box and wedge

const P_BOX: &[Param] = &[len("width", "along x", 1.0), len("height", "along y", 1.0), len("depth", "along z", 1.0)];

fn box_poly(a: &Args) -> Poly {
    let (w, h, d) = (0.5 * a.f("width"), a.f("height"), 0.5 * a.f("depth"));
    let pts = (0..8).map(|i| [if i & 1 != 0 { w } else { -w }, if i & 2 != 0 { h } else { 0.0 }, if i & 4 != 0 { d } else { -d }]).collect();
    let faces = vec![vec![0, 1, 5, 4], vec![2, 6, 7, 3], vec![0, 4, 6, 2], vec![1, 3, 7, 5], vec![0, 2, 3, 1], vec![4, 5, 7, 6]];
    let names = ["bottom", "top", "left", "right", "back", "front"].map(String::from).to_vec();
    Poly { pts, faces, surf: (0..6).collect(), names }
}
fn box_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(box_poly(a))))
}
fn box_closed(a: &Args) -> Measures {
    let (w, h, d) = (a.f("width"), a.f("height"), a.f("depth"));
    Measures {
        size: Some(w * h * d),
        boundary: Some(2.0 * (w * h + h * d + d * w)),
        centroid: Some([0.0, 0.5 * h, 0.0]),
        bounds: Some(([-0.5 * w, 0.0, -0.5 * d], [0.5 * w, h, 0.5 * d])),
        series: None,
    }
}
fn box_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, d) = (0.5 * a.f("width"), a.f("height"), 0.5 * a.f("depth"));
    let mut v = vec![
        at3("centre", [0.0, 0.5 * h, 0.0], On::Inside),
        out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("left", [-w, 0.5 * h, 0.0], [-1.0, 0.0, 0.0], On::Boundary),
        out3("right", [w, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("back", [0.0, 0.5 * h, -d], [0.0, 0.0, -1.0], On::Boundary),
        out3("front", [0.0, 0.5 * h, d], [0.0, 0.0, 1.0], On::Boundary),
    ];
    let p = box_poly(a);
    v.extend(p.pts.iter().enumerate().map(|(i, q)| out3(&format!("corner_{i}"), *q, sub3(*q, [0.0, 0.5 * h, 0.0]), On::Vertex)));
    v
}

fn wedge_poly(a: &Args) -> Poly {
    let (w, h, d) = (0.5 * a.f("width"), a.f("height"), 0.5 * a.f("depth"));
    let pts = vec![[-w, 0.0, -d], [w, 0.0, -d], [-w, 0.0, d], [w, 0.0, d], [w, h, -d], [w, h, d]];
    let faces = vec![vec![0, 1, 3, 2], vec![1, 4, 5, 3], vec![0, 2, 5, 4], vec![0, 4, 1], vec![2, 3, 5]];
    let names = ["bottom", "back_wall", "slope", "back", "front"].map(String::from).to_vec();
    Poly { pts, faces, surf: (0..5).collect(), names }
}
fn wedge_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(wedge_poly(a))))
}
fn wedge_closed(a: &Args) -> Measures {
    let (w, h, d) = (a.f("width"), a.f("height"), a.f("depth"));
    let slope = (w * w + h * h).sqrt();
    Measures {
        size: Some(0.5 * w * h * d),
        boundary: Some(w * h + w * d + h * d + slope * d),
        centroid: Some([w / 6.0, h / 3.0, 0.0]),
        bounds: Some(([-0.5 * w, 0.0, -0.5 * d], [0.5 * w, h, 0.5 * d])),
        series: None,
    }
}
fn wedge_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("width"), a.f("height"));
    let mut v = vec![
        at3("centre", [w / 6.0, h / 3.0, 0.0], On::Inside),
        out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        out3("slope", [0.0, 0.5 * h, 0.0], [-h, w, 0.0], On::Boundary),
        out3("back_wall", [0.5 * w, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Boundary),
    ];
    v.extend(wedge_poly(a).pts.iter().enumerate().map(|(i, q)| at3(&format!("corner_{i}"), *q, On::Vertex)));
    v
}

// ---------------------------------------------------------------- prisms and their kin

const P_PRISM: &[Param] = &[count("sides", "the number of sides", 6.0, 3.0, 256.0), len("radius", "the circumradius of the end polygons", 0.5), len("height", "the height", 1.0)];
const P_FRUSTUM: &[Param] = &[count("sides", "the number of sides", 4.0, 3.0, 256.0), len("radius", "the circumradius of the bottom", 0.5), len("top", "the circumradius of the top", 0.25), len("height", "the height", 1.0)];
const P_STARPRISM: &[Param] = &[count("points", "the number of points", 5.0, 3.0, 100.0), len("outer", "centre to a point's tip", 0.5), len("inner", "centre to a notch between points", 0.2), len("height", "the height", 0.2)];
const P_EDGE: &[Param] = &[len("edge", "each edge", 1.0)];
const P_RADIUS: &[Param] = &[len("radius", "the radius of the sphere through every point", 1.0)];
const P_INRADIUS: &[Param] = &[len("radius", "the radius of the sphere every face touches", 1.0)];

/// A regular polygon's measures in the plane, from its pieces.
fn ring_measures(pts: &[Pt]) -> Measures {
    Geom::Rings(vec![poly(pts)]).exact()
}

/// The regular `n`-gon's area of circumradius `r`.
fn ngon_area(n: f64, r: f64) -> f64 {
    0.5 * n * r * r * sin(TAU / n)
}

fn prism_geom(a: &Args) -> Result<Geom, Refusal> {
    let pts = regular(a.n("sides"), a.f("radius"));
    Ok(Geom::Solid(Rc::new(Extrude { segs: poly(&pts), height: a.f("height"), plane: ring_measures(&pts) })))
}
fn prism_closed(a: &Args) -> Measures {
    let (n, r, h) = (a.f("sides"), a.f("radius"), a.f("height"));
    let side = 2.0 * r * sin(PI / n);
    Measures { size: Some(ngon_area(n, r) * h), boundary: Some(2.0 * ngon_area(n, r) + n * side * h), centroid: Some([0.0, 0.5 * h, 0.0]), ..Measures::default() }
}
fn prism_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    let mut v = vec![at3("centre", [0.0, 0.5 * h, 0.0], On::Inside), out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary)];
    let pts = regular(a.n("sides"), a.f("radius"));
    v.extend(pts.iter().take(MAX_NUMBERED).enumerate().map(|(i, p)| at3(&format!("vertex_{i}"), [p[0], 0.0, -p[1]], On::Vertex)));
    v
}

fn starprism_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (o, i) = (a.f("outer"), a.f("inner"));
    if i >= o {
        return Err(refuse("inner_inside_outer", "the notches must be nearer the centre than the tips", &[("outer", o), ("inner", i)]));
    }
    Ok(star(a.n("points"), o, i))
}
fn starprism_geom(a: &Args) -> Result<Geom, Refusal> {
    let pts = starprism_pts(a)?;
    Ok(Geom::Solid(Rc::new(Extrude { segs: poly(&pts), height: a.f("height"), plane: ring_measures(&pts) })))
}
fn starprism_closed(a: &Args) -> Measures {
    let (n, o, i, h) = (a.f("points"), a.f("outer"), a.f("inner"), a.f("height"));
    Measures { size: Some(n * o * i * sin(PI / n) * h), centroid: Some([0.0, 0.5 * h, 0.0]), ..Measures::default() }
}
fn starprism_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    let mut v = vec![at3("centre", [0.0, 0.5 * h, 0.0], On::Inside), out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary)];
    if let Ok(pts) = starprism_pts(a) {
        v.extend(pts.iter().step_by(2).take(MAX_NUMBERED).enumerate().map(|(i, p)| at3(&format!("tip_{i}"), [p[0], 0.0, -p[1]], On::Vertex)));
    }
    v
}

fn pyramid_geom(a: &Args) -> Result<Geom, Refusal> {
    let pts = regular(a.n("sides"), a.f("radius"));
    let (n, r, h) = (a.f("sides"), a.f("radius"), a.f("height"));
    let lateral = n * r * sin(PI / n) * (h * h + (r * cos(PI / n)) * (r * cos(PI / n))).sqrt();
    Ok(Geom::Solid(Rc::new(Cone { segs: poly(&pts), height: h, plane: ring_measures(&pts), lateral: Some(lateral) })))
}
fn pyramid_closed(a: &Args) -> Measures {
    let (n, r, h) = (a.f("sides"), a.f("radius"), a.f("height"));
    // The slant height of each face: from the base edge's middle (at r cos(π/n)) to the apex.
    let slant = (h * h + (r * cos(PI / n)) * (r * cos(PI / n))).sqrt();
    let side = 2.0 * r * sin(PI / n);
    Measures { size: Some(ngon_area(n, r) * h / 3.0), boundary: Some(ngon_area(n, r) + 0.5 * n * side * slant), centroid: Some([0.0, 0.25 * h, 0.0]), ..Measures::default() }
}
fn pyramid_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    let mut v = vec![at3("centre", [0.0, 0.25 * h, 0.0], On::Inside), out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), at3("apex", [0.0, h, 0.0], On::Vertex)];
    let pts = regular(a.n("sides"), a.f("radius"));
    v.extend(pts.iter().take(MAX_NUMBERED).enumerate().map(|(i, p)| at3(&format!("vertex_{i}"), [p[0], 0.0, -p[1]], On::Vertex)));
    v
}

/// A point of the ring of `n` at radius `r`, height `y`, turned by `t0` (a vertex on −z at 0,
/// as the plane polygon's on +y).
fn ring_pt(n: usize, i: usize, r: f64, y: f64, t0: f64) -> P3 {
    let (s, c) = sin_cos(0.5 * PI + t0 + TAU * i as f64 / n as f64);
    [r * c, y, -r * s]
}

fn antiprism_poly(a: &Args) -> Poly {
    let (n, r, h) = (a.n("sides"), a.f("radius"), a.f("height"));
    let mut pts: Vec<P3> = (0..n).map(|i| ring_pt(n, i, r, 0.0, 0.0)).collect();
    pts.extend((0..n).map(|i| ring_pt(n, i, r, h, PI / n as f64)));
    let n32 = n as u32;
    let mut faces = vec![(0..n32).rev().collect::<Vec<u32>>(), (n32..2 * n32).collect()];
    let mut surf = vec![0, 0];
    for i in 0..n32 {
        let j = (i + 1) % n32;
        faces.push(vec![i, j, n32 + i]);
        faces.push(vec![n32 + i, j, n32 + j]);
        surf.extend([1, 1]);
    }
    Poly { pts, faces, surf, names: vec!["ends".into(), "triangles".into()] }
}
fn antiprism_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(antiprism_poly(a))))
}
fn antiprism_closed(a: &Args) -> Measures {
    // The prismatoid formula, exact for a solid whose points lie on two parallel planes:
    // V = h (A_bottom + A_top + 4 A_middle) / 6; the middle section is the regular 2n-gon
    // through the side edges' middles, of circumradius r cos(π / 2n).
    let (n, r, h) = (a.f("sides"), a.f("radius"), a.f("height"));
    let rm = r * cos(PI / (2.0 * n));
    let mid = ngon_area(2.0 * n, rm);
    Measures { size: Some(h * (2.0 * ngon_area(n, r) + 4.0 * mid) / 6.0), centroid: Some([0.0, 0.5 * h, 0.0]), ..Measures::default() }
}
fn antiprism_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    let mut v = vec![at3("centre", [0.0, 0.5 * h, 0.0], On::Inside), out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary)];
    v.extend(antiprism_poly(a).pts.iter().take(MAX_NUMBERED).enumerate().map(|(i, p)| at3(&format!("vertex_{i}"), *p, On::Vertex)));
    v
}

fn frustum_poly(a: &Args) -> Result<Poly, Refusal> {
    let (n, r, t, h) = (a.n("sides"), a.f("radius"), a.f("top"), a.f("height"));
    if t >= r {
        return Err(refuse("top_under_bottom", "the top must be smaller than the bottom (turn it over for the other way)", &[("radius", r), ("top", t)]));
    }
    let mut pts: Vec<P3> = (0..n).map(|i| ring_pt(n, i, r, 0.0, 0.0)).collect();
    pts.extend((0..n).map(|i| ring_pt(n, i, t, h, 0.0)));
    let n32 = n as u32;
    let mut faces = vec![(0..n32).rev().collect::<Vec<u32>>(), (n32..2 * n32).collect()];
    let mut surf = vec![0, 1];
    for i in 0..n32 {
        let j = (i + 1) % n32;
        faces.push(vec![i, j, n32 + j, n32 + i]);
        surf.push(2);
    }
    Ok(Poly { pts, faces, surf, names: vec!["bottom".into(), "top".into(), "sides".into()] })
}
fn frustum_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(frustum_poly(a)?)))
}
fn frustum_closed(a: &Args) -> Measures {
    let (n, r, t, h) = (a.f("sides"), a.f("radius"), a.f("top"), a.f("height"));
    let (a1, a2) = (ngon_area(n, r), ngon_area(n, t));
    // V = h (A1 + A2 + √(A1 A2)) / 3; the centroid's height from the cone's: h (A1 + 2√(A1A2) + 3A2) / (4 (A1 + √(A1A2) + A2)).
    let g = (a1 * a2).sqrt();
    Measures { size: Some(h * (a1 + a2 + g) / 3.0), centroid: Some([0.0, h * (a1 + 2.0 * g + 3.0 * a2) / (4.0 * (a1 + g + a2)), 0.0]), ..Measures::default() }
}
fn frustum_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary)]
}

fn bipyramid_poly(a: &Args) -> Poly {
    let (n, r, h) = (a.n("sides"), a.f("radius"), a.f("height"));
    let mut pts: Vec<P3> = (0..n).map(|i| ring_pt(n, i, r, 0.5 * h, 0.0)).collect();
    pts.push([0.0, 0.0, 0.0]);
    pts.push([0.0, h, 0.0]);
    let n32 = n as u32;
    let mut faces = Vec::new();
    for i in 0..n32 {
        let j = (i + 1) % n32;
        faces.push(vec![i, j, n32 + 1]);
        faces.push(vec![j, i, n32]);
    }
    let surf = vec![0; faces.len()];
    Poly { pts, faces, surf, names: vec!["triangles".into()] }
}
fn bipyramid_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(bipyramid_poly(a))))
}
fn bipyramid_closed(a: &Args) -> Measures {
    let (n, r, h) = (a.f("sides"), a.f("radius"), a.f("height"));
    Measures { size: Some(ngon_area(n, r) * h / 3.0), centroid: Some([0.0, 0.5 * h, 0.0]), ..Measures::default() }
}
fn bipyramid_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    vec![at3("centre", [0.0, 0.5 * h, 0.0], On::Inside), at3("base", [0.0, 0.0, 0.0], On::Vertex), at3("top", [0.0, h, 0.0], On::Vertex)]
}

/// The trapezohedron of `n` kites a side: apexes at `0` and `h`; a zigzag of `2n` points at
/// radius `r`, alternately `z` above and below the middle. Each kite (an apex and three zigzag
/// points) is planar exactly when `H = z (1 + c) / (1 − c)`, `c = cos(π / n)` and `H = h / 2`
/// (by symmetry the kite's plane has its normal `(a cos(π/n), a sin(π/n), b)` with z up; through
/// the apex `(0, 0, H)` it meets the outer point `(r, 0, z)` when `a r cos(π/n) = b (H − z)` and
/// the middle point `(r cos(π/n), r sin(π/n), −z)` when `a r = b (H + z)`; so `cos(π/n) =
/// (H − z) / (H + z)`), so `z = H (1 − c) / (1 + c)`.
fn trap_poly(a: &Args) -> Poly {
    let (n, r, h) = (a.n("sides"), a.f("radius"), a.f("height"));
    let big = 0.5 * h;
    let c = cos(PI / n as f64);
    let z = big * (1.0 - c) / (1.0 + c);
    let mut pts: Vec<P3> = Vec::new();
    for j in 0..n {
        pts.push(ring_pt(n, j, r, big + z, 0.0));
        pts.push(ring_pt(n, j, r, big - z, PI / n as f64));
    }
    pts.push([0.0, 0.0, 0.0]);
    pts.push([0.0, h, 0.0]);
    let n32 = n as u32;
    let (u, l) = (|j: u32| 2 * (j % n32), |j: u32| 2 * (j % n32) + 1);
    let (bottom, top) = (2 * n32, 2 * n32 + 1);
    let mut faces = Vec::new();
    for j in 0..n32 {
        faces.push(vec![u(j), l(j), u(j + 1), top]);
        faces.push(vec![l(j + n32 - 1), bottom, l(j), u(j)]);
    }
    let surf = vec![0; faces.len()];
    Poly { pts, faces, surf, names: vec!["kites".into()] }
}
fn trap_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(trap_poly(a))))
}
fn trap_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    vec![at3("centre", [0.0, 0.5 * h, 0.0], On::Inside), at3("base", [0.0, 0.0, 0.0], On::Vertex), at3("top", [0.0, h, 0.0], On::Vertex)]
}
fn trap_closed(a: &Args) -> Measures {
    Measures { centroid: Some([0.0, 0.5 * a.f("height"), 0.0]), ..Measures::default() }
}

// ---------------------------------------------------------------- stars in space

/// The stella octangula's outer surface: an octahedron with a regular tetrahedron raised on
/// each face. Its tips are a cube's corners `(±s, ±s, ±s)`, its hidden points the octahedron's
/// `(±s, 0, 0)`..., and its edges (the two tetrahedra's) `2√2 s`.
fn stella_poly(a: &Args) -> Poly {
    let s = a.f("edge") / (2.0 * s2());
    let mut pts: Vec<P3> = vec![[s, 0.0, 0.0], [-s, 0.0, 0.0], [0.0, s, 0.0], [0.0, -s, 0.0], [0.0, 0.0, s], [0.0, 0.0, -s]];
    let mut faces = Vec::new();
    for m in 0..8u32 {
        let sg = [0, 1, 2].map(|i| if m & (1 << i) != 0 { -1.0 } else { 1.0 });
        let tip = pts.len() as u32;
        pts.push([sg[0] * s, sg[1] * s, sg[2] * s]);
        let (x, y, z) = (if sg[0] > 0.0 { 0 } else { 1 }, if sg[1] > 0.0 { 2 } else { 3 }, if sg[2] > 0.0 { 4 } else { 5 });
        let tri = if sg[0] * sg[1] * sg[2] > 0.0 { [x, y, z] } else { [x, z, y] };
        for k in 0..3 {
            faces.push(vec![tri[k], tri[(k + 1) % 3], tip]);
        }
    }
    let surf = vec![0; faces.len()];
    Poly { pts, faces, surf, names: vec!["triangles".into()] }.placed(1.0)
}
fn stella_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(stella_poly(a))))
}
fn stella_closed(a: &Args) -> Measures {
    // The octahedron of edge e/2, √2/3 (e/2)³, and eight regular tetrahedra of edge e/2, each
    // (e/2)³ / (6√2): in all (e/2)³ √2; the area is 24 equilateral triangles of side e/2.
    let e = a.f("edge");
    let h = 0.5 * e;
    Measures { size: Some(s2() / 3.0 * h * h * h + 8.0 * h * h * h / (6.0 * s2())), boundary: Some(24.0 * 3.0f64.sqrt() / 4.0 * h * h), ..Measures::default() }
}

/// The small stellated dodecahedron's outer surface: a dodecahedron of edge `a` with a
/// pentagonal pyramid on each face whose sides are golden triangles (sides `φ a`, the
/// pentagram's): its apex `√((φ a)² − ρ²)` above the face's middle, `ρ = a / (2 sin 36°)` the
/// pentagon's circumradius.
fn ssd_poly(a: &Args) -> Poly {
    let e = a.f("edge");
    let core = units().0[3].map(|q| scale3(q, e));
    let mesh = core.mesh();
    let rho = e / (2.0 * sin(PI / 5.0));
    let h = ((phi() * e) * (phi() * e) - rho * rho).sqrt();
    let mut pts = core.pts.clone();
    let mut faces = Vec::new();
    for f in &mesh.faces {
        let c = scale3(f.verts.iter().fold([0.0; 3], |s, &v| add3(s, core.pts[v as usize])), 1.0 / f.verts.len() as f64);
        let n = norm3(mesh.area_vector(f));
        let tip = pts.len() as u32;
        pts.push(add3(c, scale3(n, h)));
        for k in 0..f.verts.len() {
            faces.push(vec![f.verts[k], f.verts[(k + 1) % f.verts.len()], tip]);
        }
    }
    let surf = vec![0; faces.len()];
    Poly { pts, faces, surf, names: vec!["triangles".into()] }.placed(1.0)
}
fn ssd_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(ssd_poly(a))))
}
/// A star's anchors: as a polyhedron's, but its base and top are points of reference (`Free`):
/// a star rests on several tips, and the middle of them is in the air.
fn star_anchors(p: Poly) -> Vec<Anchor> {
    let mut v = poly_anchors(&p);
    for a in v.iter_mut().filter(|a| a.name == "base" || a.name == "top") {
        a.on = On::Free;
    }
    v
}
fn stella_anchors(a: &Args) -> Vec<Anchor> {
    star_anchors(stella_poly(a))
}
fn ssd_anchors(a: &Args) -> Vec<Anchor> {
    star_anchors(ssd_poly(a))
}

// ---------------------------------------------------------------- the table

/// The plane kin of a regular solid by the kinds of its faces.
const K_T: &[&str] = &["triangle_sss"];
const K_S: &[&str] = &["rectangle"];
const K_P: &[&str] = &["regular_polygon"];
const K_TS: &[&str] = &["triangle_sss", "rectangle"];
const K_TP: &[&str] = &["triangle_sss", "regular_polygon"];
const K_SP: &[&str] = &["rectangle", "regular_polygon"];
const K_TSP: &[&str] = &["triangle_sss", "rectangle", "regular_polygon"];

macro_rules! reg {
    ($i:expr, $fam:ident, $about:expr, $params:expr, $kin:expr) => {{
        const F: (fn(&Args) -> Result<Geom, Refusal>, fn(&Args) -> Measures, fn(&Args) -> Vec<Anchor>) = regular_form!($i);
        solid!(REGULAR[$i].name, $fam, $about, "the middle of its lowest face, edge or point, on y = 0", $params, curved: false, kin: $kin, F.0, F.1, F.2)
    }};
}
macro_rules! reg_r {
    ($name:expr, $i:expr, $about:expr, $kin:expr) => {{
        const F: (fn(&Args) -> Result<Geom, Refusal>, fn(&Args) -> Measures, fn(&Args) -> Vec<Anchor>) = regular_form!($i);
        solid!($name, Platonic, $about, "the middle of its lowest face, edge or point, on y = 0", P_RADIUS, curved: false, kin: $kin, F.0, F.1, F.2)
    }};
}
macro_rules! cat {
    ($i:expr, $about:expr, $kin:expr) => {{
        const F: (fn(&Args) -> Result<Geom, Refusal>, fn(&Args) -> Vec<Anchor>) = catalan_form!($i);
        solid!(CATALAN[$i].0, Catalan, $about, "the middle of its lowest face, edge or point, on y = 0", P_INRADIUS, curved: false, kin: $kin, F.0, super::none, F.1)
    }};
}

const STANDS: &str = "the middle of its base, on y = 0";

pub(crate) const FORMS: &[Form] = &[
    solid!("box", Block, "a rectangular box (a cuboid)", STANDS, P_BOX, curved: false, kin: &["rectangle"], box_geom, box_closed, box_anchors),
    solid!("wedge", Block, "a ramp: a box cut along a diagonal, rising to the high end at +x", STANDS, P_BOX, curved: false, kin: &["rectangle", "triangle"], wedge_geom, wedge_closed, wedge_anchors),
    reg!(0, Platonic, "the regular tetrahedron, by its edge", P_EDGE, &["triangle_sss", "triangle_sas", "triangle"]),
    reg!(2, Platonic, "the regular octahedron, by its edge", P_EDGE, K_T),
    reg!(3, Platonic, "the regular dodecahedron, by its edge", P_EDGE, K_P),
    reg!(4, Platonic, "the regular icosahedron, by its edge", P_EDGE, K_T),
    reg_r!("tetrahedron_in_sphere", 0, "the regular tetrahedron, by the radius of the sphere through its points", K_T),
    reg_r!("cube_in_sphere", 1, "the cube, by the radius of the sphere through its corners", K_S),
    reg_r!("octahedron_in_sphere", 2, "the regular octahedron, by the radius of the sphere through its points", K_T),
    reg_r!("dodecahedron_in_sphere", 3, "the regular dodecahedron, by the radius of the sphere through its points", K_P),
    reg_r!("icosahedron_in_sphere", 4, "the regular icosahedron, by the radius of the sphere through its points", K_T),
    reg!(5, Archimedean, "the truncated tetrahedron: 4 triangles, 4 hexagons", P_EDGE, K_TP),
    reg!(6, Archimedean, "the cuboctahedron: 8 triangles, 6 squares", P_EDGE, K_TS),
    reg!(7, Archimedean, "the truncated cube: 8 triangles, 6 octagons", P_EDGE, K_TP),
    reg!(8, Archimedean, "the truncated octahedron: 6 squares, 8 hexagons", P_EDGE, K_SP),
    reg!(9, Archimedean, "the (small) rhombicuboctahedron: 8 triangles, 18 squares", P_EDGE, K_TS),
    reg!(10, Archimedean, "the truncated cuboctahedron (great rhombicuboctahedron): 12 squares, 8 hexagons, 6 octagons", P_EDGE, K_SP),
    reg!(11, Archimedean, "the snub cube: 32 triangles, 6 squares (this hand of the two)", P_EDGE, K_TS),
    reg!(12, Archimedean, "the icosidodecahedron: 20 triangles, 12 pentagons", P_EDGE, K_TP),
    reg!(13, Archimedean, "the truncated dodecahedron: 20 triangles, 12 decagons", P_EDGE, K_TP),
    reg!(14, Archimedean, "the truncated icosahedron (a football's pattern): 12 pentagons, 20 hexagons", P_EDGE, K_P),
    reg!(15, Archimedean, "the (small) rhombicosidodecahedron: 20 triangles, 30 squares, 12 pentagons", P_EDGE, K_TSP),
    reg!(16, Archimedean, "the truncated icosidodecahedron (great rhombicosidodecahedron): 30 squares, 20 hexagons, 12 decagons", P_EDGE, K_SP),
    reg!(17, Archimedean, "the snub dodecahedron: 80 triangles, 12 pentagons (this hand of the two)", P_EDGE, K_TP),
    cat!(0, "the triakis tetrahedron: 12 isosceles triangles (the truncated tetrahedron's dual)", &["triangle"]),
    cat!(1, "the rhombic dodecahedron: 12 rhombi (the cuboctahedron's dual)", &["rhombus"]),
    cat!(2, "the triakis octahedron: 24 isosceles triangles (the truncated cube's dual)", &["triangle"]),
    cat!(3, "the tetrakis hexahedron: 24 isosceles triangles (the truncated octahedron's dual)", &["triangle"]),
    cat!(4, "the deltoidal icositetrahedron: 24 kites (the rhombicuboctahedron's dual)", &["kite"]),
    cat!(5, "the disdyakis dodecahedron: 48 scalene triangles (the truncated cuboctahedron's dual)", &["triangle_sss"]),
    cat!(6, "the pentagonal icositetrahedron: 24 irregular pentagons (the snub cube's dual)", K_P),
    cat!(7, "the rhombic triacontahedron: 30 golden rhombi (the icosidodecahedron's dual)", &["rhombus"]),
    cat!(8, "the triakis icosahedron: 60 isosceles triangles (the truncated dodecahedron's dual)", &["triangle"]),
    cat!(9, "the pentakis dodecahedron: 60 isosceles triangles (the truncated icosahedron's dual)", &["triangle"]),
    cat!(10, "the deltoidal hexecontahedron: 60 kites (the rhombicosidodecahedron's dual)", &["kite"]),
    cat!(11, "the disdyakis triacontahedron: 120 scalene triangles (the truncated icosidodecahedron's dual)", &["triangle_sss"]),
    cat!(12, "the pentagonal hexecontahedron: 60 irregular pentagons (the snub dodecahedron's dual)", K_P),
    solid!("prism", Prismatoid, "a right prism on a regular polygon (a vertex on −z)", STANDS, P_PRISM, curved: false, kin: KIN_PRISM, prism_geom, prism_closed, prism_anchors),
    solid!("antiprism", Prismatoid, "a regular antiprism: two regular polygons, the top turned half a side, joined by triangles", STANDS, P_PRISM, curved: false, kin: &["regular_polygon", "triangle_sss"], antiprism_geom, antiprism_closed, antiprism_anchors),
    solid!("pyramid", Prismatoid, "a right pyramid on a regular polygon", STANDS, P_PRISM, curved: false, kin: &["regular_polygon", "triangle_sss", "triangle_sas", "triangle"], pyramid_geom, pyramid_closed, pyramid_anchors),
    solid!("pyramid_frustum", Prismatoid, "a right pyramid's frustum: a regular polygon below, a smaller one above", STANDS, P_FRUSTUM, curved: false, kin: &["regular_polygon", "trapezium"], frustum_geom, frustum_closed, frustum_anchors),
    solid!("bipyramid", Prismatoid, "two right pyramids base to base (a dipyramid), standing on a tip", "the lower tip, on y = 0", P_PRISM, curved: false, kin: &["regular_polygon", "triangle"], bipyramid_geom, bipyramid_closed, bipyramid_anchors),
    solid!("trapezohedron", Prismatoid, "a trapezohedron: 2n kites about two tips (the antiprism's dual; d10 at five), standing on a tip", "the lower tip, on y = 0", P_PRISM, curved: false, kin: &["kite"], trap_geom, trap_closed, trap_anchors),
    solid!("star_prism", Prismatoid, "a prism on a star of n points", STANDS, P_STARPRISM, curved: false, kin: &["star", "star_polygon"], starprism_geom, starprism_closed, starprism_anchors),
    solid!("stella_octangula", Stellated, "the stella octangula's outer surface: a regular tetrahedron raised on each face of an octahedron (two tetrahedra through each other)", "the middle of its lowest point, on y = 0", P_EDGE, curved: false, kin: &["triangle_sss", "star_polygon"], stella_geom, stella_closed, stella_anchors),
    solid!("small_stellated_dodecahedron", Stellated, "the small stellated dodecahedron's outer surface: a pentagonal pyramid of golden triangles on each face of a dodecahedron of this edge", "the middle of its lowest points, on y = 0", P_EDGE, curved: false, kin: &["star_polygon", "triangle"], ssd_geom, super::none, ssd_anchors),
];

/// The plane forms a prism of their outline is kin to (the prism here is the regular one; any
/// one-ring figure is raised by the operator in `ops`).
const KIN_PRISM: &[&str] = &[
    "regular_polygon", "triangle_sss", "triangle_sas", "triangle", "parallelogram", "rhombus", "kite", "dart", "trapezium", "cross", "saltire", "arrow", "double_arrow", "chevron", "shield",
    "l_section", "t_section", "u_section", "h_section", "z_section", "gear", "sawtooth_band", "battlement_band",
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("cube", Solid, "box", "a cube", &[Set::Tie { name: "size", about: "each edge", targets: &["width", "height", "depth"] }]),
    alias!("hexahedron", Solid, "box", "the regular hexahedron (a cube)", &[Set::Tie { name: "edge", about: "each edge", targets: &["width", "height", "depth"] }]),
    alias!("d6", Solid, "box", "a six-sided die (a cube)", &[Set::Tie { name: "size", about: "each edge", targets: &["width", "height", "depth"] }]),
    alias!("cuboid", Solid, "box", "a box", &[]),
    alias!("rectangular_prism", Solid, "box", "a box", &[]),
    alias!("brick", Solid, "box", "a box", &[]),
    alias!("plank", Solid, "box", "a long thin box", &[]),
    alias!("ramp", Solid, "wedge", "a ramp", &[]),
    alias!("d4", Solid, "tetrahedron", "a four-sided die", &[]),
    alias!("d8", Solid, "octahedron", "an eight-sided die", &[]),
    alias!("d12", Solid, "dodecahedron", "a twelve-sided die", &[]),
    alias!("d20", Solid, "icosahedron", "a twenty-sided die", &[]),
    alias!("d10", Solid, "trapezohedron", "a ten-sided die: the pentagonal trapezohedron", &[Set::Fixed("sides", 5.0)]),
    alias!("great_rhombicuboctahedron", Solid, "truncated_cuboctahedron", "the truncated cuboctahedron", &[]),
    alias!("small_rhombicuboctahedron", Solid, "rhombicuboctahedron", "the rhombicuboctahedron", &[]),
    alias!("great_rhombicosidodecahedron", Solid, "truncated_icosidodecahedron", "the truncated icosidodecahedron", &[]),
    alias!("small_rhombicosidodecahedron", Solid, "rhombicosidodecahedron", "the rhombicosidodecahedron", &[]),
    alias!("buckyball", Solid, "truncated_icosahedron", "the truncated icosahedron", &[]),
    alias!("football", Solid, "truncated_icosahedron", "the truncated icosahedron (British: a football's pattern)", &[]),
    alias!("soccer_ball", Solid, "truncated_icosahedron", "the truncated icosahedron (American: a soccer ball's pattern)", &[]),
    alias!("triangular_prism", Solid, "prism", "a prism on an equilateral triangle", &[Set::Fixed("sides", 3.0)]),
    alias!("pentagonal_prism", Solid, "prism", "a prism on a regular pentagon", &[Set::Fixed("sides", 5.0)]),
    alias!("hexagonal_prism", Solid, "prism", "a prism on a regular hexagon", &[Set::Fixed("sides", 6.0)]),
    alias!("octagonal_prism", Solid, "prism", "a prism on a regular octagon", &[Set::Fixed("sides", 8.0)]),
    alias!("square_antiprism", Solid, "antiprism", "the square antiprism", &[Set::Fixed("sides", 4.0)]),
    alias!("square_pyramid", Solid, "pyramid", "a pyramid on a square", &[Set::Fixed("sides", 4.0)]),
    alias!("triangular_pyramid", Solid, "pyramid", "a pyramid on an equilateral triangle", &[Set::Fixed("sides", 3.0)]),
    alias!("pentagonal_pyramid", Solid, "pyramid", "a pyramid on a regular pentagon", &[Set::Fixed("sides", 5.0)]),
    alias!("hexagonal_pyramid", Solid, "pyramid", "a pyramid on a regular hexagon", &[Set::Fixed("sides", 6.0)]),
    alias!("truncated_pyramid", Solid, "pyramid_frustum", "a pyramid's frustum", &[]),
    alias!("dipyramid", Solid, "bipyramid", "a bipyramid", &[]),
    alias!("triangular_bipyramid", Solid, "bipyramid", "the triangular bipyramid", &[Set::Fixed("sides", 3.0)]),
    alias!("pentagonal_bipyramid", Solid, "bipyramid", "the pentagonal bipyramid", &[Set::Fixed("sides", 5.0)]),
    alias!("deltohedron", Solid, "trapezohedron", "a trapezohedron", &[]),
    alias!("stellated_octahedron", Solid, "stella_octangula", "the stella octangula", &[]),
];
