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
fn words() -> Vec<Found> {
    let mut v: Vec<Found> = forms().iter().map(|f| Found { form: f, alias: None }).collect();
    for a in aliases() {
        v.push(find(a.name, Some(a.kind)).expect("every alias resolves"));
    }
    v
}

/// The cases of a word: its defaults, then each parameter at its least and its greatest.
fn cases(found: &Found) -> Vec<(String, Vec<(&'static str, f64)>)> {
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
#[test]
fn t1_every_form_builds_at_its_extremes() {
    let mut built = 0;
    for w in words() {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            for d in [DETAIL_MIN, DETAIL_DEFAULT, DETAIL_MAX] {
                let what = format!("{} {label} detail {d}", w.name());
                match s.build(d) {
                    Ok(b) => {
                        built += 1;
                        let c = s.count(d).unwrap();
                        assert_eq!((c.points, c.parts), (b.points(), b.parts()), "{what}: the count before building is not what was built");
                        let pts: Vec<Pt> = match &b {
                            Built::Outline(o) => o.points().copied().collect(),
                            Built::Curve(c) => c.points.clone(),
                        };
                        assert!(pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()), "{what}: a point is not finite");
                        assert!(b.chord_error().is_finite(), "{what}");
                    }
                    Err(Error::Constraint { .. }) if label != "defaults" => {}
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
    for w in words() {
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
                }
            }
        }
    }
}

/// Every form's pieces join end to end: a gap of more than a billionth of the figure's size
/// would be a mistake in the form's geometry that sampling hides.
#[test]
fn pieces_join() {
    for w in words() {
        for (label, given) in cases(&w) {
            let s = spec(&w, &given);
            let Ok(b) = s.build(DETAIL_MIN) else { continue };
            let size = size_of(&b);
            assert!(s.gap() <= size * 1e-9, "{} {label}: pieces {} apart", w.name(), s.gap());
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
    for w in words() {
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
    for f in forms().iter().filter(|f| f.curved) {
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
            let other = if f.kind == Kind::Solid { None } else { Some(Kind::Solid) };
            match find(k, other) {
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
    for w in words() {
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
        };
        for p in pts {
            bytes.extend_from_slice(&p[0].to_bits().to_le_bytes());
            bytes.extend_from_slice(&p[1].to_bits().to_le_bytes());
        }
    }
    let h = specula_core::hash::hash_bytes(&bytes);
    assert_eq!(h, PINNED, "the pinned builds moved: hash {h:#018x}");
}
const PINNED: u64 = 0xc095_62ee_60d3_2fea;

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
        assert_eq!(find(word, None).unwrap().form.name, form, "{word}");
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
