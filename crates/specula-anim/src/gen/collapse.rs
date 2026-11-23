//! The `collapse` generator: deaths (topple, crumble, scatter, fade).

use super::*;

pub(super) fn collapse(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let style = o.str("style")?.unwrap_or("topple").to_string();
    let height = skel.heads.iter().chain(skel.tails.iter()).map(|p| p[1]).fold(0.0f32, f32::max).max(0.1);
    let hip_h = body.pelvis.map_or(0.5 * height, |p| skel.heads[p][1]);
    let drop = o.num("drop", 0.45 * hip_h, 0.0, 100.0)?;
    let curl = o.num("curl", 50.0, -180.0, 180.0)?;
    let fall = o.num("fall", 82.0, 0.0, 180.0)?;
    let lie = o.num("lie", 0.12 * height, 0.0, 100.0)?;
    let reach = o.num("reach", 0.0, -180.0, 180.0)?;
    let dir = match o.str("direction")?.unwrap_or("forward") {
        "forward" => ([1.0, 0.0, 0.0], 1.0),
        "back" => ([1.0, 0.0, 0.0], -1.0),
        "left" => ([0.0, 0.0, 1.0], -1.0),
        "right" => ([0.0, 0.0, 1.0], 1.0),
        other => return Err(err(&o.at("direction"), AnimErrorCode::UnknownName, format!("direction '{other}' (forward, back, left, right)"))),
    };
    let sink = o.num("sink", 0.5, 0.0, 100.0)?;
    let shrink = o.num("shrink", 0.35, 0.02, 1.0)?;
    let rise = o.num("rise", 1.0, -100.0, 100.0)?;
    let spin = o.num("spin", 270.0, -3600.0, 3600.0)?;
    let spread = o.num("spread", 2.5, 0.0, 50.0)?;
    // Scatter: the height the bones come to rest at (the bone line, so the bones' own thickness
    // lies on the ground rather than in it).
    let floor = o.num("floor", 0.06, 0.0, 10.0)?;
    let seed = o.num("seed", 1.0, 0.0, 1e9)? as u64;
    let ns = body.spine.len().max(1) as f32;
    let bend_body = |f: &mut Frame, w: f32, crouch_drop: f32| {
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch_drop, 0.0]);
        }
        for &b in &body.spine {
            f.rot(b, qaxis([1.0, 0.0, 0.0], curl * w / ns));
        }
        if let Some(h) = body.head {
            f.rot(h, qaxis([1.0, 0.0, 0.0], 0.6 * curl * w));
        }
    };
    let mut events = Vec::new();
    let mut curves = Vec::new();
    let frames = match style.as_str() {
        "topple" | "crumble" => {
            let crumble = style == "crumble";
            let (buckle_end, fall_end) = if crumble { (0.35, 0.55) } else { (0.3, 0.72) };
            let fall_deg = if crumble { fall.min(40.0) } else { fall };
            let make = |t: f32, crouch: f32| -> Frame {
                let q = t / duration;
                let mut f = Frame::new(skel.len(), body.legs.len());
                let buckle = smoothstep(q / buckle_end);
                let u = ((q - buckle_end) / (fall_end - buckle_end)).clamp(0.0, 1.0);
                // Gravity: the angle grows with the square of time, then a small bounce.
                let settle = ((q - fall_end) / (1.0 - fall_end)).clamp(0.0, 1.0);
                let bounce = 5.0 * specula_core::dmath::sin(std::f32::consts::PI * settle.min(0.5) * 2.0) * (1.0 - settle);
                let angle = fall_deg * u * u - if q > fall_end { bounce } else { 0.0 };
                f.mov(0, [0.0, lie * (angle / fall_deg.max(1e-3)).clamp(0.0, 1.0), 0.0]);
                f.rot(0, qaxis(dir.0, dir.1 * angle));
                bend_body(&mut f, buckle + 0.5 * u, drop * buckle + crouch);
                for a in &body.arms {
                    // Arms fly up as the body drops, then splay.
                    f.rot(a.upper, qeuler([-a.swing * (buckle - 0.6 * u) - reach * u, 0.0, a.roll * u]));
                    if let Some(fo) = a.fore {
                        f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * buckle));
                    }
                }
                if crumble {
                    let c = smoothstep(((q - fall_end) / (1.0 - fall_end)).clamp(0.0, 1.0));
                    // The whole body shrinks towards the ground and sinks into it.
                    f.mov(0, [0.0, -sink * c, 0.0]);
                    f.scale(0, [1.0 - (1.0 - shrink) * c; 3]);
                }
                planted(&mut f, body);
                apply_wobbles(&mut f, body, q);
                f
            };
            events.push(Event { name: "impact".into(), time: fall_end * duration, data: String::new() });
            if crumble {
                events.push(Event { name: "crumble".into(), time: fall_end * duration, data: String::new() });
            }
            solve_all(skel, body, &make, times, false, "drop")?.0
        }
        "scatter" => {
            let stagger = 0.15f32;
            let make = |t: f32| -> Frame {
                let q = (t / duration).min(stagger) / stagger;
                let mut f = Frame::new(skel.len(), body.legs.len());
                bend_body(&mut f, -0.4 * specula_core::dmath::sin(std::f32::consts::PI * q), 0.05 * hip_h * q);
                planted(&mut f, body);
                f
            };
            let mut base = make(stagger * duration);
            solve(skel, body, &mut base);
            let g0 = fk(skel, &base.d);
            // Each bone (not the root) gets a seeded launch velocity and spin.
            let mut rng = Rng::new(seed, 7);
            let launch: Vec<(V3, V3, f32)> = (0..skel.len())
                .map(|b| {
                    let ang = rng.range_f32(0.0, 1.0);
                    let sp = spread * rng.range_f32(0.4, 1.0);
                    let up = rng.range_f32(1.0, 3.0);
                    let axis = math3::normalize([rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0)]);
                    let w = rng.range_f32(180.0, 720.0) * if b % 2 == 0 { 1.0 } else { -1.0 };
                    ([sp * cos_turns(ang), up, sp * sin_turns(ang)], axis, w)
                })
                .collect();
            events.push(Event { name: "shatter".into(), time: stagger * duration, data: String::new() });
            times
                .iter()
                .map(|&t| {
                    if t <= stagger * duration {
                        let mut f = make(t);
                        solve(skel, body, &mut f);
                        return f;
                    }
                    let tau = t - stagger * duration;
                    let mut g = g0.clone();
                    for b in 1..skel.len() {
                        let (v, axis, w) = launch[b];
                        let p0 = g0[b].t;
                        // Ballistic flight until the joint reaches the floor, then it rests.
                        let land = {
                            let (a, bb, c) = (-4.9f32, v[1], p0[1] - floor);
                            let disc = (bb * bb - 4.0 * a * c).max(0.0);
                            ((-bb - disc.sqrt()) / (2.0 * a)).max(0.0)
                        };
                        let tt = tau.min(land);
                        let off = [v[0] * tt, v[1] * tt - 4.9 * tt * tt, v[2] * tt];
                        let rm = Affine { cols: math::qcols(qaxis(axis, w * tt)), t: [0.0; 3] };
                        // Spin about the joint, then move it.
                        let about = Affine::translate(math3::add(p0, off)).then_after(&rm.then_after(&Affine::translate(math3::scale(p0, -1.0))));
                        g[b] = about.then_after(&g0[b]);
                        // A spinning bone never passes into the ground: its lower end stays on
                        // the floor (it rolls over it instead).
                        let tail = g[b].apply_point(math3::sub(skel.tails[b], skel.heads[b]));
                        let under = floor - g[b].t[1].min(tail[1]);
                        if under > 0.0 {
                            g[b].t[1] += under;
                        }
                    }
                    let mut f = Frame::new(skel.len(), body.legs.len());
                    f.d = deltas_from_globals(skel, &g);
                    f
                })
                .collect()
        }
        "fade" => {
            curves.push(Curve { name: "opacity".into(), keys: Keys { interp: Interp::Linear, times: vec![0.0, 0.3 * duration, duration], values: vec![[1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]] } });
            events.push(Event { name: "vanish".into(), time: duration, data: String::new() });
            let pelvis = body.pelvis.unwrap_or(0);
            times
                .iter()
                .map(|&t| {
                    let q = t / duration;
                    let e = smoothstep(q);
                    let mut f = Frame::new(skel.len(), body.legs.len());
                    f.mov(pelvis, [0.0, rise * e, 0.0]);
                    f.rot(pelvis, qaxis([0.0, 1.0, 0.0], spin * q * q));
                    f.scale(pelvis, [1.0 - (1.0 - shrink) * e; 3]);
                    bend_body(&mut f, -0.5 * e, 0.0);
                    for a in &body.arms {
                        f.rot(a.upper, qeuler([-a.swing * e, 0.0, a.roll * e]));
                    }
                    apply_wobbles(&mut f, body, q);
                    f
                })
                .collect()
        }
        other => return Err(err(&o.at("style"), AnimErrorCode::UnknownName, format!("collapse style '{other}' (topple, crumble, scatter, fade)"))),
    };
    Ok(Out { looping: LoopMode::Once, distance: 0.0, events, curves, frames })
}
