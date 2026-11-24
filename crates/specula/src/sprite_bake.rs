//! `sprite_bake`: a model and its clips baked into a sprite sheet (docs/design/SPRITE_BAKE.md),
//! so a 2D game and a 3D game take the same creature from the same source.
//!
//! On the caller's connection thread: the arguments (each bounded), the model built as an entity
//! would build it (lent here, as `model_export` borrows it), the plan (clips, frames, directions,
//! one orthographic scale and one pivot fitted to every posed vertex, the atlas's layout) and every
//! refusal a plan can make, before the engine renders anything. Then the bake is a job of the
//! caller's identity ([`job`]): its style files read, its cells rendered without waiting on the
//! GPU, resolved, encoded and stored on workers, and its two files committed in one journal entry
//! by the engine when it ends. The call waits for it up to `wait_s`; a bake still running then is
//! answered with its job, and `sprite_bake {job}` gives its progress or its kept result.

use crate::models::BuiltSlot;
use crate::project::FileKind;
use crate::tools::{bool_arg, check_keys, int_arg, params_arg, string_arg, Command, Tools};
use crate::world::{self, ModelSpec};
use plan::{Ask, Clips, Rate, View};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(crate) mod job;
pub mod plan;
pub mod sheet;

/// Longest `recipe`, `path` and clip name.
pub const MAX_PATH_CHARS: usize = 256;
pub const MAX_NAME_CHARS: usize = 64;
/// The folder sheets go to when no path is given.
pub const FOLDER: &str = "assets/sprites";
/// Strips the answer lists (the file has them all).
const MAX_LISTED: usize = 16;
/// The most bakes one agent runs at once (each holds an atlas of up to 28 MiB).
pub const MAX_RUNNING_PER_OWNER: usize = 2;
/// How long a call waits for its bake before answering with the job (`wait_s`): the default
/// and the most (under the engine's 25 s answer deadline).
pub const WAIT_S: u64 = 20;
/// A finished bake's result is kept for its owner this long, at most so many an owner.
pub const KEPT_FOR: Duration = Duration::from_secs(30 * 60);
pub const MAX_KEPT_PER_OWNER: usize = 8;

fn err(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

/// Which light a bake is lit by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    /// The studio's neutral rig (white key from the upper left, grey ambient).
    Neutral,
    /// The project's: its `style.toml` `[stage]` game rig (the studio's "game" light).
    Project,
}

/// What `sprite_bake` was asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub spec: ModelSpec,
    pub ask: Ask,
    pub view: View,
    pub pixel_art: bool,
    /// Pixel art's outline: the palette's darkest colour round the silhouette.
    pub outline: bool,
    pub light: Light,
    /// The atlas's project path; the frame data is beside it (`.json`).
    pub path: String,
    pub dry_run: bool,
    /// How long the call waits for the bake before answering with its job (seconds).
    pub wait_s: u64,
}

/// A parsed call: a bake; the caller's own bakes cancelled (all, or one job); or a bake's
/// progress or kept result asked for (waiting up to `wait_s` for it to end).
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Bake(Box<Request>),
    Cancel(Option<u64>),
    Job { id: u64, wait_s: u64 },
}

/// The frame data's path for an atlas path.
pub fn frames_path(png: &str) -> String {
    format!("{}.json", png.strip_suffix(".png").unwrap_or(png))
}

fn wait_of(args: &Json, default: u64) -> Result<u64, String> {
    Ok(int_arg(args, "wait_s", 0, WAIT_S)?.unwrap_or(default))
}

/// Reads the arguments (every one bounded: AGENTS rule 15).
pub fn parse(args: &Json) -> Result<Call, String> {
    check_keys(args, "sprite_bake", &[&["recipe", "params", "seed", "clips", "directions", "view", "elevation_deg", "frames", "fps", "cell", "supersample", "pixel_art", "outline", "in_place", "shadow", "light", "path", "dry_run", "wait_s", "cancel", "job"]])?;
    let job = int_arg(args, "job", 1, u64::MAX >> 11)?;
    let n = args.as_object().map_or(0, |o| o.len());
    if bool_arg(args, "cancel")? == Some(true) {
        if n > 1 + usize::from(job.is_some()) {
            return Err("'cancel' stands alone, or with 'job': {\"cancel\": true} stops your own bakes, {\"cancel\": true, \"job\": N} that one".into());
        }
        return Ok(Call::Cancel(job));
    }
    if let Some(id) = job {
        if n > 1 + usize::from(args.get("wait_s").is_some()) {
            return Err("'job' asks for a bake's progress or result, with 'wait_s' at most: {\"job\": N, \"wait_s\": 20}".into());
        }
        return Ok(Call::Job { id, wait_s: wait_of(args, 0)? });
    }
    let recipe = string_arg(args, "recipe")?.ok_or("'recipe' is required: a recipe (or .glb) under the assets root, e.g. \"recipes/shambler.json\"")?;
    if recipe.chars().count() > MAX_PATH_CHARS {
        return Err(format!("'recipe' is at most {MAX_PATH_CHARS} characters"));
    }
    let spec = ModelSpec { recipe: recipe.to_owned(), params: params_arg(args)?.unwrap_or_default(), seed: int_arg(args, "seed", 0, world::MAX_MODEL_SEED)? };
    spec.check().map_err(|e| e.message)?;
    let clips = match args.get("clips") {
        None | Some(Json::Null) => Clips::Still,
        Some(Json::Str(s)) if s == "all" => Clips::All,
        Some(Json::Arr(a)) if a.is_empty() => Clips::Still,
        Some(Json::Arr(a)) if a.len() <= plan::MAX_CLIPS => {
            let mut names = Vec::new();
            for c in a {
                let n = c.as_str().filter(|n| !n.is_empty() && n.chars().count() <= MAX_NAME_CHARS).ok_or(format!("'clips' names clips (1 to {MAX_NAME_CHARS} characters each)"))?;
                if !names.iter().any(|m: &String| m == n) {
                    names.push(n.to_owned());
                }
            }
            Clips::Named(names)
        }
        Some(_) => return Err(format!("'clips' is a list of up to {} clip names, or \"all\" (left out: a still)", plan::MAX_CLIPS)),
    };
    let view = match string_arg(args, "view")? {
        None => View::ThreeQuarter,
        Some(v) => View::ALL.iter().find(|x| x.0 == v).map(|x| x.1).ok_or_else(|| format!("'view' is one of side, three_quarter, top (not '{v}')"))?,
    };
    let elevation_deg = match args.get("elevation_deg") {
        None | Some(Json::Null) => view.elevation_deg(),
        Some(v) => v.as_f64().filter(|e| (0.0..=90.0).contains(e)).ok_or("'elevation_deg' is 0 (level) to 90 (straight down)")? as f32,
    };
    let directions = match args.get("directions") {
        None | Some(Json::Null) => plan::directions_around(if view == View::Side { 1 } else { 8 }),
        Some(Json::Arr(a)) if !a.is_empty() && a.len() <= plan::MAX_DIRECTIONS => {
            let mut out: Vec<plan::Direction> = Vec::new();
            for f in a {
                let deg = f.as_f64().filter(|d| (0.0..360.0).contains(d)).ok_or("'directions' as a list holds facings in degrees, 0 to under 360 (0 faces the viewer, 90 faces right)")?;
                let d = plan::direction_at(deg as f32);
                if out.iter().any(|o| o.name == d.name) {
                    return Err(format!("'directions': {} is given twice", d.name));
                }
                out.push(d);
            }
            out
        }
        Some(v) => match v.as_u64() {
            Some(n @ (1 | 4 | 8 | 16)) => plan::directions_around(n as u32),
            _ => return Err(format!("'directions' is 1, 4, 8 or 16 (evenly round), or a list of up to {} facings in degrees", plan::MAX_DIRECTIONS)),
        },
    };
    let rate = match (args.get("frames").filter(|v| !v.is_null()), args.get("fps").filter(|v| !v.is_null())) {
        (Some(_), Some(_)) => return Err("give 'frames' (per clip) or 'fps', not both".into()),
        (Some(_), None) => Rate::Frames(int_arg(args, "frames", 1, u64::from(plan::MAX_FRAMES_PER_CLIP))?.unwrap_or(1) as u32),
        (None, Some(v)) => Rate::Fps(v.as_f64().filter(|f| *f >= 1.0 && *f <= f64::from(plan::MAX_FPS)).ok_or(format!("'fps' is 1 to {}", plan::MAX_FPS))? as f32),
        (None, None) => Rate::Fps(12.0),
    };
    let pixel_art = bool_arg(args, "pixel_art")?.unwrap_or(false);
    let side = |v: &Json| v.as_u64().filter(|n| (u64::from(plan::MIN_CELL_PX)..=u64::from(plan::MAX_CELL_PX)).contains(n)).map(|n| n as u32);
    let cell_err = || format!("'cell' is the cell's side in pixels, or [width, height], each {} to {}", plan::MIN_CELL_PX, plan::MAX_CELL_PX);
    let cell = match args.get("cell") {
        None | Some(Json::Null) => {
            let d = if pixel_art { 64 } else { 128 };
            [d, d]
        }
        Some(Json::Arr(a)) if a.len() == 2 => [side(&a[0]).ok_or_else(cell_err)?, side(&a[1]).ok_or_else(cell_err)?],
        Some(v) => {
            let s = side(v).ok_or_else(cell_err)?;
            [s, s]
        }
    };
    let supersample = match int_arg(args, "supersample", 1, 4)? {
        None => {
            if pixel_art {
                1
            } else {
                2
            }
        }
        Some(s @ (1 | 2 | 4)) => s as u32,
        Some(_) => return Err("'supersample' is 1, 2 or 4 (samples a side of each pixel)".into()),
    };
    if cell[0].max(cell[1]) * supersample > plan::MAX_RENDER_SIDE {
        return Err(format!("a cell of {}x{} at supersample {supersample} renders {}x{}; at most {} a side (a smaller cell or supersample)", cell[0], cell[1], cell[0] * supersample, cell[1] * supersample, plan::MAX_RENDER_SIDE));
    }
    let light = match string_arg(args, "light")? {
        None | Some("neutral") => Light::Neutral,
        Some("project") => Light::Project,
        Some(other) => return Err(format!("'light' is \"neutral\" or \"project\" (the style.toml's [stage] game rig), not '{other}'")),
    };
    let path = match string_arg(args, "path")? {
        Some(p) if p.chars().count() > MAX_PATH_CHARS => return Err(format!("'path' is at most {MAX_PATH_CHARS} characters")),
        Some(p) => p.to_owned(),
        None => format!("{FOLDER}/{}.png", crate::model_export::stem_of(recipe)),
    };
    let in_sprites = path.split('/').rev().skip(1).any(|s| s == "sprites");
    if !path.starts_with("assets/") || !path.ends_with(".png") || !in_sprites {
        return Err(format!("'path' is where the atlas goes: a .png in a folder named sprites under assets/, e.g. \"{FOLDER}/wolf.png\" (its frame data goes beside it, .json); got '{path}'"));
    }
    crate::project::check_path(&path).map_err(|e| e.message)?;
    let dry_run = bool_arg(args, "dry_run")?.unwrap_or(false);
    let outline = bool_arg(args, "outline")?.unwrap_or(false);
    if outline && !pixel_art {
        return Err("'outline' draws one palette colour round a pixel-art silhouette: give it with pixel_art: true".into());
    }
    let in_place = bool_arg(args, "in_place")?.unwrap_or(false);
    let shadow_blob = match string_arg(args, "shadow")? {
        None | Some("none") => false,
        Some("blob") => true,
        Some(other) => return Err(format!("'shadow' is \"none\" (default: the 2D game draws its own) or \"blob\" (a soft alpha-only blob in a cell of its own), not '{other}'")),
    };
    let ask = Ask { clips, directions, elevation_deg, rate, cell, supersample, in_place, shadow_blob };
    Ok(Call::Bake(Box::new(Request { spec, ask, view, pixel_art, outline, light, path, dry_run, wait_s: wait_of(args, WAIT_S)? })))
}

// ---------------------------------------------------------------------------------------------
// Bakes as jobs: who owns each, its progress, and its one answer kept for its owner

/// Where a bake is (its progress's `phase`).
pub const PHASE_RENDERING: u8 = 1;
pub const PHASE_ENCODING: u8 = 2;
pub const PHASE_WRITING: u8 = 3;
pub const PHASE_HELD: u8 = 4;
const PHASES: [&str; 5] = ["reading its style files", "rendering", "encoding", "writing its two files", "held before its first render (SPECULA_TEST_BAKE_HOLD: a test's gate)"];

/// A bake's progress, written by the engine and read by its owner's `{job}` calls.
#[derive(Default)]
pub struct Progress {
    /// Cells rendered and resolved into the atlas.
    pub cells: AtomicUsize,
    phase: AtomicU8,
}

impl Progress {
    pub fn set_phase(&self, p: u8) {
        self.phase.store(p, Ordering::Relaxed);
    }

    pub fn phase(&self) -> &'static str {
        PHASES.get(usize::from(self.phase.load(Ordering::Relaxed))).copied().unwrap_or("?")
    }
}

/// One bake: whose, what it writes, how far it got, and its answer.
struct Entry {
    /// The owner's identity key, and name as the engine knows it (set when its job starts).
    owner: String,
    name: Arc<OnceLock<String>>,
    path: String,
    cells: usize,
    cancel: Arc<AtomicBool>,
    progress: Arc<Progress>,
    started: Instant,
    /// Where its answer comes (`None` while its call waits on it, or once it came).
    answer: Option<mpsc::Receiver<ToolOutput>>,
    /// Its answer once it came, kept for its owner, and when it came.
    result: Option<(ToolOutput, Instant)>,
}

impl Entry {
    fn running(&self) -> bool {
        self.result.is_none()
    }

    fn name(&self) -> String {
        self.name.get().cloned().unwrap_or_else(|| "an agent whose bake is starting".to_owned())
    }
}

/// A bake started: its id, its cancel flag, its owner's name cell and its progress.
pub struct Started {
    pub id: u64,
    pub cancel: Arc<AtomicBool>,
    pub name: Arc<OnceLock<String>>,
    pub progress: Arc<Progress>,
}

/// Every bake, by id (docs/design/AUTHORITY.md, "Jobs"): a busy atlas refused naming its owner,
/// cancel by the owner only, progress and the kept result for the owner only.
#[derive(Default)]
pub struct Running {
    next: AtomicU64,
    by: Mutex<BTreeMap<u64, Entry>>,
}

fn not_owner(id: u64, owner: &str, what: &str) -> ToolOutput {
    ToolOutput::error(
        Json::obj([
            ("error", Json::from("not_job_owner")),
            ("message", Json::from(format!("bake {id} is {owner}'s, and only a bake's owner {what} it"))),
            ("job", Json::from(id)),
            ("owner", Json::from(owner)),
        ])
        .to_string(),
    )
}

fn dropped() -> ToolOutput {
    err("reply_dropped", "the engine dropped the bake without answering (shutting down): nothing of it was written")
}

fn unknown(id: u64) -> ToolOutput {
    err("unknown_job", format!("there is no bake {id} (a finished one's result is kept {} minutes for its owner)", KEPT_FOR.as_secs() / 60))
}

impl Running {
    fn locked(&self) -> std::sync::MutexGuard<'_, BTreeMap<u64, Entry>> {
        self.by.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Answers that came for bakes no call waits on are taken in; results kept past
    /// [`KEPT_FOR`] go, and an owner's oldest past [`MAX_KEPT_PER_OWNER`].
    fn tidy(by: &mut BTreeMap<u64, Entry>) {
        for e in by.values_mut().filter(|e| e.running()) {
            let got = match e.answer.as_ref().map(mpsc::Receiver::try_recv) {
                Some(Ok(out)) => Some(out),
                Some(Err(mpsc::TryRecvError::Disconnected)) => Some(dropped()),
                _ => None,
            };
            if let Some(out) = got {
                e.answer = None;
                e.result = Some((out, Instant::now()));
            }
        }
        let now = Instant::now();
        by.retain(|_, e| e.result.as_ref().is_none_or(|(_, at)| now.duration_since(*at) < KEPT_FOR));
        let mut kept: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
        for (id, e) in by.iter().filter(|(_, e)| !e.running()) {
            kept.entry(e.owner.as_str()).or_default().push(*id);
        }
        let gone: Vec<u64> = kept.values().flat_map(|ids| ids.iter().rev().skip(MAX_KEPT_PER_OWNER).copied()).collect();
        for id in gone {
            by.remove(&id);
        }
    }

    /// A new bake of `owner`'s writing `path` (`cells` of them): refused while the owner runs
    /// [`MAX_RUNNING_PER_OWNER`], or while another bake writes the same atlas (`job_busy`,
    /// naming that bake's owner and job).
    pub fn start(&self, owner: &str, path: &str, cells: usize) -> Result<Started, ToolOutput> {
        let mut by = self.locked();
        Self::tidy(&mut by);
        if let Some((id, e)) = by.iter().find(|(_, e)| e.running() && e.path == path) {
            let name = e.name();
            return Err(ToolOutput::error(
                Json::obj([
                    ("error", Json::from("job_busy")),
                    ("message", Json::from(format!("{name}'s bake (job {id}) is writing {path}; one bake writes an atlas at a time: wait for it to answer, or ask {name}"))),
                    ("job", Json::from(*id)),
                    ("owner", Json::from(name.as_str())),
                ])
                .to_string(),
            ));
        }
        let mine = by.values().filter(|e| e.running() && e.owner == owner).count();
        if mine >= MAX_RUNNING_PER_OWNER {
            return Err(err("job_busy", format!("you already run {mine} bakes (at most {MAX_RUNNING_PER_OWNER}): wait for one to answer (sprite_bake {{job}}), or sprite_bake {{cancel: true}}")));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let s = Started { id, cancel: Arc::default(), name: Arc::default(), progress: Arc::default() };
        let e = Entry { owner: owner.to_owned(), name: Arc::clone(&s.name), path: path.to_owned(), cells, cancel: Arc::clone(&s.cancel), progress: Arc::clone(&s.progress), started: Instant::now(), answer: None, result: None };
        by.insert(id, e);
        Ok(s)
    }

    /// The call that started bake `id` answered it itself: nothing is kept.
    pub fn end(&self, id: u64) {
        self.locked().remove(&id);
    }

    /// The call that started bake `id` stopped waiting: its answer is kept for its owner.
    pub fn park(&self, id: u64, answer: mpsc::Receiver<ToolOutput>) {
        if let Some(e) = self.locked().get_mut(&id) {
            e.answer = Some(answer);
        }
    }

    /// Cancels `owner`'s bakes (all, or `job`): how many. Another's bake is refused
    /// (`not_job_owner`, naming its owner); with no `job`, when the caller runs none and others
    /// do, the refusal names their owners (nothing is cancelled).
    pub fn cancel(&self, owner: &str, job: Option<u64>) -> Result<usize, ToolOutput> {
        let mut by = self.locked();
        Self::tidy(&mut by);
        if let Some(id) = job {
            return match by.get(&id) {
                None => Err(unknown(id)),
                Some(e) if e.owner != owner => Err(not_owner(id, &e.name(), "cancels")),
                Some(e) if !e.running() => Ok(0),
                Some(e) => {
                    e.cancel.store(true, Ordering::Relaxed);
                    Ok(1)
                }
            };
        }
        let mine = by.values().filter(|e| e.running() && e.owner == owner).map(|e| e.cancel.store(true, Ordering::Relaxed)).count();
        let mut others: Vec<String> = by.values().filter(|e| e.running() && e.owner != owner).map(Entry::name).collect();
        if mine == 0 && !others.is_empty() {
            others.sort();
            others.dedup();
            return Err(ToolOutput::error(
                Json::obj([
                    ("error", Json::from("not_job_owner")),
                    ("message", Json::from(format!("you run no bake; the bakes running are {}'s, and only a bake's owner cancels it: nothing was cancelled", others.join(", ")))),
                    ("owners", Json::arr(others.into_iter().map(Json::from))),
                ])
                .to_string(),
            ));
        }
        Ok(mine)
    }

    /// Bake `id`'s progress, or its kept result, for its owner only; waiting up to `wait` for a
    /// bake still running to end.
    pub fn ask(&self, owner: &str, id: u64, wait: Duration) -> ToolOutput {
        let taken = {
            let mut by = self.locked();
            Self::tidy(&mut by);
            match by.get_mut(&id) {
                None => return unknown(id),
                Some(e) if e.owner != owner => return not_owner(id, &e.name(), "reads"),
                Some(e) => match &e.result {
                    Some((out, _)) => return out.clone(),
                    None if wait.is_zero() => None,
                    None => e.answer.take(),
                },
            }
        };
        // Waited for outside the lock (other agents' calls go on meanwhile).
        let got = taken.map(|rx| (rx.recv_timeout(wait), rx));
        let mut by = self.locked();
        let Some(e) = by.get_mut(&id) else { return unknown(id) };
        match got {
            Some((Ok(out), _)) => {
                e.result = Some((out.clone(), Instant::now()));
                return out;
            }
            Some((Err(mpsc::RecvTimeoutError::Disconnected), _)) => {
                e.result = Some((dropped(), Instant::now()));
                return dropped();
            }
            Some((Err(mpsc::RecvTimeoutError::Timeout), rx)) => e.answer = Some(rx),
            None => {}
        }
        ToolOutput::text(running(id, e).to_string())
    }
}

/// A running bake as its owner is told of it.
fn running(id: u64, e: &Entry) -> Json {
    Json::obj([
        ("job", Json::from(id)),
        ("status", Json::from("running")),
        ("phase", Json::from(e.progress.phase())),
        ("cells_done", Json::from(e.progress.cells.load(Ordering::Relaxed))),
        ("cells", Json::from(e.cells)),
        ("elapsed_s", Json::from((e.started.elapsed().as_secs_f64() * 10.0).round() / 10.0)),
        ("path", Json::from(e.path.as_str())),
        (
            "next",
            Json::from(format!("sprite_bake {{job: {id}, wait_s: {WAIT_S}}} waits for its answer (kept {} minutes for you); sprite_bake {{cancel: true, job: {id}}} stops it; nothing is written until it ends", KEPT_FOR.as_secs() / 60)),
        ),
    ])
}

// ---------------------------------------------------------------------------------------------
// The call

/// `sprite_bake` (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    match parse(args) {
        Err(m) => ToolOutput::error(crate::tools::argument_error(&m).to_string()),
        Ok(c) => run(tools, session, c),
    }
}

/// The caller's identity key (another connection of the same agent cancels its bakes).
fn owner_key(tools: &Tools, session: &SessionInfo) -> Option<String> {
    crate::project_binary::owner(tools, session).map(|k| k.split('\u{1f}').next().unwrap_or_default().to_owned())
}

fn refusal(r: &plan::Refusal) -> ToolOutput {
    let mut j = Json::obj([("error", Json::from(r.code)), ("message", Json::from(r.message.as_str())), ("rendered", Json::from(0))]);
    if let Some([w, h, bytes]) = r.would_be {
        j.set("would_be", Json::obj([("width", Json::from(w)), ("height", Json::from(h)), ("rgba_bytes", Json::from(bytes))]));
    }
    ToolOutput::error(j.to_string())
}

/// The frame data of a plan.
pub fn sheet_of(p: &plan::Plan, req: &Request, built: &crate::models::Built, light: &str) -> sheet::SheetData {
    let strips = p
        .strips
        .iter()
        .map(|s| {
            let d = &p.directions[s.direction];
            sheet::StripData {
                clip: s.clip.clone(),
                direction: d.name.clone(),
                facing_deg: d.facing_deg,
                first: s.first,
                count: s.count,
                fps: s.fps,
                looping: s.looping,
                duration_s: s.duration_s,
                distance_m: s.distance_m,
                events: s.events.clone(),
                feet: s.feet.clone(),
                feet_down: s.feet_down.clone(),
            }
        })
        .collect();
    let cells = p.cells.iter().map(|c| sheet::CellData { rect: c.rect, strip: c.strip as u32, frame: c.frame, time_s: c.time_s }).collect();
    let mut source = Json::obj([("recipe", Json::from(req.spec.recipe.as_str())), ("model", Json::from(built.model.name.as_str())), ("content", Json::from(format!("{:016x}", built.content)))]);
    if !req.spec.params.is_empty() {
        source.set("params", Json::obj(req.spec.params.iter().map(|(k, v)| (k.clone(), Json::from(*v)))));
    }
    if let Some(seed) = req.spec.seed {
        source.set("seed", Json::from(seed));
    }
    sheet::SheetData {
        texture: req.path.strip_prefix("assets/").unwrap_or(&req.path).to_owned(),
        size: p.atlas,
        cell: p.cell,
        columns: p.columns,
        pivot: p.pivot,
        metres_per_pixel: p.metres_per_pixel,
        view: req.view.name().to_owned(),
        elevation_deg: p.elevation_deg,
        supersample: p.supersample,
        pixel_art: req.pixel_art,
        light: light.to_owned(),
        source,
        outline: req.outline,
        in_place: req.ask.in_place,
        strips,
        cells,
        shadow: p.shadow.as_ref().map(|s| sheet::ShadowData { frame: s.frame, rect: s.rect, pivot: s.pivot, radii_px: s.radii_px }),
    }
}

fn run(tools: &Tools, session: &SessionInfo, call: Call) -> ToolOutput {
    let started = Instant::now();
    let Some(owner) = owner_key(tools, session) else { return err("no_answer", "the engine didn't say who you are") };
    let req = match call {
        Call::Cancel(job) => {
            return match tools.bakes.cancel(&owner, job) {
                Ok(n) => ToolOutput::text(
                    Json::obj([
                        ("cancelled", Json::from(n)),
                        ("note", Json::from(if n == 0 { "no bake of yours is running: nothing was cancelled" } else { "each stops at its next step and answers \"cancelled\" (to its call, or to sprite_bake {job}); nothing of it is written" })),
                    ])
                    .to_string(),
                ),
                Err(out) => out,
            };
        }
        Call::Job { id, wait_s } => return tools.bakes.ask(&owner, id, Duration::from_secs(wait_s)),
        Call::Bake(r) => *r,
    };
    // The model, built as an entity would build it, lent to this thread.
    let slot = BuiltSlot::default();
    let built_answer = tools.send(session, Command::ModelExport(req.spec.clone(), Arc::clone(&slot)));
    if built_answer.is_error {
        return built_answer;
    }
    let Some(built) = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() else { return err("no_answer", "the engine built the model and did not hand it over") };
    // The plan, and every refusal it makes, before anything renders.
    let plan = match plan::plan(&built, built.anims.as_deref(), &req.ask) {
        Ok(p) => p,
        Err(r) => return refusal(&r),
    };
    let png_path = req.path.clone();
    let json_path = frames_path(&png_path);
    // Where it goes: both files must be the project's kinds there, and not another's claim.
    let mut bases = [(0, 0); 2];
    if !req.dry_run {
        for (i, (p, kind)) in [(&png_path, FileKind::Png), (&json_path, FileKind::SpriteSheet)].into_iter().enumerate() {
            let snap = match crate::project_tools::snapshot(tools, session, p, None, false) {
                Ok(s) => s,
                Err(out) => return out,
            };
            if snap.kind != Some(kind) {
                return err("not_a_sprite_path", format!("{p} isn't a {} file of the project here (an atlas and its frame data go in a folder named sprites under assets/, e.g. \"{FOLDER}/wolf.png\")", kind.name()));
            }
            if let Some(e) = snap.claimed {
                return ToolOutput::error(e.to_json().to_string());
            }
            bases[i] = snap.current.as_ref().map_or((0, 0), |f| (f.version, f.hash));
        }
        if tools.content.is_none() {
            return err("no_content_store", "this engine has no content store (binary project files need one)");
        }
    }
    // The bake: a job of the caller's identity, cancellable by its owner (a dry run is none).
    let s = if req.dry_run {
        Started { id: 0, cancel: Arc::default(), name: Arc::default(), progress: Arc::default() }
    } else {
        match tools.bakes.start(&owner, &png_path, plan.cells.len()) {
            Ok(s) => s,
            Err(out) => return out,
        }
    };
    let (done, answer) = crate::jobs::channel();
    let wait = Duration::from_secs(if req.dry_run { WAIT_S } else { req.wait_s });
    let bake = job::Bake {
        req,
        plan,
        built,
        bases,
        assets: tools.procgen.assets.clone(),
        store: tools.content.as_ref().map(Arc::clone),
        cancel: Arc::clone(&s.cancel),
        owner_name: Arc::clone(&s.name),
        progress: Arc::clone(&s.progress),
        done,
        started,
    };
    let sent = tools.send(session, Command::SpriteBake(Box::new(bake)));
    if sent.is_error {
        tools.bakes.end(s.id);
        return sent;
    }
    match answer.recv_timeout(wait) {
        Ok(out) => {
            tools.bakes.end(s.id);
            out
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            tools.bakes.end(s.id);
            dropped()
        }
        Err(mpsc::RecvTimeoutError::Timeout) if s.id == 0 => err("no_answer", "the dry run did not answer in time: call again"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Still running: its answer is kept for its owner, who is told how to ask.
            tools.bakes.park(s.id, answer);
            tools.bakes.ask(&owner, s.id, Duration::ZERO)
        }
    }
}

/// The answer: what was written (or would be), the atlas, what it cost. Bounded: at most
/// [`MAX_LISTED`] strips are listed (the file has all of them).
fn answer(p: &plan::Plan, data: &sheet::SheetData, req: &Request, baked: Option<(&job::Baked, usize)>, commit: Option<Json>, started: Instant) -> Json {
    let r3 = |x: f64| Json::from((x * 1000.0).round() / 1000.0);
    let strips: Vec<Json> = data
        .strips
        .iter()
        .take(MAX_LISTED)
        .map(|s| Json::obj([("clip", s.clip.as_deref().map_or(Json::Null, Json::from)), ("direction", Json::from(s.direction.as_str())), ("first", Json::from(s.first)), ("count", Json::from(s.count)), ("fps", r3(f64::from(s.fps)))]))
        .collect();
    let mut a = Json::obj([
        ("recipe", Json::from(req.spec.recipe.as_str())),
        ("path", Json::from(req.path.as_str())),
        ("frames_path", Json::from(frames_path(&req.path))),
        ("atlas", Json::obj([("width", Json::from(p.atlas[0])), ("height", Json::from(p.atlas[1])), ("pixels", Json::from(u64::from(p.atlas[0]) * u64::from(p.atlas[1])))])),
        ("cell", Json::arr(p.cell.map(Json::from))),
        ("cells", Json::from(p.cells.len())),
        ("columns", Json::from(p.columns)),
        ("metres_per_pixel", Json::from(p.metres_per_pixel)),
        ("pivot", Json::arr(p.pivot.map(Json::from))),
        ("view", Json::from(req.view.name())),
        ("elevation_deg", Json::from(p.elevation_deg)),
        ("directions", Json::arr(p.directions.iter().map(|d| Json::from(d.name.as_str())))),
        ("light", Json::from(data.light.as_str())),
        ("strips", Json::from(data.strips.len())),
        ("listed", Json::arr(strips)),
    ]);
    if let Some((clip, times)) = &p.scale_set_by {
        a.set(
            "scale_set_by",
            Json::obj([
                ("clip", Json::from(clip.as_str())),
                ("times_coarser", r3(f64::from(*times))),
                ("note", Json::from(format!("one scale holds every clip: '{clip}' reaches farthest, so the clips are drawn about {times:.1} times smaller than a middling one of them alone would be; bake it in a sheet of its own for larger frames"))),
            ]),
        );
    }
    let drift: Vec<Json> = p.root_drift_m.iter().filter(|(_, d)| *d > p.metres_per_pixel).map(|(c, d)| Json::obj([("clip", Json::from(c.as_str())), ("metres", r3(f64::from(*d)))])).collect();
    if !drift.is_empty() {
        a.set("root_drift", Json::arr(drift.into_iter().take(MAX_LISTED)));
        a.set(
            "root_drift_is",
            Json::from(if req.ask.in_place { "how far each clip's root wanders sideways as the clip has it; in_place took it out of every frame" } else { "how far each clip's root wanders sideways over its frames (baked as it is): in_place: true takes it out, for a 2D game that moves the sprite itself" }),
        );
    }
    if let Some(sh) = &p.shadow {
        a.set("shadow", Json::obj([("frame", Json::from(sh.frame)), ("rect", Json::arr(sh.rect.map(Json::from))), ("pivot", Json::arr(sh.pivot.map(Json::from)))]));
    }
    let footed = data.strips.iter().filter(|s| !s.feet.is_empty()).count();
    if data.strips.iter().any(|s| s.clip.is_some()) {
        a.set("feet", Json::from(if footed > 0 { format!("{footed} strip(s) say which feet are down in each frame (strips[].feet_down in the frame data)") } else { "the model has no feet anim_check knows (no graph feet, footstep events or bones named foot*), so which feet are down is not said".to_owned() }));
    }
    let mut cost = vec![("wall_ms", r3(started.elapsed().as_secs_f64() * 1000.0))];
    if let Some((b, png_bytes)) = baked {
        cost.push(("renders", Json::from(b.renders)));
        cost.push(("engine_ms", r3(b.engine_ms)));
        cost.push(("max_step_ms", r3(b.max_step_ms)));
        a.set("png_bytes", Json::from(png_bytes));
    }
    a.set("cost", Json::obj(cost));
    if let Some(c) = commit {
        for k in ["files", "journal"] {
            if let Some(v) = c.get(k) {
                a.set(k, v.clone());
            }
        }
        a.set("next", Json::from(format!("world_spawn {{shape: \"sprite\", sprite: <strips[i].sprite of {}>}} plays a strip in the 2D path; world_undo takes both files back", frames_path(&req.path))));
    }
    a.set("same_bytes", Json::from("the same inputs on the same adapter and driver give the same bytes; another adapter rasterises edges, shades and rounds differently, so its sheet differs by a few levels at edges and in shading"));
    a
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let int = |lo: u64, hi: u64, d: &str| Json::obj([("type", Json::from("integer")), ("minimum", Json::from(lo)), ("maximum", Json::from(hi)), ("description", Json::from(d))]);
    let text = |n: usize, d: &str| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(n)), ("description", Json::from(d))]);
    vec![ToolDef {
        name: "sprite_bake".into(),
        description: format!(
            "Bake a model and its clips into a sprite sheet for a 2D game: one PNG atlas with alpha (the model's silhouette exactly, from the ID buffer, \
             never a colour key) and its frame data as JSON beside it ({FOLDER}/<recipe's name>.png and .json unless path says where). The camera is \
             orthographic and fitted to every posed vertex of every frame from every direction, so a swung limb is never cut off and one pixel is the \
             same size in every cell; the pivot (the model's origin on its ground) is the same pixel in every cell. Each strip (a clip from a direction) \
             carries the 2D path's own sprite object: world_spawn {{shape: \"sprite\", sprite: <strips[i].sprite>}} plays it. pixel_art: no smoothing, \
             all-or-nothing alpha, colours snapped to the project's palette. Refused before any render when the atlas would pass {} pixels a side or \
             {} in all (the size it would have been is said), past {} cells, or a clip past {} frames. Written as two project files in one journal entry \
             (claims, versions, world_undo). The same inputs on the same adapter give the same bytes. Seen from the view asked: side (level, 0 degrees), \
             three_quarter (30 degrees down, the default) or top (90), or elevation_deg; the angle is in the answer and the frame data. Rendered \
             off the engine thread; the call waits up to wait_s (default {WAIT_S}) and a bake still running then answers {{job, status: \
             \"running\"}}: {{job: N, wait_s}} gives its progress or its result (kept for its owner {} minutes; its files are written when it \
             ends); {{cancel: true}} or {{cancel: true, job: N}} stops your own. No shadow is baked in (shadow: \"blob\" adds a soft one in a cell of its own); each strip says which feet are down per \
             frame. dry_run plans and says the atlas without rendering.",
            plan::MAX_ATLAS_SIDE,
            plan::MAX_ATLAS_PIXELS,
            plan::MAX_CELLS,
            plan::MAX_FRAMES_PER_CLIP,
            KEPT_FOR.as_secs() / 60
        ),
        input_schema: Json::obj([
            ("type", Json::from("object")),
            (
                "properties",
                Json::obj([
                    ("recipe", text(MAX_PATH_CHARS, "The model: a recipe (or .glb) under the assets folder, \"recipes/shambler.json\"")),
                    ("params", Json::obj([("type", Json::from("object")), ("maxProperties", Json::from(world::MAX_MODEL_PARAMS)), ("description", Json::from("Recipe parameters, e.g. {\"tier\": 2}"))])),
                    ("seed", int(0, world::MAX_MODEL_SEED, "Variant seed (default: the recipe's)")),
                    (
                        "clips",
                        Json::obj([
                            ("description", Json::from(format!("Clip names to bake (at most {}), or \"all\"; left out: a still (the model as it stands)", plan::MAX_CLIPS))),
                            ("anyOf", Json::arr([Json::obj([("type", Json::from("array")), ("maxItems", Json::from(plan::MAX_CLIPS)), ("items", text(MAX_NAME_CHARS, "a clip"))]), Json::obj([("type", Json::from("string")), ("enum", Json::arr([Json::from("all")]))])])),
                        ]),
                    ),
                    (
                        "directions",
                        Json::obj([
                            ("description", Json::from("1, 4, 8 or 16 evenly round (default 8; 1 for the side view, facing right), or a list of facings in degrees (0 faces the viewer, 90 right, 180 away)")),
                            ("anyOf", Json::arr([Json::obj([("type", Json::from("integer")), ("enum", Json::arr([1, 4, 8, 16].map(Json::from)))]), Json::obj([("type", Json::from("array")), ("maxItems", Json::from(plan::MAX_DIRECTIONS)), ("items", Json::obj([("type", Json::from("number")), ("minimum", Json::from(0)), ("exclusiveMaximum", Json::from(360))]))])])),
                        ]),
                    ),
                    ("view", Json::obj([("type", Json::from("string")), ("enum", Json::arr(View::ALL.map(|v| Json::from(v.0)))), ("description", Json::from("Camera height: side (level), three_quarter (30 degrees down; default), top (straight down)"))])),
                    ("elevation_deg", Json::obj([("type", Json::from("number")), ("minimum", Json::from(0)), ("maximum", Json::from(90)), ("description", Json::from("The camera's angle down, instead of the view's"))])),
                    ("frames", int(1, u64::from(plan::MAX_FRAMES_PER_CLIP), "Frames per clip, whatever its length (or fps)")),
                    ("fps", Json::obj([("type", Json::from("number")), ("minimum", Json::from(1)), ("maximum", Json::from(plan::MAX_FPS)), ("description", Json::from("Frames a second of each clip (default 12)"))])),
                    (
                        "cell",
                        Json::obj([
                            ("description", Json::from(format!("A cell's side in pixels, or [width, height] ({} to {}; default 128, 64 for pixel_art)", plan::MIN_CELL_PX, plan::MAX_CELL_PX))),
                            ("anyOf", Json::arr([int(u64::from(plan::MIN_CELL_PX), u64::from(plan::MAX_CELL_PX), "side"), Json::obj([("type", Json::from("array")), ("minItems", Json::from(2)), ("maxItems", Json::from(2)), ("items", int(u64::from(plan::MIN_CELL_PX), u64::from(plan::MAX_CELL_PX), "pixels"))])])),
                        ]),
                    ),
                    ("supersample", Json::obj([("type", Json::from("integer")), ("enum", Json::arr([1, 2, 4].map(Json::from))), ("description", Json::from("Samples a side of each pixel: an edge pixel's alpha is the share the model covers (default 2; 1 for pixel_art)"))])),
                    ("pixel_art", t("boolean", "No smoothing: all-or-nothing alpha, colours snapped to the project's palette (style.json), played with nearest filtering")),
                    ("outline", t("boolean", "Pixel art only: one palette colour (its darkest) drawn round the silhouette, inside the cell's clear margin")),
                    ("in_place", t("boolean", "Take each clip's sideways root wander out of every frame (a 2D game moves the sprite itself); the wander is said either way")),
                    ("shadow", Json::obj([("type", Json::from("string")), ("enum", Json::arr([Json::from("none"), Json::from("blob")])), ("description", Json::from("none (default: no shadow baked in; a 2D game draws its own) or blob: a soft alpha-only ellipse in a cell of its own, drawn under the model at its pivot"))])),
                    ("light", Json::obj([("type", Json::from("string")), ("enum", Json::arr([Json::from("neutral"), Json::from("project")])), ("description", Json::from("neutral (default) or the project's: its style.toml [stage] game rig; the light turns with the camera, so every direction is lit alike on screen"))])),
                    ("path", text(MAX_PATH_CHARS, "Where the atlas goes: a .png in a folder named sprites under assets/ (the .json goes beside it)")),
                    ("dry_run", t("boolean", "Plan only: the atlas's size, cells and scale, nothing rendered or written")),
                    ("wait_s", int(0, WAIT_S, "How long the call waits for the bake (default 20); a bake still running then answers with its job. With job: how long to wait for its answer (default 0)")),
                    ("job", int(1, u64::MAX >> 11, "A bake's job: alone (or with wait_s) its progress or kept result; with cancel, stop that one")),
                    ("cancel", t("boolean", "Alone or with job: stop your own bakes running now (nothing of them is written)")),
                ]),
            ),
        ]),
    }]
}

#[cfg(test)]
mod tests;
