//! Specula Locus (*locus*, "place"): placement by relations (docs/design/LOCUS.md).
//!
//! An agent states where something goes relative to other things ("on the table, against the
//! north wall, 2 m apart, facing the door"); [`solve`] turns the relations into candidate
//! positions, measures every relation's violation at each (metres or degrees), checks collisions
//! through a [`Probe`] the engine implements over physics and the navmesh, and keeps the best.
//! Unsatisfied relations come back as data with the nearest valid (collision-free) alternative.
//!
//! **Determinism (F2):** candidates are generated in a fixed order; arithmetic is IEEE
//! `+ - * /`, `sqrt` and `specula_core::dmath`; ties go to the earlier candidate.
//!
//! **Bounded (F3):** at most [`MAX_CANDIDATES`] candidates per instance (plus a fixed refinement
//! pass), [`MAX_COUNT`] instances and [`MAX_RELATIONS`] relations per call; probes are skipped
//! for candidates that can't beat the best so far.

#![forbid(unsafe_code)]

pub mod geom;
pub mod solve;

pub use geom::Box3;
pub use solve::{solve, Placement, Probe, Rel, RelReport, Relation, Request, Solution};

/// Most relations per call.
pub const MAX_RELATIONS: usize = 16;
/// Most instances per call.
pub const MAX_COUNT: usize = 64;
/// Most coarse candidates per instance.
pub const MAX_CANDIDATES: usize = 2304;
