//! sRGB ↔ linear conversion, deterministic on every platform (foundation F2): the tables are
//! built with `specula_core::dmath::pow` (correctly rounded) and lookups are comparisons.
//!
//! glTF vertex colours and colour factors are **linear**; Specula's `MeshData::colors` and
//! material presets are **sRGB**. Linear → sRGB8 rounds in sRGB space (the code whose
//! `(k ± 0.5) / 255` interval holds the value), so every table value maps back to its own code:
//! export → import of sRGB8 colours is exact.

use std::sync::OnceLock;

use specula_core::dmath;

fn decode(s: f32) -> f32 {
    if s <= 0.040_45 {
        s / 12.92
    } else {
        dmath::pow((s + 0.055) / 1.055, 2.4)
    }
}

struct Tables {
    linear: [f32; 256],
    /// `thresholds[k]` = linear value of sRGB `(k + 0.5) / 255`: codes above `k` start there.
    thresholds: [f32; 255],
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Tables { linear: [0.0; 256], thresholds: [0.0; 255] };
        for (k, v) in t.linear.iter_mut().enumerate() {
            *v = decode(k as f32 / 255.0);
        }
        for (k, v) in t.thresholds.iter_mut().enumerate() {
            *v = decode((k as f32 + 0.5) / 255.0);
        }
        t
    })
}

/// Linear value of an sRGB8 code.
pub fn srgb8_to_linear(c: u8) -> f32 {
    tables().linear[c as usize]
}

/// The sRGB8 code of a linear value (clamped to 0..1; NaN → 0).
pub fn linear_to_srgb8(x: f32) -> u8 {
    // NaN fails this test too.
    if x.is_nan() || x <= 0.0 {
        return 0;
    }
    tables().thresholds.partition_point(|&t| t <= x) as u8
}

/// sRGB 0..1 of a linear value (via the 8-bit code: presets are 8-bit colours).
pub fn linear_to_srgb_unit(x: f32) -> f32 {
    linear_to_srgb8(x) as f32 / 255.0
}

/// A linear (alpha, normalized) value in 0..1 to unorm8, rounded to nearest.
pub fn unorm8(x: f32) -> u8 {
    if x.is_nan() || x <= 0.0 {
        return 0;
    }
    dmath::round(x.min(1.0) * 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_are_monotone() {
        for c in 0..=255u8 {
            assert_eq!(linear_to_srgb8(srgb8_to_linear(c)), c);
        }
        assert_eq!(linear_to_srgb8(-1.0), 0);
        assert_eq!(linear_to_srgb8(f32::NAN), 0);
        assert_eq!(linear_to_srgb8(2.0), 255);
        assert_eq!(linear_to_srgb8(0.5), 188); // 0.5 linear ≈ 0.7354 sRGB
        assert_eq!(linear_to_srgb8(0.2159), 128);
        let mut prev = 0;
        for i in 0..=1000 {
            let c = linear_to_srgb8(i as f32 / 1000.0);
            assert!(c >= prev);
            prev = c;
        }
        assert_eq!((unorm8(0.5), unorm8(1.5), unorm8(f32::NAN)), (128, 255, 0));
    }
}
