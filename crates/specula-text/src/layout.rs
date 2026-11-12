//! Text layout: UTF-8 → positioned glyphs with kerning → wrapped, aligned lines → quads (stage T3;
//! design: `docs/design/TEXT.md`).
//!
//! Layout works in **physical pixels**: sizes and widths in [`TextStyle`] are logical pixels and
//! are multiplied by [`TextStyle::scale`] (the DPI / UI scale factor). Positions are relative to
//! the top-left of the text box, y down.
//!
//! - **Line breaking** follows a documented subset of Unicode's rules ([`crate::linebreak`]):
//!   mandatory breaks at newlines and paragraph separators, soft breaks after spaces and hyphens
//!   and between ideographs, never before closing punctuation or small kana. A word wider than
//!   the line breaks between its characters (never inside a base + combining mark cluster).
//! - **Fonts:** a [`FontSet`] gives a regular face, an optional bold face and fallback faces tried
//!   in order for characters the first lacks (CJK, symbols); each glyph records its face.
//! - **Rich text:** [`Span`]s mark byte ranges bold, coloured or as an inline icon (see
//!   [`crate::rich`] for the markup); each glyph records its span, and icons get boxes
//!   ([`TextLayout::inlines`]) that the caller draws (controller prompts, for one).
//! - **Overflow:** `max_lines` cuts the text and, with `ellipsis`, ends the last line with "…";
//!   `wrap: false` keeps each paragraph on one line (ellipsis then truncates it to `max_width`).
//!   The layout reports what happened ([`TextLayout::truncated`], [`TextLayout::overflow`]) and
//!   every line's box, for `ui_check`.
//! - **Editing support:** [`TextLayout::caret`], [`TextLayout::hit`] and
//!   [`TextLayout::selection_rects`] map between byte offsets and positions.

use crate::atlas::{GlyphAtlas, SUBPIXEL_BINS};
use crate::error::AtlasError;
use crate::font::{Font, GlyphId};
use crate::linebreak::{self, Class};
use std::ops::Range;

/// The object replacement character: an inline icon's place in the text.
pub const INLINE_OBJECT: char = '\u{FFFC}';

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
    /// Wrap width in logical pixels; `None` = no wrapping (only mandatory breaks).
    pub max_width: Option<f32>,
    pub align: Align,
    /// Apply the font's pair kerning (`kern` and GPOS).
    pub kerning: bool,
    /// Wrap at `max_width` (false: one line per paragraph, cut by `ellipsis`).
    pub wrap: bool,
    /// At most this many lines; the rest is cut ([`TextLayout::truncated`]).
    pub max_lines: Option<u32>,
    /// End cut text with "…" (fitted inside `max_width`).
    pub ellipsis: bool,
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle {
            size_px: 16.0,
            scale: 1.0,
            line_height: 1.0,
            max_width: None,
            align: Align::Left,
            kerning: true,
            wrap: true,
            max_lines: None,
            ellipsis: false,
        }
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

    /// At most `lines` lines, the cut marked with an ellipsis.
    pub fn with_max_lines(mut self, lines: u32) -> TextStyle {
        self.max_lines = Some(lines);
        self.ellipsis = true;
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

/// The faces a layout draws from: `regular`, `bold` for bold spans (regular when absent), and
/// `fallback` faces tried in order for characters the span's face lacks.
#[derive(Clone, Copy, Debug)]
pub struct FontSet<'a> {
    pub regular: &'a Font,
    pub bold: Option<&'a Font>,
    pub fallback: &'a [&'a Font],
}

/// Face index of the regular face in [`PositionedGlyph::face`].
pub const FACE_REGULAR: u8 = 0;
/// Face index of the bold face.
pub const FACE_BOLD: u8 = 1;
/// Face index of the first fallback face (then 3, 4, …).
pub const FACE_FALLBACK: u8 = 2;
/// Most fallback faces.
pub const MAX_FALLBACK_FACES: usize = 32;

impl<'a> FontSet<'a> {
    pub fn single(font: &'a Font) -> FontSet<'a> {
        FontSet { regular: font, bold: None, fallback: &[] }
    }

    /// The face of [`PositionedGlyph::face`] `index` (regular for an index out of range).
    pub fn face(&self, index: u8) -> &'a Font {
        match index {
            FACE_REGULAR => self.regular,
            FACE_BOLD => self.bold.unwrap_or(self.regular),
            i => self.fallback.get((i - FACE_FALLBACK) as usize).copied().unwrap_or(self.regular),
        }
    }

    /// The face for `c` in a span (bold or not): its own face when it has the glyph, else the first
    /// fallback that does, else `None`.
    fn pick(&self, c: char, bold: bool) -> Option<(u8, GlyphId)> {
        let own = if bold && self.bold.is_some() { FACE_BOLD } else { FACE_REGULAR };
        if let Some(g) = self.face(own).glyph_index(c) {
            return Some((own, g));
        }
        if own == FACE_BOLD {
            if let Some(g) = self.regular.glyph_index(c) {
                return Some((FACE_REGULAR, g));
            }
        }
        self.fallback.iter().take(MAX_FALLBACK_FACES).enumerate().find_map(|(i, f)| f.glyph_index(c).map(|g| (FACE_FALLBACK + i as u8, g)))
    }
}

/// A styled byte range of the text (rich text): later spans override earlier ones where they
/// overlap.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub bytes: Range<usize>,
    pub bold: bool,
    /// sRGB colour, straight alpha (`None`: the text's colour).
    pub color: Option<[f32; 4]>,
    /// An inline icon: the span covers one [`INLINE_OBJECT`] character, drawn by the caller in
    /// the box the layout gives it ([`TextLayout::inlines`]). Its width in ems.
    pub icon: Option<(String, f32)>,
}

impl Span {
    pub fn new(bytes: Range<usize>) -> Span {
        Span { bytes, bold: false, color: None, icon: None }
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
    /// The face it comes from ([`FontSet::face`]).
    pub face: u8,
    /// Index of the span styling it + 1 (0: none).
    pub span: u16,
}

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Indices into [`TextLayout::glyphs`].
    pub glyphs: Range<usize>,
    /// Source bytes of the line (trailing spaces included, the line break excluded).
    pub bytes: Range<usize>,
    /// Left edge after alignment, physical px.
    pub x: f32,
    /// Ink advance width without trailing whitespace, physical px.
    pub width: f32,
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
}

/// An inline icon's box (physical px, the text box's top-left at 0, 0).
#[derive(Clone, Debug, PartialEq)]
pub struct InlineBox {
    /// Index into the spans given to the layout.
    pub span: usize,
    pub byte: u32,
    pub rect: [f32; 4],
}

/// Where the caret sits for a byte offset (physical px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretPos {
    pub x: f32,
    pub top: f32,
    pub height: f32,
    pub line: usize,
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
    /// Characters no face has a glyph for (drawn as `.notdef`), in first-seen order, unique.
    pub missing: Vec<char>,
    /// Inline icons' boxes.
    pub inlines: Vec<InlineBox>,
    /// Text was cut (`max_lines`, or `wrap: false` with an ellipsis): the bytes from here on are
    /// not shown.
    pub truncated: Option<usize>,
    /// A line is still wider than `max_width` (nowhere to break, or `wrap: false` without an
    /// ellipsis).
    pub overflow: bool,
    /// Glyphs drawn from fallback faces.
    pub fallback_glyphs: usize,
    /// The text the layout was made from, byte length (offsets above refer to it).
    pub text_len: usize,
    /// Reused buffers, so [`layout_into`] does not allocate once warmed up.
    scratch: Scratch,
}

/// Internal per-layout buffers. Ignored by `PartialEq`, emptied by `Clone`.
#[derive(Default)]
struct Scratch {
    items: Vec<Item>,
    breaks: Vec<linebreak::Break>,
    /// Paragraph starts: (item index, byte).
    paras: Vec<(usize, usize)>,
    /// Span indices by start byte and by end byte, and the spans open at the current byte (a
    /// max-heap; closed ones are dropped lazily), for the span sweep.
    span_starts: Vec<(usize, usize)>,
    span_ends: Vec<(usize, usize)>,
    open: std::collections::BinaryHeap<usize>,
    closed: Vec<bool>,
}

impl TextLayout {
    /// Frees the reused buffers and spare capacity: for a layout that is kept (stored with a
    /// component) rather than redone every frame.
    pub fn compact(&mut self) {
        self.scratch = Scratch::default();
        self.glyphs.shrink_to_fit();
        self.lines.shrink_to_fit();
        self.missing.shrink_to_fit();
        self.inlines.shrink_to_fit();
    }
}

/// Which span styles each character, in one pass: spans open and close as the bytes go by, and
/// the last one listed among the open ones wins. O((characters + spans) log spans), where looking
/// every span up per character was O(characters × spans) (64 KiB of markup with 1024 spans was
/// 64 M checks per layout).
struct SpanSweep<'s> {
    spans: &'s [Span],
    next_start: usize,
    next_end: usize,
}

impl<'s> SpanSweep<'s> {
    fn new(spans: &'s [Span], s: &mut Scratch) -> SpanSweep<'s> {
        s.span_starts.clear();
        s.span_ends.clear();
        s.open.clear();
        s.closed.clear();
        s.closed.resize(spans.len(), false);
        for (i, sp) in spans.iter().enumerate() {
            if sp.bytes.start < sp.bytes.end {
                s.span_starts.push((sp.bytes.start, i));
                s.span_ends.push((sp.bytes.end, i));
            }
        }
        s.span_starts.sort_unstable();
        s.span_ends.sort_unstable();
        SpanSweep { spans, next_start: 0, next_end: 0 }
    }

    /// The span (index + 1) covering `byte` (0: none). Bytes must come in increasing order.
    fn at(&mut self, byte: usize, s: &mut Scratch) -> u16 {
        while let Some(&(at, i)) = s.span_starts.get(self.next_start) {
            if at > byte {
                break;
            }
            s.open.push(i);
            self.next_start += 1;
        }
        while let Some(&(at, i)) = s.span_ends.get(self.next_end) {
            if at > byte {
                break;
            }
            s.closed[i] = true;
            self.next_end += 1;
        }
        while let Some(&top) = s.open.peek() {
            if !s.closed[top] {
                break;
            }
            s.open.pop();
        }
        debug_assert!(s.open.peek().is_none_or(|&i| self.spans[i].bytes.contains(&byte)));
        s.open.peek().map_or(0, |&i| (i + 1).min(u16::MAX as usize) as u16)
    }
}

impl Clone for Scratch {
    fn clone(&self) -> Scratch {
        Scratch::default()
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
    /// Continues the previous item's cluster (a combining mark): never a line start.
    cluster: bool,
    face: u8,
    span: u16,
}

/// Lays `text` out in one font.
pub fn layout(font: &Font, text: &str, style: &TextStyle) -> TextLayout {
    let mut out = TextLayout::default();
    layout_into(font, text, style, &mut out);
    out
}

/// [`layout`] into an existing result, reusing its allocations (for per-frame HUD text).
pub fn layout_into(font: &Font, text: &str, style: &TextStyle, out: &mut TextLayout) {
    layout_rich_into(&FontSet::single(font), text, &[], style, out);
}

/// Lays `text` out from `fonts` with `spans` (rich text; may be empty).
pub fn layout_rich(fonts: &FontSet, text: &str, spans: &[Span], style: &TextStyle) -> TextLayout {
    let mut out = TextLayout::default();
    layout_rich_into(fonts, text, spans, style, &mut out);
    out
}

/// The span (index + 1) covering `byte`: the last one listed wins (the reference [`SpanSweep`] is
/// tested against).
#[cfg(test)]
fn span_at(spans: &[Span], byte: usize) -> u16 {
    spans.iter().enumerate().rev().find(|(_, s)| s.bytes.contains(&byte)).map_or(0, |(i, _)| (i + 1).min(u16::MAX as usize) as u16)
}

/// [`layout_rich`] into an existing result, reusing its allocations.
pub fn layout_rich_into(fonts: &FontSet, text: &str, spans: &[Span], style: &TextStyle, out: &mut TextLayout) {
    out.glyphs.clear();
    out.lines.clear();
    out.missing.clear();
    out.inlines.clear();
    out.truncated = None;
    out.overflow = false;
    out.fallback_glyphs = 0;
    out.text_len = text.len();
    let font = fonts.regular;
    let size = style.physical_size();
    let scale = font.scale_for_px(size);
    let lm = font.line_metrics();
    let ascent = lm.ascent as f32 * scale;
    let descent = -(lm.descent as f32) * scale;
    let line_mul = if style.line_height.is_finite() && style.line_height > 0.0 { style.line_height.min(100.0) } else { 1.0 };
    let line_h = (lm.line_height().max(0) as f32 * scale * line_mul).max(0.0);
    let max_w = style.max_width.map(|w| w * style.scale).filter(|w| w.is_finite()).map(|w| w.max(0.0));
    let wrap_w = if style.wrap { max_w } else { None };
    let max_lines = style.max_lines.map(|n| n.max(1) as usize);
    out.size_px = size;
    out.ascent = ascent;
    out.descent = descent;

    let space_glyph = font.glyph_index(' ');
    let space_adv = space_glyph.map(|g| font.advance_width(g)).unwrap_or(font.units_per_em() / 4) as f32 * scale;
    let face_scale = |face: u8| fonts.face(face).scale_for_px(size);

    // Shape: one item per drawn character, with its break opportunity.
    let mut items = std::mem::take(&mut out.scratch.items);
    let mut brk = std::mem::take(&mut out.scratch.breaks);
    items.clear();
    linebreak::breaks_into(text, &mut brk);
    let mut bi = 0usize;
    // Mandatory break positions split paragraphs: items after one start a new line.
    let mut para_starts = std::mem::take(&mut out.scratch.paras);
    para_starts.clear();
    para_starts.push((0, 0));
    let mut prev: Option<(u8, GlyphId)> = None;
    let mut sweep_scratch = Scratch {
        span_starts: std::mem::take(&mut out.scratch.span_starts),
        span_ends: std::mem::take(&mut out.scratch.span_ends),
        open: std::mem::take(&mut out.scratch.open),
        closed: std::mem::take(&mut out.scratch.closed),
        ..Scratch::default()
    };
    let mut sweep = SpanSweep::new(spans, &mut sweep_scratch);
    for (i, c) in text.char_indices() {
        // The break opportunity before this character, if any.
        while bi < brk.len() && brk[bi].byte < i {
            bi += 1;
        }
        let here = brk.get(bi).filter(|b| b.byte == i).copied();
        if let Some(b) = here {
            if b.mandatory {
                para_starts.push((items.len(), i));
                prev = None;
            }
        }
        let soft = here.is_some_and(|b| !b.mandatory);
        let span = sweep.at(i, &mut sweep_scratch);
        let sp = span.checked_sub(1).and_then(|s| spans.get(s as usize));
        let byte = i as u32;
        let len = c.len_utf8() as u8;
        if let Some((_, width)) = sp.and_then(|s| s.icon.as_ref()).filter(|_| c == INLINE_OBJECT) {
            let w = if width.is_finite() { width.clamp(0.0, 16.0) } else { 1.0 } * size;
            items.push(Item { glyph: 0, byte, len, advance: w, kern: 0.0, space: false, break_before: soft, cluster: false, face: 0, span });
            prev = None;
            continue;
        }
        if c == '\t' {
            items.push(Item { glyph: space_glyph.unwrap_or(0), byte, len, advance: space_adv * 4.0, kern: 0.0, space: true, break_before: soft, cluster: false, face: 0, span });
            prev = None;
            continue;
        }
        if c == '\u{200B}' {
            items.push(Item { glyph: 0, byte, len, advance: 0.0, kern: 0.0, space: true, break_before: soft, cluster: false, face: 0, span });
            prev = None;
            continue;
        }
        let class = linebreak::class_of(c);
        if c.is_control() || matches!(c, '\u{FEFF}' | '\u{200C}' | '\u{200D}' | '\u{00AD}' | '\u{2060}' | '\u{2028}' | '\u{2029}') {
            continue;
        }
        let bold = sp.is_some_and(|s| s.bold);
        let (face, glyph) = match fonts.pick(c, bold) {
            Some(fg) => fg,
            None => {
                if !out.missing.contains(&c) && out.missing.len() < 256 {
                    out.missing.push(c);
                }
                (if bold && fonts.bold.is_some() { FACE_BOLD } else { FACE_REGULAR }, 0)
            }
        };
        if face >= FACE_FALLBACK {
            out.fallback_glyphs += 1;
        }
        let space = c.is_whitespace() && c != '\u{00A0}' && c != '\u{202F}';
        let s = face_scale(face);
        let kern = match prev {
            Some((pf, pg)) if pf == face && style.kerning && fonts.face(face).has_kerning() => fonts.face(face).kerning(pg, glyph) as f32 * s,
            _ => 0.0,
        };
        items.push(Item {
            glyph,
            byte,
            len,
            advance: fonts.face(face).advance_width(glyph) as f32 * s,
            kern,
            space,
            break_before: soft,
            cluster: class == Class::CM && !items.is_empty(),
            face,
            span,
        });
        prev = Some((face, glyph));
    }

    // A text ending in a line break ends with an empty line.
    if text.ends_with(['\n', '\r', '\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}']) {
        para_starts.push((items.len(), text.len()));
    }

    // Lines: paragraphs between mandatory breaks, wrapped greedily at break opportunities.
    let mut y = 0.0f32;
    let mut max_line_w = 0.0f32;
    let mut truncated: Option<usize> = None;
    'paras: for (p, &(first, para_byte)) in para_starts.iter().enumerate() {
        let end = para_starts.get(p + 1).map_or(items.len(), |&(f, _)| f);
        let para = first..end;
        let mut line_start = para.start;
        let mut pen = 0.0f32;
        let mut last_break: Option<usize> = None;
        let mut i = para.start;
        while i < para.end {
            let it = items[i];
            if i > line_start && it.break_before {
                last_break = Some(i);
            }
            let k = if i > line_start { it.kern } else { 0.0 };
            let right = pen + k + it.advance;
            if let Some(mw) = wrap_w {
                if !it.space && i > line_start && right > mw + 0.01 {
                    let mut brk_at = match last_break {
                        Some(b) if b > line_start => b,
                        _ => i,
                    };
                    // Never split a base character from its combining marks.
                    while brk_at > line_start + 1 && items[brk_at].cluster {
                        brk_at -= 1;
                    }
                    if max_lines.is_some_and(|m| out.lines.len() + 1 >= m) {
                        // The last line allowed: cut here.
                        truncated = Some(items[brk_at].byte as usize);
                        let w = emit_line(&items, line_start..brk_at, y, ascent, line_h, para_byte, out);
                        max_line_w = max_line_w.max(w);
                        y += line_h;
                        break 'paras;
                    }
                    let w = emit_line(&items, line_start..brk_at, y, ascent, line_h, para_byte, out);
                    max_line_w = max_line_w.max(w);
                    y += line_h;
                    line_start = brk_at;
                    pen = 0.0;
                    last_break = None;
                    i = brk_at;
                    continue;
                }
            }
            pen = right;
            i += 1;
        }
        if max_lines.is_some_and(|m| out.lines.len() >= m) {
            // Lines are used up: this paragraph (and the rest) is cut.
            truncated = Some(para_byte);
            break;
        }
        let first_glyph = out.lines.len();
        let w = emit_line(&items, line_start..para.end, y, ascent, line_h, para_byte, out);
        if para.is_empty() {
            if let Some(l) = out.lines.get_mut(first_glyph) {
                l.bytes = para_byte..para_byte;
            }
        }
        max_line_w = max_line_w.max(w);
        y += line_h;
    }
    out.scratch.items = items;
    out.scratch.breaks = brk;
    out.scratch.paras = para_starts;
    out.scratch.span_starts = sweep_scratch.span_starts;
    out.scratch.span_ends = sweep_scratch.span_ends;
    out.scratch.open = sweep_scratch.open;
    out.scratch.closed = sweep_scratch.closed;

    // Overflow on one line without wrapping: cut at the width when an ellipsis is wanted.
    if !style.wrap && style.ellipsis {
        if let Some(mw) = max_w {
            for li in 0..out.lines.len() {
                if out.lines[li].width > mw + 0.01 {
                    // Each overlong line is cut; the first cut is the one reported.
                    let cut = fit_ellipsis(fonts, out, li, mw, size);
                    truncated = truncated.or(Some(cut));
                }
            }
            max_line_w = out.lines.iter().map(|l| l.width).fold(0.0, f32::max);
        }
    } else if truncated.is_some() && style.ellipsis {
        if let Some(li) = out.lines.len().checked_sub(1) {
            let mw = max_w.unwrap_or(f32::INFINITY);
            let cut = fit_ellipsis(fonts, out, li, mw, size);
            truncated = Some(truncated.map_or(cut, |t| t.min(cut)));
            max_line_w = out.lines.iter().map(|l| l.width).fold(0.0, f32::max);
        }
    }
    out.truncated = truncated;
    out.overflow = max_w.is_some_and(|mw| out.lines.iter().any(|l| l.width > mw + 0.01));

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
    // Inline boxes, after alignment.
    for g in &out.glyphs {
        if g.glyph == 0 && g.span > 0 {
            if let Some(s) = spans.get(g.span as usize - 1) {
                if s.icon.is_some() && text.get(g.byte as usize..).is_some_and(|t| t.starts_with(INLINE_OBJECT)) {
                    let top = g.y - ascent;
                    out.inlines.push(InlineBox { span: g.span as usize - 1, byte: g.byte, rect: [g.x, top, g.x + g.advance, top + ascent + descent] });
                }
            }
        }
    }
    out.width = box_w;
    out.height = y;
}

/// Ends line `li` with an ellipsis inside `max_w`, dropping glyphs (and trailing spaces) from its
/// end; returns the byte where the shown text stops. Runs before alignment (lines start at 0).
fn fit_ellipsis(fonts: &FontSet, out: &mut TextLayout, li: usize, max_w: f32, size: f32) -> usize {
    let font = fonts.regular;
    let scale = font.scale_for_px(size);
    let (glyph, count) = match font.glyph_index('\u{2026}') {
        Some(g) => (g, 1usize),
        None => (font.glyph_index('.').unwrap_or(0), 3usize),
    };
    let adv = font.advance_width(glyph) as f32 * scale;
    let ell_w = adv * count as f32;
    let line = out.lines[li].clone();
    let mut end = line.glyphs.end;
    while end > line.glyphs.start {
        let g = out.glyphs[end - 1];
        if g.x + g.advance + ell_w <= max_w + 0.01 && !is_space_glyph(fonts, g) {
            break;
        }
        end -= 1;
    }
    let cut = if end < line.glyphs.end { out.glyphs[end].byte as usize } else { line.bytes.end };
    let removed = line.glyphs.end - end;
    out.glyphs.drain(end..line.glyphs.end);
    let pen = if end > line.glyphs.start { out.glyphs[end - 1].x + out.glyphs[end - 1].advance } else { 0.0 };
    for k in 0..count {
        let g = PositionedGlyph { glyph, x: pen + adv * k as f32, y: line.baseline, advance: adv, byte: cut as u32, face: FACE_REGULAR, span: 0 };
        out.glyphs.insert(end + k, g);
    }
    let l = &mut out.lines[li];
    l.glyphs.end = end + count;
    l.width = pen + ell_w;
    l.bytes.end = cut.max(l.bytes.start);
    for later in out.lines.iter_mut().skip(li + 1) {
        later.glyphs.start = later.glyphs.start + count - removed;
        later.glyphs.end = later.glyphs.end + count - removed;
    }
    cut
}

fn is_space_glyph(fonts: &FontSet, g: PositionedGlyph) -> bool {
    fonts.face(g.face).glyph_index(' ') == Some(g.glyph) && g.glyph != 0
}

/// Append one line; returns its width.
fn emit_line(items: &[Item], range: Range<usize>, top: f32, ascent: f32, line_h: f32, para_start: usize, out: &mut TextLayout) -> f32 {
    let baseline = top + ascent;
    let first_glyph = out.glyphs.len();
    let mut pen = 0.0f32;
    let mut width = 0.0f32;
    for (n, it) in items[range.clone()].iter().enumerate() {
        if n > 0 {
            pen += it.kern;
        }
        out.glyphs.push(PositionedGlyph { glyph: it.glyph, x: pen, y: baseline, advance: it.advance, byte: it.byte, face: it.face, span: it.span });
        pen += it.advance;
        if !it.space {
            width = pen;
        }
    }
    let bytes = match (items.get(range.start), range.end.checked_sub(1).and_then(|e| items.get(e))) {
        (Some(a), Some(b)) if !range.is_empty() => a.byte as usize..b.byte as usize + b.len as usize,
        _ => para_start..para_start,
    };
    out.lines.push(Line { glyphs: first_glyph..out.glyphs.len(), bytes, x: 0.0, width, top, baseline, height: line_h });
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
    pub fn quads(&self, font: &Font, atlas: &mut GlyphAtlas, origin: [f32; 2], color: [f32; 4]) -> Result<Vec<GlyphQuad>, AtlasError> {
        let mut out = Vec::with_capacity(self.glyphs.len());
        self.append_quads(font, atlas, origin, color, &mut out)?;
        Ok(out)
    }

    /// [`TextLayout::quads`] appending to `out` (no allocation once `out` has capacity).
    /// Glyphs are positioned to 1/4 px horizontally (the bin is baked into the atlas bitmap) and
    /// baselines are snapped to whole pixels, so every quad maps texels 1:1. Glyphs whose outline
    /// is corrupt are skipped; only atlas capacity errors are returned.
    pub fn append_quads(&self, font: &Font, atlas: &mut GlyphAtlas, origin: [f32; 2], color: [f32; 4], out: &mut Vec<GlyphQuad>) -> Result<(), AtlasError> {
        self.append_quads_rich(&FontSet::single(font), &[], atlas, origin, color, out)
    }

    /// Quads of a layout made with [`layout_rich`]: each glyph from its own face, coloured by its
    /// span (or `color`); inline icons are left to the caller ([`TextLayout::inlines`]).
    pub fn append_quads_rich(
        &self,
        fonts: &FontSet,
        spans: &[Span],
        atlas: &mut GlyphAtlas,
        origin: [f32; 2],
        color: [f32; 4],
        out: &mut Vec<GlyphQuad>,
    ) -> Result<(), AtlasError> {
        let inv = 1.0 / atlas.page_size() as f32;
        let bins = SUBPIXEL_BINS as f32;
        for g in &self.glyphs {
            let span = g.span.checked_sub(1).and_then(|s| spans.get(s as usize));
            if span.is_some_and(|s| s.icon.is_some()) && g.glyph == 0 {
                continue;
            }
            let pen_x = origin[0] + g.x;
            let q = (pen_x * bins).round();
            if !q.is_finite() {
                continue;
            }
            let ix = (q / bins).floor();
            let bin = (q - ix * bins) as u8;
            let by = (origin[1] + g.y).round();
            let a = match atlas.glyph(fonts.face(g.face), g.glyph, self.size_px, bin) {
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
                uv: [a.x as f32 * inv, a.y as f32 * inv, (a.x + a.width) as f32 * inv, (a.y + a.height) as f32 * inv],
                page: a.page,
                color: span.and_then(|s| s.color).unwrap_or(color),
            });
        }
        Ok(())
    }

    /// The line showing byte `byte` (the last line for the end of the text).
    pub fn line_of(&self, byte: usize) -> usize {
        let mut best = 0;
        for (i, l) in self.lines.iter().enumerate() {
            if l.bytes.start <= byte {
                best = i;
            }
        }
        best
    }

    /// Where the caret goes for byte offset `byte` (before the character there).
    pub fn caret(&self, byte: usize) -> CaretPos {
        let Some(li) = (!self.lines.is_empty()).then(|| self.line_of(byte)) else {
            return CaretPos { x: 0.0, top: 0.0, height: self.ascent + self.descent, line: 0 };
        };
        let l = &self.lines[li];
        let mut x = l.x;
        for g in &self.glyphs[l.glyphs.clone()] {
            if (g.byte as usize) < byte {
                x = g.x + g.advance;
            } else {
                x = g.x;
                break;
            }
        }
        CaretPos { x, top: l.top, height: l.height, line: li }
    }

    /// The byte offset nearest the point (`x`, `y`) (physical px from the box's top-left): the
    /// caret position a click there gives.
    pub fn hit(&self, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let li = self.lines.iter().position(|l| y < l.top + l.height).unwrap_or(self.lines.len() - 1);
        let l = &self.lines[li];
        for g in &self.glyphs[l.glyphs.clone()] {
            if x < g.x + g.advance * 0.5 {
                return g.byte as usize;
            }
        }
        l.bytes.end
    }

    /// Selection rectangles for bytes `range` (one per line it touches), physical px.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<[f32; 4]> {
        let mut out = Vec::new();
        if range.start >= range.end {
            return out;
        }
        for l in &self.lines {
            if l.bytes.end < range.start || l.bytes.start >= range.end {
                continue;
            }
            let mut x0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            for g in &self.glyphs[l.glyphs.clone()] {
                let b = g.byte as usize;
                if b >= range.start && b < range.end {
                    x0 = x0.min(g.x);
                    x1 = x1.max(g.x + g.advance);
                }
            }
            if x0 < x1 {
                out.push([x0, l.top, x1, l.top + l.height]);
            }
        }
        out
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

    // --- T3/T4: real fonts, fallback, rich text, overflow, caret ---

    use crate::bundled::Bundled;

    fn ui() -> Font {
        Bundled::UiRegular.load().unwrap()
    }

    #[test]
    fn every_script_lays_out_in_the_bundled_faces() {
        let f = ui();
        for text in ["Here lies Ada, beloved.", "Здесь покоится Ада", "Ἐνθάδε κεῖται Ἄδα", "Tiếng Việt", "ß œ Ø ł ğ"] {
            let l = layout(&f, text, &TextStyle::new(20.0));
            assert!(l.missing.is_empty(), "{text}: missing {:?}", l.missing);
            assert_eq!(l.fallback_glyphs, 0);
        }
        // Kerning applies (GPOS pair kerning in Alegreya Sans): "AV" is tighter than A + V.
        let av = layout(&f, "AV", &TextStyle::new(100.0)).lines[0].width;
        let a = layout(&f, "A", &TextStyle::new(100.0)).lines[0].width;
        let v = layout(&f, "V", &TextStyle::new(100.0)).lines[0].width;
        assert!(av < a + v - 1.0, "AV {av} vs {a} + {v}");
        assert!(f.has_gpos_kerning());
    }

    #[test]
    fn fallback_faces_fill_what_the_first_lacks() {
        let f = ui();
        let display = Bundled::Display.load().unwrap();
        // The display face stands in as a "fallback" for a character set we make the primary lack:
        // use a tiny font as primary, so every real letter falls back.
        let tiny = font();
        let fallback = [&display];
        let set = FontSet { regular: &tiny, bold: None, fallback: &fallback };
        let l = layout_rich(&set, "AxЖ", &[], &TextStyle::new(20.0));
        assert_eq!(l.glyphs.iter().map(|g| g.face).collect::<Vec<_>>(), vec![FACE_REGULAR, FACE_FALLBACK, FACE_FALLBACK]);
        assert_eq!(l.fallback_glyphs, 2);
        assert!(l.missing.is_empty());
        let none = layout_rich(&FontSet::single(&tiny), "AxЖ", &[], &TextStyle::new(20.0));
        assert_eq!(none.missing, vec!['x', 'Ж']);
        // A system CJK face, when there is one.
        match crate::system::load_fallbacks("ja", 1).pop() {
            Some((path, cjk)) => {
                let fb = [&cjk];
                let set = FontSet { regular: &f, bold: None, fallback: &fb };
                let l = layout_rich(&set, "墓地 graveyard", &[], &TextStyle::new(20.0));
                assert!(l.missing.is_empty(), "{}: {:?}", path.display(), l.missing);
                assert_eq!(l.fallback_glyphs, 2);
                let mut atlas = GlyphAtlas::new(AtlasConfig::default());
                let mut q = Vec::new();
                l.append_quads_rich(&set, &[], &mut atlas, [0.0; 2], [1.0; 4], &mut q).unwrap();
                assert_eq!(q.len(), 11, "every inked glyph from its own face");
            }
            None => println!("skipped: no system CJK font"),
        }
    }

    #[test]
    fn rich_spans_pick_bold_and_colour_and_make_icon_boxes() {
        let f = ui();
        let bold = Bundled::UiBold.load().unwrap();
        let set = FontSet { regular: &f, bold: Some(&bold), fallback: &[] };
        let r = crate::rich::parse("Press [icon=pad_a] to [b]build[/b] a [color=#3fe0d0]ward[/color]").unwrap();
        let l = layout_rich(&set, &r.text, &r.spans, &TextStyle::new(20.0));
        assert_eq!(l.inlines.len(), 1);
        let ib = &l.inlines[0];
        assert!((ib.rect[2] - ib.rect[0] - 20.0).abs() < 0.01, "one em wide: {ib:?}");
        let faces: Vec<u8> = l.glyphs.iter().filter(|g| r.text[g.byte as usize..].starts_with('b') && g.span > 0).map(|g| g.face).collect();
        assert_eq!(faces, vec![FACE_BOLD]);
        let mut atlas = GlyphAtlas::new(AtlasConfig::default());
        let mut q = Vec::new();
        l.append_quads_rich(&set, &r.spans, &mut atlas, [0.0; 2], [1.0; 4], &mut q).unwrap();
        let cyan = q.iter().filter(|q| q.color != [1.0; 4]).count();
        assert_eq!(cyan, 4, "the four letters of 'ward' are coloured");
        assert_eq!(q.len(), "Pressto build a ward".chars().filter(|c| !c.is_whitespace()).count(), "the icon is the caller's");
    }

    #[test]
    fn max_lines_and_no_wrap_end_with_an_ellipsis() {
        let f = ui();
        let text = "Here lies Ada, who built the first ward and held the gate for ten long waves.";
        let style = TextStyle::new(20.0).with_max_width(160.0).with_max_lines(2);
        let l = layout(&f, text, &style);
        assert_eq!(l.lines.len(), 2);
        let cut = l.truncated.expect("cut");
        assert!(cut > 10 && cut < text.len(), "{cut}");
        let last = l.lines.last().unwrap();
        let g = l.glyphs[last.glyphs.end - 1];
        assert_eq!(g.glyph, f.glyph_index('\u{2026}').unwrap(), "ends with an ellipsis");
        assert!(last.width <= 160.01 && !l.overflow, "{} {}", last.width, l.overflow);
        // No wrap: one line, cut to the width.
        let one = layout(&f, text, &TextStyle { wrap: false, ellipsis: true, ..TextStyle::new(20.0).with_max_width(120.0) });
        assert_eq!(one.lines.len(), 1);
        assert!(one.lines[0].width <= 120.01 && one.truncated.is_some() && !one.overflow);
        // No wrap and no ellipsis: reported as overflow.
        let over = layout(&f, text, &TextStyle { wrap: false, ..TextStyle::new(20.0).with_max_width(120.0) });
        assert!(over.overflow && over.truncated.is_none());
        // Fits: nothing to report.
        let fits = layout(&f, "Ada", &style);
        assert_eq!((fits.truncated, fits.overflow), (None, false));
        // Mandatory breaks past the line limit cut at the paragraph.
        let paras = layout(&f, "one\ntwo\nthree", &TextStyle::new(20.0).with_max_lines(2));
        assert_eq!(paras.lines.len(), 2);
        assert_eq!(paras.truncated, Some(7), "the shown text stops before the second line break");
    }

    #[test]
    fn caret_hit_and_selection_agree() {
        let f = ui();
        let text = "Rest in peace\nold friend";
        let l = layout(&f, text, &TextStyle::new(20.0));
        for byte in [0usize, 5, 13, 14, 18, text.len()] {
            let c = l.caret(byte);
            let back = l.hit(c.x + 0.1, c.top + c.height * 0.5);
            assert_eq!(back, byte, "caret at {byte}: {c:?}");
        }
        assert_eq!(l.caret(text.len()).line, 1);
        assert_eq!(l.hit(-50.0, 1e6), 14, "below and left: start of the last line");
        let rects = l.selection_rects(5..18);
        assert_eq!(rects.len(), 2, "two lines: {rects:?}");
        assert!(rects[0][0] > 0.0 && rects[1][0] == 0.0);
        assert!(l.selection_rects(3..3).is_empty());
    }

    #[test]
    fn cjk_wraps_between_ideographs_with_fallback() {
        let Some((_, cjk)) = crate::system::load_fallbacks("ja", 1).pop() else {
            println!("skipped: no system CJK font");
            return;
        };
        let f = ui();
        let fb = [&cjk];
        let set = FontSet { regular: &f, bold: None, fallback: &fb };
        let text = "静かな墓地に月が昇る。風が吹く。";
        let l = layout_rich(&set, text, &[], &TextStyle::new(20.0).with_max_width(100.0));
        assert!(l.lines.len() >= 3, "{}", l.lines.len());
        for line in &l.lines {
            let first = text[line.bytes.clone()].chars().next().unwrap();
            assert!(first != '。', "a full stop never starts a line");
            assert!(line.width <= 100.01);
        }
    }

    #[test]
    fn the_span_sweep_matches_looking_each_span_up() {
        // Overlapping, nested, empty, inverted and out-of-range spans, in every order.
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let text = "Wave 3 of 10: the dead walk at dusk, Ж и Ω, 墓地";
        let mut scratch = Scratch::default();
        for _ in 0..500 {
            let spans: Vec<Span> = (0..next(12)).map(|_| Span::new(next(text.len() + 4)..next(text.len() + 4))).collect();
            let mut sweep = SpanSweep::new(&spans, &mut scratch);
            for (i, _) in text.char_indices() {
                assert_eq!(sweep.at(i, &mut scratch), span_at(&spans, i), "byte {i} of {spans:?}");
            }
        }
    }

    #[test]
    fn the_largest_markup_lays_out_within_its_cost() {
        // Cost before work: the most markup rich text takes (64 KiB) with the most spans (1024),
        // each covering everything after it. Looking every span up per character made this
        // 64 M checks; the sweep is linear in characters and spans.
        let f = ui();
        let body: String = "Here lies Ada, who kept the lamps. ".chars().cycle().take(crate::rich::MAX_MARKUP_BYTES - 1024 * 8).collect();
        let mut markup = String::new();
        for _ in 0..1024 {
            markup.push_str("[b]");
        }
        markup.push_str(&body);
        let r = crate::rich::parse(&markup);
        // Nesting is capped at 16: build the spans directly instead (the API takes any).
        assert!(r.is_err() || r.as_ref().is_ok_and(|r| r.spans.len() <= crate::rich::MAX_SPANS));
        let spans: Vec<Span> = (0..crate::rich::MAX_SPANS).map(|i| Span { bold: i % 2 == 0, ..Span::new(i..body.len()) }).collect();
        let set = FontSet::single(&f);
        let style = TextStyle::new(16.0).with_max_width(800.0);
        let started = std::time::Instant::now();
        let l = layout_rich(&set, &body, &spans, &style);
        let took = started.elapsed();
        assert!(l.glyphs.len() > 50_000);
        assert_eq!(l.glyphs.last().map(|g| g.span as usize), Some(crate::rich::MAX_SPANS), "the last span listed wins");
        // About 25 ms in a debug build; the old per-character lookup took seconds.
        assert!(took < crate::speed_budget(std::time::Duration::from_millis(1500)), "{took:?}");
    }
}
