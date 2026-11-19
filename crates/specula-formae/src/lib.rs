//! Specula Formae (*formae*: shapes): a library of named shapes an agent starts from by name and
//! changes by numbers, as a character starts from the measured human figure.
//!
//! The library is **a table** ([`forms`], [`aliases`], [`find`]): each [`Form`] has a name, a
//! kind ([`Kind::Plane`], [`Kind::Curve`], and from stage 1b [`Kind::Solid`]), a family, one line
//! of what it is, its parameters (unit, default, range, whether whole), whether it has curved
//! parts, and its relatives of the other kinds. A [`Spec`] is a form with resolved arguments:
//! it is counted ([`Spec::count`]: exactly, before any work), built ([`Spec::build`]),
//! measured in closed form ([`Spec::closed`]), its error at a detail known ([`Spec::error`]),
//! and anchored ([`Spec::anchors`]). The built shape measures itself ([`Built::measures`]), so
//! a closed form can always be set beside the shape as built.
//!
//! **Conventions.**
//! - Metres. Plane figures in x right, y up; solids right-handed, +Y up.
//! - A plane figure builds to an [`Outline`]: rings of `f64` points, the first the outer
//!   boundary counter-clockwise, any further rings holes clockwise, each ring simple, no ring
//!   repeating its first point. A curve builds to a [`Curve`]: points in order, open or closed.
//! - Each form's origin is said in its `origin` (its natural centre: a circle's centre, a
//!   triangle's centroid, an arch's base).
//! - Angles are given in degrees. Detail is the number of segments in a full turn
//!   ([`detail`] has the rule, the range and the error).
//! - A refusal is an [`Error`] with a code and the numbers that decided it.
//! - **A build is well made at every detail it accepts, or it is refused as data**: what
//!   [`Spec::build`] returns passes [`Outline::validate`] (or [`Curve::validate`]). A figure
//!   whose parts are farther apart in size than [`EDGE_FLOOR`] (a billionth) allows is refused
//!   as `edge_under_floor`, and a build whose chords cross at a coarse detail, though the
//!   figure does not, as `crosses_at_this_detail`; each names a detail that holds where one
//!   does. [`Spec::build`] has the rules.
//! - Deterministic: only IEEE basic operations, square roots and `specula_core::dmath`, so the
//!   same arguments give the same bytes on every machine.
//!
//! **Adding a form**: one entry in its family's list (`src/forms/*.rs`) with its parameters and
//! its functions beside it: `geom` (its exact pieces: lines, arcs, parametric pieces), `closed`
//! (its textbook formulas, where there are any) and `anchors`. Counting, building, the exact
//! measures of lines and arcs and the error are derived from `geom`; the crate's tests run over
//! the whole table, so a new entry is tested by being there.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod anchor;
pub mod detail;
pub mod error;
mod forms;
mod geom;
pub mod maths;
pub mod outline;
pub mod spec;
pub mod table;
#[cfg(test)]
mod tests;

pub use anchor::{Anchor, On};
pub use detail::{detail_for, sag, segments, DETAIL_DEFAULT, DETAIL_MAX, DETAIL_MIN};
pub use error::Error;
pub use outline::{Curve, Fault, Outline, EDGE_FLOOR};
pub use spec::{params_of, Args, Spec};
pub use table::{aliases, find, form, forms, nearest, Alias, Family, Form, Found, Kind, Param, Set, Unit};

/// The most points one build may have; a build over it is refused before any work.
pub const MAX_POINTS: u64 = 262_144;

/// What a build has, known before it is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Count {
    /// Points.
    pub points: u64,
    /// Rings of a plane figure; 1 for a curve; faces of a solid (stage 1b).
    pub parts: u64,
}

/// A shape's measures. For a plane figure: `size` its area, `boundary` its perimeter (holes'
/// included); for a curve: `boundary` its length (`size` and `centroid` none); for a solid
/// (stage 1b): `size` its volume, `boundary` its surface area. Plane coordinates have `z = 0`.
/// A measure that is not known is `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measures {
    /// Area or volume.
    pub size: Option<f64>,
    /// Perimeter, length or surface area.
    pub boundary: Option<f64>,
    /// The centroid of the area (or volume).
    pub centroid: Option<[f64; 3]>,
    /// The least and greatest corner of the bounding box.
    pub bounds: Option<([f64; 3], [f64; 3])>,
    /// Where a measure comes from a series or an iteration rather than a finite formula: its
    /// name and its error.
    pub series: Option<&'static str>,
}

/// A built shape.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum Built {
    /// A plane figure.
    Outline(Outline),
    /// A curve.
    Curve(Curve),
}

impl Built {
    /// The measures of what was built.
    pub fn measures(&self) -> Measures {
        match self {
            Built::Outline(o) => o.measures(),
            Built::Curve(c) => c.measures(),
        }
    }
    /// The number of points built.
    pub fn points(&self) -> u64 {
        match self {
            Built::Outline(o) => o.rings.iter().map(|r| r.len() as u64).sum(),
            Built::Curve(c) => c.points.len() as u64,
        }
    }
    /// Rings of a plane figure; 1 for a curve.
    pub fn parts(&self) -> u64 {
        match self {
            Built::Outline(o) => o.rings.len() as u64,
            Built::Curve(_) => 1,
        }
    }
    /// The build's chord error (see [`Outline::chord_error`]).
    pub fn chord_error(&self) -> f64 {
        match self {
            Built::Outline(o) => o.chord_error,
            Built::Curve(c) => c.chord_error,
        }
    }
}
