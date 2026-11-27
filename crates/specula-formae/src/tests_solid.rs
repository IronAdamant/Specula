//! The solids' table-driven tests: the brief's 3 (well made), 4 (closed forms against the
//! build), 5 (the regular families by what defines them), 7 (anchors) and 8 (determinism) for
//! every solid of the table, by being in it.

use crate::forms::flat::{units, CATALAN, REGULAR};
use crate::maths::*;
use crate::mesh::Mesh;
use crate::solid::gauss;
use crate::table::{Found, Kind};
use crate::tests::{cases, cases_together, part, value, words, PARTS};
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
            let small = value(values, "edge").or(value(values, "width")).or(value(values, "gap")).unwrap_or_else(|| panic!("{what}: names no edge, no width and no gap: {values:?}"));
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
            } else if let Some(gap) = value(values, "gap") {
                // Two edges of the section nearer than the floor: the build's own check finds them.
                let (a, b) = ((v("ring_a") as usize, v("edge_a") as usize), (v("ring_b") as usize, v("edge_b") as usize));
                assert_eq!(s.solid_fault(d), Some(crate::SolidFault::SectionGap { a, b, gap }), "{what}: the edges it names are not what its check finds");
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
        "normal_behind_face" => {
            // The corner it names has its normal on its face's inner side in the build as made,
            // and a finer detail it names builds (or none does to the end of the range).
            assert_eq!(v("detail"), d as f64, "{what}");
            let m = raw();
            let f = &m.faces[v("face") as usize];
            let n = f.normals.as_ref().unwrap_or_else(|| panic!("{what}: the face it names gives no normals"))[v("corner") as usize];
            assert!(dot3(n, norm3(m.area_vector(f))) <= 0.0, "{what}: the corner it names has its normal on the outer side");
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

/// Test 3: every solid word, with each parameter at its least and its greatest, with its
/// parameters moved together (every pair at its corners, all least, all greatest and drawn
/// sets), and at its defaults: built and [`check_built`], or refused soundly. The default run
/// builds the defaults at the least, an odd, the default and a fine detail, each parameter's
/// extremes at the least and the default, and the parameters moved together (one drawn set) at
/// the least and an odd one; [`t3_full`] builds each of those at more details (the greatest for
/// the defaults, a fine one for the extremes, the default and odd ones for the parameters moved
/// together) and three drawn sets. Every build is held to the same checks in both.
#[test]
fn t3_every_solid_is_well_made() {
    t3(false, (0, PARTS));
}

/// Test 3 in full (see [`t3_every_solid_is_well_made`]): about six minutes in the gate's build.
#[test]
#[ignore = "the full run of test 3 (every solid word at more details and drawn sets): minutes; run in release at a stage's end"]
fn t3_full() {
    t3(true, (0, 1));
}

fn t3(full: bool, parts: (usize, usize)) {
    let (mut built, mut refused) = (0usize, 0usize);
    let ws = part(solids(), parts);
    assert!(!ws.is_empty());
    for w in ws {
        let mut all = cases(&w);
        all.extend(cases_together(&w, if full { 3 } else { 1 }));
        for (label, given) in all {
            let what = format!("{} {label}", w.name());
            let s = Spec::of(w, &given).unwrap_or_else(|e| panic!("{what}: {e}"));
            let single = label.contains('=') && !label.contains(' ');
            let details: &[u32] = match (full, label == "defaults", single) {
                (false, true, _) => &[DETAIL_MIN, 9, DETAIL_DEFAULT, 256],
                (false, false, true) => &[DETAIL_MIN, DETAIL_DEFAULT],
                (false, false, false) => &[DETAIL_MIN, 9],
                (true, true, _) => &[DETAIL_MIN, 9, 30, DETAIL_DEFAULT, 101, 256, DETAIL_MAX],
                (true, false, true) => &[DETAIL_MIN, 9, DETAIL_DEFAULT, 101, 256],
                (true, false, false) => &[DETAIL_MIN, 9, 30, DETAIL_DEFAULT],
            };
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
    assert!(built > 500 / parts.1, "{built} built, {refused} refused");
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
///
/// The default run builds the extremes at details 64 and 128 (the defaults at 128 and 256, where
/// the rate is held); [`t4_full`] builds every case at 128 and 256.
#[test]
fn t4_solids_closed_forms_hold_against_the_build() {
    t4(false, (0, PARTS));
}

/// Test 4 for solids in full (see [`t4_solids_closed_forms_hold_against_the_build`]).
#[test]
#[ignore = "the full run of test 4 for solids (every extreme at details 128 and 256): a minute; run in release at a stage's end"]
fn t4_full() {
    t4(true, (0, 1));
}

fn t4(full: bool, parts: (usize, usize)) {
    let mut checked = 0;
    for w in part(solids(), parts) {
        for (label, given) in cases(&w) {
            let s = Spec::of(w, &given).unwrap();
            let what = format!("{} {label}", w.name());
            let Ok(closed) = s.closed() else { continue };
            let curved = s.geom_curved();
            let (d1, d2) = match (curved, full || label == "defaults") {
                (false, _) => (DETAIL_DEFAULT, DETAIL_DEFAULT),
                (true, true) => (128, 256),
                (true, false) => (64, 128),
            };
            let (Ok(b1), Ok(b2)) = (s.build(d1), s.build(d2)) else { continue };
            let (m1, m2) = (mesh_of(&b1), mesh_of(&b2));
            let (g1, g2) = (m1.measures(), m2.measures());
            let r = reach_about_middle(m2);
            let (rv, ra) = rounding(m2);
            let (typed, geometry) = (s.typed(), s.geometry());
            if let (Some(t), Some(c)) = (typed.size, geometry.size) {
                assert!((t - c).abs() <= 1e-9 * r * r * r + rv, "{what}: typed volume {t} against the geometry's {c}");
                checked += 1;
            }
            if let (Some(t), Some(c)) = (typed.boundary, geometry.boundary) {
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
    assert!(checked > 100 / parts.1, "only {checked} checks");
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
// (Moved on 2026-11-20, from 0xa6f9_09c6_ca30_fb04: the superellipsoid is built as a profile
// turned on a superellipse, not a cube's directions sent to its surface (review 1b); each of the
// other ten builds hashes as before.)
// (Moved again on 2026-11-20, from 0xcbc0_d9d6_79ba_a7b2: arcs placed from their nearer end, as
// the plane figures' pinned builds moved.)
const PINNED: u64 = 0x504f_415b_b170_9c8e;

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

/// The review of stage 1b's cases, and the stretched forms at their corners together and at odd
/// details: the ellipsoid, the superellipsoid and the elliptic cylinder are built on their own
/// ring with its axis points among the samples, so the build reaches its six bounds exactly (to
/// rounding), stands on `y = 0`, holds the volume within `2 h A` of its closed form, and gives at
/// every corner the surface's own normal (the gradient of `|x/rx|^n + |y/ry|^n + |z/rz|^n`), on
/// its face's outer side. (Before, the superellipsoid sent a cube's directions to the surface and
/// a long one was built 8.5 m long of 20 km, and long ellipsoids gave normals behind their faces.)
#[test]
fn stretched_forms_reach_their_bounds_and_keep_their_normals() {
    let review: &[(&str, &[(&str, f64)])] = &[
        ("superellipsoid", &[("rx", 1.1371588282277725), ("ry", 4.399442507768615), ("rz", 10000.0), ("exponent", 3.467246616652003)]),
        ("superellipsoid", &[("rx", 9990.00999000999), ("ry", 0.00035081709165283757), ("rz", 0.049869934383160036), ("exponent", 4.0)]),
        ("superellipsoid", &[("rx", 0.9503690220996647), ("ry", 2.835845645517228), ("rz", 0.012524097963417519), ("exponent", 4.0)]),
        ("superellipsoid", &[("rx", 7.417752403704147), ("ry", 205.22400463685764), ("rz", 2129.382121878588), ("exponent", 4.044087714203489)]),
        ("superellipsoid", &[("rx", 413.32790249992337), ("ry", 9778.493687919841), ("rz", 0.5), ("exponent", 31.9999695)]),
        ("superellipsoid", &[("rx", 0.013406543549556999), ("ry", 9990.00999000999), ("rz", 10000.0), ("exponent", 1.5)]),
        ("ellipsoid", &[("rx", 0.0001), ("ry", 10000.0), ("rz", 1.8966804249323033)]),
        ("ellipsoid", &[("rx", 0.033), ("ry", 10000.0), ("rz", 9990.0)]),
        ("elliptic_cylinder", &[("rx", 0.0001), ("rz", 48.69957914368398), ("height", 0.00010557210467012696)]),
        ("elliptic_cylinder", &[("rx", 22.4), ("rz", 4525.0)]),
        ("elliptic_torus", &[("radius", 8834.817246636827), ("width", 0.0001001), ("height", 1048.3844376613379)]),
    ];
    let mut sets: Vec<(&str, Vec<(&'static str, f64)>)> = Vec::new();
    for (name, given) in review {
        let f = crate::table::form(name).unwrap();
        sets.push((name, given.iter().map(|&(n, v)| (f.params.iter().find(|p| p.name == n).unwrap().name, v)).collect()));
    }
    for name in ["ellipsoid", "superellipsoid", "elliptic_cylinder", "elliptic_torus"] {
        let w = Found { form: crate::table::form(name).unwrap(), alias: None };
        for (_, given) in cases_together(&w, 6) {
            sets.push((name, given));
        }
    }
    let mut built = 0;
    for (name, given) in sets {
        let s = Spec::new(name, Some(Kind::Solid), &given).unwrap();
        for d in [8u32, 9, 30, 101] {
            let what = format!("{name} {given:?} at detail {d}");
            let b = match s.build(d) {
                Ok(b) => b,
                // (Refused only for an edge under the floor or the form's own rules: never for a
                // normal behind its face or a crossing.)
                Err(e @ Error::Constraint { rule, .. }) if !matches!(rule, "normal_behind_face" | "crosses_at_this_detail" | "not_well_made") => {
                    sound_solid_refusal(&what, &s, d, &e);
                    continue;
                }
                Err(e) => panic!("{what}: {e}"),
            };
            built += 1;
            let m = mesh_of(&b);
            check_built(&what, &s, d, &b);
            let c = s.closed().unwrap();
            let (lo, hi) = c.bounds.unwrap();
            let (blo, bhi) = m.bounds().unwrap();
            let r = m.reach();
            let h = m.chord_error;
            // (The elliptic torus is turned on a circle, which reaches its bounds within its error.)
            let reach = if name == "elliptic_torus" { 2.0 * h + 1e-12 * r } else { 1e-12 * r };
            for k in 0..3 {
                assert!(blo[k] >= lo[k] - 1e-12 * r && bhi[k] <= hi[k] + 1e-12 * r && blo[k] - lo[k] <= reach && hi[k] - bhi[k] <= reach, "{what}: built {blo:?}..{bhi:?}, its bounds {lo:?}..{hi:?}");
            }
            let (v, a) = (m.measures().size.unwrap(), m.measures().boundary.unwrap());
            let cv = c.size.unwrap();
            assert!((cv - v).abs() <= 2.0 * h * a + 64.0 * m.points.len() as f64 * EPS * r * r * r, "{what}: volume closed {cv}, built {v}, error {h}");
            if name == "elliptic_torus" {
                continue;
            }
            let (rx, ry, rz) = (s.args.f("rx"), if name == "elliptic_cylinder" { 1.0 } else { s.args.f("ry") }, s.args.f("rz"));
            let n = if name == "superellipsoid" { s.args.f("exponent") } else { 2.0 };
            // The error is no less than how far the true surface stands out of any face's plane
            // (the build is convex with its points on the surface, so the surface's farthest point
            // out along a face's normal is at least that far from the build): the support function
            // in the face's normal less the plane's own distance.
            let q = n / (n - 1.0);
            let support = |d: P3| -> f64 {
                let pw = |x: f64| if x == 0.0 { 0.0 } else { powf(x.abs(), q) };
                if name == "elliptic_cylinder" {
                    ((rx * d[0]) * (rx * d[0]) + (rz * d[2]) * (rz * d[2])).sqrt() + (d[1] * s.args.f("height")).max(0.0)
                } else {
                    powf(pw(rx * d[0]) + pw(ry * d[1]) + pw(rz * d[2]), 1.0 / q) + d[1] * ry
                }
            };
            let cap = m.faces.iter().map(|f| {
                let nn = norm3(m.area_vector(f));
                support(nn) - dot3(nn, m.points[f.verts[0] as usize])
            });
            let cap = cap.fold(0.0f64, f64::max);
            assert!(h >= cap * (1.0 - 1e-9) - 1e-12 * r, "{what}: its error {h} is under how far the surface stands out of a face's plane, {cap}");
            for f in &m.faces {
                let Some(ns) = &f.normals else { continue };
                for (nn, &v) in ns.iter().zip(&f.verts) {
                    let p = m.points[v as usize];
                    let g = |x: f64, r: f64| if x < 0.0 { -powf(-x / r, n - 1.0) / r } else { powf(x / r, n - 1.0) / r };
                    let grad = if name == "elliptic_cylinder" { [g(p[0], rx), 0.0, g(p[2], rz)] } else { [g(p[0], rx), g(p[1] - ry, ry), g(p[2], rz)] };
                    if len3(grad) > 0.0 {
                        assert!(dist3(*nn, norm3(grad)) <= 1e-9, "{what}: a normal {nn:?} at {p:?}, the surface's is {:?}", norm3(grad));
                    }
                }
            }
        }
    }
    assert!(built > 300, "{built} built");
}

/// The review of stage 1b's second finding: a turned figure's volume, and the table's tubes', tori's
/// and shells' closed forms, keep their digits when the section is thin and far out. Each against
/// a form with nothing in it to cancel, to `1e-12`: Pappus's theorem from the figure's own area
/// and centroid (`revolve_of`); `π h (R − r)(R + r)` (the tube); `2π² R r²` and `θ R π r²` (the
/// torus and its sector); `2π R w h` times π (the elliptic torus); the dome and the bowl as
/// `2π t (R² + R r + r²) / 3` with `t` as given; the spherical segment by Gauss–Legendre of
/// `π r³ (1 − u)(1 + u)` between its cuts. The table's tube, tori and the turned figures are
/// held so by their geometry's own measures too (the profile's integrals about its own origin:
/// before, about the axis, a section 1e-4 across at 675 m lost all but three digits).
#[test]
fn thin_sections_far_out_keep_their_digits() {
    let near = |what: &str, got: f64, want: f64| assert!((got - want).abs() <= 1e-12 * want.abs(), "{what}: {got}, and with nothing to cancel {want} (off by {:e} of it)", (got - want).abs() / want.abs());
    let thin = [1e-4, 3.7e-4, 1e-3];
    let far = [10.0, 675.065_368_564_906_1, 9_999.0];
    for &t in &thin {
        for &big in &far {
            let tube = Spec::new("tube", Some(Kind::Solid), &[("radius", big), ("inner", big - t), ("height", 3.0)]).unwrap();
            let (r, i, h) = (tube.args.f("radius"), tube.args.f("inner"), 3.0);
            for (what, m) in [("tube closed", tube.closed().unwrap()), ("tube geometry", tube.geometry())] {
                near(what, m.size.unwrap(), PI * h * (r - i) * (r + i));
            }
            let torus = Spec::new("torus", Some(Kind::Solid), &[("radius", big), ("tube", t)]).unwrap();
            for (what, m) in [("torus closed", torus.closed().unwrap()), ("torus geometry", torus.geometry())] {
                near(what, m.size.unwrap(), 2.0 * PI * PI * big * t * t);
            }
            let bend = Spec::new("torus_sector", Some(Kind::Solid), &[("radius", big), ("tube", t), ("angle", 37.0)]).unwrap();
            for (what, m) in [("torus_sector closed", bend.closed().unwrap()), ("torus_sector geometry", bend.geometry())] {
                near(what, m.size.unwrap(), rad(37.0) * big * PI * t * t);
            }
            let et = Spec::new("elliptic_torus", Some(Kind::Solid), &[("radius", big), ("width", t), ("height", 2.0 * t)]).unwrap();
            near("elliptic_torus closed", et.closed().unwrap().size.unwrap(), TAU * big * PI * t * 2.0 * t);
            for name in ["dome", "bowl"] {
                let s = Spec::new(name, Some(Kind::Solid), &[("radius", big), ("thickness", t)]).unwrap();
                let i = big - t;
                near(&format!("{name} closed"), s.closed().unwrap().size.unwrap(), TAU * t * (big * big + big * i + i * i) / 3.0);
            }
            // A zone between two cuts near the north pole, t apart (over the radius).
            let (f, u) = (0.998, (0.998 + t).min(0.999));
            let zone = Spec::new("spherical_segment", Some(Kind::Solid), &[("radius", big), ("from", f), ("to", u)]).unwrap();
            let (hh, mm) = (0.5 * (u - f), 0.5 * (u + f));
            // (Each node as f + hh (1 + x), and 1 less it as (1 − f) − hh (1 + x): exact to rounding.)
            let want: f64 = gauss().iter().map(|&(x, w)| w * hh * PI * big * big * big * ((1.0 - f) - hh * (1.0 + x)) * (1.0 + mm + hh * x)).sum();
            // (Held to 1e-14: the ranges stop the cuts at 0.999, where the textbook form
            // r² − (y1² + y1 y0 + y0²)/3 cancels by a factor of about 500, about 5e-14 lost.)
            let got = zone.closed().unwrap().size.unwrap();
            assert!((got - want).abs() <= 1e-14 * want, "spherical_segment closed {got}, by quadrature {want} (off by {:e})", (got - want).abs() / want);
            // Thin figures turned far out: Pappus from the figure's own closed measures.
            for (name, given) in [("rectangle", vec![("width", t), ("height", 342.66)]), ("chevron", vec![("width", t), ("height", 342.655_950_984_820_04), ("thickness", t)]), ("circle", vec![("radius", t)]), ("rhombus", vec![("width", t), ("height", 9990.0)]), ("annular_sector", vec![("radius", 1.0), ("inner", 1.0 - t), ("angle", 40.0)])] {
                let fig = Spec::new(name, Some(Kind::Plane), &given).unwrap();
                let pm = fig.closed().unwrap();
                let (a, g) = (pm.size.unwrap(), pm.centroid.unwrap());
                let lo = pm.bounds.unwrap().0;
                for angle in [360.0, 155.714_723_463_240_46] {
                    let offset = -lo[0] + big;
                    let solid = crate::ops::revolve_of(&fig, offset, angle).unwrap();
                    near(&format!("revolve_of {name} {given:?} at {offset}, {angle} degrees"), solid.closed().size.unwrap(), a * (g[0] + offset) * rad(angle));
                }
            }
        }
    }
}

/// The solids turned from parametric pieces have an error that is a bound, known before
/// building and equal to the build's: every step of their profiles turns by under a quarter turn
/// (the hyperboloid's side is cut at its waist, the elliptic torus's ellipse into quarters, the
/// ellipsoid's and superellipsoid's profiles at the equator), so each chord's sag is bounded by its
/// triangle or given exactly by its curve's support function. A steep hyperboloid (1e-3 tall
/// with ends of 0.5) at an odd detail is one: cut whole, its middle step straddled the waist
/// and turned past a quarter turn, and its error was only an estimate.
#[test]
fn turned_parametric_pieces_have_bounds() {
    let rows: &[(&str, &[(&str, f64)])] = &[
        ("paraboloid", &[]),
        ("paraboloid", &[("radius", 1e-4), ("height", 1e4)]),
        ("hyperboloid", &[]),
        ("hyperboloid", &[("waist", 1e-4)]),
        ("hyperboloid", &[("height", 1e-3)]),
        ("elliptic_torus", &[]),
        ("elliptic_torus", &[("width", 1e-4), ("height", 100.0)]),
        ("ellipsoid", &[("rx", 1e-3), ("ry", 1e3)]),
        ("superellipsoid", &[("exponent", 32.0)]),
        ("superellipsoid", &[("exponent", 1.5), ("rz", 1e3)]),
        ("elliptic_cylinder", &[("rx", 1e-3)]),
    ];
    for (name, given) in rows {
        let s = Spec::new(name, Some(Kind::Solid), given).unwrap();
        for d in [8u32, 9, 30, 101] {
            let what = format!("{name} {given:?} at detail {d}");
            let Ok(b) = s.build(d) else { continue };
            assert_eq!(s.error(d).unwrap(), Some(mesh_of(&b).chord_error), "{what}: its error is not a bound known before building");
        }
    }
}

/// The mesh's own check refuses a normal on its face's inner side: a sphere's build with one
/// corner's normal turned past a right angle from its face is not well made, and names that face
/// and corner.
#[test]
fn validate_refuses_a_normal_behind_its_face() {
    let b = Spec::new("sphere", Some(Kind::Solid), &[]).unwrap().build(16).unwrap();
    let mut m = mesh_of(&b).clone();
    assert_eq!(m.validate(0), Ok(()));
    let fi = m.faces.iter().position(|f| f.normals.is_some()).unwrap();
    let n = norm3(m.area_vector(&m.faces[fi]));
    if let Some(ns) = m.faces[fi].normals.as_mut() {
        ns[1] = scale3(n, -1.0);
    }
    match m.validate(0) {
        Err(crate::SolidFault::NormalBehind { face, corner, cos }) => assert!(face == fi && corner == 1 && cos < 0.0, "{face} {corner} {cos}"),
        other => panic!("a normal turned in passed the check: {other:?}"),
    }
}

/// A form says it is curved exactly when it is: its build has a curved part (a smooth surface,
/// for a solid; an arc or a parametric piece, for a figure) and an error above nothing, at the
/// defaults and at its parameters' extremes. (`round_arrow`, a cylinder and a cone, said it was
/// not.)
#[test]
fn curved_says_what_a_build_has() {
    for w in words().into_iter().filter(|w| w.alias.is_none()) {
        for (label, given) in cases(&w) {
            let s = Spec::of(w, &given).unwrap();
            let Ok(b) = s.build(DETAIL_DEFAULT) else { continue };
            let what = format!("{} {label}", w.name());
            assert_eq!(w.form.curved, s.geom_curved(), "{what}: the table says curved {}, its geometry the other", w.form.curved);
            assert_eq!(w.form.curved, b.chord_error() > 0.0, "{what}: the table says curved {}, and the build's error is {}", w.form.curved, b.chord_error());
            if let Built::Mesh(m) = &b {
                assert_eq!(w.form.curved, m.surfaces.iter().any(|x| x.smooth), "{what}: the table says curved {}, and its surfaces {:?}", w.form.curved, m.surfaces);
            }
        }
    }
}

/// Test 3's default run, its second part (see [`t3_every_solid_is_well_made`]).
#[test]
fn t3_every_solid_is_well_made_2() {
    t3(false, (1, PARTS));
}

/// Test 3's default run, its third part.
#[test]
fn t3_every_solid_is_well_made_3() {
    t3(false, (2, PARTS));
}

/// Test 3's default run, its fourth part.
#[test]
fn t3_every_solid_is_well_made_4() {
    t3(false, (3, PARTS));
}

/// Test 4's default run for solids, its second part (see
/// [`t4_solids_closed_forms_hold_against_the_build`]).
#[test]
fn t4_solids_closed_forms_hold_against_the_build_2() {
    t4(false, (1, PARTS));
}

/// Test 4's default run for solids, its third part.
#[test]
fn t4_solids_closed_forms_hold_against_the_build_3() {
    t4(false, (2, PARTS));
}

/// Test 4's default run for solids, its fourth part.
#[test]
fn t4_solids_closed_forms_hold_against_the_build_4() {
    t4(false, (3, PARTS));
}

/// **The bell's true convergence** (review 1 of stage 4, finding 5). The defaults' check above
/// asks a curved solid's error to fall three to five times when the detail doubles. That is not a
/// property of the bell: its volume is the outside's less the inside's, whose chords' errors
/// cancel in part (measured at 1.4 to 8.6 times over its range), and a short arc's segments do
/// not double exactly (`ceil`), which moves the area's ratio too (2.7 at a strong flare). What
/// does hold across its range, and is held here over a grid of it: at every detail each measure
/// is within the bound the build states for it (twice the chord error times the area, and the
/// faceting's), and that bound falls as the detail rises.
#[test]
fn the_bells_error_is_within_its_bound_across_its_range() {
    let mut checked = 0;
    for hr in [1.5, 2.0, 2.5, 3.0] {
        for wr in [0.45, 0.55, 0.7, 0.9] {
            for fr in [0.45, 0.6, 0.8] {
                for sr in [0.1, 0.4, 0.9] {
                    for wall in [0.002, 0.025, 0.05] {
                        let given = [("height_ratio", hr), ("waist_ratio", wr), ("flare_ratio", fr), ("shoulder_ratio", sr), ("wall", wall)];
                        let Ok(s) = Spec::new("bell", Some(Kind::Solid), &given) else { continue };
                        let Ok(c) = s.closed() else { continue };
                        let mut last = f64::INFINITY;
                        for d in [64, 128, 256] {
                            let Ok(b) = s.build(d) else { continue };
                            let m = mesh_of(&b);
                            let g = m.measures();
                            let (lv, la) = allowed(m);
                            let (ev, ea) = ((c.size.unwrap() - g.size.unwrap()).abs(), (c.boundary.unwrap() - g.boundary.unwrap()).abs());
                            assert!(ev <= lv && ea <= la, "bell {given:?} at {d}: volume off {ev} (allowed {lv}), area off {ea} (allowed {la})");
                            assert!(lv < last, "bell {given:?}: the volume's bound did not fall at {d}");
                            last = lv;
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked >= 300, "{checked}");
}
