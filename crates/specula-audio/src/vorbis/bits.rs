//! Vorbis's bit packer: LSB-first within each byte, values up to 32 bits.
//!
//! Reading past the end of a packet sets the end-of-packet flag and returns zeros, as the spec
//! asks ("an end-of-packet condition"): header readers turn it into an error, audio decoding
//! treats it as the nominal end of the data.

#[derive(Clone, Debug)]
pub(crate) struct Bits<'a> {
    data: &'a [u8],
    /// Bit position.
    pos: usize,
    eop: bool,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Bits<'a> {
        Bits { data, pos: 0, eop: false }
    }

    /// Bits left in the packet.
    #[inline]
    pub fn remaining(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.pos)
    }

    /// The end of the packet was reached by a read.
    pub fn eop(&self) -> bool {
        self.eop
    }

    /// The next 32 bits (zero past the end) without consuming them.
    #[inline]
    pub fn peek32(&self) -> u32 {
        let byte = self.pos >> 3;
        let shift = (self.pos & 7) as u32;
        let mut buf = [0u8; 8];
        if let Some(s) = self.data.get(byte..) {
            let n = s.len().min(8);
            buf[..n].copy_from_slice(&s[..n]);
        }
        (u64::from_le_bytes(buf) >> shift) as u32
    }

    /// Consumes `n` bits (the caller checked they exist).
    #[inline]
    pub fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// Reads `n` bits (0..=32) as an unsigned value.
    #[inline]
    pub fn read(&mut self, n: u32) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        if self.remaining() < n as usize {
            self.eop = true;
            self.pos = self.data.len() * 8;
            return 0;
        }
        let v = self.peek32();
        self.pos += n as usize;
        if n == 32 {
            v
        } else {
            v & ((1u32 << n) - 1)
        }
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.read(1) != 0
    }

    /// Marks the packet as ended (an undecodable codeword).
    pub fn set_eop(&mut self) {
        self.eop = true;
        self.pos = self.data.len() * 8;
    }
}

/// The number of bits needed to hold `x` (`ilog` in the spec): 0 → 0, 1 → 1, 7 → 3, 8 → 4.
pub(crate) fn ilog(x: u32) -> u32 {
    32 - x.leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lsb_first_and_flags_the_end() {
        let mut b = Bits::new(&[0b1010_1100, 0xff]);
        assert_eq!(b.read(2), 0b00);
        assert_eq!(b.read(3), 0b011);
        assert_eq!(b.read(5), 0b11_101);
        assert_eq!(b.read(6), 0b11_1111);
        assert!(!b.eop());
        assert_eq!(b.read(1), 0);
        assert!(b.eop());
        let mut b = Bits::new(&[1, 2, 3, 4, 5]);
        assert_eq!(b.read(32), 0x0403_0201);
        assert_eq!(ilog(0), 0);
        assert_eq!(ilog(1), 1);
        assert_eq!(ilog(7), 3);
        assert_eq!(ilog(8), 4);
    }
}
