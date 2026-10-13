//! Procedural models in the world: recipe files built with `specula-model`, cached, uploaded once
//! and drawn at a level of detail that fits their size on screen.
//!
//! # Where recipes come from
//!
//! A model entity names a recipe file relative to the **assets root** (`--assets DIR`, default the
//! engine's working directory), e.g. `recipes/warding_stone.json`. Every `*.json` file in the same
//! folder is the recipe's include library (by file stem), and its style is `--style FILE` or the
//! first `style.json` found walking up from the recipe's folder to the assets root (Sepulcra keeps
//! `assets/style.json` above `assets/recipes/`). Paths can't leave the assets root.
//!
//! # Cache and sharing
//!
//! A build is keyed by **content**: a hash of the recipe file, its folder's library and its style,
//! plus the parameters and seed. Entities with the same key share one [`Built`] model, and the
//! GPU side ([`GpuModels`]) uploads each build once: one mesh per LOD per material part, so equal
//! models instance together. A spec (recipe path + parameters + seed) gets a stable **handle**;
//! the world stores it on the entity (with the model's bounds), and the renderer maps handles to
//! uploaded meshes without locking the library.
//!
//! # Hot reload
//!
//! [`Library::poll`] checks recipe and style modification times at most once a second (or now,
//! for `model_reload` and `model_build`). When anything changed, every handle is rebuilt; unchanged
//! content hits the cache, changed content gets a new build, and a recipe that broke keeps its
//! last good build (the error is reported). The world then re-reads bounds
//! ([`crate::world::World::refresh_models`]) and the GPU cache re-syncs.
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

use crate::world::{Entity, ModelSource, ModelSpec, ResolvedModel, World, WorldError};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::{DrawItem, Gpu, Material, MeshId, Surface, Vertex};
use specula_model::{build, BuildParams, MaterialKind, MaterialPreset, MeshData, Model, RecipeError, Style};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
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
/// Unused handles allowed before the library drops them.
const SPARE_HANDLES: usize = 256;

/// One built model, shared by every spec with the same content, parameters and seed.
pub struct Built {
    /// Unique per build: the GPU cache's key.
    pub id: u64,
    pub model: Model,
    /// Content hash of the recipe, its library folder and its style.
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

/// One recipe file as last read.
struct SourceFile {
    text: Result<String, String>,
    mtime: Option<SystemTime>,
}

/// A recipe folder: its files (the include library) and their combined hash.
struct Folder {
    files: BTreeMap<String, SourceFile>,
    /// The folder's own modification time (files added or removed).
    mtime: Option<SystemTime>,
    hash: u64,
    /// Stem → text of every readable file, for `include`.
    library: BTreeMap<String, String>,
}

struct StyleFile {
    parsed: Result<Arc<Style>, RecipeError>,
    mtime: Option<SystemTime>,
    hash: u64,
}

/// A spec's slot: what it currently builds to.
struct Handle {
    spec: ModelSpec,
    built: Option<Arc<Built>>,
    /// The latest rebuild's error (the last good build stays in `built`).
    error: Option<WorldError>,
}

type BuildKey = (u64, String, Option<u64>);

/// What a reload changed.
#[derive(Default)]
pub struct Reload {
    pub files_changed: usize,
    /// Recipes whose model changed.
    pub rebuilt: Vec<String>,
    /// Specs whose rebuild failed (they keep their last good build).
    pub errors: Vec<Json>,
}

impl Reload {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("files_changed", Json::from(self.files_changed)),
            ("rebuilt", Json::arr(self.rebuilt.iter().map(|r| Json::from(r.as_str())))),
            ("errors", Json::Arr(self.errors.clone())),
        ])
    }
}

/// Recipe files, styles, builds and handles (see the module docs).
pub struct Library {
    root: PathBuf,
    /// `root`, canonicalized when it exists (paths are checked to stay inside it).
    canonical_root: Option<PathBuf>,
    style_override: Option<PathBuf>,
    /// By folder, relative to the root.
    folders: BTreeMap<PathBuf, Folder>,
    /// By absolute path.
    styles: BTreeMap<PathBuf, StyleFile>,
    builds: BTreeMap<BuildKey, Arc<Built>>,
    handles: Vec<Option<Handle>>,
    free_handles: Vec<u32>,
    by_spec: BTreeMap<String, u32>,
    next_build: u64,
    /// Bumped whenever a handle's build changes (the GPU cache re-syncs on it).
    generation: u64,
    last_poll: Instant,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Library {
    /// `root`: the assets root; `style`: a style file for every recipe (otherwise looked up next
    /// to each recipe).
    pub fn new(root: &Path, style: Option<&Path>) -> Library {
        Library {
            root: root.to_owned(),
            canonical_root: root.canonicalize().ok(),
            style_override: style.map(Path::to_owned),
            folders: BTreeMap::new(),
            styles: BTreeMap::new(),
            builds: BTreeMap::new(),
            handles: Vec::new(),
            free_handles: Vec::new(),
            by_spec: BTreeMap::new(),
            next_build: 1,
            generation: 0,
            last_poll: Instant::now(),
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

    /// The latest error of a handle whose recipe broke after it built once.
    pub fn handle_error(&self, handle: u32) -> Option<&WorldError> {
        self.handles.get(handle as usize)?.as_ref()?.error.as_ref()
    }

    pub fn handle_spec(&self, handle: u32) -> Option<&ModelSpec> {
        self.handles.get(handle as usize)?.as_ref().map(|h| &h.spec)
    }

    /// The spec's handle and bounds, building it if needed.
    pub fn resolve(&mut self, spec: &ModelSpec) -> Result<ResolvedModel, WorldError> {
        let key = spec.key();
        if let Some(&h) = self.by_spec.get(&key) {
            let slot = self.handles[h as usize].as_ref().expect("indexed handles exist");
            if let Some(b) = &slot.built {
                return Ok(ResolvedModel { handle: h, bounds: b.bounds() });
            }
        }
        let built = self.build(spec)?.0;
        let bounds = built.bounds();
        let h = match self.by_spec.get(&key) {
            Some(&h) => h,
            None => {
                let slot = Some(Handle { spec: spec.clone(), built: None, error: None });
                let h = match self.free_handles.pop() {
                    Some(h) => {
                        self.handles[h as usize] = slot;
                        h
                    }
                    None => {
                        self.handles.push(slot);
                        (self.handles.len() - 1) as u32
                    }
                };
                self.by_spec.insert(key, h);
                h
            }
        };
        let slot = self.handles[h as usize].as_mut().expect("just set");
        slot.built = Some(built);
        slot.error = None;
        self.generation += 1;
        Ok(ResolvedModel { handle: h, bounds })
    }

    /// Builds `spec` (from the cache when its content, parameters and seed were built before).
    /// Returns the build and whether it came from the cache.
    pub fn build(&mut self, spec: &ModelSpec) -> Result<(Arc<Built>, bool), WorldError> {
        spec.check()?;
        let rel = Path::new(&spec.recipe);
        let folder_rel = rel.parent().unwrap_or(Path::new("")).to_owned();
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        self.check_inside(rel, spec)?;
        if !self.folders.contains_key(&folder_rel) {
            let folder = self.read_folder(&folder_rel);
            self.folders.insert(folder_rel.clone(), folder);
        }
        let style_path = self.style_path(&folder_rel).ok_or_else(|| {
            WorldError::new(
                "style_not_found",
                format!(
                    "no {STYLE_FILE} next to {} or above it inside the assets root {}; add one or start the engine with --style",
                    spec.recipe,
                    self.root.display()
                ),
            )
            .with("field", "recipe")
            .with("recipe", spec.recipe.as_str())
        })?;
        let (style, style_hash) = self.style(&style_path, spec)?;
        let folder = &self.folders[&folder_rel];
        let text = match folder.files.get(&stem).map(|f| &f.text) {
            Some(Ok(t)) => t,
            Some(Err(e)) => {
                return Err(WorldError::new("recipe_unreadable", format!("{}: {e}", spec.recipe))
                    .with("field", "recipe")
                    .with("recipe", spec.recipe.as_str()))
            }
            None => {
                return Err(WorldError::new(
                    "recipe_not_found",
                    format!("no recipe file {} under the assets root {}", spec.recipe, self.root.display()),
                )
                .with("field", "recipe")
                .with("recipe", spec.recipe.as_str())
                .with("assets_root", self.root.display().to_string()))
            }
        };
        let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0001);
        h.write_str(text);
        h.write_u64(folder.hash);
        h.write_u64(style_hash);
        let content = h.finish();
        let params_key: String = spec.params.iter().map(|(k, v)| format!("{k}={:016x};", v.to_bits())).collect();
        let key = (content, params_key, spec.seed);
        if let Some(b) = self.builds.get(&key) {
            self.cache_hits += 1;
            return Ok((Arc::clone(b), true));
        }
        self.cache_misses += 1;
        let values: Vec<(&str, f64)> = spec.params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let started = Instant::now();
        let params = BuildParams { values: &values, seed: spec.seed, library: Some(&folder.library), ..BuildParams::new(&style) };
        let model = build(text, &params).map_err(|e| recipe_error(spec, &e))?;
        check_uploadable(&model).map_err(|m| {
            WorldError::new("recipe_invalid", format!("{}: {m}", spec.recipe)).with("field", "recipe").with("recipe", spec.recipe.as_str())
        })?;
        let built = Arc::new(Built {
            id: self.next_build,
            model,
            content,
            build_ms: started.elapsed().as_secs_f64() * 1000.0,
            style: style_path,
        });
        self.next_build += 1;
        self.builds.insert(key, Arc::clone(&built));
        Ok((built, false))
    }

    /// Refuses a recipe path that leads outside the assets root (through a link, say).
    fn check_inside(&self, rel: &Path, spec: &ModelSpec) -> Result<(), WorldError> {
        let (Some(root), Ok(full)) = (&self.canonical_root, self.root.join(rel).canonicalize()) else {
            return Ok(()); // missing files are reported when read
        };
        if full.starts_with(root) {
            Ok(())
        } else {
            Err(WorldError::new("recipe_outside_assets", format!("{} leads outside the assets root", spec.recipe))
                .with("field", "recipe")
                .with("recipe", spec.recipe.as_str()))
        }
    }

    fn read_folder(&self, rel: &Path) -> Folder {
        let dir = self.root.join(rel);
        let mut files = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_none_or(|x| x != "json") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
                if stem == "style" {
                    continue;
                }
                let text = std::fs::read_to_string(&path).map_err(|e| e.to_string());
                files.insert(stem.to_owned(), SourceFile { text, mtime: mtime(&path) });
            }
        }
        let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0002);
        let mut library = BTreeMap::new();
        for (stem, f) in &files {
            h.write_str(stem);
            match &f.text {
                Ok(t) => {
                    h.write_str(t);
                    library.insert(stem.clone(), t.clone());
                }
                Err(e) => h.write_str(e),
            }
        }
        Folder { files, mtime: mtime(&dir), hash: h.finish(), library }
    }

    /// `--style`, or the nearest `style.json` from the recipe's folder up to the root.
    fn style_path(&self, folder_rel: &Path) -> Option<PathBuf> {
        if let Some(p) = &self.style_override {
            return Some(p.clone());
        }
        let mut at = Some(folder_rel);
        while let Some(rel) = at {
            let candidate = self.root.join(rel).join(STYLE_FILE);
            if candidate.is_file() {
                return Some(candidate);
            }
            at = rel.parent();
        }
        None
    }

    fn style(&mut self, path: &Path, spec: &ModelSpec) -> Result<(Arc<Style>, u64), WorldError> {
        if !self.styles.contains_key(path) {
            let file = read_style(path);
            self.styles.insert(path.to_owned(), file);
        }
        let s = &self.styles[path];
        match &s.parsed {
            Ok(style) => Ok((Arc::clone(style), s.hash)),
            Err(e) => Err(WorldError::new("style_invalid", format!("{} (style for {}): {e}", path.display(), spec.recipe))
                .with("field", "recipe")
                .with("style", path.display().to_string())
                .with("recipe_error", e.to_json())),
        }
    }

    /// Re-reads changed recipe folders and styles (at most once per [`POLL_EVERY`] unless
    /// `force`) and rebuilds every handle. `None` when nothing was checked or changed.
    pub fn poll(&mut self, force: bool) -> Option<Reload> {
        if !force && self.last_poll.elapsed() < POLL_EVERY {
            return None;
        }
        self.last_poll = Instant::now();
        let mut changed = 0;
        let stale: Vec<PathBuf> = self
            .folders
            .iter()
            .filter(|(rel, f)| {
                let dir = self.root.join(rel);
                mtime(&dir) != f.mtime || f.files.keys().any(|stem| mtime(&dir.join(format!("{stem}.json"))) != f.files[stem].mtime)
            })
            .map(|(rel, _)| rel.clone())
            .collect();
        for rel in stale {
            let folder = self.read_folder(&rel);
            let old = self.folders.insert(rel, folder);
            changed += old.map_or(1, |o| o.files.len().max(1));
        }
        let stale_styles: Vec<PathBuf> = self.styles.iter().filter(|(p, s)| mtime(p) != s.mtime).map(|(p, _)| p.clone()).collect();
        for p in stale_styles {
            let file = read_style(&p);
            self.styles.insert(p, file);
            changed += 1;
        }
        if changed == 0 && !force {
            return None;
        }
        let mut report = Reload { files_changed: changed, ..Reload::default() };
        for h in 0..self.handles.len() {
            let Some(spec) = self.handles[h].as_ref().map(|s| s.spec.clone()) else { continue };
            let result = self.build(&spec);
            let slot = self.handles[h].as_mut().expect("checked above");
            match result {
                Ok((built, _)) => {
                    if slot.built.as_ref().is_none_or(|b| b.id != built.id) {
                        report.rebuilt.push(spec.key());
                        slot.built = Some(built);
                        self.generation += 1;
                    }
                    slot.error = None;
                }
                Err(e) => {
                    report.errors.push(e.to_json());
                    slot.error = Some(e);
                }
            }
        }
        // Builds no handle uses any more (old content, model_build experiments) are dropped.
        let used: BTreeSet<u64> = self.handles.iter().flatten().filter_map(|h| h.built.as_ref().map(|b| b.id)).collect();
        self.builds.retain(|_, b| used.contains(&b.id));
        Some(report)
    }

    /// Forgets handles no live entity uses once there are many of them (a game spawning a fresh
    /// seed per enemy would otherwise keep every variant forever). Entities that come back later
    /// (undo) resolve their spec again.
    pub fn collect_garbage(&mut self, live: &BTreeSet<u32>) {
        let used = self.handles.iter().flatten().count();
        if used <= live.len() + SPARE_HANDLES {
            return;
        }
        for (h, slot) in self.handles.iter_mut().enumerate() {
            if slot.is_some() && !live.contains(&(h as u32)) {
                let gone = slot.take().expect("checked");
                self.by_spec.remove(&gone.spec.key());
                self.free_handles.push(h as u32);
            }
        }
        let kept: BTreeSet<u64> = self.handles.iter().flatten().filter_map(|h| h.built.as_ref().map(|b| b.id)).collect();
        self.builds.retain(|_, b| kept.contains(&b.id));
        self.generation += 1;
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
        Json::obj([
            ("assets_root", Json::from(self.root.display().to_string())),
            ("style", self.style_override.as_ref().map_or(Json::Null, |p| Json::from(p.display().to_string()))),
            ("specs", Json::from(self.handles.iter().flatten().count())),
            ("builds", Json::from(self.builds.len())),
            ("cache_hits", Json::from(self.cache_hits)),
            ("cache_misses", Json::from(self.cache_misses)),
            ("broken", Json::Arr(errors)),
        ])
    }
}

fn read_style(path: &Path) -> StyleFile {
    let text = std::fs::read_to_string(path);
    let mut h = StableHasher::with_seed(0x6d6f_6465_6c00_0003);
    let parsed = match &text {
        Ok(t) => {
            h.write_str(t);
            Style::parse(t).map(Arc::new)
        }
        Err(e) => Err(RecipeError::new("", specula_model::ErrorCode::Style, format!("can't read {}: {e}", path.display()))),
    };
    StyleFile { parsed, mtime: mtime(path), hash: h.finish() }
}

/// A recipe error as a world error: code `recipe_invalid`, the recipe, and the build's own
/// structured error (`code`, `path` into the recipe, `message`, offsets and numbers).
pub fn recipe_error(spec: &ModelSpec, e: &RecipeError) -> WorldError {
    WorldError::new("recipe_invalid", format!("{}: {e}", spec.recipe))
        .with("field", "recipe")
        .with("recipe", spec.recipe.as_str())
        .with("recipe_error", e.to_json())
}

/// Every part's vertices fit in one upload (u32 indices).
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
        let mut lib = library.lock();
        let key = (world.mesh_epoch(), lib.generation(), world_generation);
        if self.synced == Some(key) {
            return Vec::new();
        }
        let live: BTreeSet<u32> = world.entities().filter_map(|e| e.resolved_model().map(|r| r.handle)).collect();
        lib.collect_garbage(&live);
        self.synced = Some((world.mesh_epoch(), lib.generation(), world_generation));
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

    #[test]
    fn every_sepulcra_recipe_builds_in_the_engine() {
        let mut lib = Library::new(&sepulcra(), None);
        let mut n = 0;
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
                let (lo, hi) = r.bounds;
                assert!(hi.y > lo.y && hi.x > lo.x, "{name}: bounds {lo:?} {hi:?}");
                n += 1;
            }
        }
        assert!(n >= 20, "{n} builds");
    }

    #[test]
    fn identical_specs_share_one_build_and_variants_do_not() {
        let root = scratch("share", &[("block", &block(1.0)), ("twin", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let a = ModelSpec::new("recipes/block.json").with_param("tier", 2.0);
        let (r1, r2) = (lib.resolve(&a).unwrap(), lib.resolve(&a).unwrap());
        assert_eq!(r1, r2, "one handle per spec");
        assert_eq!((lib.cache_misses, lib.cache_hits), (1, 0), "the second resolve didn't even look");
        // The same content under another name is the same build (keyed by content, not path).
        let twin = lib.resolve(&ModelSpec::new("recipes/twin.json").with_param("tier", 2.0)).unwrap();
        assert_ne!(twin.handle, r1.handle);
        assert_eq!(lib.built(twin.handle).unwrap().id, lib.built(r1.handle).unwrap().id);
        // Other parameters or seeds are other builds.
        let t3 = lib.resolve(&ModelSpec::new("recipes/block.json").with_param("tier", 3.0)).unwrap();
        let seeded = lib.resolve(&a.clone().with_seed(5)).unwrap();
        let ids: BTreeSet<u64> = [r1, t3, seeded].iter().map(|r| lib.built(r.handle).unwrap().id).collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(t3.bounds.1.y, 3.0 + 0.9, "bounds come from the model (box + a halo resting 0.3 above it)");
        let (_, cached) = lib.build(&a).unwrap();
        assert!(cached);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recipe_failures_are_structured() {
        let bad_op = r#"{"recipe":1,"name":"bad","parts":[{"name":"p","material":"stone","shape":{"op":"box","size":[1,1,1]},
                         "ops":[{"op":"explode"}]}]}"#;
        let root = scratch("errors", &[("bad_op", bad_op), ("bad_json", "{\"recipe\":"), ("block", &block(1.0))]);
        let mut lib = Library::new(&root, None);
        let err = |lib: &mut Library, spec: ModelSpec| lib.resolve(&spec).err().unwrap();
        let e = err(&mut lib, ModelSpec::new("recipes/bad_op.json"));
        assert_eq!(e.code, "recipe_invalid");
        let inner = e.detail("recipe_error").unwrap();
        assert_eq!(inner.get("code"), Some(&Json::from("UnknownOp")));
        assert_eq!(inner.get("path"), Some(&Json::from("parts[0].ops[0].op")), "{inner}");
        let e = err(&mut lib, ModelSpec::new("recipes/bad_json.json"));
        assert_eq!(e.detail("recipe_error").and_then(|j| j.get("code")), Some(&Json::from("Json")));
        assert_eq!(err(&mut lib, ModelSpec::new("recipes/nope.json")).code, "recipe_not_found");
        let e = err(&mut lib, ModelSpec::new("recipes/block.json").with_param("tier", 9.0));
        assert_eq!(e.detail("recipe_error").and_then(|j| j.get("code")), Some(&Json::from("OutOfRange")));
        let e = err(&mut lib, ModelSpec::new("recipes/block.json").with_param("height", 1.0));
        assert_eq!(e.detail("recipe_error").and_then(|j| j.get("code")), Some(&Json::from("UnknownName")));
        assert_eq!(err(&mut lib, ModelSpec::new("../escape.json")).code, "invalid_value");
        // No style anywhere: a clear error, not a panic.
        std::fs::remove_file(root.join(STYLE_FILE)).unwrap();
        let mut lib = Library::new(&root, None);
        assert_eq!(err(&mut lib, ModelSpec::new("recipes/block.json")).code, "style_not_found");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn edited_recipes_rebuild_and_broken_edits_keep_the_last_good_model() {
        let root = scratch("reload", &[("block", &block(1.0))]);
        let file = root.join("recipes/block.json");
        let shared = Arc::new(SharedLibrary::new(Library::new(&root, None)));
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let id = world.spawn(SpawnSpec::model(ModelSpec::new("recipes/block.json")), "t").unwrap().id;
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 1.9);
        assert!(shared.lock().poll(false).is_none(), "polled at most once a second");
        // Edit: taller block. (The modification time is set explicitly: some file systems
        // only keep whole seconds.)
        let touch = |text: &str, secs: u64| {
            std::fs::write(&file, text).unwrap();
            let f = std::fs::File::options().write(true).open(&file).unwrap();
            f.set_modified(SystemTime::now() + Duration::from_secs(secs)).unwrap();
        };
        touch(&block(2.0), 5);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!((report.files_changed > 0, report.rebuilt.len(), report.errors.len()), (true, 1, 0));
        assert!(world.refresh_models().is_empty());
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 2.9);
        // Break it: the error is reported and the last good build stays.
        touch("{\"recipe\": 1, \"parts\": 7}", 10);
        let report = shared.lock().poll(true).unwrap();
        assert_eq!((report.rebuilt.len(), report.errors.len()), (0, 1));
        let handle = world.get(id).unwrap().resolved_model().unwrap().handle;
        assert!(shared.lock().handle_error(handle).is_some());
        assert_eq!(world.get(id).unwrap().local_bounds().unwrap().1.y, 2.9);
        // A spawn of the same spec still works (last good build), a new spec is refused.
        world.spawn(SpawnSpec::model(ModelSpec::new("recipes/block.json")), "t").unwrap();
        let up = UpdateSpec { params: Some(vec![("tier".into(), 2.0)]), ..Default::default() };
        assert_eq!(world.update(&EntityKey::Id(id), up, "t").err().unwrap().code, "recipe_invalid");
        let _ = std::fs::remove_dir_all(&root);
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
}
