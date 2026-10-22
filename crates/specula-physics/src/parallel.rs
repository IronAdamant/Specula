//! Physics on several threads ([`Settings::threads`](crate::Settings::threads)): a persistent,
//! deterministic worker pool that runs the narrowphase's pairs and (with
//! [`Settings::coloured`](crate::Settings::coloured)) the contact solver.
//!
//! - **Persistent.** The workers are started once and wait between steps (spinning briefly,
//!   then blocking), so a step pays one wake-up, not a thread start.
//! - **Deterministic.** Contacts are solved a colour group at a time ([`crate::solver::colour`]):
//!   no two constraints of a group share a moving body, so they can run in any order or at once
//!   with the same result; the overflow group, the joints and everything between passes run
//!   serially. Every constraint runs the very code the single-threaded solver runs, on bodies and
//!   impulses loaded bit for bit, so the states are the same on any thread count.
//! - **Not waiting on threads that haven't started.** Work is split into chunks that whoever is
//!   free claims (the step's own thread too); a phase ends when its chunks are done, so a worker
//!   the machine hasn't scheduled yet simply misses the step. But a worker descheduled while it
//!   holds a claimed chunk does stall the others at that phase's end until it runs again: on a
//!   busy machine more threads can be slower than one (a review run under a 40-process load:
//!   4 threads 66 ms a step against 49 on one), which is why threads are opt-in.
//! - **No unsafe.** Bodies and impulses live in atomics (`f32` bits) while the step runs; the
//!   colouring guarantees no two threads write the same body in a phase, and a phase's
//!   completion count (release, acquire) orders the phases.

use crate::contact::{Manifold, SatCache};
use crate::joint::JointConstraint;
use crate::math::{Quat, Vec3};
use crate::solver::{self, Constraint, Imp, Local, SolverBody, SolverPoint, Tuning, STATIC};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::wide;

/// A lane with nothing to solve.
const EMPTY: Constraint = Constraint {
    a: STATIC,
    b: STATIC,
    friction: 0.0,
    restitution: 0.0,
    soft: crate::solver::Softness { bias_rate: 0.0, mass_scale: 1.0, impulse_scale: 0.0 },
    start: 0,
    len: 0,
    contact: 0,
};

/// Constraints per chunk a thread claims at once.
const CHUNK: usize = 24;
/// The smallest colour group run in parallel (smaller ones, and all after them, run serially).
const MIN_PARALLEL: usize = 4 * CHUNK;
/// Bodies per chunk in the integration phases.
const BODY_CHUNK: usize = 128;
/// Spins (with a pause hint) a waiting thread makes before yielding, and before blocking.
const SPINS: u32 = 2_000;
const IDLE_SPINS: u32 = 256;

/// Floats per body in the atomic store: v, w, dp, dq.
const BODY_F: usize = 13;

/// One body's motion, a cache line of its own (threads writing neighbouring bodies would
/// otherwise fight over shared lines).
#[repr(align(64))]
struct BodyCell([AtomicU32; BODY_F]);

impl Default for BodyCell {
    fn default() -> BodyCell {
        BodyCell(std::array::from_fn(|_| AtomicU32::new(0)))
    }
}
/// Floats per point's impulses: normal, two tangents, max normal.
const IMP_F: usize = 4;

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Bodies' velocities over a substep.
    Velocities,
    Positions,
    /// Joints' warm start, solve (with bias) or relax: serial, one chunk.
    Joints(u8),
    /// Contact constraints `[from, to)`: warm start, solve, relax or restitution.
    Warm(u32, u32),
    Solve(u32, u32, bool),
    Bounce(u32, u32),
}

#[derive(Clone, Copy, Debug)]
struct Phase {
    kind: Kind,
    chunks: usize,
    /// One chunk only (a serial phase: the overflow group, joints).
    serial: bool,
}

/// What one step's threads share.
#[derive(Default)]
struct StepData {
    bodies: Vec<BodyCell>,
    /// The bodies as the step began (masses, inertia, gravity, damping, locks are read from here).
    fixed: Vec<SolverBody>,
    cons: Vec<Constraint>,
    points: Vec<SolverPoint>,
    imps: Vec<AtomicU32>,
    joints: Mutex<Vec<JointConstraint>>,
    /// The serial joint phases' view of the bodies.
    scratch: Mutex<Vec<SolverBody>>,
    phases: Vec<Phase>,
    /// Per phase: chunks claimed, chunks done.
    claimed: Vec<AtomicUsize>,
    done: Vec<AtomicUsize>,
    h: f32,
    inv_h: f32,
    tuning: Option<Tuning>,
    /// Solve passes four constraints at a time ([`crate::wide`]).
    wide: bool,
}

fn f(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

fn put(a: &AtomicU32, x: f32) {
    a.store(x.to_bits(), Ordering::Relaxed);
}

impl StepData {
    fn body(&self, i: usize) -> SolverBody {
        let s = &self.bodies[i].0;
        let mut b = self.fixed[i];
        b.v = Vec3::new(f(&s[0]), f(&s[1]), f(&s[2]));
        b.w = Vec3::new(f(&s[3]), f(&s[4]), f(&s[5]));
        b.dp = Vec3::new(f(&s[6]), f(&s[7]), f(&s[8]));
        b.dq = Quat { x: f(&s[9]), y: f(&s[10]), z: f(&s[11]), w: f(&s[12]) };
        b
    }

    fn store_body(&self, i: usize, b: &SolverBody) {
        let s = &self.bodies[i].0;
        for (k, x) in [b.v.x, b.v.y, b.v.z, b.w.x, b.w.y, b.w.z, b.dp.x, b.dp.y, b.dp.z, b.dq.x, b.dq.y, b.dq.z, b.dq.w].into_iter().enumerate() {
            put(&s[k], x);
        }
    }

    /// A constraint side as the serial solver loads it.
    fn local(&self, i: u32) -> Local {
        if i == STATIC {
            return Local::load(&[], STATIC);
        }
        let s = &self.bodies[i as usize].0;
        Local::with_motion(
            &self.fixed[i as usize],
            Vec3::new(f(&s[0]), f(&s[1]), f(&s[2])),
            Vec3::new(f(&s[3]), f(&s[4]), f(&s[5])),
            Vec3::new(f(&s[6]), f(&s[7]), f(&s[8])),
            Quat { x: f(&s[9]), y: f(&s[10]), z: f(&s[11]), w: f(&s[12]) },
        )
    }

    fn store_local(&self, l: &Local, i: u32) {
        if i == STATIC || !l.moves() {
            return;
        }
        let s = &self.bodies[i as usize].0;
        for (k, x) in [l.v.x, l.v.y, l.v.z, l.w.x, l.w.y, l.w.z].into_iter().enumerate() {
            put(&s[k], x);
        }
    }

    fn load_imps(&self, c: &Constraint, out: &mut [Imp; 4]) {
        for (k, slot) in out.iter_mut().enumerate().take(c.len as usize) {
            let s = &self.imps[(c.start as usize + k) * IMP_F..];
            *slot = Imp { normal: f(&s[0]), tangent: [f(&s[1]), f(&s[2])], max_normal: f(&s[3]) };
        }
    }

    fn store_imps(&self, c: &Constraint, from: &[Imp; 4]) {
        for (k, i) in from.iter().enumerate().take(c.len as usize) {
            let s = &self.imps[(c.start as usize + k) * IMP_F..];
            put(&s[0], i.normal);
            put(&s[1], i.tangent[0]);
            put(&s[2], i.tangent[1]);
            put(&s[3], i.max_normal);
        }
    }

    /// Runs chunk `k` of phase `p`.
    fn run_chunk(&self, p: &Phase, k: usize) {
        let t = self.tuning.expect("set");
        match p.kind {
            Kind::Velocities | Kind::Positions => {
                let n = self.fixed.len();
                for i in k * BODY_CHUNK..((k + 1) * BODY_CHUNK).min(n) {
                    let mut b = self.body(i);
                    if matches!(p.kind, Kind::Velocities) {
                        solver::integrate_velocity(&mut b, self.h, t.max_linear_speed);
                    } else {
                        solver::integrate_position(&mut b, self.h);
                    }
                    self.store_body(i, &b);
                }
            }
            Kind::Joints(pass) => {
                let mut joints = self.joints.lock().unwrap_or_else(|e| e.into_inner());
                let mut bodies = self.scratch.lock().unwrap_or_else(|e| e.into_inner());
                bodies.clear();
                bodies.extend((0..self.fixed.len()).map(|i| self.body(i)));
                match pass {
                    0 => crate::joint::warm_start(&joints, &mut bodies),
                    1 => crate::joint::solve(&mut joints, &mut bodies, self.h, true),
                    _ => crate::joint::solve(&mut joints, &mut bodies, self.h, false),
                }
                for (i, b) in bodies.iter().enumerate() {
                    self.store_body(i, b);
                }
            }
            Kind::Solve(from, to, bias) if !p.serial && self.wide => {
                // Four constraints of the group at a time (bit-identical to one by one).
                let (lo, hi) = (from as usize + k * CHUNK, (from as usize + (k + 1) * CHUNK).min(to as usize));
                for batch in self.cons[lo..hi].chunks(wide::W) {
                    let cs: [&Constraint; wide::W] = std::array::from_fn(|l| batch.get(l).unwrap_or(&EMPTY));
                    let pts: [&[SolverPoint]; wide::W] = std::array::from_fn(|l| if l < batch.len() { &self.points[cs[l].range()] } else { &[] });
                    let mut imps = [[Imp::default(); 4]; wide::W];
                    let mut a: [Local; wide::W] = std::array::from_fn(|l| self.local(if l < batch.len() { cs[l].a } else { STATIC }));
                    let mut b: [Local; wide::W] = std::array::from_fn(|l| self.local(if l < batch.len() { cs[l].b } else { STATIC }));
                    for (l, c) in batch.iter().enumerate() {
                        self.load_imps(c, &mut imps[l]);
                    }
                    wide::solve4(&cs, &pts, &mut imps, &mut a, &mut b, self.inv_h, t.push_speed, bias);
                    for (l, c) in batch.iter().enumerate() {
                        self.store_imps(c, &imps[l]);
                        self.store_local(&a[l], c.a);
                        self.store_local(&b[l], c.b);
                    }
                }
            }
            Kind::Warm(from, to) | Kind::Solve(from, to, _) | Kind::Bounce(from, to) => {
                let (lo, hi) = if p.serial { (from as usize, to as usize) } else { (from as usize + k * CHUNK, (from as usize + (k + 1) * CHUNK).min(to as usize)) };
                let mut imps = [Imp::default(); 4];
                for c in &self.cons[lo..hi] {
                    let (mut a, mut b) = (self.local(c.a), self.local(c.b));
                    self.load_imps(c, &mut imps);
                    let n = c.len as usize;
                    let pts = &self.points[c.range()];
                    match p.kind {
                        Kind::Warm(..) => solver::warm_one(pts, &imps[..n], &mut a, &mut b),
                        Kind::Solve(_, _, bias) => solver::solve_one(c, pts, &mut imps[..n], &mut a, &mut b, self.inv_h, t.push_speed, bias),
                        _ => {
                            if c.restitution != 0.0 {
                                solver::restitution_one(c, pts, &mut imps[..n], &mut a, &mut b, t.restitution_threshold);
                            }
                        }
                    }
                    self.store_imps(c, &imps);
                    self.store_local(&a, c.a);
                    self.store_local(&b, c.b);
                }
            }
        }
    }

}

/// Work the pool's threads share: each thread (the caller's too) calls `work` once and takes
/// part until it's done; it must be correct with any number of threads joining, at any time.
pub(crate) trait Work: Send + Sync {
    fn work(&self);
}

impl Work for StepData {
    /// Takes part in the step: claims chunks phase by phase, waiting at each phase's end until
    /// all its chunks are done.
    fn work(&self) {
        for (pi, p) in self.phases.iter().enumerate() {
            loop {
                let k = self.claimed[pi].fetch_add(1, Ordering::Relaxed);
                if k >= p.chunks {
                    break;
                }
                self.run_chunk(p, k);
                self.done[pi].fetch_add(1, Ordering::Release);
            }
            let mut spins = 0u32;
            while self.done[pi].load(Ordering::Acquire) < p.chunks {
                spins += 1;
                if spins < SPINS {
                    std::hint::spin_loop();
                } else {
                    std::thread::yield_now();
                }
            }
        }
    }
}

/// The pool's shared state.
struct Shared {
    /// The current job (`None` between jobs).
    job: Mutex<Option<Arc<dyn Work>>>,
    /// Bumped for every step; workers sleep on `wake` when it doesn't change.
    epoch: AtomicU64,
    /// Workers inside a step.
    active: AtomicUsize,
    quit: AtomicBool,
    wake: Condvar,
    sleep: Mutex<()>,
}

/// The persistent workers and the step data they share (kept between steps).
pub struct Parallel {
    shared: Arc<Shared>,
    handles: Vec<JoinHandle<()>>,
    data: Arc<StepData>,
    narrow: Arc<NarrowJob>,
}

impl std::fmt::Debug for Parallel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parallel({} workers)", self.handles.len())
    }
}

impl Parallel {
    /// `threads` in all: this one and `threads - 1` workers.
    pub fn new(threads: usize) -> Parallel {
        let shared = Arc::new(Shared { job: Mutex::new(None), epoch: AtomicU64::new(0), active: AtomicUsize::new(0), quit: AtomicBool::new(false), wake: Condvar::new(), sleep: Mutex::new(()) });
        let handles = (1..threads.max(1))
            .filter_map(|i| {
                let s = Arc::clone(&shared);
                std::thread::Builder::new().name(format!("specula-physics-{i}")).spawn(move || worker(&s)).ok()
            })
            .collect();
        Parallel { shared, handles, data: Arc::new(StepData::default()), narrow: Arc::new(NarrowJob::default()) }
    }

    pub fn threads(&self) -> usize {
        self.handles.len() + 1
    }

    /// The substeps' passes on the pool (see the module docs); the same results as
    /// [`solver::step`] on one thread.
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, cons: &mut Vec<Constraint>, groups: &[u32], points: &mut Vec<SolverPoint>, imps: &mut [Imp], joints: &mut Vec<JointConstraint>, bodies: &mut [SolverBody], h: f32, n: u32, t: &Tuning) {
        // Fill the shared data (workers hold none of it between steps).
        let d = match Arc::get_mut(&mut self.data) {
            Some(d) => d,
            None => {
                self.data = Arc::new(StepData::default());
                Arc::get_mut(&mut self.data).expect("fresh")
            }
        };
        d.fixed.clear();
        d.fixed.extend_from_slice(bodies);
        d.bodies.resize_with(bodies.len(), BodyCell::default);
        for (i, b) in bodies.iter().enumerate() {
            let s = &mut d.bodies[i].0;
            for (k, x) in [b.v.x, b.v.y, b.v.z, b.w.x, b.w.y, b.w.z, b.dp.x, b.dp.y, b.dp.z, b.dq.x, b.dq.y, b.dq.z, b.dq.w].into_iter().enumerate() {
                *s[k].get_mut() = x.to_bits();
            }
        }
        // Moved in and back out, not copied.
        std::mem::swap(&mut d.cons, cons);
        std::mem::swap(&mut d.points, points);
        d.imps.resize_with(imps.len() * IMP_F, || AtomicU32::new(0));
        for (i, m) in imps.iter().enumerate() {
            let s = &mut d.imps[i * IMP_F..(i + 1) * IMP_F];
            for (k, x) in [m.normal, m.tangent[0], m.tangent[1], m.max_normal].into_iter().enumerate() {
                *s[k].get_mut() = x.to_bits();
            }
        }
        std::mem::swap(d.joints.get_mut().unwrap_or_else(|e| e.into_inner()), joints);
        d.h = h;
        d.inv_h = 1.0 / h;
        d.wide = t.wide;
        d.tuning = Some(*t);
        // The plan: the same order as the serial solver.
        d.phases.clear();
        let has_joints = !d.joints.get_mut().unwrap_or_else(|e| e.into_inner()).is_empty();
        let body_chunks = bodies.len().div_ceil(BODY_CHUNK);
        // A colour group too small to be worth a phase of its own (each phase ends with a wait for
        // every chunk) runs serially with all the groups after it, in the same order: from the
        // first small group to the end (the overflow included) is one serial phase.
        let serial_from = (0..groups.len() - 1).find(|&g| g == groups.len() - 2 || ((groups[g + 1] - groups[g]) as usize) < MIN_PARALLEL).unwrap_or(groups.len() - 2);
        let groups_of = |make: &dyn Fn(u32, u32) -> Kind, phases: &mut Vec<Phase>| {
            for g in 0..serial_from {
                let (from, to) = (groups[g], groups[g + 1]);
                phases.push(Phase { kind: make(from, to), chunks: ((to - from) as usize).div_ceil(CHUNK), serial: false });
            }
            let (from, to) = (groups[serial_from], groups[groups.len() - 1]);
            if from < to {
                phases.push(Phase { kind: make(from, to), chunks: 1, serial: true });
            }
        };
        for _ in 0..n {
            d.phases.push(Phase { kind: Kind::Velocities, chunks: body_chunks, serial: false });
            if has_joints {
                d.phases.push(Phase { kind: Kind::Joints(0), chunks: 1, serial: true });
            }
            groups_of(&|a, b| Kind::Warm(a, b), &mut d.phases);
            if has_joints {
                d.phases.push(Phase { kind: Kind::Joints(1), chunks: 1, serial: true });
            }
            groups_of(&|a, b| Kind::Solve(a, b, true), &mut d.phases);
            d.phases.push(Phase { kind: Kind::Positions, chunks: body_chunks, serial: false });
            if has_joints {
                d.phases.push(Phase { kind: Kind::Joints(2), chunks: 1, serial: true });
            }
            groups_of(&|a, b| Kind::Solve(a, b, false), &mut d.phases);
        }
        groups_of(&|a, b| Kind::Bounce(a, b), &mut d.phases);
        let np = d.phases.len();
        d.claimed.resize_with(np, || AtomicUsize::new(0));
        d.done.resize_with(np, || AtomicUsize::new(0));
        for i in 0..np {
            *d.claimed[i].get_mut() = 0;
            *d.done[i].get_mut() = 0;
        }
        let job: Arc<dyn Work> = Arc::clone(&self.data) as Arc<dyn Work>;
        self.run(job);
        // Read the results back (a late worker may still hold the data: read, don't take).
        let d = &*self.data;
        for (i, b) in bodies.iter_mut().enumerate() {
            let s = &d.bodies[i].0;
            let g = |k: usize| f(&s[k]);
            b.v = Vec3::new(g(0), g(1), g(2));
            b.w = Vec3::new(g(3), g(4), g(5));
            b.dp = Vec3::new(g(6), g(7), g(8));
            b.dq = Quat { x: g(9), y: g(10), z: g(11), w: g(12) };
        }
        for (i, m) in imps.iter_mut().enumerate() {
            let s = &d.imps[i * IMP_F..(i + 1) * IMP_F];
            let g = |k: usize| f(&s[k]);
            *m = Imp { normal: g(0), tangent: [g(1), g(2)], max_normal: g(3) };
        }
        std::mem::swap(&mut *d.joints.lock().unwrap_or_else(|e| e.into_inner()), joints);
        // The constraints and points go back to the caller (copied if a late worker holds them).
        match Arc::get_mut(&mut self.data) {
            Some(d) => {
                std::mem::swap(&mut d.cons, cons);
                std::mem::swap(&mut d.points, points);
            }
            None => {
                cons.clone_from(&self.data.cons);
                points.clone_from(&self.data.points);
            }
        }
    }
}

impl Parallel {
    /// Runs `job` on the pool: posts it, wakes the workers, takes part (its `work` returns when
    /// all of it is done, whoever did it), then closes it. A worker still inside (late, finding
    /// nothing left) isn't waited for: the next job fills fresh data if it still holds this.
    fn run(&self, job: Arc<dyn Work>) {
        {
            let mut slot = self.shared.job.lock().unwrap_or_else(|e| e.into_inner());
            *slot = Some(Arc::clone(&job));
            self.shared.epoch.fetch_add(1, Ordering::Release);
        }
        {
            let _g = self.shared.sleep.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.wake.notify_all();
        }
        job.work();
        drop(job);
        *self.shared.job.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The narrowphase's pair tests on the pool: `fill` puts the inputs in (it gets the job's
    /// input list, cleared), and the results come back in input order through `take` (index,
    /// manifold, polytope memory). The same results as testing them one by one.
    pub(crate) fn narrowphase(&mut self, fill: impl FnOnce(&mut Vec<NarrowIn>), mut take: impl FnMut(usize, Manifold, SatCache)) {
        let job = match Arc::get_mut(&mut self.narrow) {
            Some(j) => j,
            None => {
                self.narrow = Arc::new(NarrowJob::default());
                Arc::get_mut(&mut self.narrow).expect("fresh")
            }
        };
        job.inputs.clear();
        fill(&mut job.inputs);
        let chunks = job.inputs.len().div_ceil(NARROW_CHUNK);
        job.out.resize_with(chunks, || Mutex::new(Vec::new()));
        for o in &mut job.out {
            o.get_mut().unwrap_or_else(|e| e.into_inner()).clear();
        }
        *job.claimed.get_mut() = 0;
        *job.done.get_mut() = 0;
        job.chunks = chunks;
        let run: Arc<dyn Work> = Arc::clone(&self.narrow) as Arc<dyn Work>;
        self.run(run);
        for (k, o) in self.narrow.out.iter().enumerate() {
            for (i, (m, sat)) in o.lock().unwrap_or_else(|e| e.into_inner()).drain(..).enumerate() {
                take(k * NARROW_CHUNK + i, m, sat);
            }
        }
    }
}

/// Pairs per chunk of the narrowphase.
const NARROW_CHUNK: usize = 64;

/// One pair to test: its shapes where they are, how far to look, its polytope memory.
#[derive(Clone, Debug)]
pub(crate) struct NarrowIn {
    pub a: crate::shape::Shape,
    pub pa: crate::math::Pose,
    pub b: crate::shape::Shape,
    pub pb: crate::math::Pose,
    pub margin: f32,
    pub trigger: bool,
    pub sat: SatCache,
}

#[derive(Default)]
struct NarrowJob {
    inputs: Vec<NarrowIn>,
    /// Per chunk: its results in input order.
    out: Vec<Mutex<Vec<(Manifold, SatCache)>>>,
    claimed: AtomicUsize,
    done: AtomicUsize,
    chunks: usize,
}

/// One pair's test (the same on any thread).
pub(crate) fn test_pair(i: &NarrowIn) -> (Manifold, SatCache) {
    test_shapes(&i.a, &i.pa, &i.b, &i.pb, i.margin, i.trigger, i.sat)
}

/// [`test_pair`] on borrowed shapes (the single-threaded narrowphase copies nothing).
pub(crate) fn test_shapes(a: &crate::shape::Shape, pa: &crate::math::Pose, b: &crate::shape::Shape, pb: &crate::math::Pose, margin: f32, trigger: bool, sat: SatCache) -> (Manifold, SatCache) {
    let mut m = Manifold::default();
    let mut sat = sat;
    if trigger {
        crate::contact::collide(a, pa, b, pb, 0.0, &mut m);
    } else {
        crate::contact::collide_cached(a, pa, b, pb, margin, &mut m, Some(&mut sat));
    }
    (m, sat)
}

impl Work for NarrowJob {
    fn work(&self) {
        loop {
            let k = self.claimed.fetch_add(1, Ordering::Relaxed);
            if k >= self.chunks {
                break;
            }
            let res: Vec<(Manifold, SatCache)> = self.inputs[k * NARROW_CHUNK..((k + 1) * NARROW_CHUNK).min(self.inputs.len())].iter().map(test_pair).collect();
            *self.out[k].lock().unwrap_or_else(|e| e.into_inner()) = res;
            self.done.fetch_add(1, Ordering::Release);
        }
        while self.done.load(Ordering::Acquire) < self.chunks {
            std::hint::spin_loop();
            std::thread::yield_now();
        }
    }
}

fn worker(s: &Shared) {
    let mut seen = s.epoch.load(Ordering::Acquire);
    let mut idle = 0u32;
    loop {
        if s.quit.load(Ordering::Acquire) {
            return;
        }
        let now = s.epoch.load(Ordering::Acquire);
        if now != seen {
            seen = now;
            idle = 0;
            // Join only while the step is open (the step's thread closes it, then waits for
            // those inside to leave).
            let job = {
                let g = s.job.lock().unwrap_or_else(|e| e.into_inner());
                g.as_ref().map(|j| {
                    s.active.fetch_add(1, Ordering::AcqRel);
                    Arc::clone(j)
                })
            };
            if let Some(j) = job {
                j.work();
                drop(j);
                s.active.fetch_sub(1, Ordering::AcqRel);
            }
            continue;
        }
        idle += 1;
        if idle < IDLE_SPINS {
            if idle % SPINS == 0 {
                std::thread::yield_now();
            } else {
                std::hint::spin_loop();
            }
        } else {
            let g = s.sleep.lock().unwrap_or_else(|e| e.into_inner());
            if s.epoch.load(Ordering::Acquire) == seen && !s.quit.load(Ordering::Acquire) {
                let _unused = s.wake.wait_timeout(g, std::time::Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner());
            }
        }
    }
}

impl Drop for Parallel {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        {
            let _g = self.shared.sleep.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.wake.notify_all();
        }
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}
