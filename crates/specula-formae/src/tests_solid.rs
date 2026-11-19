//! The solids' table-driven tests: the brief's 3 (well made), 4 (closed forms against the
//! build), 5 (the regular families by what defines them), 7 (anchors) and 8 (determinism) for
//! every solid of the table, by being in it.

use crate::forms::flat::{units, CATALAN, REGULAR};
use crate::maths::*;
use crate::mesh::Mesh;
use crate::solid::gauss;
use crate::table::{Found, Kind};
use crate::tests::{cases, cases_together, value, words};
use crate::{Built, Error, Spec, DETAIL_DEFAULT, DETAIL_MAX, DETAIL_MIN, EDGE_FLOOR, MAX_POINTS};

const EPS: f64 = f64::EPSILON;

fn solids() -> Vec<Found> {
    words().into_iter().filter(|w| w.form.kind == Kind::Solid).collect()
}

fn mesh_of(b: &Built) -> &Mesh {
    match b {
        Built::Mesh(m) => m,
        _ => panic!("a solid built something else"),
    }
}

/// The reach about the bounds' middle: what rounding of a volume or an area scales with.
fn reach_about_middle(m: &Mesh) -> f64 {
    let (lo, hi) = m.bounds().unwrap();
    0.5 * dist3(lo, hi)
}

/// The triangles a measure sums over (each face a fan).
fn tri_count(m: &Mesh) -> f64 {
    m.faces.iter().map(|f| f.verts.len() - 2).sum::<usize>() as f64
}

/// What rounding allows a volume and an area: each fan triangle's term rounds by a few ε of its
/// size (`R³` for a volume, `R²` for an area); 16 of them, summed over the triangles.
fn rounding(m: &Mesh) -> (f64, f64) {
    let r = reach_about_middle(m);
    let t = tri_count(m);
    (16.0 * t * EPS * r * r * r, 16.0 * t * EPS * r * r)
}

/// The total length of the mesh's edges.
fn edge_length(m: &Mesh) -> f64 {
    let mut l = 0.0;
    for f in &m.faces {
        let n = f.verts.len();
        for i in 0..n {
            l += dist3(m.points[f.verts[i] as usize], m.points[f.verts[(i + 1) % n] as usize]);
        }
    }
    0.5 * l
}

/// The greatest turn between neighbouring faces of any smooth surface, radians.
fn turn(m: &Mesh) -> f64 {
    m.rounds().iter().map(|r| rad(r.turn_deg)).fold(0.0, f64::max)
}

/// How far a curved solid's built volume and area may be from the true ones, derived from its
/// error bound `h`:
/// - the volume: the true and built solids differ only within `h` of the true surface, a shell
///   of volume at most `2 h A` (`A` the area; as the plane figures' `2 h P`);
/// - the area: a face of a smooth surface stands for a patch whose normals lean from the face's
///   by at most the turn `t` to its neighbours, so the patch is `1 / cos t` of the face's area
///   at most (and no less than it); and each face's edge lies within `h` of where the true
///   patch's edge projects, which moves an area of at most `h` times the face's perimeter: in
///   all `A (1 / cos t − 1) + 2 h L`, `L` the length of every edge.
fn allowed(m: &Mesh) -> (f64, f64) {
    let (rv, ra) = rounding(m);
    let a = m.measures().boundary.unwrap();
    let h = m.chord_error;
    let t = turn(m).min(1.5);
    (2.0 * h * a + rv, a * (1.0 / cos(t) - 1.0) + 2.0 * h * edge_length(m) + ra)
}

/// A refusal of a solid's build at `d` is sound: a form's own rule passes; `not_well_made`
/// never; `edge_under_floor` names an edge that is that long (a face's corner, in the build as
/// made) and under the floor, and a coarser detail it names builds; `crosses_at_this_detail`
/// names two faces the build's own search finds crossing (or a section's two edges), and a
/// finer detail it names builds; where none is named, the last detail tried does not build
/// either, or the search reached the end of the range or the ceiling.
pub(crate) fn sound_solid_refusal(what: &str, s: &Spec, d: u32, e: &Error) {
    let Error::Constraint { rule, values, .. } = e else { panic!("{what}: {e}") };
    let v = |n: &str| value(values, n).unwrap_or_else(|| panic!("{what}: {rule} carries no {n}: {values:?}"));
    let raw = || match s.raw(d) {
        Some(Built::Mesh(m)) => m,
        _ => panic!("{what}: refused {rule} and its geometry is refused too"),
    };
    let holds = |h: f64| s.build(h as u32).unwrap_or_else(|e| panic!("{what}: the detail {h} it names does not hold: {e}"));
    match *rule {
        "not_well_made" => panic!("{what}: {e}"),
        "edge_under_floor" => {
            assert_eq!(v("detail"), d as f64, "{what}");
            // (An edge, or a face's width: its area over its longest edge.)
            let small = value(values, "edge").or(value(values, "width")).unwrap_or_else(|| panic!("{what}: names no edge and no width: {values:?}"));
            assert!(small <= v("floor") && v("floor") == EDGE_FLOOR * v("size"), "{what}: {values:?}");
            if let (Some(face), Some(w)) = (value(values, "face"), value(values, "width")) {
                let m = raw();
                let f = &m.faces[face as usize];
                let n = f.verts.len();
                let longest = (0..n).map(|i| dist3(m.points[f.verts[i] as usize], m.points[f.verts[(i + 1) % n] as usize])).fold(0.0, f64::max);
                assert_eq!(len3(m.area_vector(f)) / longest, w, "{what}: the face it names is not that thin");
            } else if let (Some(face), Some(corner)) = (value(values, "face"), value(values, "corner")) {
                let m = raw();
                let f = &m.faces[face as usize];
                let n = f.verts.len();
                let (a, b) = (f.verts[corner as usize], f.verts[(corner as usize + 1) % n]);
                assert_eq!(dist3(m.points[a as usize], m.points[b as usize]), v("edge"), "{what}: the edge it names is not that long");
            } else {
                v("point");
            }
            match value(values, "coarser_detail_that_holds") {
                Some(h) => {
                    assert!(h < d as f64 && h >= DETAIL_MIN as f64, "{what}");
                    holds(h);
                }
                None => {
                    let last = v("searched_to") as u32;
                    assert!(last <= d && (last == DETAIL_MIN || s.build(last).is_err()), "{what}: names no coarser detail, and {last} holds");
                }
            }
        }
        "crosses_at_this_detail" => {
            assert_eq!(v("detail"), d as f64, "{what}");
            if let (Some(a), Some(b)) = (value(values, "face_a"), value(values, "face_b")) {
                assert_eq!(s.solid_fault(d), Some(crate::SolidFault::Crossing { a: a as usize, b: b as usize }), "{what}: the faces it names are not what its check finds");
            } else {
                v("ring_a");
            }
            match value(values, "finer_detail_that_holds") {
                Some(h) => {
                    assert!(h > d as f64 && h <= DETAIL_MAX as f64, "{what}");
                    holds(h);
                }
                None => {
                    let last = v("searched_to") as u32;
                    let ended = last == DETAIL_MAX || s.count((2 * last).min(DETAIL_MAX)).map(|c| c.points > MAX_POINTS).unwrap_or(false);
                    assert!(last >= d && (ended || s.build(last).is_err()), "{what}: names no finer detail, and {last} holds");
                }
            }
        }
        _ => {}
    }
}

/// What every built solid must be, whatever its arguments: the count it gave; well made by the
/// mesh's own check for its genus (closed, wound outward, planar faces, no edge or face under
/// the floor, Euler's formula); no two faces crossing (searched where the mesh is not too large
/// for a test: up to 10,000 faces); every normal given a unit vector on the outer side of its
/// face; every smooth surface with a radius has a finite turn; and its volume and area within
/// what its error allows of the closed forms ([`allowed`]; exact to rounding for flat faces).
fn check_built(what: &str, s: &Spec, d: u32, b: &Built) {
    let m = mesh_of(b);
    let c = s.count(d).unwrap();
    assert_eq!((c.points, c.parts), (b.points(), b.parts()), "{what}: the count before building");
    let genus = s.genus().unwrap();
    if let Err(f) = m.validate(genus) {
        panic!("{what}: built and not well made: {f:?}");
    }
    if m.faces.len() <= 10_000 {
        assert_eq!(m.self_crossing(), None, "{what}: two faces cross");
    }
    for (fi, f) in m.faces.iter().enumerate() {
        if let Some(ns) = &f.normals {
            assert_eq!(ns.len(), f.verts.len(), "{what}: face {fi}");
            assert!(m.surfaces[f.surface as usize].smooth, "{what}: a flat face {fi} has normals");
            let fnrm = norm3(m.area_vector(f));
            for n in ns {
                assert!((len3(*n) - 1.0).abs() < 1e-12, "{what}: face {fi}: a normal is not a unit vector");
                assert!(dot3(*n, fnrm) > 0.0, "{what}: face {fi}: a normal {n:?} points in (the face's is {fnrm:?})");
            }
        }
    }
    for r in m.rounds() {
        assert!(r.turn_deg.is_finite() && r.radius > 0.0, "{what}: {r:?}");
    }
    let closed = s.closed().unwrap_or_else(|e| panic!("{what}: built, and its closed forms refused: {e}"));
    let got = m.measures();
    let (av, aa) = allowed(m);
    if let (Some(cv), Some(x)) = (closed.size, got.size) {
        assert!((cv - x).abs() <= av, "{what}: volume closed {cv}, built {x} (allowed {av})");
    }
    if let (Some(ca), Some(x)) = (closed.boundary, got.boundary) {
        assert!((ca - x).abs() <= aa, "{what}: area closed {ca}, built {x} (allowed {aa})");
    }
}

/// Test 3: every solid word, with each parameter at its least and its greatest (at the least,
/// the default and a fine detail), with its parameters moved together (every pair at its
/// corners, all least, all greatest and three drawn sets, at the least and the default detail),
/// and at its defaults at the greatest detail too: built and [`check_built`], or refused
/// soundly.
#[test]
fn t3_every_solid_is_well_made() {
    let (mut built, mut refused) = (0usize, 0usize);
    let ws = solids();
    assert!(!ws.is_empty());
    for w in ws {
        let mut all = cases(&w);
        all.extend(cases_together(&w, 3));
        for (label, given) in all {
            let what = format!("{} {label}", w.name());
            let s = Spec::of(w, &given).unwrap_or_else(|e| panic!("{what}: {e}"));
            // (The parameters moved together at the least and the default detail; one at a time at
            // a fine one too, and the defaults at the greatest.)
            let details: &[u32] = if label == "defaults" { &[DETAIL_MIN, DETAIL_DEFAULT, 256, DETAIL_MAX] } else if label.contains('=') && !label.contains(' ') { &[DETAIL_MIN, DETAIL_DEFAULT, 256] } else { &[DETAIL_MIN, DETAIL_DEFAULT] };
            for &d in details {
                let what = format!("{what} detail {d}");
                match s.build(d) {
                    Ok(b) => {
                        built += 1;
                        check_built(&what, &s, d, &b);
                    }
                    Err(Error::TooManyPoints { points, ceiling, .. }) => {
                        assert!(points > ceiling, "{what}");
                        refused += 1;
                    }
                    Err(e) if label != "defaults" || d == DETAIL_MAX => {
                        sound_solid_refusal(&what, &s, d, &e);
                        refused += 1;
                    }
                    Err(e) => panic!("{what}: its defaults refused: {e}"),
                }
            }
        }
    }
    assert!(built > 500, "{built} built, {refused} refused");
}

/// Test 4 for solids: every closed form against the shape as built, one parameter at a time at
/// its extremes and at the defaults.
/// - A typed textbook formula and the geometry's exact measure, where both exist, agree to
///   rounding (`1e-9` of the reach to the power of the measure, as the plane figures').
/// - A solid of flat faces: its build is its geometry, so its volume and area equal the
///   closed forms to rounding.
/// - A curved solid: within [`allowed`] at details 128 and 256, and at its defaults the errors
///   fall by 3 to 5 times from one to the other (the sag goes with the square of a facet's
///   size).
/// - The centroid: moving at most `2 h A` of volume by at most the solid's size `D` moves it by
///   at most `2 (2 h A) D / V`.
/// - The bounds: what is built lies within `2 h` of the closed bounds and reaches them to `2 h`.
#[test]
fn t4_solids_closed_forms_hold_against_the_build() {
    let mut checked = 0;
    for w in solids() {
        for (label, given) in cases(&w) {
            let s = Spec::of(w, &given).unwrap();
            let what = format!("{} {label}", w.name());
            let Ok(closed) = s.closed() else { continue };
            let curved = s.geom_curved();
            let (d1, d2) = if curved { (128, 256) } else { (DETAIL_DEFAULT, DETAIL_DEFAULT) };
            let (Ok(b1), Ok(b2)) = (s.build(d1), s.build(d2)) else { continue };
            let (m1, m2) = (mesh_of(&b1), mesh_of(&b2));
            let (g1, g2) = (m1.measures(), m2.measures());
            let r = reach_about_middle(m2);
            let (rv, ra) = rounding(m2);
            let typed = s.typed();
            if let (Some(t), Some(c)) = (typed.size, closed.size) {
                assert!((t - c).abs() <= 1e-9 * r * r * r + rv, "{what}: typed volume {t} against the geometry's {c}");
                checked += 1;
            }
            if let (Some(t), Some(c)) = (typed.boundary, closed.boundary) {
                assert!((t - c).abs() <= 1e-9 * r * r + ra, "{what}: typed area {t} against the geometry's {c}");
                checked += 1;
            }
            for (name, c, x1, x2, round, i) in [("volume", closed.size, g1.size, g2.size, rv, 0), ("area", closed.boundary, g1.boundary, g2.boundary, ra, 1)] {
                let (Some(c), Some(x1), Some(x2)) = (c, x1, x2) else { continue };
                checked += 1;
                let (e1, e2) = ((c - x1).abs(), (c - x2).abs());
                if !curved {
                    assert!(e2 <= round, "{what}: {name} closed {c}, built {x2}");
                    continue;
                }
                let lim = if i == 0 { allowed(m2).0 } else { allowed(m2).1 };
                assert!(e2 <= lim, "{what}: {name} closed {c}, built {x2}: off by {e2}, allowed {lim}");
                if label == "defaults" && e1 > 1000.0 * round {
                    let k = e1 / e2;
                    assert!((3.0..=5.0).contains(&k), "{what}: the {name}'s error fell by {k} from detail {d1} to {d2} ({e1} to {e2})");
                }
            }
            let h = m2.chord_error;
            if let (Some(c), Some(x), Some(v)) = (closed.centroid, g2.centroid, closed.size) {
                let dv = 2.0 * h * g2.boundary.unwrap() + rv;
                let tol = 2.0 * dv * m2.size() / v.abs() + 1e-12 * r;
                assert!(dist3(c, x) <= tol, "{what}: centroid closed {c:?}, built {x:?} (allowed {tol})");
                checked += 1;
            }
            if let (Some((lo, hi)), Some((blo, bhi))) = (closed.bounds, g2.bounds) {
                let t = 2.0 * h + 1e-12 * r;
                for k in 0..3 {
                    assert!(blo[k] >= lo[k] - t && bhi[k] <= hi[k] + t, "{what}: built bounds {blo:?}..{bhi:?} outside {lo:?}..{hi:?}");
                    assert!(blo[k] <= lo[k] + t && bhi[k] >= hi[k] - t, "{what}: built bounds {blo:?}..{bhi:?} short of {lo:?}..{hi:?}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} checks");
}

/// Test 5: the regular families by what defines them. Each Platonic and Archimedean solid has
/// the counts of points, edges and faces of the literature (and Euler's formula holds of
/// them), its faces of each number of sides as many as the literature says, every edge the
/// same length, every point on one sphere, and every point on or behind every face's plane
/// (convex). Each Catalan solid has its Archimedean solid's counts swapped (faces for points),
/// every face congruent (equal areas and equal sorted edge lengths) and every face tangent to
/// one sphere, and is convex. Tolerances are rounding of a few operations on numbers of size 1
/// (`1e-12`).
#[test]
fn t5_regular_families_are_what_defines_them() {
    let (polys, radii, cats) = units();
    let convex = |what: &str, m: &Mesh| {
        for (fi, f) in m.faces.iter().enumerate() {
            let n = norm3(m.area_vector(f));
            let p0 = m.points[f.verts[0] as usize];
            for (pi, p) in m.points.iter().enumerate() {
                assert!(dot3(n, sub3(*p, p0)) <= 1e-12, "{what}: point {pi} is in front of face {fi}");
            }
        }
    };
    let edges = |m: &Mesh| -> Vec<f64> {
        let mut v = Vec::new();
        for f in &m.faces {
            let n = f.verts.len();
            for i in 0..n {
                let (a, b) = (f.verts[i], f.verts[(i + 1) % n]);
                if a < b {
                    v.push(dist3(m.points[a as usize], m.points[b as usize]));
                }
            }
        }
        v
    };
    for (i, r) in REGULAR.iter().enumerate() {
        let m = polys[i].mesh();
        let what = r.name;
        let es = edges(&m);
        assert_eq!((m.points.len(), es.len(), m.faces.len()), r.vef, "{what}: points, edges, faces");
        assert_eq!(r.vef.0 as i64 - r.vef.1 as i64 + r.vef.2 as i64, 2, "{what}: the literature's counts break Euler's formula");
        for &(sides, count) in r.faces {
            assert_eq!(m.faces.iter().filter(|f| f.verts.len() == sides).count(), count, "{what}: faces of {sides} sides");
        }
        for e in &es {
            assert!((e - 1.0).abs() <= 1e-12, "{what}: an edge is {e}");
        }
        for p in &m.points {
            assert!((len3(*p) - radii[i]).abs() <= 1e-12, "{what}: a point at {} from the centre, not {}", len3(*p), radii[i]);
        }
        convex(what, &m);
        m.validate(0).unwrap_or_else(|f| panic!("{what}: {f:?}"));
    }
    for (k, &(name, i)) in CATALAN.iter().enumerate() {
        let m = cats[k].mesh();
        let (v, e, f) = REGULAR[i].vef;
        assert_eq!((m.points.len(), edges(&m).len(), m.faces.len()), (f, e, v), "{name}: points, edges, faces");
        let area = |f: &crate::mesh::Face| len3(m.area_vector(f));
        let sorted = |f: &crate::mesh::Face| {
            let n = f.verts.len();
            let mut l: Vec<f64> = (0..n).map(|j| dist3(m.points[f.verts[j] as usize], m.points[f.verts[(j + 1) % n] as usize])).collect();
            l.sort_by(f64::total_cmp);
            l
        };
        let (a0, l0) = (area(&m.faces[0]), sorted(&m.faces[0]));
        for (fi, f) in m.faces.iter().enumerate() {
            assert!((area(f) - a0).abs() <= 1e-12 * a0, "{name}: face {fi}'s area {} is not {a0}", area(f));
            let l = sorted(f);
            assert_eq!(l.len(), l0.len(), "{name}: face {fi}");
            for (x, y) in l.iter().zip(&l0) {
                assert!((x - y).abs() <= 1e-12, "{name}: face {fi}'s edges {l:?} are not {l0:?}");
            }
            let n = norm3(m.area_vector(f));
            let d = dot3(n, m.points[f.verts[0] as usize]);
            assert!((d - 1.0).abs() <= 1e-12, "{name}: face {fi} is {d} from the centre, not 1");
        }
        convex(name, &m);
        m.validate(0).unwrap_or_else(|f| panic!("{name}: {f:?}"));
    }
}

/// Test 7 for solids: anchors lie where they say. A vertex is a point of the build; a boundary
/// anchor is within twice the build's error (and the floor) of its surface; an inside one is
/// inside (by the winding number); a direction is a unit vector, and at the defaults a small
/// step along it leaves the solid while a step against it enters.
#[test]
fn t7_solid_anchors_lie_where_they_say() {
    let mut checked = 0;
    for w in solids() {
        for (label, given) in cases(&w) {
            let s = Spec::of(w, &given).unwrap();
            let Ok(b) = s.build(DETAIL_DEFAULT) else { continue };
            let m = mesh_of(&b);
            if m.faces.len() > 20_000 {
                continue;
            }
            let size = m.size();
            let h = 2.0 * m.chord_error + 1e-9 * size;
            for a in s.anchors().unwrap() {
                let what = format!("{} {label}: anchor {}", w.name(), a.name);
                let p = a.at;
                assert!(p.iter().all(|x| x.is_finite()), "{what}");
                if let Some(d) = a.dir {
                    assert!((len3(d) - 1.0).abs() < 1e-12, "{what}: not a unit direction");
                }
                match a.on {
                    crate::On::Free => {}
                    crate::On::Vertex => assert!(m.points.iter().any(|q| dist3(*q, p) <= 1e-9 * size), "{what}: no point at {p:?}"),
                    crate::On::Boundary => assert!(m.distance(p) <= h, "{what}: {} from the surface", m.distance(p)),
                    crate::On::Inside => assert!(m.contains(p), "{what}: not inside"),
                }
                if let (true, Some(d), crate::On::Vertex | crate::On::Boundary) = (label == "defaults", a.dir, a.on) {
                    let e = 1e-6 * size + 2.0 * m.distance(p);
                    assert!(!m.contains(add3(p, scale3(d, e))), "{what}: a step out along its direction is still inside");
                    assert!(m.contains(sub3(p, scale3(d, e))), "{what}: a step in against its direction is not inside");
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 200, "only {checked} anchors");
}

/// Test 8 for solids: the bytes of a few builds (every coordinate's bits and every face's
/// points, in order) hash to a pinned value.
#[test]
fn t8_solids_are_pinned() {
    let mut bytes = Vec::new();
    for (name, d) in [("snub_dodecahedron", 8), ("pentagonal_hexecontahedron", 8), ("trapezohedron", 8), ("small_stellated_dodecahedron", 8), ("sphere", 33), ("torus_knot", 16), ("rounded_box", 16), ("superellipsoid", 16), ("mobius_band", 8), ("egg_of_revolution", 24), ("paraboloid", 24)] {
        let b = Spec::new(name, Some(Kind::Solid), &[]).unwrap().build(d).unwrap();
        let m = mesh_of(&b);
        for p in &m.points {
            for x in p {
                bytes.extend_from_slice(&x.to_bits().to_le_bytes());
            }
        }
        for f in &m.faces {
            for v in &f.verts {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    let h = specula_core::hash::hash_bytes(&bytes);
    assert_eq!(h, PINNED, "the pinned builds moved: hash {h:#018x}");
}
const PINNED: u64 = 0xa6f9_09c6_ca30_fb04;

/// The 8-point Gauss–Legendre rule integrates `x^k` on [−1, 1] exactly for `k` up to 15, and its
/// weights sum to 2.
#[test]
fn gauss_rule_is_exact_to_degree_fifteen() {
    for k in 0..16 {
        let got: f64 = gauss().iter().map(|&(x, w)| w * (0..k).fold(1.0, |p, _| p * x)).sum();
        let want = if k % 2 == 1 { 0.0 } else { 2.0 / (k + 1) as f64 };
        assert!((got - want).abs() <= 8.0 * EPS, "x^{k}: {got}, not {want}");
    }
}


/// The snub solids made as an orbit (the snub dodecahedron is made so) are the snub solids:
/// the snub cube made that way has the counts, all edges equal, and the same circumradius at
/// edge 1 as the snub cube from its coordinates (the tribonacci constant's), which test 5
/// holds; the ratio of circumradius to edge, with the counts, is what tells one snub from
/// another.
#[test]
fn snub_orbit_makes_the_snub_cube() {
    let p = crate::solid::Poly::by_edges(crate::forms::flat::snub_cube_orbit());
    let m = p.mesh();
    let e: usize = m.faces.iter().map(|f| f.verts.len()).sum::<usize>() / 2;
    assert_eq!((m.points.len(), e, m.faces.len()), (24, 60, 38));
    let f = &m.faces[0];
    let edge = dist3(m.points[f.verts[0] as usize], m.points[f.verts[1] as usize]);
    let radius = len3(m.points[0]) / edge;
    assert!((radius - units().1[11]).abs() <= 1e-12, "{radius} against {}", units().1[11]);
}

/// The operators, on every word of a plane figure with one ring, at its defaults and at a
/// coarse and a default detail: `prism_of` builds well made (test 3's checks: closed, outward,
/// planar, Euler's formula, no faces crossing) with the volume of its figure's closed area times
/// the height, within the figure's own chord bound times the height (`2 h P` the area's, as the
/// plane figures' tests allow); `pyramid_of` builds well made, or is refused because its apex
/// does not see an edge, and then that edge truly turns the wrong way about the origin;
/// `revolve_of`, moved out to clear the axis by half the figure's width, builds well made as a
/// ring (genus 1) and through 90 degrees as a capped bend (genus 0), with its volume within
/// [`allowed`] of the closed form where the figure is lines and arcs. A figure with holes is
/// refused by all three as `one_ring_only`.
#[test]
fn operators_raise_and_turn_every_one_ring_figure() {
    use crate::ops::{prism_of, pyramid_of, revolve_of};
    let check = |what: &str, m: &Mesh, genus: u32| {
        m.validate(genus).unwrap_or_else(|f| panic!("{what}: {f:?}"));
        if m.faces.len() <= 10_000 {
            assert_eq!(m.self_crossing(), None, "{what}: two faces cross");
        }
    };
    let (mut built, mut apex) = (0, 0);
    for w in words().into_iter().filter(|w| w.form.kind == Kind::Plane) {
        let s = Spec::of(w, &[]).unwrap();
        let Ok(crate::Built::Outline(o)) = s.build(DETAIL_DEFAULT) else { continue };
        if o.rings.len() > 1 {
            for e in [prism_of(&s, 1.0).err(), pyramid_of(&s, 1.0).err(), revolve_of(&s, 1.0, 360.0).err()] {
                assert!(matches!(e, Some(Error::Constraint { rule: "one_ring_only", .. })), "{}: {e:?}", w.name());
            }
            continue;
        }
        let closed = s.closed().unwrap();
        let (lo, hi) = closed.bounds.unwrap_or(o.measures().bounds.unwrap());
        let width = hi[0] - lo[0];
        let h = width.max(hi[1] - lo[1]);
        for d in [DETAIL_MIN, DETAIL_DEFAULT] {
            let what = format!("{} detail {d}", w.name());
            let plane = s.build(d).unwrap();
            let (pa, pp) = (plane.measures().size.unwrap(), plane.measures().boundary.unwrap());
            let p = prism_of(&s, h).unwrap();
            let m = p.build(d).unwrap_or_else(|e| panic!("{what}: prism_of: {e}"));
            check(&format!("{what} prism"), &m, 0);
            assert_eq!(p.count(d).unwrap().points, m.points.len() as u64, "{what}");
            let got = m.measures().size.unwrap();
            assert!((got - pa * h).abs() <= rounding(&m).0, "{what}: the prism's volume {got} is not its built figure's area times its height {}", pa * h);
            if let Some(c) = p.closed().size {
                assert!((c - got).abs() <= 2.0 * plane.chord_error() * pp * h + rounding(&m).0 + 1e-9 * c.abs(), "{what}: prism volume closed {c}, built {got}");
            }
            match pyramid_of(&s, h).unwrap().build(d) {
                Ok(m) => {
                    check(&format!("{what} pyramid"), &m, 0);
                    built += 1;
                }
                Err(Error::Constraint { rule: "apex_not_over_the_figure", values, .. }) => {
                    let Some(crate::Built::Outline(o)) = s.build(d).ok() else { panic!() };
                    let r = &o.rings[0];
                    let i = value(&values, "edge").unwrap() as usize;
                    let (a, b) = (r[i], r[(i + 1) % r.len()]);
                    assert!(a[0] * b[1] - a[1] * b[0] <= 0.0, "{what}: refused for edge {i}, which turns the right way");
                    apex += 1;
                }
                Err(e) => panic!("{what}: pyramid_of: {e}"),
            }
            for (angle, genus) in [(360.0, 1), (90.0, 0)] {
                let r = match revolve_of(&s, -lo[0] + 0.5 * width, angle) {
                    Ok(r) => r,
                    // (A figure without bounds in closed form cannot be shown clear of the axis.)
                    Err(Error::Constraint { rule: "bounds_unknown", .. }) => continue,
                    Err(e) => panic!("{what}: revolve_of {angle}: {e}"),
                };
                let m = r.build(d).unwrap_or_else(|e| panic!("{what}: revolve_of {angle}: {e}"));
                check(&format!("{what} revolved {angle}"), &m, genus);
                assert_eq!(r.genus(), genus, "{what}");
                if let Some(c) = r.closed().size {
                    let got = m.measures().size.unwrap();
                    assert!((c - got).abs() <= allowed(&m).0, "{what}: revolved volume closed {c}, built {got}");
                }
                built += 1;
            }
        }
    }
    assert!(built > 100 && apex > 0, "{built} built, {apex} pyramids refused for their apex");
}

/// The needle superellipsoid (rx 1e-4, exponent 32, the rest at their defaults) at detail 64, which
/// test 3 once reported crossing (before the crossing predicate counted orientations within their
/// rounding as touching: most likely that report was rounding): it is refused as crossing at this
/// detail, soundly, or built with no two faces crossing, searched in full (test 3's own search
/// stops at 10,000 faces, under this build's 24,576).
#[test]
fn the_needle_superellipsoid_is_refused_or_simple() {
    let s = Spec::new("superellipsoid", Some(Kind::Solid), &[("rx", 1e-4), ("exponent", 32.0)]).unwrap();
    match s.build(64) {
        Ok(b) => assert_eq!(mesh_of(&b).self_crossing(), None, "built with two faces crossing"),
        Err(e @ Error::Constraint { rule: "crosses_at_this_detail", .. }) => sound_solid_refusal("the needle superellipsoid", &s, 64, &e),
        Err(e) => panic!("{e}"),
    }
}
