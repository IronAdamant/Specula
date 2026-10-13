//! Deterministic maths for mesh building (foundation F2): `specula_core::dmath` plus a
//! degree-based `sin_cos_deg` that keeps axis-aligned angles exact.
//!
//! Only IEEE basic operations are used, so results are identical on every platform.

pub use specula_core::dmath::*;

/// π as an `f64` (exact bits of `std::f64::consts::PI`).
pub const PI: f64 = std::f64::consts::PI;

/// `(sin, cos)` of an angle in degrees, reduced exactly to [0, 360) first so whole-degree
/// multiples of 90 give exact 0 and ±1.
pub fn sin_cos_deg(deg: f64) -> (f64, f64) {
    let r = deg - 360.0 * floor_f64(deg / 360.0);
    if r == 0.0 {
        return (0.0, 1.0);
    }
    if r == 90.0 {
        return (1.0, 0.0);
    }
    if r == 180.0 {
        return (0.0, -1.0);
    }
    if r == 270.0 {
        return (-1.0, 0.0);
    }
    sin_cos_f64(r * (PI / 180.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_quadrants_in_degrees() {
        assert_eq!(sin_cos_deg(90.0), (1.0, 0.0));
        assert_eq!(sin_cos_deg(-90.0), (-1.0, 0.0));
        assert_eq!(sin_cos_deg(540.0), (0.0, -1.0));
        let (s, c) = sin_cos_deg(30.0);
        assert!((s - 0.5).abs() < 1e-15 && (c - 0.75f64.sqrt()).abs() < 1e-15);
    }

    #[test]
    fn core_functions_reachable() {
        assert_eq!(floor(-0.5), -1.0);
        assert_eq!(round_f64(0.49999997), 0.0);
        assert!((sin_cos(1.0).0 - 1.0f32.sin()).abs() < 1e-6);
    }
}
