//! Table-driven tests: a form is tested by being in the table.
//!
//! The numbered tests are the brief's (1 builds at the extremes, 2 outlines well made, 4 closed
//! forms against the build, 6 names, 7 anchors, 8 determinism); 3, 5 and 9 to 10 are the
//! solids', the modeller's and the tools', and come with them.

use crate::maths::*;
use crate::table::{aliases, find, forms, Found, Kind, Param, Set, Unit};
use crate::{params_of, Built, Error, Measures, Spec, DETAIL_DEFAULT, DETAIL_MAX, DETAIL_MIN, MAX_POINTS};

const EPS: f64 = f64::EPSILON;

/// Every word: each form by its name, and each alias.
pub(crate) fn words() -> Vec<Found> {
    let mut v: Vec<Found> = forms().iter().map(|f| Found { form: f, alias: None }).collect();
    for a in aliases() {
        v.push(find(a.name, Some(a.kind)).expect("every alias resolves"));
    }
    v
}

/// The cases of a word: its defaults, then each parameter at its least and its greatest.
pub(crate) fn cases(found: &Found) -> Vec<(String, Vec<(&'static str, f64)>)> {
    let mut v = vec![("defaults".to_string(), Vec::new())];
    for p in params_of(found) {
        v.push((format!("{}={}", p.name, p.min), vec![(p.name, p.min)]));
        v.push((format!("{}={}", p.name, p.max), vec![(p.name, p.max)]));
    }
    v
}

fn spec(found: &Found, given: &[(&'static str, f64)]) -> Spec {
    Spec::of(*found, given).unwrap_or_else(|e| panic!("{}: {e}", found.name()))
}

fn size_of(b: &Built) -> f64 {
    let (lo, hi) = b.measures().bounds.expect("a build has points");
    len([hi[0] - lo[0], hi[1] - lo[1]])
}

/// Test 1: every word builds at its defaults and with each parameter at its least and its
/// greatest, at the least, the default and the greatest detail, or is refused as data (a
/// constraint the extremes break, or the point ceiling). What is built has finite points and
/// exactly the count that was given before building. Defaults always build, at every detail.
///
/// The default run builds a solid's extremes at the least and the default detail (its defaults,
/// and every plane figure and curve, at all three); [`t1_full`] builds the solids' extremes at the
/// greatest detail too.
#[test]
fn t1_every_form_builds_at_its_extremes() {
    t1(false);
}

/// Test 1 in full (see [`t1_every_form_builds_at_its_extremes`]).
#[test]
#[ignore = "the full run of test 1 (every solid's extremes at the greatest detail too): a minute; run in release at a stage's end"]
fn t1_full() {
    t1(true);
}

fn t1(full: bool) {
    let mut built = 0;
    for w in words() {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            let details: &[u32] = if full || label == "defaults" || w.form.kind != Kind::Solid { &[DETAIL_MIN, DETAIL_DEFAULT, DETAIL_MAX] } else { &[DETAIL_MIN, DETAIL_DEFAULT] };
            for &d in details {
                let what = format!("{} {label} detail {d}", w.name());
                match s.build(d) {
                    Ok(b) => {
                        built += 1;
                        let c = s.count(d).unwrap();
                        assert_eq!((c.points, c.parts), (b.points(), b.parts()), "{what}: the count before building is not what was built");
                        let pts: Vec<Pt> = match &b {
                            Built::Outline(o) => o.points().copied().collect(),
                            Built::Curve(c) => c.points.clone(),
                            Built::Mesh(m) => m.points.iter().flat_map(|p| [[p[0], p[1]], [p[2], 0.0]]).collect(),
                        };
                        assert!(pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "{what}: a point is not finite");
                        assert!(b.chord_error().is_finite(), "{what}");
                    }
                    Err(e @ Error::Constraint { .. }) if label != "defaults" => sound_refusal(&what, &s, d, &e),
                    Err(Error::TooManyPoints { points, ceiling, .. }) if d == DETAIL_MAX || label != "defaults" => {
                        assert!(points > ceiling && ceiling == MAX_POINTS, "{what}");
                    }
                    Err(e) => panic!("{what}: {e}"),
                }
            }
        }
    }
    assert!(built > 1000, "only {built} builds");
}

/// Test 2: every plane figure's outline is well made (rings simple, the outer one
/// counter-clockwise, holes clockwise and inside it, no zero-length edge), at every case the
/// table allows, at the least and the default detail and, at its defaults, the greatest; every
/// curve's points are finite and no step of it has zero length.
#[test]
fn t2_every_outline_is_well_made() {
    for w in words().into_iter().filter(|w| w.form.kind != Kind::Solid) {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            let details: &[u32] = if label == "defaults" { &[DETAIL_MIN, DETAIL_DEFAULT, DETAIL_MAX] } else { &[DETAIL_MIN, DETAIL_DEFAULT] };
            for &d in details {
                let Ok(b) = s.build(d) else { continue };
                let what = format!("{} {label} detail {d}", w.name());
                match &b {
                    Built::Outline(o) => {
                        if let Err(f) = o.validate() {
                            panic!("{what}: {f:?}");
                        }
                    }
                    Built::Curve(c) => {
                        let size = size_of(&b);
                        let n = c.points.len();
                        assert!(n >= 2, "{what}");
                        let steps = if c.closed { n } else { n - 1 };
                        for i in 0..steps {
                            let l = dist(c.points[i], c.points[(i + 1) % n]);
                            assert!(l > size * 1e-9, "{what}: step {i} has length {l}");
                        }
                    }
                    // (A solid's mesh is checked by test 3, in tests_solid.rs.)
                    Built::Mesh(_) => {}
                }
            }
        }
    }
}

/// A value a refusal carries.
pub(crate) fn value(values: &[(&'static str, f64)], name: &str) -> Option<f64> {
    values.iter().find(|v| v.0 == name).map(|v| v.1)
}

/// Whether a build is well made, as `Spec::build` must have checked.
fn fault(b: &Built) -> Option<crate::Fault> {
    match b {
        Built::Outline(o) => o.validate().err(),
        Built::Curve(c) => c.validate().err(),
        Built::Mesh(_) => None,
    }
}

/// A refusal of a build at `d` is sound. A form's own rule (a triangle's inequality) passes.
/// `not_well_made` is never sound: it is a mistake in a form's geometry. For the two rules a
/// build's own check gives, the fault is real (the build as sampled has that edge, or those two
/// edges crossing), its numbers are what they say, and a detail it names builds well made; where
/// it names none, the last detail it tried is not well made too (or the search reached the end
/// of the range or the ceiling).
fn sound_refusal(what: &str, s: &Spec, d: u32, e: &Error) {
    let Error::Constraint { rule, values, .. } = e else { panic!("{what}: {e}") };
    if s.form.kind == Kind::Solid {
        return crate::tests_solid::sound_solid_refusal(what, s, d, e);
    }
    let v = |n: &str| value(values, n).unwrap_or_else(|| panic!("{what}: {rule} carries no {n}: {values:?}"));
    let raw = || s.raw(d).unwrap_or_else(|| panic!("{what}: refused {rule} and its geometry is refused too"));
    match *rule {
        "not_well_made" => panic!("{what}: {e}"),
        "edge_under_floor" if value(values, "gap").is_some() => {
            // Two edges nearer than the floor: the build as made has that fault, those edges.
            assert_eq!(v("detail"), d as f64, "{what}");
            assert!(v("gap") <= v("floor") && v("floor") == crate::EDGE_FLOOR * v("size"), "{what}: {values:?}");
            let (a, b) = ((v("ring_a") as usize, v("edge_a") as usize), (v("ring_b") as usize, v("edge_b") as usize));
            match fault(&raw()) {
                Some(crate::Fault::Gap { a: x, b: y, gap }) => assert_eq!((x, y, gap), (a, b, v("gap")), "{what}"),
                other => panic!("{what}: refused for a gap, and the build's fault is {other:?}"),
            }
            match value(values, "coarser_detail_that_holds") {
                Some(h) => {
                    assert!(h < d as f64 && h >= DETAIL_MIN as f64, "{what}: {values:?}");
                    s.build(h as u32).unwrap_or_else(|e| panic!("{what}: the detail {h} it names does not hold: {e}"));
                }
                None => {
                    let last = v("searched_to") as u32;
                    assert!(last <= d && fault(&s.raw(last).unwrap()).is_some(), "{what}: names no coarser detail, and {last} holds");
                }
            }
        }
        "edge_under_floor" => {
            assert_eq!(v("detail"), d as f64, "{what}");
            assert!(v("edge") <= v("floor") && v("floor") == crate::EDGE_FLOOR * v("size"), "{what}: {values:?}");
            let (ring, point) = (v("ring") as usize, v("point") as usize);
            let edge = match raw() {
                Built::Outline(o) => {
                    let r = &o.rings[ring];
                    dist(r[point], r[(point + 1) % r.len()])
                }
                Built::Curve(c) => dist(c.points[point], c.points[(point + 1) % c.points.len()]),
                Built::Mesh(_) => unreachable!("solids are refused by sound_solid_refusal"),
            };
            assert_eq!(edge, v("edge"), "{what}: the edge it names is not that long");
            match value(values, "coarser_detail_that_holds") {
                Some(h) => {
                    assert!(h < d as f64 && h >= DETAIL_MIN as f64, "{what}: {values:?}");
                    s.build(h as u32).unwrap_or_else(|e| panic!("{what}: the detail {h} it names does not hold: {e}"));
                }
                None => {
                    let last = v("searched_to") as u32;
                    assert!(last <= d && fault(&s.raw(last).unwrap()).is_some(), "{what}: names no coarser detail, and {last} holds");
                }
            }
        }
        "crosses_at_this_detail" => {
            assert_eq!(v("detail"), d as f64, "{what}");
            let (a, b) = ((v("ring_a") as usize, v("edge_a") as usize), (v("ring_b") as usize, v("edge_b") as usize));
            match fault(&raw()) {
                Some(crate::Fault::Crossing { a: x, b: y }) => assert_eq!((x, y), (a, b), "{what}"),
                other => panic!("{what}: refused for a crossing, and the build's fault is {other:?}"),
            }
            match value(values, "finer_detail_that_holds") {
                Some(h) => {
                    assert!(h > d as f64 && h <= DETAIL_MAX as f64, "{what}: {values:?}");
                    s.build(h as u32).unwrap_or_else(|e| panic!("{what}: the detail {h} it names does not hold: {e}"));
                }
                None => {
                    let last = v("searched_to") as u32;
                    let ended = last == DETAIL_MAX || s.count((2 * last).min(DETAIL_MAX)).map(|c| c.points > MAX_POINTS).unwrap_or(false);
                    assert!(last >= d && (ended || fault(&s.raw(last).unwrap()).is_some()), "{what}: names no finer detail, and {last} holds");
                }
            }
        }
        _ => {}
    }
}

/// A fixed sequence (a 64-bit linear congruential generator; the high bits).
struct Seq(u64);

impl Seq {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The cases of a word with its parameters **moved together**: every pair of parameters at
/// each of its four corners (least and greatest), all at their least, all at their greatest,
/// and `drawn` sets drawn at random together from a fixed sequence (lengths by their
/// logarithm, so a millimetre is as likely as a kilometre). (The cases of [`cases`] move one
/// parameter at a time; a figure whose parameters must agree, a hole and its disc, a spiral's
/// start and its turns, is tried there at one corner only.)
pub(crate) fn cases_together(found: &Found, drawn: usize) -> Vec<(String, Vec<(&'static str, f64)>)> {
    let ps = params_of(found);
    let mut v = Vec::new();
    for i in 0..ps.len() {
        for j in i + 1..ps.len() {
            for (x, y) in [(ps[i].min, ps[j].min), (ps[i].min, ps[j].max), (ps[i].max, ps[j].min), (ps[i].max, ps[j].max)] {
                v.push((format!("{}={x} {}={y}", ps[i].name, ps[j].name), vec![(ps[i].name, x), (ps[j].name, y)]));
            }
        }
    }
    v.push(("all least".to_string(), ps.iter().map(|p| (p.name, p.min)).collect()));
    v.push(("all greatest".to_string(), ps.iter().map(|p| (p.name, p.max)).collect()));
    let mut seq = Seq(found.name().bytes().fold(0x9e37_79b9_7f4a_7c15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)));
    for k in 0..drawn {
        let given: Vec<(&'static str, f64)> = ps
            .iter()
            .map(|p| {
                let u = seq.next();
                let x = if p.integer {
                    let span = (p.max - p.min).min(if k % 4 == 0 { f64::MAX } else { 12.0 });
                    (p.min + floor(u * (span + 1.0))).min(p.max)
                } else if p.min > 0.0 && p.max / p.min > 100.0 {
                    let (lo, hi) = (ln(p.min), ln(p.max));
                    exp(lo + (hi - lo) * u).clamp(p.min, p.max)
                } else {
                    p.min + (p.max - p.min) * u
                };
                (p.name, x)
            })
            .collect();
        v.push((format!("drawn {k}"), given));
    }
    v
}

/// Test 2b: every word with its parameters moved together ([`cases_together`]), at the least,
/// the default and a fine detail: each case is refused soundly ([`sound_refusal`]: a form's
/// rule, the ceiling, or a build's own check with its numbers right), or it builds into
/// what is well made, with the count it gave, and with closed forms the build agrees with as
/// far as its chord error allows: the area within `2 h P` (as in test 4) and no chords longer
/// than the curve (the straight chord between two points is the shortest way between them).
/// The tolerances for rounding are test 4's.
#[test]
fn t2b_parameters_moved_together() {
    let (mut built, mut refused) = (0usize, 0usize);
    for w in words().into_iter().filter(|w| w.form.kind != Kind::Solid) {
        for (label, given) in cases_together(&w, 24) {
            let what = format!("{} {label}", w.name());
            let s = Spec::of(w, &given).unwrap_or_else(|e| panic!("{what}: arguments in their ranges refused: {e}"));
            for d in [DETAIL_MIN, DETAIL_DEFAULT, 256] {
                let what = format!("{what} detail {d}");
                let b = match s.build(d) {
                    Ok(b) => b,
                    Err(Error::TooManyPoints { points, ceiling, .. }) => {
                        assert!(points > ceiling, "{what}");
                        refused += 1;
                        continue;
                    }
                    Err(e) => {
                        sound_refusal(&what, &s, d, &e);
                        refused += 1;
                        continue;
                    }
                };
                built += 1;
                assert_eq!(fault(&b), None, "{what}: built and not well made");
                let c = s.count(d).unwrap();
                assert_eq!((c.points, c.parts), (b.points(), b.parts()), "{what}: the count before building");
                let m = b.measures();
                let r = reach(&b);
                let round = 16.0 * b.points() as f64 * EPS * r;
                let closed = s.closed().unwrap_or_else(|e| panic!("{what}: built, and its closed forms refused: {e}"));
                if let (Some(c), Some(x), Some(p)) = (closed.size, m.size, m.boundary) {
                    let allowed = 2.0 * b.chord_error() * p + round * r;
                    assert!((c - x).abs() <= allowed, "{what}: area closed {c}, built {x} (allowed {allowed})");
                }
                if let (Some(c), Some(x)) = (closed.boundary, m.boundary) {
                    assert!(x <= c + round, "{what}: the chords ({x}) are longer than the curve ({c})");
                }
            }
        }
    }
    assert!(built > 10_000 && refused > 0, "{built} built, {refused} refused");
}

/// The figures the corner drawing of review 1b's mends found (`tests/corners.rs`): every edge
/// over the floor, and two of them nearer each other than it (a chevron's arms 9e-13 apart in a
/// figure 1e4 across; a trapezium's corner of 1.6e-7 radians; a sine band's sides 1e-13 apart),
/// where the crossing test's signs cannot be trusted. Each is refused as `edge_under_floor` with
/// the two edges and their `gap`, soundly.
#[test]
fn edges_nearer_than_the_floor_are_refused() {
    type Row = (&'static str, &'static [(&'static str, f64)]);
    let rows: &[Row] = &[
        ("chevron", &[("width", 0.0001), ("height", 9990.00999000999), ("thickness", 0.0001818923744097851)]),
        ("trapezoid", &[("bottom", 0.0001001), ("top", 10000.0), ("height", 0.0021223161290852407), ("lean", -3.7299868978068096)]),
        ("sine_band", &[("length", 0.0001), ("amplitude", 6685.441234336266), ("cycles", 100.0), ("phase", -360.0), ("thickness", 0.0008653913923270919)]),
        // (A ring a micrometre wide 10 km out: its two rings' edges, never neighbours, that near.)
        ("ring", &[("radius", 10000.0), ("inner", 9999.999999)]),
    ];
    for (word, args) in rows {
        for d in [8u32, 9] {
            let what = format!("{word} {args:?} detail {d}");
            let s = Spec::new(word, Some(Kind::Plane), args).unwrap();
            match s.build(d) {
                Err(e @ Error::Constraint { rule: "edge_under_floor", .. }) => {
                    let Error::Constraint { values, .. } = &e else { unreachable!() };
                    assert!(value(values, "gap").is_some(), "{what}: refused, and for no gap: {values:?}");
                    sound_refusal(&what, &s, d, &e);
                }
                other => panic!("{what}: {other:?}"),
            }
        }
    }
}

/// The review of stage 1a's cases, each found by drawing arguments together, with what the
/// library does with each now: the first three kinds are refused as data with their numbers
/// (and a detail that holds where one does), or build well made where the sampling was mended;
/// the catenary's closed length is no longer shorter than its chords.
#[test]
fn the_reviews_cases_are_refused_or_build() {
    // (word, arguments, detail, the rule it is refused by or "" when it builds, the detail
    // the refusal names as holding, if any)
    type Row = (&'static str, &'static [(&'static str, f64)], u32, &'static str, Option<f64>);
    let rows: &[Row] = &[
        ("gear", &[("teeth", 105.0), ("root", 0.00011284783532522105), ("tip", 1031.793784777701), ("tooth", 0.7129560673336757), ("land", 0.6323433519833985)], 8, "edge_under_floor", None),
        ("battlement_band", &[("length", 0.00010853812235728455), ("base", 8057.930403089384), ("height", 23.069361422647724), ("merlons", 10.0), ("merlon", 0.6379562031322041)], 64, "edge_under_floor", None),
        ("ring", &[("radius", 1256.5230118449845), ("inner", 0.00014468423384050833)], 256, "edge_under_floor", Some(128.0)),
        ("ring", &[("radius", 1256.5230118449845), ("inner", 0.00014468423384050833)], 128, "", None),
        ("round_arch", &[("width", 0.00011602386460976056), ("height", 8525.439259243396)], 64, "edge_under_floor", Some(32.0)),
        ("pill", &[("length", 4185.131940510725), ("radius", 0.00012452536911405925)], 256, "edge_under_floor", Some(128.0)),
        ("sine_band", &[("length", 0.0006145017350052247), ("amplitude", 0.0011813095366212962), ("cycles", 12.462733997305241), ("phase", -15.348398427860786), ("thickness", 9416.204099087514)], 8, "edge_under_floor", None),
        ("involute_band", &[("radius", 2908.7223467649874), ("turns", 18.1994108338729), ("width", 0.00013514101361150228)], 64, "edge_under_floor", None),
        ("elliptical_arc_band", &[("rx", 4.3437798963557235), ("ry", 2082.586216072282), ("angle", 279.8106240402298), ("width", 0.7800532177121826)], 8, "", None),
        ("elliptical_arc_band", &[("rx", 4.3437798963557235), ("ry", 2082.586216072282), ("angle", 279.8106240402298), ("width", 0.7800532177121826)], 16, "", None),
        ("archimedean_spiral_band", &[("start", 4978.123567788994), ("pitch", 49.35527769712236), ("turns", 17.943618443656764), ("width", 4.3715589553345415)], 8, "crosses_at_this_detail", Some(16.0)),
        ("chain_curve", &[("width", 0.00013199255086421892), ("a", 3764.1320216008635)], 8, "", None),
    ];
    for (word, args, d, rule, holds) in rows {
        let what = format!("{word} {args:?} detail {d}");
        let s = Spec::new(word, Some(Kind::Plane), args).or_else(|_| Spec::new(word, Some(Kind::Curve), args)).unwrap();
        match (s.build(*d), *rule) {
            (Ok(b), "") => {
                assert_eq!(fault(&b), None, "{what}");
                if let (Some(c), Some(x)) = (s.closed().unwrap().boundary, b.measures().boundary) {
                    assert!(x <= c, "{what}: the chords ({x}) are longer than the curve ({c})");
                }
            }
            (Err(e @ Error::Constraint { .. }), rule) if !rule.is_empty() => {
                let Error::Constraint { rule: got, values, .. } = &e else { unreachable!() };
                assert_eq!(got, &rule, "{what}");
                let named = value(values, "coarser_detail_that_holds").or(value(values, "finer_detail_that_holds"));
                assert_eq!(named, *holds, "{what}: {values:?}");
                sound_refusal(&what, &s, *d, &e);
            }
            (other, _) => panic!("{what}: {other:?}, expected {rule:?}"),
        }
    }
    // The involute band's short edge is its end cap: the band's own width.
    let s = Spec::new("involute_band", None, &[("radius", 2908.7223467649874), ("turns", 18.1994108338729), ("width", 0.00013514101361150228)]).unwrap();
    let Err(Error::Constraint { values, .. }) = s.build(8) else { panic!() };
    // (To the rounding of its ends, which lie some 3e5 m out: a few ulps of that.)
    assert!((value(&values, "edge").unwrap() - 0.00013514101361150228).abs() <= 8.0 * EPS * value(&values, "size").unwrap(), "{values:?}");
}

/// The elliptical arc band's two edges are sampled on shared rays, so no detail can make its
/// chords cross: at every detail from the least to 512, a band of every shape the drawn
/// arguments give builds (a crossing refusal here would be the sampling's fault).
#[test]
fn elliptical_arc_band_is_simple_at_every_detail() {
    let w = find("elliptical_arc_band", None).unwrap();
    let mut built = 0;
    for (label, given) in cases_together(&w, 200) {
        let s = Spec::of(w, &given).unwrap();
        for d in [8, 9, 12, 16, 24, 64, 512] {
            match s.build(d) {
                Ok(_) => built += 1,
                Err(Error::Constraint { rule, .. }) if rule != "crosses_at_this_detail" => {}
                Err(Error::TooManyPoints { .. }) => {}
                Err(e) => panic!("elliptical_arc_band {label} detail {d}: {e}"),
            }
        }
    }
    assert!(built > 500, "{built}");
}

/// Simpson's rule with `n` (even) intervals: a reference for closed forms whose integrands are
/// smooth and, at the arguments used, nearly constant over each interval (its error is
/// `h⁴ f⁗ / 180` of the span, far under the tolerances here).
fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        s += if i % 2 == 1 { 4.0 } else { 2.0 } * f(a + h * i as f64);
    }
    s * h / 3.0
}

/// The closed forms that take a difference of near-equal numbers, or `sinh`, `asinh`, `exp`
/// at small arguments, against Simpson's rule on their own integrands, at the corners of the
/// ranges where the plain forms lose digits (a shallow chain, a nearly flat parabola, a short
/// arc of a spiral far out, a thin band of a large ellipse), and at their defaults.
#[test]
fn closed_forms_keep_their_digits() {
    let rel = |what: &str, got: f64, want: f64, tol: f64| assert!((got - want).abs() <= tol * want.abs(), "{what}: {got}, the integral says {want} (off by {:e} of it)", (got - want).abs() / want.abs());
    let len_of = |name: &str, args: &[(&str, f64)]| Spec::new(name, None, args).unwrap().closed().unwrap().boundary.unwrap();
    for (w, a) in [(1e-4, 1e4), (0.00013199255086421892, 3764.1320216008635), (1.0, 0.5), (10.0, 0.5)] {
        let want = simpson(|x| crate::maths::coshm1(x / a) + 1.0, -0.5 * w, 0.5 * w, 200_000);
        rel("catenary", len_of("catenary", &[("width", w), ("a", a)]), want, 1e-13);
    }
    for (w, h) in [(1e4, 1e-4), (1.0, 0.5), (1e-4, 1e4)] {
        let c = 4.0 * h / (w * w);
        let want = simpson(|x| (1.0 + 4.0 * c * c * x * x).sqrt(), -0.5 * w, 0.5 * w, 200_000);
        rel("parabola", len_of("parabola", &[("width", w), ("height", h)]), want, 1e-12);
    }
    for (s0, p, n) in [(1e4, 1e-4, 0.25), (4978.1, 49.4, 17.9), (0.0, 1.0, 3.0), (0.05, 0.1, 3.0), (1e4, 1e4, 50.0)] {
        let b = p / TAU;
        let want = simpson(|t| ((s0 + b * t) * (s0 + b * t) + b * b).sqrt(), 0.0, TAU * n, 200_000);
        rel("archimedean spiral", len_of("archimedean_spiral", &[("start", s0), ("pitch", p), ("turns", n)]), want, 1e-12);
    }
    for (s0, g, n) in [(1.0, 1.01, 0.25), (0.05, 1.5, 3.0)] {
        let k = ln(g) / TAU;
        let want = simpson(|t| (1.0 + k * k).sqrt() * s0 * exp(k * t), 0.0, TAU * n, 20_000);
        rel("logarithmic spiral", len_of("logarithmic_spiral", &[("start", s0), ("growth", g), ("turns", n)]), want, 1e-12);
    }
    // A thin band of a circle (rx = ry): exactly 2 r h θ, h half the width.
    for (r, w, t) in [(1e4, 1e-4, 90.0), (1e4, 1e-4, 359.0), (0.5, 0.05, 200.0)] {
        let got = Spec::new("elliptical_arc_band", None, &[("rx", r), ("ry", r), ("width", w), ("angle", t)]).unwrap().closed().unwrap().size.unwrap();
        rel("elliptical_arc_band of a circle", got, r * w * rad(t), 1e-13);
    }
    // And of an ellipse, where nothing cancels: half the integral over the polar angle of the
    // difference of the squared radii.
    let (rx, ry, w, t) = (0.6, 0.35, 0.05, 250.0);
    let r2 = |a: f64, b: f64, f: f64| {
        let (s, c) = sin_cos(f);
        a * a * b * b / ((b * c) * (b * c) + (a * s) * (a * s))
    };
    let want = simpson(|f| 0.5 * (r2(rx + 0.5 * w, ry + 0.5 * w, f) - r2(rx - 0.5 * w, ry - 0.5 * w, f)), 0.0, rad(t), 20_000);
    let got = Spec::new("elliptical_arc_band", None, &[("rx", rx), ("ry", ry), ("width", w), ("angle", t)]).unwrap().closed().unwrap().size.unwrap();
    rel("elliptical_arc_band", got, want, 1e-12);
}

/// `expm1`, `ln1p`, `sinh` and `asinh` keep their digits at small arguments: against their
/// series, which at these arguments are exact to rounding (the first term left out is under
/// `1e-17` of the sum), to a few units in the last place; and against the plain forms where
/// those do not cancel.
#[test]
fn small_argument_functions_keep_their_digits() {
    let close = |what: &str, got: f64, want: f64| assert!((got - want).abs() <= 4.0 * EPS * want.abs(), "{what}: {got}, the series says {want}");
    for x in [1e-12, 3.7e-9, 1e-8, -2e-7, 1e-6, 4e-6] {
        close(&format!("expm1({x})"), expm1(x), x + x * x / 2.0 + x * x * x / 6.0);
        close(&format!("ln1p({x})"), ln1p(x), x - x * x / 2.0 + x * x * x / 3.0);
        close(&format!("sinh({x})"), sinh(x), x + x * x * x / 6.0);
        close(&format!("asinh({x})"), asinh(x), x - x * x * x / 6.0);
        close(&format!("coshm1({x})"), coshm1(x), x * x / 2.0 + x * x * x * x / 24.0);
        close(&format!("versin({x})"), versin(x), x * x / 2.0 - x * x * x * x / 24.0);
    }
    // An arc's sag where `1 − cos` rounds to nothing: a sag of the flattest lens's arc
    // (radius 1e12, a step of 2.5e-9 radians) is 3.1 µm, not 0; and the detail for an error far
    // under the radius is the one whose sag meets it.
    let x = PI / 1_256_637_062.0;
    close("the sag of a shallow step", crate::detail::sag(1e12, 1_256_637_062), 1e12 * (x * x / 2.0 - x * x * x * x / 24.0));
    assert!(crate::detail::sag(1e12, 1_256_637_062) > 3e-6);
    let n = crate::detail::detail_for(1e4, 1.0);
    assert!(crate::detail::sag(1e4, n) <= 1.0 && crate::detail::sag(1e4, n - 1) > 1.0, "detail_for(1e4, 1) = {n}");
    // Between: the series to many terms (each term `x² / ((2i)(2i + 1))` of the one before).
    for x in [0.01, 0.3, 0.9, -0.6] {
        let (mut term, mut sum) = (x, x);
        for i in 1..30 {
            term *= x * x / ((2 * i) as f64 * (2 * i + 1) as f64);
            sum += term;
        }
        close(&format!("sinh({x})"), sinh(x), sum);
        close(&format!("sinh(asinh({x}))"), sinh(asinh(x)), x);
    }
    for x in [1.5, 3.0, 20.0] {
        let e = exp(x);
        close(&format!("sinh({x})"), sinh(x), 0.5 * (e - 1.0 / e));
        close(&format!("asinh({x})"), asinh(x), ln(x + (x * x + 1.0).sqrt()));
        close(&format!("expm1({x})"), expm1(x), e - 1.0);
    }
}

/// Every form's pieces join end to end: a gap of more than a billionth of the figure's size
/// would be a mistake in the form's geometry that sampling hides.
#[test]
fn pieces_join() {
    for w in words().into_iter().filter(|w| w.form.kind != Kind::Solid) {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            let Ok(b) = s.build(DETAIL_MIN) else { continue };
            let size = size_of(&b);
            assert!(s.gap() <= size * 1e-9, "{} {label}: pieces {} apart", w.name(), s.gap());
        }
    }
}

/// Every arc's ends lie on its circle at its angles: the ends a form gives exactly
/// (`geom::arc_ends`) and its centre, radius, start angle and sweep say one arc, to `1e-9` of the
/// arc's centre's distance and radius (what the angles can carry), at the defaults, each
/// parameter at its least and greatest, and every parameter at its least or greatest together.
#[test]
fn arc_ends_lie_on_their_circles() {
    use crate::geom::{Geom, Seg};
    for w in words() {
        let params = params_of(&w);
        let mut all = cases(&w);
        let free: Vec<&Param> = params.iter().filter(|p| !p.integer).take(5).collect();
        for bits in 0..(1u32 << free.len()) {
            all.push((format!("corner {bits}"), free.iter().enumerate().map(|(k, p)| (p.name, if bits >> k & 1 == 1 { p.max } else { p.min })).collect()));
        }
        for (label, given) in all {
            let Ok(s) = Spec::of(w, &given) else { continue };
            let Ok(g) = s.geom_of() else { continue };
            let segs: Vec<Seg> = match g {
                Geom::Rings(r) => r.into_iter().flatten().collect(),
                Geom::Path { segs, .. } => segs,
                Geom::Solid(m) => m.pieces(),
            };
            for seg in segs {
                if let Seg::Arc { c, r, a0, sweep, ends } = seg {
                    let scale = len(c) + r;
                    for (end, at) in [(ends[0], a0), (ends[1], a0 + sweep)] {
                        let off = dist(end, polar(c, r, at));
                        assert!(off <= 1e-9 * scale, "{} {label}: an arc's end {end:?} is {off} from its circle's point at its angle (centre {c:?}, radius {r})", w.name());
                    }
                }
            }
        }
    }
}

/// The scale of a figure for its rounding: the greatest coordinate's size.
fn reach(b: &Built) -> f64 {
    let (lo, hi) = b.measures().bounds.unwrap();
    lo.iter().chain(hi.iter()).fold(0.0f64, |m, x| m.max(x.abs()))
}

/// Test 4: every closed-form measure against the measure of the shape as built, with a derived
/// tolerance, and, for curved forms, the error falling as the detail rule says (by four for
/// twice the detail: chords of a smooth curve err by the square of their length).
///
/// - A figure of lines and arcs only: its build's area must be the exact area less the signed
///   area between each chord and its arc, `r²(δ − sin δ)/2`, to rounding (`16 N ε R²` for `N`
///   points of reach `R`: each shoelace term rounds by a few ε R²).
/// - Any curved figure: `|closed − built| ≤ 2 h P` for the area (the region between a chord and
///   its curve lies in a strip as long as the chord and `h` wide; twice, because `h` is the
///   deviation at the middle of the parameter, not its greatest), and the area's and the
///   perimeter's errors at 256 and 512 must fall by between 3 and 5 times.
/// - A straight figure: equal to rounding.
/// - A typed textbook formula and the exact measure of the geometry, where both exist, agree
///   to rounding: so a geometry that is wrong but sampled right is caught too.
#[test]
fn t4_closed_forms_hold_against_the_build() {
    let mut checked = 0;
    for w in words().into_iter().filter(|w| w.form.kind != Kind::Solid) {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            let Ok(closed) = s.closed() else { continue };
            let what = format!("{} {label}", w.name());
            let (n1, n2) = (256, 512);
            let (Ok(b1), Ok(b2)) = (s.build(n1), s.build(n2)) else { continue };
            let (m1, m2) = (b1.measures(), b2.measures());
            let r = reach(&b2);
            let pts = b2.points() as f64;
            let round_a = 16.0 * pts * EPS * r * r;
            let round_l = 16.0 * pts * EPS * r;
            let curved = s.geom_curved();
            // The typed formula against the geometry's exact measures.
            let typed = s.typed();
            if let (Some(t), Some(c)) = (typed.size, closed.size) {
                assert!((t - c).abs() <= 1e-9 * r * r + round_a, "{what}: typed area {t} against the geometry's {c}");
                checked += 1;
            }
            if let (Some(t), Some(c)) = (typed.boundary, closed.boundary) {
                assert!((t - c).abs() <= 1e-9 * r + round_l, "{what}: typed length {t} against the geometry's {c}");
                checked += 1;
            }
            // The built area of a figure of lines and arcs, exactly.
            if let Some(expect) = s.built_area(n2) {
                let got = m2.size.unwrap();
                assert!((got - expect).abs() <= round_a, "{what}: built area {got}, the chords say {expect}");
                checked += 1;
            }
            for (name, c, x1, x2, round) in [("area", closed.size, m1.size, m2.size, round_a), ("length", closed.boundary, m1.boundary, m2.boundary, round_l)] {
                let (Some(c), Some(x1), Some(x2)) = (c, x1, x2) else { continue };
                checked += 1;
                let (e1, e2) = ((c - x1).abs(), (c - x2).abs());
                if !curved {
                    assert!(e2 <= round, "{what}: {name} closed {c}, built {x2}");
                    continue;
                }
                if name == "area" {
                    let h = b2.chord_error();
                    let p = m2.boundary.unwrap();
                    assert!(e2 <= 2.0 * h * p + round, "{what}: area closed {c}, built {x2}: off by {e2}, over 2 h P = {}", 2.0 * h * p);
                } else {
                    assert!(x2 <= c + round, "{what}: the chords are longer ({x2}) than the curve ({c})");
                }
                // The fall by four is the rule once chords are short beside the curve's radius
                // of curvature; at the parameters' extremes (an ellipse 3,500 times as long as
                // wide) 512 chords are not yet there, so the rate is held at the defaults.
                if label == "defaults" && e1 > 1000.0 * round {
                    let k = e1 / e2;
                    assert!((3.0..=5.0).contains(&k), "{what}: the {name}'s error fell by {k} from detail {n1} to {n2} ({e1} to {e2})");
                }
            }
            // The centroid: the area the chords add and take away (unsigned: a concave arc adds
            // where a convex one takes, and the two can cancel in the net area) is at most
            // 2 h P, as above; moving it by at most the figure's size D moves the centroid by at
            // most 2 (2 h P) D / A.
            if let (Some(c), Some(x), Some(a)) = (closed.centroid, m2.centroid, closed.size) {
                let da = 2.0 * b2.chord_error() * m2.boundary.unwrap() + round_a;
                let tol = 2.0 * da * size_of(&b2) / a.abs() + 1e-12 * r;
                assert!(dist([c[0], c[1]], [x[0], x[1]]) <= tol, "{what}: centroid closed {c:?}, built {x:?}");
                checked += 1;
            }
            // Bounds: what is built lies inside the closed bounds, and reaches them to within
            // the chord error (a sample can miss an extreme by at most that).
            if let (Some((lo, hi)), Some((blo, bhi))) = (closed.bounds, m2.bounds) {
                let h = 2.0 * b2.chord_error() + 1e-12 * r;
                for i in 0..2 {
                    assert!(blo[i] >= lo[i] - h && bhi[i] <= hi[i] + h, "{what}: built bounds {blo:?}..{bhi:?} outside {lo:?}..{hi:?}");
                    assert!(blo[i] <= lo[i] + h && bhi[i] >= hi[i] - h, "{what}: built bounds {blo:?}..{bhi:?} short of {lo:?}..{hi:?}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 500, "only {checked} checks");
}

/// The error at a detail: for a figure of arcs it is the sag, exactly, and the build's own
/// chord error agrees; for every curved figure the chord error falls by about four when the
/// detail doubles (the sag of a chord goes with the square of its length).
#[test]
fn error_falls_as_the_rule_says() {
    for f in forms().iter().filter(|f| f.curved && f.kind != Kind::Solid) {
        let w = Found { form: f, alias: None };
        let s = spec(&w, &[]);
        let (b1, b2) = (s.build(256).unwrap(), s.build(512).unwrap());
        if let Some(e) = s.error(512).unwrap() {
            assert!((e - b2.chord_error()).abs() <= 1e-12 * reach(&b2), "{}: the sag {e} is not the build's error {}", f.name, b2.chord_error());
        }
        let k = b1.chord_error() / b2.chord_error();
        assert!((3.0..=5.0).contains(&k), "{}: the chord error fell by {k} when the detail doubled", f.name);
    }
}

/// Test 6: names. Every form's name is unique, and no word names two things of one kind (form
/// names never repeat as aliases); every name is snake_case ASCII; every alias names a form of
/// its kind, fixes only parameters the form has with values in their ranges, and ties
/// parameters of one range; every parameter has `min <= default <= max`, a unit, a whole default
/// where it is whole, and counts are whole; a curve's band is a plane figure.
#[test]
fn t6_names_are_sound() {
    let snake = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') && !s.starts_with('_') && !s.ends_with('_') && !s.contains("__");
    let mut seen: Vec<(&str, Kind)> = Vec::new();
    for f in forms() {
        assert!(snake(f.name), "{}", f.name);
        assert_eq!(forms().iter().filter(|g| g.name == f.name).count(), 1, "{} twice", f.name);
        seen.push((f.name, f.kind));
        let mut names: Vec<&str> = f.params.iter().map(|p| p.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), f.params.len(), "{}: a parameter twice", f.name);
        for p in f.params {
            param_sound(f.name, p);
        }
        assert!(!f.about.is_empty() && !f.origin.is_empty(), "{}", f.name);
        if let Some(b) = f.band {
            assert_eq!(f.kind, Kind::Curve, "{}: only a curve has a band", f.name);
            assert_eq!(find(b, Some(Kind::Plane)).unwrap().form.kind, Kind::Plane, "{}'s band {b}", f.name);
        }
    }
    for a in aliases() {
        assert!(snake(a.name), "{}", a.name);
        assert!(!forms().iter().any(|f| f.name == a.name), "{} is a form's name and an alias", a.name);
        assert!(!seen.contains(&(a.name, a.kind)), "{} names two {}s", a.name, a.kind.name());
        seen.push((a.name, a.kind));
        let f = crate::form(a.of).unwrap_or_else(|| panic!("{} names {}, which is not a form", a.name, a.of));
        assert_eq!(f.kind, a.kind, "{}", a.name);
        for s in a.set {
            match s {
                Set::Fixed(n, v) => {
                    let p = f.param(n).unwrap_or_else(|| panic!("{} fixes {n}, which {} has not", a.name, f.name));
                    assert!(*v >= p.min && *v <= p.max && (!p.integer || floor(*v) == *v), "{}: {n} = {v} out of range", a.name);
                }
                Set::Tie { name, targets, .. } => {
                    assert!(snake(name) && !targets.is_empty(), "{}", a.name);
                    let first = f.param(targets[0]).unwrap();
                    for t in *targets {
                        let p = f.param(t).unwrap_or_else(|| panic!("{} ties {t}, which {} has not", a.name, f.name));
                        assert!((p.min, p.max, p.unit, p.integer) == (first.min, first.max, first.unit, first.integer), "{}: tied {t} has another range", a.name);
                    }
                }
            }
        }
        let w = find(a.name, Some(a.kind)).unwrap();
        for p in params_of(&w) {
            param_sound(a.name, &p);
        }
        // Its defaults build.
        spec(&w, &[]).build(DETAIL_DEFAULT).unwrap_or_else(|e| panic!("{}: {e}", a.name));
    }
}

fn param_sound(form: &str, p: &Param) {
    assert!(p.name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'), "{form}.{}", p.name);
    assert!(p.min <= p.default && p.default <= p.max, "{form}.{}: {} <= {} <= {}", p.name, p.min, p.default, p.max);
    assert!(!p.about.is_empty(), "{form}.{}", p.name);
    assert!(p.min.is_finite() && p.max.is_finite(), "{form}.{}", p.name);
    if p.integer {
        assert!(floor(p.default) == p.default && floor(p.min) == p.min && floor(p.max) == p.max, "{form}.{}", p.name);
    }
    assert_eq!(p.unit == Unit::Count, p.integer, "{form}.{}: counts, and only counts, are whole", p.name);
}

/// Kin: each name is snake_case and names a form of another kind, and kin is mutual. **The
/// kin of plane figures are solids, which stage 1b builds**: while the table has no solid, a
/// kin name that does not resolve is allowed; as soon as it has one, every kin name must
/// resolve, so this test fails in stage 1b until every solid named here is built (or its name
/// taken out).
#[test]
fn kin_resolve_and_are_mutual() {
    let solids = forms().iter().any(|f| f.kind == Kind::Solid);
    for f in forms() {
        for k in f.kin {
            assert!(k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'), "{}: kin {k}", f.name);
            // A solid's kin is a plane figure or a curve: looked for as each (a word may name
            // a solid as well, as `lens` does).
            // A plane figure may name a raised form: every figure has it, so nothing names it back.
            if f.kind == Kind::Plane && crate::raise::RAISED.contains(k) {
                continue;
            }
            let found = if f.kind == Kind::Solid { find(k, Some(Kind::Plane)).or_else(|_| find(k, Some(Kind::Curve))) } else { find(k, Some(Kind::Solid)) };
            match found {
                Ok(g) => {
                    assert_ne!(g.form.kind, f.kind, "{}: kin {k} is of its own kind", f.name);
                    let back = g.form.kin.iter().any(|x| *x == f.name || aliases().iter().any(|a| a.name == *x && a.of == f.name));
                    assert!(back, "{}: kin {k} does not name it back", f.name);
                }
                Err(_) => assert!(!solids, "{}: kin {k} is not built (stage 1b must build it or take it out)", f.name),
            }
        }
    }
}

/// Test 7: anchors lie where they say: a vertex is a point of every build; a boundary anchor
/// lies within twice the chord error of the built outline or curve; an inside one is inside;
/// a direction is a unit vector, and a small step along it leaves a plane figure while a step
/// against it enters.
#[test]
fn t7_anchors_lie_where_they_say() {
    let mut checked = 0;
    for w in words().into_iter().filter(|w| w.form.kind != Kind::Solid) {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            // A vertex must be a point of every build: an odd detail too.
            for d in [DETAIL_MIN, 77, DETAIL_DEFAULT] {
            let Ok(b) = s.build(d) else { continue };
            let anchors = s.anchors().unwrap();
            let size = size_of(&b);
            let h = 2.0 * b.chord_error() + 1e-9 * size;
            for a in anchors {
                let what = format!("{} {label}: anchor {}", w.name(), a.name);
                let p = [a.at[0], a.at[1]];
                assert!(a.at.iter().all(|x| x.is_finite()) && a.at[2] == 0.0, "{what}");
                if let Some(d) = a.dir {
                    assert!((len([d[0], d[1]]) - 1.0).abs() < 1e-12 && d[2] == 0.0, "{what}: not a unit direction");
                }
                match (&b, a.on) {
                    (_, crate::On::Free) => {}
                    (Built::Outline(o), crate::On::Vertex) => assert!(o.points().any(|q| dist(*q, p) <= 1e-9 * size), "{what}: no vertex at {p:?}"),
                    (Built::Curve(c), crate::On::Vertex) => assert!(c.points.iter().any(|q| dist(*q, p) <= 1e-9 * size), "{what}: no point at {p:?}"),
                    (Built::Outline(o), crate::On::Boundary) => assert!(o.distance_to_boundary(p) <= h, "{what}: {} from the outline", o.distance_to_boundary(p)),
                    (Built::Curve(c), crate::On::Boundary) => assert!(c.distance(p) <= h, "{what}: {} from the curve", c.distance(p)),
                    (Built::Outline(o), crate::On::Inside) => assert!(o.contains(p), "{what}: not inside"),
                    (Built::Curve(_), crate::On::Inside) => panic!("{what}: a curve has no inside"),
                    (Built::Mesh(_), _) => unreachable!("solids have their own test 7"),
                }
                // The direction is held where the figure is thicker than its build's error
                // there: at the defaults, and at every case for straight figures. (An ellipse
                // 1e-4 wide and 0.7 tall is narrower near its tip than its chords' error, so no
                // inward step from the true tip lands in the built needle.)
                let fat = label == "defaults" || b.chord_error() == 0.0;
                if let (true, Built::Outline(o), Some(d), crate::On::Vertex | crate::On::Boundary) = (fat, &b, a.dir, a.on) {
                    // A step past the anchor's own distance from the built outline (a point of
                    // the true arc lies up to the sag outside its chord), and small beside the
                    // thinnest figure the table allows (a triangle 1e-4 thick on a side of 1).
                    let e = 1e-7 * size + 2.0 * o.distance_to_boundary(p);
                    assert!(!o.contains([p[0] + e * d[0], p[1] + e * d[1]]), "{what}: a step out along its direction is still inside");
                    assert!(o.contains([p[0] - e * d[0], p[1] - e * d[1]]), "{what}: a step in against its direction is not inside");
                }
                checked += 1;
            }
            }
        }
    }
    assert!(checked > 1000, "only {checked} anchors");
}

/// Test 8: determinism. The bytes of a few builds (every coordinate's bits, in order) hash to
/// a pinned value: the same arguments give the same bytes on every machine and in every
/// build. A change to any form's geometry that moves a bit changes it.
#[test]
fn t8_builds_are_pinned() {
    let mut bytes = Vec::new();
    for (name, d) in [("heart", 64), ("superformula", 64), ("star", 8), ("archimedean_spiral_band", 100), ("moss_egg", 33), ("hypocycloid", 64), ("oval", 77), ("lemniscate", 64)] {
        let b = Spec::new(name, None, &[]).unwrap().build(d).unwrap();
        let pts: Vec<Pt> = match &b {
            Built::Outline(o) => o.points().copied().collect(),
            Built::Curve(c) => c.points.clone(),
            Built::Mesh(_) => Vec::new(),
        };
        for p in pts {
            bytes.extend_from_slice(&p[0].to_bits().to_le_bytes());
            bytes.extend_from_slice(&p[1].to_bits().to_le_bytes());
        }
    }
    let h = specula_core::hash::hash_bytes(&bytes);
    assert_eq!(h, PINNED, "the pinned builds moved: hash {h:#018x}");
}
// (Moved on 2026-11-20, from 0xc095_62ee_60d3_2fea: an arc's samples are placed from its nearer end
// (`geom::Seg::at`), not from its centre; at every form's defaults no sample moved by more than
// 1.6e-15 of its arc's centre distance and radius.)
const PINNED: u64 = 0x5828_3630_0961_f0e0;

// ---------------------------------------------------------------- particular checks

#[test]
fn gamma_holds_known_values() {
    let sp = PI.sqrt();
    for (x, want) in [(1.0, 1.0), (2.0, 1.0), (5.0, 24.0), (0.5, sp), (1.5, 0.5 * sp), (2.5, 0.75 * sp), (10.0, 362_880.0)] {
        let g = gamma(x);
        assert!((g - want).abs() <= 1e-13 * want, "Γ({x}) = {g}, not {want}");
    }
}

#[test]
fn ellipse_perimeter_of_a_circle_and_of_a_flat_ellipse() {
    assert!((ellipse_perimeter(2.0, 2.0) - 4.0 * PI).abs() < 1e-14);
    // The ellipse of semi-axes 1 and 1/2: 4.8442241102738380... (a published value, typed from
    // memory: the test against the built ellipse in t4 is the one that decides).
    let p = ellipse_perimeter(1.0, 0.5);
    assert!((p - 4.844_224_110_273_838).abs() < 1e-12, "{p}");
    assert_eq!(ellipse_perimeter(0.5, 1.0), p);
}

/// The star polygon's notches are where its edges cross: each notch lies on the line from
/// tip 0 to tip k.
#[test]
fn star_polygon_notches_lie_on_its_edges() {
    for (n, k) in [(5, 2), (6, 2), (7, 2), (7, 3), (8, 3), (12, 5), (100, 49)] {
        let r = 1.0;
        let inner = crate::forms::polygons::star_polygon_inner(n as f64, k as f64, r);
        let tip = |i: usize| polar([0.0, 0.0], r, 0.5 * PI + TAU * i as f64 / n as f64);
        let notch = polar([0.0, 0.0], inner, 0.5 * PI + PI / n as f64);
        let (a, b) = (tip(0), tip(k));
        let cross = (b[0] - a[0]) * (notch[1] - a[1]) - (b[1] - a[1]) * (notch[0] - a[0]);
        assert!(cross.abs() < 1e-12, "{{{n}/{k}}}: the notch is {cross} off the edge");
    }
}

#[test]
fn detail_for_meets_the_error_and_no_less_would() {
    for (r, e) in [(1.0, 1e-3), (0.5, 1e-4), (10.0, 0.01), (0.01, 1e-5), (2.0, 0.3)] {
        let n = crate::detail_for(r, e);
        assert!(crate::sag(r, n) <= e, "detail {n} leaves {} > {e}", crate::sag(r, n));
        if n > DETAIL_MIN {
            assert!(crate::sag(r, n - 1) > e, "detail {} would do", n - 1);
        }
    }
    assert_eq!(crate::segments(0.25, 64), 16);
    assert_eq!(crate::segments(1e-9, 64), 2);
}

/// Refusals are data: each carries its code and its numbers.
#[test]
fn refusals_carry_their_numbers() {
    match Spec::new("hexagon", None, &[("radius", -1.0)]) {
        Err(Error::OutOfRange { param: "radius", value, min, max, .. }) => assert!(value == -1.0 && min > 0.0 && max > min),
        other => panic!("{other:?}"),
    }
    match Spec::new("hexagon", None, &[("sides", 7.0)]) {
        Err(Error::FixedByAlias { alias: "hexagon", param: "sides", value }) => assert_eq!(value, 6.0),
        other => panic!("{other:?}"),
    }
    match Spec::new("circle", None, &[("radiuss", 1.0)]) {
        Err(e @ Error::UnknownParam { .. }) => {
            assert_eq!(e.code(), "unknown_param");
            let Error::UnknownParam { takes, .. } = e else { unreachable!() };
            assert_eq!(takes, vec!["radius"]);
        }
        other => panic!("{other:?}"),
    }
    match Spec::new("star", None, &[("points", 5.5)]) {
        Err(Error::NotInteger { param: "points", .. }) => {}
        other => panic!("{other:?}"),
    }
    match Spec::new("circle", None, &[("radius", 1.0), ("radius", 2.0)]) {
        Err(Error::DuplicateParam { param: "radius", .. }) => {}
        other => panic!("{other:?}"),
    }
    match Spec::new("triangle_sss", None, &[("a", 1.0), ("b", 1.0), ("c", 3.0)]).unwrap().build(DETAIL_DEFAULT) {
        Err(Error::Constraint { rule: "triangle_inequality", values, .. }) => assert_eq!(values.len(), 3),
        other => panic!("{other:?}"),
    }
    match Spec::new("archimedean_spiral_band", None, &[("turns", 50.0)]).unwrap().build(DETAIL_MAX) {
        Err(Error::TooManyPoints { points, ceiling, .. }) => assert!(points > ceiling),
        other => panic!("{other:?}"),
    }
    match Spec::new("circle", None, &[]).unwrap().build(3) {
        Err(Error::DetailOutOfRange { detail: 3, min, max }) => assert_eq!((min, max), (DETAIL_MIN, DETAIL_MAX)),
        other => panic!("{other:?}"),
    }
    match find("hexagonn", None) {
        Err(Error::UnknownForm { nearest, .. }) => assert_eq!(nearest[0], "hexagon"),
        other => panic!("{other:?}"),
    }
    // A word of one kind asked for as another.
    assert!(matches!(find("circle", Some(Kind::Curve)), Err(Error::UnknownForm { .. })));
}

/// The common words resolve to what they mean.
#[test]
fn common_words_resolve() {
    for (word, form) in [
        ("square", "rectangle"),
        ("oblong", "rectangle"),
        ("diamond", "rhombus"),
        ("lozenge", "rhombus"),
        ("trapezoid", "trapezium"),
        ("disc", "circle"),
        ("disk", "circle"),
        ("ring", "annulus"),
        ("pill", "stadium"),
        ("pie", "sector"),
        ("quadrant", "sector"),
        ("moon", "lune"),
        ("crescent", "lune"),
        ("drop", "teardrop"),
        ("egg", "moss_egg"),
        ("squircle", "superellipse"),
        ("cardioid", "limacon"),
        ("astroid", "hypocycloid"),
        ("deltoid", "hypocycloid"),
        ("nephroid", "epicycloid"),
        ("pentagram", "star_polygon"),
        ("vesica_piscis", "lens"),
        ("reuleaux_triangle", "reuleaux_polygon"),
        ("trefoil", "foil"),
    ] {
        // (Some of these words name a solid too: `disc` is also the short cylinder.)
        assert_eq!(find(word, Some(Kind::Plane)).unwrap().form.name, form, "{word}");
    }
    let sq = Spec::new("square", None, &[("size", 2.0)]).unwrap();
    assert_eq!((sq.args.f("width"), sq.args.f("height")), (2.0, 2.0));
}

/// A measure whose closed form is absent says so (`None`), and the ellipse's perimeter is
/// named a series.
#[test]
fn series_are_named() {
    let m: Measures = Spec::new("ellipse", None, &[]).unwrap().closed().unwrap();
    assert!(m.series.is_some() && m.boundary.is_some());
    assert!(Spec::new("superformula", None, &[]).unwrap().closed().unwrap().size.is_none());
}
