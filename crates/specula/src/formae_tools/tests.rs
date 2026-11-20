//! The shape library's tools and items, tested in-process.

use super::*;

fn j(text: &str) -> Json {
    Json::parse(text).unwrap()
}

fn ok(name: &str, args: &str) -> Json {
    let r = if name == "formae_list" { list_tool(&j(args)) } else { get_tool(&j(args)) };
    r.unwrap_or_else(|e| panic!("{name} {args}: {e}"))
}

/// The catalogue lists every form, page by page, under its ceiling, and the figure with the
/// modeller's own fields for it.
#[test]
fn the_catalogue_lists_every_form_and_the_figure() {
    let all: Vec<String> = formae::forms().iter().map(|f| f.name.to_string()).collect();
    // The first answer: every name, by kind and family, under its small ceiling, the figure
    // named and how to ask; no rows and no figure's fields.
    let first = ok("formae_list", "{}");
    let text = first.to_string();
    assert!(text.len() <= MAX_INDEX_BYTES, "the first answer is {} bytes", text.len());
    assert!(first.get("forms").is_none() && first.get("figures").is_none(), "{text}");
    // (Every form is named, or counted on its family's line: see `index_holds_any_table`.)
    let rows: Vec<(Kind, &str, &str)> = formae::forms().iter().map(|f| (f.kind, f.family.name(), f.name)).collect();
    check_index(&first, &rows);
    assert!(text.contains("figure") && text.contains("formae_get"), "{text}");
    // Rows, every one, page by page under their ceiling, each with what pastes.
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..100 {
        let args = match &cursor {
            None => r#"{"rows": true, "limit": 200}"#.to_string(),
            Some(c) => format!(r#"{{"limit": 200, "cursor": "{c}"}}"#),
        };
        let a = ok("formae_list", &args);
        assert!(a.to_string().len() <= MAX_ROWS_BYTES + 1024, "a page of {} bytes", a.to_string().len());
        for r in a.get("forms").and_then(Json::as_array).unwrap() {
            assert!(r.get("recipe_shape").is_some() || r.get("shape2d_item").is_some(), "{r}");
            seen.push(r.get("name").and_then(Json::as_str).unwrap().to_string());
        }
        assert!(a.get("figures").is_none());
        cursor = a.get("next_cursor").and_then(Json::as_str).map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen, all);
    // The figure's fields, when asked for: the modeller's own.
    for args in [r#"{"figures": true}"#, r#"{"word": "heart", "figures": true}"#] {
        let a = ok("formae_list", args);
        let fig = a.get("figures").and_then(Json::as_array).unwrap();
        let fields = fig[0].get("fields").and_then(Json::as_array).unwrap();
        let names: Vec<&str> = fields.iter().filter_map(|f| f.get("name").and_then(Json::as_str)).collect();
        let want: Vec<&str> = specula_model::shape_fields("figure").unwrap().iter().map(|f| f.0).collect();
        assert!(!want.is_empty() && names == want, "{names:?} against {want:?}");
    }
    let solids = ok("formae_list", r#"{"kind": "solid", "family": "platonic"}"#);
    let rows = solids.get("forms").and_then(Json::as_array).unwrap();
    assert!(!rows.is_empty() && rows.iter().all(|r| r.get("kind").and_then(Json::as_str) == Some("solid")));
    let hearts = ok("formae_list", r#"{"word": "heart"}"#);
    assert!(hearts.get("forms").and_then(Json::as_array).unwrap().iter().any(|r| r.get("name").and_then(Json::as_str) == Some("heart")));
    assert!(list_tool(&j(r#"{"family": "nope"}"#)).is_err());
    assert!(list_tool(&j(r#"{"limit": 1000}"#)).is_err());
    assert!(list_tool(&j(r#"{"colour": "red"}"#)).is_err());
}

/// `formae_get` of every form and alias answers under its ceiling with its parameters, counts
/// and snippets; an unknown word answers with the nearest; a count past what it builds is
/// answered without the measures as built.
#[test]
fn every_form_is_answered_under_its_ceiling() {
    let mut words: Vec<(String, &str)> = formae::forms().iter().map(|f| (f.name.to_string(), f.kind.name())).collect();
    words.extend(formae::aliases().iter().map(|a| (a.name.to_string(), a.kind.name())));
    for (w, k) in &words {
        for detail in [8, 64] {
            let a = ok("formae_get", &format!(r#"{{"name": "{w}", "kind": "{k}", "detail": {detail}, "full": true}}"#));
            assert!(a.to_string().len() <= MAX_ANSWER_BYTES, "{w}");
            assert!(a.get("params").is_some() && a.get("count").is_some() && a.get("snippets").is_some() && a.get("kin").is_some(), "{w}: {a}");
        }
        // Short, by default: the snippets first, and nothing that needs a build.
        let a = ok("formae_get", &format!(r#"{{"name": "{w}", "kind": "{k}"}}"#));
        let text = a.to_string();
        assert!(text.len() <= MAX_SHORT_BYTES, "{w}: {} bytes", text.len());
        assert!(a.get("snippets").is_some() && a.get("args").is_some() && a.get("kin").is_none() && a.get("built").is_none(), "{w}: {a}");
        assert!(text.find("\"snippets\"") < text.find("\"args\""), "{w}: the snippets come first");
    }
    let e = get_tool(&j(r#"{"name": "icosahedrn"}"#)).unwrap_err();
    assert!(e.contains("icosahedron"), "{e}");
    let e = get_tool(&j(r#"{"name": "torus", "args": {"tub": 1}}"#)).unwrap_err();
    assert!(e.contains("tube"), "{e}");
    let big = ok("formae_get", r#"{"name": "sphere", "detail": 4096, "full": true}"#);
    assert!(big.get("built").and_then(|b| b.get("not_built")).is_some(), "{big}");
}

/// Measures as built and in closed form are the library's: a ball's volume.
#[test]
fn a_balls_measures_are_the_librarys() {
    let a = ok("formae_get", r#"{"name": "sphere", "args": {"radius": 2}, "detail": 64, "full": true}"#);
    let closed = a.get("closed").and_then(|c| c.get("size")).and_then(Json::as_f64).unwrap();
    assert!((closed - 4.0 / 3.0 * std::f64::consts::PI * 8.0).abs() < 1e-9, "{closed}");
    let built = a.get("built").and_then(|b| b.get("measures")).and_then(|m| m.get("size")).and_then(Json::as_f64).unwrap();
    assert!(built < closed && built > 0.98 * closed, "{built} against {closed}");
}

/// The 2D items by a form's name: a heart filled; an annulus with its hole; a curve stroked and
/// refused a fill; a solid drawn, its faces shaded; a solid in layers; refusals as data; the
/// canonical description is the name and the numbers, and parses back to itself.
#[test]
fn shape2d_items_by_name() {
    use crate::two_d::Shape2dSpec;
    let parse = |t: &str| Shape2dSpec::parse(&j(t));
    let heart = parse(r##"{"items": [{"form": "heart", "detail": 32, "at": [1, 2], "scale": 0.5, "fill": "#ff0000"}]}"##).unwrap();
    assert!(heart.mesh().is_some_and(|m| !m.indices.is_empty()));
    assert_eq!(heart.json.to_string(), parse(&heart.json.to_string()).unwrap().json.to_string());
    let canon = heart.json.to_string();
    assert!(canon.contains("\"form\":\"heart\"") && canon.contains("\"detail\":32") && !canon.contains("points"), "{canon}");
    let ring = parse(r##"{"items": [{"form": "annulus", "fill": "#00ff00"}]}"##).unwrap();
    assert!(ring.mesh().is_some_and(|m| !m.indices.is_empty()));
    assert!(parse(r##"{"items": [{"form": "sine_wave", "stroke": {"width": 0.02}}]}"##).is_ok());
    let e = parse(r##"{"items": [{"form": "sine_wave", "fill": "#ffffff"}]}"##).map(|_| ()).unwrap_err();
    assert!(e.contains("open"), "{e}");
    let drawn = parse(r##"{"items": [{"solid": "icosahedron", "view": "isometric", "fill": "#8c96a3"}]}"##).unwrap();
    assert!(drawn.mesh().is_some_and(|m| !m.indices.is_empty()));
    let layered = parse(r##"{"items": [{"solid": "torus", "detail": 16, "layers": {"count": 6, "axis": "y", "step": [0, 0.05]}, "fill": "#8c96a3"}]}"##).unwrap();
    assert!(layered.mesh().is_some_and(|m| !m.indices.is_empty()));
    assert_eq!(layered.json.to_string(), parse(&layered.json.to_string()).unwrap().json.to_string());
    let circle = r##"{"form": "circle", "detail": 4096, "fill": "#fff"}"##;
    let many = format!(r#"{{"items": [{}]}}"#, [circle; 5].join(", "));
    for (bad, says) in [
        (r##"{"items": [{"form": "hart", "fill": "#fff"}]}"##, "heart"),
        (r##"{"items": [{"form": "heart", "args": {"wide": 1}, "fill": "#fff"}]}"##, "takes"),
        (r##"{"items": [{"form": "heart", "detail": 5000, "fill": "#fff"}]}"##, "detail"),
        (r##"{"items": [{"solid": "sphere", "fill": "#fff"}]}"##, "view"),
        (r##"{"items": [{"solid": "sphere", "detail": 4096, "view": "top", "fill": "#fff"}]}"##, "lower detail"),
        (r##"{"items": [{"rect": [0, 0, 1, 1], "detail": 9, "fill": "#fff"}]}"##, "form's name"),
        (many.as_str(), "left"),
        (r##"{"items": [{"solid": "cube", "view": "isometric", "rotate": 30, "fill": "#fff"}]}"##, "view"),
    ] {
        let e = parse(bad).map(|_| ()).unwrap_err();
        assert!(e.contains(says), "{bad}: {e}");
    }
}

/// The dearest item the limits allow is answered: each solid at the finest detail a 2D item
/// takes, drawn from the front (where most are not convex there is a pair search) and cut into
/// the most layers. What it did is said in the assertion's count, never a time.
#[test]
fn the_dearest_items_are_built_within_their_limits() {
    use crate::two_d::Shape2dSpec;
    let mut built = 0;
    for f in formae::forms().iter().filter(|f| f.kind == Kind::Solid) {
        let spec = Spec::of(Found { form: f, alias: None }, &[]).unwrap();
        let best = [8u32, 16, 32, 64, 128, 256].into_iter().rfind(|d| spec.count(*d).is_ok_and(|c| c.points <= MAX_ITEM_SOLID_POINTS && c.parts <= MAX_DRAW_FACES));
        let Some(d) = best else { continue };
        for how in [r#""view": "front""#.to_string(), format!(r#""layers": {{"count": {MAX_ITEM_LAYERS}}}"#)] {
            let text = format!(r##"{{"items": [{{"solid": "{}", "detail": {d}, {how}, "fill": "#8c96a3"}}]}}"##, f.name);
            match Shape2dSpec::parse(&j(&text)) {
                Ok(_) => built += 1,
                Err(e) => assert!(e.contains("left") || e.contains("code"), "{text}: {e}"),
            }
        }
    }
    assert!(built > 100, "{built} of the dearest items built");
}

/// The library is pointed at where a cold agent already looks (stage 3b): `model_build`'s
/// description, the modelling family's line in the catalog, and the library's own catalog lines
/// say how to ask for it. (The project's guide and the first call's brief are held in
/// `scaffold.rs`'s tests, and the modelling document by the library's `doc_table` test.)
#[test]
fn the_library_is_pointed_at_where_an_agent_looks() {
    let defs = crate::tools::definitions();
    let build = defs.iter().find(|d| d.name == "model_build").expect("model_build");
    assert!(build.description.contains("formae_list"), "{}", build.description);
    let model = crate::toolscope::FAMILIES.iter().find(|f| f.name == "model").expect("the model family");
    assert!(model.about.contains("formae_list"), "{}", model.about);
}

/// Every tool argument has a limit in its schema.
#[test]
fn the_tools_are_defined_with_bounds() {
    let defs = definitions();
    assert_eq!(defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["formae_list", "formae_get"]);
    for d in &defs {
        let props = d.input_schema.get("properties").and_then(Json::as_object).unwrap();
        for (name, prop) in props {
            let bounded = match prop.get("type").and_then(Json::as_str).unwrap() {
                "string" => prop.get("maxLength").is_some(),
                "integer" => prop.get("maximum").is_some(),
                "object" => prop.get("maxProperties").is_some(),
                _ => true,
            };
            assert!(bounded, "{}.{name} has no limit", d.name);
        }
    }
}

/// The area a 2D item by name fills is its figure's as built (an annulus's hole is a hole), and
/// a drawn solid's faces are shaded: more than one colour, each the item's darkened.
#[test]
fn an_items_fill_is_its_figure_and_its_faces_are_shaded() {
    use crate::two_d::Shape2dSpec;
    let area = |s: &Shape2dSpec| {
        let m = s.mesh().unwrap();
        m.indices.as_chunks::<3>().0.iter().map(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.vertices[i as usize].position);
            0.5 * f64::from((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
        }).sum::<f64>()
    };
    for word in ["annulus", "heart", "gear", "frame"] {
        let s = Shape2dSpec::parse(&j(&format!(r##"{{"items": [{{"form": "{word}", "detail": 64, "fill": "#808080"}}], "aa": false}}"##))).unwrap();
        let want = Spec::new(word, Some(Kind::Plane), &[]).unwrap().build(64).unwrap().measures().size.unwrap();
        let got = area(&s);
        assert!((got - want).abs() <= 1e-4 * want, "{word}: filled {got}, built {want}");
    }
    let drawn = Shape2dSpec::parse(&j(r##"{"items": [{"solid": "icosahedron", "view": "isometric", "fill": "#ffffff"}], "aa": false}"##)).unwrap();
    let mut colours: Vec<[u8; 4]> = drawn.mesh().unwrap().vertices.iter().map(|v| v.color).collect();
    colours.sort_unstable();
    colours.dedup();
    assert!(colours.len() > 2 && colours.iter().all(|c| c[0] < 255 && c[0] >= 63), "{colours:?}");
}

/// Reads a first answer back and holds it to `rows`: under its ceiling; every kind's families
/// once each, in the table's order, each line either every name of its family or its count,
/// its first names and how many more (`"family (N): a b +M more"`), or, when not a line a
/// family fits, one line a kind with its counts of forms and families.
fn check_index(answer: &Json, rows: &[(Kind, &str, &str)]) {
    let text = answer.to_string();
    assert!(text.len() <= MAX_INDEX_BYTES, "the first answer is {} bytes of {MAX_INDEX_BYTES}", text.len());
    assert!(text.contains("figure") && text.contains("formae_get") && text.contains("formae_list"), "{text}");
    for kind in [Kind::Plane, Kind::Curve, Kind::Solid] {
        let mut families: Vec<(&str, Vec<&str>)> = Vec::new();
        for &(_, family, name) in rows.iter().filter(|r| r.0 == kind) {
            match families.iter_mut().find(|f| f.0 == family) {
                Some(f) => f.1.push(name),
                None => families.push((family, vec![name])),
            }
        }
        let lines: Vec<&str> = answer.get(kind.name()).and_then(Json::as_array).unwrap().iter().map(|l| l.as_str().unwrap()).collect();
        let forms: usize = families.iter().map(|f| f.1.len()).sum();
        if lines.len() == 1 && lines[0].starts_with(&format!("{forms} forms in {} families: ", families.len())) && !families.is_empty() {
            continue;
        }
        assert_eq!(lines.len(), families.len(), "{}: a line a family: {lines:?}", kind.name());
        for (line, (family, names)) in lines.iter().zip(&families) {
            if let Some(all) = line.strip_prefix(&format!("{family}: ")) {
                assert_eq!(all.split(' ').collect::<Vec<_>>(), *names, "{line}");
            } else {
                let cut = line.strip_prefix(&format!("{family} ({}):", names.len())).unwrap_or_else(|| panic!("{line}"));
                let words: Vec<&str> = cut.split(' ').filter(|w| !w.is_empty()).collect();
                let (more, shown) = words.split_last().unwrap();
                let more: usize = more.strip_prefix('+').unwrap_or_else(|| panic!("{line}")).parse().unwrap();
                assert!(shown.len() + more == names.len() && *shown == names[..shown.len()] && more > 0, "{line}");
            }
        }
    }
}

/// **The first answer holds whatever is added to the library**: a made-up table twice the size
/// of the library's (each family twice as long), one with twice as many families, and one of
/// two hundred families of fifty forms each, all answer under the ceiling, every family named or
/// counted (`check_index`); and the library's own table, while it fits, names every form.
#[test]
fn index_holds_any_table() {
    let real: Vec<(Kind, &str, &str)> = formae::forms().iter().map(|f| (f.kind, f.family.name(), f.name)).collect();
    let names: Vec<String> = real.iter().map(|r| format!("{}_twin", r.2)).collect();
    let families: Vec<String> = real.iter().map(|r| format!("{}_twin", r.1)).collect();
    let mut longer = real.clone();
    longer.extend(real.iter().zip(&names).map(|(r, n)| (r.0, r.1, n.as_str())));
    let mut wider = real.clone();
    wider.extend(real.iter().zip(&names).zip(&families).map(|((r, n), f)| (r.0, f.as_str(), n.as_str())));
    let many_names: Vec<String> = (0..200 * 50).map(|i| format!("made_up_form_{i}")).collect();
    let many_families: Vec<String> = (0..200).map(|i| format!("family_{i}")).collect();
    let huge: Vec<(Kind, &str, &str)> = many_names.iter().enumerate().map(|(i, n)| ([Kind::Plane, Kind::Curve, Kind::Solid][i % 3], many_families[i / 50].as_str(), n.as_str())).collect();
    for (what, rows) in [("the library", &real), ("each family twice as long", &longer), ("twice the families", &wider), ("200 families of 50", &huge)] {
        let a = index_of(rows, 79);
        check_index(&a, rows);
        if what == "the library" && a.to_string().len() + 1 <= MAX_INDEX_BYTES {
            assert!(!a.to_string().contains(" more\""), "the library's table fits, so every name is in it: {a}");
        }
    }
    // (The three larger tables are cut, the last to a line a kind.)
    assert!(index_of(&longer, 79).to_string().contains(" more"));
    assert!(index_of(&huge, 79).to_string().contains("forms in"));
}
