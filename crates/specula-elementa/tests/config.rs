//! `elements.json`: what it may say, and what a hostile one is told.

use specula_elementa::config::{MAX_FILE_BYTES, MAX_MATERIALS, MAX_NODES};
use specula_elementa::{Config, Elementa, Limits};

fn refused(json: &str) -> specula_elementa::ElementaError {
    match Config::parse(json) {
        Ok(c) => panic!("accepted {json:?}: {c:?}"),
        Err(e) => e,
    }
}

#[test]
fn the_smallest_file_gives_the_built_in_materials() {
    let c = Config::parse(r#"{"elements": 1}"#).unwrap();
    assert_eq!(c, Config::default());
    let names: Vec<&str> = c.materials.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["cloth", "flesh", "grass", "ice", "metal", "oil", "snow", "stone", "water", "wood"]);
    assert_eq!(c.limits, Limits::DEFAULT);
    assert!(c.material("stone").is_some() && c.materials[usize::from(c.material("stone").unwrap())].ignite.is_none());
}

#[test]
fn a_project_overrides_and_adds_materials_and_maps_tags() {
    let c = Config::parse(
        r#"{"elements": 1,
            "materials": {"wood": {"burn_s": 90}, "peat": {"ignite": 0.3, "burn_s": 300, "heat_out": 0.2}, "stone": {"conducts": true}},
            "tags": {"tree": "wood", "bog": "peat"},
            "cell_m": 1, "reach_m": 0.25, "spread_hz": 5,
            "limits": {"nodes": 20000, "burning": 1000, "work": 65536}}"#,
    )
    .unwrap();
    let wood = &c.materials[usize::from(c.material("wood").unwrap())];
    assert_eq!((wood.burn_s, wood.ignite), (90.0, Some(0.35)), "an override keeps the rest of the built-in row");
    let peat = &c.materials[usize::from(c.material("peat").unwrap())];
    assert!((peat.flame - 0.65).abs() < 1e-6, "a new burning material without a flame burns midway between catching and white hot");
    assert_eq!(c.tag("tree"), c.material("wood"));
    assert_eq!(c.tag("bog"), c.material("peat"));
    assert_eq!(c.limits, Limits { nodes: 20000, burning: 1000, work: 65536 });
    let w = c.worst_case();
    assert_eq!(w.ops_per_step, 20000 + 65536);
    assert_eq!(w.steps_per_second, 5);
    assert!(w.bytes > 0);
    let shown = c.to_json().to_string();
    assert!(shown.contains("\"worst_case\"") && shown.len() < 2048, "{shown}");
    // `null` takes a threshold away.
    let c = Config::parse(r#"{"elements": 1, "materials": {"wood": {"ignite": null}}}"#).unwrap();
    assert_eq!(c.materials[usize::from(c.material("wood").unwrap())].ignite, None);
}

#[test]
fn hostile_files_are_refused_with_a_code_and_a_path() {
    let cases: &[(&str, &str)] = &[
        ("not json", "bad_json"),
        ("[1, 2]", "wrong_type"),
        ("{}", "unsupported_version"),
        (r#"{"elements": 2}"#, "unsupported_version"),
        (r#"{"elements": 1.5}"#, "wrong_type"),
        (r#"{"elements": 1, "fire": true}"#, "unknown_key"),
        (r#"{"elements": 1, "elements": 1}"#, "duplicate"),
        (r#"{"elements": 1, "materials": []}"#, "wrong_type"),
        (r#"{"elements": 1, "materials": {"Wood": {}}}"#, "bad_name"),
        (r#"{"elements": 1, "materials": {"9lives": {}}}"#, "bad_name"),
        (r#"{"elements": 1, "materials": {"wood": {"colour": 1}}}"#, "unknown_key"),
        (r#"{"elements": 1, "materials": {"wood": {"ignite": "hot"}}}"#, "wrong_type"),
        (r#"{"elements": 1, "materials": {"wood": {"ignite": 2}}}"#, "out_of_range"),
        (r#"{"elements": 1, "materials": {"wood": {"ignite": -0.5}}}"#, "out_of_range"),
        (r#"{"elements": 1, "materials": {"wood": {"burn_s": 1e400}}}"#, "out_of_range"),
        (r#"{"elements": 1, "materials": {"wood": {"burn_s": -1}}}"#, "out_of_range"),
        (r#"{"elements": 1, "materials": {"wood": {"heat_out": 1e30}}}"#, "out_of_range"),
        (r#"{"elements": 1, "materials": {"wood": {"flame": 0.2}}}"#, "bad_config"),
        (r#"{"elements": 1, "materials": {"wood": {"burn_s": 0}}}"#, "bad_config"),
        (r#"{"elements": 1, "materials": {"wood": {"freeze": 0.5}}}"#, "out_of_range"),
        (r#"{"elements": 1, "materials": {"wood": {"conducts": 1}}}"#, "wrong_type"),
        (r#"{"elements": 1, "tags": {"tree": "unobtainium"}}"#, "unknown_material"),
        (r#"{"elements": 1, "tags": {"tree": 3}}"#, "wrong_type"),
        (r#"{"elements": 1, "tags": {"tree": "wood", "tree": "wood"}}"#, "duplicate"),
        (r#"{"elements": 1, "cell_m": 0}"#, "out_of_range"),
        (r#"{"elements": 1, "cell_m": 1000}"#, "out_of_range"),
        (r#"{"elements": 1, "cell_m": 1, "reach_m": 2}"#, "out_of_range"),
        (r#"{"elements": 1, "spread_hz": 0}"#, "out_of_range"),
        (r#"{"elements": 1, "spread_hz": 7.5}"#, "wrong_type"),
        (r#"{"elements": 1, "limits": {"nodes": 40000}}"#, "out_of_range"),
        (r#"{"elements": 1, "limits": {"nodes": 0}}"#, "out_of_range"),
        (r#"{"elements": 1, "limits": {"nodes": 100, "burning": 200}}"#, "out_of_range"),
        (r#"{"elements": 1, "limits": {"work": 10}}"#, "out_of_range"),
        (r#"{"elements": 1, "limits": {"work": 1e12}}"#, "out_of_range"),
        (r#"{"elements": 1, "limits": {"threads": 8}}"#, "unknown_key"),
    ];
    for (json, code) in cases {
        let e = refused(json);
        assert_eq!(e.code, *code, "{json}: {e}");
        assert!(!e.message.is_empty());
        assert!(e.to_json().to_string().len() < 1024, "{json}: an error stays short");
    }
    let e = refused(r#"{"elements": 1, "limits": {"nodes": 40000}}"#);
    assert_eq!(e.detail("max").and_then(|v| v.as_u64()), Some(u64::from(MAX_NODES)), "the limit's number is named");
    assert_eq!(e.detail("path").and_then(|v| v.as_str()), Some("limits.nodes"));
}

#[test]
fn size_depth_and_count_are_capped_before_work() {
    let big = format!(r#"{{"elements": 1, "x": "{}"}}"#, "a".repeat(MAX_FILE_BYTES));
    assert_eq!(refused(&big).code, "too_large");
    let deep = format!(r#"{{"elements": 1, "materials": {}{}}}"#, "[".repeat(5000), "]".repeat(5000));
    assert_eq!(refused(&deep).code, "bad_json", "the JSON parser's depth limit holds");
    let many: Vec<String> = (0..MAX_MATERIALS).map(|i| format!(r#""m{i}": {{}}"#)).collect();
    let e = refused(&format!(r#"{{"elements": 1, "materials": {{{}}}}}"#, many.join(",")));
    assert_eq!(e.code, "limit");
    assert_eq!(e.detail("max").and_then(|v| v.as_u64()), Some(MAX_MATERIALS as u64));
    let tags: Vec<String> = (0..1025).map(|i| format!(r#""t{i}": "wood""#)).collect();
    assert_eq!(refused(&format!(r#"{{"elements": 1, "tags": {{{}}}}}"#, tags.join(","))).code, "limit");
    let long = format!(r#"{{"elements": 1, "materials": {{"{}": {{}}}}}}"#, "a".repeat(5000));
    let e = refused(&long);
    assert_eq!(e.code, "bad_name");
    assert!(e.to_json().to_string().len() < 512, "a long name is not echoed whole");
}

#[test]
fn the_tick_rate_must_be_a_multiple_of_the_spread_rate() {
    assert!(Elementa::new(Config::default(), 60, 1).is_ok());
    assert!(Elementa::new(Config::default(), 30, 1).is_ok());
    let e = Elementa::new(Config::default(), 55, 1).unwrap_err();
    assert_eq!(e.code, "bad_config");
    assert_eq!(Elementa::new(Config::default(), 0, 1).unwrap_err().code, "bad_config");
    assert_eq!(Elementa::new(Config::default(), 60, 1).unwrap().ticks_per_step(), 6);
}
