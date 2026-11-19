//! Taste checks as data (docs/design/ASSET_STUDIO.md): what the engine measured on its stage
//! renders, against `[taste]` thresholds, reported like budget errors.
//!
//! - `silhouette_distinct`: the highest silhouette IoU with any sibling (thumbnail masks fitted
//!   to their own boxes, averaged over the angles).
//! - `thumb_solidity`: the share of the thumbnail silhouette that survives a one-pixel opening.
//! - `px_gameplay`, `px_phone`: the subject's height in pixels from the game camera.
//! - `value_contrast`, `value_contrast_worst`: how the outline separates from the ground around
//!   it (median lightness difference across the silhouette's edge, [`crate::image::Edge`]), in
//!   the gameplay shot and under the worst-case light.
//! - `flat_lighting`: the lightness spread inside the silhouette under the game light.
//! - `shape_language`: the share of the outline's turning that is of the kind the style asks
//!   for (round, square, pointed: [`crate::shape`]), the mean over the thumbnail angles.
//!   Checked only when `[taste] shape_language` is given.
//!
//! Scale against the reference figure and palette measures come from [`crate::check`].

use crate::image::{Edge, Lightness, Mask};
use crate::rules::StyleRules;
use crate::{Bound, Finding};
use specula_core::Json;

/// The taste checks `[taste.allow]` may exempt.
pub const CHECKS: [&str; 8] = ["silhouette_distinct", "thumb_solidity", "px_gameplay", "px_phone", "value_contrast", "value_contrast_worst", "flat_lighting", "shape_language"];

/// One shot's measures of a subject.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotMeasure {
    /// Height of the silhouette's box, in pixels of the shot's viewport.
    pub px_height: f32,
    pub subject: Lightness,
    /// The ring of background just outside the silhouette.
    pub ring: Lightness,
    /// Contrast along the outline.
    pub edge: Edge,
}

impl ShotMeasure {
    /// Value contrast: the median lightness difference across the outline.
    pub fn contrast(&self) -> f32 {
        self.edge.median
    }

    /// Mean lightness of the subject against the mean of its surround (a light or dark shape
    /// overall; reported, not checked).
    pub fn mean_contrast(&self) -> f32 {
        (self.subject.mean - self.ring.mean).abs()
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f32| Json::from((x as f64 * 1000.0).round() / 1000.0);
        Json::obj([
            ("px_height", r(self.px_height)),
            ("contrast", r(self.contrast())),
            ("edge_p25", r(self.edge.p25)),
            ("subject_l", r(self.subject.mean)),
            ("surround_l", r(self.ring.mean)),
            ("mean_contrast", r(self.mean_contrast())),
            ("spread", r(self.subject.spread())),
        ])
    }
}

/// What the engine measured about one subject for the taste checks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Measured {
    pub subject: String,
    /// Thumbnail silhouettes, fitted, one per angle (the same angles for every sibling).
    pub silhouettes: Vec<Mask>,
    /// The same silhouettes at [`crate::shape::PIXELS`], for the outline's shape language
    /// (a thumbnail's pixels are as large as what it measures). Empty: no shape is read.
    pub outlines: Vec<Mask>,
    pub gameplay: Option<ShotMeasure>,
    pub phone: Option<ShotMeasure>,
    pub worst: Option<ShotMeasure>,
    /// The subject under the game light (close framing), for flat lighting.
    pub lit: Option<Lightness>,
}

impl Measured {
    /// The subject's shape language, from its larger silhouettes (`None` without them: a
    /// thumbnail is under the size the measure reads at). Read anew at each call (some
    /// milliseconds a silhouette of 512 pixels: a reading kept inside went stale when a copy's
    /// outlines were changed, and made two equal measures unequal: third review).
    pub fn shape(&self) -> Option<crate::shape::ShapeLanguage> {
        crate::shape::of_masks(&self.outlines)
    }
}

#[allow(clippy::too_many_arguments)]
fn finding(rules: &StyleRules, check: &'static str, subject: &str, value: f32, limit: f32, bound: Bound, unit: &'static str, key: &str, message: String, fix: &str) -> Finding {
    let mut f = Finding::measure(check, subject, value as f64, limit as f64, bound, unit, format!("{} taste.{key}", rules.source), message, fix.to_string()).with_margin();
    if !f.passed && rules.taste.allows(subject, check) {
        f.exempt = true;
    }
    f
}

/// The mean IoU per pair of subjects (rows and columns in input order; 1 on the diagonal).
pub fn iou_matrix(subjects: &[Measured]) -> Vec<Vec<f32>> {
    let n = subjects.len();
    let mut m = vec![vec![1.0f32; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (&subjects[i].silhouettes, &subjects[j].silhouettes);
            let k = a.len().min(b.len());
            let v = if k == 0 { 0.0 } else { a.iter().zip(b).map(|(x, y)| x.iou(y)).sum::<f32>() / k as f32 };
            m[i][j] = v;
            m[j][i] = v;
        }
    }
    m
}

/// The per-subject taste checks (everything but distinctness).
pub fn check_subject(m: &Measured, rules: &StyleRules) -> Vec<Finding> {
    let t = &rules.taste;
    let s = m.subject.as_str();
    let mut out = Vec::new();
    if let Some(thumb) = m.silhouettes.last() {
        let v = thumb.solidity();
        out.push(finding(rules, "thumb_solidity", s, v, t.thumb_min_solidity, Bound::Min, "share", "thumb_min_solidity", format!("only {:.0}% of the {}-px silhouette is thicker than a few pixels: thin parts vanish at thumbnail size", v * 100.0, t.thumb_px), "thicken the thin parts or drop them; read it as a silhouette at 64 px first"));
    }
    if let (Some(kind), Some(shape)) = (t.shape_language, m.shape()) {
        let pc = |x: f32| (x * 100.0).round();
        let fix = match kind {
            crate::shape::Kind::Round => "round each part's corners with a radius of a quarter of that part's thickness or more (a roundness, a larger corner or round), and build it from balls, capsules and chains rather than boxes; a thin stalk, string or stick on a round body changes nothing, and an oval over about four times as long as wide comes to points",
            crate::shape::Kind::Square => "square it off: flat sides meeting at right angles, with rounded corners kept under a tenth of the part's thickness and corners cut off flat under a seventh of it (wider, the part goes round by degrees), fewer curves and spikes in the outline",
            crate::shape::Kind::Pointed => "give the outline points: tapers that end in a tip, wedges, spikes and blades whose tips are under 65 degrees and whose foot is thick (a spike counts by the thickness of its foot, a hair by its own: nothing), in place of rounded and squared ends; a notch cut into it adds nothing, only what sticks out is read",
        };
        out.push(finding(
            rules,
            "shape_language",
            s,
            shape.share(kind),
            t.shape_min_share,
            Bound::Min,
            "share",
            "shape_min_share",
            format!("{:.0}% of its outline reads as {} (round {:.0}%, square {:.0}%, pointed {:.0}%, by how it turns outward; a point counts double), where the style asks for a {} shape", pc(shape.share(kind)), kind.name(), pc(shape.round), pc(shape.square), pc(shape.pointed), kind.name()),
            fix,
        ));
    }
    if let Some(g) = &m.gameplay {
        out.push(finding(rules, "px_gameplay", s, g.px_height, t.min_px_gameplay, Bound::Min, "px", "min_px_gameplay", format!("{:.0} px tall from the game camera", g.px_height), "scale it up (gameplay pieces are oversized on purpose) or give it a taller silhouette"));
        out.push(finding(rules, "value_contrast", s, g.contrast(), t.value_contrast_min, Bound::Min, "lightness", "value_contrast_min", format!("its outline differs from the ground around it by {:.3} lightness (median; weakest quarter {:.3}) in the gameplay shot", g.contrast(), g.edge.p25), "separate it from the ground: lighter tops or chamfer highlights, or a darker base"));
    }
    if let Some(p) = &m.phone {
        out.push(finding(rules, "px_phone", s, p.px_height, t.min_px_phone, Bound::Min, "px", "min_px_phone", format!("{:.0} px tall on a phone-sized screen", p.px_height), "a bigger, simpler silhouette: on a phone this is all of it a player sees"));
    }
    if let Some(w) = &m.worst {
        out.push(finding(rules, "value_contrast_worst", s, w.contrast(), t.value_contrast_worst_min, Bound::Min, "lightness", "value_contrast_worst_min", format!("its outline differs from the ground by {:.3} lightness (median; weakest quarter {:.3}) when backlit", w.contrast(), w.edge.p25), "it disappears when backlit: a lighter body colour, a rim of highlight colour, or a glow with gameplay meaning"));
    }
    if let Some(l) = &m.lit {
        out.push(finding(rules, "flat_lighting", s, l.spread(), t.flat_min_range, Bound::Min, "lightness", "flat_min_range", format!("lightness spread {:.3} (10th {:.2} to 90th {:.2}): the form doesn't turn in the light", l.spread(), l.p10, l.p90), "bevel the edges, add a gradient (darker at the foot) or break the big flat faces"));
    }
    out
}

/// Distinctness: per subject, its highest mean IoU with any sibling.
pub fn check_siblings(subjects: &[Measured], rules: &StyleRules) -> Vec<Finding> {
    if subjects.len() < 2 {
        return Vec::new();
    }
    let m = iou_matrix(subjects);
    let t = &rules.taste;
    let mut out = Vec::new();
    for (i, s) in subjects.iter().enumerate() {
        let Some((j, v)) = m[i].iter().enumerate().filter(|(j, _)| *j != i).max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(&a.0))) else { continue };
        let other = &subjects[j].subject;
        let f = finding(rules, "silhouette_distinct", &s.subject, *v, t.silhouette_max_iou, Bound::Max, "iou", "silhouette_max_iou", format!("its silhouette overlaps {other}'s by {:.0}% (mean over {} angles)", v * 100.0, s.silhouettes.len()), "change the outline, not the detail: a different top, width or lean")
            .with_detail(Json::obj([("closest", Json::from(other.as_str()))]));
        out.push(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> StyleRules {
        StyleRules::parse("[taste]\nallow = { \"fence*\" = [\"thumb_solidity\"] }\n", "style.toml", &|_| None).unwrap()
    }

    fn rect(r: [u32; 4]) -> Mask {
        let mut m = Mask::empty(64, 64);
        for y in r[1]..r[1] + r[3] {
            for x in r[0]..r[0] + r[2] {
                m.on[(y * 64 + x) as usize] = true;
            }
        }
        m.fitted(64)
    }

    #[test]
    fn alike_siblings_fail_and_distinct_ones_pass() {
        let tall = Measured { subject: "a".into(), silhouettes: vec![rect([10, 0, 20, 60])], ..Default::default() };
        let tall2 = Measured { subject: "b".into(), silhouettes: vec![rect([12, 4, 19, 58])], ..Default::default() };
        let wide = Measured { subject: "c".into(), silhouettes: vec![rect([0, 40, 64, 16])], ..Default::default() };
        let f = check_siblings(&[tall.clone(), tall2, wide.clone()], &rules());
        assert!(!f[0].passed && !f[1].passed && f[2].passed, "{f:?}");
        assert_eq!(f[0].detail.as_ref().and_then(|d| d.get("closest")).and_then(Json::as_str), Some("b"));
        assert!(check_siblings(&[tall, wide], &rules()).iter().all(|f| f.passed));
        assert!(check_siblings(&[], &rules()).is_empty());
    }

    #[test]
    fn subject_checks_and_exemptions() {
        let flat = Lightness { count: 100, mean: 0.5, p10: 0.49, p50: 0.5, p90: 0.51 };
        let ring = Lightness { count: 50, mean: 0.51, ..flat };
        let edge = crate::image::Edge { count: 40, median: 0.01, p25: 0.0 };
        let mut comb = Mask::empty(64, 64);
        for x in (2..62).step_by(5) {
            for y in 0..64 {
                comb.on[y * 64 + x] = true;
            }
        }
        let m = Measured {
            subject: "fence_a".into(),
            silhouettes: vec![comb],
            outlines: Vec::new(),
            gameplay: Some(ShotMeasure { px_height: 9.0, subject: flat, ring, edge }),
            phone: Some(ShotMeasure { px_height: 4.0, subject: flat, ring, edge }),
            worst: Some(ShotMeasure { px_height: 30.0, subject: flat, ring, edge }),
            lit: Some(flat),
        };
        let f = check_subject(&m, &rules());
        let failed: Vec<&str> = f.iter().filter(|x| x.is_error()).map(|x| x.check).collect();
        assert_eq!(failed, vec!["px_gameplay", "value_contrast", "px_phone", "value_contrast_worst", "flat_lighting"]);
        assert!(f.iter().any(|x| x.check == "thumb_solidity" && x.exempt));
        // (No style asked for a shape: none is checked.)
        assert!(!f.iter().any(|x| x.check == "shape_language"));
    }

    /// A style that asks for a shape language holds an outline to it: a box fails "round" and
    /// passes "square" with the shares in its message; an exemption exempts; a silhouette with
    /// no shape (empty) is not checked.
    #[test]
    fn a_style_can_ask_for_a_shape() {
        // (A box on a silhouette of the size shape language is measured at.)
        let side = crate::shape::PIXELS;
        let mut big = Mask::empty(side, side);
        for y in side / 4..side * 3 / 4 {
            for x in side / 8..side * 7 / 8 {
                big.on[(y * side + x) as usize] = true;
            }
        }
        let boxy = Measured { subject: "crate_a".into(), silhouettes: vec![rect([8, 14, 48, 36])], outlines: vec![big], ..Measured::default() };
        // (The larger silhouettes are the ones read; a thumbnail alone is under the size the
        // measure reads at, and says nothing rather than something wrong: its second review.)
        assert!(boxy.shape().is_some_and(|s| s.square > 0.99));
        assert_eq!(Measured { outlines: Vec::new(), ..boxy.clone() }.shape(), None);
        // (A copy with other outlines is read by its own, and reading changes nothing that two
        // measures are compared by: its third review.)
        let mut disc = Mask::empty(side, side);
        for y in 0..side {
            for x in 0..side {
                let (dx, dy) = (x as f32 - side as f32 / 2.0, y as f32 - side as f32 / 2.0);
                disc.on[(y * side + x) as usize] = dx.hypot(dy) < side as f32 * 0.4;
            }
        }
        let other = Measured { outlines: vec![disc], ..boxy.clone() };
        assert!(other.shape().is_some_and(|s| s.round > 0.95) && boxy == boxy.clone());
        let of = |toml: &str, m: &Measured| check_subject(m, &StyleRules::parse(toml, "style.toml", &|_| None).unwrap()).into_iter().find(|f| f.check == "shape_language");
        let f = of("[taste]\nshape_language = \"round\"\n", &boxy).unwrap();
        assert!(f.is_error() && f.value < 0.1 && f.limit == 0.5 && f.message.contains("square 100%") && f.fix.contains("round each part's corners"), "{f:?}");
        let f = of("[taste]\nshape_language = \"square\"\nshape_min_share = 0.8\n", &boxy).unwrap();
        assert!(f.passed && f.value > 0.9 && (f.limit - 0.8).abs() < 1e-6, "{f:?}");
        let f = of("[taste]\nshape_language = \"pointed\"\nallow = { \"crate*\" = [\"shape_language\"] }\n", &boxy).unwrap();
        assert!(!f.passed && f.exempt && !f.is_error(), "{f:?}");
        let nothing = Measured { subject: "ghost".into(), silhouettes: vec![Mask::empty(64, 64)], ..Measured::default() };
        assert!(of("[taste]\nshape_language = \"round\"\n", &nothing).is_none());

        // And a failed shape reaches the critique, with what to do (its first review: no pass
        // owned the check, so the critique said "ok").
        let rules = StyleRules::parse("[taste]\nshape_language = \"round\"\n", "style.toml", &|_| None).unwrap();
        let found = check_subject(&boxy, &rules);
        let refs: Vec<&crate::Finding> = found.iter().collect();
        let said = crate::critique::critique("crate_a", None, &refs, &rules).to_string();
        assert!(said.contains("shape_language") && said.contains("round each part's corners") && !said.contains("\"verdict\":\"ok\"}],\"next\":[]"), "{said}");
    }

    /// Every taste check and every form check belongs to a pass of the critique: a failure
    /// no pass owned never reached the critique.
    #[test]
    fn every_check_has_a_pass() {
        for check in CHECKS.iter().chain(&crate::form::CHECKS) {
            assert!(crate::critique::PASSES.iter().any(|(_, owned)| owned.contains(check)), "{check} is in no pass of the critique");
        }
    }
}
