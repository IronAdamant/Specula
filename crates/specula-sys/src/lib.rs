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

    #[test]
    fn wide_is_nul_terminated_utf16() {
        assert_eq!(wide("Aé"), vec![0x41, 0xE9, 0]);
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
