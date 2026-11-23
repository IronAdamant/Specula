//! Curved plane figures: circles and their parts, ovals, arches, tracery, roulettes and polar
//! figures.

use super::{ap, none};
use crate::anchor::{self, at, out, Anchor, On};
use crate::error::{refuse, Refusal};
use crate::geom::{arc, arc_ends, func, func_rc, line, ring, Geom, PtFn, Seg};
use crate::maths::*;
use crate::spec::Args;
use crate::table::{count, deg, len, ratio, Alias, Form, Param, Set};
use crate::Measures;
use std::rc::Rc;

const O: Pt = [0.0, 0.0];

/// The four compass anchors of a figure symmetric about both axes, at `x` and `y`.
fn compass(x: f64, y: f64, on: On) -> Vec<Anchor> {
    vec![out("right", [x, 0.0], [1.0, 0.0], on), out("top", [0.0, y], [0.0, 1.0], on), out("left", [-x, 0.0], [-1.0, 0.0], on), out("bottom", [0.0, -y], [0.0, -1.0], on), at("centre", O, On::Inside)]
}

// ---------------------------------------------------------------- circles and their parts

const P_CIRCLE: &[Param] = &[len("radius", "the radius", 0.5)];
fn circle_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(ring(vec![arc(O, a.f("radius"), 0.0, TAU)]))
}
fn circle_closed(a: &Args) -> Measures {
    let r = a.f("radius");
    Measures { centroid: Some([0.0; 3]), ..ap(Some(PI * r * r), Some(TAU * r)) }
}
fn circle_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    compass(r, r, On::Boundary)
}

fn semicircle_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    Ok(ring(vec![arc(O, r, 0.0, PI), line([-r, 0.0], [r, 0.0])]))
}
fn semicircle_closed(a: &Args) -> Measures {
    let r = a.f("radius");
    Measures { centroid: Some([0.0, 4.0 * r / (3.0 * PI), 0.0]), ..ap(Some(0.5 * PI * r * r), Some(PI * r + 2.0 * r)) }
}
fn semicircle_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    vec![out("top", [0.0, r], [0.0, 1.0], On::Boundary), out("base", O, [0.0, -1.0], On::Boundary), at("corner_left", [-r, 0.0], On::Vertex), at("corner_right", [r, 0.0], On::Vertex)]
}

const P_SECTOR: &[Param] = &[len("radius", "the radius", 0.5), deg("angle", "the angle at the centre, from +x counter-clockwise", 60.0, 1.0, 359.0)];
fn sector_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    Ok(ring(vec![line(O, [r, 0.0]), arc(O, r, 0.0, t), line(polar(O, r, t), O)]))
}
fn sector_closed(a: &Args) -> Measures {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    ap(Some(0.5 * r * r * t), Some(r * t + 2.0 * r))
}
fn sector_anchors(a: &Args) -> Vec<Anchor> {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    vec![at("apex", O, On::Vertex), at("arc_start", [r, 0.0], On::Vertex), at("arc_end", polar(O, r, t), On::Vertex), out("arc_middle", polar(O, r, 0.5 * t), [cos(0.5 * t), sin(0.5 * t)], On::Boundary)]
}

fn segment_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    Ok(ring(vec![arc(O, r, 0.0, t), line(polar(O, r, t), [r, 0.0])]))
}
fn segment_closed(a: &Args) -> Measures {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    ap(Some(0.5 * r * r * (t - sin(t))), Some(r * t + 2.0 * r * sin(0.5 * t)))
}
fn segment_anchors(a: &Args) -> Vec<Anchor> {
    let (r, t) = (a.f("radius"), rad(a.f("angle")));
    vec![at("arc_start", [r, 0.0], On::Vertex), at("arc_end", polar(O, r, t), On::Vertex), out("arc_middle", polar(O, r, 0.5 * t), [cos(0.5 * t), sin(0.5 * t)], On::Boundary)]
}

const P_ANNULUS: &[Param] = &[len("radius", "the outer radius", 0.5), len("inner", "the hole's radius", 0.3)];
fn annulus_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, i) = (a.f("radius"), a.f("inner"));
    if i >= r {
        return Err(refuse("inner_inside_outer", "the hole must be smaller than the disc", &[("radius", r), ("inner", i)]));
    }
    Ok(Geom::Rings(vec![vec![arc(O, r, 0.0, TAU)], vec![arc(O, i, TAU, 0.0)]]))
}
fn annulus_closed(a: &Args) -> Measures {
    let (r, i) = (a.f("radius"), a.f("inner"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(PI * (r * r - i * i)), Some(TAU * (r + i))) }
}
fn annulus_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    compass(r, r, On::Boundary).into_iter().filter(|x| x.name != "centre").collect()
}

const P_ASECTOR: &[Param] = &[len("radius", "the outer radius", 0.5), len("inner", "the inner radius", 0.3), deg("angle", "the angle at the centre, from +x counter-clockwise", 90.0, 1.0, 359.0)];
fn asector_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, i, t) = (a.f("radius"), a.f("inner"), rad(a.f("angle")));
    if i >= r {
        return Err(refuse("inner_inside_outer", "the inner radius must be under the outer", &[("radius", r), ("inner", i)]));
    }
    Ok(ring(vec![arc(O, r, 0.0, t), line(polar(O, r, t), polar(O, i, t)), arc(O, i, t, 0.0), line([i, 0.0], [r, 0.0])]))
}
fn asector_closed(a: &Args) -> Measures {
    let (r, i, t) = (a.f("radius"), a.f("inner"), rad(a.f("angle")));
    ap(Some(0.5 * t * (r * r - i * i)), Some(t * (r + i) + 2.0 * (r - i)))
}

// Two circles: A of `radius` at the origin, B of `cut` at (offset, 0).
const P_LUNE: &[Param] = &[len("radius", "the circle the lune is cut from", 0.5), len("cut", "the circle cut away", 0.45), len("offset", "from the first circle's centre to the cut's, along +x", 0.25)];
fn lune_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64), Refusal> {
    let (ra, rb, d) = (a.f("radius"), a.f("cut"), a.f("offset"));
    if !(d > (ra - rb).abs() && d < ra + rb) {
        return Err(refuse("circles_cross", "the two circles must cross at two points: |radius - cut| < offset < radius + cut", &[("radius", ra), ("cut", rb), ("offset", d)]));
    }
    let x0 = (d * d + ra * ra - rb * rb) / (2.0 * d);
    let y0 = (ra * ra - x0 * x0).max(0.0).sqrt();
    Ok((ra, rb, d, x0, y0))
}
fn lune_geom(a: &Args) -> Result<Geom, Refusal> {
    let (ra, rb, d, x0, y0) = lune_parts(a)?;
    let phi = atan2(y0, x0);
    let (pm, pp) = (atan2(-y0, x0 - d), atan2(y0, x0 - d));
    Ok(ring(vec![arc(O, ra, phi, TAU - phi), arc([d, 0.0], rb, pm, pp - TAU)]))
}
/// The area two circles of radii `r1` and `r2`, `d` apart, have in common (crossing circles).
fn overlap(r1: f64, r2: f64, d: f64) -> f64 {
    let k = (-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2);
    r1 * r1 * acos((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)) + r2 * r2 * acos((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)) - 0.5 * k.max(0.0).sqrt()
}
fn lune_closed(a: &Args) -> Measures {
    let (ra, rb, d) = (a.f("radius"), a.f("cut"), a.f("offset"));
    ap(Some(PI * ra * ra - overlap(ra, rb, d)), None)
}
fn lune_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((ra, _, _, x0, y0)) = lune_parts(a) else { return Vec::new() };
    vec![at("horn_top", [x0, y0], On::Vertex), at("horn_bottom", [x0, -y0], On::Vertex), out("back", [-ra, 0.0], [-1.0, 0.0], On::Boundary)]
}

// Two equal circles, `separation` radii apart: their common part.
const P_LENS: &[Param] = &[len("radius", "each circle's radius", 0.5), ratio("separation", "the centres' distance as a share of the radius (1: the vesica piscis)", 1.0, 0.05, 1.95)];
fn lens_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, d) = (a.f("radius"), a.f("separation") * a.f("radius"));
    let y0 = (r * r - 0.25 * d * d).sqrt();
    let phi = atan2(y0, 0.5 * d);
    Ok(ring(vec![arc([-0.5 * d, 0.0], r, -phi, phi), arc([0.5 * d, 0.0], r, PI - phi, PI + phi)]))
}
fn lens_closed(a: &Args) -> Measures {
    let (r, d) = (a.f("radius"), a.f("separation") * a.f("radius"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(2.0 * r * r * acos(d / (2.0 * r)) - 0.5 * d * (4.0 * r * r - d * d).sqrt()), None) }
}
fn lens_anchors(a: &Args) -> Vec<Anchor> {
    let (r, d) = (a.f("radius"), a.f("separation") * a.f("radius"));
    let y0 = (r * r - 0.25 * d * d).sqrt();
    vec![at("tip_top", [0.0, y0], On::Vertex), at("tip_bottom", [0.0, -y0], On::Vertex), out("right", [r - 0.5 * d, 0.0], [1.0, 0.0], On::Boundary), out("left", [0.5 * d - r, 0.0], [-1.0, 0.0], On::Boundary), at("centre", O, On::Inside)]
}

const P_STADIUM: &[Param] = &[len("length", "end to end, along x", 1.0), len("radius", "the ends' radius (half the width)", 0.25)];
fn stadium_geom(a: &Args) -> Result<Geom, Refusal> {
    let (l, r) = (a.f("length"), a.f("radius"));
    if l <= 2.0 * r * (1.0 + 1e-9) {
        return Err(refuse("longer_than_wide", "the length must be over twice the radius (or it is a circle)", &[("length", l), ("radius", r)]));
    }
    let s = 0.5 * l - r;
    Ok(ring(vec![line([-s, -r], [s, -r]), arc([s, 0.0], r, -0.5 * PI, 0.5 * PI), line([s, r], [-s, r]), arc([-s, 0.0], r, 0.5 * PI, 1.5 * PI)]))
}
fn stadium_closed(a: &Args) -> Measures {
    let (l, r) = (a.f("length"), a.f("radius"));
    let s = l - 2.0 * r;
    Measures { centroid: Some([0.0; 3]), ..ap(Some(PI * r * r + 2.0 * r * s), Some(TAU * r + 2.0 * s)) }
}
fn stadium_anchors(a: &Args) -> Vec<Anchor> {
    compass(0.5 * a.f("length"), a.f("radius"), On::Boundary)
}

const P_RRECT: &[Param] = &[len("width", "along x", 1.0), len("height", "along y", 0.6), len("radius", "the corners' radius", 0.1)];
fn rrect_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, r) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("radius"));
    if r >= w.min(h) * (1.0 - 1e-9) {
        return Err(refuse("corners_fit", "the corner radius must be under half the width and half the height", &[("width", 2.0 * w), ("height", 2.0 * h), ("radius", r)]));
    }
    let (x, y) = (w - r, h - r);
    Ok(ring(vec![
        line([-x, -h], [x, -h]),
        arc([x, -y], r, -0.5 * PI, 0.0),
        line([w, -y], [w, y]),
        arc([x, y], r, 0.0, 0.5 * PI),
        line([x, h], [-x, h]),
        arc([-x, y], r, 0.5 * PI, PI),
        line([-w, y], [-w, -y]),
        arc([-x, -y], r, PI, 1.5 * PI),
    ]))
}
fn rrect_closed(a: &Args) -> Measures {
    let (w, h, r) = (a.f("width"), a.f("height"), a.f("radius"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(w * h - (4.0 - PI) * r * r), Some(2.0 * (w + h) - (8.0 - TAU) * r)) }
}
fn rrect_anchors(a: &Args) -> Vec<Anchor> {
    compass(0.5 * a.f("width"), 0.5 * a.f("height"), On::Boundary)
}

// ---------------------------------------------------------------- ovals

const P_ELLIPSE: &[Param] = &[len("rx", "the semi-axis along x", 0.6), len("ry", "the semi-axis along y", 0.35)];
fn ellipse_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, ry) = (a.f("rx"), a.f("ry"));
    Ok(ring(vec![func(move |t| [rx * cos(t), ry * sin(t)], 0.0, TAU, 1.0)]))
}
fn ellipse_closed(a: &Args) -> Measures {
    let (rx, ry) = (a.f("rx"), a.f("ry"));
    Measures {
        size: Some(PI * rx * ry),
        boundary: Some(ellipse_perimeter(rx, ry)),
        centroid: Some([0.0; 3]),
        bounds: Some(([-rx, -ry, 0.0], [rx, ry, 0.0])),
        series: Some("perimeter: the arithmetic-geometric mean (Gauss-Kummer) series, run to rounding"),
    }
}
fn ellipse_anchors(a: &Args) -> Vec<Anchor> {
    compass(a.f("rx"), a.f("ry"), On::Boundary)
}

const P_SUPER: &[Param] = &[len("rx", "the semi-axis along x", 0.5), len("ry", "the semi-axis along y", 0.5), ratio("exponent", "n in |x/rx|^n + |y/ry|^n = 1 (2: an ellipse; higher: squarer; under 1: a concave star)", 4.0, 0.25, 20.0)];
fn super_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, ry, n) = (a.f("rx"), a.f("ry"), a.f("exponent"));
    let p = 2.0 / n;
    let e = move |u: f64| {
        // cos u as sin(π/2 − u), so that both ends of a quarter are exact (0 and 1).
        let (s, c) = (sin(u), sin(0.5 * PI - u));
        (powf(c.abs(), p), powf(s.abs(), p))
    };
    // A quarter at a time, from each axis point, so that the axis points are exact vertices.
    let q: [PtFn; 4] = [
        Rc::new(move |u| {
            let (x, y) = e(u);
            [rx * x, ry * y]
        }),
        Rc::new(move |u| {
            let (x, y) = e(u);
            [-rx * y, ry * x]
        }),
        Rc::new(move |u| {
            let (x, y) = e(u);
            [-rx * x, -ry * y]
        }),
        Rc::new(move |u| {
            let (x, y) = e(u);
            [rx * y, -ry * x]
        }),
    ];
    Ok(ring(q.iter().map(|f| func_rc(f, 0.0, 0.5 * PI, 0.25)).collect()))
}
fn super_closed(a: &Args) -> Measures {
    let (rx, ry, n) = (a.f("rx"), a.f("ry"), a.f("exponent"));
    let g = gamma(1.0 + 1.0 / n);
    Measures { centroid: Some([0.0; 3]), bounds: Some(([-rx, -ry, 0.0], [rx, ry, 0.0])), ..ap(Some(4.0 * rx * ry * g * g / gamma(1.0 + 2.0 / n)), None) }
}
fn super_anchors(a: &Args) -> Vec<Anchor> {
    compass(a.f("rx"), a.f("ry"), On::Vertex)
}

// Moss's egg: a half circle below, two arcs of twice the radius from the ends of its diameter,
// and a small arc at the top from the half circle's top point.
pub(crate) const P_MOSS: &[Param] = &[len("radius", "the lower half circle's radius (half the egg's width)", 0.5)];
fn moss_geom(a: &Args) -> Result<Geom, Refusal> {
    let r = a.f("radius");
    let rho = (2.0 - 2f64.sqrt()) * r;
    Ok(ring(vec![arc(O, r, PI, TAU), arc([-r, 0.0], 2.0 * r, 0.0, 0.25 * PI), arc([0.0, r], rho, 0.25 * PI, 0.75 * PI), arc([r, 0.0], 2.0 * r, 0.75 * PI, PI)]))
}
fn moss_closed(a: &Args) -> Measures {
    let r = a.f("radius");
    ap(Some(r * r * ((3.0 - 2f64.sqrt()) * PI - 1.0)), Some(PI * r * (3.0 - 0.5 * 2f64.sqrt())))
}
fn moss_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    vec![out("top", [0.0, r + (2.0 - 2f64.sqrt()) * r], [0.0, 1.0], On::Boundary), out("bottom", [0.0, -r], [0.0, -1.0], On::Boundary), out("right", [r, 0.0], [1.0, 0.0], On::Vertex), out("left", [-r, 0.0], [-1.0, 0.0], On::Vertex)]
}

// The four-centre oval of the drafting construction: join the ends of the axes, mark off the
// difference of the semi-axes from the minor end, and bisect the rest: the bisector's crossings
// with the axes are the small and large arcs' centres.
const P_OVAL: &[Param] = &[len("width", "along x (the long axis)", 1.0), len("height", "along y (the short axis)", 0.6)];
fn oval_parts(a: &Args) -> Result<(f64, f64, f64, f64, f64, f64), Refusal> {
    let (w, h) = (a.f("width"), a.f("height"));
    if h >= w * (1.0 - 1e-6) {
        return Err(refuse("long_axis_is_x", "the height must be under the width (the long axis is x; turn the outline for a tall oval)", &[("width", w), ("height", h)]));
    }
    // Flatter than 100 : 1 the construction's large arcs grow past a hundred times the width,
    // and its measures lose digits to cancellation (seen at 16,000 : 1, where the typed area
    // and the geometry's differed in the fifth digit).
    if h * 100.0 < w {
        return Err(refuse("not_too_flat", "the height must be at least a hundredth of the width", &[("width", w), ("height", h)]));
    }
    let (ra, rb) = (0.5 * w, 0.5 * h);
    let l = (ra * ra + rb * rb).sqrt();
    let u = [ra / l, -rb / l]; // from the minor end C=(0, rb) toward A=(ra, 0)
    let e = [u[0] * (ra - rb), rb + u[1] * (ra - rb)];
    let m = [0.5 * (ra + e[0]), 0.5 * e[1]];
    // The bisector m + λ (rb, ra)/l.
    let p1 = m[0] - m[1] * rb / ra;
    let p2 = m[1] - m[0] * ra / rb;
    Ok((ra, rb, p1, p2, ra - p1, rb - p2))
}
fn oval_geom(a: &Args) -> Result<Geom, Refusal> {
    let (_, _, p1, p2, r1, r2) = oval_parts(a)?;
    let al = atan2(-p2, p1);
    Ok(ring(vec![arc([p1, 0.0], r1, -al, al), arc([0.0, p2], r2, al, PI - al), arc([-p1, 0.0], r1, PI - al, PI + al), arc([0.0, -p2], r2, PI + al, TAU - al)]))
}
fn oval_closed(a: &Args) -> Measures {
    let Ok((_, _, p1, p2, r1, r2)) = oval_parts(a) else { return Measures::default() };
    let al = atan2(-p2, p1);
    let quarter = 0.5 * r2 * r2 * (0.5 * PI - al) - 0.5 * p1 * (-p2) + 0.5 * r1 * r1 * al;
    Measures { centroid: Some([0.0; 3]), ..ap(Some(4.0 * quarter), Some(4.0 * (r2 * (0.5 * PI - al) + r1 * al))) }
}
fn oval_anchors(a: &Args) -> Vec<Anchor> {
    compass(0.5 * a.f("width"), 0.5 * a.f("height"), On::Boundary)
}

// Two half ellipses on one long axis: a blunt end and a pointed end.
const P_OVOID: &[Param] = &[len("width", "across the widest line", 0.8), len("top", "from the widest line up to the top", 0.6), len("bottom", "from the widest line down to the bottom", 0.4)];
fn ovoid_geom(a: &Args) -> Result<Geom, Refusal> {
    let (rx, t, b) = (0.5 * a.f("width"), a.f("top"), a.f("bottom"));
    Ok(ring(vec![func(move |u| [rx * cos(u), t * sin(u)], 0.0, PI, 0.5), func(move |u| [rx * cos(u), b * sin(u)], PI, TAU, 0.5)]))
}
fn ovoid_closed(a: &Args) -> Measures {
    let (rx, t, b) = (0.5 * a.f("width"), a.f("top"), a.f("bottom"));
    Measures {
        centroid: Some([0.0, 4.0 * (t * t - b * b) / (3.0 * PI * (t + b)), 0.0]),
        bounds: Some(([-rx, -b, 0.0], [rx, t, 0.0])),
        boundary: Some(0.5 * (ellipse_perimeter(rx, t) + ellipse_perimeter(rx, b))),
        series: Some("perimeter: the arithmetic-geometric mean (Gauss-Kummer) series, run to rounding"),
        ..ap(Some(0.5 * PI * rx * (t + b)), None)
    }
}
fn ovoid_anchors(a: &Args) -> Vec<Anchor> {
    let (rx, t, b) = (0.5 * a.f("width"), a.f("top"), a.f("bottom"));
    vec![out("top", [0.0, t], [0.0, 1.0], On::Boundary), out("bottom", [0.0, -b], [0.0, -1.0], On::Boundary), out("right", [rx, 0.0], [1.0, 0.0], On::Vertex), out("left", [-rx, 0.0], [-1.0, 0.0], On::Vertex)]
}

pub(crate) const P_DROP: &[Param] = &[len("radius", "the round end's radius", 0.3), len("length", "from the round end's centre to the tip, along +y", 0.8)];
pub(crate) fn drop_parts(a: &Args) -> Result<(f64, f64, f64), Refusal> {
    let (r, l) = (a.f("radius"), a.f("length"));
    if l <= r * (1.0 + 1e-6) {
        return Err(refuse("tip_outside_circle", "the tip must lie outside the round end: length over radius", &[("radius", r), ("length", l)]));
    }
    Ok((r, l, acos(r / l)))
}
fn drop_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, l, b) = drop_parts(a)?;
    let tip = [0.0, l];
    let (t0, t1) = (0.5 * PI + b, 0.5 * PI - b + TAU);
    Ok(ring(vec![line(tip, polar(O, r, t0)), arc(O, r, t0, t1), line(polar(O, r, t1), tip)]))
}
fn drop_closed(a: &Args) -> Measures {
    let Ok((r, l, b)) = drop_parts(a) else { return Measures::default() };
    let t = (l * l - r * r).sqrt();
    ap(Some(r * t + r * r * (PI - b)), Some(2.0 * t + r * (TAU - 2.0 * b)))
}
fn drop_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    vec![out("tip", [0.0, a.f("length")], [0.0, 1.0], On::Vertex), out("bottom", [0.0, -r], [0.0, -1.0], On::Boundary), at("centre", O, On::Inside)]
}

// The classic heart curve x = 16 sin³t, y = 13 cos t − 5 cos 2t − 2 cos 3t − cos 4t, scaled;
// traced with x mirrored so that it runs counter-clockwise.
const P_HEART: &[Param] = &[len("width", "across the lobes (32 units of the curve)", 1.0)];
fn heart_geom(a: &Args) -> Result<Geom, Refusal> {
    let k = a.f("width") / 32.0;
    let f: PtFn = Rc::new(move |t: f64| {
        let s = sin(t);
        [-16.0 * k * s * s * s, k * (13.0 * cos(t) - 5.0 * cos(2.0 * t) - 2.0 * cos(3.0 * t) - cos(4.0 * t))]
    });
    Ok(ring(vec![func_rc(&f, 0.0, PI, 0.5), func_rc(&f, PI, TAU, 0.5)]))
}
fn heart_closed(a: &Args) -> Measures {
    let k = a.f("width") / 32.0;
    // ∮ x dy with x = 12 sin t − 4 sin 3t: only the sin t·sin t and sin 3t·sin 3t terms survive.
    ap(Some(180.0 * PI * k * k), None)
}
fn heart_anchors(a: &Args) -> Vec<Anchor> {
    let k = a.f("width") / 32.0;
    vec![out("point", [0.0, -17.0 * k], [0.0, -1.0], On::Vertex), at("notch", [0.0, 5.0 * k], On::Vertex)]
}

// ---------------------------------------------------------------- arches (the opening's shape)

const P_ARCH: &[Param] = &[len("width", "the span", 1.0), len("height", "the legs' height up to the springing line", 1.0)];
fn legs(w: f64, h: f64) -> Vec<Seg> {
    vec![line([-0.5 * w, 0.0], [0.5 * w, 0.0]), line([0.5 * w, 0.0], [0.5 * w, h])]
}
fn arch_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h) = (a.f("width"), a.f("height"));
    let mut s = legs(w, h);
    s.push(arc([0.0, h], 0.5 * w, 0.0, PI));
    s.push(line([-0.5 * w, h], [-0.5 * w, 0.0]));
    Ok(ring(s))
}
fn arch_closed(a: &Args) -> Measures {
    let (w, h) = (a.f("width"), a.f("height"));
    ap(Some(w * h + PI * w * w / 8.0), Some(w + 2.0 * h + 0.5 * PI * w))
}
fn arch_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("width"), a.f("height"));
    vec![out("crown", [0.0, h + 0.5 * w], [0.0, 1.0], On::Boundary), out("base", O, [0.0, -1.0], On::Boundary), at("springing_left", [-0.5 * w, h], On::Vertex), at("springing_right", [0.5 * w, h], On::Vertex)]
}

const P_POINTED: &[Param] = &[len("width", "the span", 1.0), len("height", "the legs' height up to the springing line", 1.0), ratio("radius", "each arc's radius as a share of the span (1: the equilateral arch; 0.5: round)", 1.0, 0.5, 3.0)];
fn pointed_parts(a: &Args) -> (f64, f64, f64, f64) {
    let (w, h) = (a.f("width"), a.f("height"));
    let r = a.f("radius") * w;
    (w, h, r, acos((r - 0.5 * w) / r))
}
fn pointed_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, r, phi) = pointed_parts(a);
    let mut s = legs(w, h);
    s.push(arc([0.5 * w - r, h], r, 0.0, phi));
    s.push(arc([-0.5 * w + r, h], r, PI - phi, PI));
    s.push(line([-0.5 * w, h], [-0.5 * w, 0.0]));
    Ok(ring(s))
}
fn pointed_closed(a: &Args) -> Measures {
    let (w, h, r, phi) = pointed_parts(a);
    let ya = r * sin(phi);
    ap(Some(w * h + 2.0 * (0.5 * r * r * phi - 0.5 * (r - 0.5 * w) * ya)), Some(w + 2.0 * h + 2.0 * r * phi))
}
fn pointed_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, r, phi) = pointed_parts(a);
    vec![out("apex", [0.0, h + r * sin(phi)], [0.0, 1.0], On::Vertex), out("base", O, [0.0, -1.0], On::Boundary), at("springing_left", [-0.5 * w, h], On::Vertex), at("springing_right", [0.5 * w, h], On::Vertex)]
}

const P_HORSE: &[Param] = &[len("width", "the span between the legs", 1.0), len("height", "the legs' height", 1.0), deg("bulge", "how far below the arc's centre it meets the legs (0: a round arch)", 30.0, 0.0, 60.0)];
fn horse_parts(a: &Args) -> (f64, f64, f64, f64) {
    let (w, h, d) = (a.f("width"), a.f("height"), rad(a.f("bulge")));
    (w, h, 0.5 * w / cos(d), d)
}
fn horse_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, r, d) = horse_parts(a);
    let mut s = legs(w, h);
    s.push(arc([0.0, h + r * sin(d)], r, -d, PI + d));
    s.push(line([-0.5 * w, h], [-0.5 * w, 0.0]));
    Ok(ring(s))
}
fn horse_closed(a: &Args) -> Measures {
    let (w, h, r, d) = horse_parts(a);
    let below = r * r * acos(sin(d)) - r * sin(d) * r * cos(d);
    ap(Some(w * h + PI * r * r - below), Some(w + 2.0 * h + r * (PI + 2.0 * d)))
}
fn horse_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, r, d) = horse_parts(a);
    vec![out("crown", [0.0, h + r * sin(d) + r], [0.0, 1.0], On::Boundary), out("base", O, [0.0, -1.0], On::Boundary), at("springing_left", [-0.5 * w, h], On::Vertex), at("springing_right", [0.5 * w, h], On::Vertex)]
}

// The ogee: each side a convex arc then a concave one of the same radius, tangent at the turn,
// meeting at the apex with vertical tangents.
const P_OGEE: &[Param] = &[len("width", "the span", 1.0), len("height", "the legs' height", 1.0), ratio("radius", "the arcs' radius as a share of the span (0.25 the least)", 0.4, 0.25, 2.0)];
fn ogee_parts(a: &Args) -> (f64, f64, f64, f64) {
    let (w, h) = (a.f("width"), a.f("height"));
    let r = a.f("radius") * w;
    (w, h, r, acos(1.0 - w / (4.0 * r)))
}
fn ogee_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, r, th) = ogee_parts(a);
    let (st, ct) = sin_cos(th);
    let c1 = [0.5 * w - r, h];
    let c2 = [c1[0] + 2.0 * r * ct, h + 2.0 * r * st];
    let mut s = legs(w, h);
    s.push(arc(c1, r, 0.0, th));
    s.push(arc(c2, r, th + PI, PI));
    s.push(arc([-c2[0], c2[1]], r, 0.0, -th));
    s.push(arc([-c1[0], c1[1]], r, PI - th, PI));
    s.push(line([-0.5 * w, h], [-0.5 * w, 0.0]));
    Ok(ring(s))
}
fn ogee_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h, r, th) = ogee_parts(a);
    vec![out("apex", [0.0, h + 2.0 * r * sin(th)], [0.0, 1.0], On::Vertex), out("base", O, [0.0, -1.0], On::Boundary), at("springing_left", [-0.5 * w, h], On::Vertex), at("springing_right", [0.5 * w, h], On::Vertex)]
}

// ---------------------------------------------------------------- tracery

const P_KEY: &[Param] = &[len("radius", "the round head's radius", 0.25), len("slot", "the slot's width", 0.2), len("length", "from the head's centre down to the slot's bottom", 0.6)];
fn key_parts(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (r, s, l) = (a.f("radius"), a.f("slot"), a.f("length"));
    if s >= 2.0 * r * (1.0 - 1e-6) {
        return Err(refuse("slot_narrower_than_head", "the slot must be narrower than the head", &[("radius", r), ("slot", s)]));
    }
    let yj = (r * r - 0.25 * s * s).sqrt();
    if l <= yj * (1.0 + 1e-6) {
        return Err(refuse("slot_below_head", "the slot must reach below the head", &[("radius", r), ("slot", s), ("length", l)]));
    }
    Ok((r, s, l, yj))
}
fn key_geom(a: &Args) -> Result<Geom, Refusal> {
    let (r, s, l, yj) = key_parts(a)?;
    let x = 0.5 * s;
    let p = atan2(yj, x);
    Ok(ring(vec![line([-x, -l], [x, -l]), line([x, -l], [x, -yj]), arc(O, r, -p, PI + p), line([-x, -yj], [-x, -l])]))
}
fn key_closed(a: &Args) -> Measures {
    let Ok((r, s, l, yj)) = key_parts(a) else { return Measures::default() };
    // The disc, less the circular segment below the chord at -yj, plus the slot below it.
    let half = acos(yj / r);
    let seg = r * r * half - yj * 0.5 * s;
    ap(Some(PI * r * r - seg + s * (l - yj)), None)
}
fn key_anchors(a: &Args) -> Vec<Anchor> {
    let r = a.f("radius");
    vec![out("top", [0.0, r], [0.0, 1.0], On::Boundary), out("bottom", [0.0, -a.f("length")], [0.0, -1.0], On::Boundary), at("centre", O, On::Inside)]
}

const P_REUL: &[Param] = &[count("sides", "an odd number of arcs", 3.0, 3.0, 15.0), len("width", "its constant width", 1.0)];
fn reul_parts(a: &Args) -> Result<(usize, f64, f64), Refusal> {
    let (n, w) = (a.n("sides"), a.f("width"));
    if n % 2 == 0 {
        return Err(refuse("odd_sides", "a Reuleaux polygon has an odd number of sides", &[("sides", n as f64)]));
    }
    Ok((n, w, 0.5 * w / cos(PI / (2.0 * n as f64))))
}
fn reul_geom(a: &Args) -> Result<Geom, Refusal> {
    let (n, w, r) = reul_parts(a)?;
    let v: Vec<Pt> = (0..n).map(|i| polar(O, r, 0.5 * PI + TAU * i as f64 / n as f64)).collect();
    Ok(ring(
        (0..n)
            .map(|j| {
                let c = v[(j + n.div_ceil(2)) % n];
                let a0 = atan2(v[j][1] - c[1], v[j][0] - c[0]);
                arc_ends(c, w, a0, PI / n as f64, v[j], v[(j + 1) % n])
            })
            .collect(),
    ))
}
fn reul_closed(a: &Args) -> Measures {
    let Ok((n, w, r)) = reul_parts(a) else { return Measures::default() };
    let nf = n as f64;
    let polygon = 0.5 * nf * r * r * sin(TAU / nf);
    let seg = 0.5 * w * w * (PI / nf - sin(PI / nf));
    // Barbier: every figure of constant width w has perimeter π w.
    Measures { centroid: Some([0.0; 3]), ..ap(Some(polygon + nf * seg), Some(PI * w)) }
}
fn reul_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((n, _, r)) = reul_parts(a) else { return Vec::new() };
    let mut v = crate::anchor::vertices("vertex", &(0..n).map(|i| polar(O, r, 0.5 * PI + TAU * i as f64 / n as f64)).collect::<Vec<_>>());
    v.push(at("centre", O, On::Inside));
    v
}

// n circles of radius `lobe`·R, centred on a circle of radius R: the outline of their union
// with the polygon of their centres (the middle is filled).
const P_FOIL: &[Param] = &[count("lobes", "the number of lobes", 4.0, 3.0, 12.0), len("radius", "the lobes' centres from the middle", 0.3), ratio("lobe", "each lobe's radius as a share of that, over sin(π / lobes) and at most 1", 0.9, 0.2, 1.0)];
fn foil_parts(a: &Args) -> Result<(usize, f64, f64), Refusal> {
    let (n, r, k) = (a.n("lobes"), a.f("radius"), a.f("lobe"));
    let lo = sin(PI / n as f64);
    if k <= lo * (1.0 + 1e-6) {
        return Err(refuse("lobes_overlap", "neighbouring lobes must overlap: lobe over sin(π / lobes)", &[("lobes", n as f64), ("lobe", k), ("least", lo)]));
    }
    Ok((n, r, k * r))
}
fn foil_geom(a: &Args) -> Result<Geom, Refusal> {
    let (n, r, rho) = foil_parts(a)?;
    let h = PI / n as f64;
    let q = r * cos(h) + (rho * rho - r * r * sin(h) * sin(h)).sqrt();
    Ok(ring(
        (0..n)
            .map(|i| {
                let t = 0.5 * PI + TAU * i as f64 / n as f64;
                let c = polar(O, r, t);
                let (p0, p1) = (polar(O, q, t - h), polar(O, q, t + h));
                let a0 = atan2(p0[1] - c[1], p0[0] - c[0]);
                let mut sw = atan2(p1[1] - c[1], p1[0] - c[0]) - a0;
                while sw <= 0.0 {
                    sw += TAU;
                }
                arc_ends(c, rho, a0, sw, p0, p1)
            })
            .collect(),
    ))
}
fn foil_anchors(a: &Args) -> Vec<Anchor> {
    let Ok((n, r, rho)) = foil_parts(a) else { return Vec::new() };
    let mut v: Vec<Anchor> = (0..n.min(anchor::MAX_NUMBERED))
        .map(|i| {
            let t = 0.5 * PI + TAU * i as f64 / n as f64;
            out(&format!("lobe_{i}"), polar(O, r + rho, t), [cos(t), sin(t)], On::Boundary)
        })
        .collect();
    v.push(at("centre", O, On::Inside));
    v
}

// Petals by r(θ) = R (c + (1 − c) |cos(nθ/2)|): rounded petal tips, sharp valleys at c R.
const P_FLOWER: &[Param] = &[count("petals", "the number of petals", 6.0, 3.0, 24.0), len("radius", "to a petal's tip", 0.5), ratio("inner", "the valleys' radius as a share of the tips'", 0.4, 0.05, 0.95)];
fn flower_geom(a: &Args) -> Result<Geom, Refusal> {
    let (n, r, c) = (a.n("petals"), a.f("radius"), a.f("inner"));
    let nf = n as f64;
    let f: PtFn = Rc::new(move |t: f64| {
        let rr = r * (c + (1.0 - c) * cos(0.5 * nf * t).abs());
        polar(O, rr, t + 0.5 * PI)
    });
    Ok(ring((0..n).map(|j| func_rc(&f, (2.0 * j as f64 - 1.0) * PI / nf, (2.0 * j as f64 + 1.0) * PI / nf, 1.0 / nf)).collect()))
}
fn flower_closed(a: &Args) -> Measures {
    let (r, c) = (a.f("radius"), a.f("inner"));
    let e = 1.0 - c;
    // ½∫(c + e|cos(nθ/2)|)² dθ: ∫|cos| = 4 and ∫cos² = π over a full turn, for any whole n.
    Measures { centroid: Some([0.0; 3]), ..ap(Some(r * r * (PI * c * c + 4.0 * c * e + 0.5 * PI * e * e)), None) }
}
fn flower_anchors(a: &Args) -> Vec<Anchor> {
    let (n, r) = (a.n("petals"), a.f("radius"));
    let mut v: Vec<Anchor> = (0..n.min(anchor::MAX_NUMBERED))
        .map(|i| {
            let t = 0.5 * PI + TAU * i as f64 / n as f64;
            out(&format!("petal_{i}"), polar(O, r, t), [cos(t), sin(t)], On::Boundary)
        })
        .collect();
    v.push(at("centre", O, On::Inside));
    v
}

// ---------------------------------------------------------------- roulettes and polar figures

// r = b (1 + k cos θ) about the pole. Past k = 1 the curve loops: the outline is the outer
// loop alone (the part where r ≥ 0), which has a corner at the pole.
const P_LIMACON: &[Param] = &[len("radius", "b in r = b (1 + k cos θ)", 0.3), ratio("ratio", "k (under 0.5 convex; to 1 dimpled; 1 the cardioid; over 1 the outer loop alone)", 0.6, 0.0, 4.0)];
fn limacon_geom(a: &Args) -> Result<Geom, Refusal> {
    let (b, k) = (a.f("radius"), a.f("ratio"));
    let f: PtFn = Rc::new(move |t: f64| polar(O, b * (1.0 + k * cos(t)), t));
    if k <= 1.0 {
        Ok(ring(vec![func_rc(&f, -PI, 0.0, 0.5), func_rc(&f, 0.0, PI, 0.5)]))
    } else {
        let t0 = acos(-1.0 / k);
        Ok(ring(vec![func_rc(&f, -t0, t0, t0 / PI)]))
    }
}
fn limacon_closed(a: &Args) -> Measures {
    let (b, k) = (a.f("radius"), a.f("ratio"));
    let area = if k <= 1.0 {
        PI * b * b * (1.0 + 0.5 * k * k)
    } else {
        let t = acos(-1.0 / k);
        b * b * (t + 2.0 * k * sin(t) + k * k * (0.5 * t + 0.25 * sin(2.0 * t)))
    };
    let per = if k == 1.0 { Some(8.0 * b) } else { None };
    ap(Some(area), per)
}
fn limacon_anchors(a: &Args) -> Vec<Anchor> {
    let (b, k) = (a.f("radius"), a.f("ratio"));
    vec![out("far", [b * (1.0 + k), 0.0], [1.0, 0.0], On::Boundary)]
}

const P_EPI: &[Param] = &[count("cusps", "the number of cusps (the fixed circle is this many times the rolling one)", 3.0, 1.0, 24.0), len("radius", "the rolling circle's radius", 0.1)];
fn epi_geom(a: &Args) -> Result<Geom, Refusal> {
    let (k, r) = (a.f("cusps"), a.f("radius"));
    let big = k * r;
    let f: PtFn = Rc::new(move |t: f64| [(big + r) * cos(t) - r * cos((k + 1.0) * t), (big + r) * sin(t) - r * sin((k + 1.0) * t)]);
    let n = a.n("cusps");
    Ok(ring((0..n).map(|j| func_rc(&f, TAU * j as f64 / k, TAU * (j + 1) as f64 / k, 1.0 / k)).collect()))
}
fn epi_closed(a: &Args) -> Measures {
    let (k, r) = (a.f("cusps"), a.f("radius"));
    // From two cusps on the figure has a turn of symmetry, so its centroid is the centre; with
    // one (a cardioid) it has not, and none is given.
    let centroid = if k >= 2.0 { Some([0.0; 3]) } else { None };
    Measures { centroid, ..ap(Some(PI * r * r * (k + 1.0) * (k + 2.0)), Some(8.0 * r * (k + 1.0))) }
}
fn epi_anchors(a: &Args) -> Vec<Anchor> {
    let (k, r) = (a.f("cusps"), a.f("radius"));
    let n = a.n("cusps");
    (0..n.min(anchor::MAX_NUMBERED)).map(|j| at(&format!("cusp_{j}"), polar(O, k * r, TAU * j as f64 / k), On::Vertex)).collect()
}

const P_HYPO: &[Param] = &[count("cusps", "the number of cusps (the fixed circle is this many times the rolling one)", 4.0, 3.0, 24.0), len("radius", "the rolling circle's radius", 0.125)];
fn hypo_geom(a: &Args) -> Result<Geom, Refusal> {
    let (k, r) = (a.f("cusps"), a.f("radius"));
    let big = k * r;
    let f: PtFn = Rc::new(move |t: f64| [(big - r) * cos(t) + r * cos((k - 1.0) * t), (big - r) * sin(t) - r * sin((k - 1.0) * t)]);
    let n = a.n("cusps");
    Ok(ring((0..n).map(|j| func_rc(&f, TAU * j as f64 / k, TAU * (j + 1) as f64 / k, 1.0 / k)).collect()))
}
fn hypo_closed(a: &Args) -> Measures {
    let (k, r) = (a.f("cusps"), a.f("radius"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(PI * r * r * (k - 1.0) * (k - 2.0)), Some(8.0 * r * (k - 1.0))) }
}

// Gielis's superformula, r(φ) = scale (|cos(mφ/4)/a|^n2 + |sin(mφ/4)/b|^n3)^(-1/n1).
const P_SUPERF: &[Param] = &[
    count("m", "the symmetry: how many times the pattern repeats in a turn", 6.0, 0.0, 40.0),
    ratio("n1", "the overall exponent", 1.0, 0.5, 20.0),
    ratio("n2", "the cosine term's exponent", 7.0, 0.5, 20.0),
    ratio("n3", "the sine term's exponent", 8.0, 0.5, 20.0),
    ratio("a", "the cosine term's divisor", 1.0, 0.25, 4.0),
    ratio("b", "the sine term's divisor", 1.0, 0.25, 4.0),
    len("scale", "metres per unit of r", 0.5),
];
fn superf_geom(a: &Args) -> Result<Geom, Refusal> {
    let (m, n1, n2, n3, aa, bb, s) = (a.f("m"), a.f("n1"), a.f("n2"), a.f("n3"), a.f("a"), a.f("b"), a.f("scale"));
    if a.n("m") % 2 == 1 && (aa != bb || n2 != n3) {
        return Err(refuse("closes", "with an odd m the curve closes only when a = b and n2 = n3", &[("m", m), ("a", aa), ("b", bb), ("n2", n2), ("n3", n3)]));
    }
    // Sound bounds of r: with c² + s² = 1 one of |c|, |s| is at least 1/√2.
    let half = 0.5f64.sqrt();
    let s_hi = powf(1.0 / aa, n2) + powf(1.0 / bb, n3);
    let s_lo = powf(half / aa, n2).min(powf(half / bb, n3));
    let (r_lo, r_hi) = (s * powf(s_hi, -1.0 / n1), s * powf(s_lo, -1.0 / n1));
    if !(r_lo.is_finite() && r_hi.is_finite() && r_lo >= 1e-6 && r_hi <= 1e5 && r_hi / r_lo <= 1e6) {
        return Err(refuse("radius_in_range", "these exponents and divisors make radii from under a micrometre, over 100 km, or a million times apart", &[("least_radius", r_lo), ("greatest_radius", r_hi)]));
    }
    let r = move |c: f64, sn: f64| s * powf(powf(c / aa, n2) + powf(sn / bb, n3), -1.0 / n1);
    if a.n("m") == 0 {
        let r0 = r(1.0, 0.0);
        return Ok(ring(vec![crate::geom::arc(O, r0, 0.0, TAU)]));
    }
    // A piece for each quarter turn of mφ/4, parametrised by u = mφ/4 − jπ/2 from 0 to π/2, so
    // that |cos| and |sin| are exactly 0 and 1 at the pieces' ends (an exponent under 1 would
    // turn a rounding of 1e-16 there into a step of 1e-8).
    let pieces = a.n("m");
    let step = TAU / m;
    Ok(ring(
        (0..pieces)
            .map(|j| {
                let base = step * j as f64;
                func(
                    move |u: f64| {
                        let (su, cu) = (sin(u), sin(0.5 * PI - u));
                        let (c, sn) = if j % 2 == 0 { (cu, su) } else { (su, cu) };
                        polar(O, r(c, sn), base + 4.0 * u / m)
                    },
                    0.0,
                    0.5 * PI,
                    1.0 / m,
                )
            })
            .collect(),
    ))
}

// ---------------------------------------------------------------- the table

const KIN_CIRCLE: &[&str] = &["cylinder", "cone", "sphere", "icosphere", "cube_sphere", "spherical_segment", "hourglass", "hyperboloid"];

pub(crate) const FORMS: &[Form] = &[
    plane!("circle", Circle, "a circle", "the centre", P_CIRCLE, curved: true, kin: KIN_CIRCLE, circle_geom, circle_closed, circle_anchors),
    plane!("semicircle", Circle, "a half disc, flat side down", "the middle of the diameter", P_CIRCLE, curved: true, kin: &["half_cylinder", "hemisphere"], semicircle_geom, semicircle_closed, semicircle_anchors),
    plane!("sector", Circle, "a circular sector (a pie slice), from +x counter-clockwise", "the circle's centre (the apex)", P_SECTOR, curved: true, kin: &["cylinder_sector", "spherical_wedge"], sector_geom, sector_closed, sector_anchors),
    plane!("circular_segment", Circle, "the part of a disc cut off by a chord", "the circle's centre", P_SECTOR, curved: true, kin: &["spherical_cap"], segment_geom, segment_closed, segment_anchors),
    plane!("annulus", Circle, "a disc with a round hole (a ring)", "the centre", P_ANNULUS, curved: true, kin: &["tube", "torus", "helix", "torus_knot"], annulus_geom, annulus_closed, annulus_anchors),
    plane!("annular_sector", Circle, "a sector of a ring (a thick arc)", "the circles' centre", P_ASECTOR, curved: true, kin: &["torus_sector", "dome", "bowl"], asector_geom, asector_closed, super::no_anchors),
    plane!("lune", Circle, "a crescent: a disc less an overlapping disc", "the first circle's centre", P_LUNE, curved: true, kin: &[], lune_geom, lune_closed, lune_anchors),
    plane!("lens", Circle, "the common part of two equal discs", "midway between the centres", P_LENS, curved: true, kin: &["biconvex_lens", "spindle"], lens_geom, lens_closed, lens_anchors),
    plane!("stadium", Circle, "a rectangle with half discs on its ends (the plane capsule)", "the centre", P_STADIUM, curved: true, kin: &["capsule"], stadium_geom, stadium_closed, stadium_anchors),
    plane!("rounded_rectangle", Circle, "a rectangle with corners of a true radius", "the centre", P_RRECT, curved: true, kin: &["rounded_box"], rrect_geom, rrect_closed, rrect_anchors),
    plane!("ellipse", Oval, "an ellipse", "the centre", P_ELLIPSE, curved: true, kin: &["elliptic_cylinder", "ellipsoid", "elliptic_torus"], ellipse_geom, ellipse_closed, ellipse_anchors),
    plane!("superellipse", Oval, "a superellipse (Lamé curve)", "the centre", P_SUPER, curved: true, kin: &["superellipsoid"], super_geom, super_closed, super_anchors),
    plane!("moss_egg", Oval, "Moss's egg: four arcs of circles (a half circle, two of twice its radius, a small cap)", "the lower half circle's centre", P_MOSS, curved: true, kin: &["egg_of_revolution"], moss_geom, moss_closed, moss_anchors),
    plane!("oval", Oval, "the four-centre oval of the drafting construction (arcs of two radii)", "the centre", P_OVAL, curved: true, kin: &[], oval_geom, oval_closed, oval_anchors),
    plane!("ovoid", Oval, "an egg of two half ellipses on one widest line", "the middle of the widest line", P_OVOID, curved: true, kin: &["egg_of_revolution"], ovoid_geom, ovoid_closed, ovoid_anchors),
    plane!("teardrop", Oval, "a disc drawn out to a point by two tangent lines", "the round end's centre", P_DROP, curved: true, kin: &["teardrop_of_revolution"], drop_geom, drop_closed, drop_anchors),
    plane!("heart", Oval, "the classic heart curve (x = 16 sin³t, y = 13 cos t − 5 cos 2t − 2 cos 3t − cos 4t)", "the curve's own origin, between the lobes", P_HEART, curved: true, kin: &[], heart_geom, heart_closed, heart_anchors),
    plane!("round_arch", Arch, "a round arch's opening: legs and a half circle", "the middle of the base", P_ARCH, curved: true, kin: &[], arch_geom, arch_closed, arch_anchors),
    plane!("pointed_arch", Arch, "a pointed (gothic) arch's opening: two arcs meeting at the apex", "the middle of the base", P_POINTED, curved: true, kin: &[], pointed_geom, pointed_closed, pointed_anchors),
    plane!("horseshoe_arch", Arch, "a horseshoe arch's opening: an arc past the half circle, wider than its legs", "the middle of the base", P_HORSE, curved: true, kin: &[], horse_geom, horse_closed, horse_anchors),
    plane!("ogee_arch", Arch, "an ogee arch's opening: convex then concave arcs meeting at a point", "the middle of the base", P_OGEE, curved: true, kin: &[], ogee_geom, none, ogee_anchors),
    plane!("keyhole", Tracery, "a keyhole: a round head over a straight slot", "the head's centre", P_KEY, curved: true, kin: &[], key_geom, key_closed, key_anchors),
    plane!("reuleaux_polygon", Tracery, "a Reuleaux polygon: a figure of constant width from arcs about opposite vertices", "the centre", P_REUL, curved: true, kin: &[], reul_geom, reul_closed, reul_anchors),
    plane!("foil", Tracery, "an n-foil: overlapping round lobes about a filled middle", "the centre", P_FOIL, curved: true, kin: &[], foil_geom, none, foil_anchors),
    plane!("flower", Tracery, "a flower of rounded petals with sharp valleys (r = R (c + (1 − c)|cos(nθ/2)|))", "the centre", P_FLOWER, curved: true, kin: &[], flower_geom, flower_closed, flower_anchors),
    plane!("limacon", Roulette, "the limaçon of Pascal, r = b (1 + k cos θ); past k = 1 its outer loop alone", "the pole", P_LIMACON, curved: true, kin: &[], limacon_geom, limacon_closed, limacon_anchors),
    plane!("epicycloid", Roulette, "the curve a point of a circle traces rolling round the outside of another", "the fixed circle's centre", P_EPI, curved: true, kin: &[], epi_geom, epi_closed, epi_anchors),
    plane!("hypocycloid", Roulette, "the curve a point of a circle traces rolling round the inside of another", "the fixed circle's centre", P_HYPO, curved: true, kin: &[], hypo_geom, hypo_closed, epi_anchors),
    plane!("superformula", Roulette, "Gielis's superformula, a polar curve (closed and simple wherever it is accepted)", "the pole", P_SUPERF, curved: true, kin: &[], superf_geom, none, super::no_anchors),
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("disc", Plane, "circle", "a disc", &[]),
    alias!("disk", Plane, "circle", "a disc", &[]),
    alias!("half_circle", Plane, "semicircle", "a half disc", &[]),
    alias!("circular_sector", Plane, "sector", "a circular sector", &[]),
    alias!("pie", Plane, "sector", "a pie slice", &[]),
    alias!("quadrant", Plane, "sector", "a quarter disc", &[Set::Fixed("angle", 90.0)]),
    alias!("ring", Plane, "annulus", "a ring", &[]),
    alias!("crescent", Plane, "lune", "a crescent", &[]),
    alias!("moon", Plane, "lune", "a crescent moon", &[]),
    alias!("vesica_piscis", Plane, "lens", "the lens of two circles each through the other's centre", &[Set::Fixed("separation", 1.0)]),
    alias!("pill", Plane, "stadium", "a stadium", &[]),
    alias!("obround", Plane, "stadium", "a stadium", &[]),
    alias!("rounded_rect", Plane, "rounded_rectangle", "a rounded rectangle", &[]),
    alias!("squircle", Plane, "superellipse", "a superellipse of exponent 4 and equal axes", &[Set::Tie { name: "radius", about: "centre to each side's middle", targets: &["rx", "ry"] }, Set::Fixed("exponent", 4.0)]),
    alias!("egg", Plane, "moss_egg", "an egg (Moss's construction)", &[]),
    alias!("drop", Plane, "teardrop", "a drop", &[]),
    alias!("raindrop", Plane, "teardrop", "a drop", &[]),
    alias!("arch", Plane, "round_arch", "a round arch's opening", &[]),
    alias!("gothic_arch", Plane, "pointed_arch", "a pointed arch's opening", &[]),
    alias!("equilateral_arch", Plane, "pointed_arch", "a pointed arch whose arcs' radius is the span", &[Set::Fixed("radius", 1.0)]),
    alias!("reuleaux_triangle", Plane, "reuleaux_polygon", "the Reuleaux triangle", &[Set::Fixed("sides", 3.0)]),
    alias!("trefoil", Plane, "foil", "three lobes", &[Set::Fixed("lobes", 3.0)]),
    alias!("quatrefoil", Plane, "foil", "four lobes", &[Set::Fixed("lobes", 4.0)]),
    alias!("cinquefoil", Plane, "foil", "five lobes", &[Set::Fixed("lobes", 5.0)]),
    alias!("rose", Plane, "flower", "a flower of petals", &[]),
    alias!("cardioid", Plane, "limacon", "the cardioid, the limaçon of k = 1", &[Set::Fixed("ratio", 1.0)]),
    alias!("nephroid", Plane, "epicycloid", "the epicycloid of two cusps", &[Set::Fixed("cusps", 2.0)]),
    alias!("deltoid", Plane, "hypocycloid", "the hypocycloid of three cusps", &[Set::Fixed("cusps", 3.0)]),
    alias!("astroid", Plane, "hypocycloid", "the hypocycloid of four cusps", &[Set::Fixed("cusps", 4.0)]),
    alias!("gielis", Plane, "superformula", "Gielis's superformula", &[]),
];
