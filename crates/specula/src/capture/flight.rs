//! The capture flight: sight's renders without waiting on the GPU (docs/design/WORKERS.md, "The
//! capture flight").
//!
//! A capture-like call (`sight_frame`, `sight_orbit`, `sight_compare`, `sight_query`) prepares
//! its view on the engine thread and submits it ([`Engine::render_view_submit`]: draw list,
//! subjects from the subject cache, everything the packet says about the world) and returns: the
//! engine goes on serving agents and presenting the window while the GPU renders. One view is in
//! flight at a time; later captures wait in [`CaptureQueue`] in the order they came. Each loop pass
//! ([`Engine::pump_captures`]) polls the GPU's fence without blocking: once the render has
//! finished, the view lands ([`Engine::render_view_land`]: ID counts, pixels already copied to the
//! readback buffer in the same submission) and its capture goes on, an orbit to its next view
//! (one view per pass), the others to their packet and the encode on a worker, as before.
//!
//! Any other offscreen render (filmstrips, effect previews, the studio) lands the flight first
//! ([`Engine::land_capture_flight`]): they share the ID buffers and the cached targets.
//!
//! A pass works through as many captures as fit its share of the engine thread
//! ([`WINDOW_PASS`], [`HEADLESS_PASS`]): it waits for the view in flight at most [`WINDOW_WAIT`]
//! or [`HEADLESS_WAIT`] at a time (a view that takes longer lands on a later pass, polled), lands
//! it, and submits the next at once. Small captures so keep their synchronous throughput; big
//! ones never hold the thread past a step's budget. An orbit's views go back to back (its next
//! view goes to the front of the queue), so one orbit at a time holds a sheet, charged to the
//! workers' byte cap. A waiting call past its deadline is answered and dropped; one whose owner
//! has no room left on the workers waits instead of rendering into a refusal.

use super::*;

/// Captures waiting for the GPU at once (each is an agent's call waiting for its answer; past it,
/// `job_busy`).
pub(crate) const MAX_QUEUED: usize = 64;

/// Engine time a pass spends rebuilding the subject cache while captures wait for it.
const WARM_SLICE: std::time::Duration = std::time::Duration::from_millis(3);

/// Engine time a loop pass gives captures while a window is open (its frame needs the rest).
pub(crate) const WINDOW_PASS: Duration = Duration::from_millis(6);
/// The longest one wait for the GPU may be while a window is open.
pub(crate) const WINDOW_WAIT: Duration = Duration::from_millis(3);
/// Headless there is no window to protect, only other agents' calls (read between passes).
pub(crate) const HEADLESS_PASS: Duration = Duration::from_millis(16);
/// The longest one wait for the GPU may be headless: with a view's read it stays well inside
/// the render budget (30 ms).
pub(crate) const HEADLESS_WAIT: Duration = Duration::from_millis(12);

/// A capture between its views: its job, and what its answer is made of so far.
pub(crate) struct Pending {
    job: Job,
    session: SessionInfo,
    wait: CaptureWait,
    to: Answer,
    views: Vec<ViewReq>,
    /// Models in frame still building when it started (its warnings say so).
    building: usize,
    time_s: f64,
    state: State,
    /// The call it belongs to (its steps are traced and timed under it).
    trace: specula_core::trace::Ctx,
    /// Past it the caller has given up: a view not started yet is dropped.
    deadline: Instant,
}

enum State {
    Frame,
    Orbit(Box<OrbitRun>),
    Mark,
    Diff { marked_frame: u64, marked_tick: u64, view: MarkView },
    Query { id: u64, out: Json },
    /// The editor's pick (answered from the ID image as it lands: `land_flight`).
    Pick,
}

/// An orbit's sheet and views so far.
struct OrbitRun {
    next: usize,
    center: Vec3,
    radius: f32,
    path: Option<String>,
    distance: f32,
    tile: (u32, u32),
    sheet_size: (u32, u32),
    /// Made when its first view lands (a queued orbit holds none), charged to the workers' byte
    /// cap by `held` until the encode job takes it over.
    sheet: Vec<u8>,
    held: Option<crate::workers::Held>,
    views: Vec<Json>,
    warnings: Vec<Json>,
    overlays: Option<Json>,
    /// The tick and world revision its first view showed: later views may show a later world.
    started: (u64, u64),
}

/// What a landed view's capture still needs, taken when the view was submitted.
struct Taken {
    /// Warnings about the world as drawn (validation messages are added when it lands).
    warnings: Vec<Json>,
    /// sight_frame: structured problems and the GI section.
    problems: Vec<Json>,
    gi: Option<Json>,
    /// sight_compare mark: every entity id then; diff: which of the mark's subjects still existed.
    ids: Vec<u64>,
    tick: u64,
}

/// The view on the GPU, and its capture.
pub(crate) struct Flight {
    view: Box<ViewInFlight>,
    pending: Pending,
    taken: Taken,
}

/// Captures waiting for their turn on the GPU, oldest first: new calls, and orbits between views.
#[derive(Default)]
pub struct CaptureQueue {
    pub(crate) flight: Option<Flight>,
    waiting: VecDeque<Waiting>,
    /// A call's views as its request worked them out, for a start in the same call: `(world
    /// generation, draw epoch, revision, views)`, used only while the world is unchanged.
    pub(super) views_hint: Option<ViewsHint>,
    /// A step made a new size's target or a slice of its graph's images: the pass's captures
    /// end there.
    end_pass: bool,
    /// The form view the view being submitted is drawn as, with its curvature scale (`sight_frame
    /// form`): set for that one render, as the GI view is.
    pub(super) form: Option<(specula_gpu::FormView, f32)>,
    /// A step job's view in flight (`timeline_preview`'s tiles), with the job's own slot: submitted
    /// in one step and landed in a later one, so the GPU's work never holds the engine thread.
    /// Landed like the flight before any other offscreen render ([`Engine::land_capture_flight`]),
    /// into its job's slot. The queue holds the slot weakly: a live job's tile is never dropped,
    /// however many jobs submit after it, and a tile whose job ended goes with it (lead review R2:
    /// a shared map kept the newest 8, so more than about 9 concurrent previews lost tiles).
    pub(crate) step_flight: Option<(std::rc::Weak<StepTile>, Box<ViewInFlight>)>,
    /// The size whose target was just made: its render graph's images are made a slice per pass
    /// ([`specula_gpu::Gpu::prewarm_target`]) before its first view renders.
    cold: Option<(u32, u32)>,
    /// Passes a view waited for the asset studio's render on the GPU: after [`TURN_PASSES`] it
    /// renders anyway (the studio's render is settled first: a short wait), so neither starves.
    studio_waits: u32,
    /// Views that landed before their projection came back (projected on the engine thread).
    pub(crate) late_projections: u64,
    /// Why the last pass ended ([`PassEnd`]), and how many views have landed.
    pub(crate) last_pass: PassEnd,
    pub(crate) landed_views: u64,
    /// The subject cache was still being read when `start_next` gave up this pass.
    warming: bool,
    /// Passes a form view waited for the form views' pipelines, built on a worker (tests).
    pub(crate) form_waits: u32,
}

/// Why a capture pass ended. A pass may end because something outside the engine thread isn't
/// ready (`Gpu`), because a piece of work is sliced over passes (`Slice`) or because its share
/// of the loop is used up (`Time`). It must never end with a view waiting and nothing in the
/// way (`Held` while views wait is the engine wasting a pass): the capture tests hold that, so
/// they pass or fail by what the code does, not by how fast the GPU or a worker happened to be
/// (the orbit's pass count used to be asserted, and failed one gate in four).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PassEnd {
    /// Nothing to do.
    #[default]
    Idle,
    /// The GPU is still rendering the view in flight.
    Gpu,
    /// A sliced step took its slice: a new size's images, the subject cache, the studio's turn.
    Slice,
    /// The pass's share of the loop is used up.
    Time,
    /// Calls wait and none could start (no room on the workers for their owners), or the view
    /// in flight was kept back for another reason.
    Held,
}


/// A step job's landed tile, in the slot the job owns ([`CaptureQueue::step_flight`]).
pub(crate) type StepTile = std::cell::RefCell<Option<Result<Rendered, String>>>;

/// Passes the capture flight and the asset studio give way to each other's render on the GPU
/// before taking their turn.
pub(crate) const TURN_PASSES: u32 = 3;

/// Engine-thread time one pass gives to making a new size's render graph images (at least one
/// image a pass). Making them all inside the first render held the thread 36-67 ms at 2048 x 2048.
pub(crate) const PREWARM_SLICE: std::time::Duration = std::time::Duration::from_millis(4);

/// A capture call not started yet (its views are worked out when its turn comes).
struct Start {
    job: Job,
    session: SessionInfo,
    wait: CaptureWait,
    reply: crate::jobs::Reply,
    call: u64,
    trace: specula_core::trace::Ctx,
    deadline: Instant,
    /// Its views as its request worked them out (see [`CaptureQueue::views_hint`]).
    views: Option<ViewsHint>,
}

/// `(world generation, draw epoch, revision, views)`: views worked out with the world at those.
type ViewsHint = (u64, u64, u64, Vec<ViewReq>);

/// Worlds up to this many entities start a capture inside the call that asked for it.
const INLINE_START_ENTITIES: usize = 4096;

enum Waiting {
    /// A call not started yet (its views are worked out when its turn comes).
    Start(Box<Start>),
    /// A capture with more views to render.
    Next(Box<Pending>),
}

impl CaptureQueue {
    /// Whether a view is in flight or a capture waits.
    pub fn busy(&self) -> bool {
        self.flight.is_some() || !self.waiting.is_empty()
    }

    /// `(views in flight, captures waiting)`, for engine_info.
    pub fn counts(&self) -> (usize, usize) {
        (usize::from(self.flight.is_some()), self.waiting.len())
    }
}

impl Engine {
    /// A capture job whose models are ready: started now when the GPU is free and nothing waits
    /// before it, else queued (in order).
    pub(super) fn capture_job(&mut self, job: Job, session: &SessionInfo, wait: CaptureWait, reply: crate::jobs::Reply, call: u64, deadline: Instant) {
        // In a big world a capture's preparation (draw list, subjects, submit) is a step of its
        // own on the next pass, not part of the call that worked out its views: each stays
        // within the render budget (20,000 entities at 2048 x 2048 took 35-55 ms together).
        let light = self.world.len() <= INLINE_START_ENTITIES;
        if light && !self.captures.busy() && self.subject_cache.ready(&self.world, self.world_generation) {
            self.start_capture(job, session, wait, reply, call, specula_core::trace::Ctx::current(), deadline);
            return;
        }
        let views = self.captures.views_hint.take();
        if self.captures.waiting.len() >= MAX_QUEUED {
            let e = crate::authority::refusal(crate::authority::Site::JobBusy, format!("{MAX_QUEUED} captures are waiting for the GPU: call again when one has answered"))
                .with("retry_after_s", 1)
                .with("retry", true);
            reply.refuse(&e);
            return;
        }
        self.captures.waiting.push_back(Waiting::Start(Box::new(Start { job, session: session.clone(), wait, reply, call, trace: specula_core::trace::Ctx::current(), deadline, views })));
    }

    /// Works out a capture's views and submits its first.
    #[allow(clippy::too_many_arguments)]
    fn start_capture(&mut self, job: Job, session: &SessionInfo, wait: CaptureWait, reply: crate::jobs::Reply, call: u64, trace: specula_core::trace::Ctx, deadline: Instant) {
        // Presentation again only if the simulation moved on while the capture was parked.
        if self.game.is_some() && wait.presented_at != Some(self.clock.tick()) {
            self.before_capture();
        }
        self.pump_builds(None);
        let now = (self.world_generation, self.world.draw_epoch(), self.world.revision());
        let hinted = self.captures.views_hint.take().filter(|(g, d, r, _)| (*g, *d, *r) == now).map(|h| h.3);
        let views = match hinted.map_or_else(|| self.job_views(&job, session), Ok) {
            Ok(v) => v,
            Err(out) => {
                let _ = reply.send(out);
                return;
            }
        };
        let building = self.building_in(&views);
        let to = Answer { reply, call };
        let time_of = |e: &Engine, t: Option<f32>| t.map_or_else(|| e.time_s(), f64::from);
        let (time_s, state) = match &job {
            Job::Frame(spec) => (time_of(self, spec.time_s), State::Frame),
            Job::Mark { frame, .. } => (time_of(self, frame.time_s), State::Mark),
            Job::Diff { name, .. } => {
                let key = self.swarm.key(session);
                let Some(mark) = self.marks.get(&key, name) else {
                    let _ = to.reply.send(mark_missing(name));
                    return;
                };
                let view = mark.view.clone();
                let (marked_frame, marked_tick) = (mark.data.frame, mark.data.tick);
                (time_of(self, view.time_s), State::Diff { marked_frame, marked_tick, view })
            }
            // The window's own time, so the pick sees what the window shows.
            Job::Pick(_) => (self.window_time_s(), State::Pick),
            Job::Query(q) => {
                // Answered without a render when the entity can't be in the frame.
                match self.query_start(q, &views[0]) {
                    Ok((id, out)) => (time_of(self, q.frame.time_s), State::Query { id, out }),
                    Err(out) => {
                        let _ = to.reply.send(out);
                        return;
                    }
                }
            }
            Job::Orbit(o) => {
                let (center, radius, path) = match self.orbit_target(&o.target) {
                    Ok(t) => t,
                    Err(out) => {
                        let _ = to.reply.send(out);
                        return;
                    }
                };
                let distance = o.distance.unwrap_or_else(|| orbit::fit_distance(radius, o.fov_y_deg));
                let (tw, th, sw, sh) = orbit::layout(o.views, o.tile);
                let run = OrbitRun {
                    next: 0,
                    center,
                    radius,
                    path,
                    distance,
                    tile: (tw, th),
                    sheet_size: (sw, sh),
                    sheet: Vec::new(),
                    held: None,
                    views: Vec::new(),
                    warnings: Vec::new(),
                    overlays: None,
                    started: (self.clock.tick(), self.world.revision()),
                };
                (time_of(self, o.time_s), State::Orbit(Box::new(run)))
            }
        };
        let p = Pending { job, session: session.clone(), wait, to, views, building, time_s, state, trace, deadline };
        self.capture_submit(p);
    }

    /// Submits a capture's next view: in flight, or (the bounds overlay's two renders) landed at
    /// once.
    fn capture_submit(&mut self, p: Pending) {
        let at = match &p.state {
            State::Orbit(run) => run.next,
            _ => 0,
        };
        let Some(view) = p.views.get(at) else {
            let _ = p.to.reply.send(ToolOutput::error("internal: a capture view past its last"));
            return;
        };
        let (camera, w, h) = (view.camera, view.width, view.height);
        if !self.targets.iter().any(|(size, _)| *size == (w, h)) {
            // A new size's target and readback buffer are made in a pass of their own (allocating
            // a 2048 x 2048 one took over 10 ms on the iGPU); the view goes next.
            if let Err(e) = self.offscreen_target(w, h) {
                let _ = p.to.reply.send(ToolOutput::error(e));
                return;
            }
            // The pass ends here: the view's render (which makes the render graph's images for
            // the new size) goes on the next one. (An empty warm-up frame made those images here
            // too, but its different shape built other pipeline variants and rebound the shadow
            // map: a 300 ms window frame in stall_load, so it was taken out; see WORKERS.md.)
            self.captures.end_pass = true;
            self.captures.cold = Some((w, h));
            self.captures.waiting.push_front(Waiting::Next(Box::new(p)));
            return;
        }
        if self.gpu.offscreen_pending() && self.captures.flight.is_none() && self.captures.step_flight.is_none() && self.captures.studio_waits < TURN_PASSES {
            // The asset studio's render is on the GPU (its pump lands it without waiting): this
            // view goes on a later pass rather than waiting for it here, a few passes at most. A
            // step job's tile (`timeline_preview`) is landed by the render path itself, so a capture
            // never waits for one.
            self.captures.studio_waits += 1;
            self.captures.end_pass = true;
            self.captures.waiting.push_front(Waiting::Next(Box::new(p)));
            return;
        }
        self.captures.studio_waits = 0;
        if self.captures.cold == Some((w, h)) {
            // The new size's render graph images (depth, post chain, SSAO, ID buffers), a slice a
            // pass, predicted from the world's environment and the latest frame's content; the
            // view renders once they exist (one the prediction missed is made by the render).
            let target = self.targets.iter().find(|(size, _)| *size == (w, h)).map(|(_, t)| *t);
            let done = match target {
                Some(t) => {
                    let env = self.world.env().light;
                    // The ID buffers too when the view counts them (shared images, bucket sized).
                    let ids = match &p.job {
                        Job::Frame(spec) | Job::Mark { frame: spec, .. } => spec.ids,
                        _ => true,
                    };
                    match self.gpu.prewarm_target(t, &env, ids, None, Instant::now() + PREWARM_SLICE) {
                        Ok(done) => done,
                        Err(e) => {
                            // The render makes them (and reports a real failure itself).
                            specula_core::trace_event!(&crate::trace_sink::SIGHT, Warn, "sight.prewarm_failed", width = w, height = h, error = e.to_string());
                            true
                        }
                    }
                }
                None => true,
            };
            self.captures.cold = None;
            if !done {
                self.captures.cold = Some((w, h));
            }
            // This pass's captures end with the slice: the view renders on a later pass.
            self.captures.end_pass = true;
            self.captures.waiting.push_front(Waiting::Next(Box::new(p)));
            return;
        }
        // A form view's pipelines are a set of their own, built on a worker the first time one
        // is asked for: the view waits for them, a pass at a time. (Compiled inside its first
        // render they held the engine thread 440 ms: second review of the form views.)
        let wants_form = match &p.job {
            Job::Frame(spec) => !spec.form.is_world(),
            Job::Orbit(o) => !o.form.is_world(),
            _ => false,
        };
        if wants_form {
            let v = self.gpu.pipeline_variants();
            if let Some(f) = v.failed.as_ref().filter(|f| f.form && !v.form) {
                let mut j = Json::parse(&crate::capture::gpu_failure("form_pipelines", &f.error, self.windowed)).unwrap_or(Json::Null);
                j.set("pipeline", Json::from(f.pipeline.as_str()));
                let _ = p.to.reply.send(ToolOutput::error(j.to_string()));
                return;
            }
            if !v.form {
                self.gpu.request_form_pipelines();
                self.captures.form_waits += 1;
                self.captures.end_pass = true;
                self.captures.waiting.push_front(Waiting::Next(Box::new(p)));
                return;
            }
        }
        let none = ov::Spec::default();
        let (opts, gi, form) = match (&p.job, &p.state) {
            (Job::Frame(spec), _) => (ViewOpts { include_ui: spec.include_ui, lod: spec.lod, time_s: p.time_s, overlays: &spec.overlays, caption: None, ids: spec.ids, hud: true, fill: None }, Some(spec.gi), spec.form),
            (Job::Mark { frame: spec, .. }, _) => (ViewOpts { include_ui: spec.include_ui, lod: spec.lod, time_s: p.time_s, overlays: &spec.overlays, caption: None, ids: spec.ids, hud: true, fill: None }, None, crate::sight_tools::Form::default()),
            (Job::Diff { .. }, State::Diff { view: v, .. }) => (ViewOpts { include_ui: v.include_ui, lod: v.lod, time_s: p.time_s, overlays: &v.overlays, caption: None, ids: v.ids, hud: true, fill: None }, None, crate::sight_tools::Form::default()),
            (Job::Query(q), _) => (ViewOpts { include_ui: view.include_ui, lod: q.frame.lod, time_s: p.time_s, overlays: &none, caption: None, ids: true, hud: true, fill: None }, None, crate::sight_tools::Form::default()),
            // The window hides the game's interface while the editor is open: the pick sees what the human does.
            (Job::Pick(_), _) => (ViewOpts { include_ui: true, lod: None, time_s: p.time_s, overlays: &none, caption: None, ids: true, hud: false, fill: None }, None, crate::sight_tools::Form::default()),
            (Job::Orbit(o), State::Orbit(run)) => {
                let (_, az) = orbit::view(run.center, run.distance, o.elevation_deg, o.start_deg, at as u32, o.views, o.fov_y_deg);
                let caption = format!("{} az {}", at + 1, az.round());
                (ViewOpts { include_ui: o.include_ui, lod: o.lod, time_s: p.time_s, overlays: &o.overlays, caption: Some(caption), ids: true, hud: true, fill: None }, None, o.form)
            }
            _ => {
                let _ = p.to.reply.send(ToolOutput::error("internal: a capture job and its state disagree"));
                return;
            }
        };
        // How a sight_frame shows baked GI (lit, off, irradiance, probes): for this render only.
        if let Some(g) = gi {
            self.gi.capture_view = g;
            self.gi.marks_drawn = (0, 0);
        }
        // And as what form view (the world, unless the call asked for one).
        self.captures.form = (!form.is_world()).then(|| (form.view, form.scale_for(&camera)));
        let submitted = self.render_view_submit(camera, w, h, &opts);
        self.captures.form = None;
        self.gi.capture_view = crate::gi::CaptureView::Lit;
        let submitted = match submitted {
            Ok(s) => s,
            Err(e) => {
                let _ = p.to.reply.send(ToolOutput::error(e));
                return;
            }
        };
        let taken = self.capture_taken(&p, &submitted, at);
        match submitted {
            Submitted::Flying(view) => self.captures.flight = Some(Flight { view, pending: p, taken }),
            Submitted::Done(r) => self.capture_landed(p, taken, *r),
        }
    }

    /// What a capture's answer needs from the world as its view was drawn.
    fn capture_taken(&mut self, p: &Pending, s: &Submitted, at: usize) -> Taken {
        let _z = profile::zone("sight.taken");
        let (stats, two_d, dropped) = s.facts();
        let first = at == 0;
        let wait = if matches!(p.job, Job::Frame(_)) { p.wait } else { CaptureWait { presented_at: None, parked_at: None, cut_by_deadline: false } };
        let building = if matches!(p.job, Job::Frame(_) | Job::Orbit(_)) { p.building } else { 0 };
        let warnings = if first && !matches!(p.job, Job::Query(_) | Job::Pick(_)) { self.capture_warnings(stats, two_d, dropped, wait, building, &p.views[at]) } else { Vec::new() };
        let mut taken = Taken { warnings, problems: Vec::new(), gi: None, ids: Vec::new(), tick: self.clock.tick() };
        match &p.job {
            Job::Frame(spec) => {
                taken.problems = self.luts.warning(self.world.env()).into_iter().chain(self.models.pipeline_failure_json()).collect();
                // Only when the world has GI volumes: other worlds keep their packets as they were.
                let lib = self.library.lock();
                taken.gi = self.gi.packet_json(&self.world, &lib, spec.gi);
            }
            // Every entity's id (to tell new entities from old ones at a diff).
            Job::Mark { .. } => taken.ids = self.world.entities().map(|e| e.id).collect(),
            Job::Diff { name, .. } => {
                // Which of the mark's subjects still exist (removed ones are listed as such).
                let key = self.swarm.key(&p.session);
                if let Some(mark) = self.marks.get(&key, name) {
                    taken.ids = mark.data.subjects.keys().copied().filter(|id| self.world.get(*id).is_some()).collect();
                }
            }
            _ => {}
        }
        taken
    }

    /// Captures' share of a loop pass (see the module docs): lands the view in flight once the GPU
    /// has finished it (waiting a bounded time), starts the next waiting capture, and again while
    /// the pass's share lasts. Each step is timed against the render budget under its call.
    pub(crate) fn pump_captures(&mut self) {
        // Flash feeds that ran on workers since (the latest report, failures as events).
        self.flash.absorb();
        let (share, most) = if self.windowed { (WINDOW_PASS, WINDOW_WAIT) } else { (HEADLESS_PASS, HEADLESS_WAIT) };
        let end = Instant::now() + share;
        self.captures.last_pass = PassEnd::Idle;
        loop {
            if let Some(f) = self.captures.flight.take() {
                let wait = end.saturating_duration_since(Instant::now()).min(most);
                if !self.flight_step(f, wait, end) {
                    // Still on the GPU (`flight_step` says so): polled again next pass. Kept back
                    // for any other reason, the pass was wasted.
                    if self.captures.last_pass != PassEnd::Gpu {
                        self.captures.last_pass = PassEnd::Held;
                    }
                    return;
                }
            } else if self.captures.waiting.is_empty() {
                return;
            } else {
                self.captures.warming = false;
                if !self.start_next(end) {
                    self.captures.last_pass = if std::mem::take(&mut self.captures.warming) { PassEnd::Slice } else { PassEnd::Held };
                    return;
                }
            }
            if std::mem::take(&mut self.captures.end_pass) {
                self.captures.last_pass = PassEnd::Slice;
                return;
            }
            if Instant::now() >= end {
                self.captures.last_pass = PassEnd::Time;
                return;
            }
        }
    }

    /// One step of the view in flight: once the GPU is done (waiting at most `wait`), its counts
    /// and pixels, then (time left before `end`) the rest of its capture. False while the GPU is
    /// still rendering it.
    fn flight_step(&mut self, mut f: Flight, wait: Duration, end: Instant) -> bool {
        let trace = f.pending.trace;
        let _c = trace.enter();
        if !f.view.read() {
            let started = Instant::now();
            let settled = self.gpu.settle_offscreen_within(Some(wait));
            if matches!(settled, Ok(false)) {
                if !wait.is_zero() {
                    self.engine_work_done(trace.call, Some(crate::schedule::Class::Render), "capture_wait", started.elapsed());
                }
                self.captures.flight = Some(f);
                self.captures.last_pass = PassEnd::Gpu;
                return false;
            }
            // (Its subjects: the worker's if it got to them first, else projected as the view
            // lands. It never waits passes for a worker: `Engine::view_subjects`.)
            // An editor pick needs neither the pixels nor the ID counts: it lands now and reads
            // only the pixels it picks from (`land_flight`).
            let pick = matches!(f.pending.state, State::Pick);
            let read = {
                let _z = profile::zone("capture_read");
                settled.map_err(|e| gpu_failure("render", &e, self.windowed)).and_then(|_| if pick { Ok(()) } else { self.render_view_read(&mut f.view) })
            };
            self.engine_work_done(trace.call, Some(crate::schedule::Class::Render), "capture_read", started.elapsed());
            if let Err(e) = read {
                let _ = f.pending.to.reply.send(ToolOutput::error(e));
                return true;
            }
            if !pick && Instant::now() >= end {
                // The rest on the next pass (a big view's packet and copy).
                self.captures.flight = Some(f);
                return true;
            }
        }
        let started = Instant::now();
        {
            let _z = profile::zone("capture_land");
            self.land_flight(f, Ok(()));
        }
        self.engine_work_done(trace.call, Some(crate::schedule::Class::Render), "capture_land", started.elapsed());
        true
    }

    /// Starts the next waiting capture that can go (or the next view of one): false when none can
    /// this pass (the subject cache is being read again, or every waiting call's owner is out of
    /// room on the workers).
    fn start_next(&mut self, end: Instant) -> bool {
        // The subject cache first, if it must read every entity again (after a new world or
        // a change to every entity): a slice a pass, the captures waiting.
        let started = Instant::now();
        let warm = {
            let _z = profile::zone("sight.subject_cache");
            // Headless, the rest of the pass's share (no window waits on it): a rebuild that reads
            // 20,000 entities takes a few passes, not dozens (review of 07f3276: churn latency).
            let slice = if self.windowed { WARM_SLICE } else { end.saturating_duration_since(started).max(WARM_SLICE) };
            self.subject_cache.warm(&self.world, self.world_generation, slice)
        };
        if !warm {
            self.engine_work_done(0, None, "subject_cache", started.elapsed());
            self.captures.warming = true;
            return false;
        }
        let Some(next) = self.next_waiting() else { return false };
        let started = Instant::now();
        match next {
            Waiting::Start(s) => {
                let Start { job, session, wait, reply, call, trace, deadline, views } = *s;
                let _c = trace.enter();
                self.captures.views_hint = views;
                self.start_capture(job, &session, wait, reply, call, trace, deadline);
                self.engine_work_done(trace.call, Some(crate::schedule::Class::Render), "capture_start", started.elapsed());
            }
            Waiting::Next(p) => {
                let trace = p.trace;
                let _c = trace.enter();
                self.capture_submit(*p);
                self.engine_work_done(trace.call, Some(crate::schedule::Class::Render), "capture_view", started.elapsed());
            }
        }
        true
    }

    /// The first waiting capture that may go now. Calls past their deadline are answered and
    /// dropped (the caller has given up: rendering them would only cost the others). A call not
    /// started whose owner has no room left on the workers stays queued (its render would end in
    /// a refusal) while later ones go.
    fn next_waiting(&mut self) -> Option<Waiting> {
        let now = Instant::now();
        let mut i = 0;
        while i < self.captures.waiting.len() {
            let late = match &self.captures.waiting[i] {
                Waiting::Start(s) => now > s.deadline,
                Waiting::Next(p) => now > p.deadline,
            };
            if late {
                let out = ToolOutput::error("skipped: the capture waited for the GPU past the call's deadline; it was not rendered");
                let _ = match self.captures.waiting.remove(i) {
                    Some(Waiting::Start(s)) => s.reply.send(out),
                    Some(Waiting::Next(p)) => p.to.reply.send(out),
                    None => false,
                };
                continue;
            }
            let room = match &self.captures.waiting[i] {
                Waiting::Start(s) => match capture_bytes(&s.job) {
                    Some(bytes) => self.workers_room(&self.capture_owner(&s.session), s.call, bytes).is_ok(),
                    None => true,
                },
                Waiting::Next(_) => true,
            };
            if room {
                return self.captures.waiting.remove(i);
            }
            i += 1;
        }
        None
    }

    /// Lands the view in flight now, waiting for the GPU if it must: before any other offscreen
    /// render (they share the ID buffers and the cached targets), and at shutdown.
    pub(crate) fn land_capture_flight(&mut self) {
        if let Some(f) = self.captures.flight.take() {
            let settled = self.gpu.settle_offscreen(true);
            let _c = f.pending.trace.enter();
            self.land_flight(f, settled.map(|_| ()));
        }
        // A step job's view too (it shares the targets and the ID buffers).
        if let Some((slot, v)) = self.captures.step_flight.take() {
            let landed = self.gpu.settle_offscreen(true).map_err(|e| gpu_failure("render", &e, self.windowed)).and_then(|_| self.render_view_land(*v));
            // Into its job's slot; a job that ended (timed out, its session closed) dropped its
            // slot, and its tile goes now.
            if let Some(slot) = slot.upgrade() {
                *slot.borrow_mut() = Some(landed);
            }
        }
    }

    /// Every waiting capture is answered: the engine is stopping.
    pub(crate) fn captures_shutting_down(&mut self) {
        self.land_capture_flight();
        for w in std::mem::take(&mut self.captures.waiting) {
            let out = ToolOutput::error("the engine is shutting down; the call was not completed");
            let _ = match w {
                Waiting::Start(s) => s.reply.send(out),
                Waiting::Next(p) => p.to.reply.send(out),
            };
        }
    }

    fn land_flight(&mut self, f: Flight, settled: Result<(), specula_gpu::GpuError>) {
        self.captures.landed_views += 1;
        let Flight { view, pending, taken } = f;
        if let (Job::Pick(spec), State::Pick) = (&pending.job, &pending.state) {
            // The IDs are read before anything else renders offscreen (it would overwrite them):
            // only the pixels picked from (a click's few, a box's), and for a box each entity's
            // whole visible count (counted in place, no copy).
            let r = crate::editor_engine::pick_read_rect(spec);
            let read = settled.and_then(|()| self.gpu.read_ids_rect(view.target, r)).and_then(|img| {
                let totals: Vec<u64> = if spec.click {
                    Vec::new()
                } else {
                    self.gpu.id_coverage(view.target, view.entities.len() as u32)?.per_id.iter().map(|c| c.opaque_pixels).collect()
                };
                Ok((img, totals))
            });
            let out = match read {
                Ok((img, totals)) => ToolOutput::text(crate::editor_engine::pick_ids(spec, &img, [r[0], r[1]], &view.entities, &totals).to_string()),
                Err(e) => ToolOutput::error(gpu_failure("pick", &e, self.windowed)),
            };
            let _ = pending.to.reply.send(out);
            return;
        }
        let rendered = match settled {
            Ok(()) => self.render_view_land(*view),
            Err(e) => Err(gpu_failure("render", &e, self.windowed)),
        };
        match rendered {
            Ok(r) => self.capture_landed(pending, taken, r),
            Err(e) => {
                let _ = pending.to.reply.send(ToolOutput::error(e));
            }
        }
    }

    /// A view's pixels are in: its capture's answer, or (an orbit) its next view.
    fn capture_landed(&mut self, mut p: Pending, mut taken: Taken, mut r: Rendered) {
        taken.warnings.extend(self.validation_warnings());
        if let State::Query { .. } = p.state {
            // Its pixels go to no encode: the flash watch's feed gets them on a worker.
            if let Some(feed) = r.feed.take() {
                self.flash_job(feed, std::mem::take(&mut r.rgba), p.views[0].width, p.views[0].height);
            }
        }
        match p.state {
            State::Frame => {
                let Job::Frame(spec) = p.job else { unreachable!("state follows the job") };
                self.capture_frame_answer(spec, p.time_s, &p.session, &p.views[0], taken, r, p.to);
            }
            State::Mark => {
                let Job::Mark { name, frame } = p.job else { unreachable!("state follows the job") };
                self.compare_mark_answer(&name, frame, &p.session, &p.views[0], taken, r, p.to);
            }
            State::Diff { marked_frame, marked_tick, view } => {
                let Job::Diff { name, threshold, max_subjects } = p.job else { unreachable!("state follows the job") };
                self.compare_diff_answer(&name, threshold, max_subjects, &p.session, (marked_frame, marked_tick, view), taken, r, p.to);
            }
            State::Query { id, out } => {
                let answer = query_answer(id, out, &p.views[0], r);
                let _ = p.to.reply.send(answer);
            }
            // (Answered in `land_flight`, from the ID image.)
            State::Pick => {
                let _ = p.to.reply.send(ToolOutput::error("internal: a pick landed without its ID image"));
            }
            State::Orbit(ref mut run) => {
                let Job::Orbit(o) = &p.job else { unreachable!("state follows the job") };
                if run.held.is_none() {
                    // Its sheet, now that its views render (charged until its encode job holds it).
                    let (sw, sh) = run.sheet_size;
                    run.held = Some(self.workers.hold(u64::from(sw) * u64::from(sh) * 4));
                    run.sheet = [24u8, 26, 30, 255].repeat((sw * sh) as usize);
                }
                let i = run.next;
                orbit_view(run, o, &p.views[i], i, taken, &r);
                // The tile is on the sheet: its pixels go to the flash watch's feed on a worker.
                if let Some(feed) = r.feed.take() {
                    self.flash_job(feed, r.rgba, p.views[i].width, p.views[i].height);
                }
                run.next += 1;
                if run.next < p.views.len() {
                    // Its views go back to back (one sheet at a time; main rendered them in one call).
                    self.captures.waiting.push_front(Waiting::Next(Box::new(p)));
                    return;
                }
                let Pending { job: Job::Orbit(o), session, to, time_s, state: State::Orbit(run), .. } = p else { unreachable!("matched above") };
                self.orbit_answer(&o, time_s, &session, *run, to);
            }
        }
    }

    /// sight_frame's packet and answer (encoded, written and answered on a worker).
    #[allow(clippy::too_many_arguments)]
    fn capture_frame_answer(&mut self, spec: FrameSpec, time_s: f64, session: &SessionInfo, view: &ViewReq, taken: Taken, r: Rendered, to: Answer) {
        let (w, h) = (spec.width, spec.height);
        let packet_zone = profile::zone("sight.packet");
        let png = self.next_capture_path();
        let planned_packet = png.with_extension("json");
        let on_screen = r.subjects.iter().filter(|s| s.on_screen).count();
        let mut camera = view.camera.to_json();
        if let Json::Obj(src) = &view.source {
            for (k, v) in src {
                camera.set(k.as_str(), v.clone());
            }
        }
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("frame", Json::from(self.frames_captured)),
            ("requested_by", Json::from(self.swarm.agent_name(session))),
            // (Set once the PNG is written.)
            ("image", Json::Null),
            ("camera", camera),
            ("time_s", Json::from(time_s)),
            ("include_ui", Json::from(spec.include_ui)),
            ("models", r.stats.to_json()),
            ("lights", Json::from(r.lights)),
            ("id_buffer", r.ids.clone()),
            ("subjects_total", Json::from(r.subjects.len())),
            ("subjects_on_screen", Json::from(on_screen)),
            // (Every subject, set on the worker: 20,000 of them took about 100 ms to make here.)
            ("subjects", Json::Null),
            ("warnings", Json::Null),
            // Structured problems (objects), next to the plain-text warnings.
            ("problems", Json::Arr(taken.problems)),
        ]);
        if let Some(o) = &r.overlays {
            packet.set("overlays", o.clone());
        }
        if !r.streaming.is_null() {
            packet.set("streaming", r.streaming.clone());
        }
        if !spec.form.is_world() {
            packet.set("form", spec.form.packet_json(spec.form.scale_for(&view.camera)));
        }
        if let Some(mut g) = taken.gi {
            if spec.form.is_world() {
                crate::gi::drawn_json(&mut g, spec.gi, r.gi_drawn, r.gi_error.as_deref());
            } else {
                // (Not "its pipelines were not ready": nothing failed.)
                g.set("drawn", Json::from(false));
                g.set("not_drawn", Json::from("a form view shows shape alone: no baked light is drawn"));
            }
            if let Some((scale, start)) = r.gi_stretch {
                // The irradiance view's own stretch: shown luminance = 0.03 + (irradiance
                // luminance - start) x scale before the tone curve (the probes' 5th to 95th
                // percentiles over 0.03..0.45).
                let r4 = |x: f32| Json::from((f64::from(x) * 1e4).round() / 1e4);
                g.set("irradiance_scale", r4(scale));
                g.set("irradiance_start", r4(start));
            }
            packet.set("gi", g);
        }
        // Only when the frame had any: worlds without effects keep their packets as they were.
        if r.effects.get("effects").and_then(Json::as_array).is_some_and(|a| !a.is_empty())
            || r.effects.get("decals").and_then(Json::as_array).is_some_and(|a| !a.is_empty())
            || r.effects.get("frame").and_then(|f| f.get("instances")).and_then(Json::as_u64).is_some_and(|n| n > 0)
        {
            packet.set("effects", r.effects.clone());
        }
        let (subject_filter, max_subjects, ids) = (spec.subjects, spec.max_subjects, spec.ids);
        let filter = (subject_filter != SubjectFilter::All).then(|| subject_filter.name());
        let (subjects, drawn) = (r.subjects, r.drawn);
        let warnings = taken.warnings;
        drop(packet_zone);
        let (rgba, feed) = (r.rgba, r.feed);
        let bytes = rgba.len() as u64;
        // The flash watch's feed of these pixels runs on the worker too, before the encode.
        let make = move || {
            let report = feed.map(|f| {
                let _z = profile::zone("sight.flash");
                f.run(&rgba, w, h)
            });
            ((w, h, rgba), report)
        };
        self.offload_capture_made(session, to, bytes, make, png, "sight", move |flash, png_bytes, saved, more| {
            // (Where the packet had it when the engine thread fed the watch.)
            if let Some(f) = flash.filter(|f| f.samples >= 2 || f.over) {
                packet.set("flash", f.to_json());
            }
            let all: Vec<Json> = subjects.iter().map(Subject::full).collect();
            // The packet must agree with its own ID buffer (a monitor, docs/design/TRACE.md).
            if ids {
                crate::trace_engine::report_packet(all.iter(), w, h, drawn.as_ref(), Some(&planned_packet));
            }
            packet.set("subjects", Json::Arr(all));
            // The response carries the most relevant subjects only; agents' context is precious.
            let filtered: Vec<Subject> = subjects.iter().filter(|s| passes(s, subject_filter)).cloned().collect();
            let (chosen, omitted) = sight::most_relevant(&filtered, max_subjects);
            let omitted = omitted + (subjects.len() - filtered.len());
            let mut warnings = warnings;
            warnings.extend(more);
            let packet_path = saved.as_ref().map(|p| p.with_extension("json"));
            let show = |p: Option<&PathBuf>| -> Json { p.map(|p| p.display().to_string()).into() };
            let image = Json::obj([
                ("width", Json::from(w)),
                ("height", Json::from(h)),
                ("png_bytes", Json::from(png_bytes.len())),
                ("path", show(saved.as_ref())),
                ("packet_path", show(packet_path.as_ref())),
            ]);
            packet.set("image", image);
            packet.set("warnings", Json::Arr(warnings.clone()));
            // The full packet is saved next to its PNG so a verifier can re-check both later.
            if let Some(path) = &packet_path {
                if let Err(e) = write_packet(path, packet.to_pretty()) {
                    warnings.push(Json::from(format!("packet not saved to disk: {e}")));
                    packet.set("warnings", Json::Arr(warnings));
                }
            }
            packet.set("subjects", Json::Arr(chosen));
            packet.set("subjects_omitted", Json::from(omitted));
            if let Some(f) = filter {
                packet.set("subjects_filter", Json::from(f));
            }
            ToolOutput {
                content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_pretty())],
                is_error: false,
            }
        });
    }

    /// sight_orbit's answer, once every view is on the sheet.
    fn orbit_answer(&mut self, o: &OrbitSpec, time_s: f64, session: &SessionInfo, run: OrbitRun, to: Answer) {
        let OrbitRun { center, radius, path, distance, sheet_size: (sw, sh), sheet, held, views: out_views, mut warnings, overlays, started, .. } = run;
        // The encode job charges the sheet from here on.
        drop(held);
        if (self.clock.tick(), self.world.revision()) != started {
            // Its views render one per loop pass: the world may have moved on between them.
            warnings.push(Json::from(format!(
                "the world changed while the orbit rendered (tick {} to {}): later views show it later",
                started.0,
                self.clock.tick()
            )));
        }
        let png = self.next_capture_path();
        let frame = self.frames_captured;
        let requested_by = self.swarm.agent_name(session);
        let (elevation_deg, fov_y_deg, grid) = (o.elevation_deg, o.fov_y_deg, orbit::grid(o.views));
        // (Every view of an orbit is as far from its centre: one scale for the sheet.)
        let form = (!o.form.is_world()).then(|| o.form.packet_json(o.form.scale_for(&orbit::view(center, distance, o.elevation_deg, o.start_deg, 0, o.views, o.fov_y_deg).0)));
        self.offload_capture(session, to, (sw, sh, sheet), png, "sight", move |png_bytes, saved, more| {
            warnings.extend(more);
            let packet_path = saved.as_ref().map(|p| p.with_extension("json"));
            let mut packet = Json::obj([
                ("schema", Json::from(sight::SCHEMA)),
                ("kind", Json::from("orbit")),
                ("frame", Json::from(frame)),
                ("requested_by", Json::from(requested_by)),
                (
                    "image",
                    Json::obj([
                        ("width", Json::from(sw)),
                        ("height", Json::from(sh)),
                        ("png_bytes", Json::from(png_bytes.len())),
                        ("path", saved.as_ref().map(|p| p.display().to_string()).into()),
                        ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
                        ("grid", Json::arr([grid.0, grid.1].map(Json::from))),
                    ]),
                ),
                ("target", Json::obj([("path", Json::from(path)), ("center", world::vec3_json(center)), ("radius_m", Json::from(radius))])),
                ("distance_m", Json::from(distance)),
                ("elevation_deg", Json::from(elevation_deg)),
                ("fov_y_deg", Json::from(fov_y_deg)),
                ("time_s", Json::from(time_s)),
                ("views", Json::Arr(out_views)),
                ("warnings", Json::Arr(warnings)),
            ]);
            if let Some(form) = form {
                packet.set("form", form);
            }
            if let Some(ov) = overlays {
                packet.set("overlays", ov);
            }
            save_packet(&mut packet, packet_path.as_ref());
            ToolOutput {
                content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_string())],
                is_error: false,
            }
        });
    }

    /// sight_compare mark: the "before" is kept here; its PNG is encoded and written on a worker.
    #[allow(clippy::too_many_arguments)]
    fn compare_mark_answer(&mut self, name: &str, spec: FrameSpec, session: &SessionInfo, view: &ViewReq, taken: Taken, r: Rendered, to: Answer) {
        let png = self.next_capture_path();
        // Only what was on screen, plus every entity's id (to tell new entities from old ones).
        let subjects = r.subjects.iter().filter(|s| s.on_screen).map(SubjectState::of).collect();
        let mark_view = MarkView {
            camera: view.camera,
            width: spec.width,
            height: spec.height,
            include_ui: spec.include_ui,
            lod: spec.lod,
            time_s: spec.time_s,
            overlays: spec.overlays,
            ids: spec.ids,
        };
        let key = self.swarm.key(session);
        let on_screen = r.subjects.iter().filter(|s| s.on_screen).count();
        // Shared with the encode (no copy of the pixels here).
        let pixels = std::sync::Arc::new(r.rgba);
        // The mark keeps the PNG's planned path (informational: a diff compares its pixels).
        let data = MarkData {
            rgba: std::sync::Arc::clone(&pixels),
            subjects: std::sync::Arc::new(subjects),
            ids: std::sync::Arc::new(taken.ids),
            frame: self.frames_captured,
            tick: taken.tick,
            png_path: Some(png.display().to_string()),
            generation: self.world_generation,
        };
        let evicted = self.marks.put(&key, name, mark_view, data);
        let mut out = Json::obj([
            ("marked", Json::from(name)),
            ("frame", Json::from(self.frames_captured)),
            ("tick", Json::from(taken.tick)),
            ("camera", view.camera.to_json()),
            ("size", Json::arr([spec.width, spec.height].map(Json::from))),
            ("time_s", spec.time_s.map_or(Json::from("live"), Json::from)),
            ("subjects_on_screen", Json::from(on_screen)),
            ("path", Json::Null),
            ("evicted", Json::arr(evicted.into_iter().map(Json::from))),
            ("warnings", Json::Null),
        ]);
        let warnings = taken.warnings;
        let (w, h) = (spec.width, spec.height);
        let bytes = pixels.len() as u64;
        let feed = r.feed;
        let image = move || {
            if let Some(f) = feed {
                let _z = profile::zone("sight.flash");
                f.run(&pixels, w, h);
            }
            ((w, h, std::sync::Arc::unwrap_or_clone(pixels)), ())
        };
        self.offload_capture_made(session, to, bytes, image, png, "sight", move |(), _, saved, more| {
            let mut warnings = warnings;
            warnings.extend(more);
            out.set("path", saved.map(|p| p.display().to_string()).into());
            out.set("warnings", Json::Arr(warnings));
            ToolOutput::text(out.to_pretty())
        });
    }

    /// sight_compare diff.
    #[allow(clippy::too_many_arguments)]
    fn compare_diff_answer(&mut self, name: &str, threshold: u8, max_subjects: usize, session: &SessionInfo, marked: (u64, u64, MarkView), taken: Taken, r: Rendered, to: Answer) {
        let (marked_frame, marked_tick, view) = marked;
        let key = self.swarm.key(session);
        // The mark may have gone (or been replaced) while the view rendered.
        let Some(mark) = self.marks.get(&key, name).map(|m| &m.data).filter(|d| d.frame == marked_frame) else {
            let _ = to.reply.send(mark_missing(name));
            return;
        };
        let (w, h) = (view.width, view.height);
        let mut warnings = taken.warnings;
        // The diff, its heat panel, the subjects' changes and the sheet are made on the worker
        // (at 2048 x 2048 they held the engine thread about 190 ms).
        let (before, before_subjects, mut before_ids) = (std::sync::Arc::clone(&mark.rgba), std::sync::Arc::clone(&mark.subjects), std::sync::Arc::clone(&mark.ids));
        let mut alive = taken.ids;
        if mark.generation != self.world_generation {
            // The world was replaced since the mark: its ids name other entities now. Every
            // subject then is gone and every one now is new (never matched by a reused id).
            before_ids = std::sync::Arc::new(Vec::new());
            alive.clear();
            warnings.push("the world was replaced since the mark (world_load or save_read): its subjects are listed as removed and today's as added, never matched by id".into());
        }
        let after_rgba = r.rgba;
        let after_subjects = r.subjects;
        let bytes = (before.len() + after_rgba.len()) as u64;
        let feed = r.feed;
        let make = move || {
            if let Some(f) = feed {
                let _z = profile::zone("sight.flash");
                f.run(&after_rgba, w, h);
            }
            let d = compare::diff(&before, &after_rgba, w, h, threshold);
            let heat = compare::heat(&after_rgba, &d, threshold);
            let after: BTreeMap<u64, SubjectState> = after_subjects.iter().filter(|s| s.on_screen).map(SubjectState::of).collect();
            let subjects = compare::subject_diff(&before_subjects, &before_ids, &after, &|id| alive.binary_search(&id).is_ok(), 2, max_subjects);
            let (sheet, sw, sh, factor) = compare::side_by_side(&[&before, &after_rgba, &heat], w, h, 4, orbit::MAX_SHEET);
            ((sw, sh, sheet), (sw, sh, factor, d.to_json(), subjects))
        };
        let png = self.next_capture_path();
        let frame = self.frames_captured;
        let ticks_since = taken.tick.saturating_sub(marked_tick);
        let (name, camera) = (name.to_owned(), view.camera.to_json());
        self.offload_capture_made(session, to, bytes, make, png, "sight", move |(sw, sh, factor, pixels, subjects), png_bytes, saved, more| {
            warnings.extend(more);
            let packet_path = saved.as_ref().map(|p| p.with_extension("json"));
            let mut packet = Json::obj([
                ("schema", Json::from(sight::SCHEMA)),
                ("kind", Json::from("compare")),
                ("name", Json::from(name)),
                ("frame", Json::from(frame)),
                ("marked_frame", Json::from(marked_frame)),
                ("ticks_since_mark", Json::from(ticks_since)),
                ("camera", camera),
                (
                    "image",
                    Json::obj([
                        ("width", Json::from(sw)),
                        ("height", Json::from(sh)),
                        ("panels", Json::arr(["before", "after", "heat"].map(Json::from))),
                        ("scale", Json::from(1.0 / factor as f64)),
                        ("path", saved.as_ref().map(|p| p.display().to_string()).into()),
                        ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
                    ]),
                ),
                ("threshold", Json::from(u32::from(threshold))),
                ("pixels", pixels),
                ("subjects", subjects),
                ("warnings", Json::Arr(warnings)),
            ]);
            save_packet(&mut packet, packet_path.as_ref());
            ToolOutput {
                content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_pretty())],
                is_error: false,
            }
        });
    }

    /// sight_query before its render: the entity, or the answer when it can't be in the frame.
    fn query_start(&self, q: &QuerySpec, view: &ViewReq) -> Result<(u64, Json), ToolOutput> {
        let id = self.world.resolve(&q.key).map_err(|e| world_error(&e))?;
        let e = self.world.get(id).expect("resolved");
        let path = self.world.path_of(id);
        let mut out = Json::obj([("id", Json::from(id)), ("path", Json::from(path.as_str()))]);
        let reason = if e.local_bounds().is_none() {
            Some(if e.shape == world::Shape::Group { "a group draws nothing (query its children)" } else { "no geometry yet (model not built)" })
        } else if !sight::in_frame(e, view.include_ui) {
            Some(if e.shown() { "on the ui layer (pass include_ui)" } else { "hidden (visible: false here or above)" })
        } else {
            None
        };
        match reason {
            Some(r) => {
                out.set("drawn", Json::from(false));
                out.set("reason", Json::from(r));
                out.set("on_screen", Json::from(false));
                out.set("camera", view.camera.to_json());
                Err(ToolOutput::text(out.to_string()))
            }
            None => Ok((id, out)),
        }
    }
}

/// One orbit view onto the sheet, and its entry in the packet.
fn orbit_view(run: &mut OrbitRun, o: &OrbitSpec, v: &ViewReq, i: usize, taken: Taken, r: &Rendered) {
    let (tw, th) = run.tile;
    let (_, az) = orbit::view(run.center, run.distance, o.elevation_deg, o.start_deg, i as u32, o.views, o.fov_y_deg);
    if i == 0 {
        run.warnings = taken.warnings;
    } else {
        // Validation messages of later views.
        run.warnings.extend(taken.warnings);
    }
    if run.overlays.is_none() {
        run.overlays = r.overlays.as_ref().and_then(|j| j.get("drawn")).map(|d| Json::obj([("drawn", d.clone()), ("per_view", Json::from("each view's own counts are in views[].overlays"))]));
    }
    let (x, y) = orbit::tile_origin(i as u32, o.views, tw, th);
    compare::blit(&mut run.sheet, run.sheet_size.0, &r.rgba, tw, th, x, y);
    let on: Vec<Subject> = r.subjects.iter().filter(|s| s.on_screen).cloned().collect();
    let (chosen, omitted) = sight::most_relevant(&on, o.max_subjects.max(1));
    let chosen: Vec<Json> = if o.max_subjects == 0 { Vec::new() } else { chosen.into_iter().map(compact_subject).collect() };
    let mut view = Json::obj([
        ("view", Json::from(i + 1)),
        ("azimuth_deg", Json::from(az)),
        ("tile", Json::arr([x, y, tw, th].map(Json::from))),
        ("eye", world::vec3_json(v.camera.eye)),
        ("subjects_on_screen", Json::from(on.len())),
        ("subjects", Json::Arr(chosen)),
        ("subjects_omitted", Json::from(if o.max_subjects == 0 { on.len() } else { omitted })),
    ]);
    if let Some(ov) = &r.overlays {
        // What was drawn is listed once (top-level `overlays`); each view has its counts.
        view.set("overlays", overlay_counts(ov));
    }
    run.views.push(view);
}

/// sight_query's answer from its frame.
fn query_answer(id: u64, mut out: Json, view: &ViewReq, r: Rendered) -> ToolOutput {
    let path = out.get("path").and_then(Json::as_str).unwrap_or_default().to_owned();
    let Some(s) = r.subjects.iter().find(|s| s.id == id) else {
        return ToolOutput::error(format!("{path} was not in the frame it was queried in"));
    };
    // Drawn or not, from what the frame drew (its draw lists, the ID pass, streaming), never
    // from the world: an entity behind the camera is submitted but reaches no pixel.
    r.record.drawn(id).write(&mut out);
    let full = s.full();
    for key in ["on_screen", "screen", "distance_m", "visible_px", "visible_px_approx", "screen_box_exact", "no_pixels", "occluded", "resident_lods_from"] {
        if let Some(v) = full.get(key) {
            out.set(key, v.clone());
        }
    }
    out.set("image", Json::arr([view.width, view.height].map(Json::from)));
    let mut camera = view.camera.to_json();
    if let Json::Obj(src) = &view.source {
        for (k, v) in src {
            camera.set(k.as_str(), v.clone());
        }
    }
    out.set("camera", camera);
    if let Some(w) = &view.warning {
        out.set("warning", Json::from(w.as_str()));
    }
    ToolOutput::text(out.to_string())
}

/// The capture flight on a real GPU (skipped cleanly without Vulkan): passes per orbit, the
/// sheet's charge, the order captures land in, deadlines, a lost device and shutdown.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Writer;
    use std::sync::mpsc::Receiver;

    fn engine(tag: &str) -> Option<Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test(&format!("flight::{tag}"), "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test(&format!("flight::{tag}"), &e.to_string());
                return None;
            }
        };
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let captures = std::env::temp_dir().join(format!("specula-flight-{tag}-{}", std::process::id()));
        let mut e = Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine");
        let w = Writer::new("t", Some("t"));
        for i in 0..24 {
            let spec = crate::tools::parse_spawn(&Json::parse(&format!(r#"{{"name": "c{i}", "shape": "cube", "position": [{}, 0, {}]}}"#, i % 6, i / 6)).unwrap()).unwrap();
            e.world.commit(&mut e.swarm, 0, w, journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(spec, "t").map(|x| x.id)).unwrap();
        }
        Some(e)
    }

    fn session() -> SessionInfo {
        SessionInfo { id: "flight-test".into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() }
    }

    /// Sends a sight call (`tool` with `args`) as an agent's request would come; its answer.
    fn ask(e: &mut Engine, tool: &str, args: &str, call: u64, deadline: Instant) -> Receiver<ToolOutput> {
        let args = Json::parse(args).unwrap();
        let command = if tool == "sight_frame" {
            Command::Frame(crate::tools::parse_frame(&args).unwrap())
        } else {
            Command::Sight(crate::sight_tools::parse(tool, &args).unwrap().unwrap())
        };
        let (reply, rx) = crate::jobs::channel();
        e.handle_sight(Request { session: session(), call, command, reply, deadline, trace: specula_core::trace::Ctx::current(), via: None });
        rx
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    /// Loop passes (captures, then workers) until every receiver has its answer; the passes taken.
    fn run(e: &mut Engine, rx: &[&Receiver<ToolOutput>], mut each: impl FnMut(&mut Engine)) -> (u32, Vec<ToolOutput>) {
        let start = Instant::now();
        let mut out: Vec<Option<ToolOutput>> = rx.iter().map(|_| None).collect();
        let mut passes = 0;
        while out.iter().any(Option::is_none) {
            e.pump_captures();
            passes += 1;
            each(e);
            e.pump_workers(None);
            for (o, r) in out.iter_mut().zip(rx) {
                if o.is_none() {
                    *o = r.try_recv().ok();
                }
            }
            assert!(start.elapsed() < Duration::from_secs(120), "captures not answered");
            std::thread::sleep(Duration::from_millis(1));
        }
        (passes, out.into_iter().map(Option::unwrap).collect())
    }

    fn packet(out: &ToolOutput) -> Json {
        assert!(!out.is_error, "{:?}", out.content);
        let text = out.content.iter().find_map(|c| if let Content::Text(t) = c { Some(t.as_str()) } else { None }).unwrap();
        Json::parse(text).unwrap()
    }

    fn image(out: &ToolOutput) -> specula_core::png::Image {
        let data = out.content.iter().find_map(|c| if let Content::Image { data_base64, .. } = c { Some(data_base64.as_str()) } else { None }).unwrap();
        specula_core::png::decode(&base64::decode(data).unwrap()).unwrap()
    }

    /// A form view (`sight_frame form`, `sight_orbit form`) draws the same surfaces as shape
    /// alone, against a neutral ground, and its packet says which view it is and what it left
    /// out; the frame after it is the world's again, to the byte.
    #[test]
    fn a_form_view_is_shape_alone_and_its_packet_says_so() {
        let Some(mut e) = engine("form") else { return };
        // Straight at the front of the cube at (2, 0, 3), from +z.
        let view = r#""eye": [2, 0, 9], "target": [2, 0, 3], "width": 128, "height": 128, "fov_y_deg": 30"#;
        let mut call = 0;
        let mut shoot = |e: &mut Engine, more: &str| {
            call += 1;
            let rx = ask(e, "sight_frame", &format!("{{{view}{more}}}"), call, soon());
            let (_, out) = run(e, &[&rx], |_| {});
            (packet(&out[0]), image(&out[0]))
        };
        let px = |i: &specula_core::png::Image, x: u32, y: u32| -> [u8; 3] {
            let at = ((y * i.width + x) * 4) as usize;
            [i.rgba[at], i.rgba[at + 1], i.rgba[at + 2]]
        };
        let (world_packet, world) = shoot(&mut e, "");
        assert!(world_packet.get("form").is_none(), "the world's frame has no form entry");
        // The form views' pipelines are built on a worker when the first form view is asked
        // for, and that view waits for them: no frame compiles one. (Compiled inside the first
        // form capture's render, they held the engine thread 440 ms and left a budget error
        // for the rest of the run: second review of the form views.)
        assert!(!e.gpu.pipeline_variants().form && e.captures.form_waits == 0);
        e.gpu.mark_warm();
        let made = e.gpu.last_frame_stats().pipelines.created;

        let ground = sight::FORM_BACKGROUND.map(|c| (c * 255.0).round() as u8);
        for name in ["clay", "normals", "curvature", "stripes"] {
            let waited = e.captures.form_waits;
            let (p, i) = shoot(&mut e, &format!(r#", "form": "{name}""#));
            // (The first waited for the set, a pass at least; the others found it built.)
            assert_eq!(e.captures.form_waits > waited, name == "clay", "{name}: waited {} passes", e.captures.form_waits - waited);
            let stats = e.gpu.last_frame_stats().pipelines;
            assert!(e.gpu.pipeline_variants().form && (stats.created, stats.created_after_warm) == (made, 0), "{name}: {stats:?}");
            let form = p.get("form").unwrap_or_else(|| panic!("{name}: the packet says it is a form view"));
            assert_eq!(form.get("view").and_then(Json::as_str), Some(name));
            assert!(form.get("not_drawn").and_then(Json::as_str).is_some_and(|t| t.contains("light")), "{name}: {form:?}");
            assert_eq!(form.get("scale_m").is_some(), name == "curvature", "{name}: only curvature has a scale");
            // None of the world's lights is drawn, and the packet does not count any.
            assert_eq!(p.get("lights").and_then(Json::as_u64), Some(0), "{name}: {:?}", p.get("lights"));
            let corner = px(&i, 2, 2);
            assert!(corner.iter().zip(&ground).all(|(a, b)| a.abs_diff(*b) <= 1), "{name}: the ground behind is neutral: {corner:?}");
            assert!(i.rgba != world.rgba, "{name}: not the world's frame");
            // The ID buffer is the world's: what is visible of each subject is counted the same.
            let seen = |p: &Json| -> Vec<(String, u64)> {
                let subjects = p.get("subjects").and_then(Json::as_array).unwrap();
                subjects.iter().map(|s| (s.get("name").and_then(Json::as_str).unwrap_or("").to_string(), s.get("visible_px").and_then(Json::as_u64).unwrap_or(0))).collect()
            };
            assert!(seen(&p) == seen(&world_packet) && seen(&p).iter().any(|(_, px)| *px > 0), "{name}: {:?} against {:?}", seen(&p), seen(&world_packet));
            let centre = px(&i, 64, 64);
            match name {
                // The face looks down +z: (0, 0, 1) as colour, exactly.
                "normals" => assert!(centre[0].abs_diff(128) <= 1 && centre[1].abs_diff(128) <= 1 && centre[2] == 255, "normals: {centre:?}"),
                // A flat face is mid grey.
                "curvature" => assert!(centre.iter().all(|c| c.abs_diff(128) <= 1), "curvature: {centre:?}"),
                // Clay is a lit warm grey, whatever the world's light.
                "clay" => assert!(centre[0] >= centre[2] && centre[1] > 60 && centre[1] < 245, "clay: {centre:?}"),
                _ => assert!(centre[0] == centre[1] && centre[1] == centre[2], "stripes are grey: {centre:?}"),
            }
        }
        // The curvature view's scale: the caller's, or a twentieth of the view's half height.
        let (p, _) = shoot(&mut e, r#", "form": "curvature", "form_scale": 0.25"#);
        assert_eq!(p.get("form").and_then(|f| f.get("scale_m")).and_then(Json::as_f64), Some(0.25));
        let (p, _) = shoot(&mut e, r#", "form": "curvature""#);
        let auto = p.get("form").and_then(|f| f.get("scale_m")).and_then(Json::as_f64).unwrap();
        let expected = 0.05 * 6.0 * f64::from(15f32.to_radians().tan());
        assert!((auto - expected).abs() < 1e-3, "{auto} against {expected}");

        // A form view is for that one frame: the next is the world's again, to the byte.
        let (_, again) = shoot(&mut e, "");
        assert!(again.rgba == world.rgba, "the world's frame is unchanged by the form views before it");

        // An orbit takes it too, for every tile: its sheet has the views' neutral ground in it,
        // which the same orbit of the world has nowhere. (Counted over the sheet: a tile's
        // corner may hold its caption, and the cubes fill most of each tile.)
        let orbit = |e: &mut Engine, more: &str, call: u64| {
            let rx = ask(e, "sight_orbit", &format!(r#"{{"point": [2, 0, 2], "views": 4, "tile_width": 96, "tile_height": 96{more}}}"#), call, soon());
            let (_, out) = run(e, &[&rx], |_| {});
            let sheet = image(&out[0]);
            let on_ground = sheet.rgba.as_chunks::<4>().0.iter().filter(|p| p[..3].iter().zip(&ground).all(|(a, b)| a.abs_diff(*b) <= 1)).count();
            (packet(&out[0]), on_ground, sheet.rgba.len() / 4)
        };
        let (p, on_ground, pixels) = orbit(&mut e, r#", "form": "clay""#, 98);
        assert_eq!(p.get("form").and_then(|f| f.get("view")).and_then(Json::as_str), Some("clay"));
        assert!(on_ground * 50 > pixels, "the orbit's tiles stand on the neutral ground: {on_ground} of {pixels} pixels");
        let (p, on_ground, _) = orbit(&mut e, "", 99);
        assert!(p.get("form").is_none() && on_ground == 0, "the world's orbit has no such ground: {on_ground} pixels");
    }

    /// A 12-view orbit's views go back to back: each lands and the next is submitted in the same
    /// pass, so no pass is wasted. Every pass that ends with views still to render ends because
    /// the GPU is still rendering one, a sliced step took its slice, or the pass's time is used
    /// up ([`PassEnd`]); never with a view waiting and nothing in the way. (This used to assert
    /// "at most views + 2 passes": true on a quiet machine, and a matter of luck in a full test
    /// run, where a view can take longer than one pass waits for the GPU. It failed one gate in
    /// four, and once more after a fix that was said to cure it.) Its sheet is charged to the
    /// workers' byte cap while its views render (released once its encode job holds it).
    #[test]
    fn an_orbits_views_go_back_to_back_and_its_sheet_is_charged() {
        let Some(mut e) = engine("orbit") else { return };
        let landed = e.captures.landed_views;
        let rx = ask(&mut e, "sight_orbit", r#"{"point": [3, 0, 2], "views": 12, "tile_width": 256, "tile_height": 256}"#, 1, soon());
        let (tw, th, sw, sh) = orbit::layout(12, (256, 256));
        assert_eq!((tw, th), (256, 256));
        let sheet = u64::from(sw) * u64::from(sh) * 4;
        let mut charged = Vec::new();
        // Why each pass ended while views were still to render (its answer then waits for the
        // encode only).
        let mut ends = Vec::new();
        let (_, out) = run(&mut e, &[&rx], |e| {
            charged.push(e.workers.held_bytes());
            if e.captures.busy() {
                ends.push(e.captures.last_pass);
            }
        });
        let wasted: Vec<&PassEnd> = ends.iter().filter(|p| !matches!(p, PassEnd::Gpu | PassEnd::Slice | PassEnd::Time)).collect();
        assert!(wasted.is_empty(), "{} of {} passes ended with a view waiting and nothing in the way: {ends:?}", wasted.len(), ends.len());
        assert_eq!(e.captures.landed_views - landed, 12, "every view landed once");
        // Mid-orbit (when a pass ended between its views) its sheet was charged, and only it.
        assert!(charged.iter().all(|&b| b == 0 || b == sheet), "{charged:?} (sheet {sheet})");
        assert_eq!(e.workers.held_bytes(), 0, "released once encoded");
        let p = packet(&out[0]);
        assert_eq!(p.get("views").and_then(Json::as_array).map(<[Json]>::len), Some(12));
        // The same orbit again, with nothing left to warm (the size's images, the subject
        // cache): now only the GPU, or a pass's time running out, may end a pass. (Review R2:
        // an engine that ended the pass after every landed view, so the next view waited a
        // pass with the GPU idle, passed the check above, because a slice is a fair reason
        // the first time.)
        let landed = e.captures.landed_views;
        let again = ask(&mut e, "sight_orbit", r#"{"point": [3, 0, 2], "views": 12, "tile_width": 256, "tile_height": 256}"#, 2, soon());
        let mut ends = Vec::new();
        let _ = run(&mut e, &[&again], |e| {
            if e.captures.busy() {
                ends.push(e.captures.last_pass);
            }
        });
        let early: Vec<&PassEnd> = ends.iter().filter(|p| !matches!(p, PassEnd::Gpu | PassEnd::Time)).collect();
        assert!(early.is_empty(), "{} of {} passes of a warm orbit ended with views waiting and the GPU free: {ends:?}", early.len(), ends.len());
        assert_eq!(e.captures.landed_views - landed, 12, "every view landed once");
        // The charge counts against the cap like a queued job's bytes.
        let held = e.workers.hold(crate::workers::MAX_PENDING_BYTES);
        let owner = e.capture_owner(&session());
        assert!(e.workers_room(&owner, 1, 1).is_err(), "a held sheet fills the cap");
        drop(held);
        assert!(e.workers_room(&owner, 1, 1).is_ok());
    }

    /// Captures land in the order they came, an orbit's views back to back (a frame asked after
    /// it lands after all of them); a call whose deadline passed while it waited is answered
    /// without a render.
    #[test]
    fn captures_land_in_order_and_late_ones_are_dropped() {
        let Some(mut e) = engine("order") else { return };
        let a = ask(&mut e, "sight_frame", r#"{"width": 160, "height": 90}"#, 1, soon());
        let b = ask(&mut e, "sight_orbit", r#"{"point": [3, 0, 2], "views": 4, "tile_width": 128, "tile_height": 128}"#, 2, soon());
        let c = ask(&mut e, "sight_frame", r#"{"width": 160, "height": 90, "eye": [3, 4, 9], "target": [3, 0, 2]}"#, 3, soon());
        let late = ask(&mut e, "sight_frame", r#"{"width": 160, "height": 90}"#, 4, Instant::now());
        let (_, out) = run(&mut e, &[&a, &b, &c, &late], |_| {});
        let frame = |o: &ToolOutput| packet(o).get("frame").and_then(Json::as_u64).unwrap();
        let (fa, fb, fc) = (frame(&out[0]), frame(&out[1]), frame(&out[2]));
        assert!(fa < fb && fb < fc, "landing order: frame {fa}, orbit {fb}, frame {fc}");
        assert!(out[3].is_error, "past its deadline: not rendered");
        let text = format!("{:?}", out[3].content);
        assert!(text.contains("deadline"), "{text}");
    }

    /// A render that ends in a lost device answers its capture `gpu_device_lost`; the next
    /// capture is not left waiting on it. At shutdown the view in flight lands and every waiting
    /// call is answered.
    #[test]
    fn a_lost_device_and_shutdown_answer_every_capture() {
        let Some(mut e) = engine("lost") else { return };
        e.gpu.inject_offscreen_fault();
        let lost = ask(&mut e, "sight_frame", r#"{"width": 160, "height": 90}"#, 1, soon());
        let (_, out) = run(&mut e, &[&lost], |_| {});
        assert!(out[0].is_error);
        let text = format!("{:?}", out[0].content);
        assert!(text.contains("gpu_device_lost"), "{text}");
        // Shutdown with one view in flight and two calls waiting.
        let first = ask(&mut e, "sight_frame", r#"{"width": 160, "height": 90}"#, 2, soon());
        let orbit = ask(&mut e, "sight_orbit", r#"{"point": [3, 0, 2], "views": 4, "tile_width": 128, "tile_height": 128}"#, 3, soon());
        let last = ask(&mut e, "sight_frame", r#"{"width": 160, "height": 90}"#, 4, soon());
        assert!(e.captures.flight.is_some() && e.captures.counts().1 == 2, "one in flight, two waiting");
        e.captures_shutting_down();
        assert!(!e.captures.busy());
        e.workers_drain(Duration::from_secs(30));
        e.pump_workers(None);
        assert!(!first.recv_timeout(Duration::from_secs(30)).unwrap().is_error, "the view in flight lands");
        for rx in [orbit, last] {
            let out = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(out.is_error && format!("{:?}", out.content).contains("shutting down"));
        }
    }
}
