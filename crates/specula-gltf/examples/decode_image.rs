//! Decodes PNG/JPEG files (or every one in a folder) with the importer's decoders, printing
//! size, process and timing (the fastest of `--repeat N` runs); `--out DIR` writes each as PNG
//! so it can be looked at.
//!
//! ```text
//! cargo run --release -p specula-gltf --example decode_image -- photo.jpg textures/ [--repeat 5] [--out target/decoded]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use specula_core::deflate::Level;
use specula_core::{jpeg, png};

fn decode_one(path: &Path, out: Option<&Path>, repeat: usize) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut best = f64::MAX;
    let mut result = None;
    for _ in 0..repeat.max(1) {
        let t = Instant::now();
        let r = if bytes.starts_with(&[0xFF, 0xD8]) {
            let info = jpeg::info(&bytes).map_err(|e| e.to_string())?;
            let img = jpeg::decode(&bytes).map_err(|e| e.to_string())?;
            (img, format!("JPEG {} components{}", info.components, if info.progressive { ", progressive" } else { "" }))
        } else {
            (png::decode(&bytes).map_err(|e| e.message)?, "PNG".to_string())
        };
        best = best.min(t.elapsed().as_secs_f64() * 1e3);
        result = Some(r);
    }
    let (img, what) = result.expect("at least one run");
    let ms = best;
    if let Some(dir) = out {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let name = dir.join(format!("{}.png", path.file_stem().unwrap_or_default().to_string_lossy()));
        std::fs::write(&name, png::encode_rgba8(img.width, img.height, &img.rgba, Level::Fastest)).map_err(|e| e.to_string())?;
    }
    Ok(format!("{}x{} {what}, {} KB, {ms:.1} ms ({:.1} Mpx/s)", img.width, img.height, bytes.len() / 1024, (img.width as f64 * img.height as f64) / ms / 1e3))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out: Option<PathBuf> = None;
    let mut repeat = 1;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--out" {
            out = it.next().map(PathBuf::from);
            continue;
        }
        if a == "--repeat" {
            repeat = it.next().and_then(|n| n.parse().ok()).unwrap_or(1);
            continue;
        }
        let p = PathBuf::from(a);
        if p.is_dir() {
            let mut list: Vec<PathBuf> = std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()).filter(|f| f.extension().is_some_and(|x| ["jpg", "jpeg", "png"].contains(&x.to_string_lossy().to_lowercase().as_str()))).collect();
            list.sort();
            files.extend(list);
        } else {
            files.push(p);
        }
    }
    if files.is_empty() {
        eprintln!("usage: decode_image <file or folder>... [--out DIR]");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for f in &files {
        match decode_one(f, out.as_deref(), repeat) {
            Ok(s) => println!("{}: {s}", f.display()),
            Err(e) => {
                failed = true;
                println!("{}: error: {e}", f.display());
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
