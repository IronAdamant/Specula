//! The inverse MDCT Vorbis uses (no normalisation):
//!
//! `y[n] = Σ_k X[k] · cos(2π/N · (n + 1/2 + N/4) · (k + 1/2))`, `n < N`, `k < N/2`.
//!
//! Computed through a DCT-IV of size `M = N/2`, itself an `N/4`-point complex FFT between two
//! twiddle passes: with `c_j = X[2j] − i·X[M−1−2j]`, `W_p = e^{iπ(p+1/8)/M} · Σ_j c_j ·
//! e^{iπ(j+1/8)/M} · e^{2πi·pj/(M/2)}` gives `u[2p] = Re W_p` and `u[M−1−2p] = Im W_p`, and the
//! IMDCT output is `u` unfolded with the DCT-IV's symmetries (`u[2M−1−m] = −u[m]`,
//! `u[m+2M] = −u[m]`).

use specula_core::dmath;

pub(crate) struct Imdct {
    n: usize,
    /// e^{iπ(j+1/8)/M}, j < N/4 (used before and after the FFT).
    twiddle: Vec<(f32, f32)>,
    /// e^{2πi·k/Q}, k < Q/2, for the FFT.
    roots: Vec<(f32, f32)>,
    bitrev: Vec<u32>,
    re: Vec<f32>,
    im: Vec<f32>,
    u: Vec<f32>,
}

impl Imdct {
    pub fn new(n: usize) -> Imdct {
        debug_assert!(n.is_power_of_two() && n >= 16);
        let m = n / 2;
        let q = n / 4;
        let pi = std::f64::consts::PI;
        let twiddle = (0..q)
            .map(|j| {
                let (s, c) = dmath::sin_cos_f64(pi * (j as f64 + 0.125) / m as f64);
                (c as f32, s as f32)
            })
            .collect();
        let roots = (0..q / 2)
            .map(|k| {
                let (s, c) = dmath::sin_cos_f64(2.0 * pi * k as f64 / q as f64);
                (c as f32, s as f32)
            })
            .collect();
        let bits = q.trailing_zeros();
        let bitrev = (0..q as u32).map(|i| if bits == 0 { 0 } else { i.reverse_bits() >> (32 - bits) }).collect();
        Imdct { n, twiddle, roots, bitrev, re: vec![0.0; q], im: vec![0.0; q], u: vec![0.0; m] }
    }

    /// `x`: N/2 coefficients; `y`: N outputs.
    pub fn inverse(&mut self, x: &[f32], y: &mut [f32]) {
        let n = self.n;
        let m = n / 2;
        let q = n / 4;
        debug_assert!(x.len() >= m && y.len() >= n);
        // Pre-twiddle into bit-reversed order.
        for j in 0..q {
            let (a, b) = (x[2 * j], -x[m - 1 - 2 * j]);
            let (c, s) = self.twiddle[j];
            let r = self.bitrev[j] as usize;
            self.re[r] = a * c - b * s;
            self.im[r] = a * s + b * c;
        }
        // In-place radix-2 FFT with positive exponent.
        let mut len = 2;
        while len <= q {
            let half = len / 2;
            let stride = q / len;
            for start in (0..q).step_by(len) {
                for k in 0..half {
                    let (wc, ws) = self.roots[k * stride];
                    let (i0, i1) = (start + k, start + k + half);
                    let tr = self.re[i1] * wc - self.im[i1] * ws;
                    let ti = self.re[i1] * ws + self.im[i1] * wc;
                    self.re[i1] = self.re[i0] - tr;
                    self.im[i1] = self.im[i0] - ti;
                    self.re[i0] += tr;
                    self.im[i0] += ti;
                }
            }
            len <<= 1;
        }
        // Post-twiddle into the DCT-IV output.
        for p in 0..q {
            let (c, s) = self.twiddle[p];
            let (a, b) = (self.re[p], self.im[p]);
            self.u[2 * p] = a * c - b * s;
            self.u[m - 1 - 2 * p] = a * s + b * c;
        }
        // Unfold.
        let u = &self.u;
        let h = m / 2;
        y[..h].copy_from_slice(&u[h..2 * h]);
        for i in h..3 * h {
            y[i] = -u[3 * h - 1 - i];
        }
        for i in 3 * h..n {
            y[i] = -u[i - 3 * h];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slow(x: &[f32]) -> Vec<f64> {
        let n = x.len() * 2;
        let pi = std::f64::consts::PI;
        (0..n)
            .map(|i| {
                x.iter()
                    .enumerate()
                    .map(|(k, &v)| v as f64 * dmath::cos_f64(2.0 * pi / n as f64 * (i as f64 + 0.5 + n as f64 / 4.0) * (k as f64 + 0.5)))
                    .sum()
            })
            .collect()
    }

    #[test]
    fn matches_the_definition() {
        for n in [16usize, 64, 256, 2048] {
            let x: Vec<f32> = (0..n / 2).map(|k| (((k * 7919) % 101) as f32 / 50.0 - 1.0) * if k % 3 == 0 { 2.0 } else { 0.5 }).collect();
            let mut y = vec![0f32; n];
            Imdct::new(n).inverse(&x, &mut y);
            let r = slow(&x);
            let scale = r.iter().fold(0f64, |m, v| m.max(v.abs()));
            for (i, (&a, &b)) in y.iter().zip(&r).enumerate() {
                assert!((a as f64 - b).abs() <= 1e-4 * scale.max(1.0), "n {n} i {i}: {a} vs {b}");
            }
        }
    }
}
