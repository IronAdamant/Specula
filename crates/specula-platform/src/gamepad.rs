//! Gamepads: up to [`MAX_GAMEPADS`] pads polled once per frame.
//!
//! On Windows the backend is XInput, **loaded at runtime** (`xinput1_4.dll`, falling back to
//! `xinput9_1_0.dll`) through `specula_sys::dynlib`, never linked: a machine without it still runs,
//! just without pads. Steam Input and Proton (Steam Deck) present every controller as an XInput
//! pad. Other platforms have no backend yet: [`Gamepads::new`] returns a clear "unsupported" error.
//!
//! Values are device-neutral: buttons are named by position ([`GamepadButton::South`] is A on
//! Xbox, Cross on PlayStation), sticks are `-1..=1` with **+y up**, triggers `0..=1`. Radial dead
//! zones are applied here ([`DeadZones`], XInput's recommended values by default); the unfiltered
//! values are kept in [`GamepadState::raw`] for callers that want their own.

use crate::PlatformError;

/// Pads polled (XInput supports four user indices).
pub const MAX_GAMEPADS: usize = 4;

/// Digital buttons, named by position so the same binding works on every pad family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GamepadButton {
    /// Bottom face button: A (Xbox, Steam Deck), Cross (PlayStation).
    South,
    /// Right face button: B, Circle.
    East,
    /// Left face button: X, Square.
    West,
    /// Top face button: Y, Triangle.
    North,
    LeftBumper,
    RightBumper,
    /// View / Back / Select / Create.
    Back,
    /// Menu / Start / Options.
    Start,
    /// Left stick click.
    LeftStick,
    /// Right stick click.
    RightStick,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl GamepadButton {
    pub const ALL: [GamepadButton; 14] = [
        GamepadButton::South,
        GamepadButton::East,
        GamepadButton::West,
        GamepadButton::North,
        GamepadButton::LeftBumper,
        GamepadButton::RightBumper,
        GamepadButton::Back,
        GamepadButton::Start,
        GamepadButton::LeftStick,
        GamepadButton::RightStick,
        GamepadButton::DpadUp,
        GamepadButton::DpadDown,
        GamepadButton::DpadLeft,
        GamepadButton::DpadRight,
    ];

    /// This button's bit in [`GamepadState::buttons`].
    pub fn bit(self) -> u32 {
        1 << self as u32
    }

    /// Stable name (`"South"`, `"LeftBumper"`, …).
    pub fn name(self) -> &'static str {
        match self {
            GamepadButton::South => "South",
            GamepadButton::East => "East",
            GamepadButton::West => "West",
            GamepadButton::North => "North",
            GamepadButton::LeftBumper => "LeftBumper",
            GamepadButton::RightBumper => "RightBumper",
            GamepadButton::Back => "Back",
            GamepadButton::Start => "Start",
            GamepadButton::LeftStick => "LeftStick",
            GamepadButton::RightStick => "RightStick",
            GamepadButton::DpadUp => "DpadUp",
            GamepadButton::DpadDown => "DpadDown",
            GamepadButton::DpadLeft => "DpadLeft",
            GamepadButton::DpadRight => "DpadRight",
        }
    }
}

/// Unfiltered pad values in XInput's ranges (other backends convert into these).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawGamepad {
    /// [`GamepadButton::bit`] per held button.
    pub buttons: u32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// Stick axes, +y up.
    pub left_x: i16,
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
}

/// Dead zones as fractions of full deflection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeadZones {
    /// Radial dead zone of the left stick (XInput recommends 7849/32767 ≈ 0.24).
    pub left_stick: f32,
    /// Radial dead zone of the right stick (XInput recommends 8689/32767 ≈ 0.27).
    pub right_stick: f32,
    /// Trigger threshold (XInput recommends 30/255 ≈ 0.12).
    pub trigger: f32,
}

impl Default for DeadZones {
    fn default() -> Self {
        DeadZones { left_stick: 7849.0 / 32767.0, right_stick: 8689.0 / 32767.0, trigger: 30.0 / 255.0 }
    }
}

/// One pad's state after dead-zone processing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GamepadState {
    pub connected: bool,
    /// [`GamepadButton::bit`] per held button.
    pub buttons: u32,
    /// `[x, y]`, +x right, +y up; radial dead zone applied and rescaled, magnitude ≤ 1.
    pub left_stick: [f32; 2],
    pub right_stick: [f32; 2],
    /// `0..=1`, threshold applied and rescaled.
    pub left_trigger: f32,
    pub right_trigger: f32,
    /// The values before dead zones.
    pub raw: RawGamepad,
}

impl GamepadState {
    /// A connected pad's state from raw values.
    pub fn from_raw(raw: RawGamepad, dead_zones: &DeadZones) -> GamepadState {
        GamepadState {
            connected: true,
            buttons: raw.buttons,
            left_stick: radial_dead_zone([normalize_axis(raw.left_x), normalize_axis(raw.left_y)], dead_zones.left_stick),
            right_stick: radial_dead_zone(
                [normalize_axis(raw.right_x), normalize_axis(raw.right_y)],
                dead_zones.right_stick,
            ),
            left_trigger: trigger_value(raw.left_trigger, dead_zones.trigger),
            right_trigger: trigger_value(raw.right_trigger, dead_zones.trigger),
            raw,
        }
    }

    pub fn held(&self, button: GamepadButton) -> bool {
        self.buttons & button.bit() != 0
    }
}

/// A change between two polls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamepadEvent {
    Connected { pad: usize },
    /// Held buttons are reported released (`ButtonUp`) just before this.
    Disconnected { pad: usize },
    ButtonDown { pad: usize, button: GamepadButton },
    ButtonUp { pad: usize, button: GamepadButton },
}

/// A stick axis (`-32768..=32767`) as `-1..=1`.
pub fn normalize_axis(v: i16) -> f32 {
    (f32::from(v) / 32767.0).max(-1.0)
}

/// Scaled radial dead zone: inside `dead` the stick reads zero; outside, the magnitude is rescaled
/// from `dead..1` to `0..1` keeping the direction (no jump at the edge, full range reachable).
/// Magnitudes above 1 (square gates at the diagonals) are clamped to 1.
pub fn radial_dead_zone(v: [f32; 2], dead: f32) -> [f32; 2] {
    let dead = dead.clamp(0.0, 0.99);
    let mag = (v[0] * v[0] + v[1] * v[1]).sqrt();
    // NaN magnitudes read as centred too.
    if mag.is_nan() || mag <= dead {
        return [0.0, 0.0];
    }
    let scaled = ((mag.min(1.0) - dead) / (1.0 - dead)).clamp(0.0, 1.0);
    [v[0] / mag * scaled, v[1] / mag * scaled]
}

/// A trigger (`0..=255`) as `0..=1`, zero up to `threshold`, rescaled above it.
pub fn trigger_value(v: u8, threshold: f32) -> f32 {
    let threshold = threshold.clamp(0.0, 0.99);
    let x = f32::from(v) / 255.0;
    if x <= threshold {
        0.0
    } else {
        ((x - threshold) / (1.0 - threshold)).min(1.0)
    }
}

/// Events describing the change from `prev` to `next` for pad `pad`, in a fixed order:
/// `Connected`, then button changes in [`GamepadButton::ALL`] order, then `Disconnected`.
pub fn diff_events(pad: usize, prev: &GamepadState, next: &GamepadState) -> Vec<GamepadEvent> {
    let mut events = Vec::new();
    if next.connected && !prev.connected {
        events.push(GamepadEvent::Connected { pad });
    }
    let before = if prev.connected { prev.buttons } else { 0 };
    let after = if next.connected { next.buttons } else { 0 };
    for button in GamepadButton::ALL {
        let bit = button.bit();
        match (before & bit != 0, after & bit != 0) {
            (false, true) => events.push(GamepadEvent::ButtonDown { pad, button }),
            (true, false) => events.push(GamepadEvent::ButtonUp { pad, button }),
            _ => {}
        }
    }
    if prev.connected && !next.connected {
        events.push(GamepadEvent::Disconnected { pad });
    }
    events
}

/// XInput `wButtons` bit for each button.
#[cfg_attr(not(windows), allow(dead_code))]
const XINPUT_BUTTONS: [(u16, GamepadButton); 14] = [
    (0x0001, GamepadButton::DpadUp),
    (0x0002, GamepadButton::DpadDown),
    (0x0004, GamepadButton::DpadLeft),
    (0x0008, GamepadButton::DpadRight),
    (0x0010, GamepadButton::Start),
    (0x0020, GamepadButton::Back),
    (0x0040, GamepadButton::LeftStick),
    (0x0080, GamepadButton::RightStick),
    (0x0100, GamepadButton::LeftBumper),
    (0x0200, GamepadButton::RightBumper),
    (0x1000, GamepadButton::South),
    (0x2000, GamepadButton::East),
    (0x4000, GamepadButton::West),
    (0x8000, GamepadButton::North),
];

/// XInput `wButtons` → [`GamepadButton::bit`] mask.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn buttons_from_xinput(w: u16) -> u32 {
    XINPUT_BUTTONS.iter().filter(|(x, _)| w & x != 0).fold(0, |acc, (_, b)| acc | b.bit())
}

/// The pads of this machine. Poll once per frame; not tied to a window or thread.
pub struct Gamepads {
    backend: backend::Backend,
    states: [GamepadState; MAX_GAMEPADS],
    dead_zones: DeadZones,
    /// When to re-query an empty slot: querying a disconnected XInput slot is slow, so empty slots
    /// are probed at most once per [`Gamepads::PROBE_INTERVAL`].
    next_probe: [Option<std::time::Instant>; MAX_GAMEPADS],
}

impl Gamepads {
    /// How often empty slots are checked for a newly connected pad.
    pub const PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000);

    /// Loads the platform backend. Errors (with the OS code where there is one) when the platform
    /// has no backend or no XInput DLL is installed; pads are optional, so callers usually log
    /// the error and carry on without them.
    pub fn new() -> Result<Gamepads, PlatformError> {
        Ok(Gamepads {
            backend: backend::Backend::load()?,
            states: [GamepadState::default(); MAX_GAMEPADS],
            dead_zones: DeadZones::default(),
            next_probe: [None; MAX_GAMEPADS],
        })
    }

    /// The library in use (e.g. `"xinput1_4.dll"`).
    pub fn backend_name(&self) -> &str {
        self.backend.name()
    }

    pub fn dead_zones(&self) -> DeadZones {
        self.dead_zones
    }

    /// Takes effect at the next [`Gamepads::poll`].
    pub fn set_dead_zones(&mut self, dead_zones: DeadZones) {
        self.dead_zones = dead_zones;
    }

    /// Reads every pad and returns what changed since the last poll.
    pub fn poll(&mut self) -> Vec<GamepadEvent> {
        let now = std::time::Instant::now();
        let mut events = Vec::new();
        for pad in 0..MAX_GAMEPADS {
            let prev = self.states[pad];
            if !prev.connected && self.next_probe[pad].is_some_and(|at| now < at) {
                continue;
            }
            let next = match self.backend.read(pad) {
                Some(raw) => {
                    self.next_probe[pad] = None;
                    GamepadState::from_raw(raw, &self.dead_zones)
                }
                None => {
                    self.next_probe[pad] = Some(now + Self::PROBE_INTERVAL);
                    GamepadState::default()
                }
            };
            events.extend(diff_events(pad, &prev, &next));
            self.states[pad] = next;
        }
        events
    }

    /// State of pad `pad` as of the last poll (disconnected/default for out-of-range indices).
    pub fn state(&self, pad: usize) -> GamepadState {
        self.states.get(pad).copied().unwrap_or_default()
    }

    /// All pads as of the last poll.
    pub fn states(&self) -> &[GamepadState; MAX_GAMEPADS] {
        &self.states
    }

    /// Rumble: `low` drives the low-frequency (left, heavy) motor, `high` the high-frequency one,
    /// each `0..=1`. Pass zeros to stop. Errors carry the OS code (e.g. 1167: not connected).
    pub fn set_rumble(&mut self, pad: usize, low: f32, high: f32) -> Result<(), PlatformError> {
        if pad >= MAX_GAMEPADS {
            return Err(PlatformError { message: format!("gamepad index {pad} out of range 0..{MAX_GAMEPADS}"), os_code: None });
        }
        let speed = |v: f32| (v.clamp(0.0, 1.0) * 65535.0) as u16;
        self.backend.rumble(pad, speed(low), speed(high))
    }
}

#[cfg(windows)]
mod backend {
    use std::ffi::c_void;

    use specula_sys::dynlib::Library;

    use super::{buttons_from_xinput, RawGamepad};
    use crate::PlatformError;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputGamepad {
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        thumb_lx: i16,
        thumb_ly: i16,
        thumb_rx: i16,
        thumb_ry: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputState {
        packet_number: u32,
        gamepad: XInputGamepad,
    }

    #[repr(C)]
    struct XInputVibration {
        left_motor_speed: u16,
        right_motor_speed: u16,
    }

    type GetStateFn = unsafe extern "system" fn(u32, *mut XInputState) -> u32;
    type SetStateFn = unsafe extern "system" fn(u32, *mut XInputVibration) -> u32;

    const ERROR_SUCCESS: u32 = 0;
    const LIBRARIES: [&str; 2] = ["xinput1_4.dll", "xinput9_1_0.dll"];

    pub(super) struct Backend {
        get_state: GetStateFn,
        set_state: Option<SetStateFn>,
        /// Keeps the code mapped while the function pointers above are in use.
        library: Library,
    }

    impl Backend {
        pub(super) fn load() -> Result<Backend, PlatformError> {
            let mut failures = Vec::new();
            for name in LIBRARIES {
                let library = match Library::open(name) {
                    Ok(library) => library,
                    Err(err) => {
                        failures.push(err);
                        continue;
                    }
                };
                let Some(get) = library.symbol("XInputGetState") else {
                    failures.push(specula_sys::dynlib::LoadError {
                        library: name.into(),
                        message: "does not export XInputGetState".into(),
                        os_code: None,
                    });
                    continue;
                };
                // SAFETY: `get` is the non-null address of `XInputGetState`, C prototype
                // `DWORD WINAPI (DWORD dwUserIndex, XINPUT_STATE *pState)`; `library` is stored next
                // to the pointer, so the code stays mapped for as long as it can be called.
                let get_state = unsafe { std::mem::transmute::<*mut c_void, GetStateFn>(get) };
                // SAFETY: as above, `XInputSetState`: `DWORD WINAPI (DWORD, XINPUT_VIBRATION *)`.
                let set_state =
                    library.symbol("XInputSetState").map(|f| unsafe { std::mem::transmute::<*mut c_void, SetStateFn>(f) });
                return Ok(Backend { get_state, set_state, library });
            }
            let os_code = failures.first().and_then(|f| f.os_code);
            let detail: Vec<String> = failures.iter().map(ToString::to_string).collect();
            Err(PlatformError { message: format!("no XInput library available ({})", detail.join("; ")), os_code })
        }

        pub(super) fn name(&self) -> &str {
            self.library.name()
        }

        /// Raw state of one user index, `None` when nothing is connected there.
        pub(super) fn read(&self, pad: usize) -> Option<RawGamepad> {
            let mut state = XInputState::default();
            // SAFETY: `get_state` is XInputGetState (see `load`); `state` is a writable
            // XINPUT_STATE; user indices 0..4 are valid.
            let result = unsafe { (self.get_state)(pad as u32, &mut state) };
            if result != ERROR_SUCCESS {
                return None;
            }
            let g = state.gamepad;
            Some(RawGamepad {
                buttons: buttons_from_xinput(g.buttons),
                left_trigger: g.left_trigger,
                right_trigger: g.right_trigger,
                left_x: g.thumb_lx,
                left_y: g.thumb_ly,
                right_x: g.thumb_rx,
                right_y: g.thumb_ry,
            })
        }

        pub(super) fn rumble(&self, pad: usize, low: u16, high: u16) -> Result<(), PlatformError> {
            let Some(set_state) = self.set_state else {
                return Err(PlatformError { message: format!("{} has no XInputSetState", self.name()), os_code: None });
            };
            let mut vibration = XInputVibration { left_motor_speed: low, right_motor_speed: high };
            // SAFETY: `set_state` is XInputSetState (see `load`); `vibration` is a valid
            // XINPUT_VIBRATION for the duration of the call.
            match unsafe { set_state(pad as u32, &mut vibration) } {
                ERROR_SUCCESS => Ok(()),
                code => Err(PlatformError { message: format!("XInputSetState(pad {pad}) failed"), os_code: Some(code) }),
            }
        }
    }
}

#[cfg(not(windows))]
mod backend {
    use super::RawGamepad;
    use crate::PlatformError;

    pub(super) enum Backend {}

    impl Backend {
        pub(super) fn load() -> Result<Backend, PlatformError> {
            Err(PlatformError {
                message: format!(
                    "specula-platform: gamepads are unsupported on this platform ({}); only Windows (XInput) is implemented",
                    std::env::consts::OS
                ),
                os_code: None,
            })
        }

        pub(super) fn name(&self) -> &str {
            match *self {}
        }

        pub(super) fn read(&self, _pad: usize) -> Option<RawGamepad> {
            match *self {}
        }

        pub(super) fn rumble(&self, _pad: usize, _low: u16, _high: u16) -> Result<(), PlatformError> {
            match *self {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn axes_normalize_symmetrically() {
        assert_eq!(normalize_axis(0), 0.0);
        assert_eq!(normalize_axis(32767), 1.0);
        assert_eq!(normalize_axis(-32767), -1.0);
        assert_eq!(normalize_axis(-32768), -1.0, "the extra negative step is clamped");
    }

    #[test]
    fn radial_dead_zone_rescales_and_keeps_direction() {
        let dz = 0.25;
        assert_eq!(radial_dead_zone([0.0, 0.0], dz), [0.0, 0.0]);
        assert_eq!(radial_dead_zone([0.2, 0.1], dz), [0.0, 0.0], "inside the circle");
        assert_eq!(radial_dead_zone([0.25, 0.0], dz), [0.0, 0.0], "on the edge reads zero");
        // Just outside the edge: small output, no jump.
        let v = radial_dead_zone([0.26, 0.0], dz);
        assert!(v[0] > 0.0 && v[0] < 0.02, "{v:?}");
        // Full deflection reaches 1.
        assert_eq!(radial_dead_zone([1.0, 0.0], dz), [1.0, 0.0]);
        assert_eq!(radial_dead_zone([0.0, -1.0], dz), [0.0, -1.0]);
        // Halfway between edge and full: 0.5, along the input direction.
        let v = radial_dead_zone([0.625 * 0.6, 0.625 * 0.8], dz);
        assert!(close(v[0], 0.3) && close(v[1], 0.4), "{v:?}");
        // Square-gate corner (magnitude √2) clamps to the unit circle.
        let v = radial_dead_zone([1.0, 1.0], dz);
        assert!(close(v[0] * v[0] + v[1] * v[1], 1.0) && close(v[0], v[1]), "{v:?}");
        // Radial, not per-axis: a diagonal whose components are each below the dead zone but whose
        // length is above it is not zeroed.
        let v = radial_dead_zone([0.2, 0.2], dz);
        assert!(v[0] > 0.0 && v[1] > 0.0, "{v:?}");
        // NaN never leaks out.
        assert_eq!(radial_dead_zone([f32::NAN, 0.0], dz), [0.0, 0.0]);
    }

    #[test]
    fn triggers_use_a_threshold() {
        let t = 30.0 / 255.0;
        assert_eq!(trigger_value(0, t), 0.0);
        assert_eq!(trigger_value(30, t), 0.0);
        assert!(trigger_value(31, t) > 0.0);
        assert_eq!(trigger_value(255, t), 1.0);
        assert!(close(trigger_value(128, 0.0), 128.0 / 255.0));
    }

    #[test]
    fn raw_state_converts_with_default_dead_zones() {
        let raw = RawGamepad {
            buttons: GamepadButton::South.bit() | GamepadButton::DpadLeft.bit(),
            left_trigger: 255,
            right_trigger: 10,
            left_x: 32767,
            left_y: 0,
            right_x: 3000, // inside the right stick's dead zone
            right_y: -3000,
        };
        let s = GamepadState::from_raw(raw, &DeadZones::default());
        assert!(s.connected && s.held(GamepadButton::South) && s.held(GamepadButton::DpadLeft));
        assert!(!s.held(GamepadButton::North));
        assert_eq!(s.left_stick, [1.0, 0.0]);
        assert_eq!(s.right_stick, [0.0, 0.0]);
        assert_eq!((s.left_trigger, s.right_trigger), (1.0, 0.0));
        assert_eq!(s.raw, raw);
    }

    #[test]
    fn xinput_buttons_map_to_positions() {
        assert_eq!(buttons_from_xinput(0x1000), GamepadButton::South.bit());
        assert_eq!(buttons_from_xinput(0x8000 | 0x0200), GamepadButton::North.bit() | GamepadButton::RightBumper.bit());
        assert_eq!(buttons_from_xinput(0x0400 | 0x0800), 0, "reserved bits are ignored");
        assert_eq!(buttons_from_xinput(0xF3FF).count_ones(), 14);
        let mut seen = 0u32;
        for (_, b) in XINPUT_BUTTONS {
            assert_eq!(seen & b.bit(), 0, "{b:?} mapped twice");
            seen |= b.bit();
        }
    }

    #[test]
    fn diff_reports_connections_and_edges_in_order() {
        let off = GamepadState::default();
        let a = GamepadState { connected: true, buttons: GamepadButton::South.bit(), ..Default::default() };
        let b = GamepadState { connected: true, buttons: GamepadButton::East.bit(), ..Default::default() };
        assert_eq!(
            diff_events(2, &off, &a),
            vec![GamepadEvent::Connected { pad: 2 }, GamepadEvent::ButtonDown { pad: 2, button: GamepadButton::South }]
        );
        assert_eq!(
            diff_events(0, &a, &b),
            vec![
                GamepadEvent::ButtonUp { pad: 0, button: GamepadButton::South },
                GamepadEvent::ButtonDown { pad: 0, button: GamepadButton::East },
            ]
        );
        assert_eq!(
            diff_events(1, &b, &off),
            vec![GamepadEvent::ButtonUp { pad: 1, button: GamepadButton::East }, GamepadEvent::Disconnected { pad: 1 }]
        );
        assert!(diff_events(0, &a, &a).is_empty());
        assert!(diff_events(0, &off, &off).is_empty());
    }

    #[test]
    fn button_names_and_bits_are_distinct() {
        let mut bits = 0;
        for (i, b) in GamepadButton::ALL.iter().enumerate() {
            assert_eq!(b.bit(), 1 << i);
            bits |= b.bit();
            assert!(!b.name().is_empty());
        }
        assert_eq!(bits.count_ones(), 14);
    }

    /// Live: loads XInput and polls. Skips (prints why) without a backend or a pad.
    #[test]
    fn live_pads_poll_cleanly() {
        let mut pads = match Gamepads::new() {
            Ok(pads) => pads,
            Err(err) => {
                eprintln!("skipping: no gamepad backend here: {err}");
                return;
            }
        };
        let _ = pads.poll();
        let connected: Vec<usize> = (0..MAX_GAMEPADS).filter(|&i| pads.state(i).connected).collect();
        if connected.is_empty() {
            eprintln!("skipping live pad checks: {} loaded, but no gamepad is connected", pads.backend_name());
        }
        for pad in connected {
            let s = pads.state(pad);
            for stick in [s.left_stick, s.right_stick] {
                assert!(stick[0] * stick[0] + stick[1] * stick[1] <= 1.0 + 1e-5, "{s:?}");
            }
            assert!((0.0..=1.0).contains(&s.left_trigger) && (0.0..=1.0).contains(&s.right_trigger));
            pads.set_rumble(pad, 0.0, 0.0).expect("stopping rumble on a connected pad");
        }
        assert!(pads.set_rumble(MAX_GAMEPADS, 0.0, 0.0).is_err());
        assert_eq!(pads.state(99), GamepadState::default());
        // A second poll right away skips empty slots (probe interval) and reports nothing new
        // unless the user is pressing buttons.
        let _ = pads.poll();
    }
}
