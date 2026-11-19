//! Shape language (docs/design/FORM.md): how round, square or pointed a silhouette is, as
//! numbers. Round reads as friendly, square as sturdy, pointed as dangerous: a convention of
//! character and prop design, not a law, so it is in every asset's taste measures and is
//! checked only when a style asks for one (`[taste] shape_language`).
//!
//! Measured on a silhouette mask, **each part at its own scale and weighed by its thickness**
//! (its fourth version: the three before judged every part by a window of the whole shape's
//! size and weighed turning alone, so that a string on a balloon counted for as much as the
//! balloon, and a corner cut off flat by more than the window was two gentle turns).
//!
//! 1. **How thick the piece is under each point of its outline**: the radius of the largest
//!    disc inside the silhouette that touches the outline there ([`distances`], [`spine`],
//!    [`thickness`]). In a corner or along a taper that falls to nothing at the tip, so the
//!    outline is followed from the point, both ways, to the body it is a corner of, and not
//!    across a **joint**, where the thickness leaps as no taper's does (a string meets its
//!    balloon; a stick, its snowman). That is the point's **part**, and its thickness is the
//!    scale everything about the point is taken at.
//! 2. **The outline's turning**: traced, its long straight runs straightened to within a
//!    pixel and a half and its curves smoothed, and the net turn taken within a window
//!    [`WINDOW`] of the part's thickness long. A box's window is three tenths of its side; a
//!    plank's, three tenths of its width.
//! 3. **A corner or a curve**: a corner is turning in one place ([`KINK`], [`KINK_DEG`]; where
//!    a straightened side meets another run, for certain: [`SIDES_DEG`]); a curve is turning
//!    spread out, and is round however far it turns in all.
//! 4. **A corner's angle** is its window's net turn less the window's lopsidedness (what the
//!    two sides of the window turn counts only as far as both do): the mouth of a slot in a
//!    disc is a right angle, the rim's own turn being on one side of it only.
//! 5. **Its class**: gentle up to [`ROUND_DEG`] (part of a curve: an octagon's corners),
//!    square, or from [`POINTED_DEG`] a point. A corner with no straight side near it (the
//!    end of an oval, the horn of a crescent) is never square: a curve, or from [`SOFT_DEG`] a
//!    point.
//! 6. **Its weight**: only outward turning counts (a notch, a slot, the inside corner of an L
//!    count for nothing); a step's turn is the turn within its own window, and the window is
//!    as long as the part is thick, so that **a part's turning counts by its thickness**: the
//!    end of a hair, which turns right round, counts for a hair. A point counts double
//!    ([`POINT_WEIGHT`]).
//!
//! So: a disc, an egg, a capsule, an oval up to three times as long as wide and a box rounded
//! by a quarter of its shorter side are round, and so is a balloon on a string, a snowman with
//! stick arms and a disc with a slot cut in it; a box is square, and so is one rounded by a
//! tenth of its side or with its corners cut off by a seventh of it, a bar however thin, a
//! table on thin legs and a comb; a triangle, a spike, a star, a crescent, a mace and an oval
//! eight times as long as wide are pointed.
//!
//! **What it cannot say steadily** (measured: the tests have the numbers):
//! - A corner rounded by between a tenth and a fifth of the box's shorter side is between
//!   square and round, by degrees; a corner cut off flat by between a seventh and a fifth is
//!   between square and eight-sided. The shares move by up to a fifth as the shape is turned.
//! - An oval four to six times as long as wide is on the line between round and pointed, and
//!   its lead goes by the angle it is turned to (its end is a dozen pixels round).
//! - **A body bristling with more than about a dozen hair-thin parts** (a disc with sixteen
//!   whiskers of one pixel) is read by its hairs or not at all: the inside corners at each
//!   hair's foot take the rim's turning out of every window that reaches them, and nothing
//!   of the rim is left. Two to eight hairs change nothing.
//! - A part under four pixels thick is not read ([`MIN_THICK`]); a part whose window would be
//!   under four steps is read coarsely and counts for little.
//! - Holes are not traced. It reads outlines, not surfaces.
//!
//! It needs pixels: the studio measures it on silhouettes of [`PIXELS`], and a piece under
//! [`MIN_SIZE`] pixels across is not read at all (a thumbnail never is).

use crate::image::Mask;
use specula_core::Json;

/// A corner's angle (the turn of its window, less the window's lopsidedness) under which it
/// is gentle, part of a curve as an octagon's corners are, and by which it is wholly a corner
/// (degrees). (A band this wide: a narrower one, 50 to 70, made a share swing from a fifth to
/// nine tenths by the angle the shape was turned to.)
pub const ROUND_DEG: (f32, f32) = (45.0, 80.0);
/// A corner's angle from which it begins to be a point, and by which it wholly is (degrees:
/// the corner's own angle is then 84 to 64 degrees or less).
pub const POINTED_DEG: (f32, f32) = (96.0, 116.0);
/// The window the net turn is taken in, as a share of **the part's own thickness** there (the
/// radius of the largest disc inside the silhouette that touches the outline at that point,
/// followed up a taper to its base: [`thickness`]). A box's window is three tenths of its
/// side; a plank's, three tenths of its width; a hair's, nothing to speak of.
pub const WINDOW: f32 = 0.6;
/// The lengths that take the pixels out of the outline ([`CHORD`], [`SMOOTH`],
/// [`STRAIGHT_RUN`]) are shares of the shape's size, or of this many times its greatest
/// thickness where that is less: a bar a fourteenth as thick as long is smoothed as a thing
/// of its own thickness, not of its length (smoothed by its length its square ends were
/// gone).
pub const SPAN: f32 = 2.5;
/// A corner is turning in one place; a curve is turning spread out. The window a kink is
/// looked for in, as a share of the part's thickness (under a third of [`WINDOW`]) ...
pub const KINK: f32 = 0.18;
/// ... and the net turn within it from which the outline begins to have a corner there, and
/// by which it wholly has (degrees). Under it the outline is a curve however far it turns in
/// the larger window: the end of an oval of three to one turns a hundred degrees within
/// [`WINDOW`] and thirty within this, and is a curve; a corner cut off flat turns forty-five
/// at each end of the cut, and is a corner, of the ninety the larger window holds.
pub const KINK_DEG: (f32, f32) = (20.0, 60.0);
/// Where a straightened side meets another, or a shorter straight run, the outline has a
/// corner for certain, whatever the small window measures of it through the smoothing: from
/// this turn between the two, and wholly by the second (degrees). (A corner cut off flat has two of forty-five; the chords a
/// large circle is straightened into meet at under sixteen.)
pub const SIDES_DEG: (f32, f32) = (30.0, 40.0);
/// A corner with no straight side near it (the end of an oval, the horn of a crescent) is not
/// square whatever its angle: it is part of a curve up to the first of these turns and a point
/// from the second (degrees), and between them a share of each.
pub const SOFT_DEG: (f32, f32) = (70.0, 110.0);
/// A corner has a straight side near it when this share of its window is straightened sides.
pub const SIDED: f32 = 0.2;
/// A thickness under this many pixels is not read: the shape is `None`.
pub const MIN_THICK: f32 = 4.0;
/// Following the outline from a point to find the thickness of the part it belongs to: a rise
/// of more than this many pixels within three steps, to more than twice what it was, is the
/// joint with another, thicker part (a spike's foot on a ball), and is not followed; a taper
/// rises by about three a step at the most. (And so is a rise by half, after the thickness
/// has been level for twice its own length: a string meets its balloon.)
pub const JOINT_PX: f32 = 15.0;
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
fn outline(m: &Mask) -> Option<(Vec<P>, Vec<bool>)> {
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
    let inside: Vec<bool> = piece.iter().map(|p| *p == best).collect();
    (points.len() >= 4).then_some((points, inside))
}

/// How far each pixel of the piece is from the nearest pixel that is not of it, centre to
/// centre (0 outside it; what lies beyond the picture is not of it). Exact: down the columns,
/// then the lower envelope of parabolas along the rows (Felzenszwalb and Huttenlocher).
fn distances(inside: &[bool], w: usize, h: usize) -> Vec<f32> {
    // Down each column: the distance to the nearest pixel outside, squared.
    let mut g = vec![0.0f32; w * h];
    for x in 0..w {
        let mut run = 0u32;
        for y in 0..h {
            run = if inside[y * w + x] { run + 1 } else { 0 };
            g[y * w + x] = run as f32;
        }
        run = 0;
        for y in (0..h).rev() {
            run = if inside[y * w + x] { run + 1 } else { 0 };
            let d = g[y * w + x].min(run as f32);
            g[y * w + x] = d * d;
        }
    }
    // Along each row, with a pixel outside the picture at either end.
    let n = w + 2;
    let (mut v, mut z, mut f) = (vec![0usize; n], vec![0.0f32; n + 1], vec![0.0f32; n]);
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        f[0] = 0.0;
        f[n - 1] = 0.0;
        f[1..=w].copy_from_slice(&g[y * w..(y + 1) * w]);
        let mut k = 0usize;
        v[0] = 0;
        z[0] = f32::NEG_INFINITY;
        z[1] = f32::INFINITY;
        for q in 1..n {
            loop {
                let p = v[k];
                let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
                if s <= z[k] && k > 0 {
                    k -= 1;
                } else {
                    k += 1;
                    v[k] = q;
                    z[k] = s;
                    z[k + 1] = f32::INFINITY;
                    break;
                }
            }
        }
        k = 0;
        for q in 1..=w {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let d = q as f32 - v[k] as f32;
            out[y * w + q - 1] = (d * d + f[v[k]]).sqrt();
        }
    }
    out
}

/// The middles of the discs that fill the piece and are held in no neighbour's (its spine:
/// the middle of a bar, the bisector of a corner, the middle of a ball), each with its
/// radius, the largest first. A pixel is one where the distance to the outside rises towards
/// no neighbour as fast as a step that way (it would, towards the middle of a larger disc
/// that held this one).
fn spine(d: &[f32], w: usize, h: usize) -> Vec<(f32, f32, f32)> {
    let mut out: Vec<(f32, f32, f32)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let here = d[y * w + x];
            if here < 2.0 {
                continue;
            }
            // (Sixteen ways: the eight neighbours and the eight a knight's move off. With eight,
            // the way up a slanted side's inside is up to 22 degrees off a neighbour's and
            // rises as slowly as the bisector of a 135 degree corner does: a corner cut off
            // flat then had no spine, and no thickness.)
            let mut held = false;
            for (dx, dy) in [(-1i32, -1i32), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1), (-1, -2), (1, -2), (-2, -1), (2, -1), (-2, 1), (2, 1), (-1, 2), (1, 2)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let step = ((dx * dx + dy * dy) as f32).sqrt();
                if d[ny as usize * w + nx as usize] - here >= 0.95 * step {
                    held = true;
                    break;
                }
            }
            if !held {
                // (A disc about a pixel's middle, short of the outside pixel's middle by half
                // a pixel.)
                out.push((x as f32 + 0.5, y as f32 + 0.5, here - 0.5));
            }
        }
    }
    out.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.1.total_cmp(&b.1)).then(a.0.total_cmp(&b.0)));
    out
}

/// How thick the piece is under each point of `at` (points of its outline): the radius of
/// the largest disc of the `spine` that reaches it, 0 where none does. The discs are tried
/// the largest first, each against the points near it that have none yet (the points are
/// kept by squares of the picture).
fn thickness(spine: &[(f32, f32, f32)], at: &[P], w: usize, h: usize) -> Vec<f32> {
    const CELL: usize = 8;
    let (cw, ch) = (w / CELL + 2, h / CELL + 2);
    let cell = |p: P| -> usize {
        let (x, y) = ((p[0].max(0.0) as usize / CELL).min(cw - 1), (p[1].max(0.0) as usize / CELL).min(ch - 1));
        y * cw + x
    };
    let mut cells: Vec<Vec<u32>> = vec![Vec::new(); cw * ch];
    for (i, p) in at.iter().enumerate() {
        cells[cell(*p)].push(i as u32);
    }
    let mut out = vec![0.0f32; at.len()];
    let mut left = at.len();
    for (x, y, r) in spine {
        if left == 0 {
            break;
        }
        let reach = r + 1.25;
        let (x0, x1) = (((x - reach).max(0.0) as usize / CELL).min(cw - 1), ((x + reach).max(0.0) as usize / CELL).min(cw - 1));
        let (y0, y1) = (((y - reach).max(0.0) as usize / CELL).min(ch - 1), ((y + reach).max(0.0) as usize / CELL).min(ch - 1));
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let list = &mut cells[cy * cw + cx];
                if list.is_empty() {
                    continue;
                }
                list.retain(|i| {
                    let p = at[*i as usize];
                    let (dx, dy) = (p[0] - x, p[1] - y);
                    if dx * dx + dy * dy <= reach * reach {
                        out[*i as usize] = *r;
                        left -= 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }
    }
    out
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
/// is turned. Every pair is tried: a hull of a 512-pixel disc has under a hundred and fifty
/// points. (One point in four was tried at first, and a sliver whose far tip fell between the
/// ones tried came out a tenth to a fifth short: third review.)
fn greatest_width(hull: &[P]) -> f32 {
    let mut most = 0.0f32;
    for (i, a) in hull.iter().enumerate() {
        for b in &hull[i + 1..] {
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
    let (points, inside) = outline(m)?;
    // The shape's size is its greatest width: the same whichever way it is turned. (The
    // longer side of the box round it grows by two fifths as a square is turned to stand on
    // a corner, and every length here is a share of the size: the same corner was a curve at
    // one angle and a corner at another. Its second review.)
    let hull = hull_of(&points);
    let size = greatest_width(&hull);
    if size < MIN_SIZE {
        return None;
    }
    // How thick the piece is: the discs that fill it. What takes the pixels out of the outline
    // below is sized by the shape, or by its greatest thickness where it is all thin.
    let spine = spine(&distances(&inside, m.w as usize, m.h as usize), m.w as usize, m.h as usize);
    let thickest = spine.first().map_or(0.0, |s| s.2);
    if thickest < MIN_THICK {
        return None;
    }
    let scale = size.min(SPAN * thickest);
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
    let long = (STRAIGHT_RUN * scale).max(8.0);
    let mut walked: Vec<P> = Vec::new();
    // How long the straightened side is that each walked point is of (0: of none), and at the
    // first point of a side that follows another, the turn between the two (degrees).
    let mut met: Vec<f32> = Vec::new();
    // (Each run: where its points begin, its length if it was straightened, and the turn
    // from the run before it if that was straightened too.)
    let mut runs: Vec<(usize, f32, f32)> = Vec::new();
    let run = |k: usize| (points[kept[k % kept.len()]], points[kept[(k + 1) % kept.len()] % points.len()]);
    for (k, i) in kept.iter().enumerate() {
        let j = kept[(k + 1) % kept.len()];
        let (a, b) = run(k);
        let before = walked.len();
        let straightened = dist(a, b) >= long;
        if straightened {
            let steps = (dist(a, b).round() as usize).max(1);
            walked.extend((0..steps).map(|k| [a[0] + (b[0] - a[0]) * k as f32 / steps as f32, a[1] + (b[1] - a[1]) * k as f32 / steps as f32]));
        } else {
            let end = if j > *i { j } else { points.len() };
            walked.extend_from_slice(&points[*i..end]);
        }
        met.resize(walked.len(), 0.0);
        let (c, d) = run(k + kept.len() - 1);
        // (The turn where this run meets the one before it, when either is a straightened
        // side and neither a speck: a corner cut off flat has a side on one hand and the cut,
        // which may be too short to be straightened itself, on the other; within an arc both
        // are short, and where an arc leaves a side the turn is half a chord's.)
        let mut turned = f32::INFINITY;
        if walked.len() > before && (straightened || dist(c, d) >= long) && dist(a, b) >= 6.0 && dist(c, d) >= 6.0 {
            let (u, v) = (sub(d, c), sub(b, a));
            turned = specula_core::dmath::atan2(cross(u, v), u[0] * v[0] + u[1] * v[1]).abs().to_degrees();
            met[before] = turned;
        }
        runs.push((before, if straightened { dist(a, b) } else { 0.0 }, turned));
    }
    // A side is as long as the straightened runs that lie along one line are together: the
    // straightening cuts a slanted side in two where its pixels' steps stray, and neither
    // piece alone need be as long as a corner's window. (Runs that meet at under three degrees
    // are of one side: the pieces of a cut side meet at under two and a half, the chords a
    // long oval's flank is straightened into at five and more.)
    let joined = |k: usize| runs.len() > 1 && runs[k].1 > 0.0 && runs[(k + runs.len() - 1) % runs.len()].1 > 0.0 && runs[k].2 < 3.0;
    // (From a run that begins a side, on through the runs joined to it; all joined round a
    // closed outline cannot be, since it turns once round.)
    let mut side_of = vec![0.0f32; runs.len()];
    for first in (0..runs.len()).filter(|k| !joined(*k)) {
        let mut last = first;
        let mut length = runs[first].1;
        while joined((last + 1) % runs.len()) && (last + 1) % runs.len() != first {
            last = (last + 1) % runs.len();
            length += runs[last].1;
        }
        let mut k = first;
        loop {
            side_of[k] = length;
            if k == last {
                break;
            }
            k = (k + 1) % runs.len();
        }
    }
    let mut sided = vec![0.0f32; walked.len()];
    for (k, r) in runs.iter().enumerate() {
        let end = runs.get(k + 1).map_or(walked.len(), |n| n.0);
        sided[r.0..end].fill(side_of[k]);
    }
    let outline_points = points;
    let points = walked;
    // Then smoothed: each point the weighted mean of its neighbours along it, by a Gaussian
    // of a deviation of `SMOOTH` of the size (the outline is closed: round it goes), so that
    // the polygon's own corners on a curve run together into the curve.
    let n = points.len();
    let sigma = (SMOOTH * scale).max(2.0);
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
    let chord = ((CHORD * scale).round() as usize).max(1);
    if count < 8 * chord.max(2) {
        return None;
    }
    let step = length / count as f32;
    let mut even: Vec<P> = Vec::with_capacity(count);
    // (And which point of the smoothed outline each step lies after.)
    let mut from: Vec<usize> = Vec::with_capacity(count);
    let (mut i, mut walked, mut want) = (0usize, 0.0f32, 0.0f32);
    while even.len() < count {
        let (a, b) = (smooth[i % n], smooth[(i + 1) % n]);
        let side = dist(a, b);
        if side > 0.0 && walked + side >= want {
            let t = (want - walked) / side;
            even.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            from.push(i % n);
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
    if count < 8 * chord.max(2) {
        return None;
    }
    // How thick the part is that each step of the outline belongs to. Under a step the piece
    // is as thick as the largest disc that touches it there; in a corner or along a taper
    // that falls to nothing at the tip, so the step is followed along the outline, both ways,
    // while the thickness rises as a taper's does, to the body the corner is a corner of (a
    // box's corner: the box; a spike's tip: the spike's base). A sudden rise is the joint with
    // another part and is not followed: a string's end belongs to the string.
    let raw = thickness(&spine, &even, m.w as usize, m.h as usize);
    let thick: Vec<f32> = (0..count)
        .map(|i| {
            let mut five = [raw[(i + count - 2) % count], raw[(i + count - 1) % count], raw[i], raw[(i + 1) % count], raw[(i + 2) % count]];
            five.sort_by(f32::total_cmp);
            five[2]
        })
        .collect();
    // A joint, going on from step `at` by `way` (one step on, or one back): the thickness
    // rises as no taper's does. Either it more than doubles, by more than [`JOINT_PX`], within
    // three steps (a taper gains about three pixels a step at the most); or it has been level
    // for twice its own length and then rises by half (a part of one width, a string or a
    // stick, meeting a thicker one: a taper is never level before it rises).
    let rises = |at: usize, way: usize| -> bool {
        let here = thick[at];
        let ahead = (1..=3).map(|k| thick[(at + k * way) % count]).fold(0.0f32, f32::max);
        if ahead - here > JOINT_PX && ahead > 2.0 * here {
            return true;
        }
        // (Looked for a little further on: where a stick meets a ball the thickness creeps
        // up for a few steps before it leaps, the smoothed outline cutting the inside corner.)
        let further = (1..=8).map(|k| thick[(at + k * way) % count]).fold(0.0f32, f32::max);
        if further < 2.0 * here + 4.0 || thick[(at + way) % count] <= here + 0.5 {
            return false;
        }
        let level = ((2.0 * here).round() as usize).clamp(6, count / 4);
        let back = count - way;
        (1..=level).all(|k| (thick[(at + k * back) % count] - here).abs() <= 1.0 + 0.08 * here)
    };
    // (One pass each way, twice round: the most thickness from a step on to the next joint.)
    let up: Vec<bool> = (0..count).map(|i| rises(i, 1)).collect();
    let down: Vec<bool> = (0..count).map(|i| rises(i, count - 1)).collect();
    let mut onward = thick.clone();
    let mut back = thick.clone();
    for _ in 0..2 {
        for i in (0..count).rev() {
            if !up[i] {
                onward[i] = onward[i].max(onward[(i + 1) % count]);
            }
        }
        for i in 0..count {
            if !down[i] {
                back[i] = back[i].max(back[(i + count - 1) % count]);
            }
        }
    }
    let host: Vec<f32> = (0..count).map(|i| onward[i].max(back[i])).collect();
    let half: Vec<usize> = host.iter().map(|t| ((0.5 * WINDOW * t).round() as usize).clamp(2, (count / 8).max(2))).collect();
    let blur = (2.0 * sigma).ceil() as usize + chord;
    let small: Vec<usize> = host.iter().zip(&half).map(|(t, h)| ((0.5 * KINK * t).round() as usize).clamp(2, *h)).collect();
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
    // (The turn so far, three times round, so that a window is a difference of two sums. In f64:
    // thousands of small turns are summed.)
    // Which way round the outline goes: a closed outline turns once in all, one way or the
    // other.
    let way = if turn.iter().sum::<f32>() >= 0.0 { 1.0f32 } else { -1.0 };
    let mut sum = vec![0.0f64; 3 * count + 1];
    for j in 0..3 * count {
        sum[j + 1] = sum[j] + f64::from(turn[j % count]);
    }
    let net: Vec<f32> = (0..count).map(|i| (sum[i + count + half[i]] - sum[i + count - half[i]]) as f32).collect();
    let fine: Vec<f32> = (0..count).map(|i| (sum[i + count + small[i]] - sum[i + count - small[i]]) as f32).collect();
    // Only where the outline turns its own way round (outward: its bulges and its corners)
    // does it count; where it turns back on itself (a notch, a slot, the inside corner of an
    // L) it does not.
    let out: Vec<f32> = net.iter().map(|n| (n * way).max(0.0).to_degrees()).collect();
    // How much of a corner the outline has at each step: its turn within the small window.
    // (Either way: an inside corner is a corner too, and takes its turn back out of the window
    // it shares with an outside one, as it does out of the window's net turn.)
    let mut kink: Vec<f32> = fine.iter().map(|n| smoothstep(KINK_DEG.0, KINK_DEG.1, n.abs().to_degrees())).collect();
    // (Where two straightened sides meet, a corner for certain: found at the step that lies
    // after the meeting point, and as far about it as the smoothing spreads it.)
    for (e, src) in from.iter().enumerate() {
        let sure = smoothstep(SIDES_DEG.0, SIDES_DEG.1, met[*src]);
        if sure > 0.0 && (e == 0 || from[e - 1] != *src) {
            for k in 0..=2 * blur {
                let j = (e + count - blur + k) % count;
                kink[j] = kink[j].max(sure);
            }
        }
    }

    // The corner's angle at each step: **the window's net turn, less its lopsidedness**. The
    // turn within the window is split into the middle (as wide as the small window, or as the
    // smoothing spreads a kink if that is more) and the two sides; what the sides turn counts
    // only as far as both do. So:
    // - a box's corner is the ninety degrees of its middle, and so is a corner rounded by less
    //   than the window, whose arc is in the middle and evenly to both sides;
    // - two kinks in one window (a corner cut off flat) are one corner of their sum;
    // - the mouth of a slot in a disc is a right angle: the rim it sits on turns on one side
    //   only (taken with the window's net turn it read as a point);
    // - the end of a long oval is the whole of what its window turns, and comes to a point.
    // Taken as the mean over five windows from three quarters as long to a third as long
    // again: two kinks that a window's end falls between are neither wholly one corner nor
    // wholly two, and a box whose corners are cut off goes from square to eight-sided by
    // degrees as the cut grows.
    let cornered: Vec<f32> = (0..count)
        .map(|i| {
            let core = small[i].max(blur);
            let at = i + count;
            let net = |from: usize, to: usize| ((sum[to] - sum[from]) as f32) * way;
            let mean = [15usize, 20, 20, 24, 27]
                .iter()
                .map(|k| {
                    let reach = (half[i] * k / 20).max(core);
                    let (before, after) = (net(at - reach, at - core), net(at + core, at + reach));
                    net(at - reach, at + reach) - (before - after).abs()
                })
                .sum::<f32>()
                / 5.0;
            mean.to_degrees().clamp(0.0, 180.0)
        })
        .collect();
    let mut shares = [0.0f32; 3];
    let mut corners = 0u32;
    for (i, by) in out.iter().copied().enumerate() {
        // Classed by the most the outline turns outward in any window near this step: a step
        // beside a corner, whose own window holds only part of the corner's turn, is part of
        // that corner and not of a gentle curve.
        let (mut most, mut corner) = (0.0f32, 0.0f32);
        // (As far as the window reaches, and as far again as the smoothing spreads a
        // corner's turn: on a thin part that is as far as the window itself, and the steps
        // at the edge of a corner's turn were read as a curve.)
        let reach = half[i] + blur;
        for k in 0..=2 * reach {
            let j = (i + 2 * count - reach + k) % count;
            most = most.max(cornered[j]);
            corner = corner.max(kink[j]);
        }
        // A curve where nothing near is a corner; where something is, the corner's own class
        // by how far it turns: gently (an octagon's), squarely, or to a point.
        // And a corner between two curves, with no straight side near, is never square: a
        // curve still, or a point.
        // (A side: a straightened run longer than the window, wholly one at twice as long. The
        // flank of a long oval is straightened into chords near its end, each shorter than
        // that.)
        let window = (2 * half[i]) as f32;
        let sides = (0..=2 * reach).map(|k| smoothstep(window, 2.0 * window, sided[from[(i + 2 * count - reach + k) % count]])).sum::<f32>() / (2 * reach + 1) as f32;
        let has_side = (sides / SIDED).min(1.0);
        let soft = smoothstep(SOFT_DEG.0, SOFT_DEG.1, most);
        let round = 1.0 - corner * (has_side * smoothstep(ROUND_DEG.0, ROUND_DEG.1, most) + (1.0 - has_side) * soft);
        let pointed = corner * (has_side * smoothstep(POINTED_DEG.0, POINTED_DEG.1, most) + (1.0 - has_side) * soft);
        // (A step's turn is the turn within its own window, and its window is as long as its
        // part is thick: summed over the steps, a part's turning counts by its thickness.
        // That is the weight: a hair's end, which turns right round, counts for a hair.)
        // (A window cannot be under four steps; where the part is thinner than that allows, its
        // turn counts for the window it should have had.)
        let by = by * (0.5 * WINDOW * host[i] / half[i] as f32).min(1.0);
        shares[0] += by * round;
        shares[1] += by * (1.0 - round - pointed).max(0.0);
        // (A point draws the eye more than a right angle does: it counts for more.)
        shares[2] += by * pointed * POINT_WEIGHT;
        // (A corner is counted where the outline comes to turn that much: once for each run
        // of steps that do.)
        let is = |j: usize| kink[j] >= 0.5 && cornered[j] >= CORNER_DEG;
        if is(i) && !is((i + count - 1) % count) {
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

    /// (For whoever tunes the measure: every known shape's shares, least and most over a
    /// quarter turn, written to `specula-shape-table.txt` in the temporary folder.
    /// `cargo test --release -p specula-studio --lib shape::tests::table -- --ignored`.)
    #[test]
    #[ignore = "writes a table for tuning; asserts only that it wrote it"]
    fn table() {
        use std::fmt::Write as _;
        let mut said = String::new();
        let shapes: Vec<(&str, Inside)> = vec![
            ("disc", |x, y| x.hypot(y) < 0.44),
            ("ellipse 2:1", |x, y| (x / 0.45).hypot(y / 0.225) < 1.0),
            ("ellipse 3:1", |x, y| (x / 0.45).hypot(y / 0.15) < 1.0),
            ("ellipse 5:1", |x, y| (x / 0.45).hypot(y / 0.09) < 1.0),
            ("hexagon", |x, y| regular(x, y, 6, 0.45, None)),
            ("octagon", |x, y| regular(x, y, 8, 0.45, None)),
            ("decagon", |x, y| regular(x, y, 10, 0.45, None)),
            ("12-gon", |x, y| regular(x, y, 12, 0.45, None)),
            ("16-gon", |x, y| regular(x, y, 16, 0.45, None)),
            ("capsule 4:1", |x, y| rounded(x, y, 0.45, 0.1125, 0.1125)),
            ("capsule 6:1", |x, y| rounded(x, y, 0.45, 0.075, 0.075)),
            ("box r.02", |x, y| rounded(x, y, 0.4, 0.3, 0.02)),
            ("box r.04", |x, y| rounded(x, y, 0.4, 0.3, 0.04)),
            ("box r.06", |x, y| rounded(x, y, 0.4, 0.3, 0.06)),
            ("box r.08", |x, y| rounded(x, y, 0.4, 0.3, 0.08)),
            ("box r.10", |x, y| rounded(x, y, 0.4, 0.3, 0.10)),
            ("box r.14", |x, y| rounded(x, y, 0.4, 0.3, 0.14)),
            ("box r.20", |x, y| rounded(x, y, 0.4, 0.3, 0.20)),
            ("box cut .03", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.03),
            ("box cut .05", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.05),
            ("box cut .07", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.07),
            ("box cut .09", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.09),
            ("box cut .13", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.13),
            ("box cut .18", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.18),
            ("slot disc", |x, y| x.hypot(y) < 0.44 && !(x.abs() < 0.01 && y > 0.34)),
            ("notch disc", |x, y| x.hypot(y) < 0.44 && !(y > 0.24 && x.abs() < (y - 0.24) * 0.5)),
            ("cloud", |x, y| x.hypot(y) < 0.2 || (x - 0.22).hypot(y - 0.05) < 0.16 || (x + 0.22).hypot(y - 0.05) < 0.16 || (x - 0.1).hypot(y + 0.14) < 0.15 || (x + 0.1).hypot(y + 0.14) < 0.15),
            ("box", |x, y| x.abs() < 0.35 && y.abs() < 0.3),
            ("L", |x, y| within(&[[-0.32, -0.32], [-0.04, -0.32], [-0.04, 0.04], [0.32, 0.04], [0.32, 0.32], [-0.32, 0.32]], x, y)),
            ("cross", |x, y| (x.abs() < 0.44 && y.abs() < 0.12) || (x.abs() < 0.12 && y.abs() < 0.44)),
            ("bar 5:1", |x, y| x.abs() < 0.45 && y.abs() < 0.09),
            ("bar 8:1", |x, y| x.abs() < 0.45 && y.abs() < 0.056),
            ("bar 14:1", |x, y| x.abs() < 0.45 && y.abs() < 0.032),
            ("house 90", |x, y| within(&[[-0.3, 0.4], [-0.3, -0.1], [0.0, -0.4], [0.3, -0.1], [0.3, 0.4]], x, y)),
            ("pentagon", |x, y| regular(x, y, 5, 0.45, None)),
            ("triangle", |x, y| within(&[[0.0, -0.42], [0.36, 0.24], [-0.36, 0.24]], x, y)),
            ("star 5", |x, y| regular(x, y, 5, 0.46, Some(0.18))),
            ("star 12", |x, y| regular(x, y, 12, 0.46, Some(0.30))),
            ("spike 8", |x, y| within(&[[0.0, -0.44], [0.0615, 0.44], [-0.0615, 0.44]], x, y)),
            ("spike 25", |x, y| within(&[[0.0, -0.44], [0.195, 0.44], [-0.195, 0.44]], x, y)),
            ("thin diamond", |x, y| x.abs() / 0.11 + y.abs() / 0.44 < 1.0),
            ("crescent", |x, y| x.hypot(y) < 0.44 && (x - 0.18).hypot(y) > 0.36),
            ("box+spike", |x, y| (x.abs() < 0.3 && y.abs() < 0.2) || within(&[[-0.05, -0.2], [0.05, -0.2], [0.0, -0.44]], x, y)),
            ("balloon, string", |x, y| x.hypot(y + 0.15) < 0.28 || (x.abs() < 0.004 && (0.1..0.45).contains(&y))),
            ("cherry, stem", |x, y| x.hypot(y - 0.12) < 0.3 || (x.abs() < 0.012 && (-0.42..-0.1).contains(&y))),
            ("lollipop .05", |x, y| x.hypot(y + 0.18) < 0.25 || (x.abs() < 0.025 && (0.0..0.45).contains(&y))),
            ("bomb, fuse", |x, y| x.hypot(y - 0.08) < 0.33 || (x.abs() < 0.02 && (-0.42..-0.2).contains(&y))),
            ("snowman, arms", |x, y| x.hypot(y - 0.2) < 0.24 || x.hypot(y + 0.16) < 0.17 || (y.abs() < 0.008 && x.abs() < 0.45)),
            ("disc, 4 whiskers", |x, y| x.hypot(y) < 0.3 || ((x.abs() < 0.004 || y.abs() < 0.004) && x.hypot(y) < 0.45)),
            ("mace", |x, y| {
                x.hypot(y) < 0.26 || (0..8).any(|k| {
                    let a = std::f32::consts::TAU * k as f32 / 8.0;
                    let (c, s) = (a.cos(), a.sin());
                    let (u, v) = (x * c + y * s, -x * s + y * c);
                    (0.2..0.45).contains(&u) && v.abs() < 0.05 * (0.45 - u) / 0.25
                })
            }),
            ("thick C", |x, y| (0.27..0.44).contains(&x.hypot(y)) && !(x > 0.0 && y.abs() < 0.1)),
            ("table", |x, y| (y < -0.13 && y > -0.21 && x.abs() < 0.33) || ((x.abs() - 0.28).abs() < 0.024 && (-0.21..0.3).contains(&y))),
            ("comb", |x, y| x.abs() < 0.4 && ((0.0..0.12).contains(&y) || ((-0.12..0.0).contains(&y) && (((x + 0.4) / 0.04) as u32).is_multiple_of(2)))),
            ("egg", |x, y| if y < 0.0 { (x / 0.3).hypot(y / 0.42) < 1.0 } else { (x / 0.3).hypot(y / 0.28) < 1.0 }),
            ("teardrop", |x, y| x.hypot(y - 0.15) < 0.25 || within(&[[0.0, -0.44], [0.212, 0.018], [-0.212, 0.018]], x, y)),
            ("heart", |x, y| (x - 0.16).hypot(y + 0.12) < 0.2 || (x + 0.16).hypot(y + 0.12) < 0.2 || within(&[[-0.33, -0.02], [0.33, -0.02], [0.0, 0.4]], x, y)),
        ];
        for (name, inside) in shapes {
            let started = std::time::Instant::now();
            let (lo, hi, lead, corners) = turned(inside, 7.5);
            let ms = started.elapsed().as_secs_f32() * 1000.0 / lead.len() as f32;
            let leads: Vec<&str> = [Kind::Round, Kind::Square, Kind::Pointed].iter().filter(|k| lead.contains(k)).map(|k| k.name()).collect();
            writeln!(
                said,
                "{name:18} round {:.2}-{:.2}  square {:.2}-{:.2}  pointed {:.2}-{:.2}  leads {:<14} corners {:.0}-{:.0}  {ms:.1} ms",
                lo[0],
                hi[0],
                lo[1],
                hi[1],
                lo[2],
                hi[2],
                leads.join("/"),
                corners.iter().copied().fold(f32::INFINITY, f32::min),
                corners.iter().copied().fold(0.0f32, f32::max)
            )
            .unwrap();
        }
        let path = std::env::temp_dir().join("specula-shape-table.txt");
        std::fs::write(&path, &said).unwrap();
        assert!(said.lines().count() > 50, "{}", path.display());
    }

    /// Known shapes on a silhouette of the studio's size, **however they are turned** (its
    /// first review: upright, a box rounded by an eighth of its size was round and turned six
    /// degrees it was square; its second: a spike read under half pointed, a disc with one
    /// slot cut in it failed "round", an ellipse of three to one moved by a third). For each:
    /// what leads at every angle, the least its share is, and how far the share moves.
    #[test]
    fn known_shapes_speak_their_language_however_they_are_turned() {
        fn mace(x: f32, y: f32) -> bool {
            x.hypot(y) < 0.26
                || (0..8).any(|k| {
                    let a = std::f32::consts::TAU * k as f32 / 8.0;
                    let (c, s) = (a.cos(), a.sin());
                    let (u, v) = (x * c + y * s, -x * s + y * c);
                    (0.2..0.45).contains(&u) && v.abs() < 0.05 * (0.45 - u) / 0.25
                })
        }
        let shapes: [(&str, Inside, Kind, f32); 42] = [
            ("a disc", |x, y| x.hypot(y) < 0.44, Kind::Round, 0.97),
            ("an ellipse of 3 to 1", |x, y| (x / 0.45).hypot(y / 0.15) < 1.0, Kind::Round, 0.7),
            ("an octagon", |x, y| regular(x, y, 8, 0.45, None), Kind::Round, 0.9),
            ("a capsule of 4 to 1", |x, y| rounded(x, y, 0.45, 0.1125, 0.1125), Kind::Round, 0.9),
            ("a capsule of 6 to 1", |x, y| rounded(x, y, 0.45, 0.075, 0.075), Kind::Round, 0.85),
            // (A box of 0.8 by 0.6: its rounding is given as a length, its shorter side being
            // 0.6. Rounded by a quarter of that side and more it is round; its fourth review's
            // measure judges a corner at the scale of the part it is a corner of.)
            ("a box rounded by a quarter of its shorter side", |x, y| rounded(x, y, 0.4, 0.3, 0.14), Kind::Round, 0.9),
            ("a box rounded by a third of its shorter side", |x, y| rounded(x, y, 0.4, 0.3, 0.20), Kind::Round, 0.95),
            ("an egg", |x, y| if y < 0.0 { (x / 0.3).hypot(y / 0.42) < 1.0 } else { (x / 0.3).hypot(y / 0.28) < 1.0 }, Kind::Round, 0.95),
            // One thin part does not unmake a round body (its third review: a balloon on a
            // string read as pointed, a point counting double and a part's size for nothing).
            ("a balloon on a string", |x, y| x.hypot(y + 0.15) < 0.28 || (x.abs() < 0.004 && (0.1..0.45).contains(&y)), Kind::Round, 0.9),
            ("a cherry with its stem", |x, y| x.hypot(y - 0.12) < 0.3 || (x.abs() < 0.012 && (-0.42..-0.1).contains(&y)), Kind::Round, 0.9),
            ("a lollipop", |x, y| x.hypot(y + 0.18) < 0.25 || (x.abs() < 0.025 && (0.0..0.45).contains(&y)), Kind::Round, 0.88),
            ("a bomb with its fuse", |x, y| x.hypot(y - 0.08) < 0.33 || (x.abs() < 0.02 && (-0.42..-0.2).contains(&y)), Kind::Round, 0.9),
            ("a snowman with stick arms", |x, y| x.hypot(y - 0.2) < 0.24 || x.hypot(y + 0.16) < 0.17 || (y.abs() < 0.008 && x.abs() < 0.45), Kind::Round, 0.88),
            ("a disc with four whiskers", |x, y| x.hypot(y) < 0.3 || ((x.abs() < 0.004 || y.abs() < 0.004) && x.hypot(y) < 0.45), Kind::Round, 0.85),
            // (A corner cut off flat by less is still a corner: the test after this one.)
            ("a box with its corners cut by a quarter of its shorter side", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.15, Kind::Round, 0.9),
            ("a disc with a slot cut in it", |x, y| x.hypot(y) < 0.44 && !(x.abs() < 0.01 && y > 0.34), Kind::Round, 0.55),
            ("a disc with a notch cut in it", |x, y| x.hypot(y) < 0.44 && !(y > 0.24 && x.abs() < (y - 0.24) * 0.5), Kind::Round, 0.55),
            ("a cloud of five bumps", |x, y| x.hypot(y) < 0.2 || (x - 0.22).hypot(y - 0.05) < 0.16 || (x + 0.22).hypot(y - 0.05) < 0.16 || (x - 0.1).hypot(y + 0.14) < 0.15 || (x + 0.1).hypot(y + 0.14) < 0.15, Kind::Round, 0.9),
            ("a box", |x, y| x.abs() < 0.35 && y.abs() < 0.3, Kind::Square, 0.95),
            ("an L", |x, y| within(&[[-0.32, -0.32], [-0.04, -0.32], [-0.04, 0.04], [0.32, 0.04], [0.32, 0.32], [-0.32, 0.32]], x, y), Kind::Square, 0.95),
            ("a cross", |x, y| (x.abs() < 0.44 && y.abs() < 0.12) || (x.abs() < 0.12 && y.abs() < 0.44), Kind::Square, 0.95),
            ("a bar a fifth as thick as long", |x, y| x.abs() < 0.45 && y.abs() < 0.09, Kind::Square, 0.95),
            ("a bar an eighth as thick as long", |x, y| x.abs() < 0.45 && y.abs() < 0.056, Kind::Square, 0.95),
            // (Its third review: bars thinner than a fourteenth, and a table's legs, read as
            // pointed, their two square corners in one window. A part is judged at its own
            // thickness now.)
            ("a bar a fourteenth as thick as long", |x, y| x.abs() < 0.45 && y.abs() < 0.032, Kind::Square, 0.9),
            ("a table on thin legs", |x, y| (y < -0.13 && y > -0.21 && x.abs() < 0.33) || ((x.abs() - 0.28).abs() < 0.024 && (-0.21..0.3).contains(&y)), Kind::Square, 0.9),
            ("a comb", |x, y| x.abs() < 0.4 && ((0.0..0.12).contains(&y) || ((-0.12..0.0).contains(&y) && (((x + 0.4) / 0.04) as u32).is_multiple_of(2))), Kind::Square, 0.9),
            ("a box rounded by a tenth of its shorter side", |x, y| rounded(x, y, 0.4, 0.3, 0.06), Kind::Square, 0.85),
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
            // (Eight spikes on a ball: each as wide at its foot as a sixth of the ball. A spike
            // counts by the thickness of its foot, where a string counts by its own.)
            ("a mace", mace, Kind::Pointed, 0.95),
            ("an oval eight times as long as wide", |x, y| (x / 0.45).hypot(y * 8.0 / 0.45) < 1.0, Kind::Pointed, 0.75),
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
    /// a fiftieth to a fifth of the size, at every three degrees: the share moves by a
    /// fifth at most, and the more rounded box is the rounder at its middle. (Since its
    /// fourth version a corner is judged at the scale of the box, three tenths of its shorter
    /// side: the line between square and round lies between a rounding of a tenth and of a
    /// fifth of that side, where it lay at about a sixteenth of the whole size.)
    #[test]
    fn a_corners_rounding_moves_the_answer_smoothly() {
        let boxes: [(f32, Inside); 11] = [
            (0.02, |x, y| rounded(x, y, 0.4, 0.3, 0.02)),
            (0.04, |x, y| rounded(x, y, 0.4, 0.3, 0.04)),
            (0.05, |x, y| rounded(x, y, 0.4, 0.3, 0.05)),
            (0.06, |x, y| rounded(x, y, 0.4, 0.3, 0.06)),
            (0.07, |x, y| rounded(x, y, 0.4, 0.3, 0.07)),
            (0.08, |x, y| rounded(x, y, 0.4, 0.3, 0.08)),
            (0.09, |x, y| rounded(x, y, 0.4, 0.3, 0.09)),
            (0.10, |x, y| rounded(x, y, 0.4, 0.3, 0.10)),
            (0.12, |x, y| rounded(x, y, 0.4, 0.3, 0.12)),
            (0.14, |x, y| rounded(x, y, 0.4, 0.3, 0.14)),
            (0.20, |x, y| rounded(x, y, 0.4, 0.3, 0.20)),
        ];
        let mut last = -1.0f32;
        for (r, inside) in boxes {
            let (lo, hi, _, _) = turned(inside, 3.0);
            assert!(hi[0] - lo[0] <= 0.2, "a box rounded by {r} of its size: round from {:.2} to {:.2} as it is turned", lo[0], hi[0]);
            assert!(hi[2] < 0.02, "a box rounded by {r}: nothing of it is a point ({:.2})", hi[2]);
            let middle = (lo[0] + hi[0]) / 2.0;
            assert!(middle >= last - 0.02, "a box rounded by {r} is no less round than one rounded by less: {middle:.2} after {last:.2}");
            last = middle;
        }
        assert!(last > 0.95, "rounded by a fifth, it is round: {last:.2}");
    }

    /// **A cut corner goes from square to eight-sided by degrees** (its third review: a box
    /// whose corners were cut off flat by a fifteenth of its size or more read as wholly
    /// round, each end of the cut a turn of forty-five degrees and gentle by the rule). At
    /// every cut from a twentieth to a third of the shorter side, at every three degrees: the
    /// share moves by a fifth at most, a larger cut is no squarer, a cut of a seventh of the
    /// side is still square and one of a quarter is not.
    #[test]
    fn a_corner_cut_off_flat_is_a_corner_until_the_cut_is_a_side() {
        let boxes: [(f32, Inside); 8] = [
            (0.03, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.03),
            (0.05, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.05),
            (0.07, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.07),
            (0.09, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.09),
            (0.11, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.11),
            (0.13, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.13),
            (0.15, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.15),
            (0.18, |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.18),
        ];
        let mut last = 2.0f32;
        for (cut, inside) in boxes {
            let (lo, hi, _, _) = turned(inside, 3.0);
            assert!(hi[1] - lo[1] <= 0.2, "a box cut by {cut}: square from {:.2} to {:.2} as it is turned", lo[1], hi[1]);
            assert!(hi[2] < 0.02, "a box cut by {cut}: nothing of it is a point ({:.2})", hi[2]);
            let middle = (lo[1] + hi[1]) / 2.0;
            // (To a twentieth: a cut too short to be straightened is found a little less surely
            // than a longer one.)
            assert!(middle <= last + 0.05, "a box cut by {cut} is no squarer than one cut by less: {middle:.2} after {last:.2}");
            if cut <= 0.09 {
                assert!(lo[1] > 0.75, "a box cut by {cut} is square: {:.2}", lo[1]);
            }
            last = middle;
        }
        assert!(last < 0.05, "cut by a third of its shorter side, it is eight-sided, and round: square {last:.2}");
    }

    /// **An oval is never square.** As it grows longer its ends tighten: it is round to
    /// three times as long as wide, comes to points by eight times, and between is a share of
    /// each (about four to six times as long as wide it is on the line, and its lead goes
    /// by the angle it is turned to: its end is a dozen pixels round at the size the studio
    /// measures on). At no length and no angle is more than a fifth of it square: a corner
    /// with no straight side near it is part of a curve or a point, whatever its angle.
    #[test]
    fn an_oval_goes_from_round_to_pointed_and_is_never_square() {
        let ovals: [(f32, Inside); 8] = [
            (2.0, |x, y| (x / 0.45).hypot(y * 2.0 / 0.45) < 1.0),
            (3.0, |x, y| (x / 0.45).hypot(y * 3.0 / 0.45) < 1.0),
            (3.5, |x, y| (x / 0.45).hypot(y * 3.5 / 0.45) < 1.0),
            (4.0, |x, y| (x / 0.45).hypot(y * 4.0 / 0.45) < 1.0),
            (5.0, |x, y| (x / 0.45).hypot(y * 5.0 / 0.45) < 1.0),
            (6.0, |x, y| (x / 0.45).hypot(y * 6.0 / 0.45) < 1.0),
            (8.0, |x, y| (x / 0.45).hypot(y * 8.0 / 0.45) < 1.0),
            (12.0, |x, y| (x / 0.45).hypot(y * 12.0 / 0.45) < 1.0),
        ];
        for (long, inside) in ovals {
            let (lo, hi, lead, _) = turned(inside, 3.0);
            assert!(hi[1] < 0.2, "an oval of {long} to 1 is up to {:.2} square", hi[1]);
            if long <= 3.0 {
                assert!(lo[0] > 0.85 && lead.iter().all(|l| *l == Kind::Round), "an oval of {long} to 1 is round: {:.2} at the least", lo[0]);
            }
            if long >= 8.0 {
                assert!(lo[2] > 0.75 && lead.iter().all(|l| *l == Kind::Pointed), "an oval of {long} to 1 comes to points: {:.2} at the least", lo[2]);
            }
        }
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
        // A part is judged at its own thickness: a comb's flat teeth, a twentieth of the size
        // across, are square, as they are to the eye. (Judged by a window of the whole shape's
        // size, each tooth's two corners fell in one window and the comb read as points.)
        let comb: Inside = |x, y| x.abs() < 0.4 && ((0.0..0.12).contains(&y) || ((-0.12..0.0).contains(&y) && (((x + 0.4) / 0.04) as u32).is_multiple_of(2)));
        let a = of_mask(&mask(PIXELS, 0.0, comb)).unwrap();
        assert!(a.square > 0.9 && a.leading() == Kind::Square, "a comb's flat teeth: {a:?}");
        // A shape too thin to read has no language: a hair.
        assert_eq!(of_mask(&mask(PIXELS, 0.0, |x, y| x.abs() < 0.45 && y.abs() < 0.004)), None, "a line of four pixels");
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
