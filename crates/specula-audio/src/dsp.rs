//! DSP building blocks shared by the synthesiser, the mixer and the analysers.
//!
//! Everything here uses IEEE basic operations plus `specula_core::dmath`, never the platform maths
//! library, so a recipe renders to the same bits on every platform (F2). Coefficient helpers are
//! `f64`; per-sample state is `f32` (synth, mixer) or `f64` (analysis).

use specula_core::dmath;

pub const PI: f64 = std::f64::consts::PI;
pub const TAU: f64 = std::f64::consts::TAU;
/// log2(10) / 20: `db_to_gain(db) = 2^(db · this)`.
const DB_TO_LOG2: f64 = 0.166_096_404_744_368_1;

/// Decibels to a linear gain (deterministic).
pub fn db_to_gain(db: f64) -> f64 {
    dmath::exp2_f64(db * DB_TO_LOG2)
}

/// A linear gain to decibels (`-inf` for 0).
pub fn gain_to_db(gain: f64) -> f64 {
    if gain <= 0.0 {
        return f64::NEG_INFINITY;
    }
    dmath::log2_f64(gain) / DB_TO_LOG2
}

/// Semitones to a frequency ratio.
pub fn semitones(st: f64) -> f64 {
    dmath::exp2_f64(st / 12.0)
}

/// Soft saturation: a rational `tanh` approximation (Padé 3/2), exact at 0, smooth, monotonic,
/// clamped to ±1 beyond |x| = 3 where it meets 1 with zero slope.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x <= -3.0 {
        -1.0
    } else if x >= 3.0 {
        1.0
    } else {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

/// Constant-power pan law: `pan` in -1 (left) .. 1 (right) → (left, right) gains with
/// `l² + r² = 1`; the centre is -3 dB on each side.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0) as f64;
    let theta = (p + 1.0) * (PI / 4.0);
    let (s, c) = dmath::sin_cos_f64(theta);
    (c as f32, s as f32)
}

/// One-pole low-pass (`hp == false`) or high-pass, 6 dB/octave, `y += a (x - y)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    a: f32,
    y: f32,
    hp: bool,
}

impl OnePole {
    pub fn new(hp: bool) -> OnePole {
        OnePole { a: 1.0, y: 0.0, hp }
    }

    /// Coefficient for cutoff `fc` at sample rate `fs` (matched pole: `a = 1 - e^(-2π fc/fs)`).
    pub fn coefficient(fc: f64, fs: f64) -> f32 {
        let fc = fc.clamp(1.0, fs * 0.49);
        (1.0 - dmath::exp_f64(-TAU * fc / fs)) as f32
    }

    pub fn set(&mut self, fc: f64, fs: f64) {
        self.a = OnePole::coefficient(fc, fs);
    }

    pub fn set_coefficient(&mut self, a: f32) {
        self.a = a;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        if self.hp {
            x - self.y
        } else {
            self.y
        }
    }

    pub fn reset(&mut self) {
        self.y = 0.0;
    }
}

/// The output of a [`Svf`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvfMode {
    Low,
    High,
    Band,
    Notch,
}

/// A state-variable filter (Simper's trapezoidal "TPT" form, 12 dB/octave with resonance).
/// Stays stable when the cutoff is modulated every few samples, unlike a direct-form biquad.
#[derive(Clone, Copy, Debug)]
pub struct Svf {
    mode: SvfMode,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn new(mode: SvfMode, q: f64) -> Svf {
        Svf { mode, k: (1.0 / q.max(0.05)) as f32, a1: 1.0, a2: 0.0, a3: 0.0, ic1: 0.0, ic2: 0.0 }
    }

    /// Sets the cutoff (centre frequency for band/notch) in Hz at sample rate `fs`.
    pub fn set(&mut self, fc: f64, fs: f64) {
        let fc = fc.clamp(1.0, fs * 0.49);
        let g = dmath::tan_f64(PI * fc / fs);
        let k = self.k as f64;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        self.a1 = a1 as f32;
        self.a2 = a2 as f32;
        self.a3 = (g * a2) as f32;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match self.mode {
            SvfMode::Low => v2,
            SvfMode::Band => v1,
            SvfMode::High => x - self.k * v1 - v2,
            SvfMode::Notch => x - self.k * v1,
        }
    }
}

/// A direct-form-I biquad in `f64` (analysis: K-weighting).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub b: [f64; 3],
    pub a: [f64; 2],
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    /// `b0 + b1 z⁻¹ + b2 z⁻²` over `1 + a1 z⁻¹ + a2 z⁻²`.
    pub fn new(b: [f64; 3], a: [f64; 2]) -> Biquad {
        Biquad { b, a, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x1 + self.b[2] * self.x2 - self.a[0] * self.y1 - self.a[1] * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A fixed-length delay line (allocated once).
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    pub fn new(len: usize) -> DelayLine {
        DelayLine { buf: vec![0.0; len.max(1)], pos: 0 }
    }

    /// The sample written `len` samples ago.
    #[inline]
    pub fn read(&self) -> f32 {
        self.buf[self.pos]
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }
}

/// Freeverb's low-pass-feedback comb.
#[derive(Clone, Debug)]
struct Comb {
    line: DelayLine,
    store: f32,
}

impl Comb {
    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.line.read();
        self.store = out * (1.0 - damp) + self.store * damp;
        self.line.write(x + self.store * feedback);
        out
    }
}

/// Freeverb's Schroeder all-pass.
#[derive(Clone, Debug)]
struct Allpass {
    line: DelayLine,
}

impl Allpass {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let buf = self.line.read();
        let out = buf - x;
        self.line.write(x + buf * 0.5);
        out
    }
}

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

/// One channel of Freeverb (Jezar's public-domain design: 8 parallel combs, 4 series all-passes),
/// with delay lengths scaled from 44.1 kHz to the render rate.
#[derive(Clone, Debug)]
pub struct ReverbChannel {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl ReverbChannel {
    pub fn new(rate: f64, spread: usize) -> ReverbChannel {
        let scale = rate / 44_100.0;
        let len = |n: usize| (((n + spread) as f64 * scale) as usize).max(1);
        ReverbChannel {
            combs: COMB_TUNING.iter().map(|&n| Comb { line: DelayLine::new(len(n)), store: 0.0 }).collect(),
            allpasses: ALLPASS_TUNING.iter().map(|&n| Allpass { line: DelayLine::new(len(n)) }).collect(),
        }
    }

    /// Wet output for input `x` (Freeverb's fixed input gain 0.015 is applied here).
    #[inline]
    pub fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let input = x * 0.015;
        let mut acc = 0.0;
        for comb in &mut self.combs {
            acc += comb.process(input, feedback, damp);
        }
        for ap in &mut self.allpasses {
            acc = ap.process(acc);
        }
        acc
    }
}

/// Freeverb parameters mapped to its internal constants.
#[derive(Clone, Copy, Debug)]
pub struct ReverbParams {
    pub feedback: f32,
    pub damp: f32,
}

impl ReverbParams {
    /// `room` and `damp` in 0..1, as in Freeverb's UI.
    pub fn new(room: f32, damp: f32) -> ReverbParams {
        ReverbParams { feedback: room.clamp(0.0, 1.0) * 0.28 + 0.7, damp: damp.clamp(0.0, 1.0) * 0.4 }
    }
}

/// Freeverb's left/right spread in samples (at 44.1 kHz).
pub const REVERB_STEREO_SPREAD: usize = STEREO_SPREAD;

/// A 32-bit integer hash (the `lowbias32` finaliser): white noise without a sequential RNG, so any
/// sample can be computed independently and identically everywhere.
#[inline]
pub fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Uniform noise in [-1, 1) from `hash32(seed ^ n)`.
#[inline]
pub fn hash_noise(seed: u32, n: u32) -> f32 {
    let h = hash32(seed ^ hash32(n));
    // 24 high bits → [0, 1) exactly representable, then to [-1, 1).
    ((h >> 8) as f32) * (1.0 / 8_388_608.0) - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels() {
        assert!((db_to_gain(-6.020_599_913) - 0.5).abs() < 1e-9);
        assert!((db_to_gain(20.0) - 10.0).abs() < 1e-9);
        assert!((gain_to_db(0.1) + 20.0).abs() < 1e-9);
        assert_eq!(gain_to_db(0.0), f64::NEG_INFINITY);
        assert!((semitones(12.0) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn pan_law_is_constant_power() {
        for i in 0..=20 {
            let p = -1.0 + i as f32 * 0.1;
            let (l, r) = pan_gains(p);
            assert!((l * l + r * r - 1.0).abs() < 1e-6, "pan {p}: {l} {r}");
        }
        let (l, r) = pan_gains(0.0);
        assert!((l - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6 && (r - l).abs() < 1e-6);
        assert_eq!(pan_gains(-1.0), (1.0, 0.0));
        let (l, r) = pan_gains(1.0);
        assert!(l.abs() < 1e-7 && (r - 1.0).abs() < 1e-7);
    }

    #[test]
    fn soft_clip_shape() {
        assert_eq!(soft_clip(0.0), 0.0);
        assert_eq!(soft_clip(5.0), 1.0);
        assert_eq!(soft_clip(-5.0), -1.0);
        assert!((soft_clip(3.0) - 1.0).abs() < 1e-6);
        let mut prev = -1.0;
        for i in -300..=300 {
            let y = soft_clip(i as f32 / 100.0);
            assert!(y >= prev - 1e-6, "monotonic");
            prev = y;
        }
        assert!((soft_clip(0.5) - 0.5f32.tanh()).abs() < 0.01);
    }

    /// Steady-state gain of a filter at frequency `f` (sine in, RMS out / RMS in).
    fn gain_at(mut f: impl FnMut(f32) -> f32, freq: f64, fs: f64) -> f64 {
        let n = (fs * 0.5) as usize;
        let (mut e_in, mut e_out) = (0.0, 0.0);
        for i in 0..n {
            let x = dmath::sin_f64(TAU * freq * i as f64 / fs) as f32;
            let y = f(x);
            if i > n / 2 {
                e_in += (x * x) as f64;
                e_out += (y * y) as f64;
            }
        }
        (e_out / e_in).sqrt()
    }

    #[test]
    fn one_pole_cutoff_is_minus_3db() {
        let fs = 48_000.0;
        let mut lp = OnePole::new(false);
        lp.set(1000.0, fs);
        let g = gain_at(|x| lp.process(x), 1000.0, fs);
        assert!((gain_to_db(g) + 3.0).abs() < 0.5, "{}", gain_to_db(g));
        let mut lp = OnePole::new(false);
        lp.set(1000.0, fs);
        assert!(gain_at(|x| lp.process(x), 50.0, fs) > 0.99);
        let mut hp = OnePole::new(true);
        hp.set(1000.0, fs);
        assert!(gain_at(|x| hp.process(x), 50.0, fs) < 0.1);
    }

    #[test]
    fn svf_responses() {
        let fs = 48_000.0;
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let mk = |mode| {
            let mut f = Svf::new(mode, q);
            f.set(1000.0, fs);
            f
        };
        // Butterworth Q: -3 dB at the cutoff for low and high pass, 12 dB/oct beyond.
        let mut lp = mk(SvfMode::Low);
        assert!((gain_to_db(gain_at(|x| lp.process(x), 1000.0, fs)) + 3.01).abs() < 0.1);
        let mut lp = mk(SvfMode::Low);
        let at4k = gain_to_db(gain_at(|x| lp.process(x), 4000.0, fs));
        assert!((-26.0..-22.0).contains(&at4k), "{at4k}");
        let mut hp = mk(SvfMode::High);
        assert!((gain_to_db(gain_at(|x| hp.process(x), 1000.0, fs)) + 3.01).abs() < 0.1);
        let mut bp = mk(SvfMode::Band);
        // The TPT band output is s/(s² + ks + 1): gain 1/k = Q at the centre.
        let centre = gain_at(|x| bp.process(x), 1000.0, fs);
        assert!((centre - q).abs() < 0.02, "{centre}");
        let mut notch = mk(SvfMode::Notch);
        assert!(gain_at(|x| notch.process(x), 1000.0, fs) < 0.01);
        // Resonance: Q = 8 peaks well above unity at the cutoff.
        let mut res = Svf::new(SvfMode::Low, 8.0);
        res.set(1000.0, fs);
        assert!((gain_at(|x| res.process(x), 1000.0, fs) - 8.0).abs() < 0.3);
    }

    #[test]
    fn svf_impulse_response_is_stable_and_sums_to_dc_gain() {
        let mut lp = Svf::new(SvfMode::Low, 0.707);
        lp.set(500.0, 48_000.0);
        let mut sum = 0.0f64;
        for i in 0..48_000 {
            let y = lp.process(if i == 0 { 1.0 } else { 0.0 });
            assert!(y.is_finite());
            sum += y as f64;
        }
        assert!((sum - 1.0).abs() < 1e-3, "DC gain {sum}");
    }

    #[test]
    fn reverb_decays() {
        let mut r = ReverbChannel::new(48_000.0, 0);
        let p = ReverbParams::new(0.5, 0.5);
        let mut early = 0.0f64;
        let mut late = 0.0f64;
        for i in 0..96_000 {
            let y = r.process(if i == 0 { 1.0 } else { 0.0 }, p.feedback, p.damp) as f64;
            if i < 24_000 {
                early += y * y;
            } else if i >= 72_000 {
                late += y * y;
            }
        }
        assert!(early > 0.0 && late < early * 0.01, "{early} {late}");
    }

    #[test]
    fn hash_noise_is_uniform_and_centred() {
        let mut sum = 0.0f64;
        let mut sq = 0.0f64;
        let n = 100_000;
        for i in 0..n {
            let x = hash_noise(7, i) as f64;
            assert!((-1.0..1.0).contains(&x));
            sum += x;
            sq += x * x;
        }
        assert!((sum / n as f64).abs() < 0.01);
        assert!((sq / n as f64 - 1.0 / 3.0).abs() < 0.01);
    }
}
