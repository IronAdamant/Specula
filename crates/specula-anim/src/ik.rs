//! Inverse kinematics on poses: two-bone chains (legs, arms), feet planted on uneven ground and
//! look-at. Post-processes that run after blending, on a local pose plus its model-space
//! transforms (docs/design/ANIMATION.md, pipeline step 3).
//!
//! Everything here works on any skeleton (rest frames needn't be model-aligned) and keeps both
//! the local pose and its model-space transforms up to date, so solvers chain. All maths is IEEE
//! basic operations, `sqrt` and `specula_core::dmath`: the same inputs give the same bits
//! everywhere. In the engine IK is **cosmetic** (it runs per render frame on what the renderer
//! sees, against the ground it finds there); nothing in the simulation reads it.
//!
//! - [`two_bone`]: analytic two-joint IK (law of cosines), keeping the chain's current bend
//!   plane (so knees and elbows bend the way the animation bent them), optionally keeping the end
//!   bone's model-space rotation (a planted foot, a hand on a handle).
//! - [`plant_feet`]: each foot follows the ground under it ([`Ground`]), the pelvis drops (or
//!   rises) by the lowest foot's offset so the legs can reach, and lifts stay as animated.
//! - [`look_at`]: turns a chain (spine → head) so a bone's forward axis points at a target,
//!   sharing the turn along the chain, clamped to a limit.

use crate::math::{self, inverse, math3, qarc, qaxis_rad, qconj, qmul, qnorm, qrot, qscale, rotation_of, Affine, V3, Q, QID};
use crate::skeleton::{Pose, Skeleton};
use specula_core::dmath;

/// Recomputes model-space transforms of bones `from..` (their parents come first, so a single
/// pass is enough; bones before `from` are taken as correct).
pub fn fk_from(skel: &Skeleton, pose: &Pose, model: &mut [Affine], from: usize) {
    for i in from..skel.len() {
        let local = pose.local[i].to_affine();
        model[i] = match skel.parents[i] {
            Some(p) => model[p as usize].then_after(&local),
            None => local,
        };
    }
}

/// Post-multiplies bone `b`'s local rotation so its model-space rotation turns by `r` (a
/// model-space rotation): `local' = local · (G⁻¹ · r · G)`, with `G` the bone's current
/// model-space rotation.
fn turn_model(pose: &mut Pose, model: &[Affine], b: usize, r: Q) {
    let g = rotation_of(&model[b]);
    let local_r = qmul(qconj(g), qmul(r, g));
    pose.local[b].r = qnorm(qmul(pose.local[b].r, local_r));
}

/// Sets bone `b`'s local rotation so its model-space rotation becomes `want` (given the
/// parent's current model-space rotation).
fn set_model_rotation(skel: &Skeleton, pose: &mut Pose, model: &[Affine], b: usize, want: Q) {
    let parent = skel.parents[b].map_or(QID, |p| rotation_of(&model[p as usize]));
    pose.local[b].r = qnorm(qmul(qconj(parent), want));
}

fn angle_between(a: V3, b: V3) -> f32 {
    let (la, lb) = (math3::length(a), math3::length(b));
    if la < 1e-9 || lb < 1e-9 {
        return 0.0;
    }
    dmath::acos((math3::dot(a, b) / (la * lb)).clamp(-1.0, 1.0))
}

/// A two-bone chain: `upper` (thigh, upper arm) → `lower` (shin, forearm) → `end` (foot, hand),
/// each the parent of the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chain {
    pub upper: usize,
    pub lower: usize,
    pub end: usize,
    /// Model-space bend axis used when the chain is straight (no bend plane to keep): the rest
    /// pose's, or +X (knees and elbows of recipe rigs bend about X).
    pub hinge: V3,
}

impl Chain {
    /// The chain of three bones, checking each is the parent of the next.
    pub fn new(skel: &Skeleton, upper: usize, lower: usize, end: usize) -> Option<Chain> {
        if skel.parents.get(lower).copied().flatten() != Some(upper as u16) || skel.parents.get(end).copied().flatten() != Some(lower as u16) {
            return None;
        }
        let (a, b, c) = (skel.heads[upper], skel.heads[lower], skel.heads[end]);
        let n = math3::cross(math3::sub(c, a), math3::sub(b, a));
        let hinge = if math3::length(n) > 1e-6 { math3::normalize(n) } else { [1.0, 0.0, 0.0] };
        Some(Chain { upper, lower, end, hinge })
    }

    /// Full length (upper + lower) in the rest pose.
    pub fn rest_length(&self, skel: &Skeleton) -> f32 {
        math3::length(math3::sub(skel.heads[self.lower], skel.heads[self.upper])) + math3::length(math3::sub(skel.heads[self.end], skel.heads[self.lower]))
    }
}

/// What a solve did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reach {
    /// Distance (model units) from the end joint to the target after the solve (0 = reached).
    pub miss: f32,
    /// The target's distance from the chain root over the chain's length (above ~1: out of
    /// reach, the chain straightens towards it).
    pub stretch: f32,
}

/// Moves the chain so `end`'s joint reaches `target` (model space), blended by `weight` (0 =
/// untouched, 1 = on target; the target is blended, so the chain never shortens). With
/// `keep_end`, the end bone keeps its model-space rotation (a foot stays flat, a hand keeps its
/// grip). `model` must be the model-space transforms of `pose`; both are updated.
pub fn two_bone(skel: &Skeleton, pose: &mut Pose, model: &mut [Affine], chain: &Chain, target: V3, weight: f32, keep_end: bool) -> Reach {
    let (ia, ib, ic) = (chain.upper, chain.lower, chain.end);
    let a = model[ia].t;
    let b = model[ib].t;
    let c = model[ic].t;
    let lab = math3::length(math3::sub(b, a));
    let lcb = math3::length(math3::sub(c, b));
    let w = if weight.is_finite() { weight.clamp(0.0, 1.0) } else { 0.0 };
    if w <= 0.0 || lab < 1e-6 || lcb < 1e-6 || !target.iter().all(|v| v.is_finite()) {
        return Reach { miss: math3::length(math3::sub(target, c)), stretch: math3::length(math3::sub(target, a)) / (lab + lcb).max(1e-6) };
    }
    let end_rot = rotation_of(&model[ic]);
    let t = math3::lerp(c, target, w);
    let eps = 1e-4 * (lab + lcb);
    let lat_raw = math3::length(math3::sub(t, a));
    let lat = lat_raw.clamp(((lab - lcb).abs() + eps).min(lab + lcb - eps), lab + lcb - eps);
    // Current and wanted interior angles (Holden, "Simple Two Joint IK").
    let ac_ab_0 = angle_between(math3::sub(c, a), math3::sub(b, a));
    let ba_bc_0 = angle_between(math3::sub(a, b), math3::sub(c, b));
    let ac_ab_1 = dmath::acos(((lcb * lcb - lab * lab - lat * lat) / (-2.0 * lab * lat)).clamp(-1.0, 1.0));
    let ba_bc_1 = dmath::acos(((lat * lat - lab * lab - lcb * lcb) / (-2.0 * lab * lcb)).clamp(-1.0, 1.0));
    // Keep the current bend plane; a straight chain uses the hinge axis.
    let n = math3::cross(math3::sub(c, a), math3::sub(b, a));
    let axis0 = if math3::length(n) > 1e-6 * lab * lcb { math3::normalize(n) } else { chain.hinge };
    let r0 = qaxis_rad(axis0, ac_ab_1 - ac_ab_0);
    let r1 = qaxis_rad(axis0, ba_bc_1 - ba_bc_0);
    // Bend first (both joints about the same model axis), then swing the whole chain onto the target.
    turn_model(pose, model, ia, r0);
    fk_from(skel, pose, model, ia);
    turn_model(pose, model, ib, r1);
    fk_from(skel, pose, model, ib);
    let c1 = model[ic].t;
    let a1 = model[ia].t;
    let swing = qarc(math3::sub(c1, a1), math3::sub(t, a1));
    turn_model(pose, model, ia, swing);
    fk_from(skel, pose, model, ia);
    if keep_end {
        set_model_rotation(skel, pose, model, ic, end_rot);
        fk_from(skel, pose, model, ic);
    }
    Reach { miss: math3::length(math3::sub(t, model[ic].t)), stretch: lat_raw / (lab + lcb) }
}

/// The ground under a point, in world space: the highest walkable surface at `(x, z)` whose top
/// is at or below `top`, or `None` when there is none within `top - depth`. Implementations
/// should be continuous in `x` and `z` (soft edges), so a foot crossing a step moves smoothly.
pub trait Ground {
    fn height(&self, x: f32, z: f32, top: f32, depth: f32) -> Option<f32>;
}

/// A flat ground at a fixed height (tests, previews).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flat(pub f32);

impl Ground for Flat {
    fn height(&self, _x: f32, _z: f32, top: f32, depth: f32) -> Option<f32> {
        (self.0 <= top && self.0 >= top - depth).then_some(self.0)
    }
}

/// Feet that follow the ground: legs, the bone lowered or raised with them, and how far a foot
/// may be moved.
#[derive(Clone, Debug, PartialEq)]
pub struct Feet {
    pub legs: Vec<Chain>,
    /// Lowered (or raised) by the lowest foot's offset so the legs can reach.
    pub pelvis: Option<usize>,
    /// Highest step up and deepest step down a foot follows, in world metres.
    pub max_step: f32,
    pub max_drop: f32,
}

/// What [`plant_feet`] did to one leg.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FootResult {
    /// World height of the ground found under the foot (`None`: none in reach; the leg is left
    /// as animated).
    pub ground: Option<f32>,
    /// Vertical offset (world metres) applied to the foot: ground minus the animation's ground.
    pub offset: f32,
    pub reach: Reach,
}

/// What [`plant_feet`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeetResult {
    /// Per leg (first [`MAX_LEGS`]).
    pub legs: [FootResult; MAX_LEGS],
    pub count: usize,
    /// How far the pelvis moved (world metres, negative = down).
    pub pelvis: f32,
}

/// Legs [`plant_feet`] reports on.
pub const MAX_LEGS: usize = 8;

/// Plants each leg's foot on the ground under it. `to_world` places the model in the world (the
/// entity's transform; the animation's ground is the model's y = 0 plane), `weight` fades the
/// whole effect. Each foot keeps its animated height above the animation's ground (lifts and
/// steps stay as authored) but measured from the real ground under it; the pelvis moves by the
/// lowest foot's offset (clamped to `max_drop` / `max_step`); feet keep their model-space
/// rotation. `model` must be `pose`'s model-space transforms; both are updated.
pub fn plant_feet(skel: &Skeleton, pose: &mut Pose, model: &mut [Affine], feet: &Feet, to_world: &Affine, ground: &dyn Ground, weight: f32) -> FeetResult {
    let mut out = FeetResult { count: feet.legs.len().min(MAX_LEGS), ..FeetResult::default() };
    let w = if weight.is_finite() { weight.clamp(0.0, 1.0) } else { 0.0 };
    if w <= 0.0 || feet.legs.is_empty() {
        return out;
    }
    let Some(from_world) = inverse(to_world) else { return out };
    // Offsets per leg, from the feet as animated.
    let mut ankles: [V3; MAX_LEGS] = [[0.0; 3]; MAX_LEGS];
    let mut lowest: Option<f32> = None;
    for (i, leg) in feet.legs.iter().take(MAX_LEGS).enumerate() {
        let ankle = model[leg.end].t;
        let aw = to_world.apply_point(ankle);
        // The animation's ground under this foot.
        let g_anim = to_world.apply_point([ankle[0], 0.0, ankle[2]])[1];
        let found = ground.height(aw[0], aw[2], g_anim + feet.max_step, feet.max_step + feet.max_drop);
        ankles[i] = aw;
        if let Some(g) = found {
            let offset = (g - g_anim).clamp(-feet.max_drop, feet.max_step);
            out.legs[i] = FootResult { ground: Some(g), offset, reach: Reach::default() };
            lowest = Some(lowest.map_or(offset, |l: f32| l.min(offset)));
        }
    }
    let Some(lowest) = lowest else { return out };
    // Move the pelvis (world-vertical, converted into its parent's space).
    if let Some(p) = feet.pelvis {
        let dy = lowest * w;
        if dy != 0.0 {
            let d_model = from_world.apply_vector([0.0, dy, 0.0]);
            let d_local = match skel.parents[p] {
                Some(pp) => inverse(&model[pp as usize]).map_or(d_model, |m| m.apply_vector(d_model)),
                None => d_model,
            };
            pose.local[p].t = math3::add(pose.local[p].t, d_local);
            fk_from(skel, pose, model, p);
        }
        out.pelvis = dy;
    }
    for (i, leg) in feet.legs.iter().take(MAX_LEGS).enumerate() {
        let Some(_) = out.legs[i].ground else { continue };
        let target_w = math3::add(ankles[i], [0.0, out.legs[i].offset, 0.0]);
        let target = from_world.apply_point(target_w);
        out.legs[i].reach = two_bone(skel, pose, model, leg, target, w, true);
    }
    out
}

/// A look-at: `bones` (spine → head order) share the turn, the last bone's `axis` (in its own
/// frame; +Z is forward for recipe rigs) ends up pointing at the target, at most `limit_deg`
/// away from where the animation pointed it.
#[derive(Clone, Debug, PartialEq)]
pub struct LookAt {
    /// Bone and its share of the turn (shares are normalised).
    pub bones: Vec<(usize, f32)>,
    pub axis: V3,
    pub limit_deg: f32,
}

/// Turns the chain so the last bone looks at `target` (model space), blended by `weight`.
/// Returns the angle (degrees) the last bone turned. `model` must be `pose`'s model-space
/// transforms; both are updated.
pub fn look_at(skel: &Skeleton, pose: &mut Pose, model: &mut [Affine], spec: &LookAt, target: V3, weight: f32) -> f32 {
    let w = if weight.is_finite() { weight.clamp(0.0, 1.0) } else { 0.0 };
    let Some(&(last, _)) = spec.bones.last() else { return 0.0 };
    if w <= 0.0 || !target.iter().all(|v| v.is_finite()) {
        return 0.0;
    }
    let forward = |model: &[Affine]| qrot(rotation_of(&model[last]), spec.axis);
    let f0 = forward(model);
    let to = math3::sub(target, model[last].t);
    if math3::length(to) < 1e-6 || math3::length(f0) < 1e-9 {
        return 0.0;
    }
    // The goal direction: towards the target, at most `limit` from the animated forward, by weight.
    let full = qarc(f0, to);
    let angle = math::qangle_deg(full);
    let allowed = angle.min(spec.limit_deg.max(0.0)) * w;
    if allowed <= 1e-4 {
        return 0.0;
    }
    let goal_rot = qscale(full, allowed / angle.max(1e-6));
    let goal = qrot(goal_rot, f0);
    let total: f32 = spec.bones.iter().map(|(_, s)| s.max(0.0)).sum();
    let mut left = total.max(1e-6);
    for &(b, share) in &spec.bones {
        let share = share.max(0.0);
        let frac = if b == last { 1.0 } else { (share / left).min(1.0) };
        left -= share;
        let now = forward(model);
        let r = qarc(now, goal);
        let part = qscale(r, frac);
        turn_model(pose, model, b, part);
        fk_from(skel, pose, model, b);
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_model::{Bone, Rig};

    fn skel() -> Skeleton {
        let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
        Skeleton::from_rig(&Rig {
            bones: vec![
                b("root", None, [0.0; 3], [0.0; 3]),
                b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.2, 0.0]),
                b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
                b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.1, 0.0]),
                b("foot_l", Some(3), [0.15, 0.1, 0.0], [0.15, 0.03, 0.2]),
                b("thigh_r", Some(1), [-0.15, 1.0, 0.0], [-0.15, 0.55, 0.05]),
                b("shin_r", Some(5), [-0.15, 0.55, 0.05], [-0.15, 0.1, 0.0]),
                b("foot_r", Some(6), [-0.15, 0.1, 0.0], [-0.15, 0.03, 0.2]),
                b("chest", Some(1), [0.0, 1.3, 0.0], [0.0, 1.6, 0.0]),
                b("head", Some(8), [0.0, 1.6, 0.0], [0.0, 1.8, 0.1]),
            ],
        })
        .unwrap()
    }

    fn posed(s: &Skeleton) -> (Pose, Vec<Affine>) {
        let pose = s.rest_pose();
        let mut model = Vec::new();
        s.model_space(&pose, &mut model);
        (pose, model)
    }

    fn close(a: V3, b: V3, tol: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= tol)
    }

    #[test]
    fn two_bone_reaches_keeps_lengths_bend_plane_and_foot_rotation() {
        let s = skel();
        let (mut pose, mut model) = posed(&s);
        let chain = Chain::new(&s, 2, 3, 4).unwrap();
        assert!(Chain::new(&s, 2, 4, 3).is_none(), "not a parent chain");
        let foot_rot = rotation_of(&model[4]);
        let target = [0.2, 0.35, 0.25];
        let r = two_bone(&s, &mut pose, &mut model, &chain, target, 1.0, true);
        assert!(r.miss < 1e-4, "{r:?}");
        assert!(close(model[4].t, target, 1e-4));
        // Bone lengths are rigid.
        let len = |m: &[Affine], a: usize, b: usize| math3::length(math3::sub(m[b].t, m[a].t));
        let rest = s.rest_pose();
        let mut rm = Vec::new();
        s.model_space(&rest, &mut rm);
        assert!((len(&model, 2, 3) - len(&rm, 2, 3)).abs() < 1e-5 && (len(&model, 3, 4) - len(&rm, 3, 4)).abs() < 1e-5);
        // The knee still bends forward (+Z) like the rest pose's.
        assert!(model[3].t[2] > 0.05, "knee at {:?}", model[3].t);
        // The foot kept its model-space rotation.
        assert!(math::qdot(rotation_of(&model[4]), foot_rot).abs() > 1.0 - 1e-5);
        // The model transforms match the pose.
        let mut again = Vec::new();
        s.model_space(&pose, &mut again);
        for (a, b) in again.iter().zip(&model) {
            assert!(close(a.t, b.t, 1e-5));
        }
    }

    #[test]
    fn out_of_reach_straightens_and_weight_blends() {
        let s = skel();
        let (mut pose, mut model) = posed(&s);
        let chain = Chain::new(&s, 2, 3, 4).unwrap();
        let r = two_bone(&s, &mut pose, &mut model, &chain, [0.15, -2.0, 0.0], 1.0, false);
        assert!(r.stretch > 2.0 && r.miss > 1.0);
        let (mut p2, mut m2) = posed(&s);
        let before = m2[4].t;
        two_bone(&s, &mut p2, &mut m2, &chain, [0.15, 0.3, 0.3], 0.0, true);
        assert_eq!(m2[4].t, before, "weight 0 leaves the chain alone");
        two_bone(&s, &mut p2, &mut m2, &chain, [0.15, 0.3, 0.3], 0.5, true);
        assert!(close(m2[4].t, math3::lerp(before, [0.15, 0.3, 0.3], 0.5), 1e-4));
    }

    /// A step: ground at 0.2 for x > 0 (the left foot), 0 elsewhere.
    struct Step;
    impl Ground for Step {
        fn height(&self, x: f32, _z: f32, top: f32, depth: f32) -> Option<f32> {
            let g = if x > 0.0 { 0.2 } else { 0.0 };
            (g <= top && g >= top - depth).then_some(g)
        }
    }

    #[test]
    fn feet_follow_the_ground_and_the_pelvis_drops_for_the_lowest() {
        let s = skel();
        let feet = Feet { legs: vec![Chain::new(&s, 2, 3, 4).unwrap(), Chain::new(&s, 5, 6, 7).unwrap()], pelvis: Some(1), max_step: 0.4, max_drop: 0.4 };
        // The entity stands 0.1 m up (its model ground at 0.1): the left foot's ground is 0.1
        // higher, the right one's 0.1 lower.
        let to_world = Affine::translate([0.0, 0.1, 0.0]);
        let (mut pose, mut model) = posed(&s);
        let (l0, r0) = (model[4].t, model[7].t);
        let res = plant_feet(&s, &mut pose, &mut model, &feet, &to_world, &Step, 1.0);
        assert_eq!(res.count, 2);
        assert!((res.legs[0].offset - 0.1).abs() < 1e-5 && (res.legs[1].offset + 0.1).abs() < 1e-5, "{res:?}");
        assert!((res.pelvis + 0.1).abs() < 1e-5, "the pelvis drops for the lower foot");
        assert!(close(model[4].t, [l0[0], l0[1] + 0.1, l0[2]], 1e-3), "left ankle {:?}", model[4].t);
        assert!(close(model[7].t, [r0[0], r0[1] - 0.1, r0[2]], 1e-3), "right ankle {:?}", model[7].t);
        // No ground in reach (climbing a wall): nothing moves.
        let (mut p2, mut m2) = posed(&s);
        let far = Affine::translate([0.0, 5.0, 0.0]);
        let before = m2.clone();
        let res = plant_feet(&s, &mut p2, &mut m2, &feet, &far, &Step, 1.0);
        assert!(res.legs[0].ground.is_none() && res.pelvis == 0.0);
        assert!(m2.iter().zip(&before).all(|(a, b)| a.t == b.t));
        // Flat ground at the animation's own ground: nothing changes.
        let (mut p3, mut m3) = posed(&s);
        let before = m3.clone();
        plant_feet(&s, &mut p3, &mut m3, &feet, &Affine::IDENTITY, &Flat(0.0), 1.0);
        assert!(m3.iter().zip(&before).all(|(a, b)| close(a.t, b.t, 1e-5)));
    }

    #[test]
    fn look_at_points_the_head_within_its_limit() {
        let s = skel();
        let spec = LookAt { bones: vec![(8, 0.4), (9, 0.6)], axis: [0.0, 0.0, 1.0], limit_deg: 60.0 };
        let (mut pose, mut model) = posed(&s);
        // A target 45° to the left (+X) at head height.
        let head = model[9].t;
        let target = math3::add(head, [1.0, 0.0, 1.0]);
        let turned = look_at(&s, &mut pose, &mut model, &spec, target, 1.0);
        assert!((turned - 45.0).abs() < 0.1, "{turned}");
        let f = qrot(rotation_of(&model[9]), [0.0, 0.0, 1.0]);
        let want = math3::normalize(math3::sub(target, model[9].t));
        assert!(math3::dot(f, want) > 0.9999, "{f:?} vs {want:?}");
        // The chest took part of the turn.
        assert!(math::qangle_deg(pose.local[8].r) > 5.0);
        // Behind the creature: clamped to the limit.
        let (mut p2, mut m2) = posed(&s);
        let turned = look_at(&s, &mut p2, &mut m2, &spec, [0.0, 1.7, -5.0], 1.0);
        assert!((turned - 60.0).abs() < 1e-3, "{turned}");
        let f = qrot(rotation_of(&m2[9]), [0.0, 0.0, 1.0]);
        assert!((math::qangle_deg(qarc([0.0, 0.0, 1.0], f)) - 60.0).abs() < 0.5);
        // Half weight: half the turn.
        let (mut p3, mut m3) = posed(&s);
        let turned = look_at(&s, &mut p3, &mut m3, &spec, target, 0.5);
        assert!((turned - 22.5).abs() < 0.1);
        // Determinism: the same inputs, the same bits.
        let (mut p4, mut m4) = posed(&s);
        look_at(&s, &mut p4, &mut m4, &spec, target, 1.0);
        assert_eq!(p4, pose);
    }
}
