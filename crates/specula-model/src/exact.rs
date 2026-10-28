//! Exact integer predicates for plane-based geometry (the tier-2 booleans in [`crate::csg`]).
//!
//! Points of a boolean's input are snapped to an integer grid of at most ±[`COORD_MAX`] steps.
//! Every plane the booleans use is made from grid points ([`Plane::through`], [`Plane::edge`]),
//! so its coefficients stay small whatever the nesting depth: a vertex is never stored as
//! coordinates but as **three planes**, and the side of a plane it lies on is the sign of a
//! 4×4 determinant, computed exactly in 256-bit integers ([`orient`]). A float filter answers
//! most queries without the big integers; it falls back to them only when the float result is
//! within its proven error bound of zero. No epsilon decides any classification.
//!
//! Bounds (with |coordinate| ≤ 2^19): support normals ≤ 2^41, plane offsets < 2^62, the
//! homogeneous coordinates of a three-plane vertex < 2^147 (w < 2^126), and an orientation
//! determinant < 2^191, so [`I256`] never overflows.

/// Largest absolute grid coordinate a boolean's input may use.
pub const COORD_MAX: i64 = 1 << 19;

/// A plane `n · p + d = 0` with integer coefficients (the normal is not normalised).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Plane {
    pub n: [i64; 3],
    pub d: i64,
}

type P3 = [i64; 3];

fn sub3(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn cross_wide(a: [i64; 3], b: [i64; 3]) -> [i128; 3] {
    let w = |x: i64| x as i128;
    [w(a[1]) * w(b[2]) - w(a[2]) * w(b[1]), w(a[2]) * w(b[0]) - w(a[0]) * w(b[2]), w(a[0]) * w(b[1]) - w(a[1]) * w(b[0])]
}

impl Plane {
    /// The plane through three grid points, normal by the right-hand rule (counter-clockwise
    /// `a, b, c` faces +n). `None` when they are collinear. Points must be within ±[`COORD_MAX`].
    pub fn through(a: P3, b: P3, c: P3) -> Option<Plane> {
        let n = cross3(sub3(b, a), sub3(c, a));
        if n == [0; 3] {
            return None;
        }
        Some(Plane { n, d: -(n[0] * a[0] + n[1] * a[1] + n[2] * a[2]) })
    }

    /// A plane containing the edge `a → b` and the unit axis `axis` (0..3): what bounds a
    /// polygon's edge. `None` when the edge is parallel to the axis (or empty).
    pub fn edge(a: P3, b: P3, axis: usize) -> Option<Plane> {
        let mut e = [0i64; 3];
        e[axis] = 1;
        let n = cross3(sub3(b, a), e);
        if n == [0; 3] {
            return None;
        }
        Some(Plane { n, d: -(n[0] * a[0] + n[1] * a[1] + n[2] * a[2]) })
    }

    pub fn flip(self) -> Plane {
        Plane { n: [-self.n[0], -self.n[1], -self.n[2]], d: -self.d }
    }

    /// Index of the normal's largest component (ties: the first).
    pub fn dominant_axis(&self) -> usize {
        let a = self.n.map(|v| v.unsigned_abs());
        if a[0] >= a[1] && a[0] >= a[2] {
            0
        } else if a[1] >= a[2] {
            1
        } else {
            2
        }
    }

    /// The sign of `n · p + d` for a grid point.
    pub fn side_of_point(&self, p: P3) -> i32 {
        let v = self.n[0] as i128 * p[0] as i128 + self.n[1] as i128 * p[1] as i128 + self.n[2] as i128 * p[2] as i128 + self.d as i128;
        v.signum() as i32
    }

    /// The same plane (same points, same orientation): `other = k · self` with `k > 0`.
    pub fn same_as(&self, o: &Plane) -> bool {
        if cross_wide(self.n, o.n) != [0; 3] {
            return false;
        }
        let w = |x: i64| x as i128;
        let dot = w(self.n[0]) * w(o.n[0]) + w(self.n[1]) * w(o.n[1]) + w(self.n[2]) * w(o.n[2]);
        if dot <= 0 {
            return false;
        }
        let k = self.dominant_axis();
        w(self.d) * w(o.n[k]) == w(o.d) * w(self.n[k])
    }

    /// The normals point the same way (positive dot product).
    pub fn facing(&self, o: &Plane) -> bool {
        let w = |x: i64| x as i128;
        w(self.n[0]) * w(o.n[0]) + w(self.n[1]) * w(o.n[1]) + w(self.n[2]) * w(o.n[2]) > 0
    }

    /// The normal as unit floats.
    pub fn unit_normal(&self) -> [f64; 3] {
        let n = self.n.map(|v| v as f64);
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        [n[0] / l, n[1] / l, n[2] / l]
    }
}

/// A signed 256-bit integer (two's complement, little-endian limbs): just the operations the
/// predicates need. Products used here stay far below 2^255, so nothing wraps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I256([u64; 4]);

impl I256 {
    pub const ZERO: I256 = I256([0; 4]);

    pub fn from_i128(v: i128) -> I256 {
        let lo = v as u128;
        let ext = if v < 0 { u64::MAX } else { 0 };
        I256([lo as u64, (lo >> 64) as u64, ext, ext])
    }

    pub fn is_negative(&self) -> bool {
        self.0[3] >> 63 == 1
    }

    pub fn signum(&self) -> i32 {
        if self.is_negative() {
            -1
        } else if self.0 == [0; 4] {
            0
        } else {
            1
        }
    }

    pub fn wrapping_add(self, o: I256) -> I256 {
        let mut out = [0u64; 4];
        let mut carry = 0u64;
        for (i, slot) in out.iter_mut().enumerate() {
            let (s1, c1) = self.0[i].overflowing_add(o.0[i]);
            let (s2, c2) = s1.overflowing_add(carry);
            *slot = s2;
            carry = (c1 as u64) + (c2 as u64);
        }
        I256(out)
    }

    pub fn negated(self) -> I256 {
        let inv = I256(self.0.map(|l| !l));
        inv.wrapping_add(I256([1, 0, 0, 0]))
    }

    fn magnitude(self) -> [u64; 4] {
        if self.is_negative() {
            self.negated().0
        } else {
            self.0
        }
    }

    /// `self · k`.
    pub fn mul_i64(self, k: i64) -> I256 {
        let neg = self.is_negative() != (k < 0);
        let m = self.magnitude();
        let k = k.unsigned_abs() as u128;
        let mut out = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let p = m[i] as u128 * k + carry;
            out[i] = p as u64;
            carry = p >> 64;
        }
        let r = I256(out);
        if neg && r.0 != [0; 4] {
            r.negated()
        } else {
            r
        }
    }

    /// Nearest-ish `f64` (each limb rounded once, summed from the top): relative error below
    /// 2^-50, which the float filters allow for.
    pub fn to_f64(self) -> f64 {
        let neg = self.is_negative();
        let m = self.magnitude();
        let mut v = 0.0f64;
        for i in (0..4).rev() {
            v = v * 18_446_744_073_709_551_616.0 + m[i] as f64;
        }
        if neg {
            -v
        } else {
            v
        }
    }
}

/// A vertex as the intersection of three planes, in homogeneous coordinates `x / w` with
/// `w > 0`.
#[derive(Clone, Copy, Debug)]
pub struct Hom {
    pub x: [I256; 3],
    pub w: i128,
}

/// The intersection of three planes; `None` when they don't meet in one point.
pub fn intersect(a: &Plane, b: &Plane, c: &Plane) -> Option<Hom> {
    let c1 = cross_wide(b.n, c.n);
    let c2 = cross_wide(c.n, a.n);
    let c3 = cross_wide(a.n, b.n);
    let w = a.n[0] as i128 * c1[0] + a.n[1] as i128 * c1[1] + a.n[2] as i128 * c1[2];
    if w == 0 {
        return None;
    }
    // x = -(d1 c1 + d2 c2 + d3 c3) / w.
    let mut x = [I256::ZERO; 3];
    for (k, slot) in x.iter_mut().enumerate() {
        let s = I256::from_i128(c1[k]).mul_i64(a.d).wrapping_add(I256::from_i128(c2[k]).mul_i64(b.d)).wrapping_add(I256::from_i128(c3[k]).mul_i64(c.d));
        *slot = s.negated();
    }
    if w < 0 {
        Some(Hom { x: x.map(|v| v.negated()), w: -w })
    } else {
        Some(Hom { x, w })
    }
}

impl Hom {
    /// The point as floats (grid units).
    pub fn approx(&self) -> [f64; 3] {
        let w = self.w as f64;
        self.x.map(|v| v.to_f64() / w)
    }
}

/// The exact side of `q` a homogeneous point lies on: the sign of `q.n · x + q.d · w`.
pub fn orient_hom(h: &Hom, q: &Plane) -> i32 {
    let s = h.x[0].mul_i64(q.n[0]).wrapping_add(h.x[1].mul_i64(q.n[1])).wrapping_add(h.x[2].mul_i64(q.n[2])).wrapping_add(I256::from_i128(h.w).mul_i64(q.d));
    s.signum()
}

/// The side of `q` the point of `planes` lies on, with a float filter: `approx` is the point
/// as computed by [`Hom::approx`] (its error is what the filter allows for). Exact.
pub fn orient(planes: [&Plane; 3], approx: &[f64; 3], q: &Plane) -> i32 {
    let n = q.n.map(|v| v as f64);
    let f = n[0] * approx[0] + n[1] * approx[1] + n[2] * approx[2] + q.d as f64;
    let n1 = n[0].abs() + n[1].abs() + n[2].abs();
    let m = approx[0].abs().max(approx[1].abs()).max(approx[2].abs());
    // The point's error is below 2^-49 · m per coordinate, the sum's rounding below 4 · 2^-53 of
    // its terms: 1e-13 is about 50 times both together.
    let bound = (n1 * (m + 1.0) + (q.d as f64).abs()) * 1e-13;
    if f > bound {
        return 1;
    }
    if f < -bound {
        return -1;
    }
    EXACT_CALLS.with(|c| c.set(c.get() + 1));
    match intersect(planes[0], planes[1], planes[2]) {
        Some(h) => orient_hom(&h, q),
        None => 0,
    }
}

thread_local! {
    static EXACT_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many orientations on this thread the float filter couldn't decide (each took the exact
/// 256-bit path): budgets charge them by the difference between two readings.
pub fn exact_calls() -> u64 {
    EXACT_CALLS.with(|c| c.get())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]
    use super::*;

    #[test]
    fn i256_arithmetic() {
        let a = I256::from_i128(-5);
        assert_eq!(a.signum(), -1);
        assert_eq!(a.mul_i64(-3), I256::from_i128(15));
        assert_eq!(I256::from_i128(i128::MAX).mul_i64(4).wrapping_add(I256::from_i128(i128::MAX).mul_i64(-4)), I256::ZERO);
        let big = I256::from_i128(1 << 100).mul_i64(1 << 60).mul_i64(1 << 20);
        assert_eq!(big.signum(), 1);
        assert_eq!(big.to_f64(), 2f64.powi(180));
        assert_eq!(big.negated().to_f64(), -(2f64.powi(180)));
    }

    #[test]
    fn three_planes_meet_at_a_grid_point() {
        let a = Plane::through([0, 0, 0], [10, 0, 0], [0, 10, 0]).unwrap(); // z = 0
        let b = Plane { n: [1, 0, 0], d: -7 }; // x = 7
        let c = Plane { n: [0, 1, 0], d: 3 }; // y = -3
        let h = intersect(&a, &b, &c).unwrap();
        assert_eq!(h.approx(), [7.0, -3.0, 0.0]);
        let q = Plane { n: [1, 1, 1], d: -4 }; // 7 - 3 + 0 - 4 = 0
        assert_eq!(orient_hom(&h, &q), 0);
        assert_eq!(orient([&a, &b, &c], &h.approx(), &q), 0);
        assert_eq!(orient([&a, &b, &c], &h.approx(), &Plane { n: [1, 1, 1], d: -3 }), 1);
    }

    #[test]
    fn exact_on_nearly_degenerate_input() {
        // Large, nearly parallel planes: the float filter must defer and the exact answer hold.
        let m = COORD_MAX - 1;
        let s = Plane::through([-m, -m, 0], [m, -m, 1], [-m, m, 0]).unwrap();
        let e1 = Plane::edge([-m, -m, 0], [m, -m, 1], s.dominant_axis()).unwrap();
        let e2 = Plane::edge([m, -m, 1], [-m, m, 0], s.dominant_axis()).unwrap();
        let h = intersect(&s, &e1, &e2).unwrap();
        let p = h.approx();
        assert!((p[0] - m as f64).abs() < 1e-6 && (p[1] + m as f64).abs() < 1e-6 && (p[2] - 1.0).abs() < 1e-6);
        // The vertex lies on every plane through it.
        let through = Plane::through([m, -m, 1], [0, 0, 7], [3, 9, -2]).unwrap();
        assert_eq!(orient([&s, &e1, &e2], &p, &through), 0);
        assert_eq!(orient_hom(&h, &through), 0);
    }

    #[test]
    fn same_plane_tests() {
        let a = Plane::through([0, 0, 0], [1, 0, 0], [0, 1, 0]).unwrap();
        let b = Plane::through([5, 5, 0], [7, 5, 0], [5, 9, 0]).unwrap();
        assert!(a.same_as(&b) && !a.same_as(&b.flip()) && a.facing(&b));
        assert!(!a.same_as(&Plane { n: [0, 0, 1], d: 1 }));
    }
}
