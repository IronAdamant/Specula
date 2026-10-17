//! Joints and constraints (P4): fixed, hinge, ball (cone and twist limits), distance (rod, rope
//! and spring) and prismatic joints, with limits and motors, solved in the Soft Step solver
//! beside the contacts.
//!
//! **Frames.** A joint holds one frame in each body (`frame_a`, `frame_b`: an anchor point and an
//! orientation in the body's own frame, or in the world when that side is the world). The
//! frame's **x axis** is the joint's axis: a hinge turns about it, a ball joint twists about it
//! (and swings it within a cone), a prismatic joint slides along it. Its y axis is the reference
//! a hinge angle is measured from. At rest the two frames coincide.
//!
//! **Solving.** As in Box2D v3: every substep warm-starts, solves with a soft position bias
//! (joint stiffness in Hz with a damping ratio, from [`crate::Settings`]), and relaxes without
//! it. Anchors and axes are recomputed from each body's motion since the step began (its
//! position and rotation change), so large rotations within a step stay accurate. Point
//! constraints are solved as one 3×3 block (stiff chains), angular locks as a 3×3 block,
//! limits as one-sided rows (with speculation: a joint approaching its limit slows in time
//! instead of overshooting), motors as velocity rows clamped by their force, and drives
//! towards a target position or rotation as soft springs clamped by their force (powered
//! ragdolls).
//!
//! **Determinism (F2).** IEEE `+ - * /` and `sqrt`, `dmath` for angles; joints are visited in key
//! order. Their warm-start impulses and drive targets are part of [`crate::Space::hash`].

use crate::math::{basis, mul, normalize_or, try_normalize, Mat3, Pose, Quat, Vec3};
use crate::solver::{Local, Row, Softness, SolverBody};
use specula_core::dmath;
use std::f32::consts::PI;

/// What a joint allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum JointKind {
    /// No relative motion (welds).
    Fixed,
    /// Turns about the frame's x axis (doors, wheels, knees, elbows).
    Hinge,
    /// Turns any way about the anchor; optional swing cone and twist range (shoulders, hips,
    /// chain links).
    Ball,
    /// Keeps the anchors' distance within `length` (rods, ropes, springs); rotation is free.
    Distance,
    /// Slides along the frame's x axis without turning (pistons, lifts, drawers).
    Prismatic,
}

impl JointKind {
    pub const ALL: [JointKind; 5] = [JointKind::Fixed, JointKind::Hinge, JointKind::Ball, JointKind::Distance, JointKind::Prismatic];

    pub fn name(self) -> &'static str {
        match self {
            JointKind::Fixed => "fixed",
            JointKind::Hinge => "hinge",
            JointKind::Ball => "ball",
            JointKind::Distance => "distance",
            JointKind::Prismatic => "prismatic",
        }
    }

    pub fn parse(s: &str) -> Option<JointKind> {
        JointKind::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// What a joint's motor drives towards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drive {
    /// Hinge: rad/s about its axis; prismatic: m/s along it.
    Speed(f32),
    /// Hinge: angle (rad); prismatic: offset along the axis (m).
    Position(f32),
    /// Ball and fixed joints: frame B's orientation in frame A (a powered ragdoll's pose).
    Rotation(Quat),
}

/// A joint motor: a speed it holds, or a target it springs to, never pushing harder than
/// `max_force` (N for linear, N·m for angular drives).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motor {
    pub drive: Drive,
    pub max_force: f32,
    /// Spring stiffness (Hz) and damping ratio of `Position` and `Rotation` drives.
    pub hertz: f32,
    pub damping_ratio: f32,
}

/// Everything a joint is made of.
#[derive(Clone, Debug, PartialEq)]
pub struct JointDesc {
    pub kind: JointKind,
    /// The bodies (keys); `None` is the world (its frame is then a world pose).
    pub a: Option<u64>,
    pub b: Option<u64>,
    pub frame_a: Pose,
    pub frame_b: Pose,
    /// Hinge: angle range (rad) about the axis; ball: twist range (rad); prismatic: offset
    /// range (m) along the axis.
    pub limit: Option<[f32; 2]>,
    /// Ball: swing half-angles (rad, up to π) about the frame's y and z axes (an elliptical cone).
    pub swing: Option<[f32; 2]>,
    /// Distance: the allowed anchor distance `[min, max]` (m): a rod is `[L, L]`, a rope `[0, L]`.
    pub length: [f32; 2],
    /// Distance: a spring towards `rest_length` inside `length` (Hz, 0 = none) and its damping.
    pub spring_hertz: f32,
    pub spring_damping: f32,
    pub rest_length: f32,
    pub motor: Option<Motor>,
    /// Hinge and ball joints: friction torque (N·m) resisting turning (a ragdoll's joints lose
    /// energy like real ones, so it comes to rest).
    pub friction: f32,
    /// The two bodies still collide with each other (default: they don't).
    pub collide_connected: bool,
}

impl JointDesc {
    pub fn new(kind: JointKind, a: Option<u64>, b: Option<u64>, frame_a: Pose, frame_b: Pose) -> JointDesc {
        JointDesc {
            kind,
            a,
            b,
            frame_a,
            frame_b,
            limit: None,
            swing: None,
            length: [0.0, 0.0],
            spring_hertz: 0.0,
            spring_damping: 0.0,
            rest_length: 0.0,
            motor: None,
            friction: 0.0,
            collide_connected: false,
        }
    }

    /// The bodies it links (`None` sides, the world, left out).
    pub fn bodies(&self) -> impl Iterator<Item = u64> {
        self.a.into_iter().chain(self.b)
    }
}

/// A joint's measured state after the last step it was solved in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointState {
    /// Solved in the last step.
    pub active: bool,
    /// Why not, when it isn't: `missing_body`, `asleep`, `no_dynamic_body`, `cosmetic_link`.
    pub inactive: Option<&'static str>,
    /// Anchor separation the joint should hold at zero (m): the gap between the anchors (fixed,
    /// hinge, ball), off the axis (prismatic), or past `length` (distance).
    pub linear_error: f32,
    /// Misalignment it should hold at zero (rad): total for fixed and prismatic, off the axis
    /// for a hinge; 0 for ball and distance joints.
    pub angular_error: f32,
    /// Hinge angle or ball twist (rad), prismatic offset or distance (m).
    pub position: f32,
    /// Ball: the swing angle (rad).
    pub swing: f32,
    /// How far past a limit (rad for angles, m for offsets and lengths).
    pub limit_excess: f32,
    /// The constraint force and torque of the last step (N, N·m).
    pub force: f32,
    pub torque: f32,
}

/// Accumulated impulses (the warm start).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointImpulses {
    /// Point (or prismatic off-axis) impulse, world.
    pub linear: Vec3,
    /// Angular lock impulse, world.
    pub angular: Vec3,
    pub lower: f32,
    pub upper: f32,
    pub swing: f32,
    /// Motor impulse (scalar drives use `x`).
    pub motor: Vec3,
    /// Distance: the rod or spring row.
    pub spring: f32,
    /// Joint friction (world angular impulse).
    pub friction: Vec3,
}

impl JointImpulses {
    pub(crate) fn hash_into(&self, h: &mut specula_core::hash::StableHasher) {
        for v in [self.linear, self.angular, self.motor, self.friction] {
            h.write_f32(v.x);
            h.write_f32(v.y);
            h.write_f32(v.z);
        }
        for f in [self.lower, self.upper, self.swing, self.spring] {
            h.write_f32(f);
        }
    }
}

/// A joint prepared for one step.
#[derive(Clone, Debug)]
pub struct JointConstraint {
    pub a: u32,
    pub b: u32,
    pub desc: JointDesc,
    /// Anchors from each centre of mass, and frame rotations, world, at the step start.
    pub ra0: Vec3,
    pub rb0: Vec3,
    pub fa0: Quat,
    pub fb0: Quat,
    /// `centre B - centre A` at the step start.
    pub delta_center: Vec3,
    pub soft: Softness,
    pub spring: Softness,
    pub imp: JointImpulses,
    /// The space's joint key (to store impulses back).
    pub key: u64,
}

/// Highest position-correction speeds (m/s, rad/s): joints pulled far apart come back firmly
/// but never explode.
const MAX_LINEAR_PUSH: f32 = 10.0;
const MAX_ANGULAR_PUSH: f32 = 15.0;

/// The rotation vector (axis × angle, rad) of unit quaternion `q`, the short way round.
pub fn rotation_vector(q: Quat) -> Vec3 {
    let q = if q.w < 0.0 { Quat { x: -q.x, y: -q.y, z: -q.z, w: -q.w } } else { q };
    let v = Vec3::new(q.x, q.y, q.z);
    let s = v.length();
    if s < 1.0e-9 {
        return v * 2.0;
    }
    v * (2.0 * dmath::atan2(s, q.w) / s)
}

/// An angle wrapped into `(-π, π]`.
pub fn wrap(a: f32) -> f32 {
    let mut a = a;
    let tau = 2.0 * PI;
    while a > PI {
        a -= tau;
    }
    while a <= -PI {
        a += tau;
    }
    a
}

/// The frame's axes in the world.
#[inline]
fn axes(q: Quat) -> (Vec3, Vec3, Vec3) {
    (q.rotate(Vec3::X), q.rotate(Vec3::Y), q.rotate(Vec3::Z))
}

/// A hinge's angle about `x` (frame A's axis) from A's reference `ya` to B's `yb`.
pub fn hinge_angle(x: Vec3, ya: Vec3, yb: Vec3) -> f32 {
    dmath::atan2(ya.cross(yb).dot(x), ya.dot(yb))
}

/// Twist about the frame's x axis of relative rotation `r` (frame B in frame A), rad.
pub fn twist_angle(r: Quat) -> f32 {
    let r = if r.w < 0.0 { Quat { x: -r.x, y: -r.y, z: -r.z, w: -r.w } } else { r };
    if r.x.abs() < 1.0e-12 && r.w.abs() < 1.0e-12 {
        return 0.0;
    }
    wrap(2.0 * dmath::atan2(r.x, r.w))
}

/// Swing angle (rad) between the frames' x axes, and its world axis.
pub fn swing_of(xa: Vec3, xb: Vec3) -> (f32, Option<Vec3>) {
    let c = xa.cross(xb);
    let s = c.length();
    (dmath::atan2(s, xa.dot(xb)), (s > 1.0e-6).then(|| c * (1.0 / s)))
}

/// The cone's half-angle in the direction of world swing axis `axis`, for frame A rotation
/// `qa` and half-angles `[about y, about z]` (an ellipse between them).
pub fn cone_limit(qa: Quat, axis: Vec3, [hy, hz]: [f32; 2]) -> f32 {
    let l = qa.conjugate().rotate(axis);
    let (sy, sz) = (l.y, l.z);
    let n = (sy * sy + sz * sz).sqrt();
    if n < 1.0e-9 {
        return hy.min(hz);
    }
    let (sy, sz) = (sy / n, sz / n);
    let (hy, hz) = (hy.max(1.0e-4), hz.max(1.0e-4));
    let k = (sy / hy) * (sy / hy) + (sz / hz) * (sz / hz);
    1.0 / k.sqrt()
}

/// Current geometry of a joint in a substep.
struct Geo {
    ra: Vec3,
    rb: Vec3,
    /// Anchor B minus anchor A.
    d: Vec3,
    qa: Quat,
    qb: Quat,
}

impl JointConstraint {
    fn geo(&self, a: &Local, b: &Local) -> Geo {
        let ra = a.dq.rotate(self.ra0);
        let rb = b.dq.rotate(self.rb0);
        let d = self.delta_center + (b.dp - a.dp) + (rb - ra);
        Geo { ra, rb, d, qa: (a.dq * self.fa0).normalize(), qb: (b.dq * self.fb0).normalize() }
    }
}

/// `-r × (I (r × P))` as a matrix (the point velocity a unit impulse at `r` makes by turning).
fn arm_matrix(r: Vec3, inv_i: &Mat3) -> Mat3 {
    let col = |e: Vec3| -(r.cross(inv_i.mul_vec(r.cross(e))));
    Mat3 { c: [col(Vec3::X), col(Vec3::Y), col(Vec3::Z)] }
}

/// The inverse of a constraint mass matrix whose locked axes have zero rows and columns
/// (2D mode, kinematic-only sides): those axes are decoupled before inverting.
fn invert(mut k: Mat3) -> Mat3 {
    let tiny = 1.0e-12;
    if k.c[0].x.abs() < tiny {
        k.c[0].x = 1.0;
    }
    if k.c[1].y.abs() < tiny {
        k.c[1].y = 1.0;
    }
    if k.c[2].z.abs() < tiny {
        k.c[2].z = 1.0;
    }
    k.inverse()
}

fn point_mass(g: &Geo, a: &Local, b: &Local) -> Mat3 {
    let m = Mat3::diag(a.inv_mass + b.inv_mass);
    invert(m.add(&arm_matrix(g.ra, &a.inv_inertia)).add(&arm_matrix(g.rb, &b.inv_inertia)))
}

#[inline]
fn apply_linear(p: Vec3, ra: Vec3, rb: Vec3, a: &mut Local, b: &mut Local) {
    a.v = a.v - mul(a.inv_mass, p);
    a.w = a.w - a.inv_inertia.mul_vec(ra.cross(p));
    b.v = b.v + mul(b.inv_mass, p);
    b.w = b.w + b.inv_inertia.mul_vec(rb.cross(p));
}

#[inline]
fn apply_angular(l: Vec3, a: &mut Local, b: &mut Local) {
    a.w = a.w - a.inv_inertia.mul_vec(l);
    b.w = b.w + b.inv_inertia.mul_vec(l);
}

fn clamp_len(v: Vec3, max: f32) -> Vec3 {
    let l2 = v.dot(v);
    if l2 > max * max && l2 > 0.0 {
        v * (max / l2.sqrt())
    } else {
        v
    }
}

/// Bias, mass scale and impulse scale of a soft equality row.
#[inline]
fn soft_terms(soft: &Softness, c: f32, use_bias: bool, max_push: f32) -> (f32, f32, f32) {
    if use_bias {
        ((soft.bias_rate * c).clamp(-max_push, max_push), soft.mass_scale, soft.impulse_scale)
    } else {
        (0.0, 1.0, 0.0)
    }
}

/// The anchors' 3-row point constraint (one block).
fn solve_point(g: &Geo, acc: &mut Vec3, a: &mut Local, b: &mut Local, soft: &Softness, use_bias: bool) {
    let k = point_mass(g, a, b);
    let cdot = (b.v + b.w.cross(g.rb)) - (a.v + a.w.cross(g.ra));
    let (bias, ms, is) = if use_bias {
        (clamp_len(g.d * soft.bias_rate, MAX_LINEAR_PUSH), soft.mass_scale, soft.impulse_scale)
    } else {
        (Vec3::ZERO, 1.0, 0.0)
    };
    let impulse = -(k.mul_vec(cdot + bias) * ms) - *acc * is;
    *acc = *acc + impulse;
    apply_linear(impulse, g.ra, g.rb, a, b);
}

/// Locks relative rotation to `target` (frame B in frame A) with one 3-row block.
fn solve_lock(g: &Geo, target: Option<Quat>, acc: &mut Vec3, a: &mut Local, b: &mut Local, soft: &Softness, use_bias: bool) {
    let k = invert(a.inv_inertia.add(&b.inv_inertia));
    let want = match target {
        Some(t) => (g.qa * t).normalize(),
        None => g.qa,
    };
    let c = rotation_vector((g.qb * want.conjugate()).normalize());
    let cdot = b.w - a.w;
    let (bias, ms, is) = if use_bias {
        (clamp_len(c * soft.bias_rate, MAX_ANGULAR_PUSH), soft.mass_scale, soft.impulse_scale)
    } else {
        (Vec3::ZERO, 1.0, 0.0)
    };
    let impulse = -(k.mul_vec(cdot + bias) * ms) - *acc * is;
    *acc = *acc + impulse;
    apply_angular(impulse, a, b);
}

fn angular_row(n: Vec3, a: &Local, b: &Local) -> Row {
    let aa = a.inv_inertia.mul_vec(n);
    let ab = b.inv_inertia.mul_vec(n);
    let k = n.dot(aa) + n.dot(ab);
    Row { dir: Vec3::ZERO, rxa: n, rxb: n, aa, ab, mass: if k > 0.0 { 1.0 / k } else { 0.0 } }
}

/// A soft equality row: drives `c` (which grows at the row's velocity) to zero.
#[allow(clippy::too_many_arguments)]
fn equality(row: &Row, c: f32, acc: &mut f32, a: &mut Local, b: &mut Local, soft: &Softness, use_bias: bool, max_push: f32) {
    let (bias, ms, is) = soft_terms(soft, c, use_bias, max_push);
    let l = -row.mass * ms * (row.velocity(a, b) + bias) - is * *acc;
    *acc += l;
    row.apply(l, a, b);
}

/// A one-sided row keeping `c >= 0` (`c` grows at the row's velocity), with speculation.
#[allow(clippy::too_many_arguments)]
fn inequality(row: &Row, c: f32, acc: &mut f32, a: &mut Local, b: &mut Local, soft: &Softness, use_bias: bool, inv_h: f32, max_push: f32) {
    let (bias, ms, is) = if c > 0.0 {
        (c * inv_h, 1.0, 0.0)
    } else {
        soft_terms(soft, c, use_bias, max_push)
    };
    let l = -row.mass * ms * (row.velocity(a, b) + bias) - is * *acc;
    let new = (*acc + l).max(0.0);
    let l = new - *acc;
    *acc = new;
    row.apply(l, a, b);
}

/// A scalar motor along `row`: holds `speed`, or springs `c` (its position error) to zero,
/// within `max_impulse` per substep.
fn motor_row(row: &Row, drive: (Option<f32>, f32), acc: &mut f32, a: &mut Local, b: &mut Local, spring: &Softness, max_impulse: f32) {
    let v = row.velocity(a, b);
    let l = match drive {
        (Some(speed), _) => -row.mass * (v - speed),
        (None, c) => -row.mass * spring.mass_scale * (v + spring.bias_rate * c) - spring.impulse_scale * *acc,
    };
    let new = (*acc + l).clamp(-max_impulse, max_impulse);
    let l = new - *acc;
    *acc = new;
    row.apply(l, a, b);
}

/// Prepares a joint's softness for substep `h`.
pub fn softness(desc: &JointDesc, joint: Softness, h: f32) -> (Softness, Softness) {
    let spring = match (desc.kind, desc.motor) {
        (_, Some(Motor { drive: Drive::Position(_) | Drive::Rotation(_), hertz, damping_ratio, .. })) => Softness::new(hertz, damping_ratio, h),
        (JointKind::Distance, _) if desc.spring_hertz > 0.0 => Softness::new(desc.spring_hertz, desc.spring_damping, h),
        _ => Softness::default(),
    };
    (joint, spring)
}

/// Applies a joint's stored impulses (start of each substep).
pub fn warm_start(joints: &[JointConstraint], bodies: &mut [SolverBody]) {
    for j in joints {
        let (mut a, mut b) = (Local::load(bodies, j.a), Local::load(bodies, j.b));
        let g = j.geo(&a, &b);
        let (xa, _, _) = axes(g.qa);
        let imp = &j.imp;
        match j.desc.kind {
            JointKind::Fixed | JointKind::Hinge | JointKind::Ball => {
                apply_linear(imp.linear, g.ra, g.rb, &mut a, &mut b);
                apply_angular(imp.angular, &mut a, &mut b);
                apply_angular(imp.friction, &mut a, &mut b);
                let (xb, _, _) = axes(g.qb);
                let axis = if j.desc.kind == JointKind::Ball { normalize_or(xa + xb, xa) } else { xa };
                apply_angular(axis * (imp.lower - imp.upper), &mut a, &mut b);
                if j.desc.kind == JointKind::Ball {
                    if let (_, Some(s)) = swing_of(xa, xb) {
                        apply_angular(s * -imp.swing, &mut a, &mut b);
                    }
                    apply_angular(imp.motor, &mut a, &mut b);
                } else if j.desc.kind == JointKind::Hinge {
                    apply_angular(xa * imp.motor.x, &mut a, &mut b);
                } else {
                    apply_angular(imp.motor, &mut a, &mut b);
                }
            }
            JointKind::Prismatic => {
                apply_linear(imp.linear, g.ra + g.d, g.rb, &mut a, &mut b);
                apply_angular(imp.angular, &mut a, &mut b);
                apply_linear(xa * (imp.lower - imp.upper + imp.motor.x), g.ra + g.d, g.rb, &mut a, &mut b);
            }
            JointKind::Distance => {
                let n = normalize_or(g.d, Vec3::Y);
                apply_linear(n * (imp.spring + imp.lower - imp.upper), g.ra, g.rb, &mut a, &mut b);
            }
        }
        a.store(bodies, j.a);
        b.store(bodies, j.b);
    }
}

/// One pass over the joints (with the position bias when `use_bias`).
pub fn solve(joints: &mut [JointConstraint], bodies: &mut [SolverBody], h: f32, use_bias: bool) {
    let inv_h = 1.0 / h;
    for j in joints.iter_mut() {
        let (mut a, mut b) = (Local::load(bodies, j.a), Local::load(bodies, j.b));
        let g = j.geo(&a, &b);
        let (xa, ya, _) = axes(g.qa);
        let (xb, yb, _) = axes(g.qb);
        let soft = j.soft;
        let spring = j.spring;
        let motor = j.desc.motor;
        let max_motor = motor.map_or(0.0, |m| m.max_force * h);
        let max_friction = j.desc.friction * h;
        let imp = &mut j.imp;
        match j.desc.kind {
            JointKind::Fixed => {
                let target = match motor {
                    Some(Motor { drive: Drive::Rotation(q), .. }) => Some(q),
                    _ => None,
                };
                solve_lock(&g, target, &mut imp.angular, &mut a, &mut b, &soft, use_bias);
                solve_point(&g, &mut imp.linear, &mut a, &mut b, &soft, use_bias);
            }
            JointKind::Hinge => {
                let angle = hinge_angle(xa, ya, yb);
                let row = angular_row(xa, &a, &b);
                if max_friction > 0.0 {
                    let mut acc = imp.friction.dot(xa);
                    motor_row(&row, (Some(0.0), 0.0), &mut acc, &mut a, &mut b, &spring, max_friction);
                    imp.friction = xa * acc;
                }
                if let Some(m) = motor {
                    let drive = match m.drive {
                        Drive::Speed(s) => (Some(s), 0.0),
                        Drive::Position(t) => (None, wrap(angle - t)),
                        Drive::Rotation(_) => (Some(0.0), 0.0),
                    };
                    motor_row(&row, drive, &mut imp.motor.x, &mut a, &mut b, &spring, max_motor);
                }
                if let Some([lo, hi]) = j.desc.limit {
                    inequality(&row, angle - lo, &mut imp.lower, &mut a, &mut b, &soft, use_bias, inv_h, MAX_ANGULAR_PUSH);
                    let up = angular_row(-xa, &a, &b);
                    inequality(&up, hi - angle, &mut imp.upper, &mut a, &mut b, &soft, use_bias, inv_h, MAX_ANGULAR_PUSH);
                }
                // Keep the axes aligned: the two directions across the axis.
                let (p1, p2) = basis(xa);
                let c = xa.cross(xb);
                let mut acc = [imp.angular.dot(p1), imp.angular.dot(p2)];
                for (k, p) in [p1, p2].into_iter().enumerate() {
                    let r = angular_row(p, &a, &b);
                    equality(&r, c.dot(p), &mut acc[k], &mut a, &mut b, &soft, use_bias, MAX_ANGULAR_PUSH);
                }
                imp.angular = p1 * acc[0] + p2 * acc[1];
                solve_point(&g, &mut imp.linear, &mut a, &mut b, &soft, use_bias);
            }
            JointKind::Ball => {
                if max_friction > 0.0 {
                    // Friction: towards no relative turning, at most `friction` N·m.
                    let k = invert(a.inv_inertia.add(&b.inv_inertia));
                    let l = -k.mul_vec(b.w - a.w);
                    let new = clamp_len(imp.friction + l, max_friction);
                    apply_angular(new - imp.friction, &mut a, &mut b);
                    imp.friction = new;
                }
                if let Some(Motor { drive: Drive::Rotation(t), .. }) = motor {
                    // A spring towards the target rotation, as strong as the motor allows.
                    let k = invert(a.inv_inertia.add(&b.inv_inertia));
                    let want = (g.qa * t).normalize();
                    let c = rotation_vector((g.qb * want.conjugate()).normalize());
                    let cdot = b.w - a.w;
                    let l = -(k.mul_vec(cdot + c * spring.bias_rate) * spring.mass_scale) - imp.motor * spring.impulse_scale;
                    let new = clamp_len(imp.motor + l, max_motor);
                    apply_angular(new - imp.motor, &mut a, &mut b);
                    imp.motor = new;
                }
                if let Some([lo, hi]) = j.desc.limit {
                    let twist = twist_angle((g.qa.conjugate() * g.qb).normalize());
                    let t = normalize_or(xa + xb, xa);
                    let row = angular_row(t, &a, &b);
                    inequality(&row, twist - lo, &mut imp.lower, &mut a, &mut b, &soft, use_bias, inv_h, MAX_ANGULAR_PUSH);
                    let up = angular_row(-t, &a, &b);
                    inequality(&up, hi - twist, &mut imp.upper, &mut a, &mut b, &soft, use_bias, inv_h, MAX_ANGULAR_PUSH);
                }
                if let Some(cone) = j.desc.swing {
                    if let (theta, Some(s)) = swing_of(xa, xb) {
                        let lim = cone_limit(g.qa, s, cone);
                        if lim < PI {
                            let row = angular_row(-s, &a, &b);
                            inequality(&row, lim - theta, &mut imp.swing, &mut a, &mut b, &soft, use_bias, inv_h, MAX_ANGULAR_PUSH);
                        }
                    }
                }
                solve_point(&g, &mut imp.linear, &mut a, &mut b, &soft, use_bias);
            }
            JointKind::Prismatic => {
                let along = g.d.dot(xa);
                let row = Row::new(xa, g.ra + g.d, g.rb, &a, &b);
                if let Some(m) = motor {
                    let drive = match m.drive {
                        Drive::Speed(s) => (Some(s), 0.0),
                        Drive::Position(t) => (None, along - t),
                        Drive::Rotation(_) => (Some(0.0), 0.0),
                    };
                    motor_row(&row, drive, &mut imp.motor.x, &mut a, &mut b, &spring, max_motor);
                }
                if let Some([lo, hi]) = j.desc.limit {
                    inequality(&row, along - lo, &mut imp.lower, &mut a, &mut b, &soft, use_bias, inv_h, MAX_LINEAR_PUSH);
                    let up = Row::new(-xa, g.ra + g.d, g.rb, &a, &b);
                    inequality(&up, hi - along, &mut imp.upper, &mut a, &mut b, &soft, use_bias, inv_h, MAX_LINEAR_PUSH);
                }
                solve_lock(&g, None, &mut imp.angular, &mut a, &mut b, &soft, use_bias);
                let (p1, p2) = basis(xa);
                let mut acc = [imp.linear.dot(p1), imp.linear.dot(p2)];
                for (k, p) in [p1, p2].into_iter().enumerate() {
                    let r = Row::new(p, g.ra + g.d, g.rb, &a, &b);
                    equality(&r, g.d.dot(p), &mut acc[k], &mut a, &mut b, &soft, use_bias, MAX_LINEAR_PUSH);
                }
                imp.linear = p1 * acc[0] + p2 * acc[1];
            }
            JointKind::Distance => {
                let len = g.d.length();
                let n = normalize_or(g.d, Vec3::Y);
                let row = Row::new(n, g.ra, g.rb, &a, &b);
                let [min, max] = j.desc.length;
                if min == max {
                    equality(&row, len - min, &mut imp.spring, &mut a, &mut b, &soft, use_bias, MAX_LINEAR_PUSH);
                } else {
                    if j.desc.spring_hertz > 0.0 {
                        let c = len - j.desc.rest_length;
                        let l = -row.mass * spring.mass_scale * (row.velocity(&a, &b) + spring.bias_rate * c) - spring.impulse_scale * imp.spring;
                        imp.spring += l;
                        row.apply(l, &mut a, &mut b);
                    }
                    if min > 0.0 {
                        inequality(&row, len - min, &mut imp.lower, &mut a, &mut b, &soft, use_bias, inv_h, MAX_LINEAR_PUSH);
                    }
                    let up = Row::new(-n, g.ra, g.rb, &a, &b);
                    inequality(&up, max - len, &mut imp.upper, &mut a, &mut b, &soft, use_bias, inv_h, MAX_LINEAR_PUSH);
                }
            }
        }
        a.store(bodies, j.a);
        b.store(bodies, j.b);
    }
}

/// A joint's measured state from the bodies' poses (`None` side: the world).
/// `h` is the substep length (impulses become forces).
pub fn measure(desc: &JointDesc, pa: &Pose, pb: &Pose, imp: &JointImpulses, h: f32) -> JointState {
    let fa = pa.mul(&desc.frame_a);
    let fb = pb.mul(&desc.frame_b);
    let d = fb.p - fa.p;
    let (xa, ya, _) = axes(fa.q);
    let (xb, yb, _) = axes(fb.q);
    let rel = (fa.q.conjugate() * fb.q).normalize();
    let mut s = JointState { active: true, ..JointState::default() };
    let excess = |v: f32, r: Option<[f32; 2]>| r.map_or(0.0, |[lo, hi]| (lo - v).max(v - hi).max(0.0));
    match desc.kind {
        JointKind::Fixed => {
            s.linear_error = d.length();
            let target = match desc.motor {
                Some(Motor { drive: Drive::Rotation(q), .. }) => q,
                _ => Quat::IDENTITY,
            };
            s.angular_error = rotation_vector((rel * target.conjugate()).normalize()).length();
        }
        JointKind::Hinge => {
            s.linear_error = d.length();
            s.angular_error = swing_of(xa, xb).0;
            s.position = hinge_angle(xa, ya, yb);
            s.limit_excess = excess(s.position, desc.limit);
        }
        JointKind::Ball => {
            s.linear_error = d.length();
            s.position = twist_angle(rel);
            let (theta, axis) = swing_of(xa, xb);
            s.swing = theta;
            let cone = match (desc.swing, axis) {
                (Some(c), Some(ax)) => (theta - cone_limit(fa.q, ax, c)).max(0.0),
                _ => 0.0,
            };
            s.limit_excess = excess(s.position, desc.limit).max(cone);
        }
        JointKind::Prismatic => {
            let along = d.dot(xa);
            s.linear_error = (d - xa * along).length();
            s.angular_error = rotation_vector(rel).length();
            s.position = along;
            s.limit_excess = excess(along, desc.limit);
        }
        JointKind::Distance => {
            let len = d.length();
            s.position = len;
            let [min, max] = desc.length;
            s.linear_error = (min - len).max(len - max).max(0.0);
        }
    }
    if h > 0.0 {
        // Impulses are applied every substep: force = impulse / substep length.
        let axial = (imp.lower - imp.upper).abs();
        let (lin, ang) = match desc.kind {
            JointKind::Distance => ((imp.spring + imp.lower - imp.upper).abs(), 0.0),
            JointKind::Prismatic => (imp.linear.length() + (imp.lower - imp.upper + imp.motor.x).abs(), imp.angular.length()),
            JointKind::Hinge => (imp.linear.length(), imp.angular.length() + axial + imp.motor.x.abs()),
            JointKind::Ball => (imp.linear.length(), axial + imp.swing.abs() + imp.motor.length()),
            JointKind::Fixed => (imp.linear.length(), imp.angular.length()),
        };
        s.force = lin / h;
        s.torque = ang / h;
    }
    s
}

/// Checks a joint's numbers (finite, ordered ranges, sensible motors).
pub fn validate(desc: &JointDesc) -> Result<(), crate::PhysicsError> {
    let bad = |field: &'static str, msg: &str| Err(crate::PhysicsError::new("invalid_joint", msg.to_string()).with("field", field));
    let finite_pose = |p: &Pose| crate::math::is_finite(p.p) && [p.q.x, p.q.y, p.q.z, p.q.w].iter().all(|c| c.is_finite());
    if desc.a.is_none() && desc.b.is_none() {
        return bad("b", "a joint links at least one body (the other side may be the world)");
    }
    if desc.a.is_some() && desc.a == desc.b {
        return bad("b", "a joint links two different bodies");
    }
    if !finite_pose(&desc.frame_a) || !finite_pose(&desc.frame_b) {
        return bad("frame", "joint frames must be finite");
    }
    if let Some([lo, hi]) = desc.limit {
        if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
            return bad("limit", "a limit is [low, high] with low <= high");
        }
        if matches!(desc.kind, JointKind::Hinge | JointKind::Ball) && (lo < -PI - 1.0e-4 || hi > PI + 1.0e-4) {
            return bad("limit", "angle limits lie within -180..180 degrees");
        }
    }
    if let Some([y, z]) = desc.swing {
        if !(y.is_finite() && z.is_finite() && (0.0..=PI + 1.0e-4).contains(&y) && (0.0..=PI + 1.0e-4).contains(&z)) {
            return bad("swing", "swing half-angles lie within 0..180 degrees");
        }
    }
    if desc.kind == JointKind::Distance {
        let [min, max] = desc.length;
        if !(min.is_finite() && max.is_finite() && 0.0 <= min && min <= max && max > 0.0) {
            return bad("length", "a distance joint's length is [min, max] with 0 <= min <= max, max > 0");
        }
        if !(desc.spring_hertz.is_finite() && desc.spring_hertz >= 0.0 && desc.spring_damping.is_finite() && desc.spring_damping >= 0.0 && desc.rest_length.is_finite()) {
            return bad("spring", "spring hertz and damping must be >= 0");
        }
    }
    if !(desc.friction.is_finite() && desc.friction >= 0.0) {
        return bad("friction", "joint friction must be a finite torque >= 0 (N·m)");
    }
    if let Some(m) = desc.motor {
        if !(m.max_force.is_finite() && m.max_force >= 0.0 && m.hertz.is_finite() && m.hertz >= 0.0 && m.damping_ratio.is_finite() && m.damping_ratio >= 0.0) {
            return bad("motor", "a motor's max_force, hertz and damping must be finite and >= 0");
        }
        let ok = match m.drive {
            Drive::Speed(s) => s.is_finite() && matches!(desc.kind, JointKind::Hinge | JointKind::Prismatic),
            Drive::Position(p) => p.is_finite() && matches!(desc.kind, JointKind::Hinge | JointKind::Prismatic) && m.hertz > 0.0,
            Drive::Rotation(q) => [q.x, q.y, q.z, q.w].iter().all(|c| c.is_finite()) && matches!(desc.kind, JointKind::Ball | JointKind::Fixed) && m.hertz > 0.0,
        };
        if !ok {
            return bad(
                "motor",
                "motors: hinges and prismatic joints take a speed or a position (position needs hertz > 0); ball and fixed joints take a rotation (hertz > 0)",
            );
        }
    }
    Ok(())
}

/// Builds the frame `(anchor, x axis, y reference)` as a pose: x along `axis`, y as close to
/// `normal` as it can be while perpendicular (deterministic fallbacks for degenerate input).
pub fn frame(anchor: Vec3, axis: Vec3, normal: Vec3) -> Pose {
    let x = normalize_or(axis, Vec3::X);
    let y0 = normal - x * normal.dot(x);
    let y = try_normalize(y0).unwrap_or_else(|| basis(x).0);
    let z = x.cross(y);
    Pose { p: anchor, q: quat_from_axes(x, y, z) }
}

/// The rotation whose columns are the unit axes `x, y, z` (right-handed).
pub fn quat_from_axes(x: Vec3, y: Vec3, z: Vec3) -> Quat {
    let (m00, m11, m22) = (x.x, y.y, z.z);
    let tr = m00 + m11 + m22;
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        Quat { w: 0.25 * s, x: (y.z - z.y) / s, y: (z.x - x.z) / s, z: (x.y - y.x) / s }
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        Quat { w: (y.z - z.y) / s, x: 0.25 * s, y: (y.x + x.y) / s, z: (z.x + x.z) / s }
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        Quat { w: (z.x - x.z) / s, x: (y.x + x.y) / s, y: 0.25 * s, z: (z.y + y.z) / s }
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        Quat { w: (x.y - y.x) / s, x: (z.x + x.z) / s, y: (z.y + y.z) / s, z: 0.25 * s }
    };
    q.normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn frames_and_angles() {
        let f = frame(Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), Vec3::Y);
        assert!(close(f.q.rotate(Vec3::X).z, 1.0, 1e-6) && close(f.q.rotate(Vec3::Y).y, 1.0, 1e-6));
        // Degenerate normal: still a frame.
        let g = frame(Vec3::ZERO, Vec3::Y, Vec3::Y);
        assert!(close(g.q.rotate(Vec3::X).y, 1.0, 1e-6) && close(g.q.rotate(Vec3::Y).y, 0.0, 1e-6));
        // Rotation vectors and twists.
        let q = Quat::from_axis_angle(Vec3::Z, 0.7);
        let r = rotation_vector(q);
        assert!(close(r.z, 0.7, 1e-5) && close(r.x, 0.0, 1e-6));
        assert!(close(twist_angle(Quat::from_axis_angle(Vec3::X, -1.2)), -1.2, 1e-5));
        // A swing doesn't twist.
        assert!(close(twist_angle(Quat::from_axis_angle(Vec3::Y, 0.9)), 0.0, 1e-6));
        assert!(close(hinge_angle(Vec3::X, Vec3::Y, Quat::from_axis_angle(Vec3::X, 0.5).rotate(Vec3::Y)), 0.5, 1e-5));
        assert!(close(wrap(3.5 * PI), -0.5 * PI, 1e-5) || close(wrap(3.5 * PI), 1.5 * PI - 2.0 * PI, 1e-5));
        // An elliptical cone: its half-angle about y, about z, and between.
        let id = Quat::IDENTITY;
        assert!(close(cone_limit(id, Vec3::Y, [0.5, 1.0]), 0.5, 1e-6));
        assert!(close(cone_limit(id, Vec3::Z, [0.5, 1.0]), 1.0, 1e-6));
        let mid = cone_limit(id, normalize_or(Vec3::new(0.0, 1.0, 1.0), Vec3::Y), [0.5, 1.0]);
        assert!(mid > 0.5 && mid < 1.0);
    }

    #[test]
    fn validation_says_what_is_wrong() {
        let mut d = JointDesc::new(JointKind::Hinge, Some(1), Some(2), Pose::IDENTITY, Pose::IDENTITY);
        assert!(validate(&d).is_ok());
        d.limit = Some([1.0, -1.0]);
        assert_eq!(validate(&d).unwrap_err().code, "invalid_joint");
        d.limit = None;
        d.motor = Some(Motor { drive: Drive::Rotation(Quat::IDENTITY), max_force: 1.0, hertz: 5.0, damping_ratio: 1.0 });
        assert!(validate(&d).is_err(), "hinges take speeds or angles");
        d.b = Some(1);
        d.motor = None;
        assert!(validate(&d).is_err(), "two different bodies");
        let mut r = JointDesc::new(JointKind::Distance, None, Some(2), Pose::IDENTITY, Pose::IDENTITY);
        assert!(validate(&r).is_err(), "a distance joint needs a length");
        r.length = [0.0, 2.0];
        assert!(validate(&r).is_ok());
    }
}
