use super::*;

const RATE: u32 = 48_000;

fn mixer(max_voices: usize) -> (MixerHandle, Mixer) {
    Mixer::new(MixerConfig { rate: RATE, max_voices, command_capacity: 64 })
}

fn run(m: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    m.render(&mut out);
    out
}

fn dc(value: f32, frames: usize) -> Sound {
    Sound::new("dc", RATE, 1, vec![value; frames]).unwrap()
}

fn sine(freq: f64, rate: u32, secs: f64, amp: f32) -> Sound {
    let n = (rate as f64 * secs) as usize;
    let s = (0..n).map(|i| amp * dmath::sin_f64(dsp::TAU * freq * i as f64 / rate as f64) as f32).collect();
    Sound::new("sine", rate, 1, s).unwrap()
}

/// Frequency from rising zero crossings (with linear interpolation) over a mono channel.
fn measure_freq(x: &[f32], rate: f64) -> f64 {
    let mut crossings = Vec::new();
    for i in 1..x.len() {
        if x[i - 1] < 0.0 && x[i] >= 0.0 {
            let t = (i - 1) as f64 + (-x[i - 1] / (x[i] - x[i - 1])) as f64;
            crossings.push(t);
        }
    }
    let n = crossings.len() - 1;
    rate * n as f64 / (crossings[n] - crossings[0])
}

fn left(out: &[f32]) -> Vec<f32> {
    out.chunks_exact(2).map(|f| f[0]).collect()
}

#[test]
fn centre_pan_is_minus_3db_each_side() {
    let (mut h, mut m) = mixer(8);
    h.play(&dc(0.5, 4800), PlayParams::default()).unwrap();
    let out = run(&mut m, 256);
    let g = std::f32::consts::FRAC_1_SQRT_2 * 0.5;
    for f in out.chunks_exact(2) {
        assert!((f[0] - g).abs() < 1e-6 && (f[1] - g).abs() < 1e-6, "{f:?}");
    }
}

#[test]
fn gain_pan_and_bus_volumes() {
    let (mut h, mut m) = mixer(8);
    h.play(&dc(0.5, 48_000), PlayParams { pan: -1.0, gain: 0.5, ..Default::default() }).unwrap();
    let out = run(&mut m, 256);
    assert!((out[0] - 0.25).abs() < 1e-6 && out[1].abs() < 1e-6, "{:?}", &out[..2]);
    // Bus at half, master at half (instant): quarter of that.
    h.set_bus_gain(Bus::Sfx, 0.5, 0.0).unwrap();
    h.set_bus_gain(Bus::Master, 0.5, 0.0).unwrap();
    let out = run(&mut m, 256);
    let last = out[out.len() - 2];
    assert!((last - 0.0625).abs() < 1e-5, "{last}");
    // Another bus is unaffected by the sfx volume.
    let (mut h, mut m) = mixer(8);
    h.set_bus_gain(Bus::Sfx, 0.0, 0.0).unwrap();
    h.play(&dc(0.5, 48_000), PlayParams { bus: Bus::Ui, pan: -1.0, ..Default::default() }).unwrap();
    let out = run(&mut m, 256);
    assert!((out[out.len() - 2] - 0.5).abs() < 1e-6);
}

#[test]
fn stereo_sound_keeps_channels_at_centre() {
    let (mut h, mut m) = mixer(8);
    let mut s = Vec::new();
    for _ in 0..1000 {
        s.push(0.3);
        s.push(-0.2);
    }
    h.play(&Sound::new("st", RATE, 2, s).unwrap(), PlayParams::default()).unwrap();
    let out = run(&mut m, 128);
    assert!((out[0] - 0.3).abs() < 1e-6 && (out[1] + 0.2).abs() < 1e-6);
}

#[test]
fn one_shot_finishes_and_is_reported() {
    let (mut h, mut m) = mixer(8);
    let id = h.play(&dc(0.1, 1000), PlayParams::default()).unwrap();
    let out = run(&mut m, 2048);
    assert!(out[2 * 999].abs() > 0.0);
    assert_eq!(out[2 * 1100], 0.0);
    assert_eq!(h.drain_finished(), vec![id]);
    assert_eq!(m.voices.len(), 0);
}

#[test]
fn looping_plays_until_stopped_with_fade() {
    let (mut h, mut m) = mixer(8);
    let id = h.play(&dc(0.2, 300), PlayParams { looping: true, pan: -1.0, ..Default::default() }).unwrap();
    let out = run(&mut m, 4800);
    assert!(left(&out).iter().all(|&s| (s - 0.2).abs() < 1e-5), "loop must be seamless for DC");
    h.stop(id, 0.01).unwrap();
    let out = run(&mut m, 960);
    let l = left(&out);
    // Fades linearly over 480 frames, then silent.
    assert!(l[100] < 0.2 && l[100] > 0.1, "{}", l[100]);
    assert!(l[600..].iter().all(|&s| s == 0.0));
    run(&mut m, 256);
    assert_eq!(h.drain_finished(), vec![id]);
}

#[test]
fn priority_stealing_and_rejection() {
    let (mut h, mut m) = mixer(4);
    let s = dc(0.01, 48_000);
    let low: Vec<VoiceId> = (0..4).map(|_| h.play(&s, PlayParams { priority: 100, ..Default::default() }).unwrap()).collect();
    run(&mut m, 64);
    let rejected = h.play(&s, PlayParams { priority: 50, ..Default::default() }).unwrap();
    run(&mut m, 64);
    assert_eq!(h.drain_finished(), vec![rejected]);
    let st = Stats::read(&h.stats);
    assert_eq!(st.voices_rejected, 1);
    let _high = h.play(&s, PlayParams { priority: 200, ..Default::default() }).unwrap();
    run(&mut m, 512);
    let st = Stats::read(&h.stats);
    assert_eq!(st.voices_stolen, 1);
    assert_eq!(st.active_voices, 4);
    // The oldest of the equally quiet low-priority voices was stolen (after a de-click fade).
    assert_eq!(h.drain_finished(), vec![low[0]]);
}

#[test]
fn spatial_pan_distance_and_cull() {
    let (mut h, mut m) = mixer(8);
    // Listener at origin facing -Z: +X is to the right.
    let s = dc(0.5, 48_000);
    let right = h
        .play(&s, PlayParams { spatial: Some(Spatial { lowpass: false, ..Spatial::at([10.0, 0.0, 0.0]) }), ..Default::default() })
        .unwrap();
    let out = run(&mut m, 256);
    let (l, r) = (out[510], out[511]);
    assert!(r > 0.0 && l.abs() < 1e-6, "hard right: {l} {r}");
    // min 2, rolloff 1: gain 2 / (2 + 8) = 0.2.
    assert!((r - 0.5 * 0.2).abs() < 1e-4, "{r}");
    h.set_position(right, [0.0, 0.0, -2.0]).unwrap();
    let out = run(&mut m, 256);
    let (l, r) = (out[510], out[511]);
    assert!((l - r).abs() < 1e-5 && (l - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4, "ahead at min distance: {l} {r}");
    h.set_position(right, [0.0, 0.0, -500.0]).unwrap();
    let out = run(&mut m, 512);
    assert!(out[512..].iter().all(|&s| s == 0.0), "beyond max distance is silent");
    // Turning the listener around swaps the sides.
    h.set_position(right, [10.0, 0.0, 0.0]).unwrap();
    h.set_listener(Listener { forward: [0.0, 0.0, 1.0], ..Default::default() }).unwrap();
    let out = run(&mut m, 256);
    assert!(out[510] > 0.0 && out[511].abs() < 1e-6);
}

#[test]
fn distance_lowpass_darkens_far_sounds() {
    let tone = sine(8000.0, RATE, 1.0, 0.5);
    let energy = |pos: [f32; 3]| {
        let (mut h, mut m) = mixer(4);
        let sp = Spatial { position: pos, min_distance: 1.0, max_distance: 100.0, rolloff: 0.0, lowpass: true };
        h.play(&tone, PlayParams { spatial: Some(sp), ..Default::default() }).unwrap();
        let out = run(&mut m, 4800);
        out[2000..].iter().map(|s| (s * s) as f64).sum::<f64>()
    };
    let near = energy([0.0, 0.0, -1.0]);
    let far = energy([0.0, 0.0, -70.0]);
    // rolloff 0: no distance attenuation, so the drop is the low-pass alone.
    assert!(far < near * 0.2, "near {near} far {far}");
}

#[test]
fn resampling_and_pitch_keep_frequency() {
    let (mut h, mut m) = mixer(4);
    h.play(&sine(1000.0, 44_100, 1.0, 0.5), PlayParams { pan: -1.0, ..Default::default() }).unwrap();
    let out = left(&run(&mut m, 24_000));
    let f = measure_freq(&out[100..], RATE as f64);
    assert!((f - 1000.0).abs() < 0.05, "{f}");
    let peak = out[1000..].iter().fold(0.0f32, |a, s| a.max(s.abs()));
    assert!((peak - 0.5).abs() < 0.005, "{peak}");

    let (mut h, mut m) = mixer(4);
    let id = h.play(&sine(440.0, RATE, 2.0, 0.5), PlayParams { pan: -1.0, pitch: 1.5, ..Default::default() }).unwrap();
    let out = left(&run(&mut m, 9600));
    assert!((measure_freq(&out[100..], RATE as f64) - 660.0).abs() < 0.05);
    h.set_pitch(id, 0.5).unwrap();
    let out = left(&run(&mut m, 9600));
    assert!((measure_freq(&out[100..], RATE as f64) - 220.0).abs() < 0.05);
}

#[test]
fn ducking_lowers_music_under_sfx() {
    let (mut h, mut m) = mixer(8);
    h.play(&dc(0.2, 480_000), PlayParams { bus: Bus::Music, pan: -1.0, looping: true, ..Default::default() }).unwrap();
    h.set_duck(0, Some(DuckRule { source: Bus::Sfx, target: Bus::Music, depth_db: 12.0, threshold_db: -30.0, attack_s: 0.01, release_s: 0.2 }))
        .unwrap();
    let before = left(&run(&mut m, 4800));
    assert!((before[4799] - 0.2).abs() < 1e-4);
    // An sfx on the right only, so the left channel shows the music alone.
    h.play(&dc(0.5, 24_000), PlayParams { pan: 1.0, ..Default::default() }).unwrap();
    let during = left(&run(&mut m, 9600));
    let ducked = 0.2 * dsp::db_to_gain(-12.0) as f32;
    assert!((during[9599] - ducked).abs() < 0.005, "{} vs {ducked}", during[9599]);
    let after = left(&run(&mut m, 96_000));
    assert!((after[95_999] - 0.2).abs() < 0.005, "released: {}", after[95_999]);
}

#[test]
fn limiter_keeps_output_in_range() {
    let (mut h, mut m) = mixer(16);
    for _ in 0..8 {
        h.play(&sine(220.0, RATE, 1.0, 0.9), PlayParams::default()).unwrap();
    }
    let out = run(&mut m, 24_000);
    assert!(out.iter().all(|s| s.abs() <= 1.0));
    let st = Stats::read(&h.stats);
    assert!(st.limiter_gain < 0.5, "{}", st.limiter_gain);
    // Most of the signal is under the ceiling, not hard-clipped.
    let clipped = out.iter().filter(|s| s.abs() >= 0.999).count();
    assert!(clipped < out.len() / 1000, "{clipped} clipped samples");
}

#[test]
fn fade_in_ramps_from_silence() {
    let (mut h, mut m) = mixer(4);
    h.play(&dc(0.5, 48_000), PlayParams { pan: -1.0, fade_in_s: 0.1, ..Default::default() }).unwrap();
    let l = left(&run(&mut m, 9600));
    assert!(l[0] < 0.01);
    assert!((l[2400] - 0.25).abs() < 0.01, "{}", l[2400]);
    assert!((l[9000] - 0.5).abs() < 1e-5);
}

#[test]
fn set_gain_and_stop_all() {
    let (mut h, mut m) = mixer(4);
    let a = h.play(&dc(0.5, 48_000), PlayParams { pan: -1.0, ..Default::default() }).unwrap();
    let _b = h.play(&dc(0.5, 48_000), PlayParams { bus: Bus::Ui, pan: 1.0, ..Default::default() }).unwrap();
    h.set_gain(a, 0.2, 0.0).unwrap();
    let out = run(&mut m, 256);
    assert!((out[510] - 0.1).abs() < 1e-5 && (out[511] - 0.5).abs() < 1e-5);
    h.stop_all(Some(Bus::Ui), 0.0).unwrap();
    let out = run(&mut m, 1024);
    assert!(out[2047].abs() < 1e-6 && out[2046] > 0.09);
    h.stop_all(None, 0.0).unwrap();
    run(&mut m, 1024);
    assert_eq!(m.voices.len(), 0);
}

#[test]
fn queue_full_is_a_structured_error() {
    let (mut h, _m) = Mixer::new(MixerConfig { rate: RATE, max_voices: 4, command_capacity: 16 });
    let s = dc(0.1, 10);
    let mut err = None;
    for _ in 0..100 {
        if let Err(e) = h.play(&s, PlayParams::default()) {
            err = Some(e);
            break;
        }
    }
    assert_eq!(err.unwrap().code, ErrorCode::QueueFull);
    assert!(Stats::read(&h.stats).commands_dropped >= 1);
}

#[test]
fn non_finite_parameters_are_sanitised() {
    let (mut h, mut m) = mixer(4);
    h.play(&dc(0.5, 4800), PlayParams { gain: f32::NAN, pitch: f32::INFINITY, pan: f32::NAN, ..Default::default() }).unwrap();
    let out = run(&mut m, 512);
    assert!(out.iter().all(|s| s.is_finite()));
}
