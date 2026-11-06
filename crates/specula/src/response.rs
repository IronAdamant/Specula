//! `response_format`: concise or detailed answers from the heavier tools.
//!
//! Tokens are a cost (every agent in a swarm pays for every byte it reads), so the heavy tools
//! answer **concisely by default**: what an agent acts on (ids, paths, screen boxes, totals,
//! status, problems), as compact JSON, without what it rarely needs (per-pass tables, look fields
//! it can query, internals). `response_format: "detailed"` gives the full answer, exactly as
//! before. The engine always builds the detailed answer; the concise one is cut from it on the
//! connection's thread ([`shape`]), so the engine thread pays nothing. Some tools already had a
//! knob, which `response_format` sets: `world_query` (detailed = `fields: ["all"]`) and
//! `world_history` (detailed = `detail: true`). A game's tools parse it themselves (Sepulcra's
//! `play_state`), as do `playtest_*`. Every tool's answer also goes through [`tidy`]: compact
//! JSON, and errors as `{error, message}`. See `docs/design/TOOLS.md` for the measured savings.

use specula_core::Json;
use specula_protocol::{Content, ToolDef, ToolOutput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Concise,
    Detailed,
}

pub const KEY: &str = "response_format";

impl Format {
    /// `response_format` from a tool's arguments (`default` when absent).
    pub fn arg(args: &Json, default: Format) -> Result<Format, String> {
        match args.get(KEY) {
            None | Some(Json::Null) => Ok(default),
            Some(Json::Str(s)) if s == "concise" => Ok(Format::Concise),
            Some(Json::Str(s)) if s == "detailed" => Ok(Format::Detailed),
            Some(_) => Err(format!("'{KEY}' must be \"concise\" or \"detailed\"")),
        }
    }

    /// The schema property every tool with a format uses.
    pub fn schema() -> Json {
        Self::schema_saying("concise (default): what you act on, as compact JSON; detailed: everything")
    }

    fn schema_saying(what: &str) -> Json {
        Json::obj([
            ("type", Json::from("string")),
            ("enum", Json::arr([Json::from("concise"), Json::from("detailed")])),
            ("description", Json::from(what)),
        ])
    }
}

/// Engine tools whose answers [`shape`] cuts, and what their concise answer leaves out.
pub const SHAPED: [(&str, &str); 6] = [
    (
        "sight_frame",
        "concise (default): subjects with id, name, path, screen boxes, visible_px, occlusion, distance, layer (ui) and \
         transient only (their look \
         and transform are in world_query), no id_buffer/models/lights internals; the full packet is always saved at \
         image.packet_path. detailed: the whole packet",
    ),
    (
        "perf_frame",
        "concise (default): errors, problems, totals per path (GPU ms, CPU ms, draws, memory), pacing, sim and physics \
         totals; detailed: every pass, pipeline, heap, model and budget table",
    ),
    ("engine_info", "concise (default): engine, GPU, world summary, agents, clock, game status, swarm counts, the disk budget's state; detailed: model library, paths, journal internals, disk use by category"),
    (
        "world_list",
        "concise (default): whole entities with defaults left out (name = the last path segment, rotation_deg [0,0,0], \
         scale [1,1,1], modified_by = created_by); detailed: every field",
    ),
    ("world_query", "concise (default): the fields asked for (default id, path, shape, position, tags, version); detailed: every field (fields: [\"all\"])"),
    ("world_history", "concise (default): one line per entry; detailed: each entry's changes (detail: true)"),
];

/// Adds `response_format` to the schemas of the [`SHAPED`] tools, and to `agent_hello` (a
/// session's default).
pub fn add_to_schemas(defs: &mut [ToolDef]) {
    for d in defs.iter_mut() {
        let what = match SHAPED.iter().find(|(n, _)| *n == d.name) {
            Some((_, what)) => *what,
            None if d.name == "agent_hello" => {
                "this agent's default for every tool with a response_format (kept when a later session resumes it): \
                 concise (the engine's default unless it runs with --response-format detailed) or detailed (scripts that \
                 check data in depth)"
            }
            None => continue,
        };
        let mut props = d.input_schema.get("properties").cloned().unwrap_or_else(|| Json::obj::<&str>([]));
        props.set(KEY, Format::schema_saying(what));
        d.input_schema.set("properties", props);
    }
}

/// Default formats: the engine's (`--response-format`, concise unless set) and each agent's
/// (`agent_hello {response_format}`). An agent's default belongs to its identity (its resume
/// token), so a later session that resumes it keeps the default; a session that never said hello
/// can't have one. Shared by the connection threads and the engine thread (playtests call as
/// their starter's session).
pub struct Defaults {
    engine: Format,
    inner: std::sync::Mutex<DefaultsInner>,
}

/// Entries kept per map at most; past it the least recently used goes.
const MAX_DEFAULTS: usize = 4096;

#[derive(Default)]
struct DefaultsInner {
    /// Use counter (the age of entries).
    clock: u64,
    /// Session id to the identity it says hello as.
    identity_of: std::collections::BTreeMap<String, (String, u64)>,
    /// Identity to its default.
    formats: std::collections::BTreeMap<String, (Format, u64)>,
}

/// Inserts `k` with use time `now`, evicting the least recently used entry past [`MAX_DEFAULTS`].
fn put<V>(map: &mut std::collections::BTreeMap<String, (V, u64)>, k: &str, v: V, now: u64) {
    map.insert(k.to_owned(), (v, now));
    while map.len() > MAX_DEFAULTS {
        let oldest = map.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| k.clone());
        match oldest {
            Some(k) => map.remove(&k),
            None => break,
        };
    }
}

impl Defaults {
    pub fn new(engine: Format) -> std::sync::Arc<Defaults> {
        std::sync::Arc::new(Defaults { engine, inner: std::sync::Mutex::new(DefaultsInner::default()) })
    }

    /// The default for `session`: its identity's, else the engine's.
    pub fn get(&self, session: &str) -> Format {
        let mut g = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        g.clock += 1;
        let now = g.clock;
        let Some(identity) = g.identity_of.get_mut(session).map(|(i, t)| {
            *t = now;
            i.clone()
        }) else {
            return self.engine;
        };
        match g.formats.get_mut(&identity) {
            Some((f, t)) => {
                *t = now;
                *f
            }
            None => self.engine,
        }
    }

    /// A successful `agent_hello` of `session` as `identity` (its resume token), with the default
    /// it asked for, if it asked.
    pub fn hello(&self, session: &str, identity: &str, format: Option<Format>) {
        let mut g = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        g.clock += 1;
        let now = g.clock;
        put(&mut g.identity_of, session, identity.to_owned(), now);
        if let Some(f) = format {
            put(&mut g.formats, identity, f, now);
        }
    }
}

/// Whether `def` takes `response_format` itself (a game's tool, `logic_status`, `playtest_*`),
/// as opposed to being [`SHAPED`] here.
pub fn takes_format(def: &ToolDef) -> bool {
    !SHAPED.iter().any(|(n, _)| *n == def.name) && def.input_schema.get("properties").is_some_and(|p| p.get(KEY).is_some())
}

/// `args` with `response_format: "detailed"` when the default is detailed and the call doesn't
/// choose (for tools that [`takes_format`]); `None` when nothing changes.
pub fn with_default(args: &Json, default: Format) -> Option<Json> {
    if default == Format::Concise || args.get(KEY).is_some_and(|v| !v.is_null()) {
        return None;
    }
    let mut out = match args {
        Json::Obj(_) => args.clone(),
        _ => Json::obj::<&str>([]),
    };
    out.set(KEY, Json::from("detailed"));
    Some(out)
}

/// Takes `response_format` out of a [`SHAPED`] tool's arguments (setting the tool's own knob for
/// a detailed answer); `default` when the call doesn't choose. `None`: the tool isn't shaped here
/// (its arguments are left alone).
pub fn split(tool: &str, args: &Json, default: Format) -> Result<Option<(Format, Json)>, String> {
    if !SHAPED.iter().any(|(n, _)| *n == tool) {
        return Ok(None);
    }
    let format = Format::arg(args, default)?;
    let mut rest = match args {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| k != KEY).cloned().collect()),
        other => other.clone(),
    };
    if format == Format::Detailed && rest.is_null() && matches!(tool, "world_query" | "world_history") {
        rest = Json::obj::<&str>([]);
    }
    if format == Format::Detailed && rest.as_object().is_some() {
        match tool {
            "world_query" if rest.get("fields").is_none_or(Json::is_null) => {
                rest.set("fields", Json::arr([Json::from("all")]));
            }
            "world_history" if rest.get("detail").is_none_or(Json::is_null) => {
                rest.set("detail", Json::Bool(true));
            }
            _ => {}
        }
    }
    Ok(Some((format, rest)))
}

/// A [`SHAPED`] tool's answer in `format` (errors and non-JSON answers pass through).
pub fn shape(tool: &str, format: Format, out: ToolOutput) -> ToolOutput {
    if format == Format::Detailed || out.is_error {
        return out;
    }
    let cut: fn(&Json) -> Json = match tool {
        "sight_frame" => sight_frame,
        "perf_frame" => perf_frame,
        "engine_info" => engine_info,
        "world_list" => world_list,
        _ => return out,
    };
    let mut out = out;
    for c in &mut out.content {
        if let Content::Text(t) = c {
            if let Ok(v) = Json::parse(t) {
                *t = cut(&v).to_string();
            }
            break;
        }
    }
    out
}

/// Error texts that aren't JSON yet, by what they contain, and their codes.
const PLAIN_ERRORS: [(&str, &str); 5] = [
    ("shutting down", "shutting_down"),
    ("too busy", "engine_busy"),
    ("skipped:", "engine_busy"),
    ("no game is running", "no_game"),
    ("unknown game tool", "unknown_tool"),
];

/// Refusals whose **same call may succeed later, unchanged**: something else holds what it
/// needs for now (another agent's claim, a busy worker or GPU, a load or build in progress).
/// Every other refusal is lasting: the call fails again until the caller changes something.
/// An error may also say so itself (`retry` set where it is made), which wins.
pub const TRANSIENT: [&str; 17] = [
    "claimed",
    "claims_overlap",
    "busy",
    "engine_busy",
    "job_busy",
    "studio_busy",
    "gpu_busy",
    "capture_in_progress",
    "playtest_running",
    "onboarding_running",
    "loading",
    "nav_building",
    "navmesh_building",
    "model_building",
    "too_soon",
    "syncing",
    "window_held",
];

/// Whether a refusal is worth retrying unchanged: its own `retry` flag, else its code's class
/// ([`TRANSIENT`]). Every error answer carries the flag ([`tidy`] adds it), so callers (agents,
/// the editor's undo stack, scripts) decide from it, never from their own lists of codes.
pub fn retryable(err: &Json) -> bool {
    match err.get("retry").and_then(Json::as_bool) {
        Some(r) => r,
        None => err.get("error").and_then(Json::as_str).is_some_and(|c| TRANSIENT.contains(&c)),
    }
}

/// Every answer as compact JSON (pretty-printed answers cost an agent about a third more tokens
/// for nothing), and every error as `{error, message, retry}` (a plain-text error becomes one,
/// coded by [`PLAIN_ERRORS`], else `error`; `retry` from [`retryable`]). Text that isn't JSON
/// (play_map's map) passes through.
pub fn tidy(mut out: ToolOutput) -> ToolOutput {
    let is_error = out.is_error;
    for c in &mut out.content {
        let Content::Text(t) = c else { continue };
        match Json::parse(t) {
            Ok(v) if is_error && !matches!(v, Json::Obj(_)) => *t = plain_error(t),
            Ok(mut v) if is_error => {
                if v.get("retry").is_none() {
                    let retry = retryable(&v);
                    v.set("retry", Json::Bool(retry));
                }
                *t = v.to_string();
            }
            Ok(v) => {
                if t.contains('\n') {
                    *t = v.to_string();
                }
            }
            Err(_) if is_error => *t = plain_error(t),
            Err(_) => {}
        }
        break;
    }
    out
}

fn plain_error(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let code = PLAIN_ERRORS.iter().find(|(k, _)| lower.contains(k)).map_or("error", |(_, c)| c);
    let mut j = Json::obj([("error", Json::from(code)), ("message", Json::from(text))]);
    let retry = retryable(&j);
    j.set("retry", Json::Bool(retry));
    j.to_string()
}

/// Provenance keys: who did, said or measured something. Concise shaping never drops them
/// ([`keep`] keeps them wherever they appear), so an agent's claim never reads like an engine
/// result in a concise answer (`docs/design/AUTHORITY.md`, "Provenance by type").
pub const PROVENANCE: [&str; 10] = ["by", "source", "resolved_by", "engine", "submitted_by", "decided_by", "set_by", "flown_by", "reported_by", "authority"];

/// Keeps only `keys` of an object (in its own order), and its [`PROVENANCE`] keys; anything else
/// passes through.
fn keep(v: &Json, keys: &[&str]) -> Json {
    match v {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| keys.contains(&k.as_str()) || PROVENANCE.contains(&k.as_str())).cloned().collect()),
        other => other.clone(),
    }
}

/// Numbers rounded to `places` decimals, recursively.
fn round(v: &Json, places: i32) -> Json {
    let f = 10f64.powi(places);
    match v {
        Json::Num(n) if n.fract() != 0.0 => Json::Num((n * f).round() / f),
        Json::Arr(a) => Json::Arr(a.iter().map(|x| round(x, places)).collect()),
        Json::Obj(o) => Json::Obj(o.iter().map(|(k, x)| (k.clone(), round(x, places))).collect()),
        other => other.clone(),
    }
}

fn nonzero(v: Option<&Json>) -> bool {
    v.and_then(Json::as_f64).is_some_and(|n| n != 0.0)
}

fn non_empty(v: Option<&Json>) -> bool {
    match v {
        None | Some(Json::Null) => false,
        Some(Json::Arr(a)) => !a.is_empty(),
        Some(Json::Obj(o)) => !o.is_empty(),
        Some(_) => true,
    }
}

const SUBJECT_KEYS: [&str; 15] = [
    "id",
    "name",
    "path",
    "on_screen",
    "screen",
    "screen_box_exact",
    "visible_px",
    "visible_px_approx",
    "no_pixels",
    "occluded",
    "distance_m",
    "layer",
    "transient",
    "drawn_as",
    "resident_lods_from",
];

fn sight_frame(v: &Json) -> Json {
    let mut out = keep(v, &["image", "camera", "subjects_total", "subjects_on_screen", "subjects_omitted", "warnings", "problems", "overlays"]);
    if let Some(Json::Arr(subjects)) = v.get("subjects") {
        out.set("subjects", Json::Arr(subjects.iter().map(|s| round(&keep(s, &SUBJECT_KEYS), 2)).collect()));
    }
    // Models that weren't drawn (or were drawn merged in HLOD proxies, or aren't on the GPU)
    // are worth the bytes.
    let notable = |m: &Json| ["missing", "pipeline_failed", "not_resident", "hlod"].iter().any(|k| nonzero(m.get(k)));
    if let Some(m) = v.get("models").filter(|m| notable(m)) {
        out.set("models", m.clone());
    }
    if let Some(st) = v.get("streaming").filter(|st| ["not_resident", "hlod_entities", "coarser_lod"].iter().any(|k| nonzero(st.get(k)))) {
        out.set("streaming", st.clone());
    }
    if let Some(c) = out.get("camera").cloned() {
        out.set("camera", round(&c, 3));
    }
    out
}

/// One render path's totals.
fn path_totals(p: &Json) -> Json {
    if p.is_null() {
        return Json::Null;
    }
    let mut out = keep(p, &["frame", "size"]);
    if let Some(gpu) = p.get("gpu") {
        out.set("gpu_ms", gpu.get("total_ms").cloned().unwrap_or(Json::Null));
    }
    if let Some(cpu) = p.get("cpu") {
        out.set("cpu", cpu.clone());
    }
    if let Some(d) = p.get("draws") {
        out.set("draws", keep(d, &["draw_calls", "triangles", "culled"]));
    }
    if let Some(m) = p.get("memory") {
        out.set("memory_mb", m.get("total_mb").cloned().unwrap_or(Json::Null));
    }
    if let Some(pl) = p.get("pipelines").filter(|pl| nonzero(pl.get("created_after_warm")) || nonzero(pl.get("background_failed"))) {
        out.set("pipelines", pl.clone());
    }
    out
}

fn perf_frame(v: &Json) -> Json {
    let mut out = keep(v, &["budget_ms", "errors", "problems", "frame_pacing", "serve", "sim_tick", "game_frame", "cpu", "heap_mb", "draw_list", "entities", "transient_entities"]);
    for path in ["window", "offscreen"] {
        if let Some(p) = v.get(path) {
            out.set(path, path_totals(p));
        }
    }
    if let Some(p) = v.get("physics") {
        let mut ph = keep(p, &["bodies", "awake", "sleeping", "characters", "errors"]);
        if let Some(ms) = p.get("stages_ms").and_then(|s| s.get("total")) {
            ph.set("total_ms", ms.clone());
        }
        out.set("physics", ph);
    }
    if let Some(o) = v.get("game_overlay").filter(|o| nonzero(o.get("dropped"))) {
        out.set("game_overlay", o.clone());
    }
    if let Some(c) = v.get("clock") {
        out.set("clock", keep(c, &["tick", "paused", "scale"]));
    }
    out
}

fn engine_info(v: &Json) -> Json {
    let mut out = keep(v, &["engine", "gpu", "validation", "entities", "agents", "clock", "window", "game", "modules", "board_save_error", "board_warning"]);
    // The floating-point guard only when it had something to restore.
    let restored = |k: &str| v.get("determinism").and_then(|d| d.get(k)).and_then(|f| f.get("restored")).and_then(Json::as_u64).unwrap_or(0);
    if restored("fp_env") + restored("fp_env_modules") > 0 {
        if let Some(d) = v.get("determinism") {
            out.set("determinism", d.clone());
        }
    }
    if let Some(w) = v.get("world") {
        // last_autosave_error always (null when fine): agents and scripts check it.
        let mut world = keep(w, &["name", "entities", "unsaved_changes", "last_autosave_error", "state_hash"]);
        if non_empty(w.get("leftover_files")) {
            world.set("leftover_files", w.get("leftover_files").cloned().unwrap_or(Json::Null));
        }
        out.set("world", world);
    }
    if let Some(s) = v.get("swarm") {
        let mut swarm = keep(s, &["claims", "claim_rejections", "tasks", "notes", "intents", "leases"]);
        if let Some(j) = s.get("journal") {
            swarm.set("journal", keep(j, &["last_id", "evicted", "replayable"]));
        }
        out.set("swarm", swarm);
    }
    if let Some(m) = v.get("models") {
        let broken = m.get("library").and_then(|l| l.get("broken")).filter(|b| non_empty(Some(b)));
        if let Some(b) = broken {
            out.set("models_broken", b.clone());
        }
    }
    if let Some(c) = out.get("clock").cloned() {
        out.set("clock", keep(&c, &["tick", "tick_hz", "paused", "scale"]));
    }
    // The disk budget: its state and use (disk_status has the rest).
    if let Some(d) = v.get("disk") {
        out.set("disk", keep(d, &["state", "budget", "warning", "refused"]));
    }
    out
}

/// An entity without the values its defaults imply (see [`SHAPED`]).
fn lean_entity(e: &Json) -> Json {
    let Json::Obj(pairs) = e else { return e.clone() };
    let path_name = e.get("path").and_then(Json::as_str).and_then(|p| p.rsplit('/').next()).map(str::to_owned);
    let created = e.get("created_by").cloned();
    let zero = Json::arr([Json::from(0), Json::from(0), Json::from(0)]);
    let one = Json::arr([Json::from(1), Json::from(1), Json::from(1)]);
    let same = |a: &Json, b: &Json| a.to_string() == b.to_string();
    Json::Obj(
        pairs
            .iter()
            .filter(|(k, v)| match k.as_str() {
                "name" => v.as_str().map(str::to_owned) != path_name,
                "rotation_deg" => !same(v, &zero),
                "scale" => !same(v, &one),
                "modified_by" => created.as_ref() != Some(v),
                _ => true,
            })
            .cloned()
            .collect(),
    )
}

fn world_list(v: &Json) -> Json {
    let mut out = v.clone();
    if let Some(Json::Arr(items)) = v.get("entities") {
        out.set("entities", Json::Arr(items.iter().map(lean_entity).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(out: &ToolOutput) -> &str {
        match &out.content[0] {
            Content::Text(t) => t,
            Content::Image { .. } => "",
        }
    }

    #[test]
    fn formats_parse() {
        let a = Json::parse(r#"{"response_format": "detailed"}"#).unwrap();
        assert_eq!(Format::arg(&a, Format::Concise), Ok(Format::Detailed));
        assert_eq!(Format::arg(&Json::Null, Format::Concise), Ok(Format::Concise));
        assert!(Format::arg(&Json::parse(r#"{"response_format": "short"}"#).unwrap(), Format::Concise).is_err());
    }

    #[test]
    fn split_sets_the_tools_own_knobs() {
        let args = |t: &str| Json::parse(t).unwrap();
        assert_eq!(split("world_spawn", &args(r#"{"response_format": "concise"}"#), Format::Concise), Ok(None), "not shaped here");
        let (f, rest) = split("world_query", &args(r#"{"limit": 5, "response_format": "detailed"}"#), Format::Concise).unwrap().unwrap();
        assert_eq!((f, rest.to_string()), (Format::Detailed, r#"{"limit":5,"fields":["all"]}"#.to_owned()));
        let (_, rest) = split("world_query", &args(r#"{"fields": ["id"], "response_format": "detailed"}"#), Format::Concise).unwrap().unwrap();
        assert_eq!(rest.to_string(), r#"{"fields":["id"]}"#, "explicit fields win");
        let (_, rest) = split("world_history", &args(r#"{"response_format": "detailed"}"#), Format::Concise).unwrap().unwrap();
        assert_eq!(rest.to_string(), r#"{"detail":true}"#);
        let (f, rest) = split("perf_frame", &Json::Null, Format::Concise).unwrap().unwrap();
        assert_eq!((f, rest), (Format::Concise, Json::Null));
        assert!(split("perf_frame", &args(r#"{"response_format": 1}"#), Format::Concise).is_err());
        // A detailed default: shaped tools, and the tools' own knobs even without arguments.
        let (f, rest) = split("world_query", &Json::Null, Format::Detailed).unwrap().unwrap();
        assert_eq!((f, rest.to_string()), (Format::Detailed, r#"{"fields":["all"]}"#.to_owned()));
        let (f, _) = split("sight_frame", &args(r#"{"response_format": "concise"}"#), Format::Detailed).unwrap().unwrap();
        assert_eq!(f, Format::Concise, "the call's own choice wins");
    }

    #[test]
    fn defaults_per_engine_and_session() {
        let d = Defaults::new(Format::Concise);
        assert_eq!(d.get("s1"), Format::Concise);
        d.hello("s1", "token-a", Some(Format::Detailed));
        assert_eq!((d.get("s1"), d.get("s2")), (Format::Detailed, Format::Concise));
        // A later session resuming the same identity keeps its default.
        d.hello("s2", "token-a", None);
        assert_eq!(d.get("s2"), Format::Detailed);
        d.hello("s3", "token-b", None);
        assert_eq!(d.get("s3"), Format::Concise, "another identity");
        assert_eq!(Defaults::new(Format::Detailed).get("any"), Format::Detailed);
        // Past the cap the least recently used goes, not the smallest id.
        let d = Defaults::new(Format::Concise);
        d.hello("aaa", "t-aaa", Some(Format::Detailed));
        for i in 0..MAX_DEFAULTS {
            assert_eq!(d.get("aaa"), Format::Detailed, "kept while in use ({i})");
            d.hello(&format!("s{i:05}"), &format!("t{i:05}"), Some(Format::Detailed));
        }
        assert_eq!(d.get("aaa"), Format::Detailed);
        assert_eq!(with_default(&Json::Null, Format::Detailed).unwrap().to_string(), r#"{"response_format":"detailed"}"#);
        assert_eq!(with_default(&Json::parse(r#"{"response_format": "concise"}"#).unwrap(), Format::Detailed), None);
        assert_eq!(with_default(&Json::Null, Format::Concise), None);
        let mut defs = vec![
            ToolDef { name: "play_state".into(), description: String::new(), input_schema: Json::parse(r#"{"properties": {"response_format": {}}}"#).unwrap() },
            ToolDef { name: "perf_frame".into(), description: String::new(), input_schema: Json::parse(r#"{"properties": {}}"#).unwrap() },
        ];
        add_to_schemas(&mut defs);
        assert_eq!((takes_format(&defs[0]), takes_format(&defs[1])), (true, false), "shaped tools aren't passed through");
    }

    /// Streaming review finding 5: concise captures keep what streaming says about subjects
    /// (drawn merged in a proxy, not on the GPU) and the packet's streaming totals then.
    #[test]
    fn concise_captures_keep_streaming_facts() {
        let frame = r#"{"image": {"path": "a.png"}, "models": {"missing": 0, "pipeline_failed": 0, "not_resident": 1, "hlod": 3},
            "streaming": {"hlod_cells": 1, "hlod_entities": 3, "not_resident": 1, "coarser_lod": 0, "note": "n"},
            "subjects": [{"id": 1, "name": "p0", "drawn_as": "hlod", "visible_px": null}, {"id": 2, "name": "g", "drawn_as": "not_resident"},
                         {"id": 3, "name": "t", "resident_lods_from": 1, "occluded": false}]}"#;
        let j = sight_frame(&Json::parse(frame).unwrap());
        let subjects = j.get("subjects").and_then(Json::as_array).unwrap();
        assert_eq!(subjects[0].get("drawn_as").and_then(Json::as_str), Some("hlod"));
        assert_eq!(subjects[1].get("drawn_as").and_then(Json::as_str), Some("not_resident"));
        assert_eq!(subjects[2].get("resident_lods_from").and_then(Json::as_u64), Some(1));
        assert!(j.get("streaming").is_some() && j.get("models").is_some(), "{j}");
        // Nothing streaming-related: neither block.
        let plain = sight_frame(&Json::parse(r#"{"models": {"missing": 0, "hlod": 0, "not_resident": 0}, "streaming": {"not_resident": 0, "hlod_entities": 0}}"#).unwrap());
        assert!(plain.get("streaming").is_none() && plain.get("models").is_none(), "{plain}");
    }

    #[test]
    fn concise_answers_keep_what_agents_act_on() {
        let frame = r#"{"schema": "s", "frame": 3, "image": {"path": "a.png"}, "camera": {"eye": [1.23456, 0, 0]},
            "models": {"missing": 0}, "id_buffer": {"entities": 3}, "subjects_total": 1, "subjects_on_screen": 1,
            "subjects": [{"id": 1, "name": "a", "path": "/a", "shape": "cube", "color": "white", "position": [0, 0, 0],
            "screen": [1, 2, 3, 4], "visible_px": 10, "distance_m": 3.14159}], "warnings": [], "problems": []}"#;
        let out = shape("sight_frame", Format::Concise, ToolOutput::text(frame));
        assert_eq!(
            text(&out),
            r#"{"image":{"path":"a.png"},"camera":{"eye":[1.235,0,0]},"subjects_total":1,"subjects_on_screen":1,"warnings":[],"problems":[],"subjects":[{"id":1,"name":"a","path":"/a","screen":[1,2,3,4],"visible_px":10,"distance_m":3.14}]}"#
        );
        let ui = frame.replace(r#""visible_px": 10,"#, r#""visible_px": 10, "layer": "ui", "transient": true,"#);
        let ui = text(&shape("sight_frame", Format::Concise, ToolOutput::text(ui))).to_owned();
        assert!(ui.contains(r#""layer":"ui""#) && ui.contains(r#""transient":true"#), "{ui}");
        let info = r#"{"engine": "x", "world": {"name": null, "worlds_dir": "w", "unsaved_changes": false, "last_autosave_error": null, "leftover_files": []}}"#;
        let info = text(&shape("engine_info", Format::Concise, ToolOutput::text(info))).to_owned();
        assert_eq!(info, r#"{"engine":"x","world":{"name":null,"unsaved_changes":false,"last_autosave_error":null}}"#);
        let missing = frame.replace(r#""missing": 0"#, r#""missing": 2"#);
        assert!(text(&shape("sight_frame", Format::Concise, ToolOutput::text(missing))).contains(r#""models":{"missing":2}"#));
        assert_eq!(text(&shape("sight_frame", Format::Detailed, ToolOutput::text(frame))), frame, "detailed is untouched");
        let err = ToolOutput::error("{\"error\": \"x\"}");
        assert_eq!(text(&shape("sight_frame", Format::Concise, err)), "{\"error\": \"x\"}");
        let list = r#"{"total": 1, "entities": [{"id": 1, "name": "b", "path": "/a/b", "rotation_deg": [0, 0, 0], "scale": [1, 1, 2],
            "created_by": "x", "modified_by": "x", "version": 2}], "next_cursor": null}"#;
        assert_eq!(
            text(&shape("world_list", Format::Concise, ToolOutput::text(list))),
            r#"{"total":1,"entities":[{"id":1,"path":"/a/b","scale":[1,1,2],"created_by":"x","version":2}],"next_cursor":null}"#
        );
        let perf = r#"{"budget_ms": 16, "errors": [], "offscreen": {"frame": 1, "gpu": {"total_ms": 0.5, "passes": {"scene": 0.1}},
            "draws": {"draw_calls": 3, "instances": 9}, "memory": {"total_mb": 4, "heaps": []}, "pipelines": {"created_after_warm": 0}},
            "render_graph": {"passes": 18}, "physics": {"bodies": 2, "stages_ms": {"total": 0.25}, "settings": {}}}"#;
        assert_eq!(
            text(&shape("perf_frame", Format::Concise, ToolOutput::text(perf))),
            r#"{"budget_ms":16,"errors":[],"offscreen":{"frame":1,"gpu_ms":0.5,"draws":{"draw_calls":3},"memory_mb":4},"physics":{"bodies":2,"total_ms":0.25}}"#
        );
    }

    #[test]
    fn answers_are_compact_and_errors_structured() {
        assert_eq!(text(&tidy(ToolOutput::text("{\n  \"a\": [1,\n 2]\n}"))), r#"{"a":[1,2]}"#);
        assert_eq!(text(&tidy(ToolOutput::text("map\n# . #"))), "map\n# . #", "not JSON: untouched");
        let busy = tidy(ToolOutput::error("the engine was too busy to answer within 30 s; the command was not applied"));
        assert!(busy.is_error);
        assert_eq!(Json::parse(text(&busy)).unwrap().get("error").and_then(Json::as_str), Some("engine_busy"));
        assert_eq!(text(&tidy(ToolOutput::error("bad thing"))), r#"{"error":"error","message":"bad thing","retry":false}"#);
        assert_eq!(text(&tidy(ToolOutput::error("{\n \"error\": \"x\"\n}"))), r#"{"error":"x","retry":false}"#, "already structured: compacted, the flag added");
        assert_eq!(text(&tidy(ToolOutput::error("\"quoted\""))), r#"{"error":"error","message":"\"quoted\"","retry":false}"#);
        assert_eq!(Json::parse(text(&busy)).unwrap().get("retry").and_then(Json::as_bool), Some(true));
    }

    /// Every error says whether the same call may succeed later: transient refusals (a claim, a
    /// busy worker) yes, lasting ones (already undone, too old, not yours) no; an error's own flag
    /// wins.
    #[test]
    fn every_error_says_whether_to_retry() {
        let flag = |s: &str| Json::parse(text(&tidy(ToolOutput::error(s)))).unwrap().get("retry").and_then(Json::as_bool);
        for code in ["claimed", "job_busy", "engine_busy", "window_held"] {
            assert_eq!(flag(&format!(r#"{{"error":"{code}","message":"m"}}"#)), Some(true), "{code}");
        }
        for code in ["already_undone", "too_old", "not_found", "not_undoable", "undo_conflict", "not_yours", "stale_version", "owner_only", "input_dropped"] {
            assert_eq!(flag(&format!(r#"{{"error":"{code}","message":"m"}}"#)), Some(false), "{code}");
        }
        assert_eq!(flag(r#"{"error":"not_found","retry":true}"#), Some(true), "the error's own flag wins");
        assert!(retryable(&Json::parse(r#"{"error":"claimed"}"#).unwrap()));
        assert!(!retryable(&Json::parse(r#"{"error":"claimed","retry":false}"#).unwrap()));
    }

    #[test]
    fn schemas_gain_the_property() {
        let mut defs = vec![ToolDef { name: "perf_frame".into(), description: String::new(), input_schema: Json::parse(r#"{"type": "object", "properties": {}}"#).unwrap() }];
        add_to_schemas(&mut defs);
        assert!(defs[0].input_schema.get("properties").unwrap().get(KEY).is_some());
    }
}
