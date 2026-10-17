//! Small maths the solver and the shape tests need on top of `specula_core::math`.
//!
//! Only IEEE basic operations and `sqrt` (correctly rounded everywhere) are used here, plus
//! `specula_core::dmath` for the few trigonometric conversions, so every result is bit-identical
//! across platforms (docs/design/DETERMINISM.md, rule 4). No FMA, no reordering: expressions are
//! written out in the order they are evaluated.

pub use specula_core::math::{Quat, Vec3};
use specula_core::dmath;

/// Lengths below this are treated as zero when normalising.
pub const TINY: f32 = 1.0e-12;

#[inline]
pub fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

#[inline]
pub fn splat(v: f32) -> Vec3 {
    Vec3 { x: v, y: v, z: v }
}

/// Component-wise product.
#[inline]
pub fn mul(a: Vec3, b: Vec3) -> Vec3 {
    v3(a.x * b.x, a.y * b.y, a.z * b.z)
}

#[inline]
pub fn abs(a: Vec3) -> Vec3 {
    v3(a.x.abs(), a.y.abs(), a.z.abs())
}

#[inline]
pub fn len_sq(a: Vec3) -> f32 {
    a.dot(a)
}

#[inline]
pub fn dist_sq(a: Vec3, b: Vec3) -> f32 {
    len_sq(a - b)
}

#[inline]
pub fn get(a: Vec3, i: usize) -> f32 {
    match i {
        0 => a.x,
        1 => a.y,
        _ => a.z,
    }
}

#[inline]
pub fn set(a: &mut Vec3, i: usize, v: f32) {
    match i {
        0 => a.x = v,
        1 => a.y = v,
        _ => a.z = v,
    }
}

#[inline]
pub fn max_comp(a: Vec3) -> f32 {
    a.x.max(a.y).max(a.z)
}

#[inline]
pub fn min_comp(a: Vec3) -> f32 {
    a.x.min(a.y).min(a.z)
}

#[inline]
pub fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

pub fn is_finite(a: Vec3) -> bool {
    a.x.is_finite() && a.y.is_finite() && a.z.is_finite()
}

/// `a` normalised, or `None` when it is (nearly) zero.
#[inline]
pub fn try_normalize(a: Vec3) -> Option<Vec3> {
    let l2 = len_sq(a);
    if l2 > TINY {
        let l = l2.sqrt();
        Some(a * (1.0 / l))
    } else {
        None
    }
}

/// `a` normalised, or `fallback` when it is (nearly) zero.
#[inline]
pub fn normalize_or(a: Vec3, fallback: Vec3) -> Vec3 {
    try_normalize(a).unwrap_or(fallback)
}

/// Two unit vectors that make a right-handed orthonormal basis with unit `n` (deterministic:
/// the same `n` always gives the same pair).
pub fn basis(n: Vec3) -> (Vec3, Vec3) {
    // Pick the axis least aligned with n.
    let a = if n.x.abs() < 0.57735 { Vec3::X } else if n.y.abs() < 0.57735 { Vec3::Y } else { Vec3::Z };
    let t1 = normalize_or(n.cross(a), Vec3::X);
    let t2 = n.cross(t1);
    (t1, t2)
}

/// 3×3 matrix, column-major (`c[i]` is column i).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub c: [Vec3; 3],
}

impl Mat3 {
    pub const ZERO: Mat3 = Mat3 { c: [Vec3::ZERO, Vec3::ZERO, Vec3::ZERO] };
    pub const IDENTITY: Mat3 = Mat3 { c: [Vec3::X, Vec3::Y, Vec3::Z] };

    pub fn diag(d: Vec3) -> Mat3 {
        Mat3 { c: [v3(d.x, 0.0, 0.0), v3(0.0, d.y, 0.0), v3(0.0, 0.0, d.z)] }
    }

    pub fn from_quat(q: Quat) -> Mat3 {
        let Quat { x, y, z, w } = q;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        Mat3 {
            c: [
                v3(1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy)),
                v3(2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx)),
                v3(2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)),
            ],
        }
    }

    /// Element at row `r`, column `c`.
    #[inline]
    pub fn at(&self, r: usize, c: usize) -> f32 {
        get(self.c[c], r)
    }

    #[inline]
    pub fn mul_vec(&self, v: Vec3) -> Vec3 {
        self.c[0] * v.x + self.c[1] * v.y + self.c[2] * v.z
    }

    /// `selfᵀ · v`.
    #[inline]
    pub fn mul_vec_t(&self, v: Vec3) -> Vec3 {
        v3(self.c[0].dot(v), self.c[1].dot(v), self.c[2].dot(v))
    }

    pub fn transpose(&self) -> Mat3 {
        let m = self;
        Mat3 {
            c: [
                v3(m.c[0].x, m.c[1].x, m.c[2].x),
                v3(m.c[0].y, m.c[1].y, m.c[2].y),
                v3(m.c[0].z, m.c[1].z, m.c[2].z),
            ],
        }
    }

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        Mat3 { c: [self.mul_vec(o.c[0]), self.mul_vec(o.c[1]), self.mul_vec(o.c[2])] }
    }

    pub fn add(&self, o: &Mat3) -> Mat3 {
        Mat3 { c: [self.c[0] + o.c[0], self.c[1] + o.c[1], self.c[2] + o.c[2]] }
    }

    pub fn scale(&self, s: f32) -> Mat3 {
        Mat3 { c: [self.c[0] * s, self.c[1] * s, self.c[2] * s] }
    }

    pub fn det(&self) -> f32 {
        self.c[0].dot(self.c[1].cross(self.c[2]))
    }

    /// Inverse, or zero when singular (a body with no rotational freedom).
    pub fn inverse(&self) -> Mat3 {
        let (a, b, c) = (self.c[0], self.c[1], self.c[2]);
        let r0 = b.cross(c);
        let r1 = c.cross(a);
        let r2 = a.cross(b);
        let det = a.dot(r0);
        if det.abs() <= 1.0e-30 {
            return Mat3::ZERO;
        }
        let inv = 1.0 / det;
        // The rows of the inverse are r0, r1, r2 (scaled): transpose into columns.
        Mat3 { c: [v3(r0.x, r1.x, r2.x) * inv, v3(r0.y, r1.y, r2.y) * inv, v3(r0.z, r1.z, r2.z) * inv] }
    }

    /// `R · self · Rᵀ`: an inertia tensor turned into the frame `R` maps into.
    pub fn rotated(&self, r: &Mat3) -> Mat3 {
        r.mul(self).mul(&r.transpose())
    }
}

/// A rigid transform: rotate by `q`, then translate by `p`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub p: Vec3,
    pub q: Quat,
}

impl Default for Pose {
    fn default() -> Pose {
        Pose::IDENTITY
    }
}

impl Pose {
    pub const IDENTITY: Pose = Pose { p: Vec3::ZERO, q: Quat::IDENTITY };

    pub fn new(p: Vec3, q: Quat) -> Pose {
        Pose { p, q }
    }

    pub fn at(p: Vec3) -> Pose {
        Pose { p, q: Quat::IDENTITY }
    }

    #[inline]
    pub fn point(&self, v: Vec3) -> Vec3 {
        self.q.rotate(v) + self.p
    }

    #[inline]
    pub fn vector(&self, v: Vec3) -> Vec3 {
        self.q.rotate(v)
    }

    #[inline]
    pub fn inv_point(&self, v: Vec3) -> Vec3 {
        self.q.conjugate().rotate(v - self.p)
    }

    #[inline]
    pub fn inv_vector(&self, v: Vec3) -> Vec3 {
        self.q.conjugate().rotate(v)
    }

    /// `self ∘ o`: apply `o` first.
    pub fn mul(&self, o: &Pose) -> Pose {
        Pose { p: self.point(o.p), q: (self.q * o.q).normalize() }
    }

    pub fn inverse(&self) -> Pose {
        let q = self.q.conjugate();
        Pose { p: -q.rotate(self.p), q }
    }

    /// `self⁻¹ ∘ o`: `o` in this pose's frame.
    pub fn inv_mul(&self, o: &Pose) -> Pose {
        let qi = self.q.conjugate();
        Pose { p: qi.rotate(o.p - self.p), q: (qi * o.q).normalize() }
    }
}

/// `q` advanced by angular velocity `w` (rad/s, world frame) over `h` seconds, renormalised.
pub fn integrate_rotation(q: Quat, w: Vec3, h: f32) -> Quat {
    let hw = w * (0.5 * h);
    let dq = Quat { x: hw.x, y: hw.y, z: hw.z, w: 0.0 } * q;
    Quat { x: q.x + dq.x, y: q.y + dq.y, z: q.z + dq.z, w: q.w + dq.w }.normalize()
}

pub fn quat_dot(a: Quat, b: Quat) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w
}

/// Euler angles in degrees (X, then Y, then Z extrinsic: `q = qz · qy · qx`, the world's
/// `rotation_deg` convention) of a unit quaternion, through `dmath` (deterministic).
pub fn quat_to_euler_deg(q: Quat) -> Vec3 {
    let Quat { x, y, z, w } = q;
    // Row-major elements of R = Rz · Ry · Rx.
    let r20 = 2.0 * (x * z - w * y);
    let r21 = 2.0 * (y * z + w * x);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    let r10 = 2.0 * (x * y + w * z);
    let r00 = 1.0 - 2.0 * (y * y + z * z);
    let s = (-r20).clamp(-1.0, 1.0);
    let deg = 180.0 / std::f32::consts::PI;
    if s.abs() > 0.999_999 {
        // Gimbal lock: put all of the remaining roll into X.
        let r01 = 2.0 * (x * y - w * z);
        let r02 = 2.0 * (x * z + w * y);
        let beta = if s > 0.0 { 90.0 } else { -90.0 };
        let alpha = if s > 0.0 { dmath::atan2(r01, r02) } else { dmath::atan2(-r01, -r02) };
        return v3(alpha * deg, beta, 0.0);
    }
    let alpha = dmath::atan2(r21, r22);
    let beta = dmath::asin(s);
    let gamma = dmath::atan2(r10, r00);
    v3(alpha * deg, beta * deg, gamma * deg)
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: Vec3 { x: f32::MAX, y: f32::MAX, z: f32::MAX }, max: Vec3 { x: -f32::MAX, y: -f32::MAX, z: -f32::MAX } };

    pub fn new(min: Vec3, max: Vec3) -> Aabb {
        Aabb { min, max }
    }

    pub fn around(c: Vec3, half: Vec3) -> Aabb {
        Aabb { min: c - half, max: c + half }
    }

    pub fn of_points(points: impl IntoIterator<Item = Vec3>) -> Aabb {
        let mut b = Aabb::EMPTY;
        for p in points {
            b.min = b.min.min(p);
            b.max = b.max.max(p);
        }
        b
    }

    #[inline]
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }

    #[inline]
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && self.max.x >= o.min.x
            && self.min.y <= o.max.y
            && self.max.y >= o.min.y
            && self.min.z <= o.max.z
            && self.max.z >= o.min.z
    }

    #[inline]
    pub fn contains(&self, o: &Aabb) -> bool {
        self.min.x <= o.min.x
            && self.min.y <= o.min.y
            && self.min.z <= o.min.z
            && self.max.x >= o.max.x
            && self.max.y >= o.max.y
            && self.max.z >= o.max.z
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        p.x >= self.min.x && p.y >= self.min.y && p.z >= self.min.z && p.x <= self.max.x && p.y <= self.max.y && p.z <= self.max.z
    }

    #[inline]
    pub fn expand(&self, m: f32) -> Aabb {
        Aabb { min: self.min - splat(m), max: self.max + splat(m) }
    }

    /// Grown to include its own copy moved by `d` (a swept box).
    pub fn swept(&self, d: Vec3) -> Aabb {
        Aabb { min: self.min.min(self.min + d), max: self.max.max(self.max + d) }
    }

    /// Half the surface area (the tree's cost metric).
    #[inline]
    pub fn half_area(&self) -> f32 {
        let d = self.max - self.min;
        d.x * d.y + d.y * d.z + d.z * d.x
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    pub fn is_valid(&self) -> bool {
        is_finite(self.min) && is_finite(self.max) && self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    /// Entry and exit fractions of the ray `origin + t·dir` (`t` in `[0, max_t]`), if it hits.
    pub fn ray(&self, origin: Vec3, inv_dir: Vec3, max_t: f32) -> Option<(f32, f32)> {
        let mut t0 = 0.0f32;
        let mut t1 = max_t;
        for i in 0..3 {
            let (o, inv, lo, hi) = (get(origin, i), get(inv_dir, i), get(self.min, i), get(self.max, i));
            if inv.is_infinite() {
                // Parallel to the slab.
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let (mut a, mut b) = ((lo - o) * inv, (hi - o) * inv);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            t0 = t0.max(a);
            t1 = t1.min(b);
            if t0 > t1 {
                return None;
            }
        }
        Some((t0, t1))
    }
}

/// `1 / d` per component, with exact ±infinity for zero components (for slab tests).
pub fn inv_dir(d: Vec3) -> Vec3 {
    let inv = |c: f32| if c == 0.0 { f32::INFINITY } else { 1.0 / c };
    v3(inv(d.x), inv(d.y), inv(d.z))
}

/// Closest point to `p` on segment `a`–`b`, and its parameter.
pub fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> (Vec3, f32) {
    let ab = b - a;
    let l2 = len_sq(ab);
    if l2 <= TINY {
        return (a, 0.0);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (a + ab * t, t)
}

/// Closest points between segments `p1`–`q1` and `p2`–`q2` (Ericson 5.1.9): `(c1, c2, s, t)`.
pub fn closest_segments(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (Vec3, Vec3, f32, f32) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let eps = 1.0e-12;
    let (s, t);
    if a <= eps && e <= eps {
        return (p1, p2, 0.0, 0.0);
    }
    if a <= eps {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= eps {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > eps * a * e { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    (p1 + d1 * s, p2 + d2 * t, s, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).length() <= eps
    }

    #[test]
    fn euler_round_trips_through_quaternions() {
        for e in [v3(0.0, 0.0, 0.0), v3(30.0, -45.0, 60.0), v3(-170.0, 10.0, 95.0), v3(5.0, 89.0, 0.0)] {
            let q = Quat::from_euler_xyz_deg(e.x, e.y, e.z);
            let back = quat_to_euler_deg(q);
            let q2 = Quat::from_euler_xyz_deg(back.x, back.y, back.z);
            assert!(quat_dot(q, q2).abs() > 0.99999, "{e:?} -> {back:?}");
        }
        // Gimbal lock still gives the same rotation.
        let q = Quat::from_euler_xyz_deg(20.0, 90.0, 0.0);
        let back = quat_to_euler_deg(q);
        let q2 = Quat::from_euler_xyz_deg(back.x, back.y, back.z);
        assert!(quat_dot(q, q2).abs() > 0.9999, "{back:?}");
    }

    #[test]
    fn matrices_invert_and_match_quaternions() {
        let q = Quat::from_euler_xyz_deg(10.0, 20.0, 30.0);
        let m = Mat3::from_quat(q);
        let v = v3(1.0, 2.0, 3.0);
        assert!(close(m.mul_vec(v), q.rotate(v), 1e-5));
        let a = Mat3 { c: [v3(2.0, 0.5, 0.0), v3(0.5, 3.0, 0.1), v3(0.0, 0.1, 4.0)] };
        let i = a.mul(&a.inverse());
        for c in 0..3 {
            for r in 0..3 {
                assert!((i.at(r, c) - if r == c { 1.0 } else { 0.0 }).abs() < 1e-5);
            }
        }
        assert_eq!(Mat3::ZERO.inverse(), Mat3::ZERO);
    }

    #[test]
    fn poses_compose_and_invert() {
        let a = Pose::new(v3(1.0, 2.0, 3.0), Quat::from_euler_xyz_deg(0.0, 90.0, 0.0));
        let b = Pose::new(v3(0.5, 0.0, 0.0), Quat::from_euler_xyz_deg(45.0, 0.0, 0.0));
        let p = v3(0.3, -0.2, 0.9);
        assert!(close(a.mul(&b).point(p), a.point(b.point(p)), 1e-5));
        assert!(close(a.inverse().point(a.point(p)), p, 1e-5));
        assert!(close(a.inv_mul(&a.mul(&b)).p, b.p, 1e-5));
    }

    #[test]
    fn segments_and_boxes() {
        let (c1, c2, _, _) = closest_segments(v3(0.0, 0.0, 0.0), v3(2.0, 0.0, 0.0), v3(1.0, 1.0, -1.0), v3(1.0, 1.0, 1.0));
        assert!(close(c1, v3(1.0, 0.0, 0.0), 1e-6) && close(c2, v3(1.0, 1.0, 0.0), 1e-6));
        let b = Aabb::new(v3(-1.0, -1.0, -1.0), v3(1.0, 1.0, 1.0));
        let d = v3(1.0, 0.0, 0.0);
        let (t0, t1) = b.ray(v3(-3.0, 0.0, 0.0), inv_dir(d), 10.0).unwrap();
        assert_eq!((t0, t1), (2.0, 4.0));
        assert!(b.ray(v3(-3.0, 2.0, 0.0), inv_dir(d), 10.0).is_none());
        let (t1v, t2v) = basis(Vec3::Y);
        assert!(t1v.dot(Vec3::Y).abs() < 1e-6 && t2v.dot(Vec3::Y).abs() < 1e-6 && t1v.dot(t2v).abs() < 1e-6);
    }
}
