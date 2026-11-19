//! Shape language (docs/design/FORM.md): how round, square or pointed a silhouette is, as
//! numbers. Round reads as friendly, square as sturdy, pointed as dangerous: a convention of
//! character and prop design, not a law, so it is in every asset's taste measures and is
//! checked only when a style asks for one (`[taste] shape_language`).
//!
//! Measured on a silhouette mask. Its outline is traced and smoothed (so that the pixels'
//! steps are gone), and at every point of it the **net turn of the outline within a window**
//! a fourteenth of the shape's size long is taken. Along a straight side that is nothing,
//! whatever the pixels do; round a curve it is the window over the curve's radius; at a corner
//! it is the corner's whole turn. A point of the outline counts for that net turn, and counts
//! as part of a curve where it is small (under 40°, fading out by 75°), as a point where it
//! is large (from 95°, wholly by 125°: the corner's own angle is then under 85° to 55°), and
//! as a square corner between. The classes fade into each other: an outline that turns 60° at
//! its corners (a hexagon) is partly round and partly square whichever way it is turned,
//! where a hard line at 60° made it one or the other by the pixels' luck (its first review).
//!
//! So: a disc is round, and so is a box whose corners are rounded by a tenth of its size or
//! a capsule down to a sixth as thick as it is long; a box is square, and so is one whose
//! corners are rounded by a twentieth; a triangle and a star are pointed. What is smaller
//! than the window is not seen: the teeth of a comb, a bevel. A part thinner than about a
//! tenth of the size ends in a point at this scale, whatever its end.
//!
//! It needs pixels: the window is 18 pixels of a 256-pixel silhouette and 4 of a 64-pixel
//! one, where the pixels' steps are as large as what is measured. The studio measures it on
//! silhouettes of [`PIXELS`]; on a smaller mask it is coarser, and under [`MIN_SIZE`] it is
//! not taken.

use crate::image::Mask;
use specula_core::Json;

/// The net turn within the window under which a point of the outline is part of a curve,
/// and by which it is wholly not (degrees).
pub const ROUND_DEG: (f32, f32) = (50.0, 70.0);
/// The net turn from which a point of the outline begins to be a point, and by which it
/// wholly is (degrees).
pub const POINTED_DEG: (f32, f32) = (100.0, 112.0);
/// The window the net turn is taken in, as a share of the shape's size.
pub const WINDOW: f32 = 0.07;
/// Half the chord the outline's direction is taken along, as a share of the shape's size.
pub const CHORD: f32 = 0.02;
/// How far the outline is smoothed (a Gaussian's deviation), as a share of the shape's size,
/// and never less than two pixels.
pub const SMOOTH: f32 = 0.012;
/// How far (pixels) the straightened outline may lie from the pixels' own.
pub const STRAIGHTEN_PX: f32 = 1.5;
/// A net turn of this much or more is a corner (counted, in `corners`).
pub const CORNER_DEG: f32 = 60.0;
/// The side, in pixels, of the silhouettes the studio measures shape language on.
pub const PIXELS: u32 = 256;
/// A piece smaller than this across (pixels) has no shape to speak of.
pub const MIN_SIZE: f32 = 24.0;
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

/// A silhouette's shape language: the share of its outline's turning that is gentle, at
/// square corners and at points (they sum to 1), how much of its convex hull it fills, and
/// how many corners it has (places where the outline turns [`CORNER_DEG`] or more within the
/// window).
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
fn straighten(line: &[P], slack: f32, keep: &mut Vec<P>) {
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
    keep.extend(line.iter().zip(&kept).filter(|(_, k)| **k).map(|(p, _)| *p));
}

/// The convex hull's area of `points` (Andrew's chain).
fn hull_area(points: &[P]) -> f32 {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    sorted.dedup();
    if sorted.len() < 3 {
        return 0.0;
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
    area(&hull).abs()
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
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    if size < MIN_SIZE {
        return None;
    }
    // The pixels' steps are taken out first: the outline straightened to a polygon that
    // stays within a pixel of it (a slanted side, whose pixels step, is then one straight
    // side: smoothing alone left a ripple along it that read as a gentle curve, and a box
    // turned five degrees was a quarter round), and walked again a pixel at a time.
    let far = (0..points.len()).max_by(|a, b| dist(points[*a], points[0]).total_cmp(&dist(points[*b], points[0])).then(b.cmp(a)))?;
    let mut polygon: Vec<P> = Vec::new();
    straighten(&points[..=far], STRAIGHTEN_PX, &mut polygon);
    polygon.pop();
    let mut back: Vec<P> = points[far..].to_vec();
    back.push(points[0]);
    straighten(&back, STRAIGHTEN_PX, &mut polygon);
    polygon.pop();
    if polygon.len() < 3 {
        return None;
    }
    let mut walked: Vec<P> = Vec::new();
    for (i, a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        let steps = (dist(*a, b).round() as usize).max(1);
        walked.extend((0..steps).map(|k| [a[0] + (b[0] - a[0]) * k as f32 / steps as f32, a[1] + (b[1] - a[1]) * k as f32 / steps as f32]));
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
    let mut shares = [0.0f32; 3];
    let mut corners = 0u32;
    for (i, at) in net.iter().enumerate() {
        let by = at.abs().to_degrees();
        // Classed by the most the outline turns in any window this step is in: a step beside
        // a corner, whose own window holds only part of the corner's turn, is part of that
        // corner and not of a gentle curve.
        let most = (0..=2 * half).map(|k| net[(i + count - half + k) % count].abs()).fold(0.0f32, f32::max).to_degrees();
        let round = 1.0 - smoothstep(ROUND_DEG.0, ROUND_DEG.1, most);
        let pointed = smoothstep(POINTED_DEG.0, POINTED_DEG.1, most);
        shares[0] += by * round;
        shares[1] += by * (1.0 - round - pointed);
        shares[2] += by * pointed;
        // (A corner is counted where the outline comes to turn that much: once for each run
        // of steps that do.)
        if by >= CORNER_DEG && net[(i + count - 1) % count].abs().to_degrees() < CORNER_DEG {
            corners += 1;
        }
    }
    let all = shares[0] + shares[1] + shares[2];
    if all <= 0.0 {
        return None;
    }
    let hull = hull_area(&outline_points);
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
    /// units of the mask (0 to 1 across).
    fn mask(side: u32, inside: impl Fn(f32, f32) -> bool) -> Mask {
        let mut m = Mask::empty(side, side);
        for y in 0..side {
            for x in 0..side {
                m.on[(y * side + x) as usize] = inside((x as f32 + 0.5) / side as f32, (y as f32 + 0.5) / side as f32);
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

    fn turned(polygon: &[[f32; 2]], deg: f32) -> Vec<[f32; 2]> {
        let (s, c) = deg.to_radians().sin_cos();
        polygon.iter().map(|p| [0.5 + (p[0] - 0.5) * c - (p[1] - 0.5) * s, 0.5 + (p[0] - 0.5) * s + (p[1] - 0.5) * c]).collect()
    }

    /// Known shapes on a silhouette of the studio's size, **at every five degrees of a quarter
    /// turn** (its first review: upright, a box rounded by an eighth of its size was round and
    /// turned six degrees it was square; a bar was square or pointed by the angle; a capsule
    /// was pointed). For each, the least and the most of each share over the turns:
    ///
    /// - a disc, an ellipse of 2 to 1, an octagon: round;
    /// - a box, an L, a bar a fifth and an eighth as thick as it is long: square;
    /// - a triangle and a star: pointed;
    /// - a box rounded by a tenth and by a sixth of its size, a capsule of 4 to 1 and of 6
    ///   to 1: round (what the check's own advice says to build);
    /// - a box rounded by a twentieth: still square;
    /// - a hexagon is between round and square, and stays there;
    /// - a spike on a flat base is its point and its base's two square corners.
    #[test]
    fn known_shapes_speak_their_language_however_they_are_turned() {
        let poly = |points: Vec<[f32; 2]>| move |deg: f32| {
            let turned = turned(&points, deg);
            mask(PIXELS, move |x, y| within(&turned, x, y))
        };
        let regular = |sides: u32, r: f32| -> Vec<[f32; 2]> {
            (0..sides)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / sides as f32 - std::f32::consts::FRAC_PI_2;
                    [0.5 + r * a.cos(), 0.5 + r * a.sin()]
                })
                .collect()
        };
        // (A shape given by a test of points in its own frame, turned about the middle.)
        let by = |inside: fn(f32, f32) -> bool| move |deg: f32| {
            let (s, c) = (-deg).to_radians().sin_cos();
            mask(PIXELS, move |x, y| inside((x - 0.5) * c - (y - 0.5) * s, (x - 0.5) * s + (y - 0.5) * c))
        };
        fn rounded(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> bool {
            let (dx, dy) = (x.abs() - (hx - r), y.abs() - (hy - r));
            dx <= r && dy <= r && (dx <= 0.0 || dy <= 0.0 || dx.hypot(dy) <= r)
        }
        let star: Vec<[f32; 2]> = (0..10)
            .map(|i| {
                let (a, r) = (std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2, if i % 2 == 0 { 0.46 } else { 0.18 });
                [0.5 + r * a.cos(), 0.5 + r * a.sin()]
            })
            .collect();
        type Shape = Box<dyn Fn(f32) -> Mask>;
        // (Each within 0.47 of the middle, so that it stays in the mask however it is turned.)
        let shapes: Vec<(&str, Shape, Kind, f32)> = vec![
            ("a disc", Box::new(by(|x, y| x.hypot(y) < 0.44)), Kind::Round, 0.95),
            ("an ellipse of 2 to 1", Box::new(by(|x, y| (x / 0.45).hypot(y / 0.225) < 1.0)), Kind::Round, 0.8),
            ("an octagon", Box::new(poly(regular(8, 0.45))), Kind::Round, 0.8),
            ("a box", Box::new(poly(vec![[0.15, 0.2], [0.85, 0.2], [0.85, 0.8], [0.15, 0.8]])), Kind::Square, 0.9),
            ("an L", Box::new(poly(vec![[0.18, 0.18], [0.46, 0.18], [0.46, 0.54], [0.82, 0.54], [0.82, 0.82], [0.18, 0.82]])), Kind::Square, 0.9),
            ("a bar a fifth as thick as long", Box::new(by(|x, y| x.abs() < 0.45 && y.abs() < 0.09)), Kind::Square, 0.85),
            ("a bar an eighth as thick as long", Box::new(by(|x, y| x.abs() < 0.45 && y.abs() < 0.056)), Kind::Square, 0.6),
            ("a triangle", Box::new(poly(vec![[0.5, 0.08], [0.86, 0.74], [0.14, 0.74]])), Kind::Pointed, 0.85),
            ("a star", Box::new(poly(star)), Kind::Pointed, 0.6),
            ("a box rounded by a tenth of its size", Box::new(by(|x, y| rounded(x, y, 0.4, 0.3, 0.08))), Kind::Round, 0.8),
            ("a box rounded by a sixth of its size", Box::new(by(|x, y| rounded(x, y, 0.4, 0.3, 0.13))), Kind::Round, 0.9),
            ("a capsule of 4 to 1", Box::new(by(|x, y| rounded(x, y, 0.45, 0.1125, 0.1125))), Kind::Round, 0.9),
            ("a capsule of 6 to 1", Box::new(by(|x, y| rounded(x, y, 0.45, 0.075, 0.075))), Kind::Round, 0.6),
            ("a box rounded by a twentieth of its size", Box::new(by(|x, y| rounded(x, y, 0.4, 0.3, 0.04))), Kind::Square, 0.7),
        ];
        let turns: Vec<f32> = (0..=18).map(|i| i as f32 * 5.0).collect();
        for (name, shape, kind, least) in &shapes {
            let (mut lo, mut hi) = (1.0f32, 0.0f32);
            for deg in &turns {
                let a = of_mask(&shape(*deg)).unwrap_or_else(|| panic!("{name} turned {deg} has no shape"));
                assert_eq!(a.leading(), *kind, "{name} turned {deg}: {a:?}");
                lo = lo.min(a.share(*kind));
                hi = hi.max(a.share(*kind));
            }
            // (Steady to a tenth; the two on the edge of their class, the thinnest capsule and
            // the least rounded box, to a quarter.)
            let steady = if name.contains("6 to 1") || name.contains("twentieth") { 0.25 } else { 0.1 };
            assert!(lo >= *least && hi - lo <= steady, "{name}: {} from {lo:.2} to {hi:.2} over a quarter turn (at least {least})", kind.name());
        }
        // Between two classes, and steady there: a hexagon's corners turn 60 degrees.
        let hexagon = poly(regular(6, 0.45));
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        for deg in &turns {
            let a = of_mask(&hexagon(*deg)).unwrap();
            assert!(a.pointed < 0.05, "a hexagon turned {deg}: {a:?}");
            lo = lo.min(a.round);
            hi = hi.max(a.round);
        }
        assert!(lo > 0.2 && hi < 0.8 && hi - lo <= 0.2, "a hexagon is between round and square: round from {lo:.2} to {hi:.2}");
        // A spike on a flat base: its point, and its base's two square corners.
        let spike = by(|x, y| x.abs() < 0.4 && y.abs() < 0.12 * (0.4 - x) / 0.8);
        for deg in &turns {
            let a = of_mask(&spike(*deg)).unwrap();
            assert!(a.pointed > 0.35 && a.pointed < 0.65 && a.round < 0.1 && a.corners == 3.0, "a spike turned {deg}: {a:?}");
        }
        // And the corners are counted: four, six, three, ten; none on a disc.
        for (name, want) in [("a box", 4.0f32), ("an L", 6.0), ("a triangle", 3.0), ("a star", 10.0), ("a disc", 0.0)] {
            let shape = &shapes.iter().find(|s| s.0 == name).unwrap().1;
            for deg in [0.0f32, 15.0, 35.0] {
                assert_eq!(of_mask(&shape(deg)).unwrap().corners, want, "{name} turned {deg}");
            }
        }
    }

    /// What has no shape has no language, and the largest piece is the one read: an empty
    /// mask, a speck, a mask too large; a disc with a speck beside it is the disc; the mean of
    /// a disc and a box is half round and half square; the same mask gives the same numbers.
    #[test]
    fn what_is_measured_and_what_is_not() {
        assert_eq!(of_mask(&Mask::empty(PIXELS, PIXELS)), None);
        assert_eq!(of_mask(&mask(PIXELS, |x, y| (x - 0.5).hypot(y - 0.5) < 0.03)), None);
        assert_eq!(of_mask(&Mask::empty(600, 600)), None);
        let disc = mask(PIXELS, |x, y| (x - 0.4).hypot(y - 0.5) < 0.3);
        let with_speck = mask(PIXELS, |x, y| (x - 0.4).hypot(y - 0.5) < 0.3 || (x > 0.9 && y > 0.9));
        assert_eq!(of_mask(&disc), of_mask(&with_speck));
        let boxy = mask(PIXELS, |x, y| (0.2..0.8).contains(&x) && (0.25..0.75).contains(&y));
        let both = of_masks(&[disc.clone(), boxy, Mask::empty(PIXELS, PIXELS)]).unwrap();
        assert!((both.round - 0.5).abs() < 0.05 && (both.square - 0.5).abs() < 0.05 && both.corners == 2.0, "{both:?}");
        assert_eq!(of_masks(&[Mask::empty(8, 8)]), None);
        // (A disc with a hole is its outline: the hole is not traced.)
        let ring = mask(PIXELS, |x, y| (0.2..0.44).contains(&(x - 0.5).hypot(y - 0.5)));
        assert!(of_mask(&ring).unwrap().round > 0.95);
        // A part thinner than the window ends in a point at this scale, whatever its end: a
        // comb's flat teeth, a twentieth of the size across, are points.
        let comb = mask(PIXELS, |x, y| (0.1..0.9).contains(&x) && ((0.5..0.62).contains(&y) || ((0.38..0.5).contains(&y) && (((x - 0.1) / 0.04) as u32).is_multiple_of(2))));
        let a = of_mask(&comb).unwrap();
        assert!(a.pointed > 0.7 && a.leading() == Kind::Pointed, "a comb's thin teeth: {a:?}");
        // On a thumbnail's 64 pixels it is coarser, and still tells a disc from a box.
        assert!(of_mask(&mask(64, |x, y| (x - 0.5).hypot(y - 0.5) < 0.44)).unwrap().round > 0.8);
        assert!(of_mask(&mask(64, |x, y| (0.15..0.85).contains(&x) && (0.2..0.8).contains(&y))).unwrap().square > 0.6);
        let j = of_mask(&disc).unwrap().to_json();
        assert_eq!(j.get("leading").and_then(Json::as_str), Some("round"));
        assert_eq!(Kind::parse("pointed"), Some(Kind::Pointed));
        assert_eq!(Kind::parse("spiky"), None);
    }
}
