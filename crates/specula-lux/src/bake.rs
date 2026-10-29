//! The bake: validity and relocation, one traced pass with a hit cache, then bounces lit by the
//! previous pass's probes (docs/design/GI.md, "The bake").
//!
//! **Deterministic.** Every probe's result depends only on the scene, the request and its own
//! index (its rays are the Fibonacci set turned by a rotation drawn from the index). Workers take
//! chunks of probes in any order and the results are put back by index, so the thread count and
//! timing never change a byte.

use crate::field::{mask_bit, Field, Lattice, Probe, ALL_VISIBLE};
use crate::scene::{Prepared, Scene};
use crate::sh::{self, Accum};
use crate::half;
use specula_core::math::Vec3;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// Most probes in one volume.
pub const MAX_PROBES: usize = 65_536;
/// Most triangles a bake traces.
pub const MAX_TRIANGLES: usize = 1 << 20;
/// Most point lights a bake lights with.
pub const MAX_LIGHTS: usize = 256;
pub const MIN_RAYS: u32 = 16;
pub const MAX_RAYS: u32 = 1024;
pub const MAX_BOUNCES: u32 = 4;
pub const MIN_SPACING: f32 = 0.25;
pub const MAX_SPACING: f32 = 8.0;
/// Most ray casts a bake may plan (`probes × (validity rays + rays × (bounces + 2))`).
pub const MAX_WORK: u64 = 1 << 30;
/// Most memory the hit cache may take.
pub const MAX_CACHE_BYTES: u64 = 256 << 20;
/// Rays per probe of the inside-geometry test.
pub const VALIDITY_RAYS: usize = 64;
/// A probe more than this share of whose rays hit back faces is inside geometry.
pub const INSIDE_SHARE: f32 = 0.25;
/// Relocation moves a probe at most this many spacings per axis.
pub const MAX_RELOCATION: f32 = 0.6;
/// Probes per work item handed to a thread.
const CHUNK: usize = 16;
/// Bytes of one cached hit.
const HIT_BYTES: u64 = 20;

/// What to bake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Metres between probes.
    pub spacing: f32,
    /// Rays per probe and pass.
    pub rays: u32,
    /// Passes after the first (each adds a bounce of indirect light).
    pub bounces: u32,
    /// Most luminance one ray brings (0: no limit): small, bright emitters only a few rays hit
    /// would make blotches (fireflies); clamped, they can't.
    pub clamp: f32,
}

/// A refused or failed bake, as data: what was wrong and what is allowed.
#[derive(Clone, Debug, PartialEq)]
pub struct BakeError {
    pub code: &'static str,
    pub message: String,
    pub field: Option<&'static str>,
    pub value: Option<f64>,
    pub limit: Option<f64>,
}

impl BakeError {
    fn new(code: &'static str, message: impl Into<String>) -> BakeError {
        BakeError { code, message: message.into(), field: None, value: None, limit: None }
    }

    fn limit(field: &'static str, value: f64, limit: f64, message: impl Into<String>) -> BakeError {
        BakeError { code: "over_limit", message: message.into(), field: Some(field), value: Some(value), limit: Some(limit) }
    }
}

impl std::fmt::Display for BakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// The lattice and the cost of a request, checked against the limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub lattice: Lattice,
    pub work: u64,
    pub cache_bytes: u64,
}

/// Checks `req` and works out its lattice and cost; nothing is traced.
pub fn plan(req: &Request) -> Result<Plan, BakeError> {
    if !(MIN_SPACING..=MAX_SPACING).contains(&req.spacing) {
        return Err(BakeError::limit("spacing", f64::from(req.spacing), f64::from(MAX_SPACING), format!("'spacing' is {MIN_SPACING} to {MAX_SPACING} m between probes")));
    }
    if !(MIN_RAYS..=MAX_RAYS).contains(&req.rays) {
        return Err(BakeError::limit("rays", f64::from(req.rays), f64::from(MAX_RAYS), format!("'rays' is {MIN_RAYS} to {MAX_RAYS} per probe")));
    }
    if !(req.clamp.is_finite() && req.clamp >= 0.0) {
        return Err(BakeError::limit("clamp", f64::from(req.clamp), 1.0e4, "'clamp' is a luminance of 0 (none) or more"));
    }
    if req.bounces > MAX_BOUNCES {
        return Err(BakeError::limit("bounces", f64::from(req.bounces), f64::from(MAX_BOUNCES), format!("'bounces' is 0 to {MAX_BOUNCES}")));
    }
    for a in 0..3 {
        if !(req.min[a].is_finite() && req.max[a].is_finite()) || req.max[a] < req.min[a] {
            return Err(BakeError { field: Some("min"), ..BakeError::new("bad_bounds", "the box needs finite 'min' ≤ 'max' on every axis") });
        }
        if req.max[a] - req.min[a] > 4096.0 {
            return Err(BakeError::limit("max", f64::from(req.max[a] - req.min[a]), 4096.0, "a volume is at most 4096 m across: split the area into several volumes"));
        }
    }
    let lattice = Lattice::covering(req.min, req.max, req.spacing);
    let probes = lattice.count();
    if probes > MAX_PROBES {
        let d = lattice.dims;
        return Err(BakeError::limit(
            "spacing",
            probes as f64,
            MAX_PROBES as f64,
            format!("{probes} probes ({} x {} x {}) is more than {MAX_PROBES}: raise 'spacing', shrink the box, or split it into several volumes", d[0], d[1], d[2]),
        ));
    }
    let work = probes as u64 * (VALIDITY_RAYS as u64 + u64::from(req.rays) * (u64::from(req.bounces) + 2));
    if work > MAX_WORK {
        return Err(BakeError::limit("rays", work as f64, MAX_WORK as f64, format!("{work} ray casts is more than {MAX_WORK}: fewer 'rays' or 'bounces', or a larger 'spacing'")));
    }
    let cache_bytes = probes as u64 * u64::from(req.rays) * HIT_BYTES;
    if cache_bytes > MAX_CACHE_BYTES {
        return Err(BakeError::limit("rays", cache_bytes as f64, MAX_CACHE_BYTES as f64, format!("the hit cache would take {} MB (at most {}): fewer 'rays' or a larger 'spacing'", cache_bytes >> 20, MAX_CACHE_BYTES >> 20)));
    }
    Ok(Plan { lattice, work, cache_bytes })
}

/// Steps of a bake, for progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepare = 0,
    Validity = 1,
    Visibility = 2,
    Trace = 3,
    Bounce = 4,
    Done = 5,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Prepare => "prepare",
            Phase::Validity => "validity",
            Phase::Visibility => "visibility",
            Phase::Trace => "trace",
            Phase::Bounce => "bounce",
            Phase::Done => "done",
        }
    }

    fn from_u32(v: u32) -> Phase {
        match v {
            0 => Phase::Prepare,
            1 => Phase::Validity,
            2 => Phase::Visibility,
            3 => Phase::Trace,
            4 => Phase::Bounce,
            _ => Phase::Done,
        }
    }
}

/// Shared with whoever runs the bake: cancel it, read its progress.
#[derive(Debug, Default)]
pub struct Control {
    cancel: AtomicBool,
    phase: AtomicU32,
    bounce: AtomicU32,
    done: AtomicU64,
    total: AtomicU64,
}

/// Progress as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub phase: Phase,
    /// The bounce running (1-based) during [`Phase::Bounce`].
    pub bounce: u32,
    /// Probes done of the phase's total.
    pub done: u64,
    pub total: u64,
}

impl Control {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> Progress {
        Progress {
            phase: Phase::from_u32(self.phase.load(Ordering::Relaxed)),
            bounce: self.bounce.load(Ordering::Relaxed),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }
    }

    fn enter(&self, phase: Phase, bounce: u32, total: usize) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total as u64, Ordering::Relaxed);
        self.bounce.store(bounce, Ordering::Relaxed);
        self.phase.store(phase as u32, Ordering::Relaxed);
    }
}

/// What a bake did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub probes: usize,
    pub valid: usize,
    pub relocated: usize,
    pub invalid: usize,
    pub triangles: usize,
    pub lights: usize,
    /// Primary rays cast (validity, trace); shadow rays aren't counted.
    pub rays: u64,
    /// Milliseconds: prepare (BVH), validity, trace, bounces, all.
    pub ms: [f64; 5],
}

/// A finished bake: the field, quantized to what the GPU reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Baked {
    pub field: Field,
    pub stats: Stats,
}

#[allow(clippy::disallowed_methods, reason = "profiling only: the times go in the report, never in the baked bytes")]
fn now() -> std::time::Instant {
    std::time::Instant::now()
}

fn ms_since(t: std::time::Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// `f(i)` for `i` in `0..n` on `threads` threads, results in index order whatever ran when.
#[allow(clippy::disallowed_methods, reason = "work split by index and merged in index order: thread timing never changes the results")]
fn parallel<T: Send>(threads: usize, n: usize, control: &Control, f: impl Fn(usize) -> T + Sync) -> Result<Vec<T>, BakeError> {
    let next = AtomicUsize::new(0);
    let run = || {
        let mut out: Vec<(usize, Vec<T>)> = Vec::new();
        loop {
            if control.cancelled() {
                break;
            }
            let c = next.fetch_add(1, Ordering::Relaxed);
            let start = c * CHUNK;
            if start >= n {
                break;
            }
            let end = (start + CHUNK).min(n);
            out.push((c, (start..end).map(&f).collect()));
            control.done.fetch_add((end - start) as u64, Ordering::Relaxed);
        }
        out
    };
    let mut parts: Vec<(usize, Vec<T>)> = if threads <= 1 {
        run()
    } else {
        std::thread::scope(|s| {
            let workers: Vec<_> = (1..threads).map(|_| s.spawn(run)).collect();
            let mut all = run();
            for w in workers {
                match w.join() {
                    Ok(part) => all.extend(part),
                    Err(p) => std::panic::resume_unwind(p),
                }
            }
            all
        })
    };
    if control.cancelled() {
        return Err(BakeError::new("cancelled", "the bake was cancelled"));
    }
    parts.sort_unstable_by_key(|(c, _)| *c);
    let mut out = Vec::with_capacity(n);
    for (_, p) in parts {
        out.extend(p);
    }
    Ok(out)
}

/// A cached hit: `t` (> 0 front face, < 0 back face, infinite: escaped), the surface normal
/// (octahedral, 16 bits a coordinate), albedo and direct-plus-emitted radiance as halves.
type CachedHit = [u32; 5];

fn oct_encode(n: Vec3) -> u32 {
    let l1 = n.x.abs() + n.y.abs() + n.z.abs();
    let (mut x, mut y) = (n.x / l1, n.y / l1);
    if n.z < 0.0 {
        let (ax, ay) = (x.abs(), y.abs());
        x = (1.0 - ay) * if x >= 0.0 { 1.0 } else { -1.0 };
        y = (1.0 - ax) * if y >= 0.0 { 1.0 } else { -1.0 };
    }
    let q = |v: f32| ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * 65535.0 + 0.5) as u32;
    q(x) | (q(y) << 16)
}

fn oct_decode(w: u32) -> Vec3 {
    let x = (w & 0xffff) as f32 / 65535.0 * 2.0 - 1.0;
    let y = (w >> 16) as f32 / 65535.0 * 2.0 - 1.0;
    let z = 1.0 - x.abs() - y.abs();
    let (mut x, mut y) = (x, y);
    if z < 0.0 {
        let (ax, ay) = (x.abs(), y.abs());
        x = (1.0 - ay) * if x >= 0.0 { 1.0 } else { -1.0 };
        y = (1.0 - ax) * if y >= 0.0 { 1.0 } else { -1.0 };
    }
    Vec3::new(x, y, z).normalize()
}

/// The direction of ray `j` of probe `index`.
fn ray_dir(rot: [f32; 4], dirs: &[Vec3], j: usize) -> Vec3 {
    sh::rotate(rot, dirs[j])
}

/// The rotation a probe turns its rays by (the validity test uses another).
fn probe_rotation(index: usize, salt: u64) -> [f32; 4] {
    sh::rotation((index as u64) << 8 | salt)
}

/// Ray casts a bake of `plan` with `lights` point lights makes: [`Plan::work`] plus a shadow ray
/// per light at every first hit (the sun's is in `work`).
pub fn work_with_lights(plan: &Plan, rays: u32, lights: usize) -> u64 {
    plan.work.saturating_add((plan.lattice.count() as u64).saturating_mul(u64::from(rays)).saturating_mul(lights as u64))
}

/// Refuses a bake whose ray casts with its lights' shadow rays pass [`MAX_WORK`].
pub fn check_work(plan: &Plan, rays: u32, lights: usize) -> Result<(), BakeError> {
    let work = work_with_lights(plan, rays, lights);
    if work > MAX_WORK {
        return Err(BakeError::limit(
            "lights",
            work as f64,
            MAX_WORK as f64,
            format!("{work} ray casts with {lights} point lights' shadow rays is more than {MAX_WORK}: fewer 'rays', a larger 'spacing', or fewer lights in the box"),
        ));
    }
    Ok(())
}

/// Bakes `scene` as `req` asks, on `threads` threads (1 runs on the caller's).
pub fn bake(scene: &Scene, req: &Request, threads: usize, control: &Control) -> Result<Baked, BakeError> {
    let started = now();
    let plan = plan(req)?;
    if scene.triangles.len() > MAX_TRIANGLES {
        return Err(BakeError::limit(
            "triangles",
            scene.triangles.len() as f64,
            MAX_TRIANGLES as f64,
            format!("{} triangles is more than {MAX_TRIANGLES}: shrink the box or 'margin', or exclude detailed subtrees", scene.triangles.len()),
        ));
    }
    if scene.lights.len() > MAX_LIGHTS {
        return Err(BakeError::limit("lights", scene.lights.len() as f64, MAX_LIGHTS as f64, format!("{} point lights is more than {MAX_LIGHTS}: shrink the box", scene.lights.len())));
    }
    check_work(&plan, req.rays, scene.lights.len())?;
    let threads = threads.clamp(1, 64);
    let mut stats = Stats::default();
    control.enter(Phase::Prepare, 0, 1);
    let t = now();
    let Some(prepared) = scene.prepare_with(&|| control.cancelled()) else {
        return Err(BakeError::new("cancelled", "the bake was cancelled"));
    };
    stats.triangles = prepared.bvh.len();
    stats.lights = prepared.lights.len();
    stats.ms[0] = ms_since(t);
    let lattice = plan.lattice;
    let n = lattice.count();
    let s = lattice.spacing;
    stats.probes = n;

    // Validity and relocation.
    control.enter(Phase::Validity, 0, n);
    let t = now();
    let vdirs = sh::fibonacci(VALIDITY_RAYS);
    let placed = parallel(threads, n, control, |i| place(&prepared, &vdirs, lattice.position(i), i, s))?;

    stats.rays += (n * VALIDITY_RAYS) as u64;

    // Each cell's visibility: which corner probes each of its octants sees.
    control.enter(Phase::Visibility, 0, n);
    let masks = parallel(threads, n, control, |i| cell_mask(&prepared, &lattice, &placed, i))?;
    let reaches = parallel(threads, n, control, |i| reach(&prepared, lattice.position(i) + placed[i].0, placed[i].1, s))?;
    stats.rays += (n * (64 + 14)) as u64;
    stats.ms[1] = ms_since(t);

    // The traced pass: hits cached, and the first bounce.
    control.enter(Phase::Trace, 0, n);
    let t = now();
    let dirs = sh::fibonacci(req.rays as usize);
    let rays = dirs.len();
    let traced = parallel(threads, n, control, |i| {
        let (offset, weight, _) = placed[i];
        if weight <= 0.0 {
            return (Vec::new(), Accum::default().finish());
        }
        trace(&prepared, &dirs, lattice.position(i) + offset, probe_rotation(i, 1), req.clamp, control)
    })?;
    stats.ms[2] = ms_since(t);
    stats.rays += placed.iter().filter(|p| p.1 > 0.0).count() as u64 * rays as u64;

    let mut probes: Vec<Probe> = (0..n)
        .map(|i| {
            let (offset, weight, _) = placed[i];
            Probe { sh: traced[i].1, mask: masks[i], offset: [offset.x, offset.y, offset.z], weight, reach: reaches[i] }
        })
        .collect();
    // Bounces: each lights the cached hits with the previous pass's probes.
    let t = now();
    for b in 1..=req.bounces {
        control.enter(Phase::Bounce, b, n);
        let field = Field { lattice, probes: probes.clone() };
        let next = parallel(threads, n, control, |i| {
            if placed[i].1 <= 0.0 {
                return probes[i].sh;
            }
            bounce(&prepared, &field, &dirs, &traced[i].0, lattice.position(i) + placed[i].0, probe_rotation(i, 1), req.clamp)
        })?;
        for (p, sh) in probes.iter_mut().zip(next) {
            p.sh = sh;
        }
    }
    // One denoising pass: each probe averaged with its six neighbours that it can see (the
    // segment between them hits nothing) and that are valid, its own value counted twice. Monte
    // Carlo noise made probe-sized blotches on lit walls; light never crosses a wall this way.
    let field = Field { lattice, probes: probes.clone() };
    let smoothed = parallel(threads, n, control, |i| smooth(&prepared, &field, &placed, i))?;
    for (p, sh) in probes.iter_mut().zip(smoothed) {
        p.sh = sh;
    }
    stats.ms[3] = ms_since(t);
    for (p, placed) in probes.iter_mut().zip(&placed) {
        *p = p.quantized();
        if placed.2 {
            stats.relocated += 1;
        }
        if p.weight > 0.0 {
            stats.valid += 1;
        } else {
            stats.invalid += 1;
        }
    }
    control.enter(Phase::Done, req.bounces, 0);
    stats.ms[4] = ms_since(started);
    Ok(Baked { field: Field { lattice, probes }, stats })
}

/// Validity and relocation of the probe at `nominal`: `(offset, weight, relocated)`.
fn place(p: &Prepared, vdirs: &[Vec3], nominal: Vec3, index: usize, s: f32) -> (Vec3, f32, bool) {
    let rot = probe_rotation(index, 2);
    let mut off = Vec3::ZERO;
    let mut relocated = false;
    for _ in 0..3 {
        let pos = nominal + off;
        let mut back = 0usize;
        let mut nearest_back: Option<(f32, Vec3)> = None;
        for j in 0..vdirs.len() {
            let d = ray_dir(rot, vdirs, j);
            if let Some(h) = p.bvh.closest(pos, d, 1e-4, 1.0e5) {
                if p.surfaces[h.tri as usize].normal.dot(d) > 0.0 {
                    back += 1;
                    if nearest_back.is_none_or(|(t, _)| h.t < t) {
                        nearest_back = Some((h.t, d));
                    }
                }
            }
        }
        if (back as f32) <= INSIDE_SHARE * vdirs.len() as f32 {
            return (off, 1.0, relocated);
        }
        let Some((t, d)) = nearest_back else { break };
        let moved = off + d * (t + 0.05 * s);
        let lim = MAX_RELOCATION * s;
        let clamped = Vec3::new(moved.x.clamp(-lim, lim), moved.y.clamp(-lim, lim), moved.z.clamp(-lim, lim));
        if clamped == off {
            break;
        }
        off = clamped;
        relocated = true;
    }
    (off, 0.0, relocated)
}

/// Whether the segment `a`..`b` is clear: nothing crosses it, and it doesn't run inside a solid
/// (both ends on one solid's surfaces, as an octant centre on a wall's inner edge and a probe on its
/// outer edge are: such a segment crosses no face between its ends, yet it passes through the
/// wall). The midpoint's inside test casts two rays; a solid there faces both away.
fn clear(p: &Prepared, a: Vec3, b: Vec3) -> bool {
    let seg = b - a;
    let d = seg.length();
    if d <= 1e-5 {
        return true;
    }
    if p.bvh.occluded(a, seg * (1.0 / d), 1e-4, d - 1e-4) {
        return false;
    }
    !inside(p, a + seg * 0.5)
}

/// Whether `q` is inside a solid: of three rays in directions off the axes (so they run along no
/// face of a box), at least two first hit a back face. A majority, not all: stacked boxes share
/// faces (a wall's top and the roof's underside), and a ray through the shared face may find
/// either.
fn inside(p: &Prepared, q: Vec3) -> bool {
    const DIRS: [Vec3; 3] = [
        Vec3::new(0.267_261_24, 0.534_522_5, 0.801_783_7),
        Vec3::new(-0.666_666_7, -0.333_333_3, 0.666_666_7),
        Vec3::new(0.408_248_3, -0.816_496_6, -0.408_248_3),
    ];
    DIRS.iter().filter(|&&d| p.bvh.closest(q, d, 1e-5, 1.0e5).is_some_and(|h| p.surfaces[h.tri as usize].normal.dot(d) > 0.0)).count() >= 2
}

/// The visibility mask of the cell whose lowest corner is probe `i` (see
/// [`crate::field::mask_bit`]): a corner probe is hidden from an octant when the segment from the
/// octant's centre to the probe (where it was moved to) isn't [`clear`]. Exact at the centre only:
/// a wall through the octant puts points of both sides in it, and the centre speaks for its own
/// side; the probes' reaches ([`crate::field::Probe::reaches`]) catch most of the other side's
/// points. Cells on the far faces have no mask (no point samples them).
fn cell_mask(p: &Prepared, l: &Lattice, placed: &[(Vec3, f32, bool)], i: usize) -> u64 {
    let c = l.coords(i);
    if (0..3).any(|a| c[a] + 1 >= l.dims[a]) {
        return ALL_VISIBLE;
    }
    let s = l.spacing;
    let lo = l.at(c);
    let mut mask = ALL_VISIBLE;
    for octant in 0..8u32 {
        let centre = lo + Vec3::new(0.25 + 0.5 * (octant & 1) as f32, 0.25 + 0.5 * ((octant >> 1) & 1) as f32, 0.25 + 0.5 * (octant >> 2) as f32) * s;
        for corner in 0..8u32 {
            let cc = [c[0] + (corner & 1), c[1] + ((corner >> 1) & 1), c[2] + (corner >> 2)];
            let (offset, weight, _) = placed[l.index(cc[0], cc[1], cc[2])];
            if weight <= 0.0 {
                continue; // never used: its bit doesn't matter
            }
            if !clear(p, centre, l.at(cc) + offset) {
                mask &= !mask_bit(octant, corner);
            }
        }
    }
    mask
}

/// How far a probe at `pos` sees along each reach direction ([`Probe::reach`]), rounded down to
/// eighths of `s`; invalid probes see nothing (never used).
fn reach(p: &Prepared, pos: Vec3, weight: f32, s: f32) -> [u8; 14] {
    if weight <= 0.0 {
        return [0; 14];
    }
    let none = crate::field::REACH_NONE;
    let far = f32::from(none) * s / 8.0;
    crate::field::REACH_DIRS.map(|d| match p.bvh.closest(pos, Vec3::new(d[0], d[1], d[2]), 1e-4, far) {
        Some(h) => specula_core::dmath::floor(h.t * 8.0 / s).clamp(0.0, f32::from(none - 1)) as u8,
        None => none,
    })
}

/// `c` scaled down to luminance `max` at most (0: unchanged).
fn clamped(c: [f32; 3], max: f32) -> [f32; 3] {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    if max > 0.0 && y > max {
        let k = max / y;
        return c.map(|x| x * k);
    }
    c
}

/// Probe `i`'s SH averaged with its visible, valid axis neighbours (its own counted twice).
fn smooth(p: &Prepared, field: &Field, placed: &[(Vec3, f32, bool)], i: usize) -> crate::sh::Sh {
    let l = &field.lattice;
    let own = field.probes[i].sh;
    if placed[i].1 <= 0.0 {
        return own;
    }
    let c = l.coords(i);
    let at = l.position(i) + placed[i].0;
    let mut acc = own;
    let mut w = 2.0f32;
    let scale = |s: &mut crate::sh::Sh, k: f32| {
        for ch in 0..3 {
            s.c0[ch] *= k;
            for a in 0..3 {
                s.c1[a][ch] *= k;
            }
        }
    };
    scale(&mut acc, 2.0);
    for (axis, step) in [(0usize, -1i64), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)] {
        let mut nc = [c[0] as i64, c[1] as i64, c[2] as i64];
        nc[axis] += step;
        if nc[axis] < 0 || nc[axis] >= i64::from(l.dims[axis]) {
            continue;
        }
        let j = l.index(nc[0] as u32, nc[1] as u32, nc[2] as u32);
        if placed[j].1 <= 0.0 {
            continue;
        }
        if !clear(p, at, l.position(j) + placed[j].0) {
            continue;
        }
        let s = field.probes[j].sh;
        for ch in 0..3 {
            acc.c0[ch] += s.c0[ch];
            for a in 0..3 {
                acc.c1[a][ch] += s.c1[a][ch];
            }
        }
        w += 1.0;
    }
    scale(&mut acc, 1.0 / w);
    acc
}

/// The traced pass for a probe at `pos`: its hit cache and the first bounce's SH.
fn trace(p: &Prepared, dirs: &[Vec3], pos: Vec3, rot: [f32; 4], clamp: f32, control: &Control) -> (Vec<CachedHit>, crate::sh::Sh) {
    let mut cache = Vec::with_capacity(dirs.len());
    let mut acc = Accum::default();
    for j in 0..dirs.len() {
        // A probe's rays with hundreds of lights' shadow rays take a while: a cancel stops within
        // them (the result is thrown away).
        if j % 32 == 0 && control.cancelled() {
            break;
        }
        let d = ray_dir(rot, dirs, j);
        let (rec, radiance) = match p.bvh.closest(pos, d, 1e-4, 1.0e5) {
            None => ([f32::INFINITY.to_bits(), 0, 0, 0, 0], p.sky.radiance(d)),
            Some(h) => {
                let surf = &p.surfaces[h.tri as usize];
                if surf.normal.dot(d) > 0.0 {
                    ([(-h.t).to_bits(), 0, 0, 0, 0], [0.0; 3])
                } else {
                    let hp = pos + d * h.t;
                    let e = p.direct(hp, surf.normal);
                    let a = surf.albedo;
                    let base = [surf.emissive[0] + a[0] * e[0], surf.emissive[1] + a[1] * e[1], surf.emissive[2] + a[2] * e[2]];
                    // What later passes read back: rounded to halves, so this pass agrees with them.
                    let base = base.map(half::round);
                    let rec = [h.t.to_bits(), oct_encode(surf.normal), half::pack2(a[0], a[1]), half::pack2(a[2], base[0]), half::pack2(base[1], base[2])];
                    (rec, base)
                }
            }
        };
        cache.push(rec);
        acc.add(d, clamped(radiance, clamp));
    }
    (cache, acc.finish())
}

/// A later pass for one probe: its cached hits lit by `field` (the previous pass).
fn bounce(p: &Prepared, field: &Field, dirs: &[Vec3], cache: &[CachedHit], pos: Vec3, rot: [f32; 4], clamp: f32) -> crate::sh::Sh {
    let mut acc = Accum::default();
    let flat = p.sky.radiance;
    for (j, rec) in cache.iter().enumerate() {
        let d = ray_dir(rot, dirs, j);
        let t = f32::from_bits(rec[0]);
        let radiance = if t.is_infinite() {
            p.sky.radiance(d)
        } else if t < 0.0 {
            [0.0; 3]
        } else {
            let n = oct_decode(rec[1]);
            let (ar, ag) = half::unpack2(rec[2]);
            let (ab, b0) = half::unpack2(rec[3]);
            let (b1, b2) = half::unpack2(rec[4]);
            let hp = pos + d * t;
            let smp = field.sample(hp, n, -d, true);
            let mut e = [0.0; 3];
            for k in 0..3 {
                e[k] = flat[k] + (smp.irradiance[k] - flat[k]) * smp.fade;
            }
            [b0 + ar * e[0], b1 + ag * e[1], b2 + ab * e[2]]
        };
        acc.add(d, clamped(radiance, clamp));
    }
    acc.finish()
}

#[cfg(test)]
mod tests;
