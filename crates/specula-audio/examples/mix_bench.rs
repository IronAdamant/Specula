//! F3 budget check: the cost of mixing one 10 ms block of 64 voices at 48 kHz.
//!
//! - **worst**: every voice 3D-positioned with the distance low-pass on, pitched and from
//!   44.1 kHz sources (so every voice runs the sinc resampler), a quarter of them stereo,
//!   looping, on all buses with ducking active, the listener moving every block.
//! - **typical**: the same, but sounds rendered at the device rate and played at pitch 1 (how
//!   synthesised sets are used), so no resampling.
//!
//! ```text
//! cargo run --release -p specula-audio --example mix_bench
//! ```

use specula_audio::mixer::{Mixer, MixerConfig};
use specula_audio::{Bus, DuckRule, Listener, PlayParams, Sound, Spatial};
use std::time::Instant;

fn run(name: &str, src_rate: u32, pitched: bool) -> f64 {
    let rate = 48_000u32;
    let (mut h, mut m) = Mixer::new(MixerConfig { rate, max_voices: 64, command_capacity: 1024 });
    let n = src_rate as usize;
    let mono: Vec<f32> = (0..n).map(|i| ((i as f32 * 0.031).sin() * 0.3) + ((i * 7919 % 101) as f32 / 101.0 - 0.5) * 0.05).collect();
    let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s, -s]).collect();
    let mono = Sound::new("m", src_rate, 1, mono).unwrap();
    let stereo = Sound::new("s", src_rate, 2, stereo).unwrap();
    let buses = [Bus::Sfx, Bus::Ambience, Bus::Music, Bus::Ui];
    for i in 0..64 {
        let a = i as f32 * 0.37;
        let pos = [a.cos() * (3.0 + i as f32), 1.0, a.sin() * (3.0 + i as f32)];
        h.play(
            if i % 4 == 0 { &stereo } else { &mono },
            PlayParams {
                bus: buses[i % 4],
                gain: 0.05,
                pitch: if pitched { 0.9 + (i as f32 % 7.0) * 0.03 } else { 1.0 },
                looping: true,
                spatial: Some(Spatial { position: pos, min_distance: 2.0, max_distance: 120.0, rolloff: 1.0, lowpass: true }),
                ..Default::default()
            },
        )
        .unwrap();
    }
    h.set_duck(0, Some(DuckRule { source: Bus::Sfx, target: Bus::Music, depth_db: 9.0, threshold_db: -40.0, attack_s: 0.02, release_s: 0.3 })).unwrap();
    let block = (rate / 100) as usize; // 10 ms
    let mut out = vec![0.0f32; block * 2];
    for _ in 0..50 {
        m.render(&mut out);
    }
    // 2000 blocks = 20 s of audio, the listener moving every block.
    let mut times = Vec::with_capacity(2000);
    for b in 0..2000 {
        let t = b as f32 * 0.01;
        h.set_listener(Listener { position: [t.sin() * 5.0, 1.7, t.cos() * 5.0], forward: [t.cos(), 0.0, -t.sin()], up: [0.0, 1.0, 0.0] }).unwrap();
        let start = Instant::now();
        m.render(&mut out);
        times.push(start.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let (min, p50, p99) = (times[0], times[times.len() / 2], times[times.len() * 99 / 100]);
    println!("{name:<8} min {min:6.1} us  p50 {p50:6.1} us  mean {mean:6.1} us  p99 {p99:6.1} us  ({:.1}% of one core at p50)", p50 / 100.0);
    p99
}

fn main() {
    println!("64 voices, 48 kHz output, 10 ms (480-frame) blocks, 3D + distance low-pass + ducking:");
    let worst = run("worst", 44_100, true);
    let typical = run("typical", 48_000, false);
    let ok = worst < 1000.0 && typical < 1000.0;
    println!("budget: well under 1000 us per 10 ms block -> {}", if ok { "PASS" } else { "FAIL" });
}
