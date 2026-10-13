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
