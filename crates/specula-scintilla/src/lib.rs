//! Specula Scintilla (*scintilla*, "spark"): particles, effects and decals as data
//! (docs/design/VFX.md).
//!
//! - [`effect`]: the effect format (emitters, curves, flipbooks, collision, a light), parsed with
//!   structured errors ([`VfxError`]) and summarised analytically (reach, peak, end time).
//! - [`eval`]: every live particle of an instance at a time, in closed form: no state between
//!   frames, so the same world at the same time draws the same particles (captures repeat), and
//!   culled effects cost nothing.
//! - [`budget`]: the frame's particle budget, LOD and fill-rate cap, shared out deterministically.
//! - [`component`]: the world's `vfx` and `decal` components.
//! - [`library`]: the built-in effects; [`textures`]: the built-in (procedural) textures.
//! - [`check`]: `vfx_check`'s budget and readability checks; [`flash`]: photosensitivity (how
//!   often an effect flashes).
//!
//! **Cosmetic, never simulation:** nothing here writes the world. **Reproducible anyway (F2
//! style):** counter-based hashes, IEEE `+ − × ÷`, `sqrt` and `specula_core::dmath` only, so
//! captures repeat bit for bit on every platform (the crate carries the determinism lints).

#![forbid(unsafe_code)]

pub mod budget;
pub mod check;
pub mod component;
pub mod curve;
pub mod effect;
pub mod error;
pub mod eval;
pub mod flash;
pub mod library;
pub mod noise;
pub mod textures;

pub use component::{DecalMode, DecalSpec, Params, VfxSpec};
pub use effect::{Effect, Emitter, TextureRef};
pub use error::VfxError;
pub use eval::{evaluate, EmitterPlan, EvalStats, Instance, Particle, Placement, Track};
