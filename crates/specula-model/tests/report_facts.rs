//! What a build's report says of the mesh itself: whether it is closed. (Found missing on
//! 2026-11-21: six cold builders in a measured comparison each wrote their own check of
//! watertightness on an exported file, because no answer of the engine's said it.)

use specula_model::{build, BuildParams, Model, Style};

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "dark": "#202020" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap()
}

fn make(shape: &str) -> Model {
    let s = style();
    let text = format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {shape} }} ] }}"#);
    build(&text, &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{shape}: {e:?}"))
}

#[test]
fn a_closed_model_says_it_is_closed() {
    for shape in [r#"{ "op": "box", "size": [1, 1, 1] }"#, r#"{ "op": "cylinder", "radius": 0.5, "height": 1 }"#, r#"{ "op": "sphere", "radius": 0.5 }"#] {
        let m = make(shape);
        assert_eq!(m.open_edges, 0, "{shape}");
        let r = m.report();
        assert_eq!(r.get("closed").and_then(|v| v.as_bool()), Some(true), "{shape}: {r:?}");
        assert_eq!(r.get("open_edges").and_then(|v| v.as_f64()), Some(0.0), "{shape}");
    }
}

#[test]
fn an_open_model_says_how_many_edges_are_open() {
    // A lathe with its caps left off: a tube open at both ends, a ring of open edges at each.
    let m = make(r#"{ "op": "lathe", "profile": [[0.5, 0], [0.5, 1]], "segments": 12, "caps": false }"#);
    assert_eq!(m.open_edges, 24, "twelve open edges round each end");
    let r = m.report();
    assert_eq!(r.get("closed").and_then(|v| v.as_bool()), Some(false), "{r:?}");
    assert_eq!(r.get("open_edges").and_then(|v| v.as_f64()), Some(24.0));
    // The count is the mesh's own, not a constant: it follows the segments.
    assert_eq!(make(r#"{ "op": "lathe", "profile": [[0.5, 0], [0.5, 1]], "segments": 20, "caps": false }"#).open_edges, 40);
}
