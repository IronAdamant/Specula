//! Recording and replay of **action state** (not raw OS events), one frame per simulation tick.
//!
//! Replays feed [`ActionFrame`]s back into [`Input`](crate::Input), so game code reads
//! `pressed("place")` the same way live and in a replay, and a replay doesn't depend on window
//! size, keyboard layout, pad dead zones or binding changes made since.
//!
//! # Byte format (`SPIR` v1), deterministic: the same frames always give the same bytes
//!
//! ```text
//! "SPIR" u8:version(1)
//! varint:action_count  { u8:kind(0 button, 1 axis, 2 axis2)  varint:len  utf8:name } * action_count
//! varint:frame_count   frame * frame_count
//! frame  = varint:change_count  change * change_count        (changes vs the previous frame)
//! change = varint:slot  payload
//!   slot <  action_count: button -> u8 flags (1 held, 2 pressed, 4 released)
//!                         axis   -> f32 LE
//!                         axis2  -> f32 LE, f32 LE
//!   slot == action_count:     pointer -> u8 present, then f32 LE x, f32 LE y if present
//!   slot == action_count + 1: text    -> varint:len utf8      (text never carries over)
//! ```
//!
//! An idle frame is one byte. Axis edges (`held` = non-zero, `pressed`, `released`) are derived
//! when decoding, exactly as [`Input`](crate::Input) derives them live. Floats are stored by bit
//! pattern, so replays are bit-exact.

use crate::error::{ErrorCode, InputError};
use crate::map::{schema_hash, ActionKind, ActionMap};

const MAGIC: &[u8; 4] = b"SPIR";
const VERSION: u8 = 1;

/// One action's state for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActionState {
    /// Buttons: down. Axes: the value is non-zero.
    pub held: bool,
    /// Became held this frame (buttons: also a re-press within one frame).
    pub pressed: bool,
    /// Stopped being held this frame.
    pub released: bool,
    /// Axis value (`[v, 0]` for one-number axes); `[1, 0]` / `[0, 0]` for buttons.
    pub value: [f32; 2],
}

impl ActionState {
    /// Axis state with edges derived from the previous frame's state.
    pub(crate) fn axis(value: [f32; 2], prev: &ActionState) -> ActionState {
        let held = value != [0.0, 0.0];
        ActionState { held, pressed: held && !prev.held, released: !held && prev.held, value }
    }

    pub(crate) fn button(held: bool, pressed: bool, released: bool) -> ActionState {
        ActionState { held, pressed, released, value: [if held { 1.0 } else { 0.0 }, 0.0] }
    }

    fn flags(&self) -> u8 {
        u8::from(self.held) | u8::from(self.pressed) << 1 | u8::from(self.released) << 2
    }
}

/// Everything game code can read from input in one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionFrame {
    /// Indexed like the map's actions.
    pub actions: Vec<ActionState>,
    /// Pointer position (mouse or primary touch), client pixels.
    pub pointer: Option<[f32; 2]>,
    /// Text typed this frame (text input mode only).
    pub text: String,
}

impl ActionFrame {
    pub(crate) fn neutral(n: usize) -> ActionFrame {
        ActionFrame { actions: vec![ActionState::default(); n], pointer: None, text: String::new() }
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn bad(offset: usize, message: impl Into<String>) -> InputError {
    InputError::new(ErrorCode::BadRecording, "", message).at(offset)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Result<u8, InputError> {
        let b = *self.bytes.get(self.pos).ok_or_else(|| bad(self.pos, "unexpected end of recording"))?;
        self.pos += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u64, InputError> {
        let start = self.pos;
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= u64::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(bad(start, "varint longer than 64 bits"))
    }

    fn count(&mut self, what: &str) -> Result<usize, InputError> {
        let start = self.pos;
        let v = self.varint()?;
        // Every counted item takes at least one byte, so a count past the remaining bytes is corrupt.
        if v > (self.bytes.len() - self.pos) as u64 {
            return Err(bad(start, format!("{what} {v} exceeds the remaining {} bytes", self.bytes.len() - self.pos)));
        }
        Ok(v as usize)
    }

    fn f32(&mut self) -> Result<f32, InputError> {
        let mut b = [0u8; 4];
        for byte in &mut b {
            *byte = self.u8()?;
        }
        Ok(f32::from_le_bytes(b))
    }

    fn str(&mut self) -> Result<String, InputError> {
        let len = self.count("string length")?;
        let start = self.pos;
        let bytes = &self.bytes[start..start + len];
        self.pos += len;
        String::from_utf8(bytes.to_vec()).map_err(|_| bad(start, "string is not UTF-8"))
    }
}

/// Encodes frames one at a time as changes against the previous frame (for streaming into a
/// journal tick by tick). [`Recording::to_bytes`] uses it.
pub struct FrameEncoder {
    kinds: Vec<ActionKind>,
    prev: ActionFrame,
}

impl FrameEncoder {
    pub fn new(map: &ActionMap) -> FrameEncoder {
        FrameEncoder::with_kinds(map.actions().iter().map(|a| a.kind).collect())
    }

    fn with_kinds(kinds: Vec<ActionKind>) -> FrameEncoder {
        let n = kinds.len();
        FrameEncoder { kinds, prev: ActionFrame::neutral(n) }
    }

    /// Appends one frame. Errors (`schema_mismatch`) if the frame has the wrong number of actions.
    pub fn encode(&mut self, frame: &ActionFrame, out: &mut Vec<u8>) -> Result<(), InputError> {
        let n = self.kinds.len();
        if frame.actions.len() != n {
            return Err(InputError::new(
                ErrorCode::SchemaMismatch,
                "",
                format!("frame has {} actions, the recording has {n}", frame.actions.len()),
            ));
        }
        let mut body = Vec::new();
        let mut changes = 0u64;
        for (i, (kind, (now, prev))) in self.kinds.iter().zip(frame.actions.iter().zip(&self.prev.actions)).enumerate() {
            match kind {
                ActionKind::Button => {
                    if now.flags() != prev.flags() {
                        put_varint(&mut body, i as u64);
                        body.push(now.flags());
                        changes += 1;
                    }
                }
                ActionKind::Axis => {
                    if now.value[0].to_bits() != prev.value[0].to_bits() {
                        put_varint(&mut body, i as u64);
                        body.extend_from_slice(&now.value[0].to_le_bytes());
                        changes += 1;
                    }
                }
                ActionKind::Axis2 => {
                    if now.value.map(f32::to_bits) != prev.value.map(f32::to_bits) {
                        put_varint(&mut body, i as u64);
                        body.extend_from_slice(&now.value[0].to_le_bytes());
                        body.extend_from_slice(&now.value[1].to_le_bytes());
                        changes += 1;
                    }
                }
            }
        }
        if frame.pointer.map(|p| p.map(f32::to_bits)) != self.prev.pointer.map(|p| p.map(f32::to_bits)) {
            put_varint(&mut body, n as u64);
            match frame.pointer {
                Some([x, y]) => {
                    body.push(1);
                    body.extend_from_slice(&x.to_le_bytes());
                    body.extend_from_slice(&y.to_le_bytes());
                }
                None => body.push(0),
            }
            changes += 1;
        }
        if !frame.text.is_empty() {
            put_varint(&mut body, n as u64 + 1);
            put_varint(&mut body, frame.text.len() as u64);
            body.extend_from_slice(frame.text.as_bytes());
            changes += 1;
        }
        put_varint(out, changes);
        out.extend_from_slice(&body);
        // Store what the decoder will reconstruct (axis flags derived, buttons' value normalised),
        // so the encoder and decoder agree on "previous" exactly.
        self.prev = canonical(&self.kinds, frame, &self.prev);
        Ok(())
    }
}

/// The frame as the decoder will produce it.
fn canonical(kinds: &[ActionKind], frame: &ActionFrame, prev: &ActionFrame) -> ActionFrame {
    ActionFrame {
        actions: kinds
            .iter()
            .zip(frame.actions.iter().zip(&prev.actions))
            .map(|(kind, (now, prev))| match kind {
                ActionKind::Button => ActionState::button(now.held, now.pressed, now.released),
                ActionKind::Axis => ActionState::axis([now.value[0], 0.0], prev),
                ActionKind::Axis2 => ActionState::axis(now.value, prev),
            })
            .collect(),
        pointer: frame.pointer,
        text: frame.text.clone(),
    }
}

/// Decodes frames written by [`FrameEncoder`] (same action kinds, same order).
pub struct FrameDecoder {
    kinds: Vec<ActionKind>,
    prev: ActionFrame,
}

impl FrameDecoder {
    pub fn new(map: &ActionMap) -> FrameDecoder {
        FrameDecoder::with_kinds(map.actions().iter().map(|a| a.kind).collect())
    }

    fn with_kinds(kinds: Vec<ActionKind>) -> FrameDecoder {
        let n = kinds.len();
        FrameDecoder { kinds, prev: ActionFrame::neutral(n) }
    }

    /// Decodes one frame starting at `*pos`, advancing it.
    pub fn decode(&mut self, bytes: &[u8], pos: &mut usize) -> Result<ActionFrame, InputError> {
        let mut r = Reader { bytes, pos: *pos };
        let frame = self.read(&mut r)?;
        *pos = r.pos;
        Ok(frame)
    }

    fn read(&mut self, r: &mut Reader) -> Result<ActionFrame, InputError> {
        let n = self.kinds.len();
        let changes = r.count("change count")?;
        let mut values: Vec<ActionState> = self.prev.actions.clone();
        let mut pointer = self.prev.pointer;
        let mut text = String::new();
        let mut last_slot: Option<u64> = None;
        for _ in 0..changes {
            let at = r.pos;
            let slot = r.varint()?;
            if last_slot.is_some_and(|l| slot <= l) {
                return Err(bad(at, format!("slot {slot} out of order")));
            }
            last_slot = Some(slot);
            match slot as usize {
                i if i < n => match self.kinds[i] {
                    ActionKind::Button => {
                        let f = r.u8()?;
                        if f > 7 {
                            return Err(bad(r.pos - 1, format!("button flags {f:#x} out of range")));
                        }
                        values[i] = ActionState::button(f & 1 != 0, f & 2 != 0, f & 4 != 0);
                    }
                    ActionKind::Axis => values[i].value = [r.f32()?, 0.0],
                    ActionKind::Axis2 => values[i].value = [r.f32()?, r.f32()?],
                },
                i if i == n => {
                    pointer = match r.u8()? {
                        0 => None,
                        1 => Some([r.f32()?, r.f32()?]),
                        other => return Err(bad(r.pos - 1, format!("pointer tag {other} is not 0 or 1"))),
                    }
                }
                i if i == n + 1 => text = r.str()?,
                _ => return Err(bad(at, format!("slot {slot} out of range (0..={})", n + 1))),
            }
        }
        // Unchanged buttons repeat the previous flags exactly (the encoder writes any flag change);
        // axis edges are derived from the values.
        let actions = self
            .kinds
            .iter()
            .zip(values.iter().zip(&self.prev.actions))
            .map(|(kind, (v, prev))| match kind {
                ActionKind::Button => *v,
                ActionKind::Axis | ActionKind::Axis2 => ActionState::axis(v.value, prev),
            })
            .collect();
        let frame = ActionFrame { actions, pointer, text };
        self.prev = frame.clone();
        Ok(frame)
    }
}

/// A sequence of frames plus the action schema it was recorded with.
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    schema: Vec<(String, ActionKind)>,
    frames: Vec<ActionFrame>,
}

impl Recording {
    pub fn new(map: &ActionMap) -> Recording {
        Recording { schema: map.actions().iter().map(|a| (a.name.clone(), a.kind)).collect(), frames: Vec::new() }
    }

    /// Appends a frame (e.g. from [`Input::action_frame`](crate::Input::action_frame)).
    pub fn push(&mut self, frame: ActionFrame) -> Result<(), InputError> {
        if frame.actions.len() != self.schema.len() {
            return Err(InputError::new(
                ErrorCode::SchemaMismatch,
                "",
                format!("frame has {} actions, the recording has {}", frame.actions.len(), self.schema.len()),
            ));
        }
        self.frames.push(frame);
        Ok(())
    }

    pub fn frames(&self) -> &[ActionFrame] {
        &self.frames
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Action names and kinds, in slot order.
    pub fn schema(&self) -> &[(String, ActionKind)] {
        &self.schema
    }

    /// Same hash as [`ActionMap::schema_hash`] for the map it was recorded with.
    pub fn schema_hash(&self) -> u64 {
        schema_hash(self.schema.iter().map(|(n, k)| (n.as_str(), *k)))
    }

    /// `schema_mismatch` unless `map` has exactly the recorded actions (names, kinds, order).
    pub fn check(&self, map: &ActionMap) -> Result<(), InputError> {
        if self.schema_hash() == map.schema_hash() && self.schema.len() == map.actions().len() {
            return Ok(());
        }
        let recorded: Vec<String> = self.schema.iter().map(|(n, k)| format!("{n}:{}", k.name())).collect();
        let current: Vec<String> = map.actions().iter().map(|a| format!("{}:{}", a.name, a.kind.name())).collect();
        Err(InputError::new(
            ErrorCode::SchemaMismatch,
            "",
            format!("recorded actions [{}] differ from the map's [{}]", recorded.join(", "), current.join(", ")),
        ))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(VERSION);
        put_varint(&mut out, self.schema.len() as u64);
        for (name, kind) in &self.schema {
            out.push(*kind as u8);
            put_varint(&mut out, name.len() as u64);
            out.extend_from_slice(name.as_bytes());
        }
        put_varint(&mut out, self.frames.len() as u64);
        let mut enc = FrameEncoder::with_kinds(self.schema.iter().map(|(_, k)| *k).collect());
        for f in &self.frames {
            enc.encode(f, &mut out).expect("frames are checked on push");
        }
        out
    }

    /// Parses [`Recording::to_bytes`] output. Errors are `bad_recording` with a byte offset.
    pub fn from_bytes(bytes: &[u8]) -> Result<Recording, InputError> {
        if bytes.len() < 5 || &bytes[..4] != MAGIC {
            return Err(bad(0, "not a Specula input recording (magic \"SPIR\" missing)"));
        }
        if bytes[4] != VERSION {
            return Err(bad(4, format!("unsupported recording version {} (expected {VERSION})", bytes[4])));
        }
        let mut r = Reader { bytes, pos: 5 };
        let n = r.count("action count")?;
        let mut schema = Vec::with_capacity(n);
        for _ in 0..n {
            let at = r.pos;
            let kind = match r.u8()? {
                0 => ActionKind::Button,
                1 => ActionKind::Axis,
                2 => ActionKind::Axis2,
                k => return Err(bad(at, format!("unknown action kind {k}"))),
            };
            schema.push((r.str()?, kind));
        }
        let count = r.count("frame count")?;
        let mut dec = FrameDecoder::with_kinds(schema.iter().map(|(_, k)| *k).collect());
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            frames.push(dec.read(&mut r)?);
        }
        if r.pos != bytes.len() {
            return Err(bad(r.pos, format!("{} trailing bytes", bytes.len() - r.pos)));
        }
        Ok(Recording { schema, frames })
    }

    /// Base64 of [`Recording::to_bytes`], for JSON journals.
    pub fn to_base64(&self) -> String {
        specula_core::base64::encode(&self.to_bytes())
    }

    pub fn from_base64(text: &str) -> Result<Recording, InputError> {
        let bytes = specula_core::base64::decode(text)
            .map_err(|e| InputError::new(ErrorCode::BadRecording, "", format!("base64: {}", e.message)).at(e.offset))?;
        Recording::from_bytes(&bytes)
    }
}

/// Feeds a recording back into an [`Input`](crate::Input), one frame per call.
pub struct Player {
    frames: Vec<ActionFrame>,
    next: usize,
}

impl Player {
    /// Errors (`schema_mismatch`) unless the recording was made with this map's actions.
    pub fn new(recording: Recording, map: &ActionMap) -> Result<Player, InputError> {
        recording.check(map)?;
        Ok(Player { frames: recording.frames, next: 0 })
    }

    /// Applies the next frame to `input` (instead of `Input::update`). `false` once finished; the
    /// input is then left neutral.
    pub fn step(&mut self, input: &mut crate::Input) -> bool {
        match self.frames.get(self.next) {
            Some(frame) => {
                input.apply_frame(frame).expect("schema checked in Player::new");
                self.next += 1;
                true
            }
            None => {
                let n = input.map().actions().len();
                let _ = input.apply_frame(&ActionFrame::neutral(n));
                false
            }
        }
    }

    /// Frames applied so far.
    pub fn position(&self) -> usize {
        self.next
    }

    pub fn remaining(&self) -> usize {
        self.frames.len() - self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_round_trip() {
        for v in [0u64, 1, 127, 128, 300, 16_383, 16_384, u64::from(u32::MAX), u64::MAX] {
            let mut out = Vec::new();
            put_varint(&mut out, v);
            let mut r = Reader { bytes: &out, pos: 0 };
            assert_eq!(r.varint().unwrap(), v);
            assert_eq!(r.pos, out.len());
        }
        let mut r = Reader { bytes: &[0xFF; 11], pos: 0 };
        assert_eq!(r.varint().unwrap_err().code, ErrorCode::BadRecording);
    }
}
