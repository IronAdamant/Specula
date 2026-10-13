//! Regression tests from the UI lane review (probes in the review's scratch crate): stale widget
//! references, pointer ownership around radial menus, touch cancel and one-frame taps, modal and
//! hidden-screen scoping for agents, per-target checks, hostile input, and degenerate viewports.

mod common;
use common::*;
use specula_platform::{Event, MouseButton, PointerKind};
use specula_ui::check::{check, Target};
use specula_ui::{ErrorCode, StateMap, Ui, UiEvents, Viewport};

const BTN: &str = r#"{"type": "box", "children": [{"type": "button", "id": "go", "text": "Go", "action": "go", "anchor": "center"}]}"#;

fn acts(e: &UiEvents) -> Vec<String> {
    e.actions().map(str::to_owned).collect()
}

fn go_centre(ui: &Ui, screen: &str) -> (i32, i32) {
    let c = ui.rect(ui.widget(screen, "go").unwrap()).unwrap().center();
    (c[0] as i32, c[1] as i32)
}

fn touch(down: bool, x: i32, y: i32) -> Event {
    if down {
        Event::PointerDown { id: 1, kind: PointerKind::Touch, x, y }
    } else {
        Event::PointerUp { id: 1, kind: PointerKind::Touch, x, y }
    }
}

#[test]
fn hot_reload_during_a_list_drag_does_not_panic() {
    for prefix in [0, 10] {
        let mut ui = ui();
        let pre: String = (0..prefix).map(|i| format!(r#"{{"type": "label", "text": "x{i}", "anchor": "top_left"}},"#)).collect();
        let rows: Vec<String> = (0..30).map(|i| format!(r#"{{"type": "label", "text": "row {i}"}}"#)).collect();
        let root = format!(r#"{{"type": "box", "children": [{pre}{{"type": "list", "id": "l", "anchor": "center", "height": 200, "width": 300, "children": [{}]}}]}}"#, rows.join(","));
        ui.load_screen(&screen("s", "hud", &root)).unwrap();
        ui.show("s").unwrap();
        let mut h = Harness::new();
        let (s, v) = (StateMap::new(), view(800, 600));
        h.frame(&mut ui, &[], &s, &v);
        let r = ui.rect(ui.widget("s", "l").unwrap()).unwrap();
        let (x, y) = ((r.x + 50.0) as i32, (r.y + 150.0) as i32);
        h.frame(&mut ui, &[touch(true, x, y)], &s, &v);
        for dy in [30, 60] {
            h.frame(&mut ui, &[Event::PointerMove { id: 1, kind: PointerKind::Touch, x, y: y - dy }], &s, &v);
        }
        // Reload with a smaller screen mid-drag, then keep dragging and release over a button.
        ui.load_screen(&screen("s", "hud", BTN)).unwrap();
        h.frame(&mut ui, &[Event::PointerMove { id: 1, kind: PointerKind::Touch, x, y: y - 90 }], &s, &v);
        let (bx, by) = go_centre(&ui, "s");
        h.frame(&mut ui, &[Event::PointerMove { id: 1, kind: PointerKind::Touch, x: bx, y: by }], &s, &v);
        let e = h.frame(&mut ui, &[touch(false, bx, by)], &s, &v);
        assert!(acts(&e).is_empty(), "a press from before the reload must not activate: {:?}", acts(&e));
    }
}

#[test]
fn the_whole_radial_reach_belongs_to_the_ui() {
    let root = r#"{"type": "box", "children": [{"type": "radial", "id": "r", "anchor": "center", "radius": 100, "inner_radius": 40, "items": [
        {"id": "a", "label": "A", "action": "a"}, {"id": "b", "label": "B", "action": "b"}, {"id": "c", "label": "C", "action": "c"}, {"id": "d", "label": "D", "action": "d"}]}]}"#;
    let mut ui = ui_with("menu", root);
    let mut h = Harness::new();
    let (s, v) = (StateMap::new(), view(800, 600));
    h.frame(&mut ui, &[], &s, &v);
    // Outside the radial's square, inside the pick reach (1.6 r).
    let c = ui.rect(ui.widget("s", "r").unwrap()).unwrap().center();
    let (x, y) = ((c[0] + 140.0) as i32, c[1] as i32);
    h.frame(&mut ui, &[Event::MouseMove { x, y }], &s, &v);
    let e = h.frame(&mut ui, &[Event::MouseDown { button: MouseButton::Left, x, y }], &s, &v);
    assert!(e.consumed.pointer, "the game must not also place here");
    let e = h.frame(&mut ui, &[Event::MouseUp { button: MouseButton::Left, x, y }], &s, &v);
    assert!(e.consumed.pointer);
    assert_eq!(acts(&e), vec!["b"]);
}

#[test]
fn a_cancelled_touch_never_activates() {
    let mut ui = ui_with("hud", BTN);
    let mut h = Harness::new();
    let (s, v) = (StateMap::new(), view(800, 600));
    h.frame(&mut ui, &[], &s, &v);
    let (x, y) = go_centre(&ui, "s");
    let e = h.frame(&mut ui, &[touch(true, x, y)], &s, &v);
    assert!(acts(&e).is_empty());
    let e = h.frame(&mut ui, &[Event::PointerCancel { id: 1, kind: PointerKind::Touch }], &s, &v);
    assert!(acts(&e).is_empty(), "{:?}", acts(&e));
    assert!(e.consumed.pointer, "the cancelled press was still the UI's");
    let e = h.frame(&mut ui, &[], &s, &v);
    assert!(acts(&e).is_empty() && !e.consumed.pointer);
}

#[test]
fn taps_and_clicks_within_one_frame_activate() {
    let mut ui = ui_with("hud", BTN);
    let mut h = Harness::new();
    let (s, v) = (StateMap::new(), view(800, 600));
    h.frame(&mut ui, &[], &s, &v);
    let (x, y) = go_centre(&ui, "s");
    let mut got = Vec::new();
    for e in [&[touch(true, x, y), touch(false, x, y)][..], &[], &[]] {
        got.extend(acts(&h.frame(&mut ui, e, &s, &v)));
    }
    assert_eq!(got, vec!["go"], "touch tap");
    let mut got = Vec::new();
    let click = [Event::MouseDown { button: MouseButton::Left, x, y }, Event::MouseUp { button: MouseButton::Left, x, y }];
    for e in [&click[..], &[], &[]] {
        got.extend(acts(&h.frame(&mut ui, e, &s, &v)));
    }
    assert_eq!(got, vec!["go"], "mouse click");
}

#[test]
fn agents_cant_reach_hidden_or_modal_covered_widgets() {
    let mut ui = ui();
    ui.load_screen(&screen("a", "menu", BTN)).unwrap();
    let modal = r#"{"format": "specula.ui.screen.v1", "name": "m", "input": "menu", "modal": true, "layer": 5,
        "root": {"type": "box", "children": [{"type": "button", "id": "ok", "text": "OK", "action": "ok", "anchor": "top_left"}]}}"#;
    ui.load_screen(modal).unwrap();
    ui.show("a").unwrap();
    ui.show("m").unwrap();
    let mut h = Harness::new();
    let (s, v) = (StateMap::new(), view(800, 600));
    h.frame(&mut ui, &[], &s, &v);
    let go = ui.widget("a", "go").unwrap();
    let e = ui.activate(go).unwrap_err();
    assert_eq!(e.code, ErrorCode::ModalBlocks);
    assert!(e.message.contains("\"m\""), "{e}");
    assert_eq!(ui.focus(go).unwrap_err().code, ErrorCode::ModalBlocks);
    ui.activate(ui.widget("m", "ok").unwrap()).unwrap();
    assert_eq!(acts(&h.frame(&mut ui, &[], &s, &v)), vec!["ok"]);
    // Hidden screens: no rects, no activation.
    ui.hide("m").unwrap();
    ui.hide("a").unwrap();
    assert_eq!(ui.rect(go), None, "hide() clears layout");
    assert_eq!(ui.activate(go).unwrap_err().code, ErrorCode::NotVisible);
    // A request queued before the screen was hidden is dropped.
    ui.show("a").unwrap();
    h.frame(&mut ui, &[], &s, &v);
    ui.activate(go).unwrap();
    ui.hide("a").unwrap();
    assert!(acts(&h.frame(&mut ui, &[], &s, &v)).is_empty());
    let bogus = specula_ui::WidgetRef { screen: 0, node: 999 };
    assert_eq!(ui.activate(bogus).unwrap_err().code, ErrorCode::UnknownWidget);
}

#[test]
fn contrast_is_checked_at_every_target() {
    let root = r##"{"type": "box", "children": [{"type": "panel", "anchor": "center", "children": [
        {"type": "label", "id": "lowc", "text": "Phone only", "color": "#2b3038", "visible": "ui.portrait"}]}]}"##;
    let mut ui = ui();
    ui.load_screen(&screen("s", "hud", root)).unwrap();
    let count = |t: &[Target], ui: &mut Ui| {
        let f = check(ui, &["s"], &StateMap::new(), None, t).unwrap();
        f.into_iter().filter(|x| x.rule == "contrast").collect::<Vec<_>>()
    };
    let a = count(&Target::defaults(), &mut ui);
    let mut rev = Target::defaults();
    rev.reverse();
    let b = count(&rev, &mut ui);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(b.len(), 1);
    assert_eq!(a[0].target, "phone_1080x2400", "reported where it happens");
}

#[test]
fn check_leaves_the_ui_as_it_was() {
    let mut ui = ui_with("hud", BTN);
    let mut h = Harness::new();
    let (s, v) = (StateMap::new(), view(800, 600));
    h.frame(&mut ui, &[], &s, &v);
    let (x, y) = go_centre(&ui, "s");
    let before = ui.rect(ui.widget("s", "go").unwrap());
    h.frame(&mut ui, &[Event::MouseMove { x, y }, Event::MouseDown { button: MouseButton::Left, x, y }], &s, &v);
    check(&mut ui, &["s"], &s, None, &Target::defaults()).unwrap();
    assert_eq!(ui.rect(ui.widget("s", "go").unwrap()), before, "layout restored");
    // The press in progress survives the check and completes normally.
    let e = h.frame(&mut ui, &[Event::MouseUp { button: MouseButton::Left, x, y }], &s, &v);
    assert_eq!(acts(&e), vec!["go"]);
}

#[test]
fn hostile_sizes_stay_cheap() {
    // A 256-row list: rows scrolled out of view aren't drawn.
    let root = r#"{"type": "box", "children": [{"type": "list", "id": "l", "anchor": "center", "height": 300, "width": 300,
        "repeat": {"source": "rows", "max": 256, "template": {"type": "label", "text": "Row {item.number}: {item.name} {item.big:.6}"}}}]}"#;
    let mut ui = ui_with("menu", root);
    let mut st = StateMap::new();
    let l = st.list_mut("rows");
    for _ in 0..256 {
        l.push().set_str("name", "a fairly ordinary row label").set_num("big", 1e300);
    }
    let mut h = Harness::new();
    h.frame(&mut ui, &[], &st, &view(800, 600));
    h.frame(&mut ui, &[], &st, &view(800, 600));
    let glyphs = ui.draw_list().glyphs.len();
    // ~19 visible rows of ≤ 52 characters.
    assert!(glyphs < 19 * 60, "{glyphs} glyph quads");
    // A screen object with 40 000 junk keys fails fast.
    let keys: String = (0..40_000).map(|i| format!(r#""k{i}": 1,"#)).collect();
    let text = screen("s", "hud", &format!(r#"{{{keys} "type": "box"}}"#));
    let t0 = std::time::Instant::now();
    let e = ui.load_screen(&text).unwrap_err();
    assert_eq!(e.code, ErrorCode::UnknownField);
    assert!(t0.elapsed().as_secs_f64() < 2.0, "{:?}", t0.elapsed());
}

#[test]
fn degenerate_viewports_do_not_panic() {
    let root = r#"{"type": "box", "children": [{"type": "panel", "anchor": "center", "width": "50%", "layout": "column", "children": [
        {"type": "button", "id": "go", "text": "Go", "action": "go"}, {"type": "label", "text": "wrap me please", "wrap": true, "width": "fill"}]}]}"#;
    for (size, scale, safe) in [
        ([0u32, 0u32], 1.0f32, [0.0f32; 4]),
        ([800, 600], 1.0, [500.0; 4]),
        ([800, 600], 1e6, [0.0; 4]),
        ([800, 600], 0.0, [0.0; 4]),
        ([800, 600], f32::NAN, [f32::INFINITY; 4]),
    ] {
        let mut ui = ui_with("menu", root);
        let mut h = Harness::new();
        let vp = Viewport::new(size, scale).with_safe_area(safe);
        h.frame(&mut ui, &[], &StateMap::new(), &vp);
        h.frame(&mut ui, &[kd("Down")], &StateMap::new(), &vp);
    }
}
