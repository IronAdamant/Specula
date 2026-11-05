//! Sepulcra's music (games/sepulcra/assets/music) stays valid, levelled, seamless and cheap.

use specula_audio::music::{self, Music, MusicSource, RATE};
use specula_audio::{analysis, StreamSource};
use std::path::PathBuf;
use std::sync::Arc;

fn piece(name: &str) -> Music {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../games/sepulcra/assets/music/{name}.json"));
    let m = Music::parse(&std::fs::read_to_string(&p).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(m.name, name, "a piece's name matches its file");
    m
}

#[test]
fn map1_loops_at_its_target_without_clipping() {
    let m = piece("map1");
    let start = std::time::Instant::now();
    let r = Arc::new(music::render(&m).unwrap());
    let render_s = start.elapsed().as_secs_f64();
    let lap_s = r.lap_frames as f64 / RATE as f64;
    assert!((50.0..70.0).contains(&lap_s), "about a minute a lap: {lap_s}");
    // Two laps as the game hears them (the second carries the first's tails).
    let mut src = MusicSource::new(r.clone(), true);
    let mut out = vec![0f32; (r.lap_frames * 2 * 2) as usize];
    let n = src.read(&mut out).unwrap();
    assert_eq!(n as u64, r.lap_frames * 2);
    let second = &out[(r.lap_frames * 2) as usize..];
    let metrics = analysis::analyze(second, 2, RATE);
    assert!((metrics.lufs - -29.0).abs() < 1.0, "map 1 sits at -29 LUFS: {}", metrics.lufs);
    assert_eq!(metrics.clipped, 0);
    assert!(metrics.peak_dbfs <= -1.0 + 0.01, "{}", metrics.peak_dbfs);
    // The seam between laps is as smooth as the music around it (no click): the largest
    // sample-to-sample step there is no bigger than the largest anywhere in the lap.
    let lap = (r.lap_frames * 2) as usize;
    let step = |a: &[f32]| a.windows(4).map(|w| (w[2] - w[0]).abs().max((w[3] - w[1]).abs())).fold(0f32, f32::max);
    let seam = step(&out[lap - 2000..lap + 2000]);
    let anywhere = step(&out[..lap]);
    assert!(seam <= anywhere * 1.01, "seam step {seam} vs {anywhere}");
    // Rendering is the expensive part (instruments, transpositions); it runs on a worker.
    eprintln!("map1: {} notes, {:.1} s a lap, rendered in {render_s:.2} s, {:.1} LUFS", r.notes(), lap_s, metrics.lufs);
}

#[test]
fn cues_play_once_at_their_targets() {
    for (name, target) in [("victory", -22.0), ("defeat", -24.0)] {
        let m = piece(name);
        let r = Arc::new(music::render(&m).unwrap());
        let mut src = MusicSource::new(r.clone(), false);
        let total = src.total_frames().unwrap() as usize;
        let mut out = vec![0f32; total * 2];
        assert_eq!(src.read(&mut out).unwrap(), total);
        assert_eq!(src.read(&mut out).unwrap(), 0, "{name} ends");
        let metrics = analysis::analyze(&out, 2, RATE);
        assert!((metrics.lufs - target).abs() < 1.5, "{name}: {}", metrics.lufs);
        assert_eq!(metrics.clipped, 0, "{name}");
        // It ends in silence (the last notes ring out or fade over 50 ms), not cut mid-note.
        let tail = &out[out.len() - 200..];
        assert!(tail.iter().all(|s| s.abs() < 0.02), "{name} rings out");
        // ... and ends when its last note has rung out, not after a stretch of silence.
        let last_second = &out[out.len() - 2 * RATE as usize..];
        assert!(last_second.iter().any(|s| s.abs() > 1e-3), "{name} holds no trailing silence");
    }
}
