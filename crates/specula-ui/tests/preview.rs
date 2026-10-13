//! The software preview is deterministic: the same screens, state and input give the same pixels.

mod common;
use common::*;
use specula_ui::preview::{hash, render, to_png};
use specula_ui::{FontSet, StateMap, Theme, ToastKind, Ui, Viewport};

fn sepulcra(screens: &[&str]) -> Ui {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/ui");
    let theme = Theme::from_json_str(&std::fs::read_to_string(dir.join("theme.json")).unwrap()).unwrap();
    let mut ui = Ui::new(theme, FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()));
    for n in screens {
        ui.load_screen(&std::fs::read_to_string(dir.join(format!("{n}.json"))).unwrap()).unwrap();
        ui.show(n).unwrap();
    }
    ui
}

fn state() -> StateMap {
    let mut s = StateMap::new();
    s.set_int("wave", 4).set_int("waves", 10).set_num("next_wave_s", 23.0).set_int("dust", 135).set_int("tombstones", 7);
    s.set_int("tombstones_max", 10).set_int("speed", 2).set_str("selected", "pillar").set_str("selected_name", "Rune pillar");
    s.set_str("selected_role", "Slows").set_str("selected_detail", "Slows 35 % for 1.5 s.").set_int("selected_cost", 60);
    s.set_bool("affordable", true).set_int("early_bonus", 11);
    let t = s.list_mut("threats");
    t.push().set_num("x", 1500.0).set_num("y", 300.0).set_str("label", "East");
    s
}

fn shot(screens: &[&str]) -> specula_core::png::Image {
    let mut ui = sepulcra(screens);
    let mut h = Harness::new();
    ui.set_time(Some(1.25));
    ui.toast("A toast", ToastKind::Good);
    let s = state();
    let v = Viewport::new([640, 400], 0.5);
    for _ in 0..3 {
        h.frame(&mut ui, &[mv(320.0, 150.0)], &s, &v);
    }
    render(ui.draw_list(), ui.atlas(), &[], [0.04, 0.06, 0.12, 1.0])
}

#[test]
fn the_same_inputs_give_the_same_pixels() {
    let a = shot(&["hud", "radial"]);
    let b = shot(&["hud", "radial"]);
    assert_eq!(hash(&a), hash(&b));
    assert_ne!(hash(&a), hash(&shot(&["hud"])));
    // The PNG round-trips.
    let png = to_png(&a);
    let back = specula_core::png::decode(&png).unwrap();
    assert_eq!(back, a);
}

#[test]
fn a_menu_preview_is_pinned() {
    // Rects and text only (no sin / cos in meshes), so the bytes are the same on every
    // platform: the rasteriser and layout use + − × ÷ and sqrt, which IEEE 754 makes exact.
    let mut ui = ui();
    let root = r#"{"type": "box", "children": [{"type": "panel", "anchor": "center", "layout": "column", "gap": 6, "align": "stretch", "children": [
        {"type": "label", "text": "Paused", "role": "title"},
        {"type": "button", "id": "a", "text": "Resume", "action": "a"},
        {"type": "button", "id": "b", "text": "Quit", "action": "b", "enabled": false}]}]}"#;
    ui.load_screen(&screen("s", "menu", root)).unwrap();
    ui.show("s").unwrap();
    let mut h = Harness::new();
    h.frame(&mut ui, &[kd("Down")], &StateMap::new(), &view(320, 200));
    let img = render(ui.draw_list(), ui.atlas(), &[], [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(hash(&img), hash(&render(ui.draw_list(), ui.atlas(), &[], [0.0, 0.0, 0.0, 1.0])));
    assert_eq!(hash(&img), 6394361685778343501, "pinned hash changed");
}
