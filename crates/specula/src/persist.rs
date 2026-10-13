//! World files: the world as deterministic, diff-friendly text.
//!
//! A world named `greybox` lives in `<worlds dir>/greybox/`:
//!
//! - `world.json` — format, the id counter, the environment, the list of subtree files, and the
//!   top-level entities that have no children.
//! - one `<top-level name>.json` per top-level entity **with** children, holding that whole
//!   subtree. Agents usually build in separate top-level groups (districts), so their work lands
//!   in separate files: git diffs stay small and parallel work doesn't merge-conflict.
//!
//! Every entity is one line of JSON (fixed field order, defaults left out, keys sorted in
//! `props`), listed in path order. Each record is also valid `world_spawn` arguments plus `id`,
//! `version` and attribution, and loading goes through the same validation as the tool. Floats
//! are written as the shortest decimal that reads back bit-exactly, and colours as hex only when
//! hex is exact, so save → load restores the world bit for bit ([`World::state_hash`] matches).
//!
//! Saving writes only files whose content changed (each via a temporary file and a rename),
//! `world.json` last, then removes files the world no longer has.

use crate::tools::{parse_env, parse_spawn};
use crate::world::{color_json_exact, material_name, vec3_json, Entity, Look, World, WorldError};
use specula_core::hash::hash_str;
use specula_core::Json;
use specula_gpu::Material;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "specula.world/v1";
const INDEX: &str = "world.json";
/// Keys an entity record may have; anything else is a typo worth reporting.
const RECORD_KEYS: [&str; 25] = [
    "path", "id", "shape", "position", "rotation_deg", "scale", "color", "material", "opacity", "emissive",
    "metallic", "roughness", "specular", "seed", "color2", "light", "profile", "segments", "tags", "props",
    "collider", "body", "version", "created_by", "modified_by",
];

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

/// File names for the top-level subtrees `names` (in that order): the name with anything outside
/// `A-Z a-z 0-9 _ -` replaced by `_`, plus a short hash of the real name when that would clash
/// (case-insensitively, for Windows and macOS) or be a device name.
fn file_names(names: &[&str]) -> Vec<String> {
    let mut used: BTreeSet<String> = BTreeSet::from(["world".to_owned()]);
    names
        .iter()
        .map(|name| {
            let mut stem: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                .take(64)
                .collect();
            if used.contains(&stem.to_ascii_lowercase()) || is_device_name(&stem) {
                stem = format!("{stem}-{:08x}", hash_str(name) as u32);
            }
            let mut candidate = stem.clone();
            let mut n = 2;
            while used.contains(&candidate.to_ascii_lowercase()) {
                candidate = format!("{stem}-{n}");
                n += 1;
            }
            used.insert(candidate.to_ascii_lowercase());
            format!("{candidate}.json")
        })
        .collect()
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
    if look.material != Material::Lit || look.opacity != d.opacity {
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
    f.push(("created_by", Json::from(e.created_by.as_str())));
    f.push(("modified_by", Json::from(e.modified_by.as_str())));
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

/// Every file of the world as `(file name, content)`, `world.json` last. Pure and deterministic.
pub fn render(world: &World) -> Vec<(String, String)> {
    let tops: Vec<u64> = world.children_of(None).collect();
    let (subtrees, loose): (Vec<u64>, Vec<u64>) = tops.into_iter().partition(|&id| world.children_of(Some(id)).next().is_some());
    let names: Vec<&str> = subtrees.iter().map(|&id| world.get(id).expect("listed").name.as_str()).collect();
    let files = file_names(&names);
    let mut out = Vec::with_capacity(files.len() + 1);
    for (&root, file) in subtrees.iter().zip(&files) {
        let mut entities = Vec::new();
        world.walk(Some(root), |e, path| entities.push(record(e, path)));
        let header = vec![("format", Json::from(FORMAT)), ("root", Json::from(world.path_of(root)))];
        out.push((file.clone(), document(header, &entities)));
    }
    let loose: Vec<Json> = loose.iter().map(|&id| record(world.get(id).expect("listed"), &world.path_of(id))).collect();
    let header = vec![
        ("format", Json::from(FORMAT)),
        ("next_id", Json::from(world.next_id())),
        ("env", world.env().to_json_exact()),
        ("files", Json::arr(files.iter().map(|f| Json::from(f.as_str())))),
    ];
    out.push((INDEX.to_owned(), document(header, &loose)));
    out
}

pub struct SaveReport {
    pub dir: PathBuf,
    pub files: usize,
    /// Files whose content changed.
    pub written: usize,
    /// Files of entities that no longer exist.
    pub removed: usize,
}

/// Writes the world into `dir` (created if needed).
pub fn save(world: &World, dir: &Path) -> Result<SaveReport, PersistError> {
    let files = render(world);
    let shown = dir.display().to_string();
    std::fs::create_dir_all(dir).map_err(|e| PersistError::io(&shown, e))?;
    let mut written = 0;
    for (name, content) in &files {
        let path = dir.join(name);
        if std::fs::read(&path).is_ok_and(|old| old == content.as_bytes()) {
            continue;
        }
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, content).map_err(|e| PersistError::io(name, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| PersistError::io(name, e))?;
        written += 1;
    }
    let keep: BTreeSet<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    let mut removed = 0;
    for entry in std::fs::read_dir(dir).map_err(|e| PersistError::io(&shown, e))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let ours = name.ends_with(".json") || name.ends_with(".json.tmp");
        if ours && !keep.contains(name.as_str()) && entry.file_type().is_ok_and(|t| t.is_file()) {
            std::fs::remove_file(entry.path()).map_err(|e| PersistError::io(&name, e))?;
            removed += 1;
        }
    }
    Ok(SaveReport { dir: dir.to_owned(), files: files.len(), written, removed })
}

/// Reads the world saved in `dir`.
pub fn load(dir: &Path) -> Result<World, PersistError> {
    load_with(|file| {
        std::fs::read_to_string(dir.join(file)).map_err(|e| {
            let code = if e.kind() == std::io::ErrorKind::NotFound { "not_found" } else { "io" };
            PersistError::new(code, format!("{}: {e}", dir.join(file).display())).in_file(file)
        })
    })
}

/// [`load`] over any file source (tests read from memory).
pub fn load_with(mut read: impl FnMut(&str) -> Result<String, PersistError>) -> Result<World, PersistError> {
    let index = parse_doc(INDEX, &read(INDEX)?)?;
    let bad = |field: &str, message: &str| {
        let mut e = PersistError::new("invalid_world", message).in_file(INDEX);
        e.field = Some(field.to_owned());
        e
    };
    let next_id = index.get("next_id").and_then(Json::as_u64).ok_or_else(|| bad("next_id", "'next_id' must be an integer"))?;
    let env = index.get("env").ok_or_else(|| bad("env", "'env' is missing"))?;
    if let Some((key, _)) = env.as_object().unwrap_or_default().iter().find(|(k, _)| !ENV_KEYS.contains(&k.as_str())) {
        return Err(bad(&format!("env.{key}"), &format!("unknown environment field '{key}'")));
    }
    let env = parse_env(env).map_err(|m| bad(&format!("env.{}", quoted_field(&m).unwrap_or_default()), &m))?;
    let files: Vec<String> = match index.get("files").and_then(Json::as_array) {
        Some(items) => items
            .iter()
            .map(|f| match f.as_str() {
                Some(name) if is_subtree_file(name) => Ok(name.to_owned()),
                _ => Err(bad("files", "'files' must list plain file names like \"slab.json\"")),
            })
            .collect::<Result<_, _>>()?,
        None => return Err(bad("files", "'files' must be an array of file names")),
    };

    let mut world = World::default();
    world.set_env(&env);
    load_entities(&mut world, INDEX, &index, None)?;
    for file in &files {
        let doc = parse_doc(file, &read(file)?)?;
        let root = doc.get("root").and_then(Json::as_str).map(str::to_owned);
        let root = root.ok_or_else(|| {
            let mut e = PersistError::new("invalid_world", "'root' must be the subtree's top-level path").in_file(file);
            e.field = Some("root".into());
            e
        })?;
        load_entities(&mut world, file, &doc, Some(&root))?;
    }
    world.set_next_id(next_id).map_err(|e| from_world(e, INDEX, None))?;
    Ok(world)
}

const ENV_KEYS: [&str; 8] = [
    "sun_direction", "sun_color", "sun_intensity", "ambient_color", "ambient_intensity", "exposure", "tonemap",
    "background",
];

fn is_subtree_file(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|stem| {
        !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }) && name != INDEX
}

fn parse_doc(file: &str, text: &str) -> Result<Json, PersistError> {
    let doc = Json::parse(text).map_err(|e| {
        let mut err = PersistError::new("invalid_json", e.to_string()).in_file(file);
        err.field = Some(format!("byte {}", e.offset));
        err
    })?;
    match doc.get("format").and_then(Json::as_str) {
        Some(FORMAT) => Ok(doc),
        other => {
            let mut e = PersistError::new(
                "unsupported_format",
                format!("format is {}, this engine reads {FORMAT}", other.map_or("missing".into(), |f| format!("'{f}'"))),
            )
            .in_file(file);
            e.field = Some("format".into());
            Err(e)
        }
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
    let records = doc.get("entities").and_then(Json::as_array).ok_or_else(|| {
        let mut e = PersistError::new("invalid_world", "'entities' must be an array").in_file(file);
        e.field = Some("entities".into());
        e
    })?;
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
            rec.get(key).and_then(Json::as_u64).filter(|&n| n >= 1).ok_or_else(|| fail(Some(key.into()), format!("'{key}' must be an integer >= 1")))
        };
        let (id, version) = (int("id")?, int("version")?);
        let text = |key: &str| match rec.get(key) {
            Some(Json::Str(s)) => Ok(s.clone()),
            _ => Err(fail(Some(key.into()), format!("'{key}' must be a string"))),
        };
        let (created_by, modified_by) = (text("created_by")?, text("modified_by")?);
        let spec = parse_spawn(rec).map_err(|m| fail(quoted_field(&m), m))?;
        world.restore(spec, id, version, &created_by, &modified_by).map_err(|e| from_world(e, file, Some(path)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Body, BodyKind, Collider, ColliderShape, EntityKey, EnvPatch, LightSpec, Profile, Shape, SpawnSpec, UpdateSpec};
    use specula_core::math::Vec3;
    use std::collections::BTreeMap;

    fn sample() -> World {
        let mut w = World::default();
        w.set_env(&EnvPatch { sun_color: Some([0.123_456_79, 1.0 / 3.0, 0.0]), tonemap: Some(true), ..Default::default() });
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
            s.collider = Some(Collider { shape: ColliderShape::Box { size: Vec3::ONE }, offset: Vec3::ZERO, is_trigger: i == 1, layer: "terrain".into() });
            s.body = Some(Body { kind: BodyKind::Static, mass: 1.0 });
            w.spawn(s, "bob").unwrap();
        }
        let mut glass = SpawnSpec::new(Shape::Lathe);
        glass.name = Some("Vase (tall)".into());
        glass.profile = Some(Profile { points: vec![[0.1, 0.0], [0.15, 0.2], [0.05, 0.4]], segments: 24 });
        glass.look.material = Material::Glass;
        glass.look.opacity = 1.0;
        glass.look.surface.color2 = [0.2, 0.4, 0.6];
        glass.light = Some(LightSpec { intensity: 0.7, range: 2.5, color: Some([1.0, 0.9, 0.8]), offset: Vec3::new(0.0, 0.2, 0.0) });
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
        load_with(|f| files.get(f).cloned().ok_or_else(|| PersistError::new("not_found", f).in_file(f)))
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
        assert_eq!(names, ["slab.json", "world.json"]);
        let index = &saved["world.json"];
        assert!(index.contains(r#""files": ["slab.json"]"#), "{index}");
        // Loose top-level entities live in world.json, one line each, in path order.
        let lines: Vec<&str> = index.lines().filter(|l| l.trim_start().starts_with("{\"path\"")).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(r#""path":"/Vase (tall)""#) && lines[1].contains(r#""path":"/sun""#));
        assert!(lines[0].contains(r#""opacity":1"#), "non-standard materials always store opacity");
        let slab = &saved["slab.json"];
        assert_eq!(slab.lines().filter(|l| l.contains("\"path\"")).count(), 5);
        assert!(slab.contains(r#""position":[0.1,-0,1e-7]"#) || slab.contains(r#""position":[0.1,-0,0.0000001]"#), "{slab}");
        assert!(slab.contains(r#""props":{"faction":"old","hp":12.5}"#));
    }

    #[test]
    fn file_names_are_safe_and_distinct() {
        let names = file_names(&["slab", "Slab", "a/b", "con", "world", "x.y", "a_b"]);
        assert_eq!(names[0], "slab.json");
        assert!(names[1].starts_with("Slab-") && names[1].ends_with(".json"), "{names:?}");
        assert_eq!(names[2], "a_b.json");
        assert!(names[3].starts_with("con-"), "{names:?}");
        assert!(names[4].starts_with("world-"), "{names:?}");
        assert_eq!(names[5], "x_y.json");
        assert!(names[6].starts_with("a_b-"), "clashes with a/b: {names:?}");
        let lower: BTreeSet<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
        assert_eq!(lower.len(), names.len());
        assert!(names.iter().all(|n| is_subtree_file(n)));
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
        let e = broken("slab.json", r#""shape":"cube""#, r#""shape":"cone""#);
        assert_eq!((e.file.as_deref(), e.entity.as_deref(), e.field.as_deref()), (Some("slab.json"), Some("/slab/stones/ward_00"), Some("shape")));
        let e = broken("slab.json", r#""scale":[0.3,0.5,0.3]"#, r#""scale":[0,0.5,0.3]"#);
        assert_eq!(e.field.as_deref(), Some("scale"));
        let e = broken("slab.json", r#""version":1"#, r#""version":0"#);
        assert_eq!(e.field.as_deref(), Some("version"));
        let e = broken("slab.json", r#""path":"/slab/stones/ward_02""#, r#""path":"/slab/stones/ward_00""#);
        assert_eq!((e.entity.as_deref(), e.field.as_deref()), (Some("/slab/stones/ward_00"), None));
        assert!(e.message.contains("already exists"), "{e}");
        let e = broken("slab.json", r#""path":"/slab/stones/ward_02""#, r#""path":"/elsewhere/ward_02""#);
        assert_eq!(e.field.as_deref(), Some("path"));
        let e = broken("slab.json", r#""path":"/slab/stones","#, r#""path":"/slab/stones","colour":1,"#);
        assert_eq!(e.field.as_deref(), Some("colour"));
        let e = broken("slab.json", "\"entities\": [", "\"entities\": [[");
        assert_eq!((e.code, e.file.as_deref()), ("invalid_json", Some("slab.json")));
        let e = broken("world.json", FORMAT, "specula.world/v9");
        assert_eq!(e.code, "unsupported_format");
        let e = broken("world.json", r#""files": ["slab.json"]"#, r#""files": ["../slab.json"]"#);
        assert_eq!(e.field.as_deref(), Some("files"));
        let e = broken("world.json", r#""files": ["slab.json"]"#, r#""files": ["gone.json"]"#);
        assert_eq!((e.code, e.file.as_deref()), ("not_found", Some("gone.json")));
        let e = broken("world.json", r#""next_id": 7"#, r#""next_id": 3"#);
        assert_eq!(e.field.as_deref(), Some("next_id"));
        let e = broken("world.json", r#""exposure":1"#, r#""exposure":-1"#);
        assert_eq!(e.field.as_deref(), Some("env.exposure"));
        let e = broken("world.json", r#""path":"/sun""#, r#""path":"/sun/x""#);
        assert_eq!(e.field.as_deref(), Some("path"));
    }

    #[test]
    fn save_writes_only_changes_and_removes_stale_files() {
        let dir = std::env::temp_dir().join(format!("specula-persist-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut w = sample();
        let first = save(&w, &dir).unwrap();
        assert_eq!((first.files, first.written, first.removed), (2, 2, 0));
        let again = save(&w, &dir).unwrap();
        assert_eq!((again.written, again.removed), (0, 0), "unchanged files are not rewritten");
        w.delete(&EntityKey::Path("/slab".into()), None).unwrap();
        let after = save(&w, &dir).unwrap();
        assert_eq!((after.files, after.written, after.removed), (1, 1, 1));
        assert!(!dir.join("slab.json").exists());
        assert_eq!(load(&dir).unwrap().state_hash(), w.state_hash());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(load(&dir).err().unwrap().code, "not_found");
    }
}
