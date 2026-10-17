//! Specula physics: deterministic, CPU-only, zero-dependency (docs/design/PHYSICS.md).
//!
//! - **P1, queries and triggers.** Shapes ([`Shape`]: sphere, capsule, box, convex hull, and
//!   static triangle meshes and heightfields), a dynamic AABB tree broadphase ([`tree`]), the
//!   narrowphase ([`contact`]), raycasts, shape casts and overlaps with a stable result order
//!   ([`Space::raycast`], [`Space::cast`], [`Space::overlap`]), and trigger volumes with
//!   enter/exit events (stay = [`Space::touching`]).
//! - **P2, the kinematic character controller** ([`character`]): slide, step up, slope limit,
//!   ground snapping, moving platforms, pushing.
//! - **P3, rigid bodies with the Soft Step solver** ([`solver`]): sub-stepped soft contacts
//!   with relax and restitution passes, friction, warm starting by feature id, islands and
//!   sleeping, and a tunnelling guard for fast bodies against static geometry.
//!
//! **Determinism (F2):** only IEEE `+ - * /`, `sqrt` and `specula_core::dmath`; ordered
//! containers; every result sorted by stable keys. [`Space::hash`] covers the whole state, so
//! two runs from the same inputs can be compared tick by tick.
//!
//! **Bounded work (F1/F3):** bodies, pairs, triangles per pair, events and query results are
//! capped, and every cap that bites is reported as a structured [`PhysicsError`] (code
//! `budget`).
//!
//! **2D:** [`Settings::plane_2d`] keeps dynamic bodies in the XY plane (no Z motion, rotation
//! about Z only), the plane Specula's 2D entities live in. Shapes stay 3D (a circle is a
//! sphere, a 2D box a thin box), so 2D games get the same solver, queries and checks.

#![forbid(unsafe_code)]

pub mod character;
pub mod contact;
pub mod gjk;
pub mod math;
pub mod query;
pub mod shape;
pub mod solver;
pub mod space;
pub mod tree;

pub use character::{CharacterConfig, CharacterMove};
pub use math::{Aabb, Pose};
pub use shape::Shape;
pub use space::{
    BodyDesc, BodyKind, BodyView, ColliderDesc, ContactView, Event, EventKind, FastBody, Filter, Hit, Settings, Space, StepReport,
};

use specula_core::Json;

/// A structured physics error: a machine-readable `code`, a message, and details.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsError {
    pub code: &'static str,
    pub message: String,
    pub details: Vec<(&'static str, Json)>,
}

impl PhysicsError {
    pub fn new(code: &'static str, message: impl Into<String>) -> PhysicsError {
        PhysicsError { code, message: message.into(), details: Vec::new() }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<Json>) -> PhysicsError {
        self.details.push((key, value.into()));
        self
    }

    pub fn to_json(&self) -> Json {
        let mut fields = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        fields.extend(self.details.iter().map(|(k, v)| (*k, v.clone())));
        Json::obj(fields)
    }
}

impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PhysicsError {}
