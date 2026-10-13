//! Deterministic sin/cos/floor for mesh building (foundation F2).
//!
//! This is a trimmed copy of `specula_core::dmath` (the Cody–Waite path of its `sin_cos`, same
//! constants and kernels, so the same bits), kept here only until that module lands on `main`.
//! When it does, replace this file with `pub use specula_core::dmath::*;`.
//!
//! Only IEEE basic operations are used, so results are identical on every platform. Arguments
//! must satisfy |x| < 1.6e6 (recipes reject larger angles); outside that the result is NaN, which
//! mesh validation then reports.

const PIO4_HI: f64 = f64::from_bits(0x3FE921FB54442D18);
const TWO_OVER_PI_F64: f64 = f64::from_bits(0x3FE45F306DC9C883);
const PIO2_1: f64 = f64::from_bits(0x3FF921FB54400000);
const PIO2_2: f64 = f64::from_bits(0x3DD0B4611A600000);
const PIO2_3: f64 = f64::from_bits(0x3BA3198A2E000000);
const PIO2_3T: f64 = f64::from_bits(0x397B839A252049C1);
const MEDIUM_LIMIT: f64 = 1.6e6;

/// π as an `f64` (exact bits of `std::f64::consts::PI`).
pub const PI: f64 = std::f64::consts::PI;

const fn inv_fact(n: u32) -> f64 {
    let mut f = 1.0;
    let mut i = 2;
    while i <= n {
        f *= i as f64;
        i += 1;
    }
    1.0 / f
}

const fn pow2(e: i32) -> f64 {
    f64::from_bits(((0x3ff + e) as u64) << 52)
}

#[inline(always)]
fn horner(x: f64, c: &[f64]) -> f64 {
    let mut p = c[c.len() - 1];
    for &k in c[..c.len() - 1].iter().rev() {
        p = k + x * p;
    }
    p
}

#[inline]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

/// Rounds towards zero (bit manipulation, exact).
pub fn trunc_f64(x: f64) -> f64 {
    let b = x.to_bits();
    let e = ((b >> 52) & 0x7ff) as i32 - 1023;
    if e >= 52 {
        return x;
    }
    if e < 0 {
        return f64::from_bits(b & (1 << 63));
    }
    f64::from_bits(b & !((1u64 << (52 - e)) - 1))
}

pub fn floor_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    if t > x {
        t - 1.0
    } else {
        t
    }
}

pub fn ceil_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    if t < x {
        t + 1.0
    } else {
        t
    }
}

/// Rounds half away from zero (like `f64::round`, which is buggy on some libms).
pub fn round_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    if (x - t).abs() >= 0.5 {
        t + 1.0f64.copysign(x)
    } else {
        t
    }
}

/// Rounds towards zero.
pub fn trunc(x: f32) -> f32 {
    let b = x.to_bits();
    let e = ((b >> 23) & 0xff) as i32 - 127;
    if e >= 23 {
        return x;
    }
    if e < 0 {
        return f32::from_bits(b & 0x8000_0000);
    }
    f32::from_bits(b & !((1u32 << (23 - e)) - 1))
}

pub fn floor(x: f32) -> f32 {
    let t = trunc(x);
    if t > x {
        t - 1.0
    } else {
        t
    }
}

fn rem_pio2(x: f64) -> (u32, f64, f64) {
    let ax = x.abs();
    if ax <= PIO4_HI {
        return (0, x, 0.0);
    }
    let k = round_f64(x * TWO_OVER_PI_F64);
    let t1 = x - k * PIO2_1;
    let (t2, e2) = two_sum(t1, -(k * PIO2_2));
    let (t3, e3) = two_sum(t2, -(k * PIO2_3));
    let (hi, lo) = two_sum(t3, (e2 + e3) - k * PIO2_3T);
    ((k as i64 & 3) as u32, hi, lo)
}

const SIN: [f64; 9] = [
    -inv_fact(3),
    inv_fact(5),
    -inv_fact(7),
    inv_fact(9),
    -inv_fact(11),
    inv_fact(13),
    -inv_fact(15),
    inv_fact(17),
    -inv_fact(19),
];

#[inline]
fn sin_kernel(x: f64, y: f64) -> f64 {
    let z = x * x;
    x + ((z * x) * horner(z, &SIN) + y * (1.0 - 0.5 * z))
}

const COS: [f64; 8] =
    [inv_fact(4), -inv_fact(6), inv_fact(8), -inv_fact(10), inv_fact(12), -inv_fact(14), inv_fact(16), -inv_fact(18)];

#[inline]
fn cos_kernel(x: f64, y: f64) -> f64 {
    let z = x * x;
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    let r = z * z * horner(z, &COS);
    w + (((1.0 - w) - hz) + (r - x * y))
}

const TINY: f64 = pow2(-27);

/// `(sin x, cos x)` in `f64`.
pub fn sin_cos_f64(x: f64) -> (f64, f64) {
    if x.abs() < TINY {
        return (x, 1.0);
    }
    if !x.is_finite() || x.abs() >= MEDIUM_LIMIT {
        return (f64::NAN, f64::NAN);
    }
    let (q, a, b) = rem_pio2(x);
    let (s, c) = (sin_kernel(a, b), cos_kernel(a, b));
    match q {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// `(sin x, cos x)`: evaluated in `f64`, rounded once.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let (s, c) = sin_cos_f64(x as f64);
    (s as f32, c as f32)
}

pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// `(sin, cos)` of an angle in degrees, reduced exactly to [-180, 180] first so whole-degree
/// multiples of 90 give exact 0 and ±1.
pub fn sin_cos_deg(deg: f64) -> (f64, f64) {
    let r = deg - 360.0 * floor_f64(deg / 360.0); // 0..360
    // Exact quadrant values keep axis-aligned rotations exact.
    if r == 0.0 {
        return (0.0, 1.0);
    }
    if r == 90.0 {
        return (1.0, 0.0);
    }
    if r == 180.0 {
        return (0.0, -1.0);
    }
    if r == 270.0 {
        return (-1.0, 0.0);
    }
    sin_cos_f64(r * (PI / 180.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_cos_close_to_std() {
        for i in -2000..2000 {
            let x = i as f32 * 0.0137;
            let (s, c) = sin_cos(x);
            assert!((s - x.sin()).abs() < 1e-6, "sin({x})");
            assert!((c - x.cos()).abs() < 1e-6, "cos({x})");
        }
    }

    #[test]
    fn exact_quadrants_in_degrees() {
        assert_eq!(sin_cos_deg(90.0), (1.0, 0.0));
        assert_eq!(sin_cos_deg(-90.0), (-1.0, 0.0));
        assert_eq!(sin_cos_deg(540.0), (0.0, -1.0));
        let (s, c) = sin_cos_deg(30.0);
        assert!((s - 0.5).abs() < 1e-15 && (c - 0.75f64.sqrt()).abs() < 1e-15);
    }

    #[test]
    fn floor_and_round() {
        assert_eq!(floor(-0.5), -1.0);
        assert_eq!(floor(2.0), 2.0);
        assert_eq!(round_f64(0.49999997), 0.0);
        assert_eq!(round_f64(-2.5), -3.0);
        assert_eq!(ceil_f64(1.2), 2.0);
        assert!(sin_cos(2e6).0.is_nan());
    }
}
