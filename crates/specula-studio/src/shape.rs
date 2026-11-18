//! Shape language (docs/design/FORM.md): how round, square or pointed a silhouette is, as
//! numbers. Round reads as friendly, square as sturdy, pointed as dangerous: a convention of
//! character and prop design, not a law, so it is in every asset's taste measures and is
//! checked only when a style asks for one (`[taste] shape_language`).
//!
//! Measured on a silhouette mask: its outline is traced, straightened to a polygon that stays
//! within a fortieth of the shape's size of it, and the polygon's corners are told apart by how
//! far the outline turns at each. A corner the outline turns gently at (under 60°) is part of
//! a curve; one of 60° to 105° is a square corner; a sharper one is a point. Corners closer
//! together than a tenth of the size are one corner (a small bevel or a tight rounding does
//! not make a square corner round, and a corner on a slanted outline, where the pixels step,
//! is not read as two); two sharp corners stay two unless they are a pixel or two apart, so
//! the cut end of a bar is two square corners and the tip of a spike is one point. The shares are of the outline's whole turning, so a
//! disc is all round, a box all square, a triangle or a star mostly pointed.

use crate::image::Mask;
use specula_core::Json;

/// A corner the outline turns less than this at is part of a curve (degrees).
pub const ROUND_BELOW_DEG: f32 = 60.0;
/// A corner the outline turns this much or more at is a point (degrees): the corner's own
/// angle is then 75° or less.
pub const POINTED_FROM_DEG: f32 = 105.0;
/// How far the polygon may lie from the outline, as a share of the shape's size.
pub const STRAIGHTEN: f32 = 0.025;
/// Corners closer together than this share of the shape's size are one corner.
pub const MERGE: f32 = 0.1;
/// The largest mask traced (pixels): a studio thumbnail is 64 to 256 a side.
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
/// how many corners of 60° or more it has.
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
/// corners of its pixels in order round it, one where the outline turns. `None` for an empty
/// mask or one over [`MAX_PIXELS`].
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
        if turned != heading || points.is_empty() {
            points.push([x as f32, y as f32]);
        }
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

/// One silhouette's shape language. `None` for an empty mask, one too large to trace, or a
/// piece too small to have a shape (under 8 pixels across).
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
    if size < 8.0 {
        return None;
    }
    // Straightened in two halves, between the first point and the one furthest from it.
    let far = (0..points.len()).max_by(|a, b| dist(points[*a], points[0]).total_cmp(&dist(points[*b], points[0])).then(b.cmp(a)))?;
    let slack = (STRAIGHTEN * size).max(1.0);
    let mut polygon: Vec<P> = Vec::new();
    straighten(&points[..=far], slack, &mut polygon);
    polygon.pop();
    let mut back: Vec<P> = points[far..].to_vec();
    back.push(points[0]);
    straighten(&back, slack, &mut polygon);
    polygon.pop();
    let n = polygon.len();
    if n < 3 {
        return None;
    }
    // The turn at each corner, and corners close together taken as one: round the outline
    // from the longest side's end, so that no corner is cut in two by where the walk begins.
    let turn = |i: usize| -> f32 {
        let (a, b, c) = (polygon[(i + n - 1) % n], polygon[i], polygon[(i + 1) % n]);
        let (u, v) = (sub(b, a), sub(c, b));
        // (The library's own arc tangent: the same on every machine.)
        specula_core::dmath::atan2(cross(u, v), u[0] * v[0] + u[1] * v[1]).to_degrees()
    };
    let side = |i: usize| dist(polygon[i], polygon[(i + 1) % n]);
    let begin = (0..n).max_by(|a, b| side(*a).total_cmp(&side(*b)).then(b.cmp(a))).map_or(0, |i| (i + 1) % n);
    let mut corners: Vec<f32> = Vec::new();
    let mut open = 0.0f32;
    for k in 0..n {
        let i = (begin + k) % n;
        let here = turn(i);
        open += here;
        // The corner ends here unless the next one is close by and before the walk's end. Two
        // sharp corners stay two unless they are within the outline's own slack of each other
        // (a point a pixel or two wide): the end of a bar is two square corners, not a point.
        let two_sharp = here.abs() >= ROUND_BELOW_DEG && turn((i + 1) % n).abs() >= ROUND_BELOW_DEG && side(i) > 2.0 * slack;
        if k + 1 == n || side(i) >= MERGE * size || two_sharp {
            corners.push(open);
            open = 0.0;
        }
    }
    let mut shares = [0.0f32; 3];
    let mut sharp = 0.0f32;
    for c in &corners {
        let by = c.abs();
        let kind = if by < ROUND_BELOW_DEG {
            0
        } else if by < POINTED_FROM_DEG {
            1
        } else {
            2
        };
        shares[kind] += by;
        if kind > 0 {
            sharp += 1.0;
        }
    }
    let all = shares[0] + shares[1] + shares[2];
    if all <= 0.0 {
        return None;
    }
    let hull = hull_area(&points);
    Some(ShapeLanguage {
        round: shares[0] / all,
        square: shares[1] / all,
        pointed: shares[2] / all,
        convexity: if hull > 0.0 { (area(&points).abs() / hull).min(1.0) } else { 1.0 },
        corners: sharp,
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

    /// Known shapes, at a thumbnail's size and at four times it, upright and turned: a disc
    /// and an ellipse are round, a box and an L are square, a triangle and a star are pointed;
    /// a box whose corners are rounded by a twentieth of its side is still square and one
    /// rounded by a third is round.
    #[test]
    fn known_shapes_speak_their_language() {
        let disc = |x: f32, y: f32| (x - 0.5).hypot(y - 0.5) < 0.44;
        let ellipse = |x: f32, y: f32| ((x - 0.5) / 0.45).hypot((y - 0.5) / 0.25) < 1.0;
        let boxy: Vec<[f32; 2]> = vec![[0.15, 0.2], [0.85, 0.2], [0.85, 0.8], [0.15, 0.8]];
        // (Each within 0.47 of the middle, so that it stays in the mask however it is turned.)
        let ell: Vec<[f32; 2]> = vec![[0.18, 0.18], [0.46, 0.18], [0.46, 0.54], [0.82, 0.54], [0.82, 0.82], [0.18, 0.82]];
        let triangle: Vec<[f32; 2]> = vec![[0.5, 0.08], [0.86, 0.74], [0.14, 0.74]];
        let star: Vec<[f32; 2]> = (0..10)
            .map(|i| {
                let (a, r) = (std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2, if i % 2 == 0 { 0.46 } else { 0.18 });
                [0.5 + r * a.cos(), 0.5 + r * a.sin()]
            })
            .collect();
        // A box with rounded corners: within the box, and not in a corner's square beyond its arc.
        let rounded = |r: f32| move |x: f32, y: f32| {
            let (dx, dy) = ((x - 0.5).abs() - (0.38 - r), (y - 0.5).abs() - (0.38 - r));
            dx <= r && dy <= r && (dx <= 0.0 || dy <= 0.0 || dx.hypot(dy) <= r)
        };
        for side in [64u32, 256] {
            let of = |m: Mask| of_mask(&m).unwrap();
            let a = of(mask(side, disc));
            assert!(a.round > 0.95 && a.leading() == Kind::Round && a.convexity > 0.97 && a.corners == 0.0, "{side}: a disc: {a:?}");
            let a = of(mask(side, ellipse));
            assert!(a.round > 0.7 && a.leading() == Kind::Round, "{side}: an ellipse: {a:?}");
            for deg in [0.0f32, 17.0, 45.0] {
                let a = of(mask(side, |x, y| within(&turned(&boxy, deg), x, y)));
                assert!(a.square > 0.9 && a.leading() == Kind::Square && a.corners == 4.0 && a.convexity > 0.93, "{side}, turned {deg}: a box: {a:?}");
                let a = of(mask(side, |x, y| within(&turned(&ell, deg), x, y)));
                assert!(a.square > 0.9 && a.corners == 6.0 && a.convexity < 0.85, "{side}, turned {deg}: an L: {a:?}");
                let a = of(mask(side, |x, y| within(&turned(&triangle, deg), x, y)));
                assert!(a.pointed > 0.9 && a.leading() == Kind::Pointed && a.corners == 3.0, "{side}, turned {deg}: a triangle: {a:?}");
                let a = of(mask(side, |x, y| within(&turned(&star, deg), x, y)));
                assert!(a.pointed > 0.55 && a.leading() == Kind::Pointed && a.round < 0.1 && a.convexity < 0.6, "{side}, turned {deg}: a star: {a:?}");
            }
            // The cut end of a bar is square, however thin the bar (down to a tenth of the
            // size); a spike's tip is a point.
            let a = of(mask(side, |x, y| (0.08..0.92).contains(&x) && (0.44..0.56).contains(&y)));
            assert!(a.square > 0.9 && a.corners == 4.0, "{side}: a thin bar: {a:?}");
            let a = of(mask(side, |x, y| (0.1..0.9).contains(&x) && (y - 0.5).abs() < 0.12 * (0.9 - x) / 0.8));
            assert!(a.pointed > 0.4 && a.leading() != Kind::Round, "{side}: a spike: {a:?}");
            let a = of(mask(side, rounded(0.04)));
            assert!(a.square > 0.85, "{side}: a box rounded by a twentieth: {a:?}");
            let a = of(mask(side, rounded(0.25)));
            assert!(a.round > 0.85, "{side}: a box rounded by a third: {a:?}");
        }
    }

    /// What has no shape has no language, and the largest piece is the one read: an empty
    /// mask, a speck, a mask too large; a disc with a speck beside it is the disc; the mean of
    /// a disc and a box is half round and half square; the same mask gives the same numbers.
    #[test]
    fn what_is_measured_and_what_is_not() {
        assert_eq!(of_mask(&Mask::empty(64, 64)), None);
        assert_eq!(of_mask(&mask(64, |x, y| (x - 0.5).hypot(y - 0.5) < 0.03)), None);
        assert_eq!(of_mask(&Mask::empty(600, 600)), None);
        let disc = mask(64, |x, y| (x - 0.4).hypot(y - 0.5) < 0.3);
        let with_speck = mask(64, |x, y| (x - 0.4).hypot(y - 0.5) < 0.3 || (x > 0.9 && y > 0.9));
        assert_eq!(of_mask(&disc), of_mask(&with_speck));
        let boxy = mask(64, |x, y| (0.2..0.8).contains(&x) && (0.25..0.75).contains(&y));
        let both = of_masks(&[disc.clone(), boxy, Mask::empty(64, 64)]).unwrap();
        assert!((both.round - 0.5).abs() < 0.05 && (both.square - 0.5).abs() < 0.05 && both.corners == 2.0, "{both:?}");
        assert_eq!(of_masks(&[Mask::empty(8, 8)]), None);
        // (A disc with a hole is its outline: the hole is not traced.)
        let ring = mask(64, |x, y| (0.2..0.44).contains(&(x - 0.5).hypot(y - 0.5)));
        assert!(of_mask(&ring).unwrap().round > 0.95);
        let j = of_mask(&disc).unwrap().to_json();
        assert_eq!(j.get("leading").and_then(Json::as_str), Some("round"));
        assert_eq!(Kind::parse("pointed"), Some(Kind::Pointed));
        assert_eq!(Kind::parse("spiky"), None);
    }
}
