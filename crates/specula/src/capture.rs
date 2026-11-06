//! Sight on the engine thread: `sight_frame`, `sight_orbit`, `sight_compare`, `sight_query` and
//! the named cameras (`camera_*`).
//!
//! Every capture-like call is a [`Job`]. A job first works out the views it will render; when a
//! model it would show is still building, the call is parked (the engine goes on serving every
//! other agent) until the model is built or its wait runs out, exactly like `sight_frame` always
//! did. Each view renders through [`Engine::render_view`]: draw list, lights, subjects, debug
//! overlays (`sight_overlay`), then the offscreen render and readback.

use super::*;
use crate::cameras::CameraError;
use crate::compare::{self, MarkData, MarkView, SubjectState};
use crate::orbit;
use crate::sight::Subject;
use crate::sight_overlay::{self as ov, Kind, Painter};
use crate::sight_tools::{CompareCall, OrbitSpec, OrbitTarget, QuerySpec, SightCall, SubjectFilter};
use specula_core::math::{Mat4, Vec3};
use specula_gpu::OverlayQuad;
use std::collections::BTreeMap;

pub(crate) mod flight;

/// A capture-like call parked until the models it would show are built.
pub(crate) struct CaptureWaiter {
    pub(crate) job: Job,
    pub(crate) session: SessionInfo,
    pub(crate) reply: crate::jobs::Reply,
    /// Rendered then whatever is still building.
    pub(crate) until: Instant,
    pub(crate) wait: CaptureWait,
    /// The call it belongs to (its render is traced under it when served).
    pub(crate) trace: specula_core::trace::Ctx,
    /// The request's call id (0: a playtest's step; see `workers::Offload::agent`).
    pub(crate) call: u64,
    /// The request's deadline (a capture still waiting for the GPU past it is dropped).
    pub(crate) deadline: Instant,
}

/// How a capture waited for models, for its packet.
#[derive(Clone, Copy)]
pub(crate) struct CaptureWait {
    /// The tick the game last brought its presentation up to (`before_capture`).
    pub(crate) presented_at: Option<u64>,
    /// When it was parked (`None`: rendered at once).
    pub(crate) parked_at: Option<Instant>,
    /// It renders because the caller's deadline came, not after the usual wait.
    pub(crate) cut_by_deadline: bool,
}

pub(crate) enum Job {
    Frame(FrameSpec),
    Orbit(OrbitSpec),
    Mark { name: String, frame: FrameSpec },
    Diff { name: String, threshold: u8, max_subjects: usize },
    Query(QuerySpec),
    /// The editor's click or box select: the window's view rendered with IDs, read at a pixel or
    /// a rectangle (docs/design/EDITOR.md, "Selection"; answered by `editor_engine::pick_ids`).
    Pick(PickSpec),
}

/// What an editor pick renders and reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PickSpec {
    pub(crate) camera: Camera,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Pixels `[x0, y0, x1, y1)` read (a click: one pixel, searched around).
    pub(crate) rect: [u32; 4],
    pub(crate) click: bool,
}

/// One view a job renders.
struct ViewReq {
    camera: Camera,
    width: u32,
    height: u32,
    include_ui: bool,
    /// How the camera was chosen, for the packet.
    source: Json,
    warning: Option<String>,
}

/// How one view renders.
pub(crate) struct ViewOpts<'a> {
    pub(crate) include_ui: bool,
    pub(crate) lod: Option<usize>,
    pub(crate) time_s: f64,
    pub(crate) overlays: &'a ov::Spec,
    /// Text in the top-left corner (orbit tiles).
    pub(crate) caption: Option<String>,
    /// Render the ID buffer (visibility per subject).
    pub(crate) ids: bool,
    /// A light added to the world's (filmstrips light their subject from the camera).
    pub(crate) fill: Option<specula_gpu::PointLight>,
    /// With `include_ui`: the hosted game's interface and the engine's screens are drawn too
    /// (false for the editor's pick: the window shows none of them while the editor is open).
    pub(crate) hud: bool,
}

/// One rendered view.
pub(crate) struct Rendered {
    /// This capture's place in the flash watch over its view ([`crate::flashwatch::Feed`]): run
    /// with the pixels where they go (a worker), [`Engine::flash_job`] when nothing else takes
    /// them; `None` with the watch off. Dropped unrun, the view's later captures don't wait.
    pub(crate) feed: Option<crate::flashwatch::Feed>,
    pub(crate) rgba: Vec<u8>,
    pub(crate) subjects: Vec<Subject>,
    pub(crate) stats: LodStats,
    pub(crate) lights: usize,
    pub(crate) dropped_lights: usize,
    /// What the overlays drew (`None`: none asked for).
    pub(crate) overlays: Option<Json>,
    /// The ID buffer's entity count and cost.
    pub(crate) ids: Json,
    /// Problems of the 2D items drawn (missing textures, frames past a sheet, ...).
    pub(crate) two_d_problems: Vec<String>,
    /// What streaming left out or merged (see `streaming`); null when streaming is off.
    pub(crate) streaming: Json,
    /// The effects and decals drawn (see `vfx`): boxes, counts, the frame's budget use.
    pub(crate) effects: Json,
    /// What this frame actually drew (the only source of drawing facts; see `sight::FrameRecord`).
    pub(crate) record: sight::FrameRecord,
    /// The entities the colour pass was given (the packet monitor's independent record).
    pub(crate) drawn: Option<crate::trace_engine::DrawnRecord>,
    /// Whether this render drew baked GI, and why not when its set failed to bind.
    pub(crate) gi_drawn: bool,
    pub(crate) gi_error: Option<String>,
    pub(crate) gi_stretch: Option<(f32, f32)>,
}

/// A view submitted to the GPU and not yet landed ([`Engine::render_view_submit`]): what its
/// packet says about the world, taken when it was drawn.
pub(crate) struct ViewInFlight {
    pub(crate) target: TargetId,
    time_s: f64,
    /// The flash watch's key for the view.
    view_key: u64,
    /// The ID pass ran and its counts are still to be read.
    ids: bool,
    include_ui: bool,
    /// The ID pass's dense ids: `entities[i]` drew as id `i + 1`.
    entities: Vec<u64>,
    /// Interface rectangles that hid the scene in the ID counts, and how many were left out.
    occluders: (usize, usize),
    subjects: Vec<Subject>,
    record: sight::FrameRecord,
    notes: Vec<(usize, crate::streaming::Note)>,
    /// Streaming's notes are in the subjects.
    noted: bool,
    proxies: Vec<(u32, Vec<u64>)>,
    kinds: Vec<(usize, SubjectKind)>,
    id_json: Option<Json>,
    stats: LodStats,
    lights: usize,
    dropped_lights: usize,
    overlays: Option<Json>,
    two_d_problems: Vec<String>,
    streaming: Json,
    effects: Json,
    drawn: Option<crate::trace_engine::DrawnRecord>,
    gi_drawn: bool,
    gi_error: Option<String>,
    /// The irradiance view's stretch `(scale, start)`, when it drew one.
    gi_stretch: Option<(f32, f32)>,
    /// Its pixels, once read ([`Engine::render_view_read`]).
    rgba: Option<Vec<u8>>,
}

impl ViewInFlight {
    /// Its counts and pixels are read (the GPU is done with it).
    pub(crate) fn read(&self) -> bool {
        self.rgba.is_some()
    }
}

/// What a terrain or water subject says it is (read when its frame was drawn).
enum SubjectKind {
    Terrain(Option<u32>),
    Water(String),
}

/// A view submitted: waiting for the GPU, or already rendered (the bounds overlay's two renders
/// wait).
pub(crate) enum Submitted {
    Done(Box<Rendered>),
    Flying(Box<ViewInFlight>),
}

impl Submitted {
    /// What its warnings need: model stats, 2D problems, lights left out.
    fn facts(&self) -> (&LodStats, &[String], usize) {
        match self {
            Submitted::Done(r) => (&r.stats, &r.two_d_problems, r.dropped_lights),
            Submitted::Flying(v) => (&v.stats, &v.two_d_problems, v.dropped_lights),
        }
    }
}

/// Set once the first lost device has been logged.
static LOSS_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A GPU failure during a capture's `step`, as a structured error (the tool's error text): a lost
/// device (`gpu_device_lost`: a hung or faulting submission) says so and that it is permanent
/// for this engine run, since Vulkan has no way back short of a new device; anything else is
/// `gpu_failed`. Both carry the `VkResult` when there is one. The first loss is also logged.
pub(crate) fn gpu_failure(step: &str, e: &specula_gpu::GpuError, windowed: bool) -> String {
    let lost = e.device_lost();
    let mut out = Json::obj([
        ("error", Json::from(if lost { "gpu_device_lost" } else { "gpu_failed" })),
        ("step", Json::from(step)),
        ("message", Json::from(e.message.as_str())),
        ("vk_result", e.vk_result.map_or(Json::Null, Json::from)),
    ]);
    if lost {
        // The window loop autosaves and exits (code 1) on its next frame; headless keeps serving.
        let recovery = if windowed {
            "none: the engine stops (it autosaves the world first); restart it"
        } else {
            "none in this run: every later render fails; restart the engine"
        };
        out.set("recovery", Json::from(recovery));
        crate::trace_engine::device_lost(step, &e.message);
        if !LOSS_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("specula: GPU device lost during a capture ({step}): {}; {recovery}", e.message);
        }
    }
    out.to_pretty()
}

fn camera_error(e: CameraError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn source(kind: &str) -> Json {
    Json::obj([("source", Json::from(kind))])
}

/// The world-space bounds of `id` and everything below it that has geometry and is shown; the
/// entity's origin when nothing does.
pub(crate) fn subtree_bounds(world: &World, id: u64) -> Option<(Vec3, Vec3)> {
    let mut acc: Option<(Vec3, Vec3)> = None;
    world.walk(Some(id), |e, _| {
        if e.shown() && e.local_bounds().is_some() {
            let (lo, hi) = e.world_aabb();
            acc = Some(match acc {
                Some((a, b)) => (a.min(lo), b.max(hi)),
                None => (lo, hi),
            });
        }
    });
    acc.or_else(|| world.get(id).map(|e| (e.world_position(), e.world_position())))
}

impl Engine {
    /// What auto-framing fits ([`sight::framing`]): the boxes from the subject cache when it is cheap to bring up
    /// to date (no walk of every entity's corners), else from the world.
    fn world_framing(&mut self, height_px: u32, fov_y_deg: f32) -> sight::Framing {
        let boxes = if self.subject_cache.ready(&self.world, self.world_generation) {
            self.subject_cache.refresh(&self.world, self.world_generation);
            self.subject_cache.boxes(&self.world)
        } else {
            sight::world_boxes(&self.world)
        };
        sight::framing(&boxes, height_px, fov_y_deg)
    }

    /// The editor's pick: queued on the capture flight like a capture (it never waits for the GPU
    /// on the engine thread), answered with the entities seen (`editor_engine::pick_ids`).
    pub(crate) fn pick_job(&mut self, spec: PickSpec, session: &SessionInfo, reply: crate::jobs::Reply, deadline: Instant) {
        let wait = CaptureWait { presented_at: Some(self.clock.tick()), parked_at: None, cut_by_deadline: false };
        self.capture_job(Job::Pick(spec), session, wait, reply, 0, deadline);
    }

    /// sight_frame, the sight tools and the camera tools.
    pub(crate) fn handle_sight(&mut self, request: Request) {
        let Request { command, session, reply, deadline, call, .. } = request;
        let job = match command {
            Command::Frame(spec) => Job::Frame(spec),
            Command::Sight(call) => match call {
                SightCall::Orbit(o) => Job::Orbit(o),
                SightCall::Compare(CompareCall::Mark { name, frame }) => Job::Mark { name, frame },
                SightCall::Compare(CompareCall::Diff { name, threshold, max_subjects }) => Job::Diff { name, threshold, max_subjects },
                SightCall::Query(q) => Job::Query(q),
                other => {
                    let _ = reply.send(self.sight_admin(other, &session));
                    return;
                }
            },
            _ => unreachable!("only sight calls come here"),
        };
        // The game brings its presentation up to date first, so models it spawns for this
        // capture are waited for too.
        let presented_at = self.before_capture();
        let mut wait = CaptureWait { presented_at, parked_at: None, cut_by_deadline: false };
        // Upload what is built, so models that start a pipeline variant build are waited for.
        let zp = profile::zone("sight.sync");
        for e in self.sync_meshes() {
            eprintln!("specula: {e}");
        }
        self.models.refresh_ready(&mut self.gpu);
        drop(zp);
        let zv = profile::zone("sight.views");
        let views = match self.job_views(&job, &session) {
            Ok(v) => v,
            Err(out) => {
                let _ = reply.send(out);
                return;
            }
        };
        drop(zv);
        // Its views become stream sources: what they need starts uploading now.
        let zw = profile::zone("sight.waiting");
        let streaming = self.stream_capture(&stream_views(&views));
        // `fresh`: sprite and tileset files in view are checked for changes now, so the capture
        // shows them as they are at the request.
        let ready = self.building_in(&views) == 0 && self.two_d_waiting(&views, true) == 0 && streaming == 0 && self.vfx_textures_waiting(true) == 0 && self.terrain_waiting(&views) == 0;
        drop(zw);
        if ready {
            // Started now, its views needn't be worked out again (auto-framing reads the bounds of
            // every entity) unless the world changes first.
            self.captures.views_hint = Some((self.world_generation, self.world.draw_epoch(), self.world.revision(), views));
            self.run_job(job, &session, wait, reply, call, deadline);
            self.captures.views_hint = None;
            return;
        }
        // Answer before the caller's own deadline, building or not.
        let usual = Instant::now() + CAPTURE_BUILD_WAIT;
        let latest = deadline.checked_sub(Duration::from_secs(2)).unwrap_or(deadline);
        wait.parked_at = Some(Instant::now());
        wait.cut_by_deadline = latest < usual;
        self.capture_waiters.push(CaptureWaiter { job, session, reply, until: usual.min(latest), wait, trace: specula_core::trace::Ctx::current(), call, deadline });
    }

    /// Renders parked captures whose models are built (or whose wait ran out).
    pub(crate) fn serve_captures(&mut self) {
        if self.capture_waiters.is_empty() {
            return;
        }
        let _z = profile::zone("parked_captures");
        for e in self.sync_meshes() {
            eprintln!("specula: {e}");
        }
        self.models.refresh_ready(&mut self.gpu);
        let now = Instant::now();
        let waiters = std::mem::take(&mut self.capture_waiters);
        for w in waiters {
            let building = match self.job_views(&w.job, &w.session) {
                Ok(views) => {
                    // Edits since it parked are filed (no old proxy drawn) and its views stay sources.
                    let streaming = self.stream_capture(&stream_views(&views));
                    self.building_in(&views) + self.two_d_waiting(&views, false) + streaming + self.vfx_textures_waiting(false) + self.terrain_waiting(&views)
                }
                Err(out) => {
                    let _ = w.reply.send(out);
                    continue;
                }
            };
            if now >= w.until || building == 0 {
                let wait = CaptureWait { cut_by_deadline: w.wait.cut_by_deadline && now >= w.until, ..w.wait };
                let _c = w.trace.enter();
                let started = Instant::now();
                self.run_job(w.job, &w.session, wait, w.reply, w.call, w.deadline);
                self.engine_work_done(w.trace.call, Some(crate::schedule::Class::Render), "parked_capture", started.elapsed());
            } else {
                self.capture_waiters.push(w);
            }
        }
    }

    /// Terrain sets `views` want that aren't on the GPU yet (and the terrain pipelines, while
    /// they build): their texels are asked of the worker, and those it has made are uploaded, so
    /// a capture never makes terrain texels on the engine thread; it waits for them parked.
    fn terrain_waiting(&mut self, views: &[ViewReq]) -> usize {
        let _z = profile::zone("terrain_wait");
        // Copies of what the worker made, a few milliseconds' worth per pass: the window's
        // frames run between the passes.
        let allowance = Some(crate::two_d::Allowance { ms: 3.0, bytes: 8 << 20, first_free: true });
        let budget = crate::terrain::Budget { allowance, radius: (self.stream.cfg.radius_m, self.stream.cfg.hlod_radius_m), memory: self.stream.terrain_room() };
        let mut pending = 0;
        for v in views {
            self.terrain.prepare(&mut self.gpu, &self.library, self.models.synced, &self.world, &v.camera, v.include_ui, false, budget);
            pending += self.terrain.stats.pending as usize;
        }
        self.stream.terrain_bytes = self.terrain.bytes();
        pending
    }

    /// 2D textures and shapes still loading that `views` would show (see `two_d_loading`).
    fn two_d_waiting(&mut self, views: &[ViewReq], fresh: bool) -> usize {
        views.iter().map(|v| self.two_d_loading(&v.camera.view_proj(v.width, v.height), v.include_ui, fresh)).sum()
    }

    /// Model entities still building that `views` would show: drawn (visible, on a layer the
    /// view includes) and with their origin in view (with a margin: their bounds aren't known
    /// until they are built).
    fn building_in(&self, views: &[ViewReq]) -> usize {
        let building: Vec<&world::Entity> =
            // Building, or built and uploaded but waiting for its pipeline variant (built on a
            // worker thread when the first such model arrives).
            self.world
                .entities()
                .filter(|e| e.resolved_model().is_some_and(|r| r.state == world::ModelState::Building) || self.models.waiting(e))
                .collect();
        if building.is_empty() {
            return 0;
        }
        building
            .iter()
            .filter(|e| {
                views.iter().any(|v| {
                    let p = v.camera.view_proj(v.width, v.height).mul_vec4(e.world_position().extend(1.0));
                    sight::in_frame(e, v.include_ui) && p.w > 0.0 && p.x.abs() <= p.w * 1.5 && p.y.abs() <= p.w * 1.5
                })
            })
            .count()
    }

    /// The agent whose cameras to use: the caller (by its agent name) or agent `of`. Cameras
    /// belong to names (see [`crate::cameras`]): whoever holds a name holds its cameras.
    fn camera_owner(&self, session: &SessionInfo, of: Option<&str>) -> String {
        of.map_or_else(|| self.swarm.agent_name(session), str::to_owned)
    }

    /// The camera of a sight_frame-style spec: a named camera, eye/target, or auto-framing.
    fn frame_view(&mut self, spec: &FrameSpec, session: &SessionInfo) -> Result<ViewReq, ToolOutput> {
        let (camera, source, warning) = match (&spec.camera, spec.eye, spec.target) {
            (Some(c), _, _) => {
                let owner = self.camera_owner(session, c.of.as_deref());
                let r = self.cameras.resolve(&owner, &c.name, c.of.as_deref(), &self.world).map_err(camera_error)?;
                let mut s = vec![("source", Json::from("named")), ("name", Json::from(c.name.as_str()))];
                if let Some(of) = &c.of {
                    s.push(("of", Json::from(of.as_str())));
                }
                // Whose camera, and which version of it: a reviewer sees it change.
                s.push(("owner", Json::from(owner.as_str())));
                s.push(("camera_version", Json::from(r.version)));
                (r.camera, Json::obj(s), r.warning)
            }
            (None, Some(eye), Some(target)) => (Camera::new(eye, target, spec.fov_y_deg), source("explicit"), None),
            // Orthographic without a camera: the 2D view of the whole XY plane.
            _ if spec.ortho.is_some() => {
                let aspect = spec.width as f32 / spec.height.max(1) as f32;
                (sight::auto_camera_2d(&self.world, spec.ortho, aspect), source("auto"), None)
            }
            // Auto-framing fits the main cluster: what it leaves out is named in the packet
            // (`camera.framing`), so an agent knows to frame it explicitly.
            _ => {
                let f = self.world_framing(spec.height, spec.fov_y_deg);
                let mut src = source("auto");
                if let Some(j) = f.to_json(&self.world) {
                    src.set("framing", j);
                }
                (sight::auto_camera_of(f.bounds, spec.fov_y_deg), src, None)
            }
        };
        let mut camera = camera;
        if let (None, Some(height_m)) = (&spec.camera, spec.ortho) {
            camera.projection = sight::Projection::Ortho { height_m };
        }
        Ok(ViewReq { camera, width: spec.width, height: spec.height, include_ui: spec.include_ui, source, warning })
    }

    /// The views a job renders (its first view is the main one).
    fn job_views(&mut self, job: &Job, session: &SessionInfo) -> Result<Vec<ViewReq>, ToolOutput> {
        match job {
            Job::Frame(spec) | Job::Mark { frame: spec, .. } => Ok(vec![self.frame_view(spec, session)?]),
            Job::Query(q) => Ok(vec![self.frame_view(&q.frame, session)?]),
            Job::Pick(p) => Ok(vec![ViewReq { camera: p.camera, width: p.width, height: p.height, include_ui: true, source: source("editor_pick"), warning: None }]),
            Job::Diff { name, .. } => {
                let key = self.swarm.key(session);
                let mark = self.marks.get(&key, name).ok_or_else(|| mark_missing(name))?;
                let v = &mark.view;
                Ok(vec![ViewReq { camera: v.camera, width: v.width, height: v.height, include_ui: v.include_ui, source: source("mark"), warning: None }])
            }
            Job::Orbit(o) => {
                let (center, radius, _) = self.orbit_target(&o.target)?;
                let distance = o.distance.unwrap_or_else(|| orbit::fit_distance(radius, o.fov_y_deg));
                let (tw, th, _, _) = orbit::layout(o.views, o.tile);
                Ok((0..o.views)
                    .map(|i| {
                        let (camera, _) = orbit::view(center, distance, o.elevation_deg, o.start_deg, i, o.views, o.fov_y_deg);
                        ViewReq { camera, width: tw, height: th, include_ui: o.include_ui, source: source("orbit"), warning: None }
                    })
                    .collect())
            }
        }
    }

    /// An orbit's centre, radius and target path.
    fn orbit_target(&self, target: &OrbitTarget) -> Result<(Vec3, f32, Option<String>), ToolOutput> {
        match target {
            OrbitTarget::Point(p) => Ok((*p, 1.0, None)),
            OrbitTarget::Entity(key) => {
                let id = self.world.resolve(key).map_err(|e| world_error(&e))?;
                let (lo, hi) = subtree_bounds(&self.world, id).expect("it exists");
                Ok(((lo + hi) * 0.5, ((hi - lo).length() * 0.5).max(0.25), Some(self.world.path_of(id))))
            }
        }
    }

    /// Calls answered at once: cameras, and listing or deleting marks.
    fn sight_admin(&mut self, call: SightCall, session: &SessionInfo) -> ToolOutput {
        let key = self.swarm.key(session);
        let name = self.swarm.agent_name(session);
        match call {
            SightCall::CameraSet(spec) => {
                // Journaled like any world change (kind `camera`), so other agents see it in
                // world_changes and world_undo puts it back; saved with the world when the agent
                // has a name to come back as (agent_hello), else for this engine run only.
                let persisted = self.swarm.introduced(&session.id);
                let w = Writer::new(&name, Some(&key));
                let tick = self.clock.tick();
                let set = self.world.commit(&mut self.swarm, tick, w, journal::Kind::Camera, crate::write_policy::Rule::World, |wd| wd.set_camera(&name, spec, persisted).cloned());
                let (cam, seq) = match set {
                    Ok(ok) => ok,
                    Err(e) => return world_error(&e),
                };
                let mut out = cam.to_json();
                match self.cameras.resolve(&name, &cam.spec.name, None, &self.world) {
                    Ok(r) => {
                        out.set("now", r.camera.to_json());
                        if let Some(w) = r.warning {
                            out.set("warning", Json::from(w));
                        }
                    }
                    // A follow camera whose entity doesn't exist yet is kept; it works once it does.
                    Err(e) => {
                        out.set("now", Json::Null);
                        out.set("warning", Json::from(e.message));
                    }
                }
                out.set("owner", Json::from(name));
                out.set("journal", seq.map_or(Json::Null, Json::from));
                out.set("persisted", Json::from(persisted));
                if !persisted {
                    out.set("note", Json::from("not saved with the world: say agent_hello with a name to keep cameras with the world"));
                }
                ToolOutput::text(out.to_pretty())
            }
            SightCall::CameraList { all: true, .. } => {
                let (owners, omitted) = cameras::list_all(&self.world, 64);
                ToolOutput::text(Json::obj([("owners", Json::Arr(owners)), ("owners_omitted", Json::from(omitted))]).to_string())
            }
            SightCall::CameraList { of, .. } => {
                let owner = self.camera_owner(session, of.as_deref());
                let listed: Vec<(String, Json)> =
                    cameras::list(&self.world, &owner).into_iter().map(|c| (c.spec.name.clone(), c.to_json())).collect();
                if listed.is_empty() && of.is_some() && !self.world.cameras().contains_key(&owner) {
                    return camera_error(CameraError::new(
                        "agent_not_found",
                        format!("no agent named '{owner}' holds cameras (camera_list {{all: true}} lists the owners)"),
                    ));
                }
                let mut cams = Vec::new();
                for (cam, mut j) in listed {
                    if let Ok(r) = self.cameras.resolve(&owner, &cam, of.as_deref(), &self.world) {
                        j.set("now", r.camera.to_json());
                    }
                    cams.push(j);
                }
                let out = Json::obj([("owner", Json::from(owner.as_str())), ("cameras", Json::Arr(cams))]);
                ToolOutput::text(out.to_string())
            }
            SightCall::CameraDelete { name: cam } => {
                let w = Writer::new(&name, Some(&key));
                let tick = self.clock.tick();
                match self.world.commit(&mut self.swarm, tick, w, journal::Kind::Camera, crate::write_policy::Rule::World, |wd| wd.delete_cameras(&name, cam.as_deref())) {
                    Ok((n, seq)) => {
                        self.cameras.prune(&self.world);
                        ToolOutput::text(Json::obj([("deleted", Json::from(n)), ("journal", seq.map_or(Json::Null, Json::from))]).to_string())
                    }
                    Err(e) => world_error(&e),
                }
            }
            SightCall::Compare(CompareCall::List) => {
                let marks = self.marks.names(&key).into_iter().map(|(n, frame, size)| {
                    Json::obj([("name", Json::from(n)), ("frame", Json::from(frame)), ("size", Json::arr(size.map(Json::from)))])
                });
                ToolOutput::text(Json::obj([("marks", Json::arr(marks)), ("max", Json::from(compare::MAX_MARKS_PER_AGENT))]).to_string())
            }
            SightCall::Compare(CompareCall::Delete { name: mark }) => {
                let n = self.marks.remove(&key, mark.as_deref());
                if n == 0 && mark.is_some() {
                    return mark_missing(mark.as_deref().unwrap_or_default());
                }
                ToolOutput::text(Json::obj([("deleted", Json::from(n))]).to_string())
            }
            _ => unreachable!("captures are jobs"),
        }
    }

    /// Renders a job and answers it: from a worker once its image is encoded and written
    /// (sight_query at once when its view lands). The render doesn't wait for the GPU: see
    /// [`flight`].
    fn run_job(&mut self, job: Job, session: &SessionInfo, wait: CaptureWait, reply: crate::jobs::Reply, call: u64, deadline: Instant) {
        // An image goes to a worker: with no slot free the call is refused before it renders.
        if let Some(bytes) = capture_bytes(&job) {
            if let Err(e) = self.workers_room(&self.capture_owner(session), call, bytes) {
                reply.refuse(&e);
                return;
            }
        }
        self.capture_job(job, session, wait, reply, call, deadline);
    }

    /// Whose a capture's worker job is.
    pub(crate) fn capture_owner(&self, session: &SessionInfo) -> crate::jobs::JobOwner {
        crate::jobs::JobOwner { name: self.swarm.agent_name(session), key: self.swarm.key(session) }
    }

    /// The next capture's number and PNG path (`frame_NNNNN.png` in this run's folder).
    pub(crate) fn next_capture_path(&mut self) -> PathBuf {
        self.frames_captured += 1;
        self.capture_dir.join(format!("frame_{:05}.png", self.frames_captured))
    }

    /// Hands a capture's pixels to a worker: it encodes the PNG, writes it to `png` through the
    /// disk budget, and calls `finish` with the PNG's bytes, where it landed (`None`: not saved)
    /// and the warnings so far; `finish` makes (and writes) the packet and the answer, which the
    /// worker sends. Encoding a 2048 x 2048 frame took about 290 ms of the engine thread (class C,
    /// 2026-10-29).
    pub(crate) fn offload_capture(&mut self, session: &SessionInfo, to: Answer, image: (u32, u32, Vec<u8>), png: PathBuf, source: &'static str, finish: impl FnOnce(Vec<u8>, Option<PathBuf>, Vec<Json>) -> ToolOutput + Send + 'static) {
        let bytes = image.2.len() as u64;
        self.offload_capture_made(session, to, bytes, move || (image, ()), png, source, move |(), png_bytes, saved, warnings| finish(png_bytes, saved, warnings));
    }

    /// [`Engine::offload_capture`] whose image is made on the worker too (`make`: a compare's
    /// diff and side-by-side sheet), with what else `make` worked out for `finish`. `bytes`: what
    /// the inputs hold meanwhile.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn offload_capture_made<R: Send + 'static>(
        &mut self,
        session: &SessionInfo,
        to: Answer,
        bytes: u64,
        make: impl FnOnce() -> ((u32, u32, Vec<u8>), R) + Send + 'static,
        png: PathBuf,
        source: &'static str,
        finish: impl FnOnce(R, Vec<u8>, Option<PathBuf>, Vec<Json>) -> ToolOutput + Send + 'static,
    ) {
        let Answer { reply, call } = to;
        let theirs = reply.clone();
        // Every capture saved is recorded for its agent (`crate::records`): a review submission
        // references it by the answer's image.recorded.
        let studio = std::sync::Arc::clone(&self.studio.shared);
        let session_id = session.id.clone();
        let work = move |_: &crate::workers::Cancel| {
            let ((w, h, rgba), made) = make();
            let bytes = encode_png(w, h, &rgba);
            drop(rgba);
            let mut warnings = Vec::new();
            let saved = {
                let _z = profile::zone("sight.save");
                crate::disk::book().write(crate::disk::Category::Captures, source, &png, &bytes, crate::disk::Need::Agent)
            };
            let saved = match saved {
                Ok(()) => Some(png),
                Err(e) => {
                    warnings.push(Json::from(format!("capture not saved to disk: {e}")));
                    None
                }
            };
            // The digest of the PNG bytes written, and (after the answer is made, which writes the
            // packet on this thread) of the packet bytes written.
            let record = saved.as_ref().map(|p| (p.clone(), specula_sys::digest::sha256(&bytes)));
            PACKET_WRITTEN.with(|w| w.borrow_mut().take());
            let mut out = finish(made, bytes, saved, warnings);
            let packet = PACKET_WRITTEN.with(|w| w.borrow_mut().take());
            if let (Some((png, sha)), false) = (record, out.is_error) {
                let packet = packet.filter(|(p, _)| *p == png.with_extension("json")).map(|(_, b)| b);
                studio.record_capture(&session_id, source, &png, sha, packet.as_deref(), &mut out);
            }
            let _ = theirs.send(out);
        };
        let kind = crate::workers::JobKind::CaptureEncode;
        let job = crate::workers::Offload::agent(kind, self.capture_owner(session), &session.id, call).class(crate::schedule::Class::Render).bytes(bytes);
        let refused = reply.clone();
        if let Err(e) = self.offload(job, work, move |_, r| {
            if let Err(f) = r {
                let _ = reply.send(f.to_output(kind));
            }
        }) {
            refused.refuse(&e);
        }
    }

    /// Warnings every capture shares, about the world as its view was drawn: failed uploads,
    /// models not drawn, dropped lights, the camera's (validation messages come when it lands,
    /// [`Engine::validation_warnings`]).
    fn capture_warnings(&mut self, stats: &LodStats, two_d_problems: &[String], dropped_lights: usize, wait: CaptureWait, building: usize, view: &ViewReq) -> Vec<Json> {
        let mut warnings: Vec<Json> = self.lathes.failures().chain(self.models.failures()).take(10).map(|m| Json::from(m.as_str())).collect();
        if stats.not_resident > 0 {
            warnings.push(Json::from(format!(
                "{} built model entit{} not drawn: not resident (streaming: too far and too small for every camera, or evicted under \
                 memory_mb; see streaming.not_resident and subjects' drawn_as, stream_state for why)",
                stats.not_resident,
                if stats.not_resident == 1 { "y" } else { "ies" },
            )));
        }
        if stats.missing > 0 {
            let waited = match (wait.parked_at, wait.cut_by_deadline) {
                (None, _) => "none was in frame when the call came".to_owned(),
                (Some(t), false) => format!("waited {:.1} s", t.elapsed().as_secs_f64()),
                (Some(t), true) => format!("waited {:.1} s, cut short by the call's deadline", t.elapsed().as_secs_f64()),
            };
            warnings.push(Json::from(format!(
                "{} model entit{} not drawn: {building} in frame still building ({waited}), others failed or \
                 started building since (world_query fields model_state, engine_info models.library.broken)",
                stats.missing,
                if stats.missing == 1 { "y" } else { "ies" },
            )));
        }
        // Terrains drawn by their stand-in meshes: their sets weren't up when the wait ran out
        // (or the frame's 256 terrain records were full).
        let t = &self.terrain.stats;
        if t.waiting > 0 {
            let why = if t.pending > 0 { format!("{} terrain set(s) still being made when the wait ran out", t.pending) } else { format!("{} terrain records in use (at most 256 a frame)", t.records) };
            warnings.push(Json::from(format!("{} terrain entit{} drawn by {} stand-in mesh: {why} (perf_frame terrain, terrain_info)", t.waiting, if t.waiting == 1 { "y" } else { "ies" }, if t.waiting == 1 { "its" } else { "their" })));
        }
        {
            let lib = self.library.lock();
            let problems = self.world.entities().filter(|e| e.anim.is_some()).filter_map(|e| models::anim_problem(&lib, e));
            warnings.extend(problems.take(5).map(Json::from));
        }
        // Models held back because their pipeline variant failed to build.
        warnings.extend(self.world.entities().filter_map(|e| self.models.pipeline_problem(e)).take(5).map(Json::from));
        // Only problems of what this frame draws.
        warnings.extend(two_d_problems.iter().map(|m| Json::from(m.as_str())));
        if dropped_lights > 0 {
            warnings.push(Json::from(format!(
                "{} point light(s) not rendered: at most {} per frame",
                dropped_lights,
                specula_gpu::MAX_POINT_LIGHTS
            )));
        }
        if let Some(w) = &view.warning {
            warnings.push(Json::from(w.as_str()));
        }
        warnings
    }

    /// The GPU's validation messages since the last capture, as warnings (and traced).
    fn validation_warnings(&mut self) -> Vec<Json> {
        let mut warnings = Vec::new();
        for m in self.gpu.take_validation_messages() {
            if m.severity >= Severity::Warning {
                warnings.push(Json::from(format!("vulkan {:?} {}: {}", m.severity, m.id_name, m.message)));
                crate::trace_engine::gpu_message(m.severity == Severity::Error, &m.id_name, &m.message);
            }
        }
        warnings
    }

    /// Renders one view and waits for it (see the module docs): [`Engine::render_view_submit`],
    /// then the GPU's fence, then [`Engine::render_view_land`]. Captures don't wait (the capture
    /// flight, `capture/flight.rs`); filmstrips and tests do.
    pub(crate) fn render_view(&mut self, camera: Camera, w: u32, h: u32, opts: &ViewOpts) -> Result<Rendered, String> {
        let mut r = match self.render_view_submit(camera, w, h, opts)? {
            Submitted::Done(r) => *r,
            Submitted::Flying(v) => {
                self.gpu.settle_offscreen(true).map_err(|e| gpu_failure("render", &e, self.windowed))?;
                self.render_view_land(*v)?
            }
        };
        // Its callers (filmstrip tiles) keep the pixels: the flash watch gets a copy on a worker.
        if let Some(feed) = r.feed.take() {
            self.flash_job(feed, r.rgba.clone(), w, h);
        }
        Ok(r)
    }

    /// Runs a capture's flash feed on a worker with `pixels` (a view whose pixels go nowhere else:
    /// orbit tiles, sight_query, filmstrip tiles).
    pub(crate) fn flash_job(&mut self, feed: crate::flashwatch::Feed, pixels: Vec<u8>, w: u32, h: u32) {
        let job = crate::workers::Offload::engine(crate::workers::JobKind::FlashFeed).class(crate::schedule::Class::Render);
        let refused = self.offload(job, move |_| {
            let _z = profile::zone("sight.flash");
            feed.run(&pixels, w, h);
        }, |_, _| {});
        // (The engine's own jobs are never refused: a refusal would only drop the feed, which
        // hands the view's state on unfed.)
        drop(refused);
    }

    /// Prepares one view and submits it to the GPU without waiting (`Gpu::render_async`):
    /// draw list, lights, subjects (from the subject cache: projections only), debug overlays.
    /// Everything the packet says about the world is taken here, when the frame is drawn;
    /// [`Engine::render_view_land`] adds what only the GPU knows (pixels, ID counts) once the
    /// render has finished.
    ///
    /// The frame renders once, with the ID buffer (unless `opts.ids` is off), whose counts go
    /// into the subjects. Overlays are drawn in that same render (the ID passes come after them
    /// and ignore them), placed by the projected boxes; only the `bounds` overlay needs the
    /// counts (fully hidden boxes are red), so with it the frame renders a second time, overlays
    /// on top and without the ID passes (the scene is the same, bit for bit): that case waits for
    /// the GPU here and comes back [`Submitted::Done`].
    pub(crate) fn render_view_submit(&mut self, camera: Camera, w: u32, h: u32, opts: &ViewOpts) -> Result<Submitted, String> {
        let view_proj = camera.view_proj(w, h);
        let mut draws = Vec::new();
        let mut owners = Vec::new();
        let draw_zone = profile::zone("sight.draws");
        let (stats, _) = self.build_draws(camera, h, opts.include_ui, opts.lod, false, opts.time_s, &mut draws, Some(&mut owners));
        drop(draw_zone);
        self.lod_capture = stats;
        // Baked GI: the chunks around this camera; `sight_frame gi: "probes"` adds the probes
        // (not counted in the ID buffer).
        // A bake that just landed loads on a worker: a capture waits for it (at most 30 s).
        let zg = profile::zone("sight.gi_lights_fx");
        self.gi.wait_loads(&self.world, std::time::Duration::from_secs(30));
        self.gi.update(&self.world, camera.eye, true);
        let gi_view = self.gi.capture_view;
        if gi_view == crate::gi::CaptureView::Probes {
            self.gi.marks_drawn = self.gi_probe_draws(camera.eye, &mut draws);
            owners.resize(draws.len(), 0);
        }
        let gi_snapshot = self.gi.snapshot();
        let gi_volumes = gi_snapshot.volumes();
        let mut lights = Vec::new();
        let dropped_lights = sight::lights(&self.world, opts.include_ui, &mut lights);
        if let Some(l) = opts.fill {
            lights.insert(0, l);
            lights.truncate(specula_gpu::MAX_POINT_LIGHTS);
        }
        // Effects and decals at the capture's time (their lights join the world's).
        let fx_view = crate::vfx::View::of(&camera, w, h);
        self.build_effects(&fx_view, opts.time_s, opts.include_ui, &mut lights, false);
        let fx = self.vfx.take_frame();
        let mut effects = self.vfx.packet_json(&self.world, &fx_view);
        drop(zg);
        let mut subjects = {
            let _z = profile::zone("sight.subjects");
            self.subject_cache.refresh(&self.world, self.world_generation);
            sight::subjects_cached(&self.subject_cache, &self.world, &view_proj, w, h, opts.include_ui)
        };
        // Sprites, tile chunks and shapes: their ids follow the meshes' (render_with_ids takes
        // one id per mesh draw, then one per 2D draw).
        let z2 = profile::zone("sight.two_d_ids");
        let mut draws_2d = Vec::new();
        let mut owners_2d = Vec::new();
        self.build_draws_2d(&view_proj, opts.include_ui, opts.time_s as f32, &mut draws_2d, Some(&mut owners_2d));
        let mut two_d_problems: Vec<String> = self.two_d.problems().to_vec();
        // In-world glyphs left to a later capture or dropped, counted (text_world).
        two_d_problems.extend(self.two_d.text.frame_problem());
        // In-world text, owned: the frame outlives other engine calls below.
        let text = self.two_d.text.snapshot();
        owners.extend_from_slice(&owners_2d);
        let mut record = sight::FrameRecord::new(&owners);
        let (ids, entities) = sight::dense_ids(&owners);
        drop(z2);
        let target = self.offscreen_target(w, h)?;
        // Animated models' bone matrices, posed at time_s by build_draws (returned below; an
        // error path just drops them).
        let bones = self.models.take_bones();
        let terrains = std::mem::take(&mut self.terrain.frame);
        let mut frame = FrameDesc {
            bones: &bones,
            draws_2d: &draws_2d,
            effects: fx.effects(),
            gi: gi_snapshot.frame(&gi_volumes, gi_view),
            terrains: &terrains,
            ..sight::frame(&self.world, &view_proj, &draws, &lights, opts.time_s as f32)
        };
        self.luts.apply(&mut self.gpu, self.world.env(), &mut frame, true);
        let painted = !opts.overlays.is_empty() || opts.caption.is_some();
        let second_render = painted && opts.ids && opts.overlays.has(Kind::Bounds);
        let mut overlays = None;
        // A hosted game's interface (include_ui), under the debug overlays.
        let mut quads = Vec::new();
        // (Not for the editor's pick, `hud: false`: the window shows none of them while editing.)
        if opts.include_ui && opts.hud {
            if opts.overlays.has(Kind::Ui) {
                // Only an interface this capture lays out may claim its layout (`ui` overlay).
                self.ui_forget_capture_views();
            }
            // ui_frame's display (the caller's, or one for this capture alone), told the game
            // for this capture and taken back after.
            let told = self.ui_capture_display(opts.overlays.ui_display.as_deref().copied());
            self.game_overlay((w, h), camera, true, &mut quads);
            self.gallery_overlay((w, h), true, &mut quads);
            self.review_overlay((w, h), true, &mut quads);
            self.ui_capture_display_done(told);
        }
        let hud = quads.len();
        // Its opaque panels hide the scene under them in the ID counts, as they hide it from a
        // player (sight's debug overlays, added below, never do).
        let mut occluders = Vec::new();
        let occluders_dropped = if opts.include_ui && opts.ids { sight::ui_occluders(&quads[..hud], w, h, &mut occluders) } else { 0 };
        // Debug overlays get what the game's interface leaves of the GPU's limit.
        let budget = specula_gpu::MAX_OVERLAY_QUADS - hud;
        if painted && !second_render {
            let (q, report) = self.paint_overlays(camera, view_proj, w, h, &subjects, opts, budget, None);
            quads.extend(q);
            overlays = (!opts.overlays.is_empty()).then_some(report);
        }
        frame.overlay = &quads;
        frame.text = text.frame();
        // What streaming says of the subjects and what the terrain and water passes drew, read
        // now (the frame as drawn), written once the ID counts are in (in the packet's order).
        let notes = {
            let _z = profile::zone("sight.annotate");
            self.stream.notes(&subjects, &self.world, &self.models)
        };
        let proxies = self.stream.hlod.capture_groups.clone();
        let mut kinds = Vec::new();
        let wet: BTreeMap<u64, String> = self.world.water_entities().filter_map(|e| e.water.as_ref().map(|w| (e.id, w.preset.as_str().to_owned()))).collect();
        for (i, s) in subjects.iter().enumerate() {
            if self.terrain.covers(s.id) {
                kinds.push((i, SubjectKind::Terrain(self.terrain.drawn(s.id).map(|d| d.patches))));
            } else if let Some(preset) = wet.get(&s.id) {
                kinds.push((i, SubjectKind::Water(preset.clone())));
            }
        }
        let streaming = self.stream.capture_json(&stats);
        let view_key = view_proj.to_cols_array().iter().fold(u64::from(w) << 32 | u64::from(h), |h, v| h.rotate_left(5) ^ u64::from(v.to_bits()));
        let submitted = {
            let _z = profile::zone("sight.render");
            if second_render {
                let rendered = if opts.ids { self.gpu.render_with_ids_occluded(target, &frame, &ids, &occluders) } else { self.gpu.render(target, &frame) };
                rendered.map_err(|e| gpu_failure("render", &e, self.windowed))
            } else {
                self.gpu.render_async(target, &frame, opts.ids.then_some((&ids[..], &occluders[..]))).map_err(|e| gpu_failure("render", &e, self.windowed))
            }
        };
        // (Read now: window frames overwrite the GPU's GI state before the view lands.)
        let (gi_drawn, gi_error, gi_stretch) = (self.gpu.gi_drawn(), self.gpu.gi_failure().map(str::to_string), self.gpu.gi_view_stretch());
        if gi_drawn && matches!(gi_view, crate::gi::CaptureView::Irradiance | crate::gi::CaptureView::Probes) {
            // The irradiance view shows the indirect light alone and draws no particles: the packet
            // says so, never "drawn" for what no pixel shows (GI fix lane).
            unlit_particles(&mut effects);
        }
        // What the colour passes actually recorded (the GPU's own list, after every readiness
        // check): the packet monitor's record, independent of the ID buffer's counts.
        let drawn = (opts.ids && submitted.is_ok()).then(|| {
            let _z = profile::zone("sight.drawn_record");
            let (mesh, two_d) = self.gpu.last_colour_draws();
            crate::trace_engine::DrawnRecord::from_recorded(&owners, draws.len(), &mesh, &two_d)
        });
        let mut v = ViewInFlight {
            target,
            time_s: opts.time_s,
            view_key,
            ids: opts.ids,
            include_ui: opts.include_ui,
            entities,
            occluders: (occluders.len(), occluders_dropped),
            subjects: std::mem::take(&mut subjects),
            record: std::mem::take(&mut record),
            notes,
            noted: false,
            id_json: None,
            proxies,
            kinds,
            stats,
            lights: lights.len(),
            dropped_lights,
            overlays: overlays.take(),
            two_d_problems,
            streaming,
            effects,
            drawn,
            gi_drawn,
            gi_error,
            gi_stretch,
            rgba: None,
        };
        if !second_render {
            self.models.put_bones(bones);
            self.vfx.put_frame(fx);
            self.terrain.frame = terrains;
            submitted?;
            return Ok(Submitted::Flying(Box::new(v)));
        }
        if let Err(e) = submitted {
            self.models.put_bones(bones);
            self.vfx.put_frame(fx);
            self.terrain.frame = terrains;
            return Err(e);
        }
        // The bounds overlay colours fully hidden boxes from the counts: the counts first, then the
        // frame again with the overlays on top (waited for here).
        self.view_counts(&mut v)?;
        let (q, report) = self.paint_overlays(camera, view_proj, w, h, &v.subjects, opts, budget, Some(&v.record));
        v.overlays = (!opts.overlays.is_empty()).then_some(report);
        let mut both = quads[..hud].to_vec();
        both.extend(q);
        frame.overlay = &both;
        let again = {
            let _z = profile::zone("sight.render");
            self.gpu.render(target, &frame).map_err(|e| gpu_failure("render", &e, self.windowed))
        };
        self.models.put_bones(bones);
        self.vfx.put_frame(fx);
        self.terrain.frame = terrains;
        again?;
        v.ids = false; // counted above
        self.render_view_land(v).map(|r| Submitted::Done(Box::new(r)))
    }

    /// The ID counts of a settled view into its subjects and record, then its streaming notes.
    fn view_counts(&mut self, v: &mut ViewInFlight) -> Result<(), String> {
        let gpu_ms = self.gpu.last_frame_stats_of(FramePath::Offscreen).and_then(|s| s.gpu).and_then(|g| {
            let ms: f64 = g.passes.iter().filter(|p| p.name == specula_gpu::PASS_ID_BUFFER).map(|p| p.gpu_ms).sum();
            (ms > 0.0).then_some(ms)
        });
        let counted = Instant::now();
        let counts = {
            let _z = profile::zone("sight.ids");
            self.gpu.id_coverage(v.target, v.entities.len() as u32).map_err(|e| gpu_failure("id readback", &e, self.windowed))?
        };
        {
            let _z = profile::zone("sight.visibility");
            sight::apply_visibility(&mut v.subjects, &v.entities, &counts.per_id);
        }
        v.record.set_ids(&v.entities, &counts.per_id);
        let cpu_ms = counted.elapsed().as_secs_f64() * 1e3;
        let entities = &v.entities;
        let mut j = Json::obj([
            // Entities only (HLOD proxies have ID-pass owners of their own, past every entity).
            ("entities", Json::from(entities.iter().filter(|e| !sight::is_proxy_owner(**e)).count())),
            ("visible", Json::from(entities.iter().zip(&counts.per_id).filter(|(e, c)| !sight::is_proxy_owner(**e) && (c.pixels > 0 || c.approx)).count())),
            ("blended_counted", Json::from(counts.blended_counted)),
            ("gpu_ms", Json::from(gpu_ms)),
            ("readback_count_ms", Json::from((cpu_ms * 1000.0).round() / 1000.0)),
        ]);
        if v.include_ui {
            // How many opaque interface rectangles hid what was under them.
            j.set("ui_occluders", Json::from(v.occluders.0));
            if v.occluders.1 > 0 {
                j.set("ui_occluders_dropped", Json::from(v.occluders.1));
            }
        }
        v.id_json = Some(j);
        self.view_notes(v);
        Ok(())
    }

    /// Streaming's notes into a view's subjects (before the bounds overlay is painted: its colours
    /// come from the record, review round 2).
    fn view_notes(&mut self, v: &mut ViewInFlight) {
        crate::streaming::apply_notes(&mut v.subjects, &v.notes);
        v.record.set_streaming(&v.subjects);
        v.record.set_proxies(&v.proxies);
        v.noted = true;
    }

    /// What a settled view needs from the GPU (a capture's first step once it is done): its ID
    /// counts into the subjects, streaming's notes, and its pixels. The GPU is free after it.
    pub(crate) fn render_view_read(&mut self, v: &mut ViewInFlight) -> Result<(), String> {
        if v.rgba.is_some() {
            return Ok(());
        }
        if v.ids {
            self.view_counts(v)?;
            v.ids = false;
        } else if !v.noted {
            self.view_notes(v);
        }
        let _z = profile::zone("sight.readback");
        v.rgba = Some(self.gpu.read_rgba8(v.target).map_err(|e| gpu_failure("readback", &e, self.windowed))?);
        Ok(())
    }

    /// A submitted view, once the GPU has finished it (`Gpu::settle_offscreen`): its ID counts,
    /// its pixels (already copied to the readback buffer by the render), the flash watch, and the
    /// terrain and water notes.
    pub(crate) fn render_view_land(&mut self, mut v: ViewInFlight) -> Result<Rendered, String> {
        self.render_view_read(&mut v)?;
        let ids_json = v.id_json.take().unwrap_or_else(|| Json::obj([("off", Json::from(true))]));
        let rgba = v.rgba.take().unwrap_or_default();
        // The flash watch over this view's captures, from their own pixels at simulation time:
        // its place in the view's order is taken now, the pixels are fed where they go next (the
        // encode worker; see `Rendered::feed`).
        let feed = self.flash.feed_for(v.view_key, v.time_s);
        // Terrain and water subjects say what they are (and how a terrain was drawn); the frame
        // record knows them too (their passes draw outside the mesh lists).
        for (i, k) in std::mem::take(&mut v.kinds) {
            let Some(s) = v.subjects.get_mut(i) else { continue };
            match k {
                SubjectKind::Terrain(patches) => {
                    s.set("kind", Json::from("terrain"));
                    if let Some(p) = patches {
                        s.set("terrain_patches", Json::from(p));
                    }
                    v.record.set_terrain(s.id, patches.unwrap_or(0));
                }
                SubjectKind::Water(preset) => {
                    s.set("kind", Json::from("water"));
                    s.set("water", Json::from(preset));
                    v.record.set_water(s.id);
                }
            }
        }
        Ok(Rendered {
            feed,
            rgba,
            subjects: v.subjects,
            stats: v.stats,
            lights: v.lights,
            dropped_lights: v.dropped_lights,
            overlays: v.overlays,
            ids: ids_json,
            two_d_problems: v.two_d_problems,
            streaming: v.streaming,
            effects: v.effects,
            record: v.record,
            drawn: v.drawn,
            gi_drawn: v.gi_drawn,
            gi_error: v.gi_error,
            gi_stretch: v.gi_stretch,
        })
    }

    /// The overlay records for one view and what they drew, at most `budget` of them.
    #[allow(clippy::too_many_arguments)]
    fn paint_overlays(&mut self, camera: Camera, vp: Mat4, w: u32, h: u32, subjects: &[Subject], opts: &ViewOpts, budget: usize, record: Option<&sight::FrameRecord>) -> (Vec<OverlayQuad>, Json) {
        let spec = opts.overlays;
        let mut p = Painter::new(vp, w, h);
        p.limit = p.limit.min(budget.saturating_sub(ov::LABEL_QUADS));
        let mut report = ov::Report::default();
        let world = &self.world;
        // On-screen subjects the overlays may mark, biggest first.
        let mut shown: Vec<(&Subject, &world::Entity, String)> = subjects
            .iter()
            .filter(|s| s.on_screen)
            .filter_map(|s| {
                let path = s.base.path.clone();
                (spec.selects(&path)).then(|| world.get(s.id).map(|e| (s, e, path))).flatten()
            })
            .collect();
        shown.sort_by(|a, b| b.0.projected_area.cmp(&a.0.projected_area).then(a.0.id.cmp(&b.0.id)));
        let centre = |p: &Painter, e: &world::Entity, s: &Subject| -> [f32; 2] {
            let (lo, hi) = e.world_aabb();
            let c = p.project((lo + hi) * 0.5);
            let r = s.screen;
            let rect_centre = r.map(|r| {
                let v = |i: usize| f64::from(r[i]) as f32;
                [v(0) + v(2) * 0.5, v(1) + v(3) * 0.5]
            });
            match (c, rect_centre) {
                (Some(c), Some(rc)) => {
                    // The box centre projects outside its own clipped rectangle when the box
                    // reaches past the image edge: use the rectangle's centre then.
                    let r = r.expect("some");
                    let v = |i: usize| f64::from(r[i]) as f32;
                    if c[0] >= v(0) && c[0] <= v(0) + v(2) && c[1] >= v(1) && c[1] <= v(1) + v(3) {
                        c
                    } else {
                        rc
                    }
                }
                (_, Some(rc)) => rc,
                (c, None) => c.unwrap_or([0.0, 0.0]),
            }
        };
        let view_dist = (camera.eye - camera.target).length().max(0.1);
        if spec.has(Kind::Claims) {
            let now = Instant::now();
            let mut owners: BTreeMap<String, usize> = BTreeMap::new();
            let (mut drawn, mut unbuilt) = (0, 0);
            for c in self.swarm.claims.live(now).take(ov::MAX_CLAIMS) {
                let color = ov::owner_color(&c.owner);
                let (bounds, what) = match &c.scope {
                    claims::Scope::Region { min, max } => (Some((*min, *max)), format!("region #{}", c.id)),
                    claims::Scope::Path(path) if path == "/" => (world.bounds(), "/".to_owned()),
                    claims::Scope::Path(path) => (world.find_path(path).ok().and_then(|id| subtree_bounds(world, id)), path.clone()),
                };
                let Some((lo, hi)) = bounds else {
                    unbuilt += 1;
                    continue;
                };
                // A flat or point subtree still shows as a thin slab.
                let pad = Vec3::new(0.02, 0.02, 0.02);
                let (lo, hi) = (lo - pad, hi + pad);
                let corners: [Vec3; 8] = std::array::from_fn(|i| {
                    Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
                });
                p.box_faces(&corners, [color[0], color[1], color[2], 0.12]);
                p.box_edges(&corners, [color[0], color[1], color[2], 0.9], 2.0);
                if let Some(s) = p.project(Vec3::new((lo.x + hi.x) * 0.5, hi.y, (lo.z + hi.z) * 0.5)) {
                    p.label(&format!("{} {what}", c.owner), s, color, u64::MAX / 4);
                }
                drawn += 1;
                *owners.entry(c.owner.clone()).or_default() += 1;
            }
            report.drawn.push("claims");
            let owners = owners.into_iter().map(|(o, n)| {
                Json::obj([("color", Json::from(ov::hex(ov::owner_color(&o)))), ("owner", Json::from(o)), ("claims", Json::from(n))])
            });
            report.details.push((
                "claims",
                Json::obj([("drawn", Json::from(drawn)), ("not_built_yet", Json::from(unbuilt)), ("owners", Json::arr(owners))]),
            ));
        }
        if spec.has(Kind::Grid) {
            let (step, lines) = ov::grid(&mut p, camera.eye, ov::ground_focus(camera.eye, camera.target));
            report.drawn.push("grid");
            report.details.push(("grid", Json::obj([("y", Json::from(0.0)), ("step_m", Json::from(step)), ("lines", Json::from(lines))])));
        }
        if spec.has(Kind::Bounds) {
            let (mut boxes, mut hidden_boxes) = (0, 0);
            for (s, e, _) in shown.iter().take(ov::MAX_BOXES) {
                let Some(corners) = e.world_corners() else { continue };
                let hidden = fully_hidden(s, record);
                p.box_edges(&corners, if hidden { ov::OCCLUDED } else { ov::VISIBLE }, 1.5);
                boxes += 1;
                hidden_boxes += usize::from(hidden);
            }
            report.drawn.push("bounds");
            report.details.push((
                "bounds",
                Json::obj([
                    ("boxes", Json::from(boxes)),
                    ("fully_hidden", Json::from(hidden_boxes)),
                    ("omitted", Json::from(shown.len().saturating_sub(ov::MAX_BOXES))),
                    ("kind", Json::from("oriented")),
                    ("legend", Json::obj([("visible", Json::from(ov::hex(ov::VISIBLE))), ("fully_hidden", Json::from(ov::hex(ov::OCCLUDED)))])),
                ]),
            ));
        }
        if spec.has(Kind::Axes) {
            let length = ov::grid_step(view_dist) * 2.0;
            ov::axes(&mut p, Vec3::ZERO, [Vec3::X, Vec3::Y, Vec3::Z], length, true);
            let mut entity_axes = 0;
            for path in &spec.entities {
                let Some(e) = world.find_path(path).ok().and_then(|id| world.get(id)) else { continue };
                let m = e.model();
                let axis = |i: usize| Vec3::new(m.cols[i][0], m.cols[i][1], m.cols[i][2]).normalize();
                let (lo, hi) = subtree_bounds(world, e.id).expect("exists");
                let len = ((hi - lo).length() * 0.5).max(0.25);
                ov::axes(&mut p, e.world_position(), [axis(0), axis(1), axis(2)], len, false);
                entity_axes += 1;
            }
            report.drawn.push("axes");
            report.details.push(("axes", Json::obj([("origin_length_m", Json::from(length)), ("entities", Json::from(entity_axes))])));
        }
        if spec.has(Kind::Skeletons) {
            let lib = self.library.lock();
            let (mut rigged, mut bones, mut rigless, mut posed_count) = (0, 0, 0, 0);
            for (_, e, _) in shown.iter().filter(|(_, e, _)| e.model.is_some()) {
                if rigged >= ov::MAX_SKELETONS {
                    break;
                }
                let rig = e.resolved_model().and_then(|r| lib.built(r.handle)).and_then(|b| b.model.rig.clone());
                let Some(rig) = rig else {
                    rigless += 1;
                    continue;
                };
                let color_of = |name: &str| {
                    if name.ends_with("_l") {
                        [0.35, 0.85, 1.0, 1.0]
                    } else if name.ends_with("_r") {
                        [1.0, 0.45, 0.8, 1.0]
                    } else {
                        [1.0, 0.9, 0.4, 1.0]
                    }
                };
                // Animated models: the joints as the frame poses them at its time_s (what the
                // image shows); joints only, so each bone runs from its parent's joint to its own.
                let posed: Option<std::collections::HashMap<String, Vec3>> =
                    self.models.bone_positions(e, opts.time_s).map(|list| list.into_iter().collect());
                let m = e.model();
                let at = |v: [f32; 3]| m.transform_point(Vec3::new(v[0], v[1], v[2]));
                if let Some(joints) = &posed {
                    posed_count += 1;
                    for b in &rig.bones {
                        let head = joints.get(&b.name).copied().unwrap_or_else(|| at(b.head));
                        if let Some(parent) = b.parent.and_then(|i| rig.bones.get(i)) {
                            let from = joints.get(&parent.name).copied().unwrap_or_else(|| at(parent.head));
                            p.line(from, head, color_of(&parent.name), 2.5);
                        }
                        p.dot(head, 3.0, color_of(&b.name));
                        bones += 1;
                    }
                } else {
                    for b in &rig.bones {
                        let color = color_of(&b.name);
                        if let Some(parent) = b.parent.and_then(|i| rig.bones.get(i)) {
                            if parent.tail != b.head {
                                p.line(at(parent.head), at(b.head), [1.0, 1.0, 1.0, 0.45], 1.0);
                            }
                        }
                        if b.tail != b.head {
                            p.line(at(b.head), at(b.tail), color, 2.5);
                        }
                        p.dot(at(b.head), 3.0, color);
                        bones += 1;
                    }
                }
                rigged += 1;
            }
            drop(lib);
            report.drawn.push("skeletons");
            report.details.push((
                "skeletons",
                Json::obj([
                    ("entities", Json::from(rigged)),
                    ("bones", Json::from(bones)),
                    ("models_without_rig", Json::from(rigless)),
                    // "posed": animated models drawn as the frame poses them; "rest": none were.
                    ("posed", Json::from(posed_count)),
                    ("pose", Json::from(if posed_count > 0 { "posed" } else { "rest" })),
                ]),
            ));
        }
        if spec.has(Kind::Lods) {
            let mut list = Vec::new();
            let mut total = 0;
            for (s, e, path) in shown.iter().filter(|(_, e, _)| e.model.is_some()) {
                let Some((level, count, tris)) = self.models.lod_of(e, camera.eye, camera.fov_y_deg, h, opts.lod) else { continue };
                total += 1;
                if list.len() < 64 {
                    let at = centre(&p, e, s);
                    p.label(&format!("LOD {level} (0-{}) {}", count - 1, tri_text(tris)), at, [0.6, 1.0, 0.6, 1.0], (1 << 40) + s.projected_area);
                    list.push(Json::obj([
                        ("path", Json::from(path.as_str())),
                        ("lod", Json::from(level)),
                        ("lods", Json::from(count)),
                        ("triangles", Json::from(tris)),
                    ]));
                }
            }
            report.drawn.push("lods");
            report.details.push(("lods", Json::obj([("models", Json::Arr(list)), ("total", Json::from(total))])));
        }
        if spec.has(Kind::Colliders) || spec.has(Kind::Contacts) {
            let (colliders, contacts) = (spec.has(Kind::Colliders), spec.has(Kind::Contacts));
            for (k, d) in crate::physics_tools::paint(&self.physics, world, &mut p, spec, colliders, contacts) {
                report.drawn.push(k);
                report.details.push((k, d));
            }
        }
        if spec.has(Kind::Joints) {
            report.drawn.push("joints");
            report.details.push(("joints", crate::joint_tools::paint(&self.physics, world, &mut p, spec)));
        }
        if spec.has(Kind::Navmesh) || spec.has(Kind::Paths) {
            for (k, d) in crate::nav_tools::paint(&self.nav, world, &mut p, spec.has(Kind::Navmesh), spec.has(Kind::Paths)) {
                report.drawn.push(k);
                report.details.push((k, d));
            }
        }
        if spec.has(Kind::Effects) {
            let d = crate::vfx_tools::paint(&self.vfx, world, &mut p);
            report.drawn.push("effects");
            report.details.push(("effects", d));
        }
        if spec.has(Kind::Labels) {
            for (s, e, _) in shown.iter().take(spec.max_labels * 4) {
                let at = centre(&p, e, s);
                p.label(&e.name, at, ov::WHITE, s.projected_area);
            }
            report.drawn.push("labels");
        }
        if spec.has(Kind::Ui) {
            // The interface as this capture laid it out (its overlay ran just before).
            let d = self.paint_ui_outlines(&mut p, spec.max_labels, opts.include_ui);
            report.drawn.push("ui");
            report.details.push(("ui", d));
        }
        if let Some(c) = &opts.caption {
            p.label(c, [0.0, 0.0], ov::WHITE, u64::MAX);
        }
        // Text last, on top of the lines.
        let requested = p.labels.len();
        let labels = std::mem::take(&mut p.labels);
        let labeler = &mut self.labeler;
        let (placed, skipped) = ov::place_labels(labels, p.size(), spec.max_labels.max(1), &mut |t| labeler.measure(t));
        let mut quads = std::mem::take(&mut p.quads);
        let font = self.labeler.font_path().map(|f| f.display().to_string());
        let text = match self.labeler.draw(&mut self.gpu, &placed, &mut quads) {
            Ok(drawn) => Json::obj([
                ("requested", Json::from(requested)),
                ("placed", Json::from(if drawn { placed.len() } else { 0 })),
                ("skipped", Json::from(skipped + if drawn { 0 } else { placed.len() })),
                ("font", Json::from(font)),
            ]),
            Err(e) => Json::obj([("requested", Json::from(requested)), ("placed", Json::from(0)), ("error", Json::from(e))]),
        };
        report.details.push(("text", text));
        // Text past the budget too (a caption longer than a label): dropped, never a failed frame.
        if quads.len() > budget {
            p.dropped += quads.len() - budget;
            quads.truncate(budget);
        }
        if p.dropped > 0 {
            report.details.push(("records_dropped", Json::from(p.dropped)));
        }
        (quads, report.to_json())
    }

}

/// Whether the bounds overlay paints `s` as fully hidden: once the frame is measured, exactly the
/// subjects the packet marks `no_pixels` (review round 2: HLOD members were painted red);
/// before, from the subject's own counted pixels.
fn fully_hidden(s: &Subject, record: Option<&sight::FrameRecord>) -> bool {
    match record {
        // Measured: in view and drawn, yet no pixel shows (`no_pixels`, the packet's own word). An
        // HLOD member has no pixels of its own (its proxy's are counted, `drawn` says whether it
        // reached the image), and an entity the frame never drew isn't hidden behind anything:
        // neither is painted red.
        Some(_) => s.vis.map_or_else(|| s.added("no_pixels").and_then(Json::as_bool) == Some(true), |v| v.no_pixels),
        None => s.shows() == Some(false),
    }
}

/// The views' cameras and sizes, as streaming takes them.
fn stream_views(views: &[ViewReq]) -> Vec<(Camera, u32, u32)> {
    views.iter().map(|v| (v.camera, v.width, v.height)).collect()
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let _z = profile::zone("sight.encode");
    png::encode_rgba8(w, h, rgba, Level::Fastest)
}

/// An effects packet (`vfx::packet_json`) for a frame drawn without particles (the irradiance
/// view): no effect is drawn or fills a pixel, and it says why.
fn unlit_particles(effects: &mut Json) {
    if let Some(Json::Arr(list)) = effects.get("effects") {
        let mut list = list.clone();
        for e in list.iter_mut() {
            e.set("drawn", Json::from(false));
            e.set("fill_px", Json::from(0));
        }
        effects.set("effects", Json::Arr(list));
    }
    effects.set("particles", Json::from("not drawn: the irradiance view shows the indirect light alone"));
}

/// What a capture's image holds until it is encoded on a worker (the frame, or a sheet within the
/// sheet cap); `None`: no image (sight_query).
fn capture_bytes(job: &Job) -> Option<u64> {
    match job {
        Job::Query(_) | Job::Pick(_) => None,
        Job::Frame(s) | Job::Mark { frame: s, .. } => Some(u64::from(s.width) * u64::from(s.height) * 4),
        _ => Some(u64::from(orbit::MAX_SHEET) * u64::from(orbit::MAX_SHEET) * 4),
    }
}

/// Where a capture's answer goes, and its call (see `workers::Offload::agent`).
pub(crate) struct Answer {
    pub(crate) reply: crate::jobs::Reply,
    pub(crate) call: u64,
}

thread_local! {
    /// The packet the last [`save_packet`] on this thread wrote (path and bytes): the capture's
    /// record vouches for exactly these bytes (`crate::records`; read back by
    /// `Engine::offload_capture_made` on the same worker, right after the answer is made).
    static PACKET_WRITTEN: std::cell::RefCell<Option<(PathBuf, Vec<u8>)>> = const { std::cell::RefCell::new(None) };
}

/// Writes a capture's packet text, remembering the bytes written for the capture's record (every
/// sight packet is written through here).
fn write_packet(path: &Path, text: String) -> Result<(), crate::disk::DiskError> {
    let written = crate::disk::book().write(crate::disk::Category::Captures, "sight", path, text.as_bytes(), crate::disk::Need::Agent);
    PACKET_WRITTEN.with(|w| *w.borrow_mut() = written.is_ok().then(|| (path.to_path_buf(), text.into_bytes())));
    written
}

/// Writes a packet next to its PNG; when that fails, the packet says so (and has no path).
fn save_packet(packet: &mut Json, path: Option<&PathBuf>) {
    let Some(path) = path else { return };
    if let Err(e) = write_packet(path, packet.to_pretty()) {
        let mut warnings = packet.get("warnings").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
        warnings.push(Json::from(format!("packet not saved to disk: {e}")));
        packet.set("warnings", Json::Arr(warnings));
        if let Some(mut image) = packet.get("image").cloned() {
            image.set("packet_path", Json::Null);
            packet.set("image", image);
        }
    }
}

/// An overlay report reduced to its counts (orbit views: what was drawn is listed once).
fn overlay_counts(report: &Json) -> Json {
    const COUNTS: [&str; 8] = ["boxes", "drawn", "lines", "entities", "bones", "total", "placed", "omitted"];
    let mut out = Json::obj::<&str>([]);
    if let Json::Obj(fields) = report {
        for (k, v) in fields {
            let Json::Obj(inner) = v else { continue };
            let kept: Vec<(String, Json)> = inner.iter().filter(|(ik, iv)| COUNTS.contains(&ik.as_str()) && matches!(iv, Json::Num(_))).cloned().collect();
            if !kept.is_empty() {
                out.set(k.as_str(), Json::Obj(kept));
            }
        }
    }
    out
}

fn mark_missing(name: &str) -> ToolOutput {
    ToolOutput::error(
        Json::obj([
            ("error", Json::from("mark_not_found")),
            ("message", Json::from(format!("you have no mark named '{name}' (sight_compare action list shows yours)"))),
        ])
        .to_pretty(),
    )
}

/// Whether a subject passes the response filter.
fn passes(s: &Subject, f: SubjectFilter) -> bool {
    match f {
        SubjectFilter::All => true,
        SubjectFilter::OnScreen => s.on_screen,
        SubjectFilter::Visible => s.shows().unwrap_or(s.on_screen),
    }
}

/// A subject trimmed to what an orbit view needs.
fn compact_subject(j: Json) -> Json {
    let mut out = Json::obj::<&str>([]);
    for key in ["id", "path", "screen", "screen_box_exact", "visible_px", "visible_px_approx", "no_pixels", "occluded", "distance_m"] {
        if let Some(v) = j.get(key) {
            out.set(key, v.clone());
        }
    }
    out
}

fn tri_text(t: u64) -> String {
    if t >= 10_000 {
        format!("{}k tri", t / 1000)
    } else if t >= 1000 {
        format!("{:.1}k tri", t as f64 / 1000.0)
    } else {
        format!("{t} tri")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reviewer finding (round 6): approximate counts are lower bounds (possibly 0 for an entity
    /// in plain view); the "visible" filter, overlays and totals must treat them as showing.
    #[test]
    fn approximate_subjects_count_as_visible() {
        let subject = |json: &str| Subject::for_test(1, Json::parse(json).unwrap(), true, 400);
        let approx = subject(r#"{"visible_px":0,"visible_px_approx":true}"#);
        let hidden = subject(r#"{"visible_px":0,"no_pixels":true,"occluded":true}"#);
        let uncounted = subject(r#"{"visible_px":null}"#);
        assert!(passes(&approx, SubjectFilter::Visible));
        assert!(!passes(&hidden, SubjectFilter::Visible));
        assert!(passes(&uncounted, SubjectFilter::Visible), "unknown: falls back to on_screen");
        assert_eq!([&approx, &hidden, &uncounted].map(|s| s.shows()), [Some(true), Some(false), None]);
    }

    #[test]
    fn gpu_failures_are_structured() {
        let lost = specula_gpu::GpuError { message: "vkQueueSubmit failed: VK_ERROR_DEVICE_LOST".into(), vk_result: Some(-4), limit: None, reason: None };
        assert!(lost.device_lost());
        let j = Json::parse(&gpu_failure("render", &lost, false)).unwrap();
        assert_eq!((j.get("error"), j.get("step"), j.get("vk_result")), (Some(&Json::from("gpu_device_lost")), Some(&Json::from("render")), Some(&Json::from(-4))));
        assert!(j.get("recovery").and_then(Json::as_str).is_some_and(|r| r.contains("every later render fails")));
        let j = Json::parse(&gpu_failure("render", &lost, true)).unwrap();
        assert!(j.get("recovery").and_then(Json::as_str).is_some_and(|r| r.contains("the engine stops")));
        let other = specula_gpu::GpuError { message: "out of memory".into(), vk_result: Some(-2), limit: None, reason: None };
        let j = Json::parse(&gpu_failure("readback", &other, false)).unwrap();
        assert_eq!((j.get("error"), j.get("recovery")), (Some(&Json::from("gpu_failed")), None));
    }
}

/// Drawn facts through the engine's real capture path (`Engine::render_view`, the frame record
/// sight_query answers from), on a real GPU (skipped without Vulkan): each way an entity can fail
/// to reach the image is knocked out, and the record must agree with an independent readback of
/// the ID image. Review of the authority lane: an HLOD cell behind the camera said `drawn: true`
/// (proxies are chosen by distance, not view); the hand-built record in
/// `tests/frame_record_gpu.rs` never saw HLOD, streaming or the real draw lists.
#[cfg(test)]
mod frame_record_engine_tests {
    use super::*;
    use crate::journal::Writer;
    use std::collections::BTreeSet;
    use std::time::{Duration, Instant};

    const W: u32 = 320;
    const H: u32 = 180;

    fn engine(tag: &str) -> Option<Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test(&format!("frame_record_engine_tests::{tag}"), "Vulkan is not available");
            return None;
        }
        let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
            Ok(g) => g,
            Err(e) => {
                crate::skip_test(&format!("frame_record_engine_tests::{tag}"), &e.to_string());
                return None;
            }
        };
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let captures = std::env::temp_dir().join(format!("specula-frame-record-{tag}-{}", std::process::id()));
        Some(Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine"))
    }

    fn spawn(e: &mut Engine, spec: &str) -> u64 {
        let spec = crate::tools::parse_spawn(&Json::parse(spec).unwrap()).unwrap();
        let w = Writer::new("t", Some("t"));
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(spec, "t").map(|x| x.id)).unwrap().0
    }

    /// Renders `cam` through the capture path; the record and the entities (or proxy owners)
    /// the readback of the ID image saw.
    fn render(e: &mut Engine, cam: Camera, ids: bool) -> (sight::FrameRecord, BTreeSet<u64>) {
        let none = ov::Spec::default();
        let opts = ViewOpts { include_ui: false, lod: None, time_s: 0.0, overlays: &none, caption: None, ids, hud: true, fill: None };
        let r = e.render_view(cam, W, H, &opts).expect("render");
        let mut seen = BTreeSet::new();
        if ids {
            let target = e.offscreen_target(W, H).unwrap();
            let image = e.gpu.read_ids(target).unwrap();
            seen = image.ids.iter().filter_map(|&i| r.record.owner_of_id(i)).collect();
        }
        (r.record, seen)
    }

    /// Lets builds, uploads and streaming run (capture sources: `cams`) until `done`.
    fn settle(e: &mut Engine, cams: &[Camera], what: &str, mut done: impl FnMut(&mut Engine) -> bool) {
        let start = Instant::now();
        loop {
            e.pump_builds(Some(Duration::from_millis(20)));
            let views: Vec<(Camera, u32, u32)> = cams.iter().map(|c| (*c, W, H)).collect();
            e.stream_capture(&views);
            e.stream_tick(None);
            if done(e) {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(300), "{what}: not settled in 300 s");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The record agrees with the readback: anything seen is drawn; anything said not drawn is
    /// never seen. HLOD members through their proxy's owner.
    fn agrees(record: &sight::FrameRecord, seen: &BTreeSet<u64>, ids: &[u64], proxy: impl Fn(u64) -> Option<u64>) {
        for &id in ids {
            let fact = record.drawn(id).drawn();
            let shown = seen.contains(&id) || proxy(id).is_some_and(|p| seen.contains(&p));
            if shown {
                assert_eq!(fact, Some(true), "entity {id} is in the image, but the record says {fact:?}");
            }
            if fact == Some(false) {
                assert!(!shown, "entity {id}: the record says not drawn, yet the image shows it");
            }
        }
    }

    #[test]
    fn drawn_facts_through_render_view_match_the_readback() {
        let Some(mut e) = engine("plain") else { return };
        let front = spawn(&mut e, r#"{"name": "front", "shape": "cube", "position": [-2, 0, 0]}"#);
        let behind = spawn(&mut e, r#"{"name": "behind", "shape": "cube", "position": [0, 0, 20]}"#);
        let hidden = spawn(&mut e, r#"{"name": "hidden", "shape": "cube", "position": [0, 0, 0], "visible": false}"#);
        let wall = spawn(&mut e, r#"{"name": "wall", "shape": "cube", "position": [2.5, 0, 4], "scale": [3, 3, 0.2]}"#);
        let occluded = spawn(&mut e, r#"{"name": "occluded", "shape": "cube", "position": [2.5, 0, -2], "scale": 0.5}"#);
        // A sprite whose texture file is missing.
        let missing = spawn(&mut e, r#"{"name": "missing", "shape": "sprite", "sprite": {"texture": "no_such_texture.png", "size": [2, 2]}, "position": [0, 2, 0]}"#);
        let cam = Camera::new(Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 0.0), 60.0);
        settle(&mut e, &[cam], "the scene", |e| e.library.lock().in_flight() == 0);
        // A model still building when the frame renders (spawned now, no pump in between).
        let unbuilt = spawn(&mut e, r#"{"name": "unbuilt", "shape": "model", "recipe": "recipes/tombstone.json", "seed": 77, "position": [0, -1, 2]}"#);
        let (record, seen) = render(&mut e, cam, true);
        let all = [front, behind, hidden, wall, occluded, unbuilt, missing];
        agrees(&record, &seen, &all, |_| None);
        assert!(seen.contains(&front) && seen.contains(&wall));
        assert_eq!(record.drawn(front).drawn(), Some(true));
        assert_eq!(record.drawn(behind).drawn(), Some(false), "M4 finding: drawn: true behind the camera");
        assert_eq!(record.drawn(hidden).drawn(), Some(false));
        if !record.submitted().contains(&unbuilt) {
            assert_eq!(record.drawn(unbuilt).drawn(), Some(false), "not built yet: not drawn");
        }
        assert!(!seen.contains(&missing) || record.drawn(missing).drawn() == Some(true), "a missing texture: whatever it draws, the record agrees");
        assert!(!seen.contains(&occluded));
        // The ID pass off, through the same path: drawn is unmeasured, never assumed.
        let (unmeasured, _) = render(&mut e, cam, false);
        let mut j = Json::Obj(Vec::new());
        unmeasured.drawn(front).write(&mut j);
        assert_eq!((j.get("drawn"), j.get("measured_by").and_then(Json::as_str)), (Some(&Json::Null), Some("draw_list")), "{j}");
        assert_eq!(unmeasured.drawn(hidden).drawn(), Some(false), "not submitted is known without the ID pass");
    }

    #[test]
    fn hlod_members_behind_the_camera_are_not_drawn() {
        let Some(mut e) = engine("hlod") else { return };
        let patch = Json::parse(r#"{"settle_s": 0, "upload_ms": 50, "upload_mb": 64, "hlod_min_m": 0}"#).unwrap();
        e.stream.set(&patch, "test", &mut e.gpu, &mut e.models).unwrap();
        let mut members = Vec::new();
        for i in 0..16u64 {
            let spec = format!(
                r#"{{"name": "stone{i}", "shape": "model", "recipe": "recipes/tombstone.json", "seed": {}, "position": [{}, 0, {}], "scale": 4}}"#,
                100 + i,
                300 + (i % 4) * 6,
                (i / 4) * 6
            );
            members.push(spawn(&mut e, &spec));
        }
        let toward = Camera::new(Vec3::new(0.0, 30.0, 10.0), Vec3::new(300.0, 0.0, 10.0), 40.0);
        let away = Camera::new(Vec3::new(0.0, 30.0, 10.0), Vec3::new(-300.0, 0.0, 10.0), 40.0);
        let first = members[0];
        settle(&mut e, &[toward], "the far cell gets a proxy", |e| {
            let (record, _) = render(e, toward, true);
            let mut j = Json::Obj(Vec::new());
            record.drawn(first).write(&mut j);
            j.get("drawn_as").and_then(Json::as_str) == Some("hlod")
        });
        let proxy = |r: &sight::FrameRecord| {
            let r = r.clone();
            move |id: u64| r.proxy_owner_of(id)
        };
        let (record, seen) = render(&mut e, toward, true);
        agrees(&record, &seen, &members, proxy(&record));
        assert!(members.iter().any(|m| record.drawn(*m).drawn() == Some(true)), "the cell in view is drawn as its proxy");
        // Review round 2: the bounds overlay painted HLOD members red (no pixels of their own).
        let bounds = ov::Spec { kinds: vec![Kind::Bounds], entities: Vec::new(), max_labels: 0, ui_display: None };
        let opts = ViewOpts { include_ui: false, lod: None, time_s: 0.0, overlays: &bounds, caption: None, ids: true, hud: true, fill: None };
        let r = e.render_view(toward, W, H, &opts).expect("render");
        let hidden = r.overlays.as_ref().and_then(|o| o.get("bounds")).and_then(|b| b.get("fully_hidden")).and_then(Json::as_u64);
        let in_view_merged = members.iter().filter(|m| r.record.drawn(**m).drawn() == Some(true) && r.record.drawn(**m).drawn_as() == Some("hlod")).count();
        assert!(in_view_merged > 0);
        let subjects_hidden = r.subjects.iter().filter(|s| members.contains(&s.id) && s.on_screen && fully_hidden(s, Some(&r.record))).count();
        assert_eq!(subjects_hidden, 0, "an HLOD member drawn in its proxy isn't fully hidden");
        assert_eq!(hidden, Some(0), "the bounds overlay paints none of them red: {:?}", r.overlays);
        // The same cell behind the camera: still drawn as a proxy (distance decides), culled.
        let (record, seen) = render(&mut e, away, true);
        agrees(&record, &seen, &members, proxy(&record));
        let mut merged = 0;
        for &m in &members {
            let mut j = Json::Obj(Vec::new());
            record.drawn(m).write(&mut j);
            if j.get("drawn_as").and_then(Json::as_str) == Some("hlod") {
                merged += 1;
                assert_eq!(j.get("drawn"), Some(&Json::Bool(false)), "a proxy behind the camera: {j}");
                assert_eq!(j.get("measured_by").and_then(Json::as_str), Some("id_buffer"), "{j}");
            }
        }
        assert!(merged > 0, "the cell behind the camera is still drawn as its proxy (distance decides): the case under test");
    }

    #[test]
    fn not_resident_models_are_not_drawn() {
        let Some(mut e) = engine("resident") else { return };
        let patch = Json::parse(r#"{"settle_s": 0, "upload_ms": 50, "upload_mb": 64, "hlod": false, "radius_m": 20}"#).unwrap();
        e.stream.set(&patch, "test", &mut e.gpu, &mut e.models).unwrap();
        let far = spawn(&mut e, r#"{"name": "far", "shape": "model", "recipe": "recipes/tombstone.json", "seed": 5, "position": [0, 0, -400]}"#);
        let cam = Camera::new(Vec3::new(0.0, 2.0, 10.0), Vec3::new(0.0, 0.0, -400.0), 60.0);
        settle(&mut e, &[cam], "the build", |e| e.library.lock().in_flight() == 0);
        let (record, seen) = render(&mut e, cam, true);
        agrees(&record, &seen, &[far], |_| None);
        let mut j = Json::Obj(Vec::new());
        record.drawn(far).write(&mut j);
        assert_eq!(j.get("drawn_as").and_then(Json::as_str), Some("not_resident"), "400 m out with a 20 m radius: left off the GPU: {j}");
        assert_eq!(j.get("drawn"), Some(&Json::Bool(false)), "{j}");
        assert!(!seen.contains(&far));
    }
}
