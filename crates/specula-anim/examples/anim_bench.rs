//! F3 budget check: one simulation tick of 300 animated creatures on one core: advance (events,
//! root motion), sample + blend (a third of them mid-crossfade, a third with an additive
//! layer), local → model transforms and skinning matrices.
//!
//! ```text
//! cargo run -p specula-anim --example anim_bench --release
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use specula_anim::math::Affine;
use specula_anim::{AnimSet, Animator, LayerMode, Skeleton};
use specula_model::{build, BuildParams, Style};

fn main() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let style = Style::parse(&std::fs::read_to_string(assets.join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(assets.join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let sets: Vec<AnimSet> = ["shambler", "skeleton_runner", "wraith"]
        .iter()
        .map(|name| {
            let m = build(&lib[*name], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
            let skel = Skeleton::from_rig(m.rig.as_ref().unwrap()).unwrap();
            AnimSet::load(&std::fs::read_to_string(assets.join(format!("anims/{name}.json"))).unwrap(), skel).unwrap()
        })
        .collect();
    let n = 300;
    let mut crowd: Vec<(usize, Animator, Vec<Affine>, Vec<Affine>)> = (0..n)
        .map(|i| {
            let s = i % sets.len();
            let set = &sets[s];
            let clips: Vec<_> = set.clips().map(|(id, _)| id).collect();
            let mut a = Animator::new(set, clips[1 % clips.len()]);
            a.advance(set, i as f32 * 0.013);
            match i % 3 {
                0 => a.crossfade(set, clips[0], 10.0),
                1 => a.set_layer(0, clips[0], LayerMode::Additive, 0.5, set.mask_id("upper"), 0.0),
                _ => {}
            }
            a.match_speed(set, 1.0);
            (s, a, Vec::with_capacity(32), Vec::with_capacity(32))
        })
        .collect();
    let bones: usize = crowd.iter().map(|(s, ..)| sets[*s].skeleton.len()).sum();
    let dt = 1.0 / 60.0;
    let mut times = Vec::new();
    for _ in 0..400 {
        let t0 = Instant::now();
        for (s, a, model, skin) in crowd.iter_mut() {
            let set = &sets[*s];
            a.advance(set, dt);
            a.evaluate(set);
            a.skin(set, model, skin);
        }
        times.push(t0.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    let median = times[times.len() / 2];
    let p95 = times[times.len() * 95 / 100];
    println!("{n} creatures, {bones} bones ({:.1} per creature): tick median {median:.3} ms, p95 {p95:.3} ms, best {:.3} ms", bones as f64 / n as f64, times[0]);
    println!("per creature {:.2} us, per bone {:.0} ns", median * 1e3 / n as f64, median * 1e6 / bones as f64);

    // The A3 path the engine draws: each creature's `creature` graph resolved and evaluated at a
    // frame time (blend trees, crossfades, the flinch layer), feet IK on a stepped ground and
    // look-at, then skinning matrices.
    use specula_anim::graph::Value;
    use specula_anim::ik::Ground;
    use specula_anim::play::{GraphPlay, GraphPlayer, Graphed, Surroundings, Timing};
    struct Steps;
    impl Ground for Steps {
        fn height(&self, x: f32, _z: f32, top: f32, depth: f32) -> Option<f32> {
            let g = if (x * 2.0).floor() as i32 % 2 == 0 { 0.0 } else { 0.06 };
            (g <= top && g >= top - depth).then_some(g)
        }
    }
    let acts = ["move", "move", "attack", "idle"];
    let mut herd: Vec<(usize, GraphPlay, Timing, GraphPlayer, Affine)> = (0..n)
        .map(|i| {
            let mut p = GraphPlay::new("creature");
            p.set_param("act", Value::Name(acts[i % acts.len()].into()));
            p.set_param("speed", Value::Num(0.6 + (i % 5) as f32 * 0.2));
            p.set_param("hit", Value::Bool(i % 7 == 0));
            p.set_param("look_at", Value::Vec3([0.0, 1.0, 0.0]));
            p.set_param("look", Value::Num(0.5));
            let at = Affine::translate([(i % 20) as f32 * 1.3 - 13.0, 0.0, (i / 20) as f32 * 1.3 - 10.0]);
            (i % sets.len(), p, Timing { anchor: -(i as f64) * 0.17, speed: 1.0 }, GraphPlayer::new(), at)
        })
        .collect();
    let (mut model, mut skin) = (Vec::new(), Vec::new());
    let mut times = Vec::new();
    for f in 0..400 {
        let t = 2.0 + f as f64 / 60.0;
        let t0 = Instant::now();
        for (s, play, timing, player, at) in herd.iter_mut() {
            let set = &sets[*s];
            let g = Graphed { set, graph: set.graph("creature").unwrap(), scale: 1.2 };
            let around = Surroundings { to_world: Some(*at), ground: Some(&Steps), external: None };
            let pose = player.evaluate(g, play, *timing, t, &around);
            set.skeleton.model_space(pose, &mut model);
            set.skeleton.skin_matrices(&model, &mut skin);
        }
        times.push(t0.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    let (median, p95) = (times[times.len() / 2], times[times.len() * 95 / 100]);
    println!("{n} graph creatures (blend trees, flinch layer, feet IK, look-at): frame median {median:.3} ms, p95 {p95:.3} ms ({:.2} us each)", median * 1e3 / n as f64);
}
