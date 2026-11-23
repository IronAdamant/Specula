//! Plane figures of stage 4: moulding profiles, an arched wall, a flight's side, a grate, a
//! truss, a banner, a leaf and two fractals of a bounded depth. Every one is lines and arcs
//! only, so its count, its build, its exact measures, its error at a detail and its distance
//! field all follow from its pieces (geom.rs, field.rs) as for the first table's figures.
//!
//! **Mouldings** stand in the frame of a profile against a wall: the back on `x = 0`, the bed on
//! `y = 0`, the face toward `+x`, so that an outline by name sweeps along a wall with its back to
//! it. The two cymas are an S of two equal arcs, point-symmetric about the S's middle: their
//! areas are a polygon's (the two arcs' segments cancel), and each arc turns through at most a
//! quarter, which keeps the S monotone in height and its region simple.

use super::{ap, none};
use crate::anchor::{at, out, Anchor, On, MAX_NUMBERED};
use crate::error::{refuse, Refusal};
use crate::geom::{arc, arc_ends, line, poly, ring, Geom, Seg};
use crate::maths::*;
use crate::spec::Args;
use crate::table::{count, len, ratio, Alias, Form, Param, Set};
use crate::Measures;

const O: Pt = [0.0, 0.0];

/// The centroid of a figure as the moment of a whole less the moments of parts taken away.
fn centroid_less(whole: (f64, Pt), parts: &[(f64, Pt)]) -> [f64; 3] {
    let mut a = whole.0;
    let mut m = scale(whole.1, whole.0);
    for &(pa, pc) in parts {
        a -= pa;
        m = sub(m, scale(pc, pa));
    }
    [m[0] / a, m[1] / a, 0.0]
}

// ---------------------------------------------------------------- mouldings

const P_CAVETTO: &[Param] = &[len("width", "its projection from the wall, along x", 0.1), len("height", "along y", 0.1), len("radius", "the cove's radius, under the width and the height", 0.08)];
fn cavetto_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (w, h, r) = (a.f("width"), a.f("height"), a.f("radius"));
    if r >= w.min(h) {
        return Err(refuse("radius_past_the_flats", "the cove's radius must be under the width and the height (a flat is left on each side)", &[("width", w), ("height", h), ("radius", r)]));
    }
    Ok((w, h, r))
}
fn cavetto_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, r) = cavetto_parts(a)?;
    // The block less a quarter disc about its outer upper corner: the concave cove.
    Ok(ring(vec![line(O, [w, 0.0]), line([w, 0.0], [w, h - r]), arc([w, h], r, -0.5 * PI, -PI), line([w - r, h], [0.0, h]), line([0.0, h], O)]))
}
fn cavetto_closed(a: &Args) -> Measures {
    let Ok((w, h, r)) = cavetto_parts(a) else { return Measures::default() };
    let q = PI * r * r / 4.0;
    let k = 4.0 * r / (3.0 * PI);
    Measures { centroid: Some(centroid_less((w * h, [0.5 * w, 0.5 * h]), &[(q, [w - k, h - k])])), ..ap(Some(w * h - q), Some(2.0 * w + 2.0 * h - 2.0 * r + 0.5 * PI * r)) }
}
fn cavetto_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, r) = (a.f("width"), a.f("height"), a.f("radius"));
    let s = std::f64::consts::FRAC_1_SQRT_2;
    vec![
        out("back_foot", O, [-1.0, -1.0], On::Vertex),
        out("bed", [0.5 * w, 0.0], [0.0, -1.0], On::Boundary),
        out("back", [0.0, 0.5 * h], [-1.0, 0.0], On::Boundary),
        out("cove", [w - r * s, h - r * s], [1.0, 1.0], On::Boundary),
        at("top_back", [0.0, h], On::Vertex),
    ]
}

/// How much flatter than square an S may be.
const FLAT: f64 = 50.0;
const P_CYMA: &[Param] = &[len("width", "its projection from the wall, along x", 0.1), len("height", "along y", 0.1), len("fillet", "the flats at the S's ends", 0.012)];
const P_CYMA_REV: &[Param] = &[len("width", "its projection from the wall, along x", 0.08), len("height", "along y", 0.12), len("fillet", "the flats at the S's ends", 0.01)];

/// The cyma recta: its S from `(f, 0)` to `(w, h − f)`, level at both ends, the arc below
/// centred above its end (convex), the arc above centred below its end (concave); `(ρ, θ)` each
/// arc's radius and turn.
fn recta_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64), Refusal> {
    let (w, h, f) = (a.f("width"), a.f("height"), a.f("fillet"));
    let (dx, dy) = (0.5 * (w - f), 0.5 * (h - f));
    if dx <= 0.0 || dy <= 0.0 {
        return Err(refuse("fillet_past_the_profile", "the fillet must be under the width and the height", &[("width", w), ("height", h), ("fillet", f)]));
    }
    if dx < dy {
        return Err(refuse("taller_than_wide", "a cyma recta's S of two arcs of at most a quarter turn is no taller than it is wide (width − fillet at least height − fillet)", &[("width", w), ("height", h), ("fillet", f)]));
    }
    if dx > FLAT * dy {
        return Err(refuse("too_flat_for_its_arcs", "the S may be at most fifty times as wide as it is tall (width − fillet over height − fillet): flatter, its arcs' centres are so far off that the measures lose their digits", &[("width", w), ("height", h), ("fillet", f)]));
    }
    let rho = (dx * dx + dy * dy) / (2.0 * dy);
    // The middle seen from the lower arc's centre `(f, ρ)`: `(dx, dy − ρ)`, `dy − ρ = (dy − dx)(dy + dx) / 2dy`.
    let th = atan2(dx, (dx - dy) * (dx + dy) / (2.0 * dy));
    Ok((w, h, f, rho, th))
}
fn recta_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, f, rho, th) = recta_parts(a)?;
    let (p0, p1) = ([f, 0.0], [w, h - f]);
    let m = [0.5 * (f + w), 0.5 * (h - f)];
    Ok(ring(vec![
        line(O, p0),
        arc_ends([f, rho], rho, -0.5 * PI, th, p0, m),
        arc_ends([w, h - f - rho], rho, 0.5 * PI + th, -th, m, p1),
        line(p1, [w, h]),
        line([w, h], [0.0, h]),
        line([0.0, h], O),
    ]))
}
fn recta_closed(a: &Args) -> Measures {
    let Ok((w, h, f, rho, th)) = recta_parts(a) else { return Measures::default() };
    // (The S's two segments cut from its chord cancel: the area is the polygon's.)
    ap(Some(w * h - 0.5 * (w - f) * (h - f)), Some(w + h + 2.0 * f + 2.0 * rho * th))
}
fn recta_anchors(a: &Args) -> Vec<Anchor> {
    let mut v = moulding_anchors(a);
    if let Ok((w, h, f, rho, _)) = recta_parts(a) {
        let m = [0.5 * (f + w), 0.5 * (h - f)];
        v.push(out("s_middle", m, sub(m, [f, rho]), On::Vertex));
    }
    v
}
fn moulding_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("width"), a.f("height"));
    vec![out("back_foot", O, [-1.0, -1.0], On::Vertex), out("back", [0.0, 0.5 * h], [-1.0, 0.0], On::Boundary), out("top_back", [0.0, h], [-1.0, 1.0], On::Vertex), out("top_front", [w, h], [1.0, 1.0], On::Vertex)]
}

/// The cyma reversa: its S from `(f, f)` to `(w, h − f)`, upright at both ends, the arc below
/// centred to its right (concave), the arc above to its left (convex).
fn reversa_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64), Refusal> {
    let (w, h, f) = (a.f("width"), a.f("height"), a.f("fillet"));
    let (dx, dy) = (0.5 * (w - f), 0.5 * (h - 2.0 * f));
    if dx <= 0.0 || dy <= 0.0 {
        return Err(refuse("fillet_past_the_profile", "the fillet must be under the width and under half the height", &[("width", w), ("height", h), ("fillet", f)]));
    }
    if dy < dx {
        return Err(refuse("wider_than_tall", "a cyma reversa's S of two arcs of at most a quarter turn is no wider than it is tall (height − 2 fillets at least width − fillet)", &[("width", w), ("height", h), ("fillet", f)]));
    }
    if dy > FLAT * dx {
        return Err(refuse("too_tall_for_its_arcs", "the S may be at most fifty times as tall as it is wide (height − 2 fillets over width − fillet): taller, its arcs' centres are so far off that the measures lose their digits", &[("width", w), ("height", h), ("fillet", f)]));
    }
    let rho = (dx * dx + dy * dy) / (2.0 * dx);
    // The middle seen from the lower arc's centre `(f + ρ, f)`: `(dx − ρ, dy)`.
    let th = atan2(dy, (dy - dx) * (dy + dx) / (2.0 * dx));
    Ok((w, h, f, rho, th))
}
fn reversa_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, f, rho, th) = reversa_parts(a)?;
    let (p0, p1) = ([f, f], [w, h - f]);
    let m = [0.5 * (f + w), 0.5 * h];
    Ok(ring(vec![
        line(O, [f, 0.0]),
        line([f, 0.0], p0),
        arc_ends([f + rho, f], rho, PI, -th, p0, m),
        arc_ends([w - rho, h - f], rho, -th, th, m, p1),
        line(p1, [w, h]),
        line([w, h], [0.0, h]),
        line([0.0, h], O),
    ]))
}
fn reversa_closed(a: &Args) -> Measures {
    let Ok((w, h, f, rho, th)) = reversa_parts(a) else { return Measures::default() };
    ap(Some(w * h - 0.5 * (w - f) * h), Some(w + h + 3.0 * f + 2.0 * rho * th))
}
fn reversa_anchors(a: &Args) -> Vec<Anchor> {
    let mut v = moulding_anchors(a);
    if let Ok((w, h, f, rho, _)) = reversa_parts(a) {
        let m = [0.5 * (f + w), 0.5 * h];
        v.push(out("s_middle", m, sub([f + rho, f], m), On::Vertex));
    }
    v
}

// ---------------------------------------------------------------- an arched wall

const P_PORTAL: &[Param] = &[len("width", "the wall's width", 2.0), len("height", "the wall's height", 3.0), len("opening", "the opening's width (the arch's span)", 1.0), len("spring", "the opening's legs, from the floor to where the arch springs", 1.6)];
fn portal_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (big_w, big_h, w, h) = (a.f("width"), a.f("height"), a.f("opening"), a.f("spring"));
    if w >= big_w {
        return Err(refuse("opening_wider_than_the_wall", "the opening must be narrower than the wall", &[("width", big_w), ("opening", w)]));
    }
    if h + 0.5 * w >= big_h {
        return Err(refuse("opening_taller_than_the_wall", "the arch's crown (spring + half the opening) must be under the wall's height", &[("height", big_h), ("spring", h), ("opening", w)]));
    }
    Ok((big_w, big_h, w, h))
}
fn portal_geom(a: &Args) -> Result<Geom, Refusal> {
    let (bw, bh, w, h) = portal_parts(a)?;
    let (x, y) = (0.5 * bw, 0.5 * w);
    Ok(ring(vec![
        line([-x, 0.0], [-y, 0.0]),
        line([-y, 0.0], [-y, h]),
        arc([0.0, h], y, PI, 0.0),
        line([y, h], [y, 0.0]),
        line([y, 0.0], [x, 0.0]),
        line([x, 0.0], [x, bh]),
        line([x, bh], [-x, bh]),
        line([-x, bh], [-x, 0.0]),
    ]))
}
fn portal_closed(a: &Args) -> Measures {
    let Ok((bw, bh, w, h)) = portal_parts(a) else { return Measures::default() };
    let half = PI * w * w / 8.0;
    let c = centroid_less((bw * bh, [0.0, 0.5 * bh]), &[(w * h, [0.0, 0.5 * h]), (half, [0.0, h + 2.0 * w / (3.0 * PI)])]);
    Measures { centroid: Some(c), ..ap(Some(bw * bh - w * h - half), Some(2.0 * bw - w + 2.0 * bh + 2.0 * h + 0.5 * PI * w)) }
}
fn portal_anchors(a: &Args) -> Vec<Anchor> {
    let (bw, bh, w, h) = (a.f("width"), a.f("height"), a.f("opening"), a.f("spring"));
    let (x, y) = (0.5 * bw, 0.5 * w);
    vec![
        at("base", O, On::Free),
        out("crown", [0.0, h + y], [0.0, -1.0], On::Boundary),
        out("top", [0.0, bh], [0.0, 1.0], On::Boundary),
        out("foot_left", [-x, 0.0], [-1.0, -1.0], On::Vertex),
        out("foot_right", [x, 0.0], [1.0, -1.0], On::Vertex),
        at("jamb_left", [-y, 0.0], On::Vertex),
        at("jamb_right", [y, 0.0], On::Vertex),
        at("springing_left", [-y, h], On::Vertex),
        at("springing_right", [y, h], On::Vertex),
    ]
}

// ---------------------------------------------------------------- a flight's side

pub(crate) const P_STEPS: &[Param] = &[count("steps", "the number of steps", 5.0, 1.0, 100.0), len("rise", "each step's height", 0.18), len("run", "each tread's depth", 0.28)];
/// The side of a flight of `n` steps rising along +x, its middle at `x = 0`, standing on `y = 0`:
/// counter-clockwise from the front foot.
pub(crate) fn steps_pts(n: usize, rise: f64, run: f64) -> Vec<Pt> {
    let l = n as f64 * run;
    let x = |j: usize| -0.5 * l + j as f64 * run;
    let mut v = vec![[-0.5 * l, 0.0], [0.5 * l, 0.0], [0.5 * l, n as f64 * rise]];
    for i in (1..=n).rev() {
        v.push([x(i - 1), i as f64 * rise]);
        if i > 1 {
            v.push([x(i - 1), (i - 1) as f64 * rise]);
        }
    }
    v
}
/// A flight's side's area, perimeter and centroid.
pub(crate) fn steps_measures(n: f64, rise: f64, run: f64) -> (f64, f64, Pt) {
    let l = n * run;
    (run * rise * n * (n + 1.0) / 2.0, 2.0 * n * (run + rise), [-0.5 * l + run * (4.0 * n - 1.0) / 6.0, rise * (2.0 * n + 1.0) / 6.0])
}
fn steps_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(ring(poly(&steps_pts(a.n("steps"), a.f("rise"), a.f("run")))))
}
fn steps_closed(a: &Args) -> Measures {
    let (ar, p, c) = steps_measures(a.f("steps"), a.f("rise"), a.f("run"));
    Measures { centroid: Some([c[0], c[1], 0.0]), ..ap(Some(ar), Some(p)) }
}
fn steps_anchors(a: &Args) -> Vec<Anchor> {
    let (n, rise, run) = (a.n("steps"), a.f("rise"), a.f("run"));
    let l = n as f64 * run;
    let mut v = vec![out("front_foot", [-0.5 * l, 0.0], [-1.0, -1.0], On::Vertex), out("back_foot", [0.5 * l, 0.0], [1.0, -1.0], On::Vertex), out("top_back", [0.5 * l, n as f64 * rise], [1.0, 1.0], On::Vertex)];
    v.extend((1..=n).take(MAX_NUMBERED).map(|i| out(&format!("nosing_{}", i - 1), [-0.5 * l + (i - 1) as f64 * run, i as f64 * rise], [-1.0, 1.0], On::Vertex)));
    v
}

// ---------------------------------------------------------------- a grate and a truss

const P_GRATE: &[Param] = &[len("width", "along x", 1.0), len("height", "along y", 1.0), count("columns", "the holes across", 4.0, 1.0, 8.0), count("rows", "the holes up", 4.0, 1.0, 8.0), len("bar", "each bar's width, the border's too", 0.05)];
fn grate_parts(a: &Args) -> Result<(f64, f64, usize, usize, f64, f64, f64), Refusal> {
    let (w, h, c, r, b) = (a.f("width"), a.f("height"), a.n("columns"), a.n("rows"), a.f("bar"));
    let hw = (w - (c as f64 + 1.0) * b) / c as f64;
    let hh = (h - (r as f64 + 1.0) * b) / r as f64;
    if hw <= 0.0 || hh <= 0.0 {
        return Err(refuse("bars_fill_the_grate", "the bars leave no hole: (count + 1) bars must be narrower than the side", &[("width", w), ("height", h), ("columns", c as f64), ("rows", r as f64), ("bar", b)]));
    }
    Ok((w, h, c, r, b, hw, hh))
}
fn grate_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, c, r, b, hw, hh) = grate_parts(a)?;
    let mut rings = vec![poly(&[[-0.5 * w, -0.5 * h], [0.5 * w, -0.5 * h], [0.5 * w, 0.5 * h], [-0.5 * w, 0.5 * h]])];
    for j in 0..r {
        for i in 0..c {
            let x0 = -0.5 * w + b + i as f64 * (hw + b);
            let y0 = -0.5 * h + b + j as f64 * (hh + b);
            rings.push(poly(&[[x0, y0], [x0, y0 + hh], [x0 + hw, y0 + hh], [x0 + hw, y0]]));
        }
    }
    Ok(Geom::Rings(rings))
}
fn grate_closed(a: &Args) -> Measures {
    let Ok((w, h, c, r, _, hw, hh)) = grate_parts(a) else { return Measures::default() };
    let n = (c * r) as f64;
    Measures { centroid: Some([0.0; 3]), ..ap(Some(w * h - n * hw * hh), Some(2.0 * (w + h) + 2.0 * n * (hw + hh))) }
}
fn corners_anchors(w: f64, y0: f64, y1: f64) -> Vec<Anchor> {
    vec![
        out("corner_0", [-0.5 * w, y0], [-1.0, -1.0], On::Vertex),
        out("corner_1", [0.5 * w, y0], [1.0, -1.0], On::Vertex),
        out("corner_2", [0.5 * w, y1], [1.0, 1.0], On::Vertex),
        out("corner_3", [-0.5 * w, y1], [-1.0, 1.0], On::Vertex),
        out("bottom", [0.0, y0], [0.0, -1.0], On::Boundary),
        out("top", [0.0, y1], [0.0, 1.0], On::Boundary),
        out("left", [-0.5 * w, 0.5 * (y0 + y1)], [-1.0, 0.0], On::Boundary),
        out("right", [0.5 * w, 0.5 * (y0 + y1)], [1.0, 0.0], On::Boundary),
    ]
}
fn grate_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("width"), a.f("height"));
    let mut v = corners_anchors(w, -0.5 * h, 0.5 * h);
    v.push(at("centre", O, On::Free));
    v
}

const P_TRUSS: &[Param] = &[len("length", "along x", 4.0), len("height", "along y", 0.6), count("bays", "the diagonals' V's along it", 4.0, 1.0, 100.0), len("bar", "each chord's, post's and diagonal's width", 0.05)];
/// A line `ν · X = c` with its side `ν · X ≥ c`.
type Half = (Pt, f64);
/// The half-plane on `toward`'s side of the line through `p` and `q`, moved toward it by `d`.
fn half(p: Pt, q: Pt, toward: Pt, d: f64) -> Half {
    let e = sub(q, p);
    let l = crate::maths::len(e);
    let mut nu = [-e[1] / l, e[0] / l];
    if nu[0] * (toward[0] - p[0]) + nu[1] * (toward[1] - p[1]) < 0.0 {
        nu = [-nu[0], -nu[1]];
    }
    (nu, nu[0] * p[0] + nu[1] * p[1] + d)
}
fn meet(a: Half, b: Half) -> Pt {
    let det = a.0[0] * b.0[1] - a.0[1] * b.0[0];
    [(a.1 * b.0[1] - b.1 * a.0[1]) / det, (a.0[0] * b.1 - b.0[0] * a.1) / det]
}
/// The truss's holes, each counter-clockwise: the triangle of three half-planes, or the hole's
/// index where the bars leave none.
fn truss_holes(l: f64, h: f64, n: usize, b: f64) -> Result<Vec<[Pt; 3]>, usize> {
    let (x0, x1, y0, y1) = (-0.5 * l + b, 0.5 * l - b, b, h - b);
    let z = |j: usize| [x0 + (x1 - x0) * j as f64 / (2 * n) as f64, if j % 2 == 0 { y0 } else { y1 }];
    let mut out = Vec::new();
    // A hole is the triangle `t` of the inner band, shrunk by the half-planes `hs`.
    let mut take = |t: [Pt; 3], hs: [Half; 3]| -> Result<(), usize> {
        // (The sides are given as p q, q r, r p: the corners p, q, r where those meet.)
        let tri = [meet(hs[2], hs[0]), meet(hs[0], hs[1]), meet(hs[1], hs[2])];
        let area2 = |p: [Pt; 3]| (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        let (s, s0) = (area2(tri), area2(t));
        // A hole is there when its three lines bound a triangle whose middle is on the inner
        // side of every one. (Its turning alone is not enough: bars so wide that the diagonals'
        // lines meet past the chord turn the triangle over twice, below the chord.)
        let mid = scale(add(add(tri[0], tri[1]), tri[2]), 1.0 / 3.0);
        let within = hs.iter().all(|(nu, c)| nu[0] * mid[0] + nu[1] * mid[1] > *c);
        if !(s.is_finite() && s * s0 > 0.0 && within) {
            return Err(out.len());
        }
        out.push(if s > 0.0 { tri } else { [tri[0], tri[2], tri[1]] });
        Ok(())
    };
    // The left end: the post, the top chord and the first diagonal.
    let (p, q, c) = (z(0), z(1), [x0, y1]);
    let mid = |t: [Pt; 3]| scale(add(add(t[0], t[1]), t[2]), 1.0 / 3.0);
    let t = [p, q, c];
    take(t, [half(p, q, mid(t), 0.5 * b), half(q, c, mid(t), 0.0), half(c, p, mid(t), 0.0)])?;
    for j in 1..2 * n {
        let (p, q, r) = (z(j - 1), z(j), z(j + 1));
        let t = [p, q, r];
        take(t, [half(p, q, mid(t), 0.5 * b), half(q, r, mid(t), 0.5 * b), half(r, p, mid(t), 0.0)])?;
    }
    let (p, q, c) = (z(2 * n - 1), z(2 * n), [x1, y1]);
    let t = [p, q, c];
    take(t, [half(p, q, mid(t), 0.5 * b), half(q, c, mid(t), 0.0), half(c, p, mid(t), 0.0)])?;
    Ok(out)
}
fn truss_parts(a: &Args) -> Result<(f64, f64, Vec<[Pt; 3]>), Refusal> {
    let (l, h, n, b) = (a.f("length"), a.f("height"), a.n("bays"), a.f("bar"));
    let nums = [("length", l), ("height", h), ("bays", n as f64), ("bar", b)];
    if 2.0 * b >= l.min(h) {
        return Err(refuse("bars_fill_the_truss", "two bars must be narrower than the length and the height", &nums));
    }
    match truss_holes(l, h, n, b) {
        Ok(v) => Ok((l, h, v)),
        Err(i) => {
            let mut nums = nums.to_vec();
            nums.push(("hole", i as f64));
            Err(refuse("bars_fill_a_bay", "the diagonals, each a bar wide, leave no hole in a bay: fewer bays, a thinner bar or a taller truss", &nums))
        }
    }
}
fn truss_geom(a: &Args) -> Result<Geom, Refusal> {
    let (l, h, holes) = truss_parts(a)?;
    let mut rings = vec![poly(&[[-0.5 * l, 0.0], [0.5 * l, 0.0], [0.5 * l, h], [-0.5 * l, h]])];
    rings.extend(holes.iter().map(|t| poly(&[t[0], t[2], t[1]])));
    Ok(Geom::Rings(rings))
}
fn truss_anchors(a: &Args) -> Vec<Anchor> {
    let (l, h, n, b) = (a.f("length"), a.f("height"), a.n("bays"), a.f("bar"));
    let mut v = corners_anchors(l, 0.0, h);
    let (x0, x1) = (-0.5 * l + b, 0.5 * l - b);
    v.extend((0..=2 * n).take(MAX_NUMBERED).map(|j| at(&format!("node_{j}"), [x0 + (x1 - x0) * j as f64 / (2 * n) as f64, if j % 2 == 0 { b } else { h - b }], On::Inside)));
    v
}

// ---------------------------------------------------------------- a banner and a leaf

const P_BANNER: &[Param] = &[len("length", "from the hoist to the fly's tips, along x", 1.0), len("height", "along y", 0.6), len("notch", "the V cut into the fly, under the length", 0.25)];
fn banner_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (l, h, d) = (a.f("length"), a.f("height"), a.f("notch"));
    if d >= l {
        return Err(refuse("notch_past_the_hoist", "the notch must be shorter than the banner", &[("length", l), ("notch", d)]));
    }
    Ok((l, h, d))
}
fn banner_geom(a: &Args) -> Result<Geom, Refusal> {
    let (l, h, d) = banner_parts(a)?;
    Ok(ring(poly(&[[0.0, -0.5 * h], [l, -0.5 * h], [l - d, 0.0], [l, 0.5 * h], [0.0, 0.5 * h]])))
}
fn banner_closed(a: &Args) -> Measures {
    let Ok((l, h, d)) = banner_parts(a) else { return Measures::default() };
    let c = centroid_less((l * h, [0.5 * l, 0.0]), &[(0.5 * d * h, [l - d / 3.0, 0.0])]);
    Measures { centroid: Some(c), ..ap(Some(l * h - 0.5 * d * h), Some(2.0 * l + h + 2.0 * (d * d + 0.25 * h * h).sqrt())) }
}
fn banner_anchors(a: &Args) -> Vec<Anchor> {
    let (l, h, d) = (a.f("length"), a.f("height"), a.f("notch"));
    // A tail's tip: out along the bisector of its two sides (the top or foot, and the cut).
    let s = (d * d + 0.25 * h * h).sqrt();
    let tip = [1.0 + d / s, 0.5 * h / s];
    vec![
        out("hoist", O, [-1.0, 0.0], On::Boundary),
        out("hoist_top", [0.0, 0.5 * h], [-1.0, 1.0], On::Vertex),
        out("hoist_foot", [0.0, -0.5 * h], [-1.0, -1.0], On::Vertex),
        out("tail_top", [l, 0.5 * h], tip, On::Vertex),
        out("tail_foot", [l, -0.5 * h], [tip[0], -tip[1]], On::Vertex),
        at("notch", [l - d, 0.0], On::Vertex),
    ]
}

const P_LEAF: &[Param] = &[len("length", "from the base to the tip, along +y", 0.1), ratio("width", "at its widest, as a share of the length", 0.4, 0.02, 1.0), ratio("widest", "where it is widest, as a share of the length from the base", 0.4, 0.1, 0.9)];
/// A leaf's widest height, and its two arcs' radii and centres' `x` on the right side.
fn leaf_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64), Refusal> {
    let (l, k) = (a.f("length"), a.f("widest"));
    let w = a.f("width") * l;
    let (hw, yw) = (0.5 * w, k * l);
    if hw > yw || hw > l - yw {
        return Err(refuse("wider_than_its_arcs_allow", "half the width must be at most the widest point's height and its distance from the tip (each arc at most a quarter turn: a convex leaf)", &[("length", l), ("width", a.f("width")), ("widest", k)]));
    }
    let r1 = (hw * hw + yw * yw) / w;
    let r2 = (hw * hw + (l - yw) * (l - yw)) / w;
    Ok((l, hw, yw, r1, r2))
}
fn leaf_geom(a: &Args) -> Result<Geom, Refusal> {
    let (l, hw, yw, r1, r2) = leaf_parts(a)?;
    // Each side two arcs, upright at the widest point; the right side's centres on the widest
    // line at `x = hw − r` (at or left of the axis: a convex leaf).
    let a0 = atan2(-yw, r1 - hw);
    let a1 = atan2(l - yw, r2 - hw);
    let (wr, wl, tip) = ([hw, yw], [-hw, yw], [0.0, l]);
    Ok(ring(vec![
        arc_ends([hw - r1, yw], r1, a0, -a0, O, wr),
        arc_ends([hw - r2, yw], r2, 0.0, a1, wr, tip),
        arc_ends([r2 - hw, yw], r2, PI - a1, a1, tip, wl),
        arc_ends([r1 - hw, yw], r1, PI, -a0, wl, O),
    ]))
}
fn leaf_anchors(a: &Args) -> Vec<Anchor> {
    let (l, k) = (a.f("length"), a.f("widest"));
    let w = a.f("width") * l;
    vec![out("base", O, [0.0, -1.0], On::Vertex), out("tip", [0.0, l], [0.0, 1.0], On::Vertex), out("right", [0.5 * w, k * l], [1.0, 0.0], On::Vertex), out("left", [-0.5 * w, k * l], [-1.0, 0.0], On::Vertex), at("centre", [0.0, k * l], On::Inside)]
}

// ---------------------------------------------------------------- fractals

const P_KOCH: &[Param] = &[len("side", "the first triangle's side", 1.0), count("depth", "how many times each side is replaced by four", 3.0, 0.0, 5.0)];
fn koch_pts(s: f64, k: usize) -> Vec<Pt> {
    let r = s / 3f64.sqrt();
    let mut pts: Vec<Pt> = (0..3).map(|i| polar(O, r, 0.5 * PI + TAU * i as f64 / 3.0)).collect();
    for _ in 0..k {
        let n = pts.len();
        let mut next = Vec::with_capacity(4 * n);
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let d = scale(sub(b, a), 1.0 / 3.0);
            let p1 = add(a, d);
            // (The ring is counter-clockwise, so outward is to the right: the tooth turns −60°.)
            next.extend([a, p1, add(p1, rotate(d, -PI / 3.0)), add(a, scale(d, 2.0))]);
        }
        pts = next;
    }
    pts
}
fn koch_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(ring(poly(&koch_pts(a.f("side"), a.n("depth")))))
}
fn koch_closed(a: &Args) -> Measures {
    let (s, k) = (a.f("side"), a.n("depth"));
    let (mut f49, mut f43) = (1.0, 1.0);
    for _ in 0..k {
        f49 *= 4.0 / 9.0;
        f43 *= 4.0 / 3.0;
    }
    Measures { centroid: Some([0.0; 3]), ..ap(Some(3f64.sqrt() / 4.0 * s * s * (1.6 - 0.6 * f49)), Some(3.0 * s * f43)) }
}
fn koch_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("side") / 3f64.sqrt();
    let mut v: Vec<Anchor> = (0..3)
        .map(|i| {
            let t = 0.5 * PI + TAU * i as f64 / 3.0;
            out(&format!("tip_{i}"), polar(O, r, t), [cos(t), sin(t)], On::Vertex)
        })
        .collect();
    v.push(at("centre", O, On::Inside));
    v
}

const P_CARPET: &[Param] = &[len("size", "the square's side", 1.0), count("depth", "how many times each square's middle ninth is taken out", 2.0, 1.0, 3.0)];
fn carpet_holes(x0: f64, y0: f64, s: f64, k: usize, out: &mut Vec<Vec<Seg>>) {
    let t = s / 3.0;
    out.push(poly(&[[x0 + t, y0 + t], [x0 + t, y0 + 2.0 * t], [x0 + 2.0 * t, y0 + 2.0 * t], [x0 + 2.0 * t, y0 + t]]));
    if k > 1 {
        for j in 0..3 {
            for i in 0..3 {
                if i != 1 || j != 1 {
                    carpet_holes(x0 + i as f64 * t, y0 + j as f64 * t, t, k - 1, out);
                }
            }
        }
    }
}
fn carpet_geom(a: &Args) -> Result<Geom, Refusal> {
    let (s, k) = (a.f("size"), a.n("depth"));
    let h = 0.5 * s;
    let mut rings = vec![poly(&[[-h, -h], [h, -h], [h, h], [-h, h]])];
    carpet_holes(-h, -h, s, k, &mut rings);
    Ok(Geom::Rings(rings))
}
fn carpet_closed(a: &Args) -> Measures {
    let (s, k) = (a.f("size"), a.n("depth"));
    let (mut f89, mut f83) = (1.0, 1.0);
    for _ in 0..k {
        f89 *= 8.0 / 9.0;
        f83 *= 8.0 / 3.0;
    }
    Measures { centroid: Some([0.0; 3]), ..ap(Some(s * s * f89), Some(4.0 * s + 0.8 * s * (f83 - 1.0))) }
}
fn carpet_anchors(a: &Args) -> Vec<Anchor> {
    let s = a.f("size");
    let mut v = corners_anchors(s, -0.5 * s, 0.5 * s);
    v.push(at("centre", O, On::Free));
    v
}

// ---------------------------------------------------------------- the table

const SLAB: &[&str] = &["slab"];

pub(crate) const FORMS: &[Form] = &[
    plane!("cavetto", Moulding, "a cove moulding: a block less a quarter disc at its outer upper corner (back on x = 0, bed on y = 0)", "the back's foot", P_CAVETTO, curved: true, kin: SLAB, cavetto_geom, cavetto_closed, cavetto_anchors),
    plane!("cyma_recta", Moulding, "an ogee moulding: an S of two equal arcs, convex below and concave above, level at its ends (a crown moulding's)", "the back's foot", P_CYMA, curved: true, kin: SLAB, recta_geom, recta_closed, recta_anchors),
    plane!("cyma_reversa", Moulding, "a reverse ogee moulding: an S of two equal arcs, concave below and convex above, upright at its ends", "the back's foot", P_CYMA_REV, curved: true, kin: SLAB, reversa_geom, reversa_closed, reversa_anchors),
    plane!("arch_portal", Arch, "a wall with a round-arched opening from its floor (a doorway or a gate's wall)", "the middle of the opening's floor", P_PORTAL, curved: true, kin: SLAB, portal_geom, portal_closed, portal_anchors),
    plane!("stair_profile", Stair, "the side of a flight of steps rising along +x, down to the floor", "the middle of its foot", P_STEPS, curved: false, kin: &["stairs", "slab"], steps_geom, steps_closed, steps_anchors),
    plane!("grate", Frame, "a panel with a grid of rectangular holes (a grille, a vent, a window's bars)", "the centre", P_GRATE, curved: false, kin: SLAB, grate_geom, grate_closed, grate_anchors),
    plane!("truss", Frame, "a Warren truss panel: chords, end posts and diagonals of one width, with triangular holes", "the middle of its bottom edge", P_TRUSS, curved: false, kin: SLAB, truss_geom, none, truss_anchors),
    plane!("banner", Symbol, "a swallowtail banner: a rectangle with a V cut into its fly end", "the middle of the hoist edge", P_BANNER, curved: false, kin: SLAB, banner_geom, banner_closed, banner_anchors),
    plane!("leaf", Leaf, "a leaf (or a petal) of two arcs a side, pointed at its base and its tip, widest where asked", "the base", P_LEAF, curved: true, kin: SLAB, leaf_geom, none, leaf_anchors),
    plane!("koch_snowflake", Fractal, "the Koch snowflake to a depth: each side of a triangle replaced by four, again and again", "the centre", P_KOCH, curved: false, kin: SLAB, koch_geom, koch_closed, koch_anchors),
    plane!("sierpinski_carpet", Fractal, "the Sierpinski carpet to a depth: a square with its middle ninth taken out, and each of the eight left the same", "the centre", P_CARPET, curved: false, kin: SLAB, carpet_geom, carpet_closed, carpet_anchors),
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("cove", Plane, "cavetto", "a cove moulding", &[]),
    alias!("cove_moulding", Plane, "cavetto", "a cove moulding", &[]),
    alias!("ogee_moulding", Plane, "cyma_recta", "an ogee moulding", &[]),
    alias!("crown_moulding", Plane, "cyma_recta", "a crown moulding's S", &[]),
    alias!("reverse_ogee", Plane, "cyma_reversa", "a reverse ogee moulding", &[]),
    alias!("ovolo", Plane, "sector", "a quarter-round moulding (a quarter disc)", &[Set::Fixed("angle", 90.0)]),
    alias!("quarter_round", Plane, "sector", "a quarter-round moulding (a quarter disc)", &[Set::Fixed("angle", 90.0)]),
    alias!("bead", Plane, "semicircle", "a bead moulding (a half disc)", &[]),
    alias!("astragal", Plane, "semicircle", "an astragal (a half-round bead)", &[]),
    alias!("doorway", Plane, "arch_portal", "a wall with an arched doorway", &[]),
    alias!("archway", Plane, "arch_portal", "a wall with an arched opening", &[]),
    alias!("stair_side", Plane, "stair_profile", "a flight's side", &[]),
    alias!("grille", Plane, "grate", "a grate", &[]),
    alias!("grid_panel", Plane, "grate", "a grate", &[]),
    alias!("warren_truss", Plane, "truss", "a Warren truss panel", &[]),
    alias!("girder", Plane, "truss", "a truss panel", &[]),
    alias!("swallowtail", Plane, "banner", "a swallowtail banner", &[]),
    alias!("pennon", Plane, "banner", "a swallowtail banner", &[]),
    alias!("petal", Plane, "leaf", "a petal (a leaf's outline)", &[]),
    alias!("snowflake", Plane, "koch_snowflake", "the Koch snowflake", &[]),
    alias!("menger_carpet", Plane, "sierpinski_carpet", "the Sierpinski carpet", &[]),
];
