//! The Fidelity track's benchmark set (docs/fidelity/assets): every subject at every tier built by
//! specula-model and checked against the set's style.toml (budgets per tier, scale, palette,
//! naming), and the T3 recipes checked for what T3 claims: distance-field sculpts, normal-mapped
//! texture graphs, and a rigged skeleton. No GPU: the sheet itself is rendered by
//! `scripts/studio_check.py --fidelity` (scripts/fidelity_t3.py).

use specula_core::Json;
use specula_model::{build, BuildParams, Model, Style};
use specula_studio::facts::{AssetFacts, RecipeInfo};
use specula_studio::Bible;
use std::collections::BTreeMap;
use std::path::PathBuf;

const SUBJECTS: [&str; 5] = ["runestone", "barrel", "lantern", "tree", "skeleton_warrior"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/fidelity/assets")
}

fn load() -> (Style, Bible, BTreeMap<String, String>) {
    let style = Style::parse(&std::fs::read_to_string(root().join("style.json")).unwrap()).unwrap();
    let toml = std::fs::read_to_string(root().join("style.toml")).unwrap();
    let bible = Bible::load(&style, Some(&toml), "style.toml").unwrap_or_else(|e| panic!("style.toml: {e}"));
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(root().join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    (style, bible, lib)
}

fn model(style: &Style, lib: &BTreeMap<String, String>, stem: &str) -> Model {
    build(&lib[stem], &BuildParams { library: Some(lib), ..BuildParams::new(style) }).unwrap_or_else(|e| panic!("{stem}: {e}"))
}

#[test]
fn every_tier_of_every_subject_keeps_the_set_rules() {
    let (style, bible, lib) = load();
    let mut broken = Vec::new();
    let mut checked = 0;
    for stem in lib.keys() {
        if Json::parse(&lib[stem]).ok().is_some_and(|j| j.get("texgraph").is_some()) {
            continue;
        }
        let m = model(&style, &lib, stem);
        let f = AssetFacts::from_model(&m, RecipeInfo::parse(&lib[stem]).as_ref(), Some(stem), &[], 0);
        for e in bible.check(&f).errors() {
            broken.push(format!("{}: {} {} (limit {}) {}: {}", f.subject, e.check, e.value, e.limit, e.rule, e.message));
        }
        checked += 1;
    }
    for s in SUBJECTS {
        for t in ["t0", "t1", "t3"] {
            assert!(lib.contains_key(&format!("{s}_{t}")), "{s}_{t} is missing from the benchmark set");
        }
    }
    assert!(checked >= 15, "{checked} subjects");
    assert!(broken.is_empty(), "style.toml violations:\n{}", broken.join("\n"));
}

#[test]
fn t3_subjects_carry_what_t3_claims() {
    let (style, bible, lib) = load();
    for s in SUBJECTS {
        let stem = format!("{s}_t3");
        let m = model(&style, &lib, &stem);
        let sculpts = m.extras.stats.iter().filter(|(_, j)| j.get("op").and_then(Json::as_str) == Some("sdf")).count();
        let normal_mapped = m.extras.textures.iter().filter(|(_, t)| t.normal.is_some()).count();
        // Every T3 subject has normal-mapped procedural materials; the sculpted ones (all but the
        // lathe-turned barrel and the forged lantern, where an SDF wouldn't help) mesh distance fields.
        assert!(normal_mapped > 0, "{stem}: no normal-mapped texture set");
        if !matches!(s, "barrel" | "lantern") {
            assert!(sculpts > 0, "{stem}: no SDF sculpt ({:?})", m.extras.stats.iter().map(|(k, _)| k).collect::<Vec<_>>());
        }
        // The T3 budget applies (not T1's).
        let f = AssetFacts::from_model(&m, RecipeInfo::parse(&lib[&stem]).as_ref(), Some(&stem), &[], 0);
        let report = bible.check(&f);
        let tri = report.findings.iter().find(|c| c.check == "triangles").unwrap_or_else(|| panic!("{stem}: no triangle budget"));
        assert_eq!(tri.limit, 16000.0, "{stem}: the *_t3 budget");
        assert!(report.findings.iter().any(|c| c.check == "texture_kb"), "{stem}: the texture budget is checked");
        assert!(m.lods.len() >= 2, "{stem}: LODs");
    }
    let skel = model(&style, &lib, "skeleton_warrior_t3");
    let rig = skel.rig.as_ref().expect("the T3 skeleton warrior is rigged");
    assert!(rig.bones.len() >= 18, "{} bones", rig.bones.len());
    let anims = std::fs::read_to_string(root().join("anims/skeleton_warrior_t3.json")).expect("its anims file");
    let anims = Json::parse(&anims).unwrap();
    assert_eq!(anims.get("rig").and_then(Json::as_str), Some("skeleton_warrior_t3"));
    assert!(anims.get("clips").and_then(Json::as_array).is_some_and(|c| c.iter().any(|c| c.get("name").and_then(Json::as_str) == Some("idle"))));
}
