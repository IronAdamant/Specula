//! Tests of the raised forms, the drawing's views and the cap's proof.

use crate::maths::*;
use crate::raise::{bevelled, pillow, relief, slab, stepped, RAISED};
use crate::table::{find, forms, Kind};
use crate::{Error, Spec};

fn figures() -> Vec<Spec> {
    forms().iter().filter(|f| f.kind == Kind::Plane).map(|f| Spec::new(f.name, Some(Kind::Plane), &[]).expect("defaults build")).collect()
}

fn straight(s: &Spec) -> bool {
    !s.form.curved
}

/// The size of a figure: the greater side of its closed bounds.
fn size(s: &Spec) -> f64 {
    s.closed().unwrap().bounds.map_or(1.0, |(lo, hi)| (hi[0] - lo[0]).max(hi[1] - lo[1]))
}

/// For every figure of straight sides, the closed volume and surface of each raised form with
/// flat steps (slab, chamfer, terraces, relief) are what is built, to rounding (the inset
/// polygon's area `A − P s + K s²` is exact for it): to 1e-12 of the volume.
#[test]
fn raised_straight_figures_hold_their_closed_forms_exactly() {
    let mut checked = 0;
    for f in figures().into_iter().filter(straight) {
        let w = size(&f);
        let made = [slab(&f, 0.3), bevelled(&f, 0.3, 0.01 * w, false, false), bevelled(&f, 0.3, 0.01 * w, false, true), stepped(&f, 3, 0.1, 0.01 * w), relief(&f, 0.2, 0.1, 0.05)];
        for s in made {
            let s = s.unwrap();
            let m = match s.build(64) {
                Ok(m) => m,
                Err(Error::Constraint { rule: "inset_too_deep", .. }) => continue,
                Err(e) => panic!("{} {}: {e}", s.operator, f.name()),
            };
            let (b, c) = (m.measures(), s.closed());
            let (vb, vc) = (b.size.unwrap(), c.size.expect("a closed volume"));
            assert!((vb - vc).abs() <= 1e-12 * vc.abs() + 1e-15, "{} of {}: built {vb}, closed {vc}", s.operator, f.name());
            let (ab, ac) = (b.boundary.unwrap(), c.boundary.expect("a closed area"));
            assert!((ab - ac).abs() <= 1e-12 * ac.abs() + 1e-15, "{} of {}: built area {ab}, closed {ac}", s.operator, f.name());
            assert_eq!(m.chord_error, 0.0);
            checked += 1;
        }
    }
    assert!(checked > 60, "only {checked}");
}

/// A round bevel's and a pillow's volume converge on their closed forms as the profile's quarter
/// circle (or ellipse) is cut finer: the error is a profile chord's sag times the band's
/// length, falling as the square of the step (three to five times each time the detail
/// doubles); and the build's error is a bound on it from above and its cause from below (it is
/// no less than the greatest sag of the profile's own chords, measured against the ellipse).
#[test]
fn round_profiles_converge_and_their_error_bounds_them() {
    let f = Spec::new("rectangle", Some(Kind::Plane), &[]).unwrap();
    for s in [bevelled(&f, 0.3, 0.1, true, true).unwrap(), pillow(&f, 0.1, 0.2, 0.15).unwrap()] {
        let c = s.closed().size.unwrap();
        let mut last: Option<f64> = None;
        for d in [16u32, 32, 64, 128] {
            let m = s.build(d).unwrap();
            let v = m.measures().size.unwrap();
            let e = (v - c).abs();
            let area = m.measures().boundary.unwrap();
            assert!(e <= 2.0 * m.chord_error * area, "{} at {d}: off by {e}, its error {} over {area}", s.operator, m.chord_error);
            if let Some(l) = last {
                let r = l / e;
                assert!((3.0..=5.0).contains(&r), "{} at {d}: the error fell {r} times", s.operator);
            }
            last = Some(e);
            // From below: the profile's chords' sag against the true quarter, measured.
            let (rs, ry) = if s.operator == "pillow" { (0.15, 0.2) } else { (0.1, 0.1) };
            let k = crate::detail::segments(0.25, d);
            let mut sag = 0.0f64;
            for j in 0..k {
                let (t0, t1) = (0.5 * PI * j as f64 / k as f64, 0.5 * PI * (j + 1) as f64 / k as f64);
                let (a, b) = ([rs * cos(t0), ry * sin(t0)], [rs * cos(t1), ry * sin(t1)]);
                for q in 1..32 {
                    let t = t0 + (t1 - t0) * q as f64 / 32.0;
                    sag = sag.max(dist_to_segment([rs * cos(t), ry * sin(t)], a, b));
                }
            }
            assert!(m.chord_error >= sag, "{} at {d}: error {} under the profile's own sag {sag}", s.operator, m.chord_error);
        }
    }
}

/// An inset refused as too deep names one that holds: built with it (a hair under), the same
/// form builds.
#[test]
fn an_inset_too_deep_names_one_that_holds() {
    let mut named = 0;
    for f in figures() {
        let w = size(&f);
        let s = pillow(&f, 0.1, 0.1, 0.6 * w).unwrap();
        match s.build(30) {
            Err(Error::Constraint { rule: "inset_too_deep", values, .. }) => {
                let holds = values.iter().find(|v| v.0 == "deepest_inset_that_holds").unwrap().1;
                if holds >= crate::table::LEN_MIN {
                    let again = pillow(&f, 0.1, 0.1, holds * (1.0 - 1e-9)).unwrap();
                    // It holds as an inset; the build may still be refused for another reason (a
                    // band thinner than the floor), never for its inset.
                    if let Err(Error::Constraint { rule, .. }) = again.build(30) {
                        assert_ne!(rule, "inset_too_deep", "{}: the named inset {holds} is too deep", f.name());
                    }
                    named += 1;
                }
            }
            Ok(_) => {}
            Err(e) => panic!("{}: {e}", f.name()),
        }
    }
    assert!(named > 20, "only {named}");
}

/// Review 1c, finding 1: a slab asks for no inset. A needle triangle that builds as a figure at a
/// detail builds as a slab at that detail (it was refused as `inset_too_deep` with inset 0: the
/// miters, which a slab never uses, were found undefined at the needle's tip).
#[test]
fn a_slab_of_a_needle_builds_where_its_figure_does() {
    for (base, height) in [(1e-4, 1e4), (1e-4, 413.0)] {
        let f = Spec::new("triangle", Some(Kind::Plane), &[("base", base), ("height", height)]).unwrap();
        for d in [8u32, 9, 30, 101] {
            if f.build(d).is_err() {
                continue;
            }
            let s = slab(&f, 0.5).unwrap();
            match s.build(d) {
                Ok(m) => m.validate(0).unwrap(),
                Err(e) => panic!("a slab of the needle {base} by {height} at {d}: {e}"),
            }
        }
    }
}

/// Review 1c, findings 2 and 3: what a refusal names as holding holds (built at it, the same form
/// is not refused for its inset), and a refusal that can name no inset a caller may ask for (none
/// at or over the least length) names none.
#[test]
fn a_named_inset_holds_and_none_is_named_under_the_least() {
    let f = Spec::new("kite", Some(Kind::Plane), &[("width", 9990.00999000999), ("top", 0.0001001), ("bottom", 0.8)]).unwrap();
    let t = 499.50049950049953;
    let s = pillow(&f, t, t, 99.9000999000999).unwrap();
    for d in [8u32, 9, 30] {
        match s.build(d) {
            Err(Error::Constraint { rule: "inset_too_deep", values, .. }) => {
                if let Some(&(_, holds)) = values.iter().find(|v| v.0 == "deepest_inset_that_holds") {
                    assert!(holds >= crate::table::LEN_MIN, "at {d} it names {holds}, under the least length");
                    if let Err(Error::Constraint { rule, values, .. }) = pillow(&f, t, t, holds).unwrap().build(d) {
                        assert_ne!(rule, "inset_too_deep", "at {d} the named inset {holds} is refused as too deep: {values:?}");
                    }
                }
            }
            other => panic!("at {d}: {:?}", other.map(|_| ())),
        }
    }
    // A needle whose inscribed circle is under the least length: no inset may be asked for that
    // holds, and none is named.
    let needle = Spec::new("triangle", Some(Kind::Plane), &[("base", 1e-4), ("height", 50.0)]).unwrap();
    for d in [8u32, 30] {
        match pillow(&needle, 1.0, 1.0, 1.0).unwrap().build(d) {
            Err(Error::Constraint { rule: "inset_too_deep", values, .. }) => {
                assert!(values.iter().all(|v| v.0 != "deepest_inset_that_holds"), "the needle at {d} names an inset: {values:?}");
                assert!(values.iter().any(|v| v.0 == "least_length"), "{values:?}");
            }
            other => panic!("the needle at {d}: {:?}", other.map(|_| ())),
        }
    }
    // Every figure, pillowed past its middle: a named inset is never under the least length.
    for f in figures() {
        let w = size(&f);
        for d in [8u32, 30] {
            if let Err(Error::Constraint { rule: "inset_too_deep", values, .. }) = pillow(&f, 0.1, 0.1, 0.6 * w).unwrap().build(d) {
                if let Some(&(_, holds)) = values.iter().find(|v| v.0 == "deepest_inset_that_holds") {
                    assert!(holds >= crate::table::LEN_MIN, "{} at {d} names {holds}", f.name());
                }
            }
        }
    }
}

/// A miter `m` is what it is defined to be, `m · n₀ = m · n₁ = 1` for the two edges' inward unit
/// normals, to rounding (`16 ε (1 + |m|)`: a dot product of numbers no larger than `|m|`), at
/// needles' tips turned to many angles, where the first form of it (`(n₀ + n₁) / (1 + n₀ · n₁)`)
/// lost all but `ε / α²` of its digits; and an outline inset near its collapse keeps every edge
/// parallel to its figure's, to the rounding of its points (`64 ε (R + s |m|)` over the edge's
/// length, `R` the figure's reach).
#[test]
fn miters_keep_their_digits_at_a_needles_tip() {
    use crate::raise::{collapse, inset, miters};
    let mut tips = 0;
    for (w, top, bottom) in [(9990.00999000999, 0.0001001, 0.8), (1e4, 1e-4, 1e-4), (2.0, 1e-4, 3e-4)] {
        for turn in [0.0, 0.3, 37.0, 90.0, 135.0, 211.0] {
            let (s, c) = sin_cos(turn * PI / 180.0);
            let rot = |p: Pt| [c * p[0] - s * p[1], s * p[0] + c * p[1]];
            // The kite of the review's finding 2, and its like: counter-clockwise.
            let q: Vec<Pt> = [[0.0, -bottom], [0.5 * w, 0.0], [0.0, top], [-0.5 * w, 0.0]].into_iter().map(rot).collect();
            let ranges: Vec<std::ops::Range<usize>> = std::iter::once(0..4).collect();
            let m = miters(&q, &ranges).expect("a kite has miters");
            for i in 0..4 {
                let (a, b, cc) = (q[(i + 3) % 4], q[i], q[(i + 1) % 4]);
                for (p0, p1) in [(a, b), (b, cc)] {
                    let d = sub(p1, p0);
                    let n = [-d[1] / len(d), d[0] / len(d)];
                    let dotn = m[i][0] * n[0] + m[i][1] * n[1];
                    assert!((dotn - 1.0).abs() <= 16.0 * f64::EPSILON * (1.0 + len(m[i])), "kite {w} {top} {bottom} turned {turn}: corner {i}'s miter {:?} meets a normal at {dotn}", m[i]);
                }
                tips += usize::from(len(m[i]) > 100.0);
            }
            let reach = q.iter().map(|p| len(*p)).fold(0.0f64, f64::max);
            let most = m.iter().map(|v| len(*v)).fold(0.0f64, f64::max);
            let at = collapse(&q, &m, &ranges);
            for share in [0.5, 0.99, 0.999_999] {
                let s = share * at;
                let p = inset(&q, &m, s);
                for i in 0..4 {
                    let (e, f) = (sub(q[(i + 1) % 4], q[i]), sub(p[(i + 1) % 4], p[i]));
                    let off = (e[0] * f[1] - e[1] * f[0]).abs() / len(e);
                    assert!(off <= 64.0 * f64::EPSILON * (reach + s * most), "kite {w} {top} {bottom} turned {turn}, inset {share} of its collapse: edge {i} turned by {} m over its {} m", off, len(f));
                }
            }
        }
    }
    assert!(tips >= 24, "only {tips} sharp tips");
}

/// A slab of a figure with holes is of their genus, and its caps are cut into the number of
/// triangles Euler's formula gives; the raised forms' names name nothing in the table.
#[test]
fn slabs_of_figures_with_holes() {
    for name in ["annulus", "frame", "polygon_ring"] {
        let f = Spec::new(name, Some(Kind::Plane), &[]).unwrap();
        let s = slab(&f, 0.1).unwrap();
        assert_eq!(s.genus(), 1);
        for d in [8u32, 9, 64, 4096] {
            let m = s.build(d).unwrap();
            m.validate(1).unwrap();
            let c = s.count(d).unwrap();
            assert_eq!((c.points, c.parts), (m.points.len() as u64, m.faces.len() as u64));
        }
    }
    for k in RAISED {
        assert!(find(k, None).is_err(), "{k} names a form");
    }
}

/// The cap's proof refuses what it must: a triangle wound the wrong way, and triangles that do
/// not add up to the rings (one left out, one doubled).
#[test]
fn the_caps_proof_refuses_a_bad_cut() {
    use crate::cap::{check, cut, CapFault};
    // A square with a square hole.
    let pts: Vec<Pt> = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0], [1.0, 1.0], [1.0, 3.0], [3.0, 3.0], [3.0, 1.0]];
    let rings = vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]];
    let tris = cut(&pts, &rings).unwrap();
    assert_eq!(tris.len() as u64, crate::cap::triangles_of(8, 2));
    let area: f64 = tris.iter().map(|t| 0.5 * ((pts[t[1] as usize][0] - pts[t[0] as usize][0]) * (pts[t[2] as usize][1] - pts[t[0] as usize][1]) - (pts[t[1] as usize][1] - pts[t[0] as usize][1]) * (pts[t[2] as usize][0] - pts[t[0] as usize][0]))).sum();
    assert!((area - 12.0).abs() < 1e-12);
    let mut flipped = tris.clone();
    flipped[0].swap(1, 2);
    assert!(matches!(check(&pts, &rings, &flipped), Err(CapFault::Flat(0))));
    let mut short = tris.clone();
    short.pop();
    assert_eq!(check(&pts, &rings, &short), Err(CapFault::Chain));
    let mut double = tris.clone();
    double.push(tris[0]);
    assert_eq!(check(&pts, &rings, &double), Err(CapFault::Chain));
}

/// The named views draw their axes where the drafting rules say: the isometric's three axes 120
/// degrees apart, each foreshortened to √(2/3); the game isometric's ground axes at atan(1/2) =
/// 26.565 degrees; the dimetric's x axis 7.18 degrees below the right and z 41.41 below the left
/// (7° 10′ and 41° 25′), x and y alike and z half of them; the cabinet's depth at half.
#[test]
fn the_views_draw_their_axes_as_the_rules_say() {
    use crate::draw::view;
    let axis = |name: &str, a: P3| {
        let r = view(name).unwrap().projection.rows();
        [dot3(r[0], a), dot3(r[1], a)]
    };
    let deg = |p: Pt| atan2(p[1], p[0]) * 180.0 / PI;
    let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    for a in [x, y, z] {
        assert!((len(axis("isometric", a)) - (2.0f64 / 3.0).sqrt()).abs() < 1e-14);
    }
    assert!((deg(axis("isometric", x)) + 30.0).abs() < 1e-9, "{}", deg(axis("isometric", x)));
    assert!((deg(axis("isometric", z)) + 150.0).abs() < 1e-9, "{}", deg(axis("isometric", z)));
    assert!((deg(axis("game_isometric", x)) + 26.565_051_177_077_99).abs() < 1e-9, "{}", deg(axis("game_isometric", x)));
    let (dx, dy, dz) = (axis("dimetric", x), axis("dimetric", y), axis("dimetric", z));
    assert!((len(dx) - len(dy)).abs() < 1e-14 && (len(dz) - 0.5 * len(dx)).abs() < 1e-14);
    // (The x axis draws at `atan(sin e tan φ)` = `atan(1 / (3 √7))` below the right and z at
    // `atan(sin e / tan φ)` = `atan(√7 / 3)` below the left; the standard gives them to the minute.)
    let (ax, az) = (atan(1.0 / (3.0 * 7f64.sqrt())) * 180.0 / PI, atan(7f64.sqrt() / 3.0) * 180.0 / PI);
    assert!((deg(dx) + ax).abs() < 1e-9 && (ax - (7.0 + 10.0 / 60.0)).abs() < 0.02, "x at {}", deg(dx));
    assert!((deg(dz) + 180.0 - az).abs() < 1e-9 && (az - (41.0 + 25.0 / 60.0)).abs() < 0.02, "z at {}", deg(dz));
    assert!((len(axis("cabinet", z)) - 0.5).abs() < 1e-15 && (len(axis("cavalier", z)) - 1.0).abs() < 1e-15);
    assert_eq!(axis("top", x), [1.0, 0.0]);
    assert_eq!(axis("top", z), [0.0, -1.0]);
    assert_eq!(axis("side", z), [-1.0, 0.0]);
}


/// A refusal's search for the deepest inset is bounded by what its caller allows: with no builds
/// it names nothing and says it did not search (never that no inset holds); with a few, what it
/// names was built and holds, and is no deeper than what the whole search names; a slab and a
/// relief search for nothing.
#[test]
fn a_refusals_search_is_limited_by_its_caller() {
    let f = Spec::new("circle", Some(Kind::Plane), &[("radius", 1.0)]).unwrap();
    let s = pillow(&f, 0.1, 0.1, 1.5).unwrap();
    assert_eq!(s.search_builds(), crate::raise::SEARCH_BUILDS);
    assert_eq!(slab(&f, 0.1).unwrap().search_builds(), 0);
    assert_eq!(relief(&f, 0.1, 0.1, 0.1).unwrap().search_builds(), 0);
    let named = |e: Error| match e {
        Error::Constraint { rule: "inset_too_deep", values, message, .. } => (values.iter().find(|v| v.0 == "deepest_inset_that_holds").map(|v| v.1), values.iter().find(|v| v.0 == "inset_that_holds").map(|v| v.1), message),
        other => panic!("{other:?}"),
    };
    let (full, full_p, _) = named(s.build(30).map(|_| ()).unwrap_err());
    let (full, full_p) = (full.unwrap(), full_p.unwrap());
    let (none, _, said) = named(s.with_search(0).build(30).map(|_| ()).unwrap_err());
    assert!(none.is_none() && said.contains("not searched"), "{said}");
    let (some, some_p, _) = named(s.with_search(4).build(30).map(|_| ()).unwrap_err());
    let (some, some_p) = (some.unwrap(), some_p.unwrap());
    assert!(some <= full && some_p <= full_p, "{some} against {full}");
    assert!(pillow(&f, 0.1, 0.1, some_p).unwrap().build(30).is_ok(), "the named inset {some_p} does not build");
}
