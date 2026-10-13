//! Pose sampling and blending: crossfades, override layers, additive layers, bone masks.
//!
//! Rotations blend with normalised linear interpolation along the shortest path (nlerp:
//! cheap, and order-independent when blending several poses; docs/design/ANIMATION.md).

use crate::clip::{Channel, Clip};
use crate::math::{qmul, qnlerp, Transform, QID};
use crate::skeleton::{Pose, Skeleton};

/// How sampled values combine with the skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// Absolute local pose: rest + clip values (missing tracks = rest).
    Rest,
    /// Raw clip values as a delta (missing tracks = no change), for additive layers.
    Delta,
}

/// Samples `clip` at clip-local time `t` into `out`. `bones[i]` is the skeleton bone of
/// `clip.tracks[i]` (see `AnimSet`). Pure: the same inputs give the same bits everywhere.
pub fn sample(skel: &Skeleton, clip: &Clip, bones: &[u16], t: f32, base: Base, out: &mut Pose) {
    out.local.clear();
    match base {
        Base::Rest => out.local.extend_from_slice(&skel.rest),
        Base::Delta => out.local.resize(skel.len(), Transform::IDENTITY),
    }
    let wrap = clip.wrap();
    for (track, &b) in clip.tracks.iter().zip(bones) {
        let b = b as usize;
        let rot = track.channel == Channel::Rotation;
        let v = track.keys.sample(t, rot, wrap);
        let rest = match base {
            Base::Rest => skel.rest[b],
            Base::Delta => Transform::IDENTITY,
        };
        let o = &mut out.local[b];
        match track.channel {
            Channel::Translation => o.t = [rest.t[0] + v[0], rest.t[1] + v[1], rest.t[2] + v[2]],
            Channel::Rotation => o.r = if rest.r == QID { v } else { qmul(rest.r, v) },
            Channel::Scale => o.s = [rest.s[0] * v[0], rest.s[1] * v[1], rest.s[2] * v[2]],
        }
    }
}

/// `into = lerp(into, other, w · mask[j])` per bone. `w = 0` leaves `into` untouched; `w = 1`
/// (mask 1) copies `other` exactly.
pub fn blend(into: &mut Pose, other: &Pose, w: f32, mask: Option<&[f32]>) {
    for (j, (a, b)) in into.local.iter_mut().zip(&other.local).enumerate() {
        let w = w * mask.map_or(1.0, |m| m[j]);
        if w <= 0.0 {
            continue;
        }
        if w >= 1.0 {
            *a = *b;
            continue;
        }
        a.t = [a.t[0] + (b.t[0] - a.t[0]) * w, a.t[1] + (b.t[1] - a.t[1]) * w, a.t[2] + (b.t[2] - a.t[2]) * w];
        a.r = qnlerp(a.r, b.r, w);
        a.s = [a.s[0] + (b.s[0] - a.s[0]) * w, a.s[1] + (b.s[1] - a.s[1]) * w, a.s[2] + (b.s[2] - a.s[2]) * w];
    }
}

/// Adds a delta pose (sampled with [`Base::Delta`]) on top of `into`, scaled by `w · mask[j]`:
/// translation offsets add, rotations post-multiply (in the bone's own frame), scales multiply.
pub fn add(into: &mut Pose, delta: &Pose, w: f32, mask: Option<&[f32]>) {
    for (j, (a, d)) in into.local.iter_mut().zip(&delta.local).enumerate() {
        let w = w * mask.map_or(1.0, |m| m[j]);
        if w <= 0.0 {
            continue;
        }
        a.t = [a.t[0] + d.t[0] * w, a.t[1] + d.t[1] * w, a.t[2] + d.t[2] * w];
        if d.r != QID {
            a.r = qmul(a.r, if w >= 1.0 { d.r } else { qnlerp(QID, d.r, w) });
        }
        a.s = [a.s[0] * (1.0 + (d.s[0] - 1.0) * w), a.s[1] * (1.0 + (d.s[1] - 1.0) * w), a.s[2] * (1.0 + (d.s[2] - 1.0) * w)];
    }
}
