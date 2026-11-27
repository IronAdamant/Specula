//! The engine's side of `sprite_bake` (docs/design/SPRITE_BAKE.md, "How"): a job that keeps the
//! engine thread free (docs/design/WORKERS.md).
//!
//! - The style files (the light's rig, pixel art's palette) are read on a worker.
//! - Each cell is **submitted** to the GPU on a target of the bake's own (`render_async`, its
//!   colour and ID buffer copied back in the same submission) and **landed** in a later step,
//!   once a poll of the fence finds it done (`settle_offscreen_within(0)`, never a wait): the
//!   job answers `Step::Wait` meanwhile and says it has a render on the GPU (`on_gpu`), as
//!   `perf_light` does. Landing is two copies out of mapped memory (`Gpu::read_landed`).
//! - A landed cell's resolve (alpha from the ID buffer, the supersampled colour, the palette,
//!   the outline, the bleed) and its blit into the atlas run on a worker, while the GPU renders
//!   the next cell.
//! - The atlas is encoded, checked and put in the content store on a worker; then the two files
//!   are committed in one journal entry through the project's own commit
//!   (`ProjectCall::CommitFiles`, which waits for a sync in progress as every project commit
//!   does), as the agent that asked, under the project rule: a claim on either path that
//!   appeared while the bake ran refuses the commit, and nothing is written.
//!
//! A bake is a job of its owner's identity: it outlives the connection that started it (a
//! shell's `specula call` closes its session at once) and answers through its own
//! [`jobs::Reply`], which the caller's thread waits on or keeps for the owner to ask later.
//! It stops at its next step once its owner (or a lead, forcing) cancels it, until it begins
//! writing its two files; from then a cancel is refused `too_late` ([`super::Stop`]).
//!
//! What it holds of the engine's (its render target, its pin on the model's build) goes back
//! on every way its step job ends ([`StepJob::on_end`]: answered, refused, timed out, the
//! engine shutting down). The pin is the job's, not its session's: a bake outlives the session
//! that started it.

use super::plan::{self, Plan};
use super::{Progress, Request, Stop};
use crate::jobs::{self, JobOwner};
use crate::models::Built;
use crate::project::FileKind;
use crate::project_tools::{Committed, ProjectCall};
use crate::steps::{Ended, Step, StepJob, StepStart};
use crate::tools::Command;
use crate::workers::{JobKind, Offload};
use crate::Engine;
use specula_core::math::Mat4;
use specula_core::Json;
use specula_gpu::{DrawItem, FrameDesc, TargetId};
use specula_protocol::ToolOutput;
use specula_studio::rules::Rig;
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// Keys of bakes' GPU pins (apart from the studio's call ids and filmstrips').
static PIN_KEYS: AtomicU64 = AtomicU64::new(1 << 61);

/// The longest a bake may run (its step job's deadline): far past any bake the ceilings allow.
pub const MAX_BAKE: Duration = Duration::from_secs(15 * 60);

/// Cells resolving on the workers at once, per bake.
const MAX_RESOLVING: usize = 2;

/// A cell rendered again at most this many times when other renders' ID buffers keep taking its
/// own before it lands (each such render settles the bake's and overwrites its ids); past it the
/// bake ends `render_contended`, nothing written.
pub const MAX_RERENDERS_PER_CELL: u32 = 8;

/// Where a test's gate holds a bake (`SPECULA_TEST_BAKE_HOLD`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hold {
    /// Before its first render.
    Start,
    /// Its atlas encoded and stored, before its writing begins (a cancel is honoured here).
    Encoded,
    /// Its writing begun, before its commit is asked for (a cancel is refused `too_late`).
    Writing,
}

/// Tests only (`SPECULA_TEST_BAKE_HOLD=<atlas file name>[@encoded|@writing]`, several comma-separated): a bake of that
/// atlas holds, while the engine's clock is paused (`time_control`), before its first render
/// (the name alone), once its atlas is encoded and stored (`@encoded`), or once its writing has
/// begun (`@writing`). So a check can cancel a bake in each phase, or claim its path before it
/// ends, on any GPU, by construction (never by timing); the clock played again lets it go on.
/// Read once.
fn held_by_test(png_path: &str, at: Hold) -> bool {
    static HOLD: std::sync::OnceLock<Vec<(String, Hold)>> = std::sync::OnceLock::new();
    let name = png_path.rsplit('/').next().unwrap_or(png_path);
    let holds = HOLD.get_or_init(|| {
        let Ok(v) = std::env::var("SPECULA_TEST_BAKE_HOLD") else { return Vec::new() };
        // Several, comma-separated, at most a few (a test's own atlas names).
        v.split(',')
            .take(8)
            .filter_map(|one| {
                let one = one.trim();
                let (file, phase) = match one.split_once('@') {
                    None => (one, Hold::Start),
                    Some((f, "encoded")) => (f, Hold::Encoded),
                    Some((f, "writing")) => (f, Hold::Writing),
                    Some(_) => return None,
                };
                (!file.is_empty()).then(|| (file.to_owned(), phase))
            })
            .collect()
    });
    holds.iter().any(|(f, p)| f == name && *p == at)
}

/// What the caller's thread hands the engine.
pub struct Bake {
    pub req: Request,
    pub plan: Plan,
    pub built: Arc<Built>,
    /// The two files' bases (version, hash) when the bake was asked for: the commit is refused
    /// if either changed meanwhile.
    pub bases: [(u64, u64); 2],
    /// The assets root the style files are found under.
    pub assets: std::path::PathBuf,
    pub store: Option<Arc<crate::blobs::ContentStore>>,
    pub stop: Arc<Stop>,
    /// The owner's name as the engine knows it, set when the job starts (another agent's
    /// refused cancel names it).
    pub owner_name: Arc<std::sync::OnceLock<String>>,
    pub progress: Arc<Progress>,
    /// The bake's one answer (the caller's thread waits on it, or keeps it for its owner).
    pub done: jobs::Reply,
    pub started: Instant,
}

/// What a bake cost: its renders (a cell rendered again counts twice), the engine thread's
/// time in its steps, and the most one step took.
pub struct Cost {
    pub renders: usize,
    pub engine_ms: f64,
    pub max_step_ms: f64,
    /// The model's build uploaded to the GPU by the bake's renders: 1 when no entity had it there,
    /// 0 when one did; never more (its pin holds it while the bake runs).
    pub uploads: u64,
}

/// Tests: the atlas once every cell is in it, and the cells with the model on their outermost
/// pixels (never, when the fit holds).
#[cfg(test)]
pub struct Baked {
    pub rgba: Vec<u8>,
    pub touching: Vec<usize>,
    pub renders: usize,
}

fn err(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

/// A bake's look: the rig, what the light is called, the palette (pixel art), the outline's
/// colour and notes for the answer.
pub struct Look {
    pub rig: Rig,
    pub light: String,
    pub palette: Option<Vec<[u8; 3]>>,
    pub outline: Option<[u8; 3]>,
    pub notes: Vec<String>,
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
        // No shadow map: a sprite carries no ground to receive one, a 2D game draws its own,
        // and the cascades are fitted for a perspective view, not this orthographic one
        // (SPRITE_BAKE.md, "Feet, outline, in place, shadow"). The rig's `shadows` is not read.
        shadow: specula_gpu::ShadowSettings { enabled: false, resolution, cascades, distance: reach, softness: 1.5, bias: 1.0 },
        // No post effect that spreads a pixel into its neighbours: alpha is the ID buffer's.
        bloom: specula_gpu::BloomSettings { enabled: false, ..Default::default() },
        fxaa: false,
        sky: specula_gpu::SkySettings { enabled: false, ..Default::default() },
        ..specula_gpu::Environment::default()
    }
}

/// The background a cell is rendered over: a colour no model is lit to, so a test sees at once
/// a pixel the ID buffer and the picture disagree on. It never reaches the atlas (alpha 0 there,
/// and the colour bleed writes over it).
pub(crate) const CLEAR: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

/// A cell's render: its colour (sRGB RGBA) and its opaque ids.
pub(crate) type Raw = (Vec<u8>, Vec<u32>);

/// Tests: every landed cell's render, by cell.
type Tap = Rc<RefCell<Vec<(usize, Raw)>>>;

/// What a cell's resolve on its worker came to: whether the model is on the cell's outermost
/// pixels, or why the cell could not be used.
type Resolved = Result<bool, ToolOutput>;

/// The cells resolving on the workers, as their continuations report back.
#[derive(Default)]
struct Resolving {
    in_flight: usize,
    done: usize,
    touching: Vec<usize>,
    failed: Option<ToolOutput>,
}

/// Where the bake is.
enum Phase {
    /// Cells submitted, landed and resolved.
    Cells,
    /// The atlas encoded and stored on a worker.
    Encoding(Rc<RefCell<Option<EncodeOut>>>),
    /// Encoded; held by a test's gate before its writing begins, or after.
    Held(Hold, Encoded),
    /// The two files' commit asked for; its answer comes on this channel.
    Committing(mpsc::Receiver<Committed>, Encoded),
}

/// The encoded atlas and the frame data's text, ready to commit; and the content the encode put
/// in the store when it was new there (its id and mark), taken back if the bake is cancelled.
struct Encoded {
    pointer: String,
    png_bytes: usize,
    text: String,
    stored: Option<(crate::blobs::BlobId, u64)>,
}

/// What the encode on its worker came to: encoded, failed, or stopped by a cancel before it
/// stored anything.
type EncodeOut = Result<Option<Encoded>, ToolOutput>;

/// A bake in steps.
pub(crate) struct BakeJob {
    bake: Bake,
    look: Look,
    owner: JobOwner,
    session: String,
    call: u64,
    target: Option<TargetId>,
    /// Its size (the cell times the supersample, a side).
    target_size: (u32, u32),
    warm: bool,
    /// The next cell to submit, and the one on the GPU.
    next: usize,
    flying: Option<usize>,
    /// A cell landed (read out of the readback at once, so no later ID render can take its ids)
    /// and waiting for a worker slot to resolve on.
    landed: Option<(usize, Raw)>,
    /// Cells rendered again (another render took their ids before they landed), by cell.
    rerendered: BTreeMap<usize, u32>,
    /// Its target and pin given back ([`BakeJob::release`]): once, whichever way it ends.
    released: bool,
    resolving: Rc<RefCell<Resolving>>,
    atlas: Arc<Mutex<Vec<u8>>>,
    phase: Phase,
    renders: usize,
    uploads: u64,
    pin: u64,
    engine_ms: f64,
    max_step_ms: f64,
    /// Tests: every landed cell's render, as it came back.
    tap: Option<Tap>,
}

impl BakeJob {
    pub(crate) fn new(e: &mut Engine, bake: Bake, look: Look, owner: JobOwner, session: &str, call: u64) -> BakeJob {
        let pin = PIN_KEYS.fetch_add(1, Ordering::Relaxed);
        // The job's pin, not its session's (review 1, finding 1): the session of a shell's call
        // is gone within a second, and a pin of its pruned then.
        e.pin_for_kept_job(pin, bake.built.id);
        let [w, h] = bake.plan.atlas;
        let atlas = Arc::new(Mutex::new(vec![0u8; w as usize * h as usize * 4]));
        let (cell, ss) = (bake.plan.cell, bake.plan.supersample);
        BakeJob {
            bake,
            look,
            owner,
            session: session.to_owned(),
            call,
            target: None,
            target_size: (cell[0] * ss, cell[1] * ss),
            warm: false,
            next: 0,
            flying: None,
            landed: None,
            rerendered: BTreeMap::new(),
            released: false,
            resolving: Rc::default(),
            atlas,
            phase: Phase::Cells,
            renders: 0,
            uploads: 0,
            pin,
            engine_ms: 0.0,
            max_step_ms: 0.0,
            tap: None,
        }
    }

    /// The bake's target and pin go: once all its cells are in (it needs neither to encode and
    /// write), and on every way its job ends ([`StepJob::on_end`]); again, it does nothing. The
    /// target goes back to the engine's cache of offscreen targets, so a bake neither ends nor
    /// begins in the driver's freeing and making of a render graph's images (a target destroyed
    /// at a bake's end held the windowed iGPU 30-152 ms); it is destroyed only when the cache is
    /// full. A target whose render is still on the GPU (a bake dropped at its deadline, say)
    /// goes back once a later poll finds that render done ([`Engine::orphan_target`]), never
    /// waited for. The pin goes at once: no pin outlives its job.
    fn release(&mut self, e: &mut Engine) {
        if let Some(t) = self.target.take() {
            let _z = specula_core::profile::zone("sprite_bake.release");
            if self.flying.take().is_some() && e.gpu.offscreen_pending_target() == Some(t) {
                e.orphan_target(self.target_size, t);
            } else {
                e.give_back_target(self.target_size, t);
            }
        }
        self.flying = None;
        if !self.released {
            self.released = true;
            e.studio_unpin(self.pin);
        }
    }

    /// Tests: whether its target and pin were given back.
    #[cfg(test)]
    pub(crate) fn released(&self) -> bool {
        self.released && self.target.is_none()
    }

    /// Tests: the key of its pin on the model's build.
    #[cfg(test)]
    pub(crate) fn pin_key(&self) -> u64 {
        self.pin
    }

    /// Tests: where it is (`cells`, `encoding`, `held`, `committing`).
    #[cfg(test)]
    pub(crate) fn phase_name(&self) -> &'static str {
        match self.phase {
            Phase::Cells => "cells",
            Phase::Encoding(_) => "encoding",
            Phase::Held(..) => "held",
            Phase::Committing(..) => "committing",
        }
    }

    /// Tests: the encode on its worker has come back (the next step reads it).
    #[cfg(test)]
    pub(crate) fn encoded(&self) -> bool {
        matches!(&self.phase, Phase::Encoding(slot) if slot.borrow().is_some())
    }

    /// A target of `w` x `h` the bake holds alone while it runs: one of the engine's cached
    /// targets of that size, taken out of the cache (so no capture renders on it meanwhile),
    /// unless a capture's view is between its render and its landing (it may be on that target:
    /// a new one is made then); else a new one.
    fn acquire(e: &mut Engine, w: u32, h: u32) -> Result<TargetId, specula_gpu::GpuError> {
        let _z = specula_core::profile::zone("sprite_bake.target");
        if e.captures.flight.is_none() && e.captures.step_flight.is_none() {
            if let Some(i) = e.targets.iter().position(|(size, _)| *size == (w, h)) {
                return Ok(e.targets.remove(i).1);
            }
        }
        e.gpu.create_target(w, h)
    }

    fn offload(&self) -> Offload {
        Offload::agent(JobKind::SpriteBake, self.owner.clone(), &self.session, self.call).outlives_session()
    }

    /// The frame of cell `at` and its draws' ids, or `None` while the pipelines it is drawn with
    /// are still being built.
    fn submit(&mut self, e: &mut Engine, target: TargetId, at: usize) -> Result<bool, ToolOutput> {
        let plan = &self.bake.plan;
        let cell = &plan.cells[at];
        let strip = &plan.strips[cell.strip];
        let cam = &plan.cameras[strip.direction];
        let mut draws: Vec<DrawItem> = Vec::new();
        let mut bones = Vec::new();
        let pose = specula_core::profile::zone("sprite_bake.pose");
        e.models.refresh_ready(&mut e.gpu);
        let uploads = e.models.uploads();
        let posed = e.models.posed_draws(&mut e.gpu, &self.bake.built, &plan.poses[cell.pose], &Mat4::IDENTITY, &mut bones, &mut draws);
        self.uploads += e.models.uploads() - uploads;
        if let Err(m) = posed {
            if m.contains("still being built") {
                return Ok(false);
            }
            return Err(err("render_unavailable", m));
        }
        drop(pose);
        let ids = vec![1u32; draws.len()];
        let reach = cam.eye.length() * 2.0 + 2.0;
        let frame = FrameDesc {
            view_proj: cam.view_proj,
            clear_color: CLEAR,
            draws: &draws,
            reverse_z: true,
            environment: environment(&self.look.rig, cam.azimuth_deg, reach, &e.world.env().light.shadow),
            bones: &bones,
            time_s: 0.0,
            ..Default::default()
        };
        let rendered = {
            let _z = specula_core::profile::zone("sprite_bake.render");
            e.gpu.render_async(target, &frame, Some((&ids, &[])))
        };
        if let Err(g) = rendered {
            return Err(err("render_unavailable", crate::capture::gpu_failure("render", &g, e.windowed)));
        }
        Ok(true)
    }

    /// The landed cell `at` handed to a worker to resolve into the atlas.
    fn resolve_later(&mut self, e: &mut Engine, at: usize, raw: Raw) -> Result<(), crate::world::WorldError> {
        let plan = &self.bake.plan;
        let (cell, ss, rect, atlas_w) = (plan.cell, plan.supersample, plan.cells[at].rect, plan.atlas[0]);
        let (palette, outline, atlas) = (self.look.palette.clone(), self.look.outline, Arc::clone(&self.atlas));
        let state = Rc::clone(&self.resolving);
        let progress = Arc::clone(&self.bake.progress);
        let bytes = (raw.0.len() + raw.1.len() * 4) as u64;
        self.resolving.borrow_mut().in_flight += 1;
        let o = self.offload().bytes(bytes);
        let sent = e.offload(
            o,
            move |_| -> Resolved { resolve_cell(raw, at, cell, ss, palette.as_deref(), outline, &atlas, atlas_w, rect) },
            move |_, r| {
                let mut s = state.borrow_mut();
                s.in_flight -= 1;
                match r {
                    Ok(Ok(touching)) => {
                        s.done += 1;
                        progress.cells.store(s.done, Ordering::Relaxed);
                        if touching {
                            s.touching.push(at);
                        }
                    }
                    Ok(Err(out)) => s.failed = s.failed.take().or(Some(out)),
                    Err(f) => s.failed = s.failed.take().or(Some(f.to_output(JobKind::SpriteBake))),
                }
            },
        );
        if sent.is_err() {
            self.resolving.borrow_mut().in_flight -= 1;
        }
        sent
    }

    /// The atlas encoded, checked and stored on a worker (the shadow blob drawn in first).
    fn encode_later(&mut self, e: &mut Engine) -> Result<(), crate::world::WorldError> {
        let plan = self.bake.plan.clone();
        let atlas = Arc::clone(&self.atlas);
        let (pixel_art, png_path) = (self.look.palette.is_some(), self.bake.req.path.clone());
        let text = super::sheet_of(&plan, &self.bake.req, &self.bake.built, &self.look.light).to_text();
        let store = self.bake.store.clone();
        let stop = Arc::clone(&self.bake.stop);
        let (store_back, stop_back) = (store.clone(), Arc::clone(&stop));
        let slot = Rc::new(RefCell::new(None));
        let back = Rc::clone(&slot);
        let bytes = u64::from(plan.atlas[0]) * u64::from(plan.atlas[1]) * 4;
        let o = self.offload().bytes(bytes);
        e.offload(
            o,
            move |_| -> EncodeOut {
                let mut rgba = std::mem::take(&mut *atlas.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
                if let Some(sh) = &plan.shadow {
                    // The blob is drawn, not rendered: an ellipse on its own pivot.
                    let blob = plan::shadow_blob(plan.cell, sh.pivot, sh.radii_px, pixel_art);
                    plan::blit(&mut rgba, plan.atlas[0], sh.rect, &blob);
                }
                let png = crate::capture::encode_png(plan.atlas[0], plan.atlas[1], &rgba);
                if png.len() as u64 > crate::blobs::MAX_BLOB_BYTES {
                    return Err(err("too_large", format!("the atlas encoded to {} bytes; a binary project file holds at most {}: a smaller cell or fewer cells", png.len(), crate::blobs::MAX_BLOB_BYTES)));
                }
                if let Err(p) = crate::project_binary::inspect(&png_path, FileKind::Png, &png) {
                    return Err(err("invalid", format!("the atlas did not check as a PNG: {}", p.first().map_or(String::new(), |p| p.message.clone()))));
                }
                if let Err(e) = super::sheet::validate(&text) {
                    return Err(err("internal", format!("the frame data did not read back ({}: {}); nothing was written", e.at, e.message)));
                }
                let Some(store) = store else { return Err(err("no_content_store", "this engine has no content store (binary project files need one)")) };
                if stop.cancelled() {
                    // Cancelled while it encoded: nothing goes into the store.
                    return Ok(None);
                }
                let (blob, mark) = store.put_new(&png).map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
                Ok(Some(Encoded { pointer: crate::blobs::pointer(blob, png.len() as u64), png_bytes: png.len(), text, stored: mark.map(|m| (blob, m)) }))
            },
            move |e, r| {
                let mut r = r.unwrap_or_else(|f| Err(f.to_output(JobKind::SpriteBake)));
                // Cancelled (or dropped: `on_end` cancels) after the worker's own check: what it
                // stored goes back, and the bake reads it as cancelled before it stored.
                if stop_back.cancelled() {
                    if let Ok(Some(enc)) = &r {
                        withdraw_from(e, store_back, enc.stored);
                        r = Ok(None);
                    }
                }
                *back.borrow_mut() = Some(r);
            },
        )?;
        self.phase = Phase::Encoding(slot);
        Ok(())
    }

    /// The atlas is encoded and stored: the bake's writing begins, unless it was cancelled first
    /// (then what it stored is taken back and it answers `cancelled`). Whichever of the two came
    /// first holds ([`Stop`]): from here a cancel is refused `too_late`.
    fn write(&mut self, e: &mut Engine, enc: Encoded) -> Step {
        if self.bake.stop.cancelled() {
            return self.cancelled(e, enc.stored);
        }
        if e.clock.paused() && held_by_test(&self.bake.req.path, Hold::Encoded) {
            self.bake.progress.set_phase(super::PHASE_HELD);
            self.phase = Phase::Held(Hold::Encoded, enc);
            return Step::Wait;
        }
        if !self.bake.stop.begin_writing() {
            return self.cancelled(e, enc.stored);
        }
        self.bake.progress.set_phase(super::PHASE_WRITING);
        if e.clock.paused() && held_by_test(&self.bake.req.path, Hold::Writing) {
            self.phase = Phase::Held(Hold::Writing, enc);
            return Step::Wait;
        }
        self.commit_now(e, enc)
    }

    fn commit_now(&mut self, e: &mut Engine, enc: Encoded) -> Step {
        match self.commit(e, enc) {
            Some(out) => Step::Done(out),
            None => Step::More,
        }
    }

    /// Asks for the two files' commit, as the agent that asked for the bake: one journal entry,
    /// charged to the call that started the bake (one tool call, one commit).
    fn commit(&mut self, e: &mut Engine, enc: Encoded) -> Option<ToolOutput> {
        let png_path = self.bake.req.path.clone();
        let json_path = super::frames_path(&png_path);
        let files = vec![
            (png_path, self.bake.bases[0], (FileKind::Png, Arc::from(enc.pointer.as_str()))),
            (json_path, self.bake.bases[1], (FileKind::SpriteSheet, Arc::from(enc.text.as_str()))),
        ];
        let (tx, rx) = mpsc::channel();
        let (name, key) = (self.owner.name.clone(), self.owner.key.clone());
        let refused = e.run_charged(self.call, &name, |e| {
            let w = crate::journal::Writer::new(&name, Some(&key));
            e.project_call(ProjectCall::CommitFiles { files, reply: tx }, w, &name)
        });
        if refused.is_error {
            // (A project call refused before it reached the commit: no project here, say.)
            self.withdraw(e, enc.stored);
            return Some(refused);
        }
        self.phase = Phase::Committing(rx, enc);
        None
    }

    /// The answer once the commit is in.
    fn answer(&mut self, committed: Committed, enc: &Encoded) -> ToolOutput {
        let (png, json) = (self.bake.req.path.clone(), super::frames_path(&self.bake.req.path));
        let commit = match committed {
            Committed::Done(j) => j,
            Committed::Changed => return err("changed", format!("{png} or {json} changed while the sheet was baked; nothing was written: bake again")),
            Committed::Refused(e) => {
                let mut j = e.to_json();
                j.set("message", Json::from(format!("{} (the bake's two files were refused when it ended; nothing was written)", e.message)));
                return ToolOutput::error(j.to_string());
            }
        };
        let r = self.resolving.borrow();
        let cost = Cost { renders: self.renders, engine_ms: self.engine_ms, max_step_ms: self.max_step_ms, uploads: self.uploads };
        let data = super::sheet_of(&self.bake.plan, &self.bake.req, &self.bake.built, &self.look.light);
        let mut a = super::answer(&self.bake.plan, &data, &self.bake.req, Some((&cost, enc.png_bytes)), Some(commit), self.bake.started);
        let mut notes = std::mem::take(&mut self.look.notes);
        if !r.touching.is_empty() {
            let mut t = r.touching.clone();
            t.sort_unstable();
            notes.push(format!("{} cell(s) have the model on their outermost pixels (cut off there): cells {:?}", t.len(), t.iter().take(8).collect::<Vec<_>>()));
        }
        if !notes.is_empty() {
            a.set("warnings", Json::arr(notes.into_iter().map(Json::from)));
        }
        ToolOutput::text(a.to_string())
    }

    /// One step, timed by the caller.
    fn step_inner(&mut self, e: &mut Engine) -> Step {
        match &self.phase {
            Phase::Encoding(slot) => {
                let Some(r) = slot.borrow_mut().take() else { return Step::Wait };
                return match r {
                    Err(out) => Step::Done(out),
                    // Cancelled before it stored anything.
                    Ok(None) => self.cancelled(e, None),
                    Ok(Some(enc)) => self.write(e, enc),
                };
            }
            Phase::Held(at, _) => {
                let at = *at;
                // Held while the clock is paused; a cancel lets a bake held before its writing
                // go at once (to answer `cancelled`).
                if e.clock.paused() && !(at == Hold::Encoded && self.bake.stop.cancelled()) {
                    return Step::Wait;
                }
                let Phase::Held(_, enc) = std::mem::replace(&mut self.phase, Phase::Cells) else { unreachable!("matched above") };
                return match at {
                    Hold::Encoded => self.write(e, enc),
                    _ => self.commit_now(e, enc),
                };
            }
            Phase::Committing(rx, _) => {
                let got = match rx.try_recv() {
                    Ok(c) => c,
                    Err(mpsc::TryRecvError::Empty) => return Step::Wait,
                    Err(mpsc::TryRecvError::Disconnected) => return Step::Done(err("no_answer", "the engine dropped the bake's commit unanswered: project_list says whether its files are there")),
                };
                let Phase::Committing(_, enc) = std::mem::replace(&mut self.phase, Phase::Cells) else { unreachable!("matched above") };
                if !matches!(got, Committed::Done(_)) {
                    // Refused when it ended: nothing refers to what it stored.
                    self.withdraw(e, enc.stored);
                }
                return Step::Done(self.answer(got, &enc));
            }
            Phase::Cells => {}
        }
        let cells = self.bake.plan.cells.len();
        // A render on the GPU first: polled, never waited for; landed (read out of the readback)
        // in the same step its poll finds it done, so no later ID render can take its ids while
        // it waits for a worker slot.
        if let Some(at) = self.flying {
            let Some(target) = self.target else { unreachable!("a cell flies on the bake's target") };
            if e.gpu.offscreen_pending_target() == Some(target) {
                let polled = {
                    let _z = specula_core::profile::zone("sprite_bake.poll");
                    e.gpu.settle_offscreen_within(Some(Duration::ZERO))
                };
                match polled {
                    Ok(false) => return Step::Wait,
                    Ok(true) => {}
                    Err(g) => {
                        self.flying = None;
                        self.release(e);
                        return Step::Done(err("render_unavailable", crate::capture::gpu_failure("render", &g, e.windowed)));
                    }
                }
            } else if e.gpu.offscreen_pending() {
                // Another tool's render settled ours and is on the GPU now: its turn.
                return Step::Wait;
            }
            let landed = {
                let _z = specula_core::profile::zone("sprite_bake.land");
                e.gpu.read_landed(target)
            };
            match landed {
                Ok(None) => return Step::Wait,
                Ok(Some((colour, ids))) => {
                    self.flying = None;
                    let (rw, rh) = (self.bake.plan.cell[0] * self.bake.plan.supersample, self.bake.plan.cell[1] * self.bake.plan.supersample);
                    if (ids.width, ids.height) != (rw, rh) || colour.len() != (rw * rh * 4) as usize {
                        self.release(e);
                        return Step::Done(err("render_unavailable", format!("the render came back {}x{} with {} colour bytes, not {rw}x{rh}", ids.width, ids.height, colour.len())));
                    }
                    if let Some(t) = &self.tap {
                        t.borrow_mut().push((at, (colour.clone(), ids.ids.clone())));
                    }
                    self.landed = Some((at, (colour, ids.ids)));
                }
                Err(_) => {
                    // Another target's ID render came after ours (its ids are gone): this
                    // cell is rendered again, so every cell is its own render exactly; a cell
                    // that keeps losing its ids so is given up at a bound, said.
                    self.flying = None;
                    let n = {
                        let n = self.rerendered.entry(at).or_insert(0);
                        *n += 1;
                        *n
                    };
                    if n > MAX_RERENDERS_PER_CELL {
                        self.release(e);
                        return Step::Done(err(
                            "render_contended",
                            format!("cell {at} was rendered {n} times and each time another render's ID buffer took its own before it landed (captures of ids running all the while); nothing was written: bake again when they have ended"),
                        ));
                    }
                    self.next = self.next.min(at);
                    return Step::More;
                }
            }
        }
        if self.bake.stop.cancelled() {
            return self.cancelled(e, None);
        }
        // A landed cell to its worker, once there is a slot (it waits in memory meanwhile).
        if let Some((at, _)) = &self.landed {
            let at = *at;
            let bytes = u64::from(self.bake.plan.cell[0] * self.bake.plan.supersample) * u64::from(self.bake.plan.cell[1] * self.bake.plan.supersample) * 8;
            if self.resolving.borrow().in_flight >= MAX_RESOLVING || e.workers_room(&self.owner, self.call, bytes).is_err() {
                return Step::Wait;
            }
            let Some((_, raw)) = self.landed.take() else { unreachable!("looked at above") };
            if self.resolve_later(e, at, raw).is_err() {
                // (Its room was asked for just now: only a race with another of the owner's
                // jobs gets here.) Rendered again when there is room.
                self.next = self.next.min(at);
            }
            return Step::More;
        }
        let failed = self.resolving.borrow_mut().failed.take();
        if let Some(out) = failed {
            self.release(e);
            return Step::Done(out);
        }
        if self.next < cells {
            if self.next == 0 && e.clock.paused() && held_by_test(&self.bake.req.path, Hold::Start) {
                self.bake.progress.set_phase(super::PHASE_HELD);
                return Step::Wait;
            }
            // Another tool's render on the GPU is never waited for: ours goes after it.
            if e.gpu.offscreen_pending() {
                return Step::Wait;
            }
            let plan = &self.bake.plan;
            let (rw, rh) = (plan.cell[0] * plan.supersample, plan.cell[1] * plan.supersample);
            let target = match self.target {
                Some(t) => t,
                None => match Self::acquire(e, rw, rh) {
                    Ok(t) => *self.target.insert(t),
                    Err(g) => {
                        self.release(e);
                        return Step::Done(err("render_unavailable", crate::capture::gpu_failure("create target", &g, e.windowed)));
                    }
                },
            };
            if !self.warm {
                // The target's graph images (and ID buffers) made a slice a step before its first
                // render, so no render makes them inside its submit (169 ms on the windowed iGPU).
                let cam = &plan.cameras[plan.strips[plan.cells[0].strip].direction];
                let env = environment(&self.look.rig, cam.azimuth_deg, cam.eye.length() * 2.0 + 2.0, &e.world.env().light.shadow);
                match e.gpu.prewarm_target(target, &env, true, None, Instant::now() + crate::capture::flight::PREWARM_SLICE) {
                    Ok(true) => self.warm = true,
                    Ok(false) => return Step::More,
                    // A prewarm that fails is the render's to say (it makes what is missing).
                    Err(_) => self.warm = true,
                }
            }
            let at = self.next;
            let submitted = {
                let _z = specula_core::profile::zone("sprite_bake.submit");
                self.submit(e, target, at)
            };
            return match submitted {
                Ok(true) => {
                    self.bake.progress.set_phase(super::PHASE_RENDERING);
                    self.flying = Some(at);
                    self.next += 1;
                    self.renders += 1;
                    Step::More
                }
                // Its pipelines are still being built: looked at again a pass later.
                Ok(false) => Step::Wait,
                Err(out) => {
                    self.release(e);
                    Step::Done(out)
                }
            };
        }
        if self.resolving.borrow().in_flight > 0 {
            return Step::Wait;
        }
        // Every cell resolved into the atlas.
        self.release(e);
        if self.look.outline.is_some() && !self.resolving.borrow().touching.is_empty() {
            let c = self.bake.plan.cell;
            return Step::Done(err(
                "no_room_for_outline",
                format!("the model reaches a cell's outermost pixels, where an outline cannot be drawn; nothing was written: a cell of at least {}x{}", c[0] + 2 * plan::MARGIN_PX, c[1] + 2 * plan::MARGIN_PX),
            ));
        }
        self.bake.progress.set_phase(super::PHASE_ENCODING);
        match self.encode_later(e) {
            Ok(()) => Step::More,
            // No room on the workers this pass: tried again on a later one.
            Err(_) => Step::Wait,
        }
    }

    /// Tests: every landed cell's render goes here too.
    #[cfg(test)]
    pub(crate) fn tap(&mut self) -> Tap {
        Rc::clone(self.tap.get_or_insert_with(Rc::default))
    }

    /// Tests: the atlas once every cell is resolved into it, before the encode (`None` before).
    #[cfg(test)]
    pub(crate) fn rendered(&self) -> Option<Baked> {
        let r = self.resolving.borrow();
        let all = matches!(self.phase, Phase::Cells) && self.next == self.bake.plan.cells.len() && self.flying.is_none() && self.landed.is_none() && r.in_flight == 0 && r.failed.is_none();
        all.then(|| Baked { rgba: self.atlas.lock().unwrap().clone(), touching: r.touching.clone(), renders: self.renders })
    }

    /// Content the encode put in the store that nothing will refer to now (a cancel, a refused
    /// commit) is taken back, on a worker; a worker with no room leaves it to the store's own
    /// pruning (nothing refers to it, so it is the first to go when the disk needs room).
    fn withdraw(&self, e: &mut Engine, stored: Option<(crate::blobs::BlobId, u64)>) {
        withdraw_from(e, self.bake.store.clone(), stored);
    }

    /// Cancelled (by its owner, or a lead's forced cancel, logged on the board here): what it
    /// holds goes back, what it stored is taken back, nothing is written.
    fn cancelled(&mut self, e: &mut Engine, stored: Option<(crate::blobs::BlobId, u64)>) -> Step {
        self.release(e);
        self.withdraw(e, stored);
        let (done, cells) = (self.resolving.borrow().done, self.bake.plan.cells.len());
        let by = match self.bake.stop.forced_by() {
            Some(sid) => {
                let lead = e.swarm.name_of(&sid).unwrap_or_else(|| sid.clone());
                let lead = lead.as_str();
                let tick = e.clock.tick();
                e.swarm.board.log(lead, tick, "sprite_bake", None, format!("forced cancel of {}'s bake of {} after {done} of {cells} cells", self.owner.name, self.bake.req.path));
                format!("by {lead} (a forced cancel, logged on the board)")
            }
            None => "by its owner".to_owned(),
        };
        Step::Done(err("cancelled", format!("the bake was cancelled {by} after {done} of {cells} cells; nothing was written")))
    }
}

/// [`BakeJob::withdraw`] from `store`: the content goes back on a worker.
fn withdraw_from(e: &mut Engine, store: Option<Arc<crate::blobs::ContentStore>>, stored: Option<(crate::blobs::BlobId, u64)>) {
    if let (Some((id, mark)), Some(store)) = (stored, store) {
        let _ = e.offload(Offload::engine(JobKind::SpriteBake), move |_| store.withdraw(id, mark), |_, _| {});
    }
}

/// One landed cell resolved and put into the atlas (on a worker): alpha from the ID buffer, the
/// supersampled colour, pixel art's palette and outline, the colour bleed.
#[allow(clippy::too_many_arguments)]
fn resolve_cell(raw: Raw, at: usize, cell: [u32; 2], ss: u32, palette: Option<&[[u8; 3]]>, outline: Option<[u8; 3]>, atlas: &Mutex<Vec<u8>>, atlas_w: u32, rect: [u32; 4]) -> Resolved {
    let (colour, ids) = raw;
    let mut px = plan::resolve(&colour, &ids, cell, ss);
    if !px.as_chunks::<4>().0.iter().any(|p| p[3] > 0) {
        // The fit holds every vertex in view: an empty cell is a missing ID buffer (its
        // pipelines not ready), never a picture to write.
        return Err(err("ids_missing", format!("cell {at} came back with no pixel of the model in the ID buffer; nothing was written: call again in a moment")));
    }
    if let Some(p) = palette {
        plan::snap_to_palette(&mut px, p);
    }
    if let Some(c) = outline {
        // One pixel round the silhouette, never on the cell's outermost pixels.
        plan::outline(&mut px, cell, c, plan::MARGIN_PX - 1);
    }
    let touching = plan::touches_edge(&px, cell);
    plan::bleed(&mut px, cell, plan::BLEED_PX);
    plan::blit(&mut atlas.lock().unwrap_or_else(std::sync::PoisonError::into_inner), atlas_w, rect, &px);
    Ok(touching)
}

impl StepJob for BakeJob {
    fn kind(&self) -> &'static str {
        "sprite_bake"
    }

    fn on_gpu(&self) -> bool {
        self.flying.is_some()
    }

    fn on_end(&mut self, e: &mut Engine, how: Ended) {
        self.release(e);
        if how != Ended::Answered {
            // Dropped mid-bake: what an encode stored and nothing will refer to goes back. The
            // stop is set first (review 2, M2), so an encode still on a worker stores nothing, or
            // has what it stored taken back as it returns; one already returned is taken back
            // here.
            let _ = self.bake.stop.cancel(None);
            let stored = match &self.phase {
                Phase::Held(_, enc) => enc.stored,
                Phase::Encoding(slot) => slot.borrow().as_ref().and_then(|r| r.as_ref().ok()).and_then(Option::as_ref).and_then(|enc| enc.stored),
                _ => None,
            };
            self.withdraw(e, stored);
        }
    }

    fn step(&mut self, e: &mut Engine, _reply: &jobs::Reply) -> Step {
        let started = Instant::now();
        if !self.released {
            // The pin again (a map lookup when it is there): whatever else took it, the next
            // cell does not upload the model again.
            e.pin_for_kept_job(self.pin, self.bake.built.id);
        }
        let r = self.step_inner(e);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        self.engine_ms += ms;
        self.max_step_ms = self.max_step_ms.max(ms);
        r
    }
}

/// The style files beside the build, read on a worker: the rig, the palette (pixel art) and
/// what the light is called (`project` falls back to `neutral`, said).
pub(crate) fn read_look(root: &std::path::Path, style: &std::path::Path, req: &Request) -> Result<Look, ToolOutput> {
    let (style_json, style_toml) = crate::studio::bible_files(root, style, &req.spec.recipe);
    let style = style_json.as_ref().and_then(|p| crate::project::read_text(p).ok().map(|(t, _)| t)).and_then(|t| specula_model::Style::parse(&t).ok());
    let mut notes = Vec::new();
    let palette: Vec<[u8; 3]> = style.as_ref().map_or_else(Vec::new, |s| s.palette.values().map(|c| [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8)).collect());
    let palette = if req.pixel_art {
        if palette.is_empty() {
            return Err(err("no_palette", format!("pixel_art snaps colours to the project's palette, and {} has none (no style.json with a palette beside it or above it)", req.spec.recipe)));
        }
        Some(palette)
    } else {
        None
    };
    let (rig, light) = match req.light {
        super::Light::Neutral => (specula_studio::rules::neutral_rig(), "neutral".to_owned()),
        super::Light::Project => {
            let parsed = style_toml.as_ref().and_then(|p| crate::project::read_text(p).ok().map(|(t, _)| t).map(|t| (p, t))).map(|(p, t)| {
                let lookup = |name: &str| style.as_ref().and_then(|s| s.palette.get(name)).map(|c| [c[0], c[1], c[2]]);
                specula_studio::rules::StyleRules::parse(&t, &p.display().to_string(), &lookup)
            });
            match parsed {
                Some(Ok(rules)) => (rules.stage.game.clone(), "project: style.toml [stage] game rig".to_owned()),
                Some(Err(e)) => return Err(err("style_invalid", format!("the project's light is its style.toml's [stage] rig, and the style.toml does not parse: {}", e.message))),
                None => {
                    notes.push("light: no style.toml beside the recipe or above it, so the neutral rig lit it".to_owned());
                    (specula_studio::rules::neutral_rig(), "neutral (no style.toml)".to_owned())
                }
            }
        }
    };
    let outline = if req.outline { palette.as_deref().and_then(plan::darkest) } else { None };
    Ok(Look { rig, light, palette, outline, notes })
}

impl Engine {
    /// `sprite_bake` on the engine thread: the look read on a worker, then (unless a dry run)
    /// the bake's step job, kept for its owner's identity. The request is answered at once; the
    /// bake's own answer goes to its [`Bake::done`].
    pub(crate) fn start_sprite_bake(&mut self, request: crate::tools::Request) {
        let crate::tools::Request { command: Command::SpriteBake(bake), session, reply, call, .. } = request else { unreachable!("only sprite bakes come here") };
        let owner = JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        let _ = bake.owner_name.set(owner.name.clone());
        let (root, style, req) = (bake.assets.clone(), bake.built.style.clone(), bake.req.clone());
        let o = Offload::agent(JobKind::SpriteBake, owner.clone(), &session.id, call).outlives_session();
        let session_id = session.id.clone();
        let started = self.offload(
            o,
            move |_| read_look(&root, &style, &req),
            move |e, r| {
                let bake = *bake;
                let look = match r {
                    Ok(Ok(look)) => look,
                    Ok(Err(out)) => {
                        let _ = bake.done.send(out);
                        return;
                    }
                    Err(f) => {
                        let _ = bake.done.send(f.to_output(JobKind::SpriteBake));
                        return;
                    }
                };
                if bake.req.dry_run {
                    let data = super::sheet_of(&bake.plan, &bake.req, &bake.built, &look.light);
                    let mut a = super::answer(&bake.plan, &data, &bake.req, None, None, bake.started);
                    a.set("dry_run", Json::Bool(true));
                    a.set("note", Json::from("nothing was rendered or written: call again without dry_run"));
                    let _ = bake.done.send(ToolOutput::text(a.to_string()));
                    return;
                }
                let done = bake.done.clone();
                let job = BakeJob::new(e, bake, look, owner.clone(), &session_id, call);
                e.step_job_kept(StepStart { owner, session: session_id, call, reply: done, deadline: Instant::now() + MAX_BAKE, class: crate::schedule::Class::Render }, Box::new(job));
            },
        );
        match started {
            Ok(()) => {
                let _ = reply.send(ToolOutput::text("{}"));
            }
            Err(e) => {
                reply.refuse(&e);
            }
        }
    }
}
