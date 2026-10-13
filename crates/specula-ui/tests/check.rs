//! `ui_check`: every rule with a failing and a passing case.

mod common;
use common::*;
use specula_input::ActionMap;
use specula_ui::check::{check, report_json, Finding, Severity, Target};
use specula_ui::{StateMap, Ui};

fn target() -> Target {
    Target::new("t", [1280, 800], None)
}

fn run_with(ui: &mut Ui, screens: &[&str], state: &StateMap, map: Option<&ActionMap>, targets: &[Target]) -> Vec<Finding> {
    check(ui, screens, state, map, targets).unwrap()
}

fn findings(input: &str, root: &str, state: &StateMap) -> Vec<Finding> {
    let mut ui = ui();
    ui.load_screen(&screen("s", input, root)).unwrap();
    run_with(&mut ui, &["s"], state, None, &[target()])
}

fn has(f: &[Finding], rule: &str, widget: &str) -> bool {
    f.iter().any(|x| x.rule == rule && x.widget.contains(widget))
}

/// A menu's content on a panel (focus rings need something solid around them).
fn panel_menu(column: &str) -> String {
    format!(r#"{{"type": "box", "children": [{{"type": "panel", "anchor": "center", "children": [{column}]}}]}}"#)
}

fn panel(child: &str) -> String {
    format!(r#"{{"type": "box", "children": [{{"type": "panel", "id": "p", "anchor": "center", "children": [{child}]}}]}}"#)
}

#[test]
fn legibility_cap_height() {
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "tiny", "role": "tiny"}"#), &StateMap::new());
    assert!(has(&f, "legibility", "t"), "{f:?}");
    let l = f.iter().find(|x| x.rule == "legibility").unwrap();
    // 8 px × 0.7 cap height = 5.6 px against 9.
    assert!((l.measured - 5.6).abs() < 0.01 && l.required == 9.0, "{l:?}");
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "fine", "role": "small"}"#), &StateMap::new());
    assert!(f.is_empty(), "{f:?}");
    // Denser screens need more physical pixels for the same physical size.
    let mut ui = ui();
    ui.load_screen(&screen("s", "hud", &panel(r#"{"type": "label", "id": "t", "text": "fine", "role": "small"}"#))).unwrap();
    let mut phone = Target::new("phone", [1080, 2400], Some(430.0));
    phone.user_scale = 0.5;
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[phone]);
    assert!(has(&f, "legibility", "t"), "{f:?}");
}

#[test]
fn contrast_of_text_on_panels_and_over_the_world() {
    let f = findings("hud", &panel(r##"{"type": "label", "id": "t", "text": "low", "color": "#3a3f48"}"##), &StateMap::new());
    assert!(has(&f, "contrast", "t"), "{f:?}");
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "ivory"}"#), &StateMap::new());
    assert!(!has(&f, "contrast", "t"), "{f:?}");
    // Ivory straight over the world fails against the bright scene sample.
    let f = findings("hud", r#"{"type": "box", "children": [{"type": "label", "id": "t", "text": "bare"}]}"#, &StateMap::new());
    assert!(has(&f, "contrast", "t"), "{f:?}");
    // A translucent panel doesn't save it.
    let f = findings("hud", r#"{"type": "box", "children": [{"type": "panel", "style": "glass", "children": [{"type": "label", "id": "t", "text": "glass"}]}]}"#, &StateMap::new());
    assert!(has(&f, "contrast", "t"), "{f:?}");
    // Large text needs only 3:1.
    let f = findings("hud", &panel(r##"{"type": "label", "id": "t", "text": "big", "role": "title", "color": "#7c8491"}"##), &StateMap::new());
    assert!(!has(&f, "contrast", "t"), "{f:?}");
    let f = findings("hud", &panel(r##"{"type": "label", "id": "t", "text": "small", "color": "#7c8491"}"##), &StateMap::new());
    assert!(has(&f, "contrast", "t"), "{f:?}");
}

#[test]
fn overlap_of_siblings_and_of_screens() {
    let a = r#"{"type": "panel", "id": "a", "anchor": "top_left", "width": 100, "height": 50}"#;
    let b = r#"{"type": "panel", "id": "b", "anchor": "top_left", "width": 100, "height": 50, "offset": [60, 20]}"#;
    let f = findings("hud", &format!(r#"{{"type": "box", "children": [{a}, {b}]}}"#), &StateMap::new());
    assert!(has(&f, "overlap", "b"), "{f:?}");
    let ok = r#"{"type": "panel", "id": "b", "anchor": "top_left", "width": 100, "height": 50, "offset": [110, 0]}"#;
    let f = findings("hud", &format!(r#"{{"type": "box", "children": [{a}, {ok}]}}"#), &StateMap::new());
    assert!(!has(&f, "overlap", "b"), "{f:?}");
    let marked = r#"{"type": "panel", "id": "b", "anchor": "top_left", "width": 100, "height": 50, "offset": [60, 20], "overlap_ok": true}"#;
    let f = findings("hud", &format!(r#"{{"type": "box", "children": [{a}, {marked}]}}"#), &StateMap::new());
    assert!(!has(&f, "overlap", "b"), "{f:?}");
    // Two screens shown together.
    let mut ui = ui();
    ui.load_screen(&screen("one", "hud", &format!(r#"{{"type": "box", "children": [{a}]}}"#))).unwrap();
    ui.load_screen(&screen("two", "hud", &format!(r#"{{"type": "box", "children": [{b}]}}"#))).unwrap();
    let f = run_with(&mut ui, &["one", "two"], &StateMap::new(), None, &[target()]);
    assert!(f.iter().any(|x| x.rule == "overlap" && x.screen == "two" && x.message.contains("one:a")), "{f:?}");
}

#[test]
fn offscreen_and_safe_area() {
    let root = r#"{"type": "box", "children": [{"type": "panel", "id": "p", "anchor": "top_left", "width": 100, "height": 50, "offset": [-30, 0]}]}"#;
    let f = findings("hud", root, &StateMap::new());
    assert!(has(&f, "offscreen", "p"), "{f:?}");
    let root = r#"{"type": "box", "children": [{"type": "panel", "id": "p", "anchor": "top_left", "width": 100, "height": 50}]}"#;
    let f = findings("hud", root, &StateMap::new());
    assert!(f.is_empty(), "{f:?}");
    // Too wide for a narrow target.
    let root = r#"{"type": "box", "children": [{"type": "panel", "id": "p", "anchor": "center", "width": 1400, "height": 50}]}"#;
    let f = findings("hud", root, &StateMap::new());
    assert!(has(&f, "offscreen", "p"), "{f:?}");
}

#[test]
fn overflow_and_text_overflow() {
    // A fixed-size panel whose content is bigger.
    let root = r#"{"type": "box", "children": [{"type": "panel", "id": "p", "width": 60, "height": 40, "children": [{"type": "panel", "id": "big", "width": 100, "height": 10}]}]}"#;
    let f = findings("hud", root, &StateMap::new());
    assert!(has(&f, "overflow", "big"), "{f:?}");
    let root = r#"{"type": "box", "children": [{"type": "panel", "id": "p", "children": [{"type": "panel", "id": "big", "width": 100, "height": 10}]}]}"#;
    let f = findings("hud", root, &StateMap::new());
    assert!(f.is_empty(), "{f:?}");
    // Text wider than a fixed label; then wrapped text taller than its box.
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "a long line of text", "width": 50}"#), &StateMap::new());
    assert!(has(&f, "text_overflow", "t"), "{f:?}");
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "a long line of text", "width": 50, "height": 16, "wrap": true}"#), &StateMap::new());
    assert!(has(&f, "text_overflow", "t"), "{f:?}");
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "a long line of text", "width": 50, "wrap": true}"#), &StateMap::new());
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn missing_glyphs() {
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "Grüße"}"#), &StateMap::new());
    assert!(has(&f, "missing_glyph", "t"), "{f:?}");
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "Grusse"}"#), &StateMap::new());
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn touch_targets_on_touch_devices() {
    let root = r#"{"type": "box", "children": [{"type": "button", "id": "b", "style": "small", "text": "x", "action": "a", "shortcut": "a"}]}"#;
    let mut ui = ui();
    ui.load_screen(&screen("s", "hud", root)).unwrap();
    let mut touch = target();
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[touch.clone()]);
    assert!(!has(&f, "touch_target", "b"), "not a touch device: {f:?}");
    touch.touch = true;
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[touch.clone()]);
    assert!(has(&f, "touch_target", "b"), "{f:?}");
    let root = r#"{"type": "box", "children": [{"type": "button", "id": "b", "text": "x", "action": "a", "shortcut": "a", "min_width": 44}]}"#;
    ui.load_screen(&screen("s", "hud", root)).unwrap();
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[touch]);
    assert!(!has(&f, "touch_target", "b"), "{f:?}");
}

#[test]
fn focus_reachability() {
    // "island" is up and to the left of everything: no direction from the others leads to it,
    // except that the neighbour search still finds it diagonally. Make it unreachable by
    // pinning every button's navigation explicitly.
    let root = r#"{"type": "box", "layout": "column", "children": [
        {"type": "button", "id": "a", "text": "a", "action": "a", "nav": {"down": "b", "up": "b", "left": "b", "right": "b"}},
        {"type": "button", "id": "b", "text": "b", "action": "b", "nav": {"down": "a", "up": "a", "left": "a", "right": "a"}},
        {"type": "button", "id": "island", "text": "c", "action": "c"}]}"#;
    let root = &panel_menu(root);
    let text = format!(r#"{{"format": "specula.ui.screen.v1", "name": "s", "input": "menu", "focus": "a", "root": {root}}}"#);
    let mut ui = ui();
    ui.load_screen(&text).unwrap();
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[target()]);
    assert!(has(&f, "focus_reachability", "island"), "{f:?}");
    assert!(!has(&f, "focus_reachability", "b"));
    // Spatial navigation alone reaches every button in a column.
    let root = r#"{"type": "box", "layout": "column", "children": [
        {"type": "button", "id": "a", "text": "a", "action": "a"}, {"type": "button", "id": "b", "text": "b", "action": "b"},
        {"type": "button", "id": "c", "text": "c", "action": "c"}]}"#;
    let root = &panel_menu(root);
    let text = format!(r#"{{"format": "specula.ui.screen.v1", "name": "s", "input": "menu", "focus": "c", "root": {root}}}"#);
    ui.load_screen(&text).unwrap();
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[target()]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn gamepad_access_for_hud_buttons() {
    let map = ActionMap::from_json_str(
        r#"{"format": "specula.input.v1", "actions": [
            {"name": "call_wave", "kind": "button", "bindings": ["Key.Space", "Pad.LeftStickClick"]},
            {"name": "speed", "kind": "button", "bindings": ["Key.F"]}]}"#,
    )
    .unwrap();
    let mut ui = ui();
    let root = r#"{"type": "box", "children": [
        {"type": "button", "id": "none", "text": "x", "action": "x", "anchor": "top_left"},
        {"type": "button", "id": "keys_only", "text": "x", "action": "x", "shortcut": "speed", "anchor": "top_right"},
        {"type": "button", "id": "good", "text": "x", "action": "x", "shortcut": "call_wave", "anchor": "bottom"}]}"#;
    ui.load_screen(&screen("s", "hud", root)).unwrap();
    let f = run_with(&mut ui, &["s"], &StateMap::new(), Some(&map), &[target()]);
    assert!(has(&f, "gamepad_access", "none"), "{f:?}");
    assert!(has(&f, "gamepad_access", "keys_only"), "{f:?}");
    assert!(!has(&f, "gamepad_access", "good"), "{f:?}");
    // Resolution-independent rules are reported once.
    let f = run_with(&mut ui, &["s"], &StateMap::new(), Some(&map), &Target::defaults());
    assert_eq!(f.iter().filter(|x| x.rule == "gamepad_access").count(), 2);
    assert!(f.iter().filter(|x| x.rule == "gamepad_access").all(|x| x.target == "all"));
}

#[test]
fn unbound_keys() {
    let root = &panel(r#"{"type": "label", "id": "t", "text": "Dust {dust}", "visible": "!paused"}"#);
    let f = findings("hud", root, &StateMap::new());
    assert!(f.iter().any(|x| x.rule == "unbound" && x.message.contains("\"dust\"")), "{f:?}");
    assert!(f.iter().any(|x| x.rule == "unbound" && x.message.contains("\"paused\"")), "{f:?}");
    assert!(f.iter().all(|x| x.severity == Severity::Warning || x.rule != "unbound"));
    let mut s = StateMap::new();
    s.set_int("dust", 3).set_bool("paused", false);
    let f = findings("hud", root, &s);
    assert!(f.is_empty(), "{f:?}");
    // ui.* keys are always bound.
    let f = findings("hud", &panel(r#"{"type": "label", "id": "t", "text": "{ui.width}", "visible": "!ui.narrow"}"#), &StateMap::new());
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn non_text_contrast_of_the_focus_ring_and_threat_arrows() {
    let low = THEME.replace(r##""threat": { "label_box": "tip" },"##, r##""focus": { "color": "#3a414c" }, "threat": { "color": "#30352f", "halo": "#00000000", "label_box": "tip" },"##);
    assert_ne!(low, THEME);
    let theme = specula_ui::Theme::from_json_str(&low).unwrap();
    let mut ui = Ui::new(theme, specula_ui::FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()));
    let text = r#"{"format": "specula.ui.screen.v1", "name": "s", "input": "menu", "root":
        {"type": "box", "children": [{"type": "panel", "anchor": "center", "children": [{"type": "button", "id": "b", "text": "x", "action": "x"}]}]}}"#;
    ui.load_screen(text).unwrap();
    let f = run_with(&mut ui, &["s"], &StateMap::new(), None, &[target()]);
    assert!(has(&f, "non_text_contrast", "b"), "{f:?}");
    assert!(has(&f, "non_text_contrast", "threat arrow"), "{f:?}");
    let mut ok = common::ui();
    ok.load_screen(text).unwrap();
    let f = run_with(&mut ok, &["s"], &StateMap::new(), None, &[target()]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn report_is_json_and_the_ui_is_restored() {
    let mut ui = ui();
    ui.load_screen(&screen("a", "hud", r#"{"type": "box", "children": [{"type": "label", "text": "{x}"}]}"#)).unwrap();
    ui.load_screen(&screen("b", "hud", r#"{"type": "box"}"#)).unwrap();
    ui.show("b").unwrap();
    let f = run_with(&mut ui, &["a"], &StateMap::new(), None, &[target()]);
    let j = report_json(&f).to_pretty();
    assert!(j.contains("\"rule\": \"unbound\"") && j.contains("\"warnings\": "), "{j}");
    assert!(ui.is_visible("b") && !ui.is_visible("a"));
    assert!(check(&mut ui, &["nope"], &StateMap::new(), None, &[target()]).is_err());
}

#[test]
fn the_sepulcra_screens_pass_with_the_box_font() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/ui");
    let theme = specula_ui::Theme::from_json_str(&std::fs::read_to_string(dir.join("theme.json")).unwrap()).unwrap();
    let mut ui = Ui::new(theme, specula_ui::FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()));
    let mut names = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "json") && p.file_stem().is_some_and(|s| s != "theme") {
            let id = ui.load_screen(&std::fs::read_to_string(&p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            names.push(ui.screen(id).name.clone());
        }
    }
    assert!(names.len() >= 9, "{names:?}");
    // Every screen parses and lays out at every target without errors.
    let mut s = StateMap::new();
    for k in ["wave", "waves", "dust", "tombstones", "tombstones_max", "selected_cost", "kills", "early_bonus", "speed"] {
        s.set_int(k, 10);
    }
    s.set_num("next_wave_s", 23.0).set_num("run_time_s", 431.0).set_num("ui_scale", 1.0).set_num("volume", 0.8);
    for k in ["selected", "selected_name", "selected_role", "hover", "glyph_style", "reduced_motion_text", "confirm_title", "rebinding_label"] {
        s.set_str(k, "Warding stone");
    }
    s.set_str("selected_detail", "Homing bolts: 10 damage every 0.8 s, 7 m range.");
    s.set_str("confirm_body", "The run will be lost.");
    for n in &names {
        let f = check(&mut ui, &[n], &s, None, &Target::defaults()).unwrap();
        let errors: Vec<&Finding> = f.iter().filter(|x| x.severity == Severity::Error && x.rule != "gamepad_access").collect();
        assert!(errors.is_empty(), "{n}: {errors:#?}");
    }
}
