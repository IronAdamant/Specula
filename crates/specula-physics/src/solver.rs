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
    /// Normal relative velocity at the step start (for restitution).
    pub relative_velocity: f32,
}

/// A contact point's accumulated impulses (what the passes change; kept apart from the
/// point's fixed rows, so a thread can load and store just these).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Imp {
    pub normal: f32,
    pub tangent: [f32; 2],
    /// The largest normal impulse of the step (whether it touched, for restitution).
    pub max_normal: f32,
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
    /// The thread pool solves four constraints at a time ([`crate::Settings::wide`]).
    pub wide: bool,
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

    /// [`Local::load`] of a body whose motion is kept apart (the thread pool's atomics).
    #[inline(always)]
    pub(crate) fn with_motion(b: &SolverBody, v: Vec3, w: Vec3, dp: Vec3, dq: Quat) -> Local {
        Local {
            v,
            w,
            dp,
            dq,
            inv_mass: if b.kinematic { Vec3::ZERO } else { b.inv_mass },
            inv_inertia: if b.kinematic { Mat3::ZERO } else { b.inv_inertia },
            moves: !b.kinematic,
        }
    }

    /// Whether it takes impulses (a dynamic body).
    #[inline]
    pub(crate) fn moves(&self) -> bool {
        self.moves
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
pub fn prepare(constraints: &[Constraint], points: &mut [SolverPoint], imps: &mut [Imp], bodies: &[SolverBody]) {
    for c in constraints {
        let (a, b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        for i in &mut imps[c.range()] {
            i.max_normal = 0.0;
        }
        for p in &mut points[c.range()] {
            let n = p.normal;
            let (t1, t2) = crate::math::basis(n);
            p.n = Row::new(n, p.ra, p.rb, &a, &b);
            p.t1 = Row::new(t1, p.ra, p.rb, &a, &b);
            p.t2 = Row::new(t2, p.ra, p.rb, &a, &b);
            p.relative_velocity = p.n.velocity(&a, &b);
        }
    }
}

pub fn integrate_velocities(bodies: &mut [SolverBody], h: f32, max_speed: f32) {
    for b in bodies.iter_mut() {
        integrate_velocity(b, h, max_speed);
    }
}

/// One body's velocity over a substep (gravity, damping, the speed caps, its locks).
#[inline(always)]
pub(crate) fn integrate_velocity(b: &mut SolverBody, h: f32, max_speed: f32) {
    {
        if b.kinematic {
            return;
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
        integrate_position(b, h);
    }
}

/// One body's position and rotation over a substep.
#[inline(always)]
pub(crate) fn integrate_position(b: &mut SolverBody, h: f32) {
    b.dp = b.dp + b.v * h;
    b.dq = integrate_rotation(b.dq, b.w, h);
}

pub fn warm_start(constraints: &[Constraint], points: &[SolverPoint], imps: &[Imp], bodies: &mut [SolverBody]) {
    for c in constraints {
        let (mut a, mut b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        warm_one(&points[c.range()], &imps[c.range()], &mut a, &mut b);
        a.store(bodies, c.a);
        b.store(bodies, c.b);
    }
}

/// One constraint's warm start (the same code on any thread).
#[inline(always)]
pub(crate) fn warm_one(pts: &[SolverPoint], imps: &[Imp], a: &mut Local, b: &mut Local) {
    for (p, i) in pts.iter().zip(imps) {
        p.n.apply(i.normal, a, b);
        p.t1.apply(i.tangent[0], a, b);
        p.t2.apply(i.tangent[1], a, b);
    }
}

/// One pass over the contacts: non-penetration (soft, with bias when `use_bias`), then friction.
pub fn solve(constraints: &[Constraint], points: &[SolverPoint], imps: &mut [Imp], bodies: &mut [SolverBody], inv_h: f32, push_speed: f32, use_bias: bool) {
    for c in constraints {
        let (mut a, mut b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        solve_one(c, &points[c.range()], &mut imps[c.range()], &mut a, &mut b, inv_h, push_speed, use_bias);
        a.store(bodies, c.a);
        b.store(bodies, c.b);
    }
}

/// One constraint's pass (`pts` and `imps` are its own points; the same code on any thread).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_one(c: &Constraint, pts: &[SolverPoint], imps: &mut [Imp], a: &mut Local, b: &mut Local, inv_h: f32, push_speed: f32, use_bias: bool) {
    {
        let dp = b.dp - a.dp;
        for (p, i) in pts.iter().zip(imps.iter_mut()) {
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
            let vn = p.n.velocity(a, b);
            let mut impulse = -p.n.mass * mass_scale * (vn + bias) - impulse_scale * i.normal;
            let new = (i.normal + impulse).max(0.0);
            impulse = new - i.normal;
            i.normal = new;
            i.max_normal = i.max_normal.max(impulse);
            p.n.apply(impulse, a, b);
        }
        // Friction: a disc of radius friction x normal impulse in the tangent plane.
        for (p, i) in pts.iter().zip(imps.iter_mut()) {
            let l1 = -p.t1.mass * p.t1.velocity(a, b);
            let l2 = -p.t2.mass * p.t2.velocity(a, b);
            let [old1, old2] = i.tangent;
            let (mut n1, mut n2) = (old1 + l1, old2 + l2);
            let max = c.friction * i.normal;
            let len2 = n1 * n1 + n2 * n2;
            if len2 > max * max {
                let k = if len2 > 0.0 { max / len2.sqrt() } else { 0.0 };
                n1 *= k;
                n2 *= k;
            }
            i.tangent = [n1, n2];
            p.t1.apply(n1 - old1, a, b);
            p.t2.apply(n2 - old2, a, b);
        }
    }
}

pub fn restitution(constraints: &[Constraint], points: &[SolverPoint], imps: &mut [Imp], bodies: &mut [SolverBody], threshold: f32) {
    for c in constraints {
        if c.restitution == 0.0 {
            continue;
        }
        let (mut a, mut b) = (Local::load(bodies, c.a), Local::load(bodies, c.b));
        restitution_one(c, &points[c.range()], &mut imps[c.range()], &mut a, &mut b, threshold);
        a.store(bodies, c.a);
        b.store(bodies, c.b);
    }
}

/// One constraint's bounce (the same code on any thread).
#[inline(always)]
pub(crate) fn restitution_one(c: &Constraint, pts: &[SolverPoint], imps: &mut [Imp], a: &mut Local, b: &mut Local, threshold: f32) {
    for (p, i) in pts.iter().zip(imps.iter_mut()) {
        if p.relative_velocity > -threshold || i.max_normal == 0.0 {
            continue;
        }
        let vn = p.n.velocity(a, b);
        let mut impulse = -p.n.mass * (vn + c.restitution * p.relative_velocity);
        let new = (i.normal + impulse).max(0.0);
        impulse = new - i.normal;
        i.normal = new;
        i.max_normal = i.max_normal.max(impulse);
        p.n.apply(impulse, a, b);
    }
}

/// Buffers [`colour`] reuses.
#[derive(Clone, Debug, Default)]
pub struct ColourScratch {
    masks: Vec<u32>,
    colours: Vec<u8>,
    out: Vec<Constraint>,
}

/// Colours constraints are spread over; one that finds none free goes to the overflow group,
/// solved after them (Box2D v3's graph colouring).
pub const MAX_COLORS: usize = 24;

/// Graph colouring (Box2D v3): reorders `constraints` into groups in which no two share a
/// body that moves (dynamic bodies; static and kinematic ones take no impulses, so any number
/// of constraints may touch them), each the lowest colour free for both its bodies, in the
/// order given, then the overflow. A group's constraints can be solved in any order, or at once,
/// with the same result. `groups` gets the groups' bounds (`groups[i]..groups[i + 1]`; the last
/// group is the overflow). Deterministic: the same constraints in the same order always colour
/// the same way. `scratch` is kept between steps (no allocation once warm).
pub fn colour(constraints: &mut Vec<Constraint>, bodies: &[SolverBody], groups: &mut Vec<u32>, scratch: &mut ColourScratch) {
    let ColourScratch { masks, colours, out } = scratch;
    masks.clear();
    masks.resize(bodies.len(), 0);
    let moving = |i: u32| i != STATIC && !bodies[i as usize].kinematic;
    let mut counts = [0u32; MAX_COLORS + 1];
    colours.clear();
    for c in constraints.iter() {
        let used = (if moving(c.a) { masks[c.a as usize] } else { 0 }) | (if moving(c.b) { masks[c.b as usize] } else { 0 });
        // As Box2D v3: a constraint with an unmoving side never takes colour 0, so the bodies
        // resting on others are solved before what they rest on is pinned (stacks hold better).
        let both = moving(c.a) && moving(c.b);
        let free = !used & ((1u32 << MAX_COLORS) - 1) & if both { u32::MAX } else { !1 };
        let k = if free == 0 { MAX_COLORS } else { free.trailing_zeros() as usize };
        if k < MAX_COLORS {
            for i in [c.a, c.b] {
                if moving(i) {
                    masks[i as usize] |= 1 << k;
                }
            }
        }
        counts[k] += 1;
        colours.push(k as u8);
    }
    groups.clear();
    let mut at = 0u32;
    let mut starts = [0u32; MAX_COLORS + 1];
    for k in 0..=MAX_COLORS {
        starts[k] = at;
        groups.push(at);
        at += counts[k];
    }
    groups.push(at);
    out.clear();
    if let Some(first) = constraints.first().copied() {
        out.resize(constraints.len(), first);
    }
    for (c, &k) in constraints.iter().zip(colours.iter()) {
        out[starts[k as usize] as usize] = *c;
        starts[k as usize] += 1;
    }
    std::mem::swap(constraints, out);
}

/// Runs a whole step on prepared bodies, contacts and joints (joints before contacts in every
/// pass, as in Box2D: contacts get the last word, so nothing sinks).
///
/// With `parallel` (and more than one colour group), the contact passes run on its threads, a
/// colour group at a time (see [`colour`]): the same states as on one thread, since a group's
/// constraints share no moving body and the overflow group stays serial.
#[allow(clippy::too_many_arguments)]
pub fn step(
    constraints: &mut Vec<Constraint>,
    groups: &[u32],
    points: &mut Vec<SolverPoint>,
    imps: &mut [Imp],
    joints: &mut Vec<crate::joint::JointConstraint>,
    bodies: &mut [SolverBody],
    dt: f32,
    t: &Tuning,
    parallel: Option<&mut crate::parallel::Parallel>,
) {
    let n = t.substeps.max(1);
    let h = dt / n as f32;
    let inv_h = 1.0 / h;
    prepare(constraints, points, imps, bodies);
    if let Some(p) = parallel {
        if groups.len() > 2 && !constraints.is_empty() {
            p.step(constraints, groups, points, imps, joints, bodies, h, n, t);
            return;
        }
    }
    for _ in 0..n {
        integrate_velocities(bodies, h, t.max_linear_speed);
        crate::joint::warm_start(joints, bodies);
        warm_start(constraints, points, imps, bodies);
        crate::joint::solve(joints, bodies, h, true);
        solve(constraints, points, imps, bodies, inv_h, t.push_speed, true);
        integrate_positions(bodies, h);
        crate::joint::solve(joints, bodies, h, false);
        solve(constraints, points, imps, bodies, inv_h, t.push_speed, false);
    }
    restitution(constraints, points, imps, bodies, t.restitution_threshold);
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
