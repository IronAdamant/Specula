//! How coarsely each shape a recipe names round was built, **measured in the finished model**
//! (docs/design/FORM.md, "Faceting, from what the recipe names round").
//!
//! A recipe says what is meant to be round; the facets are what its counts made of it. A
//! shape notes, as it is built, its turn between neighbouring faces and its radius in its own
//! frame, and it is given a slot of the build's record, so that every triangle it makes knows
//! it ([`crate::Shape::owners`]: through every operation, a boolean and a scatter). Every
//! operation that moves its triangles afterwards carries the linear map it applied
//! ([`Moved`]), so what a facet is in the finished model is known exactly:
//!
//! - under a move, a turn, a mirror, a uniform scale, copies and scatters (similarities), the
//!   turn is as built and the radius and how far a facet stands in from the true curve scale
//!   by the factor, the largest copy's;
//! - a ring (a cylinder's, a cone's, a lathe's, a torus's round its axis, a capsule's round
//!   its axis) is measured **face by face round its axis**, under any linear map (a scale that
//!   is not the same every way, a lean, a turn then an uneven scale): its profile is read off
//!   the built mesh (the edges of the column of vertices at its first vertex's angle), every
//!   band of faces between two rows of it is rebuilt from it, each face's normal is carried
//!   through the map by its inverse transpose, the turn is the angle between neighbouring
//!   faces' normals and a face stands in from the true surface of its band (the surface of
//!   revolution between the band's two rims, carried through the map) by its gap as built
//!   over the length of its carried normal. That is exact for planar faces under an affine map
//!   (parallel planes stay parallel, and the distance between two goes as one over that
//!   length). A band whose faces run along the axis (a wall) is so measured in the section
//!   square to the mapped axis, not in the plane of its rings, which a lean or an uneven
//!   scale after a turn tilts away from it; a sloped band (a cone, a flared cylinder, a
//!   lathe's slope, a capsule's ends and a torus's tube round the axis) the same way. A
//!   sphere is measured face by face too; any other kind under such a map is **not
//!   measured** and says so (a capsule's ends from one ring to the next, a torus's tube from
//!   one side to the next, a sculpt);
//! - a taper, a twist and a bend are not affine: what they make of a facet is not measured;
//! - a displacement moves the facets and the curve they stand for together (one field): the
//!   facets are as they were, and how far it moved them is said beside them.
//!
//! A shape with no triangle of its own in the finished model (a cutter that cuts nothing, a
//! shape cut wholly away) says nothing: exact, from the tie. A cutter that does cut speaks
//! for the walls it left. Nothing here changes a mesh.

use crate::dmath;
use crate::math3::Affine;
use crate::shape::{Shape, NO_OWNER};
use specula_core::Json;

/// A 3×3 linear map, columns.
pub(crate) type M3 = [[f64; 3]; 3];

pub(crate) const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Distinct linear maps one round shape keeps (its copies'); past them, while every map is a
/// similarity, only the largest factor is kept (all a similarity needs); otherwise it is not
/// measured.
pub(crate) const MAX_MAPS: usize = 32;

/// Round shapes recorded while one model is built (past them, a round shape is not recorded
/// and the facts say how many were not).
pub(crate) const MAX_RECS: usize = 4096;

/// A shape a recipe names round, and how coarsely it was built, in the finished model:
/// **what a mesh alone cannot say** (a box and a four-sided cylinder are one mesh; whether a
/// ten-sided column is a coarse cylinder or a prism is what its maker meant, and the recipe
/// says which: a prism is an extruded outline of ten sides, a cylinder is a cylinder).
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    /// Where in the recipe (`parts[2].shape`, `parts[0].shape.of[1]`,
    /// `parts[1].include(gem).parts[0].shape`; a sculpt's primitive adds `.tree` and its
    /// number, depth first: `parts[0].shape.tree#3`).
    pub path: String,
    /// The shape's name in the recipe.
    pub what: &'static str,
    /// Which of its numbers made this turn: `sides`, `segments`, `rings`, `subdivisions`,
    /// `resolution`.
    pub count: &'static str,
    /// That number as built.
    pub count_value: u32,
    /// The largest turn between neighbouring faces round it, degrees, in the finished model
    /// (a ring's: between neighbouring faces round its axis, in any band of them; a sloped
    /// band's faces turn less than a wall's: a cone's `2·asin(cos a · sin(180°/sides))`, `a`
    /// its faces' slope from the axis). What its outline does seen down its axis is [`Round::rim`].
    pub turn_deg: f32,
    /// The largest radius it has in the finished model, metres.
    pub radius_m: f32,
    /// The radius of the band of faces that turns by `turn_deg` (a ring's: its wider rim's
    /// furthest from the axis, square to the axis; any other's: `radius_m`), metres.
    pub turn_radius_m: f32,
    /// How far the middle of a facet stands in from the true curve of its own band, at the
    /// most, metres, in the finished model: the faces' own faceting. A sloped band's faces stand
    /// in less than its outline does seen down its axis ([`Round::rim`]), which a silhouette from
    /// above shows.
    pub sag: f32,
    /// A ring's widest rim seen down its axis, in the finished model (under the shape's own
    /// map, the rim projected along the mapped axis: a polygon whose true curve is an ellipse):
    /// how far its outline turns at a corner and stands in. `None` for what is not a ring (a
    /// sphere, a capsule's ends, a torus's tube, a sculpt).
    pub rim: Option<Rim>,
    /// The fewest and the most of its count it can be built with: the most is the greatest
    /// at which the build is sure every limit it holds this shape's part and the model to
    /// holds (its own range, the triangles of a part and of the model, the recipe's budget),
    /// as the build checks them, or, where [`Round::most_estimated`] says, estimated.
    pub least: u32,
    pub most: u32,
    /// What stops the count at `most`: `"its range"`, or the limit it would break past it.
    pub most_by: &'static str,
    /// Why `most` is an estimate and not sure (its triangles pass through a boolean, or it is a
    /// sculpt's resolution); `None`: sure.
    pub most_estimated: Option<&'static str>,
    /// How far a displacement moved its facets (with the curve they stand for), metres; 0 for
    /// none.
    pub displaced_m: f32,
    /// `None`: measured. Otherwise why not (a taper, a twist or a bend; a shape other than a
    /// ring stretched unevenly); the turn, radius and sag are then as it was built.
    pub unmeasured: Option<&'static str>,
    /// What measures it again at another count, the same way and through the same maps
    /// ([`Round::turn_at`]): advice is checked by it before it is given. Empty for a shape not
    /// measured, and for a record made by hand.
    pub recount: Recount,
}

/// A ring's widest rim seen down its axis ([`Round::rim`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rim {
    /// The largest turn of its outline at a corner, degrees (as built, 360° over the count).
    pub turn_deg: f32,
    /// Its furthest corner from the axis, square to it, metres.
    pub radius_m: f32,
    /// How far the middle of a side of its outline stands in from the true curve's outline
    /// (the rim's ellipse), metres.
    pub sag: f32,
}

/// Which of a ring's two turns a limit holds it to ([`Round::held`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    /// The turn between neighbouring faces.
    Faces,
    /// The turn of its widest rim's outline, seen down its axis.
    Rim,
}

/// What a [`Round`] needs to be measured again at another count (opaque).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recount {
    kind: Again,
    /// The linear maps its copies were put through (the identity for a similarity's).
    maps: Vec<M3>,
    /// What bounds the build's work at another count ([`Spent`]).
    work: Option<WorkIn>,
}

impl Recount {
    /// A shape whose turn is `per` over its count (as a wall's is as built, `per` 360°, and a
    /// capsule's ends', 90°), unmoved: for a record made by hand.
    pub fn even(per: f64) -> Recount {
        Recount { kind: Again::Even(per), maps: vec![IDENTITY], work: None }
    }

    /// A sphere of `radius`, unmoved, measured face by face: for a record made by hand.
    pub fn sphere(radius: f64) -> Recount {
        Recount { kind: Again::Sphere(radius), maps: vec![IDENTITY], work: None }
    }

    /// A sculpt's primitive, whose turn at resolution `n` is `2·asin(k / n)`: for a record made
    /// by hand.
    pub fn cell(k: f64) -> Recount {
        Recount { kind: Again::Cell(k), maps: vec![IDENTITY], work: None }
    }
}

/// What bounds the build's work were this shape built with another count: `W(k)` of
/// [`Spent`], from what the build spent with its count as it was.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkIn {
    n: u32,
    /// Its count in the coarse far level at another count: halved (rounded up to even, never
    /// below its fewest) for sides, segments and rings; as it is for subdivisions and a
    /// sculpt's resolution (which a coarse build halves too: taken whole, an estimate there).
    halves: bool,
    least: u32,
    grows: Grows,
    alone: Option<(u32, u64)>,
    made: u64,
    /// The work of the operations on pieces holding its triangles, whole.
    own: u64,
    /// The model-wide charges its parts are in.
    held: Held,
    total: u64,
    /// The simplification of the far levels a coarse rebuild stood in for, at its count.
    lods: u128,
    coarse: Coarse,
}

impl WorkIn {
    /// Its triangles at `k`: as the build checks them (its builder's prediction), and what its
    /// builder makes, at the least what it made (on the line through what it made as built and
    /// alone at another count, rounded up; with no such line, what the prediction adds, which
    /// bounds it; a sculpt's taken to go as the square of its resolution).
    fn triangles(&self, k: u32) -> (u128, u128) {
        let n = self.n;
        let a = u128::from(self.made.max(1));
        let (own, grown) = match (self.grows, self.alone) {
            (Grows::Square, _) => {
                let nn = u128::from(n.max(1)) * u128::from(n.max(1));
                let g = (a * u128::from(k) * u128::from(k)).div_ceil(nn);
                (g, g)
            }
            (g, Some((m, tm))) if m != n && self.made > 0 => {
                let (a, tm) = (i128::from(self.made), i128::from(tm));
                let rise = (a - tm) * (i128::from(k) - i128::from(n));
                let run = i128::from(n) - i128::from(m);
                (u128::from(g.at(k)), (a + up_div(rise, run)).max(0) as u128)
            }
            (g, _) => {
                let p = u128::from(g.at(k));
                (p, a + p.saturating_sub(u128::from(g.at(n))))
            }
        };
        (own, grown.max(a))
    }

    /// `ρ(k)` of [`Spent`]: the most any count of the shape's own elements grows by from its
    /// count to `k`, as a fraction (numerator, denominator). The greater of: its triangles'
    /// growth (as the triangle limits bound it); and `k / n`, which bounds every count of a
    /// ring's elements that goes as a line `αk + β` with its count with `β ≥ 0` (its vertices,
    /// rims, seams and edges). A
    /// sphere's elements all go as four to the subdivisions (`20·4ˢ` faces, `30·4ˢ` edges,
    /// `10·4ˢ + 2` points); a sculpt's are taken to go as the cube of its resolution, which its
    /// meshing's grid does at the most (an estimate: its most is one already).
    fn growth(&self, k: u32) -> (u128, u128) {
        let n = self.n.max(1);
        let a = u128::from(self.made.max(1));
        let grown = self.triangles(k).1;
        let mut best = (grown, a);
        let mut take = |p: u128, q: u128| {
            if q > 0 && p * best.1 > best.0 * q {
                best = (p, q);
            }
        };
        let (k128, n128) = (u128::from(k.max(n)), u128::from(n));
        match self.grows {
            Grows::Square => take(k128 * k128 * k128, n128 * n128 * n128),
            Grows::Fourfold => take(1u128 << (2 * k.saturating_sub(n).min(40)), 1),
            // (Every ring's builder makes vertices that go as a line with its count with no
            // part of them taken away, so `k/n` bounds their growth: held for each by
            // `a_rings_vertices_grow_no_faster_than_its_count`.)
            Grows::Linear { .. } => take(k128, n128),
        }
        best
    }

    /// The build's work at `k` in place of its count, at the most (`W(k)` of [`Spent`]).
    fn at(&self, k: u32) -> u128 {
        let (num, den) = self.growth(k);
        let up = |x: u128| x.div_ceil(den);
        let own = u128::from(self.own);
        let h = &self.held;
        let s = own + u128::from(h.cost);
        let assembled = u128::from(crate::work::ASSEMBLE_VERTEX);
        let rebuilt = own + assembled * u128::from(h.vertices);
        // (The collider's points: at k at most three a triangle of the parts holding it, less
        // what they had here.)
        let points = u128::from(h.per_point) * (up(3 * u128::from(h.triangles) * num)).saturating_sub(u128::from(h.assembled));
        // (A coarse rebuild of what holds it costs no more than the same built in full with its
        // count as the coarse build takes it: its pieces grown by that count's growth, no less
        // than as they are here. The rest of a coarse rebuild costs no more than here.)
        let c = if self.halves { (k.div_ceil(2).div_ceil(2) * 2).min(k).max(self.least) } else { k };
        let (cn, cd) = self.growth(c);
        let at_c = |x: u128| x * cn.max(cd) / cd + 1;
        let coarse = match self.coarse {
            Coarse::Never => 0,
            // (What it cost here, and what holds it at its coarse count, whole.)
            Coarse::Built => at_c(rebuilt),
            Coarse::Could { parts, vertices } | Coarse::Unknown { parts, vertices } => {
                u128::from(parts).saturating_sub(own) + assembled * u128::from(vertices.saturating_sub(h.vertices)) + at_c(rebuilt)
            }
        };
        u128::from(self.total) + up(s * (num - den)) + points + self.lods + coarse
    }
}

/// `rise / run` rounded up (never less than the line).
fn up_div(rise: i128, run: i128) -> i128 {
    rise.div_euclid(run) + i128::from(run > 0 && rise.rem_euclid(run) != 0)
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Again {
    #[default]
    No,
    /// Faces round an axis, face by face.
    Ring(Ring),
    /// A sphere of this radius, face by face.
    Sphere(f64),
    /// A turn of `per` over the count, only under similarities (a capsule's ends, a torus's
    /// tube).
    Even(f64),
    /// A sculpt: the cell is the bounds' longest side over the resolution, and turns the
    /// surface by twice the arc sine of the cell over twice the radius (`longest / 2r`).
    Cell(f64),
}

impl Round {
    /// The turns this shape would have in the finished model with `count` in place of its
    /// count, measured as this record was (the same maps, the same faces): between its faces,
    /// and (a ring's) of its widest rim seen down its axis. `None` when that cannot be said
    /// (not measured; a record made by hand).
    pub fn turns_at(&self, count: u32) -> Option<(f32, Option<f32>)> {
        let maps = &self.recount.maps;
        if maps.is_empty() {
            return None;
        }
        let (t, rim) = match &self.recount.kind {
            Again::No => return None,
            Again::Ring(ring) => maps.iter().map(|m| ring.under(m, count)).fold((0.0, Some(0.0)), |(t, r), u| (u.faces.turn.max(t), r.map(|r: f64| r.max(u.rim.map_or(0.0, |s| s.turn))))),
            Again::Sphere(r) => (maps.iter().map(|m| sphere_under(m, *r, count).0).fold(0.0, f64::max), None),
            Again::Even(per) => (per / f64::from(count.max(1)), None),
            Again::Cell(k) => (2.0 * dmath::asin_f64((k / f64::from(count.max(1))).min(1.0)).to_degrees(), None),
        };
        Some((t as f32, rim.map(|r| r as f32)))
    }

    /// The build's work at the most were it built with `count` in place of its count, in work
    /// units ([`crate::work`]): what [`Round::most`] holds to the build's work limit (`W(k)`
    /// of the modeller's bound: docs/design/FORM.md, "What was mended, fourth round"). `None`
    /// for a record made by hand.
    pub fn work_at(&self, count: u32) -> Option<u64> {
        self.recount.work.as_ref().map(|w| u64::try_from(w.at(count)).unwrap_or(u64::MAX))
    }

    /// The greater of the two turns [`Round::turns_at`] answers.
    pub fn turn_at(&self, count: u32) -> Option<f32> {
        self.turns_at(count).map(|(t, r)| t.max(r.unwrap_or(0.0)))
    }

    /// The work [`Round::turns_at`] takes at `count`, in faces and corners measured (for a
    /// caller that bounds a search).
    pub fn turn_at_work(&self, count: u32) -> u64 {
        let maps = self.recount.maps.len() as u64;
        match &self.recount.kind {
            // (Every band's faces, and the rim's corners twice: its outline and its curve.)
            Again::Ring(ring) => maps * u64::from(count.max(3)) * (ring.prof.len() as u64 + 2),
            Again::Sphere(_) => maps * (20u64 << (2 * count.min(5))),
            _ => 1,
        }
    }

    /// The turn a limit holds it to where its facets are seen (stand in by `seen` or more): of
    /// its faces' turn and its rim's (seen down its axis), the greater of those that are seen,
    /// and which it was (the faces' on a tie). `None` when neither is seen.
    pub fn held(&self, seen: f32) -> Option<(f32, By)> {
        let faces = (self.sag >= seen).then_some((self.turn_deg, By::Faces));
        let rim = self.rim.filter(|r| r.sag >= seen).map(|r| (r.turn_deg, By::Rim));
        match (faces, rim) {
            (Some(f), Some(r)) => Some(if r.0 > f.0 { r } else { f }),
            (f, r) => f.or(r),
        }
    }

    /// How many maps it is measured again through (its copies' distinct maps; one for those
    /// that are all similarities): what one measure at another count costs goes with it.
    pub fn recount_maps(&self) -> usize {
        self.recount.maps.len()
    }

    /// The greater of its two stand-ins (its faces' and its rim's).
    pub fn sag_seen(&self) -> f32 {
        self.rim.map_or(self.sag, |r| r.sag.max(self.sag))
    }
}

impl Round {
    /// How far the middle of a facet stands in from the true curve, metres.
    pub fn sag_m(&self) -> f32 {
        self.sag
    }

    pub fn measured(&self) -> bool {
        self.unmeasured.is_none()
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f32, k: f64| Json::from(dmath::round_f64(f64::from(x) * k) / k);
        let mut j = Json::obj([
            ("path", Json::from(self.path.as_str())),
            ("shape", Json::from(self.what)),
            ("count", Json::from(self.count)),
            ("count_value", Json::from(self.count_value)),
            ("turn_deg", r(self.turn_deg, 100.0)),
            ("radius_m", r(self.radius_m, 1.0e4)),
            ("sag_m", r(self.sag, 1.0e5)),
            ("turn_radius_m", r(self.turn_radius_m, 1.0e4)),
            ("most", Json::from(self.most)),
            ("most_by", Json::from(self.most_by)),
        ]);
        if let Some(rim) = self.rim {
            j.set("rim", Json::obj([("turn_deg", r(rim.turn_deg, 100.0)), ("radius_m", r(rim.radius_m, 1.0e4)), ("sag_m", r(rim.sag, 1.0e5))]));
        }
        if let Some(why) = self.most_estimated {
            j.set("most_estimated", Json::from(why));
        }
        if self.displaced_m > 0.0 {
            j.set("displaced_m", r(self.displaced_m, 1.0e4));
        }
        if let Some(why) = self.unmeasured {
            j.set("not_measured", Json::from(why));
        }
        j
    }
}

/// How far the middle of a facet that turns by `turn_deg` stands in from a curve of
/// `radius`.
pub fn sag_of(radius: f64, turn_deg: f64) -> f64 {
    radius * (1.0 - dmath::cos_f64((turn_deg * 0.5).to_radians()))
}

/// What a shape notes as it is built, before it has a slot.
#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub what: &'static str,
    pub count: &'static str,
    pub count_value: u32,
    pub turn_deg: f64,
    pub radius: f64,
    /// A ring round the shape's own Y axis of this many sides (its first vertex's angle is
    /// found on the built mesh).
    pub ring: Option<u32>,
    /// A sculpt's primitive: its number, depth first.
    pub primitive: Option<usize>,
    /// The fewest and the most of its count its builder takes (its own range).
    pub least: u32,
    pub most: u32,
    /// How the shape's triangles grow with the count, as its builder predicts them.
    pub grows: Grows,
    /// The triangles its builder makes alone at another count (a ring's go as a line with
    /// its count: with what it made as built, that line); `None` where the prediction is
    /// what it makes (a sphere's) or there is none (a sculpt's).
    pub alone: Option<(u32, u64)>,
    /// The model's triangles before the shape was checked against the limits.
    pub total_before: u64,
}

/// How a shape's triangles grow with its count, **as its builder predicts them**: the build
/// holds a shape to the limits on a part's and the model's triangles by this prediction before
/// it builds it, and never makes more than it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grows {
    /// `per` a count, and `fixed` besides.
    Linear { per: u64, fixed: u64 },
    /// Twenty, four times over for each (a sphere's subdivisions).
    Fourfold,
    /// With the square of the count, from what was built (a sculpt's resolution): an estimate,
    /// since what a mesher makes is known only once it has made it.
    Square,
}

impl Grows {
    /// The triangles predicted at `k` (none for [`Grows::Square`], which has no prediction).
    pub(crate) fn at(self, k: u32) -> u64 {
        match self {
            Grows::Linear { per, fixed } => per.saturating_mul(u64::from(k)).saturating_add(fixed),
            Grows::Fourfold => 20u64 << (2 * k.min(20)),
            Grows::Square => 0,
        }
    }
}

/// A round shape while the model is built.
#[derive(Clone, Debug)]
pub(crate) struct Rec {
    pub note: Note,
    /// The made-record it hangs on (its path) and its slot (its triangles).
    pub record: u32,
    pub slot: u32,
    /// A ring's faces round its axis, read off the built shape ([`Ring::of`]).
    pub ring: Option<Ring>,
    /// The linear maps its copies were put through since it was built.
    pub maps: Vec<M3>,
    /// Past [`MAX_MAPS`] copies, all similarities: the largest factor.
    pub many: Option<f64>,
    pub displaced: f64,
    pub unknown: Option<&'static str>,
    /// The triangles the shape had as built.
    pub made: u64,
    /// Where the build's log of the model's triangles stood when it was made (the checks
    /// after it hold its triangles too).
    pub since: usize,
    /// The tightest check on a piece of a part its triangles were in: room for its share
    /// (the part's limit less the rest of the piece), its share, and what limit it was.
    pub tight: Option<(u64, u64, &'static str)>,
    /// The most of its triangles any check counted.
    pub share_max: u64,
    /// Why its most count can only be estimated.
    pub estimate: Option<&'static str>,
    /// The work charged to the operations on pieces its triangles were in, whole (each such
    /// charge is by the piece's vertices or triangles, of which its own are a share).
    pub work: u64,
}

/// Why a count's greatest that builds is estimated: a sculpt's triangles at another resolution
/// are known only once its mesher has made them (taken here to grow as its square).
pub(crate) const SCULPT_ESTIMATE: &str = "a sculpt's triangles at another resolution are known only once it is meshed (taken to grow as the square of it)";
/// And a boolean's result.
pub(crate) const BOOLEAN_ESTIMATE: &str = "it passes through a boolean, whose triangles are known only once it is done (taken to grow as its own do)";

/// What stops a count at its greatest that builds, past its range.
pub(crate) const PART_LIMIT: &str = "the triangles one part may have";
pub(crate) const MODEL_LIMIT: &str = "the triangles the model may have";
pub(crate) const BUDGET_LIMIT: &str = "the recipe's budget.max_triangles";
pub(crate) const WORK_LIMIT: &str = "the build's work limit (1,500 million units)";
/// Why the most is an estimate where the work its far level costs at another count is not
/// known.
pub(crate) const COARSE_ESTIMATE: &str =
    "at another count its far level may be rebuilt coarse, and what a boolean, a sculpt or a generator costs there is known only once it is built (taken to cost what the parts did)";

/// The build's work, as [`Rec::most_count`] holds a count to the build's work limit
/// ([`crate::work::MAX_BUILD`]). Each charge of the build is by the vertices or the triangles
/// of what it works on (a piece's, the model's parts', the finest level's), or does not depend
/// on a round shape's count at all (a texture's bake, a boolean, a sculpt or a generator the
/// shape is not in). So with `ρ(k)` the most any count of the shape's own elements grows by
/// from its count `n` to `k`, a charge whose elements include the shape's costs at `k` at most
/// `ρ(k)` times what it cost at `n` (the rest of what it works on does not change), and
///
/// `W(k) ≤ W(n) + (ρ(k) − 1)·S + P(k)`,
///
/// `S` the charges at `n` whose elements include the shape's (every level's simplification
/// counted, charged or not), `P(k)` what may be charged at `k` and was not at `n`: a far level
/// simplified where a coarse rebuild stood in, and a coarse rebuild itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct Spent {
    /// All the work of the build, its textures' bakes included.
    pub total: u64,
    /// The far levels the recipe asks for, and how many of them were charged no
    /// simplification (a coarse rebuild stood in).
    pub levels: u64,
    pub unsimplified: u64,
    /// The finest level's triangles, all parts.
    pub triangles: u64,
    /// Each built part in order, as assembled and simplified.
    pub parts: Vec<PartSpent>,
    /// The recipe's paint strokes, and the work a hull or box point costs the collider.
    pub strokes: u64,
    pub per_point: u64,
    pub coarse: Coarse,
}

/// One built part's charges' elements.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PartSpent {
    pub triangles: u64,
    pub vertices: u64,
    /// Its vertices once assembled (split where its faces turn past its smooth angle, and at
    /// its texture's seams): the collider's points.
    pub assembled: u64,
    /// Whether paint strokes reach it (an untextured part).
    pub painted: bool,
}

/// The model-wide charges a round shape's elements are in: those of the parts holding its
/// triangles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Held {
    /// Their assembly, paint and every level's simplification (charges by those parts'
    /// vertices and triangles, which grow with the shape's at most as its own do).
    pub cost: u64,
    pub vertices: u64,
    pub triangles: u64,
    /// Their vertices once assembled, and what the collider charges a point. An assembled
    /// part's vertices are split where its faces turn past its smooth angle, which a count
    /// moves either way (a turn at the angle itself splits by the rounding): at another count
    /// they are taken at their most, a vertex for every corner of every triangle.
    pub assembled: u64,
    pub per_point: u64,
}

/// The coarse far level ([`crate::recipe`]'s rebuild) as it stood at the count built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Coarse {
    /// No far levels: never rebuilt.
    #[default]
    Never,
    /// Rebuilt whole: its cost at another count is its cost here and its share grown.
    Built,
    /// Not rebuilt, and nothing procedural in the build: at another count it may be, at no
    /// more than the parts and their assembly cost here, with what holds the shape at its
    /// coarse count (a coarse build halves counts and subdivides and displaces nothing).
    /// `parts`: the work the parts took; `vertices`: all the parts' vertices.
    Could { parts: u64, vertices: u64 },
    /// Not rebuilt whole (not at all, with a boolean, a sculpt or a generator somewhere; or
    /// stopped part way): what it costs at another count is not known. Taken as
    /// [`Coarse::Could`] would, and the most is an estimate.
    Unknown { parts: u64, vertices: u64 },
}

/// What an operation did to the triangles of a shape, for the round shapes among them.
pub(crate) enum Moved {
    Linear(M3),
    /// Copies, each through its own map (an array, a radial, a mirror that keeps both halves,
    /// a scatter's places).
    Copies(Vec<M3>),
    /// Not affine.
    Unknown(&'static str),
    /// Displaced by up to this far.
    Displaced(f64),
}

/// An affine's linear part.
pub(crate) fn linear(a: &Affine) -> M3 {
    a.cols.map(|c| c.map(f64::from))
}

fn mul(a: &M3, b: &M3) -> M3 {
    let mut out = [[0.0; 3]; 3];
    for (j, col) in b.iter().enumerate() {
        for i in 0..3 {
            out[j][i] = a[0][i] * col[0] + a[1][i] * col[1] + a[2][i] * col[2];
        }
    }
    out
}

fn apply(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// The factor of a similarity (`None` for a map that stretches some ways more than others).
pub(crate) fn similarity(m: &M3) -> Option<f64> {
    let g = [0, 1, 2].map(|i| [0, 1, 2].map(|j| dot(m[i], m[j])));
    let s = (g[0][0] + g[1][1] + g[2][2]) / 3.0;
    if !(s.is_finite() && s > 0.0) {
        return None;
    }
    let off = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (g[i][j] - if i == j { s } else { 0.0 }).abs()).fold(0.0, f64::max);
    (off <= 1e-5 * s).then(|| s.sqrt())
}

/// The largest stretch of a map: the square root of its Gram matrix's largest eigenvalue (by
/// a fixed number of power steps from a fixed start: the same on every machine; an upper
/// bound is its Frobenius norm, which it never passes).
fn stretch(m: &M3) -> f64 {
    let g = [0, 1, 2].map(|i| [0, 1, 2].map(|j| dot(m[i], m[j])));
    let mut v = [1.0, 0.7, 0.4];
    let mut l = 0.0;
    for _ in 0..64 {
        let w = [dot(g[0], v), dot(g[1], v), dot(g[2], v)];
        let n = norm(w);
        if !(n > 0.0 && n.is_finite()) {
            break;
        }
        l = n / norm(v);
        v = w.map(|x| x / n);
    }
    l.sqrt()
}

impl Rec {
    pub(crate) fn moved(&mut self, mv: &Moved) {
        if self.unknown.is_some() {
            return;
        }
        match mv {
            Moved::Linear(l) => {
                self.displaced *= stretch(l);
                match (&mut self.many, similarity(l)) {
                    (Some(f), Some(s)) => *f *= s,
                    (Some(_), None) => self.unknown = Some("copied many ways, then stretched unevenly"),
                    (None, _) => self.maps.iter_mut().for_each(|m| *m = mul(l, m)),
                }
            }
            Moved::Copies(ls) => {
                self.displaced *= ls.iter().map(stretch).fold(0.0, f64::max);
                if let Some(f) = &mut self.many {
                    match ls.iter().map(similarity).collect::<Option<Vec<f64>>>() {
                        Some(s) => *f *= s.into_iter().fold(0.0, f64::max),
                        None => self.unknown = Some("copied many ways, then stretched unevenly"),
                    }
                    return;
                }
                let mut out: Vec<M3> = Vec::new();
                for l in ls {
                    for m in &self.maps {
                        let x = mul(l, m);
                        if !out.contains(&x) {
                            out.push(x);
                        }
                    }
                }
                if out.len() <= MAX_MAPS {
                    self.maps = out;
                } else {
                    match out.iter().map(similarity).collect::<Option<Vec<f64>>>() {
                        Some(s) => {
                            self.many = Some(s.into_iter().fold(0.0, f64::max));
                            self.maps.clear();
                        }
                        None => self.unknown = Some("copied many ways, stretched unevenly"),
                    }
                }
            }
            Moved::Unknown(why) => self.unknown = Some(why),
            Moved::Displaced(d) => self.displaced += d.abs(),
        }
    }

    /// A check on a part's piece of `checked` triangles counted `share` of its own: kept when it
    /// leaves its share less room than any before.
    pub(crate) fn charge(&mut self, share: u64, checked: u64, estimate: Option<&'static str>) {
        if share == 0 {
            return;
        }
        self.share_max = self.share_max.max(share);
        if estimate.is_some() && self.estimate.is_none() {
            self.estimate = estimate;
        }
        let room = (crate::recipe::MAX_PART_TRIANGLES as u64).saturating_sub(checked.saturating_sub(share));
        let tighter = match self.tight {
            Some((r, s, _)) => u128::from(room) * u128::from(s) < u128::from(r) * u128::from(share),
            None => true,
        };
        if tighter {
            self.tight = Some((room, share, PART_LIMIT));
        }
    }

    /// The greatest count, from the one built to its range's most, at which every limit the
    /// build held this shape's triangles to would still hold, and what stops it there: the
    /// shape itself checked by its builder's prediction (against a part's limit and the model's
    /// room when it was made), each piece of a part its triangles were in (their share grown as
    /// the shape's own triangles grow: by no more than the prediction grows, which bounds what
    /// is built), the model's triangles at their greatest since it was made, and the recipe's
    /// budget (`budget`: its limit and the finished model's triangles).
    ///
    /// And the build's work limit, by [`Spent`]'s inequality: `held` is what the model-wide
    /// charges cost that the shape's elements are in (the parts holding its triangles: their
    /// assembly, paint, collider points and every level's simplification) and those parts'
    /// vertices.
    fn most_count(&self, peak: u64, budget: Option<(u64, u64)>, work: &WorkIn) -> (u32, &'static str) {
        let n = self.note.count_value;
        let a = u128::from(self.made.max(1));
        let part = crate::recipe::MAX_PART_TRIANGLES as u128;
        let model = crate::recipe::MAX_MODEL_TRIANGLES as u128;
        let before = u128::from(self.note.total_before);
        let share = u128::from(self.share_max.max(self.made));
        let model_room = model.saturating_sub(u128::from(peak));
        for k in n.saturating_add(1)..=self.note.most {
            let (own, grown) = work.triangles(k);
            let more = grown - a;
            let why = if own > part {
                Some(PART_LIMIT)
            } else if before + own > model || share * more > model_room * a {
                Some(MODEL_LIMIT)
            } else if self.tight.is_some_and(|(room, s, _)| u128::from(s) * grown > u128::from(room) * a) {
                self.tight.map(|t| t.2)
            } else if budget.is_some_and(|(limit, has)| share * more > u128::from(limit.saturating_sub(has)) * a) {
                Some(BUDGET_LIMIT)
            } else if work.at(k) > u128::from(crate::work::MAX_BUILD) {
                Some(WORK_LIMIT)
            } else {
                None
            };
            if let Some(why) = why {
                return (k - 1, why);
            }
        }
        (self.note.most.max(n), "its range")
    }

    /// What bounds the build's work at another count ([`Spent`]): `held` is the model-wide
    /// charges its elements are in.
    fn work_in(&self, spent: &Spent, held: Held) -> WorkIn {
        WorkIn {
            n: self.note.count_value,
            halves: matches!(self.note.count, "sides" | "segments" | "rings"),
            least: self.note.least,
            grows: self.note.grows,
            alone: self.note.alone,
            made: self.made,
            own: self.work,
            held,
            total: spent.total,
            lods: u128::from(spent.unsimplified) * u128::from(crate::work::SIMPLIFY_TRIANGLE) * u128::from(spent.triangles),
            coarse: spent.coarse,
        }
    }

    /// What it is in the finished model.
    fn measure(&self, path: String, peak: u64, budget: Option<(u64, u64)>, spent: &Spent, held: Held) -> Round {
        let n = &self.note;
        // (As built: a ring's and a sphere's faces measured one by one; any other's from its
        // note.)
        let sphere = |m: &M3| {
            let (turn, far, sag) = sphere_under(m, n.radius, n.count_value);
            Under { faces: Seen { turn, radius: far, sag }, far, rim: None }
        };
        let built = match (&self.ring, n.what) {
            (Some(ring), _) => ring.under(&IDENTITY, n.count_value),
            (None, "sphere") => sphere(&IDENTITY),
            _ => Under { faces: Seen { turn: n.turn_deg, radius: n.radius, sag: sag_of(n.radius, n.turn_deg) }, far: n.radius, rim: None },
        };
        let work = self.work_in(spent, held);
        let (most, most_by) = self.most_count(peak, budget, &work);
        // (Where its far level's cost at another count is not known, the work it is held to is
        // estimated: said only where that is what may bind, past the count built.)
        let estimate = self.estimate.or_else(|| matches!(spent.coarse, Coarse::Unknown { .. }).then_some(COARSE_ESTIMATE).filter(|_| most > n.count_value || most_by == WORK_LIMIT));
        let rim = |s: Seen| Rim { turn_deg: s.turn as f32, radius_m: s.radius as f32, sag: s.sag as f32 };
        let as_built = |unmeasured: Option<&'static str>| Round {
            path: path.clone(),
            what: n.what,
            count: n.count,
            count_value: n.count_value,
            turn_deg: built.faces.turn as f32,
            radius_m: built.far as f32,
            turn_radius_m: built.faces.radius as f32,
            sag: built.faces.sag as f32,
            rim: built.rim.map(rim),
            least: n.least,
            most,
            most_by,
            most_estimated: estimate,
            displaced_m: self.displaced as f32,
            unmeasured,
            recount: Recount { work: Some(work.clone()), ..Recount::default() },
        };
        if self.unknown.is_some() {
            return as_built(self.unknown);
        }
        let mut all = Under { faces: Seen::default(), far: 0.0, rim: built.rim.map(|_| Seen::default()) };
        let factors: Vec<(Option<f64>, Option<&M3>)> = match self.many {
            Some(f) => vec![(Some(f), None)],
            None => self.maps.iter().map(|m| (similarity(m), Some(m))).collect(),
        };
        let mut maps: Vec<M3> = Vec::new();
        let mut uneven = false;
        for (f, m) in factors {
            let u = match (f, m, &self.ring) {
                // (A similarity keeps every angle: as built, scaled.)
                (Some(f), _, _) => {
                    if !maps.contains(&IDENTITY) {
                        maps.push(IDENTITY);
                    }
                    built.scaled(f)
                }
                (None, Some(m), Some(ring)) => {
                    uneven = true;
                    maps.push(*m);
                    ring.under(m, n.count_value)
                }
                (None, Some(m), None) if n.what == "sphere" => {
                    uneven = true;
                    maps.push(*m);
                    sphere(m)
                }
                _ => return as_built(Some("stretched unevenly (only faces round a ring's axis and a sphere's are measured so)")),
            };
            if !u.finite() {
                return as_built(Some("flattened by a map (not measured)"));
            }
            all.take(&u);
        }
        let kind = match (&self.ring, n.what, n.count) {
            (Some(ring), _, _) => Again::Ring(ring.clone()),
            (None, "sphere", _) => Again::Sphere(n.radius),
            (None, _, "resolution") if !uneven => Again::Cell(dmath::sin_f64((0.5 * n.turn_deg).to_radians()) * f64::from(n.count_value)),
            (None, _, "rings") if !uneven => Again::Even(90.0),
            (None, _, "sides") if !uneven => Again::Even(360.0),
            _ => Again::No,
        };
        let recount = Recount { kind, maps, work: Some(work.clone()) };
        Round {
            turn_deg: all.faces.turn as f32,
            radius_m: all.far as f32,
            turn_radius_m: all.faces.radius as f32,
            sag: all.faces.sag as f32,
            rim: all.rim.map(rim),
            recount,
            ..as_built(None)
        }
    }
}

/// How far facets turn, at what radius, and stand in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Seen {
    pub turn: f64,
    pub radius: f64,
    pub sag: f64,
}

/// A round shape under one map: its faces (the radius: of the band that turns the most), its
/// largest radius, and a ring's widest rim seen down its axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Under {
    pub faces: Seen,
    pub far: f64,
    pub rim: Option<Seen>,
}

impl Under {
    fn nan() -> Under {
        let x = Seen { turn: f64::NAN, radius: f64::NAN, sag: f64::NAN };
        Under { faces: x, far: f64::NAN, rim: Some(x) }
    }

    fn scaled(&self, f: f64) -> Under {
        let s = |x: Seen| Seen { turn: x.turn, radius: x.radius * f, sag: x.sag * f };
        Under { faces: s(self.faces), far: self.far * f, rim: self.rim.map(s) }
    }

    fn finite(&self) -> bool {
        let ok = |x: &Seen| x.turn.is_finite() && x.radius.is_finite() && x.sag.is_finite();
        ok(&self.faces) && self.far.is_finite() && self.rim.as_ref().is_none_or(ok)
    }

    /// The worst of two copies: the larger turn (with its radius), radius and stand-in.
    fn take(&mut self, u: &Under) {
        if u.faces.turn > self.faces.turn {
            self.faces.turn = u.faces.turn;
            self.faces.radius = u.faces.radius;
        }
        self.faces.sag = self.faces.sag.max(u.faces.sag);
        self.far = self.far.max(u.far);
        if let (Some(a), Some(b)) = (&mut self.rim, u.rim) {
            *a = Seen { turn: a.turn.max(b.turn), radius: a.radius.max(b.radius), sag: a.sag.max(b.sag) };
        }
    }
}

/// The inverse transpose of `m`, which carries a plane's normal through it (`None` for a map
/// that flattens).
fn normal_map(m: &M3) -> Option<M3> {
    let det = dot(m[0], cross(m[1], m[2]));
    if !(det.is_finite() && det.abs() > 1e-300) {
        return None;
    }
    Some([cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1])].map(|c| c.map(|x| x / det)))
}

/// Most profile edges a ring keeps (a lathe has at most 512 rows; past this, the widest are
/// kept: a capped count says so in [`Ring::of`]'s caller by nothing, since no recipe reaches it).
const MAX_PROFILE: usize = 1024;

/// A ring's faces round its own Y axis, as built: where its vertices sit round the axis and
/// the profile they are spun from. Every band of faces is the profile edge between two rows
/// spun round the axis in `sides` steps; its faces are planar (an isosceles trapezoid, or a
/// triangle at the axis), so each is known exactly from its edge, its step and its angle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ring {
    /// What it is (its builder decides where its first vertex sits).
    what: &'static str,
    /// Its count as built, and its first vertex's angle about Y then, radians (measured on the
    /// built mesh).
    sides: u32,
    phase: f64,
    /// The largest radius any of its vertices has (the record's radius).
    radius: f64,
    /// Its widest rim's radius: the furthest of its column's vertices from the axis (its
    /// outline seen down the axis is that rim's polygon, every rim being one like it, smaller).
    rim: f64,
    /// The profile's edges that turn round the axis, as `[ρ0, y0, ρ1, y1]`, one for each
    /// direction (the widest of those alike: faces of one direction turn alike, and the
    /// widest stands in furthest).
    prof: Vec<[f64; 4]>,
}

/// The builder's first angle round the axis for `sides` (a cylinder's and a cone's polygon
/// starts at 90° and half a side; a lathe's, a capsule's and a torus's at 0).
fn first_angle(what: &str, sides: u32) -> f64 {
    match what {
        "cylinder" | "cone" => (90.0 + 180.0 / f64::from(sides.max(3))).to_radians(),
        _ => 0.0,
    }
}

impl Ring {
    /// The ring of `sides` that `s` (a shape round its own Y axis, as built) is: its first
    /// vertex's angle and its profile, from the column of vertices at that angle (and on the
    /// axis): every edge of a triangle with both ends there is an edge of the profile.
    pub(crate) fn of(s: &Shape, what: &'static str, sides: u32, radius: f64) -> Ring {
        let phase = phase_of(s);
        let rho_max = s.positions.iter().map(|p| dmath::hypot_f64(f64::from(p[0]), f64::from(p[2]))).fold(0.0f64, f64::max);
        let (sp, cp) = dmath::sin_cos_f64(phase);
        // (Where each vertex is: in the column, on the axis, or elsewhere; its radius and height.)
        let at: Vec<Option<[f64; 2]>> = s
            .positions
            .iter()
            .map(|p| {
                let (x, z) = (f64::from(p[0]), f64::from(p[2]));
                let rho = dmath::hypot_f64(x, z);
                if rho <= 1e-5 * rho_max {
                    return Some([0.0, f64::from(p[1])]);
                }
                // (In the column: along the first vertex's direction, to the precision of the
                // stored position.)
                let along = x * cp - z * sp;
                let across = x * sp + z * cp;
                (along > 0.0 && across.abs() <= 1e-5 * rho).then_some([rho, f64::from(p[1])])
            })
            .collect();
        let mut prof: Vec<[f64; 4]> = Vec::new();
        for t in &s.tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                if let (Some(p), Some(q)) = (at[a as usize], at[b as usize]) {
                    if p[0] > 0.0 || q[0] > 0.0 {
                        let (p, q) = if (p[1], p[0]) <= (q[1], q[0]) { (p, q) } else { (q, p) };
                        prof.push([p[0], p[1], q[0], q[1]]);
                    }
                }
            }
        }
        // (One edge for each direction, the widest: by its unit direction, then its width.)
        let dir = |e: &[f64; 4]| {
            let (dr, dy) = (e[2] - e[0], e[3] - e[1]);
            let l = dmath::hypot_f64(dr, dy);
            if l > 0.0 {
                [dr / l, dy / l]
            } else {
                [0.0, 0.0]
            }
        };
        prof.retain(|e| {
            let d = dir(e);
            // (A flat edge's faces lie in one plane: they never turn round the axis.)
            d[1].abs() > 1e-9
        });
        prof.sort_by(|a, b| {
            let (da, db) = (dir(a), dir(b));
            da[0].total_cmp(&db[0]).then(da[1].total_cmp(&db[1]))
        });
        // (Alike to the precision of the stored positions; of those alike, the widest.)
        let mut merged: Vec<[f64; 4]> = Vec::with_capacity(prof.len());
        for e in prof {
            match merged.last_mut() {
                Some(k) if (dir(k)[0] - dir(&e)[0]).abs() <= 1e-9 && (dir(k)[1] - dir(&e)[1]).abs() <= 1e-9 => {
                    if e[0].max(e[2]) > k[0].max(k[2]) {
                        *k = e;
                    }
                }
                _ => merged.push(e),
            }
        }
        let mut prof = merged;
        if prof.len() > MAX_PROFILE {
            prof.sort_by(|a, b| b[0].max(b[2]).total_cmp(&a[0].max(a[2])));
            prof.truncate(MAX_PROFILE);
        }
        let rim = at.iter().flatten().map(|p| p[0]).fold(0.0f64, f64::max);
        Ring { what, sides, phase, radius, rim, prof }
    }

    /// Its first vertex's angle at `count` (the builder's start, less what was measured as
    /// built).
    fn phase_at(&self, count: u32) -> f64 {
        if count == self.sides {
            return self.phase;
        }
        let step = std::f64::consts::TAU / f64::from(self.sides.max(3));
        let off = self.phase - first_angle(self.what, self.sides);
        let off = off - step * dmath::round_f64(off / step);
        first_angle(self.what, count) + off
    }

    /// Put through `m` with `count` faces round its axis: the largest turn between neighbouring
    /// faces round it (degrees) with the radius of the band that turns so, its largest radius,
    /// the furthest any face stands in from the true surface of its band (each band's surface
    /// of revolution, carried through `m`), and its widest rim seen down the mapped axis.
    pub(crate) fn under(&self, m: &M3, count: u32) -> Under {
        self.under_at(m, count, self.phase_at(count))
    }

    fn under_at(&self, m: &M3, count: u32, phase: f64) -> Under {
        let n = count.max(3) as usize;
        let step = std::f64::consts::TAU / n as f64;
        let Some(w) = normal_map(m) else { return Under::nan() };
        let half = dmath::sin_f64(0.5 * step);
        let c2 = dmath::cos_f64(0.5 * step);
        // (1 − cos of half a step, without the cancellation.)
        let one_less = 2.0 * dmath::sin_f64(0.25 * step) * dmath::sin_f64(0.25 * step);
        let mids: Vec<(f64, f64)> = (0..n).map(|k| dmath::sin_cos_f64(phase + step * (k as f64 + 0.5))).collect();
        // Seen down the mapped axis: every point less its part along the axis's image (a rim's
        // points at one height lie in a plane the map carries to one plane, and seen along the
        // axis every rim falls on the same polygon scaled by its radius: the widest is the
        // outline). The rim's corners `u` (at radius 1), and the curve's point between each two.
        let axis = apply(m, [0.0, 1.0, 0.0]);
        let al = norm(axis);
        if !(al > 0.0 && al.is_finite()) {
            return Under::nan();
        }
        let d = axis.map(|x| x / al);
        let seen = |v: [f64; 3]| {
            let t = dot(v, d);
            [v[0] - t * d[0], v[1] - t * d[1], v[2] - t * d[2]]
        };
        let corner = |&(s, c): &(f64, f64)| {
            // (The vertices are at the faces' middles less half a step.)
            let (sv, cv) = (s * c2 - c * half, c * c2 + s * half);
            [cv, sv]
        };
        let (mut far, mut reach) = (0.0f64, 0.0f64);
        let mut u: Vec<[f64; 3]> = Vec::with_capacity(n);
        for mid in &mids {
            let [cv, sv] = corner(mid);
            let v = apply(m, [cv, 0.0, -sv]);
            far = far.max(self.radius * norm(v));
            let p = seen(v);
            reach = reach.max(norm(p));
            u.push(p);
        }
        // The rim: its outline's turn at each corner, and how far the curve between two corners
        // stands out of the side between them (at the arc's middle, where the curve runs
        // parallel to the side: an affine map keeps that), at the rim's radius.
        let mut rim = Seen { turn: 0.0, radius: self.rim * reach, sag: 0.0 };
        for k in 0..n {
            let (a, b, c) = (u[(k + n - 1) % n], u[k], u[(k + 1) % n]);
            let (e0, e1) = (sub3(b, a), sub3(c, b));
            rim.turn = rim.turn.max(dmath::atan2_f64(norm(cross(e0, e1)), dot(e0, e1)).to_degrees());
            let (s, cc) = mids[k];
            let q = seen(apply(m, [cc, 0.0, -s]));
            let l = norm(e1);
            if l > 0.0 {
                rim.sag = rim.sag.max(self.rim * norm(cross(e1, sub3(q, b))) / l);
            }
        }
        let (mut turn, mut turn_radius, mut sag) = (0.0f64, 0.0f64, 0.0f64);
        for e in &self.prof {
            // A face of this band at middle angle μ: its edges at the rims are chords, at
            // ρ·cos(half a step) from the axis; its normal in the (outward, up) plane at μ is
            // square to the chord-to-chord line.
            let (dr, dy) = ((e[2] - e[0]) * c2, e[3] - e[1]);
            let l = dmath::hypot_f64(dr, dy);
            if l <= 0.0 {
                continue;
            }
            let (a, b) = (dy / l, -dr / l);
            if a.abs() <= 1e-12 {
                continue;
            }
            // (How far its true surface stands out of it as built: at the wider rim, the arc's
            // middle less the chord's, along the normal.)
            let gap = e[0].max(e[2]) * a.abs() * one_less;
            let normal = |&(s, c): &(f64, f64)| apply(&w, [a * c, b, -a * s]);
            let mut prev = normal(&mids[n - 1]);
            let mut band = 0.0f64;
            for mid in &mids {
                let cur = normal(mid);
                let l = norm(cur);
                if l > 0.0 {
                    sag = sag.max(gap / l);
                }
                band = band.max(dmath::atan2_f64(norm(cross(prev, cur)), dot(prev, cur)).to_degrees());
                prev = cur;
            }
            // (The band that turns the most, and its wider rim's reach from the axis.)
            if band > turn {
                turn = band;
                turn_radius = e[0].max(e[2]) * reach;
            }
        }
        Under { faces: Seen { turn, radius: turn_radius, sag }, far, rim: Some(rim) }
    }
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// A sphere of `radius` and `sub` subdivisions (as [`Shape::sphere`] builds it) put through
/// `m`: the largest turn between two faces that share an edge (degrees), its largest radius,
/// and the furthest any face stands in from the mapped sphere (an ellipsoid). That is exact:
/// a face's furthest point of the curve is where the curve's tangent plane is parallel to the
/// face, and an affine map keeps that, so it is the mapped sphere's point along the face's
/// own normal before the map.
pub(crate) fn sphere_under(m: &M3, radius: f64, sub: u32) -> (f64, f64, f64) {
    let s = Shape::sphere(1.0, sub.min(5));
    // (Centred: the sphere rests on y = 0.)
    let c = s.positions.iter().fold([0.0f64; 3], |a, p| [a[0] + f64::from(p[0]), a[1] + f64::from(p[1]), a[2] + f64::from(p[2])]).map(|x| x / s.positions.len().max(1) as f64);
    let local: Vec<[f64; 3]> = s.positions.iter().map(|p| [f64::from(p[0]) - c[0], f64::from(p[1]) - c[1], f64::from(p[2]) - c[2]]).collect();
    let image: Vec<[f64; 3]> = local.iter().map(|p| apply(m, p.map(|x| x * radius))).collect();
    let sub3 = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (mut sag, mut far) = (0.0f64, 0.0f64);
    let mut normals = Vec::with_capacity(s.tris.len());
    for t in &s.tris {
        let [a, b, cc] = t.map(|i| i as usize);
        let n0 = cross(sub3(local[b], local[a]), sub3(local[cc], local[a]));
        let n0 = n0.map(|x| x / norm(n0));
        let n1 = cross(sub3(image[b], image[a]), sub3(image[cc], image[a]));
        let n1 = n1.map(|x| x / norm(n1));
        let top = apply(m, n0.map(|x| x * radius));
        sag = sag.max(dot(n1, sub3(top, image[a])).abs());
        far = far.max(norm(image[a]));
        normals.push(n1);
    }
    // (Faces that share an edge, by their corners' numbers: the sphere's corners are shared.)
    let mut edges: Vec<(u32, u32, usize)> = s.tris.iter().enumerate().flat_map(|(i, t)| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])].map(|(a, b)| (a.min(b), a.max(b), i))).collect();
    edges.sort_unstable();
    let mut turn = 0.0f64;
    for w in edges.windows(2) {
        if (w[0].0, w[0].1) == (w[1].0, w[1].1) {
            let (p, q) = (normals[w[0].2], normals[w[1].2]);
            turn = turn.max(dmath::atan2_f64(norm(cross(p, q)), dot(p, q)).to_degrees());
        }
    }
    (turn, far, sag)
}

/// The angle about Y of the vertex of `s` (on triangles of `slot`, or all when it has no
/// owners) furthest from the Y axis: a ring's first vertex, up to a turn of one side.
pub(crate) fn phase_of(s: &Shape) -> f64 {
    let mut best = (-1.0f64, 0.0f64);
    for p in &s.positions {
        let r = f64::from(p[0]) * f64::from(p[0]) + f64::from(p[2]) * f64::from(p[2]);
        if r > best.0 * (1.0 + 1e-9) {
            best = (r, dmath::atan2_f64(-f64::from(p[2]), f64::from(p[0])));
        }
    }
    best.1
}

/// The slots `owners` holds, sorted, once each.
pub(crate) fn slots_in(owners: &[u32]) -> Vec<u32> {
    let mut v: Vec<u32> = owners.iter().copied().filter(|&o| o != NO_OWNER).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// What the round shapes come to in the finished model (`shapes`): one [`Round`] for each
/// with a triangle of its own left, how many there are, and how many of them were not
/// measured (both counted whatever is kept). Kept **in the order the facts answer by**: every
/// round shape that is the answer at some size of the asset (the coarsest turn among those
/// standing in at least so far: the shapes no other both stands in further and turns more
/// than), the furthest standing first; then the rest by how far they stand in; the ones not
/// measured are kept before both, up to eight. At most [`crate::MAX_ROUND`] are kept, except
/// that the answers are all kept, however many.
///
/// `totals` is the build's log of the model's triangles at each check (a record's most count
/// holds the greatest since it was made), `budget` the recipe's limit on the finished model's
/// triangles with what it has.
///
/// `spent`: the build's work ([`Spent`]), with its parts in the order of `shapes`.
pub(crate) fn finish(recs: &[Rec], paths: &[String], shapes: &[&Shape], totals: &[u64], budget: Option<(u64, u64)>, spent: &Spent) -> (Vec<Round>, usize, usize) {
    if recs.is_empty() {
        return (Vec::new(), 0, 0);
    }
    // (Which parts hold each slot's triangles, and what the model-wide charges cost for them.)
    let top = recs.iter().map(|r| r.slot as usize + 1).max().unwrap_or(0);
    let mut held = vec![Held::default(); top];
    let simplify = spent.levels.saturating_mul(crate::work::SIMPLIFY_TRIANGLE);
    for (s, p) in shapes.iter().zip(&spent.parts) {
        let cost = p
            .vertices
            .saturating_mul(crate::work::ASSEMBLE_VERTEX)
            .saturating_add(p.triangles.saturating_mul(simplify))
            .saturating_add(if p.painted { p.vertices.saturating_mul(spent.strokes).saturating_mul(crate::work::PAINT_VERTEX) } else { 0 });
        for slot in slots_in(&s.owners) {
            if let Some(h) = held.get_mut(slot as usize) {
                h.cost = h.cost.saturating_add(cost);
                h.vertices = h.vertices.saturating_add(p.vertices);
                h.triangles = h.triangles.saturating_add(p.triangles);
                h.assembled = h.assembled.saturating_add(p.assembled);
            }
        }
    }
    for h in &mut held {
        h.per_point = spent.per_point;
    }
    // (The greatest of the log from each place on.)
    let mut peak_from = vec![0u64; totals.len() + 1];
    for i in (0..totals.len()).rev() {
        peak_from[i] = peak_from[i + 1].max(totals[i]);
    }
    let slots = recs.iter().map(|r| r.slot as usize + 1).max().unwrap_or(0);
    let mut present = vec![false; slots];
    for s in shapes {
        for (t, &o) in s.tris.iter().zip(&s.owners) {
            if let Some(p) = present.get_mut(o as usize) {
                if !*p && crate::made::triangle_area(s, *t) > 0.0 {
                    *p = true;
                }
            }
        }
    }
    let mut all: Vec<Round> = recs
        .iter()
        .filter(|r| present.get(r.slot as usize).copied().unwrap_or(false))
        .map(|r| {
            let base = paths.get(r.record as usize).cloned().unwrap_or_default();
            let path = match r.note.primitive {
                Some(i) => format!("{base}.tree#{i}"),
                None => base,
            };
            r.measure(path, peak_from[r.since.min(totals.len())], budget, spent, held.get(r.slot as usize).copied().unwrap_or_default())
        })
        .collect();
    let total = all.len();
    let not_measured = all.iter().filter(|r| !r.measured()).count();
    // (Not measured first, up to eight; then the front by stand-in; then the rest.)
    let mut out: Vec<Round> = Vec::new();
    let mut rest: Vec<Round> = Vec::new();
    for r in all.drain(..) {
        if !r.measured() {
            if out.len() < 8 {
                out.push(r);
            }
        } else {
            rest.push(r);
        }
    }
    rest.sort_by(|a, b| b.sag.total_cmp(&a.sag).then(b.turn_deg.total_cmp(&a.turn_deg)).then(a.path.cmp(&b.path)));
    let mut front = Vec::new();
    let mut others = Vec::new();
    let mut best_turn = f32::NEG_INFINITY;
    for r in rest {
        if r.turn_deg > best_turn {
            best_turn = r.turn_deg;
            front.push(r);
        } else {
            others.push(r);
        }
    }
    // The front is kept whole, past the bound if it must be: every record on it is the answer
    // at some size of the asset, so leaving one out could change the answer. (Past the bound a
    // record keeps no means of being measured again: its advice says so. Only a contrived
    // recipe has a front that long: a turn that rises as the stand-in falls, record by record.)
    let room = crate::recipe::MAX_ROUND.saturating_sub(out.len());
    for (i, mut r) in front.into_iter().enumerate() {
        if i >= room {
            r.recount = Recount::default();
        }
        out.push(r);
    }
    let room = crate::recipe::MAX_ROUND.saturating_sub(out.len());
    out.extend(others.into_iter().take(room));
    (out, total, not_measured)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(v: [f64; 3]) -> M3 {
        [[v[0], 0.0, 0.0], [0.0, v[1], 0.0], [0.0, 0.0, v[2]]]
    }

    /// `ρ(k)` is never less than the counts' ratio for a ring (whose vertices may grow faster
    /// than its triangles where its triangles have a fixed part, as a capsule's ends' do) nor
    /// than `4^(k−n)` for a sphere; and the bound at the count built is the work spent there.
    #[test]
    fn the_growth_takes_the_counts_ratio() {
        let w = |grows, alone, made| WorkIn { n: 10, halves: true, least: 3, grows, alone, made, own: 1000, held: Held::default(), total: 5000, lods: 0, coarse: Coarse::Never };
        // (Triangles 2k + 100: from 10 to 20 they grow by 140/120, the count by 2.)
        let ring = w(Grows::Linear { per: 2, fixed: 100 }, Some((3, 106)), 120);
        let (p, q) = ring.growth(20);
        assert!(p * 10 >= q * 20, "{p}/{q}");
        assert_eq!(ring.at(10), 5000);
        assert_eq!(ring.at(20), 5000 + (1000 * (p - q)).div_ceil(q));
        let sphere = w(Grows::Fourfold, None, 20 << 20);
        let (p, q) = WorkIn { n: 1, ..sphere }.growth(3);
        assert!(p >= 16 * q, "{p}/{q}");
    }

    /// **What the work bound's `ρ(k)` rests on for a ring** ([`WorkIn::growth`]): every ring's
    /// builder makes vertices that grow from one count to another by no more than the greater
    /// of its triangles' growth and the counts' ratio, as built, once subdivided (whose new
    /// points are its edges) and once welded; so every charge by a count of its elements
    /// grows by no more than `ρ(k)`. Held at every pair of counts in a sweep of each kind.
    #[test]
    fn a_rings_vertices_grow_no_faster_than_its_count() {
        let prof = [[0.0, 0.0], [0.5, 0.0], [0.42, 0.15], [0.2, 0.5], [0.06, 1.0], [0.0, 1.05]];
        let open = [[0.5, 0.0], [0.4, 0.5], [0.45, 1.0]];
        type Kind = (&'static str, Box<dyn Fn(u32) -> Shape>, u32, u32);
        let kinds: Vec<Kind> = vec![
            ("cylinder", Box::new(|k| Shape::cylinder(0.5, 0.5, 1.0, k, 0.0).unwrap()), 3, 60),
            ("cone", Box::new(|k| Shape::cylinder(0.5, 0.0, 1.0, k, 0.0).unwrap()), 3, 60),
            ("bevelled cylinder", Box::new(|k| Shape::cylinder(0.5, 0.3, 1.0, k, 0.05).unwrap()), 3, 60),
            ("torus segments", Box::new(|k| Shape::torus(0.6, 0.2, k, 7)), 3, 60),
            ("torus sides", Box::new(|k| Shape::torus(0.6, 0.2, 9, k)), 3, 40),
            ("capsule segments", Box::new(|k| Shape::capsule(0.3, 1.5, k, 3)), 3, 60),
            ("capsule rings", Box::new(|k| Shape::capsule(0.3, 1.5, 9, k)), 1, 32),
            ("lathe", Box::new(move |k| Shape::lathe(&prof, k, true)), 3, 60),
            ("open lathe", Box::new(move |k| Shape::lathe(&open, k, false)), 3, 60),
        ];
        for (what, make, lo, hi) in kinds {
            let sizes = |s: Shape| {
                let plain = (s.triangle_count() as u128, s.positions.len() as u128);
                let mut sub = s.clone();
                sub.subdivide(1);
                let mut welded = s.clone();
                welded.weld(1e-4);
                [plain, (sub.triangle_count() as u128, sub.positions.len() as u128), (welded.triangle_count() as u128, welded.positions.len() as u128)]
            };
            let all: Vec<[(u128, u128); 3]> = (lo..=hi).map(|k| sizes(make(k))).collect();
            for n in lo..hi {
                for k in n + 1..=hi {
                    for form in 0..3 {
                        let (tn, vn) = all[(n - lo) as usize][form];
                        let (tk, vk) = all[(k - lo) as usize][form];
                        // (V(k)/V(n) ≤ max(T(k)/T(n), k/n).)
                        let by_tris = vk * tn <= vn * tk;
                        let by_count = vk * u128::from(n) <= vn * u128::from(k);
                        assert!(by_tris || by_count, "{what} ({}): {vn} vertices at {n}, {vk} at {k}; {tn} and {tk} triangles", ["as built", "subdivided", "welded"][form]);
                    }
                }
            }
        }
    }

    /// Under a similarity a ring is as built, scaled; under a scale that stretches one way
    /// it is an ellipse, whose sides near its ends turn more and whose middle sides stand in
    /// further: held against the same ring's polygon measured point by point here.
    /// A wall of `sides` round Y at radius 1 (a lathe's: its first vertex at angle 0).
    fn wall(sides: u32) -> Ring {
        Ring::of(&Shape::lathe(&[[1.0, 0.0], [1.0, 1.0]], sides, true), "lathe", sides, 1.0)
    }

    /// A ring's faces' turn, its largest radius and its faces' stand-in; for a wall its rim seen
    /// down its axis is its section, so the rim's turn and stand-in are its faces'.
    fn faces(ring: &Ring, m: &M3, count: u32) -> (f64, f64, f64) {
        let u = ring.under(m, count);
        let rim = u.rim.unwrap();
        assert!((rim.turn - u.faces.turn).abs() < 1e-9 && (rim.sag - u.faces.sag).abs() < 1e-9 * u.faces.sag.max(1.0), "a wall's rim is its section: {u:?}");
        (u.faces.turn, u.far, u.faces.sag)
    }

    #[test]
    fn a_ring_under_a_map_is_measured_exactly() {
        let (t, r, s) = faces(&wall(6), &scale([2.0; 3]), 6);
        assert!((t - 60.0).abs() < 1e-9 && (r - 2.0).abs() < 1e-6 && (s - 2.0 * sag_of(1.0, 60.0)).abs() < 1e-6, "{t} {r} {s}");
        // Stretched along its axis only: the ring is untouched.
        let (t, r, s) = faces(&wall(12), &scale([1.0, 5.0, 1.0]), 12);
        assert!((t - 30.0).abs() < 1e-9 && (r - 1.0).abs() < 1e-6 && (s - sag_of(1.0, 30.0)).abs() < 1e-6);
        // Stretched across it by three: an ellipse of 3 by 1 drawn with 12 points.
        let (t, r, s) = faces(&wall(12), &scale([3.0, 1.0, 1.0]), 12);
        let pts: Vec<(f64, f64)> = (0..12).map(|k| (3.0 * dmath::cos_f64(k as f64 * 30f64.to_radians()), dmath::sin_f64(k as f64 * 30f64.to_radians()))).collect();
        let mut turn: f64 = 0.0;
        for k in 0..12 {
            let (a, b, c) = (pts[(k + 11) % 12], pts[k], pts[(k + 1) % 12]);
            let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
            turn = turn.max(dmath::atan2_f64(u.0 * v.1 - u.1 * v.0, u.0 * v.0 + u.1 * v.1).to_degrees());
        }
        // (The furthest stand-in, by a fine search along each mapped arc.)
        let mut far: f64 = 0.0;
        for k in 0..12 {
            let (a, b) = (pts[k], pts[(k + 1) % 12]);
            for j in 0..=1000 {
                let th = (k as f64 + j as f64 / 1000.0) * 30f64.to_radians();
                let q = (3.0 * dmath::cos_f64(th), dmath::sin_f64(th));
                let d = ((b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0)).abs() / dmath::hypot_f64(b.0 - a.0, b.1 - a.1);
                far = far.max(d);
            }
        }
        assert!((t - turn).abs() < 1e-6 && (r - 3.0).abs() < 1e-6 && (s - far).abs() < 1e-6 * far, "{t} against {turn}; {s} against {far}");
        assert!(t > 50.0 && s > sag_of(1.0, 30.0) * 2.0, "the ends of the ellipse turn more than a twelfth of a turn");
    }

    /// A sphere measured face by face: as built its largest turn is about the nominal one (41.8°
    /// over two to the subdivisions, and up to a fifth more once subdivided) and its faces stand
    /// in by at least what that turn says; stretched to an egg twice as tall its faces turn more and stand in further,
    /// by no more than the stretch.
    #[test]
    fn a_sphere_under_a_map_is_measured_face_by_face() {
        for sub in [0u32, 1, 2, 3] {
            let (t, r, s) = sphere_under(&IDENTITY, 0.5, sub);
            let want = 41.81 / f64::from(1u32 << sub);
            // (A face's middle stands in further than its edges: the twenty faces of none stand
            // in by a fifth of the radius, three times what the turn at an edge alone says, and
            // a subdivided one's by about four times.)
            assert!(t >= want - 0.01 && t < 1.2 * want && (r - 0.5).abs() < 1e-6 && s >= sag_of(0.5, want) && s < 0.25 * 0.5, "sub {sub}: {t} {r} {s}");
            let (t2, r2, s2) = sphere_under(&scale([1.0, 2.0, 1.0]), 0.5, sub);
            assert!(t2 > t && (r2 - 1.0).abs() < 1e-6 && s2 > s && s2 <= 2.0 * s * 1.0001, "sub {sub}: egg {t2} {r2} {s2}");
        }
    }

    /// Where a ring's first vertex sits at another count is the builder's own start (the advice
    /// measures a count it has not built): held against each builder at counts round the one
    /// built, odd and even; and a ring's profile is what it was built of.
    #[test]
    fn a_ring_knows_where_it_starts_at_any_count() {
        type Make = fn(u32) -> Shape;
        let shapes: [(&'static str, Make); 5] = [
            ("cylinder", |n| Shape::cylinder(0.5, 0.5, 1.0, n, 0.0).unwrap()),
            ("cone", |n| Shape::cylinder(0.5, 0.0, 1.0, n, 0.0).unwrap()),
            ("lathe", |n| Shape::lathe(&[[0.0, 0.0], [0.5, 0.2], [0.3, 1.0]], n, false)),
            ("capsule", |n| Shape::capsule(0.4, 2.0, n, 3)),
            ("torus", |n| Shape::torus(0.6, 0.2, n, 5)),
        ];
        for (what, make) in shapes {
            for built in [3u32, 4, 7, 12, 31] {
                let ring = Ring::of(&make(built), what, built, 0.5);
                assert!(!ring.prof.is_empty(), "{what} {built}");
                for other in [3u32, 5, 8, 13, 64, 101] {
                    let truth = Ring::of(&make(other), what, other, 0.5).phase;
                    let step = std::f64::consts::TAU / f64::from(other);
                    let d = ring.phase_at(other) - truth;
                    assert!((d - step * dmath::round_f64(d / step)).abs() < 1e-5, "{what} built {built}, at {other}: {} against {truth}", ring.phase_at(other));
                }
            }
        }
        // A wall turns a full side face to face; a cone's faces, sloped, turn less:
        // 2·asin(cos a · sin(180°/n)), its faces sloped by a from the axis.
        let cone = Ring::of(&Shape::cylinder(0.5, 0.0, 1.0, 4, 0.0).unwrap(), "cone", 4, 0.5);
        let u = cone.under(&IDENTITY, 4);
        let (t, s) = (u.faces.turn, u.faces.sag);
        let ca = 1.0 / (1.0f64 + (0.5 * dmath::cos_f64(std::f64::consts::FRAC_PI_4)) * (0.5 * dmath::cos_f64(std::f64::consts::FRAC_PI_4))).sqrt();
        let want = 2.0 * dmath::asin_f64(ca * dmath::sin_f64(std::f64::consts::FRAC_PI_4)).to_degrees();
        assert!((t - want).abs() < 1e-4 && (s - 0.5 * ca * (1.0 - dmath::cos_f64(std::f64::consts::FRAC_PI_4))).abs() < 1e-6, "{t} against {want}; {s}");
        // Seen from above its rim is a square: 90° at a corner, standing in 0.5·(1 − cos 45°).
        let rim = u.rim.unwrap();
        // (Its rim's radius is read off the built mesh's f32 points.)
        assert!((rim.turn - 90.0).abs() < 1e-9 && (rim.sag - 0.5 * (1.0 - dmath::cos_f64(std::f64::consts::FRAC_PI_4))).abs() < 1e-7 && (rim.radius - 0.5).abs() < 1e-6, "{rim:?}");
        // Stretched across by two and leant, from above it is a parallelogram: the rim's
        // corners seen down the mapped axis, measured point by point here.
        let m: M3 = mul(&[[1.0, 0.0, 0.0], [0.7, 1.0, -0.3], [0.0, 0.0, 1.0]], &scale([2.0, 1.0, 1.0]));
        let u = cone.under(&m, 4);
        let d = apply(&m, [0.0, 1.0, 0.0]);
        let d = d.map(|x| x / norm(d));
        let seen = |v: [f64; 3]| sub3(v, d.map(|x| x * dot(v, d)));
        let p: Vec<[f64; 3]> = (0..4).map(|k| seen(apply(&m, [0.5 * dmath::cos_f64(ring_angle(&cone, k)), 0.0, -0.5 * dmath::sin_f64(ring_angle(&cone, k))]))).collect();
        let mut turn = 0.0f64;
        for k in 0..4 {
            let (a, b) = (sub3(p[k], p[(k + 3) % 4]), sub3(p[(k + 1) % 4], p[k]));
            turn = turn.max(dmath::atan2_f64(norm(cross(a, b)), dot(a, b)).to_degrees());
        }
        assert!((u.rim.unwrap().turn - turn).abs() < 1e-6 && turn > 95.0, "{:?} against {turn}", u.rim);
    }

    /// The angle of a ring's `k`th vertex round its axis, as built.
    fn ring_angle(ring: &Ring, k: usize) -> f64 {
        ring.phase + std::f64::consts::TAU * k as f64 / f64::from(ring.sides)
    }

    #[test]
    fn a_similarity_is_told_apart() {
        let r = linear(&Affine::rotate_euler_deg([10.0, 33.0, -70.0]));
        assert!(similarity(&r).is_some_and(|f| (f - 1.0).abs() < 1e-6));
        assert!(similarity(&mul(&scale([2.0; 3]), &r)).is_some_and(|f| (f - 2.0).abs() < 1e-5));
        assert!(similarity(&mul(&scale([2.0, 1.0, 2.0]), &r)).is_none());
        assert!(similarity(&scale([-1.0, 1.0, 1.0])).is_some_and(|f| (f - 1.0).abs() < 1e-12), "a mirror");
        assert!((stretch(&mul(&scale([1.0, 4.0, 2.0]), &r)) - 4.0).abs() < 1e-6);
        assert_eq!(mul(&IDENTITY, &r), r);
    }
}
