//! Specula SDK: the one crate game modules depend on.
//!
//! A **module** is game logic in a Rust `cdylib` that the engine loads, runs every tick and
//! hot-reloads when it is rebuilt (`logic_build`), without losing the world: the engine owns all
//! state, and a module is a stateless set of functions behind a versioned C ABI ([`abi`]).
//!
//! ```text
//! #![deny(unsafe_code)] // not forbid: export_module! allows unsafe code for its one export
//!
//! use specula_sdk::{Cx, Module, ModuleInfo, Patch, SdkError};
//!
//! #[derive(Default)]
//! pub struct Spinner;
//!
//! impl Module for Spinner {
//!     fn describe(&self) -> ModuleInfo {
//!         ModuleInfo { name: "spinner", version: env!("CARGO_PKG_VERSION"), ..ModuleInfo::default() }
//!     }
//!     fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
//!         if let Some(id) = cx.find("/spinner")? {
//!             let deg = (cx.tick() % 360) as f32;
//!             cx.set(id, Patch::new().rotation_deg(specula_sdk::Vec3::new(0.0, deg, 0.0)))?;
//!         }
//!         Ok(())
//!     }
//! }
//!
//! specula_sdk::export_module!(Spinner);
//! ```
//!
//! **Deterministic by construction (foundation F2).** The SDK gives a module only the tick
//! number (no wall clock), the engine's seeded RNG streams, the world, the tick's input
//! *actions* (never raw keys) and events, all in a fixed order, and its maps are ordered
//! ([`std::collections::BTreeMap`]). Use [`dmath`] for trigonometry and exponentials, never
//! `f32::sin` and friends. Don't spawn threads or read the clock: nothing in the SDK needs them.
//!
//! **Module crates** depend on this crate only (by path; registry crates, build scripts and
//! proc-macros are refused by `logic_build`), are `crate-type = ["cdylib"]`, and say
//! `#![deny(unsafe_code)]`, not `forbid`: [`export_module!`] expands to one exported function
//! marked `#[allow(unsafe_code)]` (`#[no_mangle]` counts as unsafe code), which `forbid` would
//! reject (E0453) and `deny` lets through. Module code runs with the engine's privileges.
//!
//! See `docs/design/MODULES.md` for the lifecycle, the ABI and the build loop.

// The SDK catches a module's panics at the C ABI boundary and reports them as data; with
// `panic = "abort"` the first panic (even the SDK's own, ending a runaway call) would abort the
// whole engine. `logic_build` passes `--config profile.<dev|release>.panic="unwind"` as well.
#[cfg(not(panic = "unwind"))]
compile_error!(
    "specula modules must be built with panic = \"unwind\": a panic in a module built with \
     panic = \"abort\" would kill the engine (remove panic = \"abort\" from the profile)"
);

pub mod abi;
mod module;
pub mod value;

pub use module::{Action, Cx, Entity, Event, HookDef, Id, Module, ModuleInfo, Moved, Patch, SdkError, Spawn, StopReason, TraceLevel};
pub use specula_core::dmath;
pub use specula_core::math::{Quat, Vec3};
pub use value::Value;

/// `[a-z][a-z0-9_]{0,max-1}`: module and hook names.
pub fn valid_name(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= max
        && b[0].is_ascii_lowercase()
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("ambulo", 32));
        assert!(valid_name("a_2", 32));
        for bad in ["", "Ambulo", "2a", "a-b", "a b", "ünï"] {
            assert!(!valid_name(bad, 32), "{bad}");
        }
        assert!(!valid_name(&"a".repeat(33), 32));
    }
}
