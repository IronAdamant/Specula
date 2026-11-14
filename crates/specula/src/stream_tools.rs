//! The streaming tools: `stream_state`, `stream_settings` and `stream_fly` (a measured flight of
//! the window camera along a path), and the engine's streaming glue. See [`crate::streaming`].

use crate::perf::Stats;
use crate::schedule::Class;
use crate::sight::Camera;
use crate::tools::{check_keys, int_arg, num_in};
use crate::world::WorldError;
use crate::Engine;
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::time::{Duration, Instant};

/// Points a flight path may have.
pub const MAX_PATH_POINTS: usize = 64;
/// Longest flight.
pub const MAX_FLY_S: f64 = 120.0;
/// Frames a flight records at most (about 120 s at 144 Hz).
const MAX_FLY_FRAMES: usize = 20_000;

pub enum StreamCall {
    State { near: Option<Vec3>, limit: usize },
    Settings(Json),
    Fly(FlySpec),
}

impl StreamCall {
    pub fn class(&self) -> Class {
        match self {
            StreamCall::State { .. } => Class::Read,
            _ => Class::Edit,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlySpec {
    pub path: Vec<Vec3>,
    pub speed_mps: f32,
    /// Look at this point the whole way (default: ahead along the path).
    pub look_at: Option<Vec3>,
    pub look_ahead_m: f32,
    pub pitch_deg: f32,
    pub fov_y_deg: f32,
}

fn point(v: &Json, what: &str) -> Result<Vec3, String> {
    let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| format!("{what} must be [x, y, z]"))?;
    let n: Vec<f32> = a.iter().map(|x| x.as_f64().filter(|x| x.is_finite() && x.abs() <= 1.0e6).map(|x| x as f32)).collect::<Option<_>>().ok_or_else(|| format!("{what}: coordinates must be numbers within ±1e6 m"))?;
    Ok(Vec3::new(n[0], n[1], n[2]))
}

pub fn parse(name: &str, args: &Json) -> Option<Result<StreamCall, String>> {
    let call = match name {
        "stream_state" => (|| {
            check_keys(args, name, &[&["near", "limit"]])?;
            let near = match args.get("near") {
                None | Some(Json::Null) => None,
                Some(v) => Some(point(v, "'near'")?),
            };
            Ok(StreamCall::State { near, limit: int_arg(args, "limit", 1, crate::streaming::MAX_LISTED as u64)?.unwrap_or(16) as usize })
        })(),
        "stream_settings" => (|| {
            let keys = crate::streaming::setting_keys();
            check_keys(args, name, &[&keys])?;
            Ok(StreamCall::Settings(args.clone()))
        })(),
        "stream_fly" => (|| {
            check_keys(args, name, &[&["path", "speed_mps", "look_at", "look_ahead_m", "pitch_deg", "fov_y_deg"]])?;
            let pts = args.get("path").and_then(Json::as_array).ok_or("'path' is required: [[x, y, z], ...], 2 to 64 points")?;
            if !(2..=MAX_PATH_POINTS).contains(&pts.len()) {
                return Err(format!("'path' must have 2 to {MAX_PATH_POINTS} points (got {})", pts.len()));
            }
            let path = pts.iter().enumerate().map(|(i, p)| point(p, &format!("path[{i}]"))).collect::<Result<Vec<_>, _>>()?;
            let speed_mps = num_in(args, "speed_mps", 0.5..=2000.0)?.unwrap_or(20.0);
            let length: f32 = path.windows(2).map(|w| (w[1] - w[0]).length()).sum();
            if length < 0.01 {
                return Err("'path' has no length".into());
            }
            if f64::from(length / speed_mps) > MAX_FLY_S {
                return Err(format!("the flight would take {:.0} s; at most {MAX_FLY_S} s (raise speed_mps or shorten the path)", length / speed_mps));
            }
            let look_at = match args.get("look_at") {
                None | Some(Json::Null) => None,
                Some(v) => Some(point(v, "'look_at'")?),
            };
            Ok(StreamCall::Fly(FlySpec {
                path,
                speed_mps,
                look_at,
                look_ahead_m: num_in(args, "look_ahead_m", 1.0..=1000.0)?.unwrap_or(40.0),
                pitch_deg: num_in(args, "pitch_deg", -80.0..=80.0)?.unwrap_or(12.0),
                fov_y_deg: num_in(args, "fov_y_deg", 10.0..=120.0)?.unwrap_or(60.0),
            }))
        })(),
        _ => return None,
    };
    Some(call)
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let vec3 = |d: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("number"))])),
            ("minItems", Json::from(3)),
            ("maxItems", Json::from(3)),
            ("description", Json::from(d)),
        ])
    };
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let def = |name: &str, d: &str, input_schema: Json| ToolDef { name: name.into(), description: d.into(), input_schema };
    vec![
        def(
            "stream_state",
            "What of the world is on the GPU (streaming never changes the simulation, only what is drawn): resident and \
             budget MB, builds wanted/pending/partial, this frame's uploads and the upload-ms series, evictions, budget misses, \
             HLOD proxies (cells current/stale/building, what the window draws merged), and the cells nearest `near` (default: \
             the window's eye) with their members, how many are resident, their proxy and switch distance, plus entities there \
             that are not resident.",
            schema(vec![("near", vec3("A point (default: the window camera's eye)")), ("limit", t("integer", "Cells and entities listed, 1..64 (default 16)"))], &[]),
        ),
        def(
            "stream_settings",
            "Read or change streaming (perf.toml [streaming] sets the start values, [streaming.integrated] etc. per adapter). \
             enabled (off: everything uploads at once, no HLOD), cell_m (4..1024), radius_m (members load within this of a \
             camera), upload_ms / upload_mb (per-frame upload budget), memory_mb (GPU memory for models and proxies; over it the \
             least recently wanted go), hlod (merged proxies for far cells), hlod_min_m, hlod_radius_m, preload (1..4: members \
             load at this times a proxied cell's switch distance), settle_s (a changed cell's proxy is rebuilt after this quiet \
             time), capture_wait (captures wait for what their view needs). Changing needs agent_hello role \"lead\" (or \
             \"director\"); anyone may read.",
            schema(
                vec![
                    ("enabled", t("boolean", "Streaming on")),
                    ("cell_m", t("number", "Cell size, metres")),
                    ("radius_m", t("number", "Members load within this of a camera")),
                    ("upload_ms", t("number", "CPU ms per frame for uploads")),
                    ("upload_mb", t("number", "MB per frame for uploads")),
                    ("memory_mb", t("number", "GPU memory budget for streamed models and proxies")),
                    ("hlod", t("boolean", "Merged proxies for far cells")),
                    ("hlod_min_m", t("number", "No proxy nearer than this")),
                    ("hlod_radius_m", t("number", "Proxies load within this of a camera")),
                    ("preload", t("number", "Members load at preload x a proxied cell's switch distance")),
                    ("settle_s", t("number", "Quiet seconds before a changed cell's proxy is rebuilt")),
                    ("capture_wait", t("boolean", "Captures wait for what their view needs to be resident")),
                ],
                &[],
            ),
        ),
        def(
            "stream_fly",
            "Fly the human's window camera along a path at speed_mps (real time; window only) and report what the frames \
             cost: frame time p50/p95/p99/max, CPU frame, streaming uploads per frame (ms, KB, deferred), frames over the frame \
             budget and how many of those uploaded something (traversal stutter), draws and HLOD cells, models not resident. \
             Answers when the flight ends (at most 120 s) or a human takes the controls. The camera looks look_ahead_m ahead \
             along the path, pitch_deg down (or at look_at), and stays where the flight ended.",
            schema(
                vec![
                    ("path", Json::obj([("type", Json::from("array")), ("items", vec3("A point")), ("description", Json::from("2..64 points, metres"))])),
                    ("speed_mps", t("number", "Metres per second (default 20)")),
                    ("look_at", vec3("Look at this point instead of ahead")),
                    ("look_ahead_m", t("number", "How far ahead along the path to look (default 40)")),
                    ("pitch_deg", t("number", "Degrees below the horizon (default 12)")),
                    ("fov_y_deg", t("number", "Field of view (default 60)")),
                ],
                &["path"],
            ),
        ),
    ]
}

/// One recorded frame of a flight.
#[derive(Clone, Copy, Debug, Default)]
struct FlyFrame {
    dt_ms: f64,
    cpu_ms: f64,
    upload_ms: f64,
    uploads: u32,
    kb: u64,
    deferred: u32,
    draws: u32,
    submitted: u32,
    not_resident: u32,
    hlod_cells: u32,
    evicted: u32,
    /// The frame's slowest upload: `(ms, KB, what)`.
    slowest: (f64, u64, &'static str),
}

/// A flight in progress.
pub struct Fly {
    spec: FlySpec,
    /// Path length up to each point.
    along: Vec<f32>,
    started: Option<Instant>,
    frames: Vec<FlyFrame>,
    deadline: Instant,
    budget_ms: f64,
    released: bool,
}

impl Fly {
    fn length(&self) -> f32 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// The point `s` metres along the path.
    fn at(&self, s: f32) -> Vec3 {
        let p = &self.spec.path;
        let s = s.clamp(0.0, self.length());
        let i = self.along.partition_point(|&a| a <= s).clamp(1, p.len() - 1);
        let (a, b) = (self.along[i - 1], self.along[i]);
        let t = if b > a { (s - a) / (b - a) } else { 0.0 };
        p[i - 1] + (p[i] - p[i - 1]) * t
    }

    fn camera(&self, s: f32) -> Camera {
        let eye = self.at(s);
        let target = match self.spec.look_at {
            Some(t) => t,
            None => {
                let ahead = self.at(s + self.spec.look_ahead_m);
                let mut dir = ahead - eye;
                if dir.length() < 1e-3 {
                    // At the end: keep looking the way the last leg went.
                    let n = self.spec.path.len();
                    dir = self.spec.path[n - 1] - self.spec.path[n - 2];
                }
                let flat = Vec3::new(dir.x, 0.0, dir.z);
                let flat = if flat.length() < 1e-3 { Vec3::new(0.0, 0.0, -1.0) } else { flat.normalize() };
                eye + flat * self.spec.look_ahead_m - Vec3::new(0.0, self.spec.pitch_deg.to_radians().tan() * self.spec.look_ahead_m, 0.0)
            }
        };
        Camera::new(eye, target, self.spec.fov_y_deg)
    }

    /// Answers the caller: the engine is stopping.
    pub fn abort(job: crate::jobs::Job<Fly>) {
        job.answer(ToolOutput::error("stream_fly: the engine is shutting down; the flight did not finish"));
    }

    fn summary(&self) -> Json {
        let f = &self.frames;
        let stat = |v: Vec<f64>| -> Json {
            let mut sorted = v.clone();
            sorted.sort_by(f64::total_cmp);
            let Some(s) = Stats::of(v.into_iter()) else { return Json::Null };
            let p99 = sorted[((0.99 * (sorted.len() - 1) as f64).round() as usize).min(sorted.len() - 1)];
            let r = |x: f64| Json::from((x * 1000.0).round() / 1000.0);
            Json::obj([("mean", r(s.mean)), ("p50", r(s.p50)), ("p95", r(s.p95)), ("p99", r(p99)), ("max", r(s.max))])
        };
        let over: Vec<&FlyFrame> = f.iter().filter(|x| x.dt_ms > self.budget_ms * 1.5).collect();
        let mut worst: Vec<&FlyFrame> = f.iter().collect();
        worst.sort_by(|a, b| b.dt_ms.total_cmp(&a.dt_ms));
        let r2 = |x: f64| Json::from((x * 100.0).round() / 100.0);
        let worst = worst.iter().take(5).map(|x| {
            Json::obj([("frame_ms", r2(x.dt_ms)), ("cpu_ms", r2(x.cpu_ms)), ("upload_ms", r2(x.upload_ms)), ("uploads", Json::from(x.uploads))])
        });
        let mut by_upload: Vec<&FlyFrame> = f.iter().filter(|x| x.uploads > 0).collect();
        by_upload.sort_by(|a, b| b.upload_ms.total_cmp(&a.upload_ms));
        let worst_uploads = by_upload.iter().take(5).map(|x| {
            Json::obj([
                ("frame_ms", r2(x.dt_ms)),
                ("upload_ms", r2(x.upload_ms)),
                ("uploads", Json::from(x.uploads)),
                ("kb", Json::from(x.kb)),
                ("evicted", Json::from(x.evicted)),
                ("slowest", Json::obj([("what", Json::from(x.slowest.2)), ("ms", r2(x.slowest.0)), ("kb", Json::from(x.slowest.1))])),
            ])
        });
        let uploading = f.iter().filter(|x| x.uploads > 0).count();
        Json::obj([
            ("frames", Json::from(f.len())),
            ("seconds", Json::from((f.iter().map(|x| x.dt_ms).sum::<f64>() / 10.0).round() / 100.0)),
            ("length_m", Json::from((self.length() * 10.0).round() / 10.0)),
            ("released_by", if self.released { Json::from("input") } else { Json::Null }),
            ("frame_budget_ms", Json::from((self.budget_ms * 100.0).round() / 100.0)),
            ("frame_ms", stat(f.iter().map(|x| x.dt_ms).collect())),
            ("cpu_ms", stat(f.iter().map(|x| x.cpu_ms).collect())),
            ("upload_ms", stat(f.iter().map(|x| x.upload_ms).collect())),
            ("frames_uploading", Json::from(uploading)),
            ("uploads", Json::from(f.iter().map(|x| u64::from(x.uploads)).sum::<u64>())),
            ("uploaded_mb", Json::from((f.iter().map(|x| x.kb).sum::<u64>() as f64 / 1024.0 * 100.0).round() / 100.0)),
            ("deferred_max", Json::from(f.iter().map(|x| x.deferred).max().unwrap_or(0))),
            // Frames 1.5x over the frame budget (missed vsync), and how many uploaded meanwhile.
            ("frames_over_budget", Json::from(over.len())),
            ("frames_over_budget_uploading", Json::from(over.iter().filter(|x| x.uploads > 0).count())),
            // The longest frames, with what streaming did in them.
            ("worst_frames", Json::arr(worst)),
            // The frames that spent longest uploading, with their slowest item.
            ("worst_upload_frames", Json::arr(worst_uploads)),
            ("draw_calls", stat(f.iter().map(|x| f64::from(x.draws)).collect())),
            ("submitted_draws", stat(f.iter().map(|x| f64::from(x.submitted)).collect())),
            ("hlod_cells", stat(f.iter().map(|x| f64::from(x.hlod_cells)).collect())),
            ("not_resident_max", Json::from(f.iter().map(|x| x.not_resident).max().unwrap_or(0))),
        ])
    }
}

impl Engine {
    /// stream_fly: starts a flight for `owner` (answered when it ends). One flies at a time:
    /// another agent's flight is refused while one flies (naming its owner); the owner's own
    /// new flight replaces its old one.
    pub(crate) fn start_fly(&mut self, spec: FlySpec, owner: crate::jobs::JobOwner, reply: crate::jobs::Reply, deadline: Instant) {
        if !self.windowed {
            reply.send(crate::world_error(&WorldError::new("no_window", "stream_fly flies the window's camera: run the engine with a window (no --headless)")));
            return;
        }
        let mut along = vec![0.0];
        for w in spec.path.windows(2) {
            along.push(along.last().copied().unwrap_or(0.0) + (w[1] - w[0]).length());
        }
        let kind = crate::perf::adapter_kind(self.gpu.adapter().kind);
        let budget_ms = self.perf_cfg.budgets(kind).frame_ms;
        let fly = || Fly { spec, along, started: None, frames: Vec::new(), deadline, budget_ms, released: false };
        let replaced = || ToolOutput::error("stream_fly: replaced by your newer flight");
        let _ = self.fly.start(owner, reply, || Ok(()), fly, replaced);
    }

    /// The flight's camera for this window frame (the window camera follows it); ends a flight
    /// that is done, past its caller's deadline, or taken over by a human.
    pub(crate) fn fly_camera(&mut self, human: bool) -> Option<Camera> {
        let fly = &mut self.fly.get_mut()?.state;
        let now = Instant::now();
        let started = *fly.started.get_or_insert(now);
        let s = now.duration_since(started).as_secs_f32() * fly.spec.speed_mps;
        let done = s >= fly.length() || human || fly.frames.len() >= MAX_FLY_FRAMES || now + Duration::from_secs(1) >= fly.deadline;
        let cam = fly.camera(s);
        if done {
            fly.released = human;
            let job = self.fly.take().expect("checked");
            let mut out = job.state.summary();
            out.set("streaming_now", self.stream.json(&self.gpu, &self.models));
            out.set("flown_by", Json::from(job.owner.name.as_str()));
            job.answer(ToolOutput::text(out.to_pretty()));
            self.window_follow = None;
            self.window_camera = Some(cam);
            return None;
        }
        Some(cam)
    }

    /// Records a presented frame of the flight.
    pub(crate) fn fly_frame(&mut self, dt: Duration, cpu: Duration) {
        let Some(fly) = self.fly.get_mut().map(|j| &mut j.state) else { return };
        if fly.started.is_none() {
            return;
        }
        let stats = self.gpu.last_frame_stats_of(specula_gpu::FramePath::Window);
        let f = &self.stream.frame;
        fly.frames.push(FlyFrame {
            dt_ms: dt.as_secs_f64() * 1e3,
            cpu_ms: cpu.as_secs_f64() * 1e3,
            upload_ms: f.ms,
            uploads: f.items,
            kb: f.bytes / 1024,
            deferred: f.deferred,
            draws: stats.as_ref().map_or(0, |s| s.draws.draw_calls),
            submitted: stats.as_ref().map_or(0, |s| s.draws.submitted),
            not_resident: self.lod_window.not_resident,
            hlod_cells: self.stream.hlod.stats.window_cells,
            evicted: f.evicted,
            slowest: f.slowest,
        });
    }

    /// stream_state and stream_settings.
    pub(crate) fn stream_call(&mut self, call: StreamCall, session: &SessionInfo) -> ToolOutput {
        match call {
            StreamCall::State { near, limit } => ToolOutput::text(self.stream.state_json(&self.gpu, &self.models, &self.world, near, limit).to_string()),
            StreamCall::Settings(patch) => {
                let changing = patch.as_object().is_some_and(|o| !o.is_empty());
                if changing {
                    let have = crate::authority::authority(&self.swarm, &session.id);
                    if let Err(e) = crate::authority::require(have, crate::authority::Authority::Lead, crate::authority::Site::StreamSettings, "changing streaming budgets (they shape the human's window too; anyone may read stream_settings)") {
                        return crate::world_error(&e);
                    }
                }
                if changing {
                    let by = self.swarm.agent_name(session);
                    if let Err(e) = self.stream.set(&patch, &by, &mut self.gpu, &mut self.models) {
                        return crate::world_error(&WorldError::new("invalid_value", e));
                    }
                }
                let source = self.perf_cfg.source.as_ref().map_or(Json::Null, |p| Json::from(p.display().to_string()));
                let mut j = self.stream.cfg.to_json();
                j.set("perf_toml", source);
                j.set("adapter", Json::from(crate::perf::adapter_kind(self.gpu.adapter().kind)));
                j.set("set_by", self.stream.set_by().map_or(Json::Null, Json::from));
                ToolOutput::text(j.to_pretty())
            }
            StreamCall::Fly(_) => unreachable!("started by handle"),
        }
    }

    /// One frame (or headless loop) of streaming: the window's camera as a source, uploads
    /// within the frame's budget.
    pub(crate) fn stream_tick(&mut self, window: Option<(Camera, u32, u32)>) {
        self.stream.set_window(window);
        self.stream.begin_frame();
        // 2D textures first (few and small; a sprite never waits behind a far cell's meshes).
        let allowance = self.stream.allowance();
        let used = self.two_d.pump(&mut self.gpu, allowance);
        if allowance.is_some() {
            self.stream.charge(used);
        }
        let generation = self.world_generation;
        self.stream.frame(&mut self.gpu, &mut self.models, &self.library, &self.world, generation, true);
    }

    /// Before a capture: its views become stream sources and their uploads start now.
    pub(crate) fn stream_capture(&mut self, views: &[crate::streaming::CaptureView]) -> usize {
        if !self.stream.enabled() {
            return 0;
        }
        for (c, w, h, lod) in views {
            self.stream.capture_source(*c, *w, *h, *lod);
        }
        let generation = self.world_generation;
        self.stream.frame(&mut self.gpu, &mut self.models, &self.library, &self.world, generation, false);
        self.stream.waiting_in(views, &self.world, &self.models)
    }
}
