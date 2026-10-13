//! Deterministic recipe rendering.
//!
//! Same recipe + same [`RenderOptions`] ⇒ the same `f32` bits on every platform (F2): only IEEE
//! basic operations and `specula_core::dmath`, integer-hash noise, seeded PCG streams keyed by
//! recipe paths, and a fixed evaluation order.

use super::recipe::*;
use crate::analysis;
use crate::dsp::{self, hash32, hash_noise, OnePole, ReverbChannel, ReverbParams, Svf, SvfMode, PI, TAU};
use crate::error::{AudioError, ErrorCode, Result};
use crate::mixer::Bus;
use crate::sound::Sound;
use specula_core::dmath;
use specula_core::hash::{hash_str, StableHasher};
use specula_core::rng::Rng;

/// How to render a recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Output sample rate (8 kHz ..= 192 kHz). Sounds are normally rendered at 48 kHz and the
    /// mixer converts to the device rate.
    pub rate: u32,
    /// Variation index: 0 is the canonical render; 1, 2, … apply the recipe's `vary` ranges and
    /// redraw every `{"rand": …}` value, for non-repetitive one-shots.
    pub variation: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions { rate: 48_000, variation: 0 }
    }
}

/// A rendered recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub name: String,
    pub bus: Bus,
    pub rate: u32,
    pub channels: u16,
    /// Interleaved samples.
    pub samples: Vec<f32>,
    /// Rendered as a seamless loop (play with `looping: true`).
    pub looping: bool,
    /// Stable hash of the sample bits (F2 checks, caching).
    pub hash: u64,
    /// Gain the loudness normaliser applied (dB, 0 without a `loudness` target).
    pub normalize_db: f64,
    /// Extra reduction to respect `peak_ceiling_db` (dB ≤ 0; non-zero means the loudness target
    /// could not be met without clipping).
    pub ceiling_db: f64,
}

impl Rendered {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn duration_s(&self) -> f64 {
        self.frames() as f64 / self.rate as f64
    }

    /// A playable [`Sound`] (loop region = whole sound when looping).
    pub fn to_sound(&self) -> Result<Sound> {
        Sound::new(self.name.clone(), self.rate, self.channels, self.samples.clone())
    }

    pub fn metrics(&self) -> analysis::Metrics {
        analysis::analyze(&self.samples, self.channels as usize, self.rate)
    }
}

/// Parses and renders recipe text in one go.
pub fn render_text(text: &str, opts: &RenderOptions) -> Result<Rendered> {
    render(&Recipe::parse(text)?, opts)
}

#[derive(Clone, Copy)]
struct Ctx {
    seed: u64,
    variation: u32,
    instance: u32,
}

impl Ctx {
    /// Uniform [0, 1) from the stream `key` for this variation and instance.
    fn draw(&self, key: u64) -> f64 {
        let mut r = Rng::new(self.seed ^ key, ((self.variation as u64) << 31) ^ self.instance as u64);
        r.next_u32() as f64 * (1.0 / 4_294_967_296.0)
    }

    fn v(&self, n: &Num) -> f64 {
        if n.lo == n.hi {
            n.lo
        } else {
            n.lo + (n.hi - n.lo) * self.draw(n.key)
        }
    }

    /// Uniform in [-1, 1).
    fn signed(&self, key: u64) -> f64 {
        self.draw(key) * 2.0 - 1.0
    }
}

/// A resolved [`Param`] evaluated at increasing times.
struct Curve {
    t: Vec<f64>,
    /// Values, or log2 values for exp curves.
    v: Vec<f64>,
    exp: bool,
    i: usize,
}

impl Curve {
    fn new(p: &Param, cx: &Ctx) -> Curve {
        match p {
            Param::Const(n) => Curve { t: vec![0.0], v: vec![cx.v(n)], exp: false, i: 0 },
            Param::Points { points, exp } => {
                let t = points.iter().map(|(t, _)| cx.v(t)).collect();
                let v = points.iter().map(|(_, v)| if *exp { dmath::log2_f64(cx.v(v)) } else { cx.v(v) }).collect();
                Curve { t, v, exp: *exp, i: 0 }
            }
        }
    }

    fn is_const(&self) -> bool {
        self.t.len() == 1
    }

    fn out(&self, v: f64) -> f64 {
        if self.exp {
            dmath::exp2_f64(v)
        } else {
            v
        }
    }

    /// Value at time `t` (seconds). `t` must not decrease between calls.
    fn at(&mut self, t: f64) -> f64 {
        let n = self.t.len();
        if n == 1 || t <= self.t[0] {
            return self.out(self.v[0]);
        }
        while self.i + 1 < n && self.t[self.i + 1] <= t {
            self.i += 1;
        }
        if self.i + 1 >= n {
            return self.out(self.v[n - 1]);
        }
        let (t0, t1) = (self.t[self.i], self.t[self.i + 1]);
        let (v0, v1) = (self.v[self.i], self.v[self.i + 1]);
        let x = (t - t0) / (t1 - t0);
        self.out(v0 + (v1 - v0) * x)
    }

    fn last_time(&self) -> f64 {
        self.t[self.t.len() - 1]
    }

    fn last_value(&self) -> f64 {
        self.out(self.v[self.v.len() - 1])
    }
}

/// Exponential segment shape: 1 at x = 0 → 0 at x = 1, -52 dB-ish curvature.
fn exp_shape(x: f64) -> f64 {
    const K: f64 = 6.0;
    const END: f64 = 0.002_478_752_176_666_358_4; // e^-6
    (dmath::exp_f64(-K * x) - END) / (1.0 - END)
}

enum EnvEval {
    Gate { gate: f64 },
    Adsr { a: f64, h: f64, d: f64, s: f64, r: f64, exp: bool, gate: f64, at_gate: f64 },
    Points(Curve),
}

impl EnvEval {
    fn new(env: Option<&Env>, gate: f64, cx: &Ctx) -> EnvEval {
        match env {
            None => EnvEval::Gate { gate },
            Some(Env::Points(p)) => EnvEval::Points(Curve::new(&Param::Points { points: p.clone(), exp: false }, cx)),
            Some(Env::Adsr { attack, hold, decay, sustain, release, exp }) => {
                let mut e = EnvEval::Adsr {
                    a: cx.v(attack),
                    h: cx.v(hold),
                    d: cx.v(decay),
                    s: cx.v(sustain),
                    r: cx.v(release),
                    exp: *exp,
                    gate,
                    at_gate: 0.0,
                };
                let g = e.pre_release(gate);
                if let EnvEval::Adsr { at_gate, .. } = &mut e {
                    *at_gate = g;
                }
                e
            }
        }
    }

    fn shape(exp: bool, x: f64) -> f64 {
        if exp {
            exp_shape(x)
        } else {
            1.0 - x
        }
    }

    fn pre_release(&self, t: f64) -> f64 {
        let EnvEval::Adsr { a, h, d, s, exp, .. } = *self else { return 1.0 };
        if t < a {
            t / a
        } else if t < a + h {
            1.0
        } else if t < a + h + d {
            s + (1.0 - s) * EnvEval::shape(exp, (t - a - h) / d)
        } else {
            s
        }
    }

    fn level(&mut self, t: f64) -> f64 {
        match *self {
            EnvEval::Gate { gate } => {
                if t < gate {
                    1.0
                } else {
                    0.0
                }
            }
            EnvEval::Points(ref mut c) => c.at(t),
            EnvEval::Adsr { r, exp, gate, at_gate, .. } => {
                if t < gate {
                    self.pre_release(t)
                } else if r <= 0.0 {
                    0.0
                } else {
                    let x = (t - gate) / r;
                    if x >= 1.0 {
                        0.0
                    } else {
                        at_gate * EnvEval::shape(exp, x)
                    }
                }
            }
        }
    }

    /// Time after which the level stays 0 (`None` = sounds until the end).
    fn end(&self) -> Option<f64> {
        match self {
            EnvEval::Gate { gate } => Some(*gate),
            EnvEval::Adsr { r, gate, .. } => Some(gate + r),
            EnvEval::Points(c) => (c.last_value() == 0.0).then(|| c.last_time()),
        }
    }
}

fn frac(x: f64) -> f64 {
    x - dmath::floor_f64(x)
}

fn blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

struct PartialState {
    ratio: f64,
    gain: f64,
    level: f64,
    mul: f64,
}

struct Osc {
    wave: Wave,
    rate: f64,
    detune: Vec<f64>,
    uni_gain: f64,
    partials: Vec<PartialState>,
    phases: Vec<f64>,
    fm: Option<(f64, Curve, f64)>,
    duty: Curve,
    noise_seed: u32,
    noise_n: u32,
    hold: f32,
    noise_phase: f64,
    noise_first: bool,
    pink: [f32; 3],
    brown: f32,
}

impl Osc {
    fn new(src: &Source, cx: &Ctx, rate: f64, noise_seed: u32) -> Osc {
        let voices = src.unison.as_ref().map_or(1, |u| u.voices) as usize;
        let cents = src.unison.as_ref().map_or(0.0, |u| cx.v(&u.cents));
        let detune: Vec<f64> = (0..voices)
            .map(|v| if voices == 1 { 1.0 } else { dsp::semitones((v as f64 / (voices - 1) as f64 * 2.0 - 1.0) * cents / 100.0) })
            .collect();
        let partials: Vec<PartialState> = if src.partials.is_empty() {
            vec![PartialState { ratio: 1.0, gain: 1.0, level: 1.0, mul: 1.0 }]
        } else {
            src.partials
                .iter()
                .map(|p| PartialState {
                    ratio: cx.v(&p.ratio),
                    gain: cx.v(&p.gain),
                    level: 1.0,
                    mul: p.decay.as_ref().map_or(1.0, |d| dmath::exp_f64(-1.0 / (cx.v(d) * rate))),
                })
                .collect()
        };
        let phase0 = cx.v(&src.phase);
        // Unison voices start at spread phases so they don't sum coherently at t = 0.
        let phases = (0..voices * partials.len())
            .map(|i| {
                let v = i / partials.len();
                if v == 0 {
                    phase0
                } else {
                    frac(phase0 + (hash32(noise_seed ^ (v as u32).wrapping_mul(0x9E37_79B9)) >> 8) as f64 / 16_777_216.0)
                }
            })
            .collect();
        Osc {
            wave: src.wave,
            rate,
            uni_gain: 1.0 / (voices as f64).sqrt(),
            detune,
            partials,
            phases,
            fm: src.fm.as_ref().map(|f| (cx.v(&f.ratio), Curve::new(&f.index, cx), 0.0)),
            duty: Curve::new(&src.duty, cx),
            noise_seed,
            noise_n: 0,
            hold: 0.0,
            noise_phase: 0.0,
            noise_first: true,
            pink: [0.0; 3],
            brown: 0.0,
        }
    }

    fn white(&mut self) -> f32 {
        self.noise_n = self.noise_n.wrapping_add(1);
        hash_noise(self.noise_seed, self.noise_n)
    }

    /// Next sample at frequency `f` Hz (ignored by plain noise), layer time `t`.
    fn next(&mut self, f: f64, t: f64) -> f32 {
        let nyq_inc = 0.5;
        match self.wave {
            Wave::Noise(color) => {
                let w = if f > 0.0 {
                    self.noise_phase += f / self.rate;
                    if self.noise_first || self.noise_phase >= 1.0 {
                        self.noise_first = false;
                        self.noise_phase = frac(self.noise_phase);
                        self.hold = self.white();
                    }
                    self.hold
                } else {
                    self.white()
                };
                match color {
                    NoiseColor::White => w,
                    NoiseColor::Pink => {
                        // Paul Kellet's economy pink filter (-3 dB/octave within ±0.5 dB).
                        let p = &mut self.pink;
                        p[0] = 0.99765 * p[0] + w * 0.099_046;
                        p[1] = 0.963 * p[1] + w * 0.296_516_4;
                        p[2] = 0.57 * p[2] + w * 1.052_691_3;
                        (p[0] + p[1] + p[2] + w * 0.1848) * 0.25
                    }
                    NoiseColor::Brown => {
                        self.brown = (self.brown + 0.02 * w) / 1.02;
                        self.brown * 3.5
                    }
                }
            }
            Wave::Sine => {
                let pm = match &mut self.fm {
                    Some((ratio, index, phase)) => {
                        let v = index.at(t) * dmath::sin_f64(TAU * *phase);
                        *phase = frac(*phase + f * *ratio / self.rate);
                        v
                    }
                    None => 0.0,
                };
                let np = self.partials.len();
                let mut sum = 0.0;
                for (v, det) in self.detune.iter().enumerate() {
                    let fv = f * det;
                    for (p, part) in self.partials.iter().enumerate() {
                        let ph = &mut self.phases[v * np + p];
                        let inc = fv * part.ratio / self.rate;
                        if inc < nyq_inc && part.level > 1e-7 {
                            sum += part.gain * part.level * dmath::sin_f64(TAU * *ph + pm);
                        }
                        *ph = frac(*ph + inc);
                    }
                }
                for part in &mut self.partials {
                    part.level *= part.mul;
                }
                (sum * self.uni_gain) as f32
            }
            Wave::Saw | Wave::Square | Wave::Triangle => {
                let duty = if self.wave == Wave::Square { self.duty.at(t) } else { 0.5 };
                let mut sum = 0.0;
                for (v, det) in self.detune.iter().enumerate() {
                    let inc = f * det / self.rate;
                    let dt = inc.min(nyq_inc);
                    let ph = self.phases[v * self.partials.len()];
                    let s = match self.wave {
                        Wave::Saw => 2.0 * ph - 1.0 - blep(ph, dt),
                        Wave::Square => {
                            let naive = if ph < duty { 1.0 } else { -1.0 };
                            // Minus the pulse's mean (2·duty − 1), so narrow pulses carry no DC.
                            naive + blep(ph, dt) - blep(frac(ph - duty + 1.0), dt) - (2.0 * duty - 1.0)
                        }
                        _ => 1.0 - 4.0 * (ph - 0.5).abs(),
                    };
                    sum += s;
                    self.phases[v * self.partials.len()] = frac(ph + inc);
                }
                (sum * self.uni_gain) as f32
            }
        }
    }
}

enum FilterState {
    Svf(Svf),
    One(OnePole),
}

struct FilterEval {
    st: FilterState,
    cutoff: Curve,
}

impl FilterEval {
    fn new(f: &Filter, cx: &Ctx, rate: f64) -> FilterEval {
        let q = cx.v(&f.q);
        let st = match f.kind {
            FilterKind::Lowpass => FilterState::Svf(Svf::new(SvfMode::Low, q)),
            FilterKind::Highpass => FilterState::Svf(Svf::new(SvfMode::High, q)),
            FilterKind::Bandpass => FilterState::Svf(Svf::new(SvfMode::Band, q)),
            FilterKind::Notch => FilterState::Svf(Svf::new(SvfMode::Notch, q)),
            FilterKind::Lp1 => FilterState::One(OnePole::new(false)),
            FilterKind::Hp1 => FilterState::One(OnePole::new(true)),
        };
        let mut fe = FilterEval { st, cutoff: Curve::new(&f.cutoff, cx) };
        let c0 = fe.cutoff.at(0.0);
        fe.set(c0, rate);
        fe
    }

    fn set(&mut self, fc: f64, rate: f64) {
        match &mut self.st {
            FilterState::Svf(s) => s.set(fc, rate),
            FilterState::One(o) => o.set(fc, rate),
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        match &mut self.st {
            FilterState::Svf(s) => s.process(x),
            FilterState::One(o) => o.process(x),
        }
    }
}

/// Cutoff modulation is applied every this many samples.
const MOD_INTERVAL: usize = 16;

enum FxState {
    Distort { drive: f32, mix: f32 },
    Bitcrush { levels: f32, inc: f64, phase: f64, hold: f32 },
    Delay { line: dsp::DelayLine, feedback: f32, mix: f32, damp: f32, lp: f32 },
    Reverb { ch: ReverbChannel, p: ReverbParams, mix: f32 },
    Tremolo { rate: f64, depth: f32 },
}

impl FxState {
    fn new(fx: &Fx, cx: &Ctx, rate: f64, spread: usize) -> FxState {
        match fx {
            Fx::Distort { drive, mix } => FxState::Distort { drive: cx.v(drive) as f32, mix: cx.v(mix) as f32 },
            Fx::Bitcrush { bits, rate: r } => FxState::Bitcrush {
                levels: dmath::exp2_f64(cx.v(bits) - 1.0) as f32,
                inc: (cx.v(r) / rate).min(1.0),
                phase: 1.0,
                hold: 0.0,
            },
            Fx::Delay { time, feedback, mix, damp } => FxState::Delay {
                line: dsp::DelayLine::new((cx.v(time) * rate) as usize),
                feedback: cx.v(feedback) as f32,
                mix: cx.v(mix) as f32,
                damp: cx.v(damp) as f32,
                lp: 0.0,
            },
            Fx::Reverb { room, damp, mix, .. } => FxState::Reverb {
                ch: ReverbChannel::new(rate, spread),
                p: ReverbParams::new(cx.v(room) as f32, cx.v(damp) as f32),
                mix: cx.v(mix) as f32,
            },
            Fx::Tremolo { rate: r, depth } => FxState::Tremolo { rate: cx.v(r), depth: cx.v(depth) as f32 },
        }
    }

    #[inline]
    fn process(&mut self, x: f32, t: f64) -> f32 {
        match self {
            FxState::Distort { drive, mix } => {
                let y = dsp::soft_clip(x * *drive);
                x + (y - x) * *mix
            }
            FxState::Bitcrush { levels, inc, phase, hold } => {
                *phase += *inc;
                if *phase >= 1.0 {
                    *phase -= dmath::floor_f64(*phase);
                    *hold = x;
                }
                dmath::round(*hold * *levels) / *levels
            }
            FxState::Delay { line, feedback, mix, damp, lp } => {
                let d = line.read();
                *lp += (1.0 - *damp) * (d - *lp);
                line.write(x + *feedback * *lp);
                x + *mix * d
            }
            FxState::Reverb { ch, p, mix } => {
                let wet = ch.process(x, p.feedback, p.damp) * 3.0;
                x * (1.0 - *mix) + wet * *mix
            }
            FxState::Tremolo { rate, depth } => {
                let m = 0.5 - 0.5 * dmath::cos_f64(TAU * *rate * t);
                x * (1.0 - *depth * m as f32)
            }
        }
    }
}

const KEY_JITTER: u64 = 0x6a09_e667_f3bc_c908;
const KEY_PITCH: u64 = 0xbb67_ae85_84ca_a73b;
const KEY_NOISE: u64 = 0x3c6e_f372_fe94_f82b;

#[allow(clippy::too_many_arguments)]
fn render_instance(layer: &Layer, cx: &Ctx, recipe: &Recipe, rate: f64, start: usize, gate: f64, gain: f32, pitch: f64, out: &mut [f32]) {
    let channels = recipe.channels as usize;
    let out_frames = out.len() / channels;
    if start >= out_frames {
        return;
    }
    let mut env = EnvEval::new(layer.env.as_ref(), gate, cx);
    let tail = layer.fx.iter().any(Fx::has_tail);
    let avail = out_frames - start;
    let n = match (tail, env.end()) {
        (false, Some(end)) => ((end * rate).ceil() as usize).min(avail),
        _ => avail,
    };
    let noise_seed = hash32((recipe.seed as u32) ^ (cx.instance.wrapping_mul(0x85EB_CA6B)) ^ ((layer.key ^ KEY_NOISE) as u32) ^ cx.variation.wrapping_mul(0xC2B2_AE35));
    let mut osc = Osc::new(&layer.source, cx, rate, noise_seed);
    let mut freq = layer.source.freq.as_ref().map(|f| Curve::new(f, cx));
    let mut filters: Vec<FilterEval> = layer.filters.iter().map(|f| FilterEval::new(f, cx, rate)).collect();
    let mut fx: Vec<FxState> = layer.fx.iter().map(|f| FxState::new(f, cx, rate, 0)).collect();
    let vib = layer.vibrato.as_ref().map(|v| (cx.v(&v.rate), cx.v(&v.depth), cx.v(&v.delay)));
    let arp = layer.arp.as_ref().map(|a| (a.steps.as_slice(), cx.v(&a.rate)));
    let (gl, gr) = if channels == 2 { dsp::pan_gains(cx.v(&layer.pan) as f32) } else { (1.0, 1.0) };
    let nyquist = rate * 0.49;
    for i in 0..n {
        let t = i as f64 / rate;
        let mut f = match &mut freq {
            Some(c) => c.at(t) * pitch,
            None => 0.0,
        };
        if f > 0.0 && (vib.is_some() || arp.is_some()) {
            let mut st = 0.0;
            if let Some((r, depth, delay)) = vib {
                if t > delay {
                    let fade = ((t - delay) / 0.1).min(1.0);
                    st += depth * fade * dmath::sin_f64(TAU * r * (t - delay));
                }
            }
            if let Some((steps, r)) = arp {
                let k = dmath::floor_f64(t * r) as usize % steps.len();
                st += steps[k];
            }
            f *= dsp::semitones(st);
        }
        let f = f.min(nyquist);
        let mut s = osc.next(f, t);
        s *= env.level(t) as f32;
        for flt in &mut filters {
            if !flt.cutoff.is_const() && i % MOD_INTERVAL == 0 {
                let c = flt.cutoff.at(t);
                flt.set(c, rate);
            }
            s = flt.process(s);
        }
        for e in &mut fx {
            s = e.process(s, t);
        }
        s *= gain;
        let o = start + i;
        if channels == 1 {
            out[o] += s;
        } else {
            out[2 * o] += s * gl;
            out[2 * o + 1] += s * gr;
        }
    }
}

/// Master effects over the whole (interleaved) buffer.
fn master_fx(fx: &[Fx], cx: &Ctx, rate: f64, channels: usize, buf: &mut [f32]) {
    for f in fx {
        if channels == 1 {
            let mut st = FxState::new(f, cx, rate, 0);
            for (i, s) in buf.iter_mut().enumerate() {
                *s = st.process(*s, i as f64 / rate);
            }
            continue;
        }
        if let Fx::Reverb { room, damp, mix, width } = f {
            // Freeverb stereo: two tanks with offset delays, cross-mixed by `width`.
            let p = ReverbParams::new(cx.v(room) as f32, cx.v(damp) as f32);
            let (mix, width) = (cx.v(mix) as f32, cx.v(width) as f32);
            let mut l = ReverbChannel::new(rate, 0);
            let mut r = ReverbChannel::new(rate, dsp::REVERB_STEREO_SPREAD);
            let wet1 = 3.0 * mix * (width / 2.0 + 0.5);
            let wet2 = 3.0 * mix * ((1.0 - width) / 2.0);
            for fr in buf.chunks_exact_mut(2) {
                let input = 0.5 * (fr[0] + fr[1]);
                let (ol, or) = (l.process(input, p.feedback, p.damp), r.process(input, p.feedback, p.damp));
                fr[0] = fr[0] * (1.0 - mix) + ol * wet1 + or * wet2;
                fr[1] = fr[1] * (1.0 - mix) + or * wet1 + ol * wet2;
            }
            continue;
        }
        let mut sl = FxState::new(f, cx, rate, 0);
        let mut sr = FxState::new(f, cx, rate, 0);
        for (i, fr) in buf.chunks_exact_mut(2).enumerate() {
            let t = i as f64 / rate;
            fr[0] = sl.process(fr[0], t);
            fr[1] = sr.process(fr[1], t);
        }
    }
}

/// Renders a recipe.
pub fn render(recipe: &Recipe, opts: &RenderOptions) -> Result<Rendered> {
    if !(8_000..=192_000).contains(&opts.rate) {
        return Err(AudioError::at(ErrorCode::OutOfRange, "rate", "render rate must be 8000 ..= 192000 Hz").with_numbers(opts.rate as f64, 192_000.0));
    }
    let work = recipe.work_estimate(opts.rate);
    if work > MAX_WORK * (opts.rate as f64 / 48_000.0).max(1.0) {
        return Err(AudioError::at(ErrorCode::Limit, "layers", "recipe is too expensive to render at this rate").with_numbers(work, MAX_WORK));
    }
    let rate = opts.rate as f64;
    let channels = recipe.channels as usize;
    let total = ((recipe.duration * rate).round() as usize).max(1);
    let xfade = recipe.loop_crossfade.map_or(0, |x| ((x * rate).round() as usize).clamp(1, total / 2));
    let frames = total + xfade;
    let mut buf = vec![0f32; frames * channels];
    let base = Ctx { seed: recipe.seed, variation: opts.variation, instance: 0 };
    let vary = Ctx { instance: u32::MAX, ..base };
    let (mut vary_st, mut vary_db) = (0.0, 0.0);
    if opts.variation > 0 {
        if let Some((lo, hi)) = recipe.vary_pitch {
            vary_st = lo + (hi - lo) * vary.draw(hash_str("vary.pitch"));
        }
        if let Some((lo, hi)) = recipe.vary_gain_db {
            vary_db = lo + (hi - lo) * vary.draw(hash_str("vary.gain_db"));
        }
    }
    let vary_ratio = dsp::semitones(vary_st);
    for layer in &recipe.layers {
        let count = layer.repeat.as_ref().map_or(1, |r| r.count);
        for k in 0..count {
            let cx = Ctx { instance: k, ..base };
            let mut start_s = cx.v(&layer.at);
            let mut gain_db = cx.v(&layer.gain_db);
            let mut pitch = vary_ratio;
            if let Some(r) = &layer.repeat {
                start_s += k as f64 * cx.v(&r.interval) + cx.v(&r.jitter) * cx.signed(layer.key ^ KEY_JITTER);
                gain_db += k as f64 * cx.v(&r.gain_db_step);
                pitch *= dsp::semitones(cx.v(&r.pitch_jitter) * cx.signed(layer.key ^ KEY_PITCH));
            }
            let start_s = start_s.max(0.0);
            let start = (start_s * rate).round() as usize;
            let gate = match &layer.length {
                Some(l) => cx.v(l),
                None => (recipe.duration - start_s).max(0.0),
            };
            let gain = dsp::db_to_gain(gain_db) as f32;
            render_instance(layer, &cx, recipe, rate, start, gate, gain, pitch, &mut buf);
        }
    }
    master_fx(&recipe.fx, &base, rate, channels, &mut buf);

    // Seamless loop: fold the tail past `total` onto the start with an equal-power crossfade.
    let mut samples = buf;
    if xfade > 0 {
        for i in 0..xfade {
            let x = (i as f64 + 0.5) / xfade as f64;
            let (fin, fout) = dmath::sin_cos_f64(x * PI / 2.0);
            for c in 0..channels {
                let head = samples[i * channels + c];
                let tail = samples[(total + i) * channels + c];
                samples[i * channels + c] = (head as f64 * fin + tail as f64 * fout) as f32;
            }
        }
        samples.truncate(total * channels);
    }

    let gain = dsp::db_to_gain(base.v(&recipe.gain_db) + vary_db) as f32;
    if gain != 1.0 {
        samples.iter_mut().for_each(|s| *s *= gain);
    }
    if samples.iter().any(|s| !s.is_finite()) {
        return Err(AudioError::at(ErrorCode::OutOfRange, "", "the render produced non-finite samples"));
    }
    let mut normalize_db = 0.0;
    if let Some(target) = recipe.loudness {
        let measured = analysis::loudness_lufs(&samples, channels, opts.rate);
        if measured.is_finite() {
            normalize_db = target - measured;
            let g = dsp::db_to_gain(normalize_db) as f32;
            samples.iter_mut().for_each(|s| *s *= g);
        }
    }
    let mut ceiling_db = 0.0;
    let ceiling = dsp::db_to_gain(recipe.peak_ceiling_db) as f32;
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > ceiling {
        let g = ceiling / peak;
        ceiling_db = dsp::gain_to_db(g as f64);
        samples.iter_mut().for_each(|s| *s *= g);
    }
    let mut h = StableHasher::new();
    for s in &samples {
        h.write_f32(*s);
    }
    Ok(Rendered {
        name: recipe.name.clone(),
        bus: recipe.bus,
        rate: opts.rate,
        channels: recipe.channels,
        hash: h.finish(),
        samples,
        looping: recipe.loop_crossfade.is_some(),
        normalize_db,
        ceiling_db,
    })
}
