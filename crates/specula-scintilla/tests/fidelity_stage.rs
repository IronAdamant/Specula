//! The Fidelity T3 stage dressing (docs/fidelity/assets/stage_t3.json) and its effects
//! (docs/fidelity/assets/vfx/*.json) parse as the engine parses them: every effect within the
//! photosensitivity rule, every decal and vfx component valid, every texture it names present. The
//! sheet itself is rendered by `scripts/studio_check.py --fidelity` (needs a GPU).

use specula_core::Json;
use specula_scintilla::{DecalSpec, Effect, VfxSpec};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/fidelity/assets")
}

fn read(rel: &str) -> Json {
    let text = std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    Json::parse(&text).unwrap_or_else(|e| panic!("{rel}: {e:?}"))
}

#[test]
fn the_t3_stage_dressing_is_valid() {
    let stage = read("stage_t3.json");
    let effects = stage.get("effects").and_then(Json::as_object).expect("effects");
    let mut names = vec!["motes".to_string()];
    for (name, file) in effects {
        Effect::parse(&read(file.as_str().unwrap())).unwrap_or_else(|e| panic!("effect {name}: {e:?}"));
        names.push(name.clone());
    }
    let subjects = stage.get("subjects").and_then(Json::as_object).expect("subjects");
    assert_eq!(subjects.len(), 5);
    let mut decals = 0;
    for (s, d) in subjects {
        for dec in d.get("decals").and_then(Json::as_array).into_iter().flatten() {
            let spec = DecalSpec::parse(dec.get("decal").unwrap()).unwrap_or_else(|e| panic!("{s} decal: {e:?}"));
            let tex = spec.texture.name().to_string();
            assert!(root().join(&tex).is_file(), "{s}: decal texture {tex} is missing");
            decals += 1;
        }
        for fx in d.get("effects").and_then(Json::as_array).into_iter().flatten() {
            let v = fx.get("vfx").unwrap();
            VfxSpec::parse(v).unwrap_or_else(|e| panic!("{s} vfx: {e:?}"));
            let effect = v.get("effect").and_then(Json::as_str).unwrap();
            assert!(names.iter().any(|n| n == effect), "{s}: effect {effect} is neither built in here nor in the stage's library");
        }
        let gi = d.get("gi").expect("every T3 subject bakes a GI volume");
        assert!(gi.get("spacing").and_then(Json::as_f64).is_some_and(|x| (0.25..=8.0).contains(&x)), "{s}: GI spacing");
    }
    assert!(decals >= 8, "{decals} decals");
    let leaves = read("vfx/tree_leaves.json");
    let tex = leaves.get("emitters").and_then(Json::as_array).unwrap()[0].get("texture").and_then(Json::as_str).unwrap().to_string();
    assert!(root().join(&tex).is_file(), "the leaf texture {tex} is missing");
}
