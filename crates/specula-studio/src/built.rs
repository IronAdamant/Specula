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
//! What it cannot say: anything of an imported mesh or a generated part (no recipe speaks for
//! them: [`BuiltOf::read`] is the share of the surface that was read); what a part's
//! operations did to its form (a taper, a bend); how the parts meet.

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
    /// The share of the asset's surface these speak for (the rest: generated parts, shapes
    /// whose outlines could not be read, shapes past the ones the model kept).
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

/// The shape language of one outline, drawn and read.
pub fn of_outline(points: &[[f32; 2]]) -> Option<ShapeLanguage> {
    crate::shape::of_mask(&drawn(points, PIXELS)?)
}

/// What the shapes a recipe recorded come to: each shape's outlines read and weighed as the
/// recipe weighed them, the shapes weighed by their surface. `rest_m2` is the surface the
/// model said nothing speaks for. `None` when nothing could be read.
pub fn built_of(made: &[Made], rest_m2: f32) -> Option<BuiltOf> {
    // (One reading for outlines that are the same: a recipe of twenty boxes has few.)
    fn outline_read<'a>(seen: &mut Vec<(&'a [[f32; 2]], Option<ShapeLanguage>)>, points: &'a [[f32; 2]]) -> Option<ShapeLanguage> {
        if let Some((_, s)) = seen.iter().find(|(p, _)| *p == points) {
            return *s;
        }
        let s = of_outline(points);
        seen.push((points, s));
        s
    }
    let mut seen = Vec::new();
    let mut parts: Vec<BuiltPart> = Vec::new();
    let (mut total, mut read) = (rest_m2.max(0.0), 0.0f32);
    let mut sum = [0.0f32; 3];
    for m in made {
        if !(m.area_m2.is_finite() && m.area_m2 > 0.0) {
            continue;
        }
        total += m.area_m2;
        let (mut shares, mut weight) = ([0.0f32; 3], 0.0f32);
        for (points, w) in &m.views {
            if let Some(s) = outline_read(&mut seen, points) {
                shares = [shares[0] + s.round * w, shares[1] + s.square * w, shares[2] + s.pointed * w];
                weight += w;
            }
        }
        if weight <= 0.0 {
            continue;
        }
        let shares = shares.map(|x| x / weight);
        read += m.area_m2;
        sum = [sum[0] + shares[0] * m.area_m2, sum[1] + shares[1] * m.area_m2, sum[2] + shares[2] * m.area_m2];
        parts.push(BuiltPart { path: m.path.clone(), what: m.what, share: m.area_m2, round: shares[0], square: shares[1], pointed: shares[2] });
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
}
