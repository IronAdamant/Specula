//! Measured checks on clips, returned as numbers agents can act on (docs/design/ANIMATION.md):
//! foot sliding, loop seams, and a stable hash of sampled poses.

use specula_core::hash::StableHasher;

use crate::blend::{self, Base};
use crate::clip::LoopMode;
use crate::math::{math3, Affine, V3};
use crate::set::{AnimSet, ClipId};

/// Model-space joint positions (bone heads) of a clip at clip-local time `t`.
pub fn joints_at(set: &AnimSet, id: ClipId, t: f32) -> Vec<V3> {
    let mut pose = set.skeleton.rest_pose();
    blend::sample(&set.skeleton, set.clip(id), set.track_bones(id), t, Base::Rest, &mut pose);
    let mut model: Vec<Affine> = Vec::new();
    set.skeleton.model_space(&pose, &mut model);
    model.iter().map(|m| m.t).collect()
}

/// Foot sliding of a locomotion clip, as the clip would look moving over the ground at its
/// natural speed (`distance / duration` along +Z): for each named foot, the fastest ground-
/// relative horizontal speed (m/s) during sustained contact (the foot within `planted_m` of its
/// lowest height for two samples either side of the step, so lift-off and touch-down moments
/// don't count). 0 means perfectly planted feet. `steps` samples per cycle.
pub fn foot_slide(set: &AnimSet, id: ClipId, feet: &[usize], planted_m: f32, steps: usize) -> f32 {
    let clip = set.clip(id);
    let v = clip.speed();
    let steps = steps.max(8);
    let dt = clip.duration / steps as f32;
    let track: Vec<Vec<V3>> = (0..=steps).map(|k| joints_at(set, id, (k as f32 * dt).min(clip.duration))).collect();
    let mut worst = 0.0f32;
    for &f in feet {
        let low = track.iter().map(|j| j[f][1]).fold(f32::MAX, f32::min);
        // Looping clips wrap; others clamp at the ends.
        let looping = clip.looping == LoopMode::Loop;
        let planted = |k: isize| -> bool {
            let i = if looping { k.rem_euclid(steps as isize) } else { k.clamp(0, steps as isize) } as usize;
            track[i][f][1] <= low + planted_m
        };
        for k in 0..steps {
            let (a, b) = (track[k][f], track[k + 1][f]);
            let ki = k as isize;
            if (ki - 2..=ki + 3).all(planted) {
                // World position = model position + root travel.
                let dx = b[0] - a[0];
                let dz = (b[2] - a[2]) + v * dt;
                worst = worst.max((dx * dx + dz * dz).sqrt() / dt);
            }
        }
    }
    worst
}

/// Largest joint distance (m) between the first and last frame of a looping clip (0 = a
/// seamless loop); `None` for clips that don't loop.
pub fn loop_seam(set: &AnimSet, id: ClipId) -> Option<f32> {
    let clip = set.clip(id);
    if clip.looping != LoopMode::Loop {
        return None;
    }
    let a = joints_at(set, id, 0.0);
    // Just before the end (the end itself wraps to 0).
    let b = joints_at(set, id, clip.duration);
    Some(a.iter().zip(&b).map(|(p, q)| math3::length(math3::sub(*p, *q))).fold(0.0, f32::max))
}

/// A stable hash of the local pose sampled every `step` seconds over the clip (exact float
/// bits): the same clip must hash the same on every platform (F2).
pub fn pose_hash(set: &AnimSet, id: ClipId, step: f32) -> u64 {
    let clip = set.clip(id);
    let mut h = StableHasher::new();
    let mut pose = set.skeleton.rest_pose();
    let n = (clip.duration / step) as usize;
    for k in 0..=n {
        blend::sample(&set.skeleton, clip, set.track_bones(id), k as f32 * step, Base::Rest, &mut pose);
        for t in &pose.local {
            t.t.iter().chain(t.r.iter()).chain(t.s.iter()).for_each(|v| h.write_f32(*v));
        }
    }
    h.finish()
}
