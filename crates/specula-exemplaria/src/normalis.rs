//! The standard normal distribution and its inverse, for draws truncated to a range
//! (docs/design/EXEMPLARIA.md, A.2): one uniform from a stream becomes one standard normal
//! residual inside `[a, b]` by the inverse of the distribution, so nothing piles at a range's
//! ends (a clip would put every draw past an end at that end) and no draw is rejected and drawn
//! again (a loop whose length depends on the seed).
//!
//! **Determinism**: everything is IEEE arithmetic (`+ - * /`, `sqrt`) and `specula_core::dmath`
//! (`exp_f64`, `ln_f64`), with a fixed number of steps, so the same bytes come out on every
//! platform.
//!
//! **Domain and accuracy** (held by the tests, against published values):
//!
//! - [`cdf`], for every `x`: for |x| ≤ 3 the series `Φ(x) = 1/2 + φ(x) Σ x^(2n+1)/(2n+1)!!`
//!   (every term of one sign, so nothing cancels inside the sum), within 1e-15 of the true
//!   value; past 3 the tail's continued fraction (below), so that a tail is not the difference
//!   of two nearly equal numbers. For `x ≤ 0` it is within a relative 1e-13 of the true value
//!   down to -20 (held at published values), and further out within what rounding `x²/2` allows
//!   (about 1e-13 at -37, where Φ is 6e-300; past about -38.5 it is 0); for `x > 0` within 1e-15
//!   (absolute: 1 - Φ(x) is the upper tail, given relatively by `cdf(-x)`).
//! - [`quantile`], for `p` from 1e-300 up to 1 (exclusive): Acklam's rational approximation
//!   (relative error under 1.2e-9) and two of Halley's steps on [`cdf`], each of which then only
//!   improves it; above Φ(3) it is `-quantile(1 - p)` (`1 - p` is exact there), so the upper tail
//!   is the lower tail's. Within 1e-13 of the true value of the `p` given (held at published
//!   values and an independent bisection from 1e-15 to 1 - 1e-3), and `cdf(quantile(p))` within
//!   5e-16 of `p` where `x` is within 3, and past it within a relative `1e-14 + 4e-16 x²` (what a
//!   double's `x` can hold: 2e-13 at 1e-300). Under 1e-300 it is Acklam's
//!   alone (Halley's step there would need `exp(x²/2)` past what a double holds).
//! - **Monotone to within 2e-13, not to the last bit**: a `p` one unit in the last place larger
//!   can give an `x` a few units in the last place smaller (the review counted 23,291 such steps
//!   in 400,000 inside ±2.5, none larger than 2.2e-14; review 4 measured the largest anywhere,
//!   1.24e-13, at `p` 7,460 units in the last place below Φ(3), where the series' Halley steps
//!   meet the upper tail's), and [`cdf`] likewise. Nothing depends on more:
//!   [`truncated`] holds its result inside `[a, b]`, and the reach uses only the windows' ends,
//!   inside which every draw then lies.
//!
//! The draws use |x| ≤ 2.5 and `p` between Φ(-2.5) and Φ(2.5); there these are the same
//! functions, to the bit, as before the tails were added (the specimens' hashes hold it).

use specula_core::dmath;

/// 1 / sqrt(2π).
const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
/// sqrt(2π).
const SQRT_2PI: f64 = 2.506_628_274_631_000_5;
/// Past this the tails are computed by their continued fraction (the series would cancel).
const TAIL: f64 = 3.0;
/// Φ(3): above it [`quantile`] is the lower tail's, by symmetry.
const UPPER: f64 = 0.998_650_101_968_369_9;
/// Under this [`quantile`] takes no step of Halley's (see the module's notes).
const LEAST_P: f64 = 1e-300;
/// The levels of the tail's continued fraction, evaluated from the deepest back: at 3 it has
/// settled to the last bit well before (further out, sooner).
const TAIL_LEVELS: u32 = 300;

/// The standard normal density.
pub fn pdf(x: f64) -> f64 {
    INV_SQRT_2PI * dmath::exp_f64(-0.5 * x * x)
}

/// The upper tail `Q(t) = 1 - Φ(t)` for `t > 3`, by Laplace's continued fraction
/// `Q(t) = φ(t) / (t + 1/(t + 2/(t + 3/(t + ...))))` (every level positive: nothing cancels).
fn upper_tail(t: f64) -> f64 {
    let mut f = 0.0;
    for k in (1..=TAIL_LEVELS).rev() {
        f = f64::from(k) / (t + f);
    }
    pdf(t) / (t + f)
}

/// The standard normal distribution, `P(Z <= x)`.
pub fn cdf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < -TAIL {
        return upper_tail(-x);
    }
    if x > TAIL {
        return 1.0 - upper_tail(x);
    }
    // Σ x^(2n+1) / (2n+1)!!: term n is term n-1 times x² / (2n+1). Up to 400 terms (at |x| = 3 the
    // terms peak near n = 4 and fall below 1e-17 of the sum before 60).
    let x2 = x * x;
    let mut term = x;
    let mut sum = x;
    let mut n = 1.0;
    for _ in 0..400 {
        term *= x2 / (2.0 * n + 1.0);
        sum += term;
        if term.abs() <= 1e-17 * sum.abs() {
            break;
        }
        n += 1.0;
    }
    (0.5 + pdf(x) * sum).clamp(0.0, 1.0)
}

/// The inverse of [`cdf`]: the `x` with `cdf(x) = p`, for `p` in (0, 1). 0 and 1 give -∞ and +∞;
/// a `p` that is not a number gives NaN. See the module's notes for its domain and accuracy.
pub fn quantile(p: f64) -> f64 {
    if p.is_nan() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    if p > UPPER {
        // (1 - p is exact for p above a half.)
        return -quantile(1.0 - p);
    }
    // Acklam's approximation (P. J. Acklam, "An algorithm for computing the inverse normal
    // cumulative distribution function", 2003): a rational function in the middle, another in
    // sqrt(-2 ln p) in each tail.
    const A: [f64; 6] = [-3.969_683_028_665_376e1, 2.209_460_984_245_205e2, -2.759_285_104_469_687e2, 1.383_577_518_672_69e2, -3.066_479_806_614_716e1, 2.506_628_277_459_239];
    const B: [f64; 5] = [-5.447_609_879_822_406e1, 1.615_858_368_580_409e2, -1.556_989_798_598_866e2, 6.680_131_188_771_972e1, -1.328_068_155_288_572e1];
    const C: [f64; 6] = [-7.784_894_002_430_293e-3, -3.223_964_580_411_365e-1, -2.400_758_277_161_838, -2.549_732_539_343_734, 4.374_664_141_464_968, 2.938_163_982_698_783];
    const D: [f64; 4] = [7.784_695_709_041_462e-3, 3.224_671_290_700_398e-1, 2.445_134_137_142_996, 3.754_408_661_907_416];
    const LOW: f64 = 0.024_25;
    let tail = |q: f64| (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    let mut x = if p < LOW {
        tail((-2.0 * dmath::ln_f64(p)).sqrt())
    } else if p > 1.0 - LOW {
        -tail((-2.0 * dmath::ln_f64(1.0 - p)).sqrt())
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    };
    if p < LEAST_P {
        return x;
    }
    // Two of Halley's steps on the accurate distribution (each about triples the digits).
    for _ in 0..2 {
        let e = cdf(x) - p;
        let u = e * SQRT_2PI * dmath::exp_f64(0.5 * x * x);
        x -= u / (1.0 + 0.5 * x * u);
    }
    x
}

/// A standard normal truncated to `[a, b]`, from a uniform `u` in [0, 1): the inverse of the
/// distribution at `cdf(a) + u (cdf(b) - cdf(a))`, held inside `[a, b]` (the last bit of a
/// rounding can step past an end; nothing else does). `a == b` gives `a`; an empty interval
/// (`a > b`) gives `a`, and an end that is not a number gives NaN.
pub fn truncated(u: f64, [a, b]: [f64; 2]) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a >= b {
        return a;
    }
    let (pa, pb) = (cdf(a), cdf(b));
    quantile(pa + u * (pb - pa)).clamp(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values of Φ to 16 digits (printed tables; Φ(1) = 0.8413447460685429...), in the middle
    /// within 1e-15, in the lower tail within a relative 1e-13.
    #[test]
    fn known_values() {
        let cases = [
            (0.0, 0.5),
            (1.0, 0.841_344_746_068_542_9),
            (-1.0, 0.158_655_253_931_457_05),
            (2.0, 0.977_249_868_051_820_8),
            (2.5, 0.993_790_334_674_223_7),
            (-2.5, 0.006_209_665_325_776_132),
            (1.959_963_984_540_054, 0.975),
            (0.5, 0.691_462_461_274_013_1),
            (4.0, 0.999_968_328_758_166_9),
            (5.0, 0.999_999_713_348_428_1),
        ];
        for (x, p) in cases {
            assert!((cdf(x) - p).abs() < 1e-15, "cdf({x}) = {} against {p}", cdf(x));
            if (0.006..=0.994).contains(&p) {
                assert!((quantile(p) - x).abs() < 1e-13, "quantile({p}) = {} against {x}", quantile(p));
            }
        }
        // The lower tail, where the series would cancel (review 3: cdf(-7) was 9e-5 off, cdf(-8) 0).
        let tail = [
            (-3.0, 1.349_898_031_630_094_6e-3),
            (-3.5, 2.326_290_790_355_250_4e-4),
            (-4.0, 3.167_124_183_311_998e-5),
            (-5.0, 2.866_515_718_791_939e-7),
            (-6.0, 9.865_876_450_376_982e-10),
            (-7.0, 1.279_812_543_885_835e-12),
            (-8.0, 6.220_960_574_271_784e-16),
            (-10.0, 7.619_853_024_160_527e-24),
            (-20.0, 2.753_624_118_606_233_7e-89),
        ];
        for (x, p) in tail {
            assert!(((cdf(x) - p) / p).abs() < 1e-13, "cdf({x}) = {:e} against {p:e}", cdf(x));
            assert!((quantile(p) - x).abs() < 1e-13, "quantile({p:e}) = {} against {x}", quantile(p));
        }
        // The quantile's values (review 3: quantile(1e-12) was 3.3e-5 off, 1e-15 0.051). Each is
        // the review's bisection of its own Φ in std maths; for 1e-12 that bisection, not the
        // review's printed -7.03448382545049, which is 1.5e-10 from it.
        for (p, x) in [(1e-15, -7.941_345_326_170_997), (1e-12, -7.034_483_825_301_132), (1e-10, -6.361_340_902_404_056), (1e-8, -5.612_001_244_174_789), (1e-6, -4.753_424_308_822_899), (1e-3, -3.090_232_306_167_813_6)] {
            assert!((quantile(p) - x).abs() < 1e-13, "quantile({p:e}) = {} against {x}", quantile(p));
        }
        assert_eq!((quantile(0.0), quantile(1.0)), (f64::NEG_INFINITY, f64::INFINITY));
        assert!(quantile(f64::NAN).is_nan() && cdf(f64::NAN).is_nan());
        assert_eq!((cdf(f64::NEG_INFINITY), cdf(f64::INFINITY)), (0.0, 1.0));
    }

    /// `cdf(quantile(p))` returns `p`: within 5e-16 across the range the draws use and beyond it,
    /// and past 3 (from Φ(-3) down to 1e-300) within a relative 1e-14 + 4e-16 x² (what a double's
    /// `x` allows: 2e-13 at 1e-300); the upper tail is the lower's.
    #[test]
    fn the_inverse_inverts() {
        let mut worst = 0.0f64;
        for i in 1..20_000 {
            let p = f64::from(i) / 20_000.0;
            worst = worst.max((cdf(quantile(p)) - p).abs());
        }
        assert!(worst < 5e-16, "worst {worst:e}");
        let (mut worst, mut at) = (0.0f64, 0.0);
        for i in 0..=3_000 {
            // From 1e-300 to a half, evenly in the exponent.
            let p = dmath::exp_f64(dmath::ln_f64(1e-300) * (1.0 - f64::from(i) / 3_000.0) + dmath::ln_f64(0.5) * f64::from(i) / 3_000.0);
            let x = quantile(p);
            // Past 3, relatively (a unit in the last place of a double's x moves Φ by x² 1.1e-16);
            // nearer the middle the series' 5e-16, absolutely.
            let allowed = if x < -TAIL { p * (1e-14 + 4e-16 * x * x) } else { 5e-16 };
            let r = (cdf(x) - p).abs() / allowed;
            if r > worst {
                (worst, at) = (r, p);
            }
            // (1 - q is exact for these q, each under Φ(-3).)
            let q = f64::from(i + 1) / 4_194_304.0;
            assert_eq!(quantile(1.0 - q), -quantile(q));
        }
        assert!(worst < 1.0, "{worst} of its allowance at {at:e}: {:e}", cdf(quantile(at)) - at);
        // Under 1e-300: Acklam's start alone, within its 1.2e-9.
        // (Acklam's own figure there, -37.6631, to its 1.2e-9: Φ itself is past what a double holds.)
        let x = quantile(1e-310);
        assert!((x + 37.663_06).abs() < 1e-4 && x < quantile(1e-300), "{x}");
    }

    /// Monotone to within 2e-13, not to the last bit (review 3 counted 23,291 steps back in
    /// 400,000 one-unit steps of `p` inside ±2.5; review 4 measured 1.24e-13 just below Φ(3), a
    /// region the walk from 10,000 units below the join now crosses): a step back is never larger
    /// than that, and the
    /// truncated draw does not depend on more: it stays inside its window at every unit step of
    /// `u` next to the window's ends.
    #[test]
    fn monotone_to_within_its_accuracy_and_the_draw_does_not_need_more() {
        let (mut back, mut most) = (0u32, 0.0f64);
        for start in [cdf(-2.5), 0.024_25, 0.3, 0.5, 0.9, 1.0 - 0.024_25, cdf(2.4), f64::from_bits(UPPER.to_bits() - 10_000)] {
            let mut p = start;
            let mut last = quantile(p);
            for _ in 0..20_000 {
                p = f64::from_bits(p.to_bits() + 1);
                let x = quantile(p);
                if x < last {
                    back += 1;
                    most = most.max(last - x);
                }
                last = x;
            }
        }
        assert!(most < 2e-13, "{back} steps back, the largest {most:e}");
        // (The uniforms a stream gives next to 0 and 1: k / 2^53.)
        let step = 1.0 / 9_007_199_254_740_992.0;
        for w in [[-2.5, 2.5], [-1.67, 1.43], [0.3, 0.31], [-2.5, -2.49], [2.0, 2.5]] {
            for k in 0..1_000u32 {
                for u in [f64::from(k) * step, 1.0 - f64::from(k + 1) * step] {
                    let x = truncated(u, w);
                    assert!(x >= w[0] && x <= w[1], "{x} outside {w:?} at {u}");
                }
            }
        }
    }

    #[test]
    fn a_truncated_draw_stays_inside_and_fills_its_interval() {
        assert_eq!(truncated(0.0, [-2.5, 2.5]), -2.5);
        assert!(truncated(0.999_999_999, [-2.5, 2.5]) <= 2.5);
        assert_eq!(truncated(0.3, [1.0, 1.0]), 1.0);
        assert_eq!(truncated(0.3, [1.0, 0.5]), 1.0);
        assert!(truncated(0.3, [f64::NAN, 1.0]).is_nan());
        assert!((truncated(0.5, [-2.5, 2.5])).abs() < 1e-15);
        for [a, b] in [[-2.5, 2.5], [-1.67, 1.43], [0.3, 0.31], [-2.5, -2.49], [-10.0, -9.0]] {
            for i in 0..1000 {
                let z = truncated(f64::from(i) / 1000.0, [a, b]);
                assert!(z >= a && z <= b, "{z} outside {a}..{b}");
            }
        }
        // A window in the far tail is drawn in, not piled at its end (it was, past -8).
        assert!((truncated(0.5, [-10.0, -9.0]) + 9.1).abs() < 0.05, "{}", truncated(0.5, [-10.0, -9.0]));
    }
}
