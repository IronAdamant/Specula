//! Colour-vision differences, simulated, so a theme can be checked for the players it is for
//! (docs/design/ACCESSIBILITY.md): [`simulate`] shows a colour as a person with a deficiency sees
//! it (Machado, Oliveira and Fernandes 2009, full severity, in linear light), [`delta_e`] says how
//! far apart two colours look (CIE76 in L*a*b*), and [`confusable`] lists the pairs of a theme's
//! named colours that come too close for a deficiency. Pure and deterministic.

use crate::theme::Theme;

/// A colour-vision deficiency (dichromacy, full severity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deficiency {
    /// No deficiency (the colours as they are).
    None,
    /// Red-weak.
    Protan,
    /// Green-weak (the most common).
    Deutan,
    /// Blue-yellow.
    Tritan,
}

impl Deficiency {
    pub const ALL: [Deficiency; 4] = [Deficiency::None, Deficiency::Protan, Deficiency::Deutan, Deficiency::Tritan];

    pub fn name(self) -> &'static str {
        match self {
            Deficiency::None => "none",
            Deficiency::Protan => "protan",
            Deficiency::Deutan => "deutan",
            Deficiency::Tritan => "tritan",
        }
    }

    fn matrix(self) -> [[f32; 3]; 3] {
        match self {
            Deficiency::None => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Deficiency::Protan => [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
            Deficiency::Deutan => [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]],
            Deficiency::Tritan => [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]],
        }
    }
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `rgb` (sRGB, 0..1) as a person with `d` sees it, in sRGB.
pub fn simulate(rgb: [f32; 3], d: Deficiency) -> [f32; 3] {
    let l = rgb.map(|c| to_linear(if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 }));
    let m = d.matrix();
    [0, 1, 2].map(|r| to_srgb((m[r][0] * l[0] + m[r][1] * l[1] + m[r][2] * l[2]).clamp(0.0, 1.0)))
}

fn lab(rgb: [f32; 3]) -> [f32; 3] {
    let l = rgb.map(|c| to_linear(if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 }));
    let x = (0.4124 * l[0] + 0.3576 * l[1] + 0.1805 * l[2]) / 0.95047;
    let y = 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
    let z = (0.0193 * l[0] + 0.1192 * l[1] + 0.9505 * l[2]) / 1.08883;
    let f = |t: f32| if t > 0.008_856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// How different two sRGB colours look (CIE76: about 2.3 is just noticeable; 30 and more is
/// clearly another colour).
pub fn delta_e(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (a, b) = (lab(a), lab(b));
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A pair of a theme's named colours that look alike for a deficiency.
#[derive(Clone, Debug, PartialEq)]
pub struct Confusion {
    pub deficiency: Deficiency,
    pub a: String,
    pub b: String,
    /// How far apart they look to that deficiency.
    pub delta_e: f32,
}

/// The `pairs` of `theme`'s named colours that come closer than `min_delta_e` for any of
/// `deficiencies` (an unknown name is reported as a pair with a distance of 0).
pub fn confusable(theme: &Theme, pairs: &[(&str, &str)], deficiencies: &[Deficiency], min_delta_e: f32) -> Vec<Confusion> {
    let rgb = |n: &str| theme.color(n).map(|c| [c[0], c[1], c[2]]);
    let mut out = Vec::new();
    for &d in deficiencies {
        for &(a, b) in pairs {
            let de = match (rgb(a), rgb(b)) {
                (Some(x), Some(y)) => delta_e(simulate(x, d), simulate(y, d)),
                _ => 0.0,
            };
            if de < min_delta_e {
                out.push(Confusion { deficiency: d, a: a.to_owned(), b: b.to_owned(), delta_e: de });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(h: &str) -> [f32; 3] {
        let v = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap();
        [(v >> 16) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0]
    }

    #[test]
    fn red_and_green_merge_for_red_green_deficiencies_only() {
        let (red, green) = (hex("#d03030"), hex("#30a030"));
        let seen = |d| delta_e(simulate(red, d), simulate(green, d));
        assert!(seen(Deficiency::None) > 60.0);
        assert!(seen(Deficiency::Deutan) < seen(Deficiency::None) / 2.0, "deutan {}", seen(Deficiency::Deutan));
        assert!(seen(Deficiency::Protan) < seen(Deficiency::None) / 2.0);
        // Greys are greys to everyone.
        for d in Deficiency::ALL {
            let g = simulate([0.5; 3], d);
            assert!(g.iter().all(|c| (c - 0.5).abs() < 0.02), "{d:?}: {g:?}");
        }
        // Sepulcra's standard colours: the dead's green and the warning's yellow are close for deutans.
        assert!(delta_e(simulate(hex("#9cff4f"), Deficiency::Deutan), simulate(hex("#ffd27a"), Deficiency::Deutan)) < 30.0);
        assert!(delta_e(simulate(hex("#3fe0d0"), Deficiency::Tritan), simulate(hex("#9cff4f"), Deficiency::Tritan)) < 30.0);
    }

    #[test]
    fn hostile_values_stay_in_range() {
        for d in Deficiency::ALL {
            for c in [[f32::NAN, 2.0, -1.0], [f32::INFINITY; 3]] {
                let s = simulate(c, d);
                assert!(s.iter().all(|v| (0.0..=1.0).contains(v)), "{d:?} {s:?}");
            }
        }
        assert_eq!(delta_e([0.2; 3], [0.2; 3]), 0.0);
    }
}
