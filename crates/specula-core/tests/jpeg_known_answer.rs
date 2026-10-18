//! JPEG known-answer test against an external codec: Windows GDI+ (System.Drawing), so the encoder and
//! decoder agree with the rest of the world and not only with each other.
//!
//! Fixtures in `tests/data/` (all 96x64):
//! - `kat_source.png`: a synthetic test picture (gradients, a checkerboard, a disc, saturated stripes),
//!   written by a small standard-library Python script.
//! - `kat_gdi.jpg`: the source encoded by GDI+ at quality 85; `kat_gdi_decoded.png`: GDI+'s own decode of it.
//! - `kat_specula.jpg`: the source encoded the way the gallery's `thumb` example writes thumbnails
//!   (baseline 4:2:0, optimised Huffman tables, quality 82); `kat_specula_gdi_decoded.png`: GDI+'s decode
//!   of that file.
//!
//! GDI+ upsamples chroma smoothly where this decoder replicates samples, so the decodes are compared by
//! PSNR: luma everywhere (measured about 57 dB), chroma in the source's smooth areas (about 41 dB).
//! Full-resolution RGB differs at sharp colour edges by design (about 26 dB). The encoder is checked
//! twice: GDI+'s decode of our JPEG must be a faithful picture of the source (measured: luma about 42 dB,
//! smooth chroma about 39 dB), so a broken encoder fails even after the fixtures are regenerated; and its
//! bytes are pinned.
//!
//! Regenerate (Windows), from `crates/specula-core`: `python tests/data/kat_make_source.py
//! tests/data/kat_source.png`, then `cargo test --test jpeg_known_answer -- --ignored
//! regenerate_kat_specula` (the exact pinned options), then `powershell -File tests/data/kat_gdi.ps1
//! -dir <absolute path of tests/data>`.

use specula_core::jpeg::{self, ColorModel, EncodeOptions, Subsampling};
use specula_core::png::{self, Image};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)).unwrap()
}

fn png(name: &str) -> Image {
    png::decode(&data(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Luma (BT.601, as JPEG stores it) of every pixel.
fn luma(img: &Image) -> Vec<f64> {
    img.rgba.chunks_exact(4).map(|p| 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64).collect()
}

fn cbcr(p: &[u8]) -> (f64, f64) {
    let (r, g, b) = (p[0] as f64, p[1] as f64, p[2] as f64);
    (-0.168736 * r - 0.331264 * g + 0.5 * b, 0.5 * r - 0.418688 * g - 0.081312 * b)
}

/// Cb and Cr of the pixels in the source's smooth areas: those whose 5x5 neighbourhood in the source
/// varies by less than 12 in Cb and Cr. There smooth and replicating chroma upsampling must agree;
/// at sharp colour edges they differ by design.
fn smooth_chroma(img: &Image, source: &Image) -> Vec<f64> {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut out = Vec::new();
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            let at = |i: &Image, x: usize, y: usize| cbcr(&i.rgba[(y * w + x) * 4..][..3]);
            let c0 = at(source, x, y);
            let flat = (y - 2..=y + 2).all(|yy| (x - 2..=x + 2).all(|xx| {
                let c = at(source, xx, yy);
                (c.0 - c0.0).abs() < 12.0 && (c.1 - c0.1).abs() < 12.0
            }));
            if flat {
                let c = at(img, x, y);
                out.push(c.0);
                out.push(c.1);
            }
        }
    }
    out
}

fn rgb(img: &Image) -> Vec<f64> {
    img.rgba.chunks_exact(4).flat_map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect()
}

fn psnr(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mse = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>() / a.len() as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

/// Our decode and GDI+'s agree: luma everywhere, chroma in the source's smooth areas.
fn agree(ours: &Image, theirs: &Image, what: &str) {
    assert_eq!((ours.width, ours.height), (theirs.width, theirs.height));
    let source = png("kat_source.png");
    let y = psnr(&luma(ours), &luma(theirs));
    let (a, b) = (smooth_chroma(ours, &source), smooth_chroma(theirs, &source));
    assert!(a.len() > 2 * 2000, "too few smooth pixels to compare chroma ({})", a.len() / 2);
    let c = psnr(&a, &b);
    println!("{what}: luma {y:.2} dB, smooth chroma {c:.2} dB over {} px, full RGB {:.2} dB", a.len() / 2, psnr(&rgb(ours), &rgb(theirs)));
    assert!(y >= 50.0, "{what}: luma differs from GDI+'s decode ({y:.2} dB)");
    assert!(c >= 37.0, "{what}: chroma differs from GDI+'s decode ({c:.2} dB)");
}

#[test]
fn our_decoder_matches_gdiplus_on_a_gdiplus_jpeg() {
    let ours = jpeg::decode(&data("kat_gdi.jpg")).unwrap();
    agree(&ours, &png("kat_gdi_decoded.png"), "a GDI+ JPEG");
    // And it is a fair picture of the source.
    assert!(psnr(&luma(&ours), &luma(&png("kat_source.png"))) >= 30.0);
}

#[test]
fn gdiplus_decodes_our_jpeg_as_we_do() {
    let ours = jpeg::decode(&data("kat_specula.jpg")).unwrap();
    let theirs = png("kat_specula_gdi_decoded.png");
    agree(&ours, &theirs, "our JPEG");
    // What an outside decoder sees must be the source: this is what catches a broken encoder whose
    // output both decoders agree on.
    let source = png("kat_source.png");
    let y = psnr(&luma(&theirs), &luma(&source));
    let c = psnr(&smooth_chroma(&theirs, &source), &smooth_chroma(&source, &source));
    println!("GDI+ decode of our JPEG vs the source: luma {y:.2} dB, smooth chroma {c:.2} dB");
    assert!(y >= 36.0, "GDI+'s decode of our JPEG is not the source picture (luma {y:.2} dB)");
    assert!(c >= 34.0, "GDI+'s decode of our JPEG has the wrong colours (smooth chroma {c:.2} dB)");
}

/// The options the gallery's `thumb` example uses for a thumbnail that meets its quality floor.
fn thumbnail_options() -> EncodeOptions {
    EncodeOptions { quality: 82, subsampling: Subsampling::S420, color: ColorModel::YCbCr, optimize: true, ..EncodeOptions::default() }
}

#[test]
fn thumbnail_encoding_is_pinned() {
    let source = png("kat_source.png");
    let bytes = jpeg::encode(source.width, source.height, &source.rgba, &thumbnail_options()).unwrap();
    assert!(bytes == data("kat_specula.jpg"), "the thumbnail encoder's output changed: regenerate the fixtures (see the module docs)");
}

/// Writes `kat_specula.jpg` with the pinned options (run on purpose: `-- --ignored regenerate_kat_specula`),
/// then re-run `kat_gdi.ps1` so GDI+'s decode matches it.
#[test]
#[ignore]
fn regenerate_kat_specula() {
    let source = png("kat_source.png");
    let bytes = jpeg::encode(source.width, source.height, &source.rgba, &thumbnail_options()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/kat_specula.jpg");
    std::fs::write(&path, bytes).unwrap();
    println!("wrote {}", path.display());
}
