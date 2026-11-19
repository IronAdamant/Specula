//! What plane figures and curves build to, and their measures as built.

use crate::maths::*;
use crate::Measures;

/// A built plane figure: rings of points in metres (x right, y up). The first ring is the outer
/// boundary, counter-clockwise; any further rings are holes, clockwise. No ring repeats its
/// first point at its end.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    /// The rings.
    pub rings: Vec<Vec<Pt>>,
    /// The build's error at its detail: for an arc, exactly its sag (the greatest distance
    /// between a chord and its arc); for a parametric piece, the greatest distance from a chord
    /// to the true curve at a quarter, a half and three quarters of each sample's parameter,
    /// which is an estimate and not a bound (a curve whose curvature changes sharply inside one
    /// sample can stray farther). 0 for straight-sided figures.
    pub chord_error: f64,
}

/// A built curve: points in metres, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// The points. A closed curve does not repeat its first point.
    pub points: Vec<Pt>,
    /// Whether the curve closes back to its first point.
    pub closed: bool,
    /// As [`Outline::chord_error`].
    pub chord_error: f64,
}

/// Why an outline is not well made (from [`Outline::validate`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Fault {
    /// No rings, or a ring of fewer than three points.
    TooFew {
        /// The ring.
        ring: usize,
    },
    /// A coordinate is not finite.
    NotFinite {
        /// The ring.
        ring: usize,
        /// The point.
        point: usize,
    },
    /// An edge is no longer than [`EDGE_FLOOR`] (a billionth) of the figure's size.
    ShortEdge {
        /// The ring.
        ring: usize,
        /// The edge's first point.
        point: usize,
        /// Its length.
        length: f64,
    },
    /// The outer ring is not counter-clockwise, or a hole not clockwise.
    Winding {
        /// The ring.
        ring: usize,
        /// Its signed area.
        signed_area: f64,
    },
    /// Two edges cross or touch.
    Crossing {
        /// The first edge's ring and point.
        a: (usize, usize),
        /// The second edge's ring and point.
        b: (usize, usize),
    },
    /// Two edges that do not cross come no farther apart than [`EDGE_FLOOR`] of the figure's
    /// size (two edges that are not neighbours, or a neighbour's far end and the other edge: a
    /// corner sharper than the floor allows): parts of the figure too far apart in size for
    /// the crossing test's signs to be trusted there.
    Gap {
        /// The first edge's ring and point.
        a: (usize, usize),
        /// The second edge's ring and point.
        b: (usize, usize),
        /// How far apart they come.
        gap: f64,
    },
    /// A hole lies outside the outer ring, or inside another hole.
    HoleOutside {
        /// The hole.
        ring: usize,
    },
}

/// The least length an edge (or a curve's step) of a build may have, as a share of the
/// figure's size (the diagonal of its bounds). Why a billionth: an edge's ends are rounded to
/// about `1e-16` of the figure's reach, so an edge a billionth of it long is known to seven
/// digits, which the crossing test (signs of products of such differences) still reads
/// rightly; much shorter, and whether two edges cross is decided by rounding. A figure whose
/// parts are farther apart in size than this (a hole of a tenth of a millimetre in a disc of a
/// kilometre, sampled finely) is refused as data by [`crate::Spec::build`]. (The modeller
/// works in `f32`, whose points are rounded to about a ten-millionth of their reach: a part
/// under that is lost there whatever this floor says.)
pub const EDGE_FLOOR: f64 = 1e-9;

/// The signed area of a ring (positive counter-clockwise), by the shoelace formula.
pub fn signed_area(ring: &[Pt]) -> f64 {
    let n = ring.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

fn bounds_of<'a>(pts: impl Iterator<Item = &'a Pt>) -> Option<([f64; 3], [f64; 3])> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut any = false;
    for p in pts {
        any = true;
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    any.then_some(([lo[0], lo[1], 0.0], [hi[0], hi[1], 0.0]))
}

impl Outline {
    /// Every point of every ring.
    pub fn points(&self) -> impl Iterator<Item = &Pt> {
        self.rings.iter().flatten()
    }

    /// The measures of what was built: the shoelace area (holes subtracted), the length of
    /// every ring, the polygon centroid and the bounds.
    pub fn measures(&self) -> Measures {
        let (mut a, mut cx, mut cy, mut per) = (0.0, 0.0, 0.0, 0.0);
        for r in &self.rings {
            let n = r.len();
            for i in 0..n {
                let (p, q) = (r[i], r[(i + 1) % n]);
                let cr = p[0] * q[1] - q[0] * p[1];
                a += 0.5 * cr;
                cx += (p[0] + q[0]) * cr / 6.0;
                cy += (p[1] + q[1]) * cr / 6.0;
                per += dist(p, q);
            }
        }
        Measures { size: Some(a), boundary: Some(per), centroid: Some([cx / a, cy / a, 0.0]), bounds: bounds_of(self.points()), series: None }
    }

    /// The figure's size: the diagonal of its bounds.
    pub fn size(&self) -> f64 {
        bounds_of(self.points()).map(|(lo, hi)| len([hi[0] - lo[0], hi[1] - lo[1]])).unwrap_or(0.0)
    }

    /// Whether `p` is inside the figure (inside the outer ring and in no hole), by crossings.
    pub fn contains(&self, p: Pt) -> bool {
        let mut inside = false;
        for r in &self.rings {
            let n = r.len();
            for i in 0..n {
                let (a, b) = (r[i], r[(i + 1) % n]);
                if (a[1] > p[1]) != (b[1] > p[1]) {
                    let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                    if x > p[0] {
                        inside = !inside;
                    }
                }
            }
        }
        inside
    }

    /// The distance from `p` to the nearest edge of any ring.
    pub fn distance_to_boundary(&self, p: Pt) -> f64 {
        let mut d = f64::INFINITY;
        for r in &self.rings {
            let n = r.len();
            for i in 0..n {
                d = d.min(dist_to_segment(p, r[i], r[(i + 1) % n]));
            }
        }
        d
    }

    /// Checks that the outline is well made: at least one ring of three or more points, every
    /// coordinate finite, no edge shorter than a billionth of the figure's size, the outer ring
    /// counter-clockwise and holes clockwise, no two edges crossing or touching (edges next to
    /// each other share only their common point), no two edges nearer each other than that
    /// billionth (nor a neighbour's far end to an edge: a corner that sharp), and every hole
    /// inside the outer ring and in no other hole. Edges are bucketed on a grid, so the cost is
    /// near linear for figures whose edges are of like length.
    ///
    /// (The gap: the crossing test's signs are only trusted where parts are farther apart than
    /// rounding; with every edge over the floor but two edges of a chevron 1e4 long 9e-13 apart,
    /// they were not, and it passed a figure it could not tell from a crossing one.)
    pub fn validate(&self) -> Result<(), Fault> {
        if self.rings.is_empty() {
            return Err(Fault::TooFew { ring: 0 });
        }
        for (ri, r) in self.rings.iter().enumerate() {
            if r.len() < 3 {
                return Err(Fault::TooFew { ring: ri });
            }
            for (pi, p) in r.iter().enumerate() {
                if !(p[0].is_finite() && p[1].is_finite()) {
                    return Err(Fault::NotFinite { ring: ri, point: pi });
                }
            }
        }
        let size = self.size();
        for (ri, r) in self.rings.iter().enumerate() {
            let n = r.len();
            for i in 0..n {
                let l = dist(r[i], r[(i + 1) % n]);
                if l <= size * EDGE_FLOOR {
                    return Err(Fault::ShortEdge { ring: ri, point: i, length: l });
                }
            }
            let s = signed_area(r);
            if (ri == 0) != (s > 0.0) {
                return Err(Fault::Winding { ring: ri, signed_area: s });
            }
        }
        // Edges: (ring, index), each with its box grown by the floor, so that two edges nearer
        // than it have boxes that meet. Every pair whose boxes meet is looked at once, by a tree
        // of boxes ([`box_pairs`]). (A grid of about one cell per edge was used before: a
        // logarithmic spiral band of twenty turns at detail 4096 crowded its inner turns, most of
        // its 160,000 points, into a few cells, and its check took half a minute.)
        let edges: Vec<(usize, usize)> = self.rings.iter().enumerate().flat_map(|(ri, r)| (0..r.len()).map(move |i| (ri, i))).collect();
        let seg = |e: (usize, usize)| {
            let r = &self.rings[e.0];
            (r[e.1], r[(e.1 + 1) % r.len()])
        };
        let floor = size * EDGE_FLOOR;
        let boxes: Vec<(Pt, Pt)> = edges
            .iter()
            .map(|&e| {
                let (a, b) = seg(e);
                ([a[0].min(b[0]) - floor, a[1].min(b[1]) - floor], [a[0].max(b[0]) + floor, a[1].max(b[1]) + floor])
            })
            .collect();
        let adjacent = |a: (usize, usize), b: (usize, usize)| {
            if a.0 != b.0 {
                return false;
            }
            let n = self.rings[a.0].len();
            a.1 == b.1 || (a.1 + 1) % n == b.1 || (b.1 + 1) % n == a.1
        };
        let pair = |i: usize, j: usize| -> Option<Fault> {
            let (ea, eb) = (edges[i.min(j)], edges[i.max(j)]);
            let (a0, a1) = seg(ea);
            let (b0, b1) = seg(eb);
            if adjacent(ea, eb) {
                // Neighbours share one point; they must not fold back over each other, nor the
                // far end of either come within the floor of the other.
                let n = self.rings[ea.0].len();
                if n > 3 && collinear_overlap(a0, a1, b0, b1) {
                    return Some(Fault::Crossing { a: ea, b: eb });
                }
                // (`a` then `b` round the ring, or `b` then `a`: the far ends are the others.)
                let (fa, fb) = if a1 == b0 { (a0, b1) } else { (a1, b0) };
                let gap = dist_to_segment(fa, b0, b1).min(dist_to_segment(fb, a0, a1));
                return (gap <= floor).then_some(Fault::Gap { a: ea, b: eb, gap });
            }
            if segments_meet(a0, a1, b0, b1) {
                return Some(Fault::Crossing { a: ea, b: eb });
            }
            let gap = dist_to_segment(a0, b0, b1).min(dist_to_segment(a1, b0, b1)).min(dist_to_segment(b0, a0, a1)).min(dist_to_segment(b1, a0, a1));
            (gap <= floor).then_some(Fault::Gap { a: ea, b: eb, gap })
        };
        if let Some(f) = box_pairs(&boxes, pair) {
            return Err(f);
        }
        // No crossings, so one point of a hole tells where the whole hole is.
        let outer = Outline { rings: vec![self.rings[0].clone()], chord_error: 0.0 };
        for hi in 1..self.rings.len() {
            let p = self.rings[hi][0];
            if !outer.contains(p) {
                return Err(Fault::HoleOutside { ring: hi });
            }
            for (hj, other) in self.rings.iter().enumerate().skip(1) {
                if hj != hi && (Outline { rings: vec![other.clone()], chord_error: 0.0 }).contains(p) {
                    return Err(Fault::HoleOutside { ring: hi });
                }
            }
        }
        Ok(())
    }
}

/// Every pair `(i, j)`, `i ≠ j`, of `boxes` (closed, `(lo, hi)`) that meet, each once, given to
/// `visit` until it answers something. A tree of the boxes, split at the median of their middles
/// along the axis where the middles spread farthest down to leaves of at most 8, descended two
/// branches at once wherever their boxes meet (as [`crate::Mesh::self_crossing`]'s): the work is
/// near `n log n` plus the pairs that meet, however unlike in size the boxes are, where a grid's
/// is not (a grid sized for the whole puts a spiral's small inner turns in a few cells).
pub(crate) fn box_pairs<T>(boxes: &[(Pt, Pt)], mut visit: impl FnMut(usize, usize) -> Option<T>) -> Option<T> {
    struct Node {
        lo: Pt,
        hi: Pt,
        kids: Option<(usize, usize)>,
        span: std::ops::Range<usize>,
    }
    if boxes.is_empty() {
        return None;
    }
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack = vec![(0usize, boxes.len(), usize::MAX, false)];
    while let Some((s, e, parent, right)) = stack.pop() {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        let (mut clo, mut chi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for &b in &order[s..e] {
            for k in 0..2 {
                lo[k] = lo[k].min(boxes[b].0[k]);
                hi[k] = hi[k].max(boxes[b].1[k]);
                let c = boxes[b].0[k] + boxes[b].1[k];
                clo[k] = clo[k].min(c);
                chi[k] = chi[k].max(c);
            }
        }
        let id = nodes.len();
        nodes.push(Node { lo, hi, kids: None, span: s..e });
        if parent != usize::MAX {
            let kids = nodes[parent].kids.get_or_insert((usize::MAX, usize::MAX));
            if right {
                kids.1 = id;
            } else {
                kids.0 = id;
            }
        }
        if e - s > 8 {
            let ax = usize::from(chi[1] - clo[1] > chi[0] - clo[0]);
            let mid = (s + e) / 2;
            let centre = |b: usize| boxes[b].0[ax] + boxes[b].1[ax];
            order[s..e].select_nth_unstable_by(mid - s, |&x, &y| centre(x).total_cmp(&centre(y)).then(x.cmp(&y)));
            stack.push((mid, e, id, true));
            stack.push((s, mid, id, false));
        }
    }
    let meet = |a: (Pt, Pt), b: (Pt, Pt)| (0..2).all(|k| a.0[k] <= b.1[k] && b.0[k] <= a.1[k]);
    let mut pairs = vec![(0usize, 0usize)];
    while let Some((a, b)) = pairs.pop() {
        let (na, nb) = (&nodes[a], &nodes[b]);
        if a != b && !meet((na.lo, na.hi), (nb.lo, nb.hi)) {
            continue;
        }
        match (na.kids, nb.kids) {
            (None, None) => {
                for (x, &i) in order[na.span.clone()].iter().enumerate() {
                    let from = if a == b { x + 1 } else { 0 };
                    for &j in &order[nb.span.clone()][from..] {
                        if meet(boxes[i], boxes[j]) {
                            if let Some(found) = visit(i, j) {
                                return Some(found);
                            }
                        }
                    }
                }
            }
            _ if a == b => {
                let (l, r) = na.kids.expect("a branch");
                pairs.extend([(l, l), (r, r), (l, r)]);
            }
            (Some((l, r)), _) if nb.kids.is_none() || na.span.len() >= nb.span.len() => pairs.extend([(l, b), (r, b)]),
            (_, Some((l, r))) => pairs.extend([(a, l), (a, r)]),
            (Some((l, r)), None) => pairs.extend([(l, b), (r, b)]),
        }
    }
    None
}

fn orient(a: Pt, b: Pt, c: Pt) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn on_segment(a: Pt, b: Pt, p: Pt) -> bool {
    p[0] >= a[0].min(b[0]) && p[0] <= a[0].max(b[0]) && p[1] >= a[1].min(b[1]) && p[1] <= a[1].max(b[1])
}

/// Whether the closed segments `a`-`b` and `c`-`d` share any point.
fn segments_meet(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
    if ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0)) && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0)) {
        return true;
    }
    (o1 == 0.0 && on_segment(a, b, c)) || (o2 == 0.0 && on_segment(a, b, d)) || (o3 == 0.0 && on_segment(c, d, a)) || (o4 == 0.0 && on_segment(c, d, b))
}

/// For two edges that share an end: whether they lie along one line and overlap (a fold back).
fn collinear_overlap(a0: Pt, a1: Pt, b0: Pt, b1: Pt) -> bool {
    let shared = if a1 == b0 { Some((a0, a1, b1)) } else if b1 == a0 { Some((b0, a0, a1)) } else { None };
    match shared {
        Some((p, q, r)) => {
            let u = sub(p, q);
            let v = sub(r, q);
            orient(p, q, r) == 0.0 && u[0] * v[0] + u[1] * v[1] > 0.0
        }
        None => false,
    }
}

impl Curve {
    /// The measures of what was built: its length (`boundary`) and bounds.
    pub fn measures(&self) -> Measures {
        let n = self.points.len();
        let mut l = 0.0;
        for i in 1..n {
            l += dist(self.points[i - 1], self.points[i]);
        }
        if self.closed && n > 1 {
            l += dist(self.points[n - 1], self.points[0]);
        }
        Measures { size: None, boundary: Some(l), centroid: None, bounds: bounds_of(self.points.iter()), series: None }
    }

    /// The curve's size: the diagonal of its bounds.
    pub fn size(&self) -> f64 {
        bounds_of(self.points.iter()).map(|(lo, hi)| len([hi[0] - lo[0], hi[1] - lo[1]])).unwrap_or(0.0)
    }

    /// Checks that the curve is well made: two points or more, every coordinate finite, and no
    /// step (the closing one too, if it is closed) shorter than [`EDGE_FLOOR`] of its size. A
    /// curve may cross itself (a figure eight does), so crossings are not looked for. A fault
    /// names ring 0.
    pub fn validate(&self) -> Result<(), Fault> {
        let n = self.points.len();
        if n < 2 {
            return Err(Fault::TooFew { ring: 0 });
        }
        if let Some(i) = self.points.iter().position(|p| !(p[0].is_finite() && p[1].is_finite())) {
            return Err(Fault::NotFinite { ring: 0, point: i });
        }
        let size = self.size();
        let steps = if self.closed { n } else { n - 1 };
        for i in 0..steps {
            let l = dist(self.points[i], self.points[(i + 1) % n]);
            if l <= size * EDGE_FLOOR {
                return Err(Fault::ShortEdge { ring: 0, point: i, length: l });
            }
        }
        Ok(())
    }

    /// The distance from `p` to the curve.
    pub fn distance(&self, p: Pt) -> f64 {
        let n = self.points.len();
        let mut d = f64::INFINITY;
        let last = if self.closed { n } else { n.saturating_sub(1) };
        for i in 0..last {
            d = d.min(dist_to_segment(p, self.points[i], self.points[(i + 1) % n]));
        }
        if n == 1 {
            d = dist(p, self.points[0]);
        }
        d
    }
}
