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
        let _ = o;
        todo!("specula-core: Vec3::dot")
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        let _ = o;
        todo!("specula-core: Vec3::cross")
    }

    pub fn length(self) -> f32 {
        todo!("specula-core: Vec3::length")
    }

    /// Returns `self` unchanged if the length is ~0 (no NaNs).
    pub fn normalize(self) -> Vec3 {
        todo!("specula-core: Vec3::normalize")
    }

    /// Component-wise min/max (for bounding boxes).
    pub fn min(self, o: Vec3) -> Vec3 {
        let _ = o;
        todo!("specula-core: Vec3::min")
    }

    pub fn max(self, o: Vec3) -> Vec3 {
        let _ = o;
        todo!("specula-core: Vec3::max")
    }

    pub fn extend(self, w: f32) -> Vec4 {
        Vec4 { x: self.x, y: self.y, z: self.z, w }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        let _ = o;
        todo!("specula-core: Vec3 + Vec3")
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        let _ = o;
        todo!("specula-core: Vec3 - Vec3")
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        let _ = s;
        todo!("specula-core: Vec3 * f32")
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        todo!("specula-core: -Vec3")
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
    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Quat {
        let _ = (axis, radians);
        todo!("specula-core: Quat::from_axis_angle")
    }

    /// Degrees; rotate about world X, then Y, then Z (`qz * qy * qx`).
    pub fn from_euler_xyz_deg(x: f32, y: f32, z: f32) -> Quat {
        let _ = (x, y, z);
        todo!("specula-core: Quat::from_euler_xyz_deg")
    }

    pub fn normalize(self) -> Quat {
        todo!("specula-core: Quat::normalize")
    }

    pub fn rotate(self, v: Vec3) -> Vec3 {
        let _ = v;
        todo!("specula-core: Quat::rotate")
    }
}

impl Mul for Quat {
    type Output = Quat;
    /// `a * b` applies `b` first, then `a`.
    fn mul(self, o: Quat) -> Quat {
        let _ = o;
        todo!("specula-core: Quat * Quat")
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        cols: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
    };

    /// Translation × Rotation × Scale.
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
        let _ = (translation, rotation, scale);
        todo!("specula-core: Mat4::from_trs")
    }

    /// Right-handed view matrix: camera at `eye` looking at `target`.
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let _ = (eye, target, up);
        todo!("specula-core: Mat4::look_at_rh")
    }

    /// Right-handed perspective for Vulkan: view space looks down −Z; output clip has y down,
    /// depth 0 at `near`, 1 at `far`.
    pub fn perspective_vk(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let _ = (fov_y_radians, aspect, near, far);
        todo!("specula-core: Mat4::perspective_vk")
    }

    /// Right-handed orthographic for Vulkan (y down clip, depth 0..1). Visible view-space box is
    /// x ∈ [left, right], y ∈ [bottom, top], z ∈ [−near, −far].
    pub fn orthographic_vk(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        let _ = (left, right, bottom, top, near, far);
        todo!("specula-core: Mat4::orthographic_vk")
    }

    pub fn mul_vec4(&self, v: Vec4) -> Vec4 {
        let _ = v;
        todo!("specula-core: Mat4::mul_vec4")
    }

    /// Affine point transform with perspective divide when w ≠ 1.
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let _ = p;
        todo!("specula-core: Mat4::transform_point")
    }

    /// `None` if the matrix is singular.
    pub fn inverse(&self) -> Option<Mat4> {
        todo!("specula-core: Mat4::inverse")
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
        let _ = o;
        todo!("specula-core: Mat4 * Mat4")
    }
}

/// Project a world point through `view_proj` to screen pixels (origin top-left) for a
/// `width`×`height` image. Returns `(x, y, depth)` with depth in 0..1, or `None` if the point is
/// behind the camera (clip w ≤ 0). Points outside the image are still returned (callers clip).
pub fn project_to_screen(view_proj: &Mat4, p: Vec3, width: u32, height: u32) -> Option<(f32, f32, f32)> {
    let _ = (view_proj, p, width, height);
    todo!("specula-core: project_to_screen")
}
