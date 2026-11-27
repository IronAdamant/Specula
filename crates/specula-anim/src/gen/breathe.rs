//! The `breathe` generator: breathing only, for an additive layer.

use super::*;

/// Breathing only, for an additive layer.
pub(super) fn breathe(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let chest = *body.spine.last().ok_or_else(|| err("spine", AnimErrorCode::MissingField, "breathe needs a \"spine\" (its last bone breathes)"))?;
    let deg = o.num("deg", 3.0, -30.0, 30.0)?;
    let swell = o.num("scale", 0.03, 0.0, 0.3)?;
    let breaths = o.num("breaths", 1.0, 1.0, 32.0)?;
    if breaths != specula_core::dmath::round(breaths) {
        return Err(err(&o.at("breaths"), AnimErrorCode::OutOfRange, "breaths per loop must be a whole number"));
    }
    let frames = times
        .iter()
        .map(|&t| {
            let mut f = Frame::new(skel.len(), body.legs.len());
            // In through the first half (chest lifts back and swells), out through the second.
            let k = 0.5 - 0.5 * cos_turns(breaths * t / duration);
            f.rot(chest, qaxis([1.0, 0.0, 0.0], -deg * k));
            if swell > 0.0 {
                f.scale(chest, [1.0 + swell * k, 1.0 + 0.5 * swell * k, 1.0 + swell * k]);
            }
            if let Some(h) = body.head {
                f.rot(h, qaxis([1.0, 0.0, 0.0], 0.6 * deg * k));
            }
            apply_wobbles(&mut f, body, t / duration);
            f
        })
        .collect();
    Ok(Out { looping: LoopMode::Loop, distance: 0.0, events: Vec::new(), curves: Vec::new(), frames })
}
