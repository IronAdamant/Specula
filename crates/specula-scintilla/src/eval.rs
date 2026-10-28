//! The particle evaluator: every live particle of an effect instance at time `t`, computed from
//! scratch in closed form (docs/design/VFX.md, "Particles are a pure function of time").
//!
//! Inputs: the effect, the instance's seed and start time, where its emitter is now
//! ([`Placement`]) and, for world-space emitters, where it was ([`Track`]). Nothing is kept
//! between calls, so the same inputs give the same particles, bit for bit, whatever was evaluated
//! before (or culled). The output array is the caller's and is reused: no allocation per call.

use crate::component::Params;
use crate::curve::srgb_to_linear;
use crate::effect::{Collide, CollideMode, Direction, Effect, Emitter, Shape, Space, Surface};
use crate::noise::{curl, hash3, unit};
use specula_core::dmath;
use std::sync::OnceLock;

/// One particle, ready to draw (the GPU layer's `Particle` mirrors it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Particle {
    pub position: [f32; 3],
    /// Quad side in metres.
    pub size: f32,
    /// Linear RGB (HDR: may exceed 1) and straight alpha.
    pub color: [f32; 4],
    /// Radians.
    pub rotation: f32,
    /// World vector the quad is stretched along (zero: square).
    pub stretch: [f32; 3],
    /// 0 = alpha blended, 1 = additive.
    pub additive: f32,
    /// The caller's material index (from the [`EmitterPlan`]).
    pub material: u16,
    /// Flipbook frame.
    pub frame: u16,
    /// The particle's own hash (stable for its life: a later thinning keeps the same ones).
    pub id: u32,
}

/// Where an emitter's entity is now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: [f32; 3],
    /// Unit axes of its world rotation (columns: local X, Y, Z in world space).
    pub axes: [[f32; 3]; 3],
    /// Uniform scale (spawn shapes, speeds and sizes grow with it).
    pub scale: f32,
}

impl Placement {
    pub fn at(position: [f32; 3]) -> Placement {
        Placement { position, axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], scale: 1.0 }
    }

    /// From a column-major model matrix (rotation and uniform scale taken from its columns).
    pub fn from_model(m: &[f32; 16]) -> Placement {
        let col = |c: usize| [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
        let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let (x, y, z) = (col(0), col(1), col(2));
        let (lx, ly, lz) = (len(x), len(y), len(z));
        let unit = |v: [f32; 3], l: f32, fallback: [f32; 3]| if l > 1e-12 && l.is_finite() { [v[0] / l, v[1] / l, v[2] / l] } else { fallback };
        let scale = (lx + ly + lz) / 3.0;
        Placement {
            position: [m[12], m[13], m[14]],
            axes: [unit(x, lx, [1.0, 0.0, 0.0]), unit(y, ly, [0.0, 1.0, 0.0]), unit(z, lz, [0.0, 0.0, 1.0])],
            scale: if scale.is_finite() && scale > 0.0 { scale } else { 1.0 },
        }
    }

    fn rotate(&self, v: [f32; 3]) -> [f32; 3] {
        let a = &self.axes;
        [
            a[0][0] * v[0] + a[1][0] * v[1] + a[2][0] * v[2],
            a[0][1] * v[0] + a[1][1] * v[1] + a[2][1] * v[2],
            a[0][2] * v[0] + a[1][2] * v[1] + a[2][2] * v[2],
        ]
    }
}

/// Samples of where a world-space emitter's entity was (world time, position), oldest first, at
/// most [`Track::CAPACITY`]. Recorded once per simulation tick by the engine.
#[derive(Clone, Debug)]
pub struct Track {
    times: Vec<f64>,
    points: Vec<[f32; 3]>,
    /// Index of the oldest sample (a ring once full).
    head: usize,
}

impl Default for Track {
    fn default() -> Self {
        Track::new()
    }
}

impl Track {
    pub const CAPACITY: usize = 128;

    pub fn new() -> Track {
        Track { times: Vec::with_capacity(Track::CAPACITY), points: Vec::with_capacity(Track::CAPACITY), head: 0 }
    }

    pub fn len(&self) -> usize {
        self.times.len()
    }

    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    fn get(&self, k: usize) -> (f64, [f32; 3]) {
        let i = (self.head + k) % self.times.len();
        (self.times[i], self.points[i])
    }

    /// Adds a sample; one at the newest sample's time (or earlier: time went back, a load)
    /// replaces it (or starts over).
    pub fn push(&mut self, t: f64, p: [f32; 3]) {
        if let Some((newest, _)) = self.newest() {
            if t < newest {
                self.clear();
            } else if t == newest {
                let n = self.times.len();
                let i = (self.head + n - 1) % n;
                self.points[i] = p;
                return;
            }
        }
        if self.times.len() < Track::CAPACITY {
            self.times.push(t);
            self.points.push(p);
        } else {
            self.times[self.head] = t;
            self.points[self.head] = p;
            self.head = (self.head + 1) % Track::CAPACITY;
        }
    }

    pub fn clear(&mut self) {
        self.times.clear();
        self.points.clear();
        self.head = 0;
    }

    pub fn newest(&self) -> Option<(f64, [f32; 3])> {
        (!self.times.is_empty()).then(|| self.get(self.times.len() - 1))
    }

    pub fn oldest(&self) -> Option<(f64, [f32; 3])> {
        (!self.times.is_empty()).then(|| self.get(0))
    }

    /// The position at `t`, linear between samples, held before the first and after the last;
    /// `now` (the entity's current position at `now_t`) extends the track past its newest sample.
    pub fn at(&self, t: f64, now_t: f64, now: [f32; 3]) -> [f32; 3] {
        let n = self.times.len();
        if n == 0 {
            return now;
        }
        let (t_last, p_last) = self.get(n - 1);
        if t >= t_last {
            return if now_t > t_last && t < now_t { lerp3(p_last, now, ((t - t_last) / (now_t - t_last)) as f32) } else if t >= now_t { now } else { p_last };
        }
        let (t0, p0) = self.get(0);
        if t <= t0 {
            return p0;
        }
        // Binary search for the segment [k, k + 1] holding t.
        let (mut lo, mut hi) = (0usize, n - 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.get(mid).0 <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (ta, pa) = self.get(lo);
        let (tb, pb) = self.get(hi);
        let f = if tb > ta { ((t - ta) / (tb - ta)) as f32 } else { 1.0 };
        lerp3(pa, pb, f)
    }

    /// The emitter's velocity around `t` (m/s), from the samples on either side.
    pub fn velocity(&self, t: f64, now_t: f64, now: [f32; 3]) -> [f32; 3] {
        let h = 1.0 / 60.0;
        let a = self.at(t - h, now_t, now);
        let b = self.at(t + h, now_t, now);
        let k = (1.0 / (2.0 * h)) as f32;
        [(b[0] - a[0]) * k, (b[1] - a[1]) * k, (b[2] - a[2]) * k]
    }

    /// The fastest the track moved between `from` and `now_t` (m/s; segment by segment, so it
    /// bounds [`Track::velocity`] there).
    pub fn max_speed(&self, from: f64, now_t: f64, now: [f32; 3]) -> f32 {
        let from = from - 1.0 / 60.0;
        let mut prev: Option<(f64, [f32; 3])> = None;
        let mut best = 0.0f32;
        let points = (0..self.times.len()).map(|k| self.get(k)).chain(std::iter::once((now_t, now)));
        for (t, p) in points {
            if let Some((tp, pp)) = prev {
                if t > tp && t >= from && tp <= now_t {
                    let d = [p[0] - pp[0], p[1] - pp[1], p[2] - pp[2]];
                    best = best.max((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / (t - tp) as f32);
                }
            }
            prev = Some((t, p));
        }
        best
    }

    /// The box around every position in `[from, now_t]`.
    pub fn bounds(&self, from: f64, now_t: f64, now: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let mut lo = now;
        let mut hi = now;
        let mut grow = |p: [f32; 3]| {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        };
        grow(self.at(from, now_t, now));
        for k in 0..self.times.len() {
            let (t, p) = self.get(k);
            if t >= from && t <= now_t {
                grow(p);
            }
        }
        (lo, hi)
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], f: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
}

/// The world's ground for `collide: {ground: true}`.
pub trait Ground {
    /// The ground's height under `(x, z)` near `y` (searching from a little above it), if any.
    fn height(&self, x: f32, z: f32, y: f32) -> Option<f32>;
}

/// An effect playing somewhere.
#[derive(Clone, Copy)]
pub struct Instance<'a> {
    pub effect: &'a Effect,
    pub params: Params,
    /// From [`seed_of`].
    pub seed: u32,
    /// World time the effect started.
    pub start: f64,
    pub placement: Placement,
    /// World-space emitters: where the entity was (`None`: always where it is now).
    pub track: Option<&'a Track>,
    pub ground: Option<&'a dyn Ground>,
}

/// An instance's seed from its entity, its start time and `params.seed`.
pub fn seed_of(entity: u64, start: f64, param_seed: u32) -> u32 {
    let s = start.to_bits();
    hash3(hash3(entity as u32, (entity >> 32) as u32, param_seed), s as u32, (s >> 32) as u32)
}

/// What the caller decided for one emitter this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitterPlan {
    /// Fraction of its particles drawn (budget and LOD thinning: a particle is kept when its own
    /// hash is below it, so the same ones stay).
    pub keep: f32,
    /// Size multiplier for the survivors (keeps a thinned effect's weight).
    pub size_boost: f32,
    /// Written into every particle.
    pub material: u16,
}

impl Default for EmitterPlan {
    fn default() -> Self {
        EmitterPlan { keep: 1.0, size_boost: 1.0, material: 0 }
    }
}

/// Counts of one evaluation (added to).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvalStats {
    /// Alive at `t` (before thinning).
    pub live: u32,
    /// Written to the output.
    pub drawn: u32,
    /// Left out by the plan's keep fraction.
    pub thinned: u32,
    /// Dead on a collision surface (`collide.mode: kill`).
    pub killed: u32,
    /// Alive but not written: the output was full.
    pub dropped: u32,
    /// Particle indices looked at.
    pub visited: u64,
    /// The work done, in units of one plain particle's evaluation (see [`work_per_particle`]):
    /// every index looked at, each position computed (noise costs more), each bisection step of
    /// `stick`. Evaluation stops when it reaches the caller's budget.
    pub work: u64,
    /// Drawn, then left out by the caller: every one of the instance's particles was out of
    /// view (live = drawn + thinned + killed + dropped + offscreen).
    pub offscreen: u32,
}

// Hash fields of a particle (each its own stream).
const F_LIFE: u32 = 1;
const F_KEEP: u32 = 2;
const F_S1: u32 = 3;
const F_S2: u32 = 4;
const F_S3: u32 = 5;
const F_D1: u32 = 6;
const F_D2: u32 = 7;
const F_SPEED: u32 = 8;
const F_SIZE: u32 = 9;
const F_ROT: u32 = 10;
const F_SPIN: u32 = 11;
const F_FRAME: u32 = 12;
const F_JITTER: u32 = 13;

/// Unit vectors at 4096 angles around the circle (random angles need no more precision), built
/// once with `dmath`.
fn circle() -> &'static [[f32; 2]] {
    static TABLE: OnceLock<Vec<[f32; 2]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..4096)
            .map(|i| {
                let (s, c) = dmath::sin_cos(i as f32 * (std::f32::consts::TAU / 4096.0));
                [c, s]
            })
            .collect()
    })
}

/// The unit vector at angle `u` turns (`u` in 0..1).
#[inline]
fn around(u: f32) -> [f32; 2] {
    circle()[((u * 4096.0) as usize) & 4095]
}

/// A uniformly random direction on the cap within `cos_max` of +Y.
#[inline]
fn cap(u1: f32, u2: f32, cos_max: f32) -> [f32; 3] {
    let c = 1.0 - u1 * (1.0 - cos_max);
    let s = (1.0 - c * c).max(0.0).sqrt();
    let [x, z] = around(u2);
    [x * s, c, z * s]
}

/// An orthonormal basis `(t, d, b)` around the unit vector `d`.
fn basis(d: [f32; 3]) -> [[f32; 3]; 3] {
    let helper = if d[1].abs() < 0.99 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let t = normalize(cross(helper, d));
    [t, d, cross(d, t)]
}

/// `v` (relative to +Y) turned so +Y maps onto `basis[1]`.
#[inline]
fn align(v: [f32; 3], basis: &[[f32; 3]; 3]) -> [f32; 3] {
    let [t, d, b] = basis;
    [t[0] * v[0] + d[0] * v[1] + b[0] * v[2], t[1] * v[0] + d[1] * v[1] + b[1] * v[2], t[2] * v[0] + d[2] * v[1] + b[2] * v[2]]
}

#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[inline]
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-12 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Where a particle spawns (local, unscaled) from three uniform numbers.
fn spawn(shape: &Shape, u1: f32, u2: f32, u3: f32) -> [f32; 3] {
    match *shape {
        Shape::Point => [0.0; 3],
        Shape::Sphere { radius, surface } => {
            let d = cap(u1, u2, -1.0);
            let r = if surface { radius } else { radius * dmath::pow(u3, 1.0 / 3.0) };
            [d[0] * r, d[1] * r, d[2] * r]
        }
        Shape::Box { size } => [(u1 - 0.5) * size[0], (u2 - 0.5) * size[1], (u3 - 0.5) * size[2]],
        Shape::Disc { radius } | Shape::Cone { radius, .. } => {
            let r = radius * u1.sqrt();
            let [x, z] = around(u2);
            [x * r, 0.0, z * r]
        }
        Shape::Line { length } => [(u1 - 0.5) * length, 0.0, 0.0],
    }
}

/// Displacement and velocity after `a` seconds from `v0` under gravity `g` and linear drag `k`.
#[inline]
fn ballistic(v0: [f32; 3], g: [f32; 3], k: f32, a: f32) -> ([f32; 3], [f32; 3]) {
    // d = v0·S1 + g·S2 and v = v0·e^(−x) + g·S1 with x = k·a, S1 = (1 − e^(−x))/k and
    // S2 = (a − S1)/k; for small x their series (the direct forms cancel catastrophically).
    let x = k * a;
    let (e, s1, s2) = if x < 0.1 {
        let s1 = a * (1.0 - x * (0.5 - x * (1.0 / 6.0 - x * (1.0 / 24.0))));
        let s2 = a * a * (0.5 - x * (1.0 / 6.0 - x * (1.0 / 24.0 - x * (1.0 / 120.0))));
        (1.0 - k * s1, s1, s2)
    } else {
        let e = crate::noise::exp_neg(x);
        let s1 = (1.0 - e) / k;
        (e, s1, (a - s1) / k)
    };
    let d = std::array::from_fn(|i| v0[i] * s1 + g[i] * s2);
    let v = std::array::from_fn(|i| v0[i] * e + g[i] * s1);
    (d, v)
}

/// What evaluation needs of an emitter beyond its data, worked out once per call.
struct Prep {
    rate: f32,
    cos_spread: f32,
    /// Around the emitter's direction (+Y when radial).
    basis: [[f32; 3]; 3],
    tint: [f32; 3],
    gravity: [f32; 3],
    /// Dead particles that touched the surface (kill) count here.
    stick_iters: u32,
    /// Stop once `stats.work` reaches this (the caller's work budget).
    cap: u64,
}

/// Work units, about 10-20 ns each on the reference laptop's CPU (`eval_cost_by_kind`, release):
/// an index looked at; one position (the ballistic part); the curl noise on top of a position;
/// finishing a drawn particle (curves, flipbook, output). A plain drawn particle costs 11, one
/// with noise 26, with `stick` 29, with both 179 (measured about 1 : 2.4 : 2.7 : 12-16).
pub const W_VISIT: u64 = 1;
pub const W_POSITION: u64 = 2;
pub const W_NOISE: u64 = 15;
pub const W_FINISH: u64 = 8;
/// Bisection steps `collide: stick` takes to find the first touch.
pub const STICK_ITERS: u32 = 8;

/// The work one drawn particle of `e` costs (for budgets: expected particles × this).
pub fn work_per_particle(e: &Emitter) -> f32 {
    let at = (W_POSITION + if e.noise.is_some() { W_NOISE } else { 0 }) as f32;
    let stick = matches!(e.collide, Some(Collide { mode: CollideMode::Stick, .. }));
    (W_VISIT + W_FINISH) as f32 + at * if stick { (STICK_ITERS + 2) as f32 } else { 1.0 }
}

/// Evaluates `inst` at world time `t`, appending its particles to `out` (at most `limit` in
/// total in `out`). `plans` has one entry per emitter (missing ones: defaults).
///
/// `max_work` bounds the work: evaluation stops once `stats.work` reaches it (mid-emitter
/// included), so no effect, however dense, can stall a frame.
pub fn evaluate(inst: &Instance, t: f64, plans: &[EmitterPlan], limit: usize, max_work: u64, out: &mut Vec<Particle>, stats: &mut EvalStats) {
    let tint = inst.params.tint.map(srgb_to_linear);
    for (k, e) in inst.effect.emitters.iter().enumerate() {
        let plan = plans.get(k).copied().unwrap_or_default();
        let cos_spread = match e.shape {
            Shape::Cone { angle_deg, .. } => dmath::cos(angle_deg.to_radians()),
            _ => dmath::cos(e.spread_deg.to_radians()),
        };
        let along = match e.direction {
            Direction::Along(d) => d,
            Direction::Radial => [0.0, 1.0, 0.0],
        };
        let prep = Prep { rate: e.rate * inst.params.rate, cos_spread, basis: basis(along), tint, gravity: e.gravity, stick_iters: STICK_ITERS, cap: max_work };
        if stats.work >= max_work || out.len() >= limit {
            return;
        }
        emitter(inst, k, e, &prep, &plan, t, limit, out, stats);
    }
}

#[allow(clippy::too_many_arguments)]
fn emitter(inst: &Instance, k: usize, e: &Emitter, prep: &Prep, plan: &EmitterPlan, t: f64, limit: usize, out: &mut Vec<Particle>, stats: &mut EvalStats) {
    let em_start = inst.start + e.delay as f64;
    let local = t - em_start;
    if local < 0.0 {
        return;
    }
    let lmax = e.lifetime[1] as f64;
    let stream_seed = hash3(inst.seed, k as u32, 0xE1A7_0000);
    // Thinned emitters visit only every `stride`-th particle (keeping each of those with
    // probability keep × stride): the work follows what is drawn, not what would live.
    if plan.keep <= 0.0 {
        return;
    }
    let stride = if plan.keep >= 1.0 { 1u64 } else { (1.0 / plan.keep).min(1.0e6) as u64 }.max(1);
    let plan = &EmitterPlan { keep: (plan.keep * stride as f32).min(1.0), ..*plan };
    if prep.rate > 0.0 {
        let rate = prep.rate as f64;
        let end = e.duration.map_or(f64::INFINITY, |d| d as f64);
        let from = (local - lmax).max(0.0);
        let to = local.min(end);
        if to >= from {
            let first = (dmath::floor_f64(from * rate) - 1.0).max(0.0) as u64;
            let first = first.div_ceil(stride) * stride;
            let last = dmath::floor_f64(to * rate).max(0.0) as u64;
            for i in (first..=last).step_by(stride as usize) {
                if stats.work >= prep.cap || out.len() >= limit {
                    return;
                }
                let id = hash3(stream_seed, i as u32, (i >> 32) as u32);
                let birth = (i as f64 + unit(id, F_JITTER, 0) as f64) / rate;
                if birth > to || birth >= end {
                    continue;
                }
                particle(inst, e, prep, plan, id, em_start, birth, local, t, limit, out, stats);
            }
        }
    }
    for (bi, b) in e.bursts.iter().enumerate() {
        let count = dmath::round(b.count as f32 * inst.params.rate) as u32;
        if count == 0 || local < b.time as f64 {
            continue;
        }
        let (lo, hi) = if b.interval > 0.0 {
            let iv = b.interval as f64;
            let hi = dmath::floor_f64((local - b.time as f64) / iv).max(0.0) as u64;
            let hi = if b.cycles == 0 { hi } else { hi.min(u64::from(b.cycles) - 1) };
            let lo = dmath::ceil_f64((local - lmax - b.time as f64) / iv).max(0.0) as u64;
            (lo, hi)
        } else {
            (0, 0)
        };
        // Cycles × count flattened into one index, so the stride thins across cycles too (a
        // burst of 1 repeating fast is thinned like a rate); aligned to absolute indices, so the
        // kept subset is stable as old cycles die.
        let n = u64::from(count);
        let first = (lo * n).div_ceil(stride) * stride;
        let end = (hi + 1) * n;
        let mut cycle = u64::MAX;
        let (mut fired, mut cycle_seed, mut alive) = (0.0f64, 0u32, false);
        for idx in (first..end).step_by(stride as usize) {
            if stats.work >= prep.cap || out.len() >= limit {
                return;
            }
            let (c, j) = (idx / n, (idx % n) as u32);
            if c != cycle {
                cycle = c;
                fired = b.time as f64 + c as f64 * b.interval as f64;
                alive = fired <= local && local - fired < lmax;
                cycle_seed = hash3(stream_seed, 0xB000 + bi as u32, c as u32);
            }
            if !alive {
                continue;
            }
            let id = hash3(cycle_seed, j, 0x0B57);
            particle(inst, e, prep, plan, id, em_start, fired, local, t, limit, out, stats);
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn particle(
    inst: &Instance,
    e: &Emitter,
    prep: &Prep,
    plan: &EmitterPlan,
    id: u32,
    em_start: f64,
    birth: f64,
    local: f64,
    t: f64,
    limit: usize,
    out: &mut Vec<Particle>,
    stats: &mut EvalStats,
) {
    stats.visited += 1;
    stats.work += W_VISIT;
    let age = (local - birth) as f32;
    let life = e.lifetime[0] + (e.lifetime[1] - e.lifetime[0]) * unit(id, F_LIFE, 0);
    if !(0.0..life).contains(&age) {
        return;
    }
    stats.live += 1;
    if unit(id, F_KEEP, 0) >= plan.keep {
        stats.thinned += 1;
        return;
    }
    // Full output: counted, never computed (a dropped particle costs a visit, not a position).
    if out.len() >= limit {
        stats.dropped += 1;
        return;
    }
    let at_cost = W_POSITION + if e.noise.is_some() { W_NOISE } else { 0 };
    stats.work += at_cost + W_FINISH;
    let u = age / life;
    let pl = &inst.placement;
    let scale = pl.scale;
    let off = spawn(&e.shape, unit(id, F_S1, 0), unit(id, F_S2, 0), unit(id, F_S3, 0));
    let dir = match e.direction {
        Direction::Along(_) => align(cap(unit(id, F_D1, 0), unit(id, F_D2, 0), prep.cos_spread), &prep.basis),
        Direction::Radial => {
            let l = (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt();
            if l > 1e-6 {
                [off[0] / l, off[1] / l, off[2] / l]
            } else {
                cap(unit(id, F_D1, 0), unit(id, F_D2, 0), -1.0)
            }
        }
    };
    let speed = (e.speed[0] + (e.speed[1] - e.speed[0]) * unit(id, F_SPEED, 0)) * inst.params.speed * scale;
    let off_w = pl.rotate([off[0] * scale, off[1] * scale, off[2] * scale]);
    let dir_w = pl.rotate(dir);
    let mut v0 = [dir_w[0] * speed, dir_w[1] * speed, dir_w[2] * speed];
    let birth_abs = em_start + birth;
    let origin = match (e.space, inst.track) {
        (Space::World, Some(track)) => {
            if e.inherit_velocity > 0.0 {
                let v = track.velocity(birth_abs, t, pl.position);
                for i in 0..3 {
                    v0[i] += v[i] * e.inherit_velocity;
                }
            }
            track.at(birth_abs, t, pl.position)
        }
        _ => pl.position,
    };
    let at = |a: f32| -> ([f32; 3], [f32; 3]) {
        let (d, v) = ballistic(v0, prep.gravity, e.drag, a);
        let rel = [off_w[0] + d[0], off_w[1] + d[1], off_w[2] + d[2]];
        let mut p = [origin[0] + rel[0], origin[1] + rel[1], origin[2] + rel[2]];
        if let Some(n) = &e.noise {
            // Local emitters' field moves with them; world ones' stays in the world.
            let q = if e.space == Space::Local { rel } else { p };
            let phase = (birth + f64::from(a)) * f64::from(n.speed);
            let c = curl([q[0] * n.frequency, q[1] * n.frequency, q[2] * n.frequency], phase, inst.seed);
            let s = n.strength * a * scale;
            p = [p[0] + c[0] * s, p[1] + c[1] * s, p[2] + c[2] * s];
        }
        (p, v)
    };
    let (mut p, mut vel) = at(age);
    if let Some(col) = &e.collide {
        let surface = |p: [f32; 3]| -> Option<f32> {
            match col.surface {
                Surface::Plane(h) => Some(origin[1] + h * scale),
                Surface::Ground => inst.ground.and_then(|g| g.height(p[0], p[2], p[1] + 0.5)),
            }
        };
        if let Some(s) = surface(p) {
            let floor = s + col.radius;
            if p[1] < floor {
                match col.mode {
                    CollideMode::Kill => {
                        stats.killed += 1;
                        return;
                    }
                    CollideMode::Slide => {
                        p[1] = floor;
                        vel[1] = vel[1].max(0.0);
                    }
                    CollideMode::Stick => {
                        // The first touch, by bisection on the age (below the surface at `age`).
                        let (mut lo, mut hi) = (0.0f32, age);
                        stats.work += at_cost * u64::from(prep.stick_iters + 1);
                        for _ in 0..prep.stick_iters {
                            let mid = 0.5 * (lo + hi);
                            let (q, _) = at(mid);
                            if surface(q).is_some_and(|s| q[1] < s + col.radius) {
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                        p = at(lo).0;
                        p[1] = p[1].max(floor);
                        vel = [0.0; 3];
                    }
                }
            }
        }
    }
    let size = e.size.eval(u) * (e.size_random[0] + (e.size_random[1] - e.size_random[0]) * unit(id, F_SIZE, 0)) * inst.params.size * scale * plan.size_boost;
    let c = e.color.eval(u);
    let glow = e.emissive * inst.params.intensity;
    let alpha = (e.alpha.eval(u) * inst.params.alpha).clamp(0.0, 1.0);
    let rot0 = e.rotation_deg[0] + (e.rotation_deg[1] - e.rotation_deg[0]) * unit(id, F_ROT, 0);
    let spin = e.spin_deg[0] + (e.spin_deg[1] - e.spin_deg[0]) * unit(id, F_SPIN, 0);
    let frame = match &e.flipbook {
        None => 0,
        Some(fb) => {
            let n = fb.frames.max(1);
            let f = if fb.over_life {
                ((u * n as f32) as u32).min(n - 1)
            } else {
                let start = if fb.random_start { unit(id, F_FRAME, 0) * n as f32 } else { 0.0 };
                (dmath::floor(age * fb.fps + start).max(0.0) as u32) % n
            };
            f as u16
        }
    };
    let stretch = if e.stretch > 0.0 { [vel[0] * e.stretch, vel[1] * e.stretch, vel[2] * e.stretch] } else { [0.0; 3] };
    out.push(Particle {
        position: p,
        size,
        color: [c[0] * prep.tint[0] * glow, c[1] * prep.tint[1] * glow, c[2] * prep.tint[2] * glow, alpha],
        rotation: (rot0 + spin * age).to_radians(),
        stretch,
        additive: e.additive,
        material: plan.material,
        frame,
        id,
    });
    stats.drawn += 1;
}

/// The world box an instance's particles can be in at `t` (for culling): the emitter (or its
/// track over the longest lifetime) grown by the effect's reach.
pub fn bounds(inst: &Instance, t: f64) -> ([f32; 3], [f32; 3]) {
    let p = inst.placement.position;
    let life = inst.effect.max_lifetime();
    let (lo, hi, thrown) = match (inst.effect.world_space(), inst.track) {
        (true, Some(track)) => {
            let (lo, hi) = track.bounds(t - life as f64, t, p);
            // Inherited velocity throws particles ahead of the track, and stretches them.
            let thrown = inst.effect.emitters.iter().map(|e| e.inherit_velocity * (life + e.stretch)).fold(0.0, f32::max);
            let thrown = if thrown > 0.0 { thrown * track.max_speed(t - life as f64, t, p) } else { 0.0 };
            (lo, hi, thrown)
        }
        _ => (p, p, 0.0),
    };
    let params = &inst.params;
    let r = inst.effect.reach() * inst.placement.scale * params.speed.max(1.0) * params.size.max(1.0) + thrown;
    (lo.map(|v| v - r), hi.map(|v| v + r))
}

/// Particles an instance is expected to have alive at `t` (analytic, before thinning), per
/// emitter.
pub fn expected(inst: &Instance, t: f64, out: &mut [f32]) {
    let age = (t - inst.start) as f32;
    for (k, e) in inst.effect.emitters.iter().enumerate() {
        if let Some(slot) = out.get_mut(k) {
            *slot = e.expected(age) * inst.params.rate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_core::Json;

    fn effect(text: &str) -> Effect {
        Effect::parse(&Json::parse(text).unwrap()).unwrap()
    }

    fn run(e: &Effect, t: f64, placement: Placement, track: Option<&Track>) -> (Vec<Particle>, EvalStats) {
        let inst = Instance { effect: e, params: Params::default(), seed: seed_of(7, 0.0, 0), start: 0.0, placement, track, ground: None };
        let mut out = Vec::new();
        let mut stats = EvalStats::default();
        evaluate(&inst, t, &[EmitterPlan::default(); 8], usize::MAX, u64::MAX, &mut out, &mut stats);
        (out, stats)
    }

    const FOUNTAIN: &str = r#"{"emitters": [{"rate": 100, "lifetime": [1, 2], "speed": [2, 3], "spread_deg": 30,
        "gravity": [0, -9.8, 0], "drag": 0.5, "noise": {"strength": 0.2}, "size": 0.1}]}"#;

    #[test]
    fn the_same_inputs_give_the_same_particles() {
        let e = effect(FOUNTAIN);
        let (a, sa) = run(&e, 3.25, Placement::at([1.0, 2.0, 3.0]), None);
        let (b, sb) = run(&e, 3.25, Placement::at([1.0, 2.0, 3.0]), None);
        assert_eq!(a, b);
        assert_eq!(sa, sb);
        // Steady state: rate × mean lifetime alive (within sampling noise).
        assert!((120..=180).contains(&sa.live), "{sa:?}");
        // Evaluating other times in between changes nothing (no hidden state).
        let _ = run(&e, 1.0, Placement::at([0.0; 3]), None);
        assert_eq!(run(&e, 3.25, Placement::at([1.0, 2.0, 3.0]), None).0, a);
        // Pinned: a checksum of every bit of the output (a change here changes every capture).
        let mut h = 0x5EEDu32;
        for p in &a {
            for v in p.position.iter().chain([p.size, p.rotation].iter()).chain(p.color.iter()).chain(p.stretch.iter()) {
                h = crate::noise::hash3(h, v.to_bits(), 0x601D);
            }
            h = crate::noise::hash3(h, u32::from(p.frame), u32::from(p.material));
        }
        assert_eq!((a.len(), h), GOLDEN_FOUNTAIN, "{sa:?}");
    }

    const GOLDEN_FOUNTAIN: (usize, u32) = (147, 450_199_383);

    #[test]
    fn particles_stay_within_the_analytic_reach() {
        let e = effect(FOUNTAIN);
        let (ps, _) = run(&e, 5.0, Placement::at([0.0; 3]), None);
        let reach = e.reach();
        for p in &ps {
            let d = (p.position[0] * p.position[0] + p.position[1] * p.position[1] + p.position[2] * p.position[2]).sqrt();
            assert!(d <= reach, "{d} > {reach}");
        }
        let inst = Instance { effect: &e, params: Params::default(), seed: 1, start: 0.0, placement: Placement::at([0.0; 3]), track: None, ground: None };
        let (lo, hi) = bounds(&inst, 5.0);
        assert!(lo[1] <= -reach + 1e-4 && hi[1] >= reach - 1e-4);
    }

    #[test]
    fn ballistic_matches_the_no_drag_limit() {
        let (d, v) = ballistic([1.0, 2.0, 0.0], [0.0, -10.0, 0.0], 0.0, 0.5);
        assert_eq!(d, [0.5, 1.0 - 1.25, 0.0]);
        assert_eq!(v, [1.0, -3.0, 0.0]);
        let (d2, _) = ballistic([1.0, 2.0, 0.0], [0.0, -10.0, 0.0], 1e-3, 0.5);
        assert!((d2[1] - d[1]).abs() < 1e-3, "{d2:?}");
    }

    #[test]
    fn thinning_keeps_a_stable_subset() {
        let e = effect(FOUNTAIN);
        let inst = Instance { effect: &e, params: Params::default(), seed: 3, start: 0.0, placement: Placement::at([0.0; 3]), track: None, ground: None };
        let (mut all, mut half) = (Vec::new(), Vec::new());
        let mut s = EvalStats::default();
        evaluate(&inst, 4.0, &[EmitterPlan::default()], usize::MAX, u64::MAX, &mut all, &mut s);
        let mut sh = EvalStats::default();
        evaluate(&inst, 4.0, &[EmitterPlan { keep: 0.5, ..Default::default() }], usize::MAX, u64::MAX, &mut half, &mut sh);
        assert!(sh.visited * 2 <= s.visited + 2, "thinned emitters visit only what they may keep: {sh:?} vs {s:?}");
        assert_eq!(sh.drawn + sh.thinned, sh.live);
        assert!(half.iter().all(|p| all.contains(p)), "the kept ones are a subset");
        let ratio = sh.drawn as f32 / s.drawn as f32;
        assert!((0.35..0.65).contains(&ratio), "{ratio}");
        // A limit drops the rest (counted).
        let mut few = Vec::new();
        let mut sf = EvalStats::default();
        evaluate(&inst, 4.0, &[EmitterPlan::default()], 10, u64::MAX, &mut few, &mut sf);
        // The output fills, then the emitter stops (the rest is never computed).
        assert_eq!(few.len(), 10);
        assert!(sf.visited < s.visited / 2, "{sf:?} vs {s:?}");
    }

    #[test]
    fn fast_repeating_bursts_thin_across_cycles_and_work_is_capped() {
        // One particle every 1/100 s living 10 s: 1,000 alive, all from separate cycles.
        let e = effect(r#"{"emitters": [{"bursts": [{"count": 1, "cycles": 0, "interval": 0.01}], "lifetime": 10}]}"#);
        let inst = Instance { effect: &e, params: Params::default(), seed: 3, start: 0.0, placement: Placement::at([0.0; 3]), track: None, ground: None };
        let (mut all, mut few) = (Vec::new(), Vec::new());
        let (mut s, mut sf) = (EvalStats::default(), EvalStats::default());
        evaluate(&inst, 20.0, &[EmitterPlan::default()], usize::MAX, u64::MAX, &mut all, &mut s);
        assert!((995..=1001).contains(&s.live), "{s:?}");
        evaluate(&inst, 20.0, &[EmitterPlan { keep: 0.01, ..Default::default() }], usize::MAX, u64::MAX, &mut few, &mut sf);
        assert!(sf.visited <= 12, "the stride thins across cycles: {sf:?}");
        assert!(few.iter().all(|p| all.contains(p)), "a subset");
        // The caller's work budget stops evaluation mid-emitter.
        let (mut capped, mut sc) = (Vec::new(), EvalStats::default());
        evaluate(&inst, 20.0, &[EmitterPlan::default()], usize::MAX, 100, &mut capped, &mut sc);
        assert!((100..=120).contains(&sc.work), "{sc:?}");
    }

    #[test]
    fn bursts_fire_once_and_end() {
        let e = effect(r#"{"emitters": [{"bursts": [{"time": 0.5, "count": 40}], "lifetime": 1, "direction": "radial", "shape": {"kind": "sphere", "radius": 0.1}, "speed": 1}]}"#);
        assert_eq!(run(&e, 0.4, Placement::at([0.0; 3]), None).1.live, 0);
        let (ps, s) = run(&e, 1.0, Placement::at([0.0; 3]), None);
        assert_eq!(s.live, 40);
        // Radial: every particle moved outward by 0.5 s × 1 m/s.
        for p in &ps {
            let d = (p.position[0] * p.position[0] + p.position[1] * p.position[1] + p.position[2] * p.position[2]).sqrt();
            assert!((0.45..=0.61).contains(&d), "{d}");
        }
        assert_eq!(run(&e, 1.6, Placement::at([0.0; 3]), None).1.live, 0);
    }

    #[test]
    fn collisions_kill_slide_and_stick() {
        let make = |mode: &str| effect(&format!(r#"{{"emitters": [{{"rate": 50, "lifetime": 3, "speed": 0, "gravity": [0, -10, 0], "collide": {{"plane": -1, "mode": "{mode}"}}}}]}}"#));
        let (k, sk) = run(&make("kill"), 3.0, Placement::at([0.0; 3]), None);
        assert!(sk.killed > 0 && k.iter().all(|p| p.position[1] >= -1.0));
        let (s, _) = run(&make("slide"), 3.0, Placement::at([0.0; 3]), None);
        assert!(s.iter().all(|p| p.position[1] >= -1.0) && s.iter().any(|p| p.position[1] == -1.0));
        let (st, _) = run(&make("stick"), 3.0, Placement::at([0.0; 3]), None);
        assert!(st.iter().all(|p| p.position[1] >= -1.0 - 1e-3 && p.stretch == [0.0; 3]));
    }

    #[test]
    fn world_space_particles_stay_where_they_were_born() {
        let e = effect(r#"{"emitters": [{"rate": 20, "lifetime": 2, "space": "world", "speed": 0}]}"#);
        let mut track = Track::new();
        for i in 0..=60 {
            let t = i as f64 / 30.0;
            track.push(t, [t as f32 * 3.0, 0.0, 0.0]);
        }
        let (ps, _) = run(&e, 2.0, Placement::at([6.0, 0.0, 0.0]), Some(&track));
        // Born along the path from x = 0 to 6: spread out behind the emitter.
        let xs: Vec<f32> = ps.iter().map(|p| p.position[0]).collect();
        assert!(xs.iter().any(|x| *x < 2.0) && xs.iter().any(|x| *x > 4.0), "{xs:?}");
        // Local space: all at the emitter.
        let local = effect(r#"{"emitters": [{"rate": 20, "lifetime": 2, "speed": 0}]}"#);
        let (ps, _) = run(&local, 2.0, Placement::at([6.0, 0.0, 0.0]), Some(&track));
        assert!(ps.iter().all(|p| p.position == [6.0, 0.0, 0.0]));
    }

    #[test]
    fn tracks_interpolate_wrap_and_reset() {
        let mut t = Track::new();
        t.push(0.0, [0.0; 3]);
        t.push(1.0, [10.0, 0.0, 0.0]);
        assert_eq!(t.at(0.5, 1.0, [10.0, 0.0, 0.0]), [5.0, 0.0, 0.0]);
        assert_eq!(t.at(-1.0, 1.0, [10.0, 0.0, 0.0]), [0.0; 3]);
        assert_eq!(t.at(1.5, 2.0, [20.0, 0.0, 0.0]), [15.0, 0.0, 0.0], "past the newest sample: towards now");
        for i in 0..300 {
            t.push(2.0 + i as f64, [i as f32; 3]);
        }
        assert_eq!(t.len(), Track::CAPACITY);
        assert_eq!(t.oldest().unwrap().0, 2.0 + (300 - Track::CAPACITY) as f64);
        t.push(0.0, [1.0; 3]);
        assert_eq!(t.len(), 1, "time going back starts over");
    }

    #[test]
    fn flipbooks_and_spin_follow_age() {
        let e = effect(r#"{"emitters": [{"bursts": [{"count": 1}], "lifetime": 2, "flipbook": {"cols": 4, "rows": 2, "fps": 4}, "spin_deg": 90}]}"#);
        let (ps, _) = run(&e, 0.75, Placement::at([0.0; 3]), None);
        assert_eq!(ps[0].frame, 3);
        assert!((ps[0].rotation - 67.5f32.to_radians()).abs() < 1e-5);
        let (ps, _) = run(&e, 1.9, Placement::at([0.0; 3]), None);
        assert_eq!(ps[0].frame, 7);
    }

    #[test]
    fn placement_rotates_and_scales_emission() {
        // A model turned 90° about Z (local +Y → world −X) and scaled 2.
        let m = [0.0, 2.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 5.0, 0.0, 0.0, 1.0];
        let pl = Placement::from_model(&m);
        assert_eq!(pl.scale, 2.0);
        let e = effect(r#"{"emitters": [{"bursts": [{"count": 4}], "lifetime": 2, "speed": 1, "size": 0.5}]}"#);
        let (ps, _) = run(&e, 1.0, pl, None);
        for p in &ps {
            assert!((p.position[0] - 3.0).abs() < 1e-4 && p.position[1].abs() < 1e-4, "{:?}", p.position);
            assert!((p.size - 1.0).abs() < 1e-6);
        }
    }

    /// Timing report: `cargo test --release -p specula-scintilla eval_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing report"]
    #[allow(clippy::disallowed_methods, reason = "a timing report, outside the results")]
    fn eval_cost_by_kind() {
        let base = r##""rate": 2000, "lifetime": [2, 3], "speed": [1, 2], "spread_deg": 40, "gravity": [0, -3, 0], "size": 0.1"##;
        for (label, extra) in [
            ("plain", ""),
            ("noise", r#", "noise": {"strength": 0.3}"#),
            ("stick", r#", "collide": {"plane": -0.5, "mode": "stick"}"#),
            ("noise+stick", r#", "noise": {"strength": 0.3}, "collide": {"plane": -0.5, "mode": "stick"}"#),
            ("kill", r#", "collide": {"plane": -0.5, "mode": "kill"}"#),
        ] {
            let e = effect(&format!(r#"{{"emitters": [{{{base}{extra}}}]}}"#));
            let inst = Instance { effect: &e, params: Params::default(), seed: 3, start: 0.0, placement: Placement::at([0.0; 3]), track: None, ground: None };
            let mut out = Vec::with_capacity(8000);
            let mut s = EvalStats::default();
            let start = std::time::Instant::now();
            for i in 0..50 {
                out.clear();
                s = EvalStats::default();
                evaluate(&inst, 10.0 + i as f64 / 60.0, &[EmitterPlan::default()], usize::MAX, u64::MAX, &mut out, &mut s);
            }
            let ms = start.elapsed().as_secs_f64() * 1e3 / 50.0;
            eprintln!("{label}: {} drawn, {} visited, work {}: {:.3} ms, {:.1} ns per visit, {:.2} ns per work unit", s.drawn, s.visited, s.work, ms, ms * 1e6 / s.visited as f64, ms * 1e6 / s.work.max(1) as f64);
        }
        // Thinned: visits only.
        let e = effect(&format!(r#"{{"emitters": [{{{base}}}]}}"#));
        let inst = Instance { effect: &e, params: Params::default(), seed: 3, start: 0.0, placement: Placement::at([0.0; 3]), track: None, ground: None };
        let mut out = Vec::new();
        let mut s = EvalStats::default();
        let start = std::time::Instant::now();
        for i in 0..50 {
            s = EvalStats::default();
            evaluate(&inst, 10.0 + i as f64 / 60.0, &[EmitterPlan { keep: 1.0, ..Default::default() }], 0, u64::MAX, &mut out, &mut s);
        }
        let ms = start.elapsed().as_secs_f64() * 1e3 / 50.0;
        eprintln!("dropped (limit 0): {} visited: {:.1} ns per visit", s.visited, ms * 1e6 / s.visited as f64);
    }

    #[test]
    #[ignore = "timing report"]
    #[allow(clippy::disallowed_methods, reason = "a timing report, outside the results")]
    fn eval_cost_of_5000_particles() {
        let e = effect(r##"{"emitters": [{"rate": 2000, "lifetime": [2, 3], "speed": [1, 2], "spread_deg": 40, "gravity": [0, 1, 0], "drag": 1, "noise": {"strength": 0.3}, "size": [[0, 0.1], [1, 0.3]], "color": [[0, "#ffd27a"], [1, "#c2410c"]]}]}"##);
        let inst = Instance { effect: &e, params: Params::default(), seed: 3, start: 0.0, placement: Placement::at([0.0; 3]), track: None, ground: None };
        let mut out = Vec::with_capacity(8000);
        let mut s = EvalStats::default();
        evaluate(&inst, 10.0, &[EmitterPlan::default()], usize::MAX, u64::MAX, &mut out, &mut s);
        let start = std::time::Instant::now();
        for i in 0..50 {
            out.clear();
            evaluate(&inst, 10.0 + i as f64 / 60.0, &[EmitterPlan::default()], usize::MAX, u64::MAX, &mut out, &mut s);
        }
        eprintln!("evaluate: {} particles in {:.3} ms", out.len(), start.elapsed().as_secs_f64() * 1e3 / 50.0);
        let quiet = effect(r##"{"emitters": [{"rate": 2000, "lifetime": [2, 3], "speed": [1, 2], "spread_deg": 40, "gravity": [0, 1, 0], "drag": 1, "size": [[0, 0.1], [1, 0.3]], "color": [[0, "#ffd27a"], [1, "#c2410c"]]}]}"##);
        let inst = Instance { effect: &quiet, ..inst };
        let start = std::time::Instant::now();
        for i in 0..50 {
            out.clear();
            evaluate(&inst, 10.0 + i as f64 / 60.0, &[EmitterPlan::default()], usize::MAX, u64::MAX, &mut out, &mut s);
        }
        eprintln!("evaluate without noise: {} particles in {:.3} ms", out.len(), start.elapsed().as_secs_f64() * 1e3 / 50.0);
    }
}
