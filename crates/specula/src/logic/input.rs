//! A module's input actions, once per tick: window and gamepad input from the frames since the
//! last tick, with what agents hold through `logic_input` on top.
//!
//! Frames and ticks don't line up, so a frame's edges are latched until the next tick reads
//! them: a press shorter than a tick still arrives as `pressed`. Delta axes (mouse motion) sum
//! over the frames; value axes (sticks, held keys) report their level at the tick. What a module
//! sees is the tick's action state only, never keys (F6).

use specula_core::Json;
use specula_input::{ActionKind, ActionMap, ActionState, AxisOutput, Input};
use specula_platform::gamepad::GamepadState;
use specula_platform::Event;
use specula_sdk::Value;
use std::collections::BTreeMap;

/// Longest an agent may hold an action in one `logic_input` call (10 minutes at 30 Hz).
pub const MAX_HOLD_TICKS: u64 = 18_000;

/// An action an agent holds.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    value: [f32; 2],
    ticks_left: u64,
}

pub struct ModuleInput {
    map: ActionMap,
    input: Input,
    /// Per action: the window's state since the last tick (edges latched, deltas summed).
    latch: Vec<ActionState>,
    /// What each action reported to the module last tick (edges come from it).
    last: Vec<ActionState>,
    holds: BTreeMap<String, Hold>,
    /// Seconds of window frames fed (only differences matter to [`Input`]).
    time_s: f64,
}

impl ModuleInput {
    /// From the module's `specula.input.v1` text (empty: no actions).
    pub fn new(actions: &str) -> Result<ModuleInput, specula_input::InputError> {
        let map = if actions.trim().is_empty() { ActionMap::new() } else { ActionMap::from_json_str(actions)? };
        let n = map.actions().len();
        Ok(ModuleInput {
            input: Input::new(map.clone()),
            map,
            latch: vec![ActionState::default(); n],
            last: vec![ActionState::default(); n],
            holds: BTreeMap::new(),
            time_s: 0.0,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.map.actions().is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> + '_ {
        self.map.actions().iter().map(|a| a.name.as_str())
    }

    /// Takes over what an agent held on the build this one replaces (by action name).
    pub fn adopt_holds(&mut self, old: &ModuleInput) {
        for (name, h) in &old.holds {
            if self.map.index_of(name).is_some() {
                self.holds.insert(name.clone(), *h);
            }
        }
    }

    /// One window frame's input.
    pub fn feed(&mut self, events: &[Event], pads: &[GamepadState], dt_s: f32) {
        if self.is_empty() {
            return;
        }
        self.time_s += f64::from(dt_s.max(0.0));
        self.input.update(self.time_s, events, pads);
        for (i, (st, a)) in self.input.states().iter().zip(self.map.actions()).enumerate() {
            let l = &mut self.latch[i];
            l.pressed |= st.pressed;
            l.released |= st.released;
            l.held = st.held;
            let delta = a.kind != ActionKind::Button && a.output == AxisOutput::Delta;
            l.value = if delta { [l.value[0] + st.value[0], l.value[1] + st.value[1]] } else { st.value };
        }
    }

    /// `logic_input`: hold each named action for `ticks` (0 releases it). Values: `true`/`false`
    /// for buttons, a number for axes, `[x, y]` for 2D axes.
    pub fn hold(&mut self, actions: &Json, ticks: u64) -> Result<Json, String> {
        let Some(obj) = actions.as_object() else {
            return Err("'actions' must be an object: {\"action\": true | number | [x, y]}".into());
        };
        let mut plan = Vec::new();
        for (name, v) in obj {
            let Some(a) = self.map.action(name) else {
                let known: Vec<&str> = self.names().collect();
                return Err(format!("no action '{name}' (this module's actions: {})", known.join(", ")));
            };
            let value = match (a.kind, v) {
                (ActionKind::Button, Json::Bool(b)) => [if *b { 1.0 } else { 0.0 }, 0.0],
                (ActionKind::Axis, Json::Num(x)) => [*x as f32, 0.0],
                (ActionKind::Axis2, Json::Arr(xy)) if xy.len() == 2 && xy.iter().all(|c| c.as_f64().is_some()) => {
                    [xy[0].as_f64().unwrap_or(0.0) as f32, xy[1].as_f64().unwrap_or(0.0) as f32]
                }
                (kind, _) => {
                    let want = match kind {
                        ActionKind::Button => "true or false",
                        ActionKind::Axis => "a number",
                        ActionKind::Axis2 => "[x, y]",
                    };
                    return Err(format!("'{name}' is a {} action: give {want}", kind.name()));
                }
            };
            if !value.iter().all(|c| c.is_finite() && c.abs() <= 1.0e6) {
                return Err(format!("'{name}': values must be finite, within ±1e6"));
            }
            plan.push((name.clone(), value));
        }
        for (name, value) in plan {
            if ticks == 0 || value == [0.0, 0.0] {
                self.holds.remove(&name);
            } else {
                self.holds.insert(name, Hold { value, ticks_left: ticks });
            }
        }
        Ok(self.holds_json())
    }

    pub fn holds_json(&self) -> Json {
        Json::obj(self.holds.iter().map(|(n, h)| {
            (n.as_str(), Json::obj([("value", Json::arr(h.value.iter().map(|v| Json::from(*v)))), ("ticks_left", Json::from(h.ticks_left))]))
        }))
    }

    /// The actions for one tick, as the SDK reads them (`{name: [held, pressed, released, x, y]}`),
    /// and the latch reset for the next.
    pub fn take_tick(&mut self) -> Value {
        let mut out = Value::map();
        for (i, a) in self.map.actions().iter().enumerate() {
            let w = std::mem::take(&mut self.latch[i]);
            // The window's held state carries over; only edges and deltas start again.
            self.latch[i].held = w.held;
            if a.kind == ActionKind::Button || a.output == AxisOutput::Value {
                self.latch[i].value = if a.kind == ActionKind::Button { [0.0, 0.0] } else { w.value };
            }
            let hold = self.holds.get_mut(&a.name).filter(|h| h.ticks_left > 0);
            let state = match hold {
                Some(h) => {
                    h.ticks_left -= 1;
                    let value = if a.kind == ActionKind::Button { [1.0, 0.0] } else { h.value };
                    ActionState { held: true, pressed: !self.last[i].held, released: false, value }
                }
                None => {
                    let held = w.held;
                    ActionState {
                        held,
                        pressed: w.pressed || (held && !self.last[i].held),
                        released: w.released || (!held && self.last[i].held),
                        value: w.value,
                    }
                }
            };
            self.last[i] = state;
            if state != ActionState::default() {
                out.insert(
                    &a.name,
                    Value::List(vec![
                        state.held.into(),
                        state.pressed.into(),
                        state.released.into(),
                        state.value[0].into(),
                        state.value[1].into(),
                    ]),
                );
            }
        }
        self.holds.retain(|_, h| h.ticks_left > 0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"{"format": "specula.input.v1", "actions": [
        {"name": "jump", "kind": "button", "bindings": ["Key.Space"]},
        {"name": "move", "kind": "axis2", "output": "value", "bindings": [{"composite": {"up": "Key.W", "down": "Key.S", "left": "Key.A", "right": "Key.D"}}]}
    ]}"#;

    fn state(v: &Value, name: &str) -> Option<(bool, bool, bool, f32, f32)> {
        let l = v.get(name)?.as_list()?;
        Some((l[0].as_bool()?, l[1].as_bool()?, l[2].as_bool()?, l[3].as_f32()?, l[4].as_f32()?))
    }

    #[test]
    fn agent_holds_press_hold_and_release() {
        let mut m = ModuleInput::new(MAP).unwrap();
        m.hold(&Json::parse(r#"{"jump": true, "move": [1, 0.5]}"#).unwrap(), 2).unwrap();
        let t1 = m.take_tick();
        assert_eq!(state(&t1, "jump"), Some((true, true, false, 1.0, 0.0)));
        assert_eq!(state(&t1, "move"), Some((true, true, false, 1.0, 0.5)));
        let t2 = m.take_tick();
        assert_eq!(state(&t2, "jump"), Some((true, false, false, 1.0, 0.0)));
        let t3 = m.take_tick();
        assert_eq!(state(&t3, "jump"), Some((false, false, true, 0.0, 0.0)), "released after its ticks");
        let t4 = m.take_tick();
        assert_eq!(t4, Value::map(), "idle actions are left out");
    }

    #[test]
    fn bad_holds_say_what_to_give() {
        let mut m = ModuleInput::new(MAP).unwrap();
        let e = m.hold(&Json::parse(r#"{"jmup": true}"#).unwrap(), 1).unwrap_err();
        assert!(e.contains("jump, move"), "{e}");
        let e = m.hold(&Json::parse(r#"{"move": true}"#).unwrap(), 1).unwrap_err();
        assert!(e.contains("[x, y]"), "{e}");
        assert!(ModuleInput::new("{\"format\": \"nope\"}").is_err());
        assert!(ModuleInput::new("").unwrap().is_empty());
    }

    #[test]
    fn window_presses_shorter_than_a_tick_still_arrive() {
        use specula_platform::keys;
        let mut m = ModuleInput::new(MAP).unwrap();
        let down = Event::KeyDown { code: keys::SPACE, repeat: false };
        let up = Event::KeyUp { code: keys::SPACE };
        m.feed(&[down], &[], 0.016);
        m.feed(&[up], &[], 0.016);
        let t = m.take_tick();
        let (held, pressed, released, ..) = state(&t, "jump").unwrap();
        assert!(!held && pressed && released, "{t:?}");
        assert_eq!(state(&m.take_tick(), "jump"), None);
    }
}
