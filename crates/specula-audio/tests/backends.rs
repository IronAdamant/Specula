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

/// Polls `done` (a poll's interval, 2 ms) until it holds; `HANG_BOUND` only names a hang.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < specula_core::HANG_BOUND, "still waiting for {what} after {:?}: hung?", specula_core::HANG_BOUND);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The null backend mixes on its own clock, in real time: a sound ends when its frames have
/// been mixed, and the clock never runs ahead of the wall.
///
/// By what the backend's own clock did, never by how long this thread slept (before
/// 2026-11-24: a sleep of 100 ms that a loaded machine overran past 200 ms found the 200 ms
/// sound over; and 0.8..1.1 times real time over a 500 ms sleep, where a starved audio thread
/// falls behind on a busy machine). What is held:
/// - a sound is not reported finished before its frames are mixed (to within the block it
///   ended in): read in that order, what had finished, then how much had been mixed;
/// - it is reported finished once they are (waited for, by the frames);
/// - the clock never mixes ahead of real time: frames mixed are at most the wall time since
///   before the backend started, at the rate, and one block (the clock renders a block, then
///   waits for its deadline). A clock that did not wait fails this at once.
///
/// What it no longer holds: that the clock keeps up with the wall (the old lower bound, 0.8
/// times real time over half a second). A clock thread the machine does not run falls behind
/// whatever the code does, so no bound on that is a guarantee; a clock that never advanced
/// still fails here (it never reaches 400 ms of frames, and the wait names the hang). No other
/// test holds the lower bound, nor the clock's catch-up after it fell ten blocks behind (it
/// counts an underrun and starts again from now: `backend::Clock::tick`).
#[test]
fn null_backend_consumes_audio_in_real_time() {
    let start = Instant::now();
    let audio = Audio::start(AudioConfig { backend: Backend::Null, ..Default::default() }).unwrap();
    let st = audio.stats();
    let (rate, block) = (st.output_rate as u64, st.buffer_frames as u64);
    assert!(rate > 0 && block > 0 && block < rate, "{st:?}");
    let length = rate / 5; // 200 ms
    let id = audio.play(&tone(440.0, 0.2, 0.1, 0.0), PlayParams::default()).unwrap();
    let mut finished = Vec::new();
    until("the 200 ms sound to finish on the null clock", || {
        finished.extend(audio.drain_finished());
        let mixed = audio.stats().frames_mixed;
        assert!(finished.is_empty() || mixed + block >= length, "a {length}-frame sound was over when {mixed} frames had been mixed");
        !finished.is_empty()
    });
    assert_eq!(finished, vec![id]);
    until("400 ms of frames mixed", || audio.stats().frames_mixed >= 2 * length);
    let st = audio.stats();
    let elapsed = start.elapsed().as_secs_f64();
    let ahead = st.frames_mixed as f64 - elapsed * rate as f64;
    assert!(ahead <= block as f64, "the null clock mixed {} frames in {elapsed:.3} s: {ahead:.0} frames ahead of real time", st.frames_mixed);
    assert_eq!(st.backend, BackendState::Null);
    audio.shutdown().unwrap();
}

#[test]
fn auto_backend_always_starts_and_stops_cleanly() {
    let audio = Audio::start(AudioConfig::default()).unwrap();
    // Until it has left Starting (by its state, not after a fixed 50 ms a loaded machine may not
    // have given it): then it is on a device, on the null clock, or reconnecting.
    until("the backend to leave Starting", || audio.stats().backend != BackendState::Starting);
    let st = audio.stats();
    assert!(matches!(st.backend, BackendState::Device | BackendState::Null | BackendState::Reconnecting), "{st:?}");
    audio.shutdown().unwrap();
}

/// Opens the real default output device and plays 200 ms: 100 ms of silence then a quiet
/// (-30 dBFS) 440 Hz tone. Skips cleanly when there is no device (CI, headless, non-Windows).
/// It waits for the sound to finish on the device's clock (not 600 ms by the wall, which a
/// loaded machine can overrun or a starved device thread not fill).
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
    let mut finished = Vec::new();
    until("the 200 ms sound to finish on the device clock", || {
        finished.extend(audio.drain_finished());
        !finished.is_empty()
    });
    let st = audio.stats();
    println!(
        "device: {} Hz, {} ch, buffer {} frames, latency {:.1} ms, {} mix calls, mix mean {:.1} us max {:.1} us, underruns {}",
        st.output_rate, st.device_channels, st.buffer_frames, st.latency_ms, st.mix_calls, st.mix_us_mean, st.mix_us_max, st.underruns
    );
    assert_eq!(finished, vec![id], "the 200 ms sound finished on the device clock");
    // Its frames were pulled by the device before it was over (to within a buffer).
    assert!(st.frames_mixed + st.buffer_frames as u64 >= st.output_rate as u64 / 5, "the device pulled {} frames, and a 200 ms sound was over", st.frames_mixed);
    assert!(st.latency_ms > 0.0 && st.latency_ms < 200.0);
    audio.shutdown().unwrap();
}
