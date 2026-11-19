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
