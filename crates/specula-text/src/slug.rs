//! Slug-style glyph data: outlines evaluated per pixel on the GPU, sharp at any size, rotation or
//! perspective, with no atlas (design: `docs/design/TEXT.md`, stage T2).
//!
//! The algorithm is Eric Lengyel's (JCGT 2017, "GPU-Centered Font Rendering Directly from Glyph
//! Outlines"; the patent was dedicated to the public domain in March 2026); this is our own
//! implementation of it. Per pixel, a horizontal and a vertical ray are cast from the pixel
//! centre through the glyph's quadratic curves:
//!
//! - **Root eligibility** comes from the signs of the curve's three control points relative to
//!   the ray (the `0x2E74` lookup): which of the two roots of `y(t) = 0` the ray crosses, and in
//!   which direction, without ever testing `t` against `[0, 1]`. Robust at endpoints shared by
//!   two curves.
//! - **Coverage** of each crossing is `saturate(x · pixels_per_em + 0.5)`: a one-pixel box filter
//!   along the ray. Summed with signs, that is the (anti-aliased) winding number.
//! - The two rays' coverages are **blended by distance** to their nearest crossing, which fixes
//!   each ray's blind spot (curves nearly parallel to it).
//! - **Bands** keep the work per pixel small: the glyph's box is cut into horizontal and vertical
//!   bands, each listing the curves that reach it, sorted so the loop stops at the first curve
//!   wholly behind the pixel.
//!
//! [`SlugGlyph::coverage`] is the reference implementation; the GPU shader
//! (`crates/specula-gpu/shaders/glyph.glsl`) is a line-for-line port, and the tests here compare
//! it with the exact-area rasteriser. Lines become quadratics with a mid control point.
//!
//! # Word layout (the GPU store, `u32` words; `f32` stored as bits)
//!
//! A glyph at word `g`:
//!
//! | word | meaning |
//! |---|---|
//! | g+0..g+4 | box in em units: x0, y0, x1, y1 (f32) |
//! | g+4 | horizontal bands (low 16 bits), vertical bands (high 16 bits) |
//! | g+5 | first band entry (absolute word) |
//! | g+6, g+7 | reserved (0) |
//!
//! Then the band entries (two words each: first index word, count), horizontal bands bottom to
//! top, then vertical bands left to right; then the index lists (one word each: the absolute word
//! of a curve); then the curves (six f32 words each: p1, p2, p3 in em units, y up).

use crate::error::FontError;
use crate::font::{Font, FontId, GlyphId};
use crate::outline::{Outline, Segment};
use std::collections::BTreeMap;
use std::fmt;

/// Most bands per direction.
pub const MAX_BANDS: usize = 16;
/// Most curves of one glyph (CJK glyphs reach about 600; more is a hostile font).
pub const MAX_GLYPH_CURVES: usize = 4096;
/// Most curves one band lists: the most a pixel ever walks per ray (the shader stops there too),
/// which bounds the GPU's work per pixel whatever the font.
pub const MAX_BAND_CURVES: usize = 512;
/// Words of a glyph header.
pub const HEADER_WORDS: usize = 8;
/// Words of one curve.
pub const CURVE_WORDS: usize = 6;
/// The default store cap: 4 MiB of words (about 4,000 Latin glyphs or 1,500 CJK ones).
pub const DEFAULT_STORE_BYTES: usize = 4 << 20;

/// Why a glyph can't be prepared or stored.
#[derive(Clone, Debug, PartialEq)]
pub enum SlugError {
    /// The outline couldn't be read.
    Font(FontError),
    /// The glyph has more than [`MAX_GLYPH_CURVES`] curves.
    TooManyCurves { curves: usize, max: usize },
    /// One band would list more than [`MAX_BAND_CURVES`] curves.
    BandTooFull { curves: usize, max: usize },
    /// The outline holds a non-finite coordinate.
    NotFinite,
    /// The store would pass its cap with glyphs this frame still needs.
    StoreFull { words: usize, cap_words: usize },
}

impl SlugError {
    pub fn code(&self) -> &'static str {
        match self {
            SlugError::Font(e) => e.reason.code(),
            SlugError::TooManyCurves { .. } => "too_many_curves",
            SlugError::BandTooFull { .. } => "band_too_full",
            SlugError::NotFinite => "not_finite",
            SlugError::StoreFull { .. } => "glyph_store_full",
        }
    }
}

impl fmt::Display for SlugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlugError::Font(e) => write!(f, "{e}"),
            SlugError::TooManyCurves { curves, max } => write!(f, "glyph has {curves} curves (at most {max})"),
            SlugError::BandTooFull { curves, max } => write!(f, "a band of the glyph crosses {curves} curves (at most {max})"),
            SlugError::NotFinite => f.write_str("glyph outline has a non-finite coordinate"),
            SlugError::StoreFull { words, cap_words } => {
                write!(f, "glyph store full: this frame needs more than {cap_words} words ({words} in use)")
            }
        }
    }
}

impl std::error::Error for SlugError {}

impl From<FontError> for SlugError {
    fn from(e: FontError) -> SlugError {
        SlugError::Font(e)
    }
}

/// One quadratic curve in em units: start, control, end.
pub type Curve = [[f32; 2]; 3];

/// A glyph ready for per-pixel evaluation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlugGlyph {
    /// Box of the curves in em units: x0, y0, x1, y1 (all zero for an empty glyph).
    pub bounds: [f32; 4],
    pub curves: Vec<Curve>,
    /// Curve indices per horizontal band (bottom to top), by descending maximum x.
    pub hbands: Vec<Vec<u32>>,
    /// Curve indices per vertical band (left to right), by descending maximum y.
    pub vbands: Vec<Vec<u32>>,
}

fn max3(a: f32, b: f32, c: f32) -> f32 {
    a.max(b).max(c)
}

fn min3(a: f32, b: f32, c: f32) -> f32 {
    a.min(b).min(c)
}

fn saturate(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// The band `v` falls in, of `n` bands over `lo..hi` (clamped; what the shader computes).
pub fn band_of(v: f32, lo: f32, hi: f32, n: usize) -> usize {
    let span = hi - lo;
    if n <= 1 || span <= 0.0 {
        return 0;
    }
    let b = ((v - lo) * n as f32 / span).floor();
    if b.is_nan() || b < 0.0 {
        0
    } else {
        (b as usize).min(n - 1)
    }
}

/// Bands for `curves` curves: about one per four curves, 1..=[`MAX_BANDS`].
fn band_count(curves: usize) -> usize {
    curves.div_ceil(4).clamp(1, MAX_BANDS)
}

/// Lists the curves reaching each of `n` bands over `lo..hi` along `axis` (0 = x, 1 = y), sorted
/// by descending maximum on the other axis. Curves flat along the band direction never cross a
/// ray parallel to it and are left out.
fn bands(curves: &[Curve], axis: usize, lo: f32, hi: f32, n: usize) -> Vec<Vec<u32>> {
    let other = 1 - axis;
    let mut out = vec![Vec::new(); n];
    let span = hi - lo;
    for (i, c) in curves.iter().enumerate() {
        let (a, b, d) = (c[0][axis], c[1][axis], c[2][axis]);
        if a == b && b == d {
            continue;
        }
        let (cmin, cmax) = (min3(a, b, d), max3(a, b, d));
        for (k, list) in out.iter_mut().enumerate() {
            let b0 = lo + span * k as f32 / n as f32;
            let b1 = if k + 1 == n { hi } else { lo + span * (k + 1) as f32 / n as f32 };
            // Inclusive with a margin: a ray exactly on a band edge sees both bands' curves.
            let eps = span * 1e-5;
            if cmax >= b0 - eps && cmin <= b1 + eps {
                list.push(i as u32);
            }
        }
    }
    for list in &mut out {
        list.sort_by(|&p, &q| {
            let m = |i: u32| {
                let c = &curves[i as usize];
                max3(c[0][other], c[1][other], c[2][other])
            };
            m(q).total_cmp(&m(p)).then(p.cmp(&q))
        });
    }
    out
}

/// Both roots' positions along a ray at 0 (the curve already relative to the pixel): for a
/// horizontal ray, `y(t) = 0` solved, `x` returned; `swap` solves the vertical ray instead.
fn roots(c: &Curve, swap: bool) -> (f32, f32) {
    let (u, v) = if swap { (1, 0) } else { (0, 1) };
    let (p1, p2, p3) = (c[0], c[1], c[2]);
    let a = [p1[u] - 2.0 * p2[u] + p3[u], p1[v] - 2.0 * p2[v] + p3[v]];
    let b = [p1[u] - p2[u], p1[v] - p2[v]];
    let (t1, t2) = if a[1].abs() < 1.0 / 65536.0 {
        // Linear in t: one root, shared.
        let t = if b[1] != 0.0 { p1[v] * 0.5 / b[1] } else { 0.0 };
        (t, t)
    } else {
        let ra = 1.0 / a[1];
        let d = (b[1] * b[1] - a[1] * p1[v]).max(0.0).sqrt();
        ((b[1] - d) * ra, (b[1] + d) * ra)
    };
    let x = |t: f32| (a[0] * t - 2.0 * b[0]) * t + p1[u];
    (x(t1), x(t2))
}

/// Which roots a ray crosses (bit 0: root 1, crossing downward; bit 1: root 2, upward), from
/// the signs of the three control points across the ray.
fn eligible(y1: f32, y2: f32, y3: f32) -> u32 {
    let shift = (if y1 > 0.0 { 2 } else { 0 }) + (if y2 > 0.0 { 4 } else { 0 }) + (if y3 > 0.0 { 8 } else { 0 });
    (0x2E74u32 >> shift) & 3
}

impl SlugGlyph {
    /// Prepares `outline` (font units, y up) at `units_per_em`.
    pub fn from_outline(outline: &Outline, units_per_em: u16) -> Result<SlugGlyph, SlugError> {
        let s = 1.0 / units_per_em.max(1) as f32;
        let mut curves: Vec<Curve> = Vec::new();
        let mut finite = true;
        let mut too_many = 0usize;
        outline.segments(|seg| {
            let (p1, p2, p3) = match seg {
                Segment::Line(a, b) => (a, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5], b),
                Segment::Quad(a, c, b) => (a, c, b),
            };
            let c = [[p1[0] * s, p1[1] * s], [p2[0] * s, p2[1] * s], [p3[0] * s, p3[1] * s]];
            if !c.iter().flatten().all(|v| v.is_finite()) {
                finite = false;
                return;
            }
            if c[0] == c[2] && c[0] == c[1] {
                return; // a point: crosses nothing
            }
            if curves.len() >= MAX_GLYPH_CURVES {
                too_many += 1;
                return;
            }
            curves.push(c);
        });
        if !finite {
            return Err(SlugError::NotFinite);
        }
        if too_many > 0 {
            return Err(SlugError::TooManyCurves { curves: MAX_GLYPH_CURVES + too_many, max: MAX_GLYPH_CURVES });
        }
        let g = SlugGlyph::from_curves(curves);
        if let Some(n) = g.hbands.iter().chain(&g.vbands).map(Vec::len).find(|&n| n > MAX_BAND_CURVES) {
            return Err(SlugError::BandTooFull { curves: n, max: MAX_BAND_CURVES });
        }
        Ok(g)
    }

    /// A glyph from curves in em units (quadratic; a line has its control point at its middle).
    pub fn from_curves(curves: Vec<Curve>) -> SlugGlyph {
        if curves.is_empty() {
            return SlugGlyph::default();
        }
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in curves.iter().flatten() {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
        }
        let n = band_count(curves.len());
        let hbands = bands(&curves, 1, b[1], b[3], n);
        let vbands = bands(&curves, 0, b[0], b[2], n);
        SlugGlyph { bounds: b, curves, hbands, vbands }
    }

    /// Prepares glyph `glyph` of `font`.
    pub fn of(font: &Font, glyph: GlyphId) -> Result<SlugGlyph, SlugError> {
        SlugGlyph::from_outline(&font.glyph_outline(glyph)?, font.units_per_em())
    }

    pub fn is_empty(&self) -> bool {
        self.curves.is_empty()
    }

    /// Words this glyph takes in a store.
    pub fn words(&self) -> usize {
        if self.is_empty() {
            return 0;
        }
        let entries: usize = self.hbands.iter().chain(&self.vbands).map(Vec::len).sum();
        HEADER_WORDS + 2 * (self.hbands.len() + self.vbands.len()) + entries + CURVE_WORDS * self.curves.len()
    }

    /// Appends the glyph's words (layout in the module docs) to `out`; returns its first word.
    pub fn encode(&self, out: &mut Vec<u32>) -> u32 {
        let g = out.len();
        let f = |v: f32| v.to_bits();
        out.extend_from_slice(&[f(self.bounds[0]), f(self.bounds[1]), f(self.bounds[2]), f(self.bounds[3])]);
        out.push(self.hbands.len() as u32 | (self.vbands.len() as u32) << 16);
        let band_at = g + HEADER_WORDS;
        out.extend_from_slice(&[band_at as u32, 0, 0]);
        let n_bands = self.hbands.len() + self.vbands.len();
        let mut index_at = band_at + 2 * n_bands;
        let entries: usize = self.hbands.iter().chain(&self.vbands).map(Vec::len).sum();
        let curves_at = index_at + entries;
        for list in self.hbands.iter().chain(&self.vbands) {
            out.extend_from_slice(&[index_at as u32, list.len() as u32]);
            index_at += list.len();
        }
        for list in self.hbands.iter().chain(&self.vbands) {
            out.extend(list.iter().map(|&i| (curves_at + CURVE_WORDS * i as usize) as u32));
        }
        for c in &self.curves {
            out.extend(c.iter().flatten().map(|&v| f(v)));
        }
        g as u32
    }

    /// Coverage (0..1) of the pixel centred at `p` (em units, y up) whose size is
    /// `1 / pixels_per_em` em on each axis. The reference for the GPU shader.
    pub fn coverage(&self, p: [f32; 2], pixels_per_em: [f32; 2]) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let b = self.bounds;
        let rel = |c: &Curve| -> Curve { [[c[0][0] - p[0], c[0][1] - p[1]], [c[1][0] - p[0], c[1][1] - p[1]], [c[2][0] - p[0], c[2][1] - p[1]]] };
        // Horizontal ray (+x).
        let (mut xcov, mut xwgt) = (0.0f32, 0.0f32);
        for &i in &self.hbands[band_of(p[1], b[1], b[3], self.hbands.len())] {
            let c = rel(&self.curves[i as usize]);
            if max3(c[0][0], c[1][0], c[2][0]) * pixels_per_em[0] < -0.5 {
                break;
            }
            let code = eligible(c[0][1], c[1][1], c[2][1]);
            if code != 0 {
                let (r1, r2) = roots(&c, false);
                let (r1, r2) = (r1 * pixels_per_em[0], r2 * pixels_per_em[0]);
                if code & 1 != 0 {
                    xcov += saturate(r1 + 0.5);
                    xwgt = xwgt.max(saturate(1.0 - r1.abs() * 2.0));
                }
                if code > 1 {
                    xcov -= saturate(r2 + 0.5);
                    xwgt = xwgt.max(saturate(1.0 - r2.abs() * 2.0));
                }
            }
        }
        // Vertical ray (+y): the swapped plane mirrors the winding, so the signs flip.
        let (mut ycov, mut ywgt) = (0.0f32, 0.0f32);
        for &i in &self.vbands[band_of(p[0], b[0], b[2], self.vbands.len())] {
            let c = rel(&self.curves[i as usize]);
            if max3(c[0][1], c[1][1], c[2][1]) * pixels_per_em[1] < -0.5 {
                break;
            }
            let code = eligible(c[0][0], c[1][0], c[2][0]);
            if code != 0 {
                let (r1, r2) = roots(&c, true);
                let (r1, r2) = (r1 * pixels_per_em[1], r2 * pixels_per_em[1]);
                if code & 1 != 0 {
                    ycov -= saturate(r1 + 0.5);
                    ywgt = ywgt.max(saturate(1.0 - r1.abs() * 2.0));
                }
                if code > 1 {
                    ycov += saturate(r2 + 0.5);
                    ywgt = ywgt.max(saturate(1.0 - r2.abs() * 2.0));
                }
            }
        }
        let blended = (xcov * xwgt + ycov * ywgt).abs() / (xwgt + ywgt).max(1.0 / 65536.0);
        saturate(blended.max(xcov.abs().min(ycov.abs())))
    }
}

/// Coverage of the glyph at word `g` of `words` (the encoded layout): what the GPU shader reads,
/// evaluated on the CPU. Bad offsets read as empty (never a panic).
pub fn coverage_from_words(words: &[u32], g: u32, p: [f32; 2], pixels_per_em: [f32; 2]) -> f32 {
    let w = |i: usize| words.get(i).copied().unwrap_or(0);
    let fw = |i: usize| f32::from_bits(w(i));
    let g = g as usize;
    let b = [fw(g), fw(g + 1), fw(g + 2), fw(g + 3)];
    let counts = w(g + 4);
    let (nh, nv) = ((counts & 0xFFFF) as usize, (counts >> 16) as usize);
    let band_at = w(g + 5) as usize;
    if nh == 0 || nv == 0 {
        return 0.0;
    }
    let curve = |at: usize| -> Curve {
        let v = |k: usize| fw(at + k);
        [[v(0) - p[0], v(1) - p[1]], [v(2) - p[0], v(3) - p[1]], [v(4) - p[0], v(5) - p[1]]]
    };
    let mut cov = [0.0f32; 2];
    let mut wgt = [0.0f32; 2];
    for (axis, (band, ppe)) in [(band_of(p[1], b[1], b[3], nh), pixels_per_em[0]), (nh + band_of(p[0], b[0], b[2], nv), pixels_per_em[1])].into_iter().enumerate() {
        let (first, count) = (w(band_at + 2 * band) as usize, w(band_at + 2 * band + 1) as usize);
        let swap = axis == 1;
        let (u, v) = if swap { (1, 0) } else { (0, 1) };
        let sign = if swap { -1.0 } else { 1.0 };
        for k in 0..count.min(MAX_BAND_CURVES) {
            let c = curve(w(first + k) as usize);
            if max3(c[0][u], c[1][u], c[2][u]) * ppe < -0.5 {
                break;
            }
            let code = eligible(c[0][v], c[1][v], c[2][v]);
            if code != 0 {
                let (r1, r2) = roots(&c, swap);
                let (r1, r2) = (r1 * ppe, r2 * ppe);
                if code & 1 != 0 {
                    cov[axis] += sign * saturate(r1 + 0.5);
                    wgt[axis] = wgt[axis].max(saturate(1.0 - r1.abs() * 2.0));
                }
                if code > 1 {
                    cov[axis] -= sign * saturate(r2 + 0.5);
                    wgt[axis] = wgt[axis].max(saturate(1.0 - r2.abs() * 2.0));
                }
            }
        }
    }
    let blended = (cov[0] * wgt[0] + cov[1] * wgt[1]).abs() / (wgt[0] + wgt[1]).max(1.0 / 65536.0);
    saturate(blended.max(cov[0].abs().min(cov[1].abs())))
}

/// Where a stored glyph is, and its box (em units) for laying out its quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredGlyph {
    /// First word of the glyph in [`SlugStore::words`].
    pub word: u32,
    pub bounds: [f32; 4],
}

/// The GPU store's contents on the CPU: glyphs appended by (font, glyph), each prepared once.
///
/// The GPU copy is incremental: a renderer that has uploaded the first `n` words of store
/// [`SlugStore::id`] only uploads the rest. Within a frame nothing moves: a glyph past the cap
/// fails ([`SlugError::StoreFull`]) and is **remembered**, so it is never prepared again until
/// the store has room (review of cd954bd, major 1: a refusal that wasn't remembered was prepared
/// again every frame, and a frame that overflowed threw the whole store away at the next one,
/// so a page of CJK text flickered and never finished drawing).
///
/// A store below its maximum grows (doubling) as soon as a glyph doesn't fit: that moves nothing.
/// Between frames ([`SlugStore::begin_frame`], [`SlugStore::begin_capture`]) a store that
/// overflowed, or is three quarters full of glyphs nobody used lately, makes room:
/// - **it grows** (doubling, up to its maximum) when the glyphs of the last two frames take more
///   than half of it: the same id, so the GPU keeps what it has and uploads the rest;
/// - **else it evicts** the least recently used glyphs, keeping the last two frames' and then the
///   most recently used others up to half the store: the kept words are copied (their offsets
///   moved, [`relocate`]), never prepared again, under a new id;
/// - **else** (a maximal store the recent frames fill) it stays as it is, and the glyphs that
///   didn't fit stay refused: the scene converges, it doesn't flicker.
#[derive(Debug)]
pub struct SlugStore {
    id: u64,
    generation: u64,
    words: Vec<u32>,
    /// Each glyph, its words, and the frame that last used it.
    glyphs: BTreeMap<(FontId, GlyphId), (Option<StoredGlyph>, u32, u64)>,
    /// Glyphs that didn't fit (with their words), until the store makes room.
    refused: BTreeMap<(FontId, GlyphId), u32>,
    cap_words: usize,
    max_cap_words: usize,
    frame: u64,
    /// Words of the glyphs this frame used, the frame before's (any kind), and the last scene
    /// frame's.
    used_words: usize,
    prev_used: usize,
    last_used_words: usize,
    /// A glyph this frame didn't fit, and one refused earlier that this frame asked for again.
    overflowed: bool,
    refused_hit: bool,
    /// The usage at the last room making that found nothing to free (it isn't tried again until
    /// usage moves by an eighth of the store).
    fruitless: Option<usize>,
    /// Times the store moved its glyphs (evicted some; a new id: the GPU uploads it again).
    pub resets: u64,
    /// Times the store grew.
    pub grows: u64,
    /// Glyphs prepared (outlines turned into curves and bands) since it was made, and their
    /// words (stored or refused: the work a preparation budget counts).
    pub prepared: u64,
    pub prepared_words: u64,
}

/// Most refused glyphs a store remembers (past it, the oldest refusals are forgotten and may be
/// prepared again once).
pub const MAX_REFUSED: usize = 65_536;

static NEXT_STORE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Moves an encoded glyph (`words`, starting at its header) by `delta` words: its band table,
/// band entries and curve index hold absolute word offsets.
pub fn relocate(words: &mut [u32], delta: i64) {
    let shift = |w: &mut u32| *w = (i64::from(*w) + delta) as u32;
    if words.len() < HEADER_WORDS {
        return;
    }
    let bands = (words[4] & 0xFFFF) as usize + (words[4] >> 16) as usize;
    shift(&mut words[5]);
    let mut entries = 0usize;
    for b in 0..bands {
        let at = HEADER_WORDS + 2 * b;
        if at + 1 >= words.len() {
            return;
        }
        shift(&mut words[at]);
        entries += words[at + 1] as usize;
    }
    let first = HEADER_WORDS + 2 * bands;
    let end = (first + entries).min(words.len());
    for w in &mut words[first..end] {
        shift(w);
    }
}

impl SlugStore {
    /// A store of at most `cap_bytes` bytes of words (it never grows).
    pub fn new(cap_bytes: usize) -> SlugStore {
        SlugStore::growing(cap_bytes, cap_bytes)
    }

    /// A store of `cap_bytes` that grows, as scenes need, up to `max_bytes`.
    pub fn growing(cap_bytes: usize, max_bytes: usize) -> SlugStore {
        let generation = NEXT_STORE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let cap_words = (cap_bytes / 4).max(HEADER_WORDS);
        SlugStore {
            id: generation << 16,
            generation,
            words: Vec::new(),
            glyphs: BTreeMap::new(),
            refused: BTreeMap::new(),
            cap_words,
            max_cap_words: (max_bytes / 4).max(cap_words),
            frame: 0,
            used_words: 0,
            prev_used: 0,
            last_used_words: 0,
            overflowed: false,
            refused_hit: false,
            fruitless: None,
            resets: 0,
            grows: 0,
            prepared: 0,
            prepared_words: 0,
        }
    }

    /// Identity of the current contents (changes when the store moves its glyphs).
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn words(&self) -> &[u32] {
        &self.words
    }

    pub fn bytes(&self) -> usize {
        self.words.len() * 4
    }

    pub fn cap_bytes(&self) -> usize {
        self.cap_words * 4
    }

    pub fn max_bytes(&self) -> usize {
        self.max_cap_words * 4
    }

    /// Glyphs stored (empty glyphs included).
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Glyphs refused for want of room (not prepared again until the store makes room).
    pub fn refused_count(&self) -> usize {
        self.refused.len()
    }

    /// Starts a frame of the scene (the window's).
    pub fn begin_frame(&mut self) {
        self.last_used_words = self.used_words;
        self.next_frame();
    }

    /// Starts a frame that isn't the scene's own (a capture of some other view): the same room
    /// making, but the scene's per-frame usage isn't reset, so a capture never makes the scene's
    /// glyphs look stale.
    pub fn begin_capture(&mut self) {
        self.next_frame();
    }

    fn next_frame(&mut self) {
        // The last two frames' glyphs (the scene's and a capture between) are what's in use; the
        // larger of their counters is a cheap lower bound of it, so staleness is judged eagerly
        // and the exact words are summed only when making room.
        let used = self.used_words.max(self.prev_used);
        let stale = self.words.len() * 4 > self.cap_words * 3 && used * 2 < self.cap_words;
        let overflowed = std::mem::take(&mut self.overflowed);
        let refused_hit = std::mem::take(&mut self.refused_hit);
        self.prev_used = std::mem::take(&mut self.used_words);
        let since = self.frame.saturating_sub(1);
        self.frame += 1;
        if !(overflowed || stale || refused_hit) {
            return;
        }
        // A scene that fills a maximal store asks every frame for glyphs that can't fit: once a
        // room making found nothing to free, it waits until usage moves.
        if !overflowed && self.fruitless.is_some_and(|u| u.abs_diff(used) < self.cap_words / 8) {
            return;
        }
        let recent: usize = self.glyphs.values().filter(|(_, _, f)| *f >= since).map(|(_, w, _)| *w as usize).sum();
        self.fruitless = (!self.make_room(since, recent)).then_some(used);
    }

    /// Grows or evicts (see the type's docs); false when neither was possible or worth it.
    fn make_room(&mut self, since: u64, recent: usize) -> bool {
        // Grow while what recent frames use takes more than half of it.
        let cap = self.cap_words;
        while recent * 2 > self.cap_words && self.cap_words < self.max_cap_words {
            self.cap_words = (self.cap_words * 2).min(self.max_cap_words);
        }
        let grew = self.cap_words != cap;
        if grew {
            self.grows += 1;
            self.refused.clear();
            if self.words.len() * 4 <= self.cap_words * 3 {
                return true;
            }
        }
        // Evict the least recently used: keep recent frames' glyphs, then the most recently used
        // others while the kept words stay under half the store.
        let mut others: Vec<(u64, (FontId, GlyphId), u32)> = self.glyphs.iter().filter(|(_, (_, _, f))| *f < since).map(|(k, (_, w, f))| (*f, *k, *w)).collect();
        others.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut kept = recent;
        let mut drop = Vec::new();
        for (_, key, w) in others {
            if kept + w as usize <= self.cap_words / 2 {
                kept += w as usize;
            } else {
                drop.push(key);
            }
        }
        // Worth moving only when it frees at least an eighth of the store.
        if (self.words.len() - kept.min(self.words.len())) * 8 < self.cap_words {
            return grew;
        }
        for key in &drop {
            self.glyphs.remove(key);
        }
        let mut live: Vec<(&mut Option<StoredGlyph>, u32)> = self.glyphs.values_mut().filter_map(|(s, w, _)| s.is_some().then_some((s, *w))).collect();
        live.sort_by_key(|(s, _)| s.map_or(0, |g| g.word));
        let mut words = Vec::with_capacity(kept);
        for (s, n) in live {
            let Some(g) = s.as_mut() else { continue };
            let (from, at) = (g.word as usize, words.len());
            words.extend_from_slice(&self.words[from..from + n as usize]);
            relocate(&mut words[at..], at as i64 - from as i64);
            g.word = at as u32;
        }
        self.words = words;
        self.refused.clear();
        self.resets += 1;
        self.id = (self.generation << 16) | (self.resets & 0xFFFF);
        true
    }

    /// The stored glyph (prepared and appended on first use); `None` for a glyph with no
    /// outline (a space). A glyph that didn't fit is refused again without being prepared.
    pub fn glyph(&mut self, font: &Font, glyph: GlyphId) -> Result<Option<StoredGlyph>, SlugError> {
        if let Some((s, words, frame)) = self.glyphs.get_mut(&(font.id(), glyph)) {
            if *frame != self.frame {
                *frame = self.frame;
                self.used_words += *words as usize;
            }
            return Ok(*s);
        }
        if self.refused.contains_key(&(font.id(), glyph)) {
            // Remembered: no preparation; the next frame judges whether room can be made.
            self.refused_hit = true;
            return Err(SlugError::StoreFull { words: self.words.len(), cap_words: self.cap_words });
        }
        let g = SlugGlyph::of(font, glyph)?;
        self.prepared += 1;
        self.prepared_words += g.words().max(HEADER_WORDS) as u64;
        self.insert(font.id(), glyph, &g)
    }

    /// Whether the glyph is stored already (using it costs no preparation).
    pub fn contains(&self, font: &Font, glyph: GlyphId) -> bool {
        self.glyphs.contains_key(&(font.id(), glyph))
    }

    /// Whether the glyph was refused for want of room (using it costs no preparation either).
    pub fn is_refused(&self, font: &Font, glyph: GlyphId) -> bool {
        self.refused.contains_key(&(font.id(), glyph))
    }

    /// Words the glyphs of the last finished frame used (what a steady scene needs).
    pub fn last_frame_words(&self) -> usize {
        self.last_used_words
    }

    /// Stores a prepared glyph under (`font`, `glyph`) (a test or a custom outline source).
    pub fn insert(&mut self, font: FontId, glyph: GlyphId, g: &SlugGlyph) -> Result<Option<StoredGlyph>, SlugError> {
        if g.is_empty() {
            self.glyphs.insert((font, glyph), (None, 0, self.frame));
            return Ok(None);
        }
        let need = g.words();
        // Growing moves nothing (only the cap rises): done at once, within a frame too.
        if self.words.len() + need > self.cap_words && self.cap_words < self.max_cap_words {
            while self.words.len() + need > self.cap_words && self.cap_words < self.max_cap_words {
                self.cap_words = (self.cap_words * 2).min(self.max_cap_words);
            }
            self.grows += 1;
        }
        if self.words.len() + need > self.cap_words {
            self.overflowed = true;
            if self.refused.len() >= MAX_REFUSED {
                self.refused.clear();
            }
            self.refused.insert((font, glyph), need as u32);
            return Err(SlugError::StoreFull { words: self.words.len(), cap_words: self.cap_words });
        }
        let word = g.encode(&mut self.words);
        let s = StoredGlyph { word, bounds: g.bounds };
        self.glyphs.insert((font, glyph), (Some(s), need as u32, self.frame));
        self.used_words += need;
        Ok(Some(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundled::Bundled;
    use crate::raster::Rasterizer;

    fn square(x0: f32, y0: f32, x1: f32, y1: f32, clockwise: bool) -> Vec<Curve> {
        let line = |a: [f32; 2], b: [f32; 2]| [a, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5], b];
        let pts = if clockwise { [[x0, y0], [x0, y1], [x1, y1], [x1, y0]] } else { [[x0, y0], [x1, y0], [x1, y1], [x0, y1]] };
        (0..4).map(|i| line(pts[i], pts[(i + 1) % 4])).collect()
    }

    #[test]
    fn a_square_covers_its_inside_and_nothing_outside_either_winding() {
        for cw in [true, false] {
            let g = SlugGlyph::from_curves(square(0.1, 0.1, 0.9, 0.7, cw));
            let ppe = [100.0, 100.0];
            assert_eq!(g.coverage([0.5, 0.4], ppe), 1.0, "inside, cw {cw}");
            assert_eq!(g.coverage([0.95, 0.4], ppe), 0.0, "right of it");
            assert_eq!(g.coverage([0.5, 0.75], ppe), 0.0, "above it");
            assert_eq!(g.coverage([0.05, 0.05], ppe), 0.0, "below left");
            // Half a pixel straddles the edge: half covered.
            let edge = g.coverage([0.9, 0.4], ppe);
            assert!((edge - 0.5).abs() < 0.02, "edge {edge}");
            // A pixel centred just inside a corner: partly covered. (Exactly on the corner, both
            // rays run along an edge and see nothing: a measure-zero case.)
            let corner = g.coverage([0.899, 0.699], ppe);
            assert!((0.1..0.9).contains(&corner), "corner {corner}");
        }
    }

    #[test]
    fn holes_and_overlaps_follow_nonzero_winding() {
        let mut curves = square(0.0, 0.0, 1.0, 1.0, true);
        curves.extend(square(0.3, 0.3, 0.7, 0.7, false)); // a hole
        let g = SlugGlyph::from_curves(curves);
        assert_eq!(g.coverage([0.5, 0.5], [50.0, 50.0]), 0.0, "the hole");
        assert_eq!(g.coverage([0.15, 0.5], [50.0, 50.0]), 1.0, "the ring");
        let mut curves = square(0.0, 0.0, 1.0, 1.0, true);
        curves.extend(square(0.3, 0.3, 0.7, 0.7, true)); // an overlap, same winding
        let g = SlugGlyph::from_curves(curves);
        assert_eq!(g.coverage([0.5, 0.5], [50.0, 50.0]), 1.0, "winding 2 is still covered once");
    }

    #[test]
    fn encoded_words_evaluate_exactly_like_the_glyph() {
        let font = Bundled::UiRegular.load().unwrap();
        let mut words = vec![7u32; 3]; // not at word 0
        for c in ['g', 'A', '@', 'Ж', 'Ω', 'ß'] {
            let gid = font.glyph_index(c).unwrap();
            let g = SlugGlyph::of(&font, gid).unwrap();
            let at = g.encode(&mut words);
            assert_eq!(words.len() - at as usize, g.words(), "{c}");
            for i in 0..400 {
                let p = [g.bounds[0] - 0.02 + (i % 20) as f32 * 0.04, g.bounds[1] - 0.02 + (i / 20) as f32 * 0.04];
                for ppe in [[12.0, 12.0], [300.0, 40.0]] {
                    let a = g.coverage(p, ppe);
                    let b = coverage_from_words(&words, at, p, ppe);
                    assert_eq!(a.to_bits(), b.to_bits(), "{c} at {p:?}");
                }
            }
        }
        // Garbage offsets read as nothing.
        assert_eq!(coverage_from_words(&words, 1_000_000, [0.0; 2], [10.0; 2]), 0.0);
        assert_eq!(coverage_from_words(&[u32::MAX; 16], 0, [0.0; 2], [10.0; 2]).clamp(0.0, 1.0), coverage_from_words(&[u32::MAX; 16], 0, [0.0; 2], [10.0; 2]));
    }

    /// Slug's coverage against the exact-area rasteriser, glyph by glyph, at 48 px: the pixel
    /// centres agree closely (the two filters differ only along edges).
    #[test]
    fn matches_the_exact_area_rasteriser() {
        for face in [Bundled::UiRegular, Bundled::Display] {
            let font = face.load().unwrap();
            let px = 48.0f32;
            let mut r = Rasterizer::new();
            for c in "AgW@&8Жщλψ".chars() {
                let gid = font.glyph_index(c).unwrap();
                let outline = font.glyph_outline(gid).unwrap();
                let scale = font.scale_for_px(px);
                let bmp = r.rasterize(&outline, scale, 0.0).unwrap();
                let g = SlugGlyph::from_outline(&outline, font.units_per_em()).unwrap();
                let ppe = px; // pixels per em
                let (mut sum, mut worst, mut n) = (0.0f32, 0.0f32, 0);
                for y in 0..bmp.height {
                    for x in 0..bmp.width {
                        // Pixel centre in em units (bitmap rows go down from `top`).
                        let ex = (bmp.left as f32 + x as f32 + 0.5) / ppe;
                        let ey = -(bmp.top as f32 + y as f32 + 0.5) / ppe;
                        let slug = g.coverage([ex, ey], [ppe, ppe]);
                        let exact = bmp.get(x, y) as f32 / 255.0;
                        let d = (slug - exact).abs();
                        sum += d;
                        worst = worst.max(d);
                        n += 1;
                    }
                }
                let mean = sum / n as f32;
                assert!(mean < 0.03, "{face:?} {c}: mean difference {mean}");
                assert!(worst < 0.6, "{face:?} {c}: worst difference {worst}");
            }
        }
    }

    fn chars() -> Vec<char> {
        ('A'..='Z').chain('a'..='z').chain('\u{410}'..='\u{44F}').chain('\u{391}'..='\u{3A9}').filter(|c| *c != '\u{3A2}').collect()
    }

    #[test]
    fn the_store_appends_once_and_remembers_a_glyph_that_did_not_fit() {
        let font = Bundled::UiRegular.load().unwrap();
        let mut s = SlugStore::new(64 * 1024);
        let id = s.id();
        let a = s.glyph(&font, font.glyph_index('a').unwrap()).unwrap().unwrap();
        let len = s.words().len();
        assert_eq!(s.glyph(&font, font.glyph_index('a').unwrap()).unwrap(), Some(a), "cached");
        assert_eq!(s.words().len(), len);
        assert_eq!(s.glyph(&font, font.glyph_index(' ').unwrap()).unwrap(), None, "a space has no curves");
        // Fill it within one frame: the cap fails the glyph, nothing moves.
        let mut full = None;
        for c in chars() {
            let g = font.glyph_index(c).unwrap();
            if let Err(e) = s.glyph(&font, g) {
                full = Some((g, e));
                break;
            }
        }
        let (g, e) = full.expect("64 KiB holds fewer glyphs than that");
        assert_eq!(e.code(), "glyph_store_full");
        assert_eq!(s.id(), id, "nothing moves within a frame");
        assert_eq!(s.glyph(&font, font.glyph_index('a').unwrap()).unwrap(), Some(a));
        // Review of cd954bd, major 1: a refused glyph is remembered, never prepared again.
        let prepared = s.prepared;
        for _ in 0..10 {
            assert_eq!(s.glyph(&font, g).unwrap_err().code(), "glyph_store_full");
        }
        assert_eq!(s.prepared, prepared, "refused from memory");
        assert!(s.is_refused(&font, g));
    }

    /// Review of cd954bd, major 1: a scene that needs more than a store holds converges: the
    /// same glyphs draw every frame, no glyph is prepared twice, and the store never starts over.
    #[test]
    fn a_scene_larger_than_a_full_store_converges_without_resets() {
        let font = Bundled::UiRegular.load().unwrap();
        let mut s = SlugStore::new(32 * 1024);
        let cs = chars();
        let mut drawn_before = None;
        let mut prepared_after_first = 0;
        for frame in 0..20 {
            s.begin_frame();
            let drawn: Vec<bool> = cs.iter().map(|&c| s.glyph(&font, font.glyph_index(c).unwrap()).is_ok()).collect();
            assert!(drawn.iter().any(|d| !d), "the scene is larger than the store");
            if let Some(before) = &drawn_before {
                assert_eq!(&drawn, before, "frame {frame}: the same glyphs draw (no flicker)");
                assert_eq!(s.prepared, prepared_after_first, "frame {frame}: nothing prepared again");
            } else {
                prepared_after_first = s.prepared;
            }
            drawn_before = Some(drawn);
        }
        assert_eq!(s.resets, 0, "a store the scene fills is never thrown away");
    }

    /// A growing store makes room by growing (same id: the GPU keeps what it has) up to its
    /// maximum, then the scene draws whole; a changed scene evicts the least recently used
    /// glyphs, and the kept ones are copied (moved), not prepared again.
    #[test]
    fn a_growing_store_grows_in_place_then_evicts_what_went_unused() {
        let font = Bundled::UiRegular.load().unwrap();
        // Latin takes about 44 KB of words, Cyrillic and Greek about 74 KB.
        let mut s = SlugStore::growing(16 * 1024, 64 * 1024);
        let cs = chars();
        let (latin, rest) = cs.split_at(52);
        let id = s.id();
        let mut steady = 0;
        for frame in 0..6 {
            s.begin_frame();
            let ok = latin.iter().filter(|&&c| s.glyph(&font, font.glyph_index(c).unwrap()).is_ok()).count();
            if frame == 3 {
                steady = s.prepared;
            }
            if frame >= 3 {
                assert_eq!(ok, latin.len(), "frame {frame}: grown, the whole scene draws");
                assert_eq!(s.prepared, steady, "frame {frame}: nothing prepared again");
            }
        }
        assert!(s.grows >= 1 && s.cap_bytes() > 16 * 1024, "grew to {}", s.cap_bytes());
        assert_eq!(s.id(), id, "growing keeps the id (no re-upload)");
        assert!(s.prepared as usize <= 2 * latin.len(), "a refusal is retried once, after the store grew: {}", s.prepared);
        let a = font.glyph_index('a').unwrap();
        let encoded_at = |at: u32| {
            let mut w = vec![0u32; at as usize];
            SlugGlyph::of(&font, a).unwrap().encode(&mut w);
            w.split_off(at as usize)
        };
        let before = s.glyph(&font, a).unwrap().unwrap();
        let a_words = encoded_at(before.word);
        assert_eq!(&s.words()[before.word as usize..before.word as usize + a_words.len()], &a_words[..], "where it was put");
        // The scene moves on to Cyrillic and Greek (and keeps 'a'): Latin goes unused, and once
        // the store is full of it, the unused glyphs are evicted and the rest moved.
        let (prepared, made) = (s.prepared, s.resets + s.grows);
        let mut moved = false;
        let mut settled = 0;
        for frame in 0..16 {
            s.begin_frame();
            s.glyph(&font, a).unwrap();
            for &c in rest {
                let _ = s.glyph(&font, font.glyph_index(c).unwrap());
            }
            moved |= s.id() != id;
            if frame == 10 {
                settled = s.prepared;
            }
        }
        assert!(moved, "evicted (a new id)");
        assert_eq!(s.prepared, settled, "the new scene converges: nothing prepared again");
        // Each glyph is prepared once, and a refused one once more each time room was made.
        let rooms = s.resets + s.grows - made;
        assert!(s.prepared - prepared <= rest.len() as u64 * (1 + rooms), "{} preparations, {rooms} room makings", s.prepared - prepared);
        let after = s.glyph(&font, a).unwrap().unwrap();
        assert_ne!(after.word, before.word, "'a' moved");
        assert_eq!(&s.words()[after.word as usize..after.word as usize + a_words.len()], &encoded_at(after.word)[..], "moved glyphs point at their own words");
    }

    #[test]
    fn relocating_a_glyph_is_encoding_it_at_its_new_place() {
        let font = Bundled::UiRegular.load().unwrap();
        for c in ['g', '@', 'Ж', 'ß'] {
            let g = SlugGlyph::of(&font, font.glyph_index(c).unwrap()).unwrap();
            let mut at_7 = vec![0u32; 7];
            g.encode(&mut at_7);
            let mut at_300 = vec![0u32; 300];
            g.encode(&mut at_300);
            let mut moved = at_300[300..].to_vec();
            relocate(&mut moved, 7 - 300);
            assert_eq!(moved, at_7[7..].to_vec(), "{c}");
        }
    }

    /// A steady scene keeps its store frame after frame, a capture of another view between its
    /// frames doesn't make its glyphs look stale, and a headless engine (captures only) makes room
    /// after a small view.
    #[test]
    fn steady_scenes_and_captures_keep_their_store() {
        let font = Bundled::UiRegular.load().unwrap();
        let cs = chars();
        let mut s = SlugStore::new(64 * 1024);
        let mut n = 0;
        for &c in &cs {
            if s.glyph(&font, font.glyph_index(c).unwrap()).is_err() {
                break;
            }
            n += 1;
            if s.bytes() * 4 > s.cap_bytes() * 3 {
                break;
            }
        }
        let id = s.id();
        // Each window frame: begin, then draw (as the engine does).
        for _ in 0..3 {
            s.begin_frame();
            assert_eq!(s.id(), id, "a full store the frame keeps using isn't thrown away");
            for &c in &cs[..n] {
                s.glyph(&font, font.glyph_index(c).unwrap()).unwrap();
            }
        }
        s.begin_capture();
        assert!(s.last_frame_words() * 4 > s.cap_bytes() / 2, "{}", s.last_frame_words());
        s.glyph(&font, font.glyph_index('A').unwrap()).unwrap();
        s.begin_frame();
        assert_eq!(s.id(), id, "a capture doesn't make the scene's glyphs stale");
        for &c in &cs[..n] {
            s.glyph(&font, font.glyph_index(c).unwrap()).unwrap();
        }
        // Unused for two frames: stale, and the unused glyphs go.
        s.begin_frame();
        s.begin_frame();
        s.begin_frame();
        assert_ne!(s.id(), id);
        assert!(s.words().len() * 4 <= s.cap_bytes() / 2, "the most recently used half is kept");
        // Headless: captures only.
        let mut s = SlugStore::new(64 * 1024);
        s.begin_capture();
        for &c in &cs[..n] {
            s.glyph(&font, font.glyph_index(c).unwrap()).unwrap();
        }
        let id = s.id();
        s.begin_capture();
        assert_eq!(s.id(), id, "the view the last capture used is kept");
        s.glyph(&font, font.glyph_index('A').unwrap()).unwrap();
        s.begin_capture();
        s.glyph(&font, font.glyph_index('A').unwrap()).unwrap();
        s.begin_capture();
        assert_ne!(s.id(), id, "three quarters full and the last captures used little: room made");
        assert!(s.words().len() * 4 <= s.cap_bytes() / 2, "at most half the store is kept (the most recently used)");
    }

    #[test]
    fn hostile_outlines_are_refused() {
        let mut o = Outline::default();
        o.points.push(crate::outline::Point { x: f32::NAN, y: 0.0, on_curve: true });
        o.points.push(crate::outline::Point { x: 1.0, y: 1.0, on_curve: true });
        o.contour_ends.push(2);
        assert_eq!(SlugGlyph::from_outline(&o, 1000).unwrap_err(), SlugError::NotFinite);
        let mut o = Outline::default();
        for i in 0..(MAX_GLYPH_CURVES + 10) {
            o.points.push(crate::outline::Point { x: i as f32, y: (i % 2) as f32, on_curve: true });
        }
        o.contour_ends.push(o.points.len());
        assert_eq!(SlugGlyph::from_outline(&o, 1000).unwrap_err().code(), "too_many_curves");
        // 1,000 tall zigzag curves: every band crosses all of them.
        let mut o = Outline::default();
        for i in 0..1000 {
            o.points.push(crate::outline::Point { x: i as f32, y: if i % 2 == 0 { 0.0 } else { 1000.0 }, on_curve: true });
        }
        o.contour_ends.push(o.points.len());
        assert_eq!(SlugGlyph::from_outline(&o, 1000).unwrap_err().code(), "band_too_full");
        assert_eq!(band_of(f32::NAN, 0.0, 1.0, 8), 0);
        assert_eq!(band_of(5.0, 0.0, 1.0, 8), 7);
    }
}
