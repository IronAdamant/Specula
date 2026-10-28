//! Every Sepulcra recipe, at every tier, built by specula-model and checked against
//! games/sepulcra/assets/style.toml's studio rules (budgets, scale, palette, naming). No GPU: this
//! is the part of the asset studio CI can run. A rule broken is a failed test with the finding.

use specula_core::Json;
use specula_model::{build, BuildParams, Style};
use specula_studio::facts::{AssetFacts, RecipeInfo};
use specula_studio::Bible;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
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

fn facts_of(style: &Style, lib: &BTreeMap<String, String>, stem: &str, params: &[(String, f64)]) -> AssetFacts {
    let text = &lib[stem];
    let vals: Vec<(&str, f64)> = params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let model = build(text, &BuildParams { values: &vals, library: Some(lib), ..BuildParams::new(style) }).unwrap_or_else(|e| panic!("{stem}: {e}"));
    AssetFacts::from_model(&model, RecipeInfo::parse(text).as_ref(), Some(stem), params, 0)
}

/// Every recipe at every value of its small integer parameters (the stones' tiers).
fn all() -> Vec<(AssetFacts, specula_studio::Report)> {
    let (style, bible, lib) = load();
    let mut out = Vec::new();
    for (stem, text) in &lib {
        // Texture graphs (`"texgraph": 1`) live beside the recipes that use them; they're not models.
        if Json::parse(text).ok().is_some_and(|j| j.get("texgraph").is_some()) {
            continue;
        }
        let info = RecipeInfo::parse(text).unwrap();
        for params in info.variants(4, 8) {
            let f = facts_of(&style, &lib, stem, &params);
            let r = bible.check(&f);
            out.push((f, r));
        }
    }
    out
}

#[test]
fn every_sepulcra_recipe_keeps_the_style_rules() {
    let all = all();
    assert!(all.len() >= 20, "{} subjects", all.len());
    let mut broken = Vec::new();
    for (f, r) in &all {
        for e in r.errors() {
            broken.push(format!("{}: {} {} (limit {}) {}: {}", f.subject, e.check, e.value, e.limit, e.rule, e.message));
        }
    }
    assert!(broken.is_empty(), "style.toml violations:\n{}", broken.join("\n"));
}

#[test]
fn the_rules_that_apply_are_actually_checked() {
    let all = all();
    let checks = |subject: &str| -> Vec<&'static str> { all.iter().find(|(f, _)| f.subject == subject).unwrap_or_else(|| panic!("no {subject}")).1.findings.iter().map(|f| f.check).collect() };
    // A tier III stone: its own budget and height rows, the stone aspect, the palette.
    let w3 = checks("warding_stone tier=3");
    for c in ["triangles", "height", "aspect", "palette_distance", "neutral_saturation", "family_forbidden", "emissive_preset", "name_case", "name_file", "category", "parts_named"] {
        assert!(w3.contains(&c), "warding_stone tier=3 lacks {c}: {w3:?}");
    }
    let (f, r) = all.iter().find(|(f, _)| f.subject == "warding_stone tier=3").unwrap();
    let tri = r.findings.iter().find(|x| x.check == "triangles").unwrap();
    assert_eq!(tri.limit, 3000.0, "the tier = 3 entry overrides the stone budget");
    assert_eq!(tri.value, f.lod_triangles[0] as f64);
    // Enemies have a minimum too, and must not use the player's cyan.
    let s = checks("shambler");
    assert!(s.contains(&"triangles_min") && s.contains(&"family_forbidden"), "{s:?}");
}

#[test]
fn a_violation_is_caught_with_its_numbers() {
    // The warding stone recoloured with grave green and doubled in size: the checks that must say so.
    let (style, bible, mut lib) = load();
    let text = lib["warding_stone"].replace("\"from\": \"carved_shadow\", \"to\": \"carved_stone\"", "\"from\": \"grave_deep\", \"to\": \"grave_sickly\"");
    assert_ne!(text, lib["warding_stone"], "the edit applies");
    lib.insert("warding_stone".into(), text);
    let f = facts_of(&style, &lib, "warding_stone", &[]);
    let r = bible.check(&f);
    let fam = r.errors().find(|e| e.check == "family_forbidden").expect("grave green on a stone is an error");
    assert!(fam.value > 0.3, "{fam:?}");
    assert!(fam.rule.contains("palette.forbid.stone"), "{}", fam.rule);
    let j = r.to_json(10, None);
    assert_eq!(j.get("passed").and_then(Json::as_bool), Some(false));
}

/// Review probe (round 2, palflip and palprobe): the palette search must be exact. Against an
/// independent brute force, over 10,000 pseudo-random colours on Sepulcra's 28-colour palette and
/// several named sets (none, a few, every colour named), it gives the same distance and family.
#[test]
fn the_palette_search_is_exact() {
    use specula_studio::colour::{delta_e, nearest_on_segment};
    let (style, bible, _) = load();
    let entries: Vec<(String, [f32; 3], Option<&str>)> = style.palette.iter().map(|(n, c)| (n.clone(), [c[0], c[1], c[2]], bible.rules.family_of(n))).collect();
    let all: Vec<String> = entries.iter().map(|e| e.0.clone()).collect();
    let sets: Vec<Vec<String>> = vec![Vec::new(), vec!["mist".into(), "grave_deep".into(), "bone".into()], vec!["rune_glow".into(), "fire_core".into(), "moss".into(), "sky".into()], all];
    // A fixed LCG: the same colours every run.
    let mut seed: u64 = 0x5eed_c0de;
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 40) as u32 & 0xff) as f32 / 255.0
    };
    for named in &sets {
        for _ in 0..2_500 {
            let c = [next(), next(), next()];
            let fast = bible.palette.nearest(c, named).unwrap();
            let mut exact = f32::MAX;
            for (i, (na, a, fa)) in entries.iter().enumerate() {
                exact = exact.min(delta_e(c, *a));
                let (t, q) = nearest_on_segment(c, [0.0; 3], *a);
                if t < 1.0 {
                    exact = exact.min(delta_e(c, q));
                }
                for (nb, bb, fb) in &entries[i + 1..] {
                    if fa != fb && !(named.contains(na) && named.contains(nb)) {
                        continue;
                    }
                    let (t, q) = nearest_on_segment(c, *a, *bb);
                    if t > 0.0 && t < 1.0 {
                        exact = exact.min(delta_e(c, q));
                    }
                }
            }
            assert!((fast.delta_e - exact).abs() < 1e-6, "{c:?} with {} named: {} vs exact {exact}", named.len(), fast.delta_e);
        }
    }
    // The reviewer's colour: exactly 0.0496 from the palette, so it passes 0.05.
    let flip = specula_studio::colour::parse_hex("#cda3a3").unwrap();
    let d = bible.palette.nearest(flip, &[]).unwrap().delta_e;
    assert!(d < 0.05, "#cda3a3 measured {d}");
}

/// Review probe (round 1): assets that plainly break the bible but slipped through, now caught.
#[test]
fn evasions_are_caught() {
    let (style, bible, mut lib) = load();
    let part = |name: &str, colour: &str, shape: &str, ops: &str, mat: &str| format!(r#"{{"name": "{name}", "material": "{mat}", "color": "{colour}", "shape": {shape}, "ops": {ops}}}"#);
    // A 6 cm tombstone lifted 0.75 m: STYLE.md's tombstone is 0.8 m tall and stands on the slab.
    let tiny = format!(r#"{{"recipe": 1, "name": "tombstone", "category": "graveyard", "seed": 1, "parts": [{}]}}"#, part("slab", "grave_stone", r#"{"op": "box", "size": [0.05, 0.06, 0.02], "bevel": 0.005}"#, r#"[{"op": "move", "by": [0, 0.75, 0]}]"#, "graveyard_stone"));
    // A wall piece not named wall_segment: 30 m high and about 15k triangles.
    let tower = format!(r#"{{"recipe": 1, "name": "wall_tower", "category": "wall", "seed": 1, "parts": [{}]}}"#, part("body", "carved_stone", r#"{"op": "sphere", "radius": 0.5, "subdivisions": 4}"#, r#"[{"op": "scale", "by": [2, 30, 2]}, {"op": "move", "by": [0, 15, 0]}, {"op": "radial", "count": 3}]"#, "carved_stone"));
    // Round 2 (evade2): a 2.4 m stone sunk 2.3 m into the slab.
    let sunk = format!(r#"{{"recipe": 1, "name": "sunk_stone", "category": "stone", "seed": 1, "parts": [{}]}}"#, part("body", "carved_stone", r#"{"op": "box", "size": [0.8, 2.4, 0.8], "bevel": 0.05}"#, r#"[{"op": "move", "by": [0, -2.3, 0]}]"#, "carved_stone"));
    lib.insert("tombstone".into(), tiny);
    lib.insert("wall_tower".into(), tower);
    lib.insert("sunk_stone".into(), sunk);
    let errors = |stem: &str| -> Vec<&'static str> {
        let f = facts_of(&style, &lib, stem, &[]);
        bible.check(&f).errors().map(|e| e.check).collect()
    };
    let t = errors("tombstone");
    assert!(t.contains(&"height") && t.contains(&"float"), "the floating 6 cm tombstone: {t:?}");
    let k = errors("sunk_stone");
    assert!(k.contains(&"sink"), "the sunk stone: {k:?}");
    let w = errors("wall_tower");
    assert!(w.contains(&"triangles") && w.contains(&"height"), "the 30 m wall: {w:?}");
    // The wraith may hover (its own band); everything else stands on the slab.
    let (_, wraith) = all().into_iter().find(|(f, _)| f.subject == "wraith").unwrap();
    let fl = wraith.findings.iter().find(|f| f.check == "float").unwrap();
    assert!(fl.passed && fl.value > 0.2 && fl.rule.contains("scale["), "{fl:?}");
}

/// `cargo test -p specula-studio --test sepulcra -- --ignored --nocapture`: every subject's facts
/// and findings, for tuning the rules.
#[test]
#[ignore]
fn dump() {
    for (f, r) in all() {
        println!("{}\n  {}", f.to_json(12), r.to_json(20, None));
    }
}

/// Review round 3 (palbench): the palette search is bounded. A palette too big is refused with a
/// structured error, a stopped check says so (and drops the palette findings), and cached
/// answers are the search's.
#[test]
fn the_palette_search_is_bounded() {
    let mut text = String::from("{\"palette\": {");
    for i in 0..=specula_studio::palette::MAX_PALETTE {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&format!("\"c{i}\": \"#{:02x}{:02x}{:02x}\"", (i * 37) % 256, (i * 91) % 256, (i * 53) % 256));
    }
    text.push_str("}}");
    let big = Style::parse(&text).unwrap();
    let e = Bible::load(&big, None, "defaults").unwrap_err();
    assert_eq!(e.code, "palette_too_big");
    assert_eq!(e.to_json().get("error").and_then(Json::as_str), Some("invalid_style_json"));

    let (style, bible, lib) = load();
    let f = facts_of(&style, &lib, "wraith", &[]);
    assert!(f.colours.len() >= specula_studio::check::STOP_EVERY, "enough colours to stop part-way ({})", f.colours.len());
    let stopped = bible.check_with(&f, &|| true);
    assert!(stopped.cut_short && !stopped.notes.is_empty());
    assert!(!stopped.findings.iter().any(|x| x.check.starts_with("palette")), "no palette findings from a stopped search");
    let full = bible.check_with(&f, &|| false);
    assert!(!full.cut_short);
    // Asked again (cached), the same report.
    assert_eq!(bible.check(&f).findings, full.findings);
}
