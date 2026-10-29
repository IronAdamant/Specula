//! Reference boards: images the human likes, measured, and the measures' spread as **targets**
//! a scene is checked against (reported like the studio's taste checks: value, target range,
//! the rule it comes from, a revise hint).
//!
//! Measures (every image is shrunk to at most [`MEASURE_SIDE`] px first, so numbers don't depend
//! on resolution): a 16-bin histogram of Oklab lightness with its mean and percentiles;
//! Oklch chroma (mean, p90); a 6-colour palette (seeded k-means in Oklab); contrast (p90 - p10,
//! and RMS); edge density (share of pixels whose Sobel lightness gradient passes
//! [`EDGE_THRESHOLD`]); shadow softness (at strong edges, the width in pixels of the lightness
//! step: step height over gradient, the median, at the measuring size).

use crate::colour::{self, Rgb};
use crate::{r3, r4, valid_name, Result, TasteError};
use specula_core::rng::Rng;
use specula_core::Json;

pub const FORMAT: &str = "specula.taste.board.v1";
pub const MEASURE_SIDE: u32 = 256;
pub const MAX_IMAGES: usize = 32;
pub const MAX_IMAGE_PIXELS: u64 = 4096 * 4096;
pub const EDGE_THRESHOLD: f64 = 0.1;
const STRONG_EDGE: f64 = 0.15;
const PALETTE: usize = 6;
const KMEANS_SAMPLES: usize = 4096;

/// What one image measures.
#[derive(Clone, Debug, PartialEq)]
pub struct Measures {
    pub histogram: [f64; 16],
    pub value_mean: f64,
    pub value_p10: f64,
    pub value_p50: f64,
    pub value_p90: f64,
    pub chroma_mean: f64,
    pub chroma_p90: f64,
    /// (colour, share of pixels), most common first.
    pub palette: Vec<(Rgb, f64)>,
    pub contrast: f64,
    pub rms_contrast: f64,
    pub edge_density: f64,
    pub shadow_softness: f64,
}

/// The scalar measures by name, with their tolerance (how far outside the board's range still
/// passes) and what they mean for hints.
pub const SCALARS: [(&str, f64, &str, &str); 9] = [
    ("value_mean", 0.04, "darker", "lighter"),
    ("value_p10", 0.04, "deeper shadows", "lifted shadows"),
    ("value_p50", 0.04, "darker", "lighter"),
    ("value_p90", 0.04, "dimmer highlights", "brighter highlights"),
    ("chroma_mean", 0.015, "more muted", "more colourful"),
    ("chroma_p90", 0.02, "less saturated accents", "more saturated accents"),
    ("contrast", 0.04, "flatter", "punchier"),
    ("edge_density", 0.03, "simpler (fewer edges)", "busier (more edges)"),
    ("shadow_softness", 0.75, "crisper shadows", "softer shadows"),
];

impl Measures {
    pub fn scalar(&self, name: &str) -> f64 {
        match name {
            "value_mean" => self.value_mean,
            "value_p10" => self.value_p10,
            "value_p50" => self.value_p50,
            "value_p90" => self.value_p90,
            "chroma_mean" => self.chroma_mean,
            "chroma_p90" => self.chroma_p90,
            "contrast" => self.contrast,
            "edge_density" => self.edge_density,
            "shadow_softness" => self.shadow_softness,
            _ => f64::NAN,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([("histogram", Json::arr(self.histogram.iter().map(|v| r4(*v))))]);
        for (n, ..) in SCALARS {
            j.set(n, r4(self.scalar(n)));
        }
        j.set("rms_contrast", r4(self.rms_contrast));
        j.set("palette", Json::arr(self.palette.iter().map(|(c, w)| Json::obj([("color", Json::from(colour::hex(*c))), ("share", r3(*w))]))));
        j
    }

    pub fn from_json(j: &Json) -> Option<Measures> {
        let f = |k: &str| j.get(k).and_then(Json::as_f64);
        let h = j.get("histogram")?.as_array()?;
        let mut histogram = [0.0; 16];
        for (i, v) in h.iter().take(16).enumerate() {
            histogram[i] = v.as_f64()?;
        }
        let palette = j.get("palette")?.as_array()?.iter().filter_map(|p| Some((colour::parse_hex(p.get("color")?.as_str()?)?, p.get("share")?.as_f64()?))).collect();
        Some(Measures {
            histogram,
            value_mean: f("value_mean")?,
            value_p10: f("value_p10")?,
            value_p50: f("value_p50")?,
            value_p90: f("value_p90")?,
            chroma_mean: f("chroma_mean")?,
            chroma_p90: f("chroma_p90")?,
            palette,
            contrast: f("contrast")?,
            rms_contrast: f("rms_contrast").unwrap_or(0.0),
            edge_density: f("edge_density")?,
            shadow_softness: f("shadow_softness")?,
        })
    }
}

/// A box-filtered copy at most `side` px on its longest side.
pub fn shrink(rgba: &[u8], w: u32, h: u32, side: u32) -> (Vec<u8>, u32, u32) {
    let k = w.max(h).div_ceil(side.max(1)).max(1);
    if k == 1 {
        return (rgba.to_vec(), w, h);
    }
    let (nw, nh) = (w.div_ceil(k), h.div_ceil(k));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let (mut acc, mut n) = ([0u32; 4], 0u32);
            for sy in (y * k)..((y + 1) * k).min(h) {
                for sx in (x * k)..((x + 1) * k).min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += u32::from(rgba[i + c]);
                    }
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    (out, nw, nh)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * p) as usize;
    sorted[i.min(sorted.len() - 1)]
}

/// Measures an RGBA8 image (alpha is ignored: the scene as seen).
pub fn measure(rgba: &[u8], w: u32, h: u32) -> Result<Measures> {
    if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
        return Err(TasteError::new("bad_image", "the image has no pixels"));
    }
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
        return Err(TasteError::new("image_too_large", format!("{w}x{h}: images up to 4096 x 4096 are measured")));
    }
    let (px, w, h) = shrink(rgba, w, h, MEASURE_SIDE);
    let lin = colour::linear_table();
    let n = (w * h) as usize;
    let mut lab = Vec::with_capacity(n);
    for i in 0..n {
        let c = [lin[px[i * 4] as usize], lin[px[i * 4 + 1] as usize], lin[px[i * 4 + 2] as usize]];
        lab.push(colour::linear_to_oklab(c));
    }
    let l: Vec<f64> = lab.iter().map(|c| c[0].clamp(0.0, 1.0)).collect();
    let chroma: Vec<f64> = lab.iter().map(|c| (c[1] * c[1] + c[2] * c[2]).sqrt()).collect();
    let mut histogram = [0.0; 16];
    for v in &l {
        histogram[((v * 16.0) as usize).min(15)] += 1.0 / n as f64;
    }
    let mut ls = l.clone();
    ls.sort_by(f64::total_cmp);
    let mut cs = chroma.clone();
    cs.sort_by(f64::total_cmp);
    let mean = l.iter().sum::<f64>() / n as f64;
    let var = l.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64;
    // Sobel on lightness.
    let at = |x: i64, y: i64| l[(y.clamp(0, h as i64 - 1) * w as i64 + x.clamp(0, w as i64 - 1)) as usize];
    // Lightness change per pixel (a ramp of slope s measures s; a hard step of height H measures
    // H / 2), and its direction.
    let mut grad = Vec::with_capacity(n);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            grad.push(((gx * gx + gy * gy).sqrt() / 8.0, gx, gy));
        }
    }
    let edges = grad.iter().filter(|g| g.0 > EDGE_THRESHOLD / 2.0).count();
    // Shadow softness at the image's strongest edges (at least half its 99th-percentile
    // gradient): the step across the edge (6 px either side) over the gradient, which is how
    // many pixels the transition takes (2 for a hard step).
    let mut gs: Vec<f64> = grad.iter().map(|g| g.0).collect();
    gs.sort_by(f64::total_cmp);
    let strong = (percentile(&gs, 0.99) * 0.5).max(STRONG_EDGE / 10.0);
    let mut widths = Vec::new();
    for (i, (g, gx, gy)) in grad.iter().enumerate() {
        if *g < strong {
            continue;
        }
        let (x, y) = ((i as u32 % w) as i64, (i as u32 / w) as i64);
        let len = (gx * gx + gy * gy).sqrt().max(1e-9);
        let (dx, dy) = (gx / len, gy / len);
        let step = |s: f64| at(x + specula_core::dmath::round_f64(dx * s) as i64, y + specula_core::dmath::round_f64(dy * s) as i64);
        let height = (step(6.0) - step(-6.0)).abs();
        widths.push((height / g.max(1e-6)).clamp(1.0, 16.0));
    }
    widths.sort_by(f64::total_cmp);
    Ok(Measures {
        histogram,
        value_mean: mean,
        value_p10: percentile(&ls, 0.1),
        value_p50: percentile(&ls, 0.5),
        value_p90: percentile(&ls, 0.9),
        chroma_mean: chroma.iter().sum::<f64>() / n as f64,
        chroma_p90: percentile(&cs, 0.9),
        palette: palette(&lab),
        contrast: percentile(&ls, 0.9) - percentile(&ls, 0.1),
        rms_contrast: var.sqrt(),
        edge_density: edges as f64 / n as f64,
        shadow_softness: if widths.is_empty() { 0.0 } else { percentile(&widths, 0.5) },
    })
}

/// A seeded k-means palette in Oklab.
fn palette(lab: &[[f64; 3]]) -> Vec<(Rgb, f64)> {
    let stride = (lab.len() / KMEANS_SAMPLES).max(1);
    let pts: Vec<[f64; 3]> = lab.iter().step_by(stride).copied().collect();
    if pts.is_empty() {
        return Vec::new();
    }
    // k-means++ seeding with a fixed stream.
    let mut rng = Rng::named(0x7061_6c65, "palette");
    let mut centres = vec![pts[rng.below(pts.len() as u32) as usize]];
    while centres.len() < PALETTE.min(pts.len()) {
        let d: Vec<f64> = pts.iter().map(|p| centres.iter().map(|c| colour::lab_distance(*p, *c)).fold(f64::INFINITY, f64::min)).map(|d| d * d).collect();
        let total: f64 = d.iter().sum();
        if total <= 1e-12 {
            break;
        }
        let mut r = f64::from(rng.next_u32()) / 4_294_967_296.0 * total;
        let mut pick = pts.len() - 1;
        for (i, x) in d.iter().enumerate() {
            if r < *x {
                pick = i;
                break;
            }
            r -= x;
        }
        centres.push(pts[pick]);
    }
    let mut assign = vec![0usize; pts.len()];
    for _ in 0..12 {
        for (i, p) in pts.iter().enumerate() {
            assign[i] = (0..centres.len()).min_by(|a, b| colour::lab_distance(*p, centres[*a]).total_cmp(&colour::lab_distance(*p, centres[*b]))).unwrap_or(0);
        }
        for (k, c) in centres.iter_mut().enumerate() {
            let (mut s, mut n) = ([0.0; 3], 0usize);
            for (i, p) in pts.iter().enumerate() {
                if assign[i] == k {
                    for d in 0..3 {
                        s[d] += p[d];
                    }
                    n += 1;
                }
            }
            if n > 0 {
                *c = s.map(|v| v / n as f64);
            }
        }
    }
    let mut out: Vec<(Rgb, f64)> = centres.iter().enumerate().map(|(k, c)| (colour::oklab_to_srgb(*c), assign.iter().filter(|a| **a == k).count() as f64 / pts.len() as f64)).filter(|(_, w)| *w > 0.0).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(colour::hex(a.0).cmp(&colour::hex(b.0))));
    out
}

/// One measured image of a board.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub file: String,
    /// The bytes' content hash (a changed image makes the board stale).
    pub hash: String,
    pub size: [u32; 2],
    pub measures: Measures,
}

/// A board: its images and the targets their spread makes.
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub name: String,
    pub images: Vec<Image>,
}

/// One check of a scene against a board target.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub measure: String,
    pub value: f64,
    pub target: [f64; 2],
    pub passed: bool,
    pub hint: String,
}

impl Finding {
    pub fn to_json(&self, board: &str) -> Json {
        let mut j = Json::obj([
            ("check", Json::from(format!("board_{}", self.measure))),
            ("rule", Json::from(format!("board:{board}.{}", self.measure))),
            ("value", r4(self.value)),
            ("target", Json::arr(self.target.map(r4))),
            ("passed", Json::Bool(self.passed)),
            ("severity", Json::from(if self.passed { "ok" } else { "error" })),
        ]);
        if !self.hint.is_empty() {
            j.set("hint", Json::from(self.hint.as_str()));
        }
        j
    }
}

impl Board {
    pub fn new(name: &str, images: Vec<Image>) -> Result<Board> {
        if !valid_name(name) {
            return Err(TasteError::new("bad_board", "board names are [a-z0-9_-], 1-40 characters").at("board"));
        }
        if images.is_empty() || images.len() > MAX_IMAGES {
            return Err(TasteError::new("bad_board", format!("a board has 1..={MAX_IMAGES} images")).at("board"));
        }
        Ok(Board { name: name.into(), images })
    }

    /// The target range of a scalar measure: the images' range widened by its tolerance.
    pub fn target(&self, measure: &str) -> [f64; 2] {
        let tol = SCALARS.iter().find(|s| s.0 == measure).map_or(0.0, |s| s.1);
        let vals = self.images.iter().map(|i| i.measures.scalar(measure));
        let lo = vals.clone().fold(f64::INFINITY, f64::min);
        let hi = vals.fold(f64::NEG_INFINITY, f64::max);
        [lo - tol, hi + tol]
    }

    /// The board's palette: every image's, weighted by share.
    pub fn palette(&self) -> Vec<(Rgb, f64)> {
        let n = self.images.len() as f64;
        self.images.iter().flat_map(|i| i.measures.palette.iter().map(move |(c, w)| (*c, w / n))).collect()
    }

    /// The mean Oklab distance from each of `m`'s palette colours (by share) to the board's
    /// nearest; passes under 0.08.
    pub fn palette_distance(&self, m: &Measures) -> f64 {
        let board: Vec<[f64; 3]> = self.palette().iter().map(|(c, _)| colour::srgb_to_oklab(*c)).collect();
        let total: f64 = m.palette.iter().map(|(_, w)| w).sum::<f64>().max(1e-9);
        m.palette.iter().map(|(c, w)| w * board.iter().map(|b| colour::lab_distance(colour::srgb_to_oklab(*c), *b)).fold(f64::INFINITY, f64::min)).sum::<f64>() / total
    }

    /// Every target checked against a scene's measures.
    pub fn check(&self, m: &Measures) -> Vec<Finding> {
        let mut out: Vec<Finding> = SCALARS
            .iter()
            .map(|(name, _, high, low)| {
                let v = m.scalar(name);
                let t = self.target(name);
                let passed = v >= t[0] && v <= t[1];
                let hint = if passed {
                    String::new()
                } else if v > t[1] {
                    format!("{high} than the board: {name} {:.3}, the board's {:.3}-{:.3}", v, t[0], t[1])
                } else {
                    format!("{low} than the board: {name} {:.3}, the board's {:.3}-{:.3}", v, t[0], t[1])
                };
                // The hint's first words say which way to go: "darker" means the scene should
                // become darker (it measures brighter than the board).
                Finding { measure: (*name).into(), value: v, target: t, passed, hint }
            })
            .collect();
        let d = self.palette_distance(m);
        out.push(Finding { measure: "palette_distance".into(), value: d, target: [0.0, 0.08], passed: d <= 0.08, hint: if d <= 0.08 { String::new() } else { format!("the scene's colours are {d:.3} (Oklab) from the board's palette: move them towards its colours") } });
        out
    }

    pub fn to_text(&self) -> String {
        let mut targets = Json::obj(SCALARS.iter().map(|(n, ..)| ((*n).to_owned(), Json::arr(self.target(n).map(r4)))));
        targets.set("palette_distance", Json::arr([r4(0.0), r4(0.08)]));
        let j = Json::obj([
            ("format", Json::from(FORMAT)),
            ("board", Json::from(self.name.as_str())),
            ("images", Json::arr(self.images.iter().map(|i| Json::obj([("file", Json::from(i.file.as_str())), ("hash", Json::from(i.hash.as_str())), ("size", Json::arr(i.size.map(Json::from))), ("measures", i.measures.to_json())])))),
            ("palette", Json::arr(self.palette().iter().map(|(c, w)| Json::obj([("color", Json::from(colour::hex(*c))), ("share", r3(*w))])))),
            ("targets", targets),
        ]);
        let mut t = j.to_pretty();
        t.push('\n');
        t
    }

    pub fn parse(text: &str) -> Result<Board> {
        let j = Json::parse(text).map_err(|e| TasteError::new("invalid_json", e.to_string()))?;
        if j.get("format").and_then(Json::as_str) != Some(FORMAT) {
            return Err(TasteError::new("bad_format", format!("\"format\": \"{FORMAT}\"")).at("format"));
        }
        let images = j
            .get("images")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .take(MAX_IMAGES)
            .map(|i| {
                Some(Image {
                    file: i.get("file")?.as_str()?.to_owned(),
                    hash: i.get("hash")?.as_str()?.to_owned(),
                    size: [i.get("size")?.as_array()?.first()?.as_u64()? as u32, i.get("size")?.as_array()?.get(1)?.as_u64()? as u32],
                    measures: Measures::from_json(i.get("measures")?)?,
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| TasteError::new("bad_board", "each image has file, hash, size and measures").at("images"))?;
        Board::new(j.get("board").and_then(Json::as_str).unwrap_or(""), images)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dark image with a soft or hard light disc.
    fn disc(w: u32, light: [u8; 3], soft: f64) -> Vec<u8> {
        let mut px = vec![0u8; (w * w * 4) as usize];
        for y in 0..w {
            for x in 0..w {
                let (dx, dy) = (x as f64 - w as f64 / 2.0, y as f64 - w as f64 / 2.0);
                let d = (dx * dx + dy * dy).sqrt() / w as f64;
                let t = ((0.3 - d) / soft.max(1e-3) + 0.5).clamp(0.0, 1.0);
                let i = ((y * w + x) * 4) as usize;
                for c in 0..3 {
                    px[i + c] = (12.0 + t * (f64::from(light[c]) - 12.0)) as u8;
                }
                px[i + 3] = 255;
            }
        }
        px
    }

    #[test]
    fn measures_say_what_an_image_is() {
        let hard = measure(&disc(200, [220, 230, 255], 0.002), 200, 200).unwrap();
        let soft = measure(&disc(200, [220, 230, 255], 0.08), 200, 200).unwrap();
        assert!(hard.shadow_softness < soft.shadow_softness, "{} vs {}", hard.shadow_softness, soft.shadow_softness);
        assert!(hard.edge_density > 0.0 && hard.contrast > 0.5);
        assert!((hard.histogram.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(hard.palette.len() >= 2 && hard.palette[0].1 >= hard.palette[1].1);
        let grey = measure(&vec![128u8; 64 * 64 * 4], 64, 64).unwrap();
        assert!(grey.chroma_mean < 0.01 && grey.contrast < 1e-9 && grey.edge_density == 0.0);
        // Measures are deterministic and don't depend much on resolution.
        assert_eq!(measure(&disc(200, [220, 230, 255], 0.02), 200, 200).unwrap(), measure(&disc(200, [220, 230, 255], 0.02), 200, 200).unwrap());
        let big = measure(&disc(512, [220, 230, 255], 0.02), 512, 512).unwrap();
        let small = measure(&disc(256, [220, 230, 255], 0.02), 256, 256).unwrap();
        assert!((big.value_mean - small.value_mean).abs() < 0.01);
    }

    #[test]
    fn boards_make_targets_and_check_scenes() {
        let img = |soft: f64, light: [u8; 3]| {
            let m = measure(&disc(128, light, soft), 128, 128).unwrap();
            Image { file: "a.png".into(), hash: "0".into(), size: [128, 128], measures: m }
        };
        let board = Board::new("moonlit", vec![img(0.04, [180, 200, 255]), img(0.06, [160, 180, 240])]).unwrap();
        let t = board.target("value_mean");
        assert!(t[0] < t[1]);
        // A scene like the board passes; a bright flat one fails with hints.
        let like = measure(&disc(128, [170, 190, 250], 0.05), 128, 128).unwrap();
        let fails: Vec<Finding> = board.check(&like).into_iter().filter(|f| !f.passed).collect();
        assert!(fails.len() <= 1, "{fails:?}");
        let flat = measure(&vec![230u8; 64 * 64 * 4], 64, 64).unwrap();
        let f = board.check(&flat);
        let v = f.iter().find(|f| f.measure == "value_mean").unwrap();
        assert!(!v.passed && v.hint.starts_with("darker"), "{v:?}");
        assert_eq!(v.to_json("moonlit").get("rule").and_then(Json::as_str), Some("board:moonlit.value_mean"));
        let back = Board::parse(&board.to_text()).unwrap();
        assert_eq!(back.images.len(), 2);
        assert_eq!(back.target("contrast").map(|v| (v * 1000.0) as i64), board.target("contrast").map(|v| (v * 1000.0) as i64));
    }
}
