//! TrueType / OpenType (`glyf` outlines) font parsing: table directory, `head`, `hhea`, `maxp`,
//! `hmtx`, `cmap` (formats 4 and 12), `loca`/`glyf`, `kern` (format 0), `GPOS` pair kerning,
//! `name` and `OS/2`.
//!
//! Every read is bounds-checked; malformed input produces a [`FontError`], never a panic.
//! Tables are validated at load time so the per-glyph metric queries can be infallible.

use crate::error::{FontError, Reason, Tag};
use crate::reader::View;
use std::sync::atomic::{AtomicU32, Ordering};

/// A glyph index into the font (`0` is `.notdef`).
pub type GlyphId = u16;

/// Identifies a loaded [`Font`] for caching (atlas keys). Unique per process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

static NEXT_FONT_ID: AtomicU32 = AtomicU32::new(1);

/// Vertical metrics in font units (y up). `descent` is negative below the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineMetrics {
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
}

impl LineMetrics {
    /// Baseline-to-baseline distance in font units.
    pub fn line_height(&self) -> i32 {
        self.ascent as i32 - self.descent as i32 + self.line_gap as i32
    }
}

/// Which `cmap` subtable is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cmap {
    /// Absolute offset of the subtable in the file.
    offset: usize,
    format: u16,
    /// Symbol encoding (3,0): characters U+0020..U+00FF are also looked up at U+F020..U+F0FF.
    symbol: bool,
}

#[derive(Clone, Copy, Debug)]
struct Range {
    offset: usize,
    len: usize,
}

/// A parsed font. Owns its bytes; cheap queries parse the tables lazily.
pub struct Font {
    data: Box<[u8]>,
    id: FontId,
    units_per_em: u16,
    loca_long: bool,
    num_glyphs: u16,
    num_hmetrics: u16,
    line: LineMetrics,
    bbox: [i16; 4],
    cap_height: Option<i16>,
    x_height: Option<i16>,
    family: String,
    hmtx: Range,
    loca: Range,
    glyf: Range,
    cmap: Option<Cmap>,
    /// `kern` format 0 pairs: key `(left << 16) | right`, sorted by key.
    kern_pairs: Vec<(u32, i16)>,
    /// GPOS `kern` lookups: each lookup is a list of absolute offsets of PairPos subtables.
    gpos_kern: Vec<Vec<usize>>,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("id", &self.id)
            .field("family", &self.family)
            .field("units_per_em", &self.units_per_em)
            .field("num_glyphs", &self.num_glyphs)
            .field("bytes", &self.data.len())
            .finish()
    }
}

const TTCF: u32 = u32::from_be_bytes(*b"ttcf");
const TRUE: u32 = u32::from_be_bytes(*b"true");
const OTTO: u32 = u32::from_be_bytes(*b"OTTO");

/// Number of fonts in a file: 1 for a plain font, N for a `.ttc` collection.
pub fn font_count(data: &[u8]) -> Result<u32, FontError> {
    let v = View::new(data, 0, Tag::HEADER);
    let magic = v.u32(0)?;
    match magic {
        TTCF => v.u32(8),
        0x0001_0000 | TRUE | OTTO => Ok(1),
        found => Err(v.err(0, Reason::BadMagic { found })),
    }
}

fn table_directory(file: View<'_>, start: usize) -> Result<Vec<(Tag, Range)>, FontError> {
    if start > file.len() {
        return Err(file.err(start, Reason::Truncated { needed: 12, available: 0 }));
    }
    let magic = file.u32(start)?;
    match magic {
        0x0001_0000 | TRUE | OTTO => {}
        found => return Err(file.err(start, Reason::BadMagic { found })),
    }
    let num_tables = file.u16(start + 4)? as usize;
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let rec = start + 12 + i * 16;
        let tag = Tag(file.bytes(rec, 4)?.try_into().unwrap_or([0; 4]));
        let offset = file.u32(rec + 8)? as usize;
        let mut len = file.u32(rec + 12)? as usize;
        if offset > file.len() {
            return Err(FontError::new(tag, offset, Reason::Truncated { needed: len, available: 0 }));
        }
        // Lenient: some real fonts declare a last table that runs a few padding bytes past EOF.
        len = len.min(file.len() - offset);
        tables.push((tag, Range { offset, len }));
    }
    Ok(tables)
}

impl Font {
    /// Parse a font file (`.ttf`, or the first font of a `.ttc` collection).
    pub fn from_bytes(data: Vec<u8>) -> Result<Font, FontError> {
        Font::from_collection(data, 0)
    }

    /// Parse font number `index` of a collection (`index` 0 for a plain font file).
    pub fn from_collection(data: Vec<u8>, index: u32) -> Result<Font, FontError> {
        let data = data.into_boxed_slice();
        let parsed = Parsed::parse(&data, index)?;
        Ok(Font {
            data,
            id: FontId(NEXT_FONT_ID.fetch_add(1, Ordering::Relaxed)),
            units_per_em: parsed.units_per_em,
            loca_long: parsed.loca_long,
            num_glyphs: parsed.num_glyphs,
            num_hmetrics: parsed.num_hmetrics,
            line: parsed.line,
            bbox: parsed.bbox,
            cap_height: parsed.cap_height,
            x_height: parsed.x_height,
            family: parsed.family,
            hmtx: parsed.hmtx,
            loca: parsed.loca,
            glyf: parsed.glyf,
            cmap: parsed.cmap,
            kern_pairs: parsed.kern_pairs,
            gpos_kern: parsed.gpos_kern,
        })
    }

    pub fn id(&self) -> FontId {
        self.id
    }

    /// The raw font file bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Family name from the `name` table (typographic family preferred), or `""`.
    pub fn family_name(&self) -> &str {
        &self.family
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn num_glyphs(&self) -> u16 {
        self.num_glyphs
    }

    /// Scale factor from font units to pixels for a font size of `px` pixels per em.
    pub fn scale_for_px(&self, px: f32) -> f32 {
        px / self.units_per_em as f32
    }

    /// Ascent / descent / line gap in font units: OS/2 typo metrics when the font asks for them
    /// (`USE_TYPO_METRICS`), otherwise `hhea`.
    pub fn line_metrics(&self) -> LineMetrics {
        self.line
    }

    /// Font bounding box `[x_min, y_min, x_max, y_max]` from `head`, font units.
    pub fn bbox(&self) -> [i16; 4] {
        self.bbox
    }

    /// Capital letter height (OS/2 v2+, else measured from 'H'), font units.
    pub fn cap_height(&self) -> Option<i16> {
        self.cap_height
    }

    /// Lower-case x height (OS/2 v2+, else measured from 'x'), font units.
    pub fn x_height(&self) -> Option<i16> {
        self.x_height
    }

    /// True if the font has any kerning (GPOS `kern` lookups or a `kern` table).
    pub fn has_kerning(&self) -> bool {
        !self.gpos_kern.is_empty() || !self.kern_pairs.is_empty()
    }

    /// True if kerning comes from `GPOS` (otherwise from the legacy `kern` table, if any).
    pub fn has_gpos_kerning(&self) -> bool {
        !self.gpos_kern.is_empty()
    }

    fn view(&self, r: Range, tag: &[u8; 4]) -> View<'_> {
        View::new(&self.data[r.offset..r.offset + r.len], r.offset, Tag(*tag))
    }

    /// Advance width of a glyph in font units (0 for out-of-range glyphs).
    pub fn advance_width(&self, glyph: GlyphId) -> u16 {
        self.h_metrics(glyph).0
    }

    /// `(advance width, left side bearing)` in font units.
    pub fn h_metrics(&self, glyph: GlyphId) -> (u16, i16) {
        if glyph >= self.num_glyphs {
            return (0, 0);
        }
        let v = self.view(self.hmtx, b"hmtx");
        let n = self.num_hmetrics as usize;
        let g = glyph as usize;
        if g < n {
            (v.u16(g * 4).unwrap_or(0), v.i16(g * 4 + 2).unwrap_or(0))
        } else {
            let adv = v.u16((n - 1) * 4).unwrap_or(0);
            (adv, v.i16(n * 4 + (g - n) * 2).unwrap_or(0))
        }
    }

    /// Glyph for a character, or `None` if the font has no mapping (render `.notdef`, glyph 0).
    pub fn glyph_index(&self, c: char) -> Option<GlyphId> {
        let cmap = self.cmap?;
        let v = View::new(&self.data, 0, Tag(*b"cmap"));
        let code = c as u32;
        let mut g = lookup_cmap(v, cmap, code).unwrap_or(0);
        if g == 0 && cmap.symbol && (0x20..=0xff).contains(&code) {
            g = lookup_cmap(v, cmap, 0xf000 + code).unwrap_or(0);
        }
        if g == 0 || g >= self.num_glyphs as u32 {
            None
        } else {
            Some(g as GlyphId)
        }
    }

    /// Horizontal kerning adjustment between two glyphs, font units (added to the left glyph's
    /// advance). GPOS pair positioning when present, otherwise the `kern` table.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> i16 {
        if !self.gpos_kern.is_empty() {
            let v = View::new(&self.data, 0, Tag(*b"GPOS"));
            let mut total = 0i32;
            for lookup in &self.gpos_kern {
                for &sub in lookup {
                    if let Ok(Some(k)) = pair_pos(v, sub, left, right) {
                        total += k as i32;
                        break;
                    }
                }
            }
            return total.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
        let key = ((left as u32) << 16) | right as u32;
        match self.kern_pairs.binary_search_by_key(&key, |p| p.0) {
            Ok(i) => self.kern_pairs[i].1,
            Err(_) => 0,
        }
    }

    /// The raw `glyf` record of a glyph (empty for glyphs with no outline, e.g. space).
    pub(crate) fn glyph_view(&self, glyph: GlyphId) -> Result<View<'_>, FontError> {
        if glyph >= self.num_glyphs {
            return Err(FontError::new(
                Tag(*b"glyf"),
                self.glyf.offset,
                Reason::GlyphId { glyph: glyph as u32, num_glyphs: self.num_glyphs as u32 },
            ));
        }
        let loca = self.view(self.loca, b"loca");
        let g = glyph as usize;
        let (start, end) = if self.loca_long {
            (loca.u32(g * 4)? as usize, loca.u32(g * 4 + 4)? as usize)
        } else {
            (loca.u16(g * 2)? as usize * 2, loca.u16(g * 2 + 2)? as usize * 2)
        };
        let glyf = self.view(self.glyf, b"glyf");
        if end < start {
            return Err(loca.bad(g * if self.loca_long { 4 } else { 2 }, "loca offsets decrease", end as i64));
        }
        glyf.sub(start, end - start)
    }
}

/// Result of parsing everything at load time.
struct Parsed {
    units_per_em: u16,
    loca_long: bool,
    num_glyphs: u16,
    num_hmetrics: u16,
    line: LineMetrics,
    bbox: [i16; 4],
    cap_height: Option<i16>,
    x_height: Option<i16>,
    family: String,
    hmtx: Range,
    loca: Range,
    glyf: Range,
    cmap: Option<Cmap>,
    kern_pairs: Vec<(u32, i16)>,
    gpos_kern: Vec<Vec<usize>>,
}

impl Parsed {
    fn parse(data: &[u8], index: u32) -> Result<Parsed, FontError> {
        let file = View::new(data, 0, Tag::HEADER);
        let magic = file.u32(0)?;
        let start = if magic == TTCF {
            let count = file.u32(8)?;
            if index >= count {
                return Err(file.err(8, Reason::FontIndex { index, count }));
            }
            file.u32(12 + index as usize * 4)? as usize
        } else if index != 0 {
            return Err(file.err(0, Reason::FontIndex { index, count: 1 }));
        } else {
            0
        };
        let tables = table_directory(file, start)?;
        let find = |tag: &[u8; 4]| tables.iter().find(|t| t.0 .0 == *tag).map(|t| t.1);
        let table = |tag: &[u8; 4]| -> Result<View<'_>, FontError> {
            match find(tag) {
                Some(r) => Ok(View::new(&data[r.offset..r.offset + r.len], r.offset, Tag(*tag))),
                None => Err(FontError::new(Tag(*tag), start, Reason::MissingTable)),
            }
        };

        // Outline format first: CFF fonts get a clear error rather than "missing glyf".
        if find(b"glyf").is_none() {
            if find(b"CFF ").is_some() {
                return Err(FontError::new(Tag(*b"CFF "), start, Reason::UnsupportedOutline { format: "CFF" }));
            }
            if find(b"CFF2").is_some() {
                return Err(FontError::new(Tag(*b"CFF2"), start, Reason::UnsupportedOutline { format: "CFF2" }));
            }
        }

        let head = table(b"head")?;
        let units_per_em = head.u16(18)?;
        if units_per_em == 0 {
            return Err(head.bad(18, "head.unitsPerEm", 0));
        }
        let bbox = [head.i16(36)?, head.i16(38)?, head.i16(40)?, head.i16(42)?];
        let loca_format = head.i16(50)?;
        if !(0..=1).contains(&loca_format) {
            return Err(head.bad(50, "head.indexToLocFormat", loca_format as i64));
        }
        let loca_long = loca_format == 1;

        let maxp = table(b"maxp")?;
        let num_glyphs = maxp.u16(4)?;
        if num_glyphs == 0 {
            return Err(maxp.bad(4, "maxp.numGlyphs", 0));
        }

        let hhea = table(b"hhea")?;
        let hhea_line = LineMetrics { ascent: hhea.i16(4)?, descent: hhea.i16(6)?, line_gap: hhea.i16(8)? };
        let num_hmetrics = hhea.u16(34)?;
        if num_hmetrics == 0 || num_hmetrics > num_glyphs {
            return Err(hhea.bad(34, "hhea.numberOfHMetrics", num_hmetrics as i64));
        }

        let hmtx = table(b"hmtx")?;
        let need = num_hmetrics as usize * 4 + (num_glyphs - num_hmetrics) as usize * 2;
        if hmtx.len() < num_hmetrics as usize * 4 {
            return Err(hmtx.err(0, Reason::Truncated { needed: need, available: hmtx.len() }));
        }

        let loca = table(b"loca")?;
        let loca_need = (num_glyphs as usize + 1) * if loca_long { 4 } else { 2 };
        if loca.len() < loca_need {
            return Err(loca.err(0, Reason::Truncated { needed: loca_need, available: loca.len() }));
        }
        table(b"glyf")?;

        // OS/2 is optional (required on Windows, absent from some Apple fonts).
        let mut line = hhea_line;
        let mut cap_height = None;
        let mut x_height = None;
        if let Ok(os2) = table(b"OS/2") {
            let version = os2.u16(0).unwrap_or(0);
            let typo = (|| -> Result<LineMetrics, FontError> {
                Ok(LineMetrics { ascent: os2.i16(68)?, descent: os2.i16(70)?, line_gap: os2.i16(72)? })
            })();
            let use_typo = os2.u16(62).map(|s| s & (1 << 7) != 0).unwrap_or(false);
            if let Ok(typo) = typo {
                if use_typo || (line.ascent == 0 && line.descent == 0) {
                    line = typo;
                }
            }
            if line.ascent == 0 && line.descent == 0 {
                if let (Ok(a), Ok(d)) = (os2.u16(74), os2.u16(76)) {
                    line = LineMetrics {
                        ascent: a.min(i16::MAX as u16) as i16,
                        descent: -(d.min(i16::MAX as u16) as i16),
                        line_gap: 0,
                    };
                }
            }
            if version >= 2 {
                x_height = os2.i16(86).ok().filter(|&v| v > 0);
                cap_height = os2.i16(88).ok().filter(|&v| v > 0);
            }
        }
        if line.ascent == 0 && line.descent == 0 {
            // Last resort: the font bounding box.
            line = LineMetrics { ascent: bbox[3], descent: bbox[1], line_gap: 0 };
        }

        let cmap = table(b"cmap").ok().and_then(|v| choose_cmap(v).ok().flatten());
        let family = table(b"name").ok().and_then(|v| family_name(v).ok().flatten()).unwrap_or_default();
        let gpos_kern = table(b"GPOS").ok().and_then(|v| gpos_kern_lookups(v).ok()).unwrap_or_default();
        let kern_pairs = if gpos_kern.is_empty() {
            table(b"kern").ok().and_then(|v| kern_table(v).ok()).unwrap_or_default()
        } else {
            Vec::new()
        };

        let hmtx_r = find(b"hmtx").unwrap_or(Range { offset: 0, len: 0 });
        let loca_r = find(b"loca").unwrap_or(Range { offset: 0, len: 0 });
        let glyf_r = find(b"glyf").unwrap_or(Range { offset: 0, len: 0 });
        let mut parsed = Parsed {
            units_per_em,
            loca_long,
            num_glyphs,
            num_hmetrics,
            line,
            bbox,
            cap_height,
            x_height,
            family,
            hmtx: hmtx_r,
            loca: loca_r,
            glyf: glyf_r,
            cmap,
            kern_pairs,
            gpos_kern,
        };
        // Measure cap / x height from the outlines when OS/2 does not give them.
        if parsed.cap_height.is_none() {
            parsed.cap_height = parsed.measure_top(data, 'H');
        }
        if parsed.x_height.is_none() {
            parsed.x_height = parsed.measure_top(data, 'x');
        }
        Ok(parsed)
    }

    /// The glyph's `yMax` from its `glyf` header (used for cap/x height fallbacks).
    fn measure_top(&self, data: &[u8], c: char) -> Option<i16> {
        let cmap = self.cmap?;
        let g = lookup_cmap(View::new(data, 0, Tag(*b"cmap")), cmap, c as u32).ok()?;
        if g == 0 || g >= self.num_glyphs as u32 {
            return None;
        }
        let loca = View::new(&data[self.loca.offset..self.loca.offset + self.loca.len], self.loca.offset, Tag(*b"loca"));
        let g = g as usize;
        let start = if self.loca_long { loca.u32(g * 4).ok()? as usize } else { loca.u16(g * 2).ok()? as usize * 2 };
        let glyf = View::new(&data[self.glyf.offset..self.glyf.offset + self.glyf.len], self.glyf.offset, Tag(*b"glyf"));
        glyf.i16(start + 8).ok().filter(|&v| v > 0)
    }
}

/// Pick the best Unicode `cmap` subtable.
fn choose_cmap(v: View<'_>) -> Result<Option<Cmap>, FontError> {
    let count = v.u16(2)? as usize;
    // (score, cmap) — higher score wins.
    let mut best: Option<(u32, Cmap)> = None;
    for i in 0..count {
        let rec = 4 + i * 8;
        let platform = v.u16(rec)?;
        let encoding = v.u16(rec + 2)?;
        let offset = v.u32(rec + 4)? as usize;
        let Ok(format) = v.u16(offset) else { continue };
        let full = format == 12;
        if format != 4 && format != 12 {
            continue;
        }
        let score = match (platform, encoding) {
            (3, 10) | (0, 4) | (0, 6) if full => 6,
            (0, _) if full => 5,
            (3, 1) => 4,
            (0, _) => 3,
            (3, 0) => 1,
            _ => continue,
        };
        let cmap = Cmap { offset: v.base + offset, format, symbol: platform == 3 && encoding == 0 };
        if best.map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, cmap));
        }
    }
    Ok(best.map(|b| b.1))
}

/// Map a code point through a `cmap` subtable. `v` views the whole file.
fn lookup_cmap(v: View<'_>, cmap: Cmap, code: u32) -> Result<u32, FontError> {
    let o = cmap.offset;
    match cmap.format {
        4 => {
            if code > 0xffff {
                return Ok(0);
            }
            let seg_x2 = v.u16(o + 6)? as usize;
            let seg_count = seg_x2 / 2;
            if seg_count == 0 {
                return Ok(0);
            }
            let ends = o + 14;
            let starts = ends + seg_x2 + 2;
            let deltas = starts + seg_x2;
            let ranges = deltas + seg_x2;
            // Binary search for the first segment whose end >= code.
            let (mut lo, mut hi) = (0usize, seg_count);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if (v.u16(ends + mid * 2)? as u32) < code {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            if lo >= seg_count {
                return Ok(0);
            }
            let start = v.u16(starts + lo * 2)? as u32;
            if code < start {
                return Ok(0);
            }
            let delta = v.u16(deltas + lo * 2)?;
            let range_offset = v.u16(ranges + lo * 2)? as usize;
            if range_offset == 0 {
                Ok(((code as u16).wrapping_add(delta)) as u32)
            } else {
                let at = ranges + lo * 2 + range_offset + (code - start) as usize * 2;
                let g = v.u16(at)?;
                if g == 0 {
                    Ok(0)
                } else {
                    Ok(g.wrapping_add(delta) as u32)
                }
            }
        }
        12 => {
            let groups = v.u32(o + 12)? as usize;
            let (mut lo, mut hi) = (0usize, groups);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let rec = mid.saturating_mul(12).saturating_add(o + 16);
                let start = v.u32(rec)?;
                let end = v.u32(rec + 4)?;
                if code < start {
                    hi = mid;
                } else if code > end {
                    lo = mid + 1;
                } else {
                    let g = v.u32(rec + 8)?;
                    return Ok(g.wrapping_add(code - start));
                }
            }
            Ok(0)
        }
        f => Err(v.err(o, Reason::UnsupportedFormat { format: f as u32 })),
    }
}

/// Family name: typographic family (16) preferred over family (1); Windows English preferred.
fn family_name(v: View<'_>) -> Result<Option<String>, FontError> {
    let count = v.u16(2)? as usize;
    let storage = v.u16(4)? as usize;
    let mut best: Option<(u32, String)> = None;
    for i in 0..count {
        let rec = 6 + i * 12;
        let platform = v.u16(rec)?;
        let encoding = v.u16(rec + 2)?;
        let language = v.u16(rec + 4)?;
        let name_id = v.u16(rec + 6)?;
        let len = v.u16(rec + 8)? as usize;
        let off = v.u16(rec + 10)? as usize;
        let id_score = match name_id {
            16 => 20,
            1 => 10,
            _ => continue,
        };
        let Ok(bytes) = v.bytes(storage + off, len) else { continue };
        let (plat_score, text) = match (platform, encoding) {
            (3, 1) | (3, 10) | (0, _) => {
                let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                let lang = if platform == 3 && language == 0x409 { 3 } else { 2 };
                (lang, String::from_utf16_lossy(&units))
            }
            // Mac Roman: ASCII subset exactly, other bytes as Latin-1 (close enough for a name).
            (1, 0) => (1, bytes.iter().map(|&b| b as char).collect()),
            _ => continue,
        };
        let score = id_score + plat_score;
        if !text.is_empty() && best.as_ref().map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, text));
        }
    }
    Ok(best.map(|b| b.1))
}

/// Horizontal format-0 subtables of the legacy `kern` table (Microsoft and Apple headers).
fn kern_table(v: View<'_>) -> Result<Vec<(u32, i16)>, FontError> {
    let mut pairs: Vec<(u32, i16)> = Vec::new();
    let version = v.u16(0)?;
    let (count, mut at, apple) = if version == 0 {
        (v.u16(2)? as usize, 4usize, false)
    } else if v.u32(0)? == 0x0001_0000 {
        (v.u32(4)? as usize, 8usize, true)
    } else {
        return Err(v.err(0, Reason::UnsupportedFormat { format: version as u32 }));
    };
    for _ in 0..count {
        let (len, format, horizontal, cross, header) = if apple {
            let len = v.u32(at)? as usize;
            let coverage = v.u16(at + 4)?;
            (len, coverage & 0xff, coverage & 0x8000 == 0, coverage & 0x4000 != 0, 8)
        } else {
            let len = v.u16(at + 2)? as usize;
            let coverage = v.u16(at + 4)?;
            (len, coverage >> 8, coverage & 1 != 0, coverage & 4 != 0, 6)
        };
        if format == 0 && horizontal && !cross {
            let n = v.u16(at + header)? as usize;
            let base = at + header + 8;
            for i in 0..n {
                let rec = base + i * 6;
                let key = v.u32(rec)?;
                let value = v.i16(rec + 4)?;
                pairs.push((key, value));
            }
            // Microsoft's u16 length overflows for big subtables; trust nPairs for the stride.
            at = base + n * 6;
        } else {
            if len == 0 {
                break;
            }
            at = at.saturating_add(len);
        }
    }
    // Sort and merge duplicates by summing (multiple subtables accumulate).
    pairs.sort_by_key(|p| p.0);
    let mut merged: Vec<(u32, i16)> = Vec::with_capacity(pairs.len());
    for (k, val) in pairs {
        match merged.last_mut() {
            Some(last) if last.0 == k => last.1 = last.1.saturating_add(val),
            _ => merged.push((k, val)),
        }
    }
    Ok(merged)
}

/// Lookups (as lists of absolute PairPos subtable offsets) of the GPOS `kern` feature for the
/// Latin script (falling back to DFLT, then to every `kern` feature).
fn gpos_kern_lookups(v: View<'_>) -> Result<Vec<Vec<usize>>, FontError> {
    let major = v.u16(0)?;
    if major != 1 {
        return Err(v.err(0, Reason::UnsupportedFormat { format: major as u32 }));
    }
    let script_list = v.u16(4)? as usize;
    let feature_list = v.u16(6)? as usize;
    let lookup_list = v.u16(8)? as usize;

    let feature_count = v.u16(feature_list)? as usize;
    let is_kern = |fi: usize| -> bool {
        v.bytes(feature_list + 2 + fi * 6, 4).map(|t| t == b"kern").unwrap_or(false)
    };

    // Feature indices from the default LangSys of 'latn' or 'DFLT'.
    let mut features: Vec<usize> = Vec::new();
    let script_count = v.u16(script_list)? as usize;
    for want in [b"latn", b"DFLT"] {
        for si in 0..script_count {
            let rec = script_list + 2 + si * 6;
            if v.bytes(rec, 4)? != want {
                continue;
            }
            let script = script_list + v.u16(rec + 4)? as usize;
            let default_lang = v.u16(script)? as usize;
            if default_lang == 0 {
                continue;
            }
            let lang = script + default_lang;
            let n = v.u16(lang + 4)? as usize;
            for k in 0..n {
                let fi = v.u16(lang + 6 + k * 2)? as usize;
                if fi < feature_count && is_kern(fi) {
                    features.push(fi);
                }
            }
        }
        if !features.is_empty() {
            break;
        }
    }
    if features.is_empty() {
        features = (0..feature_count).filter(|&fi| is_kern(fi)).collect();
    }

    let mut lookup_indices: Vec<usize> = Vec::new();
    for fi in features {
        let feature = feature_list + v.u16(feature_list + 2 + fi * 6 + 4)? as usize;
        let n = v.u16(feature + 2)? as usize;
        for k in 0..n {
            lookup_indices.push(v.u16(feature + 4 + k * 2)? as usize);
        }
    }
    lookup_indices.sort_unstable();
    lookup_indices.dedup();

    let lookup_count = v.u16(lookup_list)? as usize;
    let mut lookups = Vec::new();
    for li in lookup_indices {
        if li >= lookup_count {
            continue;
        }
        let lookup = lookup_list + v.u16(lookup_list + 2 + li * 2)? as usize;
        let kind = v.u16(lookup)?;
        let subs = v.u16(lookup + 4)? as usize;
        let mut list = Vec::new();
        for s in 0..subs {
            let mut sub = lookup + v.u16(lookup + 6 + s * 2)? as usize;
            let mut sub_kind = kind;
            if kind == 9 {
                // Extension: format(1), extensionLookupType, u32 offset from this subtable.
                sub_kind = v.u16(sub + 2)?;
                sub = sub.saturating_add(v.u32(sub + 4)? as usize);
            }
            if sub_kind == 2 {
                let format = v.u16(sub)?;
                if format == 1 || format == 2 {
                    list.push(v.base + sub);
                }
            }
        }
        if !list.is_empty() {
            lookups.push(list);
        }
    }
    Ok(lookups)
}

fn value_record_size(format: u16) -> usize {
    (format & 0xff).count_ones() as usize * 2
}

/// Offset of XAdvance within a value record, if present.
fn x_advance_offset(format: u16) -> Option<usize> {
    if format & 0x4 == 0 {
        None
    } else {
        Some((format & 0x3).count_ones() as usize * 2)
    }
}

/// Coverage index of `glyph`, if covered. `at` is an absolute offset into `v` (whole file).
fn coverage_index(v: View<'_>, at: usize, glyph: GlyphId) -> Result<Option<usize>, FontError> {
    let format = v.u16(at)?;
    let count = v.u16(at + 2)? as usize;
    let g = glyph;
    match format {
        1 => {
            let (mut lo, mut hi) = (0usize, count);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let x = v.u16(at + 4 + mid * 2)?;
                match x.cmp(&g) {
                    std::cmp::Ordering::Less => lo = mid + 1,
                    std::cmp::Ordering::Greater => hi = mid,
                    std::cmp::Ordering::Equal => return Ok(Some(mid)),
                }
            }
            Ok(None)
        }
        2 => {
            let (mut lo, mut hi) = (0usize, count);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let rec = at + 4 + mid * 6;
                let start = v.u16(rec)?;
                let end = v.u16(rec + 2)?;
                if g < start {
                    hi = mid;
                } else if g > end {
                    lo = mid + 1;
                } else {
                    return Ok(Some(v.u16(rec + 4)? as usize + (g - start) as usize));
                }
            }
            Ok(None)
        }
        f => Err(v.err(at, Reason::UnsupportedFormat { format: f as u32 })),
    }
}

fn class_of(v: View<'_>, at: usize, glyph: GlyphId) -> Result<u16, FontError> {
    let format = v.u16(at)?;
    match format {
        1 => {
            let start = v.u16(at + 2)?;
            let count = v.u16(at + 4)?;
            if glyph < start || (glyph - start) >= count {
                return Ok(0);
            }
            v.u16(at + 6 + (glyph - start) as usize * 2)
        }
        2 => {
            let count = v.u16(at + 2)? as usize;
            let (mut lo, mut hi) = (0usize, count);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let rec = at + 4 + mid * 6;
                let start = v.u16(rec)?;
                let end = v.u16(rec + 2)?;
                if glyph < start {
                    hi = mid;
                } else if glyph > end {
                    lo = mid + 1;
                } else {
                    return v.u16(rec + 4);
                }
            }
            Ok(0)
        }
        f => Err(v.err(at, Reason::UnsupportedFormat { format: f as u32 })),
    }
}

/// Apply one PairPos subtable: `Some(x_advance adjustment)` if it applies to the pair.
fn pair_pos(v: View<'_>, sub: usize, left: GlyphId, right: GlyphId) -> Result<Option<i16>, FontError> {
    let format = v.u16(sub)?;
    let coverage = sub + v.u16(sub + 2)? as usize;
    let vf1 = v.u16(sub + 4)?;
    let vf2 = v.u16(sub + 6)?;
    let Some(cov) = coverage_index(v, coverage, left)? else { return Ok(None) };
    let rec_size = value_record_size(vf1) + value_record_size(vf2);
    match format {
        1 => {
            let set_count = v.u16(sub + 8)? as usize;
            if cov >= set_count {
                return Ok(None);
            }
            let set = sub + v.u16(sub + 10 + cov * 2)? as usize;
            let n = v.u16(set)? as usize;
            let stride = 2 + rec_size;
            let (mut lo, mut hi) = (0usize, n);
            while lo < hi {
                let mid = (lo + hi) / 2;
                let rec = set + 2 + mid * stride;
                let second = v.u16(rec)?;
                match second.cmp(&right) {
                    std::cmp::Ordering::Less => lo = mid + 1,
                    std::cmp::Ordering::Greater => hi = mid,
                    std::cmp::Ordering::Equal => {
                        return Ok(Some(match x_advance_offset(vf1) {
                            Some(o) => v.i16(rec + 2 + o)?,
                            None => 0,
                        }));
                    }
                }
            }
            Ok(None)
        }
        2 => {
            let class_def1 = sub + v.u16(sub + 8)? as usize;
            let class_def2 = sub + v.u16(sub + 10)? as usize;
            let class1_count = v.u16(sub + 12)? as usize;
            let class2_count = v.u16(sub + 14)? as usize;
            let c1 = class_of(v, class_def1, left)? as usize;
            let c2 = class_of(v, class_def2, right)? as usize;
            if c1 >= class1_count || c2 >= class2_count {
                return Ok(None);
            }
            let rec = sub + 16 + (c1 * class2_count + c2) * rec_size;
            Ok(Some(match x_advance_offset(vf1) {
                Some(o) => v.i16(rec + o)?,
                None => 0,
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a minimal valid TrueType font in memory: glyph 0 = .notdef (empty), glyph 1 = a
    /// square, glyph 2 = a square with a square hole, glyph 3 = composite of glyph 1 shifted,
    /// glyph 4 = composite with a scaled, point-matched component.
    /// cmap (format 4): 'A' -> 1, 'O' -> 2, 'C' -> 3, ' ' -> 0 is unmapped. units_per_em = 1000.
    pub(crate) fn tiny_font() -> Vec<u8> {
        fn be16(v: &mut Vec<u8>, x: u16) {
            v.extend_from_slice(&x.to_be_bytes());
        }
        fn be32(v: &mut Vec<u8>, x: u32) {
            v.extend_from_slice(&x.to_be_bytes());
        }
        // glyf
        let mut glyf = Vec::new();
        let mut loca = vec![0u32];
        // glyph 0: empty
        loca.push(glyf.len() as u32);
        // glyph 1: square 100..600 x 0..500, one contour, 4 on-curve points (clockwise in y-up).
        let square = |glyf: &mut Vec<u8>, contours: &[[(i16, i16); 4]]| {
            let n_contours = contours.len() as i16;
            let all: Vec<(i16, i16)> = contours.iter().flatten().copied().collect();
            be16(glyf, n_contours as u16);
            let xs = all.iter().map(|p| p.0);
            let ys = all.iter().map(|p| p.1);
            be16(glyf, xs.clone().min().unwrap() as u16);
            be16(glyf, ys.clone().min().unwrap() as u16);
            be16(glyf, xs.max().unwrap() as u16);
            be16(glyf, ys.max().unwrap() as u16);
            for (i, _) in contours.iter().enumerate() {
                be16(glyf, (i * 4 + 3) as u16);
            }
            be16(glyf, 0); // instruction length
            for _ in &all {
                glyf.push(1); // on curve, 16-bit deltas
            }
            let mut last = 0i16;
            for p in &all {
                be16(glyf, (p.0 - last) as u16);
                last = p.0;
            }
            last = 0;
            for p in &all {
                be16(glyf, (p.1 - last) as u16);
                last = p.1;
            }
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        };
        square(&mut glyf, &[[(100, 0), (100, 500), (600, 500), (600, 0)]]);
        loca.push(glyf.len() as u32);
        // glyph 2: outer clockwise, inner counter-clockwise (a hole).
        square(
            &mut glyf,
            &[[(100, 0), (100, 500), (600, 500), (600, 0)], [(250, 150), (450, 150), (450, 350), (250, 350)]],
        );
        loca.push(glyf.len() as u32);
        // glyph 3: composite of glyph 1, offset (+50, +100), ARGS_ARE_WORDS | ARGS_ARE_XY.
        be16(&mut glyf, (-1i16) as u16);
        for v in [150i16, 100, 650, 600] {
            be16(&mut glyf, v as u16);
        }
        be16(&mut glyf, 0x0001 | 0x0002);
        be16(&mut glyf, 1);
        be16(&mut glyf, 50);
        be16(&mut glyf, 100);
        loca.push(glyf.len() as u32);
        // glyph 4: glyph 1 as is, then glyph 1 scaled by 0.5 and placed by point matching
        // (parent point 2 = (600, 500) <- child point 0), giving a second square at (600, 500).
        be16(&mut glyf, (-1i16) as u16);
        for v in [100i16, 0, 850, 750] {
            be16(&mut glyf, v as u16);
        }
        be16(&mut glyf, 0x0020 | 0x0002); // MORE_COMPONENTS | ARGS_ARE_XY (byte args)
        be16(&mut glyf, 1);
        glyf.extend_from_slice(&[0, 0]);
        be16(&mut glyf, 0x0008); // WE_HAVE_A_SCALE, byte point indices
        be16(&mut glyf, 1);
        glyf.extend_from_slice(&[2, 0]);
        be16(&mut glyf, 0x2000); // F2DOT14 0.5
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        loca.push(glyf.len() as u32);
        let num_glyphs = 5u16;

        let mut loca_b = Vec::new();
        for o in &loca {
            be32(&mut loca_b, *o);
        }
        assert_eq!(loca_b.len(), (num_glyphs as usize + 1) * 4);

        let mut head = vec![0u8; 54];
        head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[36..38].copy_from_slice(&0i16.to_be_bytes());
        head[38..40].copy_from_slice(&(-200i16).to_be_bytes());
        head[40..42].copy_from_slice(&1000i16.to_be_bytes());
        head[42..44].copy_from_slice(&800i16.to_be_bytes());
        head[50..52].copy_from_slice(&1i16.to_be_bytes());

        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        hhea[8..10].copy_from_slice(&100i16.to_be_bytes());
        hhea[34..36].copy_from_slice(&3u16.to_be_bytes());

        let mut maxp = vec![0u8; 6];
        maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&num_glyphs.to_be_bytes());

        let mut hmtx = Vec::new();
        for (adv, lsb) in [(500u16, 0i16), (700, 100), (700, 100)] {
            be16(&mut hmtx, adv);
            be16(&mut hmtx, lsb as u16);
        }
        be16(&mut hmtx, 150); // glyph 3 lsb (advance repeats 700)
        be16(&mut hmtx, 100); // glyph 4 lsb

        // cmap format 4 with segments: 'A'(65)->1, 'C'(67)->3 via delta... use 3 segments + 0xFFFF.
        let segs: [(u16, u16, i16); 4] = [(65, 65, 1 - 65), (67, 67, 3 - 67), (79, 79, 2 - 79), (0xffff, 0xffff, 1)];
        let mut sub = Vec::new();
        be16(&mut sub, 4);
        be16(&mut sub, (16 + segs.len() * 8) as u16);
        be16(&mut sub, 0);
        be16(&mut sub, (segs.len() * 2) as u16);
        be16(&mut sub, 8);
        be16(&mut sub, 2);
        be16(&mut sub, 0);
        for s in &segs {
            be16(&mut sub, s.1);
        }
        be16(&mut sub, 0);
        for s in &segs {
            be16(&mut sub, s.0);
        }
        for s in &segs {
            be16(&mut sub, s.2 as u16);
        }
        for _ in &segs {
            be16(&mut sub, 0);
        }
        let mut cmap = Vec::new();
        be16(&mut cmap, 0);
        be16(&mut cmap, 1);
        be16(&mut cmap, 3);
        be16(&mut cmap, 1);
        be32(&mut cmap, 12);
        cmap.extend_from_slice(&sub);

        // kern format 0: (A, O) = -50
        let mut kern = Vec::new();
        be16(&mut kern, 0);
        be16(&mut kern, 1);
        be16(&mut kern, 0);
        be16(&mut kern, 6 + 8 + 6);
        be16(&mut kern, 0x0001);
        be16(&mut kern, 1);
        be16(&mut kern, 6);
        be16(&mut kern, 0);
        be16(&mut kern, 0);
        be16(&mut kern, 1);
        be16(&mut kern, 2);
        be16(&mut kern, (-50i16) as u16);

        // name: family "Tiny" (3,1,0x409, id 1)
        let family: Vec<u8> = "Tiny".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let mut name = Vec::new();
        be16(&mut name, 0);
        be16(&mut name, 1);
        be16(&mut name, 6 + 12);
        for x in [3u16, 1, 0x409, 1, family.len() as u16, 0] {
            be16(&mut name, x);
        }
        name.extend_from_slice(&family);

        let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"cmap", cmap),
            (b"glyf", glyf),
            (b"head", head),
            (b"hhea", hhea),
            (b"hmtx", hmtx),
            (b"kern", kern),
            (b"loca", loca_b),
            (b"maxp", maxp),
            (b"name", name),
        ];
        let mut out = Vec::new();
        be32(&mut out, 0x0001_0000);
        be16(&mut out, tables.len() as u16);
        be16(&mut out, 0);
        be16(&mut out, 0);
        be16(&mut out, 0);
        let mut offset = 12 + tables.len() * 16;
        let mut bodies = Vec::new();
        for (tag, body) in &tables {
            out.extend_from_slice(*tag);
            be32(&mut out, 0);
            be32(&mut out, offset as u32);
            be32(&mut out, body.len() as u32);
            let mut b = body.clone();
            while b.len() % 4 != 0 {
                b.push(0);
            }
            offset += b.len();
            bodies.push(b);
        }
        for b in bodies {
            out.extend_from_slice(&b);
        }
        out
    }

    #[test]
    fn parses_tiny_font() {
        let font = Font::from_bytes(tiny_font()).unwrap();
        assert_eq!(font.units_per_em(), 1000);
        assert_eq!(font.num_glyphs(), 5);
        assert_eq!(font.family_name(), "Tiny");
        assert_eq!(font.line_metrics(), LineMetrics { ascent: 800, descent: -200, line_gap: 100 });
        assert_eq!(font.glyph_index('A'), Some(1));
        assert_eq!(font.glyph_index('O'), Some(2));
        assert_eq!(font.glyph_index('C'), Some(3));
        assert_eq!(font.glyph_index('B'), None);
        assert_eq!(font.glyph_index('\u{1F600}'), None);
        assert_eq!(font.h_metrics(0), (500, 0));
        assert_eq!(font.h_metrics(1), (700, 100));
        assert_eq!(font.h_metrics(3), (700, 150));
        assert_eq!(font.h_metrics(99), (0, 0));
        assert_eq!(font.kerning(1, 2), -50);
        assert_eq!(font.kerning(2, 1), 0);
        assert!(font.has_kerning());
        assert!(!font.has_gpos_kerning());
        // Cap height measured from 'H' is unavailable (no 'H'); x height likewise.
        assert_eq!(font.cap_height(), None);
    }

    #[test]
    fn rejects_garbage_with_structured_errors() {
        let e = Font::from_bytes(vec![1, 2, 3]).unwrap_err();
        assert_eq!(e.reason.code(), "truncated");
        let e = Font::from_bytes(b"wOFFxxxxxxxxxxxxxxxx".to_vec()).unwrap_err();
        assert_eq!(e.reason.code(), "bad_magic");
        assert_eq!(e.table, Tag::HEADER);
        // A font with only a CFF table.
        let mut otf = Vec::new();
        otf.extend_from_slice(b"OTTO");
        otf.extend_from_slice(&1u16.to_be_bytes());
        otf.extend_from_slice(&[0; 6]);
        otf.extend_from_slice(b"CFF ");
        otf.extend_from_slice(&[0; 4]);
        otf.extend_from_slice(&28u32.to_be_bytes());
        otf.extend_from_slice(&0u32.to_be_bytes());
        let e = Font::from_bytes(otf).unwrap_err();
        assert_eq!(e.reason, Reason::UnsupportedOutline { format: "CFF" });
        assert_eq!(e.reason.code(), "unsupported_outline_format");
        assert!(e.to_string().contains("CFF"));
    }

    #[test]
    fn missing_table_names_the_table() {
        let mut data = tiny_font();
        // Rename the 'hmtx' directory entry so it is "missing".
        let pos = data.windows(4).position(|w| w == b"hmtx").unwrap();
        data[pos..pos + 4].copy_from_slice(b"xxxx");
        let e = Font::from_bytes(data).unwrap_err();
        assert_eq!(e.table, Tag(*b"hmtx"));
        assert_eq!(e.reason, Reason::MissingTable);
    }

    #[test]
    fn cmap_format_12_groups() {
        let mut sub = Vec::new();
        sub.extend_from_slice(&12u16.to_be_bytes());
        sub.extend_from_slice(&0u16.to_be_bytes());
        sub.extend_from_slice(&(16u32 + 24).to_be_bytes());
        sub.extend_from_slice(&0u32.to_be_bytes());
        sub.extend_from_slice(&2u32.to_be_bytes());
        for (start, end, glyph) in [(0x41u32, 0x5au32, 10u32), (0x1F600, 0x1F64F, 100)] {
            sub.extend_from_slice(&start.to_be_bytes());
            sub.extend_from_slice(&end.to_be_bytes());
            sub.extend_from_slice(&glyph.to_be_bytes());
        }
        let v = View::new(&sub, 0, Tag(*b"cmap"));
        let cmap = Cmap { offset: 0, format: 12, symbol: false };
        assert_eq!(lookup_cmap(v, cmap, 0x41).unwrap(), 10);
        assert_eq!(lookup_cmap(v, cmap, 0x5a).unwrap(), 35);
        assert_eq!(lookup_cmap(v, cmap, 0x5b).unwrap(), 0);
        assert_eq!(lookup_cmap(v, cmap, 0x1F601).unwrap(), 101);
        assert_eq!(lookup_cmap(v, cmap, 0x10FFFF).unwrap(), 0);
        // A group count larger than the data is an error, not a panic.
        let mut bad = sub.clone();
        bad[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
        let v = View::new(&bad, 0, Tag(*b"cmap"));
        assert!(lookup_cmap(v, cmap, 0x1F601).is_err());
    }

    #[test]
    fn collection_index_checked() {
        let e = Font::from_collection(tiny_font(), 1).unwrap_err();
        assert_eq!(e.reason, Reason::FontIndex { index: 1, count: 1 });
        assert_eq!(font_count(&tiny_font()).unwrap(), 1);
    }
}
