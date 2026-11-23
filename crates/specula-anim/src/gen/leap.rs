//! The `leap` generator: crouch, a ballistic arc, landing.

use super::*;

pub(super) fn leap(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let distance = o.num("distance", 3.0, 0.0, 50.0)?;
    let height = o.num("height", 0.8, 0.0, 20.0)?;
    let crouch_amt = o.num("crouch", 0.2, 0.0, 5.0)?;
    let tuck = o.num("tuck", 0.35, 0.0, 5.0)?;
    let split = o.num("split", 0.25, 0.0, 5.0)?;
    let ant = o.num("anticipation", 0.25, 0.05, 0.6)?;
    let fl = o.num("flight", 0.45, 0.1, 0.8)?;
    let lean = o.num("lean", 20.0, -80.0, 80.0)?;
    if ant + fl > 0.95 {
        return Err(err(&o.at("flight"), AnimErrorCode::OutOfRange, "anticipation + flight must leave time to land (< 0.95)"));
    }
    let ns = body.spine.len().max(1) as f32;
    let make = |t: f32, crouch: f32| -> Frame {
        let q = t / duration;
        let mut f = Frame::new(skel.len(), body.legs.len());
        let (z, y, dip, lean_w, arm, air) = if q < ant {
            let e = smoothstep(q / ant);
            (0.0, 0.0, crouch_amt * e, 0.5 * e, e, 0.0)
        } else if q < ant + fl {
            let u = (q - ant) / fl;
            let rise = (1.0 - smoothstep(u / 0.25)) * crouch_amt;
            (distance * u, height * 4.0 * u * (1.0 - u), rise, 0.5 + 0.5 * specula_core::dmath::sin(std::f32::consts::PI * u), 1.0 - 2.0 * smoothstep(u / 0.4), specula_core::dmath::sin(std::f32::consts::PI * u))
        } else {
            let u = (q - ant - fl) / (1.0 - ant - fl);
            (distance, 0.0, 0.8 * crouch_amt * specula_core::dmath::sin(std::f32::consts::PI * u.min(1.0)) * (1.0 - u), 0.5 * (1.0 - smoothstep(u)), -(1.0 - smoothstep(u)), 0.0)
        };
        f.mov(0, [0.0, y, z]);
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -dip - crouch, 0.0]);
        }
        for &b in &body.spine {
            f.rot(b, qaxis([1.0, 0.0, 0.0], lean * lean_w / ns));
        }
        if let Some(h) = body.head {
            f.rot(h, qaxis([1.0, 0.0, 0.0], -0.6 * lean * lean_w));
        }
        for a in &body.arms {
            // Back during the crouch (+), thrown forward in the air (−).
            f.rot(a.upper, qeuler([a.swing * arm, 0.0, a.roll * air]));
            if let Some(fo) = a.fore {
                f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * (0.5 + 0.5 * air)));
            }
        }
        for (i, l) in body.legs.iter().enumerate() {
            let fwd = if i % 2 == 0 { split } else { -split };
            f.feet[i] = Some(([l.plant[0], l.plant[1] + tuck * air, l.plant[2] + fwd * air], -25.0 * air));
        }
        apply_wobbles(&mut f, body, q);
        f
    };
    let (frames, _) = solve_all(skel, body, &make, times, true, "distance")?;
    // The takeoff carries the spec height (the root's apex over its takeoff height): the
    // `leap_apex` check measures against it.
    let mut events = vec![Event { name: "takeoff".into(), time: ant * duration, data: format!("{height}") }, Event { name: "land".into(), time: (ant + fl) * duration, data: String::new() }];
    events.extend(body.legs.iter().map(|l| Event { name: "footstep".into(), time: (ant + fl) * duration, data: skel.names[l.foot.unwrap_or(l.shin)].clone() }));
    Ok(Out { looping: LoopMode::Once, distance, events, curves: Vec::new(), frames })
}
