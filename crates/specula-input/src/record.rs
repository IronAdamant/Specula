//! Recording and replay of **action state** (not raw OS events), one frame per simulation tick.
//!
//! Replays feed [`ActionFrame`]s back into [`Input`](crate::Input) with
//! [`Input::apply_frame`](crate::Input::apply_frame). These queries then answer exactly as they did
//! live: `state` / `states` / `pressed` / `released` / `held` / `value` / `value2`, `pointer`
//! (position and kind), `touches`, `text`, `last_device`, `dt`, and therefore `prompt` / `glyphs`
//! given the same map and pad style. Not recorded: the raw device queries (`source_pressed`,
//! `source_held`, which read idle during a replay) and rebinding (a menu activity, not gameplay).
//! A replay doesn't depend on window size, keyboard layout, pad dead zones or binding changes made
//! since.
//!
//! # Byte format (`SPIR` v1), deterministic: the same frames always give the same bytes
//!
//! ```text
//! "SPIR" u8:version(1)
//! varint:action_count  { u8:kind(0 button, 1 axis, 2 axis2)  varint:len  utf8:name } * action_count
//! varint:frame_count   frame * frame_count
//! frame  = varint:change_count  change * change_count   (changes vs the previous frame, slots ascending)
//! change = varint:slot  payload                          (n = action_count)
//!   slot <  n : u8 flags (1 held, 2 pressed, 4 released); axis: + f32; axis2: + f32 f32
//!   slot n    : pointer  u8 (0 none, 1 mouse, 2 touch, 3 pen) [f32 x, f32 y]
//!   slot n+1  : text     varint:count { u8 (0 char, 1 key, 2 key repeat)  varint:scalar or code }
//!                        (text never carries over to the next frame)
//!   slot n+2  : touches  varint:count { varint:id  f32 x  f32 y }
//!   slot n+3  : device   u8 (0 none, 1 keyboard/mouse, 2 touch, 3 + pad index)
//!   slot n+4  : dt       f32 seconds
//! ```
//!
//! Floats are little-endian and stored by bit pattern, so replays are bit-exact. A frame with no
//! changes is one byte.

use crate::error::{ErrorCode, InputError};
use crate::input::{Device, Pointer, TextEvent, Touch};
use crate::map::{schema_hash, ActionKind, ActionMap};
use specula_platform::PointerKind;

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

    fn same(&self, other: &ActionState) -> bool {
        self.flags() == other.flags() && self.value.map(f32::to_bits) == other.value.map(f32::to_bits)
    }
}

/// Everything game code can read from input in one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionFrame {
    /// Indexed like the map's actions.
    pub actions: Vec<ActionState>,
    pub pointer: Option<Pointer>,
    /// Text-mode events this frame.
    pub text: Vec<TextEvent>,
    /// Touch/pen contacts down, first-down first.
    pub touches: Vec<Touch>,
    pub last_device: Option<Device>,
    /// Seconds since the previous frame (as used for level sources).
    pub dt: f32,
}

impl ActionFrame {
    pub(crate) fn neutral(n: usize) -> ActionFrame {
        ActionFrame { actions: vec![ActionState::default(); n], ..ActionFrame::default() }
    }

    /// Copies `other` into `self`, reusing `self`'s buffers.
    pub fn copy_from(&mut self, other: &ActionFrame) {
        self.actions.clone_from(&other.actions);
        self.pointer = other.pointer;
        self.text.clone_from(&other.text);
        self.touches.clone_from(&other.touches);
        self.last_device = other.last_device;
        self.dt = other.dt;
    }
}

fn pointer_bits(p: &Option<Pointer>) -> Option<(PointerKind, u32, u32)> {
    p.map(|p| (p.kind, p.x.to_bits(), p.y.to_bits()))
}

fn touches_same(a: &[Touch], b: &[Touch]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.id == b.id && a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits())
}

fn device_code(d: Option<Device>) -> u64 {
    match d {
        None => 0,
        Some(Device::KeyboardMouse) => 1,
        Some(Device::Touch) => 2,
        Some(Device::Gamepad(i)) => 3 + i as u64,
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
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
/// journal tick by tick). Allocation-free per frame once warmed up. [`Recording::to_bytes`]
/// uses it.
pub struct FrameEncoder {
    kinds: Vec<ActionKind>,
    prev: ActionFrame,
    body: Vec<u8>,
}

impl FrameEncoder {
    pub fn new(map: &ActionMap) -> FrameEncoder {
        FrameEncoder::with_kinds(map.actions().iter().map(|a| a.kind).collect())
    }

    fn with_kinds(kinds: Vec<ActionKind>) -> FrameEncoder {
        let n = kinds.len();
        FrameEncoder { kinds, prev: ActionFrame::neutral(n), body: Vec::new() }
    }

    /// Appends one frame to `out`. Errors (`schema_mismatch`) if the frame has the wrong number of
    /// actions.
    pub fn encode(&mut self, frame: &ActionFrame, out: &mut Vec<u8>) -> Result<(), InputError> {
        let n = self.kinds.len();
        if frame.actions.len() != n {
            return Err(InputError::new(
                ErrorCode::SchemaMismatch,
                "",
                format!("frame has {} actions, the recording has {n}", frame.actions.len()),
            ));
        }
        let body = &mut self.body;
        body.clear();
        let mut changes = 0u64;
        for i in 0..n {
            let kind = self.kinds[i];
            let mut now = frame.actions[i];
            if kind == ActionKind::Button {
                // Buttons' value follows `held`; store what the decoder will rebuild.
                now = ActionState::button(now.held, now.pressed, now.released);
            } else if kind == ActionKind::Axis {
                now.value[1] = 0.0;
            }
            if now.same(&self.prev.actions[i]) {
                continue;
            }
            put_varint(body, i as u64);
            body.push(now.flags());
            match kind {
                ActionKind::Button => {}
                ActionKind::Axis => put_f32(body, now.value[0]),
                ActionKind::Axis2 => {
                    put_f32(body, now.value[0]);
                    put_f32(body, now.value[1]);
                }
            }
            self.prev.actions[i] = now;
            changes += 1;
        }
        if pointer_bits(&frame.pointer) != pointer_bits(&self.prev.pointer) {
            put_varint(body, n as u64);
            match frame.pointer {
                None => body.push(0),
                Some(p) => {
                    body.push(match p.kind {
                        PointerKind::Mouse => 1,
                        PointerKind::Touch => 2,
                        PointerKind::Pen => 3,
                    });
                    put_f32(body, p.x);
                    put_f32(body, p.y);
                }
            }
            changes += 1;
        }
        if !frame.text.is_empty() {
            put_varint(body, n as u64 + 1);
            put_varint(body, frame.text.len() as u64);
            for t in &frame.text {
                match *t {
                    TextEvent::Char(c) => {
                        body.push(0);
                        put_varint(body, u64::from(u32::from(c)));
                    }
                    TextEvent::Key { code, repeat } => {
                        body.push(if repeat { 2 } else { 1 });
                        put_varint(body, u64::from(code));
                    }
                }
            }
            changes += 1;
        }
        if !touches_same(&frame.touches, &self.prev.touches) {
            put_varint(body, n as u64 + 2);
            put_varint(body, frame.touches.len() as u64);
            for t in &frame.touches {
                put_varint(body, u64::from(t.id));
                put_f32(body, t.x);
                put_f32(body, t.y);
            }
            changes += 1;
        }
        if frame.last_device != self.prev.last_device {
            put_varint(body, n as u64 + 3);
            body.push(device_code(frame.last_device) as u8);
            changes += 1;
        }
        if frame.dt.to_bits() != self.prev.dt.to_bits() {
            put_varint(body, n as u64 + 4);
            put_f32(body, frame.dt);
            changes += 1;
        }
        put_varint(out, changes);
        out.extend_from_slice(body);
        self.prev.pointer = frame.pointer;
        self.prev.touches.clone_from(&frame.touches);
        self.prev.last_device = frame.last_device;
        self.prev.dt = frame.dt;
        Ok(())
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
        let mut frame = self.prev.clone();
        frame.text.clear();
        let mut last_slot: Option<u64> = None;
        for _ in 0..changes {
            let at = r.pos;
            let slot = r.varint()?;
            if last_slot.is_some_and(|l| slot <= l) {
                return Err(bad(at, format!("slot {slot} out of order")));
            }
            last_slot = Some(slot);
            let slot = usize::try_from(slot).unwrap_or(usize::MAX);
            match slot {
                i if i < n => {
                    let f = r.u8()?;
                    if f > 7 {
                        return Err(bad(r.pos - 1, format!("action flags {f:#x} out of range")));
                    }
                    let (held, pressed, released) = (f & 1 != 0, f & 2 != 0, f & 4 != 0);
                    frame.actions[i] = match self.kinds[i] {
                        ActionKind::Button => ActionState::button(held, pressed, released),
                        ActionKind::Axis => ActionState { held, pressed, released, value: [r.f32()?, 0.0] },
                        ActionKind::Axis2 => ActionState { held, pressed, released, value: [r.f32()?, r.f32()?] },
                    };
                }
                i if i == n => {
                    let kind = match r.u8()? {
                        0 => None,
                        1 => Some(PointerKind::Mouse),
                        2 => Some(PointerKind::Touch),
                        3 => Some(PointerKind::Pen),
                        other => return Err(bad(r.pos - 1, format!("pointer tag {other} out of range"))),
                    };
                    frame.pointer = match kind {
                        None => None,
                        Some(kind) => Some(Pointer { x: r.f32()?, y: r.f32()?, kind }),
                    };
                }
                i if i == n + 1 => {
                    let count = r.count("text event count")?;
                    for _ in 0..count {
                        let at = r.pos;
                        let tag = r.u8()?;
                        let v = r.varint()?;
                        frame.text.push(match tag {
                            0 => TextEvent::Char(
                                u32::try_from(v).ok().and_then(char::from_u32).ok_or_else(|| bad(at, "invalid char"))?,
                            ),
                            1 | 2 => TextEvent::Key {
                                code: u32::try_from(v).map_err(|_| bad(at, "key code out of range"))?,
                                repeat: tag == 2,
                            },
                            other => return Err(bad(at, format!("text tag {other} out of range"))),
                        });
                    }
                }
                i if i == n + 2 => {
                    let count = r.count("touch count")?;
                    frame.touches.clear();
                    for _ in 0..count {
                        let at = r.pos;
                        let id = u32::try_from(r.varint()?).map_err(|_| bad(at, "touch id out of range"))?;
                        frame.touches.push(Touch { id, x: r.f32()?, y: r.f32()? });
                    }
                }
                i if i == n + 3 => {
                    let at = r.pos;
                    frame.last_device = match r.u8()? {
                        0 => None,
                        1 => Some(Device::KeyboardMouse),
                        2 => Some(Device::Touch),
                        p @ 3..=6 => Some(Device::Gamepad(usize::from(p - 3))),
                        other => return Err(bad(at, format!("device tag {other} out of range"))),
                    };
                }
                i if i == n + 4 => frame.dt = r.f32()?,
                _ => return Err(bad(at, format!("slot {slot} out of range (0..={})", n + 4))),
            }
        }
        self.prev.copy_from(&frame);
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
        let same = self.schema.len() == map.actions().len()
            && self.schema.iter().zip(map.actions()).all(|((n, k), a)| *n == a.name && *k == a.kind);
        if same {
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
    /// Reused for the frames after the end.
    tail: ActionFrame,
}

impl Player {
    /// Errors (`schema_mismatch`) unless the recording was made with this map's actions.
    pub fn new(recording: Recording, map: &ActionMap) -> Result<Player, InputError> {
        recording.check(map)?;
        Ok(Player { frames: recording.frames, next: 0, tail: ActionFrame::default() })
    }

    /// Applies the next frame to `input` (instead of `Input::update`). Returns `false` once the
    /// recording is finished: from then on the input goes idle, with a `released` edge (in the
    /// first frame after the end) for every action that was still held.
    pub fn step(&mut self, input: &mut crate::Input) -> bool {
        if let Some(frame) = self.frames.get(self.next) {
            input.apply_frame(frame).expect("schema checked in Player::new");
            self.next += 1;
            return true;
        }
        let tail = &mut self.tail;
        tail.actions.clear();
        tail.actions.extend(input.states().iter().map(|s| ActionState {
            held: false,
            pressed: false,
            released: s.held,
            value: [0.0, 0.0],
        }));
        tail.pointer = input.pointer();
        tail.text.clear();
        tail.touches.clear();
        tail.last_device = input.last_device();
        tail.dt = 0.0;
        input.apply_frame(tail).expect("built from the input's own actions");
        false
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
