//! Layout maths: anchors, stacks, rows / columns with fill shares, grids, percent / em units,
//! DPI scale, safe areas, min / max, wrapping and visibility. Uses the built-in box font
//! (every glyph 0.5 em wide, 1 em line height) so sizes are exact.

mod common;
use common::*;
use specula_ui::{Rect, StateMap, Ui, Viewport};

fn rect(ui: &Ui, id: &str) -> Rect {
    let w = ui.widget("s", id).unwrap_or_else(|| panic!("no widget {id}"));
    ui.rect(w).unwrap_or_else(|| panic!("{id} not visible"))
}

fn run(root: &str, v: Viewport, state: &StateMap) -> Ui {
    let mut ui = ui_with("hud", root);
    let mut h = Harness::bare();
    h.frame(&mut ui, &[], state, &v);
    ui
}

fn close(a: Rect, b: [f32; 4]) {
    let ok = (a.x - b[0]).abs() < 0.01 && (a.y - b[1]).abs() < 0.01 && (a.w - b[2]).abs() < 0.01 && (a.h - b[3]).abs() < 0.01;
    assert!(ok, "{a:?} != {b:?}");
}

#[test]
fn anchors_place_children_in_a_stack() {
    let anchors = ["top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right"];
    let kids: Vec<String> = anchors
        .iter()
        .map(|a| format!(r#"{{"type": "box", "id": "{a}", "anchor": "{a}", "width": 100, "height": 50, "margin": 10}}"#))
        .collect();
    let ui = run(&format!(r#"{{"type": "box", "children": [{}]}}"#, kids.join(",")), view(800, 600), &StateMap::new());
    let xs = [10.0, 350.0, 690.0];
    let ys = [10.0, 275.0, 540.0];
    for (k, a) in anchors.iter().enumerate() {
        close(rect(&ui, a), [xs[k % 3], ys[k / 3], 100.0, 50.0]);
    }
}

#[test]
fn offsets_and_safe_area() {
    let root = r#"{"type": "box", "children": [
        {"type": "box", "id": "a", "anchor": "top_left", "width": 10, "height": 10, "offset": [5, "50%"]},
        {"type": "box", "id": "b", "anchor": "bottom_right", "width": 10, "height": 10}]}"#;
    let v = view(200, 100).with_safe_area([20.0, 30.0, 10.0, 40.0]);
    let ui = run(root, v, &StateMap::new());
    // Safe content box: x 40..170, y 20..90 (130 × 70).
    close(rect(&ui, "a"), [45.0, 55.0, 10.0, 10.0]);
    close(rect(&ui, "b"), [160.0, 80.0, 10.0, 10.0]);
}

#[test]
fn row_shares_free_space_by_fill_weight() {
    let root = r#"{"type": "box", "children": [{"type": "box", "id": "row", "layout": "row", "width": 400, "height": 40, "padding": 10, "gap": 10,
        "children": [
          {"type": "box", "id": "fixed", "width": 100, "height": 20},
          {"type": "box", "id": "one", "width": "fill", "height": 20},
          {"type": "box", "id": "two", "width": "fill:2", "height": "fill"}]}]}"#;
    let ui = run(root, view(800, 600), &StateMap::new());
    // Content 380 wide; 100 + 2 gaps of 10 used; 260 shared 1 : 2.
    close(rect(&ui, "fixed"), [10.0, 10.0, 100.0, 20.0]);
    let one = rect(&ui, "one");
    let two = rect(&ui, "two");
    assert!((one.w - 260.0 / 3.0).abs() < 0.01 && (two.w - 520.0 / 3.0).abs() < 0.01);
    assert!((one.x - 120.0).abs() < 0.01 && (two.x - (130.0 + one.w)).abs() < 0.01);
    // Fill on the cross axis of a row takes the content height.
    assert!((two.h - 20.0).abs() < 0.01);
}

#[test]
fn column_alignment_and_justify() {
    let mk = |align: &str, justify: &str| {
        format!(
            r#"{{"type": "box", "children": [{{"type": "box", "id": "col", "layout": "column", "width": 200, "height": 200, "align": "{align}", "justify": "{justify}", "gap": 10,
            "children": [{{"type": "box", "id": "a", "width": 50, "height": 30}}, {{"type": "box", "id": "b", "height": 30}}]}}]}}"#
        )
    };
    let ui = run(&mk("center", "start"), view(400, 400), &StateMap::new());
    close(rect(&ui, "a"), [75.0, 0.0, 50.0, 30.0]);
    let ui = run(&mk("end", "end"), view(400, 400), &StateMap::new());
    close(rect(&ui, "a"), [150.0, 130.0, 50.0, 30.0]);
    close(rect(&ui, "b"), [200.0, 170.0, 0.0, 30.0]);
    let ui = run(&mk("stretch", "space_between"), view(400, 400), &StateMap::new());
    close(rect(&ui, "a"), [0.0, 0.0, 50.0, 30.0]);
    // Auto width stretches; fixed width does not.
    close(rect(&ui, "b"), [0.0, 170.0, 200.0, 30.0]);
    let ui = run(&mk("start", "center"), view(400, 400), &StateMap::new());
    close(rect(&ui, "a"), [0.0, 65.0, 50.0, 30.0]);
}

#[test]
fn percent_em_and_dpi_scale() {
    let root = r#"{"type": "box", "children": [{"type": "box", "id": "p", "width": 300, "height": 100, "padding": 10, "children": [
        {"type": "box", "id": "half", "width": "50%", "height": "2em"},
        {"type": "label", "id": "l", "text": "Hello", "anchor": "bottom_right"}]}]}"#;
    let ui = run(root, view(800, 600), &StateMap::new());
    // 50% of the 280 content width; 2em = 2 × 16.
    close(rect(&ui, "half"), [10.0, 10.0, 140.0, 32.0]);
    // 5 glyphs × 8 px, one 16 px line, anchored in the content box.
    close(rect(&ui, "l"), [250.0, 74.0, 40.0, 16.0]);
    // At scale 1.5 every logical length grows, percentages follow the (grown) parent.
    let ui = run(root, Viewport::new([800, 600], 1.5), &StateMap::new());
    close(rect(&ui, "p"), [0.0, 0.0, 450.0, 150.0]);
    close(rect(&ui, "half"), [15.0, 15.0, 210.0, 48.0]);
    close(rect(&ui, "l"), [375.0, 111.0, 60.0, 24.0]);
}

#[test]
fn rows_fit_their_content() {
    let root = r#"{"type": "box", "children": [{"type": "box", "id": "row", "layout": "row", "padding": [2, 4], "gap": 6, "children": [
        {"type": "label", "text": "abcd"}, {"type": "label", "text": "ab"}, {"type": "box", "width": 10, "height": 30, "margin": [0, 3, 0, 3]}]}]}"#;
    let ui = run(root, view(800, 600), &StateMap::new());
    // 32 + 6 + 16 + 6 + (3 + 10 + 3), plus 8 of padding; the tallest child (30) plus 4.
    close(rect(&ui, "row"), [0.0, 0.0, 84.0, 34.0]);
}

#[test]
fn grid_cells_and_rows() {
    let kids: Vec<String> = (0..5).map(|k| format!(r#"{{"type": "box", "id": "c{k}", "height": {}}}"#, 20 + k * 5)).collect();
    let root = format!(r#"{{"type": "box", "children": [{{"type": "box", "id": "g", "layout": "grid", "columns": 3, "gap": 10, "width": 320, "children": [{}]}}]}}"#, kids.join(","));
    let ui = run(&root, view(800, 600), &StateMap::new());
    // Cells (320 − 20) / 3 = 100 wide; row 1 is 30 tall (its tallest), row 2 starts at 40.
    close(rect(&ui, "c0"), [0.0, 0.0, 0.0, 20.0]);
    close(rect(&ui, "c2"), [220.0, 0.0, 0.0, 30.0]);
    close(rect(&ui, "c3"), [0.0, 40.0, 0.0, 35.0]);
    close(rect(&ui, "c4"), [110.0, 40.0, 0.0, 40.0]);
    close(rect(&ui, "g"), [0.0, 0.0, 320.0, 80.0]);
}

#[test]
fn min_and_max_clamp() {
    let root = r#"{"type": "box", "children": [
        {"type": "box", "id": "a", "width": "fill", "max_width": 120, "height": 5, "min_height": 25},
        {"type": "label", "id": "l", "text": "x", "min_width": "3em", "anchor": "bottom"}]}"#;
    let ui = run(root, view(800, 600), &StateMap::new());
    close(rect(&ui, "a"), [0.0, 0.0, 120.0, 25.0]);
    close(rect(&ui, "l"), [376.0, 584.0, 48.0, 16.0]);
}

#[test]
fn wrapping_labels_grow_downwards() {
    let root = r#"{"type": "box", "children": [{"type": "box", "layout": "column", "width": 80, "children": [
        {"type": "label", "id": "w", "text": "aaaa bbbb cccc", "wrap": true, "width": "fill"},
        {"type": "label", "id": "after", "text": "z"}]}]}"#;
    let ui = run(root, view(800, 600), &StateMap::new());
    // "aaaa bbbb" is 72 px: fits 80; "cccc" wraps. Two lines of 16.
    close(rect(&ui, "w"), [0.0, 0.0, 80.0, 32.0]);
    close(rect(&ui, "after"), [0.0, 32.0, 8.0, 16.0]);
}

#[test]
fn hidden_widgets_leave_the_flow() {
    let root = r#"{"type": "box", "children": [{"type": "box", "layout": "row", "gap": 10, "children": [
        {"type": "box", "id": "a", "width": 10, "height": 10},
        {"type": "box", "id": "b", "width": 10, "height": 10, "visible": "show_b"},
        {"type": "box", "id": "c", "width": 10, "height": 10}]}]}"#;
    let mut s = StateMap::new();
    s.set_bool("show_b", false);
    let ui = run(root, view(800, 600), &s);
    assert!(ui.rect(ui.widget("s", "b").unwrap()).is_none());
    close(rect(&ui, "c"), [20.0, 0.0, 10.0, 10.0]);
    s.set_bool("show_b", true);
    let ui = run(root, view(800, 600), &s);
    close(rect(&ui, "c"), [40.0, 0.0, 10.0, 10.0]);
}

#[test]
fn ui_keys_adapt_layout_to_the_screen() {
    let root = r#"{"type": "box", "children": [
        {"type": "box", "id": "wide", "width": 10, "height": 10, "visible": "!ui.narrow"},
        {"type": "box", "id": "narrow", "width": 10, "height": 10, "visible": "ui.narrow && ui.portrait"},
        {"type": "label", "id": "size", "text": "{ui.width}x{ui.height}", "anchor": "bottom"}]}"#;
    let mut ui = ui_with("hud", root);
    let mut h = Harness::bare();
    h.frame(&mut ui, &[], &StateMap::new(), &view(1280, 800));
    assert!(ui.rect(ui.widget("s", "wide").unwrap()).is_some());
    assert!(ui.rect(ui.widget("s", "narrow").unwrap()).is_none());
    // A phone: 1080 × 2400 at scale 2 is 540 × 1200 logical.
    h.frame(&mut ui, &[], &StateMap::new(), &Viewport::new([1080, 2400], 2.0));
    assert!(ui.rect(ui.widget("s", "wide").unwrap()).is_none());
    assert!(ui.rect(ui.widget("s", "narrow").unwrap()).is_some());
    let tree = ui.tree().to_pretty();
    assert!(tree.contains("540x1200"), "{tree}");
}

#[test]
fn repeat_rows_follow_the_state_list() {
    let root = r#"{"type": "box", "children": [{"type": "list", "id": "l", "height": 100, "repeat": {"source": "rows", "max": 5, "template":
        {"type": "label", "id": "row", "text": "{item.number}:{item.name}"}}}]}"#;
    let mut s = StateMap::new();
    let rows = s.list_mut("rows");
    for n in ["a", "b", "c"] {
        rows.push().set_str("name", n);
    }
    let ui = run(root, view(400, 400), &s);
    for k in 0..5 {
        let r = ui.rect(ui.widget("s", &format!("row[{k}]")).unwrap());
        assert_eq!(r.is_some(), k < 3, "row {k}");
    }
    // An auto-width list fits its widest row ("1:a" = 24 px) and stretches every row to it.
    close(rect(&ui, "row[2]"), [0.0, 32.0, 24.0, 16.0]);
    assert!(ui.tree().to_pretty().contains("\"3:c\""));
}
