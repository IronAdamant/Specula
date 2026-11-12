//! Animation graphs (A3): parsing and its errors, state machines resolved as pure functions of
//! time, settling, blend trees, layers, triggers, events, root motion and IK inside a graph.

#![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]

use specula_anim::graph::{AnimGraph, Value};
use specula_anim::ik::Flat;
use specula_anim::math::{math3, Affine};
use specula_anim::play::{self, GraphPlay, GraphPlayer, Graphed, Scratch, Surroundings, Timing};
use specula_anim::{AnimErrorCode, AnimSet, Skeleton};
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
            b("hand_l", Some(12), [0.3, 1.0, 0.1], [0.3, 0.9, 0.15]),
        ],
    }
}

const BODY: &str = r#""pelvis": "hips", "spine": ["spine", "chest"], "head": "head",
    "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" },
              { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r", "phase": 0.5 } ]"#;

fn set_text(graph: &str) -> String {
    format!(
        r#"{{ "anims": 1, "masks": {{ "upper": ["spine"] }},
  "clips": [
    {{ "name": "idle", "kind": "idle", "duration": 2.0, {BODY} }},
    {{ "name": "walk", "kind": "gait", "duration": 1.2, "stride": 0.9, {BODY} }},
    {{ "name": "run", "kind": "gait", "duration": 0.7, "stride": 1.6, "duty": 0.4, "lift": 0.18, {BODY} }},
    {{ "name": "attack", "kind": "swing", "duration": 1.0, "pelvis": "hips",
       "legs": [ {{ "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }}, {{ "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r" }} ],
       "windup_pose": {{ "chest": {{ "rot": [-15, 0, 0] }} }}, "strike_pose": {{ "chest": {{ "rot": [20, 0, 0] }} }} }},
    {{ "name": "leap", "kind": "leap", "duration": 1.0, "distance": 2.0, "height": 0.5, {BODY} }},
    {{ "name": "flinch", "kind": "clip", "duration": 0.4, "tracks": [
       {{ "bone": "chest", "channel": "rotation", "times": [0, 0.1, 0.4], "values": [[0, 0, 0], [-20, 0, 0], [0, 0, 0]] }} ] }},
    {{ "name": "still", "kind": "clip", "duration": 1, "loop": "loop" }}
  ],
  "graphs": {{ "g": {graph} }} }}"#
    )
}

const GRAPH: &str = r#"{ "graph": 1,
  "params": { "act": { "type": "enum", "values": ["idle", "move", "attack", "jump"] },
              "speed": { "type": "float", "min": 0, "max": 10 },
              "hit": "trigger", "look_at": "vec3", "look": { "type": "float", "min": 0, "max": 1, "default": 0 } },
  "nodes": { "locomotion": { "blend1d": "speed", "points": [[0.75, "walk"], [2.3, "run"]] } },
  "states": {
    "idle": { "play": "idle", "on_enter": "settle" },
    "move": { "play": "locomotion", "match_speed": "speed" },
    "attack": { "play": "attack" },
    "jump": { "play": "leap", "root_motion": "full", "ik": { "feet": 0 } } },
  "entry": "idle",
  "transitions": [
    { "from": "*", "to": "jump", "when": "act == jump", "blend": 0.1 },
    { "from": ["idle", "attack"], "to": "move", "when": "act == move and speed > 0.05", "blend": 0.25 },
    { "from": "move", "to": "idle", "when": "act == idle", "blend": 0.3 },
    { "from": ["idle", "move"], "to": "attack", "when": "act == attack", "blend": 0.15 },
    { "from": "attack", "to": "idle", "exit": "end", "blend": 0.2 },
    { "from": "jump", "to": "idle", "exit": "end", "blend": 0.2 } ],
  "layers": [ { "name": "flinch", "mode": "additive", "mask": "upper",
                "states": { "none": "still", "hit": "flinch" },
                "transitions": [ { "from": "none", "to": "hit", "when": "hit", "blend": 0 },
                                 { "from": "hit", "to": "none", "exit": "end", "blend": 0 } ] } ],
  "ik": [ { "name": "feet", "kind": "feet", "pelvis": "hips",
            "legs": [["thigh_l", "shin_l", "foot_l"], ["thigh_r", "shin_r", "foot_r"]] },
          { "name": "look", "kind": "look_at", "bones": ["chest", "head"], "target": "look_at", "weight": "look", "limit": 60 } ],
  "limits": { "shin_l": { "x": [-5, 160] } } }"#;

fn set() -> AnimSet {
    AnimSet::load(&set_text(GRAPH), Skeleton::from_rig(&rig()).unwrap()).unwrap_or_else(|e| panic!("{e}"))
}

fn g(set: &AnimSet) -> Graphed<'_> {
    Graphed { set, graph: set.graph("g").unwrap(), scale: 1.0 }
}

fn timing(anchor: f64) -> Timing {
    Timing { anchor, speed: 1.0 }
}

fn with(play: &GraphPlay, params: &[(&str, Value)]) -> GraphPlay {
    let mut p = play.clone();
    for (n, v) in params {
        p.set_param(n, v.clone());
    }
    p
}

fn top(set: &AnimSet, play: &GraphPlay, t: Timing, at: f64) -> String {
    let mut s = Scratch::default();
    play::state_at(g(set), play, t, at, &mut s)[0].stack.last().unwrap().state.clone()
}

#[test]
fn graph_errors_say_where_and_what() {
    let s = set();
    let bad = |patch: &str, needle: &str, code: AnimErrorCode, path: &str| {
        let mut j = Json::parse(GRAPH).unwrap();
        let p = Json::parse(patch).unwrap();
        for (k, v) in p.as_object().unwrap() {
            j.set(k.clone(), v.clone());
        }
        let e = AnimGraph::from_json(&j, "", &s).expect_err(patch);
        assert_eq!(e.code, code, "{patch}: {e}");
        assert!(e.path.starts_with(path), "{patch}: path {} (want {path})", e.path);
        assert!(e.message.contains(needle), "{patch}: {e}");
    };
    bad(r#"{"entry": "fly"}"#, "fly", AnimErrorCode::UnknownName, "entry");
    bad(r#"{"transitions": [{"from": "idle", "to": "swim", "when": "hit"}]}"#, "swim", AnimErrorCode::UnknownName, "transitions[0].to");
    bad(r#"{"transitions": [{"from": "idle", "to": "move", "when": "act == swim"}]}"#, "not a value", AnimErrorCode::UnknownName, "transitions[0].when");
    bad(r#"{"transitions": [{"from": "idle", "to": "move", "when": "pace > 1"}]}"#, "no parameter 'pace'", AnimErrorCode::UnknownName, "transitions[0].when");
    bad(r#"{"transitions": [{"from": "idle", "to": "move", "when": "speed >> 1"}]}"#, "can't read", AnimErrorCode::WrongType, "transitions[0].when");
    bad(r#"{"transitions": [{"from": "*", "to": "move"}]}"#, "every state", AnimErrorCode::MissingField, "transitions[0]");
    bad(r#"{"transitions": [{"from": "idle", "to": "move", "when": "act"}]}"#, "compare it", AnimErrorCode::WrongType, "transitions[0].when");
    bad(r#"{"states": {"idle": {"play": "moonwalk"}}}"#, "moonwalk", AnimErrorCode::UnknownName, "states.idle.play");
    bad(r#"{"states": {"idle": {"play": "idle", "spd": 2}}}"#, "accepted here", AnimErrorCode::UnknownField, "states.idle.spd");
    bad(r#"{"nodes": {"locomotion": "walk", "a": "b", "b": "a"}}"#, "contains itself", AnimErrorCode::InvalidRig, "nodes");
    bad(r#"{"nodes": {"locomotion": "walk", "l": {"blend1d": "speed", "points": [[1, "walk"], [1, "run"]]}}}"#, "increasing", AnimErrorCode::NotIncreasing, "nodes.l.points[1]");
    bad(r#"{"nodes": {"locomotion": "walk", "l": {"blend1d": "act", "points": [[1, "walk"]]}}}"#, "needs a float", AnimErrorCode::WrongType, "nodes.l.blend1d");
    bad(r#"{"ik": [{"kind": "feet", "legs": [["thigh_l", "foot_l", "shin_l"]]}]}"#, "parent chain", AnimErrorCode::InvalidRig, "ik[0].legs[0]");
    bad(r#"{"ik": [{"kind": "look_at", "name": "feet", "bones": ["head"], "target": "speed"}]}"#, "vec3", AnimErrorCode::WrongType, "ik[0].target");
    bad(r#"{"limits": {"tail": {"x": [0, 10]}}}"#, "no bone 'tail'", AnimErrorCode::UnknownBone, "limits.tail");
    bad(r#"{"layers": [{"name": "x", "mask": "lower", "states": {"a": "idle"}}]}"#, "no mask 'lower'", AnimErrorCode::UnknownName, "layers[0].mask");
    bad(r#"{"params": {"act": {"type": "enum", "values": ["a", "a"]}}}"#, "twice", AnimErrorCode::Duplicate, "params.act.values[1]");
    // Structure-only checking accepts unknown clips and bones, but not broken logic.
    let mut j = Json::parse(GRAPH).unwrap();
    j.set("states", Json::parse(r#"{"idle": "anything", "move": "else"}"#).unwrap());
    j.set("transitions", Json::parse(r#"[{"from": "idle", "to": "move", "when": "speed > 1"}]"#).unwrap());
    AnimGraph::check_structure(&j).unwrap();
    j.set("transitions", Json::parse(r#"[{"from": "idle", "to": "nowhere", "when": "speed > 1"}]"#).unwrap());
    assert_eq!(AnimGraph::check_structure(&j).unwrap_err().code, AnimErrorCode::UnknownName);
}

#[test]
fn transitions_fire_on_conditions_and_exit_times() {
    let s = set();
    let t = timing(10.0);
    let play = GraphPlay::new("g");
    assert_eq!(top(&s, &play, t, 10.0), "idle", "entry state");
    // Parameters hold from the anchor: a move fires at once, blending 0.25 s.
    let moving = with(&play, &[("act", Value::Name("move".into())), ("speed", Value::Num(1.0))]);
    let mut sc = Scratch::default();
    let now = play::state_at(g(&s), &moving, t, 10.1, &mut sc);
    let stack = &now[0].stack;
    assert_eq!(stack.iter().map(|e| e.state.as_str()).collect::<Vec<_>>(), ["idle", "move"]);
    let w = stack[1].fade;
    assert!(w > 0.0 && w < 1.0, "mid-blend {w}");
    // match_speed: walk 0.9 m per 1.2 s and run 1.6 m per 0.7 s blended at 1 m/s.
    let wr = (1.0 - 0.75) / (2.3 - 0.75);
    let dist = (1.0 - wr) * 0.9 + wr * 1.6;
    assert!((stack[1].rate - 1.0 / dist).abs() < 1e-5, "rate {} vs {}", stack[1].rate, 1.0 / dist);
    assert_eq!(play::state_at(g(&s), &moving, t, 11.0, &mut sc)[0].stack.len(), 1, "the blend is over");
    // Attack plays once, then the exit at its end returns to idle (attack is 1 s).
    let attack = with(&play, &[("act", Value::Name("attack".into()))]);
    assert_eq!(top(&s, &attack, t, 10.5), "attack");
    assert_eq!(top(&s, &attack, t, 10.99), "attack");
    // At its end it goes to idle, and from there straight back to attack (act is still attack):
    // an attack loop, period 1 s, each change blending.
    let mut sc = Scratch::default();
    let at_end = play::state_at(g(&s), &attack, t, 11.01, &mut sc);
    assert_eq!(at_end[0].stack.iter().map(|e| e.state.as_str()).collect::<Vec<_>>(), ["attack", "idle", "attack"]);
    let late = play::state_at(g(&s), &attack, t, 10.0 + 1000.5, &mut sc);
    assert_eq!(late[0].stack.last().unwrap().state, "attack");
    assert!(!late[0].truncated, "a periodic chain is skipped, not walked");
    // The same answer whether resolved in one go or settled half-way.
    let (half, _) = play::settle(g(&s), &attack, t, 10.0 + 500.25, &mut sc);
    let again = play::state_at(g(&s), &half, Timing { anchor: 510.25, speed: 1.0 }, 1010.5, &mut sc);
    let (a, b) = (late[0].stack.last().unwrap(), again[0].stack.last().unwrap());
    assert_eq!(a.state, b.state);
    assert!((a.phase - b.phase).abs() < 1e-6, "{} vs {}", a.phase, b.phase);
}

#[test]
fn settling_never_pops_and_parameters_blend() {
    let s = set();
    let t = timing(0.0);
    let walking = with(&GraphPlay::new("g"), &[("act", Value::Name("move".into())), ("speed", Value::Num(1.2))]);
    let mut player = GraphPlayer::new();
    let mut sc = Scratch::default();
    for now in [0.1, 0.4, 3.7, 12.345] {
        let before = player.evaluate(g(&s), &walking, t, now, &Surroundings::default()).clone();
        let (settled, info) = play::settle(g(&s), &walking, t, now, &mut sc);
        assert!(!info.truncated);
        let after = player.evaluate(g(&s), &settled, Timing { anchor: now, speed: 1.0 }, now, &Surroundings::default()).clone();
        assert_eq!(before, after, "settling at {now} s changes nothing at {now} s");
        // Later too (within rounding of re-anchored phases).
        let later_a = player.evaluate(g(&s), &walking, t, now + 0.7, &Surroundings::default()).clone();
        let later_b = player.evaluate(g(&s), &settled, Timing { anchor: now, speed: 1.0 }, now + 0.7, &Surroundings::default()).clone();
        for (x, y) in later_a.local.iter().zip(&later_b.local) {
            assert!((0..3).all(|k| (x.t[k] - y.t[k]).abs() < 1e-4) && specula_anim::math::qdot(x.r, y.r).abs() > 1.0 - 1e-5);
        }
    }
    // Stopping mid-walk: settle, then the new act blends out over 0.3 s (no pop at the write).
    let (settled, _) = play::settle(g(&s), &walking, t, 5.0, &mut sc);
    let stopped = with(&settled, &[("act", Value::Name("idle".into()))]);
    let t5 = Timing { anchor: 5.0, speed: 1.0 };
    let a = player.evaluate(g(&s), &walking, t, 5.0, &Surroundings::default()).clone();
    let b = player.evaluate(g(&s), &stopped, t5, 5.0, &Surroundings::default()).clone();
    assert_eq!(a, b, "the first frame after the write is the frame before it");
    let states = play::state_at(g(&s), &stopped, t5, 5.15, &mut sc);
    assert_eq!(states[0].stack.iter().map(|e| e.state.as_str()).collect::<Vec<_>>(), ["move", "idle"]);
    // Speed 0 freezes everything, fades included.
    let paused = Timing { anchor: 5.0, speed: 0.0 };
    let p0 = player.evaluate(g(&s), &stopped, paused, 5.0, &Surroundings::default()).clone();
    let p1 = player.evaluate(g(&s), &stopped, paused, 50.0, &Surroundings::default()).clone();
    assert_eq!(p0, p1);
}

#[test]
fn triggers_fire_once_and_settling_consumes_them() {
    let s = set();
    let t = timing(0.0);
    let hit = with(&GraphPlay::new("g"), &[("hit", Value::Bool(true))]);
    let mut sc = Scratch::default();
    let layer = |p: &GraphPlay, tm: Timing, at: f64, sc: &mut Scratch| play::state_at(g(&s), p, tm, at, sc)[1].stack.last().unwrap().state.clone();
    assert_eq!(layer(&hit, t, 0.2, &mut sc), "hit");
    assert_eq!(layer(&hit, t, 0.5, &mut sc), "none", "the flinch is 0.4 s, then back to none");
    assert_eq!(layer(&hit, t, 30.0, &mut sc), "none", "the trigger was used up: no second flinch");
    let (settled, _) = play::settle(g(&s), &hit, t, 1.0, &mut sc);
    assert_eq!(settled.param("hit"), Some(&Value::Bool(false)), "consumed in the stored play");
    let again = with(&settled, &[("hit", Value::Bool(true))]);
    assert_eq!(layer(&again, Timing { anchor: 1.0, speed: 1.0 }, 1.1, &mut sc), "hit", "a new trigger flinches again");
    // The additive flinch moves the chest (masked to the upper body) but not the legs.
    let mut player = GraphPlayer::new();
    let calm = player.evaluate(g(&s), &GraphPlay::new("g"), t, 0.1, &Surroundings::default()).clone();
    let hurt = player.evaluate(g(&s), &hit, t, 0.1, &Surroundings::default()).clone();
    let chest = s.skeleton.find("chest").unwrap();
    let shin = s.skeleton.find("shin_l").unwrap();
    assert!(specula_anim::math::qdot(calm.local[chest].r, hurt.local[chest].r).abs() < 0.9999);
    assert_eq!(calm.local[shin], hurt.local[shin]);
}

#[test]
fn events_and_root_motion_are_deterministic() {
    let s = set();
    let t = timing(2.0);
    let walking = with(&GraphPlay::new("g"), &[("act", Value::Name("move".into())), ("speed", Value::Num(0.75))]);
    let mut sc = Scratch::default();
    let mut out = Vec::new();
    // At 0.75 m/s the blend is all walk (0.9 m per cycle): one cycle every 1.2 s, two footsteps.
    let dropped = play::events(g(&s), &walking, t, 4.0, 4.0 + 1.2 * 5.0, &mut sc, &mut out);
    assert!(!dropped);
    let steps = out.iter().filter(|e| e.name == "footstep").count();
    assert_eq!(steps, 10, "{out:?}");
    assert!(out.windows(2).all(|w| w[0].time <= w[1].time));
    assert!(out.iter().all(|e| e.state == "move" && e.time >= 4.0 && e.time < 10.0));
    let mut again = Vec::new();
    play::events(g(&s), &walking, t, 4.0, 4.0 + 1.2 * 5.0, &mut sc, &mut again);
    assert_eq!(out, again);
    // on_enter fires when idle is entered again.
    let attack = with(&GraphPlay::new("g"), &[("act", Value::Name("attack".into()))]);
    play::events(g(&s), &attack, t, 2.0, 3.5, &mut sc, &mut out);
    assert!(out.iter().any(|e| e.name == "settle" && (e.time - 3.0).abs() < 1e-6), "{out:?}");
    assert!(out.iter().any(|e| e.name == "hit" && e.state == "attack"));
    // A leap with root motion travels its 2 m, all of it extracted from the pose.
    let jump = with(&GraphPlay::new("g"), &[("act", Value::Name("jump".into()))]);
    let d = play::root_motion(g(&s), &jump, t, 2.0, 3.0, &mut sc);
    assert!((d[2] - 2.0).abs() < 0.02 && d[0].abs() < 1e-4, "{d:?}");
    assert_eq!(d, play::root_motion(g(&s), &jump, t, 2.0, 3.0, &mut sc));
    let mut player = GraphPlayer::new();
    let mid = player.evaluate(g(&s), &jump, t, 2.5, &Surroundings::default()).clone();
    assert_eq!(mid.local[0].t, s.skeleton.rest[0].t, "the root stays home in the pose");
    // Walking has no root motion.
    assert_eq!(play::root_motion(g(&s), &walking, t, 2.0, 5.0, &mut sc), [0.0; 3]);
}

#[test]
fn poses_are_pure_functions_of_time() {
    let s = set();
    let t = timing(1.0);
    let play = with(&GraphPlay::new("g"), &[("act", Value::Name("move".into())), ("speed", Value::Num(1.5)), ("hit", Value::Bool(true))]);
    let h1 = play::pose_hash(g(&s), &play, t, 1.0, 9.0, 400);
    let h2 = play::pose_hash(g(&s), &play, t, 1.0, 9.0, 400);
    assert_eq!(h1, h2);
    // Evaluating in any order gives the same pose for a time.
    let mut a = GraphPlayer::new();
    let mut b = GraphPlayer::new();
    let x = a.evaluate(g(&s), &play, t, 4.2, &Surroundings::default()).clone();
    for k in 0..20 {
        b.evaluate(g(&s), &play, t, 9.0 - k as f64 * 0.37, &Surroundings::default());
    }
    assert_eq!(&x, b.evaluate(g(&s), &play, t, 4.2, &Surroundings::default()));
}

#[test]
fn ik_and_look_at_run_inside_the_graph() {
    let s = set();
    let t = timing(0.0);
    let feet = [s.skeleton.find("foot_l").unwrap(), s.skeleton.find("foot_r").unwrap()];
    let idle = GraphPlay::new("g");
    let mut p = GraphPlayer::new();
    p.report_ik = true;
    // Ground 0.1 m above the entity's model ground: both feet rise, the pelvis with them.
    let flat = Flat(0.1);
    let free = p.evaluate(g(&s), &idle, t, 0.5, &Surroundings::default()).clone();
    let mut m0 = Vec::new();
    s.skeleton.model_space(&free, &mut m0);
    let around = Surroundings { to_world: Some(Affine::IDENTITY), ground: Some(&flat), external: None };
    p.evaluate(g(&s), &idle, t, 0.5, &around);
    for &f in &feet {
        assert!((p.model()[f].t[1] - (m0[f].t[1] + 0.1)).abs() < 2e-3, "foot {f}: {} vs {}", p.model()[f].t[1], m0[f].t[1] + 0.1);
    }
    let (name, w, res) = &p.ik.solvers[0];
    assert_eq!((name.as_str(), *w), ("feet", 1.0));
    assert!((res.unwrap().pelvis - 0.1).abs() < 1e-4);
    // The jump state turns the feet off (its ik weight 0).
    let jump = with(&idle, &[("act", Value::Name("jump".into()))]);
    p.evaluate(g(&s), &jump, t, 0.5, &around);
    assert_eq!(p.ik.solvers[0].1, 0.0);
    // Look-at: off until the weight parameter is up.
    let target = [2.0, 1.7, 2.0];
    let looking = with(&idle, &[("look_at", Value::Vec3(target)), ("look", Value::Num(1.0))]);
    p.evaluate(g(&s), &looking, t, 0.5, &around);
    let head = s.skeleton.find("head").unwrap();
    let fwd = specula_anim::math::qrot(specula_anim::math::rotation_of(&p.model()[head]), [0.0, 0.0, 1.0]);
    let want = math3::normalize(math3::sub(target, p.model()[head].t));
    assert!(math3::dot(fwd, want) > 0.99, "{fwd:?} vs {want:?}");
}

#[test]
fn bad_plays_are_reported_and_fall_back() {
    let s = set();
    let gr = s.graph("g").unwrap();
    let mut play = GraphPlay::new("g");
    play.set_param("act", Value::Name("swim".into()));
    play.set_param("pace", Value::Num(2.0));
    play.set_param("speed", Value::Bool(true));
    let problems = play::bind_problems(gr, &play);
    assert_eq!(problems.len(), 3, "{problems:?}");
    // The defaults stand in: the creature idles.
    assert_eq!(top(&s, &play, timing(0.0), 1.0), "idle");
    let err = play::params_from_json(gr, &Json::parse(r#"{"act": "swim"}"#).unwrap(), "params").unwrap_err();
    assert_eq!((err.code, err.path.as_str()), (AnimErrorCode::WrongType, "params.act"));
    // JSON round trip of a settled play.
    let mut sc = Scratch::default();
    let moving = with(&GraphPlay::new("g"), &[("act", Value::Name("move".into())), ("speed", Value::Num(1.0))]);
    let (settled, _) = play::settle(g(&s), &moving, timing(0.0), 0.1, &mut sc);
    let j = Json::obj(settled.json_fields());
    let back = GraphPlay::from_json_fields(&j).unwrap();
    assert_eq!(back, settled);
    assert!(GraphPlay::from_json_fields(&Json::parse(r#"{"graph": "g", "layers": {"base": [{"state": "idle", "oops": 1}]}}"#).unwrap()).is_err());
}

/// Review round 1, finding 1: a named node reused under another (a DAG) used to slip past the
/// depth check and blow up exponentially (depth 20: about 1 s per pose; depth 60: never).
#[test]
fn shared_node_dags_are_bounded_at_parse() {
    let dag = |k: usize| {
        let mut nodes: Vec<String> = (0..k).map(|i| format!(r#""n{i}": {{ "base": "n{}", "add": "n{}" }}"#, i + 1, i + 1)).collect();
        nodes.push(format!(r#""n{k}": {{ "clip": "walk" }}"#));
        let states: Vec<String> = (0..=k).rev().map(|i| format!(r#""s{i}": "n{i}""#)).collect();
        format!(r#"{{ "nodes": {{ {} }}, "states": {{ {} }}, "entry": "s0" }}"#, nodes.join(", "), states.join(", "))
    };
    let skel = || Skeleton::from_rig(&rig()).unwrap();
    // 2^6 = 64 clips: the most one tree may play.
    let ok = AnimSet::load(&set_text(&dag(6)), skel()).unwrap();
    assert!(ok.graph("g").is_some());
    for k in [7usize, 20, 60] {
        let e = AnimSet::load(&set_text(&dag(k)), skel()).unwrap_err();
        assert_eq!(e.code, AnimErrorCode::Limit, "depth {k}: {e}");
        assert!(e.path.starts_with("graphs.g.nodes."), "depth {k}: names the node: {}", e.path);
    }
    // A plain chain through named nodes is depth-checked through the names too.
    let mut chain: Vec<String> = (0..12).map(|i| format!(r#""c{i}": {{ "blend1d": "speed", "points": [[0, "c{}"]] }}"#, i + 1)).collect();
    chain.push(r#""c12": "walk""#.into());
    let g = format!(r#"{{ "params": {{ "speed": "float" }}, "nodes": {{ {} }}, "states": {{ "a": "c0" }} }}"#, chain.join(", "));
    assert_eq!(AnimSet::load(&set_text(&g), skel()).unwrap_err().code, AnimErrorCode::Limit);
}

/// Review round 1, finding 3: periodic chains longer than the old 8-firing history were never
/// skipped, so a 9-state cycle truncated and froze.
#[test]
fn long_periodic_cycles_resolve_at_any_time() {
    for n in [2usize, 8, 9, 12, 30] {
        let states: Vec<String> = (0..n).map(|i| format!(r#""s{i}": "still""#)).collect();
        let trs: Vec<String> = (0..n).map(|i| format!(r#"{{ "from": "s{i}", "to": "s{}", "exit": "end", "blend": 0.1 }}"#, (i + 1) % n)).collect();
        let g = format!(r#"{{ "states": {{ {} }}, "transitions": [{}] }}"#, states.join(", "), trs.join(", "));
        let s = AnimSet::load(&set_text(&g), Skeleton::from_rig(&rig()).unwrap()).unwrap();
        let gr = Graphed { set: &s, graph: s.graph("g").unwrap(), scale: 1.0 };
        let mut sc = Scratch::default();
        for t in [100.5f64, 400.5, 3600.5, 1e6 + 0.5] {
            let now = play::state_at(gr, &GraphPlay::new("g"), timing(0.0), t, &mut sc);
            // `still` is 1 s long: state floor(t) mod n.
            let want = format!("s{}", (t.floor() as usize) % n);
            assert!(!now[0].truncated, "{n} states at {t}: truncated");
            assert_eq!(now[0].stack.last().unwrap().state, want, "{n} states at {t}");
        }
    }
}

/// Review rounds 1 and 2, finding 4 / 2: zero-time transition loops (a -> b when on, b -> a when
/// on) ran to the step limit on every pose, then flipped the visible state on every settle. They
/// are refused at parse with the transitions named; graphs whose loops can't fire stay valid.
#[test]
fn instant_cycles_are_refused_at_parse() {
    let load = |trs: &str, params: &str| AnimSet::load(&set_text(&format!(r#"{{ "params": {{ {params} }}, "states": {{ "a": "idle", "b": "walk", "c": "still" }}, "transitions": [{trs}] }}"#)), Skeleton::from_rig(&rig()).unwrap());
    // Both ways on one condition (the reviewer's flip probe).
    let e = load(r#"{ "from": "a", "to": "b", "when": "on", "blend": 0 }, { "from": "b", "to": "a", "when": "on", "blend": 0 }"#, r#""on": "bool""#).unwrap_err();
    assert_eq!(e.code, AnimErrorCode::InstantCycle, "{e}");
    assert!(e.path.ends_with("transitions[0]"), "{}", e.path);
    assert!(e.message.contains("transitions[0] (a -> b)") && e.message.contains("transitions[1] (b -> a)"), "{}", e.message);
    // A three-state loop, and "*" sources.
    assert_eq!(load(r#"{ "from": "a", "to": "b", "when": "x > 1" }, { "from": "b", "to": "c", "when": "x > 2" }, { "from": "c", "to": "a", "when": "x < 5" }"#, r#""x": "float""#).unwrap_err().code, AnimErrorCode::InstantCycle);
    assert_eq!(load(r#"{ "from": "*", "to": "a", "when": "on" }, { "from": "a", "to": "b", "when": "on" }"#, r#""on": "bool""#).unwrap_err().code, AnimErrorCode::InstantCycle);
    // Conditions that exclude each other, exit times and triggers end the loop: all valid.
    for (trs, params) in [
        (r#"{ "from": "a", "to": "b", "when": "x > 1" }, { "from": "b", "to": "a", "when": "x < 1" }"#, r#""x": "float""#),
        (r#"{ "from": "a", "to": "b", "when": "x >= 1" }, { "from": "b", "to": "a", "when": "x < 1" }"#, r#""x": "float""#),
        (r#"{ "from": "a", "to": "b", "when": "act == b" }, { "from": "b", "to": "a", "when": "act == a" }"#, r#""act": { "type": "enum", "values": ["a", "b"] }"#),
        (r#"{ "from": "a", "to": "b", "when": "on" }, { "from": "b", "to": "a", "when": "on == false" }"#, r#""on": "bool""#),
        (r#"{ "from": "a", "to": "b", "when": "on" }, { "from": "b", "to": "a", "when": "on", "exit": 0.5 }"#, r#""on": "bool""#),
        (r#"{ "from": "a", "to": "b", "when": "go" }, { "from": "b", "to": "a", "when": "go" }"#, r#""go": "trigger""#),
        (r#"{ "from": "*", "to": "a", "when": "act == a" }, { "from": "*", "to": "b", "when": "act == b" }"#, r#""act": { "type": "enum", "values": ["a", "b"] }"#),
    ] {
        load(trs, params).unwrap_or_else(|e| panic!("{trs}: {e}"));
    }
}

/// A zero-time loop past the parse check's budget (made here by editing a parsed graph): the
/// resolver undoes the looping instant, so settling is idempotent (no flip), and says so.
#[test]
fn instant_loops_left_at_runtime_are_idempotent_and_reported() {
    let mut trs = vec![r#"{ "from": "a", "to": "b", "when": "on", "blend": 0 }"#.to_string(), r#"{ "from": "b", "to": "a", "when": "on == false", "blend": 0 }"#.to_string()];
    for _ in 0..200 {
        trs.push(r#"{ "from": ["a", "b"], "to": "a", "when": ["on", "x > 1", "x > 2", "x > 3"], "exit": 0.5 }"#.to_string());
    }
    let g = format!(r#"{{ "params": {{ "on": "bool", "x": "float" }}, "states": {{ "a": "idle", "b": "walk" }}, "transitions": [{}] }}"#, trs.join(", "));
    let s = AnimSet::load(&set_text(&g), Skeleton::from_rig(&rig()).unwrap()).unwrap();
    let mut graph = s.graph("g").unwrap().clone();
    graph.layers[0].transitions[1].when = graph.layers[0].transitions[0].when.clone();
    let gr = Graphed { set: &s, graph: &graph, scale: 1.0 };
    let mut play = GraphPlay::new("g");
    play.set_param("on", Value::Bool(true));
    let mut sc = Scratch::default();
    let mut anchor = 0.0;
    let mut tops = Vec::new();
    for k in 0..4 {
        let t = 1.0 + k as f64;
        let tm = Timing { anchor, speed: 1.0 };
        let now = play::state_at(gr, &play, tm, t, &mut sc);
        assert!(now[0].truncated, "reported");
        let (settled, _) = play::settle(gr, &play, tm, t, &mut sc);
        let again = play::state_at(gr, &settled, Timing { anchor: t, speed: 1.0 }, t, &mut sc);
        assert_eq!(again[0].stack.last().unwrap().state, now[0].stack.last().unwrap().state, "settling keeps the state");
        tops.push(again[0].stack.last().unwrap().state.clone());
        play = settled;
        anchor = t;
    }
    assert!(tops.iter().all(|t| *t == tops[0]), "no flip: {tops:?}");
    let tm = Timing { anchor: 0.0, speed: 8.0 };
    let t = std::time::Instant::now();
    let a = play::root_motion(gr, &play, tm, 0.0, 30.0, &mut sc);
    assert_eq!(a, play::root_motion(gr, &play, tm, 0.0, 30.0, &mut sc), "deterministic");
    assert!(t.elapsed() < specula_core::speed_budget(std::time::Duration::from_secs(2)), "{:?}", t.elapsed());
}

/// Root motion integrated along one resolve agrees with the exact displacement of a leap
/// played through a blend, and over a window holding a whole periodic chain.
#[test]
fn root_motion_follows_blends_and_long_windows() {
    let s = set();
    let t = timing(0.0);
    let mut sc = Scratch::default();
    // From idle into the leap (0.1 s blend): the 2 m arrive, eased in by the blend.
    let jump = with(&GraphPlay::new("g"), &[("act", Value::Name("jump".into()))]);
    let d = play::root_motion(g(&s), &jump, t, 0.0, 1.0, &mut sc);
    assert!((d[2] - 2.0).abs() < 0.08, "{d:?}");
    // Split windows add up (to rounding).
    let mut sum = [0.0f32; 3];
    for k in 0..10 {
        let p = play::root_motion(g(&s), &jump, t, k as f64 * 0.1, (k + 1) as f64 * 0.1, &mut sc);
        sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
    }
    assert!((sum[2] - d[2]).abs() < 1e-3, "{sum:?} vs {d:?}");
}
