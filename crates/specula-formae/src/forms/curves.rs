//! Curves (open, and closed ones that cross themselves), and the bands that give a curve a
//! width as a closed plane figure.
//!
//! How each band takes its width, so that it is provably simple:
//! - the graphs (zigzag, parabola, catenary, sine wave) by a **vertical** thickness: the band is
//!   everything between the graph shifted down and up by half of it, so it is simple for any
//!   thickness and its area is exactly thickness × span. Where the curve is steep the band is
//!   thinner across it (by `1/√(1 + slope²)`);
//! - the elliptical arc between the ellipses of semi-axes `rx ± w/2`, `ry ± w/2` (nested, so
//!   simple), cut by the rays of its angle;
//! - the spirals by a **radial** width (along the radius): simple while the width is under the
//!   gap between turns;
//! - the involute by a true (normal) width: its offsets are involutes of the same circle, so
//!   simple while the width is under the gap between turns, 2π times the base radius.
//!   A thick arc is the `annular_sector`; a thick line segment is a `rectangle`.

use super::{ap, length, no_anchors, none};
use crate::anchor::{at, Anchor, On};
use crate::error::{refuse, Refusal};
use crate::geom::{func, func_rc, line, ring, Geom, PtFn, Seg};
use crate::maths::*;
use crate::spec::Args;
use crate::table::{count, deg, len, ratio, Alias, Form, Param};
use crate::Measures;
use std::rc::Rc;

const O: Pt = [0.0, 0.0];

/// A graph's function y = f(x).
type YFn = Rc<dyn Fn(f64) -> f64>;

fn path(segs: Vec<Seg>) -> Geom {
    Geom::Path { segs, closed: false }
}

/// A graph y = f(x) on [x0, x1] as one piece.
fn graph(f: YFn, x0: f64, x1: f64, turns: f64, dy: f64) -> Seg {
    func(move |x| [x, f(x) + dy], x0, x1, turns)
}

/// The band of a graph: `t` thick vertically.
fn graph_band(f: YFn, x0: f64, x1: f64, turns: f64, t: f64) -> Geom {
    let h = 0.5 * t;
    let (fa, fb) = (f(x0), f(x1));
    ring(vec![graph(f.clone(), x0, x1, turns, -h), line([x1, fb - h], [x1, fb + h]), graph(f, x1, x0, turns, h), line([x0, fa + h], [x0, fa - h])])
}

/// A graph band of straight pieces through `pts` (the zigzag's).
fn poly_band(pts: &[Pt], t: f64) -> Geom {
    let h = 0.5 * t;
    let mut s = Vec::new();
    for w in pts.windows(2) {
        s.push(line([w[0][0], w[0][1] - h], [w[1][0], w[1][1] - h]));
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    s.push(line([b[0], b[1] - h], [b[0], b[1] + h]));
    for w in pts.windows(2).rev() {
        s.push(line([w[1][0], w[1][1] + h], [w[0][0], w[0][1] + h]));
    }
    s.push(line([a[0], a[1] + h], [a[0], a[1] - h]));
    ring(s)
}

const P_THICK: Param = len("thickness", "the band's thickness, measured vertically", 0.05);

// ---------------------------------------------------------------- lines

const P_LINE: &[Param] = &[len("length", "end to end, along x", 1.0)];
fn line_geom(a: &Args) -> Result<Geom, Refusal> {
    let x = 0.5 * a.f("length");
    Ok(path(vec![line([-x, 0.0], [x, 0.0])]))
}
fn line_closed(a: &Args) -> Measures {
    length(a.f("length"))
}

const P_ZIG: &[Param] = &[len("length", "end to end, along x", 1.0), len("height", "from the low points to the high", 0.2), count("teeth", "the number of up-and-down pairs", 4.0, 1.0, 1000.0)];
const P_ZIGB: &[Param] = &[len("length", "end to end, along x", 1.0), len("height", "from the low points to the high", 0.2), count("teeth", "the number of up-and-down pairs", 4.0, 1.0, 1000.0), P_THICK];
fn zig_pts(a: &Args) -> Vec<Pt> {
    let (l, h, n) = (a.f("length"), a.f("height"), a.n("teeth"));
    (0..=2 * n).map(|i| [-0.5 * l + l * i as f64 / (2 * n) as f64, if i % 2 == 1 { 0.5 * h } else { -0.5 * h }]).collect()
}
fn zig_geom(a: &Args) -> Result<Geom, Refusal> {
    let p = zig_pts(a);
    Ok(path(p.windows(2).map(|w| line(w[0], w[1])).collect()))
}
fn zig_closed(a: &Args) -> Measures {
    let (l, h, n) = (a.f("length"), a.f("height"), a.f("teeth"));
    let run = l / (2.0 * n);
    length(2.0 * n * (run * run + h * h).sqrt())
}
fn zigb_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(poly_band(&zig_pts(a), a.f("thickness")))
}
fn graphb_closed(a: &Args) -> Measures {
    let span = if a.has("length") { a.f("length") } else { a.f("width") };
    ap(Some(span * a.f("thickness")), None)
}

// ---------------------------------------------------------------- arcs

const P_ARC: &[Param] = &[len("radius", "the radius", 0.5), deg("angle", "the angle swept, from +x counter-clockwise", 90.0, 1.0, 359.0)];
fn arc_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(path(vec![crate::geom::arc(O, a.f("radius"), 0.0, rad(a.f("angle")))]))
}
fn arc_anchors(a: &Args) -> Vec<Anchor> {
    vec![at("centre", O, On::Free), at("middle", polar(O, a.f("radius"), 0.5 * rad(a.f("angle"))), On::Boundary)]
}

/// The point of the ellipse of semi-axes `a`, `b` at polar angle `t`.
fn ellipse_polar(a: f64, b: f64, t: f64) -> Pt {
    let (s, c) = sin_cos(t);
    let r = a * b / ((b * c) * (b * c) + (a * s) * (a * s)).sqrt();
    [r * c, r * s]
}
/// The arc of the ellipse of semi-axes `a`, `b` between polar angles `t0` and `t1`, sampled
/// evenly in its parametric angle (even in the polar angle, a flat ellipse's chords would skip
/// most of its length near the long axis' ends).
fn ellipse_piece(a: f64, b: f64, t0: f64, t1: f64) -> Seg {
    let (u0, u1) = (param_angle(a, b, t0), param_angle(a, b, t1));
    func(move |u| [a * cos(u), b * sin(u)], u0, u1, (u1 - u0).abs() / TAU)
}
/// The ellipse's parametric angle at polar angle `t` in [0, 2π), unwrapped.
fn param_angle(a: f64, b: f64, t: f64) -> f64 {
    let (s, c) = sin_cos(t);
    let u = atan2(a * s, b * c);
    if u < 0.0 {
        u + TAU
    } else {
        u
    }
}

const P_EARC: &[Param] = &[len("rx", "the semi-axis along x", 0.6), len("ry", "the semi-axis along y", 0.35), deg("angle", "the polar angle swept, from +x counter-clockwise", 90.0, 1.0, 359.0)];
const P_EARCB: &[Param] = &[len("rx", "the middle ellipse's semi-axis along x", 0.6), len("ry", "the middle ellipse's semi-axis along y", 0.35), deg("angle", "the polar angle swept, from +x counter-clockwise", 90.0, 1.0, 359.0), len("width", "between the inner and outer ellipses, along each axis", 0.05)];
fn earc_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, ry, t) = (a.f("rx"), a.f("ry"), rad(a.f("angle")));
    Ok(path(vec![ellipse_piece(rx, ry, 0.0, t)]))
}
fn earcb_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (rx, ry, t, w) = (a.f("rx"), a.f("ry"), rad(a.f("angle")), a.f("width"));
    if 0.5 * w >= rx.min(ry) * (1.0 - 1e-6) {
        return Err(refuse("width_under_axes", "half the width must be under both semi-axes", &[("rx", rx), ("ry", ry), ("width", w)]));
    }
    Ok((rx, ry, t, 0.5 * w))
}
fn earcb_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, ry, t, h) = earcb_parts(a)?;
    let (oa, ob, ia, ib) = (rx + h, ry + h, rx - h, ry - h);
    Ok(ring(vec![
        ellipse_piece(oa, ob, 0.0, t),
        line(ellipse_polar(oa, ob, t), ellipse_polar(ia, ib, t)),
        ellipse_piece(ia, ib, t, 0.0),
        line([ia, 0.0], [oa, 0.0]),
    ]))
}
fn earcb_closed(a: &Args) -> Measures {
    let Ok((rx, ry, t, h)) = earcb_parts(a) else { return Measures::default() };
    // An ellipse's sector from parametric angle 0 to u has area a b u / 2.
    let (oa, ob, ia, ib) = (rx + h, ry + h, rx - h, ry - h);
    ap(Some(0.5 * (oa * ob * param_angle(oa, ob, t) - ia * ib * param_angle(ia, ib, t))), None)
}

// ---------------------------------------------------------------- graphs

const P_PARA: &[Param] = &[len("width", "end to end, along x", 1.0), len("height", "from the vertex up to the ends", 0.5)];
const P_PARAB: &[Param] = &[len("width", "end to end, along x", 1.0), len("height", "from the vertex up to the ends", 0.5), P_THICK];
fn para_f(a: &Args) -> (YFn, f64, f64) {
    let (w, h) = (a.f("width"), a.f("height"));
    let c = 4.0 * h / (w * w);
    (Rc::new(move |x| c * x * x), 0.5 * w, 2.0 * atan(4.0 * h / w) / TAU)
}
fn para_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, x, t) = para_f(a);
    Ok(path(vec![graph(f, -x, x, t, 0.0)]))
}
fn para_closed(a: &Args) -> Measures {
    let (w, h) = (a.f("width"), a.f("height"));
    let (c, x) = (4.0 * h / (w * w), 0.5 * w);
    let u = 2.0 * c * x;
    // ∫√(1 + (2cx)²) dx over [−X, X] = X √(1 + u²) + asinh(u) / (2c), u = 2cX.
    length(x * (1.0 + u * u).sqrt() + asinh(u) / (2.0 * c))
}
fn parab_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, x, t) = para_f(a);
    Ok(graph_band(f, -x, x, t, a.f("thickness")))
}

const P_CAT: &[Param] = &[len("width", "end to end, along x", 1.0), len("a", "the catenary's parameter (the radius of curvature at its lowest point)", 0.5)];
const P_CATB: &[Param] = &[len("width", "end to end, along x", 1.0), len("a", "the catenary's parameter (the radius of curvature at its lowest point)", 0.5), P_THICK];
fn cat_f(a: &Args) -> Result<(YFn, f64, f64), Refusal> {
    let (w, c) = (a.f("width"), a.f("a"));
    if 0.5 * w / c > 10.0 {
        return Err(refuse("not_too_steep", "half the width may be at most ten times the parameter a (the ends rise as cosh)", &[("width", w), ("a", c)]));
    }
    Ok((Rc::new(move |x| c * (cosh(x / c) - 1.0)), 0.5 * w, 2.0 * atan(sinh(0.5 * w / c)) / TAU))
}
fn cat_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, x, t) = cat_f(a)?;
    Ok(path(vec![graph(f, -x, x, t, 0.0)]))
}
fn cat_closed(a: &Args) -> Measures {
    let (w, c) = (a.f("width"), a.f("a"));
    length(2.0 * c * sinh(0.5 * w / c))
}
fn catb_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, x, t) = cat_f(a)?;
    Ok(graph_band(f, -x, x, t, a.f("thickness")))
}

const P_SINE: &[Param] = &[len("length", "end to end, along x", 2.0), len("amplitude", "the crests' height over the middle line", 0.2), ratio("cycles", "the number of whole waves along the length", 2.0, 0.25, 100.0), deg("phase", "the wave's phase at the left end", 0.0, -360.0, 360.0)];
const P_SINEB: &[Param] = &[len("length", "end to end, along x", 2.0), len("amplitude", "the crests' height over the middle line", 0.2), ratio("cycles", "the number of whole waves along the length", 2.0, 0.25, 100.0), deg("phase", "the wave's phase at the left end", 0.0, -360.0, 360.0), P_THICK];
fn sine_f(a: &Args) -> (YFn, f64, f64) {
    let (l, am, n, ph) = (a.f("length"), a.f("amplitude"), a.f("cycles"), rad(a.f("phase")));
    let k = TAU * n / l;
    (Rc::new(move |x| am * sin(k * (x + 0.5 * l) + ph)), 0.5 * l, n)
}
fn sine_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, x, t) = sine_f(a);
    Ok(path(vec![graph(f, -x, x, t, 0.0)]))
}
fn sineb_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, x, t) = sine_f(a);
    Ok(graph_band(f, -x, x, t, a.f("thickness")))
}

// ---------------------------------------------------------------- spirals

const P_ASP: &[Param] = &[
    crate::table::Param { min: 0.0, ..len("start", "the radius it starts at", 0.05) },
    len("pitch", "how much the radius grows in a turn", 0.1),
    ratio("turns", "how many turns", 3.0, 0.25, 50.0),
];
const P_ASPB: &[Param] = &[
    crate::table::Param { min: 0.0, ..len("start", "the middle line's radius at the start", 0.05) },
    len("pitch", "how much the radius grows in a turn", 0.1),
    ratio("turns", "how many turns", 3.0, 0.25, 50.0),
    len("width", "the band's width along the radius", 0.04),
];
fn asp_f(a: &Args, dr: f64) -> (PtFn, f64, f64) {
    let (s, p, n) = (a.f("start"), a.f("pitch"), a.f("turns"));
    let b = p / TAU;
    (Rc::new(move |t: f64| polar(O, s + b * t + dr, t)), TAU * n, n)
}
fn asp_geom(a: &Args) -> Result<Geom, Refusal> {
    let (f, t1, n) = asp_f(a, 0.0);
    Ok(path(vec![func_rc(&f, 0.0, t1, n)]))
}
fn asp_closed(a: &Args) -> Measures {
    let (s, p, n) = (a.f("start"), a.f("pitch"), a.f("turns"));
    let b = p / TAU;
    let g = |u: f64| 0.5 * b * (u * (1.0 + u * u).sqrt() + asinh(u));
    length(g((s + p * n) / b) - g(s / b))
}
fn aspb_geom(a: &Args) -> Result<Geom, Refusal> {
    let (s, p, w) = (a.f("start"), a.f("pitch"), a.f("width"));
    if w >= p * (1.0 - 1e-6) || 0.5 * w >= s * (1.0 - 1e-6) {
        return Err(refuse("width_fits", "the width must be under the pitch (the gap between turns), and half of it under the start radius", &[("start", s), ("pitch", p), ("width", w)]));
    }
    let (fo, t1, n) = asp_f(a, 0.5 * w);
    let (fi, _, _) = asp_f(a, -0.5 * w);
    Ok(ring(vec![func_rc(&fo, 0.0, t1, n), line(fo(t1), fi(t1)), func_rc(&fi, t1, 0.0, n), line(fi(0.0), fo(0.0))]))
}
fn aspb_closed(a: &Args) -> Measures {
    let (s, p, n, w) = (a.f("start"), a.f("pitch"), a.f("turns"), a.f("width"));
    let (b, t) = (p / TAU, TAU * n);
    ap(Some(w * (s * t + 0.5 * b * t * t)), None)
}

const P_LSP: &[Param] = &[len("start", "the radius it starts at", 0.05), ratio("growth", "how many times the radius grows in a turn", 1.5, 1.01, 100.0), ratio("turns", "how many turns", 3.0, 0.25, 20.0)];
const P_LSPB: &[Param] = &[len("start", "the middle line's radius at the start", 0.05), ratio("growth", "how many times the radius grows in a turn", 1.5, 1.01, 100.0), ratio("turns", "how many turns", 3.0, 0.25, 20.0), ratio("width", "the band's width along the radius as a share of the radius", 0.2, 0.001, 1.9)];
fn lsp_k(a: &Args) -> f64 {
    ln(a.f("growth")) / TAU
}
fn lsp_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (s, n) = (a.f("start"), a.f("turns"));
    let end = s * powf(a.f("growth"), n);
    if end > crate::table::LEN_MAX {
        return Err(refuse("end_in_range", "the spiral ends more than ten kilometres out", &[("end_radius", end)]));
    }
    Ok((s, lsp_k(a), TAU * n))
}
fn lsp_geom(a: &Args) -> Result<Geom, Refusal> {
    let (s, k, t1) = lsp_parts(a)?;
    let n = a.f("turns");
    Ok(path(vec![func(move |t| polar(O, s * exp(k * t), t), 0.0, t1, n)]))
}
fn lsp_closed(a: &Args) -> Measures {
    let Ok((s, k, t1)) = lsp_parts(a) else { return Measures::default() };
    length((1.0 + k * k).sqrt() / k * (s * exp(k * t1) - s))
}
fn lspb_geom(a: &Args) -> Result<Geom, Refusal> {
    let (s, k, t1) = lsp_parts(a)?;
    let (w, g, n) = (a.f("width"), a.f("growth"), a.f("turns"));
    if 1.0 + 0.5 * w >= g * (1.0 - 0.5 * w) * (1.0 - 1e-9) {
        return Err(refuse("width_fits", "the band's outer edge must stay inside the next turn's inner edge: (1 + width/2) < growth (1 − width/2)", &[("growth", g), ("width", w)]));
    }
    let (o, i) = (1.0 + 0.5 * w, 1.0 - 0.5 * w);
    let fo: PtFn = Rc::new(move |t: f64| polar(O, o * s * exp(k * t), t));
    let fi: PtFn = Rc::new(move |t: f64| polar(O, i * s * exp(k * t), t));
    Ok(ring(vec![func_rc(&fo, 0.0, t1, n), line(fo(t1), fi(t1)), func_rc(&fi, t1, 0.0, n), line(fi(0.0), fo(0.0))]))
}
fn lspb_closed(a: &Args) -> Measures {
    let Ok((s, k, t1)) = lsp_parts(a) else { return Measures::default() };
    let w = a.f("width");
    ap(Some(w * s * s * (exp(2.0 * k * t1) - 1.0) / (2.0 * k)), None)
}

const P_INV: &[Param] = &[len("radius", "the base circle's radius", 0.1), ratio("turns", "how far the string unwinds, in turns", 2.0, 0.1, 20.0)];
const P_INVB: &[Param] = &[len("radius", "the base circle's radius", 0.1), ratio("turns", "how far the string unwinds, in turns", 2.0, 0.1, 20.0), len("width", "the band's width across the curve", 0.05)];
fn involute(r: f64, d: f64) -> PtFn {
    // P(t) = r (cos t + t sin t, sin t − t cos t), moved d along (sin t, −cos t), away from
    // the circle.
    Rc::new(move |t: f64| {
        let (s, c) = sin_cos(t);
        [r * (c + t * s) + d * s, r * (s - t * c) - d * c]
    })
}
fn inv_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, n) = (a.f("radius"), a.f("turns"));
    Ok(path(vec![func_rc(&involute(r, 0.0), 0.0, TAU * n, n)]))
}
fn inv_closed(a: &Args) -> Measures {
    let (r, t) = (a.f("radius"), TAU * a.f("turns"));
    length(0.5 * r * t * t)
}
fn invb_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (r, n, w) = (a.f("radius"), a.f("turns"), a.f("width"));
    // The band starts where the curve's radius of curvature r t is the width (so the inner
    // offset, half the width in, is regular).
    let (t0, t1) = (w / r, TAU * n);
    if w >= TAU * r * (1.0 - 1e-6) || t1 <= t0 * (1.0 + 1e-6) {
        return Err(refuse("width_fits", "the width must be under the gap between turns (2π × radius), and the curve must unwind past width / radius radians", &[("radius", r), ("turns", n), ("width", w)]));
    }
    Ok((r, w, t0, t1))
}
fn invb_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, w, t0, t1) = invb_parts(a)?;
    let n = (t1 - t0) / TAU;
    let (fo, fi) = (involute(r, 0.5 * w), involute(r, -0.5 * w));
    Ok(ring(vec![func_rc(&fo, t0, t1, n), line(fo(t1), fi(t1)), func_rc(&fi, t1, t0, n), line(fi(t0), fo(t0))]))
}
fn invb_closed(a: &Args) -> Measures {
    let Ok((r, w, t0, t1)) = invb_parts(a) else { return Measures::default() };
    // A normal offset band of a curve, within its radius of curvature: area = width × length.
    ap(Some(w * 0.5 * r * (t1 * t1 - t0 * t0)), None)
}

// ---------------------------------------------------------------- closed curves

const P_LEMN: &[Param] = &[len("radius", "from the crossing to a lobe's far end", 0.5)];
fn lemn_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    let f: PtFn = Rc::new(move |t: f64| {
        let (s, c) = sin_cos(t);
        let d = 1.0 + s * s;
        [r * c / d, r * s * c / d]
    });
    Ok(Geom::Path { segs: (0..4).map(|q| func_rc(&f, 0.5 * PI * q as f64, 0.5 * PI * (q + 1) as f64, 0.25)).collect(), closed: true })
}
fn lemn_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    vec![at("crossing", O, On::Boundary), at("right", [r, 0.0], On::Vertex), at("left", [-r, 0.0], On::Vertex)]
}

const P_ROSE: &[Param] = &[count("k", "r = R cos(kθ): k petals when k is odd, 2k when even", 3.0, 1.0, 24.0), len("radius", "R, to a petal's tip", 0.5)];
fn rose_geom(a: &Args) -> Result<Geom, Refusal> {
    let (k, r) = (a.f("k"), a.f("radius"));
    let span = if a.n("k") % 2 == 1 { PI } else { TAU };
    let f: PtFn = Rc::new(move |t: f64| polar(O, r * cos(k * t), t));
    // A piece per petal, from tip to tip through the pole.
    let petals = if a.n("k") % 2 == 1 { a.n("k") } else { 2 * a.n("k") };
    let step = span / petals as f64;
    Ok(Geom::Path { segs: (0..petals).map(|j| func_rc(&f, step * j as f64, step * (j + 1) as f64, 1.0)).collect(), closed: true })
}

// ---------------------------------------------------------------- the table

pub(crate) const FORMS: &[Form] = &[
    curve!("line_segment", Line, "a straight segment along x", "the middle", P_LINE, curved: false, band: Some("rectangle"), kin: &[], line_geom, line_closed, no_anchors),
    curve!("zigzag", Line, "a zigzag polyline along x", "the middle of its span, halfway up", P_ZIG, curved: false, band: Some("zigzag_band"), kin: &[], zig_geom, zig_closed, no_anchors),
    curve!("arc", Arc, "a circular arc from +x counter-clockwise", "the circle's centre", P_ARC, curved: true, band: Some("annular_sector"), kin: &[], arc_geom, none, arc_anchors),
    curve!("elliptical_arc", Arc, "an arc of an ellipse, by polar angle from +x", "the ellipse's centre", P_EARC, curved: true, band: Some("elliptical_arc_band"), kin: &[], earc_geom, none, no_anchors),
    curve!("parabola", Arc, "an arc of the parabola y = c x², its vertex at the origin", "the vertex", P_PARA, curved: true, band: Some("parabola_band"), kin: &["paraboloid"], para_geom, para_closed, no_anchors),
    curve!("catenary", Arc, "a hanging chain, y = a (cosh(x / a) − 1)", "the lowest point", P_CAT, curved: true, band: Some("catenary_band"), kin: &[], cat_geom, cat_closed, no_anchors),
    curve!("sine_wave", Arc, "a sine wave along x", "the middle of its span, on the middle line", P_SINE, curved: true, band: Some("sine_band"), kin: &[], sine_geom, none, no_anchors),
    curve!("archimedean_spiral", Spiral, "the spiral r = start + pitch θ / 2π", "the pole", P_ASP, curved: true, band: Some("archimedean_spiral_band"), kin: &[], asp_geom, asp_closed, no_anchors),
    curve!("logarithmic_spiral", Spiral, "the spiral whose radius grows by a ratio each turn", "the pole", P_LSP, curved: true, band: Some("logarithmic_spiral_band"), kin: &[], lsp_geom, lsp_closed, no_anchors),
    curve!("involute", Spiral, "the involute of a circle: the path of a string's end unwound from it", "the base circle's centre", P_INV, curved: true, band: Some("involute_band"), kin: &[], inv_geom, inv_closed, no_anchors),
    curve!("lemniscate", Loop, "the lemniscate of Bernoulli, a figure eight crossing itself at the origin", "the crossing", P_LEMN, curved: true, band: None, kin: &[], lemn_geom, none, lemn_anchors),
    curve!("rose_curve", Loop, "the rose r = R cos(kθ), every petal through the pole", "the pole", P_ROSE, curved: true, band: None, kin: &[], rose_geom, none, no_anchors),
    plane!("zigzag_band", Band, "a zigzag given a vertical thickness", "the middle of its span, halfway up", P_ZIGB, curved: false, kin: &[], zigb_geom, graphb_closed, no_anchors),
    plane!("elliptical_arc_band", Band, "an elliptical arc given a width: between two nested ellipses, by polar angle", "the ellipses' centre", P_EARCB, curved: true, kin: &[], earcb_geom, earcb_closed, no_anchors),
    plane!("parabola_band", Band, "a parabola given a vertical thickness", "the vertex", P_PARAB, curved: true, kin: &[], parab_geom, graphb_closed, no_anchors),
    plane!("catenary_band", Band, "a catenary given a vertical thickness", "the lowest point", P_CATB, curved: true, kin: &[], catb_geom, graphb_closed, no_anchors),
    plane!("sine_band", Band, "a sine wave given a vertical thickness", "the middle of its span, on the middle line", P_SINEB, curved: true, kin: &[], sineb_geom, graphb_closed, no_anchors),
    plane!("archimedean_spiral_band", Band, "an Archimedean spiral given a width along the radius", "the pole", P_ASPB, curved: true, kin: &[], aspb_geom, aspb_closed, no_anchors),
    plane!("logarithmic_spiral_band", Band, "a logarithmic spiral given a width along the radius, as a share of it", "the pole", P_LSPB, curved: true, kin: &[], lspb_geom, lspb_closed, no_anchors),
    plane!("involute_band", Band, "an involute given a true width, from where its curvature allows", "the base circle's centre", P_INVB, curved: true, kin: &[], invb_geom, invb_closed, no_anchors),
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("segment", Curve, "line_segment", "a straight segment", &[]),
    alias!("polyline_zigzag", Curve, "zigzag", "a zigzag polyline", &[]),
    alias!("circular_arc", Curve, "arc", "a circular arc", &[]),
    alias!("chain_curve", Curve, "catenary", "a hanging chain's curve", &[]),
    alias!("sine", Curve, "sine_wave", "a sine wave", &[]),
    alias!("wave", Curve, "sine_wave", "a sine wave", &[]),
    alias!("spiral", Curve, "archimedean_spiral", "an Archimedean spiral", &[]),
    alias!("equiangular_spiral", Curve, "logarithmic_spiral", "the logarithmic spiral", &[]),
    alias!("figure_eight", Curve, "lemniscate", "a figure eight", &[]),
    alias!("rhodonea", Curve, "rose_curve", "the rose curve", &[]),
];
