//! The `gait` and `idle` generators: a walk, run or stomp cycle, and standing.

use super::*;

pub(super) fn gait(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32, idle: bool) -> Result<Out, AnimError> {
    let stride = if idle { 0.0 } else { o.req_num("stride", 0.0, 50.0)? };
    let duty = o.num("duty", 0.6, 0.2, 0.9)?;
    // The part of the swing at each end where the foot rises or lands in place (moving with the
    // ground), so it never scrapes forward while its toe is still down. Runs take less.
    let clear = o.num("clear", if duty >= 0.5 { 0.12 } else { 0.06 }, 0.0, 0.4)?;
    let lift = o.num("lift", if idle { 0.0 } else { 0.1 }, 0.0, 3.0)?;
    let bounce = o.num("bounce", if idle { 0.006 } else { 0.03 }, 0.0, 2.0)?;
    let sway = o.num("sway", if idle { 0.015 } else { 0.02 }, 0.0, 2.0)?;
    let roll = o.num("roll", if idle { 1.5 } else { 4.0 }, -60.0, 60.0)?;
    let twist = o.num("twist", if idle { 0.0 } else { 6.0 }, -60.0, 60.0)?;
    let lean = o.num("lean", 0.0, -80.0, 80.0)?;
    let nod = o.num("nod", if idle { 1.0 } else { 3.0 }, -45.0, 45.0)?;
    let limp = o.num("limp", 0.0, 0.0, 1.0)?;
    let breathe = o.num("breathe", if idle { 2.5 } else { 0.0 }, 0.0, 30.0)?;
    let breathe_scale = o.num("breathe_scale", if idle { 0.02 } else { 0.0 }, 0.0, 0.3)?;
    let breaths = o.num("breaths", 1.0, 0.0, 32.0)?;
    let look = o.num("look", 0.0, -90.0, 90.0)?;
    let fixed_crouch = match o.get("crouch") {
        Some(v) => Some(num(v, &o.at("crouch"), 0.0, max_crouch(skel, body))?),
        None => None,
    };
    let step_name = match o.get("footstep") {
        None => (!idle).then(|| "footstep".to_string()),
        Some(Json::Bool(false)) => None,
        Some(Json::Str(s)) if !s.is_empty() => Some(s.clone()),
        Some(_) => return Err(err(&o.at("footstep"), AnimErrorCode::WrongType, "an event name, or false for none")),
    };
    if breaths != specula_core::dmath::round(breaths) {
        return Err(err(&o.at("breaths"), AnimErrorCode::OutOfRange, "breaths per loop must be a whole number"));
    }
    let s0 = body.legs.first().map_or(0.0, |l| l.phase);
    let side0 = body.legs.first().map_or(1.0, |l| if l.ankle[0] < 0.0 { -1.0 } else { 1.0 });
    let walk = duty >= 0.5;
    let stance_len = stride * duty;
    let make = |t: f32, crouch: f32| -> Frame {
        let s = t / duration;
        let mut f = Frame::new(skel.len(), body.legs.len());
        let m = s - s0 - duty * 0.5; // 0 at leg 0's mid-stance
        let w1 = 0.5 - 0.5 * cos_turns(m); // 1 at the other leg's mid-stance
        let vert = bounce * cos_turns(2.0 * m) * if walk { 1.0 } else { -1.0 } - limp * bounce * 2.5 * w1 * w1 - crouch;
        let lateral = sway * side0 * cos_turns(m) * (1.0 + 0.8 * limp * w1);
        let roll_z = roll * side0 * cos_turns(m) * (1.0 + limp * w1);
        let yaw = -twist * side0 * sin_turns(m);
        if let Some(p) = body.pelvis {
            f.mov(p, [lateral, vert, 0.0]);
            f.rot(p, qeuler([0.0, yaw, roll_z]));
        }
        let ns = body.spine.len().max(1) as f32;
        let breath = sin_turns(breaths * s);
        for (k, &b) in body.spine.iter().enumerate() {
            let last = k + 1 == body.spine.len();
            let bob = 0.25 * bounce * 40.0 * cos_turns(2.0 * m + 0.1) * if walk { 1.0 } else { -1.0 };
            f.rot(b, qeuler([lean / ns + bob / ns - if last { breathe * breath } else { 0.0 }, -0.9 * yaw / ns, -0.6 * roll_z / ns]));
            if last && breathe_scale > 0.0 {
                f.scale(b, [1.0 + breathe_scale * breath, 1.0 + 0.5 * breathe_scale * breath, 1.0 + breathe_scale * breath]);
            }
        }
        if let Some(h) = body.head {
            f.rot(h, qeuler([-0.5 * lean + nod * cos_turns(2.0 * m - 0.15), 0.4 * yaw + look * sin_turns(s), -0.5 * roll_z]));
        }
        for a in &body.arms {
            let c = cos_turns(s - a.phase - a.lag);
            f.rot(a.upper, qeuler([-a.swing * c, 0.0, a.roll * cos_turns(m - a.lag) * side0]));
            if let Some(fo) = a.fore {
                f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * (0.7 + 0.3 * c)));
            }
        }
        for (i, l) in body.legs.iter().enumerate() {
            let p = frac(s - l.phase);
            let (dz, y, pitch) = if p < duty {
                (stance_len * (0.5 - p / duty), 0.0, 0.0)
            } else {
                let q = (p - duty) / (1.0 - duty);
                let up = specula_core::dmath::sin(std::f32::consts::PI * q);
                // The stance's backward speed (per unit of swing): the foot rises and lands
                // moving with the ground over the first and last `clear` of the swing, and
                // in between a Hermite carries it forward with matching end slopes, so it
                // leaves and meets the ground at zero ground speed.
                let m = -stance_len * (1.0 - duty) / duty;
                let (za, zb) = (-0.5 * stance_len + m * clear, 0.5 * stance_len - m * clear);
                let z = if q < clear {
                    -0.5 * stance_len + m * q
                } else if q > 1.0 - clear {
                    0.5 * stance_len + m * (q - 1.0)
                } else {
                    let span = (1.0 - 2.0 * clear).max(1e-6);
                    let u = (q - clear) / span;
                    let mu = m * span;
                    let (u2, u3) = (u * u, u * u * u);
                    (2.0 * u3 - 3.0 * u2 + 1.0) * za + (u3 - 2.0 * u2 + u) * mu + (-2.0 * u3 + 3.0 * u2) * zb + (u3 - u2) * mu
                };
                // The toe lifts through the swing (dorsiflexion) so it clears the ground.
                (z, lift * l.lift * up, -14.0 * up * (lift * l.lift / 0.1).min(1.0))
            };
            f.feet[i] = Some(([l.plant[0], l.plant[1] + y, l.plant[2] + dz], pitch));
        }
        apply_wobbles(&mut f, body, s);
        f
    };
    let (frames, _crouch) = match fixed_crouch {
        Some(c) => solve_all(skel, body, &|t, _| make(t, c), times, false, "crouch")?,
        None => solve_all(skel, body, &make, times, true, "stride")?,
    };
    let events = match step_name {
        Some(nm) => footstep_events(body, skel, duration, &nm, 0.0),
        None => Vec::new(),
    };
    Ok(Out { looping: LoopMode::Loop, distance: stride, events, curves: Vec::new(), frames })
}
