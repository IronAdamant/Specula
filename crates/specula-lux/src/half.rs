//! IEEE half floats, as the GPU's `unpackHalf2x16` reads them: the CPU packs, the shader and
//! [`crate::field`]'s sampler decode the same bits.

/// An `f32` as half-float bits (round to nearest even; NaN stays NaN, overflow → infinity).
pub fn to_bits(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mant = x & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        // Subnormal: the mantissa with its implicit 1, shifted into place, rounded to even.
        let m = mant | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let rest = m & ((1 << shift) - 1);
        let mut r = m >> shift;
        if rest > half || (rest == half && r & 1 == 1) {
            r += 1;
        }
        return sign | r as u16;
    }
    let mut r = ((e as u32) << 10) | (mant >> 13);
    let rest = mant & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && r & 1 == 1) {
        r += 1; // may carry into the exponent: still correct (up to infinity)
    }
    sign | r as u16
}

/// Half-float bits as an `f32` (exact: every half is an `f32`).
pub fn from_bits(h: u16) -> f32 {
    let sign = u32::from(h & 0x8000) << 16;
    let exp = u32::from((h >> 10) & 0x1f);
    let mant = u32::from(h & 0x3ff);
    let bits = if exp == 0 {
        // Zero or subnormal: mant × 2⁻²⁴, exact in f32.
        let v = mant as f32 * (1.0 / 16_777_216.0);
        sign | v.to_bits()
    } else if exp == 0x1f {
        sign | 0x7f80_0000 | (mant << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// Two values in one word, `a` in the low half (`unpackHalf2x16(w).x`).
pub fn pack2(a: f32, b: f32) -> u32 {
    u32::from(to_bits(a)) | (u32::from(to_bits(b)) << 16)
}

/// [`pack2`]'s inverse.
pub fn unpack2(w: u32) -> (f32, f32) {
    (from_bits(w as u16), from_bits((w >> 16) as u16))
}

/// `v` rounded to the nearest half (what the GPU will read).
pub fn round(v: f32) -> f32 {
    from_bits(to_bits(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_trip_and_round_to_even() {
        for v in [0.0f32, 1.0, -2.5, 0.333_333_34, 65504.0, 6.1e-5, 1e-7, 3.0e-6] {
            let r = round(v);
            assert!((r - v).abs() <= v.abs() * 1e-3 + 6e-8, "{v} -> {r}");
            assert_eq!(round(r), r, "a half is exact");
        }
        assert_eq!(to_bits(1.0), 0x3c00);
        assert_eq!(to_bits(-2.0), 0xc000);
        assert_eq!(to_bits(65520.0), 0x7c00, "overflow is infinity");
        assert_eq!(from_bits(0x0001), 5.960_464_5e-8, "the smallest subnormal");
        // 1 + 2^-11 is halfway between two halves: ties go to the even one (1.0).
        assert_eq!(to_bits(1.0 + 1.0 / 2048.0), 0x3c00);
        assert_eq!(to_bits(1.0 + 3.0 / 2048.0), 0x3c02);
        let (a, b) = unpack2(pack2(0.5, -4.0));
        assert_eq!((a, b), (0.5, -4.0));
    }

    #[test]
    fn every_half_decodes_to_itself() {
        for h in 0..=u16::MAX {
            let v = from_bits(h);
            if v.is_nan() {
                continue;
            }
            assert_eq!(to_bits(v), h, "{h:#06x}");
        }
    }
}
