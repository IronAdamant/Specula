//! Specula Tempus (*tempus*, "time"): timelines, sequencer tracks and camera rails as data
//! (docs/design/TIMELINE.md).
//!
//! - [`timeline`]: the format (`specula.timeline.v1`): camera rails, entity tracks (transform,
//!   visibility, colour, emissive), anim and vfx triggers, audio cues, subtitles and events for the
//!   hosted game; parsed with structured errors ([`TimelineError`]), bounded everywhere.
//! - [`rail`]: centripetal Catmull-Rom rails parameterised by arc length.
//! - [`ease`]: ease curves between keys.
//! - [`eval`]: the state at a time, a pure function of the time (scrubbing equals playing), and
//!   what fires once in a span.
//! - [`check`]: motion checks of a rail camera (speed, jumps, turn rate, upright, roll).
//!
//! **Deterministic** (F2): IEEE `+ − × ÷`, `sqrt` and `specula_core::dmath` only, ordered
//! containers, no clocks: the crate carries the determinism lints. The engine evaluates
//! timelines on its fixed tick.

#![forbid(unsafe_code)]

pub mod check;
pub mod ease;
pub mod error;
pub mod eval;
pub mod rail;
pub mod timeline;

pub use ease::Ease;
pub use error::TimelineError;
pub use eval::{EntityState, Fired, Pose};
pub use rail::Rail;
pub use timeline::{CameraTrack, Cue, CueAt, EntityTrack, GameEvent, Line, Look, Timeline, Track, TrackData, Trigger};
