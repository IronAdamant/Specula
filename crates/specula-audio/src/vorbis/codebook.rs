//! Vorbis codebooks (spec section 3): codeword lengths, the canonical Huffman assignment, and
//! VQ lookup tables of types 1 and 2.

use super::bits::{ilog, Bits};
use super::{header_err, Limits};
use crate::error::Result;

/// Codewords up to this length decode with one table lookup.
const FAST_BITS: u32 = 10;
const FAST_SIZE: usize = 1 << FAST_BITS;
/// Most entries a codebook may declare (the format allows 2^24; real encoders use a few
/// thousand).
pub(crate) const MAX_ENTRIES: u32 = 1 << 20;
/// Most VQ values (entries × dimensions) a codebook may expand to.
pub(crate) const MAX_VALUES: usize = 1 << 21;

#[derive(Clone, Debug)]
pub(crate) struct Codebook {
    pub dims: usize,
    /// `(entry, length)` for codewords of up to [`FAST_BITS`] bits, indexed by the next bits of
    /// the stream; `length == 0` means "not here, search the long codes".
    fast: Vec<(u32, u8)>,
    /// Longer codewords: `(codeword MSB-aligned, length, entry)`, sorted by codeword.
    long: Vec<(u32, u8, u32)>,
    /// The only used entry of a one-entry codebook and its length.
    single: Option<(u32, u8)>,
    /// `entries × dims` VQ values (empty without a lookup table).
    pub values: Vec<f32>,
}

/// The spec's `float32_unpack`.
pub(crate) fn float32_unpack(x: u32) -> f32 {
    let mantissa = (x & 0x1f_ffff) as f64;
    let exponent = ((x & 0x7fe0_0000) >> 21) as i32 - 788;
    // 2^exponent exactly (exponent is in -788..=235, a normal f64).
    let scale = f64::from_bits(((exponent + 1023) as u64) << 52);
    let v = mantissa * scale;
    (if x & 0x8000_0000 != 0 { -v } else { v }) as f32
}

/// The greatest `r` with `r^dims <= entries` (the spec's `lookup1_values`).
pub(crate) fn lookup1_values(entries: u32, dims: u32) -> u32 {
    if dims == 0 {
        return 0;
    }
    let fits = |r: u64| {
        let mut acc: u64 = 1;
        for _ in 0..dims {
            acc = match acc.checked_mul(r) {
                Some(a) if a <= entries as u64 => a,
                _ => return false,
            };
        }
        true
    };
    // Binary search: r is at most `entries` (dims >= 1).
    let (mut lo, mut hi) = (0u64, entries as u64);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo as u32
}

impl Codebook {
    /// Reads one codebook from the setup header.
    pub fn read(b: &mut Bits, index: usize, limits: &mut Limits) -> Result<Codebook> {
        let path = || format!("setup.codebooks[{index}]");
        if b.read(24) != 0x56_4342 {
            return Err(header_err(path(), "codebook sync pattern missing"));
        }
        let dims = b.read(16);
        let entries = b.read(24);
        if b.eop() {
            return Err(header_err(path(), "setup header ends inside a codebook"));
        }
        if dims == 0 && entries > 0 {
            return Err(header_err(path(), "a codebook with entries needs at least one dimension"));
        }
        if entries > MAX_ENTRIES {
            return Err(header_err(path(), "codebook has more entries than the decoder accepts").with_numbers(entries as f64, MAX_ENTRIES as f64));
        }
        limits.charge(entries as usize, path)?;
        let mut lengths = vec![0u8; entries as usize];
        let ordered = b.bit();
        if !ordered {
            let sparse = b.bit();
            for l in lengths.iter_mut() {
                if !sparse || b.bit() {
                    *l = (b.read(5) + 1) as u8;
                }
                if b.eop() {
                    return Err(header_err(path(), "setup header ends inside codeword lengths"));
                }
            }
        } else {
            let mut current = 0u32;
            let mut length = b.read(5) + 1;
            while current < entries {
                let number = b.read(ilog(entries - current));
                if b.eop() || length > 32 || current + number > entries {
                    return Err(header_err(path(), "bad ordered codeword lengths"));
                }
                for l in &mut lengths[current as usize..(current + number) as usize] {
                    *l = length as u8;
                }
                current += number;
                length += 1;
            }
        }
        let mut cb = Codebook { dims: dims as usize, fast: Vec::new(), long: Vec::new(), single: None, values: Vec::new() };
        cb.build_huffman(&lengths).map_err(|m| header_err(path(), m))?;

        let lookup = b.read(4);
        match lookup {
            0 => {}
            1 | 2 => {
                let min = float32_unpack(b.read(32));
                let delta = float32_unpack(b.read(32));
                let value_bits = b.read(4) + 1;
                let sequence = b.bit();
                let lookup_values = if lookup == 1 { lookup1_values(entries, dims) as usize } else { entries as usize * dims as usize };
                let total = entries as usize * dims as usize;
                if total > MAX_VALUES || lookup_values > MAX_VALUES {
                    return Err(header_err(path(), "codebook VQ table is larger than the decoder accepts").with_numbers(total.max(lookup_values) as f64, MAX_VALUES as f64));
                }
                limits.charge(lookup_values + total, path)?;
                let mut mult = Vec::with_capacity(lookup_values);
                for _ in 0..lookup_values {
                    mult.push(b.read(value_bits) as f32);
                }
                if b.eop() {
                    return Err(header_err(path(), "setup header ends inside VQ multiplicands"));
                }
                let mut values = vec![0f32; total];
                let d = dims as usize;
                for e in 0..entries as usize {
                    if lengths[e] == 0 {
                        continue;
                    }
                    let mut last = 0f32;
                    let out = &mut values[e * d..(e + 1) * d];
                    if lookup == 1 {
                        let mut divisor = 1usize;
                        for v in out.iter_mut() {
                            let off = if lookup_values == 0 { 0 } else { (e / divisor) % lookup_values };
                            let x = mult.get(off).copied().unwrap_or(0.0) * delta + min + last;
                            *v = x;
                            if sequence {
                                last = x;
                            }
                            divisor = divisor.saturating_mul(lookup_values.max(1));
                        }
                    } else {
                        for (j, v) in out.iter_mut().enumerate() {
                            let x = mult[e * d + j] * delta + min + last;
                            *v = x;
                            if sequence {
                                last = x;
                            }
                        }
                    }
                }
                cb.values = values;
            }
            other => return Err(header_err(path(), "unknown codebook lookup type").with_numbers(other as f64, 2.0)),
        }
        Ok(cb)
    }

    /// Assigns codewords the way the spec does (each entry takes the lowest free codeword of its
    /// length) and builds the decode tables.
    fn build_huffman(&mut self, lengths: &[u8]) -> std::result::Result<(), String> {
        self.fast = vec![(0, 0); FAST_SIZE];
        let used = lengths.iter().filter(|&&l| l > 0).count();
        if used == 0 {
            return Ok(());
        }
        if used == 1 {
            let e = lengths.iter().position(|&l| l > 0).expect("one used entry");
            self.single = Some((e as u32, lengths[e]));
            return Ok(());
        }
        // available[l]: the next free codeword of length l, MSB-aligned in 32 bits (0 = none).
        let mut available = [0u32; 33];
        let first = lengths.iter().position(|&l| l > 0).expect("an entry");
        let l0 = lengths[first] as usize;
        self.add(0, l0 as u8, first as u32);
        for (i, slot) in available.iter_mut().enumerate().take(l0 + 1).skip(1) {
            *slot = 1u32.checked_shl(32 - i as u32).unwrap_or(0);
        }
        for (e, &len) in lengths.iter().enumerate().skip(first + 1) {
            if len == 0 {
                continue;
            }
            let mut z = len as usize;
            while z > 0 && available[z] == 0 {
                z -= 1;
            }
            if z == 0 {
                return Err("codeword lengths over-specify the Huffman tree".into());
            }
            let res = available[z];
            available[z] = 0;
            self.add(res, len, e as u32);
            let mut y = len as usize;
            while y > z {
                available[y] = res + 1u32.checked_shl(32 - y as u32).unwrap_or(0);
                y -= 1;
            }
        }
        self.long.sort_unstable_by_key(|&(c, _, _)| c);
        Ok(())
    }

    fn add(&mut self, msb_code: u32, len: u8, entry: u32) {
        if len as u32 <= FAST_BITS {
            // The stream delivers the codeword's MSB first in its lowest bit: reverse it.
            let rev = (msb_code.reverse_bits() & ((1u32 << len) - 1)) as usize;
            let mut i = rev;
            while i < FAST_SIZE {
                self.fast[i] = (entry, len);
                i += 1 << len;
            }
        } else {
            self.long.push((msb_code, len, entry));
        }
    }

    /// Decodes one entry number; `None` (and the packet marked ended) when the bits don't form a
    /// codeword or the packet runs out.
    #[inline]
    pub fn decode(&self, b: &mut Bits) -> Option<u32> {
        if let Some((e, len)) = self.single {
            if b.remaining() < len as usize {
                b.set_eop();
                return None;
            }
            b.skip(len as usize);
            return Some(e);
        }
        let peek = b.peek32();
        let (e, len) = self.fast[(peek as usize) & (FAST_SIZE - 1)];
        if len > 0 {
            if b.remaining() < len as usize {
                b.set_eop();
                return None;
            }
            b.skip(len as usize);
            return Some(e);
        }
        // Long codes: the stream bits, MSB-aligned, fall at or after their codeword.
        let x = peek.reverse_bits();
        let i = self.long.partition_point(|&(c, _, _)| c <= x);
        if i > 0 {
            let (c, len, e) = self.long[i - 1];
            let mask = if len >= 32 { u32::MAX } else { !(u32::MAX >> len) };
            if (x & mask) == c && b.remaining() >= len as usize {
                b.skip(len as usize);
                return Some(e);
            }
        }
        b.set_eop();
        None
    }

    /// Decodes one VQ vector (`dims` values); `None` at the end of the packet.
    #[inline]
    pub fn decode_vector(&self, b: &mut Bits) -> Option<&[f32]> {
        let e = self.decode(b)? as usize;
        self.values.get(e * self.dims..(e + 1) * self.dims)
    }

    pub fn has_lookup(&self) -> bool {
        !self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codewords_follow_the_spec_example() {
        // The spec's example (section 3.2.1): lengths 2 4 4 4 4 2 3 3 give codewords
        // 00 0100 0101 0110 0111 10 110 111.
        let mut cb = Codebook { dims: 1, fast: Vec::new(), long: Vec::new(), single: None, values: Vec::new() };
        cb.build_huffman(&[2, 4, 4, 4, 4, 2, 3, 3]).unwrap();
        let codes = ["00", "0100", "0101", "0110", "0111", "10", "110", "111"];
        // Pack every codeword MSB-first into an LSB-first stream.
        let mut bits = Vec::new();
        for c in codes {
            bits.extend(c.chars().map(|ch| ch == '1'));
        }
        let mut bytes = vec![0u8; bits.len().div_ceil(8)];
        for (i, &x) in bits.iter().enumerate() {
            if x {
                bytes[i / 8] |= 1 << (i % 8);
            }
        }
        let mut b = Bits::new(&bytes);
        for e in 0..8 {
            assert_eq!(cb.decode(&mut b), Some(e));
        }
    }

    #[test]
    fn long_codes_and_overspecified_trees() {
        // 14 entries of lengths 1..=13 plus one more 13: a complete tree with long codes.
        let mut lens: Vec<u8> = (1..=13).collect();
        lens.push(13);
        let mut cb = Codebook { dims: 1, fast: Vec::new(), long: Vec::new(), single: None, values: Vec::new() };
        cb.build_huffman(&lens).unwrap();
        // Entry 12 (length 13): codeword 1111111111110.
        let mut bytes = [0u8; 2];
        for i in 0..12 {
            bytes[i / 8] |= 1 << (i % 8);
        }
        assert_eq!(cb.decode(&mut Bits::new(&bytes)), Some(12));
        let mut over = Codebook { dims: 1, fast: Vec::new(), long: Vec::new(), single: None, values: Vec::new() };
        assert!(over.build_huffman(&[1, 1, 1]).is_err());
    }

    #[test]
    fn float_and_lookup_helpers() {
        assert_eq!(float32_unpack(0), 0.0);
        // mantissa 1, exponent 788 → 1.0
        assert_eq!(float32_unpack(1 | (788 << 21)), 1.0);
        assert_eq!(float32_unpack(0x8000_0000 | 3 | (789 << 21)), -6.0);
        assert_eq!(lookup1_values(81, 4), 3);
        assert_eq!(lookup1_values(80, 4), 2);
        assert_eq!(lookup1_values(1 << 20, 1), 1 << 20);
        assert_eq!(lookup1_values(u32::MAX >> 8, 1), u32::MAX >> 8);
    }
}
