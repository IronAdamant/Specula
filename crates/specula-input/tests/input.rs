//! Behaviour of `Input` driven by synthetic platform events (no window or pad needed).

use specula_input::{
    Action, ActionKind, ActionMap, Binding, BindingInput, BindingSlot, CancelReason, CompositePart, Device, DeviceFamily, ErrorCode,
    Input, PadStyle, RebindOutcome, RebindRequest, Source, TextEvent,
};
use specula_platform::{keys, DeadZones, Event, GamepadButton, GamepadState, MouseButton, PointerKind, RawGamepad};

const DT: f64 = 1.0 / 60.0;

struct Harness {
    input: Input,
    t: f64,
    pads: [GamepadState; 4],
}

impl Harness {
    fn new(json: &str) -> Harness {
        Harness { input: Input::new(ActionMap::from_json_str(json).expect("test map")), t: 0.0, pads: Default::default() }
    }

    fn frame(&mut self, events: &[Event]) {
        self.t += DT;
        self.input.update(self.t, events, &self.pads);
    }

    fn wait(&mut self, seconds: f64) {
        self.t += seconds - DT;
        self.frame(&[]);
    }

    fn pad(&mut self, raw: RawGamepad) {
        self.pads[0] = GamepadState::from_raw(raw, &DeadZones::default());
    }
}

fn code(name: &str) -> u32 {
    keys::from_name(name).unwrap()
}

fn kd(name: &str) -> Event {
    Event::KeyDown { code: code(name), repeat: false }
}

fn ku(name: &str) -> Event {
    Event::KeyUp { code: code(name) }
}

fn touch_down(id: u32, x: i32, y: i32) -> Event {
    Event::PointerDown { id, kind: PointerKind::Touch, x, y }
}

fn touch_move(id: u32, x: i32, y: i32) -> Event {
    Event::PointerMove { id, kind: PointerKind::Touch, x, y }
}

fn touch_up(id: u32, x: i32, y: i32) -> Event {
    Event::PointerUp { id, kind: PointerKind::Touch, x, y }
}

const BASIC: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    {"name": "jump", "kind": "button", "bindings": ["Key.Space", "Pad.A", "Mouse.Left"]},
    {"name": "undo", "kind": "button", "bindings": ["Key.Ctrl+Key.Z"]},
    {"name": "zap", "kind": "button", "bindings": ["Key.Z"]},
    {"name": "fire", "kind": "button", "bindings": ["Pad.RT", {"source": "Pad.LeftStick", "threshold": 0.9}]},
    {"name": "move", "kind": "axis2", "bindings": [
      {"composite": {"up": "Key.W", "down": "Key.S", "left": "Key.A", "right": "Key.D"}, "scale": 10},
      {"source": "Pad.LeftStick", "scale": 10}
    ]},
    {"name": "throttle", "kind": "axis", "output": "value", "bindings": ["Pad.LeftY", {"source": "Pad.RightX", "dead_zone": 0.5}]},
    {"name": "look", "kind": "axis2", "bindings": ["Mouse.Right+Mouse.Motion"]},
    {"name": "scroll", "kind": "axis", "bindings": ["Mouse.Wheel", {"composite": {"negative": "Key.Q", "positive": "Key.E"}}]},
    {"name": "text_escape", "kind": "button", "text_mode": true, "bindings": ["Key.Escape"]}
  ]
}"#;

#[test]
fn button_edges_across_frames() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("Space")]);
    assert!(h.input.pressed("jump") && h.input.held("jump") && !h.input.released("jump"));
    h.frame(&[Event::KeyDown { code: code("Space"), repeat: true }]);
    assert!(!h.input.pressed("jump") && h.input.held("jump"), "auto-repeat is not a new press");
    h.frame(&[ku("Space")]);
    assert!(!h.input.held("jump") && h.input.released("jump") && !h.input.pressed("jump"));
    h.frame(&[]);
    assert!(!h.input.released("jump"));
}

#[test]
fn press_and_release_within_one_frame_still_counts() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("Space"), ku("Space")]);
    assert!(h.input.pressed("jump") && h.input.held("jump"));
    h.frame(&[]);
    assert!(h.input.released("jump") && !h.input.held("jump"));
}

#[test]
fn re_press_within_one_frame_gives_both_edges() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("Space")]);
    h.frame(&[ku("Space"), kd("Space")]);
    let s = h.input.state("jump");
    assert!(s.held && s.pressed && s.released, "{s:?}");
}

#[test]
fn several_bindings_share_one_action() {
    let mut h = Harness::new(BASIC);
    h.frame(&[Event::MouseDown { button: MouseButton::Left, x: 5, y: 5 }]);
    assert!(h.input.pressed("jump"));
    h.pad(RawGamepad { buttons: GamepadButton::South.bit(), ..Default::default() });
    h.frame(&[]);
    assert!(h.input.held("jump") && !h.input.pressed("jump"), "second binding while held: no new press");
    h.frame(&[Event::MouseUp { button: MouseButton::Left, x: 5, y: 5 }]);
    assert!(h.input.held("jump"), "pad still holds it");
    h.pad(RawGamepad::default());
    h.frame(&[]);
    assert!(h.input.released("jump"));
    assert_eq!(h.input.last_device(), Some(Device::Gamepad(0)));
}

#[test]
fn chords_block_their_plain_counterparts() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("Z")]);
    assert!(h.input.pressed("zap") && !h.input.held("undo"));
    h.frame(&[ku("Z")]);
    h.frame(&[kd("Ctrl")]);
    h.frame(&[kd("Z")]);
    assert!(h.input.pressed("undo"), "Ctrl+Z fires undo");
    assert!(!h.input.held("zap"), "and blocks plain Z");
    h.frame(&[ku("Ctrl")]);
    assert!(h.input.released("undo"));
    assert!(h.input.pressed("zap"), "Z still held without Ctrl: zap");
}

#[test]
fn modifier_gates_mouse_motion() {
    let mut h = Harness::new(BASIC);
    h.frame(&[Event::MouseMove { x: 100, y: 100 }]);
    h.frame(&[Event::MouseMove { x: 110, y: 95 }]);
    assert_eq!(h.input.value2("look"), [0.0, 0.0], "no right button: no look");
    h.frame(&[Event::MouseDown { button: MouseButton::Right, x: 110, y: 95 }, Event::MouseMove { x: 130, y: 100 }]);
    assert_eq!(h.input.value2("look"), [20.0, 5.0]);
    h.frame(&[]);
    assert_eq!(h.input.value2("look"), [0.0, 0.0], "delta sources are per frame");
    assert_eq!(h.input.pointer().map(|p| (p.x, p.y, p.kind)), Some((130.0, 100.0, PointerKind::Mouse)));
}

#[test]
fn composites_and_rates_scale_with_frame_time() {
    let mut h = Harness::new(BASIC);
    h.frame(&[]);
    h.frame(&[kd("W"), kd("D")]);
    let v = h.input.value2("move");
    let expect = 10.0 * DT as f32 / 2f32.sqrt();
    assert!((v[0] - expect).abs() < 1e-5 && (v[1] - expect).abs() < 1e-5, "diagonal is unit length: {v:?}");
    // A long hitch is capped by max_dt_s (0.1 s by default).
    h.wait(1.0);
    let v = h.input.value2("move");
    assert!((v[0] - 10.0 * 0.1 / 2f32.sqrt()).abs() < 1e-4, "{v:?}");
    h.frame(&[kd("E")]);
    assert!((h.input.value("scroll") - DT as f32).abs() < 1e-6);
    h.frame(&[Event::MouseWheel { delta: 2.0 }]);
    assert!((h.input.value("scroll") - (2.0 + DT as f32)).abs() < 1e-5, "delta + rate");
}

#[test]
fn gamepad_dead_zones_and_thresholds() {
    let mut h = Harness::new(BASIC);
    h.frame(&[]);
    // Inside the platform's left-stick dead zone (~0.24): nothing.
    h.pad(RawGamepad { left_x: 6000, left_y: 0, ..Default::default() });
    h.frame(&[]);
    assert_eq!(h.input.value2("move"), [0.0, 0.0]);
    assert_eq!(h.input.value("throttle"), 0.0);
    // Full up: value output is the level itself.
    h.pad(RawGamepad { left_y: 32767, ..Default::default() });
    h.frame(&[]);
    assert_eq!(h.input.value("throttle"), 1.0);
    assert!(h.input.held("fire"), "stick magnitude 1.0 passes fire's 0.9 threshold");
    // Per-binding dead zone (0.5) replaces the platform's on RightX.
    h.pad(RawGamepad { right_x: 13000, ..Default::default() }); // ~0.4: past platform's 0.27, inside 0.5
    h.frame(&[]);
    assert_eq!(h.input.value("throttle"), 0.0);
    h.pad(RawGamepad { right_x: 32767, ..Default::default() });
    h.frame(&[]);
    assert_eq!(h.input.value("throttle"), 1.0);
    // Trigger as a button at the press threshold (0.5).
    h.pad(RawGamepad { right_trigger: 100, ..Default::default() });
    h.frame(&[]);
    assert!(!h.input.held("fire"));
    h.pad(RawGamepad { right_trigger: 200, ..Default::default() });
    h.frame(&[]);
    assert!(h.input.pressed("fire"));
}

#[test]
fn stick_past_custom_threshold_presses_a_button() {
    let mut h = Harness::new(BASIC);
    h.pad(RawGamepad { left_x: 26000, ..Default::default() }); // ~0.73 after dead zone rescale
    h.frame(&[]);
    assert!(!h.input.held("fire"));
    h.pad(RawGamepad { left_x: 32767, ..Default::default() });
    h.frame(&[]);
    assert!(h.input.pressed("fire"));
}

#[test]
fn focus_loss_releases_everything() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("Space")]);
    h.frame(&[Event::Focused(false)]);
    assert!(h.input.released("jump") && !h.input.held("jump"));
    // The key-up arrives later (or never); nothing breaks.
    h.frame(&[Event::Focused(true), ku("Space")]);
    assert!(!h.input.held("jump"));
}

// ---- touch gestures ----

const TOUCH: &str = r#"{
  "format": "specula.input.v1",
  "settings": {"tap_max_s": 0.3, "tap_slop_px": 10, "long_press_s": 0.5},
  "actions": [
    {"name": "place", "kind": "button", "bindings": ["Touch.Tap"]},
    {"name": "cancel", "kind": "button", "bindings": ["Touch.TwoFingerTap"]},
    {"name": "menu", "kind": "button", "bindings": ["Touch.LongPress"]},
    {"name": "orbit", "kind": "axis2", "bindings": ["Touch.Drag"]},
    {"name": "pan", "kind": "axis2", "bindings": ["Touch.TwoFingerDrag"]},
    {"name": "zoom", "kind": "axis", "bindings": ["Touch.Pinch"]},
    {"name": "twist", "kind": "axis", "bindings": ["Touch.Rotate"]}
  ]
}"#;

#[test]
fn tap_fires_on_release_at_the_touch_point() {
    let mut h = Harness::new(TOUCH);
    h.frame(&[touch_down(1, 200, 150)]);
    assert!(!h.input.held("place"));
    assert_eq!(h.input.last_device(), Some(Device::Touch));
    h.frame(&[touch_move(1, 204, 152), touch_up(1, 204, 152)]);
    assert!(h.input.pressed("place"));
    let p = h.input.pointer().unwrap();
    assert_eq!((p.x, p.y, p.kind), (204.0, 152.0, PointerKind::Touch));
    h.frame(&[]);
    assert!(h.input.released("place"));
}

#[test]
fn slow_or_moved_touches_are_not_taps() {
    let mut h = Harness::new(TOUCH);
    h.frame(&[touch_down(1, 0, 0)]);
    h.wait(0.4);
    h.frame(&[touch_up(1, 0, 0)]);
    assert!(!h.input.held("place"), "too slow");

    h.frame(&[touch_down(2, 0, 0)]);
    h.frame(&[touch_move(2, 30, 0), touch_up(2, 30, 0)]);
    assert!(!h.input.held("place"), "moved past the slop");

    h.frame(&[touch_down(3, 0, 0)]);
    h.frame(&[Event::PointerCancel { id: 3, kind: PointerKind::Touch }]);
    assert!(!h.input.held("place"), "cancelled");
}

#[test]
fn drag_starts_past_the_slop_without_a_jump() {
    let mut h = Harness::new(TOUCH);
    h.frame(&[touch_down(1, 100, 100)]);
    h.frame(&[touch_move(1, 105, 100)]);
    assert_eq!(h.input.value2("orbit"), [0.0, 0.0], "inside slop");
    h.frame(&[touch_move(1, 115, 100)]);
    assert_eq!(h.input.value2("orbit"), [0.0, 0.0], "crossing the slop starts the drag here");
    h.frame(&[touch_move(1, 125, 90)]);
    assert_eq!(h.input.value2("orbit"), [10.0, -10.0]);
    h.frame(&[touch_up(1, 125, 90)]);
    assert!(!h.input.held("place"));
}

#[test]
fn long_press_fires_once_and_suppresses_the_tap() {
    let mut h = Harness::new(TOUCH);
    h.frame(&[touch_down(1, 50, 50)]);
    h.wait(0.3);
    assert!(!h.input.held("menu"));
    h.wait(0.25);
    assert!(h.input.pressed("menu"));
    h.frame(&[]);
    assert!(h.input.released("menu"), "a pulse, not held");
    h.wait(0.5);
    assert!(!h.input.held("menu"), "fires once per touch");
    h.frame(&[touch_up(1, 50, 50)]);
    assert!(!h.input.held("place"));
}

#[test]
fn two_finger_gestures() {
    let mut h = Harness::new(TOUCH);
    // Two-finger tap.
    h.frame(&[touch_down(1, 100, 100), touch_down(2, 200, 100)]);
    h.frame(&[touch_up(1, 100, 100), touch_up(2, 200, 100)]);
    assert!(h.input.pressed("cancel") && !h.input.held("place"));

    // Pinch out: distance 100 -> 200 is ln 2; centre stays.
    h.frame(&[touch_down(3, 100, 100), touch_down(4, 200, 100)]);
    h.frame(&[touch_move(3, 50, 100), touch_move(4, 250, 100)]);
    assert!((h.input.value("zoom") - 2f32.ln()).abs() < 1e-5, "{}", h.input.value("zoom"));
    assert_eq!(h.input.value2("pan"), [0.0, 0.0]);
    assert_eq!(h.input.value2("orbit"), [0.0, 0.0], "two fingers never orbit");
    // Rotate a quarter turn clockwise on screen (y down) around the centre.
    h.frame(&[touch_move(3, 150, 0), touch_move(4, 150, 200)]);
    let twist = h.input.value("twist");
    assert!((twist - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "{twist}");
    // Pan both.
    h.frame(&[touch_move(3, 160, 10), touch_move(4, 160, 210)]);
    assert_eq!(h.input.value2("pan"), [10.0, 10.0]);
    // Lifting one finger after a pinch doesn't orbit; the session never taps.
    h.frame(&[touch_up(4, 160, 210)]);
    h.frame(&[touch_move(3, 300, 300)]);
    assert_eq!(h.input.value2("orbit"), [0.0, 0.0]);
    h.frame(&[touch_up(3, 300, 300)]);
    assert!(!h.input.held("place") && !h.input.held("cancel"));
    assert!(h.input.touches().is_empty());
}

// ---- text mode ----

#[test]
fn text_mode_passes_text_and_suppresses_actions() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("Space")]);
    h.input.set_text_mode(true);
    h.frame(&[Event::Char('h'), Event::Char('i'), kd("Backspace")]);
    assert!(h.input.released("jump"), "entering text mode releases held actions");
    let typed = [TextEvent::Char('h'), TextEvent::Char('i'), TextEvent::Key { code: keys::BACKSPACE, repeat: false }];
    assert_eq!(h.input.text(), &typed);
    assert_eq!(h.input.action_frame().text, typed, "editing keys are recorded too");
    h.frame(&[kd("Z"), kd("Escape")]);
    assert!(!h.input.held("zap"));
    assert!(h.input.pressed("text_escape"), "text_mode actions stay live");
    h.input.set_text_mode(false);
    h.frame(&[]);
    assert!(!h.input.held("zap"), "Z held through the mode change is ignored until released");
    h.frame(&[ku("Z")]);
    h.frame(&[kd("Z")]);
    assert!(h.input.pressed("zap"));
    h.frame(&[Event::Char('x')]);
    assert!(h.input.text().is_empty(), "no text outside text mode");
}

// ---- focus ----

#[test]
fn pads_are_ignored_while_unfocused_unless_allowed() {
    let mut h = Harness::new(BASIC);
    let a = RawGamepad { buttons: GamepadButton::South.bit(), ..Default::default() };
    h.frame(&[Event::Focused(false)]);
    h.pad(a);
    h.frame(&[]);
    assert!(!h.input.held("jump"), "XInput reports pads while unfocused; they are ignored");
    h.frame(&[Event::Focused(true)]);
    assert!(!h.input.held("jump"), "a button held while focus returns waits for release");
    h.pad(RawGamepad::default());
    h.frame(&[]);
    h.pad(a);
    h.frame(&[]);
    assert!(h.input.pressed("jump"));

    let mut map = ActionMap::from_json_str(BASIC).unwrap();
    map.settings.background_pads = true;
    let mut h = Harness { input: Input::new(map), t: 0.0, pads: Default::default() };
    h.frame(&[Event::Focused(false)]);
    h.pad(a);
    h.frame(&[]);
    assert!(h.input.pressed("jump"), "background_pads keeps them live");
}

// ---- devices and glyphs ----

#[test]
fn prompts_follow_the_last_device() {
    let map = ActionMap::from_json_str(include_str!("../../../games/sepulcra/data/input.json")).unwrap();
    let mut h = Harness { input: Input::new(map), t: 0.0, pads: Default::default() };
    assert_eq!(h.input.prompt("place").as_deref(), Some("LMB"), "keyboard/mouse before any input");
    h.pad(RawGamepad { buttons: GamepadButton::RightBumper.bit(), ..Default::default() });
    h.frame(&[]);
    assert_eq!(h.input.last_device(), Some(Device::Gamepad(0)));
    assert_eq!(h.input.prompt("place").as_deref(), Some("A"));
    assert_eq!(h.input.prompt("next_face").as_deref(), Some("RB"));
    h.input.set_pad_style(PadStyle::PlayStation);
    assert_eq!(h.input.prompt("place").as_deref(), Some("Cross"));
    assert_eq!(h.input.prompt("next_face").as_deref(), Some("R1"));
    h.input.set_pad_style(PadStyle::SteamDeck);
    assert_eq!(h.input.prompt("pause").as_deref(), Some("Menu"));
    h.frame(&[kd("E")]);
    assert_eq!(h.input.prompt("next_face").as_deref(), Some("E"));
    assert_eq!(h.input.prompt("orbit").as_deref(), Some("RMB+Mouse"));
    h.frame(&[touch_down(1, 10, 10)]);
    assert_eq!(h.input.prompt("place").as_deref(), Some("Tap"));
    assert_eq!(h.input.prompt("zoom").as_deref(), Some("Pinch"));
    assert_eq!(h.input.prompt("pause"), None, "no touch binding: the game shows a button instead");
    assert_eq!(h.input.glyphs("orbit", DeviceFamily::KeyboardMouse), vec!["RMB+Mouse", "Up/Left/Down/Right"]);
    assert_eq!(h.input.glyphs("cursor_move", DeviceFamily::Gamepad), vec!["L Stick", "D-pad"]);
}

#[test]
fn small_mouse_jitter_does_not_steal_the_prompt() {
    let mut h = Harness::new(BASIC);
    h.pad(RawGamepad { buttons: GamepadButton::South.bit(), ..Default::default() });
    h.frame(&[Event::MouseMove { x: 10, y: 10 }]);
    h.frame(&[Event::MouseMove { x: 11, y: 10 }]);
    assert_eq!(h.input.last_device(), Some(Device::Gamepad(0)));
    h.frame(&[Event::MouseMove { x: 30, y: 10 }]);
    assert_eq!(h.input.last_device(), Some(Device::KeyboardMouse));
}

#[test]
fn gamepad_filter_limits_pads() {
    let mut h = Harness::new(BASIC);
    h.input.set_gamepad_filter(Some(1));
    h.pad(RawGamepad { buttons: GamepadButton::South.bit(), ..Default::default() });
    h.frame(&[]);
    assert!(!h.input.held("jump"), "pad 0 is not this player's");
    h.pads[1] = h.pads[0];
    h.frame(&[]);
    assert!(h.input.pressed("jump"));
}

// ---- rebinding ----

#[test]
fn rebind_captures_a_chord_and_reports_conflicts() {
    let mut h = Harness::new(BASIC);
    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Replace(0))).unwrap();
    h.frame(&[kd("Ctrl")]);
    assert!(h.input.rebinding());
    h.frame(&[kd("Z")]);
    let outcome = h.input.take_rebind_outcome().expect("captured");
    let RebindOutcome::Captured { binding, conflicts, .. } = &outcome else { panic!("{outcome:?}") };
    assert_eq!(binding.input, BindingInput::Source(Source::Key(code("Z"))));
    assert_eq!(binding.modifiers, vec![Source::Key(keys::CTRL)]);
    assert_eq!(conflicts.len(), 1);
    assert_eq!((conflicts[0].action.as_str(), conflicts[0].binding), ("undo", 0));
    assert!(!h.input.held("undo"), "nothing fires while listening");
    h.input.apply_rebind(&outcome).unwrap();
    assert_eq!(h.input.map().action("jump").unwrap().bindings[0].glyph(PadStyle::Xbox), "Ctrl+Z");
    h.frame(&[]);
    assert!(!h.input.held("jump") && !h.input.held("undo"), "keys used to rebind are ignored until released");
    h.frame(&[ku("Z"), ku("Ctrl")]);
    h.frame(&[kd("Ctrl"), kd("Z")]);
    assert!(h.input.pressed("jump") && h.input.pressed("undo"));
}

#[test]
fn rebind_modifier_alone_cancel_and_family_filter() {
    let mut h = Harness::new(BASIC);
    h.input.start_rebind(RebindRequest::new("zap", BindingSlot::Append)).unwrap();
    h.frame(&[kd("Shift")]);
    assert!(h.input.take_rebind_outcome().is_none());
    h.frame(&[ku("Shift")]);
    let Some(RebindOutcome::Captured { binding, .. }) = h.input.take_rebind_outcome() else { panic!() };
    assert_eq!(binding, Binding::source(Source::Key(keys::SHIFT)));

    h.input.start_rebind(RebindRequest::new("zap", BindingSlot::Append)).unwrap();
    h.frame(&[kd("Escape")]);
    assert!(matches!(h.input.take_rebind_outcome(), Some(RebindOutcome::Cancelled { .. })));
    assert!(!h.input.rebinding());

    let mut req = RebindRequest::new("jump", BindingSlot::Append);
    req.family = Some(DeviceFamily::Gamepad);
    h.input.start_rebind(req).unwrap();
    h.frame(&[ku("Escape"), kd("K")]);
    assert!(h.input.take_rebind_outcome().is_none(), "keyboard ignored when listening for a pad");
    h.pad(RawGamepad { buttons: GamepadButton::North.bit(), ..Default::default() });
    h.frame(&[]);
    let Some(RebindOutcome::Captured { binding, .. }) = h.input.take_rebind_outcome() else { panic!() };
    assert_eq!(binding, Binding::source(Source::Pad(GamepadButton::North)));
}

#[test]
fn rebind_axes_and_composite_parts() {
    let mut h = Harness::new(BASIC);
    // A 2D axis takes a stick, not a key.
    h.input.start_rebind(RebindRequest::new("move", BindingSlot::Replace(1))).unwrap();
    h.frame(&[kd("K")]);
    assert!(h.input.take_rebind_outcome().is_none());
    h.pad(RawGamepad { right_x: 32767, ..Default::default() });
    h.frame(&[]);
    let Some(RebindOutcome::Captured { binding, .. }) = h.input.take_rebind_outcome() else { panic!() };
    assert_eq!(binding.input, BindingInput::Source(Source::Stick(specula_input::Stick::Right)));

    // One part of the WASD composite.
    let mut req = RebindRequest::new("move", BindingSlot::Replace(0));
    req.part = Some(CompositePart::Up);
    h.input.start_rebind(req).unwrap();
    h.frame(&[kd("I")]);
    let outcome = h.input.take_rebind_outcome().unwrap();
    h.input.apply_rebind(&outcome).unwrap();
    assert_eq!(h.input.map().action("move").unwrap().bindings[0].glyph(PadStyle::Xbox), "I/A/S/D");
    assert_eq!(h.input.map().action("move").unwrap().bindings[0].scale, [10.0, 10.0], "options kept");

    // A 1D axis from a stick pushed left binds inverted, so pushing that way reads positive.
    h.pad(RawGamepad::default());
    h.frame(&[]);
    h.input.start_rebind(RebindRequest::new("scroll", BindingSlot::Append)).unwrap();
    h.pad(RawGamepad { left_x: -32767, ..Default::default() });
    h.frame(&[]);
    let Some(RebindOutcome::Captured { binding, .. }) = h.input.take_rebind_outcome() else { panic!() };
    assert_eq!(binding.input, BindingInput::Source(Source::PadAxis(specula_input::PadAxis::LeftX)));
    assert!(binding.invert_x);

    // Mouse drag with the right button onto a 2D axis.
    h.input.start_rebind(RebindRequest::new("look", BindingSlot::Append)).unwrap();
    h.frame(&[Event::MouseMove { x: 0, y: 0 }, Event::MouseDown { button: MouseButton::Right, x: 0, y: 0 }]);
    h.frame(&[Event::MouseMove { x: 60, y: 0 }]);
    let Some(RebindOutcome::Captured { binding, conflicts, .. }) = h.input.take_rebind_outcome() else { panic!() };
    assert_eq!(binding.glyph(PadStyle::Xbox), "RMB+Mouse");
    assert_eq!(conflicts.len(), 1, "same as look's existing binding");
}

#[test]
fn the_input_that_ends_a_rebind_never_reaches_gameplay() {
    let mut h = Harness::new(BASIC);
    // Capturing Z (zap's key) for jump: zap stays idle in the capture frame and after.
    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.frame(&[kd("Z")]);
    assert!(matches!(h.input.take_rebind_outcome(), Some(RebindOutcome::Captured { .. })));
    assert!(!h.input.held("zap") && !h.input.pressed("zap"));
    h.frame(&[]);
    assert!(!h.input.held("zap"), "still held: ignored until released");
    h.frame(&[ku("Z")]);

    // Cancelling with Escape doesn't fire the Escape-bound action.
    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.frame(&[kd("Escape")]);
    assert_eq!(
        h.input.take_rebind_outcome(),
        Some(RebindOutcome::Cancelled {
            request: RebindRequest::new("jump", BindingSlot::Append),
            reason: CancelReason::Input(Source::Key(keys::ESCAPE)),
        })
    );
    assert!(!h.input.held("text_escape"));
    h.frame(&[]);
    assert!(!h.input.held("text_escape"));
    h.frame(&[ku("Escape")]);

    // A press and release inside the capture frame doesn't surface as a pulse afterwards.
    h.input.start_rebind(RebindRequest::new("zap", BindingSlot::Append)).unwrap();
    h.frame(&[kd("Space"), ku("Space")]);
    assert!(matches!(h.input.take_rebind_outcome(), Some(RebindOutcome::Captured { .. })));
    assert!(!h.input.held("jump"));
    h.frame(&[]);
    assert!(!h.input.held("jump") && !h.input.released("jump"));
}

#[test]
fn start_rebind_after_update_ignores_that_frames_press() {
    let mut h = Harness::new(BASIC);
    h.frame(&[kd("F5")]);
    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.frame(&[]);
    assert!(h.input.take_rebind_outcome().is_none() && h.input.rebinding(), "F5 was not captured");
    h.frame(&[ku("F5"), kd("K")]);
    let Some(RebindOutcome::Captured { binding, .. }) = h.input.take_rebind_outcome() else { panic!() };
    assert_eq!(binding, Binding::source(Source::Key(code("K"))));
}

#[test]
fn every_device_can_back_out_of_a_rebind() {
    let mut h = Harness::new(BASIC);
    let cancelled = |h: &mut Harness| match h.input.take_rebind_outcome() {
        Some(RebindOutcome::Cancelled { reason, .. }) => Some(reason),
        _ => None,
    };
    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.pad(RawGamepad { buttons: GamepadButton::Start.bit(), ..Default::default() });
    h.frame(&[]);
    assert_eq!(cancelled(&mut h), Some(CancelReason::Input(Source::Pad(GamepadButton::Start))));
    h.pad(RawGamepad::default());
    h.frame(&[]);

    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.frame(&[touch_down(1, 10, 10), touch_down(2, 60, 10)]);
    h.frame(&[touch_up(1, 10, 10), touch_up(2, 60, 10)]);
    assert_eq!(cancelled(&mut h), Some(CancelReason::Input(Source::TwoFingerTap)));

    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.input.cancel_rebind();
    assert_eq!(cancelled(&mut h), Some(CancelReason::Requested));

    let mut map = ActionMap::from_json_str(BASIC).unwrap();
    map.settings.rebind_timeout_s = 2.0;
    map.settings.rebind_cancel = vec![Source::Pad(GamepadButton::Back)];
    let mut h = Harness { input: Input::new(map), t: 0.0, pads: Default::default() };
    h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Append)).unwrap();
    h.frame(&[]);
    h.wait(1.0);
    assert!(h.input.rebinding());
    h.wait(1.1);
    assert_eq!(cancelled(&mut h), Some(CancelReason::Timeout));
}

#[test]
fn raw_source_queries_work_in_any_mode() {
    let mut h = Harness::new(BASIC);
    h.input.set_text_mode(true);
    h.pad(RawGamepad { buttons: GamepadButton::East.bit(), right_trigger: 255, ..Default::default() });
    h.frame(&[]);
    assert!(h.input.source_pressed(Source::Pad(GamepadButton::East)), "a game's own 'back' while typing");
    assert!(h.input.source_held(Source::Pad(GamepadButton::East)));
    assert!(h.input.source_pressed(Source::PadAxis(specula_input::PadAxis::RightTrigger)));
    assert!(!h.input.held("fire"), "actions are idle in text mode");
    h.frame(&[]);
    assert!(!h.input.source_pressed(Source::Pad(GamepadButton::East)));
    assert!(h.input.source_held(Source::Pad(GamepadButton::East)));
    assert!(!h.input.source_pressed(Source::PadAxis(specula_input::PadAxis::RightTrigger)));
    h.frame(&[Event::MouseWheel { delta: -1.0 }]);
    assert!(h.input.source_pressed(Source::WheelDown) && !h.input.source_held(Source::WheelUp));
}

#[test]
fn rebind_request_errors_are_structured() {
    let mut h = Harness::new(BASIC);
    let err = h.input.start_rebind(RebindRequest::new("nope", BindingSlot::Append)).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownAction);
    let err = h.input.start_rebind(RebindRequest::new("jump", BindingSlot::Replace(9))).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownBinding);
    let mut req = RebindRequest::new("jump", BindingSlot::Replace(0));
    req.part = Some(CompositePart::Up);
    assert_eq!(h.input.start_rebind(req).unwrap_err().code, ErrorCode::BadValue);
}

#[test]
fn bindings_can_be_built_in_code() {
    let mut map = ActionMap::new();
    map.add_action(Action::new("go", ActionKind::Button).bind(Binding::source(Source::Key(keys::ENTER)))).unwrap();
    let err = map
        .add_action(Action::new("bad", ActionKind::Axis2).bind(Binding::source(Source::Key(keys::ENTER))))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::IncompatibleSource);
    assert_eq!(err.path, "actions[1].bindings[0]");
    let mut input = Input::new(map);
    input.update(0.0, &[Event::KeyDown { code: keys::ENTER, repeat: false }], &[]);
    assert!(input.pressed("go"));
    assert!(!input.pressed("unknown action"));
}

#[test]
fn touch_cancel_is_reported_for_one_frame_and_is_not_a_tap() {
    let mut h = Harness::new(BASIC);
    h.frame(&[touch_down(1, 10, 10)]);
    assert!(!h.input.touch_cancelled());
    h.frame(&[Event::PointerCancel { id: 1, kind: PointerKind::Touch }]);
    assert!(h.input.touch_cancelled());
    assert!(!h.input.source_pressed(Source::Tap));
    assert_eq!(h.input.cancelled_contacts(), &[1]);
    h.frame(&[]);
    assert!(!h.input.touch_cancelled());
    // A normal lift is not a cancel.
    h.frame(&[touch_down(2, 10, 10)]);
    h.frame(&[touch_up(2, 10, 10)]);
    assert!(!h.input.touch_cancelled());
    // Losing focus mid-touch cancels it.
    h.frame(&[touch_down(3, 10, 10)]);
    h.frame(&[touch_down(4, 20, 20)]);
    h.frame(&[Event::Focused(false)]);
    assert!(h.input.touch_cancelled());
    let mut ids = h.input.cancelled_contacts().to_vec();
    ids.sort_unstable();
    assert_eq!(ids, vec![3, 4], "focus loss cancels every contact");
    h.frame(&[]);
    assert!(h.input.cancelled_contacts().is_empty());
}
