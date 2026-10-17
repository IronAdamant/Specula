//! Loading module libraries and calling them through the SDK's C ABI.
//!
//! [`ModuleCode::open`] copies the library to a versioned name and loads the copy, so Cargo can
//! rebuild the original while the module runs (Windows locks a loaded DLL's file). It checks the
//! ABI table before calling anything. [`ModuleCode::of`] wraps a module type linked into the
//! engine instead (tests, and later shipping builds on platforms without dynamic loading, F6):
//! both go through the same `call` function pointer and byte messages.

use specula_sdk::abi::{self, status, HostApi, ModuleApi, Slice};
use specula_sdk::{Module, Value};
use specula_sys::dynlib::Library;
use std::ffi::c_void;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type CallFn = unsafe extern "C" fn(*const HostApi, u32, *const u8, usize) -> u32;
type EntryFn = extern "C" fn() -> *const ModuleApi;

/// Most bytes a module may send in one reply (an answer to a hook, an error): beyond it the call
/// fails rather than growing without bound (replies reach agents, and tokens are a cost).
pub const MAX_REPLY_BYTES: usize = 256 << 10;

/// Why a library couldn't become a module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// `missing`, `copy_failed`, `load_failed`, `no_entry`, `bad_table`, `abi_mismatch`,
    /// `unsupported`.
    pub code: &'static str,
    pub message: String,
    pub path: PathBuf,
    /// OS error (`GetLastError`) for `load_failed`.
    pub os_code: Option<u32>,
    /// The module's ABI version for `abi_mismatch`.
    pub module_abi: Option<u32>,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.path.display(), self.code, self.message)
    }
}

impl std::error::Error for LoadError {}

impl LoadError {
    fn new(code: &'static str, path: &Path, message: impl Into<String>) -> LoadError {
        LoadError { code, message: message.into(), path: path.to_owned(), os_code: None, module_abi: None }
    }
}

/// Answers a module's requests during a call (the engine's world, state store, RNG...).
pub trait Handler {
    /// `Ok(answer)` or `Err(error map {code, message, ...})`.
    fn request(&mut self, req: &Value) -> Result<Value, Value>;
}

/// How a call went.
#[derive(Clone, Debug, PartialEq)]
pub struct CallOutcome {
    /// [`specula_sdk::abi::status`].
    pub status: u32,
    /// The module's reply: the result, or an error map `{code, message, location?}`.
    pub reply: Value,
    /// Requests the module made.
    pub requests: u64,
}

impl CallOutcome {
    pub fn ok(&self) -> bool {
        self.status == status::OK
    }
}

/// A module's code: a loaded library (or a linked-in module type) and its ABI table.
pub struct ModuleCode {
    call: CallFn,
    sdk_version: String,
    /// Hash of the library's bytes (the build hash journaled with each load, F2); for a linked-in
    /// module, a hash of its type name.
    build_hash: u64,
    bytes: u64,
    /// The library loaded from (or `static:<type>`).
    source: PathBuf,
    /// The versioned copy actually loaded; removed after unloading.
    copy: Option<PathBuf>,
    /// Keeps the code mapped while `call` may run; dropped before the copy is deleted.
    lib: Option<Library>,
}

impl fmt::Debug for ModuleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleCode").field("source", &self.source).field("build_hash", &self.build_hash).finish()
    }
}

static COPIES: AtomicU64 = AtomicU64::new(0);

impl ModuleCode {
    /// Loads the module library at `path`: copies it into `copies_dir` under a name no other
    /// load uses (`<stem>-<pid>-<n>.<ext>`), loads the copy and checks its ABI table. Copies
    /// left by engines that are no longer running are deleted on the way.
    pub fn open(path: &Path, copies_dir: &Path) -> Result<ModuleCode, LoadError> {
        let bytes = std::fs::read(path).map_err(|e| LoadError::new("missing", path, format!("cannot read the library: {e}")))?;
        if !cfg!(windows) {
            return Err(LoadError::new("unsupported", path, "loading module libraries is not implemented on this platform yet"));
        }
        std::fs::create_dir_all(copies_dir)
            .map_err(|e| LoadError::new("copy_failed", copies_dir, format!("cannot create the copies folder: {e}")))?;
        sweep_stale_copies(copies_dir);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("module");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("dll");
        let n = COPIES.fetch_add(1, Ordering::Relaxed);
        let copy = copies_dir.join(format!("{stem}-{}-{n}.{ext}", std::process::id()));
        std::fs::write(&copy, &bytes).map_err(|e| LoadError::new("copy_failed", &copy, format!("cannot write the copy: {e}")))?;
        let build_hash = specula_core::hash::hash_bytes(&bytes);
        let fail = |e: LoadError| {
            let _ = std::fs::remove_file(&copy);
            e
        };
        let copy_str = copy.to_str().ok_or_else(|| fail(LoadError::new("copy_failed", &copy, "the copy's path is not UTF-8")))?;
        let lib = Library::open(copy_str).map_err(|e| {
            fail(LoadError { os_code: e.os_code, ..LoadError::new("load_failed", path, format!("the OS refused the library: {}", e.message)) })
        })?;
        let Some(sym) = lib.symbol(abi::ENTRY_SYMBOL) else {
            drop(lib);
            return Err(fail(LoadError::new(
                "no_entry",
                path,
                format!("the library exports no '{}': call specula_sdk::export_module!(YourModule) in it", abi::ENTRY_SYMBOL),
            )));
        };
        // SAFETY: the symbol is the SDK's `extern "C" fn() -> *const ModuleApi` (its name is
        // reserved for that by the ABI); a library exporting it with another signature breaks the
        // ABI contract, which the version check below can't detect.
        let entry: EntryFn = unsafe { std::mem::transmute::<*mut c_void, EntryFn>(sym) };
        let table = entry();
        if table.is_null() {
            drop(lib);
            return Err(fail(LoadError::new("bad_table", path, "the entry returned no ABI table")));
        }
        // SAFETY: the entry returns a pointer to a static `ModuleApi` in the library, valid while
        // it stays loaded; only the first two `u32`s are read before the version is checked.
        let (version, size) = unsafe { ((*table).abi_version, (*table).api_size) };
        if version != abi::ABI_VERSION {
            drop(lib);
            return Err(fail(LoadError {
                module_abi: Some(version),
                ..LoadError::new(
                    "abi_mismatch",
                    path,
                    format!(
                        "the module was built against module ABI {version}, this engine speaks {}: rebuild it with this engine's specula-sdk",
                        abi::ABI_VERSION
                    ),
                )
            }));
        }
        if (size as usize) < std::mem::size_of::<ModuleApi>() {
            drop(lib);
            return Err(fail(LoadError::new("bad_table", path, format!("the ABI table is {size} bytes, expected at least {}", std::mem::size_of::<ModuleApi>()))));
        }
        // SAFETY: same table, version and size checked: it is a whole `ModuleApi`.
        let (call, sdk) = unsafe { ((*table).call, (*table).sdk_version) };
        Ok(ModuleCode {
            call,
            sdk_version: abi::sdk_version_str(&sdk),
            build_hash,
            bytes: bytes.len() as u64,
            source: path.to_owned(),
            copy: Some(copy),
            lib: Some(lib),
        })
    }

    /// Module type `M` linked into this program (no library): same ABI, same messages.
    pub fn of<M: Module>() -> ModuleCode {
        let api = abi::module_api::<M>();
        let name = std::any::type_name::<M>();
        ModuleCode {
            call: api.call,
            sdk_version: abi::sdk_version_str(&api.sdk_version),
            build_hash: specula_core::hash::hash_str(name),
            bytes: 0,
            source: PathBuf::from(format!("static:{name}")),
            copy: None,
            lib: None,
        }
    }

    pub fn sdk_version(&self) -> &str {
        &self.sdk_version
    }

    pub fn build_hash(&self) -> u64 {
        self.build_hash
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Calls entry `entry` with `input`; the module's requests go to `handler`, at most
    /// `max_requests` of them (past that each is refused with `too_many_requests` and the call
    /// fails).
    pub fn call(&self, entry: u32, input: &Value, handler: &mut dyn Handler, max_requests: u64) -> CallOutcome {
        let bytes = if input.is_null() { Vec::new() } else { input.encode() };
        let mut cx = CallCtx { handler, answer: Vec::new(), reply: None, requests: 0, max_requests, overflowed: false, bad_reply: None };
        let host = HostApi {
            abi_version: abi::ABI_VERSION,
            api_size: std::mem::size_of::<HostApi>() as u32,
            ctx: (&mut cx as *mut CallCtx<'_>).cast::<c_void>(),
            request: host_request,
            reply: host_reply,
        };
        // SAFETY: `call` came from a checked ABI table (or a linked-in module) whose library
        // stays loaded as long as `self`; `host` and its context `cx` live until it returns, and
        // `bytes` is our own buffer.
        let status = unsafe { (self.call)(&host, entry, bytes.as_ptr(), bytes.len()) };
        let reply = match (cx.bad_reply.take(), cx.reply.take()) {
            (Some(e), _) => return CallOutcome { status: status::ERROR, reply: e, requests: cx.requests },
            (None, Some(r)) => Value::decode(&r).unwrap_or_else(|e| error("bad_reply", &format!("the module's reply was not a value: {e}"))),
            (None, None) => Value::Null,
        };
        if cx.overflowed {
            let e = error("too_many_requests", &format!("the module made more than {max_requests} requests in one call"));
            return CallOutcome { status: status::ERROR, reply: e, requests: cx.requests };
        }
        CallOutcome { status, reply, requests: cx.requests }
    }
}

impl Drop for ModuleCode {
    fn drop(&mut self) {
        // Unload first: Windows won't delete a loaded library's file.
        self.lib = None;
        if let Some(copy) = self.copy.take() {
            let _ = std::fs::remove_file(copy);
        }
    }
}

/// An error map `{code, message}`.
pub fn error(code: &str, message: &str) -> Value {
    Value::map().with("code", code).with("message", message)
}

/// Deletes copies whose engine (the pid in the name) is no longer running.
fn sweep_stale_copies(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let me = std::process::id();
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(stem) = Path::new(&name).file_stem().and_then(|s| s.to_str()) else { continue };
        let mut parts = stem.rsplitn(3, '-');
        let (_n, pid) = (parts.next(), parts.next().and_then(|p| p.parse::<u32>().ok()));
        if let Some(pid) = pid {
            if pid != me && !specula_sys::process_is_running(pid) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

struct CallCtx<'a> {
    handler: &'a mut dyn Handler,
    /// The last answer, which the module reads through the slice it was given.
    answer: Vec<u8>,
    reply: Option<Vec<u8>>,
    requests: u64,
    max_requests: u64,
    overflowed: bool,
    bad_reply: Option<Value>,
}

unsafe extern "C" fn host_request(ctx: *mut c_void, req: *const u8, req_len: usize, out: *mut Slice) -> u32 {
    if ctx.is_null() || out.is_null() || (req.is_null() && req_len > 0) {
        return status::BAD_INPUT;
    }
    // SAFETY: `ctx` is the `CallCtx` that `ModuleCode::call` created for this call; it is alive
    // until the call returns, the module calls back only on the calling thread, and `call` does
    // not touch it while the module runs.
    let cx = unsafe { &mut *ctx.cast::<CallCtx<'_>>() };
    let bytes: &[u8] = if req_len == 0 {
        &[]
    } else {
        // SAFETY: the module passes `req_len` readable bytes at `req` for the duration of this call.
        unsafe { std::slice::from_raw_parts(req, req_len) }
    };
    cx.requests += 1;
    let (status, answer) = if cx.requests > cx.max_requests {
        cx.overflowed = true;
        (status::ERROR, error("too_many_requests", "this call made too many requests; it will fail"))
    } else {
        match Value::decode(bytes) {
            Err(e) => (status::BAD_INPUT, error("bad_request", &e.to_string())),
            Ok(req) => {
                // A bug in the engine's handler must not unwind into the module.
                match catch_unwind(AssertUnwindSafe(|| cx.handler.request(&req))) {
                    Ok(Ok(v)) => (status::OK, v),
                    Ok(Err(e)) => (status::ERROR, e),
                    Err(_) => (status::ERROR, error("engine_panic", "the engine failed answering this request (an engine bug)")),
                }
            }
        }
    };
    cx.answer.clear();
    answer.encode_into(&mut cx.answer);
    // SAFETY: `out` is the module's writable `Slice`; it points into `cx.answer`, which stays
    // unchanged until the module's next request or the end of the call.
    unsafe { *out = Slice { ptr: cx.answer.as_ptr(), len: cx.answer.len() } };
    status
}

unsafe extern "C" fn host_reply(ctx: *mut c_void, bytes: *const u8, len: usize) {
    if ctx.is_null() || (bytes.is_null() && len > 0) {
        return;
    }
    // SAFETY: as in `host_request`.
    let cx = unsafe { &mut *ctx.cast::<CallCtx<'_>>() };
    if len > MAX_REPLY_BYTES {
        cx.bad_reply = Some(error("reply_too_big", &format!("the module replied {len} bytes; the limit is {MAX_REPLY_BYTES}")));
        return;
    }
    let data: &[u8] = if len == 0 {
        &[]
    } else {
        // SAFETY: the module passes `len` readable bytes at `bytes` for the duration of this call.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    cx.reply = Some(data.to_vec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_sdk::{Cx, HookDef, ModuleInfo, SdkError};

    #[derive(Default)]
    struct Probe;

    impl Module for Probe {
        fn describe(&self) -> ModuleInfo {
            ModuleInfo { name: "probe", version: "1", hooks: vec![HookDef { name: "echo", doc: "echoes", args: "" }], ..ModuleInfo::default() }
        }
        fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
            if cx.tick() == 13 {
                panic!("unlucky tick {}", cx.tick());
            }
            if cx.tick() == 99 {
                loop {
                    cx.log("spam")?;
                }
            }
            let id = cx.find("/thing")?;
            cx.set_state("found", id.map(|i| i.0))?;
            Ok(())
        }
        fn hook(&self, cx: &mut Cx, name: &str, args: &Value) -> Result<Value, SdkError> {
            match name {
                "echo" => Ok(args.clone().with("tick", cx.tick())),
                _ => Err(SdkError::new("unknown_hook", name)),
            }
        }
    }

    #[derive(Default)]
    struct Recorder {
        seen: Vec<Value>,
    }

    impl Handler for Recorder {
        fn request(&mut self, req: &Value) -> Result<Value, Value> {
            self.seen.push(req.clone());
            match req.get("op").and_then(Value::as_str) {
                Some("find") => Ok(Value::Int(42)),
                Some("state_set") | Some("log") => Ok(Value::Null),
                _ => Err(error("unknown_op", "no")),
            }
        }
    }

    #[test]
    fn linked_in_module_round_trip() {
        let code = ModuleCode::of::<Probe>();
        assert_eq!(code.sdk_version(), env!("CARGO_PKG_VERSION"));
        let mut h = Recorder::default();
        let info = code.call(abi::entry::DESCRIBE, &Value::Null, &mut h, 100);
        assert!(info.ok(), "{info:?}");
        assert_eq!(info.reply.get("name").and_then(Value::as_str), Some("probe"));

        let out = code.call(abi::entry::TICK, &Value::map().with("tick", 1).with("tick_hz", 30), &mut h, 100);
        assert!(out.ok(), "{out:?}");
        assert_eq!(out.requests, 2);
        assert_eq!(h.seen[1].get("value"), Some(&Value::Int(42)));

        let hook = code.call(abi::entry::HOOK, &Value::map().with("tick", 5).with("hook", "echo").with("args", Value::map().with("a", 1)), &mut h, 100);
        assert_eq!(hook.reply, Value::map().with("a", 1).with("tick", 5));
        let unknown = code.call(77, &Value::Null, &mut h, 100);
        assert_eq!(unknown.status, status::UNKNOWN_ENTRY);
    }

    #[test]
    fn panics_become_structured_errors() {
        let code = ModuleCode::of::<Probe>();
        let mut h = Recorder::default();
        let out = code.call(abi::entry::TICK, &Value::map().with("tick", 13), &mut h, 100);
        assert_eq!(out.status, status::PANIC);
        assert_eq!(out.reply.get("code").and_then(Value::as_str), Some("panic"));
        assert_eq!(out.reply.get("message").and_then(Value::as_str), Some("unlucky tick 13"));
        let loc = out.reply.get("location").and_then(Value::as_str).unwrap();
        assert!(loc.contains("loader.rs"), "{loc}");
        // And the next call works.
        assert!(code.call(abi::entry::TICK, &Value::map().with("tick", 14), &mut h, 100).ok());
    }

    #[test]
    fn runaway_requests_are_bounded() {
        let code = ModuleCode::of::<Probe>();
        let mut h = Recorder::default();
        let out = code.call(abi::entry::TICK, &Value::map().with("tick", 99), &mut h, 50);
        assert_eq!(out.status, status::ERROR);
        assert_eq!(out.reply.get("code").and_then(Value::as_str), Some("too_many_requests"));
        assert_eq!(out.requests, 51);
    }

    #[test]
    fn missing_or_bogus_libraries_are_structured_errors() {
        let dir = std::env::temp_dir().join(format!("specula-logic-test-{}", std::process::id()));
        let e = ModuleCode::open(&dir.join("nope.dll"), &dir).unwrap_err();
        assert_eq!(e.code, "missing");
        // Not a library at all: refused by the OS, and its copy is cleaned up.
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.dll");
        std::fs::write(&fake, b"not a library").unwrap();
        let e = ModuleCode::open(&fake, &dir.join("copies")).unwrap_err();
        assert_eq!(e.code, if cfg!(windows) { "load_failed" } else { "unsupported" }, "{e}");
        let left = std::fs::read_dir(dir.join("copies")).map(|d| d.count()).unwrap_or(0);
        assert_eq!(left, 0, "the copy was removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_copies_of_dead_engines_are_swept() {
        let dir = std::env::temp_dir().join(format!("specula-logic-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Pid 0xFFFFFFF0 is never a running process; our own pid is.
        let dead = dir.join("m-4294967280-0.dll");
        let live = dir.join(format!("m-{}-0.dll", std::process::id()));
        std::fs::write(&dead, b"x").unwrap();
        std::fs::write(&live, b"x").unwrap();
        sweep_stale_copies(&dir);
        assert_eq!(dead.exists(), !cfg!(windows), "only Windows can tell a process is gone");
        assert!(live.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
