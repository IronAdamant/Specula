//! Tier-3 SDF modelling: shapes as signed distance functions, combined with sharp or smooth
//! booleans and domain operations, then meshed by dual contouring.
//!
//! **Field:** [`SdfNode`] trees of primitives (sphere, box, cylinder/frustum, capsule, cone,
//! torus, ellipsoid, half-space), unions, differences and intersections (each optionally smooth,
//! Inigo Quilez's polynomial smooth minimum), and per-node modifiers: placement (translation,
//! rotation, uniform scale), mirror folds, finite repetition, elongation, twist, bend, rounding,
//! shells and fBm noise displacement. Primitives are **centred** on their placement (mesh
//! shapes rest on y = 0; a sculpt is placed point by point). Every node may carry a colour;
//! blends mix colours as they mix distances.
//!
//! **Mesh:** a grid over the tree's bounds (`resolution` cells along the longest side, at most
//! [`MAX_RESOLUTION`]; at most [`MAX_SAMPLES`] samples), the field sampled sparsely (8³-cell
//! blocks whose centre is farther from the surface than their reach are filled by sign, using a
//! conservative Lipschitz bound of the tree), then one vertex per surface cell placed by a
//! regularised QEF of the edge crossings' planes (dual contouring: flat faces stay flat, creases
//! stay sharp), one quad per sign-changing edge. The outer layer of samples counts as outside,
//! so the mesh is always closed. Everything is charged to a work budget before it is done.
//!
//! **Determinism (F2):** `f32`/`f64` basic operations, `sqrt`, [`crate::dmath`] trigonometry
//! (rotations are precomputed) and the crate's hash noise; dense arrays, no hash maps.

use crate::math3::{self, Affine, V3};
use crate::noise;
use crate::shape::{Rgba, Shape, TAG_FACE};

/// Most cells along the longest side.
pub const MAX_RESOLUTION: u32 = 160;
/// Most grid samples (the field's memory: 4 bytes each).
pub const MAX_SAMPLES: usize = 1 << 22;
/// Most nodes in one tree.
pub const MAX_NODES: usize = 128;
/// Most work units one sculpt may spend on its own ([`crate::work`]: samples × [`eval_cost`],
/// crossings' normals and colours included); a recipe's sculpt gets what is left of its build.
pub const MAX_WORK: u64 = crate::work::MAX_BUILD;
/// Most repetitions of one `repeat` (all axes together).
pub const MAX_REPEAT: u32 = 64;

/// A primitive's shape, in its own frame (centred on the origin, Y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prim {
    Sphere { radius: f32 },
    /// Half extents; `round` rounds the edges (inside the extents).
    Box { half: V3, round: f32 },
    /// A cylinder or frustum along Y: radius at the bottom, at the top, half height.
    Cylinder { radius: f32, radius_top: f32, half_height: f32, round: f32 },
    /// A segment along Y of half length `half_height`, thickened by `radius`.
    Capsule { radius: f32, half_height: f32 },
    /// In the XZ plane: ring radius and tube radius.
    Torus { radius: f32, tube: f32 },
    Ellipsoid { radii: V3 },
    /// The half-space below the plane `normal · p = offset` (unbounded: give the sdf bounds).
    HalfSpace { normal: V3, offset: f32 },
}

/// How children combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    Union,
    /// The first child minus every other.
    Difference,
    Intersection,
}

/// Domain and distance modifiers of a node, applied in this order: mirror (in the parent's
/// frame), placement, repeat, elongate, twist, bend; then the child/primitive distance; then
/// round, shell, displace.
#[derive(Clone, Debug, PartialEq)]
pub struct Modifiers {
    /// Fold these axes (`p.x = |p.x|`): a mirror image for free.
    pub mirror: [bool; 3],
    /// Translation, rotation (degrees, X then Y then Z) and uniform scale.
    pub at: V3,
    pub rotate: V3,
    pub scale: f32,
    /// Finite repetition: copies per axis and their spacing (metres).
    pub repeat: Option<([u32; 3], V3)>,
    /// Stretch the shape by inserting these half lengths at its middle.
    pub elongate: V3,
    /// Degrees per metre of twist about Y, of bend about Z (along X).
    pub twist: f32,
    pub bend: f32,
    /// Subtract from the distance (round everything by this much).
    pub round: f32,
    /// Keep only a shell this thick around the surface.
    pub shell: f32,
    /// fBm noise added to the distance: amplitude (metres), feature size (metres), octaves.
    pub displace: Option<(f32, f32, u32)>,
    pub color: Option<Rgba>,
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers {
            mirror: [false; 3],
            at: [0.0; 3],
            rotate: [0.0; 3],
            scale: 1.0,
            repeat: None,
            elongate: [0.0; 3],
            twist: 0.0,
            bend: 0.0,
            round: 0.0,
            shell: 0.0,
            displace: None,
            color: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Prim(Prim),
    /// Children and the smoothing radius (0 = sharp).
    Group { op: Combine, kids: Vec<SdfNode>, smooth: f32 },
}

/// A node of a distance tree.
#[derive(Clone, Debug, PartialEq)]
pub struct SdfNode {
    pub kind: Kind,
    pub m: Modifiers,
}

/// A tree prepared for evaluation: rotations as matrices, seeds fixed.
#[derive(Clone, Debug)]
struct Prepared {
    kind: PKind,
    m: Modifiers,
    /// World -> local: inverse rotation columns (the rotation's rows).
    inv_rot: [V3; 3],
    rotated: bool,
    seed: u64,
}

#[derive(Clone, Debug)]
enum PKind {
    Prim(Prim),
    Group { op: Combine, kids: Vec<Prepared>, smooth: f32 },
}

/// Why a sculpt failed.
#[derive(Clone, Debug, PartialEq)]
pub struct SdfError {
    pub limit: bool,
    pub message: String,
    pub measured: Option<f64>,
    pub max: Option<f64>,
}

impl SdfError {
    fn geometry(m: impl Into<String>) -> SdfError {
        SdfError { limit: false, message: m.into(), measured: None, max: None }
    }
    fn limit(m: impl Into<String>, measured: f64, max: f64) -> SdfError {
        SdfError { limit: true, message: m.into(), measured: Some(measured), max: Some(max) }
    }
}

/// What meshing did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SdfStats {
    pub dims: [u32; 3],
    pub cell: f32,
    pub samples_evaluated: u64,
    pub blocks_skipped: u64,
    pub work: u64,
    pub triangles: usize,
}

fn count_nodes(n: &SdfNode) -> usize {
    1 + match &n.kind {
        Kind::Prim(_) => 0,
        Kind::Group { kids, .. } => kids.iter().map(count_nodes).sum(),
    }
}

fn prepare(n: &SdfNode, seed: u64, next: &mut u64) -> Prepared {
    let r = Affine::rotate_euler_deg([n.m.rotate[0] as f64, n.m.rotate[1] as f64, n.m.rotate[2] as f64]);
    // Inverse of a rotation: its transpose (columns become rows).
    let c = r.cols;
    let inv_rot = [[c[0][0], c[1][0], c[2][0]], [c[0][1], c[1][1], c[2][1]], [c[0][2], c[1][2], c[2][2]]];
    *next += 1;
    let my_seed = seed ^ next.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let kind = match &n.kind {
        Kind::Prim(p) => PKind::Prim(*p),
        Kind::Group { op, kids, smooth } => PKind::Group { op: *op, kids: kids.iter().map(|k| prepare(k, seed, next)).collect(), smooth: *smooth },
    };
    Prepared { kind, m: n.m.clone(), inv_rot, rotated: n.m.rotate != [0.0; 3], seed: my_seed }
}

#[inline]
fn len2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn prim_distance(p: &Prim, q: V3) -> f32 {
    match *p {
        Prim::Sphere { radius } => math3::length(q) - radius,
        Prim::Box { half, round } => {
            let d = [q[0].abs() - half[0] + round, q[1].abs() - half[1] + round, q[2].abs() - half[2] + round];
            let outside = math3::length([d[0].max(0.0), d[1].max(0.0), d[2].max(0.0)]);
            outside + d[0].max(d[1]).max(d[2]).min(0.0) - round
        }
        Prim::Cylinder { radius, radius_top, half_height, round } => {
            if radius == radius_top {
                let d = [len2(q[0], q[2]) - radius + round, q[1].abs() - half_height + round];
                d[0].max(d[1]).min(0.0) + len2(d[0].max(0.0), d[1].max(0.0)) - round
            } else {
                capped_cone(q, half_height, radius, radius_top)
            }
        }
        Prim::Capsule { radius, half_height } => {
            let y = q[1] - q[1].clamp(-half_height, half_height);
            math3::length([q[0], y, q[2]]) - radius
        }
        Prim::Torus { radius, tube } => len2(len2(q[0], q[2]) - radius, q[1]) - tube,
        Prim::Ellipsoid { radii } => {
            let k0 = math3::length([q[0] / radii[0], q[1] / radii[1], q[2] / radii[2]]);
            let k1 = math3::length([q[0] / (radii[0] * radii[0]), q[1] / (radii[1] * radii[1]), q[2] / (radii[2] * radii[2])]);
            if k1 > 0.0 {
                k0 * (k0 - 1.0) / k1
            } else {
                -radii[0].min(radii[1]).min(radii[2])
            }
        }
        Prim::HalfSpace { normal, offset } => math3::dot(q, normal) - offset,
    }
}

/// Inigo Quilez's capped cone: radius `r1` at y = -h, `r2` at y = +h.
fn capped_cone(p: V3, h: f32, r1: f32, r2: f32) -> f32 {
    let q = [len2(p[0], p[2]), p[1]];
    let k1 = [r2, h];
    let k2 = [r2 - r1, 2.0 * h];
    let ca = [q[0] - q[0].min(if q[1] < 0.0 { r1 } else { r2 }), q[1].abs() - h];
    let d2 = k2[0] * k2[0] + k2[1] * k2[1];
    let t = (((k1[0] - q[0]) * k2[0] + (k1[1] - q[1]) * k2[1]) / d2).clamp(0.0, 1.0);
    let cb = [q[0] - k1[0] + k2[0] * t, q[1] - k1[1] + k2[1] * t];
    let s = if cb[0] < 0.0 && ca[1] < 0.0 { -1.0 } else { 1.0 };
    s * (ca[0] * ca[0] + ca[1] * ca[1]).min(cb[0] * cb[0] + cb[1] * cb[1]).sqrt()
}

/// Polynomial smooth minimum and the blend weight of `b` (0..1).
#[inline]
fn smin(a: f32, b: f32, k: f32) -> (f32, f32) {
    if k <= 0.0 {
        return if a <= b { (a, 0.0) } else { (b, 1.0) };
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    let w = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    (a.min(b) - h * h * k * 0.25, w)
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    crate::shape::mix(a, b, t)
}

/// The point in the node's own frame (domain modifiers applied).
fn local(n: &Prepared, p: V3) -> V3 {
    let m = &n.m;
    let mut q = p;
    for (qk, mk) in q.iter_mut().zip(m.mirror) {
        if mk {
            *qk = qk.abs();
        }
    }
    q = math3::sub(q, m.at);
    if n.rotated {
        q = [math3::dot(n.inv_rot[0], q), math3::dot(n.inv_rot[1], q), math3::dot(n.inv_rot[2], q)];
    }
    if m.scale != 1.0 {
        q = math3::scale(q, 1.0 / m.scale);
    }
    if let Some((count, spacing)) = m.repeat {
        for k in 0..3 {
            if count[k] > 1 && spacing[k] > 0.0 {
                let half = (count[k] - 1) as f32 * 0.5;
                // Index of the nearest copy, clamped to the copies that exist.
                let idx = crate::dmath::floor(q[k] / spacing[k] + half + 0.5).clamp(0.0, count[k] as f32 - 1.0);
                q[k] -= (idx - half) * spacing[k];
            }
        }
    }
    if m.elongate != [0.0; 3] {
        for (qk, ek) in q.iter_mut().zip(m.elongate) {
            *qk -= qk.clamp(-ek, ek);
        }
    }
    if m.twist != 0.0 {
        let (s, c) = crate::dmath::sin_cos(-m.twist * q[1] * (std::f32::consts::PI / 180.0));
        q = [c * q[0] - s * q[2], q[1], s * q[0] + c * q[2]];
    }
    if m.bend != 0.0 {
        let (s, c) = crate::dmath::sin_cos(-m.bend * q[0] * (std::f32::consts::PI / 180.0));
        q = [c * q[0] - s * q[1], s * q[0] + c * q[1], q[2]];
    }
    q
}

/// The distance after the node's distance modifiers.
fn finish(n: &Prepared, mut d: f32, q: V3) -> f32 {
    let m = &n.m;
    d *= m.scale;
    if m.round != 0.0 {
        d -= m.round;
    }
    if m.shell > 0.0 {
        d = d.abs() - m.shell * 0.5;
    }
    if let Some((amount, size, octaves)) = m.displace {
        let s = 1.0 / size;
        d += amount * noise::fbm3(n.seed, [q[0] * s, q[1] * s, q[2] * s], octaves);
    }
    d
}

fn eval(n: &Prepared, p: V3) -> f32 {
    let q = local(n, p);
    let d = match &n.kind {
        PKind::Prim(pr) => prim_distance(pr, q),
        PKind::Group { op, kids, smooth } => {
            let mut it = kids.iter();
            let mut acc = it.next().map_or(f32::MAX, |k| eval(k, q));
            for k in it {
                let b = eval(k, q);
                acc = match op {
                    Combine::Union => smin(acc, b, *smooth).0,
                    Combine::Intersection => -smin(-acc, -b, *smooth).0,
                    Combine::Difference => -smin(-acc, b, *smooth).0,
                };
            }
            acc
        }
    };
    finish(n, d, q)
}

/// Distance and colour (`inherit`: the colour of an uncoloured node).
fn eval_color(n: &Prepared, p: V3, inherit: Rgba) -> (f32, Rgba) {
    let q = local(n, p);
    let mine = n.m.color.unwrap_or(inherit);
    let (d, c) = match &n.kind {
        PKind::Prim(pr) => (prim_distance(pr, q), mine),
        PKind::Group { op, kids, smooth } => {
            let mut it = kids.iter();
            let (mut acc, mut col) = it.next().map_or((f32::MAX, mine), |k| eval_color(k, q, mine));
            for k in it {
                let (b, cb) = eval_color(k, q, mine);
                match op {
                    Combine::Union => {
                        let (d, w) = smin(acc, b, *smooth);
                        // w: the weight of the second (1 where it decides the surface).
                        col = mix(col, cb, w);
                        acc = d;
                    }
                    Combine::Intersection => {
                        let (d, w) = smin(-acc, -b, *smooth);
                        col = mix(col, cb, w);
                        acc = -d;
                    }
                    Combine::Difference => {
                        let (d, w) = smin(-acc, b, *smooth);
                        // Where the cutter decides the surface, the cut face takes its colour.
                        col = mix(col, cb, w);
                        acc = -d;
                    }
                }
            }
            (acc, col)
        }
    };
    (finish(n, d, q), c)
}

/// Work units ([`crate::work`], ≈ ns) of one evaluation of a node and everything under it,
/// weighed by the `calibrate_sdf` test: noise octaves and trigonometry cost far more than a
/// sphere.
pub fn eval_cost(n: &SdfNode) -> u64 {
    let m = &n.m;
    // Measured (2026-10-22, release, one core of the reference laptop): a sphere about 70 ns
    // with its frame and modifiers, a cone 105, a twist +80, a repeat +45, each noise octave
    // about 50-60; weighed half as much again for the mesher's own work per sample.
    let mut c = match &n.kind {
        Kind::Prim(p) => {
            95 + match *p {
                Prim::HalfSpace { .. } => 0,
                Prim::Sphere { .. } | Prim::Capsule { .. } => 12,
                Prim::Box { .. } | Prim::Torus { .. } | Prim::Ellipsoid { .. } => 24,
                Prim::Cylinder { radius, radius_top, .. } if radius == radius_top => 24,
                Prim::Cylinder { .. } => 70,
            }
        }
        Kind::Group { kids, smooth, .. } => 15 + kids.iter().map(eval_cost).sum::<u64>() + kids.len() as u64 * if *smooth > 0.0 { 40 } else { 6 },
    };
    if m.rotate != [0.0; 3] {
        c += 12;
    }
    if m.repeat.is_some() {
        c += 75;
    }
    if m.twist != 0.0 {
        c += 130;
    }
    if m.bend != 0.0 {
        c += 45;
    }
    if let Some((_, _, octaves)) = m.displace {
        c += 30 + 90 * octaves.max(1) as u64;
    }
    c
}

/// A node's Lipschitz factor (how much faster than distance its field may change) and slack
/// (how far smoothing and displacement may move it), in its parent's frame. Unlike [`bounds`]
/// this folds in every child, bounded or not: an unbounded cutter's displacement still counts.
fn lip_slack(n: &SdfNode) -> (f32, f32) {
    let m = &n.m;
    let (mut lip, mut slack) = match &n.kind {
        Kind::Prim(p) => (if matches!(p, Prim::Ellipsoid { .. }) { 2.0 } else { 1.0 }, 0.0),
        Kind::Group { kids, smooth, .. } => kids.iter().map(lip_slack).fold((1.0f32, smooth.max(0.0)), |(l, s), (kl, ks)| (l.max(kl), s.max(ks))),
    };
    slack *= m.scale;
    if m.twist != 0.0 || m.bend != 0.0 {
        // As in `bounds`: the field's trust falls with the radius around the axis; with no
        // radius to go by, never trust it to skip anything.
        match bounds(&SdfNode { kind: n.kind.clone(), m: Modifiers::default() }) {
            Some((lo, hi, _, _)) => {
                let r = (0..3).map(|k| lo[k].abs().max(hi[k].abs())).fold(0.0f32, f32::max);
                let ext = hi[1].abs().max(lo[1].abs()).max(hi[0].abs().max(lo[0].abs()));
                lip *= 1.0 + (m.twist.abs() + m.bend.abs()) * (std::f32::consts::PI / 180.0) * (r + ext);
            }
            None => lip = f32::INFINITY,
        }
    }
    if let Some((amount, _, _)) = m.displace {
        slack += 2.0 * amount.abs();
    }
    (lip, slack)
}

/// Conservative bounds of a node in its parent's frame, and its Lipschitz factor (how much
/// faster than distance the field may change) plus a fixed slack (displacement, smoothing).
fn bounds(n: &SdfNode) -> Option<(V3, V3, f32, f32)> {
    let m = &n.m;
    let (mut lo, mut hi, mut lip, mut slack) = match &n.kind {
        Kind::Prim(p) => {
            let e: V3 = match *p {
                Prim::Sphere { radius } => [radius; 3],
                Prim::Box { half, .. } => half,
                Prim::Cylinder { radius, radius_top, half_height, .. } => [radius.max(radius_top), half_height, radius.max(radius_top)],
                Prim::Capsule { radius, half_height } => [radius, half_height + radius, radius],
                Prim::Torus { radius, tube } => [radius + tube, tube, radius + tube],
                Prim::Ellipsoid { radii } => radii,
                Prim::HalfSpace { .. } => return None,
            };
            // The ellipsoid's field is only a bound: allow it twice the speed.
            let lip = if matches!(p, Prim::Ellipsoid { .. }) { 2.0 } else { 1.0 };
            ([-e[0], -e[1], -e[2]], e, lip, 0.0)
        }
        Kind::Group { op, kids, smooth } => {
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            let (mut lip, mut slack) = (1.0f32, *smooth);
            let mut first = true;
            for k in kids {
                let b = bounds(k);
                match (op, b) {
                    (_, Some((l, h, kl, ks))) => {
                        lip = lip.max(kl);
                        slack = slack.max(ks);
                        if first || *op == Combine::Union {
                            lo = math3::vmin(lo, l);
                            hi = math3::vmax(hi, h);
                        } else if *op == Combine::Intersection {
                            lo = math3::vmax(lo, l);
                            hi = math3::vmin(hi, h);
                        }
                    }
                    // An unbounded child: the union is unbounded; it doesn't widen a cut or an
                    // intersection's other bounds.
                    (Combine::Union, None) => return None,
                    (_, None) if first => return None,
                    _ => {}
                }
                first = false;
            }
            (lo, hi, lip, slack)
        }
    };
    if lo[0] > hi[0] || lo[1] > hi[1] || lo[2] > hi[2] {
        return Some(([0.0; 3], [0.0; 3], lip, slack));
    }
    // The children's slack (smoothing, their displacement) is in this node's units: scale it.
    slack *= m.scale;
    // Modifiers, inside out.
    if m.twist != 0.0 || m.bend != 0.0 {
        // Twisted/bent: bound by the radius around the axis, and slow the field's trust.
        let r = (0..3).map(|k| lo[k].abs().max(hi[k].abs())).fold(0.0f32, f32::max);
        let ext = hi[1].abs().max(lo[1].abs()).max(hi[0].abs().max(lo[0].abs()));
        let k = (m.twist.abs() + m.bend.abs()) * (std::f32::consts::PI / 180.0);
        lip *= 1.0 + k * (r + ext);
        lo = [-r; 3];
        hi = [r; 3];
    }
    for k in 0..3 {
        lo[k] -= m.elongate[k];
        hi[k] += m.elongate[k];
    }
    if let Some((count, spacing)) = m.repeat {
        for k in 0..3 {
            let span = (count[k].max(1) - 1) as f32 * spacing[k] * 0.5;
            lo[k] -= span;
            hi[k] += span;
        }
    }
    let grow = m.round.max(0.0) + m.shell * 0.5;
    if let Some((amount, _, _)) = m.displace {
        // Noise moves the field by up to its amplitude anywhere: twice that between two points.
        slack += 2.0 * amount.abs();
    }
    // Scale, rotate (the bounding box of the rotated box), translate.
    let s = m.scale;
    let (lo_s, hi_s) = (math3::scale(lo, s), math3::scale(hi, s));
    let r = Affine::rotate_euler_deg([m.rotate[0] as f64, m.rotate[1] as f64, m.rotate[2] as f64]);
    let mut wlo = [f32::MAX; 3];
    let mut whi = [f32::MIN; 3];
    for c in 0..8 {
        let p = [if c & 1 == 0 { lo_s[0] } else { hi_s[0] }, if c & 2 == 0 { lo_s[1] } else { hi_s[1] }, if c & 4 == 0 { lo_s[2] } else { hi_s[2] }];
        let w = math3::add(r.apply_point(p), m.at);
        wlo = math3::vmin(wlo, w);
        whi = math3::vmax(whi, w);
    }
    for k in 0..3 {
        wlo[k] -= grow + slack;
        whi[k] += grow + slack;
        if m.mirror[k] {
            let e = wlo[k].abs().max(whi[k].abs());
            wlo[k] = -e;
            whi[k] = e;
        }
    }
    Some((wlo, whi, lip, slack))
}

/// Options for [`mesh`].
#[derive(Clone, Copy, Debug)]
pub struct MeshOptions {
    /// Cells along the longest side of the bounds.
    pub resolution: u32,
    /// Explicit bounds (min, max); required when the tree is unbounded (a half-space).
    pub bounds: Option<(V3, V3)>,
    pub seed: u64,
    /// Most triangles the result may have (checked before any is made).
    pub max_triangles: usize,
    pub max_work: u64,
    /// The colour of uncoloured nodes.
    pub color: Rgba,
}

/// Meshes a tree (uncoloured nodes take `opt.color`).
pub fn mesh(tree: &SdfNode, opt: &MeshOptions) -> Result<(Shape, SdfStats), SdfError> {
    let nodes = count_nodes(tree);
    if nodes > MAX_NODES {
        return Err(SdfError::limit(format!("the sdf tree has {nodes} nodes; at most {MAX_NODES}"), nodes as f64, MAX_NODES as f64));
    }
    if !(2..=MAX_RESOLUTION).contains(&opt.resolution) {
        return Err(SdfError::limit(format!("resolution {} is outside 2..{MAX_RESOLUTION}", opt.resolution), opt.resolution as f64, MAX_RESOLUTION as f64));
    }
    let (auto_lo, auto_hi) = match bounds(tree) {
        Some((lo, hi, _, _)) => (lo, hi),
        None if opt.bounds.is_some() => ([0.0; 3], [0.0; 3]),
        None => return Err(SdfError::geometry("the sdf is unbounded (a half-space in a union): give it \"bounds\"")),
    };
    // Every node's Lipschitz factor and slack, bounded or not (an unbounded displaced cutter
    // moves the surface as much as a bounded one).
    let (lip, slack) = lip_slack(tree);
    // Work units per evaluation (≈ ns: noise octaves weigh what they cost).
    let cost = eval_cost(tree);
    let (lo, hi) = opt.bounds.unwrap_or((auto_lo, auto_hi));
    let size = math3::sub(hi, lo);
    let longest = size[0].max(size[1]).max(size[2]);
    if !(longest > 1e-4 && longest.is_finite()) || size.iter().any(|s| s.is_nan() || *s < 0.0) {
        return Err(SdfError::geometry("the sdf's bounds are empty (the shape is nowhere)"));
    }
    if longest > crate::recipe::MAX_EXTENT_M {
        return Err(SdfError::limit(format!("the sdf spans {longest} m; at most {}", crate::recipe::MAX_EXTENT_M), longest as f64, crate::recipe::MAX_EXTENT_M as f64));
    }
    let cell = longest / opt.resolution as f32;
    // One padding cell all round (forced outside), plus one so the bounds' faces are inside.
    let origin = [lo[0] - 1.5 * cell, lo[1] - 1.5 * cell, lo[2] - 1.5 * cell];
    let dims: [usize; 3] = [0, 1, 2].map(|k| (crate::dmath::ceil(size[k] / cell) as usize + 4).max(4));
    let samples = dims[0] * dims[1] * dims[2];
    if samples > MAX_SAMPLES {
        return Err(SdfError::limit(format!("the sdf grid would have {samples} samples; at most {MAX_SAMPLES} (lower the resolution)"), samples as f64, MAX_SAMPLES as f64));
    }
    let prepared = prepare(tree, opt.seed, &mut 0);
    let mut stats = SdfStats { dims: dims.map(|d| d as u32), cell, ..SdfStats::default() };
    let mut work = 0u64;
    let charge = |work: &mut u64, n: u64| -> Result<(), SdfError> {
        *work += n;
        if *work > opt.max_work {
            return Err(SdfError::limit(format!("the sdf needs more than {} work units ({cost} per sample: noise octaves and twists weigh most); lower the resolution or simplify the tree", opt.max_work), *work as f64, opt.max_work as f64));
        }
        if crate::work::cancelled() {
            return Err(SdfError::limit(crate::work::CANCELLED, *work as f64, opt.max_work as f64));
        }
        Ok(())
    };
    let at = |i: usize, j: usize, k: usize| -> V3 { [origin[0] + i as f32 * cell, origin[1] + j as f32 * cell, origin[2] + k as f32 * cell] };
    let idx = |i: usize, j: usize, k: usize| (k * dims[1] + j) * dims[0] + i;
    // Sparse sampling: blocks of B cells whose centre is clear of the surface by their reach.
    const B: usize = 8;
    let blocks = [0, 1, 2].map(|k| (dims[k] - 1).div_ceil(B));
    let mut field = vec![f32::NAN; samples];
    let mut exact = vec![false; samples];
    let half_diag = (3.0f32).sqrt() * 0.5 * B as f32 * cell;
    for bk in 0..blocks[2] {
        for bj in 0..blocks[1] {
            for bi in 0..blocks[0] {
                let (i0, j0, k0) = (bi * B, bj * B, bk * B);
                let (i1, j1, k1) = ((i0 + B).min(dims[0] - 1), (j0 + B).min(dims[1] - 1), (k0 + B).min(dims[2] - 1));
                let c = [origin[0] + (i0 + i1) as f32 * 0.5 * cell, origin[1] + (j0 + j1) as f32 * 0.5 * cell, origin[2] + (k0 + k1) as f32 * 0.5 * cell];
                charge(&mut work, cost)?;
                let dc = eval(&prepared, c);
                if dc.abs() > half_diag * lip + slack + cell {
                    stats.blocks_skipped += 1;
                    for k in k0..=k1 {
                        for j in j0..=j1 {
                            for i in i0..=i1 {
                                let s = idx(i, j, k);
                                if !exact[s] && field[s].is_nan() {
                                    field[s] = dc;
                                }
                            }
                        }
                    }
                    continue;
                }
                charge(&mut work, ((i1 - i0 + 1) * (j1 - j0 + 1) * (k1 - k0 + 1)) as u64 * cost)?;
                for k in k0..=k1 {
                    for j in j0..=j1 {
                        for i in i0..=i1 {
                            let s = idx(i, j, k);
                            if !exact[s] {
                                field[s] = eval(&prepared, at(i, j, k));
                                exact[s] = true;
                                stats.samples_evaluated += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    // The outer layer is outside: the mesh is closed even where the shape meets the bounds.
    for k in 0..dims[2] {
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                if i == 0 || j == 0 || k == 0 || i == dims[0] - 1 || j == dims[1] - 1 || k == dims[2] - 1 {
                    let s = idx(i, j, k);
                    field[s] = field[s].max(cell * 0.5);
                }
            }
        }
    }
    let inside = |s: usize| field[s] < 0.0;
    // Count crossings first: the triangle count is known before anything is built.
    let mut crossings = 0usize;
    for k in 0..dims[2] {
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                let s = idx(i, j, k);
                if i + 1 < dims[0] && inside(s) != inside(idx(i + 1, j, k)) {
                    crossings += 1;
                }
                if j + 1 < dims[1] && inside(s) != inside(idx(i, j + 1, k)) {
                    crossings += 1;
                }
                if k + 1 < dims[2] && inside(s) != inside(idx(i, j, k + 1)) {
                    crossings += 1;
                }
            }
        }
    }
    if crossings == 0 {
        return Err(SdfError::geometry("the sdf has no surface inside its bounds (all inside or all outside)"));
    }
    if 2 * crossings > opt.max_triangles {
        return Err(SdfError::limit(
            format!("the sdf would mesh to {} triangles; at most {} (lower the resolution)", 2 * crossings, opt.max_triangles),
            (2 * crossings) as f64,
            opt.max_triangles as f64,
        ));
    }
    // Each crossing: its point (linear in the two samples) and the field's gradient there.
    charge(&mut work, crossings as u64 * 6 * cost)?;
    let h = cell * 0.05;
    let grad = |p: V3| -> V3 {
        let g = [
            eval(&prepared, [p[0] + h, p[1], p[2]]) - eval(&prepared, [p[0] - h, p[1], p[2]]),
            eval(&prepared, [p[0], p[1] + h, p[2]]) - eval(&prepared, [p[0], p[1] - h, p[2]]),
            eval(&prepared, [p[0], p[1], p[2] + h]) - eval(&prepared, [p[0], p[1], p[2] - h]),
        ];
        math3::normalize(g)
    };
    // Per cell: accumulated QEF (AᵀA, Aᵀb as f64), mass point, count.
    let cells = [dims[0] - 1, dims[1] - 1, dims[2] - 1];
    let cidx = |i: usize, j: usize, k: usize| (k * cells[1] + j) * cells[0] + i;
    #[derive(Clone, Copy, Default)]
    struct Qef {
        ata: [f64; 6],
        atb: [f64; 3],
        mass: [f64; 3],
        n: u32,
    }
    let mut qef: Vec<Qef> = Vec::new();
    let mut cell_slot = vec![u32::MAX; cells[0] * cells[1] * cells[2]];
    let mut add = |qef: &mut Vec<Qef>, ci: usize, p: V3, n: V3| {
        if cell_slot[ci] == u32::MAX {
            qef.push(Qef::default());
            cell_slot[ci] = (qef.len() - 1) as u32;
        }
        let q = &mut qef[cell_slot[ci] as usize];
        let (n, p) = (n.map(|v| v as f64), p.map(|v| v as f64));
        let d = n[0] * p[0] + n[1] * p[1] + n[2] * p[2];
        q.ata[0] += n[0] * n[0];
        q.ata[1] += n[0] * n[1];
        q.ata[2] += n[0] * n[2];
        q.ata[3] += n[1] * n[1];
        q.ata[4] += n[1] * n[2];
        q.ata[5] += n[2] * n[2];
        for c in 0..3 {
            q.atb[c] += n[c] * d;
            q.mass[c] += p[c];
        }
        q.n += 1;
    };
    // Edges (sample, axis), in grid order.
    let mut edges: Vec<(usize, usize, usize, usize, bool)> = Vec::with_capacity(crossings);
    for k in 0..dims[2] {
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                let s = idx(i, j, k);
                for axis in 0..3 {
                    let (i2, j2, k2) = match axis {
                        0 => (i + 1, j, k),
                        1 => (i, j + 1, k),
                        _ => (i, j, k + 1),
                    };
                    if i2 >= dims[0] || j2 >= dims[1] || k2 >= dims[2] {
                        continue;
                    }
                    let s2 = idx(i2, j2, k2);
                    if inside(s) == inside(s2) {
                        continue;
                    }
                    let (d0, d1) = (field[s], field[s2]);
                    let t = if d0 == d1 { 0.5 } else { (d0 / (d0 - d1)).clamp(0.0, 1.0) };
                    let (a, b) = (at(i, j, k), at(i2, j2, k2));
                    let p = math3::lerp(a, b, t);
                    let n = grad(p);
                    // The four cells around this edge.
                    let around: [(isize, isize, isize); 4] = match axis {
                        0 => [(0, -1, -1), (0, 0, -1), (0, 0, 0), (0, -1, 0)],
                        1 => [(-1, 0, -1), (-1, 0, 0), (0, 0, 0), (0, 0, -1)],
                        _ => [(-1, -1, 0), (0, -1, 0), (0, 0, 0), (-1, 0, 0)],
                    };
                    for (di, dj, dk) in around {
                        let (ci, cj, ck) = (i as isize + di, j as isize + dj, k as isize + dk);
                        if ci >= 0 && cj >= 0 && ck >= 0 && (ci as usize) < cells[0] && (cj as usize) < cells[1] && (ck as usize) < cells[2] {
                            add(&mut qef, cidx(ci as usize, cj as usize, ck as usize), p, n);
                        }
                    }
                    edges.push((i, j, k, axis, inside(s)));
                }
            }
        }
    }
    // A vertex per surface cell: the QEF's minimiser, pulled towards the mass point, kept in the
    // cell.
    let mut shape = Shape::default();
    let mut vid = vec![u32::MAX; cell_slot.len()];
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                let ci = cidx(i, j, k);
                if cell_slot[ci] == u32::MAX {
                    continue;
                }
                let q = qef[cell_slot[ci] as usize];
                let n = q.n as f64;
                let mass = q.mass.map(|v| v / n);
                let x = solve_qef(&q.ata, &q.atb, mass, 0.05 * n);
                let lo = at(i, j, k);
                let p: V3 = [0, 1, 2].map(|c| (x[c] as f32).clamp(lo[c], lo[c] + cell));
                vid[ci] = shape.positions.len() as u32;
                shape.positions.push(p);
            }
        }
    }
    for &(i, j, k, axis, inside_low) in &edges {
        let around: [(isize, isize, isize); 4] = match axis {
            0 => [(0, -1, -1), (0, 0, -1), (0, 0, 0), (0, -1, 0)],
            1 => [(-1, 0, -1), (-1, 0, 0), (0, 0, 0), (0, 0, -1)],
            _ => [(-1, -1, 0), (0, -1, 0), (0, 0, 0), (-1, 0, 0)],
        };
        let mut quad = [0u32; 4];
        for (q, (di, dj, dk)) in quad.iter_mut().zip(around) {
            let (ci, cj, ck) = ((i as isize + di) as usize, (j as isize + dj) as usize, (k as isize + dk) as usize);
            *q = vid[cidx(ci, cj, ck)];
        }
        // Counter-clockwise seen from +axis when the low end is inside (the surface faces +axis).
        if !inside_low {
            quad.reverse();
        }
        let p: Vec<V3> = quad.iter().map(|&v| shape.positions[v as usize]).collect();
        // Split along the shorter diagonal.
        let d02 = math3::length(math3::sub(p[2], p[0]));
        let d13 = math3::length(math3::sub(p[3], p[1]));
        if d02 <= d13 {
            shape.tris.push([quad[0], quad[1], quad[2]]);
            shape.tris.push([quad[0], quad[2], quad[3]]);
        } else {
            shape.tris.push([quad[0], quad[1], quad[3]]);
            shape.tris.push([quad[1], quad[2], quad[3]]);
        }
    }
    shape.tags = vec![TAG_FACE; shape.tris.len()];
    // Colours at the vertices.
    charge(&mut work, shape.positions.len() as u64 * 2 * cost)?;
    shape.colors = shape.positions.iter().map(|p| eval_color(&prepared, *p, opt.color).1).collect();
    // Triangles collapsed by vertices clamped onto each other.
    shape.weld_exact();
    stats.work = work;
    stats.triangles = shape.tris.len();
    Ok((shape, stats))
}

/// Minimises |A x - b|² + w |x - mass|² (the normal equations, a 3×3 symmetric solve).
fn solve_qef(ata: &[f64; 6], atb: &[f64; 3], mass: [f64; 3], w: f64) -> [f64; 3] {
    let m = [[ata[0] + w, ata[1], ata[2]], [ata[1], ata[3] + w, ata[4]], [ata[2], ata[4], ata[5] + w]];
    let r = [atb[0] + w * mass[0], atb[1] + w * mass[1], atb[2] + w * mass[2]];
    let det = |m: &[[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = det(&m);
    if d.abs() < 1e-18 {
        return mass;
    }
    let mut x = [0.0; 3];
    for (c, slot) in x.iter_mut().enumerate() {
        let mut mc = m;
        for row in 0..3 {
            mc[row][c] = r[row];
        }
        *slot = det(&mc) / d;
    }
    if x.iter().all(|v| v.is_finite()) {
        x
    } else {
        mass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prim(p: Prim) -> SdfNode {
        SdfNode { kind: Kind::Prim(p), m: Modifiers::default() }
    }

    /// Directed edges not matched by as many reverse edges (0 = closed; dual contouring may
    /// leave an edge shared by four faces where two sheets touch, which still counts closed).
    fn unbalanced(s: &Shape) -> usize {
        let mut w = s.clone();
        w.weld_exact();
        let mut e = std::collections::BTreeMap::new();
        for t in &w.tris {
            for k in 0..3 {
                *e.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        e.iter().filter(|(&(a, b), &n)| e.get(&(b, a)).copied().unwrap_or(0) != n).count()
    }

    fn opts(res: u32) -> MeshOptions {
        MeshOptions { resolution: res, bounds: None, seed: 1, max_triangles: 200_000, max_work: MAX_WORK, color: [1.0; 4] }
    }

    /// Prints ns per evaluation of each node kind (release: `cargo test --release -p
    /// specula-model calibrate_sdf -- --ignored --nocapture`); the weights in `node_cost` come
    /// from it.
    #[test]
    #[ignore]
    #[allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]
    fn calibrate_sdf() {
        let time = |name: &str, n: &SdfNode| {
            let p = prepare(n, 1, &mut 0);
            let t = std::time::Instant::now();
            let mut acc = 0.0f32;
            let k = 400_000;
            for i in 0..k {
                let x = (i % 97) as f32 * 0.013;
                acc += eval(&p, [x, x * 0.7, 0.3 - x]);
            }
            let ns = t.elapsed().as_nanos() as f64 / k as f64;
            println!("{name:<28} {ns:7.1} ns  (cost {})  {acc}", eval_cost(n));
        };
        let with = |p: Prim, m: Modifiers| SdfNode { kind: Kind::Prim(p), m };
        let sph = Prim::Sphere { radius: 1.0 };
        time("sphere", &prim(sph));
        time("box", &prim(Prim::Box { half: [1.0; 3], round: 0.1 }));
        time("cylinder", &prim(Prim::Cylinder { radius: 1.0, radius_top: 1.0, half_height: 1.0, round: 0.0 }));
        time("cone", &prim(Prim::Cylinder { radius: 1.0, radius_top: 0.5, half_height: 1.0, round: 0.0 }));
        time("capsule", &prim(Prim::Capsule { radius: 1.0, half_height: 1.0 }));
        time("torus", &prim(Prim::Torus { radius: 1.0, tube: 0.2 }));
        time("ellipsoid", &prim(Prim::Ellipsoid { radii: [1.0, 0.5, 0.7] }));
        time("halfspace", &prim(Prim::HalfSpace { normal: [0.0, 1.0, 0.0], offset: 0.0 }));
        time("sphere rotated", &with(sph, Modifiers { rotate: [10.0, 20.0, 30.0], ..Modifiers::default() }));
        time("sphere repeat", &with(sph, Modifiers { repeat: Some(([3, 3, 3], [2.0; 3])), ..Modifiers::default() }));
        time("sphere twist", &with(sph, Modifiers { twist: 30.0, ..Modifiers::default() }));
        time("sphere bend", &with(sph, Modifiers { bend: 30.0, ..Modifiers::default() }));
        for o in [1, 3, 6, 8] {
            time(&format!("sphere displace x{o}"), &with(sph, Modifiers { displace: Some((0.1, 0.5, o)), ..Modifiers::default() }));
        }
        let kids = |k: usize| (0..k).map(|_| prim(sph)).collect::<Vec<_>>();
        time("union of 8 spheres", &SdfNode { kind: Kind::Group { op: Combine::Union, kids: kids(8), smooth: 0.0 }, m: Modifiers::default() });
        time("smooth union of 8", &SdfNode { kind: Kind::Group { op: Combine::Union, kids: kids(8), smooth: 0.2 }, m: Modifiers::default() });
    }

    #[test]
    fn sphere_meshes_closed_with_the_right_volume() {
        let (s, st) = mesh(&prim(Prim::Sphere { radius: 1.0 }), &opts(32)).unwrap();
        assert_eq!(s.open_edge_count(), 0);
        let v = s.signed_volume();
        let exact = 4.0 / 3.0 * std::f32::consts::PI;
        assert!((v - exact).abs() < 0.03 * exact, "{v} vs {exact}");
        assert!(st.blocks_skipped > 0, "{st:?}");
        // Every vertex is near the sphere.
        assert!(s.positions.iter().all(|p| (math3::length(*p) - 1.0).abs() < 0.07));
        s.finalize(60.0, "m").validate().unwrap();
    }

    #[test]
    fn boxes_keep_sharp_corners() {
        let b = prim(Prim::Box { half: [1.0, 0.5, 0.75], round: 0.0 });
        let (s, _) = mesh(&b, &opts(24)).unwrap();
        let bb = s.bounds();
        // Dual contouring puts vertices on the faces: the corners are reproduced closely.
        for k in 0..3 {
            assert!((bb.max[k] - [1.0, 0.5, 0.75][k]).abs() < 0.02, "{:?}", bb);
        }
        assert!((s.signed_volume() - 1.5 * 2.0).abs() < 0.05, "{}", s.signed_volume());
    }

    #[test]
    fn smooth_union_blends_and_colours_mix() {
        let mut a = prim(Prim::Sphere { radius: 0.6 });
        a.m.at = [-0.5, 0.0, 0.0];
        a.m.color = Some([1.0, 0.0, 0.0, 1.0]);
        let mut b = prim(Prim::Sphere { radius: 0.6 });
        b.m.at = [0.5, 0.0, 0.0];
        b.m.color = Some([0.0, 0.0, 1.0, 1.0]);
        let sharp = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![a.clone(), b.clone()], smooth: 0.0 }, m: Modifiers::default() };
        let smooth = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![a, b], smooth: 0.4 }, m: Modifiers::default() };
        let (s1, _) = mesh(&sharp, &opts(40)).unwrap();
        let (s2, _) = mesh(&smooth, &opts(40)).unwrap();
        assert!(s2.signed_volume() > s1.signed_volume() + 0.01);
        assert_eq!(unbalanced(&s2), 0);
        // Red on the left, blue on the right, a mix in the middle.
        let left = s2.positions.iter().zip(&s2.colors).find(|(p, _)| p[0] < -1.0).unwrap().1;
        let right = s2.positions.iter().zip(&s2.colors).find(|(p, _)| p[0] > 1.0).unwrap().1;
        assert!(left[0] > 0.9 && right[2] > 0.9);
        assert!(s2.positions.iter().zip(&s2.colors).any(|(p, c)| p[0].abs() < 0.1 && c[0] > 0.2 && c[2] > 0.2));
    }

    #[test]
    fn difference_and_modifiers() {
        let rock = SdfNode {
            kind: Kind::Group {
                op: Combine::Difference,
                kids: vec![prim(Prim::Box { half: [1.0; 3], round: 0.1 }), {
                    let mut c = prim(Prim::Cylinder { radius: 0.4, radius_top: 0.4, half_height: 2.0, round: 0.0 });
                    c.m.rotate = [90.0, 0.0, 0.0];
                    c
                }],
                smooth: 0.05,
            },
            m: Modifiers { displace: Some((0.03, 0.3, 3)), twist: 10.0, ..Modifiers::default() },
        };
        let (s, st) = mesh(&rock, &opts(48)).unwrap();
        assert_eq!(unbalanced(&s), 0, "{st:?}");
        s.finalize(45.0, "m").validate().unwrap();
        let again = mesh(&rock, &opts(48)).unwrap().0;
        assert_eq!(s, again, "deterministic");
        // A repeat makes copies.
        let mut row = prim(Prim::Sphere { radius: 0.3 });
        row.m.repeat = Some(([4, 1, 1], [1.0, 0.0, 0.0]));
        let (r, _) = mesh(&row, &opts(64)).unwrap();
        let v1 = mesh(&prim(Prim::Sphere { radius: 0.3 }), &opts(16)).unwrap().0.signed_volume();
        assert!((r.signed_volume() - 4.0 * v1).abs() < 0.15 * 4.0 * v1, "{} vs {}", r.signed_volume(), 4.0 * v1);
    }

    #[test]
    fn limits_and_errors() {
        let s = prim(Prim::Sphere { radius: 1.0 });
        assert!(mesh(&s, &MeshOptions { max_triangles: 50, ..opts(32) }).unwrap_err().limit);
        assert!(mesh(&s, &MeshOptions { max_work: 1000, ..opts(32) }).unwrap_err().limit);
        assert!(mesh(&s, &opts(MAX_RESOLUTION + 1)).unwrap_err().limit);
        let half = prim(Prim::HalfSpace { normal: [0.0, 1.0, 0.0], offset: 0.0 });
        assert!(!mesh(&half, &opts(16)).unwrap_err().limit);
        let ground = mesh(&half, &MeshOptions { bounds: Some(([-1.0; 3], [1.0; 3])), ..opts(16) }).unwrap().0;
        assert_eq!(ground.open_edge_count(), 0, "closed at the bounds");
    }
}
