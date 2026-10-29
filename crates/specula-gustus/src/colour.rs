//! Colour maths with `dmath` only: sRGB (0..1, gamma encoded) <-> linear <-> Oklab <-> Oklch.
//! Oklab is Björn Ottosson's (2020); distances in it are roughly perceptual.

use specula_core::dmath;

/// sRGB, each channel 0..1, gamma encoded.
pub type Rgb = [f64; 3];

/// One sRGB channel to linear light.
pub fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        pow((c + 0.055) / 1.055, 2.4)
    }
}

/// One linear channel to sRGB.
pub fn from_linear(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * pow(c, 1.0 / 2.4) - 0.055
    }
}

/// `x^y` for `x > 0` (0 otherwise).
pub fn pow(x: f64, y: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        dmath::exp_f64(y * dmath::ln_f64(x))
    }
}

fn cbrt(x: f64) -> f64 {
    if x == 0.0 {
        0.0
    } else if x < 0.0 {
        -pow(-x, 1.0 / 3.0)
    } else {
        pow(x, 1.0 / 3.0)
    }
}

/// Linear sRGB to Oklab `[L, a, b]`.
pub fn linear_to_oklab(c: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = c;
    let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let (l, m, s) = (cbrt(l), cbrt(m), cbrt(s));
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// Oklab to linear sRGB (may be outside 0..1: out of gamut).
pub fn oklab_to_linear(c: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = c;
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
}

pub fn srgb_to_oklab(c: Rgb) -> [f64; 3] {
    linear_to_oklab(c.map(to_linear))
}

/// Oklab to sRGB, clamped to 0..1 (callers wanting the gamut kept use [`oklch_to_srgb`]).
pub fn oklab_to_srgb(c: [f64; 3]) -> Rgb {
    oklab_to_linear(c).map(|v| from_linear(v.clamp(0.0, 1.0)))
}

/// Oklab to Oklch `[L, C, h°]` (h in 0..360).
pub fn oklab_to_oklch(c: [f64; 3]) -> [f64; 3] {
    let chroma = (c[1] * c[1] + c[2] * c[2]).sqrt();
    let mut h = dmath::atan2_f64(c[2], c[1]).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    [c[0], chroma, if chroma < 1e-9 { 0.0 } else { h }]
}

pub fn oklch_to_oklab(c: [f64; 3]) -> [f64; 3] {
    let (s, co) = dmath::sin_cos_f64(c[2].to_radians());
    [c[0], c[1] * co, c[1] * s]
}

fn in_gamut(lin: [f64; 3]) -> bool {
    lin.iter().all(|v| (-1e-6..=1.0 + 1e-6).contains(v))
}

/// Oklch to sRGB, keeping lightness and hue: chroma is reduced (bisection) until the colour is
/// inside the sRGB gamut.
pub fn oklch_to_srgb(c: [f64; 3]) -> Rgb {
    let l = c[0].clamp(0.0, 1.0);
    let lin = oklab_to_linear(oklch_to_oklab([l, c[1].max(0.0), c[2]]));
    if in_gamut(lin) {
        return lin.map(|v| from_linear(v.clamp(0.0, 1.0)));
    }
    let (mut lo, mut hi) = (0.0, c[1].max(0.0));
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if in_gamut(oklab_to_linear(oklch_to_oklab([l, mid, c[2]]))) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    oklab_to_linear(oklch_to_oklab([l, lo, c[2]])).map(|v| from_linear(v.clamp(0.0, 1.0)))
}

pub fn srgb_to_oklch(c: Rgb) -> [f64; 3] {
    oklab_to_oklch(srgb_to_oklab(c))
}

/// Oklab distance between two sRGB colours.
pub fn delta_e(a: Rgb, b: Rgb) -> f64 {
    lab_distance(srgb_to_oklab(a), srgb_to_oklab(b))
}

/// Euclidean distance between two Oklab colours.
pub fn lab_distance(x: [f64; 3], y: [f64; 3]) -> f64 {
    let d = [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// `#rrggbb` (or `#rgb`) to sRGB 0..1.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#')?;
    let v: Vec<u8> = match h.len() {
        6 => (0..3).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()).collect::<Option<_>>()?,
        3 => h.chars().map(|c| c.to_digit(16).map(|d| (d * 17) as u8)).collect::<Option<_>>()?,
        _ => return None,
    };
    Some([f64::from(v[0]) / 255.0, f64::from(v[1]) / 255.0, f64::from(v[2]) / 255.0])
}

/// sRGB 0..1 to `#rrggbb`.
pub fn hex(c: Rgb) -> String {
    let b = c.map(|v| dmath::round_f64(v.clamp(0.0, 1.0) * 255.0) as u8);
    format!("#{:02x}{:02x}{:02x}", b[0], b[1], b[2])
}

/// The linear value of each 8-bit sRGB level.
pub fn linear_table() -> [f64; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() {
        *v = to_linear(i as f64 / 255.0);
    }
    t
}

/// Shortest signed hue difference `to - from` in degrees (-180..180).
pub fn hue_delta(from: f64, to: f64) -> f64 {
    let mut d = (to - from) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for hx in ["#d0d9f0", "#2b3038", "#ffd27a", "#3fe0d0", "#000000", "#ffffff"] {
            let c = parse_hex(hx).unwrap();
            assert_eq!(hex(oklch_to_srgb(srgb_to_oklch(c))), hx);
        }
        // White is L 1, grey has no chroma.
        let w = srgb_to_oklab([1.0; 3]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3);
        assert!(srgb_to_oklch([0.5; 3])[1] < 1e-3);
    }

    #[test]
    fn out_of_gamut_keeps_lightness_and_hue() {
        let c = oklch_to_srgb([0.7, 0.5, 140.0]);
        let back = srgb_to_oklch(c);
        assert!((back[0] - 0.7).abs() < 0.01, "{back:?}");
        assert!(hue_delta(back[2], 140.0).abs() < 2.0, "{back:?}");
        assert!(back[1] < 0.5);
    }

    #[test]
    fn hue_deltas_wrap() {
        assert_eq!(hue_delta(350.0, 10.0), 20.0);
        assert_eq!(hue_delta(10.0, 350.0), -20.0);
        assert!(delta_e([0.0; 3], [1.0; 3]) > 0.99);
    }
}
