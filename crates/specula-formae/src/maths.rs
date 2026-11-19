//! The maths the forms use, all deterministic: IEEE basic operations and square roots, and
//! `specula_core::dmath` for everything else (the same bits on every machine).

use specula_core::dmath;

pub(crate) use std::f64::consts::{PI, TAU};

/// A point in the plane.
pub type Pt = [f64; 2];

pub(crate) fn sin(x: f64) -> f64 {
    dmath::sin_f64(x)
}
pub(crate) fn cos(x: f64) -> f64 {
    dmath::cos_f64(x)
}
/// `(sin x, cos x)`.
pub(crate) fn sin_cos(x: f64) -> (f64, f64) {
    dmath::sin_cos_f64(x)
}
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    dmath::atan2_f64(y, x)
}
pub(crate) fn acos(x: f64) -> f64 {
    dmath::acos_f64(x.clamp(-1.0, 1.0))
}
pub(crate) fn atan(x: f64) -> f64 {
    dmath::atan_f64(x)
}
pub(crate) fn exp(x: f64) -> f64 {
    dmath::exp_f64(x)
}
pub(crate) fn ln(x: f64) -> f64 {
    dmath::ln_f64(x)
}
pub(crate) fn ceil(x: f64) -> f64 {
    dmath::ceil_f64(x)
}
pub(crate) fn floor(x: f64) -> f64 {
    dmath::floor_f64(x)
}
/// `x^y` for `x >= 0`, by `exp(y ln x)` (dmath has no `pow_f64`); `0^y` is 0 for `y > 0`.
pub(crate) fn powf(x: f64, y: f64) -> f64 {
    if x <= 0.0 {
        if y > 0.0 {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        exp(y * ln(x))
    }
}
pub(crate) fn cosh(x: f64) -> f64 {
    let e = exp(x);
    0.5 * (e + 1.0 / e)
}
pub(crate) fn sinh(x: f64) -> f64 {
    let e = exp(x);
    0.5 * (e - 1.0 / e)
}
/// The inverse hyperbolic sine, `ln(x + sqrt(x^2 + 1))`, taken on `|x|` so it keeps its
/// precision for negative arguments.
pub(crate) fn asinh(x: f64) -> f64 {
    let a = x.abs();
    let v = ln(a + (a * a + 1.0).sqrt());
    if x < 0.0 {
        -v
    } else {
        v
    }
}

/// The gamma function for `x >= 0.5`, by Lanczos's approximation (g = 7, nine terms), whose
/// relative error is about 1e-15 there. Its coefficients are typed from memory, so a test holds
/// the function against known values (Γ(1) = 1, Γ(5) = 24, Γ(1/2) = √π, Γ(3/2) = √π/2).
pub(crate) fn gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + G + 0.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    (TAU).sqrt() * powf(t, x + 0.5) * exp(-t) * a
}

/// The perimeter of an ellipse of semi-axes `a` and `b`, by the arithmetic-geometric mean
/// (the Gauss-Kummer form of the complete elliptic integral of the second kind):
/// `P = 2π/AGM(a, b) · (a² − Σ 2^(n−1) c_n²)`, `c_0² = a² − b²`, `c_(n+1) = (a_n − b_n)/2`.
/// It converges quadratically: the loop stops when `c_n` no longer changes the sum (at most 12
/// steps), so the result is exact to rounding. Typed from memory and held by a test against the
/// built ellipse's perimeter.
pub(crate) fn ellipse_perimeter(a: f64, b: f64) -> f64 {
    let (mut an, mut bn) = if a >= b { (a, b) } else { (b, a) };
    let mut sum = an * an - bn * bn; // 2^-1 c_0^2 counted with weight 1/2 below
    sum *= 0.5;
    let mut w = 0.5;
    for _ in 0..12 {
        let c = 0.5 * (an - bn);
        let (na, nb) = (0.5 * (an + bn), (an * bn).sqrt());
        an = na;
        bn = nb;
        w *= 2.0;
        let add = w * c * c;
        if add <= sum * 1e-18 || c == 0.0 {
            sum += add;
            break;
        }
        sum += add;
    }
    let big = if a >= b { a } else { b };
    TAU / an * (big * big - sum)
}

pub(crate) fn add(a: Pt, b: Pt) -> Pt {
    [a[0] + b[0], a[1] + b[1]]
}
pub(crate) fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}
pub(crate) fn scale(a: Pt, s: f64) -> Pt {
    [a[0] * s, a[1] * s]
}
pub(crate) fn len(a: Pt) -> f64 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}
pub(crate) fn dist(a: Pt, b: Pt) -> f64 {
    len(sub(a, b))
}
/// The point at angle `t` (radians) on the circle of centre `c` and radius `r`.
pub(crate) fn polar(c: Pt, r: f64, t: f64) -> Pt {
    let (s, co) = sin_cos(t);
    [c[0] + r * co, c[1] + r * s]
}
/// `p` turned by `t` radians about the origin.
pub(crate) fn rotate(p: Pt, t: f64) -> Pt {
    let (s, c) = sin_cos(t);
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}
/// Degrees to radians.
pub(crate) fn rad(deg: f64) -> f64 {
    deg * (PI / 180.0)
}
/// The distance from `p` to the segment `a`-`b`.
pub(crate) fn dist_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = sub(b, a);
    let l2 = d[0] * d[0] + d[1] * d[1];
    if l2 == 0.0 {
        return dist(p, a);
    }
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
    dist(p, [a[0] + t * d[0], a[1] + t * d[1]])
}
