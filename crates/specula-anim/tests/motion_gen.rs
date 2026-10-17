//! Agent-authored motion: keyframed poses, held poses, recoils, breathing, secondary-motion
//! chains and gaits from a rig's bone names.

use specula_anim::math::{qdot, qeuler};
use specula_anim::{gen, measure, AnimErrorCode, AnimSet, Skeleton};
use specula_core::Json;
use specula_model::{Bone, Rig};

fn rig() -> Rig {
    let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
    Rig {
        bones: vec![
            b("root", None, [0.0; 3], [0.0; 3]),
            b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.2, 0.0]),
            b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
            b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.1, 0.0]),
            b("foot_l", Some(3), [0.15, 0.1, 0.0], [0.15, 0.03, 0.2]),
            b("thigh_r", Some(1), [-0.15, 1.0, 0.0], [-0.15, 0.55, 0.05]),
            b("shin_r", Some(5), [-0.15, 0.55, 0.05], [-0.15, 0.1, 0.0]),
            b("foot_r", Some(6), [-0.15, 0.1, 0.0], [-0.15, 0.03, 0.2]),
            b("spine", Some(1), [0.0, 1.2, 0.0], [0.0, 1.4, 0.0]),
            b("chest", Some(8), [0.0, 1.4, 0.0], [0.0, 1.6, 0.0]),
            b("head", Some(9), [0.0, 1.6, 0.0], [0.0, 1.8, 0.1]),
            b("upperarm_l", Some(9), [0.3, 1.55, 0.0], [0.3, 1.25, 0.0]),
            b("forearm_l", Some(11), [0.3, 1.25, 0.0], [0.3, 1.0, 0.1]),
            b("upperarm_r", Some(9), [-0.3, 1.55, 0.0], [-0.3, 1.25, 0.0]),
            b("forearm_r", Some(13), [-0.3, 1.25, 0.0], [-0.3, 1.0, 0.1]),
            b("tail_0", Some(1), [0.0, 1.0, -0.15], [0.0, 0.9, -0.45]),
            b("tail_1", Some(15), [0.0, 0.9, -0.45], [0.0, 0.8, -0.75]),
            b("tail_2", Some(16), [0.0, 0.8, -0.75], [0.0, 0.7, -1.05]),
        ],
    }
}

fn skel() -> Skeleton {
    Skeleton::from_rig(&rig()).unwrap()
}

fn set_with(clips: &str) -> Result<AnimSet, specula_anim::AnimError> {
    AnimSet::load(&format!(r#"{{ "anims": 1, "clips": [{clips}] }}"#), skel())
}

fn local_rot(set: &AnimSet, clip: &str, bone: &str, t: f32) -> [f32; 4] {
    let id = set.id(clip).unwrap();
    let mut pose = set.skeleton.rest_pose();
    specula_anim::blend::sample(&set.skeleton, set.clip(id), set.track_bones(id), t, specula_anim::blend::Base::Rest, &mut pose);
    pose.local[set.skeleton.find(bone).unwrap()].r
}

#[test]
fn keys_hit_their_poses_and_loop_seamlessly() {
    let set = set_with(
        r#"{ "name": "wave", "kind": "keys", "duration": 1.2, "loop": "loop",
            "keys": [ { "time": 0, "pose": { "upperarm_r": { "rot": [0, 0, -80] } } },
                      { "time": 0.4, "pose": { "upperarm_r": { "rot": [0, 0, -100] }, "forearm_r": { "rot": [-30, 0, 0] } } },
                      { "time": 0.8, "pose": { "upperarm_r": { "rot": [0, 0, -70] }, "forearm_r": { "rot": [-10, 0, 0] } } } ] }"#,
    )
    .unwrap();
    for (t, deg) in [(0.0, -80.0), (0.4, -100.0), (0.8, -70.0)] {
        let q = local_rot(&set, "wave", "upperarm_r", t);
        assert!(qdot(q, qeuler([0.0, 0.0, deg])).abs() > 1.0 - 1e-4, "at {t}: {q:?}");
    }
    // A bone missing from a key is at rest there.
    assert!(qdot(local_rot(&set, "wave", "forearm_r", 0.0), [0.0, 0.0, 0.0, 1.0]).abs() > 1.0 - 1e-5);
    let id = set.id("wave").unwrap();
    assert!(measure::loop_seam(&set, id).unwrap() < 1e-5);
    // Linear keys pass exactly through the midpoint; a hold pose holds.
    let set = set_with(
        r#"{ "name": "lift", "kind": "keys", "duration": 1, "interp": "linear",
             "keys": [ { "time": 0, "pose": { "head": { "rot": [0, 0, 0] } } }, { "time": 1, "pose": { "head": { "rot": [20, 0, 0] } } } ] },
           { "name": "grip", "kind": "pose", "pose": { "forearm_l": { "rot": [-40, 0, 0] } } }"#,
    )
    .unwrap();
    assert!(qdot(local_rot(&set, "lift", "head", 0.5), qeuler([10.0, 0.0, 0.0])).abs() > 1.0 - 1e-4);
    let grip = set.clip(set.id("grip").unwrap());
    assert_eq!((grip.duration, grip.looping), (1.0, specula_anim::LoopMode::Loop));
    assert!(qdot(local_rot(&set, "grip", "forearm_l", 0.63), qeuler([-40.0, 0.0, 0.0])).abs() > 1.0 - 1e-5);
    // Errors: unsorted keys, a key past the end, an unknown bone, a pose without bones.
    let e = set_with(r#"{ "name": "x", "kind": "keys", "duration": 1, "keys": [ { "time": 0.5, "pose": {} }, { "time": 0.2, "pose": {} } ] }"#).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::NotIncreasing, "clips[0].keys[1].time"));
    let e = set_with(r#"{ "name": "x", "kind": "keys", "duration": 1, "keys": [ { "time": 2, "pose": {} } ] }"#).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::OutOfRange);
    let e = set_with(r#"{ "name": "x", "kind": "keys", "duration": 1, "keys": [ { "time": 0, "pose": { "wing": { "rot": [1, 0, 0] } } } ] }"#).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::UnknownBone);
    let e = set_with(r#"{ "name": "x", "kind": "pose", "pose": {} }"#).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::MissingField);
}

#[test]
fn keys_with_legs_keep_the_feet_planted() {
    let set = set_with(
        r#"{ "name": "duck", "kind": "keys", "duration": 1, "pelvis": "hips",
             "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }, { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r" } ],
             "keys": [ { "time": 0, "pose": {} }, { "time": 0.5, "pose": { "hips": { "move": [0, -0.25, 0] } } }, { "time": 1, "pose": {} } ] }"#,
    )
    .unwrap();
    let id = set.id("duck").unwrap();
    let foot = set.skeleton.find("foot_l").unwrap();
    let rest = measure::joints_at(&set, id, 0.0)[foot];
    for k in 0..=10 {
        let p = measure::joints_at(&set, id, k as f32 / 10.0)[foot];
        assert!((0..3).all(|i| (p[i] - rest[i]).abs() < 2e-3), "foot moved to {p:?} at {}", k as f32 / 10.0);
    }
    let hips = set.skeleton.find("hips").unwrap();
    assert!(measure::joints_at(&set, id, 0.5)[hips][1] < 0.8, "the hips went down");
}

#[test]
fn recoil_snaps_springs_back_and_ends_at_rest() {
    let set = set_with(
        r#"{ "name": "hit", "kind": "recoil", "duration": 0.8, "attack": 0.05, "frequency": 4, "damping": 0.3,
             "pose": { "chest": { "rot": [-20, 0, 8] } } }"#,
    )
    .unwrap();
    let c = set.clip(set.id("hit").unwrap());
    assert_eq!(c.looping, specula_anim::LoopMode::Once);
    assert_eq!(c.events.iter().map(|e| (e.name.as_str(), e.time)).collect::<Vec<_>>(), [("recoil", 0.0)]);
    let at = |t: f32| local_rot(&set, "hit", "chest", t);
    assert!(qdot(at(0.05), qeuler([-20.0, 0.0, 8.0])).abs() > 1.0 - 1e-4, "peak at the attack");
    // It overshoots the other way (a spring), then settles at rest.
    let overshoot = (0..60).map(|k| 0.05 + k as f32 * 0.01).any(|t| qeuler([0.0; 3]) != at(t) && specula_anim::math::euler_of(at(t))[0] > 1.0);
    assert!(overshoot, "springs past rest");
    assert!(qdot(at(0.8), [0.0, 0.0, 0.0, 1.0]).abs() > 1.0 - 1e-6, "ends at rest");
}

#[test]
fn breathing_loops() {
    let set = set_with(r#"{ "name": "breath", "kind": "breathe", "duration": 3, "spine": ["spine", "chest"], "head": "head", "deg": 4, "scale": 0.05 }"#).unwrap();
    let id = set.id("breath").unwrap();
    assert!(measure::loop_seam(&set, id).unwrap() < 1e-5);
    let mut pose = set.skeleton.rest_pose();
    specula_anim::blend::sample(&set.skeleton, set.clip(id), set.track_bones(id), 1.5, specula_anim::blend::Base::Rest, &mut pose);
    let chest = set.skeleton.find("chest").unwrap();
    assert!((pose.local[chest].s[0] - 1.05).abs() < 1e-3, "fully in at mid-loop: {:?}", pose.local[chest].s);
    assert!(set_with(r#"{ "name": "b", "kind": "breathe", "duration": 3 }"#).unwrap_err().message.contains("spine"));
}

const WALK: &str = r#"{ "name": "walk", "kind": "gait", "duration": 1.0, "stride": 0.9, "pelvis": "hips", "spine": ["spine", "chest"],
    "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }, { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r", "phase": 0.5 } ],
    "bounce": 0.06 }"#;

#[test]
fn chains_trail_their_source_and_loop() {
    let text = format!(r#"{WALK}, {{ "name": "walk_tail", "kind": "chain", "source": "walk", "bones": ["tail_0", "tail_1", "tail_2"], "stiffness": 0.15, "damping": 0.12 }}"#);
    let set = set_with(&text).unwrap();
    let (walk, tail) = (set.id("walk").unwrap(), set.id("walk_tail").unwrap());
    let c = set.clip(tail);
    // It inherits the source's timing, loop, travel and events.
    assert_eq!((c.duration, c.looping, c.distance), (1.0, specula_anim::LoopMode::Loop, 0.9));
    assert_eq!(c.events.len(), set.clip(walk).events.len());
    assert!(measure::loop_seam(&set, tail).unwrap() < 1e-5, "seamless");
    // Non-chain bones move exactly as in the source.
    let shin = set.skeleton.find("shin_l").unwrap();
    for k in 0..10 {
        let t = k as f32 * 0.1;
        assert!(measure::joints_at(&set, tail, t)[shin].iter().zip(measure::joints_at(&set, walk, t)[shin]).all(|(a, b)| (a - b).abs() < 1e-4));
    }
    // The tail sags under gravity and sways as the body bounces: it isn't rigid.
    let tip = set.skeleton.find("tail_2").unwrap();
    let ys: Vec<f32> = (0..20).map(|k| measure::joints_at(&set, tail, k as f32 * 0.05)[tip][1]).collect();
    let rigid: Vec<f32> = (0..20).map(|k| measure::joints_at(&set, walk, k as f32 * 0.05)[tip][1]).collect();
    assert!(ys.iter().zip(&rigid).any(|(a, b)| (a - b).abs() > 0.01), "{ys:?} vs {rigid:?}");
    // Bone lengths hold (rotations only).
    let mut m = Vec::new();
    let id = tail;
    let mut pose = set.skeleton.rest_pose();
    specula_anim::blend::sample(&set.skeleton, set.clip(id), set.track_bones(id), 0.37, specula_anim::blend::Base::Rest, &mut pose);
    set.skeleton.model_space(&pose, &mut m);
    let len = |a: usize, b: usize| specula_anim::math::math3::length(specula_anim::math::math3::sub(m[b].t, m[a].t));
    let (t0, t1) = (set.skeleton.find("tail_0").unwrap(), set.skeleton.find("tail_1").unwrap());
    assert!((len(t0, t1) - 0.3162).abs() < 1e-3);
    // Deterministic: the same bits every time.
    let again = set_with(&text).unwrap();
    assert_eq!(measure::pose_hash(&set, tail, 0.01).unwrap(), measure::pose_hash(&again, again.id("walk_tail").unwrap(), 0.01).unwrap());
    // Errors: the source must come first; the bones must form a chain.
    let e = set_with(r#"{ "name": "t", "kind": "chain", "source": "walk", "bones": ["tail_0"] }"#).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::UnknownName, "clips[0].source"));
    let e = set_with(&format!(r#"{WALK}, {{ "name": "t", "kind": "chain", "source": "walk", "bones": ["tail_0", "tail_2"] }}"#)).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::InvalidRig);
    let e = gen::generate(&Json::parse(r#"{ "name": "t", "kind": "chain", "source": "walk", "bones": ["tail_0"] }"#).unwrap(), "", &skel()).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::MissingField);
}

#[test]
fn a_gait_from_the_rig_alone() {
    let set = set_with(r#"{ "name": "walk", "kind": "gait", "body": "auto", "duration": 1.1, "stride": 0.8 }"#).unwrap();
    let id = set.id("walk").unwrap();
    let c = set.clip(id);
    // Both legs found (footsteps), arms swing, feet planted.
    let steps: Vec<&str> = c.events.iter().filter(|e| e.name == "footstep").map(|e| e.data.as_str()).collect();
    assert_eq!(steps, ["foot_l", "foot_r"]);
    assert!(c.tracks.iter().any(|t| t.bone == "upperarm_l") && c.tracks.iter().any(|t| t.bone == "spine"));
    let feet = [set.skeleton.find("foot_l").unwrap(), set.skeleton.find("foot_r").unwrap()];
    let m = measure::foot_contact(&set, id, &feet, 0.001, 480);
    assert!(m.max_slide < 0.06 && m.contact > 0.4, "{m:?}");
    // Explicit parts win over auto ones; "body" takes only "auto".
    let set = set_with(r#"{ "name": "w", "kind": "gait", "body": "auto", "duration": 1, "stride": 0.5, "arms": [] }"#).unwrap();
    assert!(!set.clip(set.id("w").unwrap()).tracks.iter().any(|t| t.bone == "upperarm_l"));
    assert_eq!(set_with(r#"{ "name": "w", "kind": "gait", "body": "yes", "duration": 1, "stride": 0.5 }"#).unwrap_err().code, AnimErrorCode::WrongType);
}
