//! Deterministic hashing and noise: counter-based hashes (a particle's numbers never depend on
//! how many came before it) and 3D value noise with analytic derivatives, whose curl gives
//! divergence-free swirl. Only `+ − × ÷`, integer ops and `dmath::floor`: the same bits on every
//! platform.

use specula_core::dmath;

/// A 32-bit integer hash of three words (a murmur3-style finaliser over a mixed sum).
#[inline]
pub fn hash3(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77).rotate_left(13) ^ c.wrapping_mul(0xC2B2_AE3D).rotate_left(27);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

/// A uniform number in `[0, 1)` from `(seed, index, field)`.
#[inline]
pub fn unit(seed: u32, index: u32, field: u32) -> f32 {
    (hash3(seed, index, field) >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// Hashes a 64-bit value into 32 bits.
#[inline]
pub fn fold64(v: u64) -> u32 {
    hash3(v as u32, (v >> 32) as u32, 0x5EED_0001)
}

/// The noise repeats every `PERIOD` lattice cells on each axis, so a phase can be reduced modulo
/// it (in `f64`) before it becomes an `f32`: long-lived effects keep their precision.
pub const PERIOD: u32 = 1024;

/// Reduces a phase modulo [`PERIOD`] (seamless: the noise repeats there), in `f64`.
#[inline]
pub fn wrap_phase(t: f64) -> f32 {
    let p = f64::from(PERIOD);
    (t - p * dmath::floor_f64(t / p)) as f32
}

/// The lattice value in `[-1, 1]` at an integer point (periodic: see [`PERIOD`]).
#[inline]
fn lattice(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let (x, y, z) = (x as u32 & (PERIOD - 1), y as u32 & (PERIOD - 1), z as u32 & (PERIOD - 1));
    // A light spatial hash (three primes, one finalising multiply): the hot loop of the curl.
    let mut h = x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ z.wrapping_mul(0xCB1A_B31F) ^ seed;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
}

/// e^(−x) for x ≥ 0, to about 2e-6 relative, from IEEE basic operations only (so the same bits
/// everywhere, and cheaper than a correctly rounded `dmath::exp`; particles don't need the last
/// bit).
#[inline]
pub fn exp_neg(x: f32) -> f32 {
    if x.is_nan() {
        return f32::NAN;
    }
    if x > 87.0 {
        return 0.0;
    }
    let x = x.max(0.0);
    // e^(−x) = 2^(−x·log2 e) = 2^(−n) · 2^(−f), n whole, f in [0, 1).
    let y = x * std::f32::consts::LOG2_E;
    let n = specula_core::dmath::floor(y);
    let f = y - n;
    // 2^(−f) on [0, 1): Taylor in g = f·ln 2 to degree 7 (error below 3e-8).
    let g = -f * std::f32::consts::LN_2;
    let p = 1.0 + g * (1.0 + g * (0.5 + g * (1.0 / 6.0 + g * (1.0 / 24.0 + g * (1.0 / 120.0 + g * (1.0 / 720.0 + g * (1.0 / 5040.0)))))));
    let scale = f32::from_bits(((127 - n as i32).max(1) as u32) << 23);
    p * scale
}

/// 3D value noise in about `[-1, 1]` and its gradient, quintic-interpolated (C2 smooth).
pub fn value_noise(p: [f32; 3], seed: u32) -> (f32, [f32; 3]) {
    let fl = [dmath::floor(p[0]), dmath::floor(p[1]), dmath::floor(p[2])];
    // Saturating casts: huge coordinates still hash to a lattice point.
    let i = [fl[0] as i32, fl[1] as i32, fl[2] as i32];
    let f = [p[0] - fl[0], p[1] - fl[1], p[2] - fl[2]];
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let dfade = |t: f32| 30.0 * t * t * (t * (t - 2.0) + 1.0);
    let u = [fade(f[0]), fade(f[1]), fade(f[2])];
    let du = [dfade(f[0]), dfade(f[1]), dfade(f[2])];
    let c = |dx: i32, dy: i32, dz: i32| lattice(i[0].wrapping_add(dx), i[1].wrapping_add(dy), i[2].wrapping_add(dz), seed);
    let (a, b, cc, d) = (c(0, 0, 0), c(1, 0, 0), c(0, 1, 0), c(1, 1, 0));
    let (e, ff, g, h) = (c(0, 0, 1), c(1, 0, 1), c(0, 1, 1), c(1, 1, 1));
    // Trilinear in u: v = k0 + k1 u + k2 v + k3 w + k4 uv + k5 vw + k6 wu + k7 uvw.
    let k0 = a;
    let k1 = b - a;
    let k2 = cc - a;
    let k3 = e - a;
    let k4 = a - b - cc + d;
    let k5 = a - cc - e + g;
    let k6 = a - b - e + ff;
    let k7 = -a + b + cc - d + e - ff - g + h;
    let (x, y, z) = (u[0], u[1], u[2]);
    let v = k0 + k1 * x + k2 * y + k3 * z + k4 * x * y + k5 * y * z + k6 * z * x + k7 * x * y * z;
    let grad = [
        du[0] * (k1 + k4 * y + k6 * z + k7 * y * z),
        du[1] * (k2 + k5 * z + k4 * x + k7 * z * x),
        du[2] * (k3 + k6 * x + k5 * y + k7 * x * y),
    ];
    (v, grad)
}

/// A divergence-free vector field, about unit magnitude on average: the curl of the vector
/// potential `ψ1·A + ψ2·B` (two value-noise fields along fixed axes), which is
/// `∇ψ1 × A + ∇ψ2 × B` (two noise evaluations instead of three; the divergence of each term is
/// zero). `t` scrolls each field along its own direction, so it evolves without a fourth
/// dimension. `t` is reduced modulo the noise's period in `f64` (any time keeps its precision).
pub fn curl(p: [f32; 3], t: f64, seed: u32) -> [f32; 3] {
    const OFFSETS: [[f32; 3]; 2] = [[0.0, 0.0, 0.0], [31.416, 47.853, 12.793]];
    const SCROLL: [[f32; 3]; 2] = [[0.37, 0.61, -0.21], [-0.53, 0.29, 0.44]];
    // A tilted towards +Y (swirls about the vertical, as rising smoke does), B across it.
    const AXES: [[f32; 3]; 2] = [[0.236, 0.943, 0.236], [0.707, 0.0, -0.707]];
    // Gradients peak near 1.9 and average about 1.
    const SCALE: f32 = 0.7;
    let mut out = [0.0f32; 3];
    for k in 0..2 {
        let s = SCROLL[k];
        let q = [
            p[0] + OFFSETS[k][0] + wrap_phase(f64::from(s[0]) * t),
            p[1] + OFFSETS[k][1] + wrap_phase(f64::from(s[1]) * t),
            p[2] + OFFSETS[k][2] + wrap_phase(f64::from(s[2]) * t),
        ];
        let g = value_noise(q, seed.wrapping_add(k as u32 * 0x68E3_1DA4)).1;
        let a = AXES[k];
        out[0] += (g[1] * a[2] - g[2] * a[1]) * SCALE;
        out[1] += (g[2] * a[0] - g[0] * a[2]) * SCALE;
        out[2] += (g[0] * a[1] - g[1] * a[0]) * SCALE;
    }
    out
}

/// A smooth 1D noise in about `[-1, 1]` (light flicker); `t` is reduced modulo the period in
/// `f64`.
pub fn noise1(t: f64, seed: u32) -> f32 {
    value_noise([wrap_phase(t), 0.5, 0.5], seed).0
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN_HASHES: [u32; 3] = [0, 3_486_708_137, 1_497_483_828];
    const GOLDEN_UNIT: u32 = 1_064_703_097;
    const GOLDEN_NOISE: [u32; 4] = [1_055_495_030, 3_201_345_898, 3_194_924_796, 3_206_315_791];

    #[test]
    fn hashes_are_stable_and_spread() {
        // Pinned: a change here changes every effect's look (and every capture).
        assert_eq!([hash3(0, 0, 0), hash3(1, 2, 3), hash3(0xDEAD_BEEF, 7, 0x0B57)], GOLDEN_HASHES);
        assert_eq!(unit(7, 3, 1).to_bits(), GOLDEN_UNIT);
        assert_ne!(hash3(1, 0, 0), hash3(0, 1, 0));
        let mut buckets = [0u32; 16];
        for i in 0..16_000 {
            buckets[(unit(7, i, 3) * 16.0) as usize] += 1;
        }
        assert!(buckets.iter().all(|&n| (800..1200).contains(&n)), "{buckets:?}");
    }

    #[test]
    fn noise_values_are_pinned_and_long_times_keep_their_precision() {
        // Pinned bits: noise and curl at fixed points (a change here changes every capture).
        let v = value_noise([0.3, 1.7, -2.2], 42);
        let c = curl([0.5, 1.0, -0.25], 12.5, 9);
        assert_eq!([v.0.to_bits(), c[0].to_bits(), c[1].to_bits(), c[2].to_bits()], GOLDEN_NOISE);
        // Periodic: one period on is the same point.
        let p = PERIOD as f32;
        assert_eq!(value_noise([0.25 + p, 1.75, -2.25 - p], 42), value_noise([0.25, 1.75, -2.25], 42));
        // A million seconds in, a 1/60 s step still moves the flicker smoothly (no f32 stair).
        let (a, b) = (noise1(1.0e6, 3), noise1(1.0e6 + 1.0 / 60.0, 3));
        let (x, y) = (noise1(0.0, 3), noise1(1.0 / 60.0, 3));
        assert!((a - b).abs() < 0.2 && (a - b) != 0.0 && (x - y) != 0.0, "{a} {b}");
        let (a, b) = (curl([0.1, 0.2, 0.3], 1.0e7, 1), curl([0.1, 0.2, 0.3], 1.0e7 + 1.0 / 240.0, 1));
        assert!(a != b && (a[0] - b[0]).abs() < 0.1, "{a:?} {b:?}");
    }

    #[test]
    fn exp_neg_matches_dmath() {
        for i in 0..2000 {
            let x = i as f32 * 0.01;
            let (a, b) = (exp_neg(x), specula_core::dmath::exp(-x));
            assert!((a - b).abs() <= 3e-6 * b.max(1e-30), "{x}: {a} vs {b}");
        }
        assert_eq!(exp_neg(0.0), 1.0);
        assert_eq!(exp_neg(200.0), 0.0);
    }

    #[test]
    fn noise_is_continuous_and_its_gradient_matches_finite_differences() {
        let seed = 42;
        for &p in &[[0.3f32, 1.7, -2.2], [10.5, 0.01, 3.99], [-7.25, -0.5, 0.75]] {
            let (v, g) = value_noise(p, seed);
            assert!(v.abs() <= 1.0 + 1e-5);
            let e = 1e-3;
            for axis in 0..3 {
                let mut a = p;
                let mut b = p;
                a[axis] -= e;
                b[axis] += e;
                let fd = (value_noise(b, seed).0 - value_noise(a, seed).0) / (2.0 * e);
                assert!((fd - g[axis]).abs() < 2e-2, "axis {axis} at {p:?}: {fd} vs {}", g[axis]);
            }
        }
        // Across a lattice boundary the value doesn't jump.
        let a = value_noise([0.9999, 0.5, 0.5], 1).0;
        let b = value_noise([1.0001, 0.5, 0.5], 1).0;
        assert!((a - b).abs() < 1e-3);
    }

    #[test]
    fn curl_is_divergence_free_and_bounded() {
        let seed = 9;
        let e = 1e-3;
        let mut mean = 0.0;
        for i in 0..50 {
            let p = [i as f32 * 0.37, 1.0 + i as f32 * 0.11, -(i as f32) * 0.23];
            let c = curl(p, 0.0, seed);
            mean += (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 50.0;
            let mut div = 0.0;
            for axis in 0..3 {
                let mut a = p;
                let mut b = p;
                a[axis] -= e;
                b[axis] += e;
                div += (curl(b, 0.0, seed)[axis] - curl(a, 0.0, seed)[axis]) / (2.0 * e);
            }
            assert!(div.abs() < 0.05, "divergence {div} at {p:?}");
        }
        assert!((0.1..2.0).contains(&mean), "mean magnitude {mean}");
    }
}
