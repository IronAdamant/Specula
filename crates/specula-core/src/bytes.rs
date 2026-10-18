//! A small little-endian binary codec for saved simulation state (save games, snapshots).
//!
//! [`ByteWriter`] appends fixed-width integers, floats **as exact bits** (determinism rule 9:
//! a save must load bit for bit), length-prefixed strings and byte runs. [`ByteReader`] reads
//! them back and never panics or allocates more than its input could hold: every read is
//! bounds-checked, every length is checked against the bytes left (and an optional cap), and a
//! failure is a structured [`DecodeError`] with the byte offset and what was being read, so
//! corrupt or hostile input is an error an agent can act on, never a crash.
//!
//! ```text
//! let mut w = ByteWriter::new();
//! w.u32(7); w.f32(0.1); w.str("waves");
//! let mut r = ByteReader::new(w.bytes());
//! assert_eq!((r.u32("n")?, r.f32("x")?, r.str("name", 64)?), (7, 0.1, "waves".into()));
//! r.finish()?;
//! ```

use crate::math::{Quat, Vec3};
use std::fmt;

/// Why bytes could not be read: a machine-readable `code`, where (`offset`), and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    /// `truncated`, `too_long`, `invalid`, `trailing`, `bad_utf8`.
    pub code: &'static str,
    pub offset: usize,
    /// The field being read (`"bodies[3].pose"`), as the caller named it.
    pub field: String,
    pub message: String,
}

impl DecodeError {
    pub fn new(code: &'static str, offset: usize, field: &str, message: impl Into<String>) -> DecodeError {
        DecodeError { code, offset, field: field.to_owned(), message: message.into() }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {} ({}): {}", self.code, self.offset, self.field, self.message)
    }
}

impl std::error::Error for DecodeError {}

/// Appends values in the codec's format.
#[derive(Clone, Debug, Default)]
pub struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    pub fn new() -> ByteWriter {
        ByteWriter { buf: Vec::new() }
    }

    pub fn with_capacity(n: usize) -> ByteWriter {
        ByteWriter { buf: Vec::with_capacity(n) }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Exact bits (NaN payloads and `-0.0` included).
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    pub fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    pub fn quat(&mut self, q: Quat) {
        for c in [q.x, q.y, q.z, q.w] {
            self.f32(c);
        }
    }

    /// A count or length (u32; saved state never needs more).
    pub fn len_u32(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }

    /// Length-prefixed UTF-8.
    pub fn str(&mut self, s: &str) {
        self.blob(s.as_bytes());
    }

    /// Length-prefixed bytes.
    pub fn blob(&mut self, b: &[u8]) {
        self.len_u32(b.len());
        self.buf.extend_from_slice(b);
    }

    /// `Some` as 1 + value, `None` as 0.
    pub fn opt_u32(&mut self, v: Option<u32>) {
        self.bool(v.is_some());
        if let Some(v) = v {
            self.u32(v);
        }
    }

    pub fn opt_u64(&mut self, v: Option<u64>) {
        self.bool(v.is_some());
        if let Some(v) = v {
            self.u64(v);
        }
    }

    /// Raw bytes, no length (the reader must know how many).
    pub fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

/// Reads what a [`ByteWriter`] wrote. Every method takes the field's name for errors.
#[derive(Clone, Debug)]
pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> ByteReader<'a> {
        ByteReader { data, pos: 0 }
    }

    pub fn offset(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// An error at the current offset.
    pub fn error(&self, code: &'static str, field: &str, message: impl Into<String>) -> DecodeError {
        DecodeError::new(code, self.pos, field, message)
    }

    fn take(&mut self, n: usize, field: &str) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(self.error("truncated", field, format!("needs {n} more bytes, {} left", self.remaining())));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn array<const N: usize>(&mut self, field: &str) -> Result<[u8; N], DecodeError> {
        let s = self.take(N, field)?;
        let mut a = [0u8; N];
        a.copy_from_slice(s);
        Ok(a)
    }

    pub fn u8(&mut self, field: &str) -> Result<u8, DecodeError> {
        Ok(self.array::<1>(field)?[0])
    }

    pub fn bool(&mut self, field: &str) -> Result<bool, DecodeError> {
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(DecodeError::new("invalid", self.pos - 1, field, format!("a flag is 0 or 1, not {v}"))),
        }
    }

    pub fn u16(&mut self, field: &str) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.array(field)?))
    }

    pub fn u32(&mut self, field: &str) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array(field)?))
    }

    pub fn u64(&mut self, field: &str) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array(field)?))
    }

    pub fn i8(&mut self, field: &str) -> Result<i8, DecodeError> {
        Ok(self.u8(field)? as i8)
    }

    pub fn i32(&mut self, field: &str) -> Result<i32, DecodeError> {
        Ok(i32::from_le_bytes(self.array(field)?))
    }

    pub fn i64(&mut self, field: &str) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.array(field)?))
    }

    pub fn f32(&mut self, field: &str) -> Result<f32, DecodeError> {
        Ok(f32::from_bits(self.u32(field)?))
    }

    pub fn f64(&mut self, field: &str) -> Result<f64, DecodeError> {
        Ok(f64::from_bits(self.u64(field)?))
    }

    pub fn vec3(&mut self, field: &str) -> Result<Vec3, DecodeError> {
        Ok(Vec3::new(self.f32(field)?, self.f32(field)?, self.f32(field)?))
    }

    pub fn quat(&mut self, field: &str) -> Result<Quat, DecodeError> {
        Ok(Quat { x: self.f32(field)?, y: self.f32(field)?, z: self.f32(field)?, w: self.f32(field)? })
    }

    /// A count of items each at least `min_item_bytes` long, at most `max`: a count the rest of
    /// the input can't hold is refused before anything is allocated for it.
    pub fn count(&mut self, field: &str, max: usize, min_item_bytes: usize) -> Result<usize, DecodeError> {
        let at = self.pos;
        let n = self.u32(field)? as usize;
        if n > max {
            return Err(DecodeError::new("too_long", at, field, format!("{n} items; at most {max}")));
        }
        if n.saturating_mul(min_item_bytes.max(1)) > self.remaining() {
            return Err(DecodeError::new("truncated", at, field, format!("{n} items can't fit in the {} bytes left", self.remaining())));
        }
        Ok(n)
    }

    /// A length-prefixed byte run of at most `max` bytes.
    pub fn blob(&mut self, field: &str, max: usize) -> Result<&'a [u8], DecodeError> {
        let n = self.count(field, max, 1)?;
        self.take(n, field)
    }

    /// Length-prefixed UTF-8 of at most `max` bytes.
    pub fn str(&mut self, field: &str, max: usize) -> Result<String, DecodeError> {
        let at = self.pos;
        let b = self.blob(field, max)?;
        std::str::from_utf8(b).map(str::to_owned).map_err(|e| DecodeError::new("bad_utf8", at, field, e.to_string()))
    }

    pub fn opt_u32(&mut self, field: &str) -> Result<Option<u32>, DecodeError> {
        Ok(if self.bool(field)? { Some(self.u32(field)?) } else { None })
    }

    pub fn opt_u64(&mut self, field: &str) -> Result<Option<u64>, DecodeError> {
        Ok(if self.bool(field)? { Some(self.u64(field)?) } else { None })
    }

    /// `n` raw bytes.
    pub fn raw(&mut self, field: &str, n: usize) -> Result<&'a [u8], DecodeError> {
        self.take(n, field)
    }

    /// Everything must have been read.
    pub fn finish(&self) -> Result<(), DecodeError> {
        if self.remaining() > 0 {
            return Err(self.error("trailing", "end", format!("{} bytes left over", self.remaining())));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_bit_exactly() {
        let mut w = ByteWriter::new();
        w.u8(1);
        w.bool(true);
        w.u16(0xbeef);
        w.u32(7);
        w.u64(u64::MAX);
        w.i8(-3);
        w.i32(-5);
        w.i64(i64::MIN);
        w.f32(-0.0);
        w.f32(f32::from_bits(0x7fc0_1234));
        w.f64(0.1);
        w.vec3(Vec3::new(1.0, -2.5, 3.25));
        w.quat(Quat { x: 0.0, y: 0.5, z: -0.5, w: 0.70710677 });
        w.str("waves");
        w.blob(&[1, 2, 3]);
        w.opt_u32(None);
        w.opt_u64(Some(9));
        let mut r = ByteReader::new(w.bytes());
        assert_eq!(r.u8("a").unwrap(), 1);
        assert!(r.bool("b").unwrap());
        assert_eq!(r.u16("c").unwrap(), 0xbeef);
        assert_eq!(r.u32("d").unwrap(), 7);
        assert_eq!(r.u64("e").unwrap(), u64::MAX);
        assert_eq!(r.i8("f").unwrap(), -3);
        assert_eq!(r.i32("g").unwrap(), -5);
        assert_eq!(r.i64("h").unwrap(), i64::MIN);
        assert_eq!(r.f32("i").unwrap().to_bits(), (-0.0f32).to_bits());
        assert_eq!(r.f32("j").unwrap().to_bits(), 0x7fc0_1234);
        assert_eq!(r.f64("k").unwrap(), 0.1);
        assert_eq!(r.vec3("l").unwrap(), Vec3::new(1.0, -2.5, 3.25));
        assert_eq!(r.quat("m").unwrap().w, 0.70710677);
        assert_eq!(r.str("n", 16).unwrap(), "waves");
        assert_eq!(r.blob("o", 16).unwrap(), &[1, 2, 3]);
        assert_eq!(r.opt_u32("p").unwrap(), None);
        assert_eq!(r.opt_u64("q").unwrap(), Some(9));
        r.finish().unwrap();
    }

    #[test]
    fn bad_input_is_a_structured_error_never_a_panic() {
        let mut w = ByteWriter::new();
        w.str("hello");
        let b = w.into_bytes();
        // Every truncation fails cleanly.
        for n in 0..b.len() {
            let mut r = ByteReader::new(&b[..n]);
            let e = r.str("name", 64).unwrap_err();
            assert_eq!(e.code, "truncated", "{n}: {e}");
        }
        // Over the cap.
        assert_eq!(ByteReader::new(&b).str("name", 3).unwrap_err().code, "too_long");
        // A huge count with few bytes is refused before allocating.
        let mut w = ByteWriter::new();
        w.u32(u32::MAX);
        let mut r = ByteReader::new(w.bytes());
        assert_eq!(r.count("items", usize::MAX, 8).unwrap_err().code, "truncated");
        // Flags are 0 or 1; strings are UTF-8; leftovers are reported.
        assert_eq!(ByteReader::new(&[2]).bool("flag").unwrap_err().code, "invalid");
        assert_eq!(ByteReader::new(&[2, 0, 0, 0, 0xff, 0xfe]).str("s", 8).unwrap_err().code, "bad_utf8");
        let r = ByteReader::new(&[0]);
        assert_eq!(r.finish().unwrap_err().code, "trailing");
    }
}
