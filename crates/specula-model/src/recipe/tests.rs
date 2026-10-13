use super::*;

const STYLE: &str = r##"{ "style": "test",
  "palette": { "pale": "#b8fff6", "dark": "#202020" },
  "materials": { "stone": { "color": "#8c96a3", "roughness": 0.75 },
                 "glow": { "color": "#3fe0d0", "emissive": 2.5 } } }"##;

fn style() -> Style {
    Style::parse(STYLE).unwrap()
}

const PILLAR: &str = r##"{ "recipe": 1, "name": "pillar", "seed": 5,
  "params": { "tier": { "default": 1, "min": 1, "max": 3, "integer": true } },
  "let": { "h": "2 + 0.5 * (tier - 1)" },
  "budget": { "max_triangles": "tier >= 3 ? 3000 : 1500" },
  "lods": [0.5, 0.25],
  "collider": "convex_hull",
  "parts": [
    { "name": "body", "material": "stone",
      "shape": { "op": "box", "size": [0.8, "h", 0.8], "bevel": 0.05 },
      "ops": [ { "op": "subdivide", "levels": 1 },
               { "op": "displace", "amount": 0.02, "scale": 0.3 },
               { "op": "bevel_tint", "color": "pale", "amount": 0.5 },
               { "op": "noise_tint", "amount": 0.1, "note": "weathering" } ] },
    { "name": "rune", "material": "glow", "when": "tier >= 2", "lod_detail": 0,
      "shape": { "op": "box", "size": [0.08, 0.6, 0.03] },
      "ops": [ { "op": "move", "by": [0, "h * 0.4", 0.4] } ] },
    { "name": "ring", "material": "glow", "when": "tier >= 3",
      "shape": { "op": "torus", "radius": 0.7, "tube": 0.04, "segments": 24, "sides": 6 },
      "ops": [ { "op": "move", "by": [0, "h + 0.3", 0] } ] } ],
  "finish": [ { "op": "ground_shade", "amount": 0.3, "height": 0.4 } ] }"##;

fn build_with(text: &str, values: &[(&str, f64)]) -> Result<Model, RecipeError> {
    let s = style();
    build(text, &BuildParams { values, ..BuildParams::new(&s) })
}

fn err(text: &str) -> RecipeError {
    build_with(text, &[]).unwrap_err()
}

#[test]
fn builds_tiers_with_lods_and_collider() {
    let mut prev = 0;
    for tier in 1..=3 {
        let m = build_with(PILLAR, &[("tier", tier as f64)]).unwrap();
        assert!((2..=3).contains(&m.lods.len()), "{:?}", m.notes);
        assert_eq!(m.lod_ratios[..2], [1.0, 0.5]);
        assert_eq!(m.lods.len() == 3, m.notes.is_empty(), "a skipped LOD is noted: {:?}", m.notes);
        let n0 = m.lods[0].triangle_count();
        assert!(n0 > prev, "higher tiers add parts");
        prev = n0;
        for (i, l) in m.lods.iter().enumerate() {
            l.validate().unwrap();
            assert!(i == 0 || l.triangle_count() < n0);
        }
        assert!(matches!(m.collider, Collider::ConvexHull { .. }));
        let mats: Vec<&str> = m.materials.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(mats, if tier == 1 { vec!["stone"] } else { vec!["stone", "glow"] });
        assert_eq!(m.params, vec![("tier".to_string(), tier as f64)]);
        let b = m.bounds;
        assert!((b.max[1] - (2.0 + 0.5 * (tier - 1) as f32 + if tier == 3 { 0.38 } else { 0.0 })).abs() < 0.05, "{b:?}");
        let report = m.report().to_pretty();
        assert!(report.contains("\"triangles\""));
    }
}

#[test]
fn deterministic_and_seeded() {
    let a = build_with(PILLAR, &[("tier", 3.0)]).unwrap();
    let b = build_with(PILLAR, &[("tier", 3.0)]).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.hash, b.hash);
    let s = style();
    let c = build(PILLAR, &BuildParams { values: &[("tier", 3.0)], seed: Some(6), ..BuildParams::new(&s) }).unwrap();
    assert_ne!(a.hash, c.hash, "a new seed changes the displacement");
    assert_eq!(c.seed, 6);
}

#[test]
fn hash_is_pinned() {
    // If this changes, every built asset changed: check that was intended (an algorithm or
    // constant change), then update the pin.
    let m = build_with(PILLAR, &[("tier", 3.0)]).unwrap();
    assert_eq!(format!("{:016x}", m.hash), PINNED_PILLAR_HASH, "LOD0 hash {:016x}", m.lods[0].content_hash());
}

const PINNED_PILLAR_HASH: &str = "3123160fe8d87729";

#[test]
fn parameter_errors() {
    let e = build_with(PILLAR, &[("tier", 4.0)]).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::OutOfRange, "params.tier"));
    assert_eq!(e.measured, Some(4.0));
    let e = build_with(PILLAR, &[("tier", 1.5)]).unwrap_err();
    assert_eq!(e.code, ErrorCode::OutOfRange);
    let e = build_with(PILLAR, &[("height", 1.0)]).unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::UnknownName, "params.height"));
}

fn one_part(shape: &str, ops: &str) -> String {
    format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {shape}, "ops": [{ops}] }} ] }}"#)
}

#[test]
fn structural_errors_have_paths_and_codes() {
    let cases: Vec<(String, ErrorCode, &str)> = vec![
        ("{".into(), ErrorCode::Json, ""),
        (r#"{"name":"x"}"#.into(), ErrorCode::MissingField, "parts"),
        (r#"{"name":"x","parts":[],"colour":1}"#.into(), ErrorCode::UnknownField, "colour"),
        (r#"{"recipe":2,"name":"x","parts":[]}"#.into(), ErrorCode::OutOfRange, "recipe"),
        (one_part(r#"{"op":"box","size":[1,1,1],"bevl":0.1}"#, ""), ErrorCode::UnknownField, "parts[0].shape.bevl"),
        (one_part(r#"{"op":"cube","size":[1,1,1]}"#, ""), ErrorCode::UnknownOp, "parts[0].shape.op"),
        (one_part(r#"{"op":"box"}"#, ""), ErrorCode::MissingField, "parts[0].shape.size"),
        (one_part(r#"{"op":"box","size":[1,1]}"#, ""), ErrorCode::WrongType, "parts[0].shape.size"),
        (one_part(r#"{"op":"box","size":[1,"1 +",1]}"#, ""), ErrorCode::Expression, "parts[0].shape.size[1]"),
        (one_part(r#"{"op":"box","size":[1,"w",1]}"#, ""), ErrorCode::UnknownName, "parts[0].shape.size[1]"),
        (one_part(r#"{"op":"box","size":[1,1,1]}"#, r#"{"op":"spin"}"#), ErrorCode::UnknownOp, "parts[0].ops[0].op"),
        (one_part(r#"{"op":"box","size":[1,1,1]}"#, r#"{"op":"color","color":"red"}"#), ErrorCode::UnknownColor, "parts[0].ops[0].color"),
        (one_part(r#"{"op":"box","size":[1,1,1]}"#, r#"{"op":"mirror","axis":"w"}"#), ErrorCode::WrongType, "parts[0].ops[0].axis"),
        (one_part(r#"{"op":"box","size":[1,1,1]}"#, r#"{"op":"subdivide","levels":9}"#), ErrorCode::OutOfRange, "parts[0].ops[0].levels"),
        (one_part(r#"{"op":"box","size":[1,-1,1]}"#, ""), ErrorCode::OutOfRange, "parts[0].shape.size"),
        (one_part(r#"{"op":"extrude","outline":[[0,0],[1,1],[1,0],[0,1]],"height":1}"#, ""), ErrorCode::Geometry, "parts[0].shape.outline"),
        (one_part(r#"{"op":"extrude","outline":{"rect":[1,1],"sides":4},"height":1}"#, ""), ErrorCode::WrongType, "parts[0].shape.outline"),
        (one_part(r#"{"op":"sweep","profile":{"sides":4,"radius":0.1},"path":[[0,0,0],[0,0,0]]}"#, ""), ErrorCode::Geometry, "parts[0].shape.path"),
        (one_part(r#"{"op":"torus","radius":0.1,"tube":0.2}"#, ""), ErrorCode::OutOfRange, "parts[0].shape.tube"),
        (r#"{"name":"x","parts":[{"name":"p","material":"gold","shape":{"op":"box","size":[1,1,1]}}]}"#.into(), ErrorCode::UnknownMaterial, "parts[0].material"),
        (r#"{"name":"x","parts":[{"name":"p","material":"stone"}]}"#.into(), ErrorCode::MissingField, "parts[0]"),
        (r#"{"name":"x","lods":[0.5,0.6],"parts":[{"name":"p","material":"stone","shape":{"op":"box","size":[1,1,1]}}]}"#.into(), ErrorCode::OutOfRange, "lods[1]"),
        (
            r#"{"name":"x","budget":{"max_triangles":10},"parts":[{"name":"p","material":"stone","shape":{"op":"box","size":[1,1,1],"bevel":0.1}}]}"#.into(),
            ErrorCode::OverBudget,
            "budget.max_triangles",
        ),
        (r#"{"name":"x","parts":[{"name":"p","material":"stone","when":"0","shape":{"op":"box","size":[1,1,1]}}]}"#.into(), ErrorCode::Geometry, "parts"),
        (r#"{"name":"x","parts":[{"name":"p","include":"nothing"}]}"#.into(), ErrorCode::UnknownAsset, "parts[0].include"),
    ];
    for (text, code, path) in cases {
        let e = err(&text);
        assert_eq!((e.code, e.path.as_str()), (code, path), "{text}\n→ {e}");
        assert!(!e.message.is_empty());
        let j = e.to_json().to_pretty();
        assert!(j.contains("\"code\""));
    }
    let e = err(&one_part(r#"{"op":"box","size":[1,1,1],"bevel":0.1}"#, "").replace("\"name\": \"t\"", "\"name\": \"t\", \"budget\": {\"max_triangles\": 10}"));
    assert_eq!((e.measured, e.limit), (Some(60.0), Some(10.0)));
    let e = err(&one_part(r#"{"op":"box","size":[1,"1 + * 2",1]}"#, ""));
    assert_eq!(e.offset, Some(4));
}

#[test]
fn disabled_ops_are_still_validated() {
    let e = err(&one_part(r#"{"op":"box","size":[1,1,1]}"#, r#"{"op":"move","when":"0","by":[0,"nope",0]}"#));
    assert_eq!(e.code, ErrorCode::UnknownName);
}

#[test]
fn every_shape_and_op_builds() {
    let shapes = [
        r#"{"op":"box","size":[1,1,1],"bevel":0.05}"#,
        r#"{"op":"cylinder","radius":0.5,"height":1,"sides":10,"bevel":0.03,"radius_top":0.4}"#,
        r#"{"op":"cone","radius":0.5,"height":1}"#,
        r#"{"op":"sphere","radius":0.5,"subdivisions":2}"#,
        r#"{"op":"torus","radius":0.5,"tube":0.1}"#,
        r#"{"op":"capsule","radius":0.2,"height":1}"#,
        r#"{"op":"extrude","outline":{"arch":[0.5,0.8],"segments":6},"height":0.15,"bevel":0.02}"#,
        r#"{"op":"extrude","outline":{"star":5,"radius":0.5,"inner":0.2,"jitter":0.05},"height":0.3,"taper":0.5}"#,
        r#"{"op":"extrude","outline":{"sides":6,"radius":0.5,"bevel":0.05},"height":0.3,"cap_inset":[0.1,-0.05],"segments":2}"#,
        r#"{"op":"loft","outline":{"rect":[1,1],"bevel":0.05},"stations":[[0.9,0],[1,0.1],[1,1.5,0.1,0],[0,2]]}"#,
        r#"{"op":"lathe","profile":[[0,0],[0.4,0],[0.3,0.5],[0,0.6]],"segments":10}"#,
        r#"{"op":"sweep","profile":{"sides":5,"radius":0.05},"path":[[0,0,0],[0,1,0],[0.5,1.5,0]],"scales":[1,0],"smooth":4}"#,
        r#"{"op":"sweep","profile":[[0.05,0],[0,0.05],[-0.05,0],[0,-0.05]],"path":[[1,0,0],[0,0,-1],[-1,0,0],[0,0,1]],"closed":true,"smooth":3}"#,
    ];
    for s in shapes {
        let m = build_with(&one_part(s, ""), &[]).unwrap_or_else(|e| panic!("{s}: {e}"));
        let l0 = &m.lods[0];
        assert!(l0.is_watertight(), "{s} must be watertight");
        assert!(l0.signed_volume() > 0.0, "{s} must face outward");
    }
    let plane = build_with(&one_part(r#"{"op":"plane","size":[2,2],"divisions":[4,4]}"#, ""), &[]).unwrap();
    assert_eq!(plane.lods[0].triangle_count(), 32);
    let ops = [
        r#"{"op":"move","by":[1,0,0]}"#,
        r#"{"op":"rotate","deg":[0,45,10],"pivot":[0,0.5,0]}"#,
        r#"{"op":"scale","by":2}"#,
        r#"{"op":"scale","by":[1,0.5,2]}"#,
        r#"{"op":"mirror","axis":"x"}"#,
        r#"{"op":"array","count":3,"offset":[1.2,0,0],"jitter":{"offset":[0.05,0,0.05],"rotate":[0,8,0],"scale":0.1}}"#,
        r#"{"op":"radial","count":5,"angle":180}"#,
        r#"{"op":"subdivide","levels":2}"#,
        r#"{"op":"displace","amount":0.05,"scale":0.4,"octaves":3,"axes":[1,0,1]}"#,
        r#"{"op":"taper","to":0.5}"#,
        r#"{"op":"twist","deg_per_m":30}"#,
        r#"{"op":"lean","x":0.2}"#,
        r#"{"op":"bend","deg_per_m":20}"#,
        r#"{"op":"weld","eps":0.001}"#,
        r##"{"op":"color","color":"#ff0000"}"##,
        r#"{"op":"gradient","from":"dark","to":"pale","range":[0.2,0.8]}"#,
        r#"{"op":"bands","colors":["dark","pale","stone"],"width":0.2,"wobble":0.05}"#,
        r#"{"op":"noise_tint","amount":0.2}"#,
        r#"{"op":"ao","amount":0.5}"#,
        r#"{"op":"ground_shade"}"#,
        r#"{"op":"bevel_tint","color":"pale"}"#,
    ];
    assert_eq!(ops.len() - 1, op_names().len(), "every op is covered (scale twice)");
    for o in ops {
        let m = build_with(&one_part(r#"{"op":"box","size":[1,1,1],"bevel":0.05}"#, o), &[]).unwrap_or_else(|e| panic!("{o}: {e}"));
        m.lods[0].validate().unwrap();
    }
    for name in shape_names() {
        let tag = format!("\"op\":\"{name}\"");
        assert!(name == "plane" || shapes.iter().any(|s| s.contains(&tag)), "shape {name} is covered");
    }
}

#[test]
fn includes_compose_recipes() {
    let child = r#"{ "name": "gem", "params": { "size": { "default": 0.2, "min": 0.05, "max": 1 } },
        "parts": [ { "name": "g", "material": "glow", "shape": { "op": "sphere", "radius": "size", "subdivisions": 1 } } ] }"#;
    let parent = r#"{ "name": "holder",
        "parts": [ { "name": "base", "material": "stone", "shape": { "op": "box", "size": [0.5, 0.5, 0.5] } },
                   { "name": "gem", "include": "gem", "params": { "size": 0.1 }, "ops": [ { "op": "move", "by": [0, 0.5, 0] } ] } ] }"#;
    let mut lib = BTreeMap::new();
    lib.insert("gem".to_string(), child.to_string());
    lib.insert("holder".to_string(), parent.to_string());
    let s = style();
    let p = BuildParams { library: Some(&lib), ..BuildParams::new(&s) };
    let m = build(parent, &p).unwrap();
    assert_eq!(m.lods[0].parts.len(), 2);
    assert!((m.bounds.max[1] - 0.7).abs() < 1e-4);
    // Errors inside an include point through it.
    let bad = parent.replace("\"size\": 0.1", "\"size\": 5");
    let e = build(&bad, &p).unwrap_err();
    assert_eq!(e.code, ErrorCode::OutOfRange);
    assert_eq!(e.path, "parts[1].include(gem).params.size");
    // Cycles are caught.
    let cyc = r#"{ "name": "loop", "parts": [ { "name": "again", "include": "loop" } ] }"#;
    lib.insert("loop".to_string(), cyc.to_string());
    let p = BuildParams { library: Some(&lib), ..BuildParams::new(&s) };
    assert_eq!(build(cyc, &p).unwrap_err().code, ErrorCode::IncludeCycle);
}

#[test]
fn lod_ratios_are_close_to_requested() {
    let m = build_with(PILLAR, &[("tier", 1.0)]).unwrap();
    let n0 = m.lods[0].triangle_count() as f32;
    for (l, r) in m.lods.iter().zip(&m.lod_ratios).skip(1) {
        let got = l.triangle_count() as f32 / n0;
        assert!(got <= r * 1.1 && got >= r * 0.5, "ratio {r}: got {got}");
    }
}
