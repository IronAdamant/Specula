//! Input sources: the physical things a binding listens to, their names in binding files and
//! their prompt glyphs.
//!
//! Names are `Device.Name`:
//!
//! ```text
//! Key.Space  Key.W  Key.Escape  Key.Ctrl          keys (names from specula_platform::keys)
//! Mouse.Left  Mouse.Right  Mouse.Middle           mouse buttons
//! Mouse.WheelUp  Mouse.WheelDown                  one-frame pulses per wheel movement
//! Mouse.Wheel                                     1D, notches this frame (+ = away from the user)
//! Mouse.Motion                                    2D, pixels moved this frame (+y down)
//! Pad.South  Pad.East  Pad.West  Pad.North        face buttons by position (aliases A B X Y,
//!                                                 Cross Circle Square Triangle)
//! Pad.LeftBumper  Pad.RightBumper  (LB RB L1 R1)  Pad.Back  Pad.Start  (View Menu Select Options)
//! Pad.LeftStickClick  Pad.RightStickClick (LS RS L3 R3)  Pad.DpadUp/Down/Left/Right
//! Pad.LeftStick  Pad.RightStick  Pad.Dpad         2D, -1..1, +y up
//! Pad.LeftX  Pad.LeftY  Pad.RightX  Pad.RightY    1D stick components
//! Pad.LeftTrigger  Pad.RightTrigger (LT RT L2 R2) 1D, 0..1
//! Touch.Tap  Touch.TwoFingerTap  Touch.LongPress  gesture pulses
//! Touch.Drag  Touch.TwoFingerDrag                 2D, pixels moved this frame
//! Touch.Pinch                                     1D, ln(distance ratio) this frame (+ = spread)
//! Touch.Rotate                                    1D, radians this frame (+ = clockwise on screen)
//! ```
//!
//! Pen contacts count as touch for gestures.

use specula_platform::{keys, GamepadButton, MouseButton};

/// Left or right stick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stick {
    Left,
    Right,
}

/// One analog gamepad axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

/// A physical input a binding can listen to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A key code (Win32 virtual-key numbering, see `specula_platform::keys`).
    Key(u32),
    Mouse(MouseButton),
    WheelUp,
    WheelDown,
    Wheel,
    MouseMotion,
    Pad(GamepadButton),
    PadAxis(PadAxis),
    Stick(Stick),
    Dpad,
    Tap,
    TwoFingerTap,
    LongPress,
    Drag,
    TwoFingerDrag,
    Pinch,
    Rotate,
}

/// What shape of value a source produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Signal {
    /// On/off.
    Button,
    /// One number.
    Axis,
    /// Two numbers.
    Axis2,
}

/// The device family a source belongs to: picks which bindings to show as prompts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceFamily {
    KeyboardMouse,
    Gamepad,
    Touch,
}

impl DeviceFamily {
    pub fn name(self) -> &'static str {
        match self {
            DeviceFamily::KeyboardMouse => "keyboard_mouse",
            DeviceFamily::Gamepad => "gamepad",
            DeviceFamily::Touch => "touch",
        }
    }

    pub fn from_name(name: &str) -> Option<DeviceFamily> {
        [DeviceFamily::KeyboardMouse, DeviceFamily::Gamepad, DeviceFamily::Touch].into_iter().find(|f| f.name() == name)
    }
}

/// Which gamepad's button labels to show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PadStyle {
    #[default]
    Xbox,
    PlayStation,
    SteamDeck,
}

impl PadStyle {
    pub fn name(self) -> &'static str {
        match self {
            PadStyle::Xbox => "xbox",
            PadStyle::PlayStation => "playstation",
            PadStyle::SteamDeck => "steam_deck",
        }
    }

    pub fn from_name(name: &str) -> Option<PadStyle> {
        [PadStyle::Xbox, PadStyle::PlayStation, PadStyle::SteamDeck].into_iter().find(|s| s.name() == name)
    }
}

const PAD_BUTTONS: [(&str, GamepadButton); 14] = [
    ("South", GamepadButton::South),
    ("East", GamepadButton::East),
    ("West", GamepadButton::West),
    ("North", GamepadButton::North),
    ("LeftBumper", GamepadButton::LeftBumper),
    ("RightBumper", GamepadButton::RightBumper),
    ("Back", GamepadButton::Back),
    ("Start", GamepadButton::Start),
    ("LeftStickClick", GamepadButton::LeftStick),
    ("RightStickClick", GamepadButton::RightStick),
    ("DpadUp", GamepadButton::DpadUp),
    ("DpadDown", GamepadButton::DpadDown),
    ("DpadLeft", GamepadButton::DpadLeft),
    ("DpadRight", GamepadButton::DpadRight),
];

const PAD_BUTTON_ALIASES: [(&str, GamepadButton); 21] = [
    ("A", GamepadButton::South),
    ("B", GamepadButton::East),
    ("X", GamepadButton::West),
    ("Y", GamepadButton::North),
    ("Cross", GamepadButton::South),
    ("Circle", GamepadButton::East),
    ("Square", GamepadButton::West),
    ("Triangle", GamepadButton::North),
    ("LB", GamepadButton::LeftBumper),
    ("RB", GamepadButton::RightBumper),
    ("L1", GamepadButton::LeftBumper),
    ("R1", GamepadButton::RightBumper),
    ("View", GamepadButton::Back),
    ("Select", GamepadButton::Back),
    ("Menu", GamepadButton::Start),
    ("Options", GamepadButton::Start),
    ("LS", GamepadButton::LeftStick),
    ("RS", GamepadButton::RightStick),
    ("L3", GamepadButton::LeftStick),
    ("R3", GamepadButton::RightStick),
    ("Create", GamepadButton::Back),
];

const PAD_AXES: [(&str, Source); 13] = [
    ("LeftStick", Source::Stick(Stick::Left)),
    ("RightStick", Source::Stick(Stick::Right)),
    ("Dpad", Source::Dpad),
    ("LeftX", Source::PadAxis(PadAxis::LeftX)),
    ("LeftY", Source::PadAxis(PadAxis::LeftY)),
    ("RightX", Source::PadAxis(PadAxis::RightX)),
    ("RightY", Source::PadAxis(PadAxis::RightY)),
    ("LeftTrigger", Source::PadAxis(PadAxis::LeftTrigger)),
    ("RightTrigger", Source::PadAxis(PadAxis::RightTrigger)),
    ("LT", Source::PadAxis(PadAxis::LeftTrigger)),
    ("RT", Source::PadAxis(PadAxis::RightTrigger)),
    ("L2", Source::PadAxis(PadAxis::LeftTrigger)),
    ("R2", Source::PadAxis(PadAxis::RightTrigger)),
];

const MOUSE: [(&str, Source); 7] = [
    ("Left", Source::Mouse(MouseButton::Left)),
    ("Right", Source::Mouse(MouseButton::Right)),
    ("Middle", Source::Mouse(MouseButton::Middle)),
    ("WheelUp", Source::WheelUp),
    ("WheelDown", Source::WheelDown),
    ("Wheel", Source::Wheel),
    ("Motion", Source::MouseMotion),
];

const TOUCH: [(&str, Source); 7] = [
    ("Tap", Source::Tap),
    ("TwoFingerTap", Source::TwoFingerTap),
    ("LongPress", Source::LongPress),
    ("Drag", Source::Drag),
    ("TwoFingerDrag", Source::TwoFingerDrag),
    ("Pinch", Source::Pinch),
    ("Rotate", Source::Rotate),
];

fn find<T: Copy>(table: &[(&str, T)], name: &str) -> Option<T> {
    table.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, v)| v)
}

fn name_of<T: Copy + PartialEq>(table: &[(&'static str, T)], v: T) -> &'static str {
    table.iter().find(|&&(_, t)| t == v).map_or("?", |&(n, _)| n)
}

impl Source {
    /// Parses `Device.Name` (case-insensitive; aliases accepted). `None` for unknown names.
    pub fn parse(text: &str) -> Option<Source> {
        let (device, name) = text.trim().split_once('.')?;
        match device.to_ascii_lowercase().as_str() {
            "key" => keys::from_name(name)
                .or_else(|| name.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()).filter(|&c| c < 256))
                .map(Source::Key),
            "mouse" => find(&MOUSE, name),
            "pad" => find(&PAD_BUTTONS, name)
                .or_else(|| find(&PAD_BUTTON_ALIASES, name))
                .map(Source::Pad)
                .or_else(|| find(&PAD_AXES, name)),
            "touch" => find(&TOUCH, name),
            _ => None,
        }
    }

    /// Canonical name (round-trips through [`Source::parse`]).
    pub fn name(self) -> String {
        match self {
            Source::Key(code) => match keys::name(code) {
                Some(n) => format!("Key.{n}"),
                None => format!("Key.#{code}"),
            },
            Source::Mouse(_) | Source::WheelUp | Source::WheelDown | Source::Wheel | Source::MouseMotion => {
                format!("Mouse.{}", name_of(&MOUSE, self))
            }
            Source::Pad(b) => format!("Pad.{}", name_of(&PAD_BUTTONS, b)),
            Source::PadAxis(_) | Source::Stick(_) | Source::Dpad => format!("Pad.{}", name_of(&PAD_AXES, self)),
            _ => format!("Touch.{}", name_of(&TOUCH, self)),
        }
    }

    pub fn signal(self) -> Signal {
        match self {
            Source::Key(_)
            | Source::Mouse(_)
            | Source::WheelUp
            | Source::WheelDown
            | Source::Pad(_)
            | Source::Tap
            | Source::TwoFingerTap
            | Source::LongPress => Signal::Button,
            Source::Wheel | Source::PadAxis(_) | Source::Pinch | Source::Rotate => Signal::Axis,
            Source::MouseMotion | Source::Stick(_) | Source::Dpad | Source::Drag | Source::TwoFingerDrag => {
                Signal::Axis2
            }
        }
    }

    /// Delta sources report an amount that happened this frame (motion, wheel, gesture pulses);
    /// the others report a level that lasts (held key, stick deflection), which axis actions turn
    /// into a per-frame amount by multiplying with the frame time.
    pub fn is_delta(self) -> bool {
        matches!(
            self,
            Source::WheelUp
                | Source::WheelDown
                | Source::Wheel
                | Source::MouseMotion
                | Source::Tap
                | Source::TwoFingerTap
                | Source::LongPress
                | Source::Drag
                | Source::TwoFingerDrag
                | Source::Pinch
                | Source::Rotate
        )
    }

    /// Buttons that stay down (keys, mouse and pad buttons): the only valid modifiers.
    pub fn is_holdable_button(self) -> bool {
        matches!(self, Source::Key(_) | Source::Mouse(_) | Source::Pad(_))
    }

    pub fn family(self) -> DeviceFamily {
        match self {
            Source::Key(_)
            | Source::Mouse(_)
            | Source::WheelUp
            | Source::WheelDown
            | Source::Wheel
            | Source::MouseMotion => DeviceFamily::KeyboardMouse,
            Source::Pad(_) | Source::PadAxis(_) | Source::Stick(_) | Source::Dpad => DeviceFamily::Gamepad,
            _ => DeviceFamily::Touch,
        }
    }

    /// Prompt glyph id, ASCII (`"Space"`, `"LMB"`, `"A"`, `"RB"`, `"Cross"`, `"Tap"`). A UI maps
    /// these to icons; they read fine as text too.
    pub fn glyph(self, style: PadStyle) -> String {
        let s: &str = match self {
            Source::Key(code) => return keys::label(code).map_or_else(|| format!("#{code}"), str::to_owned),
            Source::Mouse(MouseButton::Left) => "LMB",
            Source::Mouse(MouseButton::Right) => "RMB",
            Source::Mouse(MouseButton::Middle) => "MMB",
            Source::WheelUp => "Wheel Up",
            Source::WheelDown => "Wheel Down",
            Source::Wheel => "Wheel",
            Source::MouseMotion => "Mouse",
            Source::Pad(b) => pad_button_glyph(b, style),
            Source::PadAxis(PadAxis::LeftX | PadAxis::LeftY) | Source::Stick(Stick::Left) => "L Stick",
            Source::PadAxis(PadAxis::RightX | PadAxis::RightY) | Source::Stick(Stick::Right) => "R Stick",
            Source::PadAxis(PadAxis::LeftTrigger) => match style {
                PadStyle::Xbox => "LT",
                _ => "L2",
            },
            Source::PadAxis(PadAxis::RightTrigger) => match style {
                PadStyle::Xbox => "RT",
                _ => "R2",
            },
            Source::Dpad => "D-pad",
            Source::Tap => "Tap",
            Source::TwoFingerTap => "Two-finger tap",
            Source::LongPress => "Long press",
            Source::Drag => "Drag",
            Source::TwoFingerDrag => "Two-finger drag",
            Source::Pinch => "Pinch",
            Source::Rotate => "Twist",
        };
        s.to_owned()
    }
}

fn pad_button_glyph(b: GamepadButton, style: PadStyle) -> &'static str {
    use GamepadButton as B;
    match (b, style) {
        (B::South, PadStyle::PlayStation) => "Cross",
        (B::East, PadStyle::PlayStation) => "Circle",
        (B::West, PadStyle::PlayStation) => "Square",
        (B::North, PadStyle::PlayStation) => "Triangle",
        (B::South, _) => "A",
        (B::East, _) => "B",
        (B::West, _) => "X",
        (B::North, _) => "Y",
        (B::LeftBumper, PadStyle::Xbox) => "LB",
        (B::RightBumper, PadStyle::Xbox) => "RB",
        (B::LeftBumper, _) => "L1",
        (B::RightBumper, _) => "R1",
        (B::Back, PadStyle::PlayStation) => "Create",
        (B::Start, PadStyle::PlayStation) => "Options",
        (B::Back, _) => "View",
        (B::Start, _) => "Menu",
        (B::LeftStick, PadStyle::Xbox) => "LS",
        (B::RightStick, PadStyle::Xbox) => "RS",
        (B::LeftStick, _) => "L3",
        (B::RightStick, _) => "R3",
        (B::DpadUp, _) => "D-pad Up",
        (B::DpadDown, _) => "D-pad Down",
        (B::DpadLeft, _) => "D-pad Left",
        (B::DpadRight, _) => "D-pad Right",
    }
}

/// Every source with a canonical name, for exhaustive tests and tooling (e.g. listing what can
/// be bound). Keys are the named ones from `specula_platform::keys`.
pub fn all_sources() -> Vec<Source> {
    let mut out: Vec<Source> = (0..=255u32).filter(|&c| keys::name(c).is_some()).map(Source::Key).collect();
    out.extend(MOUSE.iter().map(|&(_, s)| s));
    out.extend(PAD_BUTTONS.iter().map(|&(_, b)| Source::Pad(b)));
    out.extend(PAD_AXES[..9].iter().map(|&(_, s)| s));
    out.extend(TOUCH.iter().map(|&(_, s)| s));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_round_trips_by_name() {
        let all = all_sources();
        assert!(all.len() > 140, "{}", all.len());
        for s in all {
            let name = s.name();
            assert_eq!(Source::parse(&name), Some(s), "{name}");
            assert_eq!(Source::parse(&name.to_ascii_lowercase()), Some(s), "{name} lowercase");
            for style in [PadStyle::Xbox, PadStyle::PlayStation, PadStyle::SteamDeck] {
                let g = s.glyph(style);
                assert!(!g.is_empty() && g.is_ascii(), "{name}: {g:?}");
            }
        }
    }

    #[test]
    fn aliases_parse() {
        assert_eq!(Source::parse("Pad.A"), Some(Source::Pad(GamepadButton::South)));
        assert_eq!(Source::parse("Pad.Cross"), Some(Source::Pad(GamepadButton::South)));
        assert_eq!(Source::parse("Pad.RB"), Some(Source::Pad(GamepadButton::RightBumper)));
        assert_eq!(Source::parse("Pad.LT"), Some(Source::PadAxis(PadAxis::LeftTrigger)));
        assert_eq!(Source::parse("Pad.LS"), Some(Source::Pad(GamepadButton::LeftStick)));
        assert_eq!(Source::parse("Pad.LeftStick"), Some(Source::Stick(Stick::Left)));
        assert_eq!(Source::parse("Key.Esc"), Some(Source::Key(keys::ESCAPE)));
        assert_eq!(Source::parse(" Mouse.Left "), Some(Source::Mouse(MouseButton::Left)));
        for bad in ["Space", "Key.", "Key.Spaec", "Pad.Z", "Joystick.A", "Touch.Swipe", ""] {
            assert_eq!(Source::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn glyphs_follow_pad_style() {
        let a = Source::Pad(GamepadButton::South);
        assert_eq!(a.glyph(PadStyle::Xbox), "A");
        assert_eq!(a.glyph(PadStyle::PlayStation), "Cross");
        assert_eq!(a.glyph(PadStyle::SteamDeck), "A");
        let rb = Source::Pad(GamepadButton::RightBumper);
        assert_eq!((rb.glyph(PadStyle::Xbox), rb.glyph(PadStyle::SteamDeck)), ("RB".into(), "R1".into()));
        assert_eq!(Source::Mouse(MouseButton::Left).glyph(PadStyle::Xbox), "LMB");
        assert_eq!(Source::Key(keys::SPACE).glyph(PadStyle::Xbox), "Space");
        assert_eq!(Source::Tap.glyph(PadStyle::Xbox), "Tap");
    }

    #[test]
    fn classification() {
        assert_eq!(Source::Key(keys::SPACE).signal(), Signal::Button);
        assert_eq!(Source::Wheel.signal(), Signal::Axis);
        assert_eq!(Source::Stick(Stick::Left).signal(), Signal::Axis2);
        assert!(Source::MouseMotion.is_delta() && !Source::Stick(Stick::Right).is_delta());
        assert!(Source::Key(keys::CTRL).is_holdable_button() && !Source::WheelUp.is_holdable_button());
        assert_eq!(Source::Pinch.family(), DeviceFamily::Touch);
        assert_eq!(Source::Dpad.family(), DeviceFamily::Gamepad);
        assert_eq!(Source::WheelDown.family(), DeviceFamily::KeyboardMouse);
    }
}
