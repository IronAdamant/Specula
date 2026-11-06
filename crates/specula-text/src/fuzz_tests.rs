//! Fuzz-style robustness tests: mutated and truncated fonts must never panic.
//!
//! Runs a fixed, deterministic set of mutations of the tiny synthetic font and (when present) a
//! real system font. Set `SPECULA_TEXT_FUZZ_ITERS` for a longer run.

use crate::atlas::{AtlasConfig, GlyphAtlas};
use crate::font::tests::{gpos_table, tiny_font, tiny_font_with};
use crate::font::Font;
use crate::layout::{layout, Align, TextStyle};
use crate::raster::rasterize_sdf;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Byte ranges of the tables in a (well-formed) font, to aim mutations at real structures.
fn table_ranges(data: &[u8]) -> Vec<(usize, usize)> {
    let n = u16::from_be_bytes([data[4], data[5]]) as usize;
    (0..n)
        .map(|i| {
            let rec = 12 + i * 16;
            let off = u32::from_be_bytes(data[rec + 8..rec + 12].try_into().unwrap()) as usize;
            let len = u32::from_be_bytes(data[rec + 12..rec + 16].try_into().unwrap()) as usize;
            (off, len.min(data.len().saturating_sub(off)))
        })
        .filter(|r| r.1 > 0)
        .collect()
}

fn mutate(rng: &mut Rng, src: &[u8], tables: &[(usize, usize)]) -> Vec<u8> {
    let mut d = src.to_vec();
    match rng.below(6) {
        0 => {
            // Truncate anywhere.
            let len = rng.below(d.len());
            d.truncate(len);
        }
        1 | 2 => {
            // Random bytes inside one table (or the directory).
            let (off, len) = if tables.is_empty() || rng.below(4) == 0 {
                (0, d.len().min(12 + 16 * 32))
            } else {
                tables[rng.below(tables.len())]
            };
            for _ in 0..1 + rng.below(8) {
                let at = off + rng.below(len);
                if at < d.len() {
                    d[at] = rng.next() as u8;
                }
            }
        }
        3 => {
            // Extreme values (0x00 / 0xFF runs) near a table start: counts, offsets, formats.
            if let Some(&(off, len)) = tables.get(rng.below(tables.len().max(1))) {
                let at = off + rng.below(len.min(64));
                let v = if rng.below(2) == 0 { 0xff } else { 0x00 };
                for b in d.iter_mut().skip(at).take(1 + rng.below(4)) {
                    *b = v;
                }
            }
        }
        4 => {
            // Swap two table directory offsets.
            if tables.len() >= 2 {
                let n = tables.len();
                let (a, b) = (12 + rng.below(n) * 16 + 8, 12 + rng.below(n) * 16 + 8);
                for k in 0..4 {
                    d.swap(a + k, b + k);
                }
            }
        }
        _ => {
            // Bit flips anywhere.
            for _ in 0..1 + rng.below(16) {
                let at = rng.below(d.len());
                d[at] ^= 1 << rng.below(8);
            }
        }
    }
    d
}

/// Exercise every API on a (possibly broken) font. Must not panic.
fn exercise(data: Vec<u8>, rng: &mut Rng, full_glyph_scan: bool) {
    let Ok(font) = Font::from_bytes(data) else { return };
    let _ = font.family_name();
    let n = font.num_glyphs();
    let glyphs: Vec<u16> = if full_glyph_scan {
        (0..n.saturating_add(2)).collect()
    } else {
        (0..24).map(|_| rng.below(n as usize + 2) as u16).collect()
    };
    for &g in &glyphs {
        let _ = font.h_metrics(g);
        if let Ok(o) = font.glyph_outline(g) {
            let _ = o.bounds();
        }
        let _ = font.rasterize_glyph(g, 16.0, 0.25);
        let _ = font.kerning(g, glyphs[0]);
    }
    let _ = font.rasterize_glyph_sdf(glyphs[0], 24.0, 3.0);
    for c in ['A', 'V', 'é', ' ', '\u{1F600}', '\u{FFFF}', '\u{10FFFF}'] {
        let _ = font.glyph_index(c);
    }
    let style = TextStyle::new(14.0).with_max_width(120.0).with_align(Align::Center);
    let l = layout(&font, "Wave 3/10   Dust 240\nAVAToé \u{1F600} long-wordwordword", &style);
    let mut atlas = GlyphAtlas::new(AtlasConfig { page_size: 256, max_pages: 2, ..AtlasConfig::default() });
    let _ = l.quads(&font, &mut atlas, [0.0, 0.0], [1.0; 4]);
    assert!(atlas.validate().is_ok());
    if let Ok(o) = font.glyph_outline(glyphs[0]) {
        let _ = rasterize_sdf(&o, 0.01, 2.0);
    }
    // Slug: whatever prepares must evaluate inside its own words, and the store keeps its cap.
    let mut store = crate::slug::SlugStore::new(64 * 1024);
    for &g in glyphs.iter().take(8) {
        if let Ok(sg) = crate::slug::SlugGlyph::of(&font, g) {
            let mut words = Vec::new();
            let at = sg.encode(&mut words);
            let c = crate::slug::coverage_from_words(&words, at, [0.3, 0.3], [20.0, 20.0]);
            assert!((0.0..=1.0).contains(&c) || c.is_nan());
            assert!(sg.hbands.iter().chain(&sg.vbands).all(|b| b.len() <= crate::slug::MAX_BAND_CURVES));
        }
        let _ = store.glyph(&font, g);
        assert!(store.bytes() <= store.cap_bytes());
    }
    // Rich layout with a fallback face (itself) and an ellipsis.
    let set = crate::layout::FontSet { regular: &font, bold: Some(&font), fallback: &[&font] };
    let r = crate::rich::parse("[b]Wave[/b] [color=#f00]3[/color] [icon=pad_a] \u{5893}\u{5730} e\u{301}").unwrap();
    let l = crate::layout::layout_rich(&set, &r.text, &r.spans, &TextStyle::new(14.0).with_max_width(40.0).with_max_lines(2));
    for g in &l.glyphs {
        assert!((g.byte as usize) <= r.text.len());
    }
    let _ = l.caret(3);
    let _ = l.hit(10.0, 5.0);
    let _ = l.selection_rects(0..r.text.len());
}

fn iterations(default: usize) -> usize {
    std::env::var("SPECULA_TEXT_FUZZ_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[test]
fn mutated_tiny_fonts_never_panic() {
    // The plain tiny font, and one with a GPOS table (pair kerning plus duplicated indices).
    let sources = [tiny_font(), tiny_font_with(vec![(b"GPOS", gpos_table(40, 40, 40, false))])];
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    for src in &sources {
        let tables = table_ranges(src);
        for _ in 0..iterations(4000) {
            let d = mutate(&mut rng, src, &tables);
            exercise(d, &mut rng, true);
        }
        // Every truncation length.
        for len in 0..src.len() {
            exercise(src[..len].to_vec(), &mut rng, true);
        }
    }
}

#[test]
fn mutated_bundled_fonts_never_panic() {
    for (k, face) in crate::bundled::Bundled::ALL.into_iter().enumerate() {
        let src = face.bytes();
        let tables = table_ranges(src);
        let mut rng = Rng(0x5eed_1234_abcd_0100 + k as u64);
        for _ in 0..iterations(200) {
            let d = mutate(&mut rng, src, &tables);
            exercise(d, &mut rng, false);
        }
    }
}

#[test]
fn mutated_system_font_never_panics() {
    let Some((path, _)) = crate::system::find_system_ui_font() else {
        println!("skipped: no system UI font found");
        return;
    };
    let src = std::fs::read(&path).unwrap();
    let tables = table_ranges(&src);
    let mut rng = Rng(0x5eed_1234_abcd_0002);
    for _ in 0..iterations(300) {
        let d = mutate(&mut rng, &src, &tables);
        exercise(d, &mut rng, false);
    }
}

/// Characters from every line-break class the tables know, and the ones that trip parsers:
/// combining marks, joiners, CJK and kana, emoji, controls, separators, quotes, brackets.
const POOL: &[char] = &[
    'a', 'Z', '7', ' ', '\t', '\n', '\r', '\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}', '-', '\u{2010}', '\u{AD}', '/', '%', '$', '(', ')', '[',
    ']', '{', '}', '"', '\'', '\u{AB}', '\u{BB}', '!', '?', ',', '.', ':', ';', '\u{A0}', '\u{202F}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}',
    '\u{FEFF}', '\u{301}', '\u{308}', '\u{20DD}', 'é', 'Ж', 'Ω', '\u{5893}', '\u{3002}', '\u{3001}', '\u{30A2}', '\u{30FC}', '\u{AC00}', '\u{FF01}',
    '\u{1F600}', '\u{1F3FB}', '\u{FFFC}', '\u{FFFD}', '\u{10FFFF}', '\u{E000}', '\u{0}', '\u{7F}', '\u{9F}', '\u{2026}', '\u{3000}', '#', '=', '\\',
];

fn hostile_text(rng: &mut Rng, max: usize) -> String {
    (0..rng.below(max + 1)).map(|_| POOL[rng.below(POOL.len())]).collect()
}

#[test]
fn line_breaks_of_hostile_text_hold_their_invariants() {
    let mut rng = Rng(0x5eed_1234_abcd_0200);
    let mut out = Vec::new();
    for _ in 0..iterations(3000) {
        let t = hostile_text(&mut rng, 64);
        crate::linebreak::breaks_into(&t, &mut out);
        let last = out.last().copied().expect("the end is always a break");
        assert!(last.byte == t.len() && last.mandatory, "{t:?}: {out:?}");
        assert!(out.windows(2).all(|w| w[0].byte < w[1].byte), "in order, once each: {t:?}");
        assert!(out.iter().all(|b| t.is_char_boundary(b.byte) && (b.byte > 0 || t.is_empty())), "{t:?}: {out:?}");
        assert!(out.len() <= t.chars().count() + 1);
    }
    // Linear in the text: a long text of the pool (68,027 characters from this seed).
    let big = hostile_text(&mut Rng(7), 1 << 18);
    let started = std::time::Instant::now();
    crate::linebreak::breaks_into(&big, &mut out);
    assert!(started.elapsed().as_millis() < 2000, "{:?} for {} bytes", started.elapsed(), big.len());
}

#[test]
fn hostile_markup_is_refused_with_a_place_or_parsed_within_its_caps() {
    const TOKENS: &[&str] =
        &["[b]", "[/b]", "[color=#3fe0d0]", "[color=#f00]", "[color=#zzzzzz]", "[color=]", "[/color]", "[icon=pad_a]", "[icon=]", "[icon=pad_a,2]", "[/icon]", "[", "]", "[[", "]]", "\\[", "[x]", "[/]", "[b", "=", "#", "é", "\u{5893}", "a b", "\n"];
    let mut rng = Rng(0x5eed_1234_abcd_0300);
    for _ in 0..iterations(4000) {
        let mut m = String::new();
        for _ in 0..rng.below(40) {
            if rng.below(4) == 0 {
                m.push(POOL[rng.below(POOL.len())]);
            } else {
                m.push_str(TOKENS[rng.below(TOKENS.len())]);
            }
        }
        match crate::rich::parse(&m) {
            Ok(r) => {
                assert!(r.text.len() <= m.len() + 4 * r.spans.len(), "{m:?}");
                assert!(r.spans.len() <= crate::rich::MAX_SPANS);
                for s in &r.spans {
                    assert!(s.bytes.start <= s.bytes.end && s.bytes.end <= r.text.len(), "{m:?}: {s:?}");
                    assert!(r.text.is_char_boundary(s.bytes.start) && r.text.is_char_boundary(s.bytes.end));
                }
                // It lays out (fallback-free) without panicking, every glyph inside the text.
                let f = Font::from_bytes(tiny_font()).unwrap();
                let set = crate::layout::FontSet::single(&f);
                let l = crate::layout::layout_rich(&set, &r.text, &r.spans, &TextStyle::new(12.0).with_max_width(30.0).with_max_lines(3));
                assert!(l.glyphs.iter().all(|g| (g.byte as usize) <= r.text.len()));
            }
            Err(e) => {
                assert!(e.at <= m.len(), "{m:?}: {e}");
                assert!(!e.code.is_empty() && !e.message.is_empty());
            }
        }
    }
    // Over the markup cap: refused up front, not parsed.
    let big = "[b]x[/b]".repeat(crate::rich::MAX_MARKUP_BYTES / 8 + 1);
    assert!(crate::rich::parse(&big).is_err());
}

#[test]
fn random_edit_ops_keep_the_text_and_caret_valid() {
    use crate::edit::{EditOp, MemoryClipboard, TextEdit, Unit};
    let f = Font::from_bytes(tiny_font()).unwrap();
    let mut rng = Rng(0x5eed_1234_abcd_0400);
    let units = [Unit::Char, Unit::Word, Unit::Line, Unit::All];
    for round in 0..iterations(300) {
        let mut t = TextEdit::with_limit(&hostile_text(&mut rng, 40), 96);
        let mut cb = MemoryClipboard::default();
        let mut layout = crate::layout::TextLayout::default();
        for _ in 0..40 {
            let unit = units[rng.below(4)];
            let select = rng.below(2) == 0;
            let op = match rng.below(17) {
                0 => EditOp::Insert(hostile_text(&mut rng, 12)),
                1 => EditOp::Backspace(unit),
                2 => EditOp::Delete(unit),
                3 => EditOp::Left { unit, select },
                4 => EditOp::Right { unit, select },
                5 => EditOp::Up { select },
                6 => EditOp::Down { select },
                7 => EditOp::SetCaret { byte: rng.below(200), select },
                8 => EditOp::Select { anchor: rng.below(200), caret: rng.below(200) },
                9 => EditOp::SelectAll,
                10 => EditOp::Copy,
                11 => EditOp::Cut,
                12 => EditOp::Paste,
                13 => EditOp::Compose { text: hostile_text(&mut rng, 8), caret: rng.below(40) },
                14 => EditOp::Commit(hostile_text(&mut rng, 8)),
                15 => EditOp::Undo,
                _ => EditOp::Redo,
            };
            let with_layout = round % 2 == 0;
            if with_layout {
                let (shown, _) = t.display();
                crate::layout::layout_into(&f, &shown, &TextStyle::new(12.0).with_max_width(40.0), &mut layout);
            }
            // A layout of the displayed text (with a composition) or none: both must be safe.
            let _ = t.apply(&op, &mut cb, with_layout.then_some(&layout));
            let s = t.text();
            assert!(s.len() <= t.max_bytes(), "{op:?}: {} bytes", s.len());
            assert!(s.is_char_boundary(t.caret()) && s.is_char_boundary(t.anchor()), "{op:?} in {s:?}: {} {}", t.caret(), t.anchor());
            let (shown, range) = t.display();
            if let Some(r) = range {
                assert!(r.end <= shown.len() && shown.is_char_boundary(r.start) && shown.is_char_boundary(r.end));
            }
        }
    }
}
