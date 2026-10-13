//! Every Sepulcra recipe builds, is valid, stays within games/sepulcra/STYLE.md's triangle
//! budgets and proportions, simplifies into LODs, and builds bit-identically (pinned hash).

use std::collections::BTreeMap;
use std::path::PathBuf;

use specula_core::hash::StableHasher;
use specula_model::{build, BuildParams, Model, Style};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
}

fn load() -> (Style, BTreeMap<String, String>) {
    let style = Style::parse(&std::fs::read_to_string(root().join("style.json")).unwrap()).unwrap();
    let mut lib = BTreeMap::new();
    for e in std::fs::read_dir(root().join("recipes")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") {
            lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
    }
    (style, lib)
}

fn model(style: &Style, lib: &BTreeMap<String, String>, name: &str, tier: Option<f64>) -> Model {
    let vals: Vec<(&str, f64)> = tier.map(|t| vec![("tier", t)]).unwrap_or_default();
    let text = lib.get(name).unwrap_or_else(|| panic!("missing recipe {name}"));
    build(text, &BuildParams { values: &vals, library: Some(lib), ..BuildParams::new(style) }).unwrap_or_else(|e| panic!("{name} tier {tier:?}: {e}"))
}

const STONES: [&str; 4] = ["warding_stone", "rune_pillar", "brazier_cairn", "sky_obelisk"];
const ENEMIES: [(&str, f32); 3] = [("shambler", 1.7), ("skeleton_runner", 1.8), ("wraith", 2.2)];
const OTHERS: [&str; 6] = ["wall_segment", "tombstone", "graveyard_fence", "brazier_fire", "bone_colossus", "slab_cliff"];

/// Every asset (stones at each tier) with its STYLE.md triangle budget.
fn all(style: &Style, lib: &BTreeMap<String, String>) -> Vec<(String, Model, usize, usize)> {
    let mut out = Vec::new();
    for s in STONES {
        for t in 1..=3 {
            let max = if t == 3 { 3000 } else { 1500 };
            out.push((format!("{s} {t}"), model(style, lib, s, Some(t as f64)), 0, max));
        }
    }
    for (e, _) in ENEMIES {
        out.push((e.to_string(), model(style, lib, e, None), 300, 800));
    }
    for (o, max) in OTHERS.iter().zip([1000, 400, 700, 700, 2500, 3000]) {
        out.push((o.to_string(), model(style, lib, o, None), 0, max));
    }
    out
}

#[test]
fn every_recipe_in_the_folder_is_covered() {
    let (_, lib) = load();
    for name in lib.keys() {
        let known = STONES.contains(&name.as_str())
            || ENEMIES.iter().any(|(e, _)| e == name)
            || OTHERS.contains(&name.as_str())
            || ["rune_glyph", "rune_column"].contains(&name.as_str());
        assert!(known, "recipe {name} has no budget test");
    }
}

#[test]
fn budgets_validity_and_lods() {
    let (style, lib) = load();
    for (name, m, min, max) in all(&style, &lib) {
        let n0 = m.lods[0].triangle_count();
        assert!(n0 >= min && n0 <= max, "{name}: {n0} triangles, budget {min}..={max}");
        assert!((2..=3).contains(&m.lods.len()), "{name}: {} LODs", m.lods.len());
        assert_eq!(m.lods.len(), m.lod_ratios.len());
        for (i, l) in m.lods.iter().enumerate() {
            l.validate().unwrap_or_else(|e| panic!("{name} LOD{i}: {e}"));
        }
        let counts: Vec<usize> = m.lods.iter().map(|l| l.triangle_count()).collect();
        // Every LOD saves at least 10% over the one before (no useless duplicates).
        assert!(counts.windows(2).all(|w| (w[1] as f32) <= 0.9 * w[0] as f32), "{name}: LODs {counts:?} must shrink");
        assert!((counts[1] as f32) <= 0.75 * n0 as f32, "{name}: LOD1 {} of {n0} is not a real reduction", counts[1]);
        println!("{name:<20} {counts:?}");
    }
}

#[test]
fn proportions_follow_the_style_bible() {
    let (style, lib) = load();
    let size = |m: &Model| m.bounds.size();
    let near = |got: f32, want: f32, tol: f32, what: &str| assert!((got - want).abs() <= tol, "{what}: {got:.3} m, want {want} ± {tol}");
    // Warding stone tier I: 1.0 x 1.0 x 2.4 m body (plinth a little wider); tier III ~3.2 m.
    let w1 = model(&style, &lib, "warding_stone", Some(1.0));
    near(size(&w1)[1], 2.4, 0.02, "warding stone I height");
    let w3 = model(&style, &lib, "warding_stone", Some(3.0));
    near(size(&w3)[1], 3.2, 0.1, "warding stone III height with ring");
    // Sky obelisk: 0.8 x 0.8 x 4 m.
    let o = model(&style, &lib, "sky_obelisk", Some(1.0));
    near(size(&o)[1], 4.0, 0.1, "obelisk height");
    // Tiers read from the silhouette: each tier is at least as tall and has more parts/triangles.
    for s in STONES {
        let ms: Vec<Model> = (1..=3).map(|t| model(&style, &lib, s, Some(t as f64))).collect();
        assert!(ms[0].lods[0].triangle_count() < ms[1].lods[0].triangle_count(), "{s}: tier II adds detail");
        assert!(ms[1].lods[0].triangle_count() < ms[2].lods[0].triangle_count(), "{s}: tier III adds detail");
        if s != "sky_obelisk" {
            // The obelisk is already 4 m: its ring circles the needle instead of floating above.
            assert!(size(&ms[2])[1] > size(&ms[1])[1] + 0.2, "{s}: the tier III ring raises the silhouette");
        }
        assert!(ms.iter().all(|m| m.materials.iter().any(|(n, _)| n == "rune_glow")), "{s}: runes glow at every tier");
    }
    // Wall: 2 m long, 0.6 m deep, 0.9 m high (knee to waist).
    let wall = size(&model(&style, &lib, "wall_segment", None));
    near(wall[0], 2.0, 0.1, "wall length");
    near(wall[2], 0.6, 0.08, "wall depth");
    near(wall[1], 0.9, 0.06, "wall height");
    // Tombstone slab 0.5 x 0.15 x 0.8 on a low footing.
    let tomb = size(&model(&style, &lib, "tombstone", None));
    near(tomb[1], 0.85, 0.05, "tombstone height");
    // Enemies: shambler / runner / wraith tops at 1.7 / 1.8 / 2.2 m; boss about 8 m.
    for (e, h) in ENEMIES {
        let m = model(&style, &lib, e, None);
        near(m.bounds.max[1], h, 0.12, &format!("{e} height"));
        assert!(m.materials.iter().any(|(n, _)| n == "grave_glow"), "{e}: grave-green eyes");
        assert!(!m.materials.iter().any(|(n, _)| n == "rune_glow"), "{e}: no player cyan on the enemy side");
    }
    let shambler = size(&model(&style, &lib, "shambler", None));
    let runner = size(&model(&style, &lib, "skeleton_runner", None));
    assert!(shambler[0] > 2.0 * runner[0], "shambler is wide, the runner thin");
    near(model(&style, &lib, "bone_colossus", None).bounds.max[1], 8.0, 0.5, "boss height");
    // Slab chunk: cliffs about 15 m, flat top.
    let slab = model(&style, &lib, "slab_cliff", None);
    near(size(&slab)[1], 15.0, 0.01, "cliff height");
    let top: Vec<_> = slab.lods[0].positions.iter().filter(|p| p[1] > 14.99).collect();
    assert!(top.len() > 10, "flat top face at 15 m");
}

#[test]
fn builds_are_bit_identical_pinned() {
    // One hash over every asset's LODs and collider. If it changes, the assets changed: check
    // that was intended (recipe edit or algorithm change), then update the pin.
    let (style, lib) = load();
    let mut h = StableHasher::new();
    for (name, m, _, _) in all(&style, &lib) {
        h.write_str(&name);
        h.write_u64(m.hash);
    }
    let again: Vec<u64> = all(&style, &lib).iter().map(|(_, m, _, _)| m.hash).collect();
    let first: Vec<u64> = all(&style, &lib).iter().map(|(_, m, _, _)| m.hash).collect();
    assert_eq!(again, first, "two builds in one process differ");
    assert_eq!(format!("{:016x}", h.finish()), PINNED, "Sepulcra asset hash changed");
}

const PINNED: &str = "76650a02e3809c66";
