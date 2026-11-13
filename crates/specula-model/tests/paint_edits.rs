//! A recipe's `"edits"`: paint strokes as data, applied to the built model's vertex colours
//! (`specula_model::PaintEdit`). The editor's vertex-paint tool writes them; so can an agent.

use specula_model::{build, BuildParams, ErrorCode, Model, RecipeError, Style, MAX_PAINT_EDITS};

const STYLE: &str = r##"{"palette":{"moss":"#00ff00"},"materials":{"m":{"color":"#ffffff"}}}"##;

fn run(edits: &str) -> Result<Model, RecipeError> {
    let style = Style::parse(STYLE).unwrap();
    let text = format!(r#"{{"name":"x","collider":"none","lods":[],"parts":[{{"name":"a","material":"m","shape":{{"op":"box","size":[2,2,2],"bevel":0.0}}}}],"edits":{edits}}}"#);
    build(&text, &BuildParams::new(&style))
}

/// The colours of the vertices at `corner` (a box corner has one vertex per face that meets it).
fn at(m: &Model, corner: [f32; 3]) -> Vec<[u8; 4]> {
    let mesh = &m.lods[0];
    let near = |p: &[f32; 3]| (0..3).all(|k| (p[k] - corner[k]).abs() < 1e-4);
    mesh.positions.iter().zip(&mesh.colors).filter(|(p, _)| near(p)).map(|(_, c)| *c).collect()
}

/// The box's two opposite corners (wherever the shape puts its origin).
fn corners() -> ([f32; 3], [f32; 3]) {
    let m = run("[]").unwrap();
    let fold = |pick: fn(f32, f32) -> f32| m.lods[0].positions.iter().fold(m.lods[0].positions[0], |a, p| [pick(a[0], p[0]), pick(a[1], p[1]), pick(a[2], p[2])]);
    (fold(f32::max), fold(f32::min))
}

/// A stroke at the box's top corner, as JSON.
fn stroke(extra: &str) -> String {
    let (c, _) = corners();
    format!(r##"{{"at":[{},{},{}],"radius":0.5{extra}}}"##, c[0], c[1], c[2])
}

#[test]
fn a_stroke_colours_the_vertices_it_reaches_and_no_others() {
    let plain = run("[]").unwrap();
    let painted = run(&format!("[{}]", stroke(r##","op":"paint","color":"#ff0000","amount":1"##))).unwrap();
    assert_eq!(plain.lods[0].positions, painted.lods[0].positions, "paint moves nothing");
    let (corner, far) = corners();
    assert!(!at(&painted, corner).is_empty(), "the box has a vertex at its corner");
    for c in at(&painted, corner) {
        assert_eq!((c[0], c[1], c[2]), (255, 0, 0), "the stroke's centre takes its colour whole");
    }
    assert_eq!(at(&painted, far), at(&plain, far), "a vertex outside the radius keeps its colour");
    // Alpha is the wind's painted weight: a stroke never touches it.
    assert!(painted.lods[0].colors.iter().zip(&plain.lods[0].colors).all(|(a, b)| a[3] == b[3]));
    // Half the amount: half way from white to red at the centre.
    let half = run(&format!("[{}]", stroke(r##","color":"#ff0000","amount":0.5"##))).unwrap();
    for c in at(&half, corner) {
        assert!(c[0] == 255 && c[1] > 100 && c[1] < 230 && c[1] == c[2], "half an amount blends: {c:?}");
    }
}

#[test]
fn strokes_apply_in_order_and_take_palette_names() {
    let two = format!("[{},{}]", stroke(r##","color":"#ff0000""##), stroke(r##","color":"moss""##));
    let m = run(&two).unwrap();
    for c in at(&m, corners().0) {
        assert_eq!((c[0], c[1], c[2]), (0, 255, 0), "the later stroke paints over the earlier one");
    }
    // The same data, the same model (a stroke is part of what the recipe's hash covers).
    let again = run(&two).unwrap();
    assert_eq!(m.lods[0].colors, again.lods[0].colors);
    assert_eq!(m.hash, again.hash);
    assert_ne!(m.hash, run("[]").unwrap().hash, "paint changes the model's hash");
}

#[test]
fn hostile_strokes_are_refused_with_their_path() {
    let refused = |edits: &str| run(edits).err().map(|e| (e.code, e.path)).expect("refused");
    assert_eq!(refused(r##"[{"at":[0,0,0],"radius":1,"color":"nope"}]"##), (ErrorCode::OutOfRange, "edits[0].color".to_owned()));
    assert_eq!(refused(r##"[{"at":[0,0,0],"radius":0,"color":"moss"}]"##), (ErrorCode::OutOfRange, "edits[0].radius".to_owned()));
    assert_eq!(refused(r##"[{"at":[0,0,0],"radius":1e9,"color":"moss"}]"##), (ErrorCode::OutOfRange, "edits[0].radius".to_owned()));
    assert_eq!(refused(r##"[{"at":[0,0],"radius":1,"color":"moss"}]"##), (ErrorCode::MissingField, "edits[0].at".to_owned()));
    assert_eq!(refused(r##"[{"at":[1e30,0,0],"radius":1,"color":"moss"}]"##), (ErrorCode::OutOfRange, "edits[0].at".to_owned()));
    assert_eq!(refused(r##"[{"at":[0,0,0],"radius":1,"color":"moss","amount":2}]"##), (ErrorCode::OutOfRange, "edits[0].amount".to_owned()));
    assert_eq!(refused(r##"[{"op":"smear","at":[0,0,0],"radius":1,"color":"moss"}]"##), (ErrorCode::OutOfRange, "edits[0].op".to_owned()));
    assert_eq!(refused(r##"[{"at":[0,0,0],"radius":1,"color":"moss","size":3}]"##), (ErrorCode::UnknownField, "edits[0].size".to_owned()));
    assert_eq!(refused(r##"{"at":[0,0,0]}"##).0, ErrorCode::WrongType);
    // The cap, and one under it.
    let stroke = r##"{"at":[1,1,1],"radius":0.5,"color":"moss","amount":0.01}"##;
    let many = |n: usize| format!("[{}]", vec![stroke; n].join(","));
    assert_eq!(refused(&many(MAX_PAINT_EDITS + 1)), (ErrorCode::Limit, "edits".to_owned()));
    assert!(run(&many(MAX_PAINT_EDITS)).is_ok(), "the cap itself builds (its work is charged to the model's budget)");
}

/// A terrain's strokes: `terrain_edit` says a terrain holds 512, and the recipe's general cap on
/// a list of objects stopped it at 64 (the editor's sculpt brush reached that in five drags; the
/// project store then refused the stroke as a recipe that would not load).
#[test]
fn a_terrain_holds_its_512_strokes_not_64() {
    let style = Style::parse(STYLE).unwrap();
    let terrain = |strokes: usize| {
        let stroke = r#"{"op":"raise","at":[0,0],"radius":2,"amount":0.01}"#;
        let text = format!(
            r#"{{"name":"t","collider":"none","lods":[],"parts":[{{"name":"g","material":"m","generate":{{"op":"terrain","size":[32,32],"samples":[33,33],"layers":[{{"kind":"fbm","scale":20,"octaves":2,"amplitude":1}}],"edits":[{}]}}}}]}}"#,
            vec![stroke; strokes].join(",")
        );
        build(&text, &BuildParams::new(&style))
    };
    assert!(terrain(64).is_ok());
    assert!(terrain(65).is_ok(), "the 65th stroke: {:?}", terrain(65).err().map(|e| e.message));
    assert!(terrain(512).is_ok(), "the cap itself");
    let Err(over) = terrain(513) else { panic!("one past the cap builds") };
    assert_eq!(over.code, ErrorCode::Limit);
    assert!(over.path.ends_with("edits"), "{}", over.path);
}

/// The editor's path tool sketches a sweep part's path: a recipe says which of its parts are
/// sweeps, in recipe order (`model_path` counts them the same way, from the file's JSON).
#[test]
fn a_recipe_names_its_sweep_parts_in_order() {
    let style = Style::parse(STYLE).unwrap();
    let text = r#"{"name":"fence","collider":"none","lods":[],"parts":[
        {"name":"post","material":"m","shape":{"op":"box","size":[0.1,1,0.1]}},
        {"name":"rail","material":"m","shape":{"op":"sweep","profile":{"rect":[0.05,0.05]},"path":[[0,0.7,0],[1,0.7,0]]}},
        {"name":"cap","material":"m","shape":{"op":"box","size":[0.2,0.1,0.2]}},
        {"name":"wire","material":"m","shape":{"op":"sweep","profile":{"rect":[0.01,0.01]},"path":[[0,0.4,0],[1,0.4,0],[2,0.5,0]],"smooth":3}}
    ]}"#;
    let recipe = specula_model::Recipe::parse(text, &style).unwrap();
    assert_eq!(recipe.sweep_parts().collect::<Vec<_>>(), ["rail", "wire"]);
    assert_eq!(recipe.part_names().count(), 4);
    let none = specula_model::Recipe::parse(r#"{"name":"x","collider":"none","lods":[],"parts":[{"name":"a","material":"m","shape":{"op":"box","size":[1,1,1]}}]}"#, &style).unwrap();
    assert_eq!(none.sweep_parts().count(), 0);
}
