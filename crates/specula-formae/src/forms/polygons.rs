//! Straight-sided plane figures (and the heater shield, which is a symbol): triangles,
//! quadrilaterals, regular polygons, stars, symbols, letter sections, frames, the gear and the
//! patterned bands.

use super::{ap, no_anchors, none};
use crate::anchor::{self, at, out, vertices, Anchor, On};
use crate::error::{refuse, Refusal};
use crate::geom::{arc, line, poly, ring, Geom};
use crate::maths::*;
use crate::spec::Args;
use crate::table::{count, deg, len, ratio, Alias, Form, Param, Set};
use crate::Measures;

const POLY_ORIGIN: &str = "the centre of its bounds";
const KIN_TRIANGLE: &[&str] = &["triangular_prism", "pyramid", "tetrahedron"];
/// The solids with equilateral faces name the triangle by its sides (equilateral_triangle is its alias).
const KIN_TRI_SSS: &[&str] = &[
    "triangular_prism", "pyramid", "tetrahedron", "tetrahedron_in_sphere", "octahedron", "octahedron_in_sphere", "icosahedron", "icosahedron_in_sphere", "antiprism", "stella_octangula",
    "disdyakis_dodecahedron", "disdyakis_triacontahedron", "truncated_tetrahedron", "cuboctahedron", "truncated_cube", "rhombicuboctahedron", "snub_cube", "icosidodecahedron",
    "truncated_dodecahedron", "rhombicosidodecahedron", "snub_dodecahedron",
];
/// The solids with isosceles faces, and the wedge (a right triangle raised).
const KIN_TRI_BH: &[&str] = &[
    "triangular_prism", "pyramid", "tetrahedron", "wedge", "bipyramid", "small_stellated_dodecahedron", "triakis_tetrahedron", "triakis_octahedron", "tetrakis_hexahedron",
    "triakis_icosahedron", "pentakis_dodecahedron",
];
const KIN_RECT: &[&str] = &["box", "wedge", "cube_in_sphere", "cuboctahedron", "truncated_octahedron", "rhombicuboctahedron", "truncated_cuboctahedron", "snub_cube", "rhombicosidodecahedron", "truncated_icosidodecahedron", "barrel", "mobius_band", "steinmetz_solid", "chamfered_box"];
const KIN_POLY: &[&str] = &[
    "prism", "antiprism", "pyramid", "bipyramid", "pyramid_frustum", "dodecahedron", "dodecahedron_in_sphere", "truncated_tetrahedron", "truncated_cube", "truncated_octahedron",
    "truncated_cuboctahedron", "icosidodecahedron", "truncated_dodecahedron", "truncated_icosahedron", "rhombicosidodecahedron", "truncated_icosidodecahedron", "snub_dodecahedron",
    "pentagonal_icositetrahedron", "pentagonal_hexecontahedron", "crystal", "gem",
];
const KIN_PRISM: &[&str] = &["prism"];

fn pts_ring(p: Result<Vec<Pt>, Refusal>) -> Result<Geom, Refusal> {
    p.map(|p| ring(poly(&p)))
}

fn centred(mut p: Vec<Pt>) -> Vec<Pt> {
    let n = p.len() as f64;
    let c = p.iter().fold([0.0, 0.0], |s, q| add(s, *q));
    let c = scale(c, 1.0 / n);
    for q in &mut p {
        *q = sub(*q, c);
    }
    p
}

// ---------------------------------------------------------------- triangles

const P_SSS: &[Param] = &[len("a", "side a (opposite vertex a)", 1.0), len("b", "side b (opposite vertex b)", 1.0), len("c", "side c, the base (opposite vertex c)", 1.0)];

/// A triangle by its three sides: side c along the base from vertex a to vertex b, vertex c
/// above; then moved so that the centroid is at the origin.
fn sss(a: f64, b: f64, c: f64) -> Result<Vec<Pt>, Refusal> {
    let big = a.max(b).max(c);
    if !(a + b > c && a + c > b && b + c > a) {
        return Err(refuse("triangle_inequality", "each side must be shorter than the other two together", &[("a", a), ("b", b), ("c", c)]));
    }
    let x = (b * b + c * c - a * a) / (2.0 * c);
    let y2 = b * b - x * x;
    if y2 <= (big * 1e-6) * (big * 1e-6) {
        return Err(refuse("triangle_inequality", "the triangle is too flat: its height is under a millionth of its longest side", &[("a", a), ("b", b), ("c", c)]));
    }
    Ok(centred(vec![[0.0, 0.0], [c, 0.0], [x, y2.sqrt()]]))
}

fn tri_anchors(p: &[Pt]) -> Vec<Anchor> {
    let (a, b, c) = (p[0], p[1], p[2]);
    let mid = scale(add(a, b), 0.5);
    vec![at("vertex_a", a, On::Vertex), at("vertex_b", b, On::Vertex), at("vertex_c", c, On::Vertex), out("base", mid, [0.0, -1.0], On::Boundary), at("centroid", [0.0, 0.0], On::Inside)]
}

fn sss_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    sss(a.f("a"), a.f("b"), a.f("c"))
}
fn sss_closed(a: &Args) -> Measures {
    let (x, y, z) = (a.f("a"), a.f("b"), a.f("c"));
    let s = 0.5 * (x + y + z);
    Measures { centroid: Some([0.0; 3]), ..ap(Some((s * (s - x) * (s - y) * (s - z)).max(0.0).sqrt()), Some(x + y + z)) }
}

const P_SAS: &[Param] = &[len("a", "side a", 1.0), len("b", "side b", 1.0), deg("angle", "the angle between sides a and b, at vertex c", 60.0, 1.0, 179.0)];

fn sas_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (x, y, g) = (a.f("a"), a.f("b"), rad(a.f("angle")));
    let c = (x * x + y * y - 2.0 * x * y * cos(g)).max(0.0).sqrt();
    sss(x, y, c)
}
fn sas_closed(a: &Args) -> Measures {
    let (x, y, g) = (a.f("a"), a.f("b"), rad(a.f("angle")));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(0.5 * x * y * sin(g)), None) }
}

const P_TRI: &[Param] = &[len("base", "the base's length", 1.0), len("height", "the apex's height over the base", 1.0), ratio("apex", "where the apex is over the base, from its middle, as a share of the base (-0.5: over the left end)", 0.0, -2.0, 2.0)];

fn tri_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (b, h, o) = (a.f("base"), a.f("height"), a.f("apex"));
    Ok(centred(vec![[-0.5 * b, 0.0], [0.5 * b, 0.0], [o * b, h]]))
}
fn tri_closed(a: &Args) -> Measures {
    Measures { centroid: Some([0.0; 3]), ..ap(Some(0.5 * a.f("base") * a.f("height")), None) }
}

// ---------------------------------------------------------------- quadrilaterals

const P_RECT: &[Param] = &[len("width", "along x", 1.0), len("height", "along y", 1.0)];

fn rect_pts(w: f64, h: f64) -> Vec<Pt> {
    vec![[-0.5 * w, -0.5 * h], [0.5 * w, -0.5 * h], [0.5 * w, 0.5 * h], [-0.5 * w, 0.5 * h]]
}
fn rect_closed(a: &Args) -> Measures {
    let (w, h) = (a.f("width"), a.f("height"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(w * h), Some(2.0 * (w + h))) }
}
fn rect_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (0.5 * a.f("width"), 0.5 * a.f("height"));
    let mut v = vec![
        at("corner_bottom_left", [-w, -h], On::Vertex),
        at("corner_bottom_right", [w, -h], On::Vertex),
        at("corner_top_right", [w, h], On::Vertex),
        at("corner_top_left", [-w, h], On::Vertex),
    ];
    v.extend(edge_mids(w, h));
    v.push(at("centre", [0.0, 0.0], On::Inside));
    v
}
fn frame_anchors(a: &Args) -> Vec<Anchor> {
    rect_anchors(a).into_iter().filter(|x| x.name != "centre").collect()
}
fn pring_anchors(a: &Args) -> Vec<Anchor> {
    poly_anchors(a).into_iter().filter(|x| x.name != "centre").collect()
}
fn edge_mids(w: f64, h: f64) -> Vec<Anchor> {
    vec![out("bottom", [0.0, -h], [0.0, -1.0], On::Boundary), out("right", [w, 0.0], [1.0, 0.0], On::Boundary), out("top", [0.0, h], [0.0, 1.0], On::Boundary), out("left", [-w, 0.0], [-1.0, 0.0], On::Boundary)]
}

const P_PARA: &[Param] = &[len("base", "the bottom side's length", 1.0), len("height", "between the parallel sides", 1.0), deg("angle", "the interior angle at the bottom left", 60.0, 10.0, 170.0)];

fn para_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (b, h, t) = (a.f("base"), a.f("height"), rad(a.f("angle")));
    let (s, c) = sin_cos(t);
    let d = h * c / s;
    Ok(vec![[-0.5 * b - 0.5 * d, -0.5 * h], [0.5 * b - 0.5 * d, -0.5 * h], [0.5 * b + 0.5 * d, 0.5 * h], [-0.5 * b + 0.5 * d, 0.5 * h]])
}
fn para_closed(a: &Args) -> Measures {
    let (b, h, t) = (a.f("base"), a.f("height"), rad(a.f("angle")));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(b * h), Some(2.0 * b + 2.0 * h / sin(t))) }
}

const P_RHOMBUS: &[Param] = &[len("width", "the diagonal along x", 1.0), len("height", "the diagonal along y", 0.6)];
fn rhombus_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h) = (0.5 * a.f("width"), 0.5 * a.f("height"));
    Ok(vec![[0.0, -h], [w, 0.0], [0.0, h], [-w, 0.0]])
}
fn rhombus_closed(a: &Args) -> Measures {
    let (w, h) = (a.f("width"), a.f("height"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(0.5 * w * h), Some(2.0 * (w * w + h * h).sqrt())) }
}
fn diag_anchors(a: &Args) -> Vec<Anchor> {
    vertices("vertex", &kite_or_dart(a))
}

const P_KITE: &[Param] = &[len("width", "the cross diagonal", 0.8), len("top", "from the cross diagonal up to the top vertex", 0.4), len("bottom", "from the cross diagonal down to the bottom vertex", 0.8)];
const P_DART: &[Param] = &[len("width", "between the wing tips", 0.8), len("length", "from the wing line up to the tip", 1.0), len("notch", "from the wing line up to the reflex vertex", 0.4)];

fn kite_or_dart(a: &Args) -> Vec<Pt> {
    let w = 0.5 * a.f("width");
    if a.has("top") {
        vec![[0.0, -a.f("bottom")], [w, 0.0], [0.0, a.f("top")], [-w, 0.0]]
    } else {
        vec![[0.0, a.f("notch")], [w, 0.0], [0.0, a.f("length")], [-w, 0.0]]
    }
}
fn kite_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    Ok(kite_or_dart(a))
}
fn kite_closed(a: &Args) -> Measures {
    let (w, t, b) = (a.f("width"), a.f("top"), a.f("bottom"));
    Measures { centroid: Some([0.0, (t - b) / 3.0, 0.0]), ..ap(Some(0.5 * w * (t + b)), None) }
}
fn dart_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (l, n) = (a.f("length"), a.f("notch"));
    if n >= l {
        return Err(refuse("notch_below_tip", "the notch must be lower than the tip", &[("length", l), ("notch", n)]));
    }
    Ok(kite_or_dart(a))
}
fn dart_closed(a: &Args) -> Measures {
    ap(Some(0.5 * a.f("width") * (a.f("length") - a.f("notch"))), None)
}

const P_TRAP: &[Param] = &[
    len("bottom", "the bottom side's length", 1.0),
    len("top", "the top side's length", 0.6),
    len("height", "between the parallel sides", 0.6),
    ratio("lean", "where the top side's middle is: 0 over the bottom's middle, -1 with the left ends one above the other, 1 the right ends", 0.0, -4.0, 4.0),
];
fn trap_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (b, t, h, l) = (a.f("bottom"), a.f("top"), a.f("height"), a.f("lean"));
    let c = l * 0.5 * (b - t);
    Ok(vec![[-0.5 * b, -0.5 * h], [0.5 * b, -0.5 * h], [c + 0.5 * t, 0.5 * h], [c - 0.5 * t, 0.5 * h]])
}
fn trap_closed(a: &Args) -> Measures {
    let (b, t, h) = (a.f("bottom"), a.f("top"), a.f("height"));
    ap(Some(0.5 * (b + t) * h), None)
}

// ---------------------------------------------------------------- regular polygons and stars

const P_POLY: &[Param] = &[count("sides", "the number of sides", 6.0, 3.0, 256.0), len("radius", "the circumradius: centre to vertex", 0.5)];

pub(crate) fn regular(n: usize, r: f64) -> Vec<Pt> {
    (0..n).map(|i| polar([0.0, 0.0], r, 0.5 * PI + TAU * i as f64 / n as f64)).collect()
}
fn poly_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    Ok(regular(a.n("sides"), a.f("radius")))
}
fn poly_closed(a: &Args) -> Measures {
    let (n, r) = (a.f("sides"), a.f("radius"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(0.5 * n * r * r * sin(TAU / n)), Some(2.0 * n * r * sin(PI / n))) }
}
fn poly_anchors(a: &Args) -> Vec<Anchor> {
    let mut v = vertices("vertex", &regular(a.n("sides"), a.f("radius")));
    v.push(at("centre", [0.0, 0.0], On::Inside));
    v
}

const P_STAR: &[Param] = &[count("points", "the number of points", 5.0, 3.0, 100.0), len("outer", "centre to a point's tip", 0.5), len("inner", "centre to a notch between points", 0.2)];

/// A star of `n` points, tips at `outer`, notches at `inner`, the first tip on +y.
pub(crate) fn star(n: usize, outer: f64, inner: f64) -> Vec<Pt> {
    (0..2 * n).map(|i| polar([0.0, 0.0], if i % 2 == 0 { outer } else { inner }, 0.5 * PI + PI * i as f64 / n as f64)).collect()
}
fn star_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (o, i) = (a.f("outer"), a.f("inner"));
    if i >= o {
        return Err(refuse("inner_inside_outer", "the notches must be nearer the centre than the tips", &[("outer", o), ("inner", i)]));
    }
    Ok(star(a.n("points"), o, i))
}
fn star_closed(a: &Args) -> Measures {
    let (n, o, i) = (a.f("points"), a.f("outer"), a.f("inner"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(n * o * i * sin(PI / n)), None) }
}
fn star_anchors(a: &Args) -> Vec<Anchor> {
    let n = a.n("points");
    let o = a.f("outer");
    (0..n.min(anchor::MAX_NUMBERED))
        .map(|i| {
            let t = 0.5 * PI + TAU * i as f64 / n as f64;
            out(&format!("tip_{i}"), polar([0.0, 0.0], o, t), [cos(t), sin(t)], On::Vertex)
        })
        .collect()
}

const P_STARPOLY: &[Param] = &[count("points", "n: the number of points", 5.0, 5.0, 100.0), count("step", "k: each edge joins every k-th point", 2.0, 2.0, 49.0), len("radius", "centre to a point's tip", 0.5)];

/// The radius of the notches of the star polygon {n/k}: where the edge from tip 0 to tip k
/// crosses the edge from tip 1 to tip 1-k, `R cos(πk/n) / cos(π(k-1)/n)`.
pub(crate) fn star_polygon_inner(n: f64, k: f64, r: f64) -> f64 {
    r * cos(PI * k / n) / cos(PI * (k - 1.0) / n)
}
fn starpoly_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (n, k) = (a.f("points"), a.f("step"));
    if 2.0 * k >= n {
        return Err(refuse("step_under_half", "the step must be under half the points", &[("points", n), ("step", k)]));
    }
    Ok(star(a.n("points"), a.f("radius"), star_polygon_inner(n, k, a.f("radius"))))
}
fn starpoly_anchors(a: &Args) -> Vec<Anchor> {
    let n = a.n("points");
    let r = a.f("radius");
    (0..n.min(anchor::MAX_NUMBERED))
        .map(|i| {
            let t = 0.5 * PI + TAU * i as f64 / n as f64;
            out(&format!("tip_{i}"), polar([0.0, 0.0], r, t), [cos(t), sin(t)], On::Vertex)
        })
        .collect()
}

// ---------------------------------------------------------------- symbols

const P_CROSS: &[Param] = &[len("width", "arm tip to arm tip along x", 1.0), len("height", "arm tip to arm tip along y", 1.0), len("bar", "the arms' thickness", 0.3)];
fn cross(w: f64, h: f64, bar: f64) -> Vec<Pt> {
    let (x, y, t) = (0.5 * w, 0.5 * h, 0.5 * bar);
    vec![[t, -y], [t, -t], [x, -t], [x, t], [t, t], [t, y], [-t, y], [-t, t], [-x, t], [-x, -t], [-t, -t], [-t, -y]]
}
fn cross_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, b) = (a.f("width"), a.f("height"), a.f("bar"));
    if b >= w.min(h) {
        return Err(refuse("bar_thinner_than_arms", "the bar must be thinner than the cross is wide and tall", &[("width", w), ("height", h), ("bar", b)]));
    }
    Ok(cross(w, h, b))
}
fn cross_closed(a: &Args) -> Measures {
    let (w, h, b) = (a.f("width"), a.f("height"), a.f("bar"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(b * (w + h) - b * b), Some(2.0 * (w + h))) }
}
fn cross_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (0.5 * a.f("width"), 0.5 * a.f("height"));
    vec![out("arm_right", [w, 0.0], [1.0, 0.0], On::Boundary), out("arm_top", [0.0, h], [0.0, 1.0], On::Boundary), out("arm_left", [-w, 0.0], [-1.0, 0.0], On::Boundary), out("arm_bottom", [0.0, -h], [0.0, -1.0], On::Boundary), at("centre", [0.0, 0.0], On::Inside)]
}

const P_SALTIRE: &[Param] = &[len("span", "arm tip to arm tip, along each diagonal", 1.0), len("bar", "the arms' thickness", 0.25)];
fn saltire_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (s, b) = (a.f("span"), a.f("bar"));
    if b >= s {
        return Err(refuse("bar_thinner_than_arms", "the bar must be thinner than the span", &[("span", s), ("bar", b)]));
    }
    Ok(cross(s, s, b).into_iter().map(|p| rotate(p, 0.25 * PI)).collect())
}
fn saltire_closed(a: &Args) -> Measures {
    let (s, b) = (a.f("span"), a.f("bar"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(2.0 * s * b - b * b), Some(4.0 * s)) }
}

const P_ARROW: &[Param] = &[len("length", "tail to tip, along +x", 1.0), len("shaft", "the shaft's thickness", 0.2), len("head_length", "the head's length", 0.35), len("head_width", "the head's width at its back", 0.5)];
fn arrow_check(a: &Args, heads: f64) -> Result<(f64, f64, f64, f64), Refusal> {
    let (l, s, hl, hw) = (a.f("length"), a.f("shaft"), a.f("head_length"), a.f("head_width"));
    if s >= hw {
        return Err(refuse("shaft_thinner_than_head", "the shaft must be thinner than the head is wide", &[("shaft", s), ("head_width", hw)]));
    }
    if heads * hl >= l {
        return Err(refuse("head_shorter_than_arrow", "the heads must be shorter than the arrow", &[("length", l), ("head_length", hl)]));
    }
    Ok((l, s, hl, hw))
}
fn arrow_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (l, s, hl, hw) = arrow_check(a, 1.0)?;
    let (x, b) = (0.5 * l, 0.5 * l - hl);
    Ok(vec![[-x, -0.5 * s], [b, -0.5 * s], [b, -0.5 * hw], [x, 0.0], [b, 0.5 * hw], [b, 0.5 * s], [-x, 0.5 * s]])
}
fn arrow_closed(a: &Args) -> Measures {
    let (l, s, hl, hw) = (a.f("length"), a.f("shaft"), a.f("head_length"), a.f("head_width"));
    ap(Some((l - hl) * s + 0.5 * hl * hw), None)
}
fn arrow_anchors(a: &Args) -> Vec<Anchor> {
    let x = 0.5 * a.f("length");
    vec![out("tip", [x, 0.0], [1.0, 0.0], On::Vertex), out("tail", [-x, 0.0], [-1.0, 0.0], On::Boundary)]
}
fn darrow_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (l, s, hl, hw) = arrow_check(a, 2.0)?;
    let (x, b) = (0.5 * l, 0.5 * l - hl);
    Ok(vec![[-x, 0.0], [-b, -0.5 * hw], [-b, -0.5 * s], [b, -0.5 * s], [b, -0.5 * hw], [x, 0.0], [b, 0.5 * hw], [b, 0.5 * s], [-b, 0.5 * s], [-b, 0.5 * hw]])
}
fn darrow_closed(a: &Args) -> Measures {
    let (l, s, hl, hw) = (a.f("length"), a.f("shaft"), a.f("head_length"), a.f("head_width"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some((l - 2.0 * hl) * s + hl * hw), None) }
}
fn darrow_anchors(a: &Args) -> Vec<Anchor> {
    let x = 0.5 * a.f("length");
    vec![out("tip_right", [x, 0.0], [1.0, 0.0], On::Vertex), out("tip_left", [-x, 0.0], [-1.0, 0.0], On::Vertex)]
}

const P_CHEVRON: &[Param] = &[len("width", "end to end", 1.0), len("height", "base of the ends to the outer apex", 0.6), len("thickness", "measured vertically", 0.2)];
fn chevron_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, t) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("thickness"));
    if t >= 2.0 * h {
        return Err(refuse("thinner_than_tall", "the thickness must be under the height", &[("height", 2.0 * h), ("thickness", t)]));
    }
    Ok(vec![[-w, -h], [0.0, h - t], [w, -h], [w, -h + t], [0.0, h], [-w, -h + t]])
}
fn chevron_closed(a: &Args) -> Measures {
    ap(Some(a.f("width") * a.f("thickness")), None)
}
fn chevron_anchors(a: &Args) -> Vec<Anchor> {
    vec![out("apex", [0.0, 0.5 * a.f("height")], [0.0, 1.0], On::Vertex)]
}

// The heater shield: a straight top, straight sides, and two arcs meeting at the point.
const P_SHIELD: &[Param] = &[len("width", "across the top", 1.0), len("height", "top to point", 1.2), ratio("curve", "each lower arc's radius as a share of the width (0.5: a round bottom; 1: the classic heater)", 1.0, 0.5, 4.0)];
fn shield_dims(a: &Args) -> Result<(f64, f64, f64, f64), Refusal> {
    let (w, h, k) = (a.f("width"), a.f("height"), a.f("curve"));
    let r = k * w;
    let d = (w * (r - 0.25 * w)).sqrt();
    if d >= h {
        return Err(refuse("arcs_shorter_than_shield", "the arcs reach lower than the shield is tall: raise the height or lower the curve", &[("height", h), ("arc_drop", d)]));
    }
    Ok((w, h, r, d))
}
fn shield_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, r, d) = shield_dims(a)?;
    let (x, top, ys) = (0.5 * w, 0.5 * h, -0.5 * h + d);
    let cl = [-x + r, ys];
    let cr = [x - r, ys];
    let tl = atan2(-d, x - r) + TAU;
    let trr = atan2(-d, r - x);
    Ok(ring(vec![line([x, top], [-x, top]), line([-x, top], [-x, ys]), arc(cl, r, PI, tl), arc(cr, r, trr, 0.0), line([x, ys], [x, top])]))
}
fn shield_anchors(a: &Args) -> Vec<Anchor> {
    let (w, h) = (a.f("width"), a.f("height"));
    vec![out("point", [0.0, -0.5 * h], [0.0, -1.0], On::Vertex), out("top", [0.0, 0.5 * h], [0.0, 1.0], On::Boundary), at("corner_top_left", [-0.5 * w, 0.5 * h], On::Vertex), at("corner_top_right", [0.5 * w, 0.5 * h], On::Vertex)]
}

// ---------------------------------------------------------------- sections

const P_LSEC: &[Param] = &[len("width", "the foot's length", 0.6), len("height", "the upright's length", 1.0), len("thickness", "of both legs", 0.15)];
fn lsec_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, t) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("thickness"));
    if t >= 2.0 * w.min(h) {
        return Err(refuse("legs_thinner_than_long", "the thickness must be under the width and the height", &[("width", 2.0 * w), ("height", 2.0 * h), ("thickness", t)]));
    }
    Ok(vec![[-w, -h], [w, -h], [w, -h + t], [-w + t, -h + t], [-w + t, h], [-w, h]])
}
fn lsec_closed(a: &Args) -> Measures {
    let (w, h, t) = (a.f("width"), a.f("height"), a.f("thickness"));
    ap(Some(w * t + (h - t) * t), Some(2.0 * (w + h)))
}

const P_TSEC: &[Param] = &[len("width", "the flange's width", 1.0), len("height", "overall", 1.0), len("flange", "the flange's thickness", 0.2), len("web", "the web's thickness", 0.2)];
fn tsec_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, f, b) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("flange"), 0.5 * a.f("web"));
    if b >= w || f >= 2.0 * h {
        return Err(refuse("web_and_flange_fit", "the web must be narrower than the flange, and the flange thinner than the height", &[("width", 2.0 * w), ("height", 2.0 * h), ("flange", f), ("web", 2.0 * b)]));
    }
    Ok(vec![[-b, -h], [b, -h], [b, h - f], [w, h - f], [w, h], [-w, h], [-w, h - f], [-b, h - f]])
}
fn tsec_closed(a: &Args) -> Measures {
    let (w, h, f, b) = (a.f("width"), a.f("height"), a.f("flange"), a.f("web"));
    ap(Some(w * f + b * (h - f)), None)
}

const P_USEC: &[Param] = &[len("width", "overall", 1.0), len("height", "overall", 0.8), len("thickness", "of the base and the sides", 0.15)];
fn usec_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, t) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("thickness"));
    if t >= w || t >= 2.0 * h {
        return Err(refuse("walls_fit", "two walls must fit in the width, and the base under the height", &[("width", 2.0 * w), ("height", 2.0 * h), ("thickness", t)]));
    }
    Ok(vec![[-w, -h], [w, -h], [w, h], [w - t, h], [w - t, -h + t], [-w + t, -h + t], [-w + t, h], [-w, h]])
}
fn usec_closed(a: &Args) -> Measures {
    let (w, h, t) = (a.f("width"), a.f("height"), a.f("thickness"));
    ap(Some(w * h - (w - 2.0 * t) * (h - t)), None)
}

const P_HSEC: &[Param] = &[len("width", "the flanges' width", 0.8), len("height", "overall", 1.0), len("flange", "each flange's thickness", 0.15), len("web", "the web's thickness", 0.15)];
fn hsec_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, f, b) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("flange"), 0.5 * a.f("web"));
    if b >= w || f >= h {
        return Err(refuse("web_and_flanges_fit", "the web must be narrower than the flanges, and two flanges thinner than the height", &[("width", 2.0 * w), ("height", 2.0 * h), ("flange", f), ("web", 2.0 * b)]));
    }
    Ok(vec![[-w, -h], [w, -h], [w, -h + f], [b, -h + f], [b, h - f], [w, h - f], [w, h], [-w, h], [-w, h - f], [-b, h - f], [-b, -h + f], [-w, -h + f]])
}
fn hsec_closed(a: &Args) -> Measures {
    let (w, h, f, b) = (a.f("width"), a.f("height"), a.f("flange"), a.f("web"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(2.0 * w * f + b * (h - 2.0 * f)), None) }
}

const P_ZSEC: &[Param] = &[len("width", "overall", 1.0), len("height", "overall", 1.0), len("thickness", "of the web and the flanges", 0.15)];
fn zsec_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (w, h, t) = (0.5 * a.f("width"), 0.5 * a.f("height"), a.f("thickness"));
    if t >= 2.0 * w || t >= h {
        return Err(refuse("web_and_flanges_fit", "the web must be narrower than the width, and two flanges thinner than the height", &[("width", 2.0 * w), ("height", 2.0 * h), ("thickness", t)]));
    }
    let b = 0.5 * t;
    Ok(vec![[-b, -h], [w, -h], [w, -h + t], [b, -h + t], [b, h], [-w, h], [-w, h - t], [-b, h - t]])
}
fn zsec_closed(a: &Args) -> Measures {
    let (w, h, t) = (a.f("width"), a.f("height"), a.f("thickness"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(t * h + (w - t) * t), None) }
}

// ---------------------------------------------------------------- frames

const P_FRAME: &[Param] = &[len("width", "outside, along x", 1.0), len("height", "outside, along y", 0.8), len("border", "the frame's width", 0.1)];
fn frame_geom(a: &Args) -> Result<Geom, Refusal> {
    let (w, h, b) = (a.f("width"), a.f("height"), a.f("border"));
    if 2.0 * b >= w.min(h) {
        return Err(refuse("hole_left", "two borders must leave a hole", &[("width", w), ("height", h), ("border", b)]));
    }
    let mut hole = rect_pts(w - 2.0 * b, h - 2.0 * b);
    hole.reverse();
    Ok(Geom::Rings(vec![poly(&rect_pts(w, h)), poly(&hole)]))
}
fn frame_closed(a: &Args) -> Measures {
    let (w, h, b) = (a.f("width"), a.f("height"), a.f("border"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(w * h - (w - 2.0 * b) * (h - 2.0 * b)), Some(2.0 * (w + h) + 2.0 * (w + h - 4.0 * b))) }
}

const P_PRING: &[Param] = &[count("sides", "the number of sides", 6.0, 3.0, 256.0), len("radius", "the outer circumradius", 0.5), len("inner", "the hole's circumradius", 0.35)];
fn pring_geom(a: &Args) -> Result<Geom, Refusal> {
    let (n, r, i) = (a.n("sides"), a.f("radius"), a.f("inner"));
    if i >= r {
        return Err(refuse("inner_inside_outer", "the hole must be smaller than the polygon", &[("radius", r), ("inner", i)]));
    }
    let mut hole = regular(n, i);
    hole.reverse();
    Ok(Geom::Rings(vec![poly(&regular(n, r)), poly(&hole)]))
}
fn pring_closed(a: &Args) -> Measures {
    let (n, r, i) = (a.f("sides"), a.f("radius"), a.f("inner"));
    Measures { centroid: Some([0.0; 3]), ..ap(Some(0.5 * n * (r * r - i * i) * sin(TAU / n)), Some(2.0 * n * (r + i) * sin(PI / n))) }
}

// ---------------------------------------------------------------- gear

const P_GEAR: &[Param] = &[
    count("teeth", "the number of teeth", 12.0, 3.0, 200.0),
    len("root", "centre to the bottom of the gaps", 0.4),
    len("tip", "centre to the tops of the teeth", 0.5),
    ratio("tooth", "a tooth's width at the root, as a share of the angle from one tooth to the next", 0.5, 0.1, 0.9),
    ratio("land", "a tooth's top width as a share of its root width (1: radial flanks)", 0.5, 0.05, 1.0),
];
fn gear_parts(a: &Args) -> Result<(usize, f64, f64, f64, f64), Refusal> {
    let (n, r, t) = (a.n("teeth"), a.f("root"), a.f("tip"));
    if t <= r {
        return Err(refuse("tip_past_root", "the teeth's tops must be farther from the centre than the gaps' bottoms", &[("root", r), ("tip", t)]));
    }
    let alpha = PI * a.f("tooth") / n as f64;
    Ok((n, r, t, alpha, alpha * a.f("land")))
}
fn gear_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (n, r, t, al, be) = gear_parts(a)?;
    let o = [0.0, 0.0];
    let mut p = Vec::with_capacity(4 * n);
    for i in 0..n {
        let c = 0.5 * PI + TAU * i as f64 / n as f64;
        p.push(polar(o, r, c - al));
        p.push(polar(o, t, c - be));
        p.push(polar(o, t, c + be));
        p.push(polar(o, r, c + al));
    }
    Ok(p)
}
fn gear_closed(a: &Args) -> Measures {
    let Ok((n, r, t, al, be)) = gear_parts(a) else { return Measures::default() };
    let n = n as f64;
    let per = 0.5 * r * t * sin(al - be) * 2.0 + 0.5 * t * t * sin(2.0 * be) + 0.5 * r * r * sin(TAU / n - 2.0 * al);
    Measures { centroid: Some([0.0; 3]), ..ap(Some(n * per), None) }
}
fn gear_anchors(a: &Args) -> Vec<Anchor> {
    let mut v = vec![at("centre", [0.0, 0.0], On::Inside)];
    let Ok((n, _, t, _, be)) = gear_parts(a) else { return v };
    for i in 0..n.min(anchor::MAX_NUMBERED) {
        let c = 0.5 * PI + TAU * i as f64 / n as f64;
        // The middle of the tooth's top land, a chord of the tip circle.
        v.push(out(&format!("tooth_{i}"), polar([0.0, 0.0], t * cos(be), c), [cos(c), sin(c)], On::Boundary));
    }
    v
}

// ---------------------------------------------------------------- bands

const P_SAW: &[Param] = &[len("length", "along x", 2.0), len("base", "the solid strip under the teeth", 0.1), len("height", "the teeth's height", 0.2), count("teeth", "the number of teeth", 8.0, 1.0, 1000.0)];
fn saw_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (l, b, h, n) = (a.f("length"), a.f("base"), a.f("height"), a.n("teeth"));
    let (x, p) = (0.5 * l, l / n as f64);
    let mut v = vec![[-x, 0.0], [x, 0.0], [x, b + h]];
    for i in 1..n {
        let xi = x - p * i as f64;
        v.push([xi, b]);
        v.push([xi, b + h]);
    }
    v.push([-x, b]);
    Ok(v)
}
fn saw_closed(a: &Args) -> Measures {
    ap(Some(a.f("length") * (a.f("base") + 0.5 * a.f("height"))), None)
}

const P_BATTLE: &[Param] = &[
    len("length", "along x", 2.0),
    len("base", "the wall under the merlons", 0.3),
    len("height", "the merlons' height", 0.2),
    count("merlons", "the number of merlons (one at each end)", 5.0, 2.0, 200.0),
    ratio("merlon", "a merlon's width as a share of a merlon and a gap", 0.5, 0.1, 0.9),
];
fn battle_dims(a: &Args) -> (f64, f64, f64, f64, usize, f64) {
    let (l, b, h, n, f) = (a.f("length"), a.f("base"), a.f("height"), a.n("merlons"), a.f("merlon"));
    let p = l / (n as f64 * f + (n as f64 - 1.0) * (1.0 - f));
    (l, b, h, f * p, n, (1.0 - f) * p)
}
fn battle_pts(a: &Args) -> Result<Vec<Pt>, Refusal> {
    let (l, b, h, m, n, c) = battle_dims(a);
    let mut v = vec![[-0.5 * l, 0.0], [0.5 * l, 0.0]];
    let mut x = 0.5 * l;
    for j in 0..n {
        v.push([x, b + h]);
        v.push([x - m, b + h]);
        x -= m;
        if j + 1 < n {
            v.push([x, b]);
            v.push([x - c, b]);
            x -= c;
        }
    }
    Ok(v)
}
fn battle_closed(a: &Args) -> Measures {
    let (l, b, h, m, n, _) = battle_dims(a);
    ap(Some(l * b + n as f64 * m * h), None)
}

// ---------------------------------------------------------------- the table

macro_rules! pts {
    ($f:ident) => {{
        fn g(a: &Args) -> Result<Geom, Refusal> {
            pts_ring($f(a))
        }
        g
    }};
}
macro_rules! verts {
    ($f:ident) => {{
        fn g(a: &Args) -> Vec<Anchor> {
            $f(a).map(|p| vertices("vertex", &p)).unwrap_or_default()
        }
        g
    }};
}
macro_rules! tri {
    ($f:ident) => {{
        fn g(a: &Args) -> Vec<Anchor> {
            $f(a).map(|p| tri_anchors(&p)).unwrap_or_default()
        }
        g
    }};
}
fn rect_geom(a: &Args) -> Result<Geom, Refusal> {
    Ok(ring(poly(&rect_pts(a.f("width"), a.f("height")))))
}

pub(crate) const FORMS: &[Form] = &[
    plane!("triangle_sss", Triangle, "a triangle by its three sides", "the centroid", P_SSS, curved: false, kin: KIN_TRI_SSS, pts!(sss_pts), sss_closed, tri!(sss_pts)),
    plane!("triangle_sas", Triangle, "a triangle by two sides and the angle between them", "the centroid", P_SAS, curved: false, kin: KIN_TRIANGLE, pts!(sas_pts), sas_closed, tri!(sas_pts)),
    plane!("triangle", Triangle, "a triangle by its base, its height and where the apex is", "the centroid", P_TRI, curved: false, kin: KIN_TRI_BH, pts!(tri_pts), tri_closed, tri!(tri_pts)),
    plane!("rectangle", Quadrilateral, "a rectangle", "the centre", P_RECT, curved: false, kin: KIN_RECT, rect_geom, rect_closed, rect_anchors),
    plane!("parallelogram", Quadrilateral, "a parallelogram by its base, height and bottom-left angle", "the centre", P_PARA, curved: false, kin: KIN_PRISM, pts!(para_pts), para_closed, verts!(para_pts)),
    plane!("rhombus", Quadrilateral, "a rhombus by its two diagonals", "where the diagonals cross", P_RHOMBUS, curved: false, kin: &["prism", "rhombic_dodecahedron", "rhombic_triacontahedron", "bicone"], pts!(rhombus_pts), rhombus_closed, verts!(rhombus_pts)),
    plane!("kite", Quadrilateral, "a convex kite: two pairs of equal sides next to each other", "where the diagonals cross", P_KITE, curved: false, kin: &["prism", "trapezohedron", "deltoidal_icositetrahedron", "deltoidal_hexecontahedron"], pts!(kite_pts), kite_closed, diag_anchors),
    plane!("dart", Quadrilateral, "a concave kite (an arrowhead): a tip, two wings and a reflex notch", "the middle of the line between the wing tips", P_DART, curved: false, kin: KIN_PRISM, pts!(dart_pts), dart_closed, diag_anchors),
    plane!("trapezium", Quadrilateral, "a quadrilateral with one pair of parallel sides (US: trapezoid)", "the bottom side's middle, at half the height", P_TRAP, curved: false, kin: &["prism", "pyramid_frustum", "frustum"], pts!(trap_pts), trap_closed, verts!(trap_pts)),
    plane!("regular_polygon", Polygon, "a regular polygon, a vertex on +y", "the centre", P_POLY, curved: false, kin: KIN_POLY, pts!(poly_pts), poly_closed, poly_anchors),
    plane!("star", Star, "a star of n points with its tip and notch radii", "the centre", P_STAR, curved: false, kin: &["star_prism"], pts!(star_pts), star_closed, star_anchors),
    plane!("star_polygon", Star, "the outline of the regular star polygon {n/k}", "the centre", P_STARPOLY, curved: false, kin: &["star_prism", "stella_octangula", "small_stellated_dodecahedron"], pts!(starpoly_pts), none, starpoly_anchors),
    plane!("cross", Symbol, "a plus-shaped cross", "the centre", P_CROSS, curved: false, kin: KIN_PRISM, pts!(cross_pts), cross_closed, cross_anchors),
    plane!("saltire", Symbol, "an X-shaped cross (the plus turned 45 degrees)", "the centre", P_SALTIRE, curved: false, kin: KIN_PRISM, pts!(saltire_pts), saltire_closed, verts!(saltire_pts)),
    plane!("arrow", Symbol, "an arrow pointing along +x: a shaft and a triangular head", POLY_ORIGIN, P_ARROW, curved: false, kin: &["prism", "round_arrow"], pts!(arrow_pts), arrow_closed, arrow_anchors),
    plane!("double_arrow", Symbol, "an arrow with a head at each end", POLY_ORIGIN, P_ARROW, curved: false, kin: KIN_PRISM, pts!(darrow_pts), darrow_closed, darrow_anchors),
    plane!("chevron", Symbol, "a chevron: a V band pointing up, of a thickness measured vertically", POLY_ORIGIN, P_CHEVRON, curved: false, kin: KIN_PRISM, pts!(chevron_pts), chevron_closed, chevron_anchors),
    plane!("shield", Symbol, "a heater shield: a straight top and sides, and two arcs meeting at the point", POLY_ORIGIN, P_SHIELD, curved: true, kin: KIN_PRISM, shield_geom, none, shield_anchors),
    plane!("l_section", Section, "an L (angle) section", POLY_ORIGIN, P_LSEC, curved: false, kin: KIN_PRISM, pts!(lsec_pts), lsec_closed, verts!(lsec_pts)),
    plane!("t_section", Section, "a T section: a flange on top of a web", POLY_ORIGIN, P_TSEC, curved: false, kin: KIN_PRISM, pts!(tsec_pts), tsec_closed, verts!(tsec_pts)),
    plane!("u_section", Section, "a U (channel) section, open upward", POLY_ORIGIN, P_USEC, curved: false, kin: &["prism", "cup"], pts!(usec_pts), usec_closed, verts!(usec_pts)),
    plane!("h_section", Section, "an H (I-beam) section: two flanges joined by a web", POLY_ORIGIN, P_HSEC, curved: false, kin: KIN_PRISM, pts!(hsec_pts), hsec_closed, verts!(hsec_pts)),
    plane!("z_section", Section, "a Z section: a web with flanges to opposite sides", POLY_ORIGIN, P_ZSEC, curved: false, kin: KIN_PRISM, pts!(zsec_pts), zsec_closed, verts!(zsec_pts)),
    plane!("frame", Frame, "a rectangle with a rectangular hole of an even border", "the centre", P_FRAME, curved: false, kin: &["slab"], frame_geom, frame_closed, frame_anchors),
    plane!("polygon_ring", Frame, "a regular polygon with a similar hole", "the centre", P_PRING, curved: false, kin: &["slab"], pring_geom, pring_closed, pring_anchors),
    plane!("gear", Gear, "a spur gear with straight flanks (not an involute profile)", "the centre", P_GEAR, curved: false, kin: KIN_PRISM, pts!(gear_pts), gear_closed, gear_anchors),
    plane!("sawtooth_band", Band, "a strip with sawteeth on its top edge (a ramp and a drop)", "the middle of the bottom edge", P_SAW, curved: false, kin: KIN_PRISM, pts!(saw_pts), saw_closed, no_anchors),
    plane!("battlement_band", Band, "a wall with merlons on its top edge, one at each end", "the middle of the bottom edge", P_BATTLE, curved: false, kin: KIN_PRISM, pts!(battle_pts), battle_closed, no_anchors),
];

pub(crate) const ALIASES: &[Alias] = &[
    alias!("equilateral_triangle", Plane, "triangle_sss", "a triangle of three equal sides", &[Set::Tie { name: "side", about: "each side", targets: &["a", "b", "c"] }]),
    alias!("isosceles_triangle", Plane, "triangle", "a triangle with its apex over the base's middle", &[Set::Fixed("apex", 0.0)]),
    alias!("right_triangle", Plane, "triangle", "a triangle with its right angle at the base's left end", &[Set::Fixed("apex", -0.5)]),
    alias!("square", Plane, "rectangle", "a square", &[Set::Tie { name: "size", about: "each side", targets: &["width", "height"] }]),
    alias!("oblong", Plane, "rectangle", "a rectangle", &[]),
    alias!("diamond", Plane, "rhombus", "a rhombus", &[]),
    alias!("lozenge", Plane, "rhombus", "a rhombus", &[]),
    alias!("trapezoid", Plane, "trapezium", "the US word for the trapezium (one pair of parallel sides)", &[]),
    alias!("isosceles_trapezium", Plane, "trapezium", "a trapezium with equal legs", &[Set::Fixed("lean", 0.0)]),
    alias!("isosceles_trapezoid", Plane, "trapezium", "a trapezium with equal legs", &[Set::Fixed("lean", 0.0)]),
    alias!("right_trapezium", Plane, "trapezium", "a trapezium with right angles on its left side", &[Set::Fixed("lean", -1.0)]),
    alias!("right_trapezoid", Plane, "trapezium", "a trapezium with right angles on its left side", &[Set::Fixed("lean", -1.0)]),
    alias!("pentagon", Plane, "regular_polygon", "a regular pentagon", &[Set::Fixed("sides", 5.0)]),
    alias!("hexagon", Plane, "regular_polygon", "a regular hexagon", &[Set::Fixed("sides", 6.0)]),
    alias!("heptagon", Plane, "regular_polygon", "a regular heptagon", &[Set::Fixed("sides", 7.0)]),
    alias!("octagon", Plane, "regular_polygon", "a regular octagon", &[Set::Fixed("sides", 8.0)]),
    alias!("nonagon", Plane, "regular_polygon", "a regular nonagon", &[Set::Fixed("sides", 9.0)]),
    alias!("decagon", Plane, "regular_polygon", "a regular decagon", &[Set::Fixed("sides", 10.0)]),
    alias!("hendecagon", Plane, "regular_polygon", "a regular hendecagon", &[Set::Fixed("sides", 11.0)]),
    alias!("dodecagon", Plane, "regular_polygon", "a regular dodecagon", &[Set::Fixed("sides", 12.0)]),
    alias!("pentagram", Plane, "star_polygon", "the star polygon {5/2}", &[Set::Fixed("points", 5.0), Set::Fixed("step", 2.0)]),
    alias!("hexagram", Plane, "star_polygon", "the star polygon {6/2} (the outline of two triangles)", &[Set::Fixed("points", 6.0), Set::Fixed("step", 2.0)]),
    alias!("octagram", Plane, "star_polygon", "the star polygon {8/3}", &[Set::Fixed("points", 8.0), Set::Fixed("step", 3.0)]),
    alias!("plus", Plane, "cross", "a plus sign", &[]),
    alias!("channel", Plane, "u_section", "a channel section", &[]),
    alias!("i_beam", Plane, "h_section", "an I-beam section", &[]),
    alias!("angle_section", Plane, "l_section", "an angle section", &[]),
];
