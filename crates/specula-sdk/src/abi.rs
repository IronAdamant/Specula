//! The C ABI between the engine and a module: two small `#[repr(C)]` tables and byte messages.
//!
//! A module library exports one symbol, [`ENTRY_SYMBOL`], returning a pointer to its static
//! [`ModuleApi`]. The engine checks [`ModuleApi::abi_version`] and [`ModuleApi::api_size`] before
//! anything else, then talks to the module only through [`ModuleApi::call`]:
//!
//! - **engine → module:** `call(host, entry, input)` with an [`entry`] code and an encoded
//!   [`Value`] (tick input, hook arguments...). The module answers once through
//!   [`HostApi::reply`] (an encoded value: the result, or an error) and returns a [`status`].
//! - **module → engine:** during a call, [`HostApi::request`] with an encoded request
//!   (`{"op": "spawn", ...}`); the answer is written to an [`Slice`] in engine memory, valid
//!   until the module's next request or the end of the call.
//!
//! Neither side ever frees the other's memory, keeps the other's pointers past a call, or lets
//! a panic cross the boundary (both catch them). The SDK generates the module side
//! ([`crate::export_module!`] and [`call`]); `specula-logic` is the engine side.
//!
//! Versioning: any change to these tables or to the meaning of an entry, status or request
//! bumps [`ABI_VERSION`]; the engine refuses a module built against another version (it says
//! which, so the agent rebuilds). New request ops and new keys in messages are not breaking:
//! both sides ignore keys they don't know, and an unknown op is an error answer, not a crash.

use crate::module::{Cx, Module, SdkError, StopReason};
use crate::value::Value;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, Once};

/// Version of this ABI (tables, entries, statuses, message meanings).
pub const ABI_VERSION: u32 = 1;

/// The one symbol a module library exports: `extern "C" fn() -> *const ModuleApi`.
pub const ENTRY_SYMBOL: &str = "specula_module_entry";

/// What [`ModuleApi::call`] is asked to do.
pub mod entry {
    /// Answer [`crate::ModuleInfo`] (no world access).
    pub const DESCRIBE: u32 = 1;
    /// Loaded (first time or after a reload): input `{tick, tick_hz, seed, reloaded, generation}`.
    pub const START: u32 = 2;
    /// One simulation tick: input `{tick, tick_hz, actions, events, ...}`.
    pub const TICK: u32 = 3;
    /// A named native hook (Lex escape hatch): input `{tick, tick_hz, hook, args, reason, caller}`.
    pub const HOOK: u32 = 4;
    /// About to be replaced or unloaded: input `{tick, tick_hz, reason}`.
    pub const STOP: u32 = 5;
}

/// What [`ModuleApi::call`] and [`HostApi::request`] return.
pub mod status {
    /// Done; the reply (or answer) is the result.
    pub const OK: u32 = 0;
    /// Refused or failed; the reply is an error map `{code, message, ...}`.
    pub const ERROR: u32 = 1;
    /// The module panicked; the reply is `{code: "panic", message, location}`.
    pub const PANIC: u32 = 2;
    /// The other side's arguments were malformed (null table, wrong version, undecodable bytes).
    pub const BAD_INPUT: u32 = 3;
    /// The entry code isn't known to this module.
    pub const UNKNOWN_ENTRY: u32 = 4;
}

/// A borrowed byte range in the other side's memory.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Slice {
    pub ptr: *const u8,
    pub len: usize,
}

impl Slice {
    pub const EMPTY: Slice = Slice { ptr: std::ptr::null(), len: 0 };
}

/// The engine's side, passed to every [`ModuleApi::call`] (valid for that call only).
#[repr(C)]
pub struct HostApi {
    pub abi_version: u32,
    /// `size_of::<HostApi>()` on the engine side.
    pub api_size: u32,
    /// Opaque engine state for this call.
    pub ctx: *mut c_void,
    /// A request (encoded value); the answer is written to `*out`. Returns a [`status`].
    /// Safety: only during the call this table was passed to, with `ctx` from it, `req_len`
    /// readable bytes at `req` and a writable `out`.
    pub request: unsafe extern "C" fn(ctx: *mut c_void, req: *const u8, req_len: usize, out: *mut Slice) -> u32,
    /// The call's result or error (encoded value). At most once per call; later replies replace
    /// it. Safety: as for `request`, with `len` readable bytes at `bytes`.
    pub reply: unsafe extern "C" fn(ctx: *mut c_void, bytes: *const u8, len: usize),
}

/// The module's side: a static table the entry symbol points to.
#[repr(C)]
pub struct ModuleApi {
    pub abi_version: u32,
    /// `size_of::<ModuleApi>()` on the module side.
    pub api_size: u32,
    /// The SDK version the module was built with (`CARGO_PKG_VERSION`, NUL-padded ASCII).
    pub sdk_version: [u8; 16],
    /// See the module docs. Never unwinds. Safety: `host` points to a valid [`HostApi`] whose
    /// functions may be called until this returns, and `input_len` bytes at `input` are
    /// readable for that long.
    pub call: unsafe extern "C" fn(host: *const HostApi, entry: u32, input: *const u8, input_len: usize) -> u32,
}

/// The table for module type `M` (what [`crate::export_module!`] exports).
pub const fn module_api<M: Module>() -> ModuleApi {
    ModuleApi {
        abi_version: ABI_VERSION,
        api_size: std::mem::size_of::<ModuleApi>() as u32,
        sdk_version: sdk_version_bytes(),
        call: call::<M>,
    }
}

const fn sdk_version_bytes() -> [u8; 16] {
    let v = env!("CARGO_PKG_VERSION").as_bytes();
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < v.len() && i < 16 {
        out[i] = v[i];
        i += 1;
    }
    out
}

/// The SDK version in a [`ModuleApi::sdk_version`] field.
pub fn sdk_version_str(bytes: &[u8; 16]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(16);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// The last panic seen by [`call`] on this side: `(thread, message, location)`.
static LAST_PANIC: Mutex<Option<(std::thread::ThreadId, String, String)>> = Mutex::new(None);
static HOOK: Once = Once::new();

/// Records panic locations for structured errors, then defers to the previous hook (which
/// prints as usual). Installed once per copy of std: a module library has its own.
fn install_panic_hook() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
            let message = panic_message(info.payload());
            if let Ok(mut last) = LAST_PANIC.lock() {
                *last = Some((std::thread::current().id(), message, location));
            }
            previous(info);
        }));
    });
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic with a non-string payload".to_owned()
    }
}

/// The module side of [`ModuleApi::call`] for module type `M`.
///
/// # Safety
/// As [`ModuleApi::call`] says: a valid `host` table for the whole call and `input_len`
/// readable bytes at `input`.
pub unsafe extern "C" fn call<M: Module>(host: *const HostApi, entry: u32, input: *const u8, input_len: usize) -> u32 {
    if host.is_null() {
        return status::BAD_INPUT;
    }
    // SAFETY: the engine passes a pointer to a live `HostApi` that stays valid and unaliased by
    // writes for this whole call (the ABI contract above); we only read it.
    let host = unsafe { &*host };
    if host.abi_version != ABI_VERSION || (host.api_size as usize) < std::mem::size_of::<HostApi>() {
        return status::BAD_INPUT;
    }
    let input: &[u8] = if input_len == 0 {
        &[]
    } else if input.is_null() {
        return status::BAD_INPUT;
    } else {
        // SAFETY: the engine passes `input_len` readable bytes at `input`, valid for this call.
        unsafe { std::slice::from_raw_parts(input, input_len) }
    };
    install_panic_hook();
    let outcome = catch_unwind(AssertUnwindSafe(|| dispatch::<M>(host, entry, input)));
    let (status, reply) = match outcome {
        Ok(Ok(v)) => (status::OK, v),
        Ok(Err((status, e))) => (status, e.to_value()),
        Err(payload) => {
            let mut message = panic_message(payload.as_ref());
            let mut location = String::new();
            let me = std::thread::current().id();
            if let Ok(mut last) = LAST_PANIC.lock() {
                if let Some((thread, m, l)) = last.take() {
                    if thread == me {
                        (message, location) = (m, l);
                    }
                }
            }
            // A payload whose drop panics would unwind out of here: keep it from dropping.
            std::mem::forget(payload);
            let v = Value::map().with("code", "panic").with("message", message).with("location", location);
            (status::PANIC, v)
        }
    };
    let bytes = reply.encode();
    // SAFETY: during the call, with the call's own context and our own live buffer.
    unsafe { (host.reply)(host.ctx, bytes.as_ptr(), bytes.len()) };
    status
}

type Outcome = Result<Value, (u32, SdkError)>;

fn dispatch<M: Module>(host: &HostApi, entry: u32, input: &[u8]) -> Outcome {
    let bad = |e: crate::value::DecodeError| (status::BAD_INPUT, SdkError::new("bad_input", e.to_string()));
    let input = if input.is_empty() { Value::Null } else { Value::decode(input).map_err(bad)? };
    let module = M::default();
    let err = |e: SdkError| (status::ERROR, e);
    match entry {
        entry::DESCRIBE => Ok(module.describe().to_value()),
        entry::START => {
            let mut cx = Cx::new(host, &input);
            module.start(&mut cx).map_err(err)?;
            Ok(Value::Null)
        }
        entry::TICK => {
            let mut cx = Cx::new(host, &input);
            module.tick(&mut cx).map_err(err)?;
            Ok(Value::Null)
        }
        entry::HOOK => {
            let mut cx = Cx::new(host, &input);
            let name = input.get("hook").and_then(Value::as_str).unwrap_or_default().to_owned();
            let args = input.get("args").cloned().unwrap_or_default();
            module.hook(&mut cx, &name, &args).map_err(err)
        }
        entry::STOP => {
            let mut cx = Cx::new(host, &input);
            let reason = StopReason::parse(input.get("reason").and_then(Value::as_str).unwrap_or_default());
            module.stop(&mut cx, reason).map_err(err)?;
            Ok(Value::Null)
        }
        other => Err((status::UNKNOWN_ENTRY, SdkError::new("unknown_entry", format!("entry {other} is not known to this module")))),
    }
}

/// The module side of [`HostApi::request`]: sends `req`, decodes the answer.
pub(crate) fn request(host: &HostApi, req: &Value) -> Result<Value, SdkError> {
    let bytes = req.encode();
    let mut out = Slice::EMPTY;
    // SAFETY: only reached from inside `call` (a `Cx` lives no longer), with the call's own
    // context, our live request buffer and a writable `out`.
    let status = unsafe { (host.request)(host.ctx, bytes.as_ptr(), bytes.len(), &mut out) };
    let answer: &[u8] = if out.len == 0 || out.ptr.is_null() {
        &[]
    } else {
        // SAFETY: the engine wrote a slice of its own memory that stays valid until our next
        // request or the end of this call; it is copied out (decoded) right here.
        unsafe { std::slice::from_raw_parts(out.ptr, out.len) }
    };
    let value = if answer.is_empty() {
        Value::Null
    } else {
        Value::decode(answer).map_err(|e| SdkError::new("bad_answer", e.to_string()))?
    };
    match status {
        status::OK => Ok(value),
        _ => Err(SdkError::from_value(&value)),
    }
}

/// Exports module type `$ty` (which implements [`crate::Module`] and `Default`) from a
/// `cdylib`. Use it once per library:
///
/// ```text
/// #[derive(Default)]
/// pub struct Walker;
/// impl specula_sdk::Module for Walker { /* ... */ }
/// specula_sdk::export_module!(Walker);
/// ```
#[macro_export]
macro_rules! export_module {
    ($ty:ty) => {
        /// The module's ABI table (see `specula_sdk::abi`).
        #[allow(unsafe_code)]
        #[no_mangle]
        pub extern "C" fn specula_module_entry() -> *const $crate::abi::ModuleApi {
            static API: $crate::abi::ModuleApi = $crate::abi::module_api::<$ty>();
            &API
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    /// The tables' layout is the ABI: pin it (64-bit targets; 32-bit ones get their own row
    /// when one exists).
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn layout_is_pinned() {
        assert_eq!((size_of::<Slice>(), align_of::<Slice>()), (16, 8));
        assert_eq!((offset_of!(Slice, ptr), offset_of!(Slice, len)), (0, 8));
        assert_eq!(size_of::<HostApi>(), 32);
        assert_eq!(
            [offset_of!(HostApi, abi_version), offset_of!(HostApi, api_size), offset_of!(HostApi, ctx), offset_of!(HostApi, request), offset_of!(HostApi, reply)],
            [0, 4, 8, 16, 24]
        );
        assert_eq!(size_of::<ModuleApi>(), 32);
        assert_eq!(
            [offset_of!(ModuleApi, abi_version), offset_of!(ModuleApi, api_size), offset_of!(ModuleApi, sdk_version), offset_of!(ModuleApi, call)],
            [0, 4, 8, 24]
        );
    }

    #[test]
    fn sdk_version_round_trips() {
        assert_eq!(sdk_version_str(&sdk_version_bytes()), env!("CARGO_PKG_VERSION"));
    }
}
