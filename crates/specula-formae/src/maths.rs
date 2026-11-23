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
/// `e^x − 1`, keeping its digits when `x` is small (where `exp(x) − 1` keeps only those of
/// `x` past the ones `1` takes): Kahan's form `(u − 1) x / ln u`, `u = e^x`, whose rounding
/// errors in `u − 1` and `ln u` cancel. Held by a test against the series at small `x`.
pub(crate) fn expm1(x: f64) -> f64 {
    let u = exp(x);
    if u == 1.0 {
        return x;
    }
    let um1 = u - 1.0;
    if um1 == -1.0 || !u.is_finite() {
        return um1;
    }
    um1 * x / ln(u)
}
/// `ln(1 + x)`, keeping its digits when `x` is small: `ln u · x / (u − 1)`, `u = 1 + x`, by
/// the same cancellation of rounding errors. Held by a test against the series.
pub(crate) fn ln1p(x: f64) -> f64 {
    let u = 1.0 + x;
    if u == 1.0 {
        x
    } else {
        ln(u) * x / (u - 1.0)
    }
}
/// `cosh x − 1` as `2 sinh²(x / 2)`, which keeps its digits where `cosh x − 1` cancels.
pub(crate) fn coshm1(x: f64) -> f64 {
    let s = sinh(0.5 * x);
    2.0 * s * s
}
/// The hyperbolic sine. Under 1 in size it is `(m + m / (m + 1)) / 2` with `m = e^|x| − 1`
/// from [`expm1`] (the plain `(e^x − e^−x) / 2` loses as many digits as `1 / |x|` has: at
/// `x = 1e-8`, eight); over it, the plain form, which no longer cancels.
pub(crate) fn sinh(x: f64) -> f64 {
    let a = x.abs();
    let v = if a < 1.0 {
        let m = expm1(a);
        0.5 * (m + m / (m + 1.0))
    } else {
        let e = exp(a);
        0.5 * (e - 1.0 / e)
    };
    if x < 0.0 {
        -v
    } else {
        v
    }
}
/// The inverse hyperbolic sine, taken on `|x|` so it keeps its precision for negative
/// arguments: under 1 in size `ln1p(a + a² / (1 + √(1 + a²)))` (the plain `ln(a + √(a² + 1))`
/// takes the logarithm of a number near 1 and loses as many digits as `1 / a` has); over it
/// the plain form.
pub(crate) fn asinh(x: f64) -> f64 {
    let a = x.abs();
    let v = if a < 1.0 { ln1p(a + a * a / (1.0 + (1.0 + a * a).sqrt())) } else { ln(a + (a * a + 1.0).sqrt()) };
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
/// The point at angle `t` (radians) on the circle of centre `c` and radius `r`. At a whole
/// number of quarter turns (`t` exactly `q · π/2` in floating point) the cosine and sine are
/// taken as exactly 0 and ±1, so that an arc to a pole or to the circle's widest point lands on
/// it (`cos(π/2)` is 6e-17, which times a radius of 1e12 is 6e-5 m).
pub(crate) fn polar(c: Pt, r: f64, t: f64) -> Pt {
    let (s, co) = quarter_exact(t).unwrap_or_else(|| sin_cos(t));
    [c[0] + r * co, c[1] + r * s]
}
/// `(sin t, cos t)` exactly when `t` is a whole number of quarter turns as floating point
/// writes them (`0.5 · π · q` for a whole `q`, as `0.5 * PI`, `PI`, `-0.5 * PI` and `TAU` are).
pub(crate) fn quarter_exact(t: f64) -> Option<(f64, f64)> {
    let q = t / (0.5 * PI);
    let w = dmath::round_f64(q);
    if w.abs() > 64.0 || 0.5 * PI * w != t {
        return None;
    }
    Some(match (w as i64).rem_euclid(4) {
        0 => (0.0, 1.0),
        1 => (1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (-1.0, 0.0),
    })
}
/// The versine `1 − cos x` as `2 sin²(x / 2)`, which keeps its digits where `1 − cos x` rounds
/// to nothing (at `x` under 1e-8: a chord's sag of an arc of radius 1e12 cut into steps of a
/// hundred-millionth of a turn is metres, and `1 − cos` said 0).
pub(crate) fn versin(x: f64) -> f64 {
    let s = sin(0.5 * x);
    2.0 * s * s
}
/// The step along the circle of radius `r` from its point at angle `a` through the angle `d`:
/// `r (cos(a + d) − cos a, sin(a + d) − sin a)`, as `2 r sin(d/2) (−sin(a + d/2), cos(a + d/2))`.
/// Added to the exact point at `a`, it places a point of an arc from the arc's own end, in
/// numbers the size of the step and not of the radius (whose centre may be a billion times
/// farther off than the arc is long).
pub(crate) fn arc_step(r: f64, a: f64, d: f64) -> Pt {
    let k = 2.0 * r * sin(0.5 * d);
    let (s, c) = sin_cos(a + 0.5 * d);
    [-k * s, k * c]
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
/// A point in space.
pub type P3 = [f64; 3];

pub(crate) fn sub3(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn add3(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn scale3(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn dot3(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross3(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(crate) fn len3(a: P3) -> f64 {
    dot3(a, a).sqrt()
}
pub(crate) fn dist3(a: P3, b: P3) -> f64 {
    len3(sub3(a, b))
}
/// `a` scaled to unit length (`a` itself when it has none).
pub(crate) fn norm3(a: P3) -> P3 {
    let l = len3(a);
    if l > 0.0 {
        scale3(a, 1.0 / l)
    } else {
        a
    }
}
pub(crate) fn tan(x: f64) -> f64 {
    dmath::tan_f64(x)
}
pub(crate) fn asin(x: f64) -> f64 {
    dmath::asin_f64(x.clamp(-1.0, 1.0))
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
