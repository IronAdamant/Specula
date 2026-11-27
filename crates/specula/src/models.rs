//! Procedural models in the world: recipe files built with `specula-model` on worker threads,
//! cached, uploaded once and drawn at a level of detail that fits their size on screen.
//!
//! # Where recipes come from
//!
//! A model entity names a recipe file relative to the **assets root** (`--assets DIR`, default the
//! engine's working directory), e.g. `recipes/warding_stone.json`. The other `*.json` files in
//! the same folder are its include library (by file stem), and its style is `--style FILE` or the
//! first `style.json` found walking up from the recipe's folder to the assets root (Sepulcra keeps
//! `assets/style.json` above `assets/recipes/`). Paths can't leave the assets root, and links
//! inside it are not followed.
//!
//! # Never stalling the engine
//!
//! Builds run on worker threads. Resolving a spec ([`Library::resolve`], what `world_spawn` and the
//! game's in-process spawns call) only does what is cheap: it checks the path, reads the file if
//! needed, finds the style, **parses** the recipe and checks parameter names and ranges — those
//! mistakes are refused at spawn with the recipe's structured error — and then either finds the
//! build in the cache or queues it. The entity's model is `building` until the build lands; it is
//! drawn from then on (captures wait for builds in flight, so a capture right after a spawn shows
//! the model). A build that fails (budget, geometry) leaves the entity `failed`.
//!
//! Procedural builds (booleans, SDF sculpts, generators, texture graphs, or a recipe including
//! one) queue apart: one worker takes them first, the light workers but one take them when no
//! light build waits, and the first light worker never does, so a light build never waits behind
//! them. Every build is bounded by `specula-model`'s work budget (about 1.5 s of one core, tier-1
//! ops, LODs and colliders included), and a build nobody wants any more stops: queued ones are
//! skipped, running ones see their cancel flag ([`specula_model::work`]) at the next budget
//! check (a build that finished anyway is kept and cached).
//!
//! # Cache and sharing
//!
//! A build is keyed by **content**: a hash of the recipe file, the files it includes (transitively)
//! and its style, plus the parameters and seed. Entities with the same key share one [`Built`]
//! model, and the GPU side ([`GpuModels`]) uploads each build once: one mesh per LOD per material
//! part, so equal models instance together. A spec (recipe path + parameters + seed) gets a stable
//! **handle**; the world stores it on the entity with its state and bounds, and the renderer maps
//! handles to uploaded meshes without locking the library.
//!
//! # Hot reload
//!
//! [`Library::poll`] checks the modification times of the files in the folders it uses (and of
//! the styles) at most once a second; `model_reload` checks now, and `model_build` re-reads its own
//! recipe's folder. Only recipes whose content (the file, its includes or its style) changed are
//! looked at again, and only handles whose key changed are rebuilt; a recipe that broke keeps its
//! last good build (the error is reported).
//!
//! # Bounds on what agents can make it hold
//!
//! Folders that don't exist are never cached; folders, recipes, styles and builds no handle uses
//! are dropped by [`Library::collect_garbage`] (the engine runs it at most once a second after the
//! world's models changed, drawn or not, or at once when specs outnumber entities by more than
//! [`SPARE_HANDLES`]), which also forgets unused handles past that many. While builds are in
//! flight, the cheaper [`Library::drop_unwanted`] runs every 100 ms after a change: builds no
//! entity waits for any more are cancelled, so deleted entities never hold up live ones. A handle
//! no entity used meanwhile works its key out again when it is next used (its recipe may have
//! changed while nothing was watching it).
//!
//! # Materials
//!
//! Each part draws with its style preset: `standard` → [`Material::Lit`] with the preset's
//! emissive, metallic, roughness and specular, colour from the vertex colours (sRGB) times the
//! entity's `color` (white = as authored); `glass` → [`Material::Glass`] with the preset's
//! opacity, tinted by the part's mean vertex colour times the entity's colour (blended materials
//! don't read vertex colours).
//!
//! # glTF assets
//!
//! A model entity whose `recipe` ends in `.glb` or `.gltf` imports that asset (`specula-gltf`,
//! textures kept, not baked into vertex colours) on a worker thread instead of building a recipe;
//! it takes no `params` and no `seed`. Its key is the file's path, size and modification time
//! (cheap to check at spawn), so editing the file rebuilds it like a recipe. Images are decoded
//! and their mip chains made on the worker; the GPU side uploads each texture once per build and
//! every entity of the build shares them. A part whose material has a texture draws with its
//! UVs (and tangents) and the glTF material: base colour factor and texture, normal, metallic-
//! roughness and emissive maps, alpha masking. Textured parts ignore vertex colours (`COLOR_0`):
//! see `specula-gpu`.
//!
//! # Animation
//!
//! A build may carry an anim set: a rigged recipe's `anims/<recipe name>.json` (in the folder
//! next to the recipe folder, as Sepulcra keeps them; part of the build's content, so editing it
//! rebuilds), or a glTF asset's skin and animations. An entity's [`crate::world::AnimSpec`]
//! names a clip; its pose is evaluated at the frame's render time by a [`specula_anim::Animator`]
//! kept per entity (reused, so steady frames allocate nothing), turned into skinning matrices
//! written into the frame's bone list, and drawn by the GPU's skinned pipelines (every part of
//! the chosen LOD shares the palette). Rigged models without an `anim` (or naming a clip the set
//! lacks) draw their bind pose. [`GpuModels::bone_positions`] gives an entity's bones in world
//! space at a render time (the sight skeleton overlay).
//!
//! A graph-mode `anim` plays one of the set's animation graphs (`specula_anim::play`), posed at
//! the frame's time with its IK: feet on the world's ground ([`crate::anim::GroundGrid`]),
//! two-bone reaches and look-at targets. The library lays the world's anim assets (agents'
//! clips and graphs, [`crate::anim`]) over each build's anim set ([`Library::anim_set`]).
//!
//! # Levels of detail
//!
//! The size on screen is the projected diameter of the model's bounding sphere in pixels. LOD 0
//! draws at [`LOD_PIXELS`]`[0]` (192 px) and above, LOD 1 down to 64 px, LOD 2 down to 24 px,
//! LOD 3 below (clamped to the model's last LOD). The window keeps each entity's level with
//! [`LOD_HYSTERESIS`] (±15% around each threshold) so a model at a boundary doesn't flicker;
//! captures choose without memory, so a capture is reproducible.

use crate::world::{is_gltf_path, AnimSpec, Entity, ModelSource, ModelSpec, ModelState, ResolvedModel, World, WorldError};
use specula_anim::math::Affine;
use specula_anim::play::{GraphPlayer, Graphed, Surroundings, Timing};
use specula_anim::{AnimSet, Animator, LoopMode, Playback, Pose, Skeleton};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::{
    DrawItem, Gpu, Material, MaterialTextures, MeshId, MeshStreams, PipelineVariants, SkinVertex, Surface, TextureFilter, TextureFormat,
    TextureId, TextureSampler, TextureWrap, VariantFailure, Vertex,
};

/// "skinned", "textured" or "skinned and textured".
fn variant_names(f: &VariantFailure) -> &'static str {
    match (f.skinned, f.textured) {
        (true, true) => "skinned and textured",
        (true, false) => "skinned",
        _ => "textured",
    }
}
use specula_model::{build, BuildParams, ErrorCode, MaterialKind, MaterialPreset, MeshData, Model, Recipe, RecipeError, Style};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// Recipe and style files are checked for changes at most this often.
pub const POLL_EVERY: Duration = Duration::from_secs(1);

/// How long one re-key may run on the engine thread (the window's and headless loops' library,
/// [`Library::set_rekey_slice`]) before it leaves the rest of the forgotten recipes to the next
/// pass ([`Library::rekey_more`]): parsing recipes and reading their anim sets took 12 ms in one
/// re-key under the recipe-write load (review R1, minor 4). At least one recipe is worked out a
/// call, so the re-key always moves.
pub const REKEY_SLICE: Duration = Duration::from_millis(2);
/// The style file looked for next to (and above) a recipe.
pub const STYLE_FILE: &str = "style.json";
/// Projected bounding-sphere diameter (pixels) at which each coarser LOD takes over: LOD k + 1
/// below `LOD_PIXELS[k]`.
pub const LOD_PIXELS: [f32; 3] = [192.0, 64.0, 24.0];
/// Fraction around each LOD threshold the window's choice must cross before it switches.
pub const LOD_HYSTERESIS: f32 = 0.15;
/// Histogram buckets for LOD statistics (LOD 3 and coarser share the last).
pub const LOD_BUCKETS: usize = 4;
/// Unused handles kept before the library forgets them.
pub const SPARE_HANDLES: usize = 256;
/// Builds kept for `model_build` results no entity uses (so building then spawning reuses them).
const RECENT_BUILDS: usize = 16;
/// How often a request reads again when what it planned from went while it read (a collection
/// dropped its folder); past this it reads under the lock, where nothing lands in between.
const READ_AGAIN: usize = 3;
/// Worker threads that build models.
const WORKERS: usize = 2;
/// Worker threads that take procedural builds (booleans, sculpts, generators, textures) first.
/// The first light worker never takes them, so a light build never waits behind them; the
/// others take them when no light build waits.
const HEAVY_WORKERS: usize = 1;
/// Lists in a reload report stop here (the rest is counted).
const MAX_LISTED: usize = 20;

/// Where the engine thread puts a built model for the caller's thread that asked for it
/// (`model_export`: the export, its encoding and the file's check run on that thread; the
/// engine thread's part is the build request `model_build` makes, and one commit).
pub type BuiltSlot = Arc<Mutex<Option<Arc<Built>>>>;

/// One built model, shared by every spec with the same content, parameters and seed.
pub struct Built {
    /// Unique per build: the GPU cache's key.
    pub id: u64,
    pub model: Model,
    /// Content hash of the recipe, its includes and its style (or of a glTF file's path, size
    /// and modification time).
    pub content: u64,
    pub build_ms: f64,
    /// The style file it was built with (empty for glTF assets).
    pub style: PathBuf,
    /// The model's skeleton and clips, when it has both.
    pub anims: Option<Arc<AnimSet>>,
    /// Why a rigged model has no anim set (no anims file, or it failed to load).
    pub anim_note: Option<String>,
    /// Material textures, ready to upload (glTF assets).
    pub textures: Vec<TextureData>,
    /// Material textures and factors by part material name (glTF assets).
    pub materials: BTreeMap<String, PartMaterial>,
    /// What the import or the anim set warned about (bounded).
    pub warnings: Vec<String>,
}

/// A material texture with its mip chain, made on a worker thread. The pixels are taken by the
/// first upload (the CPU copy would double texture memory); a build uploaded again later (its
/// entities were all deleted, then one came back while the build was still cached) is imported
/// afresh ([`Library::reimport`]).
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    /// Mip levels in the chain.
    pub mips: usize,
    /// `levels[0]` is the image; each next level half the size. Empty once uploaded.
    pub levels: Mutex<Vec<Vec<u8>>>,
    pub sampler: TextureSampler,
}

/// A glTF material as a part draws it (texture fields index [`Built::textures`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartMaterial {
    /// sRGB colour and alpha (the base colour factor).
    pub color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub base: Option<usize>,
    pub normal: Option<usize>,
    pub normal_scale: f32,
    pub metallic_roughness: Option<usize>,
    pub emissive: Option<usize>,
    /// sRGB emissive colour and its strength (0 = none).
    pub emissive_color: [f32; 3],
    pub emissive_strength: f32,
    /// Alpha masking cutoff (0 = opaque).
    pub alpha_cutoff: f32,
}

impl PartMaterial {
    /// Draws with textures (UVs needed).
    pub fn textured(&self) -> bool {
        self.base.is_some() || self.normal.is_some() || self.metallic_roughness.is_some() || self.emissive.is_some()
    }
}

/// What a worker made besides the model.
#[derive(Default)]
pub struct Extras {
    pub anims: Option<AnimSet>,
    pub anim_note: Option<String>,
    pub textures: Vec<TextureData>,
    pub materials: BTreeMap<String, PartMaterial>,
    pub warnings: Vec<String>,
}

impl Built {
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let b = self.model.bounds;
        (Vec3::new(b.min[0], b.min[1], b.min[2]), Vec3::new(b.max[0], b.max[1], b.max[2]))
    }

    /// [`Built::bounds`] grown sideways by the farthest its wind term can sway it (the recipe's
    /// `wind` in the strongest wind): what it can cover on screen. Entities' boxes (sight's
    /// projected boxes, placement) use it, so a swaying blade never draws outside its subject.
    pub fn drawn_bounds(&self) -> (Vec3, Vec3) {
        let (lo, hi) = self.bounds();
        let reach = self.model.wind.map_or(0.0, |w| specula_gpu::Wind { amount: w.amount, height: w.height, painted: w.painted }.reach());
        (Vec3::new(lo.x - reach, lo.y, lo.z - reach), Vec3::new(hi.x + reach, hi.y, hi.z + reach))
    }
}

/// `(content, parameters as exact bits, seed)`.
type BuildKey = (u64, String, Option<u64>);

/// One file of a recipe folder as last read.
#[derive(Clone)]
struct SourceFile {
    text: Result<Arc<str>, String>,
    mtime: Option<SystemTime>,
}

/// A recipe folder: its regular `*.json` files (the include library).
#[derive(Clone)]
struct Folder {
    files: BTreeMap<String, SourceFile>,
    /// The folder's own modification time (files added or removed).
    mtime: Option<SystemTime>,
}

struct StyleFile {
    parsed: Result<Arc<Style>, RecipeError>,
    mtime: Option<SystemTime>,
    hash: u64,
}

/// What a recipe's builds need, worked out once per content.
/// The texts a recipe's build is made from (see [`Library::current_texts`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeTexts {
    pub recipe: String,
    /// The recipes it includes, transitively, by name.
    pub library: BTreeMap<String, String>,
    /// Its `style.json`.
    pub style_path: PathBuf,
}

struct RecipeInfo {
    folder: PathBuf,
    content: u64,
    text: Arc<str>,
    /// The files it includes, transitively (stem → text).
    library: Arc<BTreeMap<String, String>>,
    /// Every name it includes, transitively, found or not (a write to any of them, or to the
    /// recipe itself, works it out again; others leave it be).
    includes: BTreeSet<String>,
    style: Arc<Style>,
    style_path: PathBuf,
    recipe: Recipe,
    /// Where its anim set would be, and that file's time when read (polled for changes).
    /// `None`: there was no file, or its time was too fresh to compare with ([`stamp_of`]); a
    /// scan then compares what the file says with `anims`.
    anims_path: PathBuf,
    anims_mtime: Option<SystemTime>,
    /// The anim-set text (rigged recipes with an anims file).
    anims: Option<Arc<str>>,
    /// It or something it includes is procedural: built on the heavy worker.
    heavy: bool,
}

/// A spec's slot: what it draws now and what it waits for.
struct Handle {
    spec: ModelSpec,
    built: Option<Arc<Built>>,
    /// The key `built` was built for.
    built_key: Option<BuildKey>,
    /// The key of the build it is waiting for (queued or running).
    want: Option<BuildKey>,
    /// The latest build's error (the last good build stays in `built`).
    error: Option<WorldError>,
    /// No entity used it for a while (garbage collection stopped its waiting): its recipe may
    /// have changed meanwhile, so the next resolve works its key out again.
    verify: bool,
}

impl Handle {
    fn resolved(&self, h: u32) -> ResolvedModel {
        let state = match (&self.built, &self.want, &self.error) {
            (Some(_), _, _) => ModelState::Ready,
            (None, Some(_), _) => ModelState::Building,
            (None, None, _) => ModelState::Failed,
        };
        ResolvedModel { handle: h, state, bounds: self.built.as_ref().map(|b| b.drawn_bounds()) }
    }
}

/// A build for a worker.
struct Job {
    id: u64,
    key: BuildKey,
    recipe: String,
    text: Arc<str>,
    library: Arc<BTreeMap<String, String>>,
    style: Arc<Style>,
    style_path: PathBuf,
    params: Vec<(String, f64)>,
    seed: Option<u64>,
    /// A glTF asset to import (its full path) instead of a recipe to build.
    gltf: Option<PathBuf>,
    /// A rigged recipe's anim-set file text.
    anims: Option<Arc<str>>,
    /// Set when nobody wants the build any more: a running build stops at its next budget check.
    cancel: Arc<AtomicBool>,
    /// The call that queued it: the worker builds inside it (docs/design/TRACE.md).
    trace: specula_core::trace::Ctx,
}

/// A worker's answer.
struct Done {
    id: u64,
    key: BuildKey,
    recipe: String,
    style_path: PathBuf,
    outcome: Outcome,
    build_ms: f64,
}

/// How a job ended.
enum Outcome {
    Built(Box<Model>, Box<Extras>),
    /// The recipe failed to build (budget, geometry, a missing include...).
    Failed(RecipeError),
    /// `specula-model` panicked (a bug there, not in the recipe).
    Panicked(String),
    /// Nobody wanted it any more when a worker got to it.
    Cancelled,
}

/// A build in flight.
struct InFlight {
    job: u64,
    /// A `model_build` call waits for it: never cancelled.
    pinned: bool,
}

/// A finished build, for whoever waited on its job.
pub struct Finished {
    pub job: u64,
    pub result: Result<Arc<Built>, WorldError>,
}

/// What `model_build` gets back at once.
pub enum BuildRequest {
    /// Built already (from the cache: `true`).
    Ready(Arc<Built>, bool),
    /// Queued (or already running): wait for this job.
    Pending(u64),
}

/// What a reload looked at and queued.
#[derive(Default)]
pub struct Reload {
    pub files_changed: usize,
    /// Builds queued for changed recipes, and the specs (recipe keys) they serve.
    pub jobs: BTreeSet<u64>,
    pub rebuilding: Vec<String>,
    /// Specs that changed to a build already in the cache.
    pub rebuilt: Vec<String>,
    /// Errors (recipe, structured error), for specs that keep their last good build.
    pub errors: Vec<(String, Json)>,
    /// Handles whose source content changed (so may their anim set): the engine re-settles the
    /// graph plays of their entities, journaled, at this point.
    pub anim_handles: BTreeSet<u32>,
    /// Every handle whose key was worked out again: only their entities are resolved again
    /// (re-resolving every model entity and resetting the draw cache took 18 ms a recipe write
    /// in Sepulcra's window).
    pub handles: BTreeSet<u32>,
}

impl Reload {
    /// The report, bounded: at most [`MAX_LISTED`] names per list and identical errors grouped.
    pub fn to_json(&self) -> Json {
        let list = |items: &[String]| {
            let shown = Json::arr(items.iter().take(MAX_LISTED).map(|r| Json::from(r.as_str())));
            (shown, items.len().saturating_sub(MAX_LISTED))
        };
        let (rebuilt, rebuilt_omitted) = list(&self.rebuilt);
        let (rebuilding, rebuilding_omitted) = list(&self.rebuilding);
        let mut groups: BTreeMap<String, (Json, Vec<&str>)> = BTreeMap::new();
        for (recipe, err) in &self.errors {
            let g = groups.entry(err.to_string()).or_insert_with(|| (err.clone(), Vec::new()));
            g.1.push(recipe);
        }
        let error_groups = groups.len();
        let errors = Json::arr(groups.into_values().take(MAX_LISTED).map(|(err, specs)| {
            let mut sample: Vec<&str> = specs.clone();
            sample.sort();
            sample.dedup();
            Json::obj([
                ("error", err),
                ("specs", Json::from(specs.len())),
                ("recipes", Json::arr(sample.into_iter().take(5).map(Json::from))),
            ])
        }));
        Json::obj([
            ("files_changed", Json::from(self.files_changed)),
            ("rebuilt", rebuilt),
            ("rebuilt_omitted", Json::from(rebuilt_omitted)),
            ("rebuilding", rebuilding),
            ("rebuilding_omitted", Json::from(rebuilding_omitted)),
            ("errors", errors),
            ("errors_omitted", Json::from(error_groups.saturating_sub(MAX_LISTED))),
            ("error_count", Json::from(self.errors.len())),
        ])
    }
}

/// Jobs waiting for a worker: light ones and procedural (heavy) ones.
#[derive(Default)]
struct Queues {
    light: VecDeque<Job>,
    heavy: VecDeque<Job>,
    /// The library is gone: workers exit.
    closed: bool,
}

impl Queues {
    /// The next job a worker of this kind takes, if any.
    fn take(&mut self, takes: Takes) -> Option<Job> {
        match takes {
            Takes::Light => self.light.pop_front(),
            Takes::LightFirst => self.light.pop_front().or_else(|| self.heavy.pop_front()),
            Takes::HeavyFirst => self.heavy.pop_front().or_else(|| self.light.pop_front()),
        }
    }
}

/// A job's outcome once its cancel flag is read: a build that finished is kept (and cached) even
/// if it was dropped meanwhile; anything else from a cancelled job was stopped part way.
fn settled(o: Outcome, cancelled: bool) -> Outcome {
    match o {
        o @ Outcome::Built(..) => o,
        _ if cancelled => Outcome::Cancelled,
        o => o,
    }
}

/// What a worker takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Takes {
    /// Light jobs only: a light build never waits for a free worker behind procedural ones.
    Light,
    /// Light jobs first, procedural ones when no light job waits.
    LightFirst,
    /// Procedural jobs first, light ones when none waits.
    HeavyFirst,
}

/// The worker threads, their queues and their answers. Dropping it lets the workers finish and
/// exit.
struct Workers {
    queues: Arc<(Mutex<Queues>, Condvar)>,
    done: Receiver<Done>,
    /// Jobs nobody waits for any more: workers skip them.
    cancelled: Arc<Mutex<BTreeSet<u64>>>,
    /// Cancel flags of jobs queued or running (a running build stops when its flag is set).
    flags: Arc<Mutex<BTreeMap<u64, Arc<AtomicBool>>>>,
    /// Tests deliver results by hand through this.
    #[cfg(test)]
    inject: mpsc::Sender<Done>,
}

impl Drop for Workers {
    fn drop(&mut self) {
        let (q, cv) = &*self.queues;
        q.lock().unwrap_or_else(PoisonError::into_inner).closed = true;
        cv.notify_all();
    }
}

impl Workers {
    /// `n` workers for light builds (the first takes nothing else, the rest take procedural
    /// builds when no light one waits) and `heavy` that take procedural builds first.
    fn start(n: usize, heavy: usize) -> Workers {
        let (tx, done) = mpsc::channel::<Done>();
        let queues = Arc::new((Mutex::new(Queues::default()), Condvar::new()));
        let cancelled = Arc::new(Mutex::new(BTreeSet::new()));
        let kinds = (0..n).map(|i| (format!("specula-model-{i}"), if i == 0 { Takes::Light } else { Takes::LightFirst })).chain((0..heavy).map(|i| (format!("specula-model-heavy-{i}"), Takes::HeavyFirst)));
        for (name, takes) in kinds {
            let (tx, cancelled, queues) = (tx.clone(), Arc::clone(&cancelled), Arc::clone(&queues));
            let spawned = std::thread::Builder::new().name(name.clone()).spawn(move || loop {
                let job = {
                    let (q, cv) = &*queues;
                    let mut g = q.lock().unwrap_or_else(PoisonError::into_inner);
                    loop {
                        if g.closed {
                            return; // the library is gone
                        }
                        match g.take(takes) {
                            Some(j) => break j,
                            None => g = cv.wait(g).unwrap_or_else(PoisonError::into_inner),
                        }
                    }
                };
                let skip = cancelled.lock().unwrap_or_else(PoisonError::into_inner).remove(&job.id);
                // The build is part of the call that asked for it.
                let ctx = job.trace.enter();
                let mut span = crate::trace_engine::job_span("model.build");
                span.field("recipe", job.recipe.as_str());
                span.field("cancelled", skip);
                let started = Instant::now();
                let outcome = if skip {
                    Outcome::Cancelled
                } else {
                    let _flag = specula_model::work::cancel_on(Arc::clone(&job.cancel));
                    let o = run(&job, build);
                    settled(o, job.cancel.load(Ordering::Relaxed))
                };
                let done = Done {
                    id: job.id,
                    key: job.key,
                    recipe: job.recipe,
                    style_path: job.style_path,
                    outcome,
                    build_ms: started.elapsed().as_secs_f64() * 1000.0,
                };
                drop(span);
                drop(ctx);
                if tx.send(done).is_err() {
                    return;
                }
            });
            if let Err(e) = spawned {
                eprintln!("specula: model build thread {name} not started: {e}");
            }
        }
        Workers {
            queues,
            done,
            cancelled,
            flags: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(test)]
            inject: tx,
        }
    }

    /// Queues a job (procedural ones on their own queue).
    fn push(&self, job: Job, heavy: bool) {
        let (q, cv) = &*self.queues;
        let mut g = q.lock().unwrap_or_else(PoisonError::into_inner);
        if heavy {
            g.heavy.push_back(job);
        } else {
            g.light.push_back(job);
        }
        drop(g);
        cv.notify_all();
    }

    /// Nobody wants job `id` any more: a queued one is skipped, a running one stops at its next
    /// budget check.
    fn cancel(&self, id: u64) {
        self.cancelled.lock().unwrap_or_else(PoisonError::into_inner).insert(id);
        if let Some(f) = self.flags.lock().unwrap_or_else(PoisonError::into_inner).get(&id) {
            f.store(true, Ordering::Relaxed);
        }
    }
}

/// Whether a recipe text uses procedural tiers (booleans, sculpts, generators, texture graphs):
/// such builds go to their own worker. A false positive only changes the queue.
fn procedural(text: &str) -> bool {
    ["\"union\"", "\"difference\"", "\"intersection\"", "\"sdf\"", "\"generate\"", "\"textures\"", "\"texgraph\""].iter().any(|k| text.contains(k))
}

/// The build function workers call (`specula_model::build`; tests pass others).
type BuildFn = fn(&str, &BuildParams) -> Result<Model, RecipeError>;

/// Builds one job with `build` (or imports its glTF asset); a panic becomes
/// [`Outcome::Panicked`] instead of a dead worker.
fn run(job: &Job, build: BuildFn) -> Outcome {
    let values: Vec<(&str, f64)> = job.params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let params = BuildParams { values: &values, seed: job.seed, library: Some(&job.library), ..BuildParams::new(&job.style) };
    let built = specula_core::trace::catch_unwind(std::panic::AssertUnwindSafe(|| match &job.gltf {
        Some(path) => import_gltf(path),
        None => build(&job.text, &params).map(|model| {
            let extras = recipe_extras(&model, job.anims.as_deref());
            (model, extras)
        }),
    }));
    match built {
        Ok(Ok((model, extras))) => match check_uploadable(&model) {
            Ok(()) => Outcome::Built(Box::new(model), Box::new(extras)),
            Err(m) => Outcome::Failed(RecipeError::new("", ErrorCode::InvalidMesh, m)),
        },
        Ok(Err(e)) => Outcome::Failed(e),
        Err(panic) => {
            let why = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Outcome::Panicked(why)
        }
    }
}

/// The anim set of a rigged recipe model (from its anims file), or why there is none.
fn recipe_extras(model: &Model, anims: Option<&str>) -> Extras {
    let mut x = Extras::default();
    procgen_textures(model, &mut x);
    let Some(rig) = &model.rig else { return x };
    match anims {
        None => x.anim_note = Some("rigged, but there is no anims/<name>.json for it: drawn in its bind pose".into()),
        Some(text) => match Skeleton::from_rig(rig).and_then(|s| AnimSet::load(text, s)) {
            Ok(set) => x.anims = Some(set),
            Err(e) => x.anim_note = Some(format!("the anim set failed to load ({e}): drawn in its bind pose")),
        },
    }
    x
}

/// A recipe's baked texture sets (procedural materials, `specula_model::texgraph`) as the build's
/// textures (mip chains made here, on the worker) and textured part materials: the texture holds
/// the absolute colour, roughness and metalness (factors 1), the preset its emission.
fn procgen_textures(model: &Model, x: &mut Extras) {
    let sampler = TextureSampler {
        mag: TextureFilter::Linear,
        min: TextureFilter::Linear,
        mipmap: TextureFilter::Linear,
        wrap_u: TextureWrap::Repeat,
        wrap_v: TextureWrap::Repeat,
        anisotropy: true,
    };
    // Texture sets shared by several presets upload once (by content hash).
    let mut by_hash: BTreeMap<u64, [Option<usize>; 4]> = BTreeMap::new();
    for (key, set) in &model.extras.textures {
        let slots = *by_hash.entry(set.hash).or_insert_with(|| {
            let n = set.size;
            let mut push = |rgba: &[u8], format: TextureFormat, cutoff: f32| -> Option<usize> {
                let mut levels = specula_gpu::generate_mips_owned(n, n, format, rgba.to_vec()).ok()?;
                // A cutout keeps its coverage with distance (cards don't thin out and vanish).
                specula_gpu::preserve_alpha_coverage(&mut levels, cutoff);
                x.textures.push(TextureData { width: n, height: n, format, mips: levels.len(), levels: Mutex::new(levels), sampler });
                Some(x.textures.len() - 1)
            };
            [
                push(&set.base_color, TextureFormat::Rgba8Srgb, set.alpha_cutoff),
                set.normal.as_deref().and_then(|n| push(n, TextureFormat::Rgba8Unorm, 0.0)),
                push(&set.metallic_roughness, TextureFormat::Rgba8Unorm, 0.0),
                set.emissive.as_deref().and_then(|e| push(e, TextureFormat::Rgba8Srgb, 0.0)),
            ]
        });
        let preset = model.materials.iter().find(|(n, _)| n == key).map(|(_, p)| p);
        let (emissive_color, emissive_strength) = match preset {
            Some(p) if p.emissive > 0.0 => ([p.color[0], p.color[1], p.color[2]], p.emissive),
            _ => ([0.0; 3], 0.0),
        };
        x.materials.insert(
            key.clone(),
            PartMaterial {
                color: [1.0; 4],
                metallic: 1.0,
                roughness: 1.0,
                base: slots[0],
                normal: slots[1],
                normal_scale: 1.0,
                metallic_roughness: slots[2],
                emissive: slots[3],
                emissive_color,
                emissive_strength,
                alpha_cutoff: set.alpha_cutoff,
            },
        );
    }
}

/// The cutoff texture `t` is alpha-tested at, when every material that uses it as its base
/// colour is a `MASK` material with that one cutoff: then its mips keep the cut-out's coverage.
/// A texture that materials test differently (two cutoffs, or one of them opaque or blended) is
/// left with plain mips: one chain can't be right for both.
fn mask_cutoff(materials: &[specula_gltf::ImportedMaterial], t: usize) -> Option<f32> {
    let mut users = materials.iter().filter(|m| m.base_color_texture.is_some_and(|b| b.texture == t && b.tex_coord == 0));
    let first = users.next().filter(|m| m.alpha_mode == specula_gltf::AlphaMode::Mask)?;
    let cutoff = first.alpha_cutoff.clamp(0.0, 1.0);
    (cutoff > 0.0 && users.all(|m| m.alpha_mode == specula_gltf::AlphaMode::Mask && m.alpha_cutoff.clamp(0.0, 1.0) == cutoff)).then_some(cutoff)
}

/// A glTF sampler as the GPU's.
fn gltf_sampler(s: &specula_gltf::Sampler) -> TextureSampler {
    let nearest = |f: Option<u32>| f.is_some_and(|f| f == 9728 || f == 9984 || f == 9986);
    let wrap = |w: u32| match w {
        33071 => TextureWrap::ClampToEdge,
        33648 => TextureWrap::MirroredRepeat,
        _ => TextureWrap::Repeat,
    };
    let filter = |n: bool| if n { TextureFilter::Nearest } else { TextureFilter::Linear };
    // NEAREST_MIPMAP_NEAREST (9984) and LINEAR_MIPMAP_NEAREST (9985) step between mip levels.
    let mip_nearest = s.min_filter.is_some_and(|f| f == 9984 || f == 9985);
    TextureSampler {
        mag: filter(nearest(s.mag_filter)),
        min: filter(nearest(s.min_filter)),
        mipmap: filter(mip_nearest),
        wrap_u: wrap(s.wrap_s),
        wrap_v: wrap(s.wrap_t),
        anisotropy: true,
    }
}

/// sRGB from linear (for API colours).
fn linear_to_srgb(c: f32) -> f32 {
    let c = if c.is_nan() { 0.0 } else { c.clamp(0.0, 1.0) };
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Most warnings a build keeps.
const MAX_WARNINGS: usize = 16;

/// Imports a glTF asset: the model, its anim set, its material textures (mip chains made here)
/// and materials.
fn import_gltf(path: &Path) -> Result<(Model, Extras), RecipeError> {
    let opts = specula_gltf::ImportOptions { bake_texture_colors: false, ..Default::default() };
    let mut import = specula_gltf::import_file(path, &opts)
        .map_err(|e| RecipeError::new(&e.path, ErrorCode::Geometry, format!("glTF import: {e}")))?;
    let mut x = Extras { warnings: import.warnings.iter().take(MAX_WARNINGS).map(|w| format!("{}: {}", w.code, w.message)).collect(), ..Default::default() };
    match import.anim_set() {
        Some(Ok(set)) => x.anims = Some(set),
        Some(Err(e)) => x.anim_note = Some(format!("the asset's clips failed to load ({e}): drawn in its bind pose")),
        None => {}
    }
    // Textures by (glTF texture index, sRGB): a texture used as colour and as data uploads twice.
    // The image's decoded pixels become level 0 of the (last) chain made from them: no copy.
    let key = |t: Option<specula_gltf::TextureRef>, srgb: bool| t.filter(|t| t.tex_coord == 0).map(|t| (t.texture, srgb));
    let image_of = |texture: usize| -> Option<usize> {
        let i = import.textures.get(texture)?.image?;
        let i = import.images.get(i)?.duplicate_of.unwrap_or(i);
        (!import.images.get(i)?.rgba.is_empty()).then_some(i)
    };
    let mut wanted: Vec<(usize, bool)> = Vec::new();
    for m in &import.materials {
        let keys = [key(m.base_color_texture, true), key(m.normal_texture, false), key(m.metallic_roughness_texture, false), key(m.emissive_texture, true)];
        for k in keys.into_iter().flatten() {
            if !wanted.contains(&k) && image_of(k.0).is_some() {
                wanted.push(k);
            }
        }
    }
    let mut uses: BTreeMap<usize, usize> = BTreeMap::new();
    for &(t, _) in &wanted {
        *uses.entry(image_of(t).expect("checked")).or_default() += 1;
    }
    let samplers: Vec<TextureSampler> = wanted.iter().map(|&(t, _)| gltf_sampler(&import.textures[t].sampler)).collect();
    let mut images = std::mem::take(&mut import.images);
    let mut index: BTreeMap<(usize, bool), usize> = BTreeMap::new();
    for (&(t, srgb), sampler) in wanted.iter().zip(samplers) {
        let i = import.textures[t].image.map(|i| images[i].duplicate_of.unwrap_or(i)).expect("checked");
        let left = uses.get_mut(&i).expect("counted");
        *left -= 1;
        let (width, height) = (images[i].width, images[i].height);
        let rgba = if *left == 0 { std::mem::take(&mut images[i].rgba) } else { images[i].rgba.clone() };
        let format = if srgb { TextureFormat::Rgba8Srgb } else { TextureFormat::Rgba8Unorm };
        let Ok(mut levels) = specula_gpu::generate_mips_owned(width, height, format, rgba) else { continue };
        // A cut-out's base colour keeps its coverage with distance, as a baked set's does
        // (cards don't thin out and vanish): an imported cut-out is drawn like one made here.
        if let Some(cutoff) = srgb.then(|| mask_cutoff(&import.materials, t)).flatten() {
            specula_gpu::preserve_alpha_coverage(&mut levels, cutoff);
        }
        x.textures.push(TextureData { width, height, format, mips: levels.len(), levels: Mutex::new(levels), sampler });
        index.insert((t, srgb), x.textures.len() - 1);
    }
    drop(images);
    let tex = |t: Option<specula_gltf::TextureRef>, srgb: bool| key(t, srgb).and_then(|k| index.get(&k).copied());
    for m in &import.materials {
        let c = m.base_color;
        let alpha_cutoff = match m.alpha_mode {
            specula_gltf::AlphaMode::Mask => m.alpha_cutoff.clamp(0.0, 1.0),
            specula_gltf::AlphaMode::Blend if m.preset.kind == MaterialKind::Standard && m.base_color_texture.is_some() => {
                if x.warnings.len() < MAX_WARNINGS {
                    x.warnings.push(format!(
                        "material '{}' uses alpha BLEND: blended textured materials aren't supported, so it is alpha-masked at 0.5",
                        m.name
                    ));
                }
                0.5
            }
            _ => 0.0,
        };
        let pm = PartMaterial {
            color: [linear_to_srgb(c[0]), linear_to_srgb(c[1]), linear_to_srgb(c[2]), c[3]],
            metallic: m.metallic,
            roughness: m.roughness,
            base: tex(m.base_color_texture, true),
            normal: tex(m.normal_texture, false),
            normal_scale: m.normal_texture.map_or(1.0, |t| t.scale),
            metallic_roughness: tex(m.metallic_roughness_texture, false),
            emissive: tex(m.emissive_texture, true),
            emissive_color: m.emissive.map(linear_to_srgb),
            emissive_strength: if m.emissive.iter().any(|v| *v > 0.0) { m.emissive_strength.max(0.0) } else { 0.0 },
            alpha_cutoff,
        };
        x.materials.insert(m.name.clone(), pm);
    }
    Ok((import.model, x))
}

/// Whether an assets root holds models that need the GPU's skinned pipelines (anim sets, glTF
/// assets) and textured ones (glTF assets): looked for in the root and two folders down, so the
/// engine builds those pipelines while loading instead of at the first spawn.
pub fn asset_streams(root: &Path) -> (bool, bool) {
    fn walk(dir: &Path, depth: u32, found: &mut (bool, bool)) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if path.file_name().is_some_and(|n| n == "anims") {
                    found.0 = true;
                }
                if depth > 0 {
                    walk(&path, depth - 1, found);
                }
            } else if kind.is_file() && is_gltf_path(&path.to_string_lossy()) {
                *found = (true, true);
            } else if kind.is_file() && path.extension().is_some_and(|x| x == "json") && e.metadata().is_ok_and(|m| m.len() < 1 << 20) {
                // A recipe with procedural textures draws textured.
                if std::fs::read_to_string(&path).is_ok_and(|t| t.contains("\"textures\"")) {
                    found.1 = true;
                }
            }
            if *found == (true, true) {
                return;
            }
        }
    }
    let mut found = (false, false);
    walk(root, 2, &mut found);
    found
}

/// What a glTF source's key was made from (checked again by [`Library::poll`]).
struct GltfInfo {
    path: PathBuf,
    mtime: Option<SystemTime>,
    len: u64,
    content: u64,
}

/// Recipe files, styles, builds and handles (see the module docs).
pub struct Library {
    root: PathBuf,
    /// `root`, canonicalized (worked out when first needed: the folder may appear later).
    canonical_root: Option<PathBuf>,
    style_override: Option<PathBuf>,
    /// By folder, relative to the root. Only folders that exist.
    folders: BTreeMap<PathBuf, Folder>,
    /// By path.
    styles: BTreeMap<PathBuf, StyleFile>,
    /// By recipe path.
    recipes: BTreeMap<String, RecipeInfo>,
    /// Recipes that failed to parse (or whose style did), until their files change.
    failed: BTreeMap<String, WorldError>,
    /// glTF sources by path.
    gltfs: BTreeMap<String, GltfInfo>,
    builds: BTreeMap<BuildKey, Arc<Built>>,
    in_flight: BTreeMap<BuildKey, InFlight>,
    /// Build key → the handles waiting for it.
    waiting: BTreeMap<BuildKey, BTreeSet<u32>>,
    handles: Vec<Option<Handle>>,
    free_handles: Vec<u32>,
    by_spec: BTreeMap<String, u32>,
    /// Build ids handed out by `model_build` lately (kept even if no entity uses them).
    recent: VecDeque<u64>,
    next_id: u64,
    /// Bumped whenever a handle's build changes (the GPU cache re-syncs on it).
    generation: u64,
    last_poll: Instant,
    /// Counts scan plans and noted writes ([`Library::note_written`]). A scan planned before
    /// `scan_floor` (a newer scan was applied) is dropped; a file noted after a scan's plan
    /// (`notes`: its path and the note's place in the count) keeps the noted text over what that
    /// scan read.
    scan_seq: u64,
    scan_floor: u64,
    notes: BTreeMap<PathBuf, u64>,
    /// Per folder, the plan number of the read of it that was applied last: an older read of
    /// the same folder is never put over a newer one, whoever made it (a poll's scan, a build
    /// request's check). Reads are made without the lock and land in any order.
    folder_seq: BTreeMap<PathBuf, u64>,
    /// Jobs whose result a request made on a worker will ask for when it reaches the engine
    /// thread ([`Library::expect_claim`]: how many such requests each), and the results of
    /// those that finished before it got there. A result is kept only while a request is on
    /// its way, and goes when the last one has claimed it ([`Library::claim`]): a build no
    /// entity uses is never held here longer than that (it kept the last 64 requested builds
    /// alive, textures and all: review R3).
    claims: BTreeMap<u64, usize>,
    kept_results: Vec<(u64, Result<Arc<Built>, WorldError>)>,
    /// How long a re-key may run before the rest waits for [`Library::rekey_more`] (`None`: all
    /// at once, as tools and tests that answer with the reload need).
    rekey_slice: Option<Duration>,
    /// Forgotten recipes a sliced re-key left for the next pass, with their sources' contents
    /// from before they were forgotten (so their anim sets count as changed only if they did).
    rekey_left: BTreeMap<String, Option<u64>>,
    workers: Workers,
    pub cache_hits: u64,
    pub cache_misses: u64,
    /// The world's anim assets by model source (see [`crate::anim`]).
    anim_assets: Arc<crate::anim::Assets>,
    /// Per build: the assets' revision it was laid over with, the result and its problems.
    anim_overlays: RefCell<BTreeMap<u64, AnimOverlay>>,
    /// The anim sets graph writes settle against (see [`Library::write_set`]).
    writes: WriteSets,
}

/// Most anim sets kept for graph writes, bases and overlays together (rebuilt when needed again:
/// a cache, not state).
const MAX_WRITE_SETS: usize = 64;
/// Most bytes of clips they may hold together (about; see `AnimSet::approx_bytes`).
const MAX_WRITE_BYTES: usize = 64 << 20;
/// Most build keys remembered as aliases of a base (a map of small keys; cleared when full).
const MAX_WRITE_ALIASES: usize = 4096;

/// The anim sets graph writes settle against. A base is built from a rig and an anims file (or
/// a glTF's skeleton and clips) and shared by every build key that comes to the same ones (seed
/// and parameter variants of a recipe usually do); an overlay is a base with one revision of the
/// world's assets laid over it, shared the same way.
#[derive(Default)]
struct WriteSets {
    /// By base key: (rig hash, anims-file hash), or a glTF's content key.
    bases: BTreeMap<u64, Cached>,
    /// Build key → base key.
    aliases: BTreeMap<BuildKey, u64>,
    /// By (base key, asset revision).
    overlays: BTreeMap<(u64, u64), Cached>,
    /// Uses so far (the least recently used goes first).
    uses: u64,
}

struct Cached {
    used: u64,
    set: Result<Arc<AnimSet>, String>,
    bytes: usize,
}

impl WriteSets {
    fn tick(&mut self) -> u64 {
        self.uses += 1;
        self.uses
    }

    /// Drops the least recently used sets until there is room for one more of `bytes`.
    fn make_room(&mut self, bytes: usize) {
        loop {
            let count = self.bases.len() + self.overlays.len();
            let total: usize = self.bases.values().chain(self.overlays.values()).map(|c| c.bytes).sum();
            if count < MAX_WRITE_SETS && total + bytes <= MAX_WRITE_BYTES {
                return;
            }
            let oldest_base = self.bases.iter().min_by_key(|(_, c)| c.used).map(|(k, c)| (c.used, *k));
            let oldest_overlay = self.overlays.iter().min_by_key(|(_, c)| c.used).map(|(k, c)| (c.used, *k));
            match (oldest_base, oldest_overlay) {
                (Some((ub, kb)), Some((uo, _))) if ub <= uo => {
                    self.bases.remove(&kb);
                }
                (_, Some((_, ko))) => {
                    self.overlays.remove(&ko);
                }
                (Some((_, kb)), None) => {
                    self.bases.remove(&kb);
                }
                (None, None) => return,
            }
        }
    }
}

/// Builds a base anim set (on the calling thread: rig and clip generation are bounded by the
/// generators' budget, see `specula_anim::gen::MAX_SET_WORK`).
fn build_base(src: BaseSource) -> Result<Arc<AnimSet>, String> {
    match src {
        BaseSource::Gltf(path) => {
            let opts = specula_gltf::ImportOptions { decode_images: false, bake_texture_colors: false, ..Default::default() };
            let import = specula_gltf::import_file(&path, &opts).map_err(|e| e.to_string())?;
            match import.anim_set() {
                None => Err("the glTF asset has no skin".into()),
                Some(r) => r.map(Arc::new).map_err(|e| e.to_string()),
            }
        }
        BaseSource::Rig(rig, anims) => {
            let skel = Skeleton::from_rig(&rig).map_err(|e| e.to_string())?;
            match anims {
                Some(text) => AnimSet::load(&text, skel).map(Arc::new).map_err(|e| e.to_string()),
                None => Ok(Arc::new(AnimSet::new(skel))),
            }
        }
    }
}

/// What a base anim set is built from.
enum BaseSource {
    Rig(specula_model::Rig, Option<Arc<str>>),
    Gltf(PathBuf),
}

/// A build's anim set with the world's assets laid over it.
struct AnimOverlay {
    revision: u64,
    set: Arc<AnimSet>,
    problems: Vec<String>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::symlink_metadata(path).and_then(|m| m.modified()).ok()
}

/// How close to now a file's time may be and still not be taken as telling this read of it from
/// a later write. File times move in steps (the system clock's tick, 1 to 16 ms on Linux and
/// Windows; a whole second on HFS+, two on FAT), so a file written again within the step it was
/// read in keeps its time, and a cache that compares times alone never sees the second write.
/// A text read that soon after its file was written is compared by what it says, by every
/// scan, until one finds the time this old ([`stamp_of`]).
const FRESH: Duration = Duration::from_secs(2);

/// The clock [`stamp_of`] reads.
fn now() -> SystemTime {
    #[cfg(test)]
    if let Some(now) = NOW.get() {
        return now;
    }
    SystemTime::now()
}

/// Whether a file's `time` tells a read of it from any later write here: it was more than
/// [`FRESH`] old `before` the file was looked at, or is still more than that ahead `after`
/// (another clock's time: the next write here will not carry it). The clock is read on both
/// sides of the look, so a thread that stalled between the two (a busy machine, a laptop that
/// slept) takes a write that landed meanwhile for fresh, never for a time well ahead.
fn old_enough(time: SystemTime, before: SystemTime, after: SystemTime) -> bool {
    before.duration_since(time).is_ok_and(|d| d > FRESH) || time.duration_since(after).is_ok_and(|d| d > FRESH)
}

/// A file's time now, and that time as something a later scan may compare with: `None` (not
/// known: read the file again) while it is fresh ([`old_enough`]). **Called before the file is
/// read**, so what is read is never older than its stamp says.
fn stamp_of(path: &Path) -> (Option<SystemTime>, Option<SystemTime>) {
    let before = now();
    let time = mtime(path);
    let after = now();
    (time, time.filter(|t| old_enough(*t, before, after)))
}

#[cfg(test)]
thread_local! {
    /// Tests: what [`now`] answers on this thread, so a test says which file times
    /// are fresh by the times it gives the files, never by how fast it ran.
    static NOW: std::cell::Cell<Option<SystemTime>> = const { std::cell::Cell::new(None) };
}

/// A regular file (not a link, not a folder).
fn is_regular(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

fn not_found(spec: &ModelSpec, root: &Path) -> WorldError {
    WorldError::new("recipe_not_found", format!("no recipe file {} under the assets root {}", spec.recipe, root.display()))
        .with("field", "recipe")
        .with("recipe", spec.recipe.as_str())
        .with("assets_root", root.display().to_string())
}

/// A recipe error as a world error: code `recipe_invalid`, the recipe, and the build's own
/// structured error (`code`, `path` into the recipe, `message`, offsets and numbers).
pub fn recipe_error(recipe: &str, e: &RecipeError) -> WorldError {
    WorldError::new("recipe_invalid", format!("{recipe}: {e}"))
        .with("field", "recipe")
        .with("recipe", recipe)
        .with("recipe_error", e.to_json())
}

impl Library {
    /// `root`: the assets root; `style`: a style file for every recipe (otherwise looked up next
    /// to each recipe).
    pub fn new(root: &Path, style: Option<&Path>) -> Library {
        Library::with_workers(root, style, WORKERS)
    }

    /// With `workers` build threads (0 in tests that deliver results by hand).
    fn with_workers(root: &Path, style: Option<&Path>, workers: usize) -> Library {
        Library {
            root: root.to_owned(),
            canonical_root: None,
            style_override: style.map(Path::to_owned),
            folders: BTreeMap::new(),
            styles: BTreeMap::new(),
            recipes: BTreeMap::new(),
            failed: BTreeMap::new(),
            gltfs: BTreeMap::new(),
            builds: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            waiting: BTreeMap::new(),
            handles: Vec::new(),
            free_handles: Vec::new(),
            by_spec: BTreeMap::new(),
            recent: VecDeque::new(),
            next_id: 1,
            generation: 0,
            last_poll: Instant::now(),
            scan_seq: 0,
            scan_floor: 0,
            notes: BTreeMap::new(),
            folder_seq: BTreeMap::new(),
            claims: BTreeMap::new(),
            kept_results: Vec::new(),
            rekey_slice: None,
            rekey_left: BTreeMap::new(),
            workers: Workers::start(workers, if workers > 0 { HEAVY_WORKERS } else { 0 }),
            cache_hits: 0,
            cache_misses: 0,
            anim_assets: Arc::new(crate::anim::Assets::new()),
            anim_overlays: RefCell::new(BTreeMap::new()),
            writes: WriteSets::default(),
        }
    }

    /// Takes the world's anim assets (overlays are laid again when next asked for).
    pub fn set_anim_assets(&mut self, assets: Arc<crate::anim::Assets>) {
        self.anim_assets = assets;
        self.anim_overlays.get_mut().clear();
    }

    /// The world's anim assets for a model source.
    pub fn anim_assets_for(&self, source: &str) -> Option<&crate::anim::AssetSet> {
        self.anim_assets.get(source)
    }

    /// The anim set entities of `handle` play: its build's own, with the world's assets for its
    /// recipe laid over it (a rigged build without an anims file gets one from its rig). `None`
    /// until built, or for builds without a rig.
    pub fn anim_set(&self, handle: u32) -> Option<Arc<AnimSet>> {
        let assets = Arc::clone(&self.anim_assets);
        self.anim_set_with(handle, &assets)
    }

    /// [`Library::anim_set`] with `assets` (every model source's) in place of the ones the
    /// engine last handed over.
    pub fn anim_set_with(&self, handle: u32, assets: &crate::anim::Assets) -> Option<Arc<AnimSet>> {
        let built = self.built(handle)?;
        let source = &self.handle_spec(handle)?.recipe;
        let Some(assets) = assets.get(source.as_str()) else { return built.anims.clone() };
        let mut cache = self.anim_overlays.borrow_mut();
        if let Some(o) = cache.get(&built.id).filter(|o| o.revision == assets.revision) {
            return Some(Arc::clone(&o.set));
        }
        let base = match &built.anims {
            Some(set) => Arc::clone(set),
            None => Arc::new(AnimSet::new(Skeleton::from_rig(built.model.rig.as_ref()?).ok()?)),
        };
        let (set, problems) = base.with_overlay(&assets.clips, &assets.graphs);
        let problems = problems.iter().map(|p| format!("{} '{}': {}", p.kind, p.name, p.error)).collect();
        let set = Arc::new(set);
        cache.insert(built.id, AnimOverlay { revision: assets.revision, set: Arc::clone(&set), problems });
        Some(set)
    }

    /// The anim set a graph write to an entity of `handle` settles against: the recipe's rig and
    /// anims file as the library knows them now (a glTF's skeleton and clips), with `assets` laid
    /// over it. Built on the spot, without the mesh (the rig needs no geometry), and shared by
    /// every build key with the same rig and anims file (seeded variants), so it never waits for a
    /// build and never depends on whether one has landed: the
    /// library's knowledge of the files changes only when it polls them (the engine then
    /// re-settles the plays the change touches, see [`Reload::anim_handles`]). `None` without a
    /// rig, or when the rig or anim set fails to load (the write then only merges parameters).
    pub fn write_set(&mut self, handle: u32, assets: &crate::anim::Assets) -> Option<Arc<AnimSet>> {
        let spec = self.handles.get(handle as usize)?.as_ref()?.spec.clone();
        let key = self.key_for(&spec, false, true).ok()?;
        let used = self.writes.tick();
        // The base: by alias when this build key was seen, else from its source's key.
        let mut source = None;
        let base_key = match self.writes.aliases.get(&key) {
            Some(b) => *b,
            None => {
                let (b, src) = self.base_source(&spec, &key).ok()?;
                if self.writes.aliases.len() >= MAX_WRITE_ALIASES {
                    self.writes.aliases.clear();
                }
                self.writes.aliases.insert(key.clone(), b);
                source = Some(src);
                b
            }
        };
        if !self.writes.bases.contains_key(&base_key) {
            let src = match source {
                Some(s) => s,
                None => self.base_source(&spec, &key).ok()?.1,
            };
            let set = build_base(src);
            let bytes = set.as_ref().map_or(0, |s| s.approx_bytes());
            self.writes.make_room(bytes);
            self.writes.bases.insert(base_key, Cached { used, set, bytes });
        }
        let base = self.writes.bases.get_mut(&base_key).expect("just inserted");
        base.used = used;
        let base = Arc::clone(base.set.as_ref().ok()?);
        let Some(a) = assets.get(spec.recipe.as_str()) else { return Some(base) };
        let ok = (base_key, a.revision);
        if let Some(c) = self.writes.overlays.get_mut(&ok) {
            c.used = used;
            return c.set.as_ref().ok().map(Arc::clone);
        }
        let (set, _) = base.with_overlay(&a.clips, &a.graphs);
        let set = Arc::new(set);
        let bytes = set.approx_bytes();
        self.writes.make_room(bytes);
        self.writes.overlays.insert(ok, Cached { used, set: Ok(Arc::clone(&set)), bytes });
        Some(set)
    }

    /// What `spec`'s base anim set (build key `key`) is built from, and its base key: the rig
    /// (built without geometry) and the anims file, or the glTF file as the library last polled
    /// it (a file changed since is refused until the next poll announces it).
    fn base_source(&self, spec: &ModelSpec, key: &BuildKey) -> Result<(u64, BaseSource), String> {
        if is_gltf_path(&spec.recipe) {
            let g = self.gltfs.get(&spec.recipe).filter(|g| g.content == key.0).ok_or("the glTF file changed")?;
            let meta = std::fs::symlink_metadata(&g.path).map_err(|e| e.to_string())?;
            if meta.len() != g.len || meta.modified().ok() != g.mtime {
                return Err("the glTF file changed since the library last looked at it: it takes effect at the next poll".into());
            }
            return Ok((g.content, BaseSource::Gltf(g.path.clone())));
        }
        let info = self.recipes.get(&spec.recipe).filter(|i| i.content == key.0).ok_or("the recipe changed")?;
        let values: Vec<(&str, f64)> = spec.params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let params = BuildParams { values: &values, seed: spec.seed, library: Some(&info.library), ..BuildParams::new(&info.style) };
        let rig = specula_model::recipe::build_rig_only(&info.recipe, &params).map_err(|e| e.to_string())?.ok_or("the recipe has no rig")?;
        let mut h = StableHasher::with_seed(0x616e_696d_6261_7365);
        rig.hash_into(&mut h);
        match &info.anims {
            Some(t) => {
                h.write_u8(1);
                h.write_str(t);
            }
            None => h.write_u8(0),
        }
        Ok((h.finish(), BaseSource::Rig(rig, info.anims.clone())))
    }

    /// The named parts of a recipe the library has read (parsed already: no disk, no parse
    /// here), in recipe order, at most `max`; `None` for a recipe it hasn't read (or a glTF).
    pub fn part_names(&self, recipe: &str, max: usize) -> Option<Vec<String>> {
        self.recipes.get(recipe).map(|i| i.recipe.part_names().take(max).map(str::to_owned).collect())
    }

    /// The sweep parts of a recipe the library has read, by name, in recipe order (at most
    /// `max`): what the editor's path tool can sketch. `None` as [`Library::part_names`].
    pub fn sweep_names(&self, recipe: &str, max: usize) -> Option<Vec<String>> {
        self.recipes.get(recipe).map(|i| i.recipe.sweep_parts().take(max).map(str::to_owned).collect())
    }

    /// The content key of a source as the library knows it now (its recipe info or glTF file).
    fn source_content(&self, recipe: &str) -> Option<u64> {
        self.recipes.get(recipe).map(|i| i.content).or_else(|| self.gltfs.get(recipe).map(|g| g.content))
    }

    /// What the world's assets for `handle`'s recipe got wrong (left out of its anim set).
    pub fn anim_overlay_problems(&self, handle: u32) -> Vec<String> {
        let _ = self.anim_set(handle);
        let Some(built) = self.built(handle) else { return Vec::new() };
        self.anim_overlays.borrow().get(&built.id).map(|o| o.problems.clone()).unwrap_or_default()
    }

    /// A live handle whose spec names `source` and whose build is ready (the first by handle).
    pub fn handle_for_source(&self, source: &str) -> Option<u32> {
        (0..self.handles.len() as u32).find(|&h| self.handle_spec(h).is_some_and(|s| s.recipe == source) && self.built(h).is_some())
    }

    /// `--style FILE`, when the engine was started with one (it governs every recipe).
    pub fn style_override(&self) -> Option<&Path> {
        self.style_override.as_deref()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The build a handle draws (its last good one).
    pub fn built(&self, handle: u32) -> Option<&Arc<Built>> {
        self.handles.get(handle as usize)?.as_ref()?.built.as_ref()
    }

    /// The latest error of a handle (its build failed, or its recipe broke after it built once).
    pub fn handle_error(&self, handle: u32) -> Option<&WorldError> {
        self.handles.get(handle as usize)?.as_ref()?.error.as_ref()
    }

    pub fn handle_spec(&self, handle: u32) -> Option<&ModelSpec> {
        self.handles.get(handle as usize)?.as_ref().map(|h| &h.spec)
    }

    /// Specs and handles in use, builds cached and in flight, folders cached.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (self.handles.iter().flatten().count(), self.builds.len(), self.in_flight.len(), self.folders.len())
    }

    /// Builds queued or running.
    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// Specs with a handle.
    pub fn spec_count(&self) -> usize {
        self.by_spec.len()
    }

    /// Makes the next unforced [`Library::poll`] look at the files (tests).
    #[cfg(test)]
    fn poll_due_now(&mut self) {
        self.last_poll = Instant::now().checked_sub(POLL_EVERY).unwrap_or(self.last_poll);
    }

    /// The spec's handle, state and bounds (see the module docs: never builds on this thread).
    pub fn resolve(&mut self, spec: &ModelSpec) -> Result<ResolvedModel, WorldError> {
        if let Some(&h) = self.by_spec.get(&spec.key()) {
            let slot = self.handles[h as usize].as_mut().expect("indexed handles exist");
            if slot.verify {
                // Unused for a while: its recipe may have been edited (or broken, or fixed)
                // meanwhile, when nothing rebuilt it. Same key: the build stands.
                slot.verify = false;
                match self.key_for(spec, true, true) {
                    Ok(key) => {
                        let slot = self.handles[h as usize].as_ref().expect("live handle");
                        if slot.built_key.as_ref() != Some(&key) && slot.want.as_ref() != Some(&key) {
                            self.want(h, key, false);
                        } else if slot.built_key.as_ref() == Some(&key) {
                            self.handles[h as usize].as_mut().expect("live handle").error = None;
                        }
                    }
                    Err(e) => {
                        let slot = self.handles[h as usize].as_mut().expect("live handle");
                        if slot.built.is_none() {
                            return Err(e);
                        }
                        slot.error = Some(e); // the last good build stays
                    }
                }
            }
            return Ok(self.handles[h as usize].as_ref().expect("indexed handles exist").resolved(h));
        }
        let key = self.key_for(spec, true, true)?;
        let h = match self.free_handles.pop() {
            Some(h) => h,
            None => {
                self.handles.push(None);
                (self.handles.len() - 1) as u32
            }
        };
        self.handles[h as usize] =
            Some(Handle { spec: spec.clone(), built: None, built_key: None, want: None, error: None, verify: false });
        self.by_spec.insert(spec.key(), h);
        self.want(h, key, false);
        Ok(self.handles[h as usize].as_ref().expect("just set").resolved(h))
    }

    /// `model_build`: the spec built fresh from disk (its folder re-read now), from the cache or
    /// queued.
    ///
    /// The disk is read here, under whatever lock the caller holds (a `stat` of every file of
    /// the folder, and reads of what changed): for tests and code with a library of its own.
    /// Around the shared library use [`SharedLibrary::request`] (a caller's own thread) or
    /// [`SharedLibrary::learn`] and [`Library::request_known`] (the engine thread).
    pub fn request(&mut self, spec: &ModelSpec) -> Result<BuildRequest, WorldError> {
        spec.check()?;
        let folder = Path::new(&spec.recipe).parent().unwrap_or(Path::new("")).to_owned();
        self.refresh_folder(&folder);
        // A glTF source is looked at afresh (its size and time).
        self.forget_gltf(&spec.recipe);
        // retry: a recipe new on disk is found even when its folder's time didn't move.
        let key = self.key_for(spec, true, true)?;
        Ok(self.request_key(spec, key))
    }

    /// The build for a key: from the cache, or queued.
    fn request_key(&mut self, spec: &ModelSpec, key: BuildKey) -> BuildRequest {
        if let Some(b) = self.builds.get(&key) {
            self.cache_hits += 1;
            let b = Arc::clone(b);
            self.remember(b.id);
            return BuildRequest::Ready(b, true);
        }
        BuildRequest::Pending(self.submit(&spec.recipe, &key, spec, true))
    }

    /// What a build request for `spec` reads from the disk, planned from the caches (no disk;
    /// under the lock for a moment): [`RequestPlan::run`] reads without the lock and
    /// [`Library::request_read`] applies it and answers. Its folder is checked if it is cached,
    /// read whole if it is not, and read again whatever its times say when it is cached without
    /// the recipe (a file renamed into a folder does not always move the folder's time); a glTF
    /// source's size and time are read.
    ///
    /// A request did all of that under the lock (a `stat` of every file of the folder, on
    /// every `studio_check`, `style_check` and `model_build`): 847 holds of 2 ms and more a run
    /// of `scripts/recipe_stall_check.py` by agents' connections beside 32 busy processes, and
    /// the engine thread waited for the lock 30 to 50 ms a frame (performance pass, 2026-11-14).
    pub fn request_plan(&mut self, spec: &ModelSpec) -> RequestPlan {
        self.scan_seq += 1;
        let mut scan = ScanPlan {
            seq: self.scan_seq,
            root: self.root.clone(),
            canonical_root: self.canonical_root.clone(),
            folders: Vec::new(),
            anims: Vec::new(),
            gltfs: Vec::new(),
            styles: Vec::new(),
            missing: Vec::new(),
        };
        let rel = Path::new(&spec.recipe);
        let folder = rel.parent().unwrap_or(Path::new("")).to_owned();
        let gltf = is_gltf_path(&spec.recipe);
        if !gltf {
            let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            match self.folders.get(&folder) {
                None => scan.missing.push(folder),
                Some(cached) => {
                    let mut old = cached.clone();
                    if !old.files.contains_key(stem) {
                        // (No time matches: the folder is read again.)
                        old.mtime = None;
                    }
                    scan.folders.push((folder, old));
                }
            }
        }
        let from_cache = !scan.folders.is_empty();
        RequestPlan { scan, gltf: gltf.then(|| spec.recipe.clone()), from_cache }
    }

    /// Applies what a request's plan read: its folder (never over a newer read of it, and
    /// files the store wrote since the plan keep the cache's text), a glTF source's size and
    /// time. An error when the source leads outside the assets root.
    ///
    /// `Err(Stale)`: the read was planned from a cached folder that is no longer cached (a
    /// collection dropped it while the disk was read), so it found only what changed, and
    /// nothing here knows the folder now. The request reads again, whole
    /// ([`SharedLibrary::request`]); answering "not found" from it would say of a recipe that is
    /// on the disk that it is not, when a collection ran at the wrong moment (review R2).
    fn apply_request_read(&mut self, spec: &ModelSpec, read: RequestRead) -> Result<(), ReadError> {
        let RequestRead { scanned, gltf, from_cache } = read;
        let folder = Path::new(&spec.recipe).parent().unwrap_or(Path::new(""));
        let outside = scanned.outside.iter().any(|f| f == folder);
        self.apply_folder_scan(scanned);
        if from_cache && !self.folders.contains_key(folder) && !self.recipes.contains_key(&spec.recipe) {
            return Err(ReadError::Stale);
        }
        match gltf {
            Some(Ok(info)) => {
                if self.gltfs.get(&spec.recipe).is_none_or(|g| g.content != info.content) {
                    self.forget_gltf(&spec.recipe);
                }
                self.gltfs.insert(spec.recipe.clone(), info);
            }
            Some(Err(GltfMiss::Outside)) => return Err(ReadError::Is(outside_assets(spec))),
            Some(Err(GltfMiss::NotFound)) => self.forget_gltf(&spec.recipe),
            None => {}
        }
        if outside {
            return Err(ReadError::Is(outside_assets(spec)));
        }
        Ok(())
    }

    /// [`Library::request`] with what its plan read from the disk, applied and answered in one
    /// hold of the lock (no other read lands in between). The folder is not looked at on disk
    /// here; working out a recipe this library has not seen may look for its style and anim
    /// set (a few files): for a caller's own thread or a worker, never the engine's.
    /// `Err(Stale)`: read again (see [`Library::apply_request_read`]).
    pub fn request_read(&mut self, spec: &ModelSpec, read: RequestRead) -> Result<BuildRequest, ReadError> {
        spec.check()?;
        self.apply_request_read(spec, read)?;
        let key = self.key_for(spec, false, false)?;
        Ok(self.request_key(spec, key))
    }

    /// A build request of a source the library **knows** (an entity uses it, or a request
    /// worked it out), without the disk: for the engine thread. `None`: not known; the caller
    /// asks the old way ([`Library::request`]), which reads the disk.
    pub fn request_known(&mut self, spec: &ModelSpec) -> Option<Result<BuildRequest, WorldError>> {
        if let Err(e) = spec.check() {
            return Some(Err(e));
        }
        if !self.known(&spec.recipe) {
            return None;
        }
        Some(self.key_for(spec, false, false).map(|key| self.request_key(spec, key)))
    }

    /// The texts `spec` would build from now: its recipe, the recipes it includes and its style
    /// file's path, its folder re-read as [`Library::request`] does; what a recorded check of it
    /// is compared with (`crate::records`). Reads the disk under the caller's lock: tests and
    /// private libraries ([`SharedLibrary::current_texts`] reads without the lock). A glTF asset
    /// has no texts (`not_a_recipe`).
    pub fn current_texts(&mut self, spec: &ModelSpec) -> Result<RecipeTexts, WorldError> {
        spec.check()?;
        let folder = Path::new(&spec.recipe).parent().unwrap_or(Path::new("")).to_owned();
        self.refresh_folder(&folder);
        self.texts_for(spec, true)
    }

    /// [`Library::current_texts`] with what its plan read from the disk ([`Library::request_plan`]).
    pub fn current_texts_read(&mut self, spec: &ModelSpec, read: RequestRead) -> Result<RecipeTexts, ReadError> {
        spec.check()?;
        self.apply_request_read(spec, read)?;
        Ok(self.texts_for(spec, false)?)
    }

    fn texts_for(&mut self, spec: &ModelSpec, disk: bool) -> Result<RecipeTexts, WorldError> {
        if is_gltf_path(&spec.recipe) {
            return Err(WorldError::new("not_a_recipe", format!("{} is a glTF asset, not a recipe", spec.recipe)).with("recipe", spec.recipe.as_str()));
        }
        let key = self.key_for(spec, disk, disk)?;
        self.cached_texts(spec, key.0).ok_or_else(|| WorldError::new("recipe_unreadable", format!("{} wasn't kept after reading", spec.recipe)).with("recipe", spec.recipe.as_str()))
    }

    /// Caches a folder read just now, under the lock: it is newer than every read planned so
    /// far, and no older read is put over it ([`Library::folder_seq`]).
    fn cache_folder_read_now(&mut self, rel: &Path, folder: Folder) {
        self.scan_seq += 1;
        self.folder_seq.insert(rel.to_owned(), self.scan_seq);
        self.folders.insert(rel.to_owned(), folder);
    }

    /// Forgets the cached recipes `gone` picks: their keys are worked out again when asked for.
    /// Those a handle uses are left for the engine's next re-key ([`Library::rekey_more`]), with
    /// their contents as they were: a request that found a change on disk makes the recipe
    /// known again at once, and nothing else would ever re-key the entities that use it (they
    /// kept their old build for good: review R1 of m5-liblock, there before it).
    fn forget(&mut self, gone: impl Fn(&str, &RecipeInfo) -> bool) {
        let keys: Vec<String> = self.recipes.iter().filter(|(k, r)| gone(k, r)).map(|(k, _)| k.clone()).collect();
        for key in keys {
            self.leave_for_rekey(&key);
            self.recipes.remove(&key);
        }
    }

    /// A glTF source's size and time are forgotten (its key is worked out again), its handles
    /// left for the next re-key as [`Library::forget`] leaves a recipe's.
    fn forget_gltf(&mut self, recipe: &str) {
        self.leave_for_rekey(recipe);
        self.gltfs.remove(recipe);
    }

    fn leave_for_rekey(&mut self, recipe: &str) {
        if self.known(recipe) && self.handles.iter().flatten().any(|s| s.spec.recipe == recipe) {
            let was = self.source_content(recipe);
            self.rekey_left.entry(recipe.to_owned()).or_insert(was);
        }
    }

    /// The texts the build with content hash `content` was made from, as the library last read
    /// them (no disk); `None` when the library's copy is another version (or a glTF asset).
    pub fn cached_texts(&self, spec: &ModelSpec, content: u64) -> Option<RecipeTexts> {
        let info = self.recipes.get(&spec.recipe).filter(|i| i.content == content)?;
        Some(RecipeTexts { recipe: info.text.to_string(), library: (*info.library).clone(), style_path: info.style_path.clone() })
    }

    /// Points handle `h` at the build for `key`: at once from the cache, or once it is built.
    fn want(&mut self, h: u32, key: BuildKey, count_hit: bool) {
        if let Some(b) = self.builds.get(&key).cloned() {
            self.set_want(h, None);
            let slot = self.handles[h as usize].as_mut().expect("live handle");
            if slot.built.as_ref().is_none_or(|old| old.id != b.id) {
                slot.built = Some(b);
                self.generation += 1;
            }
            slot.built_key = Some(key);
            slot.error = None;
            if count_hit {
                self.cache_hits += 1;
            }
            return;
        }
        self.set_want(h, Some(key.clone()));
        let spec = self.handles[h as usize].as_ref().expect("live handle").spec.clone();
        self.submit(&spec.recipe, &key, &spec, false);
    }

    /// Sets what handle `h` waits for, keeping the key → handles index in step.
    fn set_want(&mut self, h: u32, key: Option<BuildKey>) {
        let slot = self.handles[h as usize].as_mut().expect("live handle");
        if let Some(old) = slot.want.take() {
            if let Some(set) = self.waiting.get_mut(&old) {
                set.remove(&h);
                if set.is_empty() {
                    self.waiting.remove(&old);
                }
            }
        }
        if let Some(k) = key {
            self.waiting.entry(k.clone()).or_default().insert(h);
            slot.want = Some(k);
        }
    }

    /// Queues the build for `key` unless it is already in flight; returns its job id.
    fn submit(&mut self, recipe: &str, key: &BuildKey, spec: &ModelSpec, pin: bool) -> u64 {
        if let Some(f) = self.in_flight.get_mut(key) {
            f.pinned |= pin;
            return f.job;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.cache_misses += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut heavy = false;
        let job = match self.gltfs.get(recipe) {
            Some(g) => Job {
                id,
                key: key.clone(),
                recipe: recipe.to_owned(),
                text: Arc::from(""),
                library: Arc::new(BTreeMap::new()),
                style: Arc::new(Style::default()),
                style_path: PathBuf::new(),
                params: Vec::new(),
                seed: None,
                gltf: Some(g.path.clone()),
                anims: None,
                cancel: Arc::clone(&cancel),
                trace: specula_core::trace::Ctx::current(),
            },
            None => {
                let info = &self.recipes[recipe];
                heavy = info.heavy;
                Job {
                    id,
                    key: key.clone(),
                    recipe: recipe.to_owned(),
                    text: Arc::clone(&info.text),
                    library: Arc::clone(&info.library),
                    style: Arc::clone(&info.style),
                    style_path: info.style_path.clone(),
                    params: spec.params.clone(),
                    seed: spec.seed,
                    gltf: None,
                    anims: info.anims.clone(),
                    cancel: Arc::clone(&cancel),
                    trace: specula_core::trace::Ctx::current(),
                }
            }
        };
        self.in_flight.insert(key.clone(), InFlight { job: id, pinned: pin });
        self.workers.flags.lock().unwrap_or_else(PoisonError::into_inner).insert(id, cancel);
        self.workers.push(job, heavy);
        id
    }

    /// The build key for `spec`: everything checkable without building is checked here.
    /// `retry`: a recipe missing from its cached folder is looked for again (it may be new).
    ///
    /// `disk`: the folder may be read here (one not cached is read whole; with `retry`, one
    /// cached without the recipe is read again). Without it the caches are all there is: what
    /// they lack is not found, as after a request's own read ([`Library::request_plan`]).
    fn key_for(&mut self, spec: &ModelSpec, retry: bool, disk: bool) -> Result<BuildKey, WorldError> {
        spec.check()?;
        if is_gltf_path(&spec.recipe) {
            return self.gltf_key(spec, disk);
        }
        let rel = Path::new(&spec.recipe);
        let folder = rel.parent().unwrap_or(Path::new("")).to_owned();
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        if !disk {
            // A known recipe needs nothing of its folder; an unknown one needs it cached.
            if !self.recipes.contains_key(&spec.recipe) && !self.folders.contains_key(&folder) {
                return Err(not_found(spec, &self.root));
            }
        } else if !self.folders.contains_key(&folder) {
            // Cached folders were checked when read (and are checked again when re-read).
            self.check_inside(&folder, spec)?;
            // Folders that don't exist are never cached (agents could make the cache grow).
            let f = self.read_folder(&folder).ok_or_else(|| not_found(spec, &self.root))?;
            self.cache_folder_read_now(&folder, f);
        } else if retry && !self.folders[&folder].files.contains_key(&stem) {
            self.refresh_folder(&folder);
            // A folder's time doesn't always move when a file is renamed into it (project_write
            // writes a temporary file and renames it into place; seen on Windows in the M4 exit
            // run, where four builders' new recipes stayed recipe_not_found): a recipe that is on
            // disk but missing from the cached folder is read afresh.
            if !self.folders[&folder].files.contains_key(&stem) && is_regular(&self.root.join(rel)) {
                if let Some(f) = self.read_folder_from(&folder, self.folders.get(&folder)) {
                    self.cache_folder_read_now(&folder, f);
                }
            }
        }
        if !self.recipes.contains_key(&spec.recipe) {
            // A recipe that failed to parse fails the same way until its files change.
            if let Some(e) = self.failed.get(&spec.recipe) {
                return Err(e.clone());
            }
            match self.recipe_info(&spec.recipe, &folder, &stem, spec) {
                Ok(info) => {
                    self.recipes.insert(spec.recipe.clone(), info);
                }
                Err(e) => {
                    if !matches!(e.code, "recipe_not_found" | "recipe_unreadable" | "style_not_found") {
                        self.failed.insert(spec.recipe.clone(), e.clone());
                    }
                    return Err(e);
                }
            }
        }
        let info = &self.recipes[&spec.recipe];
        let values: Vec<(&str, f64)> = spec.params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        info.recipe.check_values(&values).map_err(|e| recipe_error(&spec.recipe, &e))?;
        let params: String = spec.params.iter().map(|(k, v)| format!("{k}={:016x};", v.to_bits())).collect();
        Ok((info.content, params, spec.seed))
    }

    /// A glTF source's key: its path, size and modification time (no reading: spawns stay cheap).
    fn gltf_key(&mut self, spec: &ModelSpec, disk: bool) -> Result<BuildKey, WorldError> {
        if !spec.params.is_empty() || spec.seed.is_some() {
            let field = if spec.params.is_empty() { "seed" } else { "params" };
            return Err(WorldError::new("invalid_value", format!("{} is a glTF asset: it takes no params and no seed", spec.recipe))
                .with("field", field)
                .with("recipe", spec.recipe.as_str()));
        }
        if let Some(g) = self.gltfs.get(&spec.recipe) {
            return Ok((g.content, String::new(), None));
        }
        if !disk {
            return Err(not_found(spec, &self.root));
        }
        let rel = Path::new(&spec.recipe);
        let folder = rel.parent().unwrap_or(Path::new("")).to_owned();
        self.check_inside(&folder, spec)?;
        let info = gltf_info(&self.root, &spec.recipe).ok_or_else(|| not_found(spec, &self.root))?;
        let content = info.content;
        self.gltfs.insert(spec.recipe.clone(), info);
        Ok((content, String::new(), None))
    }

    /// The source's key inputs are known (a recipe's info or a glTF file's size and time).
    fn known(&self, recipe: &str) -> bool {
        self.recipes.contains_key(recipe) || self.gltfs.contains_key(recipe)
    }

    /// Where a recipe's anim set lives: `anims/<stem>.json` beside the recipe's folder (or inside
    /// it, for a recipe at the assets root).
    fn anims_path(&self, folder: &Path, stem: &str) -> PathBuf {
        self.root.join(folder.parent().unwrap_or(Path::new(""))).join("anims").join(format!("{stem}.json"))
    }

    /// Reads, parses and hashes a recipe with its transitive includes and style.
    fn recipe_info(&mut self, recipe: &str, folder: &Path, stem: &str, spec: &ModelSpec) -> Result<RecipeInfo, WorldError> {
        let files = &self.folders[folder].files;
        let text = match files.get(stem).map(|f| &f.text) {
            Some(Ok(t)) => Arc::clone(t),
            Some(Err(e)) => {
                return Err(WorldError::new("recipe_unreadable", format!("{recipe}: {e}")).with("field", "recipe").with("recipe", recipe))
            }
            None => return Err(not_found(spec, &self.root)),
        };
        let style_path = self.style_path(folder).ok_or_else(|| {
            WorldError::new(
                "style_not_found",
                format!(
                    "no {STYLE_FILE} next to {recipe} or above it inside the assets root {}; add one or start the engine with --style",
                    self.root.display()
                ),
            )
            .with("field", "recipe")
            .with("recipe", recipe)
        })?;
        let (style, style_hash) = self.style(&style_path, recipe)?;
        let parsed = Recipe::parse(&text, &style).map_err(|e| recipe_error(recipe, &e))?;
        // The files it includes, transitively; a missing one is left for the build to report.
        let files = &self.folders[folder].files;
        let mut library = BTreeMap::new();
        let mut includes = BTreeSet::new();
        let mut todo = parsed.includes();
        while let Some(name) = todo.pop() {
            if !includes.insert(name.clone()) || library.contains_key(&name) {
                continue;
            }
            let Some(Ok(t)) = files.get(&name).map(|f| &f.text) else { continue };
            if let Ok(inner) = Recipe::parse(t, &style) {
                todo.extend(inner.includes());
            }
            library.insert(name, t.to_string());
        }
        let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0001);
        h.write_str(&text);
        h.write_u64(library.len() as u64);
        for (name, t) in &library {
            h.write_str(name);
            h.write_str(t);
        }
        h.write_u64(style_hash);
        // A rig's anim set is part of the build (generated clips depend on the skeleton).
        let anims_path = self.anims_path(folder, stem);
        let (_, anims_mtime) = stamp_of(&anims_path);
        let anims = is_regular(&anims_path)
            .then(|| std::fs::read_to_string(&anims_path).ok())
            .flatten()
            .map(|t| (Arc::<str>::from(t), anims_path.clone()));
        match &anims {
            Some((t, _)) => {
                h.write_str("anims");
                h.write_str(t);
            }
            None => h.write_u8(0),
        }
        let heavy = procedural(&text) || library.values().any(|t| procedural(t));
        Ok(RecipeInfo {
            folder: folder.to_owned(),
            content: h.finish(),
            heavy,
            text,
            library: Arc::new(library),
            includes,
            style,
            style_path,
            recipe: parsed,
            anims_path,
            anims_mtime,
            anims: anims.map(|(t, _)| t),
        })
    }

    /// Refuses a folder that leads outside the assets root (through a link, say).
    /// Whether `folder` exists inside the assets root (links resolved).
    fn folder_inside(&mut self, folder: &Path) -> bool {
        if self.canonical_root.is_none() {
            self.canonical_root = self.root.canonicalize().ok();
        }
        let Some(root) = &self.canonical_root else { return false };
        self.root.join(folder).canonicalize().is_ok_and(|full| full.starts_with(root))
    }

    fn check_inside(&mut self, folder: &Path, spec: &ModelSpec) -> Result<(), WorldError> {
        if self.canonical_root.is_none() {
            self.canonical_root = self.root.canonicalize().ok();
        }
        let Some(root) = &self.canonical_root else {
            return Err(not_found(spec, &self.root)); // no assets root at all (yet)
        };
        match self.root.join(folder).canonicalize() {
            Ok(full) if full.starts_with(root) => Ok(()),
            Ok(_) => Err(WorldError::new("recipe_outside_assets", format!("{} leads outside the assets root", spec.recipe))
                .with("field", "recipe")
                .with("recipe", spec.recipe.as_str())),
            Err(_) => Err(not_found(spec, &self.root)),
        }
    }

    /// The folder's regular `*.json` files (links are not followed); `None` if it doesn't exist.
    fn read_folder(&self, rel: &Path) -> Option<Folder> {
        self.read_folder_from(rel, None)
    }

    /// Like [`Library::read_folder`], reusing files from `old` whose modification time is the same.
    fn read_folder_from(&self, rel: &Path, old: Option<&Folder>) -> Option<Folder> {
        read_folder_at(&self.root, rel, old)
    }

    /// Re-reads one cached folder when it changed; forgets the recipes there whose text changed.
    /// Returns how many files changed (a folder that vanished counts its files).
    fn refresh_folder(&mut self, rel: &Path) -> usize {
        let Some(old) = self.folders.get(rel) else { return 0 };
        let dir = self.root.join(rel);
        let unchanged = old.mtime.is_some()
            && mtime(&dir) == old.mtime
            && old.files.iter().all(|(stem, f)| f.mtime.is_some() && mtime(&dir.join(format!("{stem}.json"))) == f.mtime);
        if unchanged {
            return 0;
        }
        // Re-read only while it still lies inside the root (it may have become a link).
        let new = if self.folder_inside(rel) { self.read_folder_from(rel, self.folders.get(rel)) } else { None };
        let old = self.folders.remove(rel).expect("checked above");
        let changed = match &new {
            None => old.files.len().max(1),
            Some(n) => {
                let stems: BTreeSet<&String> = old.files.keys().chain(n.files.keys()).collect();
                stems
                    .into_iter()
                    .filter(|s| match (old.files.get(*s), n.files.get(*s)) {
                        (Some(a), Some(b)) => a.text.as_ref().ok() != b.text.as_ref().ok() || a.text.is_err() != b.text.is_err(),
                        _ => true,
                    })
                    .count()
            }
        };
        if let Some(n) = new {
            self.folders.insert(rel.to_owned(), n);
        }
        // (Read now, under the lock: newer than every read planned so far.)
        self.scan_seq += 1;
        self.folder_seq.insert(rel.to_owned(), self.scan_seq);
        if changed > 0 {
            // Any recipe there may include a changed file: work them out again (cheap: no builds).
            self.forget(|_, r| r.folder == rel);
            self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != rel);
        }
        changed
    }

    /// `--style`, or the nearest `style.json` from the recipe's folder up to the root.
    fn style_path(&self, folder_rel: &Path) -> Option<PathBuf> {
        if let Some(p) = &self.style_override {
            return Some(p.clone());
        }
        let mut at = Some(folder_rel);
        while let Some(rel) = at {
            let candidate = self.root.join(rel).join(STYLE_FILE);
            if is_regular(&candidate) {
                return Some(candidate);
            }
            at = rel.parent();
        }
        None
    }

    fn style(&mut self, path: &Path, recipe: &str) -> Result<(Arc<Style>, u64), WorldError> {
        if !self.styles.contains_key(path) {
            let file = read_style(path);
            self.styles.insert(path.to_owned(), file);
        }
        let s = &self.styles[path];
        match &s.parsed {
            Ok(style) => Ok((Arc::clone(style), s.hash)),
            Err(e) => Err(WorldError::new("style_invalid", format!("{} (style for {recipe}): {e}", path.display()))
                .with("field", "recipe")
                .with("style", path.display().to_string())
                .with("recipe_error", e.to_json())),
        }
    }

    /// Re-reads changed recipe folders and styles (at most once per [`POLL_EVERY`] unless
    /// `force`) and re-keys the handles whose recipe content changed: an unchanged key is left
    /// alone, a cached one is taken at once, anything else is queued. `None` when nothing was
    /// checked or nothing changed.
    pub fn poll(&mut self, force: bool) -> Option<Reload> {
        if !force && !self.poll_due() {
            return None;
        }
        let plan = self.scan_plan();
        let scanned = plan.run();
        // Synchronous (model_reload answers with it): every forgotten recipe now, unsliced.
        let slice = self.rekey_slice.take();
        let reload = self.apply_scan(scanned, force);
        self.rekey_slice = slice;
        reload
    }

    /// Limits how long one re-key runs ([`REKEY_SLICE`] for the engine's loops); `None`: no limit.
    pub fn set_rekey_slice(&mut self, slice: Option<Duration>) {
        self.rekey_slice = slice;
    }

    /// Recipes a sliced re-key left for later.
    pub fn rekey_pending(&self) -> usize {
        self.rekey_left.len()
    }

    /// Goes on with the recipes a sliced re-key left (each loop pass, before anything else of
    /// the library's): `None` when nothing was left.
    pub fn rekey_more(&mut self) -> Option<Reload> {
        if self.rekey_left.is_empty() {
            return None;
        }
        let contents: Vec<Option<u64>> = self.handles.iter().map(|h| h.as_ref().and_then(|h| self.source_content(&h.spec.recipe))).collect();
        self.rekey_forgotten(Reload { files_changed: self.rekey_left.len(), ..Reload::default() }, contents, true)
    }

    /// A periodic poll is due ([`POLL_EVERY`] since the last one started).
    pub fn poll_due(&self) -> bool {
        self.last_poll.elapsed() >= POLL_EVERY
    }

    /// What a poll looks at on disk, from the caches (no disk: under the lock, on the engine
    /// thread). [`ScanPlan::run`] reads the disk without the lock (on a worker) and
    /// [`Library::apply_scan`] applies what it found. Scanning a recipe folder of a few dozen
    /// files on the engine thread took 4-22 ms a poll in the M4 exit run (`stat` per file).
    pub fn scan_plan(&mut self) -> ScanPlan {
        self.last_poll = Instant::now();
        self.scan_seq += 1;
        let folders = self.folders.iter().map(|(rel, f)| (rel.clone(), f.clone())).collect();
        // Each anim-set file, and whether some recipe has no time to compare it by (the scan
        // then reads what it says).
        let mut anims: BTreeMap<PathBuf, bool> = BTreeMap::new();
        for r in self.recipes.values() {
            *anims.entry(r.anims_path.clone()).or_default() |= r.anims_mtime.is_none();
        }
        let gltfs: BTreeSet<PathBuf> = self.gltfs.values().map(|g| g.path.clone()).collect();
        let styles = self.styles.iter().map(|(p, s)| (p.clone(), s.mtime)).collect();
        // Folders handles need that weren't there (removed, or not made yet): back now?
        let missing: BTreeSet<PathBuf> = self
            .handles
            .iter()
            .flatten()
            .map(|h| Path::new(&h.spec.recipe).parent().unwrap_or(Path::new("")).to_owned())
            .filter(|f| !self.folders.contains_key(f))
            .collect();
        ScanPlan {
            seq: self.scan_seq,
            root: self.root.clone(),
            canonical_root: self.canonical_root.clone(),
            folders,
            anims: anims.into_iter().collect(),
            gltfs: gltfs.into_iter().collect(),
            styles,
            missing: missing.into_iter().collect(),
        }
    }

    /// Applies the folders of a request's read ([`Library::request_plan`]): one it re-read
    /// replaces the cached one as [`Library::apply_scan`] does it, one that was not cached is
    /// cached. Returns how many files changed. Handles are not re-keyed here: those whose
    /// recipe changed are left for the engine's next pass ([`Library::rekey_more`]).
    fn apply_folder_scan(&mut self, scanned: Scanned) -> usize {
        // (No look at `scan_floor`: a whole scan applied meanwhile did not read a folder no
        // handle uses, so dropping this read for it left such a folder unread and its recipe
        // "not found". Reads of one folder are ordered by `folder_seq`.)
        if self.canonical_root.is_none() {
            self.canonical_root = scanned.canonical_root.clone();
        }
        let plan = scanned.seq;
        let mut changed: usize = scanned.folders.into_iter().map(|(rel, new)| self.replace_folder(&rel, new, plan)).sum();
        for (f, folder) in scanned.missing {
            if self.folders.contains_key(&f) {
                // Someone cached it meanwhile: this read replaces theirs if it is the newer.
                changed += self.replace_folder(&f, Some(folder), plan);
                continue;
            }
            if self.folder_seq.get(&f).is_some_and(|&applied| plan < applied) {
                continue;
            }
            changed += folder.files.len().max(1);
            self.folder_seq.insert(f.clone(), plan);
            self.folders.insert(f.clone(), folder);
            self.forget(|_, r| r.folder == f);
            self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != f);
        }
        changed
    }

    /// Applies a scan ([`ScanPlan::run`]): changed folders, anim sets, glTF files and styles are
    /// forgotten and their handles' keys worked out again (from the cached texts: no disk but a
    /// changed recipe's anims file). `force` reports even when nothing changed.
    pub fn apply_scan(&mut self, scanned: Scanned, force: bool) -> Option<Reload> {
        // Planned before a newer scan was applied: what it read is older than the cache.
        if scanned.seq < self.scan_floor {
            return None;
        }
        self.scan_floor = scanned.seq;
        let plan = scanned.seq;
        let report = Reload::default();
        if self.canonical_root.is_none() {
            self.canonical_root = scanned.canonical_root.clone();
        }
        let contents: Vec<Option<u64>> = self.handles.iter().map(|h| h.as_ref().and_then(|h| self.source_content(&h.spec.recipe))).collect();
        let mut files_changed = 0;
        let zf = specula_core::profile::zone("models.apply_folders");
        for (rel, new) in scanned.folders {
            files_changed += self.replace_folder(&rel, new, plan);
        }
        drop(zf);
        // Anim sets and glTF assets that changed (or appeared, or went): their keys are worked out again.
        let before = self.recipes.len() + self.gltfs.len();
        self.recipes.retain(|_, r| {
            let Some(seen) = scanned.anims.get(&r.anims_path) else { return true };
            match r.anims_mtime {
                // A time that was old when the file was read tells any later write.
                Some(_) => seen.mtime == r.anims_mtime,
                // Worked out after this scan was planned, from a file it did not read: the
                // next scan compares it.
                None if seen.mtime.is_some() && !seen.read => true,
                // No time to compare by (no file then, or one just written): by what it says.
                // The same: its time is taken once the scan found it old enough.
                None => {
                    let same = seen.text.as_deref() == r.anims.as_deref();
                    if same {
                        r.anims_mtime = seen.stamp;
                    }
                    same
                }
            }
        });
        self.gltfs.retain(|_, g| scanned.gltfs.get(&g.path).is_none_or(|m| *m == Some((g.mtime, g.len))));
        files_changed += before - self.recipes.len() - self.gltfs.len();
        for (p, file) in scanned.styles {
            if self.notes.get(&p).is_some_and(|&n| n > plan) {
                continue; // the store wrote it after this scan was planned
            }
            // Read again and saying what it said (a style read while its time was fresh, or
            // one the store wrote): nothing built from it is worked out again.
            let same = self.styles.get(&p).is_some_and(|old| old.hash == file.hash && old.parsed.is_ok() == file.parsed.is_ok());
            self.styles.insert(p.clone(), file);
            if same {
                continue;
            }
            self.recipes.retain(|_, r| r.style_path != p);
            self.failed.clear();
            files_changed += 1;
        }
        let _zm = specula_core::profile::zone("models.apply_missing");
        for (f, folder) in scanned.missing {
            if self.folders.contains_key(&f) || self.folder_seq.get(&f).is_some_and(|&applied| plan < applied) {
                continue;
            }
            files_changed += folder.files.len().max(1);
            self.folder_seq.insert(f.clone(), plan);
            self.folders.insert(f.clone(), folder);
            self.recipes.retain(|_, r| r.folder != f);
            self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != f);
        }
        drop(_zm);
        // Notes this scan saw on the disk (made before it was planned) are settled.
        self.notes.retain(|_, n| *n > plan);
        self.rekey_after_poll(Reload { files_changed, ..report }, contents, force)
    }

    /// Puts a folder a scan re-read in place of the cached one (`None`: it went, or now leads
    /// outside the root), forgetting the recipes there when any text changed. Returns how many
    /// files changed.
    fn replace_folder(&mut self, rel: &Path, new: Option<Folder>, plan: u64) -> usize {
        // A read of this folder planned later was applied already: this one is the older
        // (reads are made without the lock, by polls and by requests, and land in any order).
        if self.folder_seq.get(rel).is_some_and(|&applied| plan < applied) {
            return 0;
        }
        let Some(old) = self.folders.remove(rel) else {
            // Forgotten meanwhile: a resolve reads it again when it needs it.
            return 0;
        };
        self.folder_seq.insert(rel.to_owned(), plan);
        // Files the store wrote or deleted after the scan was planned keep the cache's version
        // (the disk the scan read was older).
        let noted_after = |stem: &str| self.notes.get(&rel.join(format!("{stem}.json"))).is_some_and(|&n| n > plan);
        let new = new.map(|mut n| {
            let stems: BTreeSet<String> = old.files.keys().chain(n.files.keys()).filter(|s| noted_after(s)).cloned().collect();
            for stem in stems {
                match old.files.get(&stem) {
                    Some(f) => {
                        n.files.insert(stem, f.clone());
                    }
                    None => {
                        n.files.remove(&stem);
                    }
                }
            }
            n
        });
        let Some(n) = new else {
            // The folder went: everything there is worked out again.
            self.forget(|_, r| r.folder == rel);
            self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != rel);
            return old.files.len().max(1);
        };
        let stems: BTreeSet<&String> = old.files.keys().chain(n.files.keys()).collect();
        let changed: BTreeSet<String> = stems
            .into_iter()
            .filter(|s| match (old.files.get(*s), n.files.get(*s)) {
                (Some(a), Some(b)) => a.text.as_ref().ok() != b.text.as_ref().ok() || a.text.is_err() != b.text.is_err(),
                _ => true,
            })
            .cloned()
            .collect();
        self.folders.insert(rel.to_owned(), n);
        if !changed.is_empty() {
            // The changed recipes and those including one of them are worked out again (every
            // recipe of the folder was, 20 ms a scan in the M4 exit's write load). Failures there
            // are retried.
            self.forget(|key, r| r.folder == rel && (Path::new(key).file_stem().and_then(|s| s.to_str()).is_some_and(|s| changed.contains(s)) || r.includes.iter().any(|i| changed.contains(i))));
            self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != rel);
        }
        changed.len()
    }

    /// Files the project store just wrote (paths under the assets root, texts): the cached
    /// recipe folders take the new texts at once, so tools that read recipes right after a
    /// `project_write` answers see them, with no disk scan on the engine thread. Their times are
    /// left unknown, so the next scan (on a worker) reads them once. Returns whether a cached text
    /// changed; then [`Library::rekey`] works the affected handles out again.
    pub fn note_written(&mut self, files: &[(String, Option<Arc<str>>)]) -> bool {
        let mut any = false;
        for (rel, text) in files {
            let rel = Path::new(rel);
            if rel.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Some(stem) = rel.file_stem().and_then(|s| s.to_str()) else { continue };
            let folder = rel.parent().unwrap_or(Path::new("")).to_owned();
            if stem == "style" {
                // A style the store wrote: its recipes are worked out again with it (its time
                // unknown, so the next scan reads it once).
                let path = self.root.join(rel);
                self.scan_seq += 1;
                self.notes.insert(path.clone(), self.scan_seq);
                match text {
                    Some(t) => {
                        let file = style_file(&path, Ok(t.to_string()), None);
                        self.styles.insert(path.clone(), file);
                    }
                    None => {
                        self.styles.remove(&path);
                    }
                }
                self.recipes.retain(|_, r| r.style_path != path);
                self.failed.clear();
                any = true;
                continue;
            }
            let Some(f) = self.folders.get_mut(&folder) else { continue };
            match text {
                Some(text) => {
                    if f.files.get(stem).is_some_and(|old| old.text.as_ref().ok().is_some_and(|t| **t == **text)) {
                        continue;
                    }
                    f.files.insert(stem.to_owned(), SourceFile { text: Ok(Arc::clone(text)), mtime: None });
                }
                // Deleted: gone from the cache at once (a resolve no longer finds it).
                None => {
                    if f.files.remove(stem).is_none() {
                        continue;
                    }
                }
            }
            // Scans planned before this note keep its text (`replace_folder`).
            self.scan_seq += 1;
            self.notes.insert(rel.to_owned(), self.scan_seq);
            // Only the recipe itself and those including it are worked out again (forgetting the
            // whole folder re-keyed every recipe there: 10-70 ms a write in the M4 exit's load).
            self.recipes.retain(|key, r| r.folder != folder || (Path::new(key).file_stem().and_then(|s| s.to_str()) != Some(stem) && !r.includes.contains(stem)));
            self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != folder);
            any = true;
        }
        any
    }

    /// Works out again the keys of handles whose recipes were forgotten (after
    /// [`Library::note_written`]): the in-memory half of a poll, with no disk scan.
    pub fn rekey(&mut self, force: bool) -> Option<Reload> {
        let contents: Vec<Option<u64>> = self.handles.iter().map(|h| h.as_ref().and_then(|h| self.source_content(&h.spec.recipe))).collect();
        self.rekey_after_poll(Reload { files_changed: 1, ..Reload::default() }, contents, force)
    }

    fn rekey_after_poll(&mut self, report: Reload, contents: Vec<Option<u64>>, force: bool) -> Option<Reload> {
        if report.files_changed == 0 && !force {
            return None;
        }
        self.rekey_forgotten(report, contents, false)
    }

    /// Works out again the keys of the handles whose recipes are forgotten, and of those an
    /// earlier slice left (`left_only`: those alone), within the re-key slice: recipes not
    /// reached are left for [`Library::rekey_more`] (their handles keep drawing their last build
    /// meanwhile).
    fn rekey_forgotten(&mut self, mut report: Reload, mut contents: Vec<Option<u64>>, left_only: bool) -> Option<Reload> {
        let _z = specula_core::profile::zone("models.rekey");
        let deadline = self.rekey_slice.map(|d| Instant::now() + d);
        // Recipes left by an earlier slice compare their anim sets with what they were before
        // they were forgotten.
        let left = std::mem::take(&mut self.rekey_left);
        for (h, slot) in self.handles.iter().enumerate() {
            if let Some(was) = slot.as_ref().and_then(|s| left.get(&s.spec.recipe)) {
                contents[h] = *was;
            }
        }
        // Handles whose recipe was forgotten above get their key worked out again: every handle
        // of it (the first one's key_for makes the recipe known again, so decide up front).
        let forgotten: BTreeSet<String> = self
            .handles
            .iter()
            .flatten()
            // Left recipes are worked out even if a resolve made them known meanwhile (that
            // resolve didn't re-key their existing handles).
            .filter(|s| left.contains_key(&s.spec.recipe) || (!left_only && !self.known(&s.spec.recipe)))
            .map(|s| s.spec.recipe.clone())
            .collect();
        let mut started: BTreeSet<String> = BTreeSet::new();
        for h in 0..self.handles.len() {
            let Some(slot) = &self.handles[h] else { continue };
            if !forgotten.contains(&slot.spec.recipe) {
                continue;
            }
            if !started.contains(&slot.spec.recipe) {
                // Out of time (after at least one recipe): the rest waits for the next pass.
                if !started.is_empty() && deadline.is_some_and(|d| Instant::now() >= d) {
                    let was = contents.get(h).copied().flatten();
                    self.rekey_left.entry(slot.spec.recipe.clone()).or_insert(was);
                    continue;
                }
                started.insert(slot.spec.recipe.clone());
            }
            let spec = slot.spec.clone();
            let current = self.handles[h].as_ref().and_then(|s| s.built.as_ref().map(|b| b.id));
            report.handles.insert(h as u32);
            match self.key_for(&spec, false, true) {
                Ok(key) => {
                    let unchanged = self.builds.get(&key).is_some_and(|b| Some(b.id) == current);
                    if unchanged {
                        // Fixed back to what it was: the build stands, and so must not the error.
                        self.set_want(h as u32, None);
                        self.handles[h].as_mut().expect("live handle").error = None;
                        continue;
                    }
                    self.want(h as u32, key.clone(), true);
                    match self.in_flight.get(&key) {
                        Some(f) => {
                            report.jobs.insert(f.job);
                            report.rebuilding.push(spec.key());
                        }
                        None => report.rebuilt.push(spec.key()),
                    }
                }
                Err(e) => {
                    report.errors.push((spec.recipe.clone(), e.to_json()));
                    self.set_want(h as u32, None);
                    self.handles[h].as_mut().expect("live handle").error = Some(e);
                }
            }
        }
        // Sources whose content changed: graph writes now settle against their new anim sets
        // (those left for a later slice, when they are worked out).
        for (h, was) in contents.iter().enumerate() {
            let Some(slot) = &self.handles[h] else { continue };
            if self.rekey_left.contains_key(&slot.spec.recipe) {
                continue;
            }
            if self.source_content(&slot.spec.recipe) != *was {
                report.anim_handles.insert(h as u32);
            }
        }
        Some(report)
    }

    /// Takes finished builds (waiting up to `wait` for the first one when some are in flight)
    /// and points the handles that wanted them at them. Returns them for whoever waits on their
    /// jobs, and the handles whose build or state changed.
    pub fn finish(&mut self, wait: Option<Duration>) -> (Vec<Finished>, BTreeSet<u32>) {
        let mut finished = Vec::new();
        let mut changed = BTreeSet::new();
        let mut first = wait.filter(|_| !self.in_flight.is_empty());
        loop {
            let done = match first.take() {
                Some(d) => match self.workers.done.recv_timeout(d) {
                    Ok(done) => done,
                    Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
                },
                None => match self.workers.done.try_recv() {
                    Ok(done) => done,
                    Err(_) => break,
                },
            };
            // A job a later one replaced (it was cancelled, then the key was wanted again) speaks
            // only for itself: its failure or cancellation never reaches the handles now waiting.
            let current = self.in_flight.get(&done.key).is_some_and(|f| f.job == done.id);
            if current {
                self.in_flight.remove(&done.key);
            }
            // Ids of jobs cancelled while already running would otherwise stay in the set.
            self.workers.cancelled.lock().unwrap_or_else(PoisonError::into_inner).remove(&done.id);
            self.workers.flags.lock().unwrap_or_else(PoisonError::into_inner).remove(&done.id);
            let result = match done.outcome {
                Outcome::Built(model, extras) => {
                    let Extras { anims, anim_note, textures, materials, warnings } = *extras;
                    let built = Arc::new(Built {
                        id: self.next_id,
                        model: *model,
                        content: done.key.0,
                        build_ms: done.build_ms,
                        style: done.style_path,
                        anims: anims.map(Arc::new),
                        anim_note,
                        textures,
                        materials,
                        warnings,
                    });
                    self.next_id += 1;
                    self.builds.insert(done.key.clone(), Arc::clone(&built));
                    Ok(built)
                }
                Outcome::Failed(e) => Err(recipe_error(&done.recipe, &e)),
                Outcome::Panicked(why) => Err(WorldError::new(
                    "build_panicked",
                    format!("{}: the model build panicked ({why}); a bug in specula-model, not in the recipe", done.recipe),
                )
                .with("recipe", done.recipe.as_str())),
                Outcome::Cancelled => Err(WorldError::new("build_cancelled", "no entity needed this build any more")
                    .with("recipe", done.recipe.as_str())),
            };
            // A good build serves whoever waits for its key; anything else only if it is the
            // key's current job (and a cancellation never does).
            let applies = match &result {
                Ok(_) => true,
                Err(e) => current && e.code != "build_cancelled",
            };
            let waiting = if applies { self.waiting.remove(&done.key).unwrap_or_default() } else { BTreeSet::new() };
            if applies && !current {
                // The key's current job is no longer needed by anyone.
                if let Some(f) = self.in_flight.remove(&done.key) {
                    if !f.pinned {
                        self.workers.cancel(f.job);
                    } else {
                        self.in_flight.insert(done.key.clone(), f);
                    }
                }
            }
            for h in waiting {
                let Some(slot) = self.handles[h as usize].as_mut() else { continue };
                slot.want = None;
                match &result {
                    Ok(b) => {
                        slot.built = Some(Arc::clone(b));
                        slot.built_key = Some(done.key.clone());
                        slot.error = None;
                    }
                    Err(e) => slot.error = Some(e.clone()),
                }
                changed.insert(h);
            }
            if !changed.is_empty() {
                self.generation += 1;
            }
            // A request on its way to the engine thread will ask for it: kept until it does.
            if self.claims.contains_key(&done.id) {
                self.kept_results.push((done.id, result.clone()));
            }
            finished.push(Finished { job: done.id, result });
        }
        (finished, changed)
    }

    /// Tests: how many results are kept for requests that have not landed, and how many jobs
    /// such requests wait for.
    #[cfg(test)]
    pub(crate) fn finished_job_count(&self) -> (usize, usize) {
        (self.kept_results.len(), self.claims.len())
    }

    /// A request made away from the engine thread will [`Library::claim`] this job's result
    /// when it gets there: if the job finishes first, its result is kept for it. In the same
    /// hold as the request that returned the job.
    pub fn expect_claim(&mut self, job: u64) {
        *self.claims.entry(job).or_insert(0) += 1;
    }

    /// The result of `job` if it finished already ([`Library::finish`] took it before the
    /// request reached the engine thread); `None`: still running, wait for it as usual. One
    /// call for each [`Library::expect_claim`]: the last one releases what was kept.
    pub fn claim(&mut self, job: u64) -> Option<Result<Arc<Built>, WorldError>> {
        let last = match self.claims.get_mut(&job) {
            Some(n) if *n > 1 => {
                *n -= 1;
                false
            }
            _ => {
                self.claims.remove(&job);
                true
            }
        };
        let at = self.kept_results.iter().position(|(j, _)| *j == job)?;
        Some(if last { self.kept_results.swap_remove(at).1 } else { self.kept_results[at].1.clone() })
    }

    /// Builds `id` again (its texture pixels were released by an earlier upload): it leaves the
    /// cache and every handle drawing it waits for a fresh build of the same key.
    pub fn reimport(&mut self, id: u64) {
        self.builds.retain(|_, b| b.id != id);
        self.recent.retain(|r| *r != id);
        for h in 0..self.handles.len() {
            let Some(slot) = self.handles[h].as_mut() else { continue };
            if slot.built.as_ref().is_none_or(|b| b.id != id) {
                continue;
            }
            slot.built = None;
            let key = slot.built_key.take();
            if let Some(key) = key {
                self.want(h as u32, key, false);
            }
        }
        self.generation += 1;
    }

    /// Forgets build `id` (its textures left the CPU with an upload that has now been freed):
    /// handles drawing it work their key out again when next used, so a respawn waits for a
    /// fresh import instead of finding a build that can't be uploaded.
    pub fn forget_build(&mut self, id: u64) {
        self.builds.retain(|_, b| b.id != id);
        self.recent.retain(|r| *r != id);
        for slot in self.handles.iter_mut().flatten() {
            if slot.built.as_ref().is_some_and(|b| b.id == id) {
                slot.built = None;
                slot.built_key = None;
                slot.verify = true;
            }
        }
        self.generation += 1;
    }

    /// Keeps `id` (a `model_build` result) among the builds kept without an entity.
    fn remember(&mut self, id: u64) {
        self.recent.retain(|r| *r != id);
        self.recent.push_back(id);
        while self.recent.len() > RECENT_BUILDS {
            self.recent.pop_front();
        }
    }

    /// Notes a finished `model_build` job's build as recent (see [`Library::remember`]).
    pub fn remember_build(&mut self, built: &Built) {
        self.remember(built.id);
    }

    /// Drops what no live entity uses (see the module docs). `live`: the handles of the world's
    /// model entities.
    pub fn collect_garbage(&mut self, live: &BTreeSet<u32>) {
        let forget = self.by_spec.len() > live.len() + SPARE_HANDLES;
        for h in 0..self.handles.len() as u32 {
            if self.handles[h as usize].is_none() || live.contains(&h) {
                continue;
            }
            self.unuse(h);
            if forget {
                let gone = self.handles[h as usize].take().expect("checked");
                self.by_spec.remove(&gone.spec.key());
                self.free_handles.push(h);
            }
        }
        if forget {
            self.generation += 1;
        }
        self.cancel_unwanted();
        let handles: Vec<&Handle> = self.handles.iter().flatten().collect();
        let kept: BTreeSet<u64> = handles.iter().filter_map(|h| h.built.as_ref().map(|b| b.id)).chain(self.recent.iter().copied()).collect();
        self.builds.retain(|_, b| kept.contains(&b.id));
        // Folders, recipes and styles only live handles need (one path per recipe, not per handle).
        let recipes: BTreeSet<&str> = handles.iter().map(|h| h.spec.recipe.as_str()).collect();
        let folders: BTreeSet<&Path> = recipes.iter().map(|r| Path::new(*r).parent().unwrap_or(Path::new(""))).collect();
        self.recipes.retain(|r, _| recipes.contains(r.as_str()));
        self.gltfs.retain(|r, _| recipes.contains(r.as_str()));
        self.failed.retain(|r, _| recipes.contains(r.as_str()));
        self.folders.retain(|f, _| folders.contains(f.as_path()));
        let styles: BTreeSet<&PathBuf> = self.recipes.values().map(|r| &r.style_path).collect();
        let styles: BTreeSet<PathBuf> = styles.into_iter().cloned().collect();
        self.styles.retain(|p, _| styles.contains(p));
    }

    /// No entity uses handle `h`: it stops waiting (its build is cancelled unless something
    /// else waits for it), so deleted entities never hold up live ones, and it re-checks its key
    /// when it is used again (the recipe may change meanwhile).
    fn unuse(&mut self, h: u32) {
        if self.handles[h as usize].as_ref().is_some_and(|s| s.want.is_some()) {
            self.set_want(h, None);
        }
        if let Some(slot) = self.handles[h as usize].as_mut() {
            slot.verify = true;
        }
    }

    /// The cheap part of garbage collection, for while builds are in flight (the engine runs it
    /// after entities stopped using models, [`crate::world::World::model_drops`]): handles waiting
    /// for a build that no entity uses (`live`: the handles entities use, sorted) stop waiting,
    /// and the builds that leaves nobody waiting for are cancelled. No allocation per handle.
    pub fn drop_unwanted(&mut self, live: &[u32]) {
        let unused: Vec<u32> =
            self.waiting.values().flatten().copied().filter(|h| live.binary_search(h).is_err()).collect();
        if unused.is_empty() {
            return;
        }
        let mut freed = Vec::new();
        for h in unused {
            if let Some(k) = self.handles[h as usize].as_ref().and_then(|s| s.want.clone()) {
                freed.push(k);
            }
            self.unuse(h);
        }
        for k in freed {
            if self.waiting.contains_key(&k) || self.in_flight.get(&k).is_none_or(|f| f.pinned) {
                continue;
            }
            if let Some(f) = self.in_flight.remove(&k) {
                self.workers.cancel(f.job);
            }
        }
    }

    /// Queued builds nobody waits for: workers skip them.
    fn cancel_unwanted(&mut self) {
        let dropped: Vec<BuildKey> =
            self.in_flight.iter().filter(|(k, f)| !f.pinned && !self.waiting.contains_key(k)).map(|(k, _)| k.clone()).collect();
        if dropped.is_empty() {
            return;
        }
        for k in dropped {
            if let Some(f) = self.in_flight.remove(&k) {
                self.workers.cancel(f.job);
            }
        }
    }

    /// Sizes for engine_info and perf_frame.
    pub fn stats_json(&self) -> Json {
        let errors: Vec<Json> = self
            .handles
            .iter()
            .flatten()
            .filter_map(|h| h.error.as_ref().map(|e| Json::obj([("recipe", Json::from(h.spec.recipe.as_str())), ("error", e.to_json())])))
            .take(8)
            .collect();
        let (specs, builds, in_flight, folders) = self.counts();
        Json::obj([
            ("assets_root", Json::from(self.root.display().to_string())),
            ("style", self.style_override.as_ref().map_or(Json::Null, |p| Json::from(p.display().to_string()))),
            ("specs", Json::from(specs)),
            ("builds", Json::from(builds)),
            ("building", Json::from(in_flight)),
            ("folders", Json::from(folders)),
            ("cache_hits", Json::from(self.cache_hits)),
            ("cache_misses", Json::from(self.cache_misses)),
            ("broken", Json::Arr(errors)),
        ])
    }
}

/// What a model-library poll looks at on disk ([`Library::scan_plan`]): the cached folders with
/// their texts and times, the anim-set, glTF and style files the caches know, and the folders
/// handles need that aren't cached. Run without the library's lock ([`ScanPlan::run`]).
pub struct ScanPlan {
    seq: u64,
    root: PathBuf,
    canonical_root: Option<PathBuf>,
    folders: Vec<(PathBuf, Folder)>,
    /// Each anim-set file, and whether its text is to be read (the cache has no time for it).
    anims: Vec<(PathBuf, bool)>,
    gltfs: Vec<PathBuf>,
    styles: Vec<(PathBuf, Option<SystemTime>)>,
    missing: Vec<PathBuf>,
}

/// What a scan found ([`Library::apply_scan`] applies it): only what changed.
#[derive(Default)]
pub struct Scanned {
    /// Its plan's place in [`Library`]'s sequence.
    seq: u64,
    canonical_root: Option<PathBuf>,
    /// Folders whose time or files' times changed, read again (`None`: gone, or outside the root).
    folders: Vec<(PathBuf, Option<Folder>)>,
    /// Every anim-set file as it is now.
    anims: BTreeMap<PathBuf, AnimSeen>,
    /// Every glTF file's (time, length) now (`None`: gone).
    gltfs: BTreeMap<PathBuf, Option<(Option<SystemTime>, u64)>>,
    /// Styles whose time changed, read again.
    styles: Vec<(PathBuf, StyleFile)>,
    /// Folders that weren't cached and exist now (inside the root).
    missing: Vec<(PathBuf, Folder)>,
    /// Folders that weren't cached, exist, and lead outside the root (never cached).
    outside: Vec<PathBuf>,
}

/// An anim-set file as a scan found it.
struct AnimSeen {
    /// Its time (`None`: no such file).
    mtime: Option<SystemTime>,
    /// That time as a later scan may compare with it ([`stamp_of`], at this scan's read).
    stamp: Option<SystemTime>,
    /// The scan was asked for its text (the cache had no time for it).
    read: bool,
    /// What it says, when read and readable (as a recipe's own read of it: a regular file).
    text: Option<Arc<str>>,
}

/// What a build request reads from the disk ([`Library::request_plan`]).
pub struct RequestPlan {
    scan: ScanPlan,
    /// A glTF source (its recipe path): its size and time.
    gltf: Option<String>,
    /// The folder was cached when this was planned: the read brings only what changed.
    from_cache: bool,
}

/// What a request's plan read ([`RequestPlan::run`]).
pub struct RequestRead {
    scanned: Scanned,
    gltf: Option<Result<GltfInfo, GltfMiss>>,
    from_cache: bool,
}

enum GltfMiss {
    /// Its folder leads outside the assets root.
    Outside,
    NotFound,
}

impl RequestPlan {
    /// Reads the disk: never under the library's lock, never on the engine thread.
    pub fn run(self) -> RequestRead {
        let gltf = self.gltf.as_deref().map(|recipe| {
            let root = &self.scan.root;
            let folder = Path::new(recipe).parent().unwrap_or(Path::new(""));
            let canonical = self.scan.canonical_root.clone().or_else(|| root.canonicalize().ok());
            match (canonical, root.join(folder).canonicalize()) {
                (Some(c), Ok(full)) if !full.starts_with(&c) => Err(GltfMiss::Outside),
                (Some(_), Ok(_)) => gltf_info(root, recipe).ok_or(GltfMiss::NotFound),
                _ => Err(GltfMiss::NotFound),
            }
        });
        RequestRead { scanned: self.scan.run(), gltf, from_cache: self.from_cache }
    }
}

/// A glTF file's size and time under `root`, as its key is made of them (`None`: no such file).
fn gltf_info(root: &Path, recipe: &str) -> Option<GltfInfo> {
    let path = root.join(recipe);
    let meta = std::fs::symlink_metadata(&path).ok().filter(|m| m.file_type().is_file())?;
    let mtime = meta.modified().ok();
    let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0002);
    h.write_str(recipe);
    h.write_u64(meta.len());
    h.write_u64(mtime.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as u64));
    Some(GltfInfo { path, mtime, len: meta.len(), content: h.finish() })
}

fn outside_assets(spec: &ModelSpec) -> WorldError {
    WorldError::new("recipe_outside_assets", format!("{} leads outside the assets root", spec.recipe)).with("field", "recipe").with("recipe", spec.recipe.as_str())
}

/// Tests only: called by every scan of a root some test watches, on the thread that scans
/// (`ScanPlan::run`), before it reads anything.
#[cfg(test)]
type ScanHook = Arc<dyn Fn() + Send + Sync>;
#[cfg(test)]
static SCAN_HOOKS: Mutex<Vec<(PathBuf, ScanHook)>> = Mutex::new(Vec::new());

#[cfg(test)]
fn scan_hook(root: &Path) {
    let hooks: Vec<ScanHook> = SCAN_HOOKS.lock().unwrap_or_else(PoisonError::into_inner).iter().filter(|(r, _)| r == root).map(|(_, f)| Arc::clone(f)).collect();
    for f in hooks {
        f();
    }
}

/// Tests only: watches the scans of `root` until the returned value is dropped.
#[cfg(test)]
pub(crate) fn watch_scans(root: &Path, hook: impl Fn() + Send + Sync + 'static) -> ScanWatch {
    SCAN_HOOKS.lock().unwrap_or_else(PoisonError::into_inner).push((root.to_owned(), Arc::new(hook)));
    ScanWatch(root.to_owned())
}

#[cfg(test)]
pub(crate) struct ScanWatch(PathBuf);

#[cfg(test)]
impl Drop for ScanWatch {
    fn drop(&mut self) {
        SCAN_HOOKS.lock().unwrap_or_else(PoisonError::into_inner).retain(|(r, _)| *r != self.0);
    }
}

impl ScanPlan {
    /// Reads the disk: `stat`s every cached file, re-reads folders and styles that changed (the
    /// unchanged texts reused). Holds no lock; meant for a worker thread.
    pub fn run(self) -> Scanned {
        let _z = specula_core::profile::zone("models.scan");
        #[cfg(test)]
        scan_hook(&self.root);
        let canonical_root = self.canonical_root.clone().or_else(|| self.root.canonicalize().ok());
        let inside = |rel: &Path| canonical_root.as_ref().is_some_and(|root| self.root.join(rel).canonicalize().is_ok_and(|full| full.starts_with(root)));
        let mut out = Scanned { seq: self.seq, canonical_root: canonical_root.clone(), ..Scanned::default() };
        for (rel, old) in &self.folders {
            let dir = self.root.join(rel);
            // (A time the cache does not know, the folder's or a file's, is never "the same":
            // it was read too soon after its write to tell that write from the next, see `FRESH`.)
            let unchanged = old.mtime.is_some()
                && mtime(&dir) == old.mtime
                && old.files.iter().all(|(stem, f)| f.mtime.is_some() && mtime(&dir.join(format!("{stem}.json"))) == f.mtime);
            if unchanged {
                continue;
            }
            // Re-read only while it still lies inside the root (it may have become a link).
            let new = if inside(rel) { read_folder_at(&self.root, rel, Some(old)) } else { None };
            out.folders.push((rel.clone(), new));
        }
        for (p, read) in self.anims {
            let (m, stamp) = stamp_of(&p);
            let text = (read && m.is_some() && is_regular(&p)).then(|| std::fs::read_to_string(&p).ok()).flatten().map(Arc::<str>::from);
            out.anims.insert(p, AnimSeen { mtime: m, stamp, read, text });
        }
        for p in self.gltfs {
            let meta = std::fs::symlink_metadata(&p).ok().map(|m| (m.modified().ok(), m.len()));
            out.gltfs.insert(p, meta);
        }
        for (p, was) in self.styles {
            // (A style whose time is not known is read every time: what it says is compared.)
            if was.is_none() || mtime(&p) != was {
                let file = read_style(&p);
                out.styles.push((p, file));
            }
        }
        for f in self.missing {
            if !inside(&f) {
                if self.root.join(&f).canonicalize().is_ok() {
                    out.outside.push(f);
                }
                continue;
            }
            if let Some(folder) = read_folder_at(&self.root, &f, None) {
                out.missing.push((f, folder));
            }
        }
        out
    }
}

/// The folder's regular `*.json` files under `root` (links are not followed), reusing files from
/// `old` whose modification time is the same; `None` if it doesn't exist.
fn read_folder_at(root: &Path, rel: &Path, old: Option<&Folder>) -> Option<Folder> {
    let dir = root.join(rel);
    // (Its time before its listing: a file added while it is listed moves a time this read has
    // not taken.)
    let (_, dir_stamp) = stamp_of(&dir);
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut files = BTreeMap::new();
    for e in entries.flatten() {
        let path = e.path();
        if !e.file_type().is_ok_and(|t| t.is_file()) || path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if stem == "style" {
            continue;
        }
        let (modified, stamp) = stamp_of(&path);
        let reuse = old.and_then(|o| o.files.get(stem)).filter(|f| f.mtime == modified && modified.is_some());
        let text = match reuse {
            Some(f) => f.text.clone(),
            None => std::fs::read_to_string(&path).map(Arc::from).map_err(|e| e.to_string()),
        };
        files.insert(stem.to_owned(), SourceFile { text, mtime: stamp });
    }
    Some(Folder { files, mtime: dir_stamp })
}

fn read_style(path: &Path) -> StyleFile {
    let (_, stamp) = stamp_of(path);
    let text = if is_regular(path) || path.is_file() { std::fs::read_to_string(path) } else { Err(std::io::ErrorKind::NotFound.into()) };
    style_file(path, text, stamp)
}

/// A style file from its text (as read, or as the project store wrote it: `mtime` unknown).
fn style_file(path: &Path, text: std::io::Result<String>, mtime: Option<SystemTime>) -> StyleFile {
    let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0003);
    let parsed = match &text {
        Ok(t) => {
            h.write_str(t);
            Style::parse(t).map(Arc::new)
        }
        Err(e) => Err(RecipeError::new("", ErrorCode::Style, format!("can't read {}: {e}", path.display()))),
    };
    StyleFile { parsed, mtime, hash: h.finish() }
}

/// Every part's vertices fit in one upload (u32 indices) and every material resolved.
fn check_uploadable(model: &Model) -> Result<(), String> {
    for (i, lod) in model.lods.iter().enumerate() {
        if lod.positions.len() >= u32::MAX as usize {
            return Err(format!("LOD {i} has too many vertices to upload"));
        }
        for part in &lod.parts {
            if !model.materials.iter().any(|(n, _)| *n == part.material) {
                return Err(format!("LOD {i} uses material '{}', which the build didn't resolve", part.material));
            }
        }
    }
    Ok(())
}

/// Why entity `e`'s `anim` can't play (it draws its bind pose), once its model is built: the
/// model has no anim set, or the clip (or the clip it fades from) isn't in it. `None` when it
/// plays, has no anim, or its model is still building.
pub fn anim_problem(lib: &Library, e: &Entity) -> Option<String> {
    let a = e.anim.as_ref()?;
    let handle = e.resolved_model()?.handle;
    let built = lib.built(handle)?;
    let Some(set) = lib.anim_set(handle) else {
        let why = built.anim_note.clone().unwrap_or_else(|| "the model has no rig".into());
        let what = a.graph.as_ref().map_or_else(|| a.clip.clone(), |g| format!("graph {}", g.graph));
        return Some(format!("'{}' has anim '{what}' but no clips to play: {why}", e.name));
    };
    if a.graph.is_some() {
        return crate::anim::graph_problem(&set, e, a);
    }
    let missing = [Some(a.clip.as_str()), a.from.as_ref().filter(|f| f.graph.is_none()).map(|f| f.clip.as_str())].into_iter().flatten().find(|c| set.id(c).is_none())?;
    let clips: Vec<&str> = set.clips().map(|(_, c)| c.name.as_str()).take(32).collect();
    Some(format!("'{}' plays clip '{missing}', which its anim set lacks: drawn in its bind pose (clips: {})", e.name, clips.join(", ")))
}

/// The model handles the world's model entities use.
pub fn live_handles(world: &World) -> BTreeSet<u32> {
    world.model_entities().filter_map(|e| e.resolved_model().map(|r| r.handle)).collect()
}

/// The library as the world's [`ModelSource`], shared with the engine.
pub struct SharedLibrary(Mutex<Library>);

impl SharedLibrary {
    pub fn new(library: Library) -> SharedLibrary {
        SharedLibrary(Mutex::new(library))
    }

    /// The library (a poisoned lock is taken over: the library's state stays consistent between
    /// calls).
    ///
    /// The time it takes to get is counted for the calling thread ([`crate::lockwait`]): the
    /// engine thread shares this lock with the build workers, and a stall says how much of it
    /// was a wait for this.
    ///
    /// A hold of [`LOCK_HELD_LONG`] or more is traced when it ends (`models.lock_held`: where
    /// the lock was taken, for how long, on which thread): whoever else wants the library waits
    /// that long, the engine thread among them.
    #[track_caller]
    pub fn lock(&self) -> LibraryGuard<'_> {
        let site = std::panic::Location::caller();
        let asked = Instant::now();
        let library = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let since = Instant::now();
        crate::lockwait::waited(since.duration_since(asked));
        LibraryGuard { library: Some(library), site, since }
    }
}

impl SharedLibrary {
    /// The disk part of a request for `spec`, on the calling thread and **without the lock**
    /// (taken for a moment to plan it from the caches).
    #[track_caller]
    fn read_for(&self, spec: &ModelSpec) -> RequestRead {
        let plan = self.lock().request_plan(spec);
        plan.run()
    }

    /// [`Library::request`] for a caller's own thread or a worker: the disk is read without the
    /// lock, then the read is applied and the request answered in one hold of it. A read that
    /// went stale while it was made (a collection dropped the folder it was planned from) is
    /// made again, whole; after [`READ_AGAIN`] of those the request reads under the lock, as
    /// [`Library::request`] does, where nothing lands in between. Its answer never depends on
    /// what other threads did meanwhile.
    #[track_caller]
    pub fn request(&self, spec: &ModelSpec) -> Result<BuildRequest, WorldError> {
        self.request_as(spec, false)
    }

    /// [`SharedLibrary::request`] made on a worker for a call the engine thread answers: a
    /// job it has to wait for is marked ([`Library::expect_claim`], in the hold that returned
    /// it), so its result is there for [`Library::claim`] even if it finishes first.
    #[track_caller]
    pub fn request_to_claim(&self, spec: &ModelSpec) -> Result<BuildRequest, WorldError> {
        self.request_as(spec, true)
    }

    #[track_caller]
    fn request_as(&self, spec: &ModelSpec, claim: bool) -> Result<BuildRequest, WorldError> {
        spec.check()?;
        let expect = |library: &mut Library, requested: &BuildRequest| {
            if let (true, BuildRequest::Pending(job)) = (claim, requested) {
                library.expect_claim(*job);
            }
        };
        for _ in 0..READ_AGAIN {
            let read = self.read_for(spec);
            let mut library = self.lock();
            match library.request_read(spec, read) {
                Err(ReadError::Stale) => continue,
                Err(ReadError::Is(e)) => return Err(e),
                Ok(requested) => {
                    expect(&mut library, &requested);
                    return Ok(requested);
                }
            }
        }
        let mut library = self.lock();
        let requested = library.request(spec)?;
        expect(&mut library, &requested);
        Ok(requested)
    }

    /// [`Library::current_texts`] for a caller's own thread, as [`SharedLibrary::request`].
    #[track_caller]
    pub fn current_texts(&self, spec: &ModelSpec) -> Result<RecipeTexts, WorldError> {
        spec.check()?;
        for _ in 0..READ_AGAIN {
            let read = self.read_for(spec);
            match self.lock().current_texts_read(spec, read) {
                Err(ReadError::Stale) => continue,
                Err(ReadError::Is(e)) => return Err(e),
                Ok(texts) => return Ok(texts),
            }
        }
        self.lock().current_texts(spec)
    }
}

/// Why a request's read gave no answer.
pub enum ReadError {
    /// What it was planned from went while the disk was read: read again.
    Stale,
    /// The request's own error (not found, outside the assets root, a recipe that does not parse).
    Is(WorldError),
}

impl From<WorldError> for ReadError {
    fn from(e: WorldError) -> ReadError {
        ReadError::Is(e)
    }
}

/// A hold of the library's lock this long is traced (`models.lock_held`).
pub const LOCK_HELD_LONG: Duration = Duration::from_millis(2);

/// The library, locked (see [`SharedLibrary::lock`]).
pub struct LibraryGuard<'a> {
    /// (Always there until the guard is dropped, which releases it first and reports after.)
    library: Option<std::sync::MutexGuard<'a, Library>>,
    site: &'static std::panic::Location<'static>,
    since: Instant,
}

impl std::ops::Deref for LibraryGuard<'_> {
    type Target = Library;
    fn deref(&self) -> &Library {
        self.library.as_ref().expect("held until dropped")
    }
}

impl std::ops::DerefMut for LibraryGuard<'_> {
    fn deref_mut(&mut self) -> &mut Library {
        self.library.as_mut().expect("held until dropped")
    }
}

impl Drop for LibraryGuard<'_> {
    fn drop(&mut self) {
        // Released first: the report is not part of the hold (review R1 of m5-liblock).
        self.library = None;
        let held = self.since.elapsed();
        if held >= LOCK_HELD_LONG {
            let thread = std::thread::current();
            specula_core::trace_event!(
                &crate::trace_sink::ENGINE,
                Info,
                "models.lock_held",
                site = format!("{}:{}", self.site.file().replace('\\', "/"), self.site.line()),
                ms = held.as_secs_f64() * 1e3,
                thread = thread.name().unwrap_or("unnamed").to_owned(),
            );
        }
    }
}

impl ModelSource for SharedLibrary {
    fn resolve(&self, spec: &ModelSpec) -> Result<ResolvedModel, WorldError> {
        self.lock().resolve(spec)
    }

    fn anim_set(&self, handle: u32) -> Option<Arc<AnimSet>> {
        self.lock().anim_set(handle)
    }

    fn anim_set_for_write(&self, handle: u32, assets: &crate::anim::Assets) -> Option<Arc<AnimSet>> {
        self.lock().write_set(handle, assets)
    }
}

/// A bounded summary of a build for `model_build`.
pub fn report(spec: &ModelSpec, built: &Built, cached: bool) -> Json {
    let mut r = built.model.report();
    let presets = Json::arr(built.model.materials.iter().map(|(name, p)| preset_json(name, p)));
    r.set("recipe", Json::from(spec.recipe.as_str()));
    r.set("content_hash", Json::from(format!("{:016x}", built.content)));
    r.set("style", Json::from(built.style.display().to_string()));
    r.set("cached", Json::from(cached));
    r.set("build_ms", Json::from(built.build_ms));
    r.set("material_presets", presets);
    let (min, max) = built.model.budget;
    r.set("budget", Json::obj([("min_triangles", Json::from(min)), ("max_triangles", Json::from(max))]));
    r.set("lod_pixels", Json::arr(LOD_PIXELS.map(Json::from)));
    r.set("source", Json::from(if is_gltf_path(&spec.recipe) { "gltf" } else { "recipe" }));
    if let Some(set) = &built.anims {
        r.set("bones", Json::from(set.skeleton.len()));
        r.set(
            "clips",
            Json::arr(set.clips().take(64).map(|(_, c)| {
                let looping = match c.looping {
                    LoopMode::Once => "once",
                    LoopMode::Loop => "loop",
                    LoopMode::PingPong => "ping_pong",
                };
                Json::obj([("name", Json::from(c.name.as_str())), ("duration", Json::from(c.duration)), ("loop", Json::from(looping))])
            })),
        );
        r.set("graphs", Json::arr(set.graphs().take(32).map(|(n, _)| Json::from(n))));
    } else if let Some(rig) = &built.model.rig {
        r.set("bones", Json::from(rig.len()));
        r.set("clips", Json::arr([]));
    }
    if let Some(n) = &built.anim_note {
        r.set("anim_note", Json::from(n.as_str()));
    }
    if !built.textures.is_empty() {
        r.set(
            "textures",
            Json::arr(built.textures.iter().map(|t| {
                Json::obj([
                    ("width", Json::from(t.width)),
                    ("height", Json::from(t.height)),
                    ("srgb", Json::from(t.format == TextureFormat::Rgba8Srgb)),
                    ("mips", Json::from(t.mips)),
                ])
            })),
        );
    }
    if !built.warnings.is_empty() {
        r.set("import_warnings", Json::arr(built.warnings.iter().map(|w| Json::from(w.as_str()))));
    }
    r
}

fn preset_json(name: &str, p: &MaterialPreset) -> Json {
    let hex = |c: [f32; 4]| crate::world::color_hex([c[0], c[1], c[2]]);
    Json::obj([
        ("name", Json::from(name)),
        ("kind", Json::from(match p.kind {
            MaterialKind::Standard => "standard",
            MaterialKind::Glass => "glass",
        })),
        ("color", Json::from(hex(p.color))),
        ("roughness", Json::from(p.roughness)),
        ("metallic", Json::from(p.metallic)),
        ("emissive", Json::from(p.emissive)),
        ("specular", Json::from(p.specular)),
        ("opacity", Json::from(p.opacity)),
    ])
}

// ---------------------------------------------------------------------------------------------
// Levels of detail

/// The projected diameter, in pixels, of a sphere of `radius` at `distance` from a camera with a
/// vertical field of view `fov_y_deg` over `viewport_h` pixels. Inside the sphere: infinite.
pub fn screen_px(radius: f32, distance: f32, fov_y_deg: f32, viewport_h: u32) -> f32 {
    if distance <= radius {
        return f32::INFINITY;
    }
    let tan = (fov_y_deg.to_radians() * 0.5).tan().max(1e-6);
    radius / (distance * tan) * viewport_h as f32
}

/// The LOD for a model `px` pixels across with `count` LODs. With `previous` (the window), a
/// level changes only once `px` is [`LOD_HYSTERESIS`] past the threshold between them.
pub fn select_lod(px: f32, count: usize, previous: Option<usize>) -> usize {
    let last = count.saturating_sub(1);
    let threshold = |k: usize| LOD_PIXELS.get(k).copied().unwrap_or(0.0);
    match previous {
        None => {
            let mut level = 0;
            while level < last && px < threshold(level) {
                level += 1;
            }
            level
        }
        Some(p) => {
            let mut level = p.min(last);
            while level < last && px < threshold(level) * (1.0 - LOD_HYSTERESIS) {
                level += 1;
            }
            while level > 0 && px > threshold(level - 1) * (1.0 + LOD_HYSTERESIS) {
                level -= 1;
            }
            level
        }
    }
}

/// How models were drawn in one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LodStats {
    /// Model entities drawn at LOD 0, 1, 2 and 3+.
    pub histogram: [u32; LOD_BUCKETS],
    /// Model entities with nothing uploaded to draw (an unresolved or failed recipe).
    pub missing: u32,
    /// Model entities not drawn because their pipeline variant failed to build.
    pub pipeline_failed: u32,
    pub triangles: u64,
    /// Built model entities not drawn because their build isn't on the GPU (streaming: not yet
    /// uploaded, or evicted; see `streaming`).
    pub not_resident: u32,
    /// Model entities drawn at a coarser LOD than their size chose, the finer one still
    /// streaming in.
    pub coarser: u32,
    /// Model entities drawn merged into their cell's HLOD proxy (see `hlod`), not on their own.
    pub hlod: u32,
}

impl LodStats {
    pub fn to_json(self) -> Json {
        Json::obj([
            ("lod_histogram", Json::arr(self.histogram.map(Json::from))),
            ("missing", Json::from(self.missing)),
            ("pipeline_failed", Json::from(self.pipeline_failed)),
            ("triangles", Json::from(self.triangles)),
            ("not_resident", Json::from(self.not_resident)),
            ("coarser_lod", Json::from(self.coarser)),
            ("hlod", Json::from(self.hlod)),
        ])
    }
}

/// How a capture or window frame chooses LODs.
pub enum LodChoice<'a> {
    /// By size, remembering each entity's level (the window).
    Remembered(&'a mut BTreeMap<u64, u8>),
    /// By size, no memory (captures: reproducible).
    Fresh,
    /// Every model at this level (clamped).
    Fixed(usize),
    /// By size, remembering this one entity's level in the slot (`u8::MAX`: none yet): the
    /// window's cached draw list keeps a slot per model entity.
    Slot(&'a mut u8),
}

// ---------------------------------------------------------------------------------------------
// GPU

/// One material part of one LOD, uploaded.
pub(crate) struct PartDraw {
    pub(crate) mesh: MeshId,
    pub(crate) material: Material,
    pub(crate) surface: Surface,
    /// Multiplied by the entity's colour; alpha is the opacity of glass.
    pub(crate) color: [f32; 4],
    pub(crate) triangles: u32,
    /// Skinned parts: each bone the part's vertices are weighted to, and their farthest
    /// distance from it in bone space (see [`posed_sphere`]).
    pub(crate) influence: Vec<(u16, f32)>,
}

/// The largest factor by which the linear part of `cols` (three columns) can stretch a vector:
/// σ_max² ≤ min(Gershgorin row bound of MᵀM, ‖M‖_F²) — exact for rotation × scale.
fn stretch(cols: [[f32; 3]; 3]) -> f32 {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let [a, b, c] = cols;
    let (ab, bc, ac) = (dot(a, b).abs(), dot(b, c).abs(), dot(a, c).abs());
    let gershgorin = (dot(a, a) + ab + ac).max(dot(b, b) + ab + bc).max(dot(c, c) + ac + bc);
    gershgorin.min(dot(a, a) + dot(b, b) + dot(c, c)).sqrt()
}

/// Each bone a part's skinned vertices are weighted to, with the farthest such vertex's
/// distance from the bone's origin in bone space (`inverse_bind · v`).
pub fn part_influence(skeleton: &Skeleton, vertices: &[Vertex], skin: &[SkinVertex]) -> Vec<(u16, f32)> {
    let mut reach = vec![-1.0f32; skeleton.len()];
    for (v, s) in vertices.iter().zip(skin) {
        for k in 0..4 {
            let b = s.joints[k] as usize;
            if s.weights[k] > 0 && b < reach.len() {
                let p = skeleton.inverse_bind[b].apply_point(v.position);
                reach[b] = reach[b].max((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt());
            }
        }
    }
    reach.iter().enumerate().filter(|(_, r)| **r >= 0.0).map(|(b, r)| (b as u16, *r)).collect()
}

/// A model-space sphere holding every vertex of a part posed by `model` (the bones' posed
/// model-space transforms). A vertex weighted to bone b lies within `reach_b × stretch(b)` of
/// the bone's posed origin under that bone alone, and linear blend skinning puts it in the convex
/// hull of those points: inside the sphere around the union of the bones' spheres.
pub fn posed_sphere(influence: &[(u16, f32)], model: &[Affine]) -> Option<([f32; 3], f32)> {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for &(b, r) in influence {
        let m = model.get(b as usize)?;
        let rad = r * stretch(m.cols);
        for k in 0..3 {
            lo[k] = lo[k].min(m.t[k] - rad);
            hi[k] = hi[k].max(m.t[k] + rad);
        }
    }
    if lo[0] > hi[0] {
        return None;
    }
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let mut radius = 0.0f32;
    for &(b, r) in influence {
        let m = &model[b as usize];
        let d = [m.t[0] - c[0], m.t[1] - c[1], m.t[2] - c[2]];
        radius = radius.max((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() + r * stretch(m.cols));
    }
    // A hair of slack for rounding in the GPU's own skinning.
    (radius.is_finite()).then_some((c, radius * 1.001 + 1e-4))
}

pub(crate) struct GpuModel {
    pub(crate) lods: Vec<Vec<PartDraw>>,
    /// Bounding sphere in object space.
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
    /// Material textures, shared by every entity of the build.
    pub(crate) textures: Vec<TextureId>,
    /// The build's anim set (rigged models with clips).
    pub(crate) anims: Option<Arc<AnimSet>>,
    /// Pipeline variants its draws use: skinned (its meshes carry skin, when animated) and
    /// textured (a part has textures).
    pub(crate) needs: Needs,
    /// Its meshes carry skin, whether or not the build has clips of its own (a rig animated
    /// only by clips written into the world has none): what a caller that poses it is asked.
    pub(crate) rigged: bool,
    /// The finest LOD uploaded: levels from here to the last are drawable (streaming uploads the
    /// coarsest first; a frame wanting a finer one draws this one meanwhile). 0 = all of them.
    pub(crate) resident_from: usize,
    /// GPU bytes its meshes and textures take (streaming's memory budget).
    pub(crate) bytes: u64,
}

/// Pipeline variants a model's draws use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Needs {
    pub(crate) skinned: bool,
    pub(crate) textured: bool,
}

/// Why an uploaded model isn't drawn yet.
enum Held<'a> {
    /// Its pipeline variant is being built on a worker thread.
    Waiting,
    /// That build failed: it won't be drawn until a retry succeeds.
    Failed(&'a VariantFailure),
}

/// An entity's animator, kept across frames (seeked to each frame's time: no per-frame state).
struct EntityAnim {
    build: u64,
    animator: Animator,
    /// Graph-mode entities' evaluator (made on first use).
    graph: Option<Box<GraphPlayer>>,
    /// The frame it was last used in.
    used: u64,
}

/// Animators unused this many frames go back to their build's pool.
const ANIMATOR_IDLE_FRAMES: u64 = 120;
/// Spare animators kept per build (game entities come and go: reuse, don't allocate).
const ANIMATOR_POOL: usize = 64;

/// Per-frame animation state: the render time, the frame's bone matrices and the animators.
#[derive(Default)]
struct AnimFrame {
    time: f64,
    frame: u64,
    bones: Vec<[f32; 16]>,
    entities: BTreeMap<u64, EntityAnim>,
    pool: BTreeMap<u64, Vec<Animator>>,
    model: Vec<Affine>,
    skin: Vec<Affine>,
    /// What feet stand on (the world's ground entities).
    ground: Option<Arc<crate::anim::GroundGrid>>,
    /// A crossfade's other side (a graph faded from or into), reused.
    fade_player: GraphPlayer,
    fade_pose: Pose,
    own_pose: Pose,
    /// Ragdolls' poses this frame (the world's external poses).
    externals: BTreeMap<u64, crate::world::ExternalPose>,
}

/// Model → world of an entity, as the animation library's affine.
fn affine_of(m: &Mat4) -> Affine {
    let c = &m.cols;
    Affine { cols: [[c[0][0], c[0][1], c[0][2]], [c[1][0], c[1][1], c[1][2]], [c[2][0], c[2][1], c[2][2]]], t: [c[3][0], c[3][1], c[3][2]] }
}

/// The playback of `a` at render time `t` in `set` (`None`: no such clip).
fn playback(set: &AnimSet, a: &AnimSpec, t: f64) -> Option<Playback> {
    let clip = set.id(&a.clip)?;
    let c = set.clip(clip);
    let d = c.duration as f64;
    let mut u = a.clip_time(t);
    match (a.looping, c.looping) {
        // Hold the last pose (just short of the end, where a loop wraps to its start).
        (Some(false), LoopMode::Loop | LoopMode::PingPong) => u = u.min(d * (1.0 - 1e-6)),
        (Some(true), LoopMode::Once) if d > 0.0 => u -= (u / d).floor() * d,
        _ => {}
    }
    Some(Playback { clip, time: u, rate: 1.0 })
}

impl AnimFrame {
    /// The entity's animator slot for `build` (from the pool, or new).
    fn entry(&mut self, e: &Entity, build: u64, set: &AnimSet) -> &mut EntityAnim {
        let frame = self.frame;
        if !self.entities.get(&e.id).is_some_and(|x| x.build == build) {
            if let Some(old) = self.entities.remove(&e.id) {
                let pool = self.pool.entry(old.build).or_default();
                if pool.len() < ANIMATOR_POOL {
                    pool.push(old.animator);
                }
            }
            let animator = self.pool.get_mut(&build).and_then(Vec::pop).unwrap_or_else(|| Animator::new(set, specula_anim::ClipId(0)));
            self.entities.insert(e.id, EntityAnim { build, animator, graph: None, used: frame });
        }
        let entry = self.entities.get_mut(&e.id).expect("inserted");
        entry.used = frame;
        entry
    }

    /// A graph-mode (or graph-involving) anim: the graph's pose with IK, crossfaded with `from`
    /// when it fades. Leaves the pose in `self.model` / `self.skin`. Reuses every buffer.
    fn evaluate_graph(&mut self, e: &Entity, build: u64, set: &AnimSet, a: &AnimSpec) -> bool {
        let t = self.time;
        self.entry(e, build, set);
        let AnimFrame { entities, model, skin, ground, fade_player, fade_pose, own_pose, externals, .. } = self;
        let entry = entities.get_mut(&e.id).expect("made by entry");
        let to_world = affine_of(&e.model());
        let scale = crate::anim::entity_scale(e);
        let external = externals.get(&e.id).map(|x| &*x.pose);
        let around = Surroundings { to_world: Some(to_world), ground: ground.as_deref().map(|g| g as &dyn specula_anim::ik::Ground), external };
        // Evaluates one anim (graph or clip) into `out`; false when it has nothing to play.
        let pose_of = |spec: &AnimSpec, player: &mut GraphPlayer, animator: &mut Animator, out: &mut Pose| -> bool {
            match &spec.graph {
                Some(gp) => {
                    let Some(graph) = set.graph(&gp.graph) else { return false };
                    let pose = player.evaluate(Graphed { set, graph, scale }, gp, Timing { anchor: spec.start.unwrap_or(0.0), speed: spec.speed }, t, &around);
                    out.local.clone_from(&pose.local);
                    true
                }
                None => match playback(set, spec, t) {
                    Some(p) => {
                        animator.seek(p, None);
                        out.local.clone_from(&animator.evaluate(set).local);
                        true
                    }
                    None => false,
                },
            }
        };
        let player = entry.graph.get_or_insert_with(Default::default);
        if !pose_of(a, player, &mut entry.animator, own_pose) {
            return false;
        }
        // The anim it fades from (either kind), under this one.
        let w = a.fade_weight(t);
        if let Some(from) = a.from.as_deref().filter(|_| w < 1.0) {
            if pose_of(from, fade_player, &mut entry.animator, fade_pose) {
                specula_anim::blend::blend(fade_pose, own_pose, w, None);
                std::mem::swap(own_pose, fade_pose);
            }
        }
        set.skeleton.model_space(own_pose, model);
        set.skeleton.skin_matrices(model, skin);
        true
    }

    /// Leaves entity `e`'s pose at the frame's time in `self.model` / `self.skin`. False when it
    /// has no clip of `set` to play (it draws the bind pose).
    fn evaluate(&mut self, e: &Entity, build: u64, set: &AnimSet) -> bool {
        // A ragdoll's pose (physics, A4) plays through the graph's external nodes; an anim
        // without any (or no anim at all) shows it as it is.
        if let Some(x) = self.externals.get(&e.id).filter(|x| x.pose.local.len() == set.skeleton.len()) {
            let plays_it = e.anim.as_ref().and_then(|a| a.graph.as_ref()).and_then(|g| set.graph(&g.graph)).is_some_and(|g| g.nodes.iter().any(|n| matches!(n, specula_anim::graph::Node::External)));
            if x.direct || !plays_it {
                let p = Arc::clone(&x.pose);
                set.skeleton.model_space(&p, &mut self.model);
                set.skeleton.skin_matrices(&self.model, &mut self.skin);
                return true;
            }
        }
        let Some(a) = &e.anim else { return false };
        if a.graph.is_some() || a.from.as_deref().is_some_and(|f| f.graph.is_some()) {
            return self.evaluate_graph(e, build, set, a);
        }
        let t = self.time;
        let Some(base) = playback(set, a, t) else { return false };
        let fade = a
            .from
            .as_deref()
            .filter(|_| a.crossfade > 0.0)
            .and_then(|f| playback(set, f, t))
            .map(|p| (p, (t - a.start.unwrap_or(0.0)) as f32, a.crossfade));
        self.entry(e, build, set);
        let AnimFrame { entities, model, skin, .. } = self;
        let entry = entities.get_mut(&e.id).expect("made by entry");
        entry.animator.seek(base, fade);
        entry.animator.evaluate(set);
        entry.animator.skin(set, model, skin);
        true
    }
}

/// Uploaded builds and the handle → build map the renderer reads (see the module docs).
#[derive(Default)]
pub struct GpuModels {
    /// With a window: the engine-thread time a sync may spend uploading (the rest go on later
    /// frames); `None` (headless, tests): everything at once.
    pub(crate) upload_slice: Option<Duration>,
    pub(crate) uploaded: BTreeMap<u64, GpuModel>,
    /// Handle → build id, as of the last sync.
    pub(crate) by_handle: Vec<Option<u64>>,
    /// Builds whose upload failed, with why.
    pub(crate) failed: BTreeMap<u64, String>,
    /// `(world mesh epoch, library generation, world generation)` of the last sync.
    pub(crate) synced: Option<(u64, u64, u64)>,
    /// Entities drawn in an HLOD proxy this frame, one bit per persistent entity id (see
    /// `hlod`): their own draws are skipped. Empty when no HLOD applies.
    pub(crate) cover: Vec<u64>,
    /// Terrain entities the terrain renderer draws this frame (see `terrain`): their recipe's
    /// stand-in mesh is skipped. One bit per entity id.
    pub(crate) terrain_cover: Vec<u64>,
    /// Animation state, written while drawing (drawing takes `&self`).
    anim: RefCell<AnimFrame>,
    /// The GPU's pipeline variants as of the last sync: a model needing one not built yet (it is
    /// being built on a worker thread) is held back like a model still building.
    pub(crate) ready: PipelineVariants,
    /// Per build: the anim set its entities play (the world's assets laid over its own).
    anim_sets: BTreeMap<u64, Arc<AnimSet>>,
    /// The world's anim assets `anim_sets` were laid over with (see [`GpuModels::sync_anim_sets`]).
    sets_from: Option<Arc<crate::anim::Assets>>,
    /// The world's ground and anim assets as last read (see [`crate::anim::AnimEnv`]).
    env: crate::anim::AnimEnv,
    /// Builds kept uploaded though no entity wants them: the asset studio's calls in progress
    /// render them between frames (see [`GpuModels::set_pinned`]).
    pinned: BTreeSet<u64>,
    /// Builds uploaded since start-up (by the sync and by offscreen renders): a test counts a
    /// build uploaded twice, which it reads as no clock can.
    uploads: u64,
}

impl GpuModels {
    /// Builds uploaded since start-up ([`GpuModels::sync`] and [`GpuModels::posed_draws`]).
    pub fn uploads(&self) -> u64 {
        self.uploads
    }

    /// Tests: the next sync looks at every build again, as after a change to the world.
    #[cfg(test)]
    pub(crate) fn resync(&mut self) {
        self.synced = None;
    }

    /// Uploads the builds live model entities use and frees the rest, when the world's model
    /// entities or the library changed since the last call. Returns messages for failed uploads.
    pub fn sync(&mut self, gpu: &mut Gpu, library: &SharedLibrary, world: &World, world_generation: u64) -> Vec<String> {
        self.ready = gpu.pipeline_variants();
        self.refresh_anim_env(library, world, world_generation);
        let lib = library.lock();
        let key = (world.mesh_epoch(), lib.generation(), world_generation);
        if self.synced == Some(key) {
            if self.anim_sets_stale() {
                self.sync_anim_sets(&lib, world, true);
            }
            return Vec::new();
        }
        self.synced = Some(key);
        let live = live_handles(world);
        let before = std::mem::take(&mut self.by_handle);
        let mut wanted: BTreeMap<u64, Arc<Built>> = BTreeMap::new();
        for &h in &live {
            let Some(b) = lib.built(h) else { continue };
            if self.by_handle.len() <= h as usize {
                self.by_handle.resize(h as usize + 1, None);
            }
            self.by_handle[h as usize] = Some(b.id);
            wanted.insert(b.id, Arc::clone(b));
        }
        drop(lib);
        self.failed.retain(|id, _| wanted.contains_key(id));
        let mut errors = Vec::new();
        // Uploads are queued (copies into the staging arena, recorded for the next frame); a
        // window frame takes at most `upload_slice` of them (dozens of rebuilt models landing at
        // once held a window frame 10-20 ms), the rest go on the next frames.
        let started = Instant::now();
        let mut left = false;
        for (&id, built) in &wanted {
            if self.uploaded.contains_key(&id) || self.failed.contains_key(&id) {
                continue;
            }
            // A window frame never waits for an upload slot either (every batch slot in flight:
            // the GPU is behind); the rest go on the next frames.
            if self.upload_slice.is_some_and(|slice| started.elapsed() >= slice) || (self.upload_slice.is_some() && !gpu.upload_slot_free()) {
                left = true;
                break;
            }
            match upload(gpu, built) {
                Ok(m) => {
                    self.uploads += 1;
                    self.uploaded.insert(id, m);
                }
                Err(UploadError::Released) => {
                    // Its textures left the CPU with an earlier upload: build it again (its
                    // entities show as building meanwhile).
                    library.lock().reimport(id);
                }
                Err(UploadError::Gpu(e)) => {
                    let msg = format!("model '{}' not drawn: {e}", built.model.name);
                    errors.push(msg.clone());
                    self.failed.insert(id, msg);
                }
            }
        }
        // A handle whose new build isn't uploaded yet keeps drawing its previous one (a rebuild
        // never blinks out while the slice spreads the uploads); that build stays uploaded.
        let mut standing = std::collections::BTreeSet::new();
        for (h, slot) in self.by_handle.iter_mut().enumerate() {
            let Some(new) = *slot else { continue };
            if self.uploaded.contains_key(&new) {
                continue;
            }
            if let Some(old) = before.get(h).copied().flatten().filter(|old| self.uploaded.contains_key(old)) {
                *slot = Some(old);
                standing.insert(old);
            }
        }
        let stale: Vec<u64> = self.uploaded.keys().copied().filter(|id| !wanted.contains_key(id) && !self.pinned.contains(id) && !standing.contains(id)).collect();
        let mut released = Vec::new();
        for id in stale {
            if let Some(m) = self.uploaded.remove(&id) {
                // Retired, never waited for: freed once the frames that may draw them finish.
                for mesh in m.lods.iter().flatten().map(|p| p.mesh) {
                    gpu.retire_mesh(mesh);
                }
                if !m.textures.is_empty() {
                    released.push(id);
                }
                for t in m.textures {
                    gpu.retire_texture(t);
                }
            }
            self.anim.get_mut().pool.remove(&id);
        }
        if !released.is_empty() {
            // Their texture pixels are gone from the CPU too: the library forgets those builds.
            let mut lib = library.lock();
            for id in released {
                lib.forget_build(id);
            }
        }
        self.sync_anim_sets(&library.lock(), world, true);
        if left {
            // Sync again next frame for the rest.
            self.synced = None;
        }
        errors
    }

    /// The anim set entities of `handle` play when drawn (the world's assets laid over the
    /// build's own); `None` without one, or before the handle's build is known.
    pub fn anim_set_of(&self, handle: u32) -> Option<Arc<AnimSet>> {
        let id = self.by_handle.get(handle as usize).copied().flatten()?;
        self.anim_sets.get(&id).cloned().or_else(|| self.uploaded.get(&id).and_then(|m| m.anims.clone()))
    }

    /// Whether the world's anim assets changed since the anim sets were laid over with them
    /// (whoever read them: a tool, the clock or a sync).
    pub(crate) fn anim_sets_stale(&self) -> bool {
        !self.sets_from.as_ref().is_some_and(|a| Arc::ptr_eq(a, &self.env.assets))
    }

    /// Keeps the per-build anim sets in step with the world's anim assets: all laid over again
    /// when they changed, new builds' sets added, gone builds' dropped. `uploaded_only`: only
    /// builds on the GPU (the plain path uploads every live build); the streamed path keeps one
    /// per live build, so a build uploaded later plays the world's clips at once.
    pub(crate) fn sync_anim_sets(&mut self, lib: &Library, world: &World, uploaded_only: bool) {
        if self.anim_sets_stale() {
            self.anim_sets.clear();
            self.sets_from = Some(Arc::clone(&self.env.assets));
        }
        let live: BTreeSet<u64> = self.by_handle.iter().flatten().copied().collect();
        let uploaded = &self.uploaded;
        self.anim_sets.retain(|id, _| live.contains(id) && (!uploaded_only || uploaded.contains_key(id)));
        self.refresh_anim_sets(lib, world, uploaded_only);
    }

    /// Forgets the spare animators of build `id` (its upload is gone: see `streaming`).
    pub(crate) fn forget_animators(&mut self, id: u64) {
        self.anim.get_mut().pool.remove(&id);
    }

    /// Reads the world's ground and anim assets again when an entity that matters changed, and
    /// hands new assets to the library. Cheap when nothing did (an epoch compare); runs without a
    /// GPU too (headless engines settle graph writes against the library). Returns whether the
    /// assets changed.
    pub fn refresh_anim_env(&mut self, library: &SharedLibrary, world: &World, world_generation: u64) -> bool {
        let changed = self.env.refresh(world, world_generation);
        if changed {
            library.lock().set_anim_assets(Arc::clone(&self.env.assets));
        }
        self.anim.get_mut().ground = self.env.ground.clone();
        changed
    }

    /// Problems with the world's anim assets (entities that aren't valid assets).
    pub fn anim_asset_problems(&self) -> &[String] {
        &self.env.asset_problems
    }

    /// The world's ground for feet (`None`: no ground entities).
    pub fn ground(&self) -> Option<&Arc<crate::anim::GroundGrid>> {
        self.env.ground.as_ref()
    }

    /// Draws of `built`'s finest LOD posed by skinning matrices `skin`, placed by `world`, for an
    /// offscreen image (filmstrips): the build is uploaded if no entity has it on the GPU yet (it
    /// goes again at the next sync if none does then). The palette goes into `bones`.
    pub fn posed_draws(&mut self, gpu: &mut Gpu, built: &Arc<Built>, skin: &[Affine], world: &Mat4, bones: &mut Vec<[f32; 16]>, out: &mut Vec<DrawItem>) -> Result<(), String> {
        self.posed(gpu, built, skin, world, 0, true, bones, out)
    }

    /// [`GpuModels::posed_draws`] at level of detail `lod` (clamped to the model's last; the
    /// asset studio's shots choose it from the subject's size on screen, as the game does).
    #[allow(clippy::too_many_arguments)]
    pub fn posed_draws_lod(&mut self, gpu: &mut Gpu, built: &Arc<Built>, skin: &[Affine], world: &Mat4, lod: usize, bones: &mut Vec<[f32; 16]>, out: &mut Vec<DrawItem>) -> Result<(), String> {
        self.posed(gpu, built, skin, world, lod, false, bones, out)
    }

    /// `pose`: the caller poses the model with `skin` (a filmstrip's row), so it is drawn
    /// skinned whenever its meshes carry skin. Without it (the studio's bind-pose shots) only a
    /// build with clips of its own is. (A rig whose only clips are the world's has none of its
    /// own: its filmstrip drew every tile in the bind pose, while the same clip moved it in
    /// the world. 2026-11-17.)
    #[allow(clippy::too_many_arguments)]
    fn posed(&mut self, gpu: &mut Gpu, built: &Arc<Built>, skin: &[Affine], world: &Mat4, lod: usize, pose: bool, bones: &mut Vec<[f32; 16]>, out: &mut Vec<DrawItem>) -> Result<(), String> {
        if let std::collections::btree_map::Entry::Vacant(slot) = self.uploaded.entry(built.id) {
            let m = upload(gpu, built).map_err(|e| match e {
                UploadError::Released => "the model's textures were released; spawn an entity of it (or model_reload) and try again".to_string(),
                UploadError::Gpu(e) => format!("model upload failed: {e}"),
            })?;
            slot.insert(m);
            self.uploads += 1;
            if !self.pinned.contains(&built.id) {
                // The next sync drops it again unless an entity wants it.
                self.synced = None;
            }
        }
        let model = &self.uploaded[&built.id];
        let skinned = model.needs.skinned || (pose && model.rigged && !skin.is_empty());
        if let Some(h) = self.held_back_for(skinned, model.needs.textured) {
            return Err(match h {
                Held::Waiting => "the skinned pipeline is still being built; try again in a moment".into(),
                Held::Failed(f) => format!("the {} pipeline failed to build: {}", variant_names(f), f.error),
            });
        }
        let first = bones.len() as u32;
        bones.extend(skin.iter().map(Affine::to_cols_array));
        let cols = world.to_cols_array();
        // (Streaming may have only coarser levels on the GPU so far: the finest one there is.)
        let last = model.lods.len().saturating_sub(1);
        for p in &model.lods[lod.max(model.resident_from).min(last)] {
            out.push(DrawItem { mesh: p.mesh, model: cols, color: p.color, material: p.material, surface: p.surface, skin: skinned.then_some(first), bounds: None });
        }
        Ok(())
    }

    /// Builds to keep uploaded while no entity wants them (the asset studio's calls in progress):
    /// the sync between frames would otherwise drop each upload, and a textured glTF's texture
    /// pixels leave the CPU when uploaded. Builds that leave the set go at the next sync unless an
    /// entity wants them.
    /// Whether build `id` is pinned (see [`GpuModels::set_pinned`]): no sync or eviction drops it.
    pub(crate) fn is_pinned(&self, id: u64) -> bool {
        self.pinned.contains(&id)
    }

    pub fn set_pinned(&mut self, pinned: BTreeSet<u64>) {
        if self.pinned.iter().any(|id| !pinned.contains(id)) {
            self.synced = None;
        }
        self.pinned = pinned;
    }

    /// Each live (`uploaded_only`: uploaded) build's anim set from the library (the world's
    /// assets laid over its own).
    fn refresh_anim_sets(&mut self, lib: &Library, world: &World, uploaded_only: bool) {
        for h in live_handles(world) {
            let Some(Some(id)) = self.by_handle.get(h as usize).copied() else { continue };
            if self.anim_sets.contains_key(&id) || (uploaded_only && !self.uploaded.contains_key(&id)) {
                continue;
            }
            if let Some(set) = lib.anim_set(h) {
                self.anim_sets.insert(id, set);
            }
        }
    }

    /// The anim set a build's entities play.
    fn set_of<'a>(&'a self, id: u64, model: &'a GpuModel) -> Option<&'a Arc<AnimSet>> {
        self.anim_sets.get(&id).or(model.anims.as_ref())
    }

    /// Messages for builds that failed to upload (they aren't drawn).
    pub fn failures(&self) -> impl Iterator<Item = &String> {
        self.failed.values()
    }

    /// Builds and meshes on the GPU.
    pub fn stats_json(&self) -> Json {
        let meshes: usize = self.uploaded.values().map(|m| m.lods.iter().map(Vec::len).sum::<usize>()).sum();
        let textures: usize = self.uploaded.values().map(|m| m.textures.len()).sum();
        let anim = self.anim.borrow();
        Json::obj([
            ("builds_uploaded", Json::from(self.uploaded.len())),
            ("meshes", Json::from(meshes)),
            ("textures", Json::from(textures)),
            ("failed_uploads", Json::from(self.failed.len())),
            ("animated_entities", Json::from(anim.entities.len())),
            ("bone_matrices_last_frame", Json::from(anim.bones.len())),
        ])
    }

    /// The LOD a capture (no hysteresis) draws `e` at: `(level, levels, triangles at that
    /// level)`, or `None` when nothing is uploaded for it. `fixed` forces a level (clamped).
    pub fn lod_of(&self, e: &Entity, eye: Vec3, fov_y_deg: f32, viewport_h: u32, fixed: Option<usize>) -> Option<(usize, usize, u64)> {
        let id = (*self.by_handle.get(e.resolved_model()?.handle as usize)?)?;
        let model = self.uploaded.get(&id)?;
        let count = model.lods.len();
        let level = match fixed {
            Some(l) => l.min(count - 1),
            None => {
                let world = e.model();
                select_lod(sphere_px(&world, max_axis_scale(&world), model, eye, fov_y_deg, viewport_h), count, None)
            }
        };
        Some((level, count, model.lods[level].iter().map(|p| u64::from(p.triangles)).sum()))
    }

    /// The world's external poses (ragdolls) for this frame's draws (nothing allocates when
    /// there are none).
    pub fn set_externals(&self, world: &World) {
        let mut a = self.anim.borrow_mut();
        if a.externals.is_empty() && world.external_pose_ids().next().is_none() {
            return;
        }
        a.externals.clear();
        for id in world.external_pose_ids() {
            if let Some(x) = world.external_pose(id) {
                a.externals.insert(id, x.clone());
            }
        }
    }

    /// Starts a frame's draw list at render time `time_s` (engine clock seconds, or a capture's
    /// `time_s`): animated entities are posed at that time. Clears the frame's bone matrices.
    pub fn begin_frame(&self, time_s: f64) {
        let mut a = self.anim.borrow_mut();
        a.time = if time_s.is_finite() { time_s } else { 0.0 };
        a.frame += 1;
        a.bones.clear();
        if a.frame.is_multiple_of(64) {
            // Entities not drawn for a while (deleted, hidden, off screen) give their animators back.
            let frame = a.frame;
            let AnimFrame { entities, pool, .. } = &mut *a;
            let idle: Vec<u64> = entities.iter().filter(|(_, x)| x.used + ANIMATOR_IDLE_FRAMES < frame).map(|(id, _)| *id).collect();
            for id in idle {
                if let Some(x) = entities.remove(&id) {
                    let p = pool.entry(x.build).or_default();
                    if p.len() < ANIMATOR_POOL {
                        p.push(x.animator);
                    }
                }
            }
        }
    }

    /// The frame's bone matrices (for [`specula_gpu::FrameDesc::bones`]); give them back with
    /// [`GpuModels::put_bones`] so their storage is reused.
    pub fn take_bones(&self) -> Vec<[f32; 16]> {
        std::mem::take(&mut self.anim.borrow_mut().bones)
    }

    pub fn put_bones(&self, bones: Vec<[f32; 16]>) {
        self.anim.borrow_mut().bones = bones;
    }

    /// Takes the GPU's pipeline variants (a finished background build) without a full sync.
    pub fn refresh_ready(&mut self, gpu: &mut Gpu) {
        self.ready = gpu.pipeline_variants();
    }

    /// Entity `e` has an uploaded model whose pipeline variant is still being built: it isn't
    /// drawn yet (captures wait for it as for a build).
    pub fn waiting(&self, e: &Entity) -> bool {
        self.model_of(e).is_some_and(|(_, m)| matches!(self.held_back(e, m), Some(Held::Waiting)))
    }

    /// Entity `e` isn't drawn because the build of its pipeline variant failed: the warning
    /// (with the failing pipeline and its VkResult).
    pub fn pipeline_problem(&self, e: &Entity) -> Option<String> {
        let (_, m) = self.model_of(e)?;
        let Some(Held::Failed(f)) = self.held_back(e, m) else { return None };
        Some(format!(
            "'{}' is not drawn: building its {} pipelines failed ({}; pipeline {}); model_reload retries",
            e.name,
            variant_names(f),
            f.error,
            f.pipeline
        ))
    }

    /// The last failed background pipeline build, structured (perf_frame and sight_frame
    /// `problems`), until a retry (model_reload) starts.
    pub fn pipeline_failure_json(&self) -> Option<Json> {
        let f = self.ready.failed.as_ref()?;
        Some(Json::obj([
            ("code", Json::from("pipeline_variant_failed")),
            ("variants", Json::from(variant_names(f))),
            ("pipeline", Json::from(f.pipeline.as_str())),
            ("vk_result", f.error.vk_result.map_or(Json::Null, Json::from)),
            ("message", Json::from(f.error.to_string())),
        ]))
    }

    fn held_back(&self, e: &Entity, m: &GpuModel) -> Option<Held<'_>> {
        self.held_back_by(e.anim.is_some(), m)
    }

    fn held_back_by(&self, animated: bool, m: &GpuModel) -> Option<Held<'_>> {
        // An animated entity of a rigged model may be drawn skinned by clips written into the
        // world, whether or not its build has clips of its own: it waits for the skinned
        // pipelines too. (It did not, was drawn before they existed, and the GPU skipped it:
        // a capture taken then showed the world without it and nothing said why. 2026-11-17.)
        self.held_back_for((m.needs.skinned || m.rigged) && animated, m.needs.textured)
    }

    /// Draws that use the skinned or the textured pipelines wait for them.
    fn held_back_for(&self, skinned: bool, textured: bool) -> Option<Held<'_>> {
        let skinned = skinned && !self.ready.skinned;
        let textured = textured && !self.ready.textured;
        if !skinned && !textured {
            return None;
        }
        match &self.ready.failed {
            Some(f) if (skinned && f.skinned) || (textured && f.textured) => Some(Held::Failed(f)),
            _ => Some(Held::Waiting),
        }
    }

    fn model_of(&self, e: &Entity) -> Option<(u64, &GpuModel)> {
        self.model_of_handle(e.resolved_model().map(|r| r.handle))
    }

    fn model_of_handle(&self, handle: Option<u32>) -> Option<(u64, &GpuModel)> {
        let id = self.by_handle.get(handle.map_or(usize::MAX, |h| h as usize)).copied().flatten()?;
        self.uploaded.get(&id).map(|m| (id, m))
    }

    /// Entity `e`'s bones in world space at render time `time_s` (its animation's pose, or the
    /// bind pose), as (name, joint position), parents before children: for the sight skeleton
    /// overlay. `None` for entities without an uploaded rigged model with an anim set.
    pub fn bone_positions(&self, e: &Entity, time_s: f64) -> Option<Vec<(String, Vec3)>> {
        let (id, model) = self.model_of(e)?;
        let set = self.set_of(id, model)?;
        let mut a = self.anim.borrow_mut();
        let saved = a.time;
        a.time = time_s;
        let posed = a.evaluate(e, id, set);
        a.time = saved;
        let world = e.model();
        let heads: Vec<Vec3> = if posed {
            a.model.iter().map(|m| Vec3::new(m.t[0], m.t[1], m.t[2])).collect()
        } else {
            set.skeleton.heads.iter().map(|h| Vec3::new(h[0], h[1], h[2])).collect()
        };
        Some(set.skeleton.names.iter().zip(heads).map(|(n, p)| (n.clone(), world.transform_point(p))).collect())
    }

    /// Appends `e`'s draws (one per material part of the chosen LOD). `eye`, `fov_y_deg` and
    /// `viewport_h` size it on screen. An animated entity's palette goes into the frame's bone
    /// matrices (see [`GpuModels::begin_frame`]).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        e: &Entity,
        eye: Vec3,
        fov_y_deg: f32,
        viewport_h: u32,
        choice: &mut LodChoice,
        stats: &mut LodStats,
        out: &mut Vec<DrawItem>,
    ) {
        let m = ModelDraw::of(e);
        self.draw_model(&m, e.anim.is_some().then_some(e), eye, fov_y_deg, viewport_h, choice, stats, out);
    }

    /// [`GpuModels::draw`] from a model entity's drawing state `m` (what the window's draw cache
    /// keeps between frames). `animated` is the entity itself when it has an `anim` (its pose is
    /// evaluated at the frame's time), `None` otherwise.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_model(
        &self,
        m: &ModelDraw,
        animated: Option<&Entity>,
        eye: Vec3,
        fov_y_deg: f32,
        viewport_h: u32,
        choice: &mut LodChoice,
        stats: &mut LodStats,
        out: &mut Vec<DrawItem>,
    ) {
        if crate::hlod::covers(&self.cover, m.id) {
            stats.hlod += 1;
            return;
        }
        if crate::hlod::covers(&self.terrain_cover, m.id) {
            return; // drawn as terrain patches
        }
        let Some((id, model)) = self.model_of_handle(m.handle) else {
            // Built but not on the GPU (streaming) vs nothing built to draw.
            match m.handle.and_then(|h| self.by_handle.get(h as usize).copied().flatten()) {
                Some(b) if !self.failed.contains_key(&b) => stats.not_resident += 1,
                _ => stats.missing += 1,
            }
            return;
        };
        match self.held_back_by(animated.is_some(), model) {
            None => {}
            Some(Held::Waiting) => {
                stats.missing += 1;
                return;
            }
            Some(Held::Failed(_)) => {
                stats.pipeline_failed += 1;
                return;
            }
        }
        let world = &m.world;
        let count = model.lods.len();
        let level = match choice {
            LodChoice::Fixed(l) => (*l).min(count - 1),
            LodChoice::Fresh => select_lod(sphere_px(world, m.scale, model, eye, fov_y_deg, viewport_h), count, None),
            LodChoice::Remembered(levels) => {
                let px = sphere_px(world, m.scale, model, eye, fov_y_deg, viewport_h);
                let previous = levels.get(&m.id).map(|l| *l as usize);
                let level = select_lod(px, count, previous);
                if previous != Some(level) {
                    levels.insert(m.id, level as u8);
                }
                level
            }
            LodChoice::Slot(slot) => {
                let px = sphere_px(world, m.scale, model, eye, fov_y_deg, viewport_h);
                let previous = (**slot != u8::MAX).then_some(**slot as usize);
                let level = select_lod(px, count, previous);
                **slot = level as u8;
                level
            }
        };
        // A finer level still streaming in: the finest one uploaded meanwhile.
        if level < model.resident_from {
            stats.coarser += 1;
        }
        let level = level.max(model.resident_from);
        stats.histogram[level.min(LOD_BUCKETS - 1)] += 1;
        // Every part of the LOD shares the entity's palette; each part is culled by its posed
        // sphere.
        let mut posed = None;
        let skin = match (self.set_of(id, model), animated) {
            (Some(set), Some(e)) => {
                let mut a = self.anim.borrow_mut();
                if a.evaluate(e, id, set) {
                    let AnimFrame { bones, skin, .. } = &mut *a;
                    let first = bones.len() as u32;
                    bones.extend(skin.iter().map(Affine::to_cols_array));
                    posed = Some(a);
                    Some(first)
                } else {
                    None
                }
            }
            _ => None,
        };
        let tint = m.color;
        for p in &model.lods[level] {
            stats.triangles += u64::from(p.triangles);
            let bounds = posed.as_ref().and_then(|a| posed_sphere(&p.influence, &a.model)).map(|(centre, r)| {
                let w = world.transform_point(Vec3::new(centre[0], centre[1], centre[2]));
                [w.x, w.y, w.z, r * m.stretch]
            });
            out.push(DrawItem {
                mesh: p.mesh,
                model: m.cols,
                color: [p.color[0] * tint[0], p.color[1] * tint[1], p.color[2] * tint[2], p.color[3]],
                material: p.material,
                surface: p.surface,
                skin,
                bounds,
            });
        }
    }
}

/// What drawing a model entity needs from it besides its animation: copied out of the entity,
/// so the window's draw cache can draw static models without touching the world.
#[derive(Clone, Copy, Debug)]
pub struct ModelDraw {
    pub id: u64,
    /// The build's handle (`None` while unresolved).
    pub handle: Option<u32>,
    pub world: Mat4,
    pub cols: [f32; 16],
    /// How far the world transform stretches a unit vector at most (scales posed spheres).
    pub stretch: f32,
    /// [`max_axis_scale`] of `world` (sizes the bounding sphere on screen for the LOD).
    pub scale: f32,
    pub color: [f32; 3],
}

impl ModelDraw {
    pub fn of(e: &Entity) -> ModelDraw {
        let world = e.model();
        let c = &world.cols;
        let stretch = stretch([[c[0][0], c[0][1], c[0][2]], [c[1][0], c[1][1], c[1][2]], [c[2][0], c[2][1], c[2][2]]]);
        let scale = max_axis_scale(&world);
        ModelDraw { id: e.id, handle: e.resolved_model().map(|r| r.handle), world, cols: world.to_cols_array(), stretch, scale, color: e.color }
    }
}

/// `scale`: [`max_axis_scale`] of `world`.
fn sphere_px(world: &Mat4, scale: f32, model: &GpuModel, eye: Vec3, fov_y_deg: f32, viewport_h: u32) -> f32 {
    let center = world.transform_point(model.center);
    screen_px(model.radius * scale, (center - eye).length(), fov_y_deg, viewport_h)
}

/// The longest of a transform's three axis columns: how much it scales a model's bounding
/// sphere at most.
fn max_axis_scale(world: &Mat4) -> f32 {
    let c = &world.cols;
    [c[0], c[1], c[2]].iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0f32, f32::max)
}

/// Engine-thread time one window frame's model sync spends uploading builds (at least one a
/// frame; [`GpuModels::upload_slice`]).
pub const UPLOAD_SLICE: Duration = Duration::from_millis(3);

/// Why a build could not be uploaded.
enum UploadError {
    /// Its texture pixels went to the GPU in an earlier upload: import it again.
    Released,
    Gpu(String),
}

/// One mesh per LOD per material part (with its skin stream when rigged, and UVs when its
/// material is textured), and the build's textures, once. Queued, never waited for: the copies
/// go before the next frame on the same queue (a rebuilt model landing in the window used to
/// upload and free synchronously, waiting for the GPU: frame stalls in the M4 exit run).
fn upload(gpu: &mut Gpu, built: &Built) -> Result<GpuModel, UploadError> {
    let model = &built.model;
    let mut textures: Vec<TextureId> = Vec::with_capacity(built.textures.len());
    let mut meshes: Vec<MeshId> = Vec::new();
    let undo = |gpu: &mut Gpu, meshes: &[MeshId], textures: &[TextureId]| {
        meshes.iter().for_each(|m| gpu.retire_mesh(*m));
        textures.iter().for_each(|t| gpu.retire_texture(*t));
    };
    for t in &built.textures {
        // The pixels go to the GPU and leave the CPU (see `TextureData`).
        let data = std::mem::take(&mut *t.levels.lock().unwrap_or_else(PoisonError::into_inner));
        if data.is_empty() {
            undo(gpu, &meshes, &textures);
            return Err(UploadError::Released);
        }
        let levels: Vec<&[u8]> = data.iter().map(Vec::as_slice).collect();
        match gpu.queue_texture_levels(t.width, t.height, t.format, &levels, t.sampler) {
            Ok(id) => textures.push(id),
            Err(e) => {
                undo(gpu, &meshes, &textures);
                return Err(UploadError::Gpu(e.to_string()));
            }
        }
    }
    let mut lods = Vec::with_capacity(model.lods.len());
    let mut mesh_bytes = 0u64;
    for lod in &model.lods {
        let mut parts = Vec::with_capacity(lod.parts.len());
        for (i, part) in lod.parts.iter().enumerate() {
            let preset = &model.materials.iter().find(|(n, _)| *n == part.material).expect("checked when built").1;
            let textured = built
                .materials
                .get(&part.material)
                .filter(|m| m.textured() && !lod.uvs.is_empty() && preset.kind == MaterialKind::Standard);
            let p = part_mesh(lod, i, textured.is_some());
            if p.indices.is_empty() {
                continue;
            }
            let streams = MeshStreams { skin: p.skin.as_deref(), uvs: p.uvs.as_deref(), tangents: p.tangents.as_deref() };
            mesh_bytes += p.bytes();
            let mesh = match gpu.queue_mesh(&p.vertices, &p.indices, &streams) {
                Ok(m) => m,
                Err(e) => {
                    undo(gpu, &meshes, &textures);
                    return Err(UploadError::Gpu(e.to_string()));
                }
            };
            meshes.push(mesh);
            let (material, surface, color) = match textured {
                Some(m) => textured_look(m, &textures),
                None => part_look(preset, p.mean),
            };
            let surface = with_wind(surface, model.wind);
            let influence = match (&built.anims, &p.skin) {
                (Some(set), Some(skin)) => part_influence(&set.skeleton, &p.vertices, skin),
                _ => Vec::new(),
            };
            parts.push(PartDraw { mesh, material, surface, color, triangles: (p.indices.len() / 3) as u32, influence });
        }
        lods.push(parts);
    }
    let b = model.bounds;
    let (lo, hi) = (Vec3::new(b.min[0], b.min[1], b.min[2]), Vec3::new(b.max[0], b.max[1], b.max[2]));
    let needs = Needs {
        skinned: built.anims.is_some() && model.lods.iter().any(MeshData::is_skinned),
        textured: lods.iter().flatten().any(|p: &PartDraw| p.surface.textures != MaterialTextures::NONE),
    };
    let bytes = texture_bytes(built) + mesh_bytes;
    Ok(GpuModel {
        lods,
        center: (lo + hi) * 0.5,
        radius: ((hi - lo).length() * 0.5).max(1e-3),
        textures,
        anims: built.anims.clone(),
        needs,
        rigged: model.lods.iter().any(MeshData::is_skinned),
        resident_from: 0,
        bytes,
    })
}

/// One part of a mesh alone (re-indexed): vertices, indices, mean vertex colour (sRGB), and its
/// skin stream (rigged meshes), UVs (when asked and present) and the file's tangents (when every
/// vertex of the part has one; otherwise the GPU generates them).
pub struct PartMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub mean: [f32; 3],
    pub skin: Option<Vec<SkinVertex>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
}

impl PartMesh {
    /// GPU bytes of its streams as the GPU stores them (vertices, skin, UV + tangent, indices).
    pub fn bytes(&self) -> u64 {
        let n = self.vertices.len() as u64;
        n * 28 + self.skin.as_ref().map_or(0, |_| n * 8) + self.uvs.as_ref().map_or(0, |_| n * 16) + self.indices.len() as u64 * 4
    }
}

/// GPU bytes of a build's textures with their mip chains (about 4/3 of the base level).
pub(crate) fn texture_bytes(built: &Built) -> u64 {
    built.textures.iter().map(|t| u64::from(t.width) * u64::from(t.height) * t.format.bytes_per_pixel() as u64 * 4 / 3).sum()
}

/// Part `i` of `m` alone (see [`PartMesh`]).
pub fn part_mesh(m: &MeshData, i: usize, with_uvs: bool) -> PartMesh {
    let src = m.part_indices(i);
    let mut remap = vec![u32::MAX; m.positions.len()];
    let mut vertices = Vec::new();
    let mut indices = Vec::with_capacity(src.len());
    let skinned = m.is_skinned();
    let with_uvs = with_uvs && m.uvs.len() == m.positions.len();
    let with_tangents = with_uvs && m.tangents.len() == m.positions.len();
    let (mut skin, mut uvs, mut tangents) = (Vec::new(), Vec::new(), Vec::new());
    let mut sum = [0.0f64; 3];
    for &v in src {
        let slot = &mut remap[v as usize];
        if *slot == u32::MAX {
            *slot = vertices.len() as u32;
            let v = v as usize;
            let c = m.colors[v];
            for k in 0..3 {
                sum[k] += f64::from(c[k]);
            }
            vertices.push(Vertex::with_color(m.positions[v], m.normals[v], c));
            if skinned {
                skin.push(SkinVertex { joints: m.joints[v], weights: m.weights[v] });
            }
            if with_uvs {
                uvs.push(m.uvs[v]);
            }
            if with_tangents {
                tangents.push(m.tangents[v]);
            }
        }
        indices.push(*slot);
    }
    let n = vertices.len().max(1) as f64;
    PartMesh {
        vertices,
        indices,
        mean: sum.map(|s| (s / n / 255.0) as f32),
        skin: skinned.then_some(skin),
        uvs: with_uvs.then_some(uvs),
        tangents: (with_tangents && tangents.iter().all(|t| t[0] != 0.0 || t[1] != 0.0 || t[2] != 0.0)).then_some(tangents),
    }
}

/// A part's surface with the recipe's wind term (foliage sways in `world_env {wind}`).
pub(crate) fn with_wind(mut surface: Surface, wind: Option<specula_model::Wind>) -> Surface {
    if let Some(w) = wind {
        surface.wind = specula_gpu::Wind { amount: w.amount, height: w.height, painted: w.painted };
    }
    surface
}

/// A style preset as the engine's material (see the module docs).
pub fn part_look(p: &MaterialPreset, mean: [f32; 3]) -> (Material, Surface, [f32; 4]) {
    let surface = Surface { emissive: p.emissive, metallic: p.metallic, roughness: p.roughness, specular: p.specular, ..Surface::default() };
    match p.kind {
        MaterialKind::Standard => (Material::Lit, surface, [1.0; 4]),
        MaterialKind::Glass => (Material::Glass, surface, [mean[0], mean[1], mean[2], p.opacity]),
    }
}

/// A textured glTF material as the engine's (`textures`: the build's uploaded textures).
pub(crate) fn textured_look(m: &PartMaterial, textures: &[TextureId]) -> (Material, Surface, [f32; 4]) {
    let tex = |i: Option<usize>| i.and_then(|i| textures.get(i).copied());
    let surface = Surface {
        emissive: m.emissive_strength,
        metallic: m.metallic.clamp(0.0, 1.0),
        roughness: m.roughness.clamp(0.0, 1.0),
        specular: 1.0,
        textures: MaterialTextures {
            base_color: tex(m.base),
            normal: tex(m.normal),
            normal_scale: m.normal_scale,
            metallic_roughness: tex(m.metallic_roughness),
            emissive: tex(m.emissive),
            emissive_color: (m.emissive_strength > 0.0).then_some(m.emissive_color),
            alpha_cutoff: m.alpha_cutoff,
        },
        ..Surface::default()
    };
    (Material::Lit, surface, [m.color[0], m.color[1], m.color[2], 1.0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, SpawnSpec, UpdateSpec};

    fn sepulcra() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
    }

    /// A scratch assets root with a style and the given recipes (`recipes/<name>.json`).
    fn scratch(tag: &str, recipes: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("specula-models-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("recipes")).unwrap();
        std::fs::write(
            root.join(STYLE_FILE),
            r##"{"style":"t","materials":{"stone":{"color":"#808080","roughness":0.7},
                "ghost":{"kind":"glass","color":"#9cff4f","opacity":0.35,"emissive":0.5}}}"##,
        )
        .unwrap();
        for (name, text) in recipes {
            std::fs::write(root.join("recipes").join(format!("{name}.json")), text).unwrap();
        }
        root
    }

    fn block(height: f32) -> String {
        format!(
            r#"{{"recipe":1,"name":"block","params":{{"tier":{{"default":1,"min":1,"max":3,"integer":true}}}},
               "parts":[{{"name":"body","material":"stone","shape":{{"op":"box","size":[1,"{height} * tier",1]}}}},
                        {{"name":"halo","material":"ghost","shape":{{"op":"sphere","radius":0.3}},
                          "ops":[{{"op":"move","by":[0,"{height} * tier + 0.3",0]}}]}}]}}"#
        )
    }

    /// A recipe whose only part is `block` (included).
    const HOST: &str = r#"{"recipe":1,"name":"host","parts":[{"name":"b","include":"block"}]}"#;

    /// A heavy-but-legal recipe (from the review's probe): many subdivisions and noise passes,
    /// within the work budget (the probe's 30 displaces and 10 cavity passes are over it now).
    fn heavy() -> String {
        let mut ops = vec![r#"{"op":"subdivide","levels":4}"#.to_string(), r#"{"op":"subdivide","levels":1}"#.to_string()];
        ops.extend((0..8).map(|_| r#"{"op":"displace","amount":0.01,"scale":0.5,"octaves":6}"#.to_string()));
        ops.extend((0..2).map(|_| r#"{"op":"ao","amount":0.3}"#.to_string()));
        let part = |i: u32| format!(r#"{{"name":"p{i}","material":"stone","shape":{{"op":"box","size":[1,1,1]}},"ops":[{}]}}"#, ops.join(","));
        format!(r#"{{"recipe":1,"name":"heavy","lods":[0.5,0.25],"budget":{{"max_triangles":200000}},"parts":[{},{}]}}"#, part(0), part(1))
    }

    /// Takes finished builds until none is in flight (or 60 s pass).
    fn settle(lib: &mut Library) -> BTreeSet<u32> {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut changed = BTreeSet::new();
        // At least once: builds a graph write waited for are handed on by the next finish.
        loop {
            changed.extend(lib.finish(Some(Duration::from_millis(50))).1);
            if lib.in_flight() == 0 || Instant::now() >= deadline {
                break;
            }
        }
        assert_eq!(lib.in_flight(), 0, "builds didn't finish");
        changed
    }

    fn settle_shared(shared: &SharedLibrary, world: &mut World) {
        let changed = settle(&mut shared.lock());
        world.refresh_model_handles(&changed, false);
    }

    /// Graph anims (A3): parameter writes settle and blend, are checked against the graph, round
    /// trip through JSON, and the world's anim assets lay over the recipe's anim set.
    #[test]
    fn graph_anims_settle_on_write_and_world_assets_lay_over() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        spec.name = Some("walker".into());
        let mut play = GraphPlay::new("creature");
        play.set_param("act", Value::Name("idle".into()));
        spec.anim = Some(AnimSpec::graph(play));
        let id = world.spawn(spec, "t").unwrap().id;
        settle_shared(&shared, &mut world);
        // Start walking at 5 s: the play settles (idle stays under the 0.3 s blend), speed is added.
        world.set_clock_s(5.0);
        let mut walk = GraphPlay::new("creature");
        walk.set_param("act", Value::Name("move".into()));
        walk.set_param("speed", Value::Num(0.8));
        world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(walk))), ..Default::default() }, "t").unwrap();
        let a = world.get(id).unwrap().anim.clone().unwrap();
        let gp = a.graph.as_deref().unwrap();
        assert_eq!(a.start, Some(5.0));
        assert_eq!(gp.param("act"), Some(&Value::Name("move".into())));
        assert!((gp.clock - 5.0).abs() < 1e-9, "graph time ran from the spawn's anchor (0) to 5 s");
        assert_eq!(gp.layers[0].stack.iter().map(|e| e.state.as_str()).collect::<Vec<_>>(), ["idle"], "settled at the write: the move starts from the anchor");
        // A second write with only `speed` keeps `act` (parameters merge).
        world.set_clock_s(6.0);
        let mut faster = GraphPlay::new("creature");
        faster.set_param("speed", Value::Num(0.4));
        world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(faster))), ..Default::default() }, "t").unwrap();
        let a = world.get(id).unwrap().anim.clone().unwrap();
        let gp = a.graph.as_deref().unwrap();
        assert_eq!((gp.param("act"), gp.param("speed")), (Some(&Value::Name("move".into())), Some(&Value::Num(0.4))));
        assert_eq!(gp.layers[0].stack.last().unwrap().state, "move", "the move fired and blended in by 6 s");
        // JSON round trip (world files, world_set).
        let back = AnimSpec::from_json(&a.to_json()).unwrap();
        assert_eq!(back, a);
        // Unknown parameters and graphs are refused with the field.
        let mut bad = GraphPlay::new("creature");
        bad.set_param("pace", Value::Num(2.0));
        let e = world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(bad))), ..Default::default() }, "t").unwrap_err();
        assert_eq!(e.detail("field"), Some(&Json::from("anim.params.pace")), "{e:?}");
        let e = world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(GraphPlay::new("dance")))), ..Default::default() }, "t").unwrap_err();
        assert_eq!(e.detail("field"), Some(&Json::from("anim.graph")));
        // A world anim asset replaces the recipe's graph for every shambler.
        let graph = r#"{"params": {"act": {"type": "enum", "values": ["idle", "move", "attack", "special", "death"]}, "speed": "float"},
                        "states": {"idle": "attack", "move": "walk"},
                        "transitions": [{"from": "idle", "to": "move", "when": "act == move"}]}"#;
        let mut asset = SpawnSpec::new(crate::world::Shape::Group);
        asset.name = Some("creature".into());
        asset.tags.insert(crate::anim::ASSET_TAG.into());
        asset.props = Json::obj([(crate::anim::ASSET_PROP, Json::obj([("for", Json::from("recipes/shambler.json")), ("kind", Json::from("graph")), ("data", Json::parse(graph).unwrap())]))]);
        world.spawn(asset, "t").unwrap();
        let mut models = GpuModels::default();
        assert!(models.refresh_anim_env(&shared, &world, 0), "the asset is new");
        assert!(!models.refresh_anim_env(&shared, &world, 0), "nothing changed since");
        let handle = world.get(id).unwrap().resolved_model().unwrap().handle;
        let set = shared.lock().anim_set(handle).unwrap();
        assert_eq!(set.graph("creature").unwrap().layers[0].states.len(), 2, "the world's graph");
        assert_eq!(set.graph("creature").unwrap().params.len(), 2);
        // An entity of the model now settles against it: 'look' is gone.
        let mut look = GraphPlay::new("creature");
        look.set_param("look", Value::Num(1.0));
        assert!(world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(look))), ..Default::default() }, "t").is_err());
        assert!(anim_problem(&shared.lock(), world.get(id).unwrap()).is_some(), "its stored 'look' parameter is reported");
    }

    /// Review round 1, finding 2: a graph write used to store its anim unsettled (and unmerged)
    /// when the model wasn't built yet, so the hashed world depended on build timing.
    #[test]
    fn graph_writes_store_the_same_state_whenever_the_model_builds() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        let run = |build_first: bool| {
            let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
            let mut world = World::default();
            world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
            let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
            spec.name = Some("walker".into());
            let mut play = GraphPlay::new("creature");
            play.set_param("act", Value::Name("move".into()));
            play.set_param("speed", Value::Num(0.8));
            spec.anim = Some(AnimSpec::graph(play));
            let id = world.spawn(spec, "t").unwrap().id;
            if build_first {
                settle_shared(&shared, &mut world);
            }
            world.set_clock_s(5.0);
            let mut w = GraphPlay::new("creature");
            w.set_param("speed", Value::Num(0.4));
            world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(w))), ..Default::default() }, "t").unwrap();
            let hash_at_write = world.state_hash();
            // The builds the write waited for still reach the engine.
            settle_shared(&shared, &mut world);
            assert_eq!(world.get(id).unwrap().resolved_model().unwrap().state, ModelState::Ready);
            (hash_at_write, world.state_hash(), world.get(id).unwrap().anim.as_ref().unwrap().to_json().to_string())
        };
        let (a, b) = (run(true), run(false));
        assert_eq!(a, b, "built before the write vs after");
        assert_eq!(a.0, a.1);
        assert!(a.2.contains("\"act\":\"move\""), "the write kept 'act': {}", a.2);
    }

    /// Review round 2, finding 1: a graph write never waits for a build, and what it stores is
    /// the same whether the build landed first, lands later, never lands (no build threads), or
    /// waits behind 1,600 queued builds.
    #[test]
    fn graph_writes_never_wait_and_never_depend_on_builds() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        // `queue` other builds ahead; `land`: let builds finish before the write.
        let run = |workers: usize, queue: u64, land: bool| {
            let shared = Arc::new(SharedLibrary::new(Library::with_workers(&sepulcra(), None, workers)));
            let mut world = World::default();
            world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
            for k in 0..queue {
                world.spawn(SpawnSpec::model(ModelSpec::new("recipes/bone_colossus.json").with_seed(1000 + k)), "t").unwrap();
            }
            let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
            spec.name = Some("walker".into());
            let mut play = GraphPlay::new("creature");
            play.set_param("act", Value::Name("move".into()));
            play.set_param("speed", Value::Num(0.8));
            spec.anim = Some(AnimSpec::graph(play));
            let id = world.spawn(spec, "t").unwrap().id;
            if land {
                settle_shared(&shared, &mut world);
            }
            world.set_clock_s(5.0);
            let mut w = GraphPlay::new("creature");
            w.set_param("speed", Value::Num(0.4));
            let t = Instant::now();
            world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(w))), ..Default::default() }, "t").unwrap();
            let took = t.elapsed();
            // A bad parameter is refused the same way whether or not anything was built.
            let mut bad = GraphPlay::new("creature");
            bad.set_param("pace", Value::Num(1.0));
            assert!(world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(bad))), ..Default::default() }, "t").is_err());
            let anim = world.get(id).unwrap().anim.as_ref().unwrap().to_json().to_string();
            let in_flight = shared.lock().in_flight();
            (anim, took, in_flight)
        };
        let landed = run(WORKERS, 0, true);
        assert!(landed.0.contains("\"layers\"") && landed.0.contains("\"act\":\"move\""), "settled and merged: {}", landed.0);
        let never = run(0, 0, false);
        assert_eq!(never.0, landed.0, "no build ever lands");
        assert!(never.2 > 0, "its build is still queued");
        let queued = run(WORKERS, 1600, false);
        assert_eq!(queued.0, landed.0, "1,600 builds queued ahead");
        assert!(queued.2 > 0, "builds still in flight at the write");
        eprintln!("graph write: {:?} built, {:?} never built, {:?} with 1,600 queued", landed.1, never.1, queued.1);
        assert!(queued.1 < specula_core::speed_budget(std::time::Duration::from_secs(2)), "the write took {:?}", queued.1);
    }

    /// Review round 2, finding 1: a source changed on disk while its rebuild is in flight: writes
    /// settle against the new anim set as soon as the library polls (never when the build lands),
    /// the poll names the handles, and the engine's re-settle writes their plays.
    #[test]
    fn changed_anim_sets_apply_at_the_poll() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        let root = std::env::temp_dir().join(format!("specula-models-animreload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["recipes", "anims"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in ["recipes/shambler.json", "anims/shambler.json", STYLE_FILE] {
            std::fs::copy(sepulcra().join(f), root.join(f)).unwrap();
        }
        // No build threads: nothing ever lands, so only the poll can change what writes see.
        let shared = Arc::new(SharedLibrary::new(Library::with_workers(&root, None, 0)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        let mut play = GraphPlay::new("creature");
        play.set_param("act", Value::Name("move".into()));
        spec.anim = Some(AnimSpec::graph(play));
        let id = world.spawn(spec, "t").unwrap().id;
        let write = |world: &mut World, p: &str| {
            let mut g = GraphPlay::new("creature");
            g.set_param(p, Value::Num(1.0));
            world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(g))), ..Default::default() }, "t").map(|_| ())
        };
        write(&mut world, "speed").unwrap();
        assert!(write(&mut world, "pace").is_err());
        // The anims file changes: a graph with a 'pace' parameter only.
        let text = std::fs::read_to_string(root.join("anims/shambler.json")).unwrap();
        let start = text.find("\"graphs\"").unwrap();
        let changed = format!(r#"{}"graphs": {{ "creature": {{ "params": {{ "pace": "float" }}, "states": {{ "idle": "idle" }} }} }} }}"#, &text[..start]);
        std::fs::write(root.join("anims/shambler.json"), changed).unwrap();
        // Until the library polls, writes still see the old set.
        write(&mut world, "speed").unwrap();
        let reload = shared.lock().poll(true).unwrap();
        let handle = world.get(id).unwrap().resolved_model().unwrap().handle;
        assert!(reload.anim_handles.contains(&handle), "{:?}", reload.anim_handles);
        world.refresh_models();
        assert_eq!(crate::anim::resettle(&mut world, &reload.anim_handles), 1, "the play re-settled against the new set");
        write(&mut world, "pace").unwrap();
        assert!(write(&mut world, "speed").is_err(), "the new set has no 'speed'");
        assert!(shared.lock().built(handle).is_none(), "nothing was ever built");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Gives a file the time `at` (a write within the clock tick of the last one keeps the time).
    fn set_time(path: &Path, at: SystemTime) {
        std::fs::OpenOptions::new().write(true).open(path).unwrap().set_modified(at).unwrap();
    }

    #[test]
    fn a_time_is_compared_with_only_once_it_is_old() {
        let now = SystemTime::now();
        let (ago, ahead) = (|ms: u64| now - Duration::from_millis(ms), |ms: u64| now + Duration::from_millis(ms));
        assert!(old_enough(ago(10_000), now, now) && old_enough(ago(2_001), now, now));
        for fresh in [ago(2_000), ago(1), now, ahead(1), ahead(2_000)] {
            assert!(!old_enough(fresh, now, now), "{:?}", now.duration_since(fresh));
        }
        // A time well ahead (a file from a machine whose clock is ahead, an unpacked archive)
        // is as good as an old one: the next write by this machine's clock will not carry it.
        // (Where another machine stamps every write, a share whose server's clock is ahead,
        // two writes in one of its ticks are as before: ROADMAP.)
        assert!(old_enough(ahead(60_000), now, now));
        // The clock on both sides of the look. A thread that stalled for a minute between
        // them: a write that landed in that minute is fresh, whenever in it; one from before
        // it is old, and only a time past the second reading is "ahead".
        let (before, after) = (now, ahead(60_000));
        for landed in [ahead(1), ahead(3_000), ahead(59_999), ahead(60_000), ahead(62_000)] {
            assert!(!old_enough(landed, before, after), "{:?}", landed.duration_since(now));
        }
        assert!(old_enough(ago(2_001), before, after) && old_enough(ahead(62_001), before, after));
        // And through a file: one made a moment ago has a time and no stamp; 10 s on it has one.
        let file = std::env::temp_dir().join(format!("specula-models-stamp-{}", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let made = mtime(&file).unwrap();
        NOW.set(Some(made));
        assert_eq!(stamp_of(&file), (Some(made), None));
        NOW.set(Some(made + Duration::from_secs(10)));
        assert_eq!(stamp_of(&file), (Some(made), Some(made)));
        let _ = std::fs::remove_file(&file);
        assert_eq!(stamp_of(&file), (None, None));
        NOW.set(None);
    }

    /// An outside review's run on Linux (2026-11-15): `changed_anim_sets_apply_at_the_poll`
    /// failed in one run of the suite in two, the poll naming no handle. A file's time moves in
    /// steps of the clock; there a copy carries the time of the copying, so the test's rewrite a
    /// moment later could carry the same one, and the library compared times alone. (On Windows
    /// a copy keeps its source's old time, and the test never failed.) Here the rewrite is
    /// given the time the file had: a recipe, a style and an anim set written again within the
    /// tick they were read in are seen, by what they say; and once their times are old, the
    /// times are what is compared.
    #[test]
    fn a_file_written_again_within_the_clock_tick_it_was_read_in_is_seen() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        let root = std::env::temp_dir().join(format!("specula-models-sametick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["recipes", "anims"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        // "Now" stands still for this thread (the library reads on it: no workers), and the
        // files carry times beside it: 100 ms ago is fresh, 10 s ago is old.
        let now = SystemTime::now();
        NOW.set(Some(now));
        let (fresh, old, older) = (now - Duration::from_millis(100), now - Duration::from_secs(10), now - Duration::from_secs(11));
        let files = ["recipes/shambler.json", "anims/shambler.json", STYLE_FILE].map(|f| root.join(f));
        for (f, name) in files.iter().zip(["recipes/shambler.json", "anims/shambler.json", STYLE_FILE]) {
            std::fs::copy(sepulcra().join(name), f).unwrap();
            set_time(f, fresh);
        }
        let [recipe, anims, style] = &files;
        let shared = Arc::new(SharedLibrary::new(Library::with_workers(&root, None, 0)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        let mut play = GraphPlay::new("creature");
        play.set_param("act", Value::Name("move".into()));
        spec.anim = Some(AnimSpec::graph(play));
        let id = world.spawn(spec, "t").unwrap().id;
        let handle = world.get(id).unwrap().resolved_model().unwrap().handle;
        // What a poll says changed: (files, the handles whose anim set did).
        let poll = |world: &mut World| {
            let r = shared.lock().poll(true);
            world.refresh_models();
            r.map_or((0, false), |r| (r.files_changed, r.anim_handles.contains(&handle)))
        };
        // Written again, each with a time put back to the one it had.
        let rewrite = |f: &Path, text: String, at: SystemTime| {
            std::fs::write(f, text).unwrap();
            set_time(f, at);
        };
        let read = |f: &Path| std::fs::read_to_string(f).unwrap();
        assert_eq!(poll(&mut world), (0, false), "nothing was written");
        let text = read(anims);
        let start = text.find("\"graphs\"").unwrap();
        let other = format!(r#"{}"graphs": {{ "creature": {{ "params": {{ "pace": "float" }}, "states": {{ "idle": "idle" }} }} }} }}"#, &text[..start]);
        rewrite(anims, other.clone(), fresh);
        assert!(poll(&mut world).1, "the anim set, written again with the time it had");
        assert_eq!(poll(&mut world), (0, false), "and seen once");
        rewrite(recipe, read(recipe) + " ", fresh);
        assert!(poll(&mut world).0 >= 1, "the recipe");
        rewrite(style, read(style) + "\n", fresh);
        assert!(poll(&mut world).0 >= 1, "the style");
        assert_eq!(poll(&mut world), (0, false), "each seen once: a file read again that says the same changes nothing");
        // Their times grow old: the next poll reads them once more, finds them saying the same,
        // and takes the times.
        for f in &files {
            set_time(f, old);
        }
        assert_eq!(poll(&mut world), (0, false));
        // From here a time is what tells: a writer that puts an old time back is not seen (as
        // ever), and one that moves it is.
        rewrite(recipe, read(recipe) + " ", old);
        rewrite(anims, text.clone(), old);
        rewrite(style, read(style) + "\n", old);
        assert_eq!(poll(&mut world), (0, false), "old times that did not move: nothing is read");
        for f in &files {
            set_time(f, older);
        }
        // (Two files counted: the recipe and the style. The recipe's own change already had its
        // anim set worked out again.)
        let (changed, anim) = poll(&mut world);
        assert!(changed >= 2 && anim, "three files whose times moved ({changed}, {anim})");
        // A style that goes while its time is fresh is seen gone (no time then, none now: read).
        rewrite(style, read(style) + "
", fresh);
        assert!(poll(&mut world).0 >= 1);
        std::fs::remove_file(style).unwrap();
        assert!(poll(&mut world).0 >= 1, "the style went");
        NOW.set(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A folder with nothing in it, read a moment after it was made and then removed: with no
    /// time for it in the cache and none on the disk, it is still seen gone, by a scan and by
    /// a request's own look.
    #[test]
    fn an_empty_folder_that_went_while_its_time_was_fresh_is_seen_gone() {
        let root = scratch("freshgone", &[("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        // A folder made, read with "now" being its own time (fresh, however long this took),
        // and removed.
        let made_read_and_removed = |lib: &mut Library, name: &str| {
            let rel = PathBuf::from("recipes").join(name);
            std::fs::create_dir_all(root.join(&rel)).unwrap();
            NOW.set(mtime(&root.join(&rel)));
            assert_eq!(lib.request(&ModelSpec::new(format!("recipes/{name}/none.json"))).err().map(|e| e.code), Some("recipe_not_found"));
            assert!(lib.folders.get(&rel).is_some_and(|f| f.files.is_empty() && f.mtime.is_none()), "cached, empty, its time not known");
            std::fs::remove_dir(root.join(&rel)).unwrap();
            rel
        };
        let by_scan = made_read_and_removed(&mut lib, "scan");
        let scanned = lib.scan_plan().run();
        assert!(scanned.folders.iter().any(|(rel, now)| *rel == by_scan && now.is_none()), "the scan says it went");
        let by_request = made_read_and_removed(&mut lib, "request");
        assert_eq!(lib.request(&ModelSpec::new("recipes/request/none.json")).err().map(|e| e.code), Some("recipe_not_found"));
        assert!(!lib.folders.contains_key(&by_request), "the request's look dropped it");
        NOW.set(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review round 3, finding 1: seeded variants used to each build (and evict) their own anim
    /// set; past 64 of them every write regenerated the clips (65 shamblers: 270-320 ms a round).
    /// Variants with the same rig and anims file share one base, and one overlay per revision.
    #[test]
    fn seeded_variants_share_one_anim_set() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        let shared = Arc::new(SharedLibrary::new(Library::with_workers(&sepulcra(), None, 0)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let ids: Vec<u64> = (0..100)
            .map(|k| {
                let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json").with_seed(1 + k));
                let mut play = GraphPlay::new("creature");
                play.set_param("act", Value::Name("move".into()));
                spec.anim = Some(AnimSpec::graph(play));
                world.spawn(spec, "t").unwrap().id
            })
            .collect();
        let mut rounds = Vec::new();
        for round in 0..3 {
            world.set_clock_s(5.0 + round as f64);
            let t = Instant::now();
            for &id in &ids {
                let mut w = GraphPlay::new("creature");
                w.set_param("speed", Value::Num(0.4 + round as f32 * 0.1));
                world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(w))), ..Default::default() }, "t").unwrap();
            }
            rounds.push(t.elapsed());
        }
        eprintln!("100 seeded shamblers, one write each: {rounds:?}");
        let lib = shared.lock();
        assert_eq!(lib.writes.bases.len(), 1, "one rig and anims file: one anim set");
        assert_eq!(lib.writes.aliases.len(), 100);
        assert!(rounds[2] < specula_core::speed_budget(std::time::Duration::from_secs(1)), "{rounds:?}");
    }

    /// Review round 3, finding 3: the write-set cache is bounded in bytes as well as entries.
    #[test]
    fn write_sets_are_bounded_in_bytes() {
        let mut w = WriteSets::default();
        for k in 0..5u64 {
            let used = w.tick();
            w.make_room(20 << 20);
            w.bases.insert(k, Cached { used, set: Err("big".into()), bytes: 20 << 20 });
        }
        let total: usize = w.bases.values().map(|c| c.bytes).sum();
        assert!(total <= MAX_WRITE_BYTES && w.bases.len() == 3, "{} sets, {total} bytes", w.bases.len());
        assert!(!w.bases.contains_key(&0) && w.bases.contains_key(&4), "the least recently used went first");
        for k in 0..2 * MAX_WRITE_SETS as u64 {
            let used = w.tick();
            w.make_room(1);
            w.overlays.insert((k, 0), Cached { used, set: Err("small".into()), bytes: 1 });
        }
        assert!(w.bases.len() + w.overlays.len() <= MAX_WRITE_SETS);
    }

    /// Review R1 of m5-export, the blocker, on the import path: an imported `MASK` material
    /// whose texture is opaque (what other tools' assets usually are) keeps every mip opaque,
    /// and a real cut-out keeps its coverage with distance.
    #[test]
    fn an_imported_mask_keeps_solid_surfaces_solid_and_cut_outs_covered() {
        let root = scratch("maskmips", &[]);
        let style = Style::parse(&std::fs::read_to_string(sepulcra().join(STYLE_FILE)).unwrap()).unwrap();
        let mut lib_files = BTreeMap::new();
        for e in std::fs::read_dir(sepulcra().join("recipes")).unwrap() {
            let p = e.unwrap().path();
            lib_files.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
        let grass = build(&lib_files["grass_patch"], &BuildParams { library: Some(&lib_files), ..BuildParams::new(&style) }).unwrap();
        assert!(grass.extras.textures[0].1.alpha_cutoff > 0.0, "the grass is a cut-out");
        let cutoff8 = (grass.extras.textures[0].1.alpha_cutoff * 255.0).round() as u8;
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let levels_of = |model: &Model, file: &str| -> Vec<Vec<u8>> {
            let glb = specula_gltf::export_glb(&specula_gltf::ExportInput::new(model)).unwrap();
            std::fs::write(root.join("assets").join(file), &glb).unwrap();
            let (_, x) = import_gltf(&root.join("assets").join(file)).unwrap();
            let base = x.materials.values().find_map(|m| m.base.filter(|_| m.alpha_cutoff > 0.0)).expect("a masked material with a base colour texture");
            let levels = x.textures[base].levels.lock().unwrap().clone();
            levels
        };
        // As it is: thin blades. Every level covers at least half of level 0's share (plain
        // averaging loses them: `alpha_coverage_is_kept_through_the_mips`).
        let cover = |l: &[u8]| l.as_chunks::<4>().0.iter().filter(|p| p[3] >= cutoff8).count() as f64 / (l.len() / 4) as f64;
        let blades = levels_of(&grass, "blades.glb");
        assert!(cover(&blades[0]) > 0.02 && blades.len() > 4);
        for (i, l) in blades.iter().enumerate().take(4) {
            assert!(cover(l) >= cover(&blades[0]) * 0.5, "level {i}: {} of {}", cover(l), cover(&blades[0]));
        }
        // The same model with its texture made opaque: a solid surface under a MASK material,
        // as an asset from another tool. Every mip stays opaque.
        let mut solid = grass.clone();
        for (_, set) in &mut solid.extras.textures {
            let mut opaque = (**set).clone();
            opaque.base_color.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
            opaque.hash ^= 1;
            *set = Arc::new(opaque);
        }
        let levels = levels_of(&solid, "solid.glb");
        assert!(levels.iter().all(|l| l.as_chunks::<4>().0.iter().all(|p| p[3] == 255)), "an opaque mask's mips are opaque: nothing is cut out of a solid surface");
        // One texture, tested two ways: left with plain mips.
        let m = |mode, cutoff| specula_gltf::ImportedMaterial { alpha_mode: mode, alpha_cutoff: cutoff, base_color_texture: Some(specula_gltf::TextureRef { texture: 0, tex_coord: 0, scale: 1.0 }), ..plain_material() };
        use specula_gltf::AlphaMode::{Blend, Mask, Opaque};
        assert_eq!(mask_cutoff(&[m(Mask, 0.5)], 0), Some(0.5));
        assert_eq!(mask_cutoff(&[m(Mask, 0.5), m(Mask, 0.5)], 0), Some(0.5));
        assert_eq!(mask_cutoff(&[m(Mask, 0.5), m(Mask, 0.9)], 0), None);
        assert_eq!(mask_cutoff(&[m(Mask, 0.5), m(Blend, 0.5)], 0), None);
        assert_eq!(mask_cutoff(&[m(Opaque, 0.5), m(Mask, 0.5)], 0), None);
        assert_eq!(mask_cutoff(&[m(Opaque, 0.5)], 0), None);
        assert_eq!(mask_cutoff(&[m(Mask, 0.5)], 1), None, "another texture");
        assert_eq!(mask_cutoff(&[m(Mask, 0.0)], 0), None);
    }

    /// An imported material with nothing set.
    fn plain_material() -> specula_gltf::ImportedMaterial {
        specula_gltf::ImportedMaterial {
            name: "m".into(),
            preset: MaterialPreset { kind: MaterialKind::Standard, color: [1.0; 4], roughness: 1.0, metallic: 0.0, emissive: 0.0, specular: 0.25, opacity: 1.0 },
            base_color: [1.0; 4],
            base_color_texture: None,
            metallic: 1.0,
            roughness: 1.0,
            metallic_roughness_texture: None,
            normal_texture: None,
            occlusion_texture: None,
            emissive: [0.0; 3],
            emissive_strength: 1.0,
            emissive_texture: None,
            alpha_mode: specula_gltf::AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            from_specula: false,
            texture_baked: false,
        }
    }

    /// Review round 3, finding 4: a glTF changed on disk since the library last looked at it is
    /// not settled against until the next poll announces it.
    #[test]
    fn gltf_write_sets_wait_for_the_poll() {
        let root = scratch("gltfwrite", &[]);
        let style = Style::parse(&std::fs::read_to_string(sepulcra().join(STYLE_FILE)).unwrap()).unwrap();
        let mut lib_files = BTreeMap::new();
        for e in std::fs::read_dir(sepulcra().join("recipes")).unwrap() {
            let p = e.unwrap().path();
            lib_files.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
        let model = build(&lib_files["wraith"], &BuildParams { library: Some(&lib_files), ..BuildParams::new(&style) }).unwrap();
        let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
        let set = AnimSet::load(&std::fs::read_to_string(sepulcra().join("anims/wraith.json")).unwrap(), skel).unwrap();
        let clips: Vec<_> = set.clips().map(|(_, c)| c.clone()).collect();
        let glb = specula_gltf::export_glb(&specula_gltf::ExportInput { model: &model, lod: 0, skeleton: Some(&set.skeleton), clips: &clips }).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/wraith.glb"), &glb).unwrap();
        let mut lib = Library::with_workers(&root, None, 0);
        let h = lib.resolve(&ModelSpec::new("assets/wraith.glb")).unwrap().handle;
        // Changed on disk before the first write (and before any poll): refused for now.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("assets/wraith.glb"), &glb[..glb.len()]).unwrap();
        let mut grown = glb.clone();
        grown.extend_from_slice(&[0u8; 4]);
        std::fs::write(root.join("assets/wraith.glb"), &grown).unwrap();
        let none = crate::anim::Assets::new();
        assert!(lib.write_set(h, &none).is_none(), "unannounced content is not settled against");
        // Back as polled: the set comes.
        std::fs::write(root.join("assets/wraith.glb"), &glb).unwrap();
        lib.poll(true);
        let h = lib.resolve(&ModelSpec::new("assets/wraith.glb")).unwrap().handle;
        let s = lib.write_set(h, &none).expect("polled content");
        assert!(s.graph("creature").is_none() && s.len() > 5, "the glTF's clips");
    }

    /// The anim set built from the rig alone matches the one the full build makes.
    #[test]
    fn write_sets_match_built_anim_sets() {
        let mut lib = Library::new(&sepulcra(), None);
        let names = ["shambler", "skeleton_runner", "wraith", "bone_colossus"];
        let handles: Vec<u32> = names.iter().map(|n| lib.resolve(&ModelSpec::new(format!("recipes/{n}.json"))).unwrap().handle).collect();
        settle(&mut lib);
        let none = crate::anim::Assets::new();
        for (n, h) in names.iter().zip(handles) {
            let quick = lib.write_set(h, &none).unwrap_or_else(|| panic!("{n}: no write set"));
            let built = Arc::clone(lib.built(h).unwrap().anims.as_ref().unwrap());
            assert_eq!(quick.skeleton, built.skeleton, "{n}");
            assert_eq!(quick.len(), built.len(), "{n}");
            for ((_, a), (_, b)) in quick.clips().zip(built.clips()) {
                assert_eq!(a, b, "{n}/{}", a.name);
            }
        }
    }

    /// Review round 1, finding 2: world assets reach a graph write at once (they used to wait
    /// for the engine's next frame, so a batch or undo then a write settled against a stale graph).
    #[test]
    fn graph_writes_see_world_assets_as_of_the_write() {
        use specula_anim::graph::Value;
        use specula_anim::play::GraphPlay;
        let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        spec.anim = Some(AnimSpec::graph(GraphPlay::new("creature")));
        let id = world.spawn(spec, "t").unwrap().id;
        settle_shared(&shared, &mut world);
        let graph = r#"{"params": {"pace": "float"}, "states": {"idle": "idle"}}"#;
        let mut asset = SpawnSpec::new(crate::world::Shape::Group);
        asset.name = Some("creature".into());
        asset.tags.insert(crate::anim::ASSET_TAG.into());
        asset.props = Json::obj([(crate::anim::ASSET_PROP, Json::obj([("for", Json::from("recipes/shambler.json")), ("kind", Json::from("graph")), ("data", Json::parse(graph).unwrap())]))]);
        let asset_id = world.spawn(asset, "t").unwrap().id;
        let write = |world: &mut World, p: &str| {
            let mut g = GraphPlay::new("creature");
            g.set_param(p, Value::Num(1.0));
            world.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::graph(g))), ..Default::default() }, "t").map(|_| ())
        };
        // No engine frame ran: the write still knows the asset's graph.
        write(&mut world, "pace").unwrap();
        assert!(write(&mut world, "speed").is_err(), "the world's graph has no 'speed'");
        // Removing the asset brings the recipe's graph back at once.
        world.delete(&EntityKey::Id(asset_id), None).unwrap();
        write(&mut world, "speed").unwrap();
    }

    #[test]
    fn every_sepulcra_recipe_builds_in_the_engine() {
        let mut lib = Library::new(&sepulcra(), None);
        let mut handles = Vec::new();
        for e in std::fs::read_dir(sepulcra().join("recipes")).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            if text.contains("\"texgraph\"") {
                continue; // a texture graph (recipes use it), not a recipe
            }
            let tiers: Vec<Option<f64>> = if text.contains("\"tier\"") { vec![Some(1.0), Some(2.0), Some(3.0)] } else { vec![None] };
            for t in tiers {
                let mut spec = ModelSpec::new(format!("recipes/{name}"));
                if let Some(t) = t {
                    spec.set_param("tier", t);
                }
                let r = lib.resolve(&spec).unwrap_or_else(|e| panic!("{name} {t:?}: {e}"));
                handles.push((name.clone(), r.handle));
            }
        }
        settle(&mut lib);
        for (name, h) in &handles {
            let b = lib.built(*h).unwrap_or_else(|| panic!("{name}: {:?}", lib.handle_error(*h)));
            let (lo, hi) = b.bounds();
            assert!(hi.y > lo.y && hi.x > lo.x, "{name}: bounds {lo:?} {hi:?}");
        }
        assert!(handles.len() >= 20, "{} builds", handles.len());
    }

    #[test]
    fn identical_specs_share_one_build_and_variants_do_not() {
        let root = scratch("share", &[("block", &block(1.0)), ("twin", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let a = ModelSpec::new("recipes/block.json").with_param("tier", 2.0);
        let first = lib.resolve(&a).unwrap();
        assert_eq!((first.state, first.bounds), (ModelState::Building, None), "spawns never wait for a build");
        assert_eq!(lib.resolve(&a).unwrap(), first, "one handle per spec");
        // The same content under another name is the same build (keyed by content, not path).
        let twin = lib.resolve(&ModelSpec::new("recipes/twin.json").with_param("tier", 2.0)).unwrap();
        assert_ne!(twin.handle, first.handle);
        assert_eq!(lib.in_flight(), 1, "one build in flight for both");
        let t3 = lib.resolve(&ModelSpec::new("recipes/block.json").with_param("tier", 3.0)).unwrap();
        let seeded = lib.resolve(&a.clone().with_seed(5)).unwrap();
        settle(&mut lib);
        assert_eq!(lib.built(twin.handle).unwrap().id, lib.built(first.handle).unwrap().id);
        let ids: BTreeSet<u64> = [first, t3, seeded].iter().map(|r| lib.built(r.handle).unwrap().id).collect();
        assert_eq!(ids.len(), 3, "other parameters or seeds are other builds");
        let r3 = lib.resolve(&ModelSpec::new("recipes/block.json").with_param("tier", 3.0)).unwrap();
        assert_eq!((r3.state, r3.bounds.unwrap().1.y), (ModelState::Ready, 3.0 + 0.9), "bounds come from the model");
        match lib.request(&a).unwrap() {
            BuildRequest::Ready(_, cached) => assert!(cached),
            BuildRequest::Pending(_) => panic!("should be cached"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Performance pass, 2026-11-14 (the model library's lock). A build request read the disk
    /// under the lock (a `stat` of every file of its folder; a whole folder not cached yet), on
    /// every studio check and model_build, and the engine thread waited for the lock while
    /// agents' connections did that. A request through the shared library reads the disk
    /// **without the lock**, on each of its paths: a scan of this test's folder stops where it
    /// is about to read, and the lock is free then.
    #[test]
    fn a_request_reads_the_disk_without_holding_the_lock() {
        let root = scratch("lock_free", &[("block", &block(1.0))]);
        let shared = Arc::new(SharedLibrary::new(Library::new(&root, None)));
        // Every scan of this root says it is about to read, then waits to be let go.
        let (at_disk_tx, at_disk) = mpsc::channel::<()>();
        let (go_tx, go) = mpsc::channel::<()>();
        let (at_disk_tx, go) = (Mutex::new(at_disk_tx), Mutex::new(go));
        let here = std::thread::current().id();
        let _watch = watch_scans(&root, move || {
            // (This test's own thread scans freely: a poll it makes while a request is stopped.)
            if std::thread::current().id() == here {
                return;
            }
            at_disk_tx.lock().unwrap().send(()).unwrap();
            go.lock().unwrap().recv_timeout(Duration::from_secs(60)).expect("the test lets the scan go");
        });
        let ask = |recipe: &str, what: &str| -> Result<(), &'static str> {
            let (shared_there, spec) = (Arc::clone(&shared), ModelSpec::new(recipe));
            let asking = std::thread::spawn(move || shared_there.request(&spec).map(|_| ()).map_err(|e| e.code));
            at_disk.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| panic!("{what}: the request reads the disk"));
            // The request is at the disk now. Whoever wants the library has it.
            assert!(shared.0.try_lock().is_ok(), "{what}: the lock is held while the disk is read");
            go_tx.send(()).unwrap();
            asking.join().unwrap()
        };
        // A folder that is not cached yet (read whole).
        assert_eq!(ask("recipes/block.json", "a folder not cached"), Ok(()));
        // A cached folder (its files' times checked).
        assert_eq!(ask("recipes/block.json", "a cached folder"), Ok(()));
        // A recipe the cached folder lacks (the folder read again, whatever its times say): a
        // typo is not found, and a file written since is.
        assert_eq!(ask("recipes/nope.json", "a recipe that is not there"), Err("recipe_not_found"));
        std::fs::write(root.join("recipes").join("late.json"), block(2.0)).unwrap();
        assert_eq!(ask("recipes/late.json", "a recipe new on disk"), Ok(()));
        // A folder that does not exist.
        assert_eq!(ask("nowhere/block.json", "a folder that is not there"), Err("recipe_not_found"));
        // A folder the garbage collection dropped (no handle uses it) is read again, off the lock.
        shared.lock().collect_garbage(&BTreeSet::new());
        assert!(!shared.lock().folders.contains_key(Path::new("recipes")), "nothing uses the folder: it went");
        assert_eq!(ask("recipes/block.json", "a folder the collection dropped"), Ok(()));
        // The texts a recorded check is compared with are read the same way.
        let (shared_there, spec) = (Arc::clone(&shared), ModelSpec::new("recipes/block.json"));
        let asking = std::thread::spawn(move || shared_there.current_texts(&spec).is_ok());
        at_disk.recv_timeout(Duration::from_secs(60)).unwrap();
        assert!(shared.0.try_lock().is_ok(), "current_texts: the lock is held while the disk is read");
        go_tx.send(()).unwrap();
        assert!(asking.join().unwrap());
        // The engine thread's request of a source the library knows reads nothing; one it does
        // not know is not answered that way at all.
        assert!(shared.lock().request_known(&ModelSpec::new("recipes/unasked.json")).is_none());
        assert!(matches!(shared.lock().request_known(&ModelSpec::new("recipes/block.json")), Some(Ok(_))), "a known source is requested without the disk");
        assert!(at_disk.recv_timeout(Duration::from_millis(50)).is_err(), "request_known made a scan");
        // Review R2: a collection that lands while a request reads (it drops the folder the
        // read was planned from, and a read of a cached folder brings only what changed) does
        // not make the recipe "not found": the request reads again, whole. Every project
        // write forces a collection.
        let (shared_there, spec) = (Arc::clone(&shared), ModelSpec::new("recipes/block.json"));
        let asking = std::thread::spawn(move || shared_there.request(&spec).map(|_| ()).map_err(|e| e.code));
        at_disk.recv_timeout(Duration::from_secs(60)).expect("the request reads the disk");
        shared.lock().collect_garbage(&BTreeSet::new());
        assert!(!shared.lock().folders.contains_key(Path::new("recipes")));
        go_tx.send(()).unwrap();
        at_disk.recv_timeout(Duration::from_secs(60)).expect("what it planned from went: it reads again");
        assert!(shared.0.try_lock().is_ok(), "the second read: the lock is held while the disk is read");
        go_tx.send(()).unwrap();
        assert_eq!(asking.join().unwrap(), Ok(()), "a collection landed while the request read");
        // And a poll that lands while a request reads a folder that is not cached (the poll
        // does not read folders no handle uses) does not make its read dropped.
        shared.lock().collect_garbage(&BTreeSet::new());
        let (shared_there, spec) = (Arc::clone(&shared), ModelSpec::new("recipes/block.json"));
        let asking = std::thread::spawn(move || shared_there.request(&spec).map(|_| ()).map_err(|e| e.code));
        at_disk.recv_timeout(Duration::from_secs(60)).expect("the request reads the disk");
        let poll = shared.lock().scan_plan().run();
        let _ = shared.lock().apply_scan(poll, true);
        go_tx.send(()).unwrap();
        assert_eq!(asking.join().unwrap(), Ok(()), "a poll landed while the request read");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R1 of m5-liblock: reads made without the lock land in any order, and an older
    /// read of a folder must never be put over a newer one, nor a request build from a read
    /// older than its own. Each interleaving the review found, by hand.
    #[test]
    fn reads_of_a_folder_are_applied_in_the_order_they_were_planned() {
        let root = scratch("ordered", &[("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let spec = ModelSpec::new("recipes/block.json");
        let text = |lib: &Library| lib.folders[Path::new("recipes")].files["block"].text.as_deref().ok().map(str::to_owned);
        let edit = |height: f32| {
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(root.join("recipes").join("block.json"), block(height)).unwrap();
        };
        assert!(lib.request(&spec).is_ok());
        // Two requests' reads, applied in reverse: A read 1.5, B (planned and read later) read
        // 2.0 and is applied first. A's read is not put over it, and A's request builds 2.0.
        edit(1.5);
        let a = lib.request_plan(&spec).run();
        edit(2.0);
        let b = lib.request_plan(&spec).run();
        assert!(lib.request_read(&spec, b).is_ok());
        assert_eq!(text(&lib), Some(block(2.0)));
        assert!(lib.request_read(&spec, a).is_ok());
        assert_eq!(text(&lib), Some(block(2.0)), "an older read of the folder was put over a newer one");
        // A poll planned and read before an edit, applied before a later request's read: the
        // request's own read is applied over it (it is the newer), in the same hold as the request.
        let poll = lib.scan_plan().run();
        edit(2.5);
        let b = lib.request_plan(&spec).run();
        let _ = lib.apply_scan(poll, false);
        assert!(lib.request_read(&spec, b).is_ok());
        assert_eq!(text(&lib), Some(block(2.5)));
        // And applied after it: the poll's older read of the folder is dropped.
        let poll = lib.scan_plan().run();
        edit(3.0);
        let b = lib.request_plan(&spec).run();
        assert!(lib.request_read(&spec, b).is_ok());
        let _ = lib.apply_scan(poll, false);
        assert_eq!(text(&lib), Some(block(3.0)), "a poll's older read of the folder was put over a request's");
        // A text the project store wrote after a request's read was planned stands over it.
        edit(3.5);
        let stale = lib.request_plan(&spec).run();
        let noted: Arc<str> = Arc::from(block(4.0).as_str());
        assert!(lib.note_written(&[("recipes/block.json".to_owned(), Some(Arc::clone(&noted)))]));
        assert!(lib.request_read(&spec, stale).is_ok());
        assert_eq!(text(&lib).as_deref(), Some(&*noted), "the noted text stands");
        // The old way (read under the caller's lock) counts as a read too.
        let poll = lib.scan_plan().run();
        edit(4.5);
        assert!(lib.request(&spec).is_ok());
        let _ = lib.apply_scan(poll, false);
        assert_eq!(text(&lib), Some(block(4.5)));
        // A poll applied while a request's read of a folder that is not cached is in flight
        // raises the floor of whole scans; the request's read is applied all the same (the
        // poll did not read that folder), and the recipe is found (review R2).
        lib.collect_garbage(&BTreeSet::new());
        assert!(!lib.folders.contains_key(Path::new("recipes")), "no handle uses the folder: it went");
        let uncached = lib.request_plan(&spec).run();
        let poll = lib.scan_plan().run();
        let _ = lib.apply_scan(poll, true);
        assert!(lib.request_read(&spec, uncached).is_ok(), "a poll landed first: the request's own read of the folder was dropped");
        // A read planned from the cache that finds its folder gone says so (the request reads
        // again), and never "not found".
        let cached = lib.request_plan(&spec).run();
        lib.collect_garbage(&BTreeSet::new());
        assert!(matches!(lib.request_read(&spec, cached), Err(ReadError::Stale)));
        // So does a whole read under the lock (a spawn's first use of a folder, after a
        // collection dropped it): an older request read is not put over it (review R2).
        edit(5.0);
        let older = lib.request_plan(&spec).run();
        edit(5.5);
        lib.collect_garbage(&BTreeSet::new());
        assert!(lib.resolve(&spec).is_ok());
        assert_eq!(text(&lib), Some(block(5.5)));
        assert!(lib.request_read(&spec, older).is_ok());
        assert_eq!(text(&lib), Some(block(5.5)), "an older request read was put over a spawn's read");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R3 of m5-liblock: a finished build is kept for a request on its way to the
    /// engine thread, for every such request of the job, and no longer.
    #[test]
    fn a_finished_build_is_kept_only_for_requests_on_their_way() {
        let root = scratch("claims", &[("block", &block(1.0))]);
        let shared = SharedLibrary::new(Library::new(&root, None));
        let spec = ModelSpec::new("recipes/block.json");
        // A request nobody will claim (an agent's connection asks, then waits another way):
        // its build's result is not kept.
        let BuildRequest::Pending(_) = shared.request(&spec).unwrap() else { panic!("not built yet") };
        settle(&mut shared.lock());
        assert_eq!(shared.lock().finished_job_count(), (0, 0));
        // Two requests on their way for one job (a recipe changed, both asked before it was built).
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("recipes").join("block.json"), block(2.0)).unwrap();
        let BuildRequest::Pending(job) = shared.request_to_claim(&spec).unwrap() else { panic!("the new text is not built yet") };
        let BuildRequest::Pending(again) = shared.request_to_claim(&spec).unwrap() else { panic!("still building") };
        assert_eq!(job, again, "one job for both");
        // Still running: nothing to claim yet... (asked a third time, on its way too).
        shared.lock().expect_claim(job);
        assert!(shared.lock().claim(job).is_none(), "still running: wait for it as usual");
        // ...then it finishes and is taken before the two requests land.
        settle(&mut shared.lock());
        assert_eq!(shared.lock().finished_job_count(), (1, 1));
        let first = shared.lock().claim(job).expect("kept for the first request").expect("built");
        assert_eq!(shared.lock().finished_job_count(), (1, 1), "the second request has not landed yet");
        let second = shared.lock().claim(job).expect("and for the second").expect("built");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(shared.lock().finished_job_count(), (0, 0), "released when the last one landed");
        assert!(shared.lock().claim(job).is_none(), "nobody else was expected");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R1 of m5-liblock (there before the lane): after an outside edit, a request that
    /// found the change made the recipe known again at once, the next poll saw an unchanged
    /// folder, and the entities using the recipe kept their old build for good. Their handles
    /// are left for the engine's next re-key, whichever way the request read the disk.
    #[test]
    fn a_request_that_finds_a_change_leaves_the_handles_for_a_rekey() {
        for way in ["shared", "locked"] {
            let root = scratch(&format!("rekey_{way}"), &[("block", &block(1.0))]);
            let shared = SharedLibrary::new(Library::new(&root, None));
            let spec = ModelSpec::new("recipes/block.json");
            let entity = shared.lock().resolve(&spec).unwrap();
            settle(&mut shared.lock());
            let before = shared.lock().built(entity.handle).unwrap().id;
            assert!(shared.lock().rekey_more().is_none(), "nothing to re-key yet");
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(root.join("recipes").join("block.json"), block(2.0)).unwrap();
            let requested = if way == "shared" { shared.request(&spec) } else { shared.lock().request(&spec) };
            assert!(requested.is_ok(), "{way}: {:?}", requested.err().map(|e| e.code));
            // A poll now finds nothing changed (the request already read the folder)...
            let poll = shared.lock().scan_plan().run();
            let _ = shared.lock().apply_scan(poll, false);
            // ...and the engine's next pass re-keys the entity's handle all the same.
            let reload = shared.lock().rekey_more().unwrap_or_else(|| panic!("{way}: the handle was left for a re-key"));
            assert!(reload.handles.contains(&entity.handle), "{way}: {:?}", reload.handles);
            settle(&mut shared.lock());
            assert_ne!(shared.lock().built(entity.handle).unwrap().id, before, "{way}: the entity kept its old build");
            assert!(shared.lock().rekey_more().is_none(), "{way}: once");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// The engine's side (review R1 of m5-liblock: the blocker, and the test it asked for).
    /// `model_build` through the engine's own handler builds what is on the disk now, after an
    /// outside edit of a folder that is cached; its source is read on a worker, never on the
    /// thread that handles the call; a source that is not there is an error; and a call whose
    /// source is still being read is answered by a shutdown, not dropped.
    #[test]
    fn model_build_reads_its_source_on_a_worker_and_builds_what_is_on_disk() {
        let root = scratch("engine_build", &[("block", &block(1.0))]);
        let Some(mut e) = crate::workers::charge_tests::engine_at(&root) else { return };
        let scanned_on: Arc<Mutex<Vec<String>>> = Arc::default();
        let names = Arc::clone(&scanned_on);
        let _watch = watch_scans(&root, move || names.lock().unwrap().push(std::thread::current().name().unwrap_or("unnamed").to_owned()));
        let send = |e: &mut crate::Engine, recipe: &str| {
            let (reply, rx) = crate::jobs::channel();
            let command = crate::tools::Command::ModelBuild(ModelSpec::new(recipe));
            let request = crate::tools::Request { session: crate::editor_engine::window_session(), call: 0, command, reply, deadline: Instant::now() + Duration::from_secs(120), trace: specula_core::trace::Ctx::default(), via: None };
            e.handle_model_call(request);
            rx
        };
        let ask = |e: &mut crate::Engine, recipe: &str| -> Json {
            let rx = send(e, recipe);
            let give_up = Instant::now() + Duration::from_secs(120);
            loop {
                if let Ok(out) = rx.recv_timeout(Duration::from_millis(2)) {
                    let specula_protocol::Content::Text(t) = &out.content[0] else { panic!("text") };
                    return Json::parse(t).unwrap();
                }
                assert!(Instant::now() < give_up, "model_build never answered");
                e.pump_workers(None);
                e.pump_builds(Some(Duration::from_millis(20)));
            }
        };
        let height = |j: &Json| j.get("bounds").and_then(|b| b.get("max")).and_then(Json::as_array).and_then(|m| m[1].as_f64()).unwrap_or_else(|| panic!("{j}"));
        let first = ask(&mut e, "recipes/block.json");
        assert!((height(&first) - 1.9).abs() < 1e-3, "{first}");
        // An entity uses it (its handle is what an outside edit must reach).
        let entity = e.library.lock().resolve(&ModelSpec::new("recipes/block.json")).unwrap();
        let before = e.library.lock().built(entity.handle).expect("built").id;
        // Someone edits the recipe on disk; the folder is cached.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("recipes").join("block.json"), block(2.0)).unwrap();
        let second = ask(&mut e, "recipes/block.json");
        assert!((height(&second) - 2.9).abs() < 1e-3, "built from the disk as it is now: {second}");
        assert_eq!(second.get("cached").and_then(Json::as_bool), Some(false));
        // The entity's handle follows at the engine's next passes.
        let give_up = Instant::now() + Duration::from_secs(60);
        while e.library.lock().built(entity.handle).is_none_or(|b| b.id == before) {
            assert!(Instant::now() < give_up, "the entity kept its old build");
            e.poll_assets_async(false);
            e.pump_workers(None);
            e.pump_builds(Some(Duration::from_millis(20)));
        }
        // A recipe that is not there, and a folder that is not there.
        assert_eq!(ask(&mut e, "recipes/nope.json").get("error").and_then(Json::as_str), Some("recipe_not_found"));
        assert_eq!(ask(&mut e, "nowhere/block.json").get("error").and_then(Json::as_str), Some("recipe_not_found"));
        // Every read of the folder ran on a worker.
        let threads = scanned_on.lock().unwrap().clone();
        assert!(threads.len() >= 4 && threads.iter().all(|t| t.starts_with("specula-worker")), "the folder was read on: {threads:?}");
        // Review R2: a build that finishes, and is taken by a pass of the engine, before its
        // request lands from the worker, and a collection in between (every project write
        // forces one): the call is answered with the build, not with an error and not never.
        std::fs::write(root.join("recipes").join("tall.json"), block(3.0)).unwrap();
        let rx = send(&mut e, "recipes/tall.json");
        let give_up = Instant::now() + Duration::from_secs(120);
        // (The worker has made the request: a job is in flight. Then the build finishes and
        // this thread takes it, while the request's return still waits to be taken.)
        while e.library.lock().in_flight() == 0 && e.library.lock().finished_job_count().0 == 0 {
            assert!(Instant::now() < give_up, "the worker never made the request");
            std::thread::yield_now();
        }
        while e.library.lock().in_flight() > 0 {
            assert!(Instant::now() < give_up, "the build never finished");
            e.pump_builds(Some(Duration::from_millis(20)));
        }
        e.library.lock().collect_garbage(&BTreeSet::new());
        let out = loop {
            if let Ok(out) = rx.recv_timeout(Duration::from_millis(2)) {
                break out;
            }
            assert!(Instant::now() < give_up, "model_build never answered");
            e.pump_workers(None);
            e.pump_builds(Some(Duration::from_millis(20)));
        };
        let specula_protocol::Content::Text(t) = &out.content[0] else { panic!("text") };
        let tall = Json::parse(t).unwrap();
        assert!(!out.is_error && (height(&tall) - 3.9).abs() < 1e-3, "{tall}");
        // The result was kept for that one request and is released now it has landed: a
        // build no entity uses is not held on (review R3: the last 64 were, for good).
        assert_eq!(e.library.lock().finished_job_count(), (0, 0));
        // A call whose source is still being read is answered when the engine shuts down.
        let waiting = send(&mut e, "recipes/block.json");
        e.answer_waiters_shutting_down();
        let out = waiting.recv_timeout(Duration::from_secs(60)).expect("answered, not dropped");
        assert!(out.is_error, "{out:?}");
        drop(e);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// M4 exit run: four builders' project_writes left new recipes `recipe_not_found` for the
    /// studio, because the cached folder's time hadn't moved when the files were renamed into
    /// it. A recipe on disk that the cached folder lacks is read afresh, whatever the times say.
    #[test]
    fn a_new_recipe_is_found_when_its_folder_time_did_not_move() {
        // (This test's files are old to it: a folder with a file just written is read again
        // whatever its time says, and would find the new recipe before the path under test.)
        NOW.set(Some(SystemTime::now() + Duration::from_secs(60)));
        let root = scratch("new_recipe", &[("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        assert!(lib.request(&ModelSpec::new("recipes/block.json")).is_ok());
        std::fs::write(root.join("recipes").join("late.json"), block(2.0)).unwrap();
        // What a missed directory-time update looks like: the cache thinks it is current.
        let folder = PathBuf::from("recipes");
        let now = mtime(&root.join("recipes"));
        lib.folders.get_mut(&folder).unwrap().mtime = now;
        assert!(lib.request(&ModelSpec::new("recipes/late.json")).is_ok(), "the new recipe is found");
        assert!(lib.resolve(&ModelSpec::new("recipes/late.json")).is_ok());
        assert_eq!(lib.request(&ModelSpec::new("recipes/gone.json")).err().map(|e| e.code), Some("recipe_not_found"));
        NOW.set(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The poll split for the engine thread: the plan touches no disk, the scan runs without
    /// the library, and applying it rebuilds what changed. A project write's text reaches the
    /// cache at once (`note_written`), and a scan planned before that write still applies but
    /// never puts the older text (or a deleted recipe) back.
    #[test]
    fn scans_run_apart_and_noted_writes_win_over_older_scans() {
        // This test's files are old to it, as a project's are a moment after it is opened: their
        // times are what a scan compares (`FRESH`; files just written are the test above's).
        NOW.set(Some(SystemTime::now() + Duration::from_secs(60)));
        let root = scratch("scan_split", &[("block", &block(1.0)), ("other", &block(1.25))]);
        let mut lib = Library::new(&root, None);
        let spec = ModelSpec::new("recipes/block.json").with_param("tier", 2.0);
        let r = lib.resolve(&spec).unwrap();
        let other = lib.resolve(&ModelSpec::new("recipes/other.json")).unwrap();
        settle(&mut lib);
        let other_build = lib.built(other.handle).unwrap().id;
        let first = lib.built(r.handle).unwrap().id;
        // Nothing changed: a scan finds nothing and applying it reports nothing.
        let quiet = lib.scan_plan().run();
        assert!(quiet.folders.is_empty() && quiet.styles.is_empty() && quiet.missing.is_empty());
        assert!(lib.apply_scan(quiet, false).is_none());
        // Someone edits the disk (text B) and a scan reads it; before it is applied, the store
        // writes a newer text (C), which is not on the disk yet.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("recipes").join("block.json"), block(1.5)).unwrap();
        let stale = lib.scan_plan().run();
        assert_eq!(stale.folders.len(), 1, "the scan saw the edit");
        let text: Arc<str> = Arc::from(block(2.0).as_str());
        assert!(lib.note_written(&[("recipes/block.json".to_owned(), Some(Arc::clone(&text)))]));
        assert!(!lib.note_written(&[("recipes/block.json".to_owned(), Some(Arc::clone(&text)))]), "the same text again changes nothing");
        assert!(lib.recipes.contains_key("recipes/other.json"), "a recipe that doesn't include the written one is left be");
        let reload = lib.rekey(true).unwrap();
        assert_eq!(reload.rebuilding.len() + reload.rebuilt.len(), 1);
        settle(&mut lib);
        let second = lib.built(r.handle).unwrap().id;
        assert_ne!(first, second, "rebuilt from the new text");
        assert_eq!(lib.built(other.handle).unwrap().id, other_build);
        // The older scan never puts text B back over C.
        // (It still applies: whatever else it found lands; the noted file keeps its text.)
        let _ = lib.apply_scan(stale, true);
        settle(&mut lib);
        assert_eq!(lib.built(r.handle).unwrap().id, second, "the noted text stands");
        assert_eq!(lib.folders[Path::new("recipes")].files["block"].text.as_deref().ok(), Some(&*text));
        // The store's sync reaches the disk; the next scan reads the file's time, and afterwards
        // scans are quiet again.
        std::fs::write(root.join("recipes").join("block.json"), block(2.0)).unwrap();
        let s = lib.scan_plan().run();
        let _ = lib.apply_scan(s, false);
        let quiet = lib.scan_plan().run();
        assert!(quiet.folders.is_empty(), "times known again");
        // An older scan never lands after a newer one was applied either.
        let older = lib.scan_plan();
        let newer = lib.scan_plan().run();
        let _ = lib.apply_scan(newer, false);
        assert!(lib.apply_scan(older.run(), true).is_none());
        // Write, then delete before any scan confirmed the write: the recipe is gone, and stays
        // gone whatever scans planned before the delete find.
        let before_delete = lib.scan_plan();
        std::fs::write(root.join("recipes").join("late.json"), block(3.0)).unwrap();
        assert!(lib.note_written(&[("recipes/late.json".to_owned(), Some(Arc::from(block(3.0).as_str())))]));
        assert!(lib.resolve(&ModelSpec::new("recipes/late.json")).is_ok());
        std::fs::remove_file(root.join("recipes").join("late.json")).unwrap();
        assert!(lib.note_written(&[("recipes/late.json".to_owned(), None)]));
        let _ = lib.apply_scan(before_delete.run(), true);
        assert!(!lib.folders[Path::new("recipes")].files.contains_key("late"), "a scan planned before the delete doesn't bring it back");
        let s = lib.scan_plan().run();
        let _ = lib.apply_scan(s, true);
        assert!(!lib.folders[Path::new("recipes")].files.contains_key("late"), "the deleted recipe stays gone");
        assert_eq!(lib.request(&ModelSpec::new("recipes/late.json")).err().map(|e| e.code), Some("recipe_not_found"));
        NOW.set(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R1 (major 1): an entity left unresolved (its recipe missing when it was loaded)
    /// has no handle, so no re-key names it; when its recipe is written (or fixed), the reload
    /// resolves it again, as `refresh_models` did on every reload before. Also the written-file
    /// path through a scan (a recipe edited on disk), and a broken recipe fixed in place.
    #[test]
    fn unresolved_entities_resolve_when_their_recipe_is_written() {
        let root = scratch("unresolved_written", &[("block", &block(1.0)), ("broken", "{\"recipe\":")]);
        let shared = Arc::new(SharedLibrary::new(Library::new(&root, None)));
        let mut world = World::default();
        // The entities exist before the library (as after world_load or recover), naming a recipe
        // that isn't there yet, and one that doesn't parse.
        let late = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/late.json")), "t").unwrap().id;
        let late2 = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/late.json")), "t").unwrap().id;
        let broken = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/broken.json")), "t").unwrap().id;
        let fine = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/block.json")), "t").unwrap().id;
        let failed = world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        assert_eq!(failed.len(), 3, "late twice and broken: {failed:?}");
        settle_shared(&shared, &mut world);
        for id in [late, late2, broken] {
            assert_eq!(world.get(id).unwrap().model_state(), "unresolved");
        }
        assert_ne!(world.get(fine).unwrap().model_state(), "unresolved");
        // A build that finishes (no files changed) leaves the unresolved ones alone (no resolve
        // per finished build).
        world.refresh_model_handles(&BTreeSet::new(), false);
        assert_eq!(world.get(late).unwrap().model_state(), "unresolved");
        // The store writes the recipe (project_write): noted, re-keyed, the reload applied.
        let text: Arc<str> = Arc::from(block(2.0).as_str());
        std::fs::write(root.join("recipes").join("late.json"), &*text).unwrap();
        let reload = {
            let mut lib = shared.lock();
            assert!(lib.note_written(&[("recipes/late.json".to_owned(), Some(text))]));
            lib.rekey(true).unwrap()
        };
        assert!(reload.files_changed > 0);
        world.refresh_model_handles(&reload.handles, reload.files_changed > 0);
        settle_shared(&shared, &mut world);
        for id in [late, late2] {
            assert_eq!(world.get(id).unwrap().model_state(), "ready", "the written recipe resolves its entities");
            assert!(world.get(id).unwrap().local_bounds().is_some());
        }
        assert_eq!(world.get(late).unwrap().resolved_model().map(|r| r.handle), world.get(late2).unwrap().resolved_model().map(|r| r.handle));
        assert_eq!(world.get(broken).unwrap().model_state(), "unresolved", "still broken");
        // The broken recipe is fixed on disk (outside the store): a scan finds it.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.join("recipes").join("broken.json"), block(1.5)).unwrap();
        let reload = {
            let mut lib = shared.lock();
            let s = lib.scan_plan().run();
            lib.apply_scan(s, true).unwrap()
        };
        world.refresh_model_handles(&reload.handles, reload.files_changed > 0);
        settle_shared(&shared, &mut world);
        assert_eq!(world.get(broken).unwrap().model_state(), "ready", "the fixed recipe resolves its entity");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R1 (minor 4): a re-key on the engine thread runs in slices. With a zero slice each
    /// call works out one forgotten recipe and leaves the rest for `rekey_more`; meanwhile the
    /// left handles keep their builds; a left recipe a resolve made known meanwhile is still
    /// re-keyed; in the end every handle has its new build. The synchronous poll is never
    /// sliced (model_reload answers with the whole reload).
    #[test]
    fn rekeys_run_in_slices_and_finish() {
        let root = scratch("rekey_slices", &[("a", &block(1.0)), ("b", &block(1.25)), ("c", &block(1.5))]);
        let mut lib = Library::new(&root, None);
        let specs = ["a", "b", "c"].map(|n| ModelSpec::new(format!("recipes/{n}.json")));
        let handles = specs.clone().map(|s| lib.resolve(&s).unwrap().handle);
        settle(&mut lib);
        let before = handles.map(|h| lib.built(h).unwrap().id);
        lib.set_rekey_slice(Some(Duration::ZERO));
        // A style write forgets all three recipes.
        let style: Arc<str> = Arc::from(r##"{"style":"t","materials":{"stone":{"color":"#a0a0a0","roughness":0.6},
                "ghost":{"kind":"glass","color":"#9cff4f","opacity":0.35,"emissive":0.5}}}"##);
        assert!(lib.note_written(&[(STYLE_FILE.to_owned(), Some(style))]));
        let first = lib.rekey(true).unwrap();
        assert_eq!(first.rebuilding.len() + first.rebuilt.len(), 1, "one recipe a zero slice");
        assert_eq!(lib.rekey_pending(), 2);
        settle(&mut lib);
        let built = |lib: &Library| handles.map(|h| lib.built(h).unwrap().id);
        let changed = built(&lib).iter().zip(&before).filter(|(a, b)| a != b).count();
        assert_eq!(changed, 1, "the left handles keep their builds meanwhile");
        // A resolve makes the left "c" known again: it is still re-keyed when its turn comes.
        assert!(lib.resolve(&specs[2]).is_ok());
        let second = lib.rekey_more().unwrap();
        assert_eq!(second.handles.len(), 1);
        assert_eq!(lib.rekey_pending(), 1);
        let third = lib.rekey_more().unwrap();
        assert_eq!(third.handles.len(), 1);
        assert_eq!(lib.rekey_pending(), 0);
        assert!(lib.rekey_more().is_none(), "nothing left");
        settle(&mut lib);
        let after = built(&lib);
        assert!(after.iter().zip(&before).all(|(a, b)| a != b), "every handle rebuilt with the new style: {before:?} -> {after:?}");
        // The synchronous poll works everything out at once, whatever the slice.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            root.join(STYLE_FILE),
            r##"{"style":"t","materials":{"stone":{"color":"#606060","roughness":0.5},
                "ghost":{"kind":"glass","color":"#9cff4f","opacity":0.35,"emissive":0.5}}}"##,
        )
        .unwrap();
        let polled = lib.poll(true).unwrap();
        assert_eq!(polled.handles.len(), 3, "unsliced: {:?}", polled.handles);
        assert_eq!(lib.rekey_pending(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recipe_failures_are_structured() {
        let bad_op = r#"{"recipe":1,"name":"bad","parts":[{"name":"p","material":"stone","shape":{"op":"box","size":[1,1,1]},
                         "ops":[{"op":"explode"}]}]}"#;
        let over = r#"{"recipe":1,"name":"over","budget":{"max_triangles":4},"parts":[{"name":"p","material":"stone","shape":{"op":"box","size":[1,1,1]}}]}"#;
        let root = scratch("errors", &[("bad_op", bad_op), ("bad_json", "{\"recipe\":"), ("block", &block(1.0)), ("over", over)]);
        let mut lib = Library::new(&root, None);
        let err = |lib: &mut Library, spec: ModelSpec| lib.resolve(&spec).err().unwrap();
        let code = |e: &WorldError| e.detail("recipe_error").and_then(|j| j.get("code")).cloned();
        // Checked at spawn, without building: structure, parameters, files, paths, style.
        let e = err(&mut lib, ModelSpec::new("recipes/bad_op.json"));
        assert_eq!((e.code, code(&e)), ("recipe_invalid", Some(Json::from("UnknownOp"))));
        assert_eq!(e.detail("recipe_error").unwrap().get("path"), Some(&Json::from("parts[0].ops[0].op")));
        assert_eq!(code(&err(&mut lib, ModelSpec::new("recipes/bad_json.json"))), Some(Json::from("Json")));
        assert_eq!(err(&mut lib, ModelSpec::new("recipes/nope.json")).code, "recipe_not_found");
        assert_eq!(err(&mut lib, ModelSpec::new("nowhere/x.json")).code, "recipe_not_found");
        let e = err(&mut lib, ModelSpec::new("recipes/block.json").with_param("tier", 9.0));
        assert_eq!((code(&e), e.detail("recipe_error").unwrap().get("path")), (Some(Json::from("OutOfRange")), Some(&Json::from("params.tier"))));
        assert_eq!(code(&err(&mut lib, ModelSpec::new("recipes/block.json").with_param("tier", 1.5))), Some(Json::from("OutOfRange")));
        assert_eq!(code(&err(&mut lib, ModelSpec::new("recipes/block.json").with_param("height", 1.0))), Some(Json::from("UnknownName")));
        for bad in ["../escape.json", "recipes./block.json", "recipes/block.json."] {
            assert_eq!(err(&mut lib, ModelSpec::new(bad)).code, "invalid_value", "{bad}");
        }
        // Found only by building: the entity spawns, then its model fails.
        let r = lib.resolve(&ModelSpec::new("recipes/over.json")).unwrap();
        assert_eq!(r.state, ModelState::Building);
        settle(&mut lib);
        let r = lib.resolve(&ModelSpec::new("recipes/over.json")).unwrap();
        assert_eq!((r.state, r.bounds), (ModelState::Failed, None));
        assert_eq!(code(lib.handle_error(r.handle).unwrap()), Some(Json::from("OverBudget")));
        // No style anywhere: a clear error, not a panic.
        std::fs::remove_file(root.join(STYLE_FILE)).unwrap();
        let mut lib = Library::new(&root, None);
        assert_eq!(err(&mut lib, ModelSpec::new("recipes/block.json")).code, "style_not_found");
        // No assets root at all (yet): refused, and nothing cached.
        let mut lib = Library::new(&root.join("not_yet"), None);
        assert_eq!(err(&mut lib, ModelSpec::new("recipes/block.json")).code, "recipe_not_found");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn heavy_builds_never_block_resolving() {
        let root = scratch("heavy", &[("heavy", &heavy()), ("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let started = Instant::now();
        let h = lib.resolve(&ModelSpec::new("recipes/heavy.json")).unwrap();
        let b = lib.resolve(&ModelSpec::new("recipes/block.json")).unwrap();
        // Both still building when resolve returns is the proof it didn't wait; the time is printed,
        // not asserted (CI runners are slow and shared).
        eprintln!("resolving took {:?}", started.elapsed());
        assert_eq!((h.state, b.state), (ModelState::Building, ModelState::Building));
        // The light build lands while the heavy one may still run.
        settle(&mut lib);
        assert!(lib.built(h.handle).is_some() && lib.built(b.handle).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edits_rebuild_only_what_they_change() {
        let root = scratch("reload", &[("block", &block(1.0)), ("host", HOST), ("other", &block(1.0))]);
        let file = |name: &str| root.join(format!("recipes/{name}.json"));
        let shared = Arc::new(SharedLibrary::new(Library::new(&root, None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let id = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/block.json")), "t").unwrap().id;
        let host = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/host.json")), "t").unwrap().id;
        // The same recipe under another seed: a second handle of it.
        let seeded = world.spawn(SpawnSpec::model(ModelSpec { seed: Some(9), ..ModelSpec::new("recipes/block.json") }), "t").unwrap().id;
        assert_eq!(world.get(id).unwrap().model_state(), "building");
        assert!(world.get(id).unwrap().local_bounds().is_none(), "no bounds before the build lands");
        settle_shared(&shared, &mut world);
        assert_eq!(world.get(id).unwrap().model_state(), "ready");
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 1.9);
        assert!(shared.lock().poll(false).is_none(), "polled at most once a second");
        // (Modification times are set explicitly: some file systems keep whole seconds only.)
        let touch = |name: &str, text: &str, secs: u64| {
            std::fs::write(file(name), text).unwrap();
            let f = std::fs::File::options().write(true).open(file(name)).unwrap();
            f.set_modified(SystemTime::now() + Duration::from_secs(secs)).unwrap();
        };
        // An unrelated recipe in the same folder changes: nothing is rebuilt.
        touch("other", &block(5.0), 5);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!((report.files_changed, report.rebuilding.len(), report.rebuilt.len()), (1, 0, 0), "unrelated edits rebuild nothing");
        // The included file changes: both its own specs and the host's rebuild.
        touch("block", &block(2.0), 10);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!((report.rebuilding.len(), report.errors.len()), (3, 0), "every handle of a changed recipe: {:?}", report.rebuilding);
        settle_shared(&shared, &mut world);
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 2.9);
        assert_eq!(world.get(seeded).unwrap().local_bounds().unwrap().1.y, 2.9, "not just the first handle of the recipe");
        assert_eq!(world.get(host).unwrap().local_bounds().unwrap().1.y, 2.9, "the host follows its include");
        // Break it: the error is reported and the last good build stays.
        touch("block", "{\"recipe\": 1, \"parts\": 7}", 15);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!(report.errors.len(), 2, "the host fails at build time, both block handles at parse time");
        settle_shared(&shared, &mut world);
        assert!(world.refresh_models().is_empty(), "existing specs keep their handle");
        let handle = world.get(id).unwrap().resolved_model().unwrap().handle;
        assert!(shared.lock().handle_error(handle).is_some());
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 2.9);
        let up = UpdateSpec { params: Some(vec![("tier".into(), 2.0)]), ..Default::default() };
        assert_eq!(world.update(&EntityKey::Id(id), up, "t").err().unwrap().code, "recipe_invalid", "a new spec is refused");
        // Put back exactly as it was: the build stands and the error clears.
        touch("block", &block(2.0), 20);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!((report.rebuilding.len(), report.errors.len()), (0, 0), "{:?}", report.rebuilding);
        settle_shared(&shared, &mut world);
        assert!(shared.lock().handle_error(handle).is_none(), "the stale error is gone");
        // A recipe written just now (its folder cached) is found without waiting for the poll.
        std::fs::write(file("fresh"), block(0.5)).unwrap();
        world.spawn(SpawnSpec::model(ModelSpec::new("recipes/fresh.json")), "t").unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn what_no_entity_uses_is_dropped() {
        let root = scratch("gc", &[("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        // Agents can't grow the folder cache with folders that don't exist.
        for i in 0..200 {
            assert!(lib.resolve(&ModelSpec::new(format!("bogus_{i}/x.json"))).is_err());
        }
        assert_eq!(lib.counts().3, 0, "missing folders are never cached");
        // A game spawning a fresh seed per effect, then dropping them all.
        let keep = lib.resolve(&ModelSpec::new("recipes/block.json")).unwrap().handle;
        for s in 0..(SPARE_HANDLES as u64 + 300) {
            lib.resolve(&ModelSpec::new("recipes/block.json").with_seed(s + 1)).unwrap();
        }
        lib.collect_garbage(&BTreeSet::from([keep]));
        settle(&mut lib);
        lib.collect_garbage(&BTreeSet::from([keep]));
        let (specs, builds, in_flight, folders) = lib.counts();
        assert_eq!((specs, builds, in_flight, folders), (1, 1, 0, 1), "only what the live entity uses stays");
        assert!(lib.built(keep).is_some());
        // Nothing live: folders and recipes go too (handles stay while under the spare count).
        lib.collect_garbage(&BTreeSet::new());
        assert_eq!(lib.counts().0, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review round 2: deleted entities' queued builds held up live ones, and a cancelled job's
    /// result marked a respawned entity `failed` for good.
    #[test]
    fn cancelled_builds_never_hold_up_or_fail_live_entities() {
        let root = scratch("cancel", &[("heavy", &heavy()), ("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let heavies: Vec<ModelSpec> = (0..6).map(|s| ModelSpec::new("recipes/heavy.json").with_seed(s)).collect();
        let handles: Vec<u32> = heavies.iter().map(|s| lib.resolve(s).unwrap().handle).collect();
        assert_eq!(lib.in_flight(), 6);
        // Their entities are deleted: every queued build is dropped at once (no spare threshold).
        lib.collect_garbage(&BTreeSet::new());
        assert_eq!(lib.in_flight(), 0, "nobody waits for them");
        // A live entity's light build is not stuck behind them (at most the two already running).
        let started = Instant::now();
        let light = lib.resolve(&ModelSpec::new("recipes/block.json")).unwrap().handle;
        while lib.built(light).is_none() && started.elapsed() < Duration::from_secs(20) {
            lib.finish(Some(Duration::from_millis(20)));
        }
        assert!(lib.built(light).is_some(), "the live build landed");
        // The last heavy spec is wanted again while its cancelled job is still queued or running:
        // whatever that old job reports, the entity ends up built, not failed.
        let again = lib.resolve(&heavies[5]).unwrap();
        assert_eq!((again.handle, again.state), (handles[5], ModelState::Building));
        settle(&mut lib);
        let r = lib.resolve(&heavies[5]).unwrap();
        assert_eq!(r.state, ModelState::Ready, "{:?}", lib.handle_error(r.handle));
        // A cancelled job may still be running on a worker: its id is pruned when it ends.
        let waited = Instant::now();
        while !lib.workers.cancelled.lock().unwrap().is_empty() && waited.elapsed() < Duration::from_secs(20) {
            lib.finish(Some(Duration::from_millis(20)));
        }
        assert!(lib.workers.cancelled.lock().unwrap().is_empty(), "cancelled ids are pruned as their jobs end");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn job(id: u64) -> Job {
        Job {
            id,
            key: (0, String::new(), None),
            recipe: "recipes/x.json".into(),
            text: Arc::from("{}"),
            library: Arc::new(BTreeMap::new()),
            style: Arc::new(Style::default()),
            style_path: PathBuf::new(),
            params: Vec::new(),
            seed: None,
            gltf: None,
            anims: None,
            cancel: Arc::new(AtomicBool::new(false)),
            trace: specula_core::trace::Ctx::default(),
        }
    }

    /// Procgen review round 2: one heavy worker halved procedural throughput while light ones sat
    /// idle. Idle light workers but the first take procedural jobs; the first never does.
    #[test]
    fn idle_light_workers_take_procedural_jobs_but_one_never_does() {
        let mut q = Queues::default();
        q.heavy.extend([job(1), job(2)]);
        assert!(q.take(Takes::Light).is_none(), "the first light worker never takes a procedural job");
        assert_eq!(q.take(Takes::LightFirst).map(|j| j.id), Some(1));
        assert_eq!(q.take(Takes::HeavyFirst).map(|j| j.id), Some(2));
        q.light.push_back(job(3));
        q.heavy.push_back(job(4));
        q.light.push_back(job(5));
        assert_eq!(q.take(Takes::HeavyFirst).map(|j| j.id), Some(4));
        assert_eq!(q.take(Takes::LightFirst).map(|j| j.id), Some(3));
        assert_eq!(q.take(Takes::Light).map(|j| j.id), Some(5));
        // A build that finished as it was dropped is kept; a failure from a dropped job is a
        // cancellation.
        let failed = || Outcome::Failed(RecipeError::new("", ErrorCode::Limit, "the build was cancelled"));
        assert!(matches!(settled(failed(), true), Outcome::Cancelled));
        assert!(matches!(settled(failed(), false), Outcome::Failed(_)));
        let model = specula_model::build(&block(1.0), &BuildParams::new(&Style::parse(r##"{"materials":{"stone":{"color":"#808080"},"ghost":{"color":"#9cff4f"}}}"##).unwrap())).unwrap();
        assert!(matches!(settled(Outcome::Built(Box::new(model), Box::default()), true), Outcome::Built(..)));
    }

    /// Procgen review: two heavy sculpts held both workers while another agent's 1 ms build
    /// waited 230 s. Procedural builds now have their own worker, and dropping a running one
    /// stops it (its outcome is a cancellation, not the budget failure it would have reached).
    #[test]
    fn procedural_builds_queue_apart_and_stop_when_dropped() {
        let spheres: Vec<String> = (0..60)
            .map(|i| format!(r#"{{"op":"sphere","radius":0.6,"at":[{},{},{}],"displace":{{"amount":2,"scale":0.05,"octaves":6}}}}"#, (i * 37 % 100) as f32 * 0.04 - 2.0, (i * 53 % 100) as f32 * 0.04 - 2.0, (i * 71 % 100) as f32 * 0.04 - 2.0))
            .collect();
        let sculpt = format!(
            r#"{{"recipe":1,"name":"sculpt","lods":[],"budget":{{"max_triangles":600000}},"parts":[{{"name":"s","material":"stone","shape":{{"op":"sdf","resolution":160,"bounds":[[-3,-3,-2.8],[3,3,2.8]],"tree":{{"op":"union","of":[{}]}}}}}}]}}"#,
            spheres.join(",")
        );
        let root = scratch("heavyq", &[("sculpt", &sculpt), ("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let heavy = ModelSpec::new("recipes/sculpt.json");
        let hs: Vec<u32> = (0..2).map(|s| lib.resolve(&heavy.clone().with_seed(s)).unwrap().handle).collect();
        // Both sculpts queue on the heavy worker; the light build doesn't wait for them.
        let light = lib.resolve(&ModelSpec::new("recipes/block.json")).unwrap().handle;
        assert!(lib.recipes["recipes/sculpt.json"].heavy && !lib.recipes["recipes/block.json"].heavy);
        let started = Instant::now();
        while lib.built(light).is_none() && started.elapsed() < Duration::from_secs(20) {
            lib.finish(Some(Duration::from_millis(5)));
        }
        assert!(lib.built(light).is_some());
        assert_eq!(lib.in_flight(), 2, "the sculpts were still building when the light build landed");
        // Their entities go: the running sculpt stops, the queued one never starts.
        let jobs: BTreeSet<u64> = lib.in_flight.values().map(|f| f.job).collect();
        lib.collect_garbage(&[light].into_iter().collect());
        let mut outcomes = Vec::new();
        let waited = Instant::now();
        while outcomes.len() < 2 && waited.elapsed() < Duration::from_secs(30) {
            for f in lib.finish(Some(Duration::from_millis(20))).0 {
                if jobs.contains(&f.job) {
                    outcomes.push(f.result.err().map(|e| e.code.to_string()));
                }
            }
        }
        assert_eq!(outcomes, vec![Some("build_cancelled".to_string()); 2], "{hs:?}");
        assert!(lib.workers.flags.lock().unwrap().is_empty(), "flags are pruned as jobs end");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review round 3: deterministic (no workers; results delivered by hand). A job that was
    /// cancelled, then replaced when its key was wanted again, never fails or cancels the entity
    /// now waiting; a good result from it still serves it (and the replacement is dropped).
    #[test]
    fn stale_job_results_never_fail_a_rewanted_entity() {
        let root = scratch("stale", &[("block", &block(1.0))]);
        let mut lib = Library::with_workers(&root, None, 0);
        let spec = ModelSpec::new("recipes/block.json");
        let h = lib.resolve(&spec).unwrap().handle;
        let key = lib.handles[h as usize].as_ref().unwrap().want.clone().unwrap();
        let first = lib.in_flight[&key].job;
        lib.collect_garbage(&BTreeSet::new()); // its entity was deleted
        assert_eq!(lib.in_flight(), 0);
        assert_eq!(lib.resolve(&spec).unwrap().state, ModelState::Building, "wanted again");
        let second = lib.in_flight[&key].job;
        assert_ne!(first, second);
        let deliver = |lib: &mut Library, id: u64, outcome: Outcome| {
            let done = Done { id, key: key.clone(), recipe: spec.recipe.clone(), style_path: PathBuf::new(), outcome, build_ms: 0.0 };
            lib.workers.inject.send(done).unwrap();
            lib.finish(None)
        };
        let failure = || Outcome::Failed(RecipeError::new("parts", ErrorCode::Geometry, "old job"));
        let (finished, changed) = deliver(&mut lib, first, failure());
        assert_eq!((finished.len(), changed.len()), (1, 0), "the old job's failure changes nothing");
        deliver(&mut lib, first, Outcome::Cancelled);
        let r = lib.resolve(&spec).unwrap();
        assert_eq!((r.state, lib.handle_error(h).is_none()), (ModelState::Building, true));
        assert_eq!(lib.in_flight[&key].job, second, "the replacement still runs");
        // The real build lands.
        let style = Style::parse(&std::fs::read_to_string(root.join(STYLE_FILE)).unwrap()).unwrap();
        let model = || build(&block(1.0), &BuildParams::new(&style)).unwrap();
        deliver(&mut lib, second, Outcome::Built(Box::new(model()), Box::default()));
        assert_eq!(lib.resolve(&spec).unwrap().state, ModelState::Ready);
        // Again, but this time the old job's good result arrives first: it serves the entity and
        // the replacement is cancelled.
        lib.collect_garbage(&BTreeSet::new());
        lib.handles[h as usize].as_mut().unwrap().built = None; // as if never built
        lib.handles[h as usize].as_mut().unwrap().built_key = None;
        lib.builds.clear();
        lib.resolve(&spec).unwrap();
        let replacement = lib.in_flight[&key].job;
        deliver(&mut lib, first, Outcome::Built(Box::new(model()), Box::default()));
        assert_eq!(lib.resolve(&spec).unwrap().state, ModelState::Ready);
        assert_eq!(lib.in_flight(), 0, "the replacement is dropped");
        assert!(lib.workers.cancelled.lock().unwrap().contains(&replacement));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review round 3: a recipe edited while its entity was deleted came back with the old model,
    /// and model_reload couldn't fix it; a stale error stayed the same way.
    #[test]
    fn deleted_entities_come_back_with_the_recipe_as_it_is_now() {
        let root = scratch("comeback", &[("block", &block(1.0))]);
        let file = root.join("recipes/block.json");
        let touch = |text: &str, secs: u64| {
            std::fs::write(&file, text).unwrap();
            std::fs::File::options().write(true).open(&file).unwrap().set_modified(SystemTime::now() + Duration::from_secs(secs)).unwrap();
        };
        let shared = Arc::new(SharedLibrary::new(Library::new(&root, None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let spec = ModelSpec::new("recipes/block.json");
        let id = world.spawn(SpawnSpec::model(spec.clone()), "t").unwrap().id;
        settle_shared(&shared, &mut world);
        let saved = world.get(id).unwrap().clone();
        world.delete(&EntityKey::Id(id), None).unwrap();
        let gc = |world: &World| shared.lock().collect_garbage(&live_handles(world));
        gc(&world);
        // Edited (5 m) while nothing uses it; the reload's rebuild is dropped (nobody waits).
        touch(&block(5.0), 5);
        shared.lock().poll(true).unwrap();
        gc(&world);
        settle(&mut shared.lock());
        // Respawned: the new recipe, not the old build.
        let again = world.spawn(SpawnSpec::model(spec.clone()), "t").unwrap().id;
        settle_shared(&shared, &mut world);
        assert_eq!(world.get(again).unwrap().local_bounds().unwrap().1.y, 5.9, "built from the recipe as it is now");
        world.delete(&EntityKey::Id(again), None).unwrap();
        gc(&world);
        // Undone (an old state put back) after another edit: the same.
        touch(&block(3.0), 10);
        shared.lock().poll(true).unwrap();
        gc(&world);
        world.put_raw(saved.clone());
        settle_shared(&shared, &mut world);
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 3.9, "the undone entity too");
        world.delete(&EntityKey::Id(id), None).unwrap();
        gc(&world);
        // Broken then fixed back while unused: no stale error when it is used again.
        touch("{\"recipe\": 1, \"parts\": 7}", 15);
        shared.lock().poll(true).unwrap();
        touch(&block(3.0), 20);
        shared.lock().poll(true).unwrap();
        let back = world.spawn(SpawnSpec::model(spec.clone()), "t").unwrap().id;
        settle_shared(&shared, &mut world);
        let h = world.get(back).unwrap().resolved_model().unwrap().handle;
        assert!(shared.lock().handle_error(h).is_none(), "{:?}", shared.lock().handle_error(h));
        assert_eq!(world.get(back).unwrap().model_state(), "ready");
        // Broken while unused and not reloaded: used again, the error shows (the last good build
        // still draws).
        world.delete(&EntityKey::Id(back), None).unwrap();
        gc(&world);
        touch("{\"recipe\": 1, \"parts\": 7}", 25);
        shared.lock().poll(true).unwrap();
        gc(&world);
        let broken = world.spawn(SpawnSpec::model(spec), "t").unwrap().id;
        assert_eq!(world.get(broken).unwrap().model_state(), "ready");
        assert!(shared.lock().handle_error(h).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A panic in the model crate becomes its own structured outcome, not a dead worker.
    #[test]
    fn a_panicking_build_is_reported_not_fatal() {
        fn boom(_: &str, _: &BuildParams) -> Result<Model, RecipeError> {
            panic!("kaboom")
        }
        let job = Job {
            id: 1,
            key: (0, String::new(), None),
            recipe: "recipes/x.json".into(),
            text: Arc::from("{}"),
            library: Arc::new(BTreeMap::new()),
            style: Arc::new(Style::default()),
            style_path: PathBuf::new(),
            params: Vec::new(),
            seed: None,
            gltf: None,
            anims: None,
            cancel: Arc::new(AtomicBool::new(false)),
            trace: specula_core::trace::Ctx::default(),
        };
        match run(&job, boom) {
            Outcome::Panicked(why) => assert!(why.contains("kaboom"), "{why}"),
            _ => panic!("expected a panic outcome"),
        }
        fn fails(_: &str, _: &BuildParams) -> Result<Model, RecipeError> {
            Err(RecipeError::new("parts", ErrorCode::Geometry, "no"))
        }
        assert!(matches!(run(&job, fails), Outcome::Failed(e) if e.code == ErrorCode::Geometry));
    }

    #[test]
    fn a_folder_removed_and_restored_is_watched_again() {
        let root = scratch("folder", &[("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let h = lib.resolve(&ModelSpec::new("recipes/block.json")).unwrap().handle;
        settle(&mut lib);
        std::fs::rename(root.join("recipes"), root.join("away")).unwrap();
        lib.poll_due_now();
        let report = lib.poll(false).unwrap();
        assert_eq!(report.errors.len(), 1, "the recipe is gone");
        assert!(lib.built(h).is_some(), "the last good build stays");
        std::fs::rename(root.join("away"), root.join("recipes")).unwrap();
        lib.poll_due_now();
        lib.poll(false).expect("the restored folder is noticed by the automatic poll");
        assert!(lib.handle_error(h).is_none(), "and the error is gone");
        // Later edits are picked up again.
        let file = root.join("recipes/block.json");
        std::fs::write(&file, block(4.0)).unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(SystemTime::now() + Duration::from_secs(30)).unwrap();
        lib.poll_due_now();
        lib.poll(false).unwrap();
        settle(&mut lib);
        assert_eq!(lib.built(h).unwrap().bounds().1.y, 4.9);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reload_reports_are_bounded() {
        let mut report = Reload { files_changed: 3, ..Reload::default() };
        for i in 0..400 {
            let recipe = format!("recipes/r{}.json", i % 7);
            let e = WorldError::new("recipe_invalid", format!("{recipe}: broken")).with("recipe", recipe.as_str());
            report.errors.push((recipe, e.to_json()));
            report.rebuilt.push(format!("recipes/x.json|#{i}"));
        }
        let json = report.to_json();
        assert!(json.to_string().len() < 8 * 1024, "{} bytes", json.to_string().len());
        assert_eq!(json.get("errors").and_then(Json::as_array).unwrap().len(), 7, "identical errors grouped");
        assert_eq!(json.get("error_count"), Some(&Json::from(400)));
        assert_eq!(json.get("rebuilt_omitted"), Some(&Json::from(400 - MAX_LISTED)));
    }

    #[test]
    fn lod_thresholds_and_hysteresis() {
        assert_eq!(select_lod(500.0, 3, None), 0);
        assert_eq!(select_lod(150.0, 3, None), 1);
        assert_eq!(select_lod(30.0, 3, None), 2);
        assert_eq!(select_lod(10.0, 3, None), 2, "clamped to the last LOD");
        assert_eq!(select_lod(10.0, 4, None), 3);
        assert_eq!(select_lod(10.0, 1, None), 0);
        // Just under a threshold: a fresh choice switches, a remembered one holds until 15% past.
        assert_eq!(select_lod(185.0, 3, None), 1);
        assert_eq!(select_lod(185.0, 3, Some(0)), 0);
        assert_eq!(select_lod(192.0 * 0.84, 3, Some(0)), 1);
        assert_eq!(select_lod(200.0, 3, Some(1)), 1, "coming back needs 15% over the threshold");
        assert_eq!(select_lod(192.0 * 1.16, 3, Some(1)), 0);
        // Far jumps go all the way in one call.
        assert_eq!(select_lod(5.0, 3, Some(0)), 2);
        assert_eq!(select_lod(1000.0, 3, Some(2)), 0);
        // A level from a model with more LODs is clamped.
        assert_eq!(select_lod(5.0, 2, Some(3)), 1);
    }

    #[test]
    fn screen_size_shrinks_with_distance() {
        let near = screen_px(1.0, 10.0, 40.0, 1080);
        let far = screen_px(1.0, 20.0, 40.0, 1080);
        assert!((near / far - 2.0).abs() < 1e-4, "{near} {far}");
        assert!((near - 1.0 / (10.0 * 20f32.to_radians().tan()) * 1080.0).abs() < 1e-3);
        assert_eq!(screen_px(1.0, 0.5, 40.0, 1080), f32::INFINITY);
    }

    #[test]
    fn uploads_are_shared_per_build_and_lods_follow_size() {
        let Some(mut gpu) = gpu_or_skip() else { return };
        let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let stone = ModelSpec::new("recipes/warding_stone.json").with_param("tier", 2.0);
        let mut ids = Vec::new();
        for i in 0..3 {
            let mut s = SpawnSpec::model(stone.clone());
            s.position = Vec3::new(i as f32 * 4.0, 0.0, 0.0);
            ids.push(world.spawn(s, "t").unwrap().id);
        }
        world.spawn(SpawnSpec::model(stone.clone().with_param("tier", 3.0)), "t").unwrap();
        settle_shared(&shared, &mut world);
        let mut models = GpuModels::default();
        // Uploads are sliced over frames (UPLOAD_SLICE): sync until every build is up.
        for _ in 0..50 {
            assert!(models.sync(&mut gpu, &shared, &world, 0).is_empty());
            if models.stats_json().get("builds_uploaded") == Some(&Json::from(2)) {
                break;
            }
        }
        let stats = models.stats_json();
        assert_eq!(stats.get("builds_uploaded"), Some(&Json::from(2)), "three tier-2 stones share one upload");
        let built = Arc::clone(shared.lock().built(world.get(ids[0]).unwrap().resolved_model().unwrap().handle).unwrap());
        let parts: usize = built.model.lods.iter().map(|l| l.parts.len()).sum();
        assert!(stats.get("meshes").and_then(Json::as_u64).unwrap() as usize >= parts);
        // Near: LOD 0; far: the last LOD.
        let e = world.get(ids[0]).unwrap();
        let mut out = Vec::new();
        let mut st = LodStats::default();
        models.draw(e, Vec3::new(0.0, 1.0, 4.0), 50.0, 1080, &mut LodChoice::Fresh, &mut st, &mut out);
        assert_eq!(st.histogram[0], 1);
        assert_eq!(out.len(), built.model.lods[0].parts.len(), "one draw per material part");
        models.draw(e, Vec3::new(0.0, 1.0, 900.0), 50.0, 1080, &mut LodChoice::Fresh, &mut st, &mut out);
        assert_eq!(st.histogram[(built.model.lods.len() - 1).min(LOD_BUCKETS - 1)], 1);
        // Deleting every user of a build frees its meshes.
        for id in ids {
            world.delete(&EntityKey::Id(id), None).unwrap();
        }
        models.sync(&mut gpu, &shared, &world, 0);
        assert_eq!(models.stats_json().get("builds_uploaded"), Some(&Json::from(1)));
        drop(models);
        let msgs = gpu.close();
        assert!(msgs.iter().all(|m| m.severity < specula_gpu::Severity::Warning), "{msgs:?}");
    }

    #[test]
    fn rigged_recipes_carry_their_anim_sets() {
        let mut lib = Library::new(&sepulcra(), None);
        let creature = lib.resolve(&ModelSpec::new("recipes/shambler.json")).unwrap().handle;
        let stone = lib.resolve(&ModelSpec::new("recipes/warding_stone.json")).unwrap().handle;
        settle(&mut lib);
        let b = lib.built(creature).unwrap();
        let set = b.anims.as_ref().expect("anims/shambler.json loads");
        let names: Vec<&str> = set.clips().map(|(_, c)| c.name.as_str()).collect();
        assert_eq!(names, ["idle", "walk", "attack", "death", "climb", "shuffle", "still", "flinch"]);
        assert_eq!(set.graphs().map(|(n, _)| n).collect::<Vec<_>>(), ["creature"]);
        assert_eq!(set.skeleton.len(), b.model.rig.as_ref().unwrap().len());
        let r = report(&ModelSpec::new("recipes/shambler.json"), b, false);
        assert_eq!(r.get("clips").and_then(Json::as_array).map(|a| a.len()), Some(8));
        assert_eq!(r.get("bones").and_then(Json::as_u64), Some(set.skeleton.len() as u64));
        let s = lib.built(stone).unwrap();
        assert!(s.anims.is_none() && s.anim_note.is_none(), "unrigged: no anim set, nothing to say");
    }

    #[test]
    fn gltf_sources_import_on_workers_and_refuse_params() {
        let root = scratch("gltf", &[]);
        // A Sepulcra creature exported with its rig and clips.
        let style = Style::parse(&std::fs::read_to_string(sepulcra().join(STYLE_FILE)).unwrap()).unwrap();
        let mut lib_files = BTreeMap::new();
        for e in std::fs::read_dir(sepulcra().join("recipes")).unwrap() {
            let p = e.unwrap().path();
            lib_files.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
        let model = build(&lib_files["wraith"], &BuildParams { library: Some(&lib_files), ..BuildParams::new(&style) }).unwrap();
        let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
        let set = AnimSet::load(&std::fs::read_to_string(sepulcra().join("anims/wraith.json")).unwrap(), skel).unwrap();
        let clips: Vec<_> = set.clips().map(|(_, c)| c.clone()).collect();
        let glb = specula_gltf::export_glb(&specula_gltf::ExportInput { model: &model, lod: 0, skeleton: Some(&set.skeleton), clips: &clips }).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/wraith.glb"), &glb).unwrap();
        let mut lib = Library::new(&root, None);
        let spec = ModelSpec::new("assets/wraith.glb");
        let e = lib.resolve(&spec.clone().with_param("tier", 2.0)).unwrap_err();
        assert_eq!((e.code, e.detail("field")), ("invalid_value", Some(&Json::from("params"))));
        let missing = lib.resolve(&ModelSpec::new("assets/nope.glb")).unwrap_err();
        assert_eq!(missing.code, "recipe_not_found");
        let h = lib.resolve(&spec).unwrap();
        assert_eq!(h.state, ModelState::Building, "imports never run on the engine thread");
        settle(&mut lib);
        let b = lib.built(h.handle).unwrap_or_else(|| panic!("{:?}", lib.handle_error(h.handle)));
        assert!(b.model.lods[0].is_skinned());
        let anims = b.anims.as_ref().expect("the asset's clips");
        // The trackless "still" clip has nothing to export.
        assert_eq!(anims.clips().map(|(_, c)| c.name.clone()).collect::<Vec<_>>(), ["float", "glide", "dive", "swoop", "death", "float_shroud", "glide_shroud", "dive_shroud", "swoop_shroud", "flinch"]);
        assert_eq!(report(&spec, b, false).get("source"), Some(&Json::from("gltf")));
        // Unchanged file: same key, same build; a rewritten file rebuilds.
        let first = b.id;
        assert!(lib.poll(true).is_some_and(|r| r.jobs.is_empty()));
        assert_eq!(lib.built(h.handle).unwrap().id, first);
        std::thread::sleep(Duration::from_millis(20));
        // Another export (without its clips) over the same file.
        let changed = specula_gltf::export_glb(&specula_gltf::ExportInput { model: &model, lod: 0, skeleton: Some(&set.skeleton), clips: &clips[..1] }).unwrap();
        std::fs::write(root.join("assets/wraith.glb"), &changed).unwrap();
        let reload = lib.poll(true).unwrap();
        assert!(reload.files_changed >= 1, "{}", reload.to_json());
        settle(&mut lib);
        let rebuilt = lib.built(h.handle).unwrap_or_else(|| panic!("{:?}", lib.handle_error(h.handle)));
        assert_ne!(rebuilt.id, first, "rebuilt from the new file: {:?}", lib.handle_error(h.handle));
        assert_eq!(rebuilt.anims.as_ref().unwrap().len(), 1);
        // A build whose texture pixels went to the GPU is imported again when uploaded again.
        let before = rebuilt.id;
        lib.reimport(before);
        assert_eq!(lib.resolve(&spec).unwrap().state, ModelState::Building, "waits for a fresh import");
        settle(&mut lib);
        let again = lib.built(h.handle).unwrap();
        assert_ne!(again.id, before, "a new build of the same file");
        assert!(again.textures.iter().all(|t| !t.levels.lock().unwrap().is_empty()));
        // A build whose upload was freed (no entity used it) is forgotten: the next use of its
        // spec waits for a fresh import instead of finding a build without pixels.
        let id = again.id;
        lib.forget_build(id);
        assert_eq!(lib.resolve(&spec).unwrap().state, ModelState::Building);
        settle(&mut lib);
        assert_ne!(lib.built(h.handle).unwrap().id, id);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn playback_follows_render_time_speed_offset_and_loop_override() {
        let mut lib = Library::new(&sepulcra(), None);
        let h = lib.resolve(&ModelSpec::new("recipes/shambler.json")).unwrap().handle;
        settle(&mut lib);
        let set = Arc::clone(lib.built(h).unwrap().anims.as_ref().unwrap());
        let walk = set.clip(set.id("walk").unwrap()).clone();
        let mut a = AnimSpec::new("walk");
        a.start = Some(10.0);
        a.speed = 2.0;
        a.time_offset = 0.25;
        assert_eq!(playback(&set, &a, 11.0).unwrap().time, 2.25);
        assert_eq!(playback(&set, &a, 5.0).unwrap().time, 0.0, "before the start: the first pose");
        // Hold a looping clip's last pose.
        a.looping = Some(false);
        let held = playback(&set, &a, 1000.0).unwrap().time;
        assert!(held < walk.duration as f64 && held > walk.duration as f64 * 0.99, "{held}");
        // Loop a play-once clip.
        let death = set.clip(set.id("death").unwrap()).duration as f64;
        let mut d = AnimSpec::new("death");
        d.start = Some(0.0);
        d.looping = Some(true);
        let t = playback(&set, &d, death * 2.5).unwrap().time;
        assert!((t - death * 0.5).abs() < 1e-9, "{t}");
        assert!(playback(&set, &AnimSpec::new("moonwalk"), 1.0).is_none(), "an unknown clip plays nothing");
    }

    /// The skinned pipelines' background build fails (injected): the animated model is reported
    /// as failed (stats, warning with the VkResult and pipeline, structured problem), not as
    /// waiting (so captures don't park on it); a retry builds them and it draws.
    #[test]
    fn failed_pipeline_builds_are_failures_not_waits() {
        let Some(mut gpu) = gpu_or_skip() else { return };
        let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut walker = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        walker.anim = Some(AnimSpec::new("walk"));
        let walker = world.spawn(walker, "t").unwrap().id;
        settle_shared(&shared, &mut world);
        gpu.inject_background_fault(specula_gpu::BackgroundFault::Error);
        let mut models = GpuModels::default();
        assert!(models.sync(&mut gpu, &shared, &world, 0).is_empty());
        built(&mut models, &mut gpu);
        assert!(!models.waiting(world.get(walker).unwrap()), "the build ended: no longer a wait");
        let e = world.get(walker).unwrap();
        let warning = models.pipeline_problem(e).expect("a failed build is reported");
        assert!(warning.contains("skinned") && warning.contains("VkResult -2") && warning.contains("Mesh"), "{warning}");
        let problem = models.pipeline_failure_json().unwrap();
        assert_eq!(problem.get("vk_result").and_then(Json::as_f64), Some(-2.0), "{problem:?}");
        let mut stats = LodStats::default();
        let mut out = Vec::new();
        models.begin_frame(0.5);
        models.draw(e, Vec3::new(0.0, 1.0, 4.0), 50.0, 1080, &mut LodChoice::Fixed(0), &mut stats, &mut out);
        assert_eq!((stats.pipeline_failed, stats.missing, out.len()), (1, 0, 0));
        let _ = models.take_bones();
        // The retry (model_reload does this) builds them.
        assert!(gpu.retry_failed_variants());
        models.refresh_ready(&mut gpu);
        assert!(models.pipeline_problem(world.get(walker).unwrap()).is_none(), "retrying: waiting again, not failed");
        built(&mut models, &mut gpu);
        assert!(!models.waiting(world.get(walker).unwrap()), "the retry ended");
        assert!(models.pipeline_problem(world.get(walker).unwrap()).is_none() && models.pipeline_failure_json().is_none());
        let mut stats = LodStats::default();
        models.begin_frame(0.5);
        models.draw(world.get(walker).unwrap(), Vec3::new(0.0, 1.0, 4.0), 50.0, 1080, &mut LodChoice::Fixed(0), &mut stats, &mut out);
        assert!(stats.pipeline_failed == 0 && !out.is_empty(), "drawn after the retry");
        let _ = models.take_bones();
    }

    /// Waits for the background pipeline build to end (a join: `Gpu::wait_for_pipelines`,
    /// never a deadline: a minute's ran out once on a loaded machine, 2026-11-24) and tells
    /// `models` what it made.
    fn built(models: &mut GpuModels, gpu: &mut Gpu) {
        gpu.wait_for_pipelines();
        models.refresh_ready(gpu);
    }

    /// A GPU for a test, or `None` after saying why the test is skipped (rule 7: a machine
    /// without Vulkan passes having printed the reason).
    fn gpu_or_skip() -> Option<Gpu> {
        if !specula_gpu::is_available() {
            eprintln!("SKIPPED: Vulkan is not available");
            return None;
        }
        match Gpu::new(&specula_gpu::GpuOptions::default()) {
            Ok(g) => Some(g),
            Err(e) => {
                eprintln!("SKIPPED: {e}");
                None
            }
        }
    }

    /// A rig with no anims file of its own (its clips are written into the world) is drawn
    /// skinned when a caller poses it, as a filmstrip does; the studio's bind-pose shot of it
    /// stays unskinned.
    #[test]
    fn a_posed_rig_without_clips_of_its_own_is_drawn_skinned() {
        let Some(mut gpu) = gpu_or_skip() else { return };
        let rigged = r##"{"recipe":1,"name":"post","rig":{"blend":0.05,"bones":[{"name":"root","head":[0,0,0]},{"name":"top","parent":"root","head":[0,0.5,0],"tail":[0,1,0]}]},
            "parts":[{"name":"body","material":"stone","bones":["root","top"],"shape":{"op":"box","size":[0.2,1,0.2]}}]}"##;
        let root = scratch("posedrig", &[("post", rigged)]);
        let mut lib = Library::new(&root, None);
        let handle = lib.resolve(&ModelSpec::new("recipes/post.json")).unwrap().handle;
        settle(&mut lib);
        let built = Arc::clone(lib.built(handle).unwrap());
        assert!(built.anims.is_none() && built.model.lods[0].is_skinned(), "rigged, with no clips of its own");
        gpu.prepare_variants(true, false).expect("pipelines");
        let mut models = GpuModels::default();
        models.refresh_ready(&mut gpu);
        let palette = vec![Affine::IDENTITY; 2];
        let (mut bones, mut out) = (Vec::new(), Vec::new());
        models.posed_draws(&mut gpu, &built, &palette, &Mat4::IDENTITY, &mut bones, &mut out).unwrap();
        assert!(!out.is_empty() && out.iter().all(|d| d.skin == Some(0)) && bones.len() == 2, "posed: skinned with its palette");
        let (mut bones, mut out) = (Vec::new(), Vec::new());
        models.posed_draws_lod(&mut gpu, &built, &palette, &Mat4::IDENTITY, 0, &mut bones, &mut out).unwrap();
        assert!(!out.is_empty() && out.iter().all(|d| d.skin.is_none()), "the studio's bind-pose shot: unskinned");
        // An animated entity of it waits for the skinned pipelines; a still one does not.
        models.ready.skinned = false;
        let uploaded = &models.uploaded[&built.id];
        assert!(matches!(models.held_back_by(true, uploaded), Some(Held::Waiting)), "animated: waits for the skinned pipelines");
        assert!(models.held_back_by(false, uploaded).is_none(), "still: needs none");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn animated_entities_get_palettes_and_posed_bones() {
        let Some(mut gpu) = gpu_or_skip() else { return };
        let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut walker = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        walker.anim = Some(AnimSpec::new("walk"));
        let walker = world.spawn(walker, "t").unwrap().id;
        let still = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/shambler.json")), "t").unwrap().id;
        settle_shared(&shared, &mut world);
        let mut models = GpuModels::default();
        assert!(models.sync(&mut gpu, &shared, &world, 0).is_empty());
        // The skinned pipelines are built on a worker: the walker is held back (not drawn,
        // captures wait) until they exist; the still one, unanimated, draws at once.
        assert!(!models.waiting(world.get(still).unwrap()), "an unanimated model needs no skinned pipeline");
        built(&mut models, &mut gpu);
        assert!(!models.waiting(world.get(walker).unwrap()) && !models.waiting(world.get(still).unwrap()), "the skinned pipelines were built");
        let bones_of = |models: &GpuModels, t: f64| {
            models.begin_frame(t);
            let mut out = Vec::new();
            let mut st = LodStats::default();
            for id in [walker, still] {
                models.draw(world.get(id).unwrap(), Vec3::new(0.0, 1.0, 4.0), 50.0, 1080, &mut LodChoice::Fixed(0), &mut st, &mut out);
            }
            (out, models.take_bones())
        };
        let (draws, bones) = bones_of(&models, 0.3);
        let set = Arc::clone(shared.lock().built(world.get(walker).unwrap().resolved_model().unwrap().handle).unwrap().anims.as_ref().unwrap());
        assert_eq!(bones.len(), set.skeleton.len(), "one palette: the walker's");
        let parts = draws.len() / 2;
        assert!(draws[..parts].iter().all(|d| d.skin == Some(0)), "every part of the walker shares its palette");
        assert!(draws[..parts].iter().all(|d| d.bounds.is_some_and(|b| b[3] > 0.0 && b[3] < 3.0)), "posed parts carry posed bounds");
        assert!(draws[parts..].iter().all(|d| d.bounds.is_none()), "the bind pose culls by its mesh");
        assert!(draws[parts..].iter().all(|d| d.skin.is_none()), "no anim: the bind pose");
        models.put_bones(bones);
        // The same time gives the same pose; another time another.
        let (_, again) = bones_of(&models, 0.3);
        let (_, later) = bones_of(&models, 0.55);
        assert_eq!(again.len(), later.len());
        assert_ne!(again, later, "the walk moves");
        let (_, same) = bones_of(&models, 0.3);
        assert_eq!(same, again, "a pose is a function of render time");
        let e = world.get(walker).unwrap();
        let a = models.bone_positions(e, 0.3).unwrap();
        let b = models.bone_positions(e, 0.55).unwrap();
        assert_eq!(a.len(), set.skeleton.len());
        assert_eq!(a[0].0, set.skeleton.names[0]);
        assert!(a.iter().zip(&b).any(|(p, q)| (p.1 - q.1).length() > 0.01), "bones move between the times");
        let rest = models.bone_positions(world.get(still).unwrap(), 0.3).unwrap();
        let head = set.skeleton.heads[1];
        assert!((rest[1].1 - Vec3::new(head[0], head[1], head[2])).length() < 1e-5, "unanimated: the rest pose");
        drop(models);
        let msgs = gpu.close();
        assert!(msgs.iter().all(|m| m.severity < specula_gpu::Severity::Warning), "{msgs:?}");
    }

    #[test]
    fn anims_that_cannot_play_are_reported() {
        let shared = Arc::new(SharedLibrary::new(Library::new(&sepulcra(), None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let spawn = |world: &mut World, recipe: &str, clip: &str| {
            let mut s = SpawnSpec::model(ModelSpec::new(recipe));
            s.anim = Some(AnimSpec::new(clip));
            world.spawn(s, "t").unwrap().id
        };
        let walker = spawn(&mut world, "recipes/shambler.json", "walk");
        let typo = spawn(&mut world, "recipes/shambler.json", "moonwalk");
        let stone = spawn(&mut world, "recipes/warding_stone.json", "walk");
        assert!(anim_problem(&shared.lock(), world.get(typo).unwrap()).is_none(), "nothing to say while building");
        settle_shared(&shared, &mut world);
        let lib = shared.lock();
        assert_eq!(anim_problem(&lib, world.get(walker).unwrap()), None);
        let p = anim_problem(&lib, world.get(typo).unwrap()).unwrap();
        assert!(p.contains("'moonwalk'") && p.contains("walk, attack"), "{p}");
        let p = anim_problem(&lib, world.get(stone).unwrap()).unwrap();
        assert!(p.contains("no clips") && p.contains("no rig"), "{p}");
    }

    /// Every part of every Sepulcra creature, posed through every clip (30 samples a second, on
    /// every LOD): each skinned vertex lies inside the part's posed sphere, so a posed part (or
    /// its shadow) is never culled while visible. The probe that found the old 2× rest-sphere
    /// bound wrong (a runner's leaping eye 38× outside its part's rest radius).
    #[test]
    fn posed_spheres_hold_every_posed_vertex() {
        let style = Style::parse(&std::fs::read_to_string(sepulcra().join(STYLE_FILE)).unwrap()).unwrap();
        let mut files = BTreeMap::new();
        for e in std::fs::read_dir(sepulcra().join("recipes")).unwrap() {
            let p = e.unwrap().path();
            files.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
        }
        let mut checked = 0usize;
        for name in ["shambler", "skeleton_runner", "wraith", "bone_colossus", "brazier_fire"] {
            let model = build(&files[name], &BuildParams { library: Some(&files), ..BuildParams::new(&style) }).unwrap();
            let skel = Skeleton::from_rig(model.rig.as_ref().unwrap()).unwrap();
            let set = AnimSet::load(&std::fs::read_to_string(sepulcra().join(format!("anims/{name}.json"))).unwrap(), skel).unwrap();
            let parts: Vec<(PartMesh, Vec<(u16, f32)>)> = model
                .lods
                .iter()
                .flat_map(|lod| (0..lod.parts.len()).map(move |i| part_mesh(lod, i, false)))
                .map(|p| {
                    let infl = part_influence(&set.skeleton, &p.vertices, p.skin.as_ref().unwrap());
                    (p, infl)
                })
                .collect();
            for (clip, c) in set.clips() {
                let mut animator = Animator::new(&set, clip);
                let (mut posed, mut skin) = (Vec::new(), Vec::new());
                let steps = (c.duration * 30.0).ceil() as u32 + 1;
                for s in 0..=steps {
                    let t = s as f64 / 30.0;
                    animator.seek(Playback { clip, time: t, rate: 1.0 }, None);
                    animator.evaluate(&set);
                    animator.skin(&set, &mut posed, &mut skin);
                    for (k, (p, infl)) in parts.iter().enumerate() {
                        let (centre, r) = posed_sphere(infl, &posed).unwrap();
                        for (v, sv) in p.vertices.iter().zip(p.skin.as_ref().unwrap()) {
                            let mut q = [0.0f32; 3];
                            for j in 0..4 {
                                let w = sv.weights[j] as f32 / 255.0;
                                if w > 0.0 {
                                    let x = skin[sv.joints[j] as usize].apply_point(v.position);
                                    (0..3).for_each(|a| q[a] += w * x[a]);
                                }
                            }
                            let d = ((q[0] - centre[0]).powi(2) + (q[1] - centre[1]).powi(2) + (q[2] - centre[2]).powi(2)).sqrt();
                            assert!(d <= r, "{name}/{} t={t:.2} part {k}: a vertex {d:.4} m from the centre, sphere {r:.4} m", c.name);
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 100_000, "{checked} posed vertices checked");
    }
}
