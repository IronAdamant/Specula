//! MCP tools agents use to drive the engine.
//!
//! Tool calls arrive on protocol threads; each is parsed and validated here, sent to the main
//! thread as a [`Command`], and answered with the main thread's [`ToolOutput`]. The world is only
//! ever touched by the main thread, between frames.

use crate::sight::Camera;
use crate::world::{
    self, parse_material, Body, BodyKind, Collider, ColliderShape, EntityKey, EnvPatch, LightSpec, LookPatch, Op,
    Profile, Query, Shape, SpawnSpec, UpdateSpec, MATERIAL_NAMES,
};
use specula_core::math::Vec3;
use specula_core::{base64, Json};
use specula_protocol::{SessionInfo, ToolDef, ToolHandler, ToolOutput};
use std::collections::BTreeSet;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

pub const INSTRUCTIONS: &str = "Specula is a live game engine you build in. The world is a tree of entities: \
each has a name unique among its siblings and a path like /slab/stones/ward_03; transforms are local to the \
parent, and 'group' entities hold children without drawing anything. Add things with world_spawn (or many at \
once, all-or-nothing, with world_batch), change them with world_set, remove subtrees with world_delete, and find \
them with world_query (filters, paged) or world_list. Address an entity by id, path, or a bare name that is \
unique in the world. Entities carry tags, free-form props (game data), and reserved collider/body components. \
Every entity has a version: pass expect_version to world_set/world_delete so you never overwrite another agent's \
newer change. LOOK with sight_frame: it returns the rendered image plus a subject list giving each entity's \
on-screen rectangle (its projected bounding box, computed by the engine). world_env sets the sun, ambient light, \
background and tone mapping; window_view points the human's window. world_save / world_load keep the world in \
text files. Say who you are with agent_hello. Coordinates: metres, right-handed, +Y up; a top-down view puts \
north (-Z) at the top. Other agents may be building in the same world at the same time.";

/// How long a tool call waits for the main thread before giving up.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// The engine discards a command this long after it was sent: comfortably before the caller's
/// timeout, so a call reported as timed out has definitely not been applied.
const APPLY_DEADLINE: Duration = Duration::from_secs(25);

/// Operations per world_batch call.
pub const MAX_BATCH_OPS: usize = 1000;
/// Default and largest page sizes: world_list returns whole entities, world_query compact ones.
const LIST_LIMITS: (u32, u32) = (50, 200);
const QUERY_LIMITS: (u32, u32) = (50, 500);
/// world_query's fields when none are asked for.
pub const DEFAULT_FIELDS: [&str; 6] = ["id", "path", "shape", "position", "tags", "version"];

// Arguments each tool accepts. Anything else is rejected (with the closest valid name), so a typo
// like `rotation` for `rotation_deg` is an error instead of a silently ignored value.
const LOOK_KEYS: &[&str] = &["material", "opacity", "emissive", "metallic", "roughness", "specular", "color2", "seed", "light"];
const COMPONENT_KEYS: &[&str] = &["tags", "props", "collider", "body"];
pub(crate) const SPAWN_KEYS: &[&str] =
    &["shape", "name", "parent", "path", "position", "rotation_deg", "scale", "color", "profile", "segments"];
const SET_KEYS: &[&str] = &[
    "id", "path", "name", "expect_version", "rename", "parent", "position", "rotation_deg", "scale", "color", "profile",
    "segments", "add_tags", "remove_tags",
];
const DELETE_KEYS: &[&str] = &["id", "path", "name", "expect_version"];
const QUERY_KEYS: &[&str] = &["limit", "fields", "path_prefix", "tags", "shape", "material", "created_by", "region", "cursor"];
const FRAME_KEYS: &[&str] = &["width", "height", "eye", "target", "fov_y_deg", "max_subjects", "time_s"];
pub(crate) const ENV_KEYS: &[&str] = &[
    "sun_direction", "sun_color", "sun_intensity", "ambient_color", "ambient_intensity", "exposure", "tonemap",
    "background",
];

/// Rejects keys of `args` (an object, or null for none) that are in none of `allowed`.
pub(crate) fn check_keys(args: &Json, what: &str, allowed: &[&[&str]]) -> Result<(), String> {
    let pairs = match args {
        Json::Null => return Ok(()),
        Json::Obj(pairs) => pairs,
        _ => return Err(format!("{what} must be a JSON object")),
    };
    let known = || allowed.iter().flat_map(|list| list.iter().copied());
    for (key, _) in pairs {
        if !known().any(|k| k == key) {
            let hint = closest(key, known()).map(|c| format!("; did you mean '{c}'?")).unwrap_or_default();
            return Err(format!("'{key}' is not an argument of {what}{hint}"));
        }
    }
    Ok(())
}

/// The valid name `key` was most likely meant to be: a prefix match (`rotation` → `rotation_deg`)
/// or the nearest by edit distance, if near enough.
fn closest<'a>(key: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let distance = |a: &str, b: &str| -> usize {
        if a.starts_with(b) || b.starts_with(a) {
            return 1;
        }
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != *cb));
                prev = row[j + 1];
                row[j + 1] = next;
            }
        }
        row[b.len()]
    };
    let limit = (key.chars().count() / 3).max(2);
    candidates.map(|c| (distance(key, c), c)).filter(|(d, _)| *d <= limit).min().map(|(_, c)| c)
}

/// A world value check as an argument error (its message already starts with the quoted field).
fn value_error(e: world::WorldError) -> String {
    e.message
}

pub enum Command {
    Spawn(SpawnSpec),
    Set(EntityKey, UpdateSpec),
    /// Entity and `expect_version`.
    Delete(EntityKey, Option<u64>),
    Query(QuerySpec),
    Batch(Vec<Op>),
    /// `name: None` = the current world. Another existing world is only replaced with `overwrite`.
    Save { name: Option<String>, overwrite: bool },
    /// Replace the world with a saved one; `force` discards unsaved changes.
    Load { name: String, force: bool },
    Hello { name: String, role: String },
    Frame(FrameSpec),
    Env(EnvPatch),
    /// `None` returns the window to auto-framing.
    View(Option<Camera>),
    Info,
}

/// world_list and world_query.
pub struct QuerySpec {
    pub query: Query,
    /// `None` = whole entities (world_list).
    pub fields: Option<Vec<String>>,
    /// Response key for the items.
    pub key: &'static str,
}

pub struct FrameSpec {
    pub width: u32,
    pub height: u32,
    /// Explicit camera; `None` frames the whole world automatically.
    pub eye: Option<Vec3>,
    pub target: Option<Vec3>,
    pub fov_y_deg: f32,
    /// Subjects returned in the response (the saved packet always has all of them).
    pub max_subjects: usize,
    /// Animation time to render; `None` = the engine's current time.
    pub time_s: Option<f32>,
}

pub struct Request {
    pub session: SessionInfo,
    pub command: Command,
    pub reply: Sender<ToolOutput>,
    /// Past this instant the caller has given up; the engine drops the command unapplied.
    pub deadline: Instant,
}

pub struct Tools {
    to_engine: Sender<Request>,
}

impl Tools {
    pub fn new(to_engine: Sender<Request>) -> Tools {
        Tools { to_engine }
    }
}

impl ToolHandler for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        definitions()
    }

    fn call(&self, session: &SessionInfo, name: &str, arguments: &Json) -> ToolOutput {
        let command = match parse(name, arguments) {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(argument_error(&e).to_pretty()),
        };
        let (reply, answer) = mpsc::channel();
        let deadline = Instant::now() + APPLY_DEADLINE;
        let request = Request { session: session.clone(), command, reply, deadline };
        if self.to_engine.send(request).is_err() {
            return ToolOutput::error("the engine is shutting down");
        }
        answer.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|e| match e {
            mpsc::RecvTimeoutError::Timeout => {
                ToolOutput::error("the engine was too busy to answer within 30 s; the command was not applied")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                ToolOutput::error("the engine is shutting down or skipped the command; it was not applied")
            }
        })
    }
}

fn parse(name: &str, args: &Json) -> Result<Command, String> {
    match name {
        "world_spawn" => parse_spawn(args).map(Command::Spawn),
        "world_set" => parse_set(args).map(|(key, change)| Command::Set(key, change)),
        "world_list" => parse_query(args, true).map(Command::Query),
        "world_query" => parse_query(args, false).map(Command::Query),
        "world_delete" => parse_delete(args).map(|(key, v)| Command::Delete(key, v)),
        "world_batch" => parse_batch(args).map(Command::Batch),
        "world_save" => {
            check_keys(args, name, &[&["name", "overwrite"]])?;
            Ok(Command::Save {
                name: world_name_arg(args, false)?,
                overwrite: bool_arg(args, "overwrite")?.unwrap_or(false),
            })
        }
        "world_load" => {
            check_keys(args, name, &[&["name", "force"]])?;
            Ok(Command::Load {
                name: world_name_arg(args, true)?.expect("required"),
                force: bool_arg(args, "force")?.unwrap_or(false),
            })
        }
        "agent_hello" => parse_hello(args),
        "sight_frame" => {
            check_keys(args, name, &[FRAME_KEYS])?;
            parse_frame(args).map(Command::Frame)
        }
        "world_env" => {
            check_keys(args, name, &[ENV_KEYS])?;
            parse_env(args).map(Command::Env)
        }
        "window_view" => {
            check_keys(args, name, &[&["eye", "target", "fov_y_deg", "auto"]])?;
            parse_view(args).map(Command::View)
        }
        "engine_info" => check_keys(args, name, &[]).map(|()| Command::Info),
        other => Err(format!("unknown tool '{other}'")),
    }
}

/// A rejected argument as structured data. `field` is the first quoted name in the message: every
/// argument message names its key that way (`'scale' must be ...`).
pub fn argument_error(message: &str) -> Json {
    let field = message.split('\'').nth(1).filter(|f| !f.is_empty() && !f.contains(' '));
    let mut out = vec![("error", Json::from("invalid_argument")), ("message", Json::from(message))];
    if let Some(f) = field {
        out.push(("field", Json::from(f)));
    }
    Json::obj(out)
}

/// `id` (integer), `path` ("/a/b") or `name` (unique in the world) identifying an existing entity.
fn entity_key(args: &Json, tool: &str) -> Result<EntityKey, String> {
    let present = |k: &str| args.get(k).is_some_and(|v| !v.is_null());
    if ["id", "path", "name"].iter().filter(|k| present(k)).count() > 1 {
        let hint = if tool == "world_set" { " (to rename an entity use 'rename')" } else { "" };
        return Err(format!("{tool}: give one of 'id', 'path' or 'name', not both{hint}"));
    }
    match (args.get("id"), args.get("path"), args.get("name")) {
        (Some(id), _, _) if !id.is_null() => {
            id.as_u64().map(EntityKey::Id).ok_or_else(|| "'id' must be a non-negative integer".into())
        }
        (_, Some(Json::Str(p)), _) => Ok(EntityKey::Path(p.clone())),
        (_, Some(p), _) if !p.is_null() => Err("'path' must be a string like \"/slab/stones\"".into()),
        (_, _, Some(Json::Str(n))) => Ok(EntityKey::Name(n.clone())),
        (_, _, Some(n)) if !n.is_null() => Err("'name' must be a string".into()),
        _ => Err(format!("{tool} needs 'id' (integer), 'path' (\"/a/b\") or 'name' (unique in the world)")),
    }
}

/// An entity reference given as a value: integer id, "/path" or a unique bare name.
fn key_value(v: &Json, key: &str) -> Result<EntityKey, String> {
    match v {
        Json::Str(s) if s.starts_with('/') => Ok(EntityKey::Path(s.clone())),
        Json::Str(s) => Ok(EntityKey::Name(s.clone())),
        v => v
            .as_u64()
            .map(EntityKey::Id)
            .ok_or_else(|| format!("'{key}' must be an entity id, a path like \"/slab\" or a unique name")),
    }
}

/// `parent`: absent -> `None`; `null` -> top level; otherwise the parent entity.
fn parent_arg(args: &Json) -> Result<Option<Option<EntityKey>>, String> {
    match args.get("parent") {
        None => Ok(None),
        Some(Json::Null) => Ok(Some(None)),
        Some(v) => key_value(v, "parent").map(|k| Some(Some(k))),
    }
}

fn parse_delete(args: &Json) -> Result<(EntityKey, Option<u64>), String> {
    check_keys(args, "world_delete", &[DELETE_KEYS])?;
    parse_delete_fields(args)
}

fn parse_delete_fields(args: &Json) -> Result<(EntityKey, Option<u64>), String> {
    Ok((entity_key(args, "world_delete")?, version_arg(args)?))
}

fn version_arg(args: &Json) -> Result<Option<u64>, String> {
    match args.get("expect_version") {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if n >= 1 => Ok(Some(n)),
            _ => Err("'expect_version' must be an integer >= 1 (the version you last read)".into()),
        },
    }
}

fn bool_arg(args: &Json, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| format!("'{key}' must be true or false")),
    }
}

fn string_arg<'a>(args: &'a Json, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => Ok(Some(s)),
        Some(_) => Err(format!("'{key}' must be a string")),
    }
}

fn parse_spawn(args: &Json) -> Result<SpawnSpec, String> {
    check_keys(args, "world_spawn", &[SPAWN_KEYS, LOOK_KEYS, COMPONENT_KEYS])?;
    parse_spawn_fields(args)
}

/// world_spawn's arguments, without the key check (world files check their own keys).
pub(crate) fn parse_spawn_fields(args: &Json) -> Result<SpawnSpec, String> {
    let shape_name = match args.get("shape") {
        None | Some(Json::Null) => return Err("'shape' is required".into()),
        Some(Json::Str(s)) => s,
        Some(_) => return Err("'shape' must be a string: group, cube, quad, sphere or lathe".into()),
    };
    let shape = Shape::parse(shape_name)
        .ok_or_else(|| format!("'shape' must be group, cube, quad, sphere or lathe, not '{shape_name}'"))?;
    let profile = profile_arg(args)?;
    if (shape == Shape::Lathe) != profile.is_some() {
        return Err("'profile' is required for shape 'lathe' and only allowed for it".into());
    }
    let mut spec = SpawnSpec::new(shape);
    spec.name = name_arg(args.get("name"))?;
    spec.parent = parent_arg(args)?.flatten();
    if let Some(path) = string_arg(args, "path")? {
        if spec.name.is_some() || spec.parent.is_some() {
            return Err("give 'path', or 'name' and 'parent', not both".into());
        }
        let (parent, name) = path
            .strip_prefix('/')
            .and_then(|_| path.rsplit_once('/'))
            .ok_or("'path' must look like \"/parent/name\" (the new entity's full path)")?;
        spec.name = name_arg(Some(&Json::from(name))).map_err(|e| format!("'path': {e}"))?;
        spec.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.to_owned()));
    }
    spec.position = vec3_arg(args, "position")?.unwrap_or(Vec3::ZERO);
    spec.rotation_deg = vec3_arg(args, "rotation_deg")?.unwrap_or(Vec3::ZERO);
    spec.scale = scale_arg(args)?.unwrap_or(Vec3::ONE);
    spec.color = color_arg(args, "color")?.unwrap_or([0.8, 0.8, 0.8]);
    look_arg(args)?.apply(&mut spec.look);
    if spec.look.material != specula_gpu::Material::Lit && args.get("opacity").is_none() {
        spec.look.opacity = match spec.look.material {
            specula_gpu::Material::Glass => 0.15,
            specula_gpu::Material::BlobShadow => 0.6,
            _ => 1.0,
        };
    }
    spec.light = light_arg(args)?.flatten();
    spec.profile = profile;
    spec.tags = tags_arg(args, "tags")?.map(BTreeSet::from_iter).unwrap_or_default();
    spec.props = props_arg(args)?.unwrap_or(Json::Null);
    spec.collider = collider_arg(args)?.flatten();
    spec.body = body_arg(args)?.flatten();
    // Value ranges are the world's to decide (the same checks guard in-process game code).
    world::check_spawn(&spec).map_err(value_error)?;
    Ok(spec)
}

/// An array of tags (see [`world::validate_tag`]), at most [`world::MAX_TAGS`].
fn tags_arg(args: &Json, key: &str) -> Result<Option<Vec<String>>, String> {
    let Some(v) = args.get(key).filter(|v| !v.is_null()) else { return Ok(None) };
    let err = || format!("'{key}' must be an array of up to {} tags (A-Z a-z 0-9 _ - . :)", world::MAX_TAGS);
    let items = v.as_array().ok_or_else(err)?;
    if items.len() > world::MAX_TAGS {
        return Err(err());
    }
    let mut tags = Vec::with_capacity(items.len());
    for item in items {
        let t = item.as_str().ok_or_else(err)?;
        world::validate_tag(t).map_err(|e| format!("'{key}': {}", e.message))?;
        tags.push(t.to_owned());
    }
    Ok(Some(tags))
}

fn props_arg(args: &Json) -> Result<Option<Json>, String> {
    match args.get("props") {
        None | Some(Json::Null) => Ok(None),
        Some(v @ Json::Obj(_)) => Ok(Some(v.clone())),
        Some(_) => Err("'props' must be a JSON object".into()),
    }
}

/// A size (the world checks its range), or `default`.
fn size_arg(args: &Json, key: &str, default: Option<f32>) -> Result<f32, String> {
    match num_arg(args, key)? {
        Some(v) => Ok(v),
        None => default.ok_or_else(|| format!("'collider.{key}' is required (metres)")),
    }
}

/// `collider`: `{shape: box|sphere|capsule, size | radius (+ height), offset?, is_trigger?, layer?}`
/// or `false` to remove it.
fn collider_arg(args: &Json) -> Result<Option<Option<Collider>>, String> {
    const ERR: &str = "'collider' must be false or {shape: box|sphere|capsule, size?|radius?, height?, offset?, \
                       is_trigger?, layer?}";
    let c = match args.get("collider") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Bool(false)) => return Ok(Some(None)),
        Some(c @ Json::Obj(_)) => c,
        Some(_) => return Err(ERR.into()),
    };
    check_keys(c, "collider", &[&["shape", "size", "radius", "height", "offset", "is_trigger", "layer"]])
        .map_err(|e| e.replacen('\'', "'collider.", 1))?;
    let shape = match c.get("shape").and_then(Json::as_str) {
        Some("box") => ColliderShape::Box { size: vec3_arg(c, "size")?.unwrap_or(Vec3::ONE) },
        Some("sphere") => ColliderShape::Sphere { radius: size_arg(c, "radius", Some(0.5))? },
        Some("capsule") => {
            ColliderShape::Capsule { radius: size_arg(c, "radius", None)?, height: size_arg(c, "height", None)? }
        }
        _ => return Err(ERR.into()),
    };
    Ok(Some(Some(Collider {
        shape,
        offset: vec3_arg(c, "offset")?.unwrap_or(Vec3::ZERO),
        is_trigger: bool_arg(c, "is_trigger")?.unwrap_or(false),
        layer: string_arg(c, "layer")?.unwrap_or("default").to_owned(),
    })))
}

/// `body`: `"static"|"kinematic"|"dynamic"`, `{kind, mass?}`, or `false` to remove it.
fn body_arg(args: &Json) -> Result<Option<Option<Body>>, String> {
    const ERR: &str = "'body' must be false, \"static\", \"kinematic\", \"dynamic\" or {kind, mass? (kg)}";
    let (kind, mass) = match args.get("body") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Bool(false)) => return Ok(Some(None)),
        Some(Json::Str(k)) => (k.as_str(), None),
        Some(b @ Json::Obj(_)) => {
            check_keys(b, "body", &[&["kind", "mass"]]).map_err(|e| e.replacen('\'', "'body.", 1))?;
            (b.get("kind").and_then(Json::as_str).ok_or(ERR)?, num_arg(b, "mass")?)
        }
        Some(_) => return Err(ERR.into()),
    };
    let kind = BodyKind::parse(kind).ok_or(ERR)?;
    Ok(Some(Some(Body { kind, mass: mass.unwrap_or(1.0) })))
}

/// Optional finite number. Its range is checked by the world (or by [`num_in`] for non-world
/// values such as camera settings).
fn num_arg(args: &Json, key: &str) -> Result<Option<f32>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(f) if f.is_finite() && f.abs() <= f64::from(f32::MAX) => Ok(Some(f as f32)),
            _ => Err(format!("'{key}' must be a finite number")),
        },
    }
}

/// Optional number within `range`.
fn num_in(args: &Json, key: &str, range: std::ops::RangeInclusive<f32>) -> Result<Option<f32>, String> {
    let v = num_arg(args, key)?;
    match v {
        Some(f) if !range.contains(&f) => Err(format!("'{key}' must be a number from {} to {}", range.start(), range.end())),
        _ => Ok(v),
    }
}

/// Material and surface fields shared by world_spawn and world_set.
fn look_arg(args: &Json) -> Result<LookPatch, String> {
    let material = match args.get("material") {
        None | Some(Json::Null) => None,
        Some(Json::Str(m)) => Some(
            parse_material(m).ok_or_else(|| format!("'material' must be one of {}, not '{m}'", MATERIAL_NAMES.join(", ")))?,
        ),
        Some(_) => return Err(format!("'material' must be a string: {}", MATERIAL_NAMES.join(", "))),
    };
    Ok(LookPatch {
        material,
        opacity: num_arg(args, "opacity")?,
        emissive: num_arg(args, "emissive")?,
        metallic: num_arg(args, "metallic")?,
        roughness: num_arg(args, "roughness")?,
        specular: num_arg(args, "specular")?,
        color2: color_arg(args, "color2")?,
        seed: num_arg(args, "seed")?,
    })
}

/// `light`: an object, or `false` to remove it. `Some(None)` = remove.
fn light_arg(args: &Json) -> Result<Option<Option<LightSpec>>, String> {
    const ERR: &str = "'light' must be false or {intensity, range?, color?, offset?}";
    match args.get("light") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(false)) => Ok(Some(None)),
        Some(l @ Json::Obj(_)) => {
            check_keys(l, "light", &[&["intensity", "range", "color", "offset"]]).map_err(|e| e.replacen('\'', "'light.", 1))?;
            let intensity = num_arg(l, "intensity")?.ok_or("'light' needs 'intensity' (0..1000000)")?;
            Ok(Some(Some(LightSpec {
                intensity,
                range: num_arg(l, "range")?.unwrap_or(5.0),
                color: color_arg(l, "color").map_err(|e| e.replacen('\'', "'light.", 1))?,
                offset: vec3_arg(l, "offset")?.unwrap_or(Vec3::ZERO),
            })))
        }
        Some(_) => Err(ERR.into()),
    }
}

/// `profile` ([[radius, height], ...], bottom to top) and `segments`, for lathes.
fn profile_arg(args: &Json) -> Result<Option<Profile>, String> {
    const ERR: &str = "'profile' must be 2..256 [radius, height] pairs of numbers";
    let Some(v) = args.get("profile").filter(|v| !v.is_null()) else {
        if args.get("segments").is_some_and(|s| !s.is_null()) {
            return Err("'segments' is only used with 'profile'".into());
        }
        return Ok(None);
    };
    let items = v.as_array().filter(|a| world::PROFILE_POINTS.contains(&a.len())).ok_or(ERR)?;
    let mut points = Vec::with_capacity(items.len());
    for item in items {
        let pair: Option<Vec<f64>> = item.as_array().and_then(|a| a.iter().map(Json::as_f64).collect());
        match pair.as_deref() {
            Some(&[r, y]) if [r, y].iter().all(|c| c.is_finite() && c.abs() <= f64::from(f32::MAX)) => {
                points.push([r as f32, y as f32])
            }
            _ => return Err(ERR.into()),
        }
    }
    let segments = match args.get("segments") {
        None | Some(Json::Null) => 32,
        Some(s) => match s.as_u64() {
            Some(n) if n <= u64::from(u32::MAX) => n as u32,
            _ => return Err("'segments' must be an integer from 3 to 256".into()),
        },
    };
    Ok(Some(Profile { points, segments }))
}

/// world_env's arguments (keys are checked by the caller: the tool, or the world file loader).
pub(crate) fn parse_env(args: &Json) -> Result<EnvPatch, String> {
    let patch = EnvPatch {
        sun_direction: vec3_arg(args, "sun_direction")?,
        sun_color: color_arg(args, "sun_color")?,
        sun_intensity: num_arg(args, "sun_intensity")?,
        ambient_color: color_arg(args, "ambient_color")?,
        ambient_intensity: num_arg(args, "ambient_intensity")?,
        exposure: num_arg(args, "exposure")?,
        tonemap: bool_arg(args, "tonemap")?,
        background: color_arg(args, "background")?,
    };
    world::check_env(&patch).map_err(value_error)?;
    Ok(patch)
}

fn parse_view(args: &Json) -> Result<Option<Camera>, String> {
    if args.get("auto").and_then(Json::as_bool) == Some(true) {
        return Ok(None);
    }
    let frame = parse_frame(args)?;
    match (frame.eye, frame.target) {
        (Some(eye), Some(target)) => Ok(Some(Camera { eye, target, fov_y_deg: frame.fov_y_deg })),
        _ => Err("window_view needs 'eye' and 'target', or 'auto': true".into()),
    }
}

fn parse_set(args: &Json) -> Result<(EntityKey, UpdateSpec), String> {
    check_keys(args, "world_set", &[SET_KEYS, LOOK_KEYS, COMPONENT_KEYS])?;
    parse_set_fields(args)
}

fn parse_set_fields(args: &Json) -> Result<(EntityKey, UpdateSpec), String> {
    let key = entity_key(args, "world_set")?;
    let change = UpdateSpec {
        name: name_arg(args.get("rename"))?,
        parent: parent_arg(args)?,
        position: vec3_arg(args, "position")?,
        rotation_deg: vec3_arg(args, "rotation_deg")?,
        scale: scale_arg(args)?,
        color: color_arg(args, "color")?,
        look: look_arg(args)?,
        light: light_arg(args)?,
        profile: profile_arg(args)?,
        tags: tags_arg(args, "tags")?.map(BTreeSet::from_iter),
        add_tags: tags_arg(args, "add_tags")?.unwrap_or_default(),
        remove_tags: tags_arg(args, "remove_tags")?.unwrap_or_default(),
        props: props_arg(args)?,
        collider: collider_arg(args)?,
        body: body_arg(args)?,
        expect_version: version_arg(args)?,
    };
    if change.is_empty() {
        return Err("world_set needs at least one change: rename, parent, position, rotation_deg, scale, color, a \
                    look field (material, opacity, emissive, metallic, roughness, specular, color2, seed), light, \
                    profile, tags, add_tags, remove_tags, props, collider or body"
            .into());
    }
    world::check_update(&change).map_err(value_error)?;
    Ok((key, change))
}

fn parse_batch(args: &Json) -> Result<Vec<Op>, String> {
    check_keys(args, "world_batch", &[&["ops"]])?;
    let ops = args.get("ops").and_then(Json::as_array).ok_or("'ops' must be an array of {op: spawn|set|delete, ...}")?;
    if ops.is_empty() || ops.len() > MAX_BATCH_OPS {
        return Err(format!("'ops' must hold 1..{MAX_BATCH_OPS} operations (got {}); split larger edits", ops.len()));
    }
    ops.iter()
        .enumerate()
        .map(|(i, op)| {
            let kind = op.get("op").and_then(Json::as_str).unwrap_or("");
            const OP: &[&str] = &["op"];
            let parsed = match kind {
                "spawn" => check_keys(op, "a spawn op", &[OP, SPAWN_KEYS, LOOK_KEYS, COMPONENT_KEYS])
                    .and_then(|()| parse_spawn_fields(op))
                    .map(Op::Spawn),
                "set" => check_keys(op, "a set op", &[OP, SET_KEYS, LOOK_KEYS, COMPONENT_KEYS])
                    .and_then(|()| parse_set_fields(op))
                    .map(|(key, change)| Op::Set(key, change)),
                "delete" => check_keys(op, "a delete op", &[OP, DELETE_KEYS])
                    .and_then(|()| parse_delete_fields(op))
                    .map(|(key, v)| Op::Delete(key, v)),
                _ => Err("'op' must be \"spawn\", \"set\" or \"delete\"".into()),
            };
            parsed.map_err(|e| format!("ops[{i}] ({kind}): {e}"))
        })
        .collect()
}

/// world_list (`list`: whole entities) and world_query (compact fields by default).
fn parse_query(args: &Json, list: bool) -> Result<QuerySpec, String> {
    check_keys(args, if list { "world_list" } else { "world_query" }, &[QUERY_KEYS])?;
    let (default, max) = if list { LIST_LIMITS } else { QUERY_LIMITS };
    let limit = match args.get("limit") {
        None | Some(Json::Null) => default,
        Some(v) => match v.as_u64() {
            Some(n) if (1..=u64::from(max)).contains(&n) => n as u32,
            _ => return Err(format!("'limit' must be an integer from 1 to {max}")),
        },
    };
    let after = match string_arg(args, "cursor")? {
        None => None,
        Some(c) => {
            let path = base64::decode(c).ok().and_then(|b| String::from_utf8(b).ok()).filter(|p| p.starts_with('/'));
            Some(path.ok_or("'cursor' is not a cursor from this tool; pass next_cursor back unchanged")?)
        }
    };
    let path_prefix = string_arg(args, "path_prefix")?.map(str::to_owned);
    if path_prefix.as_ref().is_some_and(|p| !p.starts_with('/')) {
        return Err("'path_prefix' must be a path like \"/slab\"".into());
    }
    let shape = match string_arg(args, "shape")? {
        None => None,
        Some(s) => Some(Shape::parse(s).ok_or_else(|| format!("'shape' must be group, cube, quad, sphere or lathe, not '{s}'"))?),
    };
    let material = match string_arg(args, "material")? {
        None => None,
        Some(m) => Some(parse_material(m).ok_or_else(|| format!("'material' must be one of {}, not '{m}'", MATERIAL_NAMES.join(", ")))?),
    };
    let region = match args.get("region").filter(|r| !r.is_null()) {
        None => None,
        Some(r) => match check_keys(r, "region", &[&["min", "max"]]).and_then(|()| Ok((vec3_arg(r, "min")?, vec3_arg(r, "max")?)))? {
            (Some(lo), Some(hi)) if lo.x <= hi.x && lo.y <= hi.y && lo.z <= hi.z => Some((lo, hi)),
            _ => return Err("'region' must be {min: [x,y,z], max: [x,y,z]} with min <= max".into()),
        },
    };
    let fields = match args.get("fields").filter(|f| !f.is_null()) {
        None => (!list).then(|| DEFAULT_FIELDS.map(String::from).to_vec()),
        Some(f) => {
            let names: Option<Vec<&str>> = f.as_array().and_then(|a| a.iter().map(Json::as_str).collect());
            let names = names.ok_or("'fields' must be an array of field names")?;
            if names == ["all"] {
                None
            } else {
                if let Some(bad) = names.iter().find(|n| !world::FIELDS.contains(n)) {
                    return Err(format!("'fields': unknown field '{bad}' (\"all\" or: {})", world::FIELDS.join(", ")));
                }
                if names.is_empty() {
                    return Err("'fields' must name at least one field".into());
                }
                Some(names.into_iter().map(String::from).collect())
            }
        }
    };
    let query = Query {
        path_prefix,
        tags: tags_arg(args, "tags")?.unwrap_or_default(),
        shape,
        material,
        created_by: string_arg(args, "created_by")?.map(str::to_owned),
        region,
        after,
        limit: limit as usize,
    };
    Ok(QuerySpec { query, fields, key: if list { "entities" } else { "items" } })
}

/// The opaque `next_cursor` for a page ending at `path`.
pub fn cursor_for(path: &str) -> String {
    base64::encode(path.as_bytes())
}

/// `name` of a world under the worlds directory (see [`crate::persist::validate_world_name`]).
fn world_name_arg(args: &Json, required: bool) -> Result<Option<String>, String> {
    match string_arg(args, "name")? {
        Some(n) => crate::persist::validate_world_name(n).map(|()| Some(n.to_owned())),
        None if required => Err("'name' is required: the world's folder name under the worlds directory".into()),
        None => Ok(None),
    }
}

fn parse_hello(args: &Json) -> Result<Command, String> {
    check_keys(args, "agent_hello", &[&["name", "role"]])?;
    let name = name_arg(args.get("name"))?.ok_or("'name' is required: how this agent appears in attribution")?;
    if name.chars().count() > 64 {
        return Err("'name' must be at most 64 characters".into());
    }
    let role = string_arg(args, "role")?.unwrap_or("builder").to_owned();
    world::validate_tag(&role).map_err(|_| "'role' must be a short word like builder, reviewer or playtester".to_string())?;
    Ok(Command::Hello { name, role })
}

fn parse_frame(args: &Json) -> Result<FrameSpec, String> {
    let int = |key: &str, default: u32, range: std::ops::RangeInclusive<u32>| -> Result<u32, String> {
        match args.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => match v.as_u64() {
                Some(n) if n <= u64::from(*range.end()) && range.contains(&(n as u32)) => Ok(n as u32),
                _ => Err(format!("'{key}' must be an integer from {} to {}", range.start(), range.end())),
            },
        }
    };
    let fov_y_deg = match args.get("fov_y_deg") {
        None | Some(Json::Null) => 50.0,
        Some(v) => match v.as_f64() {
            Some(f) if (10.0..=120.0).contains(&f) => f as f32,
            _ => return Err("'fov_y_deg' must be a number from 10 to 120".into()),
        },
    };
    let eye = position_arg(args, "eye")?;
    let target = position_arg(args, "target")?;
    match (eye, target) {
        (Some(_), None) | (None, Some(_)) => {
            return Err("give both 'eye' and 'target', or neither (auto-framing)".into());
        }
        (Some(e), Some(t)) if (t - e).length() < 1e-4 => {
            return Err("'eye' and 'target' must be different points".into());
        }
        _ => {}
    }
    Ok(FrameSpec {
        width: int("width", 960, 64..=2048)?,
        height: int("height", 540, 64..=2048)?,
        eye,
        target,
        fov_y_deg,
        max_subjects: int("max_subjects", 50, 1..=500)? as usize,
        time_s: num_in(args, "time_s", 0.0..=1.0e6)?,
    })
}

/// A name, trimmed; see [`world::validate_name`].
fn name_arg(v: Option<&Json>) -> Result<Option<String>, String> {
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => {
            let n = s.trim();
            world::validate_name(n).map_err(|e| {
                if n.contains('/') {
                    format!("{} (to place an entity under a parent use 'parent' or 'path')", e.message)
                } else {
                    e.message
                }
            })?;
            Ok(Some(n.to_owned()))
        }
        Some(_) => Err("names must be strings".into()),
    }
}

/// `[x, y, z]` of finite numbers, or `None` when absent / null.
fn vec3_arg(args: &Json, key: &str) -> Result<Option<Vec3>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => {
            let nums: Option<Vec<f64>> = v.as_array().and_then(|a| a.iter().map(Json::as_f64).collect());
            match nums.as_deref() {
                Some(&[x, y, z]) if [x, y, z].iter().all(|c| c.is_finite() && c.abs() <= f32::MAX as f64) => {
                    Ok(Some(Vec3::new(x as f32, y as f32, z as f32)))
                }
                _ => Err(format!("'{key}' must be an array of 3 finite numbers")),
            }
        }
    }
}

/// A camera point: within ±[`world::MAX_COORD`] metres on every axis. (Entity positions are
/// checked by the world.)
fn position_arg(args: &Json, key: &str) -> Result<Option<Vec3>, String> {
    let p = vec3_arg(args, key)?;
    if let Some(p) = p {
        world::check_point(key, p).map_err(value_error)?;
    }
    Ok(p)
}

/// Number (uniform) or `[x, y, z]`.
fn scale_arg(args: &Json) -> Result<Option<Vec3>, String> {
    match args.get("scale") {
        Some(Json::Num(_)) => Ok(num_arg(args, "scale")?.map(|s| Vec3::new(s, s, s))),
        _ => vec3_arg(args, "scale"),
    }
}

/// `"#rrggbb"`, `"#rgb"`, or `[r, g, b]` (sRGB; the world checks the components are 0..1).
fn color_arg(args: &Json, key: &str) -> Result<Option<[f32; 3]>, String> {
    let err = || format!("'{key}' must be \"#rrggbb\", \"#rgb\" or [r, g, b] with components 0..1");
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => world::color_from_hex(s).map(Some).ok_or_else(err),
        Some(Json::Arr(a)) => match a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>().as_deref() {
            Some(&[r, g, b]) if [r, g, b].iter().all(|c| c.is_finite() && c.abs() <= f64::from(f32::MAX)) => {
                Ok(Some([r as f32, g as f32, b as f32]))
            }
            _ => Err(err()),
        },
        Some(_) => Err(err()),
    }
}

fn definitions() -> Vec<ToolDef> {
    let vec3 = |desc: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("number"))])),
            ("minItems", Json::from(3)),
            ("maxItems", Json::from(3)),
            ("description", Json::from(desc)),
        ])
    };
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let string = |desc: &str| Json::obj([("type", Json::from("string")), ("description", Json::from(desc))]);
    let integer = |desc: &str| Json::obj([("type", Json::from("integer")), ("description", Json::from(desc))]);
    let described = |desc: &str| Json::obj([("description", Json::from(desc))]);
    let position = "World position in metres [x, y, z]; +Y is up; each within ±1,000,000";
    let rotation = "Rotation in degrees about X, then Y, then Z";
    let scale = "Number (uniform) or [x, y, z], each 0.001..100000";
    let color = "sRGB \"#rrggbb\" or [r,g,b] in 0..1";
    let number = |desc: &str| Json::obj([("type", Json::from("number")), ("description", Json::from(desc))]);
    let look = |props: &mut Vec<(&str, Json)>| {
        props.extend([
            (
                "material",
                Json::obj([
                    ("type", Json::from("string")),
                    ("enum", Json::arr(MATERIAL_NAMES.iter().map(|m| Json::from(*m)))),
                    (
                        "description",
                        Json::from(
                            "standard (opaque; default), glass (blended thin shell: Fresnel reflections, tinted by \
                             color, opacity = body density), blob_shadow (soft dark disc on a quad; opacity = \
                             darkness), lava_lamp (ray-marched lamp filling a cube: glass vessel as tall as the cube, \
                             widest radius min(width, depth)/3.2 at 30% height, bottom radius 0.62x and top 0.42x of \
                             that; wax = color, liquid = color2, glow = emissive, seed varies the motion)",
                        ),
                    ),
                ]),
            ),
            ("opacity", number("Blended materials: 0..1 (default glass 0.15, blob_shadow 0.6)")),
            ("emissive", number("Glow as a multiple of color, 0..1000 (default 0); above 1 reads as a light source")),
            ("metallic", number("standard: 0 (default) .. 1 = metal tinted by color")),
            ("roughness", number("standard: 0 = mirror-sharp .. 1 = broad highlights (default 0.5)")),
            ("specular", number("standard: highlight and reflection strength 0..10 (default 0 = matte)")),
            ("color2", described(&format!("Secondary colour (lava_lamp liquid), {color}"))),
            ("seed", number("Variation seed (lava_lamp wax motion)")),
            (
                "light",
                described(
                    "Point light carried by the entity: {intensity (1 = lights a white surface 1 m away fully), \
                     range (m, default 5), color (default: the entity colour), offset ([x,y,z] in local units)}; \
                     false removes it. At most 16 lights render.",
                ),
            ),
        ]);
    };
    let boolean = |desc: &str| Json::obj([("type", Json::from("boolean")), ("description", Json::from(desc))]);
    let string_array = |desc: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("string"))])),
            ("description", Json::from(desc)),
        ])
    };
    let object = |desc: &str| Json::obj([("type", Json::from("object")), ("description", Json::from(desc))]);
    // How every tool that changes or removes one entity names it.
    let target = |props: &mut Vec<(&str, Json)>| {
        props.extend([
            ("id", integer("Entity id")),
            ("path", string("Entity path, e.g. \"/slab/stones/ward_03\"")),
            ("name", string("Bare entity name; must be unique in the whole world (otherwise give path or id)")),
            (
                "expect_version",
                integer("Only apply if the entity is still at this version (from your last read); a stale write is \
                         rejected with the current state"),
            ),
        ]);
    };
    let tag_rule = "tags are 1..64 characters of A-Z a-z 0-9 _ - . :";
    let components = |props: &mut Vec<(&str, Json)>| {
        props.extend([
            ("tags", string_array(&format!("Labels for queries, e.g. [\"door\", \"spawn_point\"]; at most 32; {tag_rule}"))),
            ("props", object("Free-form game data (a JSON object, at most 16 KiB)")),
            (
                "collider",
                described(
                    "Reserved for physics (stored, not simulated yet): {shape: box (size [x,y,z], default 1) | sphere \
                     (radius, default 0.5) | capsule (radius, height = total height along local +Y), offset?, \
                     is_trigger?, layer?}, in local units; false removes it",
                ),
            ),
            (
                "body",
                described(
                    "Reserved for physics (stored, not simulated yet): \"static\", \"kinematic\", \"dynamic\" or \
                     {kind, mass (kg, default 1)}; false removes it",
                ),
            ),
        ]);
    };
    let filters = |props: &mut Vec<(&str, Json)>| {
        props.extend([
            ("path_prefix", string("Only this entity and everything below it, e.g. \"/slab\"")),
            ("tags", string_array("Only entities carrying all of these tags")),
            ("shape", string("Only this shape")),
            ("material", string("Only this material (groups have none)")),
            ("created_by", string("Only entities this agent created")),
            ("region", object("Only entities whose world bounds touch this box: {min: [x,y,z], max: [x,y,z]}")),
            ("cursor", string("next_cursor from the previous page (opaque); omit for the first page")),
        ]);
    };
    vec![
        ToolDef {
            name: "world_spawn".into(),
            description: "Add an entity to the live world: a shape with an optional parent, material, point light, \
                          tags, props and reserved physics components. Returns the new entity (id, path, transform, \
                          look, version)."
                .into(),
            input_schema: schema(
                {
                    let mut props = vec![
                        (
                            "shape",
                            Json::obj([
                                ("type", Json::from("string")),
                                ("enum", Json::arr(Shape::ALL.iter().map(|s| Json::from(s.name())))),
                                (
                                    "description",
                                    Json::from(
                                        "group draws nothing and holds children (districts, rooms, an object's parts); \
                                         cube and sphere are 1 m across; quad is a 1×1 m floor tile in XZ facing up; \
                                         lathe spins 'profile' around +Y (vases, bottles, lamp bases)",
                                    ),
                                ),
                            ]),
                        ),
                        (
                            "name",
                            string("Name unique among its siblings, up to 128 characters, no '/' (default: <shape>_<id>)"),
                        ),
                        ("parent", described("Parent entity: path (\"/slab\"), id, or unique name. Default: top level")),
                        ("path", string("Alternative to name + parent: the new entity's full path, e.g. \"/slab/stones/ward_03\"")),
                        ("position", vec3(&format!("Relative to the parent. {position}. Default [0,0,0]"))),
                        ("rotation_deg", vec3(&format!("{rotation}, relative to the parent. Default [0,0,0]"))),
                        ("scale", described(&format!("{scale}. Default 1"))),
                        ("color", described(&format!("{color}. Default light grey"))),
                        (
                            "profile",
                            described(
                                "lathe only: 2..256 [radius, height] points in metres, bottom to top; ends with a \
                                 radius are capped flat; corners sharper than 35° stay crisp",
                            ),
                        ),
                        ("segments", integer("lathe only: steps around the axis, 3..256 (default 32)")),
                    ];
                    look(&mut props);
                    components(&mut props);
                    props
                },
                &["shape"],
            ),
        },
        ToolDef {
            name: "world_set".into(),
            description: "Change an existing entity (by id, path or unique name) in place; its id stays the same and \
                          its version goes up by one. Give any of: rename, parent (null = top level; the local \
                          transform is kept, so it moves with its new parent), position, rotation_deg, scale, color, \
                          the look fields, light, a lathe's profile/segments, tags / add_tags / remove_tags, props \
                          (merged key by key; a null value removes a key), collider, body."
                .into(),
            input_schema: schema(
                {
                    let mut props = Vec::new();
                    target(&mut props);
                    props.extend([
                        ("rename", string("New name, unique among its siblings")),
                        ("parent", described("New parent: path, id or unique name; null moves it to the top level")),
                        ("position", vec3(&format!("Relative to the parent. {position}"))),
                        ("rotation_deg", vec3(rotation)),
                        ("scale", described(scale)),
                        ("color", described(color)),
                        ("profile", described("lathe only: new [radius, height] points, bottom to top")),
                        ("segments", integer("lathe only, with profile: steps around the axis, 3..256")),
                        ("add_tags", string_array("Tags to add")),
                        ("remove_tags", string_array("Tags to remove")),
                    ]);
                    look(&mut props);
                    components(&mut props);
                    props
                },
                &[],
            ),
        },
        ToolDef {
            name: "world_delete".into(),
            description: "Remove an entity (by id, path or unique name) and everything below it. Returns how many \
                          entities were removed."
                .into(),
            input_schema: schema(
                {
                    let mut props = Vec::new();
                    target(&mut props);
                    props
                },
                &[],
            ),
        },
        ToolDef {
            name: "world_list".into(),
            description: "Whole entities in path order (parents first, siblings by name), one page at a time: \
                          returns total, entities and next_cursor (null on the last page). Takes the same filters as \
                          world_query; prefer world_query for big worlds."
                .into(),
            input_schema: schema(
                {
                    let mut props = vec![("limit", integer("Entities per page, 1..200 (default 50)"))];
                    filters(&mut props);
                    props
                },
                &[],
            ),
        },
        ToolDef {
            name: "world_query".into(),
            description: "Find entities: every given filter must match. Results come in path order, one page at a \
                          time: returns total (all matches), items and next_cursor (null on the last page). Items \
                          carry id, path, shape, position, tags and version unless 'fields' asks for others."
                .into(),
            input_schema: schema(
                {
                    let mut props = vec![
                        ("limit", integer("Items per page, 1..500 (default 50)")),
                        (
                            "fields",
                            string_array(&format!(
                                "Fields per item (default {}), or [\"all\"]. Available: {}",
                                DEFAULT_FIELDS.join(", "),
                                world::FIELDS.join(", ")
                            )),
                        ),
                    ];
                    filters(&mut props);
                    props
                },
                &[],
            ),
        },
        ToolDef {
            name: "world_batch".into(),
            description: "Apply up to 1000 spawn / set / delete operations atomically: either all of them apply or \
                          none does (the failing op's index and error are returned). Later ops can refer to entities \
                          created earlier in the batch by path. Each op is {op: \"spawn\"|\"set\"|\"delete\", ...the \
                          same arguments as world_spawn / world_set / world_delete}. Returns one short result per op."
                .into(),
            input_schema: schema(
                vec![(
                    "ops",
                    Json::obj([
                        ("type", Json::from("array")),
                        ("items", Json::obj([("type", Json::from("object"))])),
                        ("description", Json::from("Operations, applied in order: [{op: \"spawn\", shape: ...}, ...]")),
                    ]),
                )],
                &["ops"],
            ),
        },
        ToolDef {
            name: "world_save".into(),
            description: "Save the world as text files in <worlds dir>/<name>/: world.json (environment, top-level \
                          entities without children) plus one file per top-level subtree, so diffs stay small. The \
                          engine then autosaves there every 10 s while there are changes, and on shutdown."
                .into(),
            input_schema: schema(
                vec![
                    ("name", string("World name: letters, digits, _ - . (default: the current world)")),
                    (
                        "overwrite",
                        boolean("Replace a different world that already exists under that name (default false)"),
                    ),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_load".into(),
            description: "Replace the whole live world (every agent's work) with a saved one and make it the current \
                          world. Refuses while there are unsaved changes unless force is true."
                .into(),
            input_schema: schema(
                vec![
                    ("name", string("World name (a folder in the worlds directory)")),
                    ("force", boolean("Discard unsaved changes (default false)")),
                ],
                &["name"],
            ),
        },
        ToolDef {
            name: "agent_hello".into(),
            description: "Say who you are: the name is recorded as created_by / modified_by on everything you \
                          change, and engine_info lists you with your role. Default: your MCP client name, role \
                          builder."
                .into(),
            input_schema: schema(
                vec![
                    ("name", string("Your agent name, up to 64 characters")),
                    ("role", string(&format!("builder (default), reviewer, playtester, director, ...; {tag_rule}"))),
                ],
                &["name"],
            ),
        },
        ToolDef {
            name: "sight_frame".into(),
            description: "Render the world from a camera and return the PNG image plus JSON with each entity's \
                          on-screen rectangle [x, y, w, h] (pixels, origin top-left): the engine's projection of its \
                          bounding box, clipped to the view; occlusion is not considered. Without eye/target the camera \
                          frames the whole world from a three-quarter view. Large worlds return the biggest on-screen \
                          subjects first (max_subjects); the full list is saved at packet_path."
                .into(),
            input_schema: schema(
                vec![
                    ("width", integer("Image width 64..2048 (default 960)")),
                    ("height", integer("Image height 64..2048 (default 540)")),
                    ("eye", vec3("Camera position (give with target)")),
                    ("target", vec3("Point the camera looks at (give with eye; must differ from eye)")),
                    ("fov_y_deg", Json::obj([("type", Json::from("number")), ("description", Json::from("Vertical field of view, 10..120 (default 50)"))])),
                    ("max_subjects", integer("Subjects to include in the response, 1..500 (default 50)")),
                    (
                        "time_s",
                        number("Animation time to render in seconds (default: now). Fix it to get reproducible frames."),
                    ),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_env".into(),
            description: "Scene lighting: give any field to change it; returns the whole environment. Defaults: \
                          white sun 0.75 from [0.4,0.8,0.45], white ambient 0.25, exposure 1, no tone mapping, \
                          background #1c1d21. For night scenes lit by lamps: sun_intensity 0, ambient ~0.02, \
                          tonemap true."
                .into(),
            input_schema: schema(
                vec![
                    ("sun_direction", vec3("Direction towards the sun (any length)")),
                    ("sun_color", described(color)),
                    ("sun_intensity", number("0..1000")),
                    ("ambient_color", described(color)),
                    ("ambient_intensity", number("0..1000")),
                    ("exposure", number("Multiplies all light before display, 0..1000")),
                    ("tonemap", Json::obj([("type", Json::from("boolean")), ("description", Json::from("Filmic tone mapping + dithering; true suits bright lights and glow"))])),
                    ("background", described(&format!("Background {color}"))),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "window_view".into(),
            description: "Point the human's window camera (the live view, not captures): eye + target (+ fov_y_deg), \
                          or auto: true to frame the whole world again."
                .into(),
            input_schema: schema(
                vec![
                    ("eye", vec3("Camera position")),
                    ("target", vec3("Point the camera looks at")),
                    ("fov_y_deg", number("Vertical field of view, 10..120 (default 50)")),
                    ("auto", Json::obj([("type", Json::from("boolean")), ("description", Json::from("true: auto-frame the world"))])),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "engine_info".into(),
            description: "Engine status: GPU, validation layer state, the current world (name, entities, unsaved \
                          changes, state hash) and the connected agents with their roles."
                .into(),
            input_schema: schema(vec![], &[]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    #[test]
    fn spawn_defaults_and_values() {
        let s = parse_spawn(&args(r##"{"shape":"cube"}"##)).unwrap();
        assert_eq!(s.shape, Shape::Cube);
        assert_eq!(s.scale, Vec3::ONE);
        let s = parse_spawn(&args(r##"{"shape":"sphere","name":"sun","position":[1,2,3],"scale":2,"color":"#f80"}"##))
            .unwrap();
        assert_eq!(s.name.as_deref(), Some("sun"));
        assert_eq!(s.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(s.scale, Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(s.color, [1.0, 136.0 / 255.0, 0.0]);
    }

    #[test]
    fn spawn_rejects_bad_input() {
        let long = format!(r#"{{"shape":"cube","name":"{}"}}"#, "n".repeat(129));
        for bad in [
            r#"{}"#,
            r#"{"shape":"cone"}"#,
            r#"{"shape":"cube","position":[1,2]}"#,
            r#"{"shape":"cube","position":[1e30,0,0]}"#,
            r#"{"shape":"cube","position":[1e300,0,0]}"#,
            r#"{"shape":"cube","scale":0}"#,
            r#"{"shape":"cube","scale":1e39}"#,
            r#"{"shape":"cube","scale":[1,1e-9,1]}"#,
            r#"{"shape":"cube","color":"red"}"#,
            r#"{"shape":"cube","color":[2,0,0]}"#,
            r#"{"shape":"cube","name":"  "}"#,
            r#"{"shape":"cube","name":"a\nb"}"#,
            &long,
        ] {
            assert!(parse_spawn(&args(bad)).is_err(), "{bad} should be rejected");
        }
        assert_eq!(parse_spawn(&args(r#"{"shape":1}"#)).err().unwrap(), "'shape' must be a string: group, cube, quad, sphere or lathe");
    }

    #[test]
    fn spawn_looks_lights_and_lathes() {
        let s = parse_spawn(&args(
            r##"{"shape":"cube","material":"lava_lamp","emissive":2.5,"color2":"#ff8800","seed":3,
                "light":{"intensity":1.5,"range":4,"offset":[0,0.2,0]}}"##,
        ))
        .unwrap();
        assert_eq!(s.look.material, specula_gpu::Material::LavaLamp);
        assert_eq!((s.look.surface.emissive, s.look.surface.seed), (2.5, 3.0));
        let light = s.light.unwrap();
        assert_eq!((light.intensity, light.range, light.offset), (1.5, 4.0, Vec3::new(0.0, 0.2, 0.0)));
        let s = parse_spawn(&args(r#"{"shape":"cube","material":"glass"}"#)).unwrap();
        assert_eq!(s.look.opacity, 0.15, "glass defaults to a light body");
        let s = parse_spawn(&args(r#"{"shape":"lathe","profile":[[0.1,0],[0.05,0.3]],"segments":12}"#)).unwrap();
        assert_eq!(s.profile.unwrap().segments, 12);
        for bad in [
            r#"{"shape":"lathe"}"#,
            r#"{"shape":"cube","profile":[[0.1,0],[0.1,1]]}"#,
            r#"{"shape":"lathe","profile":[[0.1,0]]}"#,
            r#"{"shape":"lathe","profile":[[0,0],[0,1]]}"#,
            r#"{"shape":"lathe","profile":[[-1,0],[1,1]]}"#,
            r#"{"shape":"lathe","profile":[[1,0],[1,1]],"segments":2}"#,
            r#"{"shape":"cube","segments":8}"#,
            r#"{"shape":"cube","material":"plastic"}"#,
            r#"{"shape":"cube","opacity":2}"#,
            r#"{"shape":"cube","emissive":-1}"#,
            r#"{"shape":"cube","light":{"range":2}}"#,
            r#"{"shape":"cube","light":true}"#,
            r##"{"shape":"cube","color2":"orange"}"##,
        ] {
            assert!(parse_spawn(&args(bad)).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn env_and_view_arguments() {
        let e = parse_env(&args(r##"{"sun_intensity":0,"tonemap":true,"background":"#000"}"##)).unwrap();
        assert_eq!((e.sun_intensity, e.tonemap, e.background), (Some(0.0), Some(true), Some([0.0; 3])));
        assert!(parse_env(&args(r#"{"sun_direction":[0,0,0]}"#)).is_err());
        assert!(parse_env(&args(r#"{"tonemap":1}"#)).is_err());
        assert!(matches!(parse_view(&args(r#"{"auto":true}"#)), Ok(None)));
        assert!(matches!(parse_view(&args(r#"{"eye":[0,1,2],"target":[0,0,0]}"#)), Ok(Some(_))));
        assert!(parse_view(&args("{}")).is_err());
    }

    #[test]
    fn set_needs_a_key_and_a_change() {
        assert!(matches!(parse_set(&args(r#"{"id":3,"position":[1,2,3]}"#)), Ok((EntityKey::Id(3), _))));
        assert!(matches!(parse_set(&args(r#"{"name":"a","rename":"b"}"#)), Ok((EntityKey::Name(_), _))));
        assert!(matches!(parse_set(&args(r#"{"path":"/a/b","rename":"c"}"#)), Ok((EntityKey::Path(_), _))));
        assert!(parse_set(&args(r#"{"id":3}"#)).is_err(), "no change");
        assert!(parse_set(&args(r#"{"id":3,"expect_version":2}"#)).is_err(), "a version alone is no change");
        let both = parse_set(&args(r#"{"id":1,"name":"b2","position":[0,0,0]}"#)).err().unwrap();
        assert!(both.contains("not both") && both.contains("rename"), "{both}");
        assert!(parse_set(&args(r#"{"path":"/a","name":"b","position":[0,0,0]}"#)).is_err());
        assert!(parse_set(&args(r#"{"position":[0,0,0]}"#)).is_err(), "no key");
        assert!(parse_set(&args(r##"{"id":-1,"color":"#fff"}"##)).is_err());
        assert!(parse_set(&args(r#"{"id":1,"scale":0}"#)).is_err());
        assert!(parse_set(&args(r#"{"id":1,"rename":"a/b"}"#)).is_err(), "names have no '/'");
        assert!(parse_set(&args(r#"{"id":1,"emissive":2}"#)).is_ok());
        assert!(matches!(parse_set(&args(r#"{"id":1,"light":false}"#)), Ok((_, UpdateSpec { light: Some(None), .. }))));
        let (_, c) = parse_set(&args(r#"{"id":1,"parent":null,"expect_version":4}"#)).unwrap();
        assert_eq!((c.parent, c.expect_version), (Some(None), Some(4)));
        let (_, c) = parse_set(&args(r#"{"id":1,"parent":"/slab","add_tags":["a"],"props":{"k":null}}"#)).unwrap();
        assert_eq!(c.parent, Some(Some(EntityKey::Path("/slab".into()))));
        assert_eq!((c.add_tags, c.props), (vec!["a".to_string()], Some(args(r#"{"k":null}"#))));
        let (_, c) = parse_set(&args(r#"{"id":1,"parent":7,"collider":false,"body":"dynamic"}"#)).unwrap();
        assert_eq!((c.parent, c.collider), (Some(Some(EntityKey::Id(7))), Some(None)));
        assert_eq!(c.body, Some(Some(Body { kind: BodyKind::Dynamic, mass: 1.0 })));
        assert!(parse_set(&args(r##"{"id":1,"expect_version":0,"color":"#fff"}"##)).is_err());
    }

    #[test]
    fn spawn_hierarchy_and_components() {
        let s = parse_spawn(&args(r#"{"shape":"group","path":"/slab/stones"}"#)).unwrap();
        assert_eq!((s.name.as_deref(), s.parent), (Some("stones"), Some(EntityKey::Path("/slab".into()))));
        let s = parse_spawn(&args(r#"{"shape":"group","path":"/slab"}"#)).unwrap();
        assert_eq!((s.name.as_deref(), s.parent), (Some("slab"), None));
        let s = parse_spawn(&args(r#"{"shape":"cube","name":"w","parent":"gate"}"#)).unwrap();
        assert_eq!(s.parent, Some(EntityKey::Name("gate".into())));
        let s = parse_spawn(&args(
            r#"{"shape":"cube","tags":["b","a","a"],"props":{"hp":3},
                "collider":{"shape":"capsule","radius":0.3,"height":1.8,"is_trigger":true,"layer":"player"},
                "body":{"kind":"kinematic","mass":70}}"#,
        ))
        .unwrap();
        assert_eq!(s.tags.iter().map(String::as_str).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(s.props, args(r#"{"hp":3}"#));
        let c = s.collider.unwrap();
        assert_eq!((c.shape, c.is_trigger, c.layer.as_str()), (ColliderShape::Capsule { radius: 0.3, height: 1.8 }, true, "player"));
        assert_eq!(s.body, Some(Body { kind: BodyKind::Kinematic, mass: 70.0 }));
        let s = parse_spawn(&args(r#"{"shape":"cube","collider":{"shape":"box"}}"#)).unwrap();
        assert_eq!(s.collider.unwrap().shape, ColliderShape::Box { size: Vec3::ONE });
        for bad in [
            r#"{"shape":"cube","name":"a/b"}"#,
            r#"{"shape":"cube","path":"slab/x"}"#,
            r#"{"shape":"cube","path":"/"}"#,
            r#"{"shape":"cube","path":"/a/b","name":"c"}"#,
            r#"{"shape":"cube","parent":-2}"#,
            r#"{"shape":"cube","tags":"door"}"#,
            r#"{"shape":"cube","tags":["no spaces"]}"#,
            r#"{"shape":"cube","props":[1]}"#,
            r#"{"shape":"cube","collider":{"shape":"capsule","radius":1,"height":1}}"#,
            r#"{"shape":"cube","collider":{"shape":"cone"}}"#,
            r#"{"shape":"cube","collider":{"shape":"sphere","radius":0}}"#,
            r#"{"shape":"cube","body":"floaty"}"#,
            r#"{"shape":"cube","body":{"kind":"dynamic","mass":0}}"#,
        ] {
            assert!(parse_spawn(&args(bad)).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn query_list_and_batch_arguments() {
        let q = parse_query(&args("{}"), false).unwrap();
        assert_eq!((q.query.limit, q.key, q.fields.as_ref().unwrap().len()), (50, "items", DEFAULT_FIELDS.len()));
        let l = parse_query(&args("{}"), true).unwrap();
        assert_eq!((l.query.limit, l.key, l.fields), (50, "entities", None));
        assert!(parse_query(&args(r#"{"limit":201}"#), true).is_err());
        assert!(parse_query(&args(r#"{"limit":500}"#), false).is_ok());
        let cursor = cursor_for("/a/b");
        let q = parse_query(
            &args(&format!(
                r#"{{"cursor":"{cursor}","path_prefix":"/a","tags":["x"],"shape":"cube","material":"glass",
                    "created_by":"bob","region":{{"min":[0,0,0],"max":[1,1,1]}},"fields":["path","bounds"]}}"#
            )),
            false,
        )
        .unwrap();
        assert_eq!(q.query.after.as_deref(), Some("/a/b"));
        assert_eq!((q.query.shape, q.query.material), (Some(Shape::Cube), Some(specula_gpu::Material::Glass)));
        assert_eq!(q.fields.unwrap(), ["path", "bounds"]);
        assert_eq!(parse_query(&args(r#"{"fields":["all"]}"#), false).unwrap().fields, None);
        for bad in [
            r#"{"cursor":"!!"}"#,
            r#"{"path_prefix":"a"}"#,
            r#"{"fields":["nope"]}"#,
            r#"{"region":{"min":[1,1,1],"max":[0,0,0]}}"#,
            r#"{"shape":"cone"}"#,
        ] {
            assert!(parse_query(&args(bad), false).is_err(), "{bad}");
        }

        let ops = parse_batch(&args(
            r#"{"ops":[{"op":"spawn","shape":"group","path":"/g"},{"op":"set","path":"/g","position":[1,0,0]},
                {"op":"delete","path":"/g","expect_version":2}]}"#,
        ))
        .unwrap();
        assert!(matches!(ops.as_slice(), [Op::Spawn(_), Op::Set(..), Op::Delete(_, Some(2))]));
        let e = parse_batch(&args(r#"{"ops":[{"op":"spawn","shape":"cube"},{"op":"set","id":1}]}"#)).err().unwrap();
        assert!(e.starts_with("ops[1] (set): "), "{e}");
        assert!(parse_batch(&args(r#"{"ops":[{"op":"move"}]}"#)).is_err());
        assert!(parse_batch(&args(r#"{"ops":[]}"#)).is_err());
        let many = format!(r#"{{"ops":[{}]}}"#, vec![r#"{"op":"spawn","shape":"cube"}"#; MAX_BATCH_OPS + 1].join(","));
        assert!(parse_batch(&args(&many)).err().unwrap().contains("1000"));
    }

    #[test]
    fn persistence_and_identity_arguments() {
        assert!(matches!(parse("world_save", &args("{}")), Ok(Command::Save { name: None, overwrite: false })));
        assert!(matches!(parse("world_load", &args(r#"{"name":"grey","force":true}"#)), Ok(Command::Load { force: true, .. })));
        assert!(parse("world_load", &args("{}")).is_err());
        assert!(parse("world_save", &args(r#"{"name":"../escape"}"#)).is_err());
        assert!(matches!(parse("agent_hello", &args(r#"{"name":"mason"}"#)), Ok(Command::Hello { role, .. }) if role == "builder"));
        assert!(parse("agent_hello", &args(r#"{"name":"mason","role":"chief builder"}"#)).is_err());
        assert!(parse("agent_hello", &args("{}")).is_err());
    }

    /// Reviewer finding: unknown arguments were silently ignored.
    #[test]
    fn unknown_arguments_are_rejected_with_a_suggestion() {
        let e = parse("world_spawn", &args(r#"{"shape":"cube","rotation":[0,90,0]}"#)).err().unwrap();
        assert!(e.contains("'rotation'") && e.contains("did you mean 'rotation_deg'"), "{e}");
        assert_eq!(argument_error(&e).get("field"), Some(&Json::from("rotation")));
        let e = parse("world_set", &args(r##"{"id":1,"colour":"#fff"}"##)).err().unwrap();
        assert!(e.contains("did you mean 'color'"), "{e}");
        assert!(parse("world_set", &args(r#"{"id":1,"rename":"x","add_tags":["a"]}"#)).is_ok());
        let e = parse("world_spawn", &args(r#"{"shape":"cube","light":{"intensity":1,"radius":2}}"#)).err().unwrap();
        assert_eq!(argument_error(&e).get("field"), Some(&Json::from("light.radius")), "{e}");
        let e = parse("world_spawn", &args(r#"{"shape":"cube","collider":{"shape":"box","mask":1}}"#)).err().unwrap();
        assert_eq!(argument_error(&e).get("field"), Some(&Json::from("collider.mask")), "{e}");
        let e = parse("world_batch", &args(r#"{"ops":[{"op":"spawn","shape":"cube","pos":[0,0,0]}]}"#)).err().unwrap();
        assert!(e.starts_with("ops[0] (spawn): 'pos'"), "{e}");
        assert!(parse("world_batch", &args(r#"{"ops":[{"op":"delete","id":3,"expect_version":1}]}"#)).is_ok());
        for (tool, bad) in [
            ("world_query", r#"{"tag":["x"]}"#),
            ("world_list", r#"{"page":2}"#),
            ("world_delete", r#"{"id":1,"recursive":true}"#),
            ("sight_frame", r#"{"fov":60}"#),
            ("world_env", r#"{"fog":1}"#),
            ("world_save", r#"{"name":"a","force":true}"#),
            ("engine_info", r#"{"verbose":true}"#),
            ("world_query", r#"{"region":{"min":[0,0,0],"max":[1,1,1],"mid":[0,0,0]}}"#),
        ] {
            assert!(parse(tool, &args(bad)).is_err(), "{tool} {bad}");
        }
        assert_eq!(closest("rotation", SPAWN_KEYS.iter().copied()), Some("rotation_deg"));
        assert_eq!(closest("zzzzzz", SPAWN_KEYS.iter().copied()), None);
    }

    #[test]
    fn value_ranges_come_from_the_world() {
        for bad in [
            r#"{"shape":"cube","position":[0,2e6,0]}"#,
            r#"{"shape":"cube","color":[1.5,0,0]}"#,
            r#"{"shape":"cube","light":{"intensity":1,"range":0}}"#,
            r#"{"shape":"lathe","profile":[[1,0],[1,1]],"segments":1000}"#,
        ] {
            let e = parse_spawn(&args(bad)).err().unwrap();
            assert!(e.starts_with('\''), "messages lead with the field: {e}");
        }
        assert!(parse_env(&args(r#"{"exposure":5000}"#)).is_err());
    }

    #[test]
    fn argument_errors_name_their_field() {
        let e = argument_error("'scale' components must be between 0.001 and 100000");
        assert_eq!(e.get("field"), Some(&Json::from("scale")));
        assert_eq!(e.get("error"), Some(&Json::from("invalid_argument")));
        assert_eq!(argument_error("names must not be empty").get("field"), None);
        let shape = parse_spawn(&args(r#"{"shape":"cone"}"#)).err().unwrap();
        assert_eq!(argument_error(&shape).get("field"), Some(&Json::from("shape")), "{shape}");
    }

    #[test]
    fn frame_arguments() {
        let f = parse_frame(&args("{}")).unwrap();
        assert_eq!((f.width, f.height, f.eye.is_none(), f.max_subjects), (960, 540, true, 50));
        assert!(parse_frame(&args(r#"{"eye":[0,1,2]}"#)).is_err());
        assert!(parse_frame(&args(r#"{"width":10}"#)).is_err());
        assert!(parse_frame(&args(r#"{"width":4294967360}"#)).is_err(), "u32 wrap-around");
        assert!(parse_frame(&args(r#"{"eye":[1,1,1],"target":[1,1,1]}"#)).is_err(), "eye == target");
        assert!(parse_frame(&args(r#"{"max_subjects":0}"#)).is_err());
        let f = parse_frame(&args(r#"{"eye":[0,10,0],"target":[0,0,0],"fov_y_deg":70,"max_subjects":5}"#)).unwrap();
        assert_eq!((f.eye, f.max_subjects), (Some(Vec3::new(0.0, 10.0, 0.0)), 5));
        assert_eq!(parse_frame(&args(r#"{"time_s":12.5}"#)).unwrap().time_s, Some(12.5));
        assert!(parse_frame(&args(r#"{"time_s":-1}"#)).is_err());
    }

    #[test]
    fn tool_names_are_claude_code_safe() {
        for t in definitions() {
            assert!(t.name.len() <= 64 && t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        }
    }
}
