//! Another process's window, as a live check meets it (`tools/examen`): found by the process
//! that owns it, pictured as the window itself rendered it, and sent keys the way a keyboard's
//! arrive, as messages posted to that window.
//!
//! Nothing here moves the pointer, presses a key for the person at the machine, takes the
//! keyboard or raises a window: a posted message goes to the one window it names, whichever
//! window is in front, and a picture is asked of the window (`PrintWindow`), never taken off
//! the screen, so the desktop and other programs cannot be in it (HANDOFF, "Hands off the
//! person at the machine").
//!
//! Windows only; elsewhere every call answers that there is no such window. The only place
//! `gdi32` is declared (AGENTS.md rule 2).

/// A top-level window of another process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outside(isize);

/// A picture of a window's client area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    pub width: u32,
    pub height: u32,
    /// Four bytes a pixel (blue, green, red, unused), the top row first.
    pub bgra: Vec<u8>,
}

/// Enter and Escape, as [`key`] takes them: (virtual key, scan code).
pub const ENTER: (u32, u32) = (0x0D, 0x1C);
pub const ESCAPE: (u32, u32) = (0x1B, 0x01);

/// The largest client area [`shot`] pictures, a side (a window is never this large; a wrong
/// answer from the system must not ask for gigabytes).
pub const MAX_SIDE: u32 = 16_384;

/// The first visible top-level window that the process `pid` owns.
pub fn window_of(pid: u32) -> Option<Outside> {
    imp::window_of(pid)
}

/// A key pressed (`down`) or let go in `window`, posted as a keyboard's message is. `false`:
/// the window is gone.
pub fn key(window: Outside, key: (u32, u32), down: bool) -> bool {
    imp::key(window, key, down)
}

/// One typed character, posted to `window`. `false`: the window is gone.
pub fn character(window: Outside, c: char) -> bool {
    imp::character(window, c)
}

/// `window`'s client area as the window rendered it.
pub fn shot(window: Outside) -> Result<Shot, String> {
    imp::shot(window)
}

/// The process whose window is in front and has the keyboard (`None`: no window has).
pub fn foreground_pid() -> Option<u32> {
    imp::foreground_pid()
}

#[cfg(not(windows))]
mod imp {
    use super::{Outside, Shot};

    pub fn window_of(_pid: u32) -> Option<Outside> {
        None
    }
    pub fn key(_window: Outside, _key: (u32, u32), _down: bool) -> bool {
        false
    }
    pub fn character(_window: Outside, _c: char) -> bool {
        false
    }
    pub fn shot(_window: Outside) -> Result<Shot, String> {
        Err("another process's window cannot be pictured on this platform".to_owned())
    }
    pub fn foreground_pid() -> Option<u32> {
        None
    }
}

#[cfg(windows)]
mod imp {
    use super::{Outside, Shot, MAX_SIDE};
    use std::ffi::c_void;

    type Hwnd = *mut c_void;
    type Handle = *mut c_void;

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    /// `BITMAPINFOHEADER`, for a 32-bit picture with its top row first.
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_pels_per_meter: i32,
        y_pels_per_meter: i32,
        clr_used: u32,
        clr_important: u32,
    }

    type EnumProc = unsafe extern "system" fn(Hwnd, isize) -> i32;

    #[link(name = "user32", kind = "raw-dylib")]
    extern "system" {
        fn EnumWindows(visit: EnumProc, param: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: Hwnd) -> i32;
        fn GetForegroundWindow() -> Hwnd;
        fn PostMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn GetDC(hwnd: Hwnd) -> Handle;
        fn ReleaseDC(hwnd: Hwnd, dc: Handle) -> i32;
        fn PrintWindow(hwnd: Hwnd, dc: Handle, flags: u32) -> i32;
        fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    }

    #[link(name = "gdi32", kind = "raw-dylib")]
    extern "system" {
        fn CreateCompatibleDC(dc: Handle) -> Handle;
        fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
        fn SelectObject(dc: Handle, object: Handle) -> Handle;
        fn GetDIBits(dc: Handle, bitmap: Handle, start: u32, lines: u32, bits: *mut c_void, info: *mut BitmapInfoHeader, usage: u32) -> i32;
        fn DeleteObject(object: Handle) -> i32;
        fn DeleteDC(dc: Handle) -> i32;
    }

    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_CHAR: u32 = 0x0102;
    /// `PW_CLIENTONLY | PW_RENDERFULLCONTENT`: the client area, as a window that draws with the
    /// GPU presents it.
    const PRINT_CLIENT_AS_RENDERED: u32 = 3;
    const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

    /// A window's size is asked in real pixels: once a process, before the first question.
    fn in_real_pixels() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // SAFETY: no pointers; a process that already chose its awareness is refused, and
            // that answer is ignored (its sizes are then what that choice gives).
            let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        });
    }

    struct Search {
        pid: u32,
        found: Option<Hwnd>,
    }

    /// `EnumWindows`' visitor: keeps the first visible window of the process looked for.
    ///
    /// # Safety
    /// `param` is the `*mut Search` that [`window_of`] passed, alive for the whole enumeration.
    unsafe extern "system" fn visit(hwnd: Hwnd, param: isize) -> i32 {
        // SAFETY: see above; EnumWindows calls this on the caller's thread, inside the call.
        let search = unsafe { &mut *(param as *mut Search) };
        let mut owner = 0u32;
        // SAFETY: `hwnd` is the live handle the system handed this visitor; `owner` is a valid
        // out pointer.
        let visible = unsafe {
            GetWindowThreadProcessId(hwnd, &mut owner);
            IsWindowVisible(hwnd) != 0
        };
        if owner == search.pid && visible {
            search.found = Some(hwnd);
            return 0; // found: stop
        }
        1
    }

    pub fn window_of(pid: u32) -> Option<Outside> {
        in_real_pixels();
        let mut search = Search { pid, found: None };
        // SAFETY: `visit` matches the callback's type, and `search` outlives the call (the
        // enumeration is over when EnumWindows returns).
        unsafe { EnumWindows(visit, &mut search as *mut Search as isize) };
        search.found.map(|hwnd| Outside(hwnd as isize))
    }

    pub fn key(window: Outside, key: (u32, u32), down: bool) -> bool {
        if window.0 == 0 {
            // (No window: PostMessageW would post to this thread's own queue.)
            return false;
        }
        let (vk, scan) = key;
        // lParam as a keyboard's: repeat count 1, the scan code, and for a release the
        // "was down" and "going up" bits.
        let base = 1 | ((scan as isize) << 16);
        let (msg, lparam) = if down { (WM_KEYDOWN, base) } else { (WM_KEYUP, base | (0xC0 << 24)) };
        // SAFETY: PostMessageW validates the handle (a window that is gone is refused) and
        // only queues the message.
        unsafe { PostMessageW(window.0 as Hwnd, msg, vk as usize, lparam) != 0 }
    }

    pub fn character(window: Outside, c: char) -> bool {
        if window.0 == 0 {
            return false;
        }
        let mut units = [0u16; 2];
        for unit in c.encode_utf16(&mut units).iter() {
            // SAFETY: as in `key`: the handle is validated, the message only queued.
            if unsafe { PostMessageW(window.0 as Hwnd, WM_CHAR, *unit as usize, 1) } == 0 {
                return false;
            }
        }
        true
    }

    pub fn shot(window: Outside) -> Result<Shot, String> {
        in_real_pixels();
        let hwnd = window.0 as Hwnd;
        let mut rect = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: GetClientRect validates the handle and writes one RECT through a valid pointer.
        if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
            return Err("the window is gone".to_owned());
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width <= 0 || height <= 0 || width as u32 > MAX_SIDE || height as u32 > MAX_SIDE {
            return Err(format!("the window's client area is {width} x {height}"));
        }
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        let mut header = BitmapInfoHeader {
            size: std::mem::size_of::<BitmapInfoHeader>() as u32,
            width,
            height: -height, // top row first
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            clr_used: 0,
            clr_important: 0,
        };
        // SAFETY: every handle made here is checked before it is used and released before the
        // block ends; `bgra` holds `width * height * 4` bytes, which is what GetDIBits writes
        // for `height` lines of a 32-bit picture `width` wide; `header` is a valid
        // BITMAPINFOHEADER (a 32-bit uncompressed picture has no colour table after it).
        unsafe {
            let window_dc = GetDC(hwnd);
            if window_dc.is_null() {
                return Err("the window gave no device context".to_owned());
            }
            let dc = CreateCompatibleDC(window_dc);
            let bitmap = if dc.is_null() { std::ptr::null_mut() } else { CreateCompatibleBitmap(window_dc, width, height) };
            let mut result = Err("no picture could be made for the window".to_owned());
            if !bitmap.is_null() {
                let before = SelectObject(dc, bitmap);
                let printed = PrintWindow(hwnd, dc, PRINT_CLIENT_AS_RENDERED) != 0;
                // (GetDIBits wants the bitmap out of every device context.)
                SelectObject(dc, before);
                let lines = GetDIBits(dc, bitmap, 0, height as u32, bgra.as_mut_ptr().cast(), &mut header, 0);
                result = match (printed, lines == height) {
                    (true, true) => Ok(()),
                    (false, _) => Err("the window did not print itself".to_owned()),
                    (_, false) => Err(format!("{lines} of the picture's {height} lines were read")),
                };
                DeleteObject(bitmap);
            }
            if !dc.is_null() {
                DeleteDC(dc);
            }
            ReleaseDC(hwnd, window_dc);
            result?;
        }
        Ok(Shot { width: width as u32, height: height as u32, bgra })
    }

    pub fn foreground_pid() -> Option<u32> {
        // SAFETY: no preconditions; a null answer (no foreground window) is handled.
        let front = unsafe { GetForegroundWindow() };
        if front.is_null() {
            return None;
        }
        let mut pid = 0u32;
        // SAFETY: `front` was just handed out by the system; `pid` is a valid out pointer.
        unsafe { GetWindowThreadProcessId(front, &mut pid) };
        (pid != 0).then_some(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no desktop session (CI) there is no window to find: the calls answer, and say so.
    #[test]
    fn a_process_with_no_window_has_none_and_a_window_that_is_gone_is_said() {
        // (No process has this id: Windows' are multiples of four, and elsewhere there is no
        // backend to ask.)
        assert_eq!(window_of(u32::MAX), None);
        let gone = Outside(0);
        assert!(!key(gone, ENTER, true) && !key(gone, ESCAPE, false) && !character(gone, 'a'));
        assert!(shot(gone).is_err());
    }
}
