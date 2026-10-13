//! The per-frame input state machine: platform events and pad states in, action states out.

use specula_platform::gamepad::{normalize_axis, radial_dead_zone, trigger_value};
use specula_platform::{keys, Event, GamepadButton, GamepadState, MouseButton, PointerKind, MAX_GAMEPADS};

use crate::error::{ErrorCode, InputError};
use crate::gesture::Gestures;
use crate::map::{ActionKind, ActionMap, AxisOutput, Binding, BindingInput, BindingSlot, CompositePart, Conflict};
use crate::record::{ActionFrame, ActionState};
use crate::source::{DeviceFamily, PadAxis, PadStyle, Signal, Source, Stick};

/// The device that produced the most recent meaningful input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    KeyboardMouse,
    Gamepad(usize),
    Touch,
}

impl Device {
    pub fn family(self) -> DeviceFamily {
        match self {
            Device::KeyboardMouse => DeviceFamily::KeyboardMouse,
            Device::Gamepad(_) => DeviceFamily::Gamepad,
            Device::Touch => DeviceFamily::Touch,
        }
    }
}

/// Where the pointer is: the mouse, or the primary touch / pen contact (last position kept after
/// lifting, so a tap's location can be read in the frame it fires).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    pub x: f32,
    pub y: f32,
    pub kind: PointerKind,
}

/// A touch or pen contact that is down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// Text-mode input for a text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEvent {
    Char(char),
    /// Any key press (including auto-repeat), for editing keys (Backspace, arrows, Enter).
    Key { code: u32, repeat: bool },
}

/// A request to bind the next input the player makes.
#[derive(Clone, Debug, PartialEq)]
pub struct RebindRequest {
    pub action: String,
    pub slot: BindingSlot,
    /// Rebind one part of a composite (`slot` must then be `Replace` of a composite binding).
    pub part: Option<CompositePart>,
    /// Only accept inputs from this family (e.g. the "controller" column of a settings menu).
    pub family: Option<DeviceFamily>,
}

impl RebindRequest {
    pub fn new(action: &str, slot: BindingSlot) -> RebindRequest {
        RebindRequest { action: action.into(), slot, part: None, family: None }
    }
}

/// Why listening for a rebind stopped without a binding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CancelReason {
    /// One of `settings.rebind_cancel` was pressed.
    Input(Source),
    /// `settings.rebind_timeout_s` passed without input.
    Timeout,
    /// [`Input::cancel_rebind`] was called.
    Requested,
}

/// How a rebind ended. Nothing is changed until [`Input::apply_rebind`]: show `conflicts`
/// first if there are any.
#[derive(Clone, Debug, PartialEq)]
pub enum RebindOutcome {
    Captured { request: RebindRequest, binding: Binding, conflicts: Vec<Conflict> },
    Cancelled { request: RebindRequest, reason: CancelReason },
}

#[derive(Clone, Debug)]
struct Rebind {
    request: RebindRequest,
    /// What the captured source must be able to drive.
    target: ActionKind,
    /// For part rebinding: the composite binding being edited.
    base: Option<Binding>,
    /// A modifier key pressed alone; bound by itself if released without another press.
    pending_modifier: Option<u32>,
    mouse_travel: [f32; 2],
    /// Time of the first update spent listening (for the timeout).
    started: Option<f64>,
}

/// 256 bits, one per key code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Bits([u64; 4]);

impl Bits {
    fn get(&self, i: u32) -> bool {
        i < 256 && self.0[(i / 64) as usize] >> (i % 64) & 1 != 0
    }

    fn set(&mut self, i: u32, on: bool) {
        if i < 256 {
            let (w, b) = ((i / 64) as usize, i % 64);
            if on {
                self.0[w] |= 1 << b;
            } else {
                self.0[w] &= !(1 << b);
            }
        }
    }

    fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..256u32).filter(|&i| self.get(i))
    }
}

fn mouse_bit(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
    }
}

const MOUSE_BUTTONS: [MouseButton; 3] = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

/// Raw device state, updated from events.
#[derive(Clone, Debug, Default)]
struct Devices {
    key_down: Bits,
    /// Went down during this frame.
    key_pulse: Bits,
    /// Down at the start of this frame.
    key_start: Bits,
    /// Held across a mode change: ignored until released.
    key_masked: Bits,
    mouse_down: u8,
    mouse_pulse: u8,
    mouse_start: u8,
    mouse_masked: u8,
    wheel: f32,
    mouse_pos: Option<[f32; 2]>,
    mouse_delta: [f32; 2],
    pads: [GamepadState; MAX_GAMEPADS],
    pad_prev: [GamepadState; MAX_GAMEPADS],
    pad_masked: [u32; MAX_GAMEPADS],
}

impl Devices {
    fn pad_buttons(s: &GamepadState) -> u32 {
        if s.connected {
            s.buttons
        } else {
            0
        }
    }

    /// Everything currently down is ignored until released, and this frame's presses are
    /// forgotten: input that ended a mode (rebind, text entry) must not leak into gameplay.
    fn mask_all(&mut self) {
        self.key_masked = self.key_down;
        self.key_pulse = Bits::default();
        self.mouse_masked = self.mouse_down;
        self.mouse_pulse = 0;
        self.wheel = 0.0;
        for i in 0..MAX_GAMEPADS {
            self.pad_masked[i] = Devices::pad_buttons(&self.pads[i]);
        }
    }

    fn release_all(&mut self) {
        self.key_down = Bits::default();
        self.key_masked = Bits::default();
        self.mouse_down = 0;
        self.mouse_masked = 0;
        self.mouse_pos = None;
    }
}

/// Button-source state this frame.
#[derive(Clone, Copy, Debug, Default)]
struct Press {
    /// Counts as down this frame (held, or pressed and released within the frame).
    active: bool,
    /// Went down this frame.
    fresh: bool,
    /// Was down at frame start, went up and down again within the frame.
    repress: bool,
}

/// Actions, bindings and the live state of every device. One per player.
///
/// Each frame (or simulation tick, for replays): call [`Input::update`] with the window's events
/// and the pads' states, then read actions by name. Text mode, rebinding and replay all go
/// through the same queries, so game code doesn't change.
pub struct Input {
    map: ActionMap,
    /// `[action][binding]` → bindings that block it (same input, more modifiers).
    blockers: Vec<Vec<Vec<(usize, usize)>>>,
    states: Vec<ActionState>,
    dev: Devices,
    gestures: Gestures,
    touches: Vec<Touch>,
    pointer: Option<Pointer>,
    last_device: Option<Device>,
    pad_style: PadStyle,
    pad_filter: Option<usize>,
    focused: bool,
    /// Focus came back this frame: pad buttons held now are ignored until released.
    refocused: bool,
    text_mode: bool,
    text: Vec<TextEvent>,
    rebind: Option<Rebind>,
    rebind_outcome: Option<RebindOutcome>,
    last_time: Option<f64>,
    dt: f32,
}

impl Input {
    pub fn new(map: ActionMap) -> Input {
        let mut input = Input {
            map: ActionMap::new(),
            blockers: Vec::new(),
            states: Vec::new(),
            dev: Devices::default(),
            gestures: Gestures::default(),
            touches: Vec::new(),
            pointer: None,
            last_device: None,
            pad_style: PadStyle::default(),
            pad_filter: None,
            focused: true,
            refocused: false,
            text_mode: false,
            text: Vec::new(),
            rebind: None,
            rebind_outcome: None,
            last_time: None,
            dt: 0.0,
        };
        input.set_map(map);
        input
    }

    pub fn map(&self) -> &ActionMap {
        &self.map
    }

    /// Replaces the action map (e.g. after loading a player's bindings). Action states reset;
    /// buttons still held are ignored until released.
    pub fn set_map(&mut self, map: ActionMap) {
        self.states = vec![ActionState::default(); map.actions().len()];
        self.map = map;
        self.rebuild_blockers();
        self.rebind = None;
        self.dev.mask_all();
    }

    fn rebuild_blockers(&mut self) {
        let actions = self.map.actions();
        let mut blockers: Vec<Vec<Vec<(usize, usize)>>> =
            actions.iter().map(|a| vec![Vec::new(); a.bindings.len()]).collect();
        for (i, a) in actions.iter().enumerate() {
            for (j, b) in a.bindings.iter().enumerate() {
                for (i2, a2) in actions.iter().enumerate() {
                    for (j2, b2) in a2.bindings.iter().enumerate() {
                        if b2.input == b.input
                            && b2.modifiers.len() > b.modifiers.len()
                            && b.modifiers.iter().all(|m| b2.modifiers.contains(m))
                        {
                            blockers[i][j].push((i2, j2));
                        }
                    }
                }
            }
        }
        self.blockers = blockers;
    }

    // ---- per-frame update ----

    /// Advances one frame. `time_s` is a monotonic clock in seconds (only differences are used:
    /// for stick/key rates and tap/long-press timing); `events` are this frame's window events;
    /// `pads` the polled pad states (missing entries count as disconnected).
    ///
    /// Call mode switches ([`Input::start_rebind`], [`Input::set_text_mode`]) *after* `update`,
    /// typically in response to an action read this frame: the next `update` then only sees
    /// newer input, so the press that opened the mode never feeds it.
    pub fn update(&mut self, time_s: f64, events: &[Event], pads: &[GamepadState]) {
        let max_dt = f64::from(self.map.settings.max_dt_s);
        self.dt = self.last_time.map_or(0.0, |t| (time_s - t).clamp(0.0, max_dt)) as f32;
        self.last_time = Some(time_s);
        self.begin_frame();
        for e in events {
            self.event(e, time_s);
        }
        self.update_pads(pads);
        self.gestures.tick(time_s, &self.map.settings);
        self.touches.clear();
        self.touches.extend(self.gestures.contacts().iter().map(|c| Touch { id: c.id, x: c.pos[0], y: c.pos[1] }));
        // Actions stay idle for the whole frame that ends a rebind, so the input that completed
        // or cancelled it never reaches gameplay.
        let listening = self.rebind.is_some();
        if listening {
            self.capture_rebind(time_s);
        }
        self.evaluate(listening);
    }

    fn begin_frame(&mut self) {
        let d = &mut self.dev;
        d.key_start = d.key_down;
        d.key_pulse = Bits::default();
        d.mouse_start = d.mouse_down;
        d.mouse_pulse = 0;
        d.wheel = 0.0;
        d.mouse_delta = [0.0, 0.0];
        self.gestures.begin_frame();
        self.text.clear();
        self.refocused = false;
    }

    fn mouse_at(&mut self, x: i32, y: i32) {
        let pos = [x as f32, y as f32];
        if let Some(last) = self.dev.mouse_pos {
            let d = [pos[0] - last[0], pos[1] - last[1]];
            self.dev.mouse_delta = [self.dev.mouse_delta[0] + d[0], self.dev.mouse_delta[1] + d[1]];
        }
        self.dev.mouse_pos = Some(pos);
        self.pointer = Some(Pointer { x: pos[0], y: pos[1], kind: PointerKind::Mouse });
    }

    fn event(&mut self, e: &Event, now: f64) {
        match *e {
            Event::KeyDown { code, repeat } => {
                if self.text_mode {
                    self.text.push(TextEvent::Key { code, repeat });
                }
                if !repeat && code < 256 {
                    self.dev.key_down.set(code, true);
                    self.dev.key_pulse.set(code, true);
                    self.last_device = Some(Device::KeyboardMouse);
                    if let Some(r) = self.rebind.as_mut() {
                        if keys::is_modifier(code) && r.pending_modifier.is_none() {
                            r.pending_modifier = Some(code);
                        }
                    }
                }
            }
            Event::KeyUp { code } => {
                self.dev.key_down.set(code, false);
                self.dev.key_masked.set(code, false);
            }
            Event::Char(c) => {
                if self.text_mode {
                    self.text.push(TextEvent::Char(c));
                }
            }
            Event::MouseMove { x, y } => {
                self.mouse_at(x, y);
                let d = self.dev.mouse_delta;
                if d[0].abs() + d[1].abs() >= 3.0 {
                    self.last_device = Some(Device::KeyboardMouse);
                }
            }
            Event::MouseDown { button, x, y } => {
                self.mouse_at(x, y);
                self.dev.mouse_down |= mouse_bit(button);
                self.dev.mouse_pulse |= mouse_bit(button);
                self.last_device = Some(Device::KeyboardMouse);
            }
            Event::MouseUp { button, x, y } => {
                self.mouse_at(x, y);
                self.dev.mouse_down &= !mouse_bit(button);
                self.dev.mouse_masked &= !mouse_bit(button);
            }
            Event::MouseWheel { delta } => {
                if delta.is_finite() {
                    self.dev.wheel += delta;
                    self.last_device = Some(Device::KeyboardMouse);
                }
            }
            Event::PointerDown { id, kind, x, y } => match kind {
                PointerKind::Mouse => self.mouse_at(x, y),
                PointerKind::Touch | PointerKind::Pen => {
                    self.gestures.down(id, [x as f32, y as f32], now);
                    self.last_device = Some(Device::Touch);
                    self.touch_pointer(kind);
                }
            },
            Event::PointerMove { id, kind, x, y } => match kind {
                PointerKind::Mouse => self.mouse_at(x, y),
                PointerKind::Touch | PointerKind::Pen => {
                    self.gestures.moved(id, [x as f32, y as f32], &self.map.settings);
                    self.touch_pointer(kind);
                }
            },
            Event::PointerUp { id, kind, x, y } => match kind {
                PointerKind::Mouse => self.mouse_at(x, y),
                PointerKind::Touch | PointerKind::Pen => {
                    self.gestures.moved(id, [x as f32, y as f32], &self.map.settings);
                    self.touch_pointer(kind);
                    self.gestures.up(id, None, now, false, &self.map.settings);
                }
            },
            Event::PointerCancel { id, kind } => {
                if kind != PointerKind::Mouse {
                    self.gestures.up(id, None, now, true, &self.map.settings);
                }
            }
            Event::Focused(false) => {
                // Key-ups won't arrive while unfocused: release everything (actions see a
                // release), and drop touches without firing gestures. Pads are ignored too
                // unless `settings.background_pads`.
                self.focused = false;
                self.dev.release_all();
                self.gestures.clear();
            }
            Event::Focused(true) => {
                if !self.focused {
                    self.refocused = true;
                }
                self.focused = true;
            }
            _ => {}
        }
    }

    fn touch_pointer(&mut self, kind: PointerKind) {
        if let Some(c) = self.gestures.contacts().first() {
            self.pointer = Some(Pointer { x: c.pos[0], y: c.pos[1], kind });
        }
    }

    fn update_pads(&mut self, pads: &[GamepadState]) {
        let threshold = self.map.settings.press_threshold;
        let ignore = !self.focused && !self.map.settings.background_pads;
        for i in 0..MAX_GAMEPADS {
            let new = if ignore { GamepadState::default() } else { pads.get(i).copied().unwrap_or_default() };
            self.dev.pad_prev[i] = self.dev.pads[i];
            self.dev.pads[i] = new;
            let held = Devices::pad_buttons(&new);
            self.dev.pad_masked[i] &= held;
            if self.refocused {
                // Buttons held while focus returns are ignored until released.
                self.dev.pad_masked[i] = held;
                continue;
            }
            if !self.pad_allowed(i) || !new.connected {
                continue;
            }
            let fresh = held & !Devices::pad_buttons(&self.dev.pad_prev[i]);
            let stick = |v: [f32; 2]| v[0] * v[0] + v[1] * v[1] > threshold * threshold;
            if fresh != 0
                || stick(new.left_stick)
                || stick(new.right_stick)
                || new.left_trigger > threshold
                || new.right_trigger > threshold
            {
                self.last_device = Some(Device::Gamepad(i));
            }
        }
    }

    fn pad_allowed(&self, i: usize) -> bool {
        self.pad_filter.is_none_or(|p| p == i)
    }

    // ---- sources ----

    fn press(&self, s: Source) -> Press {
        let d = &self.dev;
        match s {
            Source::Key(c) => {
                let pulse = d.key_pulse.get(c);
                Press {
                    active: (d.key_down.get(c) && !d.key_masked.get(c)) || pulse,
                    fresh: pulse,
                    repress: pulse && d.key_start.get(c),
                }
            }
            Source::Mouse(b) => {
                let bit = mouse_bit(b);
                let pulse = d.mouse_pulse & bit != 0;
                Press {
                    active: (d.mouse_down & bit != 0 && d.mouse_masked & bit == 0) || pulse,
                    fresh: pulse,
                    repress: pulse && d.mouse_start & bit != 0,
                }
            }
            Source::Pad(b) => {
                let mut p = Press::default();
                for i in (0..MAX_GAMEPADS).filter(|&i| self.pad_allowed(i)) {
                    let now = Devices::pad_buttons(&d.pads[i]) & b.bit() != 0;
                    let before = Devices::pad_buttons(&d.pad_prev[i]) & b.bit() != 0;
                    let usable = now && d.pad_masked[i] & b.bit() == 0;
                    p.active |= usable;
                    p.fresh |= usable && !before;
                }
                p
            }
            Source::WheelUp => self.pulse(d.wheel > 0.0),
            Source::WheelDown => self.pulse(d.wheel < 0.0),
            Source::Tap => self.pulse(self.gestures.out.tap),
            Source::TwoFingerTap => self.pulse(self.gestures.out.two_finger_tap),
            Source::LongPress => self.pulse(self.gestures.out.long_press),
            _ => Press::default(),
        }
    }

    fn pulse(&self, on: bool) -> Press {
        Press { active: on, fresh: on, repress: false }
    }

    /// Held for modifier purposes: really down (not just a pulse), not masked.
    fn modifier_held(&self, s: Source) -> bool {
        let d = &self.dev;
        match s {
            Source::Key(c) => d.key_down.get(c) && !d.key_masked.get(c),
            Source::Mouse(b) => d.mouse_down & mouse_bit(b) != 0 && d.mouse_masked & mouse_bit(b) == 0,
            Source::Pad(b) => (0..MAX_GAMEPADS)
                .filter(|&i| self.pad_allowed(i))
                .any(|i| Devices::pad_buttons(&d.pads[i]) & b.bit() != 0 && d.pad_masked[i] & b.bit() == 0),
            _ => false,
        }
    }

    fn analog(&self, s: Source, dead_zone: Option<f32>) -> [f32; 2] {
        self.analog_with(s, dead_zone, &self.dev.pads)
    }

    /// The analog value of a source (largest across allowed pads), reading pad values from
    /// `pads` (the current or the previous poll). `dead_zone` overrides the platform's.
    fn analog_with(&self, s: Source, dead_zone: Option<f32>, pads: &[GamepadState; MAX_GAMEPADS]) -> [f32; 2] {
        let d = &self.dev;
        let pad_value = |f: &dyn Fn(&GamepadState) -> [f32; 2]| {
            let mut best = [0.0f32, 0.0f32];
            for i in (0..MAX_GAMEPADS).filter(|&i| self.pad_allowed(i) && pads[i].connected) {
                let v = f(&pads[i]);
                if v[0] * v[0] + v[1] * v[1] > best[0] * best[0] + best[1] * best[1] {
                    best = v;
                }
            }
            best
        };
        let stick = |p: &GamepadState, which: Stick| -> [f32; 2] {
            match (which, dead_zone) {
                (Stick::Left, None) => p.left_stick,
                (Stick::Right, None) => p.right_stick,
                (Stick::Left, Some(dz)) => {
                    radial_dead_zone([normalize_axis(p.raw.left_x), normalize_axis(p.raw.left_y)], dz)
                }
                (Stick::Right, Some(dz)) => {
                    radial_dead_zone([normalize_axis(p.raw.right_x), normalize_axis(p.raw.right_y)], dz)
                }
            }
        };
        let trigger = |p: &GamepadState, left: bool| -> f32 {
            match (left, dead_zone) {
                (true, None) => p.left_trigger,
                (false, None) => p.right_trigger,
                (true, Some(dz)) => trigger_value(p.raw.left_trigger, dz),
                (false, Some(dz)) => trigger_value(p.raw.right_trigger, dz),
            }
        };
        match s {
            Source::Wheel => [d.wheel, 0.0],
            Source::MouseMotion => d.mouse_delta,
            Source::Stick(which) => pad_value(&|p| stick(p, which)),
            Source::PadAxis(axis) => pad_value(&|p| {
                let v = match axis {
                    PadAxis::LeftX => stick(p, Stick::Left)[0],
                    PadAxis::LeftY => stick(p, Stick::Left)[1],
                    PadAxis::RightX => stick(p, Stick::Right)[0],
                    PadAxis::RightY => stick(p, Stick::Right)[1],
                    PadAxis::LeftTrigger => trigger(p, true),
                    PadAxis::RightTrigger => trigger(p, false),
                };
                [v, 0.0]
            }),
            Source::Dpad => {
                let held = |b: GamepadButton| {
                    let on = (0..MAX_GAMEPADS).filter(|&i| self.pad_allowed(i)).any(|i| {
                        Devices::pad_buttons(&pads[i]) & b.bit() != 0 && d.pad_masked[i] & b.bit() == 0
                    });
                    if on {
                        1.0
                    } else {
                        0.0
                    }
                };
                clamp_unit([
                    held(GamepadButton::DpadRight) - held(GamepadButton::DpadLeft),
                    held(GamepadButton::DpadUp) - held(GamepadButton::DpadDown),
                ])
            }
            Source::Drag => self.gestures.out.drag,
            Source::TwoFingerDrag => self.gestures.out.two_finger_drag,
            Source::Pinch => [self.gestures.out.pinch, 0.0],
            Source::Rotate => [self.gestures.out.rotate, 0.0],
            Source::WheelUp => [d.wheel.max(0.0), 0.0],
            Source::WheelDown => [(-d.wheel).max(0.0), 0.0],
            _ => [if self.press(s).active { 1.0 } else { 0.0 }, 0.0],
        }
    }

    fn level(&self, s: Source, pads: &[GamepadState; MAX_GAMEPADS]) -> f32 {
        let v = self.analog_with(s, None, pads);
        (v[0] * v[0] + v[1] * v[1]).sqrt()
    }

    /// Raw device query: is `source` down (buttons), past `settings.press_threshold` (sticks,
    /// triggers, d-pad) or moving this frame (wheel, mouse motion, gestures)? Ignores bindings
    /// and modes, so a game can implement its own "back" while rebinding or typing. Reads idle
    /// during a replay.
    pub fn source_held(&self, source: Source) -> bool {
        match source.signal() {
            Signal::Button => self.press(source).active,
            _ if source.is_delta() => self.level(source, &self.dev.pads) != 0.0,
            _ => self.level(source, &self.dev.pads) > self.map.settings.press_threshold,
        }
    }

    /// Raw device query: did `source` go down / cross the press threshold this frame? See
    /// [`Input::source_held`].
    pub fn source_pressed(&self, source: Source) -> bool {
        match source.signal() {
            Signal::Button => self.press(source).fresh,
            _ if source.is_delta() => self.source_held(source),
            _ => {
                let t = self.map.settings.press_threshold;
                self.level(source, &self.dev.pads) > t && self.level(source, &self.dev.pad_prev) <= t
            }
        }
    }

    // ---- evaluation ----

    fn binding_live(&self, i: usize, j: usize, b: &Binding) -> bool {
        b.modifiers.iter().all(|&m| self.modifier_held(m))
            && !self.blockers[i][j].iter().any(|&(i2, j2)| {
                self.map.actions()[i2].bindings[j2].modifiers.iter().all(|&m| self.modifier_held(m))
            })
    }

    /// Recomputes every action's state in place. `suppressed`: everything reads idle.
    fn evaluate(&mut self, suppressed: bool) {
        let mut states = std::mem::take(&mut self.states);
        for (i, a) in self.map.actions().iter().enumerate() {
            let prev = states[i];
            let live = !suppressed && (!self.text_mode || a.text_mode);
            states[i] = match a.kind {
                ActionKind::Button => {
                    let (mut held, mut repress) = (false, false);
                    if live {
                        for (j, b) in a.bindings.iter().enumerate() {
                            if !self.binding_live(i, j, b) {
                                continue;
                            }
                            let BindingInput::Source(s) = b.input else { continue };
                            if s.signal() == Signal::Button {
                                let p = self.press(s);
                                held |= p.active;
                                repress |= p.repress;
                            } else {
                                let v = self.analog(s, b.dead_zone);
                                let g = b.gain();
                                let level = match s.signal() {
                                    Signal::Axis => v[0] * g[0],
                                    _ => ((v[0] * g[0]).powi(2) + (v[1] * g[1]).powi(2)).sqrt(),
                                };
                                held |= level > b.threshold.unwrap_or(self.map.settings.press_threshold);
                            }
                        }
                    }
                    ActionState::button(held, held && (!prev.held || repress), prev.held && (!held || repress))
                }
                ActionKind::Axis | ActionKind::Axis2 => {
                    let mut delta = [0.0f32; 2];
                    let mut level = [0.0f32; 2];
                    if live {
                        for (j, b) in a.bindings.iter().enumerate() {
                            if !self.binding_live(i, j, b) {
                                continue;
                            }
                            let (v, is_delta) = match b.input {
                                BindingInput::Source(s) => (self.analog(s, b.dead_zone), s.is_delta()),
                                BindingInput::Composite1 { negative, positive } => {
                                    let on = |s| if self.press(s).active { 1.0 } else { 0.0 };
                                    ([on(positive) - on(negative), 0.0], false)
                                }
                                BindingInput::Composite2 { up, down, left, right } => {
                                    let on = |s| if self.press(s).active { 1.0 } else { 0.0 };
                                    (clamp_unit([on(right) - on(left), on(up) - on(down)]), false)
                                }
                            };
                            let g = b.gain();
                            let v = [v[0] * g[0], v[1] * g[1]];
                            let sum = if is_delta { &mut delta } else { &mut level };
                            sum[0] += v[0];
                            sum[1] += v[1];
                        }
                    }
                    let k = match a.output {
                        AxisOutput::Delta => self.dt,
                        AxisOutput::Value => 1.0,
                    };
                    let mut value = [delta[0] + level[0] * k, delta[1] + level[1] * k];
                    if a.kind == ActionKind::Axis {
                        value[1] = 0.0;
                    }
                    // -0.0 and 0.0 must not differ in recordings.
                    let value = value.map(|v| if v == 0.0 || !v.is_finite() { 0.0 } else { v });
                    ActionState::axis(value, &prev)
                }
            };
        }
        self.states = states;
    }

    // ---- queries ----

    fn get(&self, action: &str) -> ActionState {
        self.map.index_of(action).map(|i| self.states[i]).unwrap_or_default()
    }

    /// State of an action (default/neutral for unknown names).
    pub fn state(&self, action: &str) -> ActionState {
        self.get(action)
    }

    /// States indexed like `map().actions()`.
    pub fn states(&self) -> &[ActionState] {
        &self.states
    }

    pub fn pressed(&self, action: &str) -> bool {
        self.get(action).pressed
    }

    pub fn released(&self, action: &str) -> bool {
        self.get(action).released
    }

    pub fn held(&self, action: &str) -> bool {
        self.get(action).held
    }

    /// One-number axis value (first component for 2D axes).
    pub fn value(&self, action: &str) -> f32 {
        self.get(action).value[0]
    }

    pub fn value2(&self, action: &str) -> [f32; 2] {
        self.get(action).value
    }

    pub fn pointer(&self) -> Option<Pointer> {
        self.pointer
    }

    /// Touch/pen contacts currently down, first-down first.
    pub fn touches(&self) -> &[Touch] {
        &self.touches
    }

    /// A touch or pen contact ended abnormally this frame (`PointerCancel`, or focus lost while
    /// touching). UIs treat it as "abandon the press", not as a release that activates.
    pub fn touch_cancelled(&self) -> bool {
        self.gestures.out.cancelled
    }

    /// Ids (as in `Event::PointerDown::id` / [`Touch::id`]) of the touch / pen contacts that
    /// ended abnormally this frame. A UI abandons only the press made by one of these.
    pub fn cancelled_contacts(&self) -> &[u32] {
        &self.gestures.cancelled_ids
    }

    /// Seconds since the previous update (capped by `settings.max_dt_s`).
    pub fn dt(&self) -> f32 {
        self.dt
    }

    pub fn last_device(&self) -> Option<Device> {
        self.last_device
    }

    pub fn pad_style(&self) -> PadStyle {
        self.pad_style
    }

    /// Which pad labels prompts use (Steam can tell you; XInput can't).
    pub fn set_pad_style(&mut self, style: PadStyle) {
        self.pad_style = style;
    }

    /// Restrict pad input to one pad index (`None`: every pad drives this player).
    pub fn set_gamepad_filter(&mut self, pad: Option<usize>) {
        self.pad_filter = pad;
    }

    /// Glyphs for an action's bindings of one family, with the current pad style.
    pub fn glyphs(&self, action: &str, family: DeviceFamily) -> Vec<String> {
        self.map.glyphs(action, family, self.pad_style)
    }

    /// The prompt to show for an action right now: its first binding for the last-used device's
    /// family (keyboard/mouse before any input). `None` if that family has no binding.
    pub fn prompt(&self, action: &str) -> Option<String> {
        let family = self.last_device.map_or(DeviceFamily::KeyboardMouse, Device::family);
        self.glyphs(action, family).into_iter().next()
    }

    // ---- text mode ----

    pub fn text_mode(&self) -> bool {
        self.text_mode
    }

    /// Text mode: `Char` and key events are collected for a text field ([`Input::text`]) and
    /// every action not marked `text_mode` reads as idle. Keys still held when leaving text mode
    /// are ignored until released, so typing never leaks into gameplay.
    pub fn set_text_mode(&mut self, on: bool) {
        if self.text_mode && !on {
            self.dev.mask_all();
        }
        self.text_mode = on;
    }

    /// This frame's text events (text mode only), in order.
    pub fn text(&self) -> &[TextEvent] {
        &self.text
    }

    // ---- rebinding ----

    /// Listen for the next input and turn it into a binding for `request.action`. While
    /// listening (and in the frame that ends it) every action reads as idle. Any of
    /// `settings.rebind_cancel` cancels (Escape, Start and a two-finger tap by default), as does
    /// `settings.rebind_timeout_s`. Collect the result with [`Input::take_rebind_outcome`].
    ///
    /// Call it after [`Input::update`]: only input from later updates is captured, so the press
    /// that chose "rebind" in a menu is never bound itself.
    pub fn start_rebind(&mut self, request: RebindRequest) -> Result<(), InputError> {
        let i = self.map.index_of(&request.action).ok_or_else(|| {
            InputError::new(ErrorCode::UnknownAction, "", format!("no action named {:?}", request.action))
        })?;
        let action = &self.map.actions()[i];
        let existing = match request.slot {
            BindingSlot::Append => None,
            BindingSlot::Replace(j) => Some(action.bindings.get(j).cloned().ok_or_else(|| {
                InputError::new(
                    ErrorCode::UnknownBinding,
                    format!("actions[{i}].bindings[{j}]"),
                    format!("action {:?} has {} bindings", action.name, action.bindings.len()),
                )
            })?),
        };
        let (target, base) = match request.part {
            None => (action.kind, None),
            Some(part) => {
                let mut probe = existing.clone().ok_or_else(|| {
                    InputError::new(ErrorCode::BadValue, "", "rebinding a composite part needs slot Replace(binding)")
                })?;
                if !probe.input.set_part(part, Source::Key(keys::SPACE)) {
                    return Err(InputError::new(
                        ErrorCode::BadValue,
                        "",
                        format!("binding has no composite part {:?}", part.name()),
                    ));
                }
                (ActionKind::Button, existing)
            }
        };
        self.rebind =
            Some(Rebind { request, target, base, pending_modifier: None, mouse_travel: [0.0, 0.0], started: None });
        self.rebind_outcome = None;
        Ok(())
    }

    pub fn rebinding(&self) -> bool {
        self.rebind.is_some()
    }

    pub fn cancel_rebind(&mut self) {
        if let Some(r) = self.rebind.take() {
            self.rebind_outcome = Some(RebindOutcome::Cancelled { request: r.request, reason: CancelReason::Requested });
            self.dev.mask_all();
        }
    }

    pub fn take_rebind_outcome(&mut self) -> Option<RebindOutcome> {
        self.rebind_outcome.take()
    }

    /// Writes a captured binding into the map. Conflicting bindings are left alone: remove or
    /// swap them first if that's the policy.
    pub fn apply_rebind(&mut self, outcome: &RebindOutcome) -> Result<(), InputError> {
        let RebindOutcome::Captured { request, binding, .. } = outcome else { return Ok(()) };
        self.map.set_binding(&request.action, request.slot, binding.clone())?;
        self.rebuild_blockers();
        Ok(())
    }

    /// Removes a binding (and keeps the chord bookkeeping in sync).
    pub fn remove_binding(&mut self, action: &str, index: usize) -> Result<Binding, InputError> {
        let removed = self.map.remove_binding(action, index)?;
        self.rebuild_blockers();
        Ok(removed)
    }

    fn held_modifier_keys(&self, except: u32) -> Vec<Source> {
        [keys::CTRL, keys::SHIFT, keys::ALT]
            .into_iter()
            .filter(|&k| k != except && self.dev.key_down.get(k))
            .map(Source::Key)
            .collect()
    }

    fn end_rebind(&mut self, outcome: RebindOutcome) {
        self.rebind_outcome = Some(outcome);
        self.dev.mask_all();
    }

    /// Looks for the input to bind in this frame's device state.
    fn capture_rebind(&mut self, now: f64) {
        let Some(mut r) = self.rebind.take() else { return };
        let started = *r.started.get_or_insert(now);
        if let Some(&s) = self.map.settings.rebind_cancel.iter().find(|&&s| self.press(s).fresh) {
            self.end_rebind(RebindOutcome::Cancelled { request: r.request, reason: CancelReason::Input(s) });
            return;
        }
        let timeout = f64::from(self.map.settings.rebind_timeout_s);
        if timeout > 0.0 && now - started >= timeout {
            self.end_rebind(RebindOutcome::Cancelled { request: r.request, reason: CancelReason::Timeout });
            return;
        }
        r.mouse_travel[0] += self.dev.mouse_delta[0];
        r.mouse_travel[1] += self.dev.mouse_delta[1];
        let mut candidates: Vec<Binding> = Vec::new();
        let threshold = self.map.settings.press_threshold;
        let crossed = |prev: f32, now: f32| prev.abs() <= threshold && now.abs() > threshold;
        let pads: Vec<usize> = (0..MAX_GAMEPADS).filter(|&i| self.pad_allowed(i) && self.dev.pads[i].connected).collect();

        // Keys: a non-modifier key with whatever modifiers are held, or a modifier released alone.
        // (Single keys can't drive a 2D axis; composites are rebound part by part.)
        if r.target != ActionKind::Axis2 {
            for code in self.dev.key_pulse.iter().filter(|&c| !keys::is_modifier(c)) {
                candidates.push(Binding::source(Source::Key(code)).with_modifiers(&self.held_modifier_keys(code)));
            }
            if let Some(m) = r.pending_modifier {
                if !self.dev.key_down.get(m) && candidates.is_empty() {
                    candidates.push(Binding::source(Source::Key(m)));
                }
            }
        }
        match r.target {
            ActionKind::Button => {
                for b in MOUSE_BUTTONS {
                    if self.dev.mouse_pulse & mouse_bit(b) != 0 {
                        candidates.push(Binding::source(Source::Mouse(b)).with_modifiers(&self.held_modifier_keys(0)));
                    }
                }
                if self.dev.wheel != 0.0 {
                    candidates.push(Binding::source(if self.dev.wheel > 0.0 { Source::WheelUp } else { Source::WheelDown }));
                }
                for b in GamepadButton::ALL {
                    if self.press(Source::Pad(b)).fresh {
                        candidates.push(Binding::source(Source::Pad(b)));
                    }
                }
                for &i in &pads {
                    let (p, n) = (&self.dev.pad_prev[i], &self.dev.pads[i]);
                    if crossed(p.left_trigger, n.left_trigger) {
                        candidates.push(Binding::source(Source::PadAxis(PadAxis::LeftTrigger)));
                    }
                    if crossed(p.right_trigger, n.right_trigger) {
                        candidates.push(Binding::source(Source::PadAxis(PadAxis::RightTrigger)));
                    }
                }
                let g = self.gestures.out;
                for (on, s) in [(g.tap, Source::Tap), (g.two_finger_tap, Source::TwoFingerTap), (g.long_press, Source::LongPress)] {
                    if on {
                        candidates.push(Binding::source(s));
                    }
                }
            }
            ActionKind::Axis => {
                if self.dev.wheel != 0.0 {
                    candidates.push(Binding::source(Source::Wheel));
                }
                for b in GamepadButton::ALL {
                    if self.press(Source::Pad(b)).fresh {
                        candidates.push(Binding::source(Source::Pad(b)));
                    }
                }
                for &i in &pads {
                    let (p, n) = (&self.dev.pad_prev[i], &self.dev.pads[i]);
                    let axes = [
                        (p.left_trigger, n.left_trigger, PadAxis::LeftTrigger),
                        (p.right_trigger, n.right_trigger, PadAxis::RightTrigger),
                        (p.left_stick[0], n.left_stick[0], PadAxis::LeftX),
                        (p.left_stick[1], n.left_stick[1], PadAxis::LeftY),
                        (p.right_stick[0], n.right_stick[0], PadAxis::RightX),
                        (p.right_stick[1], n.right_stick[1], PadAxis::RightY),
                    ];
                    for (before, now, axis) in axes {
                        if crossed(before, now) {
                            let mut b = Binding::source(Source::PadAxis(axis));
                            // The direction pushed becomes positive.
                            b.invert_x = now < 0.0;
                            candidates.push(b);
                        }
                    }
                }
                let g = self.gestures.out;
                if g.pinch.abs() > 0.05 {
                    candidates.push(Binding::source(Source::Pinch));
                }
                if g.rotate.abs() > 0.1 {
                    candidates.push(Binding::source(Source::Rotate));
                }
            }
            ActionKind::Axis2 => {
                for &i in &pads {
                    let (p, n) = (&self.dev.pad_prev[i], &self.dev.pads[i]);
                    let mag = |v: [f32; 2]| (v[0] * v[0] + v[1] * v[1]).sqrt();
                    if crossed(mag(p.left_stick), mag(n.left_stick)) {
                        candidates.push(Binding::source(Source::Stick(Stick::Left)));
                    }
                    if crossed(mag(p.right_stick), mag(n.right_stick)) {
                        candidates.push(Binding::source(Source::Stick(Stick::Right)));
                    }
                }
                let dpad = [GamepadButton::DpadUp, GamepadButton::DpadDown, GamepadButton::DpadLeft, GamepadButton::DpadRight];
                if dpad.iter().any(|&b| self.press(Source::Pad(b)).fresh) {
                    candidates.push(Binding::source(Source::Dpad));
                }
                let t = r.mouse_travel;
                if t[0].abs() + t[1].abs() > 40.0 {
                    let mut mods: Vec<Source> = MOUSE_BUTTONS
                        .into_iter()
                        .filter(|&b| self.dev.mouse_down & mouse_bit(b) != 0)
                        .map(Source::Mouse)
                        .collect();
                    mods.extend(self.held_modifier_keys(0));
                    candidates.push(Binding::source(Source::MouseMotion).with_modifiers(&mods));
                }
                let g = self.gestures.out;
                if g.drag != [0.0, 0.0] {
                    candidates.push(Binding::source(Source::Drag));
                }
                if g.two_finger_drag[0].abs() + g.two_finger_drag[1].abs() > 4.0 {
                    candidates.push(Binding::source(Source::TwoFingerDrag));
                }
            }
        }
        if let Some(family) = r.request.family {
            candidates.retain(|b| b.family() == family);
        }
        let Some(mut binding) = candidates.into_iter().next() else {
            self.rebind = Some(r);
            return;
        };
        if let (Some(part), Some(base)) = (r.request.part, r.base.as_ref()) {
            let BindingInput::Source(s) = binding.input else { unreachable!("captures are single sources") };
            let mut b = base.clone();
            b.input.set_part(part, s);
            binding = b;
        }
        let ignore = match r.request.slot {
            BindingSlot::Replace(j) => Some(j),
            BindingSlot::Append => None,
        };
        let conflicts = self.map.conflicts_with(&r.request.action, &binding, ignore);
        self.end_rebind(RebindOutcome::Captured { request: r.request, binding, conflicts });
    }

    // ---- recording and replay ----

    /// This frame's recordable state, for a [`Recording`](crate::Recording) or a journal.
    pub fn action_frame(&self) -> ActionFrame {
        let mut frame = ActionFrame::default();
        self.write_action_frame(&mut frame);
        frame
    }

    /// [`Input::action_frame`] into an existing frame, reusing its buffers (no allocation per
    /// tick once warmed up).
    pub fn write_action_frame(&self, out: &mut ActionFrame) {
        out.actions.clone_from(&self.states);
        out.pointer = self.pointer;
        out.text.clone_from(&self.text);
        out.touches.clone_from(&self.touches);
        out.last_device = self.last_device;
        out.dt = self.dt;
    }

    /// Replays a recorded frame instead of [`Input::update`]: the recorded queries (see
    /// [`crate::record`]) answer exactly as they did live. Raw device state is cleared, so
    /// `source_pressed` / `source_held` read idle.
    pub fn apply_frame(&mut self, frame: &ActionFrame) -> Result<(), InputError> {
        if frame.actions.len() != self.states.len() {
            return Err(InputError::new(
                ErrorCode::SchemaMismatch,
                "",
                format!("frame has {} actions, the map has {}", frame.actions.len(), self.states.len()),
            ));
        }
        self.states.clone_from(&frame.actions);
        self.pointer = frame.pointer;
        self.text.clone_from(&frame.text);
        self.touches.clone_from(&frame.touches);
        self.last_device = frame.last_device;
        self.dt = frame.dt;
        self.dev = Devices::default();
        self.gestures.clear();
        self.gestures.begin_frame();
        Ok(())
    }
}

fn clamp_unit(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len > 1.0 {
        [v[0] / len, v[1] / len]
    } else {
        v
    }
}
