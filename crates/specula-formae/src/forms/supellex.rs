//! Solids of stage 4: a rounded cylinder, hollow vessels (a cup, a bell, a goblet), turned props
//! (a bottle, a vase, a baluster, a mushroom), a chamfered box, a crystal, a cut stone, a flight
//! of stairs and a chain link.
//!
//! The turned ones are [`Rev`]s: a profile of lines and arcs from the axis to the axis, so their
//! measures come from the profile exactly, their build is proved not to cross itself by the
//! profile's own check (a simple ring off the axis but at its ends), their error is a bound and
//! their field is the turned profile's, exact. A hollow vessel is still one open profile from the
//! axis to the axis: in from the floor's middle, out along the inside, over the rim and down the
//! outside (or the other way), so the cavity is part of the profile and nothing is subtracted.
//! The flat ones are [`Poly`]s: their measures are the build's, exactly. The chain link is a
//! round wire along a stadium: two half tori and two cylinders, every quad planar.

use crate::anchor::{at3, out3, Anchor, On, MAX_NUMBERED};
use crate::detail::segments;
use crate::error::{refuse, Refusal};
use crate::field::Plan;
use crate::geom::{arc, arc_ends, line, Geom, Seg};
use crate::maths::*;
use crate::mesh::{Face, Mesh, Surface};
use crate::solid::{piece, Gen, Piece, Poly, Rev, Ring};
use crate::spec::Args;
use crate::table::{count, len, ratio, Alias, Form, Param};
use crate::Measures;
use std::rc::Rc;

const O: Pt = [0.0, 0.0];

/// A full turn of an open profile (from the axis to the axis).
fn turned(pieces: Vec<Piece>) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(Rev { pieces, closed: false, sweep: TAU, ring: Ring::Circle, shift: O, cut: "cut" })))
}

fn m(size: Option<f64>, boundary: Option<f64>, cy: f64, r: f64, y1: f64) -> Measures {
    Measures { size, boundary, centroid: Some([0.0, cy, 0.0]), bounds: Some(([-r, 0.0, -r], [r, y1, r])), series: None }
}

/// A turned solid's anchors: its base and top on the axis (points of its end discs, or free
/// where the axis is open there), and four points round it at the height `y` and radius `r`.
fn round_anchors(base: On, top: (f64, On), y: f64, r: f64) -> Vec<Anchor> {
    let mut v = Vec::new();
    v.push(match base {
        On::Free => at3("base", [0.0; 3], On::Free),
        on => out3("base", [0.0; 3], [0.0, -1.0, 0.0], on),
    });
    v.push(match top.1 {
        On::Free => at3("top", [0.0, top.0, 0.0], On::Free),
        on => out3("top", [0.0, top.0, 0.0], [0.0, 1.0, 0.0], on),
    });
    for (name, d) in [("right", [1.0, 0.0, 0.0]), ("left", [-1.0, 0.0, 0.0]), ("front", [0.0, 0.0, 1.0]), ("back", [0.0, 0.0, -1.0])] {
        v.push(out3(name, [r * d[0], y, r * d[2]], d, On::Boundary));
    }
    v
}

// ---------------------------------------------------------------- the rounded cylinder

const P_RCYL: &[Param] = &[len("radius", "the radius", 0.5), len("height", "the height", 0.2), len("fillet", "the edges' radius, under the radius and half the height", 0.05)];
fn rcyl_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (rr, h, r) = (a.f("radius"), a.f("height"), a.f("fillet"));
    if r >= rr || 2.0 * r >= h {
        return Err(refuse("fillet_past_the_flats", "the fillet must be under the radius and under half the height (a flat is left on each face)", &[("radius", rr), ("height", h), ("fillet", r)]));
    }
    Ok((rr, h, r))
}
fn rcyl_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rr, h, r) = rcyl_parts(a)?;
    turned(vec![
        piece(line(O, [rr - r, 0.0]), "bottom"),
        piece(arc([rr - r, r], r, -0.5 * PI, 0.0), "lower_round"),
        piece(line([rr, r], [rr, h - r]), "side"),
        piece(arc([rr - r, h - r], r, 0.0, 0.5 * PI), "upper_round"),
        piece(line([rr - r, h], [0.0, h]), "top"),
    ])
}
fn rcyl_closed(a: &Args) -> Measures {
    let Ok((rr, h, r)) = rcyl_parts(a) else { return Measures::default() };
    // Pappus: the profile is the rectangle less two spandrels (a square less a quarter disc), of
    // area r² (1 − π/4) each, whose centroids are `r (10 − 3π) / (3 (4 − π))` in from the wall.
    let spandrel = r * r * (1.0 - 0.25 * PI);
    let inward = r * (10.0 - 3.0 * PI) / (3.0 * (4.0 - PI));
    let v = PI * rr * rr * h - 4.0 * PI * (rr - inward) * spandrel;
    // The rounds: a quarter arc turned, its centroid `R − r + 2r/π` from the axis.
    let round = 2.0 * PI * (rr - r + 2.0 * r / PI) * 0.5 * PI * r;
    let area = TAU * rr * (h - 2.0 * r) + 2.0 * PI * (rr - r) * (rr - r) + 2.0 * round;
    m(Some(v), Some(area), 0.5 * h, rr, h)
}
fn rcyl_anchors(a: &Args) -> Vec<Anchor> {
    let (rr, h) = (a.f("radius"), a.f("height"));
    let mut v = round_anchors(On::Boundary, (h, On::Boundary), 0.5 * h, rr);
    v.push(at3("centre", [0.0, 0.5 * h, 0.0], On::Inside));
    v
}

// ---------------------------------------------------------------- a profile laid out

/// A piece of a turned profile as it is laid out: a line, or an arc about `c` of radius `r` from
/// `a` to `b`, turning counter-clockwise (`ccw`) or clockwise.
#[derive(Clone, Copy, Debug)]
pub(super) enum El {
    Line(Pt, Pt),
    Arc { c: Pt, r: f64, a: Pt, b: Pt, ccw: bool },
}

impl El {
    pub(super) fn seg(self) -> Seg {
        match self {
            El::Line(a, b) => line(a, b),
            El::Arc { c, r, a, b, ccw } => {
                let a0 = atan2(a[1] - c[1], a[0] - c[0]);
                let a1 = atan2(b[1] - c[1], b[0] - c[0]);
                let mut sweep = a1 - a0;
                if ccw && sweep <= 0.0 {
                    sweep += TAU;
                } else if !ccw && sweep >= 0.0 {
                    sweep -= TAU;
                }
                arc_ends(c, r, a0, sweep, a, b)
            }
        }
    }
    /// The same piece run the other way.
    fn rev(self) -> El {
        match self {
            El::Line(a, b) => El::Line(b, a),
            El::Arc { c, r, a, b, ccw } => El::Arc { c, r, a: b, b: a, ccw: !ccw },
        }
    }
    /// The piece moved `t` to the left of the way it runs (a wall's inside, for a profile that
    /// runs up the outside with the material on its left): a line moved along its normal, an arc
    /// about the same centre with `t` less radius where it turns left and `t` more where it turns
    /// right. `None` where an arc turning left is no larger than `t`.
    fn inset(self, t: f64) -> Option<El> {
        match self {
            El::Line(a, b) => {
                let d = sub(b, a);
                let l = crate::maths::len(d);
                let n = [-d[1] / l * t, d[0] / l * t];
                Some(El::Line(add(a, n), add(b, n)))
            }
            El::Arc { c, r, a, b, ccw } => {
                let r2 = if ccw { r - t } else { r + t };
                if r2 <= 0.0 {
                    return None;
                }
                let k = r2 / r;
                Some(El::Arc { c, r: r2, a: add(c, scale(sub(a, c), k)), b: add(c, scale(sub(b, c), k)), ccw })
            }
        }
    }
    fn end(self) -> Pt {
        match self {
            El::Line(_, b) | El::Arc { b, .. } => b,
        }
    }
}

/// The two arcs of one radius from `a` up to `b`, the profile upright (running along +y) at both:
/// out and back in an S, the first arc turning toward `b`'s side and the second back. Their
/// radius `(dx² + dy²) / 4|dx|` makes them meet halfway between their centres with one tangent.
/// `Err` with the rise and the step across where the rise is less than the step (each arc would
/// turn past a quarter, and the profile would fold back down) or more than fifty times it (the
/// arcs' centres so far off that the measures' digits go).
fn s_join(a: Pt, b: Pt) -> Result<[El; 2], (f64, f64)> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    if !(dy >= dx.abs() && dy <= 50.0 * dx.abs()) {
        return Err((dy, dx.abs()));
    }
    let rho = (dx * dx + dy * dy) / (4.0 * dx.abs());
    let s = if dx > 0.0 { 1.0 } else { -1.0 };
    let (c1, c2) = ([a[0] + s * rho, a[1]], [b[0] - s * rho, b[1]]);
    let m = scale(add(c1, c2), 0.5);
    Ok([El::Arc { c: c1, r: rho, a, b: m, ccw: s < 0.0 }, El::Arc { c: c2, r: rho, a: m, b, ccw: s > 0.0 }])
}

/// An S as a refusal: its rise against its step across, with the form's numbers.
fn steep(rule: &'static str, what: &str, (dy, dx): (f64, f64), nums: &[(&'static str, f64)]) -> Refusal {
    let mut v = nums.to_vec();
    v.push(("rise", dy));
    v.push(("across", dx));
    refuse(rule, &format!("{what}: its rise must be from one to fifty times the step across it (each of its two arcs at most a quarter turn)"), &v)
}

/// The inside of a wall `t` thick under `outer` (pieces running up the outside), run back down:
/// `Err` with the radius of the first arc no larger than the wall.
fn inside(outer: &[El], t: f64) -> Result<Vec<El>, f64> {
    let mut v = Vec::with_capacity(outer.len());
    for e in outer.iter().rev() {
        match e.inset(t) {
            Some(i) => v.push(i.rev()),
            None => {
                return Err(match e {
                    El::Arc { r, .. } => *r,
                    El::Line(..) => 0.0,
                })
            }
        }
    }
    Ok(v)
}

/// A turned form from named pieces laid out.
fn turned_els(els: &[(El, &'static str)]) -> Result<Geom, Refusal> {
    turned(els.iter().map(|&(e, name)| piece(e.seg(), name)).collect())
}

// ---------------------------------------------------------------- vessels

const P_CUP: &[Param] = &[len("radius", "the outside's radius at the bottom", 0.4), len("top", "the outside's radius at the rim", 0.45), len("height", "the height", 0.5), len("wall", "the wall's thickness, measured across the wall", 0.03), len("floor", "the floor's thickness", 0.04)];
/// A cup's numbers, and the inside's radii at the rim and at the floor. The inside is the
/// outside's line moved `wall` across it (along its normal), so the wall is `wall` thick
/// measured across, however the cup flares; level, it is `wall · L / h` (`L` the side's slant
/// length): review 1 of stage 4 found a level offset, 0.0033 m across where 0.03 was asked.
fn cup_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64, f64, f64), Refusal> {
    let (r, t, h, w, f) = (a.f("radius"), a.f("top"), a.f("height"), a.f("wall"), a.f("floor"));
    if f >= h {
        return Err(refuse("floor_over_the_rim", "the floor must be thinner than the cup is tall", &[("height", h), ("floor", f)]));
    }
    let level = w * ((t - r) * (t - r) + h * h).sqrt() / h;
    let ti = t - level;
    let rf = r + (t - r) * (f / h) - level;
    if rf <= 0.0 || ti <= 0.0 {
        return Err(refuse("wall_fills_the_cup", "the wall must leave a hole at the floor and at the rim", &[("radius", r), ("top", t), ("height", h), ("wall", w), ("floor", f), ("inside_at_floor", rf)]));
    }
    Ok((r, t, h, w, f, ti, rf))
}
fn cup_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t, h, _, f, ti, rf) = cup_parts(a)?;
    turned(vec![
        piece(line(O, [r, 0.0]), "bottom"),
        piece(line([r, 0.0], [t, h]), "outside"),
        piece(line([t, h], [ti, h]), "rim"),
        piece(line([ti, h], [rf, f]), "inside"),
        piece(line([rf, f], [0.0, f]), "floor"),
    ])
}
fn frustum_v(h: f64, a: f64, b: f64) -> f64 {
    PI * h * (a * a + a * b + b * b) / 3.0
}
fn cup_closed(a: &Args) -> Measures {
    let Ok((r, t, h, _, f, ti, rf)) = cup_parts(a) else { return Measures::default() };
    let v = frustum_v(h, r, t) - frustum_v(h - f, rf, ti);
    let area = PI * r * r + PI * (r + t) * ((t - r) * (t - r) + h * h).sqrt() + PI * (t + ti) * (t - ti) + PI * (rf + ti) * ((ti - rf) * (ti - rf) + (h - f) * (h - f)).sqrt() + PI * rf * rf;
    Measures { size: Some(v), boundary: Some(area), centroid: None, bounds: Some(([-r.max(t), 0.0, -r.max(t)], [r.max(t), h, r.max(t)])), series: None }
}
fn cup_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((r, t, h, _, f, ti, _)) = cup_parts(a) else { return Vec::new() };
    vec![
        out3("base", [0.0; 3], [0.0, -1.0, 0.0], On::Boundary),
        at3("top", [0.0, h, 0.0], On::Free),
        out3("rim", [0.5 * (t + ti), h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("inside_floor", [0.0, f, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("side", [0.5 * (r + t), 0.5 * h, 0.0], [h, r - t, 0.0], On::Boundary),
    ]
}

const P_BELL: &[Param] = &[
    len("radius", "the lip's radius", 0.5),
    ratio("height_ratio", "the height, as a share of the lip's radius", 1.5, 0.6, 3.0),
    ratio("waist_ratio", "the waist's radius, as a share of the lip's", 0.55, 0.25, 0.95),
    ratio("flare_ratio", "how high the flare from the lip rises before the waist, as a share of the height", 0.45, 0.05, 0.9),
    ratio("shoulder_ratio", "the shoulder's radius (round from the waist into the crown), as a share of the waist's", 0.4, 0.05, 0.95),
    len("wall", "the wall's thickness, measured across the wall (the lip is a band two walls high, thicker where the flare is strong)", 0.025),
];
/// A bell's pieces, up the outside from the lip: a lip band two walls high; the flare, an arc
/// bowed in from the lip up to the waist, upright there; the waist, straight up; the shoulder, a
/// quarter round into the flat crown. Its inside is the outside moved `wall` across it, down to
/// the lip band's top, then straight down to the mouth.
struct Bell {
    r: f64,
    h: f64,
    t: f64,
    outer: Vec<El>,
    rc: f64,
    inner_lip: f64,
}
fn bell_parts(a: &Args) -> Result<Bell, Refusal> {
    let r = a.f("radius");
    let (h, rw, t) = (a.f("height_ratio") * r, a.f("waist_ratio") * r, a.f("wall"));
    let yw = a.f("flare_ratio") * h;
    let s = a.f("shoulder_ratio") * rw;
    let nums = [("radius", r), ("height_ratio", a.f("height_ratio")), ("waist_ratio", a.f("waist_ratio")), ("flare_ratio", a.f("flare_ratio")), ("shoulder_ratio", a.f("shoulder_ratio")), ("wall", t)];
    let lip = 2.0 * t;
    if yw <= lip {
        return Err(refuse("flare_under_the_lip", "the flare must rise above the lip band (two walls): a larger flare_ratio or a thinner wall", &nums));
    }
    let ys = h - s;
    if ys <= yw {
        return Err(refuse("shoulder_under_the_flare", "the shoulder's round must start above the flare: a smaller shoulder_ratio or flare_ratio", &nums));
    }
    if t >= s {
        return Err(refuse("wall_past_the_shoulder", "the wall must be thinner than the shoulder's radius", &nums));
    }
    // The flare: about (rw + ρ, yw), through the lip band's top (r, lip). It rises at least as
    // far as it steps in, or the circle's lowest point would come between the waist and the
    // lip, and the flare would dip under the lip.
    let (k, d) = (r - rw, yw - lip);
    if d < k {
        return Err(refuse("flare_too_shallow", "the flare must rise (from the lip band to the waist) at least as far as it steps in (from the lip to the waist): a larger height_ratio or flare_ratio, or a larger waist_ratio", &nums));
    }
    let rho = (k * k + d * d) / (2.0 * k);
    let c = [rw + rho, yw];
    let rc = rw - s;
    let outer = vec![
        El::Line([r, 0.0], [r, lip]),
        El::Arc { c, r: rho, a: [r, lip], b: [rw, yw], ccw: false },
        El::Line([rw, yw], [rw, ys]),
        El::Arc { c: [rc, ys], r: s, a: [rw, ys], b: [rc, h], ccw: true },
    ];
    // The inside's flare meets the lip band's top at `x = c − √((ρ + t)² − d²)`: at least `t`
    // in from the lip, since √(A + 2ρt + t²) − √A ≥ t for A = ρ² − d² ≤ ρ².
    let inner_lip = c[0] - ((rho + t) * (rho + t) - d * d).sqrt();
    Ok(Bell { r, h, t, outer, rc, inner_lip })
}
fn bell_geom(a: &Args) -> Result<Geom, Refusal> {
    let b = bell_parts(a)?;
    let (r, h, t, lip) = (b.r, b.h, b.t, 2.0 * b.t);
    let names = ["lip", "flare", "waist", "shoulder"];
    // The inside of the flare, waist and shoulder (the lip band's is the straight drop).
    let ins = inside(&b.outer[1..], t).map_err(|_| refuse("wall_past_the_shoulder", "the wall must be thinner than the shoulder's radius", &[("wall", t)]))?;
    let mut els: Vec<(El, &'static str)> = vec![(El::Line([0.0, h - t], [b.rc, h - t]), "under_crown")];
    for (e, n) in ins.iter().zip(["inside_shoulder", "inside_waist", "inside_flare"]) {
        els.push((*e, n));
    }
    // The inside flare ends on the lip band's top, at `inner_lip`.
    if let Some((El::Arc { c, r: ri, a, ccw, .. }, n)) = els.last().copied() {
        let end = [b.inner_lip, lip];
        *els.last_mut().unwrap() = (El::Arc { c, r: ri, a, b: end, ccw }, n);
    }
    els.push((El::Line([b.inner_lip, lip], [b.inner_lip, 0.0]), "inside_lip"));
    els.push((El::Line([b.inner_lip, 0.0], [r, 0.0]), "mouth"));
    for (e, n) in b.outer.iter().zip(names) {
        els.push((*e, n));
    }
    els.push((El::Line([b.rc, h], [0.0, h]), "crown"));
    turned_els(&els)
}
fn bell_anchors(a: &Args) -> Vec<Anchor> {
    let Ok(b) = bell_parts(a) else { return Vec::new() };
    vec![
        at3("mouth", [0.0; 3], On::Free),
        out3("top", [0.0, b.h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("inside_top", [0.0, b.h - b.t, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        at3("lip", [b.r, 0.0, 0.0], On::Vertex),
    ]
}

const P_GOBLET: &[Param] = &[
    len("radius", "the cup's outside radius", 0.04),
    len("wall", "the cup's wall", 0.003),
    len("stem", "the stem's radius", 0.005),
    len("length", "the stem's length", 0.08),
    len("foot", "the foot's radius", 0.035),
    len("base", "the foot's thickness", 0.004),
];
/// Seven numbers of a turned form, in the order its `_parts` function says.
type Seven = (f64, f64, f64, f64, f64, f64, f64);
fn goblet_parts(a: &Args) -> Result<Seven, Refusal> {
    let (r, t, s, l, f, b) = (a.f("radius"), a.f("wall"), a.f("stem"), a.f("length"), a.f("foot"), a.f("base"));
    let nums = [("radius", r), ("wall", t), ("stem", s), ("foot", f)];
    if s >= r || s >= f {
        return Err(refuse("stem_wider_than_cup_or_foot", "the stem must be narrower than the cup and the foot", &nums));
    }
    if t >= r {
        return Err(refuse("wall_fills_the_cup", "the wall must be thinner than the cup's radius", &nums));
    }
    // Where the stem meets the cup's outside, the inside's lowest point must stay above it.
    let drop = (r - s) * (r + s);
    let d = drop.sqrt();
    if r - d >= t {
        return Err(refuse("wall_too_thin_over_the_stem", "the cup's inside would reach down into the stem: a thicker wall or a thinner stem", &nums));
    }
    Ok((r, t, s, l, f, b, d))
}
fn goblet_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t, s, l, f, b, d) = goblet_parts(a)?;
    let yc = b + l + d;
    let a0 = atan2(-d, s);
    turned(vec![
        piece(line(O, [f, 0.0]), "foot_under"),
        piece(line([f, 0.0], [f, b]), "foot_edge"),
        piece(line([f, b], [s, b]), "foot_top"),
        piece(line([s, b], [s, b + l]), "stem"),
        piece(arc_ends([0.0, yc], r, a0, -a0, [s, b + l], [r, yc]), "cup_outside"),
        piece(line([r, yc], [r - t, yc]), "rim"),
        piece(arc([0.0, yc], r - t, 0.0, -0.5 * PI), "cup_inside"),
    ])
}
fn goblet_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((r, t, _, l, f, b, d)) = goblet_parts(a) else { return Vec::new() };
    let yc = b + l + d;
    vec![
        out3("base", [0.0; 3], [0.0, -1.0, 0.0], On::Boundary),
        out3("rim", [r - 0.5 * t, yc, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("bowl_bottom", [0.0, yc - r + t, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        at3("mouth", [0.0, yc, 0.0], On::Free),
        at3("foot_edge", [f, 0.0, 0.0], On::Boundary),
    ]
}

// ---------------------------------------------------------------- turned props

const P_BOTTLE: &[Param] = &[
    len("radius", "the body's radius", 0.038),
    ratio("height_ratio", "the height, as a share of the body's radius", 8.0, 3.0, 30.0),
    ratio("neck_ratio", "the neck's radius, as a share of the body's", 0.36, 0.1, 0.9),
    ratio("neck_length_ratio", "the neck's length (with its lip), as a share of the height", 0.28, 0.05, 0.6),
    ratio("shoulder_ratio", "the shoulder's height, as a multiple of the step in from the body to the neck (1 to 50: its S of two arcs at most a quarter turn each)", 1.6, 1.0, 10.0),
    len("wall", "the wall's thickness, measured across the wall (the floor is as thick)", 0.003),
];
/// A bottle up its outside: the body, straight; the shoulder, an S of two arcs from the body in
/// to the neck, upright at both; the neck; a lip ring at the top (a seventh wider than the neck
/// and an eighth of the neck's length tall); open at the mouth, its inside the outside moved
/// `wall` across and a floor as thick.
struct Bottle {
    r: f64,
    h: f64,
    t: f64,
    ys: f64,
    outer: Vec<El>,
    lip: (f64, f64),
    n: f64,
}
fn bottle_parts(a: &Args) -> Result<Bottle, Refusal> {
    let r = a.f("radius");
    let (h, n, t) = (a.f("height_ratio") * r, a.f("neck_ratio") * r, a.f("wall"));
    let ln = a.f("neck_length_ratio") * h;
    let sh = a.f("shoulder_ratio") * (r - n);
    let nums = [("radius", r), ("height_ratio", a.f("height_ratio")), ("neck_ratio", a.f("neck_ratio")), ("neck_length_ratio", a.f("neck_length_ratio")), ("shoulder_ratio", a.f("shoulder_ratio")), ("wall", t)];
    let ys = h - ln - sh;
    if ys <= t {
        return Err(refuse("neck_and_shoulder_over_the_height", "the neck and the shoulder must leave some body over the floor", &nums));
    }
    if t >= n {
        return Err(refuse("wall_fills_the_neck", "the wall must be thinner than the neck's radius", &nums));
    }
    let s = s_join([r, ys], [n, ys + sh]).map_err(|x| steep("shoulder_too_steep", "the shoulder", x, &nums))?;
    let (rl, hl) = (n * 8.0 / 7.0, ln / 8.0);
    let outer = vec![El::Line([r, t], [r, ys]), s[0], s[1], El::Line([n, ys + sh], [n, h])];
    Ok(Bottle { r, h, t, ys, outer, lip: (rl, hl), n })
}
fn bottle_geom(a: &Args) -> Result<Geom, Refusal> {
    let b = bottle_parts(a)?;
    let (r, h, t, (rl, hl), n) = (b.r, b.h, b.t, b.lip, b.n);
    // The inside: the outside moved across, from the mouth (the neck's top) down to the floor
    // (the body's foot, `t` up).
    let ins = inside(&b.outer, t).map_err(|rad| refuse("wall_past_the_shoulder", "the wall must be thinner than the shoulder's arcs", &[("wall", t), ("arc_radius", rad)]))?;
    let mut els: Vec<(El, &'static str)> = vec![(El::Line(O, [r, 0.0]), "base"), (El::Line([r, 0.0], [r, b.ys]), "body"), (b.outer[1], "shoulder"), (b.outer[2], "shoulder")];
    els.push((El::Line([n, b.outer[2].end()[1]], [n, h - hl]), "neck"));
    els.push((El::Line([n, h - hl], [rl, h - hl]), "lip_under"));
    els.push((El::Line([rl, h - hl], [rl, h]), "lip"));
    els.push((El::Line([rl, h], [n - t, h]), "rim"));
    for (e, name) in ins.iter().zip(["inside_neck", "inside_shoulder", "inside_shoulder", "inside_body"]) {
        els.push((*e, name));
    }
    els.push((El::Line([r - t, t], [0.0, t]), "floor"));
    turned_els(&els)
}
fn bottle_anchors(a: &Args) -> Vec<Anchor> {
    let Ok(b) = bottle_parts(a) else { return Vec::new() };
    let mut v = round_anchors(On::Boundary, (b.h, On::Free), 0.5 * b.ys, b.r);
    v.push(out3("rim", [0.5 * (b.lip.0 + b.n - b.t), b.h, 0.0], [0.0, 1.0, 0.0], On::Boundary));
    v.push(out3("inside_floor", [0.0, b.t, 0.0], [0.0, 1.0, 0.0], On::Boundary));
    v
}

const P_VASE: &[Param] = &[
    len("radius", "the belly's radius", 0.12),
    ratio("height_ratio", "the height, as a share of the belly's radius", 3.0, 1.5, 8.0),
    ratio("foot_ratio", "the foot's radius, as a share of the belly's", 0.6, 0.2, 0.98),
    ratio("waist_ratio", "the waist's radius (between the foot and the belly), as a share of the foot's", 0.6, 0.1, 0.95),
    ratio("neck_ratio", "the neck's radius, as a share of the belly's", 0.4, 0.1, 0.95),
    ratio("mouth_ratio", "the mouth's radius, as a share of the neck's (over 1: a flared lip)", 1.5, 1.05, 4.0),
    len("wall", "the wall's thickness, measured across the wall", 0.006),
];
/// The heights of a vase's foot band, waist, belly, neck and the lip's start, as shares of its
/// height (the engine's own proportions: the belly a little under the middle, the neck at four
/// fifths).
const VASE_AT: [f64; 5] = [0.05, 0.15, 0.45, 0.8, 0.93];
/// A vase up its outside: a foot disc, then S curves (two arcs a time, upright at each end) in
/// to the waist, out to the belly, in to the neck and out to the mouth, and a short lip straight
/// up; open at the mouth, its inside the outside moved `wall` across, down to the waist, where
/// its floor is (the foot and the waist below it are solid).
struct Vase {
    rf: f64,
    h: f64,
    t: f64,
    outer: Vec<El>,
    yw: f64,
    rm: f64,
    belly: (f64, f64),
}
fn vase_parts(a: &Args) -> Result<Vase, Refusal> {
    let r = a.f("radius");
    let h = a.f("height_ratio") * r;
    let rf = a.f("foot_ratio") * r;
    let rw = a.f("waist_ratio") * rf;
    let rn = a.f("neck_ratio") * r;
    let rm = a.f("mouth_ratio") * rn;
    let t = a.f("wall");
    let nums = [("radius", r), ("height_ratio", a.f("height_ratio")), ("foot_ratio", a.f("foot_ratio")), ("waist_ratio", a.f("waist_ratio")), ("neck_ratio", a.f("neck_ratio")), ("mouth_ratio", a.f("mouth_ratio")), ("wall", t)];
    let [hf, yw, yb, yn, ym] = VASE_AT.map(|k| k * h);
    if t >= rw.min(rn) {
        return Err(refuse("wall_fills_the_vase", "the wall must be thinner than the waist's and the neck's radii", &nums));
    }
    let foot = s_join([rf, hf], [rw, yw]).map_err(|x| steep("foot_too_steep", "the curve from the foot in to the waist", x, &nums))?;
    let belly = s_join([rw, yw], [r, yb]).map_err(|x| steep("belly_too_steep", "the curve from the waist out to the belly", x, &nums))?;
    let shoulder = s_join([r, yb], [rn, yn]).map_err(|x| steep("shoulder_too_steep", "the curve from the belly in to the neck", x, &nums))?;
    let flare = s_join([rn, yn], [rm, ym]).map_err(|x| steep("lip_too_steep", "the curve from the neck out to the mouth", x, &nums))?;
    let outer = vec![El::Line([rf, 0.0], [rf, hf]), foot[0], foot[1], belly[0], belly[1], shoulder[0], shoulder[1], flare[0], flare[1], El::Line([rm, ym], [rm, h])];
    Ok(Vase { rf, h, t, outer, yw, rm, belly: (r, yb) })
}
fn vase_geom(a: &Args) -> Result<Geom, Refusal> {
    let v = vase_parts(a)?;
    let (h, t) = (v.h, v.t);
    let names = ["foot_edge", "foot_top", "foot_top", "belly", "belly", "shoulder", "shoulder", "flare", "flare", "lip"];
    // The inside, from the mouth down to the waist (the outside's pieces from the waist up).
    let ins = inside(&v.outer[3..], t).map_err(|rad| refuse("wall_past_a_curve", "the wall must be thinner than the radius of every arc that turns toward the inside", &[("wall", t), ("arc_radius", rad)]))?;
    let mut els: Vec<(El, &'static str)> = vec![(El::Line(O, [v.rf, 0.0]), "foot_under")];
    for (e, name) in v.outer.iter().zip(names) {
        els.push((*e, name));
    }
    let top_in = match ins[0] {
        El::Line(a, _) | El::Arc { a, .. } => a,
    };
    els.push((El::Line([v.rm, h], top_in), "rim"));
    for e in &ins {
        els.push((*e, "inside"));
    }
    let floor = ins.last().map_or([0.0, v.yw], |e| e.end());
    els.push((El::Line(floor, [0.0, floor[1]]), "floor"));
    turned_els(&els)
}
fn vase_anchors(a: &Args) -> Vec<Anchor> {
    let Ok(v) = vase_parts(a) else { return Vec::new() };
    let mut out = round_anchors(On::Boundary, (v.h, On::Free), v.belly.1, v.belly.0);
    out.push(out3("rim", [v.rm - 0.5 * v.t, v.h, 0.0], [0.0, 1.0, 0.0], On::Boundary));
    out.push(out3("inside_floor", [0.0, v.yw, 0.0], [0.0, 1.0, 0.0], On::Boundary));
    out
}

const P_BALUSTER: &[Param] = &[
    len("radius", "the plinth's and the cap's radius", 0.06),
    ratio("height_ratio", "the height, as a share of the plinth's radius", 13.0, 6.0, 20.0),
    ratio("bulb_ratio", "the bulb's radius, as a share of the plinth's", 0.85, 0.5, 1.0),
    ratio("neck_ratio", "the neck's radius (the narrowest, above the bulb), as a share of the bulb's", 0.42, 0.2, 0.7),
];
/// The heights a baluster's turned part reaches, as shares of that part: the bulb at 0.22, the
/// neck at 0.72, the collar at 0.86 and the throat under the cap at 1; its plinth is 0.11 of the
/// whole height and its cap 0.08. (The engine's own proportions, after the vase-shaped baluster
/// of a railing: the research library holds no balusters.)
const BALUSTER_AT: [f64; 4] = [0.22, 0.72, 0.86, 1.0];
/// A baluster up its outside: a plinth (a disc), a ledge in to the turned part, then S curves
/// out to the bulb, in along a long neck, out to a collar and in to the throat, a ledge out to
/// the cap (a disc) and its top. Solid.
struct Baluster {
    h: f64,
    rb: f64,
    els: Vec<El>,
    bulb: (f64, f64),
}
fn baluster_parts(a: &Args) -> Result<Baluster, Refusal> {
    let rb = a.f("radius");
    let h = a.f("height_ratio") * rb;
    let rbulb = a.f("bulb_ratio") * rb;
    let rneck = a.f("neck_ratio") * rbulb;
    let (r0, rcol, rq) = (0.6 * rbulb, 1.4 * rneck, 1.1 * rneck);
    let (tb, tc) = (0.11 * h, 0.08 * h);
    let l = h - tb - tc;
    let [yb, yn, yc, yq] = BALUSTER_AT.map(|k| tb + k * l);
    let nums = [("radius", rb), ("height_ratio", a.f("height_ratio")), ("bulb_ratio", a.f("bulb_ratio")), ("neck_ratio", a.f("neck_ratio"))];
    let bulb = s_join([r0, tb], [rbulb, yb]).map_err(|x| steep("bulb_too_steep", "the curve out to the bulb", x, &nums))?;
    let neck = s_join([rbulb, yb], [rneck, yn]).map_err(|x| steep("neck_too_steep", "the curve in from the bulb to the neck", x, &nums))?;
    let collar = s_join([rneck, yn], [rcol, yc]).map_err(|x| steep("collar_too_steep", "the curve out to the collar", x, &nums))?;
    let throat = s_join([rcol, yc], [rq, yq]).map_err(|x| steep("throat_too_steep", "the curve in under the cap", x, &nums))?;
    let mut els = vec![El::Line(O, [rb, 0.0]), El::Line([rb, 0.0], [rb, tb]), El::Line([rb, tb], [r0, tb])];
    els.extend(bulb.into_iter().chain(neck).chain(collar).chain(throat));
    els.extend([El::Line([rq, yq], [rb, yq]), El::Line([rb, yq], [rb, h]), El::Line([rb, h], [0.0, h])]);
    Ok(Baluster { h, rb, els, bulb: (rbulb, yb) })
}
fn baluster_geom(a: &Args) -> Result<Geom, Refusal> {
    let b = baluster_parts(a)?;
    let names = ["plinth_under", "plinth", "plinth_top", "bulb", "bulb", "neck", "neck", "collar", "collar", "throat", "throat", "cap_under", "cap", "top"];
    turned_els(&b.els.iter().copied().zip(names).collect::<Vec<_>>())
}
fn baluster_anchors(a: &Args) -> Vec<Anchor> {
    let Ok(b) = baluster_parts(a) else { return Vec::new() };
    let mut v = round_anchors(On::Boundary, (b.h, On::Boundary), b.bulb.1, b.bulb.0);
    v.push(at3("plinth_edge", [b.rb, 0.0, 0.0], On::Vertex));
    v
}

const P_MUSHROOM: &[Param] = &[
    len("stem", "the stem's radius, under the cap's", 0.02),
    len("stem_height", "from the ground to the cap's underside", 0.06),
    len("cap", "the cap's radius", 0.06),
    len("cap_height", "the cap's height, at most its radius (a half ball)", 0.04),
];
fn mushroom_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64), Refusal> {
    let (s, hs, c, hc) = (a.f("stem"), a.f("stem_height"), a.f("cap"), a.f("cap_height"));
    if s >= c {
        return Err(refuse("stem_wider_than_the_cap", "the stem must be narrower than the cap", &[("stem", s), ("cap", c)]));
    }
    if hc > c {
        return Err(refuse("cap_past_a_half_ball", "the cap is a spherical cap of at most half a ball: its height at most its radius", &[("cap", c), ("cap_height", hc)]));
    }
    Ok((s, hs, c, hc, (c * c + hc * hc) / (2.0 * hc)))
}
fn mushroom_geom(a: &Args) -> Result<Geom, Refusal> {
    let (s, hs, c, hc, rho) = mushroom_parts(a)?;
    // The cap's sphere about `(0, hs + hc − ρ)`; its rim `(c, hs)` is at `(c, ρ − hc)` from it,
    // `ρ − hc = (c − hc)(c + hc) / 2hc`.
    let a0 = atan2((c - hc) * (c + hc) / (2.0 * hc), c);
    turned(vec![
        piece(line(O, [s, 0.0]), "stem_foot"),
        piece(line([s, 0.0], [s, hs]), "stem"),
        piece(line([s, hs], [c, hs]), "gills"),
        piece(arc_ends([0.0, hs + hc - rho], rho, a0, 0.5 * PI - a0, [c, hs], [0.0, hs + hc]), "cap"),
    ])
}
fn mushroom_closed(a: &Args) -> Measures {
    let Ok((s, hs, c, hc, rho)) = mushroom_parts(a) else { return Measures::default() };
    let v = PI * s * s * hs + PI * hc * hc * (3.0 * rho - hc) / 3.0;
    let area = PI * s * s + TAU * s * hs + PI * (c - s) * (c + s) + TAU * rho * hc;
    Measures { size: Some(v), boundary: Some(area), centroid: None, bounds: Some(([-c, 0.0, -c], [c, hs + hc, c])), series: None }
}
fn mushroom_anchors(a: &Args) -> Vec<Anchor> {
    let (s, hs, c, hc) = (a.f("stem"), a.f("stem_height"), a.f("cap"), a.f("cap_height"));
    let mut v = round_anchors(On::Boundary, (hs + hc, On::Vertex), 0.5 * hs, s);
    v.push(at3("cap_rim", [c, hs, 0.0], On::Boundary));
    v.push(out3("under_cap", [0.5 * (s + c), hs, 0.0], [0.0, -1.0, 0.0], On::Boundary));
    v
}

// ---------------------------------------------------------------- flat solids

/// Each face wound counter-clockwise seen from outside a convex solid about `c`.
fn outward(pts: &[P3], faces: Vec<Vec<u32>>, c: P3) -> Vec<Vec<u32>> {
    faces
        .into_iter()
        .map(|mut f| {
            let mut area = [0.0; 3];
            let mut mid = [0.0; 3];
            for i in 0..f.len() {
                let (p, q) = (pts[f[i] as usize], pts[f[(i + 1) % f.len()] as usize]);
                area = add3(area, cross3(p, q));
                mid = add3(mid, p);
            }
            let mid = scale3(mid, 1.0 / f.len() as f64);
            if dot3(area, sub3(mid, c)) < 0.0 {
                f.reverse();
            }
            f
        })
        .collect()
}

const P_CBOX: &[Param] = &[len("width", "along x", 1.0), len("height", "along y", 1.0), len("depth", "along z", 1.0), len("chamfer", "how far each cut reaches along a face from its edge, under half the least side", 0.1)];
fn cbox_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (w, h, d, c) = (a.f("width"), a.f("height"), a.f("depth"), a.f("chamfer"));
    if 2.0 * c >= w.min(h).min(d) {
        return Err(refuse("chamfer_past_the_faces", "two chamfers must leave each face a flat: the chamfer under half the least side", &[("width", w), ("height", h), ("depth", d), ("chamfer", c)]));
    }
    Ok((w, h, d, c))
}
fn cbox_poly(w: f64, h: f64, d: f64, c: f64) -> Poly {
    let half = [0.5 * w, 0.5 * h, 0.5 * d];
    let mut pts = Vec::new();
    // Each corner's three points, one on each face that meets there, `c` in from the corner
    // along that face's two other axes.
    for corner in 0..8 {
        let s = [if corner & 1 != 0 { 1.0 } else { -1.0 }, if corner & 2 != 0 { 1.0 } else { -1.0 }, if corner & 4 != 0 { 1.0 } else { -1.0 }];
        for k in 0..3 {
            let mut p = [0.0; 3];
            for j in 0..3 {
                p[j] = s[j] * if j == k { half[j] } else { half[j] - c };
            }
            pts.push([p[0], p[1] + 0.5 * h, p[2]]);
        }
    }
    // The faces by index, never by where the points fell (so a box seven times as large has the
    // same faces): corner `q`'s point on the face across axis `k` is `3q + k`, and corner `q` has
    // bit `k` set where it is on the + side of axis `k`.
    let at = |q: u32, k: u32| 3 * q + k;
    let mut faces = Vec::new();
    let mut surf = Vec::new();
    for k in 0..3u32 {
        let (i, j) = ((k + 1) % 3, (k + 2) % 3);
        let corner = |si: u32, sj: u32, sk: u32| (si << i) | (sj << j) | (sk << k);
        for sk in 0..2 {
            // A face: its four corners' points, (−,−), (+,−), (+,+), (−,+) in (i, j), which turn
            // counter-clockwise about +k (e_i × e_j = e_k).
            faces.push([(0, 0), (1, 0), (1, 1), (0, 1)].map(|(si, sj)| at(corner(si, sj, sk), k)).to_vec());
            surf.push(0);
        }
        for (si, sj) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
            // The chamfer along the edge in direction k at (si, sj): two corners' points on face i
            // and on face j.
            faces.push(vec![at(corner(si, sj, 0), i), at(corner(si, sj, 0), j), at(corner(si, sj, 1), j), at(corner(si, sj, 1), i)]);
            surf.push(1);
        }
    }
    for q in 0..8 {
        faces.push(vec![at(q, 0), at(q, 1), at(q, 2)]);
        surf.push(2);
    }
    // (Each wound outward: the solid is convex and its middle is inside every face's plane.)
    let faces = outward(&pts, faces, [0.0, 0.5 * h, 0.0]);
    Poly { pts, faces, surf, names: ["faces", "chamfers", "corners"].map(String::from).to_vec() }
}
fn cbox_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, d, c) = cbox_parts(a)?;
    Ok(Geom::Solid(Rc::new(cbox_poly(w, h, d, c))))
}
fn cbox_closed(a: &Args) -> Measures {
    let Ok((w, h, d, c)) = cbox_parts(a) else { return Measures::default() };
    // The box less a triangular prism along each edge (legs c, length L − 2c) and, in each
    // corner's cube of side c, five sixths of it (the three chamfers leave a quarter of it, and
    // the corner's cut a twelfth more).
    let v = w * h * d - 2.0 * c * c * (w + h + d) + 16.0 / 3.0 * c * c * c;
    let (a2, b2, c2) = (w - 2.0 * c, h - 2.0 * c, d - 2.0 * c);
    let area = 2.0 * (a2 * b2 + b2 * c2 + c2 * a2) + 2f64.sqrt() * c * (4.0 * (w + h + d) - 24.0 * c) + 4.0 * 3f64.sqrt() * c * c;
    Measures { size: Some(v), boundary: Some(area), centroid: Some([0.0, 0.5 * h, 0.0]), bounds: Some(([-0.5 * w, 0.0, -0.5 * d], [0.5 * w, h, 0.5 * d])), series: None }
}
fn cbox_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, d) = (0.5 * a.f("width"), a.f("height"), 0.5 * a.f("depth"));
    vec![
        at3("centre", [0.0, 0.5 * h, 0.0], On::Inside),
        out3("base", [0.0; 3], [0.0, -1.0, 0.0], On::Boundary),
        out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("left", [-w, 0.5 * h, 0.0], [-1.0, 0.0, 0.0], On::Boundary),
        out3("right", [w, 0.5 * h, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("back", [0.0, 0.5 * h, -d], [0.0, 0.0, -1.0], On::Boundary),
        out3("front", [0.0, 0.5 * h, d], [0.0, 0.0, 1.0], On::Boundary),
    ]
}

/// The point of a ring of `n` about Y at radius `r`, height `y` and angle `t` (from −z, turning
/// toward +x: a vertex on −z at `t = 0`, as the prism's).
fn ring_pt(r: f64, y: f64, t: f64) -> P3 {
    let (s, c) = sin_cos(t);
    [r * s, y, -r * c]
}
fn ngon_area(n: f64, r: f64) -> f64 {
    0.5 * n * r * r * sin(TAU / n)
}

const P_CRYSTAL: &[Param] = &[
    count("sides", "the number of sides", 6.0, 3.0, 64.0),
    len("radius", "the circumradius of its section", 0.1),
    len("height", "the prism's height", 0.3),
    len("tip", "the top pyramid's height", 0.1),
    Param { min: 0.0, ..len("foot", "the bottom pyramid's height (0: a flat foot)", 0.05) },
];
fn crystal_poly(a: &Args) -> Poly {
    let (n, r, h, tip, foot) = (a.n("sides"), a.f("radius"), a.f("height"), a.f("tip"), a.f("foot"));
    let ang = |i: usize| TAU * i as f64 / n as f64;
    let mut pts: Vec<P3> = (0..n).map(|i| ring_pt(r, foot, ang(i))).collect();
    pts.extend((0..n).map(|i| ring_pt(r, foot + h, ang(i))));
    pts.push([0.0, foot + h + tip, 0.0]);
    let apex = 2 * n as u32;
    let mut faces = Vec::new();
    let mut surf = Vec::new();
    let nn = n as u32;
    for i in 0..nn {
        let j = (i + 1) % nn;
        faces.push(vec![i, j, nn + j, nn + i]);
        surf.push(0);
        faces.push(vec![nn + i, nn + j, apex]);
        surf.push(1);
    }
    if foot > 0.0 {
        pts.push([0.0; 3]);
        for i in 0..nn {
            faces.push(vec![i, apex + 1, (i + 1) % nn]);
            surf.push(2);
        }
    } else {
        faces.push((0..nn).collect());
        surf.push(2);
    }
    let faces = outward(&pts, faces, [0.0, foot + 0.5 * h, 0.0]);
    Poly { pts, faces, surf, names: ["sides", "tip", "foot"].map(String::from).to_vec() }
}
fn crystal_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(crystal_poly(a))))
}
fn crystal_closed(a: &Args) -> Measures {
    let (n, r, h, tip, foot) = (a.f("sides"), a.f("radius"), a.f("height"), a.f("tip"), a.f("foot"));
    let base = ngon_area(n, r);
    let side = 2.0 * r * sin(PI / n);
    let ap = r * cos(PI / n);
    let v = base * (h + (tip + foot) / 3.0);
    let lateral = |t: f64| 0.5 * n * side * (t * t + ap * ap).sqrt();
    let area = n * side * h + lateral(tip) + if foot > 0.0 { lateral(foot) } else { base };
    // The centroids: the prism's at its middle, each pyramid's a quarter of its height from its base.
    let cy = (base * h * (foot + 0.5 * h) + base * tip / 3.0 * (foot + h + 0.25 * tip) + base * foot / 3.0 * (0.75 * foot)) / v;
    Measures { size: Some(v), boundary: Some(area), centroid: Some([0.0, cy, 0.0]), bounds: None, series: None }
}
fn crystal_anchors(a: &Args) -> Vec<Anchor> {
    let (h, tip, foot) = (a.f("height"), a.f("tip"), a.f("foot"));
    vec![
        at3("tip", [0.0, foot + h + tip, 0.0], On::Vertex),
        if foot > 0.0 { at3("base", [0.0; 3], On::Vertex) } else { out3("base", [0.0; 3], [0.0, -1.0, 0.0], On::Boundary) },
        at3("centre", [0.0, foot + 0.5 * h, 0.0], On::Inside),
    ]
}

const P_GEM: &[Param] = &[
    count("sides", "the girdle's sides", 8.0, 3.0, 64.0),
    len("radius", "the girdle's circumradius", 0.05),
    ratio("table_ratio", "the flat top's circumradius, as a share of the girdle's (under cos(180°/sides))", 0.55, 0.1, 0.95),
    len("crown", "from the girdle's top up to the table", 0.016),
    Param { min: 0.0, ..len("girdle", "the girdle band's height, upright between the crown and the pavilion (0: a knife edge)", 0.0015) },
    len("pavilion", "from the point below up to the girdle", 0.043),
];
fn gem_parts(a: &Args) -> Result<(usize, f64, f64, f64, f64, f64), Refusal> {
    let (n, r, t, c, g, p) = (a.n("sides"), a.f("radius"), a.f("table_ratio"), a.f("crown"), a.f("girdle"), a.f("pavilion"));
    // (With a margin: a table whose corner is on a girdle side to rounding makes a crown triangle
    // of no area.)
    if t >= cos(PI / n as f64) - 1e-6 {
        return Err(refuse("table_past_the_girdle", "the table's corners must lie inside the girdle's sides: table_ratio under cos(180°/sides)", &[("sides", n as f64), ("table_ratio", t)]));
    }
    Ok((n, r, t * r, c, g, p))
}
/// A cut stone: the culet (the pavilion's point) at the origin; the pavilion, a pyramid of
/// triangles up to the girdle; the girdle band, upright quads `g` tall (none at `g = 0`); the
/// crown, triangles between the girdle's top and the table turned half a side; the table.
fn gem_poly(n: usize, r: f64, rt: f64, c: f64, g: f64, p: f64) -> Poly {
    let step = TAU / n as f64;
    let band = g > 0.0;
    let mut pts = vec![[0.0; 3]];
    pts.extend((0..n).map(|i| ring_pt(r, p, step * i as f64)));
    if band {
        pts.extend((0..n).map(|i| ring_pt(r, p + g, step * i as f64)));
    }
    pts.extend((0..n).map(|i| ring_pt(rt, p + g + c, step * (i as f64 + 0.5))));
    let nn = n as u32;
    let lo = |i: u32| 1 + i % nn;
    let hi = |i: u32| if band { 1 + nn + i % nn } else { lo(i) };
    let t = |i: u32| if band { 1 + 2 * nn + i % nn } else { 1 + nn + i % nn };
    let mut faces = Vec::new();
    let mut surf = Vec::new();
    for i in 0..nn {
        faces.push(vec![0, lo(i + 1), lo(i)]);
        surf.push(2);
        if band {
            faces.push(vec![lo(i), lo(i + 1), hi(i + 1), hi(i)]);
            surf.push(3);
        }
        faces.push(vec![hi(i), hi(i + 1), t(i)]);
        surf.push(1);
        faces.push(vec![t(i + nn - 1), hi(i), t(i)]);
        surf.push(1);
    }
    faces.push((0..nn).map(t).collect());
    surf.push(0);
    // (The stone is convex: each face wound outward about a point inside it.)
    let faces = outward(&pts, faces, [0.0, p + 0.5 * g, 0.0]);
    let names = if band { &["table", "crown", "pavilion", "girdle"][..] } else { &["table", "crown", "pavilion"][..] };
    Poly { pts, faces, surf, names: names.iter().map(|x| x.to_string()).collect() }
}
fn gem_geom(a: &Args) -> Result<Geom, Refusal> {
    let (n, r, rt, c, g, p) = gem_parts(a)?;
    Ok(Geom::Solid(Rc::new(gem_poly(n, r, rt, c, g, p))))
}
fn gem_closed(a: &Args) -> Measures {
    let Ok((n, r, rt, c, g, p)) = gem_parts(a) else { return Measures::default() };
    let nf = n as f64;
    // The crown is a prismatoid (every point in the girdle's plane or the table's): its volume is
    // c (A₀ + 4 M + A₁) / 6, M the area of its middle section, the polygon of its slant edges'
    // middles (two an edge of the girdle: toward the table's corner before it and after it).
    let step = TAU / nf;
    let mids: Vec<Pt> = (0..2 * n)
        .map(|k| {
            let i = k / 2;
            let gp = ring_pt(r, 0.0, step * i as f64);
            let tq = ring_pt(rt, 0.0, step * (i as f64 + if k % 2 == 0 { -0.5 } else { 0.5 }));
            [0.5 * (gp[0] + tq[0]), 0.5 * (gp[2] + tq[2])]
        })
        .collect();
    let mut twice = 0.0;
    for k in 0..mids.len() {
        let (u, w) = (mids[k], mids[(k + 1) % mids.len()]);
        twice += u[0] * w[1] - u[1] * w[0];
    }
    let mid = 0.5 * twice.abs();
    let v = ngon_area(nf, r) * (p / 3.0 + g) + c * (ngon_area(nf, r) + 4.0 * mid + ngon_area(nf, rt)) / 6.0;
    Measures { size: Some(v), ..Measures::default() }
}
fn gem_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((_, r, _, c, g, p)) = gem_parts(a) else { return Vec::new() };
    vec![at3("culet", [0.0; 3], On::Vertex), out3("table", [0.0, p + g + c, 0.0], [0.0, 1.0, 0.0], On::Boundary), at3("girdle_0", [0.0, p, -r], On::Vertex), at3("centre", [0.0, p + 0.5 * g, 0.0], On::Inside)]
}

const P_STAIRS: &[Param] = &[
    count("steps", "the number of steps", 5.0, 1.0, 100.0),
    len("rise", "each step's height", 0.18),
    len("run", "each tread's depth", 0.28),
    len("width", "the flight's width, along z", 1.0),
];
fn stairs_poly(a: &Args) -> Poly {
    let (n, rise, run, w) = (a.n("steps"), a.f("rise"), a.f("run"), a.f("width"));
    let prof = super::ornamenta::steps_pts(n, rise, run);
    let k = prof.len() as u32;
    let mut pts: Vec<P3> = prof.iter().map(|p| [p[0], p[1], 0.5 * w]).collect();
    pts.extend(prof.iter().map(|p| [p[0], p[1], -0.5 * w]));
    let mut faces = vec![(0..k).collect::<Vec<u32>>(), (0..k).rev().map(|i| k + i).collect()];
    let mut surf = vec![0, 0];
    // Each edge of the profile (counter-clockwise in x, y) a band across the width; the profile's
    // outward normal is to the right of its edge, and so is the band's.
    for i in 0..k {
        let j = (i + 1) % k;
        faces.push(vec![i, k + i, k + j, j]);
        let (p, q) = (prof[i as usize], prof[j as usize]);
        surf.push(if p[1] == q[1] { if p[1] == 0.0 { 3 } else { 1 } } else if p[0] == q[0] && i == 1 { 4 } else { 2 });
    }
    Poly { pts, faces, surf, names: ["sides", "treads", "risers", "bottom", "back"].map(String::from).to_vec() }
}
fn stairs_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(Geom::Solid(Rc::new(stairs_poly(a))))
}
fn stairs_closed(a: &Args) -> Measures {
    let (n, rise, run, w) = (a.f("steps"), a.f("rise"), a.f("run"), a.f("width"));
    let (side, per, c) = super::ornamenta::steps_measures(n, rise, run);
    let l = n * run;
    Measures { size: Some(side * w), boundary: Some(2.0 * side + per * w), centroid: Some([c[0], c[1], 0.0]), bounds: Some(([-0.5 * l, 0.0, -0.5 * w], [0.5 * l, n * rise, 0.5 * w])), series: None }
}
fn stairs_anchors(a: &Args) -> Vec<Anchor> {
    let (n, rise, run) = (a.n("steps"), a.f("rise"), a.f("run"));
    let l = n as f64 * run;
    let mut v = vec![out3("base", [0.0; 3], [0.0, -1.0, 0.0], On::Boundary), out3("landing", [0.5 * l - 0.5 * run, n as f64 * rise, 0.0], [0.0, 1.0, 0.0], On::Boundary), out3("foot", [-0.5 * l, 0.5 * rise, 0.0], [-1.0, 0.0, 0.0], On::Boundary)];
    v.extend((1..=n).take(MAX_NUMBERED).map(|i| out3(&format!("tread_{}", i - 1), [-0.5 * l + (i as f64 - 0.5) * run, i as f64 * rise, 0.0], [0.0, 1.0, 0.0], On::Boundary)));
    v
}

// ---------------------------------------------------------------- the chain link

const P_LINK: &[Param] = &[len("radius", "the ends' radius, to the wire's middle", 0.03), len("length", "the straight sides' length", 0.04), len("wire", "the wire's radius, under the ends'", 0.008)];
fn link_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, l, t) = (a.f("radius"), a.f("length"), a.f("wire"));
    if t >= r {
        return Err(refuse("wire_thicker_than_the_bend", "the wire's radius must be under the ends' (else the link closes its own hole)", &[("radius", r), ("wire", t)]));
    }
    Ok((r, l, t))
}
/// The link's middle line, a stadium in `(u, v) = (x, −z)`, counter-clockwise from its lower
/// straight's left end.
fn link_ring(r: f64, l: f64) -> Vec<Seg> {
    let h = 0.5 * l;
    vec![line([-h, -r], [h, -r]), arc([h, 0.0], r, -0.5 * PI, 0.5 * PI), line([h, r], [-h, r]), arc([-h, 0.0], r, 0.5 * PI, 1.5 * PI)]
}
fn link_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, l, t) = link_parts(a)?;
    let length = 2.0 * l + TAU * r;
    let exact = Measures {
        size: Some(PI * t * t * length),
        boundary: Some(TAU * t * length),
        centroid: Some([0.0, t, 0.0]),
        bounds: Some(([-0.5 * l - r - t, 0.0, -r - t], [0.5 * l + r + t, 2.0 * t, r + t])),
        series: None,
    };
    let sections = move |d: u32| 2 * (segments(0.5, d) + 1);
    Ok(Geom::Solid(Rc::new(Gen {
        count: Rc::new(move |d| {
            let n = sections(d) * segments(1.0, d);
            (n, n)
        }),
        build: Rc::new(move |d| (link_mesh(r, l, t, d), None)),
        exact,
        error: Rc::new(move |d| Some(link_error(r, t, d))),
        genus: 1,
        check: None,
        field: Some(Rc::new(move || Plan::wire(&[link_ring(r, l)], crate::detail::DETAIL_MIN, t, t))),
    })))
}
/// A bound on how far the build is from the link: a section's chord sag, and on the bends the
/// sag of a step of the turn at the wire's far side (the bends are a torus's, built as a solid of
/// revolution is: `Rev`'s bound).
fn link_error(r: f64, t: f64, d: u32) -> f64 {
    let k = segments(0.5, d) as f64;
    t * versin(PI / segments(1.0, d) as f64) + (r + t) * versin(0.5 * PI / k)
}
/// The link: `2 (k + 1)` sections (`k + 1` on each bend, its ends included) of `m` points round the
/// wire, each section in the plane across the middle line there; neighbouring sections joined by
/// quads. On a bend two sections are one turned about the bend's upright axis, so each quad has
/// two level sides of one direction (a trapezoid); along a straight one is the other moved, so
/// each quad is a parallelogram: every quad is planar. The sections lie in planes that meet only
/// on the bends' axes, `r` from the middle line where the wire is `t < r` thick, and the two
/// straights are `2r` apart: each strip of quads lies in its own wedge or slab and no two strips
/// meet but along a section, which is why nothing is searched.
fn link_mesh(r: f64, l: f64, t: f64, d: u32) -> Mesh {
    let k = segments(0.5, d) as usize;
    let mc = segments(1.0, d) as usize;
    let h = 0.5 * l;
    // Each section: its middle in (u, v) and its outward unit direction in the plane.
    let mut sec: Vec<(Pt, Pt)> = Vec::new();
    for (c, a0) in [([h, 0.0], -0.5 * PI), ([-h, 0.0], 0.5 * PI)] {
        for i in 0..=k {
            let a = a0 + PI * i as f64 / k as f64;
            let (s, co) = quarter_exact(a).unwrap_or_else(|| sin_cos(a));
            sec.push(([c[0] + r * co, c[1] + r * s], [co, s]));
        }
    }
    let mut points = Vec::new();
    let mut normals = Vec::new();
    for (c, nu) in &sec {
        for j in 0..mc {
            let (s, co) = sin_cos(TAU * j as f64 / mc as f64);
            // The direction across the wire: out in the plane (u, v) → (x, −z), and up.
            let e = [co * nu[0], s, -co * nu[1]];
            points.push([c[0] + t * e[0], t + t * e[1], -c[1] + t * e[2]]);
            normals.push(e);
        }
    }
    let ns = sec.len();
    let mut faces = Vec::new();
    for i in 0..ns {
        let i2 = (i + 1) % ns;
        // The bends' strips are sections 0..k and k+1..2k+1; the straights join k to k+1, 2k+1 to 0.
        let surface = if i == k || i == ns - 1 { 1 } else { 0 };
        for j in 0..mc {
            let j2 = (j + 1) % mc;
            let v = vec![(i * mc + j) as u32, (i2 * mc + j) as u32, (i2 * mc + j2) as u32, (i * mc + j2) as u32];
            let nn = v.iter().map(|&x| normals[x as usize]).collect();
            faces.push(Face { verts: v, surface, normals: Some(nn) });
        }
    }
    let surfaces = vec![Surface { name: "bends".into(), smooth: true, radius: Some(t) }, Surface { name: "sides".into(), smooth: true, radius: Some(t) }];
    Mesh { points, faces, surfaces, chord_error: link_error(r, t, d) }
}
fn link_anchors(a: &Args) -> Vec<Anchor> {
    let (r, l, t) = (a.f("radius"), a.f("length"), a.f("wire"));
    vec![
        at3("centre", [0.0, t, 0.0], On::Free),
        out3("end_right", [0.5 * l + r + t, t, 0.0], [1.0, 0.0, 0.0], On::Boundary),
        out3("end_left", [-0.5 * l - r - t, t, 0.0], [-1.0, 0.0, 0.0], On::Boundary),
        out3("top", [0.0, 2.0 * t, r], [0.0, 1.0, 0.0], On::Boundary),
        out3("base", [0.0, 0.0, r], [0.0, -1.0, 0.0], On::Boundary),
    ]
}

// ---------------------------------------------------------------- the table

const STANDS: &str = "the middle of its base, on y = 0";

pub(crate) const FORMS: &[Form] = &[
    solid!("rounded_cylinder", Cylinder, "a cylinder with its two rims rounded to a true radius (a puck, a coin, a button, a cushion)", STANDS, P_RCYL, curved: true, kin: &["circle"], rcyl_geom, rcyl_closed, rcyl_anchors),
    solid!("chamfered_box", Block, "a box with every edge cut at 45 degrees and a triangle at each corner (the bevelled cube of modelling tools)", STANDS, P_CBOX, curved: false, kin: &["rectangle"], cbox_geom, cbox_closed, cbox_anchors),
    solid!("stairs", Stair, "a straight flight of steps rising along +x, solid down to the floor", "the middle of its foot, on y = 0", P_STAIRS, curved: false, kin: &["stair_profile"], stairs_geom, stairs_closed, stairs_anchors),
    solid!("cup", Vessel, "a hollow vessel open at the top: a floor and a wall of one thickness measured across it, straight or tapered (a cup, a bucket, a pot)", STANDS, P_CUP, curved: true, kin: &["u_section"], cup_geom, cup_closed, cup_anchors),
    solid!("bell", Vessel, "a hollow bell open below: a lip band, a flare bowed in up to a straight waist, a round shoulder and a flat crown, its wall one thickness", "the middle of the mouth, on y = 0", P_BELL, curved: true, kin: &[], bell_geom, super::none, bell_anchors),
    solid!("goblet", Vessel, "a goblet: a hollow half-ball cup on a stem and a round foot", STANDS, P_GOBLET, curved: true, kin: &[], goblet_geom, super::none, goblet_anchors),
    solid!("bottle", Vessel, "a hollow bottle open at the mouth: a straight body, a shoulder curving in to the neck (an S, smooth at both ends), a neck and a lip ring; its wall one thickness", STANDS, P_BOTTLE, curved: true, kin: &[], bottle_geom, super::none, bottle_anchors),
    solid!("vase", Vessel, "a hollow vase open at the mouth: a foot, a waist, a belly, a neck and a lip flaring out, every join smooth; its wall one thickness down to a floor at the waist", STANDS, P_VASE, curved: true, kin: &[], vase_geom, super::none, vase_anchors),
    solid!("baluster", Lathe, "a turned baluster (a railing's post, a table's leg): a plinth, a bulb low down, a long neck tapering up, a collar and a cap, every curve smooth", STANDS, P_BALUSTER, curved: true, kin: &[], baluster_geom, super::none, baluster_anchors),
    solid!("mushroom", Lathe, "a mushroom: a stem under a spherical cap", STANDS, P_MUSHROOM, curved: true, kin: &[], mushroom_geom, mushroom_closed, mushroom_anchors),
    solid!("crystal", Crystal, "a crystal: a prism on a regular polygon with a pyramid on its top and, if asked, under its foot", "its lowest point, on y = 0", P_CRYSTAL, curved: false, kin: &["regular_polygon"], crystal_geom, crystal_closed, crystal_anchors),
    solid!("gem", Crystal, "a cut stone: a flat table, a crown of triangles, an upright girdle band (or a knife edge) and a pointed pavilion, every facet flat", "the pavilion's point, on y = 0", P_GEM, curved: false, kin: &["regular_polygon"], gem_geom, gem_closed, gem_anchors),
    solid!("chain_link", Torus, "a chain link: a round wire along a stadium (two half tori and two cylinders), lying flat", "the middle of the link, its lowest side on y = 0", P_LINK, curved: true, kin: &["stadium"], link_geom, super::none, link_anchors),
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("puck", Solid, "rounded_cylinder", "a rounded cylinder", &[]),
    alias!("tablet", Solid, "rounded_cylinder", "a rounded cylinder", &[]),
    alias!("bevelled_box", Solid, "chamfered_box", "a chamfered box", &[]),
    alias!("beveled_box", Solid, "chamfered_box", "a chamfered box", &[]),
    alias!("staircase", Solid, "stairs", "a flight of stairs", &[]),
    alias!("steps", Solid, "stairs", "a flight of stairs", &[]),
    alias!("bucket", Solid, "cup", "a cup wider at its rim", &[]),
    alias!("pail", Solid, "cup", "a cup wider at its rim", &[]),
    alias!("tumbler", Solid, "cup", "a cup", &[]),
    alias!("flowerpot", Solid, "cup", "a cup wider at its rim", &[]),
    alias!("chalice", Solid, "goblet", "a goblet", &[]),
    alias!("wine_glass", Solid, "goblet", "a goblet", &[]),
    alias!("urn", Solid, "vase", "a vase", &[]),
    alias!("toadstool", Solid, "mushroom", "a mushroom", &[]),
    alias!("gemstone", Solid, "gem", "a cut stone", &[]),
    alias!("jewel", Solid, "gem", "a cut stone", &[]),
    alias!("brilliant", Solid, "gem", "a cut stone", &[]),
    alias!("link", Solid, "chain_link", "a chain link", &[]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::Spec;
    use crate::table::Kind;

    /// The unit direction a piece runs in at its start (`at_end` false) or its end.
    fn heading(e: El, at_end: bool) -> Pt {
        let d = match e {
            El::Line(a, b) => sub(b, a),
            El::Arc { c, a, b, ccw, .. } => {
                let p = if at_end { b } else { a };
                let r = sub(p, c);
                if ccw {
                    [-r[1], r[0]]
                } else {
                    [r[1], -r[0]]
                }
            }
        };
        scale(d, 1.0 / crate::maths::len(d))
    }
    fn start(e: El) -> Pt {
        match e {
            El::Line(a, _) | El::Arc { a, .. } => a,
        }
    }

    /// Every join of `els` meets: the end of one is the start of the next (exactly, or to a
    /// rounding of the size), and where it is said to be smooth, the two run the same way there
    /// (to 1e-12 of a radian).
    fn joins(name: &str, els: &[El], smooth: &[bool]) {
        for (i, w) in els.windows(2).enumerate() {
            let (e, f) = (w[0].end(), start(w[1]));
            assert!(dist(e, f) <= 1e-12 * (1.0 + crate::maths::len(e)), "{name}: join {i} is {} apart", dist(e, f));
            if smooth[i] {
                let (h0, h1) = (heading(w[0], true), heading(w[1], false));
                let turn = (h0[0] * h1[1] - h0[1] * h1[0]).abs();
                assert!(turn <= 1e-12 && h0[0] * h1[0] + h0[1] * h1[1] > 0.0, "{name}: join {i} turns by {turn}");
            }
        }
    }

    fn args(name: &str, given: &[(&str, f64)]) -> Args {
        Spec::new(name, Some(Kind::Solid), given).unwrap().args.clone()
    }

    /// The picture check saw a faint line across the bottle's body near its shoulder and on the
    /// mushroom's stem: the profiles' joins are measured here. The bottle's, the vase's and the
    /// baluster's turned stretches meet with one point and one tangent at every join the form
    /// calls smooth (all of them but the foot's, the lip's and the plinth's and cap's ledges),
    /// and the bell's flare, waist and shoulder do. (The mushroom's stem is one line from the
    /// ground to the cap: it has no join to show; the line in that picture is a step of one
    /// pixel in its silhouette, which a camera above draws converging downward.)
    #[test]
    fn turned_joins_are_smooth() {
        for given in [vec![], vec![("shoulder_ratio", 1.0)], vec![("neck_ratio", 0.8), ("shoulder_ratio", 9.0)]] {
            let b = bottle_parts(&args("bottle", &given)).unwrap();
            joins("bottle", &b.outer, &[true, true, true]);
        }
        for given in [vec![], vec![("mouth_ratio", 1.06), ("waist_ratio", 0.9)]] {
            let v = vase_parts(&args("vase", &given)).unwrap();
            // The foot's band is upright and the S from it starts upright: every join smooth.
            joins("vase", &v.outer, &[true; 9]);
        }
        let b = baluster_parts(&args("baluster", &[])).unwrap();
        // Lines: the plinth's foot, side and top, then eight arcs, then the cap's ledge, side and
        // top: corners at the ledges.
        let mut smooth = vec![false, false, false];
        smooth.extend([true; 7]);
        smooth.extend([false, false, false]);
        joins("baluster", &b.els, &smooth);
        let b = bell_parts(&args("bell", &[])).unwrap();
        joins("bell", &b.outer, &[false, true, true]);
        // The S itself, any way round.
        for (a, z) in [([1.0, 0.0], [0.4, 2.0]), ([0.4, 0.0], [1.0, 0.6]), ([0.2, 1.0], [0.3, 3.5])] {
            let s = s_join(a, z).unwrap();
            joins("an S", &s, &[true]);
            assert!(dist(start(s[0]), a) == 0.0 && dist(s[1].end(), z) == 0.0);
            assert!(heading(s[0], false)[1] == 1.0 && heading(s[1], true)[1] == 1.0, "upright at both ends");
        }
    }
}
