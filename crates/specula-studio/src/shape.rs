//! Shape language (docs/design/FORM.md): how round, square or pointed a silhouette is, as
//! numbers. Round reads as friendly, square as sturdy, pointed as dangerous: a convention of
//! character and prop design, not a law, so it is in every asset's taste measures and is
//! checked only when a style asks for one (`[taste] shape_language`).
//!
//! Measured on a silhouette mask. Its outline is traced and smoothed (so that the pixels'
//! steps are gone), and at every point of it the **net turn of the outline within a window**
//! a fourteenth of the shape's size long is taken. Along a straight side that is nothing,
//! whatever the pixels do; round a curve it is the window over the curve's radius; at a corner
//! it is the corner's whole turn. Then:
//!
//! - **Only outward turning counts**: the outline's bulges and its corners. Where it turns
//!   back on itself (a notch, a slot, the inside corner of an L, the cusp between two bumps
//!   of a cloud) it counts for nothing, so one small cut does not unmake a disc.
//! - A point of the outline counts for its outward turn, and is **part of a curve** where
//!   the most the outline turns in a window near it is small (under [`ROUND_DEG`]`.0`,
//!   fading out by `.1`), **a point** where that is large (from [`POINTED_DEG`]`.0`, wholly
//!   by `.1`: the corner's own angle is then under 84 to 64 degrees), and **a square
//!   corner** between. The classes fade into each other: an outline that turns 60 degrees at
//!   its corners (a hexagon) is partly round and partly square whichever way it is turned.
//! - **A point counts double** ([`POINT_WEIGHT`]): a spike's tip is what is seen of it, and
//!   by turning alone it is under half of a spike, whose base has two square corners.
//! - **The size is the shape's greatest width**, the same whichever way it is turned.
//!
//! So: a disc is round, and so is a box whose corners are rounded by a tenth of its size or
//! a capsule down to a sixth as thick as it is long, and a disc with a slot cut in it; a box
//! is square, and so is one whose corners are rounded by a thirtieth; a triangle, a spike, a
//! star and a crescent are pointed.
//!
//! **What it cannot say steadily**, by its own scale (the window):
//! - a corner rounded by about a twentieth to a fourteenth of the size is between square and
//!   round, and the share moves by up to a fifth as the shape is turned;
//! - a part about as thick as the window (a fifteenth of the size: a dagger's grip) ends
//!   between a square end and a point, and reads as either by the angle; thinner, it ends in
//!   a point whatever its end, and what is smaller than the window (a comb's teeth, a bevel)
//!   is seen as a ripple or not at all.
//!
//! It needs pixels: the studio measures it on silhouettes of [`PIXELS`], and a piece under
//! [`MIN_SIZE`] pixels across is not read at all (a thumbnail never is).

use crate::image::Mask;
use specula_core::Json;

/// The net turn within the window under which a point of the outline is part of a curve,
/// and by which it is wholly not (degrees). (A band this wide: a narrower one, 50 to 70, made
/// the share of a corner rounded by a sixteenth of the size swing from a fifth to nine tenths
/// by the angle the shape was turned to.)
pub const ROUND_DEG: (f32, f32) = (45.0, 80.0);
/// The net turn from which a point of the outline begins to be a point, and by which it
/// wholly is (degrees).
pub const POINTED_DEG: (f32, f32) = (96.0, 116.0);
/// The window the net turn is taken in, as a share of the shape's size.
pub const WINDOW: f32 = 0.07;
/// Half the chord the outline's direction is taken along, as a share of the shape's size.
pub const CHORD: f32 = 0.02;
/// How far the outline is smoothed (a Gaussian's deviation), as a share of the shape's size,
/// and never less than two pixels.
pub const SMOOTH: f32 = 0.012;
/// A run of the outline is straightened when it is at least this long, as a share of the
/// shape's size.
pub const STRAIGHT_RUN: f32 = 0.1;
/// How far (pixels) the straightened outline may lie from the pixels' own.
pub const STRAIGHTEN_PX: f32 = 1.5;
/// How much more a point counts for than a curve or a square corner of the same turn: a
/// sharp corner draws the eye before a right angle does. Without it a spike is never mostly
/// pointed: its tip turns under 180 degrees and the two corners of its base 90 each.
pub const POINT_WEIGHT: f32 = 2.0;
/// A net turn of this much or more is a corner (counted, in `corners`).
pub const CORNER_DEG: f32 = 60.0;
/// The side, in pixels, of the silhouettes the studio measures shape language on.
pub const PIXELS: u32 = 512;
/// A piece smaller than this across (pixels, its greatest width) is not read: under it the
/// pixels' steps are as large as what is measured, and a triangle of 96 pixels read as
/// anything from a tenth to wholly pointed by the angle it was turned to.
pub const MIN_SIZE: f32 = 110.0;
/// The largest mask traced (pixels).
pub const MAX_PIXELS: usize = 512 * 512;

/// What a style may ask most of an outline to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Round,
    Square,
    Pointed,
}

impl Kind {
    pub const NAMES: [&'static str; 3] = ["round", "square", "pointed"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn parse(name: &str) -> Option<Kind> {
        [Kind::Round, Kind::Square, Kind::Pointed].into_iter().find(|k| k.name() == name)
    }
}

/// A silhouette's shape language: the share of its outline's outward turning that is gentle,
/// at square corners and at points (a point counted double; they sum to 1), how much of its
/// convex hull it fills, and how many outward corners it has (places where the outline turns
/// outward by [`CORNER_DEG`] or more within the window: an L has five).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeLanguage {
    pub round: f32,
    pub square: f32,
    pub pointed: f32,
    /// The outline's area over its convex hull's: 1 for a convex shape.
    pub convexity: f32,
    pub corners: f32,
}

impl ShapeLanguage {
    pub fn share(&self, kind: Kind) -> f32 {
        match kind {
            Kind::Round => self.round,
            Kind::Square => self.square,
            Kind::Pointed => self.pointed,
        }
    }

    /// The kind with the largest share (round on a tie, then square).
    pub fn leading(&self) -> Kind {
        if self.round >= self.square && self.round >= self.pointed {
            Kind::Round
        } else if self.square >= self.pointed {
            Kind::Square
        } else {
            Kind::Pointed
        }
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f32| Json::from((f64::from(x) * 100.0).round() / 100.0);
        Json::obj([
            ("round", r(self.round)),
            ("square", r(self.square)),
            ("pointed", r(self.pointed)),
            ("leading", Json::from(self.leading().name())),
            ("convexity", r(self.convexity)),
            ("corners", r(self.corners)),
        ])
    }
}

type P = [f32; 2];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: P, b: P) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn dist(a: P, b: P) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

/// The outline of the mask's largest piece (pixels joined by a side or a corner), as the
/// corners of its pixels in order round it, a pixel's side apart. `None` for an empty mask or
/// one over [`MAX_PIXELS`].
fn outline(m: &Mask) -> Option<Vec<P>> {
    let (w, h) = (m.w as usize, m.h as usize);
    if w == 0 || h == 0 || w * h > MAX_PIXELS || m.on.len() != w * h {
        return None;
    }
    // The largest piece, by flood.
    let mut piece = vec![0u32; w * h];
    let (mut best, mut best_size, mut next) = (0u32, 0usize, 0u32);
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if !m.on[start] || piece[start] != 0 {
            continue;
        }
        next += 1;
        let mut size = 0usize;
        piece[start] = next;
        stack.push(start);
        while let Some(at) = stack.pop() {
            size += 1;
            let (x, y) = ((at % w) as i32, (at / w) as i32);
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let n = ny as usize * w + nx as usize;
                if m.on[n] && piece[n] == 0 {
                    piece[n] = next;
                    stack.push(n);
                }
            }
        }
        if size > best_size {
            (best, best_size) = (next, size);
        }
    }
    if best == 0 {
        return None;
    }
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x < w as i32 && y < h as i32 && piece[y as usize * w + x as usize] == best;
    // Round the piece along its pixels' sides, the piece on the right hand: from the top left
    // corner of its first pixel (the leftmost of its top row), heading right.
    let first = (0..w * h).find(|i| piece[*i] == best)?;
    let start = ((first % w) as i32, (first / w) as i32);
    let (mut at, mut heading) = (start, 0usize);
    let step = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let mut points: Vec<P> = Vec::new();
    for _ in 0..4 * (w + 1) * (h + 1) + 4 {
        // The two pixels ahead of this corner: to the left of the way ahead, and to the right.
        let (x, y) = at;
        let (left, right) = match heading {
            0 => ((x, y - 1), (x, y)),
            1 => ((x, y), (x - 1, y)),
            2 => ((x - 1, y), (x - 1, y - 1)),
            _ => ((x - 1, y - 1), (x, y - 1)),
        };
        let turned = if inside(left.0, left.1) {
            (heading + 3) % 4
        } else if inside(right.0, right.1) {
            heading
        } else {
            (heading + 1) % 4
        };
        // (Back at the first corner and leaving it as at first: once round.)
        if at == start && turned == 0 && !points.is_empty() {
            break;
        }
        points.push([x as f32, y as f32]);
        heading = turned;
        at = (x + step[heading].0, y + step[heading].1);
    }
    (points.len() >= 4).then_some(points)
}

/// The points of `line` (open, at least two) that a straightened line keeps: its ends, and
/// between two kept points the one furthest from the straight line through them while it is
/// further than `slack` (Douglas and Peucker's rule).
fn straighten(line: &[P], slack: f32, base: usize, keep: &mut Vec<usize>) {
    let mut spans = vec![(0usize, line.len() - 1)];
    let mut kept = vec![false; line.len()];
    kept[0] = true;
    kept[line.len() - 1] = true;
    while let Some((a, b)) = spans.pop() {
        if b <= a + 1 {
            continue;
        }
        let along = sub(line[b], line[a]);
        let length = dist(line[a], line[b]);
        let (mut far, mut most) = (a, slack);
        for (i, p) in line.iter().enumerate().take(b).skip(a + 1) {
            let off = if length > 1e-6 { cross(along, sub(*p, line[a])).abs() / length } else { dist(*p, line[a]) };
            if off > most {
                (far, most) = (i, off);
            }
        }
        if far != a {
            kept[far] = true;
            spans.push((a, far));
            spans.push((far, b));
        }
    }
    keep.extend(kept.iter().enumerate().filter(|(_, k)| **k).map(|(i, _)| base + i));
}

/// The convex hull of `points` (Andrew's chain); empty for fewer than three apart.
fn hull_of(points: &[P]) -> Vec<P> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    sorted.dedup();
    if sorted.len() < 3 {
        return Vec::new();
    }
    let mut hull: Vec<P> = Vec::new();
    for pass in 0..2 {
        let from = hull.len();
        let walk: Box<dyn Iterator<Item = &P>> = if pass == 0 { Box::new(sorted.iter()) } else { Box::new(sorted.iter().rev()) };
        for p in walk {
            while hull.len() >= from + 2 && cross(sub(hull[hull.len() - 1], hull[hull.len() - 2]), sub(*p, hull[hull.len() - 1])) <= 0.0 {
                hull.pop();
            }
            hull.push(*p);
        }
        hull.pop();
    }
    hull
}

/// The furthest two points of a hull are apart: the shape's greatest width, whichever way it
/// is turned. (Every pair of a thinned hull: a hull of a 512-pixel disc has under two thousand
/// points, and one in `HULL_STEP` of them is tried against the rest.)
fn greatest_width(hull: &[P]) -> f32 {
    const HULL_STEP: usize = 4;
    let mut most = 0.0f32;
    for a in hull.iter().step_by(HULL_STEP) {
        for b in hull {
            most = most.max(dist(*a, *b));
        }
    }
    most
}

fn area(polygon: &[P]) -> f32 {
    let n = polygon.len();
    (0..n).map(|i| cross(polygon[i], polygon[(i + 1) % n])).sum::<f32>() * 0.5
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One silhouette's shape language. `None` for an empty mask, one too large to trace, or a
/// piece too small to have a shape (under [`MIN_SIZE`] pixels across).
pub fn of_mask(m: &Mask) -> Option<ShapeLanguage> {
    let points = outline(m)?;
    // The shape's size is its greatest width: the same whichever way it is turned. (The
    // longer side of the box round it grows by two fifths as a square is turned to stand on
    // a corner, and every length here is a share of the size: the same corner was a curve at
    // one angle and a corner at another. Its second review.)
    let hull = hull_of(&points);
    let size = greatest_width(&hull);
    if size < MIN_SIZE {
        return None;
    }
    // The pixels' steps are taken out first: the outline straightened to a polygon that
    // stays within a pixel of it (a slanted side, whose pixels step, is then one straight
    // side: smoothing alone left a ripple along it that read as a gentle curve, and a box
    // turned five degrees was a quarter round), and walked again a pixel at a time.
    let far = (0..points.len()).max_by(|a, b| dist(points[*a], points[0]).total_cmp(&dist(points[*b], points[0])).then(b.cmp(a)))?;
    let mut kept: Vec<usize> = Vec::new();
    straighten(&points[..=far], STRAIGHTEN_PX, 0, &mut kept);
    kept.pop();
    let mut back: Vec<P> = points[far..].to_vec();
    back.push(points[0]);
    straighten(&back, STRAIGHTEN_PX, far, &mut kept);
    kept.pop();
    if kept.len() < 3 {
        return None;
    }
    // Only a long run is a side to straighten. A short one is a piece of a curve, and a curve
    // made a polygon has corners of its own, laid by where the tracing happened to begin: a
    // corner rounded by a sixteenth of the size read as round or as square by the angle the
    // box was turned to (its second review). A curve keeps its pixels, and the smoothing
    // below, which is as long as their steps there, takes them out.
    let long = (STRAIGHT_RUN * size).max(8.0);
    let mut walked: Vec<P> = Vec::new();
    for (k, i) in kept.iter().enumerate() {
        let j = kept[(k + 1) % kept.len()];
        let (a, b) = (points[*i], points[j % points.len()]);
        if dist(a, b) >= long {
            let steps = (dist(a, b).round() as usize).max(1);
            walked.extend((0..steps).map(|k| [a[0] + (b[0] - a[0]) * k as f32 / steps as f32, a[1] + (b[1] - a[1]) * k as f32 / steps as f32]));
        } else {
            let end = if j > *i { j } else { points.len() };
            walked.extend_from_slice(&points[*i..end]);
        }
    }
    let outline_points = points;
    let points = walked;
    // Then smoothed: each point the weighted mean of its neighbours along it, by a Gaussian
    // of a deviation of `SMOOTH` of the size (the outline is closed: round it goes), so that
    // the polygon's own corners on a curve run together into the curve.
    let n = points.len();
    let sigma = (SMOOTH * size).max(2.0);
    let reach = (3.0 * sigma).ceil() as usize;
    let weights: Vec<f32> = (0..=reach).map(|k| specula_core::dmath::exp(-0.5 * (k as f32 / sigma) * (k as f32 / sigma))).collect();
    let total: f32 = weights[0] + 2.0 * weights[1..].iter().sum::<f32>();
    let smooth: Vec<P> = (0..n)
        .map(|i| {
            let mut at = [points[i][0] * weights[0], points[i][1] * weights[0]];
            for (k, w) in weights.iter().enumerate().skip(1) {
                let (a, b) = (points[(i + k) % n], points[(i + n - k % n) % n]);
                at = [at[0] + (a[0] + b[0]) * w, at[1] + (a[1] + b[1]) * w];
            }
            [at[0] / total, at[1] / total]
        })
        .collect();
    // Walked again in steps of one pixel of its own length (smoothed, a slanted side's steps
    // are shorter than the pixels were).
    let length: f32 = (0..n).map(|i| dist(smooth[i], smooth[(i + 1) % n])).sum();
    let count = length.round() as usize;
    let chord = ((CHORD * size).round() as usize).max(1);
    let half = ((WINDOW * size * 0.5).round() as usize).max(1);
    if count < 8 * half.max(chord) {
        return None;
    }
    let step = length / count as f32;
    let mut even: Vec<P> = Vec::with_capacity(count);
    let (mut i, mut walked, mut want) = (0usize, 0.0f32, 0.0f32);
    while even.len() < count {
        let (a, b) = (smooth[i % n], smooth[(i + 1) % n]);
        let side = dist(a, b);
        if side > 0.0 && walked + side >= want {
            let t = (want - walked) / side;
            even.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            want += step;
        } else {
            walked += side;
            i += 1;
            if i > 2 * n {
                break;
            }
        }
    }
    let count = even.len();
    if count < 8 * half.max(chord) {
        return None;
    }
    // The outline's direction at each step (along a chord about it), how far it turns from
    // one step to the next, and the net turn within the window about each step.
    // (From the middle of the `chord` steps behind to the middle of those ahead: the mean of
    // many points is steadier than two, and what is left of the pixels' steps on a slanted
    // side made a square corner read as anything from 75 to 108 degrees.)
    let mut ahead = [0.0f32; 2];
    let mut behind = [0.0f32; 2];
    for k in 1..=chord {
        let (a, b) = (even[k % count], even[(count - k) % count]);
        ahead = [ahead[0] + a[0], ahead[1] + a[1]];
        behind = [behind[0] + b[0], behind[1] + b[1]];
    }
    let heading: Vec<f32> = (0..count)
        .map(|i| {
            let d = sub(ahead, behind);
            // (On a step: the point just ahead joins the ones behind, and so round.)
            let (enter, leave) = (even[(i + chord + 1) % count], even[(i + 1) % count]);
            ahead = [ahead[0] + enter[0] - leave[0], ahead[1] + enter[1] - leave[1]];
            let (enter, leave) = (even[i], even[(i + count - chord) % count]);
            behind = [behind[0] + enter[0] - leave[0], behind[1] + enter[1] - leave[1]];
            // (The library's own arc tangent: the same on every machine.)
            specula_core::dmath::atan2(d[1], d[0])
        })
        .collect();
    let turn: Vec<f32> = (0..count)
        .map(|i| {
            let d = heading[(i + 1) % count] - heading[i];
            if d > std::f32::consts::PI {
                d - std::f32::consts::TAU
            } else if d < -std::f32::consts::PI {
                d + std::f32::consts::TAU
            } else {
                d
            }
        })
        .collect();
    let mut net = vec![0.0f32; count];
    let mut run: f32 = (0..2 * half).map(|j| turn[(count - half + j) % count]).sum();
    for (i, slot) in net.iter_mut().enumerate() {
        *slot = run;
        // (The window moves on a step: the turn it leaves behind out, the one ahead in.)
        run += turn[(i + half) % count] - turn[(i + count - half) % count];
    }
    // Which way round the outline goes: a closed outline turns once in all, one way or the
    // other. Only where it turns that way (outward: its bulges and its corners) does it count;
    // where it turns back on itself (a notch, a slot, the inside corner of an L) it does not.
    let way = if turn.iter().sum::<f32>() >= 0.0 { 1.0f32 } else { -1.0 };
    let out: Vec<f32> = net.iter().map(|n| (n * way).max(0.0).to_degrees()).collect();
    let mut shares = [0.0f32; 3];
    let mut corners = 0u32;
    for (i, by) in out.iter().copied().enumerate() {
        // Classed by the most the outline turns outward in any window near this step: a step
        // beside a corner, whose own window holds only part of the corner's turn, is part of
        // that corner and not of a gentle curve.
        let most = (0..=2 * half).map(|k| out[(i + count - half + k) % count]).fold(0.0f32, f32::max);
        let round = 1.0 - smoothstep(ROUND_DEG.0, ROUND_DEG.1, most);
        let pointed = smoothstep(POINTED_DEG.0, POINTED_DEG.1, most);
        shares[0] += by * round;
        shares[1] += by * (1.0 - round - pointed);
        // (A point draws the eye more than a right angle does: it counts for more.)
        shares[2] += by * pointed * POINT_WEIGHT;
        // (A corner is counted where the outline comes to turn that much: once for each run
        // of steps that do.)
        if by >= CORNER_DEG && out[(i + count - 1) % count] < CORNER_DEG {
            corners += 1;
        }
    }
    let all = shares[0] + shares[1] + shares[2];
    if all <= 0.0 {
        return None;
    }
    let hull = area(&hull).abs();
    Some(ShapeLanguage {
        round: shares[0] / all,
        square: shares[1] / all,
        pointed: shares[2] / all,
        convexity: if hull > 0.0 { (area(&outline_points).abs() / hull).min(1.0) } else { 1.0 },
        corners: corners as f32,
    })
}

/// The shape language of an asset's silhouettes from several sides: the mean of each.
/// `None` when none of them has a shape.
pub fn of_masks(masks: &[Mask]) -> Option<ShapeLanguage> {
    let each: Vec<ShapeLanguage> = masks.iter().filter_map(of_mask).collect();
    if each.is_empty() {
        return None;
    }
    let n = each.len() as f32;
    let mean = |f: fn(&ShapeLanguage) -> f32| each.iter().map(f).sum::<f32>() / n;
    Some(ShapeLanguage { round: mean(|s| s.round), square: mean(|s| s.square), pointed: mean(|s| s.pointed), convexity: mean(|s| s.convexity), corners: mean(|s| s.corners) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mask of `side` pixels whose pixels are on where `inside` says, at their middles, in
    /// units of the mask about its middle (-0.5 to 0.5 across), the shape turned by `deg`.
    fn mask(side: u32, deg: f32, inside: impl Fn(f32, f32) -> bool) -> Mask {
        let (s, c) = (-deg).to_radians().sin_cos();
        let mut m = Mask::empty(side, side);
        for y in 0..side {
            for x in 0..side {
                let (px, py) = ((x as f32 + 0.5) / side as f32 - 0.5, (y as f32 + 0.5) / side as f32 - 0.5);
                m.on[(y * side + x) as usize] = inside(px * c - py * s, px * s + py * c);
            }
        }
        m
    }

    /// Whether (x, y) is inside the polygon (even-odd).
    fn within(polygon: &[[f32; 2]], x: f32, y: f32) -> bool {
        let n = polygon.len();
        let mut odd = false;
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            if (a[1] > y) != (b[1] > y) && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                odd = !odd;
            }
        }
        odd
    }

    /// A box of half sizes `hx`, `hy` with its corners rounded by `r`.
    fn rounded(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> bool {
        let (dx, dy) = (x.abs() - (hx - r), y.abs() - (hy - r));
        dx <= r && dy <= r && (dx <= 0.0 || dy <= 0.0 || dx.hypot(dy) <= r)
    }

    fn regular(x: f32, y: f32, sides: usize, r: f32, inner: Option<f32>) -> bool {
        let n = if inner.is_some() { 2 * sides } else { sides };
        let p: Vec<[f32; 2]> = (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
                let r = if i % 2 == 1 { inner.unwrap_or(r) } else { r };
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        within(&p, x, y)
    }

    type Inside = fn(f32, f32) -> bool;

    /// The least and the most of each share as the shape is turned through a quarter turn in
    /// steps of `step` degrees, on a silhouette of the studio's size, and what led at each.
    fn turned(inside: Inside, step: f32) -> ([f32; 3], [f32; 3], Vec<Kind>, Vec<f32>) {
        let (mut lo, mut hi, mut lead, mut corners) = ([1.0f32; 3], [0.0f32; 3], Vec::new(), Vec::new());
        let mut deg = 0.0f32;
        while deg < 90.0 - 1e-3 {
            let a = of_mask(&mask(PIXELS, deg, inside)).unwrap_or_else(|| panic!("no shape at {deg} degrees"));
            let sum = a.round + a.square + a.pointed;
            assert!((sum - 1.0).abs() < 1e-3 && [a.round, a.square, a.pointed].iter().all(|v| (0.0..=1.0).contains(v)), "shares at {deg}: {a:?}");
            for (k, v) in [a.round, a.square, a.pointed].into_iter().enumerate() {
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
            lead.push(a.leading());
            corners.push(a.corners);
            deg += step;
        }
        (lo, hi, lead, corners)
    }

    /// Known shapes on a silhouette of the studio's size, **however they are turned** (its
    /// first review: upright, a box rounded by an eighth of its size was round and turned six
    /// degrees it was square; its second: a spike read under half pointed, a disc with one
    /// slot cut in it failed "round", an ellipse of three to one moved by a third). For each:
    /// what leads at every angle, the least its share is, and how far the share moves.
    #[test]
    fn known_shapes_speak_their_language_however_they_are_turned() {
        let shapes: [(&str, Inside, Kind, f32); 28] = [
            ("a disc", |x, y| x.hypot(y) < 0.44, Kind::Round, 0.97),
            ("an ellipse of 3 to 1", |x, y| (x / 0.45).hypot(y / 0.15) < 1.0, Kind::Round, 0.7),
            ("an octagon", |x, y| regular(x, y, 8, 0.45, None), Kind::Round, 0.9),
            ("a capsule of 4 to 1", |x, y| rounded(x, y, 0.45, 0.1125, 0.1125), Kind::Round, 0.9),
            ("a capsule of 6 to 1", |x, y| rounded(x, y, 0.45, 0.075, 0.075), Kind::Round, 0.85),
            // (A box of 0.8 by 0.6 is 1 across, corner to corner: its rounding is a share of that.)
            ("a box rounded by a twelfth of its size", |x, y| rounded(x, y, 0.4, 0.3, 0.08), Kind::Round, 0.9),
            ("a box rounded by a tenth of its size", |x, y| rounded(x, y, 0.4, 0.3, 0.10), Kind::Round, 0.95),
            ("a disc with a slot cut in it", |x, y| x.hypot(y) < 0.44 && !(x.abs() < 0.01 && y > 0.34), Kind::Round, 0.55),
            ("a disc with a notch cut in it", |x, y| x.hypot(y) < 0.44 && !(y > 0.24 && x.abs() < (y - 0.24) * 0.5), Kind::Round, 0.55),
            ("a cloud of five bumps", |x, y| x.hypot(y) < 0.2 || (x - 0.22).hypot(y - 0.05) < 0.16 || (x + 0.22).hypot(y - 0.05) < 0.16 || (x - 0.1).hypot(y + 0.14) < 0.15 || (x + 0.1).hypot(y + 0.14) < 0.15, Kind::Round, 0.9),
            ("a box", |x, y| x.abs() < 0.35 && y.abs() < 0.3, Kind::Square, 0.95),
            ("an L", |x, y| within(&[[-0.32, -0.32], [-0.04, -0.32], [-0.04, 0.04], [0.32, 0.04], [0.32, 0.32], [-0.32, 0.32]], x, y), Kind::Square, 0.95),
            ("a cross", |x, y| (x.abs() < 0.44 && y.abs() < 0.12) || (x.abs() < 0.12 && y.abs() < 0.44), Kind::Square, 0.95),
            ("a bar a fifth as thick as long", |x, y| x.abs() < 0.45 && y.abs() < 0.09, Kind::Square, 0.95),
            ("a bar an eighth as thick as long", |x, y| x.abs() < 0.45 && y.abs() < 0.056, Kind::Square, 0.95),
            ("a box rounded by a fiftieth of its size", |x, y| rounded(x, y, 0.4, 0.3, 0.02), Kind::Square, 0.93),
            ("a box rounded by a twenty-fifth of its size", |x, y| rounded(x, y, 0.4, 0.3, 0.04), Kind::Square, 0.78),
            ("a house (a box under a right-angled roof)", |x, y| within(&[[-0.3, 0.4], [-0.3, -0.1], [0.0, -0.4], [0.3, -0.1], [0.3, 0.4]], x, y), Kind::Square, 0.65),
            ("a pentagon", |x, y| regular(x, y, 5, 0.45, None), Kind::Square, 0.7),
            ("a triangle", |x, y| within(&[[0.0, -0.42], [0.36, 0.24], [-0.36, 0.24]], x, y), Kind::Pointed, 0.95),
            ("a star of five points", |x, y| regular(x, y, 5, 0.46, Some(0.18)), Kind::Pointed, 0.95),
            ("a star of twelve points", |x, y| regular(x, y, 12, 0.46, Some(0.30)), Kind::Pointed, 0.95),
            ("a spike of 8 degrees", |x, y| within(&[[0.0, -0.44], [0.0615, 0.44], [-0.0615, 0.44]], x, y), Kind::Pointed, 0.55),
            ("a spike of 15 degrees", |x, y| within(&[[0.0, -0.44], [0.1158, 0.44], [-0.1158, 0.44]], x, y), Kind::Pointed, 0.55),
            ("a spike of 25 degrees", |x, y| within(&[[0.0, -0.44], [0.195, 0.44], [-0.195, 0.44]], x, y), Kind::Pointed, 0.55),
            ("a thin diamond", |x, y| x.abs() / 0.11 + y.abs() / 0.44 < 1.0, Kind::Pointed, 0.85),
            ("a crescent", |x, y| x.hypot(y) < 0.44 && (x - 0.18).hypot(y) > 0.36, Kind::Pointed, 0.65),
            ("a box with a spike on it", |x, y| (x.abs() < 0.3 && y.abs() < 0.2) || within(&[[-0.05, -0.2], [0.05, -0.2], [0.0, -0.44]], x, y), Kind::Square, 0.48),
        ];
        for (name, inside, kind, least) in shapes {
            let (lo, hi, lead, _) = turned(inside, 10.0);
            let k = kind as usize;
            assert!(lead.iter().all(|l| *l == kind), "{name} leads {lead:?}");
            // Steady to a tenth.
            assert!(lo[k] >= least && hi[k] - lo[k] <= 0.1, "{name}: {} from {:.2} to {:.2} over a quarter turn (at least {least})", kind.name(), lo[k], hi[k]);
        }
        // Between two classes, and steady there: a hexagon's corners turn 60 degrees.
        let (lo, hi, _, _) = turned(|x, y| regular(x, y, 6, 0.45, None), 10.0);
        assert!(lo[0] > 0.5 && hi[0] < 0.8 && hi[0] - lo[0] <= 0.1 && hi[2] < 0.02, "a hexagon is between round and square: round from {:.2} to {:.2}", lo[0], hi[0]);
        // Outward corners are counted: four, five (an L's inside corner is not one), three,
        // five; none on a disc.
        for (name, want) in [("a box", 4.0f32), ("an L", 5.0), ("a triangle", 3.0), ("a star of five points", 5.0), ("a disc", 0.0)] {
            let inside = shapes.iter().find(|s| s.0 == name).unwrap().1;
            for deg in [0.0f32, 15.0, 35.0] {
                assert_eq!(of_mask(&mask(PIXELS, deg, inside)).unwrap().corners, want, "{name} turned {deg}");
            }
        }
    }

    /// **A corner's rounding moves the answer smoothly** (its second review: between the two
    /// radii its first test tried, a box rounded by a sixteenth of its size read as a fifth
    /// round at one angle and nine tenths at another, a degree apart; the size was the longer
    /// side of the box round the shape, which grows as it is turned). At every rounding from
    /// a fiftieth to a tenth of the size, at every three degrees: the share moves by a
    /// quarter at most, and the more rounded box is the rounder at its middle.
    #[test]
    fn a_corners_rounding_moves_the_answer_smoothly() {
        let boxes: [(f32, Inside); 7] = [
            (0.02, |x, y| rounded(x, y, 0.4, 0.3, 0.02)),
            (0.04, |x, y| rounded(x, y, 0.4, 0.3, 0.04)),
            (0.05, |x, y| rounded(x, y, 0.4, 0.3, 0.05)),
            (0.055, |x, y| rounded(x, y, 0.4, 0.3, 0.055)),
            (0.06, |x, y| rounded(x, y, 0.4, 0.3, 0.06)),
            (0.07, |x, y| rounded(x, y, 0.4, 0.3, 0.07)),
            (0.10, |x, y| rounded(x, y, 0.4, 0.3, 0.10)),
        ];
        let mut last = -1.0f32;
        for (r, inside) in boxes {
            let (lo, hi, _, _) = turned(inside, 3.0);
            assert!(hi[0] - lo[0] <= 0.25, "a box rounded by {r} of its size: round from {:.2} to {:.2} as it is turned", lo[0], hi[0]);
            assert!(hi[2] < 0.02, "a box rounded by {r}: nothing of it is a point ({:.2})", hi[2]);
            let middle = (lo[0] + hi[0]) / 2.0;
            assert!(middle >= last - 0.02, "a box rounded by {r} is no less round than one rounded by less: {middle:.2} after {last:.2}");
            last = middle;
        }
        assert!(last > 0.95, "rounded by a tenth, it is round: {last:.2}");
    }

    /// What has no shape has no language, and the largest piece is the one read: an empty
    /// mask, a speck, a mask too large, a piece under the size it reads at (a thumbnail: its
    /// second review found a 64-pixel square read as wholly round at one angle in five); a
    /// disc with a speck beside it is the disc; the mean of a disc and a box is half round
    /// and half square; the same mask gives the same numbers, and half the pixels much the same.
    #[test]
    fn what_is_measured_and_what_is_not() {
        let disc: Inside = |x, y| (x + 0.1).hypot(y) < 0.3;
        let boxy: Inside = |x, y| x.abs() < 0.3 && y.abs() < 0.25;
        assert_eq!(of_mask(&Mask::empty(PIXELS, PIXELS)), None);
        assert_eq!(of_mask(&mask(PIXELS, 0.0, |x, y| x.hypot(y) < 0.03)), None);
        assert_eq!(of_mask(&Mask::empty(600, 600)), None);
        for small in [24u32, 64, 96] {
            assert_eq!(of_mask(&mask(small, 0.0, boxy)), None, "a box on {small} pixels is too small to read");
            assert_eq!(of_mask(&mask(small, 0.0, |x, y| x.hypot(y) < 0.44)), None, "a disc on {small} pixels");
        }
        assert_eq!(of_mask(&mask(PIXELS, 0.0, disc)), of_mask(&mask(PIXELS, 0.0, |x, y| (x + 0.1).hypot(y) < 0.3 || (x > 0.4 && y > 0.4))));
        let both = of_masks(&[mask(PIXELS, 0.0, disc), mask(PIXELS, 0.0, boxy), Mask::empty(PIXELS, PIXELS)]).unwrap();
        assert!((both.round - 0.5).abs() < 0.05 && (both.square - 0.5).abs() < 0.05 && both.corners == 2.0, "{both:?}");
        assert_eq!(of_masks(&[Mask::empty(8, 8)]), None);
        assert_eq!(of_masks(&[]), None);
        // (A disc with a hole is its outline: the hole is not traced.)
        assert!(of_mask(&mask(PIXELS, 0.0, |x, y| (0.2..0.44).contains(&x.hypot(y)))).unwrap().round > 0.95);
        // A part thinner than the window ends in a point at this scale, whatever its end: a
        // comb's flat teeth, a twentieth of the size across, are points.
        let comb: Inside = |x, y| x.abs() < 0.4 && ((0.0..0.12).contains(&y) || ((-0.12..0.0).contains(&y) && (((x + 0.4) / 0.04) as u32).is_multiple_of(2)));
        let a = of_mask(&mask(PIXELS, 0.0, comb)).unwrap();
        assert!(a.pointed > 0.7 && a.leading() == Kind::Pointed, "a comb's thin teeth: {a:?}");
        // The same shape on half the pixels (still over the size it reads at) says the same.
        for (name, inside, kind) in [("a disc", disc, Kind::Round), ("a box", boxy, Kind::Square)] {
            let (full, half) = (of_mask(&mask(PIXELS, 20.0, inside)).unwrap(), of_mask(&mask(PIXELS / 2, 20.0, inside)).unwrap());
            assert!((full.share(kind) - half.share(kind)).abs() < 0.1 && half.leading() == kind, "{name}: {full:?} and on half the pixels {half:?}");
        }
        let j = of_mask(&mask(PIXELS, 0.0, disc)).unwrap().to_json();
        assert_eq!(j.get("leading").and_then(Json::as_str), Some("round"));
        assert_eq!(Kind::parse("pointed"), Some(Kind::Pointed));
        assert_eq!(Kind::parse("spiky"), None);
    }
}
