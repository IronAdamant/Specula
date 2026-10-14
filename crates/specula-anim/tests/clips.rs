//! Clips, sampling, blending, events, the animator, generators and skinning on a small
//! synthetic rig (no assets needed).

use specula_anim::blend::{self, Base};
use specula_anim::math::{qaxis, qdot, qrot, Transform};
use specula_anim::{gen, measure, skin_mesh, AnimErrorCode, AnimSet, Animator, Clip, Interp, LayerMode, LoopMode, RootMotion, Skeleton};
use specula_core::Json;
use specula_model::{Bone, MeshData, Part, Rig};

/// root → hips → (thigh → shin → foot) ×2 → spine → head: a 1.8 m biped facing +Z.
fn rig() -> Rig {
    let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
    Rig {
        bones: vec![
            b("root", None, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.3, 0.0]),
            b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
            b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.1, 0.0]),
            b("foot_l", Some(3), [0.15, 0.1, 0.0], [0.15, 0.05, 0.2]),
            b("thigh_r", Some(1), [-0.15, 1.0, 0.0], [-0.15, 0.55, 0.05]),
            b("shin_r", Some(5), [-0.15, 0.55, 0.05], [-0.15, 0.1, 0.0]),
            b("foot_r", Some(6), [-0.15, 0.1, 0.0], [-0.15, 0.05, 0.2]),
            b("spine", Some(1), [0.0, 1.3, 0.0], [0.0, 1.6, 0.0]),
            b("head", Some(8), [0.0, 1.6, 0.0], [0.0, 1.8, 0.0]),
        ],
    }
}

fn skel() -> Skeleton {
    Skeleton::from_rig(&rig()).unwrap()
}

const WALK: &str = r#"{ "name": "walk", "kind": "gait", "duration": 1.0, "stride": 1.2,
  "pelvis": "hips", "spine": ["spine"], "head": "head",
  "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" },
            { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r", "phase": 0.5 } ] }"#;

fn walk_set() -> AnimSet {
    let s = skel();
    let clip = gen::generate(&Json::parse(WALK).unwrap(), "", &s).unwrap();
    let mut set = AnimSet::new(s);
    set.add(clip).unwrap();
    set
}

fn clip(text: &str) -> Clip {
    Clip::parse(text).unwrap_or_else(|e| panic!("{e}"))
}

const KEYED: &str = r#"{ "clip": 1, "name": "k", "duration": 2, "loop": "loop",
  "events": [ { "name": "a", "time": 0 }, { "name": "b", "time": 0.5, "data": "x" }, { "name": "c", "time": 1.999 } ],
  "tracks": [
    { "bone": "hips", "channel": "translation", "interp": "linear", "times": [0, 1, 2], "values": [[0, 0, 0], [0, 0.2, 1], [0, 0, 2]] },
    { "bone": "spine", "channel": "rotation", "interp": "cubic", "times": [0, 0.5, 1.5, 2], "values": [[0, 0, 0], [30, 0, 0], [-20, 10, 0], [0, 0, 0]] },
    { "bone": "head", "channel": "scale", "interp": "step", "times": [0, 1], "values": [1, [1, 2, 1]] } ],
  "curves": [ { "name": "glow", "times": [0, 2], "values": [0, 1] } ] }"#;

#[test]
fn sampling_is_exact_at_keys_and_clamped() {
    let c = clip(KEYED);
    for t in &c.tracks {
        let rot = t.channel == specula_anim::Channel::Rotation;
        for (time, v) in t.keys.times.iter().zip(&t.keys.values) {
            assert_eq!(t.keys.sample(*time, rot, c.wrap()), *v, "{} at {time}", t.bone);
        }
        assert_eq!(t.keys.sample(-1.0, rot, None), t.keys.values[0]);
        assert_eq!(t.keys.sample(9.0, rot, None), *t.keys.values.last().unwrap());
    }
    // Linear midpoint, step hold.
    assert_eq!(c.tracks[0].keys.sample(0.5, false, None)[..3], [0.0, 0.1, 0.5]);
    assert_eq!(c.tracks[2].keys.sample(0.999, false, None)[1], 1.0);
    assert_eq!(c.tracks[2].keys.sample(1.5, false, None)[1], 2.0);
    // Cubic rotations stay unit length everywhere.
    for k in 0..=200 {
        let q = c.tracks[1].keys.sample(k as f32 * 0.01, true, c.wrap());
        assert!((qdot(q, q) - 1.0).abs() < 1e-5, "{q:?}");
    }
}

#[test]
fn rotations_take_the_shortest_path() {
    // 170° → −170° about Y is a 20° turn through 180°, not 340° through 0°.
    let c = clip(r#"{ "name": "s", "duration": 1, "tracks": [ { "bone": "head", "channel": "rotation",
      "times": [0, 1], "values": [[0, 170, 0], [0, -170, 0]] } ] }"#);
    for interp in [Interp::Linear, Interp::Cubic] {
        let mut k = c.tracks[0].keys.clone();
        k.interp = interp;
        let mid = k.sample(0.5, true, None);
        let v = qrot(mid, [0.0, 0.0, 1.0]);
        assert!(v[2] < -0.99, "{interp:?}: {v:?} should face −Z");
    }
}

#[test]
fn loop_modes_map_time() {
    let mut c = clip(KEYED);
    assert_eq!(c.local_time(5.0), 1.0);
    assert_eq!(c.local_time(4.0), 0.0);
    c.looping = LoopMode::PingPong;
    assert_eq!(c.local_time(2.5), 1.5);
    assert_eq!(c.local_time(4.5), 0.5);
    c.looping = LoopMode::Once;
    assert_eq!(c.local_time(7.0), 2.0);
}

#[test]
fn events_fire_once_per_loop_across_any_tick_split() {
    let c = clip(KEYED);
    for steps in [&[1.0f64 / 60.0][..], &[0.37, 0.011, 1.3], &[2.5], &[0.5], &[0.001, 0.999]] {
        let mut counts = [0u32; 3];
        let (mut u, mut k) = (0.0f64, 0usize);
        while u < 20.0 - 1e-9 {
            let dt = steps[k % steps.len()].min(20.0 - u);
            let mut out = Vec::new();
            c.events_between(u, u + dt, &mut out);
            for (e, _) in out {
                counts[e as usize] += 1;
            }
            u += dt;
            k += 1;
        }
        assert_eq!(counts, [10, 10, 10], "steps {steps:?}");
    }
    // Once: the event at the very end fires in the step that reaches the end, then never again.
    let once = clip(r#"{ "name": "o", "duration": 1, "events": [ { "name": "end", "time": 1 } ] }"#);
    let mut out = Vec::new();
    once.events_between(0.0, 0.6, &mut out);
    assert!(out.is_empty());
    once.events_between(0.6, 1.0, &mut out);
    assert_eq!(out.len(), 1);
    once.events_between(1.0, 1.2, &mut out);
    once.events_between(1.2, 3.0, &mut out);
    assert_eq!(out.len(), 1);
}

#[test]
fn json_is_strict_with_paths_and_codes() {
    let cases: &[(&str, &str, AnimErrorCode, &str)] = &[
        ("\"loop\": \"loop\"", "\"loop\": \"forever\"", AnimErrorCode::UnknownName, "loop"),
        ("\"duration\": 2", "\"duration\": 2, \"speed\": 1", AnimErrorCode::UnknownField, "speed"),
        ("\"duration\": 2", "\"duration\": -1", AnimErrorCode::OutOfRange, "duration"),
        ("\"times\": [0, 1, 2]", "\"times\": [0, 2, 1]", AnimErrorCode::NotIncreasing, "tracks[0].times[2]"),
        ("\"times\": [0, 1, 2]", "\"times\": [0, 1, 3]", AnimErrorCode::OutOfRange, "tracks[0].times[2]"),
        ("\"times\": [0, 1, 2]", "\"times\": [0, 1]", AnimErrorCode::WrongType, "tracks[0].values"),
        ("\"channel\": \"scale\"", "\"channel\": \"size\"", AnimErrorCode::UnknownName, "tracks[2].channel"),
        ("\"interp\": \"step\"", "\"interp\": \"smooth\"", AnimErrorCode::UnknownName, "tracks[2].interp"),
        ("[[0, 0, 0], [30, 0, 0]", "[[0, 0, 0, 0], [30, 0, 0]", AnimErrorCode::OutOfRange, "tracks[1].values[0]"),
        ("[[0, 0, 0], [30, 0, 0]", "[[0, 0], [30, 0, 0]", AnimErrorCode::WrongType, "tracks[1].values[0]"),
        ("\"values\": [1, [1, 2, 1]]", "\"values\": [-1, [1, 2, 1]]", AnimErrorCode::OutOfRange, "tracks[2].values[0]"),
        ("\"bone\": \"spine\", \"channel\": \"rotation\"", "\"bone\": \"hips\", \"channel\": \"translation\"", AnimErrorCode::Duplicate, "tracks[1]"),
        ("{ \"name\": \"b\", \"time\": 0.5", "{ \"name\": \"b\", \"time\": 2.5", AnimErrorCode::OutOfRange, "events[1].time"),
        ("\"name\": \"glow\"", "\"name\": \"glow\", \"name\": \"x\"", AnimErrorCode::Duplicate, "curves[0].name"),
        ("\"clip\": 1", "\"clip\": 2", AnimErrorCode::OutOfRange, "clip"),
    ];
    for (from, to, code, path) in cases {
        assert!(KEYED.contains(from), "case {from}");
        let e = Clip::parse(&KEYED.replacen(from, to, 1)).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), (*code, *path), "{to}: {e}");
    }
    assert_eq!(Clip::parse("{").unwrap_err().code, AnimErrorCode::Json);
    // Unknown bones are caught when the clip joins a set.
    let mut set = AnimSet::new(skel());
    let e = set.add(clip(&KEYED.replace("\"head\"", "\"tail\""))).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::UnknownBone, "tracks[2].bone"));
}

#[test]
fn json_round_trips_exactly() {
    let c = clip(KEYED);
    let back = Clip::from_json(&Json::parse(&c.to_json().to_pretty()).unwrap(), "").unwrap();
    assert_eq!(back, c);
    let g = walk_set().clip(specula_anim::ClipId(0)).clone();
    assert_eq!(Clip::from_json(&Json::parse(&g.to_json().to_pretty()).unwrap(), "").unwrap(), g);
}

#[test]
fn blend_weights_and_masks() {
    let s = skel();
    let rest = s.rest_pose();
    let mut other = s.rest_pose();
    for t in &mut other.local {
        t.t[1] += 1.0;
        t.r = qaxis([1.0, 0.0, 0.0], 40.0);
    }
    let mut p = rest.clone();
    blend::blend(&mut p, &other, 0.0, None);
    assert_eq!(p, rest);
    blend::blend(&mut p, &other, 1.0, None);
    assert_eq!(p, other);
    let mut p = rest.clone();
    blend::blend(&mut p, &other, 0.5, None);
    assert!((p.local[3].t[1] - (rest.local[3].t[1] + 0.5)).abs() < 1e-6);
    assert!(qdot(p.local[3].r, qaxis([1.0, 0.0, 0.0], 20.0)) > 1.0 - 1e-6);
    // Mask: only the spine branch (spine, head).
    let mask = specula_anim::BoneMask::branches(&s, &["spine"]).unwrap();
    assert_eq!(mask.weights, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    let mut p = rest.clone();
    blend::blend(&mut p, &other, 1.0, Some(&mask.weights));
    assert_eq!(p.local[2], rest.local[2]);
    assert_eq!(p.local[9], other.local[9]);
    // Additive: a delta of +0.1 m and 10° applied at half weight.
    let mut delta = specula_anim::Pose { local: vec![Transform::IDENTITY; s.len()] };
    delta.local[9] = Transform { t: [0.0, 0.1, 0.0], r: qaxis([0.0, 1.0, 0.0], 10.0), s: [1.0; 3] };
    let mut p = rest.clone();
    blend::add(&mut p, &delta, 0.0, None);
    assert_eq!(p, rest);
    blend::add(&mut p, &delta, 0.5, None);
    assert!((p.local[9].t[1] - (rest.local[9].t[1] + 0.05)).abs() < 1e-6);
    assert!(qdot(p.local[9].r, qaxis([0.0, 1.0, 0.0], 5.0)) > 1.0 - 1e-6);
}

#[test]
fn animator_crossfades_layers_and_matches_speed() {
    let mut set = walk_set();
    let idle = gen::generate(
        &Json::parse(&WALK.replace("\"kind\": \"gait\"", "\"kind\": \"idle\"").replace("\"walk\"", "\"idle\"").replace(", \"stride\": 1.2", "")).unwrap(),
        "",
        &set.skeleton,
    )
    .unwrap();
    let idle = set.add(idle).unwrap();
    let flinch = set.add(clip(r#"{ "name": "flinch", "duration": 0.4, "events": [ { "name": "hit", "time": 0.1 } ],
      "tracks": [ { "bone": "spine", "channel": "rotation", "times": [0, 0.1, 0.4], "values": [[0, 0, 0], [-30, 0, 0], [0, 0, 0]] } ] }"#)).unwrap();
    let walk = set.id("walk").unwrap();
    let mut a = Animator::new(&set, walk);
    // Speed matching: the gait covers 1.2 m per 1 s cycle.
    assert!((a.match_speed(&set, 2.4) - 2.0).abs() < 1e-6);
    a.set_rate(1.0);
    a.advance(&set, 0.25);
    let walk_pose = a.evaluate(&set).clone();
    a.crossfade(&set, idle, 0.5);
    a.advance(&set, 0.0);
    assert_eq!(a.evaluate(&set), &walk_pose, "a crossfade starts from the current pose");
    a.advance(&set, 0.6);
    assert!(!a.fading());
    let mut solo = Animator::new(&set, idle);
    solo.advance(&set, 0.6);
    assert_eq!(a.evaluate(&set), solo.evaluate(&set), "after the fade, the new clip alone");
    // An additive flinch fires its event once and removes itself when done.
    a.set_layer(0, flinch, LayerMode::Additive, 1.0, None, 0.0);
    let mut hits = 0;
    for _ in 0..60 {
        a.advance(&set, 1.0 / 60.0);
        hits += a.events().iter().filter(|e| set.clip(e.clip).events[e.event as usize].name == "hit").count();
        a.evaluate(&set);
    }
    assert_eq!(hits, 1);
    assert!(a.layer(0).is_none());
    // Determinism: replaying the same calls gives the same state hash and pose.
    let run = || {
        let mut b = Animator::new(&set, walk);
        for k in 0..100 {
            b.advance_ticks(&set, 1, 60);
            if k == 30 {
                b.crossfade(&set, idle, 0.3);
            }
        }
        (b.state_hash(), b.evaluate(&set).clone())
    };
    assert_eq!(run(), run());
}

#[test]
fn root_motion_moves_the_entity_not_the_pose() {
    let s = skel();
    let mut set = AnimSet::new(s);
    let hop = set.add(clip(r#"{ "name": "hop", "duration": 1, "loop": "loop", "tracks": [ { "bone": "root", "channel": "translation",
      "times": [0, 0.5, 1], "values": [[0, 0, 0], [0, 0.3, 0.5], [0, 0, 1]] } ] }"#)).unwrap();
    let mut a = Animator::new(&set, hop);
    a.set_root_motion(RootMotion::Horizontal);
    let mut total = [0.0f32; 3];
    for _ in 0..(3 * 60 + 30) {
        a.advance(&set, 1.0 / 60.0);
        let d = a.root_motion();
        (0..3).for_each(|k| total[k] += d[k]);
    }
    assert!((total[2] - 3.5).abs() < 1e-3, "{total:?}");
    assert_eq!(total[1], 0.0);
    let p = a.evaluate(&set);
    assert_eq!(p.local[0].t[2], 0.0, "the pose keeps the root in place horizontally");
    assert!(p.local[0].t[1] > 0.29, "height stays in the pose");
}

#[test]
fn gait_generator_plants_feet_and_loops() {
    let set = walk_set();
    let id = specula_anim::ClipId(0);
    let c = set.clip(id);
    assert_eq!(c.looping, LoopMode::Loop);
    assert_eq!(c.distance, 1.2);
    assert_eq!(c.events.iter().filter(|e| e.name == "footstep").count(), 2);
    let feet = [set.skeleton.find("foot_l").unwrap(), set.skeleton.find("foot_r").unwrap()];
    let m = measure::foot_contact(&set, id, &feet, 0.001, 480);
    assert!(m.contact > 0.5, "contact {} of the cycle (stance 0.6)", m.contact);
    let slide = m.max_slide;
    assert!(slide < 0.1, "foot slide {slide} m/s");
    assert!(measure::loop_seam(&set, id).unwrap() < 1e-6);
    // Feet never go below the rest ankle height by more than a few mm.
    for k in 0..=100 {
        let j = measure::joints_at(&set, id, k as f32 * 0.01);
        assert!(j[feet[0]][1] > 0.1 - 0.005 && j[feet[1]][1] > 0.1 - 0.005);
    }
    // The same spec gives the same bits.
    assert_eq!(walk_set().clip(id), c);
}

#[test]
fn generator_specs_are_strict() {
    let s = skel();
    let cases: &[(&str, &str, AnimErrorCode, &str)] = &[
        ("\"kind\": \"gait\"", "\"kind\": \"dance\"", AnimErrorCode::UnknownName, "kind"),
        ("\"pelvis\": \"hips\"", "\"pelvis\": \"waist\"", AnimErrorCode::UnknownBone, "pelvis"),
        ("\"stride\": 1.2", "\"stride\": 1.2, \"strid\": 1", AnimErrorCode::UnknownField, "strid"),
        ("\"stride\": 1.2", "\"stride\": -1", AnimErrorCode::OutOfRange, "stride"),
        ("\"shin\": \"shin_l\"", "\"shin\": \"shin_r\"", AnimErrorCode::InvalidRig, "legs[0].shin"),
        ("\"phase\": 0.5", "\"phase\": 0.5, \"knee\": 1", AnimErrorCode::UnknownField, "legs[1].knee"),
        ("\"head\": \"head\"", "\"head\": \"head\", \"wobble\": [ { \"bone\": \"head\", \"deg\": 5, \"cycles\": 1.5 } ]", AnimErrorCode::OutOfRange, "wobble[0].cycles"),
    ];
    for (from, to, code, path) in cases {
        assert!(WALK.contains(from), "case {from}");
        let e = gen::generate(&Json::parse(&WALK.replacen(from, to, 1)).unwrap(), "clips[3]", &s).unwrap_err();
        assert_eq!((e.code, e.path.as_str()), (*code, format!("clips[3].{path}").as_str()), "{to}: {e}");
    }
}

#[test]
fn every_kind_generates_sane_clips() {
    let s = skel();
    let specs = [
        r#"{ "name": "i", "kind": "idle", "duration": 2, "pelvis": "hips", "spine": ["spine"], "head": "head", "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" } ] }"#,
        r#"{ "name": "h", "kind": "hover", "duration": 2, "pelvis": "hips", "chain": { "bones": ["thigh_l", "shin_l"] } }"#,
        r#"{ "name": "s", "kind": "swing", "duration": 1, "pelvis": "hips", "windup_pose": { "spine": { "rot": [-20, 0, 0] } }, "strike_pose": { "spine": { "rot": [30, 0, 0] }, "hips": { "move": [0, -0.1, 0.1] } },
             "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }, { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r" } ] }"#,
        r#"{ "name": "l", "kind": "leap", "duration": 1, "pelvis": "hips", "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" } ] }"#,
        r#"{ "name": "t", "kind": "collapse", "style": "topple", "duration": 2, "pelvis": "hips", "spine": ["spine"], "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" } ] }"#,
        r#"{ "name": "c", "kind": "collapse", "style": "crumble", "duration": 2, "pelvis": "hips", "spine": ["spine"] }"#,
        r#"{ "name": "x", "kind": "collapse", "style": "scatter", "duration": 2, "pelvis": "hips", "seed": 3 }"#,
        r#"{ "name": "f", "kind": "collapse", "style": "fade", "duration": 2, "pelvis": "hips" }"#,
        r#"{ "name": "w", "kind": "wobble", "duration": 1, "wobble": [ { "bone": "head", "axis": "y", "deg": 10, "cycles": 2 }, { "bone": "spine", "axis": "y", "scale": 0.2 } ] }"#,
    ];
    for spec in specs {
        let c = gen::generate(&Json::parse(spec).unwrap(), "", &s).unwrap_or_else(|e| panic!("{spec}: {e}"));
        c.validate().unwrap();
        assert!(!c.tracks.is_empty(), "{}", c.name);
        for t in &c.tracks {
            for v in &t.keys.values {
                match t.channel {
                    specula_anim::Channel::Rotation => assert!((qdot(*v, *v) - 1.0).abs() < 1e-4),
                    // Nothing flies further than a few body heights (scatter, leaps).
                    specula_anim::Channel::Translation => assert!(v[..3].iter().all(|x| x.abs() < 6.0), "{} {:?}", c.name, v),
                    specula_anim::Channel::Scale => assert!(v[..3].iter().all(|x| *x >= 0.0 && *x <= 2.0)),
                }
            }
        }
    }
    // Scatter: every bone ends at or above the floor.
    let mut set = AnimSet::new(s.clone());
    let x = set.add(gen::generate(&Json::parse(specs[6]).unwrap(), "", &s).unwrap()).unwrap();
    let end = measure::joints_at(&set, x, 2.0);
    assert!(end.iter().skip(1).all(|p| p[1] > 0.05), "{end:?}");
    // Swing has its hit event at the end of the strike.
    let sw = gen::generate(&Json::parse(specs[2]).unwrap(), "", &s).unwrap();
    assert_eq!(sw.events[0].name, "hit");
    assert!((sw.events[0].time - 0.5).abs() < 1e-6);
}

/// A two-bone stick: bind pose skinning returns the rest mesh, and rotating the upper bone
/// moves only the vertices bound to it.
#[test]
fn skinning_bind_pose_is_the_rest_mesh() {
    let s = skel();
    let positions = vec![[0.0, 1.5, 0.0], [0.0, 1.7, 0.0], [0.05, 1.7, 0.0], [0.0, 1.45, 0.0]];
    let mesh = MeshData {
        normals: vec![[0.0, 0.0, 1.0]; 4],
        colors: vec![[255; 4]; 4],
        indices: vec![0, 2, 1, 0, 3, 2],
        parts: vec![Part { material: "m".into(), first_index: 0, index_count: 6 }],
        joints: vec![[8, 0, 0, 0], [9, 0, 0, 0], [9, 8, 0, 0], [8, 1, 0, 0]],
        weights: vec![[255, 0, 0, 0], [255, 0, 0, 0], [128, 127, 0, 0], [200, 55, 0, 0]],
        positions,
        uvs: Vec::new(),
    };
    mesh.validate().unwrap();
    let (mut model, mut sk) = (Vec::new(), Vec::new());
    s.model_space(&s.rest_pose(), &mut model);
    s.skin_matrices(&model, &mut sk);
    let out = skin_mesh(&mesh, &sk);
    for (a, b) in out.positions.iter().zip(&mesh.positions) {
        assert!((0..3).all(|k| (a[k] - b[k]).abs() < 1e-6), "{a:?} vs {b:?}");
    }
    // Turn the head 90° about Y: head-bound vertices swing, spine-bound ones stay.
    let mut pose = s.rest_pose();
    pose.local[9].r = qaxis([0.0, 1.0, 0.0], 90.0);
    s.model_space(&pose, &mut model);
    s.skin_matrices(&model, &mut sk);
    let out = skin_mesh(&mesh, &sk);
    assert_eq!(out.positions[0], mesh.positions[0]);
    let p = out.positions[1];
    assert!((p[0]).abs() < 1e-6 && (p[1] - 1.7).abs() < 1e-6, "on the axis it only turns: {p:?}");
    let n = out.normals[1];
    assert!((n[0] - 1.0).abs() < 1e-5, "the normal turns with the head: {n:?}");
}

#[test]
fn skeleton_rules() {
    let mut r = rig();
    r.bones[3].parent = Some(5);
    assert_eq!(Skeleton::from_rig(&r).unwrap_err().code, AnimErrorCode::InvalidRig);
    let e = Skeleton::new(vec!["a".into(), "b".into()], vec![None, None], vec![Transform::IDENTITY; 2]).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::InvalidRig);
    let ok = Skeleton::new(vec!["a".into(), "b".into()], vec![None, Some(0)], vec![Transform::IDENTITY, Transform { t: [0.0, 1.0, 0.0], ..Transform::IDENTITY }]).unwrap();
    assert_eq!(ok.heads[1], [0.0, 1.0, 0.0]);
    // Generators refuse skeletons whose rest frames aren't model-aligned.
    let rot = Skeleton::new(vec!["a".into()], vec![None], vec![Transform { r: qaxis([0.0, 1.0, 0.0], 30.0), ..Transform::IDENTITY }]).unwrap();
    let e = gen::generate(&Json::parse(r#"{ "name": "w", "kind": "wobble", "duration": 1 }"#).unwrap(), "", &rot).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::InvalidRig);
    // Sampling into a pose with a delta base leaves untouched bones at identity.
    let set = walk_set();
    let mut p = set.skeleton.rest_pose();
    blend::sample(&set.skeleton, set.clip(specula_anim::ClipId(0)), set.track_bones(specula_anim::ClipId(0)), 0.3, Base::Delta, &mut p);
    assert_eq!(p.local[0], Transform::IDENTITY);
}
