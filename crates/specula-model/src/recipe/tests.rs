#![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]
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
        let skipped = m.notes.iter().any(|n| n.contains("skipped"));
        assert_eq!(m.lods.len() == 3, !skipped, "a skipped LOD is noted: {:?}", m.notes);
        assert!(m.notes.iter().all(|n| n.contains("skipped") || n.contains(COARSE_NOTE)), "{:?}", m.notes);
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

const PINNED_PILLAR_HASH: &str = "a874468d2a5ff929";

/// Small bevelled parts can't be clustered (each part's cell is capped by its size), so a far
/// LOD that clustering can't make comes from the recipe rebuilt coarse: no bevels, half the
/// segments, no displacement; within the bounds and volume tolerances, noted in the report.
#[test]
fn far_level_falls_back_to_a_coarse_rebuild() {
    const STONE: &str = r##"{ "recipe": 1, "name": "stone", "lods": [0.5, 0.25],
      "parts": [
        { "name": "footing", "material": "stone",
          "shape": { "op": "box", "size": [0.62, 0.07, 0.28], "bevel": 0.02 },
          "ops": [ { "op": "displace", "amount": 0.01, "scale": 0.15 } ] },
        { "name": "slab", "material": "stone",
          "shape": { "op": "extrude", "outline": { "arch": [0.5, 0.8], "segments": 6 }, "height": 0.15, "bevel": 0.02, "corner_bevel": 0.02 },
          "ops": [ { "op": "rotate", "deg": [-90, 0, 0] }, { "op": "move", "by": [0, 0.05, 0.075] } ] },
        { "name": "mark", "material": "glow",
          "shape": { "op": "box", "size": [0.05, 0.26, 0.012], "bevel": 0.004 },
          "ops": [ { "op": "move", "by": [0, 0.4, 0.079] } ] },
        { "name": "bar", "material": "glow",
          "shape": { "op": "box", "size": [0.17, 0.05, 0.012], "bevel": 0.004 },
          "ops": [ { "op": "move", "by": [0, 0.56, 0.079] } ] } ] }"##;
    let m = build_with(STONE, &[]).unwrap();
    let counts: Vec<usize> = m.lods.iter().map(|l| l.triangle_count()).collect();
    assert_eq!(m.lods.len(), 3, "{counts:?} {:?}", m.notes);
    assert!(counts[2] * 2 <= counts[0] && counts[2] < counts[1], "the coarse level is a real far level: {counts:?}");
    assert!(m.notes.iter().any(|n| n.starts_with("lods[1] (0.25)") && n.contains(COARSE_NOTE)), "{:?}", m.notes);
    let (b0, b2) = (m.lods[0].bounds(), m.lods[2].bounds());
    let s = b0.size();
    let tol = lod::MODEL_BOUNDS_TOLERANCE * s[0].max(s[1]).max(s[2]);
    for k in 0..3 {
        assert!((b0.min[k] - b2.min[k]).abs() <= tol && (b0.max[k] - b2.max[k]).abs() <= tol, "{b0:?} vs {b2:?}");
    }
    assert_eq!(m.lods[2].parts.len(), m.lods[0].parts.len(), "every material stays");
    assert_eq!(build_with(STONE, &[]).unwrap(), m, "deterministic");

    // A shape the coarse rebuild would change too much (a pentagonal disc halved to a triangle
    // moves its bounds) doesn't get one.
    const DISC: &str = r##"{ "recipe": 1, "name": "disc", "lods": [0.25],
      "parts": [ { "name": "d", "material": "stone",
        "shape": { "op": "cylinder", "radius": 1.0, "height": 0.02, "sides": 5, "bevel": 0.005 } } ] }"##;
    let d = build_with(DISC, &[]).unwrap();
    assert!(d.notes.iter().all(|n| !n.contains(COARSE_NOTE)), "{:?}", d.notes);
}

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
        // Roundness as one number: one way to round at a time, within 0 to 1, one radius for a cylinder.
        (one_part(r#"{"op":"sdf","tree":{"op":"box","size":[1,1,1],"roundness":0.5,"corner":0.1}}"#, ""), ErrorCode::Geometry, "parts[0].shape.tree.roundness"),
        (one_part(r#"{"op":"sdf","tree":{"op":"box","size":[1,1,1],"roundness":1.5}}"#, ""), ErrorCode::OutOfRange, "parts[0].shape.tree.roundness"),
        (one_part(r#"{"op":"sdf","tree":{"op":"cylinder","radius":0.3,"radius_top":0.2,"height":1,"roundness":0.5}}"#, ""), ErrorCode::Geometry, "parts[0].shape.tree.roundness"),
        (one_part(r#"{"op":"extrude","outline":{"rect":[1,1],"roundness":2},"height":1}"#, ""), ErrorCode::OutOfRange, "parts[0].shape.outline.roundness"),
        // (And a field that would do nothing on an outline is refused, not ignored.)
        (one_part(r#"{"op":"extrude","outline":{"sides":6,"radius":0.5,"roundness":0.5},"height":1}"#, ""), ErrorCode::Geometry, "parts[0].shape.outline.roundness"),
        (one_part(r#"{"op":"extrude","outline":{"star":5,"radius":0.5,"roundness":0.5},"height":1}"#, ""), ErrorCode::Geometry, "parts[0].shape.outline.roundness"),
        (one_part(r#"{"op":"extrude","outline":{"rect":[1,1],"segments":300},"height":1}"#, ""), ErrorCode::Geometry, "parts[0].shape.outline.segments"),
        (one_part(r#"{"op":"extrude","outline":{"rect":[1,1],"roundness":0.5,"segments":300},"height":1}"#, ""), ErrorCode::OutOfRange, "parts[0].shape.outline.segments"),
        (one_part(r#"{"op":"extrude","outline":{"sides":6,"radius":0.5,"segments":12},"height":1}"#, ""), ErrorCode::Geometry, "parts[0].shape.outline.segments"),
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
        r#"{"op":"difference","of":[{"op":"box","size":[1,1,1]},{"op":"box","size":[0.4,2,0.4],"ops":[{"op":"move","by":[0,-0.5,0]}]}],"bevel":0.03}"#,
        r#"{"op":"union","of":[{"op":"box","size":[1,1,1]},{"op":"box","size":[0.5,0.5,0.5],"ops":[{"op":"move","by":[0.5,0.25,0]}]}]}"#,
        r#"{"op":"intersection","of":[{"op":"box","size":[1,1,1]},{"op":"sphere","radius":0.65,"subdivisions":2}]}"#,
        r#"{"op":"sdf","resolution":24,"tree":{"op":"union","smooth":0.1,"of":[{"op":"sphere","radius":0.4},{"op":"box","size":[0.5,0.5,0.5],"at":[0.3,0,0]}]}}"#,
        r#"{"op":"figure","part":"head","sex":"female","stature":1.7,"build":0.5,"age":40,"traits":{"legs":1.2,"shoulders":-0.8}}"#,
        r#"{"op":"figure","part":"foot_l","sex":"male"}"#,
        r#"{"op":"sdf","resolution":32,"tree":{"op":"sphere","radius":0.4,"stretch":[1.5,1,0.6],"at":[0,0.5,0]}}"#,
        r#"{"op":"sdf","resolution":40,"tree":{"op":"column","from":[0,0.1,0],"to":[0.1,0.9,0],"sections":[[0,0.2,0.12],[0.5,0.14,0.1],[1,0.08,0.08]]}}"#,
        r#"{"op":"sdf","resolution":32,"tree":{"op":"round_cone","from":[0,0.1,0],"to":[0.3,0.8,0.1],"radius":0.1,"radius_to":0.04}}"#,
        r#"{"op":"sdf","resolution":40,"tree":{"op":"column","from":[0,0.1,0],"to":[0,0.9,0],"square":3,"sections":[[0,0.2,0.12],[1,0.16,0.1]]}}"#,
        r#"{"op":"sdf","resolution":32,"tree":{"op":"box","size":[0.8,0.5,0.4],"roundness":0.5,"at":[0,0.4,0]}}"#,
        // (With the modifier `round`, a swelling in metres, which is another thing and may go with it.)
        r#"{"op":"sdf","resolution":32,"tree":{"op":"box","size":[0.8,0.5,0.4],"roundness":0.3,"round":0.02,"at":[0,0.4,0]}}"#,
        r#"{"op":"sdf","resolution":32,"tree":{"op":"cylinder","radius":0.3,"height":0.8,"roundness":0.3,"at":[0,0.5,0]}}"#,
        r#"{"op":"extrude","outline":{"rect":[1,0.6],"roundness":0.5,"segments":32},"height":0.4}"#,
        // (A roundness of nothing is the rectangle, and may keep its `segments`.)
        r#"{"op":"extrude","outline":{"rect":[1,0.6],"roundness":0,"segments":16},"height":0.4}"#,
        // What the review of roundness built that came out cut flat: a thin board swollen by a centimetre.
        // (Its board thinner than a cell, turned, builds now and is no more watertight than the hard box is at
        // that size: `sdf::tests::a_rounded_shape_is_measured_in_metres`.)
        r#"{"op":"sdf","resolution":48,"tree":{"op":"box","size":[1,0.05,0.6],"roundness":0.2,"round":0.01,"at":[0,0.5,0]}}"#,
        r#"{"op":"sdf","resolution":40,"tree":{"op":"chain","smooth":0.03,"mirror":"x","points":[[0.2,1.4,0,0.05],[0.25,1.1,0,0.045],[0.27,0.85,0.05,0.03],[0.27,0.75,0.1,0.035]]}}"#,
        r#"{"op":"form","form":"snub_cube","args":{"edge":0.4},"detail":16}"#,
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

/// A legged test creature: one mirrored leg part, a body, a head; leaned forward by `finish`.
const WALKER: &str = r##"{ "recipe": 1, "name": "walker",
  "params": { "lean": { "default": 0, "min": -30, "max": 30 } },
  "rig": { "blend": 0.06, "bones": [
    { "name": "root", "head": [0, 0, 0] },
    { "name": "hips", "parent": "root", "head": [0, 1, 0], "tail": [0, 1.5, 0] },
    { "name": "head", "parent": "hips", "head": [0, 1.5, 0], "tail": [0, 1.9, 0] },
    { "name": "thigh_l", "parent": "hips", "head": [0.2, 1, 0], "mirror": true },
    { "name": "shin_l", "parent": "thigh_l", "head": [0.2, 0.5, 0.05], "tail": [0.2, 0.05, 0], "mirror": true } ] },
  "parts": [
    { "name": "body", "material": "stone", "bones": "hips",
      "shape": { "op": "box", "size": [0.5, 0.5, 0.3] }, "ops": [ { "op": "move", "by": [0, 1, 0] } ] },
    { "name": "skull", "material": "stone", "bones": ["head"],
      "shape": { "op": "sphere", "radius": 0.18, "subdivisions": 1 }, "ops": [ { "op": "move", "by": [0, 1.5, 0] } ] },
    { "name": "legs", "material": "stone", "bones": ["thigh_l", "shin_l", "thigh_r", "shin_r"],
      "shape": { "op": "sweep", "profile": { "sides": 6, "radius": 0.06 },
                 "path": [[0.2, 1, 0], [0.2, 0.5, 0.05], [0.2, 0.05, 0]] },
      "ops": [ { "op": "mirror", "axis": "x" } ] } ],
  "finish": [ { "op": "rotate", "deg": ["lean", 0, 0], "pivot": [0, 0, 0] } ] }"##;

#[test]
fn rigs_bind_mirrored_parts_and_follow_the_finish() {
    let m = build_with(WALKER, &[]).unwrap();
    let rig = m.rig.as_ref().unwrap();
    let names: Vec<&str> = rig.bones.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["root", "hips", "head", "thigh_l", "thigh_r", "shin_l", "shin_r"]);
    // Twins are mirrored and hang from the parent's twin; default tail = the only child's head.
    let (tr, sr) = (&rig.bones[4], &rig.bones[6]);
    assert_eq!(tr.head, [-0.2, 1.0, 0.0]);
    assert_eq!(tr.tail, [-0.2, 0.5, 0.05]);
    assert_eq!(sr.parent, Some(4));
    assert_eq!(rig.bones[0].tail, [0.0, 1.0, 0.0], "the root's only child is the hips");
    assert_eq!(rig.bones[2].tail, [0.0, 1.9, 0.0]);
    assert_eq!(rig.bones[6].tail, [-0.2, 0.05, 0.0]);
    for (i, l) in m.lods.iter().enumerate() {
        l.validate().unwrap_or_else(|e| panic!("LOD{i}: {e}"));
        assert_eq!(l.joints.len(), l.vertex_count(), "LOD{i} is skinned");
    }
    // Every leg vertex binds to its own side; the skull only to the head.
    let l0 = &m.lods[0];
    for (p, (j, w)) in l0.positions.iter().zip(l0.joints.iter().zip(&l0.weights)) {
        let bones: Vec<&str> = (0..4).filter(|k| w[*k] > 0).map(|k| rig.bones[j[k] as usize].name.as_str()).collect();
        if p[1] < 0.75 {
            let side = if p[0] > 0.0 { "_l" } else { "_r" };
            assert!(bones.iter().all(|b| b.ends_with(side)), "{p:?} bound to {bones:?}");
        }
        if p[1] > 1.6 {
            assert_eq!(bones, ["head"]);
        }
    }
    // Knee vertices blend thigh and shin.
    assert!(l0.weights.iter().any(|w| w[1] > 0), "some vertices blend between two bones");
    // A leaning finish moves the rig with the mesh.
    let leaned = build_with(WALKER, &[("lean", 20.0)]).unwrap();
    let head = leaned.rig.as_ref().unwrap().bones[2].head;
    assert!(head[2] > 0.45 && (head[1] - 1.5 * 20f32.to_radians().cos()).abs() < 1e-4, "{head:?}");
    assert_ne!(leaned.hash, m.hash);
    assert_eq!(build_with(WALKER, &[]).unwrap().hash, m.hash);
}

#[test]
fn rig_errors_have_paths_and_codes() {
    let cases: &[(&str, &str, ErrorCode, &str)] = &[
        ("\"bones\": [\"thigh_l\", \"shin_l\", \"thigh_r\", \"shin_r\"]", "\"bones\": [\"thigh_l\", \"knee\"]", ErrorCode::UnknownName, "parts[2].bones[1]"),
        ("\"bones\": \"hips\"", "\"bones\": []", ErrorCode::WrongType, "parts[0].bones"),
        ("\"bones\": \"hips\"", "\"bones\": \"hips\", \"blend\": 20", ErrorCode::OutOfRange, "parts[0].blend"),
        ("{ \"name\": \"head\", \"parent\": \"hips\"", "{ \"name\": \"head\", \"parent\": \"neck\"", ErrorCode::UnknownName, "rig.bones[2].parent"),
        ("{ \"name\": \"hips\", \"parent\": \"root\",", "{ \"name\": \"hips\",", ErrorCode::MissingField, "rig.bones[1].parent"),
        ("{ \"name\": \"root\", \"head\"", "{ \"name\": \"root\", \"parent\": \"hips\", \"head\"", ErrorCode::InvalidRig, "rig.bones[0].parent"),
        ("{ \"name\": \"head\", \"parent\"", "{ \"name\": \"hips\", \"parent\"", ErrorCode::Duplicate, "rig.bones[2].name"),
        ("\"name\": \"thigh_l\"", "\"name\": \"thigh\"", ErrorCode::InvalidRig, "rig.bones[3].mirror"),
        ("\"bones\": [\"head\"]", "\"bones\": [\"head\", \"head\"]", ErrorCode::Duplicate, "parts[1].bones[1]"),
        ("\"head\": [0, 1.5, 0], \"tail\"", "\"head\": [0, 1.5], \"tail\"", ErrorCode::WrongType, "rig.bones[2].head"),
        ("\"blend\": 0.06", "\"blend\": 0.06, \"scale\": 2", ErrorCode::UnknownField, "rig.scale"),
        ("[0, 1.5, 0], \"tail\": [0, 1.9, 0] }", "[0, 1.5, 0], \"tail\": [0, 1.9, 0], \"roll\": 3 }", ErrorCode::UnknownField, "rig.bones[2].roll"),
        ("\"pivot\": [0, 0, 0] }", "\"pivot\": [0, 0, 0] }, { \"op\": \"mirror\", \"axis\": \"z\" }", ErrorCode::WrongType, "finish[1]"),
    ];
    for (from, to, code, path) in cases {
        assert!(WALKER.contains(from), "case text {from}");
        let er = err(&WALKER.replacen(from, to, 1));
        assert_eq!((er.code, er.path.as_str()), (*code, *path), "{to}: {er}");
    }
    // Bones without a rig.
    let no_rig = one_part(r#"{ "op": "box", "size": [1, 1, 1] }"#, "").replacen("\"material\"", "\"bones\": \"hips\", \"material\"", 1);
    assert_eq!(err(&no_rig).code, ErrorCode::MissingField);
    // A rigid mirror (no keep) is followed: the twin bones swap sides.
    let flipped = WALKER.replacen("\"pivot\": [0, 0, 0] }", "\"pivot\": [0, 0, 0] }, { \"op\": \"mirror\", \"axis\": \"x\", \"keep\": false }", 1);
    let m = build_with(&flipped, &[]).unwrap();
    assert_eq!(m.rig.unwrap().bones[3].head, [-0.2, 1.0, 0.0]);
}

/// A rig made from a measured body: the bones every figure has, placed by the body, and its
/// parts bound to them. A hand's vertices move with the hand and the forearm and nothing else;
/// the same recipe with another body has the same bones in other places.
#[test]
fn a_figures_rig_is_its_bodys_skeleton() {
    let recipe = |stature: f32| {
        format!(
            r##"{{ "recipe": 1, "name": "person", "smooth_angle": 60,
              "rig": {{ "blend": 0.04, "figure": {{ "sex": "female", "stature": {stature} }} }},
              "parts": [
                {{ "name": "body", "material": "stone", "bones": ["root", "hips", "spine", "chest", "neck", "head", "upperarm_l", "upperarm_r", "forearm_l", "forearm_r", "hand_l", "hand_r", "thigh_l", "thigh_r", "shin_l", "shin_r", "foot_l", "foot_r"],
                  "shape": {{ "op": "figure", "part": "body", "sex": "female", "stature": {stature}, "resolution": 56 }} }},
                {{ "name": "hand_l", "material": "stone", "bones": ["forearm_l", "hand_l"],
                  "shape": {{ "op": "figure", "part": "hand_l", "sex": "female", "stature": {stature}, "resolution": 40 }} }},
                {{ "name": "foot_l", "material": "stone", "bones": ["shin_l", "foot_l"],
                  "shape": {{ "op": "figure", "part": "foot_l", "sex": "female", "stature": {stature}, "resolution": 40 }} }} ] }}"##
        )
    };
    let m = build_with(&recipe(1.60), &[]).unwrap_or_else(|e| panic!("{e}"));
    let rig = m.rig.as_ref().unwrap();
    assert_eq!(rig.bones.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), crate::figure::BONES.iter().map(|(n, _)| *n).collect::<Vec<_>>());
    let l0 = &m.lods[0];
    let (hand, forearm) = (rig.find("hand_l").unwrap() as u8, rig.find("forearm_l").unwrap() as u8);
    let mut on_hand = 0;
    for (i, p) in l0.positions.iter().enumerate() {
        let (joints, weights) = (l0.joints[i], l0.weights[i]);
        // Left of the left wrist is the hand's part: its vertices know the hand and the forearm only.
        if p[0] > rig.bones[hand as usize].head[0] + 0.02 {
            on_hand += 1;
            assert!((0..4).all(|k| weights[k] == 0 || joints[k] == hand || joints[k] == forearm), "{p:?} binds to {joints:?} {weights:?}");
        }
    }
    assert!(on_hand > 200, "{on_hand} vertices past the wrist");
    // The hips keep their sides: between the hip joints' height and the waist, out at the
    // body's side, a vertex moves with the pelvis or the spine and never mostly with a thigh
    // (bound by plain distance it did, and the pelvis tore when a leg swung). Down the thigh,
    // well under the crotch, it moves with the thigh.
    let bone = |name: &str| rig.find(name).unwrap();
    let (hips, spine, thigh_l, thigh_r) = (bone("hips") as u8, bone("spine") as u8, bone("thigh_l") as u8, bone("thigh_r") as u8);
    let (joint_y, waist_y, knee_y) = (rig.bones[thigh_l as usize].head[1], rig.bones[spine as usize].head[1], rig.bones[thigh_l as usize].tail[1]);
    let (mut sides, mut down) = (0, 0);
    for (i, p) in l0.positions.iter().enumerate() {
        let lead = l0.joints[i][0];
        if p[1] > joint_y + 0.02 && p[1] < waist_y && p[0].abs() > 0.10 && p[0].abs() < 0.22 {
            sides += 1;
            assert!(lead == hips || lead == spine, "{p:?} at the hips' side is led by {} ({:?})", rig.bones[lead as usize].name, l0.weights[i]);
        }
        if p[1] < joint_y - 0.16 && p[1] > knee_y + 0.08 && p[0].abs() < 0.3 {
            down += 1;
            assert!(lead == thigh_l || lead == thigh_r, "{p:?} down the thigh is led by {}", rig.bones[lead as usize].name);
        }
    }
    assert!(sides > 40 && down > 100, "{sides} at the hips' sides, {down} down the thighs");
    // The heel is the foot's: it lies behind the ankle along the foot's own line, and a rule
    // that gave a bone only what is ahead of its joint handed it to the shin (rings showed at
    // the ankles of the walking figure).
    let (foot, ankle) = (bone("foot_l") as u8, rig.bones[bone("foot_l")].head);
    let mut heel = 0;
    for (i, p) in l0.positions.iter().enumerate() {
        if p[0] > 0.0 && p[1] < 0.035 && p[2] < ankle[2] - 0.02 {
            heel += 1;
            assert_eq!(l0.joints[i][0], foot, "{p:?} on the heel is led by {} ({:?})", rig.bones[l0.joints[i][0] as usize].name, l0.weights[i]);
        }
    }
    assert!(heel > 20, "{heel} vertices on the heel");
    // The root has no flesh: listed with the rest, it takes no vertex. (It took the legs from
    // the knee down when it was scored in metres against bones scored in their radii.)
    let led_by_root = l0.joints.iter().zip(&l0.weights).filter(|(j, w)| (0..4).any(|k| w[k] > 0 && j[k] == 0)).count();
    assert_eq!(led_by_root, 0, "vertices that move with the root");
    // A taller body: the same bones, the head's higher.
    let tall = build_with(&recipe(1.80), &[]).unwrap_or_else(|e| panic!("{e}"));
    let (a, b) = (rig.bones[rig.find("head").unwrap()].head[1], tall.rig.as_ref().unwrap().bones[rig.find("head").unwrap()].head[1]);
    assert!(b > a + 0.12 && tall.rig.as_ref().unwrap().len() == rig.len(), "{a} {b}");
    // The tallest and the shortest statures the rows cover build, written as a recipe writes
    // them; a stature outside them is refused at its field, with what was given and the limit.
    for sex in ["female", "male"] {
        let limits = specula_corpus::ansur::statures(specula_corpus::Sex::parse(sex).unwrap()).map(|mm| mm / 1000.0);
        for stature in limits {
            let text = format!(r#"{{"op":"figure","part":"head","sex":"{sex}","stature":{stature}}}"#);
            build_with(&one_part(&text, ""), &[]).unwrap_or_else(|e| panic!("{sex} of {stature} m: {e}"));
        }
        let text = format!(r#"{{"op":"figure","part":"head","sex":"{sex}","stature":{}}}"#, limits[1] + 0.05);
        let er = build_with(&one_part(&text, ""), &[]).unwrap_err();
        assert!(er.path.ends_with(".stature") && er.to_string().contains("stature"), "{er} at {}", er.path);
    }
    // Bones and a figure together are refused, by name.
    let both = r##"{ "name": "x", "rig": { "figure": { "sex": "male" }, "bones": [ { "name": "root", "head": [0, 0, 0] } ] }, "parts": [] }"##;
    let er = build_with(both, &[]).unwrap_err();
    assert_eq!(er.path, "rig.bones", "{er}");
}

#[test]
fn includes_bind_to_the_including_parts_bones() {
    let parent = r##"{ "name": "rider", "rig": { "bones": [ { "name": "root", "head": [0, 0, 0] }, { "name": "saddle", "parent": "root", "head": [0, 1, 0] } ] },
      "parts": [ { "name": "mount", "include": "walker", "bones": "saddle" } ] }"##;
    let mut lib = BTreeMap::new();
    lib.insert("walker".to_string(), WALKER.to_string());
    let s = style();
    let m = build(parent, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) }).unwrap();
    assert_eq!(m.rig.as_ref().unwrap().len(), 2);
    assert!(m.lods[0].joints.iter().all(|j| j[0] == 1), "every included vertex binds to the saddle");
    // Without a rig, an included rigged recipe gives an unskinned mesh.
    let plain = r##"{ "name": "statue", "parts": [ { "name": "mount", "include": "walker" } ] }"##;
    let m = build(plain, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) }).unwrap();
    assert!(m.rig.is_none() && !m.lods[0].is_skinned());
}

/// A recipe's `wind`: the model carries it (its height defaulting to the model's), it is part of
/// the model's identity, and bad values are structured errors at their path.
#[test]
fn wind_terms_ride_on_the_model() {
    let s = style();
    let plain = build(PILLAR, &BuildParams::new(&s)).unwrap();
    assert!(plain.wind.is_none());
    let windy_text = PILLAR.replacen(r#""seed": 5,"#, r#""seed": 5, "wind": { "amount": 0.07 },"#, 1);
    let windy = build(&windy_text, &BuildParams::new(&s)).unwrap();
    let w = windy.wind.expect("a wind term");
    assert_eq!(w.amount, 0.07);
    assert!(!w.painted);
    assert!((w.height - windy.bounds.max[1]).abs() < 1e-6, "height defaults to the model's: {} vs {}", w.height, windy.bounds.max[1]);
    assert_ne!(plain.hash, windy.hash, "wind is part of the model's identity");
    assert_eq!(plain.lods[0].positions, windy.lods[0].positions, "the mesh itself is the same");
    let given = build(&PILLAR.replacen(r#""seed": 5,"#, r#""seed": 5, "wind": { "amount": 0.2, "height": 0.5, "painted": true },"#, 1), &BuildParams::new(&s)).unwrap();
    assert_eq!(given.wind.map(|w| (w.height, w.painted)), Some((0.5, true)));
    for (bad, path) in [
        (r#""wind": { "height": 1 }"#, "wind.amount"),
        (r#""wind": { "amount": 2 }"#, "wind.amount"),
        (r#""wind": { "amount": 0.1, "height": 9 }"#, "wind.height"),
        (r#""wind": { "amount": 0.1, "gusty": true }"#, "wind.gusty"),
        (r#""wind": { "amount": 0.1, "painted": 1 }"#, "wind.painted"),
        (r#""wind": 3"#, "wind"),
    ] {
        let text = PILLAR.replacen(r#""seed": 5,"#, &format!(r#""seed": 5, {bad},"#), 1);
        let e = build(&text, &BuildParams::new(&s)).unwrap_err();
        assert_eq!(e.path, path, "{bad}: {e:?}");
    }
}

/// What a recipe names round is recorded with how coarsely it was built, and what it names
/// a prism is not (a mesh alone cannot tell the two: an extruded outline of ten sides and a
/// ten-sided cylinder are the same triangles). The turn between neighbouring faces and the
/// radius it turns at, the ones whose facets stand furthest in from their curve first; the
/// coarse far level, which halves every count on purpose, records nothing.
#[test]
fn what_a_recipe_names_round_says_how_coarsely_it_was_built() {
    let part = |name: &str, shape: &str| format!(r#"{{ "name": "{name}", "material": "stone", "shape": {shape} }}"#);
    let text = format!(
        r#"{{ "recipe": 1, "name": "rounds", "lods": [0.25], "parts": [{}] }}"#,
        [
            part("column", r#"{"op":"cylinder","radius":0.5,"height":2,"sides":6}"#),
            part("post", r#"{"op":"cylinder","radius":0.1,"height":1}"#),
            part("spire", r#"{"op":"cone","radius":0.3,"height":1,"sides":24}"#),
            part("ball", r#"{"op":"sphere","radius":0.4,"subdivisions":2}"#),
            part("ring", r#"{"op":"torus","radius":0.7,"tube":0.05,"segments":24,"sides":6}"#),
            part("pill", r#"{"op":"capsule","radius":0.2,"height":1,"segments":16,"rings":2}"#),
            part("urn", r#"{"op":"lathe","profile":[[0.1,0],[0.3,0.4],[0.2,0.8]],"segments":10}"#),
            part("prism", r#"{"op":"extrude","outline":{"sides":6,"radius":0.5},"height":2}"#),
            part("crate", r#"{"op":"box","size":[1,1,1]}"#),
            part("bead", r#"{"op":"sdf","resolution":24,"tree":{"op":"sphere","radius":0.3}}"#),
        ]
        .join(", ")
    );
    let model = build_with(&text, &[]).unwrap();
    let round = &model.extras.round;
    let of = |path: &str| round.iter().filter(|r| r.path == path).collect::<Vec<_>>();
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    // Six sides: sixty degrees a side, at half a metre. Twelve unless said: thirty.
    assert!(matches!(of("parts[0].shape")[..], [r] if r.what == "cylinder" && near(r.turn_deg, 60.0) && near(r.radius_m, 0.5)), "{round:?}");
    assert!(matches!(of("parts[1].shape")[..], [r] if near(r.turn_deg, 30.0) && near(r.radius_m, 0.1)));
    assert!(matches!(of("parts[2].shape")[..], [r] if r.what == "cone" && near(r.turn_deg, 15.0)));
    assert!(matches!(of("parts[3].shape")[..], [r] if r.what == "sphere" && r.turn_deg > 41.81 / 4.0 - 0.01 && r.turn_deg < 1.2 * 41.81 / 4.0 && near(r.radius_m, 0.4)));
    // A torus turns twice: round its ring and round its tube.
    let ring = of("parts[4].shape");
    assert!(ring.len() == 2 && ring.iter().any(|r| near(r.turn_deg, 15.0) && near(r.radius_m, 0.75)) && ring.iter().any(|r| near(r.turn_deg, 60.0) && near(r.radius_m, 0.05)), "{ring:?}");
    // A capsule turns twice too: round its axis (by its segments) and over its ends (by its rings).
    let pill = of("parts[5].shape");
    assert!(pill.len() == 2 && pill.iter().any(|r| r.count == "segments" && near(r.turn_deg, 22.5)) && pill.iter().any(|r| r.count == "rings" && near(r.turn_deg, 45.0) && near(r.radius_m, 0.2)), "{pill:?}");
    assert!(matches!(of("parts[6].shape")[..], [r] if r.what == "lathe" && near(r.turn_deg, 36.0) && near(r.radius_m, 0.3)));
    // A prism and a box are what they are named: nothing is said of them.
    assert!(of("parts[7].shape").is_empty() && of("parts[8].shape").is_empty());
    // A sculpt: its cells against its tightest radius.
    assert!(matches!(of("parts[9].shape.tree#0")[..], [r] if r.what == "sdf" && r.count == "resolution" && r.count_value == 24 && near(r.radius_m, 0.3) && r.turn_deg > 2.0 && r.turn_deg < 20.0), "{round:?}");
    // The far level, built with half of every count, added nothing: one fact a round.
    assert!(round.len() == 10 && model.extras.round_total == 10);
    // Kept in the order the answer reads: first the furthest standing, the six-sided column by
    // 6.7 cm (half a metre, less its cosine of thirty degrees); then each that turns more than
    // all before it; then the rest by how far they stand in.
    let front = round.iter().scan(f32::NEG_INFINITY, |best, r| { let more = r.turn_deg > *best; *best = best.max(r.turn_deg); Some(more) }).take_while(|m| *m).count();
    assert!(round[..front].windows(2).all(|w| w[0].sag_m() >= w[1].sag_m() && w[0].turn_deg < w[1].turn_deg) && round[front..].windows(2).all(|w| w[0].sag_m() >= w[1].sag_m()), "{round:?}");
    assert!(round[0].path == "parts[0].shape" && near(round[0].sag_m(), 0.5 * (1.0 - 0.8660254)), "{:?}", round[0]);
    let j = round[0].to_json();
    assert_eq!((j.get("shape").and_then(Json::as_str), j.get("turn_deg").and_then(Json::as_f64)), (Some("cylinder"), Some(60.0)));
    // A model of boxes says nothing.
    assert!(build_with(&one_part(r#"{"op":"box","size":[1,1,1]}"#, ""), &[]).unwrap().extras.round.is_empty());
}

/// What a recipe is built of: each shape it names is recorded with its own outlines and its
/// surface in the model. A part's operations change the surface and not the outlines (three
/// copies count three times); what a boolean takes away is not recorded; a sculpt's
/// primitives are, the one a difference takes away left out; and what nothing speaks for is
/// said. (The far level, where it is built again coarsely, has a tally of its own: it cannot
/// add to this one.)
#[test]
fn what_a_recipe_is_built_of_is_recorded_with_its_surface() {
    let part = |name: &str, shape: &str, ops: &str| format!(r#"{{ "name": "{name}", "material": "stone", "shape": {shape}, "ops": [{ops}] }}"#);
    let text = format!(
        r#"{{ "recipe": 1, "name": "made", "lods": [0.25], "parts": [{}] }}"#,
        [
            part("crate", r#"{"op":"box","size":[1,2,3]}"#, ""),
            part("posts", r#"{"op":"box","size":[0.5,0.5,0.5]}"#, r#"{"op":"move","by":[4,0,0]},{"op":"array","count":3,"offset":[1,0,0]}"#),
            part("ball", r#"{"op":"sphere","radius":0.5,"subdivisions":3}"#, r#"{"op":"move","by":[-4,0,0]},{"op":"scale","by":2}"#),
            part("spire", r#"{"op":"cone","radius":0.3,"height":1,"sides":24}"#, r#"{"op":"move","by":[0,0,6]}"#),
            part("holed", r#"{"op":"difference","of":[{"op":"box","size":[2,2,2],"ops":[]},{"op":"cylinder","radius":0.4,"height":3,"ops":[]}]}"#, r#"{"op":"move","by":[0,0,-6]}"#),
            part("bead", r#"{"op":"sdf","resolution":24,"tree":{"op":"difference","of":[{"op":"sphere","radius":0.3},{"op":"box","size":[0.2,0.2,2]}]}}"#, r#"{"op":"move","by":[8,0,0]}"#),
            part("urn", r#"{"op":"lathe","profile":[[0.1,0],[0.3,0.4],[0.2,0.8]],"segments":10}"#, r#"{"op":"move","by":[-8,0,0]}"#),
        ]
        .join(", ")
    );
    let model = build_with(&text, &[]).unwrap();
    let made = &model.extras.made;
    let of = |path: &str| made.iter().find(|m| m.path == path).unwrap_or_else(|| panic!("nothing recorded at {path}: {:?}", made.iter().map(|m| m.path.as_str()).collect::<Vec<_>>()));
    let near = |a: f32, b: f32, rel: f32| (a - b).abs() <= rel * b;
    // A box: three rectangles, and its own surface.
    let c = of("parts[0].shape");
    assert_eq!((c.what, c.views.len()), ("box", 3));
    assert!(c.views.iter().all(|(p, w)| p.len() == 4 && *w == c.views[0].1) && near(c.area_m2, 22.0, 1e-3), "{c:?}");
    // Three copies of a box of half a metre: three times its surface, and one record.
    let p = of("parts[1].shape");
    assert!(near(p.area_m2, 3.0 * 1.5, 1e-3), "three copies: {}", p.area_m2);
    // A ball scaled by two has four times the surface; its outline is a circle still.
    let b = of("parts[2].shape");
    assert_eq!((b.what, b.views.len(), b.views[0].0.len()), ("sphere", 1, 48));
    assert!(near(b.area_m2, 4.0 * std::f32::consts::PI, 0.03), "a ball of radius 1 as built: {}", b.area_m2);
    // A cone: its section (twenty-four sides: a circle) and a triangle, the triangle twice.
    let s = of("parts[3].shape");
    assert_eq!((s.what, s.views.len(), s.views[1].0.len()), ("cone", 2, 3));
    assert!(near(s.views[1].1, 2.0 * s.views[0].1, 1e-5), "{:?}", s.views.iter().map(|v| v.1).collect::<Vec<_>>());
    // A box with a hole cut through it: the box is recorded, the cylinder that cut it is not,
    // and the record's surface is what is left of the box (its faces less the two discs). The
    // hole's wall is the cylinder's surface, and no record speaks for it.
    let h = of("parts[4].shape.of[0]");
    assert!(h.what == "box" && near(h.area_m2, 24.0 - 2.0 * std::f32::consts::PI * 0.16, 0.02), "{h:?}");
    assert!(!made.iter().any(|m| m.path == "parts[4].shape.of[1]"), "what a difference takes away is not what the thing is made of");
    // A sculpt: its ball's three circles; the box it is cut by adds nothing.
    let d = of("parts[5].shape");
    assert_eq!((d.what, d.views.len()), ("sdf", 3));
    assert!(d.views.iter().all(|(p, _)| p.len() == 48), "{:?}", d.views.iter().map(|v| v.0.len()).collect::<Vec<_>>());
    // A lathe: its section (ten sides) and its profile mirrored.
    let u = of("parts[6].shape");
    assert_eq!((u.what, u.views[0].0.len(), u.views[1].0.len()), ("lathe", 10, 6));
    // Seven shapes in all, the largest first, and all the model's surface spoken for but the
    // walls the two cuts made (the hole's, 2.5 m2 a side of 0.4 m by 2 m; the bead's slot).
    assert_eq!(made.len(), 7, "{:?}", made.iter().map(|m| m.path.as_str()).collect::<Vec<_>>());
    assert!(made.windows(2).all(|w| w[0].area_m2 >= w[1].area_m2));
    let spoken: f32 = made.iter().map(|m| m.area_m2).sum();
    let wall = 2.0 * std::f32::consts::PI * 0.4 * 2.0;
    assert!(model.extras.made_rest_m2 >= 0.97 * wall && model.extras.made_rest_m2 <= wall + 0.6, "of {spoken} m2, {} is spoken for by nothing", model.extras.made_rest_m2);
    // The same recipe says the same.
    assert_eq!(build_with(&text, &[]).unwrap().extras.made, *made);
}

/// A build asks every triangle of a sculpt of up to 16,384 which primitive made it, and every
/// other one of 16,385: held by what the build's own asking does, not by the constant.
#[test]
fn a_build_asks_up_to_its_cap() {
    use crate::sdf::{Combine, Kind, Modifiers, Prim, SdfNode};
    let one = |x: f32| SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.5 }), m: Modifiers { at: [x, 0.0, 0.0], ..Modifiers::default() } };
    let ball = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![one(0.0), one(5.0)], smooth: 0.0 }, m: Modifiers::default() };
    let at = |n: usize| Shape { positions: vec![[0.5, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 0.5]], colors: vec![[1.0; 4]; 3], tris: vec![[0, 1, 2]; n], tags: vec![0; n], ..Shape::default() };
    for (n, asked) in [(16_384usize, 16_384usize), (16_385, 8_193)] {
        let tally = Tally::default();
        let a = tally.ask_owners(&ball, 1, &at(n));
        assert_eq!(a.asked, asked, "a sculpt of {n} triangles");
        assert!(tally.owner_work.get() == a.work && a.work > 0);
    }
}

/// While a model is built, at most 512 shapes are recorded and 8,192 slots of outlines kept
/// (a sculpt has one for each primitive): a shape past them is no record's, and its triangles
/// say so.
#[test]
fn the_tallys_record_is_bounded() {
    let tally = Tally::default();
    let views = || vec![(made::circle(1.0), 1.0)];
    for i in 0..600 {
        let mut s = Shape::cuboid([1.0; 3], 0.0).unwrap();
        tally.making.replace(Some(("box", views())));
        tally.made_now(&format!("parts[{i}].shape"), &mut s);
        assert_eq!(s.owners.is_empty(), i >= 512, "shape {i}");
    }
    assert_eq!(tally.made.borrow().len(), 512);
    // (Sculpts of 127 primitives: 64 fill 8,128 slots, the 65th 64 of its 127.)
    let tally = Tally::default();
    let mut ball = Shape::cuboid([1.0; 3], 0.0).unwrap();
    let of: Vec<u32> = (0..ball.tris.len() as u32).map(|t| t * 10).collect();
    for i in 0..66 {
        tally.made_sculpt(&format!("parts[{i}].shape"), "sdf", (0..127).map(|_| Some(views())).collect(), Vec::new(), &of, &mut ball);
    }
    assert_eq!(tally.slots.borrow().len(), 8192);
    assert_eq!(tally.made.borrow().len(), 65, "the 66th sculpt has no slot left");
    assert_eq!(ball.owners.iter().filter(|&&o| o != crate::shape::NO_OWNER).count(), 0, "{:?}", ball.owners);
}
