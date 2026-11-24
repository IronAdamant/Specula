//! Tests of the cap cutter (cap.rs) on outlines with holes: the three faults review 1 of stage 4
//! found (a bridge along a line through other holes' corners, two holes bridged to one corner in
//! the wrong order, a rounding bound that held a clear point to be on a long edge), each on a
//! small region made by hand and on the figures that met it, and every truss, grate and arched
//! wall of a grid over their ranges raised by every profile.

use crate::cap::{check, cut};
use crate::maths::*;
use crate::raise::{bevelled, pillow, relief, slab, stepped};
use crate::table::{Kind, LEN_MAX, LEN_MIN};
use crate::{Built, Error, Spec};

/// A figure's rings as the cutter takes them: points, and rings of indices.
fn rings_of(s: &Spec, detail: u32) -> (Vec<Pt>, Vec<Vec<u32>>) {
    let Ok(Built::Outline(o)) = s.build(detail) else { panic!("{}: not an outline", s.name()) };
    let mut pts = Vec::new();
    let mut rings = Vec::new();
    for r in &o.rings {
        let from = pts.len() as u32;
        pts.extend_from_slice(r);
        rings.push((from..pts.len() as u32).collect());
    }
    (pts, rings)
}

/// The cut of a region given as rings of points, checked by the proof and counted.
fn cuts(rings: &[Vec<Pt>]) {
    let mut pts = Vec::new();
    let mut idx = Vec::new();
    for r in rings {
        let from = pts.len() as u32;
        pts.extend_from_slice(r);
        idx.push((from..pts.len() as u32).collect::<Vec<u32>>());
    }
    let tris = cut(&pts, &idx).unwrap_or_else(|f| panic!("not cut: {f:?}"));
    check(&pts, &idx, &tris).unwrap();
    assert_eq!(tris.len() as u64, crate::cap::triangles_of(pts.len() as u64, rings.len() as u64));
}

/// A rectangle's ring, counter-clockwise (an outer ring), or clockwise (a hole).
fn rect(lo: Pt, hi: Pt, hole: bool) -> Vec<Pt> {
    let mut v = vec![lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
    if hole {
        v.reverse();
    }
    v
}

/// A bridge from a hole's corner along a line through two corners of another hole (on the line,
/// or a rounding off it, as a truss's are) touches that hole: the cutter takes another.
#[test]
fn a_bridge_along_other_holes_corners_is_not_taken() {
    let y = 0.05;
    for lift in [0.0, 1e-17, 7e-18] {
        // Hole A left, hole B right, their bottoms on the line y = 0.05 (B's a rounding above
        // it); A's rightmost corner looks along that line through B's bottom to the outer ring.
        let outer = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 1.0], [0.0, 1.0]];
        let a = rect([0.5, y], [1.0, 0.5], true);
        let b = vec![[2.0, y + lift], [2.0, 0.5], [3.0, 0.5], [3.0, y + lift]];
        cuts(&[outer, a, b]);
    }
    // And the truss that met it (7 bays at its defaults).
    let s = Spec::new("truss", Some(Kind::Plane), &[("bays", 7.0)]).unwrap();
    let (pts, rings) = rings_of(&s, 8);
    check(&pts, &rings, &cut(&pts, &rings).unwrap()).unwrap();
}

/// Two holes bridged to one corner of the outer ring: the second bridge is spliced into the
/// side of the first that faces it.
#[test]
fn two_holes_bridged_to_one_corner_are_spliced_in_order() {
    // Both holes' rightmost corners see only the outer ring's corner (2, 0) to their right (the
    // ring's right side runs from there up and away).
    let outer = vec![[-2.0, 0.0], [2.0, 0.0], [2.2, 3.2], [-2.0, 3.2]];
    let low = vec![[1.7, 0.192], [-1.7, 0.192], [-1.7, 1.0], [1.0, 1.0]];
    let high = vec![[1.808, 3.008], [-1.0, 1.5], [-1.808, 3.008]];
    cuts(&[outer, low, high]);
    // And the truss that met it.
    let s = Spec::new("truss", Some(Kind::Plane), &[("length", 4.0), ("height", 3.2), ("bays", 2.0), ("bar", 0.192)]).unwrap();
    let (pts, rings) = rings_of(&s, 8);
    check(&pts, &rings, &cut(&pts, &rings).unwrap()).unwrap();
}

/// A point clear of a long edge by more than the rounding of the turn as it is computed is not
/// on that edge: the strip of a grate 1 by 10,000 m with bars of 0.3 mm (whose cap a bound that
/// counted the far side of the triangle held to have no ear) is cut.
#[test]
fn a_long_thin_strip_is_cut() {
    let outer = rect([-0.5, -5000.0], [0.5, 5000.0], false);
    let hole = (10000.0 - 5.0 * 0.0003) / 4.0;
    let holes = (0..4).map(|j| {
        let y0 = -5000.0 + 0.0003 + j as f64 * (hole + 0.0003);
        rect([-0.4997, y0], [0.4997, y0 + hole], true)
    });
    cuts(&std::iter::once(outer).chain(holes).collect::<Vec<_>>());
    let s = Spec::new("grate", Some(Kind::Plane), &[("width", 1.0), ("height", 10000.0), ("bar", 0.0003)]).unwrap();
    let (pts, rings) = rings_of(&s, 8);
    check(&pts, &rings, &cut(&pts, &rings).unwrap()).unwrap();
}

/// What a raise of `s` by `how` gives at `detail` (its thickness a tenth of `size`, its insets
/// `inset`, a relief's margin a five-hundredth of `size`): built and valid, or the rule that
/// refused it. (A relief's plate has only its four corners on its rim: a margin much under a
/// bar, round a figure whose border has points close together, makes the plate's triangles
/// from the rim's far corners slivers under the mesh's floor, which is refused as such.)
fn raised(s: &Spec, how: &str, size: f64, inset: f64, detail: u32) -> Result<(), &'static str> {
    let (t, w) = ((0.1 * size).clamp(LEN_MIN, LEN_MAX), inset.clamp(LEN_MIN, LEN_MAX));
    let r = match how {
        "slab" => slab(s, t),
        "bevelled" => bevelled(s, t, w, false, false),
        "pillow" => pillow(s, t, t, w),
        "stepped" => stepped(s, 2, 0.5 * t, w),
        _ => relief(s, t, (0.002 * size).clamp(LEN_MIN, LEN_MAX), t),
    };
    match r.and_then(|x| x.build(detail).map(|m| (m, x.genus()))) {
        Ok((m, g)) => {
            m.validate(g).unwrap_or_else(|e| panic!("{} {how}: built and not valid: {e:?}", s.name()));
            Ok(())
        }
        Err(Error::Constraint { rule, .. }) => Err(rule),
        Err(e) => panic!("{} {how}: {e}", s.name()),
    }
}

/// The rules a raise of these figures may refuse with, each true of what it names: an inset
/// deeper than the figure holds, and a part under the mesh's floor (at the extremes of a
/// figure's ranges). Never `not_well_made`, which is a fault of the library.
const TRUE_RULES: &[&str] = &["inset_too_deep", "edge_under_floor"];

/// A figure with what it was given, its size and an inset it holds (a quarter of a bar, or a
/// five-hundredth of its size).
type Case = (Spec, Vec<(&'static str, f64)>, f64, f64);

fn holds(c: &Case, every: bool, faults: &mut Vec<String>) {
    for how in ["slab", "relief", "bevelled", "pillow", "stepped"] {
        if !every && !matches!(how, "slab" | "relief") {
            continue;
        }
        if let Err(rule) = raised(&c.0, how, c.2, c.3, 8) {
            if !TRUE_RULES.contains(&rule) && faults.len() < 12 {
                faults.push(format!("{} {:?} {how}: {rule}", c.0.name(), c.1));
            }
        }
    }
}

fn case(name: &str, given: Vec<(&'static str, f64)>, size: f64) -> Option<Case> {
    let s = Spec::new(name, Some(Kind::Plane), &given).ok()?;
    s.build(8).ok()?;
    let inset = given.iter().find(|g| g.0 == "bar").map_or(0.002 * size, |g| 0.25 * g.1);
    Some((s, given, size, inset))
}

/// The trusses of review 1's scan (length 4, heights 0.05 to 6.4 m, bars 3 to 18 % of the
/// height) with the bays given, at a scale: those the figure accepts.
fn trusses(scale: f64, bays: &[usize]) -> Vec<Case> {
    let mut v = Vec::new();
    for &n in bays {
        for hi in 0..8 {
            let h = 0.05 * pow(2.0, hi);
            for bi in 1..=6 {
                let b = h * 0.03 * bi as f64;
                v.extend(case("truss", vec![("length", 4.0 * scale), ("height", h * scale), ("bays", n as f64), ("bar", b * scale)], (4.0f64).max(h) * scale));
            }
        }
    }
    v
}

/// The trusses review 1 found could not be raised (`not_well_made`, the cap not cut): the
/// defaults with 7 and 10 bays, a tall truss of two bays, a long one of 100, by every profile.
#[test]
fn the_trusses_review_1_found_are_raised() {
    for given in [vec![("bays", 7.0)], vec![("bays", 10.0)], vec![("height", 3.2), ("bays", 2.0), ("bar", 0.192)], vec![("length", 4.0), ("height", 0.2), ("bays", 2.0), ("bar", 0.012)], vec![("length", 40.0), ("bays", 100.0)]] {
        let s = Spec::new("truss", Some(Kind::Plane), &given).unwrap();
        let bar = given.iter().find(|g| g.0 == "bar").map_or(0.05, |g| g.1);
        for how in ["slab", "bevelled", "pillow", "stepped", "relief"] {
            assert_eq!(raised(&s, how, 4.0, 0.25 * bar, 8), Ok(()), "truss {given:?} {how}");
        }
    }
}

/// Trusses away from their defaults (some of the scan's bays, at three scales) are raised as a
/// slab and as a relief.
#[test]
fn trusses_away_from_their_defaults_are_raised() {
    let mut faults = Vec::new();
    for scale in [0.37, 1.0, 1000.0] {
        for c in trusses(scale, &[1, 2, 3, 7, 10, 15, 40, 100]) {
            holds(&c, false, &mut faults);
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

/// The whole scan: every truss it accepts, 1 to 100 bays at three scales, by every profile (a
/// seventh of them by the three that inset, which search).
#[test]
#[ignore = "the whole scan of trusses by every profile: minutes; run in release at a stage's end"]
fn every_truss_of_the_scan_is_raised() {
    let mut faults = Vec::new();
    let bays: Vec<usize> = (1..=100).collect();
    for scale in [0.37, 1.0, 1000.0] {
        for (k, c) in trusses(scale, &bays).iter().enumerate() {
            holds(c, k % 7 == 0, &mut faults);
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Grates over a grid of their ranges: one to eight columns and rows (one column and one row
/// among them), 0.01 to 197 m tall against a width of 1, bars up to a few hundredths.
fn grates(scale: f64) -> Vec<Case> {
    let mut v = Vec::new();
    for c in 1..=8 {
        for r in 1..=8 {
            for hi in 0..10 {
                for bi in 1..=6 {
                    let h = 0.01 * pow(3.0, hi);
                    let b = 0.02 * bi as f64 / (c.max(r) as f64 + 1.0);
                    v.extend(case("grate", vec![("width", scale), ("height", h * scale), ("columns", c as f64), ("rows", r as f64), ("bar", b * scale)], h.max(1.0) * scale));
                }
            }
        }
    }
    v
}

/// A one-column grate tall against its bars (a ladder) is raised: it was refused
/// `edge_under_floor` with words about its parts' sizes that were not true (review 1, finding 7).
/// The sliver was a cap triangle from a hole's corner to the far corner of a side with no point
/// on it; the grate's border has a point level with every hole's corner now.
#[test]
fn a_ladder_grate_is_raised() {
    let s = Spec::new("grate", Some(Kind::Plane), &[("width", 1.0), ("height", 65.61), ("columns", 1.0), ("rows", 7.0), ("bar", 0.0025)]).unwrap();
    for how in ["slab", "relief", "bevelled", "pillow", "stepped"] {
        assert_eq!(raised(&s, how, 65.61, 0.0006, 8), Ok(()), "ladder {how}");
    }
    let s = Spec::new("grate", Some(Kind::Plane), &[("width", 1.0), ("height", 196.83), ("columns", 1.0), ("rows", 8.0), ("bar", 0.002)]).unwrap();
    assert_eq!(raised(&s, "slab", 196.83, 0.0005, 8), Ok(()));
}

/// Grates over the grid (a fifth of it) raised by a slab and a relief.
#[test]
fn grates_over_their_ranges_are_raised() {
    let mut faults = Vec::new();
    for (k, c) in grates(1.0).iter().enumerate() {
        if k % 5 == 0 {
            holds(c, false, &mut faults);
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

#[test]
#[ignore = "every grate of the grid at three scales by every profile: minutes; run in release at a stage's end"]
fn every_grate_of_the_grid_is_raised() {
    let mut faults = Vec::new();
    for scale in [0.37, 1.0, 1000.0] {
        for (k, c) in grates(scale).iter().enumerate() {
            holds(c, k % 7 == 0, &mut faults);
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

/// Arched walls over a grid: widths and heights 0.01 to 655 m, openings 1e-8 to 0.999 of the
/// width, springs 1e-6 to 0.999 of what the height leaves.
fn arches(scale: f64) -> Vec<Case> {
    let mut v = Vec::new();
    for wi in 0..9 {
        for hi in 0..9 {
            for oi in 0..9 {
                for si in 0..5 {
                    let (w, h) = (0.01 * pow(4.0, wi), 0.01 * pow(4.0, hi));
                    let o = w * [1e-8, 1e-6, 1e-4, 0.01, 0.1, 0.3, 0.6, 0.9, 0.999][oi];
                    let sp = (h - 0.5 * o) * [1e-6, 0.01, 0.3, 0.7, 0.999][si];
                    v.extend(case("arch_portal", vec![("width", w * scale), ("height", h * scale), ("opening", o * scale), ("spring", sp * scale)], w.max(h) * scale));
                }
            }
        }
    }
    v
}

/// The arched wall review 1 raised as a relief (an opening 1.3e-8 of the wall) is built or
/// refused by a true rule at every detail, never `not_well_made`.
#[test]
fn the_arch_review_1_found_is_raised_or_refused_truly() {
    let s = Spec::new("arch_portal", Some(Kind::Plane), &[("width", 7900.0), ("height", 3200.0), ("opening", 0.0001), ("spring", 38.0)]).unwrap();
    for d in [8, 33, 64] {
        match relief(&s, 0.001, 0.0001, 0.001).and_then(|x| x.build(d).map(|_| ())) {
            Ok(()) => {}
            Err(Error::Constraint { rule, .. }) => assert!(TRUE_RULES.contains(&rule), "detail {d}: {rule}"),
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
#[ignore = "every arched wall of the grid at three scales by every profile: minutes; run in release at a stage's end"]
fn every_arch_of_the_grid_is_raised() {
    let mut faults = Vec::new();
    for scale in [0.37, 1.0, 1000.0] {
        for (k, c) in arches(scale).iter().enumerate() {
            holds(c, k % 7 == 0, &mut faults);
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

/// `b` to a whole power, by products (the crate's lints keep platform maths out).
fn pow(b: f64, n: i32) -> f64 {
    (0..n).fold(1.0, |x, _| x * b)
}
