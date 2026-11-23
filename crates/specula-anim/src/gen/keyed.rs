//! The `keys` and `pose` generators: keyframed and held poses.

use super::*;

/// `keys` (poses at times) and `pose` (one held pose).
pub(super) fn keyed(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32, single: bool, looping: LoopMode) -> Result<Out, AnimError> {
    let mut keys: Vec<(f32, Vec<PoseKey>)> = Vec::new();
    let interp = if single {
        let pose = parse_pose(o, "pose", skel)?;
        if pose.is_empty() {
            return Err(err(&o.at("pose"), AnimErrorCode::MissingField, "a pose names bones: {\"bone\": {\"rot\": [x, y, z], \"move\": [x, y, z], \"scale\": s}}"));
        }
        keys.push((0.0, pose));
        Interp::Step
    } else {
        let interp = match o.str("interp")? {
            None | Some("cubic") => Interp::Cubic,
            Some("linear") => Interp::Linear,
            Some("step") => Interp::Step,
            Some(other) => return Err(err(&o.at("interp"), AnimErrorCode::UnknownName, format!("unknown interp '{other}' (step, linear, cubic)"))),
        };
        let kp = o.at("keys");
        let items = capped(o, "keys", MAX_POSE_KEYS)?.ok_or_else(|| err(&kp, AnimErrorCode::MissingField, "missing \"keys\": [{time, pose}, ...]"))?;
        if items.is_empty() {
            return Err(err(&kp, AnimErrorCode::MissingField, "at least one key"));
        }
        for (i, k) in items.iter().enumerate() {
            let p = format!("{kp}[{i}]");
            let mut ko = Obj::new(k, &p, "a key")?;
            let t = ko.req_num("time", 0.0, duration)?;
            if let Some((prev, _)) = keys.last() {
                if t <= *prev {
                    return Err(err(&ko.at("time"), AnimErrorCode::NotIncreasing, format!("{t} s is not after {prev} s")));
                }
            }
            let pose = parse_pose(&mut ko, "pose", skel)?;
            ko.finish("time, pose")?;
            keys.push((t, pose));
        }
        interp
    };
    // A looping clip ends where it starts; a once clip holds its last key.
    let closed = looping == LoopMode::Loop && keys.len() > 1;
    let key_at = |i: isize| -> (f32, &[PoseKey]) {
        let n = keys.len() as isize;
        if closed {
            let k = i.rem_euclid(n) as usize;
            let wraps = i.div_euclid(n) as f32;
            (keys[k].0 + wraps * duration, &keys[k].1)
        } else {
            let k = i.clamp(0, n - 1) as usize;
            (keys[k].0, &keys[k].1)
        }
    };
    let make = |t: f32, crouch: f32| -> Frame {
        let mut f = Frame::new(skel.len(), body.legs.len());
        // The key span holding `t` (wrapping for loops).
        let n = keys.len() as isize;
        let mut i = keys.iter().rposition(|(kt, _)| *kt <= t).map_or(-1, |i| i as isize);
        if !closed && i < 0 {
            i = 0;
        }
        let (t0, p0) = key_at(i);
        let (t1, p1) = key_at(i + 1);
        let u = if t1 > t0 && (closed || i + 1 < n) { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
        match interp {
            Interp::Step => apply_poses(&mut f, &[(p0, 1.0)]),
            Interp::Linear => apply_poses(&mut f, &[(p0, 1.0 - u), (p1, u)]),
            Interp::Cubic => {
                // Catmull–Rom weights over the four keys around the span (uniform).
                let (_, pm) = key_at(i - 1);
                let (_, p2) = key_at(i + 2);
                let (u2, u3) = (u * u, u * u * u);
                let w = [-0.5 * u3 + u2 - 0.5 * u, 1.5 * u3 - 2.5 * u2 + 1.0, -1.5 * u3 + 2.0 * u2 + 0.5 * u, 0.5 * u3 - 0.5 * u2];
                apply_poses(&mut f, &[(pm, w[0]), (p0, w[1]), (p1, w[2]), (p2, w[3])]);
            }
        }
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch, 0.0]);
        }
        planted(&mut f, body);
        apply_wobbles(&mut f, body, t / duration);
        f
    };
    let (frames, _) = solve_all(skel, body, &make, times, !body.legs.is_empty(), "keys")?;
    Ok(Out { looping, distance: 0.0, events: Vec::new(), curves: Vec::new(), frames })
}
