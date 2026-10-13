//! Accuracy, special values and golden bits for `dmath`.
//!
//! Accuracy is measured against `std` (the platform libm) only as an oracle: for f32 the reference
//! is the f64 std result rounded to f32, which is correctly rounded except in rare double-rounding
//! cases. Huge-argument trig is checked against values computed offline with exact arithmetic.

use super::*;

/// Distance in units in the last place (±0 are equal; NaN only matches NaN).
fn ulps32(a: f32, b: f32) -> u64 {
    fn ord(x: f32) -> i64 {
        let b = x.to_bits();
        if b >> 31 == 1 {
            -((b & 0x7fff_ffff) as i64)
        } else {
            b as i64
        }
    }
    match (a.is_nan(), b.is_nan()) {
        (true, true) => 0,
        (false, false) => (ord(a) - ord(b)).unsigned_abs(),
        _ => u64::MAX,
    }
}

fn ulps64(a: f64, b: f64) -> u64 {
    fn ord(x: f64) -> i128 {
        let b = x.to_bits();
        if b >> 63 == 1 {
            -((b & 0x7fff_ffff_ffff_ffff) as i128)
        } else {
            b as i128
        }
    }
    match (a.is_nan(), b.is_nan()) {
        (true, true) => 0,
        (false, false) => (ord(a) - ord(b)).unsigned_abs().min(u64::MAX as u128) as u64,
        _ => u64::MAX,
    }
}

/// xorshift64* — test inputs only.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    /// ±10^[lo_exp, hi_exp), log-uniform.
    fn log_signed(&mut self, lo_exp: f64, hi_exp: f64) -> f64 {
        let m = 10f64.powf(self.range(lo_exp, hi_exp));
        if self.next() & 1 == 1 {
            -m
        } else {
            m
        }
    }
}

fn sweep(lo: f64, hi: f64, n: usize) -> impl Iterator<Item = f64> {
    (0..=n).map(move |i| lo + (hi - lo) * i as f64 / n as f64)
}

/// Max ulp error of an f32 function against `reference(x as f64) as f32`.
fn max_err32(name: &str, f: impl Fn(f32) -> f32, reference: impl Fn(f64) -> f64, xs: impl Iterator<Item = f64>) -> u64 {
    let (mut worst, mut at, mut n) = (0, 0.0f32, 0);
    for x in xs {
        let x = x as f32;
        let e = ulps32(f(x), reference(x as f64) as f32);
        if e > worst {
            (worst, at) = (e, x);
        }
        n += 1;
    }
    println!("{name:>8} f32: max {worst} ulp over {n} inputs (worst at {at:?})");
    worst
}

fn max_err64(name: &str, f: impl Fn(f64) -> f64, reference: impl Fn(f64) -> f64, xs: impl Iterator<Item = f64>) -> u64 {
    let (mut worst, mut at, mut n) = (0, 0.0, 0);
    for x in xs {
        let e = ulps64(f(x), reference(x));
        if e > worst {
            (worst, at) = (e, x);
        }
        n += 1;
    }
    println!("{name:>8} f64: max {worst} ulp over {n} inputs (worst at {at:?})");
    worst
}

const N: usize = 200_000;

type Named32 = (&'static str, fn(f32) -> f32);
type Named64 = (&'static str, fn(f64) -> f64);

fn trig_inputs(seed: u64) -> impl Iterator<Item = f64> {
    let mut rng = Rng(seed);
    sweep(-10.0, 10.0, N).chain((0..N).map(move |_| rng.log_signed(-8.0, 6.0)))
}

#[test]
fn trig_accuracy_f32() {
    assert!(max_err32("sin", sin, f64::sin, trig_inputs(1)) <= 1);
    assert!(max_err32("cos", cos, f64::cos, trig_inputs(2)) <= 1);
    assert!(max_err32("tan", tan, f64::tan, trig_inputs(3)) <= 1);
    assert!(max_err32("sin_cos.0", |x| sin_cos(x).0, f64::sin, trig_inputs(4)) <= 1);
    assert!(max_err32("sin_cos.1", |x| sin_cos(x).1, f64::cos, trig_inputs(4)) <= 1);
}

/// std is only a usable f64 oracle away from the zeros of the function: some libms (mingw-w64's
/// x87 `fsin`, measured here) reduce with a 66-bit π, so `f64::sin(-6.2832)` is 5 ulp off,
/// `f64::sin(992.744…)` 6 ulp and `f64::sin(-946140.89…)` 11 000 ulp. So this compares only for
/// |x| ≤ 100 and results away from zero; zeros and large arguments are checked against exact
/// references in `trig_reference_values`.
#[test]
fn trig_accuracy_f64() {
    let inputs = |seed, f: fn(f64) -> f64| {
        let mut rng = Rng(seed);
        sweep(-100.0, 100.0, N)
            .chain((0..N).map(move |_| rng.log_signed(-8.0, 2.0)))
            .filter(move |&x| x.abs() < 0.78 || (1e-2..=1e2).contains(&f(x).abs()))
    };
    let inputs = |seed, f| inputs(seed, f).chain(sweep(-0.78, 0.78, N / 4));
    assert!(max_err64("sin", sin_f64, f64::sin, inputs(5, f64::sin)) <= 1);
    assert!(max_err64("cos", cos_f64, f64::cos, inputs(6, f64::cos)) <= 1);
    assert!(max_err64("tan", tan_f64, f64::tan, inputs(7, f64::tan)) <= 2);
}

/// sin/cos of medium and huge arguments (both reduction paths), reference computed with 1600-bit
/// exact arithmetic (Python). Some rows sit at π, π/2 and π/4 on purpose.
#[allow(clippy::approx_constant)]
#[test]
fn trig_reference_values() {
    const CASES: [(f64, f64, f64); 26] = [
        (355.0, -3.014435335948845e-05, -0.999999999545659),
        (103993.0, -1.9129335778423752e-05, 0.9999999998170342),
        (-946140.8922534189, 0.000857604646629783, 0.9999996322570675),
        (52.5, 0.7877052269841179, -0.616052331690985),
        (1000.0, 0.8268795405320025, 0.5623790762907029),
        (12345.678, -0.7040813137533816, 0.7101193587160628),
        (-98765.4321, -0.04224386964079158, 0.9991073293084041),
        (1570796.0, -0.3210092056776958, 0.9470760739614188),
        (6.283185307179586, -2.4492935982947064e-16, 1.0),
        (3.141592653589793, 1.2246467991473532e-16, -1.0),
        (1.5707963267948966, 1.0, 6.123233995736766e-17),
        (100.0, -0.5063656411097588, 0.8623188722876839),
        (0.7853981633974483, 0.7071067811865475, 0.7071067811865476),
        (710.0, 6.0288706691585265e-05, 0.999999998182636),
        (-33.3, -0.9513287387867827, -0.3081779206211071),
        (1e22, -0.8522008497671888, 0.523214785395139),
        (1e300, -0.8178819121159085, -0.5753861119575491),
        (1.7976931348623157e308, 0.004961954789184062, -0.9999876894265599),
        (3.4028234663852886e38, -0.5218765233336585, 0.8530210398303042),
        (1e10, -0.4875060250875107, 0.873119622676856),
        (123456792.0, -0.9604127276688877, -0.27858103404863527),
        (1.0715086071862673e301, -0.15920170308624243, 0.9872460775989135),
        (5e15, -0.901711760523585, -0.432337716297638),
        (1.6e6, -0.5414010921983009, 0.8407644482056118),
        (1647100.0, 0.6216400379214212, 0.7833030468809974),
        (1e7, 0.4205477931907825, -0.9072703861817396),
    ];
    for (x, s, c) in CASES {
        for sign in [1.0, -1.0] {
            let x = x * sign;
            assert!(ulps64(sin_f64(x), s * sign) <= 1, "sin({x:e}) = {} want {}", sin_f64(x), s * sign);
            assert!(ulps64(cos_f64(x), c) <= 1, "cos({x:e}) = {} want {c}", cos_f64(x));
            let (ss, cc) = sin_cos_f64(x);
            assert_eq!((ss.to_bits(), cc.to_bits()), (sin_f64(x).to_bits(), cos_f64(x).to_bits()));
        }
    }
    assert_eq!(sin(f32::MAX), -0.5218765233336585f64 as f32);
    assert_eq!(cos(f32::MAX), 0.8530210398303042f64 as f32);
}

#[test]
fn inverse_trig_accuracy() {
    let mut rng = Rng(11);
    let wide = move |rng: &mut Rng| rng.log_signed(-30.0, 30.0);
    let atan_in = sweep(-4.0, 4.0, N).chain((0..N).map({
        let mut r = Rng(12);
        move |_| wide(&mut r)
    }));
    assert!(max_err32("atan", atan, f64::atan, atan_in) <= 1);
    let atan_in = sweep(-4.0, 4.0, N).chain((0..N).map({
        let mut r = Rng(13);
        move |_| wide(&mut r)
    }));
    assert!(max_err64("atan", atan_f64, f64::atan, atan_in) <= 2);

    let unit = || sweep(-1.0, 1.0, N).chain((0..N).map({
        let mut r = Rng(14);
        // Cluster near ±1 and 0 where the reductions are hardest.
        move |i| {
            let t = 10f64.powf(r.range(-12.0, 0.0));
            let v = if i % 2 == 0 { 1.0 - t } else { t };
            if r.next() & 1 == 1 {
                -v
            } else {
                v
            }
        }
    }));
    assert!(max_err32("asin", asin, f64::asin, unit()) <= 1);
    assert!(max_err32("acos", acos, f64::acos, unit()) <= 1);
    assert!(max_err64("asin", asin_f64, f64::asin, unit()) <= 2);
    assert!(max_err64("acos", acos_f64, f64::acos, unit()) <= 2);

    let (mut w32, mut w64) = (0, 0);
    for _ in 0..N {
        let (y, x) = (rng.log_signed(-20.0, 20.0), rng.log_signed(-20.0, 20.0));
        let (yf, xf) = (y as f32, x as f32);
        w32 = w32.max(ulps32(atan2(yf, xf), (yf as f64).atan2(xf as f64) as f32));
        w64 = w64.max(ulps64(atan2_f64(y, x), y.atan2(x)));
    }
    println!("   atan2 f32: max {w32} ulp; f64: max {w64} ulp");
    assert!(w32 <= 1 && w64 <= 2);
}

#[test]
fn exp_log_accuracy() {
    let exp_in = |seed| {
        let mut r = Rng(seed);
        sweep(-104.0, 89.0, N).chain((0..N).map(move |_| r.log_signed(-10.0, 2.0)))
    };
    assert!(max_err32("exp", exp, f64::exp, exp_in(21)) <= 1);
    assert!(max_err32("exp2", exp2, f64::exp2, sweep(-150.0, 128.0, N).chain(exp_in(22))) <= 1);
    let exp64_in = |seed| {
        let mut r = Rng(seed);
        sweep(-745.0, 709.0, N).chain((0..N).map(move |_| r.log_signed(-10.0, 2.8)))
    };
    assert!(max_err64("exp", exp_f64, f64::exp, exp64_in(23)) <= 1);
    assert!(max_err64("exp2", exp2_f64, f64::exp2, sweep(-1074.0, 1023.0, N).chain(exp64_in(24))) <= 1);

    // Logs: every exponent including subnormals, plus a dense sweep around 1.
    let log_in = |seed| {
        let mut r = Rng(seed);
        sweep(0.5, 2.0, N).chain((0..N).map(move |_| f32::from_bits((r.next() as u32) & 0x7f7f_ffff) as f64))
    };
    assert!(max_err32("ln", ln, f64::ln, log_in(31)) <= 1);
    assert!(max_err32("log2", log2, f64::log2, log_in(32)) <= 1);
    let log64_in = |seed| {
        let mut r = Rng(seed);
        sweep(0.5, 2.0, N).chain((0..N).map(move |_| f64::from_bits(r.next() & 0x7fef_ffff_ffff_ffff)))
    };
    assert!(max_err64("ln", ln_f64, f64::ln, log64_in(33)) <= 1);
    assert!(max_err64("log2", log2_f64, f64::log2, log64_in(34)) <= 1);
}

#[test]
fn pow_hypot_accuracy() {
    let mut rng = Rng(41);
    let (mut wp, mut wh, mut wh64) = (0, 0, 0);
    for i in 0..N {
        let x = rng.log_signed(-6.0, 6.0).abs() as f32;
        // Mix integer and fractional exponents, and negative bases with integer exponents.
        let (x, y) = match i % 3 {
            0 => (x, rng.range(-12.0, 12.0) as f32),
            1 => (-x, rng.range(-12.0, 12.0).round() as f32),
            _ => (x, rng.range(-0.5, 0.5) as f32),
        };
        wp = wp.max(ulps32(pow(x, y), (x as f64).powf(y as f64) as f32));
        let (a, b) = (rng.log_signed(-40.0, 38.0) as f32, rng.log_signed(-40.0, 38.0) as f32);
        wh = wh.max(ulps32(hypot(a, b), (a as f64).hypot(b as f64) as f32));
        let (a, b) = (rng.log_signed(-300.0, 300.0), rng.log_signed(-300.0, 300.0));
        wh64 = wh64.max(ulps64(hypot_f64(a, b), a.hypot(b)));
    }
    println!("     pow f32: max {wp} ulp; hypot f32: max {wh} ulp; hypot f64: max {wh64} ulp");
    assert!(wp <= 1 && wh <= 1 && wh64 <= 1);
    // Exact cases, including exact midpoints (which must tie to even, not follow the error).
    assert_eq!(pow(2.0, 10.0), 1024.0);
    assert_eq!(pow(10.0, 3.0), 1000.0);
    assert_eq!(pow(-2.0, 3.0), -8.0);
    assert_eq!(pow(9.0, 0.5), 3.0);
    assert_eq!(pow(3.0, 15.0), 14_348_907.0);
    assert_eq!(pow(4097.0, 2.0), 16_785_408.0); // 16785409 is a midpoint
    assert_eq!(pow(257.0, 3.0), 16_974_592.0); // 16974593 is a midpoint
    assert_eq!(pow(66049.0, 1.5), 16_974_592.0); // = 257³ via an exact square root
    assert_eq!(pow(16.0, 0.25), 2.0);
    assert_eq!(pow(2.0, -150.0), 0.0); // 2⁻¹⁵⁰ is the midpoint between 0 and the smallest subnormal
    assert_eq!(pow(0.25, 75.0), 0.0);
    assert_eq!(pow(2.0, -149.0), f32::from_bits(1));
    assert_eq!(pow(-0.5, -3.0), -8.0);
    // The IEEE-exact operations for the commonest exponents, over random inputs.
    for _ in 0..N {
        let x = f32::from_bits(rng.next() as u32 & 0x7fff_ffff);
        if !x.is_finite() || x == 0.0 {
            continue;
        }
        assert_eq!(pow(x, 2.0).to_bits(), (x * x).to_bits());
        assert_eq!(pow(x, 0.5).to_bits(), x.sqrt().to_bits());
        assert_eq!(pow(x, -1.0).to_bits(), (1.0 / x).to_bits());
        assert_eq!(pow(-x, 3.0).to_bits(), (-pow(x, 3.0)).to_bits());
    }
    assert_eq!(hypot(3.0, 4.0), 5.0);
    assert_eq!(hypot_f64(3e300, 4e300), 5e300);
    assert_eq!(hypot_f64(3e-310, 4e-310), 5e-310);
}

/// `round` is checked by its definition, not against std: mingw-w64's `roundf`/`round` (what
/// `f32::round` calls on windows-gnu) return 1.0 for 0.49999997, the classic `floor(x + 0.5)` bug.
fn check_round(r: f64, x: f64) {
    if x.is_nan() {
        return assert!(r.is_nan());
    }
    let d = (x - r).abs();
    let ok = r == r.trunc()
        && r.is_sign_negative() == x.is_sign_negative()
        && (d < 0.5 || (d == 0.5 && r.abs() > x.abs()) || (x.is_infinite() && r == x));
    assert!(ok, "round({x:e}) = {r:e}");
}

#[test]
fn rounding_helpers_match_std_bit_for_bit() {
    assert_eq!(round(0.49999997), 0.0);
    assert_eq!(round_f64(0.49999999999999994), 0.0);
    assert_eq!(round(-8388609.0), -8388609.0);
    let mut rng = Rng(51);
    let check32 = |x: f32| {
        for (ours, theirs, name) in [
            (trunc(x), x.trunc(), "trunc"),
            (floor(x), x.floor(), "floor"),
            (ceil(x), x.ceil(), "ceil"),
            (round_ties_even(x), x.round_ties_even(), "round_ties_even"),
            (fract(x), x.fract(), "fract"),
        ] {
            assert!(ours.to_bits() == theirs.to_bits() || (ours.is_nan() && theirs.is_nan()), "{name}({x:e})");
        }
        check_round(round(x) as f64, x as f64);
    };
    for x in sweep(-8.0, 8.0, 4096) {
        check32(x as f32);
    }
    for _ in 0..N {
        check32(f32::from_bits(rng.next() as u32));
    }
    for x in [0.5, 1.5, 2.5, -0.5, -2.5, 0.49999997, 8388607.5, -0.0, 0.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        check32(x);
    }
    let check64 = |x: f64| {
        for (ours, theirs, name) in [
            (trunc_f64(x), x.trunc(), "trunc"),
            (floor_f64(x), x.floor(), "floor"),
            (ceil_f64(x), x.ceil(), "ceil"),
            (round_ties_even_f64(x), x.round_ties_even(), "round_ties_even"),
            (fract_f64(x), x.fract(), "fract"),
        ] {
            assert!(ours.to_bits() == theirs.to_bits() || (ours.is_nan() && theirs.is_nan()), "{name}({x:e})");
        }
        check_round(round_f64(x), x);
    };
    for x in sweep(-8.0, 8.0, 4096) {
        check64(x);
    }
    for _ in 0..N {
        check64(f64::from_bits(rng.next()));
    }
    for x in [0.5, 1.5, 2.5, -0.5, -2.5, 0.49999999999999994, 4503599627370495.5, -0.0, f64::INFINITY, f64::NAN] {
        check64(x);
    }
}

#[test]
fn special_values() {
    let (inf, nan) = (f32::INFINITY, f32::NAN);
    let same = |a: f32, b: f32| a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan());
    for f in [sin, tan, atan, asin] {
        assert!(same(f(0.0), 0.0) && same(f(-0.0), -0.0));
        assert!(f(nan).is_nan());
        assert!(same(f(1e-30), 1e-30) && same(f(-1e-45), -1e-45)); // tiny and subnormal
    }
    for f in [sin, cos, tan] {
        assert!(f(inf).is_nan() && f(-inf).is_nan());
    }
    assert_eq!(cos(0.0), 1.0);
    assert_eq!(cos(-0.0), 1.0);
    let (s, c) = sin_cos(-0.0);
    assert!(same(s, -0.0) && c == 1.0);
    assert_eq!(atan(inf), std::f32::consts::FRAC_PI_2);
    assert_eq!(atan(-inf), -std::f32::consts::FRAC_PI_2);
    assert!(asin(1.0001).is_nan() && acos(-1.0001).is_nan() && acos(nan).is_nan());
    assert_eq!(asin(1.0), std::f32::consts::FRAC_PI_2);
    assert_eq!(acos(-1.0), std::f32::consts::PI);
    assert!(same(acos(1.0), 0.0));
    assert!(same(exp(-inf), 0.0) && exp(inf) == inf && exp(nan).is_nan() && exp(0.0) == 1.0 && exp(-0.0) == 1.0);
    assert!(exp(89.0) == inf && exp(-104.0) == 0.0);
    assert!(same(exp2(-inf), 0.0) && exp2(inf) == inf && exp2(-149.0) == f32::from_bits(1));
    assert_eq!(exp2(10.0), 1024.0);
    for f in [ln, log2] {
        assert!(f(0.0) == -inf && f(-0.0) == -inf && f(inf) == inf && f(-1.0).is_nan() && f(nan).is_nan());
        assert!(same(f(1.0), 0.0));
    }
    assert_eq!(log2(f32::from_bits(1)), -149.0);
    assert_eq!(log2(1024.0), 10.0);
    assert!(hypot(inf, nan) == inf && hypot(nan, -inf) == inf && hypot(nan, 1.0).is_nan());
    assert!(same(hypot(-0.0, 0.0), 0.0));

    // atan2 and pow: compare the special-value grid with std (C99 Annex F). Zero, infinite, NaN
    // and sign results must match bit for bit; ordinary finite results to 1 ulp (std's f32 libm
    // need not be correctly rounded).
    let specials = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0, 3.0, -3.0, 2.5, -2.5, 1e-45, 1e30, inf, -inf, nan];
    let close = |a: f32, b: f32| same(a, b) || (a.is_normal() && b.is_normal() && ulps32(a, b) <= 1);
    for &y in &specials {
        for &x in &specials {
            let (ours, theirs) = (atan2(y, x), y.atan2(x));
            assert!(close(ours, theirs), "atan2({y}, {x}) = {ours} want {theirs}");
            let (ours, theirs) = (pow(x, y), x.powf(y));
            assert!(close(ours, theirs), "pow({x}, {y}) = {ours} want {theirs}");
        }
    }
    assert!(pow(-8.0, 1.0 / 3.0).is_nan());
    assert!(hypot(0.0, -0.0) == 0.0 && hypot(f32::MAX, f32::MAX) == inf);
    assert_eq!(pow(-2.0, 1e10), inf); // huge even exponent
}

#[test]
fn identities() {
    let mut rng = Rng(61);
    for _ in 0..N / 4 {
        let x = rng.log_signed(-6.0, 7.0);
        let xf = x as f32;
        // Exact symmetry.
        assert_eq!(sin_f64(-x).to_bits(), (-sin_f64(x)).to_bits());
        assert_eq!(cos_f64(-x).to_bits(), cos_f64(x).to_bits());
        assert_eq!(tan(-xf).to_bits(), (-tan(xf)).to_bits());
        assert_eq!(atan(-xf).to_bits(), (-atan(xf)).to_bits());
        let (s, c) = sin_cos_f64(x);
        assert!((s * s + c * c - 1.0).abs() < 4.0 * f64::EPSILON);
        assert_eq!((s.to_bits(), c.to_bits()), (sin_f64(x).to_bits(), cos_f64(x).to_bits()));
        let a = x.abs();
        assert!(ulps64(exp_f64(ln_f64(a)), a) <= 1024, "exp(ln({a}))"); // ln's error ×|ln a|
        assert!(ulps64(exp2_f64(log2_f64(a)), a) <= 1024, "exp2(log2({a}))");
        let t = rng.range(-1.5, 1.5);
        assert!((atan_f64(tan_f64(t)) - t).abs() <= 4.0 * f64::EPSILON);
        let u = rng.range(-1.0, 1.0);
        assert!((asin_f64(u) + acos_f64(u) - std::f64::consts::FRAC_PI_2).abs() <= 4.0 * f64::EPSILON);
    }
    for k in -1074..1024 {
        assert_eq!(log2_f64(pow2_any(k)), k as f64);
        assert_eq!(exp2_f64(k as f64), pow2_any(k));
    }
}

fn pow2_any(k: i32) -> f64 {
    if k < -1022 {
        pow2(-1022) * pow2(k + 1022)
    } else {
        pow2(k)
    }
}

/// Golden outputs. These bit patterns are what replays and lockstep peers rely on: they must
/// **never change** without bumping `dmath::VERSION` (and noting it in DETERMINISM.md).
#[test]
fn golden_bits_f32() {
    const INPUTS: [f32; 8] = [0.5, -1.25, 3.0, 10.0, 100.0, 1.0e-3, 12345.678, -7.0e6];
    #[rustfmt::skip]
    const GOLDEN: [(&str, [u32; 8]); 15] = [
        ("sin", [0x3EF57744, 0xBF72F0A8, 0x3E1081C3, 0xBF0B44F8, 0xBF01A12E, 0x3A83126E, 0xBF344B08, 0x3F189A75]),
        ("cos", [0x3F60A940, 0x3EA171EF, 0xBF7D7026, 0xBF56CD64, 0x3F5CC0EE, 0x3F7FFFF8, 0x3F35BE20, 0xBF4D8B31]),
        ("sin_cos", [0x3FADB271, 0xBF2237B0, 0xBF594FB5, 0xBFB1092E, 0x3EB63F80, 0x3F8020C1, 0x3BB98C00, 0xBE53C2F0]),
        ("tan", [0x3F0BDA7B, 0xC0409CCA, 0xBE11F7B9, 0x3F25FAFA, 0xBF1653A7, 0x3A831272, 0xBF7DF549, 0xBF3E1062]),
        ("atan", [0x3EED6338, 0xBF6563E3, 0x3F9FE0BB, 0x3FBC4DE9, 0x3FC7C82F, 0x3A83126C, 0x3FC90D33, 0xBFC90FD9]),
        ("atan2", [0x40236E05, 0xC0071E29, 0x3FE86B51, 0x3FD2A4DD, 0x3FCA059C, 0x4048FA02, 0x3FC911D8, 0xBFC90FDC]),
        ("asin", [0x3356BF95, 0xB40637BD, 0x34A10FB0, 0x358637BD, 0x3727C5AC, 0x2EDBE6FF, 0x3AA1D13B, 0xBF468073]),
        ("acos", [0x3FC90FDA, 0x3FC90FDC, 0x3FC90FD8, 0x3FC90FD2, 0x3FC90F87, 0x3FC90FDB, 0x3FC8E766, 0x4016280A]),
        ("exp", [0x3F80A440, 0x3F7CD1E6, 0x3F83E5EF, 0x3F8D763E, 0x402DF854, 0x3F800054, 0x7F800000, 0x00000000]),
        ("exp2", [0x3F80E3EB, 0x3F7B9A21, 0x3F856F85, 0x3F93088C, 0x40800000, 0x3F800074, 0x7F800000, 0x00000000]),
        ("ln", [0xBF317218, 0x3E647FBE, 0x3F8C9F54, 0x40135D8E, 0x40935D8E, 0xC0DD0C55, 0x4116BCAB, 0x417C2EC7]),
        ("log2", [0xBF800000, 0x3EA4D3C2, 0x3FCAE00D, 0x40549A78, 0x40D49A78, 0xC11F73DA, 0x415977AE, 0x41B5E951]),
        ("pow", [0x3FA56BD1, 0x3F6BB67D, 0x3F2A7E10, 0x3EDA68A1, 0x3E3A565D, 0x414E1EB4, 0x3CFAEAA6, 0x3B4037C2]),
        ("hypot", [0x4042A5FE, 0x40500000, 0x4087C3B6, 0x41270B7F, 0x42C81709, 0x40400001, 0x4640E6B6, 0x4AD59F80]),
        ("round", [0x3FC66666, 0xC0180000, 0x40533334, 0x41300000, 0x42DC0000, 0x3F80240B, 0x465430FC, 0xCAEAFC40]),
    ];
    let fns: [Named32; 15] = [
        ("sin", sin),
        ("cos", cos),
        ("sin_cos", |x| sin_cos(x).0 + sin_cos(x).1),
        ("tan", tan),
        ("atan", atan),
        ("atan2", |x| atan2(x, -0.75)),
        ("asin", |x| asin(x / 1.0e7)),
        ("acos", |x| acos(x / 1.0e7)),
        ("exp", |x| exp(x / 100.0)),
        ("exp2", |x| exp2(x / 50.0)),
        ("ln", |x| ln(x.abs())),
        ("log2", |x| log2(x.abs())),
        ("pow", |x| pow(x.abs(), -0.37)),
        ("hypot", |x| hypot(x, 3.0)),
        ("round", |x| round(x * 0.7) + floor(x * 0.3) + ceil(x * 0.1) + fract(x * 1.1)),
    ];
    let mut missing = Vec::new();
    for (name, f) in fns {
        let got: Vec<u32> = INPUTS.iter().map(|&x| f(x).to_bits()).collect();
        match GOLDEN.iter().find(|(n, _)| *n == name) {
            Some((_, want)) => {
                assert_eq!(&got[..], &want[..], "{name}: golden bits changed — bump dmath::VERSION if intended")
            }
            None => missing.push(format!(
                "(\"{name}\", [{}]),",
                got.iter().map(|b| format!("0x{b:08X}")).collect::<Vec<_>>().join(", ")
            )),
        }
    }
    assert!(missing.is_empty(), "add to GOLDEN:\n{}", missing.join("\n"));
}

/// FNV-1a over the output bits of each f64 function on 4096 inputs built with basic operations
/// only (so the inputs themselves are platform-independent). Same rule as `golden_bits_f32`.
#[test]
fn golden_hash_f64() {
    #[rustfmt::skip]
    const GOLDEN: [(&str, u64); 13] = [
        ("sin", 0xB46DFB3810D5ED8E),
        ("cos", 0xF83001175D66CAE8),
        ("tan", 0x0E240D97A91E92A8),
        ("atan", 0xEA9AE23CD870F8AC),
        ("atan2", 0x5EC76EF633586C86),
        ("asin", 0x4D3CA054A73AB6FE),
        ("acos", 0x6FCD7BF3E5C7C614),
        ("exp", 0x056F0A27C6105B5F),
        ("exp2", 0x2C671BD77F5D18EB),
        ("ln", 0xA21B94F8C89EAB55),
        ("log2", 0x5874D2CEC9764BB9),
        ("hypot", 0x983B86DD558784FA),
        ("huge_sin", 0x9459B18B414D40D4),
    ];
    let fns: [Named64; 13] = [
        ("sin", sin_f64),
        ("cos", cos_f64),
        ("tan", tan_f64),
        ("atan", atan_f64),
        ("atan2", |x| atan2_f64(x, 7.5)),
        ("asin", |x| asin_f64(x / 2.0e6)),
        ("acos", |x| acos_f64(x / 2.0e6)),
        ("exp", |x| exp_f64(x / 1500.0)),
        ("exp2", |x| exp2_f64(x / 1000.0)),
        ("ln", |x| ln_f64(x.abs())),
        ("log2", |x| log2_f64(x.abs())),
        ("hypot", |x| hypot_f64(x, 1.0e3)),
        ("huge_sin", |x| sin_f64(x * 1.0e300)),
    ];
    let mut missing = Vec::new();
    for (name, f) in fns {
        let mut rng = Rng(0x5EED);
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for _ in 0..4096 {
            let x = rng.range(-1.0e6, 1.0e6) * rng.unit() * rng.unit();
            for byte in f(x).to_bits().to_le_bytes() {
                h = (h ^ byte as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
        match GOLDEN.iter().find(|(n, _)| *n == name) {
            Some(&(_, want)) => assert_eq!(h, want, "{name}: golden hash changed — bump dmath::VERSION if intended"),
            None => missing.push(format!("(\"{name}\", 0x{h:016X}),")),
        }
    }
    assert!(missing.is_empty(), "add to GOLDEN:\n{}", missing.join("\n"));
}

/// Exhaustive scan of every f32 input of the one-argument functions (about 3 minutes on 32
/// threads, release). For each input it checks that the public function equals the accurate
/// double-double path rounded with `narrow` (so the fast path never decided a case it couldn't),
/// and writes every input whose exact value lies within 2⁻²⁴ f32 ulp of a midpoint to
/// `$DMATH_HARD` (default `target/dmath-hard.txt`; `$DMATH_ONLY=name` scans one function) as `name input_bits result_bits`. Those are the
/// only inputs where a double-double error far below 2⁻⁴⁸ could matter; check them against an
/// independent high-precision oracle with `python scripts/dmath_oracle.py target/dmath-hard.txt`.
/// Run with
/// `cargo test -p specula-core --release dmath::tests::exhaustive -- --ignored --nocapture`.
#[test]
#[ignore]
fn exhaustive() {
    type Case = (&'static str, fn(f32) -> f32, fn(f64) -> Dd, fn(f32) -> bool);
    let all = |x: f32| x.is_finite();
    let cases: [Case; 10] = [
        ("sin", sin, |x| sin_cos_dd(x).0, all),
        ("cos", cos, |x| sin_cos_dd(x).1, all),
        ("tan", tan, tan_dd, all),
        ("atan", atan, atan_dd::<true>, all),
        ("asin", asin, asin_dd::<true>, |x| x.abs() <= 1.0),
        ("acos", acos, acos_dd::<true>, |x| x.abs() <= 1.0),
        ("exp", exp, |x| exp_dd((x, 0.0)), |x| (-104.0..89.0).contains(&x)),
        ("exp2", exp2, exp2_dd, |x| (-150.0..128.0).contains(&x)),
        ("ln", ln, ln_dd, |x| x > 0.0 && x.is_finite()),
        ("log2", log2, log2_dd, |x| x > 0.0 && x.is_finite()),
    ];
    let path = std::env::var("DMATH_HARD").unwrap_or_else(|_| "../../target/dmath-hard.txt".into());
    let mut out = String::new();
    let threads = std::thread::available_parallelism().map_or(8, |n| n.get());
    let mut total_mismatches = 0;
    let only = std::env::var("DMATH_ONLY").ok(); // e.g. DMATH_ONLY=tan to scan one function
    for (name, f, dd, domain) in cases {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let t = std::time::Instant::now();
        let chunk = (1u64 << 32).div_ceil(threads as u64);
        let results: Vec<(Vec<String>, u64)> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads as u64)
                .map(|i| {
                    s.spawn(move || {
                        let (mut hard, mut mismatches) = (Vec::new(), 0u64);
                        for bits in (i * chunk)..((i + 1) * chunk).min(1 << 32) {
                            let x = f32::from_bits(bits as u32);
                            if !domain(x) {
                                continue;
                            }
                            let r = f(x);
                            let (h, l) = dd(x as f64);
                            if r.to_bits() != narrow(h, l).to_bits() && !(r.is_nan() && h.is_nan()) {
                                mismatches += 1;
                                if mismatches < 5 {
                                    println!("{name}: fast/accurate mismatch at {:#010x}", bits);
                                }
                            }
                            // Distance of h + l from the nearest f32 midpoint, in f32 ulp.
                            let (h, l) = fast_two_sum(h, l);
                            let c = h as f32;
                            if !c.is_finite() || c == 0.0 && h == 0.0 {
                                continue;
                            }
                            let d = h - c as f64;
                            let next = next_f32(c, if d == 0.0 { 1.0 } else { d });
                            let mid = (c as f64 + next as f64) * 0.5;
                            let dist = (((h - mid) + l) / (next as f64 - c as f64)).abs();
                            if dist < 1.0 / (1u64 << 24) as f64 {
                                hard.push(format!("{name} {:#010x} {:#010x}", bits, r.to_bits()));
                            }
                        }
                        (hard, mismatches)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mismatches: u64 = results.iter().map(|r| r.1).sum();
        let hard: Vec<String> = results.into_iter().flat_map(|r| r.0).collect();
        println!("{name:>5}: {} hard inputs, {mismatches} fast/accurate mismatches, {:.1?}", hard.len(), t.elapsed());
        total_mismatches += mismatches;
        for line in hard {
            out.push_str(&line);
            out.push('\n');
        }
    }
    // Two-argument functions can't be enumerated: random samples (with integer and half-integer
    // exponents for pow, where the exact cases live) go to the oracle instead.
    let mut rng = Rng(0xB1);
    for i in 0..20_000 {
        let x = f32::from_bits(rng.next() as u32 & 0x7fff_ffff);
        let y = match i % 4 {
            0 => (rng.range(-40.0, 40.0) as f32).round(),
            1 => (rng.range(-40.0, 40.0) as f32).round() * 0.5,
            _ => rng.range(-8.0, 8.0) as f32,
        };
        let x = if x.is_finite() && i % 2 == 0 { x } else { rng.range(0.0, 1000.0) as f32 };
        let r = pow(x, y);
        out.push_str(&format!("pow {:#010x} {:#010x} {:#010x}\n", x.to_bits(), y.to_bits(), r.to_bits()));
        let (a, b) = (rng.log_signed(-30.0, 30.0) as f32, rng.log_signed(-30.0, 30.0) as f32);
        out.push_str(&format!("atan2 {:#010x} {:#010x} {:#010x}\n", a.to_bits(), b.to_bits(), atan2(a, b).to_bits()));
        out.push_str(&format!("hypot {:#010x} {:#010x} {:#010x}\n", a.to_bits(), b.to_bits(), hypot(a, b).to_bits()));
    }
    std::fs::write(&path, out).expect("write hard-case list");
    println!("hard cases and two-argument samples written to {path}");
    assert_eq!(total_mismatches, 0, "the fast path disagreed with the accurate path");
}

/// The 17 inputs (over all 2³² per function) where rounding the f64 result to f32 rounds twice
/// and lands on the wrong side: the f64 result is exactly an f32 midpoint. Expected bits come from
/// the independent 120-digit oracle (`scripts/dmath_oracle.py --print`), not from this code.
#[test]
fn double_rounding_cases_are_correctly_rounded() {
    #[rustfmt::skip]
    const CASES: [(&str, u32, u32); 17] = [
        ("sin", 0x46199998, 0xbeb1fa5d), ("sin", 0xc6199998, 0x3eb1fa5d),
        ("cos", 0x5f18b878, 0x3f7f14bb), ("cos", 0x6115cb11, 0x3f78142f),
        ("cos", 0xdf18b878, 0x3f7f14bb), ("cos", 0xe115cb11, 0x3f78142f),
        ("atan", 0x3d8d6b23, 0x3d8d31c3), ("atan", 0xbd8d6b23, 0xbd8d31c3),
        ("acos", 0x328885a3, 0x3fc90fdb), ("acos", 0x39826222, 0x3fc907b5),
        ("exp2", 0x3b429d37, 0x3f804385), ("exp2", 0xbcf3a937, 0x3f7ac6b1),
        ("ln", 0x3c413d3a, 0xc08e158f), ("ln", 0x41178feb, 0x400fe5e7),
        ("ln", 0x4c5d65a5, 0x418f034b), ("ln", 0x65d890d3, 0x4254d1f9),
        ("ln", 0x6f31a8ec, 0x42845a89),
    ];
    for (name, x, want) in CASES {
        let x = f32::from_bits(x);
        let (got, naive) = match name {
            "sin" => (sin(x), sin_f64(x as f64) as f32),
            "cos" => (cos(x), cos_f64(x as f64) as f32),
            "atan" => (atan(x), atan_f64(x as f64) as f32),
            "acos" => (acos(x), acos_f64(x as f64) as f32),
            "exp2" => (exp2(x), exp2_f64(x as f64) as f32),
            _ => (ln(x), ln_f64(x as f64) as f32),
        };
        assert_eq!(got.to_bits(), want, "{name}({x:e})");
        assert_ne!(naive.to_bits(), want, "{name}({x:e}) is no longer a double-rounding case");
    }
    let (s, c) = sin_cos(f32::from_bits(0x46199998));
    assert_eq!(s.to_bits(), 0xbeb1fa5d);
    assert_eq!(c.to_bits(), cos(f32::from_bits(0x46199998)).to_bits());
}

/// Rough speed against std. Run with
/// `cargo test -p specula-core --release dmath::tests::bench -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench() {
    use std::hint::black_box;
    use std::time::Instant;
    let xs: Vec<f32> = (0..1_000_000).map(|i| (i as f32) * 1.0e-5 - 5.0).collect();
    let time = |f: &dyn Fn(f32) -> f32| {
        let t = Instant::now();
        let mut acc = 0.0f32;
        for &x in &xs {
            acc += f(black_box(x));
        }
        black_box(acc);
        t.elapsed().as_secs_f64() * 1e9 / xs.len() as f64
    };
    type F = dyn Fn(f32) -> f32;
    let pairs: [(&str, &F, &F); 12] = [
        ("sin", &sin, &f32::sin),
        ("cos", &cos, &f32::cos),
        ("tan", &tan, &f32::tan),
        ("atan", &atan, &f32::atan),
        ("atan2", &|x| atan2(x, 0.7), &|x| x.atan2(0.7)),
        ("asin", &|x| asin(x * 0.19), &|x| (x * 0.19).asin()),
        ("exp", &exp, &f32::exp),
        ("ln", &|x| ln(x + 5.5), &|x| (x + 5.5).ln()),
        ("pow", &|x| pow(x + 5.5, 1.7), &|x| (x + 5.5).powf(1.7)),
        ("hypot", &|x| hypot(x, 0.7), &|x| x.hypot(0.7)),
        // The cost of correct rounding: against the f64 result simply cast (double rounding).
        ("sin/f64", &sin, &|x| sin_f64(x as f64) as f32),
        ("atan/f64", &atan, &|x| atan_f64(x as f64) as f32),
    ];
    for (name, ours, theirs) in pairs {
        let (a, b) = (time(ours), time(theirs));
        println!("{name:>6}: dmath {a:5.1} ns, std {b:5.1} ns ({:.1}x)", a / b);
    }
}
