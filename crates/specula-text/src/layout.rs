//! Text layout: UTF-8 → positioned glyphs with kerning → wrapped, aligned lines → quads.
//!
//! Layout works in **physical pixels**: sizes and widths in [`TextStyle`] are logical pixels and
//! are multiplied by [`TextStyle::scale`] (the DPI / UI scale factor). Positions are relative to
//! the top-left of the text box, y down. Line breaking is simplified (T3 adds the Unicode rules):
//! hard breaks at `\n`, soft breaks after whitespace and after a hyphen inside a word, and a
//! fallback break inside a word that is wider than the line.

use crate::atlas::{GlyphAtlas, SUBPIXEL_BINS};
use crate::error::AtlasError;
use crate::font::{Font, GlyphId};
use std::ops::Range;

/// Horizontal alignment of each line inside the text box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// How to lay text out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in logical pixels per em.
    pub size_px: f32,
    /// Physical pixels per logical pixel (DPI scale × UI scale).
    pub scale: f32,
    /// Multiplier on the font's own line height (ascent − descent + line gap).
    pub line_height: f32,
    /// Wrap width in logical pixels; `None` = no wrapping (only `\n` breaks).
    pub max_width: Option<f32>,
    pub align: Align,
    /// Apply the font's pair kerning.
    pub kerning: bool,
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle { size_px: 16.0, scale: 1.0, line_height: 1.0, max_width: None, align: Align::Left, kerning: true }
    }
}

impl TextStyle {
    pub fn new(size_px: f32) -> TextStyle {
        TextStyle { size_px, ..TextStyle::default() }
    }

    pub fn with_scale(mut self, scale: f32) -> TextStyle {
        self.scale = scale;
        self
    }

    pub fn with_max_width(mut self, width: f32) -> TextStyle {
        self.max_width = Some(width);
        self
    }

    pub fn with_align(mut self, align: Align) -> TextStyle {
        self.align = align;
        self
    }

    /// The size actually rasterised, in physical pixels.
    pub fn physical_size(&self) -> f32 {
        let s = self.size_px * self.scale;
        if s.is_finite() {
            s.clamp(0.0, 1024.0)
        } else {
            0.0
        }
    }
}

/// One glyph placed on a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub glyph: GlyphId,
    /// Pen x (physical px from the box's left edge, alignment applied).
    pub x: f32,
    /// Baseline y (physical px from the box's top edge).
    pub y: f32,
    pub advance: f32,
    /// Byte offset of the source character in the text.
    pub byte: u32,
}

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Indices into [`TextLayout::glyphs`].
    pub glyphs: Range<usize>,
    /// Source bytes of the line (trailing spaces included, the `\n` excluded).
    pub bytes: Range<usize>,
    /// Left edge after alignment, physical px.
    pub x: f32,
    /// Ink advance width without trailing whitespace, physical px.
    pub width: f32,
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
}

/// The result of [`layout`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<PositionedGlyph>,
    pub lines: Vec<Line>,
    /// Box size in physical px: the wrap width (or the widest line) × the total line height.
    pub width: f32,
    pub height: f32,
    /// Physical font size used.
    pub size_px: f32,
    pub ascent: f32,
    pub descent: f32,
    /// Characters the font has no glyph for (drawn as `.notdef`), in first-seen order, unique.
    pub missing: Vec<char>,
    /// Reused shaping buffer, so [`layout_into`] does not allocate once warmed up.
    scratch: Scratch,
}

/// Internal per-layout buffer. Ignored by `PartialEq`, emptied by `Clone`.
#[derive(Default)]
struct Scratch(Vec<Item>);

impl Clone for Scratch {
    fn clone(&self) -> Scratch {
        Scratch(Vec::new())
    }
}

impl PartialEq for Scratch {
    fn eq(&self, _: &Scratch) -> bool {
        true
    }
}

impl std::fmt::Debug for Scratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("..")
    }
}

/// A textured quad for the overlay renderer: draw `rect` sampling `uv` from atlas `page`
/// (an R8 coverage texture), tinted by `color` (sRGB, straight alpha, coverage multiplies alpha).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
    /// `[x0, y0, x1, y1]` in physical px, y down. Always whole pixels (1:1 texel mapping).
    pub rect: [f32; 4],
    /// `[u0, v0, u1, v1]` normalised texture coordinates.
    pub uv: [f32; 4],
    pub page: u32,
    pub color: [f32; 4],
}

/// Size of laid-out text, physical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub lines: usize,
}

#[derive(Clone, Copy)]
struct Item {
    glyph: GlyphId,
    byte: u32,
    len: u8,
    advance: f32,
    /// Kerning with the previous item (dropped at a line start).
    kern: f32,
    space: bool,
    /// A soft line break is allowed before this item.
    break_before: bool,
}

fn is_hyphen(c: char) -> bool {
    matches!(c, '-' | '\u{2010}' | '\u{2013}')
}

/// Lay out `text`. Never fails: missing glyphs become `.notdef` and are listed in `missing`.
pub fn layout(font: &Font, text: &str, style: &TextStyle) -> TextLayout {
    let mut out = TextLayout::default();
    layout_into(font, text, style, &mut out);
    out
}

/// [`layout`] into an existing result, reusing its allocations (for per-frame HUD text).
pub fn layout_into(font: &Font, text: &str, style: &TextStyle, out: &mut TextLayout) {
    out.glyphs.clear();
    out.lines.clear();
    out.missing.clear();
    let size = style.physical_size();
    let scale = font.scale_for_px(size);
    let lm = font.line_metrics();
    let ascent = lm.ascent as f32 * scale;
    let descent = -(lm.descent as f32) * scale;
    let line_mul = if style.line_height.is_finite() && style.line_height > 0.0 { style.line_height } else { 1.0 };
    let line_h = (lm.line_height().max(0) as f32 * scale * line_mul).max(0.0);
    let max_w = style
        .max_width
        .map(|w| w * style.scale)
        .filter(|w| w.is_finite())
        .map(|w| w.max(0.0));
    out.size_px = size;
    out.ascent = ascent;
    out.descent = descent;

    let space_glyph = font.glyph_index(' ');
    let space_adv = space_glyph.map(|g| font.advance_width(g)).unwrap_or(font.units_per_em() / 4) as f32 * scale;

    let mut items = std::mem::take(&mut out.scratch.0);
    let mut para_start = 0usize;
    let mut y = 0.0f32;
    let mut max_line_w = 0.0f32;
    loop {
        let para_end = text[para_start..].find('\n').map(|i| para_start + i).unwrap_or(text.len());
        let para = &text[para_start..para_end];
        // Shape the paragraph.
        items.clear();
        let mut prev: Option<(GlyphId, char)> = None;
        let mut prev_char: Option<char> = None;
        for (i, c) in para.char_indices() {
            let byte = (para_start + i) as u32;
            let prev_c = prev_char;
            prev_char = Some(c);
            if c == '\t' {
                items.push(Item {
                    glyph: space_glyph.unwrap_or(0),
                    byte,
                    len: 1,
                    advance: space_adv * 4.0,
                    kern: 0.0,
                    space: true,
                    break_before: false,
                });
                prev = None;
                continue;
            }
            if c == '\u{200B}' {
                // Zero-width space: an invisible break opportunity.
                items.push(Item { glyph: 0, byte, len: 3, advance: 0.0, kern: 0.0, space: true, break_before: false });
                prev = None;
                continue;
            }
            if c.is_control() || matches!(c, '\u{FEFF}' | '\u{200C}' | '\u{200D}' | '\u{00AD}') {
                continue;
            }
            let glyph = match font.glyph_index(c) {
                Some(g) => g,
                None => {
                    if !out.missing.contains(&c) {
                        out.missing.push(c);
                    }
                    0
                }
            };
            let space = c.is_whitespace() && c != '\u{00A0}' && c != '\u{202F}';
            let kern = match prev {
                Some((pg, _)) if style.kerning && font.has_kerning() => font.kerning(pg, glyph) as f32 * scale,
                _ => 0.0,
            };
            let break_before = match (items.last(), prev_c) {
                (Some(last), _) if last.space && !space => true,
                (Some(_), Some(pc)) if is_hyphen(pc) && c.is_alphabetic() => {
                    // Only inside a word: the char before the hyphen must be alphanumeric.
                    let before = text[..byte as usize].chars().rev().nth(1);
                    before.map(|b| b.is_alphanumeric()).unwrap_or(false)
                }
                _ => false,
            };
            items.push(Item {
                glyph,
                byte,
                len: c.len_utf8() as u8,
                advance: font.advance_width(glyph) as f32 * scale,
                kern,
                space,
                break_before,
            });
            prev = Some((glyph, c));
        }

        // Greedy wrap.
        let mut line_start = 0usize;
        let mut pen = 0.0f32;
        let mut last_break: Option<usize> = None;
        let mut i = 0usize;
        while i < items.len() {
            let it = items[i];
            if i > line_start && it.break_before {
                last_break = Some(i);
            }
            let k = if i > line_start { it.kern } else { 0.0 };
            let right = pen + k + it.advance;
            if let Some(mw) = max_w {
                if !it.space && i > line_start && right > mw + 0.01 {
                    let brk = match last_break {
                        Some(b) if b > line_start => b,
                        _ => i,
                    };
                    let w = emit_line(&items, line_start..brk, y, ascent, line_h, para_start, out);
                    max_line_w = max_line_w.max(w);
                    y += line_h;
                    line_start = brk;
                    pen = 0.0;
                    last_break = None;
                    i = brk;
                    continue;
                }
            }
            pen = right;
            i += 1;
        }
        let w = emit_line(&items, line_start..items.len(), y, ascent, line_h, para_start, out);
        if items.is_empty() {
            // Empty paragraph: the line still covers its position in the text.
            if let Some(l) = out.lines.last_mut() {
                l.bytes = para_start..para_start;
            }
        }
        max_line_w = max_line_w.max(w);
        y += line_h;

        if para_end >= text.len() {
            break;
        }
        para_start = para_end + 1;
        // "\r\n": the '\r' was skipped as a control character.
    }

    out.scratch.0 = items;

    // Alignment.
    let box_w = max_w.unwrap_or(max_line_w);
    for line in &mut out.lines {
        let dx = match style.align {
            Align::Left => 0.0,
            Align::Center => ((box_w - line.width) * 0.5).max(0.0),
            Align::Right => (box_w - line.width).max(0.0),
        };
        if dx != 0.0 {
            line.x += dx;
            for g in &mut out.glyphs[line.glyphs.clone()] {
                g.x += dx;
            }
        }
    }
    out.width = box_w;
    out.height = y;
}

/// Append one line; returns its width.
fn emit_line(
    items: &[Item],
    range: Range<usize>,
    top: f32,
    ascent: f32,
    line_h: f32,
    para_start: usize,
    out: &mut TextLayout,
) -> f32 {
    let baseline = top + ascent;
    let first_glyph = out.glyphs.len();
    let mut pen = 0.0f32;
    let mut width = 0.0f32;
    for (n, it) in items[range.clone()].iter().enumerate() {
        if n > 0 {
            pen += it.kern;
        }
        out.glyphs.push(PositionedGlyph { glyph: it.glyph, x: pen, y: baseline, advance: it.advance, byte: it.byte });
        pen += it.advance;
        if !it.space {
            width = pen;
        }
    }
    let bytes = match (items.get(range.start), range.end.checked_sub(1).and_then(|e| items.get(e))) {
        (Some(a), Some(b)) if !range.is_empty() => a.byte as usize..b.byte as usize + b.len as usize,
        _ => para_start..para_start,
    };
    out.lines.push(Line {
        glyphs: first_glyph..out.glyphs.len(),
        bytes,
        x: 0.0,
        width,
        top,
        baseline,
        height: line_h,
    });
    width
}

/// Width and height of `text` (physical px) without touching an atlas.
pub fn measure(font: &Font, text: &str, style: &TextStyle) -> TextMetrics {
    let l = layout(font, text, style);
    let width = l.lines.iter().map(|l| l.width).fold(0.0, f32::max);
    TextMetrics { width, height: l.height, lines: l.lines.len() }
}

impl TextLayout {
    /// Quads for every inked glyph, with the box's top-left at `origin` (physical px).
    pub fn quads(
        &self,
        font: &Font,
        atlas: &mut GlyphAtlas,
        origin: [f32; 2],
        color: [f32; 4],
    ) -> Result<Vec<GlyphQuad>, AtlasError> {
        let mut out = Vec::with_capacity(self.glyphs.len());
        self.append_quads(font, atlas, origin, color, &mut out)?;
        Ok(out)
    }

    /// [`TextLayout::quads`] appending to `out` (no allocation once `out` has capacity).
    /// Glyphs are positioned to 1/4 px horizontally (the bin is baked into the atlas bitmap) and
    /// baselines are snapped to whole pixels, so every quad maps texels 1:1. Glyphs whose outline
    /// is corrupt are skipped; only atlas capacity errors are returned.
    pub fn append_quads(
        &self,
        font: &Font,
        atlas: &mut GlyphAtlas,
        origin: [f32; 2],
        color: [f32; 4],
        out: &mut Vec<GlyphQuad>,
    ) -> Result<(), AtlasError> {
        let inv = 1.0 / atlas.page_size() as f32;
        let bins = SUBPIXEL_BINS as f32;
        for g in &self.glyphs {
            let pen_x = origin[0] + g.x;
            let q = (pen_x * bins).round();
            if !q.is_finite() {
                continue;
            }
            let ix = (q / bins).floor();
            let bin = (q - ix * bins) as u8;
            let by = (origin[1] + g.y).round();
            let a = match atlas.glyph(font, g.glyph, self.size_px, bin) {
                Ok(a) => a,
                // A corrupt glyph is skipped (counted in `AtlasStats::font_errors`), not fatal.
                Err(AtlasError::Font(_)) => continue,
                Err(e) => return Err(e),
            };
            if a.width == 0 {
                continue;
            }
            let x0 = ix + a.left as f32;
            let y0 = by + a.top as f32;
            out.push(GlyphQuad {
                rect: [x0, y0, x0 + a.width as f32, y0 + a.height as f32],
                uv: [
                    a.x as f32 * inv,
                    a.y as f32 * inv,
                    (a.x + a.width) as f32 * inv,
                    (a.y + a.height) as f32 * inv,
                ],
                page: a.page,
                color,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::AtlasConfig;
    use crate::font::tests::tiny_font;

    // Tiny font: units_per_em 1000; 'A' (glyph 1) advance 700, 'O' (2) 700, 'C' (3) 700,
    // unmapped chars (including space) -> .notdef advance 500; kern(A, O) = -50.
    // At size 10 px: advances 7, .notdef 5, kern -0.5; ascent 8, descent 2, gap 1 => line 11.

    fn font() -> Font {
        Font::from_bytes(tiny_font()).unwrap()
    }

    #[test]
    fn single_line_with_kerning() {
        let f = font();
        let l = layout(&f, "AOA", &TextStyle::new(10.0));
        assert_eq!(l.lines.len(), 1);
        let xs: Vec<f32> = l.glyphs.iter().map(|g| g.x).collect();
        assert_eq!(xs, vec![0.0, 6.5, 13.5]);
        assert_eq!(l.lines[0].width, 20.5);
        assert_eq!(l.lines[0].baseline, 8.0);
        assert_eq!(l.height, 11.0);
        assert!(l.missing.is_empty());
        let nk = layout(&f, "AOA", &TextStyle { kerning: false, ..TextStyle::new(10.0) });
        assert_eq!(nk.lines[0].width, 21.0);
    }

    #[test]
    fn missing_glyphs_reported_once() {
        let f = font();
        let l = layout(&f, "AxAx\u{1F600}", &TextStyle::new(10.0));
        assert_eq!(l.missing, vec!['x', '\u{1F600}']);
        assert_eq!(l.glyphs[1].glyph, 0);
        assert_eq!(l.glyphs.len(), 5);
    }

    #[test]
    fn hard_breaks_and_empty_lines() {
        let f = font();
        let l = layout(&f, "A\n\nAA\r\n", &TextStyle::new(10.0));
        assert_eq!(l.lines.len(), 4);
        assert_eq!(l.lines[0].bytes, 0..1);
        assert_eq!(l.lines[1].bytes, 2..2);
        assert_eq!(l.lines[2].bytes, 3..5);
        assert_eq!(l.lines[2].baseline, 8.0 + 22.0);
        assert_eq!(l.height, 44.0);
        assert_eq!(l.width, 14.0);
    }

    #[test]
    fn wraps_at_spaces_and_inside_long_words() {
        let f = font();
        // "AA AA AA": each word 14 px, space 5 px (.notdef advance in the tiny font, but whitespace).
        let style = TextStyle::new(10.0).with_max_width(35.0);
        let l = layout(&f, "AA AA AA", &style);
        assert_eq!(l.lines.len(), 2, "{:?}", l.lines);
        assert_eq!(l.lines[0].bytes, 0..6); // "AA AA " (trailing space hangs)
        assert_eq!(l.lines[0].width, 33.0);
        assert_eq!(l.lines[1].bytes, 6..8);
        assert_eq!(l.glyphs[l.lines[1].glyphs.start].x, 0.0);
        // A word longer than the line breaks inside the word, one glyph minimum per line.
        let l = layout(&f, "AAAAAAA", &TextStyle::new(10.0).with_max_width(20.0));
        assert_eq!(l.lines.len(), 4);
        assert!(l.lines.iter().all(|line| line.width <= 20.0));
        let l = layout(&f, "AA", &TextStyle::new(10.0).with_max_width(1.0));
        assert_eq!(l.lines.len(), 2);
        // Hyphen break: "AA-AA" with a 25 px line breaks after the hyphen ('-' is .notdef here).
        let l = layout(&f, "AA-AA", &TextStyle::new(10.0).with_max_width(25.0));
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[0].bytes, 0..3);
    }

    #[test]
    fn alignment_and_scale() {
        let f = font();
        let s = TextStyle::new(10.0).with_max_width(40.0);
        let c = layout(&f, "AA", &s.with_align(Align::Center));
        assert_eq!(c.lines[0].x, 13.0);
        assert_eq!(c.glyphs[0].x, 13.0);
        let r = layout(&f, "AA", &s.with_align(Align::Right));
        assert_eq!(r.lines[0].x, 26.0);
        // DPI scale 2: everything doubles.
        let d = layout(&f, "AA", &s.with_align(Align::Right).with_scale(2.0));
        assert_eq!(d.width, 80.0);
        assert_eq!(d.lines[0].x, 52.0);
        assert_eq!(d.size_px, 20.0);
        let m = measure(&f, "AA\nA", &TextStyle::new(10.0));
        assert_eq!(m, TextMetrics { width: 14.0, height: 22.0, lines: 2 });
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let f = font();
        for style in [
            TextStyle::new(0.0),
            TextStyle::new(f32::NAN),
            TextStyle::new(-5.0),
            TextStyle::new(1e9),
            TextStyle::new(10.0).with_max_width(f32::NAN),
            TextStyle::new(10.0).with_max_width(-3.0),
            TextStyle { line_height: f32::INFINITY, ..TextStyle::new(10.0) },
        ] {
            for text in ["", "\n", " ", "A A\tA\u{200B}A\u{0}", "\r\n\r\n", "\u{FEFF}A-"] {
                let l = layout(&f, text, &style);
                let mut atlas = GlyphAtlas::new(AtlasConfig::default());
                let _ = l.quads(&f, &mut atlas, [0.5, 0.5], [1.0; 4]);
            }
        }
    }

    #[test]
    fn quads_are_pixel_aligned_with_subpixel_bins() {
        let f = font();
        let mut atlas = GlyphAtlas::new(AtlasConfig { page_size: 128, ..AtlasConfig::default() });
        let l = layout(&f, "AOA", &TextStyle::new(10.0));
        let q = l.quads(&f, &mut atlas, [10.0, 20.3], [1.0, 1.0, 1.0, 1.0]).unwrap();
        assert_eq!(q.len(), 3);
        for quad in &q {
            for v in quad.rect {
                assert_eq!(v, v.round());
            }
            assert!(quad.uv.iter().all(|&u| (0.0..=1.0).contains(&u)));
        }
        // Glyph 2 sits at 16.5 px: a different sub-pixel bin than glyphs at whole pixels.
        assert_eq!(atlas.stats().misses, 3);
        // Second frame: all hits.
        atlas.begin_frame();
        let q2 = l.quads(&f, &mut atlas, [10.0, 20.3], [1.0; 4]).unwrap();
        assert_eq!(q2.len(), 3);
        assert_eq!(atlas.stats().misses, 3);
        assert_eq!(atlas.stats().hits, 3);
    }
}
