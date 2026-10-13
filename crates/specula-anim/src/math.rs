//! Quaternion and transform helpers on plain arrays (deterministic: IEEE basic operations,
//! `sqrt`, and `specula_core::dmath` for trigonometry; foundation F2).
//!
//! Quaternions are `[x, y, z, w]` (unit, `w` scalar), composed like `specula_core::math::Quat`:
//! `qmul(a, b)` applies `b` first. Matrices are [`Affine`] (three columns + translation).

use specula_core::dmath;
pub use specula_model::math3::{self, Affine, V3};

pub type Q = [f32; 4];

pub const QID: Q = [0.0, 0.0, 0.0, 1.0];

/// A bone's local transform: `p' = t + r · (s ∘ p)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub t: V3,
    pub r: Q,
    pub s: V3,
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform { t: [0.0; 3], r: QID, s: [1.0; 3] };

    /// The transform as an affine matrix (translate × rotate × scale).
    #[inline]
    pub fn to_affine(&self) -> Affine {
        let c = qcols(self.r);
        Affine { cols: [math3::scale(c[0], self.s[0]), math3::scale(c[1], self.s[1]), math3::scale(c[2], self.s[2])], t: self.t }
    }
}

#[inline]
pub fn qmul(a: Q, b: Q) -> Q {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

#[inline]
pub fn qdot(a: Q, b: Q) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

#[inline]
pub fn qconj(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}

#[inline]
pub fn qneg(q: Q) -> Q {
    [-q[0], -q[1], -q[2], -q[3]]
}

/// Unit-length copy; a (near) zero quaternion becomes the identity.
#[inline]
pub fn qnorm(q: Q) -> Q {
    let l2 = qdot(q, q);
    if l2 <= 1e-24 {
        return QID;
    }
    let k = 1.0 / l2.sqrt();
    [q[0] * k, q[1] * k, q[2] * k, q[3] * k]
}

/// `b` flipped into `a`'s hemisphere (same rotation, shortest path from `a`).
#[inline]
pub fn qalign(a: Q, b: Q) -> Q {
    if qdot(a, b) < 0.0 {
        qneg(b)
    } else {
        b
    }
}

/// Normalised linear interpolation along the shortest path. `w = 0` returns `a` exactly.
#[inline]
pub fn qnlerp(a: Q, b: Q, w: f32) -> Q {
    if w == 0.0 {
        return a;
    }
    let b = qalign(a, b);
    qnorm([a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w, a[2] + (b[2] - a[2]) * w, a[3] + (b[3] - a[3]) * w])
}

/// Rotation of `deg` degrees about a unit `axis`.
pub fn qaxis(axis: V3, deg: f32) -> Q {
    let (s, c) = dmath::sin_cos(deg.to_radians() * 0.5);
    [axis[0] * s, axis[1] * s, axis[2] * s, c]
}

/// Euler degrees, applied X, then Y, then Z (extrinsic; `specula_core::math` convention).
pub fn qeuler(d: V3) -> Q {
    let qx = qaxis([1.0, 0.0, 0.0], d[0]);
    let qy = qaxis([0.0, 1.0, 0.0], d[1]);
    let qz = qaxis([0.0, 0.0, 1.0], d[2]);
    qmul(qz, qmul(qy, qx))
}

/// Rotates `v` by unit `q`.
#[inline]
pub fn qrot(q: Q, v: V3) -> V3 {
    let u = [q[0], q[1], q[2]];
    let t = math3::scale(math3::cross(u, v), 2.0);
    math3::add(math3::add(v, math3::scale(t, q[3])), math3::cross(u, t))
}

/// The rotation matrix columns of unit `q`.
#[inline]
pub fn qcols(q: Q) -> [V3; 3] {
    let [x, y, z, w] = q;
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy)],
        [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx)],
        [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)],
    ]
}

/// The unit quaternion of a rotation matrix given by its columns (Shepperd's method: picks the
/// largest diagonal term, so it's stable for every rotation). `w ≥ 0` in the result.
pub fn qfrom_cols(c: [V3; 3]) -> Q {
    let (m00, m11, m22) = (c[0][0], c[1][1], c[2][2]);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(c[1][2] - c[2][1]) / s, (c[2][0] - c[0][2]) / s, (c[0][1] - c[1][0]) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (c[1][0] + c[0][1]) / s, (c[2][0] + c[0][2]) / s, (c[1][2] - c[2][1]) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(c[1][0] + c[0][1]) / s, 0.25 * s, (c[2][1] + c[1][2]) / s, (c[2][0] - c[0][2]) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(c[2][0] + c[0][2]) / s, (c[2][1] + c[1][2]) / s, 0.25 * s, (c[0][1] - c[1][0]) / s]
    };
    let q = qnorm(q);
    if q[3] < 0.0 {
        qneg(q)
    } else {
        q
    }
}

/// The rotation taking orthonormal frame (`a`, `n`) to (`b`, `m`): `a → b` and `n → m`. Inputs
/// need not be unit or exactly perpendicular (`n` is orthogonalised against `a`).
pub fn qframe(a: V3, n: V3, b: V3, m: V3) -> Q {
    let frame = |a: V3, n: V3| -> [V3; 3] {
        let a = math3::normalize(a);
        let n = math3::normalize(math3::sub(n, math3::scale(a, math3::dot(n, a))));
        [a, n, math3::cross(a, n)]
    };
    let f0 = frame(a, n);
    let f1 = frame(b, m);
    // R = F1 · F0ᵀ: column j of R = Σ_k F1[k] · F0[k][j].
    let mut cols = [[0.0f32; 3]; 3];
    for (j, col) in cols.iter_mut().enumerate() {
        for k in 0..3 {
            *col = math3::add(*col, math3::scale(f1[k], f0[k][j]));
        }
    }
    qfrom_cols(cols)
}

/// Inverse of an affine transform (`None` if singular).
pub fn inverse(m: &Affine) -> Option<Affine> {
    let [a, b, c] = m.cols;
    let det = math3::dot(a, math3::cross(b, c));
    if det.abs() < 1e-20 || !det.is_finite() {
        return None;
    }
    let inv = 1.0 / det;
    // Rows of the inverse are the cross products of the columns, over det.
    let r0 = math3::scale(math3::cross(b, c), inv);
    let r1 = math3::scale(math3::cross(c, a), inv);
    let r2 = math3::scale(math3::cross(a, b), inv);
    let cols = [[r0[0], r1[0], r2[0]], [r0[1], r1[1], r2[1]], [r0[2], r1[2], r2[2]]];
    let t = m.t;
    let it = [-math3::dot(r0, t), -math3::dot(r1, t), -math3::dot(r2, t)];
    Some(Affine { cols, t: it })
}

/// `a ∘ b` (apply `b` first).
#[inline]
pub fn compose(a: &Affine, b: &Affine) -> Affine {
    a.then_after(b)
}

/// Smooth 0→1 over 0..1 (cubic Hermite, zero slope at both ends).
#[inline]
pub fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// sin(2π·x), deterministic.
#[inline]
pub fn sin_turns(x: f32) -> f32 {
    dmath::sin(x * std::f32::consts::TAU)
}

/// cos(2π·x), deterministic.
#[inline]
pub fn cos_turns(x: f32) -> f32 {
    dmath::cos(x * std::f32::consts::TAU)
}

/// Fractional part in [0, 1).
#[inline]
pub fn frac(x: f32) -> f32 {
    let f = x - dmath::floor(x);
    if f >= 1.0 {
        0.0
    } else {
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32], tol: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
    }

    #[test]
    fn euler_matches_core_quat() {
        let q = qeuler([30.0, -50.0, 70.0]);
        let c = specula_core::math::Quat::from_euler_xyz_deg(30.0, -50.0, 70.0);
        assert!(close(&q, &[c.x, c.y, c.z, c.w], 1e-6));
        // +90° about X takes +Y to +Z (right-handed).
        assert!(close(&qrot(qaxis([1.0, 0.0, 0.0], 90.0), [0.0, 1.0, 0.0]), &[0.0, 0.0, 1.0], 1e-6));
    }

    #[test]
    fn matrix_round_trip_and_frames() {
        for d in [[0.0, 0.0, 0.0], [170.0, 10.0, -20.0], [0.0, 180.0, 0.0], [90.0, 90.0, 90.0], [-120.0, 45.0, 179.0]] {
            let q = qeuler(d);
            let back = qfrom_cols(qcols(q));
            assert!(qdot(q, back).abs() > 1.0 - 1e-6, "{d:?}: {q:?} vs {back:?}");
        }
        let q = qframe([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        assert!(close(&qrot(q, [0.0, -1.0, 0.0]), &[0.0, 0.0, 1.0], 1e-6));
        assert!(close(&qrot(q, [1.0, 0.0, 0.0]), &[1.0, 0.0, 0.0], 1e-6));
    }

    #[test]
    fn nlerp_shortest_path_and_unit() {
        let a = qaxis([0.0, 1.0, 0.0], 10.0);
        let b = qneg(qaxis([0.0, 1.0, 0.0], 30.0)); // same rotation, other hemisphere
        let m = qnlerp(a, b, 0.5);
        assert!((qdot(m, m) - 1.0).abs() < 1e-6);
        assert!(qdot(m, qaxis([0.0, 1.0, 0.0], 20.0)) > 1.0 - 1e-6);
        assert_eq!(qnlerp(a, b, 0.0), a);
    }

    #[test]
    fn affine_inverse() {
        let t = Transform { t: [1.0, 2.0, 3.0], r: qeuler([10.0, 20.0, 30.0]), s: [2.0, 1.0, 0.5] }.to_affine();
        let i = inverse(&t).unwrap();
        let id = compose(&t, &i);
        let p = id.apply_point([0.3, -0.7, 5.0]);
        assert!(close(&p, &[0.3, -0.7, 5.0], 1e-5));
        assert!(inverse(&Affine::scale([0.0, 1.0, 1.0])).is_none());
    }
}
