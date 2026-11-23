//! The `swing` generator: rest, windup, strike, hold, rest.

use super::*;

pub(super) fn swing(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let wu = o.num("windup", 0.35, 0.0, 1.0)?;
    let st = o.num("strike", 0.15, 0.01, 1.0)?;
    let hold = o.num("hold", 0.1, 0.0, 1.0)?;
    if wu + st + hold > 0.99 {
        return Err(err(&o.at("hold"), AnimErrorCode::OutOfRange, "windup + strike + hold must leave time to recover (< 0.99)").with_numbers((wu + st + hold) as f64, 0.99));
    }
    let windup = parse_pose(o, "windup_pose", skel)?;
    let strike = parse_pose(o, "strike_pose", skel)?;
    let event = o.str("event")?.unwrap_or("hit").to_string();
    let frames_make = |t: f32, crouch: f32| -> Frame {
        let q = t / duration;
        let mut f = Frame::new(skel.len(), body.legs.len());
        let (ww, ws) = if q < wu {
            (smoothstep(q / wu), 0.0)
        } else if q < wu + st {
            let u = (q - wu) / st;
            let e = u * u; // accelerate into the blow
            (1.0 - e, e)
        } else if q < wu + st + hold {
            (0.0, 1.0)
        } else {
            let u = (q - wu - st - hold) / (1.0 - wu - st - hold);
            (0.0, 1.0 - smoothstep(u))
        };
        apply_poses(&mut f, &[(&windup, ww), (&strike, ws)]);
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch, 0.0]);
        }
        planted(&mut f, body);
        apply_wobbles(&mut f, body, q);
        f
    };
    let (frames, _) = solve_all(skel, body, &frames_make, times, true, "strike_pose")?;
    let events = vec![Event { name: event, time: (wu + st) * duration, data: String::new() }];
    Ok(Out { looping: LoopMode::Once, distance: 0.0, events, curves: Vec::new(), frames })
}
