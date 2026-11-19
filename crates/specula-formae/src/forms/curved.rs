//! Curved solids: spheres and their parts, cylinders, cones, tubes and capsules, tori, other
//! solids of revolution, rounded blocks, shells, tubes swept along curves, and two compounds.
//!
//! Every solid stands on `y = 0` with its axis (or its middle) on the Y axis, as the modeller's
//! solids do. Most are solids of revolution ([`Rev`]): a profile of lines, arcs and parametric
//! pieces turned about Y, so their exact measures come from the profile (see
//! `solid::profile_integrals`) wherever the profile is lines and arcs. **Detail**: a full turn is
//! `detail` segments; a profile's arc is cut as a plane figure's is (`max(2, ceil(detail × its
//! turn over 2π))`), so a sphere of latitudes and longitudes has `detail` round and `detail / 2`
//! from pole to pole; an icosphere's edges are cut into `f` (its frequency), the segments its
//! edge's angle (63.43 degrees) is worth; a cube-sphere's and a superellipsoid's cube faces into
//! `g × g`, `g` the segments of a quarter turn; a rounded box's rounded edges into the segments of
//! a quarter turn; a swept wire is `detail` round and the segments of its curve's turns along (a
//! helix one turn a coil, a (p, q) knot `p + q`).

use crate::anchor::{at3, out3, Anchor, On};
use crate::detail::segments;
use crate::error::{refuse, Refusal};
use crate::forms::round::drop_parts;
use crate::geom::{arc, func, line, Geom};
use crate::maths::*;
use crate::mesh::{Mesh, SolidFault, Surface};
use crate::solid::{fpiece, gauss, middle_error, piece, sphere_error, tube_check, tube_mesh, Gen, Lattice, Piece, Rev};
use crate::spec::Args;
use crate::table::{count, deg, len, ratio, Alias, Form, Param, Set};
use crate::Measures;
use std::rc::Rc;

const O: Pt = [0.0, 0.0];

/// A solid of revolution's geometry: the profile turned through `sweep` radians.
fn rev(pieces: Vec<Piece>, closed: bool, sweep: f64, scale: [f64; 2]) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(Rev { pieces, closed, sweep, scale, cut: "cut" })))
}
/// A full turn of an open profile (from the axis to the axis).
fn turned(pieces: Vec<Piece>) -> Result<Geom, Refusal> {
    rev(pieces, false, TAU, [1.0, 1.0])
}

fn m(size: Option<f64>, boundary: Option<f64>, cy: f64, lo: P3, hi: P3) -> Measures {
    Measures { size, boundary, centroid: Some([0.0, cy, 0.0]), bounds: Some((lo, hi)), series: None }
}
/// The bounds of a round solid of radius `r` from `y0` to `y1`.
fn round_bounds(r: f64, y0: f64, y1: f64) -> (P3, P3) {
    ([-r, y0, -r], [r, y1, r])
}

/// The anchors of a round solid on the axis: its base and top on the axis (vertices where a
/// pole is built, otherwise points of a lid), its centre, and four points round it at the height
/// `y` and radius `r` (on the surface, within the build's error).
fn axial(base: (f64, On), top: (f64, On), y: f64, r: f64, centre: Option<f64>) -> Vec<Anchor> {
    let mut v = vec![out3("base", [0.0, base.0, 0.0], [0.0, -1.0, 0.0], base.1), out3("top", [0.0, top.0, 0.0], [0.0, 1.0, 0.0], top.1)];
    if let Some(c) = centre {
        v.push(at3("centre", [0.0, c, 0.0], On::Inside));
    }
    for (name, d) in [("right", [1.0, 0.0, 0.0]), ("left", [-1.0, 0.0, 0.0]), ("front", [0.0, 0.0, 1.0]), ("back", [0.0, 0.0, -1.0])] {
        v.push(out3(name, [r * d[0], y, r * d[2]], d, On::Boundary));
    }
    v
}

// ---------------------------------------------------------------- spheres

const P_SPHERE: &[Param] = &[len("radius", "the radius", 0.5)];
fn sphere_pieces(r: f64) -> Vec<Piece> {
    vec![piece(arc([0.0, r], r, -0.5 * PI, 0.5 * PI), "sphere")]
}
fn sphere_geom(a: &Args) -> Result<Geom, Refusal> {
    turned(sphere_pieces(a.f("radius")))
}
fn sphere_closed(a: &Args) -> Measures {
    let r = a.f("radius");
    let (lo, hi) = round_bounds(r, 0.0, 2.0 * r);
    m(Some(4.0 / 3.0 * PI * r * r * r), Some(4.0 * PI * r * r), r, lo, hi)
}
fn sphere_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    axial((0.0, On::Vertex), (2.0 * r, On::Vertex), r, r, Some(r))
}
/// The anchors of a sphere built without poles.
fn ball_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    axial((0.0, On::Boundary), (2.0 * r, On::Boundary), r, r, Some(r))
}

/// An icosphere's frequency at `detail`: the segments an icosahedron's edge's angle
/// (`acos(1/√5)`, 63.43 degrees) is worth.
fn ico_frequency(detail: u32) -> u64 {
    segments(acos(1.0 / 5.0f64.sqrt()) / TAU, detail)
}
/// The icosphere: each face of the icosahedron cut into `f²` triangles, every point pushed out
/// to the sphere. Its points are spread nearly evenly (the triangles near the icosahedron's
/// points are a little smaller than those in its faces' middles).
fn icosphere_mesh(r: f64, f: u32) -> Mesh {
    let ico = &crate::forms::flat::units().0[4];
    let (base, tris) = (&ico.pts, &ico.faces);
    let mut points: Vec<P3> = base.iter().map(|&p| norm3(p)).collect();
    // The points inside each edge, from its lower point to its higher.
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.push((a.min(b), a.max(b)));
        }
    }
    edges.sort_unstable();
    edges.dedup();
    let mut edge_start = Vec::new();
    for &(a, b) in &edges {
        edge_start.push(points.len() as u32);
        for k in 1..f {
            let t = k as f64 / f as f64;
            points.push(norm3(add3(base[a as usize], scale3(sub3(base[b as usize], base[a as usize]), t))));
        }
    }
    let on_edge = |a: u32, b: u32, k: u32| -> u32 {
        if k == 0 {
            return a;
        }
        if k == f {
            return b;
        }
        let e = edges.binary_search(&(a.min(b), a.max(b))).expect("an edge");
        let kk = if a < b { k } else { f - k };
        edge_start[e] + kk - 1
    };
    let mut faces = Vec::new();
    for t in tris {
        let (a, b, c) = (t[0], t[1], t[2]);
        let (pa, pb, pc) = (base[a as usize], base[b as usize], base[c as usize]);
        let start = points.len() as u32;
        // The points inside the face, (i, j) with i, j >= 1 and i + j <= f − 1, row by row.
        let mut inside = Vec::new();
        for i in 1..f {
            for j in 1..f - i {
                inside.push((i, j));
                points.push(norm3(add3(add3(pa, scale3(sub3(pb, pa), i as f64 / f as f64)), scale3(sub3(pc, pa), j as f64 / f as f64))));
            }
        }
        let id = |i: u32, j: u32| -> u32 {
            if j == 0 {
                on_edge(a, b, i)
            } else if i == 0 {
                on_edge(a, c, j)
            } else if i + j == f {
                on_edge(b, c, j)
            } else {
                start + inside.iter().position(|&x| x == (i, j)).expect("an inside point") as u32
            }
        };
        for i in 0..f {
            for j in 0..f - i {
                faces.push(vec![id(i, j), id(i + 1, j), id(i, j + 1)]);
                if i + j + 2 <= f {
                    faces.push(vec![id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
                }
            }
        }
    }
    let normals = points.clone();
    let points: Vec<P3> = points.iter().map(|p| [p[0] * r, p[1] * r + r, p[2] * r]).collect();
    let faces = faces.into_iter().map(|v: Vec<u32>| crate::mesh::Face { normals: Some(v.iter().map(|&i| normals[i as usize]).collect()), verts: v, surface: 0 }).collect();
    let mut mesh = Mesh { points, faces, surfaces: vec![Surface { name: "sphere".into(), smooth: true, radius: Some(r) }], chord_error: 0.0 };
    mesh.chord_error = sphere_error(&mesh, [0.0, r, 0.0], r, 0);
    mesh
}
fn icosphere_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(|d| {
            let f = ico_frequency(d);
            (10 * f * f + 2, 20 * f * f)
        }),
        build: Rc::new(move |d| (icosphere_mesh(r, ico_frequency(d) as u32), None)),
        exact: sphere_closed(a),
        error: Rc::new(|_| None),
        genus: 0,
        check: None,
    })))
}

/// The equiangular cube-sphere: each face of a cube cut into `g × g` by lines at equal angles
/// from the centre (`tan` of evenly spaced angles), every point pushed out to the sphere; each
/// cell two triangles. Its points are spread more evenly than a plain cube grid's.
fn cube_sphere_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    let lattice = move |d: u32| {
        let g = segments(0.25, d) as u32;
        Lattice {
            n: [g + 1; 3],
            point: move |q: [u32; 3]| {
                let c = q.map(|i| tan(-0.25 * PI + 0.5 * PI * i as f64 / g as f64));
                let u = norm3(c);
                ([u[0] * r, u[1] * r + r, u[2] * r], Some(u))
            },
            cell: |_, _, _| (true, 0u32),
            surfaces: vec![Surface { name: "sphere".into(), smooth: true, radius: Some(r) }],
        }
    };
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(move |d| {
            let l = lattice(d);
            (l.points(), l.faces())
        }),
        build: Rc::new(move |d| {
            let mut mesh = lattice(d).mesh();
            mesh.chord_error = sphere_error(&mesh, [0.0, r, 0.0], r, 0);
            (mesh, None)
        }),
        exact: sphere_closed(a),
        error: Rc::new(|_| None),
        genus: 0,
        check: None,
    })))
}

const P_ELLIPSOID: &[Param] = &[len("rx", "the semi-axis along x", 0.6), len("ry", "the semi-axis along y", 0.4), len("rz", "the semi-axis along z", 0.5)];
fn ellipsoid_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, ry, rz) = (a.f("rx"), a.f("ry"), a.f("rz"));
    rev(sphere_pieces(ry), false, TAU, [rx / ry, rz / ry])
}
fn ellipsoid_closed(a: &Args) -> Measures {
    let (rx, ry, rz) = (a.f("rx"), a.f("ry"), a.f("rz"));
    m(Some(4.0 / 3.0 * PI * rx * ry * rz), None, ry, [-rx, 0.0, -rz], [rx, 2.0 * ry, rz])
}
fn ellipsoid_anchors(a: &Args) -> Vec<Anchor> {
    let (rx, ry, rz) = (a.f("rx"), a.f("ry"), a.f("rz"));
    vec![
        out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Vertex),
        out3("top", [0.0, 2.0 * ry, 0.0], [0.0, 1.0, 0.0], On::Vertex),
        at3("centre", [0.0, ry, 0.0], On::Inside),
        out3("right", [rx, ry, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("front", [0.0, ry, rz], [0.0, 0.0, 1.0], On::Boundary),
    ]
}

fn hemisphere_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    turned(vec![piece(line(O, [r, 0.0]), "base"), piece(arc(O, r, 0.0, 0.5 * PI), "dome")])
}
fn hemisphere_closed(a: &Args) -> Measures {
    let r = a.f("radius");
    let (lo, hi) = round_bounds(r, 0.0, r);
    m(Some(2.0 / 3.0 * PI * r * r * r), Some(3.0 * PI * r * r), 3.0 * r / 8.0, lo, hi)
}
fn hemisphere_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    axial((0.0, On::Boundary), (r, On::Vertex), 0.5 * r, r * 0.75f64.sqrt(), Some(3.0 * r / 8.0))
}

const P_CAP: &[Param] = &[len("radius", "the sphere's radius", 0.5), len("height", "the cap's height, under the sphere's diameter", 0.25)];
fn cap_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, h) = (a.f("radius"), a.f("height"));
    if h >= 2.0 * r {
        return Err(refuse("cap_under_diameter", "the cap's height must be under the sphere's diameter", &[("radius", r), ("height", h)]));
    }
    Ok((r, h, (h * (2.0 * r - h)).sqrt()))
}
fn cap_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, h, base) = cap_parts(a)?;
    let cy = h - r;
    turned(vec![piece(line(O, [base, 0.0]), "base"), piece(arc([0.0, cy], r, atan2(-cy, base), 0.5 * PI), "cap")])
}
fn cap_closed(a: &Args) -> Measures {
    let Ok((r, h, base)) = cap_parts(a) else { return Measures::default() };
    let wide = if h >= r { r } else { base };
    let (lo, hi) = round_bounds(wide, 0.0, h);
    m(Some(PI * h * h * (3.0 * r - h) / 3.0), Some(2.0 * PI * r * h + PI * base * base), (h - r) + 3.0 * (2.0 * r - h) * (2.0 * r - h) / (4.0 * (3.0 * r - h)), lo, hi)
}
fn cap_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Vertex)]
}

const P_ZONE: &[Param] = &[len("radius", "the sphere's radius", 0.5), ratio("from", "the lower cut's height from the centre, over the radius", -0.5, -0.999, 0.999), ratio("to", "the upper cut's height from the centre, over the radius", 0.5, -0.999, 0.999)];
fn zone_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, f, t) = (a.f("radius"), a.f("from"), a.f("to"));
    if f >= t {
        return Err(refuse("from_under_to", "the lower cut must be under the upper one", &[("from", f), ("to", t)]));
    }
    Ok((r, f, t))
}
fn zone_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, f, t) = zone_parts(a)?;
    let (y0, y1) = (f * r, t * r);
    let (a0, a1) = (r * ((1.0 - f) * (1.0 + f)).sqrt(), r * ((1.0 - t) * (1.0 + t)).sqrt());
    turned(vec![piece(line(O, [a0, 0.0]), "bottom"), piece(arc([0.0, -y0], r, asin(f), asin(t)), "zone"), piece(line([a1, y1 - y0], [0.0, y1 - y0]), "top")])
}
fn zone_closed(a: &Args) -> Measures {
    let Ok((r, f, t)) = zone_parts(a) else { return Measures::default() };
    let (y0, y1) = (f * r, t * r);
    let h = y1 - y0;
    // V = π ∫ (r² − y²) dy = π h (r² − (y1² + y1 y0 + y0²) / 3), with no difference of cubes;
    // the lids' areas π r² (1 − f²) as products.
    let vol = PI * h * (r * r - (y1 * y1 + y1 * y0 + y0 * y0) / 3.0);
    let area = 2.0 * PI * r * h + PI * r * r * ((1.0 - f) * (1.0 + f) + (1.0 - t) * (1.0 + t));
    let wide = if f <= 0.0 && t >= 0.0 { r } else { r * ((1.0 - f * f).max(1.0 - t * t)).sqrt() };
    Measures { size: Some(vol), boundary: Some(area), bounds: Some(round_bounds(wide, 0.0, h)), ..Measures::default() }
}
fn zone_anchors(a: &Args) -> Vec<Anchor> {
    let h = (a.f("to") - a.f("from")) * a.f("radius");
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary)]
}

const P_WEDGE: &[Param] = &[len("radius", "the sphere's radius", 0.5), deg("angle", "the wedge's angle about Y, from +x", 90.0, 1.0, 359.0)];
fn swedge_geom(a: &Args) -> Result<Geom, Refusal> {
    rev(sphere_pieces(a.f("radius")), false, rad(a.f("angle")), [1.0, 1.0])
}
fn swedge_closed(a: &Args) -> Measures {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    Measures { size: Some(2.0 / 3.0 * t * r * r * r), boundary: Some(2.0 * t * r * r + PI * r * r), ..Measures::default() }
}
fn pole_anchors(a: &Args) -> Vec<Anchor> {
    // The poles as points (on the axis, an edge of the wedge), and the middle of its round side.
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    let (s, c) = sin_cos(0.5 * t);
    vec![at3("base", [0.0, 0.0, 0.0], On::Vertex), at3("top", [0.0, 2.0 * r, 0.0], On::Vertex), out3("side", [r * c, r, -r * s], [c, 0.0, -s], On::Boundary)]
}

// ---------------------------------------------------------------- cylinders, cones, tubes

const P_CYL: &[Param] = &[len("radius", "the radius", 0.5), len("height", "the height", 1.0)];
fn cyl_pieces(r: f64, h: f64) -> Vec<Piece> {
    vec![piece(line(O, [r, 0.0]), "bottom"), piece(line([r, 0.0], [r, h]), "side"), piece(line([r, h], [0.0, h]), "top")]
}
fn cyl_geom(a: &Args) -> Result<Geom, Refusal> {
    turned(cyl_pieces(a.f("radius"), a.f("height")))
}
fn cyl_closed(a: &Args) -> Measures {
    let (r, h) = (a.f("radius"), a.f("height"));
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(PI * r * r * h), Some(2.0 * PI * r * (r + h)), 0.5 * h, lo, hi)
}
fn cyl_anchors(a: &Args) -> Vec<Anchor> {
    let (r, h) = (a.f("radius"), a.f("height"));
    axial((0.0, On::Boundary), (h, On::Boundary), 0.5 * h, r, Some(0.5 * h))
}

const P_ECYL: &[Param] = &[len("rx", "the semi-axis along x", 0.6), len("rz", "the semi-axis along z", 0.4), len("height", "the height", 1.0)];
fn ecyl_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, rz) = (a.f("rx"), a.f("rz"));
    rev(cyl_pieces(rx, a.f("height")), false, TAU, [1.0, rz / rx])
}
fn ecyl_closed(a: &Args) -> Measures {
    let (rx, rz, h) = (a.f("rx"), a.f("rz"), a.f("height"));
    Measures {
        size: Some(PI * rx * rz * h),
        boundary: Some(2.0 * PI * rx * rz + h * ellipse_perimeter(rx, rz)),
        centroid: Some([0.0, 0.5 * h, 0.0]),
        bounds: Some(([-rx, 0.0, -rz], [rx, h, rz])),
        series: Some("area: the ellipse's perimeter by the arithmetic-geometric mean (Gauss-Kummer) series, run to rounding"),
    }
}
fn ecyl_anchors(a: &Args) -> Vec<Anchor> {
    let (rx, rz, h) = (a.f("rx"), a.f("rz"), a.f("height"));
    vec![
        out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        at3("centre", [0.0, 0.5 * h, 0.0], On::Inside),
        out3("right", [rx, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("front", [0.0, 0.5 * h, rz], [0.0, 0.0, 1.0], On::Boundary),
    ]
}

const P_CSECT: &[Param] = &[len("radius", "the radius", 0.5), len("height", "the height", 1.0), deg("angle", "the sector's angle about Y, from +x", 90.0, 1.0, 359.0)];
fn csect_geom(a: &Args) -> Result<Geom, Refusal> {
    rev(cyl_pieces(a.f("radius"), a.f("height")), false, rad(a.f("angle")), [1.0, 1.0])
}
fn csect_closed(a: &Args) -> Measures {
    let (r, h, t) = (a.f("radius"), a.f("height"), rad(a.f("angle")));
    Measures { size: Some(0.5 * t * r * r * h), boundary: Some(t * r * r + t * r * h + 2.0 * r * h), ..Measures::default() }
}
fn csect_anchors(a: &Args) -> Vec<Anchor> {
    // The base and top on the sector's bisector at half its radius (the axis is an edge, with no
    // inside to step into); the axis's ends as points.
    let (r, h, t) = (a.f("radius"), a.f("height"), rad(a.f("angle")));
    let (s, c) = sin_cos(0.5 * t);
    let mid = [0.5 * r * c, 0.0, -0.5 * r * s];
    vec![
        out3("base", mid, [0.0, -1.0, 0.0], On::Boundary),
        out3("top", [mid[0], h, mid[2]], [0.0, 1.0, 0.0], On::Boundary),
        at3("axis_base", [0.0, 0.0, 0.0], On::Vertex),
        at3("axis_top", [0.0, h, 0.0], On::Vertex),
        out3("side", [r * c, 0.5 * h, -r * s], [c, 0.0, -s], On::Boundary),
    ]
}

const P_TUBE: &[Param] = &[len("radius", "the outer radius", 0.5), len("inner", "the inner radius (the bore's)", 0.4), len("height", "the height", 1.0)];
fn tube_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, i, h) = (a.f("radius"), a.f("inner"), a.f("height"));
    if i >= r {
        return Err(refuse("inner_inside_outer", "the bore must be narrower than the tube", &[("radius", r), ("inner", i)]));
    }
    Ok((r, i, h))
}
fn tube_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, i, h) = tube_parts(a)?;
    rev(vec![piece(line([i, 0.0], [r, 0.0]), "bottom"), piece(line([r, 0.0], [r, h]), "outside"), piece(line([r, h], [i, h]), "top"), piece(line([i, h], [i, 0.0]), "inside")], true, TAU, [1.0, 1.0])
}
fn tube_closed(a: &Args) -> Measures {
    let Ok((r, i, h)) = tube_parts(a) else { return Measures::default() };
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(PI * h * (r - i) * (r + i)), Some(2.0 * PI * (r - i) * (r + i) + 2.0 * PI * h * (r + i)), 0.5 * h, lo, hi)
}
fn tube_anchors(a: &Args) -> Vec<Anchor> {
    let (r, i, h) = (a.f("radius"), a.f("inner"), a.f("height"));
    vec![
        at3("axis_base", [0.0, 0.0, 0.0], On::Free),
        at3("axis_top", [0.0, h, 0.0], On::Free),
        out3("outside", [r, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("inside", [i, 0.5 * h, 0.0], [-1.0, 0.0, 0.0], On::Boundary),
        out3("rim", [0.5 * (r + i), h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("foot", [0.5 * (r + i), 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary),
    ]
}

fn cone_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, h) = (a.f("radius"), a.f("height"));
    turned(vec![piece(line(O, [r, 0.0]), "base"), piece(line([r, 0.0], [0.0, h]), "side")])
}
fn cone_closed(a: &Args) -> Measures {
    let (r, h) = (a.f("radius"), a.f("height"));
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(PI * r * r * h / 3.0), Some(PI * r * r + PI * r * (r * r + h * h).sqrt()), 0.25 * h, lo, hi)
}
fn cone_anchors(a: &Args) -> Vec<Anchor> {
    let (r, h) = (a.f("radius"), a.f("height"));
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), at3("apex", [0.0, h, 0.0], On::Vertex), at3("centre", [0.0, 0.25 * h, 0.0], On::Inside), out3("rim", [r, 0.0, 0.0], [h / (r * r + h * h).sqrt(), r / (r * r + h * h).sqrt() - 1.0, 0.0], On::Vertex)]
}

const P_FRUSTUM: &[Param] = &[len("radius", "the bottom's radius", 0.5), len("top", "the top's radius, under the bottom's", 0.25), len("height", "the height", 1.0)];
fn frustum_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, t, h) = (a.f("radius"), a.f("top"), a.f("height"));
    if t >= r {
        return Err(refuse("top_under_bottom", "the top must be narrower than the bottom (turn it over for the other way)", &[("radius", r), ("top", t)]));
    }
    Ok((r, t, h))
}
fn frustum_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t, h) = frustum_parts(a)?;
    turned(vec![piece(line(O, [r, 0.0]), "bottom"), piece(line([r, 0.0], [t, h]), "side"), piece(line([t, h], [0.0, h]), "top")])
}
fn frustum_closed(a: &Args) -> Measures {
    let Ok((r, t, h)) = frustum_parts(a) else { return Measures::default() };
    let s = r * r + r * t + t * t;
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(PI * h * s / 3.0), Some(PI * (r * r + t * t) + PI * (r + t) * ((r - t) * (r - t) + h * h).sqrt()), h * (r * r + 2.0 * r * t + 3.0 * t * t) / (4.0 * s), lo, hi)
}
fn frustum_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary)]
}

fn bicone_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, h) = (a.f("radius"), a.f("height"));
    turned(vec![piece(line(O, [r, 0.5 * h]), "lower"), piece(line([r, 0.5 * h], [0.0, h]), "upper")])
}
fn bicone_closed(a: &Args) -> Measures {
    let (r, h) = (a.f("radius"), a.f("height"));
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(PI * r * r * h / 3.0), Some(2.0 * PI * r * (r * r + 0.25 * h * h).sqrt()), 0.5 * h, lo, hi)
}
fn bicone_anchors(a: &Args) -> Vec<Anchor> {
    let (r, h) = (a.f("radius"), a.f("height"));
    vec![at3("base", [0.0, 0.0, 0.0], On::Vertex), at3("top", [0.0, h, 0.0], On::Vertex), at3("centre", [0.0, 0.5 * h, 0.0], On::Inside), out3("rim", [r, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Vertex)]
}

const P_CAPSULE: &[Param] = &[len("radius", "the radius", 0.25), len("length", "the length of its straight middle", 0.5)];
fn capsule_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, l) = (a.f("radius"), a.f("length"));
    turned(vec![piece(arc([0.0, r], r, -0.5 * PI, 0.0), "bottom"), piece(line([r, r], [r, r + l]), "side"), piece(arc([0.0, r + l], r, 0.0, 0.5 * PI), "top")])
}
fn capsule_closed(a: &Args) -> Measures {
    let (r, l) = (a.f("radius"), a.f("length"));
    let (lo, hi) = round_bounds(r, 0.0, 2.0 * r + l);
    m(Some(PI * r * r * (4.0 * r / 3.0 + l)), Some(4.0 * PI * r * r + 2.0 * PI * r * l), r + 0.5 * l, lo, hi)
}
fn capsule_anchors(a: &Args) -> Vec<Anchor> {
    let (r, l) = (a.f("radius"), a.f("length"));
    axial((0.0, On::Vertex), (2.0 * r + l, On::Vertex), r + 0.5 * l, r, Some(r + 0.5 * l))
}

// ---------------------------------------------------------------- tori

const P_TORUS: &[Param] = &[len("radius", "from the axis to the tube's middle", 0.5), len("tube", "the tube's radius, under the other", 0.15)];
fn torus_parts(a: &Args) -> Result<(f64, f64), Refusal> {
    let (big, r) = (a.f("radius"), a.f("tube"));
    if r >= big {
        return Err(refuse("tube_inside_radius", "the tube must be thinner than its distance from the axis (or the torus crosses itself)", &[("radius", big), ("tube", r)]));
    }
    Ok((big, r))
}
fn torus_ring(big: f64, r: f64) -> Vec<Piece> {
    vec![piece(arc([big, r], r, -0.5 * PI, 1.5 * PI), "torus")]
}
fn torus_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, r) = torus_parts(a)?;
    rev(torus_ring(big, r), true, TAU, [1.0, 1.0])
}
fn torus_closed(a: &Args) -> Measures {
    let Ok((big, r)) = torus_parts(a) else { return Measures::default() };
    let (lo, hi) = round_bounds(big + r, 0.0, 2.0 * r);
    m(Some(2.0 * PI * PI * big * r * r), Some(4.0 * PI * PI * big * r), r, lo, hi)
}
fn torus_anchors(a: &Args) -> Vec<Anchor> {
    let (big, r) = (a.f("radius"), a.f("tube"));
    let mut v = vec![at3("centre", [0.0, r, 0.0], On::Free), out3("outer", [big + r, r, 0.0], [1.0, 0.0, 0.0], On::Boundary), out3("top", [big, 2.0 * r, 0.0], [0.0, 1.0, 0.0], On::Boundary), out3("base", [big, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary)];
    for i in 0..8 {
        let (s, c) = sin_cos(TAU * i as f64 / 8.0);
        v.push(at3(&format!("ring_{i}"), [big * c, r, -big * s], On::Inside));
    }
    v
}

const P_TSECT: &[Param] = &[len("radius", "from the axis to the tube's middle", 0.5), len("tube", "the tube's radius, under the other", 0.15), deg("angle", "the bend's angle about Y, from +x", 90.0, 1.0, 359.0)];
fn tsect_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, r) = torus_parts(a)?;
    rev(torus_ring(big, r), true, rad(a.f("angle")), [1.0, 1.0])
}
fn tsect_closed(a: &Args) -> Measures {
    let Ok((big, r)) = torus_parts(a) else { return Measures::default() };
    let t = rad(a.f("angle"));
    Measures { size: Some(t * big * PI * r * r), boundary: Some(t * big * TAU * r + TAU * r * r), ..Measures::default() }
}
fn tsect_anchors(a: &Args) -> Vec<Anchor> {
    let (big, r, t) = (a.f("radius"), a.f("tube"), rad(a.f("angle")));
    let (s, c) = sin_cos(t);
    vec![out3("start", [big, r, 0.0], [0.0, 0.0, 1.0], On::Boundary), out3("end", [big * c, r, -big * s], [-s, 0.0, -c], On::Boundary)]
}

const P_ETORUS: &[Param] = &[len("radius", "from the axis to the tube's middle", 0.5), len("width", "the tube's semi-axis outward from the axis", 0.15), len("height", "the tube's semi-axis along Y", 0.25)];
fn etorus_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (big, w, h) = (a.f("radius"), a.f("width"), a.f("height"));
    if w >= big {
        return Err(refuse("tube_inside_radius", "the tube must be narrower than its distance from the axis", &[("radius", big), ("width", w)]));
    }
    Ok((big, w, h))
}
fn etorus_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, w, h) = etorus_parts(a)?;
    let f = move |t: f64| {
        let (s, c) = sin_cos(t);
        [big + w * c, h + h * s]
    };
    let n = move |t: f64| {
        let (s, c) = sin_cos(t);
        [h * c, w * s]
    };
    rev(vec![fpiece(func(f, 0.0, TAU, 1.0), "torus", n)], true, TAU, [1.0, 1.0])
}
fn etorus_closed(a: &Args) -> Measures {
    let Ok((big, w, h)) = etorus_parts(a) else { return Measures::default() };
    m(Some(TAU * big * PI * w * h), None, h, [-(big + w), 0.0, -(big + w)], [big + w, 2.0 * h, big + w])
}
fn etorus_anchors(a: &Args) -> Vec<Anchor> {
    let (big, w, h) = (a.f("radius"), a.f("width"), a.f("height"));
    vec![at3("centre", [0.0, h, 0.0], On::Free), out3("outer", [big + w, h, 0.0], [1.0, 0.0, 0.0], On::Vertex), at3("ring_0", [big, h, 0.0], On::Inside)]
}

// ---------------------------------------------------------------- other solids of revolution

const P_LENS: &[Param] = &[len("radius", "the rim's radius", 0.5), len("thickness", "from face to face on the axis, at most the rim's diameter", 0.2)];
fn lens_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, t) = (a.f("radius"), a.f("thickness"));
    if t > 2.0 * r {
        return Err(refuse("thicker_than_wide", "each face is at most a half sphere: the thickness at most the rim's diameter", &[("radius", r), ("thickness", t)]));
    }
    let h = 0.5 * t;
    Ok((r, t, (r * r + h * h) / (2.0 * h)))
}
fn lens_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t, rho) = lens_parts(a)?;
    let h = 0.5 * t;
    turned(vec![piece(arc([0.0, rho], rho, -0.5 * PI, atan2(h - rho, r)), "lower"), piece(arc([0.0, t - rho], rho, atan2(rho - h, r), 0.5 * PI), "upper")])
}
fn lens_closed(a: &Args) -> Measures {
    let Ok((r, t, rho)) = lens_parts(a) else { return Measures::default() };
    let h = 0.5 * t;
    let (lo, hi) = round_bounds(r, 0.0, t);
    m(Some(2.0 * PI * h * h * (3.0 * rho - h) / 3.0), Some(4.0 * PI * rho * h), h, lo, hi)
}
fn lens_anchors(a: &Args) -> Vec<Anchor> {
    let (r, t) = (a.f("radius"), a.f("thickness"));
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Vertex), out3("top", [0.0, t, 0.0], [0.0, 1.0, 0.0], On::Vertex), at3("centre", [0.0, 0.5 * t, 0.0], On::Inside), at3("rim", [r, 0.5 * t, 0.0], On::Vertex)]
}

const P_SPINDLE: &[Param] = &[len("radius", "the middle's radius, at most half the height", 0.3), len("height", "from tip to tip", 1.0)];
fn spindle_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (w, h) = (a.f("radius"), a.f("height"));
    if w > 0.5 * h {
        return Err(refuse("wider_than_half_height", "a spindle is at most half as wide as it is tall (past that its arc dips below its tips: an apple, which is not built)", &[("radius", w), ("height", h)]));
    }
    // The arc's centre (c, h/2), c <= 0, through both tips and (w, h/2).
    let c = (w - 0.5 * h) * (w + 0.5 * h) / (2.0 * w);
    Ok((w, h, c, w - c))
}
// (The textbook formulas, V = π (ρ² h − h³/12 + 2 c ρ² asin(h / 2ρ)) and A = 2π ρ (h + 2 c
// asin(h / 2ρ)), cancel for a thin spindle, where ρ is far over h: at a radius of 1e-4 and a height
// of 1 the volume came out 4% short. They are left out; the arc's own integrals give them.)
fn spindle_geom(a: &Args) -> Result<Geom, Refusal> {
    let (_, h, c, rho) = spindle_parts(a)?;
    turned(vec![piece(arc([c, 0.5 * h], rho, atan2(-0.5 * h, -c), atan2(0.5 * h, -c)), "spindle")])
}
fn spindle_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("radius"), a.f("height"));
    axial((0.0, On::Vertex), (h, On::Vertex), 0.5 * h, w, Some(0.5 * h))
}

const P_BARREL: &[Param] = &[len("radius", "the ends' radius", 0.4), len("bulge", "the middle's radius, over the ends'", 0.5), len("height", "the height", 1.0)];
fn barrel_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (re, rm, h) = (a.f("radius"), a.f("bulge"), a.f("height"));
    if rm <= re {
        return Err(refuse("bulge_over_ends", "a barrel's middle is wider than its ends (an hourglass is the other way)", &[("radius", re), ("bulge", rm)]));
    }
    // The staves' arc: centre (c, h/2) with (re − c)² + h²/4 = (rm − c)².
    let c = ((rm - re) * (rm + re) - 0.25 * h * h) / (2.0 * (rm - re));
    if c >= re {
        return Err(refuse("staves_past_half_circle", "the staves would bend past a half circle (the bulge is too great for the height)", &[("radius", re), ("bulge", rm), ("height", h)]));
    }
    Ok((re, h, c, rm - c))
}
fn barrel_geom(a: &Args) -> Result<Geom, Refusal> {
    let (re, h, c, rho) = barrel_parts(a)?;
    turned(vec![piece(line(O, [re, 0.0]), "bottom"), piece(arc([c, 0.5 * h], rho, atan2(-0.5 * h, re - c), atan2(0.5 * h, re - c)), "side"), piece(line([re, h], [0.0, h]), "top")])
}
fn barrel_anchors(a: &Args) -> Vec<Anchor> {
    let (rm, h) = (a.f("bulge"), a.f("height"));
    axial((0.0, On::Boundary), (h, On::Boundary), 0.5 * h, rm, Some(0.5 * h))
}

const P_HOURGLASS: &[Param] = &[len("radius", "the ends' radius", 0.5), len("waist", "the waist's radius, under the ends'", 0.2), len("height", "the height", 1.0)];
fn hourglass_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (re, rw, h) = (a.f("radius"), a.f("waist"), a.f("height"));
    if rw >= re {
        return Err(refuse("waist_under_ends", "an hourglass's waist is narrower than its ends", &[("radius", re), ("waist", rw)]));
    }
    // The side's arc: centre (c, h/2), c > rw, with (c − re)² + h²/4 = (c − rw)².
    let c = ((re - rw) * (re + rw) + 0.25 * h * h) / (2.0 * (re - rw));
    if c <= re {
        return Err(refuse("side_past_half_circle", "the side would bend past a half circle (the waist is too narrow for the height)", &[("radius", re), ("waist", rw), ("height", h)]));
    }
    Ok((re, h, c, c - rw))
}
fn hourglass_geom(a: &Args) -> Result<Geom, Refusal> {
    let (re, h, c, rho) = hourglass_parts(a)?;
    let (s, e) = (atan2(-0.5 * h, re - c), atan2(0.5 * h, re - c));
    turned(vec![piece(line(O, [re, 0.0]), "bottom"), piece(arc([c, 0.5 * h], rho, s, e - TAU), "side"), piece(line([re, h], [0.0, h]), "top")])
}
fn hourglass_anchors(a: &Args) -> Vec<Anchor> {
    let (rw, h) = (a.f("waist"), a.f("height"));
    axial((0.0, On::Boundary), (h, On::Boundary), 0.5 * h, rw, Some(0.5 * h))
}

fn egg_geom(a: &Args) -> Result<Geom, Refusal> {
    // Moss's egg (the plane `moss_egg`), its right half turned: the lower half circle's quarter,
    // the arc of twice its radius about the far end of its diameter, and the small cap.
    let r = a.f("radius");
    let rho = (2.0 - 2f64.sqrt()) * r;
    turned(vec![piece(arc([0.0, r], r, -0.5 * PI, 0.0), "lower"), piece(arc([-r, r], 2.0 * r, 0.0, 0.25 * PI), "side"), piece(arc([0.0, 2.0 * r], rho, 0.25 * PI, 0.5 * PI), "cap")])
}
fn egg_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    axial((0.0, On::Vertex), (2.0 * r + (2.0 - 2f64.sqrt()) * r, On::Vertex), r, r, Some(r))
}

fn tdrop_geom(a: &Args) -> Result<Geom, Refusal> {
    // The plane `teardrop`'s right half turned: the round end's arc to the tangent point, and the
    // tangent line to the tip.
    let (r, l, b) = drop_parts(a)?;
    let c = [0.0, r];
    turned(vec![piece(arc(c, r, -0.5 * PI, 0.5 * PI - b), "round"), piece(line(polar(c, r, 0.5 * PI - b), [0.0, l + r]), "point")])
}
fn tdrop_anchors(a: &Args) -> Vec<Anchor> {
    let (r, l) = (a.f("radius"), a.f("length"));
    axial((0.0, On::Vertex), (l + r, On::Vertex), r, r, Some(r))
}

const P_PARAB: &[Param] = &[len("radius", "the rim's radius", 0.5), len("height", "from the vertex up to the rim", 0.5)];
fn parab_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, h) = (a.f("radius"), a.f("height"));
    let k = h / (r * r);
    let f = move |t: f64| [t, k * t * t];
    let n = move |t: f64| [2.0 * k * t, -1.0];
    turned(vec![fpiece(func(f, 0.0, r, atan(2.0 * h / r) / TAU), "bowl", n), piece(line([r, h], [0.0, h]), "top")])
}
fn parab_closed(a: &Args) -> Measures {
    let (r, h) = (a.f("radius"), a.f("height"));
    // The bowl's area π r / (6 h²) ((r² + 4h²)^{3/2} − r³) as π r⁴ / (6 h²) ((1 + u)^{3/2} − 1),
    // u = 4h²/r², by expm1 and ln1p so that a shallow bowl keeps its digits.
    let u = 4.0 * h * h / (r * r);
    let bowl = PI * r * r * r * r / (6.0 * h * h) * expm1(1.5 * ln1p(u));
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(0.5 * PI * r * r * h), Some(bowl + PI * r * r), 2.0 * h / 3.0, lo, hi)
}
fn parab_anchors(a: &Args) -> Vec<Anchor> {
    let h = a.f("height");
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Vertex), out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary), at3("centre", [0.0, 2.0 * h / 3.0, 0.0], On::Inside)]
}

const P_HYPER: &[Param] = &[len("radius", "the ends' radius", 0.5), len("waist", "the waist's radius, under the ends'", 0.3), len("height", "the height", 1.0)];
fn hyper_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (r, w, h) = (a.f("radius"), a.f("waist"), a.f("height"));
    if w >= r {
        return Err(refuse("waist_under_ends", "the waist must be narrower than the ends", &[("radius", r), ("waist", w)]));
    }
    // r(u) = w √(1 + u²/c²) through (r, ±h/2): c = (h/2) w / √(r² − w²).
    Ok((r, w, h, 0.5 * h * w / ((r - w) * (r + w)).sqrt()))
}
fn hyper_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, w, h, c) = hyper_parts(a)?;
    let f = move |u: f64| [w * (1.0 + u * u / (c * c)).sqrt(), u + 0.5 * h];
    let n = move |u: f64| [1.0, -w * u / (c * c * (1.0 + u * u / (c * c)).sqrt())];
    let slope = w * w * h / (2.0 * c * c * r);
    turned(vec![piece(line(O, [r, 0.0]), "bottom"), fpiece(func(f, -0.5 * h, 0.5 * h, 2.0 * atan(slope) / TAU), "side", n), piece(line([r, h], [0.0, h]), "top")])
}
fn hyper_closed(a: &Args) -> Measures {
    let Ok((r, w, h, _)) = hyper_parts(a) else { return Measures::default() };
    let (lo, hi) = round_bounds(r, 0.0, h);
    m(Some(PI * h * (2.0 * w * w + r * r) / 3.0), None, 0.5 * h, lo, hi)
}
fn hyper_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("waist"), a.f("height"));
    axial((0.0, On::Boundary), (h, On::Boundary), 0.5 * h, w, Some(0.5 * h))
}

// ---------------------------------------------------------------- rounded blocks

const P_SUPERELL: &[Param] = &[len("rx", "the semi-axis along x", 0.5), len("ry", "the semi-axis along y", 0.5), len("rz", "the semi-axis along z", 0.5), ratio("exponent", "|x/rx|^n + |y/ry|^n + |z/rz|^n = 1: 2 a sphere, larger squarer", 4.0, 1.5, 32.0)];
fn superell_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, ry, rz, n) = (a.f("rx"), a.f("ry"), a.f("rz"), a.f("exponent"));
    let rr = [rx, ry, rz];
    let surf = move |d: P3| -> P3 {
        let s: f64 = (0..3).map(|i| powf((d[i] / rr[i]).abs(), n)).sum();
        scale3(d, 1.0 / powf(s, 1.0 / n))
    };
    let grad = move |p: P3| -> P3 { norm3([0, 1, 2].map(|i| (if p[i] < 0.0 { -1.0 } else { 1.0 }) * powf((p[i] / rr[i]).abs(), n - 1.0) / rr[i])) };
    let big = rx.max(ry).max(rz);
    let lattice = move |d: u32| {
        let g = segments(0.25, d) as u32;
        Lattice {
            n: [g + 1; 3],
            point: move |q: [u32; 3]| {
                let c = q.map(|i| tan(-0.25 * PI + 0.5 * PI * i as f64 / g as f64));
                let p = surf(c);
                ([p[0], p[1] + ry, p[2]], Some(grad(p)))
            },
            cell: |_, _, _| (true, 0u32),
            surfaces: vec![Surface { name: "superellipsoid".into(), smooth: true, radius: Some(big) }],
        }
    };
    let g1 = gamma(1.0 + 1.0 / n);
    let exact = m(Some(8.0 * rx * ry * rz * g1 * g1 * g1 / gamma(1.0 + 3.0 / n)), None, ry, [-rx, 0.0, -rz], [rx, 2.0 * ry, rz]);
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(move |d| {
            let l = lattice(d);
            (l.points(), l.faces())
        }),
        build: Rc::new(move |d| {
            let mut mesh = lattice(d).mesh();
            mesh.chord_error = middle_error(&mesh, |p| {
                let q = surf([p[0], p[1] - ry, p[2]]);
                [q[0], q[1] + ry, q[2]]
            });
            (mesh, None)
        }),
        exact,
        error: Rc::new(|_| None),
        genus: 0,
        check: Some(Rc::new(|m: &Mesh, _| m.self_crossing().map(|(a, b)| SolidFault::Crossing { a, b }))),
    })))
}
fn superell_anchors(a: &Args) -> Vec<Anchor> {
    let (rx, ry, rz) = (a.f("rx"), a.f("ry"), a.f("rz"));
    vec![
        out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        out3("top", [0.0, 2.0 * ry, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        at3("centre", [0.0, ry, 0.0], On::Inside),
        out3("right", [rx, ry, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("front", [0.0, ry, rz], [0.0, 0.0, 1.0], On::Boundary),
    ]
}

const P_RBOX: &[Param] = &[len("width", "along x", 1.0), len("height", "along y", 1.0), len("depth", "along z", 1.0), len("radius", "the edges' and corners' radius, at most half the least side", 0.1)];
fn rbox_parts(a: &Args) -> Result<([f64; 3], f64), Refusal> {
    let (w, h, d, r) = (a.f("width"), a.f("height"), a.f("depth"), a.f("radius"));
    let least = w.min(h).min(d);
    if r > 0.5 * least {
        return Err(refuse("radius_over_half_side", "the rounding is at most half the least side", &[("radius", r), ("least_side", least)]));
    }
    Ok(([0.5 * w, 0.5 * h, 0.5 * d], r))
}
/// One axis of a rounded box's lattice: the outer box's coordinates of its samples (a rounded
/// band at each end, cut at equal angles: `tan` of them; and between them, where the side is
/// longer than the two roundings, one flat cell), and whether that flat cell is there.
fn rbox_axis(half: f64, r: f64, q: u32) -> (Vec<f64>, bool) {
    let inner = half - r;
    let mut v: Vec<f64> = (0..=q).map(|j| -(inner + r * tan(0.25 * PI * (q - j) as f64 / q as f64))).collect();
    let flat = inner > 0.0;
    let plus = (0..=q).map(|j| inner + r * tan(0.25 * PI * j as f64 / q as f64));
    if flat {
        v.extend(plus);
    } else {
        v.extend(plus.skip(1));
    }
    (v, flat)
}
fn rbox_geom(a: &Args) -> Result<Geom, Refusal> {
    let (half, r) = rbox_parts(a)?;
    let lattice = move |d: u32| {
        let q = segments(0.25, d) as u32;
        let axes: Vec<(Vec<f64>, bool)> = half.iter().map(|&h| rbox_axis(h, r, q)).collect();
        let n = [0, 1, 2].map(|k| axes[k].0.len() as u32);
        let flat = [0, 1, 2].map(|k| axes[k].1);
        let coords: Vec<Vec<f64>> = axes.into_iter().map(|a| a.0).collect();
        Lattice {
            n,
            point: move |i: [u32; 3]| {
                let qp = [0, 1, 2].map(|k| coords[k][i[k] as usize]);
                let c = [0, 1, 2].map(|k| qp[k].clamp(-(half[k] - r), half[k] - r));
                let nrm = norm3(sub3(qp, c));
                let p = add3(c, scale3(nrm, r));
                ([p[0], p[1] + half[1], p[2]], Some(nrm))
            },
            cell: move |k: usize, i: u32, j: u32| {
                let (u, v) = ((k + 1) % 3, (k + 2) % 3);
                let fu = flat[u] && i == q;
                let fv = flat[v] && j == q;
                match (fu, fv) {
                    (true, true) => (false, 0u32),
                    (true, false) | (false, true) => (false, 1),
                    (false, false) => (true, 2),
                }
            },
            surfaces: vec![Surface { name: "faces".into(), smooth: false, radius: None }, Surface { name: "edges".into(), smooth: true, radius: Some(r) }, Surface { name: "corners".into(), smooth: true, radius: Some(r) }],
        }
    };
    let (w, h, dd) = (2.0 * half[0], 2.0 * half[1], 2.0 * half[2]);
    // Steiner's formula for the inner box (a b c) grown by r: V = abc + 2r(ab + bc + ca) +
    // πr²(a + b + c) + 4πr³/3; A = 2(ab + bc + ca) + 2πr(a + b + c) + 4πr².
    let (ia, ib, ic) = (w - 2.0 * r, h - 2.0 * r, dd - 2.0 * r);
    let s2 = ia * ib + ib * ic + ic * ia;
    let s1 = ia + ib + ic;
    let exact = m(Some(ia * ib * ic + 2.0 * r * s2 + PI * r * r * s1 + 4.0 / 3.0 * PI * r * r * r), Some(2.0 * s2 + TAU * r * s1 + 4.0 * PI * r * r), half[1], [-half[0], 0.0, -half[2]], [half[0], h, half[2]]);
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(move |d| {
            let l = lattice(d);
            (l.points(), l.faces())
        }),
        build: Rc::new(move |d| {
            let mut mesh = lattice(d).mesh();
            let q = segments(0.25, d) as f64;
            // The edges' sag exactly; and each corner face's distance from its own ball (its
            // points all have one centre: a point less the radius along its normal).
            let mut err = r * (1.0 - cos(0.125 * PI / q));
            for f in mesh.faces.iter().filter(|f| f.surface == 2) {
                let n0 = f.normals.as_ref().map(|n| n[0]).unwrap_or([0.0; 3]);
                let c = sub3(mesh.points[f.verts[0] as usize], scale3(n0, r));
                let fnrm = norm3(mesh.area_vector(f));
                err = err.max(r - dot3(fnrm, sub3(mesh.points[f.verts[0] as usize], c)).abs());
            }
            mesh.chord_error = err;
            (mesh, None)
        }),
        exact,
        error: Rc::new(|_| None),
        genus: 0,
        check: None,
    })))
}
fn rbox_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, d) = (0.5 * a.f("width"), a.f("height"), 0.5 * a.f("depth"));
    vec![
        at3("centre", [0.0, 0.5 * h, 0.0], On::Inside),
        out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("right", [w, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("left", [-w, 0.5 * h, 0.0], [-1.0, 0.0, 0.0], On::Boundary),
        out3("front", [0.0, 0.5 * h, d], [0.0, 0.0, 1.0], On::Boundary),
        out3("back", [0.0, 0.5 * h, -d], [0.0, 0.0, -1.0], On::Boundary),
    ]
}

// ---------------------------------------------------------------- shells

const P_SHELL: &[Param] = &[len("radius", "the outer radius", 0.5), len("thickness", "the wall's thickness, under the radius", 0.05)];
fn shell_parts(a: &Args) -> Result<(f64, f64), Refusal> {
    let (r, t) = (a.f("radius"), a.f("thickness"));
    if t >= r {
        return Err(refuse("thickness_under_radius", "the wall must be thinner than the radius (a solid half ball is `hemisphere`)", &[("radius", r), ("thickness", t)]));
    }
    Ok((r, r - t))
}
/// The shell's volume `2π (R³ − r³) / 3` as `2π (R − r)(R² + R r + r²) / 3`, which keeps its
/// digits for a thin wall; its area; and its centroid's height over the rim,
/// `3 (R⁴ − r⁴) / (8 (R³ − r³)) = 3 (R + r)(R² + r²) / (8 (R² + R r + r²))`.
fn shell_measures(r: f64, i: f64) -> (f64, f64, f64) {
    let t = r - i;
    let s = r * r + r * i + i * i;
    (TAU * t * s / 3.0, TAU * (r * r + i * i) + PI * t * (r + i), 3.0 * (r + i) * (r * r + i * i) / (8.0 * s))
}
fn dome_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, i) = shell_parts(a)?;
    turned(vec![piece(arc(O, i, 0.5 * PI, 0.0), "inside"), piece(line([i, 0.0], [r, 0.0]), "rim"), piece(arc(O, r, 0.0, 0.5 * PI), "outside")])
}
fn dome_closed(a: &Args) -> Measures {
    let Ok((r, i)) = shell_parts(a) else { return Measures::default() };
    let (v, ar, cy) = shell_measures(r, i);
    let (lo, hi) = round_bounds(r, 0.0, r);
    m(Some(v), Some(ar), cy, lo, hi)
}
fn dome_anchors(a: &Args) -> Vec<Anchor> {
    let (r, t) = (a.f("radius"), a.f("thickness"));
    vec![out3("top", [0.0, r, 0.0], [0.0, 1.0, 0.0], On::Vertex), out3("underside", [0.0, r - t, 0.0], [0.0, -1.0, 0.0], On::Vertex), out3("rim", [r - 0.5 * t, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), at3("axis_base", [0.0, 0.0, 0.0], On::Free)]
}
fn bowl_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, i) = shell_parts(a)?;
    let c = [0.0, r];
    turned(vec![piece(arc(c, r, -0.5 * PI, 0.0), "outside"), piece(line([r, r], [i, r]), "rim"), piece(arc(c, i, 0.0, -0.5 * PI), "inside")])
}
fn bowl_closed(a: &Args) -> Measures {
    let Ok((r, i)) = shell_parts(a) else { return Measures::default() };
    let (v, ar, cy) = shell_measures(r, i);
    let (lo, hi) = round_bounds(r, 0.0, r);
    m(Some(v), Some(ar), r - cy, lo, hi)
}
fn bowl_anchors(a: &Args) -> Vec<Anchor> {
    let (r, t) = (a.f("radius"), a.f("thickness"));
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Vertex), out3("inside_bottom", [0.0, t, 0.0], [0.0, 1.0, 0.0], On::Vertex), out3("rim", [r - 0.5 * t, r, 0.0], [0.0, 1.0, 0.0], On::Boundary), at3("axis_top", [0.0, r, 0.0], On::Free)]
}

// ---------------------------------------------------------------- tubes along curves

/// A round wire of radius `w` along a curve: the curve's parameters at its samples (`ts` of the
/// number of steps, `along(detail)`), its centre and its frame `(n, b)` (unit, at right angles
/// to the curve and to each other, `b = t × n`), closed or capped. The build's error is an
/// estimate, not a bound: the wire's round sag `w (1 − cos(π / detail))` plus the curve's chord
/// sag on the wire's outer side, `(1/κ + w)(1 − cos(κ s / 2))` for the longest step `s` and `κ` a
/// bound on the curve's curvature.
#[allow(clippy::too_many_arguments)]
fn sweep(w: f64, exact: Measures, along: impl Fn(u32) -> u64 + 'static, ts: impl Fn(u64) -> Vec<f64> + 'static, centre: impl Fn(f64) -> P3 + 'static, frame: impl Fn(f64) -> (P3, P3) + 'static, closed: bool, kappa: f64) -> Result<Geom, Refusal> {
    let along = Rc::new(along);
    let (a2, a3) = (along.clone(), along.clone());
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(move |d| {
            let mm = a2(d);
            let rings = if closed { mm } else { mm + 1 };
            let corners = segments(1.0, d);
            (rings * corners, 2 * corners * mm + if closed { 0 } else { 2 })
        }),
        build: Rc::new(move |d| {
            let mm = along(d);
            let t = ts(mm);
            let corners = segments(1.0, d) as usize;
            let rings = if closed { mm as usize } else { mm as usize + 1 };
            let surfaces = vec![Surface { name: "wire".into(), smooth: true, radius: Some(w) }, Surface { name: "ends".into(), smooth: false, radius: None }];
            let ring = |i: usize| {
                let c = centre(t[i]);
                let (n, b) = frame(t[i]);
                (0..corners)
                    .map(|j| {
                        let (s, co) = sin_cos(TAU * j as f64 / corners as f64);
                        let u = add3(scale3(n, co), scale3(b, s));
                        (add3(c, scale3(u, w)), Some(u))
                    })
                    .collect()
            };
            let mut mesh = tube_mesh(rings, corners, closed, 0, ring, |_| 0, surfaces, 1);
            let step = (1..t.len()).map(|i| dist3(centre(t[i - 1]), centre(t[i]))).fold(0.0f64, f64::max);
            let bend = if kappa > 0.0 { (1.0 / kappa + w) * (1.0 - cos(0.5 * kappa * step)) } else { 0.0 };
            mesh.chord_error += w * (1.0 - cos(PI / corners as f64)) + bend;
            (mesh, None)
        }),
        exact,
        error: Rc::new(|_| None),
        genus: u32::from(closed),
        check: Some(Rc::new(move |m: &Mesh, d| {
            let mm = a3(d) as usize;
            tube_check(m, if closed { mm } else { mm + 1 }, segments(1.0, d) as usize, closed, 0)
        })),
    })))
}

const P_HELIX: &[Param] = &[len("radius", "from the axis to the wire's middle", 0.3), len("wire", "the wire's radius", 0.03), len("pitch", "the rise in one turn", 0.15), ratio("coils", "the number of turns (need not be whole)", 5.0, 0.25, 50.0)];
fn helix_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, w, p, coils) = (a.f("radius"), a.f("wire"), a.f("pitch"), a.f("coils"));
    if w >= big {
        return Err(refuse("wire_inside_radius", "the wire must be thinner than its distance from the axis", &[("radius", big), ("wire", w)]));
    }
    // Neighbouring turns, seen across the wire (along the axis, leaned by the helix's angle), are
    // `p cos(angle)` apart: the wire must be thinner than half of that.
    let clear = p * TAU * big / ((TAU * big) * (TAU * big) + p * p).sqrt();
    if clear <= 2.0 * w {
        return Err(refuse("coils_touch", "neighbouring turns would touch: the pitch, measured across the wire, must be over the wire's diameter", &[("pitch", p), ("wire", w), ("clearance", clear)]));
    }
    let b = p / TAU;
    let total = TAU * coils;
    let ls = (big * big + b * b).sqrt();
    let y0 = w * big / ls;
    let centre = move |t: f64| [big * cos(t), y0 + b * t, -big * sin(t)];
    let frame = move |t: f64| {
        let (s, c) = sin_cos(t);
        let tan = scale3([-big * s, b, -big * c], 1.0 / ls);
        let n = [c, 0.0, -s];
        (n, cross3(tan, n))
    };
    let length = coils * ((TAU * big) * (TAU * big) + p * p).sqrt();
    // The centroid: the centreline's, moved out from the axis by κ w² / 4 (the wire's outer side
    // is longer than its inner), κ = R / (R² + b²) the helix's curvature.
    let kappa = big / (ls * ls);
    let rr = big + kappa * w * w / 4.0;
    let hs = sin(0.5 * total);
    let exact = Measures {
        size: Some(PI * w * w * length),
        boundary: Some(TAU * w * length + TAU * w * w),
        centroid: Some([rr * sin(total) / total, y0 + 0.5 * b * total, -rr * 2.0 * hs * hs / total]),
        bounds: None,
        series: None,
    };
    sweep(w, exact, move |d| segments(coils, d), move |m| (0..=m).map(|i| total * i as f64 / m as f64).collect(), centre, frame, false, kappa)
}
fn helix_anchors(a: &Args) -> Vec<Anchor> {
    let (big, w, p) = (a.f("radius"), a.f("wire"), a.f("pitch"));
    let b = p / TAU;
    let y0 = w * big / (big * big + b * b).sqrt();
    vec![at3("start", [big, y0, 0.0], On::Boundary), at3("axis_base", [0.0, 0.0, 0.0], On::Free)]
}

const P_KNOT: &[Param] = &[
    len("radius", "the torus's radius (axis to the tube's middle)", 0.5),
    len("tube", "the torus's tube, which the knot winds on", 0.2),
    len("wire", "the knot's wire radius", 0.05),
    count("p", "turns about the axis", 2.0, 2.0, 16.0),
    count("q", "turns through the hole", 3.0, 1.0, 16.0),
];
fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}
fn knot_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, r0, w) = (a.f("radius"), a.f("tube"), a.f("wire"));
    let (p, q) = (a.f("p"), a.f("q"));
    if gcd(p as u64, q as u64) != 1 {
        return Err(refuse("p_q_share_a_factor", "p and q must have no common factor (otherwise the curve is a link of several loops, not one knot)", &[("p", p), ("q", q)]));
    }
    if r0 >= big {
        return Err(refuse("tube_inside_radius", "the torus's tube must be thinner than its distance from the axis", &[("radius", big), ("tube", r0)]));
    }
    if w >= r0 {
        return Err(refuse("wire_under_tube", "the wire must be thinner than the torus's tube it winds on", &[("tube", r0), ("wire", w)]));
    }
    let lift = r0 + w;
    let centre = move |t: f64| {
        let (sq, cq) = sin_cos(q * t);
        let (sp, cp) = sin_cos(p * t);
        let rr = big + r0 * cq;
        [rr * cp, lift + r0 * sq, -rr * sp]
    };
    let frame = move |t: f64| {
        let (sq, cq) = sin_cos(q * t);
        let (sp, cp) = sin_cos(p * t);
        let rr = big + r0 * cq;
        let d = [-r0 * q * sq * cp - rr * p * sp, r0 * q * cq, r0 * q * sq * sp - rr * p * cp];
        let n = [cq * cp, sq, -cq * sp];
        (n, cross3(norm3(d), n))
    };
    // The centreline's length ∫ |c'| over a turn, by the 8-point rule on 64 (p + q) pieces (on
    // each, the integrand's phases move by at most 2π/64).
    let pieces = 64 * (p + q) as usize;
    let speed = move |t: f64| {
        let rr = big + r0 * cos(q * t);
        ((p * rr) * (p * rr) + (q * r0) * (q * r0)).sqrt()
    };
    let mut length = 0.0;
    for i in 0..pieces {
        let (t0, t1) = (TAU * i as f64 / pieces as f64, TAU * (i + 1) as f64 / pieces as f64);
        let (hh, mm) = (0.5 * (t1 - t0), 0.5 * (t0 + t1));
        length += gauss().iter().map(|&(x, wt)| wt * hh * speed(mm + hh * x)).sum::<f64>();
    }
    // A bound on the curvature for the error estimate: no tighter than the torus's tube's.
    let kappa = 1.0 / (0.5 * r0).min(big - r0);
    let exact = Measures {
        size: Some(PI * w * w * length),
        boundary: Some(TAU * w * length),
        centroid: Some([0.0, lift, 0.0]),
        bounds: None,
        series: Some("the centreline's length: the 8-point Gauss-Legendre rule on 64 (p + q) pieces, to rounding"),
    };
    let turns = p + q;
    sweep(w, exact, move |d| segments(turns, d), move |m| (0..m).map(|i| TAU * i as f64 / m as f64).collect(), centre, frame, true, kappa)
}
fn knot_anchors(a: &Args) -> Vec<Anchor> {
    let lift = a.f("tube") + a.f("wire");
    vec![at3("centre", [0.0, lift, 0.0], On::Free)]
}

const P_MOBIUS: &[Param] = &[len("radius", "from the axis to the band's middle", 0.5), len("width", "the band's width", 0.2), len("thickness", "the band's thickness", 0.02)];
fn mobius_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, w, th) = (a.f("radius"), a.f("width"), a.f("thickness"));
    let d = 0.5 * (w * w + th * th).sqrt();
    if d >= big {
        return Err(refuse("band_crosses_axis", "the band's half diagonal must be under its radius", &[("radius", big), ("half_diagonal", d)]));
    }
    let corners: [Pt; 4] = [[0.5 * w, -0.5 * th], [0.5 * w, 0.5 * th], [-0.5 * w, 0.5 * th], [-0.5 * w, -0.5 * th]];
    // (No bounds: its reach out from the axis is greatest only where the section's diagonal
    // points outward, so they are not the same on every side.)
    let exact = Measures { size: Some(w * th * TAU * big), boundary: None, centroid: Some([0.0, d, 0.0]), bounds: None, series: None };
    Ok(Geom::Solid(Rc::new(Gen {
        // Two turns' worth of sections along: one for the loop and one for the twist (the section
        // turns half a turn as the loop turns once; at eight sections a turn it crosses itself).
        count: Rc::new(|dd| {
            let m = segments(2.0, dd);
            (4 * m, 8 * m)
        }),
        build: Rc::new(move |dd| {
            let mm = segments(2.0, dd) as usize;
            let ring = |i: usize| {
                let t = TAU * i as f64 / mm as f64;
                let (s, c) = sin_cos(t);
                let n = [c, 0.0, -s];
                let tan = [-s, 0.0, -c];
                let b = cross3(tan, n);
                let (ps, pc) = sin_cos(0.5 * t);
                corners
                    .iter()
                    .map(|q| {
                        let (u, v) = (q[0] * pc - q[1] * ps, q[0] * ps + q[1] * pc);
                        (add3([big * c, d, -big * s], add3(scale3(n, u), scale3(b, v))), None)
                    })
                    .collect()
            };
            let surfaces = vec![Surface { name: "edge".into(), smooth: true, radius: Some(big + d) }, Surface { name: "face".into(), smooth: true, radius: Some(big + d) }];
            let mut mesh = tube_mesh(mm, 4, true, 2, ring, |j| (j % 2) as u32, surfaces, 0);
            // An estimate: the quads' twist, and the band's sag round the loop at its outer reach.
            let step = PI / mm as f64;
            mesh.chord_error += (big + d) * (1.0 - cos(step)) + d * (1.0 - cos(0.5 * step));
            (mesh, None)
        }),
        exact,
        error: Rc::new(|_| None),
        genus: 1,
        check: Some(Rc::new(|m: &Mesh, d| tube_check(m, segments(2.0, d) as usize, 4, true, 2))),
    })))
}
fn mobius_anchors(a: &Args) -> Vec<Anchor> {
    let d = 0.5 * (a.f("width") * a.f("width") + a.f("thickness") * a.f("thickness")).sqrt();
    vec![at3("centre", [0.0, d, 0.0], On::Free), at3("band", [a.f("radius"), d, 0.0], On::Inside)]
}

// ---------------------------------------------------------------- compounds

/// The Steinmetz solid of two cylinders of radius `r`, along x and along z, through each other.
/// At the height `y` (from its middle) its section is the square of half side `√(r² − y²)`; so
/// it is made of rows of squares at the heights `r (1 − cos α)`, α evenly from 0 to π (the
/// cylinders' own angle), each row joined to the next by four planar trapezoids (each side of
/// the square stays on one cylinder, straight along that cylinder's axis), and fans to its two
/// tips. `m` rows: the segments of half a turn.
fn steinmetz_mesh(r: f64, m: usize) -> Mesh {
    let mut points = vec![[0.0, 0.0, 0.0]];
    for j in 1..m {
        let (s, c) = sin_cos(PI * j as f64 / m as f64);
        let (hs, y) = (r * s, r - r * c);
        points.extend([[hs, y, hs], [hs, y, -hs], [-hs, y, -hs], [-hs, y, hs]]);
    }
    points.push([0.0, 2.0 * r, 0.0]);
    let top = (points.len() - 1) as u32;
    let row = |j: usize, k: usize| (1 + 4 * (j - 1) + k % 4) as u32;
    // Side k joins corner k to corner k + 1: 0 is x = +s (on the cylinder along z), 1 z = −s, 2
    // x = −s, 3 z = +s (on the cylinder along x).
    let normal = |p: P3, k: usize| if k.is_multiple_of(2) { norm3([p[0], p[1] - r, 0.0]) } else { norm3([0.0, p[1] - r, p[2]]) };
    let mut faces = Vec::new();
    for k in 0..4 {
        let surface = (k % 2) as u32;
        faces.push((vec![0, row(1, k + 1), row(1, k)], surface, k));
        for j in 1..m - 1 {
            faces.push((vec![row(j, k), row(j, k + 1), row(j + 1, k + 1), row(j + 1, k)], surface, k));
        }
        faces.push((vec![row(m - 1, k), row(m - 1, k + 1), top], surface, k));
    }
    let faces = faces.into_iter().map(|(verts, surface, k)| crate::mesh::Face { normals: Some(verts.iter().map(|&v| normal(points[v as usize], k)).collect()), verts, surface }).collect();
    let surfaces = vec![Surface { name: "along_z".into(), smooth: true, radius: Some(r) }, Surface { name: "along_x".into(), smooth: true, radius: Some(r) }];
    Mesh { points, faces, surfaces, chord_error: r * (1.0 - cos(0.5 * PI / m as f64)) }
}
fn steinmetz_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    let (lo, hi) = round_bounds(r, 0.0, 2.0 * r);
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(|d| {
            let m = segments(0.5, d);
            (2 + 4 * (m - 1), 4 * m)
        }),
        build: Rc::new(move |d| (steinmetz_mesh(r, segments(0.5, d) as usize), None)),
        exact: m(Some(16.0 * r * r * r / 3.0), Some(16.0 * r * r), r, lo, hi),
        error: Rc::new(move |d| Some(r * (1.0 - cos(0.5 * PI / segments(0.5, d) as f64)))),
        genus: 0,
        check: None,
    })))
}
fn steinmetz_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    vec![out3("base", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Vertex), out3("top", [0.0, 2.0 * r, 0.0], [0.0, 1.0, 0.0], On::Vertex), at3("centre", [0.0, r, 0.0], On::Inside)]
}

const P_RARROW: &[Param] = &[len("shaft", "the shaft's radius", 0.05), len("length", "the shaft's length", 0.8), len("head", "the head's radius, over the shaft's", 0.12), len("head_length", "the head's length", 0.25)];
fn rarrow_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rs, l, rh, hl) = (a.f("shaft"), a.f("length"), a.f("head"), a.f("head_length"));
    if rh <= rs {
        return Err(refuse("head_over_shaft", "the head must be wider than the shaft", &[("shaft", rs), ("head", rh)]));
    }
    turned(vec![piece(line(O, [rs, 0.0]), "tail"), piece(line([rs, 0.0], [rs, l]), "shaft"), piece(line([rs, l], [rh, l]), "barb"), piece(line([rh, l], [0.0, l + hl]), "head")])
}
fn rarrow_anchors(a: &Args) -> Vec<Anchor> {
    let (l, hl) = (a.f("length"), a.f("head_length"));
    vec![out3("tail", [0.0, 0.0, 0.0], [0.0, -1.0, 0.0], On::Boundary), at3("tip", [0.0, l + hl, 0.0], On::Vertex)]
}

// ---------------------------------------------------------------- the table

const STANDS: &str = "the middle of its base, on y = 0";

pub(crate) const FORMS: &[Form] = &[
    solid!("sphere", Sphere, "a sphere of latitudes and longitudes (rings of points at equal angles from pole to pole)", "the south pole, on y = 0", P_SPHERE, curved: true, kin: &["circle"], sphere_geom, sphere_closed, sphere_anchors),
    solid!("icosphere", Sphere, "a geodesic sphere: an icosahedron's faces cut into triangles and pushed out to the sphere (points spread nearly evenly)", "its lowest point, on y = 0", P_SPHERE, curved: true, kin: &["circle"], icosphere_geom, sphere_closed, ball_anchors),
    solid!("cube_sphere", Sphere, "a cube's faces cut at equal angles and pushed out to the sphere (an equiangular cube-sphere)", "its lowest point, on y = 0", P_SPHERE, curved: true, kin: &["circle"], cube_sphere_geom, sphere_closed, ball_anchors),
    solid!("ellipsoid", Sphere, "an ellipsoid of three semi-axes (a sphere of latitudes and longitudes stretched)", "its lowest point, on y = 0", P_ELLIPSOID, curved: true, kin: &["ellipse"], ellipsoid_geom, ellipsoid_closed, ellipsoid_anchors),
    solid!("hemisphere", Sphere, "half a ball, flat side down", STANDS, P_SPHERE, curved: true, kin: &["semicircle"], hemisphere_geom, hemisphere_closed, hemisphere_anchors),
    solid!("spherical_cap", Sphere, "the part of a ball cut off by a plane, flat side down", STANDS, P_CAP, curved: true, kin: &["circular_segment"], cap_geom, cap_closed, cap_anchors),
    solid!("spherical_segment", Sphere, "a zone: the part of a ball between two parallel planes", STANDS, P_ZONE, curved: true, kin: &["circle"], zone_geom, zone_closed, zone_anchors),
    solid!("spherical_wedge", Sphere, "a wedge of a ball between two half planes through its axis (Y)", "the south pole, on y = 0", P_WEDGE, curved: true, kin: &["sector"], swedge_geom, swedge_closed, pole_anchors),
    solid!("cylinder", Cylinder, "a right circular cylinder", STANDS, P_CYL, curved: true, kin: &["circle"], cyl_geom, cyl_closed, cyl_anchors),
    solid!("elliptic_cylinder", Cylinder, "a right cylinder on an ellipse", STANDS, P_ECYL, curved: true, kin: &["ellipse"], ecyl_geom, ecyl_closed, ecyl_anchors),
    solid!("cylinder_sector", Cylinder, "a sector of a cylinder between two half planes through its axis (Y)", "the axis's foot, on y = 0", P_CSECT, curved: true, kin: &["sector", "semicircle"], csect_geom, csect_closed, csect_anchors),
    solid!("tube", Cylinder, "a hollow cylinder (a pipe; a washer when short)", "the axis's foot, on y = 0", P_TUBE, curved: true, kin: &["annulus"], tube_geom, tube_closed, tube_anchors),
    solid!("cone", Cylinder, "a right circular cone", STANDS, P_CYL, curved: true, kin: &["circle"], cone_geom, cone_closed, cone_anchors),
    solid!("frustum", Cylinder, "a truncated cone: a cone with its tip cut off", STANDS, P_FRUSTUM, curved: true, kin: &["trapezium"], frustum_geom, frustum_closed, frustum_anchors),
    solid!("bicone", Cylinder, "two cones base to base, standing on a tip", "the lower tip, on y = 0", P_CYL, curved: true, kin: &["rhombus"], bicone_geom, bicone_closed, bicone_anchors),
    solid!("capsule", Cylinder, "a cylinder with half balls on its ends (the solid pill)", "the lower tip, on y = 0", P_CAPSULE, curved: true, kin: &["stadium"], capsule_geom, capsule_closed, capsule_anchors),
    solid!("torus", Torus, "a ring torus: a circle turned about an axis in its plane", "its axis's foot, on y = 0", P_TORUS, curved: true, kin: &["annulus"], torus_geom, torus_closed, torus_anchors),
    solid!("torus_sector", Torus, "a bend of a torus between two half planes through its axis (a pipe elbow), capped", "its axis's foot, on y = 0", P_TSECT, curved: true, kin: &["annular_sector"], tsect_geom, tsect_closed, tsect_anchors),
    solid!("elliptic_torus", Torus, "a torus whose tube is an ellipse", "its axis's foot, on y = 0", P_ETORUS, curved: true, kin: &["ellipse"], etorus_geom, etorus_closed, etorus_anchors),
    solid!("biconvex_lens", Revolution, "a lens of two equal spherical caps, rim to rim", "the lower face's middle, on y = 0", P_LENS, curved: true, kin: &["lens"], lens_geom, lens_closed, lens_anchors),
    solid!("spindle", Revolution, "a spindle (a lemon): a circle's arc turned about its chord", "the lower tip, on y = 0", P_SPINDLE, curved: true, kin: &["lens"], spindle_geom, super::none, spindle_anchors),
    solid!("barrel", Revolution, "a barrel: flat ends, its side an arc bulging out", STANDS, P_BARREL, curved: true, kin: &["rectangle"], barrel_geom, super::none, barrel_anchors),
    solid!("hourglass", Revolution, "an hourglass: flat ends, its side an arc drawn in to a waist", STANDS, P_HOURGLASS, curved: true, kin: &["circle"], hourglass_geom, super::none, hourglass_anchors),
    solid!("egg_of_revolution", Revolution, "Moss's egg turned about its axis (arcs of circles)", "its lower tip, on y = 0", crate::forms::round::P_MOSS, curved: true, kin: &["moss_egg", "ovoid"], egg_geom, super::none, egg_anchors),
    solid!("teardrop_of_revolution", Revolution, "the plane teardrop turned about its axis: a ball drawn out to a point", "its round end's lowest point, on y = 0", crate::forms::round::P_DROP, curved: true, kin: &["teardrop"], tdrop_geom, super::none, tdrop_anchors),
    solid!("paraboloid", Revolution, "a paraboloid, its bowl filled and capped flat", "the vertex, on y = 0", P_PARAB, curved: true, kin: &["parabola"], parab_geom, parab_closed, parab_anchors),
    solid!("hyperboloid", Revolution, "a hyperboloid of one sheet, capped flat at both ends (a cooling tower)", STANDS, P_HYPER, curved: true, kin: &["circle"], hyper_geom, hyper_closed, hyper_anchors),
    solid!("superellipsoid", Rounded, "a superellipsoid, |x/a|^n + |y/b|^n + |z/c|^n = 1 (a rounded cube by its exponent): a cube-sphere's directions to its surface", "its lowest point, on y = 0", P_SUPERELL, curved: true, kin: &["superellipse"], superell_geom, super::none, superell_anchors),
    solid!("rounded_box", Rounded, "a box with edges and corners of a true radius (quarter cylinders and eighth balls)", STANDS, P_RBOX, curved: true, kin: &["rounded_rectangle"], rbox_geom, super::none, rbox_anchors),
    solid!("dome", Shell, "a hemispherical shell, rim down", "the middle of the rim's circle, on y = 0", P_SHELL, curved: true, kin: &["annular_sector"], dome_geom, dome_closed, dome_anchors),
    solid!("bowl", Shell, "a hemispherical shell, rim up", "its lowest point, on y = 0", P_SHELL, curved: true, kin: &["annular_sector"], bowl_geom, bowl_closed, bowl_anchors),
    solid!("helix", Sweep, "a round wire wound in a helix about Y (a spring), its ends cut square", "the helix's axis, its lowest point on y = 0", P_HELIX, curved: true, kin: &["annulus", "sine_wave"], helix_geom, super::none, helix_anchors),
    solid!("torus_knot", Sweep, "a round wire along the (p, q) torus knot: p times about the axis, q times through the hole", "the torus's centre, lifted so the wire rests on y = 0", P_KNOT, curved: true, kin: &["annulus"], knot_geom, super::none, knot_anchors),
    solid!("mobius_band", Sweep, "a Möbius band with a thickness: a rectangle swept round a circle turning half a turn", "its axis's foot, on y = 0", P_MOBIUS, curved: true, kin: &["rectangle"], mobius_geom, super::none, mobius_anchors),
    solid!("steinmetz_solid", Compound, "the bicylinder: where two cylinders of one radius, along x and along z, cross", "its lowest point, on y = 0", P_SPHERE, curved: true, kin: &["rectangle"], steinmetz_geom, super::none, steinmetz_anchors),
    solid!("round_arrow", Compound, "an arrow as one solid of revolution along +Y: a shaft and a conical head", "the tail's middle, on y = 0", P_RARROW, curved: false, kin: &["arrow"], rarrow_geom, super::none, rarrow_anchors),
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("ball", Solid, "sphere", "a ball", &[]),
    alias!("globe", Solid, "sphere", "a globe", &[]),
    alias!("uv_sphere", Solid, "sphere", "a sphere of latitudes and longitudes", &[]),
    alias!("geodesic_sphere", Solid, "icosphere", "a geodesic sphere", &[]),
    alias!("geosphere", Solid, "icosphere", "a geodesic sphere", &[]),
    alias!("spherified_cube", Solid, "cube_sphere", "a cube-sphere", &[]),
    alias!("spheroid", Solid, "ellipsoid", "an ellipsoid of revolution about Y", &[Set::Tie { name: "radius", about: "the equator's radius", targets: &["rx", "rz"] }]),
    alias!("oblate_spheroid", Solid, "ellipsoid", "a spheroid flattened at its poles (give ry under the radius)", &[Set::Tie { name: "radius", about: "the equator's radius", targets: &["rx", "rz"] }]),
    alias!("prolate_spheroid", Solid, "ellipsoid", "a spheroid drawn out at its poles (give ry over the radius)", &[Set::Tie { name: "radius", about: "the equator's radius", targets: &["rx", "rz"] }]),
    alias!("half_sphere", Solid, "hemisphere", "a hemisphere", &[]),
    alias!("zone", Solid, "spherical_segment", "a spherical zone", &[]),
    alias!("spherical_lune", Solid, "spherical_wedge", "a spherical wedge", &[]),
    alias!("disc", Solid, "cylinder", "a short cylinder", &[]),
    alias!("disk", Solid, "cylinder", "a short cylinder", &[]),
    alias!("coin", Solid, "cylinder", "a short cylinder", &[]),
    alias!("rod", Solid, "cylinder", "a long thin cylinder", &[]),
    alias!("half_cylinder", Solid, "cylinder_sector", "half a cylinder, cut through its axis", &[Set::Fixed("angle", 180.0)]),
    alias!("pipe", Solid, "tube", "a tube", &[]),
    alias!("hollow_cylinder", Solid, "tube", "a tube", &[]),
    alias!("washer", Solid, "tube", "a short tube", &[]),
    alias!("truncated_cone", Solid, "frustum", "a frustum of a cone", &[]),
    alias!("conical_frustum", Solid, "frustum", "a frustum of a cone", &[]),
    alias!("double_cone", Solid, "bicone", "a bicone", &[]),
    alias!("pill", Solid, "capsule", "a capsule", &[]),
    alias!("spherocylinder", Solid, "capsule", "a capsule", &[]),
    alias!("donut", Solid, "torus", "a torus (American spelling)", &[]),
    alias!("doughnut", Solid, "torus", "a torus (British spelling)", &[]),
    alias!("elbow", Solid, "torus_sector", "a pipe elbow", &[]),
    alias!("pipe_elbow", Solid, "torus_sector", "a pipe elbow", &[]),
    alias!("lemon", Solid, "spindle", "a spindle", &[]),
    alias!("egg", Solid, "egg_of_revolution", "an egg (Moss's construction, turned)", &[]),
    alias!("drop", Solid, "teardrop_of_revolution", "a drop", &[]),
    alias!("raindrop", Solid, "teardrop_of_revolution", "a drop", &[]),
    alias!("cooling_tower", Solid, "hyperboloid", "a hyperboloid of one sheet", &[]),
    alias!("rounded_cube", Solid, "superellipsoid", "a superellipsoid of equal semi-axes (rounder or squarer by its exponent)", &[Set::Tie { name: "radius", about: "centre to each face's middle", targets: &["rx", "ry", "rz"] }]),
    alias!("rounded_cuboid", Solid, "rounded_box", "a rounded box", &[]),
    alias!("spring", Solid, "helix", "a coil spring", &[]),
    alias!("coil", Solid, "helix", "a coil", &[]),
    alias!("trefoil_knot", Solid, "torus_knot", "the trefoil knot (2, 3)", &[Set::Fixed("p", 2.0), Set::Fixed("q", 3.0)]),
    alias!("cinquefoil_knot", Solid, "torus_knot", "the cinquefoil knot (2, 5)", &[Set::Fixed("p", 2.0), Set::Fixed("q", 5.0)]),
    alias!("moebius_band", Solid, "mobius_band", "a Möbius band", &[]),
    alias!("mobius_strip", Solid, "mobius_band", "a Möbius band", &[]),
    alias!("bicylinder", Solid, "steinmetz_solid", "the Steinmetz bicylinder", &[]),
    alias!("arrow_solid", Solid, "round_arrow", "a round arrow", &[]),
];
