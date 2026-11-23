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
#[derive(Clone, Debug, PartialEq)]
struct Hold {
    value: [f32; 2],
    ticks_left: u64,
    /// Placed since the last tick where none stood: a press, whatever the tick before held. An
    /// action let go and held again between two ticks is pressed again (a script's `5:right,
    /// 6:right` is two presses: 2026-11-23, three of five games of a job whose brief says
    /// every word acts once failed on the second press). A hold made longer while it
    /// stands is no press.
    fresh: bool,
    /// Who placed it (`None`: restored from a save): a freeze of that identity releases it, and
    /// the human taking the action back is reported under it (docs/design/SECURITY.md).
    by: Option<crate::inputguard::Actor>,
}

/// Holds the human took back, kept until the engine reports them (a tick has at most one per
/// action; this bounds a long run without a report).
const MAX_ENDED: usize = 64;

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
    /// Per action: when (in `time_s`) the human last used it at the window. An agent's hold on an
    /// action the human is using takes their input over: the input monitor judges it
    /// (docs/design/SECURITY.md).
    human_at: Vec<f64>,
    /// Holds the window's human ended by using the held action (the window wins): `(action,
    /// holder)`, until [`ModuleInput::take_ended`].
    ended: Vec<(String, Option<crate::inputguard::Actor>)>,
    /// Per action: an agent's hold that was placed and let go again before a tick read it (a
    /// script's `1:put,1:next_good` lets `put` go for `next_good`). The next tick reports it
    /// pressed and released, as a window's press shorter than a tick is: never lost.
    tapped: Vec<bool>,
    /// Every press since the last tick, in order, repeats included (two holds of one action
    /// between two ticks are two; `pressed` can say only one): told to the module with its tick
    /// as the engine's events `press` ([`ModuleInput::take_tick_and_presses`]).
    presses: Vec<String>,
    /// Per action: went down at the window while the game's clock was held, and is still down
    /// ([`ModuleInput::feed_held`]): held when the clock runs again, and no press then.
    ///
    /// Neither it nor [`ModuleInput::touched`] is saved: they are of the device at the window
    /// (keys physically down, the person's hands), which is the person's and not the save's, as
    /// the device state behind the latch is not; a load meets whatever is down then.
    quiet: Vec<bool>,
    /// Per action: the person used it at the window while the game held its clock (pressed,
    /// released, held or moved it). It ends an agent's hold on it at the next tick, as the
    /// window always wins, and is never a press nor a `press` event.
    touched: Vec<bool>,
}

/// Most presses one tick tells a module (the rest of a tick's are not told; `pressed` still is).
pub const MAX_PRESSES: usize = 64;

/// An action the human used at the window this recently (seconds of window frames) is theirs.
pub const HUMAN_RECENT_S: f64 = 1.0;

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
            human_at: vec![f64::NEG_INFINITY; n],
            ended: Vec::new(),
            tapped: vec![false; n],
            presses: Vec::new(),
            quiet: vec![false; n],
            touched: vec![false; n],
        })
    }

    /// The actions among `actions` (an object keyed by action name) the human used at the window
    /// within [`HUMAN_RECENT_S`]: a hold on them would take the human's input over.
    pub fn human_using(&self, actions: &Json) -> Vec<String> {
        let Some(obj) = actions.as_object() else { return Vec::new() };
        obj.iter()
            .filter(|(name, _)| self.map.index_of(name).and_then(|i| self.human_at.get(i)).is_some_and(|t| self.time_s - t <= HUMAN_RECENT_S))
            .map(|(name, _)| name.clone())
            .take(16)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.map.actions().is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> + '_ {
        self.map.actions().iter().map(|a| a.name.as_str())
    }

    /// Takes over the input of the build this one replaces. With the same action map it carries
    /// on exactly (window state, keys held down, agent holds); otherwise by action name: what
    /// agents hold, the latched state, and what each action reported last tick. Either way a
    /// button held across a reload doesn't report `pressed` again.
    pub fn adopt(&mut self, old: ModuleInput) {
        if old.map == self.map {
            *self = old;
            return;
        }
        for (name, h) in &old.holds {
            if self.map.index_of(name).is_some() {
                self.holds.insert(name.clone(), h.clone());
            }
        }
        for (i, a) in old.map.actions().iter().enumerate() {
            if let Some(j) = self.map.index_of(&a.name) {
                if self.map.actions()[j].kind == a.kind {
                    self.last[j] = old.last[i];
                    self.latch[j] = old.latch[i];
                    self.tapped[j] = old.tapped[i];
                    self.quiet[j] = old.quiet[i];
                    self.touched[j] = old.touched[i];
                }
            }
        }
        self.presses = old.presses.into_iter().filter(|name| self.map.index_of(name).is_some()).collect();
        self.time_s = old.time_s;
    }

    /// For save games: what agents hold, the latched window edges and what each action reported
    /// last tick (the edges the next tick derives from), by action name. The device state
    /// behind window input (keys physically down) is the human's, not the save's.
    pub(crate) fn save(&self, w: &mut specula_core::bytes::ByteWriter) {
        let state = |w: &mut specula_core::bytes::ByteWriter, s: &ActionState| {
            w.bool(s.held);
            w.bool(s.pressed);
            w.bool(s.released);
            w.f32(s.value[0]);
            w.f32(s.value[1]);
        };
        w.len_u32(self.holds.len());
        for (name, h) in &self.holds {
            w.str(name);
            w.f32(h.value[0]);
            w.f32(h.value[1]);
            w.u64(h.ticks_left);
            // (Version 2: whether no tick has read it yet, so a save between a script's word and
            // the next tick keeps the press.)
            w.bool(h.fresh);
        }
        w.len_u32(self.map.actions().len());
        for (i, a) in self.map.actions().iter().enumerate() {
            w.str(&a.name);
            state(w, &self.last[i]);
            state(w, &self.latch[i]);
            w.bool(self.tapped[i]);
        }
        w.len_u32(self.presses.len());
        for name in &self.presses {
            w.str(name);
        }
    }

    /// Reads [`ModuleInput::save`]'s bytes, as the logic section's `version` wrote them (1:
    /// before a press waiting for its tick was kept); actions this build doesn't have are
    /// skipped.
    pub(crate) fn restore(&mut self, r: &mut specula_core::bytes::ByteReader<'_>, version: u32) -> Result<(), specula_core::bytes::DecodeError> {
        let v2 = version >= 2;
        let state = |r: &mut specula_core::bytes::ByteReader<'_>| -> Result<ActionState, specula_core::bytes::DecodeError> {
            Ok(ActionState { held: r.bool("held")?, pressed: r.bool("pressed")?, released: r.bool("released")?, value: [r.f32("value")?, r.f32("value")?] })
        };
        let n = r.count("holds", 4096, 20)?;
        let mut holds = BTreeMap::new();
        for _ in 0..n {
            let name = r.str("holds.name", 256)?;
            let value = [r.f32("holds.value")?, r.f32("holds.value")?];
            let ticks_left = r.u64("holds.ticks")?;
            let fresh = if v2 { r.bool("holds.fresh")? } else { false };
            if self.map.index_of(&name).is_some() {
                holds.insert(name, Hold { value, ticks_left, fresh, by: None });
            }
        }
        // (An action's record is its name and two states: 4 + the name + 22 bytes. The least is
        // a name of no letters, 26: the 30 that stood here refused every save of a module whose
        // action has a name of three letters or fewer, such as a new project's own `tap`.)
        let n = r.count("actions", 4096, 26)?;
        let mut tapped = vec![false; self.tapped.len()];
        for _ in 0..n {
            let name = r.str("actions.name", 256)?;
            let (last, latch) = (state(r)?, state(r)?);
            let tap = if v2 { r.bool("actions.tapped")? } else { false };
            if let Some(i) = self.map.index_of(&name) {
                self.last[i] = last;
                self.latch[i] = latch;
                tapped[i] = tap;
            }
        }
        let mut presses = Vec::new();
        if v2 {
            let n = r.count("presses", MAX_PRESSES, 4)?;
            for _ in 0..n {
                let name = r.str("presses.name", 256)?;
                if self.map.index_of(&name).is_some() {
                    presses.push(name);
                }
            }
        }
        self.holds = holds;
        self.tapped = tapped;
        self.presses = presses;
        Ok(())
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
            if st.pressed && self.presses.len() < MAX_PRESSES {
                self.presses.push(a.name.clone());
            }
            if st.held || st.pressed || st.released || st.value != [0.0, 0.0] {
                if let Some(t) = self.human_at.get_mut(i) {
                    *t = self.time_s;
                }
            }
        }
    }

    /// One window frame's input while the game's clock is held (its title, a pause, a run that
    /// ended: no rule ticks): what is held is followed, and nothing pressed now acts when the
    /// clock runs again. A press made in a pause menu (Enter on Resume, keys in the settings)
    /// is no press of the game's: no edge is latched, no `press` is told, a key still down
    /// when the clock starts is not pressed then, and motion summed is dropped. A release of
    /// what was held before is still told (the rules saw it held). An agent's holds are kept,
    /// except that the person using one of their actions ends its hold at the next tick,
    /// as the window always wins ([`ModuleInput::touched`]).
    pub fn feed_held(&mut self, events: &[Event], pads: &[GamepadState], dt_s: f32) {
        let before = self.presses.len();
        self.feed(events, pads, dt_s);
        self.presses.truncate(before);
        for (i, a) in self.map.actions().iter().enumerate() {
            let l = &mut self.latch[i];
            // What the person did is kept for the take-over alone (the window wins over an
            // agent's hold, paused or not), and is no press of the game's.
            self.touched[i] |= l.pressed || l.released || l.held || l.value != [0.0, 0.0];
            l.pressed = false;
            l.released = false;
            if a.kind != ActionKind::Button && a.output == AxisOutput::Delta {
                l.value = [0.0, 0.0];
            }
            // Down now and not when the rules last ticked: held when the clock runs, never a
            // press then. (Let go again, it was never there.)
            self.quiet[i] = l.held && !self.last[i].held;
        }
    }

    /// `logic_input`: hold each named action for `ticks` (0 releases it), as `by`. Values:
    /// `true`/`false` for buttons, a number for axes, `[x, y]` for 2D axes.
    pub fn hold(&mut self, actions: &Json, ticks: u64, by: Option<crate::inputguard::Actor>) -> Result<Json, String> {
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
                // Let go before any tick read it: still a press, told at the next tick.
                if self.holds.remove(&name).is_some_and(|h| h.fresh) {
                    if let Some(i) = self.map.index_of(&name) {
                        self.tapped[i] = true;
                    }
                }
            } else {
                // (A hold made longer while it stands is no new press, and one no tick has read
                // yet stays the press it is.)
                let fresh = self.holds.get(&name).is_none_or(|h| h.fresh);
                if !self.holds.contains_key(&name) && self.presses.len() < MAX_PRESSES {
                    self.presses.push(name.clone());
                }
                self.holds.insert(name, Hold { value, ticks_left: ticks, fresh, by: by.clone() });
            }
        }
        Ok(self.holds_json())
    }

    /// Ends an agent's hold of `name` (whatever its kind), as if released.
    pub fn release(&mut self, name: &str) {
        self.holds.remove(name);
    }

    /// Ends every hold the identity `key` placed (its session froze); how many.
    pub fn release_holds_of(&mut self, key: &str) -> usize {
        let before = self.holds.len();
        self.holds.retain(|_, h| h.by.as_ref().and_then(crate::inputguard::Actor::key) != Some(key));
        before - self.holds.len()
    }

    /// Holds the window's human took back since the last call: `(action, holder)`.
    pub fn take_ended(&mut self) -> Vec<(String, Option<crate::inputguard::Actor>)> {
        std::mem::take(&mut self.ended)
    }

    pub fn holds_json(&self) -> Json {
        Json::obj(self.holds.iter().map(|(n, h)| {
            (n.as_str(), Json::obj([("value", Json::arr(h.value.iter().map(|v| Json::from(*v)))), ("ticks_left", Json::from(h.ticks_left))]))
        }))
    }

    /// The actions for one tick, as the SDK reads them (`{name: [held, pressed, released, x, y]}`),
    /// and the latch reset for the next.
    pub fn take_tick(&mut self) -> Value {
        self.take_tick_and_presses().0
    }

    /// [`ModuleInput::take_tick`], and every press since the last tick in the order made,
    /// repeats included: each action reported `pressed` is among them at least once.
    pub fn take_tick_and_presses(&mut self) -> (Value, Vec<String>) {
        let mut out = Value::map();
        let mut presses = std::mem::take(&mut self.presses);
        for (i, a) in self.map.actions().iter().enumerate() {
            let tapped = std::mem::take(&mut self.tapped[i]);
            // (Down since a frame of a held clock: held now, and no press of the game's.)
            let quiet = std::mem::take(&mut self.quiet[i]);
            // (Used by the person while the game held its clock: their take-over, never a press.)
            let touched = std::mem::take(&mut self.touched[i]);
            let w = std::mem::take(&mut self.latch[i]);
            // The window's held state carries over; only edges and deltas start again.
            self.latch[i].held = w.held;
            if a.kind == ActionKind::Button || a.output == AxisOutput::Value {
                self.latch[i].value = if a.kind == ActionKind::Button { [0.0, 0.0] } else { w.value };
            }
            // The window wins: the human's own input on a held action ends the hold (the action
            // reports the window's state), and the engine reports who held it.
            let human = touched || w.held || w.pressed || w.released || w.value != [0.0, 0.0];
            if human {
                if let Some(h) = self.holds.remove(&a.name) {
                    if self.ended.len() < MAX_ENDED {
                        self.ended.push((a.name.clone(), h.by));
                    }
                }
            }
            let hold = self.holds.get_mut(&a.name).filter(|h| h.ticks_left > 0);
            let state = match hold {
                Some(h) => {
                    h.ticks_left -= 1;
                    let fresh = std::mem::take(&mut h.fresh);
                    let value = if a.kind == ActionKind::Button { [1.0, 0.0] } else { h.value };
                    ActionState { held: true, pressed: fresh || tapped || !self.last[i].held, released: false, value }
                }
                None => {
                    let held = w.held;
                    // (An agent's press let go before this tick: pressed and released in it, as a
                    // window's press shorter than a tick is.)
                    ActionState {
                        held,
                        pressed: w.pressed || tapped || (held && !self.last[i].held && !quiet),
                        released: w.released || (tapped && !held) || (!held && self.last[i].held),
                        value: w.value,
                    }
                }
            };
            if state.pressed && !presses.contains(&a.name) {
                presses.push(a.name.clone());
            }
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
        (out, presses)
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

    /// A module's input comes back from a save whatever its actions are called: one action
    /// with a one-letter name is the smallest record there is (review R1 of M6 part 3: a new
    /// project's own module, whose one action is `tap`, could not load any save: the reader
    /// took 30 bytes as the least an action is, and `tap` is 29).
    #[test]
    fn input_with_a_short_action_name_comes_back_from_a_save() {
        for name in ["t", "tap", "jump", "a_much_longer_action_name"] {
            let map = format!(r#"{{"format": "specula.input.v1", "actions": [{{"name": "{name}", "kind": "button", "bindings": ["Key.Space"]}}]}}"#);
            let mut m = ModuleInput::new(&map).unwrap();
            m.hold(&Json::parse(&format!(r#"{{"{name}": true}}"#)).unwrap(), 3, None).unwrap();
            m.take_tick();
            let mut w = specula_core::bytes::ByteWriter::new();
            m.save(&mut w);
            let bytes = w.into_bytes();
            let mut back = ModuleInput::new(&map).unwrap();
            let mut r = specula_core::bytes::ByteReader::new(&bytes);
            back.restore(&mut r, crate::logic::save::LOGIC_VERSION).unwrap_or_else(|e| panic!("an action named {name:?}: {} ({})", e.message, e.code));
            r.finish().unwrap();
            // The hold goes on where it was: held, no longer a fresh press.
            assert_eq!(state(&back.take_tick(), name), Some((true, false, false, 1.0, 0.0)), "{name}");
        }
    }

    #[test]
    fn agent_holds_press_hold_and_release() {
        let mut m = ModuleInput::new(MAP).unwrap();
        m.hold(&Json::parse(r#"{"jump": true, "move": [1, 0.5]}"#).unwrap(), 2, None).unwrap();
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
        let e = m.hold(&Json::parse(r#"{"jmup": true}"#).unwrap(), 1, None).unwrap_err();
        assert!(e.contains("jump, move"), "{e}");
        let e = m.hold(&Json::parse(r#"{"move": true}"#).unwrap(), 1, None).unwrap_err();
        assert!(e.contains("[x, y]"), "{e}");
        assert!(ModuleInput::new("{\"format\": \"nope\"}").is_err());
        assert!(ModuleInput::new("").unwrap().is_empty());
    }

    /// An action let go and held again between two ticks is pressed again; one made longer
    /// while it stands is not.
    #[test]
    fn an_action_let_go_and_held_again_is_pressed_again() {
        let jump = |m: &mut ModuleInput, ticks: u64| m.hold(&Json::parse(r#"{"jump": true}"#).unwrap(), ticks, None).map(drop);
        let mut m = ModuleInput::new(MAP).unwrap();
        jump(&mut m, 10).unwrap();
        assert_eq!(state(&m.take_tick(), "jump"), Some((true, true, false, 1.0, 0.0)));
        assert_eq!(state(&m.take_tick(), "jump"), Some((true, false, false, 1.0, 0.0)), "held on: no press");
        // The same word again, as a script gives it: let go (no ticks), then held.
        jump(&mut m, 0).unwrap();
        jump(&mut m, 10).unwrap();
        assert_eq!(state(&m.take_tick(), "jump"), Some((true, true, false, 1.0, 0.0)), "a second press, with no tick between");
        assert_eq!(state(&m.take_tick(), "jump"), Some((true, false, false, 1.0, 0.0)));
        // A hold made longer while it stands (an agent keeping a key down) is no press.
        jump(&mut m, 30).unwrap();
        assert_eq!(state(&m.take_tick(), "jump"), Some((true, false, false, 1.0, 0.0)));
    }

    /// Several words at one tick (a script's `1:jump,1:move=1/0`, `1:jump,1:jump`): each one
    /// held and let go again before the tick is still a press at it, and every press is told in
    /// the order made, repeats included; a save between the words and the tick keeps them all.
    #[test]
    fn several_holds_between_two_ticks_are_each_a_press_in_order() {
        let jump = |m: &mut ModuleInput, ticks: u64| m.hold(&Json::parse(r#"{"jump": true}"#).unwrap(), ticks, None).map(drop);
        let walk = |m: &mut ModuleInput, ticks: u64| m.hold(&Json::parse(r#"{"move": [1, 0]}"#).unwrap(), ticks, None).map(drop);
        // Two actions: `jump` let go for `move` before the tick, as a script's next word does.
        let mut m = ModuleInput::new(MAP).unwrap();
        jump(&mut m, 10).unwrap();
        m.hold(&Json::parse(r#"{"jump": true}"#).unwrap(), 0, None).unwrap();
        walk(&mut m, 10).unwrap();
        let (t, presses) = m.take_tick_and_presses();
        assert_eq!(state(&t, "jump"), Some((false, true, true, 0.0, 0.0)), "pressed and let go in the one tick: {t:?}");
        assert_eq!(state(&t, "move"), Some((true, true, false, 1.0, 0.0)));
        assert_eq!(presses, ["jump", "move"], "in the order given");
        let (t, presses) = m.take_tick_and_presses();
        assert_eq!((state(&t, "jump"), state(&t, "move").map(|s| s.1)), (None, Some(false)), "each once");
        assert!(presses.is_empty());
        // The same action twice: one `pressed`, two presses.
        walk(&mut m, 0).unwrap();
        jump(&mut m, 10).unwrap();
        jump(&mut m, 0).unwrap();
        jump(&mut m, 10).unwrap();
        let (t, presses) = m.take_tick_and_presses();
        assert_eq!(state(&t, "jump"), Some((true, true, false, 1.0, 0.0)));
        assert_eq!(presses, ["jump", "jump"]);
        // Probing a word with no ticks, where nothing was held, presses nothing.
        let mut idle = ModuleInput::new(MAP).unwrap();
        jump(&mut idle, 0).unwrap();
        assert_eq!(idle.take_tick_and_presses(), (Value::map(), Vec::new()));
        // A hold made again before any tick read it stays one press (not two, not none).
        jump(&mut idle, 10).unwrap();
        jump(&mut idle, 20).unwrap();
        let (t, presses) = idle.take_tick_and_presses();
        assert_eq!((state(&t, "jump").map(|s| s.1), presses), (Some(true), vec!["jump".to_owned()]));
        // A save between the words and the tick: the tick after the load is the same tick.
        let mut a = ModuleInput::new(MAP).unwrap();
        jump(&mut a, 10).unwrap();
        jump(&mut a, 0).unwrap();
        jump(&mut a, 10).unwrap();
        let mut w = specula_core::bytes::ByteWriter::new();
        a.save(&mut w);
        let bytes = w.into_bytes();
        let mut b = ModuleInput::new(MAP).unwrap();
        let mut r = specula_core::bytes::ByteReader::new(&bytes);
        b.restore(&mut r, crate::logic::save::LOGIC_VERSION).unwrap();
        r.finish().unwrap();
        assert_eq!(b.take_tick_and_presses(), a.take_tick_and_presses());
        // And a window's press is told as a press too.
        use specula_platform::keys;
        let mut human = ModuleInput::new(MAP).unwrap();
        human.feed(&[Event::KeyDown { code: keys::SPACE, repeat: false }], &[], 0.016);
        assert_eq!(human.take_tick_and_presses().1, ["jump"]);
    }

    /// What the player presses while the game's clock is held (a pause menu, the title) does not
    /// act in the game when it runs again: no edge, no `press`, and a key still down then is no
    /// press; a release of what was held before the pause is still told, and an agent's hold
    /// made then is its own press.
    #[test]
    fn presses_while_the_clock_is_held_do_not_act_after_it() {
        use specula_platform::keys;
        let down = || Event::KeyDown { code: keys::SPACE, repeat: false };
        let up = || Event::KeyUp { code: keys::SPACE };
        // Pressed and let go in the pause menu (Enter, Space in a menu): nothing at the next tick.
        let mut m = ModuleInput::new(MAP).unwrap();
        m.feed_held(&[down()], &[], 0.016);
        m.feed_held(&[up()], &[], 0.016);
        assert_eq!(m.take_tick_and_presses(), (Value::map(), Vec::new()));
        // Pressed in the pause and still down when it runs: held, and no press.
        m.feed_held(&[down()], &[], 0.016);
        m.feed(&[], &[], 0.016);
        let (t, presses) = m.take_tick_and_presses();
        assert_eq!((state(&t, "jump"), presses), (Some((true, false, false, 1.0, 0.0)), Vec::new()));
        // Held before the pause, let go in it: the release is told when it runs.
        let mut m = ModuleInput::new(MAP).unwrap();
        m.feed(&[down()], &[], 0.016);
        assert_eq!(state(&m.take_tick(), "jump").map(|s| s.1), Some(true));
        m.feed_held(&[up()], &[], 0.016);
        m.feed(&[], &[], 0.016);
        assert_eq!(state(&m.take_tick(), "jump"), Some((false, false, true, 0.0, 0.0)));
        // An agent's hold placed while the clock is held is still its press.
        m.feed_held(&[], &[], 0.016);
        m.hold(&Json::parse(r#"{"jump": true}"#).unwrap(), 5, None).unwrap();
        let (t, presses) = m.take_tick_and_presses();
        assert_eq!((state(&t, "jump").map(|s| s.1), presses), (Some(true), vec!["jump".to_owned()]));
    }

    /// The window wins over an agent's hold while the game's clock is held too: a tap of the
    /// held action in the pause menu, or mouse motion on a held delta axis, ends the hold at
    /// the next tick and names its holder; neither is a press of the game's.
    #[test]
    fn the_person_takes_a_held_action_back_while_the_clock_is_held() {
        use crate::authority::Authority;
        use crate::inputguard::Actor;
        use specula_platform::keys;
        const LOOK: &str = r#"{"format": "specula.input.v1", "actions": [
            {"name": "jump", "kind": "button", "bindings": ["Key.Space"]},
            {"name": "look", "kind": "axis2", "output": "delta", "bindings": ["Mouse.Motion"]}
        ]}"#;
        let mallory = Actor::identity("mal-key", "mallory", Authority::Agent);
        let mut m = ModuleInput::new(LOOK).unwrap();
        m.hold(&Json::parse(r#"{"jump": true, "look": [1, 0]}"#).unwrap(), 100, Some(mallory.clone())).unwrap();
        assert_eq!(state(&m.take_tick(), "jump").map(|s| s.1), Some(true));
        // Paused: the person taps Space and moves the mouse.
        m.feed_held(&[Event::KeyDown { code: keys::SPACE, repeat: false }], &[], 0.016);
        m.feed_held(&[Event::KeyUp { code: keys::SPACE }], &[], 0.016);
        m.feed_held(&[Event::MouseMove { x: 10, y: 10 }], &[], 0.016);
        m.feed_held(&[Event::MouseMove { x: 60, y: 40 }], &[], 0.016);
        let (t, presses) = m.take_tick_and_presses();
        assert!(presses.is_empty(), "no press of the game's: {presses:?}");
        let mut ended = m.take_ended();
        ended.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(ended, vec![("jump".to_owned(), Some(mallory.clone())), ("look".to_owned(), Some(mallory))], "both holds ended, under their holder");
        assert_eq!(state(&t, "jump").map(|s| (s.0, s.1)), Some((false, false)), "the hold is gone: {t:?}");
        assert!(m.holds_json().as_object().is_some_and(|h| h.is_empty()));
    }

    /// The input of a save written before presses were kept (the logic section's version 1)
    /// still loads: a hold in it goes on, no longer a press.
    #[test]
    fn an_input_saved_in_the_first_layout_loads() {
        let mut w = specula_core::bytes::ByteWriter::new();
        w.len_u32(1);
        w.str("jump");
        w.f32(1.0);
        w.f32(0.0);
        w.u64(5);
        w.len_u32(0);
        let bytes = w.into_bytes();
        let mut m = ModuleInput::new(MAP).unwrap();
        let mut r = specula_core::bytes::ByteReader::new(&bytes);
        m.restore(&mut r, 1).unwrap();
        r.finish().unwrap();
        assert_eq!(state(&m.take_tick(), "jump").map(|s| s.0), Some(true));
    }

    #[test]
    fn a_reload_mid_hold_is_no_new_press() {
        let mut old = ModuleInput::new(MAP).unwrap();
        old.hold(&Json::parse(r#"{"jump": true}"#).unwrap(), 10, None).unwrap();
        assert_eq!(state(&old.take_tick(), "jump"), Some((true, true, false, 1.0, 0.0)));
        let mut new = ModuleInput::new(MAP).unwrap();
        new.adopt(old);
        assert_eq!(state(&new.take_tick(), "jump"), Some((true, false, false, 1.0, 0.0)), "still held, not pressed again");
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

    /// Review R1 of m5-inputguard, major 5: an agent's hold never takes the human's input over.
    /// The human's own input on a held action ends the hold (the window wins), reported under the
    /// hold's holder; a freeze releases the holds its identity placed.
    #[test]
    fn the_human_takes_a_held_action_back_and_a_freeze_releases_holds() {
        use crate::authority::Authority;
        use crate::inputguard::Actor;
        use specula_platform::keys;
        let mut m = ModuleInput::new(MAP).unwrap();
        let mallory = Actor::identity("mal-key", "mallory", Authority::Agent);
        // Placed while the human was idle: 18,000 ticks of "move right".
        m.hold(&Json::parse(r#"{"move": [1, 0], "jump": true}"#).unwrap(), MAX_HOLD_TICKS, Some(mallory.clone())).unwrap();
        assert_eq!(state(&m.take_tick(), "move"), Some((true, true, false, 1.0, 0.0)));
        // The human presses A (move left): the window wins at once, and the hold is over.
        m.feed(&[Event::KeyDown { code: u32::from(b'A'), repeat: false }], &[], 0.016);
        let t = m.take_tick();
        assert_eq!(state(&t, "move").map(|s| (s.0, s.3)), Some((true, -1.0)), "the human's state is reported: {t:?}");
        assert_eq!(m.take_ended(), vec![("move".to_owned(), Some(mallory.clone()))], "reported under its holder");
        assert!(m.take_ended().is_empty());
        m.feed(&[Event::KeyUp { code: u32::from(b'A') }], &[], 0.016);
        let t = m.take_tick();
        assert_eq!(state(&t, "move").map(|s| (s.0, s.2)), Some((false, true)), "released when the human lets go: the hold doesn't come back");
        // The other hold (jump) stays until its identity is frozen.
        assert_eq!(state(&t, "jump").map(|s| s.0), Some(true));
        m.hold(&Json::parse(r#"{"move": [0, 1]}"#).unwrap(), 50, Some(Actor::identity("lee-key", "lee", Authority::Lead))).unwrap();
        assert_eq!(m.release_holds_of("mal-key"), 1);
        let t = m.take_tick();
        assert_eq!(state(&t, "jump").map(|s| (s.0, s.2)), Some((false, true)), "mallory's jump is released");
        assert_eq!(state(&t, "move").map(|s| s.0), Some(true), "another identity's hold stays");
        // Bounded: a run nobody reports on keeps at most MAX_ENDED.
        for _ in 0..(MAX_ENDED + 10) {
            m.hold(&Json::parse(r#"{"jump": true}"#).unwrap(), 5, Some(mallory.clone())).unwrap();
            m.feed(&[Event::KeyDown { code: keys::SPACE, repeat: false }], &[], 0.016);
            let _ = m.take_tick();
            m.feed(&[Event::KeyUp { code: keys::SPACE }], &[], 0.016);
        }
        assert_eq!(m.take_ended().len(), MAX_ENDED);
    }
}
