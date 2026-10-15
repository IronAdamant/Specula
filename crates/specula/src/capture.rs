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
use std::sync::mpsc::Sender;

/// A capture-like call parked until the models it would show are built.
pub(crate) struct CaptureWaiter {
    pub(crate) job: Job,
    pub(crate) session: SessionInfo,
    pub(crate) reply: Sender<ToolOutput>,
    /// Rendered then whatever is still building.
    pub(crate) until: Instant,
    pub(crate) wait: CaptureWait,
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
}

/// One rendered view.
pub(crate) struct Rendered {
    pub(crate) rgba: Vec<u8>,
    pub(crate) subjects: Vec<Subject>,
    pub(crate) stats: LodStats,
    pub(crate) lights: usize,
    pub(crate) dropped_lights: usize,
    /// What the overlays drew (`None`: none asked for).
    pub(crate) overlays: Option<Json>,
    /// The ID buffer's entity count and cost.
    pub(crate) ids: Json,
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
    /// sight_frame, the sight tools and the camera tools.
    pub(crate) fn handle_sight(&mut self, request: Request) {
        let Request { command, session, reply, deadline } = request;
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
        for e in self.sync_meshes() {
            eprintln!("specula: {e}");
        }
        self.models.refresh_ready(&mut self.gpu);
        let views = match self.job_views(&job, &session) {
            Ok(v) => v,
            Err(out) => {
                let _ = reply.send(out);
                return;
            }
        };
        if self.building_in(&views) == 0 {
            let _ = reply.send(self.run_job(&job, &session, wait));
            return;
        }
        // Answer before the caller's own deadline, building or not.
        let usual = Instant::now() + CAPTURE_BUILD_WAIT;
        let latest = deadline.checked_sub(Duration::from_secs(2)).unwrap_or(deadline);
        wait.parked_at = Some(Instant::now());
        wait.cut_by_deadline = latest < usual;
        self.capture_waiters.push(CaptureWaiter { job, session, reply, until: usual.min(latest), wait });
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
                Ok(views) => self.building_in(&views),
                Err(out) => {
                    let _ = w.reply.send(out);
                    continue;
                }
            };
            if now >= w.until || building == 0 {
                let wait = CaptureWait { cut_by_deadline: w.wait.cut_by_deadline && now >= w.until, ..w.wait };
                let _ = w.reply.send(self.run_job(&w.job, &w.session, wait));
            } else {
                self.capture_waiters.push(w);
            }
        }
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
            _ => (sight::auto_camera(&self.world, spec.fov_y_deg), source("auto"), None),
        };
        Ok(ViewReq { camera, width: spec.width, height: spec.height, include_ui: spec.include_ui, source, warning })
    }

    /// The views a job renders (its first view is the main one).
    fn job_views(&mut self, job: &Job, session: &SessionInfo) -> Result<Vec<ViewReq>, ToolOutput> {
        match job {
            Job::Frame(spec) | Job::Mark { frame: spec, .. } => Ok(vec![self.frame_view(spec, session)?]),
            Job::Query(q) => Ok(vec![self.frame_view(&q.frame, session)?]),
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
                let set = self.swarm.commit(&mut self.world, tick, w, journal::Kind::Camera, |wd| wd.set_camera(&name, spec, persisted).cloned());
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
                match self.swarm.commit(&mut self.world, tick, w, journal::Kind::Camera, |wd| wd.delete_cameras(&name, cam.as_deref())) {
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

    fn run_job(&mut self, job: &Job, session: &SessionInfo, wait: CaptureWait) -> ToolOutput {
        // Presentation again only if the simulation moved on while the capture was parked.
        if self.game.is_some() && wait.presented_at != Some(self.clock.tick()) {
            self.before_capture();
        }
        self.pump_builds(None);
        let views = match self.job_views(job, session) {
            Ok(v) => v,
            Err(out) => return out,
        };
        let building = self.building_in(&views);
        match job {
            Job::Frame(spec) => self.capture_frame(spec, session, wait, &views[0], building),
            Job::Mark { name, frame } => self.compare_mark(name, frame, session, &views[0]),
            Job::Diff { name, threshold, max_subjects } => self.compare_diff(name, *threshold, *max_subjects, session),
            Job::Orbit(o) => self.orbit(o, session, &views, building),
            Job::Query(q) => self.sight_query(q, &views[0]),
        }
    }

    /// Warnings every capture shares: failed uploads, models not drawn, dropped lights, the
    /// camera's, and validation messages.
    fn capture_warnings(&mut self, r: &Rendered, wait: CaptureWait, building: usize, view: &ViewReq) -> Vec<Json> {
        let mut warnings: Vec<Json> = self.lathes.failures().chain(self.models.failures()).take(10).map(|m| Json::from(m.as_str())).collect();
        if r.stats.missing > 0 {
            let waited = match (wait.parked_at, wait.cut_by_deadline) {
                (None, _) => "none was in frame when the call came".to_owned(),
                (Some(t), false) => format!("waited {:.1} s", t.elapsed().as_secs_f64()),
                (Some(t), true) => format!("waited {:.1} s, cut short by the call's deadline", t.elapsed().as_secs_f64()),
            };
            warnings.push(Json::from(format!(
                "{} model entit{} not drawn: {building} in frame still building ({waited}), others failed or \
                 started building since (world_query fields model_state, engine_info models.library.broken)",
                r.stats.missing,
                if r.stats.missing == 1 { "y" } else { "ies" },
            )));
        }
        {
            let lib = self.library.lock();
            let problems = self.world.entities().filter(|e| e.anim.is_some()).filter_map(|e| models::anim_problem(&lib, e));
            warnings.extend(problems.take(5).map(Json::from));
        }
        // Models held back because their pipeline variant failed to build.
        warnings.extend(self.world.entities().filter_map(|e| self.models.pipeline_problem(e)).take(5).map(Json::from));
        if r.dropped_lights > 0 {
            warnings.push(Json::from(format!(
                "{} point light(s) not rendered: at most {} per frame",
                r.dropped_lights,
                specula_gpu::MAX_POINT_LIGHTS
            )));
        }
        if let Some(w) = &view.warning {
            warnings.push(Json::from(w.as_str()));
        }
        for m in self.gpu.take_validation_messages() {
            if m.severity >= Severity::Warning {
                warnings.push(Json::from(format!("vulkan {:?} {}: {}", m.severity, m.id_name, m.message)));
            }
        }
        warnings
    }

    /// Renders one view (see the module docs).
    ///
    /// The frame renders once, with the ID buffer (unless `opts.ids` is off), whose counts go
    /// into the subjects. Overlays are drawn in that same render (the ID passes come after them
    /// and ignore them), placed by the projected boxes; only the `bounds` overlay needs the
    /// counts (fully hidden boxes are red), so with it the frame renders a second time, overlays
    /// on top and without the ID passes (the scene is the same, bit for bit).
    pub(crate) fn render_view(&mut self, camera: Camera, w: u32, h: u32, opts: &ViewOpts) -> Result<Rendered, String> {
        let view_proj = camera.view_proj(w, h);
        let mut draws = Vec::new();
        let mut owners = Vec::new();
        let (stats, _) = self.build_draws(camera, h, opts.include_ui, opts.lod, false, opts.time_s, &mut draws, Some(&mut owners));
        self.lod_capture = stats;
        let (lights, dropped_lights) = sight::lights(&self.world, opts.include_ui);
        let mut subjects = sight::subjects(&self.world, &view_proj, w, h, opts.include_ui);
        let (ids, entities) = sight::dense_ids(&owners);
        let target = self.offscreen_target(w, h)?;
        // Animated models' bone matrices, posed at time_s by build_draws (returned below; an
        // error path just drops them).
        let bones = self.models.take_bones();
        let mut frame = FrameDesc { bones: &bones, ..sight::frame(&self.world, &view_proj, &draws, &lights, opts.time_s as f32) };
        self.luts.apply(&mut self.gpu, self.world.env(), &mut frame, true);
        let painted = !opts.overlays.is_empty() || opts.caption.is_some();
        let second_render = painted && opts.ids && opts.overlays.has(Kind::Bounds);
        let mut overlays = None;
        let mut quads = Vec::new();
        if painted && !second_render {
            let (q, report) = self.paint_overlays(camera, view_proj, w, h, &subjects, opts);
            quads = q;
            overlays = (!opts.overlays.is_empty()).then_some(report);
        }
        frame.overlay = &quads;
        {
            let _z = profile::zone("sight.render");
            let rendered = if opts.ids { self.gpu.render_with_ids(target, &frame, &ids) } else { self.gpu.render(target, &frame) };
            rendered.map_err(|e| gpu_failure("render", &e, self.windowed))?;
        }
        let ids_json = if opts.ids {
            let gpu_ms = self.gpu.last_frame_stats_of(FramePath::Offscreen).and_then(|s| s.gpu).and_then(|g| {
                let ms: f64 = g.passes.iter().filter(|p| p.name == specula_gpu::PASS_ID_BUFFER).map(|p| p.gpu_ms).sum();
                (ms > 0.0).then_some(ms)
            });
            let counted = Instant::now();
            let counts = {
                let _z = profile::zone("sight.ids");
                self.gpu.id_coverage(target, entities.len() as u32).map_err(|e| gpu_failure("id readback", &e, self.windowed))?
            };
            sight::apply_visibility(&mut subjects, &entities, &counts.per_id);
            let cpu_ms = counted.elapsed().as_secs_f64() * 1e3;
            Json::obj([
                ("entities", Json::from(entities.len())),
                ("visible", Json::from(counts.per_id.iter().filter(|c| c.pixels > 0 || c.approx).count())),
                ("blended_counted", Json::from(counts.blended_counted)),
                ("gpu_ms", Json::from(gpu_ms)),
                ("readback_count_ms", Json::from((cpu_ms * 1000.0).round() / 1000.0)),
            ])
        } else {
            Json::obj([("off", Json::from(true))])
        };
        if second_render {
            let (q, report) = self.paint_overlays(camera, view_proj, w, h, &subjects, opts);
            overlays = (!opts.overlays.is_empty()).then_some(report);
            frame.overlay = &q;
            let _z = profile::zone("sight.render");
            self.gpu.render(target, &frame).map_err(|e| gpu_failure("render", &e, self.windowed))?;
        }
        let rgba = self.gpu.read_rgba8(target).map_err(|e| gpu_failure("readback", &e, self.windowed))?;
        self.models.put_bones(bones);
        Ok(Rendered { rgba, subjects, stats, lights: lights.len(), dropped_lights, overlays, ids: ids_json })
    }

    /// The overlay records for one view and what they drew.
    fn paint_overlays(&mut self, camera: Camera, vp: Mat4, w: u32, h: u32, subjects: &[Subject], opts: &ViewOpts) -> (Vec<OverlayQuad>, Json) {
        let spec = opts.overlays;
        let mut p = Painter::new(vp, w, h);
        let mut report = ov::Report::default();
        let world = &self.world;
        // On-screen subjects the overlays may mark, biggest first.
        let mut shown: Vec<(&Subject, &world::Entity, String)> = subjects
            .iter()
            .filter(|s| s.on_screen)
            .filter_map(|s| {
                let path = s.json.get("path").and_then(Json::as_str).unwrap_or_default().to_owned();
                (spec.selects(&path)).then(|| world.get(s.id).map(|e| (s, e, path))).flatten()
            })
            .collect();
        shown.sort_by(|a, b| b.0.projected_area.cmp(&a.0.projected_area).then(a.0.id.cmp(&b.0.id)));
        let centre = |p: &Painter, e: &world::Entity, s: &Subject| -> [f32; 2] {
            let (lo, hi) = e.world_aabb();
            let c = p.project((lo + hi) * 0.5);
            let r = s.json.get("screen").and_then(Json::as_array);
            let rect_centre = r.map(|r| {
                let v = |i: usize| r[i].as_f64().unwrap_or(0.0) as f32;
                [v(0) + v(2) * 0.5, v(1) + v(3) * 0.5]
            });
            match (c, rect_centre) {
                (Some(c), Some(rc)) => {
                    // The box centre projects outside its own clipped rectangle when the box
                    // reaches past the image edge: use the rectangle's centre then.
                    let r = r.expect("some");
                    let v = |i: usize| r[i].as_f64().unwrap_or(0.0) as f32;
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
                let hidden = sight::shows(&s.json) == Some(false);
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
        if spec.has(Kind::Labels) {
            for (s, e, _) in shown.iter().take(spec.max_labels * 4) {
                let at = centre(&p, e, s);
                p.label(&e.name, at, ov::WHITE, s.projected_area);
            }
            report.drawn.push("labels");
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
        if p.dropped > 0 {
            report.details.push(("records_dropped", Json::from(p.dropped)));
        }
        (quads, report.to_json())
    }

    /// sight_frame.
    fn capture_frame(&mut self, spec: &FrameSpec, session: &SessionInfo, wait: CaptureWait, view: &ViewReq, building: usize) -> ToolOutput {
        let (w, h) = (spec.width, spec.height);
        let time_s = spec.time_s.map_or_else(|| self.time_s(), f64::from);
        let opts = ViewOpts { include_ui: spec.include_ui, lod: spec.lod, time_s, overlays: &spec.overlays, caption: None, ids: spec.ids };
        let r = match self.render_view(view.camera, w, h, &opts) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(e),
        };
        let mut warnings = self.capture_warnings(&r, wait, building, view);
        let png_bytes = encode_png(w, h, &r.rgba);
        self.frames_captured += 1;
        let png_path = self.save_png(&png_bytes);
        if let Err(e) = &png_path {
            warnings.push(Json::from(format!("capture not saved to disk: {e}")));
        }
        // The full packet is saved next to its PNG so a verifier can re-check both later.
        let png_file = png_path.as_ref().ok();
        let packet_path = png_file.map(|p| p.with_extension("json"));
        let show = |p: Option<&PathBuf>| -> Json { p.map(|p| p.display().to_string()).into() };
        let image = Json::obj([
            ("width", Json::from(w)),
            ("height", Json::from(h)),
            ("png_bytes", Json::from(png_bytes.len())),
            ("path", show(png_file)),
            ("packet_path", show(packet_path.as_ref())),
        ]);
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
            ("image", image),
            ("camera", camera),
            ("time_s", Json::from(time_s)),
            ("include_ui", Json::from(spec.include_ui)),
            ("models", r.stats.to_json()),
            ("lights", Json::from(r.lights)),
            ("id_buffer", r.ids.clone()),
            ("subjects_total", Json::from(r.subjects.len())),
            ("subjects_on_screen", Json::from(on_screen)),
            ("subjects", Json::arr(r.subjects.iter().map(|s| s.json.clone()))),
            ("warnings", Json::Arr(warnings.clone())),
            // Structured problems (objects), next to the plain-text warnings.
            ("problems", Json::Arr(self.luts.warning(self.world.env()).into_iter().chain(self.models.pipeline_failure_json()).collect())),
        ]);
        if let Some(o) = &r.overlays {
            packet.set("overlays", o.clone());
        }
        if let Some(path) = &packet_path {
            if let Err(e) = std::fs::write(path, packet.to_pretty()) {
                warnings.push(Json::from(format!("packet not saved to disk: {e}")));
                packet.set("warnings", Json::Arr(warnings));
            }
        }
        // The response carries the most relevant subjects only; agents' context is precious.
        let filtered: Vec<Subject> = r.subjects.iter().filter(|s| passes(s, spec.subjects)).cloned().collect();
        let (chosen, omitted) = sight::most_relevant(&filtered, spec.max_subjects);
        packet.set("subjects", Json::Arr(chosen));
        packet.set("subjects_omitted", Json::from(omitted + (r.subjects.len() - filtered.len())));
        if spec.subjects != SubjectFilter::All {
            packet.set("subjects_filter", Json::from(spec.subjects.name()));
        }
        ToolOutput {
            content: vec![
                Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) },
                Content::Text(packet.to_pretty()),
            ],
            is_error: false,
        }
    }

    /// sight_orbit.
    fn orbit(&mut self, o: &OrbitSpec, session: &SessionInfo, views: &[ViewReq], building: usize) -> ToolOutput {
        let (center, radius, path) = match self.orbit_target(&o.target) {
            Ok(t) => t,
            Err(out) => return out,
        };
        let distance = o.distance.unwrap_or_else(|| orbit::fit_distance(radius, o.fov_y_deg));
        let (tw, th, sw, sh) = orbit::layout(o.views, o.tile);
        let time_s = o.time_s.map_or_else(|| self.time_s(), f64::from);
        let mut sheet = vec![0u8; (sw * sh * 4) as usize];
        for px in sheet.chunks_exact_mut(4) {
            px.copy_from_slice(&[24, 26, 30, 255]);
        }
        let mut out_views = Vec::new();
        let mut warnings = Vec::new();
        let mut overlays = None;
        for (i, v) in views.iter().enumerate() {
            let (_, az) = orbit::view(center, distance, o.elevation_deg, o.start_deg, i as u32, o.views, o.fov_y_deg);
            let caption = format!("{} az {}", i + 1, az.round());
            let opts = ViewOpts { include_ui: o.include_ui, lod: o.lod, time_s, overlays: &o.overlays, caption: Some(caption), ids: true };
            let r = match self.render_view(v.camera, tw, th, &opts) {
                Ok(r) => r,
                Err(e) => return ToolOutput::error(e),
            };
            if i == 0 {
                warnings = self.capture_warnings(&r, CaptureWait { presented_at: None, parked_at: None, cut_by_deadline: false }, building, v);
            }
            if overlays.is_none() {
                overlays = r.overlays.as_ref().and_then(|j| j.get("drawn")).map(|d| Json::obj([("drawn", d.clone()), ("per_view", Json::from("each view's own counts are in views[].overlays"))]));
            }
            let (x, y) = orbit::tile_origin(i as u32, o.views, tw, th);
            compare::blit(&mut sheet, sw, &r.rgba, tw, th, x, y);
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
            out_views.push(view);
        }
        let png_bytes = encode_png(sw, sh, &sheet);
        self.frames_captured += 1;
        let png_path = self.save_png(&png_bytes);
        if let Err(e) = &png_path {
            warnings.push(Json::from(format!("capture not saved to disk: {e}")));
        }
        let png_file = png_path.as_ref().ok();
        let packet_path = png_file.map(|p| p.with_extension("json"));
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("orbit")),
            ("frame", Json::from(self.frames_captured)),
            ("requested_by", Json::from(self.swarm.agent_name(session))),
            (
                "image",
                Json::obj([
                    ("width", Json::from(sw)),
                    ("height", Json::from(sh)),
                    ("png_bytes", Json::from(png_bytes.len())),
                    ("path", png_file.map(|p| p.display().to_string()).into()),
                    ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
                    ("grid", Json::arr({
                        let (c, r) = orbit::grid(o.views);
                        [c, r].map(Json::from)
                    })),
                ]),
            ),
            ("target", Json::obj([("path", Json::from(path)), ("center", world::vec3_json(center)), ("radius_m", Json::from(radius))])),
            ("distance_m", Json::from(distance)),
            ("elevation_deg", Json::from(o.elevation_deg)),
            ("fov_y_deg", Json::from(o.fov_y_deg)),
            ("time_s", Json::from(time_s)),
            ("views", Json::Arr(out_views)),
            ("warnings", Json::Arr(warnings)),
        ]);
        if let Some(ov) = overlays {
            packet.set("overlays", ov);
        }
        save_packet(&mut packet, packet_path.as_ref());
        ToolOutput {
            content: vec![
                Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) },
                Content::Text(packet.to_string()),
            ],
            is_error: false,
        }
    }

    /// sight_compare mark.
    fn compare_mark(&mut self, name: &str, spec: &FrameSpec, session: &SessionInfo, view: &ViewReq) -> ToolOutput {
        let time_s = spec.time_s.map_or_else(|| self.time_s(), f64::from);
        let opts = ViewOpts { include_ui: spec.include_ui, lod: spec.lod, time_s, overlays: &spec.overlays, caption: None, ids: spec.ids };
        let r = match self.render_view(view.camera, spec.width, spec.height, &opts) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(e),
        };
        let warnings = self.capture_warnings(&r, CaptureWait { presented_at: None, parked_at: None, cut_by_deadline: false }, 0, view);
        let png_bytes = encode_png(spec.width, spec.height, &r.rgba);
        self.frames_captured += 1;
        let png_path = self.save_png(&png_bytes).ok().map(|p| p.display().to_string());
        // Only what was on screen, plus every entity's id (to tell new entities from old ones).
        let subjects = r.subjects.iter().filter(|s| s.on_screen).filter_map(|s| SubjectState::from_json(&s.json)).collect();
        let mut ids: Vec<u64> = self.world.entities().map(|e| e.id).collect();
        ids.sort_unstable();
        let mark_view = MarkView {
            camera: view.camera,
            width: spec.width,
            height: spec.height,
            include_ui: spec.include_ui,
            lod: spec.lod,
            time_s: spec.time_s,
            overlays: spec.overlays.clone(),
            ids: spec.ids,
        };
        let key = self.swarm.key(session);
        let data = MarkData { rgba: r.rgba, subjects, ids, frame: self.frames_captured, tick: self.clock.tick(), png_path: png_path.clone() };
        let evicted = self.marks.put(&key, name, mark_view, data);
        let out = Json::obj([
            ("marked", Json::from(name)),
            ("frame", Json::from(self.frames_captured)),
            ("tick", Json::from(self.clock.tick())),
            ("camera", view.camera.to_json()),
            ("size", Json::arr([spec.width, spec.height].map(Json::from))),
            ("time_s", spec.time_s.map_or(Json::from("live"), Json::from)),
            ("subjects_on_screen", Json::from(r.subjects.iter().filter(|s| s.on_screen).count())),
            ("path", Json::from(png_path)),
            ("evicted", Json::arr(evicted.into_iter().map(Json::from))),
            ("warnings", Json::Arr(warnings)),
        ]);
        ToolOutput::text(out.to_pretty())
    }

    /// sight_compare diff.
    fn compare_diff(&mut self, name: &str, threshold: u8, max_subjects: usize, session: &SessionInfo) -> ToolOutput {
        let key = self.swarm.key(session);
        let Some(mark) = self.marks.get(&key, name) else { return mark_missing(name) };
        let view = mark.view.clone();
        let (marked_frame, marked_tick) = (mark.data.frame, mark.data.tick);
        let time_s = view.time_s.map_or_else(|| self.time_s(), f64::from);
        let opts = ViewOpts { include_ui: view.include_ui, lod: view.lod, time_s, overlays: &view.overlays, caption: None, ids: view.ids };
        let r = match self.render_view(view.camera, view.width, view.height, &opts) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(e),
        };
        let req = ViewReq { camera: view.camera, width: view.width, height: view.height, include_ui: view.include_ui, source: source("mark"), warning: None };
        let mut warnings = self.capture_warnings(&r, CaptureWait { presented_at: None, parked_at: None, cut_by_deadline: false }, 0, &req);
        let mark = &self.marks.get(&key, name).expect("still there: nothing ran in between").data;
        let (w, h) = (view.width, view.height);
        let d = compare::diff(&mark.rgba, &r.rgba, w, h, threshold);
        let heat = compare::heat(&r.rgba, &d, threshold);
        let after: BTreeMap<u64, SubjectState> =
            r.subjects.iter().filter(|s| s.on_screen).filter_map(|s| SubjectState::from_json(&s.json)).collect();
        let world = &self.world;
        let subjects = compare::subject_diff(&mark.subjects, &mark.ids, &after, &|id| world.get(id).is_some(), 2, max_subjects);
        let (sheet, sw, sh, factor) = compare::side_by_side(&[&mark.rgba, &r.rgba, &heat], w, h, 4, orbit::MAX_SHEET);
        let png_bytes = encode_png(sw, sh, &sheet);
        self.frames_captured += 1;
        let png_path = self.save_png(&png_bytes);
        if let Err(e) = &png_path {
            warnings.push(Json::from(format!("capture not saved to disk: {e}")));
        }
        let png_file = png_path.as_ref().ok();
        let packet_path = png_file.map(|p| p.with_extension("json"));
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("compare")),
            ("name", Json::from(name)),
            ("frame", Json::from(self.frames_captured)),
            ("marked_frame", Json::from(marked_frame)),
            ("ticks_since_mark", Json::from(self.clock.tick().saturating_sub(marked_tick))),
            ("camera", view.camera.to_json()),
            (
                "image",
                Json::obj([
                    ("width", Json::from(sw)),
                    ("height", Json::from(sh)),
                    ("panels", Json::arr(["before", "after", "heat"].map(Json::from))),
                    ("scale", Json::from(1.0 / factor as f64)),
                    ("path", png_file.map(|p| p.display().to_string()).into()),
                    ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
                ]),
            ),
            ("threshold", Json::from(u32::from(threshold))),
            ("pixels", d.to_json()),
            ("subjects", subjects),
            ("warnings", Json::Arr(warnings)),
        ]);
        save_packet(&mut packet, packet_path.as_ref());
        ToolOutput {
            content: vec![
                Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) },
                Content::Text(packet.to_pretty()),
            ],
            is_error: false,
        }
    }

    /// sight_query: one entity, no image.
    fn sight_query(&mut self, q: &QuerySpec, view: &ViewReq) -> ToolOutput {
        let id = match self.world.resolve(&q.key) {
            Ok(id) => id,
            Err(e) => return world_error(&e),
        };
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
        if let Some(r) = reason {
            out.set("drawn", Json::from(false));
            out.set("reason", Json::from(r));
            out.set("on_screen", Json::from(false));
            out.set("camera", view.camera.to_json());
            return ToolOutput::text(out.to_string());
        }
        // The ID buffer needs the frame rendered (no PNG is made or sent).
        let time_s = q.frame.time_s.map_or_else(|| self.time_s(), f64::from);
        let none = ov::Spec::default();
        let opts = ViewOpts { include_ui: view.include_ui, lod: q.frame.lod, time_s, overlays: &none, caption: None, ids: true };
        let r = match self.render_view(view.camera, view.width, view.height, &opts) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(e),
        };
        let Some(s) = r.subjects.into_iter().find(|s| s.id == id) else {
            return ToolOutput::error(format!("{path} was not in the frame it was queried in"));
        };
        out.set("drawn", Json::from(true));
        for key in ["on_screen", "screen", "distance_m", "visible_px", "visible_px_approx", "screen_box_exact", "no_pixels", "occluded"] {
            if let Some(v) = s.json.get(key) {
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
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let _z = profile::zone("sight.encode");
    png::encode_rgba8(w, h, rgba, Level::Fastest)
}

impl Engine {
    fn save_png(&self, bytes: &[u8]) -> Result<PathBuf, String> {
        let _z = profile::zone("sight.save");
        self.save_capture("png", bytes)
    }
}

/// Writes a packet next to its PNG; when that fails, the packet says so (and has no path).
fn save_packet(packet: &mut Json, path: Option<&PathBuf>) {
    let Some(path) = path else { return };
    if let Err(e) = std::fs::write(path, packet.to_pretty()) {
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
        SubjectFilter::Visible => sight::shows(&s.json).unwrap_or(s.on_screen),
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
        let subject = |json: &str| Subject { id: 1, json: Json::parse(json).unwrap(), on_screen: true, area: 0, projected_area: 400 };
        let approx = subject(r#"{"visible_px":0,"visible_px_approx":true}"#);
        let hidden = subject(r#"{"visible_px":0,"no_pixels":true,"occluded":true}"#);
        let uncounted = subject(r#"{"visible_px":null}"#);
        assert!(passes(&approx, SubjectFilter::Visible));
        assert!(!passes(&hidden, SubjectFilter::Visible));
        assert!(passes(&uncounted, SubjectFilter::Visible), "unknown: falls back to on_screen");
        assert_eq!([&approx, &hidden, &uncounted].map(|s| sight::shows(&s.json)), [Some(true), Some(false), None]);
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
