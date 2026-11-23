//! The `hover` generator: a float with a waving chain.

use super::*;

pub(super) fn hover(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let pelvis = body.pelvis.ok_or_else(|| err("pelvis", AnimErrorCode::MissingField, "hover needs a \"pelvis\" bone to float"))?;
    let bob = o.num("bob", 0.12, 0.0, 5.0)?;
    let bobs = o.num("bobs", 1.0, 1.0, 16.0)?;
    let pitch = o.num("pitch", 4.0, -60.0, 60.0)?;
    let roll = o.num("roll", 5.0, -60.0, 60.0)?;
    let yaw = o.num("yaw", 0.0, -90.0, 90.0)?;
    let lean = o.num("lean", 0.0, -80.0, 80.0)?;
    let distance = o.num("distance", 0.0, 0.0, 100.0)?;
    // Arms held back (degrees about X, positive = back), as when gliding forward.
    let sweep = o.num("sweep", 0.0, -90.0, 90.0)?;
    let (chain, chain_deg, chain_roll, delay, trail) = match o.get("chain") {
        Some(v) => {
            let mut co = Obj::new(v, &o.at("chain"), "a chain")?;
            let bones = bone_list(&mut co, "bones", skel)?;
            // `trail`: each link held back by that much (a shroud streaming behind).
            let r = (bones, co.num("deg", 8.0, -90.0, 90.0)?, co.num("roll", 4.0, -90.0, 90.0)?, co.num("delay", 0.12, 0.0, 1.0)?, co.num("trail", 0.0, -90.0, 90.0)?);
            co.finish("bones, deg, roll, delay, trail")?;
            r
        }
        None => (Vec::new(), 0.0, 0.0, 0.0, 0.0),
    };
    if bobs != specula_core::dmath::round(bobs) {
        return Err(err(&o.at("bobs"), AnimErrorCode::OutOfRange, "bobs per loop must be a whole number"));
    }
    let frames = times
        .iter()
        .map(|&t| {
            let s = t / duration;
            let mut f = Frame::new(skel.len(), body.legs.len());
            f.mov(pelvis, [0.0, bob * sin_turns(bobs * s), 0.0]);
            f.rot(pelvis, qeuler([lean + pitch * sin_turns(bobs * s + 0.25), yaw * sin_turns(s), roll * sin_turns(s + 0.1)]));
            for (k, &b) in chain.iter().enumerate() {
                let ph = bobs * s - k as f32 * delay;
                f.rot(b, qeuler([trail + chain_deg * sin_turns(ph), 0.0, chain_roll * cos_turns(s - k as f32 * delay)]));
            }
            if let Some(h) = body.head {
                f.rot(h, qeuler([-0.6 * pitch * sin_turns(bobs * s + 0.25), -0.5 * yaw * sin_turns(s), -0.5 * roll * sin_turns(s + 0.1)]));
            }
            for a in &body.arms {
                let c = sin_turns(bobs * s - a.phase - a.lag);
                f.rot(a.upper, qeuler([sweep - a.swing * c, 0.0, a.roll * cos_turns(s - a.phase)]));
                if let Some(fo) = a.fore {
                    f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * (0.6 + 0.4 * c)));
                }
            }
            apply_wobbles(&mut f, body, s);
            f
        })
        .collect();
    Ok(Out { looping: LoopMode::Loop, distance, events: Vec::new(), curves: Vec::new(), frames })
}
