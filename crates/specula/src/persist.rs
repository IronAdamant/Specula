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
//! one, never a mix. Unchanged subtrees keep their files (and names), and files the world never
//! listed (a user's notes) are never touched.
//!
//! Saving is split so the engine can write from a worker thread: [`plan`] takes what changed off
//! the live world (on the engine thread; unchanged subtrees cost nothing), and [`execute`] renders
//! and writes it.

use crate::tools::{check_keys, parse_env, parse_spawn_fields, ENV_KEYS};
use crate::world::{color_json_exact, material_name, vec3_json, Entity, Look, World, WorldError};
use specula_core::hash::hash_str;
use specula_core::Json;
use specula_gpu::Material;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "specula.world/v1";
const INDEX: &str = "world.json";
const INDEX_TMP: &str = "world.json.tmp";
/// Keys an entity record may have; anything else is a typo worth reporting.
const RECORD_KEYS: [&str; 25] = [
    "path", "id", "shape", "position", "rotation_deg", "scale", "color", "material", "opacity", "emissive",
    "metallic", "roughness", "specular", "seed", "color2", "light", "profile", "segments", "tags", "props",
    "collider", "body", "version", "created_by", "modified_by",
];
const INDEX_KEYS: &[&str] = &["format", "generation", "next_id", "env", "subtrees", "entities"];
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
}

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

/// A world save detached from the live world: owned copies of what changed, ready for
/// [`execute`] on any thread.
pub struct Plan {
    dir: PathBuf,
    next_id: u64,
    env: Json,
    /// Top-level entities without children (they live in `world.json`).
    loose: Vec<(Entity, String)>,
    parts: Vec<Part>,
    previous: Option<Saved>,
}

/// Takes what [`execute`] needs from `world`. `previous` is what was saved into `dir` before (by
/// this engine); subtrees whose stamp hasn't moved since are kept, not copied.
pub fn plan(world: &World, dir: &Path, previous: Option<&Saved>) -> Plan {
    let previous = previous.filter(|p| p.dir == dir).cloned();
    let tops: Vec<u64> = world.children_of(None).collect();
    let (subtrees, loose): (Vec<u64>, Vec<u64>) = tops.into_iter().partition(|&id| world.children_of(Some(id)).next().is_some());
    let loose = loose.into_iter().map(|id| (world.get(id).expect("listed").clone(), world.path_of(id))).collect();
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
        world.walk(Some(id), |e, path| entities.push((e.clone(), path.to_owned())));
        parts.push(Part::Write { id, stamp, root, stem, entities });
    }
    Plan { dir: dir.to_owned(), next_id: world.next_id(), env: world.env().to_json_exact(), loose, parts, previous }
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
        let mut saved = Saved { dir: self.dir.clone(), generation, subtrees: BTreeMap::new() };
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
        let loose: Vec<Json> = self.loose.iter().map(|(e, path)| record(e, path)).collect();
        let header = vec![
            ("format", Json::from(FORMAT)),
            ("generation", Json::from(generation)),
            ("next_id", Json::from(self.next_id)),
            ("env", self.env.clone()),
            ("subtrees", subtrees),
        ];
        files.push((INDEX.to_owned(), document(header, &loose)));
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
    /// Files of the previous generation that are no longer part of the world.
    pub removed: usize,
}

/// The generation and files of the world already in `dir`: `None` for a missing or empty folder.
/// A non-empty folder without `world.json` is refused rather than adopted: it isn't ours.
fn on_disk(dir: &Path) -> Result<Option<(u64, Vec<String>)>, PersistError> {
    let index = dir.join(INDEX);
    if !index.exists() {
        let occupied = std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some());
        if occupied {
            return Err(PersistError::new(
                "not_a_world",
                format!("{} has files but no world.json; refusing to save a world into it", dir.display()),
            ));
        }
        return Ok(None);
    }
    let text = std::fs::read_to_string(&index).map_err(|e| PersistError::io(INDEX, e))?;
    let doc = parse_doc(INDEX, &text)?;
    let generation = doc.get("generation").and_then(Json::as_u64).unwrap_or(0);
    let files = doc
        .get("subtrees")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| s.get("file").and_then(Json::as_str))
        .filter(|f| is_subtree_file(f))
        .map(str::to_owned)
        .collect();
    Ok(Some((generation, files)))
}

/// Whether a world may be created in `dir` (missing, empty, or already a world).
pub fn check_adoptable(dir: &Path) -> Result<(), PersistError> {
    on_disk(dir).map(|_| ())
}

fn write_synced(path: &Path, name: &str, content: &str) -> Result<(), PersistError> {
    let mut f = std::fs::File::create(path).map_err(|e| PersistError::io(name, e))?;
    f.write_all(content.as_bytes()).map_err(|e| PersistError::io(name, e))?;
    f.sync_all().map_err(|e| PersistError::io(name, e))
}

/// Writes `plan` (see the module docs for the commit order) and returns what is now on disk.
pub fn execute(plan: Plan) -> Result<(Saved, SaveReport), PersistError> {
    let dir = &plan.dir;
    let shown = dir.display().to_string();
    let (generation, old_files) = match &plan.previous {
        Some(p) => (p.generation, p.subtrees.values().map(|s| s.file.clone()).collect()),
        None => on_disk(dir)?.unwrap_or_default(),
    };
    std::fs::create_dir_all(dir).map_err(|e| PersistError::io(&shown, e))?;
    let (files, saved) = plan.render(generation + 1);
    let (index, subtrees) = files.split_last().expect("world.json is always rendered");
    for (name, content) in subtrees {
        write_synced(&dir.join(name), name, content)?;
    }
    write_synced(&dir.join(INDEX_TMP), INDEX, &index.1)?;
    std::fs::rename(dir.join(INDEX_TMP), dir.join(INDEX)).map_err(|e| PersistError::io(INDEX, e))?;
    // Committed. What the previous generation listed and this one doesn't is garbage now.
    let keep: BTreeSet<&str> = saved.subtrees.values().map(|s| s.file.as_str()).collect();
    let mut removed = 0;
    for old in old_files.iter().filter(|f| !keep.contains(f.as_str())) {
        match std::fs::remove_file(dir.join(old)) {
            Ok(()) => removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(PersistError::io(old, e)),
        }
    }
    let report =
        SaveReport { dir: dir.clone(), generation: saved.generation, files: keep.len() + 1, written: files.len(), removed };
    Ok((saved, report))
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

    let mut world = World::default();
    world.set_env(&env).map_err(|e| from_world(e, INDEX, None))?;
    load_entities(&mut world, INDEX, &index, None)?;
    let mut saved = Saved { dir: PathBuf::new(), generation, subtrees: BTreeMap::new() };
    for (root, file) in &listed {
        let doc = parse_doc(file, &read(file)?)?;
        check_keys(&doc, file, &[SUBTREE_KEYS])
            .map_err(|m| PersistError::new("invalid_world", m.clone()).in_file(file).field(&quoted_field(&m).unwrap_or_default()))?;
        if doc.get("root").and_then(Json::as_str) != Some(root.as_str()) {
            return Err(PersistError::new("invalid_world", format!("'root' must be {root}, as world.json lists it"))
                .in_file(file)
                .field("root"));
        }
        load_entities(&mut world, file, &doc, Some(root))?;
        let id = world.find_path(root).map_err(|e| from_world(e, file, Some(root)))?;
        saved.subtrees.insert(id, SavedSubtree { stamp: 0, root: root.clone(), file: file.clone() });
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

        // A crash after the subtree files but before world.json leaves the old world loadable:
        // the new generation's files are simply not listed.
        std::fs::write(dir.join("slab.3.json"), "half a file").unwrap();
        assert_eq!(load(&dir).unwrap().0.state_hash(), w.state_hash());

        // Files the world never listed are never deleted; a new top-level name that clashes takes
        // the hash and leaves the existing file's name alone.
        std::fs::write(dir.join("notes.json"), "{}").unwrap();
        let mut rival = SpawnSpec::new(Shape::Group);
        rival.name = Some("Slab".into());
        w.spawn(rival, "t").unwrap();
        let mut kid = SpawnSpec::new(Shape::Cube);
        kid.parent = Some(EntityKey::Path("/Slab".into()));
        w.spawn(kid, "t").unwrap();
        w.delete(&EntityKey::Path("/culvert".into()), None).unwrap();
        let (_, third) = save(&w, &dir, Some(&saved)).unwrap();
        let names = listing(&dir);
        assert!(names.contains(&"notes.json".to_string()) && names.contains(&"slab.1.json".to_string()), "{names:?}");
        assert!(names.iter().any(|n| n.starts_with("Slab-") && n.ends_with(".3.json")), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("culvert")), "{names:?}");
        assert_eq!(third.removed, 1);
        let (back, reread) = load(&dir).unwrap();
        assert_eq!(back.state_hash(), w.state_hash());

        // Saving the loaded world without changes writes only world.json.
        let (_, fourth) = save(&back, &dir, Some(&reread)).unwrap();
        assert_eq!((fourth.written, fourth.removed), (1, 0));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(load(&dir).err().unwrap().code, "not_found");
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
