//! The engine side of effects and decals (docs/design/VFX.md): the effect library (built-ins and
//! the world's own), the `vfx` and `decal` components turned into a frame's particles, materials,
//! decals and lights within the `[vfx]` budgets, world-space emitter tracks, the GPU textures
//! effects use, and what sight packets and `vfx_info` report.
//!
//! **Never simulation.** Nothing here writes the world: `world_hash` is the same with effects on
//! or off (`--vfx off`). Particles are evaluated in closed form (`specula_scintilla::eval`) at the
//! frame's time, so a capture at a given `time_s` shows the same particles every time.
//!
//! **No allocation per frame once warm:** every buffer is kept and reused; the frame's output
//! buffers are taken by the renderer and handed back ([`Vfx::take_frame`], [`Vfx::put_frame`]).

use crate::sight::in_frame;
use crate::world::{Entity, World};
use specula_core::math::{Mat4, Vec3, Vec4};
use specula_core::Json;
use specula_gpu::{Gpu, PointLight, TextureFormat, TextureId, TextureSampler, TextureWrap, MAX_POINT_LIGHTS};
use specula_scintilla::budget::{allocate, lod_keep, Candidate, Share};
use specula_scintilla::effect::{Effect, TextureRef};
use specula_scintilla::eval::{self, EmitterPlan, EvalStats, Instance, Placement, Track};
use specula_scintilla::{library, textures as builtin_textures, DecalMode, DecalSpec, Params, VfxError, VfxSpec};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::SystemTime;

/// Tag of the entities that hold the world's own effects (`props.vfx_effect`).
pub const EFFECT_TAG: &str = "vfx_effect";
/// Their props key.
pub const EFFECT_PROP: &str = "vfx_effect";
/// The group `vfx_set` writes effects under.
pub const LIBRARY_GROUP: &str = "vfx";
/// World effects the library reads (by path); more are reported and left out.
pub const MAX_WORLD_EFFECTS: usize = 256;
/// Effect and decal entries a sight packet lists (the rest are counted).
pub const MAX_PACKET_ITEMS: usize = 32;
/// Largest side of an effect texture loaded from the assets folder.
pub const MAX_ASSET_TEXTURE_SIDE: u32 = 2048;
/// Problems one frame keeps (deduplicated).
const MAX_PROBLEMS: usize = 8;

/// A component's error as tool text: `'field': message (allowed: ...)`, so `argument_error`
/// finds the field.
pub fn error_text(e: &VfxError) -> String {
    let mut s = format!("'{}': {}", e.field, e.message);
    if let Some(a) = &e.allowed {
        s.push_str(&format!(" (allowed: {a})"));
    }
    // A safety refusal keeps its own code (`argument_error` reads the marker).
    if e.code == "photosensitive" {
        s.push_str(" [error: photosensitive]");
    }
    s
}

/// `vfx`: an effect name, `{effect, params?, start?}`, or `false` to remove it.
pub fn vfx_arg(args: &Json) -> Result<Option<Option<VfxSpec>>, String> {
    match args.get("vfx") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(false)) => Ok(Some(None)),
        Some(v) => VfxSpec::parse(v).map(|s| Some(Some(s))).map_err(|e| error_text(&e)),
    }
}

/// `decal`: a texture name, `{texture, size, ...}`, or `false` to remove it.
pub fn decal_arg(args: &Json) -> Result<Option<Option<DecalSpec>>, String> {
    match args.get("decal") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(false)) => Ok(Some(None)),
        Some(v) => DecalSpec::parse(v).map(|s| Some(Some(s))).map_err(|e| error_text(&e)),
    }
}

// ---------------------------------------------------------------------------------------------
// Budgets: perf.toml [vfx]

const ADAPTER_KINDS: [&str; 5] = ["integrated", "discrete", "virtual", "cpu", "other"];
const BUDGET_KEYS: [&str; 8] = ["max_particles", "max_work", "max_emitters", "max_decals", "fill_cap", "decal_fill_cap", "max_per_effect", "lights"];

/// What effects may cost per frame (perf.toml `[vfx]`, overridable per adapter kind).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// Particles drawn per frame, all effects together.
    pub max_particles: u32,
    /// CPU work evaluating them may take per frame, in work units (about 10-20 ns each on the
    /// reference laptop; a plain particle costs 11, with noise 26, `stick` 29, both 179:
    /// `specula_scintilla::eval::work_per_particle`). Over it, the dearest and farthest thin.
    pub max_work: f32,
    /// Emitters evaluated per frame (nearest first).
    pub max_emitters: u32,
    /// Decals drawn per frame (nearest first).
    pub max_decals: u32,
    /// Estimated particle pixels per frame, in screens.
    pub fill_cap: f32,
    /// Screen area decal boxes may cover per frame, in screens, `lit` decals counting double
    /// (each covered pixel reconstructs the surface; a lit one also shades it with every light;
    /// the nearest decals win). About 2.7 ms per lit screen on the iGPU at 720p.
    pub decal_fill_cap: f32,
    /// What `vfx_check` allows one instance at its peak.
    pub max_per_effect: f32,
    /// Point lights effects may add (the frame's 16 are shared with the world's).
    pub lights: u32,
}

impl Budget {
    /// The defaults for an adapter kind: the iGPU (the reference) gets less.
    pub fn defaults(kind: &str) -> Budget {
        let low = kind != "discrete";
        Budget {
            max_particles: if low { 4000 } else { 6000 },
            max_work: if low { 100_000.0 } else { 160_000.0 },
            max_emitters: 512,
            max_decals: 256,
            fill_cap: if low { 1.5 } else { 3.0 },
            decal_fill_cap: if low { 1.0 } else { 3.0 },
            max_per_effect: 1500.0,
            lights: 4,
        }
    }

    fn apply(&mut self, table: &Json, section: &str) -> Result<(), String> {
        for (k, v) in table.as_object().ok_or(format!("[{section}] must be a table"))? {
            let int = |lo: u64, hi: u64| v.as_u64().filter(|n| (lo..=hi).contains(n)).ok_or(format!("[{section}] {k} must be a whole number from {lo} to {hi}"));
            let num = |lo: f64, hi: f64| v.as_f64().filter(|n| n.is_finite() && (lo..=hi).contains(n)).ok_or(format!("[{section}] {k} must be a number from {lo} to {hi}"));
            match k.as_str() {
                "max_particles" => self.max_particles = int(0, specula_gpu::MAX_PARTICLES as u64)? as u32,
                "max_work" => self.max_work = num(0.0, 1.0e8)? as f32,
                "max_emitters" => self.max_emitters = int(0, 65_536)? as u32,
                "max_decals" => self.max_decals = int(0, specula_gpu::MAX_DECALS as u64)? as u32,
                "fill_cap" => self.fill_cap = num(0.0, 100.0)? as f32,
                "decal_fill_cap" => self.decal_fill_cap = num(0.0, 100.0)? as f32,
                "max_per_effect" => self.max_per_effect = num(1.0, 1.0e6)? as f32,
                "lights" => self.lights = int(0, MAX_POINT_LIGHTS as u64)? as u32,
                other => return Err(format!("unknown key '{other}' in [{section}] (known: {})", BUDGET_KEYS.join(", "))),
            }
        }
        Ok(())
    }

    pub fn to_json(self) -> Json {
        Json::obj([
            ("max_particles", Json::from(self.max_particles)),
            ("max_work", Json::from(self.max_work)),
            ("max_emitters", Json::from(self.max_emitters)),
            ("max_decals", Json::from(self.max_decals)),
            ("fill_cap", Json::from(self.fill_cap)),
            ("decal_fill_cap", Json::from(self.decal_fill_cap)),
            ("max_per_effect", Json::from(self.max_per_effect)),
            ("lights", Json::from(self.lights)),
        ])
    }
}

/// perf.toml `[vfx]` as read: the base table and the adapter sections.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VfxTable {
    base: Vec<(String, Json)>,
    adapters: Vec<(String, Json)>,
}

impl VfxTable {
    pub fn parse(v: &Json) -> Result<VfxTable, String> {
        let mut t = VfxTable::default();
        for (k, x) in v.as_object().ok_or("[vfx] must be a table")? {
            if ADAPTER_KINDS.contains(&k.as_str()) {
                if x.as_object().is_none() {
                    return Err(format!("[vfx.{k}] must be a table"));
                }
                t.adapters.push((k.clone(), x.clone()));
            } else {
                t.base.push((k.clone(), x.clone()));
            }
        }
        for kind in ADAPTER_KINDS {
            t.try_budget(kind)?;
        }
        Ok(t)
    }

    fn try_budget(&self, kind: &str) -> Result<Budget, String> {
        let mut b = Budget::defaults(kind);
        b.apply(&Json::Obj(self.base.clone()), "vfx")?;
        if let Some((_, t)) = self.adapters.iter().find(|(k, _)| k == kind) {
            b.apply(t, &format!("vfx.{kind}"))?;
        }
        Ok(b)
    }

    /// The budget for an adapter kind.
    pub fn budget(&self, kind: &str) -> Budget {
        self.try_budget(kind).unwrap_or_else(|_| Budget::defaults(kind))
    }
}

// ---------------------------------------------------------------------------------------------
// The library

/// A world effect: a child of `/vfx` tagged [`EFFECT_TAG`] holding the effect in
/// `props.vfx_effect` (only there: a claim on `/vfx` covers the whole library).
#[derive(Clone, Debug)]
pub struct WorldEffect {
    pub effect: Arc<Effect>,
    pub id: u64,
    pub path: String,
    pub version: u64,
}

/// Built-in effects and the world's own, read again only when an effect entity changed.
#[derive(Default)]
pub struct Library {
    seen: Option<(u64, u64)>,
    /// Effect entities that failed to parse, by id: `(version, why)` (not parsed again until
    /// they change).
    failed: BTreeMap<u64, (u64, String)>,
    ids: BTreeSet<u64>,
    pub effects: BTreeMap<String, WorldEffect>,
    /// Effect entities left out, and why.
    pub problems: Vec<String>,
    /// Bumped whenever the world's effects change.
    pub revision: u64,
}

/// The effect a world entity holds, or why it holds none.
pub fn effect_of(e: &Entity) -> Result<Effect, String> {
    let v = e.props.get(EFFECT_PROP).ok_or_else(|| format!("'{}' is tagged {EFFECT_TAG} but has no props.{EFFECT_PROP}", e.name))?;
    Effect::parse(v).map_err(|err| format!("props.{EFFECT_PROP}.{}", error_text(&err).trim_start_matches('\'')))
}

impl Library {
    /// Reads the world's effects again if an effect entity changed since the last call.
    pub fn refresh(&mut self, world: &World, generation: u64) {
        let epoch = world.draw_epoch();
        if self.seen == Some((generation, epoch)) {
            return;
        }
        let stale = match self.seen.filter(|(g, _)| *g == generation).and_then(|(_, e)| world.draw_changes_since(e)) {
            None => true,
            Some(mut changed) => changed.any(|id| self.ids.contains(&id) || world.get(id).is_some_and(|x| x.tags.contains(EFFECT_TAG))),
        };
        self.seen = Some((generation, epoch));
        if !stale {
            return;
        }
        let root = world.find_path(&format!("/{LIBRARY_GROUP}")).ok();
        let tagged: Vec<&Entity> = world.persistent_entities().filter(|e| e.tags.contains(EFFECT_TAG)).collect();
        self.ids = tagged.iter().map(|e| e.id).chain(root).collect();
        let mut problems = Vec::new();
        // Only the library's children are effects: an effect tagged anywhere else can't shadow
        // one (and claims on /vfx cover every effect).
        let outside: Vec<&&Entity> = tagged.iter().filter(|e| root.is_none() || e.parent != root).collect();
        for e in outside.iter().take(4) {
            problems.push(format!("{}: tagged {EFFECT_TAG} outside /{LIBRARY_GROUP} (ignored: world effects are the children of /{LIBRARY_GROUP}; vfx_set makes them)", world.path_of(e.id)));
        }
        if outside.len() > 4 {
            problems.push(format!("{} more entities tagged {EFFECT_TAG} outside /{LIBRARY_GROUP} (ignored)", outside.len() - 4));
        }
        let mut ents: Vec<(String, &Entity)> = tagged.iter().filter(|e| root.is_some() && e.parent == root).map(|e| (world.path_of(e.id), *e)).collect();
        ents.sort_by(|a, b| a.0.cmp(&b.0));
        if ents.len() > MAX_WORLD_EFFECTS {
            problems.push(format!("{} world effects; only the first {MAX_WORLD_EFFECTS} by path are used", ents.len()));
            ents.truncate(MAX_WORLD_EFFECTS);
        }
        let mut next: BTreeMap<String, WorldEffect> = BTreeMap::new();
        for (path, e) in ents {
            if library::builtin(&e.name).is_some() {
                problems.push(format!("{path}: '{}' is a built-in effect's name (world effects can't replace built-ins; rename it)", e.name));
                continue;
            }
            if next.contains_key(&e.name) {
                problems.push(format!("{path}: a second effect named '{}' (the first by path is used)", e.name));
                continue;
            }
            // Unchanged entities keep their parsed effect (its Arc) or their failure: no parse.
            if let Some(old) = self.effects.get(&e.name).filter(|o| o.id == e.id && o.version == e.version) {
                next.insert(e.name.clone(), WorldEffect { effect: Arc::clone(&old.effect), id: e.id, path, version: e.version });
                continue;
            }
            if let Some((_, m)) = self.failed.get(&e.id).filter(|(v, _)| *v == e.version) {
                problems.push(format!("{path}: {m}"));
                continue;
            }
            match effect_of(e) {
                Ok(effect) => {
                    self.failed.remove(&e.id);
                    next.insert(e.name.clone(), WorldEffect { effect: Arc::new(effect), id: e.id, path, version: e.version });
                }
                Err(m) => {
                    problems.push(format!("{path}: {m}"));
                    self.failed.insert(e.id, (e.version, m));
                }
            }
        }
        let changed = next.len() != self.effects.len() || next.iter().zip(&self.effects).any(|((a, x), (b, y))| a != b || x.id != y.id || x.version != y.version);
        if changed {
            self.revision += 1;
        }
        self.effects = next;
        self.problems = problems;
        let live: BTreeSet<u64> = self.ids.clone();
        self.failed.retain(|id, _| live.contains(id));
    }

    /// An effect by name: a built-in, or the world's.
    pub fn get(&self, name: &str) -> Option<&Arc<Effect>> {
        library::builtin(name).or_else(|| self.effects.get(name).map(|w| &w.effect))
    }

    /// Every effect name, built-ins first, then the world's (by name).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        library::BUILTIN.iter().map(|(n, _)| *n).chain(self.effects.keys().map(String::as_str))
    }

    pub fn len(&self) -> usize {
        library::BUILTIN.len() + self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------------------------
// The write policy's effect rules

/// Whether `path` is the library group or inside it.
fn under_library(path: &str) -> bool {
    path.strip_prefix('/').and_then(|p| p.strip_prefix(LIBRARY_GROUP)).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether a change touches the effect library, which only vfx_set writes
/// ([`crate::write_policy::Rule::Vfx`]): anything in `/vfx` (made, changed, moved in or out,
/// removed), or an entity given the `vfx_effect` tag anywhere (the tag marks a world effect, and
/// world effects are `/vfx`'s children, so a claim on `/vfx` covers them all). Removing an entity
/// tagged outside `/vfx` (never an effect: a loaded world's leftovers) is anyone's.
pub fn touches_library(change: &crate::world::Change) -> bool {
    use crate::world::Change;
    let tagged = |e: &Entity| e.tags.contains(EFFECT_TAG);
    match change {
        Change::Spawn { path, after } => under_library(path) || tagged(after),
        Change::Set { path, prev_path, before, after } => {
            under_library(path) || prev_path.as_deref().is_some_and(under_library) || (tagged(after) && !before.as_deref().is_some_and(tagged))
        }
        Change::Delete { path, .. } | Change::Pose { path, .. } => under_library(path),
        Change::Env { .. } | Change::Camera { .. } | Change::File { .. } => false,
    }
}

/// Distinct (effect, params) flash checks one write may ask for (each up to about half a
/// millisecond); a write past it is refused (`too_many_effect_params`).
pub const MAX_WRITE_FLASH_CHECKS: usize = 64;

/// **The effect rules of the write policy**, on the changes of every checked writer (agents'
/// commits under every [`crate::write_policy::Rule`], what agents ask the hosted game, a module
/// or a live Lex run to do, and modules' own requests), against the world as written:
///
/// - an effect instance (a `vfx` component) made or changed must name an effect there is (a
///   built-in, or a child of `/vfx` that parses), else `unknown_effect`;
/// - played with its params (the seed left out), it must stay within 3 flashes a second
///   (WCAG 2.3.1), else `photosensitive`;
/// - an effect the library gains or changes must stay so for every instance already playing it
///   with params, else `photosensitive` (naming them).
///
/// Writes the policy exempts (the hosted game's own calls, ticks, undo, loads) aren't refused
/// here: evaluation leaves out whatever would still flash ([`FlashCache`], reported as
/// `photosensitive`), so nothing that flashes is ever drawn whichever path wrote it.
pub fn write_rules(world: &World, changes: &[crate::world::Change]) -> Result<(), crate::world::WorldError> {
    use crate::world::{Change, WorldError};
    let none = params_key(&Params::default());
    let root = world.find_path(&format!("/{LIBRARY_GROUP}")).ok();
    // Effects looked up once per write: a built-in, or the library's child of that name.
    let mut found: Vec<(String, Option<Arc<Effect>>)> = Vec::new();
    let mut lookup = |name: &str| -> Option<Arc<Effect>> {
        if let Some(e) = library::builtin(name) {
            return Some(Arc::clone(e));
        }
        if let Some((_, e)) = found.iter().find(|(n, _)| n == name) {
            return e.clone();
        }
        let e = world
            .find_path(&format!("/{LIBRARY_GROUP}/{name}"))
            .ok()
            .and_then(|id| world.get(id))
            .filter(|e| root.is_some() && e.parent == root && e.tags.contains(EFFECT_TAG))
            .and_then(|e| effect_of(e).ok())
            .map(Arc::new);
        found.push((name.to_owned(), e.clone()));
        e
    };
    let mut checked: Vec<(String, [u32; 9])> = Vec::new();
    let mut flashes = |name: &str, effect: &Effect, params: &Params| -> Result<bool, WorldError> {
        let key = params_key(params);
        if key == none || checked.iter().any(|(n, k)| n == name && *k == key) {
            return Ok(false);
        }
        if checked.len() >= MAX_WRITE_FLASH_CHECKS {
            return Err(WorldError::new("too_many_effect_params", format!("this write plays effects with more than {MAX_WRITE_FLASH_CHECKS} different params, each checked for flashing: split it")));
        }
        checked.push((name.to_owned(), key));
        Ok(specula_scintilla::flash::flash(effect, params).hz > specula_scintilla::effect::MAX_FLASH_HZ)
    };
    for c in changes {
        let (path, after, before) = match c {
            Change::Spawn { path, after } => (path, after, None),
            Change::Set { path, before, after, .. } => (path, after, before.as_deref()),
            _ => continue,
        };
        // An instance made or changed.
        if let Some(spec) = after.vfx.as_ref().filter(|s| before.is_none_or(|b| b.vfx.as_ref() != Some(*s))) {
            let Some(effect) = lookup(&spec.effect) else {
                let mut names: Vec<&str> = library::BUILTIN.iter().map(|(n, _)| *n).collect();
                let world_names: Vec<String> = root
                    .map(|r| world.persistent_entities().filter(|e| e.parent == Some(r) && e.tags.contains(EFFECT_TAG)).take(32).map(|e| e.name.clone()).collect())
                    .unwrap_or_default();
                names.extend(world_names.iter().map(String::as_str));
                return Err(WorldError::new("unknown_effect", format!("'vfx.effect': no effect named '{}' (built-ins and the world's: {}; vfx_set adds one)", spec.effect, names.join(", ")))
                    .with("path", path.clone())
                    .with("field", "vfx.effect"));
            };
            if flashes(&spec.effect, &effect, &spec.params)? {
                return Err(WorldError::new(
                    "photosensitive",
                    format!(
                        "'{}' played with these params flashes more than {} times a second: that can trigger photosensitive seizures (WCAG 2.3.1); lower params.rate, size, alpha or intensity",
                        spec.effect,
                        specula_scintilla::effect::MAX_FLASH_HZ
                    ),
                )
                .with("path", path.clone())
                .with("field", "vfx.params"));
            }
        }
        // A library effect gained or changed: every instance already playing it with params.
        let library_effect = under_library(path) && after.tags.contains(EFFECT_TAG) && root.is_some() && after.parent == root;
        if library_effect && before.is_none_or(|b| b.props.get(EFFECT_PROP) != after.props.get(EFFECT_PROP) || b.name != after.name) {
            let Ok(effect) = effect_of(after) else { continue };
            let mut playing: Vec<String> = Vec::new();
            for x in world.fx_entities() {
                let Some(v) = x.vfx.as_ref().filter(|v| v.effect == after.name) else { continue };
                if flashes(&after.name, &effect, &v.params)? {
                    playing.push(world.path_of(x.id));
                }
            }
            if !playing.is_empty() {
                return Err(WorldError::new(
                    "photosensitive",
                    format!(
                        "'{}' changed this way would flash more than 3 times a second where it plays with these params: {} (WCAG 2.3.1); change those instances' params first, or keep the effect calmer",
                        after.name,
                        playing.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
                    ),
                )
                .with("path", path.clone()));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Photosensitivity at evaluation time

/// Verdicts of `flash` for an effect played with params (the effect itself passed when parsed):
/// whatever set the params (a spawn before `vfx_set` changed the effect, a loaded world, an undo,
/// game code), an instance that would flash isn't drawn.
#[derive(Default)]
pub struct FlashCache {
    revision: u64,
    verdicts: BTreeMap<(usize, [u32; 9]), bool>,
}

/// Params as bits, the seed left out (it never changes how often an effect flashes).
pub fn params_key(p: &Params) -> [u32; 9] {
    [p.rate.to_bits(), p.size.to_bits(), p.speed.to_bits(), p.alpha.to_bits(), p.intensity.to_bits(), p.tint[0].to_bits(), p.tint[1].to_bits(), p.tint[2].to_bits(), u32::from(p.light)]
}

impl FlashCache {
    /// Whether `effect` played with `params` stays within 3 flashes a second.
    pub fn ok(&mut self, effect: &Arc<Effect>, params: &Params, revision: u64) -> bool {
        let key = params_key(params);
        if key == params_key(&Params::default()) {
            return true;
        }
        if revision != self.revision {
            self.verdicts.clear();
            self.revision = revision;
        }
        let k = (Arc::as_ptr(effect) as usize, key);
        if let Some(v) = self.verdicts.get(&k) {
            return *v;
        }
        if self.verdicts.len() >= 1024 {
            self.verdicts.clear();
        }
        let ok = specula_scintilla::flash::flash(effect, params).hz <= specula_scintilla::effect::MAX_FLASH_HZ;
        self.verdicts.insert(k, ok);
        ok
    }
}

// ---------------------------------------------------------------------------------------------
// Textures

/// A loader job: the asset path, linear (normal maps) or sRGB, and the file.
type Job = (String, bool, PathBuf);
/// A loader result: the asset path, linear, the file's modification time and the image.
type Loaded = (String, bool, Option<SystemTime>, Result<specula_core::png::Image, String>);
/// The loader thread's job and result channels.
type Loader = (Sender<Job>, Receiver<Loaded>);

enum TexState {
    Ready(TextureId),
    /// Loading (again, when the file changed: the old texture draws meanwhile).
    Loading(Option<TextureId>),
    Failed(String),
    /// Over the asset texture budget (not retried until it is freed and asked for again).
    Refused(String),
}

/// Distinct asset textures effects and decals may hold at once (each takes a bindless slot).
pub const MAX_ASSET_TEXTURES: usize = 64;
/// GPU bytes they may take together (with mips).
pub const MAX_ASSET_BYTES: u64 = 256 << 20;

struct TexEntry {
    /// GPU bytes (asset textures, with mips).
    bytes: u64,
    state: TexState,
    /// The file's modification time when it was read (asset PNGs).
    stamp: Option<SystemTime>,
    /// [`Textures::scans`] when it was last asked for (or named by the world).
    used: u64,
}

/// Asset textures neither drawn nor named by the world for this many scans (about a second
/// apart) are freed: a texture a world effect or decal names is never freed.
const EVICT_AFTER: u64 = 10;
/// How often window frames look for changed asset PNGs (captures always look first).
const RESCAN: std::time::Duration = std::time::Duration::from_secs(1);

/// The GPU textures effects and decals sample: built-ins made on first use (synchronously:
/// small, procedural), PNGs from the assets folder decoded on a loader thread and queued.
/// Asset PNGs are keyed with their colour space (normal maps are linear), reloaded when the file
/// changes (a failed one is retried), and freed when nothing has used them for a while.
pub struct Textures {
    assets: PathBuf,
    /// Keyed by the file's canonical path under the assets folder (so `FX/a.png` and `fx/A.png`
    /// naming one file on Windows share one texture) and the colour space.
    entries: BTreeMap<(TextureRef, bool), TexEntry>,
    /// Asset paths as written → their canonical key (cleared each scan).
    aliases: BTreeMap<String, String>,
    loader: Option<Loader>,
    scans: u64,
    scanned: Option<std::time::Instant>,
}

impl Textures {
    fn new(assets: &Path) -> Textures {
        Textures { assets: assets.to_path_buf(), entries: BTreeMap::new(), aliases: BTreeMap::new(), loader: None, scans: 0, scanned: None }
    }

    /// The colour texture, if it is ready; asset PNGs start loading on first ask.
    pub fn get(&mut self, gpu: &mut Gpu, r: &TextureRef) -> Option<TextureId> {
        self.get_as(gpu, r, false)
    }

    /// A normal map (linear data, never sRGB-decoded), if it is ready.
    pub fn get_linear(&mut self, gpu: &mut Gpu, r: &TextureRef) -> Option<TextureId> {
        self.get_as(gpu, r, true)
    }

    /// The canonical key of an asset path: the file's real path under the assets folder ('/'
    /// separators; case-folded on Windows, whose file names ignore case).
    fn canon(&mut self, path: &str) -> String {
        if let Some(c) = self.aliases.get(path) {
            return c.clone();
        }
        let fold = |s: String| if cfg!(windows) { s.to_lowercase() } else { s };
        let real = std::fs::canonicalize(self.assets.join(path))
            .ok()
            .and_then(|f| std::fs::canonicalize(&self.assets).ok().and_then(|a| f.strip_prefix(a).ok().map(|r| r.to_string_lossy().replace('\\', "/"))));
        let c = fold(real.unwrap_or_else(|| path.to_owned()));
        if self.aliases.len() >= 4 * MAX_ASSET_TEXTURES {
            self.aliases.clear();
        }
        self.aliases.insert(path.to_owned(), c.clone());
        c
    }

    fn key(&mut self, r: &TextureRef, linear: bool) -> (TextureRef, bool) {
        match r {
            // Built-ins carry their own colour space.
            TextureRef::Builtin(_) => (r.clone(), false),
            TextureRef::Asset(p) => (TextureRef::Asset(self.canon(p)), linear),
        }
    }

    /// [`Textures::key`] without resolving a path not seen yet.
    fn key_ref(&self, r: &TextureRef, linear: bool) -> (TextureRef, bool) {
        match r {
            TextureRef::Builtin(_) => (r.clone(), false),
            TextureRef::Asset(p) => (TextureRef::Asset(self.aliases.get(p).cloned().unwrap_or_else(|| p.clone())), linear),
        }
    }

    /// Whether another asset texture of `bytes` fits the budget (`None`: just counting).
    fn admit(&self, except: &(TextureRef, bool), bytes: Option<u64>) -> Result<(), String> {
        let live = self.entries.iter().filter(|(k, e)| *k != except && matches!(k.0, TextureRef::Asset(_)) && matches!(e.state, TexState::Ready(_) | TexState::Loading(_)));
        let (count, used) = live.fold((0usize, 0u64), |(n, b), (_, e)| (n + 1, b + e.bytes));
        if count >= MAX_ASSET_TEXTURES {
            return Err(format!("texture_budget: {} distinct asset textures are loaded (at most {MAX_ASSET_TEXTURES}); reuse textures or free some", count));
        }
        if let Some(b) = bytes {
            if used + b > MAX_ASSET_BYTES {
                return Err(format!("texture_budget: {} MiB of asset textures with this one (at most {} MiB); use smaller PNGs", (used + b) >> 20, MAX_ASSET_BYTES >> 20));
            }
        }
        Ok(())
    }

    fn get_as(&mut self, gpu: &mut Gpu, r: &TextureRef, linear: bool) -> Option<TextureId> {
        let frame = self.scans;
        let key = self.key(r, linear);
        if let Some(e) = self.entries.get_mut(&key) {
            e.used = frame;
            return match e.state {
                TexState::Ready(id) | TexState::Loading(Some(id)) => Some(id),
                _ => None,
            };
        }
        match r {
            TextureRef::Builtin(name) => {
                let state = match builtin_textures::builtin(name) {
                    None => TexState::Failed(format!("no built-in texture '{name}'")),
                    Some(img) => {
                        let format = if img.srgb { TextureFormat::Rgba8Srgb } else { TextureFormat::Rgba8Unorm };
                        match gpu.upload_material_texture(img.width, img.height, format, &img.pixels, sampler()) {
                            Ok(id) => TexState::Ready(id),
                            Err(e) => TexState::Failed(format!("texture {name}: {e}")),
                        }
                    }
                };
                let id = match &state {
                    TexState::Ready(id) => Some(*id),
                    _ => None,
                };
                self.entries.insert(key, TexEntry { bytes: 0, state, stamp: None, used: frame });
                id
            }
            TextureRef::Asset(_) => {
                let TextureRef::Asset(path) = &key.0 else { return None };
                let state = match self.admit(&key, None) {
                    Ok(()) => self.send(&path.clone(), linear),
                    Err(m) => TexState::Refused(format!("texture {}: {m}", r.name())),
                };
                self.entries.insert(key, TexEntry { bytes: 0, state, stamp: None, used: frame });
                None
            }
        }
    }

    fn send(&mut self, path: &str, linear: bool) -> TexState {
        let file = self.assets.join(path);
        if self.loader().0.send((path.to_string(), linear, file)).is_ok() {
            TexState::Loading(None)
        } else {
            TexState::Failed(format!("texture {path}: the loader thread is not running"))
        }
    }

    /// The texture now, loading an asset PNG on this thread if it isn't ready (tool previews:
    /// the same call gives the same images, whatever the loader thread is doing).
    pub fn ensure(&mut self, gpu: &mut Gpu, r: &TextureRef) -> Option<TextureId> {
        if !matches!(r, TextureRef::Asset(_)) {
            return self.get(gpu, r);
        }
        let key = self.key(r, false);
        let TextureRef::Asset(path) = key.0.clone() else { return None };
        let path = path.as_str();
        let stamp = file_stamp(&self.assets.join(path));
        if let Some(e) = self.entries.get_mut(&key) {
            e.used = self.scans;
            if let TexState::Ready(id) = e.state {
                if e.stamp == stamp {
                    return Some(id);
                }
            }
        }
        let (stamp, loaded) = read_png(path, &self.assets.join(path));
        let loaded = loaded.and_then(|img| self.admit(&key, Some(texture_bytes(&img))).map(|()| img));
        let state = upload(gpu, path, false, stamp, loaded, true);
        let id = match state.0 {
            TexState::Ready(id) => Some(id),
            _ => None,
        };
        self.replace(gpu, key, state);
        id
    }

    /// Puts a load's result in place, freeing the texture it replaces.
    fn replace(&mut self, gpu: &mut Gpu, key: (TextureRef, bool), (state, stamp, bytes): (TexState, Option<SystemTime>, u64)) {
        let used = self.entries.get(&key).map_or(self.scans, |e| e.used);
        let old = self.entries.insert(key, TexEntry { bytes, state, stamp, used });
        if let Some(TexEntry { state: TexState::Ready(id) | TexState::Loading(Some(id)), .. }) = old {
            gpu.destroy_texture(id);
        }
    }

    fn loader(&mut self) -> &Loader {
        self.loader.get_or_insert_with(|| {
            let (job_tx, job_rx) = std::sync::mpsc::channel::<Job>();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name("specula-vfx-loader".into()).spawn(move || {
                for (key, linear, file) in job_rx {
                    let (stamp, result) = read_png(&key, &file);
                    if done_tx.send((key, linear, stamp, result)).is_err() {
                        break;
                    }
                }
            });
            if let Err(e) = spawned {
                eprintln!("specula: vfx loader thread: {e}");
            }
            (job_tx, done_rx)
        })
    }

    /// Uploads finished loads (queued: never waits for the GPU). Returns whether a scan is due
    /// (`fresh`: now; else at most every second): the caller marks what the world names
    /// ([`Textures::touch`]), then calls [`Textures::rescan`].
    pub fn pump(&mut self, gpu: &mut Gpu, fresh: bool) -> bool {
        if let Some((_, rx)) = &self.loader {
            let done: Vec<Loaded> = rx.try_iter().collect();
            for (path, linear, stamp, result) in done {
                let key = (TextureRef::Asset(path), linear);
                // A synchronous load (`ensure`) may have got there first.
                if !matches!(self.entries.get(&key), Some(TexEntry { state: TexState::Loading(_), .. })) {
                    continue;
                }
                let result = result.and_then(|img| self.admit(&key, Some(texture_bytes(&img))).map(|()| img));
                let state = upload(gpu, key.0.name(), linear, stamp, result, false);
                self.replace(gpu, key, state);
            }
        }
        let due = fresh || self.scanned.is_none_or(|t| t.elapsed() >= RESCAN);
        if due {
            self.scanned = Some(std::time::Instant::now());
        }
        due
    }

    /// Marks a texture the world names as in use (it is never freed).
    pub fn touch(&mut self, r: &TextureRef, linear: bool) {
        let scans = self.scans;
        let key = self.key(r, linear);
        if let Some(e) = self.entries.get_mut(&key) {
            e.used = scans;
        }
    }

    /// Reloads changed asset PNGs and frees the ones nothing has used for a while.
    pub fn rescan(&mut self, gpu: &mut Gpu) {
        self.scans += 1;
        // Paths are resolved again (a file may have appeared, or been renamed).
        self.aliases.clear();
        let frame = self.scans;
        let stale: Vec<(TextureRef, bool)> = self
            .entries
            .iter()
            .filter(|((r, _), e)| matches!(r, TextureRef::Asset(_)) && frame.saturating_sub(e.used) > EVICT_AFTER)
            .map(|(k, _)| k.clone())
            .collect();
        for key in stale {
            if let Some(TexEntry { state: TexState::Ready(id) | TexState::Loading(Some(id)), .. }) = self.entries.remove(&key) {
                gpu.destroy_texture(id);
            }
        }
        let changed: Vec<(TextureRef, bool)> = self
            .entries
            .iter()
            .filter(|(_, e)| !matches!(e.state, TexState::Loading(_) | TexState::Refused(_)))
            .filter_map(|((r, linear), e)| match r {
                TextureRef::Asset(path) => (file_stamp(&self.assets.join(path)) != e.stamp).then(|| (r.clone(), *linear)),
                TextureRef::Builtin(_) => None,
            })
            .collect();
        // Refused for the budget: tried again once there is room.
        let refused: Vec<(TextureRef, bool)> = self.entries.iter().filter(|(_, e)| matches!(e.state, TexState::Refused(_))).map(|(k, _)| k.clone()).collect();
        for key in refused {
            if self.admit(&key, None).is_ok() {
                if let TextureRef::Asset(path) = &key.0 {
                    let state = self.send(&path.clone(), key.1);
                    if let Some(e) = self.entries.get_mut(&key) {
                        e.state = state;
                    }
                }
            }
        }
        for key in changed {
            let TextureRef::Asset(path) = &key.0 else { continue };
            let path = path.clone();
            let state = self.send(&path, key.1);
            if let Some(e) = self.entries.get_mut(&key) {
                e.state = match (std::mem::replace(&mut e.state, TexState::Failed(String::new())), state) {
                    (TexState::Ready(old), TexState::Loading(None)) => TexState::Loading(Some(old)),
                    (_, s) => s,
                };
            }
        }
    }

    /// Asset textures still loading.
    pub fn pending(&self) -> usize {
        self.entries.values().filter(|e| matches!(e.state, TexState::Loading(_))).count()
    }

    /// Why a texture isn't drawn (loading or failed), if it isn't.
    fn problem(&self, r: &TextureRef) -> Option<String> {
        let mut m = String::new();
        self.write_problem(r, &mut m).then_some(m)
    }

    /// Writes why a texture isn't drawn (`false`: it is, or isn't known).
    fn write_problem(&self, r: &TextureRef, out: &mut String) -> bool {
        use std::fmt::Write;
        match self.entries.get(&self.key_ref(r, false)).map(|e| &e.state) {
            None => {
                let _ = write!(out, "texture {}: unavailable", r.name());
            }
            Some(TexState::Ready(_) | TexState::Loading(Some(_))) => return false,
            Some(TexState::Loading(None)) => {
                let _ = write!(out, "texture {}: still loading", r.name());
            }
            Some(TexState::Failed(m) | TexState::Refused(m)) => out.push_str(m),
        }
        true
    }

    pub fn loaded(&self) -> usize {
        self.entries.values().filter(|e| matches!(e.state, TexState::Ready(_))).count()
    }
}

/// GPU bytes of an image with its mips.
fn texture_bytes(img: &specula_core::png::Image) -> u64 {
    u64::from(img.width) * u64::from(img.height) * 4 * 4 / 3
}

/// A decoded PNG uploaded (`now`: synchronously, else queued), with its file time and bytes.
fn upload(gpu: &mut Gpu, key: &str, linear: bool, stamp: Option<SystemTime>, loaded: Result<specula_core::png::Image, String>, now: bool) -> (TexState, Option<SystemTime>, u64) {
    let format = if linear { TextureFormat::Rgba8Unorm } else { TextureFormat::Rgba8Srgb };
    match loaded {
        Err(m) if m.starts_with("texture_budget") => (TexState::Refused(format!("texture {key}: {m}")), stamp, 0),
        Err(m) => (TexState::Failed(m), stamp, 0),
        Ok(img) => {
            let bytes = texture_bytes(&img);
            let levels = specula_gpu::generate_mips_owned(img.width, img.height, format, img.rgba);
            let id = levels.and_then(|l| {
                let refs: Vec<&[u8]> = l.iter().map(Vec::as_slice).collect();
                if now {
                    gpu.upload_texture_levels(img.width, img.height, format, &refs, sampler())
                } else {
                    gpu.queue_texture_levels(img.width, img.height, format, &refs, sampler())
                }
            });
            match id {
                Ok(id) => (TexState::Ready(id), stamp, bytes),
                Err(e) => (TexState::Failed(format!("texture {key}: {e}")), stamp, 0),
            }
        }
    }
}

fn file_stamp(file: &Path) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

/// Trilinear, clamped (particles and decals never wrap).
fn sampler() -> TextureSampler {
    TextureSampler { wrap_u: TextureWrap::ClampToEdge, wrap_v: TextureWrap::ClampToEdge, ..TextureSampler::default() }
}

/// The file's modification time (read first: a change after it is seen next scan) and its image.
fn read_png(key: &str, file: &Path) -> (Option<SystemTime>, Result<specula_core::png::Image, String>) {
    (file_stamp(file), decode_png(key, file))
}

fn decode_png(key: &str, file: &Path) -> Result<specula_core::png::Image, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("texture {key}: {e}"))?;
    if bytes.len() < 24 || bytes[..8] != *b"\x89PNG\r\n\x1a\n" {
        return Err(format!("texture {key}: not a PNG"));
    }
    let (w, h) = (u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]));
    if w > MAX_ASSET_TEXTURE_SIDE || h > MAX_ASSET_TEXTURE_SIDE {
        return Err(format!("texture {key}: {w}x{h} pixels; effect textures may be at most {MAX_ASSET_TEXTURE_SIDE} per side"));
    }
    specula_core::png::decode(&bytes).map_err(|e| format!("texture {key}: not a readable PNG ({e:?})"))
}

/// The grid a texture is laid out in when an emitter gives no flipbook (built-in sheets).
fn natural_grid(r: &TextureRef) -> [u32; 2] {
    match r {
        TextureRef::Builtin("smoke" | "flame") => [4, 4],
        _ => [1, 1],
    }
}

// ---------------------------------------------------------------------------------------------
// Frames

/// The view a frame's effects are built for.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub view_proj: Mat4,
    pub eye: Vec3,
    /// Unit view direction.
    pub forward: Vec3,
    /// Orthographic height in metres (`None`: perspective with `fov_y_deg`).
    pub ortho: Option<f32>,
    pub fov_y_deg: f32,
    pub width: u32,
    pub height: u32,
}

impl View {
    pub fn of(camera: &crate::sight::Camera, width: u32, height: u32) -> View {
        let ortho = match camera.projection {
            crate::sight::Projection::Ortho { height_m } => Some(height_m),
            crate::sight::Projection::Perspective => None,
        };
        View {
            view_proj: camera.view_proj(width, height),
            eye: camera.eye,
            forward: (camera.target - camera.eye).normalize(),
            ortho,
            fov_y_deg: camera.fov_y_deg,
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// Pixels per metre at `distance` metres.
    pub fn px_per_m(&self, distance: f32) -> f32 {
        match self.ortho {
            Some(h) => self.height as f32 / h.max(1e-3),
            None => {
                let half = (self.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
                self.height as f32 * 0.5 / (half * distance.max(0.05))
            }
        }
    }

    /// Whether any of the box can be in view (conservative).
    pub fn sees(&self, lo: Vec3, hi: Vec3) -> bool {
        let clip: [Vec4; 8] = std::array::from_fn(|i| {
            let p = Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
            self.view_proj.mul_vec4(p.extend(1.0))
        });
        let all = |f: &dyn Fn(&Vec4) -> bool| clip.iter().all(f);
        !(all(&|p| p.w <= 0.0) || all(&|p| p.x < -p.w) || all(&|p| p.x > p.w) || all(&|p| p.y < -p.w) || all(&|p| p.y > p.w))
    }

    /// The pixel rectangle `[x, y, w, h]` of a box (clipped to the image), if any of it is in front.
    pub fn screen_rect(&self, lo: Vec3, hi: Vec3) -> Option<[u32; 4]> {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut any = false;
        for i in 0..8 {
            let p = Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
            let c = self.view_proj.mul_vec4(p.extend(1.0));
            if c.w <= 1e-4 {
                // A corner behind the eye: the box may reach any edge.
                (x0, y0, x1, y1) = (0.0, 0.0, self.width as f32, self.height as f32);
                any = true;
                break;
            }
            let (sx, sy) = ((c.x / c.w * 0.5 + 0.5) * self.width as f32, (c.y / c.w * 0.5 + 0.5) * self.height as f32);
            (x0, y0, x1, y1) = (x0.min(sx), y0.min(sy), x1.max(sx), y1.max(sy));
            any = true;
        }
        if !any {
            return None;
        }
        let (w, h) = (self.width as f32, self.height as f32);
        let (x0, y0, x1, y1) = (x0.clamp(0.0, w), y0.clamp(0.0, h), x1.clamp(0.0, w), y1.clamp(0.0, h));
        (x1 > x0 && y1 > y0).then(|| [x0 as u32, y0 as u32, (x1 - x0).ceil() as u32, (y1 - y0).ceil() as u32])
    }
}

/// A frame's effects, owned by the caller while the frame renders (see [`Vfx::take_frame`]).
#[derive(Default)]
pub struct Frame {
    pub particles: Vec<specula_gpu::Particle>,
    pub materials: Vec<specula_gpu::ParticleMaterial>,
    pub decals: Vec<specula_gpu::Decal>,
}

impl Frame {
    pub fn effects(&self) -> specula_gpu::Effects<'_> {
        specula_gpu::Effects { particles: &self.particles, materials: &self.materials, decals: &self.decals }
    }

    fn clear(&mut self) {
        self.particles.clear();
        self.materials.clear();
        self.decals.clear();
    }
}

/// One effect instance drawn this frame (for sight packets and `vfx_info`).
#[derive(Clone, Copy, Debug)]
pub struct Shown {
    pub entity: u64,
    /// Particles drawn and alive (before thinning).
    pub drawn: u32,
    pub live: u32,
    /// Box of the drawn particles (`None`: none drawn).
    pub bounds: Option<(Vec3, Vec3)>,
    /// Estimated screen pixels its particles cover.
    pub fill_px: f32,
}

/// One decal drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct ShownDecal {
    pub entity: u64,
    pub lo: Vec3,
    pub hi: Vec3,
}

/// What a frame's effects did.
#[derive(Default)]
pub struct Report {
    /// Instances with a `vfx` in the frame (shown, on its layer), and how many were drawn.
    pub instances: u32,
    pub culled: u32,
    pub finished: u32,
    pub emitters: u32,
    pub emitters_over_cap: u32,
    pub expected: f32,
    pub stats: EvalStats,
    pub fill_px: f32,
    pub fill_px_before_cap: f32,
    pub fill_cap_px: f32,
    pub screen_px: f32,
    pub over_particles: bool,
    pub over_fill: bool,
    /// Instances not drawn: their params make the effect flash (photosensitivity).
    pub flashing: u32,
    /// Expected CPU work after the budget, and whether the work budget cut something.
    pub work_expected: f32,
    pub over_work: bool,
    /// Particles the fill cap removed after evaluation (what the estimate missed).
    pub fill_thinned: u32,
    pub decals: u32,
    pub decals_skipped: u32,
    /// Screen area the drawn decals' boxes cover, in screens.
    pub decal_fill: f32,
    pub lights: u32,
    pub lights_dropped: u32,
    pub cpu_ms: f64,
    pub problems: Vec<String>,
    pub shown: Vec<Shown>,
    pub shown_decals: Vec<ShownDecal>,
    /// The frame's time and view (packets project with it).
    pub time_s: f64,
    /// Last frame's problems (a problem that persists reuses its string) and a reused buffer.
    prev: Vec<String>,
    buf: String,
}

impl Report {
    fn reset(&mut self) {
        let (mut problems, mut shown, mut decals) = (std::mem::take(&mut self.problems), std::mem::take(&mut self.shown), std::mem::take(&mut self.shown_decals));
        let (mut prev, buf) = (std::mem::take(&mut self.prev), std::mem::take(&mut self.buf));
        prev.clear();
        std::mem::swap(&mut prev, &mut problems);
        shown.clear();
        decals.clear();
        *self = Report { problems, shown, shown_decals: decals, prev, buf, ..Report::default() };
    }

    /// Notes a problem written by `m` (no allocation while the same problem persists).
    fn problem(&mut self, m: impl FnOnce(&mut String)) {
        if self.problems.len() >= MAX_PROBLEMS {
            return;
        }
        self.buf.clear();
        m(&mut self.buf);
        if self.problems.contains(&self.buf) {
            return;
        }
        let kept = match self.prev.iter().position(|p| *p == self.buf) {
            Some(i) => self.prev.swap_remove(i),
            None => self.buf.clone(),
        };
        self.problems.push(kept);
    }

    /// Counts for perf_frame and vfx_info.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("instances", Json::from(self.instances)),
            ("culled", Json::from(self.culled)),
            ("finished", Json::from(self.finished)),
            ("emitters", Json::from(self.emitters)),
            ("emitters_over_cap", Json::from(self.emitters_over_cap)),
            ("particles", Json::obj([
                ("expected", Json::from(self.expected.round())),
                ("live", Json::from(self.stats.live)),
                ("drawn", Json::from(self.stats.drawn)),
                ("thinned", Json::from(self.stats.thinned)),
                ("dropped", Json::from(self.stats.dropped)),
                ("killed", Json::from(self.stats.killed)),
                ("offscreen", Json::from(self.stats.offscreen)),
                ("fill_thinned", Json::from(self.fill_thinned)),
            ])),
            ("flashing", Json::from(self.flashing)),
            ("work", Json::obj([("expected", Json::from(self.work_expected.round())), ("done", Json::from(self.stats.work)), ("over", Json::from(self.over_work))])),
            ("fill_screens", Json::from(round3(self.fill_px / self.screen_px.max(1.0)))),
            ("fill_px", Json::from(self.fill_px.round())),
            ("fill_cap_px", Json::from(self.fill_cap_px.round())),
            ("over_particles", Json::from(self.over_particles)),
            ("over_fill", Json::from(self.over_fill)),
            ("decals", Json::from(self.decals)),
            ("decals_skipped", Json::from(self.decals_skipped)),
            ("decal_fill_screens", Json::from(round3(self.decal_fill))),
            ("lights", Json::from(self.lights)),
            ("lights_dropped", Json::from(self.lights_dropped)),
            ("cpu_ms", Json::from(round3(self.cpu_ms as f32))),
            ("time_s", Json::from((self.time_s * 1000.0).round() / 1000.0)),
            ("problems", Json::arr(self.problems.iter().map(|p| Json::from(p.as_str())))),
        ])
    }
}

fn round3(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}

/// The anim ground grid as effects' collision ground.
struct GroundOf<'a>(&'a crate::anim::GroundGrid);

impl eval::Ground for GroundOf<'_> {
    fn height(&self, x: f32, z: f32, y: f32) -> Option<f32> {
        specula_anim::ik::Ground::height(self.0, x, z, y + 0.5, 50.0)
    }
}

/// One visible instance while a frame is built.
#[derive(Clone)]
struct Pending {
    entity: u64,
    effect: Arc<Effect>,
    params: specula_scintilla::Params,
    start: f64,
    seed: u32,
    placement: Placement,
    distance: f32,
    /// First candidate (emitter) of this instance in the frame's candidate list.
    first: usize,
}

/// Reused per-frame scratch.
#[derive(Default)]
struct Scratch {
    pending: Vec<Pending>,
    cands: Vec<Candidate>,
    shares: Vec<Share>,
    order: Vec<u32>,
    values: Vec<f32>,
    plans: Vec<EmitterPlan>,
    expected: Vec<f32>,
    cpu: Vec<eval::Particle>,
    /// Each shown instance's particles in `cpu`, and its distance.
    ranges: Vec<(usize, usize)>,
    pending_distance: Vec<f32>,
    /// `(hash, pixels)` of every evaluated particle when the fill cap cuts after evaluation, the
    /// order to consider them in, and which are kept.
    fill_order: Vec<(u32, f32)>,
    fill_rank: Vec<u32>,
    fill_keep: Vec<bool>,
    keys: Vec<u64>,
    /// `(distance, order, entity, decal, box lo, box hi, screen area in screens)`.
    decals: Vec<(f32, i32, u64, specula_gpu::Decal, Vec3, Vec3, f32)>,
    by_distance: Vec<(f32, u64)>,
    lights: Vec<(f32, PointLight)>,
    material_keys: Vec<(Option<TextureId>, [u32; 2], u32, bool)>,
}

/// The engine's effects: library, budgets, tracks, textures and the last frames' reports.
pub struct Vfx {
    /// `--vfx off` turns every effect and decal off (the simulation is the same either way).
    pub enabled: bool,
    pub budget: Budget,
    /// Params that make an effect flash, checked as instances are evaluated.
    pub flash_cache: FlashCache,
    pub library: Library,
    pub textures: Textures,
    tracks: BTreeMap<u64, Track>,
    scratch: Scratch,
    frame: Frame,
    /// The last window frame's and the last capture's reports.
    pub window: Report,
    pub capture: Report,
}

impl Vfx {
    pub fn new(assets: &Path) -> Vfx {
        Vfx {
            enabled: true,
            flash_cache: FlashCache::default(),
            budget: Budget::defaults("integrated"),
            library: Library::default(),
            textures: Textures::new(assets),
            tracks: BTreeMap::new(),
            scratch: Scratch::default(),
            frame: Frame::default(),
            window: Report::default(),
            capture: Report::default(),
        }
    }

    /// The frame buffers, for the renderer to borrow while it renders (hand them back with
    /// [`Vfx::put_frame`]).
    pub fn take_frame(&mut self) -> Frame {
        std::mem::take(&mut self.frame)
    }

    pub fn put_frame(&mut self, frame: Frame) {
        self.frame = frame;
    }

    /// Records where world-space emitters are, once per simulation tick (`t`: world time at the
    /// tick's end). Their tracks place trails that repeat exactly on a stepped clock.
    pub fn tick(&mut self, world: &World, generation: u64, t: f64) {
        if !self.enabled {
            return;
        }
        self.library.refresh(world, generation);
        let library = &self.library;
        for e in world.fx_entities() {
            let Some(spec) = &e.vfx else { continue };
            if !library.get(&spec.effect).is_some_and(|f| f.world_space()) {
                continue;
            }
            let p = e.world_position();
            self.tracks.entry(e.id).or_default().push(t, [p.x, p.y, p.z]);
        }
        if !self.tracks.is_empty() {
            self.tracks.retain(|id, _| world.get(*id).is_some_and(|e| e.vfx.as_ref().is_some_and(|v| library.get(&v.effect).is_some_and(|f| f.world_space()))));
        }
    }

    /// Starts loading every texture the world's effects and decals use (finished ones upload).
    /// `fresh`: look for changed asset PNGs now (a capture's first check).
    pub fn request_textures(&mut self, gpu: &mut Gpu, world: &World, fresh: bool) {
        self.pump_textures(gpu, world, fresh);
        for e in world.fx_entities() {
            if let Some(effect) = e.vfx.as_ref().and_then(|v| self.library.get(&v.effect)) {
                for t in effect.textures() {
                    self.textures.get(gpu, t);
                }
            }
            if let Some(d) = &e.decal {
                self.textures.get(gpu, &d.texture);
                if let Some(n) = &d.normal {
                    self.textures.get_linear(gpu, n);
                }
            }
        }
    }

    /// Uploads finished loads; on a scan, marks every texture the world names first, so only
    /// textures nothing names (previews, removed effects) are ever freed.
    fn pump_textures(&mut self, gpu: &mut Gpu, world: &World, fresh: bool) {
        if !self.textures.pump(gpu, fresh) {
            return;
        }
        for e in world.fx_entities() {
            if let Some(effect) = e.vfx.as_ref().and_then(|v| self.library.get(&v.effect)) {
                for t in effect.textures() {
                    self.textures.touch(t, false);
                }
            }
            if let Some(d) = &e.decal {
                self.textures.touch(&d.texture, false);
                if let Some(n) = &d.normal {
                    self.textures.touch(n, true);
                }
            }
        }
        self.textures.rescan(gpu);
    }

    /// Forgets every track (a world was loaded or replaced).
    pub fn reset(&mut self) {
        self.tracks.clear();
    }

    /// Unknown effect names in a component an agent sets: a structured error naming them.
    pub fn check_name(&self, spec: &VfxSpec) -> Result<(), String> {
        if self.library.get(&spec.effect).is_some() {
            return Ok(());
        }
        let names: Vec<&str> = self.library.names().take(64).collect();
        Err(format!("'vfx.effect': no effect named '{}' (built-ins and the world's: {}; vfx_set adds one)", spec.effect, names.join(", ")))
    }

    /// Builds a frame's effects into the frame buffers: particles (sorted far to near), their
    /// materials, decals, and effect lights appended to `lights`. `window`: the report goes to
    /// [`Vfx::window`], else [`Vfx::capture`].
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        gpu: &mut Gpu,
        world: &World,
        generation: u64,
        ground: Option<&crate::anim::GroundGrid>,
        view: &View,
        t: f64,
        include_ui: bool,
        lights: &mut Vec<PointLight>,
        window: bool,
    ) {
        let started = std::time::Instant::now();
        let mut report = std::mem::take(if window { &mut self.window } else { &mut self.capture });
        report.reset();
        report.time_s = t;
        self.frame.clear();
        if self.enabled {
            self.library.refresh(world, generation);
            self.pump_textures(gpu, world, false);
            self.build_decals(gpu, world, view, t, include_ui, &mut report);
            self.build_particles(gpu, world, ground, view, t, include_ui, &mut report);
            self.build_lights(world, view, t, include_ui, lights, &mut report);
        }
        report.cpu_ms = started.elapsed().as_secs_f64() * 1e3;
        *(if window { &mut self.window } else { &mut self.capture }) = report;
    }

    fn build_decals(&mut self, gpu: &mut Gpu, world: &World, view: &View, t: f64, include_ui: bool, report: &mut Report) {
        let s = &mut self.scratch;
        s.decals.clear();
        for e in world.fx_entities() {
            let Some(d) = &e.decal else { continue };
            if !in_frame(e, include_ui) {
                continue;
            }
            let opacity = d.opacity_at(t);
            if opacity <= 0.0 {
                continue;
            }
            let model = e.model() * Mat4::from_trs(Vec3::ZERO, specula_core::math::Quat::IDENTITY, Vec3::new(d.size[0], d.size[1], d.size[2]));
            let (lo, hi) = box_bounds(&model);
            if !view.sees(lo, hi) {
                report.decals_skipped += 1;
                continue;
            }
            let center = (lo + hi) * 0.5;
            let distance = (center - view.eye).length();
            let radius_px = (hi - lo).length() * 0.5 * view.px_per_m(distance);
            if radius_px < 1.0 {
                report.decals_skipped += 1;
                continue;
            }
            let Some(albedo) = self.textures.get(gpu, &d.texture) else {
                let textures = &self.textures;
                report.problem(|m| {
                    textures.write_problem(&d.texture, m);
                });
                continue;
            };
            let normal = match (&d.normal, d.mode) {
                (Some(n), DecalMode::Lit) => self.textures.get_linear(gpu, n),
                _ => None,
            };
            let c = e.color;
            let gd = specula_gpu::Decal {
                model: model.to_cols_array(),
                color: [d.color[0] * c[0], d.color[1] * c[1], d.color[2] * c[2], opacity],
                albedo: Some(albedo),
                uv: d.uv,
                normal,
                normal_strength: d.normal_strength,
                roughness: d.roughness,
                specular: d.specular,
                emissive: d.emissive,
                mode: match d.mode {
                    DecalMode::Lit => specula_gpu::DecalMode::Lit,
                    DecalMode::Multiply => specula_gpu::DecalMode::Multiply,
                },
                angle_fade: d.angle_fade,
            };
            let screen = (view.width.max(1) * view.height.max(1)) as f32;
            let weight = if d.mode == DecalMode::Lit { 2.0 } else { 1.0 };
            let area = weight * view.screen_rect(lo, hi).map_or(0.0, |r| (r[2] * r[3]) as f32 / screen);
            s.decals.push((distance, d.order, e.id, gd, lo, hi, area));
        }
        // The nearest win: at most `max_decals`, covering at most `decal_fill_cap` screens.
        s.decals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        let (max, cap) = (self.budget.max_decals as usize, self.budget.decal_fill_cap);
        let mut fill = 0.0f32;
        let mut keep = 0;
        for d in s.decals.iter() {
            if keep == max || (keep > 0 && fill + d.6 > cap) || (keep == 0 && cap <= 0.0) {
                break;
            }
            fill += d.6;
            keep += 1;
        }
        report.decals_skipped += (s.decals.len() - keep) as u32;
        report.decal_fill = fill;
        s.decals.truncate(keep);
        s.decals.sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
        for (_, _, id, d, lo, hi, _) in s.decals.iter() {
            self.frame.decals.push(*d);
            report.shown_decals.push(ShownDecal { entity: *id, lo: *lo, hi: *hi });
        }
        report.decals = self.frame.decals.len() as u32;
    }

    #[allow(clippy::too_many_arguments)]
    fn build_particles(&mut self, gpu: &mut Gpu, world: &World, ground: Option<&crate::anim::GroundGrid>, view: &View, t: f64, include_ui: bool, report: &mut Report) {
        let s = &mut self.scratch;
        s.pending.clear();
        s.cands.clear();
        for e in world.fx_entities() {
            let Some(spec) = &e.vfx else { continue };
            if !in_frame(e, include_ui) {
                continue;
            }
            let Some(effect) = self.library.get(&spec.effect) else {
                report.problem(|m| {
                    use std::fmt::Write;
                    let _ = write!(m, "entity {} '{}': no effect named '{}' (vfx_get lists them)", e.id, e.name, spec.effect);
                });
                continue;
            };
            report.instances += 1;
            // Params that make this effect flash (set before a vfx_set changed it, loaded, undone,
            // or by game code): not drawn, and said so.
            if !self.flash_cache.ok(effect, &spec.params, self.library.revision) {
                report.flashing += 1;
                report.problem(|m| {
                    use std::fmt::Write;
                    let _ = write!(m, "photosensitive: entity {} '{}' plays '{}' with params that flash more than 3 times a second; not drawn (lower params.rate, size, alpha or intensity)", e.id, e.name, spec.effect);
                });
                continue;
            }
            let start = spec.start.unwrap_or(0.0);
            if effect.finished_after().is_some_and(|f| t - start > f64::from(f)) || t < start {
                report.finished += u32::from(t >= start);
                continue;
            }
            let placement = Placement::from_model(&e.model().to_cols_array());
            let track = if effect.world_space() { self.tracks.get(&e.id) } else { None };
            let inst = Instance { effect, params: spec.params, seed: eval::seed_of(e.id, start, spec.params.seed), start, placement, track, ground: None };
            let (lo, hi) = eval::bounds(&inst, t);
            let (lo, hi) = (Vec3::new(lo[0], lo[1], lo[2]), Vec3::new(hi[0], hi[1], hi[2]));
            if !view.sees(lo, hi) {
                report.culled += 1;
                continue;
            }
            let center = (lo + hi) * 0.5;
            let distance = (center - view.eye).length();
            let px = view.px_per_m(distance);
            let cull_px = effect.emitters.iter().map(|m| m.cull_px).fold(f32::MAX, f32::min);
            if (hi - lo).length() * 0.5 * px < cull_px {
                report.culled += 1;
                continue;
            }
            let first = s.cands.len();
            s.expected.resize(effect.emitters.len(), 0.0);
            eval::expected(&inst, t, &mut s.expected);
            for (k, m) in effect.emitters.iter().enumerate() {
                // Pixels one particle covers: the mean of size² × alpha (a spiking size curve
                // counts by its square), and what it costs to evaluate.
                let px_m = spec.params.size * placement.scale * px;
                s.cands.push(Candidate {
                    priority: m.priority,
                    distance,
                    key: e.id << 3 | k as u64,
                    expected: s.expected[k],
                    px_per_particle: m.mean_area() * px_m * px_m,
                    work: eval::work_per_particle(m),
                    lod_keep: lod_keep(distance, &m.lod),
                });
            }
            s.pending.push(Pending { entity: e.id, effect: Arc::clone(effect), params: spec.params, start, seed: inst.seed, placement, distance, first });
        }
        // The emitter cap: the nearest instances keep theirs.
        let max_emitters = self.budget.max_emitters as usize;
        if s.cands.len() > max_emitters {
            s.by_distance.clear();
            s.by_distance.extend(s.cands.iter().map(|c| (c.distance, c.key)));
            s.by_distance.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let cut = s.by_distance[max_emitters];
            for c in s.cands.iter_mut() {
                if (c.distance, c.key) >= cut {
                    c.expected = 0.0;
                    c.lod_keep = 0.0;
                    report.emitters_over_cap += 1;
                }
            }
        }
        report.emitters = s.cands.len() as u32;
        let screen = (view.width * view.height) as f32;
        report.screen_px = screen;
        report.fill_cap_px = self.budget.fill_cap * screen;
        let b = allocate(&s.cands, self.budget.max_particles as f32, self.budget.max_work, report.fill_cap_px, &mut s.order, &mut s.values, &mut s.shares);
        report.work_expected = b.work;
        report.over_work = b.over_work;
        report.expected = b.expected;
        report.fill_px = b.fill_px;
        report.fill_px_before_cap = b.fill_px_before_cap;
        report.over_particles = b.over_particles;
        report.over_fill = b.over_fill;
        // Evaluate.
        s.cpu.clear();
        s.material_keys.clear();
        let limit = self.budget.max_particles as usize;
        let ground = ground.map(GroundOf);
        // A hard stop past twice the work budget (the budget above is an expectation).
        let max_work = (2.0 * f64::from(self.budget.max_work)).max(1.0e4) as u64;
        s.ranges.clear();
        s.pending_distance.clear();
        for p in &s.pending {
            if report.stats.work >= max_work {
                report.problem(|m| {
                    use std::fmt::Write;
                    let _ = write!(m, "effects' evaluation stopped after {max_work} work units this frame (twice [vfx] max_work: dense or costly effects)");
                });
                break;
            }
            s.plans.clear();
            for (k, m) in p.effect.emitters.iter().enumerate() {
                let share = s.shares[p.first + k];
                if share.keep <= 0.0 {
                    s.plans.push(EmitterPlan { keep: 0.0, size_boost: 1.0, material: 0 });
                    continue;
                }
                let texture = match self.textures.get(gpu, &m.texture) {
                    Some(id) => Some(id),
                    None => {
                        let textures = &self.textures;
                        report.problem(|msg| {
                            textures.write_problem(&m.texture, msg);
                        });
                        s.plans.push(EmitterPlan { keep: 0.0, size_boost: 1.0, material: 0 });
                        continue;
                    }
                };
                let grid = m.flipbook.map_or_else(|| natural_grid(&m.texture), |f| [f.cols, f.rows]);
                let key = (texture, grid, m.soft.to_bits(), m.lit);
                let material = match s.material_keys.iter().position(|k| *k == key) {
                    Some(i) => i,
                    None if s.material_keys.len() < specula_gpu::MAX_PARTICLE_MATERIALS => {
                        s.material_keys.push(key);
                        self.frame.materials.push(specula_gpu::ParticleMaterial { texture, grid, soft: m.soft, lit: m.lit });
                        s.material_keys.len() - 1
                    }
                    None => {
                        s.plans.push(EmitterPlan { keep: 0.0, size_boost: 1.0, material: 0 });
                        continue;
                    }
                };
                s.plans.push(EmitterPlan { keep: share.keep, size_boost: share.size_boost, material: material as u16 });
            }
            let track = if p.effect.world_space() { self.tracks.get(&p.entity) } else { None };
            let inst = Instance {
                effect: &p.effect,
                params: p.params,
                seed: p.seed,
                start: p.start,
                placement: p.placement,
                track,
                ground: ground.as_ref().map(|g| g as &dyn eval::Ground),
            };
            let before = s.cpu.len();
            let mut st = EvalStats::default();
            eval::evaluate(&inst, t, &s.plans, limit, max_work - report.stats.work, &mut s.cpu, &mut st);
            let bounds = s.cpu[before..].iter().fold(None, |acc: Option<(Vec3, Vec3)>, q| {
                let (a, b) = particle_box(q);
                Some(acc.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))))
            });
            // The reach test above is conservative (a world-space trail's reach follows its
            // emitter): particles that all landed out of view aren't drawn, counted or reported.
            if bounds.is_some_and(|(lo, hi)| !view.sees(lo, hi)) {
                s.cpu.truncate(before);
                st.offscreen = st.drawn;
                st.drawn = 0;
                add_stats(&mut report.stats, &st);
                continue;
            }
            add_stats(&mut report.stats, &st);
            let drawn = &s.cpu[before..];
            let ppm = view.px_per_m(p.distance);
            let fill_px: f32 = drawn.iter().map(|q| (q.size * ppm) * (q.size * ppm) * q.color[3]).sum();
            report.shown.push(Shown { entity: p.entity, drawn: st.drawn, live: st.live, bounds, fill_px });
            s.ranges.push((before, s.cpu.len()));
            s.pending_distance.push(p.distance);
        }
        // The fill cap again, on what was actually evaluated (an estimate can't see a size curve
        // spiking in a burst): past it, particles are kept in the order of a stable hash of
        // their own until the cap is reached (the same ones frame to frame; every effect alike).
        let fill: f32 = report.shown.iter().map(|x| x.fill_px).sum();
        report.fill_px = fill;
        if fill > report.fill_cap_px && fill > 0.0 {
            s.fill_order.clear();
            for (i, &(a, b)) in s.ranges.iter().enumerate() {
                let ppm = view.px_per_m(s.pending_distance[i]);
                for j in a..b {
                    let q = &s.cpu[j];
                    let px = (q.size * ppm) * (q.size * ppm) * q.color[3];
                    s.fill_order.push((specula_scintilla::noise::hash3(q.id, 0xF111, 0), px));
                }
            }
            // In the stable hash's order, every particle that still fits is kept (one big near
            // particle that doesn't is skipped, not the end of the list).
            s.fill_keep.clear();
            s.fill_keep.resize(s.fill_order.len(), false);
            s.fill_rank.clear();
            s.fill_rank.extend(0..s.fill_order.len() as u32);
            let order = &s.fill_order;
            s.fill_rank.sort_unstable_by(|&x, &y| order[x as usize].0.cmp(&order[y as usize].0).then(x.cmp(&y)));
            let mut sum = 0.0f32;
            for &k in &s.fill_rank {
                let px = s.fill_order[k as usize].1;
                if sum + px <= report.fill_cap_px {
                    sum += px;
                    s.fill_keep[k as usize] = true;
                }
            }
            let mut w = 0;
            let mut removed = 0u32;
            let mut at = 0usize;
            for (i, &(a, b)) in s.ranges.iter().enumerate() {
                let start = w;
                for j in a..b {
                    let q = s.cpu[j];
                    if s.fill_keep[at] {
                        s.cpu[w] = q;
                        w += 1;
                    }
                    at += 1;
                }
                let kept = (w - start) as u32;
                let shown = &mut report.shown[i];
                removed += shown.drawn - kept;
                shown.fill_px *= if shown.drawn > 0 { kept as f32 / shown.drawn as f32 } else { 0.0 };
                shown.drawn = kept;
            }
            s.cpu.truncate(w);
            report.stats.drawn -= removed;
            report.stats.thinned += removed;
            report.fill_thinned = removed;
            report.over_fill = true;
            report.fill_px = report.shown.iter().map(|x| x.fill_px).sum();
        }
        // Far to near (every particle, whatever its blend: one draw, one correct order).
        s.keys.clear();
        let (eye, fwd) = (view.eye, view.forward);
        for (i, q) in s.cpu.iter().enumerate() {
            let d = (q.position[0] - eye.x) * fwd.x + (q.position[1] - eye.y) * fwd.y + (q.position[2] - eye.z) * fwd.z;
            s.keys.push(u64::from(!order_bits(d)) << 32 | i as u64);
        }
        s.keys.sort_unstable();
        for &k in &s.keys {
            let q = &s.cpu[(k & 0xffff_ffff) as usize];
            self.frame.particles.push(specula_gpu::Particle {
                position: q.position,
                size: q.size,
                color: q.color,
                rotation: q.rotation,
                stretch: q.stretch,
                additive: q.additive,
                material: q.material,
                frame: q.frame,
            });
        }
    }

    fn build_lights(&mut self, world: &World, view: &View, t: f64, include_ui: bool, lights: &mut Vec<PointLight>, report: &mut Report) {
        let s = &mut self.scratch;
        s.lights.clear();
        for e in world.fx_entities() {
            let Some(spec) = &e.vfx else { continue };
            if !spec.params.light || !in_frame(e, include_ui) {
                continue;
            }
            let Some(effect) = self.library.get(&spec.effect) else { continue };
            let Some(l) = &effect.light else { continue };
            let start = spec.start.unwrap_or(0.0);
            let age = t - start;
            let level = l.level(age, eval::seed_of(e.id, start, spec.params.seed));
            if level <= 0.0 || effect.finished_after().is_some_and(|f| age > f64::from(f)) {
                continue;
            }
            let model = e.model();
            let p = model.transform_point(Vec3::new(l.offset[0], l.offset[1], l.offset[2]));
            let tint = spec.params.tint;
            let light = PointLight {
                position: [p.x, p.y, p.z],
                color: [l.color[0] * tint[0], l.color[1] * tint[1], l.color[2] * tint[2]],
                intensity: l.intensity * level * spec.params.intensity,
                range: l.range * Placement::from_model(&model.to_cols_array()).scale,
            };
            // Lights near the view matter most (a light off screen can still light what's on it).
            let d = (p - view.eye).length() - light.range;
            s.lights.push((d, light));
        }
        s.lights.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.position[0].total_cmp(&b.1.position[0])).then(a.1.position[2].total_cmp(&b.1.position[2])));
        let mut added = 0;
        for (_, l) in &s.lights {
            if added < self.budget.lights && lights.len() < MAX_POINT_LIGHTS {
                lights.push(*l);
                added += 1;
            } else {
                report.lights_dropped += 1;
            }
        }
        report.lights = added;
    }

    /// The `effects` part of a sight packet for the frame just built (capture report), its
    /// screen boxes in a `width` x `height` image.
    pub fn packet_json(&self, world: &World, view: &View) -> Json {
        let r = &self.capture;
        let effects = r.shown.iter().take(MAX_PACKET_ITEMS).map(|s| {
            let e = world.get(s.entity);
            let mut f = vec![
                ("id", Json::from(s.entity)),
                ("path", Json::from(world.path_of(s.entity))),
                ("effect", Json::from(e.and_then(|e| e.vfx.as_ref()).map_or("", |v| v.effect.as_str()))),
                ("drawn", Json::from(s.drawn)),
                ("live", Json::from(s.live)),
            ];
            if let Some((lo, hi)) = s.bounds {
                f.push(("bounds", Json::obj([("min", crate::world::vec3_json(lo)), ("max", crate::world::vec3_json(hi))])));
                f.push(("screen", view.screen_rect(lo, hi).map_or(Json::Null, |r| Json::arr(r.map(Json::from)))));
            }
            f.push(("fill_px", Json::from(s.fill_px.round())));
            Json::obj(f)
        });
        let decals = r.shown_decals.iter().take(MAX_PACKET_ITEMS).map(|d| {
            let e = world.get(d.entity);
            Json::obj([
                ("id", Json::from(d.entity)),
                ("path", Json::from(world.path_of(d.entity))),
                ("texture", Json::from(e.and_then(|e| e.decal.as_ref()).map_or("", |d| d.texture.name()))),
                ("screen", view.screen_rect(d.lo, d.hi).map_or(Json::Null, |r| Json::arr(r.map(Json::from)))),
            ])
        });
        Json::obj([
            ("enabled", Json::from(self.enabled)),
            ("effects", Json::arr(effects)),
            ("effects_omitted", Json::from(r.shown.len().saturating_sub(MAX_PACKET_ITEMS))),
            ("decals", Json::arr(decals)),
            ("decals_omitted", Json::from(r.shown_decals.len().saturating_sub(MAX_PACKET_ITEMS))),
            ("frame", r.to_json()),
            ("id_buffer", Json::from("particles and decals don't write the ID buffer: subjects' visible_px and occluded are about geometry; an effect's screen box says what it covers")),
        ])
    }
}

/// What one previewed frame of an effect held.
#[derive(Clone, Debug, Default)]
pub struct PreviewFacts {
    pub stats: EvalStats,
    pub bounds: Option<(Vec3, Vec3)>,
    /// Estimated covered pixels (size² × alpha).
    pub fill_px: f32,
    pub problems: Vec<String>,
}

impl Vfx {
    /// One instance at `t` (a tool preview), into `out` sorted far to near for `view`, bounded
    /// like a window frame: thinned to `[vfx] max_particles` and `max_work`, at most `limit`
    /// particles, and a hard stop at twice the work budget.
    pub fn preview_frame(&mut self, gpu: &mut Gpu, inst: &Instance, t: f64, view: &View, limit: usize, out: &mut Frame) -> PreviewFacts {
        out.clear();
        let limit = limit.min(self.budget.max_particles as usize);
        // The frame budget, shared out evenly over the instance's emitters.
        self.scratch.expected.resize(inst.effect.emitters.len(), 0.0);
        eval::expected(inst, t, &mut self.scratch.expected);
        let want: f32 = self.scratch.expected.iter().sum();
        let work: f32 = inst.effect.emitters.iter().zip(&self.scratch.expected).map(|(m, n)| n * eval::work_per_particle(m)).sum();
        let keep = 1.0f32.min(self.budget.max_particles as f32 / want.max(1e-6)).min(self.budget.max_work / work.max(1e-6));
        // No world here: finished loads only (scans run with the world's frames).
        let _ = self.textures.pump(gpu, false);
        let mut facts = PreviewFacts::default();
        let s = &mut self.scratch;
        s.plans.clear();
        s.material_keys.clear();
        for m in &inst.effect.emitters {
            let Some(texture) = self.textures.ensure(gpu, &m.texture) else {
                facts.problems.push(self.textures.problem(&m.texture).unwrap_or_else(|| format!("texture {}: unavailable", m.texture.name())));
                s.plans.push(EmitterPlan { keep: 0.0, size_boost: 1.0, material: 0 });
                continue;
            };
            let grid = m.flipbook.map_or_else(|| natural_grid(&m.texture), |f| [f.cols, f.rows]);
            let key = (Some(texture), grid, m.soft.to_bits(), m.lit);
            let material = s.material_keys.iter().position(|k| *k == key).unwrap_or_else(|| {
                s.material_keys.push(key);
                out.materials.push(specula_gpu::ParticleMaterial { texture: Some(texture), grid, soft: m.soft, lit: m.lit });
                s.material_keys.len() - 1
            });
            s.plans.push(EmitterPlan { keep, size_boost: 1.0, material: material as u16 });
        }
        if keep < 1.0 {
            facts.problems.push(format!("thinned to the frame budget ([vfx] max_particles {}, max_work {}): {:.0}% of the particles drawn", self.budget.max_particles, self.budget.max_work, keep * 100.0));
        }
        s.cpu.clear();
        let max_work = (2.0 * f64::from(self.budget.max_work)).max(1.0e4) as u64;
        eval::evaluate(inst, t, &s.plans, limit, max_work, &mut s.cpu, &mut facts.stats);
        if facts.stats.work >= max_work {
            facts.problems.push(format!("evaluation stopped after {max_work} work units (twice [vfx] max_work)"));
        }
        s.keys.clear();
        for (i, q) in s.cpu.iter().enumerate() {
            let p = Vec3::new(q.position[0], q.position[1], q.position[2]);
            let (a, b) = particle_box(q);
            facts.bounds = Some(facts.bounds.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
            let d = (p - view.eye).dot(view.forward);
            let ppm = view.px_per_m(d.max(0.05));
            facts.fill_px += (q.size * ppm) * (q.size * ppm) * q.color[3];
            s.keys.push(u64::from(!order_bits(d)) << 32 | i as u64);
        }
        s.keys.sort_unstable();
        for &k in &s.keys {
            let q = &s.cpu[(k & 0xffff_ffff) as usize];
            out.particles.push(specula_gpu::Particle {
                position: q.position,
                size: q.size,
                color: q.color,
                rotation: q.rotation,
                stretch: q.stretch,
                additive: q.additive,
                material: q.material,
                frame: q.frame,
            });
        }
        facts
    }
}

/// The box a particle's quad can cover: a rotated square (half diagonal) and its stretch.
fn particle_box(q: &eval::Particle) -> (Vec3, Vec3) {
    let s = Vec3::new(q.stretch[0], q.stretch[1], q.stretch[2]);
    let h = q.size * 0.5 * std::f32::consts::SQRT_2 + 0.5 * s.length();
    let p = Vec3::new(q.position[0], q.position[1], q.position[2]);
    (p - Vec3::new(h, h, h), p + Vec3::new(h, h, h))
}

fn add_stats(a: &mut EvalStats, b: &EvalStats) {
    a.offscreen += b.offscreen;
    a.live += b.live;
    a.drawn += b.drawn;
    a.thinned += b.thinned;
    a.killed += b.killed;
    a.dropped += b.dropped;
    a.visited += b.visited;
    a.work += b.work;
}

/// Ascending order of `f` as unsigned bits (NaN as 0).
fn order_bits(f: f32) -> u32 {
    let f = if f.is_nan() { 0.0 } else { f };
    let b = f.to_bits();
    if b >> 31 == 1 {
        !b
    } else {
        b | 0x8000_0000
    }
}

/// The world box of the unit box under `m`.
fn box_bounds(m: &Mat4) -> (Vec3, Vec3) {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for i in 0..8 {
        let p = m.transform_point(Vec3::new(if i & 1 == 0 { -0.5 } else { 0.5 }, if i & 2 == 0 { -0.5 } else { 0.5 }, if i & 4 == 0 { -0.5 } else { 0.5 }));
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, Shape, SpawnSpec, UpdateSpec};

    #[test]
    fn budgets_parse_per_adapter() {
        let t = VfxTable::parse(&specula_core::toml::parse("max_particles = 3000\n[integrated]\nfill_cap = 1.0\n").unwrap()).unwrap();
        assert_eq!(t.budget("integrated").max_particles, 3000);
        assert_eq!(t.budget("integrated").fill_cap, 1.0);
        assert_eq!(t.budget("discrete").fill_cap, 3.0);
        assert!(VfxTable::parse(&specula_core::toml::parse("max_partcles = 3\n").unwrap()).unwrap_err().contains("unknown key"));
        assert!(VfxTable::parse(&specula_core::toml::parse("lights = 99\n").unwrap()).is_err());
    }

    #[test]
    fn the_library_reads_world_effects_and_reports_bad_ones() {
        let mut w = World::default();
        let mut lib = Library::default();
        lib.refresh(&w, 0);
        assert!(lib.get("fire").is_some() && lib.get("mine").is_none());
        // Tagged outside /vfx: ignored (it can't shadow the library's own), and reported.
        let mine = || {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some("mine".into());
            s.tags.insert(EFFECT_TAG.into());
            s.props = Json::parse(r#"{"vfx_effect": {"emitters": [{"rate": 3}]}}"#).unwrap();
            s
        };
        let stray = w.spawn(mine(), "t").unwrap().id;
        lib.refresh(&w, 0);
        assert!(lib.get("mine").is_none() && lib.problems[0].contains("outside /vfx"), "{:?}", lib.problems);
        w.delete(&EntityKey::Id(stray), None).unwrap();
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some(LIBRARY_GROUP.into());
        w.spawn(g, "t").unwrap();
        let mut s = mine();
        s.parent = Some(EntityKey::Path(format!("/{LIBRARY_GROUP}")));
        let id = w.spawn(s, "t").unwrap().id;
        lib.refresh(&w, 0);
        assert!(lib.get("mine").is_some(), "{:?}", lib.problems);
        let rev = lib.revision;
        w.update(&EntityKey::Id(id), UpdateSpec { props: Some(Json::parse(r#"{"vfx_effect": {"emitters": [{"rate": 9000}]}}"#).unwrap()), ..Default::default() }, "t").unwrap();
        lib.refresh(&w, 0);
        assert!(lib.get("mine").is_none());
        assert!(lib.problems[0].contains("rate"), "{:?}", lib.problems);
        assert_ne!(lib.revision, rev);
        let mut fire = SpawnSpec::new(Shape::Group);
        fire.name = Some("fire".into());
        fire.tags.insert(EFFECT_TAG.into());
        fire.props = Json::parse(r#"{"vfx_effect": {"emitters": [{"rate": 3}]}}"#).unwrap();
        fire.parent = Some(EntityKey::Path(format!("/{LIBRARY_GROUP}")));
        w.spawn(fire, "t").unwrap();
        lib.refresh(&w, 0);
        assert!(lib.problems.iter().any(|p| p.contains("built-in")), "{:?}", lib.problems);
    }

    #[test]
    fn components_round_trip_through_the_world() {
        let mut w = World::default();
        w.set_clock_s(5.0);
        let mut s = SpawnSpec::new(Shape::Group);
        s.vfx = Some(VfxSpec::new("fire"));
        let id = w.spawn(s, "t").unwrap().id;
        let e = w.get(id).unwrap();
        assert_eq!(e.vfx.as_ref().unwrap().start, Some(5.0), "starts at the world time");
        assert_eq!(w.fx_entities().count(), 1);
        let h = w.state_hash();
        w.update(&EntityKey::Id(id), UpdateSpec { vfx: Some(None), ..Default::default() }, "t").unwrap();
        assert_eq!(w.fx_entities().count(), 0);
        assert_ne!(w.state_hash(), h, "components are world data");
        let decal = DecalSpec::parse(&Json::from("scorch")).unwrap();
        w.update(&EntityKey::Id(id), UpdateSpec { decal: Some(Some(decal)), ..Default::default() }, "t").unwrap();
        assert_eq!(w.fx_entities().count(), 1);
        let json = w.get(id).unwrap().to_json("/x");
        assert_eq!(json.get("decal").and_then(|d| d.get("texture")).and_then(Json::as_str), Some("scorch"));
    }

    #[test]
    fn views_project_and_cull() {
        let cam = crate::sight::Camera::new(Vec3::new(0.0, 2.0, 10.0), Vec3::ZERO, 50.0);
        let v = View::of(&cam, 640, 360);
        assert!(v.sees(Vec3::new(-1.0, -1.0, -1.0), Vec3::ONE));
        assert!(!v.sees(Vec3::new(-1.0, -1.0, 20.0), Vec3::new(1.0, 1.0, 22.0)), "behind the eye");
        let r = v.screen_rect(Vec3::new(-1.0, -1.0, -1.0), Vec3::ONE).unwrap();
        assert!(r[0] > 200 && r[0] + r[2] < 440, "{r:?}");
        assert!((v.px_per_m(10.0) - 360.0 * 0.5 / (25f32.to_radians().tan() * 10.0)).abs() < 1e-3);
    }
}
