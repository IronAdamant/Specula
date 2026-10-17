//! Validation quality: every error names the item and field path, says what was expected and
//! what was found, and suggests the fix. These are the messages agents learn from.

use specula_core::Json;
use specula_lex::{compile, validate, Diagnostic, Document, Options};

fn doc(items: &str) -> Document {
    Document::parse(&format!(r#"{{"lex": 1, "name": "t", "tick_hz": 30, "items": {{{items}}}}}"#)).unwrap_or_else(|d| panic!("{d:?}"))
}

fn diags(items: &str) -> Vec<Diagnostic> {
    validate(&doc(items), &Options::default())
}

/// The one diagnostic with `code` (panics with all of them otherwise).
fn one<'a>(ds: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    let found: Vec<&Diagnostic> = ds.iter().filter(|d| d.code == code).collect();
    assert_eq!(found.len(), 1, "expected one '{code}', got:\n{}", ds.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("\n"));
    found[0]
}

const TOWER: &str = r#"
    "foe": {"kind": "archetype", "tags": ["enemy"], "attributes": {"hp": "10pt"}, "behaviours": [{"type": "health", "attribute": "hp"}]},
    "tower": {"kind": "archetype", "behaviours": [ATTACK]}
"#;

fn tower(attack: &str) -> Vec<Diagnostic> {
    diags(&TOWER.replace("ATTACK", attack))
}

#[test]
fn a_valid_set_has_no_diagnostics() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": "0.8s", "payload": [{"damage": 10}]}"#);
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn unknown_kind_suggests_the_closest() {
    let d = one(&diags(r#""a": {"kind": "archtype"}"#), "unknown_ref").clone();
    assert_eq!((d.item.as_str(), d.path.as_str()), ("a", "kind"));
    assert_eq!(d.suggestion.as_deref(), Some("did you mean 'archetype'?"));
    assert!(d.expected.unwrap().contains("resource"));
}

#[test]
fn unknown_fields_name_their_path_and_the_fix() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "rnage": 7, "range": 7, "cooldown": 1, "payload": []}"#);
    let d = one(&ds, "unknown_field");
    assert_eq!((d.item.as_str(), d.path.as_str()), ("tower", "behaviours[0].rnage"));
    assert_eq!(d.suggestion.as_deref(), Some("did you mean 'range'?"));
}

#[test]
fn missing_fields_say_what_to_add() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": 7, "payload": []}"#);
    let d = one(&ds, "missing_field");
    assert_eq!(d.path, "behaviours[0].cooldown");
    assert_eq!(d.suggestion.as_deref(), Some("add \"cooldown\""));
}

#[test]
fn a_number_without_its_unit_gets_the_literal_to_write() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": "7", "cooldown": 1, "payload": []}"#);
    let d = one(&ds, "unit_mismatch");
    assert_eq!(d.path, "behaviours[0].range");
    assert_eq!(d.suggestion.as_deref(), Some("write 7m (metres)"));
    assert_eq!(d.got.as_deref(), Some("a plain number"));
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": "7m", "payload": []}"#);
    let d = one(&ds, "unit_mismatch");
    assert_eq!(d.expected.as_deref(), Some("a duration value"));
    assert_eq!(d.got.as_deref(), Some("a distance value"));
}

#[test]
fn references_are_checked_with_kind_and_spelling() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": 1, "projectile": {"archetype": "bolts"}, "payload": []}"#);
    let d = one(&ds, "unknown_ref");
    assert_eq!(d.path, "behaviours[0].projectile.archetype");
    assert!(d.message.contains("no archetype 'bolts'"), "{}", d.message);
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": 1, "payload": [{"effect": "foe"}]}"#);
    let d = one(&ds, "wrong_kind");
    assert!(d.message.contains("'foe' is an archetype, but an effect is expected"), "{}", d.message);
}

#[test]
fn a_projectile_archetype_must_fly() {
    let ds = diags(&format!(
        "{}, \"rock\": {{\"kind\": \"archetype\"}}",
        TOWER.replace("ATTACK", r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": 1, "projectile": {"archetype": "rock"}, "payload": []}"#)
    ));
    let d = one(&ds, "invalid");
    assert!(d.message.contains("no projectile behaviour"));
    assert!(d.suggestion.as_deref().unwrap().contains("\"type\": \"projectile\""));
}

#[test]
fn extends_cycles_are_reported() {
    let ds = diags(r#""a": {"kind": "archetype", "extends": "b"}, "b": {"kind": "archetype", "extends": "a"}"#);
    assert!(ds.iter().any(|d| d.code == "cycle" && d.message.contains("a → b → a")), "{ds:?}");
}

#[test]
fn an_attribute_has_one_unit_everywhere() {
    let ds = diags(r#""a": {"kind": "archetype", "attributes": {"speed": "1m/s"}}, "b": {"kind": "archetype", "attributes": {"speed": "1m"}}"#);
    let d = one(&ds, "unit_mismatch");
    assert_eq!((d.item.as_str(), d.path.as_str()), ("b", "attributes.speed"));
    assert!(d.message.contains("distance here but speed in 'a'"), "{}", d.message);
}

#[test]
fn names_used_where_they_dont_exist_say_where_they_do() {
    let ds = diags(r#""foe": {"kind": "archetype", "attributes": {"hp": "1pt"}, "behaviours": [{"type": "health", "attribute": "hp", "on_death": [{"damage": "1pt", "to": "target"}]}]}"#);
    let d = one(&ds, "not_available");
    assert_eq!(d.path, "behaviours[0].on_death[0].to");
    assert!(d.suggestion.as_deref().unwrap().contains("attack payloads"));
}

#[test]
fn inexact_durations_warn_with_both_neighbours() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": 0.25, "payload": []}"#);
    let d = one(&ds, "inexact");
    assert!(!d.is_error());
    assert_eq!(d.suggestion.as_deref(), Some("use \"7t\" or \"8t\" for an exact value"));
    // A warning doesn't stop compilation.
    assert!(compile(&doc(&TOWER.replace("ATTACK", r#"{"type": "attack", "targets": ["enemy"], "range": 7, "cooldown": 0.25, "payload": []}"#)), &Options::default()).is_ok());
}

#[test]
fn param_overrides_are_range_checked() {
    let d = doc(r#""speed": {"kind": "param", "type": "speed", "value": 1, "min": 0.5, "max": 2}"#);
    let ok = Options { overrides: vec![("speed".into(), Json::from(1.5))], natives: None };
    let c = compile(&d, &ok).unwrap();
    assert_eq!(c.rules.params[0].value, 1500);
    let bad = Options { overrides: vec![("speed".into(), Json::from(3.0))], natives: None };
    let ds = compile(&d, &bad).unwrap_err();
    let e = one(&ds, "out_of_range");
    assert_eq!(e.expected.as_deref(), Some("0.5m/s..=2m/s"));
    let typo = Options { overrides: vec![("sped".into(), Json::from(1.0))], natives: None };
    let ds = compile(&d, &typo).unwrap_err();
    assert_eq!(one(&ds, "unknown_ref").suggestion.as_deref(), Some("did you mean 'speed'?"));
    // The override changes the rules' hash.
    assert_ne!(c.rules.hash, compile(&d, &Options::default()).unwrap().rules.hash);
}

#[test]
fn natives_need_a_reason_and_a_provider() {
    let ds = diags(r#""h": {"kind": "native", "hook": "game.route"}"#);
    let d = one(&ds, "missing_field");
    assert!(d.suggestion.as_deref().unwrap().contains("why no Lex building block fits"));
    let d = doc(r#""h": {"kind": "native", "hook": "game.route", "reason": "graph search"}"#);
    let ds = validate(&d, &Options { natives: Some(vec![]), ..Default::default() });
    assert_eq!(one(&ds, "native_unbound").severity, specula_lex::Severity::Warning);
    assert!(validate(&d, &Options { natives: Some(vec!["game.route".into()]), ..Default::default() }).is_empty());
}

#[test]
fn many_errors_come_back_in_one_pass() {
    let ds = diags(
        r#""a": {"kind": "resource"},
           "b": {"kind": "archetype", "attributes": {"hp": "1pt"}, "behaviours": [{"type": "helth"}]},
           "c": {"kind": "outcome", "result": "won", "when": "res.gold > 0"},
           "d": {"kind": "timer", "do": [{"add": "a", "amount": "1m"}]}"#,
    );
    let errors: Vec<&Diagnostic> = ds.iter().filter(|d| d.is_error()).collect();
    assert!(errors.len() >= 4, "{errors:#?}");
    let items: Vec<&str> = errors.iter().map(|d| d.item.as_str()).collect();
    for i in ["a", "b", "c", "d"] {
        assert!(items.contains(&i), "an error for {i}: {items:?}");
    }
}

#[test]
fn rule_events_and_expressions_point_at_the_problem() {
    let ds = diags(r#""r": {"kind": "rule", "on": "dide", "do": []}"#);
    assert_eq!(one(&ds, "unknown_ref").suggestion.as_deref(), Some("did you mean 'died'?"));
    let ds = diags(r#""gold": {"kind": "resource", "start": 0}, "o": {"kind": "outcome", "result": "won", "when": "res.gold >= 10 &&"}"#);
    let d = one(&ds, "syntax");
    assert_eq!(d.path, "when");
    assert_eq!(d.column, Some(18), "just past the end, where the value is missing");
    assert!(d.message.contains("in \"res.gold >= 10 &&\""));
}

#[test]
fn actions_have_exactly_one_verb() {
    let ds = diags(r#""gold": {"kind": "resource", "start": 0}, "t": {"kind": "timer", "do": [{"add": "gold", "amount": 1, "set": "gold", "value": 2}]}"#);
    let d = one(&ds, "invalid");
    assert!(d.message.contains("exactly one verb"), "{}", d.message);
    let ds = diags(r#""gold": {"kind": "resource", "start": 0}, "t": {"kind": "timer", "do": [{"ad": "gold", "amount": 1}]}"#);
    let d = one(&ds, "invalid");
    assert_eq!(d.suggestion.as_deref(), Some("did you mean 'add'?"));
}

#[test]
fn commands_check_their_args() {
    let ds = diags(r#""gold": {"kind": "resource", "start": 0}, "c": {"kind": "command", "args": {"n": "int"}, "require": [{"when": "arg.m > 0", "code": "x"}], "do": []}"#);
    let d = one(&ds, "unknown_ref");
    assert_eq!(d.suggestion.as_deref(), Some("did you mean 'n'?"));
    assert_eq!(d.path, "require[0].when");
}

#[test]
fn a_machine_must_match_its_archetypes_behaviours() {
    let ds = diags(
        r#""m": {"kind": "machine", "initial": "idle", "states": {"idle": {"behaviours": ["wlak"]}}},
           "p": {"kind": "path", "points": [[0,0,0],[1,0,0]]},
           "a": {"kind": "archetype", "attributes": {"speed": "1m/s"}, "machine": "m",
                 "behaviours": [{"type": "follow_path", "name": "walk", "path": "p", "speed": "self.speed"}]}"#,
    );
    let d = one(&ds, "unknown_ref");
    assert_eq!(d.suggestion.as_deref(), Some("did you mean 'walk'?"));
}

#[test]
fn hostile_sizes_are_errors_not_crashes() {
    // Deep nesting and long expressions.
    let deep = format!("{}1{} > 0", "(".repeat(2000), ")".repeat(2000));
    let ds = diags(&format!(r#""o": {{"kind": "outcome", "result": "won", "when": "{deep}"}}"#));
    assert_eq!(one(&ds, "too_many").path, "when");
    let chain = format!("{} > 0", vec!["1"; 100].join(" + "));
    let ds = diags(&format!(r#""o": {{"kind": "outcome", "result": "won", "when": "{chain}"}}"#));
    assert!(one(&ds, "too_many").message.contains("deep"));
    // Coordinates so large their squares would overflow.
    let ds = diags(r#""p": {"kind": "path", "points": [[-9e15, 0, 0], [9e15, 0, 0]]}"#);
    assert!(ds.iter().any(|d| d.code == "out_of_range"), "{ds:?}");
    // A huge duration in an out-of-range message.
    let ds = diags(r#""p": {"kind": "param", "type": "duration", "value": "10000000000000000t", "max": "1t"}"#);
    assert_eq!(one(&ds, "out_of_range").item, "p");
    // A wave that would queue millions of spawns.
    let ds = diags(
        r#""foe": {"kind": "archetype"}, "lane": {"kind": "path", "points": [[0,0,0],[1,0,0]]},
           "waves": {"kind": "waves", "waves": [{"groups": [{"archetype": "foe", "count": 100000, "at": ["lane"]}, {"archetype": "foe", "count": 100000, "at": ["lane"]}]}]}"#,
    );
    assert!(one(&ds, "too_many").message.contains("per wave"));
}

#[test]
fn sizes_that_cost_memory_or_time_are_refused() {
    // An item too big for one world entity, caught before an import fails.
    let big: Vec<String> = (0..800).map(|_| r#"{"duration": "1s", "groups": []}"#.to_owned()).collect();
    let ds = diags(&format!(r#""waves": {{"kind": "waves", "waves": [{}]}}"#, big.join(",")));
    let d = one(&ds, "too_large");
    assert!(d.suggestion.as_deref().unwrap().contains("several waves items"), "{d}");
    // Too many attributes on one archetype (state per entity).
    let attrs: Vec<String> = (0..65).map(|i| format!("\"a{i}\": 1")).collect();
    let ds = diags(&format!(r#""fat": {{"kind": "archetype", "attributes": {{{}}}}}"#, attrs.join(",")));
    assert_eq!(one(&ds, "too_many").path, "attributes");
    // A tag filter with too many tags (every candidate check scans them).
    let tags: Vec<String> = (0..17).map(|i| format!("\"!t{i}\"")).collect();
    let ds = tower(&format!(r#"{{"type": "attack", "targets": ["enemy", {}], "range": 7, "cooldown": 1, "payload": []}}"#, tags.join(",")));
    assert!(ds.iter().any(|d| d.code == "too_many" && d.path == "behaviours[0].targets"), "{ds:?}");
    // Many waves: the counter can't overflow. (Split over items to keep each small.)
    let mut items = vec![r#""r": {"kind": "resource", "start": 0}"#.to_owned()];
    for k in 0..26 {
        let w: Vec<&str> = (0..400).map(|_| r#"{"duration": "0s", "groups": []}"#).collect();
        items.push(format!(r#""w{k:02}": {{"kind": "waves", "waves": [{}]}}"#, w.join(",")));
    }
    let ds = diags(&items.join(","));
    assert!(ds.iter().any(|d| d.code == "too_many" && d.message.contains("waves")), "{:?}", ds.iter().map(|d| d.to_string()).collect::<Vec<_>>());
    // Many custom events compile quickly and are capped.
    let emits: Vec<String> = (0..61_000).map(|i| format!(r#"{{"emit": "e{i}"}}"#)).collect();
    let ds = diags(&format!(r#""t": {{"kind": "timer", "do": [{}]}}"#, emits.join(",")));
    assert!(ds.iter().any(|d| d.code == "too_many" && d.message.contains("events")));
}

#[test]
fn a_notes_named_by_the_author_is_data() {
    let a = doc(r#""a": {"kind": "archetype", "attributes": {"notes": 1}}"#);
    let b = doc(r#""a": {"kind": "archetype", "attributes": {"notes": 2}}"#);
    assert_ne!(a.hash(), b.hash(), "an attribute called notes changes the rules");
    let c = doc(r#""a": {"kind": "archetype", "notes": "x", "attributes": {"notes": 1}}"#);
    assert_eq!(a.hash(), c.hash(), "the item's own notes don't");
}

#[test]
fn diagnostics_serialise_as_data() {
    let ds = tower(r#"{"type": "attack", "targets": ["enemy"], "range": "7", "cooldown": 1, "payload": []}"#);
    let j = ds[0].to_json();
    for k in ["severity", "code", "item", "path", "message", "expected", "got", "suggestion"] {
        assert!(j.get(k).is_some(), "{k} in {j}");
    }
}
