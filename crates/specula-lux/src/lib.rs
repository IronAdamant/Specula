//! Specula Lux (*lux*, "light"): baked global illumination as irradiance probe volumes
//! (docs/design/GI.md).
//!
//! The engine gathers a [`Scene`] (triangles with albedo and emission, point lights, the sun and
//! the sky) and a [`Request`] (a box, a spacing, rays and bounces); [`bake`] traces it into a
//! [`Field`] of probes, each with L1 spherical harmonics of the irradiance it sees, its cell's
//! visibility mask (which corner probes each octant of the cell sees), how far it sees along
//! fourteen directions (its reaches), and the offset that moved it out of geometry. Masks and
//! reaches together keep light from leaking through walls ([`Field::sample`]). [`file`] stores a
//! field content-addressed; [`Field::sample`] is the CPU twin of the GPU sampler
//! (`crates/specula-gpu/shaders/gi.glsl`).
//!
//! **Determinism (F2):** arithmetic is IEEE `+ - * /`, `sqrt` and `specula_core::dmath`; probes
//! are independent and merged by index, so the same scene and request give the same bytes on
//! every machine, adapter and thread count.
//!
//! **Bounded (F3):** [`bake::plan`] refuses requests past [`bake::MAX_PROBES`],
//! [`bake::MAX_WORK`] ray casts (the point lights' shadow rays included: [`check_work`]) or
//! [`bake::MAX_CACHE_BYTES`] of hit cache before any work, and a bake can be cancelled at any
//! point: between chunks of probes, within a probe's rays, and in the BVH build ([`Control`]).

#![forbid(unsafe_code)]

pub mod bake;
pub mod bvh;
pub mod field;
pub mod file;
pub mod half;
pub mod scene;
pub mod sh;

pub use bake::{bake, check_work, plan, work_with_lights, BakeError, Baked, Control, Phase, Plan, Progress, Request, Stats};
pub use field::{Contribution, Field, Lattice, Probe, Record, Sample};
pub use scene::{PointLight, Scene, Sky, Sun, Triangle};
pub use sh::Sh;
