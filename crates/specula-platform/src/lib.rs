//! Specula platform layer: native windows and input events (keyboard, mouse, touch/pen pointers),
//! gamepads ([`gamepad`], XInput loaded at runtime), key names ([`keys`]) and the system timer's
//! resolution ([`timer`]; the only crate that declares `winmm`). Mapping input onto game actions
//! is `specula-input`'s job.
//!
//! Windows first (raw Win32 via `#[link(kind = "raw-dylib")]`, no import libraries); Linux and
//! macOS backends come later behind this same API. On other platforms `Window::new` returns
//! `PlatformError` so the rest of the workspace still compiles and headless code still runs.

use std::ffi::c_void;
use std::fmt;

mod decode;
pub mod gamepad;
pub mod keys;
pub mod timer;
#[cfg(windows)]
mod win32;

pub use gamepad::{DeadZones, GamepadButton, GamepadEvent, GamepadState, Gamepads, RawGamepad, MAX_GAMEPADS};
#[cfg(windows)]
use win32 as imp;

#[derive(Clone, Debug, PartialEq)]
pub struct WindowDesc {
    pub title: String,
    /// Client-area size in physical pixels.
    pub width: u32,
    pub height: u32,
    /// Hidden windows still have a valid surface handle (useful for tests).
    pub visible: bool,
    pub resizable: bool,
}

impl Default for WindowDesc {
    fn default() -> Self {
        WindowDesc { title: "Specula".into(), width: 1280, height: 720, visible: true, resizable: true }
    }
}

/// Native handles for creating a GPU surface. On Windows: `HINSTANCE` and `HWND`.
#[derive(Clone, Copy, Debug)]
pub struct RawWindow {
    pub hinstance: *mut c_void,
    pub hwnd: *mut c_void,
}

// SAFETY: the handles are plain identifiers, not pointers this crate dereferences. A render thread
// may hold them while another thread owns and pumps the `Window`: surface creation, presentation and
// `client_size_of` are valid from any thread on Win32.
unsafe impl Send for RawWindow {}
// SAFETY: as above; sharing the identifiers grants no access by itself.
unsafe impl Sync for RawWindow {}

/// Client-area size of a window from **any** thread (e.g. a render thread presenting to a window
/// that another thread pumps). `(0, 0)` if the window no longer exists or there is no backend.
pub fn client_size_of(raw: RawWindow) -> (u32, u32) {
    #[cfg(windows)]
    {
        win32::client_size_of(raw.hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = raw;
        (0, 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// What produced a pointer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerKind {
    /// Reserved for backends that report the mouse as a pointer (Win32 reports it through the
    /// `Mouse*` events instead).
    Mouse,
    Touch,
    Pen,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// User clicked close (or Alt+F4). The window stays open until dropped.
    CloseRequested,
    /// New client size in physical pixels (0×0 when minimized).
    Resized {
        width: u32,
        height: u32,
    },
    Focused(bool),
    /// `code` is the platform virtual-key code (Win32 `VK_*`); `repeat` for auto-repeat.
    KeyDown {
        code: u32,
        repeat: bool,
    },
    KeyUp {
        code: u32,
    },
    /// Text input (one Unicode scalar), after keyboard layout processing.
    Char(char),
    /// Client-area pixels, origin top-left.
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseDown {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    MouseUp {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    /// Wheel notches; positive = away from the user.
    MouseWheel {
        delta: f32,
    },
    /// A touch contact or pen tip touched down. `id` is stable for the contact's lifetime and may
    /// be reused afterwards. Client-area pixels, origin top-left. Touch and pen input is reported
    /// only through `Pointer*` events (Win32 does not also synthesise mouse events for it); the
    /// mouse keeps using the `Mouse*` events.
    PointerDown {
        id: u32,
        kind: PointerKind,
        x: i32,
        y: i32,
    },
    /// A contact that is down moved.
    PointerMove {
        id: u32,
        kind: PointerKind,
        x: i32,
        y: i32,
    },
    /// A contact lifted normally.
    PointerUp {
        id: u32,
        kind: PointerKind,
        x: i32,
        y: i32,
    },
    /// A contact ended abnormally (palm rejection, capture lost): treat as lifted, but don't act
    /// on it (no tap).
    PointerCancel {
        id: u32,
        kind: PointerKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformError {
    pub message: String,
    /// OS error code (Win32 `GetLastError`) when available.
    pub os_code: Option<u32>,
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.os_code {
            Some(code) => write!(f, "{} (OS error {code})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for PlatformError {}

/// A native top-level window. Not `Send`: create, pump and drop it on one thread.
///
/// Win32 notes: `Resized` is reported only when the size actually changes (the initial size may
/// be reported once while the window is created); `Char` carries text only — control characters
/// (Enter, Backspace, Tab, Esc, Ctrl+letter) are left to `KeyDown`. Alt or F10 alone does not
/// enter the system-menu loop; Alt+F4 and Alt+Space keep their usual meaning.
pub struct Window {
    inner: imp::Window,
    _not_send: std::marker::PhantomData<*mut c_void>,
}

impl Window {
    /// Creates the window. The process is made per-monitor-DPI-aware (v2) on first use so sizes are
    /// physical pixels.
    ///
    /// Each side must be `1..=32767`; the OS may still enforce a minimum width for captioned
    /// windows, so check [`Window::client_size`] for very small requests.
    pub fn new(desc: &WindowDesc) -> Result<Window, PlatformError> {
        Ok(Window { inner: imp::Window::new(desc)?, _not_send: std::marker::PhantomData })
    }

    /// Non-blocking: drain pending OS messages and return the events they produced, in order.
    pub fn poll_events(&mut self) -> Vec<Event> {
        self.inner.poll_events()
    }

    pub fn raw(&self) -> RawWindow {
        self.inner.raw()
    }

    /// Current client-area size in physical pixels.
    pub fn client_size(&self) -> (u32, u32) {
        self.inner.client_size()
    }

    pub fn set_title(&mut self, title: &str) {
        self.inner.set_title(title)
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.inner.set_visible(visible)
    }
}

/// Backend for platforms without a native implementation yet: `Window` is uninhabited.
#[cfg(not(windows))]
mod imp {
    use crate::{Event, PlatformError, RawWindow, WindowDesc};

    pub(crate) enum Window {}

    impl Window {
        pub(crate) fn new(_desc: &WindowDesc) -> Result<Window, PlatformError> {
            Err(PlatformError {
                message: format!(
                    "specula-platform: native windows are unsupported on this platform ({}); only Windows (Win32) is implemented",
                    std::env::consts::OS
                ),
                os_code: None,
            })
        }

        pub(crate) fn poll_events(&mut self) -> Vec<Event> {
            match *self {}
        }

        pub(crate) fn raw(&self) -> RawWindow {
            match *self {}
        }

        pub(crate) fn client_size(&self) -> (u32, u32) {
            match *self {}
        }

        pub(crate) fn set_title(&mut self, _title: &str) {
            match *self {}
        }

        pub(crate) fn set_visible(&mut self, _visible: bool) {
            match *self {}
        }
    }
}
