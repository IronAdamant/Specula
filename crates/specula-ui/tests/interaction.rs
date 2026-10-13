//! Input: pointer, touch, gamepad / keyboard focus navigation, radial menus, scrolling, prompt
//! glyphs, and what the UI tells the game it consumed.

mod common;
use common::*;
use specula_platform::{Event, GamepadButton, PointerKind, RawGamepad};
use specula_ui::{StateMap, Ui, UiEvent, UiEvents};

const MENU: &str = r#"{"type": "box", "children": [{"type": "panel", "id": "panel", "anchor": "center", "layout": "column", "gap": 10, "align": "stretch", "width": 200, "children": [
    {"type": "button", "id": "resume", "text": "Resume", "action": "resume"},
    {"type": "button", "id": "settings", "text": "Settings", "action": "settings", "enabled": "allow_settings"},
    {"type": "button", "id": "quit", "text": "Quit", "action": "quit", "tooltip": "Leave the run"}]},
    {"type": "button", "id": "side", "text": "Side", "action": "side", "anchor": "right", "margin": 20}]}"#;

fn menu_ui() -> Ui {
    let mut ui = ui();
    let text = format!(
        r#"{{"format": "specula.ui.screen.v1", "name": "s", "input": "menu", "modal": true, "focus": "resume", "back": "resume", "root": {MENU}}}"#
    );
    ui.load_screen(&text).unwrap();
    ui.show("s").unwrap();
    ui
}

fn focused(ui: &Ui, e: &UiEvents) -> String {
    e.focused.and_then(|w| ui.widget_id(w)).unwrap_or("-").to_owned()
}

fn actions(e: &UiEvents) -> Vec<String> {
    e.actions().map(str::to_owned).collect()
}

fn centre(ui: &Ui, id: &str) -> [f32; 2] {
    ui.rect(ui.widget("s", id).unwrap()).unwrap().center()
}

#[test]
fn keyboard_navigation_moves_focus_spatially_and_accept_activates() {
    let mut ui = menu_ui();
    let mut h = Harness::new();
    let mut s = StateMap::new();
    s.set_bool("allow_settings", true);
    let v = view(800, 600);
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(focused(&ui, &e), "resume");
    assert!(e.consumed.navigation && e.consumed.all);
    let e = h.frame(&mut ui, &[kd("Down")], &s, &v);
    assert_eq!(focused(&ui, &e), "settings");
    h.frame(&mut ui, &[ku("Down")], &s, &v);
    let e = h.frame(&mut ui, &[kd("Down")], &s, &v);
    assert_eq!(focused(&ui, &e), "quit");
    h.frame(&mut ui, &[ku("Down")], &s, &v);
    // Nothing below quit: focus stays.
    let e = h.frame(&mut ui, &[kd("Down")], &s, &v);
    assert_eq!(focused(&ui, &e), "quit");
    h.frame(&mut ui, &[ku("Down")], &s, &v);
    // Right reaches the side button, left comes back to the column.
    let e = h.frame(&mut ui, &[kd("Right")], &s, &v);
    assert_eq!(focused(&ui, &e), "side");
    h.frame(&mut ui, &[ku("Right")], &s, &v);
    let e = h.frame(&mut ui, &[kd("Left")], &s, &v);
    assert_ne!(focused(&ui, &e), "side");
    h.frame(&mut ui, &[ku("Left")], &s, &v);
    let e = h.frame(&mut ui, &[kd("Enter")], &s, &v);
    assert_eq!(actions(&e).len(), 1);
    // Back sends Back and the screen's back action.
    h.frame(&mut ui, &[ku("Enter")], &s, &v);
    let e = h.frame(&mut ui, &[kd("Escape")], &s, &v);
    assert!(e.events.iter().any(|x| matches!(x, UiEvent::Back { .. })));
    assert_eq!(actions(&e), vec!["resume"]);
}

#[test]
fn disabled_buttons_are_skipped_and_refuse() {
    let mut ui = menu_ui();
    let mut h = Harness::new();
    let s = StateMap::new(); // allow_settings missing: disabled
    let v = view(800, 600);
    h.frame(&mut ui, &[], &s, &v);
    let e = h.frame(&mut ui, &[kd("Down")], &s, &v);
    assert_eq!(focused(&ui, &e), "quit");
    // Clicking the disabled one refuses.
    let p = centre(&ui, "settings");
    h.frame(&mut ui, &[ku("Down"), mv(p[0], p[1])], &s, &v);
    h.frame(&mut ui, &[down(p[0], p[1])], &s, &v);
    let e = h.frame(&mut ui, &[up(p[0], p[1])], &s, &v);
    assert!(e.events.iter().any(|x| matches!(x, UiEvent::Refused { .. })), "{:?}", e.events);
    assert!(actions(&e).is_empty());
}

#[test]
fn gamepad_fallback_without_ui_actions() {
    let mut ui = menu_ui();
    let mut h = Harness::bare();
    let mut s = StateMap::new();
    s.set_bool("allow_settings", true);
    let v = view(800, 600);
    h.frame(&mut ui, &[], &s, &v);
    h.pad = RawGamepad { buttons: GamepadButton::DpadDown.bit(), ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(focused(&ui, &e), "settings");
    h.pad = RawGamepad { buttons: GamepadButton::South.bit(), ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(actions(&e), vec!["settings"]);
}

#[test]
fn holding_a_direction_repeats() {
    let mut ui = ui();
    let kids: Vec<String> = (0..8).map(|k| format!(r#"{{"type": "button", "id": "b{k}", "text": "B", "action": "b"}}"#)).collect();
    ui.load_screen(&screen("s", "menu", &format!(r#"{{"type": "box", "layout": "column", "children": [{}]}}"#, kids.join(",")))).unwrap();
    ui.show("s").unwrap();
    let mut h = Harness::new();
    let s = StateMap::new();
    let v = view(400, 800);
    h.frame(&mut ui, &[], &s, &v);
    let mut e = h.frame(&mut ui, &[kd("Down")], &s, &v);
    assert_eq!(focused(&ui, &e), "b1");
    // Held for one second at 60 fps: 1 press + repeats after 0.4 s every 0.12 s.
    for _ in 0..60 {
        e = h.frame(&mut ui, &[], &s, &v);
    }
    assert_eq!(focused(&ui, &e), "b6");
}

#[test]
fn mouse_click_activates_and_hover_focuses() {
    let mut ui = menu_ui();
    let mut h = Harness::new();
    let mut s = StateMap::new();
    s.set_bool("allow_settings", true);
    let v = view(800, 600);
    h.frame(&mut ui, &[], &s, &v);
    let q = centre(&ui, "quit");
    let e = h.frame(&mut ui, &[mv(q[0], q[1])], &s, &v);
    assert_eq!(e.hovered.and_then(|w| ui.widget_id(w)), Some("quit"));
    assert_eq!(focused(&ui, &e), "quit");
    assert!(e.consumed.pointer);
    let e = h.frame(&mut ui, &[down(q[0], q[1])], &s, &v);
    assert!(actions(&e).is_empty(), "fires on release");
    let e = h.frame(&mut ui, &[up(q[0], q[1])], &s, &v);
    assert_eq!(actions(&e), vec!["quit"]);
    // Press on one button, release on another: nothing.
    let r = centre(&ui, "resume");
    h.frame(&mut ui, &[down(q[0], q[1])], &s, &v);
    let e = h.frame(&mut ui, &[mv(r[0], r[1]), up(r[0], r[1])], &s, &v);
    assert!(actions(&e).is_empty());
}

#[test]
fn touch_tap_activates() {
    let mut ui = menu_ui();
    let mut h = Harness::new();
    let s = StateMap::new();
    let v = view(800, 600);
    h.frame(&mut ui, &[], &s, &v);
    let r = centre(&ui, "resume");
    let (x, y) = (r[0] as i32, r[1] as i32);
    h.frame(&mut ui, &[Event::PointerDown { id: 1, kind: PointerKind::Touch, x, y }], &s, &v);
    let e = h.frame(&mut ui, &[Event::PointerUp { id: 1, kind: PointerKind::Touch, x, y }], &s, &v);
    assert_eq!(actions(&e), vec!["resume"]);
}

#[test]
fn hud_consumes_only_the_pointer_over_ui() {
    let root = r#"{"type": "box", "children": [
        {"type": "panel", "id": "bar", "anchor": "top", "width": 200, "height": 40},
        {"type": "label", "id": "free_text", "text": "over the world", "anchor": "center"},
        {"type": "button", "id": "call", "text": "Call", "action": "call_wave", "shortcut": "call_wave", "anchor": "bottom_right"}]}"#;
    let mut ui = ui_with("hud", root);
    let mut h = Harness::new();
    let s = StateMap::new();
    let v = view(800, 600);
    let e = h.frame(&mut ui, &[mv(400.0, 20.0)], &s, &v);
    assert!(e.consumed.pointer, "over the panel");
    assert!(!e.consumed.navigation && !e.consumed.all, "a HUD never takes the pad");
    let e = h.frame(&mut ui, &[mv(400.0, 300.0)], &s, &v);
    assert!(!e.consumed.pointer, "labels over the world don't block clicks");
    let e = h.frame(&mut ui, &[mv(400.0, 450.0)], &s, &v);
    assert!(!e.consumed.pointer);
    // The press-drag-release that starts on UI is the UI's to the end.
    h.frame(&mut ui, &[mv(400.0, 20.0)], &s, &v);
    let e = h.frame(&mut ui, &[down(400.0, 20.0)], &s, &v);
    assert!(e.consumed.pointer);
    let e = h.frame(&mut ui, &[mv(400.0, 300.0)], &s, &v);
    assert!(e.consumed.pointer, "captured while held");
    let e = h.frame(&mut ui, &[up(400.0, 300.0)], &s, &v);
    assert!(e.consumed.pointer, "the release belongs to the UI too");
    let e = h.frame(&mut ui, &[], &s, &v);
    assert!(!e.consumed.pointer);
    // Keys are the game's: no focus, no events.
    let e = h.frame(&mut ui, &[kd("Enter")], &s, &v);
    assert!(e.events.is_empty() && e.focused.is_none());
}

const RADIAL: &str = r#"{"type": "box", "children": [{"type": "radial", "id": "r", "anchor": "center", "radius": 100, "inner_radius": 40, "items": [
    {"id": "n", "label": "N", "action": "build:n", "detail": "North {cost}"},
    {"id": "e", "label": "E", "action": "build:e"},
    {"id": "s2", "label": "S", "action": "build:s", "enabled": "dust >= 50"},
    {"id": "w", "label": "W", "action": "build:w"}]}]}"#;

#[test]
fn radial_menu_picks_by_stick_angle() {
    let mut ui = ui_with("menu", RADIAL);
    let mut h = Harness::new();
    let mut s = StateMap::new();
    s.set_int("dust", 10).set_int("cost", 5);
    let v = view(800, 600);
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(focused(&ui, &e), "n");
    assert!(ui.tree().to_pretty().contains("North 5"), "the centre shows the item's detail");
    // Stick right (raw +x).
    h.pad = RawGamepad { left_x: 30000, ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(focused(&ui, &e), "e");
    // Stick down (raw y is up-positive): the disabled item is still selectable, and refuses.
    h.pad = RawGamepad { left_y: -30000, ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(focused(&ui, &e), "s2");
    h.pad = RawGamepad { left_y: -30000, buttons: GamepadButton::South.bit(), ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert!(e.events.iter().any(|x| matches!(x, UiEvent::Refused { .. })));
    // Stick left, accept.
    h.pad = RawGamepad { left_x: -30000, ..RawGamepad::default() };
    h.frame(&mut ui, &[], &s, &v);
    h.pad = RawGamepad { left_x: -30000, buttons: GamepadButton::South.bit(), ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(actions(&e), vec!["build:w"]);
    // Released stick: the selection stays; d-pad steps around the ring.
    h.pad = RawGamepad::default();
    h.frame(&mut ui, &[], &s, &v);
    h.pad = RawGamepad { buttons: GamepadButton::DpadRight.bit(), ..RawGamepad::default() };
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(focused(&ui, &e), "n", "right from the last item wraps to the first");
}

#[test]
fn radial_menu_picks_by_pointer_angle() {
    let mut ui = ui_with("menu", RADIAL);
    let mut h = Harness::new();
    let s = StateMap::new();
    let v = view(800, 600);
    h.frame(&mut ui, &[], &s, &v);
    // Pointing anywhere in the east sector (even outside the ring) picks it; a click builds.
    let e = h.frame(&mut ui, &[mv(400.0 + 130.0, 300.0 + 20.0)], &s, &v);
    assert_eq!(focused(&ui, &e), "e");
    h.frame(&mut ui, &[down(530.0, 320.0)], &s, &v);
    let e = h.frame(&mut ui, &[up(530.0, 320.0)], &s, &v);
    assert_eq!(actions(&e), vec!["build:e"]);
    // The dead zone in the middle keeps the current pick.
    let e = h.frame(&mut ui, &[mv(405.0, 302.0)], &s, &v);
    assert_eq!(focused(&ui, &e), "e");
    let e = h.frame(&mut ui, &[mv(400.0, 300.0 - 70.0)], &s, &v);
    assert_eq!(focused(&ui, &e), "n");
}

#[test]
fn lists_scroll_by_wheel_and_follow_focus() {
    let root = r#"{"type": "box", "children": [{"type": "list", "id": "l", "width": 200, "height": 100, "repeat": {"source": "rows", "max": 20, "template":
        {"type": "button", "id": "row", "style": "small", "text": "{item.name}", "action": "pick"}}}]}"#;
    let mut ui = ui_with("menu", root);
    let mut h = Harness::new();
    let mut s = StateMap::new();
    let rows = s.list_mut("rows");
    for k in 0..20 {
        rows.push().set_str("name", &format!("row {k}"));
    }
    let v = view(400, 400);
    h.frame(&mut ui, &[], &s, &v);
    let row0 = ui.rect(ui.widget("s", "row[0]").unwrap()).unwrap();
    // Wheel down over the list scrolls it.
    h.frame(&mut ui, &[mv(50.0, 50.0), Event::MouseWheel { delta: -1.0 }], &s, &v);
    h.frame(&mut ui, &[], &s, &v);
    let after = ui.rect(ui.widget("s", "row[0]").unwrap()).unwrap();
    assert!(after.y < row0.y - 10.0, "{row0:?} -> {after:?}");
    // Scroll can't go past the end.
    for _ in 0..50 {
        h.frame(&mut ui, &[Event::MouseWheel { delta: -1.0 }], &s, &v);
    }
    h.frame(&mut ui, &[], &s, &v);
    let last = ui.rect(ui.widget("s", "row[19]").unwrap()).unwrap();
    assert!((last.bottom() - 100.0).abs() < 0.5, "{last:?}");
    // Back to the top by wheel, then navigating down past the view scrolls it.
    for _ in 0..50 {
        h.frame(&mut ui, &[Event::MouseWheel { delta: 1.0 }], &s, &v);
    }
    // Move the mouse away so hover doesn't steer focus.
    h.frame(&mut ui, &[mv(390.0, 390.0)], &s, &v);
    for _ in 0..12 {
        h.frame(&mut ui, &[kd("Down")], &s, &v);
        h.frame(&mut ui, &[ku("Down")], &s, &v);
    }
    h.frame(&mut ui, &[], &s, &v);
    let r = ui.rect(ui.widget("s", "row[12]").unwrap()).unwrap();
    assert!(r.y >= -0.5 && r.bottom() <= 100.5, "focused row is in view: {r:?}");
    let tree = ui.tree().to_pretty();
    assert!(tree.contains("\"clipped\": true"), "rows outside the view are reported clipped");
}

#[test]
fn prompts_follow_the_last_device() {
    let root = r#"{"type": "box", "children": [{"type": "prompt", "id": "p", "action": "place", "text": "Build"}]}"#;
    let mut ui = ui_with("hud", root);
    let mut h = Harness::new();
    let s = StateMap::new();
    let v = view(400, 300);
    h.frame(&mut ui, &[], &s, &v);
    assert!(ui.tree().to_pretty().contains("\"LMB\""));
    h.pad = RawGamepad { buttons: GamepadButton::South.bit(), ..RawGamepad::default() };
    h.frame(&mut ui, &[], &s, &v);
    assert!(ui.tree().to_pretty().contains("\"A\""));
    // An action with no binding for the device hides the whole prompt (never a wrong glyph).
    let root = r#"{"type": "box", "children": [{"type": "prompt", "id": "p", "action": "speed", "text": "Speed"}]}"#;
    ui.load_screen(&screen("s", "hud", root)).unwrap();
    h.frame(&mut ui, &[], &s, &v);
    assert!(ui.rect(ui.widget("s", "p").unwrap()).is_none());
    h.pad = RawGamepad::default();
    h.frame(&mut ui, &[kd("F")], &s, &v);
    assert!(ui.rect(ui.widget("s", "p").unwrap()).is_some());
}

#[test]
fn agents_can_activate_by_id_and_tooltips_appear_after_a_delay() {
    let mut ui = menu_ui();
    let mut h = Harness::new();
    let s = StateMap::new();
    let v = view(800, 600);
    h.frame(&mut ui, &[], &s, &v);
    ui.activate(ui.widget("s", "quit").unwrap());
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(actions(&e), vec!["quit"]);
    // "quit" (which has a tooltip) is focused now; a key press makes keyboard focus visible, and
    // after the delay the tooltip's text is drawn.
    h.frame(&mut ui, &[kd("Down")], &s, &v);
    h.frame(&mut ui, &[ku("Down")], &s, &v);
    let glyphs = |ui: &Ui| ui.draw_list().glyphs.len();
    let before = glyphs(&ui);
    for _ in 0..40 {
        h.frame(&mut ui, &[], &s, &v);
    }
    assert!(glyphs(&ui) > before + 5, "{} -> {}", before, glyphs(&ui));
}

#[test]
fn hit_testing_matches_layout() {
    let mut ui = menu_ui();
    let mut h = Harness::new();
    let s = StateMap::new();
    h.frame(&mut ui, &[], &s, &view(800, 600));
    let c = centre(&ui, "settings");
    assert_eq!(ui.hit_test(c).and_then(|w| ui.widget_id(w)), Some("settings"));
    assert_eq!(ui.hit_test([1.0, 1.0]), None);
    assert!(ui.hits().any(|(_, w, interactive)| !interactive && ui.widget_id(w) == Some("panel")));
}
