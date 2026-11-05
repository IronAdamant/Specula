//! Hearing for agents: live loudness, peaks, overs, silence and a coarse spectrum for every bus
//! and the master, measured on the worker thread from the mixer's tap ([`TapBlock`]).
//!
//! - **Loudness** (ITU-R BS.1770-4): K-weighted mean square summed over both channels, kept as
//!   100 ms sub-blocks for the last ten minutes. *Momentary* is the last 400 ms, *short-term* the
//!   last 3 s, *integrated* is gated (absolute -70 LUFS, relative -10 LU) over 400 ms blocks with
//!   75 % overlap inside the window an agent asks for.
//! - **Peaks**: sample peak, and true peak from 4× oversampling (a 48-tap Kaiser-windowed sinc,
//!   12 taps per phase, as BS.1770 Annex 2 describes), in dBFS / dBTP.
//! - **Overs**: samples at or beyond full scale (on the master, after the limiter, these are
//!   the clamped samples: they should stay 0).
//! - **Silence**: how long since a sub-block was louder than -70 LUFS or peaked over -60 dBFS.
//! - **Spectrum**: octave bands (31.5 Hz .. 16 kHz) of the last 4096 frames, Hann-windowed, in
//!   dB where a full-scale sine reads 0.

use crate::analysis::{fft, k_weighting};
use crate::dsp::Biquad;
use crate::mixer::{Bus, TapBlock, BUS_COUNT};
use specula_core::dmath;

/// Sub-block length (seconds).
pub const SUB_S: f64 = 0.1;
/// Sub-blocks kept: ten minutes.
pub const HISTORY_SUBS: usize = 6000;
/// Frames the spectrum looks at.
pub const SPECTRUM_FRAMES: usize = 4096;
/// Tracks: every bus, then the master output.
pub const TRACKS: usize = BUS_COUNT + 1;
/// The longest window a report covers (seconds).
pub const MAX_WINDOW_S: f64 = HISTORY_SUBS as f64 * SUB_S;
/// Octave band centres of the spectrum (Hz).
pub const BANDS: [f64; 10] = [31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

const TP_TAPS: usize = 12;
const TP_PHASES: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
struct Sub {
    energy: f64,
    peak: f32,
    true_peak: f32,
    overs: u32,
}

struct Track {
    k: [(Biquad, Biquad); 2],
    acc_energy: f64,
    acc_frames: usize,
    acc_peak: f32,
    acc_tp: f32,
    acc_overs: u32,
    subs: Vec<Sub>,
    /// Sub-blocks completed (the newest is at `(count - 1) % HISTORY_SUBS`).
    count: u64,
    last_audible: Option<u64>,
    /// Last 12 input samples per channel, twice (so a phase's taps read one contiguous slice).
    tp_hist: [[f32; 2 * TP_TAPS]; 2],
    tp_pos: usize,
    spec: Vec<f32>,
    spec_pos: usize,
    /// Consecutive silent frames (exact zeros).
    quiet: usize,
}

impl Track {
    fn new(rate: u32) -> Track {
        Track {
            k: [k_weighting(rate), k_weighting(rate)],
            acc_energy: 0.0,
            acc_frames: 0,
            acc_peak: 0.0,
            acc_tp: 0.0,
            acc_overs: 0,
            subs: vec![Sub::default(); HISTORY_SUBS],
            count: 0,
            last_audible: None,
            tp_hist: [[0.0; 2 * TP_TAPS]; 2],
            tp_pos: 0,
            spec: vec![0.0; SPECTRUM_FRAMES],
            spec_pos: 0,
            quiet: 0,
        }
    }

    fn sub(&self, i: u64) -> &Sub {
        &self.subs[(i % HISTORY_SUBS as u64) as usize]
    }
}

/// Closes `t`'s current 100 ms sub-block.
fn finish_sub(t: &mut Track) {
    let sub = Sub { energy: t.acc_energy / t.acc_frames.max(1) as f64, peak: t.acc_peak, true_peak: t.acc_tp.max(t.acc_peak), overs: t.acc_overs };
    let idx = (t.count % HISTORY_SUBS as u64) as usize;
    t.subs[idx] = sub;
    if lufs(sub.energy) > -70.0 || sub.peak > 0.001 {
        t.last_audible = Some(t.count);
    }
    t.count += 1;
    t.acc_energy = 0.0;
    t.acc_frames = 0;
    t.acc_peak = 0.0;
    t.acc_tp = 0.0;
    t.acc_overs = 0;
}

/// The meter over every bus and the master.
pub struct Meter {
    rate: u32,
    sub_frames: usize,
    tracks: Vec<Track>,
    /// Polyphase true-peak filter: `[phase][tap]`.
    tp: [[f32; TP_TAPS]; TP_PHASES],
    frames: u64,
}

/// What a report says about one bus (or the master) over a window.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub track: &'static str,
    /// Last 400 ms and last 3 s (LUFS; `-inf` when silent).
    pub momentary_lufs: f64,
    pub short_term_lufs: f64,
    /// Gated over the window (LUFS; `-inf` when nothing passed the gates).
    pub integrated_lufs: f64,
    pub peak_dbfs: f64,
    pub true_peak_dbtp: f64,
    pub overs: u64,
    /// Seconds since the last audible sub-block (`None`: never audible).
    pub silent_for_s: Option<f64>,
    /// Seconds of the window with audible sub-blocks.
    pub audible_s: f64,
    /// The window actually covered (shorter early on).
    pub window_s: f64,
    /// Octave bands `(centre Hz, dB)`, when asked for.
    pub spectrum: Option<Vec<(f64, f64)>>,
}

fn lufs(energy: f64) -> f64 {
    if energy <= 0.0 {
        f64::NEG_INFINITY
    } else {
        -0.691 + 10.0 * dmath::log2_f64(energy) * std::f64::consts::LOG10_2
    }
}

fn dbfs(amp: f64) -> f64 {
    if amp <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * dmath::log2_f64(amp) * std::f64::consts::LOG10_2
    }
}

/// The 4× true-peak interpolator: h[n] = sinc((n - 23.5) / 4) · Kaiser(β = 7), each phase
/// normalised to unity DC gain. `[phase][tap]`.
fn tp_filter() -> [[f32; TP_TAPS]; TP_PHASES] {
    let mut tp = [[0f32; TP_TAPS]; TP_PHASES];
    let n_total = TP_TAPS * TP_PHASES;
    let centre = (n_total as f64 - 1.0) / 2.0;
    let beta = 7.0;
    let i0 = |x: f64| {
        let (mut sum, mut term) = (1.0, 1.0);
        for k in 1..40 {
            term *= (x * x / 4.0) / (k as f64 * k as f64);
            sum += term;
        }
        sum
    };
    for (p, phase) in tp.iter_mut().enumerate() {
        let mut sum = 0.0;
        let mut taps = [0f64; TP_TAPS];
        for (k, t) in taps.iter_mut().enumerate() {
            let n = (k * TP_PHASES + p) as f64;
            let x = (n - centre) / TP_PHASES as f64;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { dmath::sin_f64(std::f64::consts::PI * x) / (std::f64::consts::PI * x) };
            let r = (n - centre) / (centre + 1.0);
            let w = i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0(beta);
            *t = sinc * w;
            sum += *t;
        }
        for (k, t) in taps.iter().enumerate() {
            phase[k] = (t / sum) as f32;
        }
    }
    tp
}

/// True peak (4× oversampled, linear) of interleaved `samples`.
pub fn true_peak(samples: &[f32], channels: usize) -> f32 {
    let ch = channels.max(1);
    let tp = tp_filter();
    let mut peak = 0f32;
    for c in 0..ch {
        let x: Vec<f32> = samples.iter().skip(c).step_by(ch).copied().collect();
        for m in 0..x.len() {
            peak = peak.max(x[m].abs());
            for phase in &tp {
                let mut acc = 0f32;
                for (k, h) in phase.iter().enumerate() {
                    if m >= k {
                        acc += h * x[m - k];
                    }
                }
                peak = peak.max(acc.abs());
            }
        }
    }
    peak
}

/// Octave-band levels of interleaved `samples` (dB, a full-scale sine reads 0), averaged over up
/// to 64 Hann windows of [`SPECTRUM_FRAMES`] spread over the sound.
pub fn bands(samples: &[f32], channels: usize, rate: u32) -> Vec<(f64, f64)> {
    let ch = channels.max(1);
    let frames = samples.len() / ch;
    let n = SPECTRUM_FRAMES;
    let windows = frames.div_ceil(n).clamp(1, 64);
    let hop = if windows > 1 { (frames.saturating_sub(n)) / (windows - 1) } else { 0 };
    let mut power = vec![0f64; n / 2 + 1];
    let mut re = vec![0f64; n];
    let mut im = vec![0f64; n];
    for w in 0..windows {
        let start = w * hop;
        for i in 0..n {
            let f = start + i;
            let s = if f < frames { (0..ch).map(|c| samples[f * ch + c]).sum::<f32>() / ch as f32 } else { 0.0 };
            let win = 0.5 - 0.5 * dmath::cos_f64(2.0 * std::f64::consts::PI * i as f64 / n as f64);
            re[i] = s as f64 * win;
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for (k, p) in power.iter_mut().enumerate() {
            *p += (re[k] * re[k] + im[k] * im[k]) / windows as f64;
        }
    }
    band_levels(&power, n, rate)
}

/// Octave bands from a power spectrum of an `n`-point Hann FFT.
fn band_levels(power: &[f64], n: usize, rate: u32) -> Vec<(f64, f64)> {
    let bin_hz = rate as f64 / n as f64;
    let norm = 1.5 * (n as f64 / 4.0) * (n as f64 / 4.0);
    BANDS
        .iter()
        .map(|&fc| {
            let (lo, hi) = (fc / std::f64::consts::SQRT_2, fc * std::f64::consts::SQRT_2);
            let (a, b) = ((lo / bin_hz).ceil() as usize, ((hi / bin_hz).floor() as usize).min(n / 2));
            let p: f64 = (a.max(1)..=b).map(|k| power[k]).sum();
            let db = if p > 0.0 { 10.0 * dmath::log2_f64(p / norm) * std::f64::consts::LOG10_2 } else { f64::NEG_INFINITY };
            (fc, db)
        })
        .collect()
}

impl Meter {
    pub fn new(rate: u32) -> Meter {
        let rate = rate.max(8_000);
        let tp = tp_filter();
        Meter { rate, sub_frames: (rate as f64 * SUB_S) as usize, tracks: (0..TRACKS).map(|_| Track::new(rate)).collect(), tp, frames: 0 }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Seconds of audio measured so far.
    pub fn clock_s(&self) -> f64 {
        self.frames as f64 / self.rate as f64
    }

    /// Track names: the buses, then `master_out`.
    pub fn track_name(i: usize) -> &'static str {
        if i < BUS_COUNT {
            Bus::ALL[i].name()
        } else {
            "master_out"
        }
    }

    /// Measures one tap block.
    pub fn feed(&mut self, tap: &TapBlock) {
        if tap.rate != self.rate && tap.rate >= 8_000 {
            // The device changed rate: new filters, keep the history.
            self.rate = tap.rate;
            self.sub_frames = (tap.rate as f64 * SUB_S) as usize;
            for t in &mut self.tracks {
                t.k = [k_weighting(tap.rate), k_weighting(tap.rate)];
            }
        }
        let n = tap.frames.min(crate::mixer::MAX_BLOCK);
        let sub_frames = self.sub_frames;
        for (ti, t) in self.tracks.iter_mut().enumerate() {
            let x = &tap.buses[ti][..n * 2];
            // An idle bus (most of them, most of the time) skips the filters once a second of
            // silence has flushed them: nothing can change but time.
            let silent = x.iter().all(|&s| s == 0.0);
            if silent && t.quiet >= self.rate as usize {
                for _ in 0..n {
                    t.spec[t.spec_pos] = 0.0;
                    t.spec_pos = (t.spec_pos + 1) % SPECTRUM_FRAMES;
                    t.acc_frames += 1;
                    if t.acc_frames >= sub_frames {
                        finish_sub(t);
                    }
                }
                continue;
            }
            t.quiet = if silent { t.quiet + n } else { 0 };
            for f in 0..n {
                let (l, r) = (x[2 * f], x[2 * f + 1]);
                let [kl, kr] = &mut t.k;
                let yl = kl.1.process(kl.0.process(l as f64));
                let yr = kr.1.process(kr.0.process(r as f64));
                t.acc_energy += yl * yl + yr * yr;
                let (al, ar) = (l.abs(), r.abs());
                t.acc_peak = t.acc_peak.max(al).max(ar);
                t.acc_overs += (al >= 1.0) as u32 + (ar >= 1.0) as u32;
                // True peak: push the sample, then evaluate the four phases.
                let p = t.tp_pos;
                for (c, s) in [l, r].into_iter().enumerate() {
                    t.tp_hist[c][p] = s;
                    t.tp_hist[c][p + TP_TAPS] = s;
                }
                t.tp_pos = (p + 1) % TP_TAPS;
                // Taps run newest-first from p (the newest sample) backwards.
                for c in 0..2 {
                    let h = &t.tp_hist[c][p + 1..p + 1 + TP_TAPS];
                    for phase in &self.tp {
                        let mut acc = 0f32;
                        for k in 0..TP_TAPS {
                            acc += phase[k] * h[TP_TAPS - 1 - k];
                        }
                        t.acc_tp = t.acc_tp.max(acc.abs());
                    }
                }
                t.spec[t.spec_pos] = 0.5 * (l + r);
                t.spec_pos = (t.spec_pos + 1) % SPECTRUM_FRAMES;
                t.acc_frames += 1;
                if t.acc_frames >= sub_frames {
                    finish_sub(t);
                }
            }
        }
        self.frames += n as u64;
    }

    /// Mean energy of the last `subs` completed sub-blocks of `t`.
    fn recent(t: &Track, subs: u64) -> f64 {
        let n = subs.min(t.count);
        if n == 0 {
            return 0.0;
        }
        (t.count - n..t.count).map(|i| t.sub(i).energy).sum::<f64>() / n as f64
    }

    /// A report on track `index` (0..[`TRACKS`]) over the last `window_s` seconds.
    pub fn report(&self, index: usize, window_s: f64, spectrum: bool) -> Report {
        let t = &self.tracks[index.min(TRACKS - 1)];
        let window_subs = ((window_s.clamp(SUB_S, MAX_WINDOW_S) / SUB_S).round() as u64).min(t.count);
        let from = t.count - window_subs;
        let (mut peak, mut tp, mut overs, mut audible) = (0f32, 0f32, 0u64, 0u64);
        for i in from..t.count {
            let s = t.sub(i);
            peak = peak.max(s.peak);
            tp = tp.max(s.true_peak);
            overs += s.overs as u64;
            if lufs(s.energy) > -70.0 || s.peak > 0.001 {
                audible += 1;
            }
        }
        // Gated integration over 400 ms blocks (4 sub-blocks), one every 100 ms.
        let mut blocks = Vec::new();
        if window_subs > 0 {
            if window_subs < 4 {
                blocks.push((from..t.count).map(|i| t.sub(i).energy).sum::<f64>() / window_subs as f64);
            } else {
                for end in from + 4..=t.count {
                    blocks.push((end - 4..end).map(|i| t.sub(i).energy).sum::<f64>() / 4.0);
                }
            }
        }
        let abs: Vec<f64> = blocks.into_iter().filter(|&e| lufs(e) > -70.0).collect();
        let integrated = if abs.is_empty() {
            f64::NEG_INFINITY
        } else {
            let rel = lufs(abs.iter().sum::<f64>() / abs.len() as f64) - 10.0;
            let gated: Vec<f64> = abs.into_iter().filter(|&e| lufs(e) > rel).collect();
            lufs(gated.iter().sum::<f64>() / gated.len().max(1) as f64)
        };
        let silent_for_s = t.last_audible.map(|l| (t.count - 1 - l) as f64 * SUB_S);
        Report {
            track: Meter::track_name(index.min(TRACKS - 1)),
            momentary_lufs: lufs(Meter::recent(t, 4)),
            short_term_lufs: lufs(Meter::recent(t, 30)),
            integrated_lufs: integrated,
            peak_dbfs: dbfs(peak as f64),
            true_peak_dbtp: dbfs(tp as f64),
            overs,
            silent_for_s,
            audible_s: audible as f64 * SUB_S,
            window_s: window_subs as f64 * SUB_S,
            spectrum: spectrum.then(|| self.spectrum(t)),
        }
    }

    fn spectrum(&self, t: &Track) -> Vec<(f64, f64)> {
        let n = SPECTRUM_FRAMES;
        let mut re = vec![0f64; n];
        let mut im = vec![0f64; n];
        for (i, r) in re.iter_mut().enumerate() {
            let w = 0.5 - 0.5 * dmath::cos_f64(2.0 * std::f64::consts::PI * i as f64 / n as f64);
            *r = t.spec[(t.spec_pos + i) % n] as f64 * w;
        }
        fft(&mut re, &mut im);
        // A sine of amplitude A puts about (A·N/4)² · 1.5 (Hann's noise bandwidth) in its band.
        let power: Vec<f64> = (0..=n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        band_levels(&power, n, self.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_sine(m: &mut Meter, bus: usize, amp: f32, freq: f32, secs: f64) {
        let rate = m.rate();
        let total = (rate as f64 * secs) as usize;
        let mut tap = TapBlock::boxed();
        let mut i = 0usize;
        while i < total {
            let n = (total - i).min(crate::mixer::MAX_BLOCK);
            tap.frames = n;
            tap.rate = rate;
            for b in tap.buses.iter_mut() {
                b.fill(0.0);
            }
            for f in 0..n {
                let s = amp * (2.0 * std::f32::consts::PI * freq * (i + f) as f32 / rate as f32).sin();
                tap.buses[bus][2 * f] = s;
                tap.buses[bus][2 * f + 1] = s;
            }
            m.feed(&tap);
            i += n;
        }
    }

    #[test]
    fn a_full_scale_1k_sine_reads_like_the_standard() {
        let mut m = Meter::new(48_000);
        feed_sine(&mut m, Bus::Music as usize, 0.999, 997.0, 5.0);
        let r = m.report(Bus::Music as usize, 5.0, true);
        // BS.1770: a 0 dBFS 1 kHz sine in both channels reads +0.0 LUFS (each channel alone
        // reads -3.01). Amplitude 0.999 so no sample touches full scale (an over).
        assert!((r.integrated_lufs - 0.0).abs() < 0.2, "{r:?}");
        assert!((r.momentary_lufs - 0.0).abs() < 0.2);
        assert!((r.short_term_lufs - 0.0).abs() < 0.2);
        assert!(r.peak_dbfs > -0.05 && r.peak_dbfs <= 0.0);
        assert!(r.true_peak_dbtp > -0.1 && r.true_peak_dbtp < 0.3, "{}", r.true_peak_dbtp);
        assert_eq!(r.overs, 0);
        assert_eq!(r.silent_for_s, Some(0.0));
        let spec = r.spectrum.unwrap();
        let kilo = spec.iter().find(|b| b.0 == 1000.0).unwrap().1;
        assert!((kilo - 0.0).abs() < 1.0, "1 kHz band {kilo}");
        assert!(spec.iter().filter(|b| b.0 != 1000.0).all(|b| b.1 < kilo - 25.0), "{spec:?}");
        // Other buses heard nothing.
        let sfx = m.report(Bus::Sfx as usize, 5.0, false);
        assert_eq!(sfx.integrated_lufs, f64::NEG_INFINITY);
        assert_eq!(sfx.silent_for_s, None);
    }

    #[test]
    fn true_peak_sees_intersample_overs() {
        // A sine at a quarter of the rate, phase-shifted 45°: samples peak at 0.707·A, the wave
        // itself at A.
        let mut m = Meter::new(48_000);
        let mut tap = TapBlock::boxed();
        tap.frames = 256;
        tap.rate = 48_000;
        for _ in 0..40 {
            for f in 0..256 {
                let s = (std::f32::consts::FRAC_PI_2 * f as f32 + std::f32::consts::FRAC_PI_4).sin();
                tap.buses[TapBlock::MASTER][2 * f] = s;
                tap.buses[TapBlock::MASTER][2 * f + 1] = s;
            }
            m.feed(&tap);
        }
        let r = m.report(TapBlock::MASTER, 1.0, false);
        assert!((r.peak_dbfs - -3.01).abs() < 0.05, "{}", r.peak_dbfs);
        assert!(r.true_peak_dbtp > -0.6, "{}", r.true_peak_dbtp);
    }

    #[test]
    fn silence_and_gating_over_windows() {
        let mut m = Meter::new(48_000);
        feed_sine(&mut m, Bus::Sfx as usize, 0.5, 440.0, 2.0);
        feed_sine(&mut m, Bus::Sfx as usize, 0.0, 440.0, 3.0);
        let r = m.report(Bus::Sfx as usize, 10.0, false);
        // Integrated ignores the silence (absolute gate); the window is what exists.
        let loud = m.report(Bus::Sfx as usize, 10.0, false).integrated_lufs;
        // -6.02 dB of amplitude, -0.69 offset, about -0.33 dB of K-weighting at 440 Hz.
        assert!((loud - -7.04).abs() < 0.2, "{loud}");
        assert!((r.window_s - 5.0).abs() < 0.11);
        assert!((r.audible_s - 2.0).abs() < 0.11, "{}", r.audible_s);
        let quiet = r.silent_for_s.unwrap();
        assert!((quiet - 3.0).abs() < 0.15, "{quiet}");
        assert_eq!(r.momentary_lufs, f64::NEG_INFINITY);
        // A 1 s window sees only silence.
        assert_eq!(m.report(Bus::Sfx as usize, 1.0, false).integrated_lufs, f64::NEG_INFINITY);
        // Sound after a long silence is measured again (the idle fast path skips only silence;
        // a mutant that skipped every block after a quiet second survived until this check).
        feed_sine(&mut m, Bus::Sfx as usize, 0.5, 440.0, 1.0);
        let back = m.report(Bus::Sfx as usize, 0.4, false);
        assert!(back.momentary_lufs > -8.0 && back.momentary_lufs < -6.0, "{back:?}");
        assert_eq!(back.silent_for_s, Some(0.0));
    }
}
