//! Localisation in the toolkit: `@key` texts, messages in state, plural forms and number formats
//! through a string table, per-locale faces, and `ui_check` in another language and the
//! pseudo-locale (docs/design/LOCALISATION.md).

mod common;
use common::*;
use specula_lingua::{Locale, Table};
use specula_ui::check::{check_with, CheckOpts, Target};
use specula_ui::state::{Scope, Template, MAX_TEXT};
use specula_ui::{StateMap, Strings, TreeOpts, Ui};
use std::sync::Arc;

const EN: &str = r#"{"format": "specula.strings.v1", "locale": "en", "name": "English", "source": true, "strings": {
    "hud.wave": "Wave {wave}",
    "hud.menu": "Menu",
    "hud.enemies": {"plural": "n", "one": "{n} enemy", "other": "{n} enemies"},
    "hud.dust": "{dust} dust",
    "hud.seed": "Seed {seed:plain}",
    "hint.cost": "Costs {cost} dust",
    "hint.who": "Built by {who}",
    "nest": "<{inner}>"
}}"#;

const DE: &str = r#"{"format": "specula.strings.v1", "locale": "de", "name": "Deutsch", "strings": {
    "hud.wave": "Welle {wave}",
    "hud.menu": "Menü",
    "hud.enemies": {"plural": "n", "one": "{n} Feind", "other": "{n} Feinde"},
    "hint.cost": "Kostet {cost} Staub"
}}"#;

const RU: &str = r#"{"format": "specula.strings.v1", "locale": "ru", "name": "Русский", "strings": {
    "hud.enemies": {"plural": "n", "one": "{n} враг", "few": "{n} врага", "many": "{n} врагов", "other": "{n} врага"}
}}"#;

fn tables() -> Vec<Arc<Table>> {
    [EN, DE, RU].iter().map(|t| Arc::new(Table::parse(t).unwrap())).collect()
}

fn strings(locale: &str) -> Arc<Strings> {
    Arc::new(Strings::build(&Locale::parse(locale).unwrap(), &tables()).unwrap())
}

fn render(text: &str, state: &StateMap, s: Option<&Strings>) -> String {
    let mut out = String::new();
    Template::parse(text, "t").unwrap().render(&Scope::new(state).with_strings(s), &mut out);
    out
}

#[test]
fn key_texts_show_the_tables_words_in_its_language() {
    let mut st = StateMap::new();
    st.set_int("wave", 3).set_int("n", 1).set_int("dust", 1234).set_int("seed", 123456);
    let (en, de, ru) = (strings("en"), strings("de"), strings("ru"));
    assert_eq!(render("@hud.wave", &st, Some(&en)), "Wave 3");
    assert_eq!(render("@hud.wave", &st, Some(&de)), "Welle 3");
    assert_eq!(render("@hud.wave", &st, Some(&ru)), "Wave 3", "ru lacks it: the source's words");
    assert_eq!(render("@hud.enemies", &st, Some(&de)), "1 Feind");
    st.set_int("n", 5);
    assert_eq!(render("@hud.enemies", &st, Some(&de)), "5 Feinde");
    assert_eq!(render("@hud.enemies", &st, Some(&ru)), "5 врагов");
    st.set_int("n", 22);
    assert_eq!(render("@hud.enemies", &st, Some(&ru)), "22 врага");
    // Numbers take the locale's format with a table, and stay plain ASCII without one.
    assert_eq!(render("@hud.dust", &st, Some(&en)), "1,234 dust");
    assert_eq!(render("@hud.dust", &st, Some(&de)), "1.234 dust");
    assert_eq!(render("{dust} dust", &st, None), "1234 dust", "no table: as before tables");
    assert_eq!(render("@hud.seed", &st, Some(&de)), "Seed 123456", ":plain is never grouped");
    // A key the table lacks shows as @key; so does any key with no table.
    assert_eq!(render("@hud.nope", &st, Some(&en)), "@hud.nope");
    assert_eq!(render("@hud.wave", &st, None), "@hud.wave");
    // @@ is a literal @; a bad key is a structured error.
    assert_eq!(render("@@handle {wave}", &st, None), "@handle 3");
    assert!(Template::parse("@bad key", "root.text").is_err());
    assert!(Template::parse("@1x", "root.text").is_err());
}

/// Review R2 of m5-l10n, major 3: the pseudo-locale is made once per `Strings` (and can be made
/// ahead, off the engine thread), so switching a UI to it and back takes a reference.
#[test]
fn the_pseudo_locale_is_made_once_and_kept() {
    let s = strings("en");
    assert!(!s.pseudo_ready());
    s.warm_pseudo();
    assert!(s.pseudo_ready());
    let (a, b) = (s.pseudo().unwrap(), s.pseudo().unwrap());
    assert!(std::sync::Arc::ptr_eq(&a, &b), "the same strings each time");
    let mut st = StateMap::new();
    st.set_int("wave", 3).set_int("waves", 10);
    let shown = render("@hud.wave", &st, Some(&a));
    assert!(shown != render("@hud.wave", &st, Some(&s)) && shown.contains('3'), "{shown}");
}

#[test]
fn messages_in_state_are_shown_in_the_uis_language() {
    let mut st = StateMap::new();
    st.set_msg("hint", "hint.cost").set_int("cost", 1200);
    assert_eq!(render("{hint}", &st, Some(&strings("de"))), "Kostet 1.200 Staub");
    assert_eq!(render("{hint}", &st, Some(&strings("en"))), "Costs 1,200 dust");
    assert_eq!(render("{hint}", &st, None), "@hint.cost");
    // Setting it again reuses the message and clears its old arguments.
    st.set_msg("hint", "hint.who").set_str("who", "Ada");
    assert_eq!(render("{hint}", &st, Some(&strings("en"))), "Built by Ada");
    // Messages nest MAX_DEPTH deep, as the design says (review R2 of m5-l10n: each level cost
    // two, so the third message down showed its key): exactly MAX_DEPTH render, one more doesn't.
    let nested = |levels: usize| {
        let mut st = StateMap::new();
        {
            let mut at = st.set_msg("x", "nest");
            for _ in 1..levels {
                at = at.set_msg("inner", "nest");
            }
            at.set_str("inner", "core");
        }
        render("{x}", &st, Some(&strings("en")))
    };
    let depth = usize::from(specula_ui::state::MAX_DEPTH);
    assert_eq!(nested(1), "<core>");
    assert_eq!(nested(depth), format!("{}core{}", "<".repeat(depth), ">".repeat(depth)));
    assert_eq!(nested(depth + 1), format!("{}@nest{}", "<".repeat(depth), ">".repeat(depth)), "one past the limit shows its key");
    // Hostile nesting is bounded: depth, and the text cap.
    let mut deep = StateMap::new();
    {
        let mut at = deep.set_msg("x", "nest");
        for _ in 0..64 {
            at = at.set_msg("inner", "nest");
        }
        at.set_str("inner", &"y".repeat(10_000));
    }
    let out = render("{x}{x}{x}{x}", &deep, Some(&strings("en")));
    assert!(out.len() <= MAX_TEXT + 3, "{} bytes", out.len());
    assert!(out.contains("@nest"), "past the depth limit a message shows its key: {out}");
}

fn hud_ui() -> Ui {
    let mut ui = ui();
    let root = r#"{"type": "box", "children": [{"type": "panel", "anchor": "center", "layout": "column", "children": [
        {"type": "label", "id": "w", "text": "@hud.wave"},
        {"type": "label", "id": "e", "text": "@hud.enemies"},
        {"type": "label", "id": "h", "text": "{hint}"},
        {"type": "label", "id": "lit", "text": "Hard-coded words"},
        {"type": "label", "id": "gone", "text": "@hud.gone"}]}]}"#;
    ui.load_screen(&screen("s", "hud", root)).unwrap();
    ui.show("s").unwrap();
    ui
}

fn hud_state() -> StateMap {
    let mut st = StateMap::new();
    st.set_int("wave", 2).set_int("n", 3);
    st.set_msg("hint", "hint.cost").set_int("cost", 60);
    st
}

#[test]
fn checks_name_missing_keys_written_words_and_run_in_other_languages() {
    let mut ui = hud_ui();
    let st = hud_state();
    let t = [Target::new("t", [1280, 800], None)];
    // Without a table: no l10n findings (the game isn't localised).
    let f = check_with(&mut ui, &["s"], &st, None, &t, &CheckOpts::default()).unwrap();
    assert!(!f.iter().any(|f| f.rule == "hardcoded" || f.rule == "missing_key"), "{f:#?}");
    let en = strings("en");
    ui.set_strings(Some(Arc::clone(&en)));
    let f = check_with(&mut ui, &["s"], &st, None, &t, &CheckOpts::default()).unwrap();
    assert!(f.iter().any(|f| f.rule == "missing_key" && f.widget == "gone" && f.severity == specula_ui::Severity::Error), "{f:#?}");
    assert!(f.iter().any(|f| f.rule == "hardcoded" && f.widget == "lit"), "{f:#?}");
    assert!(!f.iter().any(|f| f.rule == "hardcoded" && f.widget != "lit"), "@key texts and messages aren't written words: {f:#?}");
    // In another language for the check alone: the UI's table is put back.
    let de = strings("de");
    let opts = CheckOpts { strings: Some(Arc::clone(&de)), ..CheckOpts::default() };
    check_with(&mut ui, &["s"], &st, None, &t, &opts).unwrap();
    assert!(Arc::ptr_eq(ui.strings().unwrap(), &en), "the player's table is back");
    // The pseudo-locale comes from the source; messages are translated, plain state words aren't.
    let mut st2 = hud_state();
    st2.set_str("who", "Ada Lovelace");
    let f = check_with(&mut ui, &["s"], &st2, None, &t, &CheckOpts { pseudo: true, ..CheckOpts::default() }).unwrap();
    assert!(!f.iter().any(|f| f.rule == "untranslated" && f.widget == "h"), "a message is the table's: {f:#?}");
    assert!(Arc::ptr_eq(ui.strings().unwrap(), &en) && !ui.pseudo_locale());
}

#[test]
fn the_tree_names_each_labels_key_and_shows_the_language() {
    let mut ui = hud_ui();
    let st = hud_state();
    let v = view(1280, 800);
    ui.set_strings(Some(strings("de")));
    Harness::new().frame(&mut ui, &[], &st, &v);
    let t = ui.tree_page(&TreeOpts::default());
    let ws = t.get("widgets").and_then(|w| w.as_array()).unwrap();
    let w = ws.iter().find(|w| w.get("id").and_then(|i| i.as_str()) == Some("w")).unwrap();
    assert_eq!(w.get("text").and_then(|x| x.as_str()), Some("Welle 2"));
    assert_eq!(w.get("text_key").and_then(|x| x.as_str()), Some("hud.wave"));
    let h = ws.iter().find(|w| w.get("id").and_then(|i| i.as_str()) == Some("h")).unwrap();
    assert_eq!(h.get("text").and_then(|x| x.as_str()), Some("Kostet 60 Staub"));
    // The pseudo-locale while it is on, then German again.
    ui.set_pseudo_locale(true);
    Harness::new().frame(&mut ui, &[], &st, &v);
    let t = ui.tree_page(&TreeOpts::default()).to_string();
    assert!(t.contains("[Wávé 2"), "{t}");
    ui.set_pseudo_locale(false);
    Harness::new().frame(&mut ui, &[], &st, &v);
    assert!(ui.tree_page(&TreeOpts::default()).to_string().contains("Welle 2"));
}

#[test]
fn a_locales_faces_take_their_slots_while_its_table_is_set() {
    let mut ui = ui();
    let base = ui.fonts().index("ui");
    let src = Arc::new(Table::parse(EN).unwrap());
    let catalog = specula_lingua::Catalog::build(&Locale::parse("en").unwrap(), &[&*src]).unwrap();
    let face = Arc::new(specula_ui::testfont::box_font_parsed());
    let s = Arc::new(Strings::new(catalog, Some(src), vec![("ui".into(), Arc::clone(&face))]));
    ui.set_strings(Some(s));
    let i = ui.fonts().index("ui").unwrap();
    assert_ne!(Some(i), base);
    assert_eq!(ui.fonts().font(i).id(), face.id());
    ui.set_strings(None);
    assert_eq!(ui.fonts().index("ui"), base, "the slot's own face is back");
}

/// Review of m5-l10n, finding 5: two locales giving faces for one slot share its index, and the
/// labels' layout cache kept the first face's glyphs. Layouts are keyed by the face now.
#[test]
fn switching_between_two_locales_faces_lays_text_out_again() {
    let src = Arc::new(Table::parse(EN).unwrap());
    let with_face = |b: specula_text::Bundled| {
        let catalog = specula_lingua::Catalog::build(&Locale::parse("en").unwrap(), &[&*src]).unwrap();
        Arc::new(Strings::new(catalog, Some(Arc::clone(&src)), vec![("ui".into(), Arc::new(b.load().unwrap()))]))
    };
    let (a, b) = (with_face(specula_text::Bundled::Display), with_face(specula_text::Bundled::UiBold));
    let st = hud_state();
    let v = view(1280, 800);
    let rect_of = |ui: &mut Ui| {
        Harness::new().frame(ui, &[], &st, &v);
        Harness::new().frame(ui, &[], &st, &v);
        ui.rect(ui.widget("s", "w").unwrap()).unwrap()
    };
    let mut ui = hud_ui();
    ui.set_strings(Some(Arc::clone(&a)));
    let ra = rect_of(&mut ui);
    ui.set_strings(Some(Arc::clone(&b)));
    let switched = rect_of(&mut ui);
    let mut fresh = hud_ui();
    fresh.set_strings(Some(b));
    assert_eq!(switched, rect_of(&mut fresh), "after the switch the label is laid out with the new face");
    assert_ne!(ra, switched, "the two faces differ in width");
}

/// Japanese text with the system's Japanese face for the "ui" slot: no missing glyphs (skips
/// without a CJK face, as on CI).
#[test]
fn a_cjk_locale_uses_the_systems_face() {
    if specula_text::system_fallback_candidates("ja").is_empty() {
        eprintln!("skipped: no system CJK face on this machine");
        return;
    }
    let ja = r#"{"format": "specula.strings.v1", "locale": "ja", "name": "日本語", "fonts": {"ui": "system:ja"}, "strings": {"hud.wave": "ウェーブ {wave}", "hud.menu": "メニュー"}}"#;
    let tables: Vec<Arc<Table>> = [EN, ja].iter().map(|t| Arc::new(Table::parse(t).unwrap())).collect();
    let catalog = specula_lingua::Catalog::build(&Locale::parse("ja").unwrap(), &tables.iter().map(|t| &**t).collect::<Vec<_>>()).unwrap();
    let (fonts, problems) = Strings::load_fonts(&catalog);
    assert!(problems.is_empty(), "{problems:?}");
    let mut ui = hud_ui();
    ui.set_strings(Some(Arc::new(Strings::new(catalog, Some(Arc::clone(&tables[0])), fonts))));
    let f = check_with(&mut ui, &["s"], &hud_state(), None, &[Target::deck()], &CheckOpts::default()).unwrap();
    assert!(!f.iter().any(|f| f.rule == "missing_glyph"), "{f:#?}");
}
