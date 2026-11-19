//! What an asset is built of, as shape language (docs/design/FORM.md, "Shape language"): the
//! second measure, beside the silhouette's ([`crate::shape`]).
//!
//! A recipe says what each of its shapes was made as: `specula-model` records, for every
//! shape it names, that shape's own outlines along its own axes and the surface it has in the
//! model ([`specula_model::Made`]). Here each outline is read by the same measure a silhouette
//! is read by (drawn on [`PIXELS`] pixels), a shape's outlines are weighed as the recipe
//! weighed them, and the shapes are weighed **by their surface**.
//!
//! What it says that a silhouette cannot: **which part is what** (the largest are named, each
//! with its share of the surface and what it reads as, so a failed check says what to change),
//! and it is not moved by how the parts are arranged or seen: a ball built of forty small
//! boxes is boxes here and a ball in silhouette; a crate under a round lid is half of each
//! here and by the angle there.
//!
//! What it cannot say: anything of an imported mesh, a terrain or a building, or of the wall a
//! cut makes (no record speaks for them: [`BuiltOf::read`] is the share of the surface that was
//! read, and an outline the measure cannot read, or past [`MAX_READS`], counts as not read);
//! what a part's operations did to its form (a taper, a bend); how the parts meet. A thin
//! outline is read at [`MAX_ASPECT`] to one at the most ([`at_most_long`]).

use crate::image::Mask;
use crate::shape::{Kind, ShapeLanguage};
use specula_core::Json;
use specula_model::Made;

/// The side, in pixels, an outline is drawn on to be read (over [`crate::shape::MIN_SIZE`]
/// with room to spare; a shape's own outline is clean, and reads the same on fewer pixels than
/// a rendered silhouette needs).
pub const PIXELS: u32 = 384;
/// Shapes named in the answer: the ones with the most surface.
pub const MAX_PARTS: usize = 8;

/// One shape of the recipe, read.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltPart {
    /// Where in the recipe.
    pub path: String,
    /// What it was made as (`box`, `sphere`, `sdf`, ...).
    pub what: &'static str,
    /// Its share of the surface that was read.
    pub share: f32,
    pub round: f32,
    pub square: f32,
    pub pointed: f32,
}

impl BuiltPart {
    pub fn leading(&self) -> Kind {
        leading(self.round, self.square, self.pointed)
    }
}

/// What an asset is built of.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltOf {
    pub round: f32,
    pub square: f32,
    pub pointed: f32,
    /// The share of the asset's surface these speak for (the rest: what no record speaks for,
    /// outlines that could not be read or were past [`MAX_READS`], each with its share of its
    /// shape's surface, and shapes past the ones the model kept).
    pub read: f32,
    /// How many shapes were read.
    pub shapes: usize,
    /// The shapes with the most surface, the largest first (at most [`MAX_PARTS`]).
    pub parts: Vec<BuiltPart>,
}

fn leading(round: f32, square: f32, pointed: f32) -> Kind {
    // (Ties go to the first, as the silhouette's do.)
    if round >= square && round >= pointed {
        Kind::Round
    } else if square >= pointed {
        Kind::Square
    } else {
        Kind::Pointed
    }
}

impl BuiltOf {
    pub fn share(&self, kind: Kind) -> f32 {
        match kind {
            Kind::Round => self.round,
            Kind::Square => self.square,
            Kind::Pointed => self.pointed,
        }
    }

    pub fn leading(&self) -> Kind {
        leading(self.round, self.square, self.pointed)
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f32| Json::from((f64::from(x) * 1000.0).round() / 1000.0);
        Json::obj([
            ("round", r(self.round)),
            ("square", r(self.square)),
            ("pointed", r(self.pointed)),
            ("leading", Json::from(self.leading().name())),
            ("read_share", r(self.read)),
            ("shapes", Json::from(self.shapes)),
            (
                "parts",
                Json::arr(self.parts.iter().map(|p| {
                    Json::obj([
                        ("path", Json::from(p.path.as_str())),
                        ("shape", Json::from(p.what)),
                        ("surface_share", r(p.share)),
                        ("leading", Json::from(p.leading().name())),
                        ("round", r(p.round)),
                        ("square", r(p.square)),
                        ("pointed", r(p.pointed)),
                    ])
                })),
            ),
        ])
    }
}

/// A closed outline drawn on a mask of `side` pixels, fitted with a margin (even-odd, by
/// rows: the same pixels on every machine). `None` for an outline that encloses nothing.
pub fn drawn(points: &[[f32; 2]], side: u32) -> Option<Mask> {
    if points.len() < 3 || side < 16 || points.iter().any(|p| !(p[0].is_finite() && p[1].is_finite())) {
        return None;
    }
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    // (By its greatest width, so that it fits however it is turned: its corner to corner.)
    let across = ((hi[0] - lo[0]) * (hi[0] - lo[0]) + (hi[1] - lo[1]) * (hi[1] - lo[1])).sqrt();
    if across.is_nan() || across <= 0.0 {
        return None;
    }
    let scale = 0.92 * side as f32 / across;
    let mid = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])];
    let at = |p: [f32; 2]| [(p[0] - mid[0]) * scale + 0.5 * side as f32, (mid[1] - p[1]) * scale + 0.5 * side as f32];
    let ring: Vec<[f32; 2]> = points.iter().map(|p| at(*p)).collect();
    let mut m = Mask::empty(side, side);
    let mut crossings: Vec<f32> = Vec::new();
    for y in 0..side {
        let level = y as f32 + 0.5;
        crossings.clear();
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a[1] <= level) != (b[1] <= level) {
                crossings.push(a[0] + (level - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        crossings.sort_by(f32::total_cmp);
        for pair in crossings.as_chunks::<2>().0 {
            let (from, to) = ((pair[0] - 0.5).ceil().max(0.0) as u32, ((pair[1] - 0.5).floor().min(side as f32 - 1.0)) as i64);
            for x in from as i64..=to {
                m.on[(y * side) as usize + x as usize] = true;
            }
        }
    }
    (m.area() > 0).then_some(m)
}

/// How much longer than wide an outline is read at, at the most ([`at_most_long`]).
pub const MAX_ASPECT: f32 = 6.0;
/// Outlines read for one asset, at the most (identical ones are read once): what is past them
/// counts as not read. The largest record a model keeps, every outline different, read in
/// 0.27 s on the reference laptop while other builds ran (an outline 8 ms there).
pub const MAX_READS: usize = 32;

/// An outline longer than [`MAX_ASPECT`] times its width, shortened to that. Drawn at its own
/// length a thin one is too thin for the measure to read (under [`crate::shape::MIN_THICK`]
/// pixels: about forty to one at [`PIXELS`]) or is read wrongly near that, and differently at
/// another size. A recorded outline is one part's:
/// - **straight along its middle** (no corner of it between its two ends, each as long as the
///   outline is wide: a rod's rectangle, a capsule's stadium, a spike's triangle, a round
///   cone's side), the length is taken out of the middle and its ends kept as they are, so a
///   rod is read as a rectangle and a capsule as a stadium however long they are;
/// - **curved along it** (an oval's, a profile's), it is pressed evenly from end to end, so an
///   oval of a thousand to one is read as one of six: pointed, as the measure reads ovals of
///   over about four to one.
pub fn at_most_long(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let w = [hi[0] - lo[0], hi[1] - lo[1]];
    let (long, short) = if w[0] >= w[1] { (0, 1) } else { (1, 0) };
    if !(w[short] > 0.0 && w[long].is_finite()) || w[long] <= MAX_ASPECT * w[short] {
        return points.to_vec();
    }
    let end = w[short];
    let middle = w[long] - 2.0 * end;
    // (An outline with corners along its middle, an oval's or a profile's, is curved there:
    // it is pressed evenly from end to end, and an oval of a thousand to one is read as one of
    // six.)
    if points.iter().any(|p| p[long] - lo[long] > end && p[long] - lo[long] < w[long] - end) {
        let k = MAX_ASPECT * w[short] / w[long];
        return points
            .iter()
            .map(|p| {
                let mut q = *p;
                q[long] = lo[long] + (p[long] - lo[long]) * k;
                q
            })
            .collect();
    }
    let k = (MAX_ASPECT - 2.0) * w[short] / middle;
    points
        .iter()
        .map(|p| {
            let u = p[long] - lo[long];
            let u = if u <= end {
                u
            } else if u >= w[long] - end {
                u - middle * (1.0 - k)
            } else {
                end + (u - end) * k
            };
            let mut q = *p;
            q[long] = lo[long] + u;
            q
        })
        .collect()
}

/// The shape language of one outline, drawn and read (at most [`MAX_ASPECT`] times as long as
/// wide: [`at_most_long`]).
pub fn of_outline(points: &[[f32; 2]]) -> Option<ShapeLanguage> {
    crate::shape::of_mask(&drawn(&at_most_long(points), PIXELS)?)
}

/// What the shapes a recipe recorded come to: each shape's outlines read and weighed as the
/// recipe weighed them, the shapes weighed by their surface. `rest_m2` is the surface the
/// model said nothing speaks for. **What is not read counts as not read** ([`BuiltOf::read`]):
/// an outline the measure cannot read takes its share of its shape's surface with it, and so
/// do the outlines past [`MAX_READS`] (the largest shapes are read first). `None` when nothing
/// could be read.
pub fn built_of(made: &[Made], rest_m2: f32) -> Option<BuiltOf> {
    // (One reading for outlines that are the same: a recipe of twenty boxes has few.)
    let mut seen: Vec<(&[[f32; 2]], Option<ShapeLanguage>)> = Vec::new();
    let mut parts: Vec<BuiltPart> = Vec::new();
    let (mut total, mut read) = (rest_m2.max(0.0), 0.0f32);
    let mut sum = [0.0f32; 3];
    // (The largest first, so that a budget cuts the smallest.)
    let mut order: Vec<&Made> = made.iter().filter(|m| m.area_m2.is_finite() && m.area_m2 > 0.0).collect();
    order.sort_by(|a, b| b.area_m2.total_cmp(&a.area_m2).then_with(|| a.path.cmp(&b.path)));
    for m in order {
        total += m.area_m2;
        let (mut shares, mut weight, mut all) = ([0.0f32; 3], 0.0f32, 0.0f32);
        for (points, w) in &m.views {
            if !(w.is_finite() && *w > 0.0) {
                continue;
            }
            all += w;
            let known = seen.iter().find(|(p, _)| *p == points.as_slice()).map(|(_, s)| *s);
            let s = match known {
                Some(s) => s,
                // (An outline of no points: one the model did not keep. Not read.)
                None if points.len() < 3 => None,
                None if seen.len() < MAX_READS => {
                    let s = of_outline(points);
                    seen.push((points.as_slice(), s));
                    s
                }
                None => None,
            };
            if let Some(s) = s {
                shares = [shares[0] + s.round * w, shares[1] + s.square * w, shares[2] + s.pointed * w];
                weight += w;
            }
        }
        if weight <= 0.0 || all <= 0.0 {
            continue;
        }
        let shares = shares.map(|x| x / weight);
        // (The share of its surface its read outlines speak for.)
        let spoken = m.area_m2 * weight / all;
        read += spoken;
        sum = [sum[0] + shares[0] * spoken, sum[1] + shares[1] * spoken, sum[2] + shares[2] * spoken];
        parts.push(BuiltPart { path: m.path.clone(), what: m.what, share: spoken, round: shares[0], square: shares[1], pointed: shares[2] });
    }
    if read <= 0.0 {
        return None;
    }
    let shapes = parts.len();
    parts.sort_by(|a, b| b.share.total_cmp(&a.share).then_with(|| a.path.cmp(&b.path)));
    parts.truncate(MAX_PARTS);
    for p in &mut parts {
        p.share /= read;
    }
    Some(BuiltOf { round: sum[0] / read, square: sum[1] / read, pointed: sum[2] / read, read: read / total, shapes, parts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_model::made::{circle, mirrored, ngon, rect, rounded, stadium, trapezoid};

    fn made(path: &str, what: &'static str, area_m2: f32, views: Vec<(Vec<[f32; 2]>, f32)>) -> Made {
        Made { path: path.into(), what, area_m2, views }
    }

    /// An outline drawn is the outline: its area in pixels is its area by the scale it was
    /// fitted with, a line is nothing, and the same outline draws the same mask.
    #[test]
    fn an_outline_is_drawn_as_it_is() {
        let side = 256u32;
        let m = drawn(&rect(2.0, 1.0, 0.0), side).unwrap();
        let scale = 0.92 * side as f32 / 5.0f32.sqrt();
        let want = 2.0 * scale * scale;
        assert!((m.area() as f32 - want).abs() < 0.02 * want, "{} pixels, {want} wanted", m.area());
        let disc = drawn(&circle(1.0), side).unwrap();
        let want = std::f32::consts::PI * (0.92 * side as f32 / (2.0 * 2.0f32.sqrt())).powi(2);
        assert!((disc.area() as f32 - want).abs() < 0.02 * want, "{} pixels, {want} wanted", disc.area());
        assert!(drawn(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], side).is_none());
        assert!(drawn(&[[0.0, 0.0], [1.0, 0.0]], side).is_none());
        assert!(drawn(&[[0.0, 0.0], [f32::NAN, 0.0], [0.0, 1.0]], side).is_none());
        assert_eq!(drawn(&circle(3.0), side), drawn(&circle(3.0), side));
    }

    /// The shapes a recipe is built of read as what they are: a box is square, a ball, a
    /// capsule and a barrel-sided lathe round, a cone and a spike pointed; a cylinder is its
    /// round section and its square side, a third and two thirds.
    #[test]
    fn the_shapes_a_recipe_names_read_as_what_they_are() {
        let one = |views: Vec<(Vec<[f32; 2]>, f32)>| built_of(&[made("parts[0].shape", "x", 1.0, views)], 0.0).unwrap();
        let cube = one(vec![(rect(1.0, 1.0, 0.0), 1.0), (rect(1.0, 2.0, 0.0), 1.0), (rect(2.0, 1.0, 0.0), 1.0)]);
        assert!(cube.square > 0.95 && cube.leading() == Kind::Square && cube.read == 1.0, "{cube:?}");
        let ball = one(vec![(circle(1.0), 1.0)]);
        assert!(ball.round > 0.95, "{ball:?}");
        let pill = one(vec![(circle(0.5), 1.0), (stadium(3.0, 0.5), 2.0)]);
        assert!(pill.round > 0.95, "{pill:?}");
        let can = one(vec![(ngon(24, 0.5), 1.0), (trapezoid(0.5, 0.5, 2.0), 2.0)]);
        assert!((can.round - 1.0 / 3.0).abs() < 0.05 && (can.square - 2.0 / 3.0).abs() < 0.05, "a can: its round section and its square side: {can:?}");
        let cone = one(vec![(ngon(24, 0.5), 1.0), (trapezoid(0.5, 0.0, 2.0), 2.0)]);
        // (Its round section a third; from the side a triangle, whose tip is a point and whose
        // two foot corners, of 76 degrees here, are between square and pointed.)
        assert!(cone.pointed > 0.45 && (cone.round - 1.0 / 3.0).abs() < 0.05 && cone.leading() == Kind::Pointed, "{cone:?}");
        let barrel = one(vec![(circle(0.6), 1.0), (mirrored(&(0..=24).map(|i| { let t = i as f32 / 24.0; [0.4 + 0.25 * (1.0 - (2.0 * t - 1.0) * (2.0 * t - 1.0)), t] }).collect::<Vec<_>>()), 2.0)]);
        assert!(barrel.round > 0.5, "a barrel bulges: {barrel:?}");
        let soft = one(vec![(rounded(1.0, 1.0, 0.4), 1.0)]);
        assert!(soft.round > 0.9, "a box rounded by four tenths of its side: {soft:?}");
    }

    /// Shapes are weighed by their surface, the largest are named, and what nothing speaks
    /// for is said: a small ball on a large crate is a crate; a recipe nothing can be read
    /// of has no answer.
    #[test]
    fn shapes_are_weighed_by_their_surface_and_named() {
        let crate_ = made("parts[0].shape", "box", 6.0, vec![(rect(1.0, 1.0, 0.0), 1.0)]);
        let knob = made("parts[1].shape", "sphere", 0.5, vec![(circle(0.2), 1.0)]);
        let spike = made("parts[2].shape", "cone", 1.5, vec![(trapezoid(0.3, 0.0, 1.5), 1.0)]);
        let unread = made("parts[3].shape", "sweep", 1.0, vec![(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 1.0)]);
        let b = built_of(&[crate_.clone(), knob.clone(), spike.clone(), unread.clone()], 1.0).unwrap();
        assert!((b.read - 8.0 / 10.0).abs() < 1e-4, "8 of 10 square metres were read: {}", b.read);
        assert_eq!(b.shapes, 3);
        // (The crate's six of the eight square metres read are square, the knob's half round,
        // and the spike's one and a half are a point at its tip and nearly square at its foot.)
        assert!(b.square >= 6.0 / 8.0 - 0.01 && (b.round - 0.5 / 8.0).abs() < 0.01 && b.pointed > 0.1 && b.pointed <= 1.5 / 8.0 + 0.01, "{b:?}");
        assert_eq!(b.parts.iter().map(|p| (p.path.as_str(), p.leading())).collect::<Vec<_>>(), [("parts[0].shape", Kind::Square), ("parts[2].shape", Kind::Pointed), ("parts[1].shape", Kind::Round)]);
        assert!((b.parts[0].share - 0.75).abs() < 1e-4);
        let j = b.to_json();
        assert_eq!(j.get("leading").and_then(Json::as_str), Some("square"));
        assert_eq!(j.get("parts").and_then(Json::as_array).map(|a| a.len()), Some(3));
        assert_eq!(built_of(&[unread], 0.0), None);
        assert_eq!(built_of(&[], 3.0), None);
        // Twenty of the same box are read once and count twenty times.
        let many: Vec<Made> = (0..20).map(|i| made(&format!("parts[{i}].shape"), "box", 1.0, vec![(rect(1.0, 1.0, 0.0), 1.0)])).collect();
        let b = built_of(&many, 0.0).unwrap();
        assert!(b.square > 0.95 && b.shapes == 20 && b.parts.len() == MAX_PARTS, "{b:?}");
    }

    /// The reading of an outline at a side of `side` pixels, as [`of_outline`] reads it at its own.
    fn read_at(points: &[[f32; 2]], side: u32) -> Option<ShapeLanguage> {
        crate::shape::of_mask(&drawn(&at_most_long(points), side)?)
    }

    /// A thin outline is read as what it is, however thin: a rod's side is a rectangle and a
    /// capsule's a stadium at fifty, a hundred and a thousand times as long as wide, as at six,
    /// and at 512 pixels as at 384 (drawn at its own length, past about forty to one it was not
    /// read, and near that it was read as pointed). So a cylinder is a third round and two thirds
    /// square whether it is a coin, a can or a pole.
    #[test]
    fn a_thin_outline_is_read_as_what_it_is() {
        type Outline = Box<dyn Fn(f32) -> Vec<[f32; 2]>>;
        let kinds: Vec<(&str, Outline, Kind)> = vec![
            ("rectangle", Box::new(|a| rect(a, 1.0, 0.0)), Kind::Square),
            ("stadium", Box::new(|a| stadium(a, 0.5)), Kind::Round),
            ("rounded by four tenths", Box::new(|a| rounded(a, 1.0, 0.4)), Kind::Round),
            ("frustum", Box::new(|a| trapezoid(0.5, 0.3, a)), Kind::Square),
            ("spike", Box::new(|a| trapezoid(0.5, 0.0, a)), Kind::Pointed),
            ("oval", Box::new(|a| specula_model::made::ellipse(a, 1.0)), Kind::Pointed),
        ];
        for (name, outline, kind) in &kinds {
            for aspect in [6.0f32, 12.0, 50.0, 100.0, 1000.0] {
                for side in [PIXELS, 512] {
                    let s = read_at(&outline(aspect), side).unwrap_or_else(|| panic!("a {name} {aspect} to 1 is not read at {side}"));
                    assert!(s.share(*kind) > 0.6 && leading(s.round, s.square, s.pointed) == *kind, "a {name} {aspect} to 1 at {side}: {s:?}");
                }
            }
        }
        // Not long: unchanged.
        let short = rect(5.0, 1.0, 0.0);
        assert_eq!(at_most_long(&short), short);
        let long = at_most_long(&stadium(300.0, 0.5));
        let e = specula_model::made::half_extents(&long);
        assert!((e[0] / e[1] - MAX_ASPECT).abs() < 1e-3, "{e:?}");
        // A cylinder of any proportions.
        for (r, h) in [(1.0f32, 0.02f32), (1.0, 0.1), (0.5, 1.0), (0.03, 3.0)] {
            let b = built_of(&[made("parts[0].shape", "cylinder", 1.0, vec![(ngon(48, r), 1.0), (trapezoid(r, r, h), 2.0)])], 0.0).unwrap();
            assert!((b.round - 1.0 / 3.0).abs() < 0.05 && (b.square - 2.0 / 3.0).abs() < 0.05 && b.read == 1.0, "a cylinder of {r} by {h}: {b:?}");
        }
    }

    /// What is not read counts as not read: an outline the measure cannot read takes its share
    /// of its shape's surface out of what was read, and so do the outlines past [`MAX_READS`],
    /// the smallest shapes' first.
    #[test]
    fn what_is_not_read_counts_as_not_read() {
        let line = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let half = built_of(&[made("parts[0].shape", "box", 2.0, vec![(rect(1.0, 1.0, 0.0), 1.0), (line, 1.0)])], 0.0).unwrap();
        assert!((half.read - 0.5).abs() < 1e-4 && half.square > 0.95, "{half:?}");
        // An outline the model did not keep (no points, its weight kept) is not read either.
        let cut = built_of(&[made("parts[0].shape", "sdf", 2.0, vec![(rect(1.0, 1.0, 0.0), 1.0), (Vec::new(), 3.0)])], 0.0).unwrap();
        assert!((cut.read - 0.25).abs() < 1e-4, "{cut:?}");
        // More outlines than are read: the largest shapes are read, the rest is not.
        let many: Vec<Made> = (0..MAX_READS + 12).map(|i| made(&format!("parts[{i}].shape"), "box", 100.0 - i as f32, vec![(rect(1.0 + 0.01 * i as f32, 1.0, 0.0), 1.0)])).collect();
        let b = built_of(&many, 0.0).unwrap();
        let read: f32 = (0..MAX_READS).map(|i| 100.0 - i as f32).sum::<f32>();
        let all: f32 = (0..MAX_READS + 12).map(|i| 100.0 - i as f32).sum::<f32>();
        assert!(b.shapes == MAX_READS && (b.read - read / all).abs() < 1e-4, "{} shapes, {} read", b.shapes, b.read);
        // An outline twenty shapes share is read once.
        let same: Vec<Made> = (0..MAX_READS + 12).map(|i| made(&format!("parts[{i}].shape"), "box", 1.0, vec![(rect(2.0, 1.0, 0.0), 1.0)])).collect();
        assert_eq!(built_of(&same, 0.0).unwrap().read, 1.0);
    }

    /// The cost has a ceiling: the largest record a model keeps (its most outlines, each of the
    /// most points, every one different) is read in tens of milliseconds to a few hundred, and
    /// what is past the budget is said as not read.
    #[test]
    fn reading_the_largest_record_is_bounded() {
        let views_each = specula_model::made::MAX_RECORD_VIEWS / specula_model::MAX_MADE;
        let record: Vec<Made> = (0..specula_model::MAX_MADE)
            .map(|i| {
                let views = (0..views_each)
                    .map(|j| {
                        let k = (i * views_each + j) as f32;
                        let ring: Vec<[f32; 2]> = (0..specula_model::made::MAX_VIEW_POINTS)
                            .map(|t| {
                                let (s, c) = specula_model::dmath::sin_cos_deg(360.0 * t as f64 / specula_model::made::MAX_VIEW_POINTS as f64);
                                let r = 1.0 + 0.2 * specula_model::dmath::sin_cos_deg(t as f64 * 17.0 * (1.0 + f64::from(k) * 0.001)).0 as f32;
                                [r * c as f32, (1.0 + k * 0.0001) * r * s as f32]
                            })
                            .collect();
                        (ring, 1.0)
                    })
                    .collect();
                made(&format!("parts[{i}].shape"), "sdf", 1.0, views)
            })
            .collect();
        let points: usize = record.iter().map(|m| m.views.iter().map(|v| v.0.len()).sum::<usize>()).sum();
        assert!(points * 8 <= 300_000, "the record is {} bytes of points", points * 8);
        let t = std::time::Instant::now();
        let b = built_of(&record, 0.0).unwrap();
        let took = t.elapsed();
        assert!(took < specula_core::speed_budget(std::time::Duration::from_millis(400)), "{took:?}");
        assert!(b.read < 0.2, "only what the budget read is read: {}", b.read);
    }
}
