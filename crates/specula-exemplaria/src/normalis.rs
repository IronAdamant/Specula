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
//! **Accuracy** (held by the tests): [`cdf`] is the series `Φ(x) = 1/2 + φ(x) Σ x^(2n+1)/(2n+1)!!`
//! (every term of one sign, so nothing cancels inside the sum), within 1e-15 of the true value
//! for |x| ≤ 5 (the draws use |x| ≤ 2.5). [`quantile`] starts from Acklam's rational
//! approximation (relative error under 1.2e-9) and takes two of Halley's steps on [`cdf`], so
//! `cdf(quantile(p))` is within 5e-16 (two units in the last place near 1) of `p` wherever the draws use it (p between Φ(-2.5) and
//! Φ(2.5)), and `quantile` within 1e-13 of the true value.

use specula_core::dmath;

/// 1 / sqrt(2π).
const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
/// sqrt(2π).
const SQRT_2PI: f64 = 2.506_628_274_631_000_5;

/// The standard normal density.
pub fn pdf(x: f64) -> f64 {
    INV_SQRT_2PI * dmath::exp_f64(-0.5 * x * x)
}

/// The standard normal distribution, `P(Z <= x)`. Beyond ±8 it is 0 or 1 (within 7e-16).
pub fn cdf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x <= -8.0 {
        return 0.0;
    }
    if x >= 8.0 {
        return 1.0;
    }
    // Σ x^(2n+1) / (2n+1)!!: term n is term n-1 times x² / (2n+1). Up to 400 terms (at |x| = 8 the
    // terms peak near n = 32 and fall below 1e-17 of the sum well before 200).
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

/// The inverse of [`cdf`]: the `x` with `cdf(x) = p`, for `p` in (0, 1). 0 and 1 give ∓8 (where
/// [`cdf`] reaches them); a `p` that is not a number gives NaN.
pub fn quantile(p: f64) -> f64 {
    if p.is_nan() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return -8.0;
    }
    if p >= 1.0 {
        return 8.0;
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

    /// Values of Φ and its inverse to 16 digits (printed tables; Φ(1) = 0.8413447460685429...).
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
        ];
        for (x, p) in cases {
            assert!((cdf(x) - p).abs() < 1e-15, "cdf({x}) = {} against {p}", cdf(x));
            if (0.006..=0.994).contains(&p) {
                assert!((quantile(p) - x).abs() < 1e-13, "quantile({p}) = {} against {x}", quantile(p));
            }
        }
    }

    /// `cdf(quantile(p))` returns `p` across the range the draws use and beyond it.
    #[test]
    fn the_inverse_inverts() {
        let mut worst = 0.0f64;
        for i in 1..20_000 {
            let p = f64::from(i) / 20_000.0;
            worst = worst.max((cdf(quantile(p)) - p).abs());
        }
        assert!(worst < 5e-16, "worst {worst:e}");
        // Monotone: a larger p never gives a smaller x.
        let mut last = -9.0;
        for i in 0..100_000 {
            let p = cdf(-2.5) + (cdf(2.5) - cdf(-2.5)) * f64::from(i) / 100_000.0;
            let x = quantile(p);
            assert!(x >= last, "{p}: {x} < {last}");
            last = x;
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
        for [a, b] in [[-2.5, 2.5], [-1.67, 1.43], [0.3, 0.31], [-2.5, -2.49]] {
            for i in 0..1000 {
                let z = truncated(f64::from(i) / 1000.0, [a, b]);
                assert!(z >= a && z <= b, "{z} outside {a}..{b}");
            }
        }
    }
}
