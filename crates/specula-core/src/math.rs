//! Linear algebra for the engine.
//!
//! # Conventions (every crate relies on these)
//!
//! - **World space:** right-handed, **+Y up**, metres. A camera with identity rotation looks down **−Z**.
//! - **Matrices:** column-major (`cols[c][r]`), column vectors: `p' = M * p`. `to_cols_array` gives the
//!   16 floats in GPU (GLSL `mat4`) order.
//! - **Clip space (Vulkan):** x right, **y down**, depth **0..1**. `perspective_vk` / `orthographic_vk`
//!   already flip Y, so world +Y appears at the top of the image with a plain `gl_Position = M * p`.
//! - **Screen space:** pixels, origin top-left, +x right, +y down.
//! - **Rotations:** quaternions; Euler helpers take degrees and apply X, then Y, then Z (extrinsic,
//!   i.e. `q = qz * qy * qx`).
//! - **Determinism:** trigonometry goes through [`crate::dmath`], never `f32::sin`/`tan` (platform
//!   libm), so the same inputs give bit-identical transforms on every platform.

use crate::dmath;
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// Unit quaternion (x, y, z = vector part, w = scalar part).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// Column-major 4×4 matrix: `cols[c][r]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
    pub const ONE: Vec3 = Vec3 { x: 1.0, y: 1.0, z: 1.0 };
    pub const X: Vec3 = Vec3 { x: 1.0, y: 0.0, z: 0.0 };
    pub const Y: Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };
    pub const Z: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.y * o.z - self.z * o.y, y: self.z * o.x - self.x * o.z, z: self.x * o.y - self.y * o.x }
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Returns `self` unchanged if the length is ~0 (no NaNs).
    pub fn normalize(self) -> Vec3 {
        let len = self.length();
        if len > NORMALIZE_EPSILON {
            self * (1.0 / len)
        } else {
            self
        }
    }

    /// Component-wise min/max (for bounding boxes).
    pub fn min(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x.min(o.x), y: self.y.min(o.y), z: self.z.min(o.z) }
    }

    pub fn max(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x.max(o.x), y: self.y.max(o.y), z: self.z.max(o.z) }
    }

    pub fn extend(self, w: f32) -> Vec4 {
        Vec4 { x: self.x, y: self.y, z: self.z, w }
    }
}

/// Lengths at or below this are treated as zero by the `normalize` helpers.
const NORMALIZE_EPSILON: f32 = 1e-12;

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x + o.x, y: self.y + o.y, z: self.z + o.z }
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3 { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z }
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3 { x: self.x * s, y: self.y * s, z: self.z * s }
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3 { x: -self.x, y: -self.y, z: -self.z }
    }
}

impl Vec4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Vec4 { x, y, z, w }
    }

    pub fn truncate(self) -> Vec3 {
        Vec3 { x: self.x, y: self.y, z: self.z }
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    /// `axis` need not be normalized; `radians` counter-clockwise looking down the axis (right-hand rule).
    /// A zero-length axis gives the identity.
    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Quat {
        let len = axis.length();
        if len <= NORMALIZE_EPSILON {
            return Quat::IDENTITY;
        }
        let (s, c) = dmath::sin_cos(radians * 0.5);
        let v = axis * (s / len);
        Quat { x: v.x, y: v.y, z: v.z, w: c }
    }

    /// Degrees; rotate about world X, then Y, then Z (`qz * qy * qx`).
    pub fn from_euler_xyz_deg(x: f32, y: f32, z: f32) -> Quat {
        let qx = Quat::from_axis_angle(Vec3::X, x.to_radians());
        let qy = Quat::from_axis_angle(Vec3::Y, y.to_radians());
        let qz = Quat::from_axis_angle(Vec3::Z, z.to_radians());
        qz * qy * qx
    }

    /// Unit-length copy; a ~zero quaternion becomes the identity.
    pub fn normalize(self) -> Quat {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if len <= NORMALIZE_EPSILON {
            return Quat::IDENTITY;
        }
        let k = 1.0 / len;
        Quat { x: self.x * k, y: self.y * k, z: self.z * k, w: self.w * k }
    }

    /// Inverse rotation of a unit quaternion.
    pub fn conjugate(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }

    pub fn rotate(self, v: Vec3) -> Vec3 {
        // v' = v + w·t + q×t with t = 2·(q×v), q = vector part.
        let q = Vec3 { x: self.x, y: self.y, z: self.z };
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }
}

impl Mul for Quat {
    type Output = Quat;
    /// `a * b` applies `b` first, then `a`.
    fn mul(self, o: Quat) -> Quat {
        let a = self;
        Quat {
            x: a.w * o.x + a.x * o.w + a.y * o.z - a.z * o.y,
            y: a.w * o.y - a.x * o.z + a.y * o.w + a.z * o.x,
            z: a.w * o.z + a.x * o.y - a.y * o.x + a.z * o.w,
            w: a.w * o.w - a.x * o.x - a.y * o.y - a.z * o.z,
        }
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        cols: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
    };

    /// Translation × Rotation × Scale. `rotation` is expected to be unit length.
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
        let Quat { x, y, z, w } = rotation;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        Mat4 {
            cols: [
                [(1.0 - 2.0 * (yy + zz)) * scale.x, 2.0 * (xy + wz) * scale.x, 2.0 * (xz - wy) * scale.x, 0.0],
                [2.0 * (xy - wz) * scale.y, (1.0 - 2.0 * (xx + zz)) * scale.y, 2.0 * (yz + wx) * scale.y, 0.0],
                [2.0 * (xz + wy) * scale.z, 2.0 * (yz - wx) * scale.z, (1.0 - 2.0 * (xx + yy)) * scale.z, 0.0],
                [translation.x, translation.y, translation.z, 1.0],
            ],
        }
    }

    /// Right-handed view matrix: camera at `eye` looking at `target`.
    /// The camera looks down its local −Z, with `up` projected onto its local +Y.
    ///
    /// When the view direction is (anti)parallel to `up` — e.g. looking straight down with
    /// `up = +Y`, the usual top-down view — `up` gives no roll, so a fallback is used: the world
    /// axis least aligned with the view direction, signed so that looking down along −Y puts −Z
    /// (north) at the top of the image. `eye == target` has no direction; the result then looks
    /// down −Z from `eye` (callers should reject it).
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let dir = target - eye;
        let f = if dir.length() > NORMALIZE_EPSILON { dir.normalize() } else { -Vec3::Z };
        let mut s = f.cross(up);
        if s.length() <= 1e-6 * up.length().max(1.0) {
            s = f.cross(look_at_fallback_up(f));
        }
        let s = s.normalize();
        let u = s.cross(f);
        Mat4 {
            cols: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    /// Right-handed perspective for Vulkan: view space looks down −Z; output clip has y down,
    /// depth 0 at `near`, 1 at `far`.
    pub fn perspective_vk(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / dmath::tan(fov_y_radians * 0.5);
        let range = near - far;
        Mat4 {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, -f, 0.0, 0.0],
                [0.0, 0.0, far / range, -1.0],
                [0.0, 0.0, near * far / range, 0.0],
            ],
        }
    }

    /// Right-handed perspective for Vulkan with **reversed Z and an infinite far plane**: depth is 1
    /// at `near` and falls towards 0 at infinity (render with depth compare GREATER, clear depth 0).
    /// With a float depth buffer this keeps precision roughly uniform from centimetres to
    /// thousands of kilometres, so nothing far away ever pushes the near plane out. Clip y is down.
    pub fn perspective_vk_reverse_z(fov_y_radians: f32, aspect: f32, near: f32) -> Mat4 {
        let f = 1.0 / dmath::tan(fov_y_radians * 0.5);
        Mat4 {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, -f, 0.0, 0.0],
                [0.0, 0.0, 0.0, -1.0],
                [0.0, 0.0, near, 0.0],
            ],
        }
    }

    /// Right-handed orthographic for Vulkan (y down clip, depth 0..1). Visible view-space box is
    /// x ∈ [left, right], y ∈ [bottom, top], z ∈ [−near, −far].
    pub fn orthographic_vk(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        let (w, h, d) = (right - left, top - bottom, far - near);
        Mat4 {
            cols: [
                [2.0 / w, 0.0, 0.0, 0.0],
                [0.0, -2.0 / h, 0.0, 0.0],
                [0.0, 0.0, -1.0 / d, 0.0],
                [-(right + left) / w, (top + bottom) / h, -near / d, 1.0],
            ],
        }
    }

    /// [`Mat4::orthographic_vk`] with **reversed Z**: depth 1 at `near`, 0 at `far` (render with
    /// depth compare GREATER, clear depth 0, like [`Mat4::perspective_vk_reverse_z`]).
    pub fn orthographic_vk_reverse_z(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        let (w, h, d) = (right - left, top - bottom, far - near);
        Mat4 {
            cols: [
                [2.0 / w, 0.0, 0.0, 0.0],
                [0.0, -2.0 / h, 0.0, 0.0],
                [0.0, 0.0, 1.0 / d, 0.0],
                [-(right + left) / w, (top + bottom) / h, far / d, 1.0],
            ],
        }
    }

    pub fn mul_vec4(&self, v: Vec4) -> Vec4 {
        let c = &self.cols;
        Vec4 {
            x: c[0][0] * v.x + c[1][0] * v.y + c[2][0] * v.z + c[3][0] * v.w,
            y: c[0][1] * v.x + c[1][1] * v.y + c[2][1] * v.z + c[3][1] * v.w,
            z: c[0][2] * v.x + c[1][2] * v.y + c[2][2] * v.z + c[3][2] * v.w,
            w: c[0][3] * v.x + c[1][3] * v.y + c[2][3] * v.z + c[3][3] * v.w,
        }
    }

    /// Affine point transform with perspective divide when w ≠ 1. A zero w leaves the result
    /// undivided rather than producing infinities.
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let v = self.mul_vec4(p.extend(1.0));
        if v.w != 1.0 && v.w != 0.0 {
            v.truncate() * (1.0 / v.w)
        } else {
            v.truncate()
        }
    }

    /// `None` if the matrix is singular.
    pub fn inverse(&self) -> Option<Mat4> {
        // Cofactor expansion via 2×2 sub-determinants; `m(r, c)` reads row r, column c.
        let m = |r: usize, c: usize| self.cols[c][r];
        let s0 = m(0, 0) * m(1, 1) - m(1, 0) * m(0, 1);
        let s1 = m(0, 0) * m(1, 2) - m(1, 0) * m(0, 2);
        let s2 = m(0, 0) * m(1, 3) - m(1, 0) * m(0, 3);
        let s3 = m(0, 1) * m(1, 2) - m(1, 1) * m(0, 2);
        let s4 = m(0, 1) * m(1, 3) - m(1, 1) * m(0, 3);
        let s5 = m(0, 2) * m(1, 3) - m(1, 2) * m(0, 3);
        let c5 = m(2, 2) * m(3, 3) - m(3, 2) * m(2, 3);
        let c4 = m(2, 1) * m(3, 3) - m(3, 1) * m(2, 3);
        let c3 = m(2, 1) * m(3, 2) - m(3, 1) * m(2, 2);
        let c2 = m(2, 0) * m(3, 3) - m(3, 0) * m(2, 3);
        let c1 = m(2, 0) * m(3, 2) - m(3, 0) * m(2, 2);
        let c0 = m(2, 0) * m(3, 1) - m(3, 0) * m(2, 1);
        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let k = 1.0 / det;
        // Rows of the inverse.
        let inv = [
            [
                (m(1, 1) * c5 - m(1, 2) * c4 + m(1, 3) * c3) * k,
                (-m(0, 1) * c5 + m(0, 2) * c4 - m(0, 3) * c3) * k,
                (m(3, 1) * s5 - m(3, 2) * s4 + m(3, 3) * s3) * k,
                (-m(2, 1) * s5 + m(2, 2) * s4 - m(2, 3) * s3) * k,
            ],
            [
                (-m(1, 0) * c5 + m(1, 2) * c2 - m(1, 3) * c1) * k,
                (m(0, 0) * c5 - m(0, 2) * c2 + m(0, 3) * c1) * k,
                (-m(3, 0) * s5 + m(3, 2) * s2 - m(3, 3) * s1) * k,
                (m(2, 0) * s5 - m(2, 2) * s2 + m(2, 3) * s1) * k,
            ],
            [
                (m(1, 0) * c4 - m(1, 1) * c2 + m(1, 3) * c0) * k,
                (-m(0, 0) * c4 + m(0, 1) * c2 - m(0, 3) * c0) * k,
                (m(3, 0) * s4 - m(3, 1) * s2 + m(3, 3) * s0) * k,
                (-m(2, 0) * s4 + m(2, 1) * s2 - m(2, 3) * s0) * k,
            ],
            [
                (-m(1, 0) * c3 + m(1, 1) * c1 - m(1, 2) * c0) * k,
                (m(0, 0) * c3 - m(0, 1) * c1 + m(0, 2) * c0) * k,
                (-m(3, 0) * s3 + m(3, 1) * s1 - m(3, 2) * s0) * k,
                (m(2, 0) * s3 - m(2, 1) * s1 + m(2, 2) * s0) * k,
            ],
        ];
        let mut out = Mat4 { cols: [[0.0; 4]; 4] };
        for (r, row) in inv.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                out.cols[c][r] = v;
            }
        }
        if out.cols.iter().flatten().all(|v| v.is_finite()) {
            Some(out)
        } else {
            None
        }
    }

    /// 16 floats in GPU (column-major) order.
    pub fn to_cols_array(&self) -> [f32; 16] {
        let mut out = [0.0; 16];
        for c in 0..4 {
            out[c * 4..c * 4 + 4].copy_from_slice(&self.cols[c]);
        }
        out
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    /// `a * b` applies `b` first.
    fn mul(self, o: Mat4) -> Mat4 {
        let mut out = Mat4 { cols: [[0.0; 4]; 4] };
        for (dst, col) in out.cols.iter_mut().zip(o.cols) {
            let v = self.mul_vec4(Vec4 { x: col[0], y: col[1], z: col[2], w: col[3] });
            *dst = [v.x, v.y, v.z, v.w];
        }
        out
    }
}

/// Up vector for [`Mat4::look_at_rh`] when the requested one is parallel to the view direction.
fn look_at_fallback_up(f: Vec3) -> Vec3 {
    if f.y.abs() >= f.x.abs().max(f.z.abs()) {
        // Looking (mostly) down: −Z up on screen; looking up: +Z.
        if f.y < 0.0 {
            -Vec3::Z
        } else {
            Vec3::Z
        }
    } else {
        Vec3::Y
    }
}

/// Project a world point through `view_proj` to screen pixels (origin top-left) for a
/// `width`×`height` image. Returns `(x, y, depth)` with depth in 0..1, or `None` if the point is
/// behind the camera (clip w ≤ 0). Points outside the image are still returned (callers clip).
pub fn project_to_screen(view_proj: &Mat4, p: Vec3, width: u32, height: u32) -> Option<(f32, f32, f32)> {
    let clip = view_proj.mul_vec4(p.extend(1.0));
    if clip.w <= 0.0 || !clip.w.is_finite() {
        return None;
    }
    let inv_w = 1.0 / clip.w;
    let (nx, ny, nz) = (clip.x * inv_w, clip.y * inv_w, clip.z * inv_w);
    // Vulkan viewport transform: NDC −1..1 → 0..size; clip y already points down.
    Some(((nx + 1.0) * 0.5 * width as f32, (ny + 1.0) * 0.5 * height as f32, nz))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= EPS * (1.0 + a.abs().max(b.abs()))
    }

    fn vclose(a: Vec3, b: Vec3) -> bool {
        close(a.x, b.x) && close(a.y, b.y) && close(a.z, b.z)
    }

    fn mclose(a: &Mat4, b: &Mat4) -> bool {
        a.to_cols_array().iter().zip(b.to_cols_array()).all(|(&x, y)| close(x, y))
    }

    fn camera(eye: Vec3, target: Vec3) -> Mat4 {
        Mat4::perspective_vk(60f32.to_radians(), 16.0 / 9.0, 0.1, 100.0) * Mat4::look_at_rh(eye, target, Vec3::Y)
    }

    #[test]
    fn vector_basics() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(-4.0, 0.5, 2.0);
        assert_eq!(a.dot(b), -4.0 + 1.0 + 6.0);
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z); // right-handed
        assert_eq!(a + b - b, a);
        assert_eq!(-a * 2.0, Vec3::new(-2.0, -4.0, -6.0));
        assert_eq!(a.min(b), Vec3::new(-4.0, 0.5, 2.0));
        assert_eq!(a.max(b), Vec3::new(1.0, 2.0, 3.0));
        assert!(close(Vec3::new(3.0, 4.0, 0.0).length(), 5.0));
        assert!(close(a.normalize().length(), 1.0));
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
    }

    #[test]
    fn perspective_depth_range() {
        let (near, far) = (0.1, 100.0);
        let p = Mat4::perspective_vk(1.0, 1.5, near, far);
        let n = p.mul_vec4(Vec4::new(0.0, 0.0, -near, 1.0));
        let f = p.mul_vec4(Vec4::new(0.0, 0.0, -far, 1.0));
        assert!(close(n.z / n.w, 0.0), "near depth {}", n.z / n.w);
        assert!(close(f.z / f.w, 1.0), "far depth {}", f.z / f.w);
        // View-space +Y comes out as clip −y (Vulkan y down).
        let up = p.mul_vec4(Vec4::new(0.0, 1.0, -5.0, 1.0));
        assert!(up.y < 0.0);
    }

    #[test]
    fn orthographic_maps_box_to_clip() {
        let o = Mat4::orthographic_vk(-2.0, 6.0, -1.0, 3.0, 0.5, 10.0);
        // Top-left near corner → clip (−1, −1, 0); bottom-right far corner → (1, 1, 1).
        assert!(vclose(o.transform_point(Vec3::new(-2.0, 3.0, -0.5)), Vec3::new(-1.0, -1.0, 0.0)));
        assert!(vclose(o.transform_point(Vec3::new(6.0, -1.0, -10.0)), Vec3::new(1.0, 1.0, 1.0)));
    }

    #[test]
    fn reversed_orthographic_puts_near_at_one() {
        let o = Mat4::orthographic_vk_reverse_z(-2.0, 6.0, -1.0, 3.0, 0.5, 10.0);
        assert!(vclose(o.transform_point(Vec3::new(-2.0, 3.0, -0.5)), Vec3::new(-1.0, -1.0, 1.0)));
        assert!(vclose(o.transform_point(Vec3::new(6.0, -1.0, -10.0)), Vec3::new(1.0, 1.0, 0.0)));
    }

    #[test]
    fn screen_projection_is_top_left_origin() {
        let vp = camera(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let (w, h) = (1280, 720);
        let (cx, cy, _) = project_to_screen(&vp, Vec3::ZERO, w, h).unwrap();
        assert!(close(cx, 640.0) && close(cy, 360.0));
        let (_, y_up, _) = project_to_screen(&vp, Vec3::new(0.0, 1.0, 0.0), w, h).unwrap();
        assert!(y_up < 360.0, "a point above the view axis must be in the top half (y = {y_up})");
        let (x_right, _, _) = project_to_screen(&vp, Vec3::new(1.0, 0.0, 0.0), w, h).unwrap();
        assert!(x_right > 640.0);
        // Depth grows with distance and stays inside 0..1.
        let (_, _, d_near) = project_to_screen(&vp, Vec3::new(0.0, 0.0, 4.0), w, h).unwrap();
        let (_, _, d_far) = project_to_screen(&vp, Vec3::new(0.0, 0.0, -50.0), w, h).unwrap();
        assert!(0.0 < d_near && d_near < d_far && d_far < 1.0);
        // At or behind the camera.
        assert_eq!(project_to_screen(&vp, Vec3::new(0.0, 0.0, 6.0), w, h), None);
        assert_eq!(project_to_screen(&vp, Vec3::new(0.0, 0.0, 5.0), w, h), None);
    }

    #[test]
    fn reverse_z_depth_is_one_at_near_and_falls_towards_zero() {
        let near = 0.05;
        let p = Mat4::perspective_vk_reverse_z(1.0, 1.5, near);
        let depth = |z: f32| {
            let c = p.mul_vec4(Vec4::new(0.0, 0.0, -z, 1.0));
            c.z / c.w
        };
        assert!(close(depth(near), 1.0));
        assert!(depth(10.0) > depth(1000.0) && depth(1000.0) > depth(1.0e6) && depth(1.0e6) > 0.0);
        // A point 1 mm apart at 1 km still gets a distinct f32 depth (no z-fighting there).
        assert_ne!(depth(1000.0), depth(1000.001));
        // Behind the camera: w ≤ 0.
        assert!(p.mul_vec4(Vec4::new(0.0, 0.0, 1.0, 1.0)).w <= 0.0);
        // Same x/y mapping as the standard projection.
        let s = Mat4::perspective_vk(1.0, 1.5, near, 100.0);
        let (a, b) = (p.mul_vec4(Vec4::new(1.0, 2.0, -5.0, 1.0)), s.mul_vec4(Vec4::new(1.0, 2.0, -5.0, 1.0)));
        assert!(close(a.x / a.w, b.x / b.w) && close(a.y / a.w, b.y / b.w));
    }

    #[test]
    fn look_at_handles_straight_down_up_and_degenerate() {
        let (w, h) = (800, 600);
        // Top-down (the common agent view): +X right, −Z (north) up, nothing NaN.
        let down = camera(Vec3::new(0.0, 10.0, 0.0), Vec3::ZERO);
        assert!(down.to_cols_array().iter().all(|v| v.is_finite()));
        let (cx, cy, _) = project_to_screen(&down, Vec3::ZERO, w, h).unwrap();
        assert!(close(cx, 400.0) && close(cy, 300.0));
        let (ex, _, _) = project_to_screen(&down, Vec3::X, w, h).unwrap();
        let (_, ny, _) = project_to_screen(&down, -Vec3::Z, w, h).unwrap();
        assert!(ex > cx, "+X must be right of centre");
        assert!(ny < cy, "−Z must be above centre");
        // Straight up and eye == target stay finite too.
        let up = camera(Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0));
        assert!(up.to_cols_array().iter().all(|v| v.is_finite()));
        let same = Mat4::look_at_rh(Vec3::ONE, Vec3::ONE, Vec3::Y);
        assert!(same.to_cols_array().iter().all(|v| v.is_finite()));
        // Nearly-but-not-exactly vertical still uses the requested up (no snapping).
        let tilted = Mat4::look_at_rh(Vec3::new(0.0, 10.0, 0.01), Vec3::ZERO, Vec3::Y);
        let (_, fy, _) = project_to_screen(&(Mat4::perspective_vk(1.0, 1.0, 0.1, 100.0) * tilted), Vec3::new(0.0, 0.0, -1.0), w, h).unwrap();
        assert!(fy < 300.0, "slightly tilted top-down keeps north up");
    }

    #[test]
    fn look_at_puts_target_at_screen_centre() {
        let eye = Vec3::new(3.0, 2.0, 7.0);
        let target = Vec3::new(-1.0, 0.5, 2.0);
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        // In view space the target is straight ahead on −Z.
        let t = view.transform_point(target);
        assert!(close(t.x, 0.0) && close(t.y, 0.0), "{t:?}");
        assert!(close(t.z, -(target - eye).length()));
        assert!(vclose(view.transform_point(eye), Vec3::ZERO));
        let (x, y, _) = project_to_screen(&camera(eye, target), target, 800, 600).unwrap();
        assert!(close(x, 400.0) && close(y, 300.0), "({x}, {y})");
    }

    #[test]
    fn quaternion_rotation_conventions() {
        let q = Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2);
        assert!(vclose(q.rotate(Vec3::X), -Vec3::Z), "+X rotated +90° about +Y must be −Z");
        // Unnormalized axis; a zero axis is the identity.
        let q2 = Quat::from_axis_angle(Vec3::new(0.0, 5.0, 0.0), std::f32::consts::FRAC_PI_2);
        assert!(vclose(q2.rotate(Vec3::X), -Vec3::Z));
        assert_eq!(Quat::from_axis_angle(Vec3::ZERO, 1.0), Quat::IDENTITY);
        // +90° about +Z takes +X to +Y; about +X takes +Y to +Z.
        assert!(vclose(Quat::from_euler_xyz_deg(0.0, 0.0, 90.0).rotate(Vec3::X), Vec3::Y));
        assert!(vclose(Quat::from_euler_xyz_deg(90.0, 0.0, 0.0).rotate(Vec3::Y), Vec3::Z));
        // `a * b` applies b first.
        let a = Quat::from_axis_angle(Vec3::X, 0.7);
        let b = Quat::from_axis_angle(Vec3::new(1.0, 2.0, -1.0), 1.3);
        let v = Vec3::new(0.3, -1.2, 2.0);
        assert!(vclose((a * b).rotate(v), a.rotate(b.rotate(v))));
        assert!(vclose(b.conjugate().rotate(b.rotate(v)), v));
        let n = Quat { x: 0.0, y: 0.0, z: 3.0, w: 4.0 }.normalize();
        assert!(close(n.z, 0.6) && close(n.w, 0.8));
    }

    #[test]
    fn euler_order_is_x_then_y_then_z() {
        // X first: +Y → +Z; then Y: +Z → +X. (Y first would leave +Y alone, then X sends it to +Z.)
        let q = Quat::from_euler_xyz_deg(90.0, 90.0, 0.0);
        assert!(vclose(q.rotate(Vec3::Y), Vec3::X));
        let (x, y, z) = (20.0f32, -35.0f32, 70.0f32);
        let qx = Quat::from_axis_angle(Vec3::X, x.to_radians());
        let qy = Quat::from_axis_angle(Vec3::Y, y.to_radians());
        let qz = Quat::from_axis_angle(Vec3::Z, z.to_radians());
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(vclose(Quat::from_euler_xyz_deg(x, y, z).rotate(v), qz.rotate(qy.rotate(qx.rotate(v)))));
    }

    #[test]
    fn from_trs_scales_then_rotates_then_translates() {
        let m = Mat4::from_trs(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2),
            Vec3::new(2.0, 3.0, 4.0),
        );
        // (1,0,0) → scale (2,0,0) → rotate (0,0,−2) → translate (1,2,1).
        assert!(vclose(m.transform_point(Vec3::X), Vec3::new(1.0, 2.0, 1.0)));
        assert!(vclose(m.transform_point(Vec3::Y), Vec3::new(1.0, 5.0, 3.0)));
        assert!(vclose(m.transform_point(Vec3::ZERO), Vec3::new(1.0, 2.0, 3.0)));
        // Agrees with the quaternion for an arbitrary rotation.
        let q = Quat::from_euler_xyz_deg(10.0, 50.0, -30.0);
        let r = Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE);
        let v = Vec3::new(-0.4, 1.1, 0.25);
        assert!(vclose(r.transform_point(v), q.rotate(v)));
        // Translation lives in the last column (GPU-order elements 12..16).
        assert_eq!(&m.to_cols_array()[12..], &[1.0, 2.0, 3.0, 1.0]);
    }

    #[test]
    fn matrix_product_order_and_inverse() {
        let t = Mat4::from_trs(Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let s = Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, Vec3::new(2.0, 2.0, 2.0));
        // (t * s) scales first.
        assert!(vclose((t * s).transform_point(Vec3::X), Vec3::new(7.0, 0.0, 0.0)));
        assert!(mclose(&(Mat4::IDENTITY * t), &t));
        for m in [
            Mat4::from_trs(
                Vec3::new(1.0, -2.0, 3.0),
                Quat::from_euler_xyz_deg(15.0, 30.0, 45.0),
                Vec3::new(0.5, 2.0, 3.0),
            ),
            camera(Vec3::new(3.0, 2.0, 7.0), Vec3::new(-1.0, 0.5, 2.0)),
            Mat4::orthographic_vk(-2.0, 6.0, -1.0, 3.0, 0.5, 10.0),
        ] {
            let inv = m.inverse().expect("invertible");
            assert!(mclose(&(inv * m), &Mat4::IDENTITY), "{:?}", inv * m);
            assert!(mclose(&(m * inv), &Mat4::IDENTITY));
        }
        let singular = Mat4::from_trs(Vec3::ONE, Quat::IDENTITY, Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(singular.inverse(), None);
    }

    #[test]
    fn transform_point_divides_by_w() {
        let p = Mat4::perspective_vk(1.0, 1.0, 1.0, 10.0);
        assert!(close(p.transform_point(Vec3::new(0.0, 0.0, -10.0)).z, 1.0));
        assert!(close(p.transform_point(Vec3::new(0.0, 0.0, -1.0)).z, 0.0));
    }
}
