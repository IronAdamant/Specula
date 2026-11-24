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

// ---------------------------------------------------------------- vessels

const P_CUP: &[Param] = &[len("radius", "the outside's radius at the bottom", 0.4), len("top", "the outside's radius at the rim", 0.45), len("height", "the height", 0.5), len("wall", "the wall's thickness, across", 0.03), len("floor", "the floor's thickness", 0.04)];
/// A cup's numbers and the inside's radius at the floor.
fn cup_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64, f64), Refusal> {
    let (r, t, h, w, f) = (a.f("radius"), a.f("top"), a.f("height"), a.f("wall"), a.f("floor"));
    if f >= h {
        return Err(refuse("floor_over_the_rim", "the floor must be thinner than the cup is tall", &[("height", h), ("floor", f)]));
    }
    let rf = r - w + (t - r) * (f / h);
    if rf <= 0.0 || w >= t {
        return Err(refuse("wall_fills_the_cup", "the wall must leave a hole at the floor and at the rim", &[("radius", r), ("top", t), ("wall", w), ("floor", f), ("inside_at_floor", rf)]));
    }
    Ok((r, t, h, w, f, rf))
}
fn cup_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t, h, w, f, rf) = cup_parts(a)?;
    turned(vec![
        piece(line(O, [r, 0.0]), "bottom"),
        piece(line([r, 0.0], [t, h]), "outside"),
        piece(line([t, h], [t - w, h]), "rim"),
        piece(line([t - w, h], [rf, f]), "inside"),
        piece(line([rf, f], [0.0, f]), "floor"),
    ])
}
fn frustum_v(h: f64, a: f64, b: f64) -> f64 {
    PI * h * (a * a + a * b + b * b) / 3.0
}
fn cup_closed(a: &Args) -> Measures {
    let Ok((r, t, h, w, f, rf)) = cup_parts(a) else { return Measures::default() };
    let ti = t - w;
    let v = frustum_v(h, r, t) - frustum_v(h - f, rf, ti);
    let area = PI * r * r + PI * (r + t) * ((t - r) * (t - r) + h * h).sqrt() + PI * (t + ti) * w + PI * (rf + ti) * ((ti - rf) * (ti - rf) + (h - f) * (h - f)).sqrt() + PI * rf * rf;
    Measures { size: Some(v), boundary: Some(area), centroid: None, bounds: Some(([-r.max(t), 0.0, -r.max(t)], [r.max(t), h, r.max(t)])), series: None }
}
fn cup_anchors(a: &Args) -> Vec<Anchor> {
    let (r, t, h, w, f) = (a.f("radius"), a.f("top"), a.f("height"), a.f("wall"), a.f("floor"));
    let mut v = round_anchors(On::Boundary, (h, On::Free), 0.0, r);
    v.retain(|x| !matches!(x.name.as_str(), "right" | "left" | "front" | "back"));
    v.push(out3("rim", [t - 0.5 * w, h, 0.0], [0.0, 1.0, 0.0], On::Boundary));
    v.push(out3("inside_floor", [0.0, f, 0.0], [0.0, 1.0, 0.0], On::Boundary));
    v.push(out3("side", [0.5 * (r + t), 0.5 * h, 0.0], [h, r - t, 0.0], On::Boundary));
    v
}

const P_BELL: &[Param] = &[len("radius", "the lip's radius", 0.5), ratio("crown", "the flat top's radius, as a share of the lip's", 0.4, 0.1, 0.9), ratio("height", "the height, as a share of the lip's radius", 1.2, 0.3, 3.0), ratio("wall", "the wall's thickness, as a share of the lip's radius", 0.06, 0.005, 0.3), ratio("curve", "the flare's radius as a share of its chord (from the crown's edge to the lip)", 1.5, 0.55, 4.0)];
/// A bell: the outside's flare an arc of radius `ρ` about `c` from the lip `(R, 0)` up to the
/// crown's edge `(rc, H)`, bowed in toward the axis; the inside the concentric arc `ρ + t` (a wall
/// of one thickness) from under the crown (`y = H − t`) to the floor. Returns the centre, `ρ`,
/// and the inside's radii at the crown and at the lip.
#[allow(clippy::type_complexity, reason = "a bell's numbers, read by its geometry and its anchors")]
fn bell_parts(a: &Args) -> Result<(f64, f64, f64, f64, Pt, f64, f64, f64), Refusal> {
    // (Its proportions are shares of the lip's radius, so that a bell drawn at random is mostly a
    // bell: four lengths drawn apart over eight decades almost never are.)
    let big = a.f("radius");
    let (rc, h, t, k) = (a.f("crown") * big, a.f("height") * big, a.f("wall") * big, a.f("curve"));
    let nums = [("radius", big), ("crown", a.f("crown")), ("height", a.f("height")), ("wall", a.f("wall")), ("curve", k)];
    if rc >= big {
        return Err(refuse("crown_over_the_lip", "the crown must be narrower than the lip", &nums));
    }
    if t >= h {
        return Err(refuse("wall_over_the_height", "the wall must be thinner than the bell is tall", &nums));
    }
    let chord = [rc - big, h];
    let lc = crate::maths::len(chord);
    let rho = k * lc;
    let kk = ((rho - 0.5 * lc) * (rho + 0.5 * lc)).sqrt();
    let c = [0.5 * (big + rc) + h / lc * kk, 0.5 * h + (big - rc) / lc * kk];
    if c[0] <= big {
        return Err(refuse("flare_too_deep", "the flare's arc must rise from the lip to the crown without dipping under the lip: a larger curve", &nums));
    }
    let ri = rho + t;
    let at_y = |y: f64| -> Option<f64> {
        let s = (ri - (y - c[1])) * (ri + (y - c[1]));
        (s >= 0.0).then(|| c[0] - s.sqrt())
    };
    let (Some(r_top), Some(r_lip)) = (at_y(h - t), at_y(0.0)) else {
        return Err(refuse("wall_too_thick_for_the_flare", "the inside's arc does not reach the crown and the lip", &nums));
    };
    let least = if c[1] > 0.0 && c[1] < h - t { c[0] - ri } else { r_top.min(r_lip) };
    if r_top <= 0.0 || least <= 0.0 {
        return Err(refuse("wall_too_thick_for_the_flare", "the inside would reach the axis: a thinner wall, a wider crown or a larger curve", &nums));
    }
    Ok((big, rc, h, t, c, rho, r_top, r_lip))
}
/// The angle of `p` about `c` taken in (π/2, 3π/2]: the side of the circle toward the axis.
fn inner_angle(p: Pt, c: Pt) -> f64 {
    let a = atan2(p[1] - c[1], p[0] - c[0]);
    if a < 0.0 {
        a + TAU
    } else {
        a
    }
}
fn bell_geom(a: &Args) -> Result<Geom, Refusal> {
    let (big, rc, h, t, c, rho, r_top, r_lip) = bell_parts(a)?;
    let (ci, li) = ([r_top, h - t], [r_lip, 0.0]);
    let (a_ci, a_li) = (inner_angle(ci, c), inner_angle(li, c));
    let (a_lo, a_co) = (inner_angle([big, 0.0], c), inner_angle([rc, h], c));
    turned(vec![
        piece(line([0.0, h - t], ci), "under_crown"),
        piece(arc_ends(c, rho + t, a_ci, a_li - a_ci, ci, li), "inside"),
        piece(line(li, [big, 0.0]), "lip"),
        piece(arc_ends(c, rho, a_lo, a_co - a_lo, [big, 0.0], [rc, h]), "outside"),
        piece(line([rc, h], [0.0, h]), "crown"),
    ])
}
fn bell_anchors(a: &Args) -> Vec<Anchor> {
    let big = a.f("radius");
    let (h, t) = (a.f("height") * big, a.f("wall") * big);
    vec![
        at3("mouth", [0.0; 3], On::Free),
        out3("top", [0.0, h, 0.0], [0.0, 1.0, 0.0], On::Boundary),
        out3("inside_top", [0.0, h - t, 0.0], [0.0, -1.0, 0.0], On::Boundary),
        at3("lip", [big, 0.0, 0.0], On::Boundary),
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
    ratio("height", "the height, as a share of the body's radius", 8.0, 2.0, 30.0),
    ratio("neck", "the neck's radius, as a share of the body's", 0.38, 0.05, 0.95),
    ratio("neck_length", "the neck's length, as a share of the height", 0.3, 0.02, 0.8),
    ratio("shoulder", "the shoulder's height over the body's radius less the neck's (at least 1: an arc of at most a quarter turn)", 1.7, 1.0, 20.0),
];
/// A bottle's radius, height, neck radius, shoulder height, the body's height and the shoulder arc's
/// radius. (Its proportions are shares of its radius, so that a bottle drawn at random is mostly a
/// bottle: five lengths drawn apart over eight decades almost never were.)
fn bottle_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64, f64), Refusal> {
    let r = a.f("radius");
    let (h, n) = (a.f("height") * r, a.f("neck") * r);
    let (ln, s) = (a.f("neck_length") * h, a.f("shoulder") * (r - n));
    let nums = [("radius", r), ("height", a.f("height")), ("neck", a.f("neck")), ("neck_length", a.f("neck_length")), ("shoulder", a.f("shoulder"))];
    let ys = h - ln - s;
    if ys <= 0.0 {
        return Err(refuse("neck_and_shoulder_over_the_height", "the neck and the shoulder must leave some body", &nums));
    }
    let k = r - n;
    Ok((r, h, n, s, ys, (k * k + s * s) / (2.0 * k)))
}
fn bottle_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, h, n, s, ys, rho) = bottle_parts(a)?;
    // The shoulder's centre `(r − ρ, ys)`; it ends at `(n, ys + s)`, at `(n − r + ρ, s)` from it.
    let k = r - n;
    let end = atan2(s, (s - k) * (s + k) / (2.0 * k));
    turned(vec![
        piece(line(O, [r, 0.0]), "base"),
        piece(line([r, 0.0], [r, ys]), "body"),
        piece(arc_ends([r - rho, ys], rho, 0.0, end, [r, ys], [n, ys + s]), "shoulder"),
        piece(line([n, ys + s], [n, h]), "neck"),
        piece(line([n, h], [0.0, h]), "mouth"),
    ])
}
fn bottle_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((r, h, n, _, ys, _)) = bottle_parts(a) else { return Vec::new() };
    // (Round the body at half its height: on the cylinder whatever the shoulder takes.)
    let mut v = round_anchors(On::Boundary, (h, On::Boundary), 0.5 * ys, r);
    v.push(at3("neck_top", [n, h, 0.0], On::Boundary));
    v
}

const P_VASE: &[Param] = &[
    len("radius", "the ball belly's radius", 0.12),
    len("foot", "the foot's radius, under the belly's", 0.06),
    len("neck", "the neck's radius where it leaves the belly, under the belly's", 0.05),
    len("mouth", "the mouth's radius", 0.07),
    len("neck_length", "the neck's length", 0.08),
];
fn vase_parts(a: &Args) -> Result<Seven, Refusal> {
    let (r, f, n, mo, ln) = (a.f("radius"), a.f("foot"), a.f("neck"), a.f("mouth"), a.f("neck_length"));
    if f >= r || n >= r {
        return Err(refuse("foot_or_neck_past_the_belly", "the foot and the neck must be narrower than the belly", &[("radius", r), ("foot", f), ("neck", n)]));
    }
    let c = ((r - f) * (r + f)).sqrt();
    let y1 = c + ((r - n) * (r + n)).sqrt();
    Ok((r, f, n, mo, ln, c, y1))
}
fn vase_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, f, n, mo, ln, c, y1) = vase_parts(a)?;
    let (a0, a1) = (atan2(-c, f), atan2(y1 - c, n));
    turned(vec![
        piece(line(O, [f, 0.0]), "foot"),
        piece(arc_ends([0.0, c], r, a0, a1 - a0, [f, 0.0], [n, y1]), "belly"),
        piece(line([n, y1], [mo, y1 + ln]), "neck"),
        piece(line([mo, y1 + ln], [0.0, y1 + ln]), "mouth"),
    ])
}
fn vase_closed(a: &Args) -> Measures {
    let Ok((r, f, n, mo, ln, c, y1)) = vase_parts(a) else { return Measures::default() };
    let zone = PI * (r * r * y1 - ((y1 - c) * (y1 - c) * (y1 - c) + c * c * c) / 3.0);
    let v = zone + frustum_v(ln, n, mo);
    let area = PI * f * f + TAU * r * y1 + PI * (n + mo) * ((mo - n) * (mo - n) + ln * ln).sqrt() + PI * mo * mo;
    let w = r.max(mo);
    Measures { size: Some(v), boundary: Some(area), centroid: None, bounds: Some(([-w, 0.0, -w], [w, y1 + ln, w])), series: None }
}
fn vase_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((r, _, _, mo, ln, c, y1)) = vase_parts(a) else { return Vec::new() };
    let mut v = round_anchors(On::Boundary, (y1 + ln, On::Boundary), c, r);
    v.push(at3("mouth_rim", [mo, y1 + ln, 0.0], On::Boundary));
    v
}

const P_BALUSTER: &[Param] = &[
    len("radius", "the ball's radius", 0.06),
    len("base", "the plinth's and the cap's radius", 0.05),
    len("thickness", "the plinth's and the cap's thickness", 0.03),
    len("neck", "the neck's radius, under the ball's and the plinth's", 0.025),
    len("neck_length", "the neck's length", 0.25),
];
fn baluster_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64, f64), Refusal> {
    let (r, b, t, n, ln) = (a.f("radius"), a.f("base"), a.f("thickness"), a.f("neck"), a.f("neck_length"));
    if n >= r || n >= b {
        return Err(refuse("neck_past_the_ball_or_plinth", "the neck must be narrower than the ball and the plinth", &[("radius", r), ("base", b), ("neck", n)]));
    }
    Ok((r, b, t, n, ln, ((r - n) * (r + n)).sqrt()))
}
fn baluster_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, b, t, n, ln, h) = baluster_parts(a)?;
    let al = atan2(h, n);
    let y2 = t + 2.0 * h + ln;
    turned(vec![
        piece(line(O, [b, 0.0]), "plinth_under"),
        piece(line([b, 0.0], [b, t]), "plinth"),
        piece(line([b, t], [n, t]), "plinth_top"),
        piece(arc_ends([0.0, t + h], r, -al, 2.0 * al, [n, t], [n, t + 2.0 * h]), "ball"),
        piece(line([n, t + 2.0 * h], [n, y2]), "neck"),
        piece(line([n, y2], [b, y2]), "cap_under"),
        piece(line([b, y2], [b, y2 + t]), "cap"),
        piece(line([b, y2 + t], [0.0, y2 + t]), "top"),
    ])
}
fn baluster_closed(a: &Args) -> Measures {
    let Ok((r, b, t, n, ln, h)) = baluster_parts(a) else { return Measures::default() };
    let v = 2.0 * PI * b * b * t + PI * (2.0 * r * r * h - 2.0 * h * h * h / 3.0) + PI * n * n * ln;
    let area = 2.0 * PI * b * b + 4.0 * PI * b * t + 2.0 * PI * (b - n) * (b + n) + 4.0 * PI * r * h + TAU * n * ln;
    let total = 2.0 * t + 2.0 * h + ln;
    let w = r.max(b);
    Measures { size: Some(v), boundary: Some(area), centroid: None, bounds: Some(([-w, 0.0, -w], [w, total, w])), series: None }
}
fn baluster_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((r, _, t, _, ln, h)) = baluster_parts(a) else { return Vec::new() };
    round_anchors(On::Boundary, (2.0 * t + 2.0 * h + ln, On::Boundary), t + h, r)
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
    ratio("table", "the flat top's circumradius as a share of the girdle's, under cos(180°/sides)", 0.55, 0.1, 0.95),
    len("crown", "from the girdle up to the table", 0.016),
    len("pavilion", "from the point below up to the girdle", 0.043),
];
fn gem_parts(a: &Args) -> Result<(usize, f64, f64, f64, f64), Refusal> {
    let (n, r, t, c, p) = (a.n("sides"), a.f("radius"), a.f("table"), a.f("crown"), a.f("pavilion"));
    // (With a margin: a table whose corner is on a girdle side to rounding makes a crown triangle
    // of no area.)
    if t >= cos(PI / n as f64) - 1e-6 {
        return Err(refuse("table_past_the_girdle", "the table's corners must lie inside the girdle's sides: table under cos(180°/sides)", &[("sides", n as f64), ("table", t)]));
    }
    Ok((n, r, t * r, c, p))
}
fn gem_poly(n: usize, r: f64, rt: f64, c: f64, p: f64) -> Poly {
    let step = TAU / n as f64;
    let mut pts = vec![[0.0; 3]];
    pts.extend((0..n).map(|i| ring_pt(r, p, step * i as f64)));
    pts.extend((0..n).map(|i| ring_pt(rt, p + c, step * (i as f64 + 0.5))));
    let nn = n as u32;
    let g = |i: u32| 1 + i % nn;
    let t = |i: u32| 1 + nn + i % nn;
    let mut faces = Vec::new();
    let mut surf = Vec::new();
    for i in 0..nn {
        faces.push(vec![0, g(i + 1), g(i)]);
        surf.push(2);
        faces.push(vec![g(i), g(i + 1), t(i)]);
        surf.push(1);
        faces.push(vec![t(i + nn - 1), g(i), t(i)]);
        surf.push(1);
    }
    faces.push((0..nn).map(t).collect());
    surf.push(0);
    // (The pavilion is a graph below the girdle and the crown and table one above it, both
    // over the girdle's polygon: each face is wound so that its normal points down below and up
    // above.)
    let faces = faces
        .into_iter()
        .zip(&surf)
        .map(|(mut f, &s)| {
            let mut area = [0.0; 3];
            for i in 0..f.len() {
                area = add3(area, cross3(pts[f[i] as usize], pts[f[(i + 1) % f.len()] as usize]));
            }
            if (area[1] > 0.0) != (s != 2) {
                f.reverse();
            }
            f
        })
        .collect();
    Poly { pts, faces, surf, names: ["table", "crown", "pavilion"].map(String::from).to_vec() }
}
fn gem_geom(a: &Args) -> Result<Geom, Refusal> {
    let (n, r, rt, c, p) = gem_parts(a)?;
    Ok(Geom::Solid(Rc::new(gem_poly(n, r, rt, c, p))))
}
fn gem_closed(a: &Args) -> Measures {
    let Ok((n, r, rt, c, p)) = gem_parts(a) else { return Measures::default() };
    let nf = n as f64;
    // The crown is a prismatoid (every point in the girdle's plane or the table's): its volume is
    // c (A₀ + 4 M + A₁) / 6, M the area of its middle section, the polygon of its slant edges'
    // middles (two an edge of the girdle: toward the table's corner before it and after it).
    let step = TAU / nf;
    let mids: Vec<Pt> = (0..2 * n)
        .map(|k| {
            let i = k / 2;
            let g = ring_pt(r, 0.0, step * i as f64);
            let tq = ring_pt(rt, 0.0, step * (i as f64 + if k % 2 == 0 { -0.5 } else { 0.5 }));
            [0.5 * (g[0] + tq[0]), 0.5 * (g[2] + tq[2])]
        })
        .collect();
    let mut twice = 0.0;
    for k in 0..mids.len() {
        let (u, w) = (mids[k], mids[(k + 1) % mids.len()]);
        twice += u[0] * w[1] - u[1] * w[0];
    }
    let mid = 0.5 * twice.abs();
    let v = ngon_area(nf, r) * p / 3.0 + c * (ngon_area(nf, r) + 4.0 * mid + ngon_area(nf, rt)) / 6.0;
    Measures { size: Some(v), ..Measures::default() }
}
fn gem_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((_, r, _, c, p)) = gem_parts(a) else { return Vec::new() };
    vec![at3("culet", [0.0; 3], On::Vertex), out3("table", [0.0, p + c, 0.0], [0.0, 1.0, 0.0], On::Boundary), at3("girdle_0", [0.0, p, -r], On::Vertex), at3("centre", [0.0, p, 0.0], On::Inside)]
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
    solid!("cup", Vessel, "a hollow vessel: a floor and a wall of one thickness, straight or tapered (a cup, a bucket, a pot)", STANDS, P_CUP, curved: true, kin: &["u_section"], cup_geom, cup_closed, cup_anchors),
    solid!("bell", Vessel, "a hollow bell: a flat crown and a flare bowed in, its wall one thickness (two concentric arcs)", "the middle of the mouth, on y = 0", P_BELL, curved: true, kin: &[], bell_geom, super::none, bell_anchors),
    solid!("goblet", Vessel, "a goblet: a hollow half-ball cup on a stem and a round foot", STANDS, P_GOBLET, curved: true, kin: &[], goblet_geom, super::none, goblet_anchors),
    solid!("bottle", Lathe, "a bottle: a body, a shoulder (an arc upright at the body) and a neck", STANDS, P_BOTTLE, curved: true, kin: &[], bottle_geom, super::none, bottle_anchors),
    solid!("vase", Lathe, "a vase: a ball belly on a foot, a neck flaring to the mouth (solid: no cavity)", STANDS, P_VASE, curved: true, kin: &[], vase_geom, vase_closed, vase_anchors),
    solid!("baluster", Lathe, "a baluster: a ball on a plinth, a neck and a cap (a railing's post, a table's leg)", STANDS, P_BALUSTER, curved: true, kin: &[], baluster_geom, baluster_closed, baluster_anchors),
    solid!("mushroom", Lathe, "a mushroom: a stem under a spherical cap", STANDS, P_MUSHROOM, curved: true, kin: &[], mushroom_geom, mushroom_closed, mushroom_anchors),
    solid!("crystal", Crystal, "a crystal: a prism on a regular polygon with a pyramid on its top and, if asked, under its foot", "its lowest point, on y = 0", P_CRYSTAL, curved: false, kin: &["regular_polygon"], crystal_geom, crystal_closed, crystal_anchors),
    solid!("gem", Crystal, "a cut stone: a flat table, a crown of triangles, a girdle and a pointed pavilion", "the pavilion's point, on y = 0", P_GEM, curved: false, kin: &["regular_polygon"], gem_geom, gem_closed, gem_anchors),
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
