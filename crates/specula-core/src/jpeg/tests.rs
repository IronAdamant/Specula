use super::encode::{STD_AC_CHROMA_BITS, STD_AC_CHROMA_VALS, STD_AC_LUMA_BITS, STD_AC_LUMA_VALS};
use super::*;
use crate::hash::hash_bytes;
use crate::rng::Rng;

/// A smooth test picture: gradients, a soft disc and a hard-edged bar; alpha varies too.
fn picture(w: u32, h: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w.max(2) as f32;
            let fy = y as f32 / h.max(2) as f32;
            let d = ((fx - 0.6) * (fx - 0.6) + (fy - 0.4) * (fy - 0.4)).sqrt();
            let disc = (1.0 - (d * 4.0).min(1.0)) * 180.0;
            let bar = if (w / 5..w / 4 + 1).contains(&x) { 90.0 } else { 0.0 };
            let r = (40.0 + 200.0 * fx + bar).min(255.0);
            let g = (30.0 + 150.0 * fy + disc * 0.4).min(255.0);
            let b = (220.0 - 160.0 * fx * fy + disc * 0.2).min(255.0);
            let a = 255.0 - 120.0 * fy;
            px.extend_from_slice(&[r as u8, g as u8, b as u8, a as u8]);
        }
    }
    px
}

/// What a decoder should give back for `color` (alpha is never stored).
fn expected(src: &[u8], color: ColorModel) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    for p in src.chunks_exact(4) {
        let (r, g, b, a) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
        match color {
            ColorModel::Gray => {
                let y = ((19_595 * r + 38_470 * g + 7_471 * b + 32_768) >> 16) as u8;
                out.extend_from_slice(&[y, y, y, 255]);
            }
            ColorModel::AdobeCmyk | ColorModel::AdobeYcck => {
                out.extend_from_slice(&[(r * a / 255) as u8, (g * a / 255) as u8, (b * a / 255) as u8, 255]);
            }
            _ => out.extend_from_slice(&[r as u8, g as u8, b as u8, 255]),
        }
    }
    out
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mut se = 0f64;
    let mut n = 0f64;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        for k in 0..3 {
            let d = pa[k] as f64 - pb[k] as f64;
            se += d * d;
            n += 1.0;
        }
    }
    if se == 0.0 {
        return 99.0;
    }
    10.0 * (255.0 * 255.0 / (se / n)).log10()
}

#[test]
fn zigzag_is_the_standard_order() {
    assert_eq!(&ZIGZAG[..10], &[0, 1, 8, 16, 9, 2, 3, 10, 17, 24]);
    assert_eq!(ZIGZAG[63], 63);
    let mut seen = [false; 64];
    for &z in &ZIGZAG {
        assert!(!seen[z]);
        seen[z] = true;
    }
}

#[test]
fn standard_ac_tables_cover_every_symbol() {
    for (bits, vals) in [(STD_AC_LUMA_BITS, &STD_AC_LUMA_VALS), (STD_AC_CHROMA_BITS, &STD_AC_CHROMA_VALS)] {
        assert_eq!(bits.iter().map(|&b| b as usize).sum::<usize>(), vals.len());
        let mut want: Vec<u8> = vec![0x00, 0xF0];
        for r in 0..16u8 {
            for s in 1..=10u8 {
                want.push((r << 4) | s);
            }
        }
        let mut have = vals.to_vec();
        have.sort_unstable();
        want.sort_unstable();
        assert_eq!(have, want);
        Huff::build(&bits, vals).unwrap();
    }
}

#[test]
fn round_trips_every_mode_within_psnr() {
    let (w, h) = (61, 45); // not a multiple of any MCU size
    let src = picture(w, h);
    let colors = [ColorModel::YCbCr, ColorModel::Gray, ColorModel::AdobeRgb, ColorModel::AdobeCmyk, ColorModel::AdobeYcck];
    for color in colors {
        let want = expected(&src, color);
        for sub in [Subsampling::S444, Subsampling::S422, Subsampling::S420] {
            for (progressive, optimize, restart) in [(false, false, 0), (false, true, 3), (true, false, 0), (true, true, 5)] {
                let opts = EncodeOptions { quality: 92, subsampling: sub, color, progressive, optimize, restart_interval: restart };
                let jpg = encode(w, h, &src, &opts).unwrap();
                let img = decode(&jpg).unwrap_or_else(|e| panic!("{opts:?}: {e}"));
                assert_eq!((img.width, img.height), (w, h));
                let p = psnr(&img.rgba, &want);
                let floor = match (color, sub) {
                    // Measured: 4:4:4 47-55 dB, grey 53.5 dB, 4:2:x 40.7-49.7 dB.
                    (ColorModel::Gray, _) | (_, Subsampling::S444) => 45.0,
                    _ => 38.0,
                };
                assert!(p > floor, "{opts:?}: PSNR {p:.2} dB");
                let i = info(&jpg).unwrap();
                assert_eq!((i.width, i.height, i.progressive), (w, h, progressive));
            }
        }
    }
}

/// Progressive scans (spectral selection + successive approximation, EOB runs, correction
/// bits) must rebuild exactly the coefficients a baseline file holds: identical pixels.
#[test]
fn progressive_decodes_bit_identical_to_baseline() {
    for (w, h) in [(1, 1), (8, 8), (9, 17), (64, 48), (130, 70)] {
        let src = picture(w, h);
        for quality in [10, 75, 100] {
            for sub in [Subsampling::S444, Subsampling::S420] {
                for restart in [0, 1, 7] {
                    let base = EncodeOptions { quality, subsampling: sub, restart_interval: restart, ..EncodeOptions::default() };
                    let a = decode(&encode(w, h, &src, &base).unwrap()).unwrap();
                    let b = decode(&encode(w, h, &src, &EncodeOptions { progressive: true, ..base }).unwrap()).unwrap();
                    assert!(a.rgba == b.rgba, "{w}x{h} q{quality} {sub:?} ri{restart}");
                    // One sequential scan per component decodes the same too.
                    let c = decode(&super::encode::encode_with(w, h, &src, &base, false).unwrap()).unwrap();
                    assert!(a.rgba == c.rgba, "non-interleaved {w}x{h} q{quality} {sub:?} ri{restart}");
                }
            }
        }
    }
}

#[test]
fn flat_and_extreme_images() {
    for v in [0u8, 128, 255] {
        let src = vec![v; 16 * 16 * 4];
        let img = decode(&encode_baseline(16, 16, &src, 100)).unwrap();
        assert!(img.rgba.chunks_exact(4).all(|p| p[0].abs_diff(v) <= 1 && p[3] == 255), "{v}");
    }
    // Checkerboard at quality 100: high-frequency content, largest coefficients.
    let src: Vec<u8> = (0..32 * 32).flat_map(|i| if (i % 32 + i / 32) % 2 == 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
    let img = decode(&encode(32, 32, &src, &EncodeOptions { quality: 100, subsampling: Subsampling::S444, ..EncodeOptions::default() }).unwrap()).unwrap();
    assert!(psnr(&img.rgba, &src) > 30.0);
}

#[test]
fn output_is_pinned() {
    // Encoder bytes and decoder pixels are deterministic (integer arithmetic only).
    let src = picture(40, 30);
    let jpg = encode_baseline(40, 30, &src, 85);
    let prog = encode(40, 30, &src, &EncodeOptions { quality: 85, progressive: true, ..EncodeOptions::default() }).unwrap();
    let img = decode(&jpg).unwrap();
    let pinned = (hash_bytes(&jpg), hash_bytes(&prog), hash_bytes(&img.rgba));
    assert_eq!(pinned, (0x54bf_d837_5cb7_1588, 0xd326_731f_439c_944a, 0x44be_3854_1866_aac8), "{:#018x} {:#018x} {:#018x}", pinned.0, pinned.1, pinned.2);
}

#[test]
fn errors_are_structured() {
    let src = picture(24, 24);
    let jpg = encode_baseline(24, 24, &src, 80);
    assert_eq!(decode(b"hello").unwrap_err().code, JpegErrorCode::NotJpeg);
    assert_eq!(decode(&[]).unwrap_err().code, JpegErrorCode::NotJpeg);
    // Truncated inside the entropy-coded data.
    let sos = jpg.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    let cut = &jpg[..sos + 30];
    let e = decode(cut).unwrap_err();
    assert_eq!(e.code, JpegErrorCode::Truncated, "{e}");
    // Truncated inside a header segment.
    assert_eq!(decode(&jpg[..25]).unwrap_err().code, JpegErrorCode::Truncated);
    // Unsupported processes and precisions.
    let sof = jpg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    for (marker, code) in [(0xC3u8, JpegErrorCode::Unsupported), (0xC9, JpegErrorCode::Unsupported), (0xC5, JpegErrorCode::Unsupported)] {
        let mut m = jpg.clone();
        m[sof + 1] = marker;
        let e = decode(&m).unwrap_err();
        assert_eq!((e.code, e.offset), (code, sof), "{e}");
    }
    let mut m = jpg.clone();
    m[sof + 4] = 12;
    assert_eq!(decode(&m).unwrap_err().code, JpegErrorCode::Unsupported);
    // Size limit refused before allocating.
    let e = decode_with_limits(&jpg, &Limits { max_pixels: 100 }).unwrap_err();
    assert_eq!(e.code, JpegErrorCode::Limit);
    assert!(e.message.contains("576"), "{e}");
    // A huge declared size with tiny data fails without allocating gigabytes.
    let mut m = jpg.clone();
    m[sof + 5..sof + 9].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    let e = decode_with_limits(&m, &Limits { max_pixels: u64::MAX }).unwrap_err();
    assert_eq!(e.code, JpegErrorCode::Truncated, "{e}");
    // An invalid Huffman table (more codes than lengths allow).
    let dht = jpg.windows(2).position(|w| w == [0xFF, 0xC4]).unwrap();
    let mut m = jpg.clone();
    m[dht + 5] = 3; // three 1-bit codes
    assert_eq!(decode(&m).unwrap_err().code, JpegErrorCode::BadHuffman);
    // Restart markers out of sequence.
    let rst = encode(64, 64, &picture(64, 64), &EncodeOptions { restart_interval: 2, ..EncodeOptions::default() }).unwrap();
    let at = rst.windows(2).position(|w| w == [0xFF, 0xD1]).unwrap();
    let mut m = rst.clone();
    m[at + 1] = 0xD3;
    assert_eq!(decode(&m).unwrap_err().code, JpegErrorCode::BadData);
    // Tables missing.
    let mut m = jpg[..dht].to_vec();
    m.extend_from_slice(&jpg[sos..]);
    assert_eq!(decode(&m).unwrap_err().code, JpegErrorCode::BadSegment);
    // `info` reads the header only.
    assert_eq!(info(&jpg).unwrap(), Info { width: 24, height: 24, components: 3, progressive: false });
}

/// Mutated files (byte flips, insertions, deletions, truncations) must never panic.
#[test]
fn fuzz_mutations_never_panic() {
    let src = picture(48, 40);
    let seeds = [
        encode_baseline(48, 40, &src, 75),
        encode(48, 40, &src, &EncodeOptions { progressive: true, restart_interval: 4, ..EncodeOptions::default() }).unwrap(),
        encode(48, 40, &src, &EncodeOptions { color: ColorModel::AdobeCmyk, subsampling: Subsampling::S422, optimize: true, ..EncodeOptions::default() }).unwrap(),
        encode(48, 40, &src, &EncodeOptions { color: ColorModel::Gray, progressive: true, ..EncodeOptions::default() }).unwrap(),
    ];
    // `SPECULA_FUZZ_SCALE=100` runs a hundred times as many mutants.
    let scale: usize = std::env::var("SPECULA_FUZZ_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut rng = Rng::new(0x1234 + scale as u64 - 1, 7);
    let limits = Limits { max_pixels: 1 << 20 };
    let mut ok = 0;
    for i in 0..6000 * scale {
        let mut m = seeds[i % seeds.len()].clone();
        for _ in 0..1 + rng.below(4) {
            let at = rng.below(m.len() as u32) as usize;
            match rng.below(5) {
                0 => m[at] ^= 1 << rng.below(8),
                1 => m[at] = rng.below(256) as u8,
                2 => m.insert(at, rng.below(256) as u8),
                3 => {
                    m.remove(at);
                }
                _ => m.truncate(at.max(2)),
            }
            if m.is_empty() {
                break;
            }
        }
        if decode_with_limits(&m, &limits).is_ok() {
            ok += 1;
        }
        let _ = info(&m);
    }
    // Most single flips land in entropy data and still decode (to garbage), which is fine.
    assert!(ok > 100, "only {ok} mutants decoded");
}

/// `cargo test -p specula-core --release jpeg_decode_speed -- --ignored --nocapture`
#[test]
#[ignore]
fn jpeg_decode_speed() {
    let (w, h) = (2048, 1536);
    // The test picture plus noise, so the entropy decoder has realistic work.
    let mut src = picture(w, h);
    let mut rng = Rng::new(9, 9);
    for v in src.iter_mut() {
        *v = v.saturating_add(rng.below(24) as u8);
    }
    for (name, opts) in [
        ("baseline 4:2:0", EncodeOptions { quality: 85, ..EncodeOptions::default() }),
        ("baseline 4:4:4", EncodeOptions { quality: 85, subsampling: Subsampling::S444, ..EncodeOptions::default() }),
        ("progressive 4:2:0", EncodeOptions { quality: 85, progressive: true, ..EncodeOptions::default() }),
    ] {
        let jpg = encode(w, h, &src, &opts).unwrap();
        let t = std::time::Instant::now();
        let n = 5;
        for _ in 0..n {
            decode(&jpg).unwrap();
        }
        let s = t.elapsed().as_secs_f64() / n as f64;
        println!("{name}: {} KB -> {:.1} ms, {:.0} MB/s RGBA out, {:.1} Mpx/s", jpg.len() / 1024, s * 1e3, (w * h * 4) as f64 / s / 1e6, (w * h) as f64 / s / 1e6);
    }
}
