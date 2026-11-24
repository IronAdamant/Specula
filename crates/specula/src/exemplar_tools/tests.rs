//! The catalogue's answers and their ceilings, the nearest names across the lists, the copy a
//! project takes, and a kind read as a recipe (by the model library and the project's checks).

use super::*;

fn j(s: &str) -> Json {
    Json::parse(s).unwrap()
}

fn ok(name: &str, args: &str) -> Json {
    let r = if name == "exemplar_list" { list_tool(&j(args)) } else { get_tool(&j(args)) };
    r.unwrap_or_else(|e| panic!("{name} {args}: {e}"))
}

fn err(name: &str, args: &str) -> String {
    let r = if name == "exemplar_list" { list_tool(&j(args)) } else { get_tool(&j(args)) };
    r.expect_err(args)
}

fn session() -> SessionInfo {
    SessionInfo { id: "exemplar-test".into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
}

const STYLE: &str = r##"{ "materials": { "matte": { "color": "#8c96a3" }, "claw": { "color": "#202020" } }, "roles": { "coat": "matte", "eye": "matte" } }"##;

#[test]
fn the_first_answer_is_short_and_names_the_other_lists() {
    let a = ok("exemplar_list", "{}");
    let text = a.to_string();
    assert!(text.len() <= MAX_INDEX_BYTES, "{} bytes", text.len());
    assert!(text.contains("box_creature") && text.contains("exemplar_get") && text.contains("exemplar_take"), "{text}");
    assert!(text.contains("shapes (formae_list)"), "names the shape library: {text}");
    // (The shape library's first answer does not name this one: its table fills its 3 KB.)
}

#[test]
fn rows_filter_and_page_under_their_ceiling() {
    let rows = ok("exemplar_list", r#"{"rows": true}"#);
    assert_eq!(rows.get("matched").and_then(Json::as_u64), Some(library::ENTRIES.len() as u64));
    assert!(rows.to_string().len() <= MAX_ROWS_BYTES + 512);
    let row = &rows.get("kinds").unwrap().as_array().unwrap()[0];
    assert!(row.get("use").and_then(|u| u.get("spawn")).is_some(), "{row}");
    assert_eq!(ok("exemplar_list", r#"{"role": "claw"}"#).get("matched").and_then(Json::as_u64), Some(1));
    assert_eq!(ok("exemplar_list", r#"{"word": "beast"}"#).get("matched").and_then(Json::as_u64), Some(1), "an alias");
    let none = ok("exemplar_list", r#"{"word": "gearr"}"#);
    assert!(none.get("elsewhere").is_some_and(|e| e.to_string().contains("\"gear\"")), "a word no kind has points at the shapes: {none}");
    assert!(err("exemplar_list", r#"{"plan": "box_creatur"}"#).contains("nearest: box_creature"));
    assert!(err("exemplar_list", r#"{"colour": 1}"#).contains("colour"));
    assert!(err("exemplar_list", r#"{"limit": 101}"#).contains("limit"));
}

#[test]
fn every_name_and_alias_answers_short_at_any_seed_and_full_under_the_ceiling() {
    for (name, _) in library::names() {
        for seed in ["", r#", "seed": 0"#, r#", "seed": 7"#, r#", "seed": 1000000"#] {
            let a = ok("exemplar_get", &format!(r#"{{"name": "{name}"{seed}}}"#));
            let n = a.to_string().len();
            assert!(n <= MAX_SHORT_BYTES, "{name}{seed}: {n} bytes");
            assert!(a.get("use").is_some() && a.get("own").is_some(), "{a}");
            assert_eq!(a.get("specimen").is_some(), !seed.is_empty());
        }
        let full = ok("exemplar_get", &format!(r#"{{"name": "{name}", "seed": 3, "full": true}}"#));
        assert!(full.to_string().len() <= MAX_ANSWER_BYTES);
        let recipe = full.get("recipe").and_then(Json::as_str).unwrap();
        assert!(recipe.contains("seed 3") && full.get("anims").is_some() && full.get("kind_file").is_some());
        assert_eq!(full.get("number_rows").and_then(Json::as_array).map(<[Json]>::len), Some(9));
    }
    assert!(err("exemplar_get", r#"{"name": "box_creature", "seed": 1000001}"#).contains("seed"));
    assert!(err("exemplar_get", "{}").contains("required"));
    let long = format!(r#"{{"name": "{}"}}"#, "x".repeat(65));
    assert!(err("exemplar_get", &long).contains("at most 64"));
}

#[test]
fn a_wrong_name_answers_with_the_nearest_across_the_lists() {
    let e = err("exemplar_get", r#"{"name": "box_creatur"}"#);
    assert!(e.contains("\"box_creature\"") && e.contains("exemplar_get"), "{e}");
    let e = err("exemplar_get", r#"{"name": "torrus"}"#);
    assert!(e.contains("\"torus\"") && e.contains("formae_get"), "a shape's name asked of the kinds: {e}");
    // And the shape library, asked a kind's name, names the kind.
    let f = crate::formae_tools::call(&session(), "formae_get", &j(r#"{"name": "box_creatur"}"#));
    assert!(f.is_error && format!("{:?}", f.content).contains("box_creature"), "{:?}", f.content);
}

#[test]
fn the_copy_a_project_takes_is_one_file_that_validates_and_builds() {
    let c = copy_of("box_beast", None).unwrap();
    assert_eq!((c.path.as_str(), c.as_name.as_str()), ("assets/models/box_creature.kind.json", "box_creature"));
    assert!(c.body.contains(&c.entry.from()) && c.body.ends_with('\n'));
    let k = Kind::parse(&c.body).unwrap();
    assert_eq!((k.base.as_ref(), k.from.as_deref()), (None, Some(c.entry.from().as_str())));
    let renamed = copy_of("box_creature", Some("pack_mule")).unwrap();
    assert_eq!(renamed.path, "assets/models/pack_mule.kind.json");
    assert!(Kind::parse(&renamed.body).unwrap().name == "pack_mule");
    assert!(copy_of("box_creature", Some("Pack Mule")).is_err());
    assert!(copy_of("nothing_here", None).unwrap_err().contains("no kind is called"));
    // The project's checks read it as the model library will.
    let style = Style::parse(STYLE).unwrap();
    let files = BTreeMap::new();
    let mean = check_kind(&c.body, &|_| None, &style, &c.path).unwrap();
    specula_model::Recipe::parse(&mean, &style).unwrap();
    let (recipe, anims) = expand_for_build(&c.body, &files, Some(5), &style, "recipes/box_creature.kind.json").unwrap();
    let model = specula_model::build(&recipe, &specula_model::BuildParams { seed: Some(5), ..specula_model::BuildParams::new(&style) }).unwrap();
    assert!(model.rig.is_some() && anims.is_some());
    let ctx = vec![("assets/style.json".to_string(), crate::project::FileKind::Style, std::sync::Arc::from(STYLE))];
    assert!(crate::project_kinds::validate(&c.path, crate::project::FileKind::Recipe, &c.body, None, &ctx).is_empty());
    // A style that can't draw a role refuses the write, naming the role and the nearest presets.
    let bare = r##"{ "materials": { "matte": { "color": "#8c96a3" } } }"##;
    let ctx = vec![("assets/style.json".to_string(), crate::project::FileKind::Style, std::sync::Arc::from(bare))];
    let p = crate::project_kinds::validate(&c.path, crate::project::FileKind::Recipe, &c.body, None, &ctx);
    assert!(p.iter().any(|p| p.message.contains("role") && p.message.contains("matte")), "{p:?}");
}

#[test]
fn a_kind_with_a_base_beside_it_reads_over_it() {
    let style = Style::parse(STYLE).unwrap();
    let base = library::entry("box_creature").unwrap().text.to_string();
    let child = r#"{"kind": 1, "name": "tall_box", "plan": "box_creature", "about": "long legs", "base": "box_creature",
        "numbers": {"leg_length": {"share": 0.8, "own": "a probe"}}}"#;
    let mut files = BTreeMap::new();
    files.insert("box_creature.kind".to_string(), base.clone());
    let (recipe, _) = expand_for_build(child, &files, None, &style, "t").unwrap();
    assert!(recipe.contains("leg_length 0.48 m") && recipe.contains("tail_length"), "the base's numbers under the kind's: {recipe}");
    let e = expand_for_build(child, &BTreeMap::new(), None, &style, "t").unwrap_err();
    assert_eq!(e.code, ErrorCode::UnknownAsset, "{e:?}");
    // The project's checks find the base beside it.
    let ctx = vec![
        ("assets/style.json".to_string(), crate::project::FileKind::Style, std::sync::Arc::from(STYLE)),
        ("assets/recipes/box_creature.kind.json".to_string(), crate::project::FileKind::Recipe, std::sync::Arc::from(base.as_str())),
    ];
    assert!(crate::project_kinds::validate("assets/recipes/tall_box.kind.json", crate::project::FileKind::Recipe, child, None, &ctx).is_empty());
    assert!(is_kind("recipes/a.kind.json") && is_kind("a.kind") && !is_kind("recipes/a.json"));
}

#[test]
fn every_argument_is_bounded() {
    for d in definitions() {
        let props = d.input_schema.get("properties").and_then(Json::as_object).unwrap();
        for (k, v) in props {
            let ty = v.get("type").and_then(Json::as_str).unwrap();
            let bounded = match ty {
                "string" => v.get("maxLength").is_some(),
                "integer" => v.get("maximum").is_some(),
                "boolean" => true,
                _ => false,
            };
            assert!(bounded, "{}.{k} has no limit", d.name);
        }
        assert!(crate::authority::access(&d.name).is_some(), "{} declares its access", d.name);
    }
}

/// A scatter item that names a kind builds a specimen at each variant's seed (EXEMPLARIA.md,
/// A.2: "a scatter item's variant seed"): 32 variants are 32 different recipes, the same
/// recipe built twice is the same model, and the item is bounded as a scatter's are.
#[test]
fn a_scatter_of_a_kind_is_a_crowd_of_specimens_and_the_same_twice() {
    use specula_model::Specimens;
    /// Records every specimen written, as the build asks for them.
    #[derive(Debug)]
    struct Seen<'a>(FolderKinds<'a>, std::sync::Mutex<Vec<String>>);
    impl Specimens for Seen<'_> {
        fn specimen(&self, name: &str, seed: u64, style: &Style) -> Result<String, RecipeError> {
            let t = self.0.specimen(name, seed, style)?;
            self.1.lock().unwrap().push(t.clone());
            Ok(t)
        }
    }
    let style = Style::parse(STYLE).unwrap();
    let mut files = BTreeMap::new();
    files.insert("box_creature.kind".to_string(), library::entry("box_creature").unwrap().text.to_string());
    let herd = |variants: u32| {
        format!(
            r#"{{"name": "herd", "parts": [{{"name": "herd", "material": "matte", "generate": {{"op": "scatter", "area": [[-20, -20], [20, -20], [20, 20], [-20, 20]], "min_distance": 2.5, "count": 64,
                "items": [{{"kind": "box_creature", "variants": {variants}}}]}}}}]}}"#
        )
    };
    let text = herd(specula_model::recipe::MAX_KIND_VARIANTS);
    let build = |text: &str| {
        let seen = Seen(FolderKinds { library: &files, from: "recipes/herd.json" }, std::sync::Mutex::new(Vec::new()));
        let model = specula_model::build(text, &specula_model::BuildParams { library: Some(&files), specimens: Some(&seen), ..specula_model::BuildParams::new(&style) }).unwrap_or_else(|e| panic!("{e}"));
        (model.hash, seen.1.into_inner().unwrap())
    };
    let (hash, written) = build(&text);
    // (A build may make its parts more than once, a coarse pass among them: each time the same.)
    assert!(!written.is_empty() && written.len() % 32 == 0, "every variant a specimen: {}", written.len());
    let unlike: std::collections::BTreeSet<&String> = written.iter().collect();
    assert_eq!(unlike.len(), 32, "32 seeds, 32 different recipes");
    let (again, written_again) = build(&text);
    assert_eq!((hash, &written), (again, &written_again), "the same recipe twice is the same crowd");
    // Bounded: one variant past the most is refused, a kind beside no recipe is named with the
    // nearest, a kind takes no params, and a build without the engine's specimens refuses it.
    let over = specula_model::build(&herd(33), &specula_model::BuildParams { library: Some(&files), specimens: Some(&FolderKinds { library: &files, from: "t" }), ..specula_model::BuildParams::new(&style) });
    assert!(over.is_err_and(|e| e.path.contains("variants")));
    let wrong = text.replace("\"kind\": \"box_creature\"", "\"kind\": \"box_creatur\"");
    let e = specula_model::build(&wrong, &specula_model::BuildParams { library: Some(&files), specimens: Some(&FolderKinds { library: &files, from: "t" }), ..specula_model::BuildParams::new(&style) }).unwrap_err();
    assert!(e.code == ErrorCode::UnknownAsset && e.message.contains("nearest: box_creature") && e.path.contains("items[0].kind"), "{e:?}");
    let with_params = text.replace("\"variants\"", "\"params\": {\"x\": [0, 1]}, \"variants\"");
    assert!(specula_model::build(&with_params, &specula_model::BuildParams { library: Some(&files), specimens: Some(&FolderKinds { library: &files, from: "t" }), ..specula_model::BuildParams::new(&style) }).is_err_and(|e| e.path.contains("params")));
    let alone = specula_model::build(&text, &specula_model::BuildParams { library: Some(&files), ..specula_model::BuildParams::new(&style) }).unwrap_err();
    assert_eq!(alone.code, ErrorCode::UnknownAsset, "{alone:?}");
    // The recipe names the kind as what it includes, so the model library reads its file.
    assert_eq!(specula_model::Recipe::parse(&text, &style).unwrap().includes(), ["box_creature.kind"]);
}
