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
//! # Levels of detail
//!
//! The size on screen is the projected diameter of the model's bounding sphere in pixels. LOD 0
//! draws at [`LOD_PIXELS`]`[0]` (192 px) and above, LOD 1 down to 64 px, LOD 2 down to 24 px,
//! LOD 3 below (clamped to the model's last LOD). The window keeps each entity's level with
//! [`LOD_HYSTERESIS`] (±15% around each threshold) so a model at a boundary doesn't flicker;
//! captures choose without memory, so a capture is reproducible.

use crate::world::{Entity, ModelSource, ModelSpec, ModelState, ResolvedModel, World, WorldError};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::{DrawItem, Gpu, Material, MeshId, Surface, Vertex};
use specula_model::{build, BuildParams, ErrorCode, MaterialKind, MaterialPreset, MeshData, Model, Recipe, RecipeError, Style};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// Recipe and style files are checked for changes at most this often.
pub const POLL_EVERY: Duration = Duration::from_secs(1);
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
/// Worker threads that build models.
const WORKERS: usize = 2;
/// Lists in a reload report stop here (the rest is counted).
const MAX_LISTED: usize = 20;

/// One built model, shared by every spec with the same content, parameters and seed.
pub struct Built {
    /// Unique per build: the GPU cache's key.
    pub id: u64,
    pub model: Model,
    /// Content hash of the recipe, its includes and its style.
    pub content: u64,
    pub build_ms: f64,
    /// The style file it was built with.
    pub style: PathBuf,
}

impl Built {
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let b = self.model.bounds;
        (Vec3::new(b.min[0], b.min[1], b.min[2]), Vec3::new(b.max[0], b.max[1], b.max[2]))
    }
}

/// `(content, parameters as exact bits, seed)`.
type BuildKey = (u64, String, Option<u64>);

/// One file of a recipe folder as last read.
struct SourceFile {
    text: Result<Arc<str>, String>,
    mtime: Option<SystemTime>,
}

/// A recipe folder: its regular `*.json` files (the include library).
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
struct RecipeInfo {
    folder: PathBuf,
    content: u64,
    text: Arc<str>,
    /// The files it includes, transitively (stem → text).
    library: Arc<BTreeMap<String, String>>,
    style: Arc<Style>,
    style_path: PathBuf,
    recipe: Recipe,
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
        ResolvedModel { handle: h, state, bounds: self.built.as_ref().map(|b| b.bounds()) }
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
    Built(Model),
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

/// The worker threads and their channels. Dropping it lets the workers finish and exit.
struct Workers {
    jobs: Option<Sender<Job>>,
    done: Receiver<Done>,
    /// Jobs nobody waits for any more: workers skip them.
    cancelled: Arc<Mutex<BTreeSet<u64>>>,
    /// The job queue's receiving end (kept so a library without workers still queues).
    _queue: Arc<Mutex<Receiver<Job>>>,
    /// Tests deliver results by hand through this.
    #[cfg(test)]
    inject: Sender<Done>,
}

impl Workers {
    fn start(n: usize) -> Workers {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, done) = mpsc::channel::<Done>();
        let rx = Arc::new(Mutex::new(rx));
        let cancelled = Arc::new(Mutex::new(BTreeSet::new()));
        for i in 0..n {
            let (rx, tx, cancelled) = (Arc::clone(&rx), tx.clone(), Arc::clone(&cancelled));
            let spawned = std::thread::Builder::new().name(format!("specula-model-{i}")).spawn(move || loop {
                let job = match rx.lock().unwrap_or_else(PoisonError::into_inner).recv() {
                    Ok(job) => job,
                    Err(_) => return, // the library is gone
                };
                let skip = cancelled.lock().unwrap_or_else(PoisonError::into_inner).remove(&job.id);
                let started = Instant::now();
                let outcome = if skip { Outcome::Cancelled } else { run(&job, build) };
                let done = Done {
                    id: job.id,
                    key: job.key,
                    recipe: job.recipe,
                    style_path: job.style_path,
                    outcome,
                    build_ms: started.elapsed().as_secs_f64() * 1000.0,
                };
                if tx.send(done).is_err() {
                    return;
                }
            });
            if let Err(e) = spawned {
                eprintln!("specula: model build thread {i} not started: {e}");
            }
        }
        Workers {
            jobs: Some(jobs),
            done,
            cancelled,
            _queue: rx,
            #[cfg(test)]
            inject: tx,
        }
    }
}

/// The build function workers call (`specula_model::build`; tests pass others).
type BuildFn = fn(&str, &BuildParams) -> Result<Model, RecipeError>;

/// Builds one job with `build`; a panic becomes [`Outcome::Panicked`] instead of a dead worker.
fn run(job: &Job, build: BuildFn) -> Outcome {
    let values: Vec<(&str, f64)> = job.params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let params = BuildParams { values: &values, seed: job.seed, library: Some(&job.library), ..BuildParams::new(&job.style) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(&job.text, &params)));
    match built {
        Ok(Ok(model)) => match check_uploadable(&model) {
            Ok(()) => Outcome::Built(model),
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
    workers: Workers,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::symlink_metadata(path).and_then(|m| m.modified()).ok()
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
            workers: Workers::start(workers),
            cache_hits: 0,
            cache_misses: 0,
        }
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
                match self.key_for(spec, true) {
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
        let key = self.key_for(spec, true)?;
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
    pub fn request(&mut self, spec: &ModelSpec) -> Result<BuildRequest, WorldError> {
        spec.check()?;
        let folder = Path::new(&spec.recipe).parent().unwrap_or(Path::new("")).to_owned();
        self.refresh_folder(&folder);
        let key = self.key_for(spec, false)?;
        if let Some(b) = self.builds.get(&key) {
            self.cache_hits += 1;
            let b = Arc::clone(b);
            self.remember(b.id);
            return Ok(BuildRequest::Ready(b, true));
        }
        Ok(BuildRequest::Pending(self.submit(&spec.recipe, &key, spec, true)))
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
        let info = &self.recipes[recipe];
        let id = self.next_id;
        self.next_id += 1;
        self.cache_misses += 1;
        let job = Job {
            id,
            key: key.clone(),
            recipe: recipe.to_owned(),
            text: Arc::clone(&info.text),
            library: Arc::clone(&info.library),
            style: Arc::clone(&info.style),
            style_path: info.style_path.clone(),
            params: spec.params.clone(),
            seed: spec.seed,
        };
        self.in_flight.insert(key.clone(), InFlight { job: id, pinned: pin });
        if let Some(tx) = &self.workers.jobs {
            if tx.send(job).is_err() {
                eprintln!("specula: model build workers are gone; {recipe} will not build");
            }
        }
        id
    }

    /// The build key for `spec`: everything checkable without building is checked here.
    /// `retry`: a recipe missing from its cached folder is looked for again (it may be new).
    fn key_for(&mut self, spec: &ModelSpec, retry: bool) -> Result<BuildKey, WorldError> {
        spec.check()?;
        let rel = Path::new(&spec.recipe);
        let folder = rel.parent().unwrap_or(Path::new("")).to_owned();
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        if !self.folders.contains_key(&folder) {
            // Cached folders were checked when read (and are checked again when re-read).
            self.check_inside(&folder, spec)?;
            // Folders that don't exist are never cached (agents could make the cache grow).
            let f = self.read_folder(&folder).ok_or_else(|| not_found(spec, &self.root))?;
            self.folders.insert(folder.clone(), f);
        } else if retry && !self.folders[&folder].files.contains_key(&stem) {
            self.refresh_folder(&folder);
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
        let mut todo = parsed.includes();
        while let Some(name) = todo.pop() {
            if library.contains_key(&name) {
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
        Ok(RecipeInfo {
            folder: folder.to_owned(),
            content: h.finish(),
            text,
            library: Arc::new(library),
            style,
            style_path,
            recipe: parsed,
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
        let dir = self.root.join(rel);
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
            let modified = mtime(&path);
            let reuse = old.and_then(|o| o.files.get(stem)).filter(|f| f.mtime == modified && modified.is_some());
            let text = match reuse {
                Some(f) => f.text.clone(),
                None => std::fs::read_to_string(&path).map(Arc::from).map_err(|e| e.to_string()),
            };
            files.insert(stem.to_owned(), SourceFile { text, mtime: modified });
        }
        Some(Folder { files, mtime: mtime(&dir) })
    }

    /// Re-reads one cached folder when it changed; forgets the recipes there whose text changed.
    /// Returns how many files changed (a folder that vanished counts its files).
    fn refresh_folder(&mut self, rel: &Path) -> usize {
        let Some(old) = self.folders.get(rel) else { return 0 };
        let dir = self.root.join(rel);
        let unchanged = mtime(&dir) == old.mtime
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
        if changed > 0 {
            // Any recipe there may include a changed file: work them out again (cheap: no builds).
            self.recipes.retain(|_, r| r.folder != rel);
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
        if !force && self.last_poll.elapsed() < POLL_EVERY {
            return None;
        }
        self.last_poll = Instant::now();
        let mut report = Reload::default();
        let folders: Vec<PathBuf> = self.folders.keys().cloned().collect();
        for f in folders {
            report.files_changed += self.refresh_folder(&f);
        }
        let stale_styles: Vec<PathBuf> = self.styles.iter().filter(|(p, s)| mtime(p) != s.mtime).map(|(p, _)| p.clone()).collect();
        for p in stale_styles {
            let file = read_style(&p);
            self.styles.insert(p.clone(), file);
            self.recipes.retain(|_, r| r.style_path != p);
            self.failed.clear();
            report.files_changed += 1;
        }
        // Folders handles need that weren't there (removed, or not made yet): back now?
        let missing: BTreeSet<PathBuf> = self
            .handles
            .iter()
            .flatten()
            .map(|h| Path::new(&h.spec.recipe).parent().unwrap_or(Path::new("")).to_owned())
            .filter(|f| !self.folders.contains_key(f))
            .collect();
        for f in missing {
            if !self.folder_inside(&f) {
                continue;
            }
            if let Some(folder) = self.read_folder(&f) {
                report.files_changed += folder.files.len().max(1);
                self.folders.insert(f.clone(), folder);
                self.recipes.retain(|_, r| r.folder != f);
                self.failed.retain(|r, _| Path::new(r).parent().unwrap_or(Path::new("")) != f);
            }
        }
        if report.files_changed == 0 && !force {
            return None;
        }
        // Handles whose recipe was forgotten above get their key worked out again.
        for h in 0..self.handles.len() {
            let Some(slot) = &self.handles[h] else { continue };
            if self.recipes.contains_key(&slot.spec.recipe) {
                continue;
            }
            let spec = slot.spec.clone();
            let current = self.handles[h].as_ref().and_then(|s| s.built.as_ref().map(|b| b.id));
            match self.key_for(&spec, false) {
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
            let result = match done.outcome {
                Outcome::Built(model) => {
                    let built = Arc::new(Built {
                        id: self.next_id,
                        model,
                        content: done.key.0,
                        build_ms: done.build_ms,
                        style: done.style_path,
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
                        self.workers.cancelled.lock().unwrap_or_else(PoisonError::into_inner).insert(f.job);
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
            finished.push(Finished { job: done.id, result });
        }
        (finished, changed)
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
        let mut cancelled = self.workers.cancelled.lock().unwrap_or_else(PoisonError::into_inner);
        for k in freed {
            if self.waiting.contains_key(&k) || self.in_flight.get(&k).is_none_or(|f| f.pinned) {
                continue;
            }
            if let Some(f) = self.in_flight.remove(&k) {
                cancelled.insert(f.job);
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
        let mut cancelled = self.workers.cancelled.lock().unwrap_or_else(PoisonError::into_inner);
        for k in dropped {
            if let Some(f) = self.in_flight.remove(&k) {
                cancelled.insert(f.job);
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

fn read_style(path: &Path) -> StyleFile {
    let text = if is_regular(path) || path.is_file() { std::fs::read_to_string(path) } else { Err(std::io::ErrorKind::NotFound.into()) };
    let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0003);
    let parsed = match &text {
        Ok(t) => {
            h.write_str(t);
            Style::parse(t).map(Arc::new)
        }
        Err(e) => Err(RecipeError::new("", ErrorCode::Style, format!("can't read {}: {e}", path.display()))),
    };
    StyleFile { parsed, mtime: mtime(path), hash: h.finish() }
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

/// The model handles the world's model entities use.
pub fn live_handles(world: &World) -> BTreeSet<u32> {
    world.entities().filter_map(|e| e.resolved_model().map(|r| r.handle)).collect()
}

/// The library as the world's [`ModelSource`], shared with the engine.
pub struct SharedLibrary(Mutex<Library>);

impl SharedLibrary {
    pub fn new(library: Library) -> SharedLibrary {
        SharedLibrary(Mutex::new(library))
    }

    /// The library (a poisoned lock is taken over: the library's state stays consistent between
    /// calls).
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Library> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ModelSource for SharedLibrary {
    fn resolve(&self, spec: &ModelSpec) -> Result<ResolvedModel, WorldError> {
        self.lock().resolve(spec)
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
    pub triangles: u64,
}

impl LodStats {
    pub fn to_json(self) -> Json {
        Json::obj([
            ("lod_histogram", Json::arr(self.histogram.map(Json::from))),
            ("missing", Json::from(self.missing)),
            ("triangles", Json::from(self.triangles)),
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
}

// ---------------------------------------------------------------------------------------------
// GPU

/// One material part of one LOD, uploaded.
struct PartDraw {
    mesh: MeshId,
    material: Material,
    surface: Surface,
    /// Multiplied by the entity's colour; alpha is the opacity of glass.
    color: [f32; 4],
    triangles: u32,
}

struct GpuModel {
    lods: Vec<Vec<PartDraw>>,
    /// Bounding sphere in object space.
    center: Vec3,
    radius: f32,
}

/// Uploaded builds and the handle → build map the renderer reads (see the module docs).
#[derive(Default)]
pub struct GpuModels {
    uploaded: BTreeMap<u64, GpuModel>,
    /// Handle → build id, as of the last sync.
    by_handle: Vec<Option<u64>>,
    /// Builds whose upload failed, with why.
    failed: BTreeMap<u64, String>,
    /// `(world mesh epoch, library generation, world generation)` of the last sync.
    synced: Option<(u64, u64, u64)>,
}

impl GpuModels {
    /// Uploads the builds live model entities use and frees the rest, when the world's model
    /// entities or the library changed since the last call. Returns messages for failed uploads.
    pub fn sync(&mut self, gpu: &mut Gpu, library: &SharedLibrary, world: &World, world_generation: u64) -> Vec<String> {
        let lib = library.lock();
        let key = (world.mesh_epoch(), lib.generation(), world_generation);
        if self.synced == Some(key) {
            return Vec::new();
        }
        self.synced = Some(key);
        let live = live_handles(world);
        self.by_handle.clear();
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
        let stale: Vec<u64> = self.uploaded.keys().copied().filter(|id| !wanted.contains_key(id)).collect();
        for id in stale {
            if let Some(m) = self.uploaded.remove(&id) {
                for mesh in m.lods.iter().flatten().map(|p| p.mesh) {
                    gpu.destroy_mesh(mesh);
                }
            }
        }
        self.failed.retain(|id, _| wanted.contains_key(id));
        let mut errors = Vec::new();
        for (id, built) in wanted {
            if self.uploaded.contains_key(&id) || self.failed.contains_key(&id) {
                continue;
            }
            match upload(gpu, &built.model) {
                Ok(m) => {
                    self.uploaded.insert(id, m);
                }
                Err(e) => {
                    let msg = format!("model '{}' not drawn: {e}", built.model.name);
                    errors.push(msg.clone());
                    self.failed.insert(id, msg);
                }
            }
        }
        errors
    }

    /// Messages for builds that failed to upload (they aren't drawn).
    pub fn failures(&self) -> impl Iterator<Item = &String> {
        self.failed.values()
    }

    /// Builds and meshes on the GPU.
    pub fn stats_json(&self) -> Json {
        let meshes: usize = self.uploaded.values().map(|m| m.lods.iter().map(Vec::len).sum::<usize>()).sum();
        Json::obj([
            ("builds_uploaded", Json::from(self.uploaded.len())),
            ("meshes", Json::from(meshes)),
            ("failed_uploads", Json::from(self.failed.len())),
        ])
    }

    /// Appends `e`'s draws (one per material part of the chosen LOD). `eye`, `fov_y_deg` and
    /// `viewport_h` size it on screen.
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
        let model = self
            .by_handle
            .get(e.resolved_model().map_or(usize::MAX, |r| r.handle as usize))
            .copied()
            .flatten()
            .and_then(|id| self.uploaded.get(&id));
        let Some(model) = model else {
            stats.missing += 1;
            return;
        };
        let world = e.model();
        let count = model.lods.len();
        let level = match choice {
            LodChoice::Fixed(l) => (*l).min(count - 1),
            LodChoice::Fresh => select_lod(sphere_px(&world, model, eye, fov_y_deg, viewport_h), count, None),
            LodChoice::Remembered(levels) => {
                let px = sphere_px(&world, model, eye, fov_y_deg, viewport_h);
                let previous = levels.get(&e.id).map(|l| *l as usize);
                let level = select_lod(px, count, previous);
                if previous != Some(level) {
                    levels.insert(e.id, level as u8);
                }
                level
            }
        };
        stats.histogram[level.min(LOD_BUCKETS - 1)] += 1;
        let cols = world.to_cols_array();
        let tint = e.color;
        for p in &model.lods[level] {
            stats.triangles += u64::from(p.triangles);
            out.push(DrawItem {
                mesh: p.mesh,
                model: cols,
                color: [p.color[0] * tint[0], p.color[1] * tint[1], p.color[2] * tint[2], p.color[3]],
                material: p.material,
                surface: p.surface,
                skin: None,
            });
        }
    }
}

/// `model`'s bounding sphere under the entity's world transform, in pixels across.
fn sphere_px(world: &Mat4, model: &GpuModel, eye: Vec3, fov_y_deg: f32, viewport_h: u32) -> f32 {
    let center = world.transform_point(model.center);
    let c = &world.cols;
    let scale = [c[0], c[1], c[2]].iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0f32, f32::max);
    screen_px(model.radius * scale, (center - eye).length(), fov_y_deg, viewport_h)
}

/// One mesh per LOD per material part.
fn upload(gpu: &mut Gpu, model: &Model) -> Result<GpuModel, String> {
    let mut lods = Vec::with_capacity(model.lods.len());
    let mut uploaded: Vec<MeshId> = Vec::new();
    for lod in &model.lods {
        let mut parts = Vec::with_capacity(lod.parts.len());
        for (i, part) in lod.parts.iter().enumerate() {
            let preset = &model.materials.iter().find(|(n, _)| *n == part.material).expect("checked when built").1;
            let (vertices, indices, mean) = part_mesh(lod, i);
            if indices.is_empty() {
                continue;
            }
            let mesh = match gpu.upload_mesh(&vertices, &indices) {
                Ok(m) => m,
                Err(e) => {
                    for m in uploaded {
                        gpu.destroy_mesh(m);
                    }
                    return Err(e.to_string());
                }
            };
            uploaded.push(mesh);
            let (material, surface, color) = part_look(preset, mean);
            parts.push(PartDraw { mesh, material, surface, color, triangles: (indices.len() / 3) as u32 });
        }
        lods.push(parts);
    }
    let b = model.bounds;
    let (lo, hi) = (Vec3::new(b.min[0], b.min[1], b.min[2]), Vec3::new(b.max[0], b.max[1], b.max[2]));
    Ok(GpuModel { lods, center: (lo + hi) * 0.5, radius: ((hi - lo).length() * 0.5).max(1e-3) })
}

/// The vertices and indices of part `i` alone (re-indexed), and its mean vertex colour (sRGB).
pub fn part_mesh(m: &MeshData, i: usize) -> (Vec<Vertex>, Vec<u32>, [f32; 3]) {
    let src = m.part_indices(i);
    let mut remap = vec![u32::MAX; m.positions.len()];
    let mut vertices = Vec::new();
    let mut indices = Vec::with_capacity(src.len());
    let mut sum = [0.0f64; 3];
    for &v in src {
        let slot = &mut remap[v as usize];
        if *slot == u32::MAX {
            *slot = vertices.len() as u32;
            let c = m.colors[v as usize];
            for k in 0..3 {
                sum[k] += f64::from(c[k]);
            }
            vertices.push(Vertex::with_color(m.positions[v as usize], m.normals[v as usize], c));
        }
        indices.push(*slot);
    }
    let n = vertices.len().max(1) as f64;
    (vertices, indices, sum.map(|s| (s / n / 255.0) as f32))
}

/// A style preset as the engine's material (see the module docs).
pub fn part_look(p: &MaterialPreset, mean: [f32; 3]) -> (Material, Surface, [f32; 4]) {
    let surface = Surface { emissive: p.emissive, metallic: p.metallic, roughness: p.roughness, specular: p.specular, ..Surface::default() };
    match p.kind {
        MaterialKind::Standard => (Material::Lit, surface, [1.0; 4]),
        MaterialKind::Glass => (Material::Glass, surface, [mean[0], mean[1], mean[2], p.opacity]),
    }
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

    /// A heavy-but-legal recipe (from the review's probe): many subdivisions and noise passes.
    fn heavy() -> String {
        let mut ops = vec![r#"{"op":"subdivide","levels":4}"#.to_string(), r#"{"op":"subdivide","levels":2}"#.to_string()];
        ops.extend((0..30).map(|_| r#"{"op":"displace","amount":0.01,"scale":0.5,"octaves":6}"#.to_string()));
        ops.extend((0..10).map(|_| r#"{"op":"ao","amount":0.3}"#.to_string()));
        let part = |i: u32| format!(r#"{{"name":"p{i}","material":"stone","shape":{{"op":"box","size":[1,1,1]}},"ops":[{}]}}"#, ops.join(","));
        format!(r#"{{"recipe":1,"name":"heavy","lods":[0.5,0.25],"budget":{{"max_triangles":200000}},"parts":[{},{}]}}"#, part(0), part(1))
    }

    /// Takes finished builds until none is in flight (or 60 s pass).
    fn settle(lib: &mut Library) -> BTreeSet<u32> {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut changed = BTreeSet::new();
        while lib.in_flight() > 0 && Instant::now() < deadline {
            changed.extend(lib.finish(Some(Duration::from_millis(50))).1);
        }
        assert_eq!(lib.in_flight(), 0, "builds didn't finish");
        changed
    }

    fn settle_shared(shared: &SharedLibrary, world: &mut World) {
        let changed = settle(&mut shared.lock());
        world.refresh_model_handles(&changed);
    }

    #[test]
    fn every_sepulcra_recipe_builds_in_the_engine() {
        let mut lib = Library::new(&sepulcra(), None);
        let mut handles = Vec::new();
        for e in std::fs::read_dir(sepulcra().join("recipes")).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let text = std::fs::read_to_string(&path).unwrap();
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
        let took = started.elapsed();
        assert!(took < Duration::from_millis(100), "resolving took {took:?}");
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
        // The included file changes: both its own spec and the host's rebuild.
        touch("block", &block(2.0), 10);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!((report.rebuilding.len(), report.errors.len()), (2, 0), "{:?}", report.rebuilding);
        settle_shared(&shared, &mut world);
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 2.9);
        assert_eq!(world.get(host).unwrap().local_bounds().unwrap().1.y, 2.9, "the host follows its include");
        // Break it: the error is reported and the last good build stays.
        touch("block", "{\"recipe\": 1, \"parts\": 7}", 15);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!(report.errors.len(), 1, "the host fails at build time, the block at parse time");
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
        deliver(&mut lib, second, Outcome::Built(model()));
        assert_eq!(lib.resolve(&spec).unwrap().state, ModelState::Ready);
        // Again, but this time the old job's good result arrives first: it serves the entity and
        // the replacement is cancelled.
        lib.collect_garbage(&BTreeSet::new());
        lib.handles[h as usize].as_mut().unwrap().built = None; // as if never built
        lib.handles[h as usize].as_mut().unwrap().built_key = None;
        lib.builds.clear();
        lib.resolve(&spec).unwrap();
        let replacement = lib.in_flight[&key].job;
        deliver(&mut lib, first, Outcome::Built(model()));
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
        if !specula_gpu::is_available() {
            eprintln!("SKIPPED: Vulkan is not available");
            return;
        }
        let mut gpu = match Gpu::new(&specula_gpu::GpuOptions::default()) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("SKIPPED: {e}");
                return;
            }
        };
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
        assert!(models.sync(&mut gpu, &shared, &world, 0).is_empty());
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
}
