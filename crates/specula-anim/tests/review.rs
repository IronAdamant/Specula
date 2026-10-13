//! Regression tests from the lane review: event boundaries (once ends, ping-pong turnarounds,
//! loop ends, floods), layer retirement, crossfade interrupts, hostile and degenerate generator
//! specs, unreachable strides and pose-hash arguments.

use std::time::Instant;

use specula_anim::clip::MAX_EVENTS_PER_STEP;
use specula_anim::{gen, measure, AnimErrorCode, AnimSet, Animator, Channel, Clip, LayerMode, Skeleton};
use specula_core::Json;
use specula_model::{Bone, Rig};

fn rig() -> Rig {
    let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
    Rig {
        bones: vec![
            b("root", None, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.3, 0.0]),
            b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
            b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.1, 0.0]),
            b("foot_l", Some(3), [0.15, 0.1, 0.0], [0.15, 0.05, 0.2]),
        ],
    }
}

fn skel() -> Skeleton {
    Skeleton::from_rig(&rig()).unwrap()
}

fn clip(t: &str) -> Clip {
    Clip::parse(t).unwrap()
}

fn names(c: &Clip, out: &[(u16, u32)]) -> Vec<String> {
    out.iter().map(|(i, _)| c.events[*i as usize].name.clone()).collect()
}

#[test]
fn ping_pong_turnarounds_fire_once() {
    let c = clip(r#"{ "name": "p", "duration": 1, "loop": "ping_pong",
        "events": [ { "name": "start", "time": 0 }, { "name": "mid", "time": 0.25 }, { "name": "end", "time": 1 } ] }"#);
    let mut out = Vec::new();
    c.events_between(0.0, 4.0, &mut out);
    // Forward 0..1: start, mid, end; backward 1..2: mid, start; forward 2..3: mid, end; back: mid.
    assert_eq!(names(&c, &out), ["start", "mid", "end", "mid", "start", "mid", "end", "mid"]);
    let mut split = Vec::new();
    for k in 0..40 {
        c.events_between(k as f64 / 10.0, (k + 1) as f64 / 10.0, &mut split);
    }
    assert_eq!(split, out, "the same occurrences however the time is split");
}

#[test]
fn loop_end_events_fire_at_the_end_of_each_cycle() {
    let c = clip(r#"{ "name": "l", "duration": 1, "loop": "loop", "events": [ { "name": "end", "time": 1 } ] }"#);
    let mut out = Vec::new();
    c.events_between(0.0, 0.99, &mut out);
    assert!(out.is_empty(), "not at the start of the first cycle");
    c.events_between(0.99, 1.01, &mut out);
    assert_eq!(out, [(0, 0)]);
    out.clear();
    c.events_between(0.0, 3.5, &mut out);
    assert_eq!(out.len(), 3);
}

#[test]
fn once_end_events_arrive_with_finished_on_layers_and_base() {
    let mut set = AnimSet::new(skel());
    let base = set.add(clip(r#"{ "name": "idle", "duration": 1, "loop": "loop" }"#)).unwrap();
    let die = set.add(clip(r#"{ "name": "die", "duration": 0.5, "events": [ { "name": "vanish", "time": 0.5 } ] }"#)).unwrap();
    // Layer, crossing the end mid-step.
    let mut a = Animator::new(&set, base);
    a.set_layer(0, die, LayerMode::Override, 1.0, None, 0.0);
    let mut n = 0;
    for _ in 0..60 {
        a.advance_ticks(&set, 7, 60);
        n += a.events().len();
    }
    assert_eq!(n, 1, "layer end event");
    assert!(a.layer(0).is_none());
    // Layer and base landing exactly on the end.
    let mut a = Animator::new(&set, base);
    a.set_layer(0, die, LayerMode::Override, 1.0, None, 0.0);
    a.advance_ticks(&set, 30, 60);
    assert_eq!(a.events().len(), 1);
    assert!(a.layer(0).is_none());
    a.advance_ticks(&set, 30, 60);
    assert!(a.events().is_empty());
    let mut a = Animator::new(&set, die);
    let mut total = 0;
    for t in 0..60 {
        a.advance_ticks(&set, 3, 60);
        total += a.events().len();
        if a.finished(&set) {
            assert_eq!(total, 1, "finished at tick {t} only after the end event");
        }
    }
    assert_eq!(total, 1);
    let mut a = Animator::new(&set, die);
    a.advance_ticks(&set, 30, 60);
    assert_eq!((a.events().len(), a.finished(&set)), (1, true));
}

#[test]
fn event_floods_are_capped_and_flagged() {
    let evs: Vec<String> = (0..1000).map(|i| format!(r#"{{ "name": "e", "time": {} }}"#, i as f64 * 9e-7)).collect();
    let c = clip(&format!(r#"{{ "name": "f", "duration": 0.001, "loop": "loop", "events": [{}] }}"#, evs.join(",")));
    let mut out = Vec::new();
    let t = Instant::now();
    assert!(c.events_between(0.0, 1.0 / 60.0 * 8.0, &mut out), "truncated");
    assert_eq!(out.len(), MAX_EVENTS_PER_STEP);
    assert!(t.elapsed().as_secs_f64() < 0.1);
    let mut set = AnimSet::new(skel());
    let id = set.add(c).unwrap();
    let mut a = Animator::new(&set, id);
    a.advance(&set, 0.1);
    assert!(a.events_truncated());
    a.set_rate(0.0);
    a.advance(&set, 0.1);
    assert!(!a.events_truncated());
}

#[test]
fn crossfade_interrupting_a_crossfade_does_not_pop() {
    let mut set = AnimSet::new(skel());
    let mk = |n: &str, x: f32| clip(&format!(r#"{{ "name": "{n}", "duration": 1, "loop": "loop", "tracks": [ {{ "bone": "hips", "channel": "translation", "times": [0], "values": [[{x}, 0, 0]] }} ] }}"#));
    let a_ = set.add(mk("a", 0.0)).unwrap();
    let b_ = set.add(mk("b", 1.0)).unwrap();
    let c_ = set.add(mk("c", 2.0)).unwrap();
    let mut an = Animator::new(&set, a_);
    an.crossfade(&set, b_, 1.0);
    an.advance(&set, 0.2);
    let x0 = an.evaluate(&set).local[1].t[0];
    an.crossfade(&set, c_, 1.0);
    let x1 = an.evaluate(&set).local[1].t[0];
    assert!((x1 - x0).abs() < 1e-6, "pop from {x0} to {x1}");
    // It then moves smoothly towards c and arrives.
    let mut last = x1;
    for _ in 0..70 {
        an.advance(&set, 1.0 / 60.0);
        let x = an.evaluate(&set).local[1].t[0];
        assert!(x >= last - 1e-6 && x - last < 0.05, "{last} -> {x}");
        last = x;
    }
    assert!((last - 2.0).abs() < 1e-5 && !an.fading());
}

#[test]
fn hostile_generator_specs_fail_fast() {
    let s = skel();
    let leg = r#"{ "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }"#;
    let spec = |legs: usize| format!(r#"{{ "name": "w", "kind": "gait", "duration": 60, "rate": 120, "stride": 1, "pelvis": "hips", "footstep": false, "legs": [{}] }}"#, vec![leg; legs].join(","));
    let t = Instant::now();
    let e = gen::generate(&Json::parse(&spec(1000)).unwrap(), "", &s).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::Limit, "legs"));
    assert!(t.elapsed().as_secs_f64() < 0.5);
    // The same bone in two legs.
    let e = gen::generate(&Json::parse(&spec(2)).unwrap(), "", &s).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::Duplicate, "legs[1]"));
    // Masks: capped before any work.
    let masks: Vec<String> = (0..40_000).map(|i| format!(r#""m{i}": ["root"]"#)).collect();
    let text = format!(r#"{{ "anims": 1, "masks": {{ {} }}, "clips": [] }}"#, masks.join(","));
    let t = Instant::now();
    let e = AnimSet::load(&text, s.clone()).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::Limit, "masks"));
    assert!(t.elapsed().as_secs_f64() < 1.0, "{:.2} s", t.elapsed().as_secs_f64());
    // Wobbles and arm lists too.
    let wob = vec![r#"{ "bone": "hips", "deg": 1 }"#; 65].join(",");
    let e = gen::generate(&Json::parse(&format!(r#"{{ "name": "w", "kind": "wobble", "duration": 1, "wobble": [{wob}] }}"#)).unwrap(), "", &s).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::Limit);
}

#[test]
fn degenerate_specs_error_or_stay_finite() {
    let s = skel();
    let legs = r#""pelvis": "hips", "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" } ]"#;
    let cases = [
        (format!(r#"{{ "name": "c", "kind": "collapse", "style": "crumble", "shrink": 0.02, "duration": 1, {legs} }}"#), None),
        (format!(r#"{{ "name": "c", "kind": "collapse", "style": "fade", "shrink": 0.02, "duration": 1, {legs} }}"#), None),
        (format!(r#"{{ "name": "c", "kind": "gait", "stride": 50, "duration": 0.05, {legs} }}"#), Some("stride")),
        (format!(r#"{{ "name": "c", "kind": "gait", "stride": 1, "duration": 1, "crouch": 10, {legs} }}"#), Some("crouch")),
        (format!(r#"{{ "name": "c", "kind": "collapse", "style": "scatter", "spread": 50, "seed": 1e9, "duration": 20, "rate": 60, {legs} }}"#), None),
    ];
    for (spec, err_path) in cases {
        match (gen::generate(&Json::parse(&spec).unwrap(), "", &s), err_path) {
            (Ok(c), None) => assert!(c.tracks.iter().all(|t| t.keys.values.iter().all(|v| v.iter().all(|x| x.is_finite()))), "{spec}"),
            (Err(e), Some(p)) => assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::OutOfRange, p), "{spec}: {e}"),
            (r, want) => panic!("{spec}: got {:?}, wanted error at {want:?}", r.map(|c| c.name)),
        }
    }
    // A leap that can't be reached reports where, without crouching to absurd depths.
    let leap = format!(r#"{{ "name": "c", "kind": "leap", "distance": 50, "height": 20, "tuck": 5, "split": 5, "duration": 0.05, {legs} }}"#);
    if let Ok(c) = gen::generate(&Json::parse(&leap).unwrap(), "", &s) {
        assert!(c.tracks.iter().all(|t| t.keys.values.iter().all(|v| v.iter().all(|x| x.is_finite()))));
    }
}

#[test]
fn unreachable_strides_are_rejected_with_numbers() {
    let s = skel();
    for stride in [1.5f32, 2.5, 4.0] {
        let spec = format!(r#"{{ "name": "c", "kind": "gait", "stride": {stride}, "duration": 1, "pelvis": "hips", "legs": [ {{ "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }} ] }}"#);
        match gen::generate(&Json::parse(&spec).unwrap(), "", &s) {
            Ok(c) => {
                let t = c.tracks.iter().find(|t| t.bone == "hips" && t.channel == Channel::Translation).unwrap();
                let miny = t.keys.values.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
                assert!(miny > -0.6, "stride {stride}: hips lowered {miny} m (hip height 0.9 m)");
            }
            Err(e) => {
                assert_eq!((e.code, e.path.as_str()), (AnimErrorCode::OutOfRange, "stride"), "{e}");
                assert!(e.measured.unwrap() > e.limit.unwrap(), "{e:?}");
                assert!(stride > 1.5, "a 1.5 m stride is reachable");
            }
        }
    }
}

#[test]
fn pose_hash_rejects_bad_steps() {
    let mut set = AnimSet::new(skel());
    let id = set.add(clip(r#"{ "name": "l", "duration": 1, "loop": "loop" }"#)).unwrap();
    for step in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(measure::pose_hash(&set, id, step).unwrap_err().code, AnimErrorCode::OutOfRange, "step {step}");
    }
    assert_eq!(measure::pose_hash(&set, id, 1e-9).unwrap_err().code, AnimErrorCode::Limit);
    assert!(measure::pose_hash(&set, id, 0.01).is_ok());
}
