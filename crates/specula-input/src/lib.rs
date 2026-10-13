//! Specula input: **actions, not keys** (foundation F6), with touch as a first-class device.
//!
//! - [`ActionMap`]: named actions (`place`, `orbit`, `zoom`) and their bindings, as JSON data
//!   (`specula.input.v1`, see [`map`]). Bindings name sources like `Key.Space`, `Mouse.Right`,
//!   `Pad.South`, `Pad.RightStick`, `Touch.Pinch` (see [`source`]); chords (`Key.Ctrl+Key.Z`,
//!   `Mouse.Right+Mouse.Motion`), composites (WASD), per-binding scale / invert / dead zone.
//! - [`Input`]: per frame, platform events + pad states in, action states out: `pressed`,
//!   `released`, `held`, axis values, pointer position, last-used device (for glyph swapping),
//!   text input mode, and listen-for-next-input rebinding with conflict detection.
//! - Glyphs: [`Source::glyph`], [`Input::prompt`] (`"LMB"`, `"Space"`, `"A"`, `"RB"`, `"Cross"`,
//!   `"Tap"`).
//! - Recording: [`Recording`] / [`Player`] store per-tick **action state** (not OS events) in a
//!   compact deterministic byte format for journals and replays (F2).
//!
//! The crate is pure and platform-independent: it only uses `specula-platform`'s plain data types
//! (`Event`, `GamepadState`), so headless playtests can drive it with synthetic events.
//!
//! ```text
//! let mut input = Input::new(ActionMap::from_json_str(&std::fs::read_to_string(path)?)?);
//! loop {
//!     let events = window.poll_events();
//!     pads.poll();
//!     input.update(clock_s, &events, pads.states());
//!     if input.pressed("place") { /* ... */ }
//!     let orbit = input.value2("orbit"); // this frame's amount
//!     recording.push(input.action_frame())?;
//! }
//! ```

pub mod error;
mod gesture;
mod input;
pub mod map;
pub mod record;
pub mod source;

pub use error::{ErrorCode, InputError};
pub use input::{CancelReason, Device, Input, Pointer, RebindOutcome, RebindRequest, TextEvent, Touch};
pub use map::{
    Action, ActionKind, ActionMap, AxisOutput, Binding, BindingInput, BindingSlot, CompositePart, Conflict, Settings,
};
pub use record::{ActionFrame, ActionState, FrameDecoder, FrameEncoder, Player, Recording};
pub use source::{DeviceFamily, PadAxis, PadStyle, Signal, Source, Stick};
