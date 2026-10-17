//! Evaluating a graph play every frame (blend tree, crossfades, an additive layer, feet IK on
//! the ground, look-at) allocates nothing once warmed up (F3: hundreds of creatures per frame).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use specula_anim::graph::Value;
use specula_anim::ik::Flat;
use specula_anim::math::Affine;
use specula_anim::play::{GraphPlay, GraphPlayer, Graphed, Surroundings, Timing};
use specula_anim::{AnimSet, Skeleton};
use specula_model::{Bone, Rig};

struct Count;
static N: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator unchanged; only counts calls.
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        N.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same layout contract as the caller's.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: `p` came from `System.alloc` with layout `l`.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Count = Count;

#[test]
fn graph_frames_do_not_allocate() {
    let b = |name: &str, parent: Option<usize>, head: [f32; 3], tail: [f32; 3]| Bone { name: name.into(), parent, head, tail };
    let rig = Rig {
        bones: vec![
            b("root", None, [0.0; 3], [0.0; 3]),
            b("hips", Some(0), [0.0, 1.0, 0.0], [0.0, 1.3, 0.0]),
            b("thigh_l", Some(1), [0.15, 1.0, 0.0], [0.15, 0.55, 0.05]),
            b("shin_l", Some(2), [0.15, 0.55, 0.05], [0.15, 0.1, 0.0]),
            b("foot_l", Some(3), [0.15, 0.1, 0.0], [0.15, 0.05, 0.2]),
            b("thigh_r", Some(1), [-0.15, 1.0, 0.0], [-0.15, 0.55, 0.05]),
            b("shin_r", Some(5), [-0.15, 0.55, 0.05], [-0.15, 0.1, 0.0]),
            b("foot_r", Some(6), [-0.15, 0.1, 0.0], [-0.15, 0.05, 0.2]),
            b("head", Some(1), [0.0, 1.5, 0.0], [0.0, 1.7, 0.1]),
        ],
    };
    let legs = r#""pelvis": "hips", "legs": [ { "thigh": "thigh_l", "shin": "shin_l", "foot": "foot_l" }, { "thigh": "thigh_r", "shin": "shin_r", "foot": "foot_r", "phase": 0.5 } ]"#;
    let text = format!(
        r#"{{ "anims": 1, "masks": {{ "top": ["head"] }}, "clips": [
        {{ "name": "idle", "kind": "idle", "duration": 1.5, {legs} }},
        {{ "name": "walk", "kind": "gait", "duration": 1.0, "stride": 0.6, {legs} }},
        {{ "name": "run", "kind": "gait", "duration": 0.6, "stride": 1.0, "duty": 0.45, {legs} }},
        {{ "name": "nod", "kind": "clip", "duration": 0.5, "loop": "loop", "tracks": [
           {{ "bone": "head", "channel": "rotation", "times": [0, 0.25, 0.5], "values": [[0, 0, 0], [10, 0, 0], [0, 0, 0]] }} ] }} ],
      "graphs": {{ "g": {{ "params": {{ "speed": {{ "type": "float", "min": 0, "max": 5 }}, "look": "vec3" }},
        "states": {{ "idle": "idle", "move": {{ "play": {{ "blend1d": "speed", "points": [[0.6, "walk"], [1.6, "run"]] }}, "match_speed": "speed" }} }},
        "transitions": [ {{ "from": "idle", "to": "move", "when": "speed > 0.1", "blend": 0.3 }},
                         {{ "from": "move", "to": "idle", "when": "speed <= 0.1", "blend": 0.3 }} ],
        "layers": [ {{ "name": "nodding", "mode": "additive", "mask": "top", "states": {{ "on": "nod" }} }} ],
        "ik": [ {{ "kind": "feet", "pelvis": "hips", "legs": [["thigh_l", "shin_l", "foot_l"], ["thigh_r", "shin_r", "foot_r"]] }},
                {{ "kind": "look_at", "bones": ["head"], "target": "look" }} ] }} }} }}"#
    );
    let set = AnimSet::load(&text, Skeleton::from_rig(&rig).unwrap()).unwrap();
    let g = Graphed { set: &set, graph: set.graph("g").unwrap(), scale: 1.0 };
    let mut play = GraphPlay::new("g");
    play.set_param("speed", Value::Num(1.1));
    play.set_param("look", Value::Vec3([1.0, 1.5, 3.0]));
    let ground = Flat(0.05);
    let around = Surroundings { to_world: Some(Affine::translate([0.0, 0.02, 0.0])), ground: Some(&ground), external: None };
    let timing = Timing { anchor: 0.0, speed: 1.0 };
    let mut p = GraphPlayer::new();
    for k in 0..100 {
        p.evaluate(g, &play, timing, k as f64 / 60.0, &around);
    }
    let before = N.load(Ordering::Relaxed);
    let mut moved = 0.0f32;
    for k in 100..700 {
        // Mid-blend frames (0.3 s fades) are in here too: the play enters `move` at 0 s.
        let pose = p.evaluate(g, &play, timing, k as f64 / 60.0 - 1.6, &around);
        moved += pose.local[3].r[0].abs();
    }
    let allocs = N.load(Ordering::Relaxed) - before;
    assert!(moved > 0.0);
    assert_eq!(allocs, 0, "{allocs} allocations in 600 frames");
}
