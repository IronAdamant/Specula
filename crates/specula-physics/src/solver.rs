//! The Soft Step solver (Catto, "Solver2D", 2024; Box2D v3), in 3D.
//!
//! Per step of `dt` split into `n` substeps of `h = dt / n`:
//!
//! ```text
//! prepare contacts (anchors, Jacobians, effective masses, softness) once
//! for each substep:
//!     integrate velocities (gravity, damping)
//!     warm start (apply last impulses)
//!     solve        (soft constraints with position bias: push apart at most `push` m/s)
//!     integrate positions
//!     relax        (the same constraints without bias: removes the energy the bias added)
//! restitution (bounce), then store impulses for next step's warm start
//! ```
//!
//! Soft constraints have physical parameters (stiffness in Hz, damping ratio) instead of magic
//! numbers, so tuning is explainable ("contacts: 30 Hz, damping 10"). Sub-stepping keeps stacks
//! stiff at low cost. Anchors and inverse inertias are fixed for the step (so each constraint
//! row's angular Jacobian is computed once in `prepare`); the current separation is recovered
//! from each body's position and rotation change since the step began. Points are stored
//! densely (a pair uses only the points it has), since every pass streams them.
//!
//! **Joints (P4)** ([`crate::joint`]) run beside the contacts: warm start, solve with bias and
//! relax in the same places as the contact passes (joints first, as in Box2D), with their own
//! softness (joint Hz), built from the same [`Row`]s.
//!
//! Locked axes (2D mode, or a body's own locks) zero the matching inverse mass and inertia
//! components, so constraint masses stay consistent with what the body can do.

use crate::math::{integrate_rotation, mul, Mat3, Quat, Vec3};

/// Solver view of a body.
#[derive(Clone, Copy, Debug)]
pub struct SolverBody {
    pub v: Vec3,
    pub w: Vec3,
    /// Position change of the centre of mass since the step began.
    pub dp: Vec3,
    /// Rotation change since the step began.
    pub dq: Quat,
    /// Inverse mass per world axis (0 on locked axes).
    pub inv_mass: Vec3,
    /// World inverse inertia (rows/columns of locked axes zeroed).
    pub inv_inertia: Mat3,
    /// Acceleration from gravity (dynamic bodies only).
    pub gravity: Vec3,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// Kinematic: moves at its velocity but takes no impulses.
    pub kinematic: bool,
    pub lock_linear: [bool; 3],
    pub lock_angular: [bool; 3],
}

/// "Static" body index: zero velocity, infinite mass.
pub const STATIC: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Default)]
pub struct Softness {
    pub bias_rate: f32,
    pub mass_scale: f32,
    pub impulse_scale: f32,
}

impl Softness {
    pub fn new(hertz: f32, zeta: f32, h: f32) -> Softness {
        if hertz <= 0.0 {
            return Softness { bias_rate: 0.0, mass_scale: 1.0, impulse_scale: 0.0 };
        }
        let omega = 2.0 * std::f32::consts::PI * hertz;
        let a1 = 2.0 * zeta + h * omega;
        let a2 = h * omega * a1;
        let a3 = 1.0 / (1.0 + a2);
        Softness { bias_rate: omega / a1, mass_scale: a2 * a3, impulse_scale: a3 }
    }
}

/// One constraint direction between two bodies at fixed anchors: its Jacobian and the angular
/// velocity change a unit impulse makes on each body (the linear change is the body's inverse
/// mass times `dir`, applied from the body's own data).
#[derive(Clone, Copy, Debug, Default)]
pub struct Row {
    pub dir: Vec3,
    /// `ra × dir`, `rb × dir`.
    pub rxa: Vec3,
    pub rxb: Vec3,
    /// Angular velocity change per unit impulse.
    pub aa: Vec3,
    pub ab: Vec3,
    /// The impulse per unit velocity error (1 / effective mass).
    pub mass: f32,
}

impl Row {
    pub(crate) fn new(dir: Vec3, ra: Vec3, rb: Vec3, a: &Local, b: &Local) -> Row {
        let rxa = ra.cross(dir);
        let rxb = rb.cross(dir);
        let aa = a.inv_inertia.mul_vec(rxa);
        let ab = b.inv_inertia.mul_vec(rxb);
        let k = dir.dot(mul(a.inv_mass, dir)) + dir.dot(mul(b.inv_mass, dir)) + rxa.dot(aa) + rxb.dot(ab);
        Row { dir, rxa, rxb, aa, ab, mass: if k > 0.0 { 1.0 / k } else { 0.0 } }
    }

    /// Relative velocity along the row (B minus A).
    #[inline]
    pub(crate) fn velocity(&self, a: &Local, b: &Local) -> f32 {
        self.dir.dot(b.v - a.v) + self.rxb.dot(b.w) - self.rxa.dot(a.w)
    }

    #[inline]
    pub(crate) fn apply(&self, l: f32, a: &mut Local, b: &mut Local) {
        let p = self.dir * l;
        a.v = a.v - mul(a.inv_mass, p);
        a.w = a.w - self.aa * l;
        b.v = b.v + mul(b.inv_mass, p);
        b.w = b.w + self.ab * l;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SolverPoint {
    /// Fixed anchors from each body's centre of mass at the step start.
    pub ra: Vec3,
    pub rb: Vec3,
    pub normal: Vec3,
    /// Separation minus the anchors' contribution (see the module docs).
    pub adjusted_separation: f32,
    pub n: Row,
    pub t1: Row,
    pub t2: Row,
    pub normal_impulse: f32,
    pub tangent_impulse: [f32; 2],
    pub max_normal_impulse: f32,
    /// Normal relative velocity at the step start (for restitution).
    pub relative_velocity: f32,
}

/// A contact pair's constraint; its points are `points[start..start + len]`.
#[derive(Clone, Copy, Debug)]
pub struct Constraint {
    pub a: u32,
    pub b: u32,
    pub friction: f32,
    pub restitution: f32,
    pub soft: Softness,
    pub start: u32,
    pub len: u32,
    /// Index of the contact this came from (to store impulses back).
    pub contact: u32,
}

impl Constraint {
    #[inline]
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start as usize..(self.start + self.len) as usize
    }
}

/// Solver tuning (see [`crate::Settings`]).
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    pub substeps: u32,
    pub contact_hertz: f32,
    pub damping_ratio: f32,
    pub push_speed: f32,
    pub restitution_threshold: f32,
    pub max_linear_speed: f32,
}

const MAX_ROTATION_PER_SUBSTEP: f32 = 0.25 * std::f32::consts::PI;

/// A constraint's body state in locals (zero masses for static and kinematic bodies).
pub(crate) struct Local {
    pub(crate) v: Vec3,
    pub(crate) w: Vec3,
    pub(crate) dp: Vec3,
    pub(crate) dq: Quat,
    pub(crate) inv_mass: Vec3,
    pub(crate) inv_inertia: Mat3,
    moves: bool,
}

impl Local {
    #[inline]
    pub(crate) fn load(bodies: &[SolverBody], i: u32) -> Local {
        if i == STATIC {
            return Local { v: Vec3::ZERO, w: Vec3::ZERO, dp: Vec3::ZERO, dq: Quat::IDENTITY, inv_mass: Vec3::ZERO, inv_inertia: Mat3::ZERO, moves: false };
        }
        let b = &bodies[i as usize];
        Local {
            v: b.v,
            w: b.w,
            dp: b.dp,
            dq: b.dq,
            inv_mass: if b.kinematic { Vec3::ZERO } else { b.inv_mass },
            inv_inertia: if b.kinematic { Mat3::ZERO } else { b.inv_inertia },
            moves: !b.kinematic,
        }
    }

    #[inline]
    pub(crate) fn store(&self, bodies: &mut [SolverBody], i: u32) {
        if self.moves {
            let b = &mut bodies[i as usize];
            b.v = self.v;
            b.w = self.w;
        }
    }
}

/// Prepares constraint rows (Jacobians, masses, tangents, relative velocity). Impulses carry
/// over.
pub fn prepare(constraints: &[Constraint], points: &mut [SolverPoint], bodies: &[SolverBody]) {
    for c in constraints {
        let (a, b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        for p in &mut points[c.range()] {
            let n = p.normal;
            let (t1, t2) = crate::math::basis(n);
            p.n = Row::new(n, p.ra, p.rb, &a, &b);
            p.t1 = Row::new(t1, p.ra, p.rb, &a, &b);
            p.t2 = Row::new(t2, p.ra, p.rb, &a, &b);
            p.relative_velocity = p.n.velocity(&a, &b);
            p.max_normal_impulse = 0.0;
        }
    }
}

pub fn integrate_velocities(bodies: &mut [SolverBody], h: f32, max_speed: f32) {
    for b in bodies.iter_mut() {
        if b.kinematic {
            continue;
        }
        let mut v = (b.v + b.gravity * h) * (1.0 / (1.0 + h * b.linear_damping));
        let mut w = b.w * (1.0 / (1.0 + h * b.angular_damping));
        let v2 = v.dot(v);
        if v2 > max_speed * max_speed {
            v = v * (max_speed / v2.sqrt());
        }
        let w2 = w.dot(w);
        let max_w = MAX_ROTATION_PER_SUBSTEP / h;
        if w2 > max_w * max_w {
            w = w * (max_w / w2.sqrt());
        }
        lock(&mut v, b.lock_linear);
        lock(&mut w, b.lock_angular);
        b.v = v;
        b.w = w;
    }
}

fn lock(v: &mut Vec3, axes: [bool; 3]) {
    if axes[0] {
        v.x = 0.0;
    }
    if axes[1] {
        v.y = 0.0;
    }
    if axes[2] {
        v.z = 0.0;
    }
}

pub fn integrate_positions(bodies: &mut [SolverBody], h: f32) {
    for b in bodies.iter_mut() {
        b.dp = b.dp + b.v * h;
        b.dq = integrate_rotation(b.dq, b.w, h);
    }
}

pub fn warm_start(constraints: &[Constraint], points: &[SolverPoint], bodies: &mut [SolverBody]) {
    for c in constraints {
        let (mut a, mut b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        for p in &points[c.range()] {
            p.n.apply(p.normal_impulse, &mut a, &mut b);
            p.t1.apply(p.tangent_impulse[0], &mut a, &mut b);
            p.t2.apply(p.tangent_impulse[1], &mut a, &mut b);
        }
        a.store(bodies, c.a);
        b.store(bodies, c.b);
    }
}

/// One pass over the contacts: non-penetration (soft, with bias when `use_bias`), then friction.
pub fn solve(constraints: &[Constraint], points: &mut [SolverPoint], bodies: &mut [SolverBody], inv_h: f32, push_speed: f32, use_bias: bool) {
    for c in constraints {
        let (mut a, mut b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        let dp = b.dp - a.dp;
        let pts = &mut points[c.range()];
        for p in pts.iter_mut() {
            // Current separation from the bodies' motion since the step began.
            let pra = a.dq.rotate(p.ra);
            let prb = b.dq.rotate(p.rb);
            let s = (dp + (prb - pra)).dot(p.normal) + p.adjusted_separation;
            let (bias, mass_scale, impulse_scale) = if s > 0.0 {
                // Speculative: allow closing the gap this substep.
                (s * inv_h, 1.0, 0.0)
            } else if use_bias {
                ((c.soft.bias_rate * s).max(-push_speed), c.soft.mass_scale, c.soft.impulse_scale)
            } else {
                (0.0, 1.0, 0.0)
            };
            let vn = p.n.velocity(&a, &b);
            let mut impulse = -p.n.mass * mass_scale * (vn + bias) - impulse_scale * p.normal_impulse;
            let new = (p.normal_impulse + impulse).max(0.0);
            impulse = new - p.normal_impulse;
            p.normal_impulse = new;
            p.max_normal_impulse = p.max_normal_impulse.max(impulse);
            p.n.apply(impulse, &mut a, &mut b);
        }
        // Friction: a disc of radius friction x normal impulse in the tangent plane.
        for p in pts.iter_mut() {
            let l1 = -p.t1.mass * p.t1.velocity(&a, &b);
            let l2 = -p.t2.mass * p.t2.velocity(&a, &b);
            let (old1, old2) = (p.tangent_impulse[0], p.tangent_impulse[1]);
            let (mut n1, mut n2) = (old1 + l1, old2 + l2);
            let max = c.friction * p.normal_impulse;
            let len2 = n1 * n1 + n2 * n2;
            if len2 > max * max {
                let k = if len2 > 0.0 { max / len2.sqrt() } else { 0.0 };
                n1 *= k;
                n2 *= k;
            }
            p.tangent_impulse = [n1, n2];
            p.t1.apply(n1 - old1, &mut a, &mut b);
            p.t2.apply(n2 - old2, &mut a, &mut b);
        }
        a.store(bodies, c.a);
        b.store(bodies, c.b);
    }
}

pub fn restitution(constraints: &[Constraint], points: &mut [SolverPoint], bodies: &mut [SolverBody], threshold: f32) {
    for c in constraints {
        if c.restitution == 0.0 {
            continue;
        }
        let (mut a, mut b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        for p in &mut points[c.range()] {
            if p.relative_velocity > -threshold || p.max_normal_impulse == 0.0 {
                continue;
            }
            let vn = p.n.velocity(&a, &b);
            let mut impulse = -p.n.mass * (vn + c.restitution * p.relative_velocity);
            let new = (p.normal_impulse + impulse).max(0.0);
            impulse = new - p.normal_impulse;
            p.normal_impulse = new;
            p.max_normal_impulse = p.max_normal_impulse.max(impulse);
            p.n.apply(impulse, &mut a, &mut b);
        }
        a.store(bodies, c.a);
        b.store(bodies, c.b);
    }
}

/// Runs a whole step on prepared bodies, contacts and joints (joints before contacts in every
/// pass, as in Box2D: contacts get the last word, so nothing sinks).
pub fn step(constraints: &[Constraint], points: &mut [SolverPoint], joints: &mut [crate::joint::JointConstraint], bodies: &mut [SolverBody], dt: f32, t: &Tuning) {
    let n = t.substeps.max(1);
    let h = dt / n as f32;
    let inv_h = 1.0 / h;
    prepare(constraints, points, bodies);
    for _ in 0..n {
        integrate_velocities(bodies, h, t.max_linear_speed);
        crate::joint::warm_start(joints, bodies);
        warm_start(constraints, points, bodies);
        crate::joint::solve(joints, bodies, h, true);
        solve(constraints, points, bodies, inv_h, t.push_speed, true);
        integrate_positions(bodies, h);
        crate::joint::solve(joints, bodies, h, false);
        solve(constraints, points, bodies, inv_h, t.push_speed, false);
    }
    restitution(constraints, points, bodies, t.restitution_threshold);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softness_is_stiff_at_high_rates_and_off_at_zero() {
        let s = Softness::new(30.0, 10.0, 1.0 / 240.0);
        assert!(s.mass_scale > 0.0 && s.mass_scale < 1.0 && s.impulse_scale > 0.0 && s.bias_rate > 0.0);
        assert!((s.mass_scale + s.impulse_scale - 1.0).abs() < 1e-6);
        let off = Softness::new(0.0, 1.0, 0.01);
        assert_eq!((off.mass_scale, off.impulse_scale, off.bias_rate), (1.0, 0.0, 0.0));
    }

    #[test]
    fn rows_match_the_textbook_effective_mass() {
        // A unit-mass sphere (I = 0.4) hit at its surface along the normal through its centre:
        // effective mass 1 (no rotation).
        let b = SolverBody {
            v: Vec3::ZERO,
            w: Vec3::ZERO,
            dp: Vec3::ZERO,
            dq: Quat::IDENTITY,
            inv_mass: Vec3::ONE,
            inv_inertia: Mat3::diag(Vec3::ONE * 2.5),
            gravity: Vec3::ZERO,
            linear_damping: 0.0,
            angular_damping: 0.0,
            kinematic: false,
            lock_linear: [false; 3],
            lock_angular: [false; 3],
        };
        let bodies = [b];
        let (a, b) = (Local::load(&bodies, STATIC), Local::load(&bodies, 0));
        let r = Row::new(Vec3::Y, Vec3::ZERO, Vec3::new(0.0, -1.0, 0.0), &a, &b);
        assert!((r.mass - 1.0).abs() < 1e-6);
        // Off-centre: rotation makes it lighter.
        let r = Row::new(Vec3::Y, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), &a, &b);
        assert!((r.mass - 1.0 / 3.5).abs() < 1e-6);
    }
}
