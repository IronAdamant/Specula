//! Specula skeletal animation (docs/design/ANIMATION.md, stages A1–A2): data-first and
//! deterministic.
//!
//! - [`Skeleton`] from a recipe [`specula_model::Rig`]: parents-first bones, rest pose,
//!   inverse bind matrices; [`Skeleton::model_space`] and [`Skeleton::skin_matrices`].
//! - [`Clip`]: JSON tracks (translation / rotation / scale) with step, linear or cubic keys,
//!   loop modes, scalar curves and events; sampling is a pure function of (clip, time) built
//!   from IEEE basic operations and `specula_core::dmath`, so the same bits come out everywhere.
//! - [`blend`]: crossfades, override and additive layers, bone masks.
//! - [`gen`]: procedural generators (gait, idle, hover, swing, leap, collapse, wobble) that
//!   turn parameters into clips: agents author creatures without hand keyframing.
//! - [`AnimSet`]: one skeleton's clips and masks, loaded from an anim-set file.
//! - [`Animator`]: the per-entity player the engine drives: `set_clip`, `crossfade`,
//!   `set_layer`, `advance`/`advance_ticks`, `evaluate`, `events`, `root_motion`,
//!   `match_speed`.
//! - [`skin`]: CPU linear-blend skinning of `MeshData` (previews, tests, fallback). The GPU
//!   path takes [`write_gpu_matrices`] (column-major `mat4` per bone, one storage-buffer region
//!   per creature) plus the mesh's `joints`/`weights` attributes.
//! - [`measure`]: foot sliding, loop seams and pose hashes as numbers.
//!
//! # Conventions
//!
//! Right-handed, +Y up, metres; creatures face +Z, left is +X. Recipe rigs have rest frames
//! aligned with the model axes, so clip rotations are about the model's own axes: +X pitches
//! forward/down (knee bend, nod, jaw open), Y yaws, Z rolls. Clip values are **rest-relative**
//! (see [`clip`]).

pub mod animator;
pub mod blend;
pub mod clip;
pub mod error;
pub mod gen;
pub mod math;
pub mod measure;
pub mod set;
pub mod skeleton;
pub mod skin;

pub use animator::{Animator, FiredEvent, Layer, LayerMode, Playback, RootMotion, MAX_LAYERS};
pub use clip::{Channel, Clip, Curve, Event, Interp, Keys, LoopMode, Track};
pub use error::{AnimError, AnimErrorCode};
pub use math::Transform;
pub use set::{AnimSet, ClipId, MaskId};
pub use skeleton::{write_gpu_matrices, BoneMask, Pose, Skeleton};
pub use skin::skin_mesh;
