//! The editor's gizmo maths (docs/design/EDITOR.md, "Gizmos"): screen rays, handle picking in
//! screen space, drag deltas with snapping, and the local transforms a drag writes. Pure functions
//! of a camera, a cursor and transforms (no engine state), so every case is unit-tested.
//!
//! Conventions are the engine's: right-handed, +Y up, column-major matrices, screen pixels from
//! the top left; Euler angles are degrees applied X, then Y, then Z (`q = qz * qy * qx`), as
//! `world_set rotation_deg` takes them. Trigonometry goes through `dmath` (rule 6).

use crate::sight::{Camera, Projection};
use specula_core::dmath;
use specula_core::math::{Mat4, Quat, Vec3};

/// The gizmo's on-screen size: its axes are this many pixels long at any distance.
pub const GIZMO_PX: f32 = 96.0;
/// How near (pixels) the cursor must be to an axis, ring or handle to grab it.
pub const GRAB_PX: f32 = 9.0;
/// Segments of a drawn (and picked) rotation ring.
pub const RING_SEGMENTS: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Translate,
    Rotate,
    Scale,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Translate => "translate",
            Mode::Rotate => "rotate",
            Mode::Scale => "scale",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Space {
    #[default]
    World,
    Local,
}

impl Space {
    pub fn name(self) -> &'static str {
        match self {
            Space::World => "world",
            Space::Local => "local",
        }
    }
}

/// A part of the gizmo the cursor can grab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Along one axis (0 = X, 1 = Y, 2 = Z): translate or scale.
    Axis(usize),
    /// In the plane whose normal is this axis (translate).
    Plane(usize),
    /// Around this axis (rotate).
    Ring(usize),
    /// Uniform scale (the centre handle).
    Uniform,
}

impl Handle {
    pub fn name(self) -> String {
        const AX: [&str; 3] = ["x", "y", "z"];
        match self {
            Handle::Axis(i) => format!("axis_{}", AX[i % 3]),
            Handle::Plane(i) => format!("plane_{}", ["yz", "xz", "xy"][i % 3]),
            Handle::Ring(i) => format!("ring_{}", AX[i % 3]),
            Handle::Uniform => "uniform".into(),
        }
    }
}

/// Snapping steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub on: bool,
    /// Metres.
    pub translate: f32,
    /// Degrees.
    pub rotate: f32,
    /// Scale units.
    pub scale: f32,
}

impl Default for Snap {
    fn default() -> Snap {
        Snap { on: true, translate: 0.25, rotate: 15.0, scale: 0.1 }
    }
}

/// `v` to the nearest multiple of `step` (a step of 0 or less leaves it).
pub fn snap_to(v: f32, step: f32) -> f32 {
    if step > 0.0 && v.is_finite() {
        dmath::round(v / step) * step
    } else {
        v
    }
}

/// A ray in world space (`dir` unit length).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

/// The view a gizmo is seen through: the camera, its matrix and the image size.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub camera: Camera,
    pub vp: Mat4,
    pub inv: Mat4,
    pub w: f32,
    pub h: f32,
}

impl View {
    /// `None` for a degenerate camera (no inverse).
    pub fn new(camera: Camera, width: u32, height: u32) -> Option<View> {
        let vp = camera.view_proj(width.max(1), height.max(1));
        let inv = vp.inverse()?;
        Some(View { camera, vp, inv, w: width.max(1) as f32, h: height.max(1) as f32 })
    }

    /// A world point in pixels, `None` behind the camera.
    pub fn project(&self, p: Vec3) -> Option<[f32; 2]> {
        let c = self.vp.mul_vec4(p.extend(1.0));
        if c.w <= 1e-6 || !(c.x.is_finite() && c.y.is_finite()) {
            return None;
        }
        Some([(c.x / c.w + 1.0) * 0.5 * self.w, (c.y / c.w + 1.0) * 0.5 * self.h])
    }

    /// The ray through pixel `p` (from the near plane, away from the camera).
    pub fn ray(&self, p: [f32; 2]) -> Ray {
        let nx = p[0] / self.w * 2.0 - 1.0;
        let ny = p[1] / self.h * 2.0 - 1.0;
        // Reversed Z: depth 1 is the near plane; 0.5 lies in front of it for both projections.
        let unproject = |z: f32| {
            let v = self.inv.mul_vec4(specula_core::math::Vec4::new(nx, ny, z, 1.0));
            Vec3::new(v.x / v.w, v.y / v.w, v.z / v.w)
        };
        let (a, b) = (unproject(1.0), unproject(0.5));
        let dir = (b - a).normalize();
        let fallback = (self.camera.target - self.camera.eye).normalize();
        let ok = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
        if ok(a) && ok(dir) && dir.length() > 0.5 {
            Ray { origin: a, dir }
        } else {
            Ray { origin: self.camera.eye, dir: fallback }
        }
    }

    /// World metres that cover one pixel at `p` (for a gizmo of constant size on screen).
    pub fn metres_per_px(&self, p: Vec3) -> f32 {
        match self.camera.projection {
            Projection::Ortho { height_m } => height_m / self.h,
            Projection::Perspective => {
                let forward = (self.camera.target - self.camera.eye).normalize();
                let depth = (p - self.camera.eye).dot(forward).max(crate::sight::NEAR);
                let half = (self.camera.fov_y_deg.to_radians() * 0.5).clamp(0.01, 1.5);
                2.0 * depth * dmath::tan(half) / self.h
            }
        }
    }
}

/// The gizmo's frame: where it is, its axes, and how long they are in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: Vec3,
    pub axes: [Vec3; 3],
    pub length: f32,
}

impl Frame {
    /// At `origin`, world axes or the rotation `rot`'s (local space).
    pub fn new(view: &View, origin: Vec3, rot: Quat, space: Space) -> Frame {
        let axes = match space {
            Space::World => [Vec3::X, Vec3::Y, Vec3::Z],
            Space::Local => [rot.rotate(Vec3::X).normalize(), rot.rotate(Vec3::Y).normalize(), rot.rotate(Vec3::Z).normalize()],
        };
        Frame { origin, axes, length: view.metres_per_px(origin) * GIZMO_PX }
    }

    /// The other two axes of the plane whose normal is axis `i`.
    pub fn plane_axes(i: usize) -> (usize, usize) {
        ((i + 1) % 3, (i + 2) % 3)
    }

    /// Plane handle `i`'s corners: a square between 0.25 and 0.45 of the axes' length.
    pub fn plane_quad(&self, i: usize) -> [Vec3; 4] {
        let (a, b) = Frame::plane_axes(i);
        let (u, v) = (self.axes[a] * self.length, self.axes[b] * self.length);
        let (lo, hi) = (0.25, 0.45);
        [self.origin + u * lo + v * lo, self.origin + u * hi + v * lo, self.origin + u * hi + v * hi, self.origin + u * lo + v * hi]
    }

    /// Ring `i`'s points (radius 0.85 of the axes' length), closed (the first repeated last).
    pub fn ring(&self, i: usize) -> [Vec3; RING_SEGMENTS + 1] {
        let (a, b) = Frame::plane_axes(i);
        let r = self.length * 0.85;
        std::array::from_fn(|k| {
            let t = k as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
            let (s, c) = dmath::sin_cos(t);
            self.origin + self.axes[a] * (c * r) + self.axes[b] * (s * r)
        })
    }
}

/// Distance from `p` to the segment `a`-`b` (pixels).
pub fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 1e-9 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (a[0] + dx * t - p[0], a[1] + dy * t - p[1]);
    (cx * cx + cy * cy).sqrt()
}

/// Whether `p` is inside the convex quad `q` (either winding).
pub fn in_quad(p: [f32; 2], q: &[[f32; 2]; 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

/// The handle under the cursor, if any (nearest wins; plane and centre handles before axes).
pub fn hit(view: &View, f: &Frame, mode: Mode, cursor: [f32; 2]) -> Option<Handle> {
    let o = view.project(f.origin)?;
    match mode {
        Mode::Translate | Mode::Scale => {
            if mode == Mode::Scale && dist(cursor, o) <= GRAB_PX * 1.4 {
                return Some(Handle::Uniform);
            }
            if mode == Mode::Translate {
                for i in 0..3 {
                    let q = f.plane_quad(i);
                    let pts: Option<Vec<[f32; 2]>> = q.iter().map(|p| view.project(*p)).collect();
                    if let Some(pts) = pts {
                        if in_quad(cursor, &[pts[0], pts[1], pts[2], pts[3]]) {
                            return Some(Handle::Plane(i));
                        }
                    }
                }
            }
            let mut best: Option<(f32, usize)> = None;
            for i in 0..3 {
                let Some(tip) = view.project(f.origin + f.axes[i] * f.length) else { continue };
                // An axis seen end-on (under 6 px long) can't be dragged along.
                if dist(o, tip) < 6.0 {
                    continue;
                }
                let d = segment_distance(cursor, o, tip);
                if d <= GRAB_PX && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
            best.map(|(_, i)| Handle::Axis(i))
        }
        Mode::Rotate => {
            let mut best: Option<(f32, usize)> = None;
            for i in 0..3 {
                let mut last: Option<[f32; 2]> = None;
                for p in f.ring(i) {
                    let s = view.project(p);
                    if let (Some(a), Some(b)) = (last, s) {
                        let d = segment_distance(cursor, a, b);
                        if d <= GRAB_PX && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, i));
                        }
                    }
                    last = s.or(last);
                }
            }
            best.map(|(_, i)| Handle::Ring(i))
        }
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// The parameter along the line `origin + t * axis` closest to `ray` (`None` when they are
/// nearly parallel).
pub fn closest_on_axis(origin: Vec3, axis: Vec3, ray: &Ray) -> Option<f32> {
    let w0 = origin - ray.origin;
    let (a, b, c) = (axis.dot(axis), axis.dot(ray.dir), ray.dir.dot(ray.dir));
    let (d, e) = (axis.dot(w0), ray.dir.dot(w0));
    let denom = a * c - b * b;
    if denom.abs() < 1e-6 * a.max(1e-9) * c.max(1e-9) {
        return None;
    }
    Some((b * e - c * d) / denom)
}

/// Where `ray` meets the plane through `origin` with normal `n` (`None` when parallel or
/// behind the ray's start).
pub fn ray_plane(ray: &Ray, origin: Vec3, n: Vec3) -> Option<Vec3> {
    let denom = n.dot(ray.dir);
    if denom.abs() < 1e-4 {
        return None;
    }
    let t = n.dot(origin - ray.origin) / denom;
    (t >= 0.0).then(|| ray.origin + ray.dir * t)
}

/// What a drag has done so far, in world terms, about the gizmo's origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delta {
    /// A world offset.
    Move(Vec3),
    /// A rotation about the pivot (world).
    Turn(Quat, f32),
    /// Scale factors along the entity's own axes.
    Grow(Vec3),
}

impl Delta {
    pub fn is_identity(&self) -> bool {
        match *self {
            Delta::Move(v) => v.length() < 1e-7,
            Delta::Turn(_, deg) => deg.abs() < 1e-5,
            Delta::Grow(s) => (s - Vec3::ONE).length() < 1e-7,
        }
    }
}

/// A drag in progress: what was grabbed, and where.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub handle: Handle,
    pub frame: Frame,
    pub start_cursor: [f32; 2],
    /// Translate along an axis: the start's parameter on it. Translate in a plane / rotate: the
    /// start's point on the plane.
    start_t: Option<f32>,
    start_hit: Option<Vec3>,
}

impl Drag {
    pub fn begin(view: &View, frame: Frame, handle: Handle, cursor: [f32; 2]) -> Drag {
        let ray = view.ray(cursor);
        let (start_t, start_hit) = match handle {
            Handle::Axis(i) => (closest_on_axis(frame.origin, frame.axes[i], &ray), None),
            Handle::Plane(i) | Handle::Ring(i) => (None, ray_plane(&ray, frame.origin, frame.axes[i])),
            Handle::Uniform => (None, None),
        };
        Drag { handle, frame, start_cursor: cursor, start_t, start_hit }
    }

    /// The drag's delta with the cursor at `cursor`. `snap` (None: off) snaps the amount moved
    /// along an axis (world space translations snap the *result* through [`snap_position`]),
    /// the angle, or the scale factor's result (through [`snap_scale`]).
    pub fn delta(&self, view: &View, mode: Mode, cursor: [f32; 2], snap: Option<&Snap>) -> Delta {
        let f = &self.frame;
        let ray = view.ray(cursor);
        match (mode, self.handle) {
            (Mode::Translate, Handle::Axis(i)) => {
                let (Some(t0), Some(t)) = (self.start_t, closest_on_axis(f.origin, f.axes[i], &ray)) else { return Delta::Move(Vec3::ZERO) };
                // A runaway parameter (an axis pointing almost at the camera) is capped.
                let mut d = (t - t0).clamp(-f.length * 400.0, f.length * 400.0);
                if let Some(s) = snap {
                    d = snap_to(d, s.translate);
                }
                Delta::Move(f.axes[i] * d)
            }
            (Mode::Translate, Handle::Plane(i)) => {
                let (Some(h0), Some(h)) = (self.start_hit, ray_plane(&ray, f.origin, f.axes[i])) else { return Delta::Move(Vec3::ZERO) };
                let (a, b) = Frame::plane_axes(i);
                let d = h - h0;
                let (mut da, mut db) = (d.dot(f.axes[a]), d.dot(f.axes[b]));
                let cap = f.length * 400.0;
                da = da.clamp(-cap, cap);
                db = db.clamp(-cap, cap);
                if let Some(s) = snap {
                    da = snap_to(da, s.translate);
                    db = snap_to(db, s.translate);
                }
                Delta::Move(f.axes[a] * da + f.axes[b] * db)
            }
            (Mode::Rotate, Handle::Ring(i)) => {
                let axis = f.axes[i];
                let mut deg = match (self.start_hit, ray_plane(&ray, f.origin, axis)) {
                    (Some(h0), Some(h)) if (h0 - f.origin).length() > 1e-6 && (h - f.origin).length() > 1e-6 => {
                        let (u, v) = ((h0 - f.origin).normalize(), (h - f.origin).normalize());
                        dmath::atan2(u.cross(v).dot(axis), u.dot(v)).to_degrees()
                    }
                    // The ring seen edge-on: the cursor's angle about the gizmo's centre on screen.
                    _ => screen_angle(view, f.origin, self.start_cursor, cursor, axis),
                };
                if let Some(s) = snap {
                    deg = snap_to(deg, s.rotate);
                }
                Delta::Turn(Quat::from_axis_angle(axis, deg.to_radians()), deg)
            }
            (Mode::Scale, Handle::Axis(i)) => {
                let (Some(o), Some(tip)) = (view.project(f.origin), view.project(f.origin + f.axes[i] * f.length)) else { return Delta::Grow(Vec3::ONE) };
                let (ax, ay) = (tip[0] - o[0], tip[1] - o[1]);
                let len2 = (ax * ax + ay * ay).max(1.0);
                let moved = ((cursor[0] - self.start_cursor[0]) * ax + (cursor[1] - self.start_cursor[1]) * ay) / len2;
                let k = (1.0 + moved).max(0.01);
                let mut s = Vec3::ONE;
                set(&mut s, i, k);
                Delta::Grow(s)
            }
            (Mode::Scale, Handle::Uniform) => {
                let moved = ((cursor[0] - self.start_cursor[0]) - (cursor[1] - self.start_cursor[1])) / (GIZMO_PX * 1.5);
                let k = (1.0 + moved).max(0.01);
                Delta::Grow(Vec3::new(k, k, k))
            }
            // A handle of another mode (the mode changed mid-drag): nothing.
            (Mode::Rotate, _) => Delta::Turn(Quat::IDENTITY, 0.0),
            (Mode::Scale, _) => Delta::Grow(Vec3::ONE),
            (Mode::Translate, _) => Delta::Move(Vec3::ZERO),
        }
    }
}

/// The signed angle (degrees) the cursor swept about `centre` on screen, turned to match the
/// right-hand rule about `axis` as seen from the camera.
fn screen_angle(view: &View, centre: Vec3, from: [f32; 2], to: [f32; 2], axis: Vec3) -> f32 {
    let Some(c) = view.project(centre) else { return 0.0 };
    let a0 = dmath::atan2(from[1] - c[1], from[0] - c[0]);
    let a1 = dmath::atan2(to[1] - c[1], to[0] - c[0]);
    let d = wrap_deg((a1 - a0).to_degrees(), 0.0);
    // Screen y points down: a clockwise sweep on screen is counter-clockwise about an axis that
    // points at the camera.
    let toward = axis.dot(view.camera.eye - centre) > 0.0;
    if toward {
        -d
    } else {
        d
    }
}

fn set(v: &mut Vec3, i: usize, x: f32) {
    match i {
        0 => v.x = x,
        1 => v.y = x,
        _ => v.z = x,
    }
}

fn get(v: Vec3, i: usize) -> f32 {
    match i {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

/// An entity's transform as `world_set` writes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
}

/// The rotation part of a matrix (columns normalised: scale removed).
pub fn rotation_of(m: &Mat4) -> Quat {
    let col = |c: usize| Vec3::new(m.cols[c][0], m.cols[c][1], m.cols[c][2]).normalize();
    let (x, y, z) = (col(0), col(1), col(2));
    // Rows/columns of the rotation matrix R (r[row][col]).
    let r = [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]];
    let trace = r[0][0] + r[1][1] + r[2][2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        Quat { w: 0.25 * s, x: (r[2][1] - r[1][2]) / s, y: (r[0][2] - r[2][0]) / s, z: (r[1][0] - r[0][1]) / s }
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let s = (1.0 + r[0][0] - r[1][1] - r[2][2]).max(1e-12).sqrt() * 2.0;
        Quat { w: (r[2][1] - r[1][2]) / s, x: 0.25 * s, y: (r[0][1] + r[1][0]) / s, z: (r[0][2] + r[2][0]) / s }
    } else if r[1][1] > r[2][2] {
        let s = (1.0 + r[1][1] - r[0][0] - r[2][2]).max(1e-12).sqrt() * 2.0;
        Quat { w: (r[0][2] - r[2][0]) / s, x: (r[0][1] + r[1][0]) / s, y: 0.25 * s, z: (r[1][2] + r[2][1]) / s }
    } else {
        let s = (1.0 + r[2][2] - r[0][0] - r[1][1]).max(1e-12).sqrt() * 2.0;
        Quat { w: (r[1][0] - r[0][1]) / s, x: (r[0][2] + r[2][0]) / s, y: (r[1][2] + r[2][1]) / s, z: 0.25 * s }
    };
    q.normalize()
}

/// Euler degrees (X, then Y, then Z) of `q`, the solution nearest `near` (each angle within
/// 180 degrees of it), so a turn about one axis keeps the angles a human typed.
pub fn euler_xyz_deg(q: Quat, near: Vec3) -> Vec3 {
    // Any finite angle is world data an agent may write (1e10 degrees, say): beyond a few turns
    // `near` is taken modulo 360 first, so the wrap below works in closed form at any size.
    let tame = |n: f32| {
        if n.is_finite() && n.abs() <= MAX_NEAR_DEG {
            n
        } else if n.is_finite() {
            wrap_deg(n, 0.0)
        } else {
            0.0
        }
    };
    let near = Vec3::new(tame(near.x), tame(near.y), tame(near.z));
    let q = q.normalize();
    let Quat { x, y, z, w } = q;
    // R = Rz * Ry * Rx: r20 = -sin(b), r21 = sin(a) cos(b), r22 = cos(a) cos(b),
    // r10 = sin(c) cos(b), r00 = cos(c) cos(b).
    let r00 = 1.0 - 2.0 * (y * y + z * z);
    let r10 = 2.0 * (x * y + w * z);
    let r20 = 2.0 * (x * z - w * y);
    let r21 = 2.0 * (y * z + w * x);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    let r01 = 2.0 * (x * y - w * z);
    let r11 = 1.0 - 2.0 * (x * x + z * z);
    let sb = (-r20).clamp(-1.0, 1.0);
    let candidates: Vec<Vec3> = if sb.abs() > 0.99999 {
        // Gimbal lock: X and Z turn about the same axis; X keeps its value.
        let b = if sb > 0.0 { 90.0 } else { -90.0 };
        let a = near.x;
        // With b = +90: R = Rz(c) Ry(90) Rx(a) depends on c - a; with -90 on c + a.
        let s = dmath::atan2(-r01, r11).to_degrees();
        let c = if sb > 0.0 { s + a } else { s - a };
        vec![Vec3::new(a, b, c)]
    } else {
        let b1 = dmath::asin(sb).to_degrees();
        let b2 = 180.0 - b1;
        let mut out = Vec::with_capacity(2);
        for (b, sign) in [(b1, 1.0f32), (b2, -1.0f32)] {
            let a = dmath::atan2(r21 * sign, r22 * sign).to_degrees();
            let c = dmath::atan2(r10 * sign, r00 * sign).to_degrees();
            out.push(Vec3::new(a, b, c));
        }
        out
    };
    let near_of = |e: Vec3| Vec3::new(wrap_deg(e.x, near.x), wrap_deg(e.y, near.y), wrap_deg(e.z, near.z));
    candidates
        .into_iter()
        .map(near_of)
        .min_by(|a, b| {
            let da = (*a - near).dot(*a - near);
            let db = (*b - near).dot(*b - near);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(near)
}

/// `|near|` above this many degrees is reduced modulo 360 before solving ([`euler_xyz_deg`]).
pub const MAX_NEAR_DEG: f32 = 36_000.0;

/// `v` plus a whole number of turns, within 180 degrees of `n` (closed form, in f64: no loop that
/// a huge angle could keep from ending).
pub fn wrap_deg(v: f32, n: f32) -> f32 {
    let d = f64::from(v) - f64::from(n);
    // rem_euclid is exact (no rounding), so this holds at any size.
    let r = (d + 180.0).rem_euclid(360.0) - 180.0;
    (f64::from(n) + r) as f32
}

/// Rounds a written value: no `0.30000001` in the journal from float noise (0.1 mm, 0.001 deg).
pub fn tidy(v: Vec3, step: f32) -> Vec3 {
    Vec3::new(snap_to(v.x, step), snap_to(v.y, step), snap_to(v.z, step))
}

/// Where a selected entity ends up after `delta` about `pivot`: its new local transform, given
/// its local transform `start`, its world matrix and its parent's world matrix. `snap` with a
/// world-space translation snaps the moved world position's components along the dragged axes
/// to the grid (`axes`: which world axes moved), so things land on grid lines.
pub fn apply(start: &Local, world: &Mat4, parent: &Mat4, pivot: Vec3, delta: &Delta, snap: Option<&Snap>, grid_axes: [bool; 3]) -> Local {
    let parent_inv = parent.inverse().unwrap_or(Mat4::IDENTITY);
    let world_pos = Vec3::new(world.cols[3][0], world.cols[3][1], world.cols[3][2]);
    match *delta {
        Delta::Move(d) => {
            let mut p = world_pos + d;
            if let Some(s) = snap {
                for (i, on) in grid_axes.iter().enumerate() {
                    if *on {
                        let v = snap_to(get(p, i), s.translate);
                        set(&mut p, i, v);
                    }
                }
            }
            Local { position: tidy(parent_inv.transform_point(p), 1e-4), ..*start }
        }
        Delta::Turn(q, _) => {
            let p = pivot + q.rotate(world_pos - pivot);
            let parent_rot = rotation_of(parent);
            let local_rot = Quat::from_euler_xyz_deg(start.rotation_deg.x, start.rotation_deg.y, start.rotation_deg.z);
            let world_rot = parent_rot * local_rot;
            let new_local = parent_rot.conjugate() * (q * world_rot);
            let deg = euler_xyz_deg(new_local, start.rotation_deg);
            Local { position: tidy(parent_inv.transform_point(p), 1e-4), rotation_deg: tidy(deg, 1e-3), ..*start }
        }
        Delta::Grow(k) => {
            let mut s = Vec3::new(start.scale.x * k.x, start.scale.y * k.y, start.scale.z * k.z);
            if let Some(sn) = snap {
                for i in 0..3 {
                    if (get(k, i) - 1.0).abs() > 1e-7 {
                        let v = snap_to(get(s, i), sn.scale);
                        set(&mut s, i, v);
                    }
                }
            }
            // Never zero or negative (world_set refuses them): at least a thousandth.
            let s = Vec3::new(s.x.max(1e-3), s.y.max(1e-3), s.z.max(1e-3));
            Local { scale: tidy(s, 1e-4), ..*start }
        }
    }
}

/// The world axes a world-space translation handle moves along (for grid snapping).
pub fn grid_axes(handle: Handle, space: Space) -> [bool; 3] {
    if space != Space::World {
        return [false; 3];
    }
    match handle {
        Handle::Axis(i) => {
            let mut a = [false; 3];
            a[i % 3] = true;
            a
        }
        Handle::Plane(i) => {
            let mut a = [true; 3];
            a[i % 3] = false;
            a
        }
        _ => [false; 3],
    }
}

/// The world matrix an entity would have after a drag: `delta` applied to `world` about `pivot`
/// (the live preview's draws are moved by `preview * world⁻¹`).
pub fn preview_matrix(world: &Mat4, pivot: Vec3, delta: &Delta) -> Mat4 {
    match *delta {
        Delta::Move(d) => translation(d) * *world,
        Delta::Turn(q, _) => translation(pivot) * Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE) * translation(-pivot) * *world,
        Delta::Grow(k) => *world * Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, k),
    }
}

fn translation(d: Vec3) -> Mat4 {
    Mat4::from_trs(d, Quat::IDENTITY, Vec3::ONE)
}

/// A rectangle from two corners, as `[x0, y0, x1, y1]` with `x0 <= x1`, `y0 <= y1`.
pub fn rect(a: [f32; 2], b: [f32; 2]) -> [f32; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View::new(Camera::new(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, 60.0), 800, 600).unwrap()
    }

    fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).length() <= eps
    }

    #[test]
    fn rays_go_through_their_pixel() {
        let v = view();
        let centre = v.ray([400.0, 300.0]);
        assert!(close(centre.dir, Vec3::new(0.0, 0.0, -1.0), 1e-4), "{:?}", centre.dir);
        // A point projected and its pixel's ray agree.
        let p = Vec3::new(1.5, -0.7, 2.0);
        let px = v.project(p).unwrap();
        let r = v.ray(px);
        let to = (p - r.origin).normalize();
        assert!(close(to, r.dir, 1e-3), "{to:?} {:?}", r.dir);
        // Screen y is down: a point above the centre is above it on screen.
        assert!(v.project(Vec3::new(0.0, 1.0, 0.0)).unwrap()[1] < 300.0);
        assert!(v.project(Vec3::new(0.0, 0.0, 20.0)).is_none(), "behind the camera");
    }

    #[test]
    fn ortho_rays_are_parallel() {
        let mut cam = Camera::new(Vec3::new(0.0, 10.0, 0.01), Vec3::ZERO, 50.0);
        cam.projection = Projection::Ortho { height_m: 20.0 };
        let v = View::new(cam, 400, 400).unwrap();
        let (a, b) = (v.ray([10.0, 10.0]), v.ray([390.0, 200.0]));
        assert!(close(a.dir, b.dir, 1e-3));
        assert!((v.metres_per_px(Vec3::ZERO) - 0.05).abs() < 1e-6);
    }

    #[test]
    fn handles_are_picked_in_screen_space() {
        let v = view();
        let f = Frame::new(&v, Vec3::ZERO, Quat::IDENTITY, Space::World);
        let o = v.project(Vec3::ZERO).unwrap();
        let x_tip = v.project(f.axes[0] * f.length).unwrap();
        assert!((x_tip[0] - o[0] - GIZMO_PX).abs() < 1.0, "the X axis is {GIZMO_PX} px long: {o:?} {x_tip:?}");
        let mid_x = [(o[0] + x_tip[0]) / 2.0, o[1] + 3.0];
        assert_eq!(hit(&v, &f, Mode::Translate, mid_x), Some(Handle::Axis(0)));
        let mid_y = [o[0] + 2.0, o[1] - GIZMO_PX * 0.8];
        assert_eq!(hit(&v, &f, Mode::Translate, mid_y), Some(Handle::Axis(1)));
        // The XY plane square (normal Z) sits between the X and Y axes.
        let in_xy = [o[0] + GIZMO_PX * 0.35, o[1] - GIZMO_PX * 0.35];
        assert_eq!(hit(&v, &f, Mode::Translate, in_xy), Some(Handle::Plane(2)));
        // Z points at the camera (end-on): not grabbable along.
        assert_eq!(hit(&v, &f, Mode::Translate, [o[0] - 30.0, o[1] + 30.0]), None);
        assert_eq!(hit(&v, &f, Mode::Scale, o), Some(Handle::Uniform));
        // The Z ring faces the camera: a circle of 0.85 * the length around the centre (seen at 45
        // degrees, away from the X and Y rings, which are edge-on lines through the centre).
        let r = GIZMO_PX * 0.85 * std::f32::consts::FRAC_1_SQRT_2;
        let on_ring = [o[0] + r, o[1] - r];
        assert_eq!(hit(&v, &f, Mode::Rotate, on_ring), Some(Handle::Ring(2)));
        assert_eq!(hit(&v, &f, Mode::Rotate, [o[0] + 300.0, o[1] + 200.0]), None);
    }

    #[test]
    fn axis_drags_follow_the_cursor_and_snap() {
        let v = view();
        let f = Frame::new(&v, Vec3::ZERO, Quat::IDENTITY, Space::World);
        let o = v.project(Vec3::ZERO).unwrap();
        let start = [o[0] + 40.0, o[1]];
        let d = Drag::begin(&v, f, Handle::Axis(0), start);
        // Moving the cursor to where x = 1.1 projects moves the entity 1.1 m along X.
        let target = v.project(Vec3::new(1.1, 0.0, 0.0)).unwrap();
        let start_x = v.ray(start);
        let t0 = closest_on_axis(Vec3::ZERO, Vec3::X, &start_x).unwrap();
        let cursor = [target[0] + (start[0] - o[0]), target[1]];
        let Delta::Move(m) = d.delta(&v, Mode::Translate, cursor, None) else { panic!() };
        assert!((m.x - 1.1).abs() < 0.01 && m.y.abs() < 1e-4 && m.z.abs() < 1e-4, "{m:?} (t0 {t0})");
        let snap = Snap::default();
        let Delta::Move(m) = d.delta(&v, Mode::Translate, cursor, Some(&snap)) else { panic!() };
        assert!((m.x - 1.0).abs() < 1e-5, "1.1 snaps to 1.0 with 0.25 m steps: {m:?}");
        // A cursor that didn't move moves nothing.
        assert!(d.delta(&v, Mode::Translate, start, Some(&snap)).is_identity());
    }

    #[test]
    fn plane_drags_stay_in_the_plane() {
        let cam = Camera::new(Vec3::new(4.0, 8.0, 6.0), Vec3::ZERO, 55.0);
        let v = View::new(cam, 640, 480).unwrap();
        let f = Frame::new(&v, Vec3::ZERO, Quat::IDENTITY, Space::World);
        let a = v.project(Vec3::new(0.3, 0.0, 0.3) * f.length).unwrap();
        let d = Drag::begin(&v, f, Handle::Plane(1), a);
        let b = v.project(Vec3::new(2.0, 0.0, -1.0) + Vec3::new(0.3, 0.0, 0.3) * f.length).unwrap();
        let Delta::Move(m) = d.delta(&v, Mode::Translate, b, None) else { panic!() };
        assert!(close(m, Vec3::new(2.0, 0.0, -1.0), 0.02), "{m:?}");
    }

    #[test]
    fn rings_turn_by_the_swept_angle() {
        let v = view();
        let f = Frame::new(&v, Vec3::ZERO, Quat::IDENTITY, Space::World);
        let o = v.project(Vec3::ZERO).unwrap();
        let r = GIZMO_PX * 0.85;
        // From the ring's right to its top: a quarter turn counter-clockwise about +Z (toward us).
        let d = Drag::begin(&v, f, Handle::Ring(2), [o[0] + r, o[1]]);
        let Delta::Turn(q, deg) = d.delta(&v, Mode::Rotate, [o[0], o[1] - r], None) else { panic!() };
        assert!((deg - 90.0).abs() < 0.5, "{deg}");
        assert!(close(q.rotate(Vec3::X), Vec3::Y, 0.01));
        let Delta::Turn(_, snapped) = d.delta(&v, Mode::Rotate, [o[0] + r * 0.93, o[1] - r * 0.37], Some(&Snap::default())) else { panic!() };
        assert!((snapped - 15.0).abs() < 1e-4 || (snapped - 30.0).abs() < 1e-4, "snapped to 15 degree steps: {snapped}");
    }

    #[test]
    fn scale_drags_grow_along_their_axis() {
        let v = view();
        let f = Frame::new(&v, Vec3::ZERO, Quat::IDENTITY, Space::Local);
        let o = v.project(Vec3::ZERO).unwrap();
        let d = Drag::begin(&v, f, Handle::Axis(0), [o[0] + GIZMO_PX, o[1]]);
        let Delta::Grow(k) = d.delta(&v, Mode::Scale, [o[0] + GIZMO_PX * 2.0, o[1]], None) else { panic!() };
        assert!((k.x - 2.0).abs() < 0.02 && k.y == 1.0 && k.z == 1.0, "{k:?}");
        let start = Local { position: Vec3::ZERO, rotation_deg: Vec3::ZERO, scale: Vec3::new(0.5, 1.0, 1.0) };
        let out = apply(&start, &Mat4::IDENTITY, &Mat4::IDENTITY, Vec3::ZERO, &Delta::Grow(Vec3::new(2.13, 1.0, 1.0)), Some(&Snap::default()), [false; 3]);
        assert!((out.scale.x - 1.1).abs() < 1e-5 && out.scale.y == 1.0, "{:?}", out.scale);
        let tiny = apply(&start, &Mat4::IDENTITY, &Mat4::IDENTITY, Vec3::ZERO, &Delta::Grow(Vec3::new(0.0, 1.0, 1.0)), None, [false; 3]);
        assert!(tiny.scale.x > 0.0, "never zero: {:?}", tiny.scale);
    }

    #[test]
    fn euler_round_trips_and_stays_near() {
        for e in [Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0), Vec3::new(-45.0, 60.0, 170.0), Vec3::new(0.0, 120.0, 0.0), Vec3::new(30.0, -89.0, 12.0)] {
            let q = Quat::from_euler_xyz_deg(e.x, e.y, e.z);
            let back = euler_xyz_deg(q, e);
            let q2 = Quat::from_euler_xyz_deg(back.x, back.y, back.z);
            let same = (q.x * q2.x + q.y * q2.y + q.z * q2.z + q.w * q2.w).abs();
            assert!(same > 0.9999, "{e:?} -> {back:?}");
            assert!(close(back, e, 0.05), "the angles typed come back: {e:?} -> {back:?}");
        }
        // Gimbal lock keeps X.
        let q = Quat::from_euler_xyz_deg(25.0, 90.0, 40.0);
        let back = euler_xyz_deg(q, Vec3::new(25.0, 90.0, 0.0));
        let q2 = Quat::from_euler_xyz_deg(back.x, back.y, back.z);
        assert!((q.x * q2.x + q.y * q2.y + q.z * q2.z + q.w * q2.w).abs() > 0.9999, "{back:?}");
    }

    #[test]
    fn huge_angles_end_in_closed_form() {
        // An agent may write any finite angle; the editor's turn must still end (lane review).
        for big in [1e9f32, 1e10, 1e30, -3.4e38, f32::MAX] {
            let near = Vec3::new(big, 0.0, -big);
            let q = Quat::from_axis_angle(Vec3::Y, 30f32.to_radians());
            let e = euler_xyz_deg(q, near);
            assert!(e.x.is_finite() && e.y.is_finite() && e.z.is_finite(), "{big}: {e:?}");
            assert!(e.x.abs() <= 540.0 && e.z.abs() <= 540.0, "{big}: {e:?}");
            let q2 = Quat::from_euler_xyz_deg(e.x, e.y, e.z);
            assert!((q.x * q2.x + q.y * q2.y + q.z * q2.z + q.w * q2.w).abs() > 0.999, "{big}: {e:?}");
        }
        assert_eq!(wrap_deg(370.0, 0.0), 10.0);
        assert_eq!(wrap_deg(-190.0, 0.0), 170.0);
        assert_eq!(wrap_deg(10.0, 720.0), 730.0);
        let w = wrap_deg(1e10, 0.0);
        assert!((-180.0..=180.0).contains(&w), "{w}");
        let start = Local { position: Vec3::ZERO, rotation_deg: Vec3::new(1e10, 0.0, 0.0), scale: Vec3::ONE };
        let q = Quat::from_axis_angle(Vec3::Y, 15f32.to_radians());
        let out = apply(&start, &Mat4::IDENTITY, &Mat4::IDENTITY, Vec3::ZERO, &Delta::Turn(q, 15.0), None, [false; 3]);
        assert!(out.rotation_deg.x.abs() <= 540.0, "{:?}", out.rotation_deg);
    }

    #[test]
    fn moves_and_turns_land_in_the_parents_space() {
        // A parent at (10, 0, 0) turned 90 degrees about Y, scaled 2.
        let parent = Mat4::from_trs(Vec3::new(10.0, 0.0, 0.0), Quat::from_euler_xyz_deg(0.0, 90.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let start = Local { position: Vec3::new(1.0, 0.0, 0.0), rotation_deg: Vec3::ZERO, scale: Vec3::ONE };
        let local = Mat4::from_trs(start.position, Quat::IDENTITY, start.scale);
        let world = parent * local;
        let wp = world.transform_point(Vec3::ZERO);
        // Moving +1 m along world X.
        let out = apply(&start, &world, &parent, wp, &Delta::Move(Vec3::new(1.0, 0.0, 0.0)), None, [false; 3]);
        let now = parent.transform_point(out.position);
        assert!(close(now, wp + Vec3::X, 1e-3), "{now:?} vs {:?}", wp + Vec3::X);
        // Snapped to the world grid.
        let out = apply(&start, &world, &parent, wp, &Delta::Move(Vec3::new(0.13, 0.0, 0.0)), Some(&Snap::default()), [true, false, false]);
        let now = parent.transform_point(out.position);
        assert!((now.x - snap_to(wp.x + 0.13, 0.25)).abs() < 1e-3, "{now:?}");
        // A world turn about Y by 90 degrees adds 90 to the local Y here (the parent turns about Y too).
        let q = Quat::from_axis_angle(Vec3::Y, 90f32.to_radians());
        let out = apply(&start, &world, &parent, wp, &Delta::Turn(q, 90.0), None, [false; 3]);
        assert!(close(out.rotation_deg, Vec3::new(0.0, 90.0, 0.0), 0.01), "{:?}", out.rotation_deg);
        // The preview matrix agrees with the written transform.
        let pm = preview_matrix(&world, wp, &Delta::Turn(q, 90.0));
        let written = parent * Mat4::from_trs(out.position, Quat::from_euler_xyz_deg(out.rotation_deg.x, out.rotation_deg.y, out.rotation_deg.z), out.scale);
        for c in 0..4 {
            for r in 0..3 {
                assert!((pm.cols[c][r] - written.cols[c][r]).abs() < 1e-3, "{pm:?} vs {written:?}");
            }
        }
    }

    #[test]
    fn turning_a_group_orbits_its_members_about_the_pivot() {
        let start = Local { position: Vec3::new(2.0, 0.0, 0.0), rotation_deg: Vec3::ZERO, scale: Vec3::ONE };
        let world = Mat4::from_trs(start.position, Quat::IDENTITY, Vec3::ONE);
        let q = Quat::from_axis_angle(Vec3::Y, 180f32.to_radians());
        let out = apply(&start, &world, &Mat4::IDENTITY, Vec3::ZERO, &Delta::Turn(q, 180.0), None, [false; 3]);
        assert!(close(out.position, Vec3::new(-2.0, 0.0, 0.0), 1e-3), "{:?}", out.position);
    }

    #[test]
    fn quads_and_segments() {
        let q = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(in_quad([5.0, 5.0], &q));
        assert!(!in_quad([15.0, 5.0], &q));
        let rev = [q[3], q[2], q[1], q[0]];
        assert!(in_quad([5.0, 5.0], &rev));
        assert!((segment_distance([5.0, 3.0], [0.0, 0.0], [10.0, 0.0]) - 3.0).abs() < 1e-6);
        assert!((segment_distance([-4.0, 3.0], [0.0, 0.0], [10.0, 0.0]) - 5.0).abs() < 1e-6);
        assert_eq!(rect([5.0, 1.0], [2.0, 8.0]), [2.0, 1.0, 5.0, 8.0]);
        assert_eq!(snap_to(0.37, 0.25), 0.25);
        assert_eq!(snap_to(0.38, 0.25), 0.5);
        assert_eq!(snap_to(0.37, 0.0), 0.37);
        assert_eq!(grid_axes(Handle::Plane(1), Space::World), [true, false, true]);
        assert_eq!(grid_axes(Handle::Axis(2), Space::Local), [false; 3]);
    }
}
