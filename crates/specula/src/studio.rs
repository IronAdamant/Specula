//! The asset studio's stage (docs/design/ASSET_STUDIO.md): subjects built from the model library
//! and rendered offscreen with the stage's own backdrop, light rigs and scale references, never
//! the world. Sheets, line-ups, compares, the taste loop and `style_check` are answered here; the
//! rules and measures are `specula-studio`'s, the arguments `studio_tools`'.
//!
//! Every render is reproducible: fixed cameras, fixed light, the build's own LODs, time 0, and a
//! rigged model in its bind pose. Nothing here reads or writes the world.

use super::*;
use crate::response::Format;
use crate::studio_tools::{CompareFrom, Pass, RigName, SheetSpec, Shot, StudioCall, SubjectSpec, View, MAX_SHEET_PX, MAX_SUBJECTS};
use crate::world::{ModelSpec, Shape};
use specula_anim::film;
use specula_anim::math::Affine;
use specula_core::math::{Quat, Vec3, Vec4};
use specula_studio::facts::{AssetFacts, RecipeInfo};
use specula_studio::image::{self as img, Mask};
use specula_studio::rules::{neutral_rig, GameCamera, Rig, StyleRules};
use specula_studio::taste::{self, Measured, ShotMeasure};
use specula_studio::{critique, Bible, Finding, Report};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};

/// Marks one agent identity keeps (the least recently used goes first).
pub const MAX_MARKS_PER_AGENT: usize = 8;
/// Every agent's marks together (pixels, estimated).
pub const MAX_MARK_BYTES: usize = 192 * 1024 * 1024;
/// Largest answer text (a detailed style_check over many subjects stops listing there).
const MAX_ANSWER_BYTES: usize = 160 * 1024;
/// Recipes a sibling search opens at most.
const MAX_SCANNED: usize = 256;
/// Errors listed per subject in concise answers.
const CONCISE_ERRORS: usize = 8;
/// The figure's height and the cube's side (metres).
const FIGURE_M: f32 = 1.8;
const CUBE_M: f32 = 1.0;
/// Field of view of fitted (non-game) cameras: long enough to keep perspective mild.
const FIT_FOV: f32 = 30.0;
/// The three thumbnail silhouette angles (front, side, three-quarter), (az, el).
const THUMB_ANGLES: [(f32, f32); 3] = [(0.0, 8.0), (90.0, 8.0), (35.0, 20.0)];
/// A pixel counts as changed in a compare when a channel moved more than this.
const CHANGED: u8 = 12;

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn r3(x: f32) -> Json {
    Json::from((x as f64 * 1000.0).round() / 1000.0)
}

fn spec_json(s: &ModelSpec) -> Json {
    let mut j = Json::obj([("recipe", Json::from(s.recipe.as_str()))]);
    if !s.params.is_empty() {
        j.set("params", Json::obj(s.params.iter().map(|(k, v)| (k.clone(), Json::from(*v)))));
    }
    if let Some(seed) = s.seed {
        j.set("seed", Json::from(seed));
    }
    j
}

// ---------------------------------------------------------------------------------------------
// Marks

/// A tile's record in a sheet's packet.
#[derive(Clone, Debug)]
struct TileRec {
    row: usize,
    subject: String,
    view: String,
    pass: Pass,
    rect: [u32; 4],
    /// The subject's box inside the tile (tile pixels).
    bbox: Option<[u32; 4]>,
    /// Mean lightness inside the box (or the whole tile).
    lightness: f32,
    lod: usize,
    /// Shots: the measures in the shot's own viewport, and how much the tile magnifies it.
    shot: Option<(ShotMeasure, f32)>,
}

impl TileRec {
    fn to_json(&self) -> Json {
        let mut j = Json::obj([
            ("subject", Json::from(self.subject.as_str())),
            ("view", Json::from(self.view.as_str())),
            ("pass", Json::from(self.pass.name())),
            ("tile", Json::arr(self.rect.map(Json::from))),
            ("lightness", r3(self.lightness)),
            ("lod", Json::from(self.lod)),
        ]);
        if let Some(b) = self.bbox {
            j.set("box", Json::arr(b.map(Json::from)));
        }
        if let Some((m, mag)) = &self.shot {
            j.set("shot", m.to_json());
            j.set("magnified", r3(*mag));
        }
        j
    }
}

/// A sheet as rendered: pixels, tiles, and each subject's facts and rule findings.
#[derive(Clone)]
struct SheetOut {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    tiles: Vec<TileRec>,
    subjects: Vec<(String, Json, Report)>,
}

struct Mark {
    sheet: SheetSpec,
    out: SheetOut,
    used: u64,
}

/// `studio_sheet {mark}`: sheets kept per MCP session for `studio_compare`.
#[derive(Default)]
pub struct Marks {
    by: BTreeMap<(String, String), Mark>,
    clock: u64,
}

impl Marks {
    fn bytes(&self) -> usize {
        self.by.values().map(|m| m.out.rgba.len()).sum()
    }

    fn put(&mut self, agent: &str, name: &str, sheet: SheetSpec, out: SheetOut) {
        self.clock += 1;
        self.by.insert((agent.to_string(), name.to_string()), Mark { sheet, out, used: self.clock });
        loop {
            let mine: Vec<(&(String, String), u64)> = self.by.iter().filter(|(k, _)| k.0 == agent).map(|(k, m)| (k, m.used)).collect();
            let victim = if mine.len() > MAX_MARKS_PER_AGENT {
                mine.iter().min_by_key(|(_, u)| *u).map(|(k, _)| (*k).clone())
            } else if self.bytes() > MAX_MARK_BYTES && self.by.len() > 1 {
                self.by.iter().min_by_key(|(_, m)| m.used).map(|(k, _)| k.clone())
            } else {
                None
            };
            match victim {
                Some(k) => {
                    self.by.remove(&k);
                }
                None => break,
            }
        }
    }

    fn get(&mut self, agent: &str, name: &str) -> Option<(SheetSpec, SheetOut)> {
        self.clock += 1;
        let c = self.clock;
        let m = self.by.get_mut(&(agent.to_string(), name.to_string()))?;
        m.used = c;
        Some((m.sheet.clone(), m.out.clone()))
    }

    /// A mark's sheet, without counting it as used.
    fn spec(&self, agent: &str, name: &str) -> Option<SheetSpec> {
        self.by.get(&(agent.to_string(), name.to_string())).map(|m| m.sheet.clone())
    }

    fn names(&self, agent: &str) -> Vec<String> {
        self.by.keys().filter(|k| k.0 == agent).map(|k| k.1.clone()).collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Calls on the caller's thread, renders on the engine's

/// Renders one studio call may make (each shot and each silhouette is one).
pub const MAX_RENDERS: usize = 128;
/// Studio calls running at once: per agent, and every agent's together.
pub const MAX_CALLS_PER_AGENT: usize = 2;
pub const MAX_CALLS: usize = 16;
/// The longest a studio call may take; it is refused up front when the queue says it can't.
pub const CALL_LIMIT: Duration = Duration::from_secs(40);
/// A call is admitted only while the queue's estimated time stays under this share of
/// [`CALL_LIMIT`] (render times vary; round-robin finishes the running calls together).
const ADMIT_SHARE: f64 = 0.6;
/// The studio's own offscreen targets: at most this many and this many bytes (estimated), each
/// freed after [`TARGET_IDLE`] without use.
const MAX_STUDIO_TARGETS: usize = 16;
const MAX_TARGET_BYTES: u64 = 96 * 1024 * 1024;
const TARGET_IDLE: Duration = Duration::from_secs(10);
/// A call's builds stay pinned on the GPU at most this long after its last render (a call that
/// never says it is done, say).
const PIN_IDLE: Duration = Duration::from_secs(60);
/// How often idle targets and stale pins are pruned.
const TIDY_EVERY: Duration = Duration::from_secs(1);
/// How long an ended call's builds stay pinned.
const PIN_GRACE: Duration = Duration::from_secs(10);
/// The share of the engine thread's time studio renders may take: in the window (about 4 ms of
/// a 60 Hz frame, on average) and headless (agents' other calls are read between renders).
pub const WINDOW_SHARE: f64 = 0.25;
pub const HEADLESS_SHARE: f64 = 0.8;
/// Most pixels one render may have: one render can't be split between frames, and a 2048-wide
/// line-up frame took 50-110 ms on the Intel iGPU. Bigger images (stage frames, line-ups) render
/// at a lower resolution through the same camera and are scaled up on the caller's thread
/// (`render_scale` in their packets).
const MAX_RENDER_PIXELS: u32 = 640 * 640;
/// Most render credit saved up while idle (ms): a burst can't take more than this at once.
const MAX_CREDIT_MS: f64 = 8.0;

/// One subject in a render request, owning what it needs.
pub(crate) struct PlacedOwned {
    built: Arc<models::Built>,
    skin: Vec<Affine>,
    at: Vec3,
    yaw: f32,
    lod: usize,
    radius: f32,
}

/// One stage render: everything the engine needs to draw it.
pub(crate) struct RenderReq {
    subjects: Vec<PlacedOwned>,
    refs: Option<Vec3>,
    cam: Cam,
    /// The pixels rendered (at most [`MAX_RENDER_PIXELS`]) and the image's aspect (width over
    /// height), which may differ slightly when a big frame renders smaller.
    w: u32,
    h: u32,
    aspect: f32,
    /// `None`: a silhouette.
    light: Option<(Rig, bool)>,
}

/// A render on its way to the engine, and where its pixels go.
pub struct RenderJob {
    call: u64,
    session: String,
    req: RenderReq,
    reply: crate::jobs::Reply<Result<Rendered, ToolOutput>>,
    deadline: Instant,
    /// The agent call it renders for (docs/design/TRACE.md).
    trace: specula_core::trace::Ctx,
}

/// A render's pixels, and what it cost the engine thread: wall time, and the GPU's own time
/// for it (timestamps), which tells the studio's work apart from waiting on a GPU that other
/// work (other processes too) keeps busy.
pub(crate) struct Rendered {
    px: Vec<u8>,
    cost: RenderCost,
}

/// What one render cost (or, summed up for a call, the most any of its renders cost).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RenderCost {
    /// Engine-thread wall time.
    ms: f64,
    /// The GPU's own time (timestamps).
    gpu_ms: f64,
    /// Of `ms`: preparing and recording the frame on the CPU (`CpuTiming::record_ms`), the
    /// queue submit, and waiting for its fence.
    record_ms: f64,
    submit_ms: f64,
    wait_ms: f64,
}

impl RenderCost {
    fn max(self, o: RenderCost) -> RenderCost {
        RenderCost { ms: self.ms.max(o.ms), gpu_ms: self.gpu_ms.max(o.gpu_ms), record_ms: self.record_ms.max(o.record_ms), submit_ms: self.submit_ms.max(o.submit_ms), wait_ms: self.wait_ms.max(o.wait_ms) }
    }
}

/// What studio calls (on their callers' connection threads) share with the engine.
pub struct Shared {
    library: Arc<SharedLibrary>,
    capture_dir: PathBuf,
    frames: AtomicU64,
    marks: Mutex<Marks>,
    calls: Mutex<Calls>,
}

/// The calls running, for admission.
#[derive(Default)]
struct Calls {
    next: u64,
    /// call -> (session, renders it still plans).
    running: BTreeMap<u64, (String, usize)>,
    /// Recent mean render time (ms) and the engine's share, for estimates.
    render_ms: f64,
    share: f64,
}

impl Calls {
    /// See [`Shared::admit`].
    fn admit(&mut self, session: &str, renders: usize) -> Result<u64, ToolOutput> {
        let c = self;
        let ms = c.render_ms.max(2.0);
        let share = c.share.clamp(0.05, 1.0);
        let ahead: usize = c.running.values().map(|r| r.1).sum();
        let wait_s = |n: usize| ((n as f64 * ms / share) / 1000.0).ceil().max(1.0);
        let busy = |why: String, retry: f64| {
            ToolOutput::error(Json::obj([("error", Json::from("studio_busy")), ("message", Json::from(why)), ("retry_after_s", Json::from(retry))]).to_string())
        };
        let mine: Vec<usize> = c.running.values().filter(|r| r.0 == session).map(|r| r.1).collect();
        if mine.len() >= MAX_CALLS_PER_AGENT {
            return Err(busy(format!("you already have {} studio calls running (at most {MAX_CALLS_PER_AGENT} per agent): wait for one to answer", mine.len()), wait_s(mine.iter().sum())));
        }
        if c.running.len() >= MAX_CALLS {
            return Err(busy(format!("{MAX_CALLS} studio calls are running (every agent's); call again shortly"), wait_s(ahead / c.running.len().max(1))));
        }
        // Renders are served round-robin, so the calls running finish together near the end of
        // the queue: admit only while the whole queue fits well inside the limit.
        let est_ms = (ahead + renders) as f64 * ms / share;
        if renders > 0 && ahead > 0 && est_ms > ADMIT_SHARE * CALL_LIMIT.as_secs_f64() * 1000.0 {
            return Err(busy(
                format!("the studio queue holds about {ahead} renders ({:.0} s at the engine's share); this call's {renders} would not finish within {} s", ahead as f64 * ms / share / 1000.0, CALL_LIMIT.as_secs()),
                wait_s(ahead),
            ));
        }
        c.next += 1;
        let id = c.next;
        c.running.insert(id, (session.to_string(), renders));
        Ok(id)
    }
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    pub(crate) fn new(library: Arc<SharedLibrary>, capture_dir: PathBuf) -> Shared {
        Shared { library, capture_dir, frames: AtomicU64::new(0), marks: Mutex::default(), calls: Mutex::new(Calls { render_ms: 20.0, share: HEADLESS_SHARE, ..Calls::default() }) }
    }

    fn marks(&self) -> std::sync::MutexGuard<'_, Marks> {
        locked(&self.marks)
    }

    /// Admits a call by `session` that plans `renders` renders: its id, or `studio_busy` with
    /// `retry_after_s` when the agent already runs [`MAX_CALLS_PER_AGENT`] calls, every agent's
    /// calls fill [`MAX_CALLS`], or the renders queued ahead of it can't finish within
    /// [`CALL_LIMIT`] at the engine's share. Checked before any work.
    fn admit(&self, session: &str, renders: usize) -> Result<u64, ToolOutput> {
        locked(&self.calls).admit(session, renders)
    }

    fn rendered(&self, call: u64) {
        if let Some(r) = locked(&self.calls).running.get_mut(&call) {
            r.1 = r.1.saturating_sub(1);
        }
    }

    fn finish(&self, call: u64) {
        locked(&self.calls).running.remove(&call);
    }

    /// The engine notes each render's time and its share (for estimates).
    fn note(&self, render_ms: Option<f64>, share: f64) {
        let mut c = locked(&self.calls);
        if let Some(ms) = render_ms {
            c.render_ms = c.render_ms * 0.9 + ms * 0.1;
        }
        c.share = share;
    }

    fn next_frame(&self) -> u64 {
        self.frames.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// A studio target and its estimated bytes.
struct Target {
    key: (u32, u32, u8),
    id: TargetId,
    bytes: u64,
    last: Instant,
}

/// A call's pinned builds (see [`GpuModels::set_pinned`]).
struct Pin {
    session: String,
    builds: BTreeSet<u64>,
    last: Instant,
    /// The call ended: its builds stay pinned for [`PIN_GRACE`] (an agent's next call in a taste
    /// loop renders the same builds without uploading them again).
    done: bool,
}

/// The engine's side of the studio: renders queued per session, served round-robin within the
/// render budget, the studio's targets and the pins.
pub struct Studio {
    pub(crate) shared: Arc<Shared>,
    queue: BTreeMap<String, VecDeque<Box<RenderJob>>>,
    last_session: Option<String>,
    targets: Vec<Target>,
    pins: BTreeMap<u64, Pin>,
    /// The render budget (ms): earned at a share of wall time, spent by renders.
    credit_ms: f64,
    last_pump: Option<Instant>,
    /// When idle targets and stale pins were last pruned (about once a second, busy or not).
    last_tidy: Option<Instant>,
    /// Queued `vfx_preview`s, rendered a frame per step within the same budget.
    pub(crate) previews: VecDeque<Box<crate::vfx_tools::PreviewJob>>,
    /// Whether a preview's step goes before the next studio render (they take turns).
    preview_turn: bool,
    /// Finished previews whose sheets are being encoded on their own threads.
    pub(crate) encoders: Arc<std::sync::atomic::AtomicUsize>,
}

impl Studio {
    pub(crate) fn new(library: Arc<SharedLibrary>, capture_dir: PathBuf) -> Studio {
        Studio { shared: Arc::new(Shared::new(library, capture_dir)), queue: BTreeMap::new(), last_session: None, targets: Vec::new(), pins: BTreeMap::new(), credit_ms: MAX_CREDIT_MS, last_pump: None, last_tidy: None, previews: VecDeque::new(), preview_turn: false, encoders: Arc::default() }
    }

    /// The next render, round-robin over the sessions with renders queued (in key order after
    /// the last one served).
    fn next(&mut self) -> Option<Box<RenderJob>> {
        rr_next(&mut self.queue, &mut self.last_session)
    }

    fn put_back(&mut self, job: Box<RenderJob>) {
        self.queue.entry(job.session.clone()).or_default().push_front(job);
    }
}

fn r1(v: f64) -> Json {
    Json::from((v * 10.0).round() / 10.0)
}

/// The next item of `queue`, round-robin over its keys: the first key after `last` (wrapping),
/// its oldest item; emptied keys are removed.
fn rr_next<T>(queue: &mut BTreeMap<String, VecDeque<T>>, last: &mut Option<String>) -> Option<T> {
    let key = match last.as_ref() {
        Some(l) => queue.range::<String, _>((std::ops::Bound::Excluded(l.clone()), std::ops::Bound::Unbounded)).next().or_else(|| queue.iter().next()).map(|(k, _)| k.clone()),
        None => queue.keys().next().cloned(),
    }?;
    let q = queue.get_mut(&key)?;
    let item = q.pop_front();
    if q.is_empty() {
        queue.remove(&key);
    }
    *last = Some(key);
    item
}

/// Shot crops' sides: a few classes, so their offscreen targets are reused.
const CROP_CLASSES: [u32; 8] = [32, 64, 96, 128, 192, 256, 384, 512];

/// The crop class for `px` pixels (the largest class beyond the last).
fn crop_class(px: u32) -> u32 {
    CROP_CLASSES.iter().copied().find(|c| *c >= px).unwrap_or(CROP_CLASSES[CROP_CLASSES.len() - 1])
}

// ---------------------------------------------------------------------------------------------
// Subjects, cameras and light

/// A subject ready for the stage.
struct Staged {
    spec: ModelSpec,
    name: String,
    built: Arc<models::Built>,
    facts: AssetFacts,
    bible: Rc<Bible>,
    lo: Vec3,
    hi: Vec3,
    /// Bind-pose skinning matrices (identity per bone) for rigged models.
    skin: Vec<Affine>,
}

impl Staged {
    fn centre(&self) -> Vec3 {
        (self.lo + self.hi) * 0.5
    }

    fn radius(&self) -> f32 {
        ((self.hi - self.lo).length() * 0.5).max(0.05)
    }

    fn lods(&self) -> usize {
        self.built.model.lods.len().max(1)
    }

    fn rules(&self) -> &StyleRules {
        &self.bible.rules
    }
}

/// A subject placed on the stage at an offset, drawn at a level of detail.
#[derive(Clone, Copy)]
struct Placed<'a> {
    s: &'a Staged,
    at: Vec3,
    lod: usize,
    /// Turned about +Y (degrees): angle views turn the subject, not the camera, so the scale
    /// references always stand at the image's left (the light follows the camera either way).
    yaw: f32,
}

/// The box of `s` turned by `yaw` degrees about +Y and moved by `at`.
fn turned_box(s: &Staged, yaw: f32, at: Vec3) -> (Vec3, Vec3) {
    let q = Quat::from_axis_angle(Vec3::Y, yaw.to_radians());
    let (mut lo, mut hi) = (Vec3::new(f32::MAX, f32::MAX, f32::MAX), Vec3::new(f32::MIN, f32::MIN, f32::MIN));
    for c in corners(s.lo, s.hi) {
        let p = q.rotate(c) + at;
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cam {
    eye: Vec3,
    target: Vec3,
    fov: f32,
}

impl Cam {
    fn view_proj(&self, aspect: f32) -> Mat4 {
        let up = if (self.eye - self.target).normalize().y.abs() > 0.995 { Vec3::new(0.0, 0.0, -1.0) } else { Vec3::Y };
        Mat4::perspective_vk_reverse_z(self.fov.to_radians(), aspect, 0.05) * Mat4::look_at_rh(self.eye, self.target, up)
    }

    fn yaw(&self) -> f32 {
        let d = self.eye - self.target;
        d.x.atan2(d.z).to_degrees()
    }

    fn to_json(self) -> Json {
        let v = |p: Vec3| Json::arr([r3(p.x), r3(p.y), r3(p.z)]);
        Json::obj([("eye", v(self.eye)), ("target", v(self.target)), ("fov_y_deg", r3(self.fov))])
    }
}

fn dir_of(az: f32, el: f32) -> Vec3 {
    let (a, e) = (az.to_radians(), el.to_radians());
    Vec3::new(e.cos() * a.sin(), e.sin(), e.cos() * a.cos())
}

fn corners(lo: Vec3, hi: Vec3) -> [Vec3; 8] {
    let mut c = [Vec3::ZERO; 8];
    for (i, p) in c.iter_mut().enumerate() {
        *p = Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
    }
    c
}

/// A camera from `az`/`el` that fits the box `lo`..`hi` in a view of `aspect` (width/height).
fn fit(lo: Vec3, hi: Vec3, az: f32, el: f32, fov: f32, aspect: f32, margin: f32) -> Cam {
    let centre = (lo + hi) * 0.5;
    let dir = dir_of(az, el);
    let fwd = -dir;
    let right = if fwd.cross(Vec3::Y).length() < 1e-3 { Vec3::X } else { fwd.cross(Vec3::Y).normalize() };
    let up = right.cross(fwd).normalize();
    let (mut ex, mut ey, mut ez) = (0.0f32, 0.0f32, 0.0f32);
    for c in corners(lo, hi) {
        let d = c - centre;
        ex = ex.max(d.dot(right).abs());
        ey = ey.max(d.dot(up).abs());
        ez = ez.max(d.dot(dir));
    }
    let tv = (fov.to_radians() * 0.5).tan();
    let th = tv * aspect.max(0.05);
    let dist = (ex / th).max(ey / tv).max(0.05) * margin + ez;
    Cam { eye: centre + dir * dist, target: centre, fov }
}

/// The game camera on a subject (`centre`, bounding `radius`, `lods` levels), cropped to the
/// pixels around it at the shot's viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GameShot {
    cam: Cam,
    /// The render's side in pixels.
    crop: u32,
    /// Viewport pixels per render pixel: 1 for a crop at true size; more when the subject is at
    /// least as big as the viewport and the render shows all of it, scaled down.
    scale: f32,
    lod: usize,
}

fn game_cam(centre: Vec3, radius: f32, lods: usize, c: &GameCamera, viewport_h: u32) -> GameShot {
    let eye = centre + dir_of(c.yaw_deg, c.pitch_deg) * c.distance_m;
    let vh = viewport_h.max(1);
    // Infinite when the camera is inside the subject's bounds: bounded before any integer maths.
    let px = models::screen_px(radius, c.distance_m, c.fov_deg, vh).min(vh as f32 * 8.0);
    let lod = models::select_lod(px, lods, None);
    let want = ((px * 1.3).ceil() as u32).saturating_add(10);
    if want >= vh {
        // At least the whole screen: render the whole viewport (at most 512 pixels tall).
        let crop = vh.min(CROP_CLASSES[CROP_CLASSES.len() - 1]);
        return GameShot { cam: Cam { eye, target: centre, fov: c.fov_deg }, crop, scale: vh as f32 / crop as f32, lod };
    }
    // A crop class (a few sizes, so a sheet's shots reuse offscreen targets); it maps 1:1 to
    // viewport pixels.
    let crop = crop_class(want).min(vh);
    let fov = 2.0 * ((c.fov_deg.to_radians() * 0.5).tan() * crop as f32 / vh as f32).atan().to_degrees();
    GameShot { cam: Cam { eye, target: centre, fov }, crop, scale: 1.0, lod }
}

/// The box `lo`..`hi` on screen, clipped to `w`×`h` (`None` when off it or behind the camera).
fn project_box(vp: &Mat4, lo: Vec3, hi: Vec3, w: u32, h: u32) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for c in corners(lo, hi) {
        let p = vp.mul_vec4(Vec4::new(c.x, c.y, c.z, 1.0));
        if p.w <= 1e-6 {
            return None;
        }
        let (x, y) = ((p.x / p.w + 1.0) * 0.5 * w as f32, (p.y / p.w + 1.0) * 0.5 * h as f32);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (cx0, cy0, cx1, cy1) = (x0.max(0.0), y0.max(0.0), x1.min(w as f32), y1.min(h as f32));
    (cx0 < cx1 && cy0 < cy1).then(|| [cx0 as u32, cy0 as u32, ((cx1 - cx0).ceil() as u32).max(1), ((cy1 - cy0).ceil() as u32).max(1)])
}

/// How a frame is lit.
#[derive(Clone, Copy)]
enum Light<'a> {
    /// A rig; `true` at the iGPU shot's settings (lower shadows, no bloom or FXAA).
    Rig(&'a Rig, bool),
    /// Black on white, no light: masks.
    Silhouette,
}

/// The rig's light. Its shadow map has one shape for every studio render, the world's own when
/// the world has shadows on (see [`studio_shadow_map`]).
fn environment(rig: &Rig, cam: &Cam, igpu: bool, reach: f32, (shadow_res, cascades): (u32, u32)) -> specula_gpu::Environment {
    let (az, el) = ((cam.yaw() + rig.key_azimuth_deg).to_radians(), rig.key_elevation_deg.to_radians());
    specula_gpu::Environment {
        sun_direction: [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()],
        sun_color: rig.key_color,
        sun_intensity: rig.key_intensity,
        ambient_color: rig.ambient_color,
        ambient_intensity: rig.ambient_intensity,
        exposure: rig.exposure,
        tonemap: true,
        shadow: specula_gpu::ShadowSettings { enabled: rig.shadows, resolution: shadow_res, cascades, distance: reach, softness: 1.5, bias: 1.0 },
        bloom: specula_gpu::BloomSettings { enabled: rig.bloom && !igpu, threshold: 1.0, intensity: 0.6, ..Default::default() },
        fxaa: !igpu,
        ..specula_gpu::Environment::default()
    }
}

/// The studio's shadow map shape (resolution, cascades). The map is one image shared by every
/// render graph of that shape, and the frame descriptors point at one map: a render with a map
/// of another shape than the last frame's rebinds it, which waits for the GPU to go idle (and a
/// new shape allocates a new map). So every studio render uses one shape: the world's, when its
/// shadows are on (the window and the studio then share the map), else [`STUDIO_SHADOW_RES`]
/// with one cascade.
fn studio_shadow_map(world: &specula_gpu::ShadowSettings) -> (u32, u32) {
    if world.enabled {
        (world.resolution, world.cascades.max(1))
    } else {
        (STUDIO_SHADOW_RES, 1)
    }
}

/// The studio's shadow map side when the world has no shadows.
const STUDIO_SHADOW_RES: u32 = 1024;

/// The scale references' parts, standing at `at` (the figure's feet): (centre, size, sphere).
fn reference_parts(at: Vec3) -> Vec<(Vec3, Vec3, bool, [f32; 3])> {
    let fig = [0.62, 0.62, 0.6];
    let cube = [0.78, 0.78, 0.76];
    let p = |x: f32, y: f32, z: f32| at + Vec3::new(x, y, z);
    vec![
        (p(-0.1, 0.43, 0.0), Vec3::new(0.15, 0.86, 0.17), false, fig),
        (p(0.1, 0.43, 0.0), Vec3::new(0.15, 0.86, 0.17), false, fig),
        (p(0.0, 1.16, 0.0), Vec3::new(0.42, 0.62, 0.24), false, fig),
        (p(-0.28, 1.12, 0.0), Vec3::new(0.1, 0.62, 0.12), false, fig),
        (p(0.28, 1.12, 0.0), Vec3::new(0.1, 0.62, 0.12), false, fig),
        (p(0.0, 1.51, 0.0), Vec3::new(0.1, 0.06, 0.1), false, fig),
        (p(0.0, 1.67, 0.0), Vec3::new(0.22, 0.26, 0.24), true, fig),
        (p(-0.85, CUBE_M * 0.5, 0.0), Vec3::new(CUBE_M, CUBE_M, CUBE_M), false, cube),
    ]
}

/// The references' box when the figure stands at `at`.
fn reference_box(at: Vec3) -> (Vec3, Vec3) {
    (at + Vec3::new(-0.85 - CUBE_M * 0.5, 0.0, -0.5), at + Vec3::new(0.35, FIGURE_M, 0.5))
}

/// Subjects side by side along +X (each centred in depth), with the references at the left:
/// placements and the whole box.
fn arrange<'a>(subjects: &'a [Staged], refs: bool, lod: usize) -> (Vec<Placed<'a>>, Option<Vec3>, Vec3, Vec3) {
    let widest = subjects.iter().map(|s| s.hi.x - s.lo.x).fold(0.0f32, f32::max);
    let gap = (widest * 0.35).clamp(0.3, 3.0);
    let mut x = 0.0f32;
    let (mut lo, mut hi) = (Vec3::new(f32::MAX, 0.0, f32::MAX), Vec3::new(f32::MIN, 0.0, f32::MIN));
    let mut fig = None;
    if refs {
        let at = Vec3::new(0.85 + CUBE_M * 0.5, 0.0, 0.0);
        let (a, b) = reference_box(at);
        lo = lo.min(a);
        hi = hi.max(b);
        x = b.x + gap;
        fig = Some(at);
    }
    let mut placed = Vec::new();
    for s in subjects {
        let at = Vec3::new(x - s.lo.x, 0.0, -(s.lo.z + s.hi.z) * 0.5);
        lo = lo.min(s.lo + at);
        hi = hi.max(s.hi + at);
        x += s.hi.x - s.lo.x + gap;
        placed.push(Placed { s, at, lod: lod.min(s.lods() - 1), yaw: 0.0 });
    }
    (placed, fig, lo, hi)
}

/// The size a `w`×`h` frame renders at: itself, or scaled down (keeping its shape, in whole 16s)
/// to at most [`MAX_RENDER_PIXELS`].
fn render_size(w: u32, h: u32) -> (u32, u32) {
    let px = u64::from(w) * u64::from(h);
    if px <= u64::from(MAX_RENDER_PIXELS) {
        return (w, h);
    }
    let s = (f64::from(MAX_RENDER_PIXELS) / px as f64).sqrt();
    let snap = |v: u32| (((f64::from(v) * s) as u32) / 16 * 16).max(16);
    (snap(w), snap(h))
}

/// `src` (`sw`×`sh`) scaled up to `dw`×`dh` with bilinear filtering.
fn upscale(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    let at = |x: u32, y: u32, c: usize| f32::from(src[((y.min(sh - 1) * sw + x.min(sw - 1)) * 4) as usize + c]);
    for y in 0..dh {
        let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).max(0.0);
        let (y0, ty) = (fy as u32, fy.fract());
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).max(0.0);
            let (x0, tx) = (fx as u32, fx.fract());
            for c in 0..4 {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x0 + 1, y0, c) * tx;
                let bottom = at(x0, y0 + 1, c) * (1.0 - tx) + at(x0 + 1, y0 + 1, c) * tx;
                out[((y * dw + x) * 4) as usize + c] = (top * (1.0 - ty) + bottom * ty).round() as u8;
            }
        }
    }
    out
}

/// `src` (`sw`×`sh`) scaled to `dw`×`dh` by nearest neighbour, into `sheet` at (`x0`, `y0`).
#[allow(clippy::too_many_arguments)]
fn blit_scaled(sheet: &mut [u8], shw: u32, x0: u32, y0: u32, src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) {
    for y in 0..dh {
        let sy = (y as u64 * sh as u64 / dh.max(1) as u64) as u32;
        for x in 0..dw {
            let sx = (x as u64 * sw as u64 / dw.max(1) as u64) as u32;
            let s = ((sy * sw + sx) * 4) as usize;
            let d = (((y0 + y) * shw + x0 + x) * 4) as usize;
            if s + 4 <= src.len() && d + 4 <= sheet.len() {
                sheet[d..d + 4].copy_from_slice(&src[s..s + 4]);
            }
        }
    }
}

fn mask_rgba(m: &Mask) -> Vec<u8> {
    m.on.iter().flat_map(|on| if *on { [16, 16, 16, 255] } else { [250, 250, 250, 255] }).collect()
}

/// What one sheet view renders for its passes: colour and silhouette pixels, the subject's box,
/// a shot's measures and magnification, the LOD drawn, and the pixels' size.
type ViewPixels = (Option<Vec<u8>>, Option<Vec<u8>>, Option<[u32; 4]>, Option<(ShotMeasure, f32)>, usize, u32, u32);
/// A taste-loop sheet cell: pixels, their size, and a shot's measures.
type Cell = (Vec<u8>, u32, u32, Option<ShotMeasure>);
/// A bible's files: `style.json` and the `style.toml` found for it.
type BibleKey = (PathBuf, Option<PathBuf>);

/// A shot or angle render of one subject: colour pixels, its mask and measures.
struct Render {
    w: u32,
    h: u32,
    colour: Vec<u8>,
    mask: Mask,
    measure: ShotMeasure,
    lod: usize,
    cam: Cam,
}

impl Engine {
    /// Renders one recorded request on the GPU (the studio's own targets).
    fn render_req(&mut self, req: &RenderReq) -> Result<Vec<u8>, ToolOutput> {
        let (w, h, cam) = (req.w, req.h, &req.cam);
        let vp = cam.view_proj(req.aspect);
        let mut draws: Vec<DrawItem> = Vec::new();
        let mut bones: Vec<[f32; 16]> = Vec::new();
        let reach = (cam.eye - cam.target).length() * 2.0 + req.subjects.iter().map(|p| p.radius).fold(2.0, f32::max) * 4.0;
        let (environment, clear) = match &req.light {
            Some((rig, igpu)) => {
                let igpu = *igpu;
                let size = reach.max(60.0) * 3.0;
                if let Some(q) = self.meshes.get(Shape::Quad) {
                    let m = Mat4::from_trs(Vec3::new(cam.target.x, 0.0, cam.target.z), Quat::IDENTITY, Vec3::new(size, 1.0, size));
                    let mut d = DrawItem::new(q, m.to_cols_array(), [rig.ground[0], rig.ground[1], rig.ground[2], 1.0]);
                    d.surface = specula_gpu::Surface { roughness: 0.9, specular: 0.05, ..specula_gpu::Surface::default() };
                    draws.push(d);
                }
                if let Some(at) = req.refs {
                    for (c, s, sphere, col) in reference_parts(at) {
                        let Some(mesh) = self.meshes.get(if sphere { Shape::Sphere } else { Shape::Cube }) else { continue };
                        let mut d = DrawItem::new(mesh, Mat4::from_trs(c, Quat::IDENTITY, s).to_cols_array(), [col[0], col[1], col[2], 1.0]);
                        d.surface = specula_gpu::Surface { roughness: 0.8, specular: 0.15, ..specula_gpu::Surface::default() };
                        draws.push(d);
                    }
                }
                (environment(rig, cam, igpu, reach, studio_shadow_map(&self.world.env().light.shadow)), [rig.background[0], rig.background[1], rig.background[2], 1.0])
            }
            None => (
                specula_gpu::Environment { sun_intensity: 0.0, ambient_intensity: 0.0, tonemap: false, ..specula_gpu::Environment::default() },
                [1.0, 1.0, 1.0, 1.0],
            ),
        };
        let first_subject = draws.len();
        for p in &req.subjects {
            let m = Mat4::from_trs(p.at, Quat::from_axis_angle(Vec3::Y, p.yaw.to_radians()), Vec3::ONE);
            if let Err(e) = self.models.posed_draws_lod(&mut self.gpu, &p.built, &p.skin, &m, p.lod, &mut bones, &mut draws) {
                return Err(error("render_unavailable", e));
            }
        }
        if req.light.is_none() {
            for d in &mut draws[first_subject..] {
                d.material = specula_gpu::Material::Lit;
                d.color = [0.0, 0.0, 0.0, 1.0];
                d.surface = specula_gpu::Surface { specular: 0.0, ..specula_gpu::Surface::default() };
            }
        }
        let frame = FrameDesc {
            view_proj: vp.to_cols_array(),
            clear_color: clear,
            draws: &draws,
            reverse_z: true,
            environment,
            lights: &[],
            time_s: 0.0,
            overlay: &[],
            draws_2d: &[],
            bones: &bones,
            // The stage shows assets, never effects (nor baked GI: its own lights only).
            effects: Default::default(),
            gi: Default::default(),
            terrains: &[],
        };
        // A target per light kind: its render graph stays as built (lit and silhouette frames
        // alternate, and a target rebuilds its graph whenever its settings change).
        let kind = match &req.light {
            None => 0u8,
            Some((rig, igpu)) => 1 + u8::from(*igpu) * 2 + u8::from(rig.bloom),
        };
        let fresh = !self.studio.targets.iter().any(|t| t.key == (w, h, kind));
        let target = self.studio_target(w, h, kind).map_err(ToolOutput::error)?;
        // A new target's first frame also builds its render graph (shown apart in perf zones).
        let _z = fresh.then(|| profile::zone("studio_first_frame"));
        self.gpu.render(target, &frame).map_err(|e| ToolOutput::error(capture::gpu_failure("render", &e, self.windowed)))?;
        drop(_z);
        let _r = profile::zone("studio_readback");
        self.gpu.read_rgba8(target).map_err(|e| ToolOutput::error(capture::gpu_failure("readback", &e, self.windowed)))
    }

    /// The studio's own offscreen target of `w` × `h` for a light kind, apart from the sight
    /// tools': at most [`MAX_STUDIO_TARGETS`] and [`MAX_TARGET_BYTES`] (estimated: colour, depth,
    /// post chain and shadow map), the least recently used freed first, and every one freed after
    /// [`TARGET_IDLE`] without use. A target per light kind keeps its render graph as built (lit
    /// and silhouette frames alternate; a target rebuilds its graph when its settings change).
    fn studio_target(&mut self, w: u32, h: u32, kind: u8) -> Result<TargetId, String> {
        let now = Instant::now();
        if let Some(t) = self.studio.targets.iter_mut().find(|t| t.key == (w, h, kind)) {
            t.last = now;
            return Ok(t.id);
        }
        // (The shadow map is one image every render shares: see studio_shadow_map.)
        let bytes = u64::from(w) * u64::from(h) * 32;
        loop {
            let total: u64 = self.studio.targets.iter().map(|t| t.bytes).sum();
            if self.studio.targets.is_empty() || (self.studio.targets.len() < MAX_STUDIO_TARGETS && total + bytes <= MAX_TARGET_BYTES) {
                break;
            }
            let (i, _) = self.studio.targets.iter().enumerate().min_by_key(|(_, t)| t.last).expect("not empty");
            let old = self.studio.targets.remove(i);
            self.gpu.destroy_target(old.id);
        }
        let _z = profile::zone("studio_target_create");
        let id = self.gpu.create_target(w, h).map_err(|e| capture::gpu_failure("create target", &e, self.windowed))?;
        self.studio.targets.push(Target { key: (w, h, kind), id, bytes, last: now });
        Ok(id)
    }

    /// Queues a call's render (round-robin per session; see [`Engine::pump_studio`]) and pins
    /// its builds on the GPU until the call ends, so the model sync between frames can't drop
    /// an upload the call still needs (a textured glTF's textures leave the CPU when uploaded).
    pub(crate) fn studio_enqueue(&mut self, job: Box<RenderJob>) {
        let now = Instant::now();
        let pin = self.studio.pins.entry(job.call).or_insert_with(|| Pin { session: job.session.clone(), builds: BTreeSet::new(), last: now, done: false });
        pin.last = now;
        let mut changed = false;
        for p in &job.req.subjects {
            changed |= pin.builds.insert(p.built.id);
        }
        if changed {
            self.studio_pins_changed();
        }
        self.studio.queue.entry(job.session.clone()).or_default().push_back(job);
    }

    /// A call ended: its builds stay pinned for [`PIN_GRACE`], then go.
    pub(crate) fn studio_done(&mut self, call: u64) {
        if let Some(p) = self.studio.pins.get_mut(&call) {
            p.done = true;
            p.last = Instant::now();
        }
    }

    fn studio_pins_changed(&mut self) {
        let set: BTreeSet<u64> = self.studio.pins.values().flat_map(|p| p.builds.iter().copied()).collect();
        self.models.set_pinned(set);
    }

    /// Whether studio renders are waiting.
    pub(crate) fn studio_busy(&self) -> bool {
        !self.studio.queue.is_empty() || !self.studio.previews.is_empty()
    }

    /// One step of the next queued preview (they go round-robin), charged to the render
    /// credit; whether one ran.
    fn pump_preview(&mut self) -> bool {
        let Some(mut job) = self.studio.previews.pop_front() else { return false };
        if Instant::now() > job.deadline {
            let _ = job.reply.send(error("timed_out", "the vfx_preview ran out of time while its frames were queued; call again (fewer frames or a smaller tile render sooner)"));
            return true;
        }
        let _z = profile::zone("studio_preview");
        let started = Instant::now();
        let step = self.vfx_preview_step(&mut job);
        self.studio.credit_ms -= started.elapsed().as_secs_f64() * 1000.0;
        match step {
            crate::vfx_tools::PreviewStep::Done(answer) => {
                let _ = job.reply.send(answer);
            }
            crate::vfx_tools::PreviewStep::Handed => {}
            crate::vfx_tools::PreviewStep::More => self.studio.previews.push_back(job),
        }
        true
    }

    /// Renders queued studio frames, round-robin over sessions, within the render budget:
    /// credit earned at `share` of the wall time since the last pump (at most [`MAX_CREDIT_MS`]
    /// saved), spent by each render's measured time. In debt, nothing runs until it is paid
    /// back, so a 30 ms render on a busy iGPU is followed by frames without studio work and the
    /// studio's average stays at `share` of the engine thread. Renders of closed sessions and
    /// past their deadline are dropped; a render whose pipeline variant is still compiling
    /// waits. Idle: targets unused for [`TARGET_IDLE`] and stale pins go.
    pub(crate) fn pump_studio(&mut self, share: f64, server: &Server) {
        let now = Instant::now();
        let earned = self.studio.last_pump.map_or(0.0, |t| (now - t).as_secs_f64() * 1000.0 * share);
        self.studio.last_pump = Some(now);
        self.studio.shared.note(None, share);
        if self.studio.last_tidy.is_none_or(|t| now - t >= TIDY_EVERY) {
            self.studio.last_tidy = Some(now);
            self.studio_tidy(now, server);
        }
        if self.studio.queue.is_empty() && self.studio.previews.is_empty() {
            self.studio.credit_ms = MAX_CREDIT_MS;
            return;
        }
        let alive: BTreeSet<String> = server.all_sessions().into_iter().map(|s| s.id).collect();
        let gone: Vec<String> = self.studio.queue.keys().filter(|k| !alive.contains(*k)).cloned().collect();
        for k in gone {
            for job in self.studio.queue.remove(&k).into_iter().flatten() {
                let _ = job.reply.send(Err(error("session_closed", "the session closed; its studio render was dropped")));
            }
        }
        for job in std::mem::take(&mut self.studio.previews) {
            // A playtest's preview outlives its starter's session (the playtest goes on); its
            // deadline bounds it.
            if job.from_playtest || alive.contains(&job.session) {
                self.studio.previews.push_back(job);
            } else {
                let _ = job.reply.send(error("session_closed", "the session closed; its vfx_preview was dropped"));
            }
        }
        self.studio.credit_ms = (self.studio.credit_ms + earned).min(MAX_CREDIT_MS);
        while self.studio.credit_ms > 0.0 {
            // Previews and the studio's renders take turns (each alone has every turn).
            self.studio.preview_turn = !self.studio.preview_turn;
            if (self.studio.preview_turn || self.studio.queue.is_empty()) && self.pump_preview() {
                continue;
            }
            let Some(job) = self.studio.next() else { return };
            if Instant::now() > job.deadline {
                let _ = job.reply.send(Err(error("timed_out", "the studio call ran out of time while its renders were queued; call again")));
                continue;
            }
            // Builds not on the GPU yet (or not down to the level the render draws) are uploaded
            // a level per step, coarsest first, through streaming's queued path and within the
            // frame's upload budget, charged like a render: a cold call's first frames don't
            // upload whole models at once.
            if let Some(p) = job.req.subjects.iter().find(|p| self.models.uploaded.get(&p.built.id).is_none_or(|m| m.resident_from > p.lod.min(m.lods.len().saturating_sub(1)))) {
                let allowance = self.stream.allowance();
                if allowance.is_some_and(|a| a.ms <= 0.0 && !a.first_free) {
                    // This frame's upload budget is spent: the next frame.
                    self.studio.put_back(job);
                    return;
                }
                let built = Arc::clone(&p.built);
                let _z = profile::zone("studio_upload");
                let started = Instant::now();
                let uploaded = crate::streaming::upload_next_level(&mut self.gpu, &mut self.models, &self.library, &built);
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                self.studio.credit_ms -= ms;
                // The streamer's resident total counts it (its memory budget holds).
                self.stream.uploaded_outside(&uploaded);
                if allowance.is_some() {
                    self.stream.charge(crate::two_d::Used { items: 1, bytes: *uploaded.as_ref().unwrap_or(&0), ms, deferred: 0 });
                }
                match uploaded {
                    Err(e) => {
                        let _ = job.reply.send(Err(error("render_unavailable", e)));
                    }
                    // A textured build whose pixels already left the CPU: the library builds it again.
                    Ok(_) if !self.models.uploaded.contains_key(&built.id) => {
                        let _ = job.reply.send(Err(error("changed_during_job", "the model's textures were released by an earlier upload and it is being imported again; call again in a moment")));
                    }
                    Ok(_) => self.studio.put_back(job),
                }
                continue;
            }
            if self.studio_variants_waiting(&job.req) {
                self.studio.put_back(job);
                return;
            }
            let _c = job.trace.enter();
            let mut span = crate::trace_engine::job_span("studio_render");
            span.field("subjects", job.req.subjects.len());
            let started = Instant::now();
            let r = self.render_req(&job.req);
            drop(span);
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            let stats = self.gpu.last_frame_stats_of(FramePath::Offscreen);
            let gpu_ms = stats.as_ref().and_then(|s| s.gpu.as_ref().filter(|g| g.frame == s.frame)).map_or(0.0, |g| g.total_ms);
            let cpu = stats.map(|s| s.cpu).unwrap_or_default();
            let cost = RenderCost { ms, gpu_ms, record_ms: cpu.record_ms, submit_ms: cpu.submit_ms, wait_ms: cpu.wait_ms };
            let r = r.map(|px| Rendered { px, cost });
            self.studio.credit_ms -= ms;
            self.studio.shared.note(Some(ms), share);
            if let Some(p) = self.studio.pins.get_mut(&job.call) {
                p.last = Instant::now();
            }
            let _ = job.reply.send(r);
        }
    }

    /// Idle upkeep: targets unused for [`TARGET_IDLE`], and pins of closed sessions or idle
    /// for [`PIN_IDLE`].
    fn studio_tidy(&mut self, now: Instant, server: &Server) {
        let idle: Vec<usize> = self.studio.targets.iter().enumerate().filter(|(_, t)| now - t.last > TARGET_IDLE).map(|(i, _)| i).collect();
        for i in idle.into_iter().rev() {
            let t = self.studio.targets.remove(i);
            self.gpu.destroy_target(t.id);
        }
        if self.studio.pins.is_empty() {
            return;
        }
        let alive: BTreeSet<String> = server.all_sessions().into_iter().map(|s| s.id).collect();
        let before = self.studio.pins.len();
        self.studio.pins.retain(|_, p| alive.contains(&p.session) && now - p.last <= if p.done { PIN_GRACE } else { PIN_IDLE });
        if self.studio.pins.len() != before {
            self.studio_pins_changed();
        }
    }

    /// Whether a render needs a skinned or textured pipeline variant that is still compiling
    /// (the builds' uploads started it; the render waits); a failed variant doesn't wait (the
    /// render reports it).
    fn studio_variants_waiting(&mut self, req: &RenderReq) -> bool {
        let skinned = req.subjects.iter().any(|p| !p.skin.is_empty());
        let textured = req.subjects.iter().any(|p| p.built.materials.values().any(|m| m.textured()));
        if !skinned && !textured {
            return false;
        }
        self.models.refresh_ready(&mut self.gpu);
        let v = self.gpu.pipeline_variants();
        let failed = |sk: bool| v.failed.as_ref().is_some_and(|f| if sk { f.skinned } else { f.textured });
        // (The builds' uploads, a step earlier, started building them.)
        (skinned && !v.skinned && !failed(true)) || (textured && !v.textured && !failed(false))
    }

    /// Answers every queued studio render: the engine is stopping.
    pub(crate) fn studio_shutting_down(&mut self) {
        for (_, q) in std::mem::take(&mut self.studio.queue) {
            for job in q {
                let _ = job.reply.send(Err(ToolOutput::error("the engine is shutting down; the call was not completed")));
            }
        }
    }
}

/// A studio call's context on its caller's connection thread: the shared state, the way to the
/// engine, the call's id (admission, pins) and its deadline.
pub(crate) struct Ctx<'a> {
    shared: &'a Shared,
    tools: &'a crate::tools::Tools,
    session: &'a SessionInfo,
    call: u64,
    deadline: Instant,
    renders: usize,
    /// The smallest scale a big frame rendered at in this call (1: none was scaled).
    render_scale: f32,
    /// This call's renders: how many, and the most one took on the engine thread and on the GPU.
    timing: (usize, RenderCost),
}

impl Drop for Ctx<'_> {
    fn drop(&mut self) {
        self.shared.finish(self.call);
        self.tools.post(self.session, Command::StudioDone(self.call));
    }
}

/// Answers a studio tool on the caller's connection thread: admission first (before any work),
/// then the call; only its renders go to the engine thread.
pub(crate) fn run(tools: &crate::tools::Tools, shared: &Shared, session: &SessionInfo, call: StudioCall) -> ToolOutput {
    let planned = planned_renders(&call, shared, &session.id);
    let id = match shared.admit(&session.id, planned) {
        Ok(id) => id,
        Err(e) => return e,
    };
    let mut ctx = Ctx { shared, tools, session, call: id, deadline: Instant::now() + CALL_LIMIT, renders: 0, render_scale: 1.0, timing: (0, RenderCost::default()) };
    ctx.studio_call(call)
}

/// About how many renders a call will make (for admission; the cap is enforced as it renders).
fn planned_renders(call: &StudioCall, shared: &Shared, agent: &str) -> usize {
    let sheet = |s: &SheetSpec| -> usize {
        let per_view = |v: &View| match v {
            View::Shot(_) => 2,
            View::Angle { .. } => usize::from(s.passes.iter().any(|p| *p != Pass::Silhouette)) + usize::from(s.passes.contains(&Pass::Silhouette)),
        };
        s.subjects.len() * s.views.iter().map(per_view).sum::<usize>()
    };
    match call {
        StudioCall::Stage { subjects, .. } => usize::from(!subjects.is_empty()),
        StudioCall::Sheet { sheet: s, .. } => sheet(s),
        StudioCall::Lineup { subjects, .. } => 2 + 3 * subjects.len(),
        StudioCall::Compare { from: CompareFrom::Pair { .. }, sheet: s, .. } => 2 * sheet(&SheetSpec { subjects: vec![SubjectSpec { spec: ModelSpec::new("x"), label: None }], ..s.clone() }),
        StudioCall::Compare { from: CompareFrom::Mark(name), .. } => shared.marks().spec(agent, name).map_or(0, |s| sheet(&s)),
        // 13 for the subject, 5 per sibling (defaulted siblings: up to MAX_SUBJECTS - 1).
        StudioCall::Check { siblings, .. } => 13 + 5 * siblings.as_ref().map_or(MAX_SUBJECTS - 1, Vec::len),
        StudioCall::Style { .. } => 0,
    }
}

impl Ctx<'_> {
    /// Answers a studio tool (see [`crate::studio_tools`]).
    fn studio_call(&mut self, call: StudioCall) -> ToolOutput {
        // Marks belong to the session (one agent's connection).
        let agent = self.session.id.clone();
        let mut bibles = Bibles::default();
        match call {
            StudioCall::Stage { subjects, view, rig, size, references, format } => {
                if subjects.is_empty() {
                    return self.studio_describe();
                }
                match self.studio_staged(&subjects, &mut bibles) {
                    Ok(staged) => self.studio_stage(&staged, &view, rig, size, references, format),
                    Err(e) => e,
                }
            }
            StudioCall::Sheet { sheet, mark, format } => {
                let staged = match self.studio_staged(&sheet.subjects, &mut bibles) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                let out = match self.studio_sheet(&sheet, &staged, "SHEET") {
                    Ok(o) => o,
                    Err(e) => return e,
                };
                let answer = sheet_answer(&sheet, &out, format, mark.as_deref());
                let packet = sheet_packet("studio_sheet", &sheet, &out, &staged);
                if let Some(m) = &mark {
                    self.shared.marks().put(&agent, m, sheet.clone(), out.clone());
                }
                self.studio_output(out.w, out.h, &out.rgba, packet, answer, true)
            }
            StudioCall::Lineup { subjects, rig, width, format } => match self.studio_staged(&subjects, &mut bibles) {
                Ok(staged) => self.studio_lineup(&staged, rig, width, format),
                Err(e) => e,
            },
            StudioCall::Compare { from, sheet, format } => self.studio_compare(&agent, from, sheet, format, &mut bibles),
            StudioCall::Check { subject, siblings, tile, image, format } => self.studio_check(subject, siblings, tile, image, format, &mut bibles),
            StudioCall::Style { subjects, folder, variants, format } => self.studio_style(subjects, folder, variants, format, &mut bibles),
        }
    }

    // -- subjects ---------------------------------------------------------------------------

    /// Builds and facts of `subjects`, waiting (as `model_build` does) for any still building.
    fn studio_staged(&mut self, subjects: &[SubjectSpec], bibles: &mut Bibles) -> Result<Vec<Staged>, ToolOutput> {
        let root = self.shared.library.lock().root().to_path_buf();
        let mut out = Vec::new();
        for s in subjects {
            let mut built = None;
            for _ in 0..3 {
                if Instant::now() > self.deadline {
                    return Err(error("timed_out", "the subjects' builds didn't finish in time; call again"));
                }
                let requested = self.shared.library.lock().request(&s.spec);
                match requested {
                    Err(e) => {
                        let mut j = e.to_json();
                        j.set("subject", Json::from(s.spec.recipe.as_str()));
                        return Err(ToolOutput::error(j.to_string()));
                    }
                    Ok(models::BuildRequest::Ready(b, _)) => {
                        built = Some(b);
                        break;
                    }
                    Ok(models::BuildRequest::Pending(_)) => {
                        let waited = self.tools.send(self.session, Command::ModelBuild(s.spec.clone()));
                        if waited.is_error {
                            let text = waited.content.iter().find_map(|c| match c {
                                Content::Text(t) => Some(t.clone()),
                                Content::Image { .. } => None,
                            });
                            let mut j = text.as_deref().and_then(|t| Json::parse(t).ok()).unwrap_or_else(|| Json::obj([("error", Json::from("build_failed")), ("message", Json::from(text.unwrap_or_default()))]));
                            j.set("subject", Json::from(s.spec.recipe.as_str()));
                            return Err(ToolOutput::error(j.to_string()));
                        }
                    }
                }
            }
            let Some(built) = built else { return Err(error("model_building", format!("{} is still building; call again in a moment", s.spec.recipe))) };
            let bible = bibles.get(&root, &built, &s.spec.recipe)?;
            let text = if s.spec.recipe.ends_with(".json") { std::fs::read_to_string(root.join(&s.spec.recipe)).ok() } else { None };
            let info = text.as_deref().and_then(RecipeInfo::parse);
            let stem = Path::new(&s.spec.recipe).file_stem().map(|x| x.to_string_lossy().to_string());
            let mut facts = AssetFacts::from_model(&built.model, info.as_ref(), stem.as_deref(), &s.spec.params, models::texture_bytes(&built));
            // A glTF base-colour texture paints what its vertex colours don't say.
            facts.textured = built.materials.values().any(|m| m.base.is_some());
            if let Some(l) = &s.label {
                facts.subject = l.clone();
            }
            let b = built.model.bounds;
            let (lo, hi) = if b.max[0] >= b.min[0] { (Vec3::new(b.min[0], b.min[1], b.min[2]), Vec3::new(b.max[0], b.max[1], b.max[2])) } else { (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5)) };
            let skin = built.model.rig.as_ref().map(|r| vec![Affine::IDENTITY; r.len()]).unwrap_or_default();
            out.push(Staged { spec: s.spec.clone(), name: facts.subject.clone(), built, facts, bible, lo, hi, skin });
        }
        Ok(out)
    }

    // -- rendering --------------------------------------------------------------------------

    /// One frame of the stage: `placed` subjects, the references with the figure at `refs`, the
    /// ground (lit frames only), through `cam` at `w`×`h`. The engine renders it between frames
    /// within its render budget, round-robin with other agents' calls; this thread waits.
    #[allow(clippy::too_many_arguments)]
    fn stage_render(&mut self, placed: &[Placed], refs: Option<Vec3>, cam: &Cam, w: u32, h: u32, light: Light) -> Result<Vec<u8>, ToolOutput> {
        if self.renders >= MAX_RENDERS {
            return Err(error("too_many_renders", format!("this call needs more than {MAX_RENDERS} renders (fewer subjects, views or passes; each shot and silhouette is a render of its own)")));
        }
        self.renders += 1;
        let (rw, rh) = render_size(w, h);
        if rw < w {
            self.render_scale = self.render_scale.min(rw as f32 / w as f32);
        }
        let req = RenderReq {
            subjects: placed.iter().map(|p| PlacedOwned { built: Arc::clone(&p.s.built), skin: p.s.skin.clone(), at: p.at, yaw: p.yaw, lod: p.lod, radius: p.s.radius() }).collect(),
            refs,
            cam: *cam,
            w: rw,
            h: rh,
            aspect: w as f32 / h as f32,
            light: match light {
                Light::Rig(rig, igpu) => Some((rig.clone(), igpu)),
                Light::Silhouette => None,
            },
        };
        let (tx, rx) = crate::jobs::channel();
        let job = RenderJob { call: self.call, session: self.session.id.clone(), req, reply: tx, deadline: self.deadline, trace: specula_core::trace::Ctx::current() };
        if !self.tools.post(self.session, Command::StudioRender(Box::new(job))) {
            return Err(ToolOutput::error("the engine is shutting down; the call was not completed"));
        }
        let wait = self.deadline.saturating_duration_since(Instant::now()) + Duration::from_secs(2);
        let r = match rx.recv_timeout(wait) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(error("timed_out", "the studio call ran out of time waiting for its renders; call again (or ask for less)")),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(error("studio_dropped", "the engine dropped the render (it is stopping, or your session closed)")),
        };
        self.shared.rendered(self.call);
        r.map(|r| {
            self.timing = (self.timing.0 + 1, self.timing.1.max(r.cost));
            if (rw, rh) == (w, h) {
                r.px
            } else {
                upscale(&r.px, rw, rh, w, h)
            }
        })
    }

    /// The `style.toml` rules on a staged subject, within the call's time limit (the palette
    /// search is the costly part: it stops at the deadline and the call answers `timed_out`).
    fn style_report(&self, s: &Staged) -> Result<specula_studio::Report, ToolOutput> {
        let deadline = self.deadline;
        let r = s.bible.check_with(&s.facts, &|| Instant::now() > deadline);
        if r.cut_short {
            return Err(error("timed_out", format!("the call ran out of time checking {}'s colours against the palette; check fewer subjects per call", s.name)));
        }
        Ok(r)
    }

    /// A lit render and its mask (a second, silhouette render) through `cam`, with measures.
    #[allow(clippy::too_many_arguments)]
    fn stage_measured(&mut self, s: &Staged, cam: Cam, w: u32, h: u32, rig: &Rig, igpu: bool, lod: usize, ring_px: u32) -> Result<Render, ToolOutput> {
        let placed = [Placed { s, at: Vec3::ZERO, lod: lod.min(s.lods() - 1), yaw: 0.0 }];
        let colour = self.stage_render(&placed, None, &cam, w, h, Light::Rig(rig, igpu))?;
        let sil = self.stage_render(&placed, None, &cam, w, h, Light::Silhouette)?;
        let mask = Mask::from_silhouette(&sil, w, h);
        let px_height = mask.bbox().map_or(0.0, |b| b[3] as f32);
        let lmap = img::lightness_map(&colour);
        let inner = mask.eroded(1);
        let subject = img::lightness_of(&lmap, if inner.area() >= 8 { &inner } else { &mask });
        let ring = img::lightness_of(&lmap, &mask.ring(ring_px));
        let edge = img::edge_contrast_of(&lmap, &mask, ring_px);
        Ok(Render { w, h, colour, mask, measure: ShotMeasure { px_height, subject, ring, edge }, lod: placed[0].lod, cam })
    }

    /// One of the standard shots of a subject. The game shots render the crop around it at true
    /// pixel size (measures are in the shot's viewport).
    fn studio_shot(&mut self, s: &Staged, shot: Shot, tile: u32) -> Result<Render, ToolOutput> {
        let rules = s.rules().clone();
        let ring = rules.taste.ring_px;
        match shot {
            Shot::CloseUp => {
                let cam = fit(s.lo, s.hi, 35.0, 20.0, FIT_FOV, 1.0, 1.08);
                self.stage_measured(s, cam, tile, tile, &neutral_rig(), false, 0, ring)
            }
            Shot::Gameplay | Shot::WorstLight | Shot::Phone | Shot::Igpu => {
                let g = &rules.shots.gameplay;
                let vh = match shot {
                    Shot::Phone => rules.shots.phone_viewport[1],
                    Shot::Igpu => rules.shots.igpu_viewport[1],
                    _ => g.viewport[1],
                };
                let gs = game_cam(s.centre(), s.radius(), s.lods(), g, vh);
                let rig = if shot == Shot::WorstLight { rules.stage.worst() } else { rules.stage.game.clone() };
                let mut r = self.stage_measured(s, gs.cam, gs.crop, gs.crop, &rig, shot == Shot::Igpu, gs.lod, ring)?;
                // Pixel heights are in the shot's viewport.
                r.measure.px_height *= gs.scale;
                Ok(r)
            }
        }
    }

    /// An angle tile: colour under `rig` (with the references when `refs`), or a silhouette.
    #[allow(clippy::too_many_arguments)]
    fn studio_angle(&mut self, s: &Staged, az: f32, el: f32, rig: &Rig, tile: u32, refs: bool, silhouette: bool) -> Result<(Vec<u8>, Option<[u32; 4]>), ToolOutput> {
        // The subject turns by -az in front of a camera at azimuth 0: the same view as a camera at
        // az, with the references always to its left.
        let (slo, shi) = turned_box(s, -az, Vec3::ZERO);
        let at = Vec3::new(-(slo.x + shi.x) * 0.5, 0.0, -(slo.z + shi.z) * 0.5);
        let (slo, shi) = (slo + at, shi + at);
        let (mut lo, mut hi) = (slo, shi);
        let fig = (refs && !silhouette).then(|| {
            let f = Vec3::new(slo.x - 0.6 - 0.35, 0.0, 0.0);
            let (a, b) = reference_box(f);
            lo = lo.min(a);
            hi = hi.max(b);
            f
        });
        let placed = [Placed { s, at, lod: 0, yaw: -az }];
        let cam = fit(lo, hi, 0.0, el, FIT_FOV, 1.0, 1.08);
        let light = if silhouette { Light::Silhouette } else { Light::Rig(rig, false) };
        let rgba = self.stage_render(&placed, fig, &cam, tile, tile, light)?;
        let bbox = project_box(&cam.view_proj(1.0), slo, shi, tile, tile);
        Ok((rgba, bbox))
    }

    /// Thumbnail silhouettes (fitted to `thumb_px`) from the three angles.
    fn studio_thumbs(&mut self, s: &Staged) -> Result<Vec<Mask>, ToolOutput> {
        let px = s.rules().taste.thumb_px;
        let mut out = Vec::new();
        for (az, el) in THUMB_ANGLES {
            let (rgba, _) = self.studio_angle(s, az, el, &neutral_rig(), px, false, true)?;
            out.push(Mask::from_silhouette(&rgba, px, px).fitted(px));
        }
        Ok(out)
    }

    /// Everything the taste checks measure on a subject (and the renders, for its sheet row).
    fn studio_measure(&mut self, s: &Staged, tile: u32) -> Result<(Measured, Vec<(String, Render)>), ToolOutput> {
        let silhouettes = self.studio_thumbs(s)?;
        let rules = s.rules().clone();
        // Flat lighting: the close framing under the game light.
        let lit = self.stage_measured(s, fit(s.lo, s.hi, 35.0, 20.0, FIT_FOV, 1.0, 1.08), tile, tile, &rules.stage.game, false, 0, rules.taste.ring_px)?;
        let mut shots = Vec::new();
        for shot in [Shot::CloseUp, Shot::Gameplay, Shot::WorstLight, Shot::Phone] {
            shots.push((shot.name().to_string(), self.studio_shot(s, shot, tile)?));
        }
        let get = |n: &str| shots.iter().find(|(k, _)| k == n).map(|(_, r)| r.measure);
        let m = Measured { subject: s.name.clone(), silhouettes, gameplay: get("gameplay"), phone: get("phone"), worst: get("worst_light"), lit: Some(lit.measure.subject) };
        shots.push(("game_light".into(), lit));
        Ok((m, shots))
    }

    // -- output -----------------------------------------------------------------------------

    /// Saves an image and its packet in the captures folder; answers `answer` (plus the image).
    fn studio_output(&mut self, w: u32, h: u32, rgba: &[u8], mut packet: Json, answer: Json, with_image: bool) -> ToolOutput {
        // On the caller's thread: encoding and writing take nothing from the engine.
        let (n, c) = self.timing;
        if n > 0 {
            packet.set(
                "renders",
                Json::obj([
                    ("count", Json::from(n as f64)),
                    ("max_engine_ms", r1(c.ms)),
                    ("max_record_ms", r1(c.record_ms)),
                    ("max_submit_ms", r1(c.submit_ms)),
                    ("max_fence_wait_ms", r1(c.wait_ms)),
                    ("max_gpu_ms", r1(c.gpu_ms)),
                ]),
            );
        }
        let frame = self.shared.next_frame();
        encode_and_save(&self.shared.capture_dir, frame, w, h, rgba, packet, answer, with_image)
    }

    // -- the tools --------------------------------------------------------------------------

    fn studio_describe(&mut self) -> ToolOutput {
        let root = self.shared.library.lock().root().to_path_buf();
        let (rules, source) = match std::fs::read_to_string(root.join("style.toml")) {
            Ok(t) => {
                let style = std::fs::read_to_string(root.join("style.json")).ok().and_then(|s| specula_model::Style::parse(&s).ok()).unwrap_or_default();
                match Bible::load(&style, Some(&t), "style.toml") {
                    Ok(b) => (b.rules, "style.toml"),
                    Err(e) => return ToolOutput::error(e.to_json().to_string()),
                }
            }
            Err(_) => (StyleRules::defaults(), "defaults (no style.toml at the assets root; each recipe's own is found next to it or above)"),
        };
        let g = &rules.shots.gameplay;
        ToolOutput::text(
            Json::obj([
                ("stage", Json::from("isolated: builds from the model library on a ground plane and backdrop; the world is never read or written")),
                ("rigs", Json::obj([("neutral", neutral_rig().to_json()), ("game", rules.stage.game.to_json()), ("worst", rules.stage.worst().to_json())])),
                (
                    "shots",
                    Json::obj([
                        ("close_up", Json::from("three-quarter (az 35, el 20), framed to fit, neutral light, LOD 0")),
                        ("gameplay", Json::from(format!("the game camera: pitch {} deg, yaw {} deg, {} m, fov {} deg, {}x{}; true pixel size, LOD by screen size, game light", g.pitch_deg, g.yaw_deg, g.distance_m, g.fov_deg, g.viewport[0], g.viewport[1]))),
                        ("worst_light", Json::from("the gameplay camera under the worst light (key behind the subject, less fill)")),
                        ("phone", Json::from(format!("the gameplay camera at {}x{}", rules.shots.phone_viewport[0], rules.shots.phone_viewport[1]))),
                        ("igpu", Json::from(format!("the gameplay camera at {}x{}, 1024 shadows, no bloom or FXAA", rules.shots.igpu_viewport[0], rules.shots.igpu_viewport[1]))),
                    ]),
                ),
                ("references", Json::from("a 1.8 m figure and a 1 m cube, left of the subjects")),
                ("views", Json::from("front, back, left, right, three_quarter, top, a shot, or {az, el}; az 0 = the model's front (+Z), 90 = its left (+X)")),
                ("rules", rules.to_json()),
                ("rules_source", Json::from(source)),
            ])
            .to_string(),
        )
    }

    fn studio_stage(&mut self, staged: &[Staged], view: &View, rig: Option<RigName>, size: [u32; 2], references: bool, format: Format) -> ToolOutput {
        let [w, h] = size;
        let rules = staged[0].rules().clone();
        let rig_of = |n: RigName| match n {
            RigName::Neutral => neutral_rig(),
            RigName::Game => rules.stage.game.clone(),
            RigName::Worst => rules.stage.worst(),
        };
        let (cam, light_rig, igpu, lod, shot_vh) = match view {
            View::Angle { az, el, .. } => {
                let (_, _, lo, hi) = arrange(staged, references, 0);
                (fit(lo, hi, *az, *el, FIT_FOV, w as f32 / h as f32, 1.06), rig_of(rig.unwrap_or(RigName::Neutral)), false, 0, None)
            }
            View::Shot(Shot::CloseUp) => {
                let (_, _, lo, hi) = arrange(staged, references, 0);
                (fit(lo, hi, 35.0, 20.0, FIT_FOV, w as f32 / h as f32, 1.06), rig_of(rig.unwrap_or(RigName::Neutral)), false, 0, None)
            }
            View::Shot(shot) => {
                let g = &rules.shots.gameplay;
                let vh = match shot {
                    Shot::Phone => rules.shots.phone_viewport[1],
                    Shot::Igpu => rules.shots.igpu_viewport[1],
                    _ => g.viewport[1],
                };
                let (_, _, lo, hi) = arrange(staged, references, 0);
                let target = (lo + hi) * 0.5;
                let eye = target + dir_of(g.yaw_deg, g.pitch_deg) * g.distance_m;
                let default = if *shot == Shot::WorstLight { RigName::Worst } else { RigName::Game };
                (Cam { eye, target, fov: g.fov_deg }, rig_of(rig.unwrap_or(default)), *shot == Shot::Igpu, 0, Some(vh))
            }
        };
        let (mut placed, fig, _, _) = arrange(staged, references, lod);
        if let Some(vh) = shot_vh {
            // Each subject's level of detail from its own size and distance on screen, as in the game.
            for p in &mut placed {
                let d = (cam.eye - (p.s.centre() + p.at)).length();
                p.lod = models::select_lod(models::screen_px(p.s.radius(), d, cam.fov, vh), p.s.lods(), None);
            }
        }
        let rgba = match self.stage_render(&placed, fig, &cam, w, h, Light::Rig(&light_rig, igpu)) {
            Ok(r) => r,
            Err(e) => return e,
        };
        let vp = cam.view_proj(w as f32 / h as f32);
        // Pixels in the shot's own viewport per pixel of this image.
        let scale = shot_vh.map_or(1.0, |vh| vh as f32 / h as f32);
        let subjects: Vec<Json> = placed
            .iter()
            .map(|p| {
                let b = project_box(&vp, p.s.lo + p.at, p.s.hi + p.at, w, h);
                let mut j = Json::obj([("subject", Json::from(p.s.name.as_str())), ("recipe", Json::from(p.s.spec.recipe.as_str())), ("height_m", r3(p.s.facts.size_m[1])), ("figure_ratio", r3(p.s.facts.size_m[1] / FIGURE_M)), ("lod", Json::from(p.lod))]);
                if let Some(b) = b {
                    j.set("box", Json::arr(b.map(Json::from)));
                    j.set("px_height", r3(b[3] as f32 * scale));
                }
                j
            })
            .collect();
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("studio_stage")),
            ("view", Json::from(view.label())),
            ("camera", cam.to_json()),
            ("rig", light_rig.to_json()),
            ("references", Json::from(references)),
            ("render_scale", r3(self.render_scale)),
            ("subjects", Json::Arr(subjects)),
            ("rules", Json::from(rules.source.as_str())),
        ]);
        if let Some(vh) = shot_vh {
            packet.set("note", Json::from(format!("px_height is in the shot's {vh}-pixel-tall viewport (boxes are projected bounds)")));
        }
        let answer = match format {
            Format::Detailed => packet.clone(),
            Format::Concise => Json::obj([("view", Json::from(view.label())), ("subjects", packet.get("subjects").cloned().unwrap_or(Json::Null))]),
        };
        if fig.is_some() {
            packet.set("figure_m", Json::from(FIGURE_M));
        }
        self.studio_output(w, h, &rgba, packet, answer, true)
    }

    /// Renders a sheet (`title` in its header).
    fn studio_sheet(&mut self, sheet: &SheetSpec, staged: &[Staged], title: &str) -> Result<SheetOut, ToolOutput> {
        // Columns are views; each subject is a block of one row per pass, labelled underneath.
        let cols = sheet.views.len() as u32;
        let np = sheet.passes.len() as u32;
        let blocks = staged.len() as u32;
        let label_h = 2 * film::CHAR_H + 6;
        let head_h = film::CHAR_H + 6;
        let tile = sheet.tile.min(MAX_SHEET_PX / cols.max(1)).min((((MAX_SHEET_PX - head_h) / blocks.max(1)).saturating_sub(label_h)) / np.max(1)).max(32);
        let block_h = np * tile + label_h;
        let (w, h) = (cols * tile, head_h + blocks * block_h);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        film::fill(&mut rgba, w, 0, 0, w, h, [14, 17, 26, 255]);
        for (ci, v) in sheet.views.iter().enumerate() {
            film::draw_text(&mut rgba, w, ci as u32 * tile + 4, 3, &v.label(), [160, 168, 180]);
        }
        film::draw_text(&mut rgba, w, w.saturating_sub((title.len() as u32 + 1) * film::CHAR_W), 3, title, [200, 200, 190]);
        let mut tiles = Vec::new();
        let mut subjects = Vec::new();
        for (ri, s) in staged.iter().enumerate() {
            let top = head_h + ri as u32 * block_h;
            let rig = match sheet.rig {
                RigName::Neutral => neutral_rig(),
                RigName::Game => s.rules().stage.game.clone(),
                RigName::Worst => s.rules().stage.worst(),
            };
            let mut ci = 0u32;
            for v in &sheet.views {
                // What this view renders, once for all its passes.
                let (colour, sil, bbox, shot, lod, sw, sh): ViewPixels = match v {
                    View::Angle { az, el, .. } => {
                        let colour = if sheet.passes.iter().any(|p| *p != Pass::Silhouette) { Some(self.studio_angle(s, *az, *el, &rig, tile, sheet.references, false)?) } else { None };
                        let sil = if sheet.passes.contains(&Pass::Silhouette) { Some(self.studio_angle(s, *az, *el, &rig, tile, false, true)?.0) } else { None };
                        let bbox = colour.as_ref().and_then(|c| c.1);
                        (colour.map(|c| c.0), sil, bbox, None, 0, tile, tile)
                    }
                    View::Shot(shot) => {
                        let r = self.studio_shot(s, *shot, tile)?;
                        let mag = tile as f32 / r.w as f32;
                        let bbox = r.mask.bbox().map(|b| b.map(|x| (x as f32 * mag) as u32));
                        (Some(r.colour), Some(mask_rgba(&r.mask)), bbox, Some((r.measure, mag)), r.lod, r.w, r.h)
                    }
                };
                for (pi, p) in sheet.passes.iter().enumerate() {
                    let (x0, y0) = (ci * tile, top + pi as u32 * tile);
                    let src = match p {
                        Pass::Colour => colour.clone(),
                        Pass::Value => colour.clone().map(|mut c| {
                            img::to_value(&mut c);
                            c
                        }),
                        Pass::Silhouette => sil.clone(),
                    };
                    let Some(src) = src else { continue };
                    blit_scaled(&mut rgba, w, x0, y0, &src, sw, sh, tile, tile);
                    let region = bbox.unwrap_or([tile / 4, tile / 4, tile / 2, tile / 2]);
                    let lightness = film::mean_luma(&rgba, w, x0 + region[0], y0 + region[1], region[2].min(tile), region[3].min(tile));
                    if let Some((m, mag)) = &shot {
                        film::draw_text(&mut rgba, w, x0 + 3, y0 + 3, &format!("{:.0}PX X{mag:.1}", m.px_height), [235, 230, 200]);
                    }
                    if np > 1 {
                        film::draw_text(&mut rgba, w, x0 + 3, y0 + tile - film::CHAR_H - 3, p.name(), [120, 128, 140]);
                    }
                    tiles.push(TileRec { row: ri, subject: s.name.clone(), view: v.label(), pass: *p, rect: [x0, y0, tile, tile], bbox, lightness, lod, shot });
                }
                ci += 1;
            }
            let y0 = top + (np - 1) * tile;
            let report = self.style_report(s)?;
            let failed = report.errors().count();
            film::draw_text(&mut rgba, w, 4, y0 + tile + 2, &format!("{}  {:.2}M  {} TRIS", s.name, s.facts.size_m[1], s.facts.lod_triangles.first().copied().unwrap_or(0)), [230, 226, 214]);
            let more = if failed > 3 { format!(" +{} MORE", failed - 3) } else { String::new() };
            let verdict = if failed == 0 { "STYLE.TOML OK".to_string() } else { format!("STYLE.TOML: {failed} ERRORS: {}{more}", report.errors().take(3).map(|e| e.check).collect::<Vec<_>>().join(", ")) };
            film::draw_text(&mut rgba, w, 4, y0 + tile + 4 + film::CHAR_H, &verdict, if failed == 0 { [140, 220, 160] } else { [255, 120, 100] });
            subjects.push((s.name.clone(), s.facts.to_json(6), report));
        }
        Ok(SheetOut { w, h, rgba, tiles, subjects })
    }

    fn studio_lineup(&mut self, staged: &[Staged], rig: RigName, width: u32, format: Format) -> ToolOutput {
        let rules = staged[0].rules().clone();
        let rig = match rig {
            RigName::Neutral => neutral_rig(),
            RigName::Game => rules.stage.game.clone(),
            RigName::Worst => rules.stage.worst(),
        };
        let (placed, fig, lo, hi) = arrange(staged, true, 0);
        let label_h = 2 * film::CHAR_H + 6;
        let span = (hi.x - lo.x).max(0.5);
        let tall = (hi.y - lo.y).max(0.5);
        let view_h = ((width as f32 * tall * 1.25 / span) as u32).clamp(160, 900);
        let views = [("front", 0.0f32, 6.0f32), ("three_quarter", 35.0, 18.0)];
        let (w, h) = (width, views.len() as u32 * (view_h + label_h));
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        film::fill(&mut rgba, w, 0, 0, w, h, [14, 17, 26, 255]);
        let mut boxes: Vec<Vec<Json>> = vec![Vec::new(); placed.len()];
        let mut cams = Vec::new();
        for (vi, (name, az, el)) in views.iter().enumerate() {
            let y0 = vi as u32 * (view_h + label_h);
            let cam = fit(lo, hi, *az, *el, FIT_FOV, w as f32 / view_h as f32, 1.04);
            let frame = match self.stage_render(&placed, fig, &cam, w, view_h, Light::Rig(&rig, false)) {
                Ok(f) => f,
                Err(e) => return e,
            };
            film::blit(&mut rgba, w, 0, y0, &frame, w, view_h);
            let vp = cam.view_proj(w as f32 / view_h as f32);
            for (i, p) in placed.iter().enumerate() {
                if let Some(b) = project_box(&vp, p.s.lo + p.at, p.s.hi + p.at, w, view_h) {
                    if vi == 0 {
                        let text = format!("{} {:.2}M", p.s.name, p.s.facts.size_m[1]);
                        film::draw_text(&mut rgba, w, b[0].min(w.saturating_sub(4)), y0 + view_h + 2 + (i as u32 % 2) * (film::CHAR_H + 2), &text, [230, 226, 214]);
                    }
                    boxes[i].push(Json::obj([("view", Json::from(*name)), ("box", Json::arr([b[0], b[1] + y0, b[2], b[3]].map(Json::from)))]));
                }
            }
            if vi == 1 {
                film::draw_text(&mut rgba, w, 4, y0 + view_h + 2, "FIGURE 1.8M, CUBE 1M", [170, 175, 185]);
            }
            cams.push(Json::obj([("view", Json::from(*name)), ("camera", cam.to_json())]));
        }
        // Distinctness between the subjects.
        let mut measured = Vec::new();
        for s in staged {
            match self.studio_thumbs(s) {
                Ok(m) => measured.push(Measured { subject: s.name.clone(), silhouettes: m, ..Default::default() }),
                Err(e) => return e,
            }
        }
        let matrix = taste::iou_matrix(&measured);
        let findings = taste::check_siblings(&measured, &rules);
        let alike: Vec<Json> = findings.iter().filter(|f| f.is_error()).map(Finding::to_json).collect();
        let subjects: Vec<Json> = placed
            .iter()
            .zip(boxes)
            .map(|(p, b)| Json::obj([("subject", Json::from(p.s.name.as_str())), ("recipe", Json::from(p.s.spec.recipe.as_str())), ("height_m", r3(p.s.facts.size_m[1])), ("figure_ratio", r3(p.s.facts.size_m[1] / FIGURE_M)), ("boxes", Json::Arr(b))]))
            .collect();
        let packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("studio_lineup")),
            ("render_scale", r3(self.render_scale)),
            ("rig", rig.to_json()),
            ("views", Json::Arr(cams)),
            ("subjects", Json::Arr(subjects.clone())),
            ("silhouette_iou", Json::arr(matrix.iter().map(|row| Json::arr(row.iter().map(|x| r3(*x)))))),
            ("silhouette_max_iou", Json::from(rules.taste.silhouette_max_iou)),
            ("alike", Json::Arr(alike.clone())),
            ("distinctness", Json::arr(findings.iter().map(Finding::to_json))),
            ("rules", Json::from(rules.source.as_str())),
        ]);
        let answer = match format {
            Format::Detailed => packet.clone(),
            Format::Concise => Json::obj([("subjects", Json::Arr(subjects)), ("silhouette_iou", packet.get("silhouette_iou").cloned().unwrap_or(Json::Null)), ("alike", Json::Arr(alike))]),
        };
        self.studio_output(w, h, &rgba, packet, answer, true)
    }

    fn studio_compare(&mut self, agent: &str, from: CompareFrom, sheet: SheetSpec, format: Format, bibles: &mut Bibles) -> ToolOutput {
        let (before_out, after_out, sheet, mode) = match from {
            CompareFrom::Mark(name) => {
                let Some((spec, before)) = self.shared.marks().get(agent, &name) else {
                    let have = self.shared.marks().names(agent);
                    return error("unknown_mark", format!("no mark '{name}' for you (yours: {}); make one with studio_sheet {{mark}}", if have.is_empty() { "none".to_string() } else { have.join(", ") }));
                };
                let staged = match self.studio_staged(&spec.subjects, bibles) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                match self.studio_sheet(&spec, &staged, "AFTER (NOW)") {
                    Ok(after) => (before, after, spec, format!("mark {name}")),
                    Err(e) => return e,
                }
            }
            CompareFrom::Pair { before, after } => {
                let mut sb = sheet.clone();
                sb.subjects = vec![before];
                let mut sa = sheet.clone();
                sa.subjects = vec![after];
                let staged_b = match self.studio_staged(&sb.subjects, bibles) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                let staged_a = match self.studio_staged(&sa.subjects, bibles) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                let b = match self.studio_sheet(&sb, &staged_b, "BEFORE") {
                    Ok(o) => o,
                    Err(e) => return e,
                };
                match self.studio_sheet(&sa, &staged_a, "AFTER") {
                    Ok(a) => (b, a, sheet, "pair".to_string()),
                    Err(e) => return e,
                }
            }
        };
        // Before over after (same layout: the same sheet).
        let w = before_out.w.max(after_out.w);
        let h = before_out.h + after_out.h;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        film::blit(&mut rgba, w, 0, 0, &before_out.rgba, before_out.w, before_out.h);
        film::blit(&mut rgba, w, 0, before_out.h, &after_out.rgba, after_out.w, after_out.h);
        // Each half's title (the sheets' own, right-aligned in their headers) becomes BEFORE / AFTER.
        let tw = 13 * film::CHAR_W;
        film::fill(&mut rgba, w, w.saturating_sub(tw), 0, tw.min(w), film::CHAR_H + 6, [14, 17, 26, 255]);
        film::fill(&mut rgba, w, w.saturating_sub(tw), before_out.h, tw.min(w), film::CHAR_H + 6, [14, 17, 26, 255]);
        film::draw_text(&mut rgba, w, w.saturating_sub(7 * film::CHAR_W), 3, "BEFORE", [255, 210, 120]);
        film::draw_text(&mut rgba, w, w.saturating_sub(7 * film::CHAR_W), before_out.h + 3, "AFTER", [140, 220, 160]);
        // Two full sheets can be twice the size cap: the stack is scaled down to fit it.
        let fit = (MAX_SHEET_PX as f32 / w as f32).min(MAX_SHEET_PX as f32 / h as f32).min(1.0);
        let (w, h, rgba) = if fit < 1.0 {
            let (nw, nh) = (((w as f32 * fit) as u32).max(1), ((h as f32 * fit) as u32).max(1));
            let mut small = vec![0u8; (nw * nh * 4) as usize];
            blit_scaled(&mut small, nw, 0, 0, &rgba, w, h, nw, nh);
            (nw, nh, small)
        } else {
            (w, h, rgba)
        };
        let mut tiles = Vec::new();
        for (tb, ta) in before_out.tiles.iter().zip(&after_out.tiles) {
            let [x, y, tw, th] = tb.rect;
            let (mut changed, mut n) = (0u64, 0u64);
            for yy in 0..th {
                for xx in 0..tw {
                    let i = (((y + yy) * before_out.w + x + xx) * 4) as usize;
                    let j = (((ta.rect[1] + yy) * after_out.w + ta.rect[0] + xx) * 4) as usize;
                    if i + 3 < before_out.rgba.len() && j + 3 < after_out.rgba.len() {
                        n += 1;
                        if (0..3).any(|k| before_out.rgba[i + k].abs_diff(after_out.rgba[j + k]) > CHANGED) {
                            changed += 1;
                        }
                    }
                }
            }
            let mut t = Json::obj([("view", Json::from(tb.view.as_str())), ("pass", Json::from(tb.pass.name())), ("changed", r3(changed as f32 / n.max(1) as f32)), ("lightness", Json::arr([r3(tb.lightness), r3(ta.lightness)]))]);
            if let (Some((mb, _)), Some((ma, _))) = (&tb.shot, &ta.shot) {
                t.set("px_height", Json::arr([r3(mb.px_height), r3(ma.px_height)]));
                t.set("contrast", Json::arr([r3(mb.contrast()), r3(ma.contrast())]));
            }
            tiles.push(t);
        }
        // Facts and findings, subject by subject (rows line up).
        let mut subjects = Vec::new();
        for ((nb, fb, rb), (na, fa, ra)) in before_out.subjects.iter().zip(&after_out.subjects) {
            let mut changed = Vec::new();
            for k in ["triangles", "size_m", "aspect", "materials", "texture_kb"] {
                if fb.get(k) != fa.get(k) {
                    changed.push(Json::obj([("fact", Json::from(k)), ("before", fb.get(k).cloned().unwrap_or(Json::Null)), ("after", fa.get(k).cloned().unwrap_or(Json::Null))]));
                }
            }
            let key = |f: &Finding| (f.check, f.rule.clone());
            let err_b: Vec<_> = rb.errors().map(key).collect();
            let err_a: Vec<_> = ra.errors().map(key).collect();
            let fixed: Vec<Json> = rb.errors().filter(|f| !err_a.contains(&key(f))).map(|f| Json::from(f.check)).collect();
            let new: Vec<Json> = ra.errors().filter(|f| !err_b.contains(&key(f))).map(Finding::to_json).collect();
            let still: Vec<Json> = ra.errors().filter(|f| err_b.contains(&key(f))).map(|f| Json::obj([("check", Json::from(f.check)), ("value", r3(f.value as f32)), ("limit", r3(f.limit as f32))])).collect();
            subjects.push(Json::obj([
                ("before", Json::from(nb.as_str())),
                ("after", Json::from(na.as_str())),
                ("facts_changed", Json::Arr(changed)),
                ("fixed", Json::Arr(fixed)),
                ("new_errors", Json::Arr(new)),
                ("still_failing", Json::Arr(still)),
            ]));
        }
        let changed_tiles = tiles.iter().filter(|t| t.get("changed").and_then(Json::as_f64).unwrap_or(0.0) > 0.0).count();
        let packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("studio_compare")),
            ("mode", Json::from(mode.as_str())),
            ("layout", Json::from("before over after; tiles at the same place in each half")),
            ("scale", r3(fit)),
            ("views", Json::arr(sheet.views.iter().map(|v| Json::from(v.label())))),
            ("tiles", Json::Arr(tiles.clone())),
            ("subjects", Json::Arr(subjects.clone())),
            ("before_tiles", Json::arr(before_out.tiles.iter().map(TileRec::to_json))),
            ("after_tiles", Json::arr(after_out.tiles.iter().map(TileRec::to_json))),
        ]);
        let answer = match format {
            Format::Detailed => packet.clone(),
            Format::Concise => Json::obj([("mode", Json::from(mode.as_str())), ("tiles_changed", Json::from(changed_tiles)), ("tiles", Json::Arr(tiles)), ("subjects", Json::Arr(subjects))]),
        };
        self.studio_output(w, h, &rgba, packet, answer, true)
    }

    /// The default siblings of a subject: the other recipes of its category in its folder.
    fn studio_siblings(&self, subject: &Staged) -> Vec<SubjectSpec> {
        let Some(cat) = subject.facts.category.clone() else { return Vec::new() };
        let root = self.shared.library.lock().root().to_path_buf();
        let folder = Path::new(&subject.spec.recipe).parent().map(Path::to_path_buf).unwrap_or_default();
        let mut names: Vec<String> = std::fs::read_dir(root.join(&folder))
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".json") && !is_texture_graph(&root.join(&folder).join(n))).collect())
            .unwrap_or_default();
        names.sort();
        let mine = Path::new(&subject.spec.recipe).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut out = Vec::new();
        // Bounded: at most MAX_SCANNED recipes are opened to find siblings.
        for n in names.into_iter().take(MAX_SCANNED) {
            if n == mine || out.len() + 1 >= MAX_SUBJECTS {
                continue;
            }
            let rel = folder.join(&n).to_string_lossy().replace('\\', "/");
            let same = std::fs::read_to_string(root.join(&rel)).ok().and_then(|t| RecipeInfo::parse(&t)).is_some_and(|i| i.category.as_deref() == Some(cat.as_str()));
            if same {
                out.push(SubjectSpec { spec: ModelSpec::new(rel), label: None });
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn studio_check(&mut self, subject: SubjectSpec, siblings: Option<Vec<SubjectSpec>>, tile: u32, image: bool, format: Format, bibles: &mut Bibles) -> ToolOutput {
        let first = match self.studio_staged(std::slice::from_ref(&subject), bibles) {
            Ok(mut s) => s.remove(0),
            Err(e) => return e,
        };
        let sib_specs = siblings.unwrap_or_else(|| self.studio_siblings(&first));
        let sibs = match self.studio_staged(&sib_specs, bibles) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let rules = first.rules().clone();
        let (measured, renders) = match self.studio_measure(&first, tile) {
            Ok(m) => m,
            Err(e) => return e,
        };
        let mut all = vec![measured.clone()];
        let mut sib_colour = Vec::new();
        for s in &sibs {
            let thumbs = match self.studio_thumbs(s) {
                Ok(t) => t,
                Err(e) => return e,
            };
            all.push(Measured { subject: s.name.clone(), silhouettes: thumbs, ..Default::default() });
            match self.studio_shot(s, Shot::CloseUp, tile) {
                Ok(r) => sib_colour.push(r),
                Err(e) => return e,
            }
        }
        let mut report = match self.style_report(&first) {
            Ok(r) => r,
            Err(e) => return e,
        };
        report.extend(taste::check_subject(&measured, &rules));
        let distinct = taste::check_siblings(&all, &rules);
        report.extend(distinct.iter().take(1).cloned());
        let matrix = taste::iou_matrix(&all);
        let errors: Vec<&Finding> = report.findings.iter().collect();
        let crit = critique::critique(&first.name, first.facts.category.as_deref(), &errors, &rules);
        // The sheet: one row per subject: three silhouettes, value, colour, gameplay, worst, phone.
        let cols = ["front", "side", "three_quarter"].map(|n| format!("silhouette {n}")).into_iter().chain(["value", "colour", "gameplay", "worst_light", "phone"].map(String::from)).collect::<Vec<_>>();
        let label_h = 2 * film::CHAR_H + 6;
        let head_h = film::CHAR_H + 6;
        let rows = 1 + sibs.len() as u32;
        let tile = tile.min(MAX_SHEET_PX / cols.len() as u32).min(((MAX_SHEET_PX - head_h) / rows).saturating_sub(label_h));
        let (w, h) = (cols.len() as u32 * tile, head_h + rows * (tile + label_h));
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        film::fill(&mut rgba, w, 0, 0, w, h, [14, 17, 26, 255]);
        for (i, c) in cols.iter().enumerate() {
            film::draw_text(&mut rgba, w, i as u32 * tile + 3, 3, &c.replace("silhouette ", "sil "), [160, 168, 180]);
        }
        let get = |n: &str| renders.iter().find(|(k, _)| k == n).map(|(_, r)| r);
        let mut tile_recs = Vec::new();
        for (ri, m) in all.iter().enumerate() {
            let y0 = head_h + ri as u32 * (tile + label_h);
            for (k, s) in m.silhouettes.iter().enumerate() {
                blit_scaled(&mut rgba, w, k as u32 * tile, y0, &mask_rgba(s), s.w, s.h, tile, tile);
            }
            let colour_of = |r: &Render| (r.colour.clone(), r.w, r.h);
            let mut cells: Vec<Option<Cell>> = Vec::new();
            if ri == 0 {
                let close = get("close_up").map(colour_of);
                cells.push(close.clone().map(|(mut c, cw, ch)| {
                    img::to_value(&mut c);
                    (c, cw, ch, None)
                }));
                cells.push(close.map(|(c, cw, ch)| (c, cw, ch, None)));
                for n in ["gameplay", "worst_light", "phone"] {
                    cells.push(get(n).map(|r| (r.colour.clone(), r.w, r.h, Some(r.measure))));
                }
            } else if let Some(r) = sib_colour.get(ri - 1) {
                let mut v = r.colour.clone();
                img::to_value(&mut v);
                cells.push(Some((v, r.w, r.h, None)));
                cells.push(Some((r.colour.clone(), r.w, r.h, None)));
            }
            for (k, cell) in cells.into_iter().enumerate() {
                let Some((px, cw, ch, sm)) = cell else { continue };
                let x0 = (3 + k as u32) * tile;
                blit_scaled(&mut rgba, w, x0, y0, &px, cw, ch, tile, tile);
                if let Some(sm) = sm {
                    film::draw_text(&mut rgba, w, x0 + 3, y0 + 3, &format!("{:.0}PX C{:.2}", sm.px_height, sm.contrast()), [235, 230, 200]);
                    tile_recs.push(Json::obj([("column", Json::from(cols[3 + k].as_str())), ("tile", Json::arr([x0, y0, tile, tile].map(Json::from))), ("measures", sm.to_json())]));
                }
            }
            let iou = if ri == 0 { None } else { Some(matrix[0][ri]) };
            let label = match iou {
                None => format!("{}  (SUBJECT)", m.subject),
                Some(v) => format!("{}  IOU WITH SUBJECT {v:.2}", m.subject),
            };
            film::draw_text(&mut rgba, w, 4, y0 + tile + 2, &label, [230, 226, 214]);
            if ri == 0 {
                let n = report.errors().count();
                let ex = report.findings.iter().filter(|f| f.exempt).count();
                let v = if n == 0 {
                    if ex == 0 { "ALL CHECKS PASS".to_string() } else { format!("ALL CHECKS PASS ({ex} EXEMPT BY STYLE.TOML)") }
                } else {
                    let more = if n > 4 { format!(" +{} MORE", n - 4) } else { String::new() };
                    format!("{n} ERRORS: {}{more}", report.errors().take(4).map(|e| e.check).collect::<Vec<_>>().join(", "))
                };
                film::draw_text(&mut rgba, w, 4, y0 + tile + 4 + film::CHAR_H, &v, if n == 0 { [140, 220, 160] } else { [255, 120, 100] });
            }
        }
        let siblings_json: Vec<Json> = all.iter().enumerate().skip(1).map(|(i, m)| Json::obj([("subject", Json::from(m.subject.as_str())), ("iou_with_subject", r3(matrix[0][i]))])).collect();
        let mut measures = Json::obj(renders.iter().map(|(n, r)| (n.clone(), {
            let mut j = r.measure.to_json();
            j.set("lod", Json::from(r.lod));
            j.set("camera", r.cam.to_json());
            j
        })));
        measures.set(
            "thumbnails",
            Json::obj([
                ("px", Json::from(rules.taste.thumb_px)),
                ("angles", Json::arr(THUMB_ANGLES.map(|(az, el)| Json::arr([Json::from(az), Json::from(el)])))),
                ("solidity", Json::arr(measured.silhouettes.iter().map(|m| r3(m.solidity())))),
            ]),
        );
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("studio_check")),
            ("subject", Json::from(first.name.as_str())),
            ("recipe", spec_json(&first.spec)),
            ("report", report.to_json(64, Some(128))),
            ("critique", crit.clone()),
            ("siblings", Json::Arr(siblings_json.clone())),
            ("silhouette_iou", Json::arr(matrix.iter().map(|row| Json::arr(row.iter().map(|x| r3(*x)))))),
            ("measures", measures),
            ("facts", first.facts.to_json(16)),
            ("tiles", Json::Arr(tile_recs)),
            ("rules", rules.to_json()),
            ("columns", Json::arr(cols.iter().map(|c| Json::from(c.as_str())))),
        ]);
        packet.set("thumb_px", Json::from(rules.taste.thumb_px));
        let mut answer = Json::obj([
            ("subject", Json::from(first.name.as_str())),
            ("passed", Json::from(report.passed())),
            ("failed", Json::from(report.errors().count())),
            ("errors", Json::arr(report.errors().take(if format == Format::Detailed { 64 } else { CONCISE_ERRORS }).map(Finding::to_json))),
            ("critique", if format == Format::Detailed { crit } else { crit.get("next").cloned().map(|n| Json::obj([("verdict", crit.get("verdict").cloned().unwrap_or(Json::Null)), ("next", n)])).unwrap_or(Json::Null) }),
            ("siblings", Json::Arr(siblings_json)),
            ("rules", Json::from(rules.source.as_str())),
        ]);
        if format == Format::Detailed {
            answer.set("report", report.to_json(64, Some(128)));
            answer.set("measures", packet.get("measures").cloned().unwrap_or(Json::Null));
            answer.set("facts", first.facts.to_json(16));
        }
        self.studio_output(w, h, &rgba, packet, answer, image)
    }

    fn studio_style(&mut self, subjects: Vec<SubjectSpec>, folder: Option<String>, variants: bool, format: Format, bibles: &mut Bibles) -> ToolOutput {
        let root = self.shared.library.lock().root().to_path_buf();
        let mut list = subjects;
        if let Some(f) = &folder {
            let mut names: Vec<String> = match std::fs::read_dir(root.join(f)) {
                Ok(d) => d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".json") && !is_texture_graph(&root.join(f).join(n))).collect(),
                Err(e) => return error("not_found", format!("folder '{f}' under the assets root: {e}")),
            };
            if names.len() > crate::studio_tools::MAX_STYLE_SUBJECTS {
                return error("too_many_subjects", format!("folder '{f}' has {} recipes; at most {} per call (check a smaller folder, or list subjects)", names.len(), crate::studio_tools::MAX_STYLE_SUBJECTS));
            }
            names.sort();
            list = names.into_iter().map(|n| SubjectSpec { spec: ModelSpec::new(format!("{f}/{n}")), label: None }).collect();
        }
        if variants {
            let mut wide = Vec::new();
            for s in &list {
                let info = if s.spec.params.is_empty() && s.spec.recipe.ends_with(".json") { std::fs::read_to_string(root.join(&s.spec.recipe)).ok().and_then(|t| RecipeInfo::parse(&t)) } else { None };
                match info.map(|i| i.variants(4, 8)) {
                    Some(sets) if sets.len() > 1 || sets.first().is_some_and(|v| !v.is_empty()) => {
                        for p in sets {
                            let mut spec = s.spec.clone();
                            spec.params = p;
                            wide.push(SubjectSpec { spec, label: None });
                        }
                    }
                    _ => wide.push(s.clone()),
                }
            }
            list = wide;
        }
        if list.len() > crate::studio_tools::MAX_STYLE_SUBJECTS {
            return error("too_many_subjects", format!("{} subjects (variants included); at most {} per call: check a smaller folder, or variants: false", list.len(), crate::studio_tools::MAX_STYLE_SUBJECTS));
        }
        let staged = match self.studio_staged(&list, bibles) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let mut results = Vec::new();
        let mut failed = 0usize;
        let mut bytes = 0usize;
        let mut truncated = 0usize;
        for s in &staged {
            let r = match self.style_report(s) {
                Ok(r) => r,
                Err(e) => return e,
            };
            if !r.passed() {
                failed += 1;
            }
            let j = match format {
                Format::Concise if r.passed() => continue,
                Format::Concise => Json::obj([("subject", Json::from(s.name.as_str())), ("recipe", spec_json(&s.spec)), ("errors", Json::arr(r.errors().take(CONCISE_ERRORS).map(Finding::to_json)))]),
                Format::Detailed => {
                    let mut j = r.to_json(64, Some(64));
                    j.set("subject", Json::from(s.name.as_str()));
                    j.set("recipe", spec_json(&s.spec));
                    j.set("facts", s.facts.to_json(6));
                    j
                }
            };
            let n = j.to_string().len();
            if bytes + n > MAX_ANSWER_BYTES {
                truncated += 1;
                continue;
            }
            bytes += n;
            results.push(j);
        }
        let sources: Vec<Json> = bibles.sources().into_iter().map(Json::from).collect();
        let mut out = Json::obj([
            ("passed", Json::from(failed == 0)),
            ("subjects", Json::from(staged.len())),
            ("failed", Json::from(failed)),
            (if format == Format::Concise { "failing" } else { "results" }, Json::Arr(results)),
            ("rules", Json::Arr(sources)),
        ]);
        if truncated > 0 {
            out.set("not_listed", Json::from(truncated));
            out.set("note", Json::from(format!("{truncated} results left out to stay under {} KiB; check fewer subjects", MAX_ANSWER_BYTES / 1024)));
        }
        ToolOutput::text(out.to_string())
    }
}


/// Encodes a studio image, saves it and its packet as capture `frame` in `dir`, and makes the
/// answer (on a worker thread for queued calls).
#[allow(clippy::too_many_arguments)]
fn encode_and_save(dir: &Path, frame: u64, w: u32, h: u32, rgba: &[u8], mut packet: Json, mut answer: Json, with_image: bool) -> ToolOutput {
    let png_bytes = png::encode_rgba8(w, h, rgba, Level::Fastest);
    let png_path = {
        let p = dir.join(format!("studio_{frame:05}.png"));
        crate::disk::book().write(crate::disk::Category::Captures, "studio", &p, &png_bytes, crate::disk::Need::Agent).map(|()| p).map_err(|e| e.to_string())
    };
    let mut warnings = Vec::new();
    if let Err(e) = &png_path {
        warnings.push(Json::from(format!("capture not saved to disk: {e}")));
    }
    let png_file = png_path.as_ref().ok();
    let packet_path = png_file.map(|p| p.with_extension("json"));
    let image = Json::obj([
        ("width", Json::from(w)),
        ("height", Json::from(h)),
        ("png_bytes", Json::from(png_bytes.len())),
        ("path", png_file.map(|p| p.display().to_string()).into()),
        ("packet_path", packet_path.as_ref().map(|p| p.display().to_string()).into()),
    ]);
    packet.set("image", image.clone());
    packet.set("frame", Json::from(frame));
    if let Some(p) = &packet_path {
        if let Err(e) = crate::disk::book().write(crate::disk::Category::Captures, "studio", p, packet.to_pretty().as_bytes(), crate::disk::Need::Agent) {
            warnings.push(Json::from(format!("packet not saved to disk: {e}")));
        }
    }
    answer.set("image", image);
    if !warnings.is_empty() {
        answer.set("warnings", Json::Arr(warnings));
    }
    let mut content = Vec::new();
    if with_image {
        content.push(Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) });
    }
    content.push(Content::Text(answer.to_string()));
    ToolOutput { content, is_error: false }
}

// ---------------------------------------------------------------------------------------------
// Bibles: each game's rules, loaded once per call

/// The rules and palettes one call uses, by (style.json, style.toml) path.
#[derive(Default)]
struct Bibles {
    loaded: Vec<(BibleKey, Rc<Bible>)>,
}

impl Bibles {
    /// The bible for a build: its style (`style.json`) and the `style.toml` next to the recipe
    /// or above it in the assets root.
    fn get(&mut self, root: &Path, built: &models::Built, recipe: &str) -> Result<Rc<Bible>, ToolOutput> {
        let mut dir = root.join(recipe);
        let mut toml = None;
        while dir.pop() && dir.starts_with(root) {
            let f = dir.join("style.toml");
            if f.is_file() {
                toml = Some(f);
                break;
            }
            if dir == root {
                break;
            }
        }
        // The palette: the build's own style; a glTF has none, so the style.json next to the
        // style.toml found (else the assets root's), whose palette the rules name.
        let style_path = if !built.style.as_os_str().is_empty() {
            Some(if built.style.is_absolute() { built.style.clone() } else { root.join(&built.style) })
        } else {
            toml.as_ref().and_then(|t| t.parent()).map(|d| d.join("style.json")).filter(|p| p.is_file()).or_else(|| Some(root.join("style.json")).filter(|p| p.is_file()))
        };
        let key = (style_path.clone().unwrap_or_default(), toml.clone());
        if let Some((_, b)) = self.loaded.iter().find(|(k, _)| *k == key) {
            return Ok(Rc::clone(b));
        }
        let style = match &style_path {
            Some(p) => match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| specula_model::Style::parse(&t).map_err(|e| e.to_string())) {
                Ok(s) => s,
                Err(e) => return Err(error("style_unreadable", format!("{}: {e}", p.display()))),
            },
            None => specula_model::Style::default(),
        };
        let (text, source) = match &toml {
            Some(f) => match std::fs::read_to_string(f) {
                Ok(t) => (Some(t), f.strip_prefix(root).map(|p| p.display().to_string().replace('\\', "/")).unwrap_or_else(|_| f.display().to_string())),
                Err(e) => return Err(error("style_unreadable", format!("{}: {e}", f.display()))),
            },
            None => (None, "defaults".to_string()),
        };
        let bible = Bible::load(&style, text.as_deref(), &source).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
        let b = Rc::new(bible);
        self.loaded.push((key, Rc::clone(&b)));
        Ok(b)
    }

    fn sources(&self) -> Vec<String> {
        let mut v: Vec<String> = self.loaded.iter().map(|(_, b)| b.rules.source.clone()).collect();
        v.dedup();
        v
    }
}

// ---------------------------------------------------------------------------------------------
// Sheet answers

fn sheet_packet(kind: &str, sheet: &SheetSpec, out: &SheetOut, staged: &[Staged]) -> Json {
    Json::obj([
        ("schema", Json::from(sight::SCHEMA)),
        ("kind", Json::from(kind)),
        ("views", Json::arr(sheet.views.iter().map(|v| Json::from(v.label())))),
        ("passes", Json::arr(sheet.passes.iter().map(|p| Json::from(p.name())))),
        ("rig", Json::from(sheet.rig.name())),
        ("references", Json::from(sheet.references)),
        ("tiles", Json::arr(out.tiles.iter().map(TileRec::to_json))),
        (
            "subjects",
            Json::arr(out.subjects.iter().zip(staged).map(|((n, facts, r), s)| Json::obj([("subject", Json::from(n.as_str())), ("recipe", spec_json(&s.spec)), ("facts", facts.clone()), ("style", r.to_json(32, Some(64)))]))),
        ),
        ("rules", Json::arr(staged.iter().map(|s| Json::from(s.rules().source.as_str())).take(1))),
    ])
}

fn sheet_answer(sheet: &SheetSpec, out: &SheetOut, format: Format, mark: Option<&str>) -> Json {
    let mut j = Json::obj([("views", Json::arr(sheet.views.iter().map(|v| Json::from(v.label())))), ("passes", Json::arr(sheet.passes.iter().map(|p| Json::from(p.name()))))]);
    let subjects = out.subjects.iter().enumerate().map(|(i, (n, facts, r))| {
        let mut s = Json::obj([("subject", Json::from(n.as_str())), ("style_passed", Json::from(r.passed()))]);
        let errs: Vec<Json> = r.errors().take(CONCISE_ERRORS).map(Finding::to_json).collect();
        if !errs.is_empty() {
            s.set("style_errors", Json::Arr(errs));
        }
        let tiles: Vec<Json> = out.tiles.iter().filter(|t| t.row == i).map(|t| match format {
            Format::Detailed => t.to_json(),
            Format::Concise => {
                let mut c = Json::obj([("view", Json::from(t.view.as_str())), ("pass", Json::from(t.pass.name())), ("tile", Json::arr(t.rect.map(Json::from)))]);
                if let Some((m, _)) = &t.shot {
                    c.set("px_height", r3(m.px_height));
                    c.set("contrast", r3(m.contrast()));
                }
                c
            }
        }).collect();
        s.set("tiles", Json::Arr(tiles));
        if format == Format::Detailed {
            s.set("facts", facts.clone());
        }
        s
    });
    j.set("subjects", Json::arr(subjects));
    if let Some(m) = mark {
        j.set("mark", Json::from(m));
    }
    j
}

/// Whether a `.json` file is a texture graph (`"texgraph": 1`, procedural modelling) rather than a
/// model recipe: they live beside the recipes that use them, and a folder sweep must skip them.
fn is_texture_graph(path: &Path) -> bool {
    std::fs::read_to_string(path).ok().and_then(|t| Json::parse(&t).ok()).is_some_and(|j| j.get("texgraph").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_keeps_the_box_in_view() {
        let (lo, hi) = (Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.4, 0.5));
        for (az, el) in [(0.0, 8.0), (90.0, 8.0), (35.0, 20.0), (0.0, 85.0)] {
            let cam = fit(lo, hi, az, el, FIT_FOV, 1.0, 1.05);
            let b = project_box(&cam.view_proj(1.0), lo, hi, 200, 200).expect("in view");
            assert!(b[0] <= 30 || b[1] <= 30, "fills the frame from {az}/{el}: {b:?}");
            assert!(b[0] + b[2] <= 200 && b[1] + b[3] <= 200, "inside from {az}/{el}: {b:?}");
        }
        assert!(project_box(&fit(lo, hi, 0.0, 8.0, FIT_FOV, 1.0, 1.05).view_proj(1.0), Vec3::new(0.0, 0.0, 500.0), Vec3::new(1.0, 1.0, 501.0), 200, 200).is_none(), "behind the camera");
    }

    /// Review probe (round 1, huge.py): a subject whose bounds reach the game camera made the
    /// crop size overflow (a panic in debug, a wrap in release).
    #[test]
    fn game_shots_of_huge_subjects_crop_the_whole_viewport() {
        let c = GameCamera { pitch_deg: 42.0, yaw_deg: 35.0, distance_m: 56.0, fov_deg: 45.0, viewport: [1920, 1080] };
        for radius in [80.0f32, 56.0, 1e6, f32::MAX] {
            let g = game_cam(Vec3::ZERO, radius, 3, &c, 1080);
            assert_eq!((g.crop, g.lod), (512, 0), "radius {radius}");
            assert!((g.scale - 1080.0 / 512.0).abs() < 1e-4 && (g.cam.fov - 45.0).abs() < 1e-4);
        }
        // An ordinary subject: a crop class at true pixel size.
        let g = game_cam(Vec3::new(0.0, 1.0, 0.0), 1.3, 3, &c, 1080);
        assert!(CROP_CLASSES.contains(&g.crop) && g.crop < 200 && g.scale == 1.0, "{g:?}");
        assert!(g.cam.fov < 45.0);
        // A viewport smaller than the smallest class.
        assert!(game_cam(Vec3::ZERO, 0.01, 1, &c, 20).crop <= 20);
        assert_eq!((crop_class(1), crop_class(65), crop_class(500), crop_class(u32::MAX)), (32, 96, 512, 512));
    }

    #[test]
    fn references_stand_on_the_ground_at_their_heights() {
        let parts = reference_parts(Vec3::ZERO);
        let top = parts.iter().filter(|p| p.3 == parts[0].3).map(|p| p.0.y + p.1.y * 0.5).fold(0.0f32, f32::max);
        assert!((top - FIGURE_M).abs() < 0.01, "figure {top}");
        let cube = parts.last().unwrap();
        assert!((cube.0.y - cube.1.y * 0.5).abs() < 1e-6 && (cube.1.y - CUBE_M).abs() < 1e-6);
        let (lo, hi) = reference_box(Vec3::ZERO);
        for (c, s, _, _) in &parts {
            assert!(c.x - s.x * 0.5 >= lo.x - 1e-4 && c.x + s.x * 0.5 <= hi.x + 1e-4, "{c:?} {s:?}");
        }
    }

    #[test]
    fn marks_are_bounded_per_agent() {
        let mut m = Marks::default();
        let sheet = SheetSpec { subjects: Vec::new(), views: Vec::new(), passes: Vec::new(), rig: RigName::Neutral, tile: 64, references: false };
        let out = SheetOut { w: 1, h: 1, rgba: vec![0; 4], tiles: Vec::new(), subjects: Vec::new() };
        for i in 0..12 {
            m.put("a", &format!("m{i}"), sheet.clone(), out.clone());
        }
        m.put("b", "x", sheet.clone(), out.clone());
        assert_eq!(m.names("a").len(), MAX_MARKS_PER_AGENT);
        assert!(m.get("a", "m0").is_none() && m.get("a", "m11").is_some() && m.get("b", "x").is_some());
    }

    #[test]
    fn big_frames_render_within_the_pixel_budget() {
        assert_eq!(render_size(512, 512), (512, 512));
        let (w, h) = render_size(2048, 900);
        assert!(w * h <= MAX_RENDER_PIXELS && w % 16 == 0 && h % 16 == 0, "{w}x{h}");
        assert!(((w as f32 / h as f32) - 2048.0 / 900.0).abs() < 0.05);
        let up = upscale(&[0, 0, 0, 255, 255, 255, 255, 255], 2, 1, 4, 1);
        assert_eq!((up[0], up[4], up[8], up[12]), (0, 64, 191, 255));
    }

    /// Review probe (round 2, jobs.py): one agent can't take the studio; admission happens before
    /// any work, says when to retry, and refuses what the queue can't finish in time.
    #[test]
    fn admission_caps_agents_and_estimates_the_queue() {
        let mut c = Calls { render_ms: 20.0, share: WINDOW_SHARE, ..Calls::default() };
        let a1 = c.admit("a", 10).unwrap();
        let _a2 = c.admit("a", 10).unwrap();
        let busy = c.admit("a", 10).unwrap_err();
        let text = match &busy.content[0] {
            Content::Text(t) => t.clone(),
            _ => panic!(),
        };
        assert!(text.contains("studio_busy") && text.contains("retry_after_s"), "{text}");
        // Another agent still gets in; a huge call behind a long queue doesn't.
        assert!(c.admit("b", 13).is_ok());
        assert!(c.admit("c", 500).is_err(), "500 renders behind a queue can't finish in time");
        // An agent's call ends: it may call again.
        c.running.remove(&a1);
        assert!(c.admit("a", 10).is_ok());
        // Every agent's calls together are capped too.
        let mut many = Calls { render_ms: 1.0, share: 1.0, ..Calls::default() };
        for i in 0..MAX_CALLS {
            many.admit(&format!("s{i}"), 1).unwrap();
        }
        assert!(many.admit("late", 1).is_err());
    }

    /// Review round 3: one shadow map shape for every studio render (the world's when it has
    /// shadows), so renders don't rebind the map and wait for the GPU to go idle.
    #[test]
    fn studio_renders_share_one_shadow_map_shape() {
        let off = specula_gpu::ShadowSettings::default();
        assert_eq!(studio_shadow_map(&off), (STUDIO_SHADOW_RES, 1));
        let game = specula_gpu::ShadowSettings { enabled: true, resolution: 2048, cascades: 3, ..off };
        assert_eq!(studio_shadow_map(&game), (2048, 3));
    }

    #[test]
    fn renders_are_served_round_robin_by_session() {
        let mut q: BTreeMap<String, VecDeque<u32>> = BTreeMap::new();
        q.entry("flood".into()).or_default().extend([1, 2, 3, 4]);
        q.entry("other".into()).or_default().extend([10, 11]);
        let mut last = None;
        let order: Vec<u32> = std::iter::from_fn(|| rr_next(&mut q, &mut last)).collect();
        assert_eq!(order, vec![1, 10, 2, 11, 3, 4]);
        assert!(q.is_empty());
    }

    #[test]
    fn scaled_blits_stay_inside() {
        let mut sheet = vec![0u8; 10 * 10 * 4];
        blit_scaled(&mut sheet, 10, 8, 8, &[255u8; 4 * 4], 2, 2, 4, 4);
        assert_eq!(sheet[((9 * 10 + 9) * 4) as usize], 255);
        let m = Mask { w: 2, h: 1, on: vec![true, false] };
        assert_eq!(mask_rgba(&m)[0], 16);
    }
}
