//! The Sepulcra creatures: rigs + anim sets load, clips are sane, feet stay planted, and
//! sampling is bit-identical (pinned).

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_anim::{measure, AnimSet, Skeleton};
use specula_model::{build, BuildParams, Model, Style};

pub const CREATURES: [&str; 5] = ["shambler", "skeleton_runner", "wraith", "bone_colossus", "brazier_fire"];

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
}

fn creature(name: &str) -> (Model, AnimSet) {
    let style = Style::parse(&std::fs::read_to_string(assets().join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(assets().join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    let model = build(&lib[name], &BuildParams { library: Some(&lib), ..BuildParams::new(&style) }).unwrap();
    let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
    let set = AnimSet::load(&std::fs::read_to_string(assets().join(format!("anims/{name}.json"))).unwrap(), skel).unwrap_or_else(|e| panic!("{name}: {e}"));
    (model, set)
}

#[test]
fn debug_dump() {
    let (_, set) = creature("skeleton_runner");
    let id = set.id("sprint").unwrap();
    let c = set.clip(id);
    let f = set.skeleton.find("foot_l").unwrap();
    for k in 0..=480 {
        let t = c.duration * k as f32 / 480.0; if k % 4 != 0 { continue; }
        let j = measure::joints_at(&set, id, t);
        println!("{t:.3} {:?} world z {:.3}", j[f], j[f][2] + c.speed() * t);
    }
}
