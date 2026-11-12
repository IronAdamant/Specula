//! The agent surface (`specula_ui::agent`): paged trees, d-pad routes that a real pad then
//! walks, layout that leaves the player's UI untouched, outlines, pseudo-locale and the new
//! check rules (modal focus scope, menu paths for HUD buttons, untranslated words).

mod common;
use common::*;
use specula_ui::check::{check, check_with, CheckOpts, Severity, Target};
use specula_ui::geom::Dir;
use specula_ui::{ErrorCode, Only, StateMap, TreeOpts};

const MENU: &str = r#"{"type": "box", "children": [{"type": "panel", "anchor": "center", "layout": "column", "gap": 8, "children": [
    {"type": "button", "id": "a", "text": "Alpha", "action": "a"},
    {"type": "button", "id": "b", "text": "Bravo", "action": "b"},
    {"type": "box", "layout": "row", "gap": 8, "children": [
        {"type": "button", "id": "c", "text": "Charlie", "action": "c"},
        {"type": "button", "id": "d", "text": "Delta", "action": "d", "enabled": "d_on"}
    ]},
    {"type": "label", "id": "who", "text": "Hello {name}"}
]}]}"#;

fn menu_ui() -> specula_ui::Ui {
    let mut ui = ui();
    let json = screen("m", "menu", MENU).replacen(r#""input": "menu","#, r#""input": "menu", "focus": "a", "back": "close","#, 1);
    ui.load_screen(&json).unwrap_or_else(|e| panic!("{e}"));
    ui.show("m").unwrap();
    ui
}

fn state() -> StateMap {
    let mut s = StateMap::new();
    s.set_str("name", "Morwen").set_bool("d_on", true);
    s
}

#[test]
fn a_route_the_pad_then_walks_lands_on_the_widget() {
    let mut ui = menu_ui();
    let (s, v) = (state(), view(800, 600));
    let mut h = Harness::new();
    h.frame(&mut ui, &[], &s, &v);
    let d = ui.widget("m", "d").unwrap();
    let route = ui.focus_route(d).unwrap();
    assert!(!route.is_empty() && route.len() <= 4, "{route:?}");
    for dir in route {
        let k = match dir {
            Dir::Up => "Up",
            Dir::Down => "Down",
            Dir::Left => "Left",
            Dir::Right => "Right",
        };
        h.frame(&mut ui, &[kd(k)], &s, &v);
        h.frame(&mut ui, &[ku(k)], &s, &v);
    }
    assert_eq!(ui.focused(), Some(d), "the keys the route named moved the focus there");
    // A label can't take focus; a disabled button can't either.
    let e = ui.focus_route(ui.widget("m", "who").unwrap()).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotReachable, "{e}");
    let mut off = state();
    off.set_bool("d_on", false);
    h.frame(&mut ui, &[], &off, &v);
    let e = ui.focus_route(ui.widget("m", "c").unwrap()).unwrap();
    assert!(!e.is_empty());
    assert_eq!(ui.focus_route(d).unwrap_err().code, ErrorCode::NotReachable);
}

#[test]
fn routes_respect_the_modal_scope() {
    let mut ui = menu_ui();
    let modal = screen("q", "menu", r#"{"type": "box", "children": [{"type": "button", "id": "yes", "text": "Yes", "action": "yes", "anchor": "center"}]}"#)
        .replacen(r#""input": "menu","#, r#""input": "menu", "modal": true, "layer": 5, "focus": "yes", "back": "no","#, 1);
    ui.load_screen(&modal).unwrap();
    ui.show("q").unwrap();
    let (s, v) = (state(), view(800, 600));
    let mut h = Harness::new();
    h.frame(&mut ui, &[], &s, &v);
    assert_eq!(ui.top_modal(), Some("q"));
    assert_eq!(ui.focus_screen(), ui.screen_id("q").map(|i| i.0));
    let e = ui.focus_route(ui.widget("m", "b").unwrap()).unwrap_err();
    assert_eq!(e.code, ErrorCode::ModalBlocks, "{e}");
    assert_eq!(ui.widget_shown("yes"), ui.widget("q", "yes"));
    // Outlines mark what the modal blocks.
    let (o, _) = ui.outlines(100);
    assert!(o.iter().filter(|o| o.screen == "m").all(|o| o.blocked) && o.iter().any(|o| o.screen == "m"));
    assert!(o.iter().filter(|o| o.screen == "q").all(|o| !o.blocked));
}

#[test]
fn inspect_lays_out_elsewhere_and_puts_everything_back() {
    let mut ui = menu_ui();
    let (s, v) = (state(), view(800, 600));
    let mut h = Harness::new();
    h.frame(&mut ui, &[kd("Down")], &s, &v);
    h.frame(&mut ui, &[ku("Down")], &s, &v);
    let b = ui.widget("m", "b").unwrap();
    let before = (ui.rect(b), ui.focused(), ui.last_view(), ui.draw_list().cmds.len());
    let big = view(1920, 1080);
    let there = ui.inspect(&s, &big, |ui| (ui.rect(b), ui.last_view()));
    assert_eq!(there.1, Some(big));
    assert_ne!(there.0, before.0, "laid out for the other size");
    assert_eq!((ui.rect(b), ui.focused(), ui.last_view(), ui.draw_list().cmds.len()), before, "the player's layout is back");
    // A queued agent request survives an inspection and lands on the next real frame.
    ui.activate(ui.widget("m", "c").unwrap()).unwrap();
    ui.inspect(&s, &big, |_| ());
    let e = h.frame(&mut ui, &[], &s, &v);
    assert_eq!(e.actions().collect::<Vec<_>>(), ["c"]);
}

#[test]
fn the_tree_pages_and_flags() {
    let mut ui = menu_ui();
    let (s, v) = (state(), view(800, 600));
    Harness::new().frame(&mut ui, &[], &s, &v);
    let all = ui.tree_page(&TreeOpts { only: Only::Named, limit: 100, ..TreeOpts::default() });
    let total = all.get("total").and_then(|t| t.as_f64()).unwrap() as usize;
    assert!(total >= 5, "{}", all.to_pretty());
    let p1 = ui.tree_page(&TreeOpts { only: Only::Named, limit: 2, offset: 0, ..TreeOpts::default() });
    assert_eq!(p1.get("widgets").and_then(|w| w.as_array()).unwrap().len(), 2);
    assert_eq!(p1.get("next_offset").and_then(|t| t.as_f64()), Some(2.0));
    let last = ui.tree_page(&TreeOpts { only: Only::Named, limit: 100, offset: total - 1, ..TreeOpts::default() });
    assert_eq!(last.get("widgets").and_then(|w| w.as_array()).unwrap().len(), 1);
    assert!(last.get("next_offset").is_none());
    let buttons = ui.tree_page(&TreeOpts { only: Only::Interactive, ..TreeOpts::default() });
    let ws = buttons.get("widgets").and_then(|w| w.as_array()).unwrap();
    assert_eq!(ws.len(), 4);
    assert_eq!(ws[0].get("focused").and_then(|f| f.as_bool()), Some(true));
    assert_eq!(ws[0].get("action").and_then(|f| f.as_str()), Some("a"));
    // Huge limits are capped.
    let capped = ui.tree_page(&TreeOpts { limit: usize::MAX, ..TreeOpts::default() });
    assert!(capped.get("widgets").and_then(|w| w.as_array()).unwrap().len() <= specula_ui::agent::MAX_PAGE);
    // Outlines: ids and buttons, focus marked.
    let (o, left) = ui.outlines(3);
    assert_eq!((o.len(), left > 0), (3, true));
    assert!(ui.outlines(100).0.iter().any(|o| o.focused && o.id.as_deref() == Some("a")));
}

#[test]
fn text_that_does_not_fit_is_flagged_in_the_tree() {
    let root = r#"{"type": "box", "layout": "column", "anchor": "center", "children": [
        {"type": "label", "id": "long", "text": "a very long line of text indeed", "width": 40},
        {"type": "label", "id": "fits", "text": "ok"},
        {"type": "label", "id": "wraps", "text": "a long line that wraps onto more lines", "width": 120, "wrap": true}]}"#;
    let mut ui = ui_with("hud", root);
    Harness::new().frame(&mut ui, &[], &StateMap::new(), &view(800, 600));
    let t = ui.tree_page(&TreeOpts::default());
    let ws = t.get("widgets").and_then(|w| w.as_array()).unwrap();
    let flag = |id: &str| ws.iter().find(|w| w.get("id").and_then(|i| i.as_str()) == Some(id)).map(|w| w.get("overflow").is_some());
    assert_eq!(flag("long"), Some(true), "{}", t.to_pretty());
    assert_eq!(flag("fits"), Some(false), "text that fits isn't flagged");
    assert_eq!(flag("wraps"), Some(false), "a wrapping label grows down, it doesn't overflow sideways");
}

#[test]
fn radial_routes_step_around_the_ring() {
    let radial = r#"{"type": "box", "children": [{"type": "radial", "id": "r", "anchor": "center", "radius": 100, "inner_radius": 40, "items": [
        {"id": "i0", "label": "A", "action": "a"}, {"id": "i1", "label": "B", "action": "b"}, {"id": "i2", "label": "C", "action": "c"},
        {"id": "i3", "label": "D", "action": "d"}, {"id": "i4", "label": "E", "action": "e"}]}]}"#;
    let json = screen("rad", "menu", radial).replacen(r#""input": "menu","#, r#""input": "menu", "back": "close","#, 1);
    let mut ui = ui();
    ui.load_screen(&json).unwrap();
    ui.show("rad").unwrap();
    let (s, v) = (StateMap::new(), view(800, 600));
    let mut h = Harness::new();
    h.frame(&mut ui, &[], &s, &v);
    let start = ui.focused().expect("a radial starts focused");
    let at = |ui: &specula_ui::Ui, id: &str| ui.widget("rad", id).unwrap();
    let from = ui.widget_id(start).unwrap().to_owned();
    assert_eq!(from, "i0");
    // Two steps right, or (shorter for the far side) left around the ring.
    assert_eq!(ui.focus_route(at(&ui, "i2")).unwrap().len(), 2);
    assert_eq!(ui.focus_route(at(&ui, "i4")).unwrap().len(), 1, "one step back wraps to the last item");
    assert_eq!(ui.focus_step(start.screen, start.node, Dir::Right), Some(at(&ui, "i1").node));
    assert_eq!(ui.focus_step(start.screen, start.node, Dir::Left), Some(at(&ui, "i4").node));
    assert!(ui.focus_route(at(&ui, "i0")).unwrap().is_empty(), "already there");
}

#[test]
fn the_tree_filters_by_screen_and_marks_what_a_modal_blocks() {
    let mut ui = menu_ui();
    let modal = screen("q", "menu", r#"{"type": "box", "children": [{"type": "button", "id": "yes", "text": "Yes", "action": "yes", "anchor": "center"}]}"#)
        .replacen(r#""input": "menu","#, r#""input": "menu", "modal": true, "layer": 5, "focus": "yes", "back": "no","#, 1);
    ui.load_screen(&modal).unwrap();
    ui.show("q").unwrap();
    Harness::new().frame(&mut ui, &[], &state(), &view(800, 600));
    let only_q = ui.tree_page(&TreeOpts { screen: Some("q".into()), ..TreeOpts::default() });
    let ws = only_q.get("widgets").and_then(|w| w.as_array()).unwrap();
    assert!(!ws.is_empty() && ws.iter().all(|w| w.get("screen").and_then(|s| s.as_str()) == Some("q")), "{}", only_q.to_pretty());
    assert!(ws.iter().all(|w| w.get("blocked").is_none()), "the modal itself isn't blocked");
    // The screens list says which is modal, which it blocks, and which has the gamepad focus.
    let all = ui.tree_page(&TreeOpts::default());
    let scr = |name: &str| all.get("screens").and_then(|s| s.as_array()).unwrap().iter().find(|s| s.get("name").and_then(|n| n.as_str()) == Some(name)).cloned().unwrap();
    let b = |s: &specula_core::json::Json, k: &str| s.get(k).and_then(|v| v.as_bool());
    assert_eq!((b(&scr("q"), "modal"), b(&scr("q"), "has_focus"), b(&scr("q"), "blocked")), (Some(true), Some(true), None));
    assert_eq!((b(&scr("m"), "modal"), b(&scr("m"), "has_focus"), b(&scr("m"), "blocked")), (None, None, Some(true)));
    assert_eq!(scr("q").get("input").and_then(|i| i.as_str()), Some("menu"));
    let yes = all.get("widgets").and_then(|w| w.as_array()).unwrap().iter().find(|w| w.get("id").and_then(|i| i.as_str()) == Some("yes")).cloned().unwrap();
    assert_eq!((b(&yes, "focused"), b(&yes, "enabled")), (Some(true), Some(true)));
    let only_m = ui.tree_page(&TreeOpts { screen: Some("m".into()), only: Only::Interactive, ..TreeOpts::default() });
    let ws = only_m.get("widgets").and_then(|w| w.as_array()).unwrap();
    assert!(!ws.is_empty() && ws.iter().all(|w| w.get("blocked").and_then(|b| b.as_bool()) == Some(true)), "{}", only_m.to_pretty());
}

#[test]
fn pseudo_locale_lengthens_screen_text_and_names_words_from_state() {
    let mut ui = menu_ui();
    let (s, v) = (state(), view(800, 600));
    let opts = CheckOpts { pseudo: true, ..CheckOpts::default() };
    let f = check_with(&mut ui, &["m"], &s, None, &[Target::new("t", [800, 600], None)], &opts).unwrap();
    let u: Vec<_> = f.iter().filter(|f| f.rule == "untranslated").collect();
    assert_eq!(u.len(), 1, "{f:#?}");
    assert_eq!((u[0].widget.as_str(), u[0].severity), ("who", Severity::Warning));
    assert!(!ui.pseudo_locale(), "the switch is put back");
    // In the pseudo-locale, the label grows (brackets, accents, padding).
    ui.set_pseudo_locale(true);
    Harness::new().frame(&mut ui, &[], &s, &v);
    let t = ui.tree_page(&TreeOpts::default());
    let alpha = t.get("widgets").and_then(|w| w.as_array()).unwrap().iter().find(|w| w.get("text").and_then(|x| x.as_str()).is_some_and(|x| x.starts_with("[Álphá"))).cloned();
    assert!(alpha.is_some(), "{}", t.to_pretty());
    ui.set_pseudo_locale(false);
}

#[test]
fn a_modal_with_buttons_and_no_menu_focus_is_a_trap() {
    let mut ui = ui();
    let trap = screen("t", "hud", r#"{"type": "box", "children": [{"type": "button", "id": "ok", "text": "OK", "action": "ok", "shortcut": "place", "anchor": "center"}]}"#)
        .replacen(r#""input": "hud","#, r#""input": "hud", "modal": true,"#, 1);
    ui.load_screen(&trap).unwrap();
    let f = check(&mut ui, &["t"], &StateMap::new(), None, &[Target::deck()]).unwrap();
    assert!(f.iter().any(|f| f.rule == "focus_scope" && f.severity == Severity::Error), "{f:#?}");
}

#[test]
fn a_hud_button_whose_action_a_menu_offers_has_a_gamepad_path() {
    let mut ui = ui();
    let hud = screen("h", "hud", r#"{"type": "box", "children": [{"type": "button", "id": "spd", "text": "x2", "action": "speed", "anchor": "top_right"}]}"#);
    ui.load_screen(&hud).unwrap();
    let f = check(&mut ui, &["h"], &StateMap::new(), None, &[Target::deck()]).unwrap();
    assert!(f.iter().any(|f| f.rule == "gamepad_access" && f.widget == "spd"), "{f:#?}");
    // Review R1 of m5-uitools, finding 9: a menu offering the action counts only when a gamepad
    // gets there. Loaded and never opened (Sepulcra's title screen), it is no path.
    let menu = |opened_by: &str| {
        screen("p", "menu", r#"{"type": "box", "children": [{"type": "button", "id": "s", "text": "Speed", "action": "speed", "anchor": "center"}, {"type": "button", "id": "o", "text": "Options", "action": "open_options", "anchor": "top"}]}"#)
            .replacen(r#""input": "menu","#, &format!(r#""input": "menu", "focus": "s", "back": "close"{opened_by},"#), 1)
    };
    let map = specula_input::ActionMap::from_json_str(
        r#"{"format": "specula.input.v1", "actions": [{"name": "pause", "kind": "button", "bindings": ["Pad.Start", "Key.P"]}, {"name": "help", "kind": "button", "bindings": ["Key.F1"]}]}"#,
    )
    .unwrap();
    let access = |ui: &mut specula_ui::Ui| check(ui, &["h"], &StateMap::new(), Some(&map), &[Target::deck()]).unwrap().into_iter().filter(|f| f.rule == "gamepad_access").count();
    ui.load_screen(&menu("")).unwrap();
    assert_eq!(access(&mut ui), 1, "a menu nothing opens");
    // Opened by an action only a key presses: still no gamepad path.
    let mut keys = two_screens(&hud, &menu(r#", "opened_by": ["help"]"#));
    assert_eq!(access(&mut keys), 1, "opened by a keyboard-only action");
    // Opened by Start: a path.
    let mut pad = two_screens(&hud, &menu(r#", "opened_by": ["pause", "help"]"#));
    assert_eq!(access(&mut pad), 0, "opened by Start");
    // Shown now: a path (the player is looking at it).
    let mut shown = two_screens(&hud, &menu(""));
    shown.show("p").unwrap();
    assert_eq!(check(&mut shown, &["h", "p"], &StateMap::new(), Some(&map), &[Target::deck()]).unwrap().iter().filter(|f| f.rule == "gamepad_access").count(), 0, "shown");
    // A chain: Start opens a menu whose button opens the one offering the action.
    let mut chain = two_screens(&hud, &menu(r#", "opened_by": ["open_options"]"#).replace(r#""name": "p""#, r#""name": "q""#));
    assert_eq!(access(&mut chain), 1, "only a menu that opens itself");
    let root = screen("r", "menu", r#"{"type": "box", "children": [{"type": "button", "id": "opt", "text": "Options", "action": "open_options", "anchor": "center"}]}"#)
        .replacen(r#""input": "menu","#, r#""input": "menu", "focus": "opt", "back": "close", "opened_by": ["pause"],"#, 1);
    chain.load_screen(&root).unwrap();
    assert_eq!(access(&mut chain), 0, "Start, then Options");
    // Hostile lists are refused with the field's path.
    let long: Vec<String> = (0..17).map(|i| format!("\"a{i}\"")).collect();
    let bad = menu(&format!(r#", "opened_by": [{}]"#, long.join(",")));
    assert!(common::ui().load_screen(&bad).unwrap_err().to_string().contains("opened_by"));
    let twice = menu(r#", "opened_by": ["pause", "pause"]"#);
    assert!(common::ui().load_screen(&twice).unwrap_err().to_string().contains("twice"));
}

fn two_screens(a: &str, b: &str) -> specula_ui::Ui {
    let mut ui = ui();
    ui.load_screen(a).unwrap();
    ui.load_screen(b).unwrap();
    ui
}

#[test]
fn presets_cover_the_aspects() {
    for p in specula_ui::check::PRESETS {
        let t = Target::preset(p).unwrap_or_else(|| panic!("{p}"));
        assert!(t.size[0] > 0 && t.size[1] > 0);
    }
    assert!(Target::preset("8k").is_none());
    let d = Target::deck().at_scale(1.5);
    assert_eq!((d.name.as_str(), d.user_scale), ("deck_1280x800@1.5x", 1.5));
    assert_eq!(Target::deck().at_scale(1.0).name, "deck_1280x800");
}

/// Review R1 of m5-uitools, finding 2: a capture's copy of the interface takes the player's
/// focus (matched by screen name; a screen whose nodes differ is left alone).
#[test]
fn a_copy_takes_the_players_focus_by_screen() {
    let mut live = menu_ui();
    let view = view(1280, 800);
    let mut h = Harness::new();
    h.frame(&mut live, &[], &state(), &view);
    let c = live.widget_shown("c").unwrap();
    live.focus(c).unwrap();
    h.frame(&mut live, &[], &state(), &view);
    assert_eq!(live.focused().and_then(|w| live.widget_id(w)), Some("c"));
    let mut copy = menu_ui();
    h.frame(&mut copy, &[], &state(), &view);
    assert_eq!(copy.focused().and_then(|w| copy.widget_id(w)), Some("a"), "the screen's default");
    copy.copy_interaction_from(&live, 2.0);
    assert_eq!(copy.focused().and_then(|w| copy.widget_id(w)), Some("c"), "the player's");
    // Same name, other nodes: not touched.
    let mut other = ui();
    let json = screen("m", "menu", r#"{"type": "box", "children": [{"type": "button", "id": "a", "text": "A", "action": "a"}, {"type": "button", "id": "z", "text": "Z", "action": "z"}]}"#)
        .replacen(r#""input": "menu","#, r#""input": "menu", "focus": "z","#, 1);
    other.load_screen(&json).unwrap();
    other.show("m").unwrap();
    h.frame(&mut other, &[], &state(), &view);
    other.copy_interaction_from(&live, 1.0);
    assert_eq!(other.focused().and_then(|w| other.widget_id(w)), Some("z"));
}
