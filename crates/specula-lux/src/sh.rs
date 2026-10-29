//! L1 spherical harmonics for irradiance, pre-convolved (docs/design/GI.md, "What a probe holds").
//!
//! A probe's irradiance is `E(n) = c0 + c1 · n` per colour channel, in the renderer's units: a
//! surface's diffuse light is `albedo × E`, and a uniform sky of radiance `L` gives `E = L`.
//! Projected from `N` uniformly spread directions `ωᵢ` with radiance `Lᵢ`:
//!
//! - `c0 = (1/N) Σ Lᵢ` (the band-0 coefficient times its basis constant, `Y₀₀² = 1/4π`, with the
//!   Monte Carlo weight `4π/N`);
//! - `c1 = (2/N) Σ Lᵢ ωᵢ` (band 1 times its basis constant, `Y₁² = 3/4π`, and the cosine lobe's
//!   band factor `2π/3`, over the diffuse BRDF's `π`).
//!
//! [`Sh::finish`] clamps `|c1| ≤ c0` per channel, so `E` is never negative (no ringing).

use specula_core::dmath;
use specula_core::math::Vec3;

/// Irradiance of one probe: `c0` and `c1` (by axis, then channel).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sh {
    pub c0: [f32; 3],
    /// `c1[axis][channel]`: the gradient of irradiance along X, Y, Z.
    pub c1: [[f32; 3]; 3],
}

impl Sh {
    /// Irradiance for a surface facing `n` (unit), clamped non-negative.
    pub fn eval(&self, n: Vec3) -> [f32; 3] {
        let mut e = [0.0; 3];
        for (k, out) in e.iter_mut().enumerate() {
            *out = (self.c0[k] + self.c1[0][k] * n.x + self.c1[1][k] * n.y + self.c1[2][k] * n.z).max(0.0);
        }
        e
    }

    /// Luminance of the mean (c0), Rec. 709.
    pub fn luma(&self) -> f32 {
        0.2126 * self.c0[0] + 0.7152 * self.c0[1] + 0.0722 * self.c0[2]
    }
}

/// Accumulates radiance samples into an [`Sh`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Accum {
    sum: [f32; 3],
    dir: [[f32; 3]; 3],
    n: u32,
}

impl Accum {
    pub fn add(&mut self, dir: Vec3, radiance: [f32; 3]) {
        for (k, r) in radiance.into_iter().enumerate() {
            self.sum[k] += r;
            self.dir[0][k] += r * dir.x;
            self.dir[1][k] += r * dir.y;
            self.dir[2][k] += r * dir.z;
        }
        self.n += 1;
    }

    /// The pre-convolved coefficients, `|c1| ≤ c0` per channel.
    pub fn finish(&self) -> Sh {
        if self.n == 0 {
            return Sh::default();
        }
        let inv = 1.0 / self.n as f32;
        let mut sh = Sh::default();
        for k in 0..3 {
            let c0 = (self.sum[k] * inv).max(0.0);
            let mut c1 = [2.0 * inv * self.dir[0][k], 2.0 * inv * self.dir[1][k], 2.0 * inv * self.dir[2][k]];
            let len = (c1[0] * c1[0] + c1[1] * c1[1] + c1[2] * c1[2]).sqrt();
            if len > c0 {
                let s = if len > 0.0 { c0 / len } else { 0.0 };
                c1 = [c1[0] * s, c1[1] * s, c1[2] * s];
            }
            sh.c0[k] = c0;
            for (a, v) in c1.iter().enumerate() {
                sh.c1[a][k] = *v;
            }
        }
        sh
    }
}

/// `n` directions spread evenly over the sphere (a spherical Fibonacci set), in a fixed order.
pub fn fibonacci(n: usize) -> Vec<Vec3> {
    // The golden angle, 2π(1 − 1/φ).
    const GOLDEN: f32 = 2.399_963_3;
    (0..n)
        .map(|i| {
            let z = 1.0 - (2.0 * i as f32 + 1.0) / n as f32;
            let r = (1.0 - z * z).max(0.0).sqrt();
            // The angle wrapped to [0, 2π) first: dmath's range reduction is exact, but small
            // arguments are cheaper.
            let phi = dmath::fract(i as f32 * (GOLDEN / std::f32::consts::TAU)) * std::f32::consts::TAU;
            let (s, c) = dmath::sin_cos(phi);
            Vec3::new(r * c, r * s, z)
        })
        .collect()
}

/// A rotation (unit quaternion as `[x, y, z, w]`) drawn from `seed`, uniform over rotations
/// (Shoemake's method): each probe turns the ray set its own way, so neighbours don't share the
/// set's pattern.
pub fn rotation(seed: u64) -> [f32; 4] {
    let u = |k: u64| {
        let mut h = specula_core::hash::StableHasher::with_seed(0x4c55_5852_4f54_0000 ^ k);
        h.write_u64(seed);
        (h.finish() >> 40) as f32 / (1u64 << 24) as f32
    };
    let (u1, u2, u3) = (u(1), u(2), u(3));
    let a = (1.0 - u1).sqrt();
    let b = u1.sqrt();
    let (s2, c2) = dmath::sin_cos(std::f32::consts::TAU * u2);
    let (s3, c3) = dmath::sin_cos(std::f32::consts::TAU * u3);
    [a * s2, a * c2, b * s3, b * c3]
}

/// `v` turned by unit quaternion `q`.
pub fn rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let u = Vec3::new(q[0], q[1], q[2]);
    let t = u.cross(v) * 2.0;
    v + t * q[3] + u.cross(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(n: usize, f: impl Fn(Vec3) -> [f32; 3]) -> Sh {
        let mut a = Accum::default();
        for d in fibonacci(n) {
            a.add(d, f(d));
        }
        a.finish()
    }

    #[test]
    fn a_uniform_sky_gives_its_radiance_everywhere() {
        let sh = project(512, |_| [0.2, 0.3, 0.4]);
        for n in [Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0)] {
            let e = sh.eval(n);
            for k in 0..3 {
                assert!((e[k] - [0.2, 0.3, 0.4][k]).abs() < 1e-3, "{e:?} at {n:?}");
            }
        }
    }

    #[test]
    fn an_upper_hemisphere_lights_up_facing_surfaces_fully_and_down_facing_ones_not_at_all() {
        let sh = project(2048, |d| if d.y > 0.0 { [1.0; 3] } else { [0.0; 3] });
        // Exact irradiance (over π): 1 facing up, 0.5 sideways, 0 facing down.
        assert!((sh.eval(Vec3::new(0.0, 1.0, 0.0))[0] - 1.0).abs() < 0.01);
        assert!((sh.eval(Vec3::new(1.0, 0.0, 0.0))[0] - 0.5).abs() < 0.01);
        assert!(sh.eval(Vec3::new(0.0, -1.0, 0.0))[0] < 0.01);
    }

    #[test]
    fn a_single_bright_direction_never_rings_negative() {
        let sh = project(256, |d| if d.x > 0.98 { [100.0; 3] } else { [0.0; 3] });
        let len = (sh.c1[0][0] * sh.c1[0][0] + sh.c1[1][0] * sh.c1[1][0] + sh.c1[2][0] * sh.c1[2][0]).sqrt();
        assert!(len <= sh.c0[0] * 1.000_01, "clamped: |c1| = {len} vs c0 = {}", sh.c0[0]);
        assert!(sh.eval(Vec3::new(-1.0, 0.0, 0.0))[0] >= 0.0);
    }

    #[test]
    fn fibonacci_directions_are_unit_and_balanced() {
        let d = fibonacci(128);
        assert_eq!(d.len(), 128);
        let mut sum = Vec3::ZERO;
        for v in &d {
            assert!((v.length() - 1.0).abs() < 1e-5);
            sum = sum + *v;
        }
        assert!(sum.length() < 0.05 * 128.0, "spread evenly: {sum:?}");
        assert_eq!(fibonacci(128), d, "deterministic");
    }

    #[test]
    fn rotations_are_unit_and_keep_lengths() {
        for seed in 0..64 {
            let q = rotation(seed);
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            assert!((n - 1.0).abs() < 1e-5);
            let v = rotate(q, Vec3::new(0.3, -0.5, 0.81));
            assert!((v.length() - Vec3::new(0.3, -0.5, 0.81).length()).abs() < 1e-5);
        }
        assert_ne!(rotation(1), rotation(2));
        assert_eq!(rotation(7), rotation(7));
    }
}
