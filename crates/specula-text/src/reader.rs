//! Bounds-checked big-endian reads. Every read returns a [`FontError`] instead of panicking.

use crate::error::{FontError, Reason, Tag};

/// A view of one table (or any byte range) of the font file. Offsets passed to the read methods
/// are relative to the start of the view; errors report absolute file offsets.
#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub data: &'a [u8],
    /// Absolute offset of `data[0]` in the font file.
    pub base: usize,
    pub tag: Tag,
}

impl<'a> View<'a> {
    pub fn new(data: &'a [u8], base: usize, tag: Tag) -> View<'a> {
        View { data, base, tag }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn err(&self, offset: usize, reason: Reason) -> FontError {
        FontError::new(self.tag, self.base.saturating_add(offset), reason)
    }

    pub fn bad(&self, offset: usize, field: &'static str, value: i64) -> FontError {
        self.err(offset, Reason::BadValue { field, value })
    }

    pub fn bytes(&self, offset: usize, len: usize) -> Result<&'a [u8], FontError> {
        match offset.checked_add(len) {
            Some(end) if end <= self.data.len() => Ok(&self.data[offset..end]),
            _ => Err(self.err(
                offset,
                Reason::Truncated { needed: len, available: self.data.len().saturating_sub(offset) },
            )),
        }
    }

    /// A sub-view `[offset, offset + len)`.
    pub fn sub(&self, offset: usize, len: usize) -> Result<View<'a>, FontError> {
        let data = self.bytes(offset, len)?;
        Ok(View { data, base: self.base + offset, tag: self.tag })
    }

    pub fn u8(&self, offset: usize) -> Result<u8, FontError> {
        Ok(self.bytes(offset, 1)?[0])
    }

    pub fn i8(&self, offset: usize) -> Result<i8, FontError> {
        Ok(self.u8(offset)? as i8)
    }

    pub fn u16(&self, offset: usize) -> Result<u16, FontError> {
        let b = self.bytes(offset, 2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn i16(&self, offset: usize) -> Result<i16, FontError> {
        Ok(self.u16(offset)? as i16)
    }

    pub fn u32(&self, offset: usize) -> Result<u32, FontError> {
        let b = self.bytes(offset, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// F2DOT14 fixed point.
    pub fn f2dot14(&self, offset: usize) -> Result<f32, FontError> {
        Ok(self.i16(offset)? as f32 / 16384.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_bounds_checked() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9a];
        let v = View::new(&data, 100, Tag(*b"test"));
        assert_eq!(v.u16(0).unwrap(), 0x1234);
        assert_eq!(v.u32(1).unwrap(), 0x3456789a);
        assert_eq!(v.i16(3).unwrap(), 0x789a_u16 as i16);
        let e = v.u32(2).unwrap_err();
        assert_eq!(e.offset, 102);
        assert_eq!(e.reason, Reason::Truncated { needed: 4, available: 3 });
        assert!(v.u8(usize::MAX).is_err());
        assert!(v.bytes(usize::MAX, 2).is_err());
        assert!(v.sub(4, 2).is_err());
        assert_eq!(v.sub(1, 2).unwrap().u16(0).unwrap(), 0x3456);
        assert_eq!(v.sub(1, 2).unwrap().base, 101);
    }
}
