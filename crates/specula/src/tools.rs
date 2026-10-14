//! MCP tools agents use to drive the engine.
//!
//! Tool calls arrive on protocol threads; each is parsed and validated here, sent to the main
//! thread as a [`Command`], and answered with the main thread's [`ToolOutput`]. The world is only
//! ever touched by the main thread, between frames.

use crate::claims::{self, ClaimSpec, Scope};
use crate::journal::{self, ChangesQuery, Cursor, HistoryQuery, Kind, UndoRequest};
use crate::perf::{CaptureSpec, ZonesQuery, MAX_CAPTURE_FRAMES};
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
newer change. Shape 'model' builds a procedural model from a recipe file (model_build reports on one without \
spawning). Entities can be hidden (visible: false) or put on the 'ui' layer (left out of captures); a hosted \
game's 'transient' entities are its presentation (never saved or journaled, read-only to agents). LOOK with sight_frame: it returns the rendered image plus a subject list giving each entity's \
on-screen rectangle (its projected bounding box, computed by the engine). world_env sets the sun, ambient light, \
background and tone mapping; window_view points the human's window. world_save / world_load keep the world in \
text files. Say who you are with agent_hello. perf_frame reports GPU time per pass, draw counts, memory, \
frame pacing and budget errors (budgets from perf.toml); perf_zones shows CPU time per engine phase, \
perf_memory the heap, and perf_capture records frames to a file and a trace timeline; time_control pauses, steps or speeds up the simulation clock. A game running in \
the engine adds its own tools (play_*). Coordinates: metres, right-handed, +Y up; a top-down view puts \
north (-Z) at the top. Other agents may be building in the same world at the same time: say who you are with \
agent_hello. Names are unique among open sessions, idle ones included (a taken name is refused with a free \
suggestion; the hosted game's name is reserved: name_reserved); \
without agent_hello you are '<client name>-<6 characters of your session id>'. Your claims and 'your own changes' \
belong to your name in this session: a later session using the same name adopts none of them. claim_take the \
subtree or region you will build in and renew it while you work: writes inside another agent's live claim are \
rejected with error 'claimed' (also when renaming, moving, rotating or scaling something would carry \
entities into or out of a claim), and 'claims_overlap' means a path claim and a region claim meet there (claim_share or release one). \
world_load is refused while other agents hold claims. Every change is journaled with its author and tick: \
world_history shows who did what, world_undo undoes your own recent changes (refusing to overwrite someone \
else's later work unless forced; claims always apply), world_changes is a feed to poll for other agents' \
changes, and world_hash gives the state hash per tick.";

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
pub(crate) const SPAWN_KEYS: &[&str] = &[
    "shape", "name", "parent", "path", "position", "rotation_deg", "scale", "color", "profile", "segments", "recipe",
    "params", "visible", "layer", "transient", "sprite", "tilemap", "shape2d",
];
const SET_KEYS: &[&str] = &[
    "id", "path", "name", "expect_version", "rename", "parent", "position", "rotation_deg", "scale", "color", "profile",
    "segments", "add_tags", "remove_tags", "recipe", "params", "visible", "layer", "sprite", "tilemap", "shape2d",
];
const SHAPE_LIST: &str = "group, cube, quad, sphere, lathe, model, sprite, tilemap or shape2d";
/// The 2D components (their keys are also their shapes' names).
const TWO_D_KEYS: [&str; 3] = ["sprite", "tilemap", "shape2d"];
const DELETE_KEYS: &[&str] = &["id", "path", "name", "expect_version"];
const QUERY_KEYS: &[&str] = &["limit", "fields", "path_prefix", "tags", "shape", "material", "created_by", "region", "cursor"];
const FRAME_KEYS: &[&str] =
    &["width", "height", "eye", "target", "fov_y_deg", "ortho", "view_2d", "max_subjects", "time_s", "include_ui", "lod"];
pub(crate) const ENV_KEYS: &[&str] = &[
    "sun_direction", "sun_color", "sun_intensity", "ambient_color", "ambient_intensity", "exposure", "tonemap",
    "background", "shadow", "bloom", "fxaa", "sky", "fog",
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
    /// `resume`: a resume_token from an earlier agent_hello (see `swarm`).
    Hello { name: String, role: String, resume: Option<String> },
    Frame(FrameSpec),
    Env(EnvPatch),
    /// `None` returns the window to auto-framing.
    View(Option<Camera>),
    Info,
    /// perf_frame; `None` = the frame budget from perf.toml.
    Perf { budget_ms: Option<f64> },
    PerfZones(ZonesQuery),
    /// perf_memory: top zones by allocation.
    PerfMemory { limit: usize },
    /// perf_capture: answered once the frames are recorded.
    PerfCapture(CaptureSpec),
    Time(TimeOp),
    /// A tool the hosted game added.
    Game { name: String, args: Json },
    History(HistoryQuery),
    Undo(UndoRequest),
    Changes(ChangesQuery),
    /// world_hash: the last `recent` per-tick hashes, the hash at `tick`, and a replay check.
    Hash { recent: usize, tick: Option<u64>, verify: bool },
    ClaimTake(ClaimSpec),
    /// `id: None` = all the caller's claims.
    ClaimRenew { id: Option<u64>, lease: Duration },
    /// `id: None` = all the caller's claims.
    ClaimRelease { id: Option<u64> },
    ClaimShare { id: u64, with: String, revoke: bool },
    ClaimList { owner: Option<String>, mine: bool, path_prefix: Option<String>, after: Option<u64>, limit: usize },
    /// model_build: build a recipe (fresh from disk) and report on it, without spawning.
    ModelBuild(world::ModelSpec),
    /// model_reload: re-read every recipe and style now.
    ModelReload,
    /// tilemap_get: cells of one layer (all layers' names when `layer` is `None`) in `region`.
    TileGet { key: EntityKey, layer: Option<String>, region: Option<[u32; 4]> },
}

/// time_control's actions on the simulation clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeOp {
    Pause,
    Resume,
    /// Pause, then run exactly this many ticks.
    Step(u64),
    Scale(f32),
    Info,
}

/// Most ticks one time_control step may run (about 10 minutes of game time at 30 Hz).
const MAX_STEP_TICKS: u64 = 18_000;

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
    /// Orthographic view `height_m` tall (`ortho` or `view_2d`); `None` = perspective.
    pub ortho: Option<f32>,
    /// Subjects returned in the response (the saved packet always has all of them).
    pub max_subjects: usize,
    /// Animation time to render; `None` = the engine's current time.
    pub time_s: Option<f32>,
    /// Draw (and list) `ui`-layer entities too.
    pub include_ui: bool,
    /// Draw every model at this LOD (clamped to the model's last) instead of choosing by size.
    pub lod: Option<usize>,
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
    /// Tools the hosted game added (answered by the game on the engine thread).
    game_tools: Vec<ToolDef>,
}

impl Tools {
    /// Game tools whose names clash with an engine tool are dropped, with a message.
    pub fn new(to_engine: Sender<Request>, game_tools: Vec<ToolDef>) -> Tools {
        let engine: Vec<String> = definitions().into_iter().map(|t| t.name).collect();
        let (game_tools, clashes): (Vec<ToolDef>, Vec<ToolDef>) =
            game_tools.into_iter().partition(|t| !engine.contains(&t.name));
        for t in clashes {
            eprintln!("specula: the game's tool '{}' clashes with an engine tool and is ignored", t.name);
        }
        Tools { to_engine, game_tools }
    }
}

impl ToolHandler for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        let mut all = definitions();
        all.extend(self.game_tools.iter().cloned());
        all
    }

    fn call(&self, session: &SessionInfo, name: &str, arguments: &Json) -> ToolOutput {
        let parsed = if self.game_tools.iter().any(|t| t.name == name) {
            Ok(Command::Game { name: name.to_owned(), args: arguments.clone() })
        } else {
            parse(name, arguments)
        };
        let command = match parsed {
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
            check_keys(args, name, &[&["eye", "target", "fov_y_deg", "ortho", "view_2d", "auto"]])?;
            parse_view(args).map(Command::View)
        }
        "engine_info" => check_keys(args, name, &[]).map(|()| Command::Info),
        "perf_frame" => {
            check_keys(args, name, &[&["budget_ms"]])?;
            let budget_ms = match args.get("budget_ms") {
                None | Some(Json::Null) => None,
                Some(v) => match v.as_f64() {
                    Some(ms) if (0.1..=10_000.0).contains(&ms) => Some(ms),
                    _ => return Err("'budget_ms' must be a number of milliseconds, 0.1..10000".into()),
                },
            };
            Ok(Command::Perf { budget_ms })
        }
        "perf_zones" => parse_perf_zones(args).map(Command::PerfZones),
        "perf_memory" => {
            check_keys(args, name, &[&["limit"]])?;
            Ok(Command::PerfMemory { limit: int_arg(args, "limit", 1, 50)?.unwrap_or(10) as usize })
        }
        "perf_capture" => parse_perf_capture(args).map(Command::PerfCapture),
        "time_control" => {
            check_keys(args, name, &[&["action", "ticks", "scale"]])?;
            parse_time(args).map(Command::Time)
        }
        "world_history" => parse_history(args).map(Command::History),
        "world_undo" => parse_undo(args).map(Command::Undo),
        "world_changes" => parse_changes(args).map(Command::Changes),
        "world_hash" => {
            check_keys(args, name, &[&["recent", "tick", "verify"]])?;
            Ok(Command::Hash {
                recent: int_arg(args, "recent", 0, MAX_HASH_RECENT)?.unwrap_or(8) as usize,
                tick: int_arg(args, "tick", 0, u64::MAX >> 11)?,
                verify: bool_arg(args, "verify")?.unwrap_or(false),
            })
        }
        "claim_take" => parse_claim_take(args).map(Command::ClaimTake),
        "claim_renew" => {
            check_keys(args, name, &[&["id", "all", "lease_s"]])?;
            Ok(Command::ClaimRenew { id: claim_target(args, name)?, lease: lease_arg(args)? })
        }
        "claim_release" => {
            check_keys(args, name, &[&["id", "all"]])?;
            Ok(Command::ClaimRelease { id: claim_target(args, name)? })
        }
        "claim_share" => {
            check_keys(args, name, &[&["id", "with", "revoke"]])?;
            let id = int_arg(args, "id", 1, u64::MAX >> 11)?.ok_or("'id' is required: the claim to share")?;
            let with = name_arg(args.get("with"))?.ok_or("'with' is required: the agent name to share the claim with")?;
            Ok(Command::ClaimShare { id, with, revoke: bool_arg(args, "revoke")?.unwrap_or(false) })
        }
        "claim_list" => {
            check_keys(args, name, &[&["owner", "mine", "path_prefix", "cursor", "limit"]])?;
            let path_prefix = string_arg(args, "path_prefix")?.map(str::to_owned);
            if path_prefix.as_ref().is_some_and(|p| !p.starts_with('/')) {
                return Err("'path_prefix' must be a path like \"/slab\"".into());
            }
            Ok(Command::ClaimList {
                owner: string_arg(args, "owner")?.map(str::to_owned),
                mine: bool_arg(args, "mine")?.unwrap_or(false),
                path_prefix,
                after: int_arg(args, "cursor", 0, u64::MAX >> 11)?,
                limit: int_arg(args, "limit", 1, 200)?.unwrap_or(50) as usize,
            })
        }
        "model_build" => {
            check_keys(args, name, &[&["recipe", "params", "seed"]])?;
            let recipe = string_arg(args, "recipe")?.ok_or("'recipe' is required: a recipe file under the assets root")?;
            let seed = int_arg(args, "seed", 0, world::MAX_MODEL_SEED)?;
            let spec = world::ModelSpec { recipe: recipe.to_owned(), params: params_arg(args)?.unwrap_or_default(), seed };
            spec.check().map_err(value_error)?;
            Ok(Command::ModelBuild(spec))
        }
        "model_reload" => check_keys(args, name, &[]).map(|()| Command::ModelReload),
        "tilemap_set" => {
            check_keys(args, name, &[&["id", "path", "name", "expect_version", "layer", "cells"]])?;
            let key = entity_key(args, name)?;
            let layer = string_arg(args, "layer")?.ok_or("'layer' is required: the layer's name")?;
            let cells = args.get("cells").filter(|c| !c.is_null()).ok_or("'cells' is required: [[x, y, tile], ...]")?;
            let tiles = crate::two_d::parse_tile_edits(layer, cells)?;
            Ok(Command::Set(key, UpdateSpec { tiles: Some(tiles), expect_version: version_arg(args)?, ..Default::default() }))
        }
        "tilemap_get" => {
            check_keys(args, name, &[&["id", "path", "name", "layer", "region"]])?;
            let key = entity_key(args, name)?;
            let region = match args.get("region").filter(|r| !r.is_null()) {
                None => None,
                Some(r) => {
                    let v: Option<Vec<u64>> = r.as_array().and_then(|a| a.iter().map(Json::as_u64).collect());
                    match v.as_deref() {
                        Some(&[x, y, w, h]) if w >= 1 && h >= 1 && w * h <= u64::from(crate::two_d::MAX_TILE_READ) && x < 1 << 20 && y < 1 << 20 => {
                            Some([x as u32, y as u32, w as u32, h as u32])
                        }
                        _ => {
                            return Err(format!(
                                "'region' must be [x, y, width, height] covering at most {} cells",
                                crate::two_d::MAX_TILE_READ
                            ))
                        }
                    }
                }
            };
            Ok(Command::TileGet { key, layer: string_arg(args, "layer")?.map(str::to_owned), region })
        }
        other => Err(format!("unknown tool '{other}'")),
    }
}

/// Recent per-tick hashes world_hash may return.
pub const MAX_HASH_RECENT: u64 = 256;

fn parse_perf_zones(args: &Json) -> Result<ZonesQuery, String> {
    check_keys(args, "perf_zones", &[&["thread", "min_ms", "limit", "cursor", "enabled"]])?;
    let min_ms = match args.get("min_ms") {
        None | Some(Json::Null) => 0.0,
        Some(v) => match v.as_f64() {
            Some(x) if (0.0..=10_000.0).contains(&x) => x,
            _ => return Err("'min_ms' must be a number of milliseconds, 0..10000".into()),
        },
    };
    let offset = match string_arg(args, "cursor")? {
        None => 0,
        Some(c) => c.parse().map_err(|_| "'cursor' must be the next_cursor of an earlier perf_zones answer".to_string())?,
    };
    Ok(ZonesQuery {
        thread: string_arg(args, "thread")?.map(str::to_owned),
        min_ms,
        limit: int_arg(args, "limit", 1, 500)?.unwrap_or(60) as usize,
        offset,
        enabled: bool_arg(args, "enabled")?,
    })
}

fn parse_perf_capture(args: &Json) -> Result<CaptureSpec, String> {
    check_keys(args, "perf_capture", &[&["frames", "trace", "label"]])?;
    let label = string_arg(args, "label")?.map(str::to_owned);
    if let Some(l) = &label {
        if l.is_empty() || l.len() > 32 || !l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            return Err("'label' must be 1..32 characters of A-Z a-z 0-9 _ -".into());
        }
    }
    Ok(CaptureSpec {
        frames: int_arg(args, "frames", 1, MAX_CAPTURE_FRAMES)?.unwrap_or(60),
        trace: bool_arg(args, "trace")?.unwrap_or(true),
        label,
    })
}

/// An optional integer within `lo..=hi`.
fn int_arg(args: &Json, key: &str, lo: u64, hi: u64) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(Some(n)),
            _ => Err(format!("'{key}' must be an integer from {lo} to {hi}")),
        },
    }
}

fn parse_history(args: &Json) -> Result<HistoryQuery, String> {
    check_keys(
        args,
        "world_history",
        &[&["agent", "path_prefix", "entity", "tick_from", "tick_to", "kinds", "cursor", "limit", "detail"]],
    )?;
    let path_prefix = string_arg(args, "path_prefix")?.map(str::to_owned);
    if path_prefix.as_ref().is_some_and(|p| !p.starts_with('/')) {
        return Err("'path_prefix' must be a path like \"/slab\"".into());
    }
    let kinds = match args.get("kinds").filter(|k| !k.is_null()) {
        None => Vec::new(),
        Some(k) => {
            let names: Option<Vec<&str>> = k.as_array().and_then(|a| a.iter().map(Json::as_str).collect());
            let all = || Kind::ALL.map(Kind::name).join(", ");
            let names = names.ok_or_else(|| format!("'kinds' must be an array of: {}", all()))?;
            names
                .into_iter()
                .map(|n| Kind::parse(n).ok_or_else(|| format!("'kinds': unknown kind '{n}' (one of: {})", all())))
                .collect::<Result<_, _>>()?
        }
    };
    let max = u64::MAX >> 11;
    Ok(HistoryQuery {
        agent: string_arg(args, "agent")?.map(str::to_owned),
        path_prefix,
        entity: int_arg(args, "entity", 1, max)?,
        tick_from: int_arg(args, "tick_from", 0, max)?,
        tick_to: int_arg(args, "tick_to", 0, max)?,
        kinds,
        before: int_arg(args, "cursor", 1, max)?,
        limit: int_arg(args, "limit", 1, 200)?.unwrap_or(20) as usize,
        detail: bool_arg(args, "detail")?.unwrap_or(false),
    })
}

fn parse_undo(args: &Json) -> Result<UndoRequest, String> {
    check_keys(args, "world_undo", &[&["count", "ids", "force"]])?;
    let count = int_arg(args, "count", 1, journal::MAX_UNDO as u64)?;
    let ids = match args.get("ids").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => {
            let err = || format!("'ids' must be an array of 1..{} journal ids", journal::MAX_UNDO);
            let ids: Option<Vec<u64>> = v.as_array().and_then(|a| a.iter().map(Json::as_u64).collect());
            let ids = ids.filter(|i| (1..=journal::MAX_UNDO).contains(&i.len())).ok_or_else(err)?;
            Some(ids)
        }
    };
    if ids.is_some() && count.is_some() {
        return Err("give 'count' (your most recent changes) or 'ids' (specific journal entries), not both".into());
    }
    Ok(UndoRequest { ids, count: count.unwrap_or(1) as usize, force: bool_arg(args, "force")?.unwrap_or(false) })
}

fn parse_changes(args: &Json) -> Result<ChangesQuery, String> {
    check_keys(args, "world_changes", &[&["since", "limit", "path_prefix", "game"]])?;
    const ERR: &str = "'since' must be a cursor: 0 for the start, or 'next' from the previous call";
    let since = match args.get("since") {
        None | Some(Json::Null) => Cursor { seq: 1, index: 0 },
        Some(Json::Str(s)) => Cursor::parse(s).ok_or(ERR)?,
        Some(v) => v.as_u64().and_then(|n| Cursor::parse(&n.to_string())).ok_or(ERR)?,
    };
    let path_prefix = string_arg(args, "path_prefix")?.map(str::to_owned);
    if path_prefix.as_ref().is_some_and(|p| !p.starts_with('/')) {
        return Err("'path_prefix' must be a path like \"/slab\"".into());
    }
    Ok(ChangesQuery {
        since,
        limit: int_arg(args, "limit", 1, 1000)?.unwrap_or(200) as usize,
        path_prefix,
        game: bool_arg(args, "game")?.unwrap_or(true),
    })
}

fn lease_arg(args: &Json) -> Result<Duration, String> {
    Ok(int_arg(args, "lease_s", claims::MIN_LEASE_S, claims::MAX_LEASE_S)?
        .map_or(claims::DEFAULT_LEASE, Duration::from_secs))
}

/// `id` of one claim, or `all: true`.
fn claim_target(args: &Json, tool: &str) -> Result<Option<u64>, String> {
    let id = int_arg(args, "id", 1, u64::MAX >> 11)?;
    match (id, bool_arg(args, "all")?.unwrap_or(false)) {
        (Some(_), true) => Err(format!("{tool}: give 'id' or 'all', not both")),
        (None, false) => Err(format!("{tool} needs 'id' (a claim) or 'all': true (every claim you hold)")),
        (id, _) => Ok(id),
    }
}

fn parse_claim_take(args: &Json) -> Result<ClaimSpec, String> {
    check_keys(args, "claim_take", &[&["path", "region", "lease_s", "note"]])?;
    let scope = match (string_arg(args, "path")?, args.get("region").filter(|r| !r.is_null())) {
        (Some(_), Some(_)) => return Err("claim_take: give 'path' or 'region', not both".into()),
        (Some(p), None) => Scope::path(p)?,
        (None, Some(r)) => {
            check_keys(r, "region", &[&["min", "max"]])?;
            match (vec3_arg(r, "min")?, vec3_arg(r, "max")?) {
                (Some(lo), Some(hi)) => Scope::region(lo, hi)?,
                _ => return Err("'region' must be {min: [x,y,z], max: [x,y,z]} with min < max on every axis".into()),
            }
        }
        (None, None) => return Err("claim_take needs 'path' (a subtree, e.g. \"/district_3\") or 'region' ({min, max})".into()),
    };
    let note = string_arg(args, "note")?.unwrap_or("").to_owned();
    if note.chars().count() > 200 {
        return Err("'note' must be at most 200 characters".into());
    }
    Ok(ClaimSpec { scope, lease: lease_arg(args)?, note })
}

fn parse_time(args: &Json) -> Result<TimeOp, String> {
    let action = string_arg(args, "action")?.unwrap_or("info");
    let only = |allowed: &str| -> Result<(), String> {
        for key in ["ticks", "scale"] {
            if key != allowed && args.get(key).is_some_and(|v| *v != Json::Null) {
                return Err(format!("'{key}' does not apply to action '{action}'"));
            }
        }
        Ok(())
    };
    match action {
        "pause" => only("").map(|()| TimeOp::Pause),
        "resume" => only("").map(|()| TimeOp::Resume),
        "info" => only("").map(|()| TimeOp::Info),
        "step" => {
            only("ticks")?;
            let ticks = match args.get("ticks") {
                None | Some(Json::Null) => 1,
                Some(v) => v.as_u64().filter(|n| (1..=MAX_STEP_TICKS).contains(n)).ok_or_else(|| {
                    format!("'ticks' must be an integer 1..{MAX_STEP_TICKS}")
                })?,
            };
            Ok(TimeOp::Step(ticks))
        }
        "scale" => {
            only("scale")?;
            match args.get("scale").and_then(Json::as_f64) {
                Some(x) if (0.05..=64.0).contains(&x) => Ok(TimeOp::Scale(x as f32)),
                _ => Err("'scale' must be a number 0.05..64 (1 = real time)".into()),
            }
        }
        other => Err(format!("'action' must be pause, resume, step, scale or info, not '{other}'")),
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
        Some(_) => return Err(format!("'shape' must be a string: {SHAPE_LIST}")),
    };
    let shape = Shape::parse(shape_name).ok_or_else(|| format!("'shape' must be {SHAPE_LIST}, not '{shape_name}'"))?;
    let profile = profile_arg(args)?;
    if (shape == Shape::Lathe) != profile.is_some() {
        return Err("'profile' is required for shape 'lathe' and only allowed for it".into());
    }
    let recipe = string_arg(args, "recipe")?;
    let params = params_arg(args)?;
    if (shape == Shape::Model) != recipe.is_some() {
        return Err("'recipe' is required for shape 'model' and only allowed for it".into());
    }
    if params.is_some() && shape != Shape::Model {
        return Err("'params' is only for shape 'model'".into());
    }
    let two_d = two_d_arg(args)?;
    match (&two_d, shape.is_2d()) {
        (Some(t), true) if t.key() == shape.name() => {}
        (None, true) if shape == Shape::Sprite => {}
        (None, true) => return Err(format!("shape '{0}' needs '{0}' (see the tool description)", shape.name())),
        (Some(t), _) => return Err(format!("'{0}' is only for shape '{0}'", t.key())),
        (None, false) => {}
    }
    let mut spec = SpawnSpec::new(shape);
    spec.two_d = match two_d {
        Some(t) => Some(t),
        // A bare sprite: a 1 m flat colour quad.
        None if shape == Shape::Sprite => Some(crate::two_d::TwoD::Sprite(Box::default())),
        None => None,
    };
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
    if let Some(c) = color_arg(args, "color")? {
        spec.color = c;
    }
    let mut look = look_arg(args)?;
    if let Some(recipe) = recipe {
        // A model's `seed` is its model seed.
        let seed = match args.get("seed") {
            None | Some(Json::Null) => None,
            Some(v) => Some(v.as_u64().filter(|s| *s <= world::MAX_MODEL_SEED).ok_or_else(|| {
                format!("'seed' of a model must be an integer 0..{}", world::MAX_MODEL_SEED)
            })?),
        };
        look.seed = None;
        spec.model = Some(world::ModelSpec { recipe: recipe.to_owned(), params: params.unwrap_or_default(), seed });
    }
    look.apply(&mut spec.look);
    spec.visible = bool_arg(args, "visible")?.unwrap_or(true);
    spec.layer = layer_arg(args)?.unwrap_or_default();
    if bool_arg(args, "transient")? == Some(true) {
        return Err("'transient' is reserved for the hosted game's presentation (its in-process API): agents' \
                    entities are always saved and journaled"
            .into());
    }
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

/// `params`: an object of numbers (`{"tier": 2}`), sorted by name.
fn params_arg(args: &Json) -> Result<Option<Vec<(String, f64)>>, String> {
    const ERR: &str = "'params' must be an object of recipe parameters to numbers, e.g. {\"tier\": 2}";
    let Some(v) = args.get("params").filter(|v| !v.is_null()) else { return Ok(None) };
    let pairs = v.as_object().ok_or(ERR)?;
    let mut sorted = std::collections::BTreeMap::new();
    for (k, v) in pairs {
        let n = v.as_f64().filter(|n| n.is_finite()).ok_or(ERR)?;
        if sorted.insert(k.clone(), n).is_some() {
            return Err(format!("'params': '{k}' is given twice"));
        }
    }
    Ok(Some(sorted.into_iter().collect()))
}

/// `layer`: "world" or "ui".
fn layer_arg(args: &Json) -> Result<Option<world::Layer>, String> {
    match string_arg(args, "layer")? {
        None => Ok(None),
        Some(l) => world::Layer::parse(l).map(Some).ok_or_else(|| format!("'layer' must be \"world\" or \"ui\", not '{l}'")),
    }
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
        shadow: effect_arg(args, "shadow", crate::effects::ShadowPatch::parse)?,
        bloom: effect_arg(args, "bloom", crate::effects::BloomPatch::parse)?,
        fxaa: bool_arg(args, "fxaa")?,
        sky: effect_arg(args, "sky", crate::effects::SkyPatch::parse)?,
        fog: effect_arg(args, "fog", crate::effects::FogPatch::parse)?,
    };
    world::check_env(&patch).map_err(value_error)?;
    Ok(patch)
}

/// An optional effect object (`"bloom": {"enabled": true}`).
fn effect_arg<T>(args: &Json, key: &str, parse: fn(&Json) -> Result<T, String>) -> Result<Option<T>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => parse(v).map(Some),
    }
}

fn parse_view(args: &Json) -> Result<Option<Camera>, String> {
    if args.get("auto").and_then(Json::as_bool) == Some(true) {
        return Ok(None);
    }
    let frame = parse_frame(args)?;
    match (frame.eye, frame.target) {
        (Some(eye), Some(target)) => {
            let mut cam = Camera::new(eye, target, frame.fov_y_deg);
            if let Some(height_m) = frame.ortho {
                cam.projection = crate::sight::Projection::Ortho { height_m };
            }
            Ok(Some(cam))
        }
        _ => Err("window_view needs 'eye' and 'target' (or 'view_2d'), or 'auto': true".into()),
    }
}

/// At most one of `sprite`, `tilemap`, `shape2d`, parsed.
fn two_d_arg(args: &Json) -> Result<Option<crate::two_d::TwoD>, String> {
    let present: Vec<&str> = TWO_D_KEYS.into_iter().filter(|k| args.get(k).is_some_and(|v| !v.is_null())).collect();
    match present.as_slice() {
        [] => Ok(None),
        [k] => crate::two_d::parse(k, args.get(k).expect("present")).map(Some),
        _ => Err(format!("give one 2D component, not {}", present.join(" and "))),
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
        recipe: string_arg(args, "recipe")?.map(str::to_owned),
        params: params_arg(args)?,
        visible: bool_arg(args, "visible")?,
        layer: layer_arg(args)?,
        two_d: two_d_arg(args)?,
        tiles: None,
        expect_version: version_arg(args)?,
    };
    if change.is_empty() {
        return Err("world_set needs at least one change: rename, parent, position, rotation_deg, scale, color, a \
                    look field (material, opacity, emissive, metallic, roughness, specular, color2, seed), light, \
                    profile, tags, add_tags, remove_tags, props, collider, body, recipe, params, visible or layer"
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
        Some(s) => Some(Shape::parse(s).ok_or_else(|| format!("'shape' must be {SHAPE_LIST}, not '{s}'"))?),
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
    check_keys(args, "agent_hello", &[&["name", "role", "resume_token"]])?;
    let name = name_arg(args.get("name"))?.ok_or("'name' is required: how this agent appears in attribution")?;
    if name.chars().count() > 64 {
        return Err("'name' must be at most 64 characters".into());
    }
    let role = string_arg(args, "role")?.unwrap_or("builder").to_owned();
    world::validate_tag(&role).map_err(|_| "'role' must be a short word like builder, reviewer or playtester".to_string())?;
    let resume = match string_arg(args, "resume_token")? {
        Some(t) if t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()) => Some(t.to_ascii_lowercase()),
        Some(_) => return Err("'resume_token' must be the 32 hex characters an earlier agent_hello returned".into()),
        None => None,
    };
    Ok(Command::Hello { name, role, resume })
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
    let mut eye = position_arg(args, "eye")?;
    let mut target = position_arg(args, "target")?;
    let mut ortho = match args.get("ortho").filter(|v| !v.is_null()) {
        None => None,
        Some(o) => {
            check_keys(o, "'ortho'", &[&["height_m"]])?;
            Some(num_in(o, "height_m", 0.01..=1.0e5)?.ok_or("'ortho' needs 'height_m': metres of the view's height")?)
        }
    };
    if let Some(v) = args.get("view_2d").filter(|v| !v.is_null()) {
        check_keys(v, "'view_2d'", &[&["center", "height_m", "z"]])?;
        if eye.is_some() || target.is_some() || ortho.is_some() {
            return Err("'view_2d' sets the camera itself: give it without eye, target or ortho".into());
        }
        let c: Option<Vec<f64>> = v.get("center").and_then(Json::as_array).and_then(|a| a.iter().map(Json::as_f64).collect());
        let c = match c.as_deref() {
            Some(&[x, y]) if x.is_finite() && y.is_finite() && x.abs() <= 1e6 && y.abs() <= 1e6 => [x as f32, y as f32],
            _ => return Err("'view_2d.center' must be [x, y] (metres)".into()),
        };
        let height_m = num_in(v, "height_m", 0.01..=1.0e5)?.ok_or("'view_2d' needs 'height_m': metres of the view's height")?;
        let z = num_in(v, "z", -1.0e6..=1.0e6)?.unwrap_or(0.0);
        let cam = Camera::view_2d(c[0], c[1], height_m);
        eye = Some(Vec3::new(cam.eye.x, cam.eye.y, z + 100.0));
        target = Some(Vec3::new(cam.target.x, cam.target.y, z));
        ortho = Some(height_m);
    }
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
        ortho,
        max_subjects: int("max_subjects", 50, 1..=500)? as usize,
        time_s: num_in(args, "time_s", 0.0..=1.0e6)?,
        include_ui: bool_arg(args, "include_ui")?.unwrap_or(false),
        lod: int_arg(args, "lod", 0, 7)?.map(|l| l as usize),
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
            ("seed", number("Variation seed: a model's variant seed (integer 0..1000000), or the lava_lamp's wax motion")),
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
    let two_d = |props: &mut Vec<(&str, Json)>| {
        props.extend([
            (
                "sprite",
                described(
                    "sprite only: {texture (.png under the assets root; none = a flat colour quad), size [w,h] m \
                     (default [1,1]), pivot [x,y] 0..1 ((0,0) bottom left, default centre), uv [u0,v0,u1,v1] or \
                     sheet {frame_px [w,h], margin, spacing} + frame (index) or anim {first, count, fps, playback \
                     loop|once|pingpong}, flip [x,y], billboard none|spherical|cylindrical, blend blended|cutout \
                     (+ cutoff 0..1; cutout writes depth), filter linear|nearest (pixel art), pixel_snap \
                     (orthographic views), lit, cast_shadows (cutout only), z_layer (higher draws over lower)}. The \
                     quad lies in the entity's local XY plane facing +Z; color + opacity tint it",
                ),
            ),
            (
                "tilemap",
                described(
                    "tilemap only: {tileset (.png), tile_px [w,h] (tile size in the tileset; tiles numbered from 1, \
                     left to right then top to bottom; 0 = empty), margin, spacing, tile_size [w,h] m (default \
                     [1,1]), size [columns, rows] (at most 1024 each), filter nearest|linear (default nearest), \
                     pixel_snap, lit, animations [{tile, frames, frame_ms}], layers [{name, z_layer, blend \
                     cutout|blended, cutoff, opacity, rows: [\"1*10,0*3,7h\", ...]}] (up to 8)}. Row 0 is the \
                     top row; the entity's origin is the map's top-left corner and rows go down local -Y. A cell \
                     token is a tile number plus h (mirror x), v (mirror y), d (swap axes) or r1..r3 (quarter \
                     turns clockwise); N*count repeats. Edit cells with tilemap_set; listings show a summary",
                ),
            ),
            (
                "shape2d",
                described(
                    "shape2d only: {items: [{one of rect [x,y,w,h] (+ radius), circle [cx,cy,r], ellipse \
                     [cx,cy,rx,ry], polygon [[x,y],...], polyline [[x,y],...], path [[\"M\",x,y],[\"L\",x,y], \
                     [\"Q\",cx,cy,x,y],[\"C\",c1x,c1y,c2x,c2y,x,y],[\"A\",cx,cy,rx,ry,start_deg,sweep_deg],[\"Z\"]]; \
                     fill \"#rrggbb(aa)\" and/or stroke {width m, color, join miter|round|bevel, cap \
                     butt|square|round, miter_limit}}], tolerance (m, default 0.005), aa (default true: one-pixel \
                     soft edges), z_layer}. Coordinates are metres in the entity's local XY plane; contours \
                     inside others are holes (even-odd)",
                ),
            ),
        ]);
    };
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
    // Model, visibility and layer fields shared by world_spawn and world_set.
    let presentation = |props: &mut Vec<(&str, Json)>| {
        props.extend([
            (
                "recipe",
                string(
                    "model only: recipe file relative to the engine's assets root (--assets), e.g. \
                     \"recipes/warding_stone.json\"; built with the style.json found next to it (or --style). \
                     A broken recipe is refused with its error path and code (model_build to iterate)",
                ),
            ),
            ("params", object("model only: recipe parameters, e.g. {\"tier\": 2} (unset ones take the recipe's defaults)")),
            (
                "visible",
                boolean("false hides it and everything below it: not drawn, not a sight subject, not in auto-framing (default true)"),
            ),
            (
                "layer",
                string(
                    "\"world\" (default) or \"ui\": game interface in the scene (cursors, range rings) - drawn in the \
                     window but left out of sight_frame captures unless include_ui; children inherit it",
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
            description: "Add an entity to the live world: a shape (or a procedural model built from a recipe file) \
                          with an optional parent, material, point light, tags, props and reserved physics \
                          components. Returns the new entity (id, path, transform, look, version). A model may \
                          still be building when this returns (model_state \"building\"); sight_frame waits for \
                          it. Entities the hosted game marks transient (its presentation) can't be spawned under, \
                          changed or deleted by agents."
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
                                         lathe spins 'profile' around +Y (vases, bottles, lamp bases); model builds \
                                         'recipe' (with 'params' and 'seed') into a mesh with LODs and its style's \
                                         materials, resting on y = 0; its color tints it (default white = as authored); \
                                         sprite, tilemap and shape2d are 2D in the entity's local XY plane (metres, +Y \
                                         up, facing +Z), described by the field of the same name; for a 2D game look \
                                         at the XY plane with sight_frame view_2d",
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
                        (
                            "transient",
                            boolean(
                                "Reserved for the hosted game: its presentation (moving enemies, effects) is \
                                 transient - not saved, journaled or in world_hash, drawn and queryable like \
                                 anything. Agents can't create, change or delete transient entities (error \
                                 transient_entity); true is refused here",
                            ),
                        ),
                    ];
                    look(&mut props);
                    presentation(&mut props);
                    components(&mut props);
                    two_d(&mut props);
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
                          the look fields, light, a lathe's profile/segments, a model's recipe / params (replaces all) \
                          / seed, visible, layer, tags / add_tags / remove_tags, props (merged key by key; a null \
                          value removes a key), collider, body."
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
                    presentation(&mut props);
                    components(&mut props);
                    two_d(&mut props);
                    props
                },
                &[],
            ),
        },
        ToolDef {
            name: "tilemap_set".into(),
            description: "Write cells of one layer of a tilemap entity, all at once (one journaled, claim-checked \
                          change; only the touched chunks are re-uploaded). cells: [[x, y, tile], ...] with x the \
                          column and y the row (0 = top), tile a number (0 clears) or a token like \"7h\" or \"3r1\" \
                          (see world_spawn tilemap). Up to 65536 cells per call. Returns the new version."
                .into(),
            input_schema: schema(
                {
                    let mut props = Vec::new();
                    target(&mut props);
                    props.extend([
                        ("layer", string("Layer name")),
                        ("cells", described("[[x, y, tile], ...]: tile = number or cell token")),
                    ]);
                    props
                },
                &["layer", "cells"],
            ),
        },
        ToolDef {
            name: "tilemap_get".into(),
            description: "Read cells of a tilemap entity as run-length rows (the same tokens tilemap_set and world \
                          files use): one layer, a region of at most 4096 cells. Without layer, lists the layers."
                .into(),
            input_schema: schema(
                {
                    let mut props = Vec::new();
                    target(&mut props);
                    props.extend([
                        ("layer", string("Layer name")),
                        ("region", described("[x, y, width, height] in cells (default: from (0, 0), up to 64 x 64)")),
                    ]);
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
                          world. Refuses while there are unsaved changes unless force is true, and while another \
                          agent holds a live claim (error claimed, listing them; force doesn't override claims)."
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
                          change, owns your claims and scopes your undo, and engine_info lists you with your role. \
                          Names are unique among open sessions, idle ones included (error name_taken with a \
                          suggestion), and the hosted game's name is reserved (error name_reserved). Default: \
                          '<MCP client name>-<6 characters of your session id>', role builder. A session holding \
                          claims can't change its name. The reply has a resume_token: a later session (your next \
                          script run) sends agent_hello {name, resume_token} to continue as you, with your claims \
                          and undo history; claims then last until their lease ends, not until a session closes."
                .into(),
            input_schema: schema(
                vec![
                    ("name", string("Your agent name, up to 64 characters")),
                    ("role", string(&format!("builder (default), reviewer, playtester, director, ...; {tag_rule}"))),
                    ("resume_token", string("The resume_token an earlier agent_hello returned: continue as that agent")),
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
                          subjects first (max_subjects); the full list is saved at packet_path. If models in view are \
                          still building, it waits for them (up to 10 s, other calls go on meanwhile) and then \
                          renders the world as it is at that moment; packet 'models' and 'warnings' say what wasn't \
                          drawn."
                .into(),
            input_schema: schema(
                vec![
                    ("width", integer("Image width 64..2048 (default 960)")),
                    ("height", integer("Image height 64..2048 (default 540)")),
                    ("eye", vec3("Camera position (give with target)")),
                    ("target", vec3("Point the camera looks at (give with eye; must differ from eye)")),
                    ("fov_y_deg", Json::obj([("type", Json::from("number")), ("description", Json::from("Vertical field of view, 10..120 (default 50)"))])),
                    (
                        "ortho",
                        described(
                            "{height_m}: an orthographic camera (parallel rays) showing height_m metres top to bottom; \
                             with eye/target, or alone to frame the world's XY plane from +Z",
                        ),
                    ),
                    (
                        "view_2d",
                        described(
                            "{center [x, y], height_m, z (default 0)}: the 2D camera - orthographic, looking down -Z \
                             at the XY plane, +Y up on screen; instead of eye/target/ortho",
                        ),
                    ),
                    ("max_subjects", integer("Subjects to include in the response, 1..500 (default 50)")),
                    (
                        "time_s",
                        number("Animation time to render in seconds (default: now). Fix it to get reproducible frames."),
                    ),
                    (
                        "include_ui",
                        boolean("Also draw and list layer \"ui\" entities (the game's cursor, range rings); default false"),
                    ),
                    (
                        "lod",
                        integer("Draw every model at this LOD (0 = full detail; clamped to each model's last) instead of by on-screen size"),
                    ),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "model_build".into(),
            description: "Build a model recipe (read fresh from disk) with params and seed, without spawning it, and \
                          report: triangles and vertices per LOD, bounds, materials, budget, collider, notes, build \
                          time and whether the build cache already had it. Recipe errors come back structured (code, \
                          path into the recipe, message). Iterate on a recipe file with this, then world_spawn it as \
                          shape \"model\"."
                .into(),
            input_schema: schema(
                vec![
                    ("recipe", string("Recipe file relative to the assets root, e.g. \"recipes/warding_stone.json\"")),
                    ("params", object("Recipe parameters, e.g. {\"tier\": 3}")),
                    ("seed", integer("Variant seed 0..1000000 (default: the recipe's)")),
                ],
                &["recipe"],
            ),
        },
        ToolDef {
            name: "model_reload".into(),
            description: "Re-read every recipe and style file now and rebuild the models that changed (the engine \
                          also notices edits on its own within a second). Returns what was rebuilt and any errors; \
                          a model whose recipe broke keeps its last good build."
                .into(),
            input_schema: schema(vec![], &[]),
        },
        ToolDef {
            name: "world_env".into(),
            description: "Scene lighting: give any field to change it; returns the whole environment. Defaults: \
                          white sun 0.75 from [0.4,0.8,0.45], white ambient 0.25, exposure 1, no tone mapping, \
                          background #1c1d21. For night scenes lit by lamps: sun_intensity 0, ambient ~0.02, \
                          tonemap true. Effects (all off by default; give only the settings to change): shadow \
                          {enabled, resolution, cascades, distance, softness, bias} from the sun; bloom {enabled, \
                          threshold, knee, intensity, levels} glow on bright/emissive light; fxaa (edge smoothing); \
                          sky {enabled, zenith, horizon, ground, intensity, horizon_sharpness, disc_size_deg, \
                          disc_intensity, glow} gradient with a sun/moon disc; fog {enabled, color, density, falloff, \
                          height, start_distance, sun_scatter} height fog. Each costs GPU time: check perf_frame."
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
                    ("shadow", object("Sun/moon shadows: {enabled, resolution 256..8192, cascades 1..4, distance m, softness 0..8, bias 0..10}")),
                    ("bloom", object("Glow: {enabled, threshold 0..100, knee 0..10, intensity 0..10, levels 1..8}")),
                    ("fxaa", Json::obj([("type", Json::from("boolean")), ("description", Json::from("Anti-aliasing post pass"))])),
                    ("sky", object("Analytic sky: {enabled, zenith, horizon, ground (colours), intensity, horizon_sharpness, disc_size_deg, disc_intensity, glow}")),
                    ("fog", object("Height fog: {enabled, color, density, falloff, height, start_distance, sun_scatter}")),
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
                    ("ortho", described("{height_m}: orthographic, with eye and target")),
                    ("view_2d", described("{center [x, y], height_m, z}: the 2D camera (orthographic, down -Z, +Y up)")),
                    ("auto", Json::obj([("type", Json::from("boolean")), ("description", Json::from("true: auto-frame the world"))])),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "perf_frame".into(),
            description: "Performance of the last frames (foundation F3): GPU ms per pass, CPU record/submit, draw \
                          calls, instances, triangles, culled draws, pipelines built after warm-up, GPU memory and \
                          heap budgets, frame pacing (mean/p95/max), simulation tick cost, CPU frame time and heap, \
                          and `errors` listing every budget miss as {error, metric, value, budget, unit, adapter}. \
                          Budgets come from perf.toml (per adapter kind). The window and the last capture are \
                          reported separately."
                .into(),
            input_schema: schema(
                vec![("budget_ms", number("Check frame, GPU, CPU and pacing against this frame time instead of perf.toml's"))],
                &[],
            ),
        },
        ToolDef {
            name: "perf_zones".into(),
            description: "CPU profiler zones (named, nested spans around the engine loop's phases and worker threads): \
                          per zone path its latest-frame calls, ms, self ms and allocations, plus rolling mean/p95 over \
                          the history. Paged; sorted engine thread first, then by path."
                .into(),
            input_schema: schema(
                vec![
                    ("thread", string("Only this thread (\"main\" = the engine thread)")),
                    ("min_ms", number("Hide zones whose rolling p95 is below this many ms")),
                    ("limit", integer("Zones per page, 1..500 (default 60)")),
                    ("cursor", string("next_cursor from the previous page")),
                    ("enabled", boolean("Turn zone recording on or off first")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "perf_memory".into(),
            description: "Heap use from the counting allocator: current and peak MB, allocations per frame \
                          (mean/p95/max), the zones that allocate most per frame, and GPU memory."
                .into(),
            input_schema: schema(vec![("limit", integer("Top zones to list, 1..50 (default 10)"))], &[]),
        },
        ToolDef {
            name: "perf_capture".into(),
            description: "Frame capture: records the next `frames` frames (CPU zones of every thread, GPU passes, \
                          draws, memory), writes them to a JSON file in the captures folder plus a Chrome trace-event \
                          file (open in chrome://tracing or ui.perfetto.dev), and answers with a summary: frame and \
                          GPU ms, top zones by self time, allocations per frame and budget errors. Answers when done \
                          (or at about 23 s with what it has)."
                .into(),
            input_schema: schema(
                vec![
                    ("frames", integer(&format!("Frames to record, 1..{MAX_CAPTURE_FRAMES} (default 60)"))),
                    ("trace", boolean("Also write the trace-event timeline (default true)")),
                    ("label", string("Added to the file names: 1..32 of A-Z a-z 0-9 _ -")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "time_control".into(),
            description: "The simulation clock (fixed ticks): action pause, resume, step (run `ticks` then stay \
                          paused; default 1), scale (`scale` 0.05..64 times real time) or info. Returns the clock."
                .into(),
            input_schema: schema(
                vec![
                    (
                        "action",
                        Json::obj([
                            ("type", Json::from("string")),
                            ("enum", Json::arr(["pause", "resume", "step", "scale", "info"].map(Json::from))),
                        ]),
                    ),
                    ("ticks", integer("Ticks to run for action 'step' (1..18000)")),
                    ("scale", number("Speed for action 'scale' (1 = real time)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_history".into(),
            description: "The journal: who changed what, when (simulation tick), newest first, one page at a time. \
                          Every given filter must match. Each entry: id, tick, by, kind, changes (count), the first \
                          paths (or with detail: the changes), undone_by / undoes. Returns entries and next_cursor."
                .into(),
            input_schema: schema(
                vec![
                    ("agent", string("Only entries by this agent (or made by the game on its behalf)")),
                    ("path_prefix", string("Only entries that changed this entity or something below it, e.g. \"/slab\"")),
                    ("entity", integer("Only entries that changed this entity id")),
                    ("tick_from", integer("Only entries at or after this tick")),
                    ("tick_to", integer("Only entries at or before this tick")),
                    (
                        "kinds",
                        string_array(&format!("Only these kinds: {}", Kind::ALL.map(Kind::name).join(", "))),
                    ),
                    ("cursor", integer("next_cursor from the previous page: entries older than this id")),
                    ("limit", integer("Entries per page, 1..200 (default 20)")),
                    ("detail", boolean("List each entry's changes (up to 50 each: op, id, path, version)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_undo".into(),
            description: "Undo journal entries: by default your own most recent change in this session (count for \
                          more), or specific entries by id. A batch undoes as a unit; several entries undo newest first, all or nothing. \
                          Refused (error undo_conflict, naming who and which entry) when another agent changed the same \
                          entities later, unless force. The undo is itself journaled: undo its id to redo. Game \
                          updates are replay-only and can't be undone."
                .into(),
            input_schema: schema(
                vec![
                    ("count", integer("How many of your most recent changes to undo, 1..100 (default 1)")),
                    ("ids", Json::obj([
                        ("type", Json::from("array")),
                        ("items", Json::obj([("type", Json::from("integer"))])),
                        ("description", Json::from("Specific journal entry ids (from world_history); others' entries need force")),
                    ])),
                    ("force", boolean("Undo even over other agents' later changes, or their entries (default false)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_changes".into(),
            description: "Change feed: every change after a cursor, oldest first, one item per change \
                          {seq, tick, by, op, id, path, version, from (old path when moved)}. Pass 'next' back as \
                          'since' to poll; more: true means call again now. gap: true means changes were dropped \
                          (journal limit or a world_load): re-read what you need with world_query."
                .into(),
            input_schema: schema(
                vec![
                    ("since", described("Cursor: 0 = from the start (default), or 'next' from the previous call")),
                    ("limit", integer("Changes per call, 1..1000 (default 200)")),
                    ("path_prefix", string("Only changes at or below this path")),
                    ("game", boolean("Include the hosted game's changes (default true)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_hash".into(),
            description: "The world's state hash (everything that defines it, by exact bits; versions and authors \
                          left out) and the simulation tick, plus the hashes the engine recorded at recent ticks \
                          ([tick, hash] pairs, recorded as each tick begins). verify: replay the journal from its base \
                          and confirm it reproduces the current hash."
                .into(),
            input_schema: schema(
                vec![
                    ("recent", integer(&format!("Recent [tick, hash] pairs to return, 0..{MAX_HASH_RECENT} (default 8)"))),
                    ("tick", integer("Also look up the hash recorded at this tick (kept for the last 1024 ticks)")),
                    ("verify", boolean("Replay the journal and compare (costs time proportional to the journal)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "claim_take".into(),
            description: "Claim part of the world so other agents can't write there while you work: a subtree path \
                          (it and everything below; \"/\" is the whole world and the environment) or a world-space \
                          region (entities whose origin is inside, min <= p < max). Writes by others inside it are \
                          rejected with error 'claimed' (owner, claim, expiry) unless you claim_share it. Claims of \
                          different agents can't overlap. The lease expires unless renewed, and the claim ends when \
                          your session does. Needs agent_hello first. Claims are not saved with the world."
                .into(),
            input_schema: schema(
                vec![
                    ("path", string("Subtree to claim, e.g. \"/district_3\" (it need not exist yet)")),
                    ("region", object("World box {min: [x,y,z], max: [x,y,z]}, min < max on every axis")),
                    ("lease_s", integer("Seconds until it expires unless renewed, 5..3600 (default 300)")),
                    ("note", string("What you're doing there (shown to others), up to 200 characters")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "claim_renew".into(),
            description: "Extend your claim's lease (or all your claims') to lease_s from now.".into(),
            input_schema: schema(
                vec![
                    ("id", integer("Claim id")),
                    ("all", boolean("Renew every claim you hold")),
                    ("lease_s", integer("Seconds from now, 5..3600 (default 300)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "claim_release".into(),
            description: "Release your claim (or all your claims) so others may write there.".into(),
            input_schema: schema(
                vec![("id", integer("Claim id")), ("all", boolean("Release every claim you hold"))],
                &[],
            ),
        },
        ToolDef {
            name: "claim_share".into(),
            description: "Let another agent write inside your claim too (co-claim), or stop letting it (revoke).".into(),
            input_schema: schema(
                vec![
                    ("id", integer("Your claim's id")),
                    ("with", string("The other agent's name (its agent_hello name)")),
                    ("revoke", boolean("Remove that agent instead (default false)")),
                ],
                &["id", "with"],
            ),
        },
        ToolDef {
            name: "claim_list".into(),
            description: "Live claims in id order, one page at a time: id, owner, path or region, expires_in_s, \
                          shared, note. Returns total, claims and next_cursor."
                .into(),
            input_schema: schema(
                vec![
                    ("owner", string("Only this agent's claims")),
                    ("mine", boolean("Only your own claims")),
                    ("path_prefix", string("Only path claims at, above or below this path")),
                    ("cursor", integer("next_cursor from the previous page")),
                    ("limit", integer("Claims per page, 1..200 (default 50)")),
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
        assert_eq!(parse_spawn(&args(r#"{"shape":1}"#)).err().unwrap(), "'shape' must be a string: group, cube, quad, sphere, lathe, model, sprite, tilemap or shape2d");
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
            ("perf_frame", r#"{"budget":16}"#),
            ("perf_frame", r#"{"budget_ms":0}"#),
            ("time_control", r#"{"action":"rewind"}"#),
            ("time_control", r#"{"action":"pause","ticks":3}"#),
            ("time_control", r#"{"action":"step","ticks":0}"#),
            ("time_control", r#"{"action":"scale","scale":100}"#),
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
    fn time_and_perf_arguments() {
        let t = |a: &str| parse("time_control", &args(a));
        assert!(matches!(t("{}"), Ok(Command::Time(TimeOp::Info))));
        assert!(matches!(t(r#"{"action":"pause"}"#), Ok(Command::Time(TimeOp::Pause))));
        assert!(matches!(t(r#"{"action":"step"}"#), Ok(Command::Time(TimeOp::Step(1)))));
        assert!(matches!(t(r#"{"action":"step","ticks":300}"#), Ok(Command::Time(TimeOp::Step(300)))));
        assert!(matches!(t(r#"{"action":"scale","scale":4}"#), Ok(Command::Time(TimeOp::Scale(x))) if x == 4.0));
        assert!(matches!(parse("perf_frame", &args("{}")), Ok(Command::Perf { budget_ms: None })));
        assert!(matches!(parse("perf_frame", &args(r#"{"budget_ms":33.3}"#)), Ok(Command::Perf { budget_ms: Some(_) })));
    }

    #[test]
    fn profiling_tool_arguments() {
        let Ok(Command::PerfZones(q)) = parse("perf_zones", &args("{}")) else { panic!() };
        assert_eq!(q, ZonesQuery { thread: None, min_ms: 0.0, limit: 60, offset: 0, enabled: None });
        let Ok(Command::PerfZones(q)) =
            parse("perf_zones", &args(r#"{"thread":"main","min_ms":0.5,"limit":5,"cursor":"10","enabled":false}"#))
        else {
            panic!()
        };
        assert_eq!(q, ZonesQuery { thread: Some("main".into()), min_ms: 0.5, limit: 5, offset: 10, enabled: Some(false) });
        assert!(matches!(parse("perf_memory", &args("{}")), Ok(Command::PerfMemory { limit: 10 })));
        assert!(matches!(parse("perf_memory", &args(r#"{"limit":3}"#)), Ok(Command::PerfMemory { limit: 3 })));
        let Ok(Command::PerfCapture(c)) = parse("perf_capture", &args("{}")) else { panic!() };
        assert_eq!(c, CaptureSpec { frames: 60, trace: true, label: None });
        let Ok(Command::PerfCapture(c)) = parse("perf_capture", &args(r#"{"frames":600,"trace":false,"label":"wave-6"}"#)) else {
            panic!()
        };
        assert_eq!(c, CaptureSpec { frames: 600, trace: false, label: Some("wave-6".into()) });
        for (tool, bad) in [
            ("perf_zones", r#"{"limit":0}"#),
            ("perf_zones", r#"{"limit":501}"#),
            ("perf_zones", r#"{"min_ms":-1}"#),
            ("perf_zones", r#"{"cursor":"abc"}"#),
            ("perf_zones", r#"{"enabled":"yes"}"#),
            ("perf_zones", r#"{"threads":"main"}"#),
            ("perf_memory", r#"{"limit":51}"#),
            ("perf_capture", r#"{"frames":0}"#),
            ("perf_capture", r#"{"frames":601}"#),
            ("perf_capture", r#"{"label":"../x"}"#),
            ("perf_capture", r#"{"label":""}"#),
            ("perf_capture", r#"{"frame":10}"#),
        ] {
            assert!(parse(tool, &args(bad)).is_err(), "{tool} {bad}");
        }
        let e = parse("perf_capture", &args(r#"{"frame":10}"#)).err().unwrap();
        assert!(e.contains("did you mean 'frames'"), "{e}");
    }

    #[test]
    fn game_tools_cannot_shadow_engine_tools() {
        let (tx, _rx) = mpsc::channel();
        let def = |name: &str| ToolDef { name: name.into(), description: String::new(), input_schema: Json::obj::<&str>([]) };
        let tools = Tools::new(tx, vec![def("play_state"), def("world_spawn")]);
        let names: Vec<String> = tools.tools().into_iter().map(|t| t.name).collect();
        assert_eq!(names.iter().filter(|n| *n == "world_spawn").count(), 1);
        assert!(names.contains(&"play_state".to_string()));
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
    fn journal_and_claim_arguments() {
        let p = |tool: &str, a: &str| parse(tool, &args(a));
        match p("world_history", r#"{"agent":"ann","kinds":["set","batch"],"cursor":9,"limit":5}"#) {
            Ok(Command::History(q)) => {
                assert_eq!((q.agent.as_deref(), q.kinds, q.before, q.limit), (Some("ann"), vec![Kind::Set, Kind::Batch], Some(9), 5))
            }
            _ => panic!("history"),
        }
        assert!(matches!(p("world_undo", "{}"), Ok(Command::Undo(UndoRequest { ids: None, count: 1, force: false }))));
        assert!(matches!(p("world_undo", r#"{"ids":[3,4],"force":true}"#), Ok(Command::Undo(UndoRequest { ids: Some(_), force: true, .. }))));
        assert!(matches!(p("world_changes", r#"{"since":"12:3"}"#), Ok(Command::Changes(ChangesQuery { since: Cursor { seq: 12, index: 3 }, .. }))));
        assert!(matches!(p("world_changes", r#"{"since":0}"#), Ok(Command::Changes(ChangesQuery { since: Cursor { seq: 1, index: 0 }, .. }))));
        assert!(matches!(p("world_hash", "{}"), Ok(Command::Hash { recent: 8, tick: None, verify: false })));
        match p("claim_take", r#"{"region":{"min":[0,0,0],"max":[1,2,3]},"lease_s":60,"note":"walls"}"#) {
            Ok(Command::ClaimTake(c)) => assert_eq!((c.lease, c.note.as_str()), (Duration::from_secs(60), "walls")),
            _ => panic!("claim_take"),
        }
        assert!(matches!(p("claim_take", r#"{"path":"/d1"}"#), Ok(Command::ClaimTake(ClaimSpec { scope: Scope::Path(_), .. }))));
        assert!(matches!(p("claim_renew", r#"{"all":true}"#), Ok(Command::ClaimRenew { id: None, .. })));
        assert!(matches!(p("claim_release", r#"{"id":4}"#), Ok(Command::ClaimRelease { id: Some(4) })));
        assert!(matches!(p("claim_share", r#"{"id":4,"with":"bob"}"#), Ok(Command::ClaimShare { revoke: false, .. })));
        assert!(matches!(p("claim_list", r#"{"mine":true}"#), Ok(Command::ClaimList { mine: true, limit: 50, .. })));
        for (tool, bad) in [
            ("world_history", r#"{"kinds":["move"]}"#),
            ("world_history", r#"{"limit":0}"#),
            ("world_history", r#"{"path_prefix":"x"}"#),
            ("world_undo", r#"{"count":0}"#),
            ("world_undo", r#"{"count":2,"ids":[1]}"#),
            ("world_undo", r#"{"ids":[]}"#),
            ("world_changes", r#"{"since":"abc"}"#),
            ("world_changes", r#"{"limit":5000}"#),
            ("world_hash", r#"{"recent":100000}"#),
            ("claim_take", "{}"),
            ("claim_take", r#"{"path":"d1"}"#),
            ("claim_take", r#"{"path":"/a","region":{"min":[0,0,0],"max":[1,1,1]}}"#),
            ("claim_take", r#"{"region":{"min":[0,0,0],"max":[0,1,1]}}"#),
            ("claim_take", r#"{"path":"/a","lease_s":1}"#),
            ("claim_renew", "{}"),
            ("claim_release", r#"{"id":1,"all":true}"#),
            ("claim_share", r#"{"id":1}"#),
        ] {
            assert!(parse(tool, &args(bad)).is_err(), "{tool} {bad}");
        }
    }

    #[test]
    fn model_and_presentation_arguments() {
        let s = parse_spawn(&args(
            r#"{"shape":"model","recipe":"recipes/warding_stone.json","params":{"tier":2,"glow":0.5},"seed":7,
                "visible":false,"layer":"ui","transient":false}"#,
        ))
        .unwrap();
        let m = s.model.as_ref().unwrap();
        assert_eq!((m.recipe.as_str(), m.seed), ("recipes/warding_stone.json", Some(7)));
        assert_eq!(m.params, vec![("glow".to_string(), 0.5), ("tier".to_string(), 2.0)], "sorted by name");
        assert_eq!((s.look.surface.seed, s.color), (0.0, [1.0; 3]), "a model's seed is its model seed; white by default");
        assert_eq!((s.visible, s.layer, s.transient), (false, world::Layer::Ui, false));
        let e = parse_spawn(&args(r#"{"shape":"cube","transient":true}"#)).err().unwrap();
        assert!(e.contains("reserved for the hosted game"), "{e}");
        let s = parse_spawn(&args(r#"{"shape":"cube","seed":2.5,"layer":"world"}"#)).unwrap();
        assert_eq!(s.look.surface.seed, 2.5, "other shapes keep the surface seed");
        for bad in [
            r#"{"shape":"model"}"#,
            r#"{"shape":"cube","recipe":"a.json"}"#,
            r#"{"shape":"cube","params":{"tier":1}}"#,
            r#"{"shape":"model","recipe":"../a.json"}"#,
            r#"{"shape":"model","recipe":"a.json","seed":1.5}"#,
            r#"{"shape":"model","recipe":"a.json","seed":2000000}"#,
            r#"{"shape":"model","recipe":"a.json","params":[1]}"#,
            r#"{"shape":"model","recipe":"a.json","params":{"tier":"two"}}"#,
            r#"{"shape":"model","recipe":"a.json","params":{"bad name":1}}"#,
            r#"{"shape":"cube","layer":"hud"}"#,
            r#"{"shape":"cube","visible":"no"}"#,
        ] {
            assert!(parse_spawn(&args(bad)).is_err(), "{bad} should be rejected");
        }
        let (_, c) = parse_set(&args(r#"{"id":1,"params":{"tier":3},"recipe":"r/b.json","visible":true,"layer":"ui"}"#)).unwrap();
        assert_eq!((c.params, c.recipe.as_deref(), c.visible, c.layer), (Some(vec![("tier".into(), 3.0)]), Some("r/b.json"), Some(true), Some(world::Layer::Ui)));
        assert!(parse_set(&args(r#"{"id":1,"visible":false}"#)).is_ok(), "visibility alone is a change");
        let f = parse_frame(&args(r#"{"include_ui":true,"lod":2}"#)).unwrap();
        assert_eq!((f.include_ui, f.lod), (true, Some(2)));
        assert!(!parse_frame(&args("{}")).unwrap().include_ui);
        assert!(parse("sight_frame", &args(r#"{"lod":9}"#)).is_err());
        match parse("model_build", &args(r#"{"recipe":"recipes/tombstone.json","params":{"tier":1},"seed":3}"#)) {
            Ok(Command::ModelBuild(spec)) => assert_eq!((spec.seed, spec.params.len()), (Some(3), 1)),
            _ => panic!("model_build"),
        }
        assert!(parse("model_build", &args("{}")).is_err());
        assert!(parse("model_build", &args(r#"{"recipe":"/abs.json"}"#)).is_err());
        assert!(matches!(parse("model_reload", &args("{}")), Ok(Command::ModelReload)));
    }

    #[test]
    fn tool_names_are_claude_code_safe() {
        for t in definitions() {
            assert!(t.name.len() <= 64 && t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        }
    }
}
