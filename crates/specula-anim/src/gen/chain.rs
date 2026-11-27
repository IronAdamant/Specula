//! The `chain` generator: a spring chain driven by a source clip.

use super::*;

/// Secondary motion: a spring chain driven by a source clip, baked over it.
pub(super) fn chain(o: &mut Obj, set: &AnimSet, source: crate::set::ClipId, times: &[f32], duration: f32, looping: LoopMode) -> Result<Out, AnimError> {
    let skel = &set.skeleton;
    let bones = bone_list(o, "bones", skel)?;
    if bones.is_empty() || bones.len() > MAX_CHAIN {
        return Err(err(&o.at("bones"), AnimErrorCode::OutOfRange, format!("a chain simulates 1..={MAX_CHAIN} bones, each the parent of the next")));
    }
    for w in bones.windows(2) {
        child_of(skel, w[1], w[0], &o.at("bones"))?;
    }
    let stiffness = o.num("stiffness", 0.2, 0.0, 1.0)?;
    let damping = o.num("damping", 0.1, 0.0, 1.0)?;
    let gravity = o.num("gravity", 9.8, -50.0, 50.0)?;
    let warmup = o.num("cycles", 3.0, 1.0, 16.0)? as usize;
    let src = set.clip(source);
    let speed = src.speed();
    // Points: each chain bone's head, then the last bone's tail.
    let last = *bones.last().expect("non-empty");
    let mut tail = math3::sub(skel.tails[last], skel.heads[last]);
    if math3::length(tail) < 1e-4 {
        tail = [0.0, -0.1, 0.0];
    }
    let n = bones.len() + 1;
    let point_of = |model: &[Affine], j: usize| -> V3 {
        if j < bones.len() {
            model[bones[j]].t
        } else {
            model[last].apply_point(tail)
        }
    };
    let rest_len: Vec<f32> = {
        let mut m = Vec::new();
        skel.model_space(&skel.rest_pose(), &mut m);
        (1..n).map(|j| math3::length(math3::sub(point_of(&m, j), point_of(&m, j - 1)))).collect()
    };
    let bound = set.track_bones(source).to_vec();
    let sample = |t: f32, pose: &mut crate::skeleton::Pose| crate::blend::sample(skel, src, &bound, src.local_time(t as f64), crate::blend::Base::Rest, pose);
    let dt = if times.len() > 1 { times[1] - times[0] } else { duration };
    let steps = times.len() - 1;
    let total_cycles = if looping == LoopMode::Loop { warmup + 2 } else { 1 };
    let mut pose = skel.rest_pose();
    let mut model: Vec<Affine> = Vec::new();
    sample(0.0, &mut pose);
    skel.model_space(&pose, &mut model);
    let mut x: Vec<V3> = (0..n).map(|j| point_of(&model, j)).collect();
    let mut prev = x.clone();
    // Per recorded cycle: per key, the chain bones' local rotations.
    let mut cycles: Vec<Vec<Vec<Q>>> = Vec::new();
    for c in 0..total_cycles {
        let mut rec: Vec<Vec<Q>> = Vec::with_capacity(times.len());
        for (k, &t) in times.iter().enumerate() {
            if k == steps && c + 1 < total_cycles {
                // The next cycle's first key is this time.
                continue;
            }
            sample(t, &mut pose);
            skel.model_space(&pose, &mut model);
            // The source travels `speed` m/s along +Z: the chain trails in the world.
            let travel = [0.0, 0.0, speed * (c as f32 * duration + t)];
            let target: Vec<V3> = (0..n).map(|j| math3::add(point_of(&model, j), travel)).collect();
            if c == 0 && k == 0 {
                x.clone_from(&target);
                prev.clone_from(&target);
            }
            x[0] = target[0];
            for j in 1..n {
                let v = math3::scale(math3::sub(x[j], prev[j]), 1.0 - damping);
                prev[j] = x[j];
                x[j] = math3::add(math3::add(x[j], v), [0.0, -gravity * dt * dt, 0.0]);
                x[j] = math3::add(x[j], math3::scale(math3::sub(target[j], x[j]), stiffness));
            }
            for j in 1..n {
                let d = math3::sub(x[j], x[j - 1]);
                let l = math3::length(d);
                x[j] = if l > 1e-6 { math3::add(x[j - 1], math3::scale(d, rest_len[j - 1] / l)) } else { target[j] };
            }
            // Turn each chain bone so it points where the simulation put its child point.
            for (j, &b) in bones.iter().enumerate() {
                let have = math3::sub(point_of(&model, j + 1), point_of(&model, j));
                let want = math3::sub(math3::sub(x[j + 1], travel), point_of(&model, j));
                let turn = crate::math::qarc(have, want);
                let g = rotation_of(&model[b]);
                pose.local[b].r = qnorm(qmul(pose.local[b].r, qmul(qconj(g), qmul(turn, g))));
                crate::ik::fk_from(skel, &pose, &mut model, b);
            }
            rec.push(bones.iter().map(|&b| pose.local[b].r).collect());
        }
        cycles.push(rec);
    }
    // Loops: the last cycle, blended at its end into the cycle before it (whose end runs into
    // the last cycle's start), so the clip ends where it begins.
    let rot_at = |k: usize, j: usize| -> Q {
        let last_c = &cycles[cycles.len() - 1];
        if looping != LoopMode::Loop {
            return last_c[k.min(last_c.len() - 1)][j];
        }
        let prev_c = &cycles[cycles.len() - 2];
        let b = if k < last_c.len() { last_c[k][j] } else { cycles[cycles.len() - 1][0][j] };
        let a = if k < prev_c.len() { prev_c[k][j] } else { last_c[0][j] };
        let w = smoothstep((k as f32 / steps as f32 - 0.7) / 0.3);
        crate::math::qnlerp(b, a, w)
    };
    let mut frames = Vec::with_capacity(times.len());
    for (k, &t) in times.iter().enumerate() {
        sample(t, &mut pose);
        let mut f = Frame::new(skel.len(), 0);
        for b in 0..skel.len() {
            let r = skel.rest[b];
            f.d[b] = Transform { t: math3::sub(pose.local[b].t, r.t), r: pose.local[b].r, s: [pose.local[b].s[0] / r.s[0], pose.local[b].s[1] / r.s[1], pose.local[b].s[2] / r.s[2]] };
        }
        for (j, &b) in bones.iter().enumerate() {
            f.d[b].r = rot_at(k, j);
        }
        frames.push(f);
    }
    Ok(Out { looping, distance: src.distance, events: src.events.clone(), curves: src.curves.clone(), frames })
}
