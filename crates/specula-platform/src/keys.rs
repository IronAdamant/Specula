//! Key names for the codes in [`Event::KeyDown`](crate::Event::KeyDown) / `KeyUp`.
//!
//! Key codes use the Win32 virtual-key numbering on every platform: future backends (Linux,
//! Android) translate their own scancodes into this space, so binding files and recordings stay
//! portable. Names are what binding files use (`"Space"`, `"W"`, `"Escape"`); lookups are
//! case-insensitive and accept a few aliases (`"Esc"`, `"Return"`, `"Control"`).
//!
//! Win32 reports the generic `Shift` / `Ctrl` / `Alt` codes in key messages, never the left/right
//! variants, so bind the generic names; `LShift` etc. exist for completeness.

/// `(canonical name, code)`. The first entry for a code is its canonical name.
const NAMES: &[(&str, u32)] = &[
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Shift", 0x10),
    ("Ctrl", 0x11),
    ("Alt", 0x12),
    ("Pause", 0x13),
    ("CapsLock", 0x14),
    ("Escape", 0x1B),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("PrintScreen", 0x2C),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("0", 0x30),
    ("1", 0x31),
    ("2", 0x32),
    ("3", 0x33),
    ("4", 0x34),
    ("5", 0x35),
    ("6", 0x36),
    ("7", 0x37),
    ("8", 0x38),
    ("9", 0x39),
    ("A", 0x41),
    ("B", 0x42),
    ("C", 0x43),
    ("D", 0x44),
    ("E", 0x45),
    ("F", 0x46),
    ("G", 0x47),
    ("H", 0x48),
    ("I", 0x49),
    ("J", 0x4A),
    ("K", 0x4B),
    ("L", 0x4C),
    ("M", 0x4D),
    ("N", 0x4E),
    ("O", 0x4F),
    ("P", 0x50),
    ("Q", 0x51),
    ("R", 0x52),
    ("S", 0x53),
    ("T", 0x54),
    ("U", 0x55),
    ("V", 0x56),
    ("W", 0x57),
    ("X", 0x58),
    ("Y", 0x59),
    ("Z", 0x5A),
    ("LWin", 0x5B),
    ("RWin", 0x5C),
    ("Apps", 0x5D),
    ("Numpad0", 0x60),
    ("Numpad1", 0x61),
    ("Numpad2", 0x62),
    ("Numpad3", 0x63),
    ("Numpad4", 0x64),
    ("Numpad5", 0x65),
    ("Numpad6", 0x66),
    ("Numpad7", 0x67),
    ("Numpad8", 0x68),
    ("Numpad9", 0x69),
    ("NumpadMultiply", 0x6A),
    ("NumpadAdd", 0x6B),
    ("NumpadSubtract", 0x6D),
    ("NumpadDecimal", 0x6E),
    ("NumpadDivide", 0x6F),
    ("F1", 0x70),
    ("F2", 0x71),
    ("F3", 0x72),
    ("F4", 0x73),
    ("F5", 0x74),
    ("F6", 0x75),
    ("F7", 0x76),
    ("F8", 0x77),
    ("F9", 0x78),
    ("F10", 0x79),
    ("F11", 0x7A),
    ("F12", 0x7B),
    ("F13", 0x7C),
    ("F14", 0x7D),
    ("F15", 0x7E),
    ("F16", 0x7F),
    ("F17", 0x80),
    ("F18", 0x81),
    ("F19", 0x82),
    ("F20", 0x83),
    ("F21", 0x84),
    ("F22", 0x85),
    ("F23", 0x86),
    ("F24", 0x87),
    ("NumLock", 0x90),
    ("ScrollLock", 0x91),
    ("LShift", 0xA0),
    ("RShift", 0xA1),
    ("LCtrl", 0xA2),
    ("RCtrl", 0xA3),
    ("LAlt", 0xA4),
    ("RAlt", 0xA5),
    ("Semicolon", 0xBA),
    ("Equals", 0xBB),
    ("Comma", 0xBC),
    ("Minus", 0xBD),
    ("Period", 0xBE),
    ("Slash", 0xBF),
    ("Backquote", 0xC0),
    ("LBracket", 0xDB),
    ("Backslash", 0xDC),
    ("RBracket", 0xDD),
    ("Quote", 0xDE),
];

const ALIASES: &[(&str, u32)] = &[
    ("Esc", 0x1B),
    ("Return", 0x0D),
    ("Control", 0x11),
    ("Menu", 0x12),
    ("Del", 0x2E),
    ("Ins", 0x2D),
    ("PgUp", 0x21),
    ("PgDn", 0x22),
    ("ArrowLeft", 0x25),
    ("ArrowUp", 0x26),
    ("ArrowRight", 0x27),
    ("ArrowDown", 0x28),
    ("Grave", 0xC0),
    ("Tilde", 0xC0),
    ("Plus", 0xBB),
    ("Apostrophe", 0xDE),
];

pub const BACKSPACE: u32 = 0x08;
pub const TAB: u32 = 0x09;
pub const ENTER: u32 = 0x0D;
pub const SHIFT: u32 = 0x10;
pub const CTRL: u32 = 0x11;
pub const ALT: u32 = 0x12;
pub const ESCAPE: u32 = 0x1B;
pub const SPACE: u32 = 0x20;
pub const LEFT: u32 = 0x25;
pub const UP: u32 = 0x26;
pub const RIGHT: u32 = 0x27;
pub const DOWN: u32 = 0x28;
pub const DELETE: u32 = 0x2E;

/// Code for a key name (case-insensitive, aliases accepted). `None` for unknown names.
pub fn from_name(name: &str) -> Option<u32> {
    NAMES.iter().chain(ALIASES).find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, code)| code)
}

/// Canonical name of a key code (the spelling binding files use), or `None` for unnamed codes.
pub fn name(code: u32) -> Option<&'static str> {
    NAMES.iter().find(|&&(_, c)| c == code).map(|&(n, _)| n)
}

/// Short label for on-screen prompts (`"Esc"`, `"Space"`, `";"`). ASCII only.
pub fn label(code: u32) -> Option<&'static str> {
    Some(match code {
        0x1B => "Esc",
        0x21 => "PgUp",
        0x22 => "PgDn",
        0x2E => "Del",
        0x2D => "Ins",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => return name(code),
    })
}

/// The modifier keys (`Shift`, `Ctrl`, `Alt` and their sided variants).
pub fn is_modifier(code: u32) -> bool {
    matches!(code, SHIFT | CTRL | ALT | 0xA0..=0xA5 | 0x5B | 0x5C)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_unique() {
        for &(n, code) in NAMES {
            assert_eq!(from_name(n), Some(code), "{n}");
            assert_eq!(name(code), Some(n), "{code:#x} has one canonical name");
            assert!(label(code).is_some_and(|l| l.is_ascii()));
        }
        for &(alias, code) in ALIASES {
            assert_eq!(from_name(alias), Some(code), "{alias}");
        }
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(from_name("space"), Some(SPACE));
        assert_eq!(from_name("ESC"), Some(ESCAPE));
        assert_eq!(from_name("w"), Some(0x57));
        assert_eq!(from_name("NoSuchKey"), None);
        assert_eq!(name(0xFF), None);
    }

    #[test]
    fn labels_and_modifiers() {
        assert_eq!(label(ESCAPE), Some("Esc"));
        assert_eq!(label(0xBC), Some(","));
        assert_eq!(label(SPACE), Some("Space"));
        assert!(is_modifier(SHIFT) && is_modifier(0xA5) && !is_modifier(SPACE));
    }
}
