//! Text input beyond `Char`: the system clipboard (plain Unicode text) and IME composition.
//!
//! - **Clipboard:** [`clipboard_text`] and [`set_clipboard_text`] read and write the OS clipboard's
//!   Unicode text (`CF_UNICODETEXT` on Windows). Only a human's window path should use them:
//!   agents get their own clipboard (`specula_text::MemoryClipboard`) so they never touch the
//!   person's.
//! - **IME** (Windows IMM32, `imm32.dll`, declared in this crate only, AGENTS.md rule 2): while a
//!   composition is open the window reports [`crate::Event::ImeComposition`] (the string so far
//!   and its caret); when the user picks the result, [`crate::Event::ImeCommit`] followed by the
//!   result's characters as `Char` events (so every reader of `Char` gets IME text), then
//!   [`crate::Event::ImeEnd`]. [`crate::Window::set_ime_position`] puts the IME's candidate window
//!   next to the caret (the engine doesn't call it yet: its window lives on the UI thread). The
//!   system's own composition window still shows beside an inline one.
//!
//! Other platforms answer "unsupported" until their backends exist.

use crate::PlatformError;

/// Longest clipboard text read (bytes of UTF-8 after conversion); longer text is cut.
pub const MAX_CLIPBOARD_TEXT: usize = 1 << 20;

/// The clipboard's text, if it holds text.
pub fn clipboard_text() -> Result<Option<String>, PlatformError> {
    imp::get()
}

/// Puts `text` on the clipboard (replacing what was there).
pub fn set_clipboard_text(text: &str) -> Result<(), PlatformError> {
    imp::set(text)
}

/// Converts UTF-16 from the OS to a `String` (unpaired surrogates become U+FFFD), at most
/// `max` bytes, stopping at a NUL.
#[cfg_attr(not(windows), allow(dead_code))] // the Windows backend's (and tests')
pub(crate) fn from_wide(units: &[u16], max: usize) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    let mut s = String::new();
    for c in char::decode_utf16(units[..end].iter().copied()).map(|r| r.unwrap_or('\u{FFFD}')) {
        if s.len() + c.len_utf8() > max {
            break;
        }
        s.push(c);
    }
    s
}

#[cfg(not(windows))]
mod imp {
    use crate::PlatformError;

    pub fn get() -> Result<Option<String>, PlatformError> {
        Err(PlatformError { message: "the clipboard is unsupported on this platform".into(), os_code: None })
    }

    pub fn set(_: &str) -> Result<(), PlatformError> {
        Err(PlatformError { message: "the clipboard is unsupported on this platform".into(), os_code: None })
    }
}

#[cfg(windows)]
mod imp {
    use super::{from_wide, MAX_CLIPBOARD_TEXT};
    use crate::PlatformError;
    use specula_sys::kernel32::{GetLastError, GlobalAlloc, GlobalFree, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use std::ffi::c_void;

    const CF_UNICODETEXT: u32 = 13;

    #[link(name = "user32", kind = "raw-dylib")]
    extern "system" {
        fn OpenClipboard(owner: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
        fn SetClipboardData(format: u32, memory: *mut c_void) -> *mut c_void;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
    }

    fn failed(what: &str) -> PlatformError {
        // SAFETY: no preconditions; reads this thread's last-error value.
        let code = unsafe { GetLastError() };
        PlatformError { message: format!("{what} failed"), os_code: Some(code) }
    }

    /// Opens the clipboard for this call: one try, never a wait (another program holding it
    /// makes this call fail; the person tries again).
    struct Open;

    impl Open {
        fn new() -> Result<Open, PlatformError> {
            // SAFETY: a null owner is allowed (the clipboard is tied to this task).
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Ok(Open);
            }
            Err(failed("OpenClipboard"))
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: this thread opened the clipboard in `Open::new`.
            unsafe { CloseClipboard() };
        }
    }

    pub fn get() -> Result<Option<String>, PlatformError> {
        // SAFETY: no preconditions.
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
            return Ok(None);
        }
        let _open = Open::new()?;
        // SAFETY: the clipboard is open; the handle stays owned by the clipboard.
        let h = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if h.is_null() {
            return Ok(None);
        }
        // SAFETY: `h` is a live global memory handle from the clipboard; locking gives its bytes,
        // `GlobalSize` their count; it is unlocked before the clipboard closes.
        unsafe {
            let p = GlobalLock(h) as *const u16;
            if p.is_null() {
                return Err(failed("GlobalLock"));
            }
            let units = (GlobalSize(h) / 2).min(MAX_CLIPBOARD_TEXT);
            let text = from_wide(std::slice::from_raw_parts(p, units), MAX_CLIPBOARD_TEXT);
            GlobalUnlock(h);
            Ok(Some(text))
        }
    }

    pub fn set(text: &str) -> Result<(), PlatformError> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * 2;
        // SAFETY: allocating moveable global memory (the clipboard requires it).
        let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
        if h.is_null() {
            return Err(failed("GlobalAlloc"));
        }
        // SAFETY: `h` is ours and at least `bytes` long; the copy fits; unlocked before handing
        // over. On any failure before SetClipboardData succeeds, it is freed here.
        unsafe {
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                GlobalFree(h);
                return Err(failed("GlobalLock"));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
            GlobalUnlock(h);
        }
        let open = match Open::new() {
            Ok(o) => o,
            Err(e) => {
                // SAFETY: still ours.
                unsafe { GlobalFree(h) };
                return Err(e);
            }
        };
        // SAFETY: the clipboard is open; after SetClipboardData succeeds the system owns `h`.
        unsafe {
            EmptyClipboard();
            if SetClipboardData(CF_UNICODETEXT, h).is_null() {
                let e = failed("SetClipboardData");
                GlobalFree(h);
                drop(open);
                return Err(e);
            }
        }
        drop(open);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_text_converts_and_stops_at_nul_and_limits() {
        let w: Vec<u16> = "héllo 世界".encode_utf16().chain([0, 65, 66]).collect();
        assert_eq!(from_wide(&w, 1000), "héllo 世界");
        assert_eq!(from_wide(&w, 3), "hé");
        assert_eq!(from_wide(&[0xD800, 65], 10), "\u{FFFD}A", "an unpaired surrogate");
        assert_eq!(from_wide(&[], 10), "");
    }

    /// Round-trips text through the real clipboard (Windows desktops only; restores the text that
    /// was there). Opt-in with `SPECULA_CLIPBOARD_TEST=1`: it borrows the person's clipboard, and
    /// anything on it that isn't text would be lost.
    #[test]
    fn the_system_clipboard_round_trips_unicode() {
        if std::env::var_os("SPECULA_CLIPBOARD_TEST").is_none() {
            println!("skipped: set SPECULA_CLIPBOARD_TEST=1 to borrow the system clipboard");
            return;
        }
        let before = match clipboard_text() {
            Ok(b) => b,
            Err(e) => {
                println!("skipped: no clipboard ({e})");
                return;
            }
        };
        let probe = "Specula ✓ Здесь λ 墓";
        if let Err(e) = set_clipboard_text(probe) {
            println!("skipped: clipboard busy ({e})");
            return;
        }
        let got = clipboard_text().unwrap();
        if let Some(b) = before {
            let _ = set_clipboard_text(&b);
        }
        assert_eq!(got.as_deref(), Some(probe));
    }
}
