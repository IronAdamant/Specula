//! Thumbnails, contact sheets and image stats for the visual history gallery (`scripts/gallery.py`).
//!
//! ```text
//! cargo run --release -p specula-core --example thumb -- IN OUT [WIDTH] [--quality Q] [--max-bytes N]
//! cargo run --release -p specula-core --example thumb -- sheet OUT --cell W [--cols N] IN...
//! cargo run --release -p specula-core --example thumb -- stats IN...
//! ```
//!
//! - **Thumbnail:** decodes a PNG or JPEG, box-filters it down to `WIDTH` pixels wide (default 640; never
//!   up) by exact area coverage in linear light, and writes a PNG or a JPEG by `OUT`'s extension. JPEGs are
//!   baseline 4:2:0 with optimised Huffman tables (quality 82 unless `--quality`); transparent pixels are
//!   composited over the gallery's dark background first. With `--max-bytes`, a JPEG is re-encoded at lower
//!   qualities (down to 40) until it fits. Every JPEG is decoded again and its PSNR against the source pixels
//!   must reach a floor that depends on the quality ([`psnr_floor`]); an image that misses it (fine detail,
//!   saturated 1-pixel colour) is retried at quality 92 with full-resolution chroma, and if that also misses,
//!   written as a PNG next to `OUT` (same stem, `.png`). `--max-bytes` caps PNG output too: a PNG over it
//!   is a `too_large` error (exit 1), never written. Prints one JSON line:
//!   `{in, out, format, width, height, bytes, quality, psnr}`; `out` is the file actually written.
//! - **Sheet:** fits each input into a `W`-wide 16:9 cell on a grid (`--cols`, default 4) with its 1-based
//!   index drawn in the corner: a quick way to review many candidate captures at once.
//! - **Stats:** one JSON line per input: size, mean and spread of luma, the fraction of near-black pixels
//!   (to catch black or failed frames), and an 8x8 luma signature (hex) so near-duplicate captures can be
//!   told apart by distance rather than by exact bytes.
//!
//! Inputs are capped at [`MAX_INPUT_PIXELS`] (checked from the header before decoding), outputs and sheets
//! at [`MAX_OUTPUT_PIXELS`]. Any failure prints a `{error, message, path}` JSON line and exits with status 1
//! (`stats` reports every input, then exits 1 if any failed).

use specula_core::deflate::Level;
use specula_core::jpeg::{self, ColorModel, EncodeOptions, Subsampling};
use specula_core::png::{self, Image};
use std::process::ExitCode;

/// Background behind transparent pixels in JPEG thumbnails and between sheet cells (sRGB).
const BACKGROUND: [u8; 3] = [16, 16, 20];
/// Largest input accepted (64 Mpx, 256 MiB decoded), checked from the header before decoding.
const MAX_INPUT_PIXELS: u64 = 1 << 26;
/// Largest thumbnail or contact sheet written (16 Mpx).
const MAX_OUTPUT_PIXELS: u64 = 1 << 24;
const MAX_WIDTH: u32 = 4096;
const MAX_CELL: u32 = 2048;
const MAX_COLS: usize = 64;
const MAX_CELLS: usize = 1024;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("sheet") => sheet(&args[1..]),
        Some("stats") => stats(&args[1..]),
        Some(_) => thumb(&args),
        None => Err(Failure::usage("no arguments")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            f.print();
            ExitCode::FAILURE
        }
    }
}

struct Failure {
    code: &'static str,
    message: String,
    path: String,
}

impl Failure {
    fn usage(message: &str) -> Failure {
        Failure {
            code: "usage",
            message: format!("{message}; usage: thumb IN OUT [WIDTH] [--quality Q] [--max-bytes N] | sheet OUT --cell W [--cols N] IN... | stats IN..."),
            path: String::new(),
        }
    }
    fn at(code: &'static str, path: &str, message: impl Into<String>) -> Failure {
        Failure { code, message: message.into(), path: path.to_owned() }
    }
    fn print(&self) {
        println!("{{\"error\":\"{}\",\"message\":\"{}\",\"path\":\"{}\"}}", self.code, esc(&self.message), esc(&self.path));
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn number<T: std::str::FromStr>(flag: &str, value: Option<&String>) -> Result<T, Failure> {
    value.and_then(|v| v.parse().ok()).ok_or_else(|| Failure::usage(&format!("{flag} needs a number in range")))
}

/// `w * h`, if it is at most `max`.
fn pixels_within(w: u64, h: u64, max: u64) -> bool {
    w.checked_mul(h).is_some_and(|n| n <= max)
}

/// A PNG's size from its IHDR chunk, without decoding.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((u32::from_be_bytes(bytes[16..20].try_into().ok()?), u32::from_be_bytes(bytes[20..24].try_into().ok()?)))
}

/// Decodes a PNG or JPEG (by signature) to RGBA8, refusing images over [`MAX_INPUT_PIXELS`] before decoding.
fn load(path: &str) -> Result<Image, Failure> {
    let bytes = std::fs::read(path).map_err(|e| Failure::at("read", path, e.to_string()))?;
    let is_jpeg = bytes.starts_with(&[0xFF, 0xD8]);
    let size = if is_jpeg {
        jpeg::info(&bytes).map(|i| (i.width, i.height)).map_err(|e| Failure::at("decode", path, e.to_string()))?
    } else {
        png_size(&bytes).ok_or_else(|| Failure::at("decode", path, "not a PNG or JPEG"))?
    };
    if !pixels_within(size.0 as u64, size.1 as u64, MAX_INPUT_PIXELS) {
        return Err(Failure::at("too_large", path, format!("{}x{} is over the {MAX_INPUT_PIXELS}-pixel input limit", size.0, size.1)));
    }
    let img = if is_jpeg {
        jpeg::decode(&bytes).map_err(|e| Failure::at("decode", path, e.to_string()))?
    } else {
        png::decode(&bytes).map_err(|e| Failure::at("decode", path, e.to_string()))?
    };
    if img.width == 0 || img.height == 0 {
        return Err(Failure::at("decode", path, "empty image"));
    }
    Ok(img)
}

fn is_jpeg(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

// ---------------------------------------------------------------------------------------------
// Resampling

fn srgb_to_linear(v: u8) -> f32 {
    let c = v as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// For each output index, the source indices it covers and their area weights (summing to 1): an
/// exact box filter for any ratio `src / dst >= 1`.
fn coverage(src: usize, dst: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|o| {
            let (lo, hi) = (o as f64 * scale, (o as f64 + 1.0) * scale);
            let mut taps = Vec::new();
            let mut i = lo.floor() as usize;
            while (i as f64) < hi && i < src {
                let w = (hi.min(i as f64 + 1.0) - lo.max(i as f64)) / scale;
                if w > 1e-9 {
                    taps.push((i, w as f32));
                }
                i += 1;
            }
            taps
        })
        .collect()
}

/// Box-filters `img` to `width` x `height` (each no larger than the source) in linear light with
/// premultiplied alpha. Source rows are converted one at a time, so the working memory is the
/// horizontally reduced image (`width * src height` pixels), not a float copy of the source.
fn resize(img: &Image, width: u32, height: u32) -> Image {
    let (sw, sh) = (img.width as usize, img.height as usize);
    let (dw, dh) = (width as usize, height as usize);
    let lut: Vec<f32> = (0..=255u8).map(srgb_to_linear).collect();
    let cols = coverage(sw, dw);
    let mut mid = vec![0f32; dw * sh * 4];
    let mut row = vec![0f32; sw * 4];
    for y in 0..sh {
        for (px, out) in img.rgba[y * sw * 4..(y + 1) * sw * 4].as_chunks::<4>().0.iter().zip(row.as_chunks_mut::<4>().0) {
            let a = px[3] as f32 / 255.0;
            out[0] = lut[px[0] as usize] * a;
            out[1] = lut[px[1] as usize] * a;
            out[2] = lut[px[2] as usize] * a;
            out[3] = a;
        }
        for (x, taps) in cols.iter().enumerate() {
            let o = (y * dw + x) * 4;
            for &(i, w) in taps {
                for c in 0..4 {
                    mid[o + c] += row[i * 4 + c] * w;
                }
            }
        }
    }
    let rows = coverage(sh, dh);
    let mut acc = vec![0f32; dw * dh * 4];
    for (y, taps) in rows.iter().enumerate() {
        for &(j, w) in taps {
            let src = &mid[j * dw * 4..(j + 1) * dw * 4];
            let dst = &mut acc[y * dw * 4..(y + 1) * dw * 4];
            for (d, s) in dst.iter_mut().zip(src) {
                *d += s * w;
            }
        }
    }
    let mut rgba = vec![0u8; dw * dh * 4];
    for (px, out) in acc.as_chunks::<4>().0.iter().zip(rgba.as_chunks_mut::<4>().0) {
        let a = px[3];
        if a > 1e-6 {
            out[0] = linear_to_srgb(px[0] / a);
            out[1] = linear_to_srgb(px[1] / a);
            out[2] = linear_to_srgb(px[2] / a);
        }
        out[3] = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
    Image { width, height, rgba }
}

/// The image composited over [`BACKGROUND`] (alpha 255 everywhere), for JPEG.
fn flatten(img: &Image) -> Image {
    let mut rgba = img.rgba.clone();
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a < 255 {
            for c in 0..3 {
                px[c] = ((px[c] as u32 * a + BACKGROUND[c] as u32 * (255 - a) + 127) / 255) as u8;
            }
            px[3] = 255;
        }
    }
    Image { width: img.width, height: img.height, rgba }
}

// ---------------------------------------------------------------------------------------------
// Encoding

/// The PSNR (dB, RGB) a JPEG at `quality` must reach against its source to count as a faithful
/// thumbnail. Rendered frames at quality 82 measure about 33-40 dB; the floor sits well below that
/// and rises with quality, so only images JPEG really mangles (dense 1-pixel colour detail) miss it.
fn psnr_floor(quality: u8) -> f64 {
    18.0 + 0.12 * quality as f64
}

/// PSNR in dB over the RGB channels (alpha ignored); infinite for identical images.
fn psnr(a: &Image, b: &Image) -> f64 {
    let mut sum = 0f64;
    let mut n = 0u64;
    for (x, y) in a.rgba.as_chunks::<4>().0.iter().zip(b.rgba.as_chunks::<4>().0) {
        for c in 0..3 {
            let d = x[c] as f64 - y[c] as f64;
            sum += d * d;
        }
        n += 3;
    }
    if n == 0 || sum == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / (sum / n as f64)).log10()
}

struct Encoded {
    bytes: Vec<u8>,
    out: String,
    format: &'static str,
    quality: u8,
    psnr: f64,
}

/// One JPEG encode plus its round trip: the bytes and their PSNR against `flat`.
fn jpeg_try(flat: &Image, quality: u8, subsampling: Subsampling, out: &str) -> Result<(Vec<u8>, f64), Failure> {
    let opts = EncodeOptions { quality, subsampling, color: ColorModel::YCbCr, optimize: true, ..EncodeOptions::default() };
    let bytes = jpeg::encode(flat.width, flat.height, &flat.rgba, &opts).map_err(|e| Failure::at("encode", out, e.to_string()))?;
    let back = jpeg::decode(&bytes).map_err(|e| Failure::at("roundtrip", out, e.to_string()))?;
    if back.width != flat.width || back.height != flat.height {
        return Err(Failure::at("roundtrip", out, "decoded JPEG has a different size"));
    }
    Ok((bytes, psnr(flat, &back)))
}

/// Encodes by `out`'s extension. A JPEG steps its quality down (to 40) to fit `max_bytes` and must meet
/// [`psnr_floor`]; when it can't, it is retried at quality 92 with 4:4:4 chroma, then written as a PNG
/// with `out`'s stem.
fn encode(img: &Image, out: &str, quality: u8, max_bytes: Option<usize>) -> Result<Encoded, Failure> {
    let png_out = |path: String| {
        let bytes = png::encode_rgba8(img.width, img.height, &img.rgba, Level::Best);
        match max_bytes {
            Some(max) if bytes.len() > max => Err(Failure::at(
                "too_large",
                &path,
                format!("as a PNG this {}x{} image is {} bytes, over --max-bytes {max}: use a smaller WIDTH", img.width, img.height, bytes.len()),
            )),
            _ => Ok(Encoded { bytes, out: path, format: "png", quality: 0, psnr: f64::INFINITY }),
        }
    };
    if !is_jpeg(out) {
        return png_out(out.to_owned());
    }
    let flat = flatten(img);
    let mut q = quality;
    loop {
        let (bytes, p) = jpeg_try(&flat, q, Subsampling::S420, out)?;
        if p < psnr_floor(q) {
            break;
        }
        match max_bytes {
            Some(max) if bytes.len() > max && q > 40 => q = q.saturating_sub(6).max(40),
            _ => return Ok(Encoded { bytes, out: out.to_owned(), format: "jpeg", quality: q, psnr: p }),
        }
    }
    let q = quality.max(92);
    let (bytes, p) = jpeg_try(&flat, q, Subsampling::S444, out)?;
    if p >= psnr_floor(q) && max_bytes.is_none_or(|max| bytes.len() <= max) {
        return Ok(Encoded { bytes, out: out.to_owned(), format: "jpeg", quality: q, psnr: p });
    }
    let stem = out.rfind('.').map_or(out, |i| &out[..i]);
    png_out(format!("{stem}.png"))
}

fn write(path: &str, bytes: &[u8]) -> Result<(), Failure> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Failure::at("write", path, e.to_string()))?;
        }
    }
    std::fs::write(path, bytes).map_err(|e| Failure::at("write", path, e.to_string()))
}

fn thumb(args: &[String]) -> Result<(), Failure> {
    let mut positional = Vec::new();
    let mut quality = 82u8;
    let mut max_bytes = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--quality" => quality = number::<u8>("--quality", it.next())?.clamp(1, 100),
            "--max-bytes" => max_bytes = Some(number::<usize>("--max-bytes", it.next())?),
            _ => positional.push(arg.as_str()),
        }
    }
    let (input, out, width) = match positional.as_slice() {
        [i, o] => (*i, *o, 640u32),
        [i, o, w] => (*i, *o, w.parse::<u32>().ok().filter(|w| (1..=MAX_WIDTH).contains(w)).ok_or_else(|| Failure::usage(&format!("WIDTH must be 1..={MAX_WIDTH}")))?),
        _ => return Err(Failure::usage("expected IN OUT [WIDTH]")),
    };
    let img = load(input)?;
    let small = if img.width > width {
        let height = ((img.height as u64 * width as u64 + img.width as u64 / 2) / img.width as u64).max(1);
        if !pixels_within(width as u64, height, MAX_OUTPUT_PIXELS) {
            return Err(Failure::at("too_large", input, format!("a {width}-wide thumbnail would be {height} px tall")));
        }
        resize(&img, width, height as u32)
    } else {
        if !pixels_within(img.width as u64, img.height as u64, MAX_OUTPUT_PIXELS) {
            return Err(Failure::at("too_large", input, format!("{}x{} is over the {MAX_OUTPUT_PIXELS}-pixel output limit", img.width, img.height)));
        }
        img
    };
    let enc = encode(&small, out, quality, max_bytes)?;
    write(&enc.out, &enc.bytes)?;
    let psnr = if enc.psnr.is_finite() { format!("{:.2}", enc.psnr) } else { "null".to_owned() };
    println!(
        "{{\"in\":\"{}\",\"out\":\"{}\",\"format\":\"{}\",\"width\":{},\"height\":{},\"bytes\":{},\"quality\":{},\"psnr\":{psnr}}}",
        esc(input),
        esc(&enc.out),
        enc.format,
        small.width,
        small.height,
        enc.bytes.len(),
        enc.quality
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Contact sheets

/// 3x5 digit glyphs, one row per `u8` (low 3 bits, leftmost pixel in bit 2).
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 3, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

fn fill(img: &mut Image, x0: usize, y0: usize, w: usize, h: usize, rgb: [u8; 3]) {
    let iw = img.width as usize;
    for y in y0..(y0 + h).min(img.height as usize) {
        for x in x0..(x0 + w).min(iw) {
            let o = (y * iw + x) * 4;
            img.rgba[o..o + 3].copy_from_slice(&rgb);
            img.rgba[o + 3] = 255;
        }
    }
}

fn label(img: &mut Image, x0: usize, y0: usize, n: usize) {
    let text = n.to_string();
    let scale = 3;
    fill(img, x0, y0, text.len() * 4 * scale + scale, 7 * scale, [0, 0, 0]);
    for (k, ch) in text.bytes().enumerate() {
        let glyph = DIGITS[(ch - b'0') as usize];
        for (gy, bits) in glyph.iter().enumerate() {
            for gx in 0..3 {
                if bits & (4 >> gx) != 0 {
                    fill(img, x0 + scale + (k * 4 + gx) * scale, y0 + scale + gy * scale, scale, scale, [255, 230, 80]);
                }
            }
        }
    }
}

fn sheet(args: &[String]) -> Result<(), Failure> {
    let mut cell = 0u32;
    let mut cols = 4usize;
    let mut out = None;
    let mut inputs = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--cell" => cell = number("--cell", it.next())?,
            "--cols" => cols = number("--cols", it.next())?,
            _ if out.is_none() => out = Some(arg.as_str()),
            _ => inputs.push(arg.as_str()),
        }
    }
    let Some(out) = out else { return Err(Failure::usage("sheet needs OUT")) };
    if !(16..=MAX_CELL).contains(&cell) || !(1..=MAX_COLS).contains(&cols) || inputs.is_empty() || inputs.len() > MAX_CELLS {
        return Err(Failure::usage(&format!("sheet needs --cell 16..={MAX_CELL}, --cols 1..={MAX_COLS} and 1..={MAX_CELLS} inputs")));
    }
    let (cw, ch, gap) = (cell as usize, (cell as usize * 9).div_ceil(16), 4usize);
    let rows = inputs.len().div_ceil(cols);
    let (w, h) = (cols * (cw + gap) + gap, rows * (ch + gap) + gap);
    if !pixels_within(w as u64, h as u64, MAX_OUTPUT_PIXELS) {
        return Err(Failure::at("too_large", out, format!("a {w}x{h} sheet is over the {MAX_OUTPUT_PIXELS}-pixel limit: smaller --cell or fewer inputs")));
    }
    let mut img = Image { width: w as u32, height: h as u32, rgba: vec![0; w * h * 4] };
    fill(&mut img, 0, 0, w, h, BACKGROUND);
    for (i, path) in inputs.iter().enumerate() {
        let (x0, y0) = (gap + (i % cols) * (cw + gap), gap + (i / cols) * (ch + gap));
        match load(path) {
            Ok(src) => {
                let s = (cw as f64 / src.width as f64).min(ch as f64 / src.height as f64).min(1.0);
                let (tw, th) = (((src.width as f64 * s).round() as u32).clamp(1, cw as u32), ((src.height as f64 * s).round() as u32).clamp(1, ch as u32));
                let t = if (tw, th) == (src.width, src.height) { src } else { resize(&src, tw, th) };
                let t = flatten(&t);
                let (ox, oy) = (x0 + (cw - tw as usize) / 2, y0 + (ch - th as usize) / 2);
                for y in 0..th as usize {
                    let s = &t.rgba[y * tw as usize * 4..(y + 1) * tw as usize * 4];
                    let o = ((oy + y) * w + ox) * 4;
                    img.rgba[o..o + s.len()].copy_from_slice(s);
                }
            }
            Err(_) => fill(&mut img, x0, y0, cw, ch, [120, 0, 0]),
        }
        label(&mut img, x0, y0, i + 1);
    }
    let enc = encode(&img, out, 85, None)?;
    write(&enc.out, &enc.bytes)?;
    println!("{{\"out\":\"{}\",\"width\":{w},\"height\":{h},\"cells\":{}}}", esc(&enc.out), inputs.len());
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Stats

fn stats(args: &[String]) -> Result<(), Failure> {
    if args.is_empty() {
        return Err(Failure::usage("stats needs at least one input"));
    }
    let mut failed = 0usize;
    for path in args {
        let img = match load(path) {
            Ok(img) => img,
            Err(f) => {
                f.print();
                failed += 1;
                continue;
            }
        };
        let n = (img.width as usize * img.height as usize).max(1);
        let luma: Vec<f64> = img
            .rgba
            .as_chunks::<4>().0.iter()
            .map(|p| (0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64) * p[3] as f64 / 255.0)
            .collect();
        let mean = luma.iter().sum::<f64>() / n as f64;
        let spread = (luma.iter().map(|l| (l - mean) * (l - mean)).sum::<f64>() / n as f64).sqrt();
        let dark = luma.iter().filter(|&&l| l < 8.0).count() as f64 / n as f64;
        let mut sig = String::with_capacity(128);
        let (w, h) = (img.width as usize, img.height as usize);
        for gy in 0..8 {
            for gx in 0..8 {
                let (x0, x1) = (gx * w / 8, ((gx + 1) * w / 8).max(gx * w / 8 + 1).min(w));
                let (y0, y1) = (gy * h / 8, ((gy + 1) * h / 8).max(gy * h / 8 + 1).min(h));
                let mut sum = 0.0;
                let mut count = 0usize;
                for y in y0..y1 {
                    for x in x0..x1 {
                        sum += luma[y * w + x];
                        count += 1;
                    }
                }
                sig.push_str(&format!("{:02x}", (sum / count.max(1) as f64).round().clamp(0.0, 255.0) as u8));
            }
        }
        println!(
            "{{\"path\":\"{}\",\"width\":{},\"height\":{},\"mean\":{mean:.2},\"spread\":{spread:.2},\"dark\":{dark:.4},\"sig\":\"{sig}\"}}",
            esc(path),
            img.width,
            img.height
        );
    }
    if failed > 0 {
        return Err(Failure::at("stats_failed", "", format!("{failed} of {} inputs could not be read", args.len())));
    }
    Ok(())
}
