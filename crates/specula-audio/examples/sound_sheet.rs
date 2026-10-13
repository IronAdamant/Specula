//! Renders every sound recipe in a folder to WAV, plus a contact sheet of waveforms and
//! spectrograms and a JSON report of the numbers, so an agent can *see* a sound set.
//!
//! ```text
//! cargo run --release -p specula-audio --example sound_sheet [recipes_dir] [out_dir]
//! ```
//!
//! Defaults: `games/sepulcra/assets/sounds` → `target/sound-sheet/` (`<name>.wav`,
//! `sound_sheet.png`, `report.json`). Exits non-zero if any recipe fails, clips, or misses its
//! loudness target by more than 0.5 LU.

use specula_audio::{analysis, synth, wav, RenderOptions};
use specula_core::Json;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args().skip(1);
    let dir = args.next().map(PathBuf::from).unwrap_or_else(|| root.join("games/sepulcra/assets/sounds"));
    let out = args.next().map(PathBuf::from).unwrap_or_else(|| root.join("target/sound-sheet"));
    std::fs::create_dir_all(&out).expect("create output dir");

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();

    let mut panels = Vec::new();
    let mut rows = Vec::new();
    let mut failures = 0;
    println!("{:<22} {:>8} {:>6} {:>7} {:>7} {:>7} {:>7} {:>8} {:>7} {:>6}", "sound", "bus", "dur s", "peak", "rms", "LUFS", "target", "centroid", "norm dB", "ms");
    for path in &files {
        let text = std::fs::read_to_string(path).expect("read recipe");
        let t0 = Instant::now();
        let recipe = match synth::Recipe::parse(&text) {
            Ok(r) => r,
            Err(e) => {
                println!("{}: {e}", path.display());
                failures += 1;
                continue;
            }
        };
        let r = match synth::render(&recipe, &RenderOptions::default()) {
            Ok(r) => r,
            Err(e) => {
                println!("{}: {e}", path.display());
                failures += 1;
                continue;
            }
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let m = r.metrics();
        let bytes = wav::encode(&r.samples, r.channels, r.rate, wav::WavFormat::Pcm16);
        std::fs::write(out.join(format!("{}.wav", r.name)), &bytes).expect("write wav");
        let back = wav::decode(&bytes).expect("decode our own wav");
        assert_eq!(back.samples.len(), r.samples.len());

        let target = recipe.loudness;
        let miss = target.map_or(0.0, |t| (m.lufs - t).abs());
        let bad = m.clipped > 0 || miss > 0.5;
        if bad {
            failures += 1;
        }
        println!(
            "{:<22} {:>8} {:>6.2} {:>7.1} {:>7.1} {:>7.1} {:>7} {:>8.0} {:>7.1} {:>6.1}{}",
            r.name,
            r.bus.name(),
            m.duration_s,
            m.peak_dbfs,
            m.rms_dbfs,
            m.lufs,
            target.map_or("-".to_string(), |t| format!("{t:.0}")),
            m.centroid_hz,
            r.normalize_db,
            ms,
            if bad { "  <-- CHECK" } else { "" }
        );
        let mut title = format!("{} ({}{})", r.name, r.bus.name(), if r.looping { ", loop" } else { "" });
        if r.ceiling_db < 0.0 {
            title.push_str(&format!(" ceil {:.1}DB", r.ceiling_db));
        }
        panels.push(analysis::sound_panel(&title, &r.samples, r.channels as usize, r.rate, 460, &m));
        rows.push(Json::obj([
            ("name", Json::from(r.name.as_str())),
            ("bus", Json::from(r.bus.name())),
            ("channels", Json::from(r.channels as u32)),
            ("looping", Json::from(r.looping)),
            ("target_lufs", Json::from(target)),
            ("normalize_db", Json::from((r.normalize_db * 100.0).round() / 100.0)),
            ("ceiling_db", Json::from((r.ceiling_db * 100.0).round() / 100.0)),
            ("hash", Json::from(format!("{:016x}", r.hash))),
            ("render_ms", Json::from((ms * 10.0).round() / 10.0)),
            ("metrics", m.to_json()),
        ]));
    }
    let sheet = analysis::contact_sheet(&format!("SEPULCRA SOUND SET - {} SOUNDS - 48 KHZ", panels.len()), &panels, 3);
    let png = out.join("sound_sheet.png");
    std::fs::write(&png, analysis::encode_png(&sheet)).expect("write png");
    std::fs::write(out.join("report.json"), Json::obj([("sounds", Json::Arr(rows))]).to_pretty()).expect("write report");
    println!("\nwrote {} WAVs, {} and report.json to {}", panels.len(), png.display(), out.display());
    if failures > 0 {
        println!("{failures} sound(s) need attention");
        std::process::exit(1);
    }
}
