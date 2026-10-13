//! Colours: sRGB, straight alpha, `[r, g, b, a]` in `0..=1` (the same convention as
//! `specula_text::GlyphQuad`). Parsing from `#rgb` / `#rrggbb` / `#rrggbbaa`, WCAG luminance and
//! contrast, and "over" compositing.
//!
//! **Blending happens in linear light**, as on the GPU (the overlay converts sRGB inputs to
//! linear and blends into `*_SRGB` targets). [`over`], the software preview and `ui_check` all
//! do the same, so measured contrast is what players see. Conversions use
//! `specula_core::dmath::pow`, so results are bit-identical on every platform.

/// sRGB, straight (non-premultiplied) alpha.
pub type Rgba = [f32; 4];

pub const TRANSPARENT: Rgba = [0.0, 0.0, 0.0, 0.0];

/// Parse `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let h = s.strip_prefix('#')?;
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nib = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    let v: [u8; 4] = match h.len() {
        3 => [nib(0)?, nib(1)?, nib(2)?, 255],
        4 => [nib(0)?, nib(1)?, nib(2)?, nib(3)?],
        6 => [byte(0)?, byte(2)?, byte(4)?, 255],
        8 => [byte(0)?, byte(2)?, byte(4)?, byte(6)?],
        _ => return None,
    };
    Some(v.map(|c| f32::from(c) / 255.0))
}

/// `#rrggbbaa`, for reports.
pub fn to_hex(c: Rgba) -> String {
    let b = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
}

/// sRGB-encoded channel → linear light.
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        specula_core::dmath::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// Linear light → sRGB-encoded channel.
pub fn to_srgb(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * specula_core::dmath::pow(l, 1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    to_linear(c)
}

/// WCAG 2 relative luminance of an opaque sRGB colour (alpha ignored).
pub fn luminance(c: Rgba) -> f32 {
    0.2126 * srgb_to_linear(c[0]) + 0.7152 * srgb_to_linear(c[1]) + 0.0722 * srgb_to_linear(c[2])
}

/// WCAG 2 contrast ratio between two opaque colours, `1..=21`.
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `top` composited over an opaque `bottom`, blended in linear light (as the GPU overlay does).
pub fn over(top: Rgba, bottom: Rgba) -> Rgba {
    let a = top[3].clamp(0.0, 1.0);
    let ch = |t: f32, b: f32| to_srgb(to_linear(t) * a + to_linear(b) * (1.0 - a));
    [ch(top[0], bottom[0]), ch(top[1], bottom[1]), ch(top[2], bottom[2]), 1.0]
}

/// Interpolate two sRGB colours in linear light (how GPU gradients between sRGB endpoints look).
pub fn mix_linear(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let ch = |x: f32, y: f32| to_srgb(to_linear(x) + (to_linear(y) - to_linear(x)) * t);
    [ch(a[0], b[0]), ch(a[1], b[1]), ch(a[2], b[2]), a[3] + (b[3] - a[3]) * t]
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

pub fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(parse_hex("#fff"), Some([1.0; 4]));
        assert_eq!(parse_hex("#00000080").map(|c| (c[3] * 255.0).round()), Some(128.0));
        assert_eq!(parse_hex("#3fe0d0").map(to_hex).as_deref(), Some("#3fe0d0ff"));
        assert_eq!(parse_hex("3fe0d0"), None);
        assert_eq!(parse_hex("#3fe0d"), None);
        assert_eq!(parse_hex("#ggg"), None);
    }

    #[test]
    fn wcag_contrast_matches_reference_values() {
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        assert!((contrast_ratio(white, black) - 21.0).abs() < 1e-3);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-6);
        // #767676 on white is the classic 4.54:1.
        let grey = parse_hex("#767676").unwrap();
        assert!((contrast_ratio(grey, white) - 4.54).abs() < 0.01);
    }

    #[test]
    fn over_blends_by_alpha_in_linear_light() {
        let c = over([1.0, 0.0, 0.0, 0.5], [0.0, 0.0, 1.0, 1.0]);
        // Half of linear 1.0 is 0.5 linear, which is 0.735 in sRGB (not 0.5).
        assert!((c[0] - 0.7354).abs() < 1e-3 && (c[2] - 0.7354).abs() < 1e-3 && c[1] == 0.0, "{c:?}");
        let o = over([0.2, 0.4, 0.6, 1.0], [1.0; 4]);
        assert_eq!(&o[..3], &[0.2f32, 0.4, 0.6].map(|v| to_srgb(to_linear(v)))[..]);
        for k in 0..=255 {
            let v = k as f32 / 255.0;
            assert!((to_srgb(to_linear(v)) - v).abs() < 1e-5, "{k}");
        }
    }
}
