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
/// Most points in one chain (a chain is a union of round cones, one fewer than its points).
pub const MAX_CHAIN: usize = 48;

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
    /// A round cone: every ball between one of radius `ra` at `a` and one of radius `rb` at
    /// `b` (a limb, a finger, a horn, one link of a chain). Unlike the other primitives it is
    /// given by its two ends, in the node's frame.
    RoundCone { a: V3, b: V3, ra: f32, rb: f32 },
    /// A column of sections along the line from `a` to `b`: at each of its first `count`
    /// `sections` (`[t, breadth, depth]`: how far along, 0 to 1 and rising, and the half
    /// breadth toward `side` and the half depth across it) the surface is that ellipse, and
    /// between two sections the breadth and the depth follow a curve through them that keeps
    /// going the way the sizes go: level at a section only where they turn, never outside the
    /// two sections it is between. A
    /// rounded end closes it past each end. A trunk, a limb with its swell, a neck, a vase.
    /// Given by its ends, in the node's frame; its field is a lower bound of the distance.
    Column { a: V3, b: V3, side: V3, sections: [[f32; 3]; MAX_SECTIONS], count: u8 },
}

/// Most sections in one column.
pub const MAX_SECTIONS: usize = 8;

/// How children combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    Union,
    /// The first child minus every other.
    Difference,
    Intersection,
    /// A union in which only the children within the smoothing of the nearest one take part
    /// (in their order, blended as a union's are). What is further away cannot change the
    /// surface, so a tree of many parts costs what is near each point and no more: a chain's
    /// links, a figure's limbs. (A plain smooth union blends its children one after another,
    /// so that three far ones can, together, reach a near one: this one cannot.)
    Near,
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
    /// A scale of its own for each of the node's axes, about its `at`: a trunk as broad and
    /// as deep as it is measured, a palm flatter than it is wide. The surface is exact; the
    /// field away from it is a lower bound of the distance (the true one is never less).
    pub stretch: V3,
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
            stretch: [1.0; 3],
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
    /// How much faster than distance the node's field may change, and how far smoothing and
    /// displacement may move it, in its parent's frame ([`lip_slack`]).
    lip: f32,
    slack: f32,
    /// Neither this node nor anything under it is repeated, elongated, twisted, bent, shelled
    /// or displaced: its distance at a ball's middle bounds its distance through the ball.
    steady: bool,
    /// A column whose breadth and depth change in different proportions (`0.0` for anything
    /// else): how much faster than its factor its field may change along it, for every one of
    /// its own radii a point is out from its axis ([`column_wander`]). A group has the greatest
    /// of its children's.
    wander: f32,
    /// The least radius of such a column (of the least such column under a group), in the
    /// node's parent's frame: what "one of its own radii" is.
    least: f32,
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
    /// Pruned trees kept for the crossings' normals (one a half block that was sampled).
    pub trees_kept: usize,
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
    let (lip, slack) = lip_slack(n);
    let m = &n.m;
    let (wander, least) = match &kind {
        PKind::Prim(Prim::Column { a, b, sections, count, .. }) => {
            let used = &sections[..(*count as usize).clamp(1, MAX_SECTIONS)];
            (column_wander(*a, *b, used), used.iter().map(|row| row[1].min(row[2])).fold(f32::MAX, f32::min) * m.scale)
        }
        PKind::Prim(_) => (0.0, f32::MAX),
        PKind::Group { kids, .. } => {
            let wander = kids.iter().map(|k| k.wander).fold(0.0f32, f32::max);
            let least = kids.iter().filter(|k| k.wander > 0.0).map(|k| k.least).fold(f32::MAX, f32::min);
            (wander, if wander > 0.0 { least * m.scale } else { f32::MAX })
        }
    };
    let steady = m.repeat.is_none()
        && m.elongate == [0.0; 3]
        && m.twist == 0.0
        && m.bend == 0.0
        && m.shell <= 0.0
        && m.displace.is_none()
        && match &kind {
            PKind::Prim(_) => true,
            PKind::Group { kids, .. } => kids.iter().all(|k| k.steady),
        }
        // A column whose breadth and depth change in different proportions (or a group with
        // one in it) is steady only unstretched: `prune` then allows for its wander.
        && (wander == 0.0 || m.stretch == [1.0; 3]);
    Prepared { kind, m: n.m.clone(), inv_rot, rotated: n.m.rotate != [0.0; 3], seed: my_seed, lip, slack, steady, wander, least }
}

/// The tree as it is for every point within `reach` of `c` (both in the node's parent's
/// frame): a union's children that are too far to touch the surface or its smoothing there
/// are left out. **Exactly**: at every such point the pruned tree's distance is the whole
/// tree's, bit for bit. Its colour is too through near-unions; through a plain smooth union it
/// may differ in the last place (a blend that reaches its end, `a + (b - a)`, is not always
/// `b`), which nothing shows: a mesh's colours are asked of the whole tree.
///
/// From a plain union a child may go when, everywhere in the ball, it is further than every child that stays by
/// more than the smoothing reaches, with room for the children that go to have blended among
/// themselves first (a quarter of the smoothing each). Then it never changes what the union
/// has accumulated: before the first child that stays, the ones that go cannot pull the
/// accumulated distance to within the smoothing of it; after it, none of them is near enough.
/// Only plain unions are pruned (a repeat, a twist, a bend, an elongation, a shell or a
/// displacement keeps every child, as its distances no longer bound each other), and a child
/// with any of those anywhere under it is never dropped.
fn prune(n: &Prepared, c: V3, reach: f32) -> Prepared {
    let keep_all = |kind: PKind| Prepared { kind, m: n.m.clone(), inv_rot: n.inv_rot, rotated: n.rotated, seed: n.seed, lip: n.lip, slack: n.slack, steady: n.steady, wander: n.wander, least: n.least };
    let PKind::Group { op, kids, smooth } = &n.kind else { return n.clone() };
    let m = &n.m;
    let plain = m.repeat.is_none() && m.elongate == [0.0; 3] && m.twist == 0.0 && m.bend == 0.0 && m.shell <= 0.0 && m.displace.is_none();
    if !plain {
        return n.clone();
    }
    let q = local(n, c);
    let r = reach / (m.scale * m.stretch[0].min(m.stretch[1]).min(m.stretch[2]));
    // A cut or an intersection keeps every child, each pruned for the same ball.
    if !matches!(op, Combine::Union | Combine::Near) {
        return keep_all(PKind::Group { op: *op, kids: kids.iter().map(|kid| prune(kid, q, r)).collect(), smooth: *smooth });
    }
    // A smooth plain union with a child that is not steady keeps every child: such a child
    // stays whatever it says, and blending in turn it could carry the pull of a child that
    // went (three spheres, the middle one twisted: the pruned tree differed in the fifth place).
    if *op == Combine::Union && *smooth > 0.0 && kids.iter().any(|k| !k.steady) {
        return keep_all(PKind::Group { op: *op, kids: kids.iter().map(|kid| prune(kid, q, r)).collect(), smooth: *smooth });
    }
    // Each child's least and greatest distance anywhere in the ball.
    let span: Vec<(f32, f32)> = kids
        .iter()
        .map(|k| {
            let d = eval(k, q);
            if k.wander == 0.0 {
                let spread = k.lip * r + k.slack;
                return (d - spread, d + spread);
            }
            // A column of changing proportions, or a group with one in it: its field changes
            // faster the further out of the column a point is, by its wander for each of the
            // column's own radii out. A point of the ball is at most this many radii out (the
            // column is no nearer than the child's own distance says: under a union the column
            // may be further than that, and its allowance then grows more slowly than its
            // distance, as long as the ball is small beside the column, which is asked here).
            if k.wander * r >= 0.9 * k.least {
                // A ball too large beside the column to say anything: stays, and is no reason
                // for another to go.
                return (f32::MIN, f32::MAX);
            }
            let out = (d.max(0.0) + r) / k.least + 1.0;
            let spread = (k.lip + k.wander * out) * r + k.slack;
            (d - spread, d + spread)
        })
        .collect();
    if span.iter().any(|(lo, hi)| !lo.is_finite() || !hi.is_finite()) {
        return n.clone();
    }
    // A child that is repeated, twisted, bent, shelled or displaced somewhere under it stays,
    // and is no reason for another to go: its field jumps (a repeat's at its cells' edges), so
    // its distance at the middle says nothing sure of the rest of the ball. (Dropped by it, a
    // repeated round cone set off its cell's middle left samples on the wrong side: review of
    // part 4's slice 2.)
    let nearest = span.iter().zip(kids).filter(|(_, k)| k.steady).map(|(s, _)| s.1).fold(f32::MAX, f32::min);
    let margin = smooth.max(0.0) * (1.0 + kids.len() as f32 / 4.0);
    let mut keep: Vec<bool> = span.iter().zip(kids).map(|(s, k)| !k.steady || s.0 <= nearest + smooth.max(0.0)).collect();
    // A near-union's children that are not within the smoothing of the nearest anywhere in
    // the ball take no part there, by its definition: nothing more to keep.
    let plain_union = *op == Combine::Union;
    loop {
        if !plain_union {
            break;
        }
        let far = keep.iter().zip(&span).zip(kids).filter(|((k, _), kid)| **k && kid.steady).map(|((_, s), _)| s.1).fold(f32::MIN, f32::max);
        let mut grew = false;
        for (k, s) in keep.iter_mut().zip(&span) {
            if !*k && s.0 <= far + margin {
                *k = true;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let kids = kids.iter().zip(&keep).filter(|(_, k)| **k).map(|(kid, _)| prune(kid, q, r)).collect();
    keep_all(PKind::Group { op: *op, kids, smooth: *smooth })
}

fn has_color(n: &Prepared) -> bool {
    n.m.color.is_some() || matches!(&n.kind, PKind::Group { kids, .. } if kids.iter().any(has_color))
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
        Prim::RoundCone { a, b, ra, rb } => round_cone(q, a, b, ra, rb),
        Prim::Column { a, b, side, sections, count } => column(q, a, b, side, &sections[..(count as usize).clamp(1, MAX_SECTIONS)]),
    }
}

/// A column's half breadth and half depth at `at` along it (within its sections' range).
fn column_sizes(sections: &[[f32; 3]], at: f32) -> (f32, f32) {
    // The section at t: on a curve through the sections that keeps going the way they go (a
    // monotone cubic). It is level at a section only where the sizes turn there, so a limb
    // that tapers through three sections tapers all the way and shows no band at the middle
    // one; and it never goes outside the sizes of the two sections it is between.
    let last = sections[sections.len() - 1];
    let (mut rx, mut rz) = (last[1], last[2]);
    if sections.len() > 1 {
        let mut i = 0;
        while i + 2 < sections.len() && at > sections[i + 1][0] {
            i += 1;
        }
        let span = (sections[i + 1][0] - sections[i][0]).max(1e-6);
        let f = ((at - sections[i][0]) / span).clamp(0.0, 1.0);
        let (f2, f3) = (f * f, f * f * f);
        let size = |c: usize| {
            let rise = |j: usize| (sections[j + 1][c] - sections[j][c]) / (sections[j + 1][0] - sections[j][0]).max(1e-6);
            let slope = |j: usize| {
                if j == 0 {
                    rise(0)
                } else if j + 1 == sections.len() {
                    rise(j - 1)
                } else {
                    let (before, after) = (rise(j - 1), rise(j));
                    if before * after <= 0.0 {
                        0.0
                    } else {
                        2.0 * before * after / (before + after)
                    }
                }
            };
            (2.0 * f3 - 3.0 * f2 + 1.0) * sections[i][c] + (f3 - 2.0 * f2 + f) * span * slope(i) + (3.0 * f2 - 2.0 * f3) * sections[i + 1][c] + (f3 - f2) * span * slope(i + 1)
        };
        (rx, rz) = (size(1), size(2));
    }
    (rx, rz)
}

/// How much faster a column's field may change along it for each of its own radii a point is
/// out from its axis, beyond what [`column_lip`] allows. A round column's, or one whose
/// breadth and depth keep one proportion, changes along it by its growth alone, wherever the
/// point is (0 here). Where the proportions change, the field at a point far out to one side
/// follows that side's radius, and changes by the difference of the two rates of growth times
/// how many radii out the point is: a wedge 5 by 4 cm growing to 20 by 4 cm over 20 cm changes
/// 9 metres a metre at 3 m out, against a factor of 1.4 (second review of part 4's slice 2).
/// The steepest such difference along the column, in units of the least radius, a quarter to
/// spare.
fn column_wander(a: V3, b: V3, sections: &[[f32; 3]]) -> f32 {
    let length = math3::length(math3::sub(b, a)).max(1e-6);
    let mut worst = 0.0f32;
    for pair in sections.windows(2) {
        let span = (pair[1][0] - pair[0][0]).max(1e-6);
        let mut before = column_sizes(sections, pair[0][0]);
        for k in 1..=24 {
            let now = column_sizes(sections, pair[0][0] + span * k as f32 / 24.0);
            let step = span / 24.0 * length;
            // The rates at which the two radii grow, each as a share of itself.
            let (wide, deep) = ((now.0 - before.0) / before.0.min(now.0), (now.1 - before.1) / before.1.min(now.1));
            worst = worst.max((wide - deep).abs() / step * before.0.min(before.1).min(now.0).min(now.1));
            before = now;
        }
    }
    if worst < 1e-5 {
        0.0
    } else {
        worst * 1.25
    }
}

/// How much faster than distance a column's field may change: its field is how far out of the
/// section at its own height a point is, and where the sections grow by `s` metres a metre
/// along the column, the nearest surface may be at another height, `sqrt(1 + s * s)` times
/// nearer. (With one fixed factor of 2, a column that widened faster than about 1.7 metres a
/// metre, a flange on a thin stem, was meshed with samples on the wrong side of its surface.)
/// The steepest growth is taken from the curve itself, with a quarter to spare.
fn column_lip(a: V3, b: V3, sections: &[[f32; 3]]) -> f32 {
    let length = math3::length(math3::sub(b, a)).max(1e-6);
    let mut steepest = 0.0f32;
    for pair in sections.windows(2) {
        let span = (pair[1][0] - pair[0][0]).max(1e-6);
        let mut before = column_sizes(sections, pair[0][0]);
        for k in 1..=24 {
            let now = column_sizes(sections, pair[0][0] + span * k as f32 / 24.0);
            let grew = (now.0 - before.0).abs().max((now.1 - before.1).abs());
            steepest = steepest.max(grew / (span / 24.0 * length));
            before = now;
        }
    }
    let steepest = steepest * 1.25;
    math3::length([1.0, steepest, 0.0])
}

/// A column's field ([`Prim::Column`]): how many of its own radii out a point is from the
/// axis, less one, times the smaller radius there (so it is never more than the distance).
fn column(p: V3, a: V3, b: V3, side: V3, sections: &[[f32; 3]]) -> f32 {
    let axis = math3::sub(b, a);
    let length = math3::length(axis).max(1e-6);
    let up = math3::scale(axis, 1.0 / length);
    // Toward `side`, squared to the axis; and across both.
    let lean = math3::sub(side, math3::scale(up, math3::dot(side, up)));
    let right = if math3::length(lean) > 1e-6 { math3::normalize(lean) } else { math3::perpendicular(up) };
    let across = math3::cross(up, right);
    let from = math3::sub(p, a);
    let (t, x, z) = (math3::dot(from, up) / length, math3::dot(from, right), math3::dot(from, across));
    let (first, last) = (sections[0], sections[sections.len() - 1]);
    let at = t.clamp(first[0], last[0]);
    let (rx, rz) = column_sizes(sections, at);
    // Past an end: a rounded cap as high as the section there is narrow.
    let beyond = (t - at).abs() * length;
    let cap = rx.min(rz);
    let k = ((x / rx) * (x / rx) + (z / rz) * (z / rz) + (beyond / cap) * (beyond / cap)).sqrt();
    (k - 1.0) * cap
}

/// Inigo Quilez's round cone between two points: the exact distance to the hull of two balls.
/// When one ball holds the other (or the ends meet) it is the larger ball.
fn round_cone(p: V3, a: V3, b: V3, ra: f32, rb: f32) -> f32 {
    let ba = math3::sub(b, a);
    let l2 = math3::dot(ba, ba);
    let rr = ra - rb;
    let a2 = l2 - rr * rr;
    if a2 <= 0.0 {
        return if ra >= rb { math3::length(math3::sub(p, a)) - ra } else { math3::length(math3::sub(p, b)) - rb };
    }
    let il2 = 1.0 / l2;
    let pa = math3::sub(p, a);
    let y = math3::dot(pa, ba);
    let z = y - l2;
    let across = math3::sub(math3::scale(pa, l2), math3::scale(ba, y));
    let x2 = math3::dot(across, across);
    let (y2, z2) = (y * y * l2, z * z * l2);
    let sign = |v: f32| if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 };
    let k = sign(rr) * rr * rr * x2;
    if sign(z) * a2 * z2 > k {
        (x2 + z2).sqrt() * il2 - rb
    } else if sign(y) * a2 * y2 < k {
        (x2 + y2).sqrt() * il2 - ra
    } else {
        ((x2 * a2 * il2).sqrt() + y * rr) * il2 - ra
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
    if m.stretch != [1.0; 3] {
        q = [q[0] / m.stretch[0], q[1] / m.stretch[1], q[2] / m.stretch[2]];
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
    // Stretched space: a step there is at least the narrowest stretch of a step here.
    d *= m.scale * m.stretch[0].min(m.stretch[1]).min(m.stretch[2]);
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

/// A [`Combine::Near`] group at a point: every child's distance (and colour), then the
/// children within the smoothing of the nearest, blended in their order.
fn near(kids: &[Prepared], smooth: f32, of: impl Fn(&Prepared) -> (f32, Rgba)) -> (f32, Rgba) {
    let mut few = [(0.0f32, [0.0f32; 4]); 16];
    let mut many = Vec::new();
    let found: &mut [(f32, Rgba)] = if kids.len() <= few.len() {
        &mut few[..kids.len()]
    } else {
        many.resize(kids.len(), (0.0, [0.0; 4]));
        &mut many
    };
    let mut nearest = f32::MAX;
    for (slot, kid) in found.iter_mut().zip(kids) {
        *slot = of(kid);
        nearest = nearest.min(slot.0);
    }
    let reach = nearest + smooth.max(0.0);
    let mut taking = found.iter().filter(|(d, _)| *d <= reach);
    let (mut acc, mut col) = taking.next().copied().unwrap_or((f32::MAX, [0.0; 4]));
    for (d, c) in taking {
        let (blended, w) = smin(acc, *d, smooth);
        col = mix(col, *c, w);
        acc = blended;
    }
    (acc, col)
}

fn eval(n: &Prepared, p: V3) -> f32 {
    let q = local(n, p);
    let d = match &n.kind {
        PKind::Prim(pr) => prim_distance(pr, q),
        PKind::Group { op: Combine::Near, kids, smooth } => near(kids, *smooth, |k| (eval(k, q), [0.0; 4])).0,
        PKind::Group { op, kids, smooth } => {
            let mut it = kids.iter();
            let mut acc = it.next().map_or(f32::MAX, |k| eval(k, q));
            for k in it {
                let b = eval(k, q);
                acc = match op {
                    Combine::Union | Combine::Near => smin(acc, b, *smooth).0,
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
        PKind::Group { op: Combine::Near, kids, smooth } => {
            if kids.is_empty() {
                (f32::MAX, mine)
            } else {
                near(kids, *smooth, |k| eval_color(k, q, mine))
            }
        }
        PKind::Group { op, kids, smooth } => {
            let mut it = kids.iter();
            let (mut acc, mut col) = it.next().map_or((f32::MAX, mine), |k| eval_color(k, q, mine));
            for k in it {
                let (b, cb) = eval_color(k, q, mine);
                match op {
                    Combine::Union | Combine::Near => {
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
    match &n.kind {
        Kind::Prim(p) => prim_cost(p) + modifiers_cost(&n.m),
        Kind::Group { kids, smooth, .. } => group_cost(kids.iter().map(eval_cost).sum(), kids.len(), *smooth) + modifiers_cost(&n.m),
    }
}

/// [`eval_cost`] of a prepared tree (a pruned one costs what is left of it).
/// A tree's field at a point (negative inside), for tests that ask where a surface is.
#[cfg(test)]
pub(crate) fn field_at(n: &SdfNode, p: V3) -> f32 {
    eval(&prepare(n, 1, &mut 0), p)
}

fn prepared_cost(n: &Prepared) -> u64 {
    match &n.kind {
        PKind::Prim(p) => prim_cost(p) + modifiers_cost(&n.m),
        PKind::Group { kids, smooth, .. } => group_cost(kids.iter().map(prepared_cost).sum(), kids.len(), *smooth) + modifiers_cost(&n.m),
    }
}

// The weights. First set on 2026-10-22 from a sphere of about 70 ns with its frame and
// modifiers, weighed half as much again for the mesher's own work per sample. Measured again
// on 2026-11-17 on the same reference laptop, three times in release with `calibrate_sdf`: a
// sphere 23, 29 and 34 ns (the weight said 107), a box 25 to 40, a round cone 37 to 48, a
// column of four sections 45 to 50, a twist 35 to 48, a displacement of eight octaves 208 to
// 260, a smooth union of eight spheres 251 to 309, a chain of 24 round cones 1,044 to 1,285.
// Every weight below is half what it was, which is still at least one and a half times the
// slowest of those measurements (the rule the first weights were set by).
fn prim_cost(p: &Prim) -> u64 {
    48 + match *p {
        Prim::HalfSpace { .. } => 0,
        Prim::Sphere { .. } | Prim::Capsule { .. } => 6,
        Prim::RoundCone { .. } => 26,
        Prim::Column { .. } => 38,
        Prim::Box { .. } | Prim::Torus { .. } | Prim::Ellipsoid { .. } => 12,
        Prim::Cylinder { radius, radius_top, .. } if radius == radius_top => 12,
        Prim::Cylinder { .. } => 35,
    }
}

fn group_cost(kids: u64, count: usize, smooth: f32) -> u64 {
    8 + kids + count as u64 * if smooth > 0.0 { 20 } else { 3 }
}

fn modifiers_cost(m: &Modifiers) -> u64 {
    let mut c = 0;
    if m.rotate != [0.0; 3] {
        c += 6;
    }
    if m.stretch != [1.0; 3] {
        c += 3;
    }
    if m.repeat.is_some() {
        c += 38;
    }
    if m.twist != 0.0 {
        c += 65;
    }
    if m.bend != 0.0 {
        c += 23;
    }
    if let Some((_, _, octaves)) = m.displace {
        c += 15 + 45 * octaves.max(1) as u64;
    }
    c
}

/// A node's Lipschitz factor (how much faster than distance its field may change) and slack
/// (how far smoothing and displacement may move it), in its parent's frame. Unlike [`bounds`]
/// this folds in every child, bounded or not: an unbounded cutter's displacement still counts.
/// How much faster than distance a primitive's field may change (1 for an exact distance):
/// an ellipsoid's is only a bound and is allowed twice the speed; a column's goes by its
/// steepest section ([`column_lip`]).
fn prim_lip(p: &Prim) -> f32 {
    match p {
        Prim::Ellipsoid { .. } => 2.0,
        Prim::Column { a, b, sections, count, .. } => column_lip(*a, *b, &sections[..(*count as usize).clamp(1, MAX_SECTIONS)]),
        _ => 1.0,
    }
}

fn lip_slack(n: &SdfNode) -> (f32, f32) {
    let m = &n.m;
    let (mut lip, mut slack) = match &n.kind {
        Kind::Prim(p) => (prim_lip(p), 0.0),
        Kind::Group { kids, smooth, .. } => kids.iter().map(lip_slack).fold((1.0f32, smooth.max(0.0)), |(l, s), (kl, ks)| (l.max(kl), s.max(ks))),
    };
    slack *= m.scale * m.stretch[0].max(m.stretch[1]).max(m.stretch[2]);
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
            let centred = |e: V3| ([-e[0], -e[1], -e[2]], e);
            let (lo, hi) = match *p {
                Prim::Sphere { radius } => centred([radius; 3]),
                Prim::Box { half, .. } => centred(half),
                Prim::Cylinder { radius, radius_top, half_height, .. } => centred([radius.max(radius_top), half_height, radius.max(radius_top)]),
                Prim::Capsule { radius, half_height } => centred([radius, half_height + radius, radius]),
                Prim::Torus { radius, tube } => centred([radius + tube, tube, radius + tube]),
                Prim::Ellipsoid { radii } => centred(radii),
                Prim::HalfSpace { .. } => return None,
                // Not centred: the box around its two balls.
                Prim::RoundCone { a, b, ra, rb } => (math3::vmin(math3::sub(a, [ra; 3]), math3::sub(b, [rb; 3])), math3::vmax(math3::add(a, [ra; 3]), math3::add(b, [rb; 3]))),
                // The box around its ends, as wide all round as its widest section.
                Prim::Column { a, b, sections, count, .. } => {
                    let used = &sections[..(count as usize).clamp(1, MAX_SECTIONS)];
                    let widest = used.iter().map(|s| s[1].max(s[2])).fold(0.0f32, f32::max);
                    // (Its rounded ends reach no further past its first and last sections than those are wide.)
                    (math3::sub(math3::vmin(a, b), [widest; 3]), math3::add(math3::vmax(a, b), [widest; 3]))
                }
            };
            let lip = prim_lip(p);
            (lo, hi, lip, 0.0)
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
                        if first || matches!(op, Combine::Union | Combine::Near) {
                            lo = math3::vmin(lo, l);
                            hi = math3::vmax(hi, h);
                        } else if *op == Combine::Intersection {
                            lo = math3::vmax(lo, l);
                            hi = math3::vmin(hi, h);
                        }
                    }
                    // An unbounded child: the union is unbounded; it doesn't widen a cut or an
                    // intersection's other bounds.
                    (Combine::Union | Combine::Near, None) => return None,
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
    slack *= m.scale * m.stretch[0].max(m.stretch[1]).max(m.stretch[2]);
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
    let (lo_s, hi_s) = (math3::mul(math3::scale(lo, s), m.stretch), math3::mul(math3::scale(hi, s), m.stretch));
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
    // What can matter in each small box of samples ([`prune`]: a block's tree, then a half
    // block's from that, then a quarter block's from that), and what one evaluation of it
    // costs: its samples, and later its crossings' normals, leave the rest of the tree out.
    // Kept by quarter block (two cells a side).
    const FINE: usize = 2;
    let fine = [0, 1, 2].map(|k| dims[k].div_ceil(FINE));
    let mut near_at = vec![u32::MAX; fine[0] * fine[1] * fine[2]];
    let mut near: Vec<(Prepared, u64)> = Vec::new();
    // The middle of a box of samples and how far its corners are from it (a hair more).
    let ball = |lo: [usize; 3], hi: [usize; 3]| -> (V3, f32) {
        let middle = [0, 1, 2].map(|k| origin[k] + (lo[k] + hi[k]) as f32 * 0.5 * cell);
        let half = [0, 1, 2].map(|k| (hi[k] - lo[k]) as f32 * 0.5 * cell);
        (middle, math3::length(half) * 1.001 + 0.25 * cell)
    };
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
                // (Finding what is near costs one more evaluation of the tree it is found in.)
                charge(&mut work, cost)?;
                let here = prune(&prepared, c, half_diag + 0.25 * cell);
                let here_cost = prepared_cost(&here);
                // Boxes of samples from `from` in steps of `step`, the last ending at `to`.
                let boxes = |from: [usize; 3], to: [usize; 3], step: usize| {
                    let along = |k: usize| (from[k]..to[k].max(from[k] + 1)).step_by(step).map(move |a| (a, (a + step).min(to[k]))).collect::<Vec<_>>();
                    let (xs, ys, zs) = (along(0), along(1), along(2));
                    let mut out = Vec::with_capacity(xs.len() * ys.len() * zs.len());
                    for z in &zs {
                        for y in &ys {
                            for x in &xs {
                                out.push(([x.0, y.0, z.0], [x.1, y.1, z.1]));
                            }
                        }
                    }
                    out
                };
                for (half_lo, half_hi) in boxes([i0, j0, k0], [i1, j1, k1], B / 2) {
                    let (middle, reach) = ball(half_lo, half_hi);
                    charge(&mut work, here_cost)?;
                    let half = prune(&here, middle, reach);
                    let half_cost = prepared_cost(&half);
                    for (lo, hi) in boxes(half_lo, half_hi, FINE) {
                        let (middle, reach) = ball(lo, hi);
                        charge(&mut work, half_cost)?;
                        let quarter = prune(&half, middle, reach);
                        let quarter_cost = prepared_cost(&quarter);
                        let mut fresh = 0u64;
                        for k in lo[2]..=hi[2] {
                            for j in lo[1]..=hi[1] {
                                for i in lo[0]..=hi[0] {
                                    fresh += u64::from(!exact[idx(i, j, k)]);
                                }
                            }
                        }
                        charge(&mut work, fresh * quarter_cost)?;
                        for k in lo[2]..=hi[2] {
                            for j in lo[1]..=hi[1] {
                                for i in lo[0]..=hi[0] {
                                    let s = idx(i, j, k);
                                    if !exact[s] {
                                        field[s] = eval(&quarter, at(i, j, k));
                                        exact[s] = true;
                                        stats.samples_evaluated += 1;
                                    }
                                }
                            }
                        }
                        // A crossing's normal is asked of the half block's tree (as exact here as
                        // the quarter's, and kept once for the eight quarters): a tree a quarter
                        // block was 30,000 trees and 188 MB for one body, and up to 742 MB for a
                        // legal build (review of part 4's slice 2).
                        near_at[((lo[2] / FINE) * fine[1] + lo[1] / FINE) * fine[0] + lo[0] / FINE] = near.len() as u32;
                    }
                    near.push((half, half_cost));
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
    // Each crossing: its point (linear in the two samples) and the field's gradient there,
    // from what is near its block (charged as each is taken).
    let h = cell * 0.05;
    let grad = |tree: &Prepared, p: V3| -> V3 {
        let g = [
            eval(tree, [p[0] + h, p[1], p[2]]) - eval(tree, [p[0] - h, p[1], p[2]]),
            eval(tree, [p[0], p[1] + h, p[2]]) - eval(tree, [p[0], p[1] - h, p[2]]),
            eval(tree, [p[0], p[1], p[2] + h]) - eval(tree, [p[0], p[1], p[2] - h]),
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
                    // The quarter block this edge begins in holds the whole of it (when it was
                    // sampled: an edge of a block that was filled by its sign asks the whole tree).
                    let slot = near_at[((k / FINE) * fine[1] + j / FINE) * fine[0] + i / FINE];
                    let (tree_here, cost_here) = near.get(slot as usize).map_or((&prepared, cost), |(t, c)| (t, *c));
                    charge(&mut work, 6 * cost_here)?;
                    let n = grad(tree_here, p);
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
    if has_color(&prepared) {
        charge(&mut work, shape.positions.len() as u64 * 2 * cost)?;
        shape.colors = shape.positions.iter().map(|p| eval_color(&prepared, *p, opt.color).1).collect();
    } else {
        // No node has a colour of its own: every vertex is the sculpt's.
        shape.colors = vec![opt.color; shape.positions.len()];
    }
    // A triangle too small to be kept is closed by its shortest edge: one end is moved onto
    // the other, so the two triangles on that edge collapse together and their neighbours
    // meet. (Left to the weld below, which drops any triangle under the least area whatever
    // its corners, such a triangle went and its three neighbours stayed: a pinhole a few
    // micrometres across between two fingers, on some hands at some resolutions. Found by the
    // second review of part 4's slice 2.)
    for _ in 0..4 {
        let mut moved = false;
        for t in 0..shape.tris.len() {
            let [a, b, c] = shape.tris[t].map(|v| v as usize);
            let p = [shape.positions[a], shape.positions[b], shape.positions[c]];
            if p[0] == p[1] || p[1] == p[2] || p[2] == p[0] {
                continue;
            }
            let area = 0.5 * math3::length(math3::cross(math3::sub(p[1], p[0]), math3::sub(p[2], p[0])));
            if area >= crate::mesh::MIN_TRIANGLE_AREA {
                continue;
            }
            let edges = [(a, b), (b, c), (c, a)];
            let shortest = (0..3).min_by(|&x, &y| {
                let len = |e: (usize, usize)| math3::length(math3::sub(shape.positions[e.0], shape.positions[e.1]));
                len(edges[x]).total_cmp(&len(edges[y]))
            });
            if let Some(e) = shortest {
                let (keep, go) = edges[e];
                let at = shape.positions[keep];
                // Every vertex at the place that goes: the mesh is not welded yet.
                let gone = shape.positions[go];
                for q in shape.positions.iter_mut().filter(|q| **q == gone) {
                    *q = at;
                }
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // Triangles collapsed by vertices clamped or moved onto each other.
    shape.weld_exact();
    stats.work = work;
    stats.trees_kept = near.len();
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
    #[ignore = "prints what each node costs, for node_cost's weights: run by hand, in release"]
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
        let link = Prim::RoundCone { a: [0.1, 0.0, 0.2], b: [0.4, 1.0, -0.1], ra: 0.3, rb: 0.15 };
        time("round cone", &prim(link));
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[..4].copy_from_slice(&[[0.0, 0.17, 0.11], [0.4, 0.13, 0.09], [0.75, 0.15, 0.11], [1.0, 0.06, 0.06]]);
        time("column of 4 sections", &prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.1, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4 }));
        time("round cone stretched", &with(link, Modifiers { stretch: [1.0, 1.0, 0.6], ..Modifiers::default() }));
        let links = |k: usize| (0..k).map(|_| prim(link)).collect::<Vec<_>>();
        time("chain of 8 round cones", &SdfNode { kind: Kind::Group { op: Combine::Union, kids: links(8), smooth: 0.02 }, m: Modifiers::default() });
        time("chain of 24 round cones", &SdfNode { kind: Kind::Group { op: Combine::Union, kids: links(24), smooth: 0.02 }, m: Modifiers::default() });
    }

    /// A column's sizes go from one section to the next without stopping at the ones between:
    /// through three sections that taper evenly it is a straight taper (no band at the
    /// middle one), it is level at a section where the sizes turn, and between two sections
    /// it is never outside their sizes.
    #[test]
    fn a_column_tapers_through_its_sections() {
        let radius_at = |sections: &[[f32; 3]], t: f32| {
            // Where the field is zero going out from the axis, at this height.
            let (mut low, mut high) = (0.0f32, 2.0f32);
            for _ in 0..40 {
                let middle = (low + high) / 2.0;
                if column([middle, t, 0.0], [0.0; 3], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], sections) < 0.0 {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            low
        };
        let even = [[0.0, 0.30, 0.30], [0.5, 0.20, 0.20], [1.0, 0.10, 0.10]];
        for i in 1..20 {
            let t = i as f32 / 20.0;
            assert!((radius_at(&even, t) - (0.30 - 0.20 * t)).abs() < 1e-4, "at {t}: {}", radius_at(&even, t));
        }
        let swell = [[0.0, 0.10, 0.10], [0.4, 0.20, 0.20], [1.0, 0.12, 0.12]];
        assert!((radius_at(&swell, 0.4) - 0.20).abs() < 1e-4);
        assert!((radius_at(&swell, 0.39) - 0.20).abs() < 2e-4 && (radius_at(&swell, 0.41) - 0.20).abs() < 2e-4, "level where the sizes turn");
        for i in 0..=40 {
            let t = i as f32 / 40.0;
            let r = radius_at(&swell, t);
            let (least, most) = if t <= 0.4 { (0.10, 0.20) } else { (0.12, 0.20) };
            assert!(r >= least - 1e-4 && r <= most + 1e-4, "at {t}: {r}");
        }
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

    /// A round cone is every ball between its two: its distance at a point outside is the
    /// least, over the balls along it, of the distance to that ball. Held against a walk of
    /// 4,000 balls, for cones that lean, taper either way, and whose one end swallows the other.
    #[test]
    fn a_round_cone_is_the_balls_between_its_ends() {
        let cones: [(V3, V3, f32, f32); 6] = [
            ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.3, 0.1),
            ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.1, 0.3),
            ([0.2, -0.4, 0.1], [0.9, 0.5, -0.6], 0.25, 0.25),
            ([-1.0, 0.3, 0.2], [0.4, 0.1, 0.9], 0.05, 0.4),
            // One ball inside the other, and two ends at one place: the larger ball.
            ([0.0, 0.0, 0.0], [0.0, 0.1, 0.0], 0.5, 0.2),
            ([0.3, 0.3, 0.3], [0.3, 0.3, 0.3], 0.2, 0.35),
        ];
        for (a, b, ra, rb) in cones {
            let walked = |p: V3| {
                (0..=4000).map(|i| {
                    let t = i as f32 / 4000.0;
                    math3::length(math3::sub(p, math3::lerp(a, b, t))) - (ra + (rb - ra) * t)
                }).fold(f32::MAX, f32::min)
            };
            let mut outside = 0;
            for i in 0..600 {
                // A lattice of points around the cone, a deterministic scatter.
                let f = |k: u32| ((i * k) % 97) as f32 / 96.0 * 3.0 - 1.5;
                let p = [f(31), f(57), f(73)];
                let (d, w) = (round_cone(p, a, b, ra, rb), walked(p));
                assert_eq!(d > 0.0, w > 0.0, "{a:?} {b:?} {ra} {rb} at {p:?}: {d} against {w}");
                if w > 0.0 {
                    outside += 1;
                    assert!((d - w).abs() < 2e-4, "{a:?} {b:?} {ra} {rb} at {p:?}: {d} against {w}");
                }
            }
            assert!(outside > 300, "{outside}");
            // On its ends' far sides the surface is a ball's.
            let along = math3::sub(b, a);
            if math3::length(along) > 0.2 {
                let n = math3::normalize(along);
                assert!(round_cone(math3::add(b, math3::scale(n, rb)), a, b, ra, rb).abs() < 1e-5);
                assert!(round_cone(math3::sub(a, math3::scale(n, ra)), a, b, ra, rb).abs() < 1e-5);
            }
        }
    }

    /// A chain meshes closed, where it is, with about the volume of its cones; and its bounds
    /// are the box around its balls, not a box about the origin (which would spend the grid on
    /// empty space for a limb far from it).
    #[test]
    fn a_chain_of_round_cones_meshes_where_it_is() {
        let link = |a: V3, b: V3, ra: f32, rb: f32| prim(Prim::RoundCone { a, b, ra, rb });
        // A leg standing two metres along X: thigh, shank, and a foot forward.
        let leg = SdfNode {
            kind: Kind::Group {
                op: Combine::Union,
                kids: vec![link([2.0, 0.9, 0.0], [2.0, 0.5, 0.0], 0.09, 0.06), link([2.0, 0.5, 0.0], [2.0, 0.08, 0.0], 0.06, 0.04), link([2.0, 0.08, 0.0], [2.0, 0.03, 0.18], 0.04, 0.03)],
                smooth: 0.02,
            },
            m: Modifiers::default(),
        };
        let (lo, hi, lip, slack) = bounds(&leg).unwrap();
        // (The box around its balls, and the 0.02 its smoothing may add all round.)
        assert!((lo[0] - 1.89).abs() < 1e-5 && (hi[0] - 2.11).abs() < 1e-5 && (lo[1] + 0.02).abs() < 1e-5 && (hi[1] - 1.01).abs() < 1e-5 && (hi[2] - 0.23).abs() < 1e-5, "{lo:?} {hi:?}");
        assert_eq!((lip, slack), (1.0, 0.02));
        let (s, _) = mesh(&leg, &opts(64)).unwrap();
        assert_eq!(s.open_edge_count(), 0);
        let bb = s.bounds();
        assert!(bb.min[0] > 1.88 && bb.max[0] < 2.12 && bb.min[1] > -0.03 && bb.max[1] < 1.02, "{bb:?}");
        // The thigh alone is a frustum of 0.4 m with a half ball at each end: about 7.5 litres.
        let thigh = mesh(&link([2.0, 0.9, 0.0], [2.0, 0.5, 0.0], 0.09, 0.06), &opts(64)).unwrap().0.signed_volume();
        let pi = std::f32::consts::PI;
        let exact = pi * 0.4 / 3.0 * (0.09 * 0.09 + 0.09 * 0.06 + 0.06 * 0.06) + 2.0 / 3.0 * pi * (0.09f32 * 0.09 * 0.09 + 0.06f32 * 0.06 * 0.06);
        assert!((thigh - exact).abs() < 0.05 * exact, "{thigh} against about {exact}");
        assert!(s.signed_volume() > thigh);
        s.finalize(60.0, "m").validate().unwrap();
    }

    /// A ball stretched to twice its breadth and half its depth is the ellipsoid of those
    /// radii: the same surface, the same bounds, and a field that never says further than true.
    #[test]
    fn a_stretched_ball_is_an_ellipsoid() {
        let mut ball = prim(Prim::Sphere { radius: 0.5 });
        ball.m.stretch = [2.0, 1.0, 0.5];
        ball.m.at = [0.3, 1.0, -0.2];
        let (lo, hi, lip, _) = bounds(&ball).unwrap();
        let near = |a: V3, b: V3| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-5);
        assert!(near(lo, [-0.7, 0.5, -0.45]) && near(hi, [1.3, 1.5, 0.05]) && lip == 1.0, "{lo:?} {hi:?} {lip}");
        let (s, _) = mesh(&ball, &opts(48)).unwrap();
        assert_eq!(s.open_edge_count(), 0);
        let exact = 4.0 / 3.0 * std::f32::consts::PI * 1.0 * 0.5 * 0.25;
        assert!((s.signed_volume() - exact).abs() < 0.03 * exact, "{} against {exact}", s.signed_volume());
        for p in &s.positions {
            let q = [(p[0] - 0.3) / 1.0, (p[1] - 1.0) / 0.5, (p[2] + 0.2) / 0.25];
            assert!((math3::length(q) - 1.0).abs() < 0.08, "{p:?} is off the ellipsoid");
        }
        // The field is a lower bound: along the short axis it is the true distance, along the
        // long one it is less.
        let mut next = 0;
        let prepared = prepare(&ball, 1, &mut next);
        let at = |x: f32, y: f32, z: f32| eval(&prepared, [0.3 + x, 1.0 + y, -0.2 + z]);
        assert!((at(0.0, 0.0, 0.75) - 0.5).abs() < 1e-5, "{}", at(0.0, 0.0, 0.75));
        assert!(at(2.0, 0.0, 0.0) > 0.0 && at(2.0, 0.0, 0.0) <= 1.0 + 1e-5, "{}", at(2.0, 0.0, 0.0));
        assert!(at(0.0, 0.0, 0.0) < 0.0);
    }

    /// Pruning a tree for a ball changes nothing in the ball: the pruned tree's distance and
    /// colour are the whole tree's bit for bit at every point within reach, for a figure's
    /// tree (unions of chains inside a smooth union, stretched slabs, a mirror) with colours
    /// on some parts. And it does prune: around a hand, most of the body is left out.
    #[test]
    fn a_pruned_tree_is_the_whole_tree_where_it_was_pruned_for() {
        let body = specula_corpus::Body::typical(specula_corpus::Sex::Female);
        let mut tree = crate::figure::tree(&body, &crate::figure::Shaping::default(), crate::figure::Part::Whole);
        // Colours on two parts and a mirror on one, so that colour blending and a fold are held too.
        if let Kind::Group { kids, .. } = &mut tree.kind {
            kids[0].m.color = Some([1.0, 0.2, 0.2, 1.0]);
            kids[3].m.color = Some([0.2, 0.2, 1.0, 1.0]);
            let last = kids.len() - 1;
            kids[last].m.mirror = [true, false, false];
        }
        let whole = prepare(&tree, 7, &mut 0);
        let whole_cost = prepared_cost(&whole);
        assert_eq!(whole_cost, eval_cost(&tree));
        let (mut pruned_cost, mut balls, mut compared) = (0u64, 0u64, 0u64);
        for b in 0..240u32 {
            // Ball centres over the figure's box, a deterministic scatter; two sizes of ball.
            let f = |k: u32, span: f32, from: f32| ((b * k) % 101) as f32 / 100.0 * span + from;
            let c = [f(37, 1.3, -0.65), f(53, 1.75, -0.05), f(71, 0.5, -0.2)];
            let reach = if b % 2 == 0 { 0.06 } else { 0.15 };
            let near = prune(&whole, c, reach);
            pruned_cost += prepared_cost(&near);
            balls += 1;
            for i in 0..40u32 {
                let g = |k: u32| (((b * 7 + i) * k) % 61) as f32 / 30.0 - 1.0;
                let d = [g(13), g(29), g(43)];
                let length = math3::length(d).max(1e-6);
                // Points inside the ball, some on its edge.
                let p = math3::add(c, math3::scale(d, reach * if i % 5 == 0 { 0.999 } else { 0.7 } / length.max(1.0)));
                let (full, full_colour) = eval_color(&whole, p, [1.0; 4]);
                let (part, part_colour) = eval_color(&near, p, [1.0; 4]);
                assert_eq!(part.to_bits(), full.to_bits(), "ball {b} at {c:?} within {reach}, point {p:?}: {part} against {full}");
                assert_eq!(part_colour.map(f32::to_bits), full_colour.map(f32::to_bits), "ball {b}, point {p:?}");
                assert_eq!(eval(&near, p).to_bits(), eval(&whole, p).to_bits());
                compared += 1;
            }
        }
        assert_eq!(compared, 9600);
        // (A little over half: it was under half before a column of changing proportions was
        // given its allowance, which keeps such a column longer than its distance alone would.)
        assert!(pruned_cost / balls < whole_cost * 3 / 5, "pruning leaves {} of {whole_cost} a ball on average", pruned_cost / balls);
        // And on a tree whose saving is plain by how it is made:
        let limb = |x: f32| {
            let links = (0..10).map(|i| prim(Prim::RoundCone { a: [x, i as f32 * 0.1, 0.0], b: [x, (i + 1) as f32 * 0.1, 0.0], ra: 0.05, rb: 0.05 })).collect();
            SdfNode { kind: Kind::Group { op: Combine::Near, kids: links, smooth: 0.01 }, m: Modifiers::default() }
        };
        let two = prepare(&SdfNode { kind: Kind::Group { op: Combine::Near, kids: vec![limb(0.0), limb(2.0)], smooth: 0.02 }, m: Modifiers::default() }, 1, &mut 0);
        let one_link = prepared_cost(&prepare(&prim(Prim::RoundCone { a: [0.0; 3], b: [0.0, 0.1, 0.0], ra: 0.05, rb: 0.05 }), 1, &mut 0));
        // Beside the middle of one limb, two metres from the other: three links at the most, of twenty.
        let beside = prune(&two, [0.06, 0.55, 0.0], 0.03);
        assert!(prepared_cost(&beside) <= 3 * one_link + 120 && prepared_cost(&two) >= 20 * one_link, "{} of {}", prepared_cost(&beside), prepared_cost(&two));
        // Through a cut: a part cut by a box is pruned inside the cut, as exactly.
        let foot = prepare(&crate::figure::tree(&body, &crate::figure::Shaping::default(), crate::figure::Part::Foot(specula_corpus::Side::Left)), 3, &mut 0);
        let mut saved = 0;
        for b in 0..60u32 {
            let c = [0.02 + (b % 7) as f32 * 0.03, (b % 5) as f32 * 0.03, -0.08 + (b % 11) as f32 * 0.03];
            let near = prune(&foot, c, 0.02);
            saved += u64::from(prepared_cost(&near) < prepared_cost(&foot) / 2);
            for i in 0..20u32 {
                let p = [c[0] + ((i * 7) % 5) as f32 * 0.004 - 0.008, c[1] + ((i * 3) % 5) as f32 * 0.004 - 0.008, c[2] + (i % 5) as f32 * 0.004 - 0.008];
                assert_eq!(eval(&near, p).to_bits(), eval(&foot, p).to_bits(), "ball {b}, point {p:?}");
            }
        }
        // (Nearly half of them: it was over half before the foot's columns of changing
        // proportions were given their allowance.)
        assert!(saved > 20, "{saved} of 60 balls about a foot cost under half of it");
        // A node that is not a plain union is kept whole.
        let mut twisted = tree.clone();
        twisted.m.twist = 20.0;
        let twisted = prepare(&twisted, 7, &mut 0);
        assert_eq!(prepared_cost(&prune(&twisted, [0.6, 0.8, 0.0], 0.05)), prepared_cost(&twisted));
    }

    /// The mesher spends less on a figure than every bone at every sample would cost, and
    /// says so in its work.
    #[test]
    fn a_figures_work_is_what_is_near_each_block() {
        let body = specula_corpus::Body::typical(specula_corpus::Sex::Male);
        let tree = crate::figure::tree(&body, &crate::figure::Shaping::default(), crate::figure::Part::Body);
        let (shape, stats) = mesh(&tree, &opts(96)).unwrap();
        assert_eq!(unbalanced(&shape), 0);
        let every_bone = stats.samples_evaluated * eval_cost(&tree);
        // (A body of columns costs about three quarters of what its whole tree would at every
        // sample. It was under two thirds until its columns of changing proportions were given
        // their allowance and each limb was cut by its own room: both keep more, and are right.)
        assert!(stats.work < every_bone * 4 / 5, "{} work for {} samples of a tree that costs {}", stats.work, stats.samples_evaluated, eval_cost(&tree));
        // What is kept for the normals is a tree a half block near the surface, not one a
        // quarter block: thousands, not tens of thousands.
        assert!(stats.trees_kept > 0 && (stats.trees_kept as u64) * 16 < stats.samples_evaluated, "{} trees kept for {} samples", stats.trees_kept, stats.samples_evaluated);
    }

    /// A column is its sections: at each one the surface is that ellipse, between two it
    /// passes smoothly from one to the other (no step in breadth or depth at a section), it is
    /// closed by a rounded end, and its field never says further than the truth.
    #[test]
    fn a_column_is_its_sections_and_smooth_between_them() {
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[0] = [0.0, 0.17, 0.11];
        sections[1] = [0.4, 0.13, 0.09];
        sections[2] = [0.75, 0.15, 0.11];
        sections[3] = [1.0, 0.06, 0.06];
        let trunk = Prim::Column { a: [0.0, 0.8, 0.0], b: [0.0, 1.4, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4 };
        let d = |x: f32, y: f32, z: f32| prim_distance(&trunk, [x, y, z]);
        // On each section, at its breadth and at its depth.
        for (t, rx, rz) in [(0.0, 0.17, 0.11), (0.4, 0.13, 0.09), (0.75, 0.15, 0.11), (1.0, 0.06, 0.06)] {
            let y = 0.8 + 0.6 * t;
            assert!(d(rx, y, 0.0).abs() < 1e-5 && d(0.0, y, rz).abs() < 1e-5 && d(-rx, y, 0.0).abs() < 1e-5, "section at {t}");
            assert!(d(0.0, y, 0.0) < 0.0);
        }
        // Between sections the breadth eases: no jump in it or in its slope at a section.
        let breadth = |y: f32| {
            let (mut lo, mut hi) = (0.0f32, 0.5f32);
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                if d(mid, y, 0.0) < 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            lo
        };
        let at = 0.8 + 0.6 * 0.4;
        let (below, on, above) = (breadth(at - 0.002), breadth(at), breadth(at + 0.002));
        assert!((below - on).abs() < 2e-4 && (above - on).abs() < 2e-4, "{below} {on} {above}: level through a section where the sizes turn");
        assert!(breadth(0.8 + 0.6 * 0.2) < 0.17 && breadth(0.8 + 0.6 * 0.2) > 0.13);
        // Closed past its ends: outside a little below the bottom and above the top.
        assert!(d(0.0, 0.8 - 0.12, 0.0) > 0.0 && d(0.0, 0.8 - 0.05, 0.0) < 0.0 && d(0.0, 1.4 + 0.07, 0.0) > 0.0);
        // Never further than the truth: against the nearest of many points of its own surface.
        let node = prim(trunk);
        let (shape, _) = mesh(&node, &opts(64)).unwrap();
        assert_eq!(shape.open_edge_count(), 0);
        for probe in [[0.4f32, 1.0, 0.0], [0.0, 1.1, 0.3], [0.25, 0.7, 0.2], [0.1, 1.6, 0.0]] {
            let truth = shape.positions.iter().map(|q| math3::length(math3::sub(*q, probe))).fold(f32::MAX, f32::min);
            let said = prim_distance(&trunk, probe);
            assert!(said > 0.0 && said <= truth + 0.01, "at {probe:?} the field says {said} and the surface is {truth} away");
        }
        let (lo, hi, lip, _) = bounds(&node).unwrap();
        let bb = shape.bounds();
        assert!((0..3).all(|k| lo[k] <= bb.min[k] + 1e-3 && hi[k] >= bb.max[k] - 1e-3) && lip > 1.0 && lip < 2.0, "{lo:?} {hi:?} against {bb:?}, lip {lip}");
    }

    /// A child that is repeated is never pruned away, and is never the reason another is: its
    /// field jumps at its cells' edges, so what it says at a ball's middle is not a bound for
    /// the ball. (A round cone set 0.2 m off its cell's middle and repeated was dropped beside
    /// a box, and samples came out on the wrong side: review of part 4's slice 2.)
    #[test]
    fn a_repeated_child_is_never_pruned_away() {
        let base = prim(Prim::Box { half: [0.6, 0.1, 0.6], round: 0.0 });
        let mut cones = prim(Prim::RoundCone { a: [0.2, 0.0, 0.0], b: [0.2, 0.5, 0.0], ra: 0.08, rb: 0.03 });
        cones.m.repeat = Some(([3, 1, 3], [0.45, 0.0, 0.45]));
        let tree = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![base, cones], smooth: 0.0 }, m: Modifiers::default() };
        let whole = prepare(&tree, 3, &mut 0);
        let mut compared = 0;
        for b in 0..400u32 {
            let f = |k: u32, span: f32, from: f32| ((b * k) % 101) as f32 / 100.0 * span + from;
            let c = [f(7, 1.6, -0.8), f(13, 0.9, -0.2), f(29, 1.6, -0.8)];
            let reach = if b % 2 == 0 { 0.05 } else { 0.15 };
            let pruned = prune(&whole, c, reach);
            let PKind::Group { kids, .. } = &pruned.kind else { panic!("a group") };
            assert!(kids.iter().any(|k| k.m.repeat.is_some()), "the repeated child was dropped at {c:?}");
            for i in 0..27u32 {
                let step = |d: u32| (d as f32 - 1.0) * reach * 0.55;
                let p = [c[0] + step(i % 3), c[1] + step(i / 3 % 3), c[2] + step(i / 9)];
                assert_eq!(eval(&pruned, p).to_bits(), eval(&whole, p).to_bits(), "at {p:?} in the ball at {c:?}");
                compared += 1;
            }
        }
        assert_eq!(compared, 400 * 27);
    }

    /// A column whose breadth and depth change in different proportions is pruned with an
    /// allowance for how far out of it a point is: far out to one side its field follows that
    /// side's radius and changes along the column much faster than its factor says. (A wedge
    /// 5 by 4 cm growing to 20 by 4 cm, near-joined with a box whose face is 3 m away, was
    /// pruned by its distance alone and came out with samples on the wrong side: second review
    /// of part 4's slice 2.) Here: wherever a ball is, the pruned tree says what the whole
    /// tree says in it.
    #[test]
    fn a_wedge_is_pruned_with_room_for_its_wander() {
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[..2].copy_from_slice(&[[0.0, 0.05, 0.04], [1.0, 0.2, 0.04]]);
        let wedge = prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.0, 0.2, 0.0], side: [1.0, 0.0, 0.0], sections, count: 2 });
        let mut slab = prim(Prim::Box { half: [0.5, 0.5, 0.5], round: 0.0 });
        slab.m.at = [3.5, 0.1, 0.0];
        let tree = SdfNode { kind: Kind::Group { op: Combine::Near, kids: vec![wedge, slab], smooth: 1.0 }, m: Modifiers::default() };
        let whole = prepare(&tree, 5, &mut 0);
        let PKind::Group { kids, .. } = &whole.kind else { panic!("a group") };
        assert!(kids[0].wander > 0.5 && kids[0].steady && kids[1].wander == 0.0, "the wedge wanders: {}", kids[0].wander);
        for b in 0..600u32 {
            let f = |k: u32, span: f32, from: f32| ((b * k) % 101) as f32 / 100.0 * span + from;
            let c = [f(7, 4.5, -0.5), f(13, 1.2, -0.5), f(29, 1.2, -0.6)];
            let reach = [0.02, 0.06, 0.2][(b % 3) as usize];
            let pruned = prune(&whole, c, reach);
            for i in 0..27u32 {
                let step = |d: u32| (d as f32 - 1.0) * reach * 0.55;
                let p = [c[0] + step(i % 3), c[1] + step(i / 3 % 3), c[2] + step(i / 9)];
                assert_eq!(eval(&pruned, p).to_bits(), eval(&whole, p).to_bits(), "at {p:?} in the ball of {reach} at {c:?}");
            }
        }
    }

    /// A smooth plain union with a child that is not steady keeps every child: three spheres,
    /// the middle one twisted, pruned for a ball at the nearest, said something in the fifth
    /// place other than the whole tree did (second review of part 4's slice 2).
    #[test]
    fn a_smooth_union_with_an_unsteady_child_keeps_them_all() {
        let ball = |x: f32, twist: f32| {
            let mut n = prim(Prim::Sphere { radius: 0.05 });
            n.m.at = [x, 0.0, 0.0];
            n.m.twist = twist;
            n
        };
        let tree = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![ball(0.235, 0.0), ball(0.14, 1.0), ball(0.05, 0.0)], smooth: 0.1 }, m: Modifiers::default() };
        let whole = prepare(&tree, 5, &mut 0);
        let pruned = prune(&whole, [0.0; 3], 0.01);
        let PKind::Group { kids, .. } = &pruned.kind else { panic!("a group") };
        assert_eq!(kids.len(), 3);
        assert_eq!(eval(&pruned, [0.0; 3]).to_bits(), eval(&whole, [0.0; 3]).to_bits());
    }

    /// A column that widens fast (a flange on a thin stem) is trusted less: its field, taken
    /// at a point's own height, says the stem is 18 cm away where the flange is 3.5 cm away,
    /// and the factor the mesher divides it by makes up the difference. With one fixed factor
    /// of 2 such a column was meshed with thousands of samples on the wrong side (review of
    /// part 4's slice 2).
    #[test]
    fn a_steep_columns_field_is_trusted_by_its_steepest_section() {
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[..4].copy_from_slice(&[[0.0, 0.02, 0.02], [0.8, 0.02, 0.02], [0.82, 0.3, 0.3], [1.0, 0.3, 0.3]]);
        let flange = Prim::Column { a: [0.0, 0.0, 0.0], b: [0.0, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4 };
        let lip = prim_lip(&flange);
        // Under the flange, beside the stem: the flange's underside is nearer than the stem.
        for probe in [[0.2f32, 0.79, 0.0], [0.1, 0.78, 0.1], [0.25, 0.8, 0.0], [0.0, 0.7, 0.15]] {
            let said = prim_distance(&flange, probe);
            // The true distance, by looking: the nearest of many points of the surface.
            let mut truth = f32::MAX;
            for i in 0..=4000 {
                let t = i as f32 / 4000.0;
                let (rx, _) = column_sizes(&sections[..4], t);
                let radial = math3::length([probe[0], 0.0, probe[2]]);
                truth = truth.min(math3::length([radial - rx, probe[1] - t, 0.0]));
            }
            assert!(said / lip <= truth + 1e-3, "at {probe:?}: the field {said} over {lip} is more than the {truth} the surface is away");
        }
        assert!(lip > 10.0, "a growth of 14 metres a metre: {lip}");
        // And it meshes closed, with the flange where it is.
        let (shape, _) = mesh(&prim(flange), &MeshOptions { resolution: 96, bounds: None, seed: 1, max_triangles: 400_000, max_work: MAX_WORK, color: [1.0; 4] }).unwrap();
        assert_eq!(shape.open_edge_count(), 0);
        let bb = shape.bounds();
        assert!((bb.max[0] - 0.3).abs() < 0.02 && bb.min[1] < 0.0 && bb.max[1] > 1.0, "{bb:?}");
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
