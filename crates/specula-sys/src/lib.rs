//! Shared OS imports.
//!
//! # Why this crate exists
//!
//! On `windows-gnu`, when two crates in one binary both declare `#[link(name = "X", kind = "raw-dylib")]`
//! imports from the same DLL, the linker keeps one import descriptor and **silently drops the
//! second crate's functions** — calls through them jump to unbound IAT slots (access violations or
//! wrong functions). So the workspace rule is: **each DLL is declared in exactly one crate.**
//!
//! | DLL | Declared in |
//! |---|---|
//! | `kernel32` | this crate (`kernel32` module + [`dynlib`]) |
//! | `user32` | `specula-platform` |
//! | `vulkan-1` | never linked — loaded at runtime via [`dynlib::Library`] |
//! | `xinput1_4` / `xinput9_1_0` | never linked — loaded at runtime by `specula-platform` (gamepads) |
//!
//! Tests and examples must not declare imports from a DLL their dependencies already declare;
//! add the function here (or to the owning crate) instead.

#[cfg(windows)]
pub mod kernel32 {
    use std::ffi::c_void;

    #[link(name = "kernel32", kind = "raw-dylib")]
    extern "system" {
        pub fn GetLastError() -> u32;
        pub fn SetLastError(code: u32);
        pub fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
        pub fn LoadLibraryW(file_name: *const u16) -> *mut c_void;
        pub fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *mut c_void;
        pub fn FreeLibrary(module: *mut c_void) -> i32;
        pub fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
        pub fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        pub fn GetExitCodeProcess(process: *mut c_void, exit_code: *mut u32) -> i32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
    }
}

/// Whether a process with this id is currently running. Conservative: on platforms without an
/// implementation, or when the answer can't be determined (e.g. access denied), returns `true`.
pub fn process_is_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const STILL_ACTIVE: u32 = 259;
        const ERROR_INVALID_PARAMETER: u32 = 87;
        // SAFETY: plain call; a null result is handled below.
        let handle = unsafe { kernel32::OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            // ERROR_INVALID_PARAMETER means no such process; anything else (access denied) is unknown.
            // SAFETY: reads the calling thread's last-error value; no preconditions.
            return unsafe { kernel32::GetLastError() } != ERROR_INVALID_PARAMETER;
        }
        let mut code = 0u32;
        // SAFETY: `handle` is a live process handle with query rights; `code` is writable. The handle
        // is closed exactly once.
        unsafe {
            let ok = kernel32::GetExitCodeProcess(handle, &mut code) != 0;
            kernel32::CloseHandle(handle);
            !ok || code == STILL_ACTIVE
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        true
    }
}

/// Graceful shutdown on Ctrl+C, Ctrl+Break, or the console window closing.
///
/// [`install`] registers a handler; the main loop polls [`requested`] and, once it has cleaned up,
/// calls [`finished`]. For console close / logoff / shutdown the OS terminates the process as soon
/// as the handler returns, so the handler waits (up to ~4.5 s, inside the OS's 5 s budget) for
/// [`finished`] before returning.
pub mod interrupt {
    use std::sync::atomic::{AtomicBool, Ordering};

    static REQUESTED: AtomicBool = AtomicBool::new(false);
    static FINISHED: AtomicBool = AtomicBool::new(false);

    #[cfg(windows)]
    const CTRL_C_EVENT: u32 = 0;
    #[cfg(windows)]
    const CTRL_BREAK_EVENT: u32 = 1;
    #[cfg(windows)]
    const CTRL_CLOSE_EVENT: u32 = 2;
    #[cfg(windows)]
    const CTRL_LOGOFF_EVENT: u32 = 5;
    #[cfg(windows)]
    const CTRL_SHUTDOWN_EVENT: u32 = 6;

    /// How long a close/logoff/shutdown handler waits for the main loop to finish.
    #[cfg(windows)]
    const CLOSE_GRACE: std::time::Duration = std::time::Duration::from_millis(4500);

    /// Start catching interrupts. Idempotent. Errors carry the OS error code.
    pub fn install() -> Result<(), String> {
        #[cfg(windows)]
        {
            // SAFETY: registers a plain `extern "system"` function that only touches atomics.
            if unsafe { crate::kernel32::SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
                // SAFETY: reads the calling thread's last-error value; no preconditions.
                let code = unsafe { crate::kernel32::GetLastError() };
                return Err(format!("SetConsoleCtrlHandler failed (OS error {code})"));
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            Err("interrupt handling is not implemented on this platform yet".into())
        }
    }

    /// True once an interrupt (or [`request`]) asked the process to stop.
    pub fn requested() -> bool {
        REQUESTED.load(Ordering::SeqCst)
    }

    /// Ask for shutdown from inside the process (same effect as Ctrl+C).
    pub fn request() {
        REQUESTED.store(true, Ordering::SeqCst);
    }

    /// Tell a waiting close handler that cleanup is done and the process may end.
    pub fn finished() {
        FINISHED.store(true, Ordering::SeqCst);
    }

    /// Runs on a thread the OS creates for each console control event.
    #[cfg(windows)]
    unsafe extern "system" fn handler(event: u32) -> i32 {
        handle(event)
    }

    #[cfg(windows)]
    fn handle(event: u32) -> i32 {
        match event {
            CTRL_C_EVENT | CTRL_BREAK_EVENT => {
                request();
                1 // handled: the process keeps running and shuts down by itself
            }
            CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
                request();
                let deadline = std::time::Instant::now() + CLOSE_GRACE;
                while !FINISHED.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                1
            }
            _ => 0,
        }
    }

    #[cfg(all(test, windows))]
    pub(crate) fn reset_for_test() {
        REQUESTED.store(false, Ordering::SeqCst);
        FINISHED.store(false, Ordering::SeqCst);
    }

    #[cfg(all(test, windows))]
    pub(crate) fn simulate(event: u32) -> i32 {
        handle(event)
    }
}

/// UTF-16, NUL-terminated (for `*W` Win32 functions).
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Runtime-loaded shared libraries (drivers such as `vulkan-1.dll` that may be absent).
pub mod dynlib {
    use std::ffi::c_void;
    use std::fmt;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct LoadError {
        pub library: String,
        pub message: String,
        /// OS error code (`GetLastError`) when available.
        pub os_code: Option<u32>,
    }

    impl fmt::Display for LoadError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "cannot load {}: {}", self.library, self.message)?;
            if let Some(code) = self.os_code {
                write!(f, " (OS error {code})")?;
            }
            Ok(())
        }
    }

    impl std::error::Error for LoadError {}

    /// A loaded library; unloaded on drop. Symbols must not be used after the `Library` is dropped.
    pub struct Library {
        // Only constructed on Windows so far; other platforms always fail in `open`.
        #[cfg_attr(not(windows), allow(dead_code))]
        handle: *mut c_void,
        name: String,
    }

    // SAFETY: a module handle is process-global; loading/looking up symbols is thread-safe in Win32.
    unsafe impl Send for Library {}
    // SAFETY: as above — GetProcAddress may be called concurrently.
    unsafe impl Sync for Library {}

    impl Library {
        /// Load by file name (e.g. `"vulkan-1.dll"`), using the OS search order.
        pub fn open(name: &str) -> Result<Library, LoadError> {
            #[cfg(windows)]
            {
                let wide = crate::wide(name);
                // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call.
                let handle = unsafe { crate::kernel32::LoadLibraryW(wide.as_ptr()) };
                if handle.is_null() {
                    // SAFETY: reads the calling thread's last-error value; no preconditions.
                    let code = unsafe { crate::kernel32::GetLastError() };
                    return Err(LoadError { library: name.into(), message: "LoadLibraryW failed".into(), os_code: Some(code) });
                }
                Ok(Library { handle, name: name.into() })
            }
            #[cfg(not(windows))]
            {
                Err(LoadError {
                    library: name.into(),
                    message: "runtime library loading is not implemented on this platform yet".into(),
                    os_code: None,
                })
            }
        }

        pub fn name(&self) -> &str {
            &self.name
        }

        /// Address of an exported symbol, or `None` if it isn't exported. The caller must transmute it
        /// to the correct `extern "system" fn` type and not use it after this `Library` is dropped.
        pub fn symbol(&self, name: &str) -> Option<*mut c_void> {
            #[cfg(windows)]
            {
                let mut c_name = name.as_bytes().to_vec();
                c_name.push(0);
                // SAFETY: `handle` is a live module handle owned by `self`; `c_name` is NUL-terminated.
                let addr = unsafe { crate::kernel32::GetProcAddress(self.handle, c_name.as_ptr()) };
                (!addr.is_null()).then_some(addr)
            }
            #[cfg(not(windows))]
            {
                let _ = name;
                None
            }
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            #[cfg(windows)]
            // SAFETY: `handle` came from LoadLibraryW and is released exactly once here.
            unsafe {
                crate::kernel32::FreeLibrary(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn process_liveness() {
        assert!(process_is_running(std::process::id()));
        let mut child = std::process::Command::new("cmd").args(["/C", "exit 0"]).spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        drop(child); // closes our handle so the process object can go away
        // The pid may be reused in theory, but not within this instant in practice.
        assert!(!process_is_running(pid));
        assert!(!process_is_running(0xFFFF_FFF0));
    }

    #[test]
    fn wide_is_nul_terminated_utf16() {
        assert_eq!(wide("Aé"), vec![0x41, 0xE9, 0]);
    }

    /// One test for all interrupt behaviour: the flags are process-global.
    #[cfg(windows)]
    #[test]
    fn interrupt_flags_follow_console_events() {
        interrupt::reset_for_test();
        interrupt::install().expect("SetConsoleCtrlHandler");
        interrupt::install().expect("installing twice is harmless");
        assert!(!interrupt::requested());
        assert_eq!(interrupt::simulate(7), 0, "unknown events are passed on");
        assert!(!interrupt::requested());
        assert_eq!(interrupt::simulate(0), 1, "Ctrl+C is handled");
        assert!(interrupt::requested());

        // Console close waits for `finished` (here signalled from another thread).
        interrupt::reset_for_test();
        let start = std::time::Instant::now();
        let signaller = std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            interrupt::finished();
        });
        assert_eq!(interrupt::simulate(2), 1);
        signaller.join().unwrap();
        let waited = start.elapsed();
        assert!(interrupt::requested());
        assert!(waited >= std::time::Duration::from_millis(90) && waited < std::time::Duration::from_secs(2), "{waited:?}");
        interrupt::reset_for_test();
    }

    #[cfg(windows)]
    #[test]
    fn loads_a_system_library_and_finds_symbols() {
        let lib = dynlib::Library::open("kernel32.dll").expect("kernel32 is always present");
        assert!(lib.symbol("GetTickCount64").is_some());
        assert!(lib.symbol("NoSuchFunctionInKernel32").is_none());
        let err = dynlib::Library::open("specula-no-such-library.dll").err().expect("missing library errors");
        assert_eq!(err.os_code, Some(126)); // ERROR_MOD_NOT_FOUND
    }
}
