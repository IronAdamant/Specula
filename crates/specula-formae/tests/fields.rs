//! Every form's distance field (`specula_formae::field`), held against a nearest point found
//! another way: a search over the form's build at a finer detail (or its outline's samples), in
//! double precision, with a sign by a ray cast, all written here and sharing no code with the
//! field. What each field must be (docs/design/FORMAE.md, "Distance fields"):
//!
//! - on a side it says is exact, the distance to within what the two builds' errors allow;
//! - on a side it says is a bound, the right sign and never more than the distance;
//! - **never over the distance anywhere** (beyond those errors);
//! - no step: along lines through every kind of region it changes by no more than the distance
//!   moved (1-Lipschitz);
//! - what a sample costs is never more than the field says, and the same field gives the same
//!   bytes twice.
//!
//! Arguments are drawn at their least, at their greatest and between, together, as every stage's
//! probes have drawn them.

use specula_core::dmath;
use specula_formae::field::{self, Cost, Field, PlaneField, Side, Truth};
use specula_formae::{forms, ops, raise, Built, Form, Kind, Mesh, Spec};

type P3 = [f64; 3];
type Pt = [f64; 2];

// ---------------------------------------------------------------- my own geometry

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A closed mesh's faces, each a flat polygon (convex or not) with its plane and a frame in it,
/// with its own nearest-point search and inside test (three rays in odd directions, the majority
/// kept: a ray through an edge is rare and never in two directions at once).
struct Truth3 {
    faces: Vec<Face3>,
    error: f64,
}

struct Face3 {
    pts: Vec<P3>,
    n: P3,
    o: P3,
    u: P3,
    v: P3,
    flat: Vec<Pt>,
}

impl Face3 {
    fn new(pts: Vec<P3>) -> Face3 {
        // (Newell's normal; the frame from the longest edge.)
        let k = pts.len();
        let mut n = [0.0; 3];
        for i in 0..k {
            let (a, b) = (pts[i], pts[(i + 1) % k]);
            n = [n[0] + (a[1] - b[1]) * (a[2] + b[2]), n[1] + (a[2] - b[2]) * (a[0] + b[0]), n[2] + (a[0] - b[0]) * (a[1] + b[1])];
        }
        let l = dot(n, n).sqrt();
        let n = n.map(|x| x / l);
        let o = pts[0];
        let e = (0..k).map(|i| sub(pts[(i + 1) % k], pts[i])).fold([0.0; 3], |a, b| if dot(b, b) > dot(a, a) { b } else { a });
        let le = dot(e, e).sqrt();
        let u = e.map(|x| x / le);
        let v = cross(n, u);
        let flat = pts.iter().map(|p| [dot(sub(*p, o), u), dot(sub(*p, o), v)]).collect();
        Face3 { pts, n, o, u, v, flat }
    }
    fn holds(&self, q: Pt) -> bool {
        let mut inside = false;
        let r = &self.flat;
        for i in 0..r.len() {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                inside = !inside;
            }
        }
        inside
    }
    fn d2(&self, p: P3) -> f64 {
        let h = dot(sub(p, self.o), self.n);
        let q = [dot(sub(p, self.o), self.u), dot(sub(p, self.o), self.v)];
        if self.holds(q) {
            return h * h;
        }
        let k = self.pts.len();
        (0..k).map(|i| seg_d2(p, self.pts[i], self.pts[(i + 1) % k])).fold(f64::INFINITY, f64::min)
    }
    fn hit(&self, p: P3, dir: P3) -> bool {
        let den = dot(dir, self.n);
        if den.abs() < 1e-300 {
            return false;
        }
        let t = dot(sub(self.o, p), self.n) / den;
        if t <= 0.0 {
            return false;
        }
        let x = [p[0] + t * dir[0], p[1] + t * dir[1], p[2] + t * dir[2]];
        self.holds([dot(sub(x, self.o), self.u), dot(sub(x, self.o), self.v)])
    }
}

fn seg_d2(p: P3, a: P3, b: P3) -> f64 {
    let ab = sub(b, a);
    let l = dot(ab, ab);
    let t = if l > 0.0 { (dot(sub(p, a), ab) / l).clamp(0.0, 1.0) } else { 0.0 };
    let d = sub(p, [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]]);
    dot(d, d)
}

impl Truth3 {
    fn of(m: &Mesh) -> Truth3 {
        let faces = m.faces.iter().map(|f| Face3::new(f.verts.iter().map(|&i| m.points[i as usize]).collect())).collect();
        Truth3 { faces, error: m.chord_error }
    }
    fn signed(&self, p: P3) -> f64 {
        let d = self.faces.iter().map(|f| f.d2(p)).fold(f64::INFINITY, f64::min).sqrt();
        let dirs = [[0.5773, 0.3141, 0.7536], [-0.2718, 0.8284, -0.4899], [0.6180, -0.7062, 0.3437]];
        let inside = dirs.iter().filter(|&&dir| self.faces.iter().filter(|f| f.hit(p, dir)).count() % 2 == 1).count() >= 2;
        if inside {
            -d
        } else {
            d
        }
    }
}

/// An outline's rings, with my own nearest-point search and a crossing count by interpolation.
struct Truth2 {
    rings: Vec<Vec<Pt>>,
    error: f64,
}

impl Truth2 {
    fn signed(&self, p: Pt) -> f64 {
        let mut best = f64::INFINITY;
        let mut inside = false;
        for r in &self.rings {
            for i in 0..r.len() {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l = dx * dx + dy * dy;
                let t = if l > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0) } else { 0.0 };
                let (qx, qy) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
                best = best.min(qx * qx + qy * qy);
                if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / dy * dx {
                    inside = !inside;
                }
            }
        }
        if inside {
            -best.sqrt()
        } else {
            best.sqrt()
        }
    }
}

// ---------------------------------------------------------------- drawing arguments and points

/// A small fixed sequence (splitmix64), the same on every machine.
struct Seq(u64);
impl Seq {
    fn draw(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A form's arguments: its defaults, then `sets` drawn together (each parameter at its least,
/// its greatest, or between, a count whole), those the form refuses left out.
fn argument_sets(f: &Form, sets: usize, seq: &mut Seq) -> Vec<Vec<(&'static str, f64)>> {
    let mut out = vec![Vec::new()];
    for _ in 0..sets {
        let mut given = Vec::new();
        for p in f.params {
            let u = seq.draw();
            let v = if u < 0.3 {
                p.min
            } else if u < 0.6 {
                p.max
            } else {
                // (Between, spread over the decades of a wide range.)
                let t = seq.draw();
                if p.min > 0.0 && p.max / p.min > 100.0 {
                    dmath::exp_f64(dmath::ln_f64(p.min) + t * (dmath::ln_f64(p.max) - dmath::ln_f64(p.min))).clamp(p.min, p.max)
                } else {
                    p.min + (p.max - p.min) * t
                }
            };
            let v = if p.integer { dmath::round_f64(v).clamp(p.min, p.max) } else { v };
            given.push((p.name, v));
        }
        out.push(given);
    }
    out
}

/// Points about a box: a grid through it and past it, its centre and its axis, points near the
/// given surface points on both sides, and drawn ones.
fn points_about(lo: P3, hi: P3, near: &[P3], seq: &mut Seq, n: usize) -> Vec<P3> {
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f64, f64::max).max(1e-9);
    let mid = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), 0.5 * (lo[2] + hi[2])];
    let mut pts = vec![mid, [0.0, mid[1], 0.0], [0.0, lo[1] + 0.25 * (hi[1] - lo[1]), 0.0]];
    for i in 0..n {
        let t = [seq.draw(), seq.draw(), seq.draw()];
        pts.push([0, 1, 2].map(|k| lo[k] - 0.3 * size + (hi[k] - lo[k] + 0.6 * size) * t[k]));
        if let Some(q) = near.get((i * 7919) % near.len().max(1)) {
            let s = (seq.draw() - 0.5) * 0.02 * size;
            pts.push([q[0] + s, q[1] - s, q[2] + s]);
        }
    }
    pts
}

// ---------------------------------------------------------------- the checks

/// How far apart the field and my search may be: the reference's chord error, the field's own
/// sampling (twice its error: DISTANCE_FIELDS.md 4.4; four times an estimate), and rounding.
fn slack(truth: Truth, reference: f64, reach: f64) -> f64 {
    let own = match truth {
        Truth::Form => 0.0,
        Truth::Sampled { error, bound: true, .. } => 2.0 * error,
        Truth::Sampled { error, bound: false, .. } => 4.0 * error,
    };
    reference + own + 1e-9 * reach
}

/// Holds a field to a reference signed distance at each point: the sign wherever the reference
/// is clear of the slack, exact sides to within the slack, bound sides never over.
#[allow(clippy::too_many_arguments, reason = "a field and its reference, each with what it says of itself")]
fn hold(what: &str, f: &dyn Fn(P3) -> f64, sides: field::Sides, truth: Truth, refd: &dyn Fn(P3) -> f64, ref_err: f64, pts: &[P3], reach: f64) {
    let tol = slack(truth, ref_err, reach);
    for &p in pts {
        let (got, want) = (f(p), refd(p));
        assert!(got.is_finite(), "{what}: not finite at {p:?}");
        assert!(got.abs() <= want.abs() + tol, "{what}: over the distance at {p:?}: field {got}, distance {want}, slack {tol}");
        if want.abs() > tol {
            assert!(got.signum() == want.signum(), "{what}: wrong sign at {p:?}: field {got}, distance {want}");
            let side = if want > 0.0 { sides.outside } else { sides.inside };
            if side == Side::Exact {
                assert!((got - want).abs() <= tol, "{what}: not exact at {p:?}: field {got}, distance {want}, slack {tol}");
            }
        }
    }
}

/// No step: along lines through the box (its axes, its diagonals, lines through drawn points in
/// drawn directions), the field never changes by more than the distance moved.
fn no_step(what: &str, f: &dyn Fn(P3) -> f64, lo: P3, hi: P3, seq: &mut Seq) {
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f64, f64::max).max(1e-9);
    let mid = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), 0.5 * (lo[2] + hi[2])];
    let mut lines: Vec<(P3, P3)> = vec![(mid, [1.0, 0.0, 0.0]), (mid, [0.0, 1.0, 0.0]), (mid, [0.0, 0.0, 1.0]), ([0.0, mid[1], 0.0], [0.6, 0.0, 0.8]), (mid, [0.577_350_269, 0.577_350_269, 0.577_350_269])];
    for _ in 0..4 {
        let o = [0, 1, 2].map(|k| lo[k] + (hi[k] - lo[k]) * seq.draw());
        let d = [seq.draw() - 0.5, seq.draw() - 0.5, seq.draw() - 0.5];
        let l = dot(d, d).sqrt().max(1e-12);
        lines.push((o, d.map(|x| x / l)));
    }
    let steps = 200;
    let h = 1.6 * size / steps as f64;
    for (o, d) in lines {
        let at = |i: i32| [0, 1, 2].map(|k| o[k] + d[k] * h * f64::from(i));
        let mut prev = f(at(-steps / 2));
        for i in (-steps / 2 + 1)..=(steps / 2) {
            let v = f(at(i));
            assert!((v - prev).abs() <= h * (1.0 + 1e-9) + 1e-12 * size, "{what}: a step of {} over {h} along {d:?} from {o:?} at {i}", v - prev);
            prev = v;
        }
    }
}

fn reach_of(lo: P3, hi: P3) -> f64 {
    (0..3).map(|k| lo[k].abs().max(hi[k].abs())).fold(0.0f64, f64::max)
}

/// One solid's field held against its build at `fine`.
fn check_solid(what: &str, fl: &Field, fine: &Mesh, seq: &mut Seq, n: usize) {
    let truth = Truth3::of(fine);
    let (lo, hi) = fl.bounds;
    // The box holds the solid: every point of the reference is in it (within the errors).
    let reach = reach_of(lo, hi);
    let tol = slack(fl.truth, truth.error, reach);
    for p in &fine.points {
        assert!((0..3).all(|k| p[k] >= lo[k] - tol && p[k] <= hi[k] + tol), "{what}: {p:?} is out of the box {lo:?} {hi:?}");
    }
    let near: Vec<P3> = fine.points.iter().step_by((fine.points.len() / 64).max(1)).copied().collect();
    let pts = points_about(lo, hi, &near, seq, n);
    let mut cost = Cost::default();
    let f = |p: P3| {
        let mut c = Cost::default();
        let v = fl.at_counted(p, &mut c);
        assert!(c.within(&fl.cost), "{what}: a sample cost {c:?}, over {:?}", fl.cost);
        v
    };
    hold(what, &f, fl.sides, fl.truth, &|p| truth.signed(p), truth.error, &pts, reach);
    no_step(what, &f, lo, hi, seq);
    // The same bytes twice.
    for p in &pts {
        assert_eq!(fl.at(*p).to_bits(), fl.at_counted(*p, &mut cost).to_bits(), "{what}");
    }
}

fn fine_detail(fl: &Field, detail: u32) -> u32 {
    match fl.truth {
        Truth::Form => 4 * detail,
        // (The field is of a build or of samples at `detail`: the same build is its truth.)
        Truth::Sampled { .. } => detail,
    }
}

fn mesh_of(b: Built) -> Mesh {
    match b {
        Built::Mesh(m) => m,
        _ => panic!("not a solid"),
    }
}

/// Every solid word at its defaults and at argument sets drawn together, against its build.
fn solids(sets: usize, detail: u32, n: usize) {
    let mut seq = Seq(5);
    let mut at_defaults = 0;
    let all = forms().iter().filter(|f| f.kind == Kind::Solid).count();
    for f in forms().iter().filter(|f| f.kind == Kind::Solid) {
        for given in argument_sets(f, sets, &mut seq) {
            at_defaults += usize::from(given.is_empty());
            let Ok(spec) = Spec::new(f.name, Some(Kind::Solid), &given) else { continue };
            let what = format!("{} {given:?} at {detail}", f.name);
            let fl = match field::of(&spec, detail) {
                Ok(fl) => fl,
                Err(e) => {
                    // A refusal is data: a rule and its numbers; a field too large names a detail.
                    // (Every word has a field at its defaults: counted below.)
                    // (At its defaults only a build's field too large for this detail, which names
                    // a coarser one.)
                    let too_large = matches!(&e, specula_formae::Error::Constraint { rule: "field_too_large", values, .. } if values.iter().any(|v| v.0 == "coarser_detail_that_fits"));
                    assert!(matches!(e, specula_formae::Error::Constraint { .. } | specula_formae::Error::TooManyPoints { .. }) && (!given.is_empty() || too_large), "{what}: {e:?}");
                    continue;
                }
            };
            let fine = fine_detail(&fl, detail).min(256);
            let b = match spec.build(fine) {
                Ok(b) => b,
                Err(e) => {
                    assert!(!given.is_empty(), "{what}: no reference at {fine}: {e:?}");
                    continue;
                }
            };
            check_solid(&what, &fl, &mesh_of(b), &mut seq, n);
        }
    }
    assert_eq!(at_defaults, all, "every solid word at its defaults");
}

/// Every plane figure at its defaults and drawn sets, against its outline at a finer detail.
fn figures(sets: usize, detail: u32, n: usize) {
    let mut seq = Seq(11);
    for f in forms().iter().filter(|f| f.kind == Kind::Plane) {
        for given in argument_sets(f, sets, &mut seq) {
            let Ok(spec) = Spec::new(f.name, Some(Kind::Plane), &given) else { continue };
            let what = format!("{} {given:?} at {detail}", f.name);
            let fl = match field::plane(&spec, detail) {
                Ok(fl) => fl,
                Err(e) => {
                    // (Refused only where the figure is not built at this detail, as data.)
                    assert!(!given.is_empty() && spec.build(detail).is_err(), "{what}: {e:?}");
                    continue;
                }
            };
            let fine = match fl.truth {
                Truth::Form => 4 * detail,
                Truth::Sampled { .. } => detail,
            };
            let Ok(Built::Outline(o)) = spec.build(fine) else {
                assert!(!given.is_empty(), "{what}: no reference at {fine}");
                continue;
            };
            check_plane(&what, &fl, &Truth2 { rings: o.rings.clone(), error: o.chord_error }, &mut seq, n);
        }
    }
}

fn check_plane(what: &str, fl: &PlaneField, truth: &Truth2, seq: &mut Seq, n: usize) {
    let (lo, hi) = fl.bounds;
    let (lo3, hi3) = ([lo[0], lo[1], 0.0], [hi[0], hi[1], 0.0]);
    let near: Vec<P3> = truth.rings.iter().flatten().step_by(7).map(|p| [p[0], p[1], 0.0]).collect();
    let mut pts = points_about(lo3, hi3, &near, seq, n);
    // (Every corner of the outline, and the line through it along +x: where a ray meets a vertex.)
    for p in truth.rings.iter().flatten().step_by(3).take(64) {
        pts.push([p[0], p[1], 0.0]);
        pts.push([p[0] - 0.1 * (hi[0] - lo[0]), p[1], 0.0]);
    }
    let pts: Vec<P3> = pts.into_iter().map(|p| [p[0], p[1], 0.0]).collect();
    let reach = reach_of(lo3, hi3);
    let f = |p: P3| {
        let mut c = Cost::default();
        let v = fl.at_counted([p[0], p[1]], &mut c);
        assert!(c.within(&fl.cost), "{what}: a sample cost {c:?}, over {:?}", fl.cost);
        v
    };
    hold(what, &f, fl.sides, fl.truth, &|p| truth.signed([p[0], p[1]]), truth.error, &pts, reach);
    // No step along lines in the plane.
    let flat = |p: P3| f([p[0], p[1], 0.0]);
    let mut lo2 = lo3;
    let mut hi2 = hi3;
    lo2[2] = 0.0;
    hi2[2] = 0.0;
    no_step(what, &flat, lo2, hi2, seq);
}

#[test]
fn every_solid_at_its_defaults_and_drawn_arguments() {
    solids(1, 16, 40);
}

#[test]
#[ignore = "full run: cargo test --release -p specula-formae --test fields -- --ignored"]
fn every_solid_full() {
    solids(6, 16, 120);
    solids(2, 33, 60);
}

#[test]
fn every_plane_figure_at_its_defaults_and_drawn_arguments() {
    figures(2, 24, 60);
}

#[test]
#[ignore = "full run: cargo test --release -p specula-formae --test fields -- --ignored"]
fn every_plane_figure_full() {
    figures(12, 24, 150);
    figures(4, 9, 100);
}

/// Every plane figure raised as a slab and as a relief, and the figures of one ring turned
/// (whole and through part of a turn), against their builds.
fn raised_and_turned(sets: usize, n: usize) {
    let mut seq = Seq(17);
    let detail = 16;
    for f in forms().iter().filter(|f| f.kind == Kind::Plane) {
        for given in argument_sets(f, sets, &mut seq) {
            let Ok(spec) = Spec::new(f.name, Some(Kind::Plane), &given) else { continue };
            let size = spec.closed().ok().and_then(|m| m.bounds).map_or(1.0, |(lo, hi)| (hi[0] - lo[0]).max(hi[1] - lo[1]));
            let mut made: Vec<(String, ops::Solid)> = Vec::new();
            if let Ok(s) = raise::slab(&spec, 0.3 * size) {
                made.push(("slab".into(), s));
            }
            if let Ok(s) = raise::relief(&spec, 0.2 * size, 0.1 * size, 0.1 * size) {
                made.push(("relief".into(), s));
            }
            if let Some((lo, _)) = spec.closed().ok().and_then(|m| m.bounds) {
                for angle in [360.0, 90.0, 250.0] {
                    if let Ok(s) = ops::revolve_of(&spec, 0.3 * size - lo[0], angle) {
                        made.push((format!("turned {angle}"), s));
                    }
                }
            }
            for (how, solid) in made {
                let what = format!("{} {given:?} {how}", f.name);
                let Ok(fl) = field::of_solid(&solid, detail) else { continue };
                let fine = match fl.truth {
                    Truth::Form => 64,
                    Truth::Sampled { .. } => detail,
                };
                let Ok(m) = solid.build(fine) else { continue };
                check_solid(&what, &fl, &m, &mut seq, n);
            }
        }
    }
}

#[test]
fn every_figure_raised_and_turned() {
    raised_and_turned(0, 20);
}

#[test]
#[ignore = "full run: cargo test --release -p specula-formae --test fields -- --ignored"]
fn every_figure_raised_and_turned_full() {
    raised_and_turned(3, 40);
}

/// **A figure that touches the axis along a stretch** (DISTANCE_FIELDS.md 4.2's second test): a
/// cylinder's profile runs along the axis, and the field on the axis is the depth to the nearer
/// of the side and the caps, not zero; a hemisphere's centre is its radius deep less nothing.
#[test]
fn the_axis_is_not_a_surface() {
    let cyl = field::of(&Spec::new("cylinder", Some(Kind::Solid), &[("radius", 0.5), ("height", 3.0)]).unwrap(), 32).unwrap();
    assert_eq!((cyl.how, cyl.sides.inside, cyl.truth), ("turned", Side::Exact, Truth::Form));
    for (y, want) in [(1.5, -0.5), (0.2, -0.2), (2.9, -0.1), (1.0, -0.5)] {
        let got = cyl.at([0.0, y, 0.0]);
        assert!((got - want).abs() < 1e-12, "cylinder at y = {y}: {got}, not {want}");
    }
    let ball = field::of(&Spec::new("sphere", Some(Kind::Solid), &[("radius", 2.0)]).unwrap(), 16).unwrap();
    for (p, want) in [([0.0, 2.0, 0.0], -2.0), ([0.0, 5.0, 0.0], 1.0), ([3.0, 2.0, 4.0], 3.0), ([0.0, 0.0, 0.0], 0.0)] {
        assert!((ball.at(p) - want).abs() < 1e-12, "sphere at {p:?}: {}", ball.at(p));
    }
}

/// **The sign is right at a vertex, on an edge and for a ring with holes**: a square, a ray
/// through two of its corners at once, an annulus's hole, a frame's hole, and an arc whose
/// chord a point sits on.
#[test]
fn the_sign_is_right_at_vertices_edges_and_holes() {
    let sq = field::plane(&Spec::new("rectangle", Some(Kind::Plane), &[("width", 2.0), ("height", 2.0)]).unwrap(), 8).unwrap();
    for (p, want) in [([0.0, 0.0], -1.0), ([-0.5, 1.0], 0.0), ([1.0, 1.0], 0.0), ([-3.0, 1.0], 2.0), ([-3.0, -1.0], 2.0), ([0.0, 1.0 - 1e-15], -1e-15), ([2.0, 2.0], 2f64.sqrt())] {
        let got = sq.at(p);
        assert!((got - want).abs() < 1e-14, "square at {p:?}: {got}, not {want}");
    }
    let ring = field::plane(&Spec::new("annulus", Some(Kind::Plane), &[("radius", 1.0), ("inner", 0.5)]).unwrap(), 8).unwrap();
    for (p, want) in [([0.0, 0.0], 0.5), ([0.75, 0.0], -0.25), ([0.0, -0.75], -0.25), ([2.0, 0.0], 1.0), ([0.5_f64.sqrt() * 0.75, 0.5_f64.sqrt() * 0.75], -0.25)] {
        let got = ring.at(p);
        assert!((got - want).abs() < 1e-12, "annulus at {p:?}: {got}, not {want}");
    }
    // (A circle's quarter arcs' chords make a diamond inside it: a point on a chord is inside.)
    let circle = field::plane(&Spec::new("circle", Some(Kind::Plane), &[("radius", 1.0)]).unwrap(), 8).unwrap();
    for p in [[0.5, 0.5], [-0.5, 0.5], [0.25, 0.75], [0.0, 1.0 - 1e-9]] {
        let p: Pt = p;
        let want = (p[0] * p[0] + p[1] * p[1]).sqrt() - 1.0;
        assert!((circle.at(p) - want).abs() < 1e-12, "circle at {p:?}: {}, not {want}", circle.at(p));
    }
}

/// **A convex solid's field looks at its faces, and a sample never costs more than the field
/// says**; the dearest fields of the table, by count, at the default detail.
#[test]
fn costs_are_counted() {
    let ico = field::of(&Spec::new("icosahedron", Some(Kind::Solid), &[]).unwrap(), 64).unwrap();
    assert_eq!((ico.how, ico.cost.pieces, ico.cost.roots, ico.cost.trig), ("convex_build", 20 + 60, 1, 0));
    let mut c = Cost::default();
    ico.at_counted([0.0, 0.3, 0.0], &mut c);
    assert_eq!(c.pieces, 20, "inside, a convex solid looks at its planes only");
    for row in field::table(64) {
        assert!(row.cost.pieces <= field::MAX_PLANE_PIECES + 4 * field::MAX_MESH_TRIANGLES, "{}: {:?}", row.name, row.cost);
    }
}

/// The table covers every form, and every form has a field or a refusal that is a rule.
#[test]
fn every_form_has_a_field_or_a_rule() {
    let rows = field::table(64);
    assert_eq!(rows.len(), forms().len());
    for r in &rows {
        match (r.kind, r.refused) {
            (Kind::Curve, refused) => assert_eq!(refused, Some("a_curve_has_no_inside"), "{}", r.name),
            (_, None) => {}
            // (A solid whose field is its build's may be too large at 64; at a coarser detail it
            // has one: the refusal names it, and the field there is made.)
            (Kind::Solid, Some("field_too_large")) => {
                let spec = Spec::new(r.name, Some(Kind::Solid), &[]).unwrap();
                let Err(specula_formae::Error::Constraint { values, .. }) = field::of(&spec, 64) else { panic!("{}", r.name) };
                let d = values.iter().find(|v| v.0 == "coarser_detail_that_fits").unwrap_or_else(|| panic!("{}: {values:?}", r.name)).1;
                let fl = field::of(&spec, d as u32).unwrap_or_else(|e| panic!("{} at {d}: {e:?}", r.name));
                assert_eq!(fl.how, "mesh_build", "{}", r.name);
            }
            (_, refused) => panic!("{}: refused as {refused:?}", r.name),
        }
    }
}

// ---------------------------------------------------------------- the table in FORMAE.md

const BEGIN: &str = "<!-- fields-table: begin -->";
const END: &str = "<!-- fields-table: end -->";

fn document() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/FORMAE.md")
}

fn side(s: Side) -> &'static str {
    match s {
        Side::Exact => "exact",
        Side::Bound => "bound",
    }
}

/// Every form's field at its defaults and the default detail, made from the crate.
fn fields_table() -> String {
    let mut out = String::from("\nEvery form at its defaults and detail 64 (made from the crate; held by `crates/specula-formae/tests/fields.rs`, written again by its ignored `write_the_fields_table`). Cost: pieces, roots and trigonometric calls a sample takes at most.\n\n| Form | Kind | Field | Outside | Inside | Of | Cost |\n|---|---|---|---|---|---|---|\n");
    for r in field::table(64) {
        let (o, i) = r.sides.map_or(("-", "-"), |s| (side(s.outside), side(s.inside)));
        let of = match r.truth {
            None => format!("refused: `{}`", r.refused.unwrap_or("-")),
            Some(Truth::Form) => "the form".to_string(),
            Some(Truth::Sampled { bound: true, .. }) => "samples (error a bound)".to_string(),
            Some(Truth::Sampled { bound: false, .. }) => "samples (error estimated)".to_string(),
        };
        let cost = if r.refused.is_some() { "-".to_string() } else { format!("{} / {} / {}", r.cost.pieces, r.cost.roots, r.cost.trig) };
        out.push_str(&format!("| `{}` | {} | {} | {o} | {i} | {of} | {cost} |\n", r.name, r.kind.name(), r.how));
    }
    out
}

fn split(doc: &str) -> (&str, &str, &str) {
    let b = doc.find(BEGIN).expect("FORMAE.md has the fields table's first marker");
    let e = doc.find(END).expect("FORMAE.md has the fields table's last marker");
    (&doc[..b + BEGIN.len()], &doc[b + BEGIN.len()..e], &doc[e..])
}

#[test]
fn the_documents_fields_table_is_the_crates() {
    let doc = std::fs::read_to_string(document()).expect("docs/design/FORMAE.md").replace("\r\n", "\n");
    let (_, between, _) = split(&doc);
    let want = fields_table();
    if between.trim() != want.trim() {
        let first = between.trim().lines().zip(want.trim().lines()).find(|(a, b)| a != b);
        panic!("FORMAE.md's fields table is not the crate's (first difference: {first:?}): write it again with `cargo test -p specula-formae --test fields write_the_fields_table -- --ignored`");
    }
}

#[test]
#[ignore = "writes the fields table into docs/design/FORMAE.md (run when a form or a field changes)"]
fn write_the_fields_table() {
    let path = document();
    let doc = std::fs::read_to_string(&path).expect("docs/design/FORMAE.md").replace("\r\n", "\n");
    let (head, _, tail) = split(&doc);
    std::fs::write(&path, format!("{head}\n{}\n{tail}", fields_table().trim())).expect("written");
}

/// **A shallow arc keeps the digits of its own size** (the lens of review 1b: rim radius
/// 4,785.8 m, a tenth of a millimetre thick, its faces' centres 2.3e11 m away): on the axis the
/// field is the half thickness deep and a metre above it reads a metre; at the rim it reads none.
/// From the centre (`|p − c| − r`) each would be off by about 5e-5 m, half the lens.
#[test]
fn a_shallow_arc_keeps_its_digits() {
    let (r, t) = (4785.767163963165, 1e-4);
    let lens = field::of(&Spec::new("biconvex_lens", Some(Kind::Solid), &[("radius", r), ("thickness", t)]).unwrap(), 8).unwrap();
    assert_eq!((lens.how, lens.truth), ("turned", Truth::Form));
    for (p, want, tol) in [([0.0, 0.5 * t, 0.0], -0.5 * t, 1e-12), ([0.0, t + 1.0, 0.0], 1.0, 1e-12), ([0.0, -2.0, 0.0], 2.0, 1e-12), ([r, 0.5 * t, 0.0], 0.0, 1e-9), ([r + 1.0, 0.5 * t, 0.0], 1.0, 1e-9)] {
        let got = lens.at(p);
        assert!((got - want).abs() <= tol, "lens at {p:?}: {got}, not {want}");
    }
}
