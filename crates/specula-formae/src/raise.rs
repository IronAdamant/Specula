//! Raised: any closed plane figure of the table made a solid (two and a half dimensions, the
//! first sense: a token, a slab, a pillow, a relief).
//!
//! - [`slab`]: the figure with straight sides, a thickness.
//! - [`bevelled`]: a slab whose top edge (and, if asked, its bottom edge) is cut back by a width,
//!   flat (a chamfer) or round (a quarter circle).
//! - [`pillow`]: a slab whose top rises by a quarter ellipse from its rim to a flat middle: the
//!   height `rise · sin θ` at the inset `inset · (1 − cos θ)`, vertical at the rim and flat at
//!   the inset.
//! - [`stepped`]: terraces, each an inset outline a rise above the one before.
//! - [`relief`]: the figure raised by a height on a plate that is its bounds widened by a margin.
//!
//! Figures with holes are raised as figures without: every ring (the outer and each hole) is
//! inset into the figure. A cap with holes is made of triangles proved to tile it (`cap`).
//!
//! **Insets.** An outline inset by `s` is the figure's sampled outline with each corner moved
//! along its miter `m` (`m · n = 1` for the inward normal `n` of both edges at it) by `s`: every
//! edge stays parallel to itself and moves in by exactly `s`, so the faces between two insets
//! are trapezoids (planar) and the insets share rays from the outline's corners. An edge
//! shortens or lengthens linearly in `s`, and **an inset at or past the first edge's collapse,
//! or one whose rings do not stay simple, is refused** (`inset_too_deep`, with the deepest inset
//! that holds at that detail). Never a tangled mesh: what is built has passed the proof below.
//!
//! **Why what is built does not cross itself.** Each raised solid is the region between two
//! surfaces over the figure, the bottom `b` and the top `t`, with walls where they step. The
//! faces of the top that are not walls are checked, in one pass, each to turn counter-clockwise
//! in plan past rounding (the trapezoids, and the cap's triangles: see `cap`), or the cap's one
//! polygon to be simple; their plan outlines add up to the rim's (each inner edge is shared by
//! construction). As in `cap`, they then tile the figure once: the top is a graph over the
//! figure, and so is the bottom. The profile rises (`y` never falls) and its inset falls to the
//! rim and then grows, so `b <= t` over every point. A graph does not cross itself, and walls
//! stand over the rings, which the tiling keeps apart.
//!
//! **Measures.** A slab's are exact for any figure with closed measures (its volume `A t`). A
//! relief's too (plate and figure). For a figure of straight sides the others are exact: an
//! inset polygon's area is `A − P s + K s²` (`K` the sum of `tan(φ/2)` over its corners' signed
//! turns), so the volume `∫ A(s(y)) dy` is integrated in closed form along the profile, and the
//! surface `A(s₀) + A(s₁) + ∫ P(s) dℓ` with `P(s) = P − 2 K s` (the pillow's surface is an
//! ellipse's arc length, and is left out). Figures with curved sides have no closed form here
//! but the slab's and the relief's (measured as built otherwise).
//!
//! **The error.** For a slab and a relief, the figure's. For the others: the figure's chord
//! error, plus `s (sec(φ/2) − 1)` for the deepest inset `s` and the greatest turn `φ` at a
//! sample of a curved piece (an inset vertex of a sampled arc lies inside the true inset arc by
//! that), plus the profile's sag times the greatest miter's length (a profile stretched along a
//! miter strays no more than the stretch times its sag). A bound for figures of straight sides
//! (then only the profile's sag); for curved figures an estimate (the junctions of curved pieces
//! are not argued).

use crate::cap;
use crate::detail::segments;
use crate::error::Error;
use crate::geom::{Geom, Seg};
use crate::maths::*;
use crate::mesh::{Face, Mesh, SolidFault, Surface};
use crate::ops::Solid;
use crate::outline::Outline;
use crate::solid::Make;
use crate::spec::Spec;
use crate::table::{Kind, LEN_MAX, LEN_MIN};
use crate::Measures;
use std::rc::Rc;

/// The names of the raised forms, as a plane figure's kin may name them.
pub const RAISED: &[&str] = &["slab", "bevelled", "pillow", "stepped", "relief"];

/// The most terraces [`stepped`] makes.
pub const MAX_STEPS: u32 = 64;

/// The most builds a refusal for an inset too deep makes to find the deepest that holds (one at
/// the least length and forty halvings): a refused build of a bevelled, pillowed or stepped form
/// costs up to this many builds of its own size, unless it is limited
/// ([`crate::ops::Solid::with_search`]).
pub const SEARCH_BUILDS: u32 = 41;

/// One step of a profile in `(s, y)`: the inset `s` from the figure's outline and the height `y`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Step {
    /// A straight step to the point.
    Line(Pt),
    /// A quarter of the ellipse of centre `c` and radii `rs` (in `s`) and `ry` (in `y`), from the
    /// angle `t0` to `t1` (whole quarter turns), cut into the segments of a quarter turn.
    Quarter { c: Pt, rs: f64, ry: f64, t0: f64, t1: f64 },
}

impl Step {
    fn end(&self) -> Pt {
        match *self {
            Step::Line(p) => p,
            Step::Quarter { c, rs, ry, t1, .. } => ell(c, rs, ry, t1),
        }
    }
    fn k(&self, detail: u32) -> u64 {
        match self {
            Step::Line(_) => 1,
            Step::Quarter { .. } => segments(0.25, detail),
        }
    }
}

/// The point of the ellipse at angle `t` (exact at whole quarter turns).
fn ell(c: Pt, rs: f64, ry: f64, t: f64) -> Pt {
    let (s, co) = quarter_exact(t).unwrap_or_else(|| sin_cos(t));
    [c[0] + rs * co, c[1] + ry * s]
}

/// What a terrace is named by: each step's kind, for its surfaces.
fn step_name(a: Pt, b: Pt, round: bool) -> &'static str {
    if round {
        "round"
    } else if a[0] == b[0] {
        "side"
    } else if a[1] == b[1] {
        "terrace"
    } else {
        "bevel"
    }
}

/// A figure raised by a profile: rings of the figure's own pieces, the profile from the bottom
/// cap's `(s₀, 0)` up to the top cap's.
#[derive(Clone)]
pub(crate) struct Terrace {
    pub rings: Vec<Vec<Seg>>,
    pub start: Pt,
    pub steps: Vec<Step>,
    /// The figure's closed measures.
    pub plane: Measures,
    /// Whether every piece of the figure is straight (then the measures are exact).
    pub polygon: bool,
    /// For a slab: its closed measures are the figure's raised (any figure).
    pub slab: bool,
    /// The raised form and its numbers, so that a refusal can build the same form at another
    /// inset before it names one.
    pub depth: Depth,
    /// The most builds a refusal's search for the deepest inset that holds may make
    /// ([`SEARCH_BUILDS`] unless limited): with none, a refusal names nothing.
    pub search: u32,
}

/// Which raised form a terrace is, with the numbers it was asked for: what sets its deepest inset
/// is one of them (`Bevel`'s width, `Pillow`'s inset, `Stepped`'s run).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Depth {
    /// A slab: no inset.
    Slab { t: f64 },
    /// [`bevelled`].
    Bevel { t: f64, w: f64, round: bool, both: bool },
    /// [`pillow`].
    Pillow { t: f64, r: f64, i: f64 },
    /// [`stepped`].
    Stepped { steps: u32, rise: f64, run: f64 },
}

impl Depth {
    /// The profile's start and steps.
    fn profile(self) -> (Pt, Vec<Step>) {
        match self {
            Depth::Slab { t } => ([0.0, 0.0], vec![Step::Line([0.0, t])]),
            Depth::Bevel { t, w, round, both } => {
                let mut steps = Vec::new();
                let start = if both { [w, 0.0] } else { [0.0, 0.0] };
                if both {
                    steps.push(if round { Step::Quarter { c: [w, w], rs: w, ry: w, t0: 1.5 * PI, t1: PI } } else { Step::Line([0.0, w]) });
                }
                let wall_top = t - w;
                let wall_from = if both { w } else { 0.0 };
                if wall_top > wall_from {
                    steps.push(Step::Line([0.0, wall_top]));
                }
                steps.push(if round { Step::Quarter { c: [w, t - w], rs: w, ry: w, t0: PI, t1: 0.5 * PI } } else { Step::Line([w, t]) });
                (start, steps)
            }
            Depth::Pillow { t, r, i } => ([0.0, 0.0], vec![Step::Line([0.0, t]), Step::Quarter { c: [i, t], rs: i, ry: r, t0: PI, t1: 0.5 * PI }]),
            Depth::Stepped { steps, rise, run } => {
                let mut st = Vec::new();
                for j in 0..steps {
                    let s = j as f64 * run;
                    st.push(Step::Line([s, (j + 1) as f64 * rise]));
                    if j + 1 < steps {
                        st.push(Step::Line([(j + 1) as f64 * run, (j + 1) as f64 * rise]));
                    }
                }
                ([0.0, 0.0], st)
            }
        }
    }
    /// The parameter that sets the inset: its name and value (`None` for a slab).
    fn param(self) -> Option<(&'static str, f64)> {
        match self {
            Depth::Slab { .. } => None,
            Depth::Bevel { w, .. } => Some(("width", w)),
            Depth::Pillow { i, .. } => Some(("inset", i)),
            Depth::Stepped { run, .. } => Some(("run", run)),
        }
    }
    /// The same form with that parameter `p`.
    fn with(self, p: f64) -> Depth {
        match self {
            Depth::Slab { .. } => self,
            Depth::Bevel { t, round, both, .. } => Depth::Bevel { t, w: p, round, both },
            Depth::Pillow { t, r, .. } => Depth::Pillow { t, r, i: p },
            Depth::Stepped { steps, rise, .. } => Depth::Stepped { steps, rise, run: p },
        }
    }
}

/// What a terrace's own checks find (beside the figure's): an inset too deep, or a cap at no
/// inset that could not be cut.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Trouble {
    Deep,
    CapNotCut,
}

/// The sampled figure: its points (rings one after another), each ring's range, each sample's
/// piece and whether it is curved, and the chord error.
struct Sampled {
    q: Vec<Pt>,
    ranges: Vec<std::ops::Range<usize>>,
    curved: Vec<bool>,
    piece: Vec<(usize, usize)>,
    err: f64,
}

fn sample(rings: &[Vec<Seg>], detail: u32) -> Sampled {
    let (mut q, mut ranges, mut curved, mut piece) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut err = 0.0f64;
    for (ri, r) in rings.iter().enumerate() {
        let from = q.len();
        for (si, s) in r.iter().enumerate() {
            let before = q.len();
            err = err.max(s.emit(detail, &mut q));
            let c = !matches!(s, Seg::Line { .. });
            for j in before..q.len() {
                // A sample inside a curved piece (not its first point, which is a junction).
                curved.push(c && j > before);
                piece.push((ri, si));
            }
        }
        ranges.push(from..q.len());
    }
    Sampled { q, ranges, curved, piece, err }
}

/// The miter of each corner: the vector `m` with `m · n₀ = m · n₁ = 1` for the inward (left) unit
/// normals of the edges before and after it (`u₀`, `u₁` their unit directions); `None` where the
/// outline turns straight back on itself (no such vector).
///
/// Written two ways so that nothing cancels (review 1c, finding 2): where the corner turns by a
/// right angle or less (`u₀ · u₁ >= 0`), `(n₀ + n₁) / (1 + u₀ · u₁)`, whose divisor is at least 1;
/// where it turns by more, `(u₁ − u₀) / (u₀ × u₁)`, whose numerator is at least √2 long (the same
/// vector: both solve the two equations). The first form at a needle's tip divides by
/// `1 + u₀ · u₁ ≈ α²/2`, a sum of numbers near ±1 that keeps only `ε / α²` of its digits: at a
/// tip of 1.6e-4 radians a miter 1.25e4 long was off by a part in 1e8, so an inset edge near its
/// collapse was no longer parallel to its figure's and a band between them folded.
pub(crate) fn miters(q: &[Pt], ranges: &[std::ops::Range<usize>]) -> Option<Vec<Pt>> {
    let mut m = vec![[0.0; 2]; q.len()];
    for r in ranges {
        let n = r.len();
        let at = |i: usize| q[r.start + (i % n)];
        let unit = |a: Pt, b: Pt| {
            let d = sub(b, a);
            let l = len(d);
            [d[0] / l, d[1] / l]
        };
        for i in 0..n {
            let u0 = unit(at(i + n - 1), at(i));
            let u1 = unit(at(i), at(i + 1));
            let dot = u0[0] * u1[0] + u0[1] * u1[1];
            let v = if dot >= 0.0 {
                let d = 1.0 + dot;
                [(-u0[1] - u1[1]) / d, (u0[0] + u1[0]) / d]
            } else {
                let cr = u0[0] * u1[1] - u0[1] * u1[0];
                [(u1[0] - u0[0]) / cr, (u1[1] - u0[1]) / cr]
            };
            if !(v[0].is_finite() && v[1].is_finite()) {
                return None;
            }
            m[r.start + i] = v;
        }
    }
    Some(m)
}

/// The inset at which the first edge collapses (its length `L + s (m₁ − m₀) · ê` reaches 0), or
/// infinity.
pub(crate) fn collapse(q: &[Pt], m: &[Pt], ranges: &[std::ops::Range<usize>]) -> f64 {
    let mut least = f64::INFINITY;
    for r in ranges {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r.start + i, r.start + (i + 1) % n);
            let e = sub(q[b], q[a]);
            let l = len(e);
            let rate = ((m[b][0] - m[a][0]) * e[0] + (m[b][1] - m[a][1]) * e[1]) / l;
            if rate < 0.0 {
                least = least.min(l / -rate);
            }
        }
    }
    least
}

pub(crate) fn inset(q: &[Pt], m: &[Pt], s: f64) -> Vec<Pt> {
    if s == 0.0 {
        return q.to_vec();
    }
    q.iter().zip(m).map(|(p, d)| [p[0] + s * d[0], p[1] + s * d[1]]).collect()
}

/// Whether the rings inset by `s` are a well-made outline, no edge has collapsed, and no edge is
/// shorter than four floors of the figure's own size (the inset's own outline is checked against
/// its own size, which a nearly collapsed one makes tiny; the solid is checked against the
/// figure's).
fn inset_holds(q: &[Pt], m: &[Pt], ranges: &[std::ops::Range<usize>], s: f64, collapse_at: f64) -> bool {
    if s >= collapse_at {
        return false;
    }
    let p = inset(q, m, s);
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for x in q {
        for k in 0..2 {
            lo[k] = lo[k].min(x[k]);
            hi[k] = hi[k].max(x[k]);
        }
    }
    let floor = 4.0 * crate::outline::EDGE_FLOOR * dist(lo, hi);
    let short = ranges.iter().any(|r| (0..r.len()).any(|i| dist(p[r.start + i], p[r.start + (i + 1) % r.len()]) <= floor));
    !short && Outline { rings: ranges.iter().map(|r| p[r.clone()].to_vec()).collect(), chord_error: 0.0 }.validate().is_ok()
}

fn turn2(a: Pt, b: Pt, c: Pt) -> (f64, f64) {
    let t = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    (t, 16.0 * f64::EPSILON * (dist(a, b) * dist(a, c)).max(dist(a, b) * dist(b, c)))
}

impl Terrace {
    /// The profile's points at `detail`, each with the step it ends (`None` for the first) and
    /// whether that step is round.
    fn profile(&self, detail: u32) -> Vec<(Pt, bool)> {
        let mut out = vec![(self.start, false)];
        for st in &self.steps {
            match *st {
                Step::Line(p) => out.push((p, false)),
                Step::Quarter { c, rs, ry, t0, t1 } => {
                    let k = st.k(detail);
                    for j in 1..=k {
                        let t = if j == k { t1 } else { t0 + (t1 - t0) * j as f64 / k as f64 };
                        out.push((ell(c, rs, ry, t), true));
                    }
                }
            }
        }
        out
    }
    fn levels(&self, detail: u32) -> u64 {
        1 + self.steps.iter().map(|s| s.k(detail)).sum::<u64>()
    }
    fn n(&self, detail: u32) -> u64 {
        self.rings.iter().flatten().map(|s| s.k(detail)).sum()
    }
    fn cap_faces(&self, n: u64) -> u64 {
        if self.rings.len() == 1 {
            1
        } else {
            cap::triangles_of(n, self.rings.len() as u64)
        }
    }
    fn top(&self) -> Pt {
        self.steps.last().map(|s| s.end()).unwrap_or(self.start)
    }
    /// `∫ dy`, `∫ s dy`, `∫ s² dy`, `∫ dℓ` and `∫ s dℓ` along the profile, exactly (`None` for the
    /// last two over an ellipse's quarter that is not a circle's).
    fn integrals(&self) -> (f64, f64, f64, Option<f64>, Option<f64>) {
        let (mut h, mut i1, mut i2) = (0.0, 0.0, 0.0);
        let (mut l, mut ls) = (Some(0.0f64), Some(0.0f64));
        let mut a = self.start;
        for st in &self.steps {
            let b = st.end();
            match *st {
                Step::Line(_) => {
                    let dy = b[1] - a[1];
                    h += dy;
                    i1 += dy * 0.5 * (a[0] + b[0]);
                    i2 += dy * (a[0] * a[0] + a[0] * b[0] + b[0] * b[0]) / 3.0;
                    let dl = dist(a, b);
                    l = l.map(|x| x + dl);
                    ls = ls.map(|x| x + dl * 0.5 * (a[0] + b[0]));
                }
                Step::Quarter { c, rs, ry, t0, t1 } => {
                    // s = c₀ + rs cos t, dy = ry cos t dt.
                    let sc = |t: f64| quarter_exact(t).unwrap_or_else(|| sin_cos(t));
                    let ((s0, c0), (s1, c1)) = (sc(t0), sc(t1));
                    let i_c2 = |t: f64, s: f64, c: f64| 0.5 * t + 0.5 * s * c;
                    let i_c3 = |s: f64| s - s * s * s / 3.0;
                    let (ds, dc2, dc3) = (s1 - s0, i_c2(t1, s1, c1) - i_c2(t0, s0, c0), i_c3(s1) - i_c3(s0));
                    h += ry * ds;
                    i1 += ry * (c[0] * ds + rs * dc2);
                    i2 += ry * (c[0] * c[0] * ds + 2.0 * c[0] * rs * dc2 + rs * rs * dc3);
                    if rs == ry {
                        let dt = (t1 - t0).abs();
                        l = l.map(|x| x + rs * dt);
                        // ∫ s dℓ = r ∫ (c₀ + r cos t) |dt|.
                        let sign = if t1 >= t0 { 1.0 } else { -1.0 };
                        ls = ls.map(|x| x + rs * (c[0] * dt + rs * sign * ds));
                    } else {
                        l = None;
                        ls = None;
                    }
                }
            }
            a = b;
        }
        (h, i1, i2, l, ls)
    }
    /// `K`: the sum over every corner of every ring of `tan(φ / 2)`, `φ` its signed turn (of a
    /// figure of straight sides, whose corners are its samples).
    fn k_sum(&self) -> f64 {
        let mut k = 0.0;
        for r in &self.rings {
            let pts: Vec<Pt> = r.iter().map(|s| s.start()).collect();
            let n = pts.len();
            for i in 0..n {
                let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
                let (u, v) = (sub(b, a), sub(c, b));
                let cr = u[0] * v[1] - u[1] * v[0];
                let dt = u[0] * v[0] + u[1] * v[1];
                // tan(φ/2) = sin φ / (1 + cos φ) = cross / (|u||v| + dot).
                k += cr / (len(u) * len(v) + dt);
            }
        }
        k
    }
}

impl Make for Terrace {
    fn count(&self, detail: u32) -> (u64, u64) {
        let (n, l) = (self.n(detail), self.levels(detail));
        (n * l, n * (l - 1) + 2 * self.cap_faces(n))
    }

    fn limited(&self, builds: u32) -> Option<Rc<dyn Make>> {
        self.depth.param()?;
        Some(Rc::new(Terrace { rings: self.rings.clone(), start: self.start, steps: self.steps.clone(), plane: self.plane, polygon: self.polygon, slab: self.slab, depth: self.depth, search: builds.min(SEARCH_BUILDS) }))
    }

    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>) {
        let sm = sample(&self.rings, detail);
        let outline = Outline { rings: sm.ranges.iter().map(|r| sm.q[r.clone()].to_vec()).collect(), chord_error: sm.err };
        let figure = outline.validate().err().map(crate::solid::section_fault);
        let n = sm.q.len();
        let s_max = self.s_max(detail);
        // A slab insets nothing and needs no miters (review 1c, finding 1: a needle's tip, whose
        // miter was taken for undefined, refused its slab as an inset too deep).
        let m = if s_max > 0.0 { miters(&sm.q, &sm.ranges) } else { Some(vec![[0.0; 2]; n]) };
        let (mesh, trouble) = self.raise(&sm, m.as_deref().unwrap_or(&vec![[0.0; 2]; n]), detail);
        let fault = match (figure, &m, trouble) {
            (Some(f), _, _) => Some(f),
            // No inset of an outline that turns straight back on itself is defined.
            (None, None, _) => Some(SolidFault::InsetTooDeep { inset: s_max, param: self.depth.param().map_or("inset", |p| p.0), holds: None, searched: true }),
            (None, Some(m), Some(Trouble::Deep)) => Some(SolidFault::InsetTooDeep { inset: s_max, param: self.depth.param().map_or("inset", |p| p.0), holds: self.deepest(&sm, m, detail), searched: self.search > 0 }),
            (None, Some(_), Some(Trouble::CapNotCut)) => Some(SolidFault::CapNotCut),
            (None, Some(_), None) => None,
        };
        (mesh, fault)
    }

    fn exact(&self) -> Measures {
        self.exact_measures()
    }

    fn error(&self, detail: u32) -> Option<f64> {
        self.error_at(detail)
    }

    fn genus(&self) -> u32 {
        (self.rings.len() - 1) as u32
    }

    fn curved(&self) -> bool {
        self.rings.iter().flatten().any(|s| !matches!(s, Seg::Line { .. })) || self.steps.iter().any(|s| matches!(s, Step::Quarter { .. }))
    }

    fn pieces(&self) -> Vec<Seg> {
        self.rings.iter().flatten().cloned().collect()
    }
}

impl Terrace {
    /// The deepest inset of the profile at `detail`.
    fn s_max(&self, detail: u32) -> f64 {
        self.profile(detail).iter().map(|p| p.0[0]).fold(0.0f64, f64::max)
    }

    /// The same raised form with its inset's parameter `p`.
    fn with(&self, p: f64) -> Terrace {
        let depth = self.depth.with(p);
        let (start, steps) = depth.profile();
        Terrace { rings: self.rings.clone(), start, steps, plane: self.plane, polygon: self.polygon, slab: self.slab, depth, search: self.search }
    }

    /// The deepest inset that holds at `detail`, and the parameter's value that gives it: the
    /// greatest value from the least length up to the one asked for, by halving, at which the
    /// same form **is built** and its own checks find nothing (review 1c, finding 2: a search on
    /// a narrower predicate than the build's named an inset whose bands folded). The halving
    /// keeps a value only after building at it, so what is named was built and held, whether or
    /// not holding is monotone in the inset. `None` where the least length does not hold
    /// (finding 3: an inset under it cannot be asked for). At most `self.search` builds of the
    /// same size as this one ([`SEARCH_BUILDS`], or fewer where a caller's budget limits it: then
    /// what is named still held, and may be shallower than the deepest; with none, `None`).
    fn deepest(&self, sm: &Sampled, m: &[Pt], detail: u32) -> Option<(f64, f64)> {
        let (_, asked) = self.depth.param()?;
        if self.search == 0 {
            return None;
        }
        let holds = |p: f64| self.with(p).raise(sm, m, detail).1.is_none();
        let mut lo = LEN_MIN;
        if !(lo < asked && holds(lo)) {
            return None;
        }
        let mut hi = asked;
        for _ in 1..self.search {
            let mid = 0.5 * (lo + hi);
            if mid <= lo || mid >= hi {
                break;
            }
            if holds(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some((self.with(lo).s_max(detail), lo))
    }

    /// The mesh at `detail` from the sampled figure and its miters, and what the terrace's own
    /// checks find (an inset too deep, or a cap at no inset not cut).
    fn raise(&self, sm: &Sampled, m: &[Pt], detail: u32) -> (Mesh, Option<Trouble>) {
        let n = sm.q.len();
        let prof = self.profile(detail);
        let s_max = prof.iter().map(|p| p.0[0]).fold(0.0f64, f64::max);
        let mut fault: Option<Trouble> = None;
        let collapse_at = if s_max > 0.0 { collapse(&sm.q, m, &sm.ranges) } else { f64::INFINITY };
        if s_max > 0.0 && !inset_holds(&sm.q, m, &sm.ranges, s_max, collapse_at) {
            fault = Some(Trouble::Deep);
        }
        // Points: a ring set for each of the profile's points.
        let mut points: Vec<P3> = Vec::with_capacity(n * prof.len());
        let mut plans: Vec<Vec<Pt>> = Vec::with_capacity(prof.len());
        for (p, _) in &prof {
            let plan = inset(&sm.q, m, p[0]);
            points.extend(plan.iter().map(|q| [q[0], p[1], -q[1]]));
            plans.push(plan);
        }
        // Surfaces: the caps; for each step, one flat surface for its faces over straight sides
        // and one smooth one for those over each curved piece (a wall over an arc is a cylinder's
        // side), or one smooth surface for a round step over straight sides.
        let mut surfaces = vec![Surface { name: "bottom".into(), smooth: false, radius: None }, Surface { name: "top".into(), smooth: false, radius: None }];
        let mut faces = Vec::new();
        let at = |lev: usize, i: usize| (lev * n + i) as u32;
        let ring_ids = |lev: usize| -> Vec<Vec<u32>> { sm.ranges.iter().map(|r| r.clone().map(|i| at(lev, i)).collect()).collect() };
        // The bottom cap and the top cap.
        let last = prof.len() - 1;
        for (lev, surface, up) in [(0usize, 0u32, false), (last, 1u32, true)] {
            let ids = ring_ids(lev);
            if ids.len() == 1 {
                let mut v = ids[0].clone();
                if !up {
                    v.reverse();
                }
                faces.push(Face { verts: v, surface, normals: None });
            } else {
                let flat: Vec<Pt> = plans[lev].clone();
                let local: Vec<Vec<u32>> = sm.ranges.iter().map(|r| r.clone().map(|i| i as u32).collect()).collect();
                match cap::cut(&flat, &local) {
                    Ok(tris) => {
                        for t in tris {
                            let t = t.map(|i| at(lev, i as usize));
                            let v = if up { vec![t[0], t[1], t[2]] } else { vec![t[2], t[1], t[0]] };
                            faces.push(Face { verts: v, surface, normals: None });
                        }
                    }
                    Err(_) => {
                        // A cap of rings that are not a well-made region: deeper than holds (or,
                        // at no inset, the figure's own fault, found above).
                        let s = prof[lev].0[0];
                        fault = fault.or(Some(if s > 0.0 { Trouble::Deep } else { Trouble::CapNotCut }));
                        // Still a closed count of faces: the right number of placeholder fans.
                        let want = cap::triangles_of(n as u64, sm.ranges.len() as u64);
                        for _ in 0..want {
                            faces.push(Face { verts: vec![at(lev, 0), at(lev, 1), at(lev, 2)], surface, normals: None });
                        }
                    }
                }
            }
        }
        let mut fig_err = sm.err;
        let mut sec = 0.0f64;
        let mut stretch = 1.0f64;
        for i in 0..n {
            stretch = stretch.max(len(m[i]));
            if sm.curved[i] {
                sec = sec.max(len(m[i]) - 1.0);
            }
        }
        let mut prof_err = 0.0f64;
        for j in 0..last {
            let (a, b) = (prof[j].0, prof[j + 1].0);
            let round = prof[j + 1].1;
            let name = step_name(a, b, round);
            // This step's surfaces.
            let flat_id = surfaces.len() as u32;
            surfaces.push(Surface { name: format!("{name}_{j}"), smooth: round, radius: None });
            let mut curved_ids: Vec<(usize, usize, u32)> = Vec::new();
            for i in 0..n {
                let (ri, si) = sm.piece[i];
                let range = &sm.ranges[ri];
                let i2 = range.start + (i - range.start + 1) % range.len();
                let seg = &self.rings[ri][si];
                let surface = match seg {
                    _ if a[1] == b[1] => flat_id,
                    Seg::Line { .. } => flat_id,
                    _ => {
                        if let Some(&(_, _, id)) = curved_ids.iter().find(|x| (x.0, x.1) == (ri, si)) {
                            id
                        } else {
                            let id = surfaces.len() as u32;
                            let radius = if let Seg::Arc { r, .. } = seg { Some(*r) } else { None };
                            surfaces.push(Surface { name: format!("{name}_{j}_{ri}_{si}"), smooth: true, radius });
                            curved_ids.push((ri, si, id));
                            id
                        }
                    }
                };
                // Over a straight side, a round step's faces are one smooth surface.
                if round && matches!(seg, Seg::Line { .. }) {
                    surfaces[flat_id as usize].smooth = true;
                }
                faces.push(Face { verts: vec![at(j, i), at(j, i2), at(j + 1, i2), at(j + 1, i)], surface, normals: None });
                // A band (not a wall) turns one way in plan, convexly, past rounding.
                if fault.is_none() && a[0] != b[0] {
                    let sign = if b[0] > a[0] { 1.0 } else { -1.0 };
                    let quad = [plans[j][i], plans[j][i2], plans[j + 1][i2], plans[j + 1][i]];
                    for c in 0..4 {
                        let (t, e) = turn2(quad[c], quad[(c + 1) % 4], quad[(c + 2) % 4]);
                        if (sign * t).is_nan() || sign * t <= e {
                            fault = Some(Trouble::Deep);
                            break;
                        }
                    }
                }
            }
            if round {
                // The step's chord sag on the profile's quarter ellipse: at most the greater radius
                // times the versine of half the step's angle.
                if let Some(Step::Quarter { rs, ry, .. }) = self.step_of(j, detail) {
                    let k = segments(0.25, detail) as f64;
                    prof_err = prof_err.max(rs.max(ry) * versin(0.25 * PI / k));
                }
            }
        }
        if self.polygon {
            fig_err = 0.0;
        }
        let chord_error = fig_err + s_max * sec + prof_err * stretch;
        let mesh = Mesh { points, faces, surfaces, chord_error };
        (mesh, fault)
    }

    fn exact_measures(&self) -> Measures {
        let p = &self.plane;
        let top = self.top();
        let h = top[1];
        let bounds = p.bounds.map(|(lo, hi)| ([lo[0], 0.0, -hi[1]], [hi[0], h, -lo[1]]));
        if self.slab {
            return Measures {
                size: p.size.map(|a| a * h),
                boundary: match (p.size, p.boundary) {
                    (Some(a), Some(l)) => Some(2.0 * a + l * h),
                    _ => None,
                },
                centroid: p.centroid.map(|c| [c[0], 0.5 * h, -c[1]]),
                bounds,
                series: p.series,
            };
        }
        if !self.polygon {
            return Measures { bounds, ..Measures::default() };
        }
        let (Some(a), Some(pp)) = (p.size, p.boundary) else { return Measures { bounds, ..Measures::default() } };
        let k = self.k_sum();
        let area_at = |s: f64| a - pp * s + k * s * s;
        let (hh, i1, i2, l, ls) = self.integrals();
        let volume = a * hh - pp * i1 + k * i2;
        let boundary = match (l, ls) {
            (Some(l), Some(ls)) => Some(area_at(self.start[0]) + area_at(top[0]) + pp * l - 2.0 * k * ls),
            _ => None,
        };
        Measures { size: Some(volume), boundary, centroid: None, bounds, series: None }
    }

    fn error_at(&self, detail: u32) -> Option<f64> {
        if self.polygon {
            let round = self.steps.iter().filter_map(|s| if let Step::Quarter { rs, ry, .. } = s { Some(rs.max(*ry)) } else { None }).fold(0.0f64, f64::max);
            if round == 0.0 {
                return Some(0.0);
            }
            // The profile's sag times the greatest miter: known from the samples.
            let sm = sample(&self.rings, detail);
            let m = miters(&sm.q, &sm.ranges).unwrap_or_else(|| vec![[0.0; 2]; sm.q.len()]);
            return Some(self.raise(&sm, &m, detail).0.chord_error);
        }
        if self.slab {
            return Geom::Rings(self.rings.clone()).sag(detail);
        }
        None
    }

    /// The step the profile's segment `j` (at `detail`) belongs to.
    fn step_of(&self, j: usize, detail: u32) -> Option<Step> {
        let mut at = 0usize;
        for st in &self.steps {
            let k = st.k(detail) as usize;
            if j < at + k {
                return Some(*st);
            }
            at += k;
        }
        None
    }
}

// ---------------------------------------------------------------- relief

/// The figure raised by `height` on a plate `plate` thick that is the figure's bounds widened by
/// `margin` on every side (its closed bounds where it has them, its build's otherwise).
#[derive(Clone)]
pub(crate) struct Relief {
    pub rings: Vec<Vec<Seg>>,
    pub height: f64,
    pub plate: f64,
    pub margin: f64,
    pub plane: Measures,
}

impl Relief {
    fn n(&self, detail: u32) -> u64 {
        self.rings.iter().flatten().map(|s| s.k(detail)).sum()
    }
    fn n_outer(&self, detail: u32) -> u64 {
        self.rings[0].iter().map(|s| s.k(detail)).sum()
    }
    fn rect(&self, sampled: &[Pt]) -> (Pt, Pt) {
        let (lo, hi) = match self.plane.bounds {
            Some((lo, hi)) => ([lo[0], lo[1]], [hi[0], hi[1]]),
            None => {
                let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
                for p in sampled {
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                (lo, hi)
            }
        };
        ([lo[0] - self.margin, lo[1] - self.margin], [hi[0] + self.margin, hi[1] + self.margin])
    }
}

impl Make for Relief {
    fn count(&self, detail: u32) -> (u64, u64) {
        let (n, no, h) = (self.n(detail), self.n_outer(detail), self.rings.len() as u64 - 1);
        let top = if h == 0 { 1 } else { cap::triangles_of(n, h + 1) };
        (8 + 2 * n, 1 + 4 + (no + 4) + h + n + top)
    }

    fn build(&self, detail: u32) -> (Mesh, Option<SolidFault>) {
        let sm = sample(&self.rings, detail);
        let outline = Outline { rings: sm.ranges.iter().map(|r| sm.q[r.clone()].to_vec()).collect(), chord_error: sm.err };
        let mut fault = outline.validate().err().map(crate::solid::section_fault);
        let n = sm.q.len();
        let (lo, hi) = self.rect(&sm.q);
        let (pl, top) = (self.plate, self.plate + self.height);
        let corners = [[lo[0], lo[1]], [hi[0], lo[1]], [hi[0], hi[1]], [lo[0], hi[1]]];
        let mut points: Vec<P3> = Vec::with_capacity(8 + 2 * n);
        points.extend(corners.iter().map(|c| [c[0], 0.0, -c[1]]));
        points.extend(corners.iter().map(|c| [c[0], pl, -c[1]]));
        points.extend(sm.q.iter().map(|q| [q[0], pl, -q[1]]));
        points.extend(sm.q.iter().map(|q| [q[0], top, -q[1]]));
        let surfaces = vec![
            Surface { name: "bottom".into(), smooth: false, radius: None },
            Surface { name: "plate_side".into(), smooth: false, radius: None },
            Surface { name: "plate_top".into(), smooth: false, radius: None },
            Surface { name: "side".into(), smooth: false, radius: None },
            Surface { name: "top".into(), smooth: false, radius: None },
        ];
        let mut surfaces = surfaces;
        let mut faces = vec![Face { verts: vec![3, 2, 1, 0], surface: 0, normals: None }];
        for i in 0..4u32 {
            let j = (i + 1) % 4;
            faces.push(Face { verts: vec![i, j, j + 4, i + 4], surface: 1, normals: None });
        }
        let fig = |lev: u32, i: usize| 8 + lev * n as u32 + i as u32;
        // The plate's top round the figure: the rectangle with the figure's outer ring as a hole.
        let outer = sm.ranges[0].clone();
        let mut flat: Vec<Pt> = corners.to_vec();
        flat.extend(outer.clone().map(|i| sm.q[i]));
        let rect_ring: Vec<u32> = vec![0, 1, 2, 3];
        let hole: Vec<u32> = (0..outer.len()).rev().map(|i| 4 + i as u32).collect();
        let want = outer.len() + 4;
        match cap::cut(&flat, &[rect_ring, hole]) {
            Ok(tris) => {
                for t in tris {
                    let v: Vec<u32> = t.iter().map(|&i| if i < 4 { 4 + i } else { fig(0, outer.start + (i as usize - 4)) }).collect();
                    faces.push(Face { verts: v, surface: 2, normals: None });
                }
            }
            Err(_) => {
                fault = fault.or(Some(SolidFault::CapNotCut));
                for _ in 0..want {
                    faces.push(Face { verts: vec![4, 5, 6], surface: 2, normals: None });
                }
            }
        }
        // The floors of the figure's holes, on the plate.
        for r in &sm.ranges[1..] {
            faces.push(Face { verts: r.clone().rev().map(|i| fig(0, i)).collect(), surface: 2, normals: None });
        }
        // The figure's walls.
        let mut seg_surface: Vec<(usize, usize, u32)> = Vec::new();
        for r in &sm.ranges {
            let nn = r.len();
            for i in r.clone() {
                let i2 = r.start + (i - r.start + 1) % nn;
                let (ri, si) = sm.piece[i];
                let seg = &self.rings[ri][si];
                let surface = match seg {
                    Seg::Line { .. } => 3,
                    _ => {
                        if let Some(&(_, _, id)) = seg_surface.iter().find(|x| (x.0, x.1) == (ri, si)) {
                            id
                        } else {
                            let id = surfaces.len() as u32;
                            let radius = if let Seg::Arc { r, .. } = seg { Some(*r) } else { None };
                            surfaces.push(Surface { name: format!("side_{ri}_{si}"), smooth: true, radius });
                            seg_surface.push((ri, si, id));
                            id
                        }
                    }
                };
                faces.push(Face { verts: vec![fig(0, i), fig(0, i2), fig(1, i2), fig(1, i)], surface, normals: None });
            }
        }
        // The figure's top.
        if sm.ranges.len() == 1 {
            faces.push(Face { verts: (0..n).map(|i| fig(1, i)).collect(), surface: 4, normals: None });
        } else {
            let local: Vec<Vec<u32>> = sm.ranges.iter().map(|r| r.clone().map(|i| i as u32).collect()).collect();
            match cap::cut(&sm.q, &local) {
                Ok(tris) => {
                    for t in tris {
                        faces.push(Face { verts: t.iter().map(|&i| fig(1, i as usize)).collect(), surface: 4, normals: None });
                    }
                }
                Err(_) => {
                    fault = fault.or(Some(SolidFault::CapNotCut));
                    for _ in 0..cap::triangles_of(n as u64, sm.ranges.len() as u64) {
                        faces.push(Face { verts: vec![fig(1, 0), fig(1, 1), fig(1, 2)], surface: 4, normals: None });
                    }
                }
            }
        }
        (Mesh { points, faces, surfaces, chord_error: sm.err }, fault)
    }

    fn exact(&self) -> Measures {
        let p = &self.plane;
        let Some((lo, hi)) = p.bounds else { return Measures::default() };
        let (w, d) = (hi[0] - lo[0] + 2.0 * self.margin, hi[1] - lo[1] + 2.0 * self.margin);
        let (cx, cz) = (0.5 * (lo[0] + hi[0]), -0.5 * (lo[1] + hi[1]));
        let top = self.plate + self.height;
        let bounds = Some(([lo[0] - self.margin, 0.0, -hi[1] - self.margin], [hi[0] + self.margin, top, -lo[1] + self.margin]));
        let (Some(a), Some(per)) = (p.size, p.boundary) else { return Measures { bounds, ..Measures::default() } };
        let (vp, vf) = (w * d * self.plate, a * self.height);
        let v = vp + vf;
        let centroid = p.centroid.map(|c| {
            let fy = self.plate + 0.5 * self.height;
            [(vp * cx + vf * c[0]) / v, (vp * 0.5 * self.plate + vf * fy) / v, (vp * cz + vf * -c[1]) / v]
        });
        Measures { size: Some(v), boundary: Some(2.0 * w * d + 2.0 * (w + d) * self.plate + per * self.height), centroid, bounds, series: None }
    }

    fn error(&self, detail: u32) -> Option<f64> {
        Geom::Rings(self.rings.clone()).sag(detail)
    }

    fn genus(&self) -> u32 {
        0
    }

    fn curved(&self) -> bool {
        self.rings.iter().flatten().any(|s| !matches!(s, Seg::Line { .. }))
    }

    fn pieces(&self) -> Vec<Seg> {
        self.rings.iter().flatten().cloned().collect()
    }
}

// ---------------------------------------------------------------- the functions

fn rings_of(figure: &Spec, operator: &'static str) -> Result<(Vec<Vec<Seg>>, Measures), Error> {
    if figure.form.kind != Kind::Plane {
        return Err(Error::Constraint { form: figure.form.name, rule: "plane_figure_only", message: format!("{operator} takes a closed plane figure, and {} is a {}", figure.name(), figure.form.kind.name()), values: Vec::new() });
    }
    match figure.geom_of()? {
        Geom::Rings(rings) => Ok((rings, figure.closed()?)),
        _ => unreachable!("a plane figure's geometry is rings"),
    }
}

fn length(operator: &'static str, name: &'static str, v: f64) -> Result<f64, Error> {
    if v.is_finite() && (LEN_MIN..=LEN_MAX).contains(&v) {
        Ok(v)
    } else {
        Err(Error::OutOfRange { form: operator, param: name, value: v, min: LEN_MIN, max: LEN_MAX })
    }
}

fn polygon(rings: &[Vec<Seg>]) -> bool {
    rings.iter().flatten().all(|s| matches!(s, Seg::Line { .. }))
}

fn terrace(figure: &Spec, operator: &'static str, rings: Vec<Vec<Seg>>, plane: Measures, depth: Depth) -> Solid {
    let polygon = polygon(&rings);
    let (start, steps) = depth.profile();
    let slab = matches!(depth, Depth::Slab { .. });
    Solid::new(operator, figure.form.name, Rc::new(Terrace { rings, start, steps, plane, polygon, slab, depth, search: SEARCH_BUILDS }))
}

/// The figure with straight sides `thickness` metres tall (holes go through it: a slab of an
/// annulus is a washer, of genus 1).
pub fn slab(figure: &Spec, thickness: f64) -> Result<Solid, Error> {
    let (rings, plane) = rings_of(figure, "slab")?;
    let t = length("slab", "thickness", thickness)?;
    Ok(terrace(figure, "slab", rings, plane, Depth::Slab { t }))
}

/// A slab `thickness` tall whose top edge is cut back by `width`, flat (a chamfer at 45 degrees)
/// or `round` (a quarter circle of radius `width`), and its bottom edge too when `both`. The
/// width is at most the thickness (half of it when both edges are cut).
pub fn bevelled(figure: &Spec, thickness: f64, width: f64, round: bool, both: bool) -> Result<Solid, Error> {
    let (rings, plane) = rings_of(figure, "bevelled")?;
    let t = length("bevelled", "thickness", thickness)?;
    let w = length("bevelled", "width", width)?;
    let most = if both { 0.5 * t } else { t };
    if w > most {
        return Err(Error::Constraint { form: "bevelled", rule: "bevel_over_thickness", message: "the bevel's width is at most the thickness (half of it when both edges are cut)".into(), values: vec![("thickness", t), ("width", w), ("greatest_width", most)] });
    }
    Ok(terrace(figure, "bevelled", rings, plane, Depth::Bevel { t, w, round, both }))
}

/// A slab `thickness` tall whose top rises by `rise` more, along a quarter ellipse, from its rim
/// to a flat middle at `inset` in from the outline (vertical at the rim, flat at the inset).
pub fn pillow(figure: &Spec, thickness: f64, rise: f64, inset: f64) -> Result<Solid, Error> {
    let (rings, plane) = rings_of(figure, "pillow")?;
    let t = length("pillow", "thickness", thickness)?;
    let r = length("pillow", "rise", rise)?;
    let i = length("pillow", "inset", inset)?;
    Ok(terrace(figure, "pillow", rings, plane, Depth::Pillow { t, r, i }))
}

/// `steps` terraces (1 to [`MAX_STEPS`]), each `rise` metres above the one before and its
/// outline `run` metres in from it; the first stands on the figure's own outline.
pub fn stepped(figure: &Spec, steps: u32, rise: f64, run: f64) -> Result<Solid, Error> {
    let (rings, plane) = rings_of(figure, "stepped")?;
    if !(1..=MAX_STEPS).contains(&steps) {
        return Err(Error::OutOfRange { form: "stepped", param: "steps", value: steps as f64, min: 1.0, max: MAX_STEPS as f64 });
    }
    let rise = length("stepped", "rise", rise)?;
    let run = length("stepped", "run", run)?;
    Ok(terrace(figure, "stepped", rings, plane, Depth::Stepped { steps, rise, run }))
}

/// The figure raised by `height` on a plate `plate` thick that reaches `margin` past the
/// figure's bounds on every side (its closed bounds where it has them, its build's otherwise):
/// one closed surface of genus 0 (a figure's holes are hollows to the plate).
pub fn relief(figure: &Spec, height: f64, margin: f64, plate: f64) -> Result<Solid, Error> {
    let (rings, plane) = rings_of(figure, "relief")?;
    let height = length("relief", "height", height)?;
    let margin = length("relief", "margin", margin)?;
    let plate = length("relief", "plate", plate)?;
    Ok(Solid::new("relief", figure.form.name, Rc::new(Relief { rings, height, plate, margin, plane })))
}
