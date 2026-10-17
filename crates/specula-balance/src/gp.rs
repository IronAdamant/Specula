//! A small Gaussian process for Bayesian optimisation: zero-dependency, deterministic.
//!
//! Squared-exponential kernel on inputs normalised to `[0, 1]^d`, observations standardised,
//! a small fixed noise term. The length scale isn't fitted by gradient steps or random restarts
//! (where GP libraries reach for platform maths and luck): [`Gp::fit_best`] tries a **fixed
//! grid** of length scales and keeps the one with the highest marginal likelihood, so the model
//! adapts to how bumpy the objective is and stays a pure function of the data. Prediction by
//! Cholesky; expected improvement for minimisation. Only `+ - * /`, `sqrt` and
//! `specula_core::dmath`, in a fixed order, so the same data give the same bits everywhere.

// Triangular solves read clearest with indices.
#![allow(clippy::needless_range_loop)]

use specula_core::dmath;

/// Length scales tried, in normalised units (from 3 % to half of each param's range).
pub const LENGTH_SCALES: [f64; 6] = [0.03, 0.06, 0.12, 0.25, 0.5, 1.0];
/// Observation noise, in standardised units (seeds average out most of it).
pub const NOISE: f64 = 1e-3;

fn kernel(a: &[f64], b: &[f64], ls: f64) -> f64 {
    let d2: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
    dmath::exp_f64(-0.5 * d2 / (ls * ls))
}

/// A fitted process.
pub struct Gp {
    /// The kernel's length scale.
    pub length_scale: f64,
    /// Log marginal likelihood of the standardised data.
    pub log_likelihood: f64,
    xs: Vec<Vec<f64>>,
    /// Lower-triangular Cholesky factor of K + noise·I, row-major.
    l: Vec<f64>,
    alpha: Vec<f64>,
    mean: f64,
    scale: f64,
}

impl Gp {
    /// Fits `ys` at `xs` (at least one point). `None` if the matrix isn't positive definite
    /// (it always is with the noise term, but never panic on data).
    pub fn fit(xs: &[Vec<f64>], ys: &[f64], ls: f64) -> Option<Gp> {
        let n = xs.len();
        if n == 0 || n != ys.len() {
            return None;
        }
        let mean = ys.iter().sum::<f64>() / n as f64;
        let var = ys.iter().map(|y| (y - mean) * (y - mean)).sum::<f64>() / n as f64;
        let scale = if var > 1e-18 { var.sqrt() } else { 1.0 };
        let z: Vec<f64> = ys.iter().map(|y| (y - mean) / scale).collect();
        let mut l = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..=i {
                let mut s = kernel(&xs[i], &xs[j], ls) + if i == j { NOISE } else { 0.0 };
                for k in 0..j {
                    s -= l[i * n + k] * l[j * n + k];
                }
                if i == j {
                    if s <= 0.0 {
                        return None;
                    }
                    l[i * n + i] = s.sqrt();
                } else {
                    l[i * n + j] = s / l[j * n + j];
                }
            }
        }
        // alpha = K⁻¹ z: forward then back substitution.
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut s = z[i];
            for k in 0..i {
                s -= l[i * n + k] * y[k];
            }
            y[i] = s / l[i * n + i];
        }
        let mut alpha = vec![0.0; n];
        for i in (0..n).rev() {
            let mut s = y[i];
            for k in i + 1..n {
                s -= l[k * n + i] * alpha[k];
            }
            alpha[i] = s / l[i * n + i];
        }
        let fit: f64 = z.iter().zip(&alpha).map(|(a, b)| a * b).sum();
        let logdet: f64 = (0..n).map(|i| dmath::ln_f64(l[i * n + i])).sum();
        let log_likelihood = -0.5 * fit - logdet - 0.5 * n as f64 * dmath::ln_f64(2.0 * std::f64::consts::PI);
        Some(Gp { length_scale: ls, log_likelihood, xs: xs.to_vec(), l, alpha, mean, scale })
    }

    /// [`Gp::fit`] at every length scale in [`LENGTH_SCALES`]; the most likely one (ties to the
    /// first).
    pub fn fit_best(xs: &[Vec<f64>], ys: &[f64]) -> Option<Gp> {
        let mut best: Option<Gp> = None;
        for ls in LENGTH_SCALES {
            if let Some(g) = Gp::fit(xs, ys, ls) {
                if best.as_ref().is_none_or(|b| g.log_likelihood > b.log_likelihood) {
                    best = Some(g);
                }
            }
        }
        best
    }

    /// Predicted mean and standard deviation at `x` (in the observations' units).
    pub fn predict(&self, x: &[f64]) -> (f64, f64) {
        let n = self.xs.len();
        let k: Vec<f64> = self.xs.iter().map(|xi| kernel(xi, x, self.length_scale)).collect();
        let mu: f64 = k.iter().zip(&self.alpha).map(|(a, b)| a * b).sum();
        // v = L⁻¹ k
        let mut v = vec![0.0; n];
        for i in 0..n {
            let mut s = k[i];
            for j in 0..i {
                s -= self.l[i * n + j] * v[j];
            }
            v[i] = s / self.l[i * n + i];
        }
        let var = (1.0 - v.iter().map(|x| x * x).sum::<f64>()).max(1e-12);
        (self.mean + mu * self.scale, var.sqrt() * self.scale)
    }
}

/// Standard normal density.
pub fn pdf(z: f64) -> f64 {
    dmath::exp_f64(-0.5 * z * z) / (2.0 * std::f64::consts::PI).sqrt()
}

/// Standard normal distribution (via a complementary error function good to ~1e-7).
pub fn cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

/// Abramowitz & Stegun 7.1.26 with the symmetric extension.
fn erfc(x: f64) -> f64 {
    let a = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * a);
    let poly = t * (0.254_829_592 + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let e = poly * dmath::exp_f64(-a * a);
    if x >= 0.0 {
        e
    } else {
        2.0 - e
    }
}

/// Expected improvement below `best` for a prediction `(mu, sigma)` (minimising).
pub fn expected_improvement(mu: f64, sigma: f64, best: f64) -> f64 {
    if sigma <= 1e-12 {
        return (best - mu).max(0.0);
    }
    let z = (best - mu) / sigma;
    (best - mu) * cdf(z) + sigma * pdf(z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_interpolates_and_is_unsure_far_away() {
        let xs: Vec<Vec<f64>> = [0.0, 0.3, 0.6, 1.0].iter().map(|x| vec![*x]).collect();
        let ys: Vec<f64> = xs.iter().map(|x| (x[0] - 0.4) * (x[0] - 0.4)).collect();
        let gp = Gp::fit(&xs, &ys, 0.25).unwrap();
        assert!(Gp::fit_best(&xs, &ys).is_some());
        for (x, y) in xs.iter().zip(&ys) {
            let (m, s) = gp.predict(x);
            assert!((m - y).abs() < 0.01 && s < 0.05, "at {x:?}: {m} ± {s} vs {y}");
        }
        let (_, near) = gp.predict(&[0.31]);
        let (_, far) = gp.predict(&[0.45]);
        assert!(far > near);
        // EI is highest where a low value is likely.
        let best = ys.iter().copied().fold(f64::MAX, f64::min);
        let ei = |x: f64| {
            let (m, s) = gp.predict(&[x]);
            expected_improvement(m, s, best)
        };
        assert!(ei(0.45) > ei(0.9));
    }

    #[test]
    fn the_normal_helpers_are_accurate_and_symmetric() {
        assert!((cdf(0.0) - 0.5).abs() < 1e-7);
        assert!((cdf(1.959_964) - 0.975).abs() < 1e-6);
        assert!((cdf(-1.0) + cdf(1.0) - 1.0).abs() < 1e-7);
        assert!((pdf(0.0) - 0.398_942_28).abs() < 1e-7);
    }
}
