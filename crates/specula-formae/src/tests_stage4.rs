//! Particular checks of stage 4's forms, beside the table-driven tests that hold every form
//! (counts, builds, measures, anchors, fields, refusals at the extremes): each form's own rules
//! refused as data with their numbers, measures against an integration of the form's own
//! description (not of its pieces), the chain link's field at points known by hand, and the
//! counts a reader would check by eye.

use crate::field;
use crate::maths::*;
use crate::table::Kind;
use crate::{Built, Error, Spec};

fn refused(name: &str, kind: Kind, given: &[(&str, f64)], rule: &str) {
    match Spec::new(name, Some(kind), given).and_then(|s| s.build(32)) {
        Err(Error::Constraint { rule: r, values, .. }) => {
            assert_eq!(r, rule, "{name} {given:?}");
            for (k, v) in given {
                assert!(values.iter().any(|(n, x)| n == k && x == v), "{name}: {rule} does not carry {k} = {v}: {values:?}");
            }
        }
        other => panic!("{name} {given:?}: expected {rule}, got {:?}", other.map(|b| b.points())),
    }
}

/// Each form's own rule, at arguments that break it, is a refusal with its rule and the numbers
/// that broke it.
#[test]
fn every_new_rule_is_refused_with_its_numbers() {
    use Kind::{Plane, Solid};
    refused("cavetto", Plane, &[("width", 0.1), ("height", 0.1), ("radius", 0.1)], "radius_past_the_flats");
    refused("cyma_recta", Plane, &[("width", 0.05), ("height", 0.1), ("fillet", 0.01)], "taller_than_wide");
    refused("cyma_recta", Plane, &[("width", 6.0), ("height", 0.1), ("fillet", 0.01)], "too_flat_for_its_arcs");
    refused("cyma_recta", Plane, &[("width", 0.1), ("height", 0.1), ("fillet", 0.1)], "fillet_past_the_profile");
    refused("cyma_reversa", Plane, &[("width", 0.12), ("height", 0.1), ("fillet", 0.01)], "wider_than_tall");
    refused("cyma_reversa", Plane, &[("width", 0.01), ("height", 6.0), ("fillet", 0.001)], "too_tall_for_its_arcs");
    refused("arch_portal", Plane, &[("width", 1.0), ("opening", 1.0)], "opening_wider_than_the_wall");
    refused("arch_portal", Plane, &[("height", 2.0), ("spring", 1.6), ("opening", 1.0)], "opening_taller_than_the_wall");
    refused("grate", Plane, &[("width", 1.0), ("columns", 8.0), ("bar", 0.12)], "bars_fill_the_grate");
    refused("truss", Plane, &[("height", 0.6), ("bar", 0.3)], "bars_fill_the_truss");
    refused("truss", Plane, &[("length", 4.0), ("bays", 100.0), ("bar", 0.05)], "bars_fill_a_bay");
    refused("banner", Plane, &[("length", 1.0), ("notch", 1.0)], "notch_past_the_hoist");
    refused("leaf", Plane, &[("width", 1.0), ("widest", 0.4)], "wider_than_its_arcs_allow");
    refused("rounded_cylinder", Solid, &[("height", 0.2), ("fillet", 0.1)], "fillet_past_the_flats");
    refused("cup", Solid, &[("height", 0.5), ("floor", 0.5)], "floor_over_the_rim");
    refused("cup", Solid, &[("radius", 0.4), ("top", 0.45), ("wall", 0.45)], "wall_fills_the_cup");
    refused("bell", Solid, &[("radius", 0.5), ("crown", 0.5)], "crown_over_the_lip");
    refused("bell", Solid, &[("height", 0.6), ("wall", 0.6)], "wall_over_the_height");
    refused("bell", Solid, &[("curve", 0.55)], "flare_too_deep");
    refused("bell", Solid, &[("wall", 0.3)], "wall_too_thick_for_the_flare");
    refused("goblet", Solid, &[("radius", 0.04), ("stem", 0.04)], "stem_wider_than_cup_or_foot");
    refused("goblet", Solid, &[("wall", 0.0001), ("stem", 0.01)], "wall_too_thin_over_the_stem");
    refused("bottle", Solid, &[("radius", 0.038), ("neck", 0.04)], "neck_wider_than_the_body");
    refused("bottle", Solid, &[("shoulder", 0.01)], "shoulder_lower_than_its_step");
    refused("bottle", Solid, &[("height", 0.12), ("neck_length", 0.09), ("shoulder", 0.04)], "neck_and_shoulder_over_the_height");
    refused("vase", Solid, &[("radius", 0.12), ("foot", 0.12)], "foot_or_neck_past_the_belly");
    refused("baluster", Solid, &[("base", 0.05), ("neck", 0.05)], "neck_past_the_ball_or_plinth");
    refused("mushroom", Solid, &[("stem", 0.06), ("cap", 0.06)], "stem_wider_than_the_cap");
    refused("mushroom", Solid, &[("cap", 0.06), ("cap_height", 0.07)], "cap_past_a_half_ball");
    refused("chamfered_box", Solid, &[("height", 0.2), ("chamfer", 0.1)], "chamfer_past_the_faces");
    refused("gem", Solid, &[("sides", 3.0), ("table", 0.5)], "table_past_the_girdle");
    refused("chain_link", Solid, &[("radius", 0.03), ("wire", 0.03)], "wire_thicker_than_the_bend");
}

fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        s += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    s * h / 3.0
}

/// The turned forms' volumes against `π ∫ r(y)² dy` of their outsides less their insides, each
/// radius written from the form's own description here (not from its pieces), by Simpson's rule
/// on each smooth stretch; and the flat ones' against their textbook sums.
#[test]
fn volumes_against_their_own_integrals() {
    let vol = |name: &str| Spec::new(name, Some(Kind::Solid), &[]).unwrap().closed().unwrap().size.unwrap();
    let close = |name: &str, got: f64, want: f64| assert!((got - want).abs() <= 1e-7 * want, "{name}: closed {got}, integrated {want}");
    let disc = |r: &dyn Fn(f64) -> f64, a: f64, b: f64| PI * simpson(|y| r(y) * r(y), a, b, 4000);
    // The rounded cylinder at its defaults: radius 0.5, height 0.2, fillet 0.05.
    let (rr, h, f) = (0.5, 0.2, 0.05);
    let round = |y: f64| rr - f + (f * f - (y - f) * (y - f)).max(0.0).sqrt();
    close("rounded_cylinder", vol("rounded_cylinder"), disc(&round, 0.0, f) + PI * rr * rr * (h - 2.0 * f) + disc(&round, 0.0, f));
    // The cup: radius 0.4, top 0.45, height 0.5, wall 0.03, floor 0.04.
    let (r, t, hh, w, fl) = (0.4, 0.45, 0.5, 0.03, 0.04);
    let outer = |y: f64| r + (t - r) * y / hh;
    let inner = |y: f64| outer(y) - w;
    close("cup", vol("cup"), disc(&outer, 0.0, hh) - disc(&inner, fl, hh));
    // The mushroom: stem 0.02 to 0.06, a cap of radius 0.06 and height 0.04.
    let (s, hs, c, hc) = (0.02, 0.06, 0.06, 0.04);
    let rho = (c * c + hc * hc) / (2.0 * hc);
    let cap = |y: f64| (rho * rho - (y - (hs + hc - rho)) * (y - (hs + hc - rho))).max(0.0).sqrt();
    close("mushroom", vol("mushroom"), PI * s * s * hs + disc(&cap, hs, hs + hc));
    // The vase: a ball of 0.12 on a foot of 0.06, a neck from 0.05 to a mouth of 0.07 over 0.08.
    let (rb, fo, ne, mo, ln): (f64, f64, f64, f64, f64) = (0.12, 0.06, 0.05, 0.07, 0.08);
    let cy = (rb * rb - fo * fo).sqrt();
    let y1 = cy + (rb * rb - ne * ne).sqrt();
    let ball = |y: f64| (rb * rb - (y - cy) * (y - cy)).max(0.0).sqrt();
    let neck = |y: f64| ne + (mo - ne) * (y - y1) / ln;
    close("vase", vol("vase"), disc(&ball, 0.0, y1) + disc(&neck, y1, y1 + ln));
    // The chamfered box at its defaults (1 × 1 × 1, chamfer 0.1) by the region's own test, on a
    // grid: the cube less where |x| + |y| > 1 − c (a pair) or |x| + |y| + |z| > 1.5 − 2c, with
    // the half side 0.5. The cells' middles fall on the cutting planes (the chamfer is a whole
    // number of cells), so the count errs by half a cell's layer on them, to first order: two
    // grids, 100³ and 200³, and the error taken out (2 g₂₀₀ − g₁₀₀; it was 7e-6 when written).
    let (hb, cb) = (0.5, 0.1);
    let grid = |n: usize| {
        let mut inside = 0usize;
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let p = [(i as f64 + 0.5) / n as f64 - 0.5, (j as f64 + 0.5) / n as f64 - 0.5, (k as f64 + 0.5) / n as f64 - 0.5].map(f64::abs);
                    let pairs = p[0] + p[1] <= 2.0 * hb - cb && p[1] + p[2] <= 2.0 * hb - cb && p[0] + p[2] <= 2.0 * hb - cb;
                    if pairs && p[0] + p[1] + p[2] <= 3.0 * hb - 2.0 * cb {
                        inside += 1;
                    }
                }
            }
        }
        inside as f64 / (n * n * n) as f64
    };
    let g = 2.0 * grid(200) - grid(100);
    assert!((vol("chamfered_box") - g).abs() <= 1e-4, "chamfered_box: closed {}, grid {g}", vol("chamfered_box"));
}

/// The Koch snowflake's and the Sierpinski carpet's measures at every depth are their builds',
/// exactly (lines only), and the textbook's limits: the snowflake's area tends to 8/5 of its
/// triangle's.
#[test]
fn fractals_at_every_depth() {
    for k in 0..=5 {
        let s = Spec::new("koch_snowflake", None, &[("depth", k as f64)]).unwrap();
        let b = s.build(8).unwrap();
        assert_eq!(b.points(), 3 * 4u64.pow(k));
        let (c, m) = (s.closed().unwrap(), b.measures());
        assert!((c.size.unwrap() - m.size.unwrap()).abs() <= 1e-12, "koch {k}");
        assert!((c.boundary.unwrap() - m.boundary.unwrap()).abs() <= 1e-12 * c.boundary.unwrap(), "koch {k}");
        assert!(c.size.unwrap() < 1.6 * 3f64.sqrt() / 4.0);
    }
    for k in 1..=3 {
        let s = Spec::new("sierpinski_carpet", None, &[("depth", k as f64)]).unwrap();
        let b = s.build(8).unwrap();
        let holes = (8u64.pow(k) - 1) / 7;
        assert_eq!((b.parts(), b.points()), (1 + holes, 4 + 4 * holes), "carpet {k}");
        let (c, m) = (s.closed().unwrap(), b.measures());
        assert!((c.size.unwrap() - m.size.unwrap()).abs() <= 1e-12 && (c.boundary.unwrap() - m.boundary.unwrap()).abs() <= 1e-12, "carpet {k}");
    }
}

/// Counts a reader checks by eye: the chamfered box's 24 points and 26 faces (6 faces, 12
/// chamfers, 8 corners), the gem's `2n + 1` points and `3n + 1` faces, the crystal's, the
/// stairs' `4n + 4` points and `2n + 4` faces, the chain link's `2 (k + 1) m`.
#[test]
fn counts_by_eye() {
    let mesh = |name: &str, given: &[(&str, f64)], d: u32| match Spec::new(name, Some(Kind::Solid), given).unwrap().build(d).unwrap() {
        Built::Mesh(m) => m,
        _ => panic!(),
    };
    let m = mesh("chamfered_box", &[], 8);
    assert_eq!((m.points.len(), m.faces.len()), (24, 26));
    assert_eq!([0, 1, 2].map(|s| m.faces.iter().filter(|f| f.surface == s).count()), [6, 12, 8]);
    let m = mesh("gem", &[("sides", 8.0)], 8);
    assert_eq!((m.points.len(), m.faces.len()), (17, 25));
    let m = mesh("crystal", &[("sides", 6.0)], 8);
    assert_eq!((m.points.len(), m.faces.len()), (14, 18));
    let m = mesh("crystal", &[("sides", 6.0), ("foot", 0.0)], 8);
    assert_eq!((m.points.len(), m.faces.len()), (13, 13));
    let m = mesh("stairs", &[("steps", 5.0)], 8);
    assert_eq!((m.points.len(), m.faces.len()), (24, 14));
    let m = mesh("chain_link", &[], 64);
    assert_eq!(m.points.len(), 2 * (32 + 1) * 64);
}

/// The chain link's field where it is known by hand: `−t` on the wire's middle line, `0` on
/// its surface, the distance less `t` out from it, and at the link's middle the distance to the
/// straights less `t`.
#[test]
fn the_chain_links_field_by_hand() {
    let (r, l, t) = (0.03, 0.04, 0.008);
    let s = Spec::new("chain_link", None, &[]).unwrap();
    let f = field::of(&s, 32).unwrap();
    assert_eq!(f.how, "wire");
    let near = |p: [f64; 3], want: f64| {
        let got = f.at(p);
        assert!((got - want).abs() <= 1e-12, "at {p:?}: {got}, not {want}");
    };
    // On a straight's middle line (z = ±r), on the bends' far points (x = ±(l/2 + r)).
    near([0.0, t, r], -t);
    near([0.0, t, -r], -t);
    near([0.5 * l + r, t, 0.0], -t);
    near([0.0, 2.0 * t, r], 0.0);
    near([0.0, t, 0.0], r - t);
    near([0.5 * l + r + 0.1, t, 0.0], 0.1 - t);
    near([0.0, t + 0.2, r], 0.2 - t);
    // Inside the bend: from its centre the middle line is r away.
    near([0.5 * l, t, 0.0], r - t);
}
