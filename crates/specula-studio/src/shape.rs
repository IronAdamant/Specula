//! Shape language (docs/design/FORM.md): how round, square or pointed a silhouette is, as
//! numbers. Round reads as friendly, square as sturdy, pointed as dangerous: a convention of
//! character and prop design, not a law, so it is in every asset's taste measures and is
//! checked only when a style asks for one (`[taste] shape_language`). **It is a heuristic**: on
//! forty common game assets written down before its seventh version read them, it leads as a
//! person would in every mask for 38 of them (FORM.md, "Its seventh version", names
//! the others); on the 162 of the corpus its tests hold, which were read while it was tuned,
//! for 136.
//!
//! Measured on a silhouette mask, **each part at its own scale and weighed by its thickness**
//! (since its fourth version). Every threshold is an angle, a share of a part's thickness or of
//! the shape's size, or a floor of the pixel grid said as one. A test holds 26 shapes drawn at
//! 256 to 512 pixels to the same reading; five common assets with no thin part drift with the
//! size by up to a quarter, cause not traced (below). Every step is linear in the picture's
//! pixels and the outline's length but for a table of range maxima, which grows as its length
//! times its logarithm ([`of_mask_counted`] counts the work).
//!
//! 1. **The parts.** The outline is smoothed by the shape's scale and its **joints** found:
//!    where it turns inward by [`JOINT_DEG`] in one place (the foot of a stalk, an armpit, the
//!    gap between teeth, a bottle's neck). Under each point of the outline the piece is as
//!    thick as the largest disc inside it that touches it there ([`distances`], [`spine`],
//!    [`thickness`]). A point's part is as thick as **the most thickness about it once that
//!    has been level** for [`PLATEAU`] times itself along the outline ([`plateaus`]): from a
//!    box's corner the thickness rises to the box's and stays (the corner is the box's); from
//!    the end of a limb it is level at once (the end is the limb's, whether or not the outline
//!    turns inward where the limb meets the body); along a taper it keeps rising, to the
//!    taper's foot (a spike's tip is the spike's). It is not followed across a joint.
//! 2. **The turning**, the outline's long straight runs straightened first ([`runs`]), each part
//!    smoothed and its direction taken along a chord at its own thickness ([`SPAN`]), and the
//!    net turn taken within a window [`WINDOW`] of the part's thickness long.
//! 3. **A corner or a curve**: a corner is turning in one place ([`KINK`], [`KINK_DEG`]); a
//!    curve is turning spread out, and is round however far it turns in all.
//! 4. **A corner's angle** is its window's net turn less the window's lopsidedness (what the
//!    two sides of the window turn counts only as far as both do): the mouth of a slot in a
//!    disc is a right angle, the rim's own turn being on one side of it only.
//!    **Two outward corners are two corners** however short the stretch between them (since
//!    its seventh version, [`apart`]): where each is square at the scale of the distance
//!    between them, no window between them reaches both (the end of an overhang, a slab or a
//!    stub judged in its body's window was one corner of 180 degrees, a point).
//! 5. **Its class**: gentle up to [`ROUND_DEG`] (part of a curve: an octagon's corners),
//!    square, or from [`POINTED_DEG`] a point. **A corner between two curves** (the end of an
//!    oval, the horn of a crescent, the tip of a leaf) is not square: a curve, or from
//!    [`SOFT_DEG`] a point. Between two curves is two pieces of positive evidence, both from the
//!    smoothed outline: no straight run near it ([`STRAIGHT_DEG`], [`SIDED`]) and the two sides
//!    of its window bending ([`CURVING_DEG`], degrees over each side). A corner with neither is
//!    read by its angle, so a side that was not found never makes a right angle a point.
//! 6. **Its weight**: only outward turning counts (a notch, a slot, the inside corner of an L
//!    count for nothing); a step's turn is the turn within its own window, and the window is
//!    as long as the part is thick, so that **a part's turning counts by its thickness**. A
//!    step takes its class from the corners near it of its own part, and only of parts at least
//!    half as thick ([`THINNER`]): an apple's stalk is not its rim's corner. A point counts
//!    double ([`POINT_WEIGHT`]).
//!
//! So: a disc, an egg, a capsule, an oval up to three times as long as wide and a box rounded
//! by a third of its shorter side are round, and so is a balloon on a string, an apple with its
//! stalk, a snowman with stick arms, a figure of a ball, a box and capsule limbs, a leaf, and a
//! disc with a slot cut in it; a box is square, and so is one rounded by a tenth of its side or
//! with its corners cut off by a seventh of it, a bar however thin, a table on thin legs, a pedestal,
//! a shelf with a cornice, a box with an overhanging slab, a staircase, a comb
//! whose teeth are twelve pixels or more, a bottle and a box whose edges wobble by up to four
//! pixels; a triangle, a spike, a star, a crescent, a mace and an oval five times as long as
//! wide or more are pointed.
//!
//! **What it cannot say steadily** (measured at every degree, mirrored and shifted: the tests
//! in `tests/shape_families.rs` hold the numbers, FORM.md has them):
//! - A corner rounded by between a sixth and a quarter of the box's shorter side is between
//!   square and round, by degrees, and so is one cut off flat by a fifth. The share moves by
//!   up to a quarter as a box of 0.8 by 0.4 or squarer is turned, by up to a third on a plank
//!   a quarter as thick as long (0.8 by 0.2), and **by up to a half on one an eighth as thick**
//!   (0.8 by 0.1: its corner is a few pixels round at 512, and the pixels decide).
//! - An oval four times as long as wide is on the line between round and pointed, and its
//!   lead goes by the angle it is turned to; so is a lens whose tips are right angles.
//! - **A part under about six pixels thick** (a comb's tooth under twelve pixels across) has
//!   its two corners smoothed into one end, and is read as a point.
//! - **A rounded stub on a body** (a sofa's arms, the rolls of a scroll, a submarine's fin)
//!   shorter than six times its own thickness is judged at the body's thickness, and its round
//!   end, a half turn in one window, reads as a corner or a point: a sofa of parts each rounded
//!   by a third reads square. Two square corners have [`apart`]; a round end has nothing yet.
//!   So does the end of a slab under about a sixth as thick as the body it overhangs, at some
//!   angles (the body's smoothing spreads its corners into one).
//! - **Five common assets with no thin part drift with the size of the picture** (a teddy
//!   bear, a penguin, a sunflower, a pig, a chess pawn: their mean share moves by 0.14 to 0.24
//!   at 256 to 384 pixels against 512, and the pawn leads square in a few masks); the cause is
//!   not traced. The 26 sized shapes of the tests hold within 0.12.
//! - A body bristling with more than about a dozen hair-thin parts is not read. Holes are not
//!   traced. It reads outlines, not surfaces.
//! - Shapes near half and half stay there and their lead says nothing: a half disc is half
//!   round and half square (a tombstone, a mushroom, a dome on a flat base, a heater shield),
//!   and so are a dagger, an axe and a figure with its arms at its sides.
//! - FORM.md lists the shapes it reads unsteadily (a T, a gear of eight triangular teeth, a
//!   disc with a notch) and the common assets it reads otherwise than a person would.
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
/// taken as the part's: [`plateaus`]). A box's window is three tenths of its
/// side; a plank's, three tenths of its width; a hair's, nothing to speak of.
pub const WINDOW: f32 = 0.6;
/// The lengths that take the pixels out of the outline ([`CHORD`], [`SMOOTH`],
/// [`STRAIGHT_RUN`]) are shares of the shape's size, or of this many times its greatest
/// thickness where that is less: a bar a fourteenth as thick as long is smoothed as a thing
/// of its own thickness, not of its length (smoothed by its length its square ends were
/// gone). Each part is smoothed, and its direction taken along, those shares of this many
/// times its own thickness (since its fourth review: a comb's tooth was smoothed by the comb).
pub const SPAN: f32 = 2.5;
/// A corner is turning in one place; a curve is turning spread out. The window a kink is
/// looked for in, as a share of the part's thickness (under a third of [`WINDOW`]) ...
pub const KINK: f32 = 0.18;
/// ... and the net turn within it from which the outline begins to have a corner there, and
/// by which it wholly has (degrees). Under it the outline is a curve however far it turns in
/// the larger window: the end of an oval of three to one turns a hundred degrees within
/// [`WINDOW`] and under twenty within this, and is a curve; a corner cut off flat turns
/// forty-five at each end of the cut, and each end is wholly a corner, of the ninety the
/// larger window holds. (Twenty to sixty until its fourth review's mends, with a second
/// source of corners beside it, which made a plank's rounded corners a lottery: [`of_mask`].)
pub const KINK_DEG: (f32, f32) = (15.0, 45.0);
/// The small window reaches at least this many times the smoothing's deviation each way, as far
/// as the smoothing spreads a corner's turn (a floor of the pixel grid: the smoothing is never
/// under two pixels).
pub const KINK_SMOOTH: f32 = 0.6;
/// A corner between two curves (the end of an oval, the horn of a crescent) is not square
/// whatever its angle: it is part of a curve up to the first of these turns and a point from
/// the second (degrees), and between them a share of each. Between two curves means both: no
/// straight side near it ([`SIDED`]), and the two sides of its window bending
/// ([`CURVING_DEG`]). A corner with neither is read by its angle alone.
pub const SOFT_DEG: (f32, f32) = (70.0, 110.0);
/// A corner has a straight side near it when this share of the steps of its part about it are
/// on a straight run ([`STRAIGHT_DEG`]).
pub const SIDED: f32 = 0.2;
/// The outline runs straight about a step when its window ([`WINDOW`]) turns by under the
/// first of these, and not at all by the second (degrees); a step is on a straight run when
/// such a window holds it.
pub const STRAIGHT_DEG: (f32, f32) = (3.0, 6.0);
/// The two sides of a corner's window bend when the less of the two turns by the first of
/// these and wholly by the second (degrees for each step along them, from the corner's middle
/// out to twice the window: the end of an oval of four to one bends by about one, a box's
/// rounded corner by under a fifth beyond its arc).
pub const CURVING_DEG: (f32, f32) = (3.0, 7.0);
/// Where the outline turns inward by this much in one place (degrees) it is a joint between
/// two parts: the foot of a stalk, the neck of a bottle, an armpit, the gap between two teeth.
pub const JOINT_DEG: f32 = 30.0;
/// A step is of a joint where the outline about it (as far each way as the first look's
/// smoothing and chord reach, which are shares of the shape's size) turns inward by this many
/// degrees in all (a sharp inward corner, or a tight inward curve: a slow one is part of a
/// curve). (Half a degree a step until its sixth version: a share of a degree for every pixel,
/// so that the same neck was a joint drawn large and not drawn small.)
pub const JOINT_EDGE: f32 = 15.0;
/// The steps at each end of a part that its thickness is not taken from, as a share of the
/// first look's reach (and an eighth of the part at most): a joint's inside corner is touched
/// by the thicker part's discs. (Six steps until its sixth version, whatever the size.)
pub const JOINT_MARGIN: f32 = 0.33;
/// A part is as thick as the most thickness about a step once that has been level for this
/// many times itself along the outline ([`plateaus`]) ...
pub const PLATEAU: f32 = 6.0;
/// ... rising by no more than this share of itself (and half a pixel) over that length. (Not
/// fewer than so many steps: a part's level is never under [`MIN_THICK`], six times which is
/// 24, the least its fifth version asked for and which therefore never bound.)
pub const PLATEAU_RISE: f32 = 0.1;
/// The thicknesses a part's level is tried at ([`plateaus`]): from [`MIN_THICK`], each this
/// many times the last (and the part's own greatest).
pub const LADDER: f32 = 1.05;
/// The least thickness a part is level at (pixels).
pub const LEVEL_FROM: f32 = 1.0;
/// Under [`MIN_THICK`] a part is level only over this many steps, and with no half pixel to spare.
pub const THIN_LEVEL: usize = 24;
/// A thickness under this many pixels is not read: the shape is `None`; and the thickness about
/// a step is not taken to be level ([`PLATEAU`]) until it is this much.
pub const MIN_THICK: f32 = 4.0;
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
/// How far (pixels) the ends of a straightened side may lie from the line of its middle half.
pub const TRIM_PX: f32 = 0.5;
/// A side's end is pulled back ([`TRIM_PX`]) only where the run beyond it turns from it by less
/// than this (degrees).
pub const TRIM_TURN: f32 = 120.0;
/// A run is a side only if its points lie, on the mean, within this many pixels to one side of
/// its chord ([`runs`]).
pub const BOW_PX: f32 = 0.2;
/// How much more a point counts for than a curve or a square corner of the same turn: a
/// sharp corner draws the eye before a right angle does. Without it a spike is never mostly
/// pointed: its tip turns under 180 degrees and the two corners of its base 90 each.
pub const POINT_WEIGHT: f32 = 2.0;
/// A step takes its class from the corners near it only of parts at least this share as thick
/// as its own: an apple's rim is not classed by the tip of its stalk, a box's side by the end
/// of a thin limb that meets it.
pub const THINNER: f32 = 0.5;
/// A net turn of this much or more is a corner (counted, in `corners`).
pub const CORNER_DEG: f32 = 60.0;
/// The side, in pixels, of the silhouettes the studio measures shape language on.
pub const PIXELS: u32 = 512;
/// A piece smaller than this across (pixels, its greatest width) is not read: under it the
/// pixels' steps are as large as what is measured, and a triangle of 96 pixels read as
/// anything from a tenth to wholly pointed by the angle it was turned to.
pub const MIN_SIZE: f32 = 192.0;
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
fn thickness(spine: &[(f32, f32, f32)], at: &[P], w: usize, h: usize, work: &mut u64) -> Vec<f32> {
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
                *work += 1 + list.len() as u64;
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

/// The points of a closed outline (`points`, from the first) at which a straightened outline
/// turns, each with whether the stretch from it to the next is a side to straighten. A run is
/// as long as it can be with every point of it within `slack` of the straight line from its
/// first point through its last, and going on along that line. Found in one walk round (each
/// point is looked at once, or twice where a run ends): the directions from a run's first point
/// that keep every point so far within `slack` narrow as the run goes on, and the run ends at
/// the first point outside them. (Douglas and Peucker's rule until its sixth version, which is
/// quadratic where the farthest point is always at one end of what is left: a comb of
/// one-pixel teeth took a third of a second.)
///
/// A long run is a side if it does not bow: the mean offset of its middle half from the line
/// through the means of its first and last quarters is under [`BOW_PX`] (the steps of a slanted
/// side's pixels fall to both sides of it; a curve's flank, within `slack` of its chord for
/// ninety pixels on a large oval, falls all to one). Where it meets the next run (or the last)
/// at a gentle turn, under [`TRIM_TURN`], its end is pulled back to where the outline leaves
/// that line by more than [`TRIM_PX`]: a run reaches as far into the rounding of a corner as
/// `slack` lets it, and left there it would end in a kink. At a sharp corner it is not (the
/// stub left at the tip of a blade turned the outline inside out).
fn runs(points: &[P], slack: f32, long: f32, work: &mut u64) -> Vec<(usize, bool)> {
    let n = points.len();
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let a = points[i];
        let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
        let mut along: Option<P> = None;
        let mut furthest = 0.0f32;
        let mut end = i + 1;
        let mut j = i + 1;
        while j <= n {
            *work += 1;
            let v = sub(points[j % n], a);
            let d = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if d > slack {
                let r = *along.get_or_insert([v[0] / d, v[1] / d]);
                let at = specula_core::dmath::atan2(cross(r, v), r[0] * v[0] + r[1] * v[1]);
                // (And the run goes on along its line: one that came back on itself, round the
                // tip of a thin blade, would cut the tip off.)
                let ahead = r[0] * v[0] + r[1] * v[1];
                if at < lo || at > hi || ahead < furthest - slack {
                    break;
                }
                furthest = furthest.max(ahead);
                let give = specula_core::dmath::atan2(slack, (d * d - slack * slack).sqrt());
                (lo, hi) = (lo.max(at - give), hi.min(at + give));
            }
            end = j;
            j += 1;
        }
        found.push((i, end));
        i = end;
    }
    // Each run's way along it (from its first point to its last).
    let way: Vec<P> = found.iter().map(|(a, b)| { let v = sub(points[*b % n], points[*a]); let d = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-6); [v[0] / d, v[1] / d] }).collect();
    let gentle = |k: usize, l: usize| way[k][0] * way[l][0] + way[k][1] * way[l][1] >= TRIM_TURN.to_radians().cos();
    let mut kept: Vec<(usize, bool)> = Vec::new();
    let runs = found.len();
    for (k, (i, end)) in found.iter().copied().enumerate() {
        kept.push((i, false));
        if dist(points[i], points[end % n]) < long || end <= i + 8 {
            continue;
        }
        let (q1, q3) = (points[i + (end - i) / 4], points[i + 3 * (end - i) / 4]);
        let (dir, len) = (sub(q3, q1), dist(q1, q3));
        let off = |p: P| if len > 1e-6 { (cross(dir, sub(p, q1)) / len).abs() } else { 0.0 };
        let (mut s, mut e) = (i, end);
        if runs > 1 && gentle((k + runs - 1) % runs, k) {
            while s + 1 < e && off(points[s % n]) > TRIM_PX {
                s += 1;
            }
        }
        if runs > 1 && gentle(k, (k + 1) % runs) {
            while e > s + 1 && off(points[e % n]) > TRIM_PX {
                e -= 1;
            }
        }
        // (The line through the means of its first and last quarters, and the mean offset of
        // its middle half from that line: means, so that the pixels' steps cancel.)
        let mean = |a: usize, b: usize| {
            let c = (b - a).max(1) as f32;
            let t = (a..b).fold([0.0f32; 2], |t, k| [t[0] + points[k % n][0], t[1] + points[k % n][1]]);
            [t[0] / c, t[1] / c]
        };
        let q = (e - s) / 4;
        let (pa, pb) = (mean(s, s + q.max(1)), mean(e - q.max(1) + 1, e + 1));
        let (cd, cl) = (sub(pb, pa), dist(pa, pb));
        let bow = if cl > 1e-6 && q >= 1 { (s + q..e - q).map(|k| cross(cd, sub(points[k % n], pa)) / cl).sum::<f32>() / (e - s - 2 * q).max(1) as f32 } else { f32::INFINITY };
        *work += 2 * (end - i) as u64;
        if bow.abs() > BOW_PX || dist(points[s % n], points[e % n]) < long {
            continue;
        }
        if s > i {
            kept.push((s, true));
        } else {
            kept.last_mut().unwrap().1 = true;
        }
        if e < end && e < n {
            kept.push((e, false));
        }
    }
    kept
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

/// A part id for the steps of a joint, which belong to no part.
const JOINT: u32 = u32::MAX;

/// An outline smoothed and walked again in steps of one pixel of its own length, and which
/// point of the outline each step lies after.
struct Walked {
    even: Vec<P>,
    from: Vec<usize>,
}

/// `points` (closed, a pixel apart) smoothed, each point the weighted mean of its neighbours
/// along it by a Gaussian of its own deviation `sigma` (so that the polygon's own corners on a
/// curve run together into the curve), then walked again in steps of one pixel of its own
/// length (smoothed, a slanted side's steps are shorter than the pixels were).
fn walk(points: &[P], sigma: &[f32], work: &mut u64) -> Walked {
    let n = points.len();
    let smooth: Vec<P> = (0..n)
        .map(|i| {
            let s = sigma[i];
            let reach = ((3.0 * s).ceil() as usize).min(n / 2);
            *work += 1 + reach as u64;
            let (mut at, mut total) = ([points[i][0], points[i][1]], 1.0f32);
            for k in 1..=reach {
                let w = specula_core::dmath::exp(-0.5 * (k as f32 / s) * (k as f32 / s));
                let (a, b) = (points[(i + k) % n], points[(i + n - k % n) % n]);
                at = [at[0] + (a[0] + b[0]) * w, at[1] + (a[1] + b[1]) * w];
                total += 2.0 * w;
            }
            [at[0] / total, at[1] / total]
        })
        .collect();
    let length: f32 = (0..n).map(|i| dist(smooth[i], smooth[(i + 1) % n])).sum();
    let count = length.round() as usize;
    let step = length / count.max(1) as f32;
    let (mut even, mut from) = (Vec::with_capacity(count), Vec::with_capacity(count));
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
    *work += (n + even.len()) as u64;
    Walked { even, from }
}

/// How far the outline turns from each step to the next (radians), its direction at a step
/// being taken along a chord about it: from the middle of the `chord[i]` steps behind to the
/// middle of those ahead (the mean of many points is steadier than two, and what is left of
/// the pixels' steps on a slanted side made a square corner read as anything from 75 to 108
/// degrees).
fn turning(even: &[P], chord: &[usize]) -> Vec<f32> {
    let count = even.len();
    let mut at = vec![[0.0f64; 2]; 3 * count + 1];
    for j in 0..3 * count {
        let p = even[j % count];
        at[j + 1] = [at[j][0] + f64::from(p[0]), at[j][1] + f64::from(p[1])];
    }
    let heading: Vec<f32> = (0..count)
        .map(|i| {
            let c = chord[i].min(count / 3);
            let (ahead, behind) = ((i + count + 1, i + count + 1 + c), (i + count - c, i + count));
            let d = [(at[ahead.1][0] - at[ahead.0][0]) - (at[behind.1][0] - at[behind.0][0]), (at[ahead.1][1] - at[ahead.0][1]) - (at[behind.1][1] - at[behind.0][1])];
            // (The library's own arc tangent: the same on every machine.)
            specula_core::dmath::atan2(d[1] as f32, d[0] as f32)
        })
        .collect();
    (0..count)
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
        .collect()
}

/// Each value the median of the five about it, round a closed outline (a disc that touches
/// one step alone is not the part's).
fn median5(raw: &[f32]) -> Vec<f32> {
    let count = raw.len();
    (0..count)
        .map(|i| {
            let mut five = [raw[(i + count - 2) % count], raw[(i + count - 1) % count], raw[i], raw[(i + 1) % count], raw[(i + 2) % count]];
            five.sort_by(f32::total_cmp);
            five[2]
        })
        .collect()
}

/// The steps of the outline that are joints: where it turns inward (against `way`, its own
/// way round) within `k` steps of each by [`JOINT_DEG`] or more in all, on through a run of
/// such steps.
fn joints(turn: &[f32], way: f32, k: usize) -> Vec<bool> {
    let count = turn.len();
    let k = k.min(count / 4).max(1);
    let mut sum = vec![0.0f64; 3 * count + 1];
    for j in 0..3 * count {
        sum[j + 1] = sum[j] + f64::from(turn[j % count]);
    }
    let inward = |a: usize, b: usize| -((sum[b] - sum[a]) as f32) * way;
    // (A step turns inward when its neighbourhood does by [`JOINT_EDGE`] in all.)
    let edge = JOINT_EDGE.to_radians();
    let candidate: Vec<bool> = (0..count).map(|i| inward(i + count - k, i + count + k + 1) >= edge).collect();
    let mut joint = vec![false; count];
    let Some(start) = (0..count).find(|i| !candidate[*i]) else {
        return joint;
    };
    let mut i = 0;
    while i < count {
        let at = (start + i) % count;
        if !candidate[at] {
            i += 1;
            continue;
        }
        let mut len = 0;
        while i + len < count && candidate[(start + i + len) % count] {
            len += 1;
        }
        let a = start + i + count;
        if inward(a - k, a + len + k) >= JOINT_DEG.to_radians() {
            for j in 0..len {
                joint[(at + j) % count] = true;
            }
        }
        i += len;
    }
    joint
}

/// The greatest of any run of a sequence, in constant time once built (a sparse table: the
/// greatest of each run of a power of two long).
struct Greatest {
    levels: Vec<Vec<f32>>,
}

impl Greatest {
    fn new(v: &[f32], work: &mut u64) -> Greatest {
        let mut levels = vec![v.to_vec()];
        let mut w = 1;
        while 2 * w <= v.len() {
            let last = &levels[levels.len() - 1];
            let next: Vec<f32> = (0..=v.len() - 2 * w).map(|i| last[i].max(last[i + w])).collect();
            *work += next.len() as u64;
            levels.push(next);
            w *= 2;
        }
        Greatest { levels }
    }

    /// The greatest of `a..=b`.
    fn of(&self, a: usize, b: usize) -> f32 {
        let k = (usize::BITS - 1 - (b - a + 1).leading_zeros()) as usize;
        self.levels[k][a].max(self.levels[k][b + 1 - (1 << k)])
    }
}

/// How thick the part is that each step of `raw` (the thickness under each step of a part's
/// outline; `cyclic` when the part is the whole outline) is of ([`PLATEAU`]): the least of
/// the thicknesses `T` of a ladder (from [`MIN_THICK`], each [`LADDER`] times the last, and the
/// part's own greatest) such that, from the nearest step at least `(1 - PLATEAU_RISE) T - 0.5`
/// thick, the outline is no thicker than `T` for [`PLATEAU`]` T` steps further each way; and
/// the part is that thick there: the greatest thickness in that reach. ...
fn plateaus(raw: &[f32], cyclic: bool, work: &mut u64) -> Vec<f32> {
    let n = raw.len();
    let most = raw.iter().copied().fold(0.0f32, f32::max);
    // (A closed part is read three times round, so that a reach is a run of the sequence.)
    let seq: Vec<f32> = if cyclic { raw.iter().chain(raw).chain(raw).copied().collect() } else { raw.to_vec() };
    let base = if cyclic { n } else { 0 };
    let table = Greatest::new(&seq, work);
    // The greatest within `reach` steps of step `i` each way (the whole part once the reach
    // goes round it, or past both its ends).
    let within = |i: usize, reach: usize| -> f32 {
        if cyclic {
            if 2 * reach + 1 >= n { most } else { table.of(base + i - reach, base + i + reach) }
        } else {
            table.of(i.saturating_sub(reach), (i + reach).min(n - 1))
        }
    };
    let mut ladder: Vec<f32> = Vec::new();
    let mut t = LEVEL_FROM;
    while t < most {
        ladder.push(t);
        t *= LADDER;
    }
    ladder.push(most);
    let mut out = vec![f32::NAN; n];
    let mut near = vec![usize::MAX; seq.len()];
    for t in ladder {
        if t < LEVEL_FROM {
            break;
        }
        // How far each step is from the nearest one at least so thick.
        let thin = t < MIN_THICK;
        let floor = (1.0 - PLATEAU_RISE) * t - if thin { 0.0 } else { 0.5 };
        let mut last: Option<usize> = None;
        for (j, v) in seq.iter().enumerate() {
            if *v >= floor {
                last = Some(j);
            }
            near[j] = last.map_or(usize::MAX, |l| j - l);
        }
        last = None;
        for j in (0..seq.len()).rev() {
            if seq[j] >= floor {
                last = Some(j);
            }
            if let Some(l) = last {
                near[j] = near[j].min(l - j);
            }
        }
        *work += 2 * seq.len() as u64;
        let span = ((PLATEAU * t).ceil() as usize).max(if thin { THIN_LEVEL } else { 0 });
        for i in 0..n {
            *work += 1;
            if !out[i].is_nan() || raw[i] > t {
                continue;
            }
            let r = near[base + i];
            if r == usize::MAX {
                continue;
            }
            let reach = r + span;
            let m = within(i, reach.min(n));
            if m <= t {
                out[i] = m;
            }
        }
    }
    // (Where no rung holds, the part is thinner than is read: it is as thick as it is at most.)
    for v in out.iter_mut() {
        if v.is_nan() {
            *v = most;
        }
    }
    out
}

/// Each step's part (the outline between two joints; [`JOINT`] at a joint) and how thick the
/// part is there ([`plateaus`]). A part's first and last few steps are read as no
/// thicker than the step within them (a joint's inside corner is touched by the thicker
/// part's discs). A joint's steps take the thinner of the two parts beside them.
fn parts(joint: &[bool], raw: &[f32], reach: usize, work: &mut u64) -> (Vec<u32>, Vec<f32>) {
    let count = raw.len();
    let mut part = vec![JOINT; count];
    let mut host = vec![0.0f32; count];
    let Some(start) = (0..count).find(|i| joint[*i]) else {
        return (vec![0; count], plateaus(raw, true, work));
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 1;
    while i < count {
        let at = (start + i) % count;
        if joint[at] {
            i += 1;
            continue;
        }
        let mut len = 0;
        while i + len < count && !joint[(start + i + len) % count] {
            len += 1;
        }
        runs.push((start + i, len));
        i += len;
    }
    *work += count as u64;
    for (id, (a, len)) in runs.iter().enumerate() {
        let margin = (len / 8).min((JOINT_MARGIN * reach as f32).round() as usize);
        let mut own: Vec<f32> = (0..*len).map(|k| raw[(a + k) % count]).collect();
        for k in 0..margin {
            own[k] = own[k].min(own[margin]);
            own[len - 1 - k] = own[len - 1 - k].min(own[len - 1 - margin]);
        }
        for (k, t) in plateaus(&own, false, work).into_iter().enumerate() {
            part[(a + k) % count] = id as u32;
            host[(a + k) % count] = t;
        }
    }
    // (A joint's steps, from the parts beside it: the nearest part's thickness each way, in
    // two walks twice round.)
    let mut back = vec![f32::NAN; count];
    let mut last = f32::NAN;
    for k in 0..2 * count {
        let j = (start + k) % count;
        if part[j] != JOINT {
            last = host[j];
        } else {
            back[j] = last;
        }
    }
    last = f32::NAN;
    for k in (0..2 * count).rev() {
        let j = (start + k) % count;
        if part[j] != JOINT {
            last = host[j];
        } else if !(back[j].is_nan() || last.is_nan()) {
            host[j] = back[j].min(last);
        } else {
            host[j] = raw[j];
        }
    }
    *work += 4 * count as u64;
    (part, host)
}

/// The five windows a corner's angle is the mean over, in twentieths of its half window.
const SPANS: [usize; 5] = [15, 20, 20, 24, 27];

/// **Two corners are two corners, however short the stretch between them** (its seventh
/// version). A stub or an overhang shorter than [`PLATEAU`] times its own thickness takes the
/// thickness of the body it stands on ([`plateaus`]: its level is not reached before the body's
/// rise), and so is judged in the body's window; that window held both square corners of its
/// end, which then read as one corner of 180 degrees, a point (a pedestal, a shelf with a
/// cornice, a slab across a box: its sixth review). So, for each step: how far its window is
/// to be cut, and by how much (`(cap, s)`; `s` 0 where nothing is cut).
///
/// - **The corners**: the peaks of `net`, the turn within the smoothing's own reach about each
///   step, from [`KINK_DEG`]'s first angle; two peaks of one sign are one corner unless the turn
///   between them falls to under half the less of them.
/// - Two corners that follow each other round the outline, no closer than the smoothing spreads
///   either (`blur`: closer, it cannot tell them apart), each **square at its own scale**: its
///   outward turn within half the distance to its nearer neighbour each way, by [`ROUND_DEG`]
///   (`s`, the less square of the two). So both are **outward**: an inward corner's own turn is
///   less than nothing, and its `s` is 0. (A separate test of the corners' sign stood here
///   until its seventh version was held; seeded away, it changed no reading of 548 shapes, since
///   this one already decides it.) A jog (an outward corner a few pixels from an inward one) and
///   the two 45-degree kinks of a corner cut off flat are not square at that scale, and stay as
///   they were.
/// - Every step between them is read, beside its own window, in one cut short of the farther
///   corner (its longest window, `SPANS`, stops before it), and its angle and weight are those
///   of the whole window moved toward the cut one's by `s` (`read`). Never a window of a length
///   between: one that holds both corners and not the inward corner beside them reads a point.
///
/// Every length is a distance along the outline between corners, or a share of one, and every
/// threshold an angle the measure already has or a half: drawn twice as large, it cuts alike.
fn apart(net: &[f32], sum: &[f64], way: f32, half: &[usize], blur: &[usize], work: &mut u64) -> Vec<(usize, f32)> {
    let count = net.len();
    let mut cut = vec![(0usize, 0.0f32); count];
    let v = |i: usize| net[i % count].abs();
    let floor = KINK_DEG.0.to_radians();
    // The corners round the outline in order, and whether each turns outward: the peaks of the
    // turn within the smoothing's own reach (`net`), from [`KINK_DEG`]'s first angle; two peaks of
    // one sign are two corners where the turn between them falls to under half the less of
    // them (two corners closer than a window of the body's are one run of turning there: an
    // overhang's end).
    let mut peaks: Vec<usize> = (0..count).filter(|i| v(*i) >= floor && v(*i) >= v(*i + count - 1) && v(*i) > v(*i + 1)).collect();
    *work += 3 * count as u64;
    if peaks.len() < 2 {
        return cut;
    }
    let mut corners: Vec<usize> = Vec::new();
    for p in peaks.drain(..) {
        if let Some(&q) = corners.last() {
            if (net[q] > 0.0) == (net[p] > 0.0) {
                let low = (q..=p).map(&v).fold(f32::INFINITY, f32::min);
                *work += (p - q + 1) as u64;
                if low >= 0.5 * v(q).min(v(p)) {
                    if v(p) > v(q) {
                        *corners.last_mut().unwrap() = p;
                    }
                    continue;
                }
            }
        }
        corners.push(p);
    }
    // (And round the end to the first.)
    if corners.len() > 2 {
        let (q, p) = (corners[corners.len() - 1], corners[0]);
        if (net[q] > 0.0) == (net[p] > 0.0) && (q..=p + count).map(&v).fold(f32::INFINITY, f32::min) >= 0.5 * v(q).min(v(p)) {
            if v(q) > v(p) {
                corners[0] = q;
            }
            corners.pop();
        }
    }
    // (Whether each turns outward is not asked here: the own turn below asks it.)
    let n = corners.len();
    if n < 2 {
        return cut;
    }
    let turn = |a: usize, b: usize| ((sum[b] - sum[a]) as f32 * way).to_degrees();
    let longest = SPANS[SPANS.len() - 1];
    for c in 0..n {
        let (a, b) = (corners[c], corners[(c + 1) % n]);
        let d = (b + count - a) % count;
        // (Two corners closer than the smoothing spreads either, as far as it reaches each way,
        // are not told apart by it: a stalk a few pixels wide judged at its rim's scale.)
        if d < blur[a].max(blur[b]) {
            continue;
        }
        // (Each corner's own turn: within half the distance to its nearer neighbour each way,
        // so that a jog, an outward corner a few pixels from an inward one, is not square at
        // its own scale where the smoothing spreads it over more than that.)
        let (before, after) = (corners[(c + n - 1) % n], corners[(c + 2) % n]);
        let (ra, rb) = (((a + count - before) % count).min(d) / 2, ((after + count - b) % count).min(d) / 2);
        let own = turn(a + count - ra, a + count + ra).min(turn(b + count - rb, b + count + rb));
        let s = smoothstep(ROUND_DEG.0, ROUND_DEG.1, own);
        if s <= 0.0 {
            continue;
        }
        for j in 0..=d {
            let i = (a + j) % count;
            // (The longest of a step's five windows reaches `longest` twentieths of its half:
            // it must stop short of the farther corner.)
            let cap = ((j.max(d - j) - 1) * 20 / longest).max(2);
            if cap < half[i] && s > cut[i].1 {
                cut[i] = (cap, s);
            }
        }
        *work += d as u64 + 1;
    }
    cut
}

/// One silhouette's shape language. `None` for an empty mask, one too large to trace, or a
/// piece too small to have a shape (under [`MIN_SIZE`] pixels across).
pub fn of_mask(m: &Mask) -> Option<ShapeLanguage> {
    of_mask_counted(m).0
}

/// What [`of_mask`] reads, and how much work it took to read it: the steps of every loop it
/// ran, in all (for the tests that hold its cost to the size of the mask and the length of
/// the outline).
#[doc(hidden)]
pub fn of_mask_counted(m: &Mask) -> (Option<ShapeLanguage>, u64) {
    let mut work = 0u64;
    let read = read(m, &mut work);
    (read, work)
}

fn read(m: &Mask, work: &mut u64) -> Option<ShapeLanguage> {
    let (points, inside) = outline(m)?;
    // The shape's size is its greatest width: the same whichever way it is turned. (The
    // longer side of the box round it grows by two fifths as a square is turned to stand on
    // a corner, and every length here is a share of the size: the same corner was a curve at
    // one angle and a corner at another. Its second review.)
    let hull = hull_of(&points);
    let size = greatest_width(&hull);
    // (The outline, the flood that finds the piece, the hull and every pair of its points.)
    *work += (m.w as u64 + 1) * (m.h as u64 + 1) * 4 + points.len() as u64 * 4 + (hull.len() * hull.len()) as u64;
    if size < MIN_SIZE {
        return None;
    }
    // How thick the piece is: the discs that fill it. What takes the pixels out of the outline
    // below is sized by the shape, or by its greatest thickness where it is all thin.
    let spine = spine(&distances(&inside, m.w as usize, m.h as usize), m.w as usize, m.h as usize);
    // (The distances, three passes over the pixels, and the spine, sixteen neighbours a pixel.)
    *work += 20 * (m.w as u64) * (m.h as u64) + spine.len() as u64;
    let thickest = spine.first().map_or(0.0, |s| s.2);
    if thickest < MIN_THICK {
        return None;
    }
    let scale = size.min(SPAN * thickest);
    // The pixels' steps are taken out first: the outline straightened to a polygon that
    // stays within a pixel of it (a slanted side, whose pixels step, is then one straight
    // side: smoothing alone left a ripple along it that read as a gentle curve, and a box
    // turned five degrees was a quarter round), and walked again a pixel at a time.
    let kept = runs(&points, STRAIGHTEN_PX, (STRAIGHT_RUN * scale).max(8.0), work);
    if kept.len() < 3 {
        return None;
    }
    // Only a long run is a side to straighten. A short one is a piece of a curve, and a curve
    // made a polygon has corners of its own, laid by where the tracing happened to begin: a
    // corner rounded by a sixteenth of the size read as round or as square by the angle the
    // box was turned to (its second review). A curve keeps its pixels, and the smoothing
    // below, which is as long as their steps there, takes them out.
    let mut walked: Vec<P> = Vec::new();
    for (k, (i, side)) in kept.iter().enumerate() {
        let j = kept[(k + 1) % kept.len()].0;
        let (a, b) = (points[*i], points[j % points.len()]);
        if *side {
            let steps = (dist(a, b).round() as usize).max(1);
            walked.extend((0..steps).map(|k| [a[0] + (b[0] - a[0]) * k as f32 / steps as f32, a[1] + (b[1] - a[1]) * k as f32 / steps as f32]));
        } else {
            let end = if j > *i { j } else { points.len() };
            walked.extend_from_slice(&points[*i..end]);
        }
    }
    let outline_points = points;
    let points = walked;
    let (w, h) = (m.w as usize, m.h as usize);
    // **The parts, from a first look** at the outline smoothed by the shape's own scale: where
    // it turns inward by [`JOINT_DEG`] or more in one place (the foot of a stalk, the neck of a
    // bottle, the armpit of a figure, the gap between two teeth) is a joint, and the outline
    // between two joints is one part. A part's thickness is the largest disc that touches it
    // (a corner's disc is small, and it is a corner of its part: a box's corner is as thick
    // as the box). Concave turning is a property of the shape, not of where the outline's walk
    // passes: a joint moves a pixel when the shape does. (Its fourth review: the joint was a
    // leap in the thickness along the outline, which flickered as limbs and necks turned.)
    let sigma_g = (SMOOTH * scale).max(2.0);
    let chord_g = ((CHORD * scale).round() as usize).max(1);
    let coarse = walk(&points, &vec![sigma_g; points.len()], work);
    if coarse.even.len() < 8 * chord_g.max(2) {
        return None;
    }
    let turn_g = turning(&coarse.even, &vec![chord_g; coarse.even.len()]);
    // (The turning, the joints and the median, a few walks round the outline.)
    *work += 12 * coarse.even.len() as u64;
    // (Its own way round, from the area the traced outline holds: the sum of its turning says
    // the same of a clean outline, and the opposite of one that a straightened run has made
    // loop on itself at a sharp tip, where the whole outline then read inside out.)
    let way = if area(&outline_points) >= 0.0 { 1.0f32 } else { -1.0 };
    let raw_g = median5(&thickness(&spine, &coarse.even, w, h, work));
    let reach_g = (2.0 * sigma_g).ceil() as usize + chord_g;
    let joint_g = joints(&turn_g, way, reach_g);
    let (part_g, host_g) = parts(&joint_g, &raw_g, reach_g, work);
    // (Carried back to the outline's own points, to smooth each part by its thickness.)
    let n = points.len();
    let (mut host_w, mut part_w) = (vec![-1.0f32; n], vec![JOINT; n]);
    for (e, src) in coarse.from.iter().enumerate() {
        if host_w[*src] < 0.0 {
            (host_w[*src], part_w[*src]) = (host_g[e], part_g[e]);
        }
    }
    let first = host_w.iter().position(|t| *t >= 0.0)?;
    for k in 1..n {
        let (i, before) = ((first + k) % n, (first + k - 1) % n);
        if host_w[i] < 0.0 {
            (host_w[i], part_w[i]) = (host_w[before], part_w[before]);
        }
    }
    // **Then each part is smoothed, and its direction taken, at its own thickness**: a comb's
    // tooth of eight pixels by a deviation of two, its body by the body's. (Smoothed by the
    // whole shape's size, a narrow tooth's square end was rounded into a half disc before its
    // own window saw it, and a comb read as points: its fourth review.)
    let sigma_w: Vec<f32> = host_w.iter().map(|t| (SMOOTH * SPAN * t).clamp(2.0, sigma_g)).collect();
    let fine = walk(&points, &sigma_w, work);
    let count = fine.even.len();
    if count < 8 * chord_g.max(2) {
        return None;
    }
    let (even, from) = (fine.even, fine.from);
    let host: Vec<f32> = from.iter().map(|s| host_w[*s]).collect();
    let part: Vec<u32> = from.iter().map(|s| part_w[*s]).collect();
    let chord: Vec<usize> = host.iter().map(|t| ((CHORD * SPAN * t).round() as usize).clamp(1, chord_g)).collect();
    let blur: Vec<usize> = from.iter().zip(&chord).map(|(s, c)| (2.0 * sigma_w[*s]).ceil() as usize + c).collect();
    let half: Vec<usize> = host.iter().map(|t| ((0.5 * WINDOW * t).round() as usize).clamp(2, (count / 8).max(2))).collect();
    let turn = turning(&even, &chord);
    // (The turn so far, three times round, so that a window is a difference of two sums. In f64:
    // thousands of small turns are summed.)
    let mut sum = vec![0.0f64; 3 * count + 1];
    for j in 0..3 * count {
        sum[j + 1] = sum[j] + f64::from(turn[j % count]);
    }
    // (The small window is not rounded to whole steps: on a part a few pixels thick it is a
    // step or two, and two steps where it should be one made a capsule's round end a corner
    // drawn small and a curve drawn large. The turn so far is read between steps.)
    let at = |x: f64| { let k = x.floor(); let j = k as usize; sum[j] + (sum[j + 1] - sum[j]) * (x - k) };
    let kinked = |half: &[usize]| -> Vec<f32> { (0..count).map(|i| { let s = f64::from((0.5 * KINK * host[i]).max(KINK_SMOOTH * sigma_w[from[i]]).min(half[i] as f32)); (at((i + count) as f64 + s) - at((i + count) as f64 - s)) as f32 }).collect() };
    // **Two corners are two corners**, however short the stretch between them (its seventh
    // version): no window holds both of two outward corners that are each square at the scale
    // of the distance between them ([`apart`]).

    let sharp: Vec<f32> = (0..count).map(|i| { let s = f64::from((KINK_SMOOTH * sigma_w[from[i]]).max(1.0)); (at((i + count) as f64 + s) - at((i + count) as f64 - s)) as f32 }).collect();
    let cut = apart(&sharp, &sum, way, &half, &blur, work);
    let small: Vec<usize> = host.iter().zip(&half).map(|(t, h)| ((0.5 * KINK * t).round() as usize).clamp(2, *h)).collect();
    let net: Vec<f32> = (0..count).map(|i| (sum[i + count + half[i]] - sum[i + count - half[i]]) as f32).collect();
    let fine_net = kinked(&half);
    // Only where the outline turns its own way round (outward: its bulges and its corners)
    // does it count; where it turns back on itself (a notch, a slot, the inside corner of an
    // L) it does not.
    // (Where [`apart`] cuts a window, what the cut window holds, by as much as the cut is made.)
    let out: Vec<f32> = (0..count).map(|i| { let whole = (net[i] * way).max(0.0).to_degrees(); let (cap, s) = cut[i]; if s > 0.0 { let held = (((sum[i + count + cap] - sum[i + count - cap]) as f32) * way).max(0.0).to_degrees(); whole + s * (held - whole) } else { whole } }).collect();
    // How much of a corner the outline has at each step: its turn within the small window.
    // (Either way: an inside corner is a corner too, and takes its turn back out of the window
    // it shares with an outside one, as it does out of the window's net turn.) (There was a
    // second source of corners until its fourth review's mends: where two runs the
    // straightening kept met at 30 to 40 degrees, a corner for certain. The chords a small
    // rounded corner is straightened into meet so, or not, by the angle the shape is turned
    // to, and a plank rounded by a sixth of its width swung from a twentieth round to three
    // fifths.)
    let kink: Vec<f32> = fine_net.iter().map(|n| smoothstep(KINK_DEG.0, KINK_DEG.1, n.abs().to_degrees())).collect();
    // Where the outline runs straight: a window about it turns by under [`STRAIGHT_DEG`] (and
    // wholly turns, by the second). A step is **on a side** where a straight window holds it
    // (and so a corner's own steps are on its sides). That is positive evidence of a side, from
    // how the smoothed outline turns, and a wobble of a pixel along it changes nothing. (Its
    // fourth review: a side was a run the straightening kept within a pixel and a half, and a
    // slanted side whose pixels wobbled by half a pixel was cut into runs too short to be
    // sides: a box had none, and its right angles read as points.)
    let straight: Vec<f32> = net.iter().map(|n| 1.0 - smoothstep(STRAIGHT_DEG.0, STRAIGHT_DEG.1, n.abs().to_degrees())).collect();
    let on_side: Vec<f32> = (0..count).map(|i| (0..=2 * half[i]).map(|k| straight[(i + 2 * count - half[i] + k) % count]).fold(0.0f32, f32::max)).collect();

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
    // And how far the two sides of the window bend, the less of the two: **positive evidence
    // that a corner lies between two curves** (the end of an oval, the horn of a crescent), as
    // a side is positive evidence of a side.
    let mut curving = vec![0.0f32; count];
    let cornered: Vec<f32> = (0..count)
        .map(|i| {
            let core = small[i].max(blur[i]);
            let at = i + count;
            let net = |from: usize, to: usize| ((sum[to] - sum[from]) as f32) * way;
            let angle = |half: usize, core: usize| {
                SPANS
                    .iter()
                    .map(|k| {
                        let reach = (half * k / 20).max(core);
                        let (before, after) = (net(at - reach, at - core), net(at + core, at + reach));
                        net(at - reach, at + reach) - (before - after).abs()
                    })
                    .sum::<f32>()
                    / 5.0
            };
            // (Where [`apart`] cuts the window, the angle the cut window holds, by as much as the
            // cut is made; the middle is cut with it: a middle as wide as the smoothing spreads a
            // corner would hold both corners again.)
            let (cap, s) = cut[i];
            let whole = angle(half[i], core);
            let mean = if s > 0.0 { whole + s * (angle(cap, core.min(cap)) - whole) } else { whole };
            // (Along each side from the middle out to twice the window: the end of a long oval is
            // straightened into chords near its tip, and its sides bend beyond them.)
            let far = (2 * half[i]).min(count / 8);
            if far >= core + 4 {
                let bend = |from: usize, to: usize| ((sum[to] - sum[from]) as f32) * way;
                let (before, after) = (bend(at - far, at - core), bend(at + core, at + far));
                curving[i] = smoothstep(CURVING_DEG.0, CURVING_DEG.1, before.min(after).to_degrees());
            }
            mean.to_degrees().clamp(0.0, 180.0)
        })
        .collect();
    // How far about each step, each way, the steps of its own part reach (as far as its window
    // reaches, and as far again as the smoothing spreads a corner's turn: on a thin part that
    // is as far as the window itself, and the steps at the edge of a corner's turn were read
    // as a curve); and how much of that runs straight, as a share of [`SIDED`].
    let extent: Vec<(usize, usize)> = (0..count)
        .map(|i| {
            let reach = half[i] + blur[i];
            let ours = |j: usize| part[j] == part[i] || part[j] == JOINT;
            let on = (1..=reach).find(|k| !ours((i + k) % count)).map_or(reach, |k| k - 1);
            let back = (1..=reach).find(|k| !ours((i + count - k) % count)).map_or(reach, |k| k - 1);
            (back, on)
        })
        .collect();
    let sided: Vec<f32> = (0..count)
        .map(|i| {
            let (back, on) = extent[i];
            let sides: f32 = (0..=back + on).map(|k| on_side[(i + count - back + k) % count]).sum();
            (sides / (back + on + 1) as f32 / SIDED).min(1.0)
        })
        .collect();
    // (Each step's reach, its sides and its window's five spans, each a walk the length of its reach.)
    *work += (0..count).map(|i| (4 * (half[i] + blur[i]) + 2 * half[i] + 1) as u64).sum::<u64>() + 10 * count as u64;
    let mut shares = [0.0f32; 3];
    let mut corners = 0u32;
    for (i, by) in out.iter().copied().enumerate() {
        // Classed by the most the outline turns outward in any window near this step, **of its
        // own part**: a step beside a corner, whose own window holds only part of the corner's
        // turn, is part of that corner and not of a gentle curve; a step of an apple's rim
        // beside its stalk is of the rim, and the stalk's tip is not its corner (its fourth
        // review: a stalk shorter than the rim's window took a quarter of a disc as a point).
        let (back, on) = extent[i];
        let (mut most, mut corner, mut at_most) = (0.0f32, 0.0f32, i);
        *work += (back + on + 1) as u64;
        for k in 0..=back + on {
            let j = (i + count - back + k) % count;
            // (Not from a part much thinner than its own: a stalk's tip is not its rim's corner,
            // whether or not a joint was found where it stands.)
            if host[j] < THINNER * host[i] {
                continue;
            }
            if cornered[j] > most {
                (most, at_most) = (cornered[j], j);
            }
            corner = corner.max(kink[j]);
        }
        // A curve where nothing near is a corner; where something is, the corner's own class
        // by how far it turns: gently (an octagon's), squarely, or to a point. And a corner
        // between two curves is never square: a curve still, or a point. Between two curves
        // means **both**, at the corner: no straight side near it, and the two sides of its
        // window bend. A corner with neither evidence is read by its angle, so a missing side
        // never makes a right angle a point.
        let between_curves = (1.0 - sided[at_most]) * curving[at_most];
        let soft = smoothstep(SOFT_DEG.0, SOFT_DEG.1, most);
        let round = 1.0 - corner * ((1.0 - between_curves) * smoothstep(ROUND_DEG.0, ROUND_DEG.1, most) + between_curves * soft);
        let pointed = corner * ((1.0 - between_curves) * smoothstep(POINTED_DEG.0, POINTED_DEG.1, most) + between_curves * soft);
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
            ("a box rounded by two fifths of its shorter side", |x, y| rounded(x, y, 0.4, 0.3, 0.24), Kind::Round, 0.95),
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
            ("a box with its corners cut by a third of its shorter side", |x, y| x.abs() < 0.4 && y.abs() < 0.3 && x.abs() + y.abs() < 0.7 - 0.2, Kind::Round, 0.9),
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
            // (A quarter at the most, and that at one angle in a few: an oval of four to one
            // turned 18 degrees, where its ends are on the line and a little of each reads as a
            // corner with a side. Its fourth review: a fifth.)
            assert!(hi[1] < 0.3, "an oval of {long} to 1 is up to {:.2} square", hi[1]);
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
        for small in [24u32, 64, 96, 128, 200] {
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
        // The same shape on half the pixels (still over the size it reads at) says the same
        // (the sized shapes of `tests/shape_families.rs` hold this at every size from 256 up),
        // and under [`MIN_SIZE`] across it is not read.
        let (big_disc, big_box): (Inside, Inside) = (|x, y| x.hypot(y) < 0.44, |x, y| x.abs() < 0.4 && y.abs() < 0.3);
        for (name, inside, kind) in [("a disc", big_disc, Kind::Round), ("a box", big_box, Kind::Square)] {
            let (full, half) = (of_mask(&mask(PIXELS, 20.0, inside)).unwrap(), of_mask(&mask(PIXELS / 2, 20.0, inside)).unwrap());
            assert!((full.share(kind) - half.share(kind)).abs() < 0.1 && half.leading() == kind, "{name}: {full:?} and on half the pixels {half:?}");
        }
        assert_eq!(of_mask(&mask(PIXELS / 2, 0.0, |x, y| x.hypot(y) < 0.36)), None, "a disc 184 pixels across");
        assert!(of_mask(&mask(PIXELS / 2, 0.0, |x, y| x.hypot(y) < 0.38)).is_some(), "a disc 195 pixels across");
        let j = of_mask(&mask(PIXELS, 0.0, disc)).unwrap().to_json();
        assert_eq!(j.get("leading").and_then(Json::as_str), Some("round"));
        assert_eq!(Kind::parse("pointed"), Some(Kind::Pointed));
        assert_eq!(Kind::parse("spiky"), None);
    }
}
