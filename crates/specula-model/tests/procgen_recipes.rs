//! Procedural tiers 2-4 through recipes: booleans, SDF sculpts, buildings, terrain, scatter and
//! texture graphs, with their errors, budgets, LODs and determinism.

use std::collections::BTreeMap;

use specula_model::{build, BuildParams, ErrorCode, Model, Style};

fn style() -> Style {
    Style::parse(
        r##"{ "style": "t",
      "palette": { "dark": "#3d434d", "pale": "#aab4c0", "moss": "#4f6b3a", "grass": "#5a7a3a", "rock": "#6b7480" },
      "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 }, "wood": { "color": "#6a4a32" },
                     "glass": { "kind": "glass", "color": "#99cccc", "opacity": 0.4 }, "slate": { "color": "#3e4248" } } }"##,
    )
    .unwrap()
}

fn lib() -> BTreeMap<String, String> {
    let mut l = BTreeMap::new();
    l.insert(
        "stone_tex".to_string(),
        r#"{ "texgraph": 1, "size": 32, "nodes": { "n": { "op": "noise", "scale": 4 }, "c": { "op": "gradient_map", "in": "n", "stops": [[0, "dark"], [1, "pale"]] } },
             "outputs": { "base_color": "c", "height": "n", "roughness": 0.8 } }"#
            .to_string(),
    );
    l.insert(
        "post".to_string(),
        r#"{ "name": "post", "params": { "h": { "default": 1, "min": 0.5, "max": 2 } },
             "parts": [ { "name": "p", "material": "wood", "shape": { "op": "box", "size": [0.2, "h", 0.2] } } ] }"#
            .to_string(),
    );
    l
}

fn make(text: &str) -> Result<Model, specula_model::RecipeError> {
    let (s, l) = (style(), lib());
    build(text, &BuildParams { library: Some(&l), ..BuildParams::new(&s) })
}

const HEADSTONE: &str = r#"{ "name": "headstone", "seed": 4, "params": { "tier": { "default": 1, "min": 1, "max": 2, "integer": true } },
  "parts": [ { "name": "slab", "material": "stone", "shape": { "op": "difference", "bevel": 0.015, "of": [
      { "op": "extrude", "outline": { "arch": [0.7, 1.1], "segments": 8 }, "height": 0.16, "ops": [ { "op": "rotate", "deg": [-90, 0, 0] }, { "op": "move", "by": [0, 0, 0.08] } ] },
      { "op": "box", "size": [0.4, 0.08, 0.1], "ops": [ { "op": "move", "by": [0, 0.7, 0.09] } ] },
      { "op": "box", "size": [0.3, 0.05, 0.1], "when": "tier >= 2", "ops": [ { "op": "move", "by": [0, 0.55, 0.09] } ] } ] } } ] }"#;

#[test]
fn a_bevelled_headstone_cut_with_booleans() {
    let m = make(HEADSTONE).unwrap_or_else(|e| panic!("{e}"));
    m.lods[0].validate().unwrap();
    assert!(m.lods.len() >= 2, "LODs: {:?}", m.notes);
    let report = m.report().to_string();
    assert!(report.contains("\"difference\"") && report.contains("bevelled_runs"), "{report}");
    // Same recipe, same bits.
    assert_eq!(make(HEADSTONE).unwrap().hash, m.hash);
}

#[test]
fn sdf_sculpts_build_lods_and_report_their_grid() {
    let rock = r##"{ "name": "rock", "seed": 2, "parts": [ { "name": "r", "material": "stone",
      "shape": { "op": "sdf", "resolution": 40, "tree": { "op": "union", "smooth": 0.15, "displace": { "amount": 0.04, "scale": 0.3 }, "of": [
        { "op": "ellipsoid", "radii": [0.6, 0.4, 0.5], "color": "pale" },
        { "op": "sphere", "radius": 0.35, "at": [0.4, 0.15, 0.1], "color": "moss" },
        { "op": "box", "size": [0.4, 0.4, 0.4], "corner": 0.05, "at": [-0.4, -0.1, 0], "rotate": [0, 30, 10] } ] } } } ] }"##;
    let m = make(rock).unwrap_or_else(|e| panic!("{e}"));
    m.lods[0].validate().unwrap();
    assert!(m.lods.len() >= 2);
    assert!(m.report().to_string().contains("samples_evaluated"));
    // Too fine: refused with the path and the numbers.
    let fine = rock.replace("\"resolution\": 40", "\"resolution\": 400");
    let e = make(&fine).unwrap_err();
    assert_eq!((e.path.as_str(), e.code), ("parts[0].shape.resolution", ErrorCode::OutOfRange));
    // A typo in a node.
    let typo = rock.replace("\"radii\"", "\"radius_xyz\"");
    let e = make(&typo).unwrap_err();
    assert_eq!(e.code, ErrorCode::UnknownField);
    assert!(e.path.starts_with("parts[0].shape.tree.of[0]"), "{}", e.path);
}

const CRYPT: &str = r#"{ "name": "crypt", "seed": 9, "textures": { "stone": "stone_tex" }, "parts": [
  { "name": "body", "material": "stone", "texture": "stone", "uv_scale": 1.5,
    "generate": { "op": "building", "plan": { "rect": [4, 5] }, "floors": 1, "floor_height": 3, "wall": 0.4,
      "plinth": [0.3, 0.06], "cornice": [0.2, 0.1],
      "windows": { "width": 0.5, "height": 1, "sill": 1.2, "spacing": 2, "margin": 0.8, "arch": true, "glass": true },
      "doors": [ { "side": 0, "width": 1.1, "height": 2.1, "arch": true } ],
      "roof": { "kind": "gable", "pitch": 35, "overhang": 0.2 },
      "materials": { "roof": "slate", "door": "wood", "glass": "glass" } } } ] }"#;

#[test]
fn a_textured_building_from_rules() {
    let m = make(CRYPT).unwrap_or_else(|e| panic!("{e}"));
    let l0 = &m.lods[0];
    l0.validate().unwrap();
    let mats: Vec<&str> = l0.parts.iter().map(|p| p.material.as_str()).collect();
    // The texture dresses the part's own material (walls, trims); roof, door and glass keep theirs.
    for want in ["stone+lib:stone_tex", "slate", "wood", "glass"] {
        assert!(mats.contains(&want), "{mats:?}");
    }
    assert_eq!(l0.uvs.len(), l0.positions.len(), "textured parts carry UVs");
    assert_eq!(m.extras.textures.len(), 1);
    assert!(m.extras.textures.iter().all(|(k, t)| k == "stone+lib:stone_tex" && t.size == 32));
    assert_eq!(make(CRYPT).unwrap().hash, m.hash);
    // The texture graph's file must be in the library.
    let e = make(&CRYPT.replace("\"stone_tex\"", "\"missing_tex\"")).unwrap_err();
    assert_eq!((e.path.as_str(), e.code), ("textures.stone", ErrorCode::UnknownAsset));
    // A door on a side that doesn't exist.
    let e = make(&CRYPT.replace("\"side\": 0", "\"side\": 7")).unwrap_err();
    assert!(e.path.ends_with("generate.doors"), "{}", e.path);
}

const GRAVEYARD: &str = r#"{ "name": "graveyard", "seed": 5, "parts": [
  { "name": "ground", "material": "stone", "generate": { "op": "terrain", "size": [16, 12], "samples": [33, 25], "depth": 0.6,
      "layers": [ { "kind": "fbm", "scale": 8, "octaves": 3, "amplitude": 0.6 } ], "pads": [ { "at": [0, 0], "radius": 2, "height": 0.2, "blend": 1 } ],
      "erosion": { "droplets": 2000 }, "splat": [ { "name": "grass", "color": "grass", "slope": [0, 20] }, { "name": "rock", "color": "rock", "slope": [20, 90] } ] } },
  { "name": "posts", "material": "wood", "generate": { "op": "scatter", "on": "ground", "area": { "rect": [14, 10] }, "min_distance": 1.6,
      "count": 40, "avoid": [[0, 0, 2.5]], "slope": [0, 25], "tilt": 6, "align": 0.5, "sink": 0.05,
      "items": [ { "include": "post", "params": { "h": [0.6, 1.4] }, "variants": 3, "weight": 2 },
                 { "shape": { "op": "cylinder", "radius": 0.1, "height": 0.5, "sides": 8, "ops": [ { "op": "taper", "to": 0.6 } ] }, "color": "dark" } ] } } ] }"#;

#[test]
fn a_scattered_graveyard_on_eroded_terrain() {
    let m = make(GRAVEYARD).unwrap_or_else(|e| panic!("{e}"));
    m.lods[0].validate().unwrap();
    let t = m.extras.terrain.as_ref().expect("a terrain");
    assert_eq!((t.columns, t.rows), (33, 25));
    // The physics export at the engine's side limit, and a walkable mask.
    let (c, r, h, _) = t.physics_heightfield(129);
    assert_eq!((c, r, h.len()), (33, 25, 33 * 25));
    assert!(t.walkable_cells(30.0).iter().filter(|w| **w).count() > 100);
    let report = m.report().to_string();
    assert!(report.contains("\"placed\""), "{report}");
    assert_eq!(make(GRAVEYARD).unwrap().hash, m.hash);
    // Scatter on a terrain that doesn't exist.
    let e = make(&GRAVEYARD.replace("\"on\": \"ground\"", "\"on\": \"hill\"")).unwrap_err();
    assert_eq!((e.path.as_str(), e.code), ("parts[1].generate.on", ErrorCode::UnknownName));
    // Too many samples.
    let e = make(&GRAVEYARD.replace("[33, 25]", "[300, 25]")).unwrap_err();
    assert_eq!(e.code, ErrorCode::Limit);
}

#[test]
fn a_terrain_parts_transform_is_kept() {
    // Moves, scales and colour ops are carried (colliders and scatters follow them); an op that
    // reshapes the surface leaves no transform (they refuse).
    for (ops, want) in [
        ("[]", Some(([0.0f32, 0.0, 0.0], [1.0f32, 1.0, 1.0]))),
        (r#"[{"op": "move", "by": [0, 3, 0]}]"#, Some(([0.0, 3.0, 0.0], [1.0, 1.0, 1.0]))),
        (r#"[{"op": "scale", "by": [2, 0.5, 2]}, {"op": "gradient", "from": "pale", "to": "dark", "range": [0, 1]}]"#, Some(([0.0, 0.0, 0.0], [2.0, 0.5, 2.0]))),
        (r#"[{"op": "displace", "amount": 0.2, "scale": 1}]"#, None),
    ] {
        let r = format!(r#"{{ "name": "t", "lods": [], "parts": [ {{ "name": "g", "material": "stone", "generate": {{ "op": "terrain", "size": [10, 10], "samples": [17, 17], "layers": [{{ "scale": 4, "amplitude": 1 }}] }}, "ops": {ops} }} ] }}"#);
        let m = make(&r).unwrap_or_else(|e| panic!("{ops}: {e}"));
        let got = m.extras.terrain_transform.map(|a| (a.t, [a.cols[0][0], a.cols[1][1], a.cols[2][2]]));
        assert_eq!(got, want, "{ops}");
    }
}

#[test]
fn an_included_terrain_carries_the_including_ops() {
    let s = style();
    let mut l = lib();
    let ground = |dy: f32| format!(r#"{{ "name": "ground", "lods": [], "parts": [ {{ "name": "g", "material": "stone", "generate": {{ "op": "terrain", "size": [10, 10], "samples": [17, 17], "layers": [{{ "scale": 4, "amplitude": 0.5 }}] }}, "ops": [{{ "op": "move", "by": [0, {dy}, 0] }}] }} ] }}"#);
    l.insert("ground".to_string(), ground(1.0));
    l.insert("ground2".to_string(), ground(7.0));
    let t = |outer: &str| {
        let m = build(outer, &BuildParams { library: Some(&l), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{outer}: {e}"));
        assert!(m.extras.terrain.is_some(), "{outer}");
        m.extras.terrain_transform.map(|a| (a.t, [a.cols[0][0], a.cols[1][1], a.cols[2][2]], a.cols[0][2].abs() > 1e-3))
    };
    let one = [1.0f32; 3];
    // The include's own move (1 up), then the including part's (3 up), then the outer finish.
    assert_eq!(t(r#"{ "name": "o", "lods": [], "parts": [ { "name": "i", "include": "ground", "ops": [ { "op": "move", "by": [0, 3, 0] } ] } ] }"#), Some(([0.0, 4.0, 0.0], one, false)));
    assert_eq!(t(r#"{ "name": "o", "lods": [], "parts": [ { "name": "i", "include": "ground" } ], "finish": [ { "op": "move", "by": [5, 0, 0] } ] }"#), Some(([5.0, 1.0, 0.0], one, false)));
    // A rotation is carried (the tools refuse it); a displace reshapes it.
    assert_eq!(t(r#"{ "name": "o", "lods": [], "parts": [ { "name": "i", "include": "ground", "ops": [ { "op": "rotate", "deg": [0, 30, 0] } ] } ] }"#).map(|x| x.2), Some(true));
    assert_eq!(t(r#"{ "name": "o", "lods": [], "parts": [ { "name": "i", "include": "ground", "ops": [ { "op": "displace", "amount": 0.5 } ] } ] }"#), None);
    // Two terrains: the first is the model's, the second never overwrites it.
    assert_eq!(t(r#"{ "name": "o", "lods": [], "parts": [ { "name": "a", "include": "ground" }, { "name": "b", "include": "ground2" } ] }"#), Some(([0.0, 1.0, 0.0], one, false)));
}

#[test]
fn a_scatter_keeps_clear_of_an_earlier_one() {
    let stones = |clear: &str| {
        GRAVEYARD.replacen(
            "] } } ] }",
            &format!(
                r#"] }} }},
  {{ "name": "stones", "material": "stone", "generate": {{ "op": "scatter", "on": "ground", "area": {{ "rect": [14, 10] }}, "min_distance": 0.7, "count": 60, {clear}
      "items": [ {{ "shape": {{ "op": "sphere", "radius": 0.15 }} }} ] }} }} ] }}"#
            ),
            1,
        )
    };
    let rejected = |m: &Model| -> f64 {
        let r = m.report();
        let steps = r.get("procgen").and_then(|p| p.get("steps")).and_then(|s| s.as_array()).expect("steps");
        let last = steps.iter().rev().find_map(|s| s.get("did").filter(|d| d.get("op").and_then(|o| o.as_str()) == Some("scatter"))).expect("a scatter step");
        last.get("rejected").and_then(|r| r.get("avoid")).and_then(|v| v.as_f64()).expect("rejected.avoid")
    };
    let free = make(&stones("")).unwrap_or_else(|e| panic!("{e}"));
    let clear = make(&stones(r#""clear_of": { "part": "posts", "radius": 0.6 },"#)).unwrap_or_else(|e| panic!("{e}"));
    assert!(rejected(&clear) > rejected(&free), "{} vs {}", rejected(&clear), rejected(&free));
    let e = make(&stones(r#""clear_of": { "part": "ground", "radius": 0.6 },"#)).unwrap_err();
    assert_eq!((e.path.as_str(), e.code), ("parts[2].generate.clear_of.part", ErrorCode::UnknownName), "{e}");
}

#[test]
fn texture_names_are_per_recipe() {
    // An including recipe naming a different graph "stone" doesn't repaint the crypt's walls.
    let (s, mut l) = (style(), lib());
    l.insert("crypt".to_string(), CRYPT.to_string());
    l.insert(
        "other_tex".to_string(),
        r#"{ "texgraph": 1, "size": 16, "nodes": { "n": { "op": "noise" } }, "outputs": { "base_color": "n" } }"#.to_string(),
    );
    let outer = r#"{ "name": "outer", "textures": { "stone": "other_tex" }, "parts": [
        { "name": "c", "include": "crypt" },
        { "name": "slab", "material": "stone", "texture": "stone", "shape": { "op": "box", "size": [6, 0.2, 6] } } ] }"#;
    let m = build(outer, &BuildParams { library: Some(&l), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{e}"));
    let keys: Vec<&str> = m.extras.textures.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    let size = |k: &str| m.extras.textures.iter().find(|(n, _)| n.ends_with(k)).map(|(_, t)| t.size);
    assert_eq!((size("lib:stone_tex"), size("lib:other_tex")), (Some(32), Some(16)));
    // A building without a part material dresses its walls only.
    let bare = CRYPT.replace(r#""material": "stone", "texture""#, r#""texture""#).replace(r#""materials": { "roof""#, r#""materials": { "walls": "stone", "roof""#);
    let m = build(&bare, &BuildParams { library: Some(&l), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{e}"));
    let mats: Vec<&str> = m.lods[0].parts.iter().map(|p| p.material.as_str()).collect();
    assert!(mats.contains(&"stone+lib:stone_tex") && mats.contains(&"wood") && mats.contains(&"glass"), "{mats:?}");
}

#[test]
fn hostile_procgen_is_bounded() {
    // A boolean of many dense operands runs out of budget with a structured error.
    let mut ops = String::new();
    for i in 0..40 {
        ops.push_str(&format!(r#"{{ "op": "sphere", "radius": 0.5, "subdivisions": 3, "ops": [ {{ "op": "move", "by": [{}, 0, 0] }} ] }},"#, i as f32 * 0.05));
    }
    let text = format!(r#"{{ "name": "h", "parts": [ {{ "name": "p", "material": "stone", "shape": {{ "op": "union", "of": [ {} {{ "op": "box", "size": [1, 1, 1] }} ] }} }} ] }}"#, ops);
    let e = make(&text).unwrap_err();
    assert!(matches!(e.code, ErrorCode::Limit), "{e}");
    // Too many operands.
    let many = vec![r#"{ "op": "box", "size": [1, 1, 1] }"#; 70].join(",");
    let e = make(&format!(r#"{{ "name": "h", "parts": [ {{ "name": "p", "material": "stone", "shape": {{ "op": "union", "of": [ {many} ] }} }} ] }}"#)).unwrap_err();
    assert_eq!((e.path.as_str(), e.code), ("parts[0].shape.of", ErrorCode::Limit));
    // An open operand.
    let e = make(r#"{ "name": "h", "parts": [ { "name": "p", "material": "stone", "shape": { "op": "difference", "of": [ { "op": "box", "size": [1, 1, 1] }, { "op": "plane", "size": [2, 2] } ] } } ] }"#).unwrap_err();
    assert_eq!(e.code, ErrorCode::Geometry);
    // A texture graph too big for the budget is refused before baking.
    let big = r#"{ "name": "h", "textures": { "t": { "size": 2048, "nodes": { "n": { "op": "noise" } }, "outputs": { "base_color": "n" } } },
                   "parts": [ { "name": "p", "material": "stone", "texture": "t", "shape": { "op": "box", "size": [1, 1, 1] } } ] }"#;
    let e = make(big).unwrap_err();
    assert_eq!((e.path.as_str(), e.code), ("textures.t.size", ErrorCode::OutOfRange));
    // A texgraph file isn't a recipe.
    let e = make(&lib()["stone_tex"]).unwrap_err();
    assert_eq!(e.path, "texgraph");
}
