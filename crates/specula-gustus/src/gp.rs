//! A Gaussian process on a latent goodness `f`, fitted to **choices** (a picked point over the
//! others shown with it: Bradley-Terry-Luce, a multinomial logit) with a Laplace approximation,
//! and expected improvement from its posterior. Dense linear algebra in `f64`; `dmath` for every
//! transcendental, so fits are identical on every platform.
//!
//! Model: `f ~ GP(0, k)`, `k(x, y) = amp * exp(-|x - y|^2 / (2 l^2))`; a choice of `w` from the
//! set `S` has likelihood `exp(f_w) / sum_{j in S} exp(f_j)`. Newton's method finds the MAP of `f`
//! at the observed points; the posterior at `x` has mean `k_x^T K^-1 f` and variance
//! `k(x,x) - k_x^T (K + W^-1)^-1 k_x` with `W` the negative Hessian of the log likelihood, written
//! as `K^-1 - K^-1 (K^-1 + W)^-1 K^-1` so a singular `W` is fine.

use specula_core::dmath;

/// Most points a fit takes (more are refused by the caller; duplicates are merged before).
pub const MAX_POINTS: usize = 288;
const NEWTON_STEPS: usize = 40;
const JITTER: f64 = 1e-6;

/// One choice: `winner` (an index into the points) over each of `others`.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub winner: usize,
    pub others: Vec<usize>,
}

/// Kernel settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kernel {
    pub length: f64,
    pub amp: f64,
}

impl Kernel {
    /// The default for `dims` unit dimensions: longer in more dimensions (points are farther
    /// apart there).
    pub fn for_dims(dims: usize) -> Kernel {
        Kernel { length: 0.18 + 0.07 * (dims.max(1) as f64).sqrt(), amp: 1.0 }
    }

    pub fn k(&self, a: &[f64], b: &[f64]) -> f64 {
        let d2: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
        self.amp * dmath::exp_f64(-d2 / (2.0 * self.length * self.length))
    }
}

/// Cholesky factor `L` (lower, row-major) of the `n`×`n` matrix `a`, or `None` if not positive
/// definite.
pub fn cholesky(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                if s <= 0.0 || !s.is_finite() {
                    return None;
                }
                l[i * n + i] = s.sqrt();
            } else {
                l[i * n + j] = s / l[j * n + j];
            }
        }
    }
    Some(l)
}

/// Solves `L L^T x = b`.
pub fn chol_solve(l: &[f64], n: usize, b: &[f64]) -> Vec<f64> {
    let mut y = b.to_vec();
    for i in 0..n {
        let mut s = y[i];
        for k in 0..i {
            s -= l[i * n + k] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k * n + i] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    y
}

/// The inverse from a Cholesky factor.
fn chol_inverse(l: &[f64], n: usize) -> Vec<f64> {
    let mut inv = vec![0.0; n * n];
    let mut e = vec![0.0; n];
    for j in 0..n {
        e.iter_mut().for_each(|v| *v = 0.0);
        e[j] = 1.0;
        let col = chol_solve(l, n, &e);
        for i in 0..n {
            inv[i * n + j] = col[i];
        }
    }
    inv
}

fn mat_vec(a: &[f64], n: usize, x: &[f64]) -> Vec<f64> {
    (0..n).map(|i| (0..n).map(|j| a[i * n + j] * x[j]).sum()).collect()
}

fn mat_mul(a: &[f64], b: &[f64], n: usize) -> Vec<f64> {
    let mut c = vec![0.0; n * n];
    for i in 0..n {
        for k in 0..n {
            let aik = a[i * n + k];
            if aik == 0.0 {
                continue;
            }
            for j in 0..n {
                c[i * n + j] += aik * b[k * n + j];
            }
        }
    }
    c
}

/// log(sum(exp(v))) over `idx`, stable.
fn logsumexp(f: &[f64], idx: impl Iterator<Item = usize> + Clone) -> f64 {
    let m = idx.clone().map(|i| f[i]).fold(f64::NEG_INFINITY, f64::max);
    m + dmath::ln_f64(idx.map(|i| dmath::exp_f64(f[i] - m)).sum::<f64>())
}

/// A fitted model.
#[derive(Clone, Debug)]
pub struct Fit {
    pub points: Vec<Vec<f64>>,
    pub kernel: Kernel,
    /// The MAP latent values at the points.
    pub f: Vec<f64>,
    /// `K^-1 f`.
    alpha: Vec<f64>,
    /// `K^-1 - K^-1 (K^-1 + W)^-1 K^-1`.
    m: Vec<f64>,
    /// Newton steps taken.
    pub steps: usize,
}

/// Fits the latent `f` to `choices` over `points`. `None` if a matrix isn't positive definite
/// (it always is with the jitter; callers fall back to a spread).
pub fn fit(points: Vec<Vec<f64>>, choices: &[Choice], kernel: Kernel) -> Option<Fit> {
    let n = points.len();
    if n == 0 || n > MAX_POINTS {
        return None;
    }
    let mut k = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let v = kernel.k(&points[i], &points[j]);
            k[i * n + j] = v;
            k[j * n + i] = v;
        }
        k[i * n + i] += JITTER;
    }
    let lk = cholesky(&k, n)?;
    let kinv = chol_inverse(&lk, n);
    let sets: Vec<(usize, Vec<usize>)> = choices
        .iter()
        .filter(|c| c.winner < n && !c.others.is_empty())
        .map(|c| {
            let mut s: Vec<usize> = std::iter::once(c.winner).chain(c.others.iter().copied().filter(|o| *o < n && *o != c.winner)).collect();
            s.dedup();
            (c.winner, s)
        })
        .collect();
    let objective = |f: &[f64]| -> f64 {
        let lik: f64 = sets.iter().map(|(w, s)| f[*w] - logsumexp(f, s.iter().copied())).sum();
        let kf = mat_vec(&kinv, n, f);
        lik - 0.5 * f.iter().zip(&kf).map(|(a, b)| a * b).sum::<f64>()
    };
    // Gradient and the negative Hessian of the log likelihood.
    let grad_w = |f: &[f64]| -> (Vec<f64>, Vec<f64>) {
        let mut g = vec![0.0; n];
        let mut w = vec![0.0; n * n];
        for (win, s) in &sets {
            let lse = logsumexp(f, s.iter().copied());
            let p: Vec<f64> = s.iter().map(|i| dmath::exp_f64(f[*i] - lse)).collect();
            g[*win] += 1.0;
            for (a, i) in s.iter().enumerate() {
                g[*i] -= p[a];
                w[i * n + i] += p[a];
                for (b, j) in s.iter().enumerate() {
                    w[i * n + j] -= p[a] * p[b];
                }
            }
        }
        (g, w)
    };
    let mut f = vec![0.0; n];
    let mut psi = objective(&f);
    let mut steps = 0;
    let mut a_last = None;
    for _ in 0..NEWTON_STEPS {
        steps += 1;
        let (g, w) = grad_w(&f);
        // A = K^-1 + W; Newton: f_new = A^-1 (W f + g).
        let mut a = kinv.clone();
        for (x, y) in a.iter_mut().zip(&w) {
            *x += y;
        }
        let la = cholesky(&a, n)?;
        let wf = mat_vec(&w, n, &f);
        let rhs: Vec<f64> = wf.iter().zip(&g).map(|(x, y)| x + y).collect();
        let target = chol_solve(&la, n, &rhs);
        // Damped: halve the step until the objective doesn't fall.
        let mut t = 1.0;
        let mut accepted = false;
        for _ in 0..12 {
            let cand: Vec<f64> = f.iter().zip(&target).map(|(x, y)| x + t * (y - x)).collect();
            let pc = objective(&cand);
            if pc >= psi - 1e-12 {
                let moved = cand.iter().zip(&f).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
                f = cand;
                psi = pc;
                accepted = moved > 1e-7;
                break;
            }
            t *= 0.5;
        }
        a_last = Some(la);
        if !accepted {
            break;
        }
    }
    // The posterior's matrices at the MAP.
    let (_, w) = grad_w(&f);
    let mut a = kinv.clone();
    for (x, y) in a.iter_mut().zip(&w) {
        *x += y;
    }
    let la = cholesky(&a, n).or(a_last)?;
    let ainv = chol_inverse(&la, n);
    let t = mat_mul(&mat_mul(&kinv, &ainv, n), &kinv, n);
    let m: Vec<f64> = kinv.iter().zip(&t).map(|(x, y)| x - y).collect();
    let alpha = mat_vec(&kinv, n, &f);
    Some(Fit { points, kernel, f, alpha, m, steps })
}

impl Fit {
    /// Posterior mean and variance of `f` at `x`.
    pub fn predict(&self, x: &[f64]) -> (f64, f64) {
        let n = self.points.len();
        let k: Vec<f64> = self.points.iter().map(|p| self.kernel.k(p, x)).collect();
        let mean: f64 = k.iter().zip(&self.alpha).map(|(a, b)| a * b).sum();
        let mut quad = 0.0;
        for i in 0..n {
            let row = &self.m[i * n..(i + 1) * n];
            quad += k[i] * row.iter().zip(&k).map(|(a, b)| a * b).sum::<f64>();
        }
        (mean, (self.kernel.amp - quad).max(1e-12))
    }

    /// The best posterior mean among the observed points, and which.
    pub fn best(&self) -> (usize, f64) {
        let mut best = (0, f64::NEG_INFINITY);
        for (i, p) in self.points.iter().enumerate() {
            let (m, _) = self.predict(p);
            if m > best.1 {
                best = (i, m);
            }
        }
        best
    }

    /// Expected improvement over `best` at `x`.
    pub fn ei(&self, x: &[f64], best: f64) -> f64 {
        let (m, v) = self.predict(x);
        let s = v.sqrt();
        let z = (m - best) / s;
        (m - best) * normal_cdf(z) + s * normal_pdf(z)
    }
}

pub fn normal_pdf(z: f64) -> f64 {
    0.398_942_280_401_432_7 * dmath::exp_f64(-0.5 * z * z)
}

pub fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

/// The error function (Abramowitz and Stegun 7.1.26, |error| < 1.5e-7).
pub fn erf(x: f64) -> f64 {
    let s = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0 - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t + 0.254_829_592) * t * dmath::exp_f64(-x * x);
    s * y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_algebra() {
        let a = [4.0, 2.0, 2.0, 3.0];
        let l = cholesky(&a, 2).unwrap();
        let x = chol_solve(&l, 2, &[2.0, 1.0]);
        assert!((4.0 * x[0] + 2.0 * x[1] - 2.0).abs() < 1e-12 && (2.0 * x[0] + 3.0 * x[1] - 1.0).abs() < 1e-12);
        assert!(cholesky(&[1.0, 2.0, 2.0, 1.0], 2).is_none());
        assert!((erf(0.5) - 0.520_499_877_8).abs() < 2e-7);
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn choices_rank_the_points() {
        // Three points on a line; the middle one is always picked.
        let pts = vec![vec![0.1], vec![0.5], vec![0.9]];
        let ch = vec![Choice { winner: 1, others: vec![0, 2] }, Choice { winner: 1, others: vec![0, 2] }];
        let f = fit(pts, &ch, Kernel::for_dims(1)).unwrap();
        assert!(f.f[1] > f.f[0] && f.f[1] > f.f[2], "{:?}", f.f);
        assert_eq!(f.best().0, 1);
        // Uncertainty grows away from the data; EI is highest where improvement is plausible.
        let (_, v_near) = f.predict(&[0.5]);
        let (_, v_far) = f.predict(&[0.0]);
        assert!(v_far > v_near);
        let best = f.best().1;
        assert!(f.ei(&[0.45], best) > f.ei(&[0.95], best) * 0.5);
        // A deterministic fit.
        let again = fit(vec![vec![0.1], vec![0.5], vec![0.9]], &ch, Kernel::for_dims(1)).unwrap();
        assert_eq!(again.f, f.f);
    }
}
