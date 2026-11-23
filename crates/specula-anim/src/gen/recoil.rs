//! The `recoil` generator: a hit or kick that springs back.

use super::*;

/// A hit or kick: rises to the pose, then a damped spring brings it back.
pub(super) fn recoil(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let pose = parse_pose(o, "pose", skel)?;
    if pose.is_empty() {
        return Err(err(&o.at("pose"), AnimErrorCode::MissingField, "a recoil needs the \"pose\" it snaps to"));
    }
    let attack = o.num("attack", 0.06, 0.0, duration * 0.5)?;
    let freq = o.num("frequency", 3.0, 0.1, 30.0)?;
    let zeta = o.num("damping", 0.35, 0.0, 0.99)?;
    let event = o.str("event")?.unwrap_or("recoil").to_string();
    let omega = std::f32::consts::TAU * freq;
    let omega_d = omega * (1.0 - zeta * zeta).sqrt();
    let make = |t: f32, crouch: f32| -> Frame {
        let mut f = Frame::new(skel.len(), body.legs.len());
        let w = if t < attack {
            smoothstep(t / attack.max(1e-4))
        } else {
            let tau = t - attack;
            specula_core::dmath::exp(-zeta * omega * tau) * specula_core::dmath::cos(omega_d * tau)
        };
        // Fade the tail out so the clip ends exactly at rest.
        let end = 1.0 - smoothstep((t / duration - 0.8) / 0.2);
        apply_poses(&mut f, &[(&pose, w * end)]);
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch, 0.0]);
        }
        planted(&mut f, body);
        apply_wobbles(&mut f, body, t / duration);
        f
    };
    let (frames, _) = solve_all(skel, body, &make, times, !body.legs.is_empty(), "pose")?;
    Ok(Out { looping: LoopMode::Once, distance: 0.0, events: vec![Event { name: event, time: 0.0, data: String::new() }], curves: Vec::new(), frames })
}
