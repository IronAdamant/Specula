//! Shared helpers: a small theme, a UI with the built-in box font, and an input harness.
#![allow(dead_code)]

use specula_input::{ActionMap, Input};
use specula_platform::{keys, DeadZones, Event, GamepadState, MouseButton, RawGamepad};
use specula_ui::{add_ui_actions, FontSet, StateMap, Theme, Ui, Viewport};

pub const THEME: &str = r##"{
  "format": "specula.ui.theme.v1",
  "name": "test",
  "colors": { "text": "#ece6d6", "dim": "#b9b2a1", "accent": "#3fe0d0", "stone": "#2b3038" },
  "spacing": { "m": 12 },
  "roles": {
    "body": { "size": 16, "color": "text" },
    "small": { "size": 13, "color": "dim" },
    "tiny": { "size": 8, "color": "text" },
    "title": { "size": 32, "color": "text" }
  },
  "boxes": {
    "panel": { "fill": "stone", "border": 1, "border_color": "#4f5866", "radius": 4, "padding": 8 },
    "button": { "fill": "#3d444f", "radius": 4 },
    "button_hot": { "fill": "#44505a", "border": 1, "border_color": "accent", "radius": 4 },
    "glass": { "fill": "#2b303820", "padding": 4 },
    "tip": { "fill": "#15181d", "padding": 4 }
  },
  "tooltip": { "box": "tip", "role": "small" },
  "toast": { "box": "tip" },
  "threat": { "label_box": "tip" },
  "buttons": {
    "default": {
      "normal": { "box": "button", "text": "text" },
      "hover": { "box": "button_hot" },
      "focused": { "box": "button_hot" },
      "disabled": { "box": "button", "text": "dim" },
      "padding": [8, 12],
      "min_height": 44
    },
    "small": { "normal": { "box": "button", "text": "text" }, "padding": [2, 4] }
  },
  "keycap": { "box": "panel", "role": "small" },
  "scene": ["#0b1020", "#c9d6ff"]
}"##;

pub fn theme() -> Theme {
    Theme::from_json_str(THEME).expect("test theme")
}

pub fn ui() -> Ui {
    Ui::new(theme(), FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()))
}

pub fn screen(name: &str, input: &str, root: &str) -> String {
    format!(r#"{{"format": "specula.ui.screen.v1", "name": "{name}", "input": "{input}", "root": {root}}}"#)
}

/// A UI with one screen loaded and shown.
pub fn ui_with(input: &str, root: &str) -> Ui {
    let mut ui = ui();
    ui.load_screen(&screen("s", input, root)).unwrap_or_else(|e| panic!("{e}"));
    ui.show("s").unwrap();
    ui
}

pub fn view(w: u32, h: u32) -> Viewport {
    Viewport::new([w, h], 1.0)
}

pub struct Harness {
    pub input: Input,
    pub t: f64,
    pub pad: RawGamepad,
}

impl Harness {
    pub fn new() -> Harness {
        let mut map = ActionMap::from_json_str(
            r#"{"format": "specula.input.v1", "actions": [
                {"name": "place", "kind": "button", "bindings": ["Mouse.Left", "Pad.South"]},
                {"name": "call_wave", "kind": "button", "bindings": ["Key.Space", "Pad.LeftStickClick"]},
                {"name": "speed", "kind": "button", "bindings": ["Key.F"]}
            ]}"#,
        )
        .unwrap();
        add_ui_actions(&mut map).unwrap();
        Harness { input: Input::new(map), t: 0.0, pad: RawGamepad::default() }
    }

    /// Without the UI actions: the UI falls back to raw keys / d-pad.
    pub fn bare() -> Harness {
        Harness { input: Input::new(ActionMap::new()), t: 0.0, pad: RawGamepad::default() }
    }

    pub fn step(&mut self, events: &[Event]) {
        self.t += 1.0 / 60.0;
        let pads = [GamepadState::from_raw(self.pad, &DeadZones::default())];
        let connected = self.pad != RawGamepad::default();
        self.input.update(self.t, events, if connected { &pads[..] } else { &[] });
    }

    pub fn frame(&mut self, ui: &mut Ui, events: &[Event], state: &StateMap, v: &Viewport) -> specula_ui::UiEvents {
        self.step(events);
        ui.frame(&self.input, state, v).events.clone()
    }
}

pub fn key(name: &str) -> u32 {
    keys::from_name(name).unwrap()
}

pub fn kd(name: &str) -> Event {
    Event::KeyDown { code: key(name), repeat: false }
}

pub fn ku(name: &str) -> Event {
    Event::KeyUp { code: key(name) }
}

pub fn mv(x: f32, y: f32) -> Event {
    Event::MouseMove { x: x as i32, y: y as i32 }
}

pub fn down(x: f32, y: f32) -> Event {
    Event::MouseDown { button: MouseButton::Left, x: x as i32, y: y as i32 }
}

pub fn up(x: f32, y: f32) -> Event {
    Event::MouseUp { button: MouseButton::Left, x: x as i32, y: y as i32 }
}
