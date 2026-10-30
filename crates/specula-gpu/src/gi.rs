//! Baked GI on the GPU (docs/design/GI.md): the resident probe volumes in one buffer, read by
//! `shaders/gi.glsl` through its device address.
//!
//! A frame names its volumes ([`GiFrame`]) with a `key` the caller changes whenever the set or
//! its contents change. The buffer is rebuilt only then: a new host-visible (device-local when
//! possible) buffer is written and the old one retired once no submitted frame reads it, so a
//! window frame in flight never sees a half-written buffer.
//!
//! Layout (16-byte units): one header (`volume count, first lean probe, 0, 0`), then per volume
//! three records (`origin xyz, spacing`; `dims xyz, first probe`; `intensity, 0, 0, 0`), then the
//! probes, each three records of 32-bit words (`specula_lux::Record`: the SH and offset as
//! halves, its cell's visibility mask, its reaches), then the lean probes, one record each
//! ([`LeanProbe`]: what tier low reads per pixel on wide draws and terrain).

use crate::device::{Buffer, Device};
use crate::stream::{Retired, Uploads};
use crate::vk::*;
use crate::{GpuError, Limit, MemoryKind};
use std::ptr;

/// Volumes (streamed chunks) a frame may sample.
pub const MAX_GI_VOLUMES: usize = 16;
/// Probes resident at once, over every volume (64 bytes each with the lean copy: 32 MB).
pub const MAX_GI_PROBES: usize = 1 << 19;
/// GPU bytes a resident probe takes: its record (48) and its lean copy (16).
pub const GI_PROBE_BYTES: usize = 64;

/// What the probes show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GiView {
    /// Lit as usual, the probes' irradiance as the ambient term.
    #[default]
    Lit,
    /// Only the indirect light (albedo 1, no direct light): what the probes contribute.
    Irradiance,
}

/// How the shaders sample (the cells' visibility masks apply in every tier: none leaks more).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GiQuality {
    /// Every lit pixel samples its eight probes (discrete GPUs, strong machines).
    #[default]
    High,
    /// Its own pipelines without the full per-pixel sampler (its registers cost the iGPU's
    /// fragment shaders their occupancy even when no pixel ran it). Draws up to [`LOW_SPAN`]
    /// spacings get an ambient cube the CPU samples once (six faces, each half a spacing out from
    /// the draw's box: a wall's inner face reads the room, its outer face the yard); wider ones
    /// (ground slabs, a hollow building) and terrain read the probes per pixel through the lean
    /// copy (one word a probe) with the cells' visibility masks, but without the reaches and the
    /// backface weights. The iGPU's default.
    Low,
}

/// Draws whose bounding radius exceeds this many probe spacings read the lean probes per pixel
/// at `Low`; smaller ones get an ambient cube. (A middle tier, the full sampler for wide draws
/// only, was dropped: any pipeline carrying it pays its registers on every draw it shades, so on
/// the iGPU it cost as much as `High`. A CPU grid over wide draws came before the lean path: it
/// blended samples across walls, and moving wide draws re-sampled it every frame.)
pub const LOW_SPAN: f32 = 2.0;

impl GiQuality {
    /// The radius (in spacings) past which a draw isn't sampled once (`None`: every draw per pixel).
    pub fn per_draw_span(self) -> Option<f32> {
        match self {
            GiQuality::High => None,
            GiQuality::Low => Some(LOW_SPAN),
        }
    }

    /// Whether this tier's pipelines carry the per-pixel sampler (`FEAT_GI_PIXEL`).
    pub fn per_pixel(self) -> bool {
        self != GiQuality::Low
    }
}

/// One draw's GI as the shader reads it (`shaders/gi.glsl`, `GiDraws`, three 16-byte words).
/// Word 11 is the mode: 0 the flat ambient (or per pixel at `High`); [`DRAW_CUBE`]: the ambient
/// cube, irradiance facing +x, -x, +y, -y, +z, -z as rgb halves in words 0..9; [`DRAW_LEAN`]: the
/// lean probes per pixel (a wide draw).
pub type DrawGi = [u32; 12];
pub const DRAW_CUBE: u32 = 1;
pub const DRAW_LEAN: u32 = 3;

/// A probe as tier low reads it per pixel (one 16-byte record): the mean irradiance (c0 rgb) and
/// the directional term's luminance (c1 x, y, z) as halves, and its validity weight (f32). The
/// direction keeps the mean's colour: wide draws are ground and walls, lit mostly by the sky.
pub type LeanProbe = [u32; 4];

/// A probe record's lean copy.
fn lean(r: &[u32; 12]) -> LeanProbe {
    let mut sh = [0.0f32; 12];
    for k in 0..6 {
        (sh[2 * k], sh[2 * k + 1]) = halves(r[k]);
    }
    let h = |a: f32, b: f32| u32::from(crate::effects::f16_bits(a)) | (u32::from(crate::effects::f16_bits(b)) << 16);
    let lum = |r: f32, g: f32, b: f32| 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let c1 = [lum(sh[3], sh[4], sh[5]), lum(sh[6], sh[7], sh[8]), lum(sh[9], sh[10], sh[11])];
    let weight = halves(r[7]).1;
    [h(sh[0], sh[1]), h(sh[2], c1[0]), h(c1[1], c1[2]), if weight.is_finite() { weight.max(0.0) } else { 0.0 }.to_bits()]
}

fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let mant = f32::from(h & 0x3ff);
    match exp {
        0 => sign * mant * (1.0 / 16_777_216.0),
        0x1f => {
            if mant == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + mant / 1024.0) * f32::from_bits(((exp - 15 + 127) as u32) << 23),
    }
}

fn halves(w: u32) -> (f32, f32) {
    (half(w as u16), half((w >> 16) as u16))
}

/// The probes' L1 SH at world point `p` (no surface: no bias, no backface weight), blended into
/// the flat ambient `flat` (linear) towards a volume's edge and outside every volume. The cell's
/// mask for `p`'s octant applies, so a prop inside a sealed room samples only the room's probes;
/// when it hides every valid probe (`p` inside a solid prop, its octant's centre in the prop too)
/// the valid probes are taken unmasked rather than going black, and with none valid (deep in
/// rock) the flat ambient. The same weights as `specula_lux::Field::sample` without a normal.
pub fn sample_sh(volumes: &[GiVolume], p: [f32; 3], flat: [f32; 3]) -> [f32; 12] {
    sample_sh_seen(volumes, p, flat).0
}

/// [`sample_sh`], and whether the point saw probes by the cells' masks and the reaches (false: it
/// fell back, as a point inside a solid or a wall does; false outside every volume too).
fn sample_sh_seen(volumes: &[GiVolume], p: [f32; 3], flat: [f32; 3]) -> ([f32; 12], bool) {
    let mut sh = [0.0f32; 12];
    sh[..3].copy_from_slice(&flat);
    let mut best: Option<(usize, f32)> = None;
    for (i, v) in volumes.iter().enumerate() {
        let out = (0..3).fold(0.0f32, |out, a| {
            let l = (p[a] - v.origin[a]) / v.spacing;
            out.max(-l).max(l - (v.dims[a] - 1) as f32)
        });
        if out < 1.0 && best.is_none_or(|(_, o)| out < o) {
            best = Some((i, out));
            if out <= 0.0 {
                break;
            }
        }
    }
    let Some((vi, out_by)) = best else { return (sh, false) };
    let v = &volumes[vi];
    let mut base = [0u32; 3];
    let mut f = [0.0f32; 3];
    for a in 0..3 {
        let m = (v.dims[a] - 1) as f32;
        let c = ((p[a] - v.origin[a]) / v.spacing).clamp(0.0, m);
        let b = c.floor().min((m - 1.0).max(0.0));
        base[a] = b as u32;
        f[a] = c - b;
    }
    let index = |c: [u32; 3]| (c[1] as usize * v.dims[2] as usize + c[2] as usize) * v.dims[0] as usize + c[0] as usize;
    let Some(cell) = v.probes.get(index(base)) else { return (sh, true) };
    let mask = u64::from(cell[8]) | (u64::from(cell[9]) << 32);
    let octant = u32::from(f[0] >= 0.5) | u32::from(f[1] >= 0.5) << 1 | u32::from(f[2] >= 0.5) << 2;
    let bits = (mask >> (octant * 8)) & 0xff;
    // (sums, weight, trilinear weight of the valid corners, of the ones seen); `strict` 2: the
    // cell's mask and the probes' reaches, 1: the reaches alone, 0: neither.
    let sum = |strict: u8| {
        let mut acc = [0.0f32; 12];
        let (mut w, mut tw_valid, mut tw_seen) = (0.0f32, 0.0f32, 0.0f32);
        for corner in 0..8u32 {
            let o = [corner & 1, (corner >> 1) & 1, corner >> 2];
            let c = [base[0] + o[0], base[1] + o[1], base[2] + o[2]];
            let Some(r) = v.probes.get(index(c)) else { continue };
            let ((ox, oy), (oz, weight)) = (halves(r[6]), halves(r[7]));
            if weight <= 0.0 {
                continue;
            }
            let tw = (0..3).fold(1.0f32, |t, a| t * if o[a] == 1 { f[a] } else { 1.0 - f[a] });
            tw_valid += tw;
            if tw <= 0.0 {
                continue;
            }
            if strict > 0 {
                let pos = [v.origin[0] + c[0] as f32 * v.spacing + ox, v.origin[1] + c[1] as f32 * v.spacing + oy, v.origin[2] + c[2] as f32 * v.spacing + oz];
                if (strict > 1 && bits >> corner & 1 == 0) || !reaches([r[10], r[11]], [p[0] - pos[0], p[1] - pos[1], p[2] - pos[2]], v.spacing) {
                    continue;
                }
            }
            tw_seen += tw;
            let wt = tw * weight;
            for k in 0..6 {
                let (a, b) = halves(r[k]);
                acc[2 * k] += wt * a;
                acc[2 * k + 1] += wt * b;
            }
            w += wt;
        }
        (acc, w, tw_valid, tw_seen)
    };
    // A point whose octant's mask hides everything (its octant centre in a wall, or inside a solid
    // prop) falls back to the reaches alone (a sealed room's point still sees only the room: the
    // outside probes' reaches end at its walls), then to every valid probe.
    let (mut acc, mut w, mut tw_valid, mut tw_seen) = sum(2);
    let seen = w > 0.0;
    if w <= 0.0 {
        (acc, w, tw_valid, tw_seen) = sum(1);
    }
    if w <= 0.0 {
        (acc, w, tw_valid, tw_seen) = sum(0);
    }
    if w <= 0.0 {
        return (sh, false);
    }
    let fade = (1.0 - out_by).clamp(0.0, 1.0);
    // A sliver of visible weight dims the result (`specula_lux::field::VISIBLE_SHARE`).
    let dim = (tw_seen / (tw_valid * 0.25)).min(1.0);
    for (k, (s, a)) in sh.iter_mut().zip(acc).enumerate() {
        let flat_part = if k < 3 { flat[k] * (1.0 - fade) } else { 0.0 };
        *s = a / w * v.intensity.max(0.0) * dim * fade + flat_part;
    }
    (sh, seen)
}

/// Whether a probe sees `d` (the point minus the probe) by its reaches (`r`: the record's last two
/// words, fourteen nibbles in eighths of the spacing `s`): the twin of
/// `specula_lux::field::Probe::reaches` (and of `gi_reaches` in `shaders/gi.glsl`).
fn reaches(r: [u32; 2], d: [f32; 3], s: f32) -> bool {
    let a = d.map(f32::abs);
    let face = a[0].max(a[1]).max(a[2]);
    let diag = (a[0] + a[1] + a[2]) * 0.577_350_26;
    let kf = if a[0] >= a[1] && a[0] >= a[2] {
        usize::from(d[0] < 0.0)
    } else if a[1] >= a[2] {
        2 + usize::from(d[1] < 0.0)
    } else {
        4 + usize::from(d[2] < 0.0)
    };
    let kd = 6 + usize::from(d[0] < 0.0) + 2 * usize::from(d[1] < 0.0) + 4 * usize::from(d[2] < 0.0);
    let ok = |k: usize, along: f32| {
        let v = (r[k / 8] >> ((k % 8) * 4)) & 15;
        v >= 15 || along <= (v as f32 + 1.0) * s * 0.125
    };
    // The nearer of the two must reach; the other too when nearly as near (REACH_TIE).
    let (near, far) = if face >= diag { ((kf, face), (kd, diag)) } else { ((kd, diag), (kf, face)) };
    ok(near.0, near.1) && (far.1 < near.1 * 0.95 || ok(far.0, far.1))
}

/// A draw's ambient cube ([`DRAW_CUBE`]) over its world box `lo..hi`: each face's irradiance
/// facing out, sampled half a spacing out from the face's centre (the faces' points see what the
/// faces see: a wall's inner face the room, its outer face the yard; one SH at the draw's centre
/// sat inside the wall and blended both).
pub fn sample_cube(volumes: &[GiVolume], lo: [f32; 3], hi: [f32; 3], spacing: f32, flat: [f32; 3]) -> DrawGi {
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let mut faces = [[0.0f32; 3]; 6];
    for (k, face) in faces.iter_mut().enumerate() {
        let (axis, sign) = (k / 2, if k % 2 == 0 { 1.0 } else { -1.0 });
        let mut p = c;
        p[axis] = if sign > 0.0 { hi[axis] } else { lo[axis] } + sign * 0.5 * spacing;
        let sh = sample_sh(volumes, p, flat);
        for (ch, f) in face.iter_mut().enumerate() {
            let v = sh[ch] + sign * sh[3 + 3 * axis + ch];
            *f = if v.is_finite() { v.max(0.0) } else { 0.0 };
        }
    }
    let h = |a: f32, b: f32| u32::from(crate::effects::f16_bits(a)) | (u32::from(crate::effects::f16_bits(b)) << 16);
    let f: Vec<f32> = faces.iter().flatten().copied().collect();
    let mut d = [0u32; 12];
    for (k, w) in d.iter_mut().take(9).enumerate() {
        *w = h(f[2 * k], f[2 * k + 1]);
    }
    d[11] = DRAW_CUBE;
    d
}

/// A wide draw's record: the shader reads the lean probes per pixel.
pub const fn lean_draw() -> DrawGi {
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, DRAW_LEAN]
}

/// Per-draw GI kept across frames by mesh, model matrix and box, for one GI set, flat ambient and
/// tier (all cleared when one changes). Entries a frame didn't use are swept after
/// [`DrawGiCache::KEEP_FRAMES`] frames, and the oldest go first past [`DrawGiCache::MAX_BYTES`]
/// (a moving draw adds a key a frame; a flood of new draws never empties the cache). A draw that
/// is exactly what the frame's draw at its index was last frame reuses that record without a
/// lookup.
#[derive(Default)]
pub struct DrawGiCache {
    key: (u64, [u32; 3], u8),
    entries: std::collections::HashMap<u64, (DrawGi, u64)>,
    frame: u64,
    /// Last frame's draws by their index in the frame (mesh, model, bounds bits) and their
    /// records; this frame's as they come.
    last: Vec<Option<([u32; 21], DrawGi)>>,
    next: Vec<Option<([u32; 21], DrawGi)>>,
    /// Samples taken (draws whose record was made) in the last frame.
    pub made: usize,
}

impl DrawGiCache {
    /// Frames an unused entry is kept.
    pub const KEEP_FRAMES: u64 = 2;
    /// Entries past this many bytes go, oldest first (each is a record and its stamp).
    pub const MAX_BYTES: usize = 4 << 20;
    const ENTRY_BYTES: usize = 64;

    /// Starts a frame: drops everything unless `key` (the GI set, the flat ambient, the tier) is
    /// the one cached.
    pub fn begin(&mut self, set: u64, flat: [f32; 3], quality: GiQuality) {
        let key = (set, flat.map(f32::to_bits), quality as u8);
        if key != self.key {
            self.entries.clear();
            self.last.clear();
            self.key = key;
        }
        self.frame += 1;
        self.made = 0;
        self.next.clear();
    }

    /// The record for the frame's draw `index`, `raw` its mesh, model and bounds bits: last
    /// frame's when the same draw had that index, else the cache's by `id` (a hash of `raw`),
    /// else `make`'s.
    pub fn get(&mut self, index: usize, raw: [u32; 21], id: impl FnOnce() -> u64, make: impl FnOnce() -> DrawGi) -> DrawGi {
        let rec = match self.last.get(index) {
            Some(Some((r, rec))) if *r == raw => *rec,
            _ => {
                let frame = self.frame;
                let mut made = false;
                let e = self.entries.entry(id()).or_insert_with(|| {
                    made = true;
                    (make(), frame)
                });
                e.1 = frame;
                self.made += usize::from(made);
                e.0
            }
        };
        if self.next.len() <= index {
            self.next.resize(index + 1, None);
        }
        self.next[index] = Some((raw, rec));
        rec
    }

    /// Ends the frame: this frame's draws become the ones the next compares with; stale entries
    /// are swept.
    pub fn end(&mut self) {
        std::mem::swap(&mut self.last, &mut self.next);
        let frame = self.frame;
        let drawn = self.last.iter().flatten().count();
        if self.entries.len() > drawn + 256 {
            self.entries.retain(|_, e| e.1 + Self::KEEP_FRAMES >= frame);
        }
        if self.bytes() > Self::MAX_BYTES {
            let mut stamps: Vec<u64> = self.entries.values().map(|e| e.1).collect();
            stamps.sort_unstable();
            let cut = stamps[stamps.len() - Self::MAX_BYTES / Self::ENTRY_BYTES];
            self.entries.retain(|_, e| e.1 > cut || e.1 == frame);
        }
    }

    /// Entries kept, and the bytes they take.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn bytes(&self) -> usize {
        self.entries.len() * Self::ENTRY_BYTES
    }
}

/// One volume (a streamed chunk of a baked lattice) as the shader reads it.
#[derive(Clone, Copy, Debug)]
pub struct GiVolume<'a> {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub dims: [u32; 3],
    /// Multiplies its irradiance.
    pub intensity: f32,
    /// `dims` product records, x fastest, then z, then y (`specula_lux::Lattice::index`).
    pub probes: &'a [[u32; 12]],
}

/// The frame's GI: none (the flat ambient) unless volumes are given.
#[derive(Clone, Copy, Debug, Default)]
pub struct GiFrame<'a> {
    /// Changes whenever `volumes` (or what they hold) change: the buffer is rebuilt only then.
    pub key: u64,
    pub volumes: &'a [GiVolume<'a>],
    pub quality: GiQuality,
    pub view: GiView,
}

impl GiFrame<'_> {
    pub fn active(&self) -> bool {
        !self.volumes.is_empty()
    }
}

/// What a frame's shaders get: the buffer's address and how to read it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GiBinding {
    pub address: u64,
    pub volumes: u32,
    pub visibility: bool,
    pub view: GiView,
    pub quality: GiQuality,
    /// The irradiance view's stretch ([`irradiance_view`]): its scale and the luminance it starts at.
    pub view_scale: f32,
    pub view_lo: f32,
}

impl GiBinding {
    pub fn active(&self) -> bool {
        self.volumes > 0 && self.address != 0
    }

    /// `FrameData::gi_params`: volume count, visibility (1, plus the irradiance view's starting
    /// luminance: shaders test it above 0.5), the irradiance view's scale (0: lit), per-draw
    /// records.
    pub fn params(&self) -> [f32; 4] {
        if !self.active() {
            return [0.0; 4];
        }
        let per_draw = if self.quality.per_draw_span().is_some() { 1.0 } else { 0.0 };
        let irr = self.view == GiView::Irradiance;
        let view = if irr { self.view_scale.max(1e-6) } else { 0.0 };
        let visibility = if self.visibility { 1.0 + if irr { self.view_lo.max(0.0) } else { 0.0 } } else { 0.0 };
        [self.volumes as f32, visibility, view, per_draw]
    }
}

/// Checks a frame's volumes: counts, sizes and finite numbers (structured errors).
pub fn check(gi: &GiFrame) -> Result<(), GpuError> {
    if gi.volumes.len() > MAX_GI_VOLUMES {
        return Err(GpuError::limit(Limit::GiVolumes, gi.volumes.len() as u64, MAX_GI_VOLUMES as u64));
    }
    let mut probes = 0usize;
    for (i, v) in gi.volumes.iter().enumerate() {
        let n = v.dims.iter().try_fold(1usize, |a, &d| a.checked_mul(d as usize)).unwrap_or(usize::MAX);
        let ok = v.dims.iter().all(|&d| d >= 2) && n == v.probes.len() && v.spacing.is_finite() && v.spacing > 0.0 && v.origin.iter().all(|x| x.is_finite()) && v.intensity.is_finite();
        if !ok {
            return Err(GpuError::new(format!(
                "GI volume {i}: dims {:?} (at least 2 each) must hold its {} probes, with a finite positive spacing ({}) and a finite origin and intensity",
                v.dims,
                v.probes.len(),
                v.spacing
            )));
        }
        probes += n;
    }
    if probes > MAX_GI_PROBES {
        return Err(GpuError::limit(Limit::GiProbes, probes as u64, MAX_GI_PROBES as u64));
    }
    Ok(())
}

/// The resident volumes' buffer.
#[derive(Default)]
pub struct GiBuffer {
    key: u64,
    buffer: Option<Buffer>,
    address: u64,
    volumes: u32,
    /// Rebuilds since start (a cache that rebuilt every frame would show here).
    pub rebuilds: u64,
    /// The set (key) that couldn't be bound, and why: not retried until the key changes.
    pub failure: Option<(u64, String)>,
    /// The irradiance view's stretch for this set ([`irradiance_view`]).
    pub view_scale: f32,
    pub view_lo: f32,
}

/// The irradiance view's range for `volumes`: the 5th and 95th percentiles of their valid probes'
/// irradiance luminance (times each volume's intensity; at most 65,536 probes looked at, evenly
/// spaced). The view stretches that range over [`IRRADIANCE_VIEW_RANGE`], so gradation shows
/// whatever the stage's brightness (a pale, evenly lit stage read near-white). `None`: nothing lit.
pub fn irradiance_range(volumes: &[GiVolume]) -> Option<(f32, f32)> {
    let total: usize = volumes.iter().map(|v| v.probes.len()).sum();
    let stride = total.div_ceil(65_536).max(1);
    let mut lum: Vec<f32> = Vec::new();
    for v in volumes {
        for r in v.probes.iter().step_by(stride) {
            if halves(r[7]).1 <= 0.0 {
                continue;
            }
            let ((cr, cg), (cb, _)) = (halves(r[0]), halves(r[1]));
            let y = (0.2126 * cr + 0.7152 * cg + 0.0722 * cb) * v.intensity.max(0.0);
            if y.is_finite() {
                lum.push(y.max(0.0));
            }
        }
    }
    if lum.is_empty() {
        return None;
    }
    lum.sort_by(f32::total_cmp);
    let at = |q: usize| lum[(lum.len() - 1) * q / 100];
    let (lo, hi) = (at(5), at(95));
    (hi > 1e-6).then_some((lo, hi))
}

/// The luminances the irradiance view shows the range's ends at (before the tone curve).
pub const IRRADIANCE_VIEW_RANGE: [f32; 2] = [0.03, 0.45];

/// The view's (scale, offset) for `range`: shown luminance = offset + (luminance - lo) x scale,
/// taken as `scale` and `lo` (see `FrameData::gi_params`). A range narrower than
/// [`IRRADIANCE_VIEW_MIN_SPREAD`] of its top (an open stage whose probes all see the same sky) is
/// widened downwards to it, so the view never magnifies probe noise into a white frame; a flat
/// range (every probe alike) shows at the top of the view's range.
pub fn irradiance_view(range: Option<(f32, f32)>) -> (f32, f32) {
    let [a, b] = IRRADIANCE_VIEW_RANGE;
    match range {
        Some((lo, hi)) => {
            let lo = lo.min(hi * (1.0 - IRRADIANCE_VIEW_MIN_SPREAD));
            ((b - a) / (hi - lo), lo)
        }
        None => (1.0, 0.0),
    }
}

/// The narrowest probe range the irradiance view stretches over, as a share of its top. (Found by
/// the Fidelity T3 sheet: an open stage with a skeleton, probes 0.339 to 0.341, was stretched
/// 264 times, and every pixel past the 95th percentile read white.)
pub const IRRADIANCE_VIEW_MIN_SPREAD: f32 = 0.08;

/// The buffer's contents for `gi` (checked first): the header, the volumes, the probes, their
/// lean copy.
pub fn words(gi: &GiFrame) -> Result<Vec<u32>, GpuError> {
    check(gi)?;
    let probes: usize = gi.volumes.iter().map(|v| v.probes.len()).sum();
    let mut words: Vec<u32> = Vec::with_capacity(4 + gi.volumes.len() * 12 + probes * 16);
    // The lean copy's first record (16-byte units).
    let lean_at = (1 + gi.volumes.len() * 3 + probes * 3) as u32;
    words.extend_from_slice(&[gi.volumes.len() as u32, lean_at, 0, 0]);
    let mut first = 0u32;
    for v in gi.volumes {
        words.extend(v.origin.iter().chain(std::iter::once(&v.spacing)).map(|f| f.to_bits()));
        words.extend_from_slice(&[v.dims[0], v.dims[1], v.dims[2], first]);
        words.extend_from_slice(&[v.intensity.max(0.0).to_bits(), 0, 0, 0]);
        first += v.probes.len() as u32;
    }
    for v in gi.volumes {
        for p in v.probes {
            words.extend_from_slice(p);
        }
    }
    for v in gi.volumes {
        for p in v.probes {
            words.extend_from_slice(&lean(p));
        }
    }
    Ok(words)
}

/// A device-local buffer (fast GPU reads: host-visible memory is uncached for the iGPU's shaders,
/// which made every probe read a trip to memory) holding `words`, copied from a staging buffer
/// with `run`, which records into a command buffer and waits for it.
pub fn upload(dev: &Device, words: &[u32], run: impl FnOnce(&Device, &dyn Fn(&Device, VkCommandBuffer)) -> Result<(), GpuError>) -> Result<Buffer, GpuError> {
    let bytes = (words.len() * 4) as u64;
    let staging = dev.create_buffer(bytes, VK_BUFFER_USAGE_TRANSFER_SRC_BIT, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT, 0, MemoryKind::Staging, "gi staging")?;
    // SAFETY: `mapped` is coherent host memory of `bytes` bytes nothing else uses; `words` holds
    // exactly that many bytes.
    unsafe { ptr::copy_nonoverlapping(words.as_ptr() as *const u8, staging.mapped as *mut u8, bytes as usize) };
    let buffer = match dev.create_buffer(bytes, VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 0, MemoryKind::Gi, "gi probes") {
        Ok(b) => b,
        Err(e) => {
            dev.destroy_buffer(&staging);
            return Err(e);
        }
    };
    let record = |dev: &Device, cmd: VkCommandBuffer| {
        let region = VkBufferCopy { src_offset: 0, dst_offset: 0, size: bytes };
        // SAFETY: recording; both buffers are live and hold `bytes` bytes.
        unsafe { (dev.fns.cmd_copy_buffer)(cmd, staging.handle, buffer.handle, 1, &region) };
        dev.barrier(
            cmd,
            &[crate::device::buffer_barrier(buffer.handle, VK_PIPELINE_STAGE_2_COPY_BIT, VK_ACCESS_2_TRANSFER_WRITE_BIT, VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT, VK_ACCESS_2_SHADER_STORAGE_READ_BIT)],
            &[],
        );
    };
    let ran = run(dev, &record);
    dev.destroy_buffer(&staging);
    if let Err(e) = ran {
        dev.destroy_buffer(&buffer);
        return Err(e);
    }
    Ok(buffer)
}

impl GiBuffer {
    /// Whether the buffer must be rebuilt for `gi` (an active frame with another key).
    pub fn stale(&self, gi: &GiFrame) -> bool {
        gi.active() && (self.buffer.is_none() || self.key != gi.key) && !self.failed(gi)
    }

    /// Whether `gi`'s set already failed to bind.
    pub fn failed(&self, gi: &GiFrame) -> bool {
        self.failure.as_ref().is_some_and(|(k, _)| *k == gi.key)
    }

    /// Records that set `key` couldn't be bound.
    pub fn fail(&mut self, key: u64, why: String) {
        self.failure = Some((key, why));
    }

    /// Puts `buffer` (made by [`upload`] for `gi`) in place; the old one is retired.
    pub fn install(&mut self, uploads: &mut Uploads, dev: &Device, buffer: Buffer, gi: &GiFrame) {
        let address = dev.buffer_address(&buffer);
        self.release(uploads);
        self.buffer = Some(buffer);
        self.address = address;
        self.volumes = gi.volumes.len() as u32;
        self.key = gi.key;
        (self.view_scale, self.view_lo) = irradiance_view(irradiance_range(gi.volumes));
        self.rebuilds += 1;
        self.failure = None;
    }

    /// The binding for `gi` (after any rebuild). `ready`: the GI pipelines exist (otherwise
    /// nothing is bound and the frame draws the flat ambient). A window frame naming no set
    /// (key 0) frees the buffer; an offscreen one (a studio render, a preview) leaves it for the
    /// window.
    pub fn bind(&mut self, uploads: &mut Uploads, gi: &GiFrame, ready: bool, offscreen: bool) -> GiBinding {
        if !gi.active() {
            if gi.key == 0 && !offscreen {
                self.release(uploads);
                self.failure = None;
            }
            return GiBinding::default();
        }
        if !ready || self.buffer.is_none() || self.key != gi.key {
            return GiBinding::default();
        }
        GiBinding { address: self.address, volumes: self.volumes, visibility: true, view: gi.view, quality: gi.quality, view_scale: self.view_scale, view_lo: self.view_lo }
    }

    /// Bytes the buffer holds.
    pub fn bytes(&self) -> u64 {
        self.buffer.as_ref().map_or(0, |b| b.bytes)
    }

    /// Retires the buffer (freed once no submitted frame reads it).
    fn release(&mut self, uploads: &mut Uploads) {
        if let Some(b) = self.buffer.take() {
            uploads.retire(Retired::Buffer(b));
        }
        self.address = 0;
        self.volumes = 0;
        self.key = 0;
    }

    /// Frees the buffer now (the device is idle: shutdown).
    pub fn destroy(&mut self, dev: &Device) {
        if let Some(b) = self.buffer.take() {
            dev.destroy_buffer(&b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_are_checked() {
        let probes = vec![[0u32; 12]; 8];
        let ok = GiVolume { origin: [0.0; 3], spacing: 1.0, dims: [2, 2, 2], intensity: 1.0, probes: &probes };
        let v = [ok];
        assert!(check(&GiFrame { key: 1, volumes: &v, ..GiFrame::default() }).is_ok());
        let short = [GiVolume { dims: [2, 2, 3], ..ok }];
        assert!(check(&GiFrame { key: 1, volumes: &short, ..GiFrame::default() }).is_err());
        let flat = [GiVolume { dims: [8, 1, 1], ..ok }];
        assert!(check(&GiFrame { key: 1, volumes: &flat, ..GiFrame::default() }).is_err());
        let nan = [GiVolume { spacing: f32::NAN, ..ok }];
        assert!(check(&GiFrame { key: 1, volumes: &nan, ..GiFrame::default() }).is_err());
        let many = [ok; MAX_GI_VOLUMES + 1];
        let e = check(&GiFrame { key: 1, volumes: &many, ..GiFrame::default() }).unwrap_err();
        assert_eq!(e.limit.map(|l| l.what), Some(Limit::GiVolumes));
    }

    #[test]
    fn the_irradiance_view_is_stretched_over_the_probes_range() {
        // A pale stage: probes 3.6 to 4.0 bright read near-white; the view stretches their 5th to
        // 95th percentiles over IRRADIANCE_VIEW_RANGE so the difference shows.
        let mut probes: Vec<[u32; 12]> = (0..100).map(|i| rec(3.6 + 0.4 * i as f32 / 99.0, 1.0, u64::MAX)).collect();
        probes[0] = rec(40.0, 0.0, u64::MAX); // invalid: left out
        let v = [GiVolume { origin: [0.0; 3], spacing: 1.0, dims: [10, 2, 5], intensity: 1.0, probes: &probes }];
        let (lo, hi) = irradiance_range(&v).expect("lit");
        assert!((3.6..3.65).contains(&lo) && (3.95..=4.0).contains(&hi), "{lo} {hi}");
        let (scale, start) = irradiance_view(Some((lo, hi)));
        let [a, b] = IRRADIANCE_VIEW_RANGE;
        assert!(((hi - start) * scale + a - b).abs() < 1e-4 && start == lo);
        assert_eq!(irradiance_range(&[]), None);
        assert_eq!(irradiance_view(None), (1.0, 0.0), "nothing lit: unchanged");
        let flat = vec![rec(0.5, 1.0, u64::MAX); 8];
        let r = irradiance_range(&[GiVolume { origin: [0.0; 3], spacing: 1.0, dims: [2, 2, 2], intensity: 1.0, probes: &flat }]);
        let (s, l) = irradiance_view(r);
        assert!(((0.5 - l) * s - (b - a)).abs() < 1e-4, "every probe alike: the top of the range");
        // Nearly alike (an open stage, the Fidelity T3 skeleton's: 0.3389 to 0.3405): widened to
        // 8% of the top, not stretched 264 times; the top still shows at the range's top.
        let (s, l) = irradiance_view(Some((0.3389, 0.3405)));
        assert!(s <= (b - a) / (0.3405 * IRRADIANCE_VIEW_MIN_SPREAD) + 1e-3 && s < 20.0, "scale {s}");
        assert!(((0.3405 - l) * s - (b - a)).abs() < 1e-4 && (0.3389 - l) * s + a > a, "lo {l}");
    }

    #[test]
    fn bindings_report_what_the_shader_reads() {
        assert_eq!(GiBinding::default().params(), [0.0; 4]);
        let b = GiBinding { address: 64, volumes: 3, visibility: true, view: GiView::Irradiance, quality: GiQuality::Low, view_scale: 2.5, view_lo: 0.25 };
        assert_eq!(b.params(), [3.0, 1.25, 2.5, 1.0], "the irradiance view carries its stretch");
        assert_eq!(GiBinding { view: GiView::Lit, ..b }.params()[2], 0.0);
        assert_eq!(GiBinding { quality: GiQuality::High, ..b }.params()[3], 0.0);
    }

    fn rec(c0: f32, weight: f32, mask: u64) -> [u32; 12] {
        let h = |a: f32, b: f32| u32::from(crate::effects::f16_bits(a)) | (u32::from(crate::effects::f16_bits(b)) << 16);
        [h(c0, c0), h(c0, 0.0), 0, 0, 0, 0, 0, h(0.0, weight), mask as u32, (mask >> 32) as u32, u32::MAX, 0x00ff_ffff]
    }

    #[test]
    fn draws_sample_the_probes_masks_and_the_flat_ambient() {
        // Two by two by two probes 1 m apart from the origin: the x = 1 side bright.
        let mut probes = vec![rec(0.1, 1.0, u64::MAX); 8];
        for i in [1, 3, 5, 7] {
            probes[i] = rec(0.9, 1.0, u64::MAX);
        }
        let v = [GiVolume { origin: [0.0; 3], spacing: 1.0, dims: [2, 2, 2], intensity: 1.0, probes: &probes }];
        // A point's irradiance (c0 red) through a one-point cube: its +x face's value at `p`
        // minus half a spacing (the cube samples half a spacing out from the face).
        let c0 = |d: DrawGi| halves(d[0]).0;
        let sample_draw = |v: &[GiVolume], p: [f32; 3], flat: [f32; 3]| {
            let d = sample_cube(v, [p[0] - 1.0, p[1], p[2]], [p[0] - 0.5, p[1], p[2]], 1.0, flat);
            assert_eq!(d[11], DRAW_CUBE);
            d
        };
        let mid = sample_draw(&v, [0.5, 0.5, 0.5], [0.0; 3]);
        assert!((c0(mid) - 0.5).abs() < 1e-3);
        // A wall at x = 0.5: from the low octants the bright side is hidden.
        let mut walled = probes.clone();
        let mut mask = u64::MAX;
        for octant in 0..8u32 {
            for corner in 0..8u32 {
                if octant & 1 != corner & 1 {
                    mask &= !(1u64 << (octant * 8 + corner));
                }
            }
        }
        walled[0] = rec(0.1, 1.0, mask);
        let v2 = [GiVolume { probes: &walled, ..v[0] }];
        assert!((c0(sample_draw(&v2, [0.3, 0.5, 0.5], [0.0; 3])) - 0.1).abs() < 1e-3);
        // Hidden from every probe (a solid prop's centre): the valid ones, unmasked.
        walled[0] = rec(0.1, 1.0, 0);
        let v3 = [GiVolume { probes: &walled, ..v[0] }];
        assert!((c0(sample_draw(&v3, [0.5, 0.5, 0.5], [0.0; 3])) - 0.5).abs() < 1e-3);
        // ...but the reaches still apply first: bright probes that see nothing (behind the walls of
        // a sealed room the point is in) stay out, so the room's own light comes through.
        let mut sealed = walled.clone();
        for i in [1, 3, 5, 7] {
            sealed[i][10] = 0;
            sealed[i][11] = 0;
        }
        let v5 = [GiVolume { probes: &sealed, ..v[0] }];
        assert!((c0(sample_draw(&v5, [0.5, 0.5, 0.5], [0.0; 3])) - 0.1).abs() < 1e-3);
        // Half a spacing outside: half the flat ambient.
        let out = sample_draw(&v, [-0.5, 0.5, 0.5], [1.0; 3]);
        assert!((c0(out) - 0.55).abs() < 2e-3, "{}", c0(out));
        assert_eq!(c0(sample_draw(&v, [5.0, 0.5, 0.5], [1.0; 3])), 1.0, "outside every volume: the flat ambient");
        // No valid probe (deep in rock): the flat ambient, not black.
        let dead = vec![rec(0.9, 0.0, u64::MAX); 8];
        let v4 = [GiVolume { probes: &dead, ..v[0] }];
        assert_eq!(c0(sample_draw(&v4, [0.5, 0.5, 0.5], [0.25; 3])), 0.25);
        // An ambient cube over a box in the cell's middle: each face reads its own side (half a
        // spacing out: the +x face at x = 1, bright; -x at x = 0, dim; +y the mean).
        let cube = sample_cube(&v, [0.25; 3], [0.75; 3], 0.5, [0.0; 3]);
        let red = |k: usize| {
            let (a, b) = halves(cube[3 * k / 2]);
            if (3 * k) % 2 == 0 { a } else { b }
        };
        assert!((red(0) - 0.9).abs() < 2e-3 && (red(1) - 0.1).abs() < 2e-3 && (red(2) - 0.5).abs() < 2e-3, "{} {} {}", red(0), red(1), red(2));
        assert_eq!(lean_draw()[11], DRAW_LEAN);
        // The lean copy: the mean, the direction's luminance and the validity weight.
        let l = lean(&rec(0.9, 0.5, u64::MAX));
        assert!((halves(l[0]).0 - 0.9).abs() < 1e-3 && f32::from_bits(l[3]) == 0.5);
        assert_eq!(f32::from_bits(lean(&rec(0.9, 0.0, 0))[3]), 0.0);
        assert_eq!(half(crate::effects::f16_bits(-2.5)), -2.5);
        assert_eq!(half(crate::effects::f16_bits(6.0e-6)), half(crate::effects::f16_bits(6.0e-6)));
    }
}
