//! Win32 backend: hand-declared `user32` imports (`raw-dylib`, no import libraries; this crate is
//! the only one allowed to declare `user32` — `kernel32` comes from `specula-sys`),
//! one window class per process, and a window procedure that turns messages into [`Event`]s.
//!
//! Each window owns a heap-allocated [`State`] whose address is stored in `GWLP_USERDATA` during
//! `WM_NCCREATE`. The window procedure only ever takes shared references to it; the event queue
//! is a `RefCell` borrowed briefly (never across a call back into the OS), so re-entrant messages
//! — e.g. `WM_SIZE` sent synchronously from inside `SetWindowPos` — are safe.

#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::{Once, OnceLock};

use specula_sys::kernel32::{GetLastError, GetModuleHandleW};
use specula_sys::wide;

use crate::decode::{self, Utf16Assembler};
use crate::{Event, MouseButton, PlatformError, PointerKind, RawWindow, WindowDesc};

type Hwnd = *mut c_void;
type Handle = *mut c_void;
type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize;

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<WndProc>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: Handle,
    h_icon: Handle,
    h_cursor: Handle,
    hbr_background: Handle,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: Handle,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
    l_private: u32,
}

/// `MONITORINFOEXW`.
#[repr(C)]
struct MonitorInfoExW {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    flags: u32,
    device: [u16; 32],
}

/// `DEVMODEW` (220 bytes) as raw bytes: only `dmSize` (written) and `dmDisplayFrequency` (read)
/// matter here, and its unions are easier skipped than declared.
#[repr(C, align(4))]
struct DevModeW([u8; DEVMODEW_SIZE]);
const DEVMODEW_SIZE: usize = 220;
const DEVMODEW_SIZE_OFFSET: usize = 68;
const DEVMODEW_FREQUENCY_OFFSET: usize = 184;

#[repr(C)]
struct CreateStructW {
    lp_create_params: *mut c_void,
    h_instance: Handle,
    h_menu: Handle,
    hwnd_parent: Hwnd,
    cy: i32,
    cx: i32,
    y: i32,
    x: i32,
    style: i32,
    lpsz_name: *const u16,
    lpsz_class: *const u16,
    dw_ex_style: u32,
}

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    fn GetDpiForSystem() -> u32;
    fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    fn AdjustWindowRectExForDpi(rect: *mut Rect, style: u32, menu: i32, ex_style: u32, dpi: u32) -> i32;
    fn RegisterClassExW(class: *const WndClassExW) -> u16;
    fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Handle,
        instance: Handle,
        param: *mut c_void,
    ) -> Hwnd;
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    fn PeekMessageW(msg: *mut Msg, hwnd: Hwnd, filter_min: u32, filter_max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> i32;
    fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn SetCapture(hwnd: Hwnd) -> Hwnd;
    fn ReleaseCapture() -> i32;
    fn ScreenToClient(hwnd: Hwnd, point: *mut Point) -> i32;
    fn GetPointerType(pointer_id: u32, pointer_type: *mut u32) -> i32;
    fn MonitorFromWindow(hwnd: Hwnd, flags: u32) -> Handle;
    fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfoExW) -> i32;
    fn EnumDisplaySettingsW(device: *const u16, mode: u32, devmode: *mut DevModeW) -> i32;
    // Test-only imports live here too: each DLL is declared by exactly one crate (AGENTS.md
    // rule 2), so integration tests and examples cannot declare their own `user32` imports.
    #[cfg(test)]
    fn PostMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> i32;
    #[cfg(test)]
    fn IsWindow(hwnd: Hwnd) -> i32;
    #[cfg(test)]
    fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, max: i32) -> i32;
}

const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

const CS_VREDRAW: u32 = 0x0001;
const CS_HREDRAW: u32 = 0x0002;
const IDC_ARROW: usize = 32512;

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const WS_EX_APPWINDOW: u32 = 0x0004_0000;
const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;

const GWLP_USERDATA: i32 = -21;
const PM_REMOVE: u32 = 0x0001;
const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 5;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SIZE_MINIMIZED: usize = 1;
const SC_KEYMENU: usize = 0xF100;
const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;

const WM_SIZE: u32 = 0x0005;
const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_CLOSE: u32 = 0x0010;
const WM_NCCREATE: u32 = 0x0081;
const WM_NCDESTROY: u32 = 0x0082;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const WM_SYSCOMMAND: u32 = 0x0112;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_CAPTURECHANGED: u32 = 0x0215;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_POINTERUPDATE: u32 = 0x0245;
const WM_POINTERDOWN: u32 = 0x0246;
const WM_POINTERUP: u32 = 0x0247;
const WM_POINTERCAPTURECHANGED: u32 = 0x024C;

const MONITOR_DEFAULTTONEAREST: u32 = 2;
const ENUM_CURRENT_SETTINGS: u32 = 0xFFFF_FFFF;

const PT_TOUCH: u32 = 2;
const PT_PEN: u32 = 3;

/// Largest client dimension accepted: `WM_SIZE` reports sizes as 16-bit words, and Win32
/// coordinates are effectively limited to 16 bits anyway.
const MAX_DIMENSION: u32 = 32767;

fn last_error(what: &str) -> PlatformError {
    // SAFETY: GetLastError has no preconditions; it reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    PlatformError { message: format!("{what} failed"), os_code: Some(code) }
}

/// Per-window state shared with the window procedure.
struct State {
    events: RefCell<Vec<Event>>,
    utf16: Cell<Utf16Assembler>,
    /// Last size reported, so redundant `WM_SIZE`s don't produce duplicate events.
    last_size: Cell<Option<(u32, u32)>>,
    /// Mouse buttons currently held (bit per `MouseButton`), for mouse capture.
    buttons: Cell<u8>,
    /// Touch/pen contacts currently down: `(pointer id, kind)`.
    pointers: RefCell<Vec<(u32, PointerKind)>>,
}

impl State {
    fn push(&self, event: Event) {
        // Never panic inside the window procedure: a re-entrant push while the queue is
        // borrowed (which our code never does) would drop the event instead.
        if let Ok(mut queue) = self.events.try_borrow_mut() {
            queue.push(event);
        }
    }
}

struct WindowClass {
    name: Vec<u16>,
    /// `HINSTANCE` of the executable, stored as an address so the class info is `Sync`.
    hinstance: usize,
}

fn init_dpi_awareness() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: plain call with a documented pseudo-handle constant. It fails harmlessly
        // (ERROR_ACCESS_DENIED) if awareness was already set by a manifest or earlier call.
        let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    });
}

fn window_class() -> Result<&'static WindowClass, PlatformError> {
    static CLASS: OnceLock<Result<WindowClass, PlatformError>> = OnceLock::new();
    CLASS.get_or_init(register_class).as_ref().map_err(Clone::clone)
}

fn register_class() -> Result<WindowClass, PlatformError> {
    // SAFETY: a null module name returns the handle of the executable; no pointer is retained.
    let hinstance = unsafe { GetModuleHandleW(null()) };
    if hinstance.is_null() {
        return Err(last_error("GetModuleHandleW"));
    }
    // The procedure's address makes the name unique even if two copies of this crate end up in
    // one process (classes are per-module, and every copy registers against the executable).
    let name = wide(&format!("Specula.Window.{:x}", wndproc as WndProc as usize));
    // SAFETY: IDC_ARROW is a predefined resource id passed as MAKEINTRESOURCE (an integer in a
    // pointer); a null instance selects the system cursors.
    let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW as *const u16) };
    let class = WndClassExW {
        cb_size: std::mem::size_of::<WndClassExW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfn_wnd_proc: Some(wndproc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: hinstance,
        h_icon: null_mut(),
        h_cursor: cursor,
        // No background brush: the renderer owns every pixel, and erasing would flicker.
        hbr_background: null_mut(),
        lpsz_menu_name: null(),
        lpsz_class_name: name.as_ptr(),
        h_icon_sm: null_mut(),
    };
    // SAFETY: `class` is fully initialised with a correct cbSize; the OS copies the strings.
    if unsafe { RegisterClassExW(&class) } == 0 {
        let err = last_error("RegisterClassExW");
        if err.os_code != Some(ERROR_CLASS_ALREADY_EXISTS) {
            return Err(err);
        }
    }
    Ok(WindowClass { name, hinstance: hinstance as usize })
}

fn styles(resizable: bool) -> (u32, u32) {
    let mut style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
    if resizable {
        style |= WS_THICKFRAME | WS_MAXIMIZEBOX;
    }
    (style, WS_EX_APPWINDOW)
}

/// Outer window size whose client area is `width×height` at `dpi`.
fn outer_size(width: u32, height: u32, style: u32, ex_style: u32, dpi: u32) -> Result<(i32, i32), PlatformError> {
    let mut rect = Rect { left: 0, top: 0, right: width as i32, bottom: height as i32 };
    // SAFETY: `rect` is a valid, writable RECT for the duration of the call.
    if unsafe { AdjustWindowRectExForDpi(&mut rect, style, 0, ex_style, dpi) } == 0 {
        return Err(last_error("AdjustWindowRectExForDpi"));
    }
    Ok((rect.right - rect.left, rect.bottom - rect.top))
}

pub(crate) struct Window {
    hwnd: Hwnd,
    hinstance: Handle,
    /// From `Box::into_raw`; freed in `Drop` after the HWND is destroyed.
    state: *mut State,
}

impl Window {
    pub(crate) fn new(desc: &WindowDesc) -> Result<Window, PlatformError> {
        if !(1..=MAX_DIMENSION).contains(&desc.width) || !(1..=MAX_DIMENSION).contains(&desc.height) {
            return Err(PlatformError {
                message: format!(
                    "window size {}x{} out of range (each side must be 1..={MAX_DIMENSION})",
                    desc.width, desc.height
                ),
                os_code: None,
            });
        }
        init_dpi_awareness();
        let class = window_class()?;
        let hinstance = class.hinstance as Handle;
        let (style, ex_style) = styles(desc.resizable);
        // SAFETY: no preconditions.
        let system_dpi = unsafe { GetDpiForSystem() };
        let (outer_w, outer_h) = outer_size(desc.width, desc.height, style, ex_style, system_dpi)?;

        let state = Box::into_raw(Box::new(State {
            events: RefCell::new(Vec::new()),
            utf16: Cell::new(Utf16Assembler::default()),
            last_size: Cell::new(None),
            buttons: Cell::new(0),
            pointers: RefCell::new(Vec::new()),
        }));
        let title = wide(&desc.title);
        // SAFETY: class name and title are NUL-terminated and outlive the call; the class was
        // registered against `hinstance`. `state` is passed as lpParam and picked up in
        // WM_NCCREATE; it stays valid until `Drop` (or the error path below) frees it.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class.name.as_ptr(),
                title.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                outer_w,
                outer_h,
                null_mut(),
                null_mut(),
                hinstance,
                state.cast(),
            )
        };
        if hwnd.is_null() {
            let err = last_error("CreateWindowExW");
            // SAFETY: creation failed, so WM_NCDESTROY (if WM_NCCREATE ran at all) has already
            // cleared the stored pointer; nothing else references `state`.
            drop(unsafe { Box::from_raw(state) });
            return Err(err);
        }
        let mut window = Window { hwnd, hinstance, state };

        // The window may have landed on a monitor whose DPI differs from the system DPI (or the
        // frame may differ from the estimate): fix the outer size so the client area is exact.
        if window.client_size() != (desc.width, desc.height) {
            // SAFETY: `hwnd` is a live window owned by this thread.
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let (w, h) = outer_size(desc.width, desc.height, style, ex_style, dpi)?;
            // SAFETY: as above; flags keep position and Z order.
            unsafe { SetWindowPos(hwnd, null_mut(), 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) };
        }
        if desc.visible {
            window.set_visible(true);
        }
        Ok(window)
    }

    pub(crate) fn poll_events(&mut self) -> Vec<Event> {
        let mut msg = Msg {
            hwnd: null_mut(),
            message: 0,
            w_param: 0,
            l_param: 0,
            time: 0,
            pt: Point { x: 0, y: 0 },
            l_private: 0,
        };
        // Null hwnd: pump every message of this thread (other windows' events land in their
        // own queues; thread messages are dispatched or ignored).
        // SAFETY: `msg` is a valid, writable MSG for each call.
        while unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            // SAFETY: `msg` was just filled in by PeekMessageW.
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        self.state().events.take()
    }

    pub(crate) fn raw(&self) -> RawWindow {
        RawWindow { hinstance: self.hinstance, hwnd: self.hwnd }
    }

    pub(crate) fn client_size(&self) -> (u32, u32) {
        client_size_of(self.hwnd)
    }

    pub(crate) fn set_title(&mut self, title: &str) {
        let title = wide(title);
        // SAFETY: `hwnd` is live; `title` is NUL-terminated and outlives the call.
        unsafe { SetWindowTextW(self.hwnd, title.as_ptr()) };
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        // SAFETY: `hwnd` is a live window owned by this thread.
        unsafe {
            ShowWindow(self.hwnd, if visible { SW_SHOW } else { SW_HIDE });
            // The first ShowWindow of a process may be overridden by the launcher's STARTUPINFO
            // (e.g. started hidden); a second call is honoured as given.
            if visible && IsWindowVisible(self.hwnd) == 0 {
                ShowWindow(self.hwnd, SW_SHOW);
            }
        }
    }

    fn state(&self) -> &State {
        // SAFETY: `state` comes from Box::into_raw and is only freed in Drop; all access to it
        // (here and in the window procedure) is through shared references.
        unsafe { &*self.state }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: `hwnd` is owned by this thread (`Window` is !Send). Destroying it delivers
        // WM_DESTROY/WM_NCDESTROY synchronously while `state` is still alive (WM_NCDESTROY
        // clears the user-data slot). If destruction failed the HWND is still ours, so clear the
        // slot by hand rather than leave a dangling pointer (after success the handle value may
        // already be reused, so it must not be touched). Only then is `state` freed; it came
        // from Box::into_raw and is freed exactly once.
        unsafe {
            if DestroyWindow(self.hwnd) == 0 {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            }
            drop(Box::from_raw(self.state));
        }
    }
}

/// Client size of any window, callable from any thread; `(0, 0)` if the window is gone.
pub(crate) fn client_size_of(hwnd: Hwnd) -> (u32, u32) {
    let mut rect = Rect::default();
    // SAFETY: GetClientRect validates the handle (failing for stale or foreign-invalid ones) and may
    // be called from any thread; `rect` is writable.
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return (0, 0);
    }
    ((rect.right - rect.left).max(0) as u32, (rect.bottom - rect.top).max(0) as u32)
}

/// The refresh rate of the display nearest `hwnd` (the primary one for a null or stale handle),
/// in Hz; `None` when the driver reports "default hardware" (0 or 1). Callable from any thread.
pub(crate) fn refresh_rate_of(hwnd: Hwnd) -> Option<f64> {
    // SAFETY: MonitorFromWindow accepts any handle value (null and stale ones included) with
    // MONITOR_DEFAULTTONEAREST and always returns a monitor.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MonitorInfoExW {
        cb_size: std::mem::size_of::<MonitorInfoExW>() as u32,
        rc_monitor: Rect::default(),
        rc_work: Rect::default(),
        flags: 0,
        device: [0; 32],
    };
    // SAFETY: `info` is a writable MONITORINFOEXW with cbSize set; a stale monitor handle fails.
    if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let mut mode = DevModeW([0; DEVMODEW_SIZE]);
    mode.0[DEVMODEW_SIZE_OFFSET..DEVMODEW_SIZE_OFFSET + 2].copy_from_slice(&(DEVMODEW_SIZE as u16).to_le_bytes());
    // SAFETY: `info.device` is the NUL-terminated device name GetMonitorInfoW wrote; `mode` is a
    // writable DEVMODEW of the size its dmSize declares.
    if unsafe { EnumDisplaySettingsW(info.device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) } == 0 {
        return None;
    }
    let at = DEVMODEW_FREQUENCY_OFFSET;
    let hz = u32::from_le_bytes([mode.0[at], mode.0[at + 1], mode.0[at + 2], mode.0[at + 3]]);
    refresh_hz(hz)
}

/// A reported whole-number rate in Hz: 59, 119 and 239 are the NTSC-style 60/1.001 family.
fn refresh_hz(reported: u32) -> Option<f64> {
    match reported {
        0 | 1 => None,
        hz if (hz + 1) % 30 == 0 => Some(f64::from(hz + 1) / 1.001),
        hz => Some(f64::from(hz)),
    }
}

unsafe extern "system" fn wndproc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_NCCREATE {
        // SAFETY: for WM_NCCREATE, lParam points to the CREATESTRUCTW of the CreateWindowExW
        // call, whose lpCreateParams is the `State` pointer passed by `Window::new`.
        unsafe {
            let create = &*(lparam as *const CreateStructW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lp_create_params as isize);
        }
    }
    // SAFETY: reading the user-data slot of a window of our class.
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const State;
    let handled = if state.is_null() {
        None
    } else {
        // SAFETY: the slot holds the live `State` of this window: it is set in WM_NCCREATE,
        // cleared in WM_NCDESTROY (below) and before `Window` frees the state.
        handle(unsafe { &*state }, hwnd, msg, wparam, lparam)
    };
    if msg == WM_NCDESTROY {
        // SAFETY: as above; the window is going away, so stop routing to its state.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    }
    match handled {
        Some(result) => result,
        // SAFETY: forwarding the unmodified message to the default procedure.
        None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn button_bit(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
    }
}

/// Translates one message. `Some(result)` = handled; `None` = pass to `DefWindowProcW`.
fn handle(state: &State, hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> Option<isize> {
    match msg {
        WM_CLOSE => {
            // Do not destroy: the owner decides, and dropping the `Window` closes it.
            state.push(Event::CloseRequested);
            Some(0)
        }
        WM_SIZE => {
            let size = if wparam == SIZE_MINIMIZED { (0, 0) } else { decode::lparam_size(lparam) };
            if state.last_size.replace(Some(size)) != Some(size) {
                state.push(Event::Resized { width: size.0, height: size.1 });
            }
            Some(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            state.push(Event::Focused(msg == WM_SETFOCUS));
            Some(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            state.push(Event::KeyDown { code: wparam as u32, repeat: decode::key_repeat(lparam) });
            // System keys continue to DefWindowProc so Alt+F4 and Alt+Space keep working.
            if msg == WM_KEYDOWN {
                Some(0)
            } else {
                None
            }
        }
        WM_KEYUP | WM_SYSKEYUP => {
            state.push(Event::KeyUp { code: wparam as u32 });
            if msg == WM_KEYUP {
                Some(0)
            } else {
                None
            }
        }
        WM_SYSCOMMAND if wparam & 0xFFF0 == SC_KEYMENU && lparam != isize::from(b' ') => {
            // Alt or F10 alone (and Alt+letter) would enter the menu loop, freezing input until
            // the next key; a menu-less game window doesn't want that. Alt+Space still opens the
            // system menu.
            Some(0)
        }
        WM_CHAR => {
            let mut assembler = state.utf16.get();
            let ch = assembler.push(wparam as u16);
            state.utf16.set(assembler);
            // Control characters (Enter, Backspace, Tab, Esc, Ctrl+letter) arrive as KeyDown.
            if let Some(ch) = ch.filter(|c| !c.is_control()) {
                state.push(Event::Char(ch));
            }
            Some(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = decode::lparam_point(lparam);
            state.push(Event::MouseMove { x, y });
            Some(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let button = match msg {
                WM_LBUTTONDOWN | WM_LBUTTONUP => MouseButton::Left,
                WM_RBUTTONDOWN | WM_RBUTTONUP => MouseButton::Right,
                _ => MouseButton::Middle,
            };
            let (x, y) = decode::lparam_point(lparam);
            let held = state.buttons.get();
            if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
                state.buttons.set(held | button_bit(button));
                state.push(Event::MouseDown { button, x, y });
                if held == 0 {
                    // Keep receiving moves and the release while dragging outside the window.
                    // SAFETY: `hwnd` is the live window this message was sent to.
                    unsafe { SetCapture(hwnd) };
                }
            } else {
                let remaining = held & !button_bit(button);
                state.buttons.set(remaining);
                state.push(Event::MouseUp { button, x, y });
                if remaining == 0 && held != 0 {
                    // SAFETY: no preconditions; releases capture held by this thread.
                    unsafe { ReleaseCapture() };
                }
            }
            Some(0)
        }
        WM_CAPTURECHANGED => {
            state.buttons.set(0);
            Some(0)
        }
        WM_MOUSEWHEEL => {
            state.push(Event::MouseWheel { delta: decode::wheel_notches(wparam) });
            Some(0)
        }
        WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP | WM_POINTERCAPTURECHANGED => {
            // Mouse and touchpad pointers (and ids the OS no longer knows) go to DefWindowProc,
            // which turns them into the usual mouse messages.
            let kind = pointer_kind(state, decode::pointer_id(wparam))?;
            handle_pointer(state, hwnd, msg, wparam, lparam, kind)
        }
        WM_DPICHANGED => {
            // SAFETY: for WM_DPICHANGED, lParam points to the suggested window RECT.
            let r = unsafe { *(lparam as *const Rect) };
            // SAFETY: `hwnd` is live; the resulting WM_SIZE reports the new client size.
            unsafe {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            Some(0)
        }
        _ => None,
    }
}

/// Kind of a touch or pen pointer: remembered for contacts that are down, else asked of the OS.
/// `None` for mouse/touchpad pointers and unknown ids.
fn pointer_kind(state: &State, id: u32) -> Option<PointerKind> {
    if let Some(&(_, kind)) = state.pointers.borrow().iter().find(|(p, _)| *p == id) {
        return Some(kind);
    }
    let mut pointer_type = 0u32;
    // SAFETY: `pointer_type` is writable; an unknown id makes the call fail (handled).
    if unsafe { GetPointerType(id, &mut pointer_type) } == 0 {
        return None;
    }
    match pointer_type {
        PT_TOUCH => Some(PointerKind::Touch),
        PT_PEN => Some(PointerKind::Pen),
        _ => None,
    }
}

/// Turns a touch/pen `WM_POINTER*` message into pointer events. Contact messages are handled
/// (`Some(0)`), so the OS doesn't also synthesise mouse messages for the same contact; pen hover
/// (in range, not touching) returns `None` so DefWindowProc turns it into mouse moves and the
/// cursor follows the pen.
fn handle_pointer(state: &State, hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize, kind: PointerKind) -> Option<isize> {
    let id = decode::pointer_id(wparam);
    let flags = decode::pointer_flags(wparam);
    let client = || {
        // Pointer messages carry screen coordinates.
        let (x, y) = decode::lparam_point(lparam);
        let mut p = Point { x, y };
        // SAFETY: `hwnd` is the live window this message was sent to; `p` is writable.
        unsafe { ScreenToClient(hwnd, &mut p) };
        (p.x, p.y)
    };
    let was_down = {
        let mut pointers = state.pointers.borrow_mut();
        let index = pointers.iter().position(|(p, _)| *p == id);
        match msg {
            WM_POINTERDOWN => {
                if index.is_none() {
                    pointers.push((id, kind));
                }
                index.is_some()
            }
            WM_POINTERUP | WM_POINTERCAPTURECHANGED => index.map(|i| pointers.remove(i)).is_some(),
            _ => index.is_some(),
        }
    };
    match msg {
        WM_POINTERDOWN => {
            let (x, y) = client();
            if was_down {
                // A down for a contact we think is already down: its up was lost. Restart it.
                state.push(Event::PointerCancel { id, kind });
            }
            state.push(Event::PointerDown { id, kind, x, y });
        }
        WM_POINTERUPDATE if was_down && flags & decode::POINTER_FLAG_INCONTACT != 0 => {
            let (x, y) = client();
            state.push(Event::PointerMove { id, kind, x, y });
        }
        WM_POINTERUP if was_down => {
            if flags & decode::POINTER_FLAG_CANCELED != 0 {
                state.push(Event::PointerCancel { id, kind });
            } else {
                let (x, y) = client();
                state.push(Event::PointerUp { id, kind, x, y });
            }
        }
        WM_POINTERCAPTURECHANGED if was_down => state.push(Event::PointerCancel { id, kind }),
        // Hover: let the OS make mouse moves of it.
        WM_POINTERUPDATE if flags & decode::POINTER_FLAG_INCONTACT == 0 => return None,
        // Messages for contacts that never went down in this window.
        _ => {}
    }
    Some(0)
}

/// Win32-specific window tests that inject messages or query the HWND directly. They skip
/// (print why and pass) when no window can be created, e.g. without a desktop session.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Window as PubWindow;

    #[test]
    fn reported_refresh_rates() {
        assert_eq!(refresh_hz(0), None);
        assert_eq!(refresh_hz(1), None);
        assert_eq!(refresh_hz(60), Some(60.0));
        assert_eq!(refresh_hz(240), Some(240.0));
        assert!((refresh_hz(59).unwrap() - 59.94).abs() < 0.01);
        assert!((refresh_hz(119).unwrap() - 119.88).abs() < 0.01);
    }

    #[test]
    fn the_primary_display_reports_a_plausible_refresh_rate() {
        // A null handle means the primary monitor. No desktop (a service session) may report none.
        match refresh_rate_of(std::ptr::null_mut()) {
            Some(hz) => {
                eprintln!("primary display: {hz} Hz");
                assert!((20.0..=1000.0).contains(&hz), "{hz} Hz");
            }
            None => eprintln!("skipped: no display refresh rate reported (no desktop session?)"),
        }
    }

    fn hidden(width: u32, height: u32) -> WindowDesc {
        WindowDesc { title: "specula-platform test".into(), width, height, visible: false, resizable: true }
    }

    fn try_window(desc: &WindowDesc) -> Option<PubWindow> {
        match PubWindow::new(desc) {
            Ok(window) => Some(window),
            Err(err) => {
                eprintln!("skipping: cannot create a window here: {err}");
                None
            }
        }
    }

    fn post(window: &PubWindow, msg: u32, wparam: usize, lparam: isize) {
        // SAFETY: the HWND is live (owned by `window`); PostMessageW only queues the message.
        let ok = unsafe { PostMessageW(window.raw().hwnd, msg, wparam, lparam) };
        assert_ne!(ok, 0, "PostMessageW failed");
    }

    fn point(x: i16, y: i16) -> isize {
        ((usize::from(y as u16) << 16) | usize::from(x as u16)) as isize
    }

    #[test]
    fn hidden_window_is_not_visible() {
        let Some(window) = try_window(&hidden(640, 360)) else { return };
        // SAFETY: query on a live window handle.
        assert_eq!(unsafe { IsWindowVisible(window.raw().hwnd) }, 0);
        // SAFETY: GetModuleHandleW(null) has no preconditions.
        assert_eq!(window.raw().hinstance, unsafe { GetModuleHandleW(null()) });
    }

    #[test]
    fn set_title_round_trips() {
        // Comfortably above any OS minimum window width (themes and DPI raise it past 200 px).
        let Some(mut window) = try_window(&hidden(480, 320)) else { return };
        let title = "Specula \u{2713} \u{1F600} title";
        window.set_title(title);
        let mut buf = [0u16; 64];
        // SAFETY: `buf` is writable for the length passed.
        let len = unsafe { GetWindowTextW(window.raw().hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(String::from_utf16(&buf[..len as usize]).unwrap(), title);
    }

    #[test]
    fn set_window_pos_produces_resized() {
        let Some(mut window) = try_window(&hidden(400, 300)) else { return };
        let _ = window.poll_events();
        let flags = SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE;
        // SAFETY: live window owned by this thread.
        assert_ne!(unsafe { SetWindowPos(window.raw().hwnd, null_mut(), 0, 0, 700, 500, flags) }, 0);
        let (w, h) = window.client_size();
        assert!(w > 400 && h > 300, "client grew: {w}x{h}");
        let events = window.poll_events();
        assert!(events.contains(&Event::Resized { width: w, height: h }), "{events:?}");
    }

    #[test]
    fn minimize_reports_zero_size_and_duplicates_are_dropped() {
        const SIZE_RESTORED: usize = 0;
        let Some(mut window) = try_window(&hidden(300, 200)) else { return };
        let _ = window.poll_events();
        post(&window, WM_SIZE, SIZE_MINIMIZED, point(0, 0));
        post(&window, WM_SIZE, SIZE_RESTORED, point(300, 200));
        post(&window, WM_SIZE, SIZE_RESTORED, point(300, 200));
        assert_eq!(
            window.poll_events(),
            vec![Event::Resized { width: 0, height: 0 }, Event::Resized { width: 300, height: 200 }]
        );
    }

    #[test]
    fn close_request_does_not_destroy() {
        // Comfortably above any OS minimum window width (themes and DPI raise it past 200 px).
        let Some(mut window) = try_window(&hidden(480, 320)) else { return };
        post(&window, WM_CLOSE, 0, 0);
        let events = window.poll_events();
        assert!(events.contains(&Event::CloseRequested), "{events:?}");
        // SAFETY: query only.
        assert_ne!(unsafe { IsWindow(window.raw().hwnd) }, 0, "window must survive WM_CLOSE");
        assert_eq!(window.client_size(), (480, 320));
    }

    #[test]
    fn drop_destroys_the_window() {
        let Some(window) = try_window(&hidden(120, 80)) else { return };
        let hwnd = window.raw().hwnd;
        drop(window);
        // SAFETY: query only; a stale handle is reported as not a window.
        assert_eq!(unsafe { IsWindow(hwnd) }, 0);
    }

    #[test]
    fn input_messages_become_events() {
        const VK_F5: usize = 0x74;
        let Some(mut window) = try_window(&hidden(300, 200)) else { return };
        let _ = window.poll_events();

        post(&window, WM_KEYDOWN, VK_F5, 0x003F_0001);
        post(&window, WM_KEYDOWN, VK_F5, 0x403F_0001); // bit 30: auto-repeat
        post(&window, WM_KEYUP, VK_F5, 0xC03F_0001_u32 as isize);
        post(&window, WM_CHAR, 0xD83D, 1); // surrogate pair -> one char
        post(&window, WM_CHAR, 0xDE00, 1);
        post(&window, WM_CHAR, 0x0D, 1); // Enter: control char, not text
        post(&window, WM_CHAR, usize::from(b'q'), 1);
        post(&window, WM_MOUSEMOVE, 0, point(-5, 7));
        post(&window, WM_LBUTTONDOWN, 0x0001, point(10, 20));
        post(&window, WM_MBUTTONDOWN, 0x0011, point(11, 21));
        post(&window, WM_MBUTTONUP, 0x0001, point(12, 22));
        post(&window, WM_LBUTTONUP, 0, point(13, 23));
        post(&window, WM_MOUSEWHEEL, usize::from((-240i16) as u16) << 16, point(0, 0));

        let events: Vec<Event> = window.poll_events().into_iter().filter(|e| !matches!(e, Event::Focused(_))).collect();
        assert_eq!(
            events,
            vec![
                Event::KeyDown { code: 0x74, repeat: false },
                Event::KeyDown { code: 0x74, repeat: true },
                Event::KeyUp { code: 0x74 },
                Event::Char('\u{1F600}'),
                Event::Char('q'),
                Event::MouseMove { x: -5, y: 7 },
                Event::MouseDown { button: MouseButton::Left, x: 10, y: 20 },
                Event::MouseDown { button: MouseButton::Middle, x: 11, y: 21 },
                Event::MouseUp { button: MouseButton::Middle, x: 12, y: 22 },
                Event::MouseUp { button: MouseButton::Left, x: 13, y: 23 },
                Event::MouseWheel { delta: -2.0 },
            ]
        );
    }

    #[test]
    fn pointer_messages_become_pointer_events() {
        let Some(mut window) = try_window(&hidden(300, 200)) else { return };
        let _ = window.poll_events();
        let hwnd = window.raw().hwnd;
        let state = window.inner.state();
        let screen = (40i16, 50i16);
        let mut expected = Point { x: screen.0.into(), y: screen.1.into() };
        // SAFETY: live window; `expected` is writable.
        unsafe { ScreenToClient(hwnd, &mut expected) };
        let (cx, cy) = (expected.x, expected.y);
        let wp = |id: usize, flags: u32| ((flags as usize) << 16) | id;
        let at = point(screen.0, screen.1);
        let contact = decode::POINTER_FLAG_INCONTACT;
        let canceled = decode::POINTER_FLAG_CANCELED;
        let touch = PointerKind::Touch;
        let pen = PointerKind::Pen;

        assert_eq!(handle_pointer(state, hwnd, WM_POINTERDOWN, wp(7, contact), at, touch), Some(0));
        handle_pointer(state, hwnd, WM_POINTERUPDATE, wp(7, contact), at, touch);
        // Pen hover goes to DefWindowProc (which makes mouse moves of it), not into pointer events.
        assert_eq!(handle_pointer(state, hwnd, WM_POINTERUPDATE, wp(9, 0), at, pen), None);
        handle_pointer(state, hwnd, WM_POINTERDOWN, wp(8, contact), at, touch);
        handle_pointer(state, hwnd, WM_POINTERUP, wp(7, 0), at, touch);
        handle_pointer(state, hwnd, WM_POINTERUP, wp(7, 0), at, touch); // already up: ignored
        handle_pointer(state, hwnd, WM_POINTERUP, wp(8, canceled), at, touch);
        handle_pointer(state, hwnd, WM_POINTERDOWN, wp(3, contact), at, pen);
        // The remembered kind wins over the OS lookup for contacts that are down.
        assert_eq!(pointer_kind(state, 3), Some(pen));
        handle_pointer(state, hwnd, WM_POINTERDOWN, wp(3, contact), at, pen); // lost up: restart
        handle_pointer(state, hwnd, WM_POINTERCAPTURECHANGED, wp(3, 0), 0, pen);
        // An id the OS has never seen is not ours to handle.
        assert_eq!(pointer_kind(state, 0xFFF0), None);
        assert!(state.pointers.borrow().is_empty());

        let events: Vec<Event> = window.poll_events().into_iter().filter(|e| !matches!(e, Event::Focused(_))).collect();
        assert_eq!(
            events,
            vec![
                Event::PointerDown { id: 7, kind: touch, x: cx, y: cy },
                Event::PointerMove { id: 7, kind: touch, x: cx, y: cy },
                Event::PointerDown { id: 8, kind: touch, x: cx, y: cy },
                Event::PointerUp { id: 7, kind: touch, x: cx, y: cy },
                Event::PointerCancel { id: 8, kind: touch },
                Event::PointerDown { id: 3, kind: pen, x: cx, y: cy },
                Event::PointerCancel { id: 3, kind: pen },
                Event::PointerDown { id: 3, kind: pen, x: cx, y: cy },
                Event::PointerCancel { id: 3, kind: pen },
            ]
        );
    }

    #[test]
    fn alt_alone_does_not_enter_menu_loop() {
        let Some(window) = try_window(&hidden(200, 100)) else { return };
        let hwnd = window.raw().hwnd;
        let state = window.inner.state();
        // Alt alone / F10 (lParam 0) and Alt+letter: swallowed.
        assert_eq!(handle(state, hwnd, WM_SYSCOMMAND, SC_KEYMENU, 0), Some(0));
        assert_eq!(handle(state, hwnd, WM_SYSCOMMAND, SC_KEYMENU, isize::from(b'f')), Some(0));
        // Alt+Space and other system commands (SC_CLOSE from Alt+F4) go to DefWindowProc.
        assert_eq!(handle(state, hwnd, WM_SYSCOMMAND, SC_KEYMENU, isize::from(b' ')), None);
        assert_eq!(handle(state, hwnd, WM_SYSCOMMAND, 0xF060, 0), None);
    }
}
