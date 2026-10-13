//! The per-entity player: a base clip with crossfades, up to [`MAX_LAYERS`] override or
//! additive layers, events, root motion and speed matching.
//!
//! ```text
//! let mut a = Animator::new(&set, set.id("walk").unwrap());
//! a.match_speed(&set, 1.1);                 // m/s from the simulation → playback rate
//! a.advance(&set, 1.0 / 60.0);              // or advance_ticks(&set, 1, 60)
//! for ev in a.events() { /* set.clip(ev.clip).events[ev.event as usize].name == "footstep" */ }
//! let pose = a.evaluate(&set);              // local pose → skeleton.model_space → skin
//! ```
//!
//! Time is kept as unwrapped `f64` seconds per playback, so long loops don't drift and events
//! fire exactly once per occurrence however the time is split into steps.

use specula_core::hash::StableHasher;

use crate::blend::{self, Base};
use crate::clip::{Channel, LoopMode};
use crate::math::{smoothstep, Affine, V3};
use crate::set::{AnimSet, ClipId, MaskId};
use crate::skeleton::Pose;

/// Layers on top of the base clip (slot 0 is applied first).
pub const MAX_LAYERS: usize = 4;
/// Playback rates are clamped to 0..=MAX_RATE.
pub const MAX_RATE: f32 = 8.0;

/// One clip being played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playback {
    pub clip: ClipId,
    /// Unwrapped seconds since this clip started (clip-local time comes from the loop mode).
    pub time: f64,
    pub rate: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerMode {
    /// Replaces the pose underneath (by weight and mask).
    Override,
    /// Adds the clip's values as deltas (a hit flinch on top of a walk).
    Additive,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub play: Playback,
    pub mode: LayerMode,
    pub weight: f32,
    pub mask: Option<MaskId>,
    /// Seconds to fade in, and (for `once` clips) to fade out before the end; the layer
    /// removes itself when a `once` clip finishes.
    pub fade: f32,
}

/// Which components of the root bone's translation track move the entity instead of the pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootMotion {
    Off,
    /// X and Z go to [`Animator::root_motion`]; Y stays in the pose (a leap's height).
    Horizontal,
    Full,
}

/// An event crossed during the last `advance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FiredEvent {
    pub clip: ClipId,
    /// Index into the clip's `events`.
    pub event: u16,
    /// Layer slot, or `None` for the base clip.
    pub layer: Option<u8>,
    /// Loop cycle the occurrence belongs to.
    pub cycle: u32,
}

#[derive(Clone, Debug)]
pub struct Animator {
    base: Playback,
    /// The source of a running crossfade: (source, elapsed s, duration s).
    fade_from: Option<(FadeSource, f32, f32)>,
    layers: [Option<Layer>; MAX_LAYERS],
    layer_age: [f64; MAX_LAYERS],
    root_mode: RootMotion,
    root_delta: V3,
    events: Vec<FiredEvent>,
    events_truncated: bool,
    pose: Pose,
    scratch: Pose,
    scratch2: Pose,
    /// The blended pose a crossfade interrupted mid-way fades out from.
    frozen: Pose,
    scan: Vec<(u16, u32)>,
}

/// What a crossfade fades out from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FadeSource {
    /// The previous base clip, still playing.
    Clip(Playback),
    /// A snapshot of the pose when a crossfade interrupted another one (`Animator::frozen`).
    Frozen,
}

impl Animator {
    pub fn new(set: &AnimSet, clip: ClipId) -> Animator {
        let rest = set.skeleton.rest_pose();
        Animator {
            base: Playback { clip, time: 0.0, rate: 1.0 },
            fade_from: None,
            layers: [None; MAX_LAYERS],
            layer_age: [0.0; MAX_LAYERS],
            root_mode: RootMotion::Off,
            root_delta: [0.0; 3],
            events: Vec::with_capacity(16),
            events_truncated: false,
            pose: rest.clone(),
            scratch: rest.clone(),
            scratch2: rest.clone(),
            frozen: rest,
            scan: Vec::with_capacity(16),
        }
    }

    /// Switches to `clip` immediately, from its start (rate kept).
    pub fn set_clip(&mut self, clip: ClipId) {
        self.base = Playback { clip, time: 0.0, rate: self.base.rate };
        self.fade_from = None;
    }

    /// Blends from the current pose to `clip` (from its start) over `seconds` (smoothstep). The
    /// outgoing clip keeps playing during the fade; if another crossfade is still running, its
    /// current blended pose is frozen and faded out from instead, so nothing pops.
    pub fn crossfade(&mut self, set: &AnimSet, clip: ClipId, seconds: f32) {
        if seconds <= 0.0 || !seconds.is_finite() {
            return self.set_clip(clip);
        }
        let source = if self.fade_from.is_some() {
            self.sample_base(set);
            std::mem::swap(&mut self.frozen, &mut self.pose);
            FadeSource::Frozen
        } else {
            FadeSource::Clip(self.base)
        };
        self.fade_from = Some((source, 0.0, seconds));
        self.base = Playback { clip, time: 0.0, rate: self.base.rate };
    }

    pub fn set_rate(&mut self, rate: f32) {
        self.base.rate = if rate.is_finite() { rate.clamp(0.0, MAX_RATE) } else { 1.0 };
    }

    /// Sets the base rate so a locomotion clip's feet stay planted at `speed` m/s (uses the
    /// clip's `distance`). Returns the rate; clips without a distance keep their rate.
    pub fn match_speed(&mut self, set: &AnimSet, speed: f32) -> f32 {
        let natural = set.clip(self.base.clip).speed();
        if natural > 0.0 {
            self.set_rate(speed / natural);
        }
        self.base.rate
    }

    /// Plays `clip` on layer `slot` (replacing what was there), from its start.
    pub fn set_layer(&mut self, slot: usize, clip: ClipId, mode: LayerMode, weight: f32, mask: Option<MaskId>, fade: f32) {
        if slot < MAX_LAYERS {
            self.layers[slot] = Some(Layer { play: Playback { clip, time: 0.0, rate: 1.0 }, mode, weight: weight.clamp(0.0, 1.0), mask, fade: fade.max(0.0) });
            self.layer_age[slot] = 0.0;
        }
    }

    pub fn set_layer_weight(&mut self, slot: usize, weight: f32) {
        if let Some(Some(l)) = self.layers.get_mut(slot) {
            l.weight = weight.clamp(0.0, 1.0);
        }
    }

    pub fn clear_layer(&mut self, slot: usize) {
        if slot < MAX_LAYERS {
            self.layers[slot] = None;
        }
    }

    pub fn layer(&self, slot: usize) -> Option<&Layer> {
        self.layers.get(slot).and_then(|l| l.as_ref())
    }

    pub fn set_root_motion(&mut self, mode: RootMotion) {
        self.root_mode = mode;
    }

    pub fn base(&self) -> &Playback {
        &self.base
    }

    /// True while a crossfade is running.
    pub fn fading(&self) -> bool {
        self.fade_from.is_some()
    }

    /// True if the base clip plays once and has reached its end.
    pub fn finished(&self, set: &AnimSet) -> bool {
        let c = set.clip(self.base.clip);
        c.looping == LoopMode::Once && self.base.time >= c.duration as f64
    }

    /// Advances `ticks` simulation ticks at `hz` (exact tick arithmetic in `f64`).
    pub fn advance_ticks(&mut self, set: &AnimSet, ticks: u32, hz: u32) {
        self.advance_f64(set, ticks as f64 / hz.max(1) as f64);
    }

    /// Advances by `dt` seconds: moves every playhead, collects events and root motion.
    pub fn advance(&mut self, set: &AnimSet, dt: f32) {
        self.advance_f64(set, dt.max(0.0) as f64);
    }

    fn advance_f64(&mut self, set: &AnimSet, dt: f64) {
        self.events.clear();
        self.events_truncated = false;
        self.root_delta = [0.0; 3];
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        // Base clip.
        let u0 = self.base.time;
        let u1 = u0 + dt * self.base.rate as f64;
        self.scan.clear();
        self.events_truncated |= set.clip(self.base.clip).events_between(u0, u1, &mut self.scan);
        self.events.extend(self.scan.iter().map(|&(event, cycle)| FiredEvent { clip: self.base.clip, event, layer: None, cycle }));
        let mut w_base = 1.0f32;
        // Crossfade: the outgoing clip keeps moving (its events are dropped).
        if let Some((source, elapsed, dur)) = &mut self.fade_from {
            *elapsed += dt as f32;
            w_base = smoothstep(*elapsed / *dur);
            if let FadeSource::Clip(from) = source {
                let f0 = from.time;
                from.time += dt * from.rate as f64;
                if self.root_mode != RootMotion::Off {
                    let d = root_offset(set, from.clip, from.time, self.root_mode, f0);
                    for (r, d) in self.root_delta.iter_mut().zip(d) {
                        *r += d * (1.0 - w_base);
                    }
                }
            }
            if *elapsed >= *dur {
                self.fade_from = None;
            }
        }
        if self.root_mode != RootMotion::Off {
            let d = root_offset(set, self.base.clip, u1, self.root_mode, u0);
            for (r, d) in self.root_delta.iter_mut().zip(d) {
                *r += d * w_base;
            }
        }
        self.base.time = u1;
        // Layers.
        for slot in 0..MAX_LAYERS {
            let Some(l) = &mut self.layers[slot] else { continue };
            let l0 = l.play.time;
            l.play.time += dt * l.play.rate as f64;
            self.layer_age[slot] += dt;
            self.scan.clear();
            let clip = set.clip(l.play.clip);
            // Events first (an event at the very end fires in this step), then retire the layer.
            self.events_truncated |= clip.events_between(l0, l.play.time, &mut self.scan);
            let id = l.play.clip;
            self.events.extend(self.scan.iter().map(|&(event, cycle)| FiredEvent { clip: id, event, layer: Some(slot as u8), cycle }));
            if clip.looping == LoopMode::Once && l.play.time >= clip.duration as f64 {
                self.layers[slot] = None;
            }
        }
    }

    /// Events crossed during the last advance, in time order per playback (base first).
    pub fn events(&self) -> &[FiredEvent] {
        &self.events
    }

    /// True if the last advance crossed more events than are reported per step
    /// ([`crate::clip::MAX_EVENTS_PER_STEP`] per playback) and some were dropped.
    pub fn events_truncated(&self) -> bool {
        self.events_truncated
    }

    /// Root translation (model space, metres) the clips moved during the last advance, when
    /// root motion is on. The pose keeps the root at rest in the extracted components.
    pub fn root_motion(&self) -> V3 {
        self.root_delta
    }

    /// The base clip crossfaded with its source, into `self.pose` (no layers).
    fn sample_base(&mut self, set: &AnimSet) {
        let skel = &set.skeleton;
        let b = set.bound(self.base.clip);
        blend::sample(skel, &b.clip, &b.bones, b.clip.local_time(self.base.time), Base::Rest, &mut self.pose);
        if let Some((source, elapsed, dur)) = &self.fade_from {
            match source {
                FadeSource::Clip(from) => {
                    let f = set.bound(from.clip);
                    blend::sample(skel, &f.clip, &f.bones, f.clip.local_time(from.time), Base::Rest, &mut self.scratch);
                }
                // Reuses the buffer's capacity: no allocation.
                FadeSource::Frozen => self.scratch.local.clone_from(&self.frozen.local),
            }
            blend::blend(&mut self.scratch, &self.pose, smoothstep(*elapsed / *dur), None);
            std::mem::swap(&mut self.pose, &mut self.scratch);
        }
    }

    /// Samples and blends everything into the current local pose.
    pub fn evaluate(&mut self, set: &AnimSet) -> &Pose {
        let skel = &set.skeleton;
        self.sample_base(set);
        match self.root_mode {
            RootMotion::Off => {}
            RootMotion::Horizontal => {
                let r = skel.rest[0].t;
                self.pose.local[0].t[0] = r[0];
                self.pose.local[0].t[2] = r[2];
            }
            RootMotion::Full => self.pose.local[0].t = skel.rest[0].t,
        }
        for slot in 0..MAX_LAYERS {
            let Some(l) = &self.layers[slot] else { continue };
            let lb = set.bound(l.play.clip);
            let local = lb.clip.local_time(l.play.time);
            let mut ramp = 1.0f32;
            if l.fade > 0.0 {
                ramp = (self.layer_age[slot] as f32 / l.fade).min(1.0);
                if lb.clip.looping == LoopMode::Once {
                    ramp = ramp.min(((lb.clip.duration - local) / l.fade).clamp(0.0, 1.0));
                }
            }
            let w = l.weight * smoothstep(ramp);
            let mask = l.mask.map(|m| &set.mask(m).weights[..]);
            match l.mode {
                LayerMode::Override => {
                    blend::sample(skel, &lb.clip, &lb.bones, local, Base::Rest, &mut self.scratch2);
                    blend::blend(&mut self.pose, &self.scratch2, w, mask);
                }
                LayerMode::Additive => {
                    blend::sample(skel, &lb.clip, &lb.bones, local, Base::Delta, &mut self.scratch2);
                    blend::add(&mut self.pose, &self.scratch2, w, mask);
                }
            }
        }
        &self.pose
    }

    /// The pose from the last [`Animator::evaluate`].
    pub fn pose(&self) -> &Pose {
        &self.pose
    }

    /// Model-space bone transforms and skinning matrices of the last evaluated pose.
    pub fn skin(&self, set: &AnimSet, model: &mut Vec<Affine>, skin: &mut Vec<Affine>) {
        set.skeleton.model_space(&self.pose, model);
        set.skeleton.skin_matrices(model, skin);
    }

    /// The value of curve `name` (base clip, crossfaded with the outgoing clip), if any of them
    /// has it.
    pub fn curve(&self, set: &AnimSet, name: &str) -> Option<f32> {
        let value = |p: &Playback| {
            let c = set.clip(p.clip);
            c.curves.iter().find(|k| k.name == name).map(|k| k.keys.sample(c.local_time(p.time), false, c.wrap())[0])
        };
        let now = value(&self.base);
        match &self.fade_from {
            Some((source, elapsed, dur)) => {
                let w = smoothstep(*elapsed / *dur);
                let from = match source {
                    FadeSource::Clip(p) => value(p),
                    FadeSource::Frozen => None,
                };
                match (from, now) {
                    (Some(a), Some(b)) => Some(a + (b - a) * w),
                    (a, b) => b.or(a),
                }
            }
            None => now,
        }
    }

    /// A hash of the playback state (clips, times, rates, layers) for determinism checks and
    /// journaling (F2).
    pub fn state_hash(&self) -> u64 {
        let mut h = StableHasher::new();
        let pb = |h: &mut StableHasher, p: &Playback| {
            h.write_u32(p.clip.0 as u32);
            h.write_f64(p.time);
            h.write_f32(p.rate);
        };
        pb(&mut h, &self.base);
        match &self.fade_from {
            Some((source, e, d)) => {
                match source {
                    FadeSource::Clip(p) => {
                        h.write_u8(1);
                        pb(&mut h, p);
                    }
                    FadeSource::Frozen => {
                        h.write_u8(2);
                        for t in &self.frozen.local {
                            t.t.iter().chain(t.r.iter()).chain(t.s.iter()).for_each(|v| h.write_f32(*v));
                        }
                    }
                }
                h.write_f32(*e);
                h.write_f32(*d);
            }
            None => h.write_u8(0),
        }
        for (l, age) in self.layers.iter().zip(&self.layer_age) {
            match l {
                Some(l) => {
                    h.write_u8(1);
                    pb(&mut h, &l.play);
                    h.write_u8(l.mode as u8);
                    h.write_f32(l.weight);
                    h.write_u32(l.mask.map_or(u32::MAX, |m| m.0 as u32));
                    h.write_f32(l.fade);
                    h.write_f64(*age);
                }
                None => h.write_u8(0),
            }
        }
        h.write_u8(self.root_mode as u8);
        h.finish()
    }
}

/// Root translation moved by `clip` between unwrapped times `u0` and `u1` (looping clips
/// accumulate one cycle's displacement per wrap; ping-pong clips don't accumulate).
fn root_offset(set: &AnimSet, clip: ClipId, u1: f64, mode: RootMotion, u0: f64) -> V3 {
    let b = set.bound(clip);
    let Some(ti) = b.root_t else { return [0.0; 3] };
    let c = &b.clip;
    let keys = &c.tracks[ti].keys;
    debug_assert_eq!(c.tracks[ti].channel, Channel::Translation);
    let at = |u: f64| -> [f64; 3] {
        let v = keys.sample(c.local_time(u), false, c.wrap());
        let mut p = [v[0] as f64, v[1] as f64, v[2] as f64];
        if c.looping == LoopMode::Loop {
            let k = specula_core::dmath::floor_f64(u.max(0.0) / c.duration as f64);
            let end = keys.sample(c.duration, false, None);
            let start = keys.sample(0.0, false, None);
            for i in 0..3 {
                p[i] += k * (end[i] - start[i]) as f64;
            }
        }
        p
    };
    let (a, b) = (at(u0), at(u1));
    let mut d = [(b[0] - a[0]) as f32, (b[1] - a[1]) as f32, (b[2] - a[2]) as f32];
    if mode == RootMotion::Horizontal {
        d[1] = 0.0;
    }
    d
}
