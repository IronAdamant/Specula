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
//! | `iphlpapi` | never linked — loaded at runtime by [`net`] (which ports have listeners) |
//! | `bcrypt` | never linked — loaded at runtime by [`digest`] (SHA-256 from the OS, never hand-rolled) |
//!
//! Tests and examples must not declare imports from a DLL their dependencies already declare;
//! add the function here (or to the owning crate) instead.
//!
//! Runtime-loaded system DLLs come from `System32` only ([`dynlib::Library::open`]): one planted
//! beside the executable is never loaded. DLLs that ship with the game load with
//! [`dynlib::Library::open_beside_exe`]; files the engine wrote itself with
//! [`dynlib::Library::open_path`] (docs/design/SECURITY.md).
//!
//! [`fpenv`] checks and restores the CPU's floating-point control register (determinism rule 6).

pub mod fpenv;

#[cfg(windows)]
pub mod kernel32 {
    use std::ffi::c_void;

    #[link(name = "kernel32", kind = "raw-dylib")]
    extern "system" {
        pub fn GetLastError() -> u32;
        pub fn SetLastError(code: u32);
        pub fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
        // No `LoadLibraryW`: its search order starts beside the executable, where a planted DLL
        // would stand in for a system one. Libraries load through [`crate::dynlib`] only.
        pub fn LoadLibraryExW(file_name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        pub fn GetModuleFileNameW(module: *mut c_void, file_name: *mut u16, size: u32) -> u32;
        pub fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *mut c_void;
        pub fn FreeLibrary(module: *mut c_void) -> i32;
        pub fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
        pub fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
        pub fn GetExitCodeProcess(process: *mut c_void, exit_code: *mut u32) -> i32;
        pub fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
        pub fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
        pub fn GetDiskFreeSpaceExW(directory: *const u16, free_to_caller: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
        pub fn GetVolumePathNameW(file_name: *const u16, volume_path: *mut u16, length: u32) -> i32;
        // Global memory: the clipboard's currency (specula-platform's clipboard).
        pub fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
        pub fn GlobalLock(memory: *mut c_void) -> *mut c_void;
        pub fn GlobalUnlock(memory: *mut c_void) -> i32;
        pub fn GlobalFree(memory: *mut c_void) -> *mut c_void;
        pub fn GlobalSize(memory: *mut c_void) -> usize;
        pub fn GetStdHandle(std_handle: u32) -> *mut c_void;
        pub fn SetHandleInformation(object: *mut c_void, mask: u32, flags: u32) -> i32;
        pub fn GetHandleInformation(object: *mut c_void, flags: *mut u32) -> i32;
        pub fn GetCurrentThread() -> *mut c_void;
        pub fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }

    /// `GMEM_MOVEABLE`: what clipboard data must be allocated as.
    pub const GMEM_MOVEABLE: u32 = 0x0002;

    /// `SYSTEM_POWER_STATUS`.
    #[repr(C)]
    #[derive(Default)]
    pub struct SystemPowerStatus {
        pub ac_line_status: u8,
        pub battery_flag: u8,
        pub battery_life_percent: u8,
        pub system_status_flag: u8,
        pub battery_life_time: u32,
        pub battery_full_life_time: u32,
    }
}

/// This process's standard handles and the processes it starts.
pub mod process {
    /// Marks this process's standard input, output and error as not inherited by the processes
    /// it starts from now on. Returns how many handles were changed.
    ///
    /// A program that starts a long-lived child and then exits (the command line starting an
    /// engine: `specula open`) calls this first. On Windows a child inherits every inheritable
    /// handle of its parent, whatever its own standard handles are set to: the engine held the
    /// command's output pipes open, and whoever was reading the command's answer (an agent's
    /// shell, a script) went on waiting for the engine to end. Found by the launcher's live
    /// check, which waited 40 minutes. Elsewhere a child's standard handles are replaced and
    /// the rest close when it starts: nothing to do.
    pub fn keep_std_handles() -> usize {
        #[cfg(windows)]
        {
            /// `STD_INPUT_HANDLE`, `STD_OUTPUT_HANDLE`, `STD_ERROR_HANDLE`.
            const STD: [u32; 3] = [-10i32 as u32, -11i32 as u32, -12i32 as u32];
            const HANDLE_FLAG_INHERIT: u32 = 1;
            let mut changed = 0;
            for id in STD {
                // SAFETY: GetStdHandle takes one of the three ids; SetHandleInformation is
                // given the handle it returned, checked to be a real one.
                unsafe {
                    let h = super::kernel32::GetStdHandle(id);
                    if !h.is_null() && h as isize != -1 && super::kernel32::SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0) != 0 {
                        changed += 1;
                    }
                }
            }
            changed
        }
        #[cfg(not(windows))]
        {
            0
        }
    }

    #[cfg(all(test, windows))]
    mod tests {
        /// After it, no standard handle of this process is inheritable (a test's own may be
        /// absent: only the ones that are there are looked at).
        #[test]
        fn standard_handles_are_no_longer_inherited() {
            super::keep_std_handles();
            for id in [-10i32 as u32, -11i32 as u32, -12i32 as u32] {
                // SAFETY: a query on the handle GetStdHandle returned, checked to be a real one.
                unsafe {
                    let h = crate::kernel32::GetStdHandle(id);
                    let mut flags = 0u32;
                    if !h.is_null() && h as isize != -1 && crate::kernel32::GetHandleInformation(h, &mut flags) != 0 {
                        assert_eq!(flags & 1, 0, "standard handle {} is still inheritable", id as i32);
                    }
                }
            }
        }
    }
}

/// Which TCP ports something listens on, read from the OS's connection table: no socket is
/// opened, so asking never trips a firewall prompt and never races the listener.
///
/// On Windows this runtime-loads `iphlpapi.dll` ([`dynlib`]) and reads `GetExtendedTcpTable`
/// (`TCP_TABLE_OWNER_PID_LISTENER`) for IPv4 and IPv6. Elsewhere it reports `unsupported`.
pub mod net {
    use std::collections::BTreeSet;
    use std::fmt;

    /// Why the listener table couldn't be read.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct NetError {
        /// `unsupported`, `load_failed`, `no_symbol` or `table_failed`.
        pub code: &'static str,
        pub message: String,
        /// The Win32 error code, when there is one.
        pub os_code: Option<u32>,
    }

    impl fmt::Display for NetError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.code, self.message)?;
            if let Some(c) = self.os_code {
                write!(f, " (OS error {c})")?;
            }
            Ok(())
        }
    }

    impl std::error::Error for NetError {}

    /// Ports (any local address, IPv4 or IPv6) with a listening TCP socket right now.
    pub fn tcp_listening_ports() -> Result<BTreeSet<u16>, NetError> {
        tcp_listeners().map(|l| l.into_iter().map(|(port, _)| port).collect())
    }

    /// Every listening TCP socket right now (any local address, IPv4 or IPv6): its port and the
    /// id of the process that owns it. "Something answers on that port" does not say who: a
    /// program that stopped leaves its port to the next one, and this tells them apart.
    pub fn tcp_listeners() -> Result<BTreeSet<(u16, u32)>, NetError> {
        #[cfg(windows)]
        {
            windows::listening()
        }
        #[cfg(not(windows))]
        {
            Err(NetError { code: "unsupported", message: "reading the TCP listener table is not implemented on this platform yet".into(), os_code: None })
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::NetError;
        use crate::dynlib::Library;
        use std::collections::BTreeSet;
        use std::ffi::c_void;
        use std::sync::OnceLock;

        type GetExtendedTcpTable = unsafe extern "system" fn(*mut c_void, *mut u32, i32, u32, u32, u32) -> u32;

        const AF_INET: u32 = 2;
        const AF_INET6: u32 = 23;
        const TCP_TABLE_OWNER_PID_LISTENER: u32 = 3;
        const NO_ERROR: u32 = 0;
        const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
        /// `MIB_TCPROW_OWNER_PID` and `MIB_TCP6ROW_OWNER_PID` sizes, and where `dwLocalPort` sits.
        const ROW4: usize = 24;
        const PORT4: usize = 8;
        const PID4: usize = 20;
        const ROW6: usize = 56;
        const PORT6: usize = 20;
        const PID6: usize = 52;

        fn library() -> Result<&'static Library, NetError> {
            static LIB: OnceLock<Result<Library, NetError>> = OnceLock::new();
            LIB.get_or_init(|| {
                Library::open("iphlpapi.dll").map_err(|e| NetError { code: "load_failed", message: e.to_string(), os_code: e.os_code })
            })
            .as_ref()
            .map_err(Clone::clone)
        }

        pub fn listening() -> Result<BTreeSet<(u16, u32)>, NetError> {
            let lib = library()?;
            let sym = lib
                .symbol("GetExtendedTcpTable")
                .ok_or(NetError { code: "no_symbol", message: "iphlpapi.dll has no GetExtendedTcpTable".into(), os_code: None })?;
            // SAFETY: the symbol is GetExtendedTcpTable, whose signature is
            // DWORD(PVOID, PDWORD, BOOL, ULONG, TCP_TABLE_CLASS, ULONG); `lib` lives for the process.
            let get: GetExtendedTcpTable = unsafe { std::mem::transmute::<*mut c_void, GetExtendedTcpTable>(sym) };
            let mut ports = BTreeSet::new();
            for (af, row, port_at, pid_at) in [(AF_INET, ROW4, PORT4, PID4), (AF_INET6, ROW6, PORT6, PID6)] {
                // Without an IPv6 stack there can be no IPv6 listeners: only the IPv4 table must read.
                let words = match table(get, af) {
                    Ok(words) => words,
                    Err(_) if af == AF_INET6 => continue,
                    Err(e) => return Err(e),
                };
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_ne_bytes()).collect();
                let count = words.first().copied().unwrap_or(0) as usize;
                for i in 0..count {
                    let at = 4 + i * row;
                    let (Some(b), Some(p)) = (bytes.get(at + port_at..at + port_at + 4), bytes.get(at + pid_at..at + pid_at + 4)) else { break };
                    // dwLocalPort: the port in network byte order in its low 16 bits.
                    // dwOwningPid: the row's last word.
                    ports.insert((u16::from_be_bytes([b[0], b[1]]), u32::from_ne_bytes([p[0], p[1], p[2], p[3]])));
                }
            }
            Ok(ports)
        }

        /// One listener table, as 32-bit words (the rows are 4-byte aligned).
        fn table(get: GetExtendedTcpTable, af: u32) -> Result<Vec<u32>, NetError> {
            let mut size: u32 = 0;
            // The table can grow between the size query and the read: retry a few times.
            for _ in 0..4 {
                let mut buf = vec![0u32; (size as usize).div_ceil(4).max(1)];
                let mut len = (buf.len() * 4) as u32;
                // SAFETY: `buf` is writable for `len` bytes and 4-byte aligned; `len` is a valid
                // in/out size; the other arguments are plain values the API documents.
                let rc = unsafe { get(buf.as_mut_ptr().cast(), &mut len, 0, af, TCP_TABLE_OWNER_PID_LISTENER, 0) };
                match rc {
                    NO_ERROR => return Ok(buf),
                    ERROR_INSUFFICIENT_BUFFER => size = len + 256,
                    code => {
                        return Err(NetError { code: "table_failed", message: format!("GetExtendedTcpTable (address family {af}) failed"), os_code: Some(code) })
                    }
                }
            }
            Err(NetError { code: "table_failed", message: "the TCP table kept growing while being read".into(), os_code: Some(ERROR_INSUFFICIENT_BUFFER) })
        }
    }
}

/// SHA-256 from the operating system (AGENTS rule 12: cryptography is never hand-rolled). On
/// Windows it is CNG's `BCryptHash` with the SHA-256 pseudo-handle (Windows 10 and later), from
/// `bcrypt.dll` loaded at runtime; elsewhere it answers `unsupported` until each platform's own
/// API is wired (CommonCrypto on macOS; Linux has none in the OS, so a caller must say what it
/// can't vouch for there).
pub mod digest {
    use std::fmt;

    /// Why a digest couldn't be made.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct DigestError {
        /// `unsupported`, `load_failed`, `no_symbol`, `too_big` or `hash_failed`.
        pub code: &'static str,
        pub message: String,
        /// The NTSTATUS or Win32 error code, when there is one.
        pub os_code: Option<u32>,
    }

    impl fmt::Display for DigestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.code, self.message)?;
            if let Some(c) = self.os_code {
                write!(f, " (OS code {c:#x})")?;
            }
            Ok(())
        }
    }

    impl std::error::Error for DigestError {}

    /// The SHA-256 of `bytes`.
    pub fn sha256(bytes: &[u8]) -> Result<[u8; 32], DigestError> {
        #[cfg(windows)]
        {
            windows::sha256(bytes)
        }
        #[cfg(not(windows))]
        {
            let _ = bytes;
            Err(DigestError { code: "unsupported", message: "the OS's SHA-256 is wired on Windows only so far".into(), os_code: None })
        }
    }

    /// Lowercase hex of a digest.
    pub fn hex(d: &[u8; 32]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[cfg(windows)]
    mod windows {
        use super::DigestError;
        use crate::dynlib::Library;
        use std::ffi::c_void;
        use std::sync::OnceLock;

        /// NTSTATUS BCryptHash(BCRYPT_ALG_HANDLE, PUCHAR secret, ULONG, PUCHAR input, ULONG, PUCHAR output, ULONG).
        type BCryptHash = unsafe extern "system" fn(*mut c_void, *const u8, u32, *const u8, u32, *mut u8, u32) -> i32;
        /// `BCRYPT_SHA256_ALG_HANDLE`: a pseudo-handle, no provider to open or close.
        const SHA256_ALG_HANDLE: usize = 0x41;

        fn hash_fn() -> Result<BCryptHash, DigestError> {
            static LIB: OnceLock<Result<Library, DigestError>> = OnceLock::new();
            let lib = LIB
                .get_or_init(|| Library::open("bcrypt.dll").map_err(|e| DigestError { code: "load_failed", message: e.to_string(), os_code: e.os_code }))
                .as_ref()
                .map_err(Clone::clone)?;
            let sym = lib.symbol("BCryptHash").ok_or(DigestError { code: "no_symbol", message: "bcrypt.dll has no BCryptHash (Windows 10 or later needed)".into(), os_code: None })?;
            // SAFETY: the symbol is BCryptHash with the signature above; `lib` lives for the process
            // (a static), so the function pointer stays valid.
            Ok(unsafe { std::mem::transmute::<*mut c_void, BCryptHash>(sym) })
        }

        pub fn sha256(bytes: &[u8]) -> Result<[u8; 32], DigestError> {
            let hash = hash_fn()?;
            let len = u32::try_from(bytes.len()).map_err(|_| DigestError { code: "too_big", message: format!("{} bytes (at most 4 GiB a call)", bytes.len()), os_code: None })?;
            let mut out = [0u8; 32];
            // SAFETY: the pseudo-handle names SHA-256; no secret (null, 0); `bytes` is readable for
            // `len` bytes; `out` is writable for its 32 bytes, SHA-256's size.
            let status = unsafe { hash(SHA256_ALG_HANDLE as *mut c_void, std::ptr::null(), 0, bytes.as_ptr(), len, out.as_mut_ptr(), 32) };
            if status != 0 {
                return Err(DigestError { code: "hash_failed", message: "BCryptHash (SHA-256) failed".into(), os_code: Some(status as u32) });
            }
            Ok(out)
        }
    }
}

/// The machine's power source, for hints about how much it can spare (a laptop on battery
/// shouldn't be pushed). `None` where unknown (other platforms, or the OS doesn't say).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Power {
    /// The machine has a system battery (a laptop or handheld, most likely).
    pub has_battery: bool,
    /// Running on mains power (`None`: unknown).
    pub on_ac: Option<bool>,
}

/// See [`Power`].
pub fn power() -> Option<Power> {
    #[cfg(windows)]
    {
        let mut st = kernel32::SystemPowerStatus::default();
        // SAFETY: `st` is a writable SYSTEM_POWER_STATUS of the documented layout.
        if unsafe { kernel32::GetSystemPowerStatus(&mut st) } == 0 {
            return None;
        }
        // BatteryFlag: 128 = no system battery, 255 = unknown. ACLineStatus: 0 off, 1 on, 255 unknown.
        if st.battery_flag == 255 {
            return None;
        }
        let on_ac = match st.ac_line_status {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        Some(Power { has_battery: st.battery_flag & 128 == 0, on_ac })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Free space on the volume that holds a folder (the disk footprint budget reads it for every
/// folder the engine writes to).
///
/// On Windows this is `GetDiskFreeSpaceExW` (what this process may use, honouring quotas) and
/// `GetVolumePathNameW` (which volume, so folders on one drive are counted once). Elsewhere it
/// reports `unsupported`: `std` has no `statvfs`, and guessing would mislead.
pub mod disk {
    use std::fmt;
    use std::path::{Path, PathBuf};

    /// Space on one volume, in bytes.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Space {
        /// The volume's root (`C:\`, or a mount folder).
        pub volume: PathBuf,
        /// Free to this process (quotas applied): what writes can use.
        pub available: u64,
        pub total: u64,
        /// Free on the volume, for everyone.
        pub free: u64,
    }

    /// Why free space couldn't be read.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct DiskError {
        /// `unsupported`, `bad_path` or `query_failed`.
        pub code: &'static str,
        pub message: String,
        /// The Win32 error code, when there is one.
        pub os_code: Option<u32>,
    }

    impl fmt::Display for DiskError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}: {}", self.code, self.message)?;
            if let Some(c) = self.os_code {
                write!(f, " (OS error {c})")?;
            }
            Ok(())
        }
    }

    impl std::error::Error for DiskError {}

    /// The space on the volume holding `dir`. `dir` need not exist: its nearest existing
    /// ancestor is asked (a folder the engine will create is on that ancestor's volume).
    pub fn space(dir: &Path) -> Result<Space, DiskError> {
        #[cfg(windows)]
        {
            windows::space(dir)
        }
        #[cfg(not(windows))]
        {
            let _ = dir;
            Err(DiskError { code: "unsupported", message: "reading free disk space is not implemented on this platform yet".into(), os_code: None })
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::{DiskError, Space};
        use crate::kernel32;
        use std::os::windows::ffi::OsStrExt;
        use std::path::{Path, PathBuf};

        fn wide_path(p: &Path) -> Result<Vec<u16>, DiskError> {
            let w: Vec<u16> = p.as_os_str().encode_wide().collect();
            if w.contains(&0) {
                return Err(DiskError { code: "bad_path", message: format!("{} holds a NUL", p.display()), os_code: None });
            }
            Ok(w.into_iter().chain(std::iter::once(0)).collect())
        }

        pub fn space(dir: &Path) -> Result<Space, DiskError> {
            // The nearest folder that exists (the engine may not have made its own yet).
            let mut at = Some(dir);
            while let Some(p) = at {
                if p.is_dir() {
                    break;
                }
                at = p.parent();
            }
            let Some(at) = at.filter(|p| !p.as_os_str().is_empty()) else {
                return Err(DiskError { code: "bad_path", message: format!("no folder of {} exists", dir.display()), os_code: None });
            };
            let w = wide_path(at)?;
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            // SAFETY: `w` is a NUL-terminated UTF-16 path; the three outputs are writable u64s.
            let ok = unsafe { kernel32::GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
            if ok == 0 {
                // SAFETY: reads the calling thread's last-error value; no preconditions.
                let code = unsafe { kernel32::GetLastError() };
                return Err(DiskError { code: "query_failed", message: format!("GetDiskFreeSpaceExW failed for {}", at.display()), os_code: Some(code) });
            }
            let mut buf = vec![0u16; 1024];
            // SAFETY: `w` is NUL-terminated; `buf` holds `buf.len()` writable UTF-16 units.
            let named = unsafe { kernel32::GetVolumePathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
            let volume = if named != 0 {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                PathBuf::from(String::from_utf16_lossy(&buf[..n]))
            } else {
                at.ancestors().last().map_or_else(|| at.to_path_buf(), Path::to_path_buf)
            };
            Ok(Space { volume, available: avail, total, free })
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn free_space_of_the_temp_folder() {
            match super::space(&std::env::temp_dir().join("specula-not-made-yet").join("deeper")) {
                Ok(s) => {
                    assert!(s.total > 0 && s.available <= s.total && s.free <= s.total, "{s:?}");
                    assert!(!s.volume.as_os_str().is_empty());
                }
                Err(e) => {
                    assert_eq!(e.code, "unsupported", "{e}");
                    eprintln!("free space unavailable here: {e}");
                }
            }
        }
    }
}

/// How long the calling thread itself has run.
pub mod thread {
    use std::time::{Duration, Instant};

    /// The calling thread's own processor time, in milliseconds since some fixed point: what
    /// it ran, not what passed while it waited (for a processor, a lock, a disk or the GPU).
    ///
    /// Beside wall time it tells "this work took 55 ms" apart from "this work took 6 ms and
    /// then waited 49": on a machine whose processors are all busy the second is common, and
    /// reads exactly like the first without it (performance pass, 2026-11-14: frame stalls
    /// that only appeared beside compilers).
    ///
    /// Windows counts a thread's processor cycles exactly at every switch
    /// (`QueryThreadCycleTime`); its thread *times* are counted in 15.6 ms steps and are no
    /// use here. Cycles become milliseconds by the rate measured in [`CpuClock::calibrate`].
    #[derive(Clone, Copy, Debug)]
    pub struct CpuClock {
        cycles_per_ms: f64,
    }

    /// The calling thread's cycle count, where the system keeps one.
    fn cycles() -> Option<u64> {
        #[cfg(windows)]
        {
            let mut n = 0u64;
            // SAFETY: the calling thread's own pseudo-handle is always valid and needs no
            // closing; `n` is a valid out-pointer for the call.
            let ok = unsafe { crate::kernel32::QueryThreadCycleTime(crate::kernel32::GetCurrentThread(), &mut n) };
            (ok != 0).then_some(n)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    impl CpuClock {
        /// Measures how many cycles a millisecond of running is: three busy milliseconds on
        /// the calling thread, the fastest of them kept. A measure taken while the thread was
        /// switched out is too low, which makes later readings too high: an error towards
        /// "the thread ran", never towards "it waited". `None` where threads' cycles are not
        /// counted.
        pub fn calibrate() -> Option<CpuClock> {
            let mut best = 0.0f64;
            for _ in 0..3 {
                let (from, started) = (cycles()?, Instant::now());
                while started.elapsed() < Duration::from_millis(1) {
                    std::hint::spin_loop();
                }
                let (to, took) = (cycles()?, started.elapsed());
                best = best.max(to.saturating_sub(from) as f64 / (took.as_secs_f64() * 1e3));
            }
            (best > 0.0).then_some(CpuClock { cycles_per_ms: best })
        }

        /// The rate measured: the thread's processor cycles a millisecond of running.
        pub fn cycles_per_ms(&self) -> f64 {
            self.cycles_per_ms
        }

        /// The calling thread's processor time so far, in milliseconds.
        pub fn now_ms(&self) -> Option<f64> {
            cycles().map(|c| c as f64 / self.cycles_per_ms)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Running counts and waiting does not. (Bounds are loose on purpose: the thread may
        /// be switched out during the busy part, never switched in during the sleep.)
        #[test]
        fn a_threads_own_time_counts_running_and_not_waiting() {
            std::thread::spawn(|| {
                let Some(clock) = CpuClock::calibrate() else {
                    eprintln!("skipped: threads' processor cycles are not counted on this platform");
                    return;
                };
                let at = |c: &CpuClock| c.now_ms().expect("counted a moment ago");
                let before = at(&clock);
                let started = Instant::now();
                while started.elapsed() < Duration::from_millis(20) {
                    std::hint::spin_loop();
                }
                let ran = at(&clock) - before;
                assert!(ran > 5.0 && ran < 40.0, "20 ms of running read as {ran} ms");
                let before = at(&clock);
                std::thread::sleep(Duration::from_millis(40));
                let slept = at(&clock) - before;
                assert!(slept < 5.0, "40 ms asleep read as {slept} ms of running");
            })
            .join()
            .unwrap();
        }
    }
}

/// Whether a process with this id is currently running. Conservative: on platforms without an
/// implementation, or when the answer can't be determined (e.g. access denied), returns `true`.
///
/// A process is running **until it has ended**: until the system has closed what it held (its
/// files, its ports, its window) and signalled its process object. Its exit code is set
/// earlier, when it starts to end, and read alone it said "gone" of a process still on its way
/// out: the launcher's `specula stop` answered while the engine it stopped was still listed and
/// still held its port and its GPU (seen in its live check, 2026-11-14). An exit code of 259
/// (`STILL_ACTIVE`) also read as "running" for ever.
pub fn process_is_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        const SYNCHRONIZE: u32 = 0x0010_0000;
        const WAIT_OBJECT_0: u32 = 0;
        const ERROR_INVALID_PARAMETER: u32 = 87;
        // SAFETY: plain call; a null result is handled below.
        let handle = unsafe { kernel32::OpenProcess(SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            // ERROR_INVALID_PARAMETER means no such process; anything else (access denied) is unknown.
            // SAFETY: reads the calling thread's last-error value; no preconditions.
            return unsafe { kernel32::GetLastError() } != ERROR_INVALID_PARAMETER;
        }
        // SAFETY: `handle` is a live process handle with the right to wait on it; a wait of
        // 0 ms only asks whether it is signalled. The handle is closed exactly once.
        unsafe {
            let state = kernel32::WaitForSingleObject(handle, 0);
            kernel32::CloseHandle(handle);
            // (Signalled: ended. Not yet, or the wait failed: running, or not known.)
            state != WAIT_OBJECT_0
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

    /// `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR`: the loaded DLL's own folder (dependencies of a library
    /// named by its full path).
    const SEARCH_DLL_LOAD_DIR: u32 = 0x0000_0100;
    /// `LOAD_LIBRARY_SEARCH_APPLICATION_DIR`: the executable's folder.
    const SEARCH_APPLICATION_DIR: u32 = 0x0000_0200;
    /// `LOAD_LIBRARY_SEARCH_SYSTEM32`: `%windir%\System32` only.
    const SEARCH_SYSTEM32: u32 = 0x0000_0800;

    /// Whether `name` is a bare file name (no folder, drive or stream part).
    fn bare(name: &str) -> bool {
        !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != ".."
    }

    impl Library {
        /// Loads a **system** library by its file name (`"vulkan-1.dll"`, `"xinput1_4.dll"`,
        /// `"bcrypt.dll"`) from `System32` only (`LoadLibraryExW(LOAD_LIBRARY_SEARCH_SYSTEM32)`):
        /// a DLL of that name planted beside the executable, in the working folder or on `PATH`
        /// is never loaded (docs/design/SECURITY.md). A DLL that ships with the game uses
        /// [`Library::open_beside_exe`]; one the engine wrote itself, [`Library::open_path`].
        pub fn open(name: &str) -> Result<Library, LoadError> {
            if !bare(name) {
                return Err(LoadError {
                    library: name.into(),
                    message: "a system library is named by its file name only (vulkan-1.dll); a path the engine made loads with open_path".into(),
                    os_code: None,
                });
            }
            Self::load(name, name, SEARCH_SYSTEM32)
        }

        /// Loads a library that ships **beside the executable** (`"steam_api64.dll"`, an optional
        /// extra): the executable's folder, then `System32`, never the working folder or `PATH`.
        /// Only for libraries the game is distributed with; system libraries use [`Library::open`].
        pub fn open_beside_exe(name: &str) -> Result<Library, LoadError> {
            if !bare(name) {
                return Err(LoadError { library: name.into(), message: "a library beside the executable is named by its file name only".into(), os_code: None });
            }
            Self::load(name, name, SEARCH_APPLICATION_DIR | SEARCH_SYSTEM32)
        }

        /// Loads the library at `path`, a file the engine itself wrote (a game module's copy); the
        /// path is made absolute, and the library's own dependencies come from its folder and
        /// `System32` only.
        pub fn open_path(path: &std::path::Path) -> Result<Library, LoadError> {
            let shown = path.display().to_string();
            let full = std::path::absolute(path).map_err(|e| LoadError { library: shown.clone(), message: format!("cannot make the path absolute: {e}"), os_code: None })?;
            let Some(s) = full.to_str() else {
                return Err(LoadError { library: shown, message: "the library's path is not UTF-8".into(), os_code: None });
            };
            Self::load(s, &shown, SEARCH_DLL_LOAD_DIR | SEARCH_SYSTEM32)
        }

        fn load(spec: &str, name: &str, flags: u32) -> Result<Library, LoadError> {
            #[cfg(windows)]
            {
                let wide = crate::wide(spec);
                // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call; the
                // reserved file handle is null as required; `flags` are documented search flags.
                let handle = unsafe { crate::kernel32::LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), flags) };
                if handle.is_null() {
                    // SAFETY: reads the calling thread's last-error value; no preconditions.
                    let code = unsafe { crate::kernel32::GetLastError() };
                    return Err(LoadError { library: name.into(), message: "LoadLibraryExW failed".into(), os_code: Some(code) });
                }
                Ok(Library { handle, name: name.into() })
            }
            #[cfg(not(windows))]
            {
                let _ = (spec, flags);
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

        /// The file the OS loaded (`None` off Windows, or when it can't say).
        pub fn path(&self) -> Option<std::path::PathBuf> {
            #[cfg(windows)]
            {
                let mut buf = vec![0u16; 1024];
                // SAFETY: `handle` is a live module handle owned by `self`; `buf` holds `len` u16s.
                let n = unsafe { crate::kernel32::GetModuleFileNameW(self.handle, buf.as_mut_ptr(), buf.len() as u32) } as usize;
                (n > 0 && n < buf.len()).then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n])))
            }
            #[cfg(not(windows))]
            {
                None
            }
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
            // SAFETY: `handle` came from LoadLibraryExW and is released exactly once here.
            unsafe {
                crate::kernel32::FreeLibrary(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// FIPS 180-2's test vectors, through the OS.
    #[test]
    fn sha256_matches_the_standard_vectors() {
        match super::digest::sha256(b"abc") {
            Ok(d) => {
                assert_eq!(super::digest::hex(&d), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
                assert_eq!(super::digest::hex(&super::digest::sha256(b"").unwrap()), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
                let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
                assert_eq!(super::digest::hex(&super::digest::sha256(long).unwrap()), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
            }
            // No OS digest wired on this platform yet: callers say so (`unsupported`).
            Err(e) if e.code == "unsupported" && !cfg!(windows) => {}
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn the_listener_table_sees_a_loopback_listener() {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        match super::net::tcp_listening_ports() {
            Ok(ports) => assert!(ports.contains(&port), "port {port} not among {} listeners", ports.len()),
            Err(e) if e.code == "unsupported" => eprintln!("skipped: {e}"),
            Err(e) => panic!("{e}"),
        }
        drop(l);
    }

    #[test]
    fn power_status_answers_or_says_unknown() {
        // Any answer is fine (machines differ); it must not crash, and battery-less machines are
        // never reported as on battery.
        if let Some(p) = super::power() {
            assert!(p.has_battery || p.on_ac != Some(false), "{p:?}");
        }
    }

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
        // A process that ended with exit code 259 (which reads as "still active") has ended.
        let mut odd = std::process::Command::new("cmd").args(["/C", "exit 259"]).spawn().unwrap();
        let pid = odd.id();
        assert_eq!(odd.wait().unwrap().code(), Some(259));
        assert!(!process_is_running(pid), "its process object is signalled: it has ended, whatever its exit code");
        drop(odd);
        // One that runs is running, for as long as it runs (its exit code is not asked).
        let mut waiting = std::process::Command::new("cmd").args(["/C", "pause"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
        assert!(process_is_running(waiting.id()));
        drop(waiting.stdin.take());
        waiting.wait().unwrap();
        assert!(!process_is_running(waiting.id()));
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
        assert!(waited >= std::time::Duration::from_millis(90) && waited < std::time::Duration::from_secs(5), "{waited:?}");
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
        // A system library is named by its file name only.
        assert!(dynlib::Library::open("C:\\Windows\\System32\\kernel32.dll").is_err());
        assert!(dynlib::Library::open("..\\kernel32.dll").is_err());
    }

    /// A DLL planted beside the executable under a System32 name is never loaded for it
    /// (docs/design/SECURITY.md): `open` takes System32's, while the application-folder search
    /// (`open_beside_exe`) would have found the plant, so the test proves something.
    #[cfg(windows)]
    #[test]
    fn a_planted_dll_beside_the_exe_is_not_loaded_for_a_system_name() {
        // A System32 DLL this test process doesn't load otherwise (a module already loaded is
        // found by name before any search).
        const NAME: &str = "dciman32.dll";
        let system = dynlib::Library::open(NAME).expect("a System32 DLL");
        let sys_path = system.path().expect("its path");
        drop(system);
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let plant = exe_dir.join(NAME);
        // (The test executable's folder is the build's own: always writable.)
        std::fs::copy(&sys_path, &plant).expect("a copy planted beside the test executable");
        let beside = dynlib::Library::open_beside_exe(NAME).expect("the plant loads beside the exe");
        let beside_path = beside.path().unwrap();
        drop(beside);
        let loaded = dynlib::Library::open(NAME).expect("System32's");
        let loaded_path = loaded.path().unwrap();
        drop(loaded);
        let _ = std::fs::remove_file(&plant);
        let lower = |p: &std::path::Path| p.display().to_string().to_ascii_lowercase();
        assert_eq!(lower(&beside_path), lower(&plant), "the application-folder search finds the plant (so the test means something)");
        assert_eq!(lower(&loaded_path), lower(&sys_path), "open loads System32's, never the plant");
        assert!(lower(&loaded_path).contains("system32"), "{}", loaded_path.display());
    }
}
