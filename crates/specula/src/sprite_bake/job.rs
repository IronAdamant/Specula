//! The engine's side of `sprite_bake`: a step job that renders one cell a step (one render with
//! the ID buffer, its readback, the cell's resolve), within the step jobs' share of the engine
//! thread (docs/design/WORKERS.md). The plan was made on the caller's thread before it came here;
//! the PNG is encoded and the files committed there after it leaves.
//!
//! A bake holds its build pinned on the GPU until it ends, and stops at the next step once its
//! owner cancels it (`sprite_bake {cancel: true}`) or its session closes.

use super::plan::{self, Plan};
use crate::jobs;
use crate::models::Built;
use crate::steps::{Step, StepJob, StepStart};
use crate::tools::{Command, Request};
use crate::Engine;
use specula_core::math::Mat4;
use specula_core::Json;
use specula_gpu::{DrawItem, FrameDesc};
use specula_protocol::ToolOutput;
use specula_studio::rules::Rig;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Keys of bakes' GPU pins (apart from the studio's call ids and filmstrips').
static PIN_KEYS: AtomicU64 = AtomicU64::new(1 << 61);

/// What the caller's thread hands the engine.
pub struct Bake {
    pub plan: Plan,
    pub built: Arc<Built>,
    pub rig: Rig,
    /// Pixel art: the palette every visible pixel is snapped to.
    pub palette: Option<Vec<[u8; 3]>>,
    pub cancel: Arc<AtomicBool>,
    /// Where the atlas goes back to the caller's thread.
    pub out: Arc<Mutex<Option<Baked>>>,
}

/// The rendered atlas and what it cost.
pub struct Baked {
    pub rgba: Vec<u8>,
    /// Cells with the model on their outermost pixels (never, when the fit holds).
    pub touching: Vec<usize>,
    pub renders: usize,
    /// The engine thread's time in this bake's steps, and the most one step took.
    pub engine_ms: f64,
    pub max_step_ms: f64,
}

fn err(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

/// The light of a rig, turned with the camera (every direction is lit the same way on screen).
fn environment(rig: &Rig, azimuth_deg: f32, reach: f32, world: &specula_gpu::ShadowSettings) -> specula_gpu::Environment {
    let (az, el) = ((azimuth_deg + rig.key_azimuth_deg).to_radians(), rig.key_elevation_deg.to_radians());
    // One shadow map shape with the window's, so a bake never makes the GPU go idle to rebind it.
    let (resolution, cascades) = if world.enabled { (world.resolution, world.cascades.max(1)) } else { (1024, 1) };
    specula_gpu::Environment {
        sun_direction: [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()],
        sun_color: rig.key_color,
        sun_intensity: rig.key_intensity,
        ambient_color: rig.ambient_color,
        ambient_intensity: rig.ambient_intensity,
        exposure: rig.exposure,
        tonemap: true,
        shadow: specula_gpu::ShadowSettings { enabled: rig.shadows, resolution, cascades, distance: reach, softness: 1.5, bias: 1.0 },
        // No post effect that spreads a pixel into its neighbours: alpha is the ID buffer's.
        bloom: specula_gpu::BloomSettings { enabled: false, ..Default::default() },
        fxaa: false,
        sky: specula_gpu::SkySettings { enabled: false, ..Default::default() },
        ..specula_gpu::Environment::default()
    }
}

/// A bake in steps.
pub(crate) struct BakeJob {
    bake: Bake,
    at: usize,
    atlas: Vec<u8>,
    touching: Vec<usize>,
    pin: u64,
    engine_ms: f64,
    max_step_ms: f64,
}

impl BakeJob {
    pub(crate) fn new(e: &mut Engine, bake: Bake, session: &str) -> BakeJob {
        let pin = PIN_KEYS.fetch_add(1, Ordering::Relaxed);
        e.pin_for_job(pin, session, bake.built.id);
        let [w, h] = bake.plan.atlas;
        BakeJob { atlas: vec![0u8; w as usize * h as usize * 4], bake, at: 0, touching: Vec::new(), pin, engine_ms: 0.0, max_step_ms: 0.0 }
    }

    /// Renders cell `self.at` into the atlas: `Ok(false)` when the pipelines it is drawn with are
    /// still being built (the step is tried again).
    fn render_cell(&mut self, e: &mut Engine) -> Result<bool, ToolOutput> {
        let Some((colour, ids)) = render_raw(e, &self.bake, self.at)? else { return Ok(false) };
        let plan = &self.bake.plan;
        let cell = &plan.cells[self.at];
        let mut px = plan::resolve(&colour, &ids, plan.cell, plan.supersample);
        if !px.chunks_exact(4).any(|p| p[3] > 0) {
            // The fit holds every vertex in view: an empty cell is a missing ID buffer (its
            // pipelines not ready), never a picture to write.
            return Err(err("ids_missing", format!("cell {} came back with no pixel of the model in the ID buffer; nothing was written: call again in a moment", self.at)));
        }
        if let Some(p) = &self.bake.palette {
            plan::snap_to_palette(&mut px, p);
        }
        if plan::touches_edge(&px, plan.cell) {
            self.touching.push(self.at);
        }
        plan::bleed(&mut px, plan.cell, plan::BLEED_PX);
        plan::blit(&mut self.atlas, plan.atlas[0], cell.rect, &px);
        Ok(true)
    }
}

/// The background a cell is rendered over: a colour no model is lit to, so a test sees at once
/// a pixel the ID buffer and the picture disagree on. It never reaches the atlas (alpha 0 there,
/// and the colour bleed writes over it).
pub(crate) const CLEAR: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

/// Cell `at`'s render at the supersampled size: its colour (sRGB RGBA) and its opaque ids, or
/// `None` while the pipelines it is drawn with are still being built.
pub(crate) fn render_raw(e: &mut Engine, bake: &Bake, at: usize) -> Result<Option<(Vec<u8>, Vec<u32>)>, ToolOutput> {
    {
        let plan = &bake.plan;
        let cell = &plan.cells[at];
        let strip = &plan.strips[cell.strip];
        let cam = &plan.cameras[strip.direction];
        let ss = plan.supersample;
        let (rw, rh) = (plan.cell[0] * ss, plan.cell[1] * ss);
        let mut draws: Vec<DrawItem> = Vec::new();
        let mut bones = Vec::new();
        e.models.refresh_ready(&mut e.gpu);
        if let Err(m) = e.models.posed_draws(&mut e.gpu, &bake.built, &plan.poses[cell.pose], &Mat4::IDENTITY, &mut bones, &mut draws) {
            if m.contains("still being built") {
                return Ok(None);
            }
            return Err(err("render_unavailable", m));
        }
        let ids = vec![1u32; draws.len()];
        let reach = cam.eye.length() * 2.0 + 2.0;
        let frame = FrameDesc {
            view_proj: cam.view_proj,
            clear_color: CLEAR,
            draws: &draws,
            reverse_z: true,
            environment: environment(&bake.rig, cam.azimuth_deg, reach, &e.world.env().light.shadow),
            bones: &bones,
            time_s: 0.0,
            ..Default::default()
        };
        let target = e.offscreen_target(rw, rh).map_err(|m| err("render_unavailable", m))?;
        let windowed = e.windowed;
        let gpu = |what: &str, g: &specula_gpu::GpuError| err("render_unavailable", crate::capture::gpu_failure(what, g, windowed));
        if let Err(g) = e.gpu.render_with_ids(target, &frame, &ids) {
            return Err(gpu("render", &g));
        }
        let colour = match e.gpu.read_rgba8(target) {
            Ok(c) => c,
            Err(g) => return Err(gpu("readback", &g)),
        };
        let idimg = match e.gpu.read_ids(target) {
            Ok(i) => i,
            Err(g) => return Err(gpu("id readback", &g)),
        };
        if (idimg.width, idimg.height) != (rw, rh) || colour.len() != (rw * rh * 4) as usize {
            return Err(err("render_unavailable", format!("the render came back {}x{} with {} colour bytes, not {rw}x{rh}", idimg.width, idimg.height, colour.len())));
        }
        Ok(Some((colour, idimg.ids)))
    }
}

impl StepJob for BakeJob {
    fn kind(&self) -> &'static str {
        "sprite_bake"
    }

    fn step(&mut self, e: &mut Engine, _reply: &jobs::Reply) -> Step {
        if self.bake.cancel.load(Ordering::Relaxed) {
            e.studio_done(self.pin);
            return Step::Done(err("cancelled", format!("the bake was cancelled by its owner after {} of {} cells; nothing was written", self.at, self.bake.plan.cells.len())));
        }
        let started = Instant::now();
        let r = self.render_cell(e);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        self.engine_ms += ms;
        self.max_step_ms = self.max_step_ms.max(ms);
        match r {
            Err(out) => {
                e.studio_done(self.pin);
                return Step::Done(out);
            }
            Ok(false) => return Step::More,
            Ok(true) => self.at += 1,
        }
        if self.at < self.bake.plan.cells.len() {
            return Step::More;
        }
        e.studio_done(self.pin);
        let baked = Baked { rgba: std::mem::take(&mut self.atlas), touching: std::mem::take(&mut self.touching), renders: self.at, engine_ms: self.engine_ms, max_step_ms: self.max_step_ms };
        *self.bake.out.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(baked);
        Step::Done(ToolOutput::text("{}"))
    }
}

impl Engine {
    /// `sprite_bake`'s renders: a step job owned by its caller.
    pub(crate) fn start_sprite_bake(&mut self, request: Request) {
        let Request { command: Command::SpriteBake(bake), session, reply, deadline, call, .. } = request else { unreachable!("only sprite bakes come here") };
        let owner = jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        let job = BakeJob::new(self, *bake, &session.id);
        self.step_job(StepStart { owner, session: session.id.clone(), call, reply, deadline, class: crate::schedule::Class::Render }, Box::new(job));
    }
}
