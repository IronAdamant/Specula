//! Deterministic maths: transcendental functions that give the same bits on every platform.
//!
//! `f32::sin`, `exp`, `ln`, `powf`, `atan2` and friends call the platform maths library (msvcrt/UCRT,
//! mingw-w64, glibc, musl, Bionic, Apple libm…). Those differ in the last bit, so a simulation that
//! uses them can diverge between our own msvc and gnu builds, and between x86 and ARM. Everything
//! here is built only from IEEE 754 basic operations (`+ - * /`, `sqrt`, conversions and integer
//! bit manipulation), which are correctly rounded and therefore identical everywhere. No `mul_add`
//! (its software fallback is slow and some targets lack hardware FMA), no libm, no lookup of the
//! floating-point environment: the default round-to-nearest mode is assumed (see DETERMINISM.md).
//!
//! # Design
//!
//! - The `f32` functions evaluate in `f64` and round once at the end, so they are almost always
//!   correctly rounded (0 ulp off on every input the tests sweep; ≤ 1 ulp asserted). `f32` is what
//!   the engine uses.
//! - The `f64` functions (`*_f64`) use the same algorithms. They are within 1–2 ulp, not correctly
//!   rounded (measured bounds are in the tests and in `docs/design/DETERMINISM.md`). There is no
//!   `pow_f64`: it would need a double-double `ln` to stay accurate.
//! - Deterministic on every IEEE 754 target with SSE2/NEON-style `f64` (x86-64, AArch64). Not on
//!   32-bit x87 builds, which round intermediates to 80 bits.
//! - Polynomials are truncated Taylor/`atanh` series with exact rational coefficients (evaluated at
//!   compile time), on arguments reduced far enough that the truncation error is below 2⁻⁶⁰.
//!   Tables (π, 2/π, `atan(k/8)`, ln 2) were generated with exact integer arithmetic.
//! - `sin`/`cos`/`tan` reduce by π/2 with a 4-part Cody–Waite split carried as a double-double for
//!   |x| < 2²⁰·π/2 and with Payne–Hanek (integer multiply by 1216 bits of 2/π) above that, so even
//!   `sin(1e300)` is accurate. No accuracy cliff for huge arguments.
//! - Special values follow C99 Annex F / Rust `std`: NaN in → NaN out, `sin(±0) = ±0`,
//!   `exp(-inf) = 0`, `ln(0) = -inf`, `atan2` and `pow` special cases, etc.
//!
//! **Changing any algorithm or constant changes simulation results.** The golden bit-pattern tests
//! catch that; bump [`VERSION`] (it goes into replay/journal headers) when they must change.

/// Bumped whenever any function here can return different bits for some input.
pub const VERSION: u32 = 1;

// ---------------------------------------------------------------------------------------------
// Constants (generated with exact integer arithmetic; hex so no decimal-parsing doubt).

const PI_HI: f64 = f64::from_bits(0x400921FB54442D18);
const PI_LO: f64 = f64::from_bits(0x3CA1A62633145C07);
const PIO2_HI: f64 = f64::from_bits(0x3FF921FB54442D18);
const PIO2_LO: f64 = f64::from_bits(0x3C91A62633145C07);
const PIO4_HI: f64 = f64::from_bits(0x3FE921FB54442D18);
/// 2/π rounded to f64 (only used to pick the quadrant, exactness not needed).
const TWO_OVER_PI_F64: f64 = f64::from_bits(0x3FE45F306DC9C883);
/// π/2 = PIO2_1 + PIO2_2 + PIO2_3 + PIO2_3T, the first three with ≤ 33 significant bits so that
/// `k * PIO2_n` is exact for |k| < 2²⁰.
const PIO2_1: f64 = f64::from_bits(0x3FF921FB54400000);
const PIO2_2: f64 = f64::from_bits(0x3DD0B4611A600000);
const PIO2_3: f64 = f64::from_bits(0x3BA3198A2E000000);
const PIO2_3T: f64 = f64::from_bits(0x397B839A252049C1);
/// Above this, Cody–Waite's `k` would exceed 2²⁰; use Payne–Hanek (2²⁰·π/2 ≈ 1.647e6).
const MEDIUM_LIMIT: f64 = 1.6e6;
/// round(π/4 · 2⁶⁴).
const PI4_FIX: u64 = 0xC90FDAA22168C235;
/// Bits of 2/π after the binary point, most significant first.
const TWO_OVER_PI: [u64; 20] = [
    0xA2F9836E4E441529, 0xFC2757D1F534DDC0, 0xDB6295993C439041, 0xFE5163ABDEBBC561,
    0xB7246E3A424DD2E0, 0x06492EEA09D1921C, 0xFE1DEB1CB129A73E, 0xE88235F52EBB4484,
    0xE99C7026B45F7E41, 0x3991D639835339F4, 0x9C845F8BBDF9283B, 0x1FF897FFDE05980F,
    0xEF2F118B5A0A6D1F, 0x6D367ECF27CB09B7, 0x4F463F669E5FEA2D, 0x7527BAC7EBE5F17B,
    0x3D0739F78A5292EA, 0x6BFB5FB11F8D5D08, 0x56033046FC7B6BAB, 0xF0CFBC209AF4361D,
];

const LN2_HI: f64 = f64::from_bits(0x3FE62E42FEFA39EF);
const LN2_LO: f64 = f64::from_bits(0x3C7ABC9E3B39803F);
/// ln 2 = LN2_CW_HI + LN2_CW_LO with a 32-bit head, so `k * LN2_CW_HI` is exact for |k| < 2²¹.
const LN2_CW_HI: f64 = f64::from_bits(0x3FE62E42FF000000);
const LN2_CW_LO: f64 = f64::from_bits(0xBDC718432A1B0E26);
const INV_LN2_HI: f64 = f64::from_bits(0x3FF71547652B82FE);
const INV_LN2_LO: f64 = f64::from_bits(0x3C7777D0FFDA0D24);
const SQRT2: f64 = f64::from_bits(0x3FF6A09E667F3BCD);

/// `atan(k/8)` for k = 0..=8 as (hi, lo).
const ATAN_K8: [(f64, f64); 9] = [
    (0.0, 0.0),
    (f64::from_bits(0x3FBFD5BA9AAC2F6E), f64::from_bits(0xBC4CD37686760C17)),
    (f64::from_bits(0x3FCF5B75F92C80DD), f64::from_bits(0x3C68AB6E3CF7AFBD)),
    (f64::from_bits(0x3FD6F61941E4DEF1), f64::from_bits(0xBC7C63AAE6F6E918)),
    (f64::from_bits(0x3FDDAC670561BB4F), f64::from_bits(0x3C7A2B7F222F65E2)),
    (f64::from_bits(0x3FE1E00BABDEFEB4), f64::from_bits(0xBC5928DF287A668F)),
    (f64::from_bits(0x3FE4978FA3269EE1), f64::from_bits(0x3C72419A87F2A458)),
    (f64::from_bits(0x3FE700A7C5784634), f64::from_bits(0xBC78C34D25AADEF6)),
    (f64::from_bits(0x3FE921FB54442D18), f64::from_bits(0x3C81A62633145C07)),
];

/// 1/n!, exact up to the final rounding (n! is exactly representable for n ≤ 22).
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

/// c[0] + x·(c[1] + x·(c[2] + …)), evaluated innermost first (a fixed order, so fixed bits).
#[inline(always)]
fn horner(x: f64, c: &[f64]) -> f64 {
    let mut p = c[c.len() - 1];
    for &k in c[..c.len() - 1].iter().rev() {
        p = k + x * p;
    }
    p
}

// ---------------------------------------------------------------------------------------------
// Error-free transformations (exact with round-to-nearest; no FMA).

/// `a + b = s + e` exactly.
#[inline]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

/// Veltkamp split into two 26-bit halves. Valid for |a| < 2⁹⁹⁶.
#[inline]
fn split(a: f64) -> (f64, f64) {
    let t = 134_217_729.0 * a; // 2^27 + 1
    let hi = t - (t - a);
    (hi, a - hi)
}

/// `a * b = p + e` exactly (Dekker). Valid when no intermediate overflows or underflows.
#[inline]
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

/// `y * 2^n` with a single rounding (musl's scalbn).
fn scalbn(mut y: f64, mut n: i32) -> f64 {
    if n > 1023 {
        y *= pow2(1023);
        n -= 1023;
        if n > 1023 {
            y *= pow2(1023);
            n = (n - 1023).min(1023);
        }
    } else if n < -1022 {
        // Scale into the subnormal range in one final step to avoid double rounding.
        y *= pow2(-1022 + 53);
        n += 1022 - 53;
        if n < -1022 {
            y *= pow2(-1022 + 53);
            n = (n + 1022 - 53).max(-1022);
        }
    }
    y * pow2(n)
}

// ---------------------------------------------------------------------------------------------
// Rounding helpers: pure bit manipulation (exact, so these also match std bit for bit).

/// Rounds towards zero.
pub fn trunc_f64(x: f64) -> f64 {
    let b = x.to_bits();
    let e = ((b >> 52) & 0x7ff) as i32 - 1023;
    if e >= 52 {
        return x; // already integral, ±inf or NaN
    }
    if e < 0 {
        return f64::from_bits(b & (1 << 63)); // |x| < 1 → ±0
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

/// Rounds half away from zero (like `f64::round`).
pub fn round_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    if (x - t).abs() >= 0.5 {
        t + 1.0f64.copysign(x)
    } else {
        t
    }
}

/// Rounds half to even (like `f64::round_ties_even`).
pub fn round_ties_even_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    let d = (x - t).abs();
    let t_odd = trunc_f64(t * 0.5) != t * 0.5;
    if d > 0.5 || (d == 0.5 && t_odd) {
        t + 1.0f64.copysign(x)
    } else {
        t
    }
}

/// `x - trunc(x)` (like `f64::fract`; sign follows `x`).
pub fn fract_f64(x: f64) -> f64 {
    x - trunc_f64(x)
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

pub fn ceil(x: f32) -> f32 {
    let t = trunc(x);
    if t < x {
        t + 1.0
    } else {
        t
    }
}

/// Rounds half away from zero (like `f32::round`).
pub fn round(x: f32) -> f32 {
    let t = trunc(x);
    if (x - t).abs() >= 0.5 {
        t + 1.0f32.copysign(x)
    } else {
        t
    }
}

/// Rounds half to even (like `f32::round_ties_even`).
pub fn round_ties_even(x: f32) -> f32 {
    let t = trunc(x);
    let d = (x - t).abs();
    let t_odd = trunc(t * 0.5) != t * 0.5;
    if d > 0.5 || (d == 0.5 && t_odd) {
        t + 1.0f32.copysign(x)
    } else {
        t
    }
}

/// `x - trunc(x)` (like `f32::fract`; sign follows `x`).
pub fn fract(x: f32) -> f32 {
    x - trunc(x)
}

// ---------------------------------------------------------------------------------------------
// sin / cos / tan

/// Reduces `x` to `q·π/2 + (hi + lo)` with |hi + lo| ≲ π/4. Returns (q mod 4, hi, lo).
/// `x` must be finite.
fn rem_pio2(x: f64) -> (u32, f64, f64) {
    let ax = x.abs();
    if ax <= PIO4_HI {
        return (0, x, 0.0);
    }
    if ax < MEDIUM_LIMIT {
        // Cody–Waite: every k·PIO2_n is exact; x - k·PIO2_1 is exact (Sterbenz); the rest is
        // accumulated error-free into a double-double.
        let k = round_f64(x * TWO_OVER_PI_F64);
        let t1 = x - k * PIO2_1;
        let (t2, e2) = two_sum(t1, -(k * PIO2_2));
        let (t3, e3) = two_sum(t2, -(k * PIO2_3));
        let (hi, lo) = two_sum(t3, (e2 + e3) - k * PIO2_3T);
        return ((k as i64 & 3) as u32, hi, lo);
    }
    rem_pio2_large(x)
}

/// 64 bits of 2/π starting at bit `p` after the binary point (bits before it are 0).
fn two_over_pi_bits(p: i32) -> u64 {
    if p < 0 {
        return if p <= -64 { 0 } else { TWO_OVER_PI[0] >> -p };
    }
    let (i, o) = ((p / 64) as usize, (p % 64) as u32);
    if o == 0 {
        TWO_OVER_PI[i]
    } else {
        (TWO_OVER_PI[i] << o) | (TWO_OVER_PI[i + 1] >> (64 - o))
    }
}

/// Payne–Hanek reduction for |x| ≥ `MEDIUM_LIMIT`, in integer arithmetic.
fn rem_pio2_large(x: f64) -> (u32, f64, f64) {
    let bits = x.to_bits();
    let m = (bits & ((1 << 52) - 1)) | (1 << 52);
    let e = ((bits >> 52) & 0x7ff) as i32 - 1075; // |x| = m · 2^e
    // x·2/π mod 8: bits of 2/π before index e-2 contribute multiples of 8, so take a 192-bit
    // window W starting there; then m·W has its units bit at 2^189.
    let p = e - 3;
    let w = [two_over_pi_bits(p), two_over_pi_bits(p + 64), two_over_pi_bits(p + 128)];
    let a2 = m as u128 * w[2] as u128;
    let a1 = m as u128 * w[1] as u128;
    let a0 = m as u128 * w[0] as u128;
    let l3 = a2 as u64;
    let c = (a2 >> 64) + (a1 as u64 as u128);
    let l2 = c as u64;
    let l1 = ((c >> 64) + (a1 >> 64) + (a0 as u64 as u128)) as u64;
    let mut q = (l1 >> 61) as u32; // integer part mod 8
    // Fraction in units of 2^-128 (the top 3 bits of l1 shift out).
    let mut f: u128 = ((l1 as u128) << 67) | ((l2 as u128) << 3) | (l3 >> 61) as u128;
    let neg_r = f >> 127 != 0;
    if neg_r {
        q += 1; // fraction ≥ ½: round to the next quadrant, remainder negative
        f = f.wrapping_neg();
    }
    let (mut hi, mut lo) = if f == 0 {
        (0.0, 0.0)
    } else {
        // r = f·2^-128·π/2 = (top·PI4_FIX)·2^(-lz-127), top = f normalised to 64 bits.
        let lz = f.leading_zeros() as i32;
        let top = ((f << lz) >> 64) as u64;
        let prod = (top as u128 * PI4_FIX as u128) >> 2; // < 2^126: fits i128 with room
        let h = prod as f64;
        let l = (prod as i128 - h as i128) as f64;
        let s = pow2(-125 - lz);
        (h * s, l * s)
    };
    if neg_r {
        hi = -hi;
        lo = -lo;
    }
    if x < 0.0 {
        q = q.wrapping_neg();
        hi = -hi;
        lo = -lo;
    }
    (q & 3, hi, lo)
}

/// Taylor coefficients of sin from x³: -1/3!, 1/5!, …, -1/19!.
const SIN: [f64; 9] = [
    -inv_fact(3), inv_fact(5), -inv_fact(7), inv_fact(9), -inv_fact(11), inv_fact(13), -inv_fact(15), inv_fact(17),
    -inv_fact(19),
];

/// sin(x + y) for |x| ≲ π/4, |y| ≤ ulp(x)/2. Truncation error < 1e-19 relative.
#[inline]
fn sin_kernel(x: f64, y: f64) -> f64 {
    let z = x * x;
    x + ((z * x) * horner(z, &SIN) + y * (1.0 - 0.5 * z))
}

/// Taylor coefficients of cos from x⁴: 1/4!, -1/6!, …, -1/18!.
const COS: [f64; 8] =
    [inv_fact(4), -inv_fact(6), inv_fact(8), -inv_fact(10), inv_fact(12), -inv_fact(14), inv_fact(16), -inv_fact(18)];

/// cos(x + y) for |x| ≲ π/4.
#[inline]
fn cos_kernel(x: f64, y: f64) -> f64 {
    let z = x * x;
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    let r = z * z * horner(z, &COS);
    // (1 - w) - hz recovers the rounding error of w exactly.
    w + (((1.0 - w) - hz) + (r - x * y))
}

/// Below this, sin x = tan x = x and cos x = 1 after rounding.
const TINY: f64 = pow2(-27);

pub fn sin_f64(x: f64) -> f64 {
    if x.abs() < TINY {
        return x; // keeps ±0 and subnormals exact
    }
    if !x.is_finite() {
        return f64::NAN;
    }
    let (q, a, b) = rem_pio2(x);
    match q {
        0 => sin_kernel(a, b),
        1 => cos_kernel(a, b),
        2 => -sin_kernel(a, b),
        _ => -cos_kernel(a, b),
    }
}

pub fn cos_f64(x: f64) -> f64 {
    if x.abs() < TINY {
        return 1.0;
    }
    if !x.is_finite() {
        return f64::NAN;
    }
    let (q, a, b) = rem_pio2(x);
    match q {
        0 => cos_kernel(a, b),
        1 => -sin_kernel(a, b),
        2 => -cos_kernel(a, b),
        _ => sin_kernel(a, b),
    }
}

/// `(sin x, cos x)` with one range reduction.
pub fn sin_cos_f64(x: f64) -> (f64, f64) {
    if x.abs() < TINY {
        return (x, 1.0);
    }
    if !x.is_finite() {
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

pub fn tan_f64(x: f64) -> f64 {
    if x.abs() < TINY {
        return x;
    }
    if !x.is_finite() {
        return f64::NAN;
    }
    let (q, a, b) = rem_pio2(x);
    let (s, c) = (sin_kernel(a, b), cos_kernel(a, b));
    if q & 1 == 0 {
        s / c
    } else {
        -c / s
    }
}

pub fn sin(x: f32) -> f32 {
    sin_f64(x as f64) as f32
}

pub fn cos(x: f32) -> f32 {
    cos_f64(x as f64) as f32
}

/// `(sin x, cos x)` with one range reduction.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let (s, c) = sin_cos_f64(x as f64);
    (s as f32, c as f32)
}

pub fn tan(x: f32) -> f32 {
    tan_f64(x as f64) as f32
}

// ---------------------------------------------------------------------------------------------
// atan / atan2 / asin / acos

/// atan(t) - t for |t| ≤ 1/16: series through t¹⁵ (truncation < 4e-21 relative).
#[inline]
fn atan_tail(t: f64) -> f64 {
    const ATAN: [f64; 7] = [-1.0 / 3.0, 1.0 / 5.0, -1.0 / 7.0, 1.0 / 9.0, -1.0 / 11.0, 1.0 / 13.0, -1.0 / 15.0];
    let z = t * t;
    t * z * horner(z, &ATAN)
}

/// atan(a + a_lo) for a ∈ [0, 1], |a_lo| ≲ ulp(a), as an unevaluated sum (hi, lo):
/// atan(c) + atan((a - c)/(1 + ac)) with c = k/8 the nearest eighth, the quotient kept as a
/// double-double so the table split adds no error.
fn atan_unit(a: f64, a_lo: f64) -> (f64, f64) {
    let k = round_f64(a * 8.0);
    if k == 0.0 {
        // atan(a + a_lo) ≈ atan(a) + a_lo/(1 + a²)
        return (a, atan_tail(a) + a_lo * (1.0 - a * a));
    }
    let c = k * 0.125;
    let num = a - c; // exact (Sterbenz)
    let (p, pe) = two_prod(a, c);
    let (d, de) = two_sum(1.0, p);
    let de = de + pe + a_lo * c;
    let t = num / d;
    let (q, qe) = two_prod(t, d);
    let t_lo = (((num - q) - qe) + a_lo - t * de) / d;
    let (hi, lo) = ATAN_K8[k as usize];
    let (s, e) = two_sum(hi, t);
    (s, e + (lo + (atan_tail(t) + t_lo * (1.0 - t * t))))
}

/// atan(n/d) for 0 ≤ n ≤ d, 0 < d < inf, with both given as double-doubles.
fn atan_ratio(n: f64, n_lo: f64, d: f64, d_lo: f64) -> (f64, f64) {
    // Scale by a power of two so d ∈ [1, 2): exact (n may underflow only where the ratio is
    // negligible) and keeps `two_prod` in range.
    let e = 1023 - ((d.to_bits() >> 52) & 0x7ff) as i32;
    let (n, n_lo, d, d_lo) = (scalbn(n, e), scalbn(n_lo, e), scalbn(d, e), scalbn(d_lo, e));
    let a = n / d;
    let (p, pe) = two_prod(a, d);
    atan_unit(a, (((n - p) - pe) + n_lo - a * d_lo) / d)
}

/// π/2 - (h + l) as a double-double.
fn pio2_minus((h, l): (f64, f64)) -> (f64, f64) {
    let (s, e) = two_sum(PIO2_HI, -h);
    (s, (e + PIO2_LO) - l)
}

/// π - (h + l), rounded.
fn pi_minus((h, l): (f64, f64)) -> f64 {
    let (s, e) = two_sum(PI_HI, -h);
    s + ((e + PI_LO) - l)
}

/// Angle of the vector (x, y) for x, y ≥ 0 finite, not both zero, given as double-doubles.
fn angle_first_quadrant(y: f64, y_lo: f64, x: f64, x_lo: f64) -> (f64, f64) {
    if y <= x {
        atan_ratio(y, y_lo, x, x_lo)
    } else {
        pio2_minus(atan_ratio(x, x_lo, y, y_lo))
    }
}

pub fn atan_f64(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let ax = x.abs();
    let (h, l) = if ax <= 1.0 {
        atan_unit(ax, 0.0)
    } else if ax.is_infinite() {
        (PIO2_HI, PIO2_LO)
    } else {
        pio2_minus(atan_ratio(1.0, 0.0, ax, 0.0))
    };
    (h + l).copysign(x)
}

/// Angle of the point (x, y) in [-π, π], with the C99 special cases (`atan2(±0, -0) = ±π`, …).
pub fn atan2_f64(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let (ay, ax) = (y.abs(), x.abs());
    let x_neg = x.is_sign_negative();
    let r = if ay == 0.0 {
        if x_neg {
            PI_HI
        } else {
            0.0
        }
    } else if ax == 0.0 {
        PIO2_HI
    } else if ay.is_infinite() {
        match (ax.is_infinite(), x_neg) {
            (true, false) => PIO4_HI,
            (true, true) => 3.0 * PIO4_HI, // rounds to the f64 nearest 3π/4
            _ => PIO2_HI,
        }
    } else if ax.is_infinite() {
        if x_neg {
            PI_HI
        } else {
            0.0
        }
    } else {
        let a = angle_first_quadrant(ay, 0.0, ax, 0.0);
        if x_neg {
            pi_minus(a)
        } else {
            a.0 + a.1
        }
    };
    r.copysign(y)
}

/// √(1 - x²) for 0 ≤ x ≤ 1 as a double-double (1 - x² is formed exactly).
fn sqrt_one_minus_sq(x: f64) -> (f64, f64) {
    let (p, pe) = two_prod(x, x);
    let (h, he) = two_sum(1.0, -p);
    let (h, l) = two_sum(h, he - pe);
    if h == 0.0 {
        return (0.0, 0.0);
    }
    let s = h.sqrt();
    // One Newton step on the double-double h + l.
    let (ss, sse) = two_prod(s, s);
    two_sum(s, (((h - ss) - sse) + l) / (2.0 * s))
}

pub fn asin_f64(x: f64) -> f64 {
    let ax = x.abs();
    if x.is_nan() || ax > 1.0 {
        return f64::NAN;
    }
    if ax < TINY {
        return x;
    }
    let (d, d_lo) = sqrt_one_minus_sq(ax);
    let (h, l) = angle_first_quadrant(ax, 0.0, d, d_lo);
    (h + l).copysign(x)
}

pub fn acos_f64(x: f64) -> f64 {
    let ax = x.abs();
    if x.is_nan() || ax > 1.0 {
        return f64::NAN;
    }
    let (d, d_lo) = sqrt_one_minus_sq(ax);
    let a = angle_first_quadrant(d, d_lo, ax, 0.0);
    if x.is_sign_negative() {
        pi_minus(a)
    } else {
        a.0 + a.1
    }
}

pub fn atan(x: f32) -> f32 {
    atan_f64(x as f64) as f32
}

/// Angle of the point (x, y) in [-π, π].
pub fn atan2(y: f32, x: f32) -> f32 {
    atan2_f64(y as f64, x as f64) as f32
}

pub fn asin(x: f32) -> f32 {
    asin_f64(x as f64) as f32
}

pub fn acos(x: f32) -> f32 {
    acos_f64(x as f64) as f32
}

// ---------------------------------------------------------------------------------------------
// exp / exp2

/// Taylor coefficients of exp from r²: 1/2!, …, 1/14!.
const EXP: [f64; 13] = {
    let mut c = [0.0; 13];
    let mut i = 0;
    while i < 13 {
        c[i] = inv_fact(i as u32 + 2);
        i += 1;
    }
    c
};

/// exp(r + re) · 2^k for |r| ≲ ln2/2, |re| ≤ ulp(r). Series through r¹⁴ (truncation < 2e-19).
#[inline]
fn exp_finish(r: f64, re: f64, k: i32) -> f64 {
    let p = r * r * horner(r, &EXP);
    let y = 1.0 + (r + (p + re * (1.0 + r)));
    scalbn(y, k)
}

/// Largest x with finite exp(x); smallest x with nonzero exp(x).
const EXP_OVERFLOW: f64 = 709.782712893384;
const EXP_UNDERFLOW: f64 = -745.1332191019412;

pub fn exp_f64(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > EXP_OVERFLOW {
        return f64::INFINITY;
    }
    if x < EXP_UNDERFLOW {
        return 0.0;
    }
    let k = round_f64(x * INV_LN2_HI);
    let hi = x - k * LN2_CW_HI; // exact
    let (r, re) = two_sum(hi, -(k * LN2_CW_LO));
    exp_finish(r, re, k as i32)
}

pub fn exp2_f64(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x >= 1024.0 {
        return f64::INFINITY;
    }
    if x <= -1075.0 {
        return 0.0;
    }
    let k = round_f64(x);
    let f = x - k; // exact, |f| ≤ ½
    let (h, l) = two_prod(f, LN2_HI);
    let (r, re) = two_sum(h, l + f * LN2_LO);
    exp_finish(r, re, k as i32)
}

pub fn exp(x: f32) -> f32 {
    exp_f64(x as f64) as f32
}

pub fn exp2(x: f32) -> f32 {
    exp2_f64(x as f64) as f32
}

// ---------------------------------------------------------------------------------------------
// ln / log2

/// Splits finite positive `x` into 2^k·(1 + f), f ∈ [√½ - 1, √2 - 1]. Special inputs give the
/// final result in `Err` (NaN, negative → NaN; 0 → -inf; +inf → +inf).
fn log_reduce(x: f64) -> Result<(i32, f64), f64> {
    if x.is_nan() {
        return Err(x);
    }
    if x < 0.0 {
        return Err(f64::NAN);
    }
    if x == 0.0 {
        return Err(f64::NEG_INFINITY);
    }
    if x.is_infinite() {
        return Err(x);
    }
    let mut bits = x.to_bits();
    let mut k = 0;
    if bits < 1 << 52 {
        bits = (x * pow2(54)).to_bits(); // subnormal: normalise exactly
        k = -54;
    }
    k += ((bits >> 52) & 0x7ff) as i32 - 1023;
    let mut m = f64::from_bits((bits & ((1 << 52) - 1)) | (0x3ff << 52));
    if m > SQRT2 {
        m *= 0.5;
        k += 1;
    }
    Ok((k, m - 1.0)) // exact
}

/// ln(1 + f) - f for f from `log_reduce`, via 2·atanh(s), s = f/(2+f) (fdlibm's arrangement).
/// Series through s²¹ (truncation < 1e-18 relative).
#[inline]
fn log1p_minus_f(f: f64) -> f64 {
    let s = f / (2.0 + f);
    let z = s * s;
    const LOG: [f64; 10] =
        [2.0 / 3.0, 2.0 / 5.0, 2.0 / 7.0, 2.0 / 9.0, 2.0 / 11.0, 2.0 / 13.0, 2.0 / 15.0, 2.0 / 17.0, 2.0 / 19.0, 2.0 / 21.0];
    let r = z * horner(z, &LOG);
    let hfsq = 0.5 * f * f;
    s * (hfsq + r) - hfsq
}

/// Natural logarithm.
pub fn ln_f64(x: f64) -> f64 {
    let (k, f) = match log_reduce(x) {
        Ok(v) => v,
        Err(special) => return special,
    };
    let dk = k as f64;
    dk * LN2_CW_HI + ((log1p_minus_f(f) + dk * LN2_CW_LO) + f)
}

/// Base-2 logarithm; exact for powers of two.
pub fn log2_f64(x: f64) -> f64 {
    let (k, f) = match log_reduce(x) {
        Ok(v) => v,
        Err(special) => return special,
    };
    // ln(1+f) = f + g; log2 = k + (f + g)·(1/ln2) with the f·INV_LN2_HI product kept exact.
    let g = log1p_minus_f(f);
    let (p, pe) = two_prod(f, INV_LN2_HI);
    let tail = pe + (f * INV_LN2_LO + g * (INV_LN2_HI + INV_LN2_LO));
    let (s, e) = two_sum(k as f64, p);
    s + (e + tail)
}

/// Natural logarithm.
pub fn ln(x: f32) -> f32 {
    ln_f64(x as f64) as f32
}

/// Base-2 logarithm; exact for powers of two.
pub fn log2(x: f32) -> f32 {
    log2_f64(x as f64) as f32
}

// ---------------------------------------------------------------------------------------------
// pow / hypot

/// `x^y` with the C99 special cases (like `f32::powf`). Evaluated as exp(y·ln x) in f64; for
/// f32 results that keeps the error ≤ 1 ulp across the whole range.
pub fn pow(x: f32, y: f32) -> f32 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f32::NAN;
    }
    let y_int = trunc(y) == y;
    // Every f32 with |y| ≥ 2^24 is even.
    let y_odd = y_int && y.abs() < 16_777_216.0 && (y as i32) & 1 == 1;
    if y.is_infinite() {
        let ax = x.abs();
        if ax == 1.0 {
            return 1.0;
        }
        return if (ax > 1.0) == (y > 0.0) { f32::INFINITY } else { 0.0 };
    }
    if x == 0.0 {
        let mag = if y < 0.0 { f32::INFINITY } else { 0.0 };
        return if y_odd { mag.copysign(x) } else { mag };
    }
    if x.is_infinite() {
        let mag = if y < 0.0 { 0.0 } else { f32::INFINITY };
        return if y_odd { mag.copysign(x) } else { mag };
    }
    let mut sign = 1.0;
    if x < 0.0 {
        if !y_int {
            return f32::NAN;
        }
        if y_odd {
            sign = -1.0;
        }
    }
    (sign * exp_f64(y as f64 * ln_f64(x.abs() as f64))) as f32
}

/// √(x² + y²) without undue overflow or underflow; `inf` if either is infinite (even with NaN).
pub fn hypot_f64(x: f64, y: f64) -> f64 {
    if x.is_infinite() || y.is_infinite() {
        return f64::INFINITY;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let (mut a, mut b) = (x.abs(), y.abs());
    if a < b {
        std::mem::swap(&mut a, &mut b);
    }
    if b == 0.0 || b < a * pow2(-60) {
        return a; // b² is below half an ulp of a²
    }
    // Here a/b ≤ 2^60, so one scale factor keeps both squares in range.
    let (scale, unscale) = if a > pow2(500) {
        (pow2(-600), pow2(600))
    } else if b < pow2(-500) {
        (pow2(600), pow2(-600))
    } else {
        (1.0, 1.0)
    };
    let (a, b) = (a * scale, b * scale);
    let (a2, a2e) = two_prod(a, a);
    let (b2, b2e) = two_prod(b, b);
    let (h, he) = two_sum(a2, b2);
    let l = he + a2e + b2e;
    let s = h.sqrt();
    // One Newton step on the double-double h + l.
    let (ss, sse) = two_prod(s, s);
    let d = ((h - ss) - sse) + l;
    (s + d / (2.0 * s)) * unscale
}

/// √(x² + y²) without undue overflow or underflow; `inf` if either is infinite (even with NaN).
pub fn hypot(x: f32, y: f32) -> f32 {
    if x.is_infinite() || y.is_infinite() {
        return f32::INFINITY;
    }
    // f32 squares are exact in f64 and can't overflow or underflow there.
    let (x, y) = (x as f64, y as f64);
    (x * x + y * y).sqrt() as f32
}

#[cfg(test)]
mod tests;
