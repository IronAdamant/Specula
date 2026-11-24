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
    assert!(err("exemplar_get", r#"{"name": "box_creature", "seed": 9007199254740992}"#).contains("seed"));
    // Any seed a scatter writes can be previewed (its variant seeds are folded to 53 bits).
    let top = ok("exemplar_get", r#"{"name": "box_creature", "seed": 9007199254740991}"#);
    assert!(top.get("specimen").is_some(), "{top}");
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

/// `exemplar_take {"as"}` copies the kind under another name and records the library kind's
/// name as what its streams are keyed on, so seed N of the copy is the specimen `exemplar_get`
/// showed at seed N (review 1, finding 3; the review's case: seed 7 of `box_creature` as
/// `my_beast`).
#[test]
fn a_take_under_another_name_builds_the_specimen_the_library_showed() {
    let lib = library::entry("box_creature").unwrap().resolved().unwrap();
    let c = copy_of("box_creature", Some("my_beast")).unwrap();
    let k = Kind::parse(&c.body).unwrap();
    assert_eq!((k.name.as_str(), k.streams()), ("my_beast", "box_creature"));
    let r = specula_exemplaria::resolve(&k, None).unwrap();
    for seed in [1, 7, 42, 999_999] {
        let (a, b) = (lib.draw(Some(seed)), r.draw(Some(seed)));
        assert_eq!((&a.size, &a.numbers, &a.traits), (&b.size, &b.numbers, &b.traits), "seed {seed}");
    }
    let shown = ok("exemplar_get", r#"{"name": "box_creature", "seed": 7}"#);
    let drawn = shown.get("specimen").and_then(|s| s.get("drawn")).and_then(Json::as_str).unwrap().to_string();
    let mine: Vec<String> = std::iter::once(&r.draw(Some(7)).size).chain(&r.draw(Some(7)).numbers).map(|v| format!("{}={}", v.name, v.value)).collect();
    assert_eq!(drawn, mine.join(" "), "what exemplar_get showed is what the copy draws");
}

/// The fix of a refused take names every role the specimen is written in: the kind's own, its
/// base's and the plan's defaults (review 1, finding 4), each with the hint the write hands the
/// style.
#[test]
fn the_roles_a_take_must_resolve_are_every_role_the_specimen_is_written_in() {
    let lib = library::entry("box_creature").unwrap().resolved().unwrap();
    let mut bare = lib.clone();
    bare.kind.roles.clear();
    let roles = roles_written(&bare);
    assert_eq!(roles.keys().map(String::as_str).collect::<Vec<_>>(), ["claw", "coat", "eye"], "the plan's roles, though the kind names none");
    assert_eq!(roles["claw"], "#2a2420", "the plan's hint");
    // And the take's own check builds a style of exactly those.
    assert!(checked_at_take(&bare, "t").is_ok());
}

/// A new project's style (`specula new`) names every role word of the plans, so a first take
/// needs no patch; a role outside it is still refused with its fix.
#[test]
fn a_new_projects_style_resolves_the_library_kinds_roles() {
    let style = Style::parse(crate::scaffold::STYLE).unwrap();
    for e in library::ENTRIES {
        check_kind(e.text, &|_| None, &style, e.path).unwrap_or_else(|x| panic!("{}: {x}", e.path));
    }
    for role in ["coat", "underside", "eye", "claw", "bark", "leaf", "hull", "panel", "glass"] {
        assert!(style.role(role, Some("#808080")).is_ok(), "{role}");
    }
    for p in specula_exemplaria::plans::PLANS {
        for r in p.roles() {
            assert!(style.role(r.name, Some(r.hint)).is_ok(), "{}: the role {} is in a new project's style", p.name(), r.name);
        }
    }
}

/// A row of the catalogue carries the scatter snippet and each number's mean, range over seeds
/// and unit (EXEMPLARIA.md, A.9), within the page's ceiling.
#[test]
fn a_row_says_each_numbers_mean_range_and_unit_and_the_scatter_snippet() {
    let rows = ok("exemplar_list", r#"{"rows": true, "limit": 100}"#);
    let row = &rows.get("kinds").unwrap().as_array().unwrap()[0];
    let numbers = row.get("numbers").and_then(Json::as_str).unwrap();
    assert!(numbers.starts_with("shoulder_height 0.6 (0.475..0.725) m; body_length 0.72 ("), "{numbers}");
    assert_eq!(numbers.matches(" m").count(), 10, "the size and nine numbers: {numbers}");
    let scatter = row.get("use").and_then(|u| u.get("scatter")).and_then(Json::as_str).unwrap();
    assert!(scatter.contains(r#"{"kind": "box_creature", "variants": 32}"#), "{scatter}");
    assert!(rows.to_string().len() <= MAX_ROWS_BYTES + 512);
}

/// A kind written into a project with an impossible body is refused by the project's checks,
/// naming the kind's field (the review's live case, offline).
#[test]
fn a_project_write_of_an_impossible_kind_names_the_kinds_field() {
    let bad = r#"{"kind": 1, "name": "bad", "plan": "box_creature", "about": "a probe", "size": {"shoulder_height": {"mean": 0.06, "sd": 0.02, "unit": "m", "own": "x"}}}"#;
    let ctx = vec![("assets/style.json".to_string(), crate::project::FileKind::Style, std::sync::Arc::from(crate::scaffold::STYLE))];
    let p = crate::project_kinds::validate("assets/models/bad.kind.json", crate::project::FileKind::Recipe, bad, None, &ctx);
    assert!(p.iter().any(|p| p.message.contains("size.shoulder_height.sd") || format!("{p:?}").contains("size.shoulder_height.sd")), "{p:?}");
}

/// Through the engine a kind refused for its reach carries the worst value and the bound it
/// passes as the recipe error's numbers (AGENTS rule 5: agents debug from data).
#[test]
fn a_kind_refused_for_its_reach_carries_its_numbers_as_a_recipe_error() {
    let bad = r#"{"kind": 1, "name": "bad", "plan": "box_creature", "about": "a probe", "size": {"shoulder_height": {"mean": 0.06, "sd": 0.02, "unit": "m", "own": "x"}}}"#;
    let e = check_kind(bad, &|_| None, &Style::parse(crate::scaffold::STYLE).unwrap(), "t").unwrap_err();
    assert_eq!((e.path.as_str(), e.limit), ("size.shoulder_height.sd", Some(0.039)), "{e:?}");
    assert!(e.measured.is_some_and(|m| (m - 0.01).abs() < 1e-12), "{e:?}");
}

/// Review 2, finding 7: what a tool's answer offers does what it says. A preview at a seed a
/// spawn or a build cannot take offers no snippet for another specimen: it says so and names
/// what does build that one; at the model library's largest seed the snippet is that seed's.
#[test]
fn a_preview_past_the_spawns_seeds_offers_no_snippet_for_another_specimen() {
    let top = ok("exemplar_get", r#"{"name": "box_creature", "seed": 9007199254740991}"#);
    let u = top.get("use").unwrap();
    assert!(u.get("spawn").is_none() && u.get("build").is_none(), "{u}");
    let why = u.get("seed_too_large").and_then(Json::as_str).unwrap();
    assert!(why.contains("0..1000000") && why.contains("9007199254740991") && why.contains("\"full\": true") && why.contains("project_write"), "{why}");
    assert!(u.get("scatter").is_some() && u.get("take").is_some());
    for seed in [0u64, 7, crate::world::MAX_MODEL_SEED] {
        let a = ok("exemplar_get", &format!(r#"{{"name": "box_creature", "seed": {seed}}}"#));
        let u = a.get("use").unwrap();
        for key in ["spawn", "build"] {
            assert_eq!(u.get(key).and_then(|s| s.get("seed")).and_then(Json::as_u64), Some(seed), "{key} at seed {seed}: {u}");
        }
    }
    let over = ok("exemplar_get", &format!(r#"{{"name": "box_creature", "seed": {}}}"#, crate::world::MAX_MODEL_SEED + 1));
    assert!(over.get("use").and_then(|u| u.get("seed_too_large")).is_some(), "{over}");
}

/// Review 2, finding 7: a plan that does not exist is refused with the argument's name as the
/// field, and the nearest plans named (every plan, when none is near the word).
#[test]
fn an_unknown_plan_names_the_argument_and_the_plans() {
    let e = argument_error(&err("exemplar_list", r#"{"plan": "quadruped"}"#));
    assert_eq!(e.get("field").and_then(Json::as_str), Some("plan"), "{e}");
    assert_eq!(e.get("error").and_then(Json::as_str), Some("not_found"), "{e}");
    let m = e.get("message").and_then(Json::as_str).unwrap();
    assert!(m.contains("\"quadruped\"") && m.contains("nearest: none near that word, so every plan: box_creature"), "{m}");
    let e = argument_error(&err("exemplar_list", r#"{"plan": "box_creatur"}"#));
    assert_eq!(e.get("field").and_then(Json::as_str), Some("plan"));
    assert!(e.get("message").and_then(Json::as_str).unwrap().contains("nearest: box_creature"), "{e}");
}

/// Review 2, finding 7: a project's kind whose base is itself is a ring (`base_cycle`), not a
/// base not found; and a kind whose name is not its file's is refused, naming both.
#[test]
fn a_kind_that_is_its_own_base_or_named_otherwise_than_its_file_is_refused() {
    let ctx = vec![("assets/style.json".to_string(), crate::project::FileKind::Style, std::sync::Arc::from(crate::scaffold::STYLE))];
    let validate = |path: &str, text: &str| crate::project_kinds::validate(path, crate::project::FileKind::Recipe, text, None, &ctx);
    let selfbase = r#"{"kind": 1, "name": "selfbase", "plan": "box_creature", "about": "a probe", "base": "selfbase"}"#;
    let p = validate("assets/models/selfbase.kind.json", selfbase);
    assert!(p.iter().any(|p| p.message.contains("base_cycle") && p.message.contains("its own base")), "{p:?}");
    let other = r#"{"kind": 1, "name": "other", "plan": "box_creature", "about": "a probe"}"#;
    let p = validate("assets/models/namemismatch.kind.json", other);
    assert!(p.iter().any(|p| p.message.contains("name_not_file") && p.message.contains("namemismatch.kind.json") && p.message.contains("'other'")), "{p:?}");
    assert!(validate("assets/models/other.kind.json", other).is_empty(), "named as its file: accepted");
    // The build worker and a scatter item hold the same rule.
    let style = Style::parse(STYLE).unwrap();
    let e = expand_for_build(other, &BTreeMap::new(), None, &style, "models/namemismatch.kind.json").unwrap_err();
    assert!(e.message.contains("name_not_file"), "{e:?}");
    use specula_model::Specimens;
    let mut files = BTreeMap::new();
    files.insert("namemismatch.kind".to_string(), other.to_string());
    let e = FolderKinds { library: &files, from: "t" }.specimen("namemismatch", 3, &style).unwrap_err();
    assert!(e.message.contains("name_not_file"), "{e:?}");
}

/// Review 2, finding 1, through the engine: a kind whose seeds could break a relation of its
/// plan is refused by the project's checks, the relation's worst sum and its least as the recipe
/// error's numbers, the relation named in its message.
#[test]
fn a_kind_that_could_cross_its_legs_is_refused_with_the_relations_numbers() {
    let bad = r#"{"kind": 1, "name": "bad", "plan": "box_creature", "about": "a probe", "size": {"shoulder_height": {"mean": 0.6, "sd": 0.02, "unit": "m", "own": "x"}}, "vary": {"build": {"spread": 0.0, "own": "x"}},
        "numbers": {"leg_girth": {"mean": 0.15, "own": "x"}, "body_width": {"mean": 0.05, "own": "x"}, "body_length": {"mean": 0.2, "own": "x"}}}"#;
    let e = check_kind(bad, &|_| None, &Style::parse(crate::scaffold::STYLE).unwrap(), "t").unwrap_err();
    assert_eq!((e.path.as_str(), e.limit), ("numbers.body_width", Some(0.0003)), "{e:?}");
    assert!(e.measured.is_some_and(|m| m < 0.0) && e.message.contains("legs_apart_sideways"), "{e:?}");
}
