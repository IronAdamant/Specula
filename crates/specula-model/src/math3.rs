//! Small vector and affine-transform helpers on plain arrays (the mesh buffers' own types).
//!
//! Only IEEE basic operations and `sqrt` (correctly rounded) are used, plus [`crate::dmath`] for
//! trigonometry, so every result is bit-identical across platforms (F2).

use crate::dmath;

pub type V3 = [f32; 3];
pub type V2 = [f32; 2];

#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
pub fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
#[inline]
pub fn mul(a: V3, b: V3) -> V3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}
#[inline]
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
#[inline]
pub fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}
/// Unit vector, or zero for a (near) zero input.
#[inline]
pub fn normalize(a: V3) -> V3 {
    let l = length(a);
    if l > 1e-20 {
        scale(a, 1.0 / l)
    } else {
        [0.0; 3]
    }
}
#[inline]
pub fn lerp(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
#[inline]
pub fn vmin(a: V3, b: V3) -> V3 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}
#[inline]
pub fn vmax(a: V3, b: V3) -> V3 {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

/// Twice the triangle's area times its unit normal (right-hand rule: CCW seen from the front).
#[inline]
pub fn tri_normal(a: V3, b: V3, c: V3) -> V3 {
    cross(sub(b, a), sub(c, a))
}

/// Any unit vector perpendicular to unit `n`, chosen deterministically.
pub fn perpendicular(n: V3) -> V3 {
    let other = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    normalize(cross(n, other))
}

/// An affine transform `p' = M p + t`, `M` stored as three columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub cols: [V3; 3],
    pub t: V3,
}

impl Default for Affine {
    fn default() -> Self {
        Affine::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine { cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };

    pub fn translate(t: V3) -> Affine {
        Affine { t, ..Affine::IDENTITY }
    }

    pub fn scale(s: V3) -> Affine {
        Affine { cols: [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]]], t: [0.0; 3] }
    }

    /// Rotation about one axis (0 = X, 1 = Y, 2 = Z), right-handed, in degrees.
    pub fn rotate_axis_deg(axis: usize, deg: f64) -> Affine {
        let (s, c) = dmath::sin_cos_deg(deg);
        let (s, c) = (s as f32, c as f32);
        let cols = match axis {
            0 => [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]],
            1 => [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]],
            _ => [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]],
        };
        Affine { cols, t: [0.0; 3] }
    }

    /// Euler rotation in degrees, applied X, then Y, then Z (extrinsic), matching
    /// `specula_core::math::Quat::from_euler_xyz_deg`.
    pub fn rotate_euler_deg(d: [f64; 3]) -> Affine {
        Affine::rotate_axis_deg(2, d[2]).then_after(&Affine::rotate_axis_deg(1, d[1]).then_after(&Affine::rotate_axis_deg(0, d[0])))
    }

    /// Rotation about an arbitrary unit axis through the origin (Rodrigues), in degrees.
    pub fn rotate_about_deg(axis: V3, deg: f64) -> Affine {
        let k = normalize(axis);
        let (s, c) = dmath::sin_cos_deg(deg);
        let (s, c) = (s as f32, c as f32);
        let t = 1.0 - c;
        let (x, y, z) = (k[0], k[1], k[2]);
        Affine {
            cols: [
                [t * x * x + c, t * x * y + s * z, t * x * z - s * y],
                [t * x * y - s * z, t * y * y + c, t * y * z + s * x],
                [t * x * z + s * y, t * y * z - s * x, t * z * z + c],
            ],
            t: [0.0; 3],
        }
    }

    /// A frame: columns are the images of X, Y and Z, then the origin.
    pub fn from_frame(x: V3, y: V3, z: V3, origin: V3) -> Affine {
        Affine { cols: [x, y, z], t: origin }
    }

    #[inline]
    pub fn apply_vector(&self, v: V3) -> V3 {
        add(add(scale(self.cols[0], v[0]), scale(self.cols[1], v[1])), scale(self.cols[2], v[2]))
    }

    #[inline]
    pub fn apply_point(&self, p: V3) -> V3 {
        add(self.apply_vector(p), self.t)
    }

    /// `self ∘ inner`: apply `inner` first, then `self`.
    pub fn then_after(&self, inner: &Affine) -> Affine {
        Affine {
            cols: [self.apply_vector(inner.cols[0]), self.apply_vector(inner.cols[1]), self.apply_vector(inner.cols[2])],
            t: self.apply_point(inner.t),
        }
    }

    pub fn determinant(&self) -> f32 {
        dot(self.cols[0], cross(self.cols[1], self.cols[2]))
    }

    /// Transforms a normal (by the cofactor matrix, then normalises): correct under non-uniform
    /// scale and mirroring.
    pub fn apply_normal(&self, n: V3) -> V3 {
        let [a, b, c] = self.cols;
        // Cofactor matrix columns = cross products of the other two columns.
        let cof = [cross(b, c), cross(c, a), cross(a, b)];
        let out = add(add(scale(cof[0], n[0]), scale(cof[1], n[1])), scale(cof[2], n[2]));
        let s = if self.determinant() < 0.0 { -1.0 } else { 1.0 };
        normalize(scale(out, s))
    }

    /// Column-major 4×4 (GPU order).
    pub fn to_cols_array(&self) -> [f32; 16] {
        let [a, b, c] = self.cols;
        let t = self.t;
        [a[0], a[1], a[2], 0.0, b[0], b[1], b[2], 0.0, c[0], c[1], c[2], 0.0, t[0], t[1], t[2], 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_are_right_handed() {
        // +90° about Y takes +X to -Z.
        let r = Affine::rotate_axis_deg(1, 90.0);
        assert_eq!(r.apply_vector([1.0, 0.0, 0.0]), [0.0, 0.0, -1.0]);
        // +90° about X takes +Y to +Z; about Z takes +X to +Y.
        assert_eq!(Affine::rotate_axis_deg(0, 90.0).apply_vector([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
        assert_eq!(Affine::rotate_axis_deg(2, 90.0).apply_vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
        let q = Affine::rotate_about_deg([0.0, 1.0, 0.0], 90.0).apply_vector([1.0, 0.0, 0.0]);
        assert!(length(sub(q, [0.0, 0.0, -1.0])) < 1e-6);
    }

    #[test]
    fn composition_order() {
        let t = Affine::translate([1.0, 0.0, 0.0]).then_after(&Affine::scale([2.0, 2.0, 2.0]));
        assert_eq!(t.apply_point([1.0, 1.0, 1.0]), [3.0, 2.0, 2.0]);
    }

    #[test]
    fn normals_under_nonuniform_scale_and_mirror() {
        let s = Affine::scale([2.0, 1.0, 1.0]);
        // A 45° normal in XY becomes steeper towards Y when X is stretched.
        let n = s.apply_normal(normalize([1.0, 1.0, 0.0]));
        assert!(n[1] > n[0]);
        let m = Affine::scale([-1.0, 1.0, 1.0]);
        assert_eq!(m.apply_normal([1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
    }
}
