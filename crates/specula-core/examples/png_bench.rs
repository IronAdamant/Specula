//! PNG encode benchmark: a realistic capture frame at every level, one thread vs several.
//!
//! ```text
//! cargo run --release -p specula-core --example png_bench [-- [FRAME.png] [--threads N] [--out DIR]]
//! ```
//!
//! Uses `target/lava/all.png` (the lava-lamp benchmark capture, `python scripts/lava_lamps.py`)
//! when present, else a synthetic dithered gradient. The 1280×720 case is a centre crop of the
//! frame. Every encode is decoded again and checked, and the single- and multi-threaded outputs
//! must be byte-identical. `--threads N` sets the parallel column's thread count (default:
//! `deflate::available_threads()`), e.g. 4 to approximate a low-end laptop. `--out DIR` also
//! writes each encoded PNG, plus multi-band zlib streams of non-image data, so an independent
//! decoder (e.g. Python's `zlib`) can check them. A very wide image (rows spanning many bands)
//! is timed too.

use specula_core::deflate::{self, Level};
use specula_core::png::{self, Image};
use std::time::Instant;

const RUNS: usize = 7;
const LEVELS: [Level; 4] = [Level::Fastest, Level::Fast, Level::Default, Level::Best];

fn main() {
    let mut path = "target/lava/all.png".to_string();
    let mut out_dir = None;
    let mut threads = deflate::available_threads();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--threads" => threads = args.next().and_then(|n| n.parse().ok()).expect("--threads needs a number"),
            "--out" => out_dir = Some(std::path::PathBuf::from(args.next().expect("--out needs a directory"))),
            _ => path = arg,
        }
    }
    if let Some(dir) = &out_dir {
        std::fs::create_dir_all(dir).expect("create output directory");
    }

    let frame = match std::fs::read(&path) {
        Ok(bytes) => {
            let img = png::decode(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            println!("frame: {path} ({}x{})", img.width, img.height);
            img
        }
        Err(_) => {
            println!("frame: synthetic dithered gradient ({path} not found)");
            synthetic(1600, 900)
        }
    };
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    println!("parallel column: {threads} threads (machine has {cores})\n");
    for (w, h) in [(frame.width, frame.height), (1280, 720)] {
        let img = crop(&frame, w, h);
        let (w, h) = (img.width, img.height);
        println!("{w}x{h} ({} KB raw RGBA)", img.rgba.len() / 1024);
        println!("  {:<8} {:>10} {:>10} {:>8} {:>9}", "level", "1 thread", "parallel", "speedup", "size KB");
        for level in LEVELS {
            let (single_ms, single) = time(|| png::encode_rgba8_threads(w, h, &img.rgba, level, 1));
            let (par_ms, par) = time(|| png::encode_rgba8_threads(w, h, &img.rgba, level, threads));
            assert_eq!(single, par, "{level:?}: output depends on the thread count");
            assert_eq!(png::decode(&par).expect("decodes").rgba, img.rgba, "{level:?}: round trip");
            println!(
                "  {:<8} {:>7.1} ms {:>7.1} ms {:>7.1}x {:>9}",
                format!("{level:?}"),
                single_ms,
                par_ms,
                single_ms / par_ms,
                par.len() / 1024
            );
            if let Some(dir) = &out_dir {
                std::fs::write(dir.join(format!("frame_{w}x{h}_{level:?}.png")), &par).expect("write PNG");
            }
        }
        println!();
    }

    // A very wide image: each row spans many compression bands.
    let (w, h) = (1u32 << 20, 2u32);
    let wide = synthetic(w, h);
    println!("{w}x{h} (rows wider than a band)");
    for level in [Level::Fastest, Level::Default] {
        let (single_ms, single) = time(|| png::encode_rgba8_threads(w, h, &wide.rgba, level, 1));
        let (par_ms, par) = time(|| png::encode_rgba8_threads(w, h, &wide.rgba, level, threads));
        assert_eq!(single, par, "{level:?}: output depends on the thread count");
        assert_eq!(png::decode(&par).expect("decodes").rgba, wide.rgba, "{level:?}: round trip");
        println!("  {:<8} {:>7.1} ms {:>7.1} ms {:>9} KB", format!("{level:?}"), single_ms, par_ms, par.len() / 1024);
    }
    println!();

    if let Some(dir) = &out_dir {
        // Non-image data across many bands: text-like words, runs and noise.
        let mut data = Vec::new();
        let mut seed = 0x2545_F491u32;
        while data.len() < 3 * deflate::BAND_SIZE + 12_345 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            match seed % 4 {
                0 => data.extend(std::iter::repeat_n(seed as u8, (seed >> 8) as usize % 700)),
                1 => data.extend((0..(seed >> 8) % 64).map(|i| (seed >> (i % 24)) as u8)),
                _ => data.extend_from_slice(b"specula watchtower agent frame "),
            }
        }
        std::fs::write(dir.join("stream.bin"), &data).expect("write stream");
        for level in LEVELS {
            let z = deflate::zlib_compress(&data, level);
            assert_eq!(z, deflate::zlib_compress_threads(&data, level, 1));
            std::fs::write(dir.join(format!("stream_{level:?}.zlib")), z).expect("write zlib");
        }
        println!("wrote PNGs and zlib streams to {}", dir.display());
    }
}

/// Best of [`RUNS`] wall-clock times in milliseconds, plus the last result.
fn time(mut f: impl FnMut() -> Vec<u8>) -> (f64, Vec<u8>) {
    let mut best = f64::MAX;
    let mut out = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        out = f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    (best, out)
}

/// Centre crop to `w`×`h` (or the whole image if it is not larger).
fn crop(img: &Image, w: u32, h: u32) -> Image {
    let (w, h) = (w.min(img.width), h.min(img.height));
    let (x0, y0) = ((img.width - w) / 2, (img.height - h) / 2);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y0 + h {
        let start = ((y * img.width + x0) * 4) as usize;
        rgba.extend_from_slice(&img.rgba[start..start + (w * 4) as usize]);
    }
    Image { width: w, height: h, rgba }
}

/// A dark-to-warm gradient with ±2 levels of hash noise, like the engine's dithered output.
fn synthetic(w: u32, h: u32) -> Image {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let n = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)).wrapping_mul(2_654_435_761) >> 29;
            let d = n as i32 - 4; // -4..=3
            let base = [20 + 200 * x / w, 30 + 120 * y / h, 60 + 40 * (x + y) / (w + h)];
            for c in base {
                rgba.push((c as i32 + d / 2).clamp(0, 255) as u8);
            }
            rgba.push(255);
        }
    }
    Image { width: w, height: h, rgba }
}
