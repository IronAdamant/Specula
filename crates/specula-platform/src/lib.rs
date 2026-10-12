//! Specula platform layer: native windows and input events.
//!
//! Windows first (raw Win32 via `#[link(kind = "raw-dylib")]`, no import libraries); Linux and
//! macOS backends come later behind this same API. On other platforms `Window::new` returns
//! `PlatformError` so the rest of the workspace still compiles and headless code still runs.

use std::ffi::c_void;
use std::fmt;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// User clicked close (or Alt+F4). The window stays open until dropped.
    CloseRequested,
    /// New client size in physical pixels (0×0 when minimized).
    Resized { width: u32, height: u32 },
    Focused(bool),
    /// `code` is the platform virtual-key code (Win32 `VK_*`); `repeat` for auto-repeat.
    KeyDown { code: u32, repeat: bool },
    KeyUp { code: u32 },
    /// Text input (one Unicode scalar), after keyboard layout processing.
    Char(char),
    /// Client-area pixels, origin top-left.
    MouseMove { x: i32, y: i32 },
    MouseDown { button: MouseButton, x: i32, y: i32 },
    MouseUp { button: MouseButton, x: i32, y: i32 },
    /// Wheel notches; positive = away from the user.
    MouseWheel { delta: f32 },
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
pub struct Window {
    _not_send: std::marker::PhantomData<*mut c_void>,
}

impl Window {
    /// Creates the window. The process is made per-monitor-DPI-aware (v2) on first use so sizes are
    /// physical pixels.
    pub fn new(desc: &WindowDesc) -> Result<Window, PlatformError> {
        let _ = desc;
        todo!("specula-platform: Window::new")
    }

    /// Non-blocking: drain pending OS messages and return the events they produced, in order.
    pub fn poll_events(&mut self) -> Vec<Event> {
        todo!("specula-platform: Window::poll_events")
    }

    pub fn raw(&self) -> RawWindow {
        todo!("specula-platform: Window::raw")
    }

    /// Current client-area size in physical pixels.
    pub fn client_size(&self) -> (u32, u32) {
        todo!("specula-platform: Window::client_size")
    }

    pub fn set_title(&mut self, title: &str) {
        let _ = title;
        todo!("specula-platform: Window::set_title")
    }

    pub fn set_visible(&mut self, visible: bool) {
        let _ = visible;
        todo!("specula-platform: Window::set_visible")
    }
}
