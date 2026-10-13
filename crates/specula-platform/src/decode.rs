//! Pure helpers for decoding OS message parameters and text. Platform-independent so they are
//! unit-tested everywhere; only the Win32 backend uses them today.

#![cfg_attr(not(windows), allow(dead_code))]

/// Reassembles Unicode scalars from UTF-16 code units delivered one at a time (Win32 `WM_CHAR`
/// sends a surrogate pair as two messages). Unpaired surrogates are dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Utf16Assembler {
    high: Option<u16>,
}

impl Utf16Assembler {
    pub(crate) fn push(&mut self, unit: u16) -> Option<char> {
        match unit {
            0xD800..=0xDBFF => {
                // A second high surrogate replaces an unpaired first one.
                self.high = Some(unit);
                None
            }
            0xDC00..=0xDFFF => {
                let high = self.high.take()?;
                let scalar = 0x10000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(unit) - 0xDC00);
                char::from_u32(scalar)
            }
            _ => {
                self.high = None;
                char::from_u32(u32::from(unit))
            }
        }
    }
}

fn loword(v: isize) -> u16 {
    (v as usize & 0xFFFF) as u16
}

fn hiword(v: isize) -> u16 {
    ((v as usize >> 16) & 0xFFFF) as u16
}

/// Signed client coordinates packed in a mouse message's `lParam` (`GET_X_LPARAM`/`GET_Y_LPARAM`).
/// Negative values occur while the mouse is captured outside the client area.
pub(crate) fn lparam_point(lparam: isize) -> (i32, i32) {
    (i32::from(loword(lparam) as i16), i32::from(hiword(lparam) as i16))
}

/// Unsigned width/height packed in `WM_SIZE`'s `lParam`.
pub(crate) fn lparam_size(lparam: isize) -> (u32, u32) {
    (u32::from(loword(lparam)), u32::from(hiword(lparam)))
}

/// Bit 30 of a key message's `lParam`: the key was already down (auto-repeat).
pub(crate) fn key_repeat(lparam: isize) -> bool {
    (lparam as usize >> 30) & 1 != 0
}

/// `WM_MOUSEWHEEL`'s signed delta (high word of `wParam`) in notches of 120; positive = away
/// from the user.
pub(crate) fn wheel_notches(wparam: usize) -> f32 {
    f32::from(hiword(wparam as isize) as i16) / 120.0
}

/// `WM_POINTER*`'s pointer id (`GET_POINTERID_WPARAM`: low word of `wParam`).
pub(crate) fn pointer_id(wparam: usize) -> u32 {
    u32::from(loword(wparam as isize))
}

/// `WM_POINTER*`'s `POINTER_MESSAGE_FLAG_*` bits (high word of `wParam`).
pub(crate) fn pointer_flags(wparam: usize) -> u32 {
    u32::from(hiword(wparam as isize))
}

/// `POINTER_MESSAGE_FLAG_INCONTACT`: touching the surface (tip down / finger down).
pub(crate) const POINTER_FLAG_INCONTACT: u32 = 0x0004;
/// `POINTER_MESSAGE_FLAG_CANCELED`: the interaction was cancelled (e.g. palm rejection).
pub(crate) const POINTER_FLAG_CANCELED: u32 = 0x8000;

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(lo: u16, hi: u16) -> isize {
        ((usize::from(hi) << 16) | usize::from(lo)) as isize
    }

    #[test]
    fn titles_encode_as_nul_terminated_utf16() {
        // Window titles go through `specula_sys::wide`; non-BMP text must become a surrogate pair.
        assert_eq!(specula_sys::wide(""), vec![0]);
        assert_eq!(specula_sys::wide("é😀"), vec![0xE9, 0xD83D, 0xDE00, 0]);
    }

    #[test]
    fn assembler_passes_bmp_characters() {
        let mut a = Utf16Assembler::default();
        assert_eq!(a.push(u16::from(b'x')), Some('x'));
        assert_eq!(a.push(0x00E9), Some('é'));
        assert_eq!(a.push(0xFFFD), Some('\u{FFFD}'));
    }

    #[test]
    fn assembler_joins_surrogate_pairs() {
        let mut a = Utf16Assembler::default();
        assert_eq!(a.push(0xD83D), None);
        assert_eq!(a.push(0xDE00), Some('😀'));
        // Highest code point.
        assert_eq!(a.push(0xDBFF), None);
        assert_eq!(a.push(0xDFFF), Some('\u{10FFFF}'));
    }

    #[test]
    fn assembler_drops_unpaired_surrogates() {
        let mut a = Utf16Assembler::default();
        assert_eq!(a.push(0xDE00), None, "lone low surrogate");
        assert_eq!(a.push(0xD83D), None);
        assert_eq!(a.push(u16::from(b'a')), Some('a'), "high surrogate followed by BMP char");
        assert_eq!(a.push(0xDE00), None, "pending high was discarded");
        assert_eq!(a.push(0xD800), None);
        assert_eq!(a.push(0xD83D), None, "second high replaces the first");
        assert_eq!(a.push(0xDE00), Some('😀'));
    }

    #[test]
    fn mouse_coordinates_are_signed() {
        assert_eq!(lparam_point(pack(10, 20)), (10, 20));
        assert_eq!(lparam_point(pack((-5i16) as u16, (-32768i16) as u16)), (-5, -32768));
        assert_eq!(lparam_point(pack(32767, 0xFFFF)), (32767, -1));
        // Upper 32 bits of a 64-bit lParam are ignored.
        assert_eq!(lparam_point(pack(3, 4) | (0x7FFF_0000_0000_0000u64 as isize)), (3, 4));
    }

    #[test]
    fn sizes_are_unsigned() {
        assert_eq!(lparam_size(pack(1920, 1080)), (1920, 1080));
        assert_eq!(lparam_size(pack(0xFFFF, 0x8000)), (65535, 32768));
        assert_eq!(lparam_size(0), (0, 0));
    }

    #[test]
    fn repeat_bit_is_bit_30() {
        assert!(!key_repeat(0x0000_0001));
        assert!(key_repeat(0x4000_0001));
        // Key-up messages also set bit 31 (transition state); bit 30 is still what matters.
        assert!(key_repeat(0xC000_0001u32 as isize));
        assert!(!key_repeat(0x8000_0001u32 as isize));
    }

    #[test]
    fn pointer_wparam_splits_id_and_flags() {
        let w = (0x8004usize << 16) | 0x0123;
        assert_eq!(pointer_id(w), 0x123);
        assert_eq!(pointer_flags(w), POINTER_FLAG_CANCELED | POINTER_FLAG_INCONTACT);
        assert_eq!(pointer_flags(0xFFFF), 0);
    }

    #[test]
    fn wheel_delta_in_notches() {
        let w = |delta: i16| (usize::from(delta as u16) << 16) | 0x0008; // low word: MK_* flags
        assert_eq!(wheel_notches(w(120)), 1.0);
        assert_eq!(wheel_notches(w(-240)), -2.0);
        assert_eq!(wheel_notches(w(60)), 0.5);
        assert_eq!(wheel_notches(w(0)), 0.0);
    }
}
