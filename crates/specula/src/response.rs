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
//! `play_state`), as do `playtest_*`. See `docs/design/TOOLS.md` for the measured savings.

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
        "concise (default): subjects with id, name, path, screen boxes, visible_px, occlusion and distance only (their look \
         and transform are in world_query), no id_buffer/models/lights internals; the full packet is always saved at \
         image.packet_path. detailed: the whole packet",
    ),
    (
        "perf_frame",
        "concise (default): errors, problems, totals per path (GPU ms, CPU ms, draws, memory), pacing, sim and physics \
         totals; detailed: every pass, pipeline, heap, model and budget table",
    ),
    ("engine_info", "concise (default): engine, GPU, world summary, agents, clock, game status, swarm counts; detailed: model library, paths, journal internals"),
    (
        "world_list",
        "concise (default): whole entities with defaults left out (name = the last path segment, rotation_deg [0,0,0], \
         scale [1,1,1], modified_by = created_by); detailed: every field",
    ),
    ("world_query", "concise (default): the fields asked for (default id, path, shape, position, tags, version); detailed: every field (fields: [\"all\"])"),
    ("world_history", "concise (default): one line per entry; detailed: each entry's changes (detail: true)"),
];

/// Adds `response_format` to the schemas of the [`SHAPED`] tools.
pub fn add_to_schemas(defs: &mut [ToolDef]) {
    for d in defs.iter_mut() {
        let Some((_, what)) = SHAPED.iter().find(|(n, _)| *n == d.name) else { continue };
        let mut props = d.input_schema.get("properties").cloned().unwrap_or_else(|| Json::obj::<&str>([]));
        props.set(KEY, Format::schema_saying(what));
        d.input_schema.set("properties", props);
    }
}

/// Takes `response_format` out of a [`SHAPED`] tool's arguments (setting the tool's own knob for
/// a detailed answer). `None`: the tool isn't shaped here (its arguments are left alone).
pub fn split(tool: &str, args: &Json) -> Result<Option<(Format, Json)>, String> {
    if !SHAPED.iter().any(|(n, _)| *n == tool) {
        return Ok(None);
    }
    let format = Format::arg(args, Format::Concise)?;
    let mut rest = match args {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| k != KEY).cloned().collect()),
        other => other.clone(),
    };
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

/// Keeps only `keys` of an object (in its own order); anything else passes through.
fn keep(v: &Json, keys: &[&str]) -> Json {
    match v {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| keys.contains(&k.as_str())).cloned().collect()),
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

const SUBJECT_KEYS: [&str; 12] = [
    "id", "name", "path", "on_screen", "screen", "screen_box_exact", "visible_px", "visible_px_approx", "no_pixels", "occluded", "distance_m", "ui",
];

fn sight_frame(v: &Json) -> Json {
    let mut out = keep(v, &["image", "camera", "subjects_total", "subjects_on_screen", "subjects_omitted", "warnings", "problems", "overlays"]);
    if let Some(Json::Arr(subjects)) = v.get("subjects") {
        out.set("subjects", Json::Arr(subjects.iter().map(|s| round(&keep(s, &SUBJECT_KEYS), 2)).collect()));
    }
    // Models that weren't drawn are worth the bytes.
    if let Some(m) = v.get("models").filter(|m| nonzero(m.get("missing")) || nonzero(m.get("pipeline_failed"))) {
        out.set("models", m.clone());
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
    if let Some(w) = v.get("world") {
        let mut world = keep(w, &["name", "entities", "unsaved_changes", "state_hash"]);
        for k in ["last_autosave_error", "leftover_files"] {
            if non_empty(w.get(k)) {
                world.set(k, w.get(k).cloned().unwrap_or(Json::Null));
            }
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
        assert_eq!(split("world_spawn", &args(r#"{"response_format": "concise"}"#)), Ok(None), "not shaped here");
        let (f, rest) = split("world_query", &args(r#"{"limit": 5, "response_format": "detailed"}"#)).unwrap().unwrap();
        assert_eq!((f, rest.to_string()), (Format::Detailed, r#"{"limit":5,"fields":["all"]}"#.to_owned()));
        let (_, rest) = split("world_query", &args(r#"{"fields": ["id"], "response_format": "detailed"}"#)).unwrap().unwrap();
        assert_eq!(rest.to_string(), r#"{"fields":["id"]}"#, "explicit fields win");
        let (_, rest) = split("world_history", &args(r#"{"response_format": "detailed"}"#)).unwrap().unwrap();
        assert_eq!(rest.to_string(), r#"{"detail":true}"#);
        let (f, rest) = split("perf_frame", &Json::Null).unwrap().unwrap();
        assert_eq!((f, rest), (Format::Concise, Json::Null));
        assert!(split("perf_frame", &args(r#"{"response_format": 1}"#)).is_err());
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
    fn schemas_gain_the_property() {
        let mut defs = vec![ToolDef { name: "perf_frame".into(), description: String::new(), input_schema: Json::parse(r#"{"type": "object", "properties": {}}"#).unwrap() }];
        add_to_schemas(&mut defs);
        assert!(defs[0].input_schema.get("properties").unwrap().get(KEY).is_some());
    }
}
