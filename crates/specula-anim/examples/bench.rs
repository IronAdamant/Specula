//! F3 budget check: one simulation tick of 300 animated creatures on one core: advance (events,
//! root motion), sample + blend (a third of them mid-crossfade, a third with an additive
//! layer), local → model transforms and skinning matrices.
//!
//! ```text
//! cargo run -p specula-anim --example bench --release
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
                0 => a.crossfade(clips[0], 10.0),
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
}
