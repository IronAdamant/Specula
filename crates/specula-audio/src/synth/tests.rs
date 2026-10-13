use super::*;
use crate::analysis;
use crate::error::ErrorCode;

fn r(text: &str) -> Rendered {
    render_text(text, &RenderOptions::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn err(text: &str) -> crate::AudioError {
    Recipe::parse(text).unwrap_err()
}

fn one(layer: &str, duration: f64) -> String {
    format!(r#"{{"sound": 1, "name": "t", "duration": {duration}, "layers": [{layer}]}}"#)
}

fn measure_freq(x: &[f32], rate: f64) -> f64 {
    let mut c = Vec::new();
    for i in 1..x.len() {
        if x[i - 1] < 0.0 && x[i] >= 0.0 {
            c.push((i - 1) as f64 + (-x[i - 1] / (x[i] - x[i - 1])) as f64);
        }
    }
    rate * (c.len() - 1) as f64 / (c[c.len() - 1] - c[0])
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

#[test]
fn sine_at_1khz_measures_1khz_at_full_scale() {
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": 1000}}"#, 1.0));
    assert_eq!(out.samples.len(), 48_000);
    let f = measure_freq(&out.samples, 48_000.0);
    assert!((f - 1000.0).abs() < 0.01, "{f}");
    let peak = out.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    // Default peak ceiling is -1 dBFS.
    assert!((peak - 0.891).abs() < 0.002, "{peak}");
    assert!((out.ceiling_db + 1.0).abs() < 0.01);
}

#[test]
fn render_is_deterministic_and_pinned() {
    let text = include_str!("../../tests/data/determinism.json");
    let a = r(text);
    let b = r(text);
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>(), b.samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>());
    // Pinned: changes when any synthesis maths changes. Update deliberately (and say why).
    assert_eq!(a.hash, PINNED_DETERMINISM_HASH, "determinism hash changed: {:#018x}", a.hash);
    let v1 = render_text(text, &RenderOptions { variation: 1, ..Default::default() }).unwrap();
    let v1b = render_text(text, &RenderOptions { variation: 1, ..Default::default() }).unwrap();
    assert_eq!(v1.hash, v1b.hash);
    assert_ne!(v1.hash, a.hash);
    assert_eq!(v1.hash, PINNED_VARIATION1_HASH, "variation hash changed: {:#018x}", v1.hash);
    let r44 = render_text(text, &RenderOptions { rate: 44_100, variation: 0 }).unwrap();
    assert_eq!(r44.hash, PINNED_44K_HASH, "44.1 kHz hash changed: {:#018x}", r44.hash);
}

const PINNED_DETERMINISM_HASH: u64 = 0x1903_a86b_e780_5ab2;
const PINNED_VARIATION1_HASH: u64 = 0xc16f_8b80_20bc_28fd;
const PINNED_44K_HASH: u64 = 0x8fd0_ee94_6775_0423;

#[test]
fn adsr_envelope_shape() {
    // DC-like source: a 0.01 Hz sine starting at phase 0.25 stays ≈ 1, so the output is the envelope.
    let text = one(
        r#"{"source": {"wave": "sine", "freq": 1, "phase": 0.25}, "length": 0.5,
            "env": {"attack": 0.1, "hold": 0.05, "decay": 0.1, "sustain": 0.5, "release": 0.2, "curve": "linear"}}"#,
        1.0,
    );
    let mut recipe = Recipe::parse(&text).unwrap();
    recipe.peak_ceiling_db = 0.0;
    let out = render(&recipe, &RenderOptions::default()).unwrap();
    let at = |t: f64| out.samples[(t * 48_000.0) as usize] as f64;
    let env_at = |t: f64| at(t) / (std::f64::consts::TAU * t + std::f64::consts::FRAC_PI_2).sin();
    assert!((env_at(0.05) - 0.5).abs() < 0.01, "attack mid {}", env_at(0.05));
    assert!((env_at(0.12) - 1.0).abs() < 0.01, "hold");
    assert!((env_at(0.2) - 0.75).abs() < 0.01, "decay mid {}", env_at(0.2));
    assert!((env_at(0.4) - 0.5).abs() < 0.01, "sustain");
    assert!((env_at(0.6) - 0.25).abs() < 0.01, "release mid {}", env_at(0.6));
    assert!(out.samples[(0.71 * 48_000.0) as usize..].iter().all(|&s| s == 0.0), "silent after release");
}

#[test]
fn exp_envelope_decays_to_zero() {
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": 200}, "env": {"attack": 0.001, "decay": 0.3, "sustain": 0}}"#, 0.5));
    let early = rms(&out.samples[480..4800]);
    let mid = rms(&out.samples[7200..9600]);
    let late = rms(&out.samples[13_000..14_000]);
    assert!(early > mid * 3.0 && mid > late * 3.0, "{early} {mid} {late}");
    assert!(out.samples[15_000..].iter().all(|&s| s == 0.0));
}

#[test]
fn filters_shape_the_spectrum() {
    let centroid = |filters: &str| {
        let out = r(&one(&format!(r#"{{"source": {{"wave": "noise"}}, "filters": [{filters}]}}"#), 1.0));
        out.metrics().centroid_hz
    };
    let white = centroid("");
    let lp = centroid(r#"{"type": "lowpass", "cutoff": 500}"#);
    let hp = centroid(r#"{"type": "highpass", "cutoff": 8000}"#);
    let bp = centroid(r#"{"type": "bandpass", "cutoff": 2000, "q": 8}"#);
    assert!(lp < 1500.0, "lp {lp}");
    assert!(hp > white, "hp {hp} white {white}");
    assert!((bp - 2000.0).abs() < 600.0, "bp {bp}");
    let lp1 = centroid(r#"{"type": "lp1", "cutoff": 500}"#);
    assert!(lp1 < white * 0.6 && lp1 > lp, "lp1 {lp1}");
}

#[test]
fn noise_colours_tilt_darker() {
    let c = |color: &str| r(&one(&format!(r#"{{"source": {{"wave": "noise", "color": "{color}"}}}}"#), 1.0)).metrics().centroid_hz;
    let (w, p, b) = (c("white"), c("pink"), c("brown"));
    assert!(w > p * 1.5 && p > b * 1.5, "white {w} pink {p} brown {b}");
    // Sample-and-hold noise at 1 kHz is darker than white.
    let sh = r(&one(r#"{"source": {"wave": "noise", "freq": 1000}}"#, 1.0)).metrics().centroid_hz;
    assert!(sh < w * 0.6, "{sh}");
}

#[test]
fn pitch_slides_vibrato_and_arpeggio() {
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": {"points": [[0, 200], [1, 800]], "curve": "exp"}}}"#, 1.0));
    // Exponential slide: 400 Hz at the midpoint.
    let mid = measure_freq(&out.samples[22_800..25_200], 48_000.0);
    assert!((mid - 400.0).abs() < 8.0, "{mid}");
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": 440}, "arp": {"steps": [0, 12], "rate": 4}}"#, 0.5));
    let a = measure_freq(&out.samples[1000..11_000], 48_000.0);
    let b = measure_freq(&out.samples[13_000..23_000], 48_000.0);
    assert!((a - 440.0).abs() < 1.0 && (b - 880.0).abs() < 2.0, "{a} {b}");
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": 440}, "vibrato": {"rate": 5, "depth": 1}}"#, 1.0));
    let avg = measure_freq(&out.samples, 48_000.0);
    assert!((avg - 440.0).abs() < 5.0, "{avg}");
}

#[test]
fn saw_and_square_are_band_limited_with_harmonics() {
    let spectrum = |wave: &str| {
        let out = r(&one(&format!(r#"{{"source": {{"wave": "{wave}", "freq": 1500}}}}"#), 0.5));
        analysis::stft(&out.samples, 1, 48_000, 4096, 4096)
    };
    for wave in ["saw", "square"] {
        let st = spectrum(wave);
        let bins = st.size / 2 + 1;
        let row = &st.db[2 * bins..3 * bins];
        let at = |hz: f64| row[(hz * st.size as f64 / 48_000.0).round() as usize];
        let h1 = at(1500.0);
        let h3 = at(4500.0);
        assert!(h1 > -12.0 && (h1 - h3 - 9.5).abs() < 2.0, "{wave}: 1st {h1} 3rd {h3}");
        if wave == "saw" {
            assert!((h1 - at(3000.0) - 6.0).abs() < 1.5, "saw 2nd harmonic");
        } else {
            assert!(at(3000.0) < h1 - 30.0, "square has no even harmonics at 50% duty");
        }
        // Aliases fall between harmonics; PolyBLEP keeps them well down.
        let alias = at(48_000.0 - 1500.0 * 17.0);
        assert!(alias < h1 - 40.0, "{wave}: alias {alias}");
    }
}

#[test]
fn partials_and_fm_make_inharmonic_bells() {
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": 200, "partials": [[1, 1, 1.0], [2.76, 0.5, 0.3], [5.4, 0.25, 0.1]]}}"#, 2.0));
    let st = analysis::stft(&out.samples, 1, 48_000, 8192, 4800);
    let bins = st.size / 2 + 1;
    let at = |fr: usize, hz: f64| st.db[fr * bins + (hz * st.size as f64 / 48_000.0).round() as usize];
    // Upper partials decay faster than the fundamental.
    let d1 = at(2, 200.0) - at(8, 200.0);
    let d2 = at(2, 552.0) - at(8, 552.0);
    assert!(d2 > d1 + 10.0, "{d1} {d2}");
    let fm = r(&one(r#"{"source": {"wave": "sine", "freq": 300, "fm": {"ratio": 1.4, "index": 3}}}"#, 0.5)).metrics().centroid_hz;
    let plain = r(&one(r#"{"source": {"wave": "sine", "freq": 300}}"#, 0.5)).metrics().centroid_hz;
    assert!(fm > plain * 1.5, "{fm} {plain}");
}

#[test]
fn repeats_make_separate_onsets() {
    let out = r(&one(
        r#"{"source": {"wave": "noise"}, "length": 0.005, "env": {"attack": 0.0005, "decay": 0.004, "sustain": 0, "release": 0.001},
            "repeat": {"count": 5, "interval": 0.1}}"#,
        0.6,
    ));
    let mut onsets = 0;
    let mut quiet = true;
    for w in out.samples.chunks(240) {
        let loud = rms(w) > 0.01;
        if loud && quiet {
            onsets += 1;
        }
        quiet = !loud;
    }
    assert_eq!(onsets, 5);
}

#[test]
fn delay_echoes_and_bitcrush_quantises() {
    let out = r(&one(
        r#"{"source": {"wave": "sine", "freq": 800}, "length": 0.02, "env": {"attack": 0.001, "decay": 0.01, "sustain": 0, "release": 0.005},
            "fx": [{"type": "delay", "time": 0.25, "feedback": 0.5, "mix": 0.5, "damp": 0}]}"#,
        1.0,
    ));
    let e = |a: f64, b: f64| rms(&out.samples[(a * 48_000.0) as usize..(b * 48_000.0) as usize]);
    let (dry, echo1, echo2) = (e(0.0, 0.03), e(0.25, 0.28), e(0.5, 0.53));
    // mix 0.5 → first echo at half the dry level; feedback 0.5 halves each repeat.
    assert!((echo1 / dry - 0.5).abs() < 0.02 && (echo2 / echo1 - 0.5).abs() < 0.02 && e(0.1, 0.2) < 1e-6, "{dry} {echo1} {echo2}");
    let out = r(&one(r#"{"source": {"wave": "sine", "freq": 100}, "fx": [{"type": "bitcrush", "bits": 2}]}"#, 0.2));
    let mut levels: Vec<u32> = out.samples.iter().map(|s| s.to_bits()).collect();
    levels.sort_unstable();
    levels.dedup();
    assert!(levels.len() <= 5, "{} levels", levels.len());
}

#[test]
fn loop_render_is_seamless() {
    let out = r(&one(r#"{"source": {"wave": "noise", "color": "brown"}}"#, 1.0).replace("\"layers\"", "\"loop\": {\"crossfade\": 0.2}, \"layers\""));
    assert!(out.looping);
    assert_eq!(out.samples.len(), 48_000);
    let s = &out.samples;
    let jump = (s[0] - s[s.len() - 1]).abs();
    let typical = s.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / s.len() as f32;
    assert!(jump < typical * 6.0, "wrap jump {jump} vs typical step {typical}");
}

#[test]
fn loudness_target_is_met() {
    let text = one(r#"{"source": {"wave": "noise", "color": "pink"}, "gain_db": -30}"#, 2.0).replace("\"layers\"", "\"loudness\": -20, \"layers\"");
    let out = r(&text);
    let l = analysis::loudness_lufs(&out.samples, 1, 48_000);
    assert!((l + 20.0).abs() < 0.05, "{l}");
    assert!(out.normalize_db > 0.0 && out.ceiling_db == 0.0);
}

#[test]
fn stereo_pan_places_layers() {
    let out = r(r#"{"sound": 1, "name": "st", "duration": 0.2, "channels": 2,
        "layers": [{"pan": -1, "source": {"wave": "sine", "freq": 300}}]}"#);
    let (l, rr): (Vec<f32>, Vec<f32>) = out.samples.chunks_exact(2).map(|f| (f[0], f[1])).unzip();
    assert!(rms(&l) > 0.3 && rms(&rr) < 1e-6);
}

#[test]
fn rand_values_are_keyed_by_path() {
    let a = Recipe::parse(&one(r#"{"source": {"wave": "sine", "freq": {"rand": [100, 200]}}}"#, 0.1)).unwrap();
    let b = Recipe::parse(
        r#"{"sound": 1, "name": "t", "duration": 0.1, "layers": [
            {"source": {"wave": "sine", "freq": {"rand": [100, 200]}}},
            {"gain_db": {"rand": [-6, 0]}, "source": {"wave": "noise"}}]}"#,
    )
    .unwrap();
    let key = |r: &Recipe| match &r.layers[0].source.freq {
        Some(recipe::Param::Const(n)) => n.key,
        _ => unreachable!(),
    };
    assert_eq!(key(&a), key(&b));
    // Two renders of `a`'s layer agree with `b`'s first layer in frequency (same draw).
    let fa = measure_freq(&render(&a, &RenderOptions::default()).unwrap().samples, 48_000.0);
    let mut b1 = b.clone();
    b1.layers.truncate(1);
    let fb = measure_freq(&render(&b1, &RenderOptions::default()).unwrap().samples, 48_000.0);
    assert!((fa - fb).abs() < 1e-6 && (100.0..=200.0).contains(&fa), "{fa} {fb}");
}

#[test]
fn validation_errors_have_paths_and_codes() {
    let cases: Vec<(String, ErrorCode, &str)> = vec![
        (r#"{"sound": 1, "name": "t", "duration": 1, "layers": [], "bogus": 1}"#.into(), ErrorCode::UnknownField, "bogus"),
        (r#"{"sound": 2, "name": "t", "duration": 1, "layers": []}"#.into(), ErrorCode::OutOfRange, "sound"),
        (r#"{"sound": 1, "name": "T!", "duration": 1, "layers": []}"#.into(), ErrorCode::OutOfRange, "name"),
        (r#"{"sound": 1, "name": "t", "duration": 99, "layers": []}"#.into(), ErrorCode::OutOfRange, "duration"),
        (r#"{"sound": 1, "name": "t", "duration": 1, "layers": []}"#.into(), ErrorCode::Limit, "layers"),
        (one(r#"{"source": {"wave": "sine"}}"#, 1.0), ErrorCode::MissingField, "layers[0].source.freq"),
        (one(r#"{"source": {"wave": "organ", "freq": 1}}"#, 1.0), ErrorCode::UnknownName, "layers[0].source.wave"),
        (one(r#"{"source": {"wave": "sine", "freq": 99999}}"#, 1.0), ErrorCode::OutOfRange, "layers[0].source.freq"),
        (one(r#"{"source": {"wave": "sine", "freq": {"rand": [300, 200]}}}"#, 1.0), ErrorCode::OutOfRange, "layers[0].source.freq.rand"),
        (one(r#"{"source": {"wave": "saw", "freq": 100, "duty": 0.3}}"#, 1.0), ErrorCode::UnknownField, "layers[0].source.duty"),
        (one(r#"{"source": {"wave": "sine", "freq": 100}, "filters": [{"type": "lowpass", "cutoff": 5, "q": 1}]}"#, 1.0), ErrorCode::OutOfRange, "layers[0].filters[0].cutoff"),
        (one(r#"{"source": {"wave": "sine", "freq": 100}, "fx": [{"type": "chorus"}]}"#, 1.0), ErrorCode::UnknownName, "layers[0].fx[0].type"),
        (one(r#"{"source": {"wave": "sine", "freq": {"points": [[0.5, 100], [0.1, 200]]}}}"#, 1.0), ErrorCode::OutOfRange, "layers[0].source.freq.points[1][0]"),
        (one(r#"{"source": {"wave": "sine", "freq": {"points": [[0, 0], [1, 200]], "curve": "exp"}}}"#, 1.0), ErrorCode::OutOfRange, "layers[0].source.freq.points[0][1]"),
        (one(r#"{"source": {"wave": "sine", "freq": 100}, "gain_db": 1, "gain_db": 2}"#, 1.0), ErrorCode::Duplicate, "layers[0].gain_db"),
        (one(r#"{"source": {"wave": "sine", "freq": 100}, "repeat": {"count": 9999}}"#, 1.0), ErrorCode::OutOfRange, "layers[0].repeat.count"),
        (one(r#"{"source": {"wave": "sine", "freq": "loud"}}"#, 1.0), ErrorCode::WrongType, "layers[0].source.freq"),
    ];
    for (text, code, path) in cases {
        let e = err(&text);
        assert_eq!((e.code, e.path.as_str()), (code, path), "{text}\n→ {e}");
    }
    let e = err("{\"sound\": 1,");
    assert_eq!(e.code, ErrorCode::Json);
    assert!(e.offset.is_some());
    // Out-of-range errors carry the numbers.
    let e = err(&one(r#"{"source": {"wave": "sine", "freq": 99999}}"#, 1.0));
    assert_eq!((e.measured, e.limit), (Some(99_999.0), Some(24_000.0)));
}

#[test]
fn expensive_recipes_are_refused() {
    let layers = (0..32).map(|_| r#"{"source": {"wave": "sine", "freq": 100, "partials": [[1,1],[2,1],[3,1],[4,1],[5,1],[6,1],[7,1],[8,1]], "unison": {"voices": 8}}, "repeat": {"count": 64}}"#).collect::<Vec<_>>().join(",");
    let e = err(&format!(r#"{{"sound": 1, "name": "t", "duration": 60, "layers": [{layers}]}}"#));
    assert_eq!(e.code, ErrorCode::Limit);
    assert!(e.measured.unwrap() > e.limit.unwrap());
}

/// Review probes `reverb` / `master` / `partials`: recipes whose render would take seconds are
/// refused at parse time; the limit doesn't grow with the render rate.
#[test]
fn review_cost_probes_are_refused() {
    let filt = r#"{"type":"lowpass","cutoff":2000}"#;
    let reverb = r#"{"type":"reverb"}"#;
    let layer = format!(r#"{{"source":{{"wave":"noise"}},"filters":[{f},{f},{f},{f}],"fx":[{r},{r},{r},{r},{r},{r},{r},{r}]}}"#, f = filt, r = reverb);
    let nine = format!(r#"{{"sound":1,"name":"bomb","duration":60,"layers":[{}]}}"#, [layer.as_str(); 9].join(","));
    let master = r#"{"sound":1,"name":"bomb","duration":60,"channels":2,"layers":[{"source":{"wave":"noise"}}],
        "fx":[{"type":"reverb"},{"type":"reverb"},{"type":"reverb"},{"type":"reverb"},{"type":"reverb"},{"type":"reverb"},{"type":"reverb"},{"type":"reverb"}]}"#
        .to_string();
    let partials: Vec<String> = (0..32).map(|i| format!("[{},1]", 1.0 + i as f64 * 0.01)).collect();
    let partials = format!(r#"{{"sound":1,"name":"bomb","duration":30,"layers":[{{"source":{{"wave":"sine","freq":100,"unison":{{"voices":8}},"partials":[{}]}}}}]}}"#, partials.join(","));
    for text in [nine, master, partials] {
        let e = err(&text);
        assert_eq!(e.code, ErrorCode::Limit, "{e}");
        assert!(e.measured.unwrap() > e.limit.unwrap());
    }
    // One reverb-stack layer for 10 s parses at 48 kHz but is refused at 192 kHz (fixed limit).
    let one = Recipe::parse(&format!(r#"{{"sound":1,"name":"ok","duration":10,"layers":[{layer}]}}"#)).unwrap();
    assert!(one.work_estimate(48_000) <= recipe::MAX_WORK);
    assert_eq!(render(&one, &RenderOptions { rate: 192_000, variation: 0 }).unwrap_err().code, ErrorCode::Limit);
}

/// Instances with `sustain: 0` stop after their decay (they are exactly silent from there), so
/// long repeats of short hits don't cost the whole sound length each.
#[test]
fn zero_sustain_instances_end_after_their_decay() {
    let text = one(r#"{"source": {"wave": "sine", "freq": 500}, "env": {"attack": 0.001, "decay": 0.05, "sustain": 0}}"#, 2.0);
    let r = Recipe::parse(&text).unwrap();
    // Charged for ~0.06 s, not 2 s.
    assert!(r.work_estimate(48_000) < 0.1 * 48_000.0 * 100.0, "{}", r.work_estimate(48_000));
    let out = render(&r, &RenderOptions::default()).unwrap();
    assert!(out.samples[(0.06 * 48_000.0) as usize..].iter().all(|&s| s == 0.0));
}

/// Named layers keep their random draws when other layers are inserted before them; duplicate
/// names are refused.
#[test]
fn named_layers_keep_their_draws_when_layers_are_inserted() {
    let a = r#"{"name": "a", "source": {"wave": "sine", "freq": {"rand": [100, 1000]}}, "gain_db": {"rand": [-12, 0]}}"#;
    let filler = r#"{"source": {"wave": "noise"}, "gain_db": {"rand": [-6, 0]}}"#;
    let solo = Recipe::parse(&format!(r#"{{"sound":1,"name":"t","duration":0.1,"layers":[{a}]}}"#)).unwrap();
    let shifted = Recipe::parse(&format!(r#"{{"sound":1,"name":"t","duration":0.1,"layers":[{filler},{filler},{a}]}}"#)).unwrap();
    let cx_keys = |l: &recipe::Layer| match &l.source.freq {
        Some(recipe::Param::Const(n)) => (n.key, l.gain_db.key, l.key),
        _ => unreachable!(),
    };
    assert_eq!(cx_keys(&solo.layers[0]), cx_keys(&shifted.layers[2]));
    // Unnamed layers are keyed by index, so the two fillers differ.
    assert_ne!(shifted.layers[0].gain_db.key, shifted.layers[1].gain_db.key);
    // And the rendered layer is identical: same frequency and level.
    let mut only_a = shifted.clone();
    only_a.layers.drain(0..2);
    assert_eq!(render(&solo, &RenderOptions::default()).unwrap().hash, render(&only_a, &RenderOptions::default()).unwrap().hash);
    let e = err(&format!(r#"{{"sound":1,"name":"t","duration":0.1,"layers":[{a},{a}]}}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::Duplicate, "layers[1].name"));
}
