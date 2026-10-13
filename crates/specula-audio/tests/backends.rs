//! Backends end to end: manual pull, the null backend's real-time clock, and (when there is one)
//! the real audio device.

use specula_audio::*;
use std::time::{Duration, Instant};

fn tone(freq: f32, secs: f32, amp: f32, lead_silence: f32) -> Sound {
    let rate = 48_000.0;
    let lead = (lead_silence * rate) as usize;
    let n = (secs * rate) as usize;
    let fade = (0.005 * rate) as usize;
    let mut s = vec![0.0; lead];
    for i in 0..n {
        let env = (i.min(n - 1 - i) as f32 / fade as f32).min(1.0);
        s.push(amp * env * (std::f32::consts::TAU * freq * i as f32 / rate).sin());
    }
    Sound::new("tone", 48_000, 1, s).unwrap()
}

#[test]
fn manual_backend_pulls_exact_frames() {
    let audio = Audio::start(AudioConfig { backend: Backend::Manual, ..Default::default() }).unwrap();
    let id = audio.play(&tone(440.0, 0.1, 0.5, 0.0), PlayParams { pan: -1.0, ..Default::default() }).unwrap();
    let mut out = vec![0.0; 9600];
    audio.pull(&mut out).unwrap();
    assert!(out.iter().step_by(2).any(|s| s.abs() > 0.4));
    assert!(out.iter().skip(1).step_by(2).all(|s| s.abs() < 1e-6));
    audio.pull(&mut out).unwrap();
    assert_eq!(audio.drain_finished(), vec![id]);
    let st = audio.stats();
    assert_eq!(st.frames_mixed, 9600);
    assert_eq!(st.backend, BackendState::Null);
    audio.shutdown().unwrap();
}

#[test]
fn pull_needs_manual_mode() {
    let audio = Audio::start(AudioConfig { backend: Backend::Null, ..Default::default() }).unwrap();
    assert_eq!(audio.pull(&mut [0.0; 4]).unwrap_err().code, ErrorCode::WrongMode);
}

#[test]
fn null_backend_consumes_audio_in_real_time() {
    let audio = Audio::start(AudioConfig { backend: Backend::Null, ..Default::default() }).unwrap();
    let start = Instant::now();
    let id = audio.play(&tone(440.0, 0.2, 0.1, 0.0), PlayParams::default()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(audio.drain_finished().is_empty(), "a 200 ms sound can't be over after 100 ms");
    std::thread::sleep(Duration::from_millis(400));
    let elapsed = start.elapsed().as_secs_f64();
    let st = audio.stats();
    let expected = elapsed * 48_000.0;
    let ratio = st.frames_mixed as f64 / expected;
    println!("null backend: {} frames in {elapsed:.3} s (ratio {ratio:.3}), mix mean {:.1} us", st.frames_mixed, st.mix_us_mean);
    assert!((0.8..1.1).contains(&ratio), "null clock ran at {ratio:.3}× real time");
    assert_eq!(audio.drain_finished(), vec![id]);
    assert_eq!(st.backend, BackendState::Null);
    audio.shutdown().unwrap();
}

#[test]
fn auto_backend_always_starts_and_stops_cleanly() {
    let audio = Audio::start(AudioConfig::default()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let st = audio.stats();
    assert!(matches!(st.backend, BackendState::Device | BackendState::Null | BackendState::Reconnecting), "{st:?}");
    audio.shutdown().unwrap();
}

/// Opens the real default output device and plays 200 ms: 100 ms of silence then a quiet
/// (-30 dBFS) 440 Hz tone. Skips cleanly when there is no device (CI, headless, non-Windows).
#[test]
fn live_device_plays_silence_then_tone() {
    let audio = match Audio::start(AudioConfig { backend: Backend::Device, ..Default::default() }) {
        Ok(a) => a,
        // Only "there is no device" skips; any other failure (COM, format, a lost device) is a bug to see.
        Err(e) if matches!(e.code, ErrorCode::NoDevice | ErrorCode::Unsupported) => {
            println!("SKIP live_device_plays_silence_then_tone: no usable audio device ({e})");
            return;
        }
        Err(e) => panic!("unexpected start error: {e}"),
    };
    let st = audio.stats();
    assert_eq!(st.backend, BackendState::Device);
    let id = audio.play(&tone(440.0, 0.1, 0.0316, 0.1), PlayParams::default()).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    let st = audio.stats();
    println!(
        "device: {} Hz, {} ch, buffer {} frames, latency {:.1} ms, {} mix calls, mix mean {:.1} us max {:.1} us, underruns {}",
        st.output_rate, st.device_channels, st.buffer_frames, st.latency_ms, st.mix_calls, st.mix_us_mean, st.mix_us_max, st.underruns
    );
    assert!(st.frames_mixed as f64 > 0.4 * st.output_rate as f64, "device pulled {} frames in 600 ms", st.frames_mixed);
    assert!(st.latency_ms > 0.0 && st.latency_ms < 200.0);
    assert_eq!(audio.drain_finished(), vec![id], "the 200 ms sound finished on the device clock");
    audio.shutdown().unwrap();
}
