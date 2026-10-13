//! Binding files (validation, round trips, the Sepulcra defaults) and recordings.

use specula_core::Json;
use specula_input::{
    ActionFrame, ActionMap, ActionState, BindingSlot, ErrorCode, FrameDecoder, FrameEncoder, Input, Player, Pointer,
    Recording, Source, TextEvent,
};
use specula_platform::{keys, DeadZones, Event, GamepadButton, GamepadState, MouseButton, PointerKind, RawGamepad};

const SEPULCRA: &str = include_str!("../../../games/sepulcra/data/input.json");

fn err(json: &str) -> specula_input::InputError {
    ActionMap::from_json_str(json).expect_err("should be rejected")
}

fn one_action(action: &str) -> String {
    format!(r#"{{"format": "specula.input.v1", "actions": [{action}]}}"#)
}

#[test]
fn sepulcra_defaults_load_cleanly() {
    let map = ActionMap::from_json_str(SEPULCRA).expect("games/sepulcra/data/input.json");
    for name in ["place", "cancel", "next_face", "prev_face", "build_menu", "pause", "orbit", "zoom", "cursor_move"] {
        assert!(map.action(name).is_some(), "{name}");
    }
    assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
    // Every gameplay action is reachable on a gamepad (Steam Deck Verified needs this).
    for a in map.actions() {
        assert!(
            a.bindings.iter().any(|b| b.family() == specula_input::DeviceFamily::Gamepad),
            "{} has no gamepad binding",
            a.name
        );
    }
    // The file is in canonical form except for its formatting: a save/load round trip is stable.
    let again = ActionMap::from_json(&map.to_json()).unwrap();
    assert_eq!(again, map);
}

#[test]
fn full_maps_and_bindings_round_trip() {
    let map = ActionMap::from_json_str(SEPULCRA).unwrap();
    let text = map.to_json().to_pretty();
    assert_eq!(ActionMap::from_json_str(&text).unwrap(), map);
    assert!(text.contains("\"Mouse.Right+Mouse.Motion\""), "chords save as strings");
    assert!(text.contains("\"scale\": 4") && !text.contains("0.30000001"), "shortest numbers:\n{text}");

    // A player's rebinds: only bindings, applied onto the defaults.
    let mut mine = map.clone();
    mine.set_binding("place", BindingSlot::Replace(0), specula_input::Binding::source(Source::Key(keys::SPACE)))
        .unwrap();
    let saved = mine.bindings_json().to_string();
    let mut restored = map.clone();
    restored.load_bindings_str(&saved).unwrap();
    assert_eq!(restored, mine);
}

#[test]
fn bindings_file_errors_leave_the_map_unchanged() {
    let map = ActionMap::from_json_str(SEPULCRA).unwrap();
    let mut m = map.clone();
    let e = m
        .load_bindings_str(r#"{"format": "specula.bindings.v1", "bindings": {"place": ["Key.Space"], "warp": []}}"#)
        .unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::UnknownAction, "bindings.warp"));
    let e = m
        .load_bindings_str(r#"{"format": "specula.bindings.v1", "bindings": {"place": ["Key.Space"], "orbit": ["Key.W"]}}"#)
        .unwrap_err();
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::IncompatibleSource, "bindings.orbit[0]"));
    assert_eq!(m, map, "all-or-nothing");
    let e = m.load_bindings_str(r#"{"format": "specula.input.v1", "bindings": {}}"#).unwrap_err();
    assert_eq!(e.code, ErrorCode::Format);
}

#[test]
fn validation_errors_carry_code_and_path() {
    let e = err("{ \"format\": ");
    assert_eq!(e.code, ErrorCode::JsonSyntax);
    assert!(e.offset.is_some());

    assert_eq!(err(r#"{"actions": []}"#).code, ErrorCode::Format);
    assert_eq!(err(r#"{"format": "specula.input.v9", "actions": []}"#).code, ErrorCode::Format);
    let e = err(r#"{"format": "specula.input.v1"}"#);
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::MissingField, "actions"));

    let e = err(&one_action(r#"{"name": "a", "kind": "button", "bindings": ["Key.Spaec"]}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::UnknownSource, "actions[0].bindings[0]"));
    assert!(e.message.contains("Key.Spaec"), "{e}");

    let e = err(&one_action(r#"{"name": "a", "kind": "axis2", "bindings": ["Key.W"]}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::IncompatibleSource, "actions[0].bindings[0]"));

    let e = err(&one_action(r#"{"name": "a", "kind": "button", "bindings": ["Mouse.Motion"]}"#));
    assert_eq!(e.code, ErrorCode::IncompatibleSource);

    let e = err(&one_action(r#"{"name": "a", "kind": "button", "bindings": ["Mouse.WheelUp+Key.A"]}"#));
    assert_eq!(e.code, ErrorCode::IncompatibleSource, "a wheel pulse can't be held as a modifier");

    let e = err(&one_action(r#"{"name": "a", "kind": "trigger"}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::BadValue, "actions[0].kind"));

    let e = err(&one_action(r#"{"name": "a b", "kind": "button"}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::BadValue, "actions[0].name"));

    let e = err(&one_action(r#"{"name": "a", "kind": "button", "bindigns": []}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::BadValue, "actions[0].bindigns"), "typos are caught");

    let e = err(&one_action(r#"{"name": "a", "kind": "axis", "bindings": [{"source": "Pad.LT", "scale": "big"}]}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::WrongType, "actions[0].bindings[0].scale"));

    let e = err(&one_action(r#"{"name": "a", "kind": "axis2", "bindings": [{"source": "Pad.LeftStick", "dead_zone": 1.5}]}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::BadValue, "actions[0].bindings[0].dead_zone"));

    let e = err(&one_action(r#"{"name": "a", "kind": "axis2", "bindings": [{"composite": {"up": "Key.W", "down": "Key.S", "left": "Key.A"}}]}"#));
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::MissingField, "actions[0].bindings[0].composite.right"));

    let e = err(r#"{"format": "specula.input.v1", "actions": [{"name": "a", "kind": "button"}, {"name": "a", "kind": "axis"}]}"#);
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::DuplicateAction, "actions[1].name"));

    let e = err(r#"{"format": "specula.input.v1", "settings": {"tap_max_s": -1}, "actions": []}"#);
    assert_eq!((e.code, e.path.as_str()), (ErrorCode::BadValue, "settings.tap_max_s"));

    // The Display form is one line an agent can act on.
    assert_eq!(e.to_string(), "bad_value at settings.tap_max_s: must be positive");
}

#[test]
fn conflicts_are_found_within_groups_only() {
    let map = ActionMap::from_json_str(
        r#"{"format": "specula.input.v1", "actions": [
          {"name": "a", "kind": "button", "bindings": ["Key.Ctrl+Key.Z", "Pad.A"]},
          {"name": "b", "kind": "button", "bindings": ["Key.Z", "Pad.South"]},
          {"name": "c", "kind": "button", "group": "menu", "bindings": ["Pad.A"]},
          {"name": "d", "kind": "button", "bindings": [{"source": "Key.Z", "modifiers": ["Key.Ctrl"]}]}
        ]}"#,
    )
    .unwrap();
    let pairs: Vec<String> = map
        .conflicts()
        .iter()
        .map(|(x, y)| format!("{}[{}]={}[{}]", x.action, x.binding, y.action, y.binding))
        .collect();
    assert_eq!(pairs, vec!["a[0]=d[0]", "a[1]=b[1]"], "Ctrl+Z vs Z is a chord, not a conflict; groups are separate");
}

// ---- recording ----

/// A scripted session over the Sepulcra map: mouse, keys, a pad and touch.
fn session() -> (ActionMap, Vec<(f64, Vec<Event>, GamepadState)>) {
    let map = ActionMap::from_json_str(SEPULCRA).unwrap();
    let pad = |raw: RawGamepad| GamepadState::from_raw(raw, &DeadZones::default());
    let mut script = Vec::new();
    let mut t = 0.0;
    let mut step = |events: Vec<Event>, p: GamepadState| {
        t += 1.0 / 60.0;
        script.push((t, events, p));
    };
    let idle = GamepadState::default();
    step(vec![Event::MouseMove { x: 400, y: 300 }], idle);
    step(vec![Event::MouseDown { button: MouseButton::Left, x: 400, y: 300 }], idle);
    step(vec![Event::MouseUp { button: MouseButton::Left, x: 400, y: 300 }], idle);
    step(vec![Event::MouseDown { button: MouseButton::Right, x: 400, y: 300 }], idle);
    for i in 1..=5 {
        step(vec![Event::MouseMove { x: 400 + 7 * i, y: 300 - 3 * i }], idle);
    }
    step(vec![Event::MouseUp { button: MouseButton::Right, x: 435, y: 285 }, Event::MouseWheel { delta: 1.0 }], idle);
    step(vec![Event::KeyDown { code: keys::from_name("E").unwrap(), repeat: false }], idle);
    step(vec![Event::KeyUp { code: keys::from_name("E").unwrap() }], idle);
    for _ in 0..4 {
        step(vec![], pad(RawGamepad { right_x: 20000, left_y: -32767, right_trigger: 255, ..Default::default() }));
    }
    step(vec![], pad(RawGamepad { buttons: GamepadButton::RightBumper.bit(), ..Default::default() }));
    step(vec![], idle);
    step(vec![Event::PointerDown { id: 5, kind: PointerKind::Touch, x: 100, y: 100 }], idle);
    step(vec![Event::PointerUp { id: 5, kind: PointerKind::Touch, x: 101, y: 100 }], idle);
    // Release edges, then their clearing, then a truly idle frame.
    for _ in 0..3 {
        step(vec![], idle);
    }
    (map, script)
}

fn run(map: &ActionMap, script: &[(f64, Vec<Event>, GamepadState)]) -> (Recording, Vec<ActionFrame>) {
    let mut input = Input::new(map.clone());
    let mut rec = Recording::new(map);
    let mut frames = Vec::new();
    for (t, events, pad) in script {
        input.update(*t, events, std::slice::from_ref(pad));
        frames.push(input.action_frame());
        rec.push(input.action_frame()).unwrap();
    }
    (rec, frames)
}

#[test]
fn recording_round_trips_bit_exactly() {
    let (map, script) = session();
    let (rec, frames) = run(&map, &script);
    // Sanity: the script did things.
    let idx = |n: &str| map.index_of(n).unwrap();
    assert!(frames.iter().any(|f| f.actions[idx("place")].pressed));
    assert!(frames.iter().any(|f| f.actions[idx("orbit")].value != [0.0, 0.0]));
    assert!(frames.iter().any(|f| f.actions[idx("zoom")].value[0] > 0.0));
    assert!(frames.iter().any(|f| f.actions[idx("next_face")].pressed));
    assert!(frames.iter().filter(|f| f.actions[idx("place")].pressed).count() >= 2, "click and tap");

    let bytes = rec.to_bytes();
    assert!(bytes.len() < 500, "compact: {} bytes for {} frames (schema header included)", bytes.len(), frames.len());
    let back = Recording::from_bytes(&bytes).unwrap();
    assert_eq!(back.frames(), &frames[..]);
    assert_eq!(back.schema_hash(), map.schema_hash());
    assert_eq!(Recording::from_base64(&rec.to_base64()).unwrap(), back);
}

#[test]
fn recording_is_deterministic() {
    let (map, script) = session();
    let a = run(&map, &script).0.to_bytes();
    let b = run(&map, &script).0.to_bytes();
    assert_eq!(a, b);
    assert_eq!(specula_core::hash::hash_bytes(&a), specula_core::hash::hash_bytes(&b));
}

#[test]
fn player_reproduces_every_query() {
    let (map, script) = session();
    let (rec, frames) = run(&map, &script);
    let mut replay = Input::new(map.clone());
    let mut player = Player::new(Recording::from_bytes(&rec.to_bytes()).unwrap(), &map).unwrap();
    // Re-run live alongside the replay and compare every recorded query.
    let mut live = Input::new(map.clone());
    for (f, (t, events, pad)) in frames.iter().zip(&script) {
        live.update(*t, events, std::slice::from_ref(pad));
        assert!(player.step(&mut replay));
        assert_eq!(&replay.action_frame(), f);
        for a in map.actions() {
            assert_eq!(replay.state(&a.name), live.state(&a.name), "{}", a.name);
            assert_eq!(replay.prompt(&a.name), live.prompt(&a.name), "{}", a.name);
        }
        assert_eq!(replay.pointer(), live.pointer());
        assert_eq!(replay.touches(), live.touches());
        assert_eq!(replay.text(), live.text());
        assert_eq!(replay.last_device(), live.last_device());
        assert_eq!(replay.dt().to_bits(), live.dt().to_bits());
        assert!(!replay.source_held(Source::Key(keys::SPACE)), "raw device queries read idle in a replay");
    }
    assert!(frames.iter().any(|f| f.touches.len() == 1), "the script has a touch down");
    assert!(frames.iter().any(|f| f.pointer.is_some_and(|p| p.kind == PointerKind::Touch)));
    assert!(frames.iter().any(|f| f.last_device == Some(specula_input::Device::Gamepad(0))));
    assert_eq!(player.remaining(), 0);
    assert!(!player.step(&mut replay), "finished");
    assert!(replay.states().iter().all(|s| *s == ActionState::default()));
}

#[test]
fn player_end_releases_held_actions() {
    let map = ActionMap::from_json_str(SEPULCRA).unwrap();
    let mut input = Input::new(map.clone());
    let mut rec = Recording::new(&map);
    let pad = GamepadState::from_raw(RawGamepad { buttons: GamepadButton::South.bit(), ..Default::default() }, &DeadZones::default());
    input.update(0.0, &[], &[pad]);
    rec.push(input.action_frame()).unwrap();
    assert!(input.held("place"));
    let mut replay = Input::new(map.clone());
    let mut player = Player::new(rec, &map).unwrap();
    assert!(player.step(&mut replay));
    assert!(replay.pressed("place"));
    assert!(!player.step(&mut replay));
    assert!(replay.released("place") && !replay.held("place"), "the end releases what was held");
    assert!(!player.step(&mut replay));
    assert!(!replay.released("place"), "once");
}

#[test]
fn axis_held_before_recording_starts_round_trips_exactly() {
    let map = ActionMap::from_json_str(SEPULCRA).unwrap();
    let mut input = Input::new(map.clone());
    let stick = GamepadState::from_raw(RawGamepad { left_x: 32767, ..Default::default() }, &DeadZones::default());
    input.update(0.0, &[], &[stick]);
    input.update(0.016, &[], &[stick]);
    let mut rec = Recording::new(&map);
    input.update(0.032, &[], &[stick]);
    let first = input.action_frame();
    let cursor = first.actions[map.index_of("cursor_move").unwrap()];
    assert!(cursor.held && !cursor.pressed, "{cursor:?}");
    rec.push(first.clone()).unwrap();
    let back = Recording::from_bytes(&rec.to_bytes()).unwrap();
    assert_eq!(back.frames()[0], first);
}

#[test]
fn frame_streaming_matches_whole_recordings() {
    let (map, script) = session();
    let (_, frames) = run(&map, &script);
    let mut enc = FrameEncoder::new(&map);
    let mut stream = Vec::new();
    let mut sizes = Vec::new();
    for f in &frames {
        let before = stream.len();
        enc.encode(f, &mut stream).unwrap();
        sizes.push(stream.len() - before);
    }
    assert_eq!(*sizes.last().unwrap(), 1, "an idle frame is one byte");
    let mut dec = FrameDecoder::new(&map);
    let mut pos = 0;
    for f in &frames {
        assert_eq!(&dec.decode(&stream, &mut pos).unwrap(), f);
    }
    assert_eq!(pos, stream.len());
}

#[test]
fn text_and_pointer_are_recorded() {
    let map = ActionMap::from_json_str(SEPULCRA).unwrap();
    let mut input = Input::new(map.clone());
    let mut rec = Recording::new(&map);
    input.set_text_mode(true);
    input.update(0.0, &[Event::Char('R'), Event::Char('\u{e9}'), Event::MouseMove { x: 3, y: 4 }], &[]);
    rec.push(input.action_frame()).unwrap();
    input.update(0.1, &[Event::Char('x')], &[]);
    rec.push(input.action_frame()).unwrap();
    input.update(0.2, &[Event::KeyDown { code: keys::BACKSPACE, repeat: false }, Event::KeyDown { code: keys::LEFT, repeat: true }], &[]);
    rec.push(input.action_frame()).unwrap();
    let back = Recording::from_bytes(&rec.to_bytes()).unwrap();
    assert_eq!(back.frames()[0].text, vec![TextEvent::Char('R'), TextEvent::Char('\u{e9}')]);
    let mouse = Some(Pointer { x: 3.0, y: 4.0, kind: PointerKind::Mouse });
    assert_eq!(back.frames()[0].pointer, mouse);
    assert_eq!(back.frames()[1].text, vec![TextEvent::Char('x')], "text doesn't carry over");
    assert_eq!(back.frames()[1].pointer, mouse, "pointer does");
    assert_eq!(
        back.frames()[2].text,
        vec![TextEvent::Key { code: keys::BACKSPACE, repeat: false }, TextEvent::Key { code: keys::LEFT, repeat: true }],
        "editing keys replay"
    );
    assert_eq!(back.frames(), rec.frames());
}

#[test]
fn corrupt_recordings_are_rejected_with_offsets() {
    let (map, script) = session();
    let bytes = run(&map, &script).0.to_bytes();
    let e = Recording::from_bytes(b"NOPE\x01").unwrap_err();
    assert_eq!((e.code, e.offset), (ErrorCode::BadRecording, Some(0)));
    let mut v = bytes.clone();
    v[4] = 9;
    assert_eq!(Recording::from_bytes(&v).unwrap_err().offset, Some(4));
    // Every truncation fails cleanly (no panic), at or before the cut.
    for cut in 0..bytes.len() {
        let e = Recording::from_bytes(&bytes[..cut]).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadRecording);
        assert!(e.offset.unwrap() <= cut, "cut {cut}: {e}");
    }
    let mut v = bytes.clone();
    v.push(0);
    assert!(Recording::from_bytes(&v).unwrap_err().message.contains("trailing"));
    // Flipping any single byte never panics.
    for i in 0..bytes.len() {
        let mut v = bytes.clone();
        v[i] ^= 0xA5;
        let _ = Recording::from_bytes(&v);
    }
}

#[test]
fn schema_mismatch_is_reported() {
    let (map, script) = session();
    let rec = run(&map, &script).0;
    let other = ActionMap::from_json_str(
        r#"{"format": "specula.input.v1", "actions": [{"name": "place", "kind": "button"}]}"#,
    )
    .unwrap();
    let e = Player::new(rec.clone(), &other).err().unwrap();
    assert_eq!(e.code, ErrorCode::SchemaMismatch);
    assert!(e.message.contains("cursor_move:axis2"), "{e}");
    let mut wrong = Recording::new(&other);
    assert_eq!(wrong.push(rec.frames()[0].clone()).unwrap_err().code, ErrorCode::SchemaMismatch);
    let mut input = Input::new(other);
    assert_eq!(input.apply_frame(&rec.frames()[0]).unwrap_err().code, ErrorCode::SchemaMismatch);
    let _ = Json::Null;
}
