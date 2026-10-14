//! World files: the world as deterministic, diff-friendly text.
//!
//! A world named `greybox` lives in `<worlds dir>/greybox/`:
//!
//! - `world.json`: the format, a save `generation`, the id counter, the environment, the list of
//!   subtree files, and the top-level entities that have no children.
//! - one `<top-level name>.<generation>.json` per top-level entity **with** children, holding
//!   that whole subtree. Agents usually build in separate top-level groups (districts), so their
//!   work lands in separate files: git diffs stay small and parallel work doesn't merge-conflict.
//!
//! Every entity is one line of JSON (fixed field order, defaults left out, keys sorted in
//! `props`), listed in path order. Each record is also valid `world_spawn` arguments plus `id`,
//! `version` and attribution, and loading goes through the same validation as the tool. Floats
//! are written as the shortest decimal that reads back bit-exactly, and colours as hex only when
//! hex is exact, so save → load restores the world bit for bit ([`World::state_hash`] matches).
//!
//! **`world.json` is the single commit point.** A save writes the subtrees that changed under new
//! generation-suffixed names (synced to disk), then replaces `world.json` (temporary file, sync,
//! rename) listing exactly the files of the new state, then deletes the previous generation's
//! files that are no longer listed. A crash at any moment leaves either the old world or the new
//! one, never a mix. A save that fails before the commit removes the files it created. Unchanged
//! subtrees keep their files (and names). Only files that are named like ours *and* start like a
//! world file (`{"format": "specula.world/v1", ...`) are ever swept, so other files in the folder
//! (a user's `notes.2026.json`) are never touched. While an engine owns a world it holds
//! `world.lock` there ([`WorldLock`]), so two engines can't interleave saves.
//!
//! Saving is split so the engine can write from a worker thread: [`plan`] takes what changed off
//! the live world (on the engine thread; unchanged subtrees cost nothing), and [`execute`] renders
//! and writes it.

use crate::tools::{check_keys, parse_env, parse_spawn_fields, ENV_KEYS};
use crate::world::{color_json_exact, material_name, vec3_json, Entity, Layer, Look, World, WorldError};
use specula_core::hash::hash_str;
use specula_core::Json;
use specula_gpu::Material;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FORMAT: &str = "specula.world/v1";
const INDEX: &str = "world.json";
const INDEX_TMP: &str = "world.json.tmp";
/// Keys an entity record may have; anything else is a typo worth reporting.
const RECORD_KEYS: [&str; 30] = [
    "path", "id", "shape", "position", "rotation_deg", "scale", "color", "material", "opacity", "emissive",
    "metallic", "roughness", "specular", "seed", "color2", "light", "profile", "segments", "tags", "props",
    "collider", "body", "version", "created_by", "modified_by", "recipe", "params", "visible", "layer", "anim",
];
const INDEX_KEYS: &[&str] = &["format", "generation", "next_id", "entity_count", "clock_s", "env", "subtrees", "entities"];
const SUBTREE_KEYS: &[&str] = &["format", "root", "entities"];

/// What went wrong reading or writing a world, with where: the file, the entity path and the
/// field, when known.
#[derive(Clone, Debug, PartialEq)]
pub struct PersistError {
    pub code: &'static str,
    pub message: String,
    pub file: Option<String>,
    pub entity: Option<String>,
    pub field: Option<String>,
}

impl PersistError {
    fn new(code: &'static str, message: impl Into<String>) -> PersistError {
        PersistError { code, message: message.into(), file: None, entity: None, field: None }
    }

    fn in_file(mut self, file: &str) -> PersistError {
        self.file.get_or_insert_with(|| file.to_owned());
        self
    }

    fn field(mut self, field: &str) -> PersistError {
        self.field = Some(field.to_owned());
        self
    }

    /// Whether the engine's [`Saved`] cache can no longer be trusted after this failed save.
    /// Failures before the commit leave every listed file as it was, so the cache stays valid;
    /// only a file the cache relies on having gone missing invalidates it.
    pub fn invalidates_cache(&self) -> bool {
        self.code == "missing_file"
    }

    fn io(file: &str, e: std::io::Error) -> PersistError {
        PersistError::new("io", e.to_string()).in_file(file)
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))];
        let opt = |v: &Option<String>| v.as_deref().map_or(Json::Null, Json::from);
        f.extend([("file", opt(&self.file)), ("entity", opt(&self.entity)), ("field", opt(&self.field))]);
        Json::obj(f)
    }
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        for (label, v) in [("file", &self.file), ("entity", &self.entity), ("field", &self.field)] {
            if let Some(v) = v {
                write!(f, " ({label} {v})")?;
            }
        }
        Ok(())
    }
}

/// World names are folder names: 1..64 of `A-Z a-z 0-9 _ - .`, starting with a letter, digit or
/// `_`, not ending in `.`, and not a Windows device name. That rules out `..`, separators and
/// absolute paths, so a name can never escape the worlds directory.
pub fn validate_world_name(name: &str) -> Result<(), String> {
    let ok = (1..=64).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && name.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.ends_with('.')
        && !is_device_name(name);
    if ok {
        Ok(())
    } else {
        Err(format!(
            "'name' '{name}' is not a valid world name: 1..64 of A-Z a-z 0-9 _ - . starting with a letter, digit or _"
        ))
    }
}

/// `con`, `nul`, `com1`… (with any extension) can't be files or folders on Windows.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || ((stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
}

/// `<stem>.<generation>.json` with a stem of `A-Z a-z 0-9 _ -`.
fn is_subtree_file(name: &str) -> bool {
    let Some(rest) = name.strip_suffix(".json") else { return false };
    let Some((stem, generation)) = rest.rsplit_once('.') else { return false };
    !stem.is_empty()
        && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && !generation.is_empty()
        && generation.bytes().all(|b| b.is_ascii_digit())
}

fn stem_of(file: &str) -> &str {
    file.strip_suffix(".json").and_then(|r| r.rsplit_once('.')).map_or(file, |(stem, _)| stem)
}

/// What the engine remembers about the world folder it last wrote or read, so the next save
/// rewrites only what changed: the generation, and per top-level subtree its file and the
/// [`World::subtree_stamp`] that file holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub dir: PathBuf,
    pub generation: u64,
    /// Top-level entity id → its file.
    pub subtrees: BTreeMap<u64, SavedSubtree>,
    /// The rendered top-level entities without children, and the `(id, stamp)` list they were
    /// rendered from: a flat world doesn't re-copy every entity on each autosave.
    pub loose: Option<LooseCache>,
}

/// `(id, subtree stamp)` of each loose top-level entity, and their rendered records.
pub type LooseCache = (Vec<(u64, u64)>, Arc<Vec<Json>>);

#[derive(Clone, Debug, PartialEq)]
pub struct SavedSubtree {
    pub stamp: u64,
    pub root: String,
    pub file: String,
}

enum Part {
    /// Unchanged since it was written: keep the file.
    Keep { id: u64, saved: SavedSubtree },
    /// Changed or new: write under `stem`.
    Write { id: u64, stamp: u64, root: String, stem: String, entities: Vec<(Entity, String)> },
}

enum Loose {
    Keep(Arc<Vec<Json>>),
    Write(Vec<(Entity, String)>),
}

/// A world save detached from the live world: owned copies of what changed, ready for
/// [`execute`] on any thread.
pub struct Plan {
    dir: PathBuf,
    next_id: u64,
    entity_count: usize,
    /// The engine clock at the save: animation starts are on it (see `world::AnimSpec::start`).
    clock_s: f64,
    env: Json,
    /// Top-level entities without children (they live in `world.json`), and their `(id, stamp)`.
    loose: Loose,
    loose_key: Vec<(u64, u64)>,
    parts: Vec<Part>,
    previous: Option<Saved>,
}

/// Takes what [`execute`] needs from `world`. `previous` is what was saved into `dir` before (by
/// this engine); subtrees whose stamp hasn't moved since are kept, not copied.
///
/// Transient entities (game presentation) are never saved; they are always leaves of the saved
/// tree (nothing persistent sits below a transient entity), so leaving them out drops nothing.
pub fn plan(world: &World, dir: &Path, previous: Option<&Saved>) -> Plan {
    let previous = previous.filter(|p| p.dir == dir).cloned();
    let persistent = |id: &u64| world.get(*id).is_some_and(|e| !e.transient);
    let tops: Vec<u64> = world.children_of(None).filter(persistent).collect();
    let (subtrees, loose): (Vec<u64>, Vec<u64>) =
        tops.into_iter().partition(|&id| world.children_of(Some(id)).any(|c| persistent(&c)));
    let loose_key: Vec<(u64, u64)> = loose.iter().map(|&id| (id, world.subtree_stamp(id))).collect();
    let loose = match previous.as_ref().and_then(|p| p.loose.as_ref()).filter(|(key, _)| *key == loose_key) {
        Some((_, records)) => Loose::Keep(Arc::clone(records)),
        None => Loose::Write(loose.into_iter().map(|id| (world.get(id).expect("listed").clone(), world.path_of(id))).collect()),
    };
    // Stems stay with their subtree (by path) so files don't churn; newcomers take what's free.
    let old_stems: BTreeMap<&str, &str> =
        previous.iter().flat_map(|p| p.subtrees.values()).map(|s| (s.root.as_str(), stem_of(&s.file))).collect();
    let roots: Vec<String> = subtrees.iter().map(|&id| world.path_of(id)).collect();
    let mut taken: BTreeSet<String> =
        roots.iter().filter_map(|r| old_stems.get(r.as_str())).map(|s| s.to_ascii_lowercase()).collect();
    let mut parts = Vec::with_capacity(subtrees.len());
    for (&id, root) in subtrees.iter().zip(roots) {
        let stamp = world.subtree_stamp(id);
        let kept = previous.as_ref().and_then(|p| p.subtrees.get(&id)).filter(|s| s.stamp == stamp && s.root == root);
        if let Some(saved) = kept {
            parts.push(Part::Keep { id, saved: saved.clone() });
            continue;
        }
        let stem = match old_stems.get(root.as_str()) {
            Some(stem) => (*stem).to_owned(),
            None => new_stem(&world.get(id).expect("listed").name, &mut taken),
        };
        let mut entities = Vec::new();
        world.walk(Some(id), |e, path| {
            if !e.transient {
                entities.push((e.clone(), path.to_owned()));
            }
        });
        parts.push(Part::Write { id, stamp, root, stem, entities });
    }
    Plan {
        dir: dir.to_owned(),
        next_id: world.next_id(),
        entity_count: world.persistent_len(),
        clock_s: world.clock_s(),
        env: world.env().to_json_exact(),
        loose,
        loose_key,
        parts,
        previous,
    }
}

/// A file stem for a new subtree: the name with anything outside `A-Z a-z 0-9 _ -` replaced by
/// `_`, plus a short hash of the real name when that clashes (case-insensitively, for Windows and
/// macOS) with a stem in use or is a device name.
fn new_stem(name: &str, taken: &mut BTreeSet<String>) -> String {
    let mut stem: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).take(64).collect();
    if taken.contains(&stem.to_ascii_lowercase()) || is_device_name(&stem) || stem == "world" {
        stem = format!("{stem}-{:08x}", hash_str(name) as u32);
    }
    let mut candidate = stem.clone();
    let mut n = 2;
    while taken.contains(&candidate.to_ascii_lowercase()) {
        candidate = format!("{stem}-{n}");
        n += 1;
    }
    taken.insert(candidate.to_ascii_lowercase());
    candidate
}

impl Plan {
    /// The files to write for `generation` (changed subtrees, then `world.json`) and the state
    /// they leave behind. Pure.
    fn render(&self, generation: u64) -> (Vec<(String, String)>, Saved) {
        let mut files = Vec::new();
        let mut saved = Saved { dir: self.dir.clone(), generation, subtrees: BTreeMap::new(), loose: None };
        for part in &self.parts {
            match part {
                Part::Keep { id, saved: s } => {
                    saved.subtrees.insert(*id, s.clone());
                }
                Part::Write { id, stamp, root, stem, entities } => {
                    let file = format!("{stem}.{generation}.json");
                    let records: Vec<Json> = entities.iter().map(|(e, path)| record(e, path)).collect();
                    let header = vec![("format", Json::from(FORMAT)), ("root", Json::from(root.as_str()))];
                    files.push((file.clone(), document(header, &records)));
                    saved.subtrees.insert(*id, SavedSubtree { stamp: *stamp, root: root.clone(), file });
                }
            }
        }
        // Listed in path order, like everything else.
        let mut listed: Vec<&SavedSubtree> = saved.subtrees.values().collect();
        listed.sort_by(|a, b| crate::world::path_cmp(&a.root, &b.root));
        let subtrees = Json::arr(listed.iter().map(|s| {
            Json::obj([("root", Json::from(s.root.as_str())), ("file", Json::from(s.file.as_str()))])
        }));
        let loose = match &self.loose {
            Loose::Keep(records) => Arc::clone(records),
            Loose::Write(entities) => Arc::new(entities.iter().map(|(e, path)| record(e, path)).collect()),
        };
        let header = vec![
            ("format", Json::from(FORMAT)),
            ("generation", Json::from(generation)),
            ("next_id", Json::from(self.next_id)),
            ("entity_count", Json::from(self.entity_count)),
            ("clock_s", Json::from(self.clock_s)),
            ("env", self.env.clone()),
            ("subtrees", subtrees),
        ];
        files.push((INDEX.to_owned(), document(header, &loose)));
        saved.loose = Some((self.loose_key.clone(), loose));
        (files, saved)
    }
}

/// Every file of the world, as a first save would write it. Pure and deterministic.
pub fn render(world: &World) -> Vec<(String, String)> {
    plan(world, Path::new(""), None).render(1).0
}

/// One entity as saved: valid `world_spawn` arguments plus id, version and attribution.
fn record(e: &Entity, path: &str) -> Json {
    let mut f = vec![
        ("path", Json::from(path)),
        ("id", Json::from(e.id)),
        ("shape", Json::from(e.shape.name())),
        ("position", vec3_json(e.position)),
        ("rotation_deg", vec3_json(e.rotation_deg)),
        ("scale", vec3_json(e.scale)),
        ("color", color_json_exact(e.color)),
    ];
    let (look, d) = (&e.look, Look::default());
    let (s, ds) = (&look.surface, &d.surface);
    if look.material != Material::Lit {
        f.push(("material", Json::from(material_name(look.material))));
    }
    // Non-standard materials get a material-specific default opacity on spawn: always write it.
    if look.material != Material::Lit || look.opacity.to_bits() != d.opacity.to_bits() {
        f.push(("opacity", Json::from(look.opacity)));
    }
    for (key, v, default) in [
        ("emissive", s.emissive, ds.emissive),
        ("metallic", s.metallic, ds.metallic),
        ("roughness", s.roughness, ds.roughness),
        ("specular", s.specular, ds.specular),
        ("seed", s.seed, ds.seed),
    ] {
        if v.to_bits() != default.to_bits() {
            f.push((key, Json::from(v)));
        }
    }
    if s.color2.map(f32::to_bits) != ds.color2.map(f32::to_bits) {
        f.push(("color2", color_json_exact(s.color2)));
    }
    if let Some(l) = e.light {
        let mut light = vec![
            ("intensity", Json::from(l.intensity)),
            ("range", Json::from(l.range)),
            ("offset", vec3_json(l.offset)),
        ];
        if let Some(c) = l.color {
            light.push(("color", color_json_exact(c)));
        }
        f.push(("light", Json::obj(light)));
    }
    if let Some(p) = &e.profile {
        f.push(("profile", Json::arr(p.points.iter().map(|p| Json::arr([Json::from(p[0]), Json::from(p[1])])))));
        f.push(("segments", Json::from(p.segments)));
    }
    if let Some(m) = &e.model {
        // A model's surface seed is always 0 (checked on spawn), so `seed` is free for its model
        // seed, as in world_spawn.
        f.push(("recipe", Json::from(m.recipe.as_str())));
        if !m.params.is_empty() {
            f.push(("params", m.params_json()));
        }
        if let Some(s) = m.seed {
            f.push(("seed", Json::from(s)));
        }
    }
    if !e.visible {
        f.push(("visible", Json::from(false)));
    }
    if e.layer != Layer::World {
        f.push(("layer", Json::from(e.layer.name())));
    }
    if !e.tags.is_empty() {
        f.push(("tags", Json::arr(e.tags.iter().map(|t| Json::from(t.as_str())))));
    }
    if e.props.as_object().is_some_and(|o| !o.is_empty()) {
        f.push(("props", e.props.clone()));
    }
    if let Some(c) = &e.collider {
        f.push(("collider", c.to_json()));
    }
    if let Some(b) = e.body {
        f.push(("body", b.to_json()));
    }
    if let Some(a) = &e.anim {
        f.push(("anim", a.to_json()));
    }
    f.push(("version", Json::from(e.version)));
    f.push(("created_by", Json::from(&*e.created_by)));
    f.push(("modified_by", Json::from(&*e.modified_by)));
    Json::obj(f)
}

/// `{header..., "entities": [one line per entity]}`.
fn document(header: Vec<(&str, Json)>, entities: &[Json]) -> String {
    let mut out = String::from("{\n");
    for (key, value) in header {
        out.push_str(&format!("  {}: {value},\n", Json::from(key)));
    }
    out.push_str("  \"entities\": [");
    for (i, e) in entities.iter().enumerate() {
        out.push_str(if i == 0 { "\n    " } else { ",\n    " });
        out.push_str(&e.to_string());
    }
    out.push_str(if entities.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
    out
}

pub struct SaveReport {
    pub dir: PathBuf,
    pub generation: u64,
    /// Files the world now consists of (`world.json` included).
    pub files: usize,
    /// Files written by this save (`world.json` included).
    pub written: usize,
    /// Old files of ours swept after the commit.
    pub removed: usize,
    /// Old files of ours that could not be removed (in use, say). The save itself succeeded; the
    /// next save sweeps them again.
    pub leftovers: Vec<String>,
}

/// What a world folder holds.
struct Folder {
    /// The committed `world.json`'s generation and the subtree files it lists, if there is one.
    committed: Option<(u64, BTreeSet<String>)>,
    /// Files of ours (`<stem>.<gen>.json` or `world.json.tmp` that start like a world file),
    /// listed or not: what a save may sweep.
    ours: Vec<String>,
    /// The highest generation suffix of any `<stem>.<gen>.json` name (any case, any content):
    /// a new save's names stay clear of all of them.
    highest: u64,
}

/// The lock file an engine holds while it owns a world folder (never swept, never a reason to
/// refuse adoption).
const LOCK: &str = "world.lock";

/// Whether a file starts like one we write: `{` then `"format": "specula.world/v1"`, ignoring
/// whitespace (so a CRLF checkout still counts).
fn starts_like_ours(path: &Path) -> bool {
    use std::io::Read as _;
    let mut head = Vec::with_capacity(64);
    let read = std::fs::File::open(path).and_then(|f| f.take(64).read_to_end(&mut head));
    if read.is_err() {
        return false;
    }
    let compact: Vec<u8> = head.into_iter().filter(|b| !b.is_ascii_whitespace()).collect();
    compact.starts_with(format!("{{\"format\":\"{FORMAT}\"").as_bytes())
}

/// Reads `dir`. A folder that has files but neither `world.json` nor only files of ours is
/// refused: it isn't a world, and saving would mix ours into someone else's. (Only files of ours
/// there means a first save crashed before committing: the folder is adopted and swept.)
fn scan(dir: &Path) -> Result<Folder, PersistError> {
    let mut folder = Folder { committed: None, ours: Vec::new(), highest: 0 };
    let mut foreign = None;
    let mut has_index = false;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(generation) = generation_of(&name) {
                folder.highest = folder.highest.max(generation);
            }
            if name == INDEX {
                has_index = true;
            } else if (name == INDEX_TMP || is_subtree_file(&name)) && starts_like_ours(&entry.path()) {
                folder.ours.push(name);
            } else if name != LOCK {
                foreign.get_or_insert(name);
            }
        }
    }
    folder.ours.sort();
    if has_index {
        let text = std::fs::read_to_string(dir.join(INDEX)).map_err(|e| PersistError::io(INDEX, e))?;
        let doc = parse_doc(INDEX, &text)?;
        let generation = doc.get("generation").and_then(Json::as_u64).unwrap_or(0);
        let listed = doc
            .get("subtrees")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|s| s.get("file").and_then(Json::as_str))
            .map(str::to_owned)
            .collect();
        folder.committed = Some((generation, listed));
    } else if let Some(other) = foreign {
        return Err(PersistError::new(
            "not_a_world",
            format!("{} has files (such as {other}) but no world.json; refusing to save a world into it", dir.display()),
        )
        .in_file(&other));
    }
    Ok(folder)
}

/// The generation in a `<stem>.<gen>.json` name (`.json` in any case). Numbers above 2^53 are
/// ignored: no save gets there, and following one could overflow.
fn generation_of(file: &str) -> Option<u64> {
    let dot = file.len().checked_sub(5)?;
    if !file.get(dot..)?.eq_ignore_ascii_case(".json") {
        return None;
    }
    let digits = file[..dot].rsplit_once('.')?.1;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|&g: &u64| g <= 1 << 53)
}

/// Whether a world may be created in `dir` (missing, empty, already a world, or the remains of
/// a first save that never committed).
pub fn check_adoptable(dir: &Path) -> Result<(), PersistError> {
    scan(dir).map(|_| ())
}

/// The entity count `world.json` in `dir` records, without loading the world: only the header
/// lines are read (`entity_count` is written before the environment and the entities). `None`
/// when there is no world there, or no count.
pub fn peek_entity_count(dir: &Path) -> Result<Option<u64>, PersistError> {
    use std::io::Read as _;
    let file = match std::fs::File::open(dir.join(INDEX)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(PersistError::io(INDEX, e)),
    };
    let mut head = Vec::new();
    file.take(1024).read_to_end(&mut head).map_err(|e| PersistError::io(INDEX, e))?;
    let head = String::from_utf8_lossy(&head);
    Ok(head
        .lines()
        .find_map(|l| l.trim().strip_prefix("\"entity_count\":"))
        .and_then(|v| v.trim().trim_end_matches(',').parse().ok()))
}

/// Exclusive ownership of a world folder while an engine works in it, so a second engine (on
/// another port, say) can't interleave saves and sweep files the first one's `world.json` lists.
///
/// On Windows the lock is `world.lock` opened with no sharing: the OS releases it when the
/// process exits, even after a crash, so a stale file never blocks anyone. Elsewhere it isn't
/// enforced yet (the engine doesn't run there).
pub struct WorldLock {
    path: PathBuf,
    /// The open handle is the lock; `None` only while dropping.
    file: Option<std::fs::File>,
}

impl WorldLock {
    /// Takes `dir` (created if needed), or fails with `world_locked`.
    pub fn acquire(dir: &Path) -> Result<WorldLock, PersistError> {
        let shown = dir.display().to_string();
        std::fs::create_dir_all(dir).map_err(|e| PersistError::io(&shown, e))?;
        let path = dir.join(LOCK);
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            options.share_mode(0);
        }
        match options.open(&path) {
            Ok(file) => Ok(WorldLock { path, file: Some(file) }),
            // ERROR_SHARING_VIOLATION: another process holds it.
            Err(e) if e.raw_os_error() == Some(32) || e.kind() == std::io::ErrorKind::PermissionDenied => Err(
                PersistError::new("world_locked", format!("{shown} is in use by another engine (it holds {LOCK})"))
                    .in_file(LOCK),
            ),
            Err(e) => Err(PersistError::io(LOCK, e)),
        }
    }

    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new(""))
    }
}

impl Drop for WorldLock {
    fn drop(&mut self) {
        // Close the handle (releasing the lock), then tidy up. A leftover file is harmless: only
        // an open handle locks.
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A filesystem step of [`execute`], for tests that cut a save short.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step<'a> {
    Write(&'a str),
    WriteIndex,
    Commit,
    Remove(&'a str),
}

/// What a test hook does at a [`Step`]: go on, fail that step (an I/O error the save handles),
/// or crash (stop right there, as if the process died: nothing is cleaned up).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))] // only tests cut saves short
pub(crate) enum Hook {
    Go,
    Fail,
    Crash,
}

fn injected(step: &Step) -> PersistError {
    PersistError::new("io", format!("injected failure at {step:?}"))
}

/// Writes a file that must not exist yet: a save never opens an existing file for writing, so
/// it can't damage one a committed `world.json` lists.
fn create_synced(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()
}

/// Writes `plan` (see the module docs for the commit order) and returns what is now on disk.
pub fn execute(plan: Plan) -> Result<(Saved, SaveReport), PersistError> {
    execute_with(plan, &mut |_| Hook::Go)
}

/// How often a save moves to a later generation when a name it wanted already exists.
const NAME_RETRIES: u32 = 8;

pub(crate) fn execute_with(plan: Plan, hook: &mut dyn FnMut(Step) -> Hook) -> Result<(Saved, SaveReport), PersistError> {
    let dir = &plan.dir;
    let shown = dir.display().to_string();
    let folder = scan(dir)?;
    // Above every generation anyone has written here (memory may be stale after a failure), so
    // new file names don't collide with a listed one.
    let previous = plan.previous.as_ref().map_or(0, |p| p.generation);
    let committed = folder.committed.as_ref().map_or(0, |(g, _)| *g);
    let overflow = || PersistError::new("generation_overflow", "no generation number is left for this world").in_file(INDEX);
    let mut generation = previous.max(committed).max(folder.highest).checked_add(1).ok_or_else(overflow)?;
    std::fs::create_dir_all(dir).map_err(|e| PersistError::io(&shown, e))?;

    // Write the new files. Before the commit nothing listed is touched, so on any failure the
    // files this attempt created are removed again (and the caller's cache stays valid).
    let mut created: Vec<PathBuf> = Vec::new();
    let mut retries = 0;
    let (files, saved) = loop {
        let (files, saved) = plan.render(generation);
        for kept in saved.subtrees.values().filter(|s| !files.iter().any(|(f, _)| *f == s.file)) {
            if !dir.join(&kept.file).is_file() {
                return Err(PersistError::new("missing_file", format!("{} is gone; the next save rewrites everything", kept.file))
                    .in_file(&kept.file));
            }
        }
        let mut clash = false;
        for (name, content) in &files[..files.len() - 1] {
            let step = Step::Write(name);
            match hook(step) {
                Hook::Go => {}
                Hook::Crash => return Err(injected(&step)),
                Hook::Fail => return Err(undo(&created, injected(&step))),
            }
            let path = dir.join(name);
            match create_synced(&path, content) {
                Ok(()) => created.push(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && retries < NAME_RETRIES => {
                    clash = true;
                    break;
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&path); // a partial write of ours
                    return Err(undo(&created, PersistError::io(name, e)));
                }
            }
        }
        if !clash {
            break (files, saved);
        }
        // Someone else's file has the name (a differently-cased extension, say): move on.
        let _ = undo(&created, PersistError::new("io", ""));
        created.clear();
        retries += 1;
        generation = generation.checked_add(1).ok_or_else(overflow)?;
    };
    let index = &files[files.len() - 1];
    match hook(Step::WriteIndex) {
        Hook::Go => {}
        Hook::Crash => return Err(injected(&Step::WriteIndex)),
        Hook::Fail => return Err(undo(&created, injected(&Step::WriteIndex))),
    }
    let tmp = dir.join(INDEX_TMP);
    let _ = std::fs::remove_file(&tmp); // a crashed save's, never listed
    if let Err(e) = create_synced(&tmp, &index.1) {
        let _ = std::fs::remove_file(&tmp);
        return Err(undo(&created, PersistError::io(INDEX, e)));
    }
    created.push(tmp.clone());
    match hook(Step::Commit) {
        Hook::Go => {}
        Hook::Crash => return Err(injected(&Step::Commit)),
        Hook::Fail => return Err(undo(&created, injected(&Step::Commit))),
    }
    if let Err(e) = std::fs::rename(&tmp, dir.join(INDEX)) {
        return Err(undo(&created, PersistError::io(INDEX, e)));
    }

    // Committed. Sweep every file of ours the new world.json doesn't list: the previous
    // generation's, and anything a crashed save left. Failures are only leftovers now.
    let keep: BTreeSet<&str> = saved.subtrees.values().map(|s| s.file.as_str()).collect();
    let (mut removed, mut leftovers) = (0, Vec::new());
    for old in folder.ours.iter().filter(|f| !keep.contains(f.as_str())) {
        let step = Step::Remove(old);
        match hook(step) {
            Hook::Crash => return Err(injected(&step)),
            Hook::Fail => leftovers.push(old.clone()),
            Hook::Go => match std::fs::remove_file(dir.join(old)) {
                Ok(()) => removed += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => leftovers.push(old.clone()),
            },
        }
    }
    let report = SaveReport {
        dir: dir.clone(),
        generation: saved.generation,
        files: keep.len() + 1,
        written: files.len(),
        removed,
        leftovers,
    };
    Ok((saved, report))
}

/// Removes the files a failed attempt created (never listed by any `world.json`, so always
/// safe) and passes the error on.
fn undo(created: &[PathBuf], error: PersistError) -> PersistError {
    for path in created {
        let _ = std::fs::remove_file(path);
    }
    error
}

/// Writes the world into `dir` (created if needed). `previous` as for [`plan`].
pub fn save(world: &World, dir: &Path, previous: Option<&Saved>) -> Result<(Saved, SaveReport), PersistError> {
    execute(plan(world, dir, previous))
}

/// Reads the world saved in `dir`, and what [`plan`] needs to save it again incrementally.
pub fn load(dir: &Path) -> Result<(World, Saved), PersistError> {
    let (world, mut saved) = load_with(|file| {
        std::fs::read_to_string(dir.join(file)).map_err(|e| {
            let code = if e.kind() == std::io::ErrorKind::NotFound { "not_found" } else { "io" };
            PersistError::new(code, format!("{}: {e}", dir.join(file).display())).in_file(file)
        })
    })?;
    saved.dir = dir.to_owned();
    Ok((world, saved))
}

/// [`load`] over any file source (tests read from memory). Only files listed in `world.json` are
/// read.
pub fn load_with(mut read: impl FnMut(&str) -> Result<String, PersistError>) -> Result<(World, Saved), PersistError> {
    let index = parse_doc(INDEX, &read(INDEX)?)?;
    let bad = |field: &str, message: &str| PersistError::new("invalid_world", message).in_file(INDEX).field(field);
    check_keys(&index, INDEX, &[INDEX_KEYS]).map_err(|m| bad(&quoted_field(&m).unwrap_or_default(), &m))?;
    let generation = index.get("generation").and_then(Json::as_u64).ok_or_else(|| bad("generation", "'generation' must be an integer"))?;
    let next_id = index.get("next_id").and_then(Json::as_u64).ok_or_else(|| bad("next_id", "'next_id' must be an integer"))?;
    let env = index.get("env").ok_or_else(|| bad("env", "'env' is missing"))?;
    let env = check_keys(env, "env", &[ENV_KEYS])
        .and_then(|()| parse_env(env))
        .map_err(|m| bad(&format!("env.{}", quoted_field(&m).unwrap_or_default()), &m))?;
    let listed: Vec<(String, String)> = match index.get("subtrees").and_then(Json::as_array) {
        Some(items) => items
            .iter()
            .map(|s| {
                let root = s.get("root").and_then(Json::as_str);
                let file = s.get("file").and_then(Json::as_str).filter(|f| is_subtree_file(f));
                let keys_ok = s.as_object().is_some_and(|o| o.len() == 2);
                match (root, file, keys_ok) {
                    (Some(r), Some(f), true) => Ok((r.to_owned(), f.to_owned())),
                    _ => Err(bad("subtrees", "'subtrees' entries must be {root: \"/name\", file: \"name.<gen>.json\"}")),
                }
            })
            .collect::<Result<_, _>>()?,
        None => return Err(bad("subtrees", "'subtrees' must be an array")),
    };

    // Only layouts the writer produces are accepted, so the next save can never drop anything:
    // each subtree root is a distinct top-level path, isn't also a loose record, and its file
    // starts with the root's own record; loose records have no children.
    let mut roots = BTreeSet::new();
    for (root, _) in &listed {
        if root.matches('/').count() != 1 || !root.starts_with('/') {
            return Err(bad("subtrees", &format!("subtree root '{root}' must be a top-level path like /name")));
        }
        if !roots.insert(root.as_str()) {
            return Err(bad("subtrees", &format!("subtree root '{root}' is listed twice")));
        }
    }
    let loose_paths: Vec<&str> = index
        .get("entities")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|r| r.get("path").and_then(Json::as_str))
        .collect();
    if let Some(dup) = loose_paths.iter().find(|p| roots.contains(*p)) {
        let mut e = PersistError::new("invalid_world", format!("{dup} is both a loose entity and a listed subtree root"))
            .in_file(INDEX)
            .field("path");
        e.entity = Some((*dup).to_owned());
        return Err(e);
    }

    // Older files have no clock: their animations started on a clock at 0.
    let clock_s = match index.get("clock_s") {
        None => 0.0,
        Some(v) => v.as_f64().filter(|c| c.is_finite() && *c >= 0.0).ok_or_else(|| bad("clock_s", "'clock_s' must be a number of seconds, at least 0"))?,
    };
    let mut world = World::default();
    world.set_clock_s(clock_s);
    world.set_env(&env).map_err(|e| from_world(e, INDEX, None))?;
    load_entities(&mut world, INDEX, &index, None)?;
    let mut saved = Saved { dir: PathBuf::new(), generation, subtrees: BTreeMap::new(), loose: None };
    for (root, file) in &listed {
        let doc = parse_doc(file, &read(file)?)?;
        check_keys(&doc, file, &[SUBTREE_KEYS])
            .map_err(|m| PersistError::new("invalid_world", m.clone()).in_file(file).field(&quoted_field(&m).unwrap_or_default()))?;
        if doc.get("root").and_then(Json::as_str) != Some(root.as_str()) {
            return Err(PersistError::new("invalid_world", format!("'root' must be {root}, as world.json lists it"))
                .in_file(file)
                .field("root"));
        }
        let first = doc.get("entities").and_then(Json::as_array).and_then(|e| e.first()).and_then(|r| r.get("path"));
        if first.and_then(Json::as_str) != Some(root.as_str()) {
            return Err(PersistError::new("invalid_world", format!("the first entity must be the root {root} itself"))
                .in_file(file)
                .field("entities"));
        }
        load_entities(&mut world, file, &doc, Some(root))?;
        let id = world.find_path(root).map_err(|e| from_world(e, file, Some(root)))?;
        saved.subtrees.insert(id, SavedSubtree { stamp: 0, root: root.clone(), file: file.clone() });
    }
    for path in loose_paths {
        let id = world.find_path(path).map_err(|e| from_world(e, INDEX, Some(path)))?;
        if world.children_of(Some(id)).next().is_some() {
            let mut e = PersistError::new(
                "invalid_world",
                format!("{path} is stored as a loose entity in world.json but has children in a subtree file"),
            )
            .in_file(INDEX)
            .field("path");
            e.entity = Some(path.to_owned());
            return Err(e);
        }
    }
    world.set_next_id(next_id);
    for (&id, s) in saved.subtrees.iter_mut() {
        s.stamp = world.subtree_stamp(id);
    }
    Ok((world, saved))
}

fn parse_doc(file: &str, text: &str) -> Result<Json, PersistError> {
    let doc = Json::parse(text).map_err(|e| PersistError::new("invalid_json", e.to_string()).in_file(file).field(&format!("byte {}", e.offset)))?;
    match doc.get("format").and_then(Json::as_str) {
        Some(FORMAT) => Ok(doc),
        other => Err(PersistError::new(
            "unsupported_format",
            format!("format is {}, this engine reads {FORMAT}", other.map_or("missing".into(), |f| format!("'{f}'"))),
        )
        .in_file(file)
        .field("format")),
    }
}

/// The first `'quoted'` word of a validation message: the field it is about.
fn quoted_field(message: &str) -> Option<String> {
    message.split('\'').nth(1).filter(|f| !f.is_empty() && !f.contains(' ')).map(str::to_owned)
}

fn from_world(e: WorldError, file: &str, entity: Option<&str>) -> PersistError {
    let field = e.detail("field").and_then(Json::as_str).map(str::to_owned);
    PersistError { code: "invalid_world", message: e.message, file: Some(file.to_owned()), entity: entity.map(str::to_owned), field }
}

fn load_entities(world: &mut World, file: &str, doc: &Json, root: Option<&str>) -> Result<(), PersistError> {
    let records = doc
        .get("entities")
        .and_then(Json::as_array)
        .ok_or_else(|| PersistError::new("invalid_world", "'entities' must be an array").in_file(file).field("entities"))?;
    for (i, rec) in records.iter().enumerate() {
        let path = rec.get("path").and_then(Json::as_str);
        let fail = |field: Option<String>, message: String| PersistError {
            code: "invalid_world",
            message,
            file: Some(file.to_owned()),
            entity: Some(path.map_or_else(|| format!("entities[{i}]"), str::to_owned)),
            field,
        };
        let Some(path) = path else { return Err(fail(Some("path".into()), "every entity needs a 'path'".into())) };
        let fields = rec.as_object().ok_or_else(|| fail(None, "entities must be objects".into()))?;
        if let Some((key, _)) = fields.iter().find(|(k, _)| !RECORD_KEYS.contains(&k.as_str())) {
            return Err(fail(Some(key.clone()), format!("unknown field '{key}'")));
        }
        let inside = match root {
            Some(r) => path == r || path.strip_prefix(r).is_some_and(|rest| rest.starts_with('/')),
            None => path.matches('/').count() == 1,
        };
        if !inside {
            let want = root.map_or("a top-level path like /name".to_owned(), |r| format!("under {r}"));
            return Err(fail(Some("path".into()), format!("'{path}' does not belong in this file: expected {want}")));
        }
        let int = |key: &str| {
            rec.get(key)
                .and_then(Json::as_u64)
                .filter(|&n| n >= 1)
                .ok_or_else(|| fail(Some(key.into()), format!("'{key}' must be an integer >= 1")))
        };
        let (id, version) = (int("id")?, int("version")?);
        let text = |key: &str| match rec.get(key) {
            Some(Json::Str(s)) => Ok(s.clone()),
            _ => Err(fail(Some(key.into()), format!("'{key}' must be a string"))),
        };
        let (created_by, modified_by) = (text("created_by")?, text("modified_by")?);
        let spec = parse_spawn_fields(rec).map_err(|m| fail(quoted_field(&m), m))?;
        world.restore(spec, id, version, &created_by, &modified_by).map_err(|e| from_world(e, file, Some(path)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Body, BodyKind, Collider, ColliderShape, EntityKey, EnvPatch, LightSpec, Profile, Shape, SpawnSpec, UpdateSpec};
    use specula_core::math::Vec3;

    fn sample() -> World {
        let mut w = World::default();
        w.set_env(&EnvPatch { sun_color: Some([0.123_456_79, 1.0 / 3.0, 0.0]), tonemap: Some(true), ..Default::default() })
            .unwrap();
        let mut slab = SpawnSpec::new(Shape::Group);
        slab.name = Some("slab".into());
        slab.position = Vec3::new(0.1, -0.0, 1.0e-7);
        w.spawn(slab, "alice").unwrap();
        let mut stones = SpawnSpec::new(Shape::Group);
        stones.name = Some("stones".into());
        stones.parent = Some(EntityKey::Path("/slab".into()));
        stones.rotation_deg = Vec3::new(0.0, 33.3, 0.0);
        w.spawn(stones, "alice").unwrap();
        for i in 0..3 {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(format!("ward_{i:02}"));
            s.parent = Some(EntityKey::Path("/slab/stones".into()));
            s.position = Vec3::new(i as f32 * 0.7, 0.25, -1.1);
            s.scale = Vec3::new(0.3, 0.5, 0.3);
            s.color = [0.31, 0.62, 0.93];
            s.tags = ["ward".to_string()].into();
            s.props = Json::parse(r#"{"hp": 12.5, "faction": "old"}"#).unwrap();
            s.collider = Some(Collider {
                shape: ColliderShape::Box { size: Vec3::ONE },
                offset: Vec3::ZERO,
                is_trigger: i == 1,
                layer: "terrain".into(),
            });
            s.body = Some(Body { kind: BodyKind::Static, mass: 1.0 });
            w.spawn(s, "bob").unwrap();
        }
        let mut glass = SpawnSpec::new(Shape::Lathe);
        glass.name = Some("Vase (tall)".into());
        glass.profile = Some(Profile { points: vec![[0.1, 0.0], [0.15, 0.2], [0.05, 0.4]], segments: 24 });
        glass.look.material = Material::Glass;
        glass.look.opacity = 1.0;
        glass.look.surface.color2 = [0.2, 0.4, 0.6];
        glass.light =
            Some(LightSpec { intensity: 0.7, range: 2.5, color: Some([1.0, 0.9, 0.8]), offset: Vec3::new(0.0, 0.2, 0.0) });
        w.spawn(glass, "carol").unwrap();
        let mut sun = SpawnSpec::new(Shape::Sphere);
        sun.name = Some("sun".into());
        sun.position = Vec3::new(0.0, 1.0e6, -3.0e5);
        w.spawn(sun, "carol").unwrap();
        // A later edit, so versions and modified_by differ from a fresh spawn.
        let edit = UpdateSpec { position: Some(Vec3::new(9.9, 0.0, 0.0)), ..Default::default() };
        w.update(&EntityKey::Path("/slab/stones/ward_01".into()), edit, "dave").unwrap();
        w
    }

    fn files(world: &World) -> BTreeMap<String, String> {
        render(world).into_iter().collect()
    }

    fn reload(files: &BTreeMap<String, String>) -> Result<World, PersistError> {
        load_with(|f| files.get(f).cloned().ok_or_else(|| PersistError::new("not_found", f).in_file(f))).map(|(w, _)| w)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-persist-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        names
    }

    #[test]
    fn save_then_load_is_bit_exact() {
        let w = sample();
        let saved = files(&w);
        let back = reload(&saved).unwrap();
        assert_eq!(back.state_hash(), w.state_hash());
        assert_eq!(back.len(), w.len());
        assert_eq!(back.next_id(), w.next_id());
        for e in w.entities() {
            let b = back.get(e.id).unwrap();
            assert_eq!(b.to_json(&back.path_of(b.id)), e.to_json(&w.path_of(e.id)));
            assert_eq!(b.model().to_cols_array().map(f32::to_bits), e.model().to_cols_array().map(f32::to_bits));
        }
        assert_eq!(files(&back), saved, "saving the loaded world gives the same bytes");
    }

    #[test]
    fn models_visibility_and_layers_save_and_transient_entities_do_not() {
        use crate::world::{Layer, ModelSpec};
        let mut w = sample();
        let mut stone = SpawnSpec::model(ModelSpec::new("recipes/warding_stone.json").with_param("tier", 2.0).with_seed(41));
        stone.name = Some("ward_model".into());
        stone.parent = Some(EntityKey::Path("/slab".into()));
        stone.color = [0.9, 1.0, 0.95];
        stone.visible = false;
        w.spawn(stone, "erin").unwrap();
        let mut ui = SpawnSpec::new(Shape::Group);
        ui.name = Some("ui".into());
        ui.layer = Layer::Ui;
        w.spawn(ui, "game").unwrap();
        let before = files(&w);
        let hash = w.state_hash();
        // Game presentation: a transient top-level entity and transient children of saved ones.
        let mut fx = SpawnSpec::new(Shape::Sphere);
        fx.name = Some("flash".into());
        fx.transient = true;
        w.spawn(fx, "game").unwrap();
        for parent in ["/slab", "/sun", "/ui"] {
            let mut enemy = SpawnSpec::new(Shape::Cube);
            enemy.name = Some("enemy".into());
            enemy.parent = Some(EntityKey::Path(parent.into()));
            enemy.transient = true;
            w.spawn(enemy, "game").unwrap();
        }
        assert_eq!(files(&w), before, "transient entities change no file, not even the layout");
        assert_eq!(w.state_hash(), hash);
        let back = reload(&before).unwrap();
        assert_eq!((back.len(), back.state_hash()), (w.persistent_len(), hash));
        let m = back.get(back.find_path("/slab/ward_model").unwrap()).unwrap();
        assert_eq!(m.model, Some(ModelSpec::new("recipes/warding_stone.json").with_param("tier", 2.0).with_seed(41)));
        assert!(!m.visible && m.look.surface.seed == 0.0);
        assert_eq!(back.get(back.find_path("/ui").unwrap()).unwrap().layer, Layer::Ui);
        assert!(before["world.json"].contains(r#""entity_count": 9,"#), "{}", before["world.json"]);
    }

    /// An animation set at clock 6 s, saved, then loaded by an engine whose clock restarted at 0:
    /// it continues from the phase it was saved at instead of freezing for 6 s. Saving changes
    /// nothing about the state (the hash doesn't depend on when a save happened).
    #[test]
    fn animations_keep_their_phase_across_a_restart() {
        use crate::world::{AnimSpec, ModelSpec};
        let mut w = sample();
        let id = w.spawn(SpawnSpec::model(ModelSpec::new("recipes/shambler.json")), "erin").unwrap().id;
        w.set_clock_s(6.0);
        w.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(AnimSpec::new("walk"))), ..Default::default() }, "erin").unwrap();
        w.set_clock_s(6.5); // saved half a second into the walk
        let hash = w.state_hash();
        let saved = files(&w);
        assert_eq!(w.state_hash(), hash, "saving changes nothing");
        assert!(saved["world.json"].contains(r#""clock_s": 6.5,"#), "{}", saved["world.json"]);
        assert_eq!(reload(&saved).unwrap().state_hash(), hash, "loaded on its own clock, it is the same state");
        // A new run: the engine's clock is at 0.25 when it loads the world.
        let mut back = reload(&saved).unwrap();
        back.rebase_clock(0.25);
        let a = back.get(id).unwrap().anim.clone().unwrap();
        assert_eq!(a.start, Some(-0.25), "6 s on the old clock is -0.25 s on the new one");
        assert!((a.clip_time(0.25) - 0.5).abs() < 1e-9, "at load it is half a second in, as when saved");
        assert!(a.clip_time(1.25) > a.clip_time(0.25), "and it keeps moving");
        // Saved again on the new clock and loaded once more: the phase still holds.
        back.set_clock_s(1.25);
        let mut again = reload(&files(&back)).unwrap();
        again.rebase_clock(0.0);
        assert!((again.get(id).unwrap().anim.as_ref().unwrap().clip_time(0.0) - 1.5).abs() < 1e-9);
        // Older files without a clock: starts are on a clock at 0.
        let old = saved.iter().map(|(k, v)| (k.clone(), v.replace("\"clock_s\": 6.5,", ""))).collect();
        assert_eq!(reload(&old).unwrap().clock_s(), 0.0);
    }

    #[test]
    fn layout_is_one_file_per_subtree() {
        let saved = files(&sample());
        let names: Vec<&str> = saved.keys().map(String::as_str).collect();
        assert_eq!(names, ["slab.1.json", "world.json"]);
        let index = &saved["world.json"];
        assert!(index.contains(r#""subtrees": [{"root":"/slab","file":"slab.1.json"}]"#), "{index}");
        assert!(index.contains(r#""generation": 1,"#), "{index}");
        // Loose top-level entities live in world.json, one line each, in path order.
        let lines: Vec<&str> = index.lines().filter(|l| l.trim_start().starts_with("{\"path\"")).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(r#""path":"/Vase (tall)""#) && lines[1].contains(r#""path":"/sun""#));
        assert!(lines[0].contains(r#""opacity":1"#), "non-standard materials always store opacity");
        let slab = &saved["slab.1.json"];
        assert_eq!(slab.lines().filter(|l| l.contains("\"path\"")).count(), 5);
        assert!(slab.contains(r#""position":[0.1,-0,0.0000001]"#), "{slab}");
        assert!(slab.contains(r#""props":{"faction":"old","hp":12.5}"#));
    }

    #[test]
    fn new_stems_are_safe_and_distinct() {
        let mut taken = BTreeSet::new();
        let stems: Vec<String> =
            ["slab", "Slab", "a/b", "con", "world", "x.y", "a_b"].iter().map(|n| new_stem(n, &mut taken)).collect();
        assert_eq!(stems[0], "slab");
        assert!(stems[1].starts_with("Slab-"), "{stems:?}");
        assert_eq!(stems[2], "a_b");
        assert!(stems[3].starts_with("con-"), "{stems:?}");
        assert!(stems[4].starts_with("world-"), "{stems:?}");
        assert_eq!(stems[5], "x_y");
        assert!(stems[6].starts_with("a_b-"), "clashes with a/b: {stems:?}");
        assert_eq!(taken.len(), stems.len());
        assert!(stems.iter().all(|s| is_subtree_file(&format!("{s}.3.json"))));
        assert!(!is_subtree_file("slab.json") && !is_subtree_file("../slab.1.json") && !is_subtree_file("world.json"));
    }

    #[test]
    fn world_names_cannot_escape() {
        for ok in ["greybox", "sepulcra_v2", "a.b-c", "_scratch", "7"] {
            assert!(validate_world_name(ok).is_ok(), "{ok}");
        }
        for bad in ["", "..", ".", ".hidden", "-x", "a/b", "a\\b", "C:", "/abs", "x.", "con", "NUL.txt", "lpt1", "a b", &"x".repeat(65)] {
            assert!(validate_world_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn malformed_files_give_located_errors() {
        let good = files(&sample());
        let broken = |file: &str, from: &str, to: &str| {
            let mut f = good.clone();
            let text = f.get_mut(file).unwrap();
            assert!(text.contains(from), "{from} not in {file}");
            *text = text.replacen(from, to, 1);
            reload(&f).unwrap_err()
        };
        let slab = "slab.1.json";
        let e = broken(slab, r#""shape":"cube""#, r#""shape":"cone""#);
        assert_eq!((e.file.as_deref(), e.entity.as_deref(), e.field.as_deref()), (Some(slab), Some("/slab/stones/ward_00"), Some("shape")));
        let e = broken(slab, r#""scale":[0.3,0.5,0.3]"#, r#""scale":[0,0.5,0.3]"#);
        assert_eq!(e.field.as_deref(), Some("scale"));
        let e = broken(slab, r#""color":[0.31,0.62,0.93]"#, r#""color":[3.1,0.62,0.93]"#);
        assert_eq!(e.field.as_deref(), Some("color"));
        let e = broken(slab, r#""version":1"#, r#""version":0"#);
        assert_eq!(e.field.as_deref(), Some("version"));
        let e = broken(slab, r#""path":"/slab/stones/ward_02""#, r#""path":"/slab/stones/ward_00""#);
        assert_eq!((e.entity.as_deref(), e.field.as_deref()), (Some("/slab/stones/ward_00"), None));
        assert!(e.message.contains("already exists"), "{e}");
        let e = broken(slab, r#""path":"/slab/stones/ward_02""#, r#""path":"/elsewhere/ward_02""#);
        assert_eq!(e.field.as_deref(), Some("path"));
        let e = broken(slab, r#""path":"/slab/stones","#, r#""path":"/slab/stones","colour":1,"#);
        assert_eq!(e.field.as_deref(), Some("colour"));
        let e = broken(slab, r#""layer":"terrain""#, r#""layer":"terrain","mask":3"#);
        assert_eq!(e.field.as_deref(), Some("collider.mask"), "{e}");
        let e = broken("world.json", r#""intensity":0.7"#, r#""intensity":0.7,"radius":2"#);
        assert_eq!(e.field.as_deref(), Some("light.radius"), "{e}");
        let e = broken(slab, "\"entities\": [", "\"entities\": [[");
        assert_eq!((e.code, e.file.as_deref()), ("invalid_json", Some(slab)));
        let e = broken(slab, r#""root": "/slab""#, r#""root": "/slab", "extra": 1"#);
        assert_eq!(e.field.as_deref(), Some("extra"));
        let e = broken(slab, r#""root": "/slab""#, r#""root": "/other""#);
        assert_eq!(e.field.as_deref(), Some("root"));
        let e = broken("world.json", FORMAT, "specula.world/v9");
        assert_eq!(e.code, "unsupported_format");
        let e = broken("world.json", r#""next_id": 7,"#, r#""next_id": 7, "notes": "x","#);
        assert_eq!(e.field.as_deref(), Some("notes"));
        let e = broken("world.json", r#""file":"slab.1.json""#, r#""file":"../slab.1.json""#);
        assert_eq!(e.field.as_deref(), Some("subtrees"));
        let e = broken("world.json", r#""file":"slab.1.json""#, r#""file":"gone.1.json""#);
        assert_eq!((e.code, e.file.as_deref()), ("not_found", Some("gone.1.json")));
        let e = broken("world.json", r#""exposure":1"#, r#""exposure":-1"#);
        assert_eq!(e.field.as_deref(), Some("env.exposure"));
        let e = broken("world.json", r#""exposure":1"#, r#""exposure":1,"fog":2"#);
        assert_eq!(e.field.as_deref(), Some("env.fog"));
        let e = broken("world.json", r#""path":"/sun""#, r#""path":"/sun/x""#);
        assert_eq!(e.field.as_deref(), Some("path"));
        // A stale id counter is repaired rather than refused: ids are never handed out twice.
        let mut low = good.clone();
        let index = low.get_mut("world.json").unwrap();
        *index = index.replacen(r#""next_id": 7"#, r#""next_id": 2"#, 1);
        assert_eq!(reload(&low).unwrap().next_id(), 7);
    }

    #[test]
    fn saves_commit_through_world_json_and_rewrite_only_changes() {
        let dir = temp_dir("commit");
        let mut w = sample();
        let mut cul = SpawnSpec::new(Shape::Group);
        cul.name = Some("culvert".into());
        w.spawn(cul, "t").unwrap();
        let mut pipe = SpawnSpec::new(Shape::Cube);
        pipe.parent = Some(EntityKey::Path("/culvert".into()));
        w.spawn(pipe, "t").unwrap();
        let (saved, first) = save(&w, &dir, None).unwrap();
        assert_eq!((first.generation, first.files, first.written, first.removed), (1, 3, 3, 0));
        assert_eq!(listing(&dir), ["culvert.1.json", "slab.1.json", "world.json"]);

        // Change one subtree: only it and world.json are written; its old file goes.
        let edit = UpdateSpec { color: Some([1.0, 0.0, 0.0]), ..Default::default() };
        w.update(&EntityKey::Path("/culvert/cube_9".into()), edit, "t").unwrap();
        let (saved, second) = save(&w, &dir, Some(&saved)).unwrap();
        assert_eq!((second.generation, second.written, second.removed), (2, 2, 1));
        assert_eq!(listing(&dir), ["culvert.2.json", "slab.1.json", "world.json"]);
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());

        // A crashed save's stray file is never listed: the world still loads as committed, the
        // next save picks a generation above it, and sweeps it.
        std::fs::write(dir.join("slab.3.json"), format!("{{\n  \"format\": \"{FORMAT}\",\n  \"root\": \"/sl")).unwrap();
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());

        // Files that aren't named like ours are never deleted; a new top-level name that clashes
        // takes the hash and leaves the existing file's name alone.
        std::fs::write(dir.join("notes.json"), "{}").unwrap();
        let mut rival = SpawnSpec::new(Shape::Group);
        rival.name = Some("Slab".into());
        w.spawn(rival, "t").unwrap();
        let mut kid = SpawnSpec::new(Shape::Cube);
        kid.parent = Some(EntityKey::Path("/Slab".into()));
        w.spawn(kid, "t").unwrap();
        w.delete(&EntityKey::Path("/culvert".into()), None).unwrap();
        let (_, third) = save(&w, &dir, Some(&saved)).unwrap();
        assert_eq!((third.generation, third.removed), (4, 2), "culvert.2.json and the stray slab.3.json");
        let names = listing(&dir);
        assert!(names.contains(&"notes.json".to_string()) && names.contains(&"slab.1.json".to_string()), "{names:?}");
        assert!(names.iter().any(|n| n.starts_with("Slab-") && n.ends_with(".4.json")), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("culvert") || n == "slab.3.json"), "{names:?}");
        let (back, reread) = load(&dir).unwrap();
        assert_eq!(back.state_hash(), w.state_hash());

        // Saving the loaded world without changes writes only world.json.
        let (_, fourth) = save(&back, &dir, Some(&reread)).unwrap();
        assert_eq!((fourth.written, fourth.removed), (1, 0));
        assert_eq!(peek_entity_count(&dir).unwrap(), Some(back.len() as u64));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(load(&dir).err().unwrap().code, "not_found");
        assert_eq!(peek_entity_count(&dir).unwrap(), None);
    }

    #[test]
    fn folders_that_are_not_worlds_are_left_alone() {
        let dir = temp_dir("adopt");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(check_adoptable(&dir).is_ok(), "an empty folder is fine");
        std::fs::write(dir.join("notes.json"), "{}").unwrap();
        assert_eq!(save(&sample(), &dir, None).err().unwrap().code, "not_a_world");
        assert_eq!(check_adoptable(&dir).err().unwrap().code, "not_a_world");
        assert_eq!(listing(&dir), ["notes.json"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Reviewer finding: a first save that crashed before committing left only our own files, and
    /// the folder was refused forever.
    #[test]
    fn a_first_save_that_never_committed_is_adopted_and_swept() {
        let dir = temp_dir("first-crash");
        let w = sample();
        assert!(execute_with(plan(&w, &dir, None), &mut |s| if s == Step::Commit { Hook::Crash } else { Hook::Go }).is_err());
        assert_eq!(listing(&dir), ["slab.1.json", "world.json.tmp"]);
        assert!(check_adoptable(&dir).is_ok());
        let (_, report) = save(&w, &dir, None).unwrap();
        assert_eq!((report.generation, report.removed), (2, 1), "slab.1.json (the stale .tmp was replaced)");
        assert_eq!(listing(&dir), ["slab.2.json", "world.json"]);
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Every way a save can be cut short leaves a world that loads as either the old or the new
    /// state; the save after it succeeds, never opens a committed file for writing, and sweeps
    /// what the cut left behind.
    #[test]
    fn saves_survive_being_cut_at_every_step() {
        let old = {
            let mut w = sample();
            let mut g = SpawnSpec::new(Shape::Group);
            g.name = Some("dock".into());
            w.spawn(g, "t").unwrap();
            let mut c = SpawnSpec::new(Shape::Cube);
            c.parent = Some(EntityKey::Path("/dock".into()));
            w.spawn(c, "t").unwrap();
            w
        };
        let new = {
            let mut w = old.clone();
            let edit = UpdateSpec { color: Some([0.0, 1.0, 0.0]), ..Default::default() };
            w.update(&EntityKey::Path("/slab/stones/ward_00".into()), edit, "t").unwrap();
            w.delete(&EntityKey::Path("/dock".into()), None).unwrap();
            w.set_env(&EnvPatch { exposure: Some(1.5), ..Default::default() }).unwrap();
            w
        };
        // The steps of the old → new save: write slab, write world.json.tmp, commit, remove the
        // old slab and dock files.
        let mut case = 0;
        loop {
            let dir = temp_dir(&format!("cut-{case}"));
            let (old_saved, _) = save(&old, &dir, None).unwrap();
            let mut steps = 0;
            let mut fail_removals = false;
            let cut = execute_with(plan(&new, &dir, Some(&old_saved)), &mut |step| {
                steps += 1;
                // Cases 0..n crash at step n; the last case fails every removal instead.
                match (steps == case + 1, step) {
                    (true, _) if case < 5 => Hook::Crash,
                    (_, Step::Remove(_)) if case == 5 => {
                        fail_removals = true;
                        Hook::Fail
                    }
                    _ => Hook::Go,
                }
            });
            let (on_disk, _) = load(&dir).unwrap();
            let hash = on_disk.state_hash();
            assert!(hash == old.state_hash() || hash == new.state_hash(), "case {case}: a mix of old and new");
            if case == 5 {
                let (_, report) = cut.as_ref().unwrap();
                assert!(fail_removals && report.leftovers.len() == 2, "case 5: removal failures are only leftovers");
                assert_eq!(hash, new.state_hash());
            }
            // The committed files must survive the next save untouched (or be swept, unlisted).
            let committed: Vec<(String, String)> = std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_subtree_file(n))
                .filter(|n| std::fs::read_to_string(dir.join(INDEX)).unwrap().contains(n.as_str()))
                .map(|n| (n.clone(), std::fs::read_to_string(dir.join(&n)).unwrap()))
                .collect();
            // The engine keeps its old cache when a save fails (or, at worst, a stale one).
            let previous = cut.as_ref().map(|(s, _)| s.clone()).unwrap_or(old_saved);
            let (_, next) = save(&new, &dir, Some(&previous)).unwrap();
            assert!(next.leftovers.is_empty(), "case {case}: {:?}", next.leftovers);
            let (back, _) = load(&dir).unwrap();
            assert_eq!(back.state_hash(), new.state_hash(), "case {case}");
            for (name, content) in committed {
                if let Ok(now) = std::fs::read_to_string(dir.join(&name)) {
                    assert_eq!(now, content, "case {case}: {name} was rewritten");
                }
            }
            let names = listing(&dir);
            assert!(names.iter().all(|n| n == INDEX || n.starts_with("slab.")), "case {case}: not swept: {names:?}");
            std::fs::remove_dir_all(&dir).unwrap();
            assert!(case == 5 || steps == case + 1, "case {case}: the cut never happened ({steps} steps)");
            case += 1;
            if case > 5 {
                break;
            }
        }
    }

    /// Reviewer repro: an old file held open (can't be deleted) failed the save after it had
    /// committed, and the next save reused the generation and rewrote a committed file.
    #[test]
    fn a_stale_cache_never_reuses_a_committed_generation() {
        let dir = temp_dir("stale");
        let mut w = sample();
        let (first, _) = save(&w, &dir, None).unwrap();
        let edit = UpdateSpec { color: Some([0.0, 0.0, 1.0]), ..Default::default() };
        w.update(&EntityKey::Path("/slab/stones/ward_02".into()), edit.clone(), "t").unwrap();
        let fail = &mut |s: Step| if matches!(s, Step::Remove(_)) { Hook::Fail } else { Hook::Go };
        let (_, report) = execute_with(plan(&w, &dir, Some(&first)), fail).unwrap();
        assert_eq!((report.generation, report.leftovers.as_slice()), (2, ["slab.1.json".to_string()].as_slice()));
        let committed = std::fs::read_to_string(dir.join("slab.2.json")).unwrap();
        // Even an engine that kept the old cache (generation 1) must write generation 3.
        w.update(&EntityKey::Path("/slab/stones/ward_02".into()), edit, "t").unwrap();
        let (_, next) = save(&w, &dir, Some(&first)).unwrap();
        assert_eq!(next.generation, 3);
        assert_eq!(listing(&dir), ["slab.3.json", "world.json"], "slab.2.json was swept, never rewritten");
        assert_ne!(std::fs::read_to_string(dir.join("slab.3.json")).unwrap(), committed);
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Reviewer repro: saves that failed before the commit left their new files behind, and
    /// every retry (autosave every 10 s) added a full copy.
    #[test]
    fn failed_saves_clean_up_after_themselves() {
        let dir = temp_dir("precommit");
        let mut w = sample();
        let (saved, _) = save(&w, &dir, None).unwrap();
        let before = listing(&dir);
        let edit = UpdateSpec { color: Some([0.0, 0.0, 1.0]), ..Default::default() };
        w.update(&EntityKey::Path("/slab/stones/ward_02".into()), edit, "t").unwrap();
        for cut in [Step::WriteIndex, Step::Commit] {
            for _ in 0..3 {
                let e = execute_with(plan(&w, &dir, Some(&saved)), &mut |s| if s == cut { Hook::Fail } else { Hook::Go });
                assert!(!e.err().unwrap().invalidates_cache(), "nothing listed was touched");
                assert_eq!(listing(&dir), before, "{cut:?}: the attempt's files are gone");
            }
        }
        #[cfg(windows)]
        {
            // The reviewer's case: world.json can't be replaced.
            let index = dir.join(INDEX);
            let mut perms = std::fs::metadata(&index).unwrap().permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(&index, perms.clone()).unwrap();
            for _ in 0..5 {
                assert_eq!(save(&w, &dir, Some(&saved)).err().unwrap().code, "io");
            }
            assert_eq!(listing(&dir), before, "five failed saves leave nothing behind");
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(&index, perms).unwrap();
        }
        // The cache kept after those failures is still right.
        save(&w, &dir, Some(&saved)).unwrap();
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Reviewer findings: files merely *named* like ours were adopted and swept; a huge suffix
    /// overflowed; an upper-case `.JSON` stray made every save fail.
    #[test]
    fn only_files_that_are_ours_are_touched() {
        let dir = temp_dir("foreign");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.2026.json"), "{\"todo\": [\"walls\"]}").unwrap();
        assert_eq!(check_adoptable(&dir).err().unwrap().code, "not_a_world", "a lone user file isn't a crashed save");
        std::fs::remove_file(dir.join("notes.2026.json")).unwrap();

        let w = sample();
        save(&w, &dir, None).unwrap();
        std::fs::write(dir.join("notes.2026.json"), "{\"todo\": []}").unwrap();
        std::fs::write(dir.join("g.18446744073709551615.json"), "{}").unwrap();
        std::fs::write(dir.join("g.9007199254740993.json"), "{}").unwrap();
        std::fs::write(dir.join("slab.2027.JSON"), "x").unwrap();
        let (_, report) = save(&w, &dir, None).unwrap();
        assert_eq!(report.generation, 2028, "past every suffix, any case, but not the absurd ones");
        let names = listing(&dir);
        for kept in ["notes.2026.json", "g.18446744073709551615.json", "g.9007199254740993.json", "slab.2027.JSON"] {
            assert!(names.contains(&kept.to_string()), "{kept} must stay: {names:?}");
        }
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());
        assert_eq!(generation_of("a.1.Json"), Some(1));
        assert_eq!(generation_of("a.18446744073709551616.json"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_world_folder_has_one_owner() {
        let dir = temp_dir("lock");
        let lock = WorldLock::acquire(&dir).unwrap();
        assert_eq!(lock.dir(), dir.as_path());
        #[cfg(windows)]
        {
            let e = WorldLock::acquire(&dir).err().unwrap();
            assert_eq!(e.code, "world_locked", "{e}");
        }
        assert!(check_adoptable(&dir).is_ok(), "the lock file doesn't make a folder foreign");
        let w = sample();
        save(&w, &dir, None).unwrap();
        assert!(listing(&dir).contains(&LOCK.to_string()), "saves leave the lock alone");
        drop(lock);
        let again = WorldLock::acquire(&dir).unwrap();
        drop(again);
        assert!(!listing(&dir).contains(&LOCK.to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn peeking_reads_only_the_header() {
        let dir = temp_dir("peek");
        let w = sample();
        save(&w, &dir, None).unwrap();
        assert_eq!(peek_entity_count(&dir).unwrap(), Some(w.len() as u64));
        // Garbage after the header doesn't matter: it is never read.
        let index = dir.join(INDEX);
        let text = std::fs::read_to_string(&index).unwrap();
        let cut = text.find("\"env\"").unwrap();
        std::fs::write(&index, format!("{}{}", &text[..cut], "x".repeat(1 << 20))).unwrap();
        assert_eq!(peek_entity_count(&dir).unwrap(), Some(w.len() as u64));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Reviewer repro: layouts the writer never produces loaded fine and the next save dropped
    /// entities. They are refused now.
    #[test]
    fn layouts_the_writer_cannot_keep_are_refused() {
        let index = |loose: &str, subtrees: &str| {
            format!(
                "{{\"format\":\"{FORMAT}\",\"generation\":2,\"next_id\":9,\"env\":{{}},\"subtrees\":[{subtrees}],\"entities\":[{loose}]}}"
            )
        };
        let rec = |path: &str, id: u64| {
            format!(r#"{{"path":"{path}","id":{id},"shape":"group","version":1,"created_by":"t","modified_by":"t"}}"#)
        };
        let file = |root: &str, recs: &[String]| format!("{{\"format\":\"{FORMAT}\",\"root\":\"{root}\",\"entities\":[{}]}}", recs.join(","));
        let try_load = |files: &[(&str, String)]| {
            let map: BTreeMap<String, String> = files.iter().map(|(n, c)| (n.to_string(), c.clone())).collect();
            reload(&map)
        };
        let a = file("/slab", &[rec("/slab", 1), rec("/slab/a", 2)]);
        let b = file("/slab", &[rec("/slab/b", 3)]);
        let twice = r#"{"root":"/slab","file":"slab.1.json"},{"root":"/slab","file":"slab.2.json"}"#;
        // The reviewer's case: /slab loose and listed twice, the second file without its root.
        let e = try_load(&[("world.json", index(&rec("/slab", 1), twice)), ("slab.1.json", a.clone()), ("slab.2.json", b.clone())]);
        assert_eq!(e.err().unwrap().code, "invalid_world");
        let e = try_load(&[("world.json", index("", twice)), ("slab.1.json", a.clone()), ("slab.2.json", b)]).err().unwrap();
        assert!(e.message.contains("listed twice"), "{e}");
        let one = r#"{"root":"/slab","file":"slab.1.json"}"#;
        let e = try_load(&[("world.json", index(&rec("/slab", 1), one)), ("slab.1.json", a.clone())]).err().unwrap();
        assert!(e.message.contains("both a loose entity"), "{e}");
        let rootless = file("/slab", &[rec("/slab/a", 2)]);
        let e = try_load(&[("world.json", index("", one)), ("slab.1.json", rootless)]).err().unwrap();
        assert_eq!(e.field.as_deref(), Some("entities"), "{e}");
        let kid_elsewhere = file("/dock", &[rec("/dock", 4), rec("/dock/x", 5)]);
        let one_dock = r#"{"root":"/dock","file":"dock.1.json"}"#;
        let e = try_load(&[("world.json", index(&rec("/slab", 1), one_dock)), ("dock.1.json", kid_elsewhere.replace("/dock/x", "/slab/x"))]);
        assert_eq!(e.err().unwrap().field.as_deref(), Some("path"), "a subtree file holds only its own subtree");
        let deep = r#"{"root":"/slab/a","file":"slab.1.json"}"#;
        let e = try_load(&[("world.json", index("", deep)), ("slab.1.json", a.clone())]).err().unwrap();
        assert_eq!(e.field.as_deref(), Some("subtrees"), "{e}");
        // The writer's own layout still loads.
        assert_eq!(try_load(&[("world.json", index("", one)), ("slab.1.json", a)]).unwrap().len(), 2);
    }

    #[test]
    fn unchanged_loose_entities_are_not_copied_again() {
        let dir = temp_dir("loose");
        let mut w = sample();
        let (first, _) = save(&w, &dir, None).unwrap();
        let edit = UpdateSpec { color: Some([0.0, 0.0, 1.0]), ..Default::default() };
        w.update(&EntityKey::Path("/slab/stones/ward_02".into()), edit, "t").unwrap();
        let p = plan(&w, &dir, Some(&first));
        assert!(matches!(p.loose, Loose::Keep(_)), "the loose section is reused");
        let (second, _) = execute(p).unwrap();
        assert!(Arc::ptr_eq(&first.loose.as_ref().unwrap().1, &second.loose.as_ref().unwrap().1));
        w.update(&EntityKey::Path("/sun".into()), UpdateSpec { scale: Some(Vec3::ONE * 2.0), ..Default::default() }, "t").unwrap();
        assert!(matches!(plan(&w, &dir, Some(&second)).loose, Loose::Write(_)), "a loose change re-renders it");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Scale check, run by hand: `cargo test -p specula -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hundred_thousand_entities() {
        let t = std::time::Instant::now();
        let mut w = World::default();
        for d in 0..100 {
            let mut g = SpawnSpec::new(Shape::Group);
            g.name = Some(format!("district_{d:02}"));
            w.spawn(g, "t").unwrap();
            let parent = format!("/district_{d:02}");
            for i in 0..999 {
                let mut s = SpawnSpec::new(Shape::Cube);
                s.name = Some(format!("house_{i:03}"));
                s.parent = Some(EntityKey::Path(parent.clone()));
                s.position = Vec3::new(d as f32 * 40.0, 0.0, i as f32 * 0.37);
                s.tags = ["house".to_string()].into();
                w.spawn(s, "t").unwrap();
            }
        }
        let built = t.elapsed();
        let t = std::time::Instant::now();
        let page = w.query(&crate::world::Query { tags: vec!["house".into()], limit: 50, ..Default::default() }).unwrap();
        assert_eq!(page.total, 99_900);
        let queried = t.elapsed();
        let dir = temp_dir("100k");
        let t = std::time::Instant::now();
        let (saved, _) = save(&w, &dir, None).unwrap();
        let first_save = t.elapsed();
        // One district changes, as a game tick would: the next save's engine-thread part.
        let edit = UpdateSpec { position: Some(Vec3::ONE), ..Default::default() };
        w.update(&EntityKey::Path("/district_42/house_007".into()), edit, "t").unwrap();
        let t = std::time::Instant::now();
        let p = plan(&w, &dir, Some(&saved));
        let planned = t.elapsed();
        let t = std::time::Instant::now();
        let (_, report) = execute(p).unwrap();
        let executed = t.elapsed();
        let t = std::time::Instant::now();
        let (back, _) = load(&dir).unwrap();
        let loaded = t.elapsed();
        assert_eq!(back.state_hash(), w.state_hash());
        assert_eq!(report.written, 2);
        println!(
            "100k entities: build {built:?}, tag query {queried:?}, first save {first_save:?}, \
             1-district change: plan {planned:?} (engine thread) + write {executed:?}, load {loaded:?}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
