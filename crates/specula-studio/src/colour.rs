//! Colour maths for the studio's measures: sRGB ↔ linear, Oklab (perceptual distance and
//! lightness), HSV saturation, and the nearest point on a palette blend.
//!
//! Colours are sRGB in 0..1 unless a name says otherwise (`*8`: 8-bit). Distances are Oklab
//! Euclidean ("ΔE ok"): about 0.02 is a just-visible step between flat swatches.

/// sRGB 0..1 per channel.
pub type Rgb = [f32; 3];

/// One sRGB channel (0..1) to linear light.
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// 8-bit sRGB to 0..1 sRGB.
pub fn from8(c: [u8; 3]) -> Rgb {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

/// Oklab `[L, a, b]` of an sRGB colour (Björn Ottosson, 2020). `L` is perceptual lightness
/// 0 (black) .. 1 (white).
pub fn oklab(c: Rgb) -> [f32; 3] {
    let [r, g, b] = c.map(|x| to_linear(x.clamp(0.0, 1.0)));
    let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Oklab lightness of an 8-bit sRGB pixel, through a 256-entry table (images are measured
/// pixel by pixel). Grey pixels are exact; coloured ones use the luminance-weighted mean.
pub fn lightness8(r: u8, g: u8, b: u8) -> f32 {
    // Relative luminance, then Oklab's L for that grey (L = cbrt(Y) for a neutral).
    let lin = linear_table();
    let y = 0.2126 * lin[r as usize] + 0.7152 * lin[g as usize] + 0.0722 * lin[b as usize];
    y.max(0.0).cbrt()
}

/// 8-bit sRGB to linear light, by table.
pub fn linear8() -> &'static [f32; 256] {
    linear_table()
}

fn linear_table() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            *v = to_linear(i as f32 / 255.0);
        }
        t
    })
}

/// Perceptual distance between two sRGB colours (Oklab Euclidean).
pub fn delta_e(a: Rgb, b: Rgb) -> f32 {
    let (p, q) = (oklab(a), oklab(b));
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

/// HSV saturation `(max - min) / max` (0 for black). Darkening a colour (shading towards black)
/// leaves it unchanged.
pub fn saturation(c: Rgb) -> f32 {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    if max <= 1e-6 {
        0.0
    } else {
        (max - min) / max
    }
}

/// Oklch chroma: how colourful a colour looks, independent of its lightness's own scale (near
/// blacks stay near 0, where HSV saturation can read 60%). Shading towards black only lowers it.
/// About 0.05 is a "25% saturated" mid-tone; the palette's glows are 0.1-0.23.
pub fn chroma(c: Rgb) -> f32 {
    let [_, a, b] = oklab(c);
    (a * a + b * b).sqrt()
}

/// `#rrggbb` (or `rrggbb`) to sRGB 0..1.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(from8([(v >> 16) as u8, (v >> 8) as u8, v as u8]))
}

/// `#rrggbb` of an sRGB 0..1 colour.
pub fn hex(c: Rgb) -> String {
    let b = c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{:02x}{:02x}{:02x}", b[0], b[1], b[2])
}

/// The point of segment `a`–`b` (in sRGB space, where recipes blend their colours) nearest to
/// `p`, and its parameter 0..1 along the segment.
pub fn nearest_on_segment(p: Rgb, a: Rgb, b: Rgb) -> (f32, Rgb) {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let t = if len2 <= 1e-12 { 0.0 } else { (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1] + (p[2] - a[2]) * d[2]) / len2).clamp(0.0, 1.0) };
    (t, [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_of_white_black_and_grey() {
        let w = oklab([1.0; 3]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3 && w[2].abs() < 1e-3, "{w:?}");
        assert!(oklab([0.0; 3])[0].abs() < 1e-6);
        // 18% grey (#767676) is about L 0.57.
        let g = oklab(parse_hex("#767676").unwrap());
        assert!((g[0] - 0.566).abs() < 0.01, "{g:?}");
        // The table path agrees with the exact one for greys.
        assert!((lightness8(0x76, 0x76, 0x76) - g[0]).abs() < 2e-3);
    }

    #[test]
    fn saturation_ignores_darkening() {
        let moss = parse_hex("#4f6b3a").unwrap();
        let dark = moss.map(|c| c * 0.4);
        assert!((saturation(moss) - saturation(dark)).abs() < 1e-5);
        assert!((saturation(moss) - 0.458).abs() < 0.01);
        assert_eq!(saturation([0.0; 3]), 0.0);
        assert!(saturation(parse_hex("#5b6470").unwrap()) < 0.25);
        // The night sky is 66% "saturated" in HSV but nearly black: its chroma is low.
        let sky = parse_hex("#0b1020").unwrap();
        assert!(saturation(sky) > 0.6 && chroma(sky) < 0.04);
        assert!(chroma(moss) > 0.07 && chroma(dark) < chroma(moss));
    }

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        assert_eq!(hex(parse_hex("#3fe0d0").unwrap()), "#3fe0d0");
        assert!(parse_hex("#3fe0d").is_none() && parse_hex("#zzzzzz").is_none() && parse_hex("").is_none());
    }

    #[test]
    fn nearest_point_on_a_blend() {
        let (a, b) = ([0.2, 0.2, 0.2], [0.6, 0.6, 0.6]);
        let (t, q) = nearest_on_segment([0.4, 0.4, 0.4], a, b);
        assert!((t - 0.5).abs() < 1e-6 && (q[0] - 0.4).abs() < 1e-6);
        assert_eq!(nearest_on_segment([0.9, 0.9, 0.9], a, b).0, 1.0);
        assert_eq!(nearest_on_segment([0.1; 3], a, a).0, 0.0);
        assert!(delta_e([0.4; 3], [0.4; 3]) < 1e-6);
    }
}
