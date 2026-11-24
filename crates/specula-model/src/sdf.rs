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
//! [`MAX_RESOLUTION`]; at most [`MAX_SAMPLES`] samples), the field sampled sparsely (8Â³-cell
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
/// Most work units one sculpt may spend on its own ([`crate::work`]: samples Ã— [`eval_cost`],
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
    /// A box (or, with `disc`, a cylinder along Y: `half` is then its radius, its half height
    /// and its radius again) rounded by one number: as wide, as tall and as deep as its hard
    /// shape, with its sides flat and its corners tight at a high `power` and all of it round
    /// at 2, where it is the ellipsoid of those radii (a superquadric). A recipe gives the
    /// roundness, 0 to 1, and the power is 2 over it. Its field is a lower bound of the distance.
    Rounded { half: V3, power: f32, disc: bool },
    /// The half-space below the plane `normal Â· p = offset` (unbounded: give the sdf bounds).
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
    /// two sections it is between. `square` says how square its sections are: 2 is an ellipse,
    /// and the higher it is the flatter the sides and the tighter the corners (a superellipse:
    /// 3 is a body's trunk, 8 nearly a box with rounded corners). A
    /// rounded end closes it past each end. A trunk, a limb with its swell, a neck, a vase.
    /// Given by its ends, in the node's frame; its field is a lower bound of the distance.
    Column { a: V3, b: V3, side: V3, sections: [[f32; 3]; MAX_SECTIONS], count: u8, square: f32 },
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
    /// This node or one under it is an ellipsoid whose radii differ ([`ellipsoid_is_loose`]):
    /// its field changes faster than any one factor says, so no factor bounds it across a
    /// ball; only [`field_range`] does.
    loose: bool,
    /// A column's box and ratio ([`Outline`]), walked once for the node before anything asks
    /// its range ([`outline_columns`]: the box's search and the mesher each outline the tree
    /// they ask); `None` until then, and for anything but a column.
    outline: Option<Outline>,
}

/// What [`field_range`] needs of a column besides its field, from a walk of its curve
/// ([`ColumnTable`] at level 0, [`column_ratio`]) and its rate across ([`column_across`]):
/// the same for every ball, so it is worked out once a node, never once an ask (the second
/// review of the box's search found it worked out at every ball, a few thousand powers a
/// column each time and charged as eight evaluations).
#[derive(Clone, Copy, Debug)]
struct Outline {
    lo: V3,
    hi: V3,
    ratio: f32,
    across: f32,
}

/// Whether a primitive's field is not bounded by its factor ([`prim_lip`]) across a ball: an
/// ellipsoid whose radii differ. Its field reads short outside and never more than the
/// distance there, but inside it reads up to the ratio of its radii too deep, and near its
/// middle it changes direction by direction between minus its greatest and minus its least
/// radius, so no factor holds it (`docs/design/DISTANCE_FIELDS.md`, 2.6). (A ball, all radii
/// equal, is the distance itself.)
fn ellipsoid_is_loose(p: &Prim) -> bool {
    matches!(p, Prim::Ellipsoid { radii } if !(radii[0] == radii[1] && radii[1] == radii[2]))
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
    /// The one error a cancelled sculpt gives, wherever the cancel is seen (the build's charge,
    /// the reach, the box's search, a walk of the columns): a limit with no measure and no
    /// limit. A cancel is not a measure passing a limit, and the work done when it is seen
    /// depends on when it came (a timing, not a fact of the recipe), so it carries neither, as
    /// a recipe's own cancel does (`recipe::charge`). (Review 4 of the box lane found three
    /// different made-up pairs: "0 of 0" from the reach, the search's work against 0, the
    /// build's work against its limit.)
    fn cancelled() -> SdfError {
        SdfError { limit: true, message: crate::work::CANCELLED.into(), measured: None, max: None }
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
    /// Powers taken solving for rounded shapes' nearest points (`roundness` other than 1),
    /// and how many of their samples were taken: what the build was charged past their
    /// weight is the powers past an allowance a sample, at a weight a power.
    pub rounded_powers: u64,
    pub rounded_samples: u64,
    /// The most powers taken between two charges: what ran before it was paid for.
    pub rounded_most_between: u64,
    /// Cells of the grid where something is thinner than a cell: those that hold more than
    /// one sheet of the surface, or a face whose four corners go inside, outside, inside,
    /// outside in turn. Each sheet there has a vertex of its own, so the mesh is a surface;
    /// a coarse one (a sheet as thin as that is a cell's vertices apart, at best).
    pub thin_cells: u64,
    /// The box round those cells, when there are any.
    pub thin_at: Option<(V3, V3)>,
    /// Faces of the grid that a thread thinner than a cell passes through, each given a
    /// vertex on the face for each side of it (in [`mesh`]).
    pub threads: u64,
    /// Cells whose crossings were grouped into sheets (six crossings or more: [`SHEETS_WORK`]
    /// each), and faces of four crossings whose middle was asked of the field (once from each
    /// cell beside it, at an evaluation each).
    pub sheet_cells: u64,
    pub faces_asked: u64,
    /// Edges of triangles under the least area tested before they were closed (a scan of
    /// the triangles each, charged a unit a triangle: [`closes_to_a_surface`]), and whether a
    /// sliver was left that no edge could close (the weld then drops it).
    pub closings_tested: u64,
    pub sliver_left: bool,
    /// The work all of that was charged: [`SHEETS_WORK`] a cell grouped, a face's tree's cost
    /// a face asked, a unit a triangle a sliver's edge tested, an eighth a vertex for finding
    /// vertices at one place and a unit a triangle when some are.
    pub sheets_work: u64,
    /// Edges of the mesh shared by other than two triangles ([`tangled_edges`]): none, by
    /// construction; counted after the last weld, so that a fault would be said.
    pub tangled_edges: u64,
    /// Evaluations of the field taken to make sure nothing lies past [`bounds`]'s box where
    /// the reach says something may (none where it does not: [`mesh_box_charged`]; at most
    /// [`CLEAR_ASKS`], counted in evaluations of the whole tree, a column's range as nine
    /// primitives), and the work that search was charged in `work`, as it was spent: what
    /// each step does, weighed as an evaluation is ([`Spent`]).
    pub box_asks: u64,
    pub box_work: u64,
    /// The work [`reach`] was charged in `work` for the box it gives the search, as it was
    /// worked out ([`reach_charged`]): every mesh without a box given pays it.
    pub reach_work: u64,
    /// The work the mesher's own ranges were charged in `work`: the walk of its tree's columns
    /// once ([`outline_columns`]), a block's range where the tree's factor bounds nothing, and
    /// the ranges of loose children when a block is pruned ([`field_range_as`], weighed as the
    /// box's search is: [`Spent`]). None where the tree's factor bounds its field.
    pub range_work: u64,
}

/// Edges shared by other than two triangles (by index, after the mesher's weld). Sorted, not
/// hashed (F2).
fn tangled_edges(shape: &Shape) -> u64 {
    let mut edges: Vec<(u32, u32)> = shape.tris.iter().flat_map(|t| [0, 1, 2].map(|k| (t[k].min(t[(k + 1) % 3]), t[k].max(t[(k + 1) % 3])))).collect();
    edges.sort_unstable();
    edges.chunk_by(|a, b| a == b).filter(|run| run.len() != 2).count() as u64
}

/// The least radius each primitive of the tree turns at, in the tree's own metres, one entry
/// a primitive in the order they are met depth first (every primitive, a half space and what
/// a difference takes away among them: [`primitive_of_triangles`]'s numbers): a ball's, a
/// capsule's, a cylinder's, a torus's tube, the thin end of a round cone (its thick end when
/// the thin one is a point), the narrowest section of a column, and an ellipsoid's tightest
/// curve (its least radius squared over its greatest). Every node above it bears on it: a
/// `round` swells it by that much (a box swollen by 2 cm turns at 2 cm), a `scale` scales it,
/// and a `stretch` tightens it as it does an ellipsoid's (by the least stretch squared over
/// the greatest). `None` for one with nothing round (a box, a half space); a rounded box or
/// cylinder (`roundness`) is not counted: how tightly it turns is its maker's number. A
/// node's twist, bend, displacement and repetition are not counted.
pub fn round_radii(n: &SdfNode) -> Vec<Option<f32>> {
    let mine: Vec<Option<f32>> = match &n.kind {
        Kind::Prim(p) => vec![match *p {
            Prim::Sphere { radius } | Prim::Capsule { radius, .. } => Some(radius),
            Prim::Cylinder { radius, radius_top, .. } => Some(radius.max(radius_top)),
            Prim::Torus { tube, .. } => Some(tube),
            Prim::Ellipsoid { radii } => {
                let (lo, hi) = (radii[0].min(radii[1]).min(radii[2]), radii[0].max(radii[1]).max(radii[2]));
                (hi > 0.0).then(|| lo * lo / hi)
            }
            // (Its thin end, unless that is a point: a point is not round.)
            Prim::RoundCone { ra, rb, .. } => Some(if ra.min(rb) > 0.0 { ra.min(rb) } else { ra.max(rb) }),
            Prim::Column { sections, count, .. } => sections[..(count as usize).clamp(1, MAX_SECTIONS)].iter().map(|s| s[1].min(s[2])).reduce(f32::min),
            Prim::Box { .. } | Prim::Rounded { .. } | Prim::HalfSpace { .. } => None,
        }],
        Kind::Group { kids, .. } => kids.iter().flat_map(round_radii).collect(),
    };
    let s = n.m.stretch.map(f32::abs);
    let (lo, hi) = (s[0].min(s[1]).min(s[2]), s[0].max(s[1]).max(s[2]));
    let tighten = if hi > 0.0 && lo > 0.0 { lo * lo / hi } else { 1.0 };
    mine.into_iter()
        .map(|own| {
            // (A swelling rounds what it swells.)
            let swollen = if n.m.round > 0.0 { Some(own.map_or(n.m.round, |r| r + n.m.round)) } else { own };
            swollen.map(|r| r * tighten * n.m.scale.abs()).filter(|r| r.is_finite() && *r > 0.0)
        })
        .collect()
}

fn count_nodes(n: &SdfNode) -> usize {
    1 + match &n.kind {
        Kind::Prim(_) => 0,
        Kind::Group { kids, .. } => kids.iter().map(count_nodes).sum(),
    }
}

fn prepare(n: &SdfNode, seed: u64, next: &mut u64) -> Prepared {
    let r = Affine::rotate_euler_deg([n.m.rotate[0] as f64, n.m.rotate[1] as f64, n.m.rotate[2] as f64]);
    // Inverse of a rotation: its transpose, whose rows are the rotation's columns. (`local`
    // dots a point with each row. These were the rotation's own rows until 2026-11-19, which
    // turned the field the opposite way to the node's bounds.)
    let inv_rot = r.cols;
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
    let loose = match &kind {
        PKind::Prim(p) => ellipsoid_is_loose(p),
        PKind::Group { kids, .. } => kids.iter().any(|k| k.loose),
    };
    Prepared { kind, m: n.m.clone(), inv_rot, rotated: n.m.rotate != [0.0; 3], seed: my_seed, lip, slack, steady, wander, least, loose, outline: None }
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
#[cfg(test)]
fn prune(n: &Prepared, c: V3, reach: f32) -> Prepared {
    prune_asking(n, c, reach, &mut Spent::default())
}

/// [`prune`], adding to `spent` what its loose children's ranges did ([`field_range_as`], in
/// the one scheme of [`Spent`]: what the build is charged for them, past the one evaluation of
/// the tree that [`valued`] makes).
fn prune_asking(n: &Prepared, c: V3, reach: f32, spent: &mut Spent) -> Prepared {
    #[cfg(test)]
    if NO_PRUNE.with(|f| f.get()) {
        return n.clone();
    }
    pruned(n, c, reach, &valued(n, c), spent, &mut Noted::new())
}

/// [`prune`], with every node's distance at the ball's middle already taken ([`valued`]).
/// `seen` holds the ranges the loose children's walks took on the way down ([`Noted`]).
fn pruned(n: &Prepared, c: V3, reach: f32, at: &Valued, spent: &mut Spent, seen: &mut Noted) -> Prepared {
    let keep_all = |kind: PKind| Prepared { kind, m: n.m.clone(), inv_rot: n.inv_rot, rotated: n.rotated, seed: n.seed, lip: n.lip, slack: n.slack, steady: n.steady, wander: n.wander, least: n.least, loose: n.loose, outline: n.outline };
    let PKind::Group { op, kids, smooth } = &n.kind else { return n.clone() };
    let m = &n.m;
    if !plain(m) || at.kids.len() != kids.len() {
        return n.clone();
    }
    let q = local(n, c);
    let r = reach / (m.scale * m.stretch[0].min(m.stretch[1]).min(m.stretch[2]));
    // A cut or an intersection keeps every child, each pruned for the same ball.
    if !matches!(op, Combine::Union | Combine::Near) {
        return keep_all(PKind::Group { op: *op, kids: kids.iter().zip(&at.kids).map(|(kid, v)| pruned(kid, q, r, v, spent, seen)).collect(), smooth: *smooth });
    }
    // A smooth plain union with a child that is not steady keeps every child: such a child
    // stays whatever it says, and blending in turn it could carry the pull of a child that
    // went (three spheres, the middle one twisted: the pruned tree differed in the fifth place).
    if *op == Combine::Union && *smooth > 0.0 && kids.iter().any(|k| !k.steady) {
        return keep_all(PKind::Group { op: *op, kids: kids.iter().zip(&at.kids).map(|(kid, v)| pruned(kid, q, r, v, spent, seen)).collect(), smooth: *smooth });
    }
    // Each child's least and greatest distance anywhere in the ball.
    let span: Vec<(f32, f32)> = kids
        .iter()
        .zip(&at.kids)
        .map(|(k, v)| {
            let d = v.d;
            #[cfg(test)]
            let old = OLD_FACTORS.with(|f| f.get());
            #[cfg(not(test))]
            let old = false;
            if k.loose && !old {
                // An ellipsoid whose radii differ, or a group with one in it: its factor holds
                // nothing across the ball (inside it changes as fast as it likes near its
                // middle; outside a flat one's changes hundreds of times faster than the
                // distance): its range in the ball is worked out through it instead.
                // (Taken once a prune: a loose child under a loose child kept was walked with
                // it, and its range is found where that walk noted it.)
                if let Some(got) = seen.get(&(std::ptr::from_ref(k) as usize)) {
                    return *got;
                }
                return field_range_noting(k, q, r, &mut 1, spent, true, Some(seen));
            }
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
    let kids = kids.iter().zip(&keep).zip(&at.kids).filter(|((_, k), _)| **k).map(|((kid, _), v)| pruned(kid, q, r, v, spent, seen)).collect();
    keep_all(PKind::Group { op: *op, kids, smooth: *smooth })
}

fn has_color(n: &Prepared) -> bool {
    n.m.color.is_some() || matches!(&n.kind, PKind::Group { kids, .. } if kids.iter().any(has_color))
}

#[inline]
fn len2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

#[cfg(test)]
thread_local! {
    /// (Tests only) the weights of every primitive evaluated on this thread: what the work
    /// charged has to cover.
    static PRIM_WEIGHED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn prim_distance(p: &Prim, q: V3) -> f32 {
    #[cfg(test)]
    PRIM_WEIGHED.with(|w| w.set(w.get() + prim_cost(p)));
    match *p {
        Prim::Sphere { radius } => math3::length(q) - radius,
        Prim::Rounded { half, power, disc } => rounded(q, half, power, disc),
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
        Prim::Column { a, b, side, sections, count, square } => column(q, a, b, side, &sections[..(count as usize).clamp(1, MAX_SECTIONS)], square),
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

/// What a rounded box or cylinder costs a sample over a plain primitive: at a power of 2 (an
/// ellipsoid's own shares: no power to take) the first; at any other the second, which pays
/// for [`ROUNDED_ALLOW`] powers a sample. What the samples take past that, over the build, is
/// charged as they take it ([`ROUNDED_POWER`] a power, in [`mesh`]), so **the work limit
/// bounds what a build of rounded shapes takes, whatever they are and wherever they are
/// sampled**: no recipe takes more than one power for each [`ROUNDED_POWER`] units it is
/// charged.
///
/// The weight is one for every thinness. (Two weights graded by thinness, set by time on a
/// quiet machine on 2026-11-19, stood for the extra steps a thin shape's samples took; an
/// eighth review found that a large thin cutter past a corner still cost four times its
/// charge; what any sample takes past the weight is charged since.) Every evaluation a build
/// makes is charged (a ninth review found pruning's were not, under nested unions:
/// [`valued`]), so the bound holds for any recipe. What the samples take, by count, is in
/// `docs/design/PROCEDURAL_MODELLING.md` (tier 3, "Roundness as one number"), each figure
/// said with the samples it is for.
const ROUNDED_COST: [u64; 2] = [300, 1450];

/// The work a build is charged for each power its rounded shapes' samples take past
/// [`ROUNDED_ALLOW`] a sample, over the build (one sample's spare powers pay for another's).
/// No build can take more than one power for each of these units it is charged: 11.9 a
/// thousand, where the calibrated case ([`ROUNDED_COST`]) takes 10.9 (at a resolution of 48,
/// 2026-11-20, by count); a test holds every case it builds to five quarters of the
/// calibrated case. (A power alone was timed at about 48 units of a plain cut's work, on a
/// busy machine, by the eighth review.)
const ROUNDED_POWER: u64 = 84;
/// The powers a sample's weight pays for: the weight over [`ROUNDED_POWER`], rounded down.
const ROUNDED_ALLOW: u64 = ROUNDED_COST[1] / ROUNDED_POWER;
/// The same at a power of 2, whose first steps take no power (only its sure way does).
const ROUNDED_ALLOW_2: u64 = ROUNDED_COST[0] / ROUNDED_POWER;

/// A rounded box's or cylinder's field ([`Prim::Rounded`]), in metres.
///
/// The shape is the unit ball of a norm, `k(p) = (sum of |p_i / half_i|^power)^(1/power)`, so it
/// is convex.
///
/// **Outside it the field is the distance itself**, to the nearest point of the surface
/// ([`rounded_first`], [`rounded_sure`]): so a swelling (`round`), a shell or a blend of any size is measured
/// from the rounded surface, on every side. (It was first `k - 1` times a size, then the
/// distance to one touching plane, then to three: each was right along the axes and wrong
/// beside a thin shape's edge, where a swelling by a centimetre came out up to five, and three
/// reviews said so.)
///
/// **Inside** it is the greater of two bounds, each never less than the depth: the hard shape's
/// own (a box's, a cylinder's), and how far the point is behind planes that touch the shape
/// (`(k - 1) / |grad k|` for the plane whose normal is the gradient where the point is, then
/// for those touching where the last one's foot lies). The depth is never further from that
/// than the hard shape's corner is from the rounded surface ([`rounded_slack`]), which is what
/// the mesher and pruning are told: a field that changes no faster than distance, give or take
/// that much. At the surface both sides are nothing.
///
/// Each share is taken of the largest (`u_i / m`), so no power overflows far away or
/// underflows near the middle.
fn rounded(q: V3, half: V3, power: f32, disc: bool) -> f32 {
    let (f, powers, _) = rounded_counted(q, half, power, disc);
    debug_assert!(powers <= ROUNDED_MOST, "{powers} powers");
    // (What the build is charged past the weight: [`RoundedBill`].)
    let allowed = if power == 2.0 { ROUNDED_ALLOW_2 } else { ROUNDED_ALLOW };
    ROUNDED_TAKEN.with(|t| {
        let [taken, samples, allowance] = t.get();
        t.set([taken + u64::from(powers), samples + 1, allowance + allowed]);
    });
    f
}

thread_local! {
    /// The powers rounded shapes' samples took on this thread, how many samples, and how many
    /// powers their weights pay for, since [`rounded_taken`] last read them. (A count, never a
    /// time: the same recipe takes the same powers on every machine. A build evaluates on one
    /// thread.)
    static ROUNDED_TAKEN: std::cell::Cell<[u64; 3]> = const { std::cell::Cell::new([0, 0, 0]) };
}

/// The powers rounded shapes' samples took on this thread, how many samples, and the powers
/// their weights pay for, since it was last called; it starts all three again.
fn rounded_taken() -> [u64; 3] {
    ROUNDED_TAKEN.with(|t| t.replace([0, 0, 0]))
}

/// What is charged for rounded shapes' powers past what their samples' weights pay for
/// ([`ROUNDED_POWER`] units a power), over a build or a run of questions: the powers taken
/// less the allowance of the samples so far, each power past it charged once (what was paid is
/// kept, so an allowance that grows and is then used up again is never charged twice). Started
/// empty, with this thread's tally emptied: whatever the thread evaluated before is not this
/// bill's.
struct RoundedBill {
    taken: u64,
    samples: u64,
    allowed: u64,
    paid: u64,
    /// The most powers taken between two bills: what ran before it was charged.
    most_between: u64,
}

impl RoundedBill {
    fn start() -> RoundedBill {
        rounded_taken();
        RoundedBill { taken: 0, samples: 0, allowed: 0, paid: 0, most_between: 0 }
    }

    /// Takes this thread's tally into the bill: the work due for it now.
    fn due(&mut self) -> u64 {
        let [taken, samples, allowed] = rounded_taken();
        self.most_between = self.most_between.max(taken);
        (self.taken, self.samples, self.allowed) = (self.taken + taken, self.samples + samples, self.allowed + allowed);
        let over = self.taken.saturating_sub(self.allowed);
        let due = over - self.paid.min(over);
        self.paid = self.paid.max(over);
        due * ROUNDED_POWER
    }
}

/// [`rounded`], how many powers it took (what a sample of a rounded shape costs, which its
/// build is charged for past [`ROUNDED_ALLOW`]), and whether the nearest point was left to
/// the sure way. Never more than [`ROUNDED_MOST`] powers.
///
/// **What is counted**: every exponential and every logarithm, as a power (an exponential of a
/// logarithm, the first steps' unit, is one), and every square root as a quarter of one (a
/// price stated, not timed: a square root is one instruction, a power a few dozen), rounded up
/// to a whole power a sample. Nothing else the solve does repeats: its other arithmetic is a
/// few dozen operations a step, and a step takes at least a power.
fn rounded_counted(q: V3, half: V3, power: f32, disc: bool) -> (f32, u32, bool) {
    // (In quarters of a power: [`POWER`] and [`ROOT`].)
    let taken = std::cell::Cell::new(0u32);
    let powers = || taken.get().div_ceil(POWER);
    // A cylinder is the same in two: out from its axis, and along it.
    let (p, h, n) = if disc { ([len2(q[0], q[2]), q[1].abs(), 0.0], [half[0], half[1], 1.0], 2) } else { ([q[0].abs(), q[1].abs(), q[2].abs()], half, 3) };
    // The hard shape's own distance (a box's, or a cylinder's).
    let d = [p[0] - h[0], p[1] - h[1], if n == 3 { p[2] - h[2] } else { f32::MIN }];
    let hard = math3::length([d[0].max(0.0), d[1].max(0.0), d[2].max(0.0)]) + d[0].max(d[1]).max(d[2]).min(0.0);
    taken.set(ROOT * (1 + u32::from(disc)));
    // (One over each half size, taken once: the steps below multiply by them.)
    let inv = [1.0 / h[0], 1.0 / h[1], if n == 3 { 1.0 / h[2] } else { 0.0 }];
    // The sure way, when no start's first steps have arrived ([`rounded_sure`]).
    let sure = |lower: f32, start: f64| ((rounded_sure(p, h, inv, power, f64::from(lower), start, &taken) as f32).max(lower), powers(), true);
    if hard > 0.0 {
        // Past the hard shape, which holds the rounded one: outside it. The nearest point is
        // looked for first along the way out of the hard shape from its nearest point (right
        // past a corner or an edge, and over a face away from its rim); then, if that has not
        // arrived, from the touching plane at the sample, as inside the hard shape (right just
        // past a thin face beside its rim, where the way out is the face's and the nearest
        // point is on the rim: ninth review); then the sure way.
        let out = [d[0].max(0.0), d[1].max(0.0), if n == 3 { d[2].max(0.0) } else { 0.0 }];
        let start = match rounded_first(p, h, inv, power, Start::Past(out), hard, &taken) {
            Ok(f) => return (f, powers(), false),
            Err(start) => start,
        };
        let Some((g, len)) = rounded_gradient(p, inv, power, &taken) else { return sure(hard, start) };
        let plane = (math3::dot(g, p) - 1.0) / len;
        if !(plane > 0.0 && plane.is_finite()) {
            return sure(hard, start);
        }
        return match rounded_first(p, h, inv, power, Start::Foot(g, len, plane), plane.max(hard), &taken) {
            Ok(f) => (f, powers(), false),
            Err(_) => sure(plane.max(hard), start),
        };
    }
    // The first touching plane: the one whose normal is the gradient where the point is.
    let Some((g, len)) = rounded_gradient(p, inv, power, &taken) else { return (hard, powers(), false) };
    let mut plane = (math3::dot(g, p) - 1.0) / len;
    if plane > 0.0 {
        // Outside (`grad k . p` is `k`): the distance, which both bounds are below.
        return match rounded_first(p, h, inv, power, Start::Foot(g, len, plane), plane.max(hard), &taken) {
            Ok(f) => (f, powers(), false),
            Err(start) => sure(plane.max(hard), start),
        };
    }
    // Inside, steps toward the nearest point: the plane touching where the last plane's foot
    // lies. Each is a touching plane of a convex shape, so each is still never less than the
    // depth, and the greatest of them is kept. A second step is taken only when the first
    // gained something (beside a flat side it gains nothing, and most of a shape is beside one).
    let least = if n == 3 { h[0].min(h[1]).min(h[2]) } else { h[0].min(h[1]) };
    let (mut g, mut len, mut at) = (g, len, plane);
    for _ in 0..2 {
        let foot = math3::sub(p, math3::scale(g, at / len));
        let Some((next, next_len)) = rounded_gradient(foot, inv, power, &taken) else { break };
        let gained = (math3::dot(next, p) - 1.0) / next_len - plane;
        (g, len, at) = (next, next_len, plane + gained);
        if gained <= 0.01 * least {
            plane = plane.max(at);
            break;
        }
        plane = at;
    }
    (plane.max(hard).min(0.0), powers(), false)
}

/// The most powers one sample of a rounded shape can take ([`rounded_counted`]), counted from
/// the code: the gradient at the sample (four); two goes at the first steps ([`FIRST_MOST`]
/// each); the sure way ([`SURE_MOST`]); and its square roots at a quarter of a power each
/// ([`ROOTS_MOST`]). (Inside the hard shape: one go, from the gradient. Inside the rounded
/// shape: three gradients and no solve.)
const ROUNDED_MOST: u32 = 4 + 2 * FIRST_MOST + SURE_MOST + ROOTS_MOST.div_ceil(POWER);
/// The square roots one sample can take: two for the hard shape's distance (a cylinder's
/// radius, and the distance), three in each of up to three gradients, fifteen in each of two
/// goes at the first steps (a support's and a direction's, one a step of twelve at a power of 2,
/// and the distance's), and three in the sure way.
const ROOTS_MOST: u32 = 2 + 3 * 3 + 2 * 15 + 3;
/// What [`rounded_counted`] counts in: a power is four, a square root one.
const POWER: u32 = 4;
const ROOT: u32 = 1;
/// One go at the first steps ([`rounded_first`]): the gradient at the touching plane's foot
/// (four), the start's support (four), a start for each share (three), and ten steps of a power
/// a share and at most a start again (6 each).
const FIRST_MOST: u32 = 4 + 4 + 3 + 10 * 6;
/// The sure way ([`rounded_sure`]): eleven exponentials and logarithms before its steps (its
/// bounds, each coordinate's constants, the start), and at each step two for each step on each
/// of three shares, three for the sum and two for a step on `mu`.
const SURE_MOST: u32 = 11 + SURE_STEPS * (3 * 2 * SURE_SHARE_STEPS + 3 + 2);

/// Where a rounded shape's nearest point is first looked for ([`rounded_first`]).
#[derive(Clone, Copy)]
enum Start {
    /// The sample is inside the hard shape: from the touching plane whose normal is the
    /// gradient at the sample (that normal, the gradient's length, and how far the sample is
    /// past the plane), whose foot is brought onto the surface.
    Foot(V3, f32, f32),
    /// The sample is past the hard shape: the way out of it from its nearest point (its
    /// coordinates made positive).
    Past(V3),
}

/// How far `p` (outside, its coordinates made positive) is from a rounded shape, by the first
/// steps from one start: the distance to the nearest point of its surface when they have
/// arrived there, or else (`Err`) the `mu` they started from, for the sure way
/// ([`rounded_sure`]). Every power it takes is counted in `taken`.
///
/// The nearest point `x` is where the line to `p` leaves the surface squarely: `p - x = mu *
/// grad k(x)` with `k(x) = 1`. For each `mu` every coordinate has one answer, `u + c *
/// u^(power-1) = p / half` in shares of its half size (`c = mu / half^2`), and the sum of the
/// shares to the power falls as `mu` grows: **there is one nearest point, and the steps that
/// arrive have found it**, whatever they started from. Arriving is checked, not assumed:
/// every coordinate on its equation to a ten-millionth and the point on the surface to a
/// millionth of the power. So a start only sets how soon they arrive; one that does not
/// arrive costs its steps and hands on to the next start or the sure way, and never gives an
/// answer.
///
/// **Where it starts.** `mu` is the distance times how far the shape reaches along the
/// normal at the nearest point (`grad k . x` is 1 there, so one over the gradient's length is
/// that reach, the support `s(n) = (sum of (half_i * n_i)^q)^(1/q)` with `q = power / (power -
/// 1)`). On a thin shape the reach changes by the thinness across its rim: a tenth of a
/// millimetre along the normal of a sheet's face, fifty metres along the normal of its edge.
/// So the start is the point of the surface that reaches furthest along a guess of the
/// normal taken from where the sample lies: past the hard shape, the way out of it from its
/// nearest point; inside the hard shape, the way to the sample from the first touching
/// plane's foot brought onto the surface. At that point each share to the power less one is
/// `half_i * n_i / s(n)`, with no power to take, and `mu` is how far the sample is past the
/// plane that touches there, times `s(n)`: the start is the answer itself for every sample
/// on that point's normal, whatever the thinness. (Before, `mu` was taken along the normal at
/// the foot's own point of the surface and each share started at the most its equation
/// allows: beside the rim of a 100 m sheet a ten-thousandth as thick that normal is the
/// face's, `mu` began three hundred thousand to five million times too small, the first steps
/// went the wrong way, and 98% of the samples took the sure way: a cutter past the corner of
/// what it cut cost four times its charge. Eighth review.)
fn rounded_first(p: V3, h: V3, inv: V3, power: f32, from: Start, lower: f32, taken: &std::cell::Cell<u32>) -> Result<f32, f64> {
    let (g, len, plane) = match from {
        Start::Foot(g, len, plane) => (g, len, plane),
        Start::Past(_) => ([0.0; 3], 1.0, 1.0),
    };
    // A sample so far off that its own distance is past what the numbers hold (a coordinate
    // past 1.8e19 squares to infinity) has no nearest point to solve for: the bound is the
    // answer, as it is for a plain box there. (Solved for all the same, a share's limit came
    // out as nothing and the solve stopped the build with a panic: a recipe nested in seven
    // scales of a thousandth does it. Sixth review.)
    if !(lower.is_finite() && len.is_finite() && plane.is_finite() && p.iter().all(|x| x.is_finite())) {
        return Ok(lower);
    }
    let n = if inv[2] == 0.0 { 2 } else { 3 };
    let least = if n == 3 { h[0].min(h[1]).min(h[2]) } else { h[0].min(h[1]) };
    // (`mu` is the distance over the length of the gradient at the nearest point, which is no
    // more than one over the least half size.)
    let floor = surely_under(lower, p) * least;
    let e = power - 1.0;
    let pw = |x: f32, y: f32| {
        taken.set(taken.get() + POWER);
        power_of(x, y)
    };
    let root = |x: f32| {
        taken.set(taken.get() + ROOT);
        x.sqrt()
    };
    // The point of the surface that reaches furthest along `dir` (a unit, its coordinates
    // positive): `mu` if the nearest point were there, and its shares to the power less one.
    let support = |dir: V3| -> Option<(f32, V3)> {
        let toward = [dir[0] * h[0], dir[1] * h[1], if n == 3 { dir[2] * h[2] } else { 0.0 }];
        let q = power / e;
        let term = |x: f32| if x <= 0.0 { 0.0 } else if power == 2.0 { x * x } else { pw(x, q) };
        let sum = term(toward[0]) + term(toward[1]) + term(toward[2]);
        if !(sum > 0.0 && sum.is_finite()) {
            return None;
        }
        let reach = if power == 2.0 { root(sum) } else { pw(sum, 1.0 / q) };
        let ahead = dir[0] * p[0] + dir[1] * p[1] + dir[2] * p[2] - reach;
        Some(((ahead * reach).max(floor), toward.map(|x| x.max(0.0) / reach)))
    };
    let unit = |x: V3| {
        let size = root(math3::dot(x, x));
        (size > 0.0 && size.is_finite()).then(|| [x[0] / size, x[1] / size, x[2] / size])
    };
    let way = match from {
        Start::Past(out) => unit(out),
        Start::Foot(..) => {
            let foot = math3::sub(p, math3::scale(g, plane / len));
            rounded_gradient(foot, inv, power, taken).and_then(|(at, _)| {
                // (`k` at the foot: the foot over it is on the surface.)
                let k = math3::dot(at, foot);
                (k > 0.0 && k.is_finite()).then(|| math3::sub(p, math3::scale(foot, 1.0 / k))).and_then(unit)
            })
        }
    };
    let (mu, began) = way.and_then(support).unwrap_or((floor, [0.0; 3]));
    // From here in double precision: far from the shape `p / half` is hundreds and a share
    // is under one, so in single precision a coordinate's equation was only known to a part
    // in ten thousand, the sum never came within its tolerance, and every sample took all of
    // the steps (fourth review: a cut by a rounded plate cost five times its weight).
    let (power, e) = (f64::from(power), f64::from(e));
    let to = |x: f64, y: f64| {
        taken.set(taken.get() + POWER);
        crate::dmath::exp_f64(y * crate::dmath::ln_f64(x))
    };
    let root64 = |x: f64| {
        taken.set(taken.get() + ROOT);
        x.sqrt()
    };
    let inv2 = [f64::from(inv[0]) * f64::from(inv[0]), f64::from(inv[1]) * f64::from(inv[1]), f64::from(inv[2]) * f64::from(inv[2])];
    let big = [f64::from(p[0]) * f64::from(inv[0]), f64::from(p[1]) * f64::from(inv[1]), f64::from(p[2]) * f64::from(inv[2])];
    let (floor, mut mu) = (f64::from(floor), f64::from(mu));
    let start = mu;
    // Each coordinate's share at the last `mu`.
    let mut u = [0.0f64; 3];
    // First, Newton's steps on everything together: one power a coordinate a step, and the
    // error squared each time from a start as near as these. (Each `mu`'s shares solved for
    // in full, as below, took three times the powers.) The unknowns are `mu` and each share
    // **to the power less one**, `v = u^(power - 1)`, not the share itself: in `v` a
    // coordinate's equation is `v^(1 / (power - 1)) + c * v = p / half`, a gentle root and a
    // straight line, and being on the surface is nearly a plain sum, however high the power.
    // (In the share itself the equation holds `u^39` at the lowest roundness, the steps
    // overshot and gave up, and the sure way ran: far from a rounded needle 63% of samples
    // took over forty steps. Fifth review.) It is not sure to arrive, so it is only believed
    // when it has: every coordinate on its equation to a ten-millionth and the point on the
    // surface to a millionth of the power. Otherwise the sure way below takes over from
    // where `mu` started.
    let mut arrived = false;
    if power != 2.0 {
        // The most a coordinate's own equation allows at a `mu`, which its answer is under:
        // the lesser of what the straight term alone allows (`p / half` over `c`) and what the
        // root alone does (`p / half` to the power, where that is under one). A step that
        // would take a share to nothing or past that begins again from there. (Left at next
        // to nothing, it crept back a millionfold a step and the solve gave up.)
        let begin = |want: f64, c: f64| -> f64 {
            let line = if c > 0.0 { want / c } else { f64::MAX };
            if want < 1.0 {
                line.min(to(want, e))
            } else {
                line.min(4.0)
            }
        };
        // Each share starts at the start's point (where that is under the most its equation
        // allows), or else at that most.
        let mut v = [0.0f64; 3];
        for i in 0..n {
            if big[i] > 0.0 {
                let c = mu * inv2[i];
                let at = f64::from(began[i]);
                v[i] = if at > 0.0 && (c <= 0.0 || at <= big[i] / c) { at } else { begin(big[i], c) };
            }
        }
        for _ in 0..10 {
            // (On the surface less one; and the two sums the step on `mu` is made of.)
            let (mut value, mut pull, mut give, mut worst) = (-1.0f64, 0.0f64, 0.0f64, 0.0f64);
            let (mut off, mut stiff, mut lean) = ([0.0f64; 3], [1.0f64; 3], [0.0f64; 3]);
            for i in 0..n {
                if big[i] <= 0.0 {
                    (u[i], v[i]) = (0.0, 0.0);
                    continue;
                }
                let c = mu * inv2[i];
                // (Never nothing, and never more than either term alone allows.)
                let most = if c > 0.0 { big[i] / c } else { f64::MAX };
                // (Written so that no pair of limits can stop it: a `clamp` whose least is over
                // its most panics.)
                let x = v[i].min(most).max(1e-200);
                let share = to(x, 1.0 / e);
                off[i] = share + c * x - big[i];
                stiff[i] = share / (e * x) + c;
                lean[i] = inv2[i] * x;
                let turn = power / e * share;
                value += x * share;
                pull += turn * off[i] / stiff[i];
                give += turn * lean[i] / stiff[i];
                worst = worst.max(off[i].abs() / big[i]);
                (u[i], v[i]) = (share, x);
            }
            if !(value.is_finite() && pull.is_finite() && give.is_finite() && give > 0.0) {
                break;
            }
            let next = (mu + (value - pull) / give).max(floor);
            if value.abs() <= ARRIVED * power && worst <= ARRIVED && (next - mu).abs() <= ARRIVED_STEP * mu {
                arrived = true;
                break;
            }
            for i in 0..n {
                if big[i] > 0.0 {
                    let stepped = v[i] - (off[i] + lean[i] * (next - mu)) / stiff[i];
                    let c = next * inv2[i];
                    v[i] = if stepped > 0.0 && (c <= 0.0 || stepped <= big[i] / c) { stepped } else { begin(big[i], c) };
                }
            }
            mu = next;
        }
    } else {
        // At a power of 2 (an ellipsoid) each share has its answer at once, `p / half / (1 +
        // c)`, and Newton's steps are taken on `mu` alone: on one over the root of the sum of
        // the shares squared, less one. Each share's own one over it rises in a straight line
        // with `mu`, so that rises along a concave curve, nearly straight (straight for a
        // ball, or wherever one coordinate leads): from either side a step lands at or below
        // the answer and the next come up to it without passing it. (On the sum itself the
        // steps gain less than half again a step where it falls as a power of `mu`.)
        for _ in 0..12 {
            let (mut sum, mut slope) = (0.0f64, 0.0f64);
            for i in 0..n {
                let (c, want) = (mu * inv2[i], big[i]);
                u[i] = if want > 0.0 { want / (1.0 + c) } else { 0.0 };
                sum += u[i] * u[i];
                slope -= 2.0 * u[i] * u[i] * inv2[i] / (1.0 + c);
            }
            if !(sum > 0.0 && sum.is_finite() && slope < 0.0 && slope.is_finite()) {
                break;
            }
            let root = root64(sum);
            let value = 1.0 / root - 1.0;
            let rise = -0.5 * slope / (sum * root);
            let next = (mu - value / rise).max(floor);
            if value.abs() <= ARRIVED && (next - mu).abs() <= ARRIVED_STEP * mu {
                arrived = true;
                break;
            }
            mu = next;
        }
    }
    if arrived {
        let off = [f64::from(p[0]) - u[0] * f64::from(h[0]), f64::from(p[1]) - u[1] * f64::from(h[1]), if n == 3 { f64::from(p[2]) - u[2] * f64::from(h[2]) } else { 0.0 }];
        return Ok((root64(off[0] * off[0] + off[1] * off[1] + off[2] * off[2]) as f32).max(lower));
    }
    Err(start)
}

/// A lower bound of the distance from `p`, worked out in single precision (the hard shape's
/// distance, or a touching plane's: `(grad k . p - 1) / |grad k|`, a difference of two
/// numbers near one close to the surface), made sure to be under the distance: less what the
/// rounding of `p`'s coordinates can have added (a few of their last places), and never less
/// than a thousandth of itself (where that much is all it is, the sample is on the surface to
/// within its own rounding). Where the bound is the distance itself (a ball's touching plane,
/// the middle of a face) its rounding can put it over the answer, and a floor over the answer
/// held the first steps away from it: 5.5% of the samples outside a ball of roundness 1, on a
/// build's grid at a resolution of 96, took the sure way for it.
fn surely_under(lower: f32, p: V3) -> f32 {
    let rounding = 8.0 * f32::EPSILON * (p[0].abs() + p[1].abs() + p[2].abs());
    (lower - rounding).max(lower * 1e-3).max(0.0)
}

/// How near the first steps must come to be believed: every coordinate on its equation to this
/// share of it, and the sum of the shares to the power within this much of one (times the
/// power); and the next step on `mu` under [`ARRIVED_STEP`] of it, which is what says how near
/// `mu` is (close to the surface the sum is nearly level in `mu`; the distance is out by about
/// the share `mu` is). (With the rule a sum within a millionth and nothing of the step, a
/// sample a hundredth of a millimetre past a rod's end at a roundness of 0.2 came out at the
/// hard shape's distance, 4% short, beside samples that were right: the step scan found it.)
const ARRIVED: f64 = 1e-9;
const ARRIVED_STEP: f64 = 1e-7;

/// The most steps the sure way takes on `mu` ([`rounded_sure`]), and on a coordinate's share
/// at each of them.
const SURE_STEPS: u32 = 64;
const SURE_SHARE_STEPS: u32 = 10;

/// The sure way to a rounded shape's nearest point from `p` (outside, coordinates made
/// positive), when the first steps have not arrived: the distance, with every exponential and
/// every logarithm it takes counted in `taken` as a power. **It cannot fail to arrive**:
///
/// For each `mu` a coordinate's share has one answer (its equation `u + c u^(power-1) = p /
/// half`, `c = mu / half^2`, rises in `u`), and the sum of the shares to the power falls as `mu`
/// grows, so the nearest point is the one `mu` where that sum is one. That `mu` lies between two
/// bounds known before a step is taken: the distance times the shape's least half size (the
/// shape holds the ball of that radius, so it reaches at least that far along any normal) and
/// a bound on the distance (`upper`: past the hard shape its distance and the most the hard
/// shape's surface is from the rounded one, [`rounded_slack`]) times the half sizes' length (it
/// reaches no further). The steps are taken on the logarithm of `mu`, on the logarithm of the
/// sum (on a thin shape the sum falls as a power of `mu` over many decades, which in these is a
/// straight line: a step on `mu` itself gained less than a doubling there, which is why the
/// sure way before this one stopped short, 56 times short at a plank's rim; ninth review), each
/// kept inside the bounds, which close in at every step; a step that would leave them, or that
/// has not halved them in two steps, is a halving of them instead. So they come to the answer
/// whatever the start, and the start only sets how soon.
///
/// Each share is solved for in logarithms too: `s = ln(u / big)` from the lesser of what each
/// term alone allows (where the sum `e^s + C e^((power-1) s) - 1` is never below nothing and at
/// most one, and is convex), by Newton's steps, which come down to the answer without passing
/// it. The distance is `|p| * share of the line term` per coordinate, taken without
/// cancelling.
fn rounded_sure(p: V3, h: V3, inv: V3, power: f32, lower: f64, start: f64, taken: &std::cell::Cell<u32>) -> f64 {
    let count = |k: u32| taken.set(taken.get() + k);
    let exp = |x: f64| {
        count(POWER);
        crate::dmath::exp_f64(x)
    };
    let ln = |x: f64| {
        count(POWER);
        crate::dmath::ln_f64(x)
    };
    let root = |x: f64| {
        count(ROOT);
        x.sqrt()
    };
    let n = if inv[2] == 0.0 { 2 } else { 3 };
    let (power, e) = (f64::from(power), f64::from(power) - 1.0);
    let half = [0, 1, 2].map(|i| if i < n { f64::from(h[i]) } else { 0.0 });
    let big = [0, 1, 2].map(|i| f64::from(p[i]) * f64::from(inv[i]));
    let least = if n == 3 { half[0].min(half[1]).min(half[2]) } else { half[0].min(half[1]) };
    let reach = root(half[0] * half[0] + half[1] * half[1] + half[2] * half[2]);
    // Past the hard shape by its distance (none inside it), and the most its surface is from
    // the rounded one: a bound on the distance.
    let hard = {
        let d = [0, 1, 2].map(|i| (f64::from(p[i]) - half[i]).max(0.0));
        root(d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
    };
    let upper = hard + reach * (1.0 - exp(-ln(n as f64) / power));
    // (The lower bound made sure, as for the first steps' floor: [`surely_under`].)
    let (mut lo, mut hi) = (ln(f64::from(surely_under(lower as f32, p)).max(f64::MIN_POSITIVE) * least), ln(upper * reach));
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return lower;
    }
    // Each coordinate's `ln(c / mu)` and the logarithm of how far out it is.
    let mut kappa = [0.0f64; 3];
    let mut log_big = [0.0f64; 3];
    for i in 0..n {
        if big[i] > 0.0 {
            log_big[i] = ln(big[i]);
            kappa[i] = (e - 1.0) * log_big[i] - 2.0 * ln(half[i]);
        }
    }
    // At a `mu` (its logarithm): the logarithm of the sum of the shares to the power, its slope
    // in the logarithm of `mu`, and each coordinate's share of its line term (`c u^(power-1) /
    // big`, which is one less the share over `big`).
    let mut last_s = [0.0f64; 3];
    let mut last_at = f64::NEG_INFINITY;
    let at = |lm: f64, last_s: &mut [f64; 3], last_at: &mut f64| -> (f64, f64, [f64; 3]) {
        let mut weight = [f64::NEG_INFINITY; 3];
        let mut lean = [0.0f64; 3];
        let mut line = [0.0f64; 3];
        for i in 0..n {
            if big[i] <= 0.0 {
                continue;
            }
            let lc = lm + kappa[i];
            let (s, t) = if e == 1.0 {
                // (An ellipsoid's share: `1 / (1 + C)`, at once.)
                let small = exp(-lc.abs());
                let t = if lc > 0.0 { 1.0 / (1.0 + small) } else { small / (1.0 + small) };
                (-(lc.max(0.0) + ln(1.0 + small)), t)
            } else {
                // From the last answer, or the first time from the lesser of what each term
                // allows (`top`, which the answer is under). A step from below the answer lands
                // above it (the sum is convex in `s`), and none is let past `top`; from above
                // they come down to it without passing it.
                let top = (-lc / e).min(0.0);
                let mut s = if last_at.is_finite() { last_s[i].min(top) } else { top };
                let mut t = 0.0;
                for _ in 0..SURE_SHARE_STEPS {
                    let a = exp(s);
                    t = exp(e * s + lc);
                    let step = (a + t - 1.0) / (a + e * t);
                    // (Done when the step is down to the last places of `s`, where rounding
                    // alone moves the sum.)
                    if step.abs() <= 1e-14 * (1.0 + s.abs()) {
                        break;
                    }
                    s = (s - step).min(top);
                }
                (s, t)
            };
            last_s[i] = s;
            line[i] = t;
            weight[i] = power * (log_big[i] + s);
            lean[i] = -power * t / ((1.0 - t) + e * t);
        }
        *last_at = lm;
        let top = weight.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut sum = 0.0;
        let mut slope = 0.0;
        for i in 0..n {
            if weight[i] > f64::NEG_INFINITY {
                let w = if weight[i] == top { 1.0 } else { exp(weight[i] - top) };
                sum += w;
                slope += w * lean[i];
            }
        }
        (top + ln(sum), slope / sum, line)
    };
    let mut lm = if start > 0.0 && start.is_finite() { ln(start).clamp(lo, hi) } else { 0.5 * (lo + hi) };
    let mut line = [0.0f64; 3];
    // (The last two steps' sizes and the bounds' widths after them.)
    let mut steps = [f64::INFINITY; 2];
    let mut widths = [f64::INFINITY; 2];
    for _ in 0..SURE_STEPS {
        let (value, slope, now) = at(lm, &mut last_s, &mut last_at);
        line = now;
        if !value.is_finite() {
            break;
        }
        if value > 0.0 {
            lo = lm;
        } else {
            hi = lm;
        }
        let mut newton = lm - value / slope;
        if !(newton > lo && newton < hi) && value > 0.0 {
            // Short of the answer, where a step in the logarithms would leave the bounds (where
            // the sum is nearly level in them, close to the surface): a step of Newton's on
            // `mu` itself, which comes up toward the answer without passing it (the sum falls
            // along a convex curve in `mu`).
            newton = lm + ln(1.0 - (1.0 - exp(-value)) / slope);
        }
        // A step of Newton's is taken when it stays inside the bounds and either it is under
        // half the step before last or the bounds are under half what they were two steps ago;
        // otherwise the bounds are halved.
        let shrinking = (newton - lm).abs() <= 0.5 * steps[0] || hi - lo <= 0.5 * widths[0];
        let next = if newton > lo && newton < hi && shrinking { newton } else { 0.5 * (lo + hi) };
        let tol = 1e-13 * (1.0 + lm.abs());
        if value == 0.0 || (next - lm).abs() <= tol || hi - lo <= tol {
            break;
        }
        steps = [steps[1], (next - lm).abs()];
        widths = [widths[1], hi - lo];
        lm = next;
    }
    let off = [0, 1, 2].map(|i| f64::from(p[i]) * line[i]);
    root(off[0] * off[0] + off[1] * off[1] + off[2] * off[2])
}

/// `x` (more than nothing) to the power `y`, for a rounded shape's shares: the exponential of
/// `y` logarithms, in double precision (the same on every machine, as [`crate::dmath`] is,
/// and two thirds of the time of its correctly rounded power).
fn power_of(x: f32, y: f32) -> f32 {
    crate::dmath::exp_f64(f64::from(y) * crate::dmath::ln_f64(f64::from(x))) as f32
}

/// The gradient of a rounded shape's measure `k` at `x` (in the shape's own two or three
/// coordinates), and its length: the normal of the plane that touches the shape along the
/// line from the middle through `x`, which lies `1 / length` from the middle. `None` at the
/// middle itself. Each share is taken of the largest, so no power overflows or underflows.
/// Every power and square root it takes is counted in `taken` (at most a power a coordinate and
/// one more, and three square roots).
fn rounded_gradient(x: V3, inv: V3, power: f32, taken: &std::cell::Cell<u32>) -> Option<(V3, f32)> {
    // (`inv` is one over each half size, and nothing for a cylinder's third.)
    let u = [x[0].abs() * inv[0], x[1].abs() * inv[1], x[2].abs() * inv[2]];
    let m = u[0].max(u[1]).max(u[2]);
    if m <= 0.0 || !m.is_finite() {
        return None;
    }
    // Each share of the largest, to the power less one (`t`), and the sum of the shares to the
    // power (`sum`, between 1 and 3).
    let over = 1.0 / m;
    let r = [u[0] * over, u[1] * over, u[2] * over];
    let (t, sum, root);
    if power == 2.0 {
        t = r;
        sum = r[0] * r[0] + r[1] * r[1] + r[2] * r[2];
        taken.set(taken.get() + ROOT);
        root = sum.sqrt();
    } else if power == 4.0 {
        t = [r[0] * r[0] * r[0], r[1] * r[1] * r[1], r[2] * r[2] * r[2]];
        sum = t[0] * r[0] + t[1] * r[1] + t[2] * r[2];
        taken.set(taken.get() + 2 * ROOT);
        root = sum.sqrt().sqrt();
    } else {
        let e = power - 1.0;
        let pw = |x: f32, y: f32| {
            taken.set(taken.get() + POWER);
            power_of(x, y)
        };
        // (The largest share is 1 and a share of nothing is nothing: no power to take.)
        let share = |x: f32| if x >= 1.0 { 1.0 } else if x <= 0.0 { 0.0 } else { pw(x, e) };
        t = [share(r[0]), share(r[1]), share(r[2])];
        sum = t[0] * r[0] + t[1] * r[1] + t[2] * r[2];
        root = pw(sum, 1.0 / power);
    }
    // k = m * root; grad k = (root / sum) * t_i / half_i, toward the side x is on.
    let scale = root / sum;
    let side = |i: usize| if x[i] < 0.0 { -scale } else { scale };
    let g = [side(0) * t[0] * inv[0], side(1) * t[1] * inv[1], side(2) * t[2] * inv[2]];
    taken.set(taken.get() + ROOT);
    let len = math3::length(g);
    if len > 0.0 && len.is_finite() {
        Some((g, len))
    } else {
        None
    }
}

/// How far a rounded box's or cylinder's field may be from a field that changes no faster
/// than distance ([`rounded`]): no further than the corner of its hard shape is from its
/// rounded surface, which is no more than the corner's distance along the line to the middle,
/// `|half| * (1 - n^(-1/power))` for a shape of `n` sides to a corner (3, or a cylinder's 2).
/// Next to nothing for a nearly hard shape, and 0.42 of the half diagonal for an ellipsoid.
fn rounded_slack(half: V3, power: f32, disc: bool) -> f32 {
    let (reach, sides) = if disc { (len2(half[0], half[1]), 2.0) } else { (math3::length(half), 3.0) };
    reach * (1.0 - crate::dmath::pow(sides, -1.0 / power))
}

/// A column's field ([`Prim::Column`]): how many of its own radii out a point is from the
/// axis, less one, times the smaller radius there (so it is never more than the distance).
fn column(p: V3, a: V3, b: V3, side: V3, sections: &[[f32; 3]], square: f32) -> f32 {
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
    // How many of the section's own radii out the point is, across the column: an ellipse's
    // measure, or a squarer one (the sum of the two shares each raised to `square`, and its
    // root). A squarer section holds the ellipse, so the field is still never more than the
    // distance across; along the column it changes as an ellipse's does.
    let across2 = if square == 2.0 {
        (x / rx) * (x / rx) + (z / rz) * (z / rz)
    } else if square == 4.0 {
        // (No power to take: the root of the root of the fourth powers' sum, squared.)
        let (u, w) = ((x / rx) * (x / rx), (z / rz) * (z / rz));
        (u * u + w * w).sqrt()
    } else {
        let (u, w) = ((x / rx).abs(), (z / rz).abs());
        // (A cube is two multiplications; any other squareness takes a power each.)
        let sum = if square == 3.0 { u * u * u + w * w * w } else { crate::dmath::pow(u, square) + crate::dmath::pow(w, square) };
        let across = if sum > 0.0 { crate::dmath::pow(sum, 1.0 / square) } else { 0.0 };
        across * across
    };
    let k = (across2 + (beyond / cap) * (beyond / cap)).sqrt();
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
    near_of(kids.len(), smooth, |i| of(&kids[i]))
}

/// [`near`] over `count` children whose distances (and colours) `of` gives by their place.
fn near_of(count: usize, smooth: f32, mut of: impl FnMut(usize) -> (f32, Rgba)) -> (f32, Rgba) {
    let mut few = [(0.0f32, [0.0f32; 4]); 16];
    let mut many = Vec::new();
    let found: &mut [(f32, Rgba)] = if count <= few.len() {
        &mut few[..count]
    } else {
        many.resize(count, (0.0, [0.0; 4]));
        &mut many
    };
    let mut nearest = f32::MAX;
    for (i, slot) in found.iter_mut().enumerate() {
        *slot = of(i);
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
        PKind::Group { op, kids, smooth } => combine(*op, *smooth, kids.len(), |i| eval(&kids[i], q)),
    };
    finish(n, d, q)
}

/// A group's distance from its `count` children's, which `of` gives by their place (each
/// asked once, in order): what [`eval`] does with them, step for step.
fn combine(op: Combine, smooth: f32, count: usize, mut of: impl FnMut(usize) -> f32) -> f32 {
    if op == Combine::Near {
        return near_of(count, smooth, |i| (of(i), [0.0; 4])).0;
    }
    let mut acc = if count > 0 { of(0) } else { f32::MAX };
    for i in 1..count {
        let b = of(i);
        acc = match op {
            Combine::Union | Combine::Near => smin(acc, b, smooth).0,
            Combine::Intersection => -smin(-acc, -b, smooth).0,
            Combine::Difference => -smin(-acc, b, smooth).0,
        };
    }
    acc
}

/// A node's distance at a point (in its parent's frame) and, for a group [`prune`] looks
/// into, each child's at the same point (in the group's own frame): **every node is evaluated
/// once**, so finding what is near a ball costs one evaluation of the tree, which is what it
/// is charged. (`prune` evaluated each child to decide, then pruned the ones it kept, which
/// evaluated their own children again: a primitive under a hundred and twenty plain unions was
/// evaluated about a hundred and twenty times for each time it was charged. Ninth review.) The
/// distance is [`eval`]'s, bit for bit: the same steps in the same order.
struct Valued {
    d: f32,
    kids: Vec<Valued>,
}

fn valued(n: &Prepared, p: V3) -> Valued {
    match &n.kind {
        PKind::Group { op, kids, smooth } if plain(&n.m) => {
            let q = local(n, p);
            let kids: Vec<Valued> = kids.iter().map(|k| valued(k, q)).collect();
            let d = combine(*op, *smooth, kids.len(), |i| kids[i].d);
            Valued { d: finish(n, d, q), kids }
        }
        _ => Valued { d: eval(n, p), kids: Vec::new() },
    }
}

/// No modifier that keeps [`prune`] out of a group (a repeat, an elongation, a twist, a bend,
/// a shell or a displacement: its children's distances no longer bound each other).
fn plain(m: &Modifiers) -> bool {
    m.repeat.is_none() && m.elongate == [0.0; 3] && m.twist == 0.0 && m.bend == 0.0 && m.shell <= 0.0 && m.displace.is_none()
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

/// Work units ([`crate::work`], â‰ˆ ns) of one evaluation of a node and everything under it,
/// weighed by the `calibrate_sdf` test: noise octaves and trigonometry cost far more than a
/// sphere.
pub fn eval_cost(n: &SdfNode) -> u64 {
    match &n.kind {
        Kind::Prim(p) => prim_cost(p) + modifiers_cost(&n.m),
        Kind::Group { kids, smooth, .. } => group_cost(kids.iter().map(eval_cost).sum(), kids.len(), *smooth) + modifiers_cost(&n.m),
    }
}

/// Triangles of one sculpt's mesh each asked which primitive made it
/// ([`primitive_of_triangles`]): every triangle of an ordinary sculpt (a measured man's whole
/// figure is about 8,000; a character of capsules at resolution 64 about 7,000; thirty balls at
/// resolution 64, 11,664). A mesh of more is asked at every so many of its triangles, each
/// answer standing for those after it until the next: the mesher writes its triangles block by
/// block, so a run of them lies together, and only the runs that cross from one primitive to
/// another are given wrongly. Measured (2026-11-20) on a woman's figure body (16,492 triangles)
/// against every triangle asked: asked every other, no primitive's share of the surface moved
/// by more than 0.1% of the whole (8% of its own, for one of 1% or more); every fourth, 0.35%
/// (15%); every eighth, 0.7% (30%). A mesh of up to twice this is asked every other triangle.
pub const MAX_OWNER_ASKED: usize = 16_384;
/// The work ([`eval_cost`]'s units) all the sculpts of one build may spend asking their
/// triangles which primitive made them. **It is a limit of its own and never the build's**, so
/// the record never refuses a build: what is past it is not asked, and those triangles are no
/// record's (the rest; the sculpt's stat says how many). A query costs what its descent
/// evaluates (each group's children on the way down to one primitive), at the weights the
/// mesher is charged by. Measured on the reference laptop while other builds ran (2026-11-20):
/// a figure's tree takes about a nanosecond a unit, so this is about an eighth of a second at
/// the most; a woman's figure body asked every other triangle spends 27 million, a man's whole
/// figure 26 million; 52 sculpts of 32 ellipsoids at resolution 48 (70,200 triangles) reach it
/// with 11% of their triangles unasked.
pub const MAX_OWNER_BUILD_WORK: u64 = 120_000_000;

/// Which primitive (numbered in the order they are met depth first: every primitive counted, a
/// half space and what a difference takes away among them) made each triangle of `shape` (the
/// tree's mesh, in the tree's frame): the one whose field is the tree's at the triangle's
/// middle (in a union the nearest; in an intersection the furthest; in a difference the first
/// child where it stands and a later one where it cut, which is that later one's). In a
/// smooth blend the nearest takes the blend. Every triangle is asked up to `max_asked`
/// ([`MAX_OWNER_ASKED`] in a build); past it, every so many, each answer standing for the
/// triangles after it until the next. `max_work` bounds the work: a triangle not answered
/// within it is [`crate::shape::NO_OWNER`]. The work spent never passes `max_work`.
pub fn primitive_of_triangles(tree: &SdfNode, seed: u64, shape: &crate::shape::Shape, max_asked: usize, max_work: u64) -> Asked {
    let none = crate::shape::NO_OWNER;
    let mut asked = Asked { of: vec![none; shape.tris.len()], work: 0, asked: 0 };
    let prepared = prepare(tree, seed, &mut 0);
    let counts = Counted::of(&prepared);
    if shape.tris.is_empty() || counts.prims == 0 {
        return asked;
    }
    let stride = shape.tris.len().div_ceil(max_asked.max(1)).max(1);
    let p = |i: u32| shape.positions.get(i as usize).copied().unwrap_or([0.0; 3]);
    let mut last = none;
    // (Rounded shapes' powers past their weight are charged as a build's are: [`RoundedBill`].)
    let mut bill = RoundedBill::start();
    for (i, t) in shape.tris.iter().enumerate() {
        if i % stride == 0 {
            // (A cancelled build stops asking: it fails at its next charge.)
            if asked.asked % 4096 == 4095 && crate::work::cancelled() {
                break;
            }
            let mid = math3::scale(math3::add(math3::add(p(t[0]), p(t[1])), p(t[2])), 1.0 / 3.0);
            let mut cost = 0u64;
            let who = counts.owner(mid, 0, &mut cost);
            cost = cost.saturating_add(bill.due());
            // (A question past the limit is not counted, nor its answer kept: the limit holds.)
            if asked.work.saturating_add(cost) > max_work {
                break;
            }
            asked.work += cost;
            asked.asked += 1;
            last = who as u32;
        }
        asked.of[i] = last;
    }
    // (Nothing of these questions is left in this thread's tally.)
    rounded_taken();
    asked
}

/// What [`primitive_of_triangles`] found.
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    /// Each triangle's primitive ([`crate::shape::NO_OWNER`]: not answered).
    pub of: Vec<u32>,
    /// The work spent ([`eval_cost`]'s units).
    pub work: u64,
    /// The triangles asked.
    pub asked: usize,
}

/// A prepared tree with how many primitives each node holds, and what evaluating it costs.
struct Counted<'a> {
    n: &'a Prepared,
    prims: usize,
    cost: u64,
    kids: Vec<Counted<'a>>,
}

impl<'a> Counted<'a> {
    fn of(n: &'a Prepared) -> Counted<'a> {
        match &n.kind {
            PKind::Prim(p) => Counted { n, prims: 1, cost: prim_cost(p) + modifiers_cost(&n.m), kids: Vec::new() },
            PKind::Group { kids, smooth, .. } => {
                let kids: Vec<Counted> = kids.iter().map(Counted::of).collect();
                let cost = group_cost(kids.iter().map(|k| k.cost).sum(), kids.len(), *smooth) + modifiers_cost(&n.m);
                Counted { n, prims: kids.iter().map(|k| k.prims).sum(), cost, kids }
            }
        }
    }

    /// The primitive (numbered from `base`) whose field is this node's at `p` (in the node's
    /// parent's frame); `cost` gets what was evaluated to find it.
    fn owner(&self, p: V3, base: usize, cost: &mut u64) -> usize {
        let PKind::Group { op, .. } = &self.n.kind else { return base };
        if self.kids.is_empty() {
            return base;
        }
        let q = local(self.n, p);
        let mut best = (0usize, 0.0f32);
        for (i, k) in self.kids.iter().enumerate() {
            let d = eval(k.n, q);
            *cost = cost.saturating_add(k.cost);
            // (What decides: the least in a union, the most in an intersection; in a
            // difference the most of the first and the others turned inside out.)
            let v = match op {
                Combine::Union | Combine::Near => -d,
                Combine::Intersection => d,
                Combine::Difference => {
                    if i == 0 {
                        d
                    } else {
                        -d
                    }
                }
            };
            if i == 0 || v > best.1 {
                best = (i, v);
            }
        }
        let before: usize = self.kids[..best.0].iter().map(|k| k.prims).sum();
        self.kids[best.0].owner(q, base + before, cost)
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
        // Measured with the rest (`calibrate_sdf`, three times in release, 2026-11-18): an
        // ellipse's column and a squareness of 4 (roots only) 49 to 52 ns; a squareness of 3
        // (one power) 106 to 113; any other (three powers) 207 to 212. Each weight with the
        // primitive's 48 is at least one and a half times the slowest.
        Prim::Column { square: 2.0 | 4.0, .. } => 38,
        Prim::Column { square: 3.0, .. } => 122,
        Prim::Column { .. } => 270,
        Prim::Box { .. } | Prim::Torus { .. } | Prim::Ellipsoid { .. } => 12,
        // (Measured with the rest, three times in release, 2026-11-19, at points in and near
        // the shape, where the closest-point steps are taken: a power of 2 or 4 takes roots
        // only, 153 to 171 ns; any other takes a power for each share but the largest and one
        // more, at each step: 369 to 781 ns. Before the steps they were 27 and 175 ns: what a
        // swelling that is right at a thin board's corners costs. What a sample takes past
        // its weight is charged as it is taken: [`ROUNDED_COST`].)
        Prim::Rounded { power: 2.0, .. } => ROUNDED_COST[0],
        Prim::Rounded { .. } => ROUNDED_COST[1],
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

/// How far a primitive's field may be from one that changes no faster than its lip says: a
/// rounded box's or cylinder's ([`rounded_slack`]); nothing for the rest.
fn prim_slack(p: &Prim) -> f32 {
    match p {
        Prim::Rounded { half, power, disc } => rounded_slack(*half, *power, *disc),
        _ => 0.0,
    }
}

fn lip_slack(n: &SdfNode) -> (f32, f32) {
    let m = &n.m;
    let (mut lip, mut slack) = match &n.kind {
        Kind::Prim(p) => (prim_lip(p), prim_slack(p)),
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

/// A value past every field a tree can give, in place of an infinity (so that `a - b` of two
/// such is never NaN).
const UNKNOWN: f32 = 1e30;

/// The most copies of a repeated node one ball of [`field_range`] may meet, all repeats under
/// it together; past it the walk says nothing ([`UNKNOWN`] both ways).
const RANGE_COPIES: u32 = 8;

/// What the box's search ([`clear_of_solid`], [`field_range`]) did, counted two ways.
///
/// - `asks`: what its budget ([`CLEAR_ASKS`]) is counted in, as it was when the second review
///   found every box it gives sound: a primitive's range one, a column's nine, a tree's
///   evaluation one a primitive. Kept as it was, so that no box moves.
/// - `work`: what it costs, in [`eval_cost`]'s units, which the build is charged as it is
///   spent. Each step is weighed by what it does:
///   - every node a ball's range passes through, [`RANGE_FRAME`] (into its frame and out), and
///     for each copy of it the ball meets, its modifiers' weight as an evaluation is charged
///     them ([`modifiers_cost`]) and [`own_range_cost`]:
///   - a primitive: its evaluation's weight ([`prim_cost`]: it is evaluated once at the ball's
///     middle; an ellipsoid's range is a few roots, under its weight); a column, also
///     [`COLUMN_RANGE`] (an exponential, and its distance from its box); a rounded shape,
///     [`ROUNDED_RANGE`] (its slack);
///   - a group: what an evaluation of it costs over its children ([`group_cost`]), twice (a
///     chain of the least values and one of the greatest), and [`RANGE_GROUP`] (the list of its
///     children's ranges);
///   - a column's walk of its curve ([`column_walk_cost`]), once a node, before the search;
///   - an evaluation of the tree at a box's middle, the tree's [`prepared_cost`];
///   - and what rounded shapes take past their weight, as a build is charged it
///     ([`RoundedBill`], in [`mesh`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Spent {
    asks: u64,
    work: u64,
    /// (What [`mesh_box_charged`] gives back only: the work its reach was charged.)
    reach: u64,
}

/// What the box's search charges its work to (in [`mesh`], the build's charge), an error ending
/// the search with it.
type Spend<'a> = dyn FnMut(u64) -> Result<(), SdfError> + 'a;
/// The box [`mesh_box_charged`] gives, and what its search spent.
type Searched = ((V3, V3), Spent);

// The weights of the search's steps, set against the mesher's own (2026-11-24, release, by a
// probe of the box lane's, three runs on a machine busy with other builds): fourteen trees
// (a ball; eight balls, sharp and smooth; a turned flat ellipsoid; rounded boxes at powers 2
// and 3; a twisted box; a repetition; columns of eight sections at squareness 1.5, 2, 3 and 5,
// with and without changing proportions; eight such columns joined), each ball's range timed
// against an evaluation of the same tree at the same points. With these weights a range
// takes 0.3 to 1.3 times an evaluation's time a unit (most under 0.9: a node's frame took
// about 110 to 140 units' time over its evaluation's weight, a smooth group's list and joins
// up to 140 more, a rounded shape's slack a power), and a column's walk 1.3 to 2.4 ns a unit
// where an evaluation took 1.6 to 5.8. Timed, so only a scale: what the tests hold is that
// every step counted is charged ([`Spent`]).
/// The work of taking a ball into a node's frame and its range out of it ([`Spent`]).
const RANGE_FRAME: u64 = 160;
/// The work of a group's list of its children's ranges, besides its joins ([`Spent`]).
const RANGE_GROUP: u64 = 200;
/// A column's range beyond its evaluation: an exponential and its distance from its box.
const COLUMN_RANGE: u64 = 96;
/// A rounded shape's range beyond its evaluation: its slack, a power ([`rounded_slack`]).
const ROUNDED_RANGE: u64 = 200;
/// One support of a column's section in its walk ([`section_support`]): a root at a
/// squareness of 2, three powers at any other.
const SUPPORT_WORK: [u64; 2] = [24, 260];
/// One piece of a column's curve in its walk (its controls in double precision, its ratios).
const PIECE_WORK: u64 = 600;

/// The work of a ball's range at one copy of a node, besides its frame and its children
/// ([`Spent`]).
fn own_range_cost(n: &Prepared) -> u64 {
    match &n.kind {
        PKind::Prim(p @ Prim::Column { .. }) => prim_cost(p) + COLUMN_RANGE,
        PKind::Prim(p @ Prim::Rounded { .. }) => prim_cost(p) + ROUNDED_RANGE,
        PKind::Prim(p) => prim_cost(p),
        PKind::Group { kids, smooth, .. } => 2 * group_cost(0, kids.len(), *smooth) + RANGE_GROUP,
    }
}

/// The work of one ball's range through the whole tree, meeting one copy of everything
/// (what [`clear_of_solid`] charges before each ball: a ball that meets more copies is
/// charged the rest after).
fn range_cost(n: &Prepared) -> u64 {
    let kids = match &n.kind {
        PKind::Prim(_) => 0,
        PKind::Group { kids, .. } => kids.iter().map(range_cost).sum(),
    };
    RANGE_FRAME + modifiers_cost(&n.m) + own_range_cost(n) + kids
}

/// The work of a walk of a column's curve with `sections` sections ([`column_outline`]): the
/// supports it takes of its sections ([`column_supports`]) and its pieces.
fn column_walk_cost(sections: usize, square: f32) -> u64 {
    let segments = (sections.max(1) - 1).max(1) as u64;
    column_supports(sections) * SUPPORT_WORK[usize::from(square != 2.0)] + segments * COLUMN_PIECES as u64 * PIECE_WORK
}

/// The most supports ([`section_support`]) a walk of a column's curve takes: its two ends on
/// three axes, and on each piece of each segment and each axis six (four for its controls'
/// shape, two for its ratios').
fn column_supports(sections: usize) -> u64 {
    let segments = (sections.max(1) - 1).max(1) as u64;
    6 + segments * COLUMN_PIECES as u64 * 3 * 6
}

/// How fast a column's measure may change across it, a radius at a time: one for an ellipse
/// or a squarer section, `2^(1/s - 1/2)` for a rounder one ([`inner_range`]).
fn column_across(square: f32) -> f32 {
    if square < 2.0 { crate::dmath::pow(2.0, 1.0 / square.max(0.5) - 0.5) } else { 1.0 }
}

/// A column's [`Outline`]: one walk of its curve.
fn column_outline(a: V3, b: V3, side: V3, used: &[[f32; 3]], square: f32) -> Outline {
    let t = column_table(a, b, side, used, square);
    Outline { lo: t.zero.0, hi: t.zero.1, ratio: t.ratio, across: column_across(square) }
}

/// Walks every column of a prepared tree once ([`Outline`]), each charged its walk
/// ([`column_walk_cost`]) through `spend` before it is taken, a cancel looked for before each.
fn outline_columns(n: &mut Prepared, spend: &mut Spend) -> Result<(), SdfError> {
    match &mut n.kind {
        PKind::Prim(Prim::Column { a, b, side, sections, count, square }) => {
            let used = &sections[..(*count as usize).clamp(1, MAX_SECTIONS)];
            if crate::work::cancelled() {
                return Err(SdfError::cancelled());
            }
            spend(column_walk_cost(used.len(), *square))?;
            n.outline = Some(column_outline(*a, *b, *side, used, *square));
        }
        PKind::Prim(_) => {}
        PKind::Group { kids, .. } => {
            for k in kids {
                outline_columns(k, spend)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// (Tests only) what the box's search and its reach did on this thread, counted where each
    /// step is done and apart from what charges it: what their charge has to cover ([`Did`]).
    static DID: std::cell::RefCell<Did> = const { std::cell::RefCell::new(Did::NONE) };
}

/// (Tests only) the steps of the box's search and of [`reach`], by kind, each counted where it
/// is done: a test holds the charge to every kind's count times its weight, so that a weight
/// left out of the charge fails it (third review of the search: with any one weight left out,
/// every test still passed).
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Did {
    /// Nodes a ball's range passed through (into their frames and out).
    range_nodes: u64,
    /// The copies of nodes a range met with each modifier: turned, stretched, repeated,
    /// twisted, bent, displaced; and the displacements' octaves.
    mods: [u64; 6],
    octaves: u64,
    /// The primitives a range asked, by the kinds [`prim_cost`] weighs apart ([`prim_kind`]),
    /// and of those, columns and rounded shapes.
    range_prims: [u64; PRIM_KINDS],
    column_ranges: u64,
    rounded_ranges: u64,
    /// Groups a range passed through, and their children joined smoothly and sharply.
    groups: u64,
    kids_smooth: u64,
    kids_sharp: u64,
    /// Supports of columns' sections taken (by a root, by powers), and pieces of their curves.
    supports_root: u64,
    supports_pow: u64,
    pieces: u64,
    /// Evaluations of the whole tree at a box's middle.
    mid_evals: u64,
    /// Nodes [`reach`] asked, their children, a smooth union's pairs, and places of columns'
    /// tables asked at a level.
    reach_nodes: u64,
    reach_kids: u64,
    reach_pairs: u64,
    table_places: u64,
    /// Levels asked of columns' tables (each charged its table's most places before it is
    /// asked: what the reach's charge may pass what it did by).
    table_asks: u64,
}

/// (Tests only) the kinds of primitive [`prim_cost`] weighs apart.
#[cfg(test)]
const PRIM_KINDS: usize = 13;

/// (Tests only) which of [`PRIM_KINDS`] a primitive is, in the order of `prim_cost`'s arms:
/// a half space; a ball or a capsule; a round cone; a column at squareness 2 or 4, at 3, at
/// any other; a box, a torus, an ellipsoid; a rounded shape at power 2, at any other; a
/// straight cylinder; a tapered one.
#[cfg(test)]
fn prim_kind(p: &Prim) -> usize {
    match *p {
        Prim::HalfSpace { .. } => 0,
        Prim::Sphere { .. } | Prim::Capsule { .. } => 1,
        Prim::RoundCone { .. } => 2,
        Prim::Column { square: 2.0 | 4.0, .. } => 3,
        Prim::Column { square: 3.0, .. } => 4,
        Prim::Column { .. } => 5,
        Prim::Box { .. } => 6,
        Prim::Torus { .. } => 7,
        Prim::Ellipsoid { .. } => 8,
        Prim::Rounded { power: 2.0, .. } => 9,
        Prim::Rounded { .. } => 10,
        Prim::Cylinder { radius, radius_top, .. } if radius == radius_top => 11,
        Prim::Cylinder { .. } => 12,
    }
}

#[cfg(test)]
impl Did {
    const NONE: Did = Did { range_nodes: 0, mods: [0; 6], octaves: 0, range_prims: [0; PRIM_KINDS], column_ranges: 0, rounded_ranges: 0, groups: 0, kids_smooth: 0, kids_sharp: 0, supports_root: 0, supports_pow: 0, pieces: 0, mid_evals: 0, reach_nodes: 0, reach_kids: 0, reach_pairs: 0, table_places: 0, table_asks: 0 };

    /// What it did since the last call, and starts again.
    fn taken() -> Did {
        DID.with(|d| d.replace(Did::NONE))
    }
}

/// **The least and greatest values a node's field takes in the ball of radius `r` about `c`**
/// (in its parent's frame), by its own primitives' factors and every operation's shape, not
/// by one factor for the whole tree. Each step is a bound:
///
/// - **Into the node's frame** ([`local`]): a mirror (`|x|`), a move, a turn and an elongation
///   (`q - clamp(q)`) take a ball into a ball of the same radius about the centre's image; a
///   scale and a stretch divide it by the scale times the least stretch; a twist or a bend
///   turns a point at `rho` from its axis by its rate times the coordinate it goes by, so two
///   points `d` apart go at most `d (1 + rate * rho)` apart (`rho` the ball's furthest from the
///   axis); a repetition takes a point to its nearest copy, so the ball goes to the copies its
///   extent along each axis can reach, each a ball (at most [`RANGE_COPIES`] in all).
/// - **A primitive**: its field at the centre, less and more its factor ([`prim_lip`]) times the
///   radius and twice its slack ([`prim_slack`]: how far it may be from such a field).
/// - **A group**: the polynomial join is non-decreasing in each operand (its derivative is
///   `1 - h / 2` toward the lesser and `h / 2` toward the greater), so a union's chain of the
///   children's least values is its least and of their greatest its greatest; a meeting the
///   same; a cut is non-decreasing in its first child and non-increasing in the rest, so its
///   least takes the rest's greatest. A near-union joins only the children within its width
///   of the nearest, in order: leaving a child out of a chain never lowers it (each join is at
///   most its running value, and the joins after are non-decreasing), so it is never below the
///   chain of all its children, nor above its least child.
/// - **Its distance modifiers**: the scale times the least stretch, a swelling (less a
///   constant), a shell (`|d| - t`, from the range of `d`), noise (its amount each way: the
///   noise is within -1 and 1).
///
/// `spent` counts what it did ([`Spent`]). (What [`clear_of_solid`] shows a box clear by.)
fn field_range(n: &Prepared, c: V3, r: f32, copies: &mut u32, spent: &mut Spent) -> (f32, f32) {
    field_range_as(n, c, r, copies, spent, false)
}

/// [`field_range`], and with `tight` an ellipsoid's greatest is bounded too (its least is the
/// same either way): what the mesher asks ([`mesh`], [`pruned`]), so that a block inside a
/// flat ellipsoid can still be shown solid. (The box's search asks without it, as it was
/// reviewed.) Whoever asks is charged `spent.work`, in the one scheme ([`Spent`]).
fn field_range_as(n: &Prepared, c: V3, r: f32, copies: &mut u32, spent: &mut Spent, tight: bool) -> (f32, f32) {
    field_range_noting(n, c, r, copies, spent, tight, None)
}

/// The ranges a walk of [`field_range_noting`] took of each child of a group it passed
/// through, by the child's place in memory (one prune's: the tree is borrowed for it, and the
/// place decides only whose range is whose, never a value).
type Noted = std::collections::BTreeMap<usize, (f32, f32)>;

/// [`field_range_as`], noting in `seen` (when given) the range it took of each child of every
/// group it passed through, so that [`pruned`], going down into a loose child whose range it
/// has just taken, finds its children's ranges there rather than walking them again (review 2
/// of the mesher lane: nested loose unions were walked once a level, 418 million evaluations
/// where the factors had taken 211 million). Such a child's range was taken over a ball about
/// the same middle, as [`local`] takes it, and a hair larger (each frame widens the radius by
/// a ten-thousandth), so it holds over the smaller ball `pruned` asks of: sound, never tighter.
#[allow(clippy::too_many_arguments, reason = "the range's walk, its counts, its two modes and what it notes")]
fn field_range_noting(n: &Prepared, c: V3, r: f32, copies: &mut u32, spent: &mut Spent, tight: bool, mut seen: Option<&mut Noted>) -> (f32, f32) {
    let m = &n.m;
    let unknown = (-UNKNOWN, UNKNOWN);
    let shrink = m.scale * m.stretch[0].min(m.stretch[1]).min(m.stretch[2]);
    #[cfg(test)]
    DID.with(|d| d.borrow_mut().range_nodes += 1);
    spent.work += RANGE_FRAME;
    if !(shrink > 0.0 && shrink.is_finite()) {
        return unknown;
    }
    // The ball in the node's frame, as `local` takes a point (mirror, move, turn, scale,
    // stretch, then repeat, elongate, twist, bend).
    let mut q = c;
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
    let r = r / shrink * 1.0001;
    // The copies the ball reaches along each axis (one, where it is not repeated).
    let mut along: [(f32, f32); 3] = [(0.0, 0.0); 3];
    let mut ways = [1u32; 3];
    if let Some((count, spacing)) = m.repeat {
        for k in 0..3 {
            if count[k] > 1 && spacing[k] > 0.0 {
                let half = (count[k] - 1) as f32 * 0.5;
                let at = |x: f32| crate::dmath::floor(x / spacing[k] + half + 0.5).clamp(0.0, count[k] as f32 - 1.0);
                let (first, last) = (at(q[k] - r), at(q[k] + r));
                along[k] = (first, spacing[k]);
                ways[k] = (last - first) as u32 + 1;
                along[k].0 -= half;
            }
        }
    }
    let total = ways[0] * ways[1] * ways[2];
    if total > 1 {
        *copies = copies.saturating_mul(total);
        if *copies > RANGE_COPIES {
            return unknown;
        }
    }
    let (mut lo, mut hi) = (UNKNOWN, -UNKNOWN);
    for i in 0..ways[0] {
        for j in 0..ways[1] {
            for k in 0..ways[2] {
                let mut p = q;
                for (a, w) in [i, j, k].into_iter().enumerate() {
                    if ways[a] > 1 || along[a].1 > 0.0 {
                        p[a] -= (along[a].0 + w as f32) * along[a].1;
                    }
                }
                let mut rr = r;
                if m.elongate != [0.0; 3] {
                    for (pk, ek) in p.iter_mut().zip(m.elongate) {
                        *pk -= pk.clamp(-ek, ek);
                    }
                }
                if m.twist != 0.0 {
                    let rate = m.twist.abs() * (std::f32::consts::PI / 180.0);
                    let rho = len2(p[0], p[2]) + rr;
                    let (s, co) = crate::dmath::sin_cos(-m.twist * p[1] * (std::f32::consts::PI / 180.0));
                    p = [co * p[0] - s * p[2], p[1], s * p[0] + co * p[2]];
                    rr *= 1.0 + rate * rho;
                }
                if m.bend != 0.0 {
                    let rate = m.bend.abs() * (std::f32::consts::PI / 180.0);
                    let rho = len2(p[0], p[1]) + rr;
                    let (s, co) = crate::dmath::sin_cos(-m.bend * p[0] * (std::f32::consts::PI / 180.0));
                    p = [co * p[0] - s * p[1], s * p[0] + co * p[1], p[2]];
                    rr *= 1.0 + rate * rho;
                }
                // (A copy's turn, twist, bend and repetition, as an evaluation is charged them.)
                #[cfg(test)]
                DID.with(|d| {
                    let mut d = d.borrow_mut();
                    for (k, on) in [n.rotated, m.stretch != [1.0; 3], m.repeat.is_some(), m.twist != 0.0, m.bend != 0.0, m.displace.is_some()].into_iter().enumerate() {
                        d.mods[k] += u64::from(on);
                    }
                    d.octaves += m.displace.map_or(0, |(_, _, o)| u64::from(o.max(1)));
                });
                spent.work += modifiers_cost(m);
                let (a, b) = inner_range(n, p, rr, copies, spent, tight, seen.as_deref_mut());
                lo = lo.min(a);
                hi = hi.max(b);
            }
        }
    }
    // (A least not known is nothing known; a greatest not known stays so.)
    if lo <= -UNKNOWN {
        return unknown;
    }
    // The distance modifiers, as `finish` applies them.
    let (mut lo, mut hi) = (lo * shrink, if hi >= UNKNOWN { UNKNOWN } else { (hi * shrink).min(UNKNOWN) });
    if m.round != 0.0 {
        (lo, hi) = (lo - m.round, hi - m.round);
    }
    if m.shell > 0.0 {
        (lo, hi) = if lo >= 0.0 { (lo, hi) } else if hi <= 0.0 { (-hi, -lo) } else { (0.0, hi.max(-lo)) };
        (lo, hi) = (lo - m.shell * 0.5, hi - m.shell * 0.5);
    }
    if let Some((amount, _, _)) = m.displace {
        (lo, hi) = (lo - amount.abs(), hi + amount.abs());
    }
    (lo, hi.min(UNKNOWN))
}

/// [`field_range`] of a node's own distance in its own frame, before its distance modifiers.
#[allow(clippy::too_many_arguments, reason = "as field_range_noting")]
fn inner_range(n: &Prepared, q: V3, r: f32, copies: &mut u32, spent: &mut Spent, tight: bool, mut seen: Option<&mut Noted>) -> (f32, f32) {
    #[cfg(test)]
    DID.with(|d| {
        let mut d = d.borrow_mut();
        match &n.kind {
            PKind::Prim(p) => {
                d.range_prims[prim_kind(p)] += 1;
                d.column_ranges += u64::from(matches!(p, Prim::Column { .. }));
                d.rounded_ranges += u64::from(matches!(p, Prim::Rounded { .. }));
            }
            PKind::Group { kids, smooth, .. } => {
                d.groups += 1;
                if *smooth > 0.0 {
                    d.kids_smooth += kids.len() as u64;
                } else {
                    d.kids_sharp += kids.len() as u64;
                }
            }
        }
    });
    spent.work += own_range_cost(n);
    let asks = &mut spent.asks;
    match &n.kind {
        PKind::Prim(Prim::Ellipsoid { radii }) => {
            // Its field reads short far from it, and faster than any one factor says (a flat
            // one's, more than twice the distance's rate). What holds ([`ellipsoid_placed`]):
            // where it is at most `L` the point is within `L (rho + 1 / rho) / 2` of the
            // ellipsoid, so outside it is never below the distance over that, and the distance
            // from a point of the ball is at least the middle's from the box round it, less the
            // radius. Inside, never below minus its greatest radius (`(k0 - 1) k0 / k1`, with
            // `k0 / k1` at most that). How far above: not bounded here.
            *asks += 1;
            let least = radii[0].min(radii[1]).min(radii[2]);
            let most = radii[0].max(radii[1]).max(radii[2]);
            if least.is_nan() || least <= 0.0 {
                return (-UNKNOWN, UNKNOWN);
            }
            let rho = most / least;
            let short = 0.5 * (rho + 1.0 / rho);
            let out = (0..3).map(|k| (q[k].abs() - radii[k]).max(0.0)).map(|x| x * x).sum::<f32>().sqrt().max(math3::length(q) - most);
            let down = if out > r { (out - r) / short } else { -most };
            if !tight {
                return (down, UNKNOWN);
            }
            // How far above (DISTANCE_FIELDS 2.6): the true signed distance at the middle is at
            // most `|q| (1 - 1 / k0)` outside (the point `q / k0` is on the surface) and at
            // most `-(1 - k0) least` inside (`k0` changes by at most one over the least radius
            // a metre), so in the ball at most that plus `r`. Where that is not below nothing
            // the field is at most it (outside, the field is the distance to a touching plane,
            // never more than the distance); where it is, every point of the ball is inside,
            // at least that deep, and the field reads at most the depth over the ratio of the
            // radii (it is at most `-(1 - k0) least` and the depth at most `(1 - k0) most`).
            let k0 = math3::length([q[0] / radii[0], q[1] / radii[1], q[2] / radii[2]]);
            let top = if k0 >= 1.0 { math3::length(q) * (1.0 - 1.0 / k0) } else { -(1.0 - k0) * least } + r;
            let up = if top >= 0.0 { top } else { top / rho };
            if !(up.is_finite() && k0.is_finite()) {
                return (down, UNKNOWN);
            }
            (down, up + 1e-5 * up.abs() + 1e-7)
        }
        PKind::Prim(p @ Prim::Column { a, b, side, sections, count, square }) => {
            // A column whose breadth and depth change in different proportions is not
            // Lipschitz: far from it, its field changes along it ever faster. Its field is
            // `(k - 1) cap`. Across the column `k` changes by at most one of the smaller radii a
            // radius (a squarer section's measure is a norm whose gradient is at most one; a
            // rounder one's, `2^(1/s - 1/2)`); past an end the section is fixed. Along it, by
            // Euler's rule for the measure, the field changes by at most the growth `g` of the
            // radii plus `(cap + f)` times how far the two radii's own rates differ, which
            // [`column_wander`] bounds as `wander` over the least radius. So the field changes
            // at most at `A + B f` a metre (`A` = the across rate + g + wander, `B` = wander /
            // least): in the ball it is at most `F = (d + A / B) e^(B r) - A / B`, and at least
            // `d - (A + B F) r`. With no wander, that is [`column_lip`].
            *asks += 1;
            let d = prim_distance(p, q);
            // (Worked out once, in [`prepare`]: its least radius there is scaled by the node's
            // scale, and its factor by any twist or bend, which only widens what follows.)
            let (wander, least, lip) = (n.wander, n.least / n.m.scale, n.lip);
            let grow = (lip * lip - 1.0).max(0.0).sqrt();
            // (Its box, ratio and rate across: the node's own, walked once before the search;
            // walked here, and charged, only for a tree that was not: [`outline_columns`].)
            let used = &sections[..(*count as usize).clamp(1, MAX_SECTIONS)];
            let outline = match n.outline {
                Some(o) => o,
                None => {
                    spent.work += column_walk_cost(used.len(), *square);
                    column_outline(*a, *b, *side, used, *square)
                }
            };
            let asks = &mut spent.asks;
            let across = outline.across;
            let rate = across + grow + wander;
            let up = if wander > 0.0 && least > 0.0 {
                let bb = wander / least;
                (d.max(0.0) + rate / bb) * crate::dmath::exp(bb * r) - rate / bb
            } else {
                d.max(0.0) + rate * r
            };
            let mut down = d - (rate + if least > 0.0 { wander / least * up.max(0.0) } else { 0.0 }) * r;
            // And what its reach rests on ([`prim_reach`]): where its field is at most `L` the
            // point is within `L` times its halves' greatest ratio of the box of its solid, so
            // it is never below its distance from that box over the ratio (far from it, much
            // the tighter). (The search's budget counts that as eight asks more, as it has since
            // the box's second round, so that what the search finds does not move: [`Spent`].)
            *asks += 8;
            let ratio = outline.ratio;
            if ratio.is_finite() {
                let (lo, hi) = (outline.lo, outline.hi);
                let out = (0..3).map(|k| (lo[k] - q[k]).max(q[k] - hi[k]).max(0.0)).map(|x| x * x).sum::<f32>().sqrt();
                if out > r {
                    down = down.max((out - r) / (ratio * (1.0 + 1e-5)));
                }
            }
            if !(d.is_finite() && up.is_finite() && down.is_finite() && up < UNKNOWN && down > -UNKNOWN) {
                return (-UNKNOWN, UNKNOWN);
            }
            (down, up.max(d))
        }
        PKind::Prim(p) => {
            *asks += 1;
            let d = prim_distance(p, q);
            let by = prim_lip(p) * r + 2.0 * prim_slack(p);
            if !(d.is_finite() && by.is_finite()) {
                return (-UNKNOWN, UNKNOWN);
            }
            (d - by, d + by)
        }
        PKind::Group { op, kids, smooth } => {
            let got: Vec<(f32, f32)> = kids.iter().map(|k| field_range_noting(k, q, r, copies, spent, tight, seen.as_deref_mut())).collect();
            if let Some(seen) = seen {
                for (k, g) in kids.iter().zip(&got) {
                    seen.entry(std::ptr::from_ref(k) as usize).or_insert(*g);
                }
            }
            if got.is_empty() {
                return (UNKNOWN, UNKNOWN);
            }
            let chain = |first: f32, rest: &mut dyn Iterator<Item = f32>, join: &dyn Fn(f32, f32) -> f32| rest.fold(first, join);
            let union = |a: f32, b: f32| smin(a, b, *smooth).0;
            match op {
                Combine::Union => (chain(got[0].0, &mut got[1..].iter().map(|g| g.0), &union), chain(got[0].1, &mut got[1..].iter().map(|g| g.1), &union)),
                // (Never below the chain of all, never above the least child.)
                Combine::Near => (chain(got[0].0, &mut got[1..].iter().map(|g| g.0), &union), got.iter().map(|g| g.1).fold(UNKNOWN, f32::min)),
                Combine::Intersection => {
                    let meet = |a: f32, b: f32| -smin(-a, -b, *smooth).0;
                    (chain(got[0].0, &mut got[1..].iter().map(|g| g.0), &meet), chain(got[0].1, &mut got[1..].iter().map(|g| g.1), &meet))
                }
                Combine::Difference => {
                    let cut = |a: f32, b: f32| -smin(-a, b, *smooth).0;
                    (chain(got[0].0, &mut got[1..].iter().map(|g| g.1), &cut), chain(got[0].1, &mut got[1..].iter().map(|g| g.0), &cut))
                }
            }
        }
    }
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
                Prim::Rounded { half, .. } => centred(half),
                Prim::HalfSpace { .. } => return None,
                // Not centred: the box around its two balls.
                Prim::RoundCone { a, b, ra, rb } => (math3::vmin(math3::sub(a, [ra; 3]), math3::sub(b, [rb; 3])), math3::vmax(math3::add(a, [ra; 3]), math3::add(b, [rb; 3]))),
                // The box around its ends, as wide all round as its widest section.
                Prim::Column { a, b, sections, count, square, .. } => {
                    let used = &sections[..(count as usize).clamp(1, MAX_SECTIONS)];
                    // (An ellipse reaches as far as its longer half; a squarer section reaches
                    // toward the corner of the box round it, which is as far as any goes.)
                    let widest = used.iter().map(|s| if square == 2.0 { s[1].max(s[2]) } else { len2(s[1], s[2]) }).fold(0.0f32, f32::max);
                    // (Its rounded ends reach no further past its first and last sections than those are wide.)
                    (math3::sub(math3::vmin(a, b), [widest; 3]), math3::add(math3::vmax(a, b), [widest; 3]))
                }
            };
            // (No slack here: a rounded shape's is how far its field may be from the distance,
            // which moves no surface, so it pads no bounds. [`lip_slack`] counts it.)
            (lo, hi, prim_lip(p), 0.0)
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

/// The box the mesher works in when a recipe gives none, for a grid of `resolution` cells
/// along its longest side: [`bounds`]'s, grown to hold what [`reach`] says the solid reaches
/// where that is further by more than a quarter cell. **Never smaller than [`bounds`]'s**:
/// a mesh's cells are its box's longest side over its resolution, so any box that shrank would
/// move every mesh that is right today. It grows where `bounds` cut the solid off (a
/// stretch, a twist or a bend; a flat ellipsoid or column swollen) and, since the reach is a
/// bound, sometimes where it did not; so where it passes a side, the field itself is asked
/// whether anything is there ([`mesh_box_seeded`]), and a side moves only where it may be.
/// `None` where the tree is unbounded (a half-space in a union).
#[cfg(test)]
fn mesh_box(tree: &SdfNode, resolution: u32) -> Option<(V3, V3)> {
    mesh_box_seeded(tree, resolution, 1).map(|(b, _)| b)
}

/// The most evaluations of the field [`mesh_box_seeded`] spends making sure that nothing lies
/// past [`bounds`]'s box where [`reach`] says something may (all sides together). Past it, a
/// side not yet shown clear moves out to the reach, which is sound.
const CLEAR_ASKS: u64 = 1 << 14;

/// [`mesh_box_charged`] with nothing to charge but cancelling, and the asks as evaluations of
/// the tree (tests).
#[cfg(test)]
fn mesh_box_seeded(tree: &SdfNode, resolution: u32, seed: u64) -> Option<((V3, V3), u64)> {
    let mut spend = |_: u64| if crate::work::cancelled() { Err(SdfError::cancelled()) } else { Ok(()) };
    mesh_box_charged(tree, resolution, seed, &mut spend).ok().flatten().map(|(b, s)| (b, s.asks))
}

/// [`mesh_box`] with the seed the field's noise is drawn with: the box, and what its search
/// spent ([`Spent`]: its asks as whole evaluations of the tree, rounded up, and the work it
/// was charged). There are none where the reach passes no side by more than a quarter cell,
/// which is every sculpt of the repository.
///
/// **The search is charged as it is spent**, through `spend` (in [`mesh`], the build's own
/// charge, which also refuses a build past its work limit and a cancelled one): every
/// column's walk before it is taken, every ball's range through the tree before it is taken
/// (a ball that meets more than one copy of a repeated node is charged the rest after its
/// walk: at most [`RANGE_COPIES`] times the tree's range), and every evaluation of the tree
/// before it is taken; the search also looks for a cancel before every ball. So the work it
/// does is never more than one ball's past what it was charged, and an error from `spend`
/// (the build's limit passed, or a cancel) ends it at once, with that error: **past the
/// build's limit nothing is meshed**, since the mesher would be refused at its first charge
/// after the search in any box. Its own budget ([`CLEAR_ASKS`]) is another matter: spent, it
/// leaves the sides not yet shown clear at the reach, which holds the solid, and the build
/// goes on in that larger box.
///
/// **A side of [`bounds`]'s box moves only where the solid may pass it.** Where the reach
/// passes a side by more than a quarter cell, the slab between the two (as wide as the
/// reach's box every other way, which holds the whole solid) is searched for solid
/// ([`clear_of_solid`]) by the range of the field over balls ([`field_range`]: each
/// primitive's own factor, and every operation's shape). A slab shown clear keeps the old
/// side; any other side, or once [`CLEAR_ASKS`] is spent, moves out to the reach. So a sculpt whose
/// solid lies within the old box (and a quarter cell) keeps the old box bit for bit, however
/// loose the reach is there, and one whose solid passes it (a stretch, a twist or a bend; a
/// flat ellipsoid or column swollen) is meshed in a box that holds it.
fn mesh_box_charged(tree: &SdfNode, resolution: u32, seed: u64, spend: &mut Spend) -> Result<Option<Searched>, SdfError> {
    let Some((lo, hi, _, _)) = bounds(tree) else { return Ok(None) };
    // A side moves only when the reach passes it by more than a quarter of a cell. Nothing
    // within that is lost: the grid's samples begin a cell and a half before the box, the
    // first layer forced outside and the second, half a cell before it, evaluated; so a solid
    // that ends within a quarter cell past the box is sampled and meshed as in any larger box.
    let longest = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
    let close = 0.25 * longest / resolution.max(1) as f32;
    // (Counted in evaluations of primitives: a tree's evaluation is all of them once.)
    let prims = count_prims(tree).max(1) as u64;
    let budget = CLEAR_ASKS * prims;
    let mut spent = Spent::default();
    // (The reach the search starts from, charged as it is worked out: [`reach_charged`].)
    let mut reached = 0u64;
    let got = reach_charged(tree, 0.0, &mut |n| {
        reached += n;
        spend(n)
    })?;
    // (What was charged: the work, and what a ball was charged before it past what it did.)
    let mut charged = 0u64;
    let mut charge = |n: u64| -> Result<(), SdfError> {
        charged += n;
        spend(n)
    };
    let (mut glo, mut ghi) = (lo, hi);
    match got {
        // (Nothing to hold: the solid is empty, and the build says so in any box.)
        Some(got) if got.is_nothing() => {}
        Some(got) => {
            let (rlo, rhi) = got.within;
            let mut prepared = None;
            for k in 0..3 {
                for low in [true, false] {
                    let passes = if low { rlo[k] < lo[k] - close } else { rhi[k] > hi[k] + close };
                    if !passes {
                        continue;
                    }
                    // The slab past this side, within the reach's box every other way.
                    let (mut slo, mut shi) = (rlo, rhi);
                    if low {
                        shi[k] = lo[k] - close;
                    } else {
                        slo[k] = hi[k] + close;
                    }
                    let p = match &mut prepared {
                        Some(p) => p,
                        None => {
                            // (Every column's walk, once, before the search: [`Outline`].)
                            let mut p = prepare(tree, seed, &mut 0);
                            outline_columns(&mut p, &mut charge)?;
                            let whole = (range_cost(&p), prepared_cost(&p));
                            prepared.insert((p, whole))
                        }
                    };
                    if !clear_of_solid(&p.0, slo, shi, prims, budget, p.1, &mut spent, &mut charge)? {
                        if low {
                            glo[k] = rlo[k];
                        } else {
                            ghi[k] = rhi[k];
                        }
                    }
                }
            }
        }
        // (A node whose scale or stretch is not above nothing, which no recipe can write.)
        None => {}
    }
    let grown = (glo, ghi);
    #[cfg(test)]
    BOXES.with(|b| {
        let [taken, grew] = b.get();
        b.set([taken + 1, grew + u64::from(grown != (lo, hi))]);
    });
    // (Whole evaluations of the tree, rounded up, and what the build was charged.)
    Ok(Some((grown, Spent { asks: spent.asks.div_ceil(prims), work: charged, reach: reached })))
}

/// Whether a node or any under it is repeated.
fn has_repeat(n: &SdfNode) -> bool {
    n.m.repeat.is_some() || matches!(&n.kind, Kind::Group { kids, .. } if kids.iter().any(has_repeat))
}

fn count_prims(n: &SdfNode) -> usize {
    match &n.kind {
        Kind::Prim(_) => 1,
        Kind::Group { kids, .. } => kids.iter().map(count_prims).sum(),
    }
}

/// Whether the field is above nothing everywhere in the box `lo..hi`, shown by halving it: a
/// box is clear where the least the field takes in the ball round it ([`field_range`]) is above
/// nothing. `false` where a box's middle is at or below nothing (solid), when `asks`
/// (evaluations of primitives) reach `budget` first, or when a box is too small to halve:
/// never clear without being shown clear. Boxes are taken in a fixed order (deterministic).
///
/// Each ball is charged through `spend` before it is taken, at the tree's range (`whole.0`:
/// [`range_cost`]), and after it whatever its walk did past that ([`Spent`]); each evaluation
/// of the tree at its own cost (`whole.1`) before it is taken. A cancel is looked for before
/// every ball, and an error from `spend` ends the search with it.
#[allow(clippy::too_many_arguments, reason = "the search's own state, threaded through one loop")]
fn clear_of_solid(tree: &Prepared, lo: V3, hi: V3, prims: u64, budget: u64, whole: (u64, u64), spent: &mut Spent, spend: &mut Spend) -> Result<bool, SdfError> {
    if !(0..3).all(|k| lo[k] <= hi[k]) {
        return Ok(true);
    }
    let mut todo = vec![(lo, hi)];
    while let Some((lo, hi)) = todo.pop() {
        if spent.asks >= budget {
            return Ok(false);
        }
        if crate::work::cancelled() {
            return Err(SdfError::cancelled());
        }
        let mid = [0, 1, 2].map(|k| 0.5 * (lo[k] + hi[k]));
        let half = [0, 1, 2].map(|k| 0.5 * (hi[k] - lo[k]));
        spend(whole.0)?;
        let before = spent.work;
        let (least, most) = field_range(tree, mid, math3::length(half) * 1.001, &mut 1, spent);
        let did = spent.work - before;
        if did > whole.0 {
            spend(did - whole.0)?;
        }
        // (Solid somewhere in it; a NaN is never clear.)
        if most.is_nan() || most <= 0.0 {
            return Ok(false);
        }
        if least > 0.0 {
            continue;
        }
        // (Solid at its middle: no more to ask.)
        spend(whole.1)?;
        spent.asks += prims;
        spent.work += whole.1;
        #[cfg(test)]
        DID.with(|d| d.borrow_mut().mid_evals += 1);
        let f = eval(tree, mid);
        if f.is_nan() || f <= 0.0 {
            return Ok(false);
        }
        let a = (0..3).fold(0, |a, k| if half[k] > half[a] { k } else { a });
        if !(mid[a] > lo[a] && mid[a] < hi[a]) {
            return Ok(false);
        }
        let (mut first_hi, mut second_lo) = (hi, lo);
        first_hi[a] = mid[a];
        second_lo[a] = mid[a];
        todo.push((second_lo, hi));
        todo.push((lo, first_hi));
    }
    Ok(true)
}

#[cfg(test)]
thread_local! {
    /// (Tests only) how many boxes [`mesh_box`] gave on this thread, and how many of them
    /// [`reach`] grew past [`bounds`]'s.
    static BOXES: std::cell::Cell<[u64; 2]> = const { std::cell::Cell::new([0; 2]) };
}

/// Where a node's field is at most `need` (metres, in its parent's frame): **every such
/// point lies within `r` of the box `lo..hi`** (`(lo, hi, r)`, in the parent's frame). At
/// `need = 0` that is the node's solid. `None` where [`bounds`] has none (a half-space in a
/// union, or first in a cut or a meeting), and for a scale or stretch not above nothing.
///
/// [`bounds`] widens a box by each distance modifier's amount and each join's width, which
/// is right only where the field reads the distance outside. A node stretched unevenly reads
/// short of it by its least stretch over its greatest
/// ([docs/design/DISTANCE_FIELDS.md](../../../docs/design/DISTANCE_FIELDS.md), 2.5), so a
/// swelling, a shell, noise or a smooth join on it reaches that much further along its long
/// axis, and `bounds` cut it off (two spheres stretched ten to one and joined smoothly lost a
/// tenth of their length). Here each node is asked for the level of its field its caller
/// needs, from the root's 0 down, and answers for that level:
///
/// - **Its distance modifiers first** ([`finish`], undone last first): noise moves the field
///   by at most its amount; a shell's solid is where `|d|` is at most half its thickness; a
///   swelling takes `round` off; and the field was multiplied by the node's scale times its
///   least stretch. So the field is at most `need` only where the node's own is at most
///   `(need + |noise| + shell / 2 + round) / (scale * least stretch)`.
/// - **A primitive** whose field is never less than the distance to its solid outside it (a
///   sphere, a box, a cylinder, a cone, a capsule, a torus, a round cone: exact; a rounded
///   box or cylinder: never below the distance to its hard shape) reaches its own box and a
///   ball of the level ([`prim_reach`] for an ellipsoid and a column, which read short).
/// - **A union or a near-union** of `n` children joined with width `k` is never below the
///   least of them by more than `min(n - 1, 4) * k / 4` (5.5: a dip of `k / 4` a join, and
///   under `k` for a chain however long; a near-union joins a chain of some of them): each
///   child is asked for the level and that much more, and the union reaches what its
///   children reach; where only one part is near, less ([`union_reach`]). **A meeting or a cut**, sharp or smooth, is never below its children's
///   greatest (a cut: its first child's; 5.6): each bounded child is asked for the level
///   itself, and the meeting reaches no further than any of them.
/// - **Then into the parent's frame**, as [`local`] maps out of it: a twist or a bend turns
///   points about an axis through the node's middle, so a point reaches no further from it,
///   and by a turn no greater than its rate times how far along the axis it is; an
///   elongation and a repetition move a point by at most their half lengths and spans; a
///   stretch makes the ball an ellipsoid, which is a ball of its least radius and a box of
///   the rest; a turn turns the box and leaves the ball, a move moves both, a mirror takes
///   both sides.
///
/// It is often tighter than `bounds` (which widens at every level of a tree by the greatest
/// width under it, so nested groups pile up) and sometimes looser (a column or an ellipsoid
/// read short of the distance, and a wide join is answered for where it could reach, not where
/// it does): the mesher takes `bounds`'s box and grows it only where this reaches further
/// ([`mesh_box`]).
#[cfg(test)]
fn reach(n: &SdfNode, need: f32) -> Option<Reach> {
    reach_charged(n, need, &mut |_| Ok(())).ok().flatten()
}

/// [`reach`], charged as it is worked out through `spend` (in [`mesh`], the build's charge):
/// every node asked, before it is asked ([`REACH_NODE`] and its children's [`REACH_KID`]); a
/// smooth union's pairs of children, before they are paired ([`REACH_PAIR`]); every column's
/// walk of its curve, once, before it is walked ([`column_walk_cost`]: the [`ColumnTable`] it
/// answers every level from after); and every level asked of a column, before it is asked
/// ([`REACH_PLACE`] a place of its table). A cancel is looked for at every node. An error from
/// `spend`, or a cancel, ends it at once with that error. (Before, nothing of it was charged,
/// and a column was walked again at every level asked: seven seconds for a chain of 64 smooth
/// unions of columns, and a cancel seen eight seconds after it was raised; third review of the
/// box's search.)
fn reach_charged(n: &SdfNode, need: f32, spend: &mut Spend) -> Result<Option<Reach>, SdfError> {
    let mut cx = ReachCx { asks: REACH_ASKS, tables: std::collections::BTreeMap::new(), spend, failed: None };
    let got = reach_within(n, need, true, &mut cx);
    match cx.failed {
        Some(e) => Err(e),
        None => Ok(got),
    }
}

/// What [`reach_within`] carries down the tree: the asks it has left ([`REACH_ASKS`]), the
/// columns' tables by node (each walked once: the node's place in memory names it while the
/// tree is asked, and decides nothing but which table is whose), what it charges, and the
/// error that ended it.
struct ReachCx<'s, 'f> {
    asks: u32,
    tables: std::collections::BTreeMap<usize, ColumnTable>,
    spend: &'s mut Spend<'f>,
    failed: Option<SdfError>,
}

impl ReachCx<'_, '_> {
    /// A cancel looked for, then `n` charged; `false` (and every ask after answers nothing) once
    /// either has ended it.
    fn pay(&mut self, n: u64) -> bool {
        if self.failed.is_some() {
            return false;
        }
        if crate::work::cancelled() {
            self.failed = Some(SdfError::cancelled());
            return false;
        }
        match (self.spend)(n) {
            Ok(()) => true,
            Err(e) => {
                self.failed = Some(e);
                false
            }
        }
    }

    /// The reach of the column `p` (node `n`, in the frame it is asked in) at `level`: its
    /// table walked the first time (charged first), every level charged by its places.
    fn column(&mut self, n: &SdfNode, p: &Prim, level: f32) -> Option<Reach> {
        let Prim::Column { a, b, side, sections, count, square } = *p else { return None };
        let used = &sections[..(count as usize).clamp(1, MAX_SECTIONS)];
        let key = n as *const SdfNode as usize;
        if !self.tables.contains_key(&key) {
            if !self.pay(column_walk_cost(used.len(), square)) {
                return None;
            }
            self.tables.insert(key, column_table(a, b, side, used, square));
        }
        if !self.pay(column_places(used.len()) * REACH_PLACE) {
            return None;
        }
        #[cfg(test)]
        DID.with(|d| d.borrow_mut().table_asks += 1);
        self.tables.get(&key).map(|t| column_reach(t, level))
    }
}

// The weights of the reach's steps: what a node's own asking does (its frame's turn, worked
// out in double precision, its boxes carried and mirrored; an ellipsoid's three reaches), each
// child's share of its group (gathered, joined), each pair of a smooth union's children (two
// boxes met), each place of a column's table. Timed (2026-11-24, release, a probe of the box
// lane's, the least of three runs on a machine busy with other builds) on the third review's
// trees of eight-section columns at squareness 2, 3 and 5 (127 in one smooth union; a chain
// of 64 two-child smooth unions, 8,191 asks; 64 in pairs; combs 15 deep by 7 and 16 by 5):
// 1.4 to 4.7 ns a unit charged, where the search took 2.6 to 6 and an evaluation 1.6 to 5.8 on
// the same machine. Timed, so only a scale: the tests hold that every step is charged.
/// The work of asking one node of [`reach`] (besides its children).
const REACH_NODE: u64 = 1200;
/// The work of each child of a group [`reach`] asks.
const REACH_KID: u64 = 60;
/// The work of each pair of a smooth union's children [`union_reach`] meets.
const REACH_PAIR: u64 = 20;
/// The work of each place of a column's table asked at a level ([`ColumnTable::at`]).
const REACH_PLACE: u64 = 6;

/// What [`reach`] answers for a node at a level of its field, in its parent's frame: every
/// point where the field is at most that level lies within `r` of the box `lo..hi` **and** in
/// the box `within`. Two forms, both sound, so that the answer's box is their meeting: the
/// first keeps the ball (a turn leaves a ball as it is, where it would take a cube's corners
/// out by up to `sqrt 3`: [`bounds`] widens by a join's width after its turn, and an answer
/// that turned the join's dip as a cube was wider than `bounds` where nothing reached), and
/// the second keeps what a meeting of boxes says ([`union_reach`]'s second answer, a meeting
/// of parts). `within` never passes the first form's box widened by its ball.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Reach {
    lo: V3,
    hi: V3,
    r: f32,
    within: (V3, V3),
}

impl Reach {
    fn new(lo: V3, hi: V3, r: f32) -> Reach {
        Reach { lo, hi, r, within: (math3::sub(lo, [r; 3]), math3::add(hi, [r; 3])) }
    }

    /// No point at all (a meeting of parts whose levels do not meet: its field, never below
    /// any part's, is above the level everywhere). A union leaves it out, a meeting or a cut
    /// that holds it first is nothing too, and it is carried through no modifier. ([`bounds`]
    /// says such a meeting is a point at its origin, which its modifiers then carry: a box
    /// round where nothing is.)
    fn nothing() -> Reach {
        Reach { lo: [0.0; 3], hi: [0.0; 3], r: 0.0, within: ([f32::MAX; 3], [f32::MIN; 3]) }
    }

    /// The same, known also to lie in the box `b`.
    fn meet(self, b: (V3, V3)) -> Reach {
        Reach { within: (math3::vmax(self.within.0, b.0), math3::vmin(self.within.1, b.1)), ..self }
    }

    fn is_nothing(&self) -> bool {
        (0..3).any(|k| self.lo[k] > self.hi[k] || self.within.0[k] > self.within.1[k])
    }
}

/// The box round boxes taken together.
fn hull_box(boxes: impl Iterator<Item = (V3, V3)>) -> (V3, V3) {
    boxes.fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (l, h)| (math3::vmin(lo, l), math3::vmax(hi, h)))
}

/// A safety net on how many nodes [`reach`] asks in all: past it, smooth unions stop asking
/// for [`union_reach`]'s second answer and ask their children once. A tree of [`MAX_NODES`]
/// never comes near it: a union asks for the second answer only where it was asked for the
/// first's own level (`full`), and its children's second answers only once each, so a node
/// is asked at most three times for each smooth union above it, and a tree of 128 nodes at
/// most about 8,300 times (the test `reach_asks_a_bounded_number_of_times`; before, every
/// ask asked three more, and a chain of two-child unions 16 deep spent it).
const REACH_ASKS: u32 = 1 << 14;

fn reach_within(n: &SdfNode, need: f32, full: bool, cx: &mut ReachCx) -> Option<Reach> {
    cx.asks = cx.asks.saturating_sub(1);
    let kids = match &n.kind {
        Kind::Group { kids, .. } => kids.len() as u64,
        Kind::Prim(_) => 0,
    };
    if !cx.pay(REACH_NODE + kids * REACH_KID) {
        return None;
    }
    // (Counted once it is paid for, where the asking is done: what the charge was for.)
    #[cfg(test)]
    DID.with(|d| {
        let mut d = d.borrow_mut();
        d.reach_nodes += 1;
        d.reach_kids += kids;
    });
    let m = &n.m;
    let least = m.stretch[0].min(m.stretch[1]).min(m.stretch[2]);
    let shrink = m.scale * least;
    if !(shrink > 0.0 && shrink.is_finite()) {
        return None;
    }
    let mut own = need + m.round;
    if m.shell > 0.0 {
        own += m.shell * 0.5;
    }
    if let Some((amount, _, _)) = m.displace {
        own += amount.abs();
    }
    let level = own.max(0.0) / shrink;
    // An ellipsoid or a column only turned, scaled evenly and moved: worked out in the
    // parent's frame (a box turned takes its corners out, by up to `sqrt 3`; [`bounds`] turns
    // their boxes and widens after the turn, and so did not).
    let rigid = m.stretch[1] == m.stretch[0] && m.stretch[2] == m.stretch[0] && m.twist == 0.0 && m.bend == 0.0 && m.elongate == [0.0; 3] && m.repeat.is_none();
    let turn = || Affine::rotate_euler_deg([m.rotate[0] as f64, m.rotate[1] as f64, m.rotate[2] as f64]);
    let placed = match n.kind {
        Kind::Prim(Prim::Ellipsoid { radii }) if rigid => {
            let turn = turn();
            Some(ellipsoid_placed(radii, level, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|e| turn.apply_point(e)), shrink, m.at))
        }
        Kind::Prim(Prim::Column { a, b, side, mut sections, count, square }) if rigid => {
            // (A column's field scaled is the column with every length scaled.)
            let turn = turn();
            let place = |v: V3| math3::add(turn.apply_point(math3::scale(v, shrink)), m.at);
            for s in &mut sections {
                (s[1], s[2]) = (s[1] * shrink, s[2] * shrink);
            }
            cx.column(n, &Prim::Column { a: place(a), b: place(b), side: turn.apply_point(side), sections, count, square }, level * shrink)
        }
        _ => None,
    };
    if let Some(got) = placed {
        let (lo, hi) = mirrored(m, got.lo, got.hi);
        return Some(Reach::new(lo, hi, got.r).meet(mirrored(m, got.within.0, got.within.1)));
    }
    if cx.failed.is_some() {
        return None;
    }
    let mut got = match &n.kind {
        Kind::Prim(p @ Prim::Column { .. }) => cx.column(n, p, level)?,
        Kind::Prim(p) => prim_reach(p, level)?,
        Kind::Group { op, kids, smooth } if matches!(op, Combine::Union | Combine::Near) && *smooth > 0.0 && kids.len() > 1 => union_reach(kids, *smooth, level, full, cx)?,
        Kind::Group { op, kids, .. } => {
            let union = matches!(op, Combine::Union | Combine::Near);
            let mut got = Vec::with_capacity(kids.len());
            for (i, k) in kids.iter().enumerate() {
                match reach_within(k, level, full, cx) {
                    Some(b) => got.push(b),
                    // As in `bounds`: an unbounded child makes a union unbounded, and a cut or
                    // a meeting whose first child it is; a later one bounds nothing.
                    None if union || i == 0 => return None,
                    None => {}
                }
                if *op == Combine::Difference {
                    break;
                }
            }
            if union {
                // (A part that is nothing adds nothing.)
                got.retain(|g| !g.is_nothing());
            } else if got.iter().any(Reach::is_nothing) {
                return Some(Reach::nothing());
            }
            if got.is_empty() {
                return Some(Reach::nothing());
            } else if got.len() == 1 {
                got[0]
            } else if union {
                hull_reach(&got)
            } else {
                // A meeting of several lies in each: the one whose own answer is the least
                // (the first such), in the meeting of all their boxes.
                let size = |g: &Reach| (0..3).map(|k| g.hi[k] - g.lo[k]).fold(0.0f32, f32::max) + 2.0 * g.r;
                let least = got.iter().copied().reduce(|a, b| if size(&b) < size(&a) { b } else { a }).expect("two or more");
                got.iter().fold(least, |acc, g| acc.meet(g.within))
            }
        }
    };
    if got.is_nothing() {
        return Some(Reach::nothing());
    }
    if m.twist != 0.0 || m.bend != 0.0 {
        let (lo, hi) = turned_reach(got.lo, got.hi, got.r, m.twist, m.bend);
        got = Reach::new(lo, hi, 0.0).meet(turned_reach(got.within.0, got.within.1, 0.0, m.twist, m.bend));
    }
    // An elongation and a repetition widen both forms alike.
    let mut widen = |k: usize, by: f32| {
        got.lo[k] -= by;
        got.hi[k] += by;
        got.within.0[k] -= by;
        got.within.1[k] += by;
    };
    for (k, by) in m.elongate.into_iter().enumerate() {
        widen(k, by);
    }
    if let Some((count, spacing)) = m.repeat {
        for k in 0..3 {
            if count[k] > 1 && spacing[k] > 0.0 {
                widen(k, (count[k] - 1) as f32 * spacing[k] * 0.5);
            }
        }
    }
    let Reach { mut lo, mut hi, mut r, within } = got;
    // A ball of radius `r` stretched is an ellipsoid of radii `r * stretch`: a ball of the
    // least of them and a box of what the others pass it by (a point `e` of the ellipsoid is
    // `e * least / stretch`, in the ball, plus the rest, each axis within the difference).
    for k in 0..3 {
        let extra = r * (1.0 - least / m.stretch[k]);
        lo[k] -= extra;
        hi[k] += extra;
    }
    r *= shrink;
    let (wlo, whi) = carry(m, lo, hi);
    Some(Reach::new(wlo, whi, r).meet(carry(m, within.0, within.1)))
}

/// A box in a node's own frame taken to its parent's: scaled and stretched, turned (the box
/// round the turned box), moved, and mirrored.
fn carry(m: &Modifiers, lo: V3, hi: V3) -> (V3, V3) {
    let s = m.scale;
    let (lo_s, hi_s) = (math3::mul(math3::scale(lo, s), m.stretch), math3::mul(math3::scale(hi, s), m.stretch));
    let turn = Affine::rotate_euler_deg([m.rotate[0] as f64, m.rotate[1] as f64, m.rotate[2] as f64]);
    let (mut wlo, mut whi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for c in 0..8 {
        let p = [if c & 1 == 0 { lo_s[0] } else { hi_s[0] }, if c & 2 == 0 { lo_s[1] } else { hi_s[1] }, if c & 4 == 0 { lo_s[2] } else { hi_s[2] }];
        let w = math3::add(turn.apply_point(p), m.at);
        wlo = math3::vmin(wlo, w);
        whi = math3::vmax(whi, w);
    }
    mirrored(m, wlo, whi)
}

/// A box in a node's parent's frame with the node's mirrors: as far each way as it goes either.
fn mirrored(m: &Modifiers, mut lo: V3, mut hi: V3) -> (V3, V3) {
    for k in 0..3 {
        if m.mirror[k] {
            let e = lo[k].abs().max(hi[k].abs());
            lo[k] = -e;
            hi[k] = e;
        }
    }
    (lo, hi)
}

/// What a union of reaches reaches: every one's ball is the least one's and a cube of the
/// difference; and it lies in the box round their boxes.
fn hull_reach(got: &[Reach]) -> Reach {
    let r = got.iter().map(|g| g.r).fold(f32::MAX, f32::min);
    let (lo, hi) = hull_box(got.iter().map(|g| (math3::sub(g.lo, [g.r - r; 3]), math3::add(g.hi, [g.r - r; 3]))));
    Reach::new(lo, hi, r).meet(hull_box(got.iter().map(|g| g.within)))
}

/// [`reach`] for a smooth union or near-union of two or more children at a level `L` of its
/// field, joined with width `k` (the polynomial join, which dips by at most `k / 4` a join and
/// is the lesser where the two differ by `k` or more: DISTANCE_FIELDS 5.1, 5.5).
///
/// Two answers, each sound, and the tighter is kept:
///
/// - Every child asked for `L` and the most the union dips, `min(n - 1, 4) * k / 4`.
/// - **Where the union dips, two children are near.** A point where the union is at most `L`
///   is one where the least child is (that child's `L`), or else the join dipped it there.
///   Then the least child is within the dip of `L`; and another child is within `k` of `L`
///   for a pair (5.4: both fields are at most `L + k (1 - h/2)^2`), or within `9 k / 4` for
///   more: at the first join that took the running value to `L` or below, the two it joined
///   were both above `L` and under `k` apart, so both are at most `L + 5 k / 4`; the running
///   value is a chain of the children before, never more than `k` below their least (5.5),
///   so one of them is under `L + 9 k / 4`, and the child joined there is another. (A
///   near-union is such a chain of the children that take part.) So the union reaches its
///   children's `L` and, for each child, its dipped reach met with another's at that level.
///   Beside a part that reads short far from itself (an ellipsoid, a column, a stretched
///   part), only where another part is near can the join reach out.
///
/// The first is the answer's ball form, and the second, a box, is met with it ([`Reach`]):
/// so a turn above the union turns the first's ball as a ball, as [`bounds`] turns a join.
/// The second asks every child three times, so it is asked for only where the union was
/// asked with `full`, and then only the children's dipped answers are asked for theirs: the
/// number of asks grows with the tree's size times its depth, not three times at each level
/// (and past [`REACH_ASKS`] in all, only the first).
fn union_reach(kids: &[SdfNode], k: f32, level: f32, full: bool, cx: &mut ReachCx) -> Option<Reach> {
    let n = kids.len();
    let dip = k * ((n - 1).min(4) as f32) * 0.25;
    let dipped = kids.iter().map(|kid| reach_within(kid, level + dip, full, cx)).collect::<Option<Vec<_>>>()?;
    let some: Vec<Reach> = dipped.iter().copied().filter(|g| !g.is_nothing()).collect();
    if some.is_empty() {
        return Some(Reach::nothing());
    }
    let first = hull_reach(&some);
    if !full || cx.asks == 0 {
        return Some(first);
    }
    let paired_level = level + if n == 2 { k } else { 2.25 * k };
    let plain = kids.iter().map(|kid| reach_within(kid, level, false, cx)).collect::<Option<Vec<_>>>()?;
    let paired = kids.iter().map(|kid| reach_within(kid, paired_level, false, cx)).collect::<Option<Vec<_>>>()?;
    if !cx.pay((n * n) as u64 * REACH_PAIR) {
        return None;
    }
    // (A part that is nothing at a level holds no point there; its box is the empty one, which
    // meets nothing and widens no hull.)
    let mut parts: Vec<(V3, V3)> = plain.iter().filter(|b| !b.is_nothing()).map(|b| b.within).collect();
    for (i, d) in dipped.iter().enumerate() {
        for (j, p) in paired.iter().enumerate() {
            #[cfg(test)]
            DID.with(|d| d.borrow_mut().reach_pairs += 1);
            let (ml, mh) = (math3::vmax(d.within.0, p.within.0), math3::vmin(d.within.1, p.within.1));
            if i != j && (0..3).all(|a| ml[a] <= mh[a]) {
                parts.push((ml, mh));
            }
        }
    }
    if parts.is_empty() {
        return Some(Reach::nothing());
    }
    Some(first.meet(hull_box(parts.into_iter())))
}

/// [`reach`] for a primitive at a level of its own field (in its own frame).
///
/// - An **ellipsoid**'s field, `k0 (k0 - 1) / k1` with `k0 = |q / radii|` and `k1 = |q /
///   radii^2|`, reads short off its axes. Outside it is at most `level` exactly where `k0 - 1`
///   is at most `level * k1 / k0`, and `k1 / k0` depends only on the direction: with `w_i`
///   the shares of `(q / radii)^2`, it is `sqrt(sum of w_i / r_i^2)`. So along an axis of
///   radius `a` a point reaches at most `a * s * (1 + level * sqrt(s^2 / a^2 + (1 - s^2) /
///   least^2))` for some `s` from 0 to 1 (the rest of the share put on the least radius,
///   where it counts most). The two terms' greatest values bound it: `a` at `s = 1`, and the
///   second's, which is `level` itself when `a` is at most `sqrt 2` times the least radius
///   and otherwise `level * a / (2 * least * sqrt(1 - (least / a)^2))`. A box, no ball.
/// - A **column**'s field is `(k - 1) * cap`: `k` is how many of its section's own radii out
///   a point is (the section's measure across, which is never less than the share of either
///   half, and the cap's along past an end), and `cap` the section's smaller half. So where
///   the field is at most `level`, `k` is at most `1 + level / cap`: the point is its foot on
///   the axis, plus at most that many breadths toward the column's side, that many depths
///   across, and, past an end, the cap there and `level` along. Between two sections the
///   halves and their ratio are bounded by the curve's control points, piece by piece
///   ([`column_piece_reach`]), and past an end the section there and its cap
///   ([`ColumnTable`]). Its ball form: its box at the level of its surface and a ball of the
///   level times its halves' greatest ratio ([`column_ratio`]: the point is its surface's
///   point, toward its foot, and `k - 1` times that far out, `k - 1` at most the level over
///   the smaller half).
/// - An **ellipsoid** or a **column** turned, scaled evenly and moved is worked out in its
///   parent's frame ([`ellipsoid_placed`], or the column with its points and halves moved):
///   [`reach_within`].
/// - **The rest** are never less than the distance to their solid outside it, and their
///   solid lies in their box: the box and a ball of `level`. (A rounded box's or cylinder's
///   field is never below its hard shape's distance, whose solid is that box.)
fn prim_reach(p: &Prim, level: f32) -> Option<Reach> {
    Some(match *p {
        Prim::HalfSpace { .. } => return None,
        Prim::Ellipsoid { radii } => ellipsoid_placed(radii, level, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], 1.0, [0.0; 3]),
        Prim::Column { a, b, side, sections, count, square } => column_reach(&column_table(a, b, side, &sections[..(count as usize).clamp(1, MAX_SECTIONS)], square), level),
        // (A rounded cylinder is round about Y with the radius `half[0]`, whatever `half[2]`
        // says: a recipe writes `[r, h, r]`, and a tree written another way is held too.)
        Prim::Rounded { half, disc: true, .. } => Reach::new([-half[0], -half[1], -half[0]], [half[0], half[1], half[0]], level),
        _ => {
            let (lo, hi, _, _) = bounds(&SdfNode { kind: Kind::Prim(*p), m: Modifiers::default() })?;
            Reach::new(lo, hi, level)
        }
    })
}

/// [`prim_reach`] for a column, from its [`ColumnTable`].
fn column_reach(t: &ColumnTable, level: f32) -> Reach {
    #[cfg(test)]
    DID.with(|d| d.borrow_mut().table_places += t.places());
    let (lo, hi) = t.at(level);
    if t.ratio.is_finite() {
        Reach::new(t.zero.0, t.zero.1, level * t.ratio * (1.0 + 1e-5)).meet((lo, hi))
    } else {
        Reach::new(lo, hi, 0.0)
    }
}

/// [`prim_reach`] for an ellipsoid of `radii`, turned so that its own axes are the columns
/// `axes` (in the parent's frame), scaled by `s` and moved to `at`: worked out in the
/// parent's frame, so that a turn does not take a box's corners out.
///
/// Its field is `(k0 - 1) / |n / radii|` where `n` is the unit direction of `q / radii` and
/// `k0 = |q / radii|` ([`prim_reach`]), so where it is at most `level` the point is its
/// surface's point `radii * n` and an offset `(k0 - 1) |radii * n|`, at most `level |n /
/// radii| |radii * n|`, which is at most `level (rho + 1 / rho) / 2` (Kantorovich's
/// inequality; `rho` the greatest radius over the least): **the ellipsoid and a ball** of
/// that radius, the ellipsoid reaching `|radii * d|` along a unit direction `d` of its own.
/// And along each of its own axes, exactly [`ellipsoid_reach`]'s, which bounds it along any
/// direction `d` by `sum |d_i| e_i`. Both, met.
fn ellipsoid_placed(radii: V3, level: f32, axes: [V3; 3], s: f32, at: V3) -> Reach {
    let least = radii[0].min(radii[1]).min(radii[2]);
    let most = radii[0].max(radii[1]).max(radii[2]);
    let e = radii.map(|a| ellipsoid_reach(a, least, level));
    let rho = most / least;
    let ball = if rho.is_finite() { s * level * 0.5 * (rho + 1.0 / rho) * (1.0 + 1e-6) } else { f32::MAX };
    let (mut lo, mut hi, mut wlo, mut whi) = ([0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3]);
    for w in 0..3 {
        // The parent's axis `w` in the ellipsoid's own frame.
        let d = [axes[0][w], axes[1][w], axes[2][w]];
        let shape = s * len2(len2(radii[0] * d[0], radii[1] * d[1]), radii[2] * d[2]) * (1.0 + 1e-6);
        let axis = s * (0..3).map(|j| d[j].abs() * e[j]).sum::<f32>() * (1.0 + 1e-6);
        (lo[w], hi[w], wlo[w], whi[w]) = (at[w] - shape, at[w] + shape, at[w] - axis, at[w] + axis);
    }
    if ball == f32::MAX {
        return Reach::new(wlo, whi, 0.0);
    }
    Reach::new(lo, hi, ball).meet((wlo, whi))
}

/// The greatest ratio of a column's two halves either way anywhere along it (from its
/// curve's control points: [`column_piece_reach`]), as a squarer section's corner over its
/// smaller half (`len2(1, ratio)`) or, for an ellipse, its longer half over its shorter.
/// Infinite where a control is not above nothing.
fn column_ratio(used: &[[f32; 3]], square: f32) -> f32 {
    let mut most = 1.0f32;
    for i in 0..(used.len() - 1).max(1) {
        let (x, z) = (column_controls(used, i, 1), column_controls(used, i, 2));
        for k in 0..4 {
            if !(x[k] > 0.0 && z[k] > 0.0) {
                return f32::INFINITY;
            }
            most = most.max(x[k] / z[k]).max(z[k] / x[k]);
        }
    }
    if square == 2.0 { most } else { len2(1.0, most) }
}

/// What one walk of a column's curve finds of its level sets, so that its box at any level
/// ([`ColumnTable::at`]) is a few products a place, **bit for bit the box the walk gave** (the
/// same operations on the same numbers, in the same order): its two ends' places, each axis's
/// reach of their sections before the cap's growth, and their caps; each piece's feet and, for
/// each axis, the two parts of [`column_piece_reach`] (its shape's reach, and what a level
/// adds); and its ratio ([`column_ratio`]). [`reach`] asks a column at many levels (in a chain
/// of 64 smooth unions over 8,000 times): until the table, every ask walked the curve again,
/// seconds of powers a mesh that nothing was charged for (third review of the box's search).
#[derive(Clone, Debug)]
struct ColumnTable {
    /// Each end: its place, its section's reach along each axis before its cap's growth, and
    /// its cap.
    ends: [(V3, V3, f32); 2],
    /// Each piece: its feet (the first one or two), and for each axis its reach's parts.
    pieces: Vec<([V3; 2], usize, [PieceReach; 3])>,
    ratio: f32,
    /// Its box at level 0 (its solid's).
    zero: (V3, V3),
}

/// One piece's reach along one axis at a level ([`column_piece_reach`]): `(shape + level *
/// grow) * spare` where its controls are all above nothing, else `most * (1 + level / cap)`.
#[derive(Clone, Copy, Debug)]
enum PieceReach {
    Even { shape: f32, grow: f32 },
    Odd { most: f32, cap: f32 },
}

impl PieceReach {
    fn at(self, level: f32) -> f32 {
        match self {
            PieceReach::Even { shape, grow } => (shape + level * grow) * (1.0 + 1e-5),
            PieceReach::Odd { most, cap } => most * (1.0 + level / cap),
        }
    }
}

impl ColumnTable {
    /// The box the column's field is at most `level` in.
    fn at(&self, level: f32) -> (V3, V3) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &(end, lateral, cap) in &self.ends {
            let g = 1.0 + level / cap;
            for w in 0..3 {
                let out = g * lateral[w] * (1.0 + 1e-5);
                lo[w] = lo[w].min(end[w] - out);
                hi[w] = hi[w].max(end[w] + out);
            }
        }
        for (feet, n, by) in &self.pieces {
            for w in 0..3 {
                let out = by[w].at(level);
                for f in &feet[..*n] {
                    lo[w] = lo[w].min(f[w] - out);
                    hi[w] = hi[w].max(f[w] + out);
                }
            }
        }
        (lo, hi)
    }

    /// (Tests only) the places [`ColumnTable::at`] works out: three an end, and each axis of
    /// each foot of each piece.
    #[cfg(test)]
    fn places(&self) -> u64 {
        6 + self.pieces.iter().map(|p| 3 * p.1 as u64).sum::<u64>()
    }
}

/// The most places a column's [`ColumnTable`] has ([`ColumnTable::places`]), from its number
/// of sections alone: what [`reach`] is charged for each level it asks of it before it asks.
fn column_places(sections: usize) -> u64 {
    let segments = (sections.max(1) - 1).max(1) as u64;
    6 + segments * COLUMN_PIECES as u64 * 3 * 2
}

/// [`ColumnTable`] of a column: one walk of its curve.
fn column_table(a: V3, b: V3, side: V3, used: &[[f32; 3]], square: f32) -> ColumnTable {
    let axis = math3::sub(b, a);
    let last = used.len() - 1;
    // Its own frame, as [`column`] takes it.
    let up = math3::scale(axis, 1.0 / math3::length(axis).max(1e-6));
    let lean = math3::sub(side, math3::scale(up, math3::dot(side, up)));
    let right = if math3::length(lean) > 1e-6 { math3::normalize(lean) } else { math3::perpendicular(up) };
    let across = math3::cross(up, right);
    let foot = |t: f32| math3::add(a, math3::scale(axis, t));
    let (first, rising) = ((foot(used[0][0]), foot(used[last][0])), used.windows(2).all(|w| w[1][0] > w[0][0]));
    // Past an end, the section there (`column_sizes` holds it) and its cap: `k` is the
    // section's measure and how many caps along the point is, together, so the point is in
    // that section's shape and `cap` along, grown `g` times as one (an ellipsoid's half for a
    // round section), whose reach along an axis is `g` times the root of the section's
    // support squared and the cap's share squared. (The whole of it, on both sides of the
    // end: the side within is the pieces' too.)
    let end = |place: V3, s: [f32; 3]| {
        let cap = s[1].min(s[2]);
        (place, [0, 1, 2].map(|w| len2(section_support(s[1], s[2], square, right[w].abs(), across[w].abs()), up[w].abs() * cap)), cap)
    };
    let ends = [end(first.0, used[0]), end(first.1, used[last])];
    // Piece by piece of the curve between each two neighbouring sections: the stretch of the
    // axis its feet lie on, and how far out from it each way its level set reaches
    // ([`column_piece_reach`]).
    let mut pieces = Vec::with_capacity(last.max(1) * COLUMN_PIECES);
    for i in 0..last.max(1) {
        let (x, z) = (column_controls(used, i, 1), column_controls(used, i, 2));
        for piece in 0..COLUMN_PIECES {
            #[cfg(test)]
            DID.with(|d| d.borrow_mut().pieces += 1);
            let (px, pz) = (bezier_piece(x, piece), bezier_piece(z, piece));
            let (feet, n) = if last == 0 {
                ([first.0; 2], 1)
            } else if rising {
                // (`column_sizes` takes a point at `t` from the curve of the pair it is
                // between, at the share of their span it is along.)
                let span = used[i + 1][0] - used[i][0];
                let at = |p: usize| foot(used[i][0] + span * p as f32 / COLUMN_PIECES as f32);
                ([at(piece), at(piece + 1)], 2)
            } else {
                // (Sections that do not rise, which no recipe writes: anywhere along.)
                ([first.0, first.1], 2)
            };
            pieces.push((feet, n, [0, 1, 2].map(|w| column_piece_reach(px, pz, square, right[w].abs(), across[w].abs()))));
        }
    }
    let mut t = ColumnTable { ends, pieces, ratio: column_ratio(used, square), zero: ([0.0; 3], [0.0; 3]) };
    t.zero = t.at(0.0);
    t
}

/// The four control points of the curve [`column_sizes`] draws one size (`c`: 1 the half
/// breadth, 2 the half depth) along between sections `i` and `i + 1`, as a cubic Bézier: the
/// same Hermite curve (its ends the sections, its slopes there `span * slope / 3` in), so the
/// size at any point between is a weighted mean of these four with weights that are never
/// below nothing and add to one. One section: that size four times.
fn column_controls(sections: &[[f32; 3]], i: usize, c: usize) -> [f32; 4] {
    if sections.len() < 2 {
        return [sections[0][c]; 4];
    }
    // (As `column_sizes` takes them, to the letter.)
    let span = (sections[i + 1][0] - sections[i][0]).max(1e-6);
    let rise = |j: usize| (sections[j + 1][c] - sections[j][c]) / (sections[j + 1][0] - sections[j][0]).max(1e-6);
    let slope = |j: usize| {
        if j == 0 {
            rise(0)
        } else if j + 1 == sections.len() {
            rise(j - 1)
        } else {
            let (before, after) = (rise(j - 1), rise(j));
            if before * after <= 0.0 { 0.0 } else { 2.0 * before * after / (before + after) }
        }
    };
    [sections[i][c], sections[i][c] + span * slope(i) / 3.0, sections[i + 1][c] - span * slope(i + 1) / 3.0, sections[i + 1][c]]
}

/// How many pieces [`prim_reach`] cuts the curve between two of a column's sections into.
const COLUMN_PIECES: usize = 8;

/// The control points of one of [`COLUMN_PIECES`] equal pieces of a cubic Bézier (by de
/// Casteljau's construction, in double precision): the same curve over that piece, so its
/// points are weighted means of these four.
fn bezier_piece(c: [f32; 4], piece: usize) -> [f32; 4] {
    let split = |p: [f64; 4], t: f64| {
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        let (a, b, c) = (lerp(p[0], p[1]), lerp(p[1], p[2]), lerp(p[2], p[3]));
        let (d, e) = (lerp(a, b), lerp(b, c));
        let f = lerp(d, e);
        ([p[0], a, d, f], [f, e, c, p[3]])
    };
    let n = COLUMN_PIECES as f64;
    let (t0, t1) = (piece as f64 / n, (piece + 1) as f64 / n);
    let left = split(c.map(f64::from), t1).0;
    let part = if t0 > 0.0 { split(left, t0 / t1).1 } else { left };
    part.map(|v| v as f32)
}

/// How far a column's section of halves `rx` and `rz` (an ellipse, or squarer: exponent
/// `square`) reaches along a line `u` of whose length lies along the column's side and `v`
/// across it: its support, `|(u rx, v rz)|_q` with `q = square / (square - 1)` (Hölder's
/// inequality, met with equality), which is convex and grows with each half.
fn section_support(rx: f32, rz: f32, square: f32, u: f32, v: f32) -> f32 {
    #[cfg(test)]
    DID.with(|d| {
        let mut d = d.borrow_mut();
        if square == 2.0 {
            d.supports_root += 1;
        } else {
            d.supports_pow += 1;
        }
    });
    let (p, q) = ((u * rx).abs(), (v * rz).abs());
    if square == 2.0 {
        len2(p, q)
    } else {
        let dual = square / (square - 1.0);
        let sum = crate::dmath::pow(p, dual) + crate::dmath::pow(q, dual);
        if sum > 0.0 { crate::dmath::pow(sum, 1.0 / dual) } else { 0.0 }
    }
}

/// How far out from its axis a column's level set reaches along one axis of the parent's frame
/// over a piece of the curve between two sections, from the piece's control points for the
/// half breadth `x` and the half depth `z` ([`column_controls`], [`bezier_piece`]); `u` and `v`
/// are how much of that axis lies along the column's side and across it.
///
/// The field is `(k - 1) * cap` with `cap` the smaller half there ([`column`]), so where it
/// is at most `level`, `k <= g = 1 + level / cap`: the point's offset from its foot is in the
/// section's own shape (an ellipse, or squarer: exponent `square`) grown `g` times, and along
/// the axis it reaches at most `g` times the support of that shape, `S(rx, rz) = |(u rx, v
/// rz)|_q` with `q` the dual exponent (`square / (square - 1)`: 2 for an ellipse). Since
/// `g S(rx, rz) = S(rx, rz) + level * S(rx, rz) / min(rx, rz)`:
///
/// - `S` is convex and grows with each half, and the pair of halves moves on a curve inside
///   the hull of the four pairs of controls, so the first is at most the greatest `S` of
///   those four;
/// - the second is `level` times `S(rx / rz, 1)` or `S(1, rz / rx)`, whichever half is the
///   smaller, and the ratio of the two halves, each a weighted mean of its controls with the
///   same weights, is never beyond the greatest ratio of the controls either way.
///
/// (Not, as it was, the greater half of two sections times `1 + level` over the least half
/// of either: where a column's proportions change from one section to the next, that is far
/// more than the field reaches, and every way at once.) Where a control is not above nothing
/// (a section no recipe writes), the greatest halves times `1 + level` over the least.
fn column_piece_reach(x: [f32; 4], z: [f32; 4], square: f32, u: f32, v: f32) -> PieceReach {
    let support = |p: f32, q: f32| section_support(p, q, square, u, v);
    // (A hundred-thousandth to spare for the rounding of the curve, its pieces and the powers:
    // [`PieceReach::at`].)
    if x.iter().chain(&z).all(|&v| v > 0.0 && v.is_finite()) {
        let ratio = |a: &[f32; 4], b: &[f32; 4]| (0..4).map(|k| a[k] / b[k]).fold(1.0f32, f32::max);
        let (rx, rz) = (ratio(&x, &z), ratio(&z, &x));
        let shape = (0..4).map(|k| support(x[k], z[k])).fold(0.0f32, f32::max);
        PieceReach::Even { shape, grow: support(rx, 1.0).max(support(1.0, rz)) }
    } else {
        let most = |c: &[f32; 4]| c.iter().fold(0.0f32, |a, &b| a.max(b));
        let cap = x.iter().chain(&z).fold(f32::MAX, |a, &b| a.min(b));
        PieceReach::Odd { most: u * most(&x) + v * most(&z), cap }
    }
}

/// How far along an axis of radius `a` an ellipsoid whose least radius is `least` reaches at
/// a level of its field ([`prim_reach`]): the greatest of `h(s) = a s (1 + level sqrt(s^2 /
/// a^2 + (1 - s^2) / least^2))` for `s` from 0 to 1. At `s = 1` it is `a + level`. Inside,
/// writing `g = least^-2 - c s^2` with `c = least^-2 - a^-2`, `h' = 0` asks `least^-2 - 2 c s^2
/// = -sqrt(g) / level`, which squared is a quadratic in `s^2`: every greatest value inside is
/// at one of its roots where the left side is below nothing. The greatest of those and the
/// end, in double precision, with a millionth to spare.
fn ellipsoid_reach(a: f32, least: f32, level: f32) -> f32 {
    // (The end is exact, as a ball's box is: no margin.)
    let end = a + level;
    let (a, least, level) = (f64::from(a), f64::from(least), f64::from(level));
    let alpha = 1.0 / (least * least);
    let c = alpha - 1.0 / (a * a);
    let mut best = 0.0f64;
    if c > 0.0 && level > 0.0 {
        let h = |y: f64| {
            let s = y.sqrt();
            a * s * (1.0 + level * (alpha - c * y).max(0.0).sqrt())
        };
        let (qa, qb, qc) = (4.0 * c * c * level * level, c - 4.0 * c * alpha * level * level, alpha * alpha * level * level - alpha);
        let disc = qb * qb - 4.0 * qa * qc;
        if disc >= 0.0 {
            for y in [(-qb + disc.sqrt()) / (2.0 * qa), (-qb - disc.sqrt()) / (2.0 * qa)] {
                if y > 0.0 && y <= 1.0 && alpha - 2.0 * c * y < 0.0 {
                    best = best.max(h(y));
                }
            }
        }
    }
    end.max((best * (1.0 + 1e-6)) as f32)
}

/// The box a twist and a bend ([`local`]: a twist turns a point about Y by `twist` degrees a
/// metre of its height, then a bend about Z by `bend` degrees a metre of its x) take a box
/// and a ball of radius `r` back to. Both turn about an axis through the middle, so the
/// ball keeps its radius about the axis, and each turns by no more than its rate times the
/// greatest the coordinate it goes by can be: under a quarter turn, a coordinate grows by at
/// most the other's greatest times the sine; at a quarter turn or more, to the radius.
fn turned_reach(lo: V3, hi: V3, r: f32, twist: f32, bend: f32) -> (V3, V3) {
    let (lo, hi) = (math3::sub(lo, [r; 3]), math3::add(hi, [r; 3]));
    let most = [0, 1, 2].map(|k| lo[k].abs().max(hi[k].abs()));
    // How far a coordinate pair `(x, y)` can be from where it was turned by up to `angle`
    // radians: each grows by the other's greatest times the sine, never past their radius.
    let turned = |x: f32, y: f32, angle: f32| {
        let radius = len2(x, y);
        if angle < 0.5 * std::f32::consts::PI {
            let (s, _) = crate::dmath::sin_cos(angle);
            ((x + y * s).min(radius), (y + x * s).min(radius))
        } else {
            (radius, radius)
        }
    };
    let deg = std::f32::consts::PI / 180.0;
    // Out of the bend first (it was applied last): its angle goes by x before it, which is no
    // more than the radius about Z.
    let (mut lo, mut hi) = (lo, hi);
    let mut t = most;
    if bend != 0.0 {
        let (x, y) = turned(most[0], most[1], bend.abs() * deg * len2(most[0], most[1]));
        t = [x, y, most[2]];
        lo[0] = -x;
        hi[0] = x;
        lo[1] = -y;
        hi[1] = y;
    }
    if twist != 0.0 {
        // Its angle goes by y, which the twist keeps.
        let (x, z) = turned(t[0], t[2], twist.abs() * deg * t[1]);
        lo[0] = -x;
        hi[0] = x;
        lo[2] = -z;
        hi[2] = z;
    }
    (lo, hi)
}

/// A cell's edge between two of its corners (each corner three bits: 1 for +x, 2 for +y, 4
/// for +z; the two differ in one). Edge `axis · 4 + o1 + 2 · o2`, where `o1` and `o2` are its
/// start's offsets along the next axis and the one after.
const fn edge_between(a: usize, b: usize) -> usize {
    let axis = (a ^ b).trailing_zeros() as usize;
    let start = a & b;
    axis * 4 + ((start >> ((axis + 1) % 3)) & 1) + 2 * ((start >> ((axis + 2) % 3)) & 1)
}

/// The two corners of a cell's edge `e` ([`edge_between`]'s numbering), its start first.
const fn edge_ends(e: usize) -> (usize, usize) {
    let axis = e / 4;
    let start = ((e & 1) << ((axis + 1) % 3)) | (((e >> 1) & 1) << ((axis + 2) % 3));
    (start, start | 1 << axis)
}

/// The four cells round an edge of the grid along each axis, as offsets from the cell whose
/// lowest corner is the edge's start, in turn round the edge (counter-clockwise seen from
/// +axis): each two in turn share a face.
const AROUND: [[(isize, isize, isize); 4]; 3] = [
    [(0, -1, -1), (0, 0, -1), (0, 0, 0), (0, -1, 0)],
    [(-1, 0, -1), (-1, 0, 0), (0, 0, 0), (0, 0, -1)],
    [(-1, -1, 0), (0, -1, 0), (0, 0, 0), (-1, 0, 0)],
];

/// Work units for grouping one cell's crossings into sheets ([`sheets`]: six faces, at most
/// twelve unions of twelve edges), charged for each cell with six crossings or more.
const SHEETS_WORK: u64 = 40;

/// A face of a cell (`n` its normal's axis, `side` 0 or 1): its four corners in turn round it,
/// and its four edges, edge `q` from corner `q` to corner `q + 1`.
const fn face_ring(n: usize, side: usize) -> ([usize; 4], [usize; 4]) {
    let (u, v) = ((n + 1) % 3, (n + 2) % 3);
    let base = side << n;
    let c = [base, base | 1 << u, base | 1 << u | 1 << v, base | 1 << v];
    (c, [edge_between(c[0], c[1]), edge_between(c[1], c[2]), edge_between(c[2], c[3]), edge_between(c[3], c[0])])
}

/// No sheet: an edge of the cell the surface does not cross.
const NO_SHEET: u8 = u8::MAX;

/// **The sheets that pass through a cell.** `inside` is each corner's side (bit `c` for corner
/// `c`); `joined(n, side)` is asked only of a face whose four corners go inside, outside,
/// inside, outside in turn, and says whether its two inside corners are joined across its
/// middle (the field there is inside). Returns how many sheets, and for each of the twelve
/// edges the sheet it belongs to ([`NO_SHEET`] where it does not cross).
///
/// On each face the crossings are paired as the surface leaves them on that face: two
/// crossings are one stretch of the surface's trace on the face; four are two stretches, each
/// cutting off one of the two corners the middle does not join. Each crossing lies on two
/// faces, so the stretches chain the crossings into loops round the cell: one loop is one
/// sheet, the edge of a disc of surface inside the cell. A face's pairing is the face's own
/// (its corners and `joined`), so the two cells either side of it pair alike. Sheets are
/// numbered by their first edge.
fn sheets(inside: u8, mut joined: impl FnMut(usize, usize) -> bool) -> (u8, [u8; 12]) {
    let side = |c: usize| inside >> c & 1 == 1;
    let crossing = |e: usize| {
        let (a, b) = edge_ends(e);
        side(a) != side(b)
    };
    // (Twelve edges; a union of two takes at most twelve steps up.)
    let mut up: [u8; 12] = std::array::from_fn(|e| e as u8);
    let root = |up: &[u8; 12], mut e: usize| {
        while up[e] as usize != e {
            e = up[e] as usize;
        }
        e
    };
    let join = |up: &mut [u8; 12], a: usize, b: usize| {
        let (ra, rb) = (root(up, a), root(up, b));
        up[ra.max(rb)] = ra.min(rb) as u8;
    };
    for n in 0..3 {
        for s in 0..2 {
            let (c, r) = face_ring(n, s);
            let cross: [bool; 4] = r.map(crossing);
            match cross.iter().filter(|&&x| x).count() {
                2 => {
                    let mut it = (0..4).filter(|&q| cross[q]);
                    let (a, b) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
                    join(&mut up, r[a], r[b]);
                }
                4 => {
                    // Corner 0 is cut off when the middle joins the other side's corners.
                    if side(c[0]) != joined(n, s) {
                        join(&mut up, r[3], r[0]);
                        join(&mut up, r[1], r[2]);
                    } else {
                        join(&mut up, r[0], r[1]);
                        join(&mut up, r[2], r[3]);
                    }
                }
                _ => {}
            }
        }
    }
    let mut of = [NO_SHEET; 12];
    let mut count = 0u8;
    for e in 0..12 {
        if !crossing(e) {
            continue;
        }
        let r = root(&up, e);
        if of[r] == NO_SHEET {
            of[r] = count;
            count += 1;
        }
        of[e] = of[r];
    }
    (count, of)
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
    // Work units per evaluation (â‰ˆ ns: noise octaves weigh what they cost).
    let cost = eval_cost(tree);
    let mut work = 0u64;
    // What rounded shapes' samples took past what their weight covers ([`ROUNDED_ALLOW`]
    // powers a sample) is charged at each charge after it, [`ROUNDED_POWER`] a power: the
    // powers taken, less the allowance of the samples so far, never charged twice. So the
    // work bounds the powers whatever a sample takes, and a sample takes at most
    // [`ROUNDED_MOST`]: between two charges no more than a batch's samples' worth runs
    // unpaid. (Whatever this thread evaluated before the build is not the build's. Started
    // before the box's search, whose evaluations take powers too: the second review of the
    // search found them never charged.)
    let bill = std::cell::RefCell::new(RoundedBill::start());
    let charge = |work: &mut u64, n: u64| -> Result<(), SdfError> {
        #[cfg(test)]
        cancel_hook(true);
        *work += n + bill.borrow_mut().due();
        #[cfg(test)]
        let _after = Defer(|| cancel_hook(false));
        if *work > opt.max_work {
            return Err(SdfError::limit(format!("the sdf needs more than {} work units ({cost} per sample: noise octaves and twists weigh most); lower the resolution or simplify the tree", opt.max_work), *work as f64, opt.max_work as f64));
        }
        if crate::work::cancelled() {
            return Err(SdfError::cancelled());
        }
        Ok(())
    };
    // (The automatic box is worked out only where the recipe gives none, its search charged
    // as it is spent: [`mesh_box_charged`].)
    let auto = if opt.bounds.is_some() { None } else { mesh_box_charged(tree, opt.resolution, opt.seed, &mut |n| charge(&mut work, n))? };
    let (box_spent, (auto_lo, auto_hi)) = match auto {
        Some((b, spent)) => (spent, b),
        None if opt.bounds.is_some() => (Spent::default(), ([0.0; 3], [0.0; 3])),
        None => return Err(SdfError::geometry("the sdf is unbounded (a half-space in a union): give it \"bounds\"")),
    };
    // Every node's Lipschitz factor and slack, bounded or not (an unbounded displaced cutter
    // moves the surface as much as a bounded one).
    let (lip, slack) = lip_slack(tree);
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
    let mut prepared = prepare(tree, opt.seed, &mut 0);
    // Whether the tree's factor bounds its field across any ball (DISTANCE_FIELDS 2.6): not
    // with an ellipsoid whose radii differ, a column whose breadth and depth change in
    // different proportions, or a repetition (its field jumps at its cells' edges) anywhere in
    // it. Then a block the factor shows clear is shown clear again by [`field_range_as`].
    let factor_holds = !prepared.loose && prepared.wander == 0.0 && !has_repeat(tree);
    #[cfg(test)]
    let factor_holds = factor_holds || OLD_FACTORS.with(|f| f.get());
    // Only then are ranges asked (of a block, and of a loose child when a block is pruned), and
    // then every column of the tree is walked once first, charged before it is walked
    // ([`outline_columns`]), so that each range asks its outline and is charged what a range
    // costs with it in hand ([`own_range_cost`]), never a walk an ask (review 4 of the box
    // lane: unoutlined, a column's range walked its curve again at every block).
    let mut range_work = 0u64;
    if !factor_holds {
        outline_columns(&mut prepared, &mut |n| {
            range_work += n;
            charge(&mut work, n)
        })?;
    }
    let prepared = prepared;
    // What a block's range costs at one copy of everything ([`range_cost`]): charged before
    // the range is asked, and whatever it met past that after (as the box's search is).
    let block_range_cost = if factor_holds { 0 } else { range_cost(&prepared) };
    let mut stats = SdfStats { dims: dims.map(|d| d as u32), cell, ..SdfStats::default() };
    // What making sure of the box took (none where the reach passes no side of `bounds`'s),
    // already charged.
    (stats.box_asks, stats.box_work, stats.reach_work) = (box_spent.asks, box_spent.work, box_spent.reach);
    // What keeping sheets apart charges (cells grouped, faces asked, places shared, slivers
    // tested): all of it, in [`SdfStats::sheets_work`] too. (Tests can leave it out, to see
    // that the work is that much less.)
    let sheets_part = |n: u64| -> u64 {
        #[cfg(test)]
        if FREE_SHEETS.with(|f| f.get()) {
            return 0;
        }
        n
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
                #[cfg(test)]
                let dc = if NO_SKIP.with(|f| f.get()) { 0.0 } else { dc };
                // Clear by the tree's factor; and where that factor is no bound of its field
                // across the block (a flat ellipsoid, a column of changing proportions, a
                // repetition: `factor_holds`), shown clear by the range its field takes there,
                // worked out through the tree ([`field_range_as`]).
                let clear = dc.abs() > half_diag * lip + slack + cell
                    && (factor_holds || {
                        charge(&mut work, block_range_cost)?;
                        let mut spent = Spent::default();
                        let (least, most) = field_range_as(&prepared, c, half_diag * 1.001, &mut 1, &mut spent, true);
                        let rest = spent.work.saturating_sub(block_range_cost);
                        range_work += block_range_cost + rest;
                        charge(&mut work, rest)?;
                        least > 0.0 || most < 0.0
                    });
                if clear {
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
                // (Finding what is near costs one more evaluation of the tree it is found in, and
                // what the ranges of its loose children evaluate, by their weights.)
                charge(&mut work, cost)?;
                let mut spent = Spent::default();
                let here = prune_asking(&prepared, c, half_diag + 0.25 * cell, &mut spent);
                range_work += spent.work;
                charge(&mut work, spent.work)?;
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
                    let mut spent = Spent::default();
                    let half = prune_asking(&here, middle, reach, &mut spent);
                    range_work += spent.work;
                    charge(&mut work, spent.work)?;
                    let half_cost = prepared_cost(&half);
                    for (lo, hi) in boxes(half_lo, half_hi, FINE) {
                        let (middle, reach) = ball(lo, hi);
                        charge(&mut work, half_cost)?;
                        let mut spent = Spent::default();
                        let quarter = prune_asking(&half, middle, reach, &mut spent);
                        range_work += spent.work;
                        charge(&mut work, spent.work)?;
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
    // Where the surface crosses the edge of the grid from sample (i, j, k) along `axis`
    // (linear in its two samples).
    let crossing_point = |i: usize, j: usize, k: usize, axis: usize| -> V3 {
        let (i2, j2, k2) = (i + usize::from(axis == 0), j + usize::from(axis == 1), k + usize::from(axis == 2));
        let (d0, d1) = (field[idx(i, j, k)], field[idx(i2, j2, k2)]);
        let t = if d0 == d1 { 0.5 } else { (d0 / (d0 - d1)).clamp(0.0, 1.0) };
        math3::lerp(at(i, j, k), at(i2, j2, k2), t)
    };
    // The tree that holds what begins at sample (i, j, k) and its cost: the quarter block it
    // begins in holds the whole of an edge or a face of the grid from there (when it was
    // sampled: one in a block that was filled by its sign asks the whole tree).
    let tree_at = |i: usize, j: usize, k: usize| -> (&Prepared, u64) {
        let slot = near_at[((k / FINE) * fine[1] + j / FINE) * fine[0] + i / FINE];
        near.get(slot as usize).map_or((&prepared, cost), |(t, c)| (t, *c))
    };
    // Per cell: accumulated QEF (Aáµ€A, Aáµ€b as f64), mass point, count.
    let cells = [dims[0] - 1, dims[1] - 1, dims[2] - 1];
    let cidx = |i: usize, j: usize, k: usize| (k * cells[1] + j) * cells[0] + i;
    #[derive(Clone, Copy, Default)]
    struct Qef {
        ata: [f64; 6],
        atb: [f64; 3],
        mass: [f64; 3],
        n: u32,
    }
    let add = |q: &mut Qef, p: V3, n: V3| {
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
    // Edges (sample, axis), in grid order, with their crossing's point and normal; and which
    // cells the surface passes through (every cell round a crossing).
    let ncells = cells[0] * cells[1] * cells[2];
    let mut first = vec![u32::MAX; ncells];
    let in_grid = |c: (isize, isize, isize)| c.0 >= 0 && c.1 >= 0 && c.2 >= 0 && (c.0 as usize) < cells[0] && (c.1 as usize) < cells[1] && (c.2 as usize) < cells[2];
    let mut edges: Vec<(usize, usize, usize, usize, bool, V3, V3)> = Vec::with_capacity(crossings);
    for k in 0..dims[2] {
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                let s = idx(i, j, k);
                for (axis, round) in AROUND.iter().enumerate() {
                    let (i2, j2, k2) = match axis {
                        0 => (i + 1, j, k),
                        1 => (i, j + 1, k),
                        _ => (i, j, k + 1),
                    };
                    if i2 >= dims[0] || j2 >= dims[1] || k2 >= dims[2] {
                        continue;
                    }
                    if inside(s) == inside(idx(i2, j2, k2)) {
                        continue;
                    }
                    let p = crossing_point(i, j, k, axis);
                    let (tree_here, cost_here) = tree_at(i, j, k);
                    charge(&mut work, 6 * cost_here)?;
                    let n = grad(tree_here, p);
                    for &(di, dj, dk) in round {
                        let c = (i as isize + di, j as isize + dj, k as isize + dk);
                        if in_grid(c) {
                            first[cidx(c.0 as usize, c.1 as usize, c.2 as usize)] = 0;
                        }
                    }
                    edges.push((i, j, k, axis, inside(s), p, n));
                }
            }
        }
    }
    // **The sheets through each surface cell** ([`sheets`]): one vertex each. A cell with fewer
    // than six crossings has one sheet and no face of four crossings (one sheet's loop crosses
    // at least three edges, and a face of four makes at least seven), as every cell had before;
    // the others are grouped, at [`SHEETS_WORK`] each, and a face of four crossings is decided
    // by the field at its middle, asked of the tree that holds the face (whichever cell asks,
    // the same tree, the same point, the same answer).
    let mut slots = 0u32;
    let mut labels: Vec<[u8; 12]> = Vec::new();
    let mut many = vec![u32::MAX; ncells];
    // Faces of four crossings, each once (from the cell below it along its axis), with the
    // answer at its middle.
    let mut fours: Vec<(usize, usize, bool)> = Vec::new();
    let (mut thin_lo, mut thin_hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                let ci = cidx(i, j, k);
                if first[ci] == u32::MAX {
                    continue;
                }
                let mut bits = 0u8;
                for c in 0..8 {
                    bits |= u8::from(inside(idx(i + (c & 1), j + (c >> 1 & 1), k + (c >> 2)))) << c;
                }
                let crossed = (0..12).filter(|&e| {
                    let (a, b) = edge_ends(e);
                    (bits >> a & 1) != (bits >> b & 1)
                });
                let (count, of) = if crossed.count() < 6 {
                    (1, [0; 12])
                } else {
                    charge(&mut work, sheets_part(SHEETS_WORK))?;
                    stats.sheets_work += SHEETS_WORK;
                    stats.sheet_cells += 1;
                    let mut joined = [false; 6];
                    let mut four = false;
                    for n in 0..3 {
                        for s in 0..2 {
                            let (c, _) = face_ring(n, s);
                            let side = c.map(|c| bits >> c & 1);
                            if side[0] == side[2] && side[1] == side[3] && side[0] != side[1] {
                                // The face's own lowest sample, and its middle.
                                let (fi, fj, fk) = (i + usize::from(s == 1 && n == 0), j + usize::from(s == 1 && n == 1), k + usize::from(s == 1 && n == 2));
                                let mut mid = at(fi, fj, fk);
                                mid[(n + 1) % 3] += 0.5 * cell;
                                mid[(n + 2) % 3] += 0.5 * cell;
                                let (tree_here, cost_here) = tree_at(fi, fj, fk);
                                charge(&mut work, sheets_part(cost_here))?;
                                stats.sheets_work += cost_here;
                                stats.faces_asked += 1;
                                joined[2 * n + s] = eval(tree_here, mid) < 0.0;
                                four = true;
                                if s == 1 {
                                    fours.push((ci, n, joined[2 * n + s]));
                                }
                            }
                        }
                    }
                    let (count, of) = sheets(bits, |n, s| joined[2 * n + s]);                    if four || count > 1 {
                        stats.thin_cells += 1;
                        let lo = at(i, j, k);
                        (thin_lo, thin_hi) = (math3::vmin(thin_lo, lo), math3::vmax(thin_hi, math3::add(lo, [cell; 3])));
                    }
                    (count, of)
                };
                first[ci] = slots;
                slots += u32::from(count);
                if count > 1 {
                    many[ci] = labels.len() as u32;
                    labels.push(of);
                }
            }
        }
    }
    stats.thin_at = (stats.thin_cells > 0).then_some((thin_lo, thin_hi));
    // The sheet an edge of the grid belongs to in a cell round it.
    let sheet_in = |c: (usize, usize, usize), i: usize, j: usize, k: usize, axis: usize| -> u32 {
        let m = many[cidx(c.0, c.1, c.2)];
        if m == u32::MAX {
            return 0;
        }
        let start = (i - c.0) | (j - c.1) << 1 | (k - c.2) << 2;
        u32::from(labels[m as usize][edge_between(start, start | 1 << axis)])
    };
    // **A thread thinner than a cell through a face.** Where one sheet of each cell either side
    // of a face of four crossings takes both of its stretches, the two cells' vertices would be
    // joined twice, once round each side of what passes between the face's two cut corners: the
    // thread's section a two-sided polygon. Each stretch then gets a vertex of its own on the
    // face (its two crossings' middle, an eighth of the way to the corner it cuts off), set
    // between the two cells' vertices in the quads of its two crossings: a section of four.
    let mut threads: Vec<(usize, usize, [u32; 4])> = Vec::new();
    let mut on_faces: Vec<V3> = Vec::new();
    for &(ci, n, joined) in &fours {
        let (i, j, k) = (ci % cells[0], (ci / cells[0]) % cells[1], ci / (cells[0] * cells[1]));
        let up = (i + usize::from(n == 0), j + usize::from(n == 1), k + usize::from(n == 2));
        let (c, r) = face_ring(n, 1);
        let one = |cell: (usize, usize, usize), s: usize| {
            let m = many[cidx(cell.0, cell.1, cell.2)];
            m == u32::MAX || {
                let (_, r) = face_ring(n, s);
                r.iter().all(|&e| labels[m as usize][e] == labels[m as usize][r[0]])
            }
        };
        if !(up.0 < cells[0] && up.1 < cells[1] && up.2 < cells[2] && one((i, j, k), 1) && one(up, 0)) {
            continue;
        }
        // Corner 0 is cut off when the middle joins the other side's corners ([`sheets`]).
        let cut0 = (inside(idx(i + (c[0] & 1), j + (c[0] >> 1 & 1), k + (c[0] >> 2)))) != joined;
        let pairs: [(usize, usize, usize); 2] = if cut0 { [(3, 0, 0), (1, 2, 2)] } else { [(0, 1, 1), (2, 3, 3)] };
        let mut verts = [u32::MAX; 4];
        for (a, b, corner) in pairs {
            let point = |q: usize| {
                let (s, e) = edge_ends(r[q]);
                let lo = s.min(e);
                crossing_point(i + (lo & 1), j + (lo >> 1 & 1), k + (lo >> 2), r[q] / 4)
            };
            let mid = math3::lerp(point(a), point(b), 0.5);
            let cc = c[corner];
            let id = slots + on_faces.len() as u32;
            on_faces.push(math3::lerp(mid, at(i + (cc & 1), j + (cc >> 1 & 1), k + (cc >> 2)), 0.125));
            (verts[a], verts[b]) = (id, id);
        }
        threads.push((ci, n, verts));
    }
    stats.threads = threads.len() as u64;
    // (Each thread puts a vertex in four quads: a triangle more each.)
    let triangles = 2 * crossings + 4 * threads.len();
    if triangles > opt.max_triangles {
        return Err(SdfError::limit(format!("the sdf would mesh to {triangles} triangles; at most {} (lower the resolution)", opt.max_triangles), triangles as f64, opt.max_triangles as f64));
    }
    // Each sheet's QEF, from its own crossings, added in the grid's order.
    let mut qef = vec![Qef::default(); slots as usize];
    for &(i, j, k, axis, _, p, n) in &edges {
        for (di, dj, dk) in AROUND[axis] {
            let c = (i as isize + di, j as isize + dj, k as isize + dk);
            if in_grid(c) {
                let c = (c.0 as usize, c.1 as usize, c.2 as usize);
                add(&mut qef[(first[cidx(c.0, c.1, c.2)] + sheet_in(c, i, j, k, axis)) as usize], p, n);
            }
        }
    }
    // A vertex per sheet: the QEF's minimiser, pulled towards the mass point, kept in the cell.
    // (Numbered as the QEFs are: a cell's sheets in turn, cells in the grid's order.)
    let mut shape = Shape::default();
    let mut masses: Vec<V3> = Vec::with_capacity(slots as usize);
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                let ci = cidx(i, j, k);
                if first[ci] == u32::MAX {
                    continue;
                }
                let next = if many[ci] == u32::MAX { first[ci] + 1 } else { first[ci] + 1 + u32::from(*labels[many[ci] as usize].iter().filter(|&&s| s != NO_SHEET).max().unwrap_or(&0)) };
                for slot in first[ci]..next {
                    let q = qef[slot as usize];
                    let n = q.n as f64;
                    let mass = q.mass.map(|v| v / n);
                    let x = solve_qef(&q.ata, &q.atb, mass, 0.05 * n);
                    let lo = at(i, j, k);
                    shape.positions.push([0, 1, 2].map(|c| (x[c] as f32).clamp(lo[c], lo[c] + cell)));
                    masses.push(mass.map(|v| v as f32));
                }
            }
        }
    }
    shape.positions.extend_from_slice(&on_faces);
    for &(i, j, k, axis, inside_low, _, _) in &edges {
        let round = AROUND[axis].map(|(di, dj, dk)| ((i as isize + di) as usize, (j as isize + dj) as usize, (k as isize + dk) as usize));
        // The polygon round this edge: a vertex a cell, and one on a face between two of them
        // where a thread passes it.
        let mut poly = [0u32; 8];
        let (mut len, mut on_face) = (0, usize::MAX);
        for q in 0..4 {
            let c = round[q];
            poly[len] = first[cidx(c.0, c.1, c.2)] + sheet_in(c, i, j, k, axis);
            len += 1;
            if threads.is_empty() {
                continue;
            }
            let d = round[(q + 1) % 4];
            let low = (c.0.min(d.0), c.1.min(d.1), c.2.min(d.2));
            let n = if c.0 != d.0 { 0 } else if c.1 != d.1 { 1 } else { 2 };
            if let Ok(t) = threads.binary_search_by_key(&(cidx(low.0, low.1, low.2), n), |&(ci, n, _)| (ci, n)) {
                let start = (i - low.0) | (j - low.1) << 1 | (k - low.2) << 2;
                let e = edge_between(start, start | 1 << axis);
                let (_, r) = face_ring(n, 1);
                if let Some(q) = r.iter().position(|&x| x == e) {
                    poly[len] = threads[t].2[q];
                    on_face = on_face.min(len);
                    len += 1;
                }
            }
        }
        let poly = &mut poly[..len];
        // Counter-clockwise seen from +axis when the low end is inside (the surface faces +axis).
        if !inside_low {
            poly.reverse();
            if on_face != usize::MAX {
                on_face = len - 1 - on_face;
            }
        }
        if len == 4 {
            let quad = &*poly;
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
        } else {
            // A fan from a vertex on a face: its chords join it to vertices of this polygon
            // alone (that vertex is in one other polygon, whose cells differ), never two cells'
            // vertices across the face it stands on.
            poly.rotate_left(on_face);
            for w in 1..len - 1 {
                shape.tris.push([poly[0], poly[w], poly[w + 1]]);
            }
        }
    }
    // **Vertices kept apart where joining them leaves no surface.** Two cells' (or two
    // sheets') vertices can be held at one place by their cells' bounds (a corner or an edge
    // both cells share); the weld below joins them. Where the two are not an edge of the mesh,
    // or are one whose ends have a neighbour in common but the far corners of its two
    // triangles, that join would put four triangles on an edge or two fans on a point: each
    // is moved a sixteenth of the way to its own crossings' middle. (Rare; a scan of the
    // triangles, charged a unit a triangle, only when two vertices share a place.)
    charge(&mut work, sheets_part(shape.positions.len() as u64 / 8))?;
    stats.sheets_work += shape.positions.len() as u64 / 8;
    let mut by_place: Vec<(u32, [u32; 3])> = (0..slots).map(|v| (v, shape.positions[v as usize].map(|x| (x + 0.0).to_bits()))).collect();
    by_place.sort_unstable_by_key(|&(v, k)| (k, v));
    let shared: Vec<Vec<u32>> = by_place.chunk_by(|a, b| a.1 == b.1).filter(|run| run.len() > 1).map(|run| run.iter().map(|&(v, _)| v).collect()).collect();
    if !shared.is_empty() {
        charge(&mut work, sheets_part(shape.tris.len() as u64))?;
        stats.sheets_work += shape.tris.len() as u64;
        let mut near: std::collections::BTreeMap<u32, Vec<u32>> = shared.iter().flatten().map(|&v| (v, Vec::new())).collect();
        let mut far: std::collections::BTreeMap<(u32, u32), Vec<u32>> = std::collections::BTreeMap::new();
        for t in &shape.tris {
            for k in 0..3 {
                let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                if let Some(n) = near.get_mut(&a) {
                    n.extend([b, c]);
                }
                if near.contains_key(&a) && near.contains_key(&b) {
                    far.entry((a.min(b), a.max(b))).or_default().push(c);
                }
            }
        }
        for n in near.values_mut() {
            n.sort_unstable();
            n.dedup();
        }
        for group in &shared {
            let joins = group.iter().enumerate().all(|(x, &a)| {
                group[x + 1..].iter().all(|&b| {
                    let Some(mut f) = far.get(&(a.min(b), a.max(b))).cloned() else { return false };
                    f.sort_unstable();
                    f.dedup();
                    let common: Vec<u32> = near[&a].iter().copied().filter(|v| *v != b && near[&b].binary_search(v).is_ok()).collect();
                    common == f
                })
            });
            if !joins {
                for &v in group {
                    let p = shape.positions[v as usize];
                    shape.positions[v as usize] = math3::lerp(p, masses[v as usize], 0.0625);
                }
            }
        }
    }
    shape.tags = vec![TAG_FACE; shape.tris.len()];
    // Colours at the vertices.
    if has_color(&prepared) {
        charge(&mut work, shape.positions.len() as u64 * 2 * cost)?;
        let mut colors = Vec::with_capacity(shape.positions.len());
        for (i, p) in shape.positions.iter().enumerate() {
            // (What rounded shapes took past their weight, every so many vertices.)
            if i % 64 == 63 {
                charge(&mut work, 0)?;
            }
            colors.push(eval_color(&prepared, *p, opt.color).1);
        }
        shape.colors = colors;
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
    // **An edge is closed only where that leaves a surface**: where its two ends have a
    // neighbour in common other than the far corners of the two triangles on it, moving one
    // onto the other would put four triangles on one edge (a needle at a cone's tip, two cells'
    // vertices kept a hair apart on the line their cells share). Such a triangle's next edge is
    // tried. (Each test scans the triangles once, charged a unit a triangle.)
    let mut stuck = false;
    for _ in 0..4 {
        let mut moved = false;
        stuck = false;
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
            let len = |e: (usize, usize)| math3::length(math3::sub(shape.positions[e.0], shape.positions[e.1]));
            let mut order = [0, 1, 2];
            // (The shortest first, as it always was: a stable sort keeps the first of equals.)
            order.sort_by(|&x, &y| len(edges[x]).total_cmp(&len(edges[y])));
            let mut closed = false;
            for e in order {
                let (keep, go) = edges[e];
                charge(&mut work, sheets_part(shape.tris.len() as u64))?;
                stats.sheets_work += shape.tris.len() as u64;
                stats.closings_tested += 1;
                if !closes_to_a_surface(&shape, shape.positions[keep], shape.positions[go]) {
                    continue;
                }
                let at = shape.positions[keep];
                // Every vertex at the place that goes: the mesh is not welded yet.
                let gone = shape.positions[go];
                for q in shape.positions.iter_mut().filter(|q| **q == gone) {
                    *q = at;
                }
                (moved, closed) = (true, true);
                break;
            }
            stuck |= !closed;
        }
        if !moved {
            break;
        }
    }
    // (A sliver no edge could close in the last pass is left to the weld, which drops it; the
    // hole is counted in `tangled_edges` and said. None was met in 857 drawn trees meshed by
    // hand while this was made, nor in any case or recipe the tests mesh.)
    stats.sliver_left = stuck;
    // Triangles collapsed by vertices clamped or moved onto each other.
    shape.weld_exact();
    stats.tangled_edges = tangled_edges(&shape);
    // (The last samples' powers past their weight.)
    charge(&mut work, 0)?;
    let bill = bill.into_inner();
    (stats.rounded_powers, stats.rounded_samples, stats.rounded_most_between) = (bill.taken, bill.samples, bill.most_between);
    stats.work = work;
    stats.range_work = range_work;
    stats.trees_kept = near.len();
    stats.triangles = shape.tris.len();
    #[cfg(test)]
    MESHED.with(|m| {
        if let Some(all) = m.borrow_mut().as_mut() {
            all.push((shape.clone(), stats));
        }
    });
    Ok((shape, stats))
}

#[cfg(test)]
thread_local! {
    /// (Tests only) every mesh made on this thread while a test keeps the list, with what
    /// meshing it did: how a build's sculpts are reached (a build meshes each at its detail
    /// and, where it needs one, coarsely).
    static MESHED: std::cell::RefCell<Option<Vec<(Shape, SdfStats)>>> = const { std::cell::RefCell::new(None) };
    /// (Tests only) a cancel raised at a build's `n`-th charge (counted down), before that
    /// charge looks for one (`true`) or after (`false`): [`cancel_hook`].
    static CANCEL_AT: std::cell::Cell<Option<(u64, bool)>> = const { std::cell::Cell::new(None) };
    /// (Tests only) leave what keeping sheets apart costs out of the work.
    static FREE_SHEETS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// (Tests only) skip no block: every sample is evaluated, so a mesh made so is what the
    /// skip must not change.
    static NO_SKIP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// (Tests only) prune nothing.
    static NO_PRUNE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// (Tests only) skip and prune by the tree's factors alone, as before 2026-11-24: what
    /// lost surface beside flat ellipsoids.
    static OLD_FACTORS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// (Tests only) at a build's charge, before it looks for a cancel (`before`) or after: counts
/// [`CANCEL_AT`] down and raises the thread's cancel flag at its charge, in its phase.
#[cfg(test)]
fn cancel_hook(before: bool) {
    let Some((left, at_before)) = CANCEL_AT.with(|c| c.get()) else { return };
    let left = if before { left.saturating_sub(1) } else { left };
    CANCEL_AT.with(|c| c.set(Some((left, at_before))));
    if left == 0 && at_before == before {
        if let Some(flag) = crate::work::current_flag() {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// (Tests only) runs its closure when dropped.
#[cfg(test)]
struct Defer<F: FnMut()>(F);

#[cfg(test)]
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// (Tests only) every byte of a mesh, as one hash: its positions, triangles and colours.
#[cfg(test)]
fn mesh_hash(shape: &Shape) -> u64 {
    let mut h = specula_core::hash::StableHasher::new();
    for p in &shape.positions {
        p.iter().for_each(|x| h.write_u32(x.to_bits()));
    }
    for t in &shape.tris {
        t.iter().for_each(|x| h.write_u32(*x));
    }
    for c in &shape.colors {
        c.iter().for_each(|x| h.write_u32(x.to_bits()));
    }
    h.finish()
}

/// Whether moving every vertex at `q` onto `p` leaves a surface (the link condition): the
/// places that neighbour both, through triangles with three places, are only the far corners
/// of the triangles that hold both. One scan of the triangles.
fn closes_to_a_surface(shape: &Shape, p: V3, q: V3) -> bool {
    let key = |v: V3| v.map(|x| (x + 0.0).to_bits());
    let (kp, kq) = (key(p), key(q));
    let (mut by_p, mut by_q, mut far) = (Vec::new(), Vec::new(), Vec::new());
    for t in &shape.tris {
        let k = t.map(|v| key(shape.positions[v as usize]));
        if k[0] == k[1] || k[1] == k[2] || k[2] == k[0] {
            continue;
        }
        let (hp, hq) = (k.contains(&kp), k.contains(&kq));
        for &x in &k {
            if x != kp && x != kq {
                if hp {
                    by_p.push(x);
                }
                if hq {
                    by_q.push(x);
                }
                if hp && hq {
                    far.push(x);
                }
            }
        }
    }
    for v in [&mut by_p, &mut by_q, &mut far] {
        v.sort_unstable();
        v.dedup();
    }
    let common: Vec<[u32; 3]> = by_p.into_iter().filter(|x| by_q.binary_search(x).is_ok()).collect();
    common == far
}

/// Minimises |A x - b|Â² + w |x - mass|Â² (the normal equations, a 3Ã—3 symmetric solve).
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
        time("column of 4 sections", &prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.1, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4, square: 2.0 }));
        time("column, square 3", &prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.1, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4, square: 3.0 }));
        // (Sized so that the points are in and near the shape, where every share takes its power.)
        for power in [2.0f32, 4.0, 6.67, 40.0] {
            time(&format!("rounded box, power {power}"), &prim(Prim::Rounded { half: [1.0, 0.8, 0.6], power, disc: false }));
        }
        for power in [6.67f32, 40.0] {
            time(&format!("rounded cylinder, power {power}"), &prim(Prim::Rounded { half: [1.0, 0.8, 1.0], power, disc: true }));
        }
        time("column, square 4", &prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.1, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4, square: 4.0 }));
        time("column, square 2.5", &prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.1, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4, square: 2.5 }));
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
                if column([middle, t, 0.0], [0.0; 3], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], sections, 2.0) < 0.0 {
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
        // (What it is held to is the sampling: the reach and the ranges its columns of changing
        // proportions are asked, which are charged in the work too, are left out of it, and
        // the ranges are held to a small share of their own: under a fiftieth (1.6% when the
        // box lane's and the mesher lane's charges were brought together, 2026-11-24, which
        // took the whole work from under four fifths of every bone to just over).)
        assert!(stats.range_work > 0 && stats.range_work < stats.work / 50, "the ranges took {} of {}", stats.range_work, stats.work);
        let stats = SdfStats { work: stats.work - stats.reach_work - stats.box_work - stats.range_work, ..stats };
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
        let trunk = Prim::Column { a: [0.0, 0.8, 0.0], b: [0.0, 1.4, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4, square: 2.0 };
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
        let wedge = prim(Prim::Column { a: [0.0, 0.0, 0.0], b: [0.0, 0.2, 0.0], side: [1.0, 0.0, 0.0], sections, count: 2, square: 2.0 });
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

    /// A squarer column: as broad and as deep as the ellipse's, holding the ellipse within it
    /// and held within the box, further round the higher its squareness; its field is still
    /// never more than the distance; and the two squarenesses that take no power (3's cubes,
    /// 4's roots) say what the general way says.
    #[test]
    fn a_square_column_is_broad_deep_and_further_round() {
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[..2].copy_from_slice(&[[0.0, 0.2, 0.1], [1.0, 0.2, 0.1]]);
        let column_of = |square: f32| Prim::Column { a: [0.0, 0.0, 0.0], b: [0.0, 2.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 2, square };
        // The section's outline at mid-height, as points, for a squareness.
        let outline = |square: f32| -> Vec<[f32; 2]> {
            (0..2000)
                .map(|i| {
                    let t = i as f32 / 2000.0 * std::f32::consts::TAU;
                    let (c, s) = (crate::dmath::cos(t), crate::dmath::sin(t));
                    [0.2 * c.signum() * crate::dmath::pow(c.abs(), 2.0 / square), 0.1 * s.signum() * crate::dmath::pow(s.abs(), 2.0 / square)]
                })
                .collect()
        };
        let round = |points: &[[f32; 2]]| (0..points.len()).map(|i| len2(points[i][0] - points[(i + 1) % points.len()][0], points[i][1] - points[(i + 1) % points.len()][1])).sum::<f32>();
        let mut last = round(&outline(2.0));
        for square in [3.0f32, 4.0, 6.5] {
            let p = column_of(square);
            let d = |x: f32, z: f32| prim_distance(&p, [x, 1.0, z]);
            assert!(d(0.2, 0.0).abs() < 1e-5 && d(0.0, 0.1).abs() < 1e-5, "square {square}: as broad and as deep");
            let diagonal = std::f32::consts::FRAC_1_SQRT_2;
            assert!(d(0.2 * diagonal, 0.1 * diagonal) < 0.0 && d(0.2, 0.1) > 0.0, "square {square}: holds the ellipse, within the box");
            let points = outline(square);
            assert!(points.iter().all(|q| d(q[0], q[1]).abs() < 2e-4), "square {square}: its outline is where the field is nothing");
            let girth = round(&points);
            assert!(girth > last * 1.01, "square {square}: {girth} round against {last}");
            last = girth;
            // Never more than the distance: outside, against the nearest point of the outline.
            for probe in [[0.3f32, 0.0], [0.25, 0.12], [0.0, 0.3], [0.21, 0.11], [0.5, 0.4], [-0.3, 0.05]] {
                let truth = points.iter().map(|q| len2(q[0] - probe[0], q[1] - probe[1])).fold(f32::MAX, f32::min);
                assert!(d(probe[0], probe[1]) <= truth + 1e-3, "square {square} at {probe:?}: {} against {truth}", d(probe[0], probe[1]));
            }
            // The ways without a power agree with the general way (a hair off 3 and off 4).
            if square != 6.5 {
                let general = column_of(square + 1e-4);
                for probe in [[0.3f32, 0.02], [0.11, 0.07], [0.05, 0.2], [0.19, 0.09]] {
                    let (fast, slow) = (d(probe[0], probe[1]), prim_distance(&general, [probe[0], 1.0, probe[1]]));
                    assert!((fast - slow).abs() < 1e-4, "square {square} at {probe:?}: {fast} against {slow}");
                }
            }
        }
        // It costs what it was measured to: an ellipse's and 4's alike, 3's more, any other most.
        let cost = |square: f32| eval_cost(&prim(column_of(square)));
        assert!(cost(2.0) == cost(4.0) && cost(3.0) > cost(4.0) && cost(2.5) > cost(3.0));
    }

    /// Roundness as one number: a box or a cylinder rounded by a power is as wide, as tall and
    /// as deep as its hard shape whatever the power; at 2 it is the ellipsoid of its sizes; the
    /// higher the power the more of the hard shape it fills; and it meshes closed. (Its field
    /// against the true distance: `a_rounded_field_is_trusted_by_its_lip`.)
    #[test]
    fn a_rounded_box_runs_from_the_ellipsoid_to_the_box() {
        let half = [0.3f32, 0.2, 0.1];
        let d = |p: &Prim, q: V3| prim_distance(p, q);
        let mut last = 0.0;
        for power in [2.0f32, 4.0, 6.67, 40.0] {
            for disc in [false, true] {
                let half = if disc { [0.3, 0.2, 0.3] } else { half };
                let p = Prim::Rounded { half, power, disc };
                for axis in 0..3 {
                    let mut q = [0.0; 3];
                    q[axis] = half[axis];
                    assert!(d(&p, q).abs() < 1e-5, "power {power}: as large as its hard shape along axis {axis}");
                }
                // The corner of the hard shape is outside; the ellipsoid's point on the diagonal is on or inside.
                let third = 1.0 / 3.0f32.sqrt();
                assert!(d(&p, half) > 0.0);
                let on_ellipsoid = if disc { [half[0] * 0.5, half[1] * std::f32::consts::FRAC_1_SQRT_2, half[2] * 0.5] } else { half.map(|h| h * third) };
                assert!(d(&p, on_ellipsoid) <= 1e-5, "power {power} disc {disc}: holds the ellipsoid ({})", d(&p, on_ellipsoid));
                if !disc {
                    let (shape, _) = mesh(&prim(p), &opts(40)).unwrap();
                    assert_eq!(shape.open_edge_count(), 0, "power {power}");
                    let volume = shape.signed_volume();
                    let (ellipsoid, hard) = (4.0 / 3.0 * std::f32::consts::PI * 0.3 * 0.2 * 0.1, 8.0 * 0.3 * 0.2 * 0.1);
                    assert!(volume > ellipsoid * 0.95 && volume < hard * 1.02 && volume > last, "power {power}: {volume} between {ellipsoid} and {hard}, more than {last}");
                    last = volume;
                }
            }
        }
        assert!(last > 8.0 * 0.3 * 0.2 * 0.1 * 0.93, "at a power of 40 it is nearly the box: {last}");
        let cost = |power: f32| eval_cost(&prim(Prim::Rounded { half, power, disc: false }));
        assert!(cost(2.0) * 3 < cost(4.0) && cost(3.0) == cost(4.0));
    }

    /// What a rounded shape costs to build, said as what builds (third review: the weights
    /// had shrunk it unsaid). The nearest point is solved for at every sample outside a shape
    /// of a roundness other than 1: a cube of roundness 0.3 builds at resolution 64 on under
    /// half of the work limit and is refused at 96, by its work, with the limit in the error.
    /// At roundness 1 (a power of 2: no power to take) the same cube's work at 96 is a third
    /// of the limit.
    #[test]
    fn what_a_rounded_shape_costs_to_build() {
        let cube = |power: f32| prim(Prim::Rounded { half: [0.5; 3], power, disc: false });
        let (_, stats) = mesh(&cube(6.67), &opts(64)).unwrap();
        assert!(stats.work > MAX_WORK / 3 && stats.work < MAX_WORK / 2, "roundness 0.3 at 64: {} of {MAX_WORK}", stats.work);
        let refused = mesh(&cube(6.67), &opts(96)).unwrap_err();
        assert!(refused.limit && refused.message.contains("work") && refused.max == Some(MAX_WORK as f64), "{refused:?}");
        let (_, stats) = mesh(&cube(2.0), &opts(96)).unwrap();
        assert!(stats.work < MAX_WORK * 2 / 5, "roundness 1 at 96: {} of {MAX_WORK}", stats.work);
        // One weight at every thinness, thin along any of the three axes, box or cylinder:
        // with the first steps started where the eighth review found them failing, a thin
        // shape's samples take what a cube's do, and what any sample takes past its weight is
        // charged as it is taken (`a_rounded_shape_is_charged_for_what_it_takes`). (Two
        // weights graded by thinness stood for those steps before.)
        let cost = |half: V3, power: f32, disc: bool| eval_cost(&prim(Prim::Rounded { half, power, disc }));
        let thick = cost([0.5; 3], 4.0, false);
        assert_eq!(thick - 48, ROUNDED_COST[1]);
        for half in [[1e-4f32, 0.5, 0.5], [0.5, 1e-4, 0.5], [0.5, 0.5, 1e-4], [0.5, 0.05, 0.3], [1e-8, 1e-8, 0.5]] {
            assert_eq!(cost(half, 6.67, false), thick, "{half:?}");
        }
        assert_eq!((cost([0.5, 0.5, 0.0], 4.0, true), cost([0.5, 0.0001, 0.0], 40.0, true), cost([0.0001, 0.5, 0.0], 2.5, true)), (thick, thick, thick));
        assert_eq!(cost([0.9, 1e-4, 0.9], 2.0, false) - 48, ROUNDED_COST[0], "at a power of 2 nothing is solved for");
        // The weight pays for the powers it is said to allow, at the rate the rest is charged.
        const { assert!(ROUNDED_ALLOW * ROUNDED_POWER <= ROUNDED_COST[1] && ROUNDED_ALLOW > 0) };
    }

    /// A sample too far for numbers has the bound for its distance, and stops nothing (a
    /// sixth review: a coordinate past 1.8e19, whose square is past what an `f32` holds, made
    /// a limit of the solve nothing and the solve panic; a rounded box nested in seven scales
    /// of a thousandth, which a recipe may write, stopped its build that way).
    #[test]
    fn a_sample_too_far_for_numbers_stops_nothing() {
        for power in [2.0f32, 2.5, 4.0, 40.0] {
            for disc in [false, true] {
                for far in [2.0e19f32, 1.0e20, 1.0e30, f32::MAX, f32::INFINITY] {
                    for q in [[far, 0.0, 0.0], [0.0, -far, 0.0], [far, far, far]] {
                        let (f, steps, _) = rounded_counted(q, [0.5; 3], power, disc);
                        // (Nothing solved for: the hard shape's distance alone, its square
                        // roots a quarter of a power each.)
                        assert!(f > 1.0e18 && steps <= 1, "power {power}, disc {disc}, at {q:?}: {f} in {steps} steps");
                    }
                }
                // (Not a number in: nothing is solved for, and nothing stops. What comes out is
                // not held: no sample of a grid is not a number.)
                assert!(rounded_counted([f32::NAN, 0.0, 0.0], [0.5; 3], power, disc).1 <= 1, "power {power}");
                // Far, and still a number: the distance, solved.
                let (f, _, _) = rounded_counted([1.0e12, 0.0, 0.0], [0.5; 3], power, disc);
                assert!((f - 1.0e12).abs() < 1.0e6, "power {power} at a billion kilometres: {f}");
            }
        }
        // The recipe: a rounded box under seven scales of a thousandth, beside a plain one.
        let mut inner = prim(Prim::Rounded { half: [0.5; 3], power: 4.0, disc: false });
        let union = |kids: Vec<SdfNode>, m: Modifiers| SdfNode { kind: Kind::Group { op: Combine::Union, kids, smooth: 0.0 }, m };
        for _ in 0..7 {
            inner = union(vec![inner], Modifiers { scale: 0.001, ..Modifiers::default() });
        }
        let plain = SdfNode { kind: Kind::Prim(Prim::Box { half: [0.5; 3], round: 0.0 }), m: Modifiers { at: [0.0, 0.5, 0.0], ..Modifiers::default() } };
        let tree = union(vec![plain, inner], Modifiers::default());
        let (mesh, _) = mesh(&tree, &opts(16)).expect("it builds: the speck is far under a cell");
        assert!(!mesh.positions.is_empty());
    }

    /// A rounded shape's surface in `f64`, for the tests below: how far out a point is by the
    /// shape's own measure, and the point of the surface a direction from the middle meets.
    #[allow(clippy::disallowed_methods, reason = "a test's own reference, in f64, apart from the code it checks")]
    fn rounded_measure(p: [f64; 3], half: V3, power: f32, disc: bool) -> f64 {
        let h = half.map(f64::from);
        let u = if disc { [(p[0] * p[0] + p[2] * p[2]).sqrt() / h[0], p[1].abs() / h[1], 0.0] } else { [p[0].abs() / h[0], p[1].abs() / h[1], p[2].abs() / h[2]] };
        let m = u[0].max(u[1]).max(u[2]);
        if m == 0.0 {
            return 0.0;
        }
        m * u.iter().map(|x| (x / m).powf(f64::from(power))).sum::<f64>().powf(1.0 / f64::from(power))
    }

    /// The nearest point of a rounded shape's surface to `probe`, and how far it is: the best
    /// of a grid of directions over a cube's six faces (each met with the surface), then
    /// refined about it, so that flat sides and tight corners are covered alike.
    fn rounded_nearest(probe: [f64; 3], half: V3, power: f32, disc: bool) -> f64 {
        let h = half.map(f64::from);
        let on = |face: usize, a: f64, b: f64| -> [f64; 3] {
            let (axis, sign) = (face / 2, if face.is_multiple_of(2) { 1.0 } else { -1.0 });
            let mut d = [0.0; 3];
            d[axis] = sign * h[axis];
            d[(axis + 1) % 3] = a * h[(axis + 1) % 3];
            d[(axis + 2) % 3] = b * h[(axis + 2) % 3];
            let k = rounded_measure(d, half, power, disc);
            d.map(|x| x / k)
        };
        let far = |q: [f64; 3]| {
            let d = [q[0] - probe[0], q[1] - probe[1], q[2] - probe[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        // (Every dip of every face's grid is refined on its own: the nearest point may lie
        // just past the edge of the face whose grid came nearest, and on a thin shape a face
        // has more than one dip: straight out to a side, and up over the edge.)
        const N: usize = 24;
        let at = |i: usize| -1.0 + 2.0 * i as f64 / N as f64;
        let mut nearest = f64::MAX;
        for face in 0..6 {
            let grid: Vec<f64> = (0..=N).flat_map(|i| (0..=N).map(move |j| (i, j))).map(|(i, j)| far(on(face, at(i), at(j)))).collect();
            let g = |i: usize, j: usize| grid[i * (N + 1) + j];
            for i in 0..=N {
                for j in 0..=N {
                    let dip = (i.saturating_sub(1)..=(i + 1).min(N)).all(|a| (j.saturating_sub(1)..=(j + 1).min(N)).all(|b| g(i, j) <= g(a, b)));
                    if !dip {
                        continue;
                    }
                    let mut best = (g(i, j), at(i), at(j));
                    let mut step = 2.0 / N as f64;
                    while step > 1e-7 {
                        let (_, a0, b0) = best;
                        for (da, db) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
                            let (a, b) = ((a0 + da * step).clamp(-1.0, 1.0), (b0 + db * step).clamp(-1.0, 1.0));
                            let d = far(on(face, a, b));
                            if d < best.0 {
                                best = (d, a, b);
                            }
                        }
                        if (best.1, best.2) == (a0, b0) {
                            step *= 0.5;
                        }
                    }
                    nearest = nearest.min(best.0);
                }
            }
        }
        nearest
    }

    /// A rounded box's or cylinder's field against the true distance (review of roundness: the
    /// first field was in the shape's own units and infinite far away). Outside it is never
    /// more than the distance and never less than it by more than the shape's slack; inside it
    /// is never less than the depth and never more than it by more than the slack (so between
    /// two points it never changes by more than their distance apart and the slack, which the
    /// mesher and pruning lean on). Just off the surface it is the distance within a few parts
    /// in ten, and far away it is finite and nearly the distance.
    #[test]
    fn a_rounded_field_is_trusted_by_its_slack() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let shapes: [(V3, bool); 6] = [
            ([0.3, 0.2, 0.1], false),
            ([0.5, 0.025, 0.3], false),
            ([1.0, 0.01, 1.0], false),
            ([0.02, 0.6, 0.05], false),
            ([0.3, 0.2, 0.3], true),
            ([0.5, 0.01, 0.5], true),
        ];
        let (mut worst, mut off) = (0.0f64, 0.0f64);
        for (half, disc) in shapes {
            for power in [2.0f32, 2.5, 4.0, 6.67, 20.0, 40.0] {
                let p = Prim::Rounded { half, power, disc };
                assert_eq!(prim_lip(&p), 1.0);
                let slack = f64::from(prim_slack(&p));
                let field = |q: [f64; 3]| f64::from(prim_distance(&p, q.map(|x| x as f32)));
                let (least, most) = (f64::from(half[0].min(half[1]).min(half[2])), f64::from(half[0].max(half[1]).max(half[2])));
                let leave = 2e-5 * (1.0 + most);
                let what = format!("half {half:?} power {power} disc {disc}");
                // Around and inside the shape.
                for _ in 0..80 {
                    let q = [0, 1, 2].map(|i| (next() * 3.2 - 1.6) * f64::from(half[i]));
                    let (f, truth, k) = (field(q), rounded_nearest(q, half, power, disc), rounded_measure(q, half, power, disc));
                    if k > 1.0 {
                        // Outside the field is the distance (third review: three touching
                        // planes were still up to 4.7 times out at a thin board's corners).
                        assert!(f > -leave && f <= truth + leave && f >= truth * (1.0 - EXACT) - leave, "{what} at {q:?}: {f} outside, where the surface is {truth} away");
                        off = off.max((truth - f) / truth.max(least));
                    } else {
                        assert!(f < leave && -f >= truth - leave && -f <= truth + slack + leave, "{what} at {q:?}: {f} inside, where the surface is {truth} away (slack {slack})");
                        worst = worst.max((-f - truth) / slack);
                    }
                }
                // Just off the surface, either way along a line from the middle: the field is
                // the distance there within a few parts in ten (a metre-valued modifier acts
                // in metres), on the right side of it.
                for _ in 0..24 {
                    let d = [next() - 0.5, next() - 0.5, next() - 0.5].map(|x| x * 2.0);
                    let d = [0, 1, 2].map(|i| d[i] * f64::from(half[i]));
                    let k = rounded_measure(d, half, power, disc);
                    if k < 1e-6 {
                        continue;
                    }
                    for out in [1.0 + 0.05 * least / most, 1.0 - 0.05 * least / most] {
                        let q = d.map(|x| x / k * out);
                        let (f, truth) = (field(q), rounded_nearest(q, half, power, disc));
                        if out > 1.0 {
                            assert!(f >= truth * (1.0 - EXACT) - leave && f <= truth + leave, "{what} at {q:?}: {f} just outside, the surface {truth} away");
                        } else {
                            assert!(-f >= truth - leave && -f <= 1.6 * truth + 1e-6, "{what} at {q:?}: {f} just inside, the surface {truth} away");
                        }
                    }
                }
                // Far away, along the axes and between them: finite, and nearly the distance.
                for reach in [9.5f64, 60.0, 5000.0] {
                    for dir in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 1.0], [1.0, 0.02, 0.0], [0.03, 1.0, 0.5], [-1.0, 0.4, -0.7]] {
                        let q = [0, 1, 2].map(|i| dir[i] * reach * most);
                        let (f, truth) = (field(q), rounded_nearest(q, half, power, disc));
                        assert!(f.is_finite() && f <= truth * (1.0 + 1e-5) + 1e-4 && f >= truth * (1.0 - EXACT) - 1e-4, "{what} at {q:?}: {f}, the surface {truth} away");
                    }
                }
            }
        }
        // The slack (needed inside only now) is not far more than is needed: the worst seen
        // is over a tenth of it.
        assert!(worst > 0.1 && worst <= 1.0, "the worst seen is {worst} of the slack");
        // (Outside, the worst seen short of the distance is 7 parts in 100,000 of it.)
        assert!(off <= 2e-4, "outside, the field is short of the distance by up to {off:.2e} of it");
    }

    /// How far short of the distance the field outside a rounded shape may be, as a share of
    /// it: the test's own reference is good to a few parts in ten thousand.
    const EXACT: f64 = 5e-4;

    /// What the fourth review of roundness found, each as it should be.
    ///
    /// Far from a rounded shape a sample costs what its work weight allows (the weight was
    /// set near the surface; solved one way only, a far sample took as many steps as the
    /// dearest near one, and a box cut by a rounded plate ran 3.8 s on a little over half of
    /// the work limit): round thin and thick shapes alike, the samples of a 2 m box take
    /// under 22 powers each on average, at every roundness, and the sure way is next to never
    /// taken. (Under twenty until the first steps were held to arrive to a ten-millionth of
    /// `mu`, after the ninth review: 1.4 to 1.7 powers a sample more, 20.4 the most, for a foil
    /// at a roundness of 0.8.)
    ///
    /// And it is the distance there too, with no step in it: at the points the review named
    /// (a first guess above the answer threw a share out to where its power overflowed, the
    /// solve stopped as if done and fell back on the hard shape's distance: 1.1% short at
    /// power 40, and a step of 12.9 mm between samples 0.28 mm apart along a plate's diagonal).
    #[test]
    fn a_rounded_shape_is_solved_as_well_far_away() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f32 / (1u64 << 53) as f32
        };
        // (A plate, a needle, a cube and a board, at every kind of roundness. A fifth review
        // found the needle, which the plate alone had hidden: 71 steps a sample at the lowest
        // roundness, 63% of them over forty. A sixth found a foil and a hair a
        // nine-thousandth as thick, which took more steps, with up to one sample in seven
        // left to the sure way.)
        let shapes = [("a plate", [0.2f32, 0.004, 0.2]), ("a needle", [0.002, 0.4, 0.002]), ("a cube", [0.3, 0.3, 0.3]), ("a board", [0.5, 0.05, 0.3]), ("a foil", [0.9, 1e-4, 0.9]), ("a hair", [1e-4, 0.9, 1e-4])];
        for (name, half) in shapes {
            for power in [2.5f32, 3.0, 4.0, 6.67, 20.0, 40.0] {
                let (mut near, mut far, mut sure, mut most) = ((0u64, 0u64), (0u64, 0u64), 0u64, 0u32);
                for _ in 0..3000 {
                    // Just off the surface: a point of the box, pushed out a little.
                    let on = [0, 1, 2].map(|i| (next() * 2.0 - 1.0) * half[i]);
                    let out = 1.0 + next() * 0.2;
                    let (f, powers, slow) = rounded_counted([on[0] * out, on[1] * out + 0.002 + next() * 0.01, on[2] * out], half, power, false);
                    if f > 0.0 {
                        near = (near.0 + u64::from(powers), near.1 + 1);
                        sure += u64::from(slow);
                        most = most.max(powers);
                    }
                    // Anywhere in a 2 m box round it.
                    let (f, powers, slow) = rounded_counted([next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0], half, power, false);
                    if f > 0.0 {
                        far = (far.0 + u64::from(powers), far.1 + 1);
                        sure += u64::from(slow);
                        most = most.max(powers);
                    }
                }
                let (near_mean, far_mean) = (near.0 as f64 / near.1.max(1) as f64, far.0 as f64 / far.1 as f64);
                let what = format!("{name}, power {power}: {far_mean:.1} powers a sample in a 2 m box, {near_mean:.1} near the surface, {sure} of {} to the sure way, the most {most}", far.1 + near.1);
                assert!(far_mean < 22.0 && near_mean < 30.0 && sure * 200 <= far.1 + near.1 && most <= ROUNDED_MOST, "{what}");
            }
        }

        let plate = [1.0f32, 0.005, 1.0];
        for (power, q) in [(40.0f32, [-1.24f32, 0.80, -1.20]), (20.0, [2.58, 4.06, 2.56]), (40.0, [1.654, 1.654, 1.654]), (2.0, [150.0, 90.0, -120.0])] {
            let f = f64::from(rounded(q, plate, power, false));
            let truth = rounded_nearest(q.map(f64::from), plate, power, false);
            assert!((f - truth).abs() <= truth * EXACT, "power {power} at {q:?}: {f}, the surface {truth} away");
        }
        // Along the plate's diagonal, from just off its corner to 60 m away: samples a quarter
        // of a millimetre apart near it, then a few centimetres apart.
        for power in [2.0f32, 2.5, 20.0, 40.0] {
            let p = Prim::Rounded { half: plate, power, disc: false };
            let dir = math3::normalize([1.0, 1.0, 1.0]);
            for (from, to, steps) in [(1.2f32, 4.2f32, 12_000u32), (4.0, 104.0, 4_000)] {
                let step = (to - from) / steps as f32;
                let (mut last, mut over) = (f32::NAN, 0.0f32);
                for i in 0..=steps {
                    let t = from + step * i as f32;
                    let f = prim_distance(&p, [dir[0] * t, dir[1] * t, dir[2] * t]);
                    if last.is_finite() {
                        over = over.max((f - last).abs() - step);
                    }
                    last = f;
                }
                // (A ten-thousandth of how far out the scan is, where the step was 13 mm.)
                assert!(over <= 1e-4 * to, "power {power}, {from} to {to} m along the diagonal: between two samples a step apart the field changes by {over} m more than the step");
            }
        }
    }


    /// What a build of rounded shapes takes for its work, by count (an eighth review: a large
    /// thin rounded cutter set past the corner of what it cut took four times the powers a
    /// unit of work that the calibrated case takes, 98% of its samples left to the sure way;
    /// other roundnesses than the levelled ones took up to 1.45 times). Every power is counted
    /// in real builds and held against the work each build is charged: no case takes more
    /// than five quarters of what the calibrated case takes (a cube of roundness 0.3 cut from
    /// a 2 m box). The cases: the review's, thin to a hundred-millionth along each axis in
    /// turn, a box and a disc, over the middle, beside an edge and past a corner, sizes to the
    /// parser's, under stacked scales, a stretch, a twist, a bend, a repeat, a displacement
    /// and a smooth blend, alone and as cutters, and each thinness at every roundness. And the
    /// first steps arrive: no case takes more than two and a half times the powers a sample
    /// that the calibrated one takes (the review's case took 125, where the calibrated one
    /// took 22; a 2 m sheet a millionth as thick cut through the middle, the dearest now, 37).
    #[test]
    fn a_rounded_shape_is_charged_for_what_it_takes() {
        let rounded = |half: V3, power: f32, disc: bool| prim(Prim::Rounded { half, power, disc });
        let with = |mut n: SdfNode, f: &dyn Fn(&mut Modifiers)| {
            f(&mut n.m);
            n
        };
        let group = |op: Combine, kids: Vec<SdfNode>, smooth: f32| SdfNode { kind: Kind::Group { op, kids, smooth }, m: Modifiers::default() };
        let cut = |cutter: SdfNode| group(Combine::Difference, vec![prim(Prim::Box { half: [1.0; 3], round: 0.0 }), cutter], 0.0);
        let at = |a: V3| move |m: &mut Modifiers| m.at = a;
        // Powers per unit of work, and powers a rounded sample.
        let take = |tree: &SdfNode| -> (f64, f64) {
            let (_, s) = mesh(tree, &MeshOptions { max_work: u64::MAX, max_triangles: 2_000_000, ..opts(32) }).unwrap();
            assert!(s.rounded_samples > 0, "{s:?}");
            // (Every power the build took was charged: none is left over for the next.)
            assert_eq!(rounded_taken(), [0, 0, 0], "{s:?}");
            (s.rounded_powers as f64 / s.work as f64, s.rounded_powers as f64 / s.rounded_samples as f64)
        };
        let calibrated = take(&cut(rounded([0.3; 3], 6.67, false)));
        let mut cases: Vec<(String, SdfNode)> = vec![
            ("the review's sheet past the corner".into(), cut(with(rounded([50.0, 5e-5, 50.0], 10.0, false), &at([-51.1, 0.0, -51.1])))),
            ("a 10 m sheet past the corner".into(), cut(with(rounded([5.0, 5e-4, 5.0], 8.0, false), &at([-6.1, 0.0, -6.1])))),
            ("a 10 m board past the corner".into(), cut(with(rounded([5.0, 0.25, 5.0], 6.67, false), &at([-6.1, 0.0, -6.1])))),
            ("a plank past the corner".into(), cut(with(rounded([2.5, 0.125, 2.5], 2.0202, false), &at([-3.6, 0.0, -3.6])))),
            ("a 100 m disc past the corner".into(), cut(with(rounded([50.0, 5e-5, 0.0], 10.0, true), &at([-36.8, 0.0, -36.8])))),
            ("a sheet of 10 km past the corner".into(), cut(with(rounded([5000.0, 5e-5, 5000.0], 13.0, false), &at([-5001.1, 0.0, -5001.1])))),
            ("a hair of a nine-thousandth".into(), cut(rounded([5.55e-5, 0.5, 5.55e-5], 40.0, false))),
            ("a foil of a nine-thousandth".into(), cut(rounded([0.5, 5.55e-5, 0.5], 8.0, false))),
            ("a sheet a millionth as thick through the middle".into(), cut(rounded([1.0, 1e-6, 1.0], 3.0, false))),
            ("a hundred-millionth beside an edge".into(), cut(with(rounded([1.0, 1e-8, 1.0], 4.0, false), &at([-1.5, 0.3, 0.0])))),
            ("a cube far off".into(), cut(with(rounded([0.3; 3], 6.67, false), &at([3.5, 0.0, 0.0])))),
            ("a stretched cube".into(), cut(with(rounded([0.3; 3], 6.67, false), &|m| m.stretch = [20.0, 0.05, 1.0]))),
            ("a cube alone".into(), rounded([0.5; 3], 6.67, false)),
            ("a board alone".into(), rounded([0.5, 0.05, 0.3], 3.0, false)),
            ("a twisted board".into(), with(rounded([0.5, 0.05, 0.3], 6.67, false), &|m| m.twist = 90.0)),
            ("a bent board".into(), with(rounded([0.5, 0.05, 0.3], 6.67, false), &|m| m.bend = 60.0)),
            ("repeated boards".into(), with(rounded([0.1, 0.02, 0.3], 6.67, false), &|m| m.repeat = Some(([4, 1, 1], [0.3, 0.0, 0.0])))),
            ("a displaced board".into(), with(rounded([0.5, 0.05, 0.3], 6.67, false), &|m| m.displace = Some((0.02, 0.2, 2)))),
            ("a smooth blend of three".into(), {
                let kids = vec![rounded([0.5, 0.05, 0.3], 6.67, false), with(rounded([0.05, 0.4, 0.05], 3.0, false), &at([0.3, 0.2, 0.0])), with(rounded([0.2, 0.2, 0.0], 20.0, true), &at([-0.3, 0.1, 0.1]))];
                group(Combine::Union, kids, 0.1)
            }),
        ];
        // Seven scales of a thousandth over a cube a thousand billion billion metres across.
        let mut nested = rounded([0.3e21; 3], 6.67, false);
        for _ in 0..7 {
            nested = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![nested], smooth: 0.0 }, m: Modifiers { scale: 0.001, ..Modifiers::default() } };
        }
        cases.push(("a cube under seven scales".into(), cut(nested)));
        // Thin along each axis in turn, and each thinness at every roundness, past a corner.
        for (i, axis) in ["x", "y", "z"].into_iter().enumerate() {
            let mut half = [0.5f32; 3];
            half[i] = 1e-4;
            cases.push((format!("a plank thin along {axis}"), cut(rounded(half, 6.67, false))));
        }
        for thin in [1.0f32, 1e-3, 1e-6] {
            for power in [2.0202f32, 2.5, 3.0, 4.0, 6.67, 10.0, 20.0, 40.0] {
                cases.push((format!("1:{} at power {power} past the corner", 1.0 / thin), cut(with(rounded([1.0, thin, 1.0], power, false), &at([-2.1, 0.0, -2.1])))));
            }
        }
        for (name, tree) in &cases {
            let (rate, each) = take(tree);
            assert!(rate <= 1.25 * calibrated.0, "{name}: {:.2} powers a thousand units, the calibrated case {:.2}", rate * 1e3, calibrated.0 * 1e3);
            assert!(each <= 2.5 * calibrated.1, "{name}: {each:.1} powers a sample, the calibrated case {:.1}", calibrated.1);
        }
    }

    /// Every evaluation a build makes is charged (ninth review: finding what is near a block
    /// evaluated a subtree again at each level of plain unions above it, and was charged once,
    /// so a rounded cube under 120 nested unions took 6.2 times the powers a unit of work that
    /// the calibrated case takes). Counted, not timed: the weights of every primitive evaluated
    /// are held against the work charged, and the powers and rounded samples a unit of work
    /// against the calibrated case, for a rounded cube under 10, 40 and 120 nested unions and
    /// under 60 unions each with a speck beside it.
    #[test]
    fn a_build_is_charged_for_every_evaluation() {
        let cube = || prim(Prim::Rounded { half: [0.3; 3], power: 6.67, disc: false });
        let union = |kids: Vec<SdfNode>| SdfNode { kind: Kind::Group { op: Combine::Union, kids, smooth: 0.0 }, m: Modifiers::default() };
        let cut = |cutter: SdfNode| SdfNode { kind: Kind::Group { op: Combine::Difference, kids: vec![prim(Prim::Box { half: [1.0; 3], round: 0.0 }), cutter], smooth: 0.0 }, m: Modifiers::default() };
        let chain = |depth: usize| (0..depth).fold(cube(), |t, _| union(vec![t]));
        let specks = (0..60).fold(cube(), |t, i| union(vec![t, SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.01 }), m: Modifiers { at: [0.0, 0.0, 0.001 * i as f32], ..Modifiers::default() } }]));
        // Weights evaluated against the work charged; powers and rounded samples a unit.
        let take = |tree: &SdfNode| -> (f64, f64) {
            PRIM_WEIGHED.with(|w| w.set(0));
            let (_, s) = mesh(tree, &MeshOptions { max_work: u64::MAX, ..opts(32) }).unwrap();
            let weighed = PRIM_WEIGHED.with(|w| w.get());
            assert!(weighed <= s.work, "primitives weighing {weighed} were evaluated for {} work", s.work);
            (s.rounded_powers as f64 / s.work as f64, s.rounded_samples as f64 / s.work as f64)
        };
        let calibrated = take(&cut(cube()));
        // (And what the ranges of loose children evaluate, pruned, is charged: review of the
        // mesher's skip, 2026-11-24, which found nested unions each holding a flat ellipsoid
        // and a few balls charged a quarter to a third less than they evaluated.)
        let flat = |i: usize| SdfNode { kind: Kind::Prim(Prim::Ellipsoid { radii: [0.4, 0.02 + 0.001 * i as f32, 0.3] }), m: Modifiers { rotate: [10.0 * i as f32, 25.0, 5.0 * i as f32], ..Modifiers::default() } };
        let ball = |i: usize, k: usize| SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.03 + 0.01 * k as f32 }), m: Modifiers { at: [0.1 * k as f32 - 0.2, 0.05 * i as f32 - 0.3, 0.07 * k as f32], ..Modifiers::default() } };
        let nested = (0..16).fold(flat(99), |t, i| union(std::iter::once(t).chain(std::iter::once(flat(i))).chain((0..3 + i % 3).map(|k| ball(i, k))).collect()));
        let wide = union(std::iter::once(flat(0)).chain((0..80).map(|k| ball(k % 12, k % 7 + k / 12))).collect());
        for (name, tree) in [("under 10 unions", cut(chain(10))), ("under 40 unions", cut(chain(40))), ("under 120 unions", cut(chain(120))), ("under 60 unions with a speck each", cut(specks)), ("sixteen nested unions of a flat ellipsoid and balls", nested), ("a flat ellipsoid among 80 balls", wide)] {
            let (powers, samples) = take(&tree);
            assert!(powers <= calibrated.0 * 1.05 && powers * ROUNDED_POWER as f64 <= 1.0, "{name}: {:.2} powers a thousand units, the calibrated case {:.2}", powers * 1e3, calibrated.0 * 1e3);
            assert!(samples <= calibrated.1 * 1.05, "{name}: {:.3} rounded samples a thousand units, the calibrated case {:.3}", samples * 1e3, calibrated.1 * 1e3);
        }
    }

    /// Asking which primitive made each triangle is charged for the powers its rounded shapes
    /// take past their weight (ninth review: it ran at up to 5.9 times the calibrated powers
    /// a unit), inside its own limit; and whatever this thread evaluated before a build is not
    /// the build's, what was paid is kept, and the colours are charged as they go.
    #[test]
    fn a_rounded_shape_is_charged_wherever_it_is_asked() {
        let cut = |cutter: SdfNode| SdfNode { kind: Kind::Group { op: Combine::Difference, kids: vec![prim(Prim::Box { half: [1.0; 3], round: 0.0 }), cutter], smooth: 0.0 }, m: Modifiers::default() };
        let mut any_over = false;
        for half in [[0.3f32; 3], [1.0, 1e-6, 1.0]] {
            for power in [3.0f32, 6.67, 40.0] {
                let tree = cut(prim(Prim::Rounded { half, power, disc: false }));
                let (shape, _) = mesh(&tree, &opts(32)).unwrap();
                assert!(shape.tris.len() <= MAX_OWNER_ASKED, "every triangle is asked");
                let asked = primitive_of_triangles(&tree, 1, &shape, MAX_OWNER_ASKED, MAX_OWNER_BUILD_WORK);
                assert_eq!(rounded_taken(), [0, 0, 0], "nothing of the questions is left in the tally");
                // The same questions asked again outside it: their weights, and the powers they
                // took past what those pay for.
                let prepared = prepare(&tree, 1, &mut 0);
                let counts = Counted::of(&prepared);
                let mut weights = 0u64;
                for t in shape.tris.iter().take(asked.asked) {
                    let mid = math3::scale(math3::add(math3::add(shape.positions[t[0] as usize], shape.positions[t[1] as usize]), shape.positions[t[2] as usize]), 1.0 / 3.0);
                    counts.owner(mid, 0, &mut weights);
                }
                let [powers, _, allowed] = rounded_taken();
                let over = powers.saturating_sub(allowed);
                any_over |= over > 0;
                assert!(asked.work >= weights + over * ROUNDED_POWER && asked.work <= weights + powers * ROUNDED_POWER, "half {half:?} power {power}: {powers} powers ({allowed} paid for by {weights} of weights) charged {}", asked.work);
            }
        }
        assert!(any_over, "some case takes more powers than its weights pay for");
        // Whatever the thread evaluated before a build is not the build's.
        let tree = cut(prim(Prim::Rounded { half: [1.0, 1e-6, 1.0], power: 3.0, disc: false }));
        let alone = mesh(&tree, &opts(24)).unwrap().1;
        for i in 0..1000 {
            rounded([1.0 + 0.001 * i as f32, 0.2, 1.3], [1.0, 1e-6, 1.0], 3.0, false);
        }
        assert_eq!(mesh(&tree, &opts(24)).unwrap().1, alone, "a build after other evaluations on its thread");
        // What was paid is kept: powers past the allowance, then samples that take none (the
        // allowance grows past what was taken), then powers again: each power past it once.
        let mut bill = RoundedBill::start();
        let mut charged = 0;
        for (powers, samples) in [(100u64, 1u64), (0, 10), (100, 0), (300, 0)] {
            ROUNDED_TAKEN.with(|t| t.set([powers, samples, samples * ROUNDED_ALLOW]));
            charged += bill.due();
        }
        assert_eq!(charged, (500 - 11 * ROUNDED_ALLOW) * ROUNDED_POWER);
        // The colours are charged as they go: no more than 64 vertices' powers run unpaid.
        let mut coloured = prim(Prim::Rounded { half: [0.5, 0.05, 0.4], power: 6.67, disc: false });
        coloured.m.color = Some([0.8, 0.2, 0.2, 1.0]);
        let (shape, stats) = mesh(&coloured, &opts(48)).unwrap();
        assert!(shape.positions.len() > 640 && stats.rounded_most_between <= 64 * u64::from(ROUNDED_MOST) && stats.rounded_most_between * 10 <= stats.rounded_powers, "{} vertices, {stats:?}", shape.positions.len());
    }

    /// Where the solve fell back most (an eighth review counted its cost there and did not
    /// look at its answer: past the corner of a large thin sheet, and near it), the field is
    /// the distance, against one found by search in double precision.
    #[test]
    fn a_rounded_shape_is_the_distance_past_a_thin_corner() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for (half, power) in [([50.0f32, 5e-5, 50.0], 10.0f32), ([5.0, 5e-4, 5.0], 8.0), ([1.0, 1.1e-4, 1.0], 6.67), ([1.0, 1e-3, 1.0], 2.5), ([1.0, 1e-3, 1.0], 40.0)] {
            let size = f64::from(half[0]);
            for _ in 0..40 {
                // Past the corner, a fiftieth to twice the sheet's size out, and up to as high.
                let out = size * (0.02 + 2.0 * next() * next());
                let q = [size + out * next(), out * (next() - 0.3), size + out * next()];
                let (f, _, _) = rounded_counted(q.map(|x| x as f32), half, power, false);
                let truth = rounded_nearest(q, half, power, false);
                assert!((f64::from(f) - truth).abs() <= truth * EXACT + 1e-5 * size, "half {half:?} power {power} at {q:?}: {f}, the surface {truth} away");
            }
        }
    }
    /// Outside a rounded shape the field changes no faster than distance, with no step in it:
    /// along fine lines out from thin shapes, through twice and five times the least half size
    /// (where steps toward the nearest point were switched on and off, the field jumped by up
    /// to 105 mm on a plate and 63 mm on a board: third review).
    #[test]
    fn a_rounded_field_has_no_step_outside() {
        for (half, power) in [([1.0f32, 0.005, 1.0], 4.0f32), ([0.5, 0.025, 0.3], 2.0), ([0.5, 0.025, 0.3], 2.5), ([0.5, 0.025, 0.3], 10.0), ([0.02, 0.6, 0.05], 6.67)] {
            let p = Prim::Rounded { half, power, disc: false };
            let least = half[0].min(half[1]).min(half[2]);
            for dir in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.7, 0.1, 0.7], [0.6, 0.5, 0.6], [1.0, 0.3, 0.0], [0.2, 1.0, 0.9], [0.9, 0.02, 0.4]] {
                let dir = math3::normalize(dir);
                // From a point of the surface's box outward, 6 least half sizes, in 600 steps.
                let most = half[0].max(half[1]).max(half[2]);
                let from = [0, 1, 2].map(|i| dir[i] * half[i] * 1.05);
                let step = 6.0 * least / 600.0;
                let mut last: Option<f32> = None;
                let mut over = 0.0f32;
                for i in 0..600 {
                    let q = [0, 1, 2].map(|k| from[k] + dir[k] * step * i as f32);
                    let f = prim_distance(&p, q);
                    if let (Some(before), true) = (last, f > 0.0) {
                        if before > 0.0 {
                            over = over.max((f - before).abs() - step);
                        }
                    }
                    last = Some(f);
                }
                // (No more than the step itself, give or take what the answer's own tolerance
                // does: a ten-thousandth of the shape's size, where the steps were tenths.)
                assert!(over <= 1e-4 * most, "half {half:?} power {power} along {dir:?}: between two samples a step apart the field changes by {over} m more than the step");
            }
        }
    }

    /// The support of a rounded shape of half sizes `a` along a unit `n` (its coordinates not
    /// negative): how far it reaches that way, and the point that reaches it. (From the ninth
    /// review's probe.)
    #[allow(clippy::disallowed_methods, reason = "a test's own reference, in f64, apart from the code it checks")]
    fn support_of(a: &[f64], n: &[f64], power: f64) -> (f64, Vec<f64>) {
        let q = power / (power - 1.0);
        let t: Vec<f64> = a.iter().zip(n).map(|(a, n)| a * n).collect();
        let m = t.iter().copied().fold(0.0, f64::max);
        if m <= 0.0 {
            return (0.0, vec![0.0; a.len()]);
        }
        let s: f64 = t.iter().map(|x| (x / m).powf(q)).sum();
        let h = m * s.powf(1.0 / q);
        (h, a.iter().zip(&t).map(|(a, t)| a * (t / h).powf(q - 1.0)).collect())
    }

    /// The most of `f` over 0 to 1: a grid, then golden sections about its best (the ninth
    /// review's probe).
    #[allow(clippy::disallowed_methods, reason = "a test's own reference, in f64, apart from the code it checks")]
    fn maximise(f: &dyn Fn(f64) -> f64) -> (f64, f64) {
        let mut grid = vec![0.0];
        grid.extend((2..=14).rev().map(|e| 10f64.powi(-e)));
        grid.extend((1..24).map(|k| k as f64 / 24.0));
        grid.extend((2..=14).map(|e| 1.0 - 10f64.powi(-e)));
        grid.push(1.0);
        let values: Vec<f64> = grid.iter().map(|&t| f(t)).collect();
        let best = (0..grid.len()).fold(0, |b, i| if values[i] > values[b] { i } else { b });
        let (mut a, mut b) = (grid[best.saturating_sub(1)], grid[(best + 1).min(grid.len() - 1)]);
        let g = 0.5 * (5f64.sqrt() - 1.0);
        let (mut c, mut d) = (b - g * (b - a), a + g * (b - a));
        let (mut fc, mut fd) = (f(c), f(d));
        for _ in 0..75 {
            if fc >= fd {
                (b, d, fd) = (d, c, fc);
                c = b - g * (b - a);
                fc = f(c);
            } else {
                (a, c, fc) = (c, d, fd);
                d = a + g * (b - a);
                fd = f(d);
            }
        }
        let mut found = if fc >= fd { (c, fc) } else { (d, fd) };
        for t in [a, b] {
            if f(t) > found.1 {
                found = (t, f(t));
            }
        }
        if found.1 >= values[best] { found } else { (grid[best], values[best]) }
    }

    /// A certified bracket on the distance from `q` to a rounded shape, or `None` when `q` is
    /// inside it. Its bounds hold whatever found them: any unit `n` gives a lower one, `n . p -
    /// h(n)`, `h` the support (a convex shape lies behind each of its touching planes), and any
    /// point of the surface an upper one. Both are taken at a nearest point found by halving
    /// alone (no start, no step of Newton's: nothing of the field's solve), brought onto the
    /// surface along the line from the middle, and with the normal there; when they are not
    /// within a ten-millionth of each other the ninth review's search over directions (a grid
    /// and golden sections, about three poles) is taken too, and the tighter of each kept.
    #[allow(clippy::disallowed_methods, reason = "a test's own reference, in f64, apart from the code it checks")]
    fn rounded_bracket(q: [f64; 3], half: V3, power: f32, disc: bool) -> Option<(f64, f64)> {
        let pw = f64::from(power);
        let e = pw - 1.0;
        let (p, a): (Vec<f64>, Vec<f64>) = if disc { (vec![(q[0] * q[0] + q[2] * q[2]).sqrt(), q[1].abs()], vec![f64::from(half[0]), f64::from(half[1])]) } else { (q.iter().map(|x| x.abs()).collect(), half.iter().map(|&x| f64::from(x)).collect()) };
        let measure = |x: &[f64]| {
            let u: Vec<f64> = x.iter().zip(&a).map(|(x, a)| x / a).collect();
            let m = u.iter().copied().fold(0.0, f64::max);
            if m == 0.0 { 0.0 } else { m * u.iter().map(|v| (v / m).powf(pw)).sum::<f64>().powf(1.0 / pw) }
        };
        if measure(&p) <= 1.0 {
            return None;
        }
        // Each share for a `mu` (its logarithm), halving `s = ln(u / big)` between where the
        // two terms of its equation each allow (`s0 - ln 2` and `s0`).
        let big: Vec<f64> = p.iter().zip(&a).map(|(p, a)| p / a).collect();
        let shares = |lm: f64| -> Vec<f64> {
            big.iter()
                .zip(&a)
                .map(|(&b, &a)| {
                    if b <= 0.0 {
                        return 0.0;
                    }
                    let lc = lm - 2.0 * a.ln() + (e - 1.0) * b.ln();
                    let top = (-lc / e).min(0.0);
                    let (mut lo, mut hi) = (top - std::f64::consts::LN_2, top);
                    for _ in 0..60 {
                        let mid = 0.5 * (lo + hi);
                        if mid.exp() + (e * mid + lc).exp() > 1.0 {
                            hi = mid;
                        } else {
                            lo = mid;
                        }
                    }
                    b * (0.5 * (lo + hi)).exp()
                })
                .collect()
        };
        let (mut lo, mut hi) = (-700.0f64, 700.0f64);
        for _ in 0..64 {
            let mid = 0.5 * (lo + hi);
            let sum: f64 = shares(mid).iter().map(|u| u.powf(pw)).sum();
            if sum > 1.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let u = shares(0.5 * (lo + hi));
        let x: Vec<f64> = u.iter().zip(&a).map(|(u, a)| u * a).collect();
        let k = measure(&x);
        let on: Vec<f64> = x.iter().map(|x| x / k).collect();
        let upper = on.iter().zip(&p).map(|(o, p)| (p - o) * (p - o)).sum::<f64>().sqrt();
        let normal: Vec<f64> = on.iter().zip(&a).map(|(o, a)| (o / a).powf(e) / a).collect();
        let size = normal.iter().map(|g| g * g).sum::<f64>().sqrt();
        let n: Vec<f64> = normal.iter().map(|g| g / size).collect();
        let lower = n.iter().zip(&p).map(|(n, p)| n * p).sum::<f64>() - support_of(&a, &n, pw).0;
        let mut bracket = (lower, upper);
        if upper - lower > 1e-7 * upper || upper.is_nan() || lower.is_nan() {
            // The review's search over directions.
            let right = std::f64::consts::FRAC_PI_2;
            if disc {
                let ahead = |t: f64| {
                    let n = [(t * right).cos(), (t * right).sin()];
                    n[0] * p[0] + n[1] * p[1] - support_of(&a, &n, pw).0
                };
                let (t, lo) = maximise(&ahead);
                let n = [(t * right).cos(), (t * right).sin()];
                let x = support_of(&a, &n, pw).1;
                bracket = (bracket.0.max(lo), bracket.1.min(((p[0] - x[0]).powi(2) + (p[1] - x[1]).powi(2)).sqrt()));
            } else {
                for pole in 0..3 {
                    let (b, c) = ((pole + 1) % 3, (pole + 2) % 3);
                    let unit = |th: f64, ph: f64| {
                        let mut n = [0.0; 3];
                        n[pole] = (th * right).cos();
                        n[b] = (th * right).sin() * (ph * right).cos();
                        n[c] = (th * right).sin() * (ph * right).sin();
                        n
                    };
                    let ahead = |th: f64, ph: f64| {
                        let n = unit(th, ph);
                        n[0] * p[0] + n[1] * p[1] + n[2] * p[2] - support_of(&a, &n, pw).0
                    };
                    let inner = |ph: f64| maximise(&|th| ahead(th, ph));
                    let (ph, _) = maximise(&|ph| inner(ph).1);
                    let (th, lo) = inner(ph);
                    let x = support_of(&a, &unit(th, ph), pw).1;
                    let up = ((p[0] - x[0]).powi(2) + (p[1] - x[1]).powi(2) + (p[2] - x[2]).powi(2)).sqrt();
                    bracket = (bracket.0.max(lo), bracket.1.min(up));
                }
            }
        }
        Some(bracket)
    }

    /// The roundnesses the field is held at, as powers (2 / roundness): ten from 0.02 to
    /// 0.99, and 1.
    const BRACKET_POWERS: [f32; 11] = [100.0, 40.0, 20.0, 10.0, 6.666_666_5, 4.0, 2.985_074_6, 2.5, 2.222_222_3, 2.020_202, 2.0];

    /// The shapes the field is held on: a box thin along each axis in turn and a cylinder thin
    /// along its height or across it, from 1:1 to a hundred-millionth (half sizes; the
    /// cylinder's are its radius and half height).
    fn bracket_shapes() -> Vec<(String, V3, bool)> {
        let mut shapes = Vec::new();
        for thin in [1.0f32, 1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8] {
            for axis in 0..3 {
                let mut half = [1.0f32, 1.0, 1.0];
                half[(axis + 2) % 3] = 0.6;
                half[axis] = thin;
                shapes.push((format!("a box {thin} thin along {}", ["x", "y", "z"][axis]), half, false));
            }
            shapes.push((format!("a disc {thin} thick"), [1.0, thin, 0.0], true));
            shapes.push((format!("a rod {thin} across"), [thin, 1.0, 0.0], true));
        }
        shapes
    }

    /// The samples about a shape (half sizes `half`; a cylinder's are its radius and half
    /// height), by region, at distances from a millionth of the thickness to a hundred sizes:
    /// over the thin face, beside its rim and at the rim's corner; beside the rim at
    /// mid-thickness; past each kind of edge and past a corner; a hair inside and a hair
    /// outside each hard face. Each with its region's name; a cylinder's made 3D, and the
    /// signs set by the sample's place.
    fn bracket_samples(half: V3, disc: bool) -> Vec<(String, [f64; 3])> {
        let n = if disc { 2 } else { 3 };
        let a: Vec<f64> = (0..n).map(|i| f64::from(half[i])).collect();
        let thin = (0..n).fold(0, |t, i| if a[i] < a[t] { i } else { t });
        let (j, k) = ((thin + 1) % n, (thin + 2) % n);
        let size = a.iter().copied().fold(0.0, f64::max);
        let mut out = Vec::new();
        let mut distances: Vec<f64> = [1e-6, 1e-4, 1e-2, 1.0].iter().map(|s| s * a[thin]).collect();
        for s in [1e-3, 0.1, 1.0, 10.0, 100.0] {
            if distances.iter().all(|d| (d / (s * size) - 1.0).abs() > 0.05) {
                distances.push(s * size);
            }
        }
        for &d in &distances {
            let at = |v: &[(usize, f64)]| -> Vec<f64> {
                let mut c = vec![0.0; n];
                for &(i, x) in v {
                    c[i] = x;
                }
                c
            };
            let mut put = |name: &str, c: Vec<f64>| out.push((format!("{name} at {d:e}"), c));
            if n == 3 {
                put("over the thin face", at(&[(thin, a[thin] + d), (j, 0.3 * a[j]), (k, 0.2 * a[k])]));
                put("over the thin face beside the rim", at(&[(thin, a[thin] + d), (j, 0.999 * a[j]), (k, 0.25 * a[k])]));
                put("over the thin face at the rim's corner", at(&[(thin, a[thin] + d), (j, (1.0 - 1e-5) * a[j]), (k, 0.999 * a[k])]));
                put("beside the rim at mid-thickness", at(&[(thin, 0.5 * a[thin]), (j, a[j] + d), (k, 0.3 * a[k])]));
                put("outside the other long face", at(&[(thin, 0.5 * a[thin]), (j, 0.3 * a[j]), (k, a[k] + d)]));
                put("past the edge of the thin face", at(&[(thin, a[thin] + 0.7 * d), (j, a[j] + 0.7 * d), (k, 0.3 * a[k])]));
                put("past the edge of the long faces", at(&[(thin, 0.3 * a[thin]), (j, a[j] + 0.7 * d), (k, a[k] + 0.7 * d)]));
                put("past a corner", at(&[(thin, a[thin] + 0.58 * d), (j, a[j] + 0.58 * d), (k, a[k] + 0.58 * d)]));
                if d < 0.5 * a[thin] {
                    put("a hair inside the thin face beside the rim", at(&[(thin, a[thin] - d), (j, 0.999 * a[j]), (k, 0.25 * a[k])]));
                }
                if d < 0.5 * a[j] {
                    put("a hair inside a long face", at(&[(thin, 0.999 * a[thin]), (j, a[j] - d), (k, 0.999 * a[k])]));
                }
                if d < 0.5 * a[k] {
                    put("a hair inside the other long face", at(&[(thin, 0.5 * a[thin]), (j, 0.999 * a[j]), (k, a[k] - d)]));
                }
            } else {
                put("over the thin face", at(&[(thin, a[thin] + d), (j, 0.3 * a[j])]));
                put("over the thin face beside the rim", at(&[(thin, a[thin] + d), (j, 0.999 * a[j])]));
                put("beside the rim at mid-thickness", at(&[(thin, 0.5 * a[thin]), (j, a[j] + d)]));
                put("past the edge", at(&[(thin, a[thin] + 0.7 * d), (j, a[j] + 0.7 * d)]));
                if d < 0.5 * a[thin] {
                    put("a hair inside the thin face beside the rim", at(&[(thin, a[thin] - d), (j, 0.999 * a[j])]));
                }
                if d < 0.5 * a[j] {
                    put("a hair inside the other face", at(&[(thin, 0.999 * a[thin]), (j, a[j] - d)]));
                }
            }
        }
        let (cos, sin) = (0.825_335_614_909_678_3, 0.564_642_473_395_035_4);
        out.into_iter()
            .enumerate()
            .map(|(i, (name, c))| {
                let sign = |b: usize| if (i >> b) & 1 == 1 { -1.0 } else { 1.0 };
                let point = if n == 3 { [sign(0) * c[0], sign(1) * c[1], sign(2) * c[2]] } else { [sign(0) * c[0] * cos, sign(1) * c[1], sign(0) * c[0] * sin] };
                (name, point)
            })
            .collect()
    }

    /// The field outside a rounded shape against [`rounded_bracket`] at every `every`th sample
    /// of [`bracket_samples`] about every shape of [`bracket_shapes`] at every roundness, and at
    /// the samples the eighth and ninth reviews named; inside, the field says inside. The worst
    /// margin past the bracket, as a share of the distance.
    fn rounded_field_in_bracket(every: usize) -> f64 {
        let check = |what: &str, q: [f32; 3], half: V3, power: f32, disc: bool, worst: &mut f64| {
            let f = f64::from(prim_distance(&Prim::Rounded { half, power, disc }, q));
            let qd = q.map(f64::from);
            // (What the coordinates' own rounding to `f32` may move the answer by, and the
            // answer's.)
            let slop = 2.0 * f64::from(f32::EPSILON) * (qd[0].abs() + qd[1].abs() + qd[2].abs() + f.abs());
            match rounded_bracket(qd, half, power, disc) {
                None => assert!(f <= slop, "{what}: inside, and the field says {f} outside"),
                Some((lo, hi)) => {
                    assert!(hi - lo <= 1e-6 * hi + slop, "{what}: the reference could not pin the distance: [{lo:e}, {hi:e}]");
                    let past = ((lo - f).max(f - hi) - slop).max(0.0) / lo;
                    *worst = worst.max(past);
                    assert!(past <= 1e-5, "{what}: the field is {f:e}, the distance is in [{lo:e}, {hi:e}]");
                }
            }
        };
        let mut worst = 0.0f64;
        // The ninth review's plank, beside the rim of its thin face (56 times short there), and
        // the eighth's foil, past its corner.
        for z in [9.99e-6f32, 1.0e-5, 1.00009e-5, 1.001e-5, 1.01e-5, 1.1e-5, 2e-5, 1e-4, 1e-3] {
            check(&format!("the ninth review's plank at z {z:e}"), [0.999, 0.25, z], [1.0, 0.5, 1e-5], 6.666_666_5, false, &mut worst);
        }
        check("the eighth review's foil past its corner", [51.5, 0.3, 51.2], [50.0, 5e-5, 50.0], 10.0, false, &mut worst);
        let mut i = 0usize;
        for (name, half, disc) in bracket_shapes() {
            for power in BRACKET_POWERS {
                for (place, q) in bracket_samples(half, disc) {
                    i += 1;
                    if i.is_multiple_of(every) {
                        check(&format!("{name} at power {power}, {place}"), q.map(|x| x as f32), half, power, disc, &mut worst);
                    }
                }
            }
        }
        worst
    }

    /// Outside a rounded shape the field is the distance, wherever the sample is (eighth and
    /// ninth reviews: a start of the solve that was right past a thin sheet's corner was 56
    /// times short just past a thin face beside its rim, and the start before it the other way
    /// round). Held against a bracket that shares nothing with the solve, over every region
    /// about shapes thin along each axis, from 1:1 to a hundred-millionth, at ten roundnesses,
    /// box and cylinder: a share of them here, all of them in the run below.
    #[test]
    fn a_rounded_field_is_the_distance_in_every_region() {
        let worst = rounded_field_in_bracket(29);
        assert!(worst <= 1e-5, "{worst:e}");
    }

    #[test]
    #[ignore = "every sample of the bracket test, each with its reference: run it in release (`--release -- --ignored`)"]
    fn a_rounded_field_is_the_distance_in_every_region_all() {
        let worst = rounded_field_in_bracket(1);
        assert!(worst <= 1e-5, "{worst:e}");
    }

    /// Along lines through every region of [`bracket_samples`] (across the thin face beside
    /// its rim, over its middle and at the rim's corner; across a long face, an edge and the
    /// corner; from a thousandth of the thickness inside to ten sizes out, finest at the hard
    /// surface), the field outside changes no faster than the distance moved: no step (the
    /// ninth review's was a factor of 56 over 3e-8 m). Every `every`th line.
    fn rounded_field_has_no_step(every: usize) {
        let mut line = 0usize;
        for (name, half, disc) in bracket_shapes() {
            let n = if disc { 2 } else { 3 };
            let a: Vec<f64> = (0..n).map(|i| f64::from(half[i])).collect();
            let thin = (0..n).fold(0, |t, i| if a[i] < a[t] { i } else { t });
            let size = a.iter().copied().fold(0.0, f64::max);
            let unit = |v: Vec<f64>| {
                let l = v.iter().map(|x| x * x).sum::<f64>().sqrt();
                v.iter().map(|x| x / l).collect::<Vec<f64>>()
            };
            // (A point of the hard surface, and the way out across it.)
            let mut lines: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
            for along in [0.999f64, 0.3, 1.0 - 1e-5] {
                let mut at: Vec<f64> = a.iter().map(|x| x * 0.25).collect();
                at[thin] = a[thin];
                at[(thin + 1) % n] = along * a[(thin + 1) % n];
                let mut way = vec![0.0; n];
                way[thin] = 1.0;
                lines.push((at, way));
            }
            for dims in [vec![(thin + 1) % n], vec![thin, (thin + 1) % n], (0..n).collect::<Vec<_>>()] {
                let mut at: Vec<f64> = a.iter().map(|x| x * 0.3).collect();
                let mut way = vec![0.0; n];
                for (m, &d) in dims.iter().enumerate() {
                    at[d] = a[d];
                    way[d] = 1.0 + 0.1 * m as f64;
                }
                lines.push((at, unit(way)));
            }
            // (From a thousandth of the thickness inside to a billionth, by tenths of a decade;
            // the surface; from a billionth outside to the thickness, by thirteenths; then on to
            // ten sizes.)
            let mut ts: Vec<f64> = Vec::new();
            let mut t = -a[thin] * 1e-3;
            for _ in 0..=60 {
                ts.push(t);
                t /= 1.258_925_411_794_167_2;
            }
            ts.push(0.0);
            let mut t = a[thin] * 1e-9;
            for _ in 0..=117 {
                ts.push(t);
                t *= 1.193_776_641_714_437_3;
            }
            ts.extend((1..=60).map(|k| a[thin] + 10.0 * size * k as f64 / 60.0));
            for power in BRACKET_POWERS {
                for (at, way) in &lines {
                    line += 1;
                    if !line.is_multiple_of(every) {
                        continue;
                    }
                    let mut last: Option<([f64; 3], f64)> = None;
                    for &t in &ts {
                        let c: Vec<f64> = at.iter().zip(way).map(|(x, w)| x + t * w).collect();
                        let q = if disc { [c[0] as f32, c[1] as f32, 0.0] } else { [c[0] as f32, c[1] as f32, c[2] as f32] };
                        let f = f64::from(prim_distance(&Prim::Rounded { half, power, disc }, q));
                        let qd = q.map(f64::from);
                        if let Some((before, fb)) = last {
                            let d = [qd[0] - before[0], qd[1] - before[1], qd[2] - before[2]];
                            let moved = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                            if f > 0.0 && fb > 0.0 {
                                let slop = 4.0 * f64::from(f32::EPSILON) * (qd[0].abs() + qd[1].abs() + qd[2].abs() + f.abs());
                                assert!((f - fb).abs() <= moved * (1.0 + 1e-5) + 1e-5 * f.max(fb) + slop, "{name} at power {power}, from {before:?} to {qd:?}: the field goes from {fb:e} to {f:e} over {moved:e}");
                            }
                        }
                        last = Some((qd, f));
                    }
                }
            }
        }
    }

    #[test]
    fn a_rounded_field_has_no_step_in_any_region() {
        rounded_field_has_no_step(7);
    }

    #[test]
    #[ignore = "every line of the step scan: run it in release (`--release -- --ignored`)"]
    fn a_rounded_field_has_no_step_in_any_region_all() {
        rounded_field_has_no_step(1);
    }

    /// The volume a rounded box of a power holds, as a share of its hard box's (in `f64`, by
    /// the midpoint rule over a quarter of its middle section).
    #[allow(clippy::disallowed_methods, reason = "a test's own reference, in f64, apart from the code it checks")]
    fn rounded_share(power: f32) -> f64 {
        let (n, p) = (600, f64::from(power));
        let mut sum = 0.0;
        for i in 0..n {
            for j in 0..n {
                let (a, b) = ((i as f64 + 0.5) / n as f64, (j as f64 + 0.5) / n as f64);
                let left = 1.0 - a.powf(p) - b.powf(p);
                if left > 0.0 {
                    sum += left.powf(1.0 / p);
                }
            }
        }
        sum / (n * n) as f64
    }

    /// What the review of roundness built that came out wrong, each as it should be now:
    /// a thin rounded board holds the volume its power gives it (it sank by a sixth); `round`
    /// swells it by its metres on every side (it swelled by its metres times the ratio of
    /// each half size to the least); a blend with a far ball is the same at any roundness (a
    /// field that was infinite a few half sizes out cut it); a board thinner than a cell,
    /// turned, builds (it was refused with a vertex at NaN); and what pruning leaves out of a
    /// tree does not depend on a box in it being rounded (an infinite span stopped it).
    #[test]
    fn a_rounded_shape_is_measured_in_metres() {
        // A thin board, at the roundness of a tenth and a twentieth.
        let half = [0.5f32, 0.025, 0.3];
        let hard = 8.0 * f64::from(half[0] * half[1] * half[2]);
        // (Every power, not only those that passed: at 4 and 6.67 the board came out 4.7% and
        // 2.4% too large with one touching plane; second review.)
        for power in [2.5f32, 4.0, 6.67, 10.0, 20.0, 40.0] {
            let (shape, _) = mesh(&prim(Prim::Rounded { half, power, disc: false }), &opts(48)).unwrap();
            let (volume, expected) = (f64::from(shape.signed_volume()), hard * rounded_share(power));
            assert_eq!(unbalanced(&shape), 0);
            assert!((volume / expected - 1.0).abs() < 0.03, "power {power}: a thin board holds {volume}, and its power gives it {expected}");
        }
        // Swollen by a centimetre: a centimetre further on every side.
        let swollen = SdfNode { kind: Kind::Prim(Prim::Rounded { half, power: 10.0, disc: false }), m: Modifiers { round: 0.01, ..Modifiers::default() } };
        let (lo, hi, lip, slack) = bounds(&swollen).unwrap();
        assert!((0..3).all(|k| (hi[k] - (half[k] + 0.01)).abs() < 1e-5 && (lo[k] + half[k] + 0.01).abs() < 1e-5) && (lip, slack) == (1.0, 0.0), "{lo:?} {hi:?}");
        let (shape, stats) = mesh(&swollen, &opts(96)).unwrap();
        let bb = shape.bounds();
        for k in 0..3 {
            let (want, slack) = (half[k] + 0.01, 0.6 * stats.cell);
            assert!((bb.max[k] - want).abs() < slack && (bb.min[k] + want).abs() < slack, "axis {k}: {} to {}, for Â±{want}", bb.min[k], bb.max[k]);
        }
        // And a centimetre from the rounded surface everywhere, not only along the axes: at a
        // thin board's plan corners one touching plane put the swollen surface up to 4.8 cm
        // out, with a lip above and below (second review). Every seventh vertex of the swollen
        // mesh is held against the unswollen surface, at two roundnesses.
        // (Third review: at roundness 0.8 to 1, and for a swelling past twice the thinness,
        // it was still out: 32 mm for 10 at power 2, 121 for 25, and a plate swollen by 3 cm
        // had vertices 30 cm out. Every roundness here, and swellings to three times the
        // least half size.)
        for (half, power, round) in [
            (half, 10.0f32, 0.01f32),
            (half, 4.0, 0.01),
            (half, 2.5, 0.01),
            (half, 2.0, 0.01),
            (half, 2.0, 0.025),
            (half, 4.0, 0.055),
            (half, 2.0, 0.075),
            ([1.0, 0.03, 1.0], 2.0, 0.03),
        ] {
            let swollen = SdfNode { kind: Kind::Prim(Prim::Rounded { half, power, disc: false }), m: Modifiers { round, ..Modifiers::default() } };
            let (shape, stats) = mesh(&swollen, &opts(96)).unwrap();
            let mut worst = 0.0f64;
            for v in shape.positions.iter().step_by(7) {
                let away = rounded_nearest(v.map(f64::from), half, power, false);
                worst = worst.max((away - f64::from(round)).abs());
            }
            assert!(worst < 0.0025 + 0.6 * f64::from(stats.cell), "half {half:?} power {power}: the surface swollen by {round} m is up to {worst} m off that from the rounded one (cell {})", stats.cell);
        }
        // A ball over a thin plate, blended: the same whatever the plate's roundness.
        let blended = |power: Option<f32>| {
            let plate = match power {
                Some(power) => Prim::Rounded { half: [0.2, 0.005, 0.2], power, disc: false },
                None => Prim::Box { half: [0.2, 0.005, 0.2], round: 0.0 },
            };
            let ball = SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.1 }), m: Modifiers { at: [0.0, 0.19, 0.0], ..Modifiers::default() } };
            let tree = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![ball, prim(plate)], smooth: 0.2 }, m: Modifiers::default() };
            let (shape, stats) = mesh(&tree, &opts(64)).unwrap();
            assert_eq!(unbalanced(&shape), 0);
            (shape.signed_volume(), stats.work)
        };
        let (box_volume, _) = blended(None);
        for power in [20.0f32, 40.0] {
            let (volume, _) = blended(Some(power));
            assert!((volume / box_volume - 1.0).abs() < 0.03, "power {power}: {volume} blended, {box_volume} with the hard plate");
        }
        // Thinner than a cell, and turned.
        let turned = SdfNode { kind: Kind::Prim(Prim::Rounded { half: [1.0, 0.004, 0.5], power: 40.0, disc: false }), m: Modifiers { rotate: [0.0, 0.0, 37.0], ..Modifiers::default() } };
        let (shape, _) = mesh(&turned, &opts(64)).unwrap();
        assert!(shape.positions.iter().all(|p| p.iter().all(|x| x.is_finite())));
        // Forty round cones and a small box, near-united: pruning leaves as much out whether
        // the box is hard or rounded.
        let crowd = |power: Option<f32>| {
            let mut kids: Vec<SdfNode> = (0..40)
                .map(|i| {
                    let x = i as f32 * 0.05;
                    prim(Prim::RoundCone { a: [x, 0.0, 0.0], b: [x, 0.3, 0.02 * (i % 5) as f32], ra: 0.02, rb: 0.012 })
                })
                .collect();
            let half = [0.02f32, 0.02, 0.02];
            kids.push(SdfNode {
                kind: Kind::Prim(match power {
                    Some(power) => Prim::Rounded { half, power, disc: false },
                    None => Prim::Box { half, round: 0.0 },
                }),
                m: Modifiers { at: [1.0, 0.4, 0.0], ..Modifiers::default() },
            });
            let tree = SdfNode { kind: Kind::Group { op: Combine::Union, kids, smooth: 0.01 }, m: Modifiers::default() };
            mesh(&tree, &opts(96)).unwrap().1.work
        };
        let (hard_work, round_work) = (crowd(None), crowd(Some(40.0)));
        assert!(round_work < hard_work * 3 / 2, "{round_work} work with a rounded box, {hard_work} with a hard one");
    }

    /// A square column reaches the corners of its sections whichever way its side points:
    /// its bounds went by an ellipse's, and a column turned an eighth of a turn about its axis
    /// was cut flat at its corners (review of roundness). The same column is built with its
    /// side along x, and toward the diagonal: the same volume, and corners as far out.
    #[test]
    fn a_square_column_keeps_its_corners_whichever_way_it_is_turned() {
        let column = |side: V3| {
            let mut sections = [[0.0; 3]; MAX_SECTIONS];
            sections[0] = [0.0, 0.2, 0.2];
            sections[1] = [1.0, 0.2, 0.2];
            prim(Prim::Column { a: [0.0; 3], b: [0.0, 1.0, 0.0], side, sections, count: 2, square: 8.0 })
        };
        let (straight, _) = mesh(&column([1.0, 0.0, 0.0]), &opts(96)).unwrap();
        let (turned, stats) = mesh(&column([1.0, 0.0, 1.0]), &opts(96)).unwrap();
        let (a, b) = (straight.signed_volume(), turned.signed_volume());
        assert!((a / b - 1.0).abs() < 0.01, "{a} with its side along x, {b} turned");
        // A section of squareness 8 and half sizes of 0.2 reaches 0.2 x 2^(1/2 - 1/8) at its corners.
        let corner = 0.2 * crate::dmath::pow(2.0, 0.5 - 0.125);
        let bb = turned.bounds();
        assert!((bb.max[0] - corner).abs() < stats.cell && (bb.min[2] + corner).abs() < stats.cell, "{:?} to {:?}, corners at Â±{corner}", bb.min, bb.max);
        let (lo, hi, _, _) = bounds(&column([1.0, 0.0, 1.0])).unwrap();
        assert!(lo[0] <= -corner && hi[0] >= corner && lo[2] <= -corner && hi[2] >= corner, "{lo:?} {hi:?}");
    }

    /// A node's `rotate` turns its field the way it turns its bounds: the way a mesh step's
    /// rotation turns a mesh (X, then Y, then Z). The field was turned the opposite way
    /// (it used the rotation where it needed its inverse), which nobody saw on a shape that
    /// looks the same turned either way: a round cone lying along x and turned a quarter turn
    /// about z stood up inside its bounds in the field and lay outside them, and was meshed
    /// at a tenth of its volume (found by the second review of roundness; the fault was as old
    /// as the modeller).
    #[test]
    fn a_turned_node_is_where_its_bounds_are() {
        let turned = |rotate: V3| SdfNode { kind: Kind::Prim(Prim::RoundCone { a: [0.0; 3], b: [1.0, 0.0, 0.0], ra: 0.05, rb: 0.05 }), m: Modifiers { rotate, ..Modifiers::default() } };
        // A capsule of radius 0.05 and length 1: its volume whichever way it is turned.
        let whole = std::f32::consts::PI * 0.05 * 0.05 * (1.0 + 4.0 / 3.0 * 0.05);
        for rotate in [[0.0, 0.0, 0.0], [0.0, 0.0, 90.0], [0.0, 90.0, 0.0], [20.0, 30.0, 40.0], [-60.0, 0.0, 35.0]] {
            let node = turned(rotate);
            let (shape, _) = mesh(&node, &opts(96)).unwrap();
            let volume = shape.signed_volume();
            assert!((volume / whole - 1.0).abs() < 0.04, "turned {rotate:?}: {volume} of {whole}");
            // Its far end is where the rotation takes (1, 0, 0), as a mesh's would be.
            let r = Affine::rotate_euler_deg([rotate[0] as f64, rotate[1] as f64, rotate[2] as f64]);
            let end = r.apply_point([1.0, 0.0, 0.0]);
            let p = prepare(&node, 1, &mut 0);
            assert!((eval(&p, end) + 0.05).abs() < 1e-4, "turned {rotate:?}: the field at its far end {end:?} is {}", eval(&p, end));
            let (lo, hi, _, _) = bounds(&node).unwrap();
            let bb = shape.bounds();
            assert!((0..3).all(|k| bb.min[k] >= lo[k] - 1e-3 && bb.max[k] <= hi[k] + 1e-3 && end[k] >= lo[k] - 1e-3 && end[k] <= hi[k] + 1e-3), "turned {rotate:?}: {:?}..{:?} within {lo:?}..{hi:?}", bb.min, bb.max);
        }
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
        let flange = Prim::Column { a: [0.0, 0.0, 0.0], b: [0.0, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 4, square: 2.0 };
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

    /// Asking which primitive made each triangle: every triangle up to 16,384, every other one
    /// of up to twice that; a limit on the work is never passed, and what is past it is
    /// unanswered.
    #[test]
    fn a_sculpts_triangles_are_asked_up_to_their_limits() {
        let mut far = prim(Prim::Sphere { radius: 0.5 });
        far.m.at = [5.0, 0.0, 0.0];
        let ball = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![prim(Prim::Sphere { radius: 0.5 }), far], smooth: 0.0 }, m: Modifiers::default() };
        let at = |n: usize| Shape { positions: vec![[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]], colors: vec![[1.0; 4]; 3], tris: vec![[0, 1, 2]; n], tags: vec![0; n], ..Shape::default() };
        let asked = |n: usize, work: u64| primitive_of_triangles(&ball, 1, &at(n), MAX_OWNER_ASKED, work);
        assert_eq!(asked(16_384, u64::MAX).asked, 16_384, "every triangle of 16,384");
        assert_eq!(asked(16_385, u64::MAX).asked, 8_193, "every other of one more");
        let a = asked(1000, u64::MAX);
        assert!(a.of.iter().all(|&o| o == 0) && a.work > 0);
        let per = a.work / 1000;
        let short = asked(1000, 100 * per + per / 2);
        assert!(short.asked == 100 && short.work == 100 * per && short.of[99] == 0 && short.of[100] == crate::shape::NO_OWNER, "{} asked, {} spent", short.asked, short.work);
    }

    /// Asked every other triangle, a measured man's primitives' shares of the surface are within
    /// two tenths of a percent of the whole of what they are with every triangle asked (the rule
    /// [`MAX_OWNER_ASKED`] states for a mesh of up to twice its triangles).
    #[test]
    fn asking_every_other_triangle_moves_a_share_by_little() {
        let body = specula_corpus::Body::typical(specula_corpus::Sex::Male);
        let tree = crate::figure::tree(&body, &crate::figure::Shaping::default(), crate::figure::Part::Whole);
        let (shape, _) = mesh(&tree, &MeshOptions { max_work: u64::MAX, ..opts(crate::figure::Part::Whole.resolution()) }).unwrap();
        let n = shape.tris.len();
        let shares = |asked: usize| -> Vec<f64> {
            let a = primitive_of_triangles(&tree, 1, &shape, asked, u64::MAX);
            let mut s: Vec<f64> = Vec::new();
            let mut total = 0.0;
            for (t, &o) in shape.tris.iter().zip(&a.of) {
                let area = f64::from(crate::made::triangle_area(&shape, *t));
                total += area;
                if s.len() <= o as usize {
                    s.resize(o as usize + 1, 0.0);
                }
                s[o as usize] += area;
            }
            s.into_iter().map(|x| x / total).collect()
        };
        let (all, half) = (shares(n), shares(n.div_ceil(2)));
        let worst = (0..all.len().max(half.len())).map(|i| (all.get(i).copied().unwrap_or(0.0) - half.get(i).copied().unwrap_or(0.0)).abs()).fold(0.0, f64::max);
        assert!(n > 4000 && worst < 0.002 && worst > 0.0, "{n} triangles; a share moved by {worst}");
    }

    /// Each primitive's tightest radius, one entry a primitive in depth-first order (what a
    /// difference takes away and a half space among them): every branch, each alone deciding.
    #[test]
    fn each_primitive_has_its_own_radius() {
        let near = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 1e-6);
        let one = |p: Prim| round_radii(&prim(p));
        assert!(near(one(Prim::Sphere { radius: 0.3 })[0], 0.3));
        assert!(near(one(Prim::Cylinder { radius: 0.2, radius_top: 0.5, half_height: 1.0, round: 0.0 })[0], 0.5));
        assert!(near(one(Prim::Torus { radius: 1.0, tube: 0.1 })[0], 0.1));
        // (An ellipsoid's tightest curve: its least radius squared over its greatest.)
        assert!(near(one(Prim::Ellipsoid { radii: [0.5, 2.0, 1.0] })[0], 0.125));
        // (A round cone's thin end; its thick one when the thin one is a point.)
        assert!(near(one(Prim::RoundCone { a: [0.0; 3], b: [0.0, 1.0, 0.0], ra: 0.3, rb: 0.1 })[0], 0.1));
        assert!(near(one(Prim::RoundCone { a: [0.0; 3], b: [0.0, 1.0, 0.0], ra: 0.0, rb: 0.2 })[0], 0.2));
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[..3].copy_from_slice(&[[0.0, 0.3, 0.4], [0.5, 0.25, 0.08], [1.0, 0.3, 0.3]]);
        assert!(near(one(Prim::Column { a: [0.0; 3], b: [0.0, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 3, square: 2.0 })[0], 0.08));
        assert!(near(one(Prim::Column { a: [0.0; 3], b: [0.0, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 1, square: 2.0 })[0], 0.3), "only its first `count` sections");
        assert_eq!(one(Prim::Box { half: [1.0; 3], round: 0.0 }), vec![None]);
        // A swelling, a scale and a stretch above it bear on it, each alone.
        let mut b = prim(Prim::Box { half: [1.0; 3], round: 0.0 });
        b.m.round = 0.02;
        assert!(near(round_radii(&b)[0], 0.02), "a box swollen by 2 cm turns at 2 cm");
        let mut s = prim(Prim::Sphere { radius: 0.3 });
        s.m.round = 0.1;
        assert!(near(round_radii(&s)[0], 0.4));
        s.m.round = 0.0;
        s.m.scale = 2.0;
        assert!(near(round_radii(&s)[0], 0.6));
        s.m.scale = 1.0;
        s.m.stretch = [2.0, 1.0, 1.0];
        assert!(near(round_radii(&s)[0], 0.15), "stretched by two one way: its ends turn at a radius of the least stretch squared over the greatest");
        // A group: one entry each, in order, its own modifiers on all of them.
        let mut g = SdfNode { kind: Kind::Group { op: Combine::Difference, kids: vec![prim(Prim::Sphere { radius: 1.0 }), prim(Prim::HalfSpace { normal: [0.0, 1.0, 0.0], offset: 0.0 }), prim(Prim::Sphere { radius: 0.1 })], smooth: 0.0 }, m: Modifiers::default() };
        g.m.scale = 3.0;
        let r = round_radii(&g);
        assert!(r.len() == 3 && near(r[0], 3.0) && r[1].is_none() && near(r[2], 0.3), "{r:?}");
    }

    // ---- The box the mesher works in ([`mesh_box`], [`reach`]) ----

    fn group(op: Combine, kids: Vec<SdfNode>, smooth: f32) -> SdfNode {
        SdfNode { kind: Kind::Group { op, kids, smooth }, m: Modifiers::default() }
    }

    /// A ball of radius `r` stretched `ratio` times along x, its +x tip at `tip`.
    fn needle(r: f32, ratio: f32, tip: V3) -> SdfNode {
        let mut n = prim(Prim::Sphere { radius: r });
        n.m.stretch = [ratio, 1.0, 1.0];
        n.m.at = [tip[0] - r * ratio, tip[1], tip[2]];
        n
    }

    /// The furthest x along the line from `from` in +x where the field is at most 0 (by a
    /// scan of 20,000 steps up to `span`, then halving): the surface, from the field alone.
    fn surface_along_x(tree: &SdfNode, from: V3, span: f32) -> f32 {
        let pr = prepare(tree, 1, &mut 0);
        let at = |t: f32| eval(&pr, [from[0] + t, from[1], from[2]]);
        let steps = 20_000;
        let last = (0..=steps).map(|i| span * i as f32 / steps as f32).rfind(|t| at(*t) <= 0.0).expect("the line starts inside");
        let (mut a, mut b) = (last, (last + span / steps as f32).min(span));
        for _ in 0..40 {
            let m = 0.5 * (a + b);
            if at(m) <= 0.0 {
                a = m;
            } else {
                b = m;
            }
        }
        from[0] + a
    }

    /// The automatic mesh is closed and reaches the surface along +x (where the field says it
    /// is), within a cell and a half.
    fn reaches(name: &str, tree: &SdfNode, res: u32, truth_x: f32) -> SdfStats {
        let (shape, st) = mesh(tree, &MeshOptions { max_triangles: 2_000_000, max_work: MAX_WORK * 8, ..opts(res) }).unwrap();
        assert_eq!(unbalanced(&shape), 0, "{name}: not closed");
        let b = shape.bounds();
        assert!((b.max[0] - truth_x).abs() <= 1.5 * st.cell, "{name}: the mesh ends at x = {} where the surface is at {truth_x} (cell {})", b.max[0], st.cell);
        st
    }

    /// Two needles stretched ten to one and joined smoothly reach `10 k / 4` past their tips
    /// (each field reads a tenth of the distance past the tip, and the join dips `k / 4`).
    /// `bounds` widened by `k` and cut the join off at 1.1275 (bounds investigation, 2026-11-20).
    #[test]
    fn two_stretched_parts_joined_reach_their_whole_join() {
        let k = 0.1;
        let t = group(Combine::Union, vec![needle(0.1, 10.0, [1.0, 0.0, 0.0]), needle(0.1, 10.0, [1.0, 0.0, 0.0])], k);
        let truth = surface_along_x(&t, [0.0; 3], 2.0);
        assert!((truth - (1.0 + 10.0 * k / 4.0)).abs() < 1e-3, "{truth}");
        reaches("two needles", &t, 96, truth);
    }

    /// A swelling on a stretched node moves its surface by the swelling over the stretch's
    /// ratio along its long axis: a needle of ten to one rounded by 0.05 reaches 0.5 past its
    /// tip (`bounds` grew by 0.05 and cut it at 1.076).
    #[test]
    fn a_round_on_a_stretched_part_reaches_through_its_short_field() {
        let mut n = needle(0.1, 10.0, [1.0, 0.0, 0.0]);
        n.m.round = 0.05;
        let truth = surface_along_x(&n, [0.0; 3], 2.0);
        assert!((truth - 1.5).abs() < 1e-3, "{truth}");
        reaches("rounded needle", &n, 96, truth);
        // A shell, and a group's swelling with a stretched child, the same way.
        let mut s = needle(0.1, 10.0, [1.0, 0.0, 0.0]);
        s.m.shell = 0.04;
        reaches("shelled needle", &s, 96, surface_along_x(&s, [0.9, 0.0, 0.0], 1.0));
        let mut g = group(Combine::Union, vec![prim(Prim::Sphere { radius: 0.3 }), needle(0.05, 12.0, [1.0, 0.0, 0.0])], 0.0);
        g.m.round = 0.03;
        reaches("a group's swelling", &g, 96, surface_along_x(&g, [0.0; 3], 2.0));
    }

    /// A column's field reads its own smaller half across its broad side, so a flat column
    /// swollen by 5 cm reaches out across it by 5 cm times its breadth over its depth: a column
    /// 0.3 broad and 0.05 deep to x = 0.6, where `bounds` took 0.35. An ellipsoid's reads short
    /// off its axes the same way (the drawn trees of the box's truth hold it).
    #[test]
    fn a_flat_column_reaches_through_its_short_field() {
        let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
        sections[..2].copy_from_slice(&[[0.0, 0.3, 0.05], [1.0, 0.3, 0.05]]);
        let mut c = prim(Prim::Column { a: [0.0, -0.5, 0.0], b: [0.0, 0.5, 0.0], side: [1.0, 0.0, 0.0], sections, count: 2, square: 2.0 });
        c.m.round = 0.05;
        let truth = surface_along_x(&c, [0.0; 3], 1.0);
        assert!((truth - 0.6).abs() < 1e-3, "{truth}");
        reaches("a flat column swollen", &c, 96, truth);
    }

    /// Groups inside groups of stretched parts: each level's dip reaches through every
    /// stretched part under it (`bounds` lost half a metre here).
    #[test]
    fn nested_groups_of_stretched_parts_reach_their_surface() {
        let k = 0.1;
        let four = || group(Combine::Union, (0..4).map(|_| needle(0.1, 10.0, [1.0, 0.0, 0.0])).collect(), k);
        let t = group(Combine::Union, vec![four(), four()], k);
        reaches("two groups of four needles", &t, 96, surface_along_x(&t, [0.0; 3], 3.0));
    }

    /// What was right stays right, at the same box: two groups of sixteen balls joined (the
    /// distance-field review's nesting), and a needle with an exact ball at its tip.
    #[test]
    fn the_box_keeps_what_was_right() {
        let k = 0.1;
        let balls = |n: usize| group(Combine::Union, (0..n).map(|_| prim(Prim::Sphere { radius: 0.5 })).collect(), k);
        let t = group(Combine::Union, vec![balls(16), balls(16)], k);
        reaches("2 x 16 balls", &t, 96, surface_along_x(&t, [0.0; 3], 1.5));
        let c = group(Combine::Union, vec![needle(0.1, 10.0, [1.0, 0.0, 0.0]), { let mut b = prim(Prim::Sphere { radius: 0.05 }); b.m.at = [1.0, 0.0, 0.0]; b }], k);
        reaches("a needle and a ball", &c, 96, surface_along_x(&c, [0.0; 3], 2.0));
        for tree in [&t, &c] {
            let (lo, hi, _, _) = bounds(tree).unwrap();
            assert_eq!(mesh_box(tree, 160), Some((lo, hi)), "the box did not move");
        }
    }

    /// A twist turns a point about Y by an angle that grows with its height, and a flat box's
    /// corners swing out past the box about it: a box 2 by 0.2 by 2 twisted 90 degrees a metre
    /// reaches x = sqrt 2 cos(36 degrees) = 1.144 at its top (`bounds` took the box's own,
    /// 1.0). A bend the same in its plane.
    #[test]
    fn a_twisted_or_bent_node_reaches_where_its_corners_swing() {
        let mut t = prim(Prim::Box { half: [1.0, 0.1, 1.0], round: 0.0 });
        t.m.twist = 90.0;
        let swing = 2f32.sqrt() * crate::dmath::sin_cos(36f32.to_radians()).1;
        reaches("twisted flat box", &t, 96, swing);
        let mut b = prim(Prim::Box { half: [1.0, 1.0, 0.1], round: 0.0 });
        b.m.bend = 10.0;
        let (lo, hi) = mesh_box(&b, 160).unwrap();
        let (lo, hi) = (math3::sub(lo, [0.25 * cell_of(lo, hi); 3]), math3::add(hi, [0.25 * cell_of(lo, hi); 3]));
        let pr = prepare(&b, 1, &mut 0);
        // Every point of the bent box (sampled finely over its own box and well past it) is in.
        let mut inside = 0;
        for i in 0..=80 {
            for j in 0..=80 {
                for k in [-0.09f32, 0.0, 0.09] {
                    let p = [-2.0 + i as f32 * 0.05, -2.0 + j as f32 * 0.05, k];
                    if eval(&pr, p) <= 0.0 {
                        inside += 1;
                        assert!((0..3).all(|a| p[a] >= lo[a] && p[a] <= hi[a]), "{p:?} is in the bent box and outside {lo:?}..{hi:?}");
                    }
                }
            }
        }
        assert!(inside > 1000);
    }

    /// The agent's path: two spheres of radius 0.1 stretched ten to one, joined at `smooth`
    /// 0.1, in a recipe. The model spans the whole join, x from -1.25 to 1.25, with no
    /// `bounds` given (it was ±1.1215).
    #[test]
    fn a_recipe_of_stretched_parts_joined_spans_its_whole_join() {
        let style = crate::Style::parse(r##"{ "style": "t", "palette": {}, "materials": { "stone": { "color": "#8c96a3" } } }"##).unwrap();
        let tree = r#"{ "op": "union", "smooth": 0.1, "of": [
            { "op": "sphere", "radius": 0.1, "stretch": [10, 1, 1] },
            { "op": "sphere", "radius": 0.1, "stretch": [10, 1, 1] } ] }"#;
        let text = format!(r#"{{ "name": "needles", "parts": [ {{ "name": "n", "material": "stone", "shape": {{ "op": "sdf", "resolution": 150, "tree": {tree} }} }} ] }}"#);
        let m = crate::build(&text, &crate::BuildParams::new(&style)).unwrap_or_else(|e| panic!("{e}"));
        let b = m.lods[0].bounds();
        // (The cell: 2.5 m and a little over the resolution.)
        let cell = 2.6 / 150.0;
        assert!((b.min[0] + 1.25).abs() <= cell && (b.max[0] - 1.25).abs() <= cell, "the model spans x {} to {}", b.min[0], b.max[0]);
    }

    /// A small deterministic sequence (xorshift) for drawing trees.
    struct Draw(u64);

    impl Draw {
        fn unit(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
        fn range(&mut self, a: f32, b: f32) -> f32 {
            a + (b - a) * self.unit()
        }
        fn pick(&mut self, n: usize) -> usize {
            ((self.unit() * n as f32) as usize).min(n - 1)
        }
        fn chance(&mut self, p: f32) -> bool {
            self.unit() < p
        }
        fn v3(&mut self, a: f32, b: f32) -> V3 {
            [self.range(a, b), self.range(a, b), self.range(a, b)]
        }
    }

    /// Every primitive but the half-space (which only a meeting or a cut may hold).
    fn drawn_prim(d: &mut Draw) -> Prim {
        match d.pick(11) {
            0 => Prim::Sphere { radius: d.range(0.05, 0.4) },
            1 => {
                let half = d.v3(0.03, 0.4);
                Prim::Box { half, round: d.range(0.0, 1.0) * half[0].min(half[1]).min(half[2]) * 0.9 }
            }
            2 => {
                let (radius, half_height) = (d.range(0.05, 0.4), d.range(0.05, 0.4));
                Prim::Cylinder { radius, radius_top: radius, half_height, round: d.range(0.0, 0.9) * radius.min(half_height) }
            }
            3 => Prim::Cylinder { radius: d.range(0.05, 0.4), radius_top: d.range(0.0, 0.4), half_height: d.range(0.05, 0.4), round: 0.0 },
            4 => Prim::Capsule { radius: d.range(0.03, 0.2), half_height: d.range(0.0, 0.4) },
            5 => Prim::Torus { radius: d.range(0.1, 0.4), tube: d.range(0.02, 0.1) },
            6 => Prim::Ellipsoid { radii: d.v3(0.02, 0.5) },
            7 => Prim::Rounded { half: d.v3(0.03, 0.4), power: d.range(2.0, 12.0), disc: d.chance(0.4) },
            8 => Prim::RoundCone { a: d.v3(-0.3, 0.3), b: d.v3(-0.3, 0.3), ra: d.range(0.0, 0.15), rb: d.range(0.01, 0.15) },
            9 => {
                let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
                let count = 1 + d.pick(4);
                let mut t = 0.0;
                for (i, s) in sections.iter_mut().take(count).enumerate() {
                    t = if i == 0 { d.range(0.0, 0.3) } else { (t + d.range(0.05, 0.4)).min(1.0 - 0.01 * (count - i) as f32) };
                    *s = [t, d.range(0.02, 0.25), d.range(0.02, 0.25)];
                }
                let square = if d.chance(0.5) { 2.0 } else { d.range(2.0, 8.0) };
                Prim::Column { a: d.v3(-0.3, 0.3), b: d.v3(-0.3, 0.3), side: d.v3(-1.0, 1.0), sections, count: count as u8, square }
            }
            _ => Prim::Sphere { radius: d.range(0.02, 0.1) },
        }
    }

    /// A tree from the sequence: every primitive, unions, near-unions, meetings and cuts, sharp
    /// and smooth, nested, and every modifier the box answers for.
    fn drawn_tree(d: &mut Draw, depth: u32) -> SdfNode {
        drawn_tree_as(d, depth, false)
    }

    /// [`drawn_tree`], or (`ordinary`) the same draws with no stretch, twist or bend kept.
    fn drawn_tree_as(d: &mut Draw, depth: u32, ordinary: bool) -> SdfNode {
        let kind = if depth < 3 && d.chance(if depth == 0 { 0.85 } else { 0.4 }) {
            let op = [Combine::Union, Combine::Near, Combine::Intersection, Combine::Difference][d.pick(4)];
            let n = 2 + d.pick(3);
            let mut kids: Vec<SdfNode> = (0..n).map(|_| drawn_tree_as(d, depth + 1, ordinary)).collect();
            if matches!(op, Combine::Intersection | Combine::Difference) && d.chance(0.3) {
                let normal = math3::normalize(d.v3(-1.0, 1.0));
                kids.push(prim(Prim::HalfSpace { normal, offset: d.range(-0.2, 0.2) }));
            }
            Kind::Group { op, kids, smooth: if d.chance(0.6) { d.range(0.01, 0.3) } else { 0.0 } }
        } else {
            match drawn_prim(d) {
                // (As a recipe writes a rounded cylinder: its third half is its radius.)
                Prim::Rounded { half, power, disc: true } if ordinary => Kind::Prim(Prim::Rounded { half: [half[0], half[1], half[0]], power, disc: true }),
                p => Kind::Prim(p),
            }
        };
        let mut m = Modifiers::default();
        if d.chance(0.6) {
            m.at = d.v3(-0.4, 0.4);
        }
        if d.chance(0.5) {
            m.rotate = d.v3(-180.0, 180.0);
        }
        if d.chance(0.3) {
            m.scale = d.range(0.5, 2.0);
        }
        if d.chance(0.5) {
            let stretch = [0, 1, 2].map(|_| if d.chance(0.5) { d.range(0.1, 12.0) } else { 1.0 });
            if !ordinary {
                m.stretch = stretch;
            }
        }
        if d.chance(0.15) {
            m.mirror = [d.chance(0.5), d.chance(0.5), d.chance(0.5)];
        }
        if d.chance(0.3) {
            m.round = d.range(0.0, 0.12);
        }
        if d.chance(0.15) {
            m.shell = d.range(0.005, 0.08);
        }
        if d.chance(0.15) {
            m.displace = Some((d.range(-0.06, 0.06), d.range(0.08, 0.5), 1 + d.pick(3) as u32));
        }
        if d.chance(0.12) {
            let twist = d.range(-300.0, 300.0);
            if !ordinary {
                m.twist = twist;
            }
        }
        if d.chance(0.12) {
            let bend = d.range(-200.0, 200.0);
            if !ordinary {
                m.bend = bend;
            }
        }
        if d.chance(0.12) {
            m.elongate = d.v3(0.0, 0.3);
        }
        if d.chance(0.1) {
            m.repeat = Some(([1 + d.pick(3) as u32, 1 + d.pick(2) as u32, 1], d.v3(0.1, 0.6)));
        }
        SdfNode { kind, m }
    }

    /// The worst point found outside the box the mesher takes for `tree` where its field is at
    /// or below nothing: how far out it is (0 for none), and the point. The field is asked and
    /// nothing else: on the box's faces pushed out by five distances, a grid on each, and at
    /// points drawn in the shell to half the box's size past it.
    fn escape(tree: &SdfNode, d: &mut Draw, take: impl Fn(&SdfNode) -> Option<(V3, V3)>) -> Option<(f32, V3)> {
        let (lo, hi) = take(tree)?;
        let size = (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max).max(1e-3);
        let pr = prepare(tree, 1, &mut 0);
        // What the box leaves out on purpose, a quarter of a cell at the finest resolution
        // ([`mesh_box`]: the grid samples half a cell past it), and a hair more.
        let tol = 0.25 * cell_of(lo, hi) + 1e-5 * size + 1e-6;
        let mut worst: Option<(f32, V3)> = None;
        let mut look = |p: V3| {
            let out = math3::length([0, 1, 2].map(|a| (lo[a] - p[a]).max(p[a] - hi[a]).max(0.0)));
            if out > tol && eval(&pr, p) <= 0.0 && worst.is_none_or(|(w, _)| out > w) {
                worst = Some((out, p));
            }
        };
        const N: usize = 14;
        for push in [2.0 * tol, 0.004 * size, 0.02 * size, 0.08 * size, 0.3 * size] {
            let (l, h) = (math3::sub(lo, [push; 3]), math3::add(hi, [push; 3]));
            for axis in 0..3 {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                for side in [l[axis], h[axis]] {
                    for i in 0..=N {
                        for j in 0..=N {
                            let mut p = [0.0; 3];
                            p[axis] = side;
                            p[u] = l[u] + (h[u] - l[u]) * i as f32 / N as f32;
                            p[v] = l[v] + (h[v] - l[v]) * j as f32 / N as f32;
                            look(p);
                        }
                    }
                }
            }
        }
        for _ in 0..1500 {
            let p = [0, 1, 2].map(|a| d.range(lo[a] - 0.5 * size, hi[a] + 0.5 * size));
            look(p);
        }
        Some(worst.unwrap_or((0.0, [0.0; 3])))
    }

    /// A cell of the box at the finest resolution.
    fn cell_of(lo: V3, hi: V3) -> f32 {
        (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max) / MAX_RESOLUTION as f32
    }

    fn finest(tree: &SdfNode) -> Option<(V3, V3)> {
        mesh_box(tree, MAX_RESOLUTION)
    }

    fn has_stretch(n: &SdfNode) -> bool {
        n.m.stretch != [1.0; 3] || matches!(&n.kind, Kind::Group { kids, .. } if kids.iter().any(has_stretch))
    }

    /// **The box holds the solid, by the field alone**: trees drawn from a fixed sequence
    /// (every primitive; unions, near-unions, meetings and cuts, sharp and smooth, nested;
    /// every modifier the box answers for: stretch, scale, turn, mirror, swelling, shell,
    /// noise, twist, bend, elongation, repetition), the field sampled outside the box the
    /// mesher would take, and no point there where it is at or below nothing. Nothing of the
    /// box's own reasoning is used: only [`eval`]. ([`bounds`] alone fails it.)
    #[test]
    fn no_solid_is_outside_the_box_the_mesher_takes() {
        let mut d = Draw(0x9e37_79b9_7f4a_7c15);
        let (mut trees, mut stretched) = (0, 0);
        for i in 0..240 {
            let tree = drawn_tree(&mut d, 0);
            if count_nodes(&tree) > MAX_NODES {
                continue;
            }
            let Some((out, at)) = escape(&tree, &mut d, finest) else { continue };
            trees += 1;
            stretched += usize::from(has_stretch(&tree));
            assert!(out == 0.0, "tree {i}: the field is at or below nothing {out} m outside the box, at {at:?}\n{tree:?}");
        }
        assert!(trees > 150, "{trees} bounded trees");
        assert!(stretched > 50, "{stretched} with a stretch");
    }

    /// The mesher's pruning ([`prune`]) is exact, bit for bit, at every point of the ball it
    /// was pruned for: drawn trees, the bounds investigation's stretched cases, and pruning
    /// again inside a pruned tree as the mesher does (a half block's tree from a block's).
    #[test]
    fn a_pruned_drawn_tree_is_the_whole_tree_in_its_ball() {
        let mut d = Draw(0x2545_f491_4f6c_dd1d);
        let mut trees: Vec<SdfNode> = (0..120).map(|_| drawn_tree(&mut d, 0)).filter(|t| count_nodes(t) <= MAX_NODES && bounds(t).is_some()).collect();
        let k = 0.1;
        let four = || group(Combine::Union, (0..4).map(|_| needle(0.1, 10.0, [1.0, 0.0, 0.0])).collect(), k);
        trees.push(group(Combine::Union, vec![four(), four()], k));
        trees.push(group(Combine::Union, vec![needle(0.05, 12.0, [0.6, 0.0, 0.0]), needle(0.04, 12.0, [0.6, 0.0, 0.0])], 0.04));
        let mut cluster: Vec<SdfNode> = (0..8).map(|_| prim(Prim::Sphere { radius: 0.5 })).collect();
        let mut up = needle(0.02, 20.0, [0.5, 0.0, 0.0]);
        up.m.rotate = [0.0, 0.0, 90.0];
        up.m.at = [0.0, 0.1, 0.0];
        cluster.push(up);
        trees.push(group(Combine::Union, cluster, k));
        let mut compared = 0u64;
        for (t, tree) in trees.iter().enumerate() {
            let (lo, hi) = finest(tree).unwrap();
            let size = (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max);
            let whole = prepare(tree, 1, &mut 0);
            for _ in 0..40 {
                let c = [0, 1, 2].map(|a| d.range(lo[a] - 0.1 * size, hi[a] + 0.1 * size));
                let reach = size * d.range(0.004, 0.15);
                let block = prune(&whole, c, reach);
                let c2 = math3::add(c, math3::scale(math3::normalize(d.v3(-1.0, 1.0)), 0.5 * reach * d.unit()));
                let half = prune(&block, c2, 0.5 * reach);
                for i in 0..24 {
                    let dir = math3::normalize(d.v3(-1.0, 1.0));
                    let (centre, r, tree) = if i % 2 == 0 { (c, reach, &block) } else { (c2, 0.5 * reach, &half) };
                    let p = math3::add(centre, math3::scale(dir, r * d.unit()));
                    assert_eq!(eval(tree, p).to_bits(), eval(&whole, p).to_bits(), "tree {t}, point {p:?}: {} against {}", eval(tree, p), eval(&whole, p));
                    compared += 1;
                }
            }
        }
        assert!(compared > 90_000, "{compared}");
    }

    /// A part thinner than a cell (a crescent's rim) is meshed as a surface, each of its sides
    /// on vertices of its own, and the build says where it is thinner than a cell, in its
    /// report and its notes; a plain ball has no such cell.
    #[test]
    fn a_part_thinner_than_a_cell_is_said_where_it_is() {
        let ball = |x: f32| {
            let mut s = prim(Prim::Sphere { radius: 0.5 });
            s.m.at = [x, 0.0, 0.0];
            s
        };
        let crescent = group(Combine::Difference, vec![ball(0.0), ball(0.05)], 0.0);
        let (shape, st) = mesh(&crescent, &opts(48)).unwrap();
        assert_eq!(unbalanced(&shape), 0);
        assert!(st.thin_cells > 100 && st.tangled_edges == 0 && shape.open_edge_count() == 0, "{} thin cells, {} tangled edges", st.thin_cells, st.tangled_edges);
        // (Toward the rim, where the two balls' surfaces cross on the plane x = 0.025 at radius
        // 0.4994, the sheet between them is thinner than a cell: x from about -0.24 to -0.04;
        // the box is of whole cells, 0.0208 m each.)
        let (lo, hi) = st.thin_at.unwrap();
        assert!(lo[0] > -0.35 && hi[0] < 0.03 && hi[1] > 0.4 && lo[1] < -0.4, "{lo:?}..{hi:?}");
        let (whole, st) = mesh(&ball(0.0), &opts(48)).unwrap();
        assert!(st.thin_cells == 0 && st.tangled_edges == 0 && st.thin_at.is_none() && whole.open_edge_count() == 0);
        // Through a recipe: the report and the notes.
        let style = crate::Style::parse(r##"{ "style": "t", "palette": {}, "materials": { "stone": { "color": "#8c96a3" } } }"##).unwrap();
        let text = r#"{ "name": "crescent", "parts": [ { "name": "c", "material": "stone", "shape": { "op": "sdf", "tree": { "op": "difference", "of": [
            { "op": "sphere", "radius": 0.5 }, { "op": "sphere", "radius": 0.5, "at": [0.05, 0, 0] } ] } } } ] }"#;
        let m = crate::build(text, &crate::BuildParams::new(&style)).unwrap();
        assert!(m.notes.iter().any(|n| n.contains("closed surface there") && n.contains("thinner than a cell")), "{:?}", m.notes);
        assert!(!m.notes.iter().any(|n| n.contains("fault of the mesher")), "{:?}", m.notes);
        assert!(m.extras.stats.iter().any(|(_, j)| j.get("thin_cells").and_then(|v| v.as_f64()).is_some_and(|c| c > 0.0)), "{:?}", m.extras.stats);
        assert!(m.lods[0].open_edge_count() == 0, "the model's mesh is closed");
    }

    /// No sculpt in the repository's recipes, and no figure at the limits of its body and
    /// shaping, has a box that [`reach`] grows: each is meshed exactly as before the box was
    /// mended (the same box is the same cells, the same samples and the same mesh).
    #[test]
    fn the_repositorys_sculpts_keep_their_boxes() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        BOXES.with(|b| b.set([0; 2]));
        for dir in [root.join("games/sepulcra/assets"), root.join("docs/fidelity/assets")] {
            let style = crate::Style::parse(&std::fs::read_to_string(dir.join("style.json")).unwrap()).unwrap();
            let mut lib = std::collections::BTreeMap::new();
            for e in std::fs::read_dir(dir.join("recipes")).unwrap() {
                let p = e.unwrap().path();
                if p.extension().is_some_and(|x| x == "json") {
                    lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
                }
            }
            for (name, text) in lib.iter().filter(|(_, t)| t.contains("\"sdf\"") || t.contains("\"figure\"")) {
                let before = BOXES.with(|b| b.get())[1];
                // (A build meshes each sculpt at its detail and, where it needs one, coarsely.)
                let (_, meshes) = kept(|| crate::build(text, &crate::BuildParams { library: Some(&lib), ..crate::BuildParams::new(&style) }).unwrap());
                assert_eq!(BOXES.with(|b| b.get())[1], before, "{name}: a sculpt's box grew");
                // (And its search asked nothing, so its work is what it was: second review of
                // the box's search, finding 3. A change that made one search would fail here.)
                assert!(!meshes.is_empty(), "{name}");
                for (i, (_, st)) in meshes.iter().enumerate() {
                    assert_eq!((st.box_asks, st.box_work), (0, 0), "{name}: mesh {i}'s box was searched");
                }
            }
        }
        let [taken, grew] = BOXES.with(|b| b.get());
        assert!(taken >= 14, "{taken} sculpts meshed");
        assert_eq!(grew, 0, "a repository sculpt's box grew");
        use specula_corpus::{Body, Sex};
        let mut figures = 0;
        for sex in [Sex::Female, Sex::Male] {
            let [short, tall] = specula_corpus::ansur::statures(sex).map(|mm| mm / 1000.0);
            for stature in [short, 0.5 * (short + tall), tall] {
                for (build, traits) in [(0.0, vec![]), (-2.5, vec![("shoulders", 3.0), ("hips", -3.0)]), (2.5, vec![("hips", 3.0), ("chest", 3.0), ("hands", 3.0), ("feet", -3.0)]), (0.0, vec![("legs", -3.0), ("arms", 3.0), ("head", 3.0)])] {
                    let Ok(body) = Body::with_build(sex, stature, build).and_then(|b| b.with_traits(&traits)) else { continue };
                    // (The default melting with the limbs at their limits, none, and twice the
                    // default. A melting much wider than the default grows some boxes: the
                    // figure's thin columns read short of the distance across their broad side,
                    // which a wide join reaches through. Said in PROCEDURAL_MODELLING.md.)
                    for (arms, legs, smooth) in [(35.0, 7.0, 0.025), (0.0, 0.0, 0.0), (60.0, 45.0, 0.025), (0.0, 45.0, 0.01), (35.0, 7.0, 0.05)] {
                        let shaping = crate::figure::Shaping { arms, legs, smooth };
                        for name in crate::figure::Part::NAMES {
                            let part = crate::figure::Part::parse(name).unwrap();
                            let tree = crate::figure::tree(&body, &shaping, part);
                            let (lo, hi, _, _) = bounds(&tree).unwrap();
                            assert_eq!(mesh_box(&tree, part.resolution()), Some((lo, hi)), "{sex:?} {stature} build {build} {traits:?}, arms {arms} legs {legs} smooth {smooth}: the {name}'s box grew");
                            figures += 1;
                        }
                    }
                }
            }
        }
        assert!(figures > 300, "{figures}");
    }

    // ---- Ordinary sculpts keep the old box (bounds review 1, 2026-11-20) ----

    /// The sequence of the review that found ordinary sculpts' boxes growing (xorshift64*),
    /// and its trees: no stretch, twist or bend; spheres, boxes, cylinders, cones, capsules,
    /// tori, ellipsoids, round cones and columns; moved, turned, scaled, swollen, shelled,
    /// noised, mirrored and elongated; unions, near-unions, meetings and cuts, sharp and
    /// smooth, three deep.
    struct Review(u64);

    impl Review {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }
        fn unit(&mut self) -> f32 {
            (self.next() >> 40) as f32 / (1u64 << 24) as f32
        }
        fn range(&mut self, a: f32, b: f32) -> f32 {
            a + (b - a) * self.unit()
        }
        fn pick(&mut self, n: usize) -> usize {
            ((self.unit() * n as f32) as usize).min(n - 1)
        }
        fn chance(&mut self, p: f32) -> bool {
            self.unit() < p
        }
        fn v3(&mut self, a: f32, b: f32) -> V3 {
            [self.range(a, b), self.range(a, b), self.range(a, b)]
        }
    }

    fn review_prim(d: &mut Review) -> Prim {
        match d.pick(9) {
            0 => Prim::Sphere { radius: d.range(0.03, 0.4) },
            1 => {
                let half = d.v3(0.02, 0.4);
                Prim::Box { half, round: d.range(0.0, 1.0) * half[0].min(half[1]).min(half[2]) }
            }
            2 => {
                let (radius, half_height) = (d.range(0.02, 0.4), d.range(0.02, 0.4));
                Prim::Cylinder { radius, radius_top: radius, half_height, round: d.range(0.0, 1.0) * radius.min(half_height) }
            }
            3 => Prim::Cylinder { radius: d.range(0.02, 0.4), radius_top: 0.0, half_height: d.range(0.02, 0.4), round: 0.0 },
            4 => Prim::Capsule { radius: d.range(0.02, 0.2), half_height: d.range(0.0, 0.4) },
            5 => Prim::Torus { radius: d.range(0.05, 0.4), tube: d.range(0.01, 0.1) },
            6 => {
                let a = d.range(0.05, 0.5);
                Prim::Ellipsoid { radii: [a, a * d.range(0.2, 1.0), a * d.range(0.2, 1.0)] }
            }
            7 => Prim::RoundCone { a: d.v3(-0.3, 0.3), b: d.v3(-0.3, 0.3), ra: d.range(0.01, 0.15), rb: d.range(0.01, 0.15) },
            _ => {
                let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
                sections[0] = [0.0, d.range(0.03, 0.2), d.range(0.03, 0.2)];
                sections[1] = [0.5, d.range(0.03, 0.2), d.range(0.03, 0.2)];
                sections[2] = [1.0, d.range(0.03, 0.2), d.range(0.03, 0.2)];
                Prim::Column { a: d.v3(-0.4, 0.4), b: d.v3(-0.4, 0.4), side: d.v3(-1.0, 1.0), sections, count: 3, square: if d.chance(0.5) { 2.0 } else { 3.0 } }
            }
        }
    }

    fn review_tree(d: &mut Review, depth: u32) -> SdfNode {
        let mut n = if depth < 3 && d.chance(if depth == 0 { 0.9 } else { 0.4 }) {
            let op = [Combine::Union, Combine::Near, Combine::Intersection, Combine::Difference][d.pick(4)];
            let kids = (0..2 + d.pick(4)).map(|_| review_tree(d, depth + 1)).collect();
            group(op, kids, if d.chance(0.6) { d.range(0.0, 0.15) } else { 0.0 })
        } else {
            prim(review_prim(d))
        };
        let m = &mut n.m;
        if d.chance(0.6) {
            m.at = d.v3(-0.3, 0.3);
        }
        if d.chance(0.5) {
            m.rotate = d.v3(-180.0, 180.0);
        }
        if d.chance(0.2) {
            m.scale = d.range(0.5, 2.0);
        }
        if d.chance(0.2) {
            m.round = d.range(0.0, 0.05);
        }
        if d.chance(0.08) {
            m.shell = d.range(0.01, 0.05);
        }
        if d.chance(0.08) {
            m.displace = Some((d.range(-0.03, 0.03), d.range(0.1, 0.5), 2));
        }
        if d.chance(0.08) {
            m.mirror = [true, false, false];
        }
        if d.chance(0.08) {
            m.elongate = d.v3(0.0, 0.2);
        }
        n
    }

    /// How far [`mesh_box`] moved a side of [`bounds`]'s box out, in cells of `res`.
    fn grown_cells(tree: &SdfNode, res: u32) -> f32 {
        let (blo, bhi, _, _) = bounds(tree).unwrap();
        let (lo, hi) = mesh_box(tree, res).unwrap();
        let cell = (0..3).map(|k| bhi[k] - blo[k]).fold(0.0f32, f32::max) / res as f32;
        (0..3).map(|k| (blo[k] - lo[k]).max(hi[k] - bhi[k])).fold(0.0f32, f32::max) / cell
    }

    /// What kinds of primitive, combination and modifier a tree holds (a bit each).
    fn kinds_in(n: &SdfNode) -> u32 {
        let d = Modifiers::default();
        let m = &n.m;
        let mods = [m.at != d.at, m.rotate != d.rotate, m.scale != d.scale, m.mirror != d.mirror, m.round != 0.0, m.shell != 0.0, m.displace.is_some(), m.elongate != d.elongate, m.repeat.is_some()];
        let mut bits = mods.iter().enumerate().map(|(i, &on)| u32::from(on) << i).sum::<u32>();
        bits |= match &n.kind {
            Kind::Prim(p) => {
                1 << (9 + match p {
                    Prim::Sphere { .. } => 0,
                    Prim::Box { .. } => 1,
                    Prim::Cylinder { radius_top, .. } if *radius_top == 0.0 => 2,
                    Prim::Cylinder { .. } => 3,
                    Prim::Capsule { .. } => 4,
                    Prim::Torus { .. } => 5,
                    Prim::Ellipsoid { .. } => 6,
                    Prim::Rounded { .. } => 7,
                    Prim::RoundCone { .. } => 8,
                    Prim::Column { .. } => 9,
                    Prim::HalfSpace { .. } => 10,
                })
            }
            Kind::Group { op, kids, smooth } => {
                let op = 1 << (20 + *op as u32 * 2 + u32::from(*smooth > 0.0));
                kids.iter().fold(op, |b, k| b | kinds_in(k))
            }
        };
        bits
    }

    /// **For a sculpt with nothing stretched, twisted or bent, the mesher's box is
    /// [`bounds`]'s** (bounds review 1: of 300 such trees drawn by the reviewer, 29 changed
    /// mesh, the box growing where nothing was cut off). The reviewer's 300 trees and 300 of
    /// this crate's own sequence (every primitive, the half-space in meetings and cuts, every
    /// combination sharp and smooth, every modifier but stretch, twist and bend), each at
    /// resolutions 24 and 64. Those whose box still grows are named, each with a build that
    /// shows why.
    #[test]
    fn an_ordinary_sculpt_is_meshed_in_the_old_box() {
        let mut grew = Vec::new();
        let mut kinds = 0u32;
        let mut trees = Vec::new();
        let mut d = Review(0x1111_2222_3333_4444);
        for i in 0..300 {
            trees.push(("review", i, review_tree(&mut d, 0)));
        }
        let mut d = Draw(0x51ed_270b_27a3_c4e9);
        for i in 0..300 {
            trees.push(("own", i, drawn_tree_as(&mut d, 0, true)));
        }
        let mut held = 0;
        for (set, i, tree) in &trees {
            if count_nodes(tree) > MAX_NODES || bounds(tree).is_none() {
                continue;
            }
            assert!(!has_stretch(tree));
            kinds |= kinds_in(tree);
            for res in [24, 64] {
                let (lo, hi, _, _) = bounds(tree).unwrap();
                if mesh_box(tree, res) == Some((lo, hi)) {
                    held += 1;
                } else {
                    grew.push((*set, *i, res, grown_cells(tree, res)));
                }
            }
        }
        // (Every primitive, combination and modifier the trees may hold is among them.)
        let all = ((1 << 9) - 1) | (((1 << 11) - 1) << 9) | (0xff << 20);
        assert_eq!(kinds & all, all, "kinds drawn: {kinds:b}");
        assert!(held > 1050, "{held} boxes held");
        // The trees whose box still grows, and why.
        let named: Vec<(&str, usize, u32)> = grew.iter().map(|g| (g.0, g.1, g.2)).collect();
        assert_eq!(named, ORDINARY_THAT_GROW.iter().map(|g| (g.0, g.1, g.2)).collect::<Vec<_>>(), "{grew:?}");
        for (g, (set, i, res, why)) in grew.iter().zip(ORDINARY_THAT_GROW) {
            assert!(g.3 <= why.most_cells(), "{set} {i} at {res}: grows {} cells", g.3);
            // (The build that shows why, at the finer of the two.)
            if *res == 64 {
                let tree = &trees.iter().find(|t| t.0 == *set && t.1 == *i).unwrap().2;
                why.shown(tree, *res);
            }
        }
    }

    /// **The figure melted four and eight times wider than its default keeps its box** (bounds
    /// review 1, finding 3: at `smooth` 0.1 and 0.2 every `whole` and some `body` builds
    /// changed, the reach being looser than the solid there): both sexes, three statures, three
    /// builds, the limbs at 35/7 and 60/45 degrees, every part. The search of the field shows
    /// nothing past the old box, within its budget.
    #[test]
    fn a_figure_melted_wide_keeps_its_box() {
        use specula_corpus::{Body, Sex};
        let (mut n, mut grew, mut most) = (0, Vec::new(), 0u64);
        for sex in [Sex::Female, Sex::Male] {
            let [short, tall] = specula_corpus::ansur::statures(sex).map(|mm| mm / 1000.0);
            for stature in [short, 0.5 * (short + tall), tall] {
                for build in [-2.0, 0.0, 2.0] {
                    let Ok(body) = Body::with_build(sex, stature, build) else { continue };
                    for (arms, legs, smooth) in [(35.0, 7.0, 0.1f32), (35.0, 7.0, 0.2), (60.0, 45.0, 0.1), (60.0, 45.0, 0.2)] {
                        let shaping = crate::figure::Shaping { arms, legs, smooth };
                        for name in crate::figure::Part::NAMES {
                            let part = crate::figure::Part::parse(name).unwrap();
                            let tree = crate::figure::tree(&body, &shaping, part);
                            let (lo, hi, _, _) = bounds(&tree).unwrap();
                            let (b, asks) = mesh_box_seeded(&tree, part.resolution(), 1).unwrap();
                            most = most.max(asks);
                            n += 1;
                            if b != (lo, hi) {
                                grew.push(format!("{sex:?} {stature} {build} {arms}/{legs} {smooth} {name} asks {asks}"));
                            }
                        }
                    }
                }
            }
        }
        assert!(n > 400, "{n} parts");
        assert!(grew.is_empty(), "{grew:#?}");
        assert!(most <= CLEAR_ASKS, "{most}");
    }

    /// **The range over a ball holds every value the field takes in it** ([`field_range`], on
    /// which keeping the old box rests): trees drawn from a fixed sequence with every primitive,
    /// combination and modifier (stretch, twist, bend, repeat, noise, near-unions among them),
    /// balls from a thousandth of the tree's size to a third of it, about its box and past it;
    /// every point sampled in a ball is evaluated by the field alone and lies within the range.
    #[test]
    fn a_fields_range_over_a_ball_holds_every_value_in_it() {
        let mut r = Review(0x9e37_79b9_7f4a_7c15);
        let (mut balls, mut unknown, mut points) = (0, 0, 0);
        // Cases where each rule is tight, sampled more (each drawn tree after them).
        let at = |mut n: SdfNode, p: V3| {
            n.m.at = p;
            n
        };
        let ball = |rad: f32, p: V3| at(prim(Prim::Sphere { radius: rad }), p);
        let mut needle = prim(Prim::Sphere { radius: 0.05 });
        needle.m.stretch = [8.0, 1.0, 1.0];
        let mut flat = prim(Prim::Ellipsoid { radii: [0.5, 0.05, 0.3] });
        flat.m.rotate = [20.0, 30.0, 40.0];
        let mut wandering = [[0.0f32; 3]; MAX_SECTIONS];
        wandering[..2].copy_from_slice(&[[0.0, 0.05, 0.25], [1.0, 0.25, 0.05]]);
        // (A ball set off its cell's middle: its field jumps at the cells' edges.)
        let mut rows = group(Combine::Union, vec![ball(0.2, [0.1, 0.0, 0.0])], 0.0);
        rows.m.repeat = Some(([4, 1, 1], [0.25, 0.0, 0.0]));
        let mut twisted = prim(Prim::Box { half: [0.4, 0.5, 0.05], round: 0.0 });
        twisted.m.twist = 90.0;
        let hard: Vec<SdfNode> = vec![
            group(Combine::Near, (0..5).map(|i| ball(0.2, [0.05 * i as f32, 0.0, 0.0])).collect(), 0.2),
            group(Combine::Union, vec![ball(0.2, [0.0; 3]), ball(0.2, [0.1, 0.0, 0.0])], 0.3),
            group(Combine::Difference, vec![ball(0.4, [0.0; 3]), ball(0.3, [0.3, 0.0, 0.0])], 0.1),
            group(Combine::Intersection, vec![ball(0.4, [0.0; 3]), ball(0.4, [0.4, 0.0, 0.0])], 0.1),
            group(Combine::Near, vec![needle.clone(), at(needle, [0.0, 0.08, 0.0])], 0.1),
            flat,
            prim(Prim::Column { a: [0.0, -0.5, 0.0], b: [0.0, 0.5, 0.0], side: [1.0, 0.0, 0.0], sections: wandering, count: 2, square: 2.0 }),
            rows,
            twisted,
        ];
        let mut d = Draw(0x2545_f491_4f6c_dd1d);
        // (The hard cases' balls mostly about their solids; the drawn trees' about and past.)
        let trees: Vec<(usize, SdfNode, usize, f32)> = hard.into_iter().enumerate().map(|(i, t)| (1000 + i, t, 1500, 0.1)).chain((0..240).map(|t| (t, drawn_tree(&mut d, 0), 12, 0.5))).collect();
        for (t, tree, many, past) in trees {
            if count_nodes(&tree) > MAX_NODES {
                continue;
            }
            let (lo, hi) = match bounds(&tree) {
                Some((lo, hi, _, _)) => (lo, hi),
                None => ([-1.0; 3], [1.0; 3]),
            };
            let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.05f32, f32::max);
            let p = prepare(&tree, 1, &mut 0);
            for _ in 0..many {
                let c = [0, 1, 2].map(|k| r.range(lo[k] - past * size, hi[k] + past * size));
                let rad = size * [0.001, 0.01, 0.05, 0.3][r.pick(4)];
                let (least, most) = field_range(&p, c, rad, &mut 1, &mut Spent::default());
                if least <= -UNKNOWN {
                    unknown += 1;
                    continue;
                }
                balls += 1;
                for _ in 0..24 {
                    let v = loop {
                        let v = r.v3(-1.0, 1.0);
                        if math3::length(v) <= 1.0 {
                            break v;
                        }
                    };
                    let q = math3::add(c, math3::scale(v, rad));
                    let f = eval(&p, q);
                    let tol = 1e-5 * (1.0 + f.abs());
                    assert!(f >= least - tol && f <= most + tol, "tree {t}: {f} at {q:?} outside {least}..{most} (ball {c:?}, {rad})\n{tree:?}");
                    points += 1;
                }
            }
        }
        assert!(balls > 1500 && points > 36_000, "{balls} balls, {unknown} unknown, {points} points");
    }

    /// **The search is bounded, charged and sound past its budget** ([`CLEAR_ASKS`]): a tree of
    /// the crate's drawn set whose search runs out (a column whose field changes nearly ten
    /// times faster than the distance, near-joined) asks no more than the budget and one ball's
    /// copies, and the side it could not show clear moves out to the reach; a build of it is
    /// charged what the search did (its work is the work of the same grid in that box given,
    /// and the search's); a tree that passes no side asks nothing.
    #[test]
    fn the_search_of_the_box_is_bounded_and_charged() {
        let mut d = Draw(0x51ed_270b_27a3_c4e9);
        let tree = (0..=150).map(|_| drawn_tree_as(&mut d, 0, true)).last().unwrap();
        let (lo, hi, _, _) = bounds(&tree).unwrap();
        let (rlo, rhi) = reach(&tree, 0.0).unwrap().within;
        let (b, asks) = mesh_box_seeded(&tree, 24, 1).unwrap();
        assert!((CLEAR_ASKS..=CLEAR_ASKS + RANGE_COPIES as u64 + 2).contains(&asks), "{asks}");
        assert!((0..3).all(|k| b.0[k] <= lo[k] && b.1[k] >= hi[k]), "{b:?}");
        assert!((0..3).any(|k| b.0[k] == rlo[k] || b.1[k] == rhi[k]), "no side moved to the reach: {b:?}");
        let (_, st) = mesh(&tree, &opts(24)).unwrap();
        let (_, given) = mesh(&tree, &MeshOptions { bounds: Some(b), ..opts(24) }).unwrap();
        assert_eq!((st.box_asks, given.box_asks, given.box_work), (asks, 0, 0));
        assert!(st.box_work > 0);
        // (The same, but for the bill of rounded shapes: the search's samples of them are the
        // build's, so their powers past their weight are charged and their spare powers pay
        // for later samples' ([`RoundedBill`]); each moves it by at most its most powers.)
        let most = ROUNDED_POWER * u64::from(ROUNDED_MOST) * (st.rounded_samples - given.rounded_samples);
        assert!(st.work.abs_diff(given.work + st.box_work) <= most, "{} {} {} {most}", st.work, given.work, st.box_work);
        // (Nothing to ask where the reach passes no side.)
        let ball = prim(Prim::Sphere { radius: 0.3 });
        assert_eq!(mesh_box_seeded(&ball, 48, 1), Some((bounds(&ball).map(|b| (b.0, b.1)).unwrap(), 0)));
    }

    /// The second review's adversarial search ([`the_search_spends_no_more_than_it_is_charged`]):
    /// `n` columns of eight sections whose halves trade places at every section (0.01 by 0.4,
    /// then 0.4 by 0.01), side by side, smoothly joined and turned.
    fn wedges(n: usize, square: f32) -> SdfNode {
        let column = |i: usize| {
            let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
            for (s, row) in sections.iter_mut().enumerate() {
                let (x, z) = if s % 2 == 0 { (0.01, 0.4) } else { (0.4, 0.01) };
                *row = [s as f32 / (MAX_SECTIONS - 1) as f32, x, z];
            }
            let a = [0.05 * i as f32, 0.0, 0.0];
            prim(Prim::Column { a, b: [a[0], 1.0, 0.3], side: [1.0, 0.0, 0.0], sections, count: MAX_SECTIONS as u8, square })
        };
        let mut t = group(Combine::Union, (0..n).map(column).collect(), 0.3);
        t.m.rotate = [20.0, 30.0, 40.0];
        t
    }

    /// What the box's search, [`reach`] and the mesher's ranges did ([`Did`], counted where
    /// each step is done), at the weights the charge is meant to pay, **each written out here
    /// as a number and not taken from the charging code**, so that a weight changed there (at
    /// its site or its constant) is changed here too, on purpose, or a test fails (review 4 of
    /// the box lane: eleven of these were the charging code's own constants, and halving one
    /// failed nothing). `tree` is the cost of one evaluation of the whole tree, for each at a
    /// box's middle. Each kind with its share, by name.
    fn did_shares(d: &Did, tree: u64) -> Vec<(&'static str, u64)> {
        // A primitive's evaluation, by `prim_kind` (as `prim_cost` and `ROUNDED_COST` have
        // them): a half space, a ball or capsule, a round cone, columns at squareness 2 or 4,
        // 3 and any other, a box, a torus, an ellipsoid, rounded shapes at power 2 and any
        // other, straight and tapered cylinders.
        const PRIM: [u64; PRIM_KINDS] = [48, 54, 74, 86, 170, 318, 60, 60, 60, 348, 1498, 60, 83];
        vec![
            ("range frames", d.range_nodes * 160),
            ("turned copies", d.mods[0] * 6),
            ("stretched copies", d.mods[1] * 3),
            ("repeated copies", d.mods[2] * 38),
            ("twisted copies", d.mods[3] * 65),
            ("bent copies", d.mods[4] * 23),
            ("displaced copies", d.mods[5] * 15),
            ("noise octaves", d.octaves * 45),
            ("primitives' ranges", d.range_prims.iter().zip(PRIM).map(|(n, w)| n * w).sum()),
            ("columns' ranges", d.column_ranges * 96),
            ("rounded shapes' ranges", d.rounded_ranges * 200),
            ("groups' ranges", d.groups * (2 * 8 + 200)),
            ("smooth joins", d.kids_smooth * 2 * 20),
            ("sharp joins", d.kids_sharp * 2 * 3),
            ("supports by a root", d.supports_root * 24),
            ("supports by powers", d.supports_pow * 260),
            ("pieces of curves", d.pieces * 600),
            ("evaluations at a middle", d.mid_evals * tree),
            ("nodes the reach asked", d.reach_nodes * 1200),
            ("children the reach gathered", d.reach_kids * 60),
            ("pairs the reach met", d.reach_pairs * 20),
            ("places of columns' tables", d.table_places * 6),
        ]
    }

    /// Trees whose box's search and reach do every kind of step: the drawn set's (every
    /// primitive, combination and modifier, stretched, twisted and bent), the second review's
    /// columns at squareness 2, 3 and 5, rounded shapes at powers 2 and 3 and as a disc,
    /// repeated, displaced, twisted and bent, and a column at squareness 1.5.
    fn charged_trees() -> Vec<SdfNode> {
        let mut d = Draw(0x51ed_270b_27d1_9a3f);
        let mut trees: Vec<SdfNode> = (0..40).map(|_| drawn_tree(&mut d, 0)).filter(|t| count_nodes(t) <= MAX_NODES && bounds(t).is_some()).collect();
        for square in [2.0, 3.0, 5.0] {
            trees.push(wedges(4, square));
        }
        let mut soft: Vec<SdfNode> = [(2.0, false), (3.0, false), (3.0, true)]
            .into_iter()
            .enumerate()
            .map(|(i, (power, disc))| {
                let mut t = prim(Prim::Rounded { half: [0.3, 0.2, 0.1], power, disc });
                t.m.at = [0.3 * i as f32, 0.0, 0.0];
                t.m.stretch = [2.0, 1.0, 0.5];
                t
            })
            .collect();
        soft[0].m.repeat = Some(([3, 1, 1], [0.4, 0.0, 0.0]));
        soft[1].m.displace = Some((0.01, 0.3, 2));
        let mut column = wedges(1, 1.5);
        column.m.bend = 20.0;
        soft.push(column);
        let mut t = group(Combine::Union, soft, 0.05);
        t.m.twist = 30.0;
        trees.push(t);
        // (A smooth union of many: the reach meets its children's pairs.)
        let mut many = group(Combine::Union, (0..40).map(|i| at_ball(0.1, [0.07 * i as f32, 0.0, 0.0])).collect(), 0.1);
        many.m.stretch = [1.0, 2.0, 1.0];
        trees.push(many);
        trees
    }

    fn at_ball(radius: f32, p: V3) -> SdfNode {
        let mut b = prim(Prim::Sphere { radius });
        b.m.at = p;
        b
    }

    /// **The box's search and its reach are charged every step they do, by kind** (third
    /// review of the search: with any one weight left out of the charge, every test still
    /// passed). Over [`charged_trees`], each step is counted where it is done ([`Did`]) and
    /// weighed here ([`did_shares`]); every kind is done; for each tree the charge covers what
    /// it did; and over all of them **the charge is what they did and less than any one kind's
    /// share more** (what is charged before it is known: a ball's whole range where it stopped
    /// early, a walk's or a table's most places), so a kind left out of the charge falls short.
    #[test]
    fn the_search_spends_no_more_than_it_is_charged() {
        let mut charged = 0u64;
        let mut sum = vec![0u64; did_shares(&Did::NONE, 0).len()];
        let mut searched = 0;
        for (i, tree) in charged_trees().iter().enumerate() {
            let cost = prepared_cost(&prepare(tree, 1, &mut 0));
            Did::taken();
            let mut paid = 0u64;
            let Some((_, spent)) = mesh_box_charged(tree, 24, 1, &mut |u| {
                paid += u;
                Ok(())
            })
            .unwrap() else {
                continue;
            };
            let d = Did::taken();
            searched += usize::from(spent.asks > 0);
            let shares = did_shares(&d, cost);
            let did: u64 = shares.iter().map(|s| s.1).sum();
            assert_eq!(paid, spent.work + spent.reach, "tree {i}");
            assert!(paid >= did, "tree {i}: charged {paid}, did {did}: {shares:?}");
            for (k, s) in shares.iter().enumerate() {
                sum[k] += s.1;
            }
            charged += paid;
        }
        let names: Vec<&str> = did_shares(&Did::NONE, 0).iter().map(|s| s.0).collect();
        let did: u64 = sum.iter().sum();
        let least = sum.iter().copied().min().unwrap();
        assert!(searched >= 10, "{searched} searches ran");
        assert!(least > 0, "a kind never done: {:?}", names.iter().zip(&sum).filter(|(_, s)| **s == 0).collect::<Vec<_>>());
        assert!(charged >= did && charged - did < least, "charged {charged}, did {did}, the least share {least}: {:?}", names.iter().zip(&sum).collect::<Vec<_>>());
    }

    /// **The reach alone is charged every step it does, and a column is walked once**: over
    /// [`charged_trees`] and a chain of 64 smooth unions of columns (8,191 asks), [`reach_charged`]
    /// is charged at least the nodes, children, pairs and table places it asked and the walks it
    /// took ([`did_shares`]), and no more than what a walk and a table charge before they are
    /// known; each column's curve is walked once, however many levels are asked of it.
    #[test]
    fn the_reach_is_charged_what_it_does() {
        let mut trees = charged_trees();
        trees.push(column_chain(64, 5.0));
        for (i, tree) in trees.iter().enumerate() {
            Did::taken();
            let mut paid = 0u64;
            let got = reach_charged(tree, 0.0, &mut |u| {
                paid += u;
                Ok(())
            })
            .unwrap();
            let d = Did::taken();
            assert_eq!(got, reach(tree, 0.0), "tree {i}");
            let did: u64 = did_shares(&d, 0).iter().map(|s| s.1).sum();
            assert!(paid >= did, "tree {i}: charged {paid}, did {did}: {d:?}");
            let columns = column_count(tree) as u64;
            // (And no more than that, past what a walk and a level of a table are charged
            // before they are known: a walk its most supports and pieces, at most 297,240
            // units for eight sections at a squareness that takes powers; a level its table's
            // most places, at most 2,052. A tree with no column is charged exactly what it did.)
            let ahead = columns * 297_240 + d.table_asks * 2_052;
            assert!(paid <= did + ahead, "tree {i}: charged {paid}, did {did}, at most {ahead} ahead: {d:?}");
            assert!(columns > 0 || paid == did, "tree {i}: no column, charged {paid}, did {did}");
            assert!(d.pieces <= columns * (MAX_SECTIONS as u64 - 1) * COLUMN_PIECES as u64, "tree {i}: {} pieces for {columns} columns", d.pieces);
            if i == trees.len() - 1 {
                assert_eq!(d.pieces, 64 * 7 * COLUMN_PIECES as u64, "each column walked once");
                assert!(d.reach_nodes > 8000, "{}", d.reach_nodes);
            }
        }
    }

    /// [`reach`] at level 0 with `asks` asks to spend, and the asks left.
    fn reach_asking(t: &SdfNode, asks: u32) -> (Option<Reach>, u32) {
        let mut free = |_: u64| Ok(());
        let mut cx = ReachCx { asks, tables: std::collections::BTreeMap::new(), spend: &mut free, failed: None };
        let got = reach_within(t, 0.0, true, &mut cx);
        (got, cx.asks)
    }

    fn column_count(n: &SdfNode) -> usize {
        match &n.kind {
            Kind::Prim(Prim::Column { .. }) => 1,
            Kind::Prim(_) => 0,
            Kind::Group { kids, .. } => kids.iter().map(column_count).sum(),
        }
    }

    /// A chain of `n` two-child smooth unions of eight-section columns, each turned: what makes
    /// [`reach`] ask the most (the third review's).
    fn column_chain(n: usize, square: f32) -> SdfNode {
        let column = |i: usize| {
            let mut t = wedges(1, square);
            let Kind::Group { kids, .. } = &mut t.kind else { unreachable!() };
            let mut c = kids.pop().unwrap();
            if let Kind::Prim(Prim::Column { a, b, .. }) = &mut c.kind {
                (a[0], b[0]) = (0.05 * i as f32, 0.05 * i as f32);
            }
            c
        };
        let mut chain = column(0);
        for i in 1..n {
            chain = group(Combine::Union, vec![column(i), chain], 0.05);
            chain.m.rotate = [5.0, 7.0, 9.0];
        }
        chain
    }

    /// **A cancel stops the reach at once**: raised at any of the reach's charges of a chain of
    /// 64 smooth unions of columns (the spender itself does not look at it), the reach ends
    /// with the cancel's error and nothing more is charged: it looks for a cancel before every
    /// charge, and between two charges does at most one node's own asking, one walk, one
    /// table's level or one union's pairs, each charged before it is done. Through [`mesh`],
    /// a work limit the reach passes is that limit's error, its measure no more than one such
    /// charge past it.
    #[test]
    fn a_cancel_or_the_limit_stops_the_reach_at_once() {
        let chain = column_chain(64, 5.0);
        let mut most = 0u64;
        let mut calls = 0u64;
        reach_charged(&chain, 0.0, &mut |u| {
            calls += 1;
            most = most.max(u);
            Ok(())
        })
        .unwrap();
        for at in [1, 2, calls / 3, calls - 1] {
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let _guard = crate::work::cancel_on(flag.clone());
            let (mut n, mut after) = (0u64, 0u64);
            let e = reach_charged(&chain, 0.0, &mut |u| {
                n += 1;
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    after += u;
                }
                if n == at {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(())
            })
            .expect_err("a cancelled reach ends");
            assert_eq!(e.message, crate::work::CANCELLED);
            assert_eq!((after, n), (0, at), "cancelled at charge {at} of {calls}");
        }
        let limit = 3_000_000;
        let e = mesh(&chain, &MeshOptions { max_work: limit, ..opts(16) }).unwrap_err();
        assert!(e.limit && e.message.contains("work units"), "{e:?}");
        let measured = e.measured.unwrap() as u64;
        assert!(measured > limit && measured <= limit + most, "{measured} (one charge at most {most})");
    }

    /// What [`Did`] holds so far on this thread, weighed ([`did_shares`]), without starting it
    /// again.
    fn did_so_far(tree: u64) -> u64 {
        let d = DID.with(|d| *d.borrow());
        did_shares(&d, tree).iter().map(|s| s.1).sum()
    }

    /// **Every step is charged before it is done** (review 4 of the box lane, finding 3: a
    /// walk or a union's pairs charged after them passed every test). At every charge of the
    /// reach, of the walk of a tree's columns ([`outline_columns`]) and of the box's search,
    /// what has been done so far ([`did_shares`]) is no more than what was charged before
    /// that charge: so nothing is done ahead of its charge. (The search is held so on trees
    /// with no repetition: a ball that meets more than one copy is charged the rest after its
    /// walk, by design: [`clear_of_solid`].) And **a charge refused stops the work it was
    /// for**: refused at any of the reach's charges, nothing more is done after it.
    #[test]
    fn every_step_is_charged_before_it_is_done() {
        let mut trees = charged_trees();
        trees.push(column_chain(16, 5.0));
        let mut searched = 0;
        for (i, tree) in trees.iter().enumerate() {
            let cost = prepared_cost(&prepare(tree, 1, &mut 0));
            let check = |what: &'static str| {
                let mut paid = 0u64;
                let mut calls = 0u64;
                Did::taken();
                move |u: u64| -> Result<(), SdfError> {
                    let did = did_so_far(cost);
                    assert!(did <= paid, "tree {i}, {what}: {did} done before charge {calls}, {paid} charged");
                    paid += u;
                    calls += 1;
                    Ok(())
                }
            };
            reach_charged(tree, 0.0, &mut check("the reach")).unwrap();
            let mut p = prepare(tree, 1, &mut 0);
            outline_columns(&mut p, &mut check("the walk")).unwrap();
            if !has_repeat(tree) {
                let got = mesh_box_charged(tree, 24, 1, &mut check("the search")).unwrap();
                searched += usize::from(got.is_some_and(|(_, s)| s.asks > 0));
            }
            // Refused at its k-th charge (a limit), the reach does nothing more.
            let mut calls = 0u64;
            reach_charged(tree, 0.0, &mut |_| {
                calls += 1;
                Ok(())
            })
            .unwrap();
            let mut ats = vec![1, 2, calls / 2, calls];
            ats.retain(|at| (1..=calls).contains(at));
            ats.dedup();
            for at in ats {
                Did::taken();
                let mut n = 0u64;
                let mut at_refusal = None;
                let e = reach_charged(tree, 0.0, &mut |_| {
                    n += 1;
                    if n == at {
                        at_refusal = Some(DID.with(|d| *d.borrow()));
                        return Err(SdfError::limit("refused", 0.0, 0.0));
                    }
                    Ok(())
                });
                assert_eq!(e.map(|_| ()).unwrap_err().message, "refused", "tree {i}: refused at charge {at} of {calls}");
                assert_eq!(Some(DID.with(|d| *d.borrow())), at_refusal, "tree {i}: work done after charge {at} of {calls} was refused");
            }
        }
        assert!(searched >= 5, "{searched} searches ran");
    }

    /// **The mesher walks each column once, and is charged its ranges** (review 4 of the box
    /// lane, item M: the mesher's own tree was never outlined, so a column's range walked its
    /// curve again at every block it was asked of, charged as eight evaluations where a walk is
    /// 43,800 to 297,240 units). Three columns whose breadth and depth trade places along them,
    /// one joined with a flat ellipsoid, meshed in a box given (no reach, no search) and as
    /// wide again as they are each way: blocks far from them ask the tree's range, and every
    /// prune asks the range of the loose union, so the columns' ranges are asked many times;
    /// each curve is walked once ([`outline_columns`]), and what the ranges and the walks did
    /// ([`did_shares`]) is in the work, as [`SdfStats::range_work`].
    #[test]
    fn the_mesher_walks_each_column_once() {
        let column = |x: f32| {
            let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
            sections[0] = [0.0, 0.1, 0.05];
            sections[1] = [1.0, 0.05, 0.1];
            prim(Prim::Column { a: [x, 0.0, 0.0], b: [x, 1.0, 0.0], side: [1.0, 0.0, 0.0], sections, count: 2, square: 3.0 })
        };
        let mut flat = prim(Prim::Ellipsoid { radii: [0.3, 0.05, 0.2] });
        flat.m.at = [0.0, 0.5, 0.0];
        let tree = group(Combine::Union, vec![group(Combine::Union, vec![flat, column(0.2)], 0.0), column(-0.3), column(0.6)], 0.0);
        let p = prepare(&tree, 1, &mut 0);
        assert!(p.wander > 0.0 && p.loose, "its ranges are asked");
        let (lo, hi, _, _) = bounds(&tree).unwrap();
        let size = math3::sub(hi, lo);
        let (lo, hi) = (math3::sub(lo, size), math3::add(hi, size));
        for res in [24, 48, 96] {
            Did::taken();
            let (_, st) = mesh(&tree, &MeshOptions { bounds: Some((lo, hi)), ..opts(res) }).unwrap();
            let d = Did::taken();
            assert_eq!((st.reach_work, st.box_work), (0, 0));
            assert_eq!(d.pieces, 3 * COLUMN_PIECES as u64, "res {res}: each column walked once");
            assert!(d.column_ranges > 3 * 20, "res {res}: {} columns' ranges asked", d.column_ranges);
            let did: u64 = did_shares(&d, 0).iter().map(|s| s.1).sum();
            assert!(st.range_work >= did && st.range_work < st.work, "res {res}: ranges charged {}, did {did}, of {}", st.range_work, st.work);
        }
    }

    /// **A prune takes each loose child's range once** (review 2 of the mesher lane, finding
    /// 1: at each kept union the range was taken over the whole loose child, then the
    /// recursion walked its loose children again, so nested loose unions cost their depth
    /// times their size). Sixteen nested unions, each of the last, a flat ellipsoid and a few
    /// balls, pruned over balls of every size about points through it: no node's range is
    /// taken more than once a prune ([`Did`]'s frames: at most the tree's nodes), and the
    /// pruned tree is the whole tree, bit for bit, through each ball.
    #[test]
    fn a_prune_takes_each_loose_childs_range_once() {
        let mut t = prim(Prim::Sphere { radius: 0.03 });
        for level in 0..16 {
            let x = 0.06 * level as f32;
            let mut e = prim(Prim::Ellipsoid { radii: [0.08, 0.02, 0.08] });
            e.m.at = [x, 0.0, 0.0];
            let mut kids = vec![t, e];
            for b in 0..3 {
                let mut s = prim(Prim::Sphere { radius: 0.01 });
                s.m.at = [x + 0.02 * b as f32, 0.03, 0.0];
                kids.push(s);
            }
            t = group(Combine::Union, kids, 0.0);
        }
        let nodes = count_nodes(&t) as u64;
        let p = prepare(&t, 1, &mut 0);
        let mut r = Review(0x7e57_0f_0e5e_1234);
        let mut asked = 0;
        for _ in 0..200 {
            let c = [r.range(-0.1, 1.0), r.range(-0.05, 0.06), r.range(-0.08, 0.08)];
            let reach = [0.005, 0.02, 0.08, 0.3][r.pick(4)];
            Did::taken();
            let here = prune_asking(&p, c, reach, &mut Spent::default());
            let d = Did::taken();
            assert!(d.range_nodes <= nodes, "{} ranges taken of {nodes} nodes about {c:?}, {reach}", d.range_nodes);
            asked += usize::from(d.range_nodes > 0);
            for _ in 0..8 {
                let q = math3::add(c, math3::scale(math3::normalize(r.v3(-1.0, 1.0)), reach * r.unit()));
                assert_eq!(eval(&here, q).to_bits(), eval(&p, q).to_bits(), "about {c:?}, {reach}, at {q:?}");
            }
        }
        assert!(asked > 100, "{asked} prunes asked a range");
    }

    /// **A cancelled sculpt gives one error, wherever the cancel is seen** (review 4 of the box
    /// lane, finding 1: the reach said "0 of a limit of 0", the search its work against 0, the
    /// build its work against its limit). A mesh of columns of changing proportions, whose
    /// reach, box's search, walk of its columns for the mesher's ranges and mesher all charge,
    /// is cancelled at forty of its charges and at the first and last of each part, the cancel
    /// raised before the charge looks for it and after (so that the reach, the search and the
    /// walk see it themselves): every one ends with [`SdfError::cancelled`], no measure and no
    /// limit.
    #[test]
    fn a_cancel_gives_one_error_wherever_it_is_seen() {
        let tree = wedges(4, 5.0);
        let o = MeshOptions { max_work: u64::MAX, ..opts(32) };
        let charges = |at: Option<(u64, bool)>| -> (Result<(), SdfError>, u64) {
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let _guard = crate::work::cancel_on(flag);
            CANCEL_AT.with(|c| c.set(Some(at.unwrap_or((u64::MAX, true)))));
            let got = mesh(&tree, &o).map(|_| ());
            let left = CANCEL_AT.with(|c| c.replace(None)).unwrap().0;
            (got, u64::MAX - left)
        };
        let (whole, n) = charges(None);
        whole.unwrap();
        // Where each part's charges end: the reach's, the search's (its walks and balls), then
        // the mesher's walk of its columns, then the mesher's own.
        let mut reach = 0u64;
        reach_charged(&tree, 0.0, &mut |_| {
            reach += 1;
            Ok(())
        })
        .unwrap();
        let mut search = 0u64;
        let (_, spent) = mesh_box_charged(&tree, 32, 1, &mut |_| {
            search += 1;
            Ok(())
        })
        .unwrap()
        .unwrap();
        let p = prepare(&tree, 1, &mut 0);
        assert!(spent.asks > 0 && p.wander > 0.0, "the search runs and the mesher asks ranges");
        let walks = column_count(&tree) as u64;
        assert!(n > search + walks + 40, "{n} charges, {search} before the mesher");
        let mut points: Vec<u64> = (1..=40).map(|k| 1 + k * (n - 2) / 41).collect();
        points.extend([1, reach, reach + 1, search, search + 1, search + walks, search + walks + 1, n - 1]);
        for at in points {
            for before in [true, false] {
                let (got, _) = charges(Some((at, before)));
                assert_eq!(got, Err(SdfError::cancelled()), "cancelled at charge {at} of {n} ({}; the reach's end at {reach}, the search's at {search})", if before { "before its check" } else { "after it" });
            }
        }
    }

    /// **A cancel stops the search within a ball**, and **a build's work limit within one
    /// charge**: a cancel raised at the search's tenth charge after its reach and its sixteen
    /// columns' walks
    /// (the spender itself does not look at it) ends it with the cancel's error before more
    /// than one ball's range at every copy and one evaluation of the tree are charged after
    /// it; raised at its first (a column's walk), before another walk. Through [`mesh`], a
    /// limit the search passes is that limit's error, its measure no more than one charge (a
    /// ball's range or a walk) past the limit, where the whole search charges over twice it.
    #[test]
    fn a_cancel_or_the_limit_stops_the_search_within_a_ball() {
        let tree = wedges(16, 5.0);
        let mut p = prepare(&tree, 1, &mut 0);
        outline_columns(&mut p, &mut |_| Ok(())).unwrap();
        let ball = RANGE_COPIES as u64 * range_cost(&p) + prepared_cost(&p);
        let walk = column_walk_cost(MAX_SECTIONS, 5.0);
        // (Its first charges are its reach's, whose own cancel is
        // `a_cancel_or_the_limit_stops_the_reach_at_once`; then its sixteen columns' walks.)
        let mut reached = 0u64;
        reach_charged(&tree, 0.0, &mut |_| {
            reached += 1;
            Ok(())
        })
        .unwrap();
        for (at, most) in [(reached + 16 + 10, ball), (reached + 1, walk)] {
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let _guard = crate::work::cancel_on(flag.clone());
            let (mut calls, mut after) = (0u64, 0u64);
            let got = mesh_box_charged(&tree, 64, 1, &mut |u| {
                calls += 1;
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    after += u;
                }
                if calls == at {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(())
            });
            let e = got.expect_err("a cancelled search ends");
            assert_eq!(e.message, crate::work::CANCELLED);
            assert!(after <= most, "cancelled at charge {at}: {after} charged after, at most {most}");
        }
        // (Without a cancel the same search charges far more than this limit.)
        let limit = 6_000_000;
        let e = mesh(&tree, &MeshOptions { max_work: limit, ..opts(64) }).unwrap_err();
        assert!(e.limit && e.message.contains("work units"), "{e:?}");
        let measured = e.measured.unwrap() as u64;
        assert!(measured > limit && measured <= limit + ball.max(walk), "{measured}");
        let (_, whole) = mesh_box_charged(&tree, 64, 1, &mut |_| Ok(())).unwrap().unwrap();
        assert!(whole.work > 2 * limit, "{}", whole.work);
    }

    /// **The powers rounded shapes take in the box's search are the build's** (second review
    /// of the search, finding 2: the bill was started after the search, emptying what it had
    /// taken): a twisted rounded box, whose search runs, is billed for more samples than the
    /// same grid in the box the search gave, which asks nothing of the field first.
    #[test]
    fn a_rounded_shapes_search_is_billed() {
        let mut t = prim(Prim::Rounded { half: [0.3, 0.2, 0.1], power: 3.0, disc: false });
        t.m.stretch = [3.0, 1.0, 0.5];
        t.m.rotate = [10.0, 20.0, 30.0];
        let mut d = group(Combine::Union, vec![t, prim(Prim::Sphere { radius: 0.05 })], 0.05);
        d.m.twist = 30.0;
        let (_, st) = mesh(&d, &opts(24)).unwrap();
        let b = mesh_box(&d, 24).unwrap();
        let (_, given) = mesh(&d, &MeshOptions { bounds: Some(b), ..opts(24) }).unwrap();
        assert!(st.box_asks > 0 && given.box_asks == 0, "the search ran");
        assert!(st.rounded_samples > given.rounded_samples, "{} samples billed, {} without the search", st.rounded_samples, given.rounded_samples);
        assert!(st.work >= given.work + st.box_work, "{} {} {}", st.work, given.work, st.box_work);
    }

    /// Why an ordinary tree's box grows ([`an_ordinary_sculpt_is_meshed_in_the_old_box`]).
    enum Grows {
        /// The reach is looser than the solid by this many cells at most, and sound, and the
        /// search of the field ([`clear_of_solid`]) could not show the slab past the old box
        /// clear: its budget ran out (repetitions, whose copies a ball can meet several of; a
        /// column whose field changes several times faster than the distance), or a rounded
        /// shape is trusted only to within its slack. The mesh
        /// in a box much larger reaches no further than the old box: nothing was cut off, and
        /// the box is that much larger. Two causes, each where a part's level set really
        /// does reach past what `bounds` says but the solid is held in by what the part is
        /// joined to or met with, which a box cannot tell: an ellipsoid or a column (a flat
        /// one most) at a level above nothing (under a join, a swelling, a shell or noise),
        /// with a turn above it (`bounds` widens by the level after the turn as if the part
        /// were exact; the reach by what its field may read short, a ball of `(rho + 1 /
        /// rho) / 2` times the level for an ellipsoid, the ratio of its halves for a
        /// column); and a meeting swollen, shelled or noised and then turned (a meeting of
        /// parts keeps no ball, since the level set of a meeting of balls is not within a
        /// ball of the meeting, so its box is turned with the level in it).
        Looser(f32),
        /// `bounds`'s box cuts the solid off: the mesh in a box much larger reaches past it
        /// by more than half a cell, and not past the mesher's box. A flat ellipsoid or column
        /// swollen: its field reads short across its broad side, so its swelling reaches
        /// further there than `bounds` widens (the old box's second fault, besides a stretch).
        Cuts,
    }

    impl Grows {
        fn most_cells(&self) -> f32 {
            match self {
                Grows::Cuts => f32::INFINITY,
                Grows::Looser(cells) => *cells,
            }
        }

        fn shown(&self, tree: &SdfNode, res: u32) {
            let (lo, hi, _, _) = bounds(tree).unwrap();
            let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
            match self {
                Grows::Looser(_) | Grows::Cuts => {
                    // (A box half its size larger on every side than the mesher's, at a cell
                    // no coarser than the old box's.)
                    let (glo, ghi) = mesh_box(tree, res).unwrap();
                    let gsize = (0..3).map(|k| ghi[k] - glo[k]).fold(0.0f32, f32::max);
                    let big = (math3::sub(glo, [0.5 * gsize; 3]), math3::add(ghi, [0.5 * gsize; 3]));
                    let cell = size / res as f32;
                    let fine = (crate::dmath::ceil(2.0 * gsize / cell) as u32).min(MAX_RESOLUTION);
                    // (Twenty times the limit, from eight: where a tree's factor bounds
                    // nothing, a block is filled only when its range shows it clear
                    // (DISTANCE_FIELDS 2.7), and in this wide box `own` 63 (a repetition, a flat
                    // ellipsoid and columns of changing proportions) has no block its range
                    // shows clear: 23.8 thousand million units, from 10.6.)
                    let (shape, _) = mesh(tree, &MeshOptions { bounds: Some(big), max_work: MAX_WORK * 20, max_triangles: 3_000_000, ..opts(fine) }).unwrap();
                    let past = |lo: V3, hi: V3| shape.positions.iter().map(|p| (0..3).map(|a| (lo[a] - p[a]).max(p[a] - hi[a])).fold(f32::MIN, f32::max)).fold(f32::MIN, f32::max);
                    if matches!(self, Grows::Cuts) {
                        assert!(past(lo, hi) > 0.5 * cell && past(glo, ghi) <= 0.25 * cell, "past the old box {}, past the new {} (cell {cell})", past(lo, hi), past(glo, ghi));
                    } else {
                        assert!(past(lo, hi) <= 0.0, "the solid passes the old box by {}", past(lo, hi));
                    }
                }
            }
        }
    }

    /// The ordinary trees whose box grows: which set, which tree, at which resolution, and why.
    /// Of the reviewer's 600 boxes, 70 grew at the merge reviewed, 3 at the round after, and
    /// none since the field is searched; of this crate's own 600, 14: 4 where `bounds` cuts
    /// the solid off, 10 (five trees) where the search could not show the slab clear (it was 27
    /// before the search, two of them an empty solid's).
    const ORDINARY_THAT_GROW: &[(&str, usize, u32, Grows)] = &[
        ("own", 37, 24, Grows::Cuts),
        ("own", 37, 64, Grows::Cuts),
        ("own", 63, 24, Grows::Looser(3.9)),
        ("own", 63, 64, Grows::Looser(10.3)),
        ("own", 101, 24, Grows::Looser(0.8)),
        ("own", 101, 64, Grows::Looser(2.2)),
        ("own", 128, 24, Grows::Looser(0.7)),
        ("own", 128, 64, Grows::Looser(1.8)),
        ("own", 150, 24, Grows::Looser(6.0)),
        ("own", 150, 64, Grows::Looser(15.9)),
        ("own", 226, 24, Grows::Cuts),
        ("own", 226, 64, Grows::Cuts),
        ("own", 296, 24, Grows::Looser(8.6)),
        ("own", 296, 64, Grows::Looser(22.9)),
    ];

    /// The review's smallest cases, by name: five balls in a row joined smoothly and turned
    /// (and two to sixteen, turned three ways); two tapering columns in a near-union at
    /// `smooth` 0.088; an ellipsoid, a rounded box and a column in a near-union at 0.081.
    /// Each was meshed in a larger box than `bounds`'s with nothing in it past the old one.
    #[test]
    fn the_reviews_smallest_cases_keep_the_old_box() {
        let ball = |at: V3| {
            let mut b = prim(Prim::Sphere { radius: 0.2 });
            b.m.at = at;
            b
        };
        let mut cases = Vec::new();
        for n in [2usize, 3, 4, 5, 8, 16] {
            for turn in [[0.0f32, 0.0, 0.0], [0.0, 0.0, 45.0], [30.0, 40.0, 50.0]] {
                let mut t = group(Combine::Union, (0..n).map(|i| ball([0.3 * i as f32, 0.0, 0.0])).collect(), 0.15);
                t.m.rotate = turn;
                cases.push((format!("{n} balls turned {turn:?}"), t));
            }
        }
        let column = |a: V3, b: V3, side: V3, s: [[f32; 3]; 3]| {
            let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
            sections[..3].copy_from_slice(&s);
            prim(Prim::Column { a, b, side, sections, count: 3, square: 2.0 })
        };
        cases.push((
            "two tapering columns, near 0.088".into(),
            group(
                Combine::Near,
                vec![
                    column([-0.3394815, 0.13028952, -0.33863646], [0.1269143, 0.35881534, -0.049892098], [0.85624623, -0.8154814, 0.5302607], [[0.0, 0.18737583, 0.16274597], [0.5, 0.040966567, 0.18978278], [1.0, 0.18777661, 0.08705926]]),
                    column([0.0505991, 0.02071762, 0.086868584], [-0.09891945, 0.11914894, -0.27209496], [0.56318593, 0.35824, 0.29844475], [[0.0, 0.13090017, 0.079004146], [0.5, 0.1908344, 0.103158005], [1.0, 0.0330021, 0.11640041]]),
                ],
                0.088469304,
            ),
        ));
        cases.push((
            "an ellipsoid, a rounded box and a column, near 0.081".into(),
            group(
                Combine::Near,
                vec![
                    prim(Prim::Ellipsoid { radii: [0.18123971, 0.140853, 0.17535011] }),
                    prim(Prim::Box { half: [0.23724872, 0.28834462, 0.3449662], round: 0.20483322 }),
                    column([-0.0075882077, 0.105427295, 0.08167881], [-0.18455629, 0.053816497, -0.1315875], [-0.3342123, -0.07776892, -0.033371568], [[0.0, 0.13715586, 0.09585944], [0.5, 0.100378335, 0.06787355], [1.0, 0.1622397, 0.18713838]]),
                ],
                0.081062414,
            ),
        ));
        for (name, t) in &cases {
            let (lo, hi, _, _) = bounds(t).unwrap();
            for res in [24, 48, 64, MAX_RESOLUTION] {
                assert_eq!(mesh_box(t, res), Some((lo, hi)), "{name} at {res}: the box grew {} cells", grown_cells(t, res));
            }
        }
    }

    /// How many nodes [`reach`] asks: never near [`REACH_ASKS`] for a tree of [`MAX_NODES`],
    /// whatever its shape (a chain of two-child smooth unions as deep as the nodes allow, a
    /// chain of eight-child ones, a balanced tree of pairs, one union of them all; the review
    /// found a chain of pairs 16 deep spent the whole budget when every ask asked three more).
    #[test]
    fn reach_asks_a_bounded_number_of_times() {
        let ball = || prim(Prim::Sphere { radius: 0.1 });
        let chain = |width: usize, depth: usize| {
            let mut t = ball();
            for i in 0..depth {
                let mut kids: Vec<SdfNode> = (1..width).map(|_| ball()).collect();
                kids.push(t);
                t = group(Combine::Union, kids, 0.05);
                t.m.rotate = [10.0 * i as f32, 20.0, 30.0];
            }
            t
        };
        fn pairs(depth: u32) -> SdfNode {
            if depth == 0 {
                return prim(Prim::Sphere { radius: 0.1 });
            }
            let mut t = group(Combine::Near, vec![pairs(depth - 1), pairs(depth - 1)], 0.05);
            t.m.at = [0.05, 0.0, 0.0];
            t
        }
        let trees = [chain(2, 63), chain(8, 15), chain(2, 16), pairs(6), group(Combine::Union, (0..MAX_NODES - 1).map(|_| ball()).collect(), 0.05)];
        for t in &trees {
            assert!(count_nodes(t) <= MAX_NODES);
            let (got, asks) = reach_asking(t, REACH_ASKS);
            let got = got.unwrap();
            let used = REACH_ASKS - asks;
            assert!(used <= 8_400, "{} nodes asked {used} times", count_nodes(t));
            // (So every union's second answer was asked for, and the answer is the one the
            // budget allows in full.)
            assert_eq!(Some(got), reach(t, 0.0));
        }
    }

    /// **Past [`REACH_ASKS`], the reach is still sound**: with no asks left, every smooth
    /// union answers with its first answer only (its children at its level and its greatest
    /// dip), which holds the solid as well (the drawn trees' truth, by the field alone, in
    /// the box that answer gives); it is never tighter than the full answer; and for ordinary
    /// chains of joins it is still `bounds`'s box (the first answer keeps each join's dip as a
    /// ball, which a turn leaves as it is).
    #[test]
    fn past_the_ask_budget_the_reach_is_still_sound() {
        let spent = |t: &SdfNode| reach_asking(t, 0).0.filter(|r| !r.is_nothing()).map(|r| r.within).or_else(|| bounds(t).map(|b| (b.0, b.1)));
        let mut d = Draw(0x6c62_272e_07bb_0142);
        let mut checked = 0;
        for i in 0..160 {
            let tree = drawn_tree(&mut d, 0);
            if count_nodes(&tree) > MAX_NODES || bounds(&tree).is_none() {
                continue;
            }
            let (full, cheap) = (reach(&tree, 0.0).unwrap(), reach_asking(&tree, 0).0.unwrap());
            if !full.is_nothing() {
                assert!((0..3).all(|k| cheap.within.0[k] <= full.within.0[k] && cheap.within.1[k] >= full.within.1[k]), "tree {i}: the budget's answer is tighter than the full one");
            }
            let Some((out, at)) = escape(&tree, &mut d, spent) else { continue };
            assert!(out == 0.0, "tree {i}: the field is at or below nothing {out} m outside the budget's box, at {at:?}");
            checked += 1;
        }
        assert!(checked > 100, "{checked}");
        // Ordinary chains of joins, turned at every level: no asks left, and the box still
        // `bounds`'s, to a quarter of a cell at the finest resolution.
        for (width, depth) in [(2, 16), (2, 40), (5, 10)] {
            let mut t = prim(Prim::Sphere { radius: 0.2 });
            for i in 0..depth {
                let mut kids: Vec<SdfNode> = (1..width).map(|j| {
                    let mut b = prim(Prim::Sphere { radius: 0.2 });
                    b.m.at = [0.3 * j as f32, 0.0, 0.0];
                    b
                }).collect();
                kids.push(t);
                t = group(Combine::Union, kids, 0.1);
                t.m.rotate = [7.0 * i as f32, 40.0, 50.0];
            }
            let (lo, hi, _, _) = bounds(&t).unwrap();
            let (rlo, rhi) = reach_asking(&t, 0).0.unwrap().within;
            let close = 0.25 * cell_of(lo, hi);
            assert!((0..3).all(|k| rlo[k] >= lo[k] - close && rhi[k] <= hi[k] + close), "{width} wide, {depth} deep: {rlo:?}..{rhi:?} passes {lo:?}..{hi:?}");
            assert_eq!(mesh_box(&t, MAX_RESOLUTION), Some((lo, hi)));
        }
    }

    /// **The quarter cell** ([`mesh_box`]): a side moves only where the reach passes
    /// [`bounds`]'s box by more than a quarter of a cell, and nothing within that is lost. A
    /// ball stretched two to one along x and swollen by `s` reaches exactly `s` further along
    /// x than `bounds` says (its field reads half the distance there), and the reach says
    /// exactly that; swept from nothing to two cells, the box moves only past a quarter cell,
    /// and the mesh reaches the surface (within a tenth of a cell) at every step.
    #[test]
    fn the_box_moves_only_past_a_quarter_cell_and_loses_nothing_within_it() {
        let res = 40;
        let mut moved_at = None;
        for step in 0..=40 {
            let mut t = prim(Prim::Sphere { radius: 0.1 });
            t.m.stretch = [2.0, 1.0, 1.0];
            // (The cell is the box's longest side, 0.4 + 2 s, over the resolution.)
            let share = 0.02 + 0.05 * step as f32;
            let s = share * 0.4 / (res as f32 - 2.0 * share);
            t.m.round = s;
            let (lo, hi, _, _) = bounds(&t).unwrap();
            let cell = (hi[0] - lo[0]) / res as f32;
            assert!((hi[0] - (0.2 + s)).abs() < 1e-6 && ((s - share * cell) / cell).abs() < 1e-4);
            let truth = surface_along_x(&t, [0.0; 3], 1.0);
            assert!((truth - (0.2 + 2.0 * s)).abs() < 1e-4, "{truth}");
            let (blo, bhi) = mesh_box(&t, res).unwrap();
            let moved = (blo, bhi) != (lo, hi);
            assert_eq!(moved, share > 0.25, "{share} of a cell past `bounds`: moved {moved}");
            if moved {
                moved_at.get_or_insert(share);
                assert!(bhi[0] >= truth, "the box moved to {} short of the surface at {truth}", bhi[0]);
            }
            let (shape, _) = mesh(&t, &opts(res)).unwrap();
            let reached = shape.bounds().max[0];
            assert!((reached - truth).abs() <= 0.1 * cell, "{share} of a cell past: the mesh ends at {reached}, the surface at {truth} (cell {cell})");
        }
        assert!(moved_at.is_some_and(|s| s <= 0.3));
    }

    /// **Every sculpt is a closed surface** ([`not_a_surface`], from positions and indices
    /// alone): every sculpt of the repository's recipes, at its detail and coarsely; the cases
    /// thinner than a cell ([`thin_cases`]) at resolutions from 8 to 157; and trees drawn from
    /// a fixed sequence at resolutions from 8 to 160. Each sheet through a cell has a vertex of
    /// its own, a face of four crossings is paired by the face alone, and a thread through a
    /// face has vertices on it; take any of the three away and this fails.
    #[test]
    fn every_sculpt_is_a_closed_surface() {
        let mut checked = 0;
        for (name, meshes) in repository_sculpts() {
            for (i, (s, st)) in meshes.iter().enumerate() {
                assert_eq!(not_a_surface(s), None, "{name}, mesh {i}");
                assert_eq!(st.tangled_edges, 0, "{name}, mesh {i}");
                checked += 1;
            }
        }
        assert!(checked >= 40, "{checked} repository meshes");
        let (mut thin, mut threads) = (0, 0);
        for (name, tree, res) in thin_cases() {
            // (A part thinner than a cell can fall between the samples: nothing to mesh.)
            let (s, st) = match mesh(&tree, &MeshOptions { max_triangles: 4_000_000, ..opts(res) }) {
                Err(e) if !e.limit && e.message.contains("no surface") => continue,
                got => got.unwrap_or_else(|e| panic!("{name} at {res}: {e:?}")),
            };
            assert_eq!(not_a_surface(&s), None, "{name} at resolution {res}");
            assert_eq!(st.tangled_edges, 0, "{name} at resolution {res}");
            thin += usize::from(st.thin_cells > 0);
            threads += usize::from(st.threads > 0);
        }
        // (The cases are thin where they mean to be, and threads are met.)
        assert!(thin > 35 && threads > 15, "{thin} cases with thin cells, {threads} with threads");
        let mut d = Draw(0x51ed_270b_27d1_9a3f);
        let (mut meshed, mut drawn_thin) = (0, 0);
        const RES: [u32; 10] = [8, 11, 16, 24, 33, 48, 64, 90, 120, 160];
        for t in 0..100 {
            let tree = drawn_tree(&mut d, 0);
            if count_nodes(&tree) > MAX_NODES || bounds(&tree).is_none() {
                continue;
            }
            for res in [RES[t % 10], RES[(t + 3) % 5]] {
                let Ok((s, st)) = mesh(&tree, &MeshOptions { max_triangles: 4_000_000, ..opts(res) }) else { continue };
                assert_eq!(not_a_surface(&s), None, "drawn tree {t} at resolution {res}\n{tree:?}");
                meshed += 1;
                drawn_thin += usize::from(st.thin_cells > 0);
            }
        }
        assert!(meshed > 100 && drawn_thin > 30, "{meshed} drawn meshes, {drawn_thin} thin");
        // (Two trees a wider run found: the 999th of another sequence at resolution 11, two
        // cells' vertices held at one corner by their cells' bounds and joined by the weld,
        // with no sheet anywhere; and its 1,415th at 90, the same among sheets.)
        let mut d = Draw(0x1234_5678_9abc_def1);
        for t in 0..1415 {
            let tree = drawn_tree(&mut d, 0);
            if t == 998 || t == 1414 {
                let (s, _) = mesh(&tree, &MeshOptions { max_triangles: 4_000_000, ..opts(if t == 998 { 11 } else { 90 }) }).unwrap();
                assert_eq!(not_a_surface(&s), None, "the wider run's tree {t}");
            }
        }
    }

    /// The meshes of the repository's recipes that had no cell with more than one sheet (and
    /// no face of four crossings), as the mesher made them before each sheet had a vertex of
    /// its own: (recipe, which of its build's meshes, every byte's hash, [`mesh_hash`]).
    const ONE_SHEET_RECIPES: [(&str, usize, &str); 30] = [
        ("grave_rock", 1, "b3425e9306f46ef9"), ("ruined_wall", 0, "936dbc9feea1e333"), ("ruined_wall", 1, "5bcbb49f5122b839"), ("statue_angel", 1, "ae067237c67a5a9f"),
        ("statue_mourner", 1, "8a5d975be0360bc2"), ("runestone_t3", 0, "5b13aaed48ed668d"), ("runestone_t3", 1, "c49e9c661bd58480"), ("runestone_t3", 2, "6472fc1385d5ae03"),
        ("runestone_t3", 4, "55c11bbff83bdb1d"), ("runestone_t3", 6, "f70a1394252a25e2"), ("runestone_t3", 7, "248553abb9c1dcab"), ("skeleton_warrior_t3", 1, "ec550b6191c9989a"),
        ("skeleton_warrior_t3", 2, "8f45668f4dfad2e6"), ("skeleton_warrior_t3", 4, "a68a6006a1e3de38"), ("skeleton_warrior_t3", 5, "7a9dc9c187b77b84"), ("skeleton_warrior_t3", 6, "d363b2fcf8d336be"),
        ("skeleton_warrior_t3", 7, "f0ff0425eb7095ee"), ("skeleton_warrior_t3", 8, "762416c432bf99db"), ("skeleton_warrior_t3", 9, "fa1fd2d70aec8cff"), ("skeleton_warrior_t3", 10, "3991361069e4897d"),
        ("skeleton_warrior_t3", 12, "a1a3921ddd32d4dc"), ("skeleton_warrior_t3", 13, "aa6233f6e9c363a0"), ("skeleton_warrior_t3", 15, "0a12156eadb1ab7d"), ("skeleton_warrior_t3", 16, "1d825141abf06d9b"),
        ("skeleton_warrior_t3", 17, "7912d6fa9c3ea8c7"), ("skeleton_warrior_t3", 18, "373be0fedf113e1b"), ("skeleton_warrior_t3", 19, "75fe5519dd514cf9"), ("skeleton_warrior_t3", 20, "b804ee5e8dc2512a"),
        ("skeleton_warrior_t3", 21, "d77f46649acda6e5"), ("tree_t3", 1, "395873a1a209c88d"),
    ];

    /// The figure's parts for each body and shaping given, meshed at each part's resolution.
    fn figure_meshes(bodies: &[(specula_corpus::Sex, usize)], shapings: &[(f64, f64, f32)]) -> Vec<(Shape, SdfStats)> {
        let mut out = Vec::new();
        for &(sex, which) in bodies {
            let [short, tall] = specula_corpus::ansur::statures(sex).map(|mm| mm / 1000.0);
            let stature = [short, 0.5 * (short + tall), tall][which];
            let body = specula_corpus::Body::with_build(sex, stature, 0.0).unwrap();
            for &(arms, legs, smooth) in shapings {
                let shaping = crate::figure::Shaping { arms, legs, smooth };
                for name in crate::figure::Part::NAMES {
                    let part = crate::figure::Part::parse(name).unwrap();
                    let tree = crate::figure::tree(&body, &shaping, part);
                    out.extend(mesh(&tree, &MeshOptions { max_triangles: 2_000_000, ..opts(part.resolution()) }));
                }
            }
        }
        out
    }

    /// Trees drawn from a fixed sequence, each at five resolutions from 8 to 90.
    fn drawn_meshes() -> Vec<(Shape, SdfStats)> {
        let mut d = Draw(0x51ed_270b_27d1_9a3f);
        let mut out = Vec::new();
        for _ in 0..60 {
            let tree = drawn_tree(&mut d, 0);
            if count_nodes(&tree) > MAX_NODES || bounds(&tree).is_none() {
                continue;
            }
            for res in [8, 13, 24, 48, 90] {
                out.extend(mesh(&tree, &MeshOptions { max_triangles: 2_000_000, ..opts(res) }));
            }
        }
        out
    }

    /// The meshes with one sheet a cell, as one hash, and how many.
    fn one_sheet(meshes: &[(Shape, SdfStats)]) -> (String, usize) {
        let mut h = specula_core::hash::StableHasher::new();
        let mut n = 0;
        for (s, _) in meshes.iter().filter(|(_, st)| st.thin_cells == 0) {
            h.write_u64(mesh_hash(s));
            n += 1;
        }
        (format!("{:016x}", h.finish()), n)
    }

    /// **Where no cell has more than one sheet, nothing changes by a byte.** Every mesh of the
    /// repository's recipes without such a cell is the mesh the mesher made before
    /// ([`ONE_SHEET_RECIPES`]), and the twelve with one are the ones that had edges four
    /// triangles shared; the figure's parts and the drawn trees without one hash as they did
    /// (each hash was taken from the mesher before the change, mesh by mesh: the lead's notes
    /// for the mend). The full set of figures is
    /// `the_figures_with_one_sheet_a_cell_are_as_they_were`.
    #[test]
    fn a_mesh_with_one_sheet_a_cell_is_as_it_was() {
        let mut thin = Vec::new();
        let mut same = 0;
        for (name, meshes) in repository_sculpts() {
            for (i, (s, st)) in meshes.iter().enumerate() {
                if st.thin_cells > 0 {
                    thin.push(format!("{name} {i}"));
                    continue;
                }
                let before = ONE_SHEET_RECIPES.iter().find(|(n, j, _)| *n == name && *j == i).unwrap_or_else(|| panic!("{name}, mesh {i}: not one of the meshes with one sheet a cell before"));
                assert_eq!(format!("{:016x}", mesh_hash(s)), before.2, "{name}, mesh {i} changed");
                same += 1;
            }
        }
        assert_eq!(same, ONE_SHEET_RECIPES.len());
        // (Each of these had edges that four triangles shared: a detail and, where a build
        // needs one, its coarse mesh.)
        assert_eq!(thin.join(", "), "grave_rock 0, statue_angel 0, statue_mourner 0, runestone_t3 3, runestone_t3 5, skeleton_warrior_t3 0, skeleton_warrior_t3 3, skeleton_warrior_t3 11, skeleton_warrior_t3 14, tree_t3 0, tree_t3 2, tree_t3 3");
        use specula_corpus::Sex;
        let figures = figure_meshes(&[(Sex::Female, 1), (Sex::Male, 1)], &[(35.0, 7.0, 0.025)]);
        assert_eq!(one_sheet(&figures), (ONE_SHEET_FIGURES.into(), 7));
        assert_eq!(one_sheet(&drawn_meshes()), (ONE_SHEET_DRAWN.into(), 72));
        // Hands whose meshes close slivers (the closing tried in the order it always was,
        // shortest edge first), as they were.
        for (sex, res, before) in [(Sex::Female, 44, "6af6fcd32dc319e1"), (Sex::Male, 108, "84faf7f67535aaac"), (Sex::Male, 152, "f03d6d2b54bd4fc0")] {
            let [short, tall] = specula_corpus::ansur::statures(sex).map(|mm| mm / 1000.0);
            let body = specula_corpus::Body::with_build(sex, 0.5 * (short + tall), 0.0).unwrap();
            let tree = crate::figure::tree(&body, &crate::figure::Shaping { arms: 35.0, legs: 7.0, smooth: 0.025 }, crate::figure::Part::parse("hand_l").unwrap());
            let (s, st) = mesh(&tree, &MeshOptions { max_triangles: 2_000_000, ..opts(res) }).unwrap();
            assert!(st.closings_tested > 0 && st.thin_cells == 0, "{sex:?} hand at {res}: {st:?}");
            assert_eq!(format!("{:016x}", mesh_hash(&s)), before, "{sex:?} hand at {res}");
        }
    }

    // (Taken from the mesher before each sheet had a vertex of its own: the hashes of the
    // meshes with one sheet a cell, in this order.)
    const ONE_SHEET_FIGURES: &str = "74bcb25b940c52b3";
    // (The drawn set's, taken again twice since: on the merge with the sculpt box's second
    // round, eight of its trees took another box than the first round gave; and with the
    // box asking the field before a side moves, eight trees (stretched, twisted or bent among
    // them) keep a side of `bounds`'s box that the reach had moved. Mesh by mesh, each time,
    // only meshes whose box moved changed.)
    const ONE_SHEET_DRAWN: &str = "f76c5dfd7b39f488";
    const ONE_SHEET_FIGURES_ALL: &str = "a7df68f7523d352a";

    /// The same for the figure at both sexes' shortest, middle and tallest statures and three
    /// shapings (default, none, the limbs at their limits): 126 parts, 61 with one sheet a cell.
    #[test]
    #[ignore = "the full set of figures (about a minute in a release build): run by hand"]
    fn the_figures_with_one_sheet_a_cell_are_as_they_were() {
        use specula_corpus::Sex;
        let bodies: Vec<(Sex, usize)> = [Sex::Female, Sex::Male].into_iter().flat_map(|s| (0..3).map(move |w| (s, w))).collect();
        let figures = figure_meshes(&bodies, &[(35.0, 7.0, 0.025), (0.0, 0.0, 0.0), (60.0, 45.0, 0.025)]);
        assert_eq!(one_sheet(&figures), (ONE_SHEET_FIGURES_ALL.into(), 61));
    }

    /// **What keeping sheets apart costs is charged, all of it.** Grouping a cell's crossings
    /// ([`SHEETS_WORK`] a cell), asking a face's middle (its tree's cost), finding vertices at
    /// one place (an eighth a vertex, a unit a triangle when some are) and testing a sliver's
    /// edge before closing it (a unit a triangle): the build's work with them is the work
    /// without them and [`SdfStats::sheets_work`] exactly, and that is at least its parts;
    /// on a crescent (sheets and faces), a cone's tip (slivers), and the wider run's tree that
    /// has vertices at one place. The work a limit refuses is the same work.
    #[test]
    fn what_keeping_sheets_apart_costs_is_charged() {
        let ball = |x: f32| {
            let mut s = prim(Prim::Sphere { radius: 0.5 });
            s.m.at = [x, 0.0, 0.0];
            s
        };
        let crescent = group(Combine::Difference, vec![ball(0.0), ball(0.05)], 0.0);
        let tip = prim(Prim::Cylinder { radius: 0.3, radius_top: 0.0, half_height: 0.5, round: 0.0 });
        let mut d = Draw(0x1234_5678_9abc_def1);
        let shared = (0..999).map(|_| drawn_tree(&mut d, 0)).last().unwrap();
        for (tree, res) in [(&crescent, 48), (&tip, 24), (&shared, 11)] {
            let (s, st) = mesh(tree, &opts(res)).unwrap();
            FREE_SHEETS.with(|f| f.set(true));
            let free = mesh(tree, &opts(res));
            FREE_SHEETS.with(|f| f.set(false));
            let (_, free) = free.unwrap();
            assert_eq!(st.work, free.work + st.sheets_work, "{st:?}");
            let least = st.sheet_cells * SHEETS_WORK + st.faces_asked + st.closings_tested * s.tris.len() as u64 + s.positions.len() as u64 / 8;
            assert!(st.sheets_work >= least, "{} < {least}: {st:?}", st.sheets_work);
            assert!(mesh(tree, &MeshOptions { max_work: st.work - 1, ..opts(res) }).is_err_and(|e| e.limit));
        }
        let ((_, a), (_, b)) = (mesh(&crescent, &opts(48)).unwrap(), mesh(&tip, &opts(24)).unwrap());
        assert!(a.sheet_cells > 100 && a.faces_asked > 100 && b.closings_tested > 0, "{a:?} {b:?}");
    }

    /// **The same bytes twice**: the thin cases and drawn trees meshed twice in one process
    /// (nothing in the sheets, the faces, the threads or the closing depends on an order a
    /// map or a clock could change).
    #[test]
    fn a_thin_sculpt_meshes_to_the_same_bytes_twice() {
        let mut d = Draw(0x7f4a_7c15_9e37_79b9);
        let mut trees: Vec<(SdfNode, u32)> = thin_cases().into_iter().filter(|(_, _, r)| *r == 24 || *r == 96).map(|(_, t, r)| (t, r)).collect();
        trees.extend((0..20).map(|i| (drawn_tree(&mut d, 0), [16, 33, 64, 90][i % 4])).filter(|(t, _)| count_nodes(t) <= MAX_NODES && bounds(t).is_some()));
        let mut thin = 0;
        for (tree, res) in &trees {
            let once = mesh(tree, &opts(*res));
            let twice = mesh(tree, &opts(*res));
            match (once, twice) {
                (Ok((a, sa)), Ok((b, sb))) => {
                    assert_eq!((mesh_hash(&a), sa), (mesh_hash(&b), sb));
                    thin += usize::from(sa.thin_cells > 0);
                }
                (a, b) => assert_eq!(a.err().map(|e| e.message), b.err().map(|e| e.message)),
            }
        }
        assert!(thin > 20, "{thin}");
    }

    /// Shapes thinner than a cell, at resolutions a recipe can ask for: a crescent, shells
    /// thinner than a cell, a blade and tips, sheets that cross, two surfaces that touch at a
    /// point or along a line, a thread, a gap thinner than a cell.
    fn thin_cases() -> Vec<(&'static str, SdfNode, u32)> {
        let at = |mut n: SdfNode, p: V3| {
            n.m.at = p;
            n
        };
        let ball = |r: f32, p: V3| at(prim(Prim::Sphere { radius: r }), p);
        let plate = |half: V3, rotate: V3| {
            let mut n = prim(Prim::Box { half, round: 0.0 });
            n.m.rotate = rotate;
            n
        };
        let mut shell = prim(Prim::Sphere { radius: 0.5 });
        shell.m.shell = 0.01;
        let mut thin_shell = prim(Prim::Ellipsoid { radii: [0.5, 0.45, 0.4] });
        thin_shell.m.shell = 0.003;
        let mut tilted_shell = prim(Prim::Box { half: [0.4, 0.3, 0.2], round: 0.05 });
        tilted_shell.m.shell = 0.006;
        tilted_shell.m.rotate = [17.0, 29.0, 41.0];
        let crescent = group(Combine::Difference, vec![ball(0.5, [0.0; 3]), ball(0.5, [0.05, 0.0, 0.0])], 0.0);
        let crescents = group(Combine::Union, vec![crescent.clone(), at(crescent.clone(), [0.08, 0.02, 0.0])], 0.1);
        let lens = group(Combine::Intersection, vec![ball(0.5, [0.47, 0.0, 0.0]), ball(0.5, [-0.47, 0.0, 0.0])], 0.0);
        let tip = prim(Prim::Cylinder { radius: 0.3, radius_top: 0.0, half_height: 0.5, round: 0.0 });
        let mut needle = prim(Prim::Sphere { radius: 0.02 });
        needle.m.stretch = [20.0, 1.0, 1.0];
        needle.m.rotate = [0.0, 33.0, 21.0];
        let cross = group(Combine::Union, vec![plate([0.5, 0.004, 0.4], [0.0; 3]), plate([0.5, 0.004, 0.4], [37.0, 0.0, 0.0])], 0.0);
        let three = group(Combine::Union, vec![plate([0.4, 0.003, 0.4], [0.0; 3]), plate([0.003, 0.4, 0.4], [0.0; 3]), plate([0.4, 0.4, 0.003], [0.0; 3])], 0.0);
        let point = group(Combine::Union, vec![ball(0.25, [-0.25, 0.0, 0.0]), ball(0.25, [0.25, 0.0, 0.0])], 0.0);
        let line = group(Combine::Union, vec![at(prim(Prim::Cylinder { radius: 0.2, radius_top: 0.2, half_height: 0.4, round: 0.0 }), [-0.2, 0.0, 0.0]), at(prim(Prim::Cylinder { radius: 0.2, radius_top: 0.2, half_height: 0.4, round: 0.0 }), [0.2, 0.0, 0.0])], 0.0);
        let edges = group(Combine::Union, vec![plate([0.2; 3], [0.0; 3]), at(plate([0.2; 3], [0.0; 3]), [0.4, 0.4, 0.0])], 0.0);
        let gap = group(Combine::Union, vec![ball(0.25, [-0.2515, 0.0, 0.0]), ball(0.25, [0.2515, 0.0, 0.0])], 0.0);
        let thread = prim(Prim::Torus { radius: 0.4, tube: 0.004 });
        let mut cases = Vec::new();
        for res in [8, 12, 24, 48, 96, 157] {
            for (name, tree) in [("crescent", &crescent), ("two crescents joined", &crescents), ("lens", &lens), ("cone's tip", &tip), ("needle", &needle), ("crossing sheets", &cross), ("three sheets", &three), ("touch at a point", &point), ("touch along a line", &line), ("touch along an edge", &edges), ("a gap", &gap), ("a thread", &thread), ("shell 0.01", &shell), ("shell 0.003", &thin_shell), ("turned rounded shell", &tilted_shell)] {
                cases.push((name, tree.clone(), res));
            }
        }
        cases
    }

    /// Runs `f` with every mesh it makes on this thread kept, and gives them back.
    fn kept<T>(f: impl FnOnce() -> T) -> (T, Vec<(Shape, SdfStats)>) {
        MESHED.with(|m| *m.borrow_mut() = Some(Vec::new()));
        let t = f();
        (t, MESHED.with(|m| m.borrow_mut().take().unwrap_or_default()))
    }

    /// Every recipe of the repository with a sculpt (Sepulcra's and the fidelity set's), by
    /// name, with every mesh its build makes, in the order it makes them (a build meshes each
    /// sculpt at its detail and, where it needs one, coarsely).
    fn repository_sculpts() -> Vec<(String, Vec<(Shape, SdfStats)>)> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut out = Vec::new();
        for dir in [root.join("games/sepulcra/assets"), root.join("docs/fidelity/assets")] {
            let style = crate::Style::parse(&std::fs::read_to_string(dir.join("style.json")).unwrap()).unwrap();
            let mut lib = std::collections::BTreeMap::new();
            for e in std::fs::read_dir(dir.join("recipes")).unwrap() {
                let p = e.unwrap().path();
                if p.extension().is_some_and(|x| x == "json") {
                    lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
                }
            }
            for (name, text) in lib.iter().filter(|(_, t)| t.contains("\"sdf\"") || t.contains("\"figure\"")) {
                let (_, meshes) = kept(|| crate::build(text, &crate::BuildParams { library: Some(&lib), ..crate::BuildParams::new(&style) }).unwrap());
                out.push((name.clone(), meshes));
            }
        }
        out
    }

    /// **What keeps a mesh from being a closed surface**, from its positions and indices
    /// alone (nothing of the mesher's): its vertices joined where their positions are the same
    /// (as every reader of a mesh joins them), then every triangle has three different
    /// corners and an area of at least [`crate::mesh::MIN_TRIANGLE_AREA`], and every edge is
    /// shared by exactly two triangles that go along it opposite ways (so the orientation
    /// agrees across it, and nothing is open), and round every vertex its triangles make one
    /// fan (no two sheets meet at a point). `None` when it is one.
    fn not_a_surface(s: &Shape) -> Option<String> {
        if s.tris.is_empty() {
            return Some("no triangles".into());
        }
        let mut ids = std::collections::BTreeMap::new();
        let one: Vec<u32> = s
            .positions
            .iter()
            .map(|p| {
                let next = ids.len() as u32;
                *ids.entry(p.map(|x| (x + 0.0).to_bits())).or_insert(next)
            })
            .collect();
        let mut directed = std::collections::BTreeMap::new();
        for (n, t) in s.tris.iter().enumerate() {
            let v = t.map(|i| one[i as usize]);
            if v[0] == v[1] || v[1] == v[2] || v[2] == v[0] {
                return Some(format!("triangle {n} has two corners at one place"));
            }
            let p = t.map(|i| s.positions[i as usize].map(f64::from));
            let (a, b) = ([0, 1, 2].map(|k| p[1][k] - p[0][k]), [0, 1, 2].map(|k| p[2][k] - p[0][k]));
            let x = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            let area = 0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            if area < f64::from(crate::mesh::MIN_TRIANGLE_AREA) {
                return Some(format!("triangle {n} has an area of {area:e}"));
            }
            for k in 0..3 {
                *directed.entry((v[k], v[(k + 1) % 3])).or_insert(0u32) += 1;
            }
        }
        let place = |v: u32| s.positions[one.iter().position(|&x| x == v).unwrap_or(0)];
        for (&(a, b), &n) in &directed {
            let back = directed.get(&(b, a)).copied().unwrap_or(0);
            if n != 1 || back != 1 {
                return Some(format!("the edge from {:?} to {:?} is gone along {n} times one way and {back} the other", place(a), place(b)));
            }
        }
        // Round every vertex, its triangles make one fan: going from each triangle to the next
        // across the edges they share comes back to the first having met them all (two fans
        // that meet at one vertex, two sheets on one point, are not a surface there).
        let mut fans: Vec<(u32, u32, u32)> = Vec::with_capacity(3 * s.tris.len());
        for t in &s.tris {
            let v = t.map(|i| one[i as usize]);
            for k in 0..3 {
                fans.push((v[k], v[(k + 1) % 3], v[(k + 2) % 3]));
            }
        }
        fans.sort_unstable();
        for fan in fans.chunk_by(|x, y| x.0 == y.0) {
            let next = |a: u32| fan.binary_search_by_key(&a, |f| f.1).ok().map(|i| fan[i].2);
            let (mut at, mut steps) = (fan[0].1, 0);
            loop {
                at = match next(at) {
                    Some(b) => b,
                    None => return Some(format!("the triangles round {:?} do not close", place(fan[0].0))),
                };
                steps += 1;
                if at == fan[0].1 || steps > fan.len() {
                    break;
                }
            }
            if steps != fan.len() {
                return Some(format!("round {:?} the triangles make more than one fan ({steps} of {})", place(fan[0].0), fan.len()));
            }
        }
        None
    }

    /// **Every primitive's factor bounds it, or it is marked loose and its range is asked
    /// instead** (`docs/design/DISTANCE_FIELDS.md`, 2.6 and 2.7). Across each primitive's whole
    /// range of parameters, near it and far from it: a primitive with a factor never changes
    /// faster than that factor between two points (with twice its slack); an ellipsoid whose
    /// radii differ, and a column whose proportions change, do change faster (so the test
    /// would see a factor taken for them), are marked so (`loose`, `wander`), and over every
    /// ball the range the mesher asks ([`field_range_as`], tight) holds every value in it.
    #[test]
    fn every_primitives_factor_bounds_it_or_its_range_is_asked() {
        let mut r = Review(0x0bad_cafe_1234_5679);
        let (mut faster, mut pairs, mut balls) = (0, 0, 0);
        for case in 0..400 {
            let p = match case % 10 {
                0 => {
                    let rho = [1.0, 1.2, 2.0, 5.0, 25.0, 100.0][r.pick(6)];
                    let a = r.range(0.05, 0.5);
                    let mut e = [a, a / rho, a * r.range(1.0 / rho, 1.0)];
                    e.rotate_left(r.pick(3));
                    Prim::Ellipsoid { radii: e }
                }
                1 => {
                    let count = 1 + r.pick(MAX_SECTIONS);
                    let mut sections = [[0.0f32; 3]; MAX_SECTIONS];
                    let steep = [0.05, 0.3, 1.0, 3.0][r.pick(4)];
                    let keep = r.chance(0.3);
                    let shape = r.range(0.5, 2.0);
                    for (i, row) in sections.iter_mut().take(count).enumerate() {
                        let w = r.range(0.01, 0.01 + 0.1 * steep);
                        *row = [if count == 1 { 0.5 } else { i as f32 / (count - 1) as f32 }, w, if keep { w * shape } else { r.range(0.01, 0.01 + 0.1 * steep) }];
                    }
                    let length = r.range(0.1, 1.0);
                    Prim::Column { a: [0.0, 0.0, 0.0], b: [0.0, length, 0.0], side: [1.0, 0.0, 0.0], sections, count: count as u8, square: [2.0, 3.0, 4.0, r.range(2.0, 8.0), 8.0][r.pick(5)] }
                }
                2 => Prim::Sphere { radius: r.range(0.01, 0.5) },
                3 => {
                    let half = r.v3(0.01, 0.5);
                    Prim::Box { half, round: r.range(0.0, 1.0) * half[0].min(half[1]).min(half[2]) }
                }
                4 => {
                    let (radius, half_height) = (r.range(0.01, 0.5), r.range(0.01, 0.5));
                    Prim::Cylinder { radius, radius_top: if r.chance(0.5) { radius } else { r.range(0.0, 0.5) }, half_height, round: 0.0 }
                }
                5 => Prim::Capsule { radius: r.range(0.01, 0.4), half_height: r.range(0.0, 0.5) },
                6 => Prim::Torus { radius: r.range(0.05, 0.5), tube: r.range(0.01, 0.2) },
                7 => Prim::RoundCone { a: r.v3(-0.3, 0.3), b: r.v3(-0.3, 0.3), ra: r.range(0.01, 0.3), rb: r.range(0.01, 0.3) },
                8 => Prim::Rounded { half: r.v3(0.02, 0.5), power: 2.0 / r.range(0.05, 1.0), disc: r.chance(0.3) },
                _ => {
                    let half = r.v3(0.01, 0.5);
                    Prim::Box { half, round: 0.0 }
                }
            };
            let node = prim(p);
            let pp = prepare(&node, 1, &mut 0);
            let (lo, hi, _, _) = bounds(&node).unwrap();
            let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
            let mid = [0, 1, 2].map(|k| 0.5 * (lo[k] + hi[k]));
            let lip = prim_lip(&p);
            let slack = prim_slack(&p);
            let loose = pp.loose || pp.wander > 0.0;
            assert_eq!(pp.loose, ellipsoid_is_loose(&p));
            let mut fastest = 0.0f32;
            for k in 0..300 {
                // Near the middle, near the surface and far out (up to five sizes).
                let span = [0.05, 0.6, 5.0][k % 3] * size;
                let c = math3::add(mid, r.v3(-span, span));
                let step = size * [1e-3, 0.05][k % 2];
                let q = math3::add(c, math3::scale(math3::normalize(r.v3(-1.0, 1.0)), step));
                let (f0, f1) = (eval(&pp, c), eval(&pp, q));
                let rate = ((f1 - f0).abs() - 2.0 * slack).max(0.0) / step;
                fastest = fastest.max(rate);
                pairs += 1;
                if !loose {
                    assert!(rate <= lip * (1.0 + 1e-3) + 1e-3, "{p:?}: changes at {rate} between {c:?} and {q:?}, past its factor {lip}");
                }
                // The tight range holds every value of a ball about `c`.
                let rad = size * [0.005, 0.05, 0.3][r.pick(3)];
                let (least, most) = field_range_as(&pp, c, rad, &mut 1, &mut Spent::default(), true);
                for _ in 0..6 {
                    let v = math3::scale(math3::normalize(r.v3(-1.0, 1.0)), rad * r.unit());
                    let f = eval(&pp, math3::add(c, v));
                    let tol = 1e-5 * (1.0 + f.abs());
                    assert!(f >= least - tol && f <= most + tol, "{p:?}: {f} at {:?} outside {least}..{most} (ball {c:?}, {rad})", math3::add(c, v));
                }
                balls += 1;
            }
            if loose && fastest > lip * 1.01 {
                faster += 1;
            }
        }
        // (The loose ones the test drew did change faster than a factor says: it would see one
        // taken for them.)
        assert!(faster > 20 && pairs == 120_000 && balls == 120_000, "{faster} loose primitives changed faster than a factor; {pairs} pairs, {balls} balls");
    }

    /// **A block is skipped, and a union pruned, only where nothing of the surface is lost**
    /// (DISTANCE_FIELDS 2.7). A flat ellipsoid whose blocks have their middles on its middle
    /// plane, where its field reads its greatest half axis deep and its surface is its least
    /// away: by the factors alone (as before 2026-11-24) four blocks were filled solid (9,010
    /// triangles for 8,290); now the mesh is that of the tree sampled whole. So are flat and
    /// long ellipsoids turned every way, joined and cut with balls, and a repetition whose
    /// copies sit off their cells' middles.
    #[test]
    fn a_skipped_block_or_a_pruned_child_loses_no_surface() {
        let whole = |tree: &SdfNode, o: &MeshOptions| {
            NO_SKIP.with(|f| f.set(true));
            NO_PRUNE.with(|f| f.set(true));
            let m = mesh(tree, o);
            NO_SKIP.with(|f| f.set(false));
            NO_PRUNE.with(|f| f.set(false));
            m.unwrap()
        };
        let flat = prim(Prim::Ellipsoid { radii: [0.5, 0.02, 0.5] });
        let o = MeshOptions { bounds: Some(([-0.6, -0.05, -0.6], [0.6, 0.05, 0.6])), ..opts(60) };
        let (now, st) = mesh(&flat, &o).unwrap();
        let all = whole(&flat, &o);
        assert_eq!((now.tris.len(), mesh_hash(&now)), (all.0.tris.len(), mesh_hash(&all.0)), "{st:?}");
        OLD_FACTORS.with(|f| f.set(true));
        let (old, _) = mesh(&flat, &o).unwrap();
        OLD_FACTORS.with(|f| f.set(false));
        assert_eq!((old.tris.len(), all.0.tris.len()), (9010, 8290), "the case no longer shows what the factors alone lost");
        let mut r = Review(0x1234_5678_9abc_def1);
        let mut old_wrong = 0;
        for case in 0..36 {
            let a = r.range(0.1, 0.5);
            let rho = [5.0, 10.0, 10.0, 30.0][r.pick(4)];
            let mut e = prim(Prim::Ellipsoid { radii: if case % 2 == 0 { [a, a / rho, a * r.range(0.4, 1.0)] } else { [a / rho, a, a / rho] } });
            e.m.rotate = r.v3(-180.0, 180.0);
            e.m.at = r.v3(-0.1, 0.1);
            let tree = match case % 4 {
                0 => e,
                1 => {
                    let mut s = prim(Prim::Sphere { radius: r.range(0.02, 0.15) });
                    s.m.at = r.v3(-1.5 * a, 1.5 * a);
                    group([Combine::Union, Combine::Near, Combine::Difference][r.pick(3)], vec![e, s], [0.0, 0.02][r.pick(2)])
                }
                2 => {
                    let mut s = prim(Prim::Sphere { radius: 0.05 });
                    s.m.at = [2.5 * a, 0.0, 0.0];
                    group(Combine::Union, vec![e, s], 0.0)
                }
                _ => {
                    let spacing = r.range(0.2, 0.6);
                    let mut s = prim(Prim::Sphere { radius: r.range(0.05, 0.25) * spacing });
                    s.m.at = [r.range(-0.45, 0.45) * spacing, 0.0, 0.0];
                    let mut t = group(Combine::Union, vec![s], 0.0);
                    t.m.repeat = Some(([2 + r.pick(4) as u32, 1, 1], [spacing, 0.0, 0.0]));
                    t
                }
            };
            let o = opts([40, 64, 96][r.pick(3)]);
            let (now, _) = mesh(&tree, &o).unwrap();
            let all = whole(&tree, &o);
            assert_eq!(mesh_hash(&now), mesh_hash(&all.0), "case {case}: {} triangles, {} sampled whole\n{tree:?}", now.tris.len(), all.0.tris.len());
            OLD_FACTORS.with(|f| f.set(true));
            let (old, _) = mesh(&tree, &o).unwrap();
            OLD_FACTORS.with(|f| f.set(false));
            old_wrong += usize::from(mesh_hash(&old) != mesh_hash(&all.0));
        }
        // (By the factors alone some of these lost surface: the set can see the fault.)
        assert!(old_wrong > 0, "none of the drawn cases differed by the factors alone");
    }
}
