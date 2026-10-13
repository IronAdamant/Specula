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
//! - The `f32` functions are **correctly rounded** (`f32` is what the engine uses). They take the
//!   fast `f64` result unless it is too close to an `f32` midpoint to round safely, and then
//!   recompute in double-double (see `to_f32`). Verified exhaustively for the one-argument
//!   functions (`tests::exhaustive` + `scripts/dmath_oracle.py`).
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
const PIO4_LO: f64 = f64::from_bits(0x3C81A62633145C07);
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
const fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

/// Veltkamp split into two 26-bit halves. Valid for |a| < 2⁹⁹⁶.
#[inline]
const fn split(a: f64) -> (f64, f64) {
    let t = 134_217_729.0 * a; // 2^27 + 1
    let hi = t - (t - a);
    (hi, a - hi)
}

/// `a * b = p + e` exactly (Dekker). Valid when no intermediate overflows or underflows.
#[inline]
const fn two_prod(a: f64, b: f64) -> (f64, f64) {
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
// Double-double arithmetic (value = hi + lo, |lo| ≤ ulp(hi)/2): ~106-bit intermediates for the
// accurate paths that decide f32 rounding.

type Dd = (f64, f64);

/// `a + b = s + e` exactly when |a| ≥ |b| (or a = 0).
#[inline]
const fn fast_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    (s, b - (s - a))
}

#[inline]
fn dd_add(a: Dd, b: Dd) -> Dd {
    let (s, e) = two_sum(a.0, b.0);
    let (t, f) = two_sum(a.1, b.1);
    let (s, e) = fast_two_sum(s, e + t);
    fast_two_sum(s, e + f)
}

#[inline]
fn dd_mul(a: Dd, b: Dd) -> Dd {
    let (p, e) = two_prod(a.0, b.0);
    fast_two_sum(p, e + (a.0 * b.1 + a.1 * b.0))
}

#[inline]
fn dd_mul_d(a: Dd, b: f64) -> Dd {
    let (p, e) = two_prod(a.0, b);
    fast_two_sum(p, e + a.1 * b)
}

fn dd_div(a: Dd, b: Dd) -> Dd {
    let q1 = a.0 / b.0;
    let r = dd_add(a, dd_mul_d(b, -q1));
    let q2 = r.0 / b.0;
    let r = dd_add(r, dd_mul_d(b, -q2));
    let q3 = r.0 / b.0;
    dd_add(fast_two_sum(q1, q2), (q3, 0.0))
}

#[inline]
const fn dd_neg(a: Dd) -> Dd {
    (-a.0, -a.1)
}

/// 1/n as a double-double (compile time).
const fn recip_dd(n: f64) -> Dd {
    let hi = 1.0 / n;
    let (p, e) = two_prod(hi, n);
    (hi, ((1.0 - p) - e) / n)
}

const THIRD_DD: Dd = recip_dd(3.0);
const FIFTH_DD: Dd = recip_dd(5.0);
const SIXTH_DD: Dd = recip_dd(6.0);
const LN2_DD: Dd = (LN2_HI, LN2_LO);
const INV_LN2_DD: Dd = (INV_LN2_HI, INV_LN2_LO);
const PI_DD: Dd = (PI_HI, PI_LO);
const PIO2_DD: Dd = (PIO2_HI, PIO2_LO);

// ---------------------------------------------------------------------------------------------
// Correct rounding to f32.
//
// Rounding an f64 result to f32 rounds twice: when the f64 value lands exactly on an f32 midpoint
// the tie is broken by ties-to-even instead of by the true value (17 such inputs over all 2³²
// for the first version of this module). So the f32 functions use a Ziv-style two-step scheme:
// the fast f64 result (error ≤ a few f64 ulp) is used unless it lies within `MIDPOINT_TOL` f64 ulp
// of an f32 midpoint (about 1 input in 2²³); then an accurate double-double result (relative
// error ≲ 2⁻⁶²) is narrowed with the tie decided by the sign of its low part.

/// Distance from an f32 midpoint, in f64 ulp, below which a fast result with error of a few ulp
/// can't be trusted.
const MIDPOINT_TOL: i64 = 32;

/// Whether `h` (an f64 approximation with error < `tol` ulp) might round to a different f32 than
/// the exact value.
#[inline]
fn near_f32_midpoint(h: f64, tol: i64) -> bool {
    let b = h.to_bits();
    let e = ((b >> 52) & 0x7ff) as i32;
    if e == 0 || e == 0x7ff {
        return false; // zero, f64-subnormal (far below f32), inf, NaN
    }
    // f64 significand bits below the f32 rounding position (more for f32-subnormal results).
    let drop = 29 + (-126 - (e - 1023)).max(0);
    if drop > 53 {
        return true; // below half the smallest f32 subnormal: let the accurate path decide
    }
    let m = (b & ((1 << 52) - 1)) | (1 << 52);
    let low = (m & ((1u64 << drop) - 1)) as i64;
    (low - (1i64 << (drop - 1))).abs() <= tol
}

/// Next f32 from `c` in the direction of the sign of `d`.
fn next_f32(c: f32, d: f64) -> f32 {
    if c == 0.0 {
        return f32::from_bits(1).copysign(d as f32);
    }
    let b = c.to_bits();
    f32::from_bits(if (c > 0.0) == (d > 0.0) { b + 1 } else { b - 1 })
}

/// Correctly rounds the double-double `h + l` to f32 (round to nearest, ties to even), assuming
/// `h + l` is closer to the exact value than to any f32 midpoint.
fn narrow(h: f64, l: f64) -> f32 {
    if l == 0.0 {
        return h as f32; // exact (keeps -0): a real tie keeps ties-to-even
    }
    let (h, l) = fast_two_sum(h, l);
    let c = h as f32;
    if c.is_infinite() {
        // Overflow boundary: h exactly halfway between MAX and 2^128 with the true value below.
        let mid = f32::MAX as f64 + pow2(103);
        return if h.abs() == mid && (l > 0.0) != (h > 0.0) { f32::MAX.copysign(c) } else { c };
    }
    let d = h - c as f64; // exact
    if d == 0.0 {
        return c;
    }
    let other = next_f32(c, d);
    if other as f64 - h == d && (l > 0.0) == (d > 0.0) {
        other // h is exactly a midpoint and the true value lies beyond it
    } else {
        c
    }
}

/// The fast f64 result rounded to f32, or the accurate one when the fast one is ambiguous.
#[inline]
fn to_f32(fast: f64, accurate: impl FnOnce() -> Dd) -> f32 {
    to_f32_tol(fast, MIDPOINT_TOL, accurate)
}

/// `to_f32` for a fast result whose error bound is `tol` f64 ulp.
#[inline]
fn to_f32_tol(fast: f64, tol: i64, accurate: impl FnOnce() -> Dd) -> f32 {
    if near_f32_midpoint(fast, tol) {
        narrow_slow(accurate)
    } else {
        fast as f32
    }
}

/// Out of line so the rarely used double-double code doesn't bloat the fast paths.
#[cold]
#[inline(never)]
fn narrow_slow(accurate: impl FnOnce() -> Dd) -> f32 {
    let (h, l) = accurate();
    narrow(h, l)
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

const S3_DD: Dd = dd_neg(SIXTH_DD);
const S5_DD: Dd = recip_dd(120.0);
const C4_DD: Dd = recip_dd(24.0);
const C6_DD: Dd = dd_neg(recip_dd(720.0));

/// (sin, cos)(x + y) for |x| ≲ π/4 as double-doubles: the leading terms in double-double,
/// the rest (≤ x⁷/5040) in double. Relative error ≲ 2⁻⁶⁶.
fn sin_cos_kernel_dd(x: f64, y: f64) -> (Dd, Dd) {
    let xy: Dd = (x, y);
    let z = dd_mul(xy, xy);
    let u = z.0;
    let qs = dd_add(S3_DD, dd_mul(z, dd_add(S5_DD, (u * horner(u, &SIN[2..]), 0.0))));
    let sin = dd_add(xy, dd_mul(dd_mul(z, xy), qs));
    let qc = dd_add(C4_DD, dd_mul(z, dd_add(C6_DD, (u * horner(u, &COS[2..]), 0.0))));
    let cos = dd_add((1.0, 0.0), dd_add((-0.5 * z.0, -0.5 * z.1), dd_mul(dd_mul(z, z), qc)));
    (sin, cos)
}

/// Accurate (sin x, cos x) for finite x as double-doubles.
fn sin_cos_dd(x: f64) -> (Dd, Dd) {
    if x.abs() < TINY {
        return ((x, -x * x * x / 6.0), (1.0, -0.5 * x * x));
    }
    let (q, a, b) = rem_pio2(x);
    let (s, c) = sin_cos_kernel_dd(a, b);
    match q {
        0 => (s, c),
        1 => (c, dd_neg(s)),
        2 => (dd_neg(s), dd_neg(c)),
        _ => (dd_neg(c), s),
    }
}

fn tan_dd(x: f64) -> Dd {
    if x.abs() < TINY {
        return (x, x * x * x / 3.0); // keeps ±0
    }
    let (s, c) = sin_cos_dd(x);
    dd_div(s, c)
}

pub fn sin(x: f32) -> f32 {
    let x = x as f64;
    to_f32(sin_f64(x), || sin_cos_dd(x).0)
}

pub fn cos(x: f32) -> f32 {
    let x = x as f64;
    to_f32(cos_f64(x), || sin_cos_dd(x).1)
}

/// `(sin x, cos x)` with one range reduction.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let x = x as f64;
    let (s, c) = sin_cos_f64(x);
    (to_f32(s, || sin_cos_dd(x).0), to_f32(c, || sin_cos_dd(x).1))
}

pub fn tan(x: f32) -> f32 {
    let x = x as f64;
    to_f32(tan_f64(x), || tan_dd(x))
}

// ---------------------------------------------------------------------------------------------
// atan / atan2 / asin / acos
//
// One implementation, two precisions: `ACC = false` evaluates the series tail in double (≤ 1 ulp
// in f64, the fast path), `ACC = true` in double-double (the accurate path for f32 rounding).

const ATAN: [f64; 6] = [1.0 / 5.0, -1.0 / 7.0, 1.0 / 9.0, -1.0 / 11.0, 1.0 / 13.0, -1.0 / 15.0];

/// atan(t) - t for |t| ≤ 1/16: t³·(-1/3 + t²/5 - …), series through t¹⁵ (truncation < 4e-21
/// relative); with `ACC` the -1/3 term in double-double (total ≲ 2⁻⁷⁰).
#[inline]
fn atan_tail<const ACC: bool>(t: f64) -> Dd {
    if ACC {
        let z = two_prod(t, t);
        let p = dd_add(dd_neg(THIRD_DD), (z.0 * horner(z.0, &ATAN), 0.0));
        dd_mul(dd_mul_d(z, t), p)
    } else {
        let z = t * t;
        (t * z * (-1.0 / 3.0 + z * horner(z, &ATAN)), 0.0)
    }
}

/// atan(a + a_lo) for a ∈ [0, 1], |a_lo| ≲ ulp(a), as a double-double:
/// atan(c) + atan((a - c)/(1 + ac)) with c = k/8 the nearest eighth, the quotient kept as a
/// double-double so the table split adds no error.
#[inline]
fn atan_unit<const ACC: bool>(a: f64, a_lo: f64) -> Dd {
    let k = round_f64(a * 8.0);
    if k == 0.0 {
        // atan(a + a_lo) ≈ atan(a) + a_lo·(1 - a²)
        let tail = atan_tail::<ACC>(a);
        return if ACC {
            dd_add(fast_two_sum(a, a_lo * (1.0 - a * a)), tail)
        } else {
            (a, tail.0 + a_lo * (1.0 - a * a))
        };
    }
    let c = k * 0.125;
    let num = a - c; // exact (Sterbenz)
    let (p, pe) = two_prod(a, c);
    let (d, de) = two_sum(1.0, p);
    let de = de + pe + a_lo * c;
    let t = num / d;
    let (q, qe) = two_prod(t, d);
    let t_lo = (((num - q) - qe) + a_lo - t * de) / d;
    let tail = atan_tail::<ACC>(t);
    let (hi, lo) = ATAN_K8[k as usize];
    if ACC {
        dd_add((hi, lo), dd_add(fast_two_sum(t, t_lo * (1.0 - t * t)), tail))
    } else {
        let (s, e) = two_sum(hi, t);
        (s, e + (lo + (tail.0 + t_lo * (1.0 - t * t))))
    }
}

/// atan(n/d) for 0 ≤ n ≤ d, 0 < d < inf, with both given as double-doubles.
#[inline]
fn atan_ratio<const ACC: bool>(n: f64, n_lo: f64, d: f64, d_lo: f64) -> Dd {
    // Scale by a power of two so d ∈ [1, 2): exact (n may underflow only where the ratio is
    // negligible) and keeps `two_prod` in range.
    let e = 1023 - ((d.to_bits() >> 52) & 0x7ff) as i32;
    let (n, n_lo, d, d_lo) = (scalbn(n, e), scalbn(n_lo, e), scalbn(d, e), scalbn(d_lo, e));
    let a = n / d;
    let (p, pe) = two_prod(a, d);
    atan_unit::<ACC>(a, (((n - p) - pe) + n_lo - a * d_lo) / d)
}

/// Angle of the vector (x, y) for x, y ≥ 0 finite, not both zero, given as double-doubles.
#[inline]
fn angle_first_quadrant<const ACC: bool>(y: f64, y_lo: f64, x: f64, x_lo: f64) -> Dd {
    if y <= x {
        atan_ratio::<ACC>(y, y_lo, x, x_lo)
    } else {
        dd_add(PIO2_DD, dd_neg(atan_ratio::<ACC>(x, x_lo, y, y_lo)))
    }
}

/// Copies the sign of `s` onto a double-double.
fn dd_copysign(a: Dd, s: f64) -> Dd {
    if (a.0 < 0.0) == s.is_sign_negative() || a.0.is_nan() {
        (a.0.copysign(s), a.1)
    } else {
        dd_neg(a)
    }
}

/// atan for x not NaN.
fn atan_dd<const ACC: bool>(x: f64) -> Dd {
    let ax = x.abs();
    let a = if ax <= 1.0 {
        atan_unit::<ACC>(ax, 0.0)
    } else if ax.is_infinite() {
        PIO2_DD
    } else {
        dd_add(PIO2_DD, dd_neg(atan_ratio::<ACC>(1.0, 0.0, ax, 0.0)))
    };
    dd_copysign(a, x)
}

/// atan2 as a double-double; special cases give exact (v, 0).
fn atan2_dd<const ACC: bool>(y: f64, x: f64) -> Dd {
    if x.is_nan() || y.is_nan() {
        return (f64::NAN, 0.0);
    }
    let (ay, ax) = (y.abs(), x.abs());
    let x_neg = x.is_sign_negative();
    let r = if ay == 0.0 {
        if x_neg {
            PI_DD
        } else {
            (0.0, 0.0)
        }
    } else if ax == 0.0 {
        PIO2_DD
    } else if ay.is_infinite() {
        match (ax.is_infinite(), x_neg) {
            (true, false) => (PIO4_HI, PIO4_LO),
            (true, true) => dd_add(PIO2_DD, (PIO4_HI, PIO4_LO)),
            _ => PIO2_DD,
        }
    } else if ax.is_infinite() {
        if x_neg {
            PI_DD
        } else {
            (0.0, 0.0)
        }
    } else {
        let a = angle_first_quadrant::<ACC>(ay, 0.0, ax, 0.0);
        if x_neg {
            dd_add(PI_DD, dd_neg(a))
        } else {
            a
        }
    };
    dd_copysign(r, y)
}

/// √(1 - x²) for 0 ≤ x ≤ 1 as a double-double (1 - x² is formed exactly).
fn sqrt_one_minus_sq(x: f64) -> Dd {
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

/// asin for |x| ≤ 1 (not NaN).
fn asin_dd<const ACC: bool>(x: f64) -> Dd {
    let ax = x.abs();
    if ax < TINY {
        return (x, x * x * x / 6.0);
    }
    let (d, d_lo) = sqrt_one_minus_sq(ax);
    dd_copysign(angle_first_quadrant::<ACC>(ax, 0.0, d, d_lo), x)
}

/// acos for |x| ≤ 1 (not NaN).
fn acos_dd<const ACC: bool>(x: f64) -> Dd {
    let (d, d_lo) = sqrt_one_minus_sq(x.abs());
    let a = angle_first_quadrant::<ACC>(d, d_lo, x.abs(), 0.0);
    if x.is_sign_negative() {
        dd_add(PI_DD, dd_neg(a))
    } else {
        a
    }
}

pub fn atan_f64(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let (h, l) = atan_dd::<false>(x);
    h + l
}

/// Angle of the point (x, y) in [-π, π], with the C99 special cases (`atan2(±0, -0) = ±π`, …).
pub fn atan2_f64(y: f64, x: f64) -> f64 {
    let (h, l) = atan2_dd::<false>(y, x);
    h + l
}

pub fn asin_f64(x: f64) -> f64 {
    if x.is_nan() || x.abs() > 1.0 {
        return f64::NAN;
    }
    let (h, l) = asin_dd::<false>(x);
    h + l
}

pub fn acos_f64(x: f64) -> f64 {
    if x.is_nan() || x.abs() > 1.0 {
        return f64::NAN;
    }
    let (h, l) = acos_dd::<false>(x);
    h + l
}

pub fn atan(x: f32) -> f32 {
    let x = x as f64;
    if x.is_nan() {
        return f32::NAN;
    }
    to_f32(atan_f64(x), || atan_dd::<true>(x))
}

/// Angle of the point (x, y) in [-π, π].
pub fn atan2(y: f32, x: f32) -> f32 {
    let (y, x) = (y as f64, x as f64);
    to_f32(atan2_f64(y, x), || atan2_dd::<true>(y, x))
}

pub fn asin(x: f32) -> f32 {
    if x.is_nan() || x.abs() > 1.0 {
        return f32::NAN;
    }
    let x = x as f64;
    to_f32(asin_f64(x), || asin_dd::<true>(x))
}

pub fn acos(x: f32) -> f32 {
    if x.is_nan() || x.abs() > 1.0 {
        return f32::NAN;
    }
    let x = x as f64;
    to_f32(acos_f64(x), || acos_dd::<true>(x))
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

/// e^(r + re) for |r| ≲ ln2/2 as a double-double: 1 + r + r²/2 + r³/6 in double-double, the
/// rest (≤ r⁴/24) in double. Relative error ≲ 2⁻⁶³.
fn exp_kernel_dd(r: f64, re: f64) -> Dd {
    let rr: Dd = (r, re);
    let r2 = dd_mul(rr, rr);
    let r3_6 = dd_mul(dd_mul(r2, rr), SIXTH_DD);
    let tail = r2.0 * r2.0 * horner(r, &EXP[2..]);
    let s = dd_add(r3_6, (tail, 0.0));
    let s = dd_add((0.5 * r2.0, 0.5 * r2.1), s);
    dd_add((1.0, 0.0), dd_add(rr, s))
}

/// e^(t.0 + t.1) as a double-double, for EXP_UNDERFLOW ≤ t.0 ≤ EXP_OVERFLOW.
fn exp_dd(t: Dd) -> Dd {
    let k = round_f64(t.0 * INV_LN2_HI);
    let hi = t.0 - k * LN2_CW_HI; // exact
    let (r, re) = two_sum(hi, -(k * LN2_CW_LO));
    let (r, re) = fast_two_sum(r, re + t.1);
    let (h, l) = exp_kernel_dd(r, re);
    (scalbn(h, k as i32), scalbn(l, k as i32))
}

/// 2^x as a double-double, for -1075 < x < 1024.
fn exp2_dd(x: f64) -> Dd {
    let k = round_f64(x);
    let f = x - k; // exact
    let (h, l) = two_prod(f, LN2_HI);
    let (r, re) = fast_two_sum(h, l + f * LN2_LO);
    let (h, l) = exp_kernel_dd(r, re);
    (scalbn(h, k as i32), scalbn(l, k as i32))
}

pub fn exp(x: f32) -> f32 {
    let x = x as f64;
    to_f32(exp_f64(x), || exp_dd((x, 0.0)))
}

pub fn exp2(x: f32) -> f32 {
    let x = x as f64;
    to_f32(exp2_f64(x), || exp2_dd(x))
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

/// ln(1 + f) for f from `log_reduce` as a double-double: 2s·(1 + s²/3 + s⁴/5 + …), s = f/(2+f),
/// with s and the first two terms in double-double. Relative error ≲ 2⁻⁷⁰.
fn ln1p_dd(f: f64) -> Dd {
    const TAIL: [f64; 12] = [
        1.0 / 7.0, 1.0 / 9.0, 1.0 / 11.0, 1.0 / 13.0, 1.0 / 15.0, 1.0 / 17.0, 1.0 / 19.0, 1.0 / 21.0, 1.0 / 23.0,
        1.0 / 25.0, 1.0 / 27.0, 1.0 / 29.0,
    ];
    let s = dd_div((f, 0.0), two_sum(2.0, f));
    let s2 = dd_mul(s, s);
    let u = s2.0;
    let inner = dd_add(FIFTH_DD, (u * horner(u, &TAIL), 0.0));
    let b = dd_mul(s2, dd_add(THIRD_DD, dd_mul(s2, inner)));
    dd_mul((2.0 * s.0, 2.0 * s.1), dd_add((1.0, 0.0), b))
}

/// ln x for finite x > 0 as a double-double.
fn ln_dd(x: f64) -> Dd {
    let (k, f) = log_reduce(x).unwrap_or((0, f64::NAN));
    dd_add(dd_mul_d(LN2_DD, k as f64), ln1p_dd(f))
}

/// log₂ x for finite x > 0 as a double-double (exact for powers of two).
fn log2_dd(x: f64) -> Dd {
    let (k, f) = log_reduce(x).unwrap_or((0, f64::NAN));
    dd_add((k as f64, 0.0), dd_mul(ln1p_dd(f), INV_LN2_DD))
}

/// Natural logarithm.
pub fn ln(x: f32) -> f32 {
    let x = x as f64;
    to_f32(ln_f64(x), || ln_dd(x))
}

/// Base-2 logarithm; exact for powers of two.
pub fn log2(x: f32) -> f32 {
    let x = x as f64;
    to_f32(log2_f64(x), || log2_dd(x))
}

// ---------------------------------------------------------------------------------------------
// pow / hypot

/// |x|^y for finite |x| ∉ {0, 1} and finite y ≠ 0, as a double-double that is **exact** whenever
/// the result is an f32 value or an f32 midpoint (so ties round correctly), accurate to ≲ 2⁻⁶²
/// otherwise.
///
/// Exact results only arise from (a) a power-of-two base with y·log₂|x| an integer, or (b) an
/// integer power of a base after taking exact square roots (y = n/2^j): both are computed
/// exactly; everything else goes through exp(y·ln|x|) in double-double.
fn pow_dd(ax: f32, y: f32) -> Dd {
    let bits = ax.to_bits();
    let (mut b, mut e) = (ax as f64, y as f64);
    if bits & 0x007f_ffff == 0 || bits < 0x0080_0000 && bits.is_power_of_two() {
        // ax = 2^j: the result is 2^(j·y), j·y exact in f64.
        let j = if bits < 0x0080_0000 { bits.trailing_zeros() as i32 - 149 } else { (bits >> 23) as i32 - 127 };
        let t = j as f64 * e;
        return if t >= 1024.0 {
            (f64::INFINITY, 0.0)
        } else if t <= -1075.0 {
            (0.0, 0.0)
        } else {
            exp2_dd(t)
        };
    }
    // Take exact square roots while y has a fractional part: x^(n/2^j) = (x^(1/2^j))^n.
    while trunc_f64(e) != e {
        let s = b.sqrt();
        let (p, pe) = two_prod(s, s);
        if p != b || pe != 0.0 {
            break;
        }
        (b, e) = (s, e * 2.0);
    }
    if trunc_f64(e) == e && e.abs() <= 64.0 {
        // b ∈ [2^lg, 2^(lg+1)): decide certain overflow/underflow first so the products stay
        // far inside the double range (two_prod needs |v| < 2^996).
        let lg = ((b.to_bits() >> 52) & 0x7ff) as i32 - 1023;
        let n = e.abs() as i32;
        if lg >= 0 && lg * n > 300 || lg < 0 && (lg + 1) * n < -300 {
            let big = (lg >= 0) == (e > 0.0);
            return if big { (f64::INFINITY, 0.0) } else { (0.0, 0.0) };
        }
        // Binary powering in double-double: exact while the result fits in 106 bits, which
        // covers every result with ≤ 25 significant bits.
        let (mut acc, mut base, mut m) = ((1.0, 0.0), (b, 0.0), n as u32);
        while m > 0 {
            if m & 1 == 1 {
                acc = dd_mul(acc, base);
            }
            m >>= 1;
            if m > 0 {
                base = dd_mul(base, base);
            }
        }
        return if e < 0.0 { dd_div((1.0, 0.0), acc) } else { acc };
    }
    let t = dd_mul_d(ln_dd(b), e);
    if t.0 > EXP_OVERFLOW {
        (f64::INFINITY, 0.0)
    } else if t.0 < EXP_UNDERFLOW {
        (0.0, 0.0)
    } else {
        exp_dd(t)
    }
}

/// `x^y` with the C99 special cases (like `f32::powf`), correctly rounded, exact cases included
/// (`pow(x, 2) == x * x`, `pow(x, 0.5) == sqrt(x)`, `pow(x, -1) == 1 / x`).
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
    if x < 0.0 && !y_int {
        return f32::NAN;
    }
    // IEEE-exact operations for the commonest exponents.
    match y {
        1.0 => return x,
        -1.0 => return 1.0 / x,
        2.0 => return x * x,
        0.5 => return x.sqrt(), // x > 0 here
        _ => {}
    }
    // Fast path exp(y·ln|x|) in f64: ln's 1-ulp error grows by |t| through exp, so the trust
    // radius around f32 midpoints widens with |t| (exact midpoint results always fall inside it).
    let ax = x.abs();
    let t = y as f64 * ln_f64(ax as f64);
    let r = to_f32_tol(exp_f64(t), MIDPOINT_TOL + 4 * t.abs().min(1e6) as i64, || pow_dd(ax, y));
    if x < 0.0 && y_odd {
        -r
    } else {
        r
    }
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
    if x.is_nan() || y.is_nan() {
        return f32::NAN;
    }
    // f32 squares are exact in f64 (no overflow or underflow there), so the fast result has two
    // roundings; near a midpoint, x² + y² as an exact double-double and one Newton step give the
    // root to ~2⁻¹⁰⁰.
    let (x, y) = (x as f64, y as f64);
    to_f32((x * x + y * y).sqrt(), || {
        let (h, l) = two_sum(x * x, y * y);
        let s = h.sqrt();
        let (ss, sse) = two_prod(s, s);
        (s, (((h - ss) - sse) + l) / (2.0 * s))
    })
}

#[cfg(test)]
mod tests;
