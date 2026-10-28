//! Photosensitivity, per effect: how often an effect would flash on its own (WCAG 2.3.1, the
//! general and red flash thresholds: no more than three flashes in any one second). Effects that
//! would flash faster are refused when parsed (`photosensitive`), like strobing lights. This is
//! the early warning agents see first; the engine's flash watch on the final image
//! (`specula::flashwatch`) is the check that covers what one effect can't know: several
//! instances, an entity's scale, a close camera, game code.
//!
//! **The model**, at a reference view (3 m away, 60° field, 720 lines):
//!
//! - A particle's area is its quad (`size² × E[random²]`, `params.size²`) times its texture's
//!   coverage (alpha × luminance, frame by frame for built-in flipbooks). Colour is its gradient
//!   times the tint, per channel, times emissive and intensity, in linear light.
//! - Three signals, summed over everything the effect plays at once: **brightening** (area ×
//!   alpha × luminance capped at white: over a dark background), **darkening** (area × alpha ×
//!   (1 − luminance), for the alpha-blended share: over a bright background) and **red** (the
//!   area × alpha of saturated red, `R / (R + G + B) ≥ 0.8` with a red value `(R − G − B) × 320`
//!   of at least 20). The light adds its range's area (capped at the view) × 15% (what lit
//!   surfaces reflect) × its level.
//! - Every burst cycle at its time, every rate emitter's births at regular `i / rate` times (the
//!   worst phase: they all line up), and the light's flicker, are summed into one timeline of
//!   each signal. Random lifetimes are mixed (half at the shortest, half at the longest).
//! - The timeline is sampled 120 times a second in windows around every onset (each emitter's
//!   start, each burst's first and last cycle) and in the steady state after the longest
//!   lifetime: late strobes are seen. A transition is a change of half the threshold's
//!   significant area at full brightness (a quarter of the 10° central field: smaller regions
//!   don't qualify) and a tenth of the larger side (light or shade already there masks a small
//!   change). A flash is a pair of transitions; more than three in any second fails.
//! - Each particle's own life is checked too (curves and built-in flipbook frames), and a
//!   flipbook's frame sequence at its frame rate.
//!
//! **Not covered here** (the flash watch covers them on the final image): several instances
//! together, entity scale, a camera closer than 3 m, flipbooks over asset PNGs (their frames are
//! loaded later: `flash_unchecked`), random sizes beyond their mean square, and what is behind the
//! effect. Births are taken as regular, the worst case for streams.
//!
//! **Bounded:** any legal effect costs at most a few thousand particle evaluations and a few
//! million additions ([`Flash::work`]); about a millisecond.

use crate::component::Params;
use crate::effect::{Effect, Emitter, Flipbook, TextureRef, MAX_FLASH_HZ};
use crate::textures;
use specula_core::dmath;
use std::sync::OnceLock;

/// The reference view (a close one: flashes matter most up close).
const REF_DISTANCE: f32 = 3.0;
const REF_FOV_DEG: f32 = 60.0;
const REF_HEIGHT: f32 = 720.0;
const REF_WIDTH: f32 = 1280.0;
/// Samples per second of the timelines (flashes up to 60 a second resolve).
const SAMPLE_HZ: f64 = 120.0;
/// Seconds of each window (a full second plus margin).
const WINDOW_S: f64 = 1.25;
/// Flipbook frame changes followed over one particle's life at most (120 a second for 10 s).
const MAX_FRAME_STEPS: u32 = 1200;
/// Overlapping cycles summed one by one at most (more overlap: the folded steady state).
const DIRECT: u64 = 8;

/// What [`flash`] found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flash {
    /// Flashes in the worst one-second window.
    pub hz: f32,
    /// The emitter responsible, when one is (`None`: several together, or the light).
    pub emitter: Option<usize>,
    /// "combined" (the summed timeline), "particle" (one particle's life), "flipbook" or "none".
    pub cause: &'static str,
    /// "brightening", "darkening" or "red".
    pub signal: &'static str,
    /// Emitters whose flipbook frames couldn't be checked (asset PNGs animating faster than 3 Hz).
    pub unchecked: Vec<usize>,
    /// Work done: particle evaluations and timeline additions.
    pub work: u64,
}

fn px_per_m() -> f32 {
    REF_HEIGHT / (2.0 * REF_DISTANCE * dmath::tan((REF_FOV_DEG * 0.5).to_radians()))
}

/// The smallest change of light that counts (pixels × relative luminance): half of a quarter of
/// the 10° central field at full brightness.
pub fn significant_change() -> f32 {
    let field = 10.0 / REF_FOV_DEG * REF_HEIGHT;
    0.5 * 0.25 * field * field
}

/// Mean coverage (alpha × luminance, 0..1) of each frame of a built-in texture laid out
/// `cols × rows`; `None` for an asset texture.
fn frame_levels(texture: &TextureRef, cols: u32, rows: u32, frames: u32) -> Option<Vec<f32>> {
    let TextureRef::Builtin(name) = texture else { return None };
    static IMAGES: OnceLock<Vec<(&'static str, textures::Image)>> = OnceLock::new();
    let images = IMAGES.get_or_init(|| textures::PARTICLE_NAMES.iter().filter_map(|n| textures::builtin(n).map(|i| (*n, i))).collect());
    let img = &images.iter().find(|(n, _)| n == name)?.1;
    let (cw, ch) = (img.width / cols.max(1), img.height / rows.max(1));
    let mut levels = Vec::with_capacity(frames as usize);
    for f in 0..frames {
        let (cx, cy) = (f % cols.max(1), f / cols.max(1));
        let mut sum = 0.0f64;
        for y in cy * ch..(cy + 1) * ch {
            for x in cx * cw..(cx + 1) * cw {
                let i = ((y * img.width + x) * 4) as usize;
                let p = &img.pixels[i..i + 4];
                let luma = 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
                sum += luma * f64::from(p[3]) / (255.0 * 255.0);
            }
        }
        levels.push((sum / f64::from(cw * ch).max(1.0)) as f32);
    }
    Some(levels)
}

/// The three signals of one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Light3 {
    bright: f32,
    dark: f32,
    red: f32,
}

impl std::ops::AddAssign for Light3 {
    fn add_assign(&mut self, o: Light3) {
        self.bright += o.bright;
        self.dark += o.dark;
        self.red += o.red;
    }
}

impl std::ops::Add for Light3 {
    type Output = Light3;
    fn add(mut self, o: Light3) -> Light3 {
        self += o;
        self
    }
}

impl std::ops::Mul<f32> for Light3 {
    type Output = Light3;
    fn mul(self, k: f32) -> Light3 {
        Light3 { bright: self.bright * k, dark: self.dark * k, red: self.red * k }
    }
}

/// One emitter, prepared: its frame levels and the scale of its light.
struct Prep<'a> {
    e: &'a Emitter,
    params: &'a Params,
    levels: Option<Vec<f32>>,
    tint: [f32; 3],
    /// Mean of the random size factor squared.
    r2: f32,
}

impl Prep<'_> {
    /// The signals of one particle living `life` seconds, `age` seconds old.
    fn light(&self, age: f32, life: f32) -> Light3 {
        if !(0.0..life).contains(&age) {
            return Light3::default();
        }
        let (base, frame) = self.base(age, life);
        base * self.coverage(frame)
    }

    /// The texture's coverage of flipbook frame `frame` (an asset PNG's is unknown: full).
    fn coverage(&self, frame: Option<u32>) -> f32 {
        match (frame, &self.levels) {
            (Some(f), Some(levels)) if !levels.is_empty() => levels[f as usize % levels.len()],
            (None, Some(levels)) if !levels.is_empty() => levels[0],
            _ => 1.0,
        }
    }

    /// The signals at full coverage, and the flipbook frame showing.
    fn base(&self, age: f32, life: f32) -> (Light3, Option<u32>) {
        let e = self.e;
        let u = (age / life).clamp(0.0, 1.0);
        let size = e.size.eval(u) * self.params.size * px_per_m();
        let frame = e.flipbook.as_ref().map(|fb| frame_at(fb, u, age));
        let area = size * size * self.r2;
        let g = e.emissive * self.params.intensity;
        let c = e.color.eval(u);
        let c = [c[0] * self.tint[0] * g, c[1] * self.tint[1] * g, c[2] * self.tint[2] * g];
        let luma = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).min(1.0);
        let alpha = (e.alpha.eval(u) * self.params.alpha).clamp(0.0, 1.0);
        let (r, gg, b) = (c[0].min(1.0), c[1].min(1.0), c[2].min(1.0));
        let sum = r + gg + b;
        // Saturated red with a red value of at least 20 (`(R − G − B) × 320`): its area counts.
        let red = if sum > 0.0 && r / sum >= 0.8 { (((r - gg - b) * 320.0) / 20.0).clamp(0.0, 1.0) } else { 0.0 };
        (Light3 { bright: area * alpha * luma, dark: area * alpha * (1.0 - e.additive) * (1.0 - luma), red: area * alpha * red }, frame)
    }

    /// One particle's light over its life, exactly: at every key of its curves (alpha, colour,
    /// size: piecewise linear) and the midpoints between them, and on both sides of every
    /// flipbook frame change (frames are steps). Nothing between can flicker unseen.
    fn pulse(&self, life: f32, work: &mut u64) -> Vec<(f64, Light3)> {
        let e = self.e;
        let mut keys: Vec<f32> = vec![0.0, life];
        keys.extend(e.alpha.keys.iter().chain(e.size.keys.iter()).map(|k| k[0] * life));
        keys.extend(e.color.keys.iter().map(|k| k.0 * life));
        keys.retain(|t| (0.0..=life).contains(t));
        keys.sort_by(f32::total_cmp);
        keys.dedup();
        let mut times: Vec<f32> = keys.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect();
        times.extend_from_slice(&keys);
        // Frame changes: the frame just before and from each boundary.
        let mut steps: Vec<f32> = Vec::new();
        if let (Some(fb), Some(levels)) = (&e.flipbook, &self.levels) {
            if levels.len() > 1 {
                let n = fb.frames.max(1);
                if fb.over_life {
                    steps.extend((1..n).map(|k| k as f32 / n as f32 * life));
                } else if fb.fps > 0.0 {
                    let count = (dmath::floor(life * fb.fps) as u32).min(MAX_FRAME_STEPS);
                    steps.extend((1..=count).map(|k| k as f32 / fb.fps).filter(|t| *t < life));
                }
            }
        }
        times.retain(|t| *t < life);
        let mut points: Vec<(f64, Light3)> = Vec::with_capacity(times.len() + 2 * steps.len() + 1);
        for &t in &times {
            points.push((f64::from(t), self.light(t, life)));
        }
        for &t in &steps {
            let (base, after) = self.base(t, life);
            let before = e.flipbook.as_ref().map(|fb| frame_at(fb, ((t - 1e-4) / life).max(0.0), (t - 1e-4).max(0.0)));
            points.push((f64::from(t) - 1e-4, base * self.coverage(before)));
            points.push((f64::from(t), base * self.coverage(after)));
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        points.dedup_by(|a, b| a.0 == b.0);
        *work += (times.len() + steps.len()) as u64;
        points
    }
}

fn frame_at(fb: &Flipbook, u: f32, age: f32) -> u32 {
    let n = fb.frames.max(1);
    if fb.over_life {
        ((u * n as f32) as u32).min(n - 1)
    } else {
        (dmath::floor(age * fb.fps).max(0.0) as u32) % n
    }
}

/// Whether going from `a` to `b` is a transition of a signal (`relative`: the brighter side's
/// tenth too).
fn counts(a: f32, b: f32, swing: f32, relative: bool) -> bool {
    let need = if relative { swing.max(0.1 * a.max(b)) } else { swing };
    (a - b).abs() >= need
}

/// [`changes`] at explicit sample times.
fn changes_at(signal: &[f32], times: &[f64], swing: f32, out: &mut Vec<f64>) {
    let mut found = Vec::new();
    changes(signal, 0.0, 1.0, swing, true, &mut found);
    // `changes` reports sample indices as times (step 1 from 0): map them to the real times.
    out.extend(found.iter().map(|&i| times[(i as usize).min(times.len() - 1)]));
}

/// Times of the transitions in `signal` (sampled at `step` seconds from `t0`).
fn changes(signal: &[f32], t0: f64, step: f64, swing: f32, relative: bool, out: &mut Vec<f64>) {
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    let mut dir = 0i8;
    let mut ext = 0.0f32;
    for (i, &v) in signal.iter().enumerate() {
        let t = t0 + i as f64 * step;
        match dir {
            0 => {
                lo = lo.min(v);
                hi = hi.max(v);
                if v > lo && counts(lo, v, swing, relative) {
                    (dir, ext) = (1, v);
                    out.push(t);
                } else if v < hi && counts(hi, v, swing, relative) {
                    (dir, ext) = (-1, v);
                    out.push(t);
                }
            }
            1 => {
                if v > ext {
                    ext = v;
                } else if counts(ext, v, swing, relative) {
                    (dir, ext) = (-1, v);
                    out.push(t);
                }
            }
            _ => {
                if v < ext {
                    ext = v;
                } else if counts(ext, v, swing, relative) {
                    (dir, ext) = (1, v);
                    out.push(t);
                }
            }
        }
    }
}

/// Flashes (pairs of transitions) in the worst one-second window.
fn worst_second(times: &[f64]) -> f32 {
    let mut best = 0usize;
    let mut j = 0;
    for i in 0..times.len() {
        while times[i] - times[j] >= 1.0 {
            j += 1;
        }
        best = best.max(i + 1 - j);
    }
    (best / 2) as f32
}

/// A train of identical pulses: a burst's cycles or a rate emitter's births.
struct Train {
    emitter: usize,
    onset: f64,
    /// Seconds between pulses (`f64::INFINITY` for one).
    period: f64,
    /// Pulses (`u64::MAX`: forever).
    cycles: u64,
    count: f32,
}

impl Train {
    fn last(&self) -> f64 {
        if self.cycles == u64::MAX {
            f64::INFINITY
        } else if self.cycles <= 1 {
            self.onset
        } else {
            self.onset + self.period * (self.cycles - 1) as f64
        }
    }
}

/// One emitter's pulse: its particle's signals by age (shortest and longest life mixed), as
/// exact points with linear interpolation between them.
struct Pulse {
    points: Vec<(f64, Light3)>,
    life: f64,
}

impl Pulse {
    fn mix(a: &[(f64, Light3)], b: Option<&[(f64, Light3)]>) -> Pulse {
        let at = |pts: &[(f64, Light3)], t: f64| interpolate(pts, t);
        let Some(b) = b else {
            return Pulse { life: a.last().map_or(0.0, |p| p.0), points: a.to_vec() };
        };
        let mut times: Vec<f64> = a.iter().chain(b.iter()).map(|p| p.0).collect();
        times.sort_by(f64::total_cmp);
        times.dedup();
        let points = times.iter().map(|&t| (t, (at(a, t) + at(b, t)) * 0.5)).collect();
        Pulse { life: times.last().copied().unwrap_or(0.0), points }
    }

    fn at(&self, age: f64) -> Light3 {
        if age < 0.0 || age > self.life + 1e-3 {
            return Light3::default();
        }
        interpolate(&self.points, age)
    }
}

/// `pts` (sorted by time) at `t`, linearly interpolated; nothing outside them.
fn interpolate(pts: &[(f64, Light3)], t: f64) -> Light3 {
    let i = pts.partition_point(|p| p.0 <= t);
    match (i.checked_sub(1).and_then(|k| pts.get(k)), pts.get(i)) {
        (Some(a), Some(b)) if b.0 > a.0 => {
            let f = ((t - a.0) / (b.0 - a.0)) as f32;
            a.1 * (1.0 - f) + b.1 * f
        }
        (Some(a), None) if t - a.0 < 1e-3 => a.1,
        (Some(a), Some(_)) => a.1,
        _ => Light3::default(),
    }
}

/// A train's steady state folded onto one period (`buckets` of it).
struct Folded {
    buckets: Vec<Light3>,
}

impl Folded {
    /// The sum of every overlapping pulse at each of `m` phases of the period (`m` samples a
    /// period at 120 a second; a period shorter than a sample is its average).
    fn new(p: &Pulse, period: f64, work: &mut u64) -> Folded {
        let m = (dmath::round_f64(period * SAMPLE_HZ) as usize).clamp(1, 4096);
        let life = p.life;
        let mut buckets = vec![Light3::default(); m];
        if m == 1 {
            // Pulses closer than a sample: the steady level (the pulse's integral per period).
            let mut total = Light3::default();
            for w in p.points.windows(2) {
                total += (w[0].1 + w[1].1) * (0.5 * (w[1].0 - w[0].0)) as f32;
            }
            buckets[0] = total * (1.0 / period) as f32;
            *work += p.points.len() as u64;
            return Folded { buckets };
        }
        for (b, v) in buckets.iter_mut().enumerate() {
            let mut age = b as f64 / m as f64 * period;
            while age < life {
                *v += p.at(age);
                age += period;
                *work += 1;
            }
        }
        Folded { buckets }
    }

    fn at(&self, phase_frac: f64) -> Light3 {
        let m = self.buckets.len();
        self.buckets[(dmath::round_f64(phase_frac * m as f64) as usize) % m]
    }
}

/// The train's contribution at `t`.
fn train_at(tr: &Train, pulse: &Pulse, folded: Option<&Folded>, life: f64, t: f64, work: &mut u64) -> Light3 {
    if t < tr.onset || t > tr.last() + life {
        return Light3::default();
    }
    if tr.period.is_infinite() || tr.cycles <= 1 {
        *work += 1;
        return pulse.at(t - tr.onset) * tr.count;
    }
    let since = t - tr.onset;
    let fired = (since / tr.period) as u64 + 1;
    let fired = fired.min(tr.cycles);
    let alive = ((life / tr.period) as u64 + 1).min(fired);
    if alive <= DIRECT {
        let mut v = Light3::default();
        for k in (fired - alive)..fired {
            v += pulse.at(t - (tr.onset + k as f64 * tr.period));
        }
        *work += alive;
        return v * tr.count;
    }
    // Many overlapping: the folded steady state (a ramp or a tail is no strobe).
    *work += 1;
    match folded {
        Some(f) => f.at((since % tr.period) / tr.period) * tr.count,
        None => Light3::default(),
    }
}

fn worst(hz: f32, emitter: Option<usize>, cause: &'static str, signal: &'static str, out: &mut Flash) {
    if hz > out.hz {
        (out.hz, out.emitter, out.cause, out.signal) = (hz, emitter, cause, signal);
    }
}

/// Scratch for judging timelines.
#[derive(Default)]
struct Judge {
    col: Vec<f32>,
    times: Vec<f64>,
}

impl Judge {
    /// Each signal at explicit times (sorted), as flashes, into `out`.
    fn points(&mut self, series: &[Light3], times: &[f64], emitter: Option<usize>, cause: &'static str, out: &mut Flash) {
        let swing = significant_change();
        for (k, signal) in [(0, "brightening"), (1, "darkening"), (2, "red")] {
            self.col.clear();
            self.col.extend(series.iter().map(|l| match k {
                0 => l.bright,
                1 => l.dark,
                _ => l.red,
            }));
            self.times.clear();
            changes_at(&self.col, times, swing, &mut self.times);
            worst(worst_second(&self.times), emitter, cause, signal, out);
        }
    }

    /// Each signal of a sampled timeline, as flashes, into `out`.
    fn series(&mut self, series: &[Light3], t0: f64, step: f64, emitter: Option<usize>, cause: &'static str, out: &mut Flash) {
        let swing = significant_change();
        for (k, signal, s, relative) in [(0, "brightening", swing, true), (1, "darkening", swing, true), (2, "red", swing, true)] {
            self.col.clear();
            self.col.extend(series.iter().map(|l| match k {
                0 => l.bright,
                1 => l.dark,
                _ => l.red,
            }));
            self.times.clear();
            changes(&self.col, t0, step, s, relative, &mut self.times);
            worst(worst_second(&self.times), emitter, cause, signal, out);
        }
    }
}

impl Flash {
    /// What flashes, in words: the signal and where it comes from.
    pub fn describe(&self) -> String {
        let what = match self.cause {
            "combined" => "the effect's bursts, streams and light together",
            "particle" => "each particle's own curves or flipbook",
            "flipbook" => "a flipbook's frames",
            _ => "nothing",
        };
        format!("{} flashes about {:.0} times a second from {what}", self.signal, self.hz)
    }
}

/// How fast `effect`, played with `params`, flashes (see the module docs).
pub fn flash(effect: &Effect, params: &Params) -> Flash {
    let tint = params.tint.map(crate::curve::srgb_to_linear);
    let mut out = Flash { cause: "none", signal: "", ..Flash::default() };
    let mut work = 0u64;
    let mut judge = Judge::default();
    let preps: Vec<Prep> = effect
        .emitters
        .iter()
        .map(|e| {
            let levels = match &e.flipbook {
                Some(fb) => frame_levels(&e.texture, fb.cols, fb.rows, fb.frames),
                None => frame_levels(&e.texture, 1, 1, 1),
            };
            let r = e.size_random;
            Prep { e, params, levels, tint, r2: (r[0] * r[0] + r[0] * r[1] + r[1] * r[1]) / 3.0 }
        })
        .collect();
    // Each emitter's pulse; and each particle's own life, padded with nothing either side.
    let mut pulses = Vec::with_capacity(preps.len());
    for (k, p) in preps.iter().enumerate() {
        let e = p.e;
        if let (Some(fb), TextureRef::Asset(_)) = (&e.flipbook, &e.texture) {
            let fps = if fb.over_life { fb.frames as f32 / e.lifetime[0] } else { fb.fps };
            if fb.frames > 1 && fps > MAX_FLASH_HZ {
                out.unchecked.push(k);
            }
        }
        let lives = if e.lifetime[0] == e.lifetime[1] { vec![e.lifetime[0]] } else { vec![e.lifetime[0], e.lifetime[1]] };
        let mut own_points = Vec::with_capacity(lives.len());
        for &life in &lives {
            let pts = p.pulse(life, &mut work);
            // Each particle's own life, padded with nothing either side.
            let mut times = Vec::with_capacity(pts.len() + 2);
            let mut own = Vec::with_capacity(pts.len() + 2);
            times.push(-1e-3);
            own.push(Light3::default());
            for (t, l) in &pts {
                times.push(*t);
                own.push(*l);
            }
            times.push(f64::from(life) + 1e-3);
            own.push(Light3::default());
            judge.points(&own, &times, Some(k), "particle", &mut out);
            own_points.push(pts);
        }
        pulses.push(Pulse::mix(&own_points[0], own_points.get(1).map(Vec::as_slice)));
    }
    // Every train: bursts at their cycles, rate emitters' births at regular times.
    let mut trains = Vec::new();
    for (k, e) in effect.emitters.iter().enumerate() {
        let rate = f64::from(e.rate * params.rate);
        if rate > 0.0 {
            let cycles = e.duration.map_or(u64::MAX, |d| dmath::ceil_f64(f64::from(d) * rate).max(1.0) as u64);
            trains.push(Train { emitter: k, onset: f64::from(e.delay), period: 1.0 / rate, cycles, count: 1.0 });
        }
        for b in &e.bursts {
            let count = dmath::round(b.count as f32 * params.rate);
            if count <= 0.0 {
                continue;
            }
            let (period, cycles) = if b.cycles == 1 || b.interval <= 0.0 { (f64::INFINITY, 1) } else { (f64::from(b.interval), if b.cycles == 0 { u64::MAX } else { u64::from(b.cycles) }) };
            trains.push(Train { emitter: k, onset: f64::from(e.delay + b.time), period, cycles, count });
        }
    }
    let folded: Vec<Option<Folded>> = trains.iter().map(|tr| tr.period.is_finite().then(|| Folded::new(&pulses[tr.emitter], tr.period, &mut work))).collect();
    let lives: Vec<f64> = effect.emitters.iter().map(|e| f64::from(e.lifetime[1])).collect();
    let max_life = lives.iter().copied().fold(0.0, f64::max);
    // The light's own area and brightness at the reference view.
    let light = effect.light.filter(|_| params.light).map(|l| {
        let c = l.color.map(crate::curve::srgb_to_linear);
        let luma = 0.2126 * c[0] * tint[0] + 0.7152 * c[1] * tint[1] + 0.0722 * c[2] * tint[2];
        let reach = l.range * px_per_m();
        let area = (std::f32::consts::PI * reach * reach).min(REF_WIDTH * REF_HEIGHT);
        // Lit surfaces reflect about 15% (typical albedo).
        (l, area * 0.15 * (luma * l.intensity * params.intensity).min(1.0))
    });
    // Windows: around every onset and end, and the steady state.
    let mut starts: Vec<f64> = vec![0.0, max_life];
    for tr in &trains {
        starts.push(tr.onset);
        if tr.last().is_finite() {
            starts.push((tr.last() - 0.5).max(tr.onset));
        } else {
            starts.push(tr.onset + max_life);
        }
    }
    starts.sort_by(f64::total_cmp);
    starts.dedup_by(|a, b| *a - *b < WINDOW_S * 0.5);
    let n = (WINDOW_S * SAMPLE_HZ) as usize;
    let mut series = vec![Light3::default(); n];
    let one = effect.emitters.len() == 1 && light.is_none();
    for &w0 in &starts {
        series.iter_mut().for_each(|s| *s = Light3::default());
        for (tr, f) in trains.iter().zip(&folded) {
            let life = lives[tr.emitter];
            if w0 + WINDOW_S < tr.onset || w0 > tr.last() + life {
                continue;
            }
            for (i, s) in series.iter_mut().enumerate() {
                *s += train_at(tr, &pulses[tr.emitter], f.as_ref(), life, w0 + i as f64 / SAMPLE_HZ, &mut work);
            }
        }
        if let Some((l, area)) = light {
            for (i, s) in series.iter_mut().enumerate() {
                s.bright += area * l.level(w0 + i as f64 / SAMPLE_HZ, 0);
            }
            work += n as u64;
        }
        judge.series(&series, w0, 1.0 / SAMPLE_HZ, if one { Some(0) } else { None }, "combined", &mut out);
    }
    out.work = work;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_core::Json;

    fn hz(text: &str) -> Flash {
        hz_with(text, Params::default())
    }

    fn hz_with(text: &str, params: Params) -> Flash {
        let v = Json::parse(text).unwrap();
        // Parsed without the flash refusal, to measure.
        flash(&Effect::parse_unchecked(&v).unwrap(), &params)
    }

    const STROBE: &str = r##""lifetime": 0.05, "size": 2, "emissive": 6, "color": "#ffffff", "alpha": [[0, 1], [1, 1]]"##;

    #[test]
    fn strobes_are_found_and_steady_effects_pass() {
        // Bursts every 0.1 s of short bright particles: 10 Hz.
        let s = hz(&format!(r#"{{"emitters": [{{"bursts": [{{"count": 1, "cycles": 0, "interval": 0.1}}], {STROBE}}}]}}"#));
        assert!((9.0..=11.0).contains(&s.hz) && s.cause == "combined", "{s:?}");
        // The same, small and dim: no significant change.
        let s = hz(r##"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.1}], "lifetime": 0.05, "size": 0.02, "color": "#202020"}]}"##);
        assert!(s.hz <= 3.0, "{s:?}");
        // A big particle whose alpha flickers 8 times a second over its life.
        let keys: Vec<String> = (0..16).map(|i| format!("[{}, {}]", i as f32 / 15.0, i % 2)).collect();
        let s = hz(&format!(r##"{{"emitters": [{{"rate": 0.5, "lifetime": 1, "size": 2, "color": "#ffffff", "alpha": [{}]}}]}}"##, keys.join(", ")));
        assert!(s.hz >= 6.0, "{s:?}");
        // A stream of big particles at full brightness, 10 a second.
        let s = hz(&format!(r#"{{"emitters": [{{"rate": 10, {STROBE}}}]}}"#));
        assert!(s.hz >= 9.0, "{s:?}");
        // A finite run of 5 cycles 0.1 s apart is 5 flashes in half a second.
        let s = hz(r##"{"emitters": [{"bursts": [{"count": 1, "cycles": 5, "interval": 0.1}], "lifetime": 0.05, "size": 2, "color": "#ffffff"}]}"##);
        assert!(s.hz >= 4.0, "{s:?}");
    }

    /// The review's bypasses (round 2), each now found.
    #[test]
    fn late_dark_red_tinted_summed_and_combined_strobes_are_found() {
        // A strobe starting after a delay, or from a later burst.
        let s = hz(&format!(r#"{{"emitters": [{{"bursts": [{{"count": 1}}], "lifetime": 0.05, "size": 0.01}}, {{"delay": 20, "bursts": [{{"count": 1, "cycles": 0, "interval": 0.1}}], {STROBE}}}]}}"#));
        assert!(s.hz >= 9.0, "delayed emitter: {s:?}");
        let s = hz(&format!(r#"{{"emitters": [{{"bursts": [{{"time": 0, "count": 1}}, {{"time": 15, "count": 1, "cycles": 0, "interval": 0.1}}], {STROBE}}}]}}"#));
        assert!(s.hz >= 9.0, "late burst: {s:?}");
        // Darkening: black alpha-blended particles blinking (over a bright sky).
        let s = hz(r##"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.1}], "lifetime": 0.05, "size": 3, "color": "#000000", "blend": "alpha", "alpha": [[0, 1], [1, 1]]}]}"##);
        assert!(s.hz >= 9.0 && s.signal == "darkening", "dark bursts: {s:?}");
        // Red against grey at about equal luminance.
        let ck: Vec<String> = (0..16).map(|i| format!("[{}, \"{}\"]", i as f32 / 15.0, if i % 2 == 0 { "#ff0000" } else { "#7f7f7f" })).collect();
        let s = hz(&format!(r##"{{"emitters": [{{"rate": 0.5, "lifetime": 1, "size": 3, "color": [{}], "alpha": [[0, 1], [1, 1]]}}]}}"##, ck.join(", ")));
        assert!(s.hz >= 6.0, "red flash: {s:?}");
        // A red tint on red particles counts per channel (not luma × luma).
        let red = r##"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.1}], "lifetime": 0.05, "size": 0.5, "emissive": 1, "color": "#ff0000", "alpha": [[0, 1], [1, 1]]}]}"##;
        let s = hz_with(red, Params { intensity: 4.0, tint: [1.0, 0.0, 0.0], ..Params::default() });
        assert!(s.hz >= 9.0, "red tint: {s:?}");
        // Two rate streams of 3 a second, offset: 6 flashes. A rate stream and bursts, offset.
        let s = hz(&format!(r#"{{"emitters": [{{"rate": 3, {STROBE}}}, {{"delay": 0.1667, "rate": 3, {STROBE}}}]}}"#));
        assert!(s.hz > 3.0, "two streams: {s:?}");
        let s = hz(&format!(r#"{{"emitters": [{{"rate": 3, {STROBE}}}, {{"delay": 0.1667, "bursts": [{{"count": 1, "cycles": 0, "interval": 0.3333}}], {STROBE}}}]}}"#));
        assert!(s.hz > 3.0, "stream and bursts: {s:?}");
        // Two emitters bursting 2.5 a second out of phase: 5.
        let s = hz(r##"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.4}], "lifetime": 0.05, "size": 2, "emissive": 6, "color": "#ffffff"}, {"delay": 0.2, "bursts": [{"count": 1, "cycles": 0, "interval": 0.4}], "lifetime": 0.05, "size": 2, "emissive": 6, "color": "#ffffff"}]}"##);
        assert!(s.hz > 3.0, "two emitters in antiphase: {s:?}");
        // A light flickering 3 a second plus bursts 2.5 a second: summed, not compared.
        let light = r##""light": {"color": "#ffffff", "intensity": 20, "range": 10, "flicker": 1, "flicker_hz": 3}"##;
        let both = hz(&format!(r#"{{{light}, "emitters": [{{"bursts": [{{"count": 1, "cycles": 0, "interval": 0.4}}], {STROBE}}}]}}"#));
        let alone = hz(&format!(r#"{{{light}, "emitters": [{{"bursts": [{{"count": 1}}], "lifetime": 0.05, "size": 0.01}}]}}"#));
        let bursts = hz(&format!(r#"{{"emitters": [{{"bursts": [{{"count": 1, "cycles": 0, "interval": 0.4}}], {STROBE}}}]}}"#));
        assert!(both.hz >= alone.hz.max(bursts.hz) && both.work > bursts.work, "light plus bursts: {both:?} (light {alone:?}, bursts {bursts:?})");
        // Random lifetimes are mixed in, not averaged away.
        let s = hz(r##"{"emitters": [{"bursts": [{"count": 4, "cycles": 0, "interval": 0.2}], "lifetime": [0.02, 0.4], "size": 1, "emissive": 6, "color": "#ffffff", "alpha": [[0, 1], [1, 1]]}]}"##);
        assert!(s.hz >= 4.0, "random lifetimes: {s:?}");
    }

    #[test]
    fn every_built_in_effect_is_below_the_threshold() {
        for (name, text) in crate::library::BUILTIN {
            let e = Effect::parse_unchecked(&Json::parse(text).unwrap()).unwrap();
            let f = flash(&e, &Params::default());
            assert!(f.hz <= 3.0, "{name}: {f:?}");
            assert!(f.unchecked.is_empty(), "{name}: built-in flipbooks are checked");
        }
        let sheet = frame_levels(&TextureRef::Builtin("flame"), 4, 4, 16).unwrap();
        assert_eq!(sheet.len(), 16);
        assert!(sheet.iter().all(|l| (0.0..=1.0).contains(l)));
    }

    /// The worst legal effect: 8 emitters × 8 bursts, all repeating at 120 a second with long
    /// lives, distinct onsets, plus rates and a light. Bounded work (about a millisecond).
    #[test]
    fn the_worst_legal_effect_is_bounded() {
        let burst = |i: usize| format!(r#"{{"time": {}, "count": 1, "cycles": 0, "interval": 0.008333334}}"#, i as f32 * 3.7);
        let bursts: Vec<String> = (0..8).map(burst).collect();
        let em = |k: usize| format!(r##"{{"name": "e{k}", "delay": {}, "rate": 5, "bursts": [{}], "lifetime": [0.01, 0.7], "size": 0.01, "color": "#101010", "flipbook": {{"cols": 4, "rows": 4, "fps": 30}}, "texture": "flame"}}"##, k as f32 * 11.3, bursts.join(","));
        let ems: Vec<String> = (0..8).map(em).collect();
        let text = format!(r##"{{"light": {{"color": "#ffffff", "flicker": 0.05, "flicker_hz": 30}}, "emitters": [{}]}}"##, ems.join(","));
        let f = hz(&text);
        assert!(f.work < 4_000_000, "work {}", f.work);
    }

    /// Round 3: flicker faster than the old sampling of a long life (alpha keys every 1/48 s
    /// over 10 s: 24 flashes a second; every 1/36 s: 18) aliased to nothing. Exact key times.
    #[test]
    fn fast_flicker_over_a_long_life_is_found() {
        for (hz_wanted, per) in [(24.0f32, 480.0f32), (18.0, 360.0)] {
            let keys: Vec<String> = (0..16).map(|k| format!("[{}, {}]", k as f32 / per, (k + 1) % 2)).collect();
            let f = hz(&format!(r##"{{"emitters": [{{"rate": 3.2, "lifetime": 10, "speed": 0, "size": 3, "emissive": 4, "color": "#ffffff", "alpha": [{}], "texture": "soft"}}]}}"##, keys.join(", ")));
            assert!(f.hz >= hz_wanted * 0.25 && f.hz > 3.0, "{hz_wanted} Hz: {f:?}");
        }
        // A built-in flipbook whose frames differ, at 24 frames a second over a 10 s life.
        let f = hz(r##"{"emitters": [{"rate": 0.5, "lifetime": 10, "size": 3, "emissive": 6, "texture": "flame", "flipbook": {"cols": 1, "rows": 2, "frames": 2, "fps": 24}, "color": "#ffffff"}]}"##);
        let g = hz(r##"{"emitters": [{"rate": 0.5, "lifetime": 1, "size": 3, "emissive": 6, "texture": "flame", "flipbook": {"cols": 1, "rows": 2, "frames": 2, "fps": 24}, "color": "#ffffff"}]}"##);
        assert_eq!(f.hz > 3.0, g.hz > 3.0, "a long life sees what a short one does: {f:?} vs {g:?}");
    }

    #[test]
    fn params_and_asset_flipbooks() {
        // Small enough to pass, until params make it big.
        let small = r##"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.1}], "lifetime": 0.05, "size": 0.15, "emissive": 6, "color": "#ffffff", "alpha": [[0, 1], [1, 1]]}]}"##;
        assert!(hz(small).hz <= 3.0);
        assert!(hz_with(small, Params { size: 10.0, ..Params::default() }).hz > 3.0);
        // An asset flipbook's frames can't be read here: said so.
        let f = hz(r##"{"emitters": [{"rate": 1, "lifetime": 2, "size": 3, "texture": "fx/strobe.png", "flipbook": {"cols": 2, "rows": 1, "frames": 2, "fps": 12}, "color": "#ffffff"}]}"##);
        assert_eq!(f.unchecked, vec![0]);
    }

    #[test]
    #[ignore = "timing report"]
    #[allow(clippy::disallowed_methods, reason = "a timing report, outside the results")]
    fn flash_cost() {
        let burst = r#"{"count": 1, "cycles": 0, "interval": 0.008333334}"#;
        let bursts: Vec<&str> = (0..8).map(|_| burst).collect();
        let ems: Vec<String> = (0..8).map(|k| format!(r##"{{"name": "e{k}", "bursts": [{}], "lifetime": 6, "size": 0.01, "color": "#101010"}}"##, bursts.join(","))).collect();
        let e = Effect::parse_unchecked(&Json::parse(&format!(r#"{{"emitters": [{}]}}"#, ems.join(","))).unwrap()).unwrap();
        let _ = flash(&e, &Params::default()); // the built-in textures are made once, on first use
        let start = std::time::Instant::now();
        let f = flash(&e, &Params::default());
        eprintln!("review's case: {:.3} ms, work {}", start.elapsed().as_secs_f64() * 1e3, f.work);
        for (name, text) in crate::library::BUILTIN {
            let e = Effect::parse_unchecked(&Json::parse(text).unwrap()).unwrap();
            let start = std::time::Instant::now();
            let f = flash(&e, &Params::default());
            eprintln!("{name}: {:.3} ms, work {}", start.elapsed().as_secs_f64() * 1e3, f.work);
        }
    }

}
