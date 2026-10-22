//! Arguments and definitions of the swarm communication tools: the task board (`task_*`),
//! intents (`intent_*`), notes (`note_*`), roll-ups (`world_rollup`), resource leases
//! (`lease_*`), the swarm settings (`swarm_settings`) and the desync detector (`desync_*`).
//! The engine side is in `swarm_engine`.

use crate::board::{check_text, NoteQuery, Status, TaskPatch, TaskQuery, TaskSpec, MAX_DETAIL, MAX_LINKS, MAX_TEXT, MAX_TITLE};
use crate::claims::Scope;
use crate::rollup::{By, RollupQuery, MAX_GROUPS};
use crate::schedule::{Class, MAX_FRAME_FLOOR_FPS, MIN_FRAME_FLOOR_FPS};
use crate::tools::{bool_arg, check_keys, int_arg, string_arg, vec3_arg};
use crate::world::EntityKey;
use specula_core::Json;
use specula_protocol::ToolDef;
use std::time::Duration;

/// Largest id accepted (JSON numbers are exact up to 2^53).
const MAX_ID: u64 = u64::MAX >> 11;

pub enum SwarmCall {
    TaskPost(TaskSpec),
    TaskTake { id: u64, expect: Option<u64> },
    TaskUpdate { id: u64, patch: TaskPatch },
    TaskDone { id: u64, summary: String, cancel: bool, expect: Option<u64> },
    /// `mine`: the caller's own (as owner); `ready_for_me`: open tasks the caller may take now.
    /// `events_since`: also the board events after this board_seq (the feed).
    TaskList { query: TaskQuery, mine: bool, ready_for_me: bool, events_since: Option<u64>, ack_board_warning: bool },
    TaskGet { id: u64, events: usize },
    IntentDeclare { scopes: Vec<Scope>, text: String, task: Option<u64>, lease: Duration, replace: Option<u64> },
    IntentList { owner: Option<String>, mine: bool, path_prefix: Option<String>, after: Option<u64>, limit: usize },
    /// `None`: all the caller's.
    IntentClear { id: Option<u64> },
    NoteAdd { target: NoteOn, text: String },
    NoteList(NoteQuery),
    NoteDelete { id: u64, force: bool },
    Rollup(RollupQuery),
    LeaseTake { want: crate::leases::Want, note: String, lease: Duration },
    LeaseRenew { id: Option<u64>, lease: Duration },
    LeaseRelease { id: Option<u64>, delete: bool },
    LeaseList { kind: Option<String>, owner: Option<String>, mine: bool, after: Option<u64>, limit: usize },
    /// `frame_floor_fps: None` only reports.
    Settings { frame_floor_fps: Option<u32> },
    DesyncTrace { from: Option<u64>, to: Option<u64> },
    /// `from_tick`: with `b: "replay"`, continue a bounded check from here.
    DesyncCompare { a: TraceRef, b: TraceRef, from_tick: Option<u64> },
}

/// What a note is left on, before the entity is resolved.
pub enum NoteOn {
    Entity(EntityKey),
    Path(String),
    Task(u64),
}

/// One side of `desync_compare`.
#[derive(Clone, Debug, PartialEq)]
pub enum TraceRef {
    Live,
    Replay,
    File(String),
}

impl SwarmCall {
    /// What it costs, for the window's schedule.
    pub fn class(&self) -> Class {
        match self {
            SwarmCall::DesyncTrace { .. } | SwarmCall::DesyncCompare { .. } => Class::Big,
            SwarmCall::LeaseTake { .. } | SwarmCall::LeaseRelease { .. } => Class::Disk,
            SwarmCall::TaskList { .. }
            | SwarmCall::TaskGet { .. }
            | SwarmCall::IntentList { .. }
            | SwarmCall::NoteList(_)
            | SwarmCall::Rollup(_)
            | SwarmCall::LeaseList { .. } => Class::Read,
            _ => Class::Edit,
        }
    }
}

fn id_arg(args: &Json, key: &str, what: &str) -> Result<u64, String> {
    int_arg(args, key, 1, MAX_ID)?.ok_or_else(|| format!("'{key}' is required: {what}"))
}

fn text_arg(args: &Json, key: &str, max: usize, required: bool) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(s) => {
            check_text(key, s, max, required)?;
            Ok(Some(s.to_owned()))
        }
        None if required => Err(format!("'{key}' is required")),
        None => Ok(None),
    }
}

fn agent_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(n) if !n.trim().is_empty() && n.chars().count() <= 64 => Ok(Some(n.trim().to_owned())),
        Some(_) => Err(format!("'{key}' must be an agent name (1..64 characters)")),
        None => Ok(None),
    }
}

fn path_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(p) => Scope::path(p).map(|_| Some(p.to_owned())).map_err(|e| e.replace("'path'", &format!("'{key}'"))),
        None => Ok(None),
    }
}

fn ids_arg(args: &Json, key: &str) -> Result<Vec<u64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(v) => {
            let ids: Option<Vec<u64>> = v.as_array().and_then(|a| a.iter().map(|x| x.as_u64().filter(|n| (1..=MAX_ID).contains(n))).collect());
            match ids {
                Some(ids) if ids.len() <= MAX_LINKS => Ok(ids),
                _ => Err(format!("'{key}' must be an array of at most {MAX_LINKS} ids (integers >= 1)")),
            }
        }
    }
}

fn paths_arg(args: &Json, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(v) => {
            let items = v.as_array().filter(|a| a.len() <= MAX_LINKS).ok_or(format!("'{key}' must be an array of at most {MAX_LINKS} paths"))?;
            items
                .iter()
                .map(|p| {
                    let s = p.as_str().ok_or(format!("'{key}' must hold paths like \"/district_3\""))?;
                    Scope::path(s).map(|_| s.to_owned()).map_err(|e| e.replace("'path'", &format!("'{key}'")))
                })
                .collect()
        }
    }
}

fn tags_arg(args: &Json, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(v) => {
            let tags: Option<Vec<String>> = v.as_array().filter(|a| a.len() <= MAX_LINKS).and_then(|a| {
                a.iter().map(|t| t.as_str().filter(|s| (1..=64).contains(&s.chars().count()) && !s.chars().any(char::is_control)).map(str::to_owned)).collect()
            });
            tags.ok_or(format!("'{key}' must be an array of at most {MAX_LINKS} tags of 1..64 characters"))
        }
    }
}

fn expect_arg(args: &Json) -> Result<Option<u64>, String> {
    int_arg(args, "expect_version", 0, MAX_ID)
}

fn lease_arg(args: &Json, default: Duration) -> Result<Duration, String> {
    Ok(int_arg(args, "lease_s", 5, 3600)?.map_or(default, Duration::from_secs))
}

fn limit_arg(args: &Json, default: u64, max: u64) -> Result<usize, String> {
    Ok(int_arg(args, "limit", 1, max)?.unwrap_or(default) as usize)
}

/// `id` of one, or `all: true`.
fn one_or_all(args: &Json, tool: &str, what: &str) -> Result<Option<u64>, String> {
    let id = int_arg(args, "id", 1, MAX_ID)?;
    match (id, bool_arg(args, "all")?.unwrap_or(false)) {
        (Some(_), true) => Err(format!("{tool}: give 'id' or 'all', not both")),
        (None, false) => Err(format!("{tool} needs 'id' (a {what}) or 'all': true (every {what} you hold)")),
        (id, _) => Ok(id),
    }
}

fn scopes_arg(args: &Json) -> Result<Vec<Scope>, String> {
    let mut scopes: Vec<Scope> = paths_arg(args, "paths")?.iter().map(|p| Scope::path(p).expect("checked")).collect();
    if let Some(r) = args.get("regions").filter(|r| !r.is_null()) {
        for region in r.as_array().ok_or("'regions' must be an array of {min: [x,y,z], max: [x,y,z]}")? {
            check_keys(region, "a region", &[&["min", "max"]])?;
            match (vec3_arg(region, "min")?, vec3_arg(region, "max")?) {
                (Some(lo), Some(hi)) => scopes.push(Scope::region(lo, hi)?),
                _ => return Err("each region must be {min: [x,y,z], max: [x,y,z]} with min < max on every axis".into()),
            }
        }
    }
    if scopes.is_empty() {
        return Err("intent_declare needs 'paths' and/or 'regions': the scope you plan to work in".into());
    }
    if scopes.len() > crate::intents::MAX_SCOPES {
        return Err(format!("an intent covers at most {} paths and regions", crate::intents::MAX_SCOPES));
    }
    Ok(scopes)
}

fn statuses_arg(args: &Json) -> Result<Vec<Status>, String> {
    let all = || Status::ALL.map(Status::name).join(", ");
    match args.get("status") {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Str(s)) => Status::parse(s).map(|s| vec![s]).ok_or(format!("'status' must be one of {}", all())),
        Some(Json::Arr(a)) => a
            .iter()
            .map(|s| s.as_str().and_then(Status::parse).ok_or(format!("'status' entries must be one of {}", all())))
            .collect(),
        Some(_) => Err(format!("'status' must be a status or an array of them ({})", all())),
    }
}

fn trace_ref(args: &Json, key: &str) -> Result<TraceRef, String> {
    match string_arg(args, key)? {
        Some("live") => Ok(TraceRef::Live),
        Some("replay") => Ok(TraceRef::Replay),
        Some(p) if p.to_ascii_lowercase().ends_with(".json") => Ok(TraceRef::File(p.to_owned())),
        Some(_) => Err(format!("'{key}' must be \"live\", \"replay\" or a trace file ending in .json (from desync_trace), relative to this engine's captures folder")),
        None => Err(format!("'{key}' is required: \"live\", \"replay\" or a trace file")),
    }
}

/// `Some(parsed)` when `name` is a swarm communication tool.
pub fn parse(name: &str, args: &Json) -> Option<Result<SwarmCall, String>> {
    let parsed = match name {
        "task_post" => (|| {
            check_keys(args, name, &[&["title", "detail", "deps", "paths", "entities", "claims", "tags", "assign", "parent", "priority"]])?;
            Ok(SwarmCall::TaskPost(TaskSpec {
                title: text_arg(args, "title", MAX_TITLE, true)?.expect("required"),
                detail: text_arg(args, "detail", MAX_DETAIL, false)?.unwrap_or_default(),
                deps: ids_arg(args, "deps")?,
                paths: paths_arg(args, "paths")?,
                entities: ids_arg(args, "entities")?,
                claims: ids_arg(args, "claims")?,
                tags: tags_arg(args, "tags")?,
                assignee: agent_arg(args, "assign")?,
                parent: int_arg(args, "parent", 1, MAX_ID)?,
                priority: int_arg(args, "priority", 0, 9)?.map(|p| p as u8),
            }))
        })(),
        "task_take" => (|| {
            check_keys(args, name, &[&["id", "expect_version"]])?;
            Ok(SwarmCall::TaskTake { id: id_arg(args, "id", "the task to take")?, expect: expect_arg(args)? })
        })(),
        "task_update" => (|| {
            check_keys(
                args,
                name,
                &[&[
                    "id", "status", "blocker", "progress", "question", "answer", "title", "detail", "assign", "priority", "add_deps", "remove_deps",
                    "add_paths", "add_entities", "add_claims", "add_tags", "expect_version",
                ]],
            )?;
            let status = match string_arg(args, "status")? {
                None => None,
                Some(s @ ("open" | "taken" | "blocked")) => Status::parse(s),
                Some(_) => return Err("'status' must be \"blocked\" (with 'blocker'), \"taken\" (unblocked) or \"open\" (give it back); finish with task_done".into()),
            };
            let answer = match args.get("answer").filter(|a| !a.is_null()) {
                None => None,
                Some(a) => {
                    check_keys(a, "answer", &[&["question", "text"]])?;
                    let q = int_arg(a, "question", 1, 1000)?.ok_or("'answer' needs 'question' (its id) and 'text'")?;
                    Some((q as u32, text_arg(a, "text", crate::board::MAX_QA_TEXT, true)?.expect("required")))
                }
            };
            let assignee = match args.get("assign") {
                None => None,
                Some(Json::Null) => Some(None),
                Some(_) => Some(agent_arg(args, "assign")?),
            };
            Ok(SwarmCall::TaskUpdate {
                id: id_arg(args, "id", "the task")?,
                patch: TaskPatch {
                    status,
                    blocker: text_arg(args, "blocker", MAX_TEXT, false)?,
                    progress: text_arg(args, "progress", MAX_TEXT, false)?,
                    question: text_arg(args, "question", crate::board::MAX_QA_TEXT, false)?.filter(|q| !q.trim().is_empty()),
                    answer,
                    title: text_arg(args, "title", MAX_TITLE, false)?.filter(|t| !t.trim().is_empty()),
                    detail: text_arg(args, "detail", MAX_DETAIL, false)?,
                    assignee,
                    priority: int_arg(args, "priority", 0, 9)?.map(|p| p as u8),
                    add_deps: ids_arg(args, "add_deps")?,
                    remove_deps: ids_arg(args, "remove_deps")?,
                    add_paths: paths_arg(args, "add_paths")?,
                    add_entities: ids_arg(args, "add_entities")?,
                    add_claims: ids_arg(args, "add_claims")?,
                    add_tags: tags_arg(args, "add_tags")?,
                    expect_version: expect_arg(args)?,
                },
            })
        })(),
        "task_done" => (|| {
            check_keys(args, name, &[&["id", "summary", "cancel", "expect_version"]])?;
            Ok(SwarmCall::TaskDone {
                id: id_arg(args, "id", "the task")?,
                summary: text_arg(args, "summary", MAX_TEXT, false)?.unwrap_or_default(),
                cancel: bool_arg(args, "cancel")?.unwrap_or(false),
                expect: expect_arg(args)?,
            })
        })(),
        "task_list" => (|| {
            check_keys(
                args,
                name,
                &[&["status", "owner", "mine", "posted_by", "assignee", "ready", "ready_for_me", "path_prefix", "tag", "parent", "changed_since", "events_since", "ack_board_warning", "cursor", "limit"]],
            )?;
            Ok(SwarmCall::TaskList {
                query: TaskQuery {
                    statuses: statuses_arg(args)?,
                    owner: agent_arg(args, "owner")?,
                    posted_by: agent_arg(args, "posted_by")?,
                    assignee: agent_arg(args, "assignee")?,
                    ready: bool_arg(args, "ready")?.unwrap_or(false),
                    ready_for: None,
                    path_prefix: path_arg(args, "path_prefix")?,
                    tag: string_arg(args, "tag")?.map(str::to_owned),
                    parent: int_arg(args, "parent", 1, MAX_ID)?,
                    changed_since: int_arg(args, "changed_since", 0, MAX_ID)?,
                    after: int_arg(args, "cursor", 0, MAX_ID)?,
                    limit: limit_arg(args, 50, 200)?,
                },
                mine: bool_arg(args, "mine")?.unwrap_or(false),
                ready_for_me: bool_arg(args, "ready_for_me")?.unwrap_or(false),
                events_since: int_arg(args, "events_since", 0, MAX_ID)?,
                ack_board_warning: bool_arg(args, "ack_board_warning")?.unwrap_or(false),
            })
        })(),
        "task_get" => (|| {
            check_keys(args, name, &[&["id", "events"]])?;
            Ok(SwarmCall::TaskGet { id: id_arg(args, "id", "the task")?, events: int_arg(args, "events", 0, 64)?.unwrap_or(16) as usize })
        })(),
        "intent_declare" => (|| {
            check_keys(args, name, &[&["paths", "regions", "text", "task", "lease_s", "replace"]])?;
            Ok(SwarmCall::IntentDeclare {
                scopes: scopes_arg(args)?,
                text: text_arg(args, "text", crate::intents::MAX_TEXT, true)?.expect("required"),
                task: int_arg(args, "task", 1, MAX_ID)?,
                lease: lease_arg(args, crate::intents::DEFAULT_LEASE)?,
                replace: int_arg(args, "replace", 1, MAX_ID)?,
            })
        })(),
        "intent_list" => (|| {
            check_keys(args, name, &[&["owner", "mine", "path_prefix", "cursor", "limit"]])?;
            Ok(SwarmCall::IntentList {
                owner: agent_arg(args, "owner")?,
                mine: bool_arg(args, "mine")?.unwrap_or(false),
                path_prefix: path_arg(args, "path_prefix")?,
                after: int_arg(args, "cursor", 0, MAX_ID)?,
                limit: limit_arg(args, 50, 200)?,
            })
        })(),
        "intent_clear" => (|| {
            check_keys(args, name, &[&["id", "all"]])?;
            Ok(SwarmCall::IntentClear { id: one_or_all(args, name, "intent")? })
        })(),
        "note_add" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "on_path", "task", "text"]])?;
            let entity = ["id", "path", "name"].iter().any(|k| args.get(k).is_some_and(|v| !v.is_null()));
            let on_path = path_arg(args, "on_path")?;
            let task = int_arg(args, "task", 1, MAX_ID)?;
            let target = match (entity, on_path, task) {
                (true, None, None) => NoteOn::Entity(crate::tools::entity_key(args, name)?),
                (false, Some(p), None) => NoteOn::Path(p),
                (false, None, Some(t)) => NoteOn::Task(t),
                _ => return Err("note_add needs exactly one target: an entity ('id', 'path' or 'name'), 'on_path' (a path, existing or not) or 'task'".into()),
            };
            Ok(SwarmCall::NoteAdd { target, text: text_arg(args, "text", MAX_TEXT, true)?.expect("required") })
        })(),
        "note_list" => (|| {
            check_keys(args, name, &[&["path_prefix", "entity", "task", "by", "cursor", "limit"]])?;
            Ok(SwarmCall::NoteList(NoteQuery {
                path_prefix: path_arg(args, "path_prefix")?,
                entity: int_arg(args, "entity", 0, MAX_ID)?,
                task: int_arg(args, "task", 1, MAX_ID)?,
                by: agent_arg(args, "by")?,
                after: int_arg(args, "cursor", 0, MAX_ID)?,
                limit: limit_arg(args, 50, 200)?,
            }))
        })(),
        "note_delete" => (|| {
            check_keys(args, name, &[&["id", "force"]])?;
            Ok(SwarmCall::NoteDelete { id: id_arg(args, "id", "the note")?, force: bool_arg(args, "force")?.unwrap_or(false) })
        })(),
        "world_rollup" => (|| {
            check_keys(args, name, &[&["by", "agent", "task", "since", "game", "limit"]])?;
            let by = match string_arg(args, "by")?.unwrap_or("agent") {
                "agent" => By::Agent,
                "task" => By::Task,
                _ => return Err("'by' must be \"agent\" or \"task\"".into()),
            };
            Ok(SwarmCall::Rollup(RollupQuery {
                by,
                agent: agent_arg(args, "agent")?,
                task: int_arg(args, "task", 1, MAX_ID)?,
                since: int_arg(args, "since", 0, MAX_ID)?.unwrap_or(0),
                game: bool_arg(args, "game")?.unwrap_or(false),
                limit: limit_arg(args, 32, MAX_GROUPS as u64)?,
            }))
        })(),
        "lease_take" => (|| {
            check_keys(args, name, &[&["kind", "count", "gpu_ms_per_s", "note", "lease_s"]])?;
            use crate::leases::Want;
            let want = match string_arg(args, "kind")? {
                Some("port") => Want::Ports(int_arg(args, "count", 1, u64::from(crate::leases::MAX_PORTS))?.unwrap_or(1) as u32),
                Some("gpu") => Want::Gpu(int_arg(args, "gpu_ms_per_s", 1, 64_000)?.ok_or("kind \"gpu\" needs 'gpu_ms_per_s': the GPU time per second you will use (1000 = a whole GPU)")? as u32),
                Some("scratch") => Want::Scratch,
                _ => return Err("'kind' is required: \"port\" (with 'count'), \"gpu\" (with 'gpu_ms_per_s') or \"scratch\" (a folder of your own)".into()),
            };
            if !matches!(want, Want::Ports(_)) && args.get("count").is_some_and(|c| !c.is_null()) {
                return Err("'count' is for kind \"port\"".into());
            }
            let note = text_arg(args, "note", 200, false)?.unwrap_or_default();
            Ok(SwarmCall::LeaseTake { want, note, lease: lease_arg(args, crate::leases::DEFAULT_LEASE)? })
        })(),
        "lease_renew" => (|| {
            check_keys(args, name, &[&["id", "all", "lease_s"]])?;
            Ok(SwarmCall::LeaseRenew { id: one_or_all(args, name, "lease")?, lease: lease_arg(args, crate::leases::DEFAULT_LEASE)? })
        })(),
        "lease_release" => (|| {
            check_keys(args, name, &[&["id", "all", "delete"]])?;
            Ok(SwarmCall::LeaseRelease { id: one_or_all(args, name, "lease")?, delete: bool_arg(args, "delete")?.unwrap_or(false) })
        })(),
        "lease_list" => (|| {
            check_keys(args, name, &[&["kind", "owner", "mine", "cursor", "limit"]])?;
            let kind = match string_arg(args, "kind")? {
                None => None,
                Some(k @ ("port" | "gpu" | "scratch")) => Some(k.to_owned()),
                Some(_) => return Err("'kind' must be \"port\", \"gpu\" or \"scratch\"".into()),
            };
            Ok(SwarmCall::LeaseList {
                kind,
                owner: agent_arg(args, "owner")?,
                mine: bool_arg(args, "mine")?.unwrap_or(false),
                after: int_arg(args, "cursor", 0, MAX_ID)?,
                limit: limit_arg(args, 50, 200)?,
            })
        })(),
        "swarm_settings" => (|| {
            check_keys(args, name, &[&["frame_floor_fps"]])?;
            Ok(SwarmCall::Settings {
                frame_floor_fps: int_arg(args, "frame_floor_fps", u64::from(MIN_FRAME_FLOOR_FPS), u64::from(MAX_FRAME_FLOOR_FPS))?.map(|f| f as u32),
            })
        })(),
        "desync_trace" => (|| {
            check_keys(args, name, &[&["from_tick", "to_tick"]])?;
            let (from, to) = (int_arg(args, "from_tick", 0, MAX_ID)?, int_arg(args, "to_tick", 0, MAX_ID)?);
            if let (Some(f), Some(t)) = (from, to) {
                if f > t {
                    return Err("'from_tick' must not be after 'to_tick'".into());
                }
            }
            Ok(SwarmCall::DesyncTrace { from, to })
        })(),
        "desync_compare" => (|| {
            check_keys(args, name, &[&["a", "b", "from_tick"]])?;
            let (a, b) = (trace_ref(args, "a")?, trace_ref(args, "b")?);
            if a == TraceRef::Replay || (b == TraceRef::Replay && a != TraceRef::Live) {
                return Err("\"replay\" compares this engine with its own journal: give a: \"live\", b: \"replay\"".into());
            }
            let from_tick = int_arg(args, "from_tick", 0, MAX_ID)?;
            if from_tick.is_some() && b != TraceRef::Replay {
                return Err("'from_tick' continues a b: \"replay\" check (pass the next_from_tick it returned)".into());
            }
            Ok(SwarmCall::DesyncCompare { a, b, from_tick })
        })(),
        _ => return None,
    };
    Some(parsed)
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let arr = |item: &str, d: &str| Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from(item))])), ("description", Json::from(d))]);
    let region = Json::obj([
        ("type", Json::from("array")),
        (
            "items",
            Json::obj([
                ("type", Json::from("object")),
                ("properties", Json::obj([("min", arr("number", "[x, y, z]")), ("max", arr("number", "[x, y, z]"))])),
            ]),
        ),
        ("description", Json::from("World boxes {min: [x,y,z], max: [x,y,z]} (an entity is inside when its world origin is)")),
    ]);
    let page = || vec![("cursor", t("integer", "next_cursor from the previous page")), ("limit", t("integer", "Page size, 1..200 (default 50)"))];
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    vec![
        def(
            "task_post",
            "Put a task on the shared board so the swarm knows what needs doing (the parent assigns work here). Agents \
             task_take open tasks whose deps are finished (done or cancelled). Returns the task. Saved with the world.",
            schema(
                vec![
                    ("title", t("string", "What to do, 1..200 characters")),
                    ("detail", t("string", "More detail, up to 4000 characters")),
                    ("deps", arr("integer", "Tasks that must be done first")),
                    ("paths", arr("string", "World paths it is about (\"/east_gate\")")),
                    ("entities", arr("integer", "Entity ids it is about")),
                    ("claims", arr("integer", "Claims linked to it")),
                    ("tags", arr("string", "Labels for task_list {tag}")),
                    ("assign", t("string", "Only this agent may take it")),
                    ("parent", t("integer", "The task this is part of")),
                    ("priority", t("integer", "0..9 (default 5)")),
                ],
                &["title"],
            ),
        ),
        def(
            "task_take",
            "Take an open task: you become its owner and it is your current task (your journal entries, refused writes and \
             new claims are linked to it, for world_rollup). Refused while deps aren't finished (deps_not_done lists them), when \
             another agent has it (task_taken), or when it is assigned to someone else. Needs agent_hello.",
            schema(vec![("id", t("integer", "Task id")), ("expect_version", t("integer", "Refuse if the task changed since you read it"))], &["id"]),
        ),
        def(
            "task_update",
            "Report on a task. Owner: progress, status \"blocked\" (with blocker) / \"taken\" (unblocked) / \"open\" (give it \
             back). Poster: title, detail, assign (null un-assigns), priority, add_deps/remove_deps; the poster may also \
             reopen it. Anyone: question (open questions show in roll-ups), answer {question, text}, add_paths/add_entities/\
             add_claims/add_tags. All or nothing.",
            schema(
                vec![
                    ("id", t("integer", "Task id")),
                    ("status", t("string", "\"blocked\", \"taken\" or \"open\"")),
                    ("blocker", t("string", "What blocks it (with status \"blocked\")")),
                    ("progress", t("string", "Your latest progress line")),
                    ("question", t("string", "A question for the poster or the swarm")),
                    ("answer", Json::obj([("type", Json::from("object")), ("description", Json::from("{question: id, text}"))])),
                    ("title", t("string", "New title (poster)")),
                    ("detail", t("string", "New detail (poster)")),
                    ("assign", t("string", "Assign to an agent; null un-assigns (poster)")),
                    ("priority", t("integer", "0..9 (poster)")),
                    ("add_deps", arr("integer", "More dependencies (poster)")),
                    ("remove_deps", arr("integer", "Dependencies to drop (poster)")),
                    ("add_paths", arr("string", "More paths")),
                    ("add_entities", arr("integer", "More entity ids")),
                    ("add_claims", arr("integer", "More claim ids")),
                    ("add_tags", arr("string", "More tags")),
                    ("expect_version", t("integer", "Refuse if the task changed since you read it (stale_task)")),
                ],
                &["id"],
            ),
        ),
        def(
            "task_done",
            "Finish your task with a summary (or cancel: true, the owner or the poster). It stays on the board as done.",
            schema(
                vec![
                    ("id", t("integer", "Task id")),
                    ("summary", t("string", "What was done (shown in roll-ups)")),
                    ("cancel", t("boolean", "Cancel instead of finishing")),
                    ("expect_version", t("integer", "Refuse if the task changed since you read it")),
                ],
                &["id"],
            ),
        ),
        def(
            "task_list",
            "The board, one page in id order, with counts per status and board_seq. Filters: status (one or a list), owner, \
             mine, posted_by, assignee, ready (open, deps finished), ready_for_me (ready and takeable by you), path_prefix, tag, \
             parent. Poll with changed_since: board_seq from your last call to get only tasks changed since (events_since adds the board's event feed).",
            schema(
                [
                    vec![
                        ("status", t("string", "open, taken, blocked, done or cancelled (or an array of them)")),
                        ("owner", t("string", "Agent name")),
                        ("mine", t("boolean", "Tasks you own")),
                        ("posted_by", t("string", "Agent name")),
                        ("assignee", t("string", "Agent name")),
                        ("ready", t("boolean", "Open with every dependency finished")),
                        ("ready_for_me", t("boolean", "Ready, and not assigned to someone else")),
                        ("path_prefix", t("string", "Tasks about this path, above or below it")),
                        ("tag", t("string", "Tasks with this tag")),
                        ("parent", t("integer", "Subtasks of this task")),
                        ("changed_since", t("integer", "Only tasks changed after this board_seq")),
                        ("events_since", t("integer", "Also return board events (posts, takes, progress, intents, leases...) after this board_seq")),
                        ("ack_board_warning", t("boolean", "Stop showing board_warning (the world's board.json was unreadable and moved aside)")),
                    ],
                    page(),
                ]
                .concat(),
                &[],
            ),
        ),
        def(
            "task_get",
            "One task in full: detail, links, questions and answers, its last events (who did what, at which tick), notes on \
             it and the work linked to it.",
            schema(vec![("id", t("integer", "Task id")), ("events", t("integer", "Events to include, 0..64 (default 16)"))], &["id"]),
        ),
        def(
            "intent_declare",
            "Say what you are about to work on BEFORE editing: paths and/or regions plus a line of text, with a lease \
             (default 600 s). The answer lists other agents' intents and claims that overlap it (overlaps, warning), so you can \
             coordinate first. Intents never block writes (claims do). replace: one of your intents to update and renew.",
            schema(
                vec![
                    ("paths", arr("string", "Subtrees you plan to change")),
                    ("regions", region),
                    ("text", t("string", "What you plan to do, up to 500 characters")),
                    ("task", t("integer", "The task it is for")),
                    ("lease_s", t("integer", "Seconds, 5..3600 (default 600)")),
                    ("replace", t("integer", "Your intent to replace (renews it)")),
                ],
                &["text"],
            ),
        ),
        def(
            "intent_list",
            "Live intents (who plans to work where), one page; filter by owner, mine or path_prefix.",
            schema(
                [vec![("owner", t("string", "Agent name")), ("mine", t("boolean", "Only yours")), ("path_prefix", t("string", "Intents on paths above or below it"))], page()]
                    .concat(),
                &[],
            ),
        ),
        def("intent_clear", "End one of your intents (id) or all of them (all: true).", schema(vec![("id", t("integer", "Intent id")), ("all", t("boolean", "All yours"))], &[])),
        def(
            "note_add",
            "Leave a note for whoever comes next (a handoff): on an entity (id, path or name), on a path that may not exist \
             yet (on_path) or on a task. Notes are saved with the world.",
            schema(
                vec![
                    ("id", t("integer", "Entity id")),
                    ("path", t("string", "Entity path (the entity must exist)")),
                    ("name", t("string", "Entity name (unique in the world)")),
                    ("on_path", t("string", "A path, existing or not")),
                    ("task", t("integer", "Task id")),
                    ("text", t("string", "Up to 2000 characters")),
                ],
                &["text"],
            ),
        ),
        def(
            "note_list",
            "Notes, one page in id order: at or below path_prefix (entity and path notes), on an entity id, on a task, or by an agent.",
            schema(
                [
                    vec![
                        ("path_prefix", t("string", "Notes at or below this path")),
                        ("entity", t("integer", "Notes on this entity")),
                        ("task", t("integer", "Notes on this task")),
                        ("by", t("string", "Notes left by this agent")),
                    ],
                    page(),
                ]
                .concat(),
                &[],
            ),
        ),
        def(
            "note_delete",
            "Delete a note you left (force: true for someone else's).",
            schema(vec![("id", t("integer", "Note id")), ("force", t("boolean", "Delete another agent's note"))], &["id"]),
        ),
        def(
            "world_rollup",
            "A parent's report instead of a log: journal entries after 'since' summarised per agent (by: \"agent\", default) \
             or per task (by: \"task\"). Each group: a one-line summary, changes by op, distinct entities, where (under, areas), \
             tick and journal range, refused writes, tasks done/taken/blocked, open questions, claims, intents and leases. \
             Scans up to 50,000 entries per call: pass next_since back while more is true.",
            schema(
                vec![
                    ("by", t("string", "\"agent\" (default) or \"task\"")),
                    ("agent", t("string", "Only this agent")),
                    ("task", t("integer", "Only this task (by: \"task\")")),
                    ("since", t("integer", "Journal id: entries after it (default 0, everything kept)")),
                    ("game", t("boolean", "Count the hosted game's own ticks too")),
                    ("limit", t("integer", "Groups, 1..128 (default 32)")),
                ],
                &[],
            ),
        ),
        def(
            "lease_take",
            "Lease a resource for a swarm that runs its own engines: kind \"port\" (count free ports, default 1, from the \
             pool in perf.toml [swarm] lease_ports, default 7500-7599; each is checked bindable), \"gpu\" (gpu_ms_per_s of GPU \
             time per second out of the machine's share; advisory) or \"scratch\" (a fresh folder of your own; returns dir). \
             Leases expire (lease_s, default 600) unless renewed. Needs agent_hello.",
            schema(
                vec![
                    ("kind", t("string", "\"port\", \"gpu\" or \"scratch\"")),
                    ("count", t("integer", "Ports, 1..16 (kind port)")),
                    ("gpu_ms_per_s", t("integer", "GPU ms per second (kind gpu)")),
                    ("note", t("string", "What it is for")),
                    ("lease_s", t("integer", "Seconds, 5..3600 (default 600)")),
                ],
                &["kind"],
            ),
        ),
        def(
            "lease_renew",
            "Extend one of your leases (id) or all of them (all: true) to lease_s from now.",
            schema(vec![("id", t("integer", "Lease id")), ("all", t("boolean", "All yours")), ("lease_s", t("integer", "Seconds, 5..3600 (default 600)"))], &[]),
        ),
        def(
            "lease_release",
            "Give back one lease (id) or all of yours (all: true); delete: true also removes scratch folders with their contents.",
            schema(vec![("id", t("integer", "Lease id")), ("all", t("boolean", "All yours")), ("delete", t("boolean", "Remove scratch folders"))], &[]),
        ),
        def(
            "lease_list",
            "Live leases, one page; filter by kind, owner or mine. Also says the pools and what is free.",
            schema(
                [vec![("kind", t("string", "\"port\", \"gpu\" or \"scratch\"")), ("owner", t("string", "Agent name")), ("mine", t("boolean", "Only yours"))], page()].concat(),
                &[],
            ),
        ),
        def(
            "swarm_settings",
            "For the lead agent BEFORE deploying a swarm. Reports the operator's hardware (GPU and its kind, VRAM, CPU threads, \
             display refresh, battery or mains power, window open or headless) and the window's frame floor: under a backlog of agent calls the \
             window still presents at least frame_floor_fps (default 60). On low- and mid-spec laptops keep 60 so the human's \
             window stays fluid; on high-spec desktops or cloud machines lower it (30, 20) to give agents more of every \
             frame. Setting frame_floor_fps (20..240) needs agent_hello with role \"lead\"; perf.toml [swarm] frame_floor_fps sets the start value.",
            schema(vec![("frame_floor_fps", t("integer", "Frames per second the window keeps under a swarm's load, 20..240"))], &[]),
        ),
        def(
            "desync_trace",
            "Write this run's trace (per tick: the world hash, the hosted game's subsystem hashes, and the entities that \
             changed going into it, with per-field digests) to a file for desync_compare. The engine keeps the last 4096 \
             ticks; from_tick/to_tick narrow it. Returns the file path and a summary.",
            schema(vec![("from_tick", t("integer", "First tick")), ("to_tick", t("integer", "Last tick"))], &[]),
        ),
        def(
            "desync_compare",
            "Find where two runs that should agree first differ: a and b are trace files (from desync_trace, any engine; \
             named relative to this engine's captures folder, its captures root or its worlds folder: copy another engine's \
             trace in first; absolute paths and '..' are refused with path_not_allowed) or \"live\" (this engine now). Reports first_divergent_tick, last_matching_tick, the subsystems whose hashes differ \
             and the entities (with fields) that changed differently going into that tick, as 'components'. a: \"live\", \
             b: \"replay\" checks this engine against its own journal replayed tick by tick, and names entities whose live \
             state the journal doesn't explain; each replay call does bounded work and returns next_from_tick (done: false) to continue. Script: scripts/desync_check.py.",
            schema(
                vec![
                    ("a", t("string", "\"live\" or a trace file")),
                    ("b", t("string", "\"live\", \"replay\" or a trace file")),
                    ("from_tick", t("integer", "With b: \"replay\": continue from the next_from_tick of the last call")),
                ],
                &["a", "b"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    fn call(name: &str, a: &str) -> Result<SwarmCall, String> {
        parse(name, &args(a)).expect("a swarm tool")
    }

    #[test]
    fn task_arguments_are_checked() {
        let SwarmCall::TaskPost(s) = call("task_post", r#"{"title": "walls", "deps": [1], "paths": ["/walls"], "priority": 7, "assign": "bob"}"#).unwrap() else { panic!() };
        assert_eq!((s.title.as_str(), s.deps, s.priority, s.assignee.as_deref()), ("walls", vec![1], Some(7), Some("bob")));
        assert!(call("task_post", "{}").err().unwrap().contains("'title'"));
        assert!(call("task_post", r#"{"title": " "}"#).err().unwrap().contains("empty"));
        assert!(call("task_post", r#"{"title": "x", "paths": ["walls"]}"#).err().unwrap().contains("'paths'"));
        assert!(call("task_post", r#"{"title": "x", "priority": 10}"#).is_err());
        assert!(call("task_post", r#"{"title": "x", "titel": 1}"#).err().unwrap().contains("did you mean 'title'"));
        assert!(call("task_update", r#"{"id": 1, "status": "done"}"#).err().unwrap().contains("task_done"));
        let SwarmCall::TaskUpdate { patch, .. } = call("task_update", r#"{"id": 1, "assign": null, "answer": {"question": 2, "text": "slate"}}"#).unwrap() else { panic!() };
        assert_eq!((patch.assignee, patch.answer), (Some(None), Some((2, "slate".into()))));
        let SwarmCall::TaskList { query, .. } = call("task_list", r#"{"status": ["open", "blocked"], "changed_since": 4}"#).unwrap() else { panic!() };
        assert_eq!((query.statuses, query.changed_since), (vec![Status::Open, Status::Blocked], Some(4)));
        assert!(call("task_list", r#"{"status": "nope"}"#).is_err());
        assert!(call("task_take", "{}").err().unwrap().contains("'id'"));
    }

    #[test]
    fn intents_notes_leases_and_desync_arguments() {
        let SwarmCall::IntentDeclare { scopes, .. } =
            call("intent_declare", r#"{"paths": ["/east"], "regions": [{"min": [0,0,0], "max": [1,1,1]}], "text": "gate"}"#).unwrap()
        else {
            panic!()
        };
        assert_eq!(scopes.len(), 2);
        assert!(call("intent_declare", r#"{"text": "x"}"#).err().unwrap().contains("'paths'"));
        assert!(call("intent_declare", r#"{"paths": ["/a"]}"#).err().unwrap().contains("'text'"));
        assert!(call("intent_clear", "{}").is_err());
        assert!(matches!(call("note_add", r#"{"on_path": "/x", "text": "hi"}"#).unwrap(), SwarmCall::NoteAdd { target: NoteOn::Path(_), .. }));
        assert!(matches!(call("note_add", r#"{"path": "/x", "text": "hi"}"#).unwrap(), SwarmCall::NoteAdd { target: NoteOn::Entity(_), .. }));
        assert!(call("note_add", r#"{"path": "/x", "task": 1, "text": "hi"}"#).err().unwrap().contains("exactly one"));
        assert!(matches!(call("lease_take", r#"{"kind": "port", "count": 3}"#).unwrap(), SwarmCall::LeaseTake { want: crate::leases::Want::Ports(3), .. }));
        assert!(call("lease_take", r#"{"kind": "gpu"}"#).err().unwrap().contains("gpu_ms_per_s"));
        assert!(call("lease_take", r#"{"kind": "scratch", "count": 2}"#).is_err());
        assert!(call("swarm_settings", r#"{"frame_floor_fps": 10}"#).is_err());
        assert!(matches!(call("swarm_settings", r#"{"frame_floor_fps": 30}"#).unwrap(), SwarmCall::Settings { frame_floor_fps: Some(30) }));
        assert!(matches!(call("desync_compare", r#"{"a": "live", "b": "replay"}"#).unwrap(), SwarmCall::DesyncCompare { b: TraceRef::Replay, .. }));
        assert!(call("desync_compare", r#"{"a": "replay", "b": "live"}"#).is_err());
        assert!(call("desync_compare", r#"{"a": "x.txt", "b": "live"}"#).is_err());
        assert!(call("world_rollup", r#"{"by": "team"}"#).is_err());
        assert!(parse("world_spawn", &Json::Null).is_none());
    }

    #[test]
    fn definitions_are_well_formed_and_every_tool_parses() {
        for d in definitions() {
            assert!(d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && d.name.len() <= 64);
            assert!(d.input_schema.get("properties").is_some());
            assert!(parse(&d.name, &Json::Null).is_some(), "{} has no parser", d.name);
            assert!(!d.description.contains("  "), "{}: runs of spaces", d.name);
        }
    }
}
