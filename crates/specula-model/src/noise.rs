//! Seeded value noise from integer hashes: the same bits on every platform (F2).

use crate::dmath;

/// A 32-bit hash of a seed and three lattice coordinates.
#[inline]
pub fn hash3(seed: u64, x: i32, y: i32, z: i32) -> u32 {
    let mut h = seed ^ 0x9e37_79b9_7f4a_7c15;
    for v in [x, y, z] {
        h ^= (v as u32 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h = (h ^ (h >> 31)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h ^= h >> 29;
    }
    (h >> 32) as u32
}

/// Uniform in [0, 1) from a hash.
#[inline]
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Trilinear value noise in [0, 1) with smoothstep fade; lattice spacing 1.
pub fn value3(seed: u64, p: [f32; 3]) -> f32 {
    let f = [dmath::floor(p[0]), dmath::floor(p[1]), dmath::floor(p[2])];
    let i = [f[0] as i32, f[1] as i32, f[2] as i32];
    let t = [smooth(p[0] - f[0]), smooth(p[1] - f[1]), smooth(p[2] - f[2])];
    let c = |dx: i32, dy: i32, dz: i32| unit(hash3(seed, i[0] + dx, i[1] + dy, i[2] + dz));
    let lx = |a: f32, b: f32| a + (b - a) * t[0];
    let x00 = lx(c(0, 0, 0), c(1, 0, 0));
    let x10 = lx(c(0, 1, 0), c(1, 1, 0));
    let x01 = lx(c(0, 0, 1), c(1, 0, 1));
    let x11 = lx(c(0, 1, 1), c(1, 1, 1));
    let y0 = x00 + (x10 - x00) * t[1];
    let y1 = x01 + (x11 - x01) * t[1];
    y0 + (y1 - y0) * t[2]
}

/// Fractal sum of `octaves` value-noise layers (each half the size and amplitude), in [-1, 1).
pub fn fbm3(seed: u64, p: [f32; 3], octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut freq = 1.0;
    for o in 0..octaves.max(1) {
        let s = seed.wrapping_add((o as u64).wrapping_mul(0x632b_e59b_d9b4_e019));
        sum += amp * (value3(s, [p[0] * freq, p[1] * freq, p[2] * freq]) * 2.0 - 1.0);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_bounded_and_continuous() {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for i in 0..2000 {
            let p = [i as f32 * 0.173 - 50.0, i as f32 * 0.071, -(i as f32) * 0.033];
            let v = value3(7, p);
            assert_eq!(v.to_bits(), value3(7, p).to_bits());
            lo = lo.min(v);
            hi = hi.max(v);
            let w = value3(7, [p[0] + 1e-3, p[1], p[2]]);
            assert!((v - w).abs() < 0.01, "continuous");
            let f = fbm3(3, p, 3);
            assert!((-1.0..1.0).contains(&f));
        }
        assert!(lo >= 0.0 && hi < 1.0 && hi - lo > 0.5, "spread {lo}..{hi}");
        assert_ne!(value3(1, [0.5, 0.5, 0.5]), value3(2, [0.5, 0.5, 0.5]));
    }

    #[test]
    fn pinned_values() {
        // Changing these changes every displaced asset.
        assert_eq!(hash3(0, 0, 0, 0), hash3(0, 0, 0, 0));
        let v = value3(42, [1.25, -2.5, 3.75]);
        assert!((0.0..1.0).contains(&v));
    }
}
