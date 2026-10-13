//! Tests against real system fonts. Each test skips cleanly (prints why, passes) when the font
//! is not installed, so CI images without fonts stay green.

use specula_text::*;
use std::path::PathBuf;

fn font_path(name: &str) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = if cfg!(windows) {
        let root = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        vec![root.join("Fonts")]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/truetype/msttcorefonts"),
            PathBuf::from("/usr/share/fonts/truetype/dejavu"),
            PathBuf::from("/usr/share/fonts/TTF"),
        ]
    };
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

fn load(name: &str) -> Option<(Font, Vec<u8>)> {
    match font_path(name) {
        Some(p) => {
            let bytes = std::fs::read(&p).unwrap();
            let font = Font::from_bytes(bytes.clone()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            Some((font, bytes))
        }
        None => {
            println!("skipped: {name} not installed");
            None
        }
    }
}

/// UI fonts to test with (all plain TrueType).
const FONTS: [&str; 4] = ["segoeui.ttf", "arial.ttf", "tahoma.ttf", "DejaVuSans.ttf"];

/// Independent minimal reader for `hmtx` advance widths (to cross-check the parser).
fn raw_advance(data: &[u8], glyph: u16) -> u16 {
    let be16 = |o: usize| u16::from_be_bytes([data[o], data[o + 1]]);
    let be32 = |o: usize| u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as usize;
    let n = be16(4) as usize;
    let find = |tag: &[u8]| (0..n).map(|i| 12 + i * 16).find(|&r| &data[r..r + 4] == tag).map(|r| be32(r + 8)).unwrap();
    let hhea = find(b"hhea");
    let hmtx = find(b"hmtx");
    let num = be16(hhea + 34) as usize;
    let g = (glyph as usize).min(num - 1);
    be16(hmtx + g * 4)
}

fn total_coverage(b: &Bitmap) -> f64 {
    b.pixels.iter().map(|&p| p as f64 / 255.0).sum()
}

#[test]
fn parses_metrics_and_advances_match_hmtx() {
    let mut tested = 0;
    for name in FONTS {
        let Some((font, bytes)) = load(name) else { continue };
        tested += 1;
        assert!(font.num_glyphs() > 200, "{name}: {} glyphs", font.num_glyphs());
        assert!(!font.family_name().is_empty(), "{name}: no family name");
        let lm = font.line_metrics();
        assert!(lm.ascent > 0 && lm.descent < 0, "{name}: {lm:?}");
        let cap = font.cap_height().expect("cap height");
        let xh = font.x_height().expect("x height");
        assert!(xh < cap && cap < lm.ascent, "{name}: x {xh} cap {cap}");
        for c in "AHOVWaegilmo0123456789 /.,!?é".chars() {
            let g = font.glyph_index(c).unwrap_or_else(|| panic!("{name}: no glyph for {c:?}"));
            assert_eq!(font.advance_width(g), raw_advance(&bytes, g), "{name}: advance of {c:?}");
        }
        // Proportional font: 'm' is wider than 'i'; digits are tabular (equal advances).
        let adv = |c| font.advance_width(font.glyph_index(c).unwrap());
        assert!(adv('m') > adv('i'));
        assert!("0123456789".chars().all(|c| adv(c) == adv('0')), "{name}: digits not tabular");
        println!(
            "{name}: family {:?}, {} glyphs, upem {}, gpos kern subtables {}, kerning {}",
            font.family_name(),
            font.num_glyphs(),
            font.units_per_em(),
            font.gpos_kern_subtables(),
            font.has_kerning()
        );
    }
    if tested == 0 {
        println!("skipped: no test fonts installed");
    }
}

#[test]
fn arial_known_values() {
    let Some((font, _)) = load("arial.ttf") else { return };
    // Published Arial metrics (2048 units per em): 'A' 1366, space 569, 'i' 455.
    assert_eq!(font.units_per_em(), 2048);
    assert_eq!(font.family_name(), "Arial");
    let adv = |c| font.advance_width(font.glyph_index(c).unwrap());
    assert_eq!(adv('A'), 1366);
    assert_eq!(adv(' '), 569);
    assert_eq!(adv('i'), 455);
}

#[test]
fn kerning_pairs_are_negative_for_classic_pairs() {
    for name in FONTS {
        let Some((font, _)) = load(name) else { continue };
        if !font.has_kerning() {
            println!("{name}: no kerning");
            continue;
        }
        let k = |a, b| font.kerning(font.glyph_index(a).unwrap(), font.glyph_index(b).unwrap());
        let pairs = [('A', 'V'), ('T', 'o'), ('V', 'A'), ('L', 'T'), ('Y', 'o'), ('A', 'T')];
        let values: Vec<i16> = pairs.iter().map(|&(a, b)| k(a, b)).collect();
        println!("{name}: AV To VA LT Yo AT = {values:?}");
        assert!(values.iter().filter(|&&v| v < 0).count() >= 3, "{name}: {values:?}");
        // Pairs that should not kern.
        assert_eq!(k('H', 'H'), 0, "{name}");
    }
}

#[test]
fn composite_glyphs_load() {
    for name in FONTS {
        let Some((font, _)) = load(name) else { continue };
        let e = font.glyph_outline(font.glyph_index('e').unwrap()).unwrap();
        let ea = font.glyph_outline(font.glyph_index('é').unwrap()).unwrap();
        assert!(ea.contour_ends.len() > e.contour_ends.len(), "{name}: é has no accent contour");
        assert!(ea.bounds().unwrap()[3] > e.bounds().unwrap()[3] + 100.0, "{name}: accent not above");
    }
}

#[test]
fn every_glyph_parses_and_rasterises() {
    for name in FONTS {
        let Some((font, _)) = load(name) else { continue };
        let mut raster = Rasterizer::new();
        let mut inked = 0;
        for g in 0..font.num_glyphs() {
            let o = font.glyph_outline(g).unwrap_or_else(|e| panic!("{name} glyph {g}: {e}"));
            let b = raster.rasterize(&o, font.scale_for_px(16.0), 0.5).unwrap();
            if !b.is_empty() {
                inked += 1;
            }
        }
        assert!(inked > 100, "{name}");
    }
}

#[test]
fn rasterisation_sanity() {
    for name in FONTS {
        let Some((font, _)) = load(name) else { continue };
        let size = 64.0;
        let glyph = |c| font.glyph_index(c).unwrap();

        // 'O' has a hole: the centre of its bitmap is empty, the ring is solid.
        let o = font.rasterize_glyph(glyph('O'), size, 0.0).unwrap();
        let (cx, cy) = (o.width / 2, o.height / 2);
        assert_eq!(o.get(cx, cy), 0, "{name}: 'O' centre filled");
        let row: Vec<u8> = (0..o.width).map(|x| o.get(x, cy)).collect();
        assert!(row.iter().filter(|&&p| p == 255).count() >= 4, "{name}: 'O' ring {row:?}");
        // Exactly two runs of ink across the middle row (left and right strokes).
        let runs = row.windows(2).filter(|w| w[0] == 0 && w[1] > 0).count() + usize::from(row[0] > 0);
        assert_eq!(runs, 2, "{name}: {row:?}");

        // 'l' is a vertical bar: its column profile is symmetric about the stem.
        let l = font.rasterize_glyph(glyph('l'), size, 0.0).unwrap();
        let cols: Vec<f64> = (0..l.width).map(|x| (0..l.height).map(|y| l.get(x, y) as f64).sum()).collect();
        let total: f64 = cols.iter().sum();
        let centre: f64 = cols.iter().enumerate().map(|(i, c)| (i as f64 + 0.5) * c).sum::<f64>() / total;
        assert!((centre - l.width as f64 / 2.0).abs() < 0.6, "{name}: 'l' off-centre {centre} of {}", l.width);

        // Coverage equals area: compare to the exact polygon area of a finely flattened outline.
        for c in ['O', 'H', 'e', 'g', '8'] {
            let outline = font.glyph_outline(glyph(c)).unwrap();
            let scale = font.scale_for_px(size) as f64;
            let mut area = 0.0f64;
            outline.segments(|s| {
                let pts: Vec<[f32; 2]> = match s {
                    Segment::Line(a, b) => vec![a, b],
                    Segment::Quad(a, k, b) => (0..=32)
                        .map(|i| {
                            let t = i as f32 / 32.0;
                            let u = 1.0 - t;
                            [
                                u * u * a[0] + 2.0 * u * t * k[0] + t * t * b[0],
                                u * u * a[1] + 2.0 * u * t * k[1] + t * t * b[1],
                            ]
                        })
                        .collect(),
                };
                for w in pts.windows(2) {
                    area += (w[0][0] as f64 * w[1][1] as f64 - w[1][0] as f64 * w[0][1] as f64) * 0.5;
                }
            });
            let expected = area.abs() * scale * scale;
            let got = total_coverage(&font.rasterize_glyph(glyph(c), size, 0.25).unwrap());
            let err = (got - expected).abs() / expected;
            assert!(err < 0.01, "{name} '{c}': coverage {got:.1} vs area {expected:.1}");
        }

        // Sub-pixel bins move the stem of 'l' (a vertical edge) right by exactly 1/4 px each:
        // the left edge sits at first-inked-column + (1 - coverage of that column).
        let mut prev: Option<f64> = None;
        for bin in 0..4 {
            let b = font.rasterize_glyph(glyph('l'), 24.0, bin as f32 * 0.25).unwrap();
            let y = b.height / 2;
            let first = (0..b.width).find(|&x| b.get(x, y) > 0).unwrap();
            let edge = b.left as f64 + first as f64 + 1.0 - b.get(first, y) as f64 / 255.0;
            if let Some(p) = prev {
                assert!(((edge - p) - 0.25).abs() < 0.01, "{name}: bin step {}", edge - p);
            }
            prev = Some(edge);
        }
    }
}

#[test]
fn layout_with_real_font() {
    let Some((_, font)) = find_system_ui_font() else {
        println!("skipped: no system UI font found");
        return;
    };
    let style = TextStyle::new(16.0);
    let hud = layout(&font, "Wave 3/10   Dust 240   Tombstones 9", &style);
    assert_eq!(hud.lines.len(), 1);
    assert!(hud.missing.is_empty());
    assert!(hud.width > 150.0 && hud.width < 400.0, "{}", hud.width);
    // Wrapping keeps every line within the width; centre alignment is symmetric.
    let para = "The tombstones hold the slab together. When the wraiths come over the broken edge, \
                every rune you carved will matter.";
    let wrapped = layout(&font, para, &style.with_max_width(200.0).with_align(Align::Center));
    assert!(wrapped.lines.len() >= 3);
    for line in &wrapped.lines {
        assert!(line.width <= 200.0 + 0.01, "{}", line.width);
        let left = line.x;
        let right = 200.0 - (line.x + line.width);
        assert!((left - right).abs() < 0.01);
        // No line starts with a space.
        let first = &para[line.bytes.clone()];
        assert!(!first.starts_with(' '), "{first:?}");
    }
    // The lines cover the whole text in order.
    let joined: String = wrapped.lines.iter().map(|l| &para[l.bytes.clone()]).collect();
    assert_eq!(joined, para);
    // DPI scale doubles everything (up to sub-pixel rounding of kerning, none here).
    let m1 = measure(&font, "Tombstones", &style);
    let m2 = measure(&font, "Tombstones", &style.with_scale(2.0));
    assert!((m2.width - 2.0 * m1.width).abs() < 0.01);
    assert!((m2.height - 2.0 * m1.height).abs() < 0.01);
    // Missing glyphs are reported.
    let l = layout(&font, "a\u{E000}b", &style);
    assert_eq!(l.missing, vec!['\u{E000}']);
}

#[test]
fn hud_frame_cost_when_cached() {
    let Some((_, font)) = find_system_ui_font() else {
        println!("skipped: no system UI font found");
        return;
    };
    let lines = [
        "Wave 3/10   Dust 240   Tombstones 9",
        "Lives 17   Next wave in 0:12",
        "Rune of Warding  (Lv 2)  Range 6.5 m  Damage 14",
        "Upgrade: 120 Dust     Sell: 60 Dust",
        "Wraiths 12   Ghouls 4   Bone Golem 1",
        "FPS 60   Frame 4.2 ms",
    ];
    let glyphs: usize = lines.iter().map(|l| l.chars().count()).sum();
    let mut atlas = GlyphAtlas::new(AtlasConfig::default());
    let mut quads = Vec::with_capacity(512);
    let mut layout_buf = TextLayout::default();
    let style = TextStyle::new(14.0).with_scale(1.25);
    let frame = |atlas: &mut GlyphAtlas, quads: &mut Vec<GlyphQuad>, buf: &mut TextLayout| {
        atlas.begin_frame();
        quads.clear();
        for (i, l) in lines.iter().enumerate() {
            layout_into(&font, l, &style, buf);
            buf.append_quads(&font, atlas, [8.0, 8.0 + 24.0 * i as f32], [0.93, 0.9, 0.84, 1.0], quads).unwrap();
        }
    };
    let t = std::time::Instant::now();
    frame(&mut atlas, &mut quads, &mut layout_buf);
    let cold = t.elapsed();
    let n = 200;
    let t = std::time::Instant::now();
    for _ in 0..n {
        frame(&mut atlas, &mut quads, &mut layout_buf);
    }
    let warm = t.elapsed() / n;
    println!(
        "HUD: {glyphs} chars, {} quads; first frame (rasterise) {:.3} ms; cached frame {:.4} ms; atlas {:?}",
        quads.len(),
        cold.as_secs_f64() * 1e3,
        warm.as_secs_f64() * 1e3,
        atlas.stats()
    );
    assert!(glyphs >= 190);
    assert!(atlas.stats().misses < 100);
    // Generous bound so slow debug CI passes; the budget (0.2 ms) is checked in release.
    assert!(warm.as_secs_f64() < 0.005, "cached HUD frame took {warm:?}");
    if !cfg!(debug_assertions) {
        assert!(warm.as_secs_f64() < 0.0002, "cached HUD frame took {warm:?} (budget 0.2 ms)");
    }
}
